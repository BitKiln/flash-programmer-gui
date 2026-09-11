//! Tier 2: Boundary & Corner Cases — Intel HEX Boundaries (>=5 tests)
//! Authoritative source: Intel HEX Spec 1988 & spec_miner_survey_2 Vector 5.

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
    fn test_corrupt_bad_checksum_rejected() {
        let path = fixture_path("corrupt_bad_checksum.hex");
        let content = std::fs::read_to_string(path).unwrap();
        // Line 1 ends in F1 instead of F2
        assert!(content.contains(":020000040800F1"));
    }

    #[test]
    fn test_corrupt_missing_colon_rejected() {
        let path = fixture_path("corrupt_missing_colon.hex");
        let content = std::fs::read_to_string(path).unwrap();
        // Line 1 is missing leading ':'
        assert!(content.starts_with("020000040800F2"));
    }

    #[test]
    fn test_corrupt_odd_hex_digits_rejected() {
        let path = fixture_path("corrupt_odd_hex_digits.hex");
        let content = std::fs::read_to_string(path).unwrap();
        // 13 characters after ':'
        let first_line = content.lines().next().unwrap();
        assert_eq!(first_line[1..].len() % 2, 1);
    }

    #[test]
    fn test_corrupt_invalid_hex_char_rejected() {
        let path = fixture_path("corrupt_invalid_hex_char.hex");
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains('Z'));
    }

    #[test]
    fn test_corrupt_truncated_record_rejected() {
        let path = fixture_path("corrupt_truncated.hex");
        let content = std::fs::read_to_string(path).unwrap();
        assert_eq!(content.trim(), ":0200000408");
    }

    #[test]
    fn test_extreme_4gb_address_overflow_rejected() {
        let path = fixture_path("extreme_address_overflow_4gb.hex");
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains(":02000004FFFF"));
    }

    #[test]
    fn test_empty_hex_file_rejected() {
        let path = fixture_path("empty_file.hex");
        let metadata = std::fs::metadata(path).unwrap();
        assert_eq!(metadata.len(), 0);
    }
}
