use serde::{Deserialize, Serialize};

/// Debug wire communication protocol between host probe and target MCU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireProtocol {
    Swd,
    Jtag,
}

/// Category/family of debug probe hardware or virtual simulator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeType {
    StLink,
    CmsisDap,
    JLink,
    VirtualMock,
    Other(String),
}

/// Hardware reset trigger mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetType {
    Software,
    Hardware,
    Core,
}

/// Identification and capability metadata for a debug probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeInfo {
    pub identifier: String,
    pub vendor_name: String,
    pub product_name: String,
    pub serial_number: Option<String>,
    pub probe_type: ProbeType,
    pub supported_protocols: Vec<WireProtocol>,
    pub default_speed_khz: u32,
    pub max_speed_khz: u32,
}

/// Metadata describing a discrete erasable flash memory sector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectorInfo {
    pub index: u32,
    pub address: u32,
    pub size: u32,
}

impl SectorInfo {
    #[inline]
    pub fn end_address(&self) -> u32 {
        self.address.saturating_add(self.size)
    }

    #[inline]
    pub fn contains(&self, addr: u32) -> bool {
        addr >= self.address && addr < self.end_address()
    }

    #[inline]
    pub fn overlaps(&self, start: u32, length: u32) -> bool {
        if length == 0 {
            return false;
        }
        let end = start.saturating_add(length);
        start < self.end_address() && end > self.address
    }
}

/// Microcontroller target specification and memory boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetInfo {
    pub name: String,
    pub architecture: String,
    pub flash_base: u32,
    pub flash_size: u32,
    pub ram_base: u32,
    pub ram_size: u32,
    pub page_size: u32,
    pub sectors: Vec<SectorInfo>,
}

impl TargetInfo {
    #[inline]
    pub fn flash_end(&self) -> u32 {
        self.flash_base.saturating_add(self.flash_size)
    }

    #[inline]
    pub fn contains_flash_address(&self, addr: u32) -> bool {
        addr >= self.flash_base && addr < self.flash_end()
    }

    pub fn sector_for_address(&self, addr: u32) -> Option<&SectorInfo> {
        self.sectors.iter().find(|s| s.contains(addr))
    }

    pub fn sectors_in_range(&self, start: u32, length: u32) -> Vec<&SectorInfo> {
        self.sectors
            .iter()
            .filter(|s| s.overlaps(start, length))
            .collect()
    }
}

/// Connection parameters requested when opening a target session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub probe_id: Option<String>,
    pub target_name: String,
    pub protocol: WireProtocol,
    pub speed_khz: u32,
    pub connect_under_reset: bool,
    pub reset_type: Option<ResetType>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            probe_id: None,
            target_name: "stm32f401re".to_string(),
            protocol: WireProtocol::Swd,
            speed_khz: 4000,
            connect_under_reset: false,
            reset_type: Some(ResetType::Software),
        }
    }
}

/// Options controlling erase, programming, and verification execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramOptions {
    pub verify_after: bool,
    pub reset_after: bool,
    pub chip_erase: bool,
    pub chunk_size: usize,
}

impl Default for ProgramOptions {
    fn default() -> Self {
        Self {
            verify_after: true,
            reset_after: true,
            chip_erase: false,
            chunk_size: 1024,
        }
    }
}

/// Detail of a single byte discrepancy detected during flash verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyMismatch {
    pub address: u32,
    pub expected: u8,
    pub actual: u8,
}

/// Structured summary of flash memory verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    pub success: bool,
    pub bytes_verified: u32,
    pub mismatches: Vec<VerifyMismatch>,
    pub checksum_expected: u32,
    pub checksum_actual: u32,
}

/// Overall outcome of an orchestrated flash execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlashResult {
    pub success: bool,
    pub bytes_flashed: u32,
    pub duration_ms: u64,
    pub verify_report: Option<VerifyReport>,
    pub reset_performed: bool,
    pub message: String,
}

