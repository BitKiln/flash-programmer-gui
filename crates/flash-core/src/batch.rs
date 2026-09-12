//! Batch (production) programming: flash the same firmware onto a series of
//! boards without restarting the tool between units.
//!
//! The runner owns the loop, not the transport: it is handed a function that
//! opens a session, so the CLI can keep its mock-persistence wrapper and the
//! desktop application can reuse the same code path.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use firmware_parser::FirmwareImage;
use serde::{Deserialize, Serialize};

use crate::error::FlashError;
use crate::manager::FlashManager;
use crate::progress::ProgressCallback;
use crate::traits::{FlashBackend, FlashSession};
use crate::types::{ConnectionConfig, ProgramOptions};

/// How the runner decides that the next board is ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RearmPolicy {
    /// Start the next unit as soon as the previous one finishes. Suited to a
    /// fixture that reuses one probe and swaps boards under software control,
    /// and to mock runs.
    Immediate,
    /// Wait for the operator to unplug the finished board, then wait for the
    /// next one to appear. This is the normal bench workflow.
    Detach,
}

/// Everything the runner needs besides the firmware image itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchConfig {
    pub connection: ConnectionConfig,
    pub options: ProgramOptions,
    /// Stop after this many units. `None` runs until cancelled.
    pub count: Option<u32>,
    /// Keep going after a unit fails instead of ending the batch.
    pub continue_on_error: bool,
    /// Pause after each unit, before re-arming.
    pub delay_ms: u64,
    pub rearm: RearmPolicy,
    /// Give up waiting for the finished board to be removed after this long.
    pub detach_timeout_ms: u64,
    /// Give up waiting for the next board to appear after this long.
    pub attach_timeout_ms: u64,
    /// How often to re-check while waiting for a detach or attach.
    pub poll_interval_ms: u64,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            options: ProgramOptions::default(),
            count: None,
            continue_on_error: true,
            delay_ms: 0,
            rearm: RearmPolicy::Detach,
            detach_timeout_ms: 300_000,
            attach_timeout_ms: 300_000,
            poll_interval_ms: 250,
        }
    }
}

/// Outcome of one board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitStatus {
    Passed,
    Failed,
}

impl UnitStatus {
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            UnitStatus::Passed => "passed",
            UnitStatus::Failed => "failed",
        }
    }
}

/// One row of the production log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnitRecord {
    /// 1-based position in the batch.
    pub index: u32,
    pub status: UnitStatus,
    pub probe_serial: Option<String>,
    pub target: Option<String>,
    pub bytes_flashed: u32,
    pub verified: bool,
    pub duration_ms: u64,
    pub started_unix_ms: u64,
    pub message: String,
}

/// Why the batch stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Requested unit count was reached.
    CountReached,
    /// The observer asked for cancellation.
    Cancelled,
    /// A unit failed and `continue_on_error` was false.
    FailureStop,
    /// No next board appeared within `attach_timeout_ms`.
    AttachTimeout,
    /// The finished board was never removed within `detach_timeout_ms`.
    DetachTimeout,
}

/// Result of a whole batch run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchReport {
    pub records: Vec<UnitRecord>,
    pub passed: u32,
    pub failed: u32,
    pub duration_ms: u64,
    pub stop_reason: StopReason,
}

impl BatchReport {
    #[inline]
    pub fn total(&self) -> u32 {
        self.passed + self.failed
    }

    /// Renders the run as CSV, one row per unit, with a header line.
    pub fn to_csv(&self) -> String {
        let mut out = String::from(
            "index,status,probe_serial,target,bytes_flashed,verified,duration_ms,started_unix_ms,message\n",
        );
        for r in &self.records {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                r.index,
                r.status.as_str(),
                csv_field(r.probe_serial.as_deref().unwrap_or("")),
                csv_field(r.target.as_deref().unwrap_or("")),
                r.bytes_flashed,
                r.verified,
                r.duration_ms,
                r.started_unix_ms,
                csv_field(&r.message),
            ));
        }
        out
    }
}

/// Quotes a CSV field when it contains a separator, quote, or newline.
fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Progress of the batch itself, distinct from the per-unit flash telemetry
/// carried by [`ProgressCallback`].
#[derive(Debug, Clone, PartialEq)]
pub enum BatchEvent {
    /// Waiting for the operator to remove the board just programmed.
    WaitingForDetach { index: u32 },
    /// Waiting for the next board to be connected.
    WaitingForAttach { index: u32 },
    UnitStarted {
        index: u32,
        probe_serial: Option<String>,
    },
    UnitFinished(UnitRecord),
    BatchFinished {
        passed: u32,
        failed: u32,
        stop_reason: StopReason,
    },
}

/// Receiver of [`BatchEvent`]s, and the batch-level cancellation source.
pub trait BatchObserver: Send + Sync {
    fn on_batch_event(&self, event: BatchEvent);

    /// Ends the batch after the unit in flight; per-unit cancellation stays
    /// with the [`ProgressCallback`].
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Opens a session for one unit. The CLI passes its mock-aware opener here.
pub type SessionOpener<'a> =
    &'a mut dyn FnMut(&ConnectionConfig) -> Result<Box<dyn FlashSession>, FlashError>;

/// Called after each unit so the caller can persist mock state, stamp a serial
/// number, or run any other post-program step. Returning an error fails the
/// unit.
pub type UnitHook<'a> = &'a mut dyn FnMut(&mut dyn FlashSession, u32) -> Result<(), FlashError>;

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Runs a batch against a backend, opening each unit session directly.
pub fn run_batch(
    backend: &dyn FlashBackend,
    firmware: &FirmwareImage,
    config: &BatchConfig,
    observer: Option<&dyn BatchObserver>,
    progress: Option<&dyn ProgressCallback>,
) -> BatchReport {
    let mut opener = |cfg: &ConnectionConfig| backend.open_session(cfg);
    run_batch_with(&mut opener, None, firmware, config, observer, progress)
}

/// Runs a batch with a caller-supplied session opener and optional per-unit hook.
pub fn run_batch_with(
    open: SessionOpener<'_>,
    mut after_unit: Option<UnitHook<'_>>,
    firmware: &FirmwareImage,
    config: &BatchConfig,
    observer: Option<&dyn BatchObserver>,
    progress: Option<&dyn ProgressCallback>,
) -> BatchReport {
    let batch_start = Instant::now();
    let mut records: Vec<UnitRecord> = Vec::new();
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut index = 0u32;

    let emit = |event: BatchEvent| {
        if let Some(obs) = observer {
            obs.on_batch_event(event);
        }
    };
    let cancelled = || observer.map(|o| o.is_cancelled()).unwrap_or(false);

    let stop_reason = loop {
        if cancelled() {
            break StopReason::Cancelled;
        }
        if let Some(limit) = config.count {
            if index >= limit {
                break StopReason::CountReached;
            }
        }

        index += 1;

        // Re-arm: on every unit after the first, wait for the finished board to
        // go away and the next one to show up. Without this the same board
        // would simply be programmed again.
        if index > 1 && config.rearm == RearmPolicy::Detach {
            emit(BatchEvent::WaitingForDetach { index });
            match wait_for(
                open,
                config,
                config.detach_timeout_ms,
                SessionPresence::Absent,
                observer,
            ) {
                WaitOutcome::Reached => {}
                WaitOutcome::TimedOut => break StopReason::DetachTimeout,
                WaitOutcome::Cancelled => break StopReason::Cancelled,
            }

            emit(BatchEvent::WaitingForAttach { index });
            match wait_for(
                open,
                config,
                config.attach_timeout_ms,
                SessionPresence::Present,
                observer,
            ) {
                WaitOutcome::Reached => {}
                WaitOutcome::TimedOut => break StopReason::AttachTimeout,
                WaitOutcome::Cancelled => break StopReason::Cancelled,
            }
        }

        let started_unix_ms = unix_ms();
        let unit_start = Instant::now();

        let mut session = match open(&config.connection) {
            Ok(s) => {
                emit(BatchEvent::UnitStarted {
                    index,
                    probe_serial: config.connection.probe_id.clone(),
                });
                s
            }
            Err(err) => {
                let record = UnitRecord {
                    index,
                    status: UnitStatus::Failed,
                    probe_serial: config.connection.probe_id.clone(),
                    target: None,
                    bytes_flashed: 0,
                    verified: false,
                    duration_ms: unit_start.elapsed().as_millis() as u64,
                    started_unix_ms,
                    message: err.to_string(),
                };
                failed += 1;
                emit(BatchEvent::UnitFinished(record.clone()));
                records.push(record);
                if !config.continue_on_error {
                    break StopReason::FailureStop;
                }
                continue;
            }
        };

        let target_name = session.target_info().map(|t| t.name.clone());

        let outcome = FlashManager::execute_flash(
            session.as_mut(),
            firmware,
            &config.options,
            progress,
        );

        let mut record = match outcome {
            Ok(result) => UnitRecord {
                index,
                status: UnitStatus::Passed,
                probe_serial: config.connection.probe_id.clone(),
                target: target_name.clone(),
                bytes_flashed: result.bytes_flashed,
                verified: result.verify_report.is_some(),
                duration_ms: result.duration_ms,
                started_unix_ms,
                message: result.message,
            },
            Err(err) => UnitRecord {
                index,
                status: UnitStatus::Failed,
                probe_serial: config.connection.probe_id.clone(),
                target: target_name.clone(),
                bytes_flashed: 0,
                verified: false,
                duration_ms: unit_start.elapsed().as_millis() as u64,
                started_unix_ms,
                message: err.to_string(),
            },
        };

        if let Some(hook) = after_unit.as_deref_mut() {
            if let Err(err) = hook(session.as_mut(), index) {
                record.status = UnitStatus::Failed;
                record.message = err.to_string();
            }
        }

        let _ = session.close();

        match record.status {
            UnitStatus::Passed => passed += 1,
            UnitStatus::Failed => failed += 1,
        }
        let stop_now = record.status == UnitStatus::Failed && !config.continue_on_error;
        emit(BatchEvent::UnitFinished(record.clone()));
        records.push(record);

        if stop_now {
            break StopReason::FailureStop;
        }

        if config.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(config.delay_ms));
        }
    };

    emit(BatchEvent::BatchFinished {
        passed,
        failed,
        stop_reason: stop_reason.clone(),
    });

    BatchReport {
        records,
        passed,
        failed,
        duration_ms: batch_start.elapsed().as_millis() as u64,
        stop_reason,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionPresence {
    Present,
    Absent,
}

enum WaitOutcome {
    Reached,
    TimedOut,
    Cancelled,
}

/// Polls until a session can (or can no longer) be opened, or the deadline passes.
fn wait_for(
    open: SessionOpener<'_>,
    config: &BatchConfig,
    timeout_ms: u64,
    want: SessionPresence,
    observer: Option<&dyn BatchObserver>,
) -> WaitOutcome {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let interval = Duration::from_millis(config.poll_interval_ms.max(10));

    loop {
        if observer.map(|o| o.is_cancelled()).unwrap_or(false) {
            return WaitOutcome::Cancelled;
        }

        let present = match open(&config.connection) {
            Ok(mut session) => {
                let _ = session.close();
                SessionPresence::Present
            }
            Err(_) => SessionPresence::Absent,
        };

        if present == want {
            return WaitOutcome::Reached;
        }
        if Instant::now() >= deadline {
            return WaitOutcome::TimedOut;
        }
        std::thread::sleep(interval);
    }
}
