# TEST_INFRA.md: Opaque-Box E2E Testing Infrastructure

**Author**: E2E Test Suite Designer & Writer  
**Target Project**: MCU Flash Programmer GUI & CLI (`flash_programmer_gui`)  
**Specification Sources**: 
- `ORIGINAL_REQUEST.md` (Requirements R1, R2, R3, R4)
- `PROJECT.md` (Features F01–F42, Interface Contracts, Milestones)
- Intel HEX Specification (Revision A, 1988)
- ARMv7-M Architecture Reference Manual (Vector Table & Reset Handler)
- IEEE 802.3 CRC32, RFC 1321 MD5, FIPS 180-4 SHA-256
- STM32 Flash Memory Programming Manuals (F1 uniform, F4 asymmetric)

---

## 1. Test Philosophy & Design Principles

The E2E testing framework is designed as a **strictly opaque-box (black-box), requirement-driven test suite**. It treats the entire system — CLI binary, mock probe driver, firmware parser, flash memory core, and configuration subsystem — as an opaque black box accessed strictly through public interfaces, command-line arguments, environment configurations, and standard return codes.

### 1.1 Core Tenets
1. **Authoritative Specification Derivation**: Every test case has an explicit, authoritative derivation source (Intel HEX Spec 1988, ARMv7-M Ref Manual, STM32 Reference Manuals, IEEE/RFC standards). Expected values (checksums, addresses, memory bytes, exit codes) are mathematically derived from first principles rather than matching arbitrary implementation artifacts.
2. **Deterministic NOR Flash Physics**: Memory simulations strictly adhere to physical semiconductor behavior:
   - Erased state is always `0xFF`.
   - Programming transitions bits exclusively from `1` to `0`. Writing `1` into a bit already `0` without sector erasure triggers a NOR flash write violation.
   - Sector erasure granularity is strictly enforced.
3. **Hermetic Test Isolation**: Every test creates its own independent target memory state, temporary configuration directories, and test artifacts. Tests run concurrently or in arbitrary order with zero cross-contamination.
4. **Deterministic Exit Codes**: All CLI test assertions validate deterministic exit codes:
   - `0`: Success
   - `1`: Flash / Verification mismatch error
   - `2`: Target connection error
   - `3`: Firmware parse / file integrity error
   - `4`: Probe detection error
   - `5`: Argument / Profile validation error
5. **Zero Flakiness**: Timeouts, sleeps, and race conditions are eliminated by using deterministic event queues, mock clocks, and synchronized step execution.

---

## 2. 4-Tier Coverage Methodology

The test suite is structured into four distinct, progressive tiers:

```
┌────────────────────────────────────────────────────────────────────────┐
│                        4-TIER TESTING HIERARCHY                        │
├────────────────────────────────────────────────────────────────────────┤
│ Tier 1: Feature Coverage (>=5 tests per feature)                       │
│ - Isolated, happy-path verification of all 8 core capabilities:        │
│   HEX parsing, BIN parsing, probe listing, flashing, erasing,          │
│   verifying, resetting, and profile management.                        │
├────────────────────────────────────────────────────────────────────────┤
│ Tier 2: Boundary & Corner Cases (>=5 tests per feature)                │
│ - Extreme bounds, corrupt checksums, 0-byte files, multi-MB payloads,  │
│   4GB address overflows, NOR write violations, fault injection.        │
├────────────────────────────────────────────────────────────────────────┤
│ Tier 3: Pairwise Cross-Feature Combinations                            │
│ - Realistic multi-step flows connecting features in tandem:            │
│   BIN + Custom Base + Erase + Program + Verify + Reset                 │
│   HEX Type 04 + Sparse Gap + Profile Save + CLI Execution              │
├────────────────────────────────────────────────────────────────────────┤
│ Tier 4: Real-World Application Workloads                               │
│ - End-to-end production scenarios: STM32 dual-bank bootloader+app,    │
│   factory batch programming, bricked firmware recovery, OTA swap.      │
└────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Tier 1: Feature Coverage (Isolated Happy Paths)
Provides at least 5 isolated tests per feature to establish baseline compliance:
- **HEX Parsing (F01–F06, F11, F14)**: Single segment, multi-segment, Type 02 segmented, Type 04 linear, Type 03/05 entry points, checksum validation.
- **BIN Parsing (F07, F11, F12)**: Default STM32 base address (0x08000000), custom base address, Cortex-M reset vector deduction, metadata summary generation.
- **Probe Listing (F16, F28)**: Virtual mock probe enumeration, probe information fields (name, vendor, protocols, speeds), JSON formatting.
- **Flashing (F19, F23, F27, F29)**: Single-sector write, multi-sector write, binary image flashing, full progress event progression.
- **Erasing (F18, F23, F30)**: Full mass erase, sector range erase, single sector erase, blank-check confirmation (all bytes 0xFF).
- **Verifying (F20, F23, F30)**: Successful byte-for-byte readback match against HEX, successful match against BIN, checksum match.
- **Resetting (F21, F23, F30)**: Target CPU reset (run mode), target CPU reset-and-halt, reset flag integration with flash command.
- **Profiles (F31)**: Save TOML profile, list profiles, inspect profile details, apply profile during flash execution, delete profile.

### 2.2 Tier 2: Boundary & Corner Cases
Attacks each feature with edge cases, corrupt data, and extreme parameters:
- **HEX Boundaries**: Checksum mismatch off-by-one, missing colon prefix, odd character count, non-hex ASCII characters, record truncation, data records after EOF, missing EOF, 32-bit address overflow (>0xFFFFFFFF), conflicting overlapping addresses, target flash boundary overflow.
- **BIN Boundaries**: Zero-byte file, 1-byte payload, exact page boundary (256B), exact sector boundary (1024B), large multi-sector binary, unaligned base address, even reset vector (Thumb bit 0 == 0 rejected).
- **Flash Boundaries**: Writing 0xFF over 0x00 without erase (NOR violation), writing exactly to the last byte of flash, attempting to write beyond flash capacity, partial sector erase boundary alignment.
- **Fault Injection (F25)**: Connection failure, erase write-protection lockout, programming timeout, verification readback corrupt byte, system reset line failure.

### 2.3 Tier 3: Pairwise Cross-Feature Combinations
Validates interaction between distinct modules:
1. `bin_load_custom_base_erase_program_verify_reset`: BIN parsing + custom offset + sector erase + program + verify + reset.
2. `hex_extended_linear_gap_profile_cli`: HEX Type 04 + sparse 256KB gap + save configuration profile + CLI execution with `--profile`.
3. `full_chip_erase_then_flash_verify`: Mass erase + blank check + flash multi-segment HEX + verify flash contents + verify gap is clean 0xFF.
4. `profile_crud_and_apply`: Create -> Show -> List -> Apply to Flash -> Modify -> Delete.
5. `flashing_with_all_flags_disabled`: Flash with `--no-verify --no-reset` executes cleanly without secondary steps.
6. `multiple_segments_interleaved_write_verify`: Write Segment 1 -> Verify Segment 1 -> Write Segment 2 -> Verify both Segment 1 and Segment 2 remain intact.
7. `fault_injection_during_program_then_retry`: Simulated brownout failure on attempt 1 (exit code 1) -> clear fault -> retry (exit code 0).
8. `json_progress_streaming_and_exit_code`: Full flash command with `--json` streaming NDJSON events -> parse events -> verify stage transitions -> exit code 0.

### 2.4 Tier 4: Real-World Embedded Workloads
Simulates complete embedded engineering lifecycles:
1. **STM32 Bootloader + Main App**: Bootloader at `0x08000000` (16KB) + Main App at `0x08010000` (32KB) separated by sparse gap. Flashes bootloader, verifies vector table, flashes main app, verifies entire memory map without erasing bootloader.
2. **Production Batch Programming Simulation**: Automated factory line test: 10 consecutive simulated microcontrollers programmed, verified, and reset in high-speed sequence with zero memory leakage.
3. **Corrupt Firmware Recovery**: Device flash starts in a corrupted/bricked state (bad vector table, partial writes). Test initiates connect-under-reset, executes full mass erase, writes golden firmware, verifies CRC32, and restores running state.
4. **Dual-Bank OTA Image Swap**: Active firmware in Bank 1 (`0x08000000`), staging firmware written to Bank 2 (`0x08040000`), verify Bank 2, simulate bootloader flag swap, verify Bank 1 was untouched.
5. **Multi-Sector Asymmetric Flash Stress**: 256KB firmware spanning asymmetric STM32F4 sectors (16KB, 16KB, 64KB, 128KB), verifying sector alignment, flash time tracking, and integrity.

---

## 3. Feature Inventory Coverage Matrix

| Feature ID | Feature Name | Tier 1 Tests | Tier 2 Boundaries | Tier 3 Combinations | Tier 4 Workloads |
|---|---|---|---|---|---|
| **F01** | Intel HEX Lexing & Framing | `test_hex_lexing_*` | `test_corrupt_framing_*` | `test_hex_flow_*` | Workloads 1, 3, 4 |
| **F02** | Intel HEX Modulo-256 Checksum | `test_hex_checksum_*` | `test_bad_checksum_*` | `test_hex_flow_*` | Workloads 1, 3 |
| **F03** | Record Types 00 & 01 (Data, EOF) | `test_hex_data_eof_*` | `test_data_after_eof_*` | `test_hex_flow_*` | Workloads 1, 3, 5 |
| **F04** | Extended Segment Address (02) | `test_hex_type02_*` | `test_type02_bounds_*` | `test_hex_flow_*` | - |
| **F05** | Extended Linear Address (04) | `test_hex_type04_*` | `test_type04_overflow_*`| `test_pairwise_hex_*` | Workloads 1, 4, 5 |
| **F06** | Start Address Records (03, 05) | `test_hex_entry_point_*`| `test_invalid_entry_*` | `test_pairwise_hex_*` | Workloads 1, 4 |
| **F07** | Raw Binary Image Loading | `test_bin_load_*` | `test_bin_boundaries_*` | `test_pairwise_bin_*` | Workloads 2, 5 |
| **F08** | Memory Segment Consolidation | `test_segment_merge_*` | `test_out_of_order_*` | `test_pairwise_hex_*` | Workloads 1, 4 |
| **F09** | Memory Gap Detection | `test_gap_detection_*` | `test_large_sparse_gap_*`| `test_pairwise_hex_*`| Workloads 1, 4 |
| **F10** | Record Overlap & Collision | `test_overlap_redundant_*`| `test_overlap_conflict_*`| `test_pairwise_hex_*` | Workload 3 |
| **F11** | Multi-Algorithm Checksums | `test_checksum_crc32_*` | `test_checksum_vectors_*`| `test_pairwise_bin_*` | Workloads 1, 2, 3, 5 |
| **F12** | Cortex-M Entry Point Heuristic| `test_cortex_m_vector_*`| `test_invalid_thumb_bit_*`| `test_pairwise_bin_*` | Workloads 1, 3 |
| **F13** | Target Bounds Checking | `test_target_bounds_*` | `test_out_of_bounds_*` | `test_pairwise_hex_*` | Workload 5 |
| **F14** | Serde Metadata Models | `test_serde_models_*` | `test_serde_edge_cases_*`| `test_json_events_*` | All Workloads |
| **F15** | Two-Tier Trait Abstraction | `test_trait_contracts_*`| `test_trait_errors_*` | All Combinations | All Workloads |
| **F16** | Probe Discovery & Enumeration | `test_probe_list_*` | `test_probe_empty_*` | `test_cli_probe_*` | Workload 2 |
| **F17** | Target Connection & Protocol | `test_target_connect_*` | `test_connect_failure_*`| All Combinations | All Workloads |
| **F18** | Flash Erase (Mass & Sector) | `test_erase_mass_*` | `test_erase_protected_*`| `test_erase_program_*`| Workloads 1, 3 |
| **F19** | Flash Programming | `test_program_basic_*` | `test_nor_violation_*` | All Combinations | All Workloads |
| **F20** | Flash Verification | `test_verify_match_*` | `test_verify_mismatch_*`| All Combinations | All Workloads |
| **F21** | Target System Reset | `test_reset_modes_*` | `test_reset_failure_*` | `test_program_reset_*` | Workloads 1, 2, 3 |
| **F23** | In-Memory Mock Backend | `test_mock_physics_*` | `test_mock_boundaries_*`| All Combinations | All Workloads |
| **F24** | Realistic STM32 Profiles | `test_stm32_profiles_*` | `test_asymmetric_geom_*`| All Combinations | Workloads 1, 4, 5 |
| **F25** | Deterministic Fault Injection | `test_fault_setup_*` | `test_fault_injection_*`| `test_fault_retry_*` | Workload 3 |
| **F26** | Event-Driven Progress | `test_progress_events_*`| `test_progress_overflow_*`| `test_json_events_*` | Workload 5 |
| **F27** | High-Level Execution Pipeline | `test_manager_flow_*` | `test_manager_errors_*` | All Combinations | All Workloads |
| **F28** | CLI Probe Listing (`devices`) | `test_cli_devices_*` | `test_cli_devices_bad_*`| `test_cli_workflow_*` | Workload 2 |
| **F29** | CLI Flash Command (`flash`) | `test_cli_flash_*` | `test_cli_flash_bad_*` | All Combinations | All Workloads |
| **F30** | CLI Standalone Erase/Verify | `test_cli_erase_verify_*`| `test_cli_verify_fail_*`| `test_cli_workflow_*` | Workload 3 |
| **F31** | CLI Profiles (`profile`) | `test_cli_profile_*` | `test_cli_profile_bad_*`| `test_profile_apply_*` | Workload 1 |
| **F32** | CLI Mock Backend Flag (`--mock`)| `test_cli_mock_flag_*` | `test_cli_mock_opts_*` | All Combinations | All Workloads |
| **F33** | Deterministic Exit Codes | `test_exit_code_0_*` | `test_exit_codes_1_5_*` | All Combinations | All Workloads |

---

## 4. Test Fixtures Library Catalog

All fixtures reside in `tests/fixtures/` with deterministic contents and verified cryptographic checksums:

| File Name | Format | Size | Description & Expected Parameters |
|---|---|---|---|
| `valid_stm32_single_segment.hex` | Intel HEX | 32 B payload | STM32 Vector Table at `0x08000000`. Reset Handler `0x080001CD`. CRC32 `0x0A5B1F0D`. |
| `valid_stm32_bootloader_app_gap.hex`| Intel HEX | 48 B payload | Dual segment: 32B @ `0x08000000` + 16B @ `0x08040000`. Gap: 262,112 B. CRC32 `0x767B0A13`. |
| `valid_stm32_out_of_order.hex` | Intel HEX | 32 B payload | Same payload as single segment, records emitted out-of-order. Coalesces to CRC32 `0x0A5B1F0D`. |
| `valid_extended_segment_type02.hex` | Intel HEX | 16 B payload | Record Type 02 (`USBA = 0x1000`). Base address `0x00010000`. |
| `valid_start_segment_type03.hex` | Intel HEX | 16 B payload | Record Type 03 (`CS=0x1000, IP=0x0100`). Entry point `0x00010100`. |
| `valid_redundant_overlap.hex` | Intel HEX | 32 B payload | Two records write identical bytes to `0x08000000`. Tolerated with warning. |
| `corrupt_bad_checksum.hex` | Intel HEX | - | Record checksum byte altered (`F1` instead of `F2`). Aborts with exit code 3. |
| `corrupt_missing_colon.hex` | Intel HEX | - | Missing leading `:` character on line 1. Aborts with exit code 3. |
| `corrupt_odd_hex_digits.hex` | Intel HEX | - | Record has 13 hex characters (odd count). Aborts with exit code 3. |
| `corrupt_invalid_hex_char.hex` | Intel HEX | - | Record contains non-hex `'Z'` character. Aborts with exit code 3. |
| `corrupt_truncated.hex` | Intel HEX | - | Record truncated mid-field. Aborts with exit code 3. |
| `corrupt_conflicting_overlap.hex` | Intel HEX | - | Conflicting bytes written to same physical address. Aborts with exit code 3. |
| `extreme_high_address_type04.hex` | Intel HEX | 16 B payload | Placed at `0x08080000` (beyond 512KB chip limit). Fails target bounds check. |
| `extreme_address_overflow_4gb.hex` | Intel HEX | 16 B payload | Base `0xFFFF0000` + offset `0xFFFF` overflows 32-bit address space. |
| `empty_file.hex` | Intel HEX | 0 B | Completely empty file. Aborts with exit code 3 (`EmptyFile`). |
| `valid_stm32_cortex_m_vector.bin` | Raw Binary | 1024 B | Vector table: SP `0x20005000`, Reset `0x080001CD` (Thumb bit 1). CRC32 derived. |
| `valid_tiny_16b.bin` | Raw Binary | 16 B | Minimal 16-byte payload `[0x00..0x0F]`. |
| `valid_exact_page_256b.bin` | Raw Binary | 256 B | Exactly 1 flash page (256 bytes) repeating test pattern. |
| `valid_exact_sector_1kb.bin` | Raw Binary | 1024 B | Exactly 1 STM32F1 sector (1024 bytes) pseudo-random pattern. |
| `valid_multi_sector_16kb.bin` | Raw Binary | 16,384 B | Exactly 1 STM32F4 Sector 0 (16 KB) payload. |
| `corrupt_odd_reset_vector.bin` | Raw Binary | 1024 B | Reset handler has even address `0x080001CC` (Thumb bit 0). Rejected as invalid entry. |
| `empty_file.bin` | Raw Binary | 0 B | Zero-byte binary file. Aborts with exit code 3. |
| `valid_u575_blinky.elf` | ELF32 | 25,796 B file / 19,476 B image | Arm GCC output for STM32U575 (source: `tests/firmware/u575/`). Two `PT_LOAD` headers; `.data` has load address `0x08004C0C` in flash but virtual address `0x20000000` in RAM. Entry `0x08000040`. Non-loadable sections must be excluded. |
| `valid_u575_blinky.bin` | Raw Binary | 19,476 B | `objcopy -O binary` of the ELF above. A correct ELF parse must reproduce it byte for byte. |
| `valid_stm32f4_profile.toml` | TOML Profile| 280 B | Profile for STM32F401RE, SWD @ 2000 kHz, verify=true, reset=true. |
| `valid_stm32f1_profile.toml` | TOML Profile| 280 B | Profile for STM32F103C8, SWD @ 1000 kHz, verify=true, reset=true. |
| `invalid_malformed_profile.toml` | TOML Profile| 150 B | Syntax error in TOML. Aborts with exit code 5. |
| `missing_fields_profile.toml` | TOML Profile| 80 B | Missing mandatory `target` field. Aborts with exit code 5. |

---

## 5. Test Runner Architecture

The test suite includes a dual-mode test runner capable of executing all test tiers both in continuous integration and local development:

### 5.1 Standalone Test Runner Script (`tests/run_e2e.py`)
- Python 3 runner that requires no external dependencies.
- Can invoke both direct crate tests (`cargo test`) and compiled binary CLI commands (`flashgui-cli --mock ...`).
- Supports tier filtering: `--tier 1`, `--tier 2`, `--tier 3`, `--tier 4`, or `--all`.
- Supports test name regex filtering: `-k <filter>`.
- Generates rich console summary with per-tier pass/fail counters and total elapsed time.
- Emits standardized exit codes (0 = all pass, 1 = test failure).

### 5.2 Rust Integration Test Suite (`tests/e2e_runner.rs` & `tests/`)
- Native Rust integration tests running via standard `cargo test --test e2e_runner`.
- Direct module imports of `flash-core`, `firmware-parser`, and CLI command drivers.
- Executes full simulation against `MockProbeBackend` without requiring live hardware.

### 5.3 Execution Commands

```bash
# 1. Run all E2E test tiers via Cargo
cargo test --test e2e_runner -- --nocapture

# 2. Run specific tiers via Cargo
cargo test --test e2e_runner tier1_ -- --nocapture
cargo test --test e2e_runner tier2_ -- --nocapture
cargo test --test e2e_runner tier3_ -- --nocapture
cargo test --test e2e_runner tier4_ -- --nocapture

# 3. Run all tiers via Standalone Python Runner
python tests/run_e2e.py --all

# 4. Run specific tier via Python Runner
python tests/run_e2e.py --tier 1
python tests/run_e2e.py --tier 4
```
