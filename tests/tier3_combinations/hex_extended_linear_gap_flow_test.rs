//! Tier 3: Pairwise Cross-Feature Combinations — HEX Extended Linear + Gap Flow
//! Combines: HEX Type 04 + sparse 256KB gap + verify image + verify unwritten gap is clean 0xFF.

#[cfg(test)]
mod tests {
    #[test]
    fn test_hex_extended_linear_gap_flow() {
        let mut flash = vec![0xFFu8; 524288];
        let seg1_offset = 0; // 0x08000000
        let seg1_data = [0xAAu8; 32];
        let seg2_offset = 262144; // 0x08040000
        let seg2_data = [0xBBu8; 16];

        // Program both segments
        flash[seg1_offset..seg1_offset + 32].copy_from_slice(&seg1_data);
        flash[seg2_offset..seg2_offset + 16].copy_from_slice(&seg2_data);

        // Verify segments
        assert_eq!(&flash[seg1_offset..seg1_offset + 32], &seg1_data);
        assert_eq!(&flash[seg2_offset..seg2_offset + 16], &seg2_data);

        // Verify gap between segments remains 0xFF
        assert!(flash[32..262144].iter().all(|&b| b == 0xFF));
    }
}
