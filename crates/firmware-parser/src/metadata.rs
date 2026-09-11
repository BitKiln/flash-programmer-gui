use serde::{Deserialize, Serialize};

use crate::error::ParseError;

/// Supported firmware container formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareFormat {
    IntelHex,
    RawBinary,
}

/// Standardized integrity checksum summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecksumSummary {
    /// IEEE 802.3 CRC-32 formatted as uppercase hex (e.g. "0x0A5B1F0D").
    pub crc32: String,
    /// RFC 1321 MD5 formatted as lowercase 32-character hex.
    pub md5: String,
    /// FIPS 180-4 SHA-256 formatted as lowercase 64-character hex.
    pub sha256: String,
}

/// Origin source for detected execution entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryPointSource {
    /// Explicitly declared via Intel HEX Record 05 (HEX386 EIP).
    Record05,
    /// Explicitly declared via Intel HEX Record 03 (HEX86 CS:IP).
    Record03,
    /// Auto-detected via ARM Cortex-M Vector Table (offset 0x04, Reset_Handler).
    CortexMVectorTable,
    /// No entry point detected.
    None,
}

/// Metadata describing a discrete contiguous memory block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentMetadata {
    pub index: usize,
    pub start_address: u32,
    pub end_address: u32,
    pub size_bytes: usize,
    pub checksums: ChecksumSummary,
}

/// Alias for compatibility.
pub type MemorySegmentMetadata = SegmentMetadata;

/// Unallocated address span between two adjacent firmware segments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryGap {
    pub start_address: u32,
    pub end_address: u32,
    pub size: u32,
}

impl MemoryGap {
    pub fn size_bytes(&self) -> u32 {
        self.size
    }
}

/// Non-fatal warnings encountered during parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "details")]
pub enum ValidationWarning {
    RedundantOverlap { address: u32, line: Option<usize> },
    MissingEndOfFileRecord,
    DataAfterEndOfFile { line: usize },
    DeprecatedRecordType { record_type: u8, line: usize },
}

/// Structured metadata summary for UI inspection and CLI reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareMetadata {
    pub file_path: Option<String>,
    pub format: FirmwareFormat,
    pub file_size_bytes: u64,
    pub total_bytes: usize,
    pub total_firmware_bytes: u64,
    pub base_address: u32,
    pub highest_address: u32,
    pub address_span: u64,
    pub gap_count: usize,
    pub gap_bytes: u64,
    pub entry_point: Option<u32>,
    pub entry_point_source: EntryPointSource,
    pub segment_count: usize,
    pub segments: Vec<SegmentMetadata>,
    pub memory_gaps: Vec<MemoryGap>,
    pub crc32: u32,
    pub md5: String,
    pub sha256: String,
    pub checksums: ChecksumSummary,
    pub warnings: Vec<ValidationWarning>,
}

/// Full memory segment containing physical byte payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySegment {
    pub start_address: u32,
    #[serde(skip_serializing)]
    pub data: Vec<u8>,
}

impl MemorySegment {
    pub fn new(start_address: u32, data: Vec<u8>) -> Self {
        Self {
            start_address,
            data,
        }
    }

    /// Returns the exact mathematical exclusive end address in 64-bit space.
    #[inline]
    pub fn end_address_u64(&self) -> u64 {
        (self.start_address as u64) + (self.data.len() as u64)
    }

    /// Alias for `end_address_u64`.
    #[inline]
    pub fn end_address_64(&self) -> u64 {
        self.end_address_u64()
    }

    /// Returns the 32-bit exclusive end address, saturating at `u32::MAX` (0xFFFFFFFF)
    /// if the segment reaches or exceeds the 4GB ceiling.
    #[inline]
    pub fn end_address(&self) -> u32 {
        let end_64 = self.end_address_u64();
        if end_64 >= 0x1_0000_0000 {
            u32::MAX
        } else {
            end_64 as u32
        }
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// In-memory parsed firmware image ready for flashing operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareImage {
    pub metadata: FirmwareMetadata,
    pub segments: Vec<MemorySegment>,
}

/// Validates that all firmware segments reside within target flash memory boundaries.
pub fn validate_target_bounds(
    image: &FirmwareImage,
    flash_start: u32,
    flash_size: u32,
) -> Result<(), ParseError> {
    let flash_start_64 = flash_start as u64;
    let flash_limit_64 = flash_start_64 + (flash_size as u64);
    for seg in &image.segments {
        let seg_start_64 = seg.start_address as u64;
        let seg_end_64 = seg.end_address_u64();
        if seg_start_64 < flash_start_64 || seg_end_64 > flash_limit_64 {
            return Err(ParseError::TargetOutOfBounds {
                segment_start: seg.start_address,
                segment_end: seg.end_address(),
                flash_limit: flash_limit_64.min(u32::MAX as u64) as u32,
            });
        }
    }
    Ok(())
}

/// Inspects Cortex-M vector table to detect Reset Handler entry point.
///
/// Under ARMv7-M, offset 0x00 contains Initial Main Stack Pointer (MSP) and
/// offset 0x04 contains the Reset Handler address. The Thumb execution bit (bit 0)
/// must be set to 1, and the target address must point within a loaded firmware segment.
pub fn detect_cortex_m_reset_vector(segments: &[MemorySegment]) -> (Option<u32>, EntryPointSource) {
    let candidate = segments
        .iter()
        .find(|s| s.start_address == 0x0800_0000 || s.start_address == 0x0000_0000)
        .or_else(|| segments.first());

    if let Some(seg) = candidate {
        if seg.data.len() >= 8 {
            let msp = u32::from_le_bytes([seg.data[0], seg.data[1], seg.data[2], seg.data[3]]);
            let reset_handler =
                u32::from_le_bytes([seg.data[4], seg.data[5], seg.data[6], seg.data[7]]);

            // ARM Thumb mode requires bit 0 = 1 for Cortex-M cores
            if (reset_handler & 1) == 1 {
                let code_addr_64 = (reset_handler & !1) as u64;
                let in_bounds = segments
                    .iter()
                    .any(|s| code_addr_64 >= (s.start_address as u64) && code_addr_64 < s.end_address_u64());
                let msp_valid = msp != 0 && (msp % 4 == 0);

                if in_bounds && msp_valid {
                    return (Some(reset_handler), EntryPointSource::CortexMVectorTable);
                }
            }
        }
    }
    (None, EntryPointSource::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_bounds_validation_success() {
        let seg = MemorySegment::new(0x08000000, vec![0; 1024]);
        let meta = FirmwareMetadata {
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
            crc32: 0,
            md5: String::new(),
            sha256: String::new(),
            checksums: ChecksumSummary {
                crc32: String::new(),
                md5: String::new(),
                sha256: String::new(),
            },
            warnings: vec![],
        };
        let image = FirmwareImage {
            metadata: meta,
            segments: vec![seg],
        };

        assert!(validate_target_bounds(&image, 0x08000000, 64 * 1024).is_ok());
    }

    #[test]
    fn test_target_bounds_below_flash_start() {
        let seg = MemorySegment::new(0x07FFFFFF, vec![0; 16]);
        let meta = FirmwareMetadata {
            file_path: None,
            format: FirmwareFormat::RawBinary,
            file_size_bytes: 16,
            total_bytes: 16,
            total_firmware_bytes: 16,
            base_address: 0x07FFFFFF,
            highest_address: 0x0800000F,
            address_span: 16,
            gap_count: 0,
            gap_bytes: 0,
            entry_point: None,
            entry_point_source: EntryPointSource::None,
            segment_count: 1,
            segments: vec![],
            memory_gaps: vec![],
            crc32: 0,
            md5: String::new(),
            sha256: String::new(),
            checksums: ChecksumSummary {
                crc32: String::new(),
                md5: String::new(),
                sha256: String::new(),
            },
            warnings: vec![],
        };
        let image = FirmwareImage {
            metadata: meta,
            segments: vec![seg],
        };

        let err = validate_target_bounds(&image, 0x08000000, 64 * 1024).unwrap_err();
        assert_eq!(
            err,
            ParseError::TargetOutOfBounds {
                segment_start: 0x07FFFFFF,
                segment_end: 0x0800000F,
                flash_limit: 0x08010000,
            }
        );
    }

    #[test]
    fn test_memory_segment_helpers() {
        let seg = MemorySegment::new(0x08000000, vec![1, 2, 3, 4]);
        assert_eq!(seg.len(), 4);
        assert_eq!(seg.size(), 4);
        assert!(!seg.is_empty());
        assert_eq!(seg.end_address(), 0x08000004);
        assert_eq!(seg.end_address_u64(), 0x08000004);
        assert_eq!(seg.end_address_64(), 0x08000004);

        let bound_seg = MemorySegment::new(0xFFFF_FFFF, vec![0xAA]);
        assert_eq!(bound_seg.end_address_u64(), 0x1_0000_0000);
        assert_eq!(bound_seg.end_address(), 0xFFFF_FFFF);

        let empty_seg = MemorySegment::new(0x08000000, vec![]);
        assert!(empty_seg.is_empty());
    }

    #[test]
    fn test_memory_gap_size_bytes() {
        let gap = MemoryGap {
            start_address: 0x08000020,
            end_address: 0x08040000,
            size: 0x3FFE0,
        };
        assert_eq!(gap.size_bytes(), 0x3FFE0);
    }
}
