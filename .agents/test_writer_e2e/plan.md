# E2E Test Suite Execution Plan

## Objective
Design and implement the complete opaque-box E2E testing infrastructure for the Flash Programmer GUI & CLI.

## Tasks
1. **Design & Document TEST_INFRA.md**:
   - Test philosophy (opaque-box, specification-derived expected outputs, zero flaky state).
   - 4-Tier coverage methodology:
     - Tier 1: Feature Coverage (>=5 isolated happy paths per feature).
     - Tier 2: Boundary & Corner Cases (>=5 boundary/malformed/edge cases per feature).
     - Tier 3: Pairwise Cross-Feature Interactions.
     - Tier 4: Realistic Real-World Workloads (dual-bank STM32 bootloader+app, batch flashing, corrupt firmware recovery).
   - Test runner architecture (multi-mode runner supporting both CLI binaries and direct crate tests).
   - Feature inventory coverage matrix mapping R1-R4 / F01-F40 to test suites.
   - Place in `TEST_INFRA.md` (project root) and `.agents/TEST_INFRA.md`.

2. **Create Test Fixtures Library (`tests/fixtures/`)**:
   - Intel HEX fixtures:
     - `valid_stm32_single_segment.hex` (from Vector 1 in survey)
     - `valid_stm32_bootloader_app_gap.hex` (from Vector 2 in survey)
     - `valid_stm32_out_of_order.hex` (from Vector 3 in survey)
     - `corrupt_bad_checksum.hex`
     - `corrupt_missing_colon.hex`
     - `corrupt_odd_hex_digits.hex`
     - `corrupt_invalid_hex_char.hex`
     - `corrupt_truncated.hex`
     - `corrupt_conflicting_overlap.hex`
     - `extreme_high_address_type04.hex` (0x08080000 limit boundary)
     - `extreme_address_overflow_4gb.hex` (0xFFFFFFFF overflow)
     - `empty_file.hex`
     - `valid_redundant_overlap.hex`
     - `valid_extended_segment_type02.hex` (HEX86 segmented)
     - `valid_start_segment_type03.hex` (HEX86 CS:IP)
   - Binary fixtures:
     - `valid_stm32_cortex_m_vector.bin` (vector table at 0x08000000)
     - `valid_tiny_16b.bin`
     - `valid_exact_page_256b.bin`
     - `valid_exact_sector_1kb.bin`
     - `valid_multi_sector_16kb.bin`
     - `corrupt_odd_reset_vector.bin`
     - `empty_file.bin`
   - Profile fixtures:
     - `valid_stm32f4_profile.toml`
     - `valid_stm32f1_profile.toml`
     - `invalid_malformed_profile.toml`
     - `missing_fields_profile.toml`

3. **Implement Test Suites Across 4 Tiers (`tests/`)**:
   - `tests/tier1_features/`:
     - `hex_parsing_test.rs` (F01-F06, F11, F14: >=5 tests)
     - `bin_parsing_test.rs` (F07, F11, F12: >=5 tests)
     - `probe_listing_test.rs` (F16, F28: >=5 tests)
     - `flashing_test.rs` (F19, F23, F27, F29: >=5 tests)
     - `erasing_test.rs` (F18, F23, F30: >=5 tests)
     - `verifying_test.rs` (F20, F23, F30: >=5 tests)
     - `resetting_test.rs` (F21, F23, F30: >=5 tests)
     - `profile_test.rs` (F31: >=5 tests)
   - `tests/tier2_boundaries/`:
     - `hex_boundary_test.rs` (boundary values, checksum corruptions, truncated, odd lengths: >=5 tests)
     - `bin_boundary_test.rs` (0-byte, huge payloads, out-of-bounds base address: >=5 tests)
     - `flash_boundary_test.rs` (sector boundary alignment, NOR flash 1->0 write violations, chip bounds: >=5 tests)
     - `gap_overlap_test.rs` (sparse gaps, redundant overlaps, conflicting overlaps: >=5 tests)
     - `fault_injection_test.rs` (disconnect, erase failure, program timeout, verification glitch: >=5 tests)
   - `tests/tier3_combinations/`:
     - `bin_custom_base_flow_test.rs` (BIN load + custom base + full erase + program + verify + reset)
     - `hex_extended_linear_gap_flow_test.rs` (HEX with Type 04 + sparse gap + verify + profile save + CLI execution)
     - `profile_cli_workflow_test.rs` (Save profile -> list -> load -> execute flash via CLI)
     - `fault_retry_recovery_test.rs` (Fault injection during flash -> error handling -> recovery flash)
   - `tests/tier4_workloads/`:
     - `workload_stm32_bootloader_app.rs` (Dual-image bootloader at 0x08000000 + main app at 0x0800C000 with gap, verify both, reset)
     - `workload_batch_programming.rs` (Simulate production factory line: flash 10 devices consecutively, verify zero cross-contamination)
     - `workload_corrupt_firmware_recovery.rs` (Corrupt flash on device -> detect verification failure -> mass erase -> reflash known-good firmware)
     - `workload_dual_bank_update.rs` (Simulate A/B dual-bank firmware update and swap)
     - `workload_large_image_stress.rs` (Flash and verify multi-sector 256KB image with progress telemetry)

4. **Implement Test Runner (`tests/runner/` / `tests/e2e_runner.rs` / `run_e2e_tests.py` or `.ps1`)**:
   - Automated runner executing all 4 tiers.
   - Produces structured pass/fail results, latency metrics, and coverage summary.
   - Supports filtering by tier (`--tier 1`, `--tier 2`, etc.) and format (`--json`, `--summary`).

5. **Verify and Execute Tests**:
   - Run compilation and tests.
   - Validate exit codes and outputs.

6. **Publish TEST_READY.md and Handoff**:
   - Document commands to run the test suite.
   - Summarize coverage across Tiers 1-4 and feature inventory.
   - Output `TEST_READY.md` in project root and `.agents/TEST_READY.md`.
   - Submit handoff report to `.agents/test_writer_e2e/handoff.md`.
