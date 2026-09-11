use std::time::Instant;

use firmware_parser::FirmwareImage;

use crate::error::FlashError;
use crate::progress::{FlashEvent, FlashStage, ProgressCallback};
use crate::traits::FlashSession;
use crate::types::{FlashResult, ProgramOptions};

/// High-level flash orchestrator coordinating the complete flash execution pipeline.
#[derive(Debug, Default, Clone, Copy)]
pub struct FlashManager;

impl FlashManager {
    /// Creates a new `FlashManager` instance.
    pub fn new() -> Self {
        Self
    }

    /// Executes the full flash lifecycle on an active session:
    /// 1. Validates segments against target flash boundaries (if target info is known)
    /// 2. Erases flash (full chip erase or affected sector ranges)
    /// 3. Programs firmware segments in chunks with progress telemetry
    /// 4. Verifies flash contents against original segments (if `options.verify_after`)
    /// 5. Resets target MCU (if `options.reset_after`)
    pub fn execute_flash(
        session: &mut dyn FlashSession,
        firmware: &FirmwareImage,
        options: &ProgramOptions,
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<FlashResult, FlashError> {
        let start_time = Instant::now();

        if firmware.segments.is_empty() {
            return Err(FlashError::ProgramError(
                "Firmware image contains no memory segments to program".to_string(),
            ));
        }

        // 1. Validate segments against target bounds if target geometry is known
        if let Some(target) = session.target_info() {
            let flash_end = target.flash_base.saturating_add(target.flash_size);
            for seg in &firmware.segments {
                let seg_start = seg.start_address;
                let seg_len = seg.data.len() as u32;
                let seg_end = seg_start.saturating_add(seg_len);

                if seg_start < target.flash_base || seg_end > flash_end {
                    return Err(FlashError::AddressOutOfBounds {
                        address: if seg_start < target.flash_base {
                            seg_start
                        } else {
                            seg_end
                        },
                        base: target.flash_base,
                        size: target.flash_size,
                    });
                }
            }
        }

        // 2. Erase flash. Backends that erase as part of programming do it in
        // step 3; erasing here as well would double every erase cycle.
        if session.program_erases_target() {
            // no-op: `program` below erases what it writes
        } else if options.chip_erase {
            session.erase_all(cb)?;
        } else {
            for seg in &firmware.segments {
                if !seg.data.is_empty() {
                    session.erase_range(seg.start_address, seg.data.len() as u32, cb)?;
                }
            }
        }

        // 3. Program segments
        session.program(&firmware.segments, options, cb)?;

        // 4. Verify if requested
        let verify_report = if options.verify_after {
            let report = session.verify(&firmware.segments, cb)?;
            if !report.success {
                if let Some(mismatch) = report.mismatches.first() {
                    return Err(FlashError::VerificationMismatch {
                        address: mismatch.address,
                        expected: mismatch.expected,
                        actual: mismatch.actual,
                    });
                } else if report.checksum_expected != report.checksum_actual {
                    return Err(FlashError::ChecksumMismatch {
                        expected: report.checksum_expected,
                        actual: report.checksum_actual,
                    });
                } else {
                    return Err(FlashError::VerifyError("Verification failed".to_string()));
                }
            }
            Some(report)
        } else {
            None
        };

        // 5. Reset target if requested
        let mut reset_performed = false;
        if options.reset_after {
            session.reset(false)?;
            reset_performed = true;
        }

        let total_bytes: u32 = firmware.segments.iter().map(|s| s.data.len() as u32).sum();
        let duration_ms = start_time.elapsed().as_millis() as u64;

        let message = format!(
            "Successfully flashed {} bytes in {} ms{}{}",
            total_bytes,
            duration_ms,
            if verify_report.is_some() {
                ", verified"
            } else {
                ""
            },
            if reset_performed {
                ", target reset"
            } else {
                ""
            }
        );

        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageStarted {
                stage: FlashStage::Completed,
                total_bytes: total_bytes as u64,
                message: message.clone(),
            });
            callback.on_event(FlashEvent::StageCompleted {
                stage: FlashStage::Completed,
                duration_ms,
            });
        }

        Ok(FlashResult {
            success: true,
            bytes_flashed: total_bytes,
            duration_ms,
            verify_report,
            reset_performed,
            message,
        })
    }
}
