# Task Assignment: Challenger 1 for Milestone M2 (`crates/flash-core`)

## Objective
Empirically verify correctness, NOR flash bit-level physics, and fault resilience of `crates/flash-core` through code execution, stress tests, or edge-case harnesses.

## Context & Inputs
- Working directory: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/challenger_m2_1`
- ORIGINAL_REQUEST.md: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md`
- PROJECT.md: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md`
- Worker handoff: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2_gen2/handoff.md`
- Target crate: `crates/flash-core`

## Verification Instructions
1. Run existing test suite: `cargo test -p flash-core`.
2. Adversarially challenge the mock NOR flash implementation:
   - Verify that an un-erased byte (e.g. 0xFE = 0b11111110) cannot be programmed to 0xFF (0b11111111) without sector erase, returning `NorFlashWriteViolation`.
   - Verify that bit-clearing `0xFF -> 0xAA -> 0x88 -> 0x00` succeeds without erase.
   - Verify sector-aligned vs non-sector-aligned erase ranges on STM32F4 asymmetric sectors.
   - Test out-of-bounds addresses beyond flash base + size.
3. Issue verdict: `APPROVE` or `REQUEST_CHANGES` with empirical test outputs and code snippets.
4. Write `handoff.md` in your working directory and notify parent via `send_message`.

## 2026-09-11T02:31:02Z
You are Challenger 1 (teamwork_preview_challenger) for Milestone M2 (crates/flash-core).
Read DISPATCH.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/challenger_m2_1/DISPATCH.md.
Read ORIGINAL_REQUEST.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md.
Read PROJECT.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md.
Read Worker handoff at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2_gen2/handoff.md.
Assigned working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/challenger_m2_1.
Adversarially challenge NOR flash bit-level physics, sector erase ranges, and out-of-bounds protection.
Write handoff.md with verdict (APPROVE or REQUEST_CHANGES) and notify parent via send_message.
