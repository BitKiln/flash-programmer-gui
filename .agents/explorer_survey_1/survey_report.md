# Architectural Survey & Specification: Core Flash Architecture & Probe Abstraction (`flash-core`)

**Author**: Explorer 1 (Core Systems Explorer)  
**Target Milestone**: R1 — Modular Flash Core & Probe Abstraction Layer  
**Workspace Path**: `crates/flash-core`  
**Reference Requirement**: `ORIGINAL_REQUEST.md` (Requirement R1, Acceptance Criteria)  
**Date**: 2026-09-10  

---

## 1. Executive Summary & Problem Scope

Requirement R1 mandates the creation of `flash-core`, a vendor-neutral, cross-platform Rust library providing a unified probe abstraction layer for embedded microcontroller flash programming. The system must support both:
1. **Live hardware debugging and flashing** via `probe-rs`, communicating directly with hardware debug probes (ST-Link v2/v3, CMSIS-DAP v1/v2, J-Link) over SWD and JTAG, with initial first-class support for STM32 targets.
2. **An in-memory Virtual/Mock Probe backend** enabling complete headless simulation of probe discovery, target connection, sector erasing, block/page flashing, byte-level memory verification, system reset, and deterministic fault injection for CI pipelines and automated testing.

This report establishes the complete architectural specification for `flash-core`, including trait designs, data structures, state machines, hardware/mock backends, error taxonomies, event-driven progress contracts, Cargo workspace dependencies, and test strategies runnable via standard `cargo test`.

---

## 2. Rust Workspace Layout & Dependencies

### 2.1 Multi-Crate Workspace Layout

To maintain clear separation of concerns, enable independent testing, and satisfy requirements R1–R4, the repository shall be structured as a Cargo workspace:

```text
flash_programmer_gui/
├── Cargo.toml                      # Top-level workspace manifest
├── Cargo.lock
├── crates/
│   ├── flash-core/                 # R1: Core flash trait, live probe-rs & mock backends
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs              # Public exports & prelude
│   │   │   ├── traits.rs           # FlashBackend & FlashSession trait definitions
│   │   │   ├── types.rs            # ProbeInfo, TargetInfo, ConnectionConfig, etc.
│   │   │   ├── error.rs            # FlashError & Result types
│   │   │   ├── events.rs           # FlashEvent, FlashStage, ProgressCallback
│   │   │   ├── manager.rs          # High-level FlashManager pipeline
│   │   │   ├── live/               # Live probe-rs implementation (optional feature)
│   │   │   │   ├── mod.rs
│   │   │   │   ├── backend.rs
│   │   │   │   └── session.rs
│   │   │   └── mock/               # Virtual / Mock probe implementation
│   │   │       ├── mod.rs
│   │   │       ├── backend.rs
│   │   │       ├── session.rs
│   │   │       ├── flash_memory.rs # In-memory NOR flash geometry & physics
│   │   │       ├── fault_injector.rs # Deterministic error injection engine
│   │   │       └── targets.rs      # Preset target definitions (STM32F1, STM32F4, etc.)
│   │   └── tests/
│   │       ├── mock_integration.rs # Full lifecycle tests (Erase -> Program -> Verify -> Reset)
│   │       ├── error_injection.rs  # Fault resilience tests
│   │       └── progress_events.rs  # Progress contract & event streaming tests
│   ├── firmware-parser/            # R2: Intel HEX & raw BIN parser
│   └── flashgui-cli/               # R4: CLI companion binary
├── src-tauri/                      # R3: Tauri desktop backend
├── src/                            # R3: React + TypeScript GUI frontend
└── package.json                    # Frontend package manifest
```

### 2.2 Root `Cargo.toml`
```toml
[workspace]
resolver = "2"
members = [
    "crates/flash-core",
    "crates/firmware-parser",
    "crates/flashgui-cli",
    "src-tauri",
]

[workspace.dependencies]
thiserror = "2.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
tokio = { version = "1.43", features = ["sync", "time", "rt", "macros"] }
```

### 2.3 `crates/flash-core/Cargo.toml`
```toml
[package]
name = "flash-core"
version = "0.1.0"
edition = "2021"
authors = ["Open Source Embedded Flash Programmer Team"]
description = "Modular flash programming core and probe abstraction layer for embedded targets"
license = "MIT OR Apache-2.0"

[features]
default = ["live-probe", "mock-probe"]
live-probe = ["dep:probe-rs"]
mock-probe = []

[dependencies]
thiserror = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
tokio = { workspace = true, optional = true }

# probe-rs provides ST-Link, CMSIS-DAP, J-Link, and target flash algorithms
probe-rs = { version = "0.32.0", default-features = true, optional = true }

[dev-dependencies]
tokio = { workspace = true }
```

**Key Architectural Insight**: By feature-gating `live-probe` (`["dep:probe-rs"]`), the entire core and mock simulation can compile and test instantly in environments without native C USB toolchains (e.g. lightweight CI nodes) via `cargo test --no-default-features --features mock-probe`, while retaining full live hardware capabilities when default features are enabled.

---

## 3. Unified Trait Architecture (`FlashBackend` & `FlashSession`)

To guarantee strict vendor-neutrality, maintainability, and clean abstraction, probe lifecycle operations are separated into two distinct contracts:
1. **`FlashBackend`**: Factory and probe discovery contract (stateless or probe-lister level).
2. **`FlashSession`**: Active target connection contract representing a live or simulated debug session with memory operations, erase, program, verify, and reset.

```
       +------------------------------------+
       |          FlashBackend              |
       +------------------------------------+
       | + list_probes() -> Vec<ProbeInfo>  |
       | + open(selector, cfg) -> Session   |
       +-----------------+------------------+
                         |
           +-------------+-------------+
           |                           |
+----------v----------+     +----------v----------+
|  LiveProbeBackend   |     |   MockProbeBackend  |
|  (probe-rs engine)  |     | (In-Memory engine)  |
+----------+----------+     +----------+----------+
           |                           |
+----------v----------+     +----------v----------+
|  LiveFlashSession   |     |   MockFlashSession  |
+----------+----------+     +----------+----------+
           |                           |
           +-------------+-------------+
                         |
       +-----------------v------------------+
       |           FlashSession             |
       +------------------------------------+
       | + probe_info()                     |
       | + target_info()                    |
       | + erase_all(cb)                    |
       | + erase_range(addr, len, cb)       |
       | + program(addr, data, opts, cb)    |
       | + verify(addr, data, cb)           |
       | + read_memory(addr, len)           |
       | + reset(halt)                      |
       | + close()                          |
       +------------------------------------+
```

### 3.1 Domain Models and Configuration Types

```rust
use serde::{Deserialize, Serialize};

/// Supported physical and virtual probe families
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbeType {
    StLink,
    CmsisDap,
    JLink,
    VirtualMock,
    Other(String),
}

/// Debug wire communication protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireProtocol {
    Swd,
    Jtag,
}

/// Metadata identifying a connected or simulated hardware probe
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeInfo {
    /// Unique identifier: e.g. "stlink:066EFF535752827167123456" or "mock:stm32f401"
    pub identifier: String,
    pub vendor_name: String,
    pub product_name: String,
    pub serial_number: Option<String>,
    pub probe_type: ProbeType,
    pub supported_protocols: Vec<WireProtocol>,
    pub default_speed_khz: u32,
    pub max_speed_khz: u32,
}

/// Target MCU specification and memory layout description
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetInfo {
    pub name: String,
    pub architecture: String,     // e.g. "ARMv7E-M"
    pub flash_base: u64,          // e.g. 0x0800_0000 for STM32
    pub flash_size: u64,          // e.g. 512 * 1024 (512 KB)
    pub ram_base: u64,            // e.g. 0x2000_0000
    pub ram_size: u64,            // e.g. 96 * 1024 (96 KB)
    pub page_size: u32,           // e.g. 256 bytes or 1024 bytes
    pub sectors: Vec<SectorInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectorInfo {
    pub index: u32,
    pub address: u64,
    pub size: u64,
}

/// Connection parameters requested by user or CLI
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub target_name: String,             // e.g. "stm32f401re"
    pub protocol: WireProtocol,          // SWD or JTAG
    pub speed_khz: u32,                  // e.g. 4000 (4 MHz)
    pub connect_under_reset: bool,       // Essential for sleeping / locked pins
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            target_name: "stm32f401re".to_string(),
            protocol: WireProtocol::Swd,
            speed_khz: 4000,
            connect_under_reset: false,
        }
    }
}

/// Options controlling the programming phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramOptions {
    pub verify_after: bool,
    pub reset_after: bool,
    pub chip_erase: bool,
    pub chunk_size: usize,
}

impl Default for ProgramOptions {
    fn default() -> Self {
        Self {
            verify_after: true,
            reset_after: true,
            chip_erase: false,
            chunk_size: 1024,
        }
    }
}
```

### 3.2 Trait Definitions

```rust
use crate::error::FlashResult;
use crate::events::ProgressCallback;
use crate::types::*;

/// Top-level factory for probe enumeration and session establishment
pub trait FlashBackend: Send + Sync {
    /// Return the backend identifier ("probe-rs" or "mock")
    fn name(&self) -> &'static str;

    /// Enumerate all currently available physical or virtual probes
    fn list_probes(&self) -> FlashResult<Vec<ProbeInfo>>;

    /// Connect to a probe and attach to the target MCU, returning an active session
    fn open_session(
        &self,
        probe_id: &str,
        config: &ConnectionConfig,
    ) -> FlashResult<Box<dyn FlashSession>>;
}

/// Active connection session with an MCU target
pub trait FlashSession: Send {
    /// Returns probe hardware metadata
    fn probe_info(&self) -> &ProbeInfo;

    /// Returns target MCU geometry and capabilities
    fn target_info(&self) -> &TargetInfo;

    /// Erase the entire target flash memory (Bulk / Mass Erase)
    fn erase_all(&mut self, progress: Option<&mut dyn ProgressCallback>) -> FlashResult<()>;

    /// Erase only the sectors covering the specified address range
    fn erase_range(
        &mut self,
        start_address: u64,
        length: u64,
        progress: Option<&mut dyn ProgressCallback>,
    ) -> FlashResult<()>;

    /// Program a binary payload starting at base address
    fn program(
        &mut self,
        start_address: u64,
        data: &[u8],
        options: &ProgramOptions,
        progress: Option<&mut dyn ProgressCallback>,
    ) -> FlashResult<()>;

    /// Verify target flash contents against expected binary payload
    fn verify(
        &mut self,
        start_address: u64,
        expected: &[u8],
        progress: Option<&mut dyn ProgressCallback>,
    ) -> FlashResult<VerifyReport>;

    /// Read raw memory bytes from target MCU
    fn read_memory(&mut self, address: u64, length: usize) -> FlashResult<Vec<u8>>;

    /// Trigger target system reset
    fn reset(&mut self, halt: bool) -> FlashResult<()>;

    /// Gracefully terminate session and release probe hardware
    fn close(&mut self) -> FlashResult<()>;
}

/// Structured report of verification comparison
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    pub success: bool,
    pub bytes_verified: u64,
    pub mismatches: Vec<MismatchDetail>,
    pub checksum_expected: u32,
    pub checksum_actual: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MismatchDetail {
    pub address: u64,
    pub expected: u8,
    pub actual: u8,
}
```

---

## 4. Live Probe Backend (`probe-rs`)

The live backend provides hardware integration using the `probe-rs` (v0.32) crate.

### 4.1 Probe Enumeration & Filtering

In modern `probe-rs`, probes are discovered via `probe_rs::probe::list::Lister::new()`. Probes are categorized as ST-Link, CMSIS-DAP, or J-Link based on vendor/product IDs or probe driver type:

```rust
#[cfg(feature = "live-probe")]
pub struct ProbeRsBackend {
    lister: probe_rs::probe::list::Lister,
}

#[cfg(feature = "live-probe")]
impl FlashBackend for ProbeRsBackend {
    fn name(&self) -> &'static str {
        "probe-rs"
    }

    fn list_probes(&self) -> FlashResult<Vec<ProbeInfo>> {
        let probes = self.lister.list_all();
        let mut list = Vec::new();

        for p in probes {
            let p_type = match p.probe_type {
                probe_rs::probe::ProbeType::StLink => ProbeType::StLink,
                probe_rs::probe::ProbeType::CmsisDap => ProbeType::CmsisDap,
                probe_rs::probe::ProbeType::JLink => ProbeType::JLink,
                _ => ProbeType::Other(format!("{:?}", p.probe_type)),
            };

            let identifier = format!("{}:{}", p.identifier(), p.serial_number.as_deref().unwrap_or("unknown"));

            list.push(ProbeInfo {
                identifier,
                vendor_name: p.vendor_id.map(|v| format!("{:04x}", v)).unwrap_or_else(|| "Unknown".into()),
                product_name: p.product_name.clone(),
                serial_number: p.serial_number.clone(),
                probe_type: p_type,
                supported_protocols: vec![WireProtocol::Swd, WireProtocol::Jtag],
                default_speed_khz: 4000,
                max_speed_khz: 10000,
            });
        }
        Ok(list)
    }

    fn open_session(&self, probe_id: &str, config: &ConnectionConfig) -> FlashResult<Box<dyn FlashSession>> {
        let probes = self.lister.list_all();
        let matched = probes.into_iter().find(|p| {
            let id = format!("{}:{}", p.identifier(), p.serial_number.as_deref().unwrap_or("unknown"));
            id == probe_id || p.identifier() == probe_id
        }).ok_or_else(|| FlashError::ProbeNotFound(probe_id.to_string()))?;

        let mut probe = matched.open().map_err(|e| FlashError::ProbeCommunication(e.to_string()))?;

        // Configure protocol and speed
        let protocol = match config.protocol {
            WireProtocol::Swd => probe_rs::probe::WireProtocol::Swd,
            WireProtocol::Jtag => probe_rs::probe::WireProtocol::Jtag,
        };
        probe.select_protocol(protocol).map_err(|e| FlashError::ProbeCommunication(e.to_string()))?;
        probe.set_speed(config.speed_khz).map_err(|e| FlashError::ProbeCommunication(e.to_string()))?;

        // Attach to target
        let target = probe_rs::config::get_target_by_name(&config.target_name)
            .map_err(|e| FlashError::TargetNotSupported(format!("{}: {}", config.target_name, e)))?;

        let session = probe.attach(target, probe_rs::Permissions::default())
            .map_err(|e| FlashError::TargetConnectionFailed {
                probe: probe_id.to_string(),
                target: config.target_name.clone(),
                source: e.to_string(),
            })?;

        Ok(Box::new(ProbeRsSession::new(session, probe_id, config)?))
    }
}
```

### 4.2 Programming with `FlashLoader` & DownloadOptions

`probe-rs` executes flash operations by uploading a RAM-based flash algorithm stub specific to the target MCU:
- Sector erase, page write, and verification can be dispatched using `session.target().flash_loader()`.
- Data buffers are appended via `loader.add_data(address, slice)`.
- Flashing is committed via `loader.commit(&mut session, options)`.
- DownloadOptions allows configuring `do_chip_erase: bool`, `verify: bool`, and `skip_erase: bool`.
- Event mapping: `probe_rs::flashing::FlashProgress` closures receive `ProgressEvent::Started`, `ProgressEvent::PageProgrammed { page_address, size }`, `ProgressEvent::FlashLayoutReady` and translate directly into `flash-core::events::FlashEvent`.

### 4.3 STM32 Specifics Handled by Live Backend
- **Connect Under Reset**: If target firmware reconfigures SWD pins (PA13/PA14 on STM32F1/F4) as GPIOs or enters low-power STOP/STANDBY mode, standard connection fails. The backend exposes `connect_under_reset` via `probe_rs::probe::Probe::attach_under_reset`.
- **System Reset**: Executes hardware NRST or software NVIC `SYSRESETREQ` via `session.core(0)?.reset()`.

---

## 5. Virtual / Mock Probe Backend (In-Memory Simulation)

The Virtual/Mock Probe backend is an essential, zero-dependency component implementing realistic NOR flash semantics, realistic sector layouts, CPU vector table initialization, and deterministic error injection.

### 5.1 NOR Flash Physics Simulation

Real microcontroller flash memory has physical characteristics fundamentally different from RAM:
1. **Erased State**: Every bit in erased flash is `1` (`0xFF` per byte).
2. **Write Rule**: Programming can only transition bits from `1` to `0`. Writing `1` into a bit that is already `0` has no effect physically on real silicon, or results in corrupt values unless the sector is erased first.
3. **Sector Erase Granularity**: Flash cannot be erased byte-by-byte; it must be erased in whole sectors.
4. **Page Programming Granularity**: Programming is done in chunks (e.g. 256-byte pages).

The Mock Backend accurately models these mechanics:

```rust
pub struct MockFlashMemory {
    base_address: u64,
    total_size: u64,
    data: Vec<u8>,
    sectors: Vec<SectorInfo>,
    erased_sectors: Vec<bool>, // tracks whether each sector is clean 0xFF
}

impl MockFlashMemory {
    pub fn new(base_address: u64, sectors: Vec<SectorInfo>) -> Self {
        let total_size = sectors.iter().map(|s| s.size).sum();
        let count = sectors.len();
        Self {
            base_address,
            total_size,
            data: vec![0xFF; total_size as usize],
            sectors,
            erased_sectors: vec![true; count],
        }
    }

    pub fn erase_sector(&mut self, sector_idx: usize) -> FlashResult<()> {
        let sector = self.sectors.get(sector_idx).ok_or_else(|| {
            FlashError::InvalidAddress { address: 0, reason: "Invalid sector index".into() }
        })?;
        let offset = (sector.address - self.base_address) as usize;
        let len = sector.size as usize;
        self.data[offset..offset + len].fill(0xFF);
        self.erased_sectors[sector_idx] = true;
        Ok(())
    }

    pub fn write_bytes(&mut self, address: u64, bytes: &[u8], strict_nor: bool) -> FlashResult<()> {
        let offset = (address - self.base_address) as usize;
        if offset + bytes.len() > self.data.len() {
            return Err(FlashError::AddressOutOfBounds {
                address: address + bytes.len() as u64,
                memory_size: self.total_size,
            });
        }

        for (i, &byte) in bytes.iter().enumerate() {
            let curr = self.data[offset + i];
            if strict_nor && (curr & byte) != byte {
                return Err(FlashError::ProgramFailed {
                    address: address + i as u64,
                    reason: format!(
                        "NOR flash violation: attempted to write 0x{:02X} over un-erased 0x{:02X} (1->0 bit violation)",
                        byte, curr
                    ),
                });
            }
            self.data[offset + i] = curr & byte; // Realistic bit-clearing
        }
        Ok(())
    }
}
```

### 5.2 Target Presets: STM32F1, STM32F4, and Generic Cortex-M

The mock backend includes built-in realistic geometry presets matching standard STM32 chips:

| Target Preset | Base Address | Total Flash | Sector Architecture |
|---|---|---|---|
| `mock:stm32f103c8` | `0x0800_0000` | 64 KB (Medium-Density) | 64 uniform sectors × 1 KB |
| `mock:stm32f401re` | `0x0800_0000` | 512 KB | Asymmetric: 4 × 16 KB, 1 × 64 KB, 3 × 128 KB |
| `mock:stm32f411ce` | `0x0800_0000` | 512 KB | Asymmetric: 4 × 16 KB, 1 × 64 KB, 3 × 128 KB |
| `mock:generic-cortex-m` | `0x0000_0000` | 1024 KB | 256 uniform sectors × 4 KB |

### 5.3 Deterministic Error Injection Engine

For robust CI validation, the mock backend provides an error injection harness allowing developers and automated tests to simulate real-world hardware failure modes:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectedFault {
    /// Probe fails to open or drops connection during connect
    ConnectFailure(String),
    /// Erasing a specific sector or address triggers a write-protection error (e.g. STM32 WRP / Option Byte lock)
    EraseFailure { address: u64, message: String },
    /// Programming fails at a specific byte offset or address (simulates VDD brownout or flash controller timeout)
    ProgramFailure { address: u64, message: String },
    /// Corrupt verification at specific address (simulates bad flash cell or bus read glitch)
    VerificationMismatch { address: u64, corrupted_byte: u8 },
    /// System reset fails (simulates floating NRST pin or watchdog lockout)
    ResetFailure(String),
    /// Disconnects unexpectedly after transferring N bytes
    DisconnectAfterBytes(usize),
}

#[derive(Default)]
pub struct FaultInjector {
    faults: Vec<InjectedFault>,
}

impl FaultInjector {
    pub fn new() -> Self { Self { faults: Vec::new() } }
    pub fn inject(&mut self, fault: InjectedFault) { self.faults.push(fault); }
    pub fn clear(&mut self) { self.faults.clear(); }
}
```

### 5.4 Timing & Delay Simulation Modes

To satisfy both high-speed automated testing and realistic GUI progress visualization, the mock backend supports 3 simulation modes:
1. **Instant / Zero-Delay Mode (`DelayMode::Zero`)**: Flashes 1 MB in microseconds for instant unit test execution.
2. **Realistic Embedded Mode (`DelayMode::Realistic`)**: Simulates real hardware delays (e.g. 15 ms per 16 KB sector erase, 40 µs per 256-byte page write, 10 ms system reset).
3. **Throttled GUI Demo Mode (`DelayMode::Throttled { bytes_per_sec: 32_000 }`)**: Smoothly streams progress at a fixed rate (e.g. 32 KB/s) so developers can evaluate frontend progress bars, rate meters, and cancellation triggers without hardware.

---

## 6. Progress Callbacks & Event Contract

Embedded flash programming operations can take from several hundred milliseconds to tens of seconds. A rich, event-driven contract is required to power both the Tauri GUI's animated progress bar / logs and the CLI's terminal progress bar (`indicatif`).

### 6.1 Lifecycle Stages

The flash workflow consists of 6 sequential stages:
1. `Connecting`: Negotiating probe protocol and identifying target MCU.
2. `Erasing`: Clearing mass flash or affected sectors.
3. `Programming`: Writing firmware blocks into flash.
4. `Verifying`: Reading back flash memory and validating checksum / byte equality.
5. `Resetting`: Issuing system reset and restoring CPU execution.
6. `Complete`: Operation successfully terminated.

### 6.2 Event Model

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlashStage {
    Connecting,
    Erasing,
    Programming,
    Verifying,
    Resetting,
    Complete,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressMetrics {
    pub stage: FlashStage,
    pub bytes_completed: u64,
    pub bytes_total: u64,
    pub percentage: f32,          // 0.0 to 100.0
    pub speed_bytes_per_sec: f64, // Instantaneous or moving-average transfer rate
    pub elapsed_ms: u64,
    pub estimated_remaining_ms: Option<u64>,
    pub current_address: u64,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum FlashEvent {
    StageStarted { stage: FlashStage, total_bytes: u64, message: String },
    Progress(ProgressMetrics),
    StageCompleted { stage: FlashStage, duration_ms: u64 },
    Log { level: LogLevel, message: String, timestamp_ms: u64 },
    Warning { message: String },
    Error { stage: FlashStage, message: String },
}

/// Callback trait for receiving synchronous or asynchronous progress events
pub trait ProgressCallback: Send {
    fn on_event(&mut self, event: FlashEvent);
    
    /// Optional cancellation check hook called at block boundaries
    fn is_cancelled(&self) -> bool {
        false
    }
}
```

### 6.3 Standard Implementations Provided by `flash-core`
1. **Channel Progress Callback (`ChannelProgressCallback`)**: Wraps a `tokio::sync::mpsc::UnboundedSender<FlashEvent>` or crossbeam channel, ideal for streaming events across thread boundaries to Tauri windows or web workers.
2. **Closure Progress Callback (`FnProgressCallback<F>`)**: Wraps an `FnMut(FlashEvent) -> bool`, ideal for unit tests and quick closures.
3. **Silent Callback (`NoopProgressCallback`)**: Drops events with zero overhead for headless automated batch runs.

---

## 7. Error Model (`flash-core::error`)

All operations in `flash-core` return `FlashResult<T> = Result<T, FlashError>`. `FlashError` is implemented using `thiserror`:

```rust
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FlashError {
    #[error("Probe not found: {0}")]
    ProbeNotFound(String),

    #[error("Probe communication failure: {0}")]
    ProbeCommunication(String),

    #[error("Target MCU not supported: {0}")]
    TargetNotSupported(String),

    #[error("Failed to connect to target '{target}' via probe '{probe}': {source}")]
    TargetConnectionFailed {
        probe: String,
        target: String,
        source: String,
    },

    #[error("Erase operation failed at address 0x{address:08X}: {reason}")]
    EraseFailed {
        address: u64,
        reason: String,
    },

    #[error("Flash programming failed at address 0x{address:08X}: {reason}")]
    ProgramFailed {
        address: u64,
        reason: String,
    },

    #[error("Memory verification mismatch at address 0x{address:08X}: expected 0x{expected:02X}, read 0x{actual:02X}")]
    VerificationMismatch {
        address: u64,
        expected: u8,
        actual: u8,
    },

    #[error("Verification checksum mismatch: expected CRC 0x{expected:08X}, read 0x{actual:08X}")]
    ChecksumMismatch {
        expected: u32,
        actual: u32,
    },

    #[error("Address out of flash bounds: address 0x{address:08X} exceeds memory size 0x{memory_size:08X}")]
    AddressOutOfBounds {
        address: u64,
        memory_size: u64,
    },

    #[error("Invalid memory address: 0x{address:08X} ({reason})")]
    InvalidAddress {
        address: u64,
        reason: String,
    },

    #[error("Target flash is protected or locked (sector 0x{address:08X})")]
    FlashProtected {
        address: u64,
    },

    #[error("Flash operation was cancelled by user")]
    OperationCancelled,

    #[error("Timeout during {operation}: {details}")]
    Timeout {
        operation: String,
        details: String,
    },

    #[error("Invalid session state: {0}")]
    InvalidState(String),

    #[error("Internal backend error: {0}")]
    Internal(String),
}

pub type FlashResult<T> = Result<T, FlashError>;
```

---

## 8. High-Level Orchestrator (`FlashManager`)

To allow GUI (Tauri commands) and CLI to execute a complete flash pipeline with a single function call, `flash-core` provides `FlashManager`:

```rust
pub struct FlashManager {
    backend: Box<dyn FlashBackend>,
}

#[derive(Debug, Clone)]
pub struct FlashTask {
    pub probe_id: String,
    pub connection: ConnectionConfig,
    pub segments: Vec<(u64, Vec<u8>)>, // Address -> Payload
    pub options: ProgramOptions,
}

impl FlashManager {
    pub fn new(backend: Box<dyn FlashBackend>) -> Self {
        Self { backend }
    }

    /// Executes complete pipeline: Open -> Erase -> Program -> Verify -> Reset -> Close
    pub fn execute_flash(
        &self,
        task: FlashTask,
        mut progress: impl ProgressCallback,
    ) -> FlashResult<FlashSummary> {
        let start_time = std::time::Instant::now();

        // 1. Connect
        progress.on_event(FlashEvent::StageStarted {
            stage: FlashStage::Connecting,
            total_bytes: 0,
            message: format!("Connecting to target {}...", task.connection.target_name),
        });
        let mut session = self.backend.open_session(&task.probe_id, &task.connection)?;
        progress.on_event(FlashEvent::StageCompleted {
            stage: FlashStage::Connecting,
            duration_ms: start_time.elapsed().as_millis() as u64,
        });

        // Calculate total bytes
        let total_bytes: u64 = task.segments.iter().map(|(_, data)| data.len() as u64).sum();

        // 2. Erase
        if task.options.chip_erase {
            session.erase_all(Some(&mut progress))?;
        } else {
            for (addr, data) in &task.segments {
                session.erase_range(*addr, data.len() as u64, Some(&mut progress))?;
            }
        }

        // 3. Program
        for (addr, data) in &task.segments {
            session.program(*addr, data, &task.options, Some(&mut progress))?;
        }

        // 4. Verify
        let mut total_verified = 0;
        if task.options.verify_after {
            for (addr, data) in &task.segments {
                let report = session.verify(*addr, data, Some(&mut progress))?;
                if !report.success {
                    let first = &report.mismatches[0];
                    return Err(FlashError::VerificationMismatch {
                        address: first.address,
                        expected: first.expected,
                        actual: first.actual,
                    });
                }
                total_verified += report.bytes_verified;
            }
        }

        // 5. Reset
        if task.options.reset_after {
            session.reset(false)?;
        }

        // 6. Close
        session.close()?;

        let total_duration = start_time.elapsed();
        progress.on_event(FlashEvent::StageCompleted {
            stage: FlashStage::Complete,
            duration_ms: total_duration.as_millis() as u64,
        });

        Ok(FlashSummary {
            total_bytes_flashed: total_bytes,
            total_bytes_verified: total_verified,
            total_duration_ms: total_duration.as_millis() as u64,
            average_speed_bps: if total_duration.as_secs_f64() > 0.0 {
                total_bytes as f64 / total_duration.as_secs_f64()
            } else {
                0.0
            },
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashSummary {
    pub total_bytes_flashed: u64,
    pub total_bytes_verified: u64,
    pub total_duration_ms: u64,
    pub average_speed_bps: f64,
}
```

---

## 9. Testing Strategy & Acceptance Verification

All acceptance criteria for R1 can be executed cleanly via standard `cargo test` using the Mock backend without any attached physical hardware.

### 9.1 Unit Test Coverage Matrix

| Test Suite | Module | Invariants Verified |
|---|---|---|
| `test_mock_flash_geometry` | `mock::flash_memory` | Address range checks, sector boundary mapping, total capacity validation |
| `test_mock_nor_flash_erase` | `mock::flash_memory` | Mass erase fills memory with `0xFF`, range erase clears only targeted sectors, un-erased sectors maintain contents |
| `test_mock_nor_bit_clearing` | `mock::flash_memory` | Successful `1 -> 0` transitions; strict mode rejects `0 -> 1` transitions without prior erase |
| `test_probe_discovery` | `mock::backend` | Mock backend lists virtual ST-Link and CMSIS-DAP probes with correct metadata and speeds |
| `test_session_lifecycle` | `mock::session` | Session transition states: Disconnected -> Connected -> Operations -> Closed |
| `test_system_reset` | `mock::session` | System reset updates target status, loads mock vector table SP and PC values |
| `test_read_memory` | `mock::session` | Arbitrary address block reads match internal flash memory bytes |

### 9.2 Integration Test Suites

1. **Full Lifecycle Integration (`tests/mock_integration.rs`)**:
   - Executes `FlashManager::execute_flash` on a simulated STM32F401 target (512 KB) with a 32 KB payload spanning multiple sectors.
   - Verifies:
     - Sector erase occurs on affected sectors only.
     - Buffer programming completes with 100% byte equality.
     - Verification passes with matching CRC32 checksums.
     - Target system reset triggers successfully.
     - Progress callbacks receive strictly monotonic byte counts and percentages reaching 100.0%.

2. **Error Injection & Fault Resilience (`tests/error_injection.rs`)**:
   - `test_injected_connection_failure`: Verifies `FlashError::TargetConnectionFailed` is returned and session resources are cleaned up.
   - `test_injected_sector_lock_failure`: Verifies `FlashError::EraseFailed` when sector write protection is simulated.
   - `test_injected_program_timeout`: Verifies `FlashError::ProgramFailed` midway through write.
   - `test_injected_verification_corrupted_byte`: Injects 1-byte corruption at address `0x0800_0124`; asserts `FlashError::VerificationMismatch` reports exact failing address, expected value, and corrupted value.
   - `test_user_cancellation_mid_transfer`: Asserts that when `ProgressCallback::is_cancelled()` returns true, flashing aborts immediately with `FlashError::OperationCancelled` without corrupting out-of-scope sectors.

3. **Progress Event Contract (`tests/progress_events.rs`)**:
   - Subscribes an unbounded channel to `FlashManager`.
   - Collects all `FlashEvent` variants.
   - Verifies:
     - Event sequence: `Connecting -> Erasing -> Programming -> Verifying -> Resetting -> Complete`.
     - Byte counts match payload length exactly.
     - Transfer speed metrics are positive non-zero numbers.

---

## 10. Recommendations & Hand-off Notes for Subsequent Workers

1. **Feature Gate Separation**: Ensure `crates/flash-core/Cargo.toml` keeps `probe-rs` as an optional dependency (`feature = "live-probe"`), so CI pipelines and headless test environments can run `cargo test --all --no-default-features --features mock-probe` without requiring native USB libraries (`libusb`).
2. **Zero-Copy Where Practical**: For `FlashSession::program` and `verify`, take slices `&[u8]` rather than owned `Vec<u8>` to minimize memory overhead when handling large firmware images (e.g. 2 MB binaries).
3. **STM32 Sector Mapping**: Pre-populate `mock::targets` with exact sector maps for popular STM32 targets (F103: 1KB pages; F401/F411: asymmetric 16/64/128KB sectors; G0/G4: 2KB pages). This ensures `firmware-parser` and UI tests have accurate sector boundaries to test against.
4. **Integration with `firmware-parser` (R2)**: `firmware-parser` will produce a `FirmwareImage` struct containing parsed segments `Vec<MemorySegment>`. Each `MemorySegment` maps directly to `(segment.address, segment.data)` accepted by `FlashSession` and `FlashManager`.
5. **Integration with Tauri & CLI (R3/R4)**: Provide a concrete `tokio::sync::mpsc` channel adapter so Tauri commands can stream `FlashEvent`s directly to the frontend window via `app_handle.emit("flash:progress", &event)`.
