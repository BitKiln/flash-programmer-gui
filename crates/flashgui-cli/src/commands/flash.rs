use std::io::Write;

use flash_core::manager::FlashManager;
use flash_core::types::{ConnectionConfig, ProgramOptions, ResetType};

use crate::cli::{Cli, FlashArgs};
use crate::commands::{
    get_backend, open_session, persist_mock_session, record_history, resolve_flash_params,
    ResolvedFlash,
};
use crate::exit_codes::CliError;
use crate::output::CliProgressCallback;

pub fn handle_flash(
    cli: &Cli,
    args: &FlashArgs,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<(), CliError> {
    // 1. Resolve firmware, connection, and options: explicit flags win over the
    //    profile, which wins over the defaults.
    let resolved = resolve_flash_params(cli, args)?;
    let ResolvedFlash {
        file_path,
        serial,
        target,
        probe,
        transport,
        interface,
        speed,
        base_address,
        verify,
        reset,
        full_erase,
    } = resolved;

    // 2. Parse firmware image
    let firmware = firmware_parser::parse_file(&file_path, base_address)?;

    // 3. Open connection to probe & target
    let backend = get_backend(cli);
    let conn_config = ConnectionConfig {
        probe_id: probe,
        target_name: target,
        protocol: interface.into(),
        speed_khz: speed,
        connect_under_reset: false,
        reset_type: Some(ResetType::Software),
        transport,
    };

    let mut session = open_session(backend.as_ref(), &conn_config, cli.mock)?;

    // The history is written from what actually happened, so the facts that
    // describe the attempt are gathered before it starts.
    let history_probe = conn_config.probe_id.clone();
    let history_target = conn_config.target_name.clone();
    let history_path = file_path.clone();
    let history_crc = firmware.metadata.crc32;

    // 4. Setup progress telemetry callback
    let (callback, output_buf) = CliProgressCallback::new(cli.quiet, cli.json);

    let options = ProgramOptions {
        verify_after: verify,
        reset_after: reset,
        chip_erase: full_erase,
        chunk_size: 1024,
    };

    // 5. Execute flash lifecycle via FlashManager
    let result =
        FlashManager::execute_flash(session.as_mut(), &firmware, &options, Some(&callback));

    let return_val = match result {
        Ok(res) => {
            // A serial is stamped only once the image itself is on the board,
            // so a failed flash never leaves a numbered but unprogrammed unit.
            let serial_result = match serial {
                Some(ref config) => {
                    flash_core::program_serial(session.as_mut(), config, config.start)
                        .map(Some)
                        .map_err(CliError::from)
                }
                None => Ok(None),
            };
            persist_mock_session(session.as_mut(), cli.mock, conn_config.probe_id.as_deref());

            match serial_result {
                Ok(Some(ref stamped)) => callback.emit_serial(stamped),
                Ok(None) => {}
                Err(err) => {
                    callback.emit_error(err.exit_code(), err.message());
                    if let Ok(buf) = output_buf.lock() {
                        let _ = stdout.write_all(&buf);
                        let _ = stdout.flush();
                    }
                    return Err(err);
                }
            }
            let mut record = flash_core::HistoryRecord::now(
                flash_core::Operation::Flash,
                flash_core::Outcome::Succeeded,
                &history_target,
            );
            record.probe = history_probe.clone();
            record.file_path = Some(history_path.clone());
            record.image_crc32 = Some(history_crc);
            record.bytes = Some(res.bytes_flashed as u64);
            record.duration_ms = res.duration_ms;
            record.verified = res.verify_report.as_ref().is_some_and(|r| r.success);
            record.serial = match serial_result {
                Ok(Some(ref stamped)) => Some(stamped.clone()),
                _ => None,
            };
            record.message = res.message.clone();
            record_history(cli, record);

            callback.emit_complete(
                Some(res.bytes_flashed),
                res.verify_report.as_ref().map(|r| r.bytes_verified),
                res.duration_ms,
                &res.message,
            );
            Ok(())
        }
        Err(err) => {
            // A failure is the more important half of a history: a board that
            // was not programmed is the one someone comes back asking about.
            let cancelled = matches!(err, flash_core::FlashError::OperationCancelled);
            let cli_err = CliError::from(err);
            let mut record = flash_core::HistoryRecord::now(
                flash_core::Operation::Flash,
                if cancelled {
                    flash_core::Outcome::Cancelled
                } else {
                    flash_core::Outcome::Failed
                },
                &history_target,
            );
            record.probe = history_probe.clone();
            record.file_path = Some(history_path.clone());
            record.image_crc32 = Some(history_crc);
            record.message = cli_err.message().to_string();
            record_history(cli, record);

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
