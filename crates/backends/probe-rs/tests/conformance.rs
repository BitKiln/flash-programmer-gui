//! The probe-rs backend against the shared backend conformance suite.
//!
//! Ignored by default: it needs a probe and a board, and **it erases and
//! reprograms the first flash sector of whatever is attached**. Opt in:
//!
//! ```text
//! FLASHGUI_HW_TARGET=STM32U575ZITx cargo test -p flash-backend-probe-rs --test conformance -- --ignored
//! ```
//!
//! Set `FLASHGUI_HW_PROBE` as well when more than one probe is attached.

use flash_backend_probe_rs::ProbeRsLiveBackend;
use flash_core::conformance::check_backend;
use flash_core::types::ConnectionConfig;

#[test]
#[ignore = "needs a debug probe and a board that is safe to erase"]
fn probe_rs_backend_conforms() {
    let Ok(target) = std::env::var("FLASHGUI_HW_TARGET") else {
        panic!("set FLASHGUI_HW_TARGET to the attached part number");
    };

    let backend = ProbeRsLiveBackend::new();
    let config = ConnectionConfig {
        probe_id: std::env::var("FLASHGUI_HW_PROBE").ok(),
        target_name: target,
        ..Default::default()
    };

    if let Err(failure) = check_backend(&backend, &config) {
        panic!("probe-rs backend breaks the backend contract -- {failure}");
    }
}
