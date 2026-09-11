# Task Assignment: Reviewer 2 for Milestone M2 (`crates/flash-core`)

## Objective
Perform independent adversarial code review, error resilience check, concurrency/safety analysis, and test verification of `crates/flash-core`.

## Context & Inputs
- Working directory: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/reviewer_m2_2`
- ORIGINAL_REQUEST.md: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md`
- PROJECT.md: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md`
- Worker handoff: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2_gen2/handoff.md`
- Crate under review: `crates/flash-core`

## Verification Instructions
1. Run `cargo test -p flash-core` and verify all tests pass.
2. Run `cargo test -p flash-core --all-features` and verify `probe-rs` live backend compilation and test pass.
3. If `tests/run_e2e.py` exists, run it via python or cargo test across workspace (`cargo test --workspace`) to ensure zero regressions on `firmware-parser`.
4. Inspect deterministic fault injection in `crates/flash-core/src/mock/fault.rs` and `tests/fault_injection.rs`. Ensure errors (`ConnectionLost`, `WriteProtected`, `ProgrammingFailed`, `VerificationFailed`) properly propagate and do not panic or deadlock.
5. Inspect `FlashManager::execute_flash` pipeline for edge cases (zero-length segments, multiple disjoint segments, chunk boundary conditions, cancellation).
6. Issue verdict: `APPROVE` or `REQUEST_CHANGES` with detailed evidence.
7. Write `handoff.md` in your working directory and notify parent via `send_message`.

## 2026-09-11T02:31:02Z
You are Reviewer 2 (teamwork_preview_reviewer) for Milestone M2 (crates/flash-core).
Read DISPATCH.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/reviewer_m2_2/DISPATCH.md.
Read ORIGINAL_REQUEST.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md.
Read PROJECT.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md.
Read Worker handoff at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2_gen2/handoff.md.
Assigned working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/reviewer_m2_2.
Perform error resilience review, workspace regression testing, and verification.
Write handoff.md with verdict (APPROVE or REQUEST_CHANGES) and notify parent via send_message.
