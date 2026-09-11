use std::io::Write;
use std::time::Instant;

use flash_core::types::{ConnectionConfig, ResetType};

use crate::cli::{parse_address, Cli, EraseArgs};
use crate::commands::{get_backend, is_supported_target, open_session, persist_mock_session};
use crate::exit_codes::CliError;
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

    let backend = get_backend(cli.mock);
    let conn_config = ConnectionConfig {
        probe_id: args.probe.clone(),
        target_name: args.target.clone(),
        protocol: args.interface.into(),
        speed_khz: args.speed,
        connect_under_reset: false,
        reset_type: Some(ResetType::Software),
    };

    let mut session = open_session(backend.as_ref(), &conn_config, cli.mock)?;
    let (callback, output_buf) = CliProgressCallback::new(cli.quiet, cli.json);
    let start_time = Instant::now();

    let res = if args.full {
        session.erase_all(Some(&callback))
    } else if let Some(ref addr_s) = args.address {
        let addr = parse_address(addr_s)?;
        let len = args.length.unwrap_or(1024);
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
                    args.length.unwrap_or(1024),
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
