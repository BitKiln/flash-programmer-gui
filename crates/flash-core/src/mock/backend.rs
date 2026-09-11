use std::sync::{Arc, Mutex};

use firmware_parser::MemorySegment;

use crate::error::FlashError;
use crate::mock::fault::{FaultInjector, InjectedFault};
use crate::mock::memory::MockFlashMemory;
use crate::mock::profiles::get_target_by_name;
use crate::progress::{FlashEvent, FlashStage, ProgressCallback, ProgressMetrics};
use crate::traits::{FlashBackend, FlashSession};
use crate::types::{
    ConnectionConfig, ProbeInfo, ProbeType, ProgramOptions, TargetInfo, VerifyMismatch,
    VerifyReport, WireProtocol,
};

/// In-memory virtual probe backend simulating debug probes and target microcontrollers.
#[derive(Debug, Clone)]
pub struct MockProbeBackend {
    probes: Vec<ProbeInfo>,
    fault_injector: Arc<Mutex<FaultInjector>>,
}

impl Default for MockProbeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProbeBackend {
    /// Creates a new `MockProbeBackend` populated with standard simulated probes.
    pub fn new() -> Self {
        Self {
            probes: Self::default_probes(),
            fault_injector: Arc::new(Mutex::new(FaultInjector::new())),
        }
    }

    /// Creates a `MockProbeBackend` with a custom probe inventory.
    pub fn with_probes(probes: Vec<ProbeInfo>) -> Self {
        Self {
            probes,
            fault_injector: Arc::new(Mutex::new(FaultInjector::new())),
        }
    }

    /// Attaches an existing shared fault injector.
    pub fn with_fault_injector(mut self, injector: Arc<Mutex<FaultInjector>>) -> Self {
        self.fault_injector = injector;
        self
    }

    /// Returns the shared fault injector.
    pub fn fault_injector(&self) -> Arc<Mutex<FaultInjector>> {
        Arc::clone(&self.fault_injector)
    }

    /// Injects a fault into the simulated probe environment.
    pub fn inject_fault(&self, fault: InjectedFault) {
        if let Ok(mut injector) = self.fault_injector.lock() {
            injector.inject(fault);
        }
    }

    /// Clears all injected faults.
    pub fn clear_faults(&self) {
        if let Ok(mut injector) = self.fault_injector.lock() {
            injector.clear();
        }
    }

    /// Standard preset probes for test and development simulation.
    pub fn default_probes() -> Vec<ProbeInfo> {
        vec![
            ProbeInfo {
                identifier: "mock:stlink-stm32f103".to_string(),
                vendor_name: "STMicroelectronics".to_string(),
                product_name: "ST-LINK/V2 (Mock)".to_string(),
                serial_number: Some("MOCKSTLINK001".to_string()),
                probe_type: ProbeType::StLink,
                supported_protocols: vec![WireProtocol::Swd, WireProtocol::Jtag],
                default_speed_khz: 4000,
                max_speed_khz: 10000,
            },
            ProbeInfo {
                identifier: "mock:cmsis-dap-stm32f401".to_string(),
                vendor_name: "ARM".to_string(),
                product_name: "CMSIS-DAP v2 (Mock)".to_string(),
                serial_number: Some("MOCKDAP002".to_string()),
                probe_type: ProbeType::CmsisDap,
                supported_protocols: vec![WireProtocol::Swd],
                default_speed_khz: 4000,
                max_speed_khz: 10000,
            },
            ProbeInfo {
                identifier: "mock:jlink-cortex-m".to_string(),
                vendor_name: "SEGGER".to_string(),
                product_name: "J-Link Plus (Mock)".to_string(),
                serial_number: Some("MOCKJLINK003".to_string()),
                probe_type: ProbeType::JLink,
                supported_protocols: vec![WireProtocol::Swd, WireProtocol::Jtag],
                default_speed_khz: 4000,
                max_speed_khz: 12000,
            },
            ProbeInfo {
                identifier: "mock:stm32f401".to_string(),
                vendor_name: "Virtual".to_string(),
                product_name: "Mock STM32F401 Probe".to_string(),
                serial_number: Some("MOCKF401".to_string()),
                probe_type: ProbeType::VirtualMock,
                supported_protocols: vec![WireProtocol::Swd, WireProtocol::Jtag],
                default_speed_khz: 4000,
                max_speed_khz: 10000,
            },
            ProbeInfo {
                identifier: "mock:stm32f103".to_string(),
                vendor_name: "Virtual".to_string(),
                product_name: "Mock STM32F103 Probe".to_string(),
                serial_number: Some("MOCKF103".to_string()),
                probe_type: ProbeType::VirtualMock,
                supported_protocols: vec![WireProtocol::Swd, WireProtocol::Jtag],
                default_speed_khz: 4000,
                max_speed_khz: 10000,
            },
            ProbeInfo {
                identifier: "mock:generic-cortex-m".to_string(),
                vendor_name: "Virtual".to_string(),
                product_name: "Mock Generic Cortex-M Probe".to_string(),
                serial_number: Some("MOCKGENERIC".to_string()),
                probe_type: ProbeType::VirtualMock,
                supported_protocols: vec![WireProtocol::Swd, WireProtocol::Jtag],
                default_speed_khz: 4000,
                max_speed_khz: 10000,
            },
        ]
    }
}

impl FlashBackend for MockProbeBackend {
    fn name(&self) -> &'static str {
        "mock-probe"
    }

    fn list_probes(&self) -> Result<Vec<ProbeInfo>, FlashError> {
        Ok(self.probes.clone())
    }

    fn open_session(&self, config: &ConnectionConfig) -> Result<Box<dyn FlashSession>, FlashError> {
        // Evaluate connection faults
        self.fault_injector
            .lock()
            .map_err(|e| FlashError::Internal(format!("Fault injector lock error: {}", e)))?
            .check_connect()?;

        // Match probe info if probe_id was specified
        let probe = if let Some(ref pid) = config.probe_id {
            self.probes
                .iter()
                .find(|p| p.identifier == *pid)
                .cloned()
                .or_else(|| {
                    if pid.starts_with("mock:") {
                        Some(ProbeInfo {
                            identifier: pid.clone(),
                            vendor_name: "Virtual".to_string(),
                            product_name: format!("Simulated {}", pid),
                            serial_number: None,
                            probe_type: ProbeType::VirtualMock,
                            supported_protocols: vec![config.protocol],
                            default_speed_khz: config.speed_khz,
                            max_speed_khz: config.speed_khz.max(10000),
                        })
                    } else {
                        None
                    }
                })
                .ok_or_else(|| FlashError::ProbeNotFound(pid.clone()))?
        } else {
            self.probes
                .first()
                .cloned()
                .unwrap_or_else(|| ProbeInfo {
                    identifier: "mock:default".to_string(),
                    vendor_name: "Virtual".to_string(),
                    product_name: "Default Mock Probe".to_string(),
                    serial_number: None,
                    probe_type: ProbeType::VirtualMock,
                    supported_protocols: vec![WireProtocol::Swd],
                    default_speed_khz: 4000,
                    max_speed_khz: 10000,
                })
        };

        // Resolve target geometry
        let target = get_target_by_name(&config.target_name)
            .or_else(|| {
                if let Some(ref pid) = config.probe_id {
                    get_target_by_name(pid)
                } else {
                    None
                }
            })
            .ok_or_else(|| FlashError::TargetNotSupported(config.target_name.clone()))?;

        let mut session = MockFlashSession::new(target, Arc::clone(&self.fault_injector));
        session.probe_info = Some(probe);
        Ok(Box::new(session))
    }
}

/// Active in-memory session simulating target MCU flash operations and status.
pub struct MockFlashSession {
    pub probe_info: Option<ProbeInfo>,
    pub target_info: TargetInfo,
    pub memory: MockFlashMemory,
    pub fault_injector: Arc<Mutex<FaultInjector>>,
    pub halted: bool,
    pub closed: bool,
    pub reset_count: usize,
}

impl MockFlashSession {
    /// Creates a new `MockFlashSession` with memory initialized to erased `0xFF`.
    pub fn new(target: TargetInfo, fault_injector: Arc<Mutex<FaultInjector>>) -> Self {
        let memory = MockFlashMemory::from_target(&target);
        Self {
            probe_info: None,
            target_info: target,
            memory,
            fault_injector,
            halted: false,
            closed: false,
            reset_count: 0,
        }
    }

    /// Creates a `MockFlashSession` initialized with existing flash memory.
    pub fn new_with_memory(
        target: TargetInfo,
        memory: MockFlashMemory,
        fault_injector: Arc<Mutex<FaultInjector>>,
    ) -> Self {
        Self {
            probe_info: None,
            target_info: target,
            memory,
            fault_injector,
            halted: false,
            closed: false,
            reset_count: 0,
        }
    }

    /// Returns a reference to the simulated NOR flash memory.
    pub fn memory(&self) -> &MockFlashMemory {
        &self.memory
    }

    /// Returns a mutable reference to the simulated NOR flash memory.
    pub fn memory_mut(&mut self) -> &mut MockFlashMemory {
        &mut self.memory
    }

    /// Injects a fault into the session's fault injector.
    pub fn inject_fault(&self, fault: InjectedFault) {
        if let Ok(mut injector) = self.fault_injector.lock() {
            injector.inject(fault);
        }
    }

    /// Clears all injected faults from the session.
    pub fn clear_faults(&self) {
        if let Ok(mut injector) = self.fault_injector.lock() {
            injector.clear();
        }
    }

    /// Returns true if core is in halted state.
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// Returns true if session has been closed.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Returns total number of reset operations performed during this session.
    pub fn reset_count(&self) -> usize {
        self.reset_count
    }

    fn check_alive(&self) -> Result<(), FlashError> {
        if self.closed {
            return Err(FlashError::InvalidState("Session is closed".to_string()));
        }
        self.fault_injector
            .lock()
            .map_err(|e| FlashError::Internal(format!("Lock error: {}", e)))?
            .check_connection()
    }
}

impl FlashSession for MockFlashSession {
    fn target_info(&self) -> Option<&TargetInfo> {
        Some(&self.target_info)
    }

    fn erase_all(&mut self, cb: Option<&dyn ProgressCallback>) -> Result<(), FlashError> {
        self.check_alive()?;

        // Fault check on mass erase
        self.fault_injector
            .lock()
            .map_err(|e| FlashError::Internal(format!("Lock error: {}", e)))?
            .check_erase(self.target_info.flash_base)?;

        let total_bytes = self.target_info.flash_size as u64;

        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageStarted {
                stage: FlashStage::Erasing,
                total_bytes,
                message: format!(
                    "Mass-erasing entire target flash ({} KB)...",
                    self.target_info.flash_size / 1024
                ),
            });
        }

        if let Some(callback) = cb {
            if callback.is_cancelled() {
                return Err(FlashError::OperationCancelled);
            }
        }

        // Perform memory erase
        self.memory.erase_all();

        if let Some(callback) = cb {
            let metrics = ProgressMetrics::new(
                FlashStage::Erasing,
                total_bytes,
                total_bytes,
                10,
                self.target_info.flash_base,
                "Mass erase complete".to_string(),
            );
            callback.on_event(FlashEvent::Progress(metrics));
            callback.on_event(FlashEvent::StageCompleted {
                stage: FlashStage::Erasing,
                duration_ms: 10,
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
        self.check_alive()?;
        self.memory.check_bounds(start, length)?;

        if length == 0 {
            return Ok(());
        }

        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageStarted {
                stage: FlashStage::Erasing,
                total_bytes: length as u64,
                message: format!(
                    "Erasing flash range 0x{:08X}..0x{:08X} ({} bytes)...",
                    start,
                    start.saturating_add(length),
                    length
                ),
            });
        }

        if let Some(callback) = cb {
            if callback.is_cancelled() {
                return Err(FlashError::OperationCancelled);
            }
        }

        // Check each affected sector for injected faults
        let affected = self.target_info.sectors_in_range(start, length);
        {
            let injector = self
                .fault_injector
                .lock()
                .map_err(|e| FlashError::Internal(format!("Lock error: {}", e)))?;
            for sector in &affected {
                injector.check_erase(sector.address)?;
            }
        }

        // Erase overlapping sectors in memory
        self.memory.erase_range(start, length)?;

        if let Some(callback) = cb {
            let metrics = ProgressMetrics::new(
                FlashStage::Erasing,
                length as u64,
                length as u64,
                5,
                start,
                "Sector erase complete".to_string(),
            );
            callback.on_event(FlashEvent::Progress(metrics));
            callback.on_event(FlashEvent::StageCompleted {
                stage: FlashStage::Erasing,
                duration_ms: 5,
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
        self.check_alive()?;

        let total_bytes: u64 = segments.iter().map(|s| s.data.len() as u64).sum();

        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageStarted {
                stage: FlashStage::Programming,
                total_bytes,
                message: format!(
                    "Programming {} memory segment(s) ({} bytes total)...",
                    segments.len(),
                    total_bytes
                ),
            });
        }

        let chunk_size = options.chunk_size.max(1);
        let mut written_so_far: u64 = 0;
        let start_time = std::time::Instant::now();

        for segment in segments {
            if segment.data.is_empty() {
                continue;
            }

            self.memory
                .check_bounds(segment.start_address, segment.data.len() as u32)?;

            for (chunk_idx, chunk) in segment.data.chunks(chunk_size).enumerate() {
                if let Some(callback) = cb {
                    if callback.is_cancelled() {
                        return Err(FlashError::OperationCancelled);
                    }
                }

                let chunk_addr = segment
                    .start_address
                    .saturating_add((chunk_idx * chunk_size) as u32);

                // Check injected programming faults
                self.fault_injector
                    .lock()
                    .map_err(|e| FlashError::Internal(format!("Lock error: {}", e)))?
                    .check_program_chunk(chunk_addr, chunk.len())?;

                // Write chunk to physical NOR flash (enforces 1->0 write limits)
                self.memory.write_bytes(chunk_addr, chunk)?;

                written_so_far += chunk.len() as u64;

                if let Some(callback) = cb {
                    let elapsed_ms = start_time.elapsed().as_millis() as u64;
                    let metrics = ProgressMetrics::new(
                        FlashStage::Programming,
                        written_so_far,
                        total_bytes,
                        elapsed_ms,
                        chunk_addr,
                        format!("Programmed chunk at 0x{:08X}", chunk_addr),
                    );
                    callback.on_event(FlashEvent::Progress(metrics));
                }
            }
        }

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
        self.check_alive()?;

        let total_bytes: u64 = segments.iter().map(|s| s.data.len() as u64).sum();

        if let Some(callback) = cb {
            callback.on_event(FlashEvent::StageStarted {
                stage: FlashStage::Verifying,
                total_bytes,
                message: format!(
                    "Verifying {} memory segment(s) ({} bytes total)...",
                    segments.len(),
                    total_bytes
                ),
            });
        }

        let mut mismatches = Vec::new();
        let mut hasher_expected = crc32fast::Hasher::new();
        let mut hasher_actual = crc32fast::Hasher::new();
        let mut bytes_verified: u32 = 0;
        let start_time = std::time::Instant::now();

        for segment in segments {
            if segment.data.is_empty() {
                continue;
            }

            let actual_bytes = self
                .memory
                .read_bytes(segment.start_address, segment.data.len() as u32)?;

            let injector = self
                .fault_injector
                .lock()
                .map_err(|e| FlashError::Internal(format!("Lock error: {}", e)))?;

            for (i, &expected_byte) in segment.data.iter().enumerate() {
                let addr = segment.start_address.saturating_add(i as u32);
                let raw_actual = actual_bytes[i];
                let corrupted_actual = injector.maybe_corrupt_verify_byte(addr, raw_actual);

                if expected_byte != corrupted_actual {
                    mismatches.push(VerifyMismatch {
                        address: addr,
                        expected: expected_byte,
                        actual: corrupted_actual,
                    });
                }

                hasher_expected.update(&[expected_byte]);
                hasher_actual.update(&[corrupted_actual]);
                bytes_verified += 1;

                if let Some(callback) = cb {
                    if (bytes_verified.is_multiple_of(1024) || (bytes_verified as u64) == total_bytes)
                        && callback.is_cancelled()
                    {
                        return Err(FlashError::OperationCancelled);
                    }
                }
            }

            if let Some(callback) = cb {
                let elapsed_ms = start_time.elapsed().as_millis() as u64;
                let metrics = ProgressMetrics::new(
                    FlashStage::Verifying,
                    bytes_verified as u64,
                    total_bytes,
                    elapsed_ms,
                    segment.start_address,
                    format!("Verified segment at 0x{:08X}", segment.start_address),
                );
                callback.on_event(FlashEvent::Progress(metrics));
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
        self.check_alive()?;
        let raw = self.memory.read_bytes(address, length)?;
        let injector = self
            .fault_injector
            .lock()
            .map_err(|e| FlashError::Internal(format!("Lock error: {}", e)))?;

        let mut result = Vec::with_capacity(raw.len());
        for (i, &b) in raw.iter().enumerate() {
            let addr = address.saturating_add(i as u32);
            result.push(injector.maybe_corrupt_verify_byte(addr, b));
        }
        Ok(result)
    }

    fn reset(&mut self, halt: bool) -> Result<(), FlashError> {
        self.check_alive()?;

        self.fault_injector
            .lock()
            .map_err(|e| FlashError::Internal(format!("Lock error: {}", e)))?
            .check_reset()?;

        self.halted = halt;
        self.reset_count += 1;
        Ok(())
    }

    fn close(&mut self) -> Result<(), FlashError> {
        self.closed = true;
        Ok(())
    }
}
