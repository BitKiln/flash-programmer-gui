use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use firmware_parser::MemorySegment;
use flash_core::mock::MockProbeBackend;
use flash_core::traits::FlashBackend;
use flash_core::types::{ConnectionConfig, ProgramOptions, WireProtocol};
use flash_core::{ClosureProgressCallback, FlashEvent, FlashManager, FlashStage};

#[test]
fn test_probe_listing_inventory() {
    let backend = MockProbeBackend::new();
    let probes = backend.list_probes().expect("Listing probes should succeed");

    assert!(probes.len() >= 3, "Expected at least 3 simulated probes");

    let identifiers: Vec<&str> = probes.iter().map(|p| p.identifier.as_str()).collect();
    assert!(
        identifiers.contains(&"mock:stlink-stm32f103"),
        "Missing ST-Link probe"
    );
    assert!(
        identifiers.contains(&"mock:cmsis-dap-stm32f401"),
        "Missing CMSIS-DAP probe"
    );
    assert!(
        identifiers.contains(&"mock:jlink-cortex-m"),
        "Missing J-Link probe"
    );

    // Verify probe capabilities
    let stlink = probes
        .iter()
        .find(|p| p.identifier == "mock:stlink-stm32f103")
        .unwrap();
    assert_eq!(stlink.vendor_name, "STMicroelectronics");
    assert!(stlink.supported_protocols.contains(&WireProtocol::Swd));
    assert!(stlink.supported_protocols.contains(&WireProtocol::Jtag));
    assert!(stlink.default_speed_khz > 0);
    assert!(stlink.max_speed_khz >= stlink.default_speed_khz);
}

#[test]
fn test_session_connection_stm32f1_and_stm32f4() {
    let backend = MockProbeBackend::new();

    // 1. Connect to STM32F103C8 (Medium-density 64KB, uniform 1KB sectors)
    let config_f1 = ConnectionConfig {
        probe_id: Some("mock:stlink-stm32f103".to_string()),
        target_name: "stm32f103c8".to_string(),
        protocol: WireProtocol::Swd,
        speed_khz: 4000,
        connect_under_reset: false,
        reset_type: None,
    };
    let session_f1 = backend
        .open_session(&config_f1)
        .expect("Opening STM32F1 session must succeed");
    let target_f1 = session_f1
        .target_info()
        .expect("Target info must be present");
    assert_eq!(target_f1.flash_base, 0x0800_0000);
    assert_eq!(target_f1.flash_size, 64 * 1024);
    assert_eq!(target_f1.sectors.len(), 64);
    assert_eq!(target_f1.sectors[0].size, 1024);

    // 2. Connect to STM32F401RE (512KB, asymmetric sectors: 4x16KB, 1x64KB, 3x128KB)
    let config_f4 = ConnectionConfig {
        probe_id: Some("mock:cmsis-dap-stm32f401".to_string()),
        target_name: "stm32f401re".to_string(),
        protocol: WireProtocol::Swd,
        speed_khz: 4000,
        connect_under_reset: false,
        reset_type: None,
    };
    let session_f4 = backend
        .open_session(&config_f4)
        .expect("Opening STM32F4 session must succeed");
    let target_f4 = session_f4
        .target_info()
        .expect("Target info must be present");
    assert_eq!(target_f4.flash_base, 0x0800_0000);
    assert_eq!(target_f4.flash_size, 512 * 1024);
    assert_eq!(target_f4.sectors.len(), 8);
    assert_eq!(target_f4.sectors[0].size, 16 * 1024);
    assert_eq!(target_f4.sectors[4].size, 64 * 1024);
    assert_eq!(target_f4.sectors[5].size, 128 * 1024);
}

#[test]
fn test_mass_erase_lifecycle_and_blank_check() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f103c8".to_string(),
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();

    // First program some data so flash is not in erased state
    let segment = MemorySegment::new(0x0800_0000, vec![0x11, 0x22, 0x33, 0x44]);
    session
        .program(&[segment], &ProgramOptions::default(), None)
        .unwrap();

    // Verify it is not 0xFF
    let read_back = session.read_memory(0x0800_0000, 4).unwrap();
    assert_eq!(read_back, vec![0x11, 0x22, 0x33, 0x44]);

    // Track progress events
    let stage_events = Arc::new(AtomicUsize::new(0));
    let stage_events_clone = Arc::clone(&stage_events);

    let callback = ClosureProgressCallback::new(
        move |event| {
            if let FlashEvent::StageStarted { stage, .. } = event {
                if stage == FlashStage::Erasing {
                    stage_events_clone.fetch_add(1, Ordering::SeqCst);
                }
            }
        },
        None::<fn() -> bool>,
    );

    // Execute mass erase
    session.erase_all(Some(&callback)).unwrap();
    assert_eq!(
        stage_events.load(Ordering::SeqCst),
        1,
        "Erasing stage must start"
    );

    // Read back entire sector 0 — all bytes must be 0xFF
    let erased = session.read_memory(0x0800_0000, 1024).unwrap();
    assert!(
        erased.iter().all(|&b| b == 0xFF),
        "All bytes must be 0xFF after mass erase"
    );
}

#[test]
fn test_sector_erase_granularity() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f103c8".to_string(), // 1KB sectors
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();

    // Program Sector 0 (0x08000000) and Sector 1 (0x08000400)
    let seg0 = MemorySegment::new(0x0800_0000, vec![0xAA; 100]);
    let seg1 = MemorySegment::new(0x0800_0400, vec![0xBB; 100]);
    session
        .program(&[seg0, seg1], &ProgramOptions::default(), None)
        .unwrap();

    // Confirm both are written
    assert_eq!(session.read_memory(0x0800_0000, 4).unwrap(), vec![0xAA; 4]);
    assert_eq!(session.read_memory(0x0800_0400, 4).unwrap(), vec![0xBB; 4]);

    // Erase only Sector 0
    session.erase_range(0x0800_0000, 50, None).unwrap();

    // Sector 0 must be 0xFF
    let s0_bytes = session.read_memory(0x0800_0000, 100).unwrap();
    assert!(
        s0_bytes.iter().all(|&b| b == 0xFF),
        "Sector 0 must be erased"
    );

    // Sector 1 must remain intact with 0xBB
    let s1_bytes = session.read_memory(0x0800_0400, 100).unwrap();
    assert_eq!(s1_bytes, vec![0xBB; 100], "Sector 1 must not be modified");
}

#[test]
fn test_single_and_multi_segment_programming_with_sparse_gap() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f401re".to_string(),
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();

    // Segment 1: Bootloader at 0x08000000 (32 bytes)
    let bootloader_data = (0..32u8).collect::<Vec<u8>>();
    let seg1 = MemorySegment::new(0x0800_0000, bootloader_data.clone());

    // Segment 2: App at 0x08010000 (64 bytes) — 64KB sparse gap in between
    let app_data = vec![0x5A; 64];
    let seg2 = MemorySegment::new(0x0801_0000, app_data.clone());

    let opts = ProgramOptions {
        chunk_size: 16,
        ..Default::default()
    };
    session.program(&[seg1.clone(), seg2.clone()], &opts, None).unwrap();

    // Verify Segment 1
    assert_eq!(
        session.read_memory(0x0800_0000, 32).unwrap(),
        bootloader_data
    );

    // Verify Gap between 0x08000020 and 0x08010000 remains 0xFF
    let gap_sample = session.read_memory(0x0800_0100, 128).unwrap();
    assert!(
        gap_sample.iter().all(|&b| b == 0xFF),
        "Gap must remain pristine 0xFF"
    );

    // Verify Segment 2
    assert_eq!(session.read_memory(0x0801_0000, 64).unwrap(), app_data);

    // Verification method
    let report = session.verify(&[seg1, seg2], None).unwrap();
    assert!(report.success, "Verification must succeed");
    assert_eq!(report.bytes_verified, 96);
    assert_eq!(report.mismatches.len(), 0);
    assert_eq!(report.checksum_expected, report.checksum_actual);
}

#[test]
fn test_reset_cycles_and_memory_persistence() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f103c8".to_string(),
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();

    // Program payload
    let payload = vec![0xCA, 0xFE, 0xBA, 0xBE];
    let seg = MemorySegment::new(0x0800_0000, payload.clone());
    session.program(&[seg], &ProgramOptions::default(), None).unwrap();

    // Reset without halt
    session.reset(false).unwrap();

    // Data persists across reset
    assert_eq!(session.read_memory(0x0800_0000, 4).unwrap(), payload);

    // Reset with halt
    session.reset(true).unwrap();
    assert_eq!(session.read_memory(0x0800_0000, 4).unwrap(), payload);
}

#[test]
fn test_flash_manager_pipeline_execution() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f401re".to_string(),
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();

    // Create realistic firmware image using firmware_parser
    let firmware_bytes = vec![0x33; 256];
    let firmware = firmware_parser::parse_bin(&firmware_bytes, 0x0800_0000)
        .expect("Binary parse must succeed");

    let options = ProgramOptions {
        verify_after: true,
        reset_after: true,
        chip_erase: false,
        chunk_size: 64,
    };

    let events = Arc::new(AtomicUsize::new(0));
    let events_clone = Arc::clone(&events);
    let callback = ClosureProgressCallback::new(
        move |_| {
            events_clone.fetch_add(1, Ordering::SeqCst);
        },
        None::<fn() -> bool>,
    );

    let result = FlashManager::execute_flash(
        session.as_mut(),
        &firmware,
        &options,
        Some(&callback),
    )
    .expect("FlashManager execution must succeed");

    assert!(result.success);
    assert_eq!(result.bytes_flashed, 256);
    assert!(result.reset_performed);
    assert!(result.verify_report.is_some());
    assert!(result.verify_report.unwrap().success);
    assert!(events.load(Ordering::SeqCst) > 0);

    // Confirm memory on target matches
    let mem = session.read_memory(0x0800_0000, 256).unwrap();
    assert_eq!(mem, firmware_bytes);
}

#[test]
fn test_flash_manager_out_of_bounds_rejection() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f103c8".to_string(), // 64KB: 0x08000000..0x08010000
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();

    // Segment placed past 64KB limit: 0x08010000 is 1 byte out of bounds
    let firmware = firmware_parser::parse_bin(&[0x12; 16], 0x0801_0000)
        .expect("Binary parse must succeed");

    let err = FlashManager::execute_flash(
        session.as_mut(),
        &firmware,
        &ProgramOptions::default(),
        None,
    )
    .expect_err("Must reject out-of-bounds segment");

    match err {
        flash_core::FlashError::AddressOutOfBounds { address, base, size } => {
            assert_eq!(base, 0x0800_0000);
            assert_eq!(size, 64 * 1024);
            assert!(address >= base + size);
        }
        other => panic!("Unexpected error: {:?}", other),
    }
}
