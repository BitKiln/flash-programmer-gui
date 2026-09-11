# Task Assignment: Reviewer 1 for Milestone M2 (`crates/flash-core`)

## Objective
Perform independent code review, completeness analysis, robustness check, and test verification of `crates/flash-core`.

## Context & Inputs
- Working directory: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/reviewer_m2_1`
- ORIGINAL_REQUEST.md: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md`
- PROJECT.md: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md`
- Worker handoff: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2_gen2/handoff.md`
- Crate under review: `crates/flash-core`

## Verification Instructions
1. Run `cargo test -p flash-core` and record verbatim outputs.
2. Run `cargo test -p flash-core --all-features` and record verbatim outputs.
3. Run `cargo clippy -p flash-core --all-targets --all-features -- -D warnings`.
4. Inspect `crates/flash-core/src/traits.rs`, `src/mock/backend.rs`, `src/mock/memory.rs`, `src/mock/fault.rs`, `src/manager.rs`, `src/live/probe_rs_backend.rs`.
5. Verify interface conformance against `PROJECT.md § Interface Contracts (Contract 2: flash-core -> Consumers)`.
6. Verify genuine physical NOR flash bit-clearing simulation (`0xFF` default, `1 -> 0` write limits, `NorFlashWriteViolation` on invalid write).
7. Issue verdict: `APPROVE` or `REQUEST_CHANGES` with detailed evidence.
8. Write `handoff.md` in your working directory and notify parent via `send_message`.

## 2026-09-11T02:31:02Z
You are Reviewer 1 (teamwork_preview_reviewer) for Milestone M2 (crates/flash-core).
Read DISPATCH.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/reviewer_m2_1/DISPATCH.md.
Read ORIGINAL_REQUEST.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md.
Read PROJECT.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md.
Read Worker handoff at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2_gen2/handoff.md.
Assigned working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/reviewer_m2_1.
Perform code review, test runs, and interface verification.
Write handoff.md with verdict (APPROVE or REQUEST_CHANGES) and notify parent via send_message.
