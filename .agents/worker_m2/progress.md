# Progress — Worker M2: Flash Core & Probe Abstraction

Last visited: 2026-09-11T01:31:05Z

## Current Status
Starting implementation of `crates/flash-core`.

## Checklist
- [x] Read DISPATCH.md, ORIGINAL_REQUEST.md, PROJECT.md, survey_report.md
- [x] Create BRIEFING.md and progress.md
- [ ] Update root `Cargo.toml` to add `"crates/flash-core"`
- [ ] Create `crates/flash-core/Cargo.toml`
- [ ] Implement `src/error.rs`
- [ ] Implement `src/types.rs`
- [ ] Implement `src/progress.rs`
- [ ] Implement `src/traits.rs`
- [ ] Implement `src/mock/memory.rs` (NOR flash physics, bit clearing 1->0, 0xFF erased)
- [ ] Implement `src/mock/profiles.rs` (STM32 geometries: F1, F4, Generic)
- [ ] Implement `src/mock/fault.rs` (Deterministic FaultInjector)
- [ ] Implement `src/mock/backend.rs` (MockProbeBackend, MockFlashSession)
- [ ] Implement `src/mock/mod.rs`
- [ ] Implement `src/live/probe_rs_backend.rs` & `src/live/mod.rs` (probe-rs 0.32 integration)
- [ ] Implement `src/manager.rs` (FlashManager::execute_flash pipeline)
- [ ] Implement `src/lib.rs` (prelude, re-exports, factory)
- [ ] Implement `tests/mock_integration.rs`
- [ ] Implement `tests/fault_injection.rs`
- [ ] Run `cargo test -p flash-core` and verify all tests pass
- [ ] Run `cargo clippy -p flash-core --all-targets -- -D warnings`
- [ ] Write handoff report `handoff.md`
- [ ] Send completion message to parent
