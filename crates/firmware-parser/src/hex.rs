use crate::checksum::compute_canonical_checksums;
use crate::error::ParseError;
use crate::metadata::{
    detect_cortex_m_reset_vector, EntryPointSource, FirmwareFormat, FirmwareImage,
    FirmwareMetadata, ValidationWarning,
};
use crate::segment::{build_segments_metadata, consolidate_chunks, RawChunk};

/// Parses an Intel HEX formatted string into a consolidated `FirmwareImage`.
pub fn parse_hex(content: &str) -> Result<FirmwareImage, ParseError> {
    if content.trim().is_empty() {
        return Err(ParseError::EmptyFile);
    }

    let mut chunks = Vec::new();
    let mut warnings = Vec::new();
    let mut active_base: u64 = 0;
    let mut seen_eof = false;
    let mut entry_point: Option<u32> = None;
    let mut entry_point_source = EntryPointSource::None;
    let mut non_empty_lines = 0;

    for (line_idx, raw_line) in content.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = raw_line.trim();

        if trimmed.is_empty() {
            continue;
        }
        non_empty_lines += 1;

        if !trimmed.starts_with(':') {
            return Err(ParseError::MissingLeadingColon { line: line_num });
        }

        let hex_part = &trimmed[1..];
        let hex_len = hex_part.len();

        if hex_len % 2 != 0 {
            return Err(ParseError::OddHexDigitCount {
                line: line_num,
                count: hex_len,
            });
        }

        if let Some(bad_char) = hex_part.chars().find(|c| !c.is_ascii_hexdigit()) {
            return Err(ParseError::InvalidHexCharacter {
                line: line_num,
                character: bad_char,
            });
        }

        // Decode hex characters to bytes
        let mut raw_bytes = Vec::with_capacity(hex_len / 2);
        for i in (0..hex_len).step_by(2) {
            let byte_val = u8::from_str_radix(&hex_part[i..i + 2], 16).map_err(|_| {
                ParseError::InvalidHexCharacter {
                    line: line_num,
                    character: hex_part[i..].chars().next().unwrap_or('?'),
                }
            })?;
            raw_bytes.push(byte_val);
        }

        if raw_bytes.len() < 5 {
            return Err(ParseError::RecordTruncated {
                line: line_num,
                byte_count: 5,
                actual_bytes: raw_bytes.len(),
            });
        }

        let declared_byte_count = raw_bytes[0] as usize;
        let expected_total_bytes = 5 + declared_byte_count;

        if raw_bytes.len() != expected_total_bytes {
            return Err(ParseError::RecordTruncated {
                line: line_num,
                byte_count: expected_total_bytes,
                actual_bytes: raw_bytes.len(),
            });
        }

        // Validate two's complement checksum modulo 256: sum of all bytes == 0
        let sum: u8 = raw_bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if sum != 0 {
            let sum_without_cs: u8 = raw_bytes[..raw_bytes.len() - 1]
                .iter()
                .fold(0u8, |acc, &b| acc.wrapping_add(b));
            let expected_cs = (!sum_without_cs).wrapping_add(1);
            let actual_cs = raw_bytes[raw_bytes.len() - 1];
            return Err(ParseError::ChecksumMismatch {
                line: line_num,
                expected: expected_cs,
                found: actual_cs,
            });
        }

        let addr_offset = ((raw_bytes[1] as u16) << 8) | (raw_bytes[2] as u16);
        let record_type = raw_bytes[3];
        let data = &raw_bytes[4..4 + declared_byte_count];

        match record_type {
            0x00 => {
                // Data Record
                let full_addr = active_base + (addr_offset as u64);
                let end_addr_64 = full_addr + (declared_byte_count as u64);

                if end_addr_64 > 0x1_0000_0000 {
                    return Err(ParseError::AddressOverflow {
                        line: line_num,
                        address: end_addr_64,
                    });
                }

                if seen_eof {
                    warnings.push(ValidationWarning::DataAfterEndOfFile { line: line_num });
                }

                chunks.push(RawChunk {
                    address: full_addr as u32,
                    data: data.to_vec(),
                    line: line_num,
                });
            }
            0x01 => {
                // End of File Record
                if declared_byte_count != 0 {
                    return Err(ParseError::InvalidRecordLength {
                        line: line_num,
                        record_type: 0x01,
                        expected: 0,
                        actual: declared_byte_count,
                    });
                }
                seen_eof = true;
            }
            0x02 => {
                // Extended Segment Address Record (HEX86)
                if declared_byte_count != 2 {
                    return Err(ParseError::InvalidRecordLength {
                        line: line_num,
                        record_type: 0x02,
                        expected: 2,
                        actual: declared_byte_count,
                    });
                }
                let usba = ((data[0] as u32) << 8) | (data[1] as u32);
                active_base = (usba as u64) << 4;
            }
            0x03 => {
                // Start Segment Address Record (HEX86)
                if declared_byte_count != 4 {
                    return Err(ParseError::InvalidRecordLength {
                        line: line_num,
                        record_type: 0x03,
                        expected: 4,
                        actual: declared_byte_count,
                    });
                }
                let cs = ((data[0] as u32) << 8) | (data[1] as u32);
                let ip = ((data[2] as u32) << 8) | (data[3] as u32);
                entry_point = Some((cs << 4) + ip);
                entry_point_source = EntryPointSource::Record03;
            }
            0x04 => {
                // Extended Linear Address Record (HEX386)
                if declared_byte_count != 2 {
                    return Err(ParseError::InvalidRecordLength {
                        line: line_num,
                        record_type: 0x04,
                        expected: 2,
                        actual: declared_byte_count,
                    });
                }
                let ulba = ((data[0] as u32) << 8) | (data[1] as u32);
                active_base = (ulba as u64) << 16;
            }
            0x05 => {
                // Start Linear Address Record (HEX386)
                if declared_byte_count != 4 {
                    return Err(ParseError::InvalidRecordLength {
                        line: line_num,
                        record_type: 0x05,
                        expected: 4,
                        actual: declared_byte_count,
                    });
                }
                let eip = ((data[0] as u32) << 24)
                    | ((data[1] as u32) << 16)
                    | ((data[2] as u32) << 8)
                    | (data[3] as u32);
                entry_point = Some(eip);
                entry_point_source = EntryPointSource::Record05;
            }
            other => {
                return Err(ParseError::UnknownRecordType {
                    line: line_num,
                    record_type: other,
                });
            }
        }
    }

    if non_empty_lines == 0 {
        return Err(ParseError::EmptyFile);
    }

    if !seen_eof {
        warnings.push(ValidationWarning::MissingEndOfFileRecord);
    }

    let (segments, gaps, consolidation_warnings) = consolidate_chunks(chunks)?;
    warnings.extend(consolidation_warnings);

    // If no explicit entry point was found, check Cortex-M vector table heuristic
    if entry_point.is_none() {
        let (cortex_ep, cortex_src) = detect_cortex_m_reset_vector(&segments);
        if cortex_ep.is_some() {
            entry_point = cortex_ep;
            entry_point_source = cortex_src;
        }
    }

    let (crc32, checksums) = compute_canonical_checksums(&segments);
    let md5 = checksums.md5.clone();
    let sha256 = checksums.sha256.clone();

    let total_bytes: usize = segments.iter().map(|s| s.data.len()).sum();
    let base_address = segments.first().map(|s| s.start_address).unwrap_or(0);
    let highest_address = segments.last().map(|s| s.end_address()).unwrap_or(0);
    let address_span = if segments.is_empty() {
        0
    } else {
        let base_64 = segments.first().map(|s| s.start_address as u64).unwrap_or(0);
        let end_64 = segments.last().map(|s| s.end_address_u64()).unwrap_or(0);
        end_64.saturating_sub(base_64)
    };
    let gap_count = gaps.len();
    let gap_bytes = gaps.iter().map(|g| g.size as u64).sum();
    let segment_metas = build_segments_metadata(&segments);

    let metadata = FirmwareMetadata {
        file_path: None,
        format: FirmwareFormat::IntelHex,
        file_size_bytes: content.len() as u64,
        total_bytes,
        total_firmware_bytes: total_bytes as u64,
        base_address,
        highest_address,
        address_span,
        gap_count,
        gap_bytes,
        entry_point,
        entry_point_source,
        segment_count: segments.len(),
        segments: segment_metas,
        memory_gaps: gaps,
        crc32,
        md5,
        sha256,
        checksums,
        warnings,
    };

    Ok(FirmwareImage { metadata, segments })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lowercase_hex_parsing() {
        let hex = ":020000040800f2\n:01000000aa55\n:00000001ff";
        let image = parse_hex(hex).unwrap();
        assert_eq!(image.metadata.total_bytes, 1);
        assert_eq!(image.segments[0].data, vec![0xAA]);
    }

    #[test]
    fn test_whitespace_and_blank_lines() {
        let hex = "\n\n  :020000040800F2  \r\n\r\n  :01000000AA55\r\n\n :00000001FF \n";
        let image = parse_hex(hex).unwrap();
        assert_eq!(image.metadata.total_bytes, 1);
    }

    #[test]
    fn test_address_overflow_in_hex() {
        // ULBA = 0xFFFF -> base = 0xFFFF0000. Data offset = 0xFFF8, len = 16 bytes.
        // 0xFFFF0000 + 0xFFF8 + 16 = 0x100000008 (overflows 32-bit space).
        // Checksum for :02000004FFFFFF: 02 + 00 + 00 + 04 + FF + FF = 0x0204. (-4) = 0xFC
        // Checksum for :10FFF800000102030405060708090A0B0C0D0E0F:
        // 10 + FF + F8 + 00 + (sum 0..15 = 120 = 0x78) = 0x0281. (-0x81) = 0x7F
        let hex = ":02000004FFFFFC\n:10FFF800000102030405060708090A0B0C0D0E0F81\n:00000001FF";
        let err = parse_hex(hex).unwrap_err();
        assert_eq!(
            err,
            ParseError::AddressOverflow {
                line: 2,
                address: 0x1_0000_0008,
            }
        );
    }
}
