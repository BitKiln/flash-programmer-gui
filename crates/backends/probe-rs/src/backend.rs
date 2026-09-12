use std::time::Instant;

use firmware_parser::MemorySegment;
use probe_rs::flashing::{DownloadOptions, FlashProgress};
use probe_rs::probe::list::Lister;
use probe_rs::probe::Probe;
use probe_rs::probe::WireProtocol as RsWireProtocol;
use probe_rs::{MemoryInterface, Permissions, Session};

use flash_core::error::FlashError;
use flash_core::progress::{FlashEvent, FlashStage, ProgressCallback, ProgressMetrics};
use flash_core::traits::{FlashBackend, FlashSession};
use flash_core::types::{
    ConnectionConfig, ProbeInfo, ProbeType, ProgramOptions, TargetInfo, VerifyMismatch,
    VerifyReport, WireProtocol,
};

/// Live hardware backend leveraging `probe-rs` to communicate with physical debug probes.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProbeRsLiveBackend;

impl ProbeRsLiveBackend {
    pub fn new() -> Self {
        Self
    }
}

pub(crate) fn open_probe_internal(
    matched: &probe_rs::probe::DebugProbeInfo,
    config: &ConnectionConfig,
) -> Result<Probe, FlashError> {
    let mut probe = matched.open().map_err(|e| {
        let err_str = e.to_string();
        if err_str.contains("code: Some(5)")
            || err_str.contains("Access is denied")
            || err_str.contains("failed to open device")
        {
            FlashError::ProbeCommunication(
                "Access denied: Probe is currently held exclusively by another application (e.g. STM32CubeProgrammer, STM32CubeIDE, or OpenOCD). Please close or disconnect other tools and try again.".to_string(),
            )
        } else {
            FlashError::ProbeCommunication(err_str)
        }
    })?;

    let protocol = match config.protocol {
        WireProtocol::Swd => RsWireProtocol::Swd,
        WireProtocol::Jtag => RsWireProtocol::Jtag,
    };
    probe
        .select_protocol(protocol)
        .map_err(|e| FlashError::ProbeCommunication(e.to_string()))?;
    probe
        .set_speed(config.speed_khz)
        .map_err(|e| FlashError::ProbeCommunication(e.to_string()))?;

    Ok(probe)
}

/// Resolves a stored probe identifier against the probes currently connected.
///
/// Matching is deliberately forgiving: a probe can re-enumerate (changing the
/// identifier string) between listing and connecting, particularly after a
/// failed attach, and the stored id would otherwise go stale.
fn resolve_probe(
    probes: Vec<probe_rs::probe::DebugProbeInfo>,
    probe_id: Option<&str>,
) -> Result<probe_rs::probe::DebugProbeInfo, FlashError> {
    if probes.is_empty() {
        return Err(FlashError::ProbeNotFound(
            "No debug probe connected. Plug in an ST-Link, J-Link or CMSIS-DAP probe and refresh."
                .to_string(),
        ));
    }

    let Some(pid) = probe_id else {
        return Ok(probes.into_iter().next().expect("probes is non-empty"));
    };

    // Tier 1: exact "identifier:serial", or bare identifier.
    if let Some(p) = probes.iter().find(|p| probe_identifier(p) == pid || p.identifier == pid) {
        return Ok(p.clone());
    }

    // Tier 2: serial number alone - survives an identifier string change.
    let serial_part = pid.rsplit(':').next().unwrap_or(pid);
    if serial_part != "unknown" {
        if let Some(p) = probes
            .iter()
            .find(|p| p.serial_number.as_deref() == Some(serial_part))
        {
            return Ok(p.clone());
        }
    }

    // Tier 3: same USB vendor/product pair.
    if let Some(p) = probes
        .iter()
        .find(|p| format!("{:04x}:{:04x}", p.vendor_id, p.product_id) == pid)
    {
        return Ok(p.clone());
    }

    // Tier 4: only one probe is connected - it is what the user meant.
    if probes.len() == 1 {
        return Ok(probes.into_iter().next().expect("len == 1"));
    }

    let available = probes
        .iter()
        .map(probe_identifier)
        .collect::<Vec<_>>()
        .join(", ");
    Err(FlashError::ProbeNotFound(format!(
        "probe '{pid}' is no longer connected. Connected probes: {available}"
    )))
}

/// Stable identifier string for a probe, shared by listing and matching.
fn probe_identifier(p: &probe_rs::probe::DebugProbeInfo) -> String {
    format!(
        "{}:{}",
        p.identifier,
        p.serial_number.as_deref().unwrap_or("unknown")
    )
}

impl FlashBackend for ProbeRsLiveBackend {
    fn name(&self) -> &'static str {
        "probe-rs"
    }

    fn scheme(&self) -> &'static str {
        "probe"
    }

    fn list_probes(&self) -> Result<Vec<ProbeInfo>, FlashError> {
        let lister = Lister::new();
        let probes = lister.list_all();
        let mut result = Vec::new();

        for p in probes {
            let p_type = p.probe_type();
            let type_str = format!("{:?}", p_type).to_lowercase();
            let probe_type = if type_str.contains("stlink") || type_str.contains("st_link") {
                ProbeType::StLink
            } else if type_str.contains("cmsis") || type_str.contains("dap") {
                ProbeType::CmsisDap
            } else if type_str.contains("jlink") || type_str.contains("j_link") {
                ProbeType::JLink
            } else {
                ProbeType::Other(format!("{:?}", p_type))
            };

            let identifier = format!(
                "{}:{}",
                p.identifier,
                p.serial_number.as_deref().unwrap_or("unknown")
            );

            result.push(ProbeInfo {
                identifier,
                vendor_name: format!("{:04x}", p.vendor_id),
                product_name: format!("{:04x}:{:04x} ({:?})", p.vendor_id, p.product_id, p_type),
                serial_number: p.serial_number,
                probe_type,
                supported_protocols: vec![WireProtocol::Swd, WireProtocol::Jtag],
                default_speed_khz: 4000,
                max_speed_khz: 10000,
            });
        }

        Ok(result)
    }

    fn open_session(&self, config: &ConnectionConfig) -> Result<Box<dyn FlashSession>, FlashError> {
        let lister = Lister::new();
        let probes = lister.list_all();
        let matched = resolve_probe(probes, config.probe_id.as_deref())?;

        let is_auto = config.target_name.trim().is_empty()
            || config.target_name.eq_ignore_ascii_case("auto")
            || config.target_name.eq_ignore_ascii_case("default");

        let permissions = || Permissions::new().allow_erase_all();

        let session = if is_auto {
            // probe-rs' own auto detection first - it covers every chip whose
            // debug interface advertises a recognisable identification code.
            let probe = open_probe_internal(&matched, config)?;
            let auto_res = if config.connect_under_reset {
                probe.attach_under_reset(probe_rs::config::TargetSelector::Auto, permissions())
            } else {
                probe.attach(probe_rs::config::TargetSelector::Auto, permissions())
            };

            match auto_res {
                Ok(s) => s,
                Err(e) => {
                    // Fall back to reading the chip's own ID registers and
                    // resolving the result through the target registry.
                    let detected = crate::detect::detect(&matched, config);

                    let selector = match (&detected.chip, detected.core) {
                        (Some(chip), _) => probe_rs::config::TargetSelector::from(chip.as_str()),
                        (None, Some(core)) => probe_rs::config::TargetSelector::from(core),
                        (None, None) => {
                            return Err(FlashError::ConnectError(format!(
                                "Auto-detection failed: {e}. The target did not respond on SWD/JTAG. Check wiring and power, try 'connect under reset', or enter the MCU part number directly."
                            )));
                        }
                    };

                    let label = detected
                        .chip
                        .clone()
                        .unwrap_or_else(|| detected.core.unwrap_or("unknown").to_string());

                    let probe = open_probe_internal(&matched, config)?;
                    let attached = if config.connect_under_reset {
                        probe.attach_under_reset(selector, permissions())
                    } else {
                        probe.attach(selector, permissions())
                    }
                    .map_err(|e2| {
                        FlashError::ConnectError(format!(
                            "Detected target '{label}', but attach failed: {e2}"
                        ))
                    })?;

                    attached
                }
            }
        } else {
            // Manual entry: try the raw name, then a board alias, then report
            // near matches from the registry instead of a bare failure.
            let raw = config.target_name.trim();
            let probe = open_probe_internal(&matched, config)?;
            let first = if config.connect_under_reset {
                probe.attach_under_reset(probe_rs::config::TargetSelector::from(raw), permissions())
            } else {
                probe.attach(probe_rs::config::TargetSelector::from(raw), permissions())
            };

            match first {
                Ok(s) => s,
                Err(first_err) => {
                    let alias = device_db::resolve_target_alias(raw);
                    let retry = match alias {
                        Some(canonical) if !canonical.eq_ignore_ascii_case(raw) => {
                            let probe = open_probe_internal(&matched, config)?;
                            let sel = probe_rs::config::TargetSelector::from(canonical);
                            if config.connect_under_reset {
                                probe.attach_under_reset(sel, permissions()).ok()
                            } else {
                                probe.attach(sel, permissions()).ok()
                            }
                        }
                        _ => None,
                    };

                    match retry {
                        Some(s) => s,
                        None => {
                            let near = crate::detect::suggestions(raw);
                            let hint = if near.is_empty() {
                                String::new()
                            } else {
                                format!(" Did you mean: {}?", near.join(", "))
                            };
                            return Err(FlashError::ConnectError(format!(
                                "Could not connect to '{raw}': {first_err}.{hint}"
                            )));
                        }
                    }
                }
            }
        };

        // A wrong manual target attaches fine and only misbehaves at erase or
        // program time, so reject it here while the failure is still explainable.
        let mut session = session;
        if !is_auto {
            if let Some(reason) = crate::detect::target_mismatch(&mut session, &config.target_name)
            {
                return Err(FlashError::ConnectError(reason));
            }
        }

        Ok(Box::new(ProbeRsLiveSession::new(session, config)))
    }
}

/// Active connection session wrapping a live `probe-rs::Session`.
pub struct ProbeRsLiveSession {
    session: Session,
    target_info: Option<TargetInfo>,
}

impl ProbeRsLiveSession {
    pub fn new(mut session: Session, _config: &ConnectionConfig) -> Self {
        let mut target_info = target_info_from_session(&session);

        // Refine the human-readable label from on-chip identification. This
        // never touches `name`, which must stay resolvable by the registry.
        if let Some(display) = read_display_name(&mut session) {
            if let Some(ref mut info) = target_info {
                info.display_name = Some(display);
            }
        }

        Self {
            session,
            target_info,
        }
    }
}

/// Builds target geometry from the probe-rs target description, which is the
/// authoritative memory map for the attached chip.
fn target_info_from_session(session: &Session) -> Option<TargetInfo> {
    let target = session.target();

    // Chips commonly split flash and RAM into several banks, plus aliased
    // views of the same memory. Sum the real banks of each kind and take the
    // lowest base, so a dual-bank part reports its whole flash.
    let mut flash: Option<(u64, u64)> = None;
    let mut ram: Option<(u64, u64)> = None;
    for region in &target.memory_map {
        match region {
            probe_rs::config::MemoryRegion::Nvm(nvm) if !nvm.is_alias && !is_aux_nvm(nvm) => {
                let size = nvm.range.end.saturating_sub(nvm.range.start);
                flash = Some(match flash {
                    Some((base, total)) => (base.min(nvm.range.start), total + size),
                    None => (nvm.range.start, size),
                });
            }
            probe_rs::config::MemoryRegion::Ram(r) if !r.is_alias => {
                let size = r.range.end.saturating_sub(r.range.start);
                // Count every bank towards the total, but report the base of
                // the conventional SRAM window: an H7 maps ITCM at 0x00000000,
                // and quoting that as "the RAM base" is misleading.
                let conventional = r.range.start >= 0x2000_0000;
                ram = Some(match ram {
                    Some((base, total)) if conventional => (base.min(r.range.start), total + size),
                    Some((base, total)) => (base, total + size),
                    None => (r.range.start, size),
                });
            }
            _ => {}
        }
    }

    let (flash_base, flash_size) = flash.unwrap_or((0x0800_0000, 0));
    let (ram_base, ram_size) = ram.unwrap_or((0x2000_0000, 0));
    let ram_base = target
        .memory_map
        .iter()
        .filter_map(|region| match region {
            probe_rs::config::MemoryRegion::Ram(r) if !r.is_alias && r.range.start >= 0x2000_0000 => {
                Some(r.range.start)
            }
            _ => None,
        })
        .min()
        .unwrap_or(ram_base);

    // Sector layout and page size come from the flash algorithm covering the
    // selected flash region.
    let props = target
        .flash_algorithms
        .iter()
        .map(|a| &a.flash_properties)
        .find(|p| p.address_range.start == flash_base)
        .or_else(|| target.flash_algorithms.first().map(|a| &a.flash_properties));

    let page_size = props.map(|p| p.page_size).filter(|s| *s > 0).unwrap_or(256);
    let sectors = props
        .map(|p| expand_sectors(p, flash_base, flash_size))
        .unwrap_or_default();

    Some(TargetInfo {
        name: target.name.clone(),
        display_name: None,
        architecture: target
            .cores
            .first()
            .map(|c| format!("{:?}", c.core_type))
            .unwrap_or_else(|| "Cortex-M".to_string()),
        flash_base: flash_base as u32,
        flash_size: flash_size as u32,
        ram_base: ram_base as u32,
        ram_size: ram_size as u32,
        page_size,
        sectors,
    })
}

/// Address ranges of the erasable program flash, in ascending order.
///
/// Excludes aliases and regions no flash algorithm can program (OTP, option
/// bytes, external-memory placeholders). Note that `access.write` is false for
/// ordinary flash too - it describes CPU stores, not programmability.
fn program_flash_ranges(session: &Session) -> Vec<std::ops::Range<u64>> {
    let target = session.target();
    let mut ranges: Vec<std::ops::Range<u64>> = target
        .memory_map
        .iter()
        .filter_map(|region| match region {
            probe_rs::config::MemoryRegion::Nvm(nvm) if !nvm.is_alias && !is_aux_nvm(nvm) => {
                Some(nvm.range.clone())
            }
            _ => None,
        })
        .filter(|range| {
            target.flash_algorithms.iter().any(|algo| {
                let covered = &algo.flash_properties.address_range;
                covered.start < range.end && covered.end > range.start
            })
        })
        .collect();
    ranges.sort_by_key(|r| r.start);
    ranges
}

/// True for non-program flash regions (OTP, option bytes, EEPROM) that should
/// not count towards the usable program flash.
fn is_aux_nvm(nvm: &probe_rs::config::NvmRegion) -> bool {
    let Some(name) = nvm.name.as_deref() else {
        return false;
    };
    let name = name.to_lowercase();
    name.contains("otp") || name.contains("option") || name.contains("eeprom")
}

/// Expands probe-rs sector *descriptions* (a size plus the offset it starts
/// applying from) into the concrete sector list this crate uses.
fn expand_sectors(
    props: &probe_rs::config::FlashProperties,
    flash_base: u64,
    flash_size: u64,
) -> Vec<flash_core::types::SectorInfo> {
    let mut out = Vec::new();
    let descriptions = &props.sectors;
    if descriptions.is_empty() || flash_size == 0 {
        return out;
    }

    let flash_end = flash_base.saturating_add(flash_size);
    let mut index = 0u32;
    for (i, desc) in descriptions.iter().enumerate() {
        if desc.size == 0 {
            continue;
        }
        // A description applies until the next one starts, or to the end of flash.
        let start = flash_base.saturating_add(desc.address);
        let end = descriptions
            .get(i + 1)
            .map(|next| flash_base.saturating_add(next.address))
            .unwrap_or(flash_end)
            .min(flash_end);

        let mut addr = start;
        while addr < end {
            out.push(flash_core::types::SectorInfo {
                index,
                address: addr as u32,
                size: desc.size as u32,
            });
            index += 1;
            addr = addr.saturating_add(desc.size);
        }
    }
    out
}

/// Reads DBGMCU identification to produce a friendlier chip/board label.
///
/// Purely cosmetic - failure simply means no label.
fn read_display_name(session: &mut Session) -> Option<String> {
    let mut core = session.core(0).ok()?;
    for addr in [0xE004_2000u64, 0x5C00_1000, 0xE004_4000, 0x4001_5800] {
        let Ok(val) = core.read_word_32(addr) else {
            continue;
        };
        if val == 0 || val == u32::MAX {
            continue;
        }
        let dev_id = val & 0x0FFF;
        let rev = val >> 16;
        if dev_id == 0 || dev_id == 0x0FFF {
            continue;
        }
        return Some(format!("dev 0x{dev_id:03X} rev 0x{rev:04X}"));
    }
    None
}

/// Bytes read per probe round-trip while verifying. Large enough to amortise
/// the USB round-trip, small enough to keep progress reporting responsive.
const VERIFY_CHUNK_BYTES: usize = 4096;

/// Upper bound on individually reported mismatches; the CRC comparison still
/// covers the whole image, so a wholly wrong region does not build a
/// multi-megabyte report.
const MAX_REPORTED_MISMATCHES: usize = 64;

impl FlashSession for ProbeRsLiveSession {
    fn target_info(&self) -> Option<&TargetInfo> {
        self.target_info.as_ref()
    }

    fn program_erases_target(&self) -> bool {
        true
    }

    /// probe-rs runs an erase and a flash download to completion inside one
    /// call, with nowhere for us to observe a cancellation request; only the
    /// verify pass, which we drive chunk by chunk, can stop early.
    fn can_interrupt(&self, stage: FlashStage) -> bool {
        matches!(stage, FlashStage::Verifying)
    }

    fn erase_all(&mut self, cb: Option<&dyn ProgressCallback>) -> Result<(), FlashError> {
        let start_time = Instant::now();
        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageStarted {
                stage: FlashStage::Erasing,
                total_bytes: 0,
                message: "Erasing all flash sectors via probe-rs...".to_string(),
            });
        }

        // probe_rs::flashing::erase_all() walks *every* NVM region, including
        // read-only ones such as the U5 OTP area at 0x0BFA_0000, which have no
        // flash algorithm and abort the erase. Erase the program-flash regions
        // explicitly instead.
        let regions = program_flash_ranges(&self.session);
        if regions.is_empty() {
            return Err(FlashError::EraseError(
                "No erasable program flash region found for this target".to_string(),
            ));
        }

        let mut progress = FlashProgress::empty();
        for range in regions {
            probe_rs::flashing::erase(&mut self.session, &mut progress, range.start, range.end, false)
                .map_err(|e| FlashError::EraseError(e.to_string()))?;
        }

        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageCompleted {
                stage: FlashStage::Erasing,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
        }

        Ok(())
    }

    fn erase_range(
        &mut self,
        start: u32,
        length: u32,
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<(), FlashError> {
        let start_time = Instant::now();
        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageStarted {
                stage: FlashStage::Erasing,
                total_bytes: length as u64,
                message: format!("Erasing range 0x{:08X}..0x{:08X}", start, start + length),
            });
        }

        let mut progress = FlashProgress::empty();
        let end = start.saturating_add(length);
        probe_rs::flashing::erase(
            &mut self.session,
            &mut progress,
            start as u64,
            end as u64,
            false,
        )
        .map_err(|e| FlashError::EraseError(e.to_string()))?;

        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageCompleted {
                stage: FlashStage::Erasing,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
        }

        Ok(())
    }

    fn program(
        &mut self,
        segments: &[MemorySegment],
        options: &ProgramOptions,
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<(), FlashError> {
        let total_bytes: u64 = segments.iter().map(|s| s.data.len() as u64).sum();
        let start_time = Instant::now();

        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageStarted {
                stage: FlashStage::Programming,
                total_bytes,
                message: format!("Flashing {} bytes...", total_bytes),
            });
        }

        let mut loader = self.session.target().flash_loader();
        for seg in segments {
            loader
                .add_data(seg.start_address as u64, &seg.data)
                .map_err(|e| FlashError::ProgramError(e.to_string()))?;
        }

        let mut download_options = DownloadOptions::default();
        download_options.do_chip_erase = options.chip_erase;
        download_options.verify = false;

        loader
            .commit(&mut self.session, download_options)
            .map_err(|e| FlashError::ProgramError(e.to_string()))?;

        if let Some(callback) = cb {
            let duration_ms = start_time.elapsed().as_millis() as u64;
            callback.on_event(FlashEvent::StageCompleted {
                stage: FlashStage::Programming,
                duration_ms,
            });
        }

        Ok(())
    }

    fn verify(
        &mut self,
        segments: &[MemorySegment],
        cb: Option<&dyn ProgressCallback>,
    ) -> Result<VerifyReport, FlashError> {
        let total_bytes: u64 = segments.iter().map(|s| s.data.len() as u64).sum();
        let start_time = Instant::now();

        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageStarted {
                stage: FlashStage::Verifying,
                total_bytes,
                message: format!("Verifying {} bytes...", total_bytes),
            });
        }

        let mut mismatches = Vec::new();
        let mut hasher_expected = crc32fast::Hasher::new();
        let mut hasher_actual = crc32fast::Hasher::new();
        let mut bytes_verified = 0u32;

        let mut core = self
            .session
            .core(0)
            .map_err(|e| FlashError::ProbeCommunication(e.to_string()))?;

        let mut read_buf = vec![0u8; VERIFY_CHUNK_BYTES];
        for seg in segments {
            for (offset, expected_chunk) in seg.data.chunks(VERIFY_CHUNK_BYTES).enumerate() {
                let chunk_start = seg
                    .start_address
                    .saturating_add((offset * VERIFY_CHUNK_BYTES) as u32);
                let buf = &mut read_buf[..expected_chunk.len()];

                // `read` batches into 32-bit bus accesses where the alignment
                // allows it; `read_8` issues byte-wide accesses and is several
                // times slower over SWD.
                core.read(chunk_start as u64, buf)
                    .map_err(|e| FlashError::ProbeCommunication(e.to_string()))?;

                for (i, (&expected, &actual)) in
                    expected_chunk.iter().zip(buf.iter()).enumerate()
                {
                    if expected != actual && mismatches.len() < MAX_REPORTED_MISMATCHES {
                        mismatches.push(VerifyMismatch {
                            address: chunk_start.saturating_add(i as u32),
                            expected,
                            actual,
                        });
                    }
                }

                hasher_expected.update(expected_chunk);
                hasher_actual.update(buf);
                bytes_verified = bytes_verified.saturating_add(expected_chunk.len() as u32);

                if let Some(callback) = cb {
                    if callback.is_cancelled() {
                        return Err(FlashError::OperationCancelled);
                    }
                    callback.on_event(FlashEvent::Progress(ProgressMetrics::new(
                        FlashStage::Verifying,
                        bytes_verified as u64,
                        total_bytes,
                        start_time.elapsed().as_millis() as u64,
                        chunk_start,
                        "Verifying".to_string(),
                    )));
                }
            }
        }

        let checksum_expected = hasher_expected.finalize();
        let checksum_actual = hasher_actual.finalize();
        let success = mismatches.is_empty() && checksum_expected == checksum_actual;

        if let Some(callback) = cb {
            let duration_ms = start_time.elapsed().as_millis() as u64;
            callback.on_event(FlashEvent::StageCompleted {
                stage: FlashStage::Verifying,
                duration_ms,
            });
        }

        Ok(VerifyReport {
            success,
            bytes_verified,
            mismatches,
            checksum_expected,
            checksum_actual,
        })
    }

    fn read_memory(&mut self, address: u32, length: u32) -> Result<Vec<u8>, FlashError> {
        let mut core = self
            .session
            .core(0)
            .map_err(|e| FlashError::ProbeCommunication(e.to_string()))?;
        let mut buf = vec![0u8; length as usize];
        core.read(address as u64, &mut buf)
            .map_err(|e| FlashError::ProbeCommunication(e.to_string()))?;
        Ok(buf)
    }

    fn reset(&mut self, halt: bool) -> Result<(), FlashError> {
        let mut core = self
            .session
            .core(0)
            .map_err(|e| FlashError::ProbeCommunication(e.to_string()))?;
        if halt {
            core.reset_and_halt(std::time::Duration::from_millis(500))
                .map_err(|e| FlashError::Internal(e.to_string()))?;
        } else {
            core.reset()
                .map_err(|e| FlashError::Internal(e.to_string()))?;
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), FlashError> {
        Ok(())
    }
}
