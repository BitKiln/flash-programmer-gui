//! Reading and writing target memory directly.
//!
//! This is the bus, not the flash controller: RAM, peripheral registers, and
//! memory-mapped configuration such as option bytes. Nothing here erases, so a
//! backend refuses an address inside flash and points at `flash` instead.

use std::io::Write;

use flash_core::types::{ConnectionConfig, ResetType};

use crate::cli::{parse_address, Cli, MemoryArgs, MemorySubcommand};
use crate::commands::{get_backend, is_supported_target, open_session, persist_mock_session};
use crate::exit_codes::CliError;

/// Parses a hex byte string such as `DEADBEEF` or `de ad be ef`.
///
/// Whitespace and `0x` are allowed because people paste from datasheets and
/// from debuggers, and both spell the same bytes differently. An odd number of
/// digits is refused rather than padded: a missing nibble means the intended
/// value is unknown, and guessing which end it belongs on would write the
/// wrong byte.
pub fn parse_hex_bytes(text: &str) -> Result<Vec<u8>, CliError> {
    let cleaned: String = text
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X")
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != ',')
        .collect();

    if cleaned.is_empty() {
        return Err(CliError::InvalidArgsOrProfile(
            "No bytes given: --data takes hex digits, such as DEADBEEF".to_string(),
        ));
    }
    if !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CliError::InvalidArgsOrProfile(format!(
            "Invalid hex byte string '{text}': only hex digits, whitespace and '_' are allowed"
        )));
    }
    if !cleaned.len().is_multiple_of(2) {
        return Err(CliError::InvalidArgsOrProfile(format!(
            "Hex byte string '{text}' has an odd number of digits ({}), so one byte is incomplete",
            cleaned.len()
        )));
    }

    (0..cleaned.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&cleaned[i..i + 2], 16).map_err(|e| {
                CliError::InvalidArgsOrProfile(format!("Invalid hex byte string '{text}': {e}"))
            })
        })
        .collect()
}

/// Formats `data` as a hex dump with an address column and ASCII gutter.
fn hex_dump(address: u32, data: &[u8]) -> String {
    let mut out = String::new();
    for (row, chunk) in data.chunks(16).enumerate() {
        let row_addr = address.saturating_add((row * 16) as u32);
        out.push_str(&format!("{row_addr:08X}  "));
        for i in 0..16 {
            match chunk.get(i) {
                Some(b) => out.push_str(&format!("{b:02X} ")),
                None => out.push_str("   "),
            }
            if i == 7 {
                out.push(' ');
            }
        }
        out.push_str(" |");
        for b in chunk {
            out.push(if b.is_ascii_graphic() || *b == b' ' {
                *b as char
            } else {
                '.'
            });
        }
        out.push_str("|\n");
    }
    out
}

pub fn handle_memory(
    cli: &Cli,
    args: &MemoryArgs,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<(), CliError> {
    if !is_supported_target(&args.target, cli.mock) {
        return Err(CliError::TargetConnection(format!(
            "Target MCU '{}' is not supported by probe backend",
            args.target
        )));
    }

    let backend = get_backend(cli);
    let (probe_id, transport) = crate::commands::resolve_transport(
        args.probe.as_deref(),
        args.port.as_deref(),
        args.baud,
        args.openocd.as_deref(),
    )?;
    let conn_config = ConnectionConfig {
        probe_id,
        target_name: args.target.clone(),
        protocol: args.interface.into(),
        speed_khz: args.speed,
        connect_under_reset: false,
        reset_type: Some(ResetType::Software),
        transport,
    };

    let mut session = open_session(backend.as_ref(), &conn_config, cli.mock)?;

    match args.action {
        MemorySubcommand::Read {
            ref address,
            ref length,
            ref out,
        } => {
            let address = parse_address(address)?;
            let length = parse_address(length)?;
            let data = session.read_memory(address, length)?;

            if let Some(path) = out {
                std::fs::write(path, &data).map_err(|e| {
                    CliError::InvalidArgsOrProfile(format!("Could not write '{path}': {e}"))
                })?;
                writeln!(
                    stdout,
                    "Read {} bytes from 0x{address:08X} into {path}",
                    data.len()
                )
                .ok();
            } else if cli.json {
                writeln!(
                    stdout,
                    "{}",
                    serde_json::json!({
                        "address": address,
                        "length": data.len(),
                        "bytes": data.iter().map(|b| format!("{b:02X}")).collect::<String>(),
                    })
                )
                .ok();
            } else {
                write!(stdout, "{}", hex_dump(address, &data)).ok();
            }
        }

        MemorySubcommand::Write {
            ref address,
            ref data,
            ref file,
        } => {
            if !session.can_write_memory() {
                return Err(CliError::TargetConnection(
                    "This connection cannot write target memory directly: it reaches the \
                     flash controller only. A debug probe is needed for RAM, registers and \
                     option bytes."
                        .to_string(),
                ));
            }

            let address = parse_address(address)?;
            let bytes = match (data, file) {
                (Some(text), None) => parse_hex_bytes(text)?,
                (None, Some(path)) => std::fs::read(path).map_err(|e| {
                    CliError::InvalidArgsOrProfile(format!("Could not read '{path}': {e}"))
                })?,
                (Some(_), Some(_)) => {
                    return Err(CliError::InvalidArgsOrProfile(
                        "Give --data or --file, not both".to_string(),
                    ))
                }
                (None, None) => {
                    return Err(CliError::InvalidArgsOrProfile(
                        "Nothing to write: give --data DEADBEEF or --file bytes.bin".to_string(),
                    ))
                }
            };

            session.write_memory(address, &bytes).map_err(|e| match e {
                flash_core::FlashError::InvalidAddress { .. }
                | flash_core::FlashError::AddressOutOfBounds { .. } => {
                    CliError::InvalidArgsOrProfile(e.to_string())
                }
                other => CliError::from(other),
            })?;
            persist_mock_session(session.as_mut(), cli.mock, conn_config.probe_id.as_deref());
            writeln!(stdout, "Wrote {} bytes to 0x{address:08X}", bytes.len()).ok();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_digits_parse_however_they_are_spaced() {
        let expected = vec![0xDE, 0xAD, 0xBE, 0xEF];
        for text in ["DEADBEEF", "de ad be ef", "0xDEADBEEF", "DE_AD_BE_EF"] {
            assert_eq!(parse_hex_bytes(text).unwrap(), expected, "for {text:?}");
        }
    }

    #[test]
    fn an_odd_number_of_digits_is_refused_rather_than_padded() {
        let err = parse_hex_bytes("ABC").unwrap_err();
        assert!(
            err.message().contains("incomplete"),
            "got {:?}",
            err.message()
        );
    }

    #[test]
    fn a_non_hex_string_is_refused() {
        assert!(parse_hex_bytes("hello!").is_err());
        assert!(parse_hex_bytes("").is_err());
    }

    #[test]
    fn the_dump_shows_the_address_the_bytes_and_the_ascii() {
        let dump = hex_dump(0x2000_0000, b"Hi\x00");
        assert!(dump.starts_with("20000000  48 69 00 "), "got {dump:?}");
        assert!(dump.trim_end().ends_with("|Hi.|"), "got {dump:?}");
    }
}
