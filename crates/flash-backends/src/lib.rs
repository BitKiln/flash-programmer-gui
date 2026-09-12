//! The backends compiled into this build, collected into one registry.
//!
//! `flash-core` defines the traits and the registry but knows no concrete
//! backend; each backend crate depends on `flash-core`. This crate is the one
//! place that depends on both, so the applications take a single dependency and
//! Cargo features decide what is actually available.

use flash_core::{BackendRegistry, FlashBackend};

/// A registry holding every backend this build was compiled with.
///
/// Registration order matters: the first backend registered serves probe
/// identifiers that carry no scheme, so the real hardware backend goes first
/// and identifiers written before schemes existed keep working.
pub fn default_registry() -> BackendRegistry {
    let mut registry = BackendRegistry::new();

    #[cfg(feature = "live-probe")]
    registry.register(Box::new(
        flash_backend_probe_rs::ProbeRsLiveBackend::new(),
    ));

    #[cfg(feature = "mock-probe")]
    registry.register(Box::new(flash_backend_mock::MockProbeBackend::new()));

    registry
}

/// A registry holding only the simulated backend, for tests and for a UI that
/// wants to demo without hardware.
#[cfg(feature = "mock-probe")]
pub fn mock_registry() -> BackendRegistry {
    BackendRegistry::new().with(Box::new(flash_backend_mock::MockProbeBackend::new()))
}

/// Names of the backends compiled into this build.
pub fn compiled_backends() -> Vec<&'static str> {
    default_registry().names()
}

/// The former hardcoded probe-rs-or-mock backend.
#[deprecated(
    since = "0.2.0",
    note = "use `flash_backends::default_registry()`; routing now happens in `flash_core::BackendRegistry`"
)]
pub struct UnifiedBackend(BackendRegistry);

#[allow(deprecated)]
impl UnifiedBackend {
    pub fn new() -> Self {
        Self(default_registry())
    }
}

#[allow(deprecated)]
impl Default for UnifiedBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(deprecated)]
impl FlashBackend for UnifiedBackend {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn scheme(&self) -> &'static str {
        self.0.scheme()
    }

    fn list_probes(&self) -> Result<Vec<flash_core::ProbeInfo>, flash_core::FlashError> {
        self.0.list_probes()
    }

    fn open_session(
        &self,
        config: &flash_core::ConnectionConfig,
    ) -> Result<Box<dyn flash_core::FlashSession>, flash_core::FlashError> {
        self.0.open_session(config)
    }

    fn detect_target(
        &self,
        config: &flash_core::ConnectionConfig,
    ) -> Result<flash_core::TargetInfo, flash_core::FlashError> {
        self.0.detect_target(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hardware_backend_is_the_fallback() {
        let registry = default_registry();
        let names = registry.names();
        assert!(!names.is_empty(), "no backend compiled in");

        #[cfg(feature = "live-probe")]
        assert_eq!(
            names[0], "probe-rs",
            "a bare probe identifier must still reach real hardware"
        );
    }

    #[cfg(feature = "mock-probe")]
    #[test]
    fn a_mock_identifier_routes_to_the_mock_backend() {
        let registry = default_registry();
        let backend = registry.route(Some("mock:stm32f401re")).unwrap();
        assert_eq!(backend.name(), "mock-probe");
    }
}
