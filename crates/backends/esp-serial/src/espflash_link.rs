//! The real wire: [`EspLink`] over `espflash`.
//!
//! Deliberately thin. Everything that can be decided without a board is decided
//! in [`crate::session`], so this file is the only thing that an `espflash`
//! upgrade should touch and the only thing that cannot be covered in CI.
//!
//! `espflash` is pinned to an exact version in `Cargo.toml`: its library API is
//! not stable across majors, and a silent bump here would break flashing rather
//! than failing to compile.

use std::path::PathBuf;
use std::time::Duration;

use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::Flasher;
use flash_core::error::FlashError;
use flash_core::types::ConnectionConfig;

use crate::link::{EspDeviceInfo, EspLink};
use crate::ports::port_from_identifier;

/// How long to wait for the bootloader to answer before giving up.
const OPEN_TIMEOUT: Duration = Duration::from_secs(3);

pub struct EspflashLink {
    flasher: Flasher,
    /// Scratch file for reads. `espflash` writes flash reads to a path rather
    /// than returning bytes, so a read round-trips through the filesystem.
    scratch: PathBuf,
}

impl std::fmt::Debug for EspflashLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EspflashLink").finish_non_exhaustive()
    }
}

/// Turns an espflash error into something a user can act on.
fn wire_error(context: &str, err: impl std::fmt::Display) -> FlashError {
    FlashError::ProbeCommunication(format!("{context}: {err}"))
}

impl EspflashLink {
    /// Opens the port named by the connection's probe identifier and connects
    /// to whatever bootloader answers.
    pub fn open(config: &ConnectionConfig) -> Result<Self, FlashError> {
        let identifier = config.probe_id.as_deref().ok_or_else(|| {
            FlashError::ProbeNotFound(
                "no serial port given; pass one as `esp:<port>`, e.g. esp:COM7 or esp:/dev/ttyUSB0"
                    .to_string(),
            )
        })?;
        let port_name = port_from_identifier(identifier);
        let baud = crate::baud_for(config);

        let port_info = serialport::available_ports()
            .map_err(|e| wire_error("could not enumerate serial ports", e))?
            .into_iter()
            .find(|p| p.port_name == port_name)
            .ok_or_else(|| {
                FlashError::ProbeNotFound(format!(
                    "no serial port named {port_name}. Check the cable, and on Linux that you are \
                     in the group that owns the port (usually `dialout`)."
                ))
            })?;

        let usb_info = match port_info.port_type {
            serialport::SerialPortType::UsbPort(info) => info,
            _ => {
                return Err(FlashError::ConnectError(format!(
                    "{port_name} is not a USB serial port; the ESP bootloader needs one"
                )))
            }
        };

        let serial = serialport::new(port_name, baud)
            .timeout(OPEN_TIMEOUT)
            .open_native()
            .map_err(|e| {
                FlashError::ConnectError(format!(
                    "could not open {port_name} at {baud} baud: {e}. \
                     Another program (a serial monitor, or the IDE) may be holding it."
                ))
            })?;

        let connection = Connection::new(
            serial,
            usb_info,
            ResetAfterOperation::NoReset,
            ResetBeforeOperation::DefaultReset,
            baud,
        );

        // `chip: None` lets the bootloader identify itself, which is the whole
        // point of auto-detection; `verify`/`skip` are left to our own verify
        // pass so that one code path reports mismatches.
        let flasher = Flasher::connect(connection, true, false, false, None, Some(baud)).map_err(
            |e| {
                FlashError::ConnectError(format!(
                    "no ESP bootloader answered on {port_name}: {e}. \
                     Hold BOOT while tapping RESET to enter download mode, or check the baud rate."
                ))
            },
        )?;

        let scratch = std::env::temp_dir().join(format!(
            "flashgui-esp-read-{}.bin",
            std::process::id()
        ));

        Ok(Self { flasher, scratch })
    }
}

impl EspLink for EspflashLink {
    fn device_info(&mut self) -> Result<EspDeviceInfo, FlashError> {
        let chip = format!("{:?}", self.flasher.chip()).to_lowercase();
        let info = self
            .flasher
            .device_info()
            .map_err(|e| wire_error("could not read device info", e))?;

        Ok(EspDeviceInfo {
            chip,
            flash_size: info.flash_size.size(),
            revision: info
                .revision
                .map(|(major, minor)| format!("v{major}.{minor}")),
        })
    }

    fn erase_all(&mut self) -> Result<(), FlashError> {
        self.flasher
            .erase_flash()
            .map_err(|e| FlashError::EraseError(e.to_string()))
    }

    fn erase_region(&mut self, offset: u32, size: u32) -> Result<(), FlashError> {
        self.flasher
            .erase_region(offset, size)
            .map_err(|e| FlashError::EraseError(e.to_string()))
    }

    fn write(&mut self, offset: u32, data: &[u8]) -> Result<(), FlashError> {
        // Progress is accounted for a chunk at a time by the session, which is
        // what drives the UI; espflash's own callbacks would double-count.
        let mut noop = espflash::target::DefaultProgressCallback;
        self.flasher
            .write_bin_to_flash(offset, data, &mut noop)
            .map_err(|e| FlashError::ProgramError(e.to_string()))
    }

    fn read(&mut self, offset: u32, size: u32) -> Result<Vec<u8>, FlashError> {
        self.flasher
            .read_flash(offset, size, 0x1000, 64, self.scratch.clone())
            .map_err(|e| wire_error("flash read failed", e))?;
        let data = std::fs::read(&self.scratch)?;
        let _ = std::fs::remove_file(&self.scratch);
        Ok(data)
    }

    fn md5(&mut self, offset: u32, size: u32) -> Result<[u8; 16], FlashError> {
        let digest = self
            .flasher
            .checksum_md5(offset, size)
            .map_err(|e| wire_error("could not read the flash checksum", e))?;
        Ok(digest.to_be_bytes())
    }

    fn reset(&mut self) -> Result<(), FlashError> {
        let chip = self.flasher.chip();
        self.flasher
            .connection()
            .reset_after(true, chip)
            .map_err(|e| wire_error("reset failed", e))
    }

    fn close(&mut self) -> Result<(), FlashError> {
        let _ = std::fs::remove_file(&self.scratch);
        Ok(())
    }
}
