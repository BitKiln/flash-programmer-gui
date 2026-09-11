# Architecture Specification: GUI, CLI & System Integration (R3 & R4)

**Author**: Explorer 3 (Application & Integration Architecture Specifier)  
**Date**: 2026-09-10  
**Target Milestone**: Phase 0 Architecture & Survey  
**Scope**: Requirement R3 (Tauri + React/TS GUI), Requirement R4 (CLI Companion & Reusable Profiles), Acceptance Criteria, and Full Test Automation Strategy

---

## 1. Executive Summary & Architecture Blueprint

The Flash Programmer system provides a vendor-neutral, modern toolchain for inspecting, erasing, programming, and verifying microcontroller flash memory. It targets ARM Cortex-M microcontrollers (with primary focus on STM32) and supports Intel HEX (`.hex`) and raw binary (`.bin`) images.

The architecture comprises two decoupled frontends sharing a common Rust core:
1. **Desktop GUI Application (`flashgui-desktop` + `frontend`)**: A lightweight, high-performance desktop application built on **Tauri v2** with a **React 18 / TypeScript 5** frontend, providing real-time probe monitoring, memory inspection, flashing progress metrics, and timestamped console streaming.
2. **CLI Companion (`flashgui-cli`)**: A scriptable command-line binary built with **Clap 4**, supporting headless CI automation, reusable TOML/JSON configuration profiles, and full simulation via an in-memory Virtual/Mock probe backend.

### 1.1 Architecture Topology

```
┌────────────────────────────────────────────────────────────────────────┐
│                          PRESENTATION TIER                             │
├──────────────────────────────────────┬─────────────────────────────────┤
│    Desktop Frontend (React 18 / TS)  │      CLI Companion Binary       │
│  - Connection Panel & Auto-Refresh   │         (flashgui-cli)          │
│  - Firmware Dropzone & Segment Table │  - Clap 4 Subcommands & Args    │
│  - Progress Bar & Telemetry HUD      │  - Reusable TOML Profiles       │
│  - Timestamped Developer Console     │  - indicatif Multi-Progress Bar │
│  - Zustand Reactive Store            │  - NDJSON CI Output Streams     │
└──────────────────┬───────────────────┴────────────────┬────────────────┘
                   │ IPC Invoke & Events                │ Direct Rust API
                   ▼                                    │
┌──────────────────────────────────────┐                │
│    Tauri v2 Desktop Host Core        │                │
│       (flashgui-desktop)             │                │
│  - Managed AppState & Tokio Runtime  │                │
│  - IPC Command Handlers              │                │
│  - Event Streaming (Progress / Log)  │                │
│  - Cancellation Token Manager        │                │
└──────────────────┬───────────────────┘                │
                   │                                    │
═══════════════════╪════════════════════════════════════╪═════════════════
                   ▼                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                        DOMAIN & LOGIC SERVICES                         │
├──────────────────────────────────────┬─────────────────────────────────┤
│        Firmware Parser Crate         │        Flash Core Crate         │
│         (firmware-parser)            │          (flash-core)           │
│  - Intel HEX Parser (Records 00-05)  │  - FlashBackend Trait           │
│  - Raw Binary Loader & Base Address  │  - Live probe-rs Driver (SWD)   │
│  - Segment Consolidation & Bounds    │  - Virtual / Mock Probe Driver  │
│  - CRC32, MD5, SHA256 Hashes         │  - Sector Erase & Block Flash   │
│  - Vector Table Entry Point Deduction│  - Progress & Log Event Sinks   │
└──────────────────────────────────────┴─────────────────────────────────┘
```

### 1.2 Technology Selection & Rationale

| Layer | Chosen Technology | Version | Rationale |
|---|---|---|---|
| Desktop Shell | **Tauri v2** | `^2.1` | Native webview (WebView2 on Windows) yields minimal memory footprint (~35MB vs Electron's 250MB+), zero-cost Rust FFI, high-performance async IPC. |
| UI Framework | **React** + **TypeScript** | `18.3` / `5.x` | Industry-standard declarative component architecture, rock-solid TypeScript typing, immense ecosystem for testing and UI components. |
| Build Tool | **Vite** | `^5.4` | Sub-second HMR, optimized production rollup bundling, native TypeScript and CSS module processing. |
| State Management | **Zustand** | `^4.5` | Zero-boilerplate reactive store with subscribe-with-selector, high performance outside React render cycles, clean state machine modeling. |
| UI Styling | **Tailwind CSS** + **Lucide React** | `^3.4` / `^0.4` | Utility-first responsive design, dark mode palette, crisp developer console aesthetics, standard embedded-tool iconography. |
| CLI Parser | **Clap** (derive) | `^4.5` | Idiomatic declarative CLI hierarchy, automated `--help`, shell completions, type-safe argument validation. |
| Profile Format | **TOML** (`serde_toml`) | `^0.8` | Human-readable, clean comments, standard for Rust developer tooling, strict schema validation. |
| Frontend Testing | **Vitest** + **React Testing Library** | `^2.1` / `^16.0` | In-memory JSDOM testing, instant execution, seamless mock IPC integration. |

---

## 2. Tauri v2 Desktop Backend Architecture

### 2.1 Workspace Layout

The repository is structured as a standard Cargo workspace with a co-located React/Vite frontend:

```
flash_programmer_gui/
├── Cargo.toml                    # Root workspace configuration
├── crates/
│   ├── flash-core/               # R1: Probe abstraction & flash engine
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── firmware-parser/          # R2: Intel HEX & BIN parser
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── flashgui-cli/             # R4: CLI companion executable
│   │   ├── Cargo.toml
│   │   └── src/
│   └── flashgui-desktop/         # R3: Tauri v2 Rust backend
│       ├── Cargo.toml
│       ├── tauri.conf.json       # Tauri v2 configuration
│       ├── capabilities/         # Tauri v2 permission sets
│       │   └── default.json
│       └── src/
│           ├── main.rs           # Tauri entry point & builder
│           ├── state.rs          # Managed AppState & session handles
│           ├── events.rs         # Event emitters & DTOs
│           ├── commands/         # Modular IPC command handlers
│           │   ├── mod.rs
│           │   ├── probe.rs      # Discovery & connection
│           │   ├── firmware.rs   # Parsing & memory inspection
│           │   ├── flash.rs      # Erase, program, verify, reset
│           │   └── profile.rs    # Configuration profile storage
│           └── error.rs          # Serializable CommandError enum
└── frontend/                     # R3: React + TypeScript UI
    ├── package.json
    ├── vite.config.ts
    ├── tsconfig.json
    ├── index.html
    ├── src/
    └── tests/
```

### 2.2 Tauri v2 Configuration (`tauri.conf.json`)

```json
{
  "$schema": "https://raw.githubusercontent.com/tauri-apps/tauri/v2/tooling/cli/schema.json",
  "productName": "Flash Programmer GUI",
  "version": "0.1.0",
  "identifier": "com.flashprogrammer.gui",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../frontend/dist"
  },
  "app": {
    "windows": [
      {
        "title": "MCU Flash Programmer",
        "width": 1024,
        "height": 768,
        "minWidth": 800,
        "minHeight": 600,
        "resizable": true,
        "fullscreen": false
      }
    ],
    "security": {
      "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:"
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/icon.ico"]
  }
}
```

### 2.3 Managed Application State (`state.rs`)

To guarantee thread safety across asynchronous IPC calls and prevent race conditions during long-running flash tasks, the Tauri backend maintains a managed `AppState`:

```rust
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use flash_core::{FlashBackend, FlashSession};
use crate::commands::profile::ProfileManager;

pub struct AppState {
    /// Active probe backend (Live probe-rs or Virtual/Mock)
    pub backend: Arc<RwLock<Box<dyn FlashBackend + Send + Sync>>>,
    /// Active attached session (if connected to a target)
    pub session: Arc<Mutex<Option<Box<dyn FlashSession + Send + Sync>>>>,
    /// Cooperative cancellation token for active flashing/erasing operations
    pub cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    /// Reusable profile persistence manager
    pub profile_manager: Arc<RwLock<ProfileManager>>,
    /// Flag indicating whether mock mode is globally enabled
    pub mock_mode: Arc<RwLock<bool>>,
}
```

### 2.4 Complete Tauri IPC Command Specifications

All IPC commands are asynchronous, annotated with `#[tauri::command]`, and return `Result<T, CommandError>` where `CommandError` implements `serde::Serialize`.

#### 2.4.1 Probe & Connection Commands (`commands/probe.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeInfoDto {
    pub id: String,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial_number: Option<String>,
    pub is_mock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectTargetParams {
    pub probe_id: String,
    pub target_chip: String,
    pub interface: String,       // "SWD" | "JTAG"
    pub speed_khz: u32,          // e.g. 2000
    pub use_mock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatusDto {
    pub is_connected: bool,
    pub probe_name: Option<String>,
    pub target_chip: Option<String>,
    pub core_state: Option<String>, // "Halted" | "Running" | "Sleeping"
    pub flash_size_bytes: Option<u64>,
}

/// Discovers connected hardware debug probes (plus mock probe if requested or enabled).
#[tauri::command]
pub async fn list_probes(
    include_mock: bool,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ProbeInfoDto>, CommandError>;

/// Attaches to target MCU using specified probe and configuration.
#[tauri::command]
pub async fn connect_target(
    params: ConnectTargetParams,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<ConnectionStatusDto, CommandError>;

/// Disconnects and releases probe and target session.
#[tauri::command]
pub async fn disconnect_target(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), CommandError>;

/// Queries current connection and target state.
#[tauri::command]
pub async fn get_connection_status(
    state: tauri::State<'_, AppState>,
) -> Result<ConnectionStatusDto, CommandError>;
```

#### 2.4.2 Firmware Commands (`commands/firmware.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySegmentDto {
    pub segment_index: usize,
    pub start_address: u64,
    pub end_address: u64,
    pub size_bytes: usize,
    pub crc32: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareMetadataDto {
    pub file_path: String,
    pub file_name: String,
    pub file_type: String,       // "IntelHex" | "RawBinary"
    pub total_size_bytes: usize,
    pub entry_point: Option<u64>,
    pub base_address: u64,
    pub calculated_crc32: String,
    pub calculated_sha256: String,
    pub segments: Vec<MemorySegmentDto>,
}

/// Parses Intel HEX or raw BIN file and returns structured metadata.
#[tauri::command]
pub async fn parse_firmware(
    file_path: String,
    base_address_override: Option<u64>,
) -> Result<FirmwareMetadataDto, CommandError>;
```

#### 2.4.3 Flashing & Memory Operations Commands (`commands/flash.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashExecutionOptionsDto {
    pub file_path: String,
    pub base_address_override: Option<u64>,
    pub verify_after: bool,
    pub reset_after: bool,
    pub full_chip_erase: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EraseOptionsDto {
    pub full_chip: bool,
    pub sector_ranges: Option<Vec<(u64, u64)>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResultDto {
    pub verified: bool,
    pub bytes_compared: usize,
    pub mismatch_address: Option<u64>,
    pub expected_byte: Option<u8>,
    pub actual_byte: Option<u8>,
}

/// Begins flash programming workflow: parses image -> sector/chip erase -> block write -> verify -> reset.
#[tauri::command]
pub async fn start_flashing(
    options: FlashExecutionOptionsDto,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), CommandError>;

/// Triggers manual flash erase (full chip or specified sector ranges).
#[tauri::command]
pub async fn start_erasing(
    options: EraseOptionsDto,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), CommandError>;

/// Triggers memory verification of flash contents against firmware file.
#[tauri::command]
pub async fn start_verifying(
    file_path: String,
    base_address_override: Option<u64>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<VerificationResultDto, CommandError>;

/// Resets target MCU (system reset with optional halt).
#[tauri::command]
pub async fn reset_target(
    halt: bool,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), CommandError>;

/// Cancels currently executing flash, erase, or verify operation.
#[tauri::command]
pub async fn cancel_operation(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), CommandError>;
```

#### 2.4.4 Profile Commands (`commands/profile.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashProfileDto {
    pub name: String,
    pub description: Option<String>,
    pub target_chip: String,
    pub interface: String,
    pub speed_khz: u32,
    pub probe_id: Option<String>,
    pub default_firmware_path: Option<String>,
    pub base_address: Option<u64>,
    pub verify_after: bool,
    pub reset_after: bool,
    pub full_chip_erase: bool,
}

#[tauri::command]
pub async fn list_profiles(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<FlashProfileDto>, CommandError>;

#[tauri::command]
pub async fn load_profile(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<FlashProfileDto, CommandError>;

#[tauri::command]
pub async fn save_profile(
    profile: FlashProfileDto,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError>;

#[tauri::command]
pub async fn delete_profile(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError>;
```

### 2.5 High-Frequency Event Streaming Architecture

Real-time telemetry and console logs must not block the UI or flood the IPC channel. Tauri v2's event system emits payloads asynchronously directly to the webview window:

```
[ Flash Thread ] ───emit───▶ [ Tauri Emitter ] ───IPC───▶ [ React Listener: listen() ]
```

#### Event Contract Schemas

1. **`flash:progress`** (Emitted periodically during erase, flash, and verify):
```typescript
export interface FlashProgressPayload {
  stage: 'idle' | 'erasing' | 'programming' | 'verifying' | 'resetting' | 'done';
  stageDescription: string;      // e.g. "Programming Sector 4 (0x08010000)"
  bytesTransferred: number;      // e.g. 65536
  totalBytes: number;            // e.g. 131072
  percentage: number;            // e.g. 50.0 (0.0 to 100.0)
  speedBytesPerSec: number;      // e.g. 45200.0
  elapsedMillis: number;         // e.g. 1450
  etaMillis: number | null;      // e.g. 1450
  currentAddress: number | null; // e.g. 0x08010000
}
```

2. **`flash:log`** (Emitted for developer console output):
```typescript
export type LogLevel = 'info' | 'success' | 'warn' | 'error' | 'debug';

export interface ConsoleLogPayload {
  id: string;                    // UUID or monotonic counter
  timestamp: string;             // ISO8601 or "HH:MM:SS.mmm"
  level: LogLevel;
  source: string;                // "probe-rs" | "flash-core" | "gui"
  message: string;
}
```

3. **`flash:status`** (Emitted on connection or core state changes):
```typescript
export interface TargetStatusPayload {
  isConnected: boolean;
  probeName: string | null;
  targetChip: string | null;
  coreState: 'Halted' | 'Running' | 'Unknown' | 'Disconnected';
  isBusy: boolean;
  currentOperation: string | null;
}
```

#### Cooperative Cancellation Pattern
When `start_flashing` is invoked:
1. An `Arc<AtomicBool>` or Tokio `CancellationToken` is created and stored in `AppState.cancel_token`.
2. The flash loop periodically evaluates `cancel_token.is_cancelled()`.
3. If true, the loop halts writing, sends a `[WARN] Operation cancelled by user` log event, restores the target core, and cleanly yields.
4. Calling `cancel_operation` triggers the token, guaranteeing no hanging threads or corrupted memory locks.

---

## 3. Frontend UI Architecture (React + TypeScript)

### 3.1 Component Hierarchy

```
App.tsx
├── Header.tsx (Title, Connection Status Badge, Mock/Live Toggle, Profiles Bar)
├── Main Grid (2-Column Responsive Layout)
│   ├── Left Column: Control & Connection
│   │   ├── ConnectionPanel.tsx
│   │   │   ├── ProbeSelector.tsx (Dropdown, Refresh Button, Auto-Refresh Toggle)
│   │   │   ├── TargetChipSelector.tsx (Searchable STM32 ComboBox)
│   │   │   ├── InterfaceSelector.tsx (SWD / JTAG segmented buttons)
│   │   │   ├── FrequencySelector.tsx (Preset frequencies dropdown)
│   │   │   └── ConnectButton.tsx (Connect / Disconnect toggle)
│   │   └── ProfileBar.tsx (Quick Save / Load / Manage Profiles)
│   │
│   └── Right Column: Firmware & Memory
│       └── FirmwarePanel.tsx
│           ├── FileDropzone.tsx (Drag & Drop zone + Browse Dialog)
│           ├── FirmwareSummaryCard.tsx (File size, format, CRC32, Entry Point)
│           ├── BaseAddressOverride.tsx (Hex input, active for BIN)
│           ├── SegmentTable.tsx (Expandable segment & memory viewer)
│           ├── RecentFilesList.tsx (Quick recent files launcher)
│           └── FlashOptionsForm.tsx (Verify, Reset, Full Erase checkboxes)
│
├── Action & Telemetry Section (Full Width)
│   ├── ControlBar.tsx (PROGRAM, ERASE, VERIFY, RESET, CANCEL buttons)
│   └── ProgressBar.tsx (Animated dual-tone bar, phase badge, telemetry metrics HUD)
│
└── Bottom Section: Developer Console
    └── ConsoleViewer.tsx
        ├── ConsoleToolbar.tsx (Filter tabs, Search, Auto-scroll lock, Clear, Copy, Export)
        └── ConsoleLogList.tsx (Virtualized or smooth scroll list with timestamped badges)
```

### 3.2 Component Specifications

#### 3.2.1 Header & Status Bar (`Header.tsx`)
- Displays app brand name: `MCU FLASH PROGRAMMER`.
- **Mock Mode Toggle**: A distinct pill switch toggle (`[Live Hardware | Virtual Simulator]`) that instantly flips backend provider mode.
- **Connection Status Badge**:
  - 🟢 `CONNECTED` (`STM32F401RE @ 2000 kHz`)
  - ⚪ `DISCONNECTED`
  - 🟡 `BUSY: PROGRAMMING`
  - 🔴 `CONNECTION ERROR`

#### 3.2.2 Connection Panel (`ConnectionPanel.tsx`)
- **Probe Selector**:
  - Populates via `list_probes(include_mock)`.
  - Displays probe product name + unique serial: e.g. `ST-LINK/V2-1 [066EFF535052]`, `CMSIS-DAP [DAP-1002]`, `Virtual Mock Probe [MOCK-STM32]`.
  - Manual Refresh button with spinning animation.
  - **Auto-Refresh Switch**: When enabled and disconnected, polls `list_probes` every 3 seconds to detect hot-plugged USB probes automatically.
- **Target Chip Selector**:
  - Filterable autocomplete dropdown pre-seeded with STM32 target devices (`STM32F401RE`, `STM32F411CE`, `STM32F103C8`, `STM32H743ZI`, `STM32G071RB`, `STM32L476RG`, `STM32F030R8`).
  - Supports custom chip entry for any probe-rs recognized target.
- **Debug Interface**: Radio group buttons for `SWD` (default) and `JTAG`.
- **Clock Frequency**:
  - Standard options: `100 kHz`, `500 kHz`, `1000 kHz`, `2000 kHz (Default)`, `4000 kHz`, `8000 kHz`, `10000 kHz`.
- **Connect / Disconnect Button**:
  - Dynamic styling: Emerald Green (`Connect`) vs Rose Red (`Disconnect`).
  - Disabled during active flashing.

#### 3.2.3 Firmware Panel & Memory Inspector (`FirmwarePanel.tsx`)
- **File Loader Dropzone**:
  - Large drag-and-drop landing area with dashed outline.
  - Accepts `.hex` and `.bin` extensions.
  - Clicking opens native file picker via Tauri dialog plugin (`open({ filters: [{ name: 'Firmware', extensions: ['hex', 'bin'] }] })`).
- **Firmware Summary Card**:
  - Badges for file format (`INTEL HEX` or `RAW BINARY`).
  - Total size formatted: e.g. `48.25 KB (49,408 bytes)`.
  - Calculated Checksums: CRC32 (`0x8F4C2A10`) and SHA256 snippet.
  - Entry Point: `0x08000189` (Thumb Reset Handler).
- **Segment / Address Table Viewer (`SegmentTable.tsx`)**:
  - Formatted tabular view showing non-contiguous segments:
    | Segment # | Start Address | End Address | Size | Segment CRC32 |
    |---|---|---|---|---|
    | 1 | `0x08000000` | `0x08003FFF` | 16.0 KB | `0x3B99A1C2` |
    | 2 | `0x08008000` | `0x0800FFFF` | 32.0 KB | `0x7C1045E1` |
- **Base Address Override Field**:
  - Read-only for `.hex` (auto-detected from Intel HEX Extended Linear Address records).
  - Editable input for `.bin` (default `0x08000000`, validates 32-bit hex address format).
- **Recent Files Dropdown**:
  - Stores last 5 files in `localStorage`. One click reloads and re-parses the file.
- **Flash Options Checkboxes**:
  - `[x] Verify after programming` (Default: ON)
  - `[x] Reset after programming` (Default: ON)
  - `[ ] Full chip erase before flash` (Default: OFF)

#### 3.2.4 Flashing Controls & Progress Bar (`ControlBar.tsx` & `ProgressBar.tsx`)
- **Action Buttons**:
  - `[ PROGRAM ]`: Primary high-emphasis button (Blue/Indigo with glow). Disabled if not connected or no firmware loaded.
  - `[ ERASE ]`: Amber button with confirmation dialog.
  - `[ VERIFY ]`: Purple button for read-back verification against loaded image.
  - `[ RESET TARGET ]`: Slate button for target core reset.
  - `[ CANCEL ]`: Appears only when `isBusy == true`.
- **Animated Progress Bar**:
  - Smooth 60fps CSS width transition: `transition: width 150ms ease-out`.
  - Pulsing animated barber-pole stripes during active operations.
  - Color shifts by stage: Amber (Erasing) -> Blue (Programming) -> Purple (Verifying) -> Emerald (Done) -> Rose (Error).
- **Telemetry HUD**:
  - Phase Pill: e.g. `PROGRAMMING SECTOR 3/6 (0x0800C000)`
  - Bytes Transferred: `32.0 KB / 64.0 KB`
  - Percentage: `50.0%`
  - Transfer Speed: `42.5 KB/s`
  - Elapsed Time: `00:01.5`
  - Estimated Remaining (ETA): `00:01.5`

#### 3.2.5 Timestamped Developer Console (`ConsoleViewer.tsx`)
- Terminal-style dark window (`bg-neutral-950 font-mono text-xs`).
- Formatted log line: `[12:34:56.789] [INFO] [probe-rs] Connected to ST-Link V2-1 (SWD @ 2000 kHz)`.
- Color badges:
  - `[INFO]`: Sky Blue
  - `[SUCCESS]`: Emerald Green
  - `[WARN]`: Amber Orange
  - `[ERROR]`: Bright Crimson Red
  - `[DEBUG]`: Slate Gray
- **Toolbar Features**:
  - Level Filter tabs: `All (142)`, `Info (98)`, `Success (12)`, `Warnings (2)`, `Errors (0)`.
  - Search filter input box.
  - `[Auto-Scroll: ON/OFF]` toggle button (locks scroll to bottom on new logs, unlocks if user scrolls up).
  - `[Clear Console]` button.
  - `[Copy All Logs]` button (copies full plain text log with timestamps to clipboard).
  - `[Export Logs]` button (saves `flash_log_<timestamp>.txt`).

### 3.3 Frontend State Management (`store/useAppStore.ts`)

State is centralized using a single Zustand store with explicit lifecycle state transitions:

```typescript
export type AppLifecycle = 
  | 'idle'
  | 'detecting'
  | 'connecting'
  | 'connected'
  | 'erasing'
  | 'programming'
  | 'verifying'
  | 'resetting'
  | 'cancelling'
  | 'error';

export interface AppStore {
  // Lifecycle
  lifecycle: AppLifecycle;
  mockMode: boolean;
  setMockMode: (mock: boolean) => void;

  // Connection State
  probes: ProbeInfoDto[];
  selectedProbeId: string | null;
  autoRefreshProbes: boolean;
  targetChip: string;
  debugInterface: 'SWD' | 'JTAG';
  speedKhz: number;
  connectionStatus: ConnectionStatusDto | null;

  // Firmware State
  firmwarePath: string | null;
  firmwareMetadata: FirmwareMetadataDto | null;
  baseAddressOverride: string;
  recentFiles: string[];

  // Flash Configuration
  verifyAfter: boolean;
  resetAfter: boolean;
  fullChipErase: boolean;

  // Progress Telemetry
  progress: FlashProgressPayload;

  // Developer Console
  logs: ConsoleLogPayload[];
  autoScroll: boolean;
  logFilterLevel: 'all' | LogLevel;
  logSearchQuery: string;

  // Actions
  refreshProbes: () => Promise<void>;
  connectTarget: () => Promise<void>;
  disconnectTarget: () => Promise<void>;
  loadFirmwareFile: (path: string) => Promise<void>;
  executeProgram: () => Promise<void>;
  executeErase: (fullChip: boolean) => Promise<void>;
  executeVerify: () => Promise<void>;
  executeReset: () => Promise<void>;
  cancelCurrentOperation: () => Promise<void>;
  appendLog: (log: ConsoleLogPayload) => void;
  clearLogs: () => void;
}
```

#### State Transition Validation Matrix

```
┌──────────────┐     connect()      ┌───────────────┐  flash()   ┌─────────────┐
│     IDLE     │ ─────────────────▶ │   CONNECTED   │ ─────────▶ │ PROGRAMMING │
└──────────────┘                    └───────┬───────┘            └──────┬──────┘
       ▲                                    │                           │
       │ disconnect()                       │                           ▼
       └────────────────────────────────────┘                    ┌─────────────┐
                                                                 │  VERIFYING  │
                                                                 └──────┬──────┘
                                                                        │
                                                                        ▼
                                                                 ┌─────────────┐
                                                                 │  RESETTING  │
                                                                 └──────┬──────┘
                                                                        │
                                                                        ▼
                                                                 ┌─────────────┐
                                                                 │  COMPLETED  │
                                                                 └──────┬──────┘
                                                                        │
                                                                        ▼
                                                                 ┌─────────────┐
                                                                 │  CONNECTED  │
                                                                 └─────────────┘
```

---

## 4. CLI Companion Architecture (`flashgui-cli`)

### 4.1 CLI Design & Clap 4 Hierarchy

`flashgui-cli` is a standalone binary sharing `flash-core` and `firmware-parser`. It provides an automated, headless interface for CI/CD pipelines, production test jigs, and terminal power users.

```rust
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "flashgui-cli", version, about = "CLI companion for MCU Flash Programmer", long_about = None)]
pub struct Cli {
    /// Enable virtual mock probe backend (no physical hardware required)
    #[arg(long, global = true)]
    pub mock: bool,

    /// Suppress human-readable progress indicators
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Output all status and progress in structured NDJSON format
    #[arg(long, global = true)]
    pub json: bool,

    /// Path to custom profile TOML file
    #[arg(long, global = true)]
    pub profile_file: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List connected debug probes
    Devices,

    /// Program firmware onto target microcontroller
    Flash {
        /// Firmware file path (.hex or .bin)
        file: String,

        /// Target microcontroller name (e.g. STM32F401RE)
        #[arg(short, long, default_value = "STM32F401RE")]
        target: String,

        /// Specific probe serial number or ID
        #[arg(short, long)]
        probe: Option<String>,

        /// Debug interface protocol
        #[arg(short, long, value_enum, default_value_t = Protocol::Swd)]
        interface: Protocol,

        /// Clock frequency in kHz
        #[arg(short, long, default_value_t = 2000)]
        speed: u32,

        /// Base address for raw binary files (e.g. 0x08000000)
        #[arg(short = 'a', long)]
        base_address: Option<String>,

        /// Verify flash contents after programming
        #[arg(long, default_value_t = true)]
        verify: bool,

        /// Issue system reset after programming
        #[arg(long, default_value_t = true)]
        reset: bool,

        /// Perform full chip erase before programming
        #[arg(long)]
        full_erase: bool,

        /// Load options from named profile
        #[arg(long)]
        profile: Option<String>,
    },

    /// Erase target MCU flash memory
    Erase {
        #[arg(short, long, default_value = "STM32F401RE")]
        target: String,
        #[arg(short, long)]
        probe: Option<String>,
        /// Perform complete chip erase
        #[arg(long)]
        full: bool,
    },

    /// Verify target memory against a firmware file
    Verify {
        file: String,
        #[arg(short, long, default_value = "STM32F401RE")]
        target: String,
        #[arg(short, long)]
        probe: Option<String>,
        #[arg(short = 'a', long)]
        base_address: Option<String>,
    },

    /// Reset target MCU
    Reset {
        #[arg(short, long, default_value = "STM32F401RE")]
        target: String,
        #[arg(short, long)]
        probe: Option<String>,
        /// Halt CPU core immediately after reset
        #[arg(long)]
        halt: bool,
    },

    /// Manage reusable programming profiles
    Profile {
        #[command(subcommand)]
        action: ProfileSubcommand,
    },
}

#[derive(Subcommand)]
pub enum ProfileSubcommand {
    /// Save a new or update an existing profile
    Save {
        name: String,
        #[arg(short, long)]
        target: String,
        #[arg(short, long)]
        probe: Option<String>,
        #[arg(short, long, default_value_t = 2000)]
        speed: u32,
        #[arg(long)]
        firmware: Option<String>,
        #[arg(long)]
        base_address: Option<String>,
        #[arg(long, default_value_t = true)]
        verify: bool,
        #[arg(long, default_value_t = true)]
        reset: bool,
    },
    /// Display details of a saved profile
    Show { name: String },
    /// List all available profiles
    List,
    /// Delete a saved profile
    Delete { name: String },
}
```

### 4.2 Reusable Profiles Schema & Storage (`profiles.rs`)

Profiles use standard TOML format. The schema supports clean versioning and team sharing:

```toml
# ~/.config/flashgui/profiles/stm32f4_dev.toml
[profile]
schema_version = 1
name = "stm32f4_dev"
description = "STM32F401RE Nucleo development board"
target = "STM32F401RE"
interface = "SWD"
speed_khz = 2000
probe_id = "auto"

[firmware]
default_path = "./target/firmware.hex"
base_address = "0x08000000"

[options]
verify_after = true
reset_after = true
full_chip_erase = false
```

#### Directory Resolution Strategy
Profile directories are resolved hierarchically using the `directories` crate:
1. **Local project directory**: `./.flashgui/profiles/<name>.toml` (checked first for repo-committed configs).
2. **User configuration directory**:
   - **Linux**: `$XDG_CONFIG_HOME/flashgui/profiles/` or `~/.config/flashgui/profiles/`
   - **Windows**: `%APPDATA%\flashgui\profiles\`
   - **macOS**: `~/Library/Application Support/flashgui/profiles/`

### 4.3 Deterministic Status Codes

The CLI strictly communicates execution outcomes via exit codes:

| Code | Constant | Meaning | Typical Causes |
|---|---|---|---|
| `0` | `EXIT_SUCCESS` | Operation completed successfully | Flashing, erase, or verification succeeded |
| `1` | `EXIT_FLASH_VERIFY_ERROR` | Flashing or memory verification failed | Byte mismatch, sector write timeout, locked flash |
| `2` | `EXIT_TARGET_CONNECTION_ERROR` | Cannot attach to target MCU | Target in sleep, SWD clock too high, unsupported chip |
| `3` | `EXIT_FIRMWARE_PARSE_ERROR` | Firmware parsing failed | Malformed Intel HEX, invalid checksum, address out of bounds |
| `4` | `EXIT_PROBE_NOT_FOUND` | Debug probe not detected | USB probe disconnected, permissions issue (udev rule on Linux) |
| `5` | `EXIT_INVALID_ARGS_OR_PROFILE` | Invalid command arguments | Profile file missing, unrecognized arguments, invalid hex address |

### 4.4 Terminal UI & Machine-Readable Output

- **Human-Friendly Terminal Mode**: Uses `indicatif` to render multi-stage spinners and progress bars:
  ```
  [1/3] Erasing Flash sectors... [========================] 100% (0.4s)
  [2/3] Programming Flash...     [=============>----------]  56% (48.2 KB/s, ETA: 0.8s)
  [3/3] Verifying Memory...      [========================] 100% (0.2s)
  ✔ Successfully flashed 49,152 bytes in 1.42s (34.6 KB/s). Target reset.
  ```
- **CI / Scripting JSON Mode (`--json`)**: Prints NDJSON (Newline Delimited JSON) events to standard output:
  ```json
  {"type":"status","stage":"erasing","progress":0.0,"message":"Erasing sectors"}
  {"type":"progress","stage":"programming","bytes_done":24576,"total_bytes":49152,"percentage":50.0,"speed_bps":48200}
  {"type":"complete","status":"success","bytes_flashed":49152,"duration_ms":1420}
  ```

---

## 5. Comprehensive Test Automation Strategy

The project implements a four-tier testing hierarchy to achieve complete test automation across hardware-dependent and UI workflows.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        TEST AUTOMATION TIERS                           │
├────────┬─────────────────────────────┬─────────────────────────────────┤
│ Tier 1 │ Rust Core & Parser Units    │ cargo test -p firmware-parser   │
│        │ (HEX/BIN records, math)     │ cargo test -p flash-core        │
├────────┼─────────────────────────────┼─────────────────────────────────┤
│ Tier 2 │ CLI Integration Tests       │ cargo test -p flashgui-cli      │
│        │ (Headless Mock CI tests)    │ (assert_cmd against mock probe) │
├────────┼─────────────────────────────┼─────────────────────────────────┤
│ Tier 3 │ Desktop IPC Integration     │ cargo test -p flashgui-desktop  │
│        │ (Rust IPC command handlers) │ (State, Tokio tasks, events)    │
├────────┼─────────────────────────────┼─────────────────────────────────┤
│ Tier 4 │ Frontend UI Component Tests │ npm run test / vitest run       │
│        │ (React Testing Library)     │ (State machines, Mocked IPC)    │
└────────┴─────────────────────────────┴─────────────────────────────────┘
```

### 5.1 Tier 1: Core & Parser Unit Tests
- **Intel HEX Parser**: Tests validating record types `00`, `01`, `02`, `03`, `04`, `05`, two's complement checksum validation, gaps handling, out-of-order records, and corrupt records.
- **Binary Parser**: Base address calculations, boundary overflows, empty files.
- **Mock Flash Backend**: Sector geometry, NOR erase (`0xFF` reset), block programming, bit-level write restrictions (bits can only transition 1 -> 0 without erase).

### 5.2 Tier 2: CLI Headless E2E Integration Tests (`tests/cli_tests.rs`)
Using `assert_cmd` and `predicates`, CLI tests run without USB hardware using the `--mock` backend:

```rust
#[test]
fn test_cli_devices_mock() {
    let mut cmd = Command::cargo_bin("flashgui-cli").unwrap();
    cmd.args(&["--mock", "devices", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Virtual Mock Probe"));
}

#[test]
fn test_cli_flash_mock_success() {
    let mut cmd = Command::cargo_bin("flashgui-cli").unwrap();
    cmd.args(&[
        "--mock",
        "flash",
        "tests/fixtures/sample_stm32.hex",
        "--verify",
        "--reset",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Successfully flashed"));
}

#[test]
fn test_cli_flash_verify_failure_returns_exit_code_1() {
    let mut cmd = Command::cargo_bin("flashgui-cli").unwrap();
    cmd.args(&[
        "--mock",
        "flash",
        "tests/fixtures/corrupt_data.hex",
        "--verify",
    ])
    .assert()
    .code(1);
}
```

### 5.3 Tier 3: Frontend Testing Strategy (Vitest + React Testing Library)

#### 5.3.1 Tauri IPC Mocking
Vitest intercepts Tauri invoke and listen APIs:

```typescript
// frontend/tests/mocks/tauriMock.ts
import { vi } from 'vitest';

export const mockInvoke = vi.fn().mockImplementation((cmd: string, args: any) => {
  switch (cmd) {
    case 'list_probes':
      return Promise.resolve([
        { id: 'mock-1', name: 'Virtual Mock Probe', vendor_id: 0, product_id: 0, is_mock: true }
      ]);
    case 'connect_target':
      return Promise.resolve({
        is_connected: true,
        probe_name: 'Virtual Mock Probe',
        target_chip: 'STM32F401RE',
        core_state: 'Halted',
        flash_size_bytes: 524288,
      });
    case 'parse_firmware':
      return Promise.resolve({
        file_name: 'test.hex',
        file_type: 'IntelHex',
        total_size_bytes: 16384,
        calculated_crc32: '0x12345678',
        segments: [{ segment_index: 0, start_address: 0x08000000, end_address: 0x08003FFF, size_bytes: 16384 }]
      });
    default:
      return Promise.resolve(null);
  }
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}));
```

#### 5.3.2 State Transition & Component Test Matrix
1. **Connection Panel Test**:
   - Renders probe dropdown.
   - Triggers probe refresh on mount.
   - User clicks `Connect` -> invokes `connect_target` -> displays Connected status badge.
2. **Firmware Panel Test**:
   - Simulates file drop event -> invokes `parse_firmware` -> renders Segment Table rows and calculated checksums.
3. **Flashing Progress & Telemetry Test**:
   - User clicks `PROGRAM` -> simulates arriving `flash:progress` events -> verifies progress bar width updates from 0% to 100% and speed/byte labels display correct numbers.
4. **Console Log Test**:
   - Emits `flash:log` event -> verifies log line with timestamp and level badge appears in the console viewport.
   - Tests `Clear Console` clears the list.

### 5.4 Tier 4: Continuous Integration (CI) Workflow

```yaml
name: Continuous Integration & Verification

on: [push, pull_request]

jobs:
  rust-verification:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Rust Unit & Integration Tests (Mock)
        run: cargo test --workspace --verbose
      - name: CLI Headless Flash Verification
        run: cargo run -p flashgui-cli -- --mock flash tests/fixtures/sample_stm32.hex --verify

  frontend-verification:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
      - name: Install dependencies
        working-directory: frontend
        run: npm ci
      - name: Vitest Suite
        working-directory: frontend
        run: npm test
      - name: Production Bundle Build Check
        working-directory: frontend
        run: npm run build
```

---

## 6. Feature Matrix & Requirements Traceability

| Requirement | Acceptance Criteria Item | Architecture Component | Verification Method |
|---|---|---|---|
| **R3 GUI** | Probe selector with live auto-refresh | `ConnectionPanel.tsx` + `commands/probe.rs` | Auto-refresh interval test + `list_probes` IPC mock |
| **R3 GUI** | Target chip selection & interface | `TargetChipSelector.tsx` + `InterfaceSelector.tsx` | ComboBox unit test + connection parameter assertion |
| **R3 GUI** | Drag & drop firmware loader | `FileDropzone.tsx` + `commands/firmware.rs` | Drag event simulation in RTL |
| **R3 GUI** | Segment / address table inspector | `SegmentTable.tsx` | Verifies segment address formatting and CRC32 display |
| **R3 GUI** | Flash options (verify, reset, full erase) | `FlashOptionsForm.tsx` | Form state reflection in `start_flashing` payload |
| **R3 GUI** | Controls: PROGRAM, ERASE, VERIFY, RESET | `ControlBar.tsx` | Button click triggers corresponding Tauri IPC command |
| **R3 GUI** | Animated progress bar & telemetry HUD | `ProgressBar.tsx` + `flash:progress` event | Emitter listener updates DOM elements & CSS widths |
| **R3 GUI** | Timestamped developer console | `ConsoleViewer.tsx` + `flash:log` event | Verifies log line timestamps, filter tabs, auto-scroll |
| **R3 GUI** | Frontend build check | `frontend/dist` via Vite | `npm run build` exits 0 with zero TypeScript errors |
| **R3 GUI** | Frontend state transitions & test suite | `tests/*.test.tsx` via Vitest | RTL tests cover idle -> connected -> flashing -> completed |
| **R4 CLI** | Command `devices` | `flashgui-cli devices` | `assert_cmd` test with `--mock` |
| **R4 CLI** | Command `flash` with verify/reset/erase | `flashgui-cli flash <file>` | Headless execution against Mock Probe |
| **R4 CLI** | Profile management (save/load/list) | `flashgui-cli profile` + `profiles.rs` | TOML serialization and CLI CRUD tests |
| **R4 CLI** | Headless Mock Backend for CI | `--mock` flag across all subcommands | Automated CI test without physical USB hardware |
| **R4 CLI** | Deterministic exit codes | `0` to `5` exit code model | CLI exit code assertion tests |

---

## 7. Next Steps & Implementation Roadmap

1. **Milestone: Core Rust Workspace Setup**
   - Initialize Cargo workspace root (`Cargo.toml`).
   - Scaffold `crates/firmware-parser`, `crates/flash-core`, `crates/flashgui-cli`, and `crates/flashgui-desktop`.
2. **Milestone: Desktop Backend & IPC**
   - Implement `AppState`, Tokio cancellation token, and IPC commands in `flashgui-desktop`.
   - Implement `flash:progress` and `flash:log` event streaming.
3. **Milestone: React Frontend**
   - Scaffold Vite + React + TypeScript + Tailwind in `frontend/`.
   - Build Zustand store, ConnectionPanel, FirmwarePanel, SegmentTable, ControlBar, ProgressBar, and ConsoleViewer.
4. **Milestone: CLI Companion**
   - Implement Clap CLI argument parsing and TOML profile manager in `flashgui-cli`.
5. **Milestone: Testing & Verification**
   - Author Vitest tests in `frontend/tests/`.
   - Author headless integration tests in `crates/flashgui-cli/tests/`.
   - Execute full verification: `cargo test --workspace` and `npm test` and `npm run build`.
