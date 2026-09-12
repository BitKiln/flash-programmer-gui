//! The ESP serial backend against the shared backend conformance suite.
//!
//! Runs against the in-memory bootloader, so it exercises the real session
//! logic — address checks, chunking, verification, idempotent close — in CI
//! with no board attached. The `espflash` adapter beneath it is the only part
//! that still needs hardware.

use flash_backend_esp_serial::EspSerialBackend;
use flash_core::conformance::check_backend;
use flash_core::traits::FlashBackend;
use flash_core::types::{ConnectionConfig, Transport};

#[test]
fn esp_serial_backend_conforms() {
    let backend = EspSerialBackend::simulated();
    let probe = backend
        .list_probes()
        .expect("the simulated backend always has a port")
        .into_iter()
        .next()
        .expect("the simulated backend always has a port");

    let config = ConnectionConfig {
        probe_id: Some(probe.identifier),
        target_name: "esp32s3".to_string(),
        transport: Transport::Serial {
            baud: 460_800,
            controls_reset: true,
        },
        ..Default::default()
    };

    if let Err(failure) = check_backend(&backend, &config) {
        panic!("esp-serial backend breaks the backend contract -- {failure}");
    }
}

#[test]
fn the_real_backend_lists_only_ports_it_can_route_back_to() {
    // No assertion about how many ports exist -- CI has none, a developer's
    // machine may have several. What must hold is that anything listed carries
    // the scheme, or the registry cannot route a later call back here.
    let backend = EspSerialBackend::new();
    for probe in backend.list_probes().expect("enumeration must not error") {
        assert!(
            probe.identifier.starts_with("esp:"),
            "{} is not routable",
            probe.identifier
        );
    }
}
