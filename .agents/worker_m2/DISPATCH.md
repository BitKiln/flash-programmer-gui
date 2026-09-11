# Dispatch for Worker M2: Flash Core & Probe Abstraction

**Role**: Flash Core Systems Worker
**Working Directory**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2
**Original Request File**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md
**Project Architecture**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md
**Specification Source**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_1/survey_report.md
**Dependency Crate**: `crates/firmware-parser` (already implemented and verified)

## Mandatory Integrity Warning
> DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

## Exclusive Write Ownership
You own:
- `Cargo.toml` (update workspace `members` to include `"crates/flash-core"`)
- `crates/flash-core/**`

## Requirements
Implement the complete `flash-core` crate:
1. `crates/flash-core/Cargo.toml`:
   - Feature flags: `default = ["mock-probe"]`, `mock-probe = []`, `live-probe = ["dep:probe-rs"]`.
   - Dependencies: `thiserror = "1.0"`, `serde = { version = "1.0", features = ["derive"] }`, `serde_json = "1.0"`, `firmware-parser = { path = "../firmware-parser" }`, optional `probe-rs = { version = "0.32", optional = true }`.
2. `src/traits.rs`:
   - `FlashBackend`: `name(&self) -> &'static str`, `list_probes(&self) -> Result<Vec<ProbeInfo>, FlashError>`, `open_session(&self, config: &ConnectionConfig) -> Result<Box<dyn FlashSession>, FlashError>`.
   - `FlashSession`: `erase_all(&mut self, cb: Option<&ProgressCallback>) -> Result<(), FlashError>`, `erase_range(&mut self, start: u32, length: u32, cb: Option<&ProgressCallback>) -> Result<(), FlashError>`, `program(&mut self, segments: &[MemorySegment], options: &ProgramOptions, cb: Option<&ProgressCallback>) -> Result<(), FlashError>`, `verify(&mut self, segments: &[MemorySegment], cb: Option<&ProgressCallback>) -> Result<VerifyReport, FlashError>`, `read_memory(&mut self, address: u32, length: u32) -> Result<Vec<u8>, FlashError>`, `reset(&mut self, halt: bool) -> Result<(), FlashError>`, `close(&mut self) -> Result<(), FlashError>`.
3. `src/types.rs`:
   - `ProbeInfo`, `TargetInfo`, `ConnectionConfig`, `ProgramOptions`, `VerifyReport`, `VerifyMismatch`, `WireProtocol` (SWD, JTAG), `ResetType`.
4. `src/error.rs`:
   - `FlashError` enum with `thiserror` (ProbeNotFound, ConnectError, EraseError, ProgramError, VerifyError, FaultInjected, etc.).
5. `src/progress.rs`:
   - `FlashEvent`, `FlashStage` (Connecting, Erasing, Programming, Verifying, Resetting, Completed, Failed), `ProgressMetrics` (bytes transferred, total bytes, percentage, speed bps, elapsed time), `ProgressCallback` trait / closure wrapper.
6. `src/mock/`:
   - `memory.rs`: `MockFlashMemory` simulating physical NOR flash behavior:
     - Memory initialized to `0xFF` when erased.
     - Writing can only change bits from `1 -> 0` (`byte & new_byte == new_byte`). Writing a 1 over a 0 triggers `FlashError::NorFlashWriteViolation`.
     - Sector erase resets sector bytes to `0xFF`.
   - `profiles.rs`: Flash geometries: STM32F1 (uniform 1KB sectors) and STM32F4 (16KB, 64KB, 128KB asymmetric sectors).
   - `fault.rs`: `FaultInjector` supporting injection of connection drops, write-protection locks, programming timeouts, and verification corruptions.
   - `backend.rs`: `MockProbeBackend` and `MockFlashSession` implementing `FlashBackend` and `FlashSession`.
7. `src/live/`:
   - `probe_rs_backend.rs`: Implementation of `FlashBackend` and `FlashSession` using `probe-rs` v0.32 under `#[cfg(feature = "live-probe")]`.
8. `src/manager.rs`:
   - `FlashManager::execute_flash` orchestrating the high-level erase -> program -> verify -> reset pipeline with progress callback forwarding.
9. `src/lib.rs`:
   - Re-exports and factory helpers (`create_default_backend()`).
10. `tests/`:
    - `mock_integration.rs`: Full integration test suite using Mock backend verifying probe detection, sector erase, buffer programming, byte verification, and reset cycles.
    - `fault_injection.rs`: Tests verifying error handling under injected faults (disconnect, write-protection, verify mismatch).

## Verification & Handoff
Execute:
- `cargo test -p flash-core`
- `cargo clippy -p flash-core --all-targets -- -D warnings`
Document full outputs and test counts in `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2/handoff.md`.

## 2026-09-11T01:30:24Z
Received worker dispatch for Worker M2: Flash Core & Probe Abstraction.
Scope:
Update root Cargo.toml to add "crates/flash-core".
Implement crates/flash-core per DISPATCH.md:
- traits.rs (FlashBackend, FlashSession)
- types.rs (ProbeInfo, TargetInfo, ConnectionConfig, ProgramOptions, VerifyReport, etc.)
- error.rs (FlashError)
- progress.rs (FlashEvent, FlashStage, ProgressMetrics, ProgressCallback)
- mock/ (memory.rs with NOR flash 0xFF/1->0 physics, profiles.rs STM32 geometries, fault.rs FaultInjector, backend.rs MockProbeBackend and MockFlashSession)
- live/ (probe_rs_backend.rs with probe-rs 0.32 under feature "live-probe")
- manager.rs (FlashManager::execute_flash)
- lib.rs
- tests/mock_integration.rs and tests/fault_injection.rs
Run cargo test -p flash-core and cargo clippy -p flash-core --all-targets -- -D warnings.
Deliver completion handoff to c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2/handoff.md.
Send a message when finished.
