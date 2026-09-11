use std::io::Write;
use std::path::Path;

use flash_core::manager::FlashManager;
use flash_core::types::{ConnectionConfig, ProgramOptions, ResetType};

use crate::cli::{parse_address, Cli, FlashArgs, Protocol};
use crate::commands::{get_backend, is_supported_target, open_session, persist_mock_session};
use crate::exit_codes::CliError;
use crate::output::CliProgressCallback;
use crate::profile::load_profile;

pub fn handle_flash(
    cli: &Cli,
    args: &FlashArgs,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<(), CliError> {
    // 1. If a profile was specified, load it first
    let profile = if let Some(ref prof_name) = args.profile {
        let custom_file = cli.profile_file.as_deref().map(Path::new);
        Some(load_profile(prof_name, custom_file)?)
    } else {
        None
    };

    // 2. Resolve parameters with hierarchy: CLI explicit > Profile > Defaults
    let file_path = args
        .file
        .clone()
        .or_else(|| profile.as_ref().and_then(|p| p.default_path().map(ToString::to_string)))
        .ok_or_else(|| {
            CliError::InvalidArgsOrProfile(
                "No firmware file specified. Provide a file path or specify a profile with default_path.".to_string(),
            )
        })?;

    let target = args
        .target
        .clone()
        .or_else(|| profile.as_ref().map(|p| p.target().to_string()))
        .unwrap_or_else(|| "STM32F401RE".to_string());

    if !is_supported_target(&target, cli.mock) {
        return Err(CliError::TargetConnection(format!(
            "Target MCU '{}' is not supported by probe backend",
            target
        )));
    }

    let probe = args
        .probe
        .clone()
        .or_else(|| profile.as_ref().and_then(|p| p.probe_id().map(ToString::to_string)));

    let interface = args
        .interface
        .or_else(|| {
            profile.as_ref().and_then(|p| match p.interface().to_lowercase().as_str() {
                "swd" => Some(Protocol::Swd),
                "jtag" => Some(Protocol::Jtag),
                _ => None,
            })
        })
        .unwrap_or(Protocol::Swd);

    let speed = args
        .speed
        .or_else(|| profile.as_ref().map(|p| p.speed_khz()))
        .unwrap_or(2000);

    let base_address_str = args
        .base_address
        .clone()
        .or_else(|| profile.as_ref().and_then(|p| p.base_address().map(ToString::to_string)));

    let base_address = if let Some(ref addr_s) = base_address_str {
        Some(parse_address(addr_s)?)
    } else {
        None
    };

    let verify = if args.no_verify {
        false
    } else {
        args.verify
            .or_else(|| profile.as_ref().map(|p| p.verify_after()))
            .unwrap_or(true)
    };

    let reset = if args.no_reset {
        false
    } else {
        args.reset
            .or_else(|| profile.as_ref().map(|p| p.reset_after()))
            .unwrap_or(true)
    };

    let full_erase =
        args.full_erase || profile.as_ref().map(|p| p.full_chip_erase()).unwrap_or(false);

    // 3. Parse firmware image
    let firmware = firmware_parser::parse_file(&file_path, base_address)?;

    // 4. Open connection to probe & target
    let backend = get_backend(cli.mock);
    let conn_config = ConnectionConfig {
        probe_id: probe,
        target_name: target,
        protocol: interface.into(),
        speed_khz: speed,
        connect_under_reset: false,
        reset_type: Some(ResetType::Software),
    };

    let mut session = open_session(backend.as_ref(), &conn_config, cli.mock)?;

    // 5. Setup progress telemetry callback
    let (callback, output_buf) = CliProgressCallback::new(cli.quiet, cli.json);

    let options = ProgramOptions {
        verify_after: verify,
        reset_after: reset,
        chip_erase: full_erase,
        chunk_size: 1024,
    };

    // 6. Execute flash lifecycle via FlashManager
    let result = FlashManager::execute_flash(session.as_mut(), &firmware, &options, Some(&callback));

    let return_val = match result {
        Ok(res) => {
            persist_mock_session(session.as_mut(), cli.mock, conn_config.probe_id.as_deref());
            callback.emit_complete(
                Some(res.bytes_flashed),
                res.verify_report.as_ref().map(|r| r.bytes_verified),
                res.duration_ms,
                &res.message,
            );
            Ok(())
        }
        Err(err) => {
            let cli_err = CliError::from(err);
            callback.emit_error(cli_err.exit_code(), cli_err.message());
            Err(cli_err)
        }
    };

    // Flush callback output to stdout
    if let Ok(buf) = output_buf.lock() {
        let _ = stdout.write_all(&buf);
        let _ = stdout.flush();
    }

    return_val
}
