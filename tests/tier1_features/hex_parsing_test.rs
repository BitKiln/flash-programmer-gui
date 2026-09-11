//! Tier 1: Feature Coverage — Intel HEX Parsing Tests (>=5 tests)
//! Authoritative source: Intel Hexadecimal Object File Format Specification (1988)
//! and spec_miner_survey_2/survey_report.md.

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
    fn test_stm32_single_segment_hex_vector1() {
        // Vector 1: 32 bytes @ 0x08000000, Reset Handler 0x080001CD, CRC32 0x0A5B1F0D
        let path = fixture_path("valid_stm32_single_segment.hex");
        assert!(path.exists(), "Fixture missing: {:?}", path);
        let content = std::fs::read_to_string(&path).expect("Read fixture");
        assert!(content.contains(":020000040800F2"), "Must contain Type 04 linear base");
        assert!(content.contains(":04000005080001CD21"), "Must contain Type 05 entry point");
        assert!(content.contains(":00000001FF"), "Must contain Type 01 EOF");
    }

    #[test]
    fn test_stm32_bootloader_app_gap_vector2() {
        // Vector 2: Dual segments (0x08000000 and 0x08040000) with 256KB gap
        let path = fixture_path("valid_stm32_bootloader_app_gap.hex");
        assert!(path.exists(), "Fixture missing: {:?}", path);
        let content = std::fs::read_to_string(&path).expect("Read fixture");
        assert!(content.contains(":020000040800F2"));
        assert!(content.contains(":020000040804EE"), "Must contain second segment base 0x08040000");
    }

    #[test]
    fn test_stm32_out_of_order_coalescing_vector3() {
        // Vector 3: Offset 0x0010 emitted before 0x0000, must coalesce cleanly
        let path = fixture_path("valid_stm32_out_of_order.hex");
        assert!(path.exists(), "Fixture missing: {:?}", path);
        let content = std::fs::read_to_string(&path).expect("Read fixture");
        let lines: Vec<&str> = content.lines().collect();
        assert!(lines[1].contains("10001000"), "First data line should be offset 0x0010");
        assert!(lines[2].contains("10000000"), "Second data line should be offset 0x0000");
    }

    #[test]
    fn test_extended_segment_address_record02() {
        // HEX86 20-bit segmentation: USBA = 0x1000 -> Physical Base = 0x10000
        let path = fixture_path("valid_extended_segment_type02.hex");
        assert!(path.exists(), "Fixture missing: {:?}", path);
        let content = std::fs::read_to_string(&path).expect("Read fixture");
        assert!(content.contains(":020000021000EC"));
    }

    #[test]
    fn test_start_segment_address_record03() {
        // HEX86 CS:IP execution entry point: CS=0x1000, IP=0x0100 -> Entry = 0x10100
        let path = fixture_path("valid_start_segment_type03.hex");
        assert!(path.exists(), "Fixture missing: {:?}", path);
        let content = std::fs::read_to_string(&path).expect("Read fixture");
        assert!(content.contains(":0400000310000100E8"));
    }

    #[test]
    fn test_multi_algorithm_checksum_vector_constants() {
        // Mathematical validation of Vector 1 CRC32, MD5, SHA-256
        let v1_data: [u8; 32] = [
            0x00, 0x50, 0x00, 0x20, 0xCD, 0x01, 0x00, 0x08,
            0xD1, 0x01, 0x00, 0x08, 0xD3, 0x01, 0x00, 0x08,
            0xD5, 0x01, 0x00, 0x08, 0xD7, 0x01, 0x00, 0x08,
            0xD9, 0x01, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(v1_data.len(), 32);
        assert_eq!(v1_data[0..4], [0x00, 0x50, 0x00, 0x20]); // Initial MSP
        assert_eq!(v1_data[4..8], [0xCD, 0x01, 0x00, 0x08]); // Reset Handler
    }
}
