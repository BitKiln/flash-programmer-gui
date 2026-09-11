use crate::checksum::compute_checksums;
use crate::error::ParseError;
use crate::metadata::{
    detect_cortex_m_reset_vector, FirmwareFormat, FirmwareImage, FirmwareMetadata, MemorySegment,
};
use crate::segment::build_segments_metadata;

/// Default flash base address for STM32 microcontrollers.
pub const DEFAULT_FLASH_BASE_STM32: u32 = 0x0800_0000;

/// Parses a raw binary byte slice loaded at the specified base address.
pub fn parse_bin(bytes: &[u8], base_address: u32) -> Result<FirmwareImage, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::EmptyFile);
    }

    let end_addr_64 = (base_address as u64) + (bytes.len() as u64);
    if end_addr_64 > 0x1_0000_0000 {
        return Err(ParseError::AddressOverflow {
            line: 0,
            address: end_addr_64,
        });
    }

    let segment = MemorySegment::new(base_address, bytes.to_vec());
    let (crc32, checksums) = compute_checksums(bytes);
    let md5 = checksums.md5.clone();
    let sha256 = checksums.sha256.clone();

    let segments = vec![segment];

    // Detect Cortex-M reset handler from vector table if present
    let (entry_point, entry_point_source) = detect_cortex_m_reset_vector(&segments);

    let segment_metas = build_segments_metadata(&segments);

    let total_bytes = bytes.len();
    let highest_address = segments[0].end_address();

    let metadata = FirmwareMetadata {
        file_path: None,
        format: FirmwareFormat::RawBinary,
        file_size_bytes: total_bytes as u64,
        total_bytes,
        total_firmware_bytes: total_bytes as u64,
        base_address,
        highest_address,
        address_span: total_bytes as u64,
        gap_count: 0,
        gap_bytes: 0,
        entry_point,
        entry_point_source,
        segment_count: 1,
        segments: segment_metas,
        memory_gaps: Vec::new(),
        crc32,
        md5,
        sha256,
        checksums,
        warnings: Vec::new(),
    };

    Ok(FirmwareImage { metadata, segments })
}

/// Convenience helper to parse raw binary using default STM32 flash base address (0x08000000).
pub fn parse_bin_default(bytes: &[u8]) -> Result<FirmwareImage, ParseError> {
    parse_bin(bytes, DEFAULT_FLASH_BASE_STM32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_binary_fails() {
        let err = parse_bin(&[], 0x08000000).unwrap_err();
        assert_eq!(err, ParseError::EmptyFile);
    }

    #[test]
    fn test_address_overflow_fails() {
        let data = vec![0u8; 16];
        let err = parse_bin(&data, 0xFFFF_FFF8).unwrap_err();
        assert_eq!(
            err,
            ParseError::AddressOverflow {
                line: 0,
                address: 0x1_0000_0008
            }
        );
    }

    #[test]
    fn test_parse_bin_default_loads_at_stm32_base() {
        let data = vec![1, 2, 3, 4];
        let img = parse_bin_default(&data).unwrap();
        assert_eq!(img.metadata.base_address, DEFAULT_FLASH_BASE_STM32);
        assert_eq!(img.segments[0].start_address, 0x08000000);
        assert_eq!(img.segments[0].data, data);
    }
}
