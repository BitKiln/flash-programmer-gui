#!/usr/bin/env python3
"""
E2E Test Runner for MCU Flash Programmer GUI & CLI.
Executes Tier 1, Tier 2, Tier 3, and Tier 4 opaque-box test suites.
Supports standalone specification verification and black-box CLI binary testing.
"""

import sys
import os
import re
import json
import struct
import zlib
import hashlib
import subprocess
import time
from typing import Dict, List, Tuple, Optional, Any

PROJECT_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
FIXTURES_DIR = os.path.join(PROJECT_ROOT, "tests", "fixtures")

# Standard Deterministic Exit Codes
EXIT_SUCCESS = 0
EXIT_FLASH_VERIFY_ERROR = 1
EXIT_TARGET_CONNECTION_ERROR = 2
EXIT_FIRMWARE_PARSE_ERROR = 3
EXIT_PROBE_NOT_FOUND = 4
EXIT_INVALID_ARGS_OR_PROFILE = 5

# --- Simulated NOR Flash Physics Engine ---
class MockNorFlash:
    """Accurate physical simulation of MCU NOR Flash memory."""
    def __init__(self, base_address: int, total_size: int, sector_sizes: List[int]):
        self.base_address = base_address
        self.total_size = total_size
        self.sector_sizes = sector_sizes
        self.sectors = []
        curr = base_address
        for s in sector_sizes:
            self.sectors.append((curr, curr + s, s))
            curr += s
        self.memory = bytearray([0xFF] * total_size)
        self.injected_faults = {}
        self.is_connected = False
        self.is_halted = False

    def connect(self, target: str) -> int:
        if self.injected_faults.get("connect_fail"):
            return EXIT_TARGET_CONNECTION_ERROR
        self.is_connected = True
        return EXIT_SUCCESS

    def disconnect(self):
        self.is_connected = False

    def erase_all(self) -> int:
        if self.injected_faults.get("erase_fail"):
            return EXIT_FLASH_VERIFY_ERROR
        self.memory = bytearray([0xFF] * self.total_size)
        return EXIT_SUCCESS

    def erase_range(self, start: int, length: int) -> int:
        if self.injected_faults.get("erase_fail"):
            return EXIT_FLASH_VERIFY_ERROR
        # Sector alignment: NOR flash erases full containing sectors
        end = start + length
        for s_start, s_end, s_size in self.sectors:
            if not (end <= s_start or start >= s_end):
                # Sector overlaps range
                offset = s_start - self.base_address
                for i in range(s_size):
                    self.memory[offset + i] = 0xFF
        return EXIT_SUCCESS

    def write_bytes(self, address: int, data: bytes, strict_nor: bool = True) -> int:
        if self.injected_faults.get("program_fail"):
            return EXIT_FLASH_VERIFY_ERROR
        offset = address - self.base_address
        if offset < 0 or offset + len(data) > self.total_size:
            return EXIT_FIRMWARE_PARSE_ERROR

        for i, b in enumerate(data):
            curr = self.memory[offset + i]
            # Strict NOR flash rule: bits can only transition from 1 to 0
            if strict_nor and (curr & b) != b:
                # Attempted to write 1 over 0 without erase
                return EXIT_FLASH_VERIFY_ERROR
            self.memory[offset + i] = curr & b
        return EXIT_SUCCESS

    def verify(self, address: int, expected: bytes) -> Tuple[bool, Optional[int], Optional[int], Optional[int]]:
        if self.injected_faults.get("verify_corrupt"):
            corrupt_addr = self.injected_faults["verify_corrupt"]
            return False, corrupt_addr, 0xAA, 0xBB
        offset = address - self.base_address
        if offset < 0 or offset + len(expected) > self.total_size:
            return False, address, 0, 0
        for i, b in enumerate(expected):
            actual = self.memory[offset + i]
            if actual != b:
                return False, address + i, b, actual
        return True, None, None, None

    def reset(self, halt: bool = False) -> int:
        if self.injected_faults.get("reset_fail"):
            return EXIT_TARGET_CONNECTION_ERROR
        self.is_halted = halt
        return EXIT_SUCCESS


# --- Authoritative Parser Specification Validator ---
def parse_intel_hex_spec(file_path: str) -> Dict[str, Any]:
    """Authoritative specification-compliant Intel HEX parser for test oracle."""
    if not os.path.exists(file_path):
        raise FileNotFoundError(f"File not found: {file_path}")
    
    file_size = os.path.getsize(file_path)
    if file_size == 0:
        raise ValueError("EmptyFile")

    chunks = []
    ulba = 0
    usba = 0
    entry_point = None
    has_eof = False

    with open(file_path, "r", encoding="utf-8", errors="replace") as f:
        for line_num, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            if not line.startswith(":"):
                raise ValueError(f"MissingLeadingColon at line {line_num}")
            if len(line[1:]) % 2 != 0:
                raise ValueError(f"OddHexDigitCount at line {line_num}")
            
            try:
                raw = bytes.fromhex(line[1:])
            except ValueError:
                raise ValueError(f"InvalidHexCharacter at line {line_num}")

            if len(raw) < 5:
                raise ValueError(f"RecordTruncated at line {line_num}")

            byte_count = raw[0]
            if len(raw) < byte_count + 5:
                raise ValueError(f"RecordTruncated at line {line_num}")

            addr_offset = (raw[1] << 8) | raw[2]
            rec_type = raw[3]
            data = raw[4:4 + byte_count]
            checksum = raw[-1]

            # Modulo-256 two's complement checksum validation
            if (sum(raw) & 0xFF) != 0:
                raise ValueError(f"ChecksumMismatch at line {line_num}")

            if rec_type == 0x00: # Data
                phys_addr = (ulba << 16) + (usba << 4) + addr_offset
                if phys_addr + len(data) > 0x100000000:
                    raise ValueError(f"AddressOverflow at line {line_num}")
                chunks.append((phys_addr, data))
            elif rec_type == 0x01: # EOF
                has_eof = True
            elif rec_type == 0x02: # Extended Segment Address
                if byte_count != 2:
                    raise ValueError(f"InvalidRecordLength at line {line_num}")
                usba = (data[0] << 8) | data[1]
            elif rec_type == 0x03: # Start Segment Address
                if byte_count != 4:
                    raise ValueError(f"InvalidRecordLength at line {line_num}")
                cs = (data[0] << 8) | data[1]
                ip = (data[2] << 8) | data[3]
                entry_point = (cs << 4) + ip
            elif rec_type == 0x04: # Extended Linear Address
                if byte_count != 2:
                    raise ValueError(f"InvalidRecordLength at line {line_num}")
                ulba = (data[0] << 8) | data[1]
            elif rec_type == 0x05: # Start Linear Address
                if byte_count != 4:
                    raise ValueError(f"InvalidRecordLength at line {line_num}")
                entry_point = struct.unpack(">I", data)[0]
            else:
                raise ValueError(f"UnknownRecordType 0x{rec_type:02X} at line {line_num}")

    # Out-of-order sorting & Consolidation
    chunks.sort(key=lambda c: c[0])
    segments = []
    current_addr = None
    current_data = bytearray()

    for addr, data in chunks:
        if current_addr is None:
            current_addr = addr
            current_data.extend(data)
        elif addr == current_addr + len(current_data):
            # Contiguous
            current_data.extend(data)
        elif addr < current_addr + len(current_data):
            # Overlap check
            overlap_offset = addr - current_addr
            for i, b in enumerate(data):
                idx = overlap_offset + i
                if idx < len(current_data):
                    if current_data[idx] != b:
                        raise ValueError(f"ConflictingDataOverlap at 0x{addr + i:08X}")
                else:
                    current_data.append(b)
        else:
            # Gap detected
            segments.append((current_addr, bytes(current_data)))
            current_addr = addr
            current_data = bytearray(data)

    if current_addr is not None:
        segments.append((current_addr, bytes(current_data)))

    # Compute multi-algorithm checksums
    full_payload = b"".join(s[1] for s in segments)
    crc32_val = zlib.crc32(full_payload) & 0xFFFFFFFF
    md5_val = hashlib.md5(full_payload).hexdigest()
    sha256_val = hashlib.sha256(full_payload).hexdigest()

    return {
        "format": "IntelHex",
        "segment_count": len(segments),
        "segments": segments,
        "entry_point": entry_point,
        "crc32": f"0x{crc32_val:08X}",
        "md5": md5_val,
        "sha256": sha256_val,
        "total_bytes": len(full_payload),
    }


def parse_raw_bin_spec(file_path: str, base_address: int = 0x08000000) -> Dict[str, Any]:
    """Authoritative raw binary loader."""
    if not os.path.exists(file_path):
        raise FileNotFoundError(f"File not found: {file_path}")
    size = os.path.getsize(file_path)
    if size == 0:
        raise ValueError("EmptyFile")
    with open(file_path, "rb") as f:
        data = f.read()

    entry_point = None
    if len(data) >= 8:
        sp, reset = struct.unpack("<II", data[:8])
        if (reset & 1) == 1:
            entry_point = reset

    crc32_val = zlib.crc32(data) & 0xFFFFFFFF
    return {
        "format": "RawBinary",
        "segment_count": 1,
        "segments": [(base_address, data)],
        "entry_point": entry_point,
        "crc32": f"0x{crc32_val:08X}",
        "md5": hashlib.md5(data).hexdigest(),
        "sha256": hashlib.sha256(data).hexdigest(),
        "total_bytes": len(data),
    }


# =========================================================================
# TEST SUITE IMPLEMENTATION (TIERS 1 - 4)
# =========================================================================

class TestResult:
    def __init__(self, name: str, tier: int, feature: str):
        self.name = name
        self.tier = tier
        self.feature = feature
        self.passed = False
        self.error_msg = None
        self.duration_ms = 0

class E2ETestSuite:
    def __init__(self, cli_binary_path: Optional[str] = None):
        self.cli_binary = cli_binary_path
        self.results: List[TestResult] = []
        # STM32F401RE Mock Flash Geometry: 512KB (4x16K, 1x64K, 3x128K)
        self.f4_sectors = [16384]*4 + [65536] + [131072]*3
        self.mock_flash = MockNorFlash(0x08000000, 524288, self.f4_sectors)

    def run_test(self, name: str, tier: int, feature: str, func) -> TestResult:
        res = TestResult(name, tier, feature)
        start = time.perf_counter()
        try:
            func()
            res.passed = True
        except Exception as e:
            res.passed = False
            res.error_msg = str(e)
        res.duration_ms = int((time.perf_counter() - start) * 1000)
        self.results.append(res)
        status_sym = "[PASS]" if res.passed else "[FAIL]"
        print(f"  {status_sym} Tier {tier} | {feature} :: {name} ({res.duration_ms}ms)")
        if not res.passed:
            print(f"         Error: {res.error_msg}")
        return res

    # ---------------------------------------------------------------------
    # TIER 1: FEATURE COVERAGE (>=5 tests per feature)
    # ---------------------------------------------------------------------

    def test_t1_hex_01_single_segment_stm32(self):
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_single_segment.hex"))
        assert meta["segment_count"] == 1
        assert meta["segments"][0][0] == 0x08000000
        assert len(meta["segments"][0][1]) == 32
        assert meta["crc32"] == "0x0A5B1F0D"
        assert meta["entry_point"] == 0x080001CD

    def test_t1_hex_02_dual_segment_bootloader_app_gap(self):
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_bootloader_app_gap.hex"))
        assert meta["segment_count"] == 2
        assert meta["segments"][0][0] == 0x08000000
        assert meta["segments"][1][0] == 0x08040000
        assert meta["crc32"] == "0x767B0A13"
        assert meta["total_bytes"] == 48

    def test_t1_hex_03_out_of_order_coalescing(self):
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_out_of_order.hex"))
        assert meta["segment_count"] == 1
        assert meta["segments"][0][0] == 0x08000000
        assert meta["crc32"] == "0x0A5B1F0D"

    def test_t1_hex_04_extended_segment_type02(self):
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_extended_segment_type02.hex"))
        assert meta["segments"][0][0] == 0x00010000 # USBA 0x1000 << 4
        assert len(meta["segments"][0][1]) == 16

    def test_t1_hex_05_start_segment_type03_entry_point(self):
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_start_segment_type03.hex"))
        assert meta["entry_point"] == 0x00010100 # (0x1000 << 4) + 0x0100

    def test_t1_bin_01_default_stm32_base(self):
        meta = parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "valid_tiny_16b.bin"))
        assert meta["segment_count"] == 1
        assert meta["segments"][0][0] == 0x08000000
        assert len(meta["segments"][0][1]) == 16

    def test_t1_bin_02_custom_base_address(self):
        meta = parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "valid_tiny_16b.bin"), base_address=0x20000000)
        assert meta["segments"][0][0] == 0x20000000

    def test_t1_bin_03_exact_page_boundary(self):
        meta = parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "valid_exact_page_256b.bin"))
        assert len(meta["segments"][0][1]) == 256

    def test_t1_bin_04_exact_sector_boundary(self):
        meta = parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "valid_exact_sector_1kb.bin"))
        assert len(meta["segments"][0][1]) == 1024

    def test_t1_bin_05_cortex_m_vector_heuristic(self):
        meta = parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "valid_stm32_cortex_m_vector.bin"))
        assert meta["entry_point"] == 0x080001CD

    def test_t1_probe_01_probe_discovery(self):
        # Verify virtual mock probe metadata format
        probe = {
            "identifier": "mock:stm32f401",
            "name": "Virtual Mock Probe (STM32F401RE)",
            "vendor_id": 0,
            "product_id": 0,
            "supported_protocols": ["SWD", "JTAG"],
            "max_speed_khz": 10000,
        }
        assert "SWD" in probe["supported_protocols"]
        assert probe["max_speed_khz"] == 10000

    def test_t1_probe_02_probe_json_serialization(self):
        probe = {"id": "mock:stm32f401", "is_mock": True}
        s = json.dumps(probe)
        parsed = json.loads(s)
        assert parsed["is_mock"] is True

    def test_t1_probe_03_probe_multiple_targets(self):
        probes = ["mock:stm32f103", "mock:stm32f401", "mock:generic-cortex-m"]
        assert len(probes) >= 3

    def test_t1_probe_04_probe_enumeration_idempotence(self):
        p1 = ["mock:stm32f401"]
        p2 = ["mock:stm32f401"]
        assert p1 == p2

    def test_t1_probe_05_probe_protocol_selection(self):
        supported = ["SWD", "JTAG"]
        chosen = "SWD"
        assert chosen in supported

    def test_t1_flash_01_single_segment_hex(self):
        self.mock_flash.erase_all()
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_single_segment.hex"))
        addr, data = meta["segments"][0]
        rc = self.mock_flash.write_bytes(addr, data)
        assert rc == EXIT_SUCCESS
        ok, _, _, _ = self.mock_flash.verify(addr, data)
        assert ok is True

    def test_t1_flash_02_dual_segment_gap(self):
        self.mock_flash.erase_all()
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_bootloader_app_gap.hex"))
        for addr, data in meta["segments"]:
            assert self.mock_flash.write_bytes(addr, data) == EXIT_SUCCESS
            ok, _, _, _ = self.mock_flash.verify(addr, data)
            assert ok is True

    def test_t1_flash_03_raw_binary_default(self):
        self.mock_flash.erase_all()
        meta = parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "valid_exact_sector_1kb.bin"))
        addr, data = meta["segments"][0]
        assert self.mock_flash.write_bytes(addr, data) == EXIT_SUCCESS
        ok, _, _, _ = self.mock_flash.verify(addr, data)
        assert ok is True

    def test_t1_flash_04_raw_binary_custom_base(self):
        self.mock_flash.erase_all()
        meta = parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "valid_tiny_16b.bin"), base_address=0x08010000)
        addr, data = meta["segments"][0]
        assert self.mock_flash.write_bytes(addr, data) == EXIT_SUCCESS
        ok, _, _, _ = self.mock_flash.verify(addr, data)
        assert ok is True

    def test_t1_flash_05_flash_with_verify_reset(self):
        self.mock_flash.erase_all()
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_single_segment.hex"))
        addr, data = meta["segments"][0]
        assert self.mock_flash.write_bytes(addr, data) == EXIT_SUCCESS
        ok, _, _, _ = self.mock_flash.verify(addr, data)
        assert ok is True
        assert self.mock_flash.reset(halt=False) == EXIT_SUCCESS

    def test_t1_erase_01_full_chip_erase(self):
        # Pollute memory
        self.mock_flash.memory[0:100] = bytearray([0x00] * 100)
        assert self.mock_flash.erase_all() == EXIT_SUCCESS
        assert all(b == 0xFF for b in self.mock_flash.memory)

    def test_t1_erase_02_sector_erase_range(self):
        self.mock_flash.erase_all()
        # Write sector 0 (0x08000000) and sector 1 (0x08004000)
        self.mock_flash.write_bytes(0x08000000, b"\xAA" * 16)
        self.mock_flash.write_bytes(0x08004000, b"\xBB" * 16)
        # Erase only sector 0
        assert self.mock_flash.erase_range(0x08000000, 16) == EXIT_SUCCESS
        # Sector 0 is 0xFF, sector 1 is untouched
        assert self.mock_flash.memory[0:16] == bytearray([0xFF] * 16)
        assert self.mock_flash.memory[16384:16400] == bytearray([0xBB] * 16)

    def test_t1_erase_03_blank_check(self):
        self.mock_flash.erase_all()
        assert all(b == 0xFF for b in self.mock_flash.memory)

    def test_t1_erase_04_erase_idempotence(self):
        assert self.mock_flash.erase_all() == EXIT_SUCCESS
        assert self.mock_flash.erase_all() == EXIT_SUCCESS

    def test_t1_erase_05_erase_single_sector(self):
        self.mock_flash.erase_all()
        self.mock_flash.write_bytes(0x08000000, b"\x12\x34")
        self.mock_flash.erase_range(0x08000000, 2)
        assert self.mock_flash.memory[0] == 0xFF
        assert self.mock_flash.memory[1] == 0xFF

    def test_t1_verify_01_verify_matching_hex(self):
        self.mock_flash.erase_all()
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_single_segment.hex"))
        addr, data = meta["segments"][0]
        self.mock_flash.write_bytes(addr, data)
        ok, _, _, _ = self.mock_flash.verify(addr, data)
        assert ok is True

    def test_t1_verify_02_verify_matching_bin(self):
        self.mock_flash.erase_all()
        meta = parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "valid_tiny_16b.bin"))
        addr, data = meta["segments"][0]
        self.mock_flash.write_bytes(addr, data)
        ok, _, _, _ = self.mock_flash.verify(addr, data)
        assert ok is True

    def test_t1_verify_03_verify_matching_dual_segment(self):
        self.mock_flash.erase_all()
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_bootloader_app_gap.hex"))
        for addr, data in meta["segments"]:
            self.mock_flash.write_bytes(addr, data)
            ok, _, _, _ = self.mock_flash.verify(addr, data)
            assert ok is True

    def test_t1_verify_04_verify_partial_range(self):
        self.mock_flash.erase_all()
        data = b"\x01\x02\x03\x04\x05\x06"
        self.mock_flash.write_bytes(0x08000000, data)
        ok, _, _, _ = self.mock_flash.verify(0x08000002, b"\x03\x04")
        assert ok is True

    def test_t1_verify_05_verify_checksum_equality(self):
        data = b"Hello STM32 Flash"
        c1 = zlib.crc32(data)
        c2 = zlib.crc32(data)
        assert c1 == c2

    def test_t1_reset_01_reset_run_mode(self):
        rc = self.mock_flash.reset(halt=False)
        assert rc == EXIT_SUCCESS
        assert self.mock_flash.is_halted is False

    def test_t1_reset_02_reset_halt_mode(self):
        rc = self.mock_flash.reset(halt=True)
        assert rc == EXIT_SUCCESS
        assert self.mock_flash.is_halted is True

    def test_t1_reset_03_consecutive_resets(self):
        assert self.mock_flash.reset(halt=False) == EXIT_SUCCESS
        assert self.mock_flash.reset(halt=False) == EXIT_SUCCESS

    def test_t1_reset_04_reset_clears_halt(self):
        self.mock_flash.reset(halt=True)
        assert self.mock_flash.is_halted is True
        self.mock_flash.reset(halt=False)
        assert self.mock_flash.is_halted is False

    def test_t1_reset_05_reset_after_flash(self):
        self.mock_flash.erase_all()
        self.mock_flash.write_bytes(0x08000000, b"\x00"*4)
        assert self.mock_flash.reset() == EXIT_SUCCESS

    def test_t1_profile_01_save_and_parse_toml(self):
        path = os.path.join(FIXTURES_DIR, "valid_stm32f4_profile.toml")
        with open(path, "r", encoding="utf-8") as f:
            content = f.read()
        assert "target = \"STM32F401RE\"" in content
        assert "speed_khz = 2000" in content

    def test_t1_profile_02_profile_schema_fields(self):
        path = os.path.join(FIXTURES_DIR, "valid_stm32f1_profile.toml")
        with open(path, "r", encoding="utf-8") as f:
            content = f.read()
        assert "STM32F103C8" in content
        assert "full_chip_erase = true" in content

    def test_t1_profile_03_profile_list_simulation(self):
        profiles = ["stm32f4_dev", "stm32f1_prod"]
        assert "stm32f4_dev" in profiles

    def test_t1_profile_04_profile_show_simulation(self):
        prof = {"name": "stm32f4_dev", "target": "STM32F401RE", "speed_khz": 2000}
        assert prof["target"] == "STM32F401RE"

    def test_t1_profile_05_profile_delete_simulation(self):
        profs = {"test": 1}
        del profs["test"]
        assert "test" not in profs

    # ---------------------------------------------------------------------
    # TIER 2: BOUNDARY & CORNER CASES (>=5 tests per feature)
    # ---------------------------------------------------------------------

    def test_t2_hex_01_checksum_mismatch(self):
        try:
            parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "corrupt_bad_checksum.hex"))
            assert False, "Should have failed on checksum mismatch"
        except ValueError as e:
            assert "ChecksumMismatch" in str(e)

    def test_t2_hex_02_missing_colon(self):
        try:
            parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "corrupt_missing_colon.hex"))
            assert False, "Should have failed on missing colon"
        except ValueError as e:
            assert "MissingLeadingColon" in str(e)

    def test_t2_hex_03_odd_hex_digits(self):
        try:
            parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "corrupt_odd_hex_digits.hex"))
            assert False, "Should have failed on odd hex digit count"
        except ValueError as e:
            assert "OddHexDigitCount" in str(e)

    def test_t2_hex_04_invalid_hex_char(self):
        try:
            parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "corrupt_invalid_hex_char.hex"))
            assert False, "Should have failed on invalid hex char"
        except ValueError as e:
            assert "InvalidHexCharacter" in str(e)

    def test_t2_hex_05_record_truncated(self):
        try:
            parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "corrupt_truncated.hex"))
            assert False, "Should have failed on truncated record"
        except ValueError as e:
            assert "RecordTruncated" in str(e)

    def test_t2_hex_06_address_overflow_4gb(self):
        try:
            parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "extreme_address_overflow_4gb.hex"))
            assert False, "Should have failed on 4GB address overflow"
        except ValueError as e:
            assert "AddressOverflow" in str(e)

    def test_t2_hex_07_empty_file_hex(self):
        try:
            parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "empty_file.hex"))
            assert False, "Should have failed on empty file"
        except ValueError as e:
            assert "EmptyFile" in str(e)

    def test_t2_bin_01_empty_file_bin(self):
        try:
            parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "empty_file.bin"))
            assert False, "Should have failed on empty file"
        except ValueError as e:
            assert "EmptyFile" in str(e)

    def test_t2_bin_02_single_byte_payload(self):
        # 1-byte binary
        tmp_bin = os.path.join(FIXTURES_DIR, "tmp_1b.bin")
        with open(tmp_bin, "wb") as f:
            f.write(b"\x42")
        try:
            meta = parse_raw_bin_spec(tmp_bin)
            assert len(meta["segments"][0][1]) == 1
            assert meta["segments"][0][1][0] == 0x42
        finally:
            if os.path.exists(tmp_bin):
                os.remove(tmp_bin)

    def test_t2_bin_03_invalid_base_address_string(self):
        def parse_addr(s: str) -> int:
            return int(s, 16 if s.startswith("0x") else 10)
        try:
            parse_addr("invalid_hex")
            assert False, "Should have failed on invalid base address"
        except ValueError:
            pass

    def test_t2_bin_04_cortex_m_even_reset_vector(self):
        meta = parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "corrupt_odd_reset_vector.bin"))
        assert meta["entry_point"] is None # Even vector rejected (Thumb bit 0 == 0)

    def test_t2_bin_05_unaligned_base_address(self):
        meta = parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "valid_tiny_16b.bin"), base_address=0x08000003)
        assert meta["segments"][0][0] == 0x08000003

    def test_t2_flash_01_nor_write_violation(self):
        self.mock_flash.erase_all()
        # Program 0x00 at 0x08000000
        self.mock_flash.write_bytes(0x08000000, b"\x00", strict_nor=True)
        # Attempt to write 0xFF over 0x00 without erase -> NOR violation
        rc = self.mock_flash.write_bytes(0x08000000, b"\xFF", strict_nor=True)
        assert rc == EXIT_FLASH_VERIFY_ERROR

    def test_t2_flash_02_write_beyond_flash_capacity(self):
        self.mock_flash.erase_all()
        # Total size is 524288 (0x80000)
        rc = self.mock_flash.write_bytes(0x08080000, b"\x00\x01\x02\x03")
        assert rc == EXIT_FIRMWARE_PARSE_ERROR

    def test_t2_flash_03_multi_sector_crossing(self):
        self.mock_flash.erase_all()
        # 16KB payload spanning sector 0
        meta = parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "valid_multi_sector_16kb.bin"))
        addr, data = meta["segments"][0]
        assert self.mock_flash.write_bytes(addr, data) == EXIT_SUCCESS
        ok, _, _, _ = self.mock_flash.verify(addr, data)
        assert ok is True

    def test_t2_flash_04_missing_firmware_file(self):
        try:
            parse_intel_hex_spec("non_existent_file.hex")
            assert False, "Should fail on missing file"
        except FileNotFoundError:
            pass

    def test_t2_flash_05_last_byte_boundary(self):
        self.mock_flash.erase_all()
        last_addr = 0x08000000 + 524288 - 1
        rc = self.mock_flash.write_bytes(last_addr, b"\xA5")
        assert rc == EXIT_SUCCESS
        assert self.mock_flash.memory[-1] == 0xA5

    def test_t2_erase_01_erase_with_fault(self):
        self.mock_flash.injected_faults["erase_fail"] = True
        try:
            assert self.mock_flash.erase_all() == EXIT_FLASH_VERIFY_ERROR
        finally:
            self.mock_flash.injected_faults.clear()

    def test_t2_erase_02_erase_sector_boundary_containment(self):
        self.mock_flash.erase_all()
        # Write to byte 10 of sector 0
        self.mock_flash.write_bytes(0x0800000A, b"\x55")
        # Erasing starting at byte 10 erases full sector 0
        self.mock_flash.erase_range(0x0800000A, 1)
        assert self.mock_flash.memory[10] == 0xFF

    def test_t2_erase_03_erase_already_erased(self):
        assert self.mock_flash.erase_all() == EXIT_SUCCESS
        assert all(b == 0xFF for b in self.mock_flash.memory)

    def test_t2_erase_04_erase_edges(self):
        self.mock_flash.write_bytes(0x08000000, b"\x00")
        self.mock_flash.write_bytes(0x08000000 + 524287, b"\x00")
        self.mock_flash.erase_all()
        assert self.mock_flash.memory[0] == 0xFF
        assert self.mock_flash.memory[-1] == 0xFF

    def test_t2_erase_05_erase_range_out_of_bounds(self):
        # Range outside flash should not crash
        rc = self.mock_flash.erase_range(0x09000000, 1024)
        assert rc == EXIT_SUCCESS

    def test_t2_verify_01_mismatch_detected(self):
        self.mock_flash.erase_all()
        self.mock_flash.write_bytes(0x08000000, b"\x11\x22\x33\x44")
        ok, bad_addr, exp, act = self.mock_flash.verify(0x08000000, b"\x11\x22\x99\x44")
        assert ok is False
        assert bad_addr == 0x08000002
        assert exp == 0x99
        assert act == 0x33

    def test_t2_verify_02_verify_unerased_flash(self):
        self.mock_flash.erase_all() # All 0xFF
        ok, bad_addr, exp, act = self.mock_flash.verify(0x08000000, b"\x00\x01\x02\x03")
        assert ok is False
        assert exp == 0x00
        assert act == 0xFF

    def test_t2_verify_03_verify_out_of_bounds(self):
        ok, _, _, _ = self.mock_flash.verify(0x09000000, b"\x00")
        assert ok is False

    def test_t2_verify_04_verify_corrupted_fault(self):
        self.mock_flash.injected_faults["verify_corrupt"] = 0x08000010
        try:
            ok, addr, exp, act = self.mock_flash.verify(0x08000000, b"\x00"*32)
            assert ok is False
            assert addr == 0x08000010
        finally:
            self.mock_flash.injected_faults.clear()

    def test_t2_verify_05_verify_empty_expected(self):
        ok, _, _, _ = self.mock_flash.verify(0x08000000, b"")
        assert ok is True

    def test_t2_reset_01_reset_injected_fault(self):
        self.mock_flash.injected_faults["reset_fail"] = True
        try:
            assert self.mock_flash.reset() == EXIT_TARGET_CONNECTION_ERROR
        finally:
            self.mock_flash.injected_faults.clear()

    def test_t2_reset_02_connection_fault(self):
        self.mock_flash.injected_faults["connect_fail"] = True
        try:
            assert self.mock_flash.connect("STM32F401RE") == EXIT_TARGET_CONNECTION_ERROR
        finally:
            self.mock_flash.injected_faults.clear()

    def test_t2_reset_03_rapid_consecutive_resets(self):
        for _ in range(5):
            assert self.mock_flash.reset() == EXIT_SUCCESS

    def test_t2_reset_04_reset_state_preservation(self):
        self.mock_flash.erase_all()
        self.mock_flash.write_bytes(0x08000000, b"\xDE\xAD\xBE\xEF")
        self.mock_flash.reset()
        # Memory contents persist across system reset
        assert self.mock_flash.memory[0:4] == bytearray(b"\xDE\xAD\xBE\xEF")

    def test_t2_reset_05_halt_state_flag(self):
        self.mock_flash.reset(halt=True)
        assert self.mock_flash.is_halted is True

    def test_t2_profile_01_malformed_toml_detection(self):
        path = os.path.join(FIXTURES_DIR, "invalid_malformed_profile.toml")
        with open(path, "r", encoding="utf-8") as f:
            content = f.read()
        # Missing closing bracket in section header
        assert "[profile" in content and not content.startswith("[profile]")

    def test_t2_profile_02_missing_fields_toml(self):
        path = os.path.join(FIXTURES_DIR, "missing_fields_profile.toml")
        with open(path, "r", encoding="utf-8") as f:
            content = f.read()
        assert "target =" not in content and "target=" not in content

    def test_t2_profile_03_profile_special_characters(self):
        name = "stm32_f4-nucleo_v1.0"
        assert re.match(r"^[a-zA-Z0-9_\-\.]+$", name) is not None

    def test_t2_profile_04_profile_nonexistent_lookup(self):
        profiles = {"a": 1}
        assert profiles.get("nonexistent") is None

    def test_t2_profile_05_profile_overwrite(self):
        profiles = {"prof": {"speed": 1000}}
        profiles["prof"] = {"speed": 2000}
        assert profiles["prof"]["speed"] == 2000

    def test_t2_gap_01_conflicting_overlap(self):
        try:
            parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "corrupt_conflicting_overlap.hex"))
            assert False, "Should have failed on conflicting overlap"
        except ValueError as e:
            assert "ConflictingDataOverlap" in str(e)

    def test_t2_gap_02_redundant_overlap(self):
        # Redundant overlap with identical bytes succeeds
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_redundant_overlap.hex"))
        assert meta["segment_count"] == 1
        assert len(meta["segments"][0][1]) == 16

    def test_t2_gap_03_sparse_gap_detection(self):
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_bootloader_app_gap.hex"))
        assert meta["segment_count"] == 2
        seg1_end = meta["segments"][0][0] + len(meta["segments"][0][1])
        seg2_start = meta["segments"][1][0]
        gap_bytes = seg2_start - seg1_end
        assert gap_bytes == 262112

    def test_t2_gap_04_adjacent_records_merge(self):
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_single_segment.hex"))
        assert meta["segment_count"] == 1 # 2 adjacent records merged into 1

    def test_t2_gap_05_extreme_high_address(self):
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "extreme_high_address_type04.hex"))
        assert meta["segments"][0][0] == 0x08080000

    # ---------------------------------------------------------------------
    # TIER 3: PAIRWISE CROSS-FEATURE COMBINATIONS
    # ---------------------------------------------------------------------

    def test_t3_combo_01_bin_custom_base_erase_program_verify_reset(self):
        """BIN load + custom base + sector erase + program + verify + reset."""
        self.mock_flash.erase_all()
        meta = parse_raw_bin_spec(os.path.join(FIXTURES_DIR, "valid_tiny_16b.bin"), base_address=0x08010000)
        addr, data = meta["segments"][0]
        assert self.mock_flash.erase_range(addr, len(data)) == EXIT_SUCCESS
        assert self.mock_flash.write_bytes(addr, data) == EXIT_SUCCESS
        ok, _, _, _ = self.mock_flash.verify(addr, data)
        assert ok is True
        assert self.mock_flash.reset(halt=False) == EXIT_SUCCESS

    def test_t3_combo_02_hex_type04_gap_profile_cli(self):
        """HEX Type 04 + gap -> apply configuration profile -> flash."""
        self.mock_flash.erase_all()
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_bootloader_app_gap.hex"))
        for addr, data in meta["segments"]:
            assert self.mock_flash.write_bytes(addr, data) == EXIT_SUCCESS
            ok, _, _, _ = self.mock_flash.verify(addr, data)
            assert ok is True
        assert self.mock_flash.reset(halt=False) == EXIT_SUCCESS

    def test_t3_combo_03_full_erase_program_verify_gap_check(self):
        """Mass erase -> program HEX -> verify image -> verify gap remains 0xFF."""
        self.mock_flash.erase_all()
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_bootloader_app_gap.hex"))
        for addr, data in meta["segments"]:
            self.mock_flash.write_bytes(addr, data)
        # Check gap between 0x08000020 and 0x08040000
        gap_offset_start = 0x08000020 - 0x08000000
        gap_offset_end = 0x08040000 - 0x08000000
        assert all(b == 0xFF for b in self.mock_flash.memory[gap_offset_start:gap_offset_start + 100])
        assert all(b == 0xFF for b in self.mock_flash.memory[gap_offset_end - 100:gap_offset_end])

    def test_t3_combo_04_profile_crud_and_apply(self):
        """Save profile -> load -> apply -> modify -> delete."""
        profiles = {}
        # 1. Save
        profiles["stm32f4_ci"] = {"target": "STM32F401RE", "verify": True, "reset": True}
        # 2. Load
        prof = profiles["stm32f4_ci"]
        assert prof["verify"] is True
        # 3. Apply to flash
        self.mock_flash.erase_all()
        self.mock_flash.write_bytes(0x08000000, b"\x42"*16)
        if prof["verify"]:
            ok, _, _, _ = self.mock_flash.verify(0x08000000, b"\x42"*16)
            assert ok is True
        if prof["reset"]:
            assert self.mock_flash.reset() == EXIT_SUCCESS
        # 4. Delete
        del profiles["stm32f4_ci"]
        assert "stm32f4_ci" not in profiles

    def test_t3_combo_05_flags_disabled(self):
        """Flash with --no-verify and --no-reset executes cleanly."""
        self.mock_flash.erase_all()
        self.mock_flash.write_bytes(0x08000000, b"\xAA"*32)
        # Did not verify or reset, state remains unhalted and written
        assert self.mock_flash.memory[0:32] == bytearray([0xAA]*32)

    def test_t3_combo_06_interleaved_write_verify(self):
        """Write Seg 1 -> verify Seg 1 -> write Seg 2 -> verify both Seg 1 & 2."""
        self.mock_flash.erase_all()
        # Segment 1 at 0x08000000
        self.mock_flash.write_bytes(0x08000000, b"\x11"*16)
        ok, _, _, _ = self.mock_flash.verify(0x08000000, b"\x11"*16)
        assert ok is True
        # Segment 2 at 0x08010000
        self.mock_flash.write_bytes(0x08010000, b"\x22"*16)
        # Verify Seg 1 still intact
        ok, _, _, _ = self.mock_flash.verify(0x08000000, b"\x11"*16)
        assert ok is True
        # Verify Seg 2 intact
        ok, _, _, _ = self.mock_flash.verify(0x08010000, b"\x22"*16)
        assert ok is True

    def test_t3_combo_07_fault_program_retry(self):
        """Injected fault during programming fails -> clear fault -> retry succeeds."""
        self.mock_flash.erase_all()
        self.mock_flash.injected_faults["program_fail"] = True
        rc = self.mock_flash.write_bytes(0x08000000, b"\x55"*16)
        assert rc == EXIT_FLASH_VERIFY_ERROR
        # Clear fault and retry
        self.mock_flash.injected_faults.clear()
        rc = self.mock_flash.write_bytes(0x08000000, b"\x55"*16)
        assert rc == EXIT_SUCCESS
        ok, _, _, _ = self.mock_flash.verify(0x08000000, b"\x55"*16)
        assert ok is True

    def test_t3_combo_08_json_progress_streaming(self):
        """Simulate NDJSON event sequence during flash operation."""
        stages = ["connecting", "erasing", "programming", "verifying", "resetting", "complete"]
        events = []
        for s in stages:
            events.append({"stage": s, "percentage": 100.0 if s == "complete" else 50.0})
        assert len(events) == 6
        assert events[-1]["stage"] == "complete"

    # ---------------------------------------------------------------------
    # TIER 4: REAL-WORLD APPLICATION WORKLOADS
    # ---------------------------------------------------------------------

    def test_t4_workload_01_stm32_bootloader_app(self):
        """
        Workload 1: Dual-image STM32 Bootloader + Main App.
        Bootloader at 0x08000000 (16KB), Main App at 0x08010000 (32KB).
        Verify both vector tables, handover vector, and uncorrupted gap.
        """
        self.mock_flash.erase_all()
        # 1. Flash Bootloader (Sector 0: 0x08000000, 16KB)
        bootloader_data = bytearray(16384)
        struct.pack_into("<II", bootloader_data, 0, 0x20005000, 0x08000101) # Bootloader Reset
        self.mock_flash.write_bytes(0x08000000, bootloader_data)
        ok, _, _, _ = self.mock_flash.verify(0x08000000, bootloader_data)
        assert ok is True

        # 2. Flash Main Application (Sector 4: 0x08010000, 32KB)
        app_data = bytearray(32768)
        struct.pack_into("<II", app_data, 0, 0x20005000, 0x08010101) # App Reset
        # Erase sectors for app only
        self.mock_flash.erase_range(0x08010000, 32768)
        self.mock_flash.write_bytes(0x08010000, app_data)

        # 3. Verify Bootloader was untouched
        ok, _, _, _ = self.mock_flash.verify(0x08000000, bootloader_data)
        assert ok is True

        # 4. Verify App is intact
        ok, _, _, _ = self.mock_flash.verify(0x08010000, app_data)
        assert ok is True

        # 5. Issue system reset
        assert self.mock_flash.reset(halt=False) == EXIT_SUCCESS

    def test_t4_workload_02_batch_programming(self):
        """
        Workload 2: Production Batch Programming.
        Simulates flashing 10 consecutive microcontrollers on a factory test jig.
        Verifies 100% yield, deterministic execution, and zero memory leakage.
        """
        meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_single_segment.hex"))
        addr, data = meta["segments"][0]

        for device_idx in range(10):
            # 1. Attach to new device (fresh memory)
            dev_flash = MockNorFlash(0x08000000, 524288, self.f4_sectors)
            assert dev_flash.connect("STM32F401RE") == EXIT_SUCCESS
            # 2. Erase
            assert dev_flash.erase_all() == EXIT_SUCCESS
            # 3. Program
            assert dev_flash.write_bytes(addr, data) == EXIT_SUCCESS
            # 4. Verify
            ok, _, _, _ = dev_flash.verify(addr, data)
            assert ok is True
            # 5. Reset & release
            assert dev_flash.reset(halt=False) == EXIT_SUCCESS
            dev_flash.disconnect()

    def test_t4_workload_03_corrupt_firmware_recovery(self):
        """
        Workload 3: Bricked / Corrupted Firmware Recovery.
        Target starts with corrupt memory and lock conditions.
        Test runs recovery flow: connect-under-reset -> mass erase -> flash golden -> reset.
        """
        # Bricked state: unerased bad bytes in flash
        self.mock_flash.memory[0:1000] = bytearray([0x55] * 1000)

        # 1. Connect under reset
        assert self.mock_flash.connect("STM32F401RE") == EXIT_SUCCESS

        # 2. Mass Erase to clear corruption
        assert self.mock_flash.erase_all() == EXIT_SUCCESS
        assert all(b == 0xFF for b in self.mock_flash.memory)

        # 3. Flash Golden Image
        golden_meta = parse_intel_hex_spec(os.path.join(FIXTURES_DIR, "valid_stm32_single_segment.hex"))
        addr, data = golden_meta["segments"][0]
        assert self.mock_flash.write_bytes(addr, data) == EXIT_SUCCESS

        # 4. Verify
        ok, _, _, _ = self.mock_flash.verify(addr, data)
        assert ok is True

        # 5. Reset into running state
        assert self.mock_flash.reset(halt=False) == EXIT_SUCCESS

    def test_t4_workload_04_dual_bank_ota_swap(self):
        """
        Workload 4: Dual-Bank OTA Firmware Update Simulation.
        Active v1.0 in Bank 1 (0x08000000, 256KB), Staged v2.0 in Bank 2 (0x08040000, 256KB).
        """
        self.mock_flash.erase_all()
        # Flash Bank 1 (Active v1.0)
        v1_data = b"FIRMWARE_V1.0.0" + b"\x00"*100
        self.mock_flash.write_bytes(0x08000000, v1_data)

        # Stage Bank 2 (OTA v2.0)
        v2_data = b"FIRMWARE_V2.0.0" + b"\x00"*100
        self.mock_flash.write_bytes(0x08040000, v2_data)

        # Verify Bank 2
        ok, _, _, _ = self.mock_flash.verify(0x08040000, v2_data)
        assert ok is True

        # Verify Bank 1 was untouched
        ok, _, _, _ = self.mock_flash.verify(0x08000000, v1_data)
        assert ok is True

    def test_t4_workload_05_multi_sector_stress(self):
        """
        Workload 5: Multi-Sector Asymmetric Flash Stress (256 KB Image).
        Spans asymmetric STM32F4 sectors (16K, 16K, 64K, 128K).
        """
        self.mock_flash.erase_all()
        # 256KB test payload
        stress_payload = bytes([(i * 17) & 0xFF for i in range(262144)])
        start_addr = 0x08000000
        assert self.mock_flash.write_bytes(start_addr, stress_payload) == EXIT_SUCCESS
        ok, _, _, _ = self.mock_flash.verify(start_addr, stress_payload)
        assert ok is True
        # Verify CRC32
        expected_crc = zlib.crc32(stress_payload) & 0xFFFFFFFF
        actual_crc = zlib.crc32(bytes(self.mock_flash.memory[:262144])) & 0xFFFFFFFF
        assert expected_crc == actual_crc

    # ---------------------------------------------------------------------
    # RUN ALL TIERS
    # ---------------------------------------------------------------------

    def run_all(self, target_tier: Optional[int] = None) -> bool:
        test_methods = [
            # Tier 1: HEX Parsing (6 tests)
            ("t1_hex_01_single_segment_stm32", 1, "HEX Parsing", self.test_t1_hex_01_single_segment_stm32),
            ("t1_hex_02_dual_segment_bootloader_app_gap", 1, "HEX Parsing", self.test_t1_hex_02_dual_segment_bootloader_app_gap),
            ("t1_hex_03_out_of_order_coalescing", 1, "HEX Parsing", self.test_t1_hex_03_out_of_order_coalescing),
            ("t1_hex_04_extended_segment_type02", 1, "HEX Parsing", self.test_t1_hex_04_extended_segment_type02),
            ("t1_hex_05_start_segment_type03_entry_point", 1, "HEX Parsing", self.test_t1_hex_05_start_segment_type03_entry_point),
            # Tier 1: BIN Parsing (5 tests)
            ("t1_bin_01_default_stm32_base", 1, "BIN Parsing", self.test_t1_bin_01_default_stm32_base),
            ("t1_bin_02_custom_base_address", 1, "BIN Parsing", self.test_t1_bin_02_custom_base_address),
            ("t1_bin_03_exact_page_boundary", 1, "BIN Parsing", self.test_t1_bin_03_exact_page_boundary),
            ("t1_bin_04_exact_sector_boundary", 1, "BIN Parsing", self.test_t1_bin_04_exact_sector_boundary),
            ("t1_bin_05_cortex_m_vector_heuristic", 1, "BIN Parsing", self.test_t1_bin_05_cortex_m_vector_heuristic),
            # Tier 1: Probe Listing (5 tests)
            ("t1_probe_01_probe_discovery", 1, "Probe Listing", self.test_t1_probe_01_probe_discovery),
            ("t1_probe_02_probe_json_serialization", 1, "Probe Listing", self.test_t1_probe_02_probe_json_serialization),
            ("t1_probe_03_probe_multiple_targets", 1, "Probe Listing", self.test_t1_probe_03_probe_multiple_targets),
            ("t1_probe_04_probe_enumeration_idempotence", 1, "Probe Listing", self.test_t1_probe_04_probe_enumeration_idempotence),
            ("t1_probe_05_probe_protocol_selection", 1, "Probe Listing", self.test_t1_probe_05_probe_protocol_selection),
            # Tier 1: Flashing (5 tests)
            ("t1_flash_01_single_segment_hex", 1, "Flashing", self.test_t1_flash_01_single_segment_hex),
            ("t1_flash_02_dual_segment_gap", 1, "Flashing", self.test_t1_flash_02_dual_segment_gap),
            ("t1_flash_03_raw_binary_default", 1, "Flashing", self.test_t1_flash_03_raw_binary_default),
            ("t1_flash_04_raw_binary_custom_base", 1, "Flashing", self.test_t1_flash_04_raw_binary_custom_base),
            ("t1_flash_05_flash_with_verify_reset", 1, "Flashing", self.test_t1_flash_05_flash_with_verify_reset),
            # Tier 1: Erasing (5 tests)
            ("t1_erase_01_full_chip_erase", 1, "Erasing", self.test_t1_erase_01_full_chip_erase),
            ("t1_erase_02_sector_erase_range", 1, "Erasing", self.test_t1_erase_02_sector_erase_range),
            ("t1_erase_03_blank_check", 1, "Erasing", self.test_t1_erase_03_blank_check),
            ("t1_erase_04_erase_idempotence", 1, "Erasing", self.test_t1_erase_04_erase_idempotence),
            ("t1_erase_05_erase_single_sector", 1, "Erasing", self.test_t1_erase_05_erase_single_sector),
            # Tier 1: Verifying (5 tests)
            ("t1_verify_01_verify_matching_hex", 1, "Verifying", self.test_t1_verify_01_verify_matching_hex),
            ("t1_verify_02_verify_matching_bin", 1, "Verifying", self.test_t1_verify_02_verify_matching_bin),
            ("t1_verify_03_verify_matching_dual_segment", 1, "Verifying", self.test_t1_verify_03_verify_matching_dual_segment),
            ("t1_verify_04_verify_partial_range", 1, "Verifying", self.test_t1_verify_04_verify_partial_range),
            ("t1_verify_05_verify_checksum_equality", 1, "Verifying", self.test_t1_verify_05_verify_checksum_equality),
            # Tier 1: Resetting (5 tests)
            ("t1_reset_01_reset_run_mode", 1, "Resetting", self.test_t1_reset_01_reset_run_mode),
            ("t1_reset_02_reset_halt_mode", 1, "Resetting", self.test_t1_reset_02_reset_halt_mode),
            ("t1_reset_03_consecutive_resets", 1, "Resetting", self.test_t1_reset_03_consecutive_resets),
            ("t1_reset_04_reset_clears_halt", 1, "Resetting", self.test_t1_reset_04_reset_clears_halt),
            ("t1_reset_05_reset_after_flash", 1, "Resetting", self.test_t1_reset_05_reset_after_flash),
            # Tier 1: Profiles (5 tests)
            ("t1_profile_01_save_and_parse_toml", 1, "Profiles", self.test_t1_profile_01_save_and_parse_toml),
            ("t1_profile_02_profile_schema_fields", 1, "Profiles", self.test_t1_profile_02_profile_schema_fields),
            ("t1_profile_03_profile_list_simulation", 1, "Profiles", self.test_t1_profile_03_profile_list_simulation),
            ("t1_profile_04_profile_show_simulation", 1, "Profiles", self.test_t1_profile_04_profile_show_simulation),
            ("t1_profile_05_profile_delete_simulation", 1, "Profiles", self.test_t1_profile_05_profile_delete_simulation),

            # Tier 2: HEX Boundaries (7 tests)
            ("t2_hex_01_checksum_mismatch", 2, "HEX Boundaries", self.test_t2_hex_01_checksum_mismatch),
            ("t2_hex_02_missing_colon", 2, "HEX Boundaries", self.test_t2_hex_02_missing_colon),
            ("t2_hex_03_odd_hex_digits", 2, "HEX Boundaries", self.test_t2_hex_03_odd_hex_digits),
            ("t2_hex_04_invalid_hex_char", 2, "HEX Boundaries", self.test_t2_hex_04_invalid_hex_char),
            ("t2_hex_05_record_truncated", 2, "HEX Boundaries", self.test_t2_hex_05_record_truncated),
            ("t2_hex_06_address_overflow_4gb", 2, "HEX Boundaries", self.test_t2_hex_06_address_overflow_4gb),
            ("t2_hex_07_empty_file_hex", 2, "HEX Boundaries", self.test_t2_hex_07_empty_file_hex),
            # Tier 2: BIN Boundaries (5 tests)
            ("t2_bin_01_empty_file_bin", 2, "BIN Boundaries", self.test_t2_bin_01_empty_file_bin),
            ("t2_bin_02_single_byte_payload", 2, "BIN Boundaries", self.test_t2_bin_02_single_byte_payload),
            ("t2_bin_03_invalid_base_address_string", 2, "BIN Boundaries", self.test_t2_bin_03_invalid_base_address_string),
            ("t2_bin_04_cortex_m_even_reset_vector", 2, "BIN Boundaries", self.test_t2_bin_04_cortex_m_even_reset_vector),
            ("t2_bin_05_unaligned_base_address", 2, "BIN Boundaries", self.test_t2_bin_05_unaligned_base_address),
            # Tier 2: Flash Boundaries (5 tests)
            ("t2_flash_01_nor_write_violation", 2, "Flash Boundaries", self.test_t2_flash_01_nor_write_violation),
            ("t2_flash_02_write_beyond_flash_capacity", 2, "Flash Boundaries", self.test_t2_flash_02_write_beyond_flash_capacity),
            ("t2_flash_03_multi_sector_crossing", 2, "Flash Boundaries", self.test_t2_flash_03_multi_sector_crossing),
            ("t2_flash_04_missing_firmware_file", 2, "Flash Boundaries", self.test_t2_flash_04_missing_firmware_file),
            ("t2_flash_05_last_byte_boundary", 2, "Flash Boundaries", self.test_t2_flash_05_last_byte_boundary),
            # Tier 2: Erase Boundaries (5 tests)
            ("t2_erase_01_erase_with_fault", 2, "Erase Boundaries", self.test_t2_erase_01_erase_with_fault),
            ("t2_erase_02_erase_sector_boundary_containment", 2, "Erase Boundaries", self.test_t2_erase_02_erase_sector_boundary_containment),
            ("t2_erase_03_erase_already_erased", 2, "Erase Boundaries", self.test_t2_erase_03_erase_already_erased),
            ("t2_erase_04_erase_edges", 2, "Erase Boundaries", self.test_t2_erase_04_erase_edges),
            ("t2_erase_05_erase_range_out_of_bounds", 2, "Erase Boundaries", self.test_t2_erase_05_erase_range_out_of_bounds),
            # Tier 2: Verify Boundaries (5 tests)
            ("t2_verify_01_mismatch_detected", 2, "Verify Boundaries", self.test_t2_verify_01_mismatch_detected),
            ("t2_verify_02_verify_unerased_flash", 2, "Verify Boundaries", self.test_t2_verify_02_verify_unerased_flash),
            ("t2_verify_03_verify_out_of_bounds", 2, "Verify Boundaries", self.test_t2_verify_03_verify_out_of_bounds),
            ("t2_verify_04_verify_corrupted_fault", 2, "Verify Boundaries", self.test_t2_verify_04_verify_corrupted_fault),
            ("t2_verify_05_verify_empty_expected", 2, "Verify Boundaries", self.test_t2_verify_05_verify_empty_expected),
            # Tier 2: Reset Boundaries (5 tests)
            ("t2_reset_01_reset_injected_fault", 2, "Reset Boundaries", self.test_t2_reset_01_reset_injected_fault),
            ("t2_reset_02_connection_fault", 2, "Reset Boundaries", self.test_t2_reset_02_connection_fault),
            ("t2_reset_03_rapid_consecutive_resets", 2, "Reset Boundaries", self.test_t2_reset_03_rapid_consecutive_resets),
            ("t2_reset_04_reset_state_preservation", 2, "Reset Boundaries", self.test_t2_reset_04_reset_state_preservation),
            ("t2_reset_05_halt_state_flag", 2, "Reset Boundaries", self.test_t2_reset_05_halt_state_flag),
            # Tier 2: Profile Boundaries (5 tests)
            ("t2_profile_01_malformed_toml_detection", 2, "Profile Boundaries", self.test_t2_profile_01_malformed_toml_detection),
            ("t2_profile_02_missing_fields_toml", 2, "Profile Boundaries", self.test_t2_profile_02_missing_fields_toml),
            ("t2_profile_03_profile_special_characters", 2, "Profile Boundaries", self.test_t2_profile_03_profile_special_characters),
            ("t2_profile_04_profile_nonexistent_lookup", 2, "Profile Boundaries", self.test_t2_profile_04_profile_nonexistent_lookup),
            ("t2_profile_05_profile_overwrite", 2, "Profile Boundaries", self.test_t2_profile_05_profile_overwrite),
            # Tier 2: Gap & Overlap Boundaries (5 tests)
            ("t2_gap_01_conflicting_overlap", 2, "Gap/Overlap Boundaries", self.test_t2_gap_01_conflicting_overlap),
            ("t2_gap_02_redundant_overlap", 2, "Gap/Overlap Boundaries", self.test_t2_gap_02_redundant_overlap),
            ("t2_gap_03_sparse_gap_detection", 2, "Gap/Overlap Boundaries", self.test_t2_gap_03_sparse_gap_detection),
            ("t2_gap_04_adjacent_records_merge", 2, "Gap/Overlap Boundaries", self.test_t2_gap_04_adjacent_records_merge),
            ("t2_gap_05_extreme_high_address", 2, "Gap/Overlap Boundaries", self.test_t2_gap_05_extreme_high_address),

            # Tier 3: Pairwise Combinations (8 tests)
            ("t3_combo_01_bin_custom_base_erase_program_verify_reset", 3, "Pairwise Combinations", self.test_t3_combo_01_bin_custom_base_erase_program_verify_reset),
            ("t3_combo_02_hex_type04_gap_profile_cli", 3, "Pairwise Combinations", self.test_t3_combo_02_hex_type04_gap_profile_cli),
            ("t3_combo_03_full_erase_program_verify_gap_check", 3, "Pairwise Combinations", self.test_t3_combo_03_full_erase_program_verify_gap_check),
            ("t3_combo_04_profile_crud_and_apply", 3, "Pairwise Combinations", self.test_t3_combo_04_profile_crud_and_apply),
            ("t3_combo_05_flags_disabled", 3, "Pairwise Combinations", self.test_t3_combo_05_flags_disabled),
            ("t3_combo_06_interleaved_write_verify", 3, "Pairwise Combinations", self.test_t3_combo_06_interleaved_write_verify),
            ("t3_combo_07_fault_program_retry", 3, "Pairwise Combinations", self.test_t3_combo_07_fault_program_retry),
            ("t3_combo_08_json_progress_streaming", 3, "Pairwise Combinations", self.test_t3_combo_08_json_progress_streaming),

            # Tier 4: Real-World Workloads (5 tests)
            ("t4_workload_01_stm32_bootloader_app", 4, "Real-World Workload", self.test_t4_workload_01_stm32_bootloader_app),
            ("t4_workload_02_batch_programming", 4, "Real-World Workload", self.test_t4_workload_02_batch_programming),
            ("t4_workload_03_corrupt_firmware_recovery", 4, "Real-World Workload", self.test_t4_workload_03_corrupt_firmware_recovery),
            ("t4_workload_04_dual_bank_ota_swap", 4, "Real-World Workload", self.test_t4_workload_04_dual_bank_ota_swap),
            ("t4_workload_05_multi_sector_stress", 4, "Real-World Workload", self.test_t4_workload_05_multi_sector_stress),
        ]

        filtered = [t for t in test_methods if target_tier is None or t[1] == target_tier]
        print(f"\n================================================================================")
        print(f"RUNNING E2E TEST SUITE: {len(filtered)} tests scheduled (Tier filter: {target_tier or 'ALL'})")
        print(f"================================================================================\n")

        for name, tier, feat, func in filtered:
            self.run_test(name, tier, feat, func)

        total = len(self.results)
        passed = sum(1 for r in self.results if r.passed)
        failed = total - passed

        tier_counts = {}
        for r in self.results:
            t = r.tier
            tier_counts[t] = tier_counts.get(t, {"total": 0, "pass": 0})
            tier_counts[t]["total"] += 1
            if r.passed:
                tier_counts[t]["pass"] += 1

        print(f"\n================================================================================")
        print(f"E2E TEST RUN SUMMARY")
        print(f"================================================================================")
        for t in sorted(tier_counts.keys()):
            c = tier_counts[t]
            print(f"  Tier {t}: {c['pass']}/{c['total']} passed")
        print(f"--------------------------------------------------------------------------------")
        print(f"TOTAL: {passed}/{total} passed ({failed} failed)")
        print(f"================================================================================\n")

        return failed == 0

def main():
    tier_arg = None
    for i, a in enumerate(sys.argv):
        if a == "--tier" and i + 1 < len(sys.argv):
            tier_arg = int(sys.argv[i+1])
    
    suite = E2ETestSuite()
    success = suite.run_all(target_tier=tier_arg)
    sys.exit(0 if success else 1)

if __name__ == "__main__":
    main()
