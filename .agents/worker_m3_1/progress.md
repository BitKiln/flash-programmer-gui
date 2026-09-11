# Progress — Milestone M3 (crates/flashgui-cli)

Last visited: 2026-09-11T08:07:00Z

## Status: IN_PROGRESS

### Completed Steps
- [x] Read DISPATCH.md, ORIGINAL_REQUEST.md, PROJECT.md, and survey_report.md § Section 4.
- [x] Inspected existing codebase (`firmware-parser`, `flash-core`, workspace layout).
- [x] Verified existing tests pass with `cargo test --workspace`.
- [x] Created `BRIEFING.md` and initialized `progress.md`.

### Next Steps
1. Add `"crates/flashgui-cli"` to root `Cargo.toml`.
2. Create `crates/flashgui-cli/Cargo.toml` with dependencies (`clap`, `serde`, `serde_json`, `toml`, `directories`, `firmware-parser`, `flash-core`, dev-deps `assert_cmd`, `predicates`, `tempfile`).
3. Implement `crates/flashgui-cli/src/error.rs` and exit code constants / error mapping.
4. Implement `crates/flashgui-cli/src/profile.rs` (TOML serialization, hierarchical resolution, CRUD operations).
5. Implement `crates/flashgui-cli/src/cli.rs` (Clap hierarchy with global flags and subcommands).
6. Implement `crates/flashgui-cli/src/commands/`:
   - `devices.rs`
   - `flash.rs`
   - `erase.rs`
   - `verify.rs`
   - `reset.rs`
   - `profile.rs`
7. Implement `crates/flashgui-cli/src/main.rs`.
8. Implement integration tests in `crates/flashgui-cli/tests/cli_mock_tests.rs`.
9. Verify with `cargo test -p flashgui-cli`, `cargo test --workspace`, and `cargo clippy`.
10. Write `handoff.md` and notify parent.
