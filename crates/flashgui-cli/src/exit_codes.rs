use firmware_parser::ParseError;
use flash_core::FlashError;
use std::fmt;

pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_FLASH_VERIFY_ERROR: i32 = 1;
pub const EXIT_TARGET_CONNECTION_ERROR: i32 = 2;
pub const EXIT_FIRMWARE_PARSE_ERROR: i32 = 3;
pub const EXIT_PROBE_NOT_FOUND: i32 = 4;
pub const EXIT_INVALID_ARGS_OR_PROFILE: i32 = 5;

#[derive(Debug)]
pub enum CliError {
    FlashVerify(String),
    TargetConnection(String),
    FirmwareParse(String),
    ProbeNotFound(String),
    InvalidArgsOrProfile(String),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::FlashVerify(_) => EXIT_FLASH_VERIFY_ERROR,
            CliError::TargetConnection(_) => EXIT_TARGET_CONNECTION_ERROR,
            CliError::FirmwareParse(_) => EXIT_FIRMWARE_PARSE_ERROR,
            CliError::ProbeNotFound(_) => EXIT_PROBE_NOT_FOUND,
            CliError::InvalidArgsOrProfile(_) => EXIT_INVALID_ARGS_OR_PROFILE,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            CliError::FlashVerify(msg)
            | CliError::TargetConnection(msg)
            | CliError::FirmwareParse(msg)
            | CliError::ProbeNotFound(msg)
            | CliError::InvalidArgsOrProfile(msg) => msg,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::FlashVerify(msg) => write!(f, "Flash/Verify Error: {}", msg),
            CliError::TargetConnection(msg) => write!(f, "Target Connection Error: {}", msg),
            CliError::FirmwareParse(msg) => write!(f, "Firmware Parse Error: {}", msg),
            CliError::ProbeNotFound(msg) => write!(f, "Probe Not Found: {}", msg),
            CliError::InvalidArgsOrProfile(msg) => write!(f, "Argument/Profile Error: {}", msg),
        }
    }
}

impl std::error::Error for CliError {}

impl From<ParseError> for CliError {
    fn from(err: ParseError) -> Self {
        CliError::FirmwareParse(err.to_string())
    }
}

impl From<flash_core::ProfileError> for CliError {
    fn from(err: flash_core::ProfileError) -> Self {
        CliError::InvalidArgsOrProfile(err.to_string())
    }
}

impl From<FlashError> for CliError {
    fn from(err: FlashError) -> Self {
        match err {
            FlashError::ProbeNotFound(p) => CliError::ProbeNotFound(p),
            FlashError::ConnectError(msg)
            | FlashError::ConnectionLost(msg)
            | FlashError::TargetNotSupported(msg) => CliError::TargetConnection(msg),
            FlashError::ProbeCommunication(msg) => CliError::TargetConnection(msg),
            FlashError::AddressOutOfBounds { .. } | FlashError::InvalidAddress { .. } => {
                CliError::FirmwareParse(err.to_string())
            }
            FlashError::VerificationMismatch { .. }
            | FlashError::ChecksumMismatch { .. }
            | FlashError::VerifyError(_)
            | FlashError::ProgramError(_)
            | FlashError::EraseError(_)
            | FlashError::NorFlashWriteViolation { .. }
            | FlashError::FlashProtected { .. } => CliError::FlashVerify(err.to_string()),
            FlashError::FaultInjected(msg) => {
                if msg.to_lowercase().contains("connect") {
                    CliError::TargetConnection(msg)
                } else {
                    CliError::FlashVerify(msg)
                }
            }
            _ => CliError::FlashVerify(err.to_string()),
        }
    }
}
