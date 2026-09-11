# Project: Flash Programmer GUI & CLI

## Architecture
The project is organized as a Cargo workspace with decoupled Rust crates and a modern Tauri v2 + React/TypeScript desktop application:
1. `crates/firmware-parser`: Zero-dependency, pure Rust library for parsing Intel HEX (`.hex`) and raw binary (`.bin`) firmware images, consolidating memory segments, detecting gaps, computing checksums (CRC32, MD5, SHA-256), and extracting entry points.
2. `crates/flash-core`: Core embedded probe abstraction layer providing `FlashBackend` and `FlashSession` traits, live `probe-rs` backend (ST-Link, CMSIS-DAP, J-Link), and in-memory Virtual/Mock Probe backend with authentic NOR flash physics (0xFF erased, 1->0 write limits), STM32 sector profiles, delay simulation, and deterministic fault injection.
3. `crates/flashgui-cli`: Headless CLI companion providing device discovery, erasing, programming, verification, reset, and TOML profile management for CI and scripting.
4. `src-tauri` & `frontend`: Desktop GUI application combining Tauri v2 Rust IPC backend and React 18 + TypeScript 5 frontend with real-time probe polling, firmware inspector, flashing controls, telemetry HUD, and timestamped developer console.

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
| F34 | Tauri Desktop Backend & IPC | 17 typed Tauri IPC commands bridging frontend to flash-core & parser | M4 | explorer_survey_3 |
| F35 | Desktop Connection Panel | Probe selector with auto-refresh, chip selector, SWD/JTAG, speed | M4 | explorer_survey_3 |
| F36 | Desktop Firmware Panel | Drag-and-drop loader, memory segment inspector table, recent files | M4 | explorer_survey_3 |
| F37 | Desktop Flash Options & Controls | Program / Erase / Verify / Reset controls with verify/reset checkboxes | M4 | explorer_survey_3 |
| F38 | Desktop Progress & Telemetry HUD | Animated progress bar, byte counters, transfer speed, elapsed time | M4 | explorer_survey_3 |
| F39 | Desktop Timestamped Console | Scrollable developer log console capturing probe, erase, write, verify logs | M4 | explorer_survey_3 |
| F40 | Desktop Cooperative Cancellation | Cancel ongoing flash operations cleanly via cancellation token | M4 | explorer_survey_3 |
| F41 | Comprehensive E2E Test Suite | 4-Tier requirement-driven opaque-box E2E test suite (Tiers 1-4) | E2E | Top-Level |
| F42 | Adversarial Hardening (Tier 5) | White-box adversarial test suite attacking edge cases and stress limits | M5 | Top-Level |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Firmware Parser Crate | Implement `firmware-parser` (HEX/BIN, segments, checksums, entry points, unit tests) | none | DONE |
| M2 | Flash Core & Probe Abstraction | Implement `flash-core` (traits, live probe-rs, virtual mock probe, fault injection, tests) | M1 (models) | DONE |
| M3 | CLI Companion & Profiles | Implement `flashgui-cli` (commands, flags, profiles, headless mock CI tests) | M1, M2 | PLANNED |
| M4 | Desktop Application GUI | Implement `src-tauri` IPC & React/TS frontend (panels, controls, console, Vitest) | M1, M2 | PLANNED |
| M5 | Final E2E Integration & Hardening | Phase 1: Pass 100% E2E test suite (Tiers 1-4); Phase 2: Tier 5 adversarial hardening | M1, M2, M3, M4, E2E | PLANNED |

## Parallel Dual-Track: E2E Testing Track
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| E2E | Opaque-Box E2E Test Suite | Requirement-driven test harness and Tiers 1-4 test suites (TEST_READY.md) | none (black-box) | DONE |

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
- `list_probes()` -> `Vec<ProbeInfo>`
- `connect_target(config: ConnectionConfig)` -> `ConnectionStatus`
- `disconnect_target()` -> `()`
- `parse_firmware(path: String, base_address: Option<u32>)` -> `FirmwareMetadata`
- `execute_flash(request: FlashRequest)` -> `FlashResult`
- `erase_flash(full_chip: bool)` -> `()`
- `verify_flash(path: String)` -> `VerifyReport`
- `reset_target(halt: bool)` -> `()`
- `cancel_operation()` -> `()`
- `list_profiles()` -> `Vec<ProfileSummary>`
- `save_profile(profile: FlashProfile)` -> `()`
- `load_profile(name: String)` -> `FlashProfile`
- Events emitted: `flash:progress` (`ProgressEvent`), `flash:log` (`LogEvent`), `flash:status` (`StatusEvent`).

## Code Layout
```
c:/web_applications/open-source/embedded/flash_programmer_gui/
├── Cargo.toml                          # Workspace root manifest
├── crates/
│   ├── firmware-parser/                # Crate: firmware-parser (M1)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── hex.rs
│   │   │   ├── bin.rs
│   │   │   ├── checksum.rs
│   │   │   ├── segment.rs
│   │   │   ├── metadata.rs
│   │   │   └── error.rs
│   │   └── tests/
│   │       └── golden_vectors.rs
│   ├── flash-core/                     # Crate: flash-core (M2)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── traits.rs
│   │   │   ├── error.rs
│   │   │   ├── types.rs
│   │   │   ├── progress.rs
│   │   │   ├── manager.rs
│   │   │   ├── live/
│   │   │   │   └── probe_rs_backend.rs
│   │   │   └── mock/
│   │   │       ├── backend.rs
│   │   │       ├── memory.rs
│   │   │       ├── fault.rs
│   │   │       └── profiles.rs
│   │   └── tests/
│   │       ├── mock_integration.rs
│   │       └── fault_injection.rs
│   └── flashgui-cli/                   # Crate: flashgui-cli (M3)
│       ├── Cargo.toml
│       ├── src/
│       │   ├── main.rs
│       │   ├── cli.rs
│       │   ├── commands/
│       │   │   ├── devices.rs
│       │   │   ├── flash.rs
│       │   │   ├── erase.rs
│       │   │   ├── verify.rs
│       │   │   ├── reset.rs
│       │   │   └── profile.rs
│       │   └── profile.rs
│       └── tests/
│           └── cli_mock_tests.rs
├── src-tauri/                          # Tauri v2 Backend (M4)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json
│   └── src/
│       ├── main.rs
│       ├── lib.rs
│       ├── commands.rs
│       ├── state.rs
│       └── events.rs
├── frontend/                           # React 18 + TS Frontend (M4)
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── vitest.config.ts
│   ├── index.html
│   └── src/
│       ├── App.tsx
│       ├── main.tsx
│       ├── store/
│       │   └── useFlashStore.ts
│       ├── components/
│       │   ├── ConnectionPanel.tsx
│       │   ├── FirmwarePanel.tsx
│       │   ├── FlashingControls.tsx
│       │   ├── ProgressTelemetry.tsx
│       │   └── ConsoleOutput.tsx
│       └── __tests__/
│           ├── store.test.ts
│           ├── ConnectionPanel.test.tsx
│           ├── FirmwarePanel.test.tsx
│           └── FlashingControls.test.tsx
└── tests/                              # E2E Testing Track (E2E)
    ├── e2e_runner.rs                   # Opaque-box E2E test harness
    ├── test_data/                      # Test hex, bin, profiles
    ├── tier1_features/
    ├── tier2_boundaries/
    ├── tier3_combinations/
    └── tier4_workloads/
```
