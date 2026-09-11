pub mod devices;
pub mod erase;
pub mod flash;
pub mod profile;
pub mod reset;
pub mod verify;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use flash_core::error::FlashError;
use flash_core::mock::{FaultInjector, MockFlashMemory, MockFlashSession};
use flash_core::traits::{FlashBackend, FlashSession};
use flash_core::types::ConnectionConfig;
use flash_core::MockProbeBackend;

#[cfg(feature = "live-probe")]
use flash_core::ProbeRsLiveBackend;

/// Returns the appropriate probe backend based on the `--mock` CLI flag.
pub fn get_backend(mock: bool) -> Box<dyn FlashBackend> {
    if mock {
        Box::new(MockProbeBackend::new())
    } else {
        #[cfg(feature = "live-probe")]
        {
            Box::new(ProbeRsLiveBackend::new())
        }
        #[cfg(not(feature = "live-probe"))]
        {
            // Fallback when live-probe is not compiled: empty live backend simulator
            Box::new(MockProbeBackend::with_probes(Vec::new()))
        }
    }
}

/// Checks whether the target MCU is recognized / supported.
pub fn is_supported_target(target: &str, mock: bool) -> bool {
    if mock {
        let normalized = target.to_lowercase().replace(['-', '_'], "");
        // The mock backend resolves these to its default profile.
        normalized.is_empty()
            || normalized == "auto"
            || normalized == "default"
            || normalized.contains("f103")
            || normalized.contains("f401")
            || normalized.contains("f411")
            || normalized.contains("stm32")
            || normalized.contains("cortex")
            || normalized.contains("generic")
    } else {
        true
    }
}

/// Computes the persistent backing file path for mock flash simulation across CLI invocations.
pub fn mock_backing_file_path(target_name: &str, probe_id: Option<&str>) -> PathBuf {
    let probe_clean = probe_id
        .unwrap_or("default")
        .to_lowercase()
        .replace(['-', '_', ' ', ':', '/', '\\'], "");
    let target_clean = target_name
        .to_lowercase()
        .replace(['-', '_', ' ', ':', '/', '\\'], "");
    std::env::temp_dir().join(format!(
        "flashgui_mock_flash_{}_{}.bin",
        target_clean, probe_clean
    ))
}

/// Establishes target connection session, restoring non-volatile mock flash memory if in mock mode.
pub fn open_session(
    backend: &dyn FlashBackend,
    config: &ConnectionConfig,
    mock: bool,
) -> Result<Box<dyn FlashSession>, FlashError> {
    if mock {
        let session = backend.open_session(config)?;
        if let Some(target) = session.target_info() {
            let backing = mock_backing_file_path(&target.name, config.probe_id.as_deref());
            if backing.is_file() {
                if let Ok(data) = fs::read(&backing) {
                    if data.len() == target.flash_size as usize {
                        let mut mem = MockFlashMemory::from_target(target);
                        mem.data = data;
                        let fault_injector = Arc::new(Mutex::new(FaultInjector::new()));
                        let mut s = MockFlashSession::new_with_memory(
                            target.clone(),
                            mem,
                            fault_injector,
                        );
                        if let Ok(probes) = backend.list_probes() {
                            s.probe_info = probes.into_iter().find(|p| {
                                config.probe_id.as_deref() == Some(&p.identifier)
                            });
                        }
                        return Ok(Box::new(s));
                    }
                }
            }
        }
        Ok(session)
    } else {
        backend.open_session(config)
    }
}

/// Persists non-volatile mock flash memory to disk after erase or program operations.
pub fn persist_mock_session(
    session: &mut dyn FlashSession,
    mock: bool,
    probe_id: Option<&str>,
) {
    if mock {
        let target_meta =
            session.target_info().map(|t| (t.flash_base, t.flash_size, t.name.clone()));
        if let Some((flash_base, flash_size, target_name)) = target_meta {
            if let Ok(data) = session.read_memory(flash_base, flash_size) {
                let backing = mock_backing_file_path(&target_name, probe_id);
                let _ = fs::write(&backing, data);
            }
        }
    }
}
