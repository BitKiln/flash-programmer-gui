//! OpenOCD backend, over its TCL RPC port.
//!
//! OpenOCD covers adapters and targets probe-rs does not: FTDI-based JTAG
//! cables, RISC-V parts, Xilinx and ESP32 JTAG, anything with a board file and
//! nothing else. It is also what a lot of vendor toolchains already have
//! running, and connecting to that process is cheaper than asking someone to
//! stop it.
//!
//! It is the third transport, and the one that makes the seam earn its keep:
//! OpenOCD is a separate *process* rather than a library. There is no handle to
//! hold, no callback to register, and no in-memory buffer to hand over — just
//! command strings, replies to parse, and files that OpenOCD opens itself.
//!
//! Layering mirrors the ESP backend:
//!
//! - [`OpenOcdBackend`] resolves an endpoint and opens sessions.
//! - [`session::OpenOcdSession`] holds the logic: geometry, alignment rules,
//!   progress, verification, the local-only interlock.
//! - [`tcl::TclLink`] is the wire, [`tcl::TcpTcl`] its TCP implementation.
//!   [`fake::FakeOpenOcd`] implements it in memory, so everything above runs in
//!   CI with no OpenOCD installed.

pub mod fake;
pub mod geometry;
pub mod session;
pub mod tcl;

use flash_core::error::FlashError;
use flash_core::traits::{FlashBackend, FlashSession};
use flash_core::types::{ConnectionConfig, ProbeInfo, ProbeType, Transport, WireProtocol};

pub use session::OpenOcdSession;
pub use tcl::{TclLink, TcpTcl, DEFAULT_PORT};

/// Opens the wire for a connection. Swapped out in tests.
type LinkFactory =
    Box<dyn Fn(&ConnectionConfig) -> Result<Box<dyn TclLink>, FlashError> + Send + Sync>;

pub struct OpenOcdBackend {
    open_link: LinkFactory,
    /// Endpoints to report from `list_probes`. `None` means the default one.
    advertised: Option<Vec<ProbeInfo>>,
}

impl std::fmt::Debug for OpenOcdBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenOcdBackend")
            .field("simulated", &self.advertised.is_some())
            .finish()
    }
}

impl Default for OpenOcdBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenOcdBackend {
    /// The real backend, connecting over TCP.
    pub fn new() -> Self {
        Self {
            open_link: Box::new(|config| {
                let endpoint = endpoint_for(config);
                TcpTcl::connect(&endpoint).map(|link| Box::new(link) as Box<dyn TclLink>)
            }),
            advertised: None,
        }
    }

    /// A backend backed by an in-memory OpenOCD, for tests and for exercising
    /// these paths with nothing installed.
    pub fn simulated() -> Self {
        Self {
            open_link: Box::new(|_| Ok(Box::new(fake::FakeOpenOcd::new()) as Box<dyn TclLink>)),
            advertised: Some(vec![probe_info_for("127.0.0.1:6666", true)]),
        }
    }

    /// A backend whose wire is whatever the caller supplies.
    pub fn with_link_factory(factory: LinkFactory, advertised: Vec<ProbeInfo>) -> Self {
        Self {
            open_link: factory,
            advertised: Some(advertised),
        }
    }
}

impl FlashBackend for OpenOcdBackend {
    fn name(&self) -> &'static str {
        "openocd"
    }

    fn scheme(&self) -> &'static str {
        "openocd"
    }

    /// Advertises the default endpoint without connecting to it.
    ///
    /// Probing the port here would be a lie in either direction: OpenOCD can be
    /// started a second after this list is drawn, and a port that answers is
    /// not proof that the adapter behind it is attached. The entry says where
    /// this backend would look, and connecting is what finds out.
    fn list_probes(&self) -> Result<Vec<ProbeInfo>, FlashError> {
        Ok(match &self.advertised {
            Some(entries) => entries.clone(),
            None => vec![probe_info_for(&format!("127.0.0.1:{DEFAULT_PORT}"), false)],
        })
    }

    fn open_session(&self, config: &ConnectionConfig) -> Result<Box<dyn FlashSession>, FlashError> {
        let link = (self.open_link)(config)?;
        Ok(Box::new(OpenOcdSession::new(link, &config.target_name)?))
    }
}

/// The endpoint a connection names, or the default.
///
/// Three spellings reach here and all mean the same thing: an
/// `openocd:host:port` identifier from the CLI or a saved profile, an
/// `Rpc { endpoint }` transport from a caller that builds the config itself,
/// and nothing at all.
pub fn endpoint_for(config: &ConnectionConfig) -> String {
    if let Transport::Rpc { ref endpoint } = config.transport {
        if !endpoint.trim().is_empty() {
            return endpoint.clone();
        }
    }
    match config.probe_id.as_deref() {
        Some(id) => {
            let trimmed = id.strip_prefix("openocd:").unwrap_or(id).trim();
            if trimmed.is_empty() {
                format!("127.0.0.1:{DEFAULT_PORT}")
            } else {
                trimmed.to_string()
            }
        }
        None => format!("127.0.0.1:{DEFAULT_PORT}"),
    }
}

/// The list entry for an OpenOCD endpoint.
fn probe_info_for(endpoint: &str, simulated: bool) -> ProbeInfo {
    ProbeInfo {
        identifier: format!("openocd:{endpoint}"),
        vendor_name: "OpenOCD".to_string(),
        product_name: if simulated {
            format!("OpenOCD at {endpoint} (simulated)")
        } else {
            format!("OpenOCD at {endpoint}")
        },
        serial_number: None,
        // Whatever adapter OpenOCD drives, this end of it is a TCP socket.
        probe_type: ProbeType::Other("OpenOCD".to_string()),
        // OpenOCD's configuration decides the wire, not this program: the
        // transport is chosen in its board file before it starts listening.
        supported_protocols: vec![WireProtocol::Swd, WireProtocol::Jtag],
        default_speed_khz: 0,
        max_speed_khz: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_are_addressed_with_the_openocd_scheme() {
        let backend = OpenOcdBackend::simulated();
        let probes = backend.list_probes().unwrap();
        assert!(probes.iter().all(|p| p.identifier.starts_with("openocd:")));
    }

    #[test]
    fn an_identifier_a_transport_or_nothing_all_name_an_endpoint() {
        let from_identifier = ConnectionConfig {
            probe_id: Some("openocd:192.168.1.5:4444".to_string()),
            ..Default::default()
        };
        assert_eq!(endpoint_for(&from_identifier), "192.168.1.5:4444");

        let from_transport = ConnectionConfig {
            transport: Transport::Rpc {
                endpoint: "localhost:7000".to_string(),
            },
            ..Default::default()
        };
        assert_eq!(endpoint_for(&from_transport), "localhost:7000");

        assert_eq!(
            endpoint_for(&ConnectionConfig::default()),
            format!("127.0.0.1:{DEFAULT_PORT}")
        );
    }

    #[test]
    fn a_transport_endpoint_wins_over_the_identifier() {
        // The transport is the more specific statement: an identifier can come
        // from a saved profile, while the transport was built for this run.
        let config = ConnectionConfig {
            probe_id: Some("openocd:127.0.0.1:6666".to_string()),
            transport: Transport::Rpc {
                endpoint: "otherhost:4444".to_string(),
            },
            ..Default::default()
        };
        assert_eq!(endpoint_for(&config), "otherhost:4444");
    }

    #[test]
    fn a_bare_scheme_falls_back_to_the_default_endpoint() {
        let config = ConnectionConfig {
            probe_id: Some("openocd:".to_string()),
            ..Default::default()
        };
        assert_eq!(endpoint_for(&config), format!("127.0.0.1:{DEFAULT_PORT}"));
    }

    #[test]
    fn the_simulated_backend_opens_a_session_with_geometry() {
        let backend = OpenOcdBackend::simulated();
        let config = ConnectionConfig {
            probe_id: Some("openocd:127.0.0.1:6666".to_string()),
            target_name: "auto".to_string(),
            ..Default::default()
        };
        let session = backend.open_session(&config).unwrap();
        let target = session.target_info().expect("a session knows its target");

        assert_eq!(target.flash_base, fake::FLASH_BASE);
        assert_eq!(target.flash_size, 128 * 1024);
        assert_eq!(target.sectors.len(), 5);
        assert_eq!(target.ram_base, fake::RAM_BASE);
        assert_eq!(target.architecture, "cortex_m");
        // "auto" takes OpenOCD's own name for the target.
        assert_eq!(target.name, "sim.cpu");
    }
}
