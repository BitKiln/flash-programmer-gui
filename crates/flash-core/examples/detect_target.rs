//! Prints what the live backend detects on the connected probe.
//!
//! Run with: cargo run -p flash-core --example detect_target
use flash_core::traits::FlashBackend;
use flash_core::types::{ConnectionConfig, WireProtocol};

fn main() {
    let backend = flash_core::live::ProbeRsLiveBackend::new();

    for probe in backend.list_probes().expect("listing probes") {
        println!("probe: {} ({})", probe.identifier, probe.product_name);
    }

    let config = ConnectionConfig {
        probe_id: None,
        target_name: "auto".to_string(),
        protocol: WireProtocol::Swd,
        speed_khz: 4000,
        connect_under_reset: false,
        reset_type: None,
    };

    match backend.open_session(&config) {
        Ok(session) => match session.target_info() {
            Some(info) => println!(
                "detected: {} [{}] {} | flash 0x{:08X} {} KB | ram 0x{:08X} {} KB | page {} B | {} sectors",
                info.name,
                info.display_name.as_deref().unwrap_or("-"),
                info.architecture,
                info.flash_base,
                info.flash_size / 1024,
                info.ram_base,
                info.ram_size / 1024,
                info.page_size,
                info.sectors.len()
            ),
            None => println!("connected, but no target info"),
        },
        Err(e) => println!("failed: {e}"),
    }
}
