//! Backend registry.
//!
//! The registry is the single place that decides which backend serves a given
//! request. Backends are addressed by the scheme on a probe identifier —
//! `probe:`, `mock:`, `esp:`, `openocd:` — so adding a transport means
//! registering one more [`FlashBackend`] rather than editing a match arm in
//! both applications.
//!
//! An identifier with no recognised scheme falls through to the first backend
//! registered, which keeps identifiers written before schemes existed (bare
//! ST-Link serial numbers, `0483:374f:0029…`) working unchanged.

use crate::error::FlashError;
use crate::traits::{FlashBackend, FlashSession};
use crate::types::{ConnectionConfig, ProbeInfo, TargetInfo};

/// Splits `probe:0483:374f` into `("probe", "0483:374f")`.
///
/// Only the first segment is treated as a scheme, and only when it looks like
/// one: a probe identifier is itself colon-separated, so `0483:374f:0029…`
/// must not be read as a scheme named `0483`.
fn split_scheme(probe_id: &str) -> Option<(&str, &str)> {
    let (head, rest) = probe_id.split_once(':')?;
    let is_scheme = !head.is_empty()
        && head
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '-' || c == '_');
    is_scheme.then_some((head, rest))
}

/// A collection of backends, addressed by identifier scheme.
///
/// Implements [`FlashBackend`] itself: `list_probes` concatenates what every
/// registered backend can see, and every other call routes to one of them.
#[derive(Default)]
pub struct BackendRegistry {
    backends: Vec<Box<dyn FlashBackend>>,
}

impl std::fmt::Debug for BackendRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackendRegistry")
            .field(
                "backends",
                &self.backends.iter().map(|b| b.name()).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl BackendRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a backend. The first one registered is the fallback for
    /// identifiers that carry no recognised scheme.
    pub fn register(&mut self, backend: Box<dyn FlashBackend>) {
        self.backends.push(backend);
    }

    /// Builder form of [`register`](Self::register).
    pub fn with(mut self, backend: Box<dyn FlashBackend>) -> Self {
        self.register(backend);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }

    /// Names of the registered backends, in registration order.
    pub fn names(&self) -> Vec<&'static str> {
        self.backends.iter().map(|b| b.name()).collect()
    }

    /// The backend owning `scheme`, if one is registered.
    pub fn by_scheme(&self, scheme: &str) -> Option<&dyn FlashBackend> {
        self.backends
            .iter()
            .find(|b| b.scheme() == scheme)
            .map(|b| b.as_ref())
    }

    /// The backend that should serve `probe_id`.
    ///
    /// Falls back to the first registered backend when the identifier carries
    /// no scheme, so existing profiles and scripts keep working.
    pub fn route(&self, probe_id: Option<&str>) -> Result<&dyn FlashBackend, FlashError> {
        if let Some(scheme) = probe_id.and_then(split_scheme).map(|(s, _)| s) {
            if let Some(backend) = self.by_scheme(scheme) {
                return Ok(backend);
            }
        }
        self.backends
            .first()
            .map(|b| b.as_ref())
            .ok_or_else(|| FlashError::ProbeNotFound("No flash backend is compiled in".to_string()))
    }
}

impl FlashBackend for BackendRegistry {
    fn name(&self) -> &'static str {
        "registry"
    }

    fn scheme(&self) -> &'static str {
        "registry"
    }

    /// Everything every backend can see, in registration order. A backend that
    /// fails to enumerate is skipped rather than failing the whole scan: an
    /// absent OpenOCD or a permissions problem on one transport should not
    /// hide the probes on another.
    fn list_probes(&self) -> Result<Vec<ProbeInfo>, FlashError> {
        let mut probes = Vec::new();
        for backend in &self.backends {
            if let Ok(found) = backend.list_probes() {
                probes.extend(found);
            }
        }
        Ok(probes)
    }

    fn open_session(&self, config: &ConnectionConfig) -> Result<Box<dyn FlashSession>, FlashError> {
        self.route(config.probe_id.as_deref())?.open_session(config)
    }

    fn detect_target(&self, config: &ConnectionConfig) -> Result<TargetInfo, FlashError> {
        self.route(config.probe_id.as_deref())?.detect_target(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_scheme_off_the_front() {
        assert_eq!(split_scheme("mock:stm32f401re"), Some(("mock", "stm32f401re")));
        assert_eq!(split_scheme("esp:COM7"), Some(("esp", "COM7")));
    }

    #[test]
    fn a_bare_probe_identifier_is_not_a_scheme() {
        // ST-Link identifiers are themselves colon-separated; reading "0483"
        // as a scheme would route every real probe to the fallback by accident
        // rather than by design.
        assert_eq!(split_scheme("0483:374f:002E00373438510A20343937"), None);
        assert_eq!(split_scheme("COM7"), None);
    }
}
