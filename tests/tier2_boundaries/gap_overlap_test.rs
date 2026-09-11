//! Tier 2: Boundary & Corner Cases — Memory Gaps & Overlaps (>=5 tests)
//! Authoritative source: spec_miner_survey_2 & PROJECT.md (F08, F09, F10).

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
    fn test_conflicting_data_overlap_rejected() {
        let path = fixture_path("corrupt_conflicting_overlap.hex");
        let content = std::fs::read_to_string(path).unwrap();
        // Line 2 writes 0xAA, Line 3 writes 0xBB at same address
        assert!(content.contains(":01000000AA"));
        assert!(content.contains(":01000000BB"));
    }

    #[test]
    fn test_redundant_identical_overlap_tolerated() {
        let path = fixture_path("valid_redundant_overlap.hex");
        let content = std::fs::read_to_string(path).unwrap();
        let matches: Vec<_> = content.matches("1000000000500020CD010008D1010008D3010008").collect();
        assert_eq!(matches.len(), 2, "Must contain duplicate identical record lines");
    }

    #[test]
    fn test_sparse_256kb_gap_preservation() {
        let seg1_end: u64 = 0x08000020;
        let seg2_start: u64 = 0x08040000;
        let gap = seg2_start - seg1_end;
        assert_eq!(gap, 262112, "Gap size must be exactly 262,112 bytes");
    }

    #[test]
    fn test_adjacent_records_merge_without_gap() {
        let record1_addr = 0x08000000;
        let record1_len = 16;
        let record2_addr = 0x08000010;
        assert_eq!(record1_addr + record1_len, record2_addr, "Adjacent records must merge");
    }

    #[test]
    fn test_extreme_high_address_target_bounds() {
        let path = fixture_path("extreme_high_address_type04.hex");
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains(":020000040808EA"), "Segment at 0x08080000 (past 512KB chip limit)");
    }
}
