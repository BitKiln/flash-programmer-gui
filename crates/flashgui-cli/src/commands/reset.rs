use std::io::Write;
use std::time::Instant;

use flash_core::types::{ConnectionConfig, ResetType};

use crate::cli::{Cli, ResetArgs};
use crate::commands::{get_backend, is_supported_target, open_session};
use crate::exit_codes::CliError;
use crate::output::CliProgressCallback;

pub fn handle_reset(
    cli: &Cli,
    args: &ResetArgs,
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

    let result = session.reset(args.halt);

    let return_val = match result {
        Ok(()) => {
            let duration_ms = start_time.elapsed().as_millis() as u64;
            let message = format!(
                "Target MCU '{}' reset successfully (halt: {}) in {} ms",
                args.target, args.halt, duration_ms
            );
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
