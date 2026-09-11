use firmware_parser::MemorySegment;
use flash_core::mock::{InjectedFault, MockProbeBackend};
use flash_core::traits::FlashBackend;
use flash_core::types::{ConnectionConfig, ProgramOptions};
use flash_core::{FlashError, FlashManager};

#[test]
fn test_connection_loss_fault() {
    let backend = MockProbeBackend::new();

    // Inject connection loss
    backend.inject_fault(InjectedFault::ConnectionLost(
        "Probe USB cable disconnected".to_string(),
    ));

    let config = ConnectionConfig::default();
    let err = match backend.open_session(&config) {
        Err(e) => e,
        Ok(_) => panic!("Connection must fail when ConnectionLost fault is injected"),
    };

    match err {
        FlashError::ConnectionLost(msg) => {
            assert!(msg.contains("Probe USB cable disconnected"));
        }
        other => panic!("Expected ConnectionLost error, got: {:?}", other),
    }

    // Clear fault and confirm connection succeeds
    backend.clear_faults();
    let mut session = backend.open_session(&config).unwrap();

    // Inject connection lost on an active session via backend
    backend.inject_fault(InjectedFault::ConnectionLost(
        "Communication dropped during erase".to_string(),
    ));

    let erase_err = session.erase_all(None).expect_err("Erase must fail on dropped connection");
    match erase_err {
        FlashError::ConnectionLost(msg) => {
            assert!(msg.contains("Communication dropped during erase"));
        }
        other => panic!("Expected ConnectionLost error, got: {:?}", other),
    }
}

#[test]
fn test_write_protection_fault() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f401re".to_string(),
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();

    // Inject WriteProtected fault at Sector 0 base address
    let protected_addr = 0x0800_0000;
    backend.inject_fault(InjectedFault::WriteProtected {
        address: protected_addr,
    });

    // 1. Erasing protected address must fail with FlashProtected
    let erase_err = session
        .erase_range(protected_addr, 16 * 1024, None)
        .expect_err("Erasing protected sector must fail");
    match erase_err {
        FlashError::FlashProtected { address } => {
            assert_eq!(address, protected_addr);
        }
        other => panic!("Expected FlashProtected error, got: {:?}", other),
    }

    // 2. Programming protected address must fail with FlashProtected
    let seg = MemorySegment::new(protected_addr, vec![0x12, 0x34]);
    let prog_err = session
        .program(&[seg], &ProgramOptions::default(), None)
        .expect_err("Programming protected sector must fail");
    match prog_err {
        FlashError::FlashProtected { address } => {
            assert_eq!(address, protected_addr);
        }
        other => panic!("Expected FlashProtected error, got: {:?}", other),
    }
}

#[test]
fn test_programming_failure_midway_and_byte_limit() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f103c8".to_string(),
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();

    // 1. Fault at specific address
    let fault_addr = 0x0800_0020;
    backend.inject_fault(InjectedFault::ProgrammingFailed {
        address: fault_addr,
        message: "VDD brownout detected during write".to_string(),
    });

    let seg = MemorySegment::new(0x0800_0000, vec![0xAA; 64]);
    let opts = ProgramOptions {
        chunk_size: 16,
        ..Default::default()
    };
    let prog_err = session
        .program(&[seg], &opts, None)
        .expect_err("Programming must fail when encountering fault address");
    match prog_err {
        FlashError::ProgramError(msg) => {
            assert!(msg.contains("VDD brownout"));
            assert!(msg.contains(&format!("0x{:08X}", fault_addr)));
        }
        other => panic!("Expected ProgramError, got: {:?}", other),
    }

    // 2. Byte threshold limit fault
    backend.clear_faults();
    session.erase_all(None).unwrap();
    backend.inject_fault(InjectedFault::ProgramFailureAfterBytes {
        byte_limit: 100,
        message: "Internal FIFO overrun".to_string(),
    });

    let large_seg = MemorySegment::new(0x0800_0000, vec![0x55; 200]);
    let limit_err = session
        .program(&[large_seg], &opts, None)
        .expect_err("Programming must fail after exceeding byte limit");
    match limit_err {
        FlashError::ProgramError(msg) => {
            assert!(msg.contains("Byte limit 100 exceeded"));
        }
        other => panic!("Expected ProgramError on limit, got: {:?}", other),
    }
}

#[test]
fn test_verification_mismatch_and_corruption() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f401re".to_string(),
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();

    // Program golden payload
    let golden_data = vec![0x10, 0x20, 0x30, 0x40, 0x50];
    let seg = MemorySegment::new(0x0800_0000, golden_data.clone());
    session.program(std::slice::from_ref(&seg), &ProgramOptions::default(), None).unwrap();

    // Inject verification corruption at 0x08000002 (original was 0x30, will read 0x99)
    backend.inject_fault(InjectedFault::VerificationFailed {
        address: 0x0800_0002,
        corrupt_byte: 0x99,
    });

    // 1. Session verify report
    let report = session.verify(&[seg], None).unwrap();
    assert!(!report.success, "Verification report must indicate failure");
    assert_eq!(report.mismatches.len(), 1);
    assert_eq!(report.mismatches[0].address, 0x0800_0002);
    assert_eq!(report.mismatches[0].expected, 0x30);
    assert_eq!(report.mismatches[0].actual, 0x99);

    // 2. FlashManager pipeline integration
    let firmware = firmware_parser::parse_bin(&golden_data, 0x0800_0000).unwrap();
    let options = ProgramOptions {
        verify_after: true,
        reset_after: false,
        chip_erase: false,
        chunk_size: 64,
    };

    let pipeline_err = FlashManager::execute_flash(
        session.as_mut(),
        &firmware,
        &options,
        None,
    )
    .expect_err("FlashManager must abort on verification mismatch");

    match pipeline_err {
        FlashError::VerificationMismatch { address, expected, actual } => {
            assert_eq!(address, 0x0800_0002);
            assert_eq!(expected, 0x30);
            assert_eq!(actual, 0x99);
        }
        other => panic!("Expected VerificationMismatch error, got: {:?}", other),
    }
}

#[test]
fn test_nor_flash_bit_clearing_violation() {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f103c8".to_string(),
        ..Default::default()
    };
    let mut session = backend.open_session(&config).unwrap();

    // Initially flash is 0xFF.
    // Writing 0x00 transitions 1 -> 0 (valid NOR write)
    let seg_zero = MemorySegment::new(0x0800_0000, vec![0x00]);
    session.program(&[seg_zero], &ProgramOptions::default(), None).unwrap();
    assert_eq!(session.read_memory(0x0800_0000, 1).unwrap(), vec![0x00]);

    // Now attempt to write 0xFF (or 0x01) over 0x00 without erasing.
    // In physical NOR flash, 0 cannot transition to 1 without a sector erase.
    let seg_violation = MemorySegment::new(0x0800_0000, vec![0x01]);
    let err = session
        .program(std::slice::from_ref(&seg_violation), &ProgramOptions::default(), None)
        .expect_err("Must reject bit 0 -> 1 transition without erase");

    match err {
        FlashError::NorFlashWriteViolation {
            address,
            attempted,
            current,
        } => {
            assert_eq!(address, 0x0800_0000);
            assert_eq!(attempted, 0x01);
            assert_eq!(current, 0x00);
        }
        other => panic!("Expected NorFlashWriteViolation, got: {:?}", other),
    }

    // Now perform sector erase to restore bits to 1 (0xFF)
    session.erase_range(0x0800_0000, 1024, None).unwrap();
    assert_eq!(session.read_memory(0x0800_0000, 1).unwrap(), vec![0xFF]);

    // Writing 0x01 now succeeds
    session.program(&[seg_violation], &ProgramOptions::default(), None).unwrap();
    assert_eq!(session.read_memory(0x0800_0000, 1).unwrap(), vec![0x01]);
}
