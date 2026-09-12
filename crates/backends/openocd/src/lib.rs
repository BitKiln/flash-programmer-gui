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

    /// Lists the default endpoint only when something is listening on it.
    ///
    /// Listing it unconditionally was worse than the alternative: every caller
    /// that groups probes by scheme showed a phantom entry, and a UI that
    /// auto-selects the first one would put it ahead of an attached debug
    /// probe. A socket that answers is not proof that OpenOCD is behind it or
    /// that an adapter is attached to it, but it is the same standard the other
    /// transports list by -- a serial port is listed because it exists, not
    /// because a board is on the other end.
    ///
    /// An endpoint that is not listening is still reachable by naming it:
    /// `--openocd <endpoint>`, or the desktop app's OpenOCD mode, neither of
    /// which goes through this list. OpenOCD started a moment from now appears
    /// on the next scan.
    fn list_probes(&self) -> Result<Vec<ProbeInfo>, FlashError> {
        if let Some(entries) = &self.advertised {
            return Ok(entries.clone());
        }
        let endpoint = format!("127.0.0.1:{DEFAULT_PORT}");
        if tcl::is_listening(&endpoint) {
            return Ok(vec![probe_info_for(&endpoint, false)]);
        }
        Ok(Vec::new())
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
    fn nothing_is_listed_when_nothing_is_listening() {
        // The phantom entry this avoids was worse than the absence: grouped by
        // scheme it looked like an attached probe, and an auto-selecting UI
        // preferred it to a real one.
        let backend = OpenOcdBackend::new();
        let probes = backend.list_probes().expect("a scan never fails");
        for probe in &probes {
            assert!(
                probe.identifier.starts_with("openocd:"),
                "anything listed must route back here: {}",
                probe.identifier
            );
        }
    }

    #[test]
    fn a_listening_endpoint_is_listed() {
        use std::net::TcpListener;

        // Bind the default TCL port if it is free, so the listing has
        // something to find. If it is already taken -- by an actual OpenOCD, or
        // by anything else -- the listing should find that instead, and either
        // way the assertion below holds.
        let _listener = TcpListener::bind(format!("127.0.0.1:{DEFAULT_PORT}")).ok();
        let backend = OpenOcdBackend::new();
        let probes = backend.list_probes().unwrap();
        assert_eq!(
            probes.len(),
            1,
            "something is listening on {DEFAULT_PORT}, so the endpoint belongs in the list"
        );
        assert_eq!(
            probes[0].identifier,
            format!("openocd:127.0.0.1:{DEFAULT_PORT}")
        );
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
