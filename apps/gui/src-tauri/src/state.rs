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
    pub backend: Arc<Mutex<Box<dyn flash_core::FlashBackend>>>,
    /// Set by `cancel_operation`, polled by the backends at block boundaries.
    pub cancelled: Arc<AtomicBool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            session: Arc::new(Mutex::new(None)),
            backend: Arc::new(Mutex::new(Box::new(flash_backends::default_registry()))),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl AppState {
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

    /// Builds a progress callback that forwards each event to `sink` and
    /// reports cancellation.
    pub fn progress_callback<S>(
        &self,
        sink: S,
    ) -> ClosureProgressCallback<impl Fn(FlashEvent) + Send + Sync, impl Fn() -> bool + Send + Sync>
    where
        S: Fn(&FlashEvent) + Send + Sync,
    {
        let cancelled = Arc::clone(&self.cancelled);
        ClosureProgressCallback::new(
            move |event: FlashEvent| {
                sink(&event);
            },
            Some(move || cancelled.load(Ordering::SeqCst)),
        )
    }
}
