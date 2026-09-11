# Original User Request

## 2026-09-10T19:24:06Z

A modern, cross-platform, vendor-neutral desktop application and CLI companion for detecting debug probes and erasing, programming, verifying, and inspecting MCU flash memory, starting with STM32 targets and Intel HEX / raw BIN firmware images.

Working directory: c:/web_applications/open-source/embedded/flash_programmer_gui
Integrity mode: development

## Requirements

### R1. Modular Flash Core & Probe Abstraction Layer
Build a modular Rust core (`flash-core`) defining a unified `FlashBackend` trait with probe discovery, target connection, flash erase, programming, memory verification, and system reset. Implement a live backend using `probe-rs` (supporting ST-Link, CMSIS-DAP) alongside an in-memory Virtual/Mock Probe backend for headless testing, CI pipelines, and environments without physical hardware.

### R2. Firmware Parser & Memory Inspector
Implement a robust firmware parsing crate (`firmware-parser`) supporting Intel HEX (`.hex`) and raw binary (`.bin`) files. Extract and validate memory segments, base addresses, total size, checksums, and entry points, reporting structured metadata for UI inspection and verification.

### R3. Desktop Application GUI (Tauri + React / TypeScript)
Build a responsive, modern desktop GUI using Tauri with React and TypeScript featuring:
- **Connection Panel:** Probe selector with live auto-refresh, target chip selection, debug interface (SWD/JTAG), frequency/speed, and real-time connection status.
- **Firmware Panel:** Drag-and-drop file loader, detailed segment/address inspector, recent files history, and flash options (verify after programming, reset after programming, full chip erase).
- **Flashing Controls & Progress:** Primary PROGRAM / ERASE / VERIFY / RESET controls, animated progress bar with byte counts, percentage, transfer speed, and elapsed time.
- **Timestamped Console Output:** Detailed, scrollable developer console logging every stage of probe detection, connection, sector erasing, block flashing, and verification.

### R4. CLI Companion & Reusable Profiles
Provide a CLI binary (`flashgui-cli` or similar) sharing `flash-core` to enable headless probe discovery, flashing, and verification in CI scripts. Support saving and loading named configuration profiles (target, probe, SWD speed, firmware path).

## Acceptance Criteria

### Core & Parser Verification
- [ ] Unit tests in `firmware-parser` verifying valid and malformed Intel HEX and binary file parsing, memory gap handling, and address bounds checks.
- [ ] Integration tests in `flash-core` using the Virtual/Mock backend verifying probe detection, sector erase, buffer programming, byte verification, and reset cycles under normal and error-injected conditions.
- [ ] Automated test suite runnable via standard `cargo test` passing with zero errors.

### CLI & Automation
- [ ] CLI commands for probe listing (`devices`), flashing with flags (`--probe`, `--target`, `--verify`, `--reset`), and profile management execute and return proper status codes.
- [ ] Automated CLI integration tests verify headless flashing against the mock backend.

### Frontend & Application Build
- [ ] Frontend build succeeds (`npm run build` / `cargo tauri build` or check) without TypeScript errors or broken styling.
- [ ] Frontend test suite (`npm test` / `vitest`) validates state transitions (idle, connecting, flashing progress, completed, error handling) and log console appending.
