# Dispatch for E2E Test Writer: Opaque-Box Test Suite & Infra

**Role**: E2E Test Suite Designer & Writer
**Working Directory**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/test_writer_e2e
**Original Request File**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md
**Project Architecture**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md
**Survey Reports**:
- `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_1/survey_report.md`
- `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2/survey_report.md`
- `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_3/survey_report.md`

## Assignment & Scope
You are responsible for designing and implementing the E2E Testing Track:
1. Create `TEST_INFRA.md` in `.agents/TEST_INFRA.md` (and copy to project root if applicable) detailing test philosophy, feature inventory coverage matrix, test runner architecture, and scenarios.
2. Design and create opaque-box E2E test cases across 4 tiers:
   - **Tier 1 - Feature Coverage (>=5 per feature)**: Isolated happy-path tests for each inventoried feature (HEX parsing, BIN parsing, probe listing, flashing, erasing, verifying, resetting, profiles).
   - **Tier 2 - Boundary & Corner Cases (>=5 per feature)**: Boundary values, corrupted checksums, extreme address ranges (e.g. 0x08000000, 0x08080000, 0xFFFFFFFF), zero-byte files, multi-megabyte payloads, memory gaps, non-monotonic addresses.
   - **Tier 3 - Cross-Feature Combinations**: Pairwise interactions (e.g., BIN load + custom base + erase + program + verify + reset; HEX with extended linear + gap + profile save + CLI execution).
   - **Tier 4 - Real-World Application Scenarios (>=5 workloads)**: Complete realistic embedded workflows (e.g., Dual-bank STM32 bootloader + application image flash; production batch programming simulation; corrupt firmware recovery).
3. Place test scripts and test fixtures under `tests/` directory (e.g. `tests/fixtures/`, test scripts/runners).
4. Publish `c:/web_applications/open-source/embedded/flash_programmer_gui/TEST_READY.md` (and in `.agents/TEST_READY.md`) when complete, providing exact commands to run the test suite and coverage summary.
5. Provide handoff report in `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/test_writer_e2e/handoff.md`.

## 2026-09-11T01:04:24Z
You are E2E Test Writer.
Working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/test_writer_e2e
Project Root: c:/web_applications/open-source/embedded/flash_programmer_gui
Read c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md, c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md, c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/test_writer_e2e/DISPATCH.md, and the survey reports in .agents/.

Design and implement the complete opaque-box E2E testing infrastructure:
1. Create TEST_INFRA.md (documenting test philosophy, 4-tier coverage methodology, runner).
2. Create test suites and test fixtures in tests/ covering Tier 1 (Feature coverage >=5/feature), Tier 2 (Boundaries/corners >=5/feature), Tier 3 (Pairwise cross-feature), Tier 4 (Realistic workloads: STM32 bootloader+app, batch programming, corrupt recovery).
3. Create test runner scripts / executable tests that can validate CLI commands and firmware flows against the mock backend.
4. Publish TEST_READY.md when complete with execution commands and coverage summary.
5. Write your handoff to c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/test_writer_e2e/handoff.md.
Send a message when finished.
