//! Reads the first bytes of flash from the connected target, to confirm what is
//! actually programmed there.
//!
//! Usage: cargo run -p flash-core --example read_flash_head [address] [length]
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
    let mut args = std::env::args().skip(1);
    let base = args
        .next()
        .map(|a| {
            let trimmed = a.trim_start_matches("0x").trim_start_matches("0X");
            u32::from_str_radix(trimmed, 16).expect("address must be hexadecimal")
        })
        .unwrap_or_else(|| {
            session
                .target_info()
                .map(|info| info.flash_base)
                .unwrap_or(0x0800_0000)
        });
    let length = args
        .next()
        .map(|a| a.parse::<u32>().expect("length must be a number"))
        .unwrap_or(32);

    let data = session.read_memory(base, length).expect("read flash");
    println!("flash @ 0x{base:08X}: {data:02X?}");
    println!(
        "erased: {}",
        data.iter().all(|b| *b == 0xFF)
    );
}
