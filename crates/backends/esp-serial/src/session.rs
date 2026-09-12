//! `FlashSession` over an ESP ROM bootloader.
//!
//! Addressing is the thing to keep straight. On an ESP part, flash is written
//! by **offset** — offset 0 is the start of flash — while the running CPU sees
//! that same flash mapped somewhere else entirely (0x3C00_0000 and friends,
//! differing by chip). Everything here, and everything the CLI and GUI show for
//! an ESP target, is an offset. `TargetInfo::flash_base` is therefore 0, which
//! also means `--base-address` for a raw `.bin` is an offset: 0x10000 for a
//! typical ESP-IDF application image.

use std::time::Instant;

use firmware_parser::MemorySegment;
use flash_core::error::FlashError;
use flash_core::progress::{
    FlashEvent, FlashStage, LogLevel, ProgressCallback, ProgressMetrics,
};
use flash_core::traits::FlashSession;
use flash_core::types::{ProgramOptions, SectorInfo, TargetInfo, VerifyMismatch, VerifyReport};
use md5::{Digest, Md5};

use crate::link::{erase_span, EspLink, SECTOR_SIZE};

/// How much is read back at a time when a verify falls back to comparing bytes.
const READ_CHUNK: u32 = 32 * 1024;

pub struct EspSession {
    link: Box<dyn EspLink>,
    target: TargetInfo,
    closed: bool,
}

impl std::fmt::Debug for EspSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EspSession")
            .field("target", &self.target.name)
            .field("closed", &self.closed)
            .finish()
    }
}

impl EspSession {
    /// Opens a session over an already-connected link.
    pub fn new(mut link: Box<dyn EspLink>) -> Result<Self, FlashError> {
        let info = link.device_info()?;
        let flash_size = if info.flash_size == 0 {
            // The chip would not say. Assume the devkit-common 4 MB rather than
            // refusing to connect, and let a write past the end fail honestly.
            4 * 1024 * 1024
        } else {
            info.flash_size
        };

        let sectors = (0..flash_size / SECTOR_SIZE)
            .map(|index| SectorInfo {
                index,
                address: index * SECTOR_SIZE,
                size: SECTOR_SIZE,
            })
            .collect();

        let target = TargetInfo {
            name: info.chip.clone(),
            display_name: match &info.revision {
                Some(rev) => Some(format!("{} ({rev})", info.chip)),
                None => Some(info.chip.clone()),
            },
            architecture: esp_architecture(&info.chip).to_string(),
            // Offsets, not the memory-mapped view. See the module comment.
            flash_base: 0,
            flash_size,
            // The ROM bootloader gives no access to RAM, so there is nothing
            // honest to report here.
            ram_base: 0,
            ram_size: 0,
            page_size: SECTOR_SIZE,
            sectors,
        };

        Ok(Self {
            link,
            target,
            closed: false,
        })
    }

    fn emit(&self, cb: Option<&dyn ProgressCallback>, event: FlashEvent) {
        if let Some(cb) = cb {
            cb.on_event(event);
        }
    }

    fn log(&self, cb: Option<&dyn ProgressCallback>, level: LogLevel, message: impl Into<String>) {
        self.emit(
            cb,
            FlashEvent::Log {
                level,
                message: message.into(),
                timestamp_ms: now_ms(),
            },
        );
    }

    fn cancelled(&self, cb: Option<&dyn ProgressCallback>) -> bool {
        cb.map(|cb| cb.is_cancelled()).unwrap_or(false)
    }

    fn check_range(&self, offset: u32, length: u32) -> Result<(), FlashError> {
        let end = offset
            .checked_add(length)
            .ok_or(FlashError::InvalidAddress {
                address: offset,
                reason: "offset + length overflows".to_string(),
            })?;
        if end > self.target.flash_size {
            return Err(FlashError::AddressOutOfBounds {
                address: offset,
                base: 0,
                size: self.target.flash_size,
            });
        }
        Ok(())
    }
}

impl FlashSession for EspSession {
    fn target_info(&self) -> Option<&TargetInfo> {
        Some(&self.target)
    }

    /// Programming and verification are driven chunk by chunk here, so Stop
    /// genuinely lands between chunks. A full chip erase is a single bootloader
    /// command with no poll point, so it is reported as uninterruptible rather
    /// than offering a button that would do nothing.
    fn can_interrupt(&self, stage: FlashStage) -> bool {
        // Programming is one bootloader transaction that ends in a reboot;
        // stopping partway would leave the chip in a state neither we nor the
        // operator could describe. Only verification polls often enough to
        // stop cleanly.
        matches!(stage, FlashStage::Verifying)
    }

    fn erase_all(&mut self, cb: Option<&dyn ProgressCallback>) -> Result<(), FlashError> {
        let started = Instant::now();
        self.emit(
            cb,
            FlashEvent::StageStarted {
                stage: FlashStage::Erasing,
                total_bytes: u64::from(self.target.flash_size),
                message: format!("Erasing all {} KB of flash", self.target.flash_size / 1024),
            },
        );
        self.log(
            cb,
            LogLevel::Warn,
            "A full chip erase runs to completion inside one bootloader command and cannot be interrupted",
        );
        self.link.erase_all()?;
        self.emit(
            cb,
            FlashEvent::StageCompleted {
                stage: FlashStage::Erasing,
                duration_ms: started.elapsed().as_millis() as u64,
            },
        );
        Ok(())
    }

    fn erase_range(
        &mut self,
        start: u32,
        length: u32,
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<(), FlashError> {
        // Erasing is sector-granular, so a region that ends mid-sector still
        // erases to the end of that sector.
        let (start, length) = erase_span(start, length)?;
        self.check_range(start, length)?;

        let started = Instant::now();
        self.emit(
            cb,
            FlashEvent::StageStarted {
                stage: FlashStage::Erasing,
                total_bytes: u64::from(length),
                message: format!("Erasing 0x{start:X}..0x{:X}", start + length),
            },
        );
        self.link.erase_region(start, length)?;
        self.emit(
            cb,
            FlashEvent::StageCompleted {
                stage: FlashStage::Erasing,
                duration_ms: started.elapsed().as_millis() as u64,
            },
        );
        Ok(())
    }

    fn program(
        &mut self,
        segments: &[MemorySegment],
        _options: &ProgramOptions,
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<(), FlashError> {
        let total: u64 = segments.iter().map(|s| s.data.len() as u64).sum();
        let started = Instant::now();

        self.emit(
            cb,
            FlashEvent::StageStarted {
                stage: FlashStage::Programming,
                total_bytes: total,
                message: format!("Writing {total} bytes over the serial bootloader"),
            },
        );

        let mut written: u64 = 0;
        for segment in segments {
            self.check_range(segment.start_address, segment.data.len() as u32)?;

            if self.cancelled(cb) {
                return Err(FlashError::OperationCancelled);
            }

            let offset = segment.start_address;
            let done_before = written;
            let mut report = |bytes: usize| {
                let so_far = done_before + bytes as u64;
                if let Some(cb) = cb {
                    cb.on_event(FlashEvent::Progress(ProgressMetrics::new(
                        FlashStage::Programming,
                        so_far,
                        total,
                        started.elapsed().as_millis() as u64,
                        offset,
                        format!("Wrote 0x{offset:X}"),
                    )));
                }
            };
            self.link.write(offset, &segment.data, &mut report)?;
            written += segment.data.len() as u64;

            // The write ended by rebooting the chip out of download mode, so
            // bring the bootloader back up before anything else speaks to it.
            self.link.resync()?;
        }

        self.emit(
            cb,
            FlashEvent::StageCompleted {
                stage: FlashStage::Programming,
                duration_ms: started.elapsed().as_millis() as u64,
            },
        );
        Ok(())
    }

    /// Verifies with the chip's own MD5 where it can, which avoids pulling the
    /// whole image back over a serial link. Only when a segment's digest
    /// disagrees does it read the bytes back, and then only to say *where* the
    /// difference is — a report that says "it differs" and nothing else is not
    /// worth much when you are staring at a board.
    fn verify(
        &mut self,
        segments: &[MemorySegment],
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<VerifyReport, FlashError> {
        let total: u64 = segments.iter().map(|s| s.data.len() as u64).sum();
        let started = Instant::now();

        self.emit(
            cb,
            FlashEvent::StageStarted {
                stage: FlashStage::Verifying,
                total_bytes: total,
                message: "Verifying with the chip's MD5".to_string(),
            },
        );

        let mut mismatches = Vec::new();
        let mut verified: u64 = 0;
        let mut expected_crc = crc32fast::Hasher::new();
        let mut actual_crc = crc32fast::Hasher::new();

        for segment in segments {
            self.check_range(segment.start_address, segment.data.len() as u32)?;
            if self.cancelled(cb) {
                return Err(FlashError::OperationCancelled);
            }

            let length = segment.data.len() as u32;
            let mut hasher = Md5::new();
            hasher.update(&segment.data);
            let expected: [u8; 16] = hasher.finalize().into();
            let actual = self.link.md5(segment.start_address, length)?;

            expected_crc.update(&segment.data);

            if expected == actual {
                actual_crc.update(&segment.data);
            } else {
                // Digests differ; find out where, in bounded reads.
                let mut offset = 0u32;
                while offset < length {
                    if self.cancelled(cb) {
                        return Err(FlashError::OperationCancelled);
                    }
                    let take = READ_CHUNK.min(length - offset);
                    let on_chip = self.link.read(segment.start_address + offset, take)?;
                    actual_crc.update(&on_chip);
                    for (i, actual_byte) in on_chip.iter().enumerate() {
                        let want = segment.data[(offset as usize) + i];
                        if *actual_byte != want && mismatches.len() < 32 {
                            mismatches.push(VerifyMismatch {
                                address: segment.start_address + offset + i as u32,
                                expected: want,
                                actual: *actual_byte,
                            });
                        }
                    }
                    offset += take;
                }
            }

            verified += u64::from(length);
            self.emit(
                cb,
                FlashEvent::Progress(ProgressMetrics::new(
                    FlashStage::Verifying,
                    verified,
                    total,
                    started.elapsed().as_millis() as u64,
                    segment.start_address,
                    format!("Verified 0x{:X}", segment.start_address),
                )),
            );
        }

        let report = VerifyReport {
            success: mismatches.is_empty(),
            bytes_verified: verified as u32,
            mismatches,
            checksum_expected: expected_crc.finalize(),
            checksum_actual: actual_crc.finalize(),
        };

        self.emit(
            cb,
            FlashEvent::StageCompleted {
                stage: FlashStage::Verifying,
                duration_ms: started.elapsed().as_millis() as u64,
            },
        );
        Ok(report)
    }

    /// Reads flash by offset. RAM is not reachable: the ROM bootloader exposes
    /// flash commands, not a debug interface into the running core.
    fn read_memory(&mut self, address: u32, length: u32) -> Result<Vec<u8>, FlashError> {
        self.check_range(address, length)?;
        self.link.read(address, length)
    }

    fn reset(&mut self, halt: bool) -> Result<(), FlashError> {
        if halt {
            return Err(FlashError::Unsupported(
                "the ESP serial bootloader can reset the chip but cannot halt its core: \
                 halting needs a debug probe over JTAG. Reset without halt, or connect a probe."
                    .to_string(),
            ));
        }
        self.link.reset()
    }

    fn close(&mut self) -> Result<(), FlashError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.link.close()
    }
}

/// Core architecture per chip family, for display.
fn esp_architecture(chip: &str) -> &'static str {
    let chip = chip.to_lowercase();
    // The original ESP32 and the S-series are Xtensa; everything else Espressif
    // has shipped since is RISC-V.
    if chip.starts_with("esp32s") || chip == "esp32" {
        "Xtensa LX"
    } else if chip.starts_with("esp32") {
        "RISC-V"
    } else {
        "unknown"
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeLink;

    fn session() -> EspSession {
        EspSession::new(Box::new(FakeLink::new("esp32s3"))).unwrap()
    }

    #[test]
    fn flash_is_addressed_by_offset_not_by_the_memory_mapped_view() {
        let s = session();
        let target = s.target_info().unwrap();
        assert_eq!(target.flash_base, 0);
        assert_eq!(target.flash_size, 4 * 1024 * 1024);
        assert_eq!(target.architecture, "Xtensa LX");
    }

    #[test]
    fn riscv_parts_are_labelled_as_such() {
        let s = EspSession::new(Box::new(FakeLink::new("esp32c6"))).unwrap();
        assert_eq!(s.target_info().unwrap().architecture, "RISC-V");
    }

    #[test]
    fn a_full_erase_does_not_pretend_to_be_interruptible() {
        let s = session();
        assert!(!s.can_interrupt(FlashStage::Erasing));
        assert!(!s.can_interrupt(FlashStage::Programming));
        assert!(s.can_interrupt(FlashStage::Verifying));
    }

    #[test]
    fn an_erase_starting_mid_sector_is_refused() {
        // Erasing from here would take the bytes in front of it as well.
        let mut s = session();
        let err = s.erase_range(0x800, 0x1000, None).unwrap_err();
        assert!(matches!(err, FlashError::InvalidAddress { .. }));
    }

    #[test]
    fn an_erase_ending_mid_sector_covers_that_whole_sector() {
        // A sector is the smallest thing the chip can erase, and a real
        // firmware image is never an exact multiple of one. Refusing the
        // length would make such an image unflashable.
        let mut s = session();
        s.erase_range(0x1000, 0x800, None).unwrap();

        let erased = s.read_memory(0x1000, 0x1000).unwrap();
        assert!(
            erased.iter().all(|b| *b == 0xFF),
            "the sector the region ends in must be erased in full"
        );
    }

    #[test]
    fn writing_past_the_end_of_flash_is_refused() {
        let mut s = session();
        let segment = MemorySegment {
            start_address: 4 * 1024 * 1024 - 8,
            data: vec![0u8; 16],
        };
        let err = s
            .program(&[segment], &ProgramOptions::default(), None)
            .unwrap_err();
        assert!(matches!(err, FlashError::AddressOutOfBounds { .. }));
    }

    #[test]
    fn program_then_verify_round_trips() {
        let mut s = session();
        let data: Vec<u8> = (0..1024u32).map(|i| (i % 251) as u8).collect();
        let segment = MemorySegment {
            start_address: 0x10000,
            data: data.clone(),
        };

        s.erase_range(0x10000, SECTOR_SIZE, None).unwrap();
        s.program(std::slice::from_ref(&segment), &ProgramOptions::default(), None)
            .unwrap();

        assert_eq!(s.read_memory(0x10000, 1024).unwrap(), data);
        assert!(s.verify(&[segment], None).unwrap().success);
    }

    #[test]
    fn verify_reports_where_the_image_differs() {
        let mut s = session();
        let mut data = vec![0x00u8; 512];
        s.erase_range(0, SECTOR_SIZE, None).unwrap();
        s.program(
            &[MemorySegment {
                start_address: 0,
                data: data.clone(),
            }],
            &ProgramOptions::default(),
            None,
        )
        .unwrap();

        // Ask it to verify an image the chip does not hold.
        data[100] = 0x5A;
        let report = s
            .verify(
                &[MemorySegment {
                    start_address: 0,
                    data,
                }],
                None,
            )
            .unwrap();

        assert!(!report.success);
        assert_eq!(report.mismatches.len(), 1);
        assert_eq!(report.mismatches[0].address, 100);
        assert_eq!(report.mismatches[0].expected, 0x5A);
        assert_eq!(report.mismatches[0].actual, 0x00);
    }

    #[test]
    fn halting_on_reset_says_why_it_cannot() {
        let mut s = session();
        let err = s.reset(true).unwrap_err();
        assert!(matches!(err, FlashError::Unsupported(_)));
        assert!(err.to_string().contains("JTAG"));
        s.reset(false).expect("a plain reset works");
    }

    #[test]
    fn close_is_idempotent() {
        let mut s = session();
        s.close().unwrap();
        s.close().unwrap();
    }
}
