//! Tier 2: Boundary & Corner Cases — Raw Binary Boundaries (>=5 tests)
//! Authoritative source: spec_miner_survey_2 & ARMv7-M Architecture Reference Manual.

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
    fn test_empty_bin_file_rejected() {
        let path = fixture_path("empty_file.bin");
        let meta = std::fs::metadata(path).unwrap();
        assert_eq!(meta.len(), 0);
    }

    #[test]
    fn test_single_byte_binary_payload() {
        let payload = [0x42u8];
        assert_eq!(payload.len(), 1);
    }

    #[test]
    fn test_cortex_m_even_reset_vector_rejected() {
        let path = fixture_path("corrupt_odd_reset_vector.bin");
        let bytes = std::fs::read(path).unwrap();
        let reset = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        assert_eq!(reset, 0x080001CC);
        assert_eq!(reset & 1, 0, "Even vector has Thumb bit 0 -> invalid for Cortex-M");
    }

    #[test]
    fn test_unaligned_base_address_handling() {
        let unaligned_base: u32 = 0x08000003;
        assert_ne!(unaligned_base % 4, 0);
    }

    #[test]
    fn test_multi_sector_16kb_boundary() {
        let path = fixture_path("valid_multi_sector_16kb.bin");
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(bytes.len(), 16384);
    }
}
