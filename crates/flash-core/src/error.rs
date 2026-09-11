use thiserror::Error;

/// Error taxonomy for all flash programming and probe operations.
#[derive(Debug, Error, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FlashError {
    #[error("Probe not found: {0}")]
    ProbeNotFound(String),

    #[error("Failed to communicate with probe: {0}")]
    ProbeCommunication(String),

    #[error("Connection to probe or target lost: {0}")]
    ConnectionLost(String),

    #[error("Failed to connect to target: {0}")]
    ConnectError(String),

    #[error("Target MCU not supported: {0}")]
    TargetNotSupported(String),

    #[error("Erase operation failed: {0}")]
    EraseError(String),

    #[error("Programming failed: {0}")]
    ProgramError(String),

    #[error("Verification failed: {0}")]
    VerifyError(String),

    #[error("Memory verification mismatch at address 0x{address:08X}: expected 0x{expected:02X}, read 0x{actual:02X}")]
    VerificationMismatch {
        address: u32,
        expected: u8,
        actual: u8,
    },

    #[error("NOR flash write violation at address 0x{address:08X}: cannot transition bit 0 to 1 without erasing (attempted 0x{attempted:02X} over un-erased 0x{current:02X})")]
    NorFlashWriteViolation {
        address: u32,
        attempted: u8,
        current: u8,
    },

    #[error("Verification checksum mismatch: expected CRC 0x{expected:08X}, read 0x{actual:08X}")]
    ChecksumMismatch {
        expected: u32,
        actual: u32,
    },

    #[error("Address out of flash bounds: address 0x{address:08X} exceeds bounds (base 0x{base:08X}, size 0x{size:08X})")]
    AddressOutOfBounds {
        address: u32,
        base: u32,
        size: u32,
    },

    #[error("Invalid memory address: 0x{address:08X} ({reason})")]
    InvalidAddress {
        address: u32,
        reason: String,
    },

    #[error("Target flash memory is write-protected or locked at address 0x{address:08X}")]
    FlashProtected {
        address: u32,
    },

    #[error("Flash operation was cancelled")]
    OperationCancelled,

    #[error("Operation timed out: {0}")]
    Timeout(String),

    #[error("Invalid session state: {0}")]
    InvalidState(String),

    #[error("Injected fault triggered: {0}")]
    FaultInjected(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<std::io::Error> for FlashError {
    fn from(err: std::io::Error) -> Self {
        FlashError::Io(err.to_string())
    }
}
