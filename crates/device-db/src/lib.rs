//! Device database.
//!
//! Everything vendor-specific that is not code lives here: the aliases people
//! actually type (`nucleo-h753zi`, `bluepill`), which silicon families exist,
//! and which backends can reach them. Both applications and every backend read
//! these tables, so adding a vendor is a data change rather than an edit to
//! detection logic, a GUI dropdown, and a CLI validator in three places.
//!
//! What is deliberately *not* here: chip geometry. Flash sizes, sector maps and
//! core types come from the probe-rs target registry (or from the target's own
//! ID registers), because a hardcoded part-number list goes stale the moment a
//! vendor ships a new package variant.

use serde::{Deserialize, Serialize};

/// Silicon vendor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vendor {
    StMicroelectronics,
    SiliconLabs,
    Espressif,
    RaspberryPi,
    Other,
}

impl Vendor {
    pub fn display_name(&self) -> &'static str {
        match self {
            Vendor::StMicroelectronics => "STMicroelectronics",
            Vendor::SiliconLabs => "Silicon Labs",
            Vendor::Espressif => "Espressif",
            Vendor::RaspberryPi => "Raspberry Pi",
            Vendor::Other => "Other",
        }
    }
}

/// A backend that can reach a family, named by its registry scheme.
///
/// These are the schemes from `flash_core::FlashBackend::scheme`, kept as
/// strings so this crate stays free of any dependency on the backends it
/// describes.
pub mod scheme {
    pub const PROBE_RS: &str = "probe";
    pub const MOCK: &str = "mock";
    pub const ESP_SERIAL: &str = "esp";
    pub const OPENOCD: &str = "openocd";
}

/// One silicon family and how it can be programmed.
///
/// Serialize only: the table is compile-time data made of `&'static str`, so
/// it can be handed to the frontend but never read back in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Family {
    /// Registry name prefix, e.g. `STM32U5`. Matching is case-insensitive.
    pub prefix: &'static str,
    pub vendor: Vendor,
    /// Human label for the GUI.
    pub display: &'static str,
    /// Backend schemes that can program this family, most preferred first.
    pub backends: &'static [&'static str],
    /// A representative part number, for the GUI's target suggestions.
    pub example: &'static str,
}

/// Every family the tool claims to support, with the backends that reach it.
///
/// Ordering is by vendor then family, which is also the order
/// `docs/supported-devices.md` renders.
pub const FAMILIES: &[Family] = &[
    // --- STMicroelectronics: Arm Cortex-M over SWD/JTAG ---
    fam("STM32C0", Vendor::StMicroelectronics, "STM32C0", "STM32C031C6Tx"),
    fam("STM32F0", Vendor::StMicroelectronics, "STM32F0", "STM32F030C8Tx"),
    fam("STM32F1", Vendor::StMicroelectronics, "STM32F1", "STM32F103C8"),
    fam("STM32F2", Vendor::StMicroelectronics, "STM32F2", "STM32F205RGTx"),
    fam("STM32F3", Vendor::StMicroelectronics, "STM32F3", "STM32F303RETx"),
    fam("STM32F4", Vendor::StMicroelectronics, "STM32F4", "STM32F401RE"),
    fam("STM32F7", Vendor::StMicroelectronics, "STM32F7", "STM32F746ZGTx"),
    fam("STM32G0", Vendor::StMicroelectronics, "STM32G0", "STM32G071RB"),
    fam("STM32G4", Vendor::StMicroelectronics, "STM32G4", "STM32G474RE"),
    fam("STM32H5", Vendor::StMicroelectronics, "STM32H5", "STM32H563ZITx"),
    fam("STM32H7", Vendor::StMicroelectronics, "STM32H7", "STM32H753ZI"),
    fam("STM32L0", Vendor::StMicroelectronics, "STM32L0", "STM32L053R8Tx"),
    fam("STM32L4", Vendor::StMicroelectronics, "STM32L4", "STM32L476RG"),
    fam("STM32L5", Vendor::StMicroelectronics, "STM32L5", "STM32L552ZETx"),
    fam("STM32U0", Vendor::StMicroelectronics, "STM32U0", "STM32U083RCTx"),
    fam("STM32U5", Vendor::StMicroelectronics, "STM32U5", "STM32U575ZITx"),
    fam("STM32WB", Vendor::StMicroelectronics, "STM32WB", "STM32WB55RGVx"),
    fam("STM32WL", Vendor::StMicroelectronics, "STM32WL", "STM32WL55JCIx"),
    // --- Silicon Labs: Arm Cortex-M over SWD, usually an onboard J-Link ---
    fam("EFM32", Vendor::SiliconLabs, "EFM32", "EFM32PG22C200F512IM40"),
    fam("EFR32BG", Vendor::SiliconLabs, "EFR32 Blue Gecko", "EFR32BG22C224F512IM40"),
    fam("EFR32FG", Vendor::SiliconLabs, "EFR32 Flex Gecko", "EFR32FG23B010F512IM48"),
    fam("EFR32MG", Vendor::SiliconLabs, "EFR32 Mighty Gecko", "EFR32MG24B210F1536IM48"),
    // --- Raspberry Pi ---
    fam("RP2040", Vendor::RaspberryPi, "RP2040", "RP2040"),
];

/// Shorthand for the common case: an Arm target reached by a debug probe.
const fn fam(
    prefix: &'static str,
    vendor: Vendor,
    display: &'static str,
    example: &'static str,
) -> Family {
    Family {
        prefix,
        vendor,
        display,
        backends: &[scheme::PROBE_RS],
        example,
    }
}

/// The family a target name belongs to, if the database knows it.
///
/// Longest prefix wins, so `STM32H7` is preferred over a hypothetical `STM32`.
pub fn family_of(target: &str) -> Option<&'static Family> {
    let upper = target.trim().to_uppercase();
    FAMILIES
        .iter()
        .filter(|f| upper.starts_with(f.prefix))
        .max_by_key(|f| f.prefix.len())
}

/// Whether the backend with this scheme can program `target`.
///
/// An unknown target is allowed through: the probe-rs registry carries far more
/// chips than this table names, and refusing them would make the table a
/// gatekeeper instead of a guide.
pub fn backend_supports(scheme_name: &str, target: &str) -> bool {
    match family_of(target) {
        Some(family) => family.backends.contains(&scheme_name),
        None => true,
    }
}

/// Example part numbers for a target picker, one per family.
pub fn target_suggestions() -> Vec<&'static str> {
    FAMILIES.iter().map(|f| f.example).collect()
}

/// Normalizes and resolves board or alias names into canonical target chip names.
///
/// These are the names people type — silkscreen board names, the short part
/// number, the Nucleo order code — mapped onto something the target registry
/// will actually match.
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

/// Renders the support matrix as Markdown.
///
/// `docs/supported-devices.md` is this function's output, checked in so it can
/// be read on the web, and checked by a test so it cannot go stale.
pub fn render_supported_devices() -> String {
    let mut out = String::new();
    out.push_str("# Supported devices

");
    for line in [
        "Generated from `crates/device-db`. Do not edit by hand — run",
        "`cargo run -p device-db --bin gen-supported-devices` instead.",
        "",
        "Chip geometry (flash size, sector map, core) is **not** listed here: it comes",
        "from the probe-rs target registry at runtime, which knows far more parts than",
        "this table names. A part missing from this list is usually still programmable",
        "— type its exact name into the target field.",
        "",
    ] {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("| Vendor | Family | Example part | Backends |
");
    out.push_str("|---|---|---|---|
");
    for family in FAMILIES {
        out.push_str(&format!(
            "| {} | {} | `{}` | {} |
",
            family.vendor.display_name(),
            family.display,
            family.example,
            family
                .backends
                .iter()
                .map(|b| format!("`{b}:`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    out.push_str("
## Backends

");
    out.push_str("| Scheme | Backend | Reaches |
|---|---|---|
");
    out.push_str(
        "| `probe:` | probe-rs | ST-Link, CMSIS-DAP/DAPLink, and J-Link probes over SWD or JTAG |
",
    );
    out.push_str("| `mock:` | simulated | Nothing physical — a NOR flash model for tests and demos |
");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_board_names_people_type() {
        assert_eq!(resolve_target_alias("nucleo-u575zi-q"), Some("STM32U575ZITx"));
        assert_eq!(resolve_target_alias("nucleo-h753zi"), Some("STM32H753ZI"));
        assert_eq!(resolve_target_alias("NUCLEO_H753ZI"), Some("STM32H753ZI"));
        assert_eq!(resolve_target_alias("h753"), Some("STM32H753ZI"));
        assert_eq!(resolve_target_alias("nucleo-f401re"), Some("STM32F401RE"));
        assert_eq!(resolve_target_alias("BluePill"), Some("STM32F103C8"));
        assert_eq!(resolve_target_alias("rp2040"), Some("RP2040"));
        assert_eq!(resolve_target_alias("not-a-board"), None);
    }

    #[test]
    fn picks_the_longest_matching_family_prefix() {
        assert_eq!(family_of("STM32H753ZI").unwrap().prefix, "STM32H7");
        assert_eq!(family_of("EFR32MG24B210F1536IM48").unwrap().prefix, "EFR32MG");
        assert!(family_of("nothing-like-this").is_none());
    }

    #[test]
    fn every_family_names_at_least_one_backend() {
        for family in FAMILIES {
            assert!(
                !family.backends.is_empty(),
                "{} claims no backend",
                family.prefix
            );
        }
    }

    #[test]
    fn the_checked_in_support_matrix_is_current() {
        // The document is generated; a stale copy in the repository is a lie
        // told to anyone reading it on the web.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/supported-devices.md");
        let checked_in = std::fs::read_to_string(path)
            .expect("docs/supported-devices.md is missing")
            .replace("\r\n", "\n");
        assert_eq!(
            checked_in,
            render_supported_devices(),
            "docs/supported-devices.md is stale; run `cargo run -p device-db --bin gen-supported-devices`"
        );
    }

    #[test]
    fn an_unknown_target_is_not_gatekept() {
        // The probe-rs registry knows far more chips than this table names.
        assert!(backend_supports(scheme::PROBE_RS, "SOME_NEW_CHIP"));
        assert!(backend_supports(scheme::PROBE_RS, "STM32U575ZITx"));
        assert!(!backend_supports(scheme::ESP_SERIAL, "STM32U575ZITx"));
    }
}
