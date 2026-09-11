use crate::types::{SectorInfo, TargetInfo};

/// Returns target geometry for STM32F103C8 (Medium-density, 64 KB flash, 1 KB uniform sectors).
pub fn stm32f103c8() -> TargetInfo {
    let mut sectors = Vec::with_capacity(64);
    let flash_base = 0x0800_0000;
    let sector_size = 1024; // 1 KB

    for i in 0..64 {
        sectors.push(SectorInfo {
            index: i,
            address: flash_base + i * sector_size,
            size: sector_size,
        });
    }

    TargetInfo {
        name: "stm32f103c8".to_string(),
        display_name: None,
        architecture: "ARMv7-M".to_string(),
        flash_base,
        flash_size: 64 * 1024,
        ram_base: 0x2000_0000,
        ram_size: 20 * 1024,
        page_size: 1024,
        sectors,
    }
}

/// Returns target geometry for STM32F103RB (Medium-density, 128 KB flash, 1 KB uniform sectors).
pub fn stm32f103rb() -> TargetInfo {
    let mut sectors = Vec::with_capacity(128);
    let flash_base = 0x0800_0000;
    let sector_size = 1024; // 1 KB

    for i in 0..128 {
        sectors.push(SectorInfo {
            index: i,
            address: flash_base + i * sector_size,
            size: sector_size,
        });
    }

    TargetInfo {
        name: "stm32f103rb".to_string(),
        display_name: None,
        architecture: "ARMv7-M".to_string(),
        flash_base,
        flash_size: 128 * 1024,
        ram_base: 0x2000_0000,
        ram_size: 20 * 1024,
        page_size: 1024,
        sectors,
    }
}

/// Returns target geometry for STM32F401RE (512 KB flash, asymmetric sectors: 4x16KB, 1x64KB, 3x128KB).
pub fn stm32f401re() -> TargetInfo {
    let flash_base = 0x0800_0000;
    let sector_layouts: [(u32, u32); 8] = [
        (flash_base, 16 * 1024),               // Sector 0: 16 KB
        (flash_base + 0x4000, 16 * 1024),       // Sector 1: 16 KB
        (flash_base + 0x8000, 16 * 1024),       // Sector 2: 16 KB
        (flash_base + 0xC000, 16 * 1024),       // Sector 3: 16 KB
        (flash_base + 0x10000, 64 * 1024),      // Sector 4: 64 KB
        (flash_base + 0x20000, 128 * 1024),     // Sector 5: 128 KB
        (flash_base + 0x40000, 128 * 1024),     // Sector 6: 128 KB
        (flash_base + 0x60000, 128 * 1024),     // Sector 7: 128 KB
    ];

    let sectors = sector_layouts
        .into_iter()
        .enumerate()
        .map(|(i, (addr, size))| SectorInfo {
            index: i as u32,
            address: addr,
            size,
        })
        .collect();

    TargetInfo {
        name: "stm32f401re".to_string(),
        display_name: None,
        architecture: "ARMv7E-M".to_string(),
        flash_base,
        flash_size: 512 * 1024,
        ram_base: 0x2000_0000,
        ram_size: 96 * 1024,
        page_size: 256,
        sectors,
    }
}

/// Returns target geometry for STM32F411CE (512 KB flash, asymmetric sectors: 4x16KB, 1x64KB, 3x128KB).
pub fn stm32f411ce() -> TargetInfo {
    let mut target = stm32f401re();
    target.name = "stm32f411ce".to_string();
    target.ram_size = 128 * 1024;
    target
}

/// Returns target geometry for STM32H753ZI (2048 KB flash, 16x128 KB uniform sectors, 1024 KB RAM).
pub fn stm32h753zi() -> TargetInfo {
    let flash_base = 0x0800_0000;
    let sector_size = 128 * 1024; // 128 KB sectors (8 sectors per bank x 2 banks = 16 sectors)
    let mut sectors = Vec::with_capacity(16);

    for i in 0..16 {
        sectors.push(SectorInfo {
            index: i,
            address: flash_base + i * sector_size,
            size: sector_size,
        });
    }

    TargetInfo {
        name: "STM32H753ZI".to_string(),
        display_name: None,
        architecture: "ARMv7E-M (Cortex-M7)".to_string(),
        flash_base,
        flash_size: 2048 * 1024,
        ram_base: 0x2000_0000,
        ram_size: 1024 * 1024,
        page_size: 256,
        sectors,
    }
}

/// Returns a generic Cortex-M target geometry (1 MB flash, 256 x 4 KB uniform sectors).
pub fn generic_cortex_m() -> TargetInfo {
    let mut sectors = Vec::with_capacity(256);
    let flash_base = 0x0800_0000;
    let sector_size = 4096; // 4 KB

    for i in 0..256 {
        sectors.push(SectorInfo {
            index: i,
            address: flash_base + i * sector_size,
            size: sector_size,
        });
    }

    TargetInfo {
        name: "generic-cortex-m".to_string(),
        display_name: None,
        architecture: "ARMv7-M".to_string(),
        flash_base,
        flash_size: 1024 * 1024,
        ram_base: 0x2000_0000,
        ram_size: 128 * 1024,
        page_size: 1024,
        sectors,
    }
}

/// Normalizes and resolves board or alias names into canonical probe-rs target chip names.
pub fn resolve_target_alias(input: &str) -> Option<&'static str> {
    let clean = input.trim().to_lowercase().replace(['-', '_'], "");
    match clean.as_str() {
        "nucleoh753zi" | "nucleoh753" | "stm32h753zi" | "stm32h753" | "h753zi" | "h753" => {
            Some("STM32H753ZI")
        }
        "nucleoh743zi" | "nucleoh743" | "stm32h743zi" | "stm32h743" | "h743zi" | "h743" => {
            Some("STM32H743ZI")
        }
        "stm32h750vbtx" | "stm32h750" | "h750" => Some("STM32H750VBTx"),
        "nucleof401re" | "nucleof401" | "stm32f401re" | "stm32f401" | "f401re" | "f401" => {
            Some("STM32F401RE")
        }
        "nucleof411re" | "nucleof411" | "stm32f411re" | "stm32f411" | "f411re" | "f411" => {
            Some("STM32F411RE")
        }
        "nucleof446re" | "nucleof446" | "stm32f446re" | "stm32f446" | "f446re" | "f446" => {
            Some("STM32F446RE")
        }
        "nucleof429zi" | "nucleof429" | "stm32f429zi" | "stm32f429" | "f429zi" | "f429" => {
            Some("STM32F429ZI")
        }
        "stm32f407vg" | "stm32f407" | "discoveryf407" | "f407vg" | "f407" => Some("STM32F407VG"),
        "nucleof103rb" | "nucleof103" | "stm32f103rb" | "stm32f103" | "bluepill"
        | "stm32f103c8" | "f103c8" => Some("STM32F103C8"),
        "nucleog071rb" | "stm32g071rb" | "stm32g0" => Some("STM32G071RB"),
        "nucleog474re" | "stm32g474re" | "stm32g4" => Some("STM32G474RE"),
        "nucleol476rg" | "stm32l476rg" | "stm32l4" => Some("STM32L476RG"),
        "nucleou575zi" | "nucleou575ziq" | "stm32u575zi" | "stm32u575" | "u575zi" | "u575" => {
            Some("STM32U575ZITx")
        }
        "nucleou585zi" | "stm32u585ai" | "stm32u585" | "u585" => Some("STM32U585AIIx"),
        "nucleoh563zi" | "stm32h563zi" | "stm32h563" | "h563zi" | "h563" => Some("STM32H563ZITx"),
        "nucleol552ze" | "stm32l552ze" | "stm32l552" | "l552" => Some("STM32L552ZETx"),
        "nucleowb55rg" | "stm32wb55rg" | "stm32wb55" | "wb55" => Some("STM32WB55RGVx"),
        "nucleowl55jc" | "stm32wl55jc" | "stm32wl55" | "wl55" => Some("STM32WL55JCIx"),
        "stm32c031c6" | "stm32c031" | "c031" => Some("STM32C031C6Tx"),
        "rp2040" | "pico" | "picow" | "raspberrypipico" => Some("RP2040"),
        _ => None,
    }
}

/// Resolves target geometry from a case-insensitive target name query.
pub fn get_target_by_name(name: &str) -> Option<TargetInfo> {
    let normalized = name.to_lowercase().replace(['-', '_'], "");
    if normalized.contains("h7") {
        Some(stm32h753zi())
    } else if normalized.contains("f103c8") {
        Some(stm32f103c8())
    } else if normalized.contains("f103") {
        Some(stm32f103rb())
    } else if normalized.contains("f411") {
        Some(stm32f411ce())
    } else if normalized.contains("f4") {
        Some(stm32f401re())
    } else if normalized.contains("cortex") || normalized.contains("generic") {
        Some(generic_cortex_m())
    } else {
        // Default to STM32H753ZI if unknown target name contains h7, else STM32F401RE
        Some(stm32f401re())
    }
}
