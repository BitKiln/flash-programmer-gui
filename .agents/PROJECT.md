# Project: Flash Programmer GUI & CLI

## Architecture
The project is organized as a Cargo workspace with decoupled Rust crates and a modern Tauri v2 + React/TypeScript desktop application:
1. `crates/firmware-parser`: Zero-dependency, pure Rust library for parsing Intel HEX (`.hex`), raw binary (`.bin`), and ELF (`.elf`/`.axf`/`.out`, PT_LOAD program headers by physical address) firmware images, consolidating memory segments, detecting gaps, computing checksums (CRC32, MD5, SHA-256), and extracting entry points.
2. `crates/flash-core`: Core embedded probe abstraction layer providing `FlashBackend` and `FlashSession` traits, live `probe-rs` backend (ST-Link, CMSIS-DAP, J-Link), and in-memory Virtual/Mock Probe backend with authentic NOR flash physics (0xFF erased, 1->0 write limits), STM32 sector profiles, delay simulation, and deterministic fault injection.
3. `crates/flashgui-cli`: Headless CLI companion providing device discovery, erasing, programming, verification, reset, and TOML profile management for CI and scripting.
4. `crates/e2e-tests`: Opaque-box end-to-end suite that spawns the shipped `flashgui-cli` binary in an isolated sandbox; Tiers 1-4 run against the mock backend, Tier 5 against real hardware behind an env-var opt-in.
5. `apps/gui` (`src-tauri` + React frontend): Desktop GUI application combining Tauri v2 Rust IPC backend and React 18 + TypeScript 5 frontend with real-time probe polling, firmware inspector, flashing controls, telemetry HUD, and timestamped developer console.

```
       ┌───────────────────────┐         ┌─────────────────────────┐
       │   flashgui-desktop    │         │      flashgui-cli       │
       │   (Tauri v2 + React)  │         │    (Clap 4 Headless)    │
       └──────────┬────────────┘         └────────────┬────────────┘
                  │                                   │
                  ├─────────────────┬─────────────────┤
                  ▼                 ▼                 ▼
       ┌─────────────────────┐  ┌──────────────────────────────────┐
       │   firmware-parser   │  │            flash-core            │
       │ (HEX / BIN parser)  │  │  (FlashBackend / FlashSession)   │
       └─────────────────────┘  └────────────────┬─────────────────┘
                                                 │
                                 ┌───────────────┴───────────────┐
                                 ▼                               ▼
                     ┌───────────────────────┐       ┌───────────────────────┐
                     │   MockProbeBackend    │       │   ProbeRsLiveBackend  │
                     │  (NOR Flash & Faults) │       │   (ST-Link, DAPLink)  │
                     └───────────────────────┘       └───────────────────────┘
```

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| F01 | Intel HEX Line Lexing & Framing | Validate `:` start code, byte count, address, record type, and checksum | M1 | spec_miner_survey_2 |
| F02 | Intel HEX Two's Complement Checksum | Modulo 256 sum verification `(sum + cs) & 0xFF == 0` | M1 | spec_miner_survey_2 |
| F03 | Intel HEX Record Types 00 & 01 | Process Data records (00) and End of File record (01) | M1 | spec_miner_survey_2 |
| F04 | Extended Segment Address (Type 02) | 20-bit segmented base calculation `(USBA << 4) + AAAA` | M1 | spec_miner_survey_2 |
| F05 | Extended Linear Address (Type 04) | 32-bit linear base calculation `(ULBA << 16) + AAAA` | M1 | spec_miner_survey_2 |
| F06 | Start Address Records (03 & 05) | Extract CS:IP (03) and 32-bit EIP (05) execution entry points | M1 | spec_miner_survey_2 |
| F07 | Raw Binary Image Loading | Load `.bin` with configurable or default base address (0x08000000) | M1 | spec_miner_survey_2 |
| F08 | Memory Segment Consolidation | Sort out-of-order records and merge contiguous slices | M1 | spec_miner_survey_2 |
| F09 | Memory Gap Detection | Detect and report unallocated address ranges without synthetic padding | M1 | spec_miner_survey_2 |
| F10 | Record Overlap & Collision Handling | Non-fatal warning on identical data; error on conflicting overlap | M1 | spec_miner_survey_2 |
| F11 | Multi-Algorithm Checksums | Compute IEEE 802.3 CRC32, RFC 1321 MD5, and FIPS SHA-256 | M1 | spec_miner_survey_2 |
| F12 | Cortex-M Entry Point Heuristic | Infer reset handler from vector table offset 0x04 with Thumb bit | M1 | spec_miner_survey_2 |
| F13 | Target Bounds Check API | Validate firmware segments against target flash address boundaries | M1 | spec_miner_survey_2 |
| F14 | Serde Metadata Models | Structured serializable metadata for IPC / CLI summary outputs | M1 | spec_miner_survey_2 |
| F15 | Two-Tier Trait Abstraction | `FlashBackend` (probe discovery/factory) and `FlashSession` (operations) | M2 | explorer_survey_1 |
| F16 | Probe Discovery & Enumeration | Enumerate USB debug probes (ST-Link, CMSIS-DAP, J-Link, Mock) | M2 | explorer_survey_1 |
| F17 | Target Connection & Protocol Config | Connect with target chip name, SWD/JTAG, speed/clock frequency | M2 | explorer_survey_1 |
| F18 | Flash Erase (Full Chip & Sectors) | Erase all flash or specific address ranges / sectors | M2 | explorer_survey_1 |
| F19 | Flash Programming | Write firmware segments into target flash memory | M2 | explorer_survey_1 |
| F20 | Flash Verification | Read back and compare flash memory against expected firmware buffers | M2 | explorer_survey_1 |
| F21 | Target System Reset | Trigger target system reset and reset-and-halt cycles | M2 | explorer_survey_1 |
| F22 | Live probe-rs Backend | Hardware driver integration via probe-rs 0.32 under `live-probe` feature | M2 | explorer_survey_1 |
| F23 | In-Memory Virtual Probe Backend | Pure Rust headless backend simulating physical NOR flash (0xFF, 1->0) | M2 | explorer_survey_1 |
| F24 | Realistic STM32 Flash Profiles | STM32F1 uniform (1KB) and STM32F4 asymmetric sector geometries | M2 | explorer_survey_1 |
| F25 | Deterministic Fault Injection | Simulate connection loss, write protection, programming failure, corrupt bytes | M2 | explorer_survey_1 |
| F26 | Event-Driven Progress Telemetry | `FlashEvent` streaming (stage, bytes transferred, speed, elapsed time) | M2 | explorer_survey_1 |
| F27 | High-Level Execution Pipeline | `FlashManager::execute_flash` orchestrating erase, program, verify, reset | M2 | explorer_survey_1 |
| F28 | CLI Probe Listing (`devices`) | List attached debug probes in human and JSON formats | M3 | explorer_survey_3 |
| F29 | CLI Flash Command (`flash`) | Flash firmware image with `--probe`, `--target`, `--verify`, `--reset` flags | M3 | explorer_survey_3 |
| F30 | CLI Standalone Erase, Verify, Reset | Standalone subcommands for erase, verify against file, and target reset | M3 | explorer_survey_3 |
| F31 | CLI Reusable Profiles (`profile`) | Save, load, list, and apply named configuration profiles (TOML format) | M3 | explorer_survey_3 |
| F32 | CLI Mock Backend Flag (`--mock`) | Enable headless CI testing against Virtual Probe without physical hardware | M3 | explorer_survey_3 |
| F33 | CLI Deterministic Exit Codes | Standardized status codes (0=success, 1=verify fail, 2=conn error, 3=parse error, etc.) | M3 | explorer_survey_3 |
| F34 | Tauri Desktop Backend & IPC | Typed Tauri IPC commands bridging frontend to flash-core & parser | M4 | explorer_survey_3 |
| F35 | Desktop Connection Panel | Probe selector with auto-refresh, chip selector, SWD/JTAG, speed | M4 | explorer_survey_3 |
| F36 | Desktop Firmware Panel | Drag-and-drop loader, memory segment inspector table, recent files | M4 | explorer_survey_3 |
| F37 | Desktop Flash Options & Controls | Program / Erase / Verify / Reset controls with verify/reset checkboxes | M4 | explorer_survey_3 |
| F38 | Desktop Progress & Telemetry HUD | Animated progress bar, byte counters, transfer speed, elapsed time | M4 | explorer_survey_3 |
| F39 | Desktop Timestamped Console | Scrollable developer log console capturing probe, erase, write, verify logs | M4 | explorer_survey_3 |
| F40 | Desktop Cooperative Cancellation | Cancel ongoing flash operations cleanly via cancellation token | M4 | explorer_survey_3 |
| F41 | Comprehensive E2E Test Suite | Opaque-box E2E suite in `crates/e2e-tests` driving the shipped CLI binary (Tiers 1-4 mock, Tier 5 hardware-gated) | E2E | Top-Level |
| F42 | Adversarial Hardening (Tier 5) | White-box adversarial test suite attacking edge cases and stress limits | M5 | Top-Level |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Firmware Parser Crate | Implement `firmware-parser` (HEX/BIN, segments, checksums, entry points, unit tests) | none | DONE |
| M2 | Flash Core & Probe Abstraction | Implement `flash-core` (traits, live probe-rs, virtual mock probe, fault injection, tests) | M1 (models) | DONE |
| M3 | CLI Companion & Profiles | Implement `flashgui-cli` (commands, flags, profiles, headless mock CI tests) | M1, M2 | DONE |
| M4 | Desktop Application GUI | Implement `src-tauri` IPC & React/TS frontend (panels, controls, console, Vitest) | M1, M2 | IN PROGRESS |
| M5 | Final E2E Integration & Hardening | Phase 1: Pass 100% E2E test suite (Tiers 1-4); Phase 2: Tier 5 adversarial hardening | M1, M2, M3, M4, E2E | PLANNED |

## Parallel Dual-Track: E2E Testing Track
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| E2E | Opaque-Box E2E Test Suite | Harness and Tier 1-4 suites in `crates/e2e-tests`, spawning the `flashgui-cli` binary; Tier 5 runs against real hardware behind `FLASHGUI_HW_TARGET` | none (black-box) | IN PROGRESS |

## Interface Contracts

### 1. `firmware-parser` -> Consumers (`flash-core`, `flashgui-cli`, `src-tauri`)
```rust
pub struct MemorySegment {
    pub start_address: u32,
    pub data: Vec<u8>,
}

pub struct FirmwareMetadata {
    pub format: FirmwareFormat, // Hex, Binary
    pub total_bytes: usize,
    pub segments: Vec<SegmentMetadata>,
    pub memory_gaps: Vec<MemoryGap>,
    pub entry_point: Option<u32>,
    pub crc32: u32,
    pub md5: String,
    pub sha256: String,
}

pub struct FirmwareImage {
    pub metadata: FirmwareMetadata,
    pub segments: Vec<MemorySegment>,
}

pub fn parse_hex(content: &str) -> Result<FirmwareImage, ParseError>;
pub fn parse_bin(bytes: &[u8], base_address: u32) -> Result<FirmwareImage, ParseError>;
pub fn validate_target_bounds(image: &FirmwareImage, flash_start: u32, flash_size: u32) -> Result<(), ParseError>;
```

### 2. `flash-core` -> Consumers (`flashgui-cli`, `src-tauri`)
```rust
pub trait FlashBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn list_probes(&self) -> Result<Vec<ProbeInfo>, FlashError>;
    fn open_session(&self, config: &ConnectionConfig) -> Result<Box<dyn FlashSession>, FlashError>;
}

pub trait FlashSession: Send {
    fn erase_all(&mut self, cb: Option<&ProgressCallback>) -> Result<(), FlashError>;
    fn erase_range(&mut self, start: u32, length: u32, cb: Option<&ProgressCallback>) -> Result<(), FlashError>;
    fn program(&mut self, segments: &[MemorySegment], options: &ProgramOptions, cb: Option<&ProgressCallback>) -> Result<(), FlashError>;
    fn verify(&mut self, segments: &[MemorySegment], cb: Option<&ProgressCallback>) -> Result<VerifyReport, FlashError>;
    fn read_memory(&mut self, address: u32, length: u32) -> Result<Vec<u8>, FlashError>;
    fn reset(&mut self, halt: bool) -> Result<(), FlashError>;
    fn close(&mut self) -> Result<(), FlashError>;
}
```

### 3. Tauri IPC Commands (`src-tauri` -> React Frontend)
Implemented and registered in `apps/gui/src-tauri/src/lib.rs`:
- `list_probes()` -> `Vec<ProbeInfoDto>`
- `connect_probe(probe_id, target, protocol, speed)` -> `TargetInfoDto`
- `auto_detect_target(probe_id, protocol, speed)` -> `TargetInfoDto`
- `disconnect_probe()` -> `String`
- `load_firmware(path, base_address)` -> `FirmwareInfoDto`
- `flash_firmware(path, base_address, verify, reset, chip_erase)` -> `FlashResultDto`
- `erase_chip()` -> `String`
- `verify_firmware(path, base_address)` -> `VerifyResultDto`
- `reset_target(halt)` -> `String`
- `get_flash_events()` -> `Vec<FlashEventDto>`

Progress telemetry is currently **polled**: the backend buffers `FlashEvent`s in `AppState` and the
frontend drains them through `get_flash_events`. Pushed `flash:progress` / `flash:log` / `flash:status`
events are planned, not implemented.

Not yet implemented: `cancel_operation` (F40), `read_memory`, and the profile commands
(`list_profiles` / `save_profile` / `load_profile`), which exist only in the CLI today.

## Code Layout
```
flash_programmer_gui/
├── Cargo.toml                          # Workspace root (virtual manifest)
├── README.md
├── crates/
│   ├── firmware-parser/                # M1: HEX / BIN / ELF parsing
│   │   ├── src/
│   │   │   ├── lib.rs                  # parse_file, detect_format
│   │   │   ├── hex.rs  bin.rs  elf.rs  # per-format readers
│   │   │   ├── segment.rs  metadata.rs  checksum.rs  error.rs
│   │   └── tests/
│   │       ├── golden_vectors.rs
│   │       ├── adversarial_stress.rs
│   │       └── elf_fixture.rs
│   ├── flash-core/                     # M2: probe abstraction
│   │   ├── src/
│   │   │   ├── traits.rs               # FlashBackend / FlashSession
│   │   │   ├── manager.rs              # FlashManager::execute_flash
│   │   │   ├── progress.rs  types.rs  error.rs  unified.rs
│   │   │   ├── live/                   # probe-rs backend + target detection
│   │   │   └── mock/                   # NOR physics, profiles, fault injection
│   │   └── tests/
│   │       ├── mock_integration.rs
│   │       ├── fault_injection.rs
│   │       └── adversarial_challenge.rs
│   ├── flashgui-cli/                   # M3: headless CLI
│   │   ├── src/
│   │   │   ├── cli.rs  lib.rs  main.rs  output.rs  profile.rs  exit_codes.rs
│   │   │   └── commands/               # devices, flash, erase, verify, reset, profile
│   │   └── tests/
│   │       └── cli_mock_tests.rs
│   └── e2e-tests/                      # E2E track: opaque-box suite
│       ├── src/lib.rs                  # sandboxed harness spawning flashgui-cli
│       └── tests/
│           ├── tier1_features.rs
│           ├── tier2_boundaries.rs
│           ├── tier3_combinations.rs
│           ├── tier4_workloads.rs
│           └── hardware.rs             # #[ignore]d; needs FLASHGUI_HW_TARGET
├── apps/
│   └── gui/                            # M4: desktop application
│       ├── package.json  vite.config.ts  vitest.config.ts  index.html
│       ├── src/                        # React 18 + TypeScript frontend
│       │   ├── App.tsx  main.tsx
│       │   ├── state/AppContext.tsx
│       │   ├── hooks/useFlashProgrammer.ts
│       │   ├── components/
│       │   │   ├── ConnectionPanel.tsx
│       │   │   ├── FirmwarePanel.tsx
│       │   │   ├── FlashControls.tsx
│       │   │   ├── ProgressBar.tsx
│       │   │   └── ConsoleOutput.tsx
│       │   └── __tests__/
│       └── src-tauri/                  # Tauri v2 IPC backend
│           ├── Cargo.toml  tauri.conf.json  capabilities/
│           └── src/main.rs  lib.rs  commands.rs  state.rs
└── tests/
    ├── fixtures/                       # shared HEX / BIN / ELF / profile fixtures
    ├── firmware/u575/                  # source of the committed U575 blinky fixture
    └── generate_fixtures.py
```
