use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use flash_core::batch::{
    run_batch_with, BatchConfig, BatchEvent, BatchObserver, BatchReport, RearmPolicy, StopReason,
    UnitStatus,
};
use flash_core::types::{ConnectionConfig, ProgramOptions, ResetType};
use serde::Serialize;

use crate::cli::{BatchArgs, Cli, Rearm};
use crate::commands::{
    get_backend, open_session, persist_mock_session, resolve_flash_params, ResolvedFlash,
};
use crate::exit_codes::CliError;
use crate::output::CliProgressCallback;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
enum BatchNdJson<'a> {
    #[serde(rename = "batch_waiting")]
    Waiting { index: u32, waiting_for: &'a str },
    #[serde(rename = "batch_unit_started")]
    UnitStarted { index: u32 },
    #[serde(rename = "batch_unit_finished")]
    UnitFinished {
        index: u32,
        status: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        serial: Option<&'a str>,
        bytes_flashed: u32,
        verified: bool,
        duration_ms: u64,
        message: &'a str,
    },
    #[serde(rename = "batch_complete")]
    Complete {
        passed: u32,
        failed: u32,
        total: u32,
        duration_ms: u64,
        stop_reason: &'a StopReason,
    },
}

/// Prints batch progress to the shared output buffer, and carries the
/// Ctrl-C style cancellation flag.
struct CliBatchObserver {
    quiet: bool,
    json: bool,
    output_buf: Arc<Mutex<Vec<u8>>>,
    cancelled: Arc<AtomicBool>,
}

impl CliBatchObserver {
    fn write_line(&self, line: &str) {
        if let Ok(mut w) = self.output_buf.lock() {
            let _ = writeln!(w, "{}", line);
        }
    }

    fn write_json(&self, msg: &BatchNdJson<'_>) {
        if let Ok(json_str) = serde_json::to_string(msg) {
            self.write_line(&json_str);
        }
    }
}

impl BatchObserver for CliBatchObserver {
    fn on_batch_event(&self, event: BatchEvent) {
        match event {
            BatchEvent::WaitingForDetach { index } => {
                if self.json {
                    self.write_json(&BatchNdJson::Waiting {
                        index,
                        waiting_for: "detach",
                    });
                } else if !self.quiet {
                    self.write_line(&format!(
                        "[BATCH] Unit {}: disconnect the programmed board...",
                        index.saturating_sub(1)
                    ));
                }
            }
            BatchEvent::WaitingForAttach { index } => {
                if self.json {
                    self.write_json(&BatchNdJson::Waiting {
                        index,
                        waiting_for: "attach",
                    });
                } else if !self.quiet {
                    self.write_line(&format!("[BATCH] Unit {}: connect the next board...", index));
                }
            }
            BatchEvent::UnitStarted { index, .. } => {
                if self.json {
                    self.write_json(&BatchNdJson::UnitStarted { index });
                } else if !self.quiet {
                    self.write_line(&format!("[BATCH] Unit {} started", index));
                }
            }
            BatchEvent::UnitFinished(record) => {
                if self.json {
                    self.write_json(&BatchNdJson::UnitFinished {
                        index: record.index,
                        status: record.status.as_str(),
                        serial: record.serial.as_deref(),
                        bytes_flashed: record.bytes_flashed,
                        verified: record.verified,
                        duration_ms: record.duration_ms,
                        message: &record.message,
                    });
                } else if !self.quiet {
                    let mark = match record.status {
                        UnitStatus::Passed => "PASS",
                        UnitStatus::Failed => "FAIL",
                    };
                    let serial = match record.serial {
                        Some(ref value) => format!(" serial {}", value),
                        None => String::new(),
                    };
                    self.write_line(&format!(
                        "[BATCH] Unit {} {}{} ({} bytes, {} ms): {}",
                        record.index,
                        mark,
                        serial,
                        record.bytes_flashed,
                        record.duration_ms,
                        record.message
                    ));
                }
            }
            BatchEvent::BatchFinished { .. } => {
                // The summary is printed by the command once the report is in
                // hand, so it can include totals and the log file path.
            }
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

pub fn handle_batch(
    cli: &Cli,
    args: &BatchArgs,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<(), CliError> {
    let resolved: ResolvedFlash = resolve_flash_params(cli, &args.flash)?;

    if let Some(0) = args.count {
        return Err(CliError::InvalidArgsOrProfile(
            "Batch --count must be at least 1".to_string(),
        ));
    }

    let firmware = firmware_parser::parse_file(&resolved.file_path, resolved.base_address)?;

    let backend = get_backend(cli.mock);
    let connection = ConnectionConfig {
        probe_id: resolved.probe.clone(),
        target_name: resolved.target.clone(),
        protocol: resolved.interface.into(),
        speed_khz: resolved.speed,
        connect_under_reset: false,
        reset_type: Some(ResetType::Software),
        transport: resolved.transport.clone(),
    };

    let config = BatchConfig {
        connection,
        options: ProgramOptions {
            verify_after: resolved.verify,
            reset_after: resolved.reset,
            chip_erase: resolved.full_erase,
            chunk_size: 1024,
        },
        count: args.count,
        continue_on_error: !args.stop_on_error,
        delay_ms: args.delay_ms,
        rearm: match args.rearm {
            Rearm::Detach => RearmPolicy::Detach,
            Rearm::Immediate => RearmPolicy::Immediate,
        },
        detach_timeout_ms: args.detach_timeout_ms,
        attach_timeout_ms: args.attach_timeout_ms,
        poll_interval_ms: args.poll_interval_ms,
    };

    let (progress, output_buf) = CliProgressCallback::new(cli.quiet || !args.unit_progress, cli.json);
    let observer = CliBatchObserver {
        quiet: cli.quiet,
        json: cli.json,
        output_buf: Arc::clone(&output_buf),
        cancelled: Arc::new(AtomicBool::new(false)),
    };

    let mock = cli.mock;
    let probe_id = config.connection.probe_id.clone();
    let mut opener =
        |cfg: &ConnectionConfig| open_session(backend.as_ref(), cfg, mock);
    let allocator = resolved
        .serial
        .as_ref()
        .map(|config| flash_core::SerialAllocator::new(config.clone()));
    let mut after_unit = |session: &mut dyn flash_core::traits::FlashSession, _index: u32| {
        let stamped = match allocator {
            Some(ref allocator) => Some(flash_core::program_serial(
                session,
                allocator.config(),
                allocator.take(),
            )?),
            None => None,
        };
        persist_mock_session(session, mock, probe_id.as_deref());
        Ok(stamped)
    };

    let report = run_batch_with(
        &mut opener,
        Some(&mut after_unit),
        &firmware,
        &config,
        Some(&observer),
        Some(&progress),
    );

    if let Some(ref log_path) = args.log {
        let contents = if args.log_json {
            serde_json::to_string_pretty(&report).map_err(|e| {
                CliError::InvalidArgsOrProfile(format!("Failed to serialize batch report: {}", e))
            })?
        } else {
            report.to_csv()
        };
        std::fs::write(log_path, contents).map_err(|e| {
            CliError::InvalidArgsOrProfile(format!(
                "Failed to write batch log '{}': {}",
                log_path, e
            ))
        })?;
    }

    emit_summary(&observer, &report);

    if let Ok(buf) = output_buf.lock() {
        let _ = stdout.write_all(&buf);
        let _ = stdout.flush();
    }

    batch_exit(&report)
}

fn emit_summary(observer: &CliBatchObserver, report: &BatchReport) {
    if observer.json {
        observer.write_json(&BatchNdJson::Complete {
            passed: report.passed,
            failed: report.failed,
            total: report.total(),
            duration_ms: report.duration_ms,
            stop_reason: &report.stop_reason,
        });
    } else if !observer.quiet {
        observer.write_line(&format!(
            "[BATCH] {} unit(s): {} passed, {} failed in {} ms ({:?})",
            report.total(),
            report.passed,
            report.failed,
            report.duration_ms,
            report.stop_reason
        ));
    }
}

/// Maps the batch outcome onto the CLI exit-code taxonomy.
fn batch_exit(report: &BatchReport) -> Result<(), CliError> {
    if report.failed > 0 {
        return Err(CliError::FlashVerify(format!(
            "{} of {} unit(s) failed",
            report.failed,
            report.total()
        )));
    }
    match report.stop_reason {
        StopReason::AttachTimeout => Err(CliError::TargetConnection(
            "Timed out waiting for the next board to be connected".to_string(),
        )),
        StopReason::DetachTimeout => Err(CliError::TargetConnection(
            "Timed out waiting for the programmed board to be disconnected".to_string(),
        )),
        _ => Ok(()),
    }
}
