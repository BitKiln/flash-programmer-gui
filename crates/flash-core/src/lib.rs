pub mod batch;
pub mod conformance;
pub mod error;
pub mod history;
pub mod manager;
pub mod profile;
pub mod progress;
pub mod serial;
pub mod traits;
pub mod types;

pub mod registry;

pub use batch::{
    run_batch, run_batch_with, BatchConfig, BatchEvent, BatchObserver, BatchReport,
    RearmPolicy, StopReason, UnitRecord, UnitStatus,
};
pub use error::FlashError;
pub use history::{HistoryRecord, Operation, Outcome};
pub use manager::FlashManager;
pub use progress::{
    ClosureProgressCallback, FlashEvent, FlashStage, LogLevel, NoopProgressCallback,
    ProgressCallback, ProgressMetrics,
};
pub use serial::{program_serial, SerialAllocator, SerialConfig, SerialEncoding};
pub use registry::BackendRegistry;
pub use traits::{FlashBackend, FlashSession};
pub use types::{
    ConnectionConfig, DebugProbeParams, FlashResult, ProbeInfo, ProbeType, ProgramOptions,
    ResetType, SectorInfo, TargetInfo, Transport, VerifyMismatch, VerifyReport, WireProtocol,
};

pub use profile::{
    delete_profile, list_profiles, load_profile, resolve_profile_path, save_profile, FlashProfile,
    ProfileError, ProfileSummary,
};
