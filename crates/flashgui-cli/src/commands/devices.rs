use std::io::Write;

use crate::cli::Cli;
use crate::commands::get_backend;
use crate::exit_codes::CliError;

pub fn handle_devices(
    cli: &Cli,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<(), CliError> {
    let backend = get_backend(cli.mock);
    let probes = backend.list_probes()?;

    if cli.json {
        let json_str = serde_json::to_string_pretty(&probes).map_err(|e| {
            CliError::InvalidArgsOrProfile(format!("Failed to serialize probes to JSON: {}", e))
        })?;
        writeln!(stdout, "{}", json_str).map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
    } else if !cli.quiet {
        if probes.is_empty() {
            writeln!(
                stdout,
                "No debug probes detected. (Use --mock to view virtual simulated probes)"
            )
            .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
        } else {
            writeln!(stdout, "Connected Debug Probes ({} found):", probes.len())
                .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
            writeln!(stdout, "{}", "-".repeat(60))
                .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
            for (idx, p) in probes.iter().enumerate() {
                writeln!(stdout, "  [{}] {}", idx + 1, p.identifier)
                    .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                writeln!(stdout, "      Product:     {}", p.product_name)
                    .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                writeln!(stdout, "      Vendor:      {}", p.vendor_name)
                    .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                if let Some(ref serial) = p.serial_number {
                    writeln!(stdout, "      Serial:      {}", serial)
                        .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                }
                let proto_strs: Vec<String> = p
                    .supported_protocols
                    .iter()
                    .map(|proto| format!("{:?}", proto))
                    .collect();
                writeln!(
                    stdout,
                    "      Protocols:   {}",
                    proto_strs.join(", ")
                )
                .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                writeln!(
                    stdout,
                    "      Speed:       {} kHz (max {} kHz)",
                    p.default_speed_khz, p.max_speed_khz
                )
                .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
            }
            writeln!(stdout, "{}", "-".repeat(60))
                .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
        }
    }

    Ok(())
}
