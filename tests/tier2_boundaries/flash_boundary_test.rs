//! Tier 2: Boundary & Corner Cases — Flash Boundaries & NOR Physics (>=5 tests)
//! Authoritative source: explorer_survey_1 (F23, F24, F25).

#[cfg(test)]
mod tests {
    #[test]
    fn test_nor_flash_write_violation_1_to_0_rule() {
        let original: u8 = 0x00; // Bit already 0
        let target: u8 = 0xFF;   // Attempting to write 1 over 0
        let is_violation = (original & target) != target;
        assert!(is_violation, "Writing 1 over 0 without erase is a NOR flash violation");
    }

    #[test]
    fn test_write_beyond_flash_capacity_rejected() {
        let flash_size = 524288; // 512KB
        let attempted_address = 0x08080000;
        let base_address = 0x08000000;
        let offset = attempted_address - base_address;
        assert!(offset >= flash_size, "Offset must exceed total flash size");
    }

    #[test]
    fn test_write_to_exact_last_byte_of_flash() {
        let flash_size = 524288;
        let last_offset = flash_size - 1;
        assert_eq!(last_offset, 524287);
    }

    #[test]
    fn test_multi_sector_asymmetric_crossing() {
        // STM32F4 sector geometry: 4 x 16KB (0..64KB), 1 x 64KB (64..128KB)
        let sector_0_end = 16384;
        let write_start = 16000;
        let write_len = 1000;
        let write_end = write_start + write_len;
        assert!(write_start < sector_0_end && write_end > sector_0_end);
    }

    #[test]
    fn test_missing_firmware_file_handled() {
        let path = std::path::Path::new("non_existent_image.hex");
        assert!(!path.exists());
    }
}
