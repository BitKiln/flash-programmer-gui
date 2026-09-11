use serde::{Deserialize, Serialize};

/// High-level lifecycle stage of a flash operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlashStage {
    Connecting,
    Erasing,
    Programming,
    Verifying,
    Resetting,
    Completed,
    Failed,
    Cancelled,
}

/// Severity level for flash session logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Detailed real-time progress metrics emitted during flashing or verification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressMetrics {
    pub stage: FlashStage,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub percentage: f32,
    pub speed_bps: f64,
    pub elapsed_ms: u64,
    pub current_address: u32,
    pub message: String,
}

impl ProgressMetrics {
    pub fn new(
        stage: FlashStage,
        bytes_transferred: u64,
        total_bytes: u64,
        elapsed_ms: u64,
        current_address: u32,
        message: String,
    ) -> Self {
        let percentage = if total_bytes > 0 {
            ((bytes_transferred as f64 / total_bytes as f64) * 100.0).clamp(0.0, 100.0) as f32
        } else {
            100.0
        };

        let speed_bps = if elapsed_ms > 0 {
            (bytes_transferred as f64) / (elapsed_ms as f64 / 1000.0)
        } else {
            0.0
        };

        Self {
            stage,
            bytes_transferred,
            total_bytes,
            percentage,
            speed_bps,
            elapsed_ms,
            current_address,
            message,
        }
    }
}

/// Event stream payloads sent to UI or CLI frontends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum FlashEvent {
    StageStarted {
        stage: FlashStage,
        total_bytes: u64,
        message: String,
    },
    Progress(ProgressMetrics),
    StageCompleted {
        stage: FlashStage,
        duration_ms: u64,
    },
    Log {
        level: LogLevel,
        message: String,
        timestamp_ms: u64,
    },
    Warning {
        message: String,
    },
    Error {
        stage: FlashStage,
        message: String,
    },
}

/// Trait for receiving synchronous or asynchronous progress telemetry events.
pub trait ProgressCallback: Send + Sync {
    fn on_event(&self, event: FlashEvent);

    /// Cooperative cancellation poll called at block boundaries.
    fn is_cancelled(&self) -> bool {
        false
    }
}

// Blanket implementation for any Fn(FlashEvent) + Send + Sync closure
impl<F> ProgressCallback for F
where
    F: Fn(FlashEvent) + Send + Sync,
{
    fn on_event(&self, event: FlashEvent) {
        self(event);
    }
}

impl ProgressCallback for Box<dyn ProgressCallback> {
    fn on_event(&self, event: FlashEvent) {
        (**self).on_event(event);
    }

    fn is_cancelled(&self) -> bool {
        (**self).is_cancelled()
    }
}


/// Closure wrapper supporting both an event receiver and a cancellation check.
pub struct ClosureProgressCallback<F, C>
where
    F: Fn(FlashEvent) + Send + Sync,
    C: Fn() -> bool + Send + Sync,
{
    on_event_fn: F,
    is_cancelled_fn: Option<C>,
}

impl<F, C> ClosureProgressCallback<F, C>
where
    F: Fn(FlashEvent) + Send + Sync,
    C: Fn() -> bool + Send + Sync,
{
    pub fn new(on_event_fn: F, is_cancelled_fn: Option<C>) -> Self {
        Self {
            on_event_fn,
            is_cancelled_fn,
        }
    }
}

impl<F, C> ProgressCallback for ClosureProgressCallback<F, C>
where
    F: Fn(FlashEvent) + Send + Sync,
    C: Fn() -> bool + Send + Sync,
{
    fn on_event(&self, event: FlashEvent) {
        (self.on_event_fn)(event);
    }

    fn is_cancelled(&self) -> bool {
        if let Some(ref check) = self.is_cancelled_fn {
            check()
        } else {
            false
        }
    }
}

/// No-op progress callback that ignores all events.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopProgressCallback;

impl ProgressCallback for NoopProgressCallback {
    fn on_event(&self, _event: FlashEvent) {}
}
