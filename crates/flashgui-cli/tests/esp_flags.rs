//! The connection flags that select a serial target.
//!
//! These check the argument layer only — that `--port` becomes an `esp:`
//! identifier and `--baud` becomes a serial transport — without opening a port.
//! The behaviour behind them is covered by the ESP backend's own tests against
//! its in-memory bootloader.

use flash_core::types::Transport;
use flashgui_cli::commands::{resolve_transport, DEFAULT_SERIAL_BAUD};

#[test]
fn a_port_becomes_an_esp_identifier() {
    let (probe, transport) = resolve_transport(None, Some("COM7"), None, None).unwrap();
    assert_eq!(probe.as_deref(), Some("esp:COM7"));
    assert_eq!(
        transport,
        Transport::Serial {
            baud: DEFAULT_SERIAL_BAUD,
            controls_reset: true
        }
    );
}

#[test]
fn a_unix_device_path_survives_the_scheme_prefix() {
    let (probe, _) = resolve_transport(None, Some("/dev/ttyUSB0"), None, None).unwrap();
    assert_eq!(probe.as_deref(), Some("esp:/dev/ttyUSB0"));
}

#[test]
fn an_esp_probe_identifier_selects_the_serial_transport_on_its_own() {
    // `--probe esp:COM7` and `--port COM7` are the same request.
    let (probe, transport) =
        resolve_transport(Some("esp:COM7"), None, Some(115_200), None).unwrap();
    assert_eq!(probe.as_deref(), Some("esp:COM7"));
    assert_eq!(
        transport,
        Transport::Serial {
            baud: 115_200,
            controls_reset: true
        }
    );
}

#[test]
fn a_debug_probe_stays_a_debug_probe() {
    let (probe, transport) = resolve_transport(Some("0483:374f:002E"), None, None, None).unwrap();
    assert_eq!(probe.as_deref(), Some("0483:374f:002E"));
    assert_eq!(transport, Transport::DebugProbe);
}

#[test]
fn baud_without_a_serial_target_is_refused_rather_than_ignored() {
    // Silently dropping the flag would leave someone convinced they had changed
    // the speed of a connection that has no baud rate at all.
    let err = resolve_transport(Some("0483:374f:002E"), None, Some(115_200), None).unwrap_err();
    assert!(err.to_string().contains("--baud"));
}

#[test]
fn no_connection_flags_at_all_is_still_a_debug_probe() {
    let (probe, transport) = resolve_transport(None, None, None, None).unwrap();
    assert!(probe.is_none());
    assert_eq!(transport, Transport::DebugProbe);
}

#[test]
fn an_openocd_endpoint_becomes_an_openocd_identifier() {
    let (probe, transport) = resolve_transport(None, None, None, Some("localhost:6666")).unwrap();
    assert_eq!(probe.as_deref(), Some("openocd:localhost:6666"));
    assert_eq!(
        transport,
        Transport::Rpc {
            endpoint: "localhost:6666".to_string()
        }
    );
}

#[test]
fn a_bare_port_is_an_openocd_endpoint_too() {
    // `--openocd 4444` is what someone with a non-default TCL port writes.
    let (probe, transport) = resolve_transport(None, None, None, Some("4444")).unwrap();
    assert_eq!(probe.as_deref(), Some("openocd:4444"));
    assert_eq!(
        transport,
        Transport::Rpc {
            endpoint: "4444".to_string()
        }
    );
}

#[test]
fn an_openocd_probe_identifier_selects_the_rpc_transport_on_its_own() {
    // `--probe openocd:6666` and `--openocd 6666` are the same request, and a
    // saved profile stores the identifier form.
    let (probe, transport) = resolve_transport(Some("openocd:6666"), None, None, None).unwrap();
    assert_eq!(probe.as_deref(), Some("openocd:6666"));
    assert_eq!(
        transport,
        Transport::Rpc {
            endpoint: "6666".to_string()
        }
    );
}

#[test]
fn baud_with_openocd_is_refused_and_says_why() {
    // OpenOCD's configuration decides the wire and its speed before it starts
    // listening, so accepting the flag would imply a control this program does
    // not have.
    let err = resolve_transport(None, None, Some(115_200), Some("6666")).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("--baud"), "got {message}");
    assert!(message.contains("OpenOCD"), "got {message}");
}
