use firmware_parser::MemorySegment;

use crate::error::FlashError;
use crate::progress::{FlashStage, ProgressCallback};
use crate::types::{ConnectionConfig, ProbeInfo, ProgramOptions, TargetInfo, VerifyReport};

/// Factory and discovery contract for debug probes.
pub trait FlashBackend: Send + Sync {
    /// Identifier of the backend driver (e.g. "mock-probe", "probe-rs").
    fn name(&self) -> &'static str;

    /// Scheme this backend claims on a probe identifier, without the colon.
    ///
    /// [`BackendRegistry`](crate::registry::BackendRegistry) routes `mock:…` to
    /// the backend whose scheme is `mock`. Keep it short, lowercase, and stable:
    /// it appears in saved profiles and in scripts.
    fn scheme(&self) -> &'static str;

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

    /// Whether a cancellation request can take effect part way through `stage`.
    ///
    /// Cancellation is cooperative: a backend can only honour it where it polls
    /// between units of work. Backends that hand a whole stage to a driver in
    /// one call cannot, and callers must not offer a Stop that would do
    /// nothing. Defaults to true for backends that poll throughout.
    fn can_interrupt(&self, stage: FlashStage) -> bool {
        matches!(
            stage,
            FlashStage::Erasing | FlashStage::Programming | FlashStage::Verifying
        )
    }

    /// Whether `program` already erases the flash it writes.
    ///
    /// Backends whose programming path performs its own erase (probe-rs's flash
    /// loader does) return `true` so the manager skips the separate erase pass
    /// instead of erasing the same sectors twice.
    fn program_erases_target(&self) -> bool {
        false
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

    /// Whether this session can write target memory directly.
    ///
    /// Defaults to false: a transport that only speaks to a flash controller
    /// has no bus access, and a caller must not offer an edit that cannot land.
    fn can_write_memory(&self) -> bool {
        false
    }

    /// Write raw bytes to target memory at `address`.
    ///
    /// This is a direct bus write, not a flash program: nothing is erased
    /// first. It is for RAM, peripheral registers, and memory-mapped
    /// configuration such as option bytes. Writing into the flash region must
    /// be refused rather than attempted -- without an erase such a write either
    /// does nothing or leaves a half-written sector, and both look like success
    /// from here. Use [`program`](FlashSession::program) for flash.
    fn write_memory(&mut self, address: u32, _data: &[u8]) -> Result<(), FlashError> {
        let _ = address;
        Err(FlashError::Unsupported(
            "this backend cannot write target memory directly".to_string(),
        ))
    }

    /// Trigger target system reset. If `halt` is true, pause core at entry.
    fn reset(&mut self, halt: bool) -> Result<(), FlashError>;

    /// Gracefully terminate session and release probe hardware.
    fn close(&mut self) -> Result<(), FlashError>;
}
