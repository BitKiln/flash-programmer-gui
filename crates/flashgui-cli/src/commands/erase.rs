use std::io::Write;
use std::time::Instant;

use flash_core::types::{ConnectionConfig, ResetType};

use crate::cli::{parse_address, Cli, EraseArgs};
use crate::commands::{get_backend, is_supported_target, open_session, persist_mock_session};
use crate::exit_codes::CliError;

/// The erase length as given, or the default, for the completion message.
fn len_for_message(args: &EraseArgs) -> Result<u32, CliError> {
    match args.length.as_deref() {
        Some(text) => parse_address(text),
        None => Ok(1024),
    }
}
use crate::output::CliProgressCallback;

pub fn handle_erase(
    cli: &Cli,
    args: &EraseArgs,
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
    let (callback, output_buf) = CliProgressCallback::new(cli.quiet, cli.json);
    let start_time = Instant::now();

    let res = if args.full {
        session.erase_all(Some(&callback))
    } else if let Some(ref addr_s) = args.address {
        let addr = parse_address(addr_s)?;
        // Hex here as well as in --address: writing one in hex and the other
        // in decimal is the sort of inconsistency that produces a wrong erase.
        let len = match args.length.as_deref() {
            Some(text) => parse_address(text)?,
            None => 1024,
        };
        session.erase_range(addr, len, Some(&callback))
    } else {
        session.erase_all(Some(&callback))
    };

    let return_val = match res {
        Ok(()) => {
            persist_mock_session(session.as_mut(), cli.mock, conn_config.probe_id.as_deref());
            let duration_ms = start_time.elapsed().as_millis() as u64;
            let message = if args.full || args.address.is_none() {
                format!(
                    "Full chip erase completed successfully in {} ms",
                    duration_ms
                )
            } else {
                format!(
                    "Erase of range 0x{:08X} ({} bytes) completed successfully in {} ms",
                    parse_address(args.address.as_deref().unwrap_or("0"))?,
                    len_for_message(args)?,
                    duration_ms
                )
            };
            callback.emit_complete(None, None, duration_ms, &message);
            Ok(())
        }
        Err(err) => {
            let cli_err = CliError::from(err);
            callback.emit_error(cli_err.exit_code(), cli_err.message());
            Err(cli_err)
        }
    };

    if let Ok(buf) = output_buf.lock() {
        let _ = stdout.write_all(&buf);
        let _ = stdout.flush();
    }

    return_val
}
