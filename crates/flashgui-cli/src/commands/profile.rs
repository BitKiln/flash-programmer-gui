use std::io::Write;
use std::path::Path;

use serde_json::json;

use crate::cli::{Cli, ProfileSubcommand};
use crate::exit_codes::CliError;
use crate::profile::{delete_profile, list_profiles, load_profile, save_profile, FlashProfile};

pub fn handle_profile(
    cli: &Cli,
    action: &ProfileSubcommand,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<(), CliError> {
    let custom_file = cli.profile_file.as_deref().map(Path::new);

    match action {
        ProfileSubcommand::Save {
            name,
            target,
            description,
            probe,
            interface,
            speed,
            firmware,
            base_address,
            verify,
            reset,
            full_erase,
        } => {
            let profile = FlashProfile::new(
                name.clone(),
                description.clone(),
                target.clone(),
                interface.to_string(),
                *speed,
                probe.clone(),
                firmware.clone(),
                base_address.clone(),
                verify.unwrap_or(true),
                reset.unwrap_or(true),
                *full_erase,
            );

            let saved_path = save_profile(&profile, custom_file)?;

            if cli.json {
                let payload = json!({
                    "status": "success",
                    "action": "save",
                    "name": name,
                    "path": saved_path.to_string_lossy(),
                    "profile": profile,
                });
                writeln!(stdout, "{}", serde_json::to_string_pretty(&payload).unwrap())
                    .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
            } else if !cli.quiet {
                writeln!(
                    stdout,
                    "✔ Profile '{}' saved successfully to '{}'",
                    name,
                    saved_path.display()
                )
                .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
            }
            Ok(())
        }

        ProfileSubcommand::Show { name } => {
            let profile = load_profile(name, custom_file)?;

            if cli.json {
                writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string_pretty(&profile).map_err(|e| {
                        CliError::InvalidArgsOrProfile(format!("JSON serialization error: {}", e))
                    })?
                )
                .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
            } else if !cli.quiet {
                let toml_str = toml::to_string_pretty(&profile).map_err(|e| {
                    CliError::InvalidArgsOrProfile(format!("TOML formatting error: {}", e))
                })?;
                writeln!(stdout, "Profile '{}':\n{}", name, toml_str)
                    .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
            }
            Ok(())
        }

        ProfileSubcommand::List => {
            let list = list_profiles(custom_file)?;

            if cli.json {
                writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string_pretty(&list).map_err(|e| {
                        CliError::InvalidArgsOrProfile(format!("JSON serialization error: {}", e))
                    })?
                )
                .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
            } else if !cli.quiet {
                if list.is_empty() {
                    writeln!(stdout, "No profiles found.")
                        .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                } else {
                    writeln!(stdout, "Available Profiles ({} found):", list.len())
                        .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                    writeln!(stdout, "{}", "-".repeat(60))
                        .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                    for p in &list {
                        writeln!(stdout, "  Name:        {}", p.name)
                            .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                        writeln!(stdout, "  Target:      {}", p.target)
                            .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                        if let Some(ref desc) = p.description {
                            writeln!(stdout, "  Description: {}", desc)
                                .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                        }
                        writeln!(stdout, "  Path:        {}", p.file_path)
                            .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                        writeln!(stdout, "{}", "-".repeat(60))
                            .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
                    }
                }
            }
            Ok(())
        }

        ProfileSubcommand::Delete { name } => {
            let path = delete_profile(name, custom_file)?;

            if cli.json {
                let payload = json!({
                    "status": "success",
                    "action": "delete",
                    "name": name,
                    "path": path.to_string_lossy(),
                });
                writeln!(stdout, "{}", serde_json::to_string_pretty(&payload).unwrap())
                    .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
            } else if !cli.quiet {
                writeln!(
                    stdout,
                    "✔ Profile '{}' deleted successfully (removed '{}')",
                    name,
                    path.display()
                )
                .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
            }
            Ok(())
        }
    }
}
