pub mod bin;
pub mod checksum;
pub mod elf;
pub mod error;
pub mod hex;
pub mod metadata;
pub mod segment;

pub use bin::{parse_bin, parse_bin_default, DEFAULT_FLASH_BASE_STM32};
pub use elf::{is_elf, parse_elf, ELF_MAGIC};
pub use checksum::{compute_canonical_checksums, compute_checksums, compute_padded_checksums};
pub use error::ParseError;
pub use hex::parse_hex;
pub use metadata::{
    detect_cortex_m_reset_vector, validate_target_bounds, ChecksumSummary, EntryPointSource,
    FirmwareFormat, FirmwareImage, FirmwareMetadata, MemoryGap, MemorySegment,
    MemorySegmentMetadata, SegmentMetadata, ValidationWarning,
};
pub use segment::{build_segments_metadata, consolidate_chunks, RawChunk};

use std::path::Path;

/// Detects the firmware format based on file extension and magic leading characters.
pub fn detect_format(content: &[u8], path: Option<&Path>) -> FirmwareFormat {
    if let Some(p) = path {
        if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();
            if ext_lower == "hex" || ext_lower == "ihex" {
                return FirmwareFormat::IntelHex;
            }
            if ext_lower == "bin" {
                return FirmwareFormat::RawBinary;
            }
            if ext_lower == "elf" || ext_lower == "axf" || ext_lower == "out" {
                return FirmwareFormat::Elf;
            }
        }
    }

    if crate::elf::is_elf(content) {
        return FirmwareFormat::Elf;
    }

    // Inspect first non-whitespace character
    for &b in content {
        if !b.is_ascii_whitespace() {
            if b == b':' {
                return FirmwareFormat::IntelHex;
            }
            break;
        }
    }

    FirmwareFormat::RawBinary
}

/// Parses firmware from raw bytes with optional format and base address overrides.
pub fn parse_bytes(
    bytes: &[u8],
    format: Option<FirmwareFormat>,
    base_address: Option<u32>,
) -> Result<FirmwareImage, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::EmptyFile);
    }

    let detected = format.unwrap_or_else(|| detect_format(bytes, None));
    match detected {
        FirmwareFormat::IntelHex => {
            let hex_str =
                std::str::from_utf8(bytes).map_err(|_| ParseError::InvalidHexCharacter {
                    line: 1,
                    character: '?',
                })?;
            parse_hex(hex_str)
        }
        FirmwareFormat::Elf => parse_elf(bytes),
        FirmwareFormat::RawBinary => {
            let base = base_address.unwrap_or(DEFAULT_FLASH_BASE_STM32);
            parse_bin(bytes, base)
        }
    }
}

/// Reads and parses a firmware file from disk.
pub fn parse_file<P: AsRef<Path>>(
    path: P,
    base_address: Option<u32>,
) -> Result<FirmwareImage, ParseError> {
    let p = path.as_ref();
    let content = std::fs::read(p)?;
    let format = detect_format(&content, Some(p));

    let mut image = match format {
        FirmwareFormat::IntelHex => {
            let text =
                std::str::from_utf8(&content).map_err(|_| ParseError::InvalidHexCharacter {
                    line: 1,
                    character: '?',
                })?;
            parse_hex(text)?
        }
        FirmwareFormat::Elf => parse_elf(&content)?,
        FirmwareFormat::RawBinary => {
            let base = base_address.unwrap_or(DEFAULT_FLASH_BASE_STM32);
            parse_bin(&content, base)?
        }
    };

    image.metadata.file_path = Some(p.to_string_lossy().to_string());
    image.metadata.file_size_bytes = content.len() as u64;

    Ok(image)
}
