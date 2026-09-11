use std::time::Instant;

use firmware_parser::MemorySegment;
use probe_rs::flashing::{DownloadOptions, FlashProgress};
use probe_rs::probe::list::Lister;
use probe_rs::probe::WireProtocol as RsWireProtocol;
use probe_rs::{MemoryInterface, Permissions, Session};

use crate::error::FlashError;
use crate::progress::{FlashEvent, FlashStage, ProgressCallback};
use crate::traits::{FlashBackend, FlashSession};
use crate::types::{
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

impl FlashBackend for ProbeRsLiveBackend {
    fn name(&self) -> &'static str {
        "probe-rs"
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
        let matched = if let Some(ref pid) = config.probe_id {
            probes
                .into_iter()
                .find(|p| {
                    let id = format!(
                        "{}:{}",
                        p.identifier,
                        p.serial_number.as_deref().unwrap_or("unknown")
                    );
                    id == *pid || p.identifier == *pid
                })
                .ok_or_else(|| FlashError::ProbeNotFound(pid.clone()))?
        } else {
            probes
                .into_iter()
                .next()
                .ok_or_else(|| FlashError::ProbeNotFound("No connected probe found".to_string()))?
        };

        let mut probe = matched
            .open()
            .map_err(|e| FlashError::ProbeCommunication(e.to_string()))?;

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

        let permissions = Permissions::default();
        let target_selector = config.target_name.as_str();

        let session = if config.connect_under_reset {
            probe
                .attach_under_reset(target_selector, permissions)
                .map_err(|e| FlashError::ConnectError(e.to_string()))?
        } else {
            probe
                .attach(target_selector, permissions)
                .map_err(|e| FlashError::ConnectError(e.to_string()))?
        };

        Ok(Box::new(ProbeRsLiveSession::new(session, config)))
    }
}

/// Active connection session wrapping a live `probe-rs::Session`.
pub struct ProbeRsLiveSession {
    session: Session,
    target_info: Option<TargetInfo>,
}

impl ProbeRsLiveSession {
    pub fn new(session: Session, config: &ConnectionConfig) -> Self {
        let target_info = crate::mock::profiles::get_target_by_name(&config.target_name);
        Self {
            session,
            target_info,
        }
    }
}

impl FlashSession for ProbeRsLiveSession {
    fn target_info(&self) -> Option<&TargetInfo> {
        self.target_info.as_ref()
    }

    fn erase_all(&mut self, cb: Option<&dyn ProgressCallback>) -> Result<(), FlashError> {
        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageStarted {
                stage: FlashStage::Erasing,
                total_bytes: 0,
                message: "Erasing all flash sectors via probe-rs...".to_string(),
            });
        }

        let mut progress = FlashProgress::empty();
        probe_rs::flashing::erase_all(&mut self.session, &mut progress, false)
            .map_err(|e| FlashError::EraseError(e.to_string()))?;

        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageCompleted {
                stage: FlashStage::Erasing,
                duration_ms: 0,
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
                duration_ms: 0,
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

        for seg in segments {
            let mut read_buf = vec![0u8; seg.data.len()];
            core.read_8(seg.start_address as u64, &mut read_buf)
                .map_err(|e| FlashError::ProbeCommunication(e.to_string()))?;

            for (i, &expected) in seg.data.iter().enumerate() {
                let actual = read_buf[i];
                let addr = seg.start_address.saturating_add(i as u32);
                if expected != actual {
                    mismatches.push(VerifyMismatch {
                        address: addr,
                        expected,
                        actual,
                    });
                }
                hasher_expected.update(&[expected]);
                hasher_actual.update(&[actual]);
                bytes_verified += 1;
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
        core.read_8(address as u64, &mut buf)
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
