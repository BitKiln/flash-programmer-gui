//! Serial port discovery.
//!
//! A serial port is not self-describing the way a debug probe is: nothing on
//! the wire says "an ESP32 is behind me". The best available signal is the USB
//! VID/PID of the bridge chip on the board, so that is what is matched, and
//! anything unrecognised is still listed — just labelled as unknown rather than
//! hidden. Refusing to show a port because its bridge is not in a table would
//! make a working board look absent.

use flash_core::types::{ProbeInfo, ProbeType};

/// USB bridges found on ESP boards, and Espressif's own native USB.
///
/// Not an allow-list: a port that matches gets a name, a port that does not is
/// listed anyway.
const KNOWN_BRIDGES: &[(u16, u16, &str)] = &[
    (0x303A, 0x1001, "Espressif USB JTAG/serial"),
    (0x303A, 0x0002, "Espressif USB bridge"),
    (0x10C4, 0xEA60, "Silicon Labs CP210x"),
    (0x10C4, 0xEA70, "Silicon Labs CP2105"),
    (0x1A86, 0x7523, "WCH CH340"),
    (0x1A86, 0x55D4, "WCH CH9102"),
    (0x0403, 0x6001, "FTDI FT232"),
    (0x0403, 0x6010, "FTDI FT2232"),
    (0x0403, 0x6014, "FTDI FT232H"),
];

/// Whether this VID/PID is a bridge commonly found on ESP boards.
pub fn bridge_name(vid: u16, pid: u16) -> Option<&'static str> {
    KNOWN_BRIDGES
        .iter()
        .find(|(v, p, _)| *v == vid && *p == pid)
        .map(|(_, _, name)| *name)
}

/// Builds the `ProbeInfo` for one serial port.
///
/// `default_speed_khz` and `max_speed_khz` carry the **baud rate**, not a debug
/// clock; the fields are shared with the debug-probe model and there is nowhere
/// else to put them. The desktop app reads the transport, not these, to decide
/// what to show.
pub fn probe_info_for(
    port_name: &str,
    usb: Option<(u16, u16)>,
    serial_number: Option<String>,
) -> ProbeInfo {
    let (vendor, product) = match usb.and_then(|(v, p)| bridge_name(v, p)) {
        Some(name) => ("Espressif-compatible".to_string(), name.to_string()),
        None => (
            "Unknown".to_string(),
            format!("Serial port {port_name}"),
        ),
    };

    ProbeInfo {
        identifier: format!("esp:{port_name}"),
        vendor_name: vendor,
        product_name: product,
        serial_number,
        probe_type: ProbeType::Other("esp-serial".to_string()),
        // A serial bootloader speaks neither SWD nor JTAG.
        supported_protocols: Vec::new(),
        default_speed_khz: crate::DEFAULT_BAUD / 1000,
        max_speed_khz: 921_600 / 1000,
    }
}

/// Strips the `esp:` scheme from an identifier, giving the port name.
pub fn port_from_identifier(identifier: &str) -> &str {
    identifier.strip_prefix("esp:").unwrap_or(identifier)
}

/// Enumerates serial ports on this machine.
#[cfg(feature = "hardware")]
pub fn list_serial_probes() -> Vec<ProbeInfo> {
    let Ok(ports) = serialport::available_ports() else {
        // Enumeration can fail on a machine with no serial subsystem at all.
        // That is "no ports", not an error worth failing a scan over.
        return Vec::new();
    };

    ports
        .into_iter()
        .map(|port| {
            let (usb, serial) = match &port.port_type {
                serialport::SerialPortType::UsbPort(info) => {
                    ((Some((info.vid, info.pid))), info.serial_number.clone())
                }
                _ => (None, None),
            };
            probe_info_for(&port.port_name, usb, serial)
        })
        .collect()
}

#[cfg(not(feature = "hardware"))]
pub fn list_serial_probes() -> Vec<ProbeInfo> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_bridge_gets_a_readable_name() {
        let info = probe_info_for("COM7", Some((0x10C4, 0xEA60)), None);
        assert_eq!(info.identifier, "esp:COM7");
        assert_eq!(info.product_name, "Silicon Labs CP210x");
    }

    #[test]
    fn an_unknown_port_is_listed_rather_than_hidden() {
        let info = probe_info_for("/dev/ttyUSB9", None, None);
        assert_eq!(info.identifier, "esp:/dev/ttyUSB9");
        assert!(info.product_name.contains("/dev/ttyUSB9"));
    }

    #[test]
    fn a_serial_port_claims_no_debug_wire_protocol() {
        let info = probe_info_for("COM3", Some((0x303A, 0x1001)), None);
        assert!(info.supported_protocols.is_empty());
    }

    #[test]
    fn the_scheme_round_trips() {
        assert_eq!(port_from_identifier("esp:COM7"), "COM7");
        assert_eq!(port_from_identifier("COM7"), "COM7");
    }
}
