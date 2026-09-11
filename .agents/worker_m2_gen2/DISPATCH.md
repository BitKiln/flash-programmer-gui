# Task Assignment: Complete Milestone M2 (`crates/flash-core`)

## Objective
Implement and verify `crates/flash-core` providing the unified probe abstraction layer (`FlashBackend`, `FlashSession`), in-memory Virtual/Mock Probe backend with NOR flash physics (`0xFF` erased, `1 -> 0` write limits) and deterministic fault injection, live `probe-rs` backend (feature-gated under `live-probe`), `FlashManager` execution pipeline, and comprehensive integration tests in `crates/flash-core/tests/`.

## Mandatory Integrity Warning
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A teamwork_preview_auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

## Context and Inputs
- Working directory: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2_gen2`
- User Request: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md`
- Project Blueprint: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md`
- Survey Architecture: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_1/survey_report.md`
- Existing drafts in `crates/flash-core`:
  - `Cargo.toml`
  - `src/traits.rs`
  - `src/types.rs`
  - `src/error.rs`
  - `src/progress.rs`
  - `src/mock/memory.rs`
  - `src/mock/fault.rs`
  - `src/mock/profiles.rs`

## Scope of Implementation
1. `src/mock/backend.rs` & `src/mock/mod.rs`:
   - `MockProbeBackend` implementing `FlashBackend` (enumerates simulated probes e.g. ST-Link STM32F103, CMSIS-DAP STM32F401, J-Link Generic; factory creates `MockFlashSession`).
   - `MockFlashSession` implementing `FlashSession` (wrapping `MockFlashMemory`, `FaultInjector`, `TargetInfo`, handling `erase_all`, `erase_range`, `program`, `verify`, `read_memory`, `reset`, `close`, emitting progress updates via `ProgressCallback`).
2. `src/live/probe_rs_backend.rs` & `src/live/mod.rs`:
   - Under `#[cfg(feature = "live-probe")]`, implement `ProbeRsLiveBackend` and `ProbeRsLiveSession` wrapping `probe-rs::probe::Probe` and `probe-rs::Session`.
   - Ensure clean compilation with both `--features mock-probe` and `--all-features` or default.
3. `src/manager.rs`:
   - `FlashManager::execute_flash(session: &mut dyn FlashSession, firmware: &FirmwareImage, options: &ProgramOptions, cb: Option<&dyn ProgressCallback>) -> Result<FlashResult, FlashError>`.
   - Pipeline: validates segments against target if known -> chip erase or sector erase range -> program segments -> verify segments if `verify_after` -> reset target if `reset_after`.
4. `src/lib.rs`:
   - Re-export traits, types, error, progress, manager, mock, live.
5. Integration Tests:
   - `crates/flash-core/tests/mock_integration.rs`: Full lifecycle testing with STM32F1 and STM32F4 profiles: probe listing, session connection, mass erase, sector erase, single & multi-segment programming, byte verification, reset, and memory read-back.
   - `crates/flash-core/tests/fault_injection.rs`: Test deterministic faults using `FaultInjector`:
     - Connection loss simulation (`ConnectionLost`)
     - Memory write protection / permission denied (`WriteProtected`)
     - Programming failure midway (`ProgrammingFailed`)
     - Verification mismatch / byte corruption (`VerificationFailed`)
     - NOR bit-clearing violation (attempting to turn `0 -> 1` without erase)
6. Run `cargo test -p flash-core` and `cargo clippy -p flash-core -- -D warnings`. Ensure 100% pass rate with zero warnings.
7. Write `handoff.md` in your working directory and notify the parent orchestrator via `send_message`.

## 2026-09-11T07:42:22Z
You are Worker M2 (teamwork_preview_worker) assigned to complete Milestone M2 (`crates/flash-core`).
Read your task assignment at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2_gen2/DISPATCH.md.
Read ORIGINAL_REQUEST.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md.
Read PROJECT.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md.
Read the survey report at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_1/survey_report.md.
Assigned working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2_gen2.
Maintain progress in progress.md in your working directory.
When complete, write handoff.md in your working directory and notify the parent via send_message.

## 2026-09-11T02:26:25Z
**Context**: Milestone M2 (crates/flash-core) implementation
**Content**: Checking in on progress. Please provide an update on tests and handoff status.
**Action**: Continue implementation, update your progress.md, and send handoff report when complete.


