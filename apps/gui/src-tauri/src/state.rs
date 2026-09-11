use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use flash_core::progress::ClosureProgressCallback;
use flash_core::traits::FlashSession;
use flash_core::FlashEvent;

/// Shared application state managed by Tauri.
///
/// Every field is an `Arc` because the long-running commands hand their work to
/// a blocking worker thread: they clone these handles, release the Tauri
/// command thread, and let `cancel_operation` run while the work is in flight.
#[derive(Clone)]
pub struct AppState {
    pub session: Arc<Mutex<Option<Box<dyn FlashSession>>>>,
    pub events: Arc<Mutex<Vec<FlashEvent>>>,
    pub backend: Arc<Mutex<Box<dyn flash_core::FlashBackend>>>,
    /// Set by `cancel_operation`, polled by the backends at block boundaries.
    pub cancelled: Arc<AtomicBool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            session: Arc::new(Mutex::new(None)),
            events: Arc::new(Mutex::new(Vec::new())),
            backend: Arc::new(Mutex::new(Box::new(flash_core::UnifiedBackend::new()))),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl AppState {
    /// Drain all buffered events, returning them and clearing the buffer.
    pub fn drain_events(&self) -> Vec<FlashEvent> {
        let mut events = self.events.lock().unwrap();
        events.drain(..).collect()
    }

    /// Push a new event into the buffer.
    pub fn push_event(&self, event: FlashEvent) {
        let mut events = self.events.lock().unwrap();
        events.push(event);
    }

    /// Clears any cancellation left over from a previous operation.
    pub fn arm(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    /// Requests cancellation of the operation currently in flight.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Builds a progress callback that forwards each event to `sink`, buffers
    /// it for `get_flash_events`, and reports cancellation.
    ///
    /// The buffer is kept alongside the sink so a frontend that missed the
    /// event subscription (or an older one still polling) is not left blind.
    pub fn progress_callback<S>(
        &self,
        sink: S,
    ) -> ClosureProgressCallback<impl Fn(FlashEvent) + Send + Sync, impl Fn() -> bool + Send + Sync>
    where
        S: Fn(&FlashEvent) + Send + Sync,
    {
        let events = Arc::clone(&self.events);
        let cancelled = Arc::clone(&self.cancelled);
        ClosureProgressCallback::new(
            move |event: FlashEvent| {
                sink(&event);
                if let Ok(mut buffer) = events.lock() {
                    buffer.push(event);
                }
            },
            Some(move || cancelled.load(Ordering::SeqCst)),
        )
    }
}
