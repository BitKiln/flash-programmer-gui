//! Reads the first bytes of flash from the connected target, to confirm what is
//! actually programmed there.
//!
//! Usage: cargo run -p flash-core --example read_flash_head
use flash_core::traits::FlashBackend;
use flash_core::types::{ConnectionConfig, WireProtocol};

fn main() {
    let backend = flash_core::live::ProbeRsLiveBackend::new();
    let config = ConnectionConfig {
        probe_id: None,
        target_name: "auto".to_string(),
        protocol: WireProtocol::Swd,
        speed_khz: 4000,
        connect_under_reset: false,
        reset_type: None,
    };

    let mut session = backend.open_session(&config).expect("open session");
    let base = session
        .target_info()
        .map(|info| info.flash_base)
        .unwrap_or(0x0800_0000);

    let data = session.read_memory(base, 32).expect("read flash");
    println!("flash @ 0x{base:08X}: {data:02X?}");
    println!(
        "erased: {}",
        data.iter().all(|b| *b == 0xFF)
    );
}
