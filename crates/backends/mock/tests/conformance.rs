//! The mock backend against the shared backend conformance suite.
//!
//! Every backend runs this same body of assertions (see
//! `flash_core::conformance`), so "plugin architecture" is something the test
//! suite checks rather than something the README claims.

use flash_backend_mock::MockProbeBackend;
use flash_core::conformance::check_backend;
use flash_core::traits::FlashBackend;
use flash_core::types::ConnectionConfig;

fn config_for(backend: &MockProbeBackend) -> ConnectionConfig {
    let probe = backend
        .list_probes()
        .expect("the mock backend always has probes")
        .into_iter()
        .next()
        .expect("the mock backend always has probes");

    ConnectionConfig {
        probe_id: Some(probe.identifier),
        target_name: "STM32F401RE".to_string(),
        ..Default::default()
    }
}

#[test]
fn mock_backend_conforms() {
    let backend = MockProbeBackend::new();
    let config = config_for(&backend);
    if let Err(failure) = check_backend(&backend, &config) {
        panic!("mock backend breaks the backend contract -- {failure}");
    }
}
