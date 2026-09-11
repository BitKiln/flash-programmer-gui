#!/usr/bin/env python3
"""
Generates all test fixtures for the Flash Programmer GUI & CLI E2E test suite.
Produces deterministic Intel HEX, raw binary, and TOML profile files.
"""

import os
import struct
import zlib
import hashlib

FIXTURES_DIR = os.path.join(os.path.dirname(__file__), "fixtures")
os.makedirs(FIXTURES_DIR, exist_ok=True)

def hex_line(byte_count: int, address: int, record_type: int, data: bytes) -> str:
    raw = bytes([byte_count, (address >> 8) & 0xFF, address & 0xFF, record_type]) + data
    checksum = (-sum(raw)) & 0xFF
    return f":{raw.hex().upper()}{checksum:02X}\n"

def main():
    # 1. valid_stm32_single_segment.hex (Vector 1)
    v1_content = (
        hex_line(2, 0x0000, 4, bytes([0x08, 0x00])) +
        hex_line(16, 0x0000, 0, bytes.fromhex("00500020CD010008D1010008D3010008")) +
        hex_line(16, 0x0010, 0, bytes.fromhex("D5010008D7010008D901000800000000")) +
        hex_line(4, 0x0000, 5, bytes.fromhex("080001CD")) +
        ":00000001FF\n"
    )
    with open(os.path.join(FIXTURES_DIR, "valid_stm32_single_segment.hex"), "w", encoding="utf-8") as f:
        f.write(v1_content)

    # 2. valid_stm32_bootloader_app_gap.hex (Vector 2)
    v2_content = (
        hex_line(2, 0x0000, 4, bytes([0x08, 0x00])) +
        hex_line(16, 0x0000, 0, bytes.fromhex("00500020CD010008D1010008D3010008")) +
        hex_line(16, 0x0010, 0, bytes.fromhex("D5010008D7010008D901000800000000")) +
        hex_line(2, 0x0000, 4, bytes([0x08, 0x04])) +
        hex_line(16, 0x0000, 0, bytes.fromhex("DEADBEEFCAFEBABE0123456789ABCDEF")) +
        ":00000001FF\n"
    )
    with open(os.path.join(FIXTURES_DIR, "valid_stm32_bootloader_app_gap.hex"), "w", encoding="utf-8") as f:
        f.write(v2_content)

    # 3. valid_stm32_out_of_order.hex (Vector 3)
    v3_content = (
        hex_line(2, 0x0000, 4, bytes([0x08, 0x00])) +
        hex_line(16, 0x0010, 0, bytes.fromhex("D5010008D7010008D901000800000000")) +
        hex_line(16, 0x0000, 0, bytes.fromhex("00500020CD010008D1010008D3010008")) +
        ":00000001FF\n"
    )
    with open(os.path.join(FIXTURES_DIR, "valid_stm32_out_of_order.hex"), "w", encoding="utf-8") as f:
        f.write(v3_content)

    # 4. valid_extended_segment_type02.hex
    v4_content = (
        hex_line(2, 0x0000, 2, bytes([0x10, 0x00])) +
        hex_line(16, 0x0000, 0, bytes(range(16))) +
        ":00000001FF\n"
    )
    with open(os.path.join(FIXTURES_DIR, "valid_extended_segment_type02.hex"), "w", encoding="utf-8") as f:
        f.write(v4_content)

    # 5. valid_start_segment_type03.hex
    v5_content = (
        hex_line(2, 0x0000, 4, bytes([0x08, 0x00])) +
        hex_line(16, 0x0000, 0, bytes.fromhex("00500020CD010008D1010008D3010008")) +
        hex_line(4, 0x0000, 3, bytes([0x10, 0x00, 0x01, 0x00])) +
        ":00000001FF\n"
    )
    with open(os.path.join(FIXTURES_DIR, "valid_start_segment_type03.hex"), "w", encoding="utf-8") as f:
        f.write(v5_content)

    # 6. valid_redundant_overlap.hex
    v6_content = (
        hex_line(2, 0x0000, 4, bytes([0x08, 0x00])) +
        hex_line(16, 0x0000, 0, bytes.fromhex("00500020CD010008D1010008D3010008")) +
        hex_line(16, 0x0000, 0, bytes.fromhex("00500020CD010008D1010008D3010008")) +
        ":00000001FF\n"
    )
    with open(os.path.join(FIXTURES_DIR, "valid_redundant_overlap.hex"), "w", encoding="utf-8") as f:
        f.write(v6_content)

    # 7. corrupt_bad_checksum.hex
    v7_content = ":020000040800F1\n:1000000000500020CD010008D1010008D3010008F4\n:00000001FF\n"
    with open(os.path.join(FIXTURES_DIR, "corrupt_bad_checksum.hex"), "w", encoding="utf-8") as f:
        f.write(v7_content)

    # 8. corrupt_missing_colon.hex
    v8_content = "020000040800F2\n:1000000000500020CD010008D1010008D3010008F4\n:00000001FF\n"
    with open(os.path.join(FIXTURES_DIR, "corrupt_missing_colon.hex"), "w", encoding="utf-8") as f:
        f.write(v8_content)

    # 9. corrupt_odd_hex_digits.hex
    v9_content = ":02000004080F2\n:00000001FF\n"
    with open(os.path.join(FIXTURES_DIR, "corrupt_odd_hex_digits.hex"), "w", encoding="utf-8") as f:
        f.write(v9_content)

    # 10. corrupt_invalid_hex_char.hex
    v10_content = ":020000040800FZ\n:00000001FF\n"
    with open(os.path.join(FIXTURES_DIR, "corrupt_invalid_hex_char.hex"), "w", encoding="utf-8") as f:
        f.write(v10_content)

    # 11. corrupt_truncated.hex
    v11_content = ":0200000408\n"
    with open(os.path.join(FIXTURES_DIR, "corrupt_truncated.hex"), "w", encoding="utf-8") as f:
        f.write(v11_content)

    # 12. corrupt_conflicting_overlap.hex
    v12_content = (
        hex_line(2, 0x0000, 4, bytes([0x08, 0x00])) +
        hex_line(1, 0x0000, 0, bytes([0xAA])) +
        hex_line(1, 0x0000, 0, bytes([0xBB])) +
        ":00000001FF\n"
    )
    with open(os.path.join(FIXTURES_DIR, "corrupt_conflicting_overlap.hex"), "w", encoding="utf-8") as f:
        f.write(v12_content)

    # 13. extreme_high_address_type04.hex
    v13_content = (
        hex_line(2, 0x0000, 4, bytes([0x08, 0x08])) +
        hex_line(16, 0x0000, 0, bytes.fromhex("00500020CD010008D1010008D3010008")) +
        ":00000001FF\n"
    )
    with open(os.path.join(FIXTURES_DIR, "extreme_high_address_type04.hex"), "w", encoding="utf-8") as f:
        f.write(v13_content)

    # 14. extreme_address_overflow_4gb.hex
    v14_content = (
        hex_line(2, 0x0000, 4, bytes([0xFF, 0xFF])) +
        hex_line(16, 0xFFF8, 0, bytes.fromhex("00500020CD010008D1010008D3010008")) +
        ":00000001FF\n"
    )
    with open(os.path.join(FIXTURES_DIR, "extreme_address_overflow_4gb.hex"), "w", encoding="utf-8") as f:
        f.write(v14_content)

    # 15. empty_file.hex
    with open(os.path.join(FIXTURES_DIR, "empty_file.hex"), "w", encoding="utf-8") as f:
        pass

    # 16. valid_stm32_cortex_m_vector.bin (1024 bytes)
    # SP: 0x20005000, Reset Handler: 0x080001CD (Thumb bit 1)
    bin_data = bytearray(1024)
    struct.pack_into("<II", bin_data, 0, 0x20005000, 0x080001CD)
    for i in range(8, 1024):
        bin_data[i] = (i * 7 + 13) & 0xFF
    with open(os.path.join(FIXTURES_DIR, "valid_stm32_cortex_m_vector.bin"), "wb") as f:
        f.write(bin_data)

    # 17. valid_tiny_16b.bin
    with open(os.path.join(FIXTURES_DIR, "valid_tiny_16b.bin"), "wb") as f:
        f.write(bytes(range(16)))

    # 18. valid_exact_page_256b.bin
    with open(os.path.join(FIXTURES_DIR, "valid_exact_page_256b.bin"), "wb") as f:
        f.write(bytes([(x * 3) & 0xFF for x in range(256)]))

    # 19. valid_exact_sector_1kb.bin
    with open(os.path.join(FIXTURES_DIR, "valid_exact_sector_1kb.bin"), "wb") as f:
        f.write(bytes([(x ^ 0x5A) & 0xFF for x in range(1024)]))

    # 20. valid_multi_sector_16kb.bin
    with open(os.path.join(FIXTURES_DIR, "valid_multi_sector_16kb.bin"), "wb") as f:
        f.write(bytes([(x & 0xFF) for x in range(16384)]))

    # 21. corrupt_odd_reset_vector.bin (Thumb bit 0: 0x080001CC)
    bad_bin = bytearray(1024)
    struct.pack_into("<II", bad_bin, 0, 0x20005000, 0x080001CC)
    with open(os.path.join(FIXTURES_DIR, "corrupt_odd_reset_vector.bin"), "wb") as f:
        f.write(bad_bin)

    # 22. empty_file.bin
    with open(os.path.join(FIXTURES_DIR, "empty_file.bin"), "wb") as f:
        pass

    # 23. valid_stm32f4_profile.toml
    p1 = """[profile]
schema_version = 1
name = "stm32f4_dev"
description = "STM32F401RE development profile"
target = "STM32F401RE"
interface = "SWD"
speed_khz = 2000
probe_id = "mock:stm32f401"

[firmware]
default_path = "tests/fixtures/valid_stm32_single_segment.hex"
base_address = "0x08000000"

[options]
verify_after = true
reset_after = true
full_chip_erase = false
"""
    with open(os.path.join(FIXTURES_DIR, "valid_stm32f4_profile.toml"), "w", encoding="utf-8") as f:
        f.write(p1)

    # 24. valid_stm32f1_profile.toml
    p2 = """[profile]
schema_version = 1
name = "stm32f1_prod"
description = "STM32F103C8 production profile"
target = "STM32F103C8"
interface = "SWD"
speed_khz = 1000
probe_id = "mock:stm32f103"

[firmware]
default_path = "tests/fixtures/valid_exact_sector_1kb.bin"
base_address = "0x08000000"

[options]
verify_after = true
reset_after = true
full_chip_erase = true
"""
    with open(os.path.join(FIXTURES_DIR, "valid_stm32f1_profile.toml"), "w", encoding="utf-8") as f:
        f.write(p2)

    # 25. invalid_malformed_profile.toml
    with open(os.path.join(FIXTURES_DIR, "invalid_malformed_profile.toml"), "w", encoding="utf-8") as f:
        f.write("[profile\nname = 'broken'\n")

    # 26. missing_fields_profile.toml
    with open(os.path.join(FIXTURES_DIR, "missing_fields_profile.toml"), "w", encoding="utf-8") as f:
        f.write("[profile]\nschema_version = 1\nname = 'incomplete'\n")

    print(f"Generated all fixtures in {FIXTURES_DIR}")

if __name__ == "__main__":
    main()
