//! One open connection to an OpenOCD process, as a [`FlashSession`].
//!
//! OpenOCD is a program rather than a library, so everything here is a command
//! string and a reply to read. Two consequences shape the whole file:
//!
//! 1. **OpenOCD reports failure as prose on the same channel as success.** A
//!    command that returns without an I/O error may still have done nothing, so
//!    every reply is inspected.
//! 2. **File-based commands are executed by OpenOCD, not by us.** `flash
//!    write_image` opens the path itself, so a path written here means nothing
//!    to an OpenOCD on another machine. Those operations are refused against a
//!    non-local endpoint rather than attempted.

use std::io::Write as _;
use std::path::PathBuf;

use firmware_parser::MemorySegment;
use flash_core::error::FlashError;
use flash_core::progress::{FlashEvent, FlashStage, ProgressCallback, ProgressMetrics};
use flash_core::traits::FlashSession;
use flash_core::types::{ProgramOptions, SectorInfo, TargetInfo, VerifyMismatch, VerifyReport};

use crate::geometry::{
    current_target_is_halted, parse_current_target, parse_flash_info, parse_flash_list, parse_mdb,
};
use crate::tcl::{reply_is_error, TclLink};

/// Largest direct memory write sent byte by byte.
///
/// Beyond this a file and `load_image` is the sane route, and that needs a
/// local OpenOCD. A byte-at-a-time write of a kilobyte is thousands of round
/// trips.
const MAX_BYTEWISE_WRITE: usize = 256;

/// Assumed erase granularity when OpenOCD will not describe the sectors.
///
/// Only used for the alignment message; the erase itself goes through
/// `flash erase_address`, which applies the driver's real granularity.
const ASSUMED_SECTOR: u32 = 4096;

pub struct OpenOcdSession {
    link: Box<dyn TclLink>,
    target: TargetInfo,
    /// Flash bank indices, in the order `flash list` reported them.
    banks: Vec<usize>,
    closed: bool,
}

impl OpenOcdSession {
    /// Interrogates a connected OpenOCD and builds the session.
    pub fn new(mut link: Box<dyn TclLink>, requested_target: &str) -> Result<Self, FlashError> {
        let targets = link.command("targets")?;
        if reply_is_error(&targets) {
            return Err(FlashError::ConnectError(format!(
                "OpenOCD would not list its targets: {targets}"
            )));
        }
        let target_name = parse_current_target(&targets).ok_or_else(|| {
            FlashError::ConnectError(format!(
                "OpenOCD has no current target. Its configuration has to name one \
                 (for example with a board file) before anything can be programmed. \
                 It answered:\n{targets}"
            ))
        })?;

        // Programming needs a halted core. Halting here rather than failing
        // later: OpenOCD's own failure for this arrives part way through a
        // write, which is a worse place to find out.
        if !current_target_is_halted(&targets) {
            let halted = link.command("halt")?;
            if reply_is_error(&halted) {
                return Err(FlashError::ConnectError(format!(
                    "the target could not be halted, and flash cannot be written \
                     while it runs: {halted}"
                )));
            }
        }

        let banks = parse_flash_list(&link.command("flash list")?);
        if banks.is_empty() {
            return Err(FlashError::ConnectError(
                "OpenOCD reports no flash banks. Its configuration has to declare \
                 one (a board or target file usually does) before flash can be \
                 erased, written or verified."
                    .to_string(),
            ));
        }

        // Probing is what teaches most drivers the real size and sector
        // layout; an unprobed bank reports a size of zero.
        let mut sectors: Vec<SectorInfo> = Vec::new();
        let mut base = u32::MAX;
        let mut end = 0u32;
        for bank in &banks {
            let _ = link.command(&format!("flash probe {}", bank.index));
            let info = link.command(&format!("capture {{flash info {}}}", bank.index))?;
            let (bank_base, bank_size, bank_sectors) = parse_flash_info(&info);

            let bank_base = if bank_base == 0 { bank.base } else { bank_base };
            let bank_size = if bank_size == 0 { bank.size } else { bank_size };
            if bank_size == 0 {
                continue;
            }

            base = base.min(bank_base);
            end = end.max(bank_base.saturating_add(bank_size));
            sectors.extend(bank_sectors);
        }

        if base == u32::MAX {
            return Err(FlashError::ConnectError(
                "OpenOCD could not say how big the flash is. The bank is declared \
                 but unprobed, which usually means the adapter is not talking to \
                 the chip."
                    .to_string(),
            ));
        }

        sectors.sort_by_key(|s| s.address);
        for (index, sector) in sectors.iter_mut().enumerate() {
            // Re-index across banks: OpenOCD numbers sectors per bank, and two
            // banks would otherwise both start at zero.
            sector.index = index as u32;
        }

        // OpenOCD does not describe RAM, but a target's work area is in it and
        // is the one region the configuration does declare.
        let (ram_base, ram_size) = work_area(link.as_mut(), &target_name);

        let page_size = sectors
            .iter()
            .map(|s| s.size)
            .min()
            .unwrap_or(ASSUMED_SECTOR);

        let name = if requested_target.trim().is_empty()
            || requested_target.eq_ignore_ascii_case("auto")
        {
            target_name.clone()
        } else {
            requested_target.to_string()
        };

        let target = TargetInfo {
            name,
            // What OpenOCD is attached to, which is more specific than the
            // name a caller asked to connect with.
            display_name: Some(format!("{target_name} via OpenOCD")),
            architecture: architecture_of(&targets, &target_name),
            flash_base: base,
            flash_size: end.saturating_sub(base),
            ram_base,
            ram_size,
            page_size,
            sectors,
        };

        Ok(Self {
            link,
            target,
            banks: banks.iter().map(|b| b.index).collect(),
            closed: false,
        })
    }

    /// Reports progress, when anyone is listening.
    fn report(
        &self,
        cb: Option<&dyn ProgressCallback>,
        stage: FlashStage,
        done: u64,
        total: u64,
        address: u32,
        message: impl Into<String>,
    ) {
        if let Some(cb) = cb {
            cb.on_event(FlashEvent::Progress(ProgressMetrics::new(
                stage,
                done,
                total,
                0,
                address,
                message.into(),
            )));
        }
    }

    fn cancelled(&self, cb: Option<&dyn ProgressCallback>) -> bool {
        cb.map(|cb| cb.is_cancelled()).unwrap_or(false)
    }

    /// Runs `command`, turning a failure reply into `error`.
    fn run(
        &mut self,
        command: &str,
        error: impl Fn(String) -> FlashError,
    ) -> Result<String, FlashError> {
        let reply = self.link.command(command)?;
        if reply_is_error(&reply) {
            return Err(error(reply));
        }
        Ok(reply)
    }

    fn check_open(&self) -> Result<(), FlashError> {
        if self.closed {
            return Err(FlashError::InvalidState(
                "the OpenOCD connection is closed".to_string(),
            ));
        }
        Ok(())
    }

    /// Refuses an operation that needs OpenOCD to open a file we wrote.
    fn require_local(&self, what: &str) -> Result<(), FlashError> {
        if self.link.is_local() {
            return Ok(());
        }
        Err(FlashError::Unsupported(format!(
            "{what} goes through a file that OpenOCD opens itself, so it only \
             works when OpenOCD runs on this machine. This connection is to {}. \
             Run OpenOCD locally, or use a debug probe directly.",
            self.link.endpoint()
        )))
    }

    fn check_range(&self, start: u32, length: u32) -> Result<(), FlashError> {
        if length == 0 {
            return Ok(());
        }
        let end = start
            .checked_add(length)
            .ok_or(FlashError::InvalidAddress {
                address: start,
                reason: "the range overflows the 32-bit address space".to_string(),
            })?;
        if start < self.target.flash_base || end > self.target.flash_end() {
            return Err(FlashError::AddressOutOfBounds {
                address: start,
                base: self.target.flash_base,
                size: self.target.flash_size,
            });
        }
        Ok(())
    }

    /// A scratch file holding `data`, for a command that takes a path.
    fn scratch_file(&self, data: &[u8], tag: &str) -> Result<PathBuf, FlashError> {
        let name = format!(
            "flashgui_openocd_{tag}_{}_{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let path = std::env::temp_dir().join(name);
        let mut file = std::fs::File::create(&path).map_err(|e| {
            FlashError::Io(format!("could not create a scratch file for OpenOCD: {e}"))
        })?;
        file.write_all(data)
            .map_err(|e| FlashError::Io(e.to_string()))?;
        file.flush().map_err(|e| FlashError::Io(e.to_string()))?;
        Ok(path)
    }
}

/// The target's work area, which is the one part of RAM the configuration
/// declares. `(0, 0)` when it declares none.
fn work_area(link: &mut dyn TclLink, target_name: &str) -> (u32, u32) {
    let number = |reply: &str| -> Option<u32> {
        let text = reply.trim();
        if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            u32::from_str_radix(hex, 16).ok()
        } else {
            text.parse().ok()
        }
    };

    let base = link
        .command(&format!("{target_name} cget -work-area-phys"))
        .ok()
        .filter(|r| !reply_is_error(r))
        .as_deref()
        .and_then(number)
        .unwrap_or(0);
    let size = link
        .command(&format!("{target_name} cget -work-area-size"))
        .ok()
        .filter(|r| !reply_is_error(r))
        .as_deref()
        .and_then(number)
        .unwrap_or(0);
    (base, size)
}

/// The target's type column from `targets`, which is the closest thing
/// OpenOCD offers to an architecture.
fn architecture_of(targets_reply: &str, target_name: &str) -> String {
    for line in targets_reply.lines() {
        if line.contains(target_name) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if let Some(position) = fields.iter().position(|f| *f == target_name) {
                if let Some(kind) = fields.get(position + 1) {
                    return (*kind).to_string();
                }
            }
        }
    }
    "unknown".to_string()
}

impl FlashSession for OpenOcdSession {
    fn target_info(&self) -> Option<&TargetInfo> {
        Some(&self.target)
    }

    /// Only verification. An erase or a write is one OpenOCD command that
    /// returns when it is finished, so there is no point inside it at which a
    /// cancellation could take effect; verification runs one command per
    /// segment and can stop between them.
    fn can_interrupt(&self, stage: FlashStage) -> bool {
        matches!(stage, FlashStage::Verifying)
    }

    /// `flash write_image erase` erases what it is about to write, so a
    /// separate erase pass would clear the same sectors twice.
    fn program_erases_target(&self) -> bool {
        true
    }

    fn erase_all(&mut self, cb: Option<&dyn ProgressCallback>) -> Result<(), FlashError> {
        self.check_open()?;
        let size = self.target.flash_size as u64;
        let base = self.target.flash_base;
        self.report(cb, FlashStage::Erasing, 0, size, base, "Erasing the chip");

        let banks = self.banks.clone();
        for bank in banks {
            self.run(&format!("flash erase_sector {bank} 0 last"), |reply| {
                FlashError::EraseError(format!("OpenOCD could not erase bank {bank}: {reply}"))
            })?;
        }

        self.report(cb, FlashStage::Erasing, size, size, base, "Chip erased");
        Ok(())
    }

    fn erase_range(
        &mut self,
        start: u32,
        length: u32,
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<(), FlashError> {
        self.check_open()?;
        if length == 0 {
            return Ok(());
        }

        // A sector is the smallest erasable unit, so an erase that began
        // mid-sector would take the bytes in front of it too. Refusing the
        // start and rounding the length is the same rule the ESP backend
        // applies, for the same reason.
        let granularity = self
            .target
            .sector_for_address(start)
            .map(|s| s.size)
            .unwrap_or(ASSUMED_SECTOR);
        if !start.is_multiple_of(granularity) {
            return Err(FlashError::InvalidAddress {
                address: start,
                reason: format!(
                    "an erase must start on a flash sector boundary; the sector \
                     at this address is {granularity} bytes, and starting \
                     mid-sector would erase the bytes in front of it too"
                ),
            });
        }

        self.check_range(start, length)?;
        self.report(
            cb,
            FlashStage::Erasing,
            0,
            length as u64,
            start,
            format!("Erasing {length:#x} bytes at {start:#010X}"),
        );

        // erase_address takes the length and applies the driver's own
        // granularity, so the rounding up happens inside OpenOCD.
        self.run(
            &format!("flash erase_address unlock {start:#x} {length:#x}"),
            |reply| {
                FlashError::EraseError(format!(
                    "OpenOCD could not erase {length:#x} bytes at {start:#010X}: {reply}"
                ))
            },
        )?;

        self.report(
            cb,
            FlashStage::Erasing,
            length as u64,
            length as u64,
            start,
            "Erased",
        );
        Ok(())
    }

    fn program(
        &mut self,
        segments: &[MemorySegment],
        _options: &ProgramOptions,
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<(), FlashError> {
        self.check_open()?;
        self.require_local("programming through OpenOCD")?;

        let total: u64 = segments.iter().map(|s| s.data.len() as u64).sum();
        let mut written = 0u64;

        for segment in segments {
            self.check_range(segment.start_address, segment.data.len() as u32)?;

            // Progress is per segment, not per byte: `flash write_image`
            // reports nothing until it finishes, so claiming finer granularity
            // would mean inventing it.
            self.report(
                cb,
                FlashStage::Programming,
                written,
                total,
                segment.start_address,
                format!("Writing {:#010X}", segment.start_address),
            );

            let path = self.scratch_file(&segment.data, "write")?;
            let address = segment.start_address;
            // `erase` here rather than a separate pass: OpenOCD erases exactly
            // the sectors the image covers, which is fewer than a caller
            // computing the span itself would.
            let command = format!(
                "flash write_image erase {} {:#x} bin",
                tcl_path(&path),
                address
            );
            let outcome = self.run(&command, |reply| {
                FlashError::ProgramError(format!(
                    "OpenOCD could not write {} bytes at {address:#010X}: {reply}",
                    segment.data.len()
                ))
            });
            let _ = std::fs::remove_file(&path);
            outcome?;

            written += segment.data.len() as u64;
        }

        self.report(
            cb,
            FlashStage::Programming,
            total,
            total,
            self.target.flash_base,
            "Programmed",
        );
        Ok(())
    }

    fn verify(
        &mut self,
        segments: &[MemorySegment],
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<VerifyReport, FlashError> {
        self.check_open()?;

        let total: u64 = segments.iter().map(|s| s.data.len() as u64).sum();
        let mut verified = 0u64;
        let mut mismatches = Vec::new();
        let mut expected_hasher = crc32fast::Hasher::new();
        let mut actual_hasher = crc32fast::Hasher::new();

        for segment in segments {
            if self.cancelled(cb) {
                return Err(FlashError::OperationCancelled);
            }
            self.report(
                cb,
                FlashStage::Verifying,
                verified,
                total,
                segment.start_address,
                format!("Verifying {:#010X}", segment.start_address),
            );

            expected_hasher.update(&segment.data);

            // Read the region back rather than calling `verify_image`.
            // OpenOCD's own verify is faster but answers only pass or fail,
            // and a verify that cannot say *where* the difference is has
            // thrown away the useful half of the answer.
            let actual = self.read_memory(segment.start_address, segment.data.len() as u32)?;
            actual_hasher.update(&actual);

            for (offset, (expected, got)) in segment.data.iter().zip(actual.iter()).enumerate() {
                if expected != got && mismatches.len() < 32 {
                    mismatches.push(VerifyMismatch {
                        address: segment.start_address.saturating_add(offset as u32),
                        expected: *expected,
                        actual: *got,
                    });
                }
            }

            verified += segment.data.len() as u64;
        }

        let checksum_expected = expected_hasher.finalize();
        let checksum_actual = actual_hasher.finalize();

        self.report(
            cb,
            FlashStage::Verifying,
            total,
            total,
            self.target.flash_base,
            "Verified",
        );

        Ok(VerifyReport {
            success: mismatches.is_empty() && checksum_expected == checksum_actual,
            bytes_verified: verified as u32,
            mismatches,
            checksum_expected,
            checksum_actual,
        })
    }

    fn read_memory(&mut self, address: u32, length: u32) -> Result<Vec<u8>, FlashError> {
        self.check_open()?;
        if length == 0 {
            return Ok(Vec::new());
        }

        // A local OpenOCD can dump straight to a file, which is one round trip
        // instead of one per 16 bytes of console output.
        if self.link.is_local() && length > 64 {
            let path = self.scratch_file(&[], "dump")?;
            let command = format!("dump_image {} {address:#x} {length:#x}", tcl_path(&path));
            let outcome = self.run(&command, |reply| {
                FlashError::ProbeCommunication(format!(
                    "OpenOCD could not read {length:#x} bytes at {address:#010X}: {reply}"
                ))
            });
            let bytes = outcome.and_then(|_| {
                std::fs::read(&path).map_err(|e| {
                    FlashError::Io(format!("OpenOCD wrote no readable dump file: {e}"))
                })
            });
            let _ = std::fs::remove_file(&path);
            let bytes = bytes?;
            if bytes.len() != length as usize {
                return Err(FlashError::ProbeCommunication(format!(
                    "OpenOCD returned {} bytes for a {length}-byte read at {address:#010X}",
                    bytes.len()
                )));
            }
            return Ok(bytes);
        }

        let reply = self.run(&format!("capture {{mdb {address:#x} {length}}}"), |reply| {
            FlashError::ProbeCommunication(format!(
                "OpenOCD could not read {length} bytes at {address:#010X}: {reply}"
            ))
        })?;
        let bytes = parse_mdb(&reply);
        if bytes.len() != length as usize {
            return Err(FlashError::ProbeCommunication(format!(
                "OpenOCD returned {} of {length} bytes for a read at {address:#010X}",
                bytes.len()
            )));
        }
        Ok(bytes)
    }

    fn can_write_memory(&self) -> bool {
        true
    }

    fn write_memory(&mut self, address: u32, data: &[u8]) -> Result<(), FlashError> {
        self.check_open()?;
        if data.is_empty() {
            return Ok(());
        }
        if self.target.overlaps_flash(address, data.len() as u32) {
            return Err(FlashError::InvalidAddress {
                address,
                reason: "a memory write does not erase, so it cannot write flash; \
                         program the image instead"
                    .to_string(),
            });
        }

        // A local OpenOCD loads a file in one command. Without one, bytes go
        // one at a time, which is only sane for a register or two.
        if self.link.is_local() {
            let path = self.scratch_file(data, "load")?;
            let command = format!("load_image {} {address:#x} bin", tcl_path(&path));
            let outcome = self.run(&command, |reply| {
                FlashError::ProbeCommunication(format!(
                    "OpenOCD could not write {} bytes at {address:#010X}: {reply}",
                    data.len()
                ))
            });
            let _ = std::fs::remove_file(&path);
            outcome?;
            return Ok(());
        }

        if data.len() > MAX_BYTEWISE_WRITE {
            return Err(FlashError::Unsupported(format!(
                "a {}-byte memory write to a remote OpenOCD would take one command \
                 per byte. Up to {MAX_BYTEWISE_WRITE} bytes is allowed that way; \
                 beyond that, run OpenOCD on this machine so the bytes can go \
                 through a file.",
                data.len()
            )));
        }

        for (offset, byte) in data.iter().enumerate() {
            let at = address.saturating_add(offset as u32);
            self.run(&format!("mwb {at:#x} {byte:#x}"), |reply| {
                FlashError::ProbeCommunication(format!(
                    "OpenOCD could not write {byte:#04X} at {at:#010X}: {reply}"
                ))
            })?;
        }
        Ok(())
    }

    fn reset(&mut self, halt: bool) -> Result<(), FlashError> {
        self.check_open()?;
        let command = if halt { "reset halt" } else { "reset run" };
        self.run(command, |reply| {
            FlashError::Internal(format!("OpenOCD could not reset the target: {reply}"))
        })?;
        Ok(())
    }

    /// Closes this connection and leaves OpenOCD running.
    ///
    /// OpenOCD is a separate process that may be serving GDB or another client,
    /// so shutting it down would be taking something that was not ours. The
    /// socket closing is the whole of the disconnection.
    fn close(&mut self) -> Result<(), FlashError> {
        self.closed = true;
        Ok(())
    }
}

/// A path as a TCL argument.
///
/// TCL treats a backslash as an escape, so a Windows path has to be written
/// with forward slashes -- which OpenOCD accepts on every platform. Quoting is
/// added because a temporary directory can contain spaces.
pub fn tcl_path(path: &std::path::Path) -> String {
    format!("{{{}}}", path.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_path_reaches_tcl_without_escapes() {
        let path = std::path::Path::new("C:\\Users\\a b\\tmp\\seg.bin");
        assert_eq!(tcl_path(path), "{C:/Users/a b/tmp/seg.bin}");
    }

    #[test]
    fn the_target_type_column_is_the_architecture() {
        let targets = " 0* stm32f4x.cpu       cortex_m   little stm32f4x.cpu       halted";
        assert_eq!(architecture_of(targets, "stm32f4x.cpu"), "cortex_m");
        assert_eq!(architecture_of(targets, "other.cpu"), "unknown");
    }
}
