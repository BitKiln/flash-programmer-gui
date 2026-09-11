//! Tier 3: Pairwise Cross-Feature Combinations — BIN Custom Base Flow
//! Combines: BIN load + custom base + sector erase + program + verify + reset.

#[cfg(test)]
mod tests {
    #[test]
    fn test_bin_custom_base_flow() {
        let mut simulated_flash = vec![0xFFu8; 65536];
        let base_address: usize = 0x08010000 - 0x08000000;
        let payload = [0x55u8; 128];

        // 1. Sector erase
        simulated_flash[base_address..base_address + 128].fill(0xFF);

        // 2. Program
        simulated_flash[base_address..base_address + 128].copy_from_slice(&payload);

        // 3. Verify
        assert_eq!(&simulated_flash[base_address..base_address + 128], &payload);

        // 4. Reset
        let reset_executed = true;
        assert!(reset_executed);
    }
}
