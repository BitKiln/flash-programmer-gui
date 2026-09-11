use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Strongly typed parse errors with line numbers and diagnostic context.
#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error_code", content = "details")]
pub enum ParseError {
    #[error("File is empty")]
    EmptyFile,

    #[error("Invalid ELF file: {reason}")]
    InvalidElf { reason: String },

    #[error("Line {line}: Missing mandatory leading colon prefix")]
    MissingLeadingColon { line: usize },

    #[error("Line {line}: Odd number of hexadecimal characters ({count})")]
    OddHexDigitCount { line: usize, count: usize },

    #[error("Line {line}: Invalid non-hexadecimal character '{character}'")]
    InvalidHexCharacter { line: usize, character: char },

    #[error(
        "Line {line}: Record truncated (expected at least {byte_count} bytes, got {actual_bytes})"
    )]
    RecordTruncated {
        line: usize,
        byte_count: usize,
        actual_bytes: usize,
    },

    #[error("Line {line}: Checksum mismatch (calculated 0x{expected:02X}, found 0x{found:02X})")]
    ChecksumMismatch {
        line: usize,
        expected: u8,
        found: u8,
    },

    #[error("Line {line}: Invalid byte count {actual} for record type 0x{record_type:02X} (expected {expected})")]
    InvalidRecordLength {
        line: usize,
        record_type: u8,
        expected: usize,
        actual: usize,
    },

    #[error("Line {line}: Unknown record type 0x{record_type:02X}")]
    UnknownRecordType { line: usize, record_type: u8 },

    #[error("Line {line}: Conflicting data overlap at physical address 0x{address:08X} (existing 0x{existing:02X}, incoming 0x{incoming:02X})")]
    ConflictingDataOverlap {
        line: usize,
        address: u32,
        existing: u8,
        incoming: u8,
    },

    #[error("Line {line}: Physical address 0x{address:X} overflows 32-bit address space")]
    AddressOverflow { line: usize, address: u64 },

    #[error("Target out of bounds: Segment 0x{segment_start:08X}..0x{segment_end:08X} exceeds target flash limit 0x{flash_limit:08X}")]
    TargetOutOfBounds {
        segment_start: u32,
        segment_end: u32,
        flash_limit: u32,
    },

    #[error("Invalid base address '{0}'")]
    InvalidBaseAddress(String),

    #[error("IO Error: {0}")]
    IoError(String),
}

impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> Self {
        ParseError::IoError(err.to_string())
    }
}
