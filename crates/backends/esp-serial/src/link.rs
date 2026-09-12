//! The seam between this backend's logic and the wire.
//!
//! Everything interesting — address validation, chunking, cancellation,
//! progress accounting, verification — lives above [`EspLink`] and is therefore
//! testable against [`crate::fake::FakeLink`] with no board attached. Below it
//! sits a thin adapter over `espflash`, which is the only part that needs
//! hardware to exercise and the only part an `espflash` upgrade should touch.

use flash_core::error::FlashError;

/// The ESP flash erase sector. Erase commands must be aligned to it, and a
/// partial erase silently takes the whole sector with it.
pub const SECTOR_SIZE: u32 = 4096;

/// What the ROM bootloader reports about the chip it is running on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EspDeviceInfo {
    /// Chip name as the bootloader reports it, e.g. "esp32s3".
    pub chip: String,
    /// Detected flash size in bytes. Zero when the chip would not say.
    pub flash_size: u32,
    /// Silicon revision, for the log.
    pub revision: Option<String>,
}

/// One open connection to an ESP ROM (or stub) bootloader.
///
/// Addresses are **flash offsets**, not the memory-mapped view: offset 0 is the
/// start of flash, which is where the second-stage bootloader lives on most
/// parts. This mirrors what `esptool.py` and every ESP-IDF build script use.
pub trait EspLink: Send {
    fn device_info(&mut self) -> Result<EspDeviceInfo, FlashError>;

    /// Erase the whole chip. One bootloader command with no poll point, so it
    /// cannot be interrupted once started.
    fn erase_all(&mut self) -> Result<(), FlashError>;

    /// Erase `size` bytes at `offset`. Both must be sector-aligned; the caller
    /// checks that before calling.
    fn erase_region(&mut self, offset: u32, size: u32) -> Result<(), FlashError>;

    /// Write `data` at `offset`, reporting bytes completed through
    /// `progress`.
    ///
    /// The whole segment goes in one call rather than being chunked by the
    /// caller, because the ESP flash protocol ends a write by rebooting the
    /// chip: a second write would find no bootloader listening. Progress
    /// therefore comes from inside the write, and the call runs to completion
    /// once started.
    fn write(
        &mut self,
        offset: u32,
        data: &[u8],
        progress: &mut dyn FnMut(usize),
    ) -> Result<(), FlashError>;

    /// Re-enter download mode after an operation that left it.
    ///
    /// Writing ends with a reboot, so anything afterwards -- reading back,
    /// asking for a checksum -- needs the bootloader brought up again.
    fn resync(&mut self) -> Result<(), FlashError>;

    fn read(&mut self, offset: u32, size: u32) -> Result<Vec<u8>, FlashError>;

    /// MD5 of a flash region, computed by the chip. Far cheaper than reading
    /// the region back over serial just to compare it.
    fn md5(&mut self, offset: u32, size: u32) -> Result<[u8; 16], FlashError>;

    /// Reset out of the bootloader into the application.
    fn reset(&mut self) -> Result<(), FlashError>;

    fn close(&mut self) -> Result<(), FlashError>;
}

/// The sector-aligned span that covers `size` bytes at `offset`.
///
/// The **offset** has to sit on a sector boundary: starting mid-sector would
/// erase the bytes in front of it, which the caller did not ask for and cannot
/// get back.
///
/// The **length** is rounded up instead of rejected. A sector is the smallest
/// thing the chip can erase, so covering a region always means erasing to the
/// end of its last sector -- and demanding a multiple of 4096 would make any
/// firmware whose size is not an exact multiple unflashable, which is nearly
/// all of them. The rounding only ever touches the tail of the last sector the
/// region already occupies.
pub fn erase_span(offset: u32, size: u32) -> Result<(u32, u32), FlashError> {
    if !offset.is_multiple_of(SECTOR_SIZE) {
        return Err(FlashError::InvalidAddress {
            address: offset,
            reason: format!(
                "an erase must start on a {SECTOR_SIZE}-byte flash sector boundary \
                 (got 0x{offset:X}); starting mid-sector would erase the bytes in \
                 front of it too"
            ),
        });
    }

    let sectors = size.div_ceil(SECTOR_SIZE);
    let aligned = sectors.checked_mul(SECTOR_SIZE).ok_or_else(|| {
        FlashError::InvalidAddress {
            address: offset,
            reason: format!("0x{size:X} bytes rounds past the end of the address space"),
        }
    })?;
    offset.checked_add(aligned).ok_or_else(|| FlashError::InvalidAddress {
        address: offset,
        reason: format!("0x{aligned:X} bytes from 0x{offset:X} runs past the end of flash"),
    })?;

    Ok((offset, aligned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offset_must_sit_on_a_sector_boundary() {
        assert!(erase_span(0, SECTOR_SIZE).is_ok());
        assert!(erase_span(SECTOR_SIZE, SECTOR_SIZE * 3).is_ok());
        assert!(erase_span(0x800, 0x1000).is_err());
    }

    #[test]
    fn a_length_is_rounded_up_to_the_sector_rather_than_refused() {
        // A real firmware image is not a multiple of the sector size, and
        // refusing one would leave it unflashable.
        assert_eq!(erase_span(0x1000, 0x1B5250).unwrap(), (0x1000, 0x1B6000));
        assert_eq!(erase_span(0, 1).unwrap(), (0, SECTOR_SIZE));
        assert_eq!(erase_span(0, 0).unwrap(), (0, 0));
    }
}
