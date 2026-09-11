# Task Assignment: Milestone M3 (`crates/flashgui-cli`)

## Objective
Implement and verify `crates/flashgui-cli`, the headless CLI companion binary for probe discovery, flashing, erasing, verifying, resetting, and TOML profile management. Add `crates/flashgui-cli` to root `Cargo.toml` workspace members.

## Mandatory Integrity Warning
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A teamwork_preview_auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

## Context & Inputs
- Working directory: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m3_1`
- ORIGINAL_REQUEST.md: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md`
- PROJECT.md: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md`
- Survey Architecture: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_3/survey_report.md § Section 4`
- Existing crates:
  - `crates/firmware-parser` (provides `parse_hex`, `parse_bin`, `MemorySegment`, `FirmwareImage`)
  - `crates/flash-core` (provides `FlashBackend`, `FlashSession`, `MockProbeBackend`, `ProbeRsLiveBackend`, `FlashManager`, `ConnectionConfig`, `ProgramOptions`, `FlashEvent`, `FlashError`)

## Scope of Implementation
1. Root `Cargo.toml`:
   - Add `"crates/flashgui-cli"` to `[workspace.members]`.
2. `crates/flashgui-cli/Cargo.toml`:
   - Dependencies: `clap` (v4.5 with `derive`), `serde`, `serde_json`, `toml` (v0.8), `firmware-parser` (path), `flash-core` (path), `directories` (v5.0 or fallback), `colored` (optional or simple ANSI).
3. `src/cli.rs`:
   - Define `Cli` with global flags: `--mock` (bool), `--quiet` (bool), `--json` (bool), `--profile-file` (Option<String>).
   - Subcommands:
     - `Devices`: list connected probes.
     - `Flash`: flash firmware file (`.hex` or `.bin`), with `--target`, `--probe`, `--interface`, `--speed`, `--base-address`, `--verify`, `--reset`, `--full-erase`, `--profile`.
     - `Erase`: erase target flash (`--target`, `--probe`, `--full`).
     - `Verify`: verify target memory against firmware file (`--target`, `--probe`, `--base-address`).
     - `Reset`: reset target (`--target`, `--probe`, `--halt`).
     - `Profile`: subcommands `Save`, `Show`, `List`, `Delete`.
4. `src/profile.rs`:
   - TOML schema for profiles (`FlashProfile`: name, description, target, interface, speed_khz, probe_id, firmware default_path & base_address, options verify_after, reset_after, full_chip_erase).
   - Functions for loading, saving, listing, deleting profiles from project/user directories or custom `--profile-file`.
5. `src/commands/`:
   - Handlers for each subcommand integrating with `flash-core` (`MockProbeBackend` if `--mock`, or live backend) and `firmware-parser`.
6. Exit Codes (strictly enforced per Section 4.3):
   - `0`: Success
   - `1`: Flash/Verify Error
   - `2`: Target Connection Error
   - `3`: Firmware Parse Error
   - `4`: Probe Not Found
   - `5`: Invalid Arguments or Profile Error
7. Integration Tests in `crates/flashgui-cli/tests/cli_mock_tests.rs`:
   - Automated tests invoking CLI logic or `assert_cmd` / direct subcommands in `--mock` mode:
     - `devices` command (text & json output)
     - `flash` command with `.hex` and `.bin` files
     - `erase` command (full & sector)
     - `verify` command
     - `reset` command (normal & halt)
     - `profile save`, `show`, `list`, `delete`, and flashing via `--profile`
     - Error conditions returning proper exit codes (e.g. invalid file format -> exit code 3, probe not found -> exit code 4).
8. Verification:
   - Run `cargo test -p flashgui-cli` and `cargo test --workspace`.
   - Run `cargo clippy -p flashgui-cli --all-targets -- -D warnings`.
   - Write `handoff.md` in your working directory and notify parent via `send_message`.
