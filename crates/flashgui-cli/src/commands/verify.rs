use std::io::Write;
use std::time::Instant;

use flash_core::error::FlashError;
use flash_core::types::{ConnectionConfig, ResetType};

use crate::cli::{parse_address, Cli, VerifyArgs};
use crate::commands::{get_backend, is_supported_target, open_session};
use crate::exit_codes::CliError;
use crate::output::CliProgressCallback;

pub fn handle_verify(
    cli: &Cli,
    args: &VerifyArgs,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<(), CliError> {
    if !is_supported_target(&args.target, cli.mock) {
        return Err(CliError::TargetConnection(format!(
            "Target MCU '{}' is not supported by probe backend",
            args.target
        )));
    }

    let base_address = if let Some(ref addr_s) = args.base_address {
        Some(parse_address(addr_s)?)
    } else {
        None
    };

    let firmware = firmware_parser::parse_file(&args.file, base_address)?;

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

    let result = session.verify(&firmware.segments, Some(&callback));

    let return_val = match result {
        Ok(report) => {
            let duration_ms = start_time.elapsed().as_millis() as u64;
            if !report.success {
                let err_msg = if let Some(mismatch) = report.mismatches.first() {
                    format!(
                        "Verification mismatch at 0x{:08X}: expected 0x{:02X}, read 0x{:02X}",
                        mismatch.address, mismatch.expected, mismatch.actual
                    )
                } else if report.checksum_expected != report.checksum_actual {
                    format!(
                        "Verification checksum mismatch: expected CRC 0x{:08X}, read 0x{:08X}",
                        report.checksum_expected, report.checksum_actual
                    )
                } else {
                    "Verification failed".to_string()
                };
                let cli_err = CliError::FlashVerify(err_msg);
                callback.emit_error(cli_err.exit_code(), cli_err.message());
                Err(cli_err)
            } else {
                let message = format!(
                    "Memory verified successfully ({} bytes, CRC32: 0x{:08X}) in {} ms",
                    report.bytes_verified, report.checksum_actual, duration_ms
                );
                callback.emit_complete(None, Some(report.bytes_verified), duration_ms, &message);
                Ok(())
            }
        }
        Err(err) => {
            let cli_err = match err {
                FlashError::VerificationMismatch {
                    address,
                    expected,
                    actual,
                } => CliError::FlashVerify(format!(
                    "Verification mismatch at 0x{:08X}: expected 0x{:02X}, read 0x{:02X}",
                    address, expected, actual
                )),
                FlashError::ChecksumMismatch { expected, actual } => {
                    CliError::FlashVerify(format!(
                        "Verification checksum mismatch: expected CRC 0x{:08X}, read 0x{:08X}",
                        expected, actual
                    ))
                }
                _ => CliError::from(err),
            };
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
