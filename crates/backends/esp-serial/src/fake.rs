//! An in-memory ESP bootloader, so the serial paths run in CI.
//!
//! This is the ESP equivalent of the mock probe backend: real NOR physics
//! (erased cells read `0xFF`, a write can only clear bits), real sector
//! geometry, and the same refusals the silicon makes. Without it the whole
//! backend would be untestable without a board on the desk, and would quietly
//! rot.

use std::sync::{Arc, Mutex};

use flash_core::error::FlashError;
use md5::{Digest, Md5};

use crate::link::{EspDeviceInfo, EspLink, SECTOR_SIZE};

/// Flash contents shared between a link and whatever is inspecting it, so a
/// test can assert on what actually landed on the "chip".
pub type FakeFlash = Arc<Mutex<Vec<u8>>>;

/// A simulated ESP32 ROM bootloader over a simulated serial link.
#[derive(Debug)]
pub struct FakeLink {
    chip: String,
    flash: FakeFlash,
    /// Set once `reset` has been called, so a test can assert the target was
    /// actually let out of the bootloader.
    pub reset_count: u32,
    /// How many times the session has had to bring the bootloader back up.
    pub resync_count: u32,
    pub closed: bool,
    /// When set, the next command of this kind fails, for error-path tests.
    pub fail_next_write: bool,
}

impl FakeLink {
    /// A 4 MB part, the common ESP32 devkit configuration.
    pub fn new(chip: impl Into<String>) -> Self {
        Self::with_flash_size(chip, 4 * 1024 * 1024)
    }

    pub fn with_flash_size(chip: impl Into<String>, flash_size: u32) -> Self {
        Self {
            chip: chip.into(),
            flash: Arc::new(Mutex::new(vec![0xFF; flash_size as usize])),
            reset_count: 0,
            resync_count: 0,
            closed: false,
            fail_next_write: false,
        }
    }

    /// A handle on the simulated flash, for assertions.
    pub fn flash(&self) -> FakeFlash {
        Arc::clone(&self.flash)
    }

    fn bounds(&self, offset: u32, size: u32) -> Result<(usize, usize), FlashError> {
        let flash = self.flash.lock().expect("fake flash lock");
        let end = offset.checked_add(size).ok_or(FlashError::InvalidAddress {
            address: offset,
            reason: "offset + length overflows".to_string(),
        })?;
        if end as usize > flash.len() {
            return Err(FlashError::AddressOutOfBounds {
                address: offset,
                base: 0,
                size: flash.len() as u32,
            });
        }
        Ok((offset as usize, end as usize))
    }
}

impl EspLink for FakeLink {
    fn device_info(&mut self) -> Result<EspDeviceInfo, FlashError> {
        Ok(EspDeviceInfo {
            chip: self.chip.clone(),
            flash_size: self.flash.lock().expect("fake flash lock").len() as u32,
            revision: Some("v0.0 (simulated)".to_string()),
        })
    }

    fn erase_all(&mut self) -> Result<(), FlashError> {
        self.flash.lock().expect("fake flash lock").fill(0xFF);
        Ok(())
    }

    fn erase_region(&mut self, offset: u32, size: u32) -> Result<(), FlashError> {
        let (start, end) = self.bounds(offset, size)?;
        self.flash.lock().expect("fake flash lock")[start..end].fill(0xFF);
        Ok(())
    }

    fn write(
        &mut self,
        offset: u32,
        data: &[u8],
        progress: &mut dyn FnMut(usize),
    ) -> Result<(), FlashError> {
        if self.fail_next_write {
            self.fail_next_write = false;
            return Err(FlashError::ProgramError(
                "injected write failure".to_string(),
            ));
        }
        let (start, _) = self.bounds(offset, data.len() as u32)?;
        let mut flash = self.flash.lock().expect("fake flash lock");
        for (i, byte) in data.iter().enumerate() {
            let current = flash[start + i];
            // NOR flash: a write clears bits, it cannot set them. Silicon does
            // this silently; refusing loudly is what catches a missing erase.
            if byte & !current != 0 {
                return Err(FlashError::NorFlashWriteViolation {
                    address: offset + i as u32,
                    attempted: *byte,
                    current,
                });
            }
            flash[start + i] = *byte;
        }
        // Real hardware reports progress as the write runs; reporting once at
        // the end is enough to keep the accounting honest here.
        progress(data.len());
        Ok(())
    }

    /// A real link reconnects here. Nothing to do in memory, but the count
    /// lets a test assert the session resyncs after a write.
    fn resync(&mut self) -> Result<(), FlashError> {
        self.resync_count += 1;
        Ok(())
    }

    fn read(&mut self, offset: u32, size: u32) -> Result<Vec<u8>, FlashError> {
        let (start, end) = self.bounds(offset, size)?;
        Ok(self.flash.lock().expect("fake flash lock")[start..end].to_vec())
    }

    fn md5(&mut self, offset: u32, size: u32) -> Result<[u8; 16], FlashError> {
        let data = self.read(offset, size)?;
        let mut hasher = Md5::new();
        hasher.update(&data);
        Ok(hasher.finalize().into())
    }

    fn reset(&mut self) -> Result<(), FlashError> {
        self.reset_count += 1;
        Ok(())
    }

    fn close(&mut self) -> Result<(), FlashError> {
        self.closed = true;
        Ok(())
    }
}

/// Sector count for a flash of this size.
pub fn sector_count(flash_size: u32) -> u32 {
    flash_size / SECTOR_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_chip_reads_erased() {
        let mut link = FakeLink::new("esp32s3");
        assert!(link.read(0, 64).unwrap().iter().all(|b| *b == 0xFF));
    }

    #[test]
    fn a_write_cannot_set_bits_back() {
        let mut link = FakeLink::new("esp32s3");
        link.write(0, &[0x0F], &mut |_| {}).unwrap();
        // 0x0F -> 0xFF needs bits to go 0 to 1, which only an erase can do.
        let err = link.write(0, &[0xFF], &mut |_| {}).unwrap_err();
        assert!(matches!(err, FlashError::NorFlashWriteViolation { .. }));
        link.erase_region(0, SECTOR_SIZE).unwrap();
        link.write(0, &[0xFF], &mut |_| {}).unwrap();
    }

    #[test]
    fn reads_past_the_end_are_refused() {
        let mut link = FakeLink::with_flash_size("esp32c6", 2 * 1024 * 1024);
        assert!(link.read(2 * 1024 * 1024 - 4, 8).is_err());
    }
}
