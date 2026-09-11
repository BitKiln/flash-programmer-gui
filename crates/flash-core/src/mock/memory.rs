use crate::error::FlashError;
use crate::types::{SectorInfo, TargetInfo};

/// Simulated physical NOR flash memory with bit-clearing physics and sector layouts.
#[derive(Debug, Clone)]
pub struct MockFlashMemory {
    pub base_address: u32,
    pub total_size: u32,
    pub sectors: Vec<SectorInfo>,
    pub data: Vec<u8>,
    pub strict_nor_mode: bool,
}

impl MockFlashMemory {
    /// Creates a new mock NOR flash storage initialized completely to erased state (0xFF).
    pub fn new(base_address: u32, total_size: u32, sectors: Vec<SectorInfo>) -> Self {
        Self {
            base_address,
            total_size,
            sectors,
            data: vec![0xFF; total_size as usize],
            strict_nor_mode: true,
        }
    }

    /// Creates a mock NOR flash storage based on a target definition.
    pub fn from_target(target: &TargetInfo) -> Self {
        Self::new(target.flash_base, target.flash_size, target.sectors.clone())
    }

    /// Sets whether strict NOR flash bit violation checks are enforced.
    pub fn set_strict_nor(&mut self, enabled: bool) {
        self.strict_nor_mode = enabled;
    }

    #[inline]
    pub fn end_address(&self) -> u32 {
        self.base_address.saturating_add(self.total_size)
    }

    /// Validates whether the range [address, address + length) falls within memory bounds.
    pub fn check_bounds(&self, address: u32, length: u32) -> Result<(), FlashError> {
        if length == 0 {
            return Ok(());
        }
        if address < self.base_address {
            return Err(FlashError::AddressOutOfBounds {
                address,
                base: self.base_address,
                size: self.total_size,
            });
        }
        let end = address.checked_add(length).ok_or(FlashError::AddressOutOfBounds {
            address,
            base: self.base_address,
            size: self.total_size,
        })?;
        if end > self.end_address() {
            return Err(FlashError::AddressOutOfBounds {
                address: end,
                base: self.base_address,
                size: self.total_size,
            });
        }
        Ok(())
    }

    /// Mass-erases the entire flash memory back to 0xFF.
    pub fn erase_all(&mut self) {
        self.data.fill(0xFF);
    }

    /// Erases a single sector by its index, resetting all its bytes to 0xFF.
    pub fn erase_sector(&mut self, sector_idx: u32) -> Result<(), FlashError> {
        let sector = self
            .sectors
            .iter()
            .find(|s| s.index == sector_idx)
            .ok_or_else(|| FlashError::InvalidAddress {
                address: 0,
                reason: format!("Sector index {} does not exist", sector_idx),
            })?;

        let offset = (sector.address - self.base_address) as usize;
        let len = sector.size as usize;
        self.data[offset..offset + len].fill(0xFF);
        Ok(())
    }

    /// Erases all sectors overlapping the given address range, resetting their bytes to 0xFF.
    /// Returns the list of sector indices that were erased.
    pub fn erase_range(&mut self, start: u32, length: u32) -> Result<Vec<u32>, FlashError> {
        self.check_bounds(start, length)?;
        if length == 0 {
            return Ok(Vec::new());
        }

        let mut erased_indices = Vec::new();
        for sector in &self.sectors {
            if sector.overlaps(start, length) {
                let offset = (sector.address - self.base_address) as usize;
                let len = sector.size as usize;
                self.data[offset..offset + len].fill(0xFF);
                erased_indices.push(sector.index);
            }
        }

        Ok(erased_indices)
    }

    /// Programs bytes into NOR flash memory at the given address.
    /// In strict NOR mode, writing a bit of '1' where current memory has '0' is rejected
    /// with `NorFlashWriteViolation`. Permissible writes transition bits 1 -> 0.
    pub fn write_bytes(&mut self, address: u32, bytes: &[u8]) -> Result<(), FlashError> {
        self.check_bounds(address, bytes.len() as u32)?;
        if bytes.is_empty() {
            return Ok(());
        }

        let offset = (address - self.base_address) as usize;

        for (i, &attempted) in bytes.iter().enumerate() {
            let current = self.data[offset + i];
            if self.strict_nor_mode {
                // If attempting to write 1 where current is 0: (current & attempted) != attempted
                if (current & attempted) != attempted {
                    return Err(FlashError::NorFlashWriteViolation {
                        address: address + i as u32,
                        attempted,
                        current,
                    });
                }
            }
            // Physical NOR write clears bits (AND operation)
            self.data[offset + i] = current & attempted;
        }

        Ok(())
    }

    /// Reads raw bytes from memory at the specified address.
    pub fn read_bytes(&self, address: u32, length: u32) -> Result<Vec<u8>, FlashError> {
        self.check_bounds(address, length)?;
        if length == 0 {
            return Ok(Vec::new());
        }

        let offset = (address - self.base_address) as usize;
        let len = length as usize;
        Ok(self.data[offset..offset + len].to_vec())
    }

    /// Checks whether all bytes in the given range are erased (equal to 0xFF).
    pub fn is_erased(&self, address: u32, length: u32) -> Result<bool, FlashError> {
        self.check_bounds(address, length)?;
        if length == 0 {
            return Ok(true);
        }

        let offset = (address - self.base_address) as usize;
        let len = length as usize;
        Ok(self.data[offset..offset + len].iter().all(|&b| b == 0xFF))
    }
}
