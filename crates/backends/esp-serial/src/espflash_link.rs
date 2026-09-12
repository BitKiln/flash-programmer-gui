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

/// The rate every ESP ROM bootloader answers the initial sync at. The
/// faster working rate is negotiated afterwards, by `Flasher::connect`;
/// opening the port at the target rate instead makes a healthy board look
/// absent.
const SYNC_BAUD: u32 = 115_200;

/// How many times to try the sync before giving up.
///
/// A board that has just been reset into its application does not always catch
/// the first auto-reset sequence, so a single attempt reports a healthy board
/// as absent. esptool retries for the same reason.
const SYNC_ATTEMPTS: usize = 4;

/// Settling time between attempts, enough for a reset to finish.
const SYNC_RETRY_DELAY: Duration = Duration::from_millis(250);

pub struct EspflashLink {
    /// `None` only between dropping a connection and making the next one.
    flasher: Option<Flasher>,
    /// Scratch file for reads. `espflash` writes flash reads to a path rather
    /// than returning bytes, so a read round-trips through the filesystem.
    scratch: PathBuf,
    /// Kept so the link can be reopened: an ESP write ends by rebooting the
    /// chip out of download mode, so the connection has to be made again.
    config: ConnectionConfig,
}

/// Relays espflash's own write progress to the session's callback.
struct ProgressRelay<'a> {
    report: &'a mut dyn FnMut(usize),
}

impl espflash::target::ProgressCallbacks for ProgressRelay<'_> {
    fn init(&mut self, _addr: u32, _total: usize) {}

    fn update(&mut self, current: usize) {
        (self.report)(current);
    }

    fn verifying(&mut self) {}

    fn finish(&mut self, _skipped: bool) {}
}

impl std::fmt::Debug for EspflashLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EspflashLink").finish_non_exhaustive()
    }
}

/// Whether this is the "nothing answered the sync" failure, which is worth
/// another attempt, rather than a port problem, which is not.
fn is_sync_failure(err: &FlashError) -> bool {
    err.to_string().contains("no ESP bootloader answered")
}

/// Turns an espflash error into something a user can act on.
fn wire_error(context: &str, err: impl std::fmt::Display) -> FlashError {
    FlashError::ProbeCommunication(format!("{context}: {err}"))
}

impl EspflashLink {
    /// Opens the port named by the connection's probe identifier and connects
    /// to whatever bootloader answers.
    pub fn open(config: &ConnectionConfig) -> Result<Self, FlashError> {
        let mut last = None;
        for attempt in 0..SYNC_ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(SYNC_RETRY_DELAY);
            }
            match Self::open_once(config) {
                Ok(link) => return Ok(link),
                // Only a failure to find the bootloader is worth retrying. A
                // port that does not exist, or is held by another program,
                // will not start working on the next pass.
                Err(e @ FlashError::ConnectError(_)) if is_sync_failure(&e) => last = Some(e),
                Err(other) => return Err(other),
            }
        }
        Err(last.unwrap_or_else(|| {
            FlashError::ConnectError("the ESP bootloader could not be reached".to_string())
        }))
    }

    fn open_once(config: &ConnectionConfig) -> Result<Self, FlashError> {
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

        let serial = serialport::new(port_name, SYNC_BAUD)
            .timeout(OPEN_TIMEOUT)
            .open_native()
            .map_err(|e| {
                FlashError::ConnectError(format!(
                    "could not open {port_name} at {SYNC_BAUD} baud: {e}. \
                     Another program (a serial monitor, or the IDE) may be holding it."
                ))
            })?;

        let connection = Connection::new(
            serial,
            usb_info,
            ResetAfterOperation::NoReset,
            ResetBeforeOperation::DefaultReset,
            SYNC_BAUD,
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

        Ok(Self {
            flasher: Some(flasher),
            scratch,
            config: config.clone(),
        })
    }
}

impl EspflashLink {
    /// The live connection, or an error saying it is gone.
    fn flasher(&mut self) -> Result<&mut Flasher, FlashError> {
        self.flasher.as_mut().ok_or_else(|| {
            FlashError::ConnectError(
                "the serial connection was dropped and could not be remade".to_string(),
            )
        })
    }
}

impl EspLink for EspflashLink {
    fn device_info(&mut self) -> Result<EspDeviceInfo, FlashError> {
        let chip = format!("{:?}", self.flasher()?.chip()).to_lowercase();
        let info = self
            .flasher()?
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
        self.flasher()?
            .erase_flash()
            .map_err(|e| FlashError::EraseError(e.to_string()))
    }

    fn erase_region(&mut self, offset: u32, size: u32) -> Result<(), FlashError> {
        self.flasher()?
            .erase_region(offset, size)
            .map_err(|e| FlashError::EraseError(e.to_string()))
    }

    fn write(
        &mut self,
        offset: u32,
        data: &[u8],
        progress: &mut dyn FnMut(usize),
    ) -> Result<(), FlashError> {
        // espflash chunks the write itself and ends it by rebooting the chip,
        // so the whole segment goes in one call and progress comes from its
        // callbacks rather than from a loop out here.
        let mut relay = ProgressRelay { report: progress };
        self.flasher()?
            .write_bin_to_flash(offset, data, &mut relay)
            .map_err(|e| FlashError::ProgramError(e.to_string()))
    }

    fn resync(&mut self) -> Result<(), FlashError> {
        // The port has to be released before it can be opened again, so the
        // old connection is dropped first and the field left empty until the
        // new one is up.
        drop(self.flasher.take());
        let reopened = Self::open(&self.config)?;
        self.flasher = reopened.flasher;
        Ok(())
    }

    fn read(&mut self, offset: u32, size: u32) -> Result<Vec<u8>, FlashError> {
        let scratch = self.scratch.clone();
        self.flasher()?
            .read_flash(offset, size, 0x1000, 64, scratch)
            .map_err(|e| wire_error("flash read failed", e))?;
        let data = std::fs::read(&self.scratch)?;
        let _ = std::fs::remove_file(&self.scratch);
        Ok(data)
    }

    fn md5(&mut self, offset: u32, size: u32) -> Result<[u8; 16], FlashError> {
        let digest = self
            .flasher()?
            .checksum_md5(offset, size)
            .map_err(|e| wire_error("could not read the flash checksum", e))?;
        Ok(digest.to_be_bytes())
    }

    fn reset(&mut self) -> Result<(), FlashError> {
        let chip = self.flasher()?.chip();
        self.flasher()?
            .connection()
            .reset_after(true, chip)
            .map_err(|e| wire_error("reset failed", e))
    }

    fn close(&mut self) -> Result<(), FlashError> {
        let _ = std::fs::remove_file(&self.scratch);
        Ok(())
    }
}
