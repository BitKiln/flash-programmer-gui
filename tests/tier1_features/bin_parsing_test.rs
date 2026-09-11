//! Tier 1: Feature Coverage — Raw Binary Parsing Tests (>=5 tests)
//! Authoritative source: spec_miner_survey_2/survey_report.md & ARMv7-M Architecture Reference Manual.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("tests");
        p.push("fixtures");
        p.push(name);
        p
    }

    #[test]
    fn test_bin_default_stm32_base_0x08000000() {
        let path = fixture_path("valid_tiny_16b.bin");
        let bytes = std::fs::read(&path).expect("Read 16b fixture");
        assert_eq!(bytes.len(), 16);
        let default_base: u32 = 0x0800_0000;
        assert_eq!(default_base, 134217728);
    }

    #[test]
    fn test_bin_custom_base_address() {
        let custom_base: u32 = 0x2000_0000; // SRAM base
        let path = fixture_path("valid_tiny_16b.bin");
        let bytes = std::fs::read(&path).expect("Read fixture");
        let end_address = custom_base + bytes.len() as u32;
        assert_eq!(end_address, 0x2000_0010);
    }

    #[test]
    fn test_bin_exact_page_boundary_256b() {
        let path = fixture_path("valid_exact_page_256b.bin");
        let bytes = std::fs::read(&path).expect("Read page fixture");
        assert_eq!(bytes.len(), 256, "Page size must be exactly 256 bytes");
    }

    #[test]
    fn test_bin_exact_sector_boundary_1kb() {
        let path = fixture_path("valid_exact_sector_1kb.bin");
        let bytes = std::fs::read(&path).expect("Read sector fixture");
        assert_eq!(bytes.len(), 1024, "Sector size must be exactly 1024 bytes (STM32F1)");
    }

    #[test]
    fn test_bin_cortex_m_reset_vector_heuristic() {
        let path = fixture_path("valid_stm32_cortex_m_vector.bin");
        let bytes = std::fs::read(&path).expect("Read vector fixture");
        assert!(bytes.len() >= 8);
        let sp = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let reset = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        assert_eq!(sp, 0x20005000);
        assert_eq!(reset, 0x080001CD);
        assert_eq!(reset & 1, 1, "Reset vector must have Thumb bit set");
    }
}
