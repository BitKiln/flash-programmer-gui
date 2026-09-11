# Dispatch for Explorer 1: Core Flash Architecture & Probe Abstraction Spec

**Role**: Core Systems Explorer
**Working Directory**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_1
**Original Request File**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md

## Assignment
1. Read `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md`.
2. Analyze requirement R1 (`flash-core`) and related acceptance criteria.
3. Investigate the Rust workspace layout, `FlashBackend` trait architecture, probe discovery mechanisms, connection parameters (target chip e.g. STM32, protocol SWD/JTAG, speed/frequency), sector erase, page/block programming, memory verification, system reset, live `probe-rs` backend (ST-Link, CMSIS-DAP), and in-memory Virtual/Mock probe backend (with error injection, state simulation, and headless execution).
4. Document all required features, interface contracts, error types, dependencies (e.g. `probe-rs`, `thiserror`, `anyhow`, etc.), and verification strategies.


## 2026-09-10T19:26:14Z

You are Explorer 1: Core Flash Architecture & Probe Abstraction Spec.
Your assigned working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_1
Read c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md and c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_1/DISPATCH.md.
Investigate requirement R1 (flash-core) and acceptance criteria:
- FlashBackend trait design (discovery, connect, erase, program, verify, reset)
- Live backend (probe-rs supporting ST-Link, CMSIS-DAP)
- Virtual/Mock Probe backend (in-memory, configurable flash size, sector size, delay simulation, error injection, state simulation)
- Rust workspace layout, dependencies, error models, and progress callback / event contracts
- Testing strategy for unit and integration testing via cargo test.
Write your full findings to c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_1/survey_report.md and your handoff to c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_1/handoff.md.
Send a message when finished.
