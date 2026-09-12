pub mod batch;
pub mod error;
pub mod manager;
pub mod profile;
pub mod progress;
pub mod serial;
pub mod traits;
pub mod types;

#[cfg(feature = "mock-probe")]
pub mod mock;

#[cfg(feature = "live-probe")]
pub mod live;

pub mod unified;

pub use batch::{
    run_batch, run_batch_with, BatchConfig, BatchEvent, BatchObserver, BatchReport,
    RearmPolicy, StopReason, UnitRecord, UnitStatus,
};
pub use error::FlashError;
pub use manager::FlashManager;
pub use progress::{
    ClosureProgressCallback, FlashEvent, FlashStage, LogLevel, NoopProgressCallback,
    ProgressCallback, ProgressMetrics,
};
pub use serial::{program_serial, SerialAllocator, SerialConfig, SerialEncoding};
pub use traits::{FlashBackend, FlashSession};
pub use types::{
    ConnectionConfig, FlashResult, ProbeInfo, ProbeType, ProgramOptions, ResetType, SectorInfo,
    TargetInfo, VerifyMismatch, VerifyReport, WireProtocol,
};

#[cfg(feature = "mock-probe")]
pub use mock::{
    generic_cortex_m, get_target_by_name, stm32f103c8, stm32f103rb, stm32f401re, stm32f411ce,
    FaultInjector, InjectedFault, MockFlashMemory, MockFlashSession, MockProbeBackend,
};

#[cfg(feature = "live-probe")]
pub use live::{ProbeRsLiveBackend, ProbeRsLiveSession};

pub use unified::UnifiedBackend;

pub use profile::{
    delete_profile, list_profiles, load_profile, resolve_profile_path, save_profile, FlashProfile,
    ProfileError, ProfileSummary,
};
