//! ESP32 backend over the serial/USB ROM bootloader.
//!
//! This is how nearly every ESP32 is actually programmed: a USB cable, no debug
//! probe, and the ROM bootloader the chip starts in when it is held in download
//! mode. It is a genuinely different transport from a debug probe — there is no
//! SWD, no JTAG, no core to halt, and flash is addressed by offset rather than
//! by the memory-mapped view the running CPU sees.
//!
//! Layering:
//!
//! - [`EspSerialBackend`] discovers serial ports and opens sessions.
//! - [`session::EspSession`] holds the logic: address checks, chunking,
//!   cancellation, progress, verification.
//! - [`link::EspLink`] is the wire. [`fake::FakeLink`] implements it in memory
//!   so all of the above runs in CI; the `espflash` adapter implements it for
//!   real hardware.

pub mod fake;
pub mod link;
pub mod ports;
pub mod session;

#[cfg(feature = "hardware")]
pub mod espflash_link;

use flash_core::error::FlashError;
use flash_core::traits::{FlashBackend, FlashSession};
use flash_core::types::{ConnectionConfig, ProbeInfo, Transport};

pub use link::{EspDeviceInfo, EspLink, SECTOR_SIZE};
pub use session::EspSession;

/// Baud used when the connection does not ask for one.
///
/// 460800 is what `esptool.py` and ESP-IDF default to for flashing: fast enough
/// to matter on a multi-megabyte image, slow enough to be reliable on the
/// cheaper USB bridges.
pub const DEFAULT_BAUD: u32 = 460_800;

/// Opens the wire for a connection. Swapped out in tests.
type LinkFactory = Box<dyn Fn(&ConnectionConfig) -> Result<Box<dyn EspLink>, FlashError> + Send + Sync>;

pub struct EspSerialBackend {
    open_link: LinkFactory,
    /// Ports to report from `list_probes`. `None` means "enumerate for real".
    simulated_ports: Option<Vec<ProbeInfo>>,
}

impl std::fmt::Debug for EspSerialBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EspSerialBackend")
            .field("simulated", &self.simulated_ports.is_some())
            .finish()
    }
}

impl Default for EspSerialBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl EspSerialBackend {
    /// The real backend: enumerates serial ports and talks to actual silicon.
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "hardware")]
            open_link: Box::new(|config| {
                espflash_link::EspflashLink::open(config)
                    .map(|link| Box::new(link) as Box<dyn EspLink>)
            }),
            #[cfg(not(feature = "hardware"))]
            open_link: Box::new(|_| {
                Err(FlashError::Unsupported(
                    "this build has no serial support compiled in (the `hardware` feature is off)"
                        .to_string(),
                ))
            }),
            simulated_ports: None,
        }
    }

    /// A backend backed by an in-memory ESP bootloader, for tests and for
    /// exercising the ESP paths with no board attached.
    pub fn simulated() -> Self {
        Self {
            open_link: Box::new(|config| {
                let chip = if config.target_name.trim().is_empty()
                    || config.target_name.eq_ignore_ascii_case("auto")
                {
                    "esp32s3".to_string()
                } else {
                    config.target_name.to_lowercase()
                };
                Ok(Box::new(fake::FakeLink::new(chip)) as Box<dyn EspLink>)
            }),
            simulated_ports: Some(vec![ports::probe_info_for(
                "sim0",
                Some((0x303A, 0x1001)),
                Some("simulated".to_string()),
            )]),
        }
    }

    /// A backend whose wire is whatever the caller supplies.
    pub fn with_link_factory(factory: LinkFactory, ports: Vec<ProbeInfo>) -> Self {
        Self {
            open_link: factory,
            simulated_ports: Some(ports),
        }
    }
}

impl FlashBackend for EspSerialBackend {
    fn name(&self) -> &'static str {
        "esp-serial"
    }

    fn scheme(&self) -> &'static str {
        "esp"
    }

    fn list_probes(&self) -> Result<Vec<ProbeInfo>, FlashError> {
        Ok(match &self.simulated_ports {
            Some(ports) => ports.clone(),
            None => ports::list_serial_probes(),
        })
    }

    fn open_session(&self, config: &ConnectionConfig) -> Result<Box<dyn FlashSession>, FlashError> {
        let link = (self.open_link)(config)?;
        Ok(Box::new(EspSession::new(link)?))
    }
}

/// The baud a connection asks for, or the default.
///
/// A connection that still carries `Transport::DebugProbe` reached this backend
/// through a bare `esp:` identifier without transport parameters, which is
/// normal from the CLI; the default applies.
pub fn baud_for(config: &ConnectionConfig) -> u32 {
    match config.transport {
        Transport::Serial { baud, .. } => baud,
        _ => DEFAULT_BAUD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_are_addressed_with_the_esp_scheme() {
        let backend = EspSerialBackend::simulated();
        let probes = backend.list_probes().unwrap();
        assert!(probes.iter().all(|p| p.identifier.starts_with("esp:")));
    }

    #[test]
    fn the_target_name_selects_the_simulated_chip() {
        let backend = EspSerialBackend::simulated();
        let config = ConnectionConfig {
            probe_id: Some("esp:sim0".to_string()),
            target_name: "ESP32C6".to_string(),
            ..Default::default()
        };
        let session = backend.open_session(&config).unwrap();
        assert_eq!(session.target_info().unwrap().name, "esp32c6");
    }

    #[test]
    fn an_unspecified_transport_falls_back_to_the_default_baud() {
        assert_eq!(baud_for(&ConnectionConfig::default()), DEFAULT_BAUD);
        assert_eq!(
            baud_for(&ConnectionConfig::serial("COM7", 115_200, "auto")),
            115_200
        );
    }
}
