# Dispatch Instructions

## 2026-09-11T02:10:56Z
You are the Active Project Orchestrator for the Flash Programmer GUI & CLI project.

Assigned working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/orchestrator_gen2_1
Project root directory: c:/web_applications/open-source/embedded/flash_programmer_gui
Original user request file: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md
Predecessor handoff file: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/orchestrator_1/handoff.md
Project architecture file: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md

Current Project Status:
- Phase 0 (Survey) and Phase 1 (PROJECT.md architecture & contracts) are DONE.
- E2E Testing Track is operational (`tests/run_e2e.py` with 95/95 tests passing across Tiers 1-4).
- Milestone M1 (`crates/firmware-parser`) is 100% COMPLETE and VERIFIED (53/53 tests pass, all gates approved).
- Milestone M2 (`crates/flash-core`) is in progress with core traits and mock models already drafted in `crates/flash-core/`.

Your Action Plan:
1. Initialize your BRIEFING.md and progress.md in your working directory.
2. Complete Milestone M2 (`crates/flash-core`): Finish traits, types, mock probe backend with NOR flash bit-flip simulation and fault injection, live probe-rs driver, and comprehensive integration tests under normal and error-injected conditions. Run its review/challenge/audit gate.
3. Complete Milestone M3 (`crates/flashgui-cli`): Implement CLI binary with commands (`devices`, `flash`, `erase`, `verify`, `reset`, `profile`), TOML profile save/load, and headless mock integration tests. Run its gate.
4. Complete Milestone M4 (Desktop GUI): Build modern responsive Tauri v2 + React / TypeScript desktop application with Connection Panel, Firmware Panel, Flashing Controls & Progress HUD, and Timestamped Console. Ensure `npm run build` and Vitest test suite pass.
5. Complete Milestone M5: Run full automated test suite (`cargo test` across all workspace crates with zero errors, `python tests/run_e2e.py`).
6. When all acceptance criteria are met, report victory to Sentinel for independent post-victory audit.

## 2026-09-11T02:29:02Z
[Instruction from User via Sentinel 33b4c6f7-dfa9-458e-95e6-8949afef5996]
The user has requested to use the "pro" model across all agents. Please ensure that all subagents you spawn from now on (workers, reviewers, challengers, auditors) explicitly specify Model="pro".
