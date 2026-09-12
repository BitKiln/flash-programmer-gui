//! Backend conformance suite.
//!
//! "Plugin architecture" is a claim, and this is what makes it testable. Every
//! backend runs the same body of assertions about the trait contract, so a new
//! transport either behaves like the others or fails a test that names exactly
//! how it differs.
//!
//! The harness lives here, in the crate that owns the traits, because
//! `flash-core` must not depend on any backend. Each backend crate calls
//! [`check_backend`] from its own test:
//!
//! ```no_run
//! # use flash_core::{ConnectionConfig, conformance::check_backend};
//! # let backend = unimplemented!() as Box<dyn flash_core::FlashBackend>;
//! let config = ConnectionConfig { /* .. */ ..Default::default() };
//! check_backend(backend.as_ref(), &config).expect("backend conforms");
//! ```
//!
//! It programs and erases whatever it is pointed at, so a backend should run it
//! against a simulated target or a board that is safe to wipe.

use firmware_parser::MemorySegment;

use crate::error::FlashError;
use crate::progress::FlashStage;
use crate::traits::FlashBackend;
use crate::types::{ConnectionConfig, ProgramOptions};

/// A contract the backend broke, phrased so the message alone says what to fix.
#[derive(Debug)]
pub struct ConformanceFailure {
    pub check: &'static str,
    pub detail: String,
}

impl std::fmt::Display for ConformanceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.check, self.detail)
    }
}

impl std::error::Error for ConformanceFailure {}

fn fail<T>(check: &'static str, detail: impl Into<String>) -> Result<T, ConformanceFailure> {
    Err(ConformanceFailure {
        check,
        detail: detail.into(),
    })
}

/// Runs every conformance check against `backend`.
///
/// `config` must open a session against a target that is safe to erase.
pub fn check_backend(
    backend: &dyn FlashBackend,
    config: &ConnectionConfig,
) -> Result<(), ConformanceFailure> {
    check_scheme(backend)?;
    check_probe_listing(backend)?;
    check_session(backend, config)?;
    Ok(())
}

/// The scheme is part of the addressing contract: it appears in saved profiles
/// and in scripts, and the registry splits on it.
fn check_scheme(backend: &dyn FlashBackend) -> Result<(), ConformanceFailure> {
    let scheme = backend.scheme();
    if scheme.is_empty() {
        return fail("scheme", "is empty; the registry cannot route to it");
    }
    if !scheme
        .chars()
        .all(|c| c.is_ascii_lowercase() || c == '-' || c == '_')
    {
        return fail(
            "scheme",
            format!("{scheme:?} must be lowercase ascii, '-' or '_' only"),
        );
    }
    if backend.name().is_empty() {
        return fail("name", "is empty");
    }
    Ok(())
}

/// Finding nothing is an empty list, never an error: the registry concatenates
/// every backend's probes, and one transport's failure must not hide another's.
fn check_probe_listing(backend: &dyn FlashBackend) -> Result<(), ConformanceFailure> {
    let probes = match backend.list_probes() {
        Ok(probes) => probes,
        Err(e) => {
            return fail(
                "list_probes",
                format!("returned an error instead of an empty list: {e}"),
            )
        }
    };
    let scheme = backend.scheme();
    for probe in &probes {
        if !probe.identifier.starts_with(&format!("{scheme}:")) {
            return fail(
                "list_probes",
                format!(
                    "identifier {:?} is not prefixed with {:?}, so the registry cannot route back to this backend",
                    probe.identifier, scheme
                ),
            );
        }
    }
    Ok(())
}

fn check_session(
    backend: &dyn FlashBackend,
    config: &ConnectionConfig,
) -> Result<(), ConformanceFailure> {
    let mut session = match backend.open_session(config) {
        Ok(session) => session,
        Err(e) => return fail("open_session", e.to_string()),
    };

    let Some(target) = session.target_info().cloned() else {
        return fail(
            "target_info",
            "an open session must know what it is attached to",
        );
    };

    // Work in the first sector, or in one page when the geometry is uniform.
    let base = target.flash_base;
    let length = target
        .sectors
        .first()
        .map(|s| s.size)
        .unwrap_or(target.page_size.max(256))
        .min(4096);

    // 1. Erase leaves every bit set.
    if let Err(e) = session.erase_range(base, length, None) {
        return fail("erase_range", e.to_string());
    }
    match session.read_memory(base, length) {
        Ok(data) => {
            if let Some(pos) = data.iter().position(|b| *b != 0xFF) {
                return fail(
                    "erase_range",
                    format!(
                        "byte at 0x{:08X} reads 0x{:02X} after erase, expected 0xFF",
                        base + pos as u32,
                        data[pos]
                    ),
                );
            }
        }
        Err(e) => return fail("read_memory", e.to_string()),
    }

    // 2. What is programmed is what reads back.
    let pattern: Vec<u8> = (0..256u32).map(|i| (i as u8) ^ 0x5A).collect();
    let segment = MemorySegment {
        start_address: base,
        data: pattern.clone(),
    };
    let options = ProgramOptions {
        verify_after: false,
        reset_after: false,
        chip_erase: false,
        ..Default::default()
    };
    if let Err(e) = session.program(std::slice::from_ref(&segment), &options, None) {
        return fail("program", e.to_string());
    }
    match session.read_memory(base, pattern.len() as u32) {
        Ok(data) => {
            if data != pattern {
                let at = data
                    .iter()
                    .zip(&pattern)
                    .position(|(a, b)| a != b)
                    .unwrap_or(0);
                return fail(
                    "program",
                    format!(
                        "read-back differs at 0x{:08X}: got 0x{:02X}, wrote 0x{:02X}",
                        base + at as u32,
                        data.get(at).copied().unwrap_or(0),
                        pattern[at]
                    ),
                );
            }
        }
        Err(e) => return fail("read_memory", e.to_string()),
    }

    // 3. Verify agrees with what is on the target.
    match session.verify(std::slice::from_ref(&segment), None) {
        Ok(report) if report.success => {}
        Ok(report) => {
            return fail(
                "verify",
                format!(
                    "reported failure against the image it just programmed ({} mismatches)",
                    report.mismatches.len()
                ),
            )
        }
        Err(e) => return fail("verify", e.to_string()),
    }

    // 4. Verify notices when the target does not hold the image. A backend that
    //    always returns success is worse than no verify at all.
    let mut wrong = pattern.clone();
    wrong[0] ^= 0xFF;
    let wrong_segment = MemorySegment {
        start_address: base,
        data: wrong,
    };
    match session.verify(std::slice::from_ref(&wrong_segment), None) {
        Ok(report) if !report.success => {}
        Ok(_) => {
            return fail(
                "verify",
                "reported success against an image that differs in its first byte",
            )
        }
        // Reporting the mismatch as an error is acceptable; silently passing is not.
        Err(_) => {}
    }

    // 5. can_interrupt must be answerable for every stage, and a backend that
    //    claims none is making a legitimate statement, not failing.
    for stage in [
        FlashStage::Erasing,
        FlashStage::Programming,
        FlashStage::Verifying,
    ] {
        let _ = session.can_interrupt(stage);
    }

    // 6. close is called on drop and again on an explicit disconnect.
    if let Err(e) = session.close() {
        return fail("close", e.to_string());
    }
    match session.close() {
        Ok(()) => {}
        Err(FlashError::Unsupported(_)) => {}
        Err(e) => {
            return fail(
                "close",
                format!("is not idempotent; the second call failed with: {e}"),
            )
        }
    }

    Ok(())
}
