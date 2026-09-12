//! probe-rs backend.
//!
//! Programs Arm targets through ST-Link, CMSIS-DAP/DAPLink, and J-Link probes
//! over SWD or JTAG, with chip geometry taken from the probe-rs target registry
//! rather than a hardcoded part list.

pub mod backend;
pub mod detect;

pub use backend::{ProbeRsLiveBackend, ProbeRsLiveSession};
