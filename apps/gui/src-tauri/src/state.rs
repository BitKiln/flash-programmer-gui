use std::sync::Mutex;

use flash_core::FlashEvent;
use flash_core::traits::FlashSession;

/// Shared application state managed by Tauri.
///
/// Holds the active probe session and a buffer of flash events
/// that the frontend polls via the `get_flash_events` command.
pub struct AppState {
    pub session: Mutex<Option<Box<dyn FlashSession>>>,
    pub events: Mutex<Vec<FlashEvent>>,
    pub backend: Mutex<Box<dyn flash_core::FlashBackend>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
            events: Mutex::new(Vec::new()),
            backend: Mutex::new(Box::new(flash_core::MockProbeBackend::new())),
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
}
