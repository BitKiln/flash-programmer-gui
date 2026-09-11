# BRIEFING — 2026-09-11T01:31:00Z

## Mission
Implement `crates/flash-core` providing traits, live probe-rs and virtual mock probe backends, memory physics, profiles, fault injection, progress events, and tests.

## 🔒 My Identity
- Archetype: worker
- Roles: implementer, qa, specialist
- Working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2
- Original parent: 1e3c0803-34e2-4843-8182-dc12e429430f
- Milestone: M2 (Flash Core & Probe Abstraction)

## 🔒 Key Constraints
- DO NOT CHEAT: Genuine logic only, no hardcoded verification strings or dummy facades.
- Own only `Cargo.toml` (members update) and `crates/flash-core/**`.
- Follow exact trait signatures and interfaces specified in DISPATCH.md and PROJECT.md.
- Run tests and clippy (`cargo test -p flash-core`, `cargo clippy -p flash-core --all-targets -- -D warnings`).
- Handoff report to `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2/handoff.md`.

## Current Parent
- Conversation ID: 1e3c0803-34e2-4843-8182-dc12e429430f
- Updated: 2026-09-11T01:31:00Z

## Task Summary
- **What to build**: Full `crates/flash-core` crate including traits, error handling, types, progress reporting, mock NOR flash simulation, profiles, fault injection, mock and live backend implementations, FlashManager pipeline, unit and integration tests.
- **Success criteria**: All integration and unit tests pass with zero warnings in clippy.
- **Interface contracts**: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md`
- **Code layout**: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md § Code Layout`

## Key Decisions Made
- Use feature flags: default = ["mock-probe"], mock-probe = [], live-probe = ["dep:probe-rs"].
- Depend on `firmware-parser` for `MemorySegment`.
- Match the exact signatures specified in DISPATCH.md: `erase_all(&mut self, cb: Option<&ProgressCallback>) -> Result<(), FlashError>`, `erase_range(&mut self, start: u32, length: u32, cb: Option<&ProgressCallback>) -> Result<(), FlashError>`, `program(&mut self, segments: &[MemorySegment], options: &ProgramOptions, cb: Option<&ProgressCallback>) -> Result<(), FlashError>`, `verify(&mut self, segments: &[MemorySegment], cb: Option<&ProgressCallback>) -> Result<VerifyReport, FlashError>`, `read_memory(&mut self, address: u32, length: u32) -> Result<Vec<u8>, FlashError>`, `reset(&mut self, halt: bool) -> Result<(), FlashError>`, `close(&mut self) -> Result<(), FlashError>`.

## Change Tracker
- **Files modified**: None yet
- **Build status**: Not built yet
- **Pending issues**: Initial implementation pending

## Quality Status
- **Build/test result**: Pending
- **Lint status**: Pending
- **Tests added/modified**: Pending

## Loaded Skills
- None loaded.

## Artifact Index
- `.agents/worker_m2/progress.md` — Liveness and task progress tracking
- `.agents/worker_m2/handoff.md` — Final completion handoff report
