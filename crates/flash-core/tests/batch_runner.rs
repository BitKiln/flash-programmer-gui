//! Batch (production) runner behaviour, driven through a scripted session
//! opener so failures, detach/attach waits, and cancellation are deterministic.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use firmware_parser::{FirmwareImage, MemorySegment};
use flash_core::batch::{
    run_batch_with, BatchConfig, BatchEvent, BatchObserver, RearmPolicy, StopReason, UnitStatus,
};
use flash_core::error::FlashError;
use flash_core::mock::MockProbeBackend;
use flash_core::traits::{FlashBackend, FlashSession};
use flash_core::types::{ConnectionConfig, ProgramOptions};

fn firmware() -> FirmwareImage {
    let data = vec![0xAA; 512];
    FirmwareImage {
        metadata: firmware_parser::parse_bin(&data, 0x0800_0000)
            .expect("fixture image must parse")
            .metadata,
        segments: vec![MemorySegment::new(0x0800_0000, data)],
    }
}

fn config(count: u32) -> BatchConfig {
    BatchConfig {
        connection: ConnectionConfig {
            probe_id: Some("mock:stlink-stm32f401re".to_string()),
            target_name: "STM32F401RE".to_string(),
            ..ConnectionConfig::default()
        },
        options: ProgramOptions {
            verify_after: true,
            reset_after: false,
            chip_erase: false,
            chunk_size: 1024,
        },
        count: Some(count),
        continue_on_error: true,
        delay_ms: 0,
        rearm: RearmPolicy::Immediate,
        detach_timeout_ms: 500,
        attach_timeout_ms: 500,
        poll_interval_ms: 10,
    }
}

/// Records every event so tests can assert on the sequence, and can be armed
/// to cancel after a given number of finished units.
#[derive(Default)]
struct RecordingObserver {
    events: Mutex<Vec<String>>,
    finished: AtomicU32,
    cancel_after: Option<u32>,
    cancelled: AtomicBool,
}

impl RecordingObserver {
    fn new(cancel_after: Option<u32>) -> Self {
        Self {
            cancel_after,
            ..Default::default()
        }
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}

impl BatchObserver for RecordingObserver {
    fn on_batch_event(&self, event: BatchEvent) {
        let tag = match &event {
            BatchEvent::WaitingForDetach { index } => format!("detach:{}", index),
            BatchEvent::WaitingForAttach { index } => format!("attach:{}", index),
            BatchEvent::UnitStarted { index, .. } => format!("start:{}", index),
            BatchEvent::UnitFinished(record) => {
                let n = self.finished.fetch_add(1, Ordering::SeqCst) + 1;
                if self.cancel_after == Some(n) {
                    self.cancelled.store(true, Ordering::SeqCst);
                }
                format!("finish:{}:{}", record.index, record.status.as_str())
            }
            BatchEvent::BatchFinished { passed, failed, .. } => {
                format!("done:{}:{}", passed, failed)
            }
        };
        self.events.lock().unwrap().push(tag);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

fn mock_opener(
    backend: &MockProbeBackend,
) -> impl FnMut(&ConnectionConfig) -> Result<Box<dyn FlashSession>, FlashError> + '_ {
    move |cfg: &ConnectionConfig| backend.open_session(cfg)
}

#[test]
fn programs_every_unit_up_to_the_requested_count() {
    let backend = MockProbeBackend::new();
    let mut opener = mock_opener(&backend);
    let observer = RecordingObserver::new(None);

    let report = run_batch_with(
        &mut opener,
        None,
        &firmware(),
        &config(3),
        Some(&observer),
        None,
    );

    assert_eq!(report.passed, 3, "records: {:?}", report.records);
    assert_eq!(report.failed, 0);
    assert_eq!(report.total(), 3);
    assert_eq!(report.stop_reason, StopReason::CountReached);

    let indices: Vec<u32> = report.records.iter().map(|r| r.index).collect();
    assert_eq!(indices, vec![1, 2, 3], "units must be numbered in order");
    assert!(report.records.iter().all(|r| r.verified));
    assert!(report.records.iter().all(|r| r.bytes_flashed == 512));
    assert!(observer.events().contains(&"done:3:0".to_string()));
}

#[test]
fn a_failed_unit_is_logged_and_the_batch_continues() {
    let backend = MockProbeBackend::new();
    let attempt = AtomicU32::new(0);
    let mut opener = |cfg: &ConnectionConfig| {
        // Fail the second board at connect time; the others program normally.
        if attempt.fetch_add(1, Ordering::SeqCst) == 1 {
            Err(FlashError::ConnectError("no target detected".to_string()))
        } else {
            backend.open_session(cfg)
        }
    };

    let report = run_batch_with(&mut opener, None, &firmware(), &config(3), None, None);

    assert_eq!(report.passed, 2);
    assert_eq!(report.failed, 1);
    assert_eq!(report.stop_reason, StopReason::CountReached);
    assert_eq!(report.records[1].status, UnitStatus::Failed);
    assert!(
        report.records[1].message.contains("no target detected"),
        "message was: {}",
        report.records[1].message
    );
    // A failure must not renumber the units that follow it.
    assert_eq!(report.records[2].index, 3);
    assert_eq!(report.records[2].status, UnitStatus::Passed);
}

#[test]
fn stop_on_error_ends_the_batch_at_the_first_failure() {
    let backend = MockProbeBackend::new();
    let attempt = AtomicU32::new(0);
    let mut opener = |cfg: &ConnectionConfig| {
        if attempt.fetch_add(1, Ordering::SeqCst) == 1 {
            Err(FlashError::ConnectError("no target detected".to_string()))
        } else {
            backend.open_session(cfg)
        }
    };

    let mut cfg = config(5);
    cfg.continue_on_error = false;
    let report = run_batch_with(&mut opener, None, &firmware(), &cfg, None, None);

    assert_eq!(report.stop_reason, StopReason::FailureStop);
    assert_eq!(report.total(), 2, "must not attempt a third board");
    assert_eq!(report.passed, 1);
    assert_eq!(report.failed, 1);
}

#[test]
fn cancellation_ends_the_batch_before_the_next_unit() {
    let backend = MockProbeBackend::new();
    let mut opener = mock_opener(&backend);
    let observer = RecordingObserver::new(Some(2));

    let report = run_batch_with(
        &mut opener,
        None,
        &firmware(),
        &config(10),
        Some(&observer),
        None,
    );

    assert_eq!(report.stop_reason, StopReason::Cancelled);
    assert_eq!(report.total(), 2);
    assert_eq!(report.passed, 2);
}

#[test]
fn detach_rearm_waits_for_the_board_to_be_swapped() {
    let backend = MockProbeBackend::new();
    // Board 1 is present, then removed, then board 2 appears: the runner has to
    // observe both transitions before it programs the second unit.
    let calls = AtomicU32::new(0);
    let mut opener = |cfg: &ConnectionConfig| {
        let n = calls.fetch_add(1, Ordering::SeqCst);
        // 0: unit 1 session. 1: detach poll (still present). 2: detach poll
        // (gone). 3: attach poll (still gone). 4: attach poll (present).
        // 5: unit 2 session.
        if n == 1 || n == 3 {
            if n == 1 {
                return backend.open_session(cfg);
            }
            return Err(FlashError::ProbeNotFound("no probe".to_string()));
        }
        if n == 2 {
            return Err(FlashError::ProbeNotFound("no probe".to_string()));
        }
        backend.open_session(cfg)
    };

    let mut cfg = config(2);
    cfg.rearm = RearmPolicy::Detach;
    let observer = RecordingObserver::new(None);
    let report = run_batch_with(&mut opener, None, &firmware(), &cfg, Some(&observer), None);

    assert_eq!(report.passed, 2, "records: {:?}", report.records);
    let events = observer.events();
    let detach_at = events.iter().position(|e| e == "detach:2").expect("detach");
    let attach_at = events.iter().position(|e| e == "attach:2").expect("attach");
    let start_at = events.iter().position(|e| e == "start:2").expect("start");
    assert!(
        detach_at < attach_at && attach_at < start_at,
        "expected detach then attach then start, got {:?}",
        events
    );
}

#[test]
fn attach_timeout_stops_the_batch() {
    let backend = MockProbeBackend::new();
    let calls = AtomicU32::new(0);
    // Only the first board ever connects; the operator never plugs in another.
    let mut opener = |cfg: &ConnectionConfig| {
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            backend.open_session(cfg)
        } else {
            Err(FlashError::ProbeNotFound("no probe".to_string()))
        }
    };

    let mut cfg = config(3);
    cfg.rearm = RearmPolicy::Detach;
    cfg.attach_timeout_ms = 60;
    let report = run_batch_with(&mut opener, None, &firmware(), &cfg, None, None);

    assert_eq!(report.stop_reason, StopReason::AttachTimeout);
    assert_eq!(report.total(), 1);
}

#[test]
fn the_per_unit_hook_runs_and_can_fail_a_unit() {
    let backend = MockProbeBackend::new();
    let mut opener = mock_opener(&backend);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_hook = Arc::clone(&seen);
    let mut hook = move |_session: &mut dyn FlashSession, index: u32| {
        seen_hook.lock().unwrap().push(index);
        if index == 2 {
            Err(FlashError::ProgramError("post-program step failed".into()))
        } else {
            Ok(())
        }
    };

    let report = run_batch_with(
        &mut opener,
        Some(&mut hook),
        &firmware(),
        &config(2),
        None,
        None,
    );

    assert_eq!(*seen.lock().unwrap(), vec![1, 2]);
    assert_eq!(report.passed, 1);
    assert_eq!(report.failed, 1);
    assert_eq!(report.records[1].status, UnitStatus::Failed);
    assert!(report.records[1].message.contains("post-program step failed"));
}

#[test]
fn csv_log_has_one_row_per_unit_and_quotes_separators() {
    let backend = MockProbeBackend::new();
    let mut opener = mock_opener(&backend);
    let report = run_batch_with(&mut opener, None, &firmware(), &config(2), None, None);

    let csv = report.to_csv();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 3, "header plus two units, got: {}", csv);
    assert!(lines[0].starts_with("index,status,probe_serial,target"));
    assert!(lines[1].starts_with("1,passed,"));
    assert!(lines[2].starts_with("2,passed,"));
    // The success message contains commas, so it must be quoted.
    assert!(
        lines[1].contains("\"Successfully flashed"),
        "unquoted message in: {}",
        lines[1]
    );
}
