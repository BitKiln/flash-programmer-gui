# BRIEFING — 2026-09-11T08:07:00Z

## Mission
Implement and verify `crates/flashgui-cli` headless CLI companion binary and integration tests.

## 🔒 My Identity
- Archetype: teamwork_preview_worker
- Roles: implementer, qa, specialist
- Working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m3_1
- Original parent: 6389518a-8c2b-438a-b9a2-a7d89f854b8a
- Milestone: M3 (crates/flashgui-cli)

## 🔒 Key Constraints
- Genuine implementation: DO NOT cheat, hardcode test results, or create dummy facades.
- Exit codes strictly enforced per Section 4.3 (0: Success, 1: Flash/Verify, 2: Target Connection, 3: Firmware Parse, 4: Probe Not Found, 5: Invalid Args/Profile).
- Support global flags: `--mock`, `--quiet`, `--json`, `--profile-file`.
- Subcommands: `devices`, `flash`, `erase`, `verify`, `reset`, `profile` (save, show, list, delete).
- TOML profile schema and hierarchical directory resolution.
- Pass `cargo test -p flashgui-cli`, `cargo test --workspace`, `cargo clippy -p flashgui-cli --all-targets -- -D warnings`.

## Current Parent
- Conversation ID: 6389518a-8c2b-438a-b9a2-a7d89f854b8a
- Updated: not yet

## Task Summary
- **What to build**: `crates/flashgui-cli` binary with Clap 4 CLI, profile manager, command handlers, and test suite.
- **Success criteria**: All commands and exit codes work in live and mock mode; 100% tests pass; clippy clean.
- **Interface contracts**: `PROJECT.md` and `survey_report.md § Section 4`.
- **Code layout**: `PROJECT.md § Code Layout`.

## Key Decisions Made
- Use Clap 4 derive for CLI hierarchy.
- Use `directories` crate for cross-platform profile resolution, with fallback to local `./.flashgui/profiles`.
- Re-use `firmware-parser` and `flash-core` APIs cleanly without duplicating logic.

## Artifact Index
- `.agents/worker_m3_1/DISPATCH.md` — task dispatch
- `.agents/worker_m3_1/BRIEFING.md` — situational awareness
- `.agents/worker_m3_1/progress.md` — liveness heartbeat

## Change Tracker
- **Files modified**: None yet
- **Build status**: Clean (firmware-parser & flash-core tests pass)
- **Pending issues**: None

## Quality Status
- **Build/test result**: Pass
- **Lint status**: Clean
- **Tests added/modified**: Pending

## Loaded Skills
- None
