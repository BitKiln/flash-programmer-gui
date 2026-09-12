use std::fmt;
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::exit_codes::CliError;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "flashgui-cli",
    version,
    about = "Headless CLI companion for MCU Flash Programmer",
    long_about = "Headless CLI companion for discovering debug probes, erasing, programming, verifying, and resetting MCU flash memory."
)]
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

    /// Load a probe-rs target description, for a chip probe-rs was not built
    /// with. Repeatable.
    ///
    /// probe-rs ships no Espressif definitions, so the JTAG route to an ESP
    /// chip needs one of these; the serial bootloader route (--port) does not.
    #[arg(long, global = true, value_name = "PATH")]
    pub target_yaml: Vec<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// List connected debug probes
    Devices,

    /// Program firmware onto target microcontroller
    Flash(FlashArgs),

    /// Program the same firmware onto a series of boards (production mode)
    Batch(BatchArgs),

    /// Erase target MCU flash memory
    Erase(EraseArgs),

    /// Verify target memory against a firmware file
    Verify(VerifyArgs),

    /// Reset target MCU
    Reset(ResetArgs),

    /// Manage reusable programming profiles
    Profile {
        #[command(subcommand)]
        action: ProfileSubcommand,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Swd,
    Jtag,
}

impl From<Protocol> for flash_core::WireProtocol {
    fn from(p: Protocol) -> Self {
        match p {
            Protocol::Swd => flash_core::WireProtocol::Swd,
            Protocol::Jtag => flash_core::WireProtocol::Jtag,
        }
    }
}

impl From<flash_core::WireProtocol> for Protocol {
    fn from(p: flash_core::WireProtocol) -> Self {
        match p {
            flash_core::WireProtocol::Swd => Protocol::Swd,
            flash_core::WireProtocol::Jtag => Protocol::Jtag,
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::Swd => write!(f, "SWD"),
            Protocol::Jtag => write!(f, "JTAG"),
        }
    }
}

#[derive(Args, Debug, Clone)]
pub struct FlashArgs {
    /// Firmware file path (.hex or .bin). Optional if provided by profile.
    pub file: Option<String>,

    /// Target microcontroller name (e.g. STM32H753ZI); "auto" identifies the connected chip
    #[arg(short, long)]
    pub target: Option<String>,

    /// Specific probe serial number or ID
    #[arg(short, long)]
    pub probe: Option<String>,

    /// Debug interface protocol
    #[arg(short, long, value_enum)]
    pub interface: Option<Protocol>,

    /// Clock frequency in kHz
    #[arg(short, long)]
    pub speed: Option<u32>,
    /// Serial port of an ESP target in download mode (e.g. COM7, /dev/ttyUSB0).
    ///
    /// Shorthand for `--probe esp:<port>`. No debug probe is involved.
    #[arg(long, conflicts_with = "probe")]
    pub port: Option<String>,

    /// Baud rate for a serial bootloader connection (default 460800).
    #[arg(long)]
    pub baud: Option<u32>,

    /// Base address for raw binary files (e.g. 0x08000000)
    #[arg(short = 'a', long)]
    pub base_address: Option<String>,

    /// Verify flash contents after programming
    #[arg(long, default_missing_value = "true", num_args = 0..=1)]
    pub verify: Option<bool>,

    /// Disable verification after programming
    #[arg(long, conflicts_with = "verify")]
    pub no_verify: bool,

    /// Issue system reset after programming
    #[arg(long, default_missing_value = "true", num_args = 0..=1)]
    pub reset: Option<bool>,

    /// Disable reset after programming
    #[arg(long, conflicts_with = "reset")]
    pub no_reset: bool,

    /// Perform full chip erase before programming
    #[arg(long)]
    pub full_erase: bool,

    /// Load options from named profile
    #[arg(long)]
    pub profile: Option<String>,

    /// Stamp a serial number at this flash address (e.g. 0x0801F800)
    #[arg(long)]
    pub serial_address: Option<String>,

    /// Serial template; {n} is the counter, {n:06} pads it to six digits
    #[arg(long, default_value = "{n}")]
    pub serial_format: String,

    /// Counter value used for the first board
    #[arg(long, default_value_t = 1)]
    pub serial_start: u64,

    /// Amount the counter advances after each board
    #[arg(long, default_value_t = 1)]
    pub serial_step: u64,

    /// How the value is laid out in flash
    #[arg(long, value_enum, default_value_t = SerialFormat::Ascii)]
    pub serial_encoding: SerialFormat,

    /// Bytes reserved for an ASCII serial field
    #[arg(long, default_value_t = 16)]
    pub serial_width: usize,

    /// Skip reading the serial back after writing it
    #[arg(long)]
    pub no_serial_verify: bool,
}

/// Flash layout of a stamped serial number.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerialFormat {
    /// Rendered text, padded to the field width with 0xFF
    Ascii,
    /// Counter as a little-endian u32
    U32le,
    /// Counter as a big-endian u32
    U32be,
    /// Counter as a little-endian u64
    U64le,
}

impl From<SerialFormat> for flash_core::SerialEncoding {
    fn from(value: SerialFormat) -> Self {
        match value {
            SerialFormat::Ascii => flash_core::SerialEncoding::Ascii,
            SerialFormat::U32le => flash_core::SerialEncoding::U32Le,
            SerialFormat::U32be => flash_core::SerialEncoding::U32Be,
            SerialFormat::U64le => flash_core::SerialEncoding::U64Le,
        }
    }
}

/// How the runner waits for the next board between units.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rearm {
    /// Wait for the programmed board to be unplugged, then for the next one
    Detach,
    /// Program again as soon as the previous unit finishes
    Immediate,
}

#[derive(Args, Debug, Clone)]
pub struct BatchArgs {
    /// Connection, firmware, and option flags, identical to `flash`
    #[command(flatten)]
    pub flash: FlashArgs,

    /// Stop after this many boards; omit to run until interrupted
    #[arg(short = 'n', long)]
    pub count: Option<u32>,

    /// End the batch on the first failing board instead of continuing
    #[arg(long)]
    pub stop_on_error: bool,

    /// Pause this long after each board
    #[arg(long, default_value_t = 0)]
    pub delay_ms: u64,

    /// How the next board is detected
    #[arg(long, value_enum, default_value_t = Rearm::Detach)]
    pub rearm: Rearm,

    /// Write the production log here (CSV unless --log-json)
    #[arg(long)]
    pub log: Option<String>,

    /// Write the production log as JSON instead of CSV
    #[arg(long)]
    pub log_json: bool,

    /// Also print the per-board flash telemetry, not just one line per board
    #[arg(long)]
    pub unit_progress: bool,

    /// Give up waiting for a board to be unplugged after this long
    #[arg(long, default_value_t = 300_000)]
    pub detach_timeout_ms: u64,

    /// Give up waiting for the next board after this long
    #[arg(long, default_value_t = 300_000)]
    pub attach_timeout_ms: u64,

    /// How often to re-check while waiting for a board
    #[arg(long, default_value_t = 250)]
    pub poll_interval_ms: u64,
}

#[derive(Args, Debug, Clone)]
pub struct EraseArgs {
    /// Target microcontroller name (e.g. STM32H753ZI); "auto" identifies the connected chip
    #[arg(short, long, default_value = "auto")]
    pub target: String,

    /// Specific probe serial number or ID
    #[arg(short, long)]
    pub probe: Option<String>,

    /// Serial port of an ESP target in download mode (e.g. COM7, /dev/ttyUSB0).
    ///
    /// Shorthand for `--probe esp:<port>`. No debug probe is involved.
    #[arg(long, conflicts_with = "probe")]
    pub port: Option<String>,

    /// Baud rate for a serial bootloader connection (default 460800).
    #[arg(long)]
    pub baud: Option<u32>,

    /// Clock frequency in kHz
    #[arg(short, long, default_value_t = 2000)]
    pub speed: u32,

    /// Debug interface protocol
    #[arg(short, long, value_enum, default_value_t = Protocol::Swd)]
    pub interface: Protocol,

    /// Perform complete chip erase
    #[arg(long)]
    pub full: bool,

    /// Specific memory start address for range erase (e.g. 0x08000000)
    #[arg(long)]
    pub address: Option<String>,

    /// Length in bytes to erase
    #[arg(long)]
    pub length: Option<u32>,
}

#[derive(Args, Debug, Clone)]
pub struct VerifyArgs {
    /// Firmware file path (.hex or .bin)
    pub file: String,

    /// Target microcontroller name (e.g. STM32H753ZI); "auto" identifies the connected chip
    #[arg(short, long, default_value = "auto")]
    pub target: String,

    /// Specific probe serial number or ID
    #[arg(short, long)]
    pub probe: Option<String>,

    /// Serial port of an ESP target in download mode (e.g. COM7, /dev/ttyUSB0).
    ///
    /// Shorthand for `--probe esp:<port>`. No debug probe is involved.
    #[arg(long, conflicts_with = "probe")]
    pub port: Option<String>,

    /// Baud rate for a serial bootloader connection (default 460800).
    #[arg(long)]
    pub baud: Option<u32>,

    /// Clock frequency in kHz
    #[arg(short, long, default_value_t = 2000)]
    pub speed: u32,

    /// Debug interface protocol
    #[arg(short, long, value_enum, default_value_t = Protocol::Swd)]
    pub interface: Protocol,

    /// Base address for raw binary files (e.g. 0x08000000)
    #[arg(short = 'a', long)]
    pub base_address: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct ResetArgs {
    /// Target microcontroller name (e.g. STM32H753ZI); "auto" identifies the connected chip
    #[arg(short, long, default_value = "auto")]
    pub target: String,

    /// Specific probe serial number or ID
    #[arg(short, long)]
    pub probe: Option<String>,

    /// Serial port of an ESP target in download mode (e.g. COM7, /dev/ttyUSB0).
    ///
    /// Shorthand for `--probe esp:<port>`. No debug probe is involved.
    #[arg(long, conflicts_with = "probe")]
    pub port: Option<String>,

    /// Baud rate for a serial bootloader connection (default 460800).
    #[arg(long)]
    pub baud: Option<u32>,

    /// Clock frequency in kHz
    #[arg(short, long, default_value_t = 2000)]
    pub speed: u32,

    /// Debug interface protocol
    #[arg(short, long, value_enum, default_value_t = Protocol::Swd)]
    pub interface: Protocol,

    /// Halt CPU core immediately after reset
    #[arg(long)]
    pub halt: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ProfileSubcommand {
    /// Save a new or update an existing profile
    Save {
        /// Profile name
        name: String,

        /// Target microcontroller name (e.g. STM32F401RE)
        #[arg(short, long)]
        target: String,

        /// Optional description of profile
        #[arg(short, long)]
        description: Option<String>,

        /// Specific probe serial number or ID
        #[arg(short, long)]
        probe: Option<String>,

        /// Debug interface protocol
        #[arg(short, long, value_enum, default_value_t = Protocol::Swd)]
        interface: Protocol,

        /// Clock frequency in kHz
        #[arg(short, long, default_value_t = 2000)]
        speed: u32,

        /// Default firmware file path (.hex or .bin)
        #[arg(long)]
        firmware: Option<String>,

        /// Base address for raw binary files (e.g. 0x08000000)
        #[arg(long)]
        base_address: Option<String>,

        /// Verify flash contents after programming
        #[arg(long, default_missing_value = "true", num_args = 0..=1)]
        verify: Option<bool>,

        /// Issue system reset after programming
        #[arg(long, default_missing_value = "true", num_args = 0..=1)]
        reset: Option<bool>,

        /// Perform full chip erase before programming
        #[arg(long)]
        full_erase: bool,
    },

    /// Display details of a saved profile
    Show {
        /// Name of the profile to display
        name: String,
    },

    /// List all available profiles
    List,

    /// Delete a saved profile
    Delete {
        /// Name of the profile to delete
        name: String,
    },
}

/// Parses an address string which can be hex (e.g. "0x08000000") or decimal.
pub fn parse_address(addr_str: &str) -> Result<u32, CliError> {
    let clean = addr_str.trim();
    if let Some(hex) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).map_err(|e| {
            CliError::InvalidArgsOrProfile(format!(
                "Invalid hex address '{}': {}",
                addr_str, e
            ))
        })
    } else {
        clean.parse::<u32>().map_err(|e| {
            CliError::InvalidArgsOrProfile(format!(
                "Invalid address '{}': {}",
                addr_str, e
            ))
        })
    }
}
