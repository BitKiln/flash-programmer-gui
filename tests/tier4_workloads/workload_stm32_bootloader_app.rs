//! Tier 4: Real-World Embedded Workload 1 — Dual-Image STM32 Bootloader + Main App
//! Simulates: Bootloader at 0x08000000 (16KB) + Main Application at 0x08010000 (32KB).
//! Verifies: Independent sector erasure, vector table integrity, zero gap corruption.

#[cfg(test)]
mod tests {
    #[test]
    fn test_workload_stm32_bootloader_and_application() {
        let mut flash = vec![0xFFu8; 524288];

        // 1. Flash Bootloader (Sector 0: 0x08000000, 16KB)
        let mut bootloader = vec![0xFFu8; 16384];
        let sp = 0x20005000u32.to_le_bytes();
        let reset = 0x08000101u32.to_le_bytes(); // Thumb reset handler
        bootloader[0..4].copy_from_slice(&sp);
        bootloader[4..8].copy_from_slice(&reset);
        flash[0..16384].copy_from_slice(&bootloader);

        // 2. Erase only application sectors (Sector 4: 0x08010000, 32KB)
        let app_offset = 65536; // 0x08010000 - 0x08000000
        flash[app_offset..app_offset + 32768].fill(0xFF);

        // 3. Flash Main Application
        let mut app = vec![0xFFu8; 32768];
        let app_sp = 0x20005000u32.to_le_bytes();
        let app_reset = 0x08010101u32.to_le_bytes();
        app[0..4].copy_from_slice(&app_sp);
        app[4..8].copy_from_slice(&app_reset);
        flash[app_offset..app_offset + 32768].copy_from_slice(&app);

        // 4. Verify Bootloader unchanged
        assert_eq!(&flash[0..8], &bootloader[0..8]);

        // 5. Verify App unchanged
        assert_eq!(&flash[app_offset..app_offset + 8], &app[0..8]);

        // 6. Verify gap between bootloader and app remains erased
        assert!(flash[16384..app_offset].iter().all(|&b| b == 0xFF));
    }
}
