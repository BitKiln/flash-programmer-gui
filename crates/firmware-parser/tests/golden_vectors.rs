use firmware_parser::{
    compute_padded_checksums, detect_format, parse_bin, parse_bytes, parse_file, parse_hex,
    validate_target_bounds, EntryPointSource, FirmwareFormat, ParseError, ValidationWarning,
};

/// Vector 1: Standard Single-Segment STM32 Cortex-M Image
#[test]
fn test_golden_vector_1_standard_cortex_m() {
    let hex_input = "\
:020000040800F2
:1000000000500020CD010008D1010008D3010008F4
:10001000D5010008D7010008D90100080000000040
:04000005080001CD21
:00000001FF";

    let image = parse_hex(hex_input).expect("Vector 1 should parse successfully");
    let meta = &image.metadata;

    assert_eq!(meta.format, FirmwareFormat::IntelHex);
    assert_eq!(meta.segment_count, 1);
    assert_eq!(meta.segments.len(), 1);

    let seg0 = &meta.segments[0];
    assert_eq!(seg0.start_address, 0x08000000);
    assert_eq!(seg0.end_address, 0x08000020);
    assert_eq!(seg0.size_bytes, 32);

    assert_eq!(meta.entry_point, Some(0x080001CD));
    assert_eq!(meta.entry_point_source, EntryPointSource::Record05);

    assert_eq!(meta.crc32, 0x0A5B1F0D);
    assert_eq!(meta.checksums.crc32, "0x0A5B1F0D");
    assert_eq!(meta.checksums.md5, "629f18994ef216238cd67914482dfb03");
    assert_eq!(
        meta.checksums.sha256,
        "b2ed8017b38167a8bd05f1b60f972ee9479abc971762f40f14c61bbf753539fc"
    );

    assert_eq!(seg0.checksums.crc32, "0x0A5B1F0D");
    assert_eq!(seg0.checksums.md5, "629f18994ef216238cd67914482dfb03");
    assert_eq!(
        seg0.checksums.sha256,
        "b2ed8017b38167a8bd05f1b60f972ee9479abc971762f40f14c61bbf753539fc"
    );

    assert!(meta.warnings.is_empty());
    assert_eq!(meta.gap_count, 0);
    assert_eq!(meta.gap_bytes, 0);
    assert_eq!(meta.total_bytes, 32);
    assert_eq!(meta.total_firmware_bytes, 32);
    assert_eq!(meta.address_span, 32);
}

/// Vector 2: Dual-Segment Non-Contiguous Flash Image (Bootloader + App Gap)
#[test]
fn test_golden_vector_2_dual_segment_gap() {
    let hex_input = "\
:020000040800F2
:1000000000500020CD010008D1010008D3010008F4
:10001000D5010008D7010008D90100080000000040
:020000040804EE
:10000000DEADBEEFCAFEBABE0123456789ABCDEFB8
:00000001FF";

    let image = parse_hex(hex_input).expect("Vector 2 should parse successfully");
    let meta = &image.metadata;

    assert_eq!(meta.total_firmware_bytes, 48);
    assert_eq!(meta.total_bytes, 48);
    assert_eq!(meta.address_span, 262160); // 0x08040010 - 0x08000000 = 0x40010
    assert_eq!(meta.gap_count, 1);
    assert_eq!(meta.gap_bytes, 262112); // 0x08040000 - 0x08000020 = 0x3FFE0
    assert_eq!(meta.segment_count, 2);

    let seg0 = &meta.segments[0];
    assert_eq!(seg0.start_address, 0x08000000);
    assert_eq!(seg0.end_address, 0x08000020);
    assert_eq!(seg0.size_bytes, 32);
    assert_eq!(seg0.checksums.crc32, "0x0A5B1F0D");

    let seg1 = &meta.segments[1];
    assert_eq!(seg1.start_address, 0x08040000);
    assert_eq!(seg1.end_address, 0x08040010);
    assert_eq!(seg1.size_bytes, 16);
    assert_eq!(seg1.checksums.crc32, "0x3DB782CE");

    // Canonical concatenated checksums (48 bytes)
    assert_eq!(meta.crc32, 0x767B0A13);
    assert_eq!(meta.checksums.crc32, "0x767B0A13");
    assert_eq!(meta.checksums.md5, "0ec2d4eca06af318796c232fafeaa288");
    assert_eq!(
        meta.checksums.sha256,
        "05e9e73074789d23556e2f2a9daafbfb8c77207499ba5af61ff9858fcbd6bb64"
    );

    // Memory gaps verification
    assert_eq!(meta.memory_gaps.len(), 1);
    assert_eq!(meta.memory_gaps[0].start_address, 0x08000020);
    assert_eq!(meta.memory_gaps[0].end_address, 0x08040000);
    assert_eq!(meta.memory_gaps[0].size, 262112);
    assert_eq!(meta.memory_gaps[0].size_bytes(), 262112);

    // Padded erased (0xFF) flash checksums (262,160 bytes)
    let padded = compute_padded_checksums(&image.segments, 0xFF)
        .expect("Padded checksums should compute for non-empty segments");
    assert_eq!(padded.crc32, "0x32BA8639");
    assert_eq!(
        padded.sha256,
        "a2a25248ae9c7cc65fe74036956e311e00b3343992793a34fe4dd59776edf58a"
    );
}

/// Vector 3: Out-of-Order Records Normalization
#[test]
fn test_golden_vector_3_out_of_order_normalization() {
    let hex_input = "\
:020000040800F2
:10001000D5010008D7010008D90100080000000040
:1000000000500020CD010008D1010008D3010008F4
:00000001FF";

    let image = parse_hex(hex_input).expect("Vector 3 should parse successfully");
    let meta = &image.metadata;

    assert_eq!(meta.segment_count, 1);
    assert_eq!(meta.segments[0].start_address, 0x08000000);
    assert_eq!(meta.segments[0].end_address, 0x08000020);
    assert_eq!(meta.segments[0].size_bytes, 32);

    // Checksums must match Vector 1 identically
    assert_eq!(meta.crc32, 0x0A5B1F0D);
    assert_eq!(meta.checksums.crc32, "0x0A5B1F0D");
    assert_eq!(meta.checksums.md5, "629f18994ef216238cd67914482dfb03");
    assert_eq!(
        meta.checksums.sha256,
        "b2ed8017b38167a8bd05f1b60f972ee9479abc971762f40f14c61bbf753539fc"
    );

    // Data ordering verified: first 4 bytes are SP 0x20005000
    assert_eq!(&image.segments[0].data[0..4], &[0x00, 0x50, 0x00, 0x20]);
    // Next 4 bytes are Reset_Handler pointer 0x080001CD
    assert_eq!(&image.segments[0].data[4..8], &[0xCD, 0x01, 0x00, 0x08]);
    // Offset 0x10 onwards matches line 2
    assert_eq!(&image.segments[0].data[16..20], &[0xD5, 0x01, 0x00, 0x08]);
}

/// Vector 4: Raw Binary with Auto-Detected Cortex-M Reset Vector
#[test]
fn test_golden_vector_4_raw_binary_cortex_m() {
    let mut bin_data = vec![0x00u8; 1024];
    // Offset 0x00: Initial MSP = 0x20005000
    bin_data[0..4].copy_from_slice(&0x20005000u32.to_le_bytes());
    // Offset 0x04: Reset Handler = 0x080001CD (Thumb bit set, 0x080001CC is code in range)
    bin_data[4..8].copy_from_slice(&0x080001CDu32.to_le_bytes());

    let image = parse_bin(&bin_data, 0x08000000).expect("Vector 4 should parse successfully");
    let meta = &image.metadata;

    assert_eq!(meta.format, FirmwareFormat::RawBinary);
    assert_eq!(meta.segment_count, 1);
    assert_eq!(meta.segments[0].start_address, 0x08000000);
    assert_eq!(meta.segments[0].size_bytes, 1024);
    assert_eq!(meta.entry_point, Some(0x080001CD));
    assert_eq!(
        meta.entry_point_source,
        EntryPointSource::CortexMVectorTable
    );
}

/// Vector 5: Negative / Malformed Test Cases Matrix
#[test]
fn test_golden_vector_5_negative_matrix() {
    // Case A: Checksum mismatch
    let err_a = parse_hex(":020000040800F1\n:00000001FF").unwrap_err();
    assert_eq!(
        err_a,
        ParseError::ChecksumMismatch {
            line: 1,
            expected: 0xF2,
            found: 0xF1,
        }
    );
    assert!(err_a
        .to_string()
        .contains("Line 1: Checksum mismatch (calculated 0xF2, found 0xF1)"));

    // Case B: Missing leading colon
    let err_b = parse_hex("020000040800F2\n:00000001FF").unwrap_err();
    assert_eq!(err_b, ParseError::MissingLeadingColon { line: 1 });
    assert!(err_b
        .to_string()
        .contains("Line 1: Missing mandatory leading colon prefix"));

    // Case C: Odd number of hex characters
    let err_c = parse_hex(":02000004080F2\n:00000001FF").unwrap_err();
    assert_eq!(err_c, ParseError::OddHexDigitCount { line: 1, count: 13 });
    assert!(err_c
        .to_string()
        .contains("Line 1: Odd number of hexadecimal characters (13)"));

    // Case D: Invalid non-hexadecimal character
    let err_d = parse_hex(":020000040800FZ\n:00000001FF").unwrap_err();
    assert_eq!(
        err_d,
        ParseError::InvalidHexCharacter {
            line: 1,
            character: 'Z',
        }
    );
    assert!(err_d
        .to_string()
        .contains("Line 1: Invalid non-hexadecimal character 'Z'"));

    // Case E: Record truncated
    let err_e = parse_hex(":0200000408\n:00000001FF").unwrap_err();
    assert!(matches!(err_e, ParseError::RecordTruncated { line: 1, .. }));
    assert!(err_e.to_string().contains("Line 1: Record truncated"));

    // Case F: Invalid byte count for Type 04 (1 instead of 2)
    let err_f = parse_hex(":0100000408F3\n:00000001FF").unwrap_err();
    assert_eq!(
        err_f,
        ParseError::InvalidRecordLength {
            line: 1,
            record_type: 4,
            expected: 2,
            actual: 1,
        }
    );
    assert!(err_f
        .to_string()
        .contains("Line 1: Invalid byte count 1 for record type 0x04 (expected 2)"));

    // Case G: Invalid byte count for Type 01 (1 instead of 0)
    let err_g = parse_hex(":0100000100FE").unwrap_err();
    assert_eq!(
        err_g,
        ParseError::InvalidRecordLength {
            line: 1,
            record_type: 1,
            expected: 0,
            actual: 1,
        }
    );
    assert!(err_g
        .to_string()
        .contains("Line 1: Invalid byte count 1 for record type 0x01 (expected 0)"));

    // Case H: Conflicting data overlap
    let hex_h = "\
:020000040800F2
:01000000AA55
:01000000BB44
:00000001FF";
    let err_h = parse_hex(hex_h).unwrap_err();
    assert_eq!(
        err_h,
        ParseError::ConflictingDataOverlap {
            line: 3,
            address: 0x08000000,
            existing: 0xAA,
            incoming: 0xBB,
        }
    );
    assert!(err_h
        .to_string()
        .contains("Conflicting data overlap at physical address 0x08000000"));

    // Case I: Target out of bounds check
    let hex_i = "\
:020000040809E9
:100000000102030405060708090A0B0C0D0E0F1068
:00000001FF";
    let image_i = parse_hex(hex_i).expect("HEX should parse syntactically");
    // 512KB STM32: 0x08000000 to 0x08080000
    let err_i = validate_target_bounds(&image_i, 0x08000000, 512 * 1024).unwrap_err();
    assert_eq!(
        err_i,
        ParseError::TargetOutOfBounds {
            segment_start: 0x08090000,
            segment_end: 0x08090010,
            flash_limit: 0x08080000,
        }
    );
    assert!(err_i
        .to_string()
        .contains("Target out of bounds: Segment 0x08090000..0x08090010 exceeds target flash limit 0x08080000"));

    // Case J: Empty file
    let err_j = parse_hex("").unwrap_err();
    assert_eq!(err_j, ParseError::EmptyFile);
    assert_eq!(err_j.to_string(), "File is empty");

    let err_j_spaces = parse_hex("   \r\n  \n  ").unwrap_err();
    assert_eq!(err_j_spaces, ParseError::EmptyFile);
}

/// Test Record Type 02 (Extended Segment Address) and Record Type 03 (Start Segment Address)
#[test]
fn test_record_type_02_and_03_hex86() {
    let hex = "\
:020000021000EC
:040000001122334452
:0400000310000100E8
:00000001FF";

    let image = parse_hex(hex).expect("HEX86 should parse successfully");
    let meta = &image.metadata;

    // USBA = 0x1000 -> Base = 0x10000
    assert_eq!(meta.segments[0].start_address, 0x00010000);
    assert_eq!(meta.segments[0].size_bytes, 4);

    // Record 03: CS=0x1000, IP=0x0100 -> Entry = (0x1000 << 4) + 0x0100 = 0x10100
    assert_eq!(meta.entry_point, Some(0x00010100));
    assert_eq!(meta.entry_point_source, EntryPointSource::Record03);
}

/// Test redundant identical overlap generates a warning without failure
#[test]
fn test_redundant_overlap_handling() {
    let hex = "\
:020000040800F2
:04000000AABBCCDDEE
:04000000AABBCCDDEE
:00000001FF";

    let image = parse_hex(hex).expect("Redundant overlap should succeed");
    assert_eq!(image.metadata.segment_count, 1);
    assert_eq!(image.segments[0].size(), 4);
    assert_eq!(image.metadata.warnings.len(), 1);
    assert!(matches!(
        image.metadata.warnings[0],
        ValidationWarning::RedundantOverlap {
            address: 0x08000000,
            line: Some(3),
        }
    ));
}

/// Test data after EOF record emits warning
#[test]
fn test_data_after_eof_warning() {
    let hex = "\
:020000040800F2
:020000001122CB
:00000001FF
:02000200334485";

    let image = parse_hex(hex).expect("Should parse with warning");
    assert_eq!(image.metadata.warnings.len(), 1);
    assert!(matches!(
        image.metadata.warnings[0],
        ValidationWarning::DataAfterEndOfFile { line: 4 }
    ));
}

/// Test missing EOF record emits warning
#[test]
fn test_missing_eof_record_warning() {
    let hex = "\
:020000040800F2
:020000001122CB";

    let image = parse_hex(hex).expect("Should parse with missing EOF warning");
    assert_eq!(image.metadata.warnings.len(), 1);
    assert!(matches!(
        image.metadata.warnings[0],
        ValidationWarning::MissingEndOfFileRecord
    ));
}

/// Test Cortex-M vector heuristic rejection when reset handler has even address (Thumb bit unset)
#[test]
fn test_cortex_m_rejects_even_reset_handler() {
    let mut bin = vec![0u8; 32];
    bin[0..4].copy_from_slice(&0x20002000u32.to_le_bytes()); // valid MSP
    bin[4..8].copy_from_slice(&0x08000010u32.to_le_bytes()); // even address (bit 0 = 0)

    let image = parse_bin(&bin, 0x08000000).unwrap();
    assert_eq!(image.metadata.entry_point, None);
    assert_eq!(image.metadata.entry_point_source, EntryPointSource::None);
}

/// Test Cortex-M vector heuristic rejection when reset handler points outside loaded segments
#[test]
fn test_cortex_m_rejects_out_of_bounds_reset_handler() {
    let mut bin = vec![0u8; 32];
    bin[0..4].copy_from_slice(&0x20002000u32.to_le_bytes()); // valid MSP
    bin[4..8].copy_from_slice(&0x08001001u32.to_le_bytes()); // points way outside 32-byte segment

    let image = parse_bin(&bin, 0x08000000).unwrap();
    assert_eq!(image.metadata.entry_point, None);
    assert_eq!(image.metadata.entry_point_source, EntryPointSource::None);
}

/// Test unknown record type rejection
#[test]
fn test_unknown_record_type_rejection() {
    // Record type 0x09
    let hex = ":020000090102F2\n:00000001FF";
    let err = parse_hex(hex).unwrap_err();
    assert_eq!(
        err,
        ParseError::UnknownRecordType {
            line: 1,
            record_type: 9,
        }
    );
}

/// Test JSON serialization for Tauri IPC compatibility
#[test]
fn test_metadata_json_serialization() {
    let hex_input = "\
:020000040800F2
:1000000000500020CD010008D1010008D3010008F4
:10001000D5010008D7010008D90100080000000040
:04000005080001CD21
:00000001FF";

    let image = parse_hex(hex_input).unwrap();
    let json_str = serde_json::to_string_pretty(&image.metadata).unwrap();
    assert!(json_str.contains("\"format\": \"intel_hex\""));
    assert!(json_str.contains("\"crc32\": \"0x0A5B1F0D\""));
    assert!(json_str.contains("\"entry_point_source\": \"record05\""));

    // Verify deserialization back into struct
    let deserialized: firmware_parser::FirmwareMetadata =
        serde_json::from_str(&json_str).expect("Should deserialize back from JSON");
    assert_eq!(deserialized.total_bytes, 32);
    assert_eq!(deserialized.crc32, 0x0A5B1F0D);
}

/// Test file format auto-detection and parse_file / parse_bytes
#[test]
fn test_file_and_bytes_parsing() {
    let hex_content = b":020000040800F2\n:02000000AABB99\n:00000001FF";
    let bin_content = b"\x00\x50\x00\x20\x09\x00\x00\x08";

    // Auto-detect format from content
    assert_eq!(detect_format(hex_content, None), FirmwareFormat::IntelHex);
    assert_eq!(detect_format(bin_content, None), FirmwareFormat::RawBinary);

    // Auto-detect with file extension
    let dummy_hex_path = std::path::Path::new("firmware.hex");
    let dummy_bin_path = std::path::Path::new("firmware.bin");
    assert_eq!(
        detect_format(b"", Some(dummy_hex_path)),
        FirmwareFormat::IntelHex
    );
    assert_eq!(
        detect_format(b"", Some(dummy_bin_path)),
        FirmwareFormat::RawBinary
    );

    // Test parse_bytes
    let hex_image = parse_bytes(hex_content, None, None).unwrap();
    assert_eq!(hex_image.metadata.format, FirmwareFormat::IntelHex);
    assert_eq!(hex_image.metadata.total_bytes, 2);

    let bin_image = parse_bytes(bin_content, None, Some(0x08000000)).unwrap();
    assert_eq!(bin_image.metadata.format, FirmwareFormat::RawBinary);
    assert_eq!(bin_image.metadata.total_bytes, 8);

    // Test parse_file using a temp file
    let temp_dir = std::env::temp_dir();
    let temp_hex = temp_dir.join("test_firmware_parser_temp.hex");
    std::fs::write(&temp_hex, hex_content).unwrap();

    let file_image = parse_file(&temp_hex, None).unwrap();
    assert_eq!(file_image.metadata.format, FirmwareFormat::IntelHex);
    assert_eq!(file_image.metadata.total_bytes, 2);
    assert!(file_image.metadata.file_path.is_some());

    let _ = std::fs::remove_file(temp_hex);
}
