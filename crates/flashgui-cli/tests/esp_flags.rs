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
    let (probe, transport) = resolve_transport(None, Some("COM7"), None).unwrap();
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
    let (probe, _) = resolve_transport(None, Some("/dev/ttyUSB0"), None).unwrap();
    assert_eq!(probe.as_deref(), Some("esp:/dev/ttyUSB0"));
}

#[test]
fn an_esp_probe_identifier_selects_the_serial_transport_on_its_own() {
    // `--probe esp:COM7` and `--port COM7` are the same request.
    let (probe, transport) = resolve_transport(Some("esp:COM7"), None, Some(115_200)).unwrap();
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
    let (probe, transport) = resolve_transport(Some("0483:374f:002E"), None, None).unwrap();
    assert_eq!(probe.as_deref(), Some("0483:374f:002E"));
    assert_eq!(transport, Transport::DebugProbe);
}

#[test]
fn baud_without_a_serial_target_is_refused_rather_than_ignored() {
    // Silently dropping the flag would leave someone convinced they had changed
    // the speed of a connection that has no baud rate at all.
    let err = resolve_transport(Some("0483:374f:002E"), None, Some(115_200)).unwrap_err();
    assert!(err.to_string().contains("--baud"));
}

#[test]
fn no_connection_flags_at_all_is_still_a_debug_probe() {
    let (probe, transport) = resolve_transport(None, None, None).unwrap();
    assert!(probe.is_none());
    assert_eq!(transport, Transport::DebugProbe);
}
