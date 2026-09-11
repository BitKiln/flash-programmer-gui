# Handoff Report: GUI, CLI & Integration Architecture Spec (Explorer 3)

## 1. Observation
- **Original Request Reference**:
  - `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md`:
    - Requirement R3 (lines 18-23): Desktop Application GUI (Tauri + React / TypeScript) with Connection Panel (live probe refresh, chip selection, SWD/JTAG, speed), Firmware Panel (drag & drop, segment viewer, recent files, flash options), Controls & animated progress bar (bytes, percentage, speed, elapsed time), and Timestamped Console.
    - Requirement R4 (lines 25-27): CLI Companion (`flashgui-cli`) sharing `flash-core`, headless probe discovery, flashing, verification in CI scripts, reusable TOML/JSON configuration profiles.
    - Acceptance Criteria (lines 35-42): CLI commands (`devices`, `flash`, `erase`, `verify`, `reset`, `profile`) returning proper status codes, CLI headless testing against mock backend, frontend build check (`npm run build`), and Vitest state transitions test suite.
- **Environment & Toolchain Survey**:
  - `rustc --version` -> `rustc 1.97.0 (2d8144b78 2026-07-07)`
  - `cargo --version` -> `cargo 1.97.0 (c980f4866 2026-06-30)`
  - `node --version` -> `v22.14.0`
  - `npm --version` -> `10.9.2`
  - `cargo tauri --version` -> `tauri-cli 2.11.4`
- **Codebase State**: Greenfield workspace, no pre-existing source files; `.agents/` contains survey dispatches.
- **Peer Agent Inputs**:
  - `explorer_survey_1`: Designing `FlashBackend` trait, probe-rs 0.32 integration, and Virtual/Mock probe with NOR flash simulation and error injection.
  - `spec_miner_survey_2`: Designing `firmware-parser` for Intel HEX (00-05) and raw BIN, segment extraction, CRC32/SHA256, and entry point detection.

## 2. Logic Chain
- **Step 1 (Toolchain Selection)**: From the presence of `tauri-cli 2.11.4`, `rustc 1.97.0`, and `node v22.14.0`, Tauri v2 is the native desktop shell for this system. Tauri v2 provides minimal binary footprint, fine-grained capability security, and zero-cost Rust async IPC.
- **Step 2 (Telemetry & Console Streaming)**: Requirement R3 mandates live auto-refresh, high-precision flashing metrics (speed, bytes, ETA, elapsed time), and timestamped console streaming. Running these as polling IPC calls would cause UI stutter and high IPC overhead. Therefore, high-frequency progress and log streams are separated from request/response commands and routed via Tauri v2's event emitter (`app_handle.emit("flash:progress", payload)` and `app_handle.emit("flash:log", payload)`).
- **Step 3 (Cooperative Operation Cancellation)**: Flashing operations can take seconds or minutes. To satisfy user responsiveness, long-running operations are spawned on Tokio async background tasks monitored via a `CancellationToken`. An IPC `cancel_operation` command trips this token, safely halting memory operations without hanging or corrupting the session.
- **Step 4 (Frontend State Machine)**: The UI requires distinct views and button states across connection and flashing stages. Modeling frontend state via a Zustand store with an explicit lifecycle state machine (`idle` -> `detecting` -> `connecting` -> `connected` -> `erasing` -> `programming` -> `verifying` -> `resetting` -> `completed` / `error`) prevents invalid state transitions (such as triggering program while disconnected).
- **Step 5 (Headless CLI Automation & Exit Codes)**: Requirement R4 requires automated CI scripts to flash and verify targets without physical hardware. By introducing a global `--mock` flag that switches `flash-core` to the in-memory Virtual Probe, all CLI subcommands (`devices`, `flash`, `erase`, `verify`, `reset`) run deterministically in CI environments with standardized exit codes (`0` for success, `1` for flash/verify mismatch, `2` for connection errors, `3` for parser errors, `4` for probe not found, `5` for invalid args).
- **Step 6 (Reusable Profiles)**: Storing named profiles in TOML format enables human editability, Git tracking, and cross-platform resolution using `directories::ProjectDirs` (`%APPDATA%\flashgui\profiles` on Windows, `~/.config/flashgui/profiles` on Linux).
- **Step 7 (Test Hierarchy)**: Vitest + React Testing Library with a mocked `@tauri-apps/api/core` module guarantees full frontend UI state and component testability in headless Node/JSDOM environments without requiring a physical display server.

## 3. Caveats
- No physical USB hardware (ST-Link, CMSIS-DAP) is connected in the local agent sandbox environment. All integration verification in CI and local test runs relies on the Virtual/Mock probe backend.
- Tauri v2 capability configuration (`src-tauri/capabilities/default.json`) must be created when scaffolding `flashgui-desktop` to grant permissions for core IPC commands.
- For raw binary (`.bin`) images, base address cannot be inferred automatically from file contents and defaults to `0x08000000` (STM32 flash base), requiring UI/CLI input for non-standard configurations.

## 4. Conclusion
The comprehensive architectural specification for R3 (Desktop GUI), R4 (CLI Companion), and Reusable Profiles is completed and documented in `.agents/explorer_survey_3/survey_report.md`. It includes:
- Exact workspace directory structure and dependency layout.
- 17 typed Tauri IPC commands with full Rust signatures, request DTOs, and response DTOs.
- High-frequency event streaming schemas for progress telemetry (`flash:progress`), console logging (`flash:log`), and status updates (`flash:status`).
- React 18 / TypeScript 5 component breakdown: Connection Panel, Firmware Dropzone, Memory Segment Table, Controls & Telemetry HUD, and Timestamped Developer Console.
- Zustand store definition and finite state machine.
- Clap 4 CLI command hierarchy, exit code matrix (0-5), TOML profile format, and mock backend integration.
- 4-tier testing hierarchy (Rust units, CLI headless E2E, Tauri command integration, Vitest component tests).

## 5. Verification Method
- **File Inspection**:
  - Review `.agents/explorer_survey_3/survey_report.md` for complete architecture and technical specifications.
- **Environment Verification**:
  - `cargo tauri --version` -> verifies Tauri v2 CLI availability.
  - `rustc --version && cargo --version && node --version && npm --version` -> verifies compiler and runtime readiness.
- **Implementation Validation Criteria (Downstream Milestones)**:
  - `npm run build` inside `frontend/` succeeds with zero TypeScript errors.
  - `npm test` runs Vitest test suite validating component rendering and state transitions.
  - `cargo test --workspace` compiles and passes all unit and mock integration tests.
  - `cargo run -p flashgui-cli -- --mock flash <hex_file> --verify` executes headless flashing and returns exit code 0.
