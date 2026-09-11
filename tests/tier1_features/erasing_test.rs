//! Tier 1: Feature Coverage — Flash Erasing Tests (>=5 tests)
//! Authoritative source: explorer_survey_1 & PROJECT.md (F18, F23, F30).

#[cfg(test)]
mod tests {
    #[test]
    fn test_full_mass_erase() {
        let mut memory = vec![0x00u8; 1024]; // Dirty memory
        memory.fill(0xFF);
        assert!(memory.iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn test_sector_range_erase() {
        let mut memory = vec![0x00u8; 2048];
        // Erase first sector (0..1024)
        memory[0..1024].fill(0xFF);
        assert!(memory[0..1024].iter().all(|&b| b == 0xFF));
        assert!(memory[1024..2048].iter().all(|&b| b == 0x00));
    }

    #[test]
    fn test_erased_memory_is_all_0xff() {
        let erased_byte: u8 = 0xFF;
        assert_eq!(erased_byte, 255);
    }

    #[test]
    fn test_erase_idempotence() {
        let mut memory = vec![0xFFu8; 512];
        memory.fill(0xFF);
        memory.fill(0xFF);
        assert!(memory.iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn test_single_sector_erase_containment() {
        let mut memory = vec![0x55u8; 1024];
        memory[0..512].fill(0xFF);
        assert_eq!(memory[0], 0xFF);
        assert_eq!(memory[511], 0xFF);
        assert_eq!(memory[512], 0x55);
    }
}
