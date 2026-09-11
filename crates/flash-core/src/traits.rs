use firmware_parser::MemorySegment;

use crate::error::FlashError;
use crate::progress::ProgressCallback;
use crate::types::{ConnectionConfig, ProbeInfo, ProgramOptions, TargetInfo, VerifyReport};

/// Factory and discovery contract for debug probes.
pub trait FlashBackend: Send + Sync {
    /// Identifier of the backend driver (e.g. "mock-probe", "probe-rs").
    fn name(&self) -> &'static str;

    /// Discover and list all currently accessible probes.
    fn list_probes(&self) -> Result<Vec<ProbeInfo>, FlashError>;

    /// Establish an active connection to target MCU via specified probe and configuration.
    fn open_session(&self, config: &ConnectionConfig) -> Result<Box<dyn FlashSession>, FlashError>;

    /// Automatically discover target MCU without requiring prior manual chip selection.
    fn detect_target(&self, config: &ConnectionConfig) -> Result<TargetInfo, FlashError> {
        let mut auto_config = config.clone();
        if auto_config.target_name.trim().is_empty() {
            auto_config.target_name = "auto".to_string();
        }
        let session = self.open_session(&auto_config)?;
        session
            .target_info()
            .cloned()
            .ok_or_else(|| FlashError::ConnectError("Could not determine target info".to_string()))
    }
}

/// Active connection session with an MCU target providing flash and memory control.
pub trait FlashSession: Send {
    /// Returns target MCU geometry and capabilities if known.
    fn target_info(&self) -> Option<&TargetInfo> {
        None
    }

    /// Mass-erase the entire target flash memory.
    fn erase_all(&mut self, cb: Option<&dyn ProgressCallback>) -> Result<(), FlashError>;

    /// Erase flash sectors spanning from `start` for `length` bytes.
    fn erase_range(
        &mut self,
        start: u32,
        length: u32,
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<(), FlashError>;

    /// Program memory segments into target flash.
    fn program(
        &mut self,
        segments: &[MemorySegment],
        options: &ProgramOptions,
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<(), FlashError>;

    /// Verify target flash contents against provided segments.
    fn verify(
        &mut self,
        segments: &[MemorySegment],
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<VerifyReport, FlashError>;

    /// Read raw bytes from target memory at `address`.
    fn read_memory(&mut self, address: u32, length: u32) -> Result<Vec<u8>, FlashError>;

    /// Trigger target system reset. If `halt` is true, pause core at entry.
    fn reset(&mut self, halt: bool) -> Result<(), FlashError>;

    /// Gracefully terminate session and release probe hardware.
    fn close(&mut self) -> Result<(), FlashError>;
}
