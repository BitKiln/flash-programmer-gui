use firmware_parser::{
    compute_canonical_checksums, compute_checksums, compute_padded_checksums, consolidate_chunks,
    detect_cortex_m_reset_vector, detect_format, parse_bin, parse_bytes, parse_hex,
    validate_target_bounds, EntryPointSource, FirmwareFormat, MemorySegment, ParseError, RawChunk,
    ValidationWarning,
};

/// Helper: generates a valid Intel HEX line from address (u16), record_type, and data
fn make_hex_line(address: u16, record_type: u8, data: &[u8]) -> String {
    let byte_count = data.len() as u8;
    let mut bytes = Vec::with_capacity(5 + data.len());
    bytes.push(byte_count);
    bytes.push((address >> 8) as u8);
    bytes.push((address & 0xFF) as u8);
    bytes.push(record_type);
    bytes.extend_from_slice(data);

    let sum: u8 = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    let checksum = (!sum).wrapping_add(1);
    bytes.push(checksum);

    let mut line = String::from(":");
    for b in bytes {
        line.push_str(&format!("{:02X}", b));
    }
    line
}

/// Helper: generates an Extended Linear Address (Type 04) record
fn make_type04_line(ulba: u16) -> String {
    make_hex_line(0, 0x04, &ulba.to_be_bytes())
}

/// Helper: generates an EOF (Type 01) record
fn make_eof_line() -> String {
    make_hex_line(0, 0x01, &[])
}

// =========================================================================
// 1. Corrupted Checksums & Tampering
// =========================================================================

#[test]
fn test_adversarial_checksum_1bit_flips() {
    let valid_line = make_hex_line(0x0000, 0x00, &[0xAA, 0xBB, 0xCC, 0xDD]);
    let hex_part = &valid_line[1..];
    let len = hex_part.len();
    let cs_byte = u8::from_str_radix(&hex_part[len - 2..], 16).unwrap();

    for bit in 0..8 {
        let flipped_cs = cs_byte ^ (1 << bit);
        let mut corrupted = valid_line[..valid_line.len() - 2].to_string();
        corrupted.push_str(&format!("{:02X}", flipped_cs));
        let full_hex = format!("{}\n{}", corrupted, make_eof_line());

        let err = parse_hex(&full_hex).expect_err(&format!("Bit {} flip must fail", bit));
        match err {
            ParseError::ChecksumMismatch {
                line,
                expected,
                found,
            } => {
                assert_eq!(line, 1);
                assert_eq!(expected, cs_byte);
                assert_eq!(found, flipped_cs);
            }
            other => panic!("Unexpected error variant: {:?}", other),
        }
    }
}

#[test]
fn test_adversarial_checksum_all_zeros_and_ones() {
    let valid_line = make_hex_line(0x0000, 0x00, &[0x10, 0x20]);
    let expected_cs = u8::from_str_radix(&valid_line[valid_line.len() - 2..], 16).unwrap();

    if expected_cs != 0x00 {
        let mut corrupted_0 = valid_line[..valid_line.len() - 2].to_string();
        corrupted_0.push_str("00");
        let hex = format!("{}\n{}", corrupted_0, make_eof_line());
        assert!(matches!(
            parse_hex(&hex),
            Err(ParseError::ChecksumMismatch { expected, found: 0, .. }) if expected == expected_cs
        ));
    }

    if expected_cs != 0xFF {
        let mut corrupted_ff = valid_line[..valid_line.len() - 2].to_string();
        corrupted_ff.push_str("FF");
        let hex = format!("{}\n{}", corrupted_ff, make_eof_line());
        assert!(matches!(
            parse_hex(&hex),
            Err(ParseError::ChecksumMismatch { expected, found: 0xFF, .. }) if expected == expected_cs
        ));
    }
}

// =========================================================================
// 2. Truncated Records & Byte Length Anomaly
// =========================================================================

#[test]
fn test_adversarial_truncated_records() {
    let valid_line = make_hex_line(0x0000, 0x00, &[0x01, 0x02, 0x03, 0x04]);

    for cut in 0..valid_line.len() - 1 {
        let truncated = &valid_line[..cut];
        if truncated.trim().is_empty() {
            continue;
        }
        let hex = format!("{}\n{}", truncated, make_eof_line());
        let res = parse_hex(&hex);
        assert!(
            res.is_err(),
            "Truncated line at cut {} must error: '{}'",
            cut,
            truncated
        );
    }
}

#[test]
fn test_adversarial_declared_vs_actual_byte_count() {
    let hex = ":100000000102ED\n:00000001FF";
    let err = parse_hex(hex).unwrap_err();
    assert!(matches!(
        err,
        ParseError::RecordTruncated {
            line: 1,
            byte_count: 21,
            actual_bytes: 7
        }
    ));

    let hex2 = ":010000000102FC\n:00000001FF";
    let err2 = parse_hex(hex2).unwrap_err();
    assert!(matches!(
        err2,
        ParseError::RecordTruncated {
            line: 1,
            byte_count: 6,
            actual_bytes: 7
        }
    ));
}

// =========================================================================
// 3. Fuzzed Characters, Framing, and Delimiters
// =========================================================================

#[test]
fn test_adversarial_framing_and_delimiters() {
    let bad_prefixes = [" ", "\t", "0", ";", "#", "/", "!", "-", ":", "::"];
    for prefix in &bad_prefixes {
        let line = format!("{}020000040800F2\n:00000001FF", prefix);
        let res = parse_hex(&line);
        if *prefix == " " || *prefix == "\t" {
            assert!(matches!(res, Err(ParseError::MissingLeadingColon { .. })));
        } else if *prefix == "::" {
            assert!(matches!(
                res,
                Err(ParseError::InvalidHexCharacter { .. })
                    | Err(ParseError::OddHexDigitCount { .. })
            ));
        } else if *prefix != ":" {
            assert!(matches!(res, Err(ParseError::MissingLeadingColon { .. })));
        }
    }
}

#[test]
fn test_adversarial_non_hex_characters() {
    let non_hex = [
        'G', 'g', 'Z', 'z', 'X', 'x', ' ', '\t', '@', '?', '🦀', 'ñ', '中',
    ];
    for ch in non_hex {
        let line = format!(":02000004080{}F2\n:00000001FF", ch);
        let err = parse_hex(&line).expect_err(&format!("Char '{}' should cause error", ch));
        assert!(
            matches!(
                err,
                ParseError::InvalidHexCharacter { character, .. } if character == ch
            ) || matches!(err, ParseError::OddHexDigitCount { .. }),
            "Expected InvalidHexCharacter for '{}', got {:?}",
            ch,
            err
        );
    }
}

#[test]
fn test_adversarial_odd_hex_lengths() {
    let odd_lines = [
        ":1\n:00000001FF",
        ":123\n:00000001FF",
        ":020000040800F\n:00000001FF",
        ":020000040800F21\n:00000001FF",
    ];
    for line in odd_lines {
        let err = parse_hex(line).unwrap_err();
        assert!(matches!(err, ParseError::OddHexDigitCount { line: 1, .. }));
    }
}

#[test]
fn test_adversarial_empty_and_whitespace_only() {
    assert_eq!(parse_hex("").unwrap_err(), ParseError::EmptyFile);
    assert_eq!(parse_hex("   ").unwrap_err(), ParseError::EmptyFile);
    assert_eq!(
        parse_hex("\r\n\t\n  \r\n").unwrap_err(),
        ParseError::EmptyFile
    );
    assert_eq!(
        parse_bin(&[], 0x08000000).unwrap_err(),
        ParseError::EmptyFile
    );
    assert_eq!(
        parse_bytes(&[], None, None).unwrap_err(),
        ParseError::EmptyFile
    );
}

// =========================================================================
// 4. Out-of-Order Records & Bank Switching Permutations
// =========================================================================

#[test]
fn test_adversarial_massive_out_of_order_chunks() {
    let mut lines = Vec::new();

    for bank in (0..4).rev() {
        let ulba = 0x0800 + bank as u16;
        lines.push(make_type04_line(ulba));

        for chunk_idx in (0..32).rev() {
            let offset = (chunk_idx * 16) as u16;
            let data = vec![(bank * 32 + chunk_idx) as u8; 16];
            lines.push(make_hex_line(offset, 0x00, &data));
        }
    }
    lines.push(make_eof_line());

    let hex_content = lines.join("\n");
    let image = parse_hex(&hex_content).expect("Massive out of order must normalize successfully");

    assert_eq!(image.metadata.segment_count, 4);
    assert_eq!(image.metadata.gap_count, 3);
    assert_eq!(image.metadata.total_bytes, 4 * 32 * 16);

    for (bank, seg) in image.segments.iter().enumerate() {
        let expected_start = 0x08000000 + (bank as u32 * 0x00010000);
        assert_eq!(seg.start_address, expected_start);
        assert_eq!(seg.size(), 512);

        for chunk_idx in 0..32 {
            let expected_byte = (bank * 32 + chunk_idx) as u8;
            for b in &seg.data[chunk_idx * 16..(chunk_idx + 1) * 16] {
                assert_eq!(*b, expected_byte);
            }
        }
    }
}

// =========================================================================
// 5. Massive Address Gaps
// =========================================================================

#[test]
fn test_adversarial_100mb_address_gap() {
    let seg1_ulba = 0x0800u16;
    let seg1_off = 0x0000u16;
    let seg2_addr: u32 = 0x08000010 + 100 * 1024 * 1024;
    let seg2_ulba = (seg2_addr >> 16) as u16;
    let seg2_off = (seg2_addr & 0xFFFF) as u16;

    let lines = [
        make_type04_line(seg1_ulba),
        make_hex_line(seg1_off, 0x00, &[0x11; 16]),
        make_type04_line(seg2_ulba),
        make_hex_line(seg2_off, 0x00, &[0x22; 16]),
        make_eof_line(),
    ];

    let hex_content = lines.join("\n");
    let image = parse_hex(&hex_content).expect("100MB gap hex should parse instantly");

    assert_eq!(image.metadata.segment_count, 2);
    assert_eq!(image.metadata.gap_count, 1);
    assert_eq!(image.metadata.memory_gaps[0].size, 100 * 1024 * 1024);
    assert_eq!(image.metadata.gap_bytes, 100 * 1024 * 1024);
    assert_eq!(image.metadata.total_bytes, 32);
    assert_eq!(image.metadata.address_span, (100 * 1024 * 1024 + 32) as u64);

    let padded = compute_padded_checksums(&image.segments, 0xFF);
    assert!(padded.is_some());
}

#[test]
fn test_adversarial_3gb_address_gap_sparse_efficiency() {
    let lines = [
        make_type04_line(0x0000),
        make_hex_line(0x1000, 0x00, &[1, 2, 3, 4]),
        make_type04_line(0xC000),
        make_hex_line(0x1000, 0x00, &[5, 6, 7, 8]),
        make_eof_line(),
    ];

    let hex_content = lines.join("\n");
    let start_time = std::time::Instant::now();
    let image = parse_hex(&hex_content).expect("3GB gap should parse without memory blowup");
    let elapsed = start_time.elapsed();

    assert!(
        elapsed.as_millis() < 50,
        "Parsing 3GB gap took too long: {:?}",
        elapsed
    );
    assert_eq!(image.metadata.gap_count, 1);
    assert_eq!(image.metadata.gap_bytes, 0xC0001000 - 0x00001004);
    assert_eq!(image.metadata.total_bytes, 8);
}

// =========================================================================
// 6. Overlaps, Redundancy, and Collisions
// =========================================================================

#[test]
fn test_adversarial_identical_redundant_overlap_warning() {
    let lines = [
        make_type04_line(0x0800),
        make_hex_line(0x0000, 0x00, &[0xAA; 32]),
        make_hex_line(0x0008, 0x00, &[0xAA; 16]),
        make_hex_line(0x0010, 0x00, &[0xAA; 16]),
        make_eof_line(),
    ];
    let image = parse_hex(&lines.join("\n")).unwrap();
    assert_eq!(image.metadata.segment_count, 1);
    assert_eq!(image.segments[0].size(), 32);
    assert_eq!(image.metadata.warnings.len(), 2);
    for w in &image.metadata.warnings {
        assert!(matches!(w, ValidationWarning::RedundantOverlap { .. }));
    }
}

#[test]
fn test_adversarial_partial_overlap_extension() {
    let lines = [
        make_type04_line(0x0800),
        make_hex_line(0x0000, 0x00, &[1, 2, 3, 4]),
        make_hex_line(0x0002, 0x00, &[3, 4, 5, 6]),
        make_eof_line(),
    ];
    let image = parse_hex(&lines.join("\n")).unwrap();
    assert_eq!(image.metadata.segment_count, 1);
    assert_eq!(image.segments[0].size(), 6);
    assert_eq!(image.segments[0].data, vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(image.metadata.warnings.len(), 1);
}

#[test]
fn test_adversarial_conflicting_overlap_at_boundary() {
    let lines_first = [
        make_type04_line(0x0800),
        make_hex_line(0x0000, 0x00, &[0x11, 0x22]),
        make_hex_line(0x0001, 0x00, &[0xFF, 0x33]),
        make_eof_line(),
    ];
    let err = parse_hex(&lines_first.join("\n")).unwrap_err();
    assert_eq!(
        err,
        ParseError::ConflictingDataOverlap {
            line: 3,
            address: 0x08000001,
            existing: 0x22,
            incoming: 0xFF,
        }
    );
}

// =========================================================================
// 7. Target Bounds Validation Stress
// =========================================================================

#[test]
fn test_adversarial_target_bounds_stress() {
    let seg = MemorySegment::new(0x08000000, vec![0; 1024]);
    let (crc, summary) = compute_checksums(&seg.data);
    let (canon_crc, canon_summary) = compute_canonical_checksums(std::slice::from_ref(&seg));
    assert_eq!(crc, canon_crc);
    assert_eq!(summary, canon_summary);

    let (image, _) = (
        firmware_parser::FirmwareImage {
            metadata: firmware_parser::FirmwareMetadata {
                file_path: None,
                format: FirmwareFormat::RawBinary,
                file_size_bytes: 1024,
                total_bytes: 1024,
                total_firmware_bytes: 1024,
                base_address: 0x08000000,
                highest_address: 0x08000400,
                address_span: 1024,
                gap_count: 0,
                gap_bytes: 0,
                entry_point: None,
                entry_point_source: EntryPointSource::None,
                segment_count: 1,
                segments: vec![],
                memory_gaps: vec![],
                crc32: crc,
                md5: summary.md5.clone(),
                sha256: summary.sha256.clone(),
                checksums: summary,
                warnings: vec![],
            },
            segments: vec![seg],
        },
        (),
    );

    // Exact match bounds
    assert!(validate_target_bounds(&image, 0x08000000, 1024).is_ok());

    // 1 byte too small flash size
    let err1 = validate_target_bounds(&image, 0x08000000, 1023).unwrap_err();
    assert!(matches!(err1, ParseError::TargetOutOfBounds { .. }));

    // Flash starting 1 byte too late
    let err2 = validate_target_bounds(&image, 0x08000001, 1024).unwrap_err();
    assert!(matches!(err2, ParseError::TargetOutOfBounds { .. }));

    // Flash size 0
    let err3 = validate_target_bounds(&image, 0x08000000, 0).unwrap_err();
    assert!(matches!(err3, ParseError::TargetOutOfBounds { .. }));
}

// =========================================================================
// 8. Cortex-M Reset Vector Detection Stress
// =========================================================================

#[test]
fn test_adversarial_cortex_m_heuristics_stress() {
    // Unaligned MSP (0x20000001, 0x20000002, 0x20000003)
    for unaligned in [0x20000001u32, 0x20000002, 0x20000003] {
        let mut data = vec![0u8; 16];
        data[0..4].copy_from_slice(&unaligned.to_le_bytes());
        data[4..8].copy_from_slice(&0x08000009u32.to_le_bytes());
        let seg = MemorySegment::new(0x08000000, data);
        let (ep, src) = detect_cortex_m_reset_vector(&[seg]);
        assert_eq!(ep, None);
        assert_eq!(src, EntryPointSource::None);
    }

    // Zero MSP
    let mut data_zero = vec![0u8; 16];
    data_zero[0..4].copy_from_slice(&0u32.to_le_bytes());
    data_zero[4..8].copy_from_slice(&0x08000009u32.to_le_bytes());
    let seg_zero = MemorySegment::new(0x08000000, data_zero);
    let (ep_z, _) = detect_cortex_m_reset_vector(&[seg_zero]);
    assert_eq!(ep_z, None);

    // Segment shorter than 8 bytes
    let seg_short = MemorySegment::new(0x08000000, vec![0u8; 7]);
    let (ep_s, _) = detect_cortex_m_reset_vector(&[seg_short]);
    assert_eq!(ep_s, None);
}

// =========================================================================
// 9. Direct Chunk Consolidation Edge Cases
// =========================================================================

#[test]
fn test_adversarial_direct_chunk_consolidation() {
    let chunks = vec![
        RawChunk {
            address: 0x1000,
            data: vec![],
            line: 1,
        },
        RawChunk {
            address: 0x1000,
            data: vec![1, 2, 3],
            line: 2,
        },
    ];
    let (segs, _, _) = consolidate_chunks(chunks).unwrap();
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].data, vec![1, 2, 3]);
}

// =========================================================================
// 10. High-Throughput Large Workload Stress
// =========================================================================

#[test]
fn test_adversarial_high_throughput_10k_records() {
    let mut lines = Vec::with_capacity(10002);
    lines.push(make_type04_line(0x0800));

    // 10,000 records of 4 bytes each = 40,000 bytes across two 64KB banks
    for i in 0..10_000 {
        if i == 5000 {
            lines.push(make_type04_line(0x0801));
        }
        let addr = ((i % 5000) * 4) as u16;
        lines.push(make_hex_line(addr, 0x00, &[0xDE, 0xAD, 0xBE, 0xEF]));
    }
    lines.push(make_eof_line());

    let content = lines.join("\n");
    let start = std::time::Instant::now();
    let img = parse_hex(&content).expect("10k records should parse cleanly");
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 500,
        "10k lines parsed in {:?}",
        elapsed
    );
    assert_eq!(img.metadata.total_bytes, 40_000);
}

// =========================================================================
// 11. Fuzz Generator: Robustness Against Random Garbage
// =========================================================================

#[test]
fn test_adversarial_random_garbage_never_panics() {
    let mut state: u64 = 0xCAFE_BABE_1234_5678;
    let mut next_rand = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (state >> 32) as u32
    };

    for _ in 0..5000 {
        let len = (next_rand() % 120) as usize;
        let mut garbage = Vec::with_capacity(len);
        for _ in 0..len {
            garbage.push((next_rand() & 0xFF) as u8);
        }

        let _ = detect_format(&garbage, None);
        let _ = parse_bytes(&garbage, None, None);
        let s = String::from_utf8_lossy(&garbage);
        let _ = parse_hex(&s);
    }
}

// =========================================================================
// 12. BOUNDARY CHALLENGES: Empirical Bug Repositories
// =========================================================================

/// BUG REPRODUCTION 1: Conflicting overlap at 0xFFFFFFFF boundary
/// In `segment.rs`, `MemorySegment::end_address()` saturates to 0xFFFFFFFF,
/// causing `chunk.address == current_end` to evaluate to true.
/// The parser treats the conflicting record as contiguous, appending it without error!
#[test]
fn test_adversarial_conflicting_overlap_at_ffffffff() {
    let lines = [
        make_type04_line(0xFFFF),
        make_hex_line(0xFFFF, 0x00, &[0xAA]),
        make_hex_line(0xFFFF, 0x00, &[0xBB]),
        make_eof_line(),
    ];
    let hex_content = lines.join("\n");
    let res = parse_hex(&hex_content);

    // SPECIFICATION REQUIREMENT: Conflicting data at same physical address must return ConflictingDataOverlap!
    // BUG: Returns Ok(FirmwareImage) with segments[0].data = [0xAA, 0xBB], wrapping past 32-bit address space.
    assert!(
        matches!(
            res,
            Err(ParseError::ConflictingDataOverlap {
                address: 0xFFFFFFFF,
                ..
            })
        ),
        "Conflicting data at 0xFFFFFFFF must be rejected with ConflictingDataOverlap, got: {:?}",
        res
    );
}

/// BUG REPRODUCTION 2: Integer truncation to address 0 in raw binary parser
/// In `bin.rs`, `end_addr_64 as u32` truncates 0x1_0000_0000 to 0x0000_0000.
/// SegmentMetadata.end_address and FirmwareMetadata.highest_address become 0x0000_0000,
/// breaking the invariant `highest_address >= base_address` (highest=0, base=0xFFFFFFF0).
#[test]
fn test_bin_boundary_saturation() {
    let data = vec![0xAA; 16];
    let img = parse_bin(&data, 0xFFFF_FFF0).unwrap();

    // SPECIFICATION REQUIREMENT: highest_address must be >= base_address for non-empty images
    // BUG: highest_address is 0x0000_0000, base_address is 0xFFFFFFF0
    assert!(
        img.metadata.highest_address >= img.metadata.base_address,
        "highest_address (0x{:08X}) must be >= base_address (0x{:08X})",
        img.metadata.highest_address,
        img.metadata.base_address
    );
}

/// BUG REPRODUCTION 3: Address span undercount on 4GB boundary
/// In `hex.rs`, `address_span = (highest_address - base_address) as u64`.
/// When highest_address saturates at 0xFFFFFFFF, span for 16 bytes is 0xFFFFFFFF - 0xFFFFFFF0 = 15.
#[test]
fn test_hex_boundary_4gb_span() {
    let lines = [
        make_type04_line(0xFFFF),
        make_hex_line(0xFFF0, 0x00, &[0xAA; 16]),
        make_eof_line(),
    ];
    let img = parse_hex(&lines.join("\n")).unwrap();

    // SPECIFICATION REQUIREMENT: A single contiguous segment of 16 bytes must have address_span == 16
    // BUG: address_span is 15
    assert_eq!(
        img.metadata.address_span,
        16,
        "address_span should be 16 for a 16-byte contiguous segment, but got {} due to 0xFFFFFFFF saturation",
        img.metadata.address_span
    );
}
