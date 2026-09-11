use crate::error::FlashError;
use crate::traits::{FlashBackend, FlashSession};
use crate::types::{ConnectionConfig, ProbeInfo};

#[cfg(feature = "live-probe")]
use crate::live::ProbeRsLiveBackend;
#[cfg(feature = "mock-probe")]
use crate::mock::MockProbeBackend;

/// Unified probe backend that automatically discovers and connects to real physical
/// hardware probes (ST-Link, CMSIS-DAP, J-Link via probe-rs) while also supporting
/// virtual simulator probes for development and testing.
#[derive(Debug, Default)]
pub struct UnifiedBackend {
    #[cfg(feature = "live-probe")]
    pub live: ProbeRsLiveBackend,
    #[cfg(feature = "mock-probe")]
    pub mock: MockProbeBackend,
}

impl UnifiedBackend {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "live-probe")]
            live: ProbeRsLiveBackend::new(),
            #[cfg(feature = "mock-probe")]
            mock: MockProbeBackend::new(),
        }
    }
}

impl FlashBackend for UnifiedBackend {
    fn name(&self) -> &'static str {
        "unified"
    }

    fn list_probes(&self) -> Result<Vec<ProbeInfo>, FlashError> {
        let mut probes = Vec::new();

        // 1. Scan for real physical hardware debug probes (ST-Link, CMSIS-DAP, J-Link, etc.)
        #[cfg(feature = "live-probe")]
        {
            if let Ok(live_probes) = self.live.list_probes() {
                probes.extend(live_probes);
            }
        }

        // 2. Also append virtual simulator probes for testing when no hardware is attached
        #[cfg(feature = "mock-probe")]
        {
            if let Ok(mock_probes) = self.mock.list_probes() {
                probes.extend(mock_probes);
            }
        }

        Ok(probes)
    }

    fn open_session(&self, config: &ConnectionConfig) -> Result<Box<dyn FlashSession>, FlashError> {
        let is_mock = config
            .probe_id
            .as_deref()
            .map(|id| id.starts_with("mock:"))
            .unwrap_or(false);

        if is_mock {
            #[cfg(feature = "mock-probe")]
            {
                self.mock.open_session(config)
            }
            #[cfg(not(feature = "mock-probe"))]
            {
                Err(FlashError::ProbeNotFound("Mock probe not enabled".to_string()))
            }
        } else {
            #[cfg(feature = "live-probe")]
            {
                self.live.open_session(config)
            }
            #[cfg(not(feature = "live-probe"))]
            {
                #[cfg(feature = "mock-probe")]
                {
                    self.mock.open_session(config)
                }
                #[cfg(not(feature = "mock-probe"))]
                {
                    Err(FlashError::ProbeNotFound("No probe backend available".to_string()))
                }
            }
        }
    }
}
