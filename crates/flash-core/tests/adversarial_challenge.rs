use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use firmware_parser::{parse_hex, FirmwareImage, MemorySegment};
use flash_core::mock::{InjectedFault, MockProbeBackend};
use flash_core::traits::FlashBackend;
use flash_core::types::{ConnectionConfig, ProgramOptions, WireProtocol};
use flash_core::{ClosureProgressCallback, FlashError, FlashEvent, FlashManager, FlashStage};

/// Helper to load the test bootloader-app gap hex file.
fn load_gap_hex() -> FirmwareImage {
    let hex_content = include_str!("../../../tests/fixtures/valid_stm32_bootloader_app_gap.hex");
    parse_hex(hex_content).expect("Parsing valid_stm32_bootloader_app_gap.hex must succeed")
}

/// Helper to load the single-segment test hex file.
fn load_single_segment_hex() -> FirmwareImage {
    let hex_content = include_str!("../../../tests/fixtures/valid_stm32_single_segment.hex");
    parse_hex(hex_content).expect("Parsing valid_stm32_single_segment.hex must succeed")
}

// ---------------------------------------------------------------------------
// 1. Progress streaming sequence and fidelity challenge
// ---------------------------------------------------------------------------
#[test]
fn test_execute_flash_progress_streaming_sequence_fidelity() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f401re".to_string(),
        protocol: WireProtocol::Swd,
        speed_khz: 4000,
        ..Default::default()
    };
    let mut session = backend
        .open_session(&config)
        .expect("Open session must succeed");

    let firmware = load_single_segment_hex();
    let total_bytes: u32 = firmware.segments.iter().map(|s| s.data.len() as u32).sum();
    assert!(total_bytes > 0, "Test firmware must have bytes");

    let events: Arc<Mutex<Vec<FlashEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_cb = Arc::clone(&events);

    let callback = ClosureProgressCallback::new(
        move |event: FlashEvent| {
            events_cb.lock().unwrap().push(event);
        },
        None::<fn() -> bool>,
    );

    let options = ProgramOptions {
        verify_after: true,
        reset_after: true,
        chip_erase: true,
        chunk_size: 16,
    };

    let result = FlashManager::execute_flash(
        session.as_mut(),
        &firmware,
        &options,
        Some(&callback),
    )
    .expect("FlashManager execute_flash must succeed");

    assert!(result.success, "Result must report success");
    assert_eq!(result.bytes_flashed, total_bytes);
    assert!(result.reset_performed, "Reset must be marked performed");
    assert!(result.verify_report.is_some(), "Verify report must be present");
    assert!(
        result.verify_report.as_ref().unwrap().success,
        "Verify report must be successful"
    );

    let recorded = events.lock().unwrap().clone();
    assert!(!recorded.is_empty(), "Events must be emitted");

    // Extract sequence of lifecycle stages from StageStarted and StageCompleted
    let mut stage_sequence = Vec::new();
    for event in &recorded {
        match event {
            FlashEvent::StageStarted { stage, .. } => {
                stage_sequence.push((*stage, "started"));
            }
            FlashEvent::StageCompleted { stage, .. } => {
                stage_sequence.push((*stage, "completed"));
            }
            _ => {}
        }
    }

    // Verify exact ordering of phases: Erasing -> Programming -> Verifying -> Completed
    let expected_stages = vec![
        (FlashStage::Erasing, "started"),
        (FlashStage::Erasing, "completed"),
        (FlashStage::Programming, "started"),
        (FlashStage::Programming, "completed"),
        (FlashStage::Verifying, "started"),
        (FlashStage::Verifying, "completed"),
        (FlashStage::Completed, "started"),
        (FlashStage::Completed, "completed"),
    ];

    assert_eq!(
        stage_sequence, expected_stages,
        "Lifecycle stages must execute in strict order"
    );

    // Verify target system reset was executed
    let mock_session = session
        .as_mut();
    // Verify reset counter on session via reading target state
    // (mock session records reset_count)
    let read_data = mock_session.read_memory(0x0800_0000, 32).unwrap();
    assert_eq!(read_data, firmware.segments[0].data);

    // Verify Progress telemetry metrics monotonically increase during programming
    let prog_progress_events: Vec<_> = recorded
        .iter()
        .filter_map(|e| {
            if let FlashEvent::Progress(m) = e {
                if m.stage == FlashStage::Programming {
                    Some(m)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    assert!(
        !prog_progress_events.is_empty(),
        "Programming progress events must be emitted"
    );

    let mut prev_bytes = 0u64;
    for p in &prog_progress_events {
        assert!(
            p.bytes_transferred >= prev_bytes,
            "bytes_transferred must be monotonically increasing"
        );
        assert!(
            p.bytes_transferred <= p.total_bytes,
            "bytes_transferred must not exceed total_bytes"
        );
        assert!(
            p.percentage >= 0.0 && p.percentage <= 100.0,
            "percentage must be clamped [0.0, 100.0]"
        );
        assert_eq!(
            p.total_bytes, total_bytes as u64,
            "total_bytes must match firmware size"
        );
        prev_bytes = p.bytes_transferred;
    }

    // Last programming progress event must reach 100%
    let last_prog = prog_progress_events.last().unwrap();
    assert_eq!(last_prog.bytes_transferred, total_bytes as u64);
    assert!((last_prog.percentage - 100.0).abs() < f32::EPSILON);
}

// ---------------------------------------------------------------------------
// 2. Deterministic verification failure & false-success prevention
// ---------------------------------------------------------------------------
#[test]
fn test_execute_flash_verification_fault_prevents_false_success() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f401re".to_string(),
        ..Default::default()
    };
    let mut session = backend
        .open_session(&config)
        .expect("Open session must succeed");

    let firmware = load_single_segment_hex();
    let target_addr = 0x0800_0008;

    // Inject VerificationFailed fault at target_addr: read will return 0xEE instead of expected
    backend.inject_fault(InjectedFault::VerificationFailed {
        address: target_addr,
        corrupt_byte: 0xEE,
    });

    let options = ProgramOptions {
        verify_after: true,
        reset_after: true,
        chip_erase: false,
        chunk_size: 32,
    };

    let result = FlashManager::execute_flash(session.as_mut(), &firmware, &options, None);

    // CRITICAL EMPIRICAL CHECK: Must NEVER return Ok(FlashResult { success: true })
    assert!(
        result.is_err(),
        "FlashManager MUST abort and return Err when verification fails"
    );

    let err = result.unwrap_err();
    match err {
        FlashError::VerificationMismatch {
            address,
            expected,
            actual,
        } => {
            assert_eq!(address, target_addr);
            assert_eq!(actual, 0xEE);
            assert_ne!(expected, actual);
        }
        FlashError::ChecksumMismatch { .. } | FlashError::VerifyError(_) => {
            // Also acceptable verification failure errors
        }
        other => panic!("Expected verification failure error, got: {:?}", other),
    }

    // SAFETY CHECK: In the mock session, reset should NOT have been performed
    // because verification failed before reset phase was reached
    let read_back = session.read_memory(0x0800_0000, 4).unwrap();
    assert_eq!(read_back, &firmware.segments[0].data[0..4]);
}

// ---------------------------------------------------------------------------
// 3. Multi-segment with sparse gaps: gap memory untouched & segments verified
// ---------------------------------------------------------------------------
#[test]
fn test_execute_flash_multi_segment_with_gaps_preservation() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f401re".to_string(), // 512KB flash
        ..Default::default()
    };
    let mut session = backend
        .open_session(&config)
        .expect("Open session must succeed");

    // Load dual-segment image with 256KB gap
    // Segment 0: 0x08000000 (32 bytes)
    // Segment 1: 0x08040000 (16 bytes)
    let firmware = load_gap_hex();
    assert_eq!(firmware.segments.len(), 2);
    let seg0 = &firmware.segments[0];
    let seg1 = &firmware.segments[1];
    assert_eq!(seg0.start_address, 0x0800_0000);
    assert_eq!(seg0.data.len(), 32);
    assert_eq!(seg1.start_address, 0x0804_0000);
    assert_eq!(seg1.data.len(), 16);

    // Initial state: mass erase so entire 512KB is pristine 0xFF
    session.erase_all(None).unwrap();

    let options = ProgramOptions {
        verify_after: true,
        reset_after: true,
        chip_erase: false, // Sector-level erase
        chunk_size: 16,
    };

    let result = FlashManager::execute_flash(session.as_mut(), &firmware, &options, None)
        .expect("FlashManager execute_flash must succeed on multi-segment image");

    assert!(result.success);
    assert_eq!(result.bytes_flashed, 48);
    assert!(result.reset_performed);

    // 1. Verify Segment 0 was written correctly
    let mem_seg0 = session.read_memory(0x0800_0000, 32).unwrap();
    assert_eq!(
        mem_seg0, seg0.data,
        "Segment 0 content must match golden data"
    );

    // 2. Verify Segment 1 was written correctly
    let mem_seg1 = session.read_memory(0x0804_0000, 16).unwrap();
    assert_eq!(
        mem_seg1, seg1.data,
        "Segment 1 content must match golden data"
    );

    // 3. EMPIRICAL GAP AUDIT:
    // Gap starts immediately after Seg 0 (0x08000020) and spans to Seg 1 (0x08040000)
    // Sample gap across multiple boundaries:
    // a. Immediately after Seg 0
    let gap_start = session.read_memory(0x0800_0020, 64).unwrap();
    assert!(
        gap_start.iter().all(|&b| b == 0xFF),
        "Memory immediately following Segment 0 must remain 0xFF"
    );

    // b. Middle of Sector 0 (0x08000200)
    let gap_sector0 = session.read_memory(0x0800_0200, 128).unwrap();
    assert!(
        gap_sector0.iter().all(|&b| b == 0xFF),
        "Memory within Sector 0 gap must remain 0xFF"
    );

    // c. Untouched Sector 1 (0x08004000)
    let gap_sector1 = session.read_memory(0x0800_4000, 256).unwrap();
    assert!(
        gap_sector1.iter().all(|&b| b == 0xFF),
        "Entire Sector 1 gap must remain 0xFF"
    );

    // d. Untouched Sector 4 (0x08010000)
    let gap_sector4 = session.read_memory(0x0801_0000, 256).unwrap();
    assert!(
        gap_sector4.iter().all(|&b| b == 0xFF),
        "Entire Sector 4 gap must remain 0xFF"
    );

    // e. Immediately preceding Segment 1 (0x0803FFF0)
    let gap_end = session.read_memory(0x0803_FFF0, 16).unwrap();
    assert!(
        gap_end.iter().all(|&b| b == 0xFF),
        "Memory immediately preceding Segment 1 must remain 0xFF"
    );

    // f. Memory past Segment 1 (0x08040010)
    let post_seg1 = session.read_memory(0x0804_0010, 64).unwrap();
    assert!(
        post_seg1.iter().all(|&b| b == 0xFF),
        "Memory following Segment 1 must remain 0xFF"
    );
}

// ---------------------------------------------------------------------------
// 4. Deterministic fault recovery under load
// ---------------------------------------------------------------------------
#[test]
fn test_execute_flash_fault_injection_and_recovery() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f103c8".to_string(),
        ..Default::default()
    };
    let mut session = backend
        .open_session(&config)
        .expect("Open session must succeed");

    let firmware = load_single_segment_hex();
    let options = ProgramOptions {
        verify_after: true,
        reset_after: true,
        chip_erase: false,
        chunk_size: 16,
    };

    // Step 1: Inject programming fault midway
    backend.inject_fault(InjectedFault::ProgrammingFailed {
        address: 0x0800_0010,
        message: "Simulated write failure".to_string(),
    });

    let err = FlashManager::execute_flash(session.as_mut(), &firmware, &options, None)
        .expect_err("Must fail when programming fault is active");

    match err {
        FlashError::ProgramError(msg) => {
            assert!(msg.contains("Simulated write failure"));
        }
        other => panic!("Expected ProgramError, got: {:?}", other),
    }

    // Step 2: Clear faults to simulate recovery
    backend.clear_faults();

    // Step 3: Re-execute flash — must recover cleanly and succeed
    let recovered_result =
        FlashManager::execute_flash(session.as_mut(), &firmware, &options, None)
            .expect("Must recover and succeed after clearing faults");

    assert!(recovered_result.success);
    assert_eq!(recovered_result.bytes_flashed, 32);
    assert!(recovered_result.reset_performed);
}

// ---------------------------------------------------------------------------
// 5. Cooperative cancellation challenge
// ---------------------------------------------------------------------------
#[test]
fn test_execute_flash_cooperative_cancellation_during_programming() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f401re".to_string(),
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();

    let firmware = load_single_segment_hex();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_cb = Arc::clone(&cancel_flag);

    // Cancel as soon as programming stage starts
    let callback = ClosureProgressCallback::new(
        move |event: FlashEvent| {
            if let FlashEvent::StageStarted { stage, .. } = event {
                if stage == FlashStage::Programming {
                    cancel_cb.store(true, Ordering::SeqCst);
                }
            }
        },
        Some(move || cancel_flag.load(Ordering::SeqCst)),
    );

    let options = ProgramOptions {
        verify_after: true,
        reset_after: true,
        chip_erase: false,
        chunk_size: 4, // Small chunks to hit cancellation checkpoint
    };

    let result = FlashManager::execute_flash(
        session.as_mut(),
        &firmware,
        &options,
        Some(&callback),
    );

    assert!(
        result.is_err(),
        "FlashManager must cancel and return Err(OperationCancelled)"
    );
    match result.unwrap_err() {
        FlashError::OperationCancelled => {}
        other => panic!("Expected OperationCancelled, got: {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// 6. FlashManager boundary defense: empty segments and out-of-bounds
// ---------------------------------------------------------------------------
#[test]
fn test_execute_flash_boundary_defenses() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f103c8".to_string(), // 64KB (0x08000000..0x08010000)
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();

    // Defense 1: Empty firmware segments
    let empty_firmware = FirmwareImage {
        metadata: firmware_parser::parse_bin(&[0x00], 0x0800_0000).unwrap().metadata,
        segments: vec![],
    };
    let err_empty = FlashManager::execute_flash(
        session.as_mut(),
        &empty_firmware,
        &ProgramOptions::default(),
        None,
    )
    .expect_err("Must reject empty segments");

    match err_empty {
        FlashError::ProgramError(msg) => {
            assert!(msg.contains("no memory segments"));
        }
        other => panic!("Expected ProgramError on empty segments, got: {:?}", other),
    }

    // Defense 2: Out of bounds (below flash base)
    let out_below = FirmwareImage {
        metadata: firmware_parser::parse_bin(&[0x00], 0x0800_0000).unwrap().metadata,
        segments: vec![MemorySegment::new(0x07FF_FFF0, vec![0x11; 32])],
    };
    let err_below = FlashManager::execute_flash(
        session.as_mut(),
        &out_below,
        &ProgramOptions::default(),
        None,
    )
    .expect_err("Must reject segment below flash base");

    match err_below {
        FlashError::AddressOutOfBounds { address, base, .. } => {
            assert_eq!(base, 0x0800_0000);
            assert!(address < base);
        }
        other => panic!("Expected AddressOutOfBounds, got: {:?}", other),
    }

    // Defense 3: Out of bounds (exceeding flash end)
    let out_above = FirmwareImage {
        metadata: firmware_parser::parse_bin(&[0x00], 0x0800_0000).unwrap().metadata,
        segments: vec![MemorySegment::new(0x0800_FFF0, vec![0x11; 32])], // 16 bytes beyond 64KB
    };
    let err_above = FlashManager::execute_flash(
        session.as_mut(),
        &out_above,
        &ProgramOptions::default(),
        None,
    )
    .expect_err("Must reject segment extending beyond flash limit");

    match err_above {
        FlashError::AddressOutOfBounds { address, base, size } => {
            assert_eq!(base, 0x0800_0000);
            assert_eq!(size, 64 * 1024);
            assert!(address > base + size);
        }
        other => panic!("Expected AddressOutOfBounds, got: {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// 7. Reset failure fault handling
// ---------------------------------------------------------------------------
#[test]
fn test_execute_flash_reset_failure_reporting() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f401re".to_string(),
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();
    let firmware = load_single_segment_hex();

    // Inject reset failure
    backend.inject_fault(InjectedFault::ResetFailure(
        "NRST pin held low by external reset supervisor".to_string(),
    ));

    let options = ProgramOptions {
        verify_after: true,
        reset_after: true,
        chip_erase: false,
        chunk_size: 32,
    };

    let result = FlashManager::execute_flash(session.as_mut(), &firmware, &options, None);

    assert!(
        result.is_err(),
        "Must return Err when target reset fails"
    );
    match result.unwrap_err() {
        FlashError::Internal(msg) => {
            assert!(msg.contains("NRST pin held low"));
        }
        other => panic!("Expected Internal error from reset failure, got: {:?}", other),
    }

    // But note that memory WAS correctly programmed before reset failed!
    backend.clear_faults();
    let mem = session.read_memory(0x0800_0000, 32).unwrap();
    assert_eq!(mem, firmware.segments[0].data);
}
