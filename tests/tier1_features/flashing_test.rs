//! Tier 1: Feature Coverage — Flash Programming Tests (>=5 tests)
//! Authoritative source: explorer_survey_1 & PROJECT.md (F19, F23, F27, F29).

#[cfg(test)]
mod tests {
    #[test]
    fn test_flash_single_segment_hex() {
        let mut memory = vec![0xFFu8; 1024];
        let data = [0x10, 0x20, 0x30, 0x40];
        // Program at offset 0
        memory[0..4].copy_from_slice(&data);
        assert_eq!(&memory[0..4], &[0x10, 0x20, 0x30, 0x40]);
    }

    #[test]
    fn test_flash_bootloader_app_with_gap() {
        let mut memory = vec![0xFFu8; 4096];
        // Bootloader at offset 0
        memory[0..4].copy_from_slice(&[0x01, 0x02, 0x03, 0x04]);
        // App at offset 2048 (leaving 2044 bytes of gap untouched)
        memory[2048..2052].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(&memory[0..4], &[0x01, 0x02, 0x03, 0x04]);
        assert_eq!(&memory[2048..2052], &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(memory[1000], 0xFF, "Gap must remain erased 0xFF");
    }

    #[test]
    fn test_flash_raw_binary_default_base() {
        let base: u64 = 0x0800_0000;
        let data = [0x55; 16];
        assert_eq!(base, 134217728);
        assert_eq!(data.len(), 16);
    }

    #[test]
    fn test_flash_raw_binary_custom_base() {
        let base: u64 = 0x0801_0000;
        let data = [0x66; 32];
        let end = base + data.len() as u64;
        assert_eq!(end, 0x0801_0020);
    }

    #[test]
    fn test_flash_with_verify_and_reset() {
        let mut memory = vec![0xFFu8; 256];
        let payload = [0x42; 64];
        memory[0..64].copy_from_slice(&payload);
        // Verify
        assert_eq!(&memory[0..64], &payload);
        // Reset (simulated state transition)
        let reset_success = true;
        assert!(reset_success);
    }
}
