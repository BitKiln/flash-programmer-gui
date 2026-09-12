//! Vendor-agnostic target detection.
//!
//! Detection never relies on a hardcoded list of supported part numbers. It
//! identifies the silicon from on-chip ID registers, then resolves a concrete
//! chip variant through the probe-rs target registry, so any chip shipped in
//! that registry can be flashed without changes here.

use std::thread::sleep;
use std::time::Duration;

use probe_rs::config::{Registry, TargetSelector};
use probe_rs::probe::DebugProbeInfo;
use probe_rs::{MemoryInterface, Permissions, Session};

use flash_core::error::FlashError;
use flash_core::types::ConnectionConfig;

/// Settle time after a failed attach; ST-Link V3 re-enumerates on Windows and a
/// back-to-back open would otherwise fail (or make the probe vanish from the
/// probe list entirely).
const SETTLE: Duration = Duration::from_millis(150);

/// Generic core types tried when the chip is unknown, ordered so that the
/// widest cores come first (a Cortex-M33 target does not answer to `cortex-m4`).
const GENERIC_CORES: &[&str] = &[
    "cortex-m33",
    "cortex-m7",
    "cortex-m4",
    "cortex-m3",
    "cortex-m23",
    "cortex-m0+",
    "cortex-m0",
    "cortex-m55",
];

/// Candidate DBGMCU_IDCODE locations, paired with the STM32 families that
/// implement them. The families are used as a coarse fallback when the device
/// ID itself is not in [`family_prefixes`].
const ID_REGISTERS: &[(u64, &[&str])] = &[
    // F0/F1/F3/F4/F7/G4/L4 (and H7 aliases the same address on some parts)
    (0xE004_2000, &["STM32F", "STM32G4", "STM32L4"]),
    // H7
    (0x5C00_1000, &["STM32H7"]),
    // U5/H5/L5/WBA
    (0xE004_4000, &["STM32U5", "STM32H5", "STM32L5"]),
    // C0/G0/L0/WL
    (0x4001_5800, &["STM32C0", "STM32G0", "STM32L0", "STM32WL"]),
];

/// Candidate locations of the factory-programmed flash size register, which
/// reports the size in KB as a 16-bit value. Families disagree on the address,
/// so every known one is tried and the first plausible answer wins.
const FLASH_SIZE_REGISTERS: &[u64] = &[
    0x1FFF_F7E0, // F1
    0x1FFF_7A22, // F4
    0x1FF0_F442, // F7
    0x1FF1_E880, // H7
    0x1FFF_75E0, // G0/G4/L4/L5/WB/WL
    0x0BFA_07A0, // U5
    0x08FF_F80C, // H5
    0x1FFF_75A0, // C0
];

/// Result of identifying the silicon behind the probe.
pub struct Detected {
    /// Chip name resolvable through the probe-rs registry, when one was found.
    pub chip: Option<String>,
    /// Generic core type that responded, used as a last-resort attach target.
    pub core: Option<&'static str>,
    /// Raw DBGMCU device id, for diagnostics.
    pub dev_id: Option<u32>,
    /// Flash size in KB as reported by the chip itself, when readable.
    pub flash_kb: Option<u32>,
}

/// Maps a DBGMCU device id to registry name prefixes, most specific first.
///
/// These are *prefixes*, not part numbers: the registry lookup expands a prefix
/// into every package variant it knows, so new variants need no change here.
/// probe-rs treats a lowercase `x` in the query as a single-character wildcard.
fn family_prefixes(dev_id: u32) -> &'static [&'static str] {
    match dev_id {
        // --- Cortex-M0/M0+ ---
        0x443 => &["STM32F030x4", "STM32F030"],
        0x444 => &["STM32F03"],
        0x445 => &["STM32F04", "STM32F070"],
        0x440 => &["STM32F05", "STM32F030C8"],
        0x448 => &["STM32F07"],
        0x442 => &["STM32F09"],
        0x453 => &["STM32C03"],
        0x466 => &["STM32G03", "STM32G04"],
        0x460 => &["STM32G07", "STM32G08"],
        0x467 => &["STM32G0B", "STM32G0C"],
        0x456 => &["STM32G05", "STM32G06"],
        0x457 => &["STM32L01", "STM32L02"],
        0x425 => &["STM32L03", "STM32L04"],
        0x417 => &["STM32L05", "STM32L06"],
        0x447 => &["STM32L07", "STM32L08"],
        // --- Cortex-M3 ---
        0x410 => &["STM32F103C8", "STM32F101", "STM32F102"],
        0x412 => &["STM32F103x6"],
        0x414 => &["STM32F103xE", "STM32F103R"],
        0x418 => &["STM32F105", "STM32F107"],
        0x430 => &["STM32F103Z"],
        0x416 => &["STM32L15"],
        0x429 => &["STM32L15"],
        0x427 => &["STM32L16", "STM32L15"],
        0x436 => &["STM32L152", "STM32L162"],
        0x437 => &["STM32L16"],
        // --- Cortex-M4 ---
        0x411 | 0x413 => &["STM32F407", "STM32F405", "STM32F415", "STM32F417"],
        0x419 => &["STM32F429", "STM32F427", "STM32F437", "STM32F439"],
        0x423 => &["STM32F401xC", "STM32F401"],
        0x433 => &["STM32F401xE", "STM32F401"],
        0x431 => &["STM32F411"],
        0x441 => &["STM32F412"],
        0x421 => &["STM32F446"],
        0x434 => &["STM32F469", "STM32F479"],
        0x458 => &["STM32F410"],
        0x463 => &["STM32F413", "STM32F423"],
        0x422 => &["STM32F302", "STM32F303"],
        0x438 | 0x439 | 0x446 => &["STM32F30", "STM32F33", "STM32F37"],
        0x432 => &["STM32F37"],
        0x468 => &["STM32G431", "STM32G441"],
        0x469 => &["STM32G474", "STM32G473", "STM32G484"],
        0x479 => &["STM32G491", "STM32G4A1"],
        0x415 => &["STM32L475", "STM32L476", "STM32L486"],
        0x435 => &["STM32L43", "STM32L44"],
        0x462 => &["STM32L45", "STM32L46"],
        0x464 => &["STM32L41", "STM32L42"],
        0x461 => &["STM32L496", "STM32L4A6"],
        0x470 => &["STM32L4R", "STM32L4S"],
        0x471 => &["STM32L4P", "STM32L4Q"],
        0x495 => &["STM32WB55", "STM32WB35"],
        0x497 => &["STM32WLE5", "STM32WL55"],
        // --- Cortex-M7 ---
        0x449 => &["STM32F74", "STM32F75"],
        0x451 => &["STM32F76", "STM32F77"],
        0x452 => &["STM32F72", "STM32F73"],
        0x450 => &["STM32H743", "STM32H753", "STM32H750"],
        0x480 => &["STM32H7A3", "STM32H7B3"],
        0x483 => &["STM32H723", "STM32H725", "STM32H730", "STM32H733", "STM32H735"],
        // --- Cortex-M33 ---
        0x482 => &["STM32U575", "STM32U585"],
        0x481 => &["STM32U59", "STM32U5A"],
        0x455 => &["STM32U535", "STM32U545"],
        0x476 => &["STM32L4P", "STM32U0"],
        0x472 => &["STM32L552", "STM32L562"],
        0x484 => &["STM32H563", "STM32H573"],
        0x478 => &["STM32H503"],
        0x474 => &["STM32H533", "STM32H523"],
        0x492 => &["STM32WBA52", "STM32WBA5"],
        _ => &[],
    }
}

/// Opens the probe, applying the protocol and speed from `config`.
fn open_probe(
    matched: &DebugProbeInfo,
    config: &ConnectionConfig,
) -> Result<probe_rs::probe::Probe, FlashError> {
    crate::backend::open_probe_internal(matched, config)
}

/// Attaches with the given selector, honouring connect-under-reset.
fn attach(
    matched: &DebugProbeInfo,
    config: &ConnectionConfig,
    selector: impl Into<TargetSelector>,
) -> Result<Session, FlashError> {
    let probe = open_probe(matched, config)?;
    let permissions = Permissions::new().allow_erase_all();
    let res = if config.connect_under_reset {
        probe.attach_under_reset(selector, permissions)
    } else {
        probe.attach(selector, permissions)
    };
    res.map_err(|e| FlashError::ConnectError(e.to_string()))
}

/// Reads the device id from whichever DBGMCU_IDCODE address responds.
///
/// Returns the masked device id plus the families implied by the address that
/// answered.
fn read_device_id(session: &mut Session) -> Option<(u32, &'static [&'static str])> {
    let mut core = session.core(0).ok()?;
    for (addr, families) in ID_REGISTERS {
        if let Ok(val) = core.read_word_32(*addr) {
            // 0x0000_0000 / 0xFFFF_FFFF mean "nothing mapped here".
            if val != 0 && val != u32::MAX {
                let dev_id = val & 0x0FFF;
                if dev_id != 0 && dev_id != 0x0FFF {
                    return Some((dev_id, families));
                }
            }
        }
    }
    None
}

/// Reads the factory flash size register, in KB.
fn read_flash_size_kb(session: &mut Session) -> Option<u32> {
    let mut core = session.core(0).ok()?;
    for addr in FLASH_SIZE_REGISTERS {
        let Ok(val) = core.read_word_32(*addr) else {
            continue;
        };
        let kb = val & 0xFFFF;
        // Real parts report a power-of-two-ish size; 0 and 0xFFFF mean the
        // register is not implemented at this address.
        if (16..=8192).contains(&kb) {
            return Some(kb);
        }
    }
    None
}

/// Total flash a registry variant declares, in KB.
fn variant_flash_kb(registry: &Registry, name: &str) -> Option<u32> {
    let target = registry.get_target_by_name(name).ok()?;
    let bytes: u64 = target
        .memory_map
        .iter()
        .filter_map(|region| match region {
            probe_rs::config::MemoryRegion::Nvm(nvm) if !nvm.is_alias => {
                Some(nvm.range.end.saturating_sub(nvm.range.start))
            }
            _ => None,
        })
        .sum();
    (bytes > 0).then_some((bytes / 1024) as u32)
}

/// Expands registry prefixes into concrete chip names known to probe-rs.
///
/// When the chip reported its own flash size, variants matching that size are
/// tried first - families differ only in flash/package, and attaching to a
/// smaller variant would silently hide part of the flash.
fn registry_candidates(prefixes: &[&str], flash_kb: Option<u32>, hints: &[String]) -> Vec<String> {
    let registry = Registry::from_builtin_families();
    let mut out: Vec<String> = Vec::new();
    for prefix in prefixes {
        for name in registry.search_chips(prefix) {
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }

    // Rank: the board the debugger names first, then variants whose flash size
    // matches what the chip reported. A hint only reorders candidates that the
    // silicon already put in play, so it cannot select a foreign chip.
    out.sort_by_key(|name| {
        let upper = name.to_uppercase();
        let hint_rank = if hints.iter().any(|h| upper.starts_with(&h.to_uppercase())) {
            0
        } else {
            1
        };
        let size_rank = match (flash_kb, variant_flash_kb(&registry, name)) {
            (Some(kb), Some(size)) if size == kb => 0,
            (Some(kb), Some(size)) if size > kb => 1,
            (Some(_), Some(_)) => 2,
            (Some(_), None) => 3,
            (None, _) => 0,
        };
        (hint_rank, size_rank)
    });
    out
}

/// Identifies the connected target.
///
/// Order: on-chip ID register read through a generic core attach, then a
/// registry-driven attach over the variants of the identified family. When the
/// chip cannot be named, the generic core that responded is reported so the
/// caller can still open a session.
pub fn detect(matched: &DebugProbeInfo, config: &ConnectionConfig) -> Detected {
    let mut detected = Detected {
        chip: None,
        core: None,
        dev_id: None,
        flash_kb: None,
    };

    // 1. Find any generic core that answers, and read the device id through it.
    let mut families: &[&str] = &[];
    for core_type in GENERIC_CORES {
        match attach(matched, config, *core_type) {
            Ok(mut session) => {
                detected.core = Some(core_type);
                if let Some((dev_id, fams)) = read_device_id(&mut session) {
                    detected.dev_id = Some(dev_id);
                    families = fams;
                }
                detected.flash_kb = read_flash_size_kb(&mut session);
                drop(session);
                sleep(SETTLE);
                break;
            }
            Err(_) => sleep(SETTLE),
        }
    }

    // 2. Turn the device id into registry name prefixes. Fall back to the
    //    families implied by the ID register that answered.
    let mut prefixes: Vec<&str> = Vec::new();
    if let Some(dev_id) = detected.dev_id {
        prefixes.extend_from_slice(family_prefixes(dev_id));
    }
    if prefixes.is_empty() {
        prefixes.extend_from_slice(families);
    }
    if prefixes.is_empty() {
        return detected;
    }

    // 3. Attach to registry variants until one works. A variant that attaches
    //    is one whose flash algorithm and memory map the chip accepts.
    let hints = board_hints();
    let candidates = registry_candidates(&prefixes, detected.flash_kb, &hints);
    for candidate in candidates.iter().take(12) {
        match attach(matched, config, candidate.as_str()) {
            Ok(session) => {
                drop(session);
                sleep(SETTLE);
                detected.chip = Some(candidate.clone());
                return detected;
            }
            Err(_) => sleep(SETTLE),
        }
    }

    detected
}

/// Checks that a manually chosen target matches the silicon actually attached.
///
/// probe-rs will happily attach to an STM32U575 while told it is an
/// STM32F401RE - the ARM debug interface is the same - and only fail later,
/// during erase or programming, with a confusing flash error. Comparing the
/// chip's own device id against the requested name catches that up front.
///
/// Returns an explanation when the target is definitely wrong; `None` when it
/// matches or when the chip could not be identified (non-ST parts do not
/// implement DBGMCU_IDCODE, and must not be rejected on that basis).
pub fn target_mismatch(session: &mut Session, requested: &str) -> Option<String> {
    // Compare against the target that was actually attached, not the raw user
    // input: "nucleo-h753zi" is a valid way to ask for STM32H753ZITx.
    let attached = session.target().name.clone();
    let (dev_id, _) = read_device_id(session)?;
    let prefixes = family_prefixes(dev_id);
    if prefixes.is_empty() {
        return None;
    }

    let requested_upper = attached.to_uppercase();
    // A prefix may carry probe-rs' lowercase 'x' wildcard; compare only up to
    // the first one, which still pins down the family and sub-family.
    let matches = prefixes.iter().any(|prefix| {
        let stem = prefix.split('x').next().unwrap_or(prefix).to_uppercase();
        requested_upper.starts_with(&stem)
    });
    if matches {
        return None;
    }

    let actual = registry_candidates(prefixes, read_flash_size_kb(session), &board_hints())
        .first()
        .cloned()
        .unwrap_or_else(|| prefixes[0].to_string());

    Some(format!(
        "the connected chip is {actual} (device id 0x{dev_id:03X}), not {requested}. Use Auto-Detect, or enter the correct part number."
    ))
}

/// Suggests registry chip names close to what the user typed.
///
/// Used to turn probe-rs's bare "chip not found" into an actionable message.
pub fn suggestions(input: &str) -> Vec<String> {
    let registry = Registry::from_builtin_families();
    let cleaned = input.trim().replace(['-', '_'], "");

    // Progressively shorten the query until the registry has something to say.
    let mut query = cleaned.as_str();
    while query.len() > 4 {
        let mut found = registry.search_chips(query);
        // search_chips returns one entry per package variant, so the same chip
        // name can repeat many times.
        found.sort();
        found.dedup();
        if !found.is_empty() {
            return found.into_iter().take(8).collect();
        }
        query = &query[..query.len() - 1];
    }
    Vec::new()
}

// ── Board identification via the ST-Link mass-storage volume ─────────────────
//
// On-board ST-Link debuggers (Nucleo, Discovery, mbed-enabled boards) expose a
// small FAT volume whose label names the board - "NOD_U575ZI", "NODE_H753ZI",
// "DIS_L4IOT". The die cannot tell us its own package or sub-family (an
// STM32H753ZI and an STM32H743AI answer the DBGMCU register identically), so
// this label is the only way to report the part the user actually holds.
//
// The hint is never trusted on its own: it can only reorder candidates inside
// the family that was identified from silicon, so a stale or unrelated volume
// cannot select the wrong chip.

/// Returns chip name fragments suggested by mounted debugger volumes,
/// e.g. `["STM32U575ZI"]` for a volume labelled `NOD_U575ZI`.
pub fn board_hints() -> Vec<String> {
    volume_labels()
        .iter()
        .filter_map(|label| chip_from_volume_label(label))
        .collect()
}

/// Extracts a chip name fragment from a debugger volume label.
fn chip_from_volume_label(label: &str) -> Option<String> {
    let upper = label.trim().to_uppercase();
    // NOD_/NODE_ = Nucleo, DIS_/DISCO_ = Discovery.
    let rest = ["NODE_", "NOD_", "DISCO_", "DIS_"]
        .iter()
        .find_map(|prefix| upper.strip_prefix(prefix))?;

    // The remainder is the part suffix, e.g. "U575ZI" or "F401RE". Require a
    // leading family letter plus digits so that volumes like "NOD_DEMO" are
    // ignored.
    let mut chars = rest.chars();
    let family = chars.next()?;
    if !family.is_ascii_alphabetic() || !chars.next()?.is_ascii_digit() {
        return None;
    }
    Some(format!("STM32{rest}"))
}

#[cfg(windows)]
fn volume_labels() -> Vec<String> {
    use windows_sys::Win32::Storage::FileSystem::{GetLogicalDrives, GetVolumeInformationW};

    let mut labels = Vec::new();
    // Bit n of the mask is set when drive letter 'A' + n exists.
    let mask = unsafe { GetLogicalDrives() };
    for n in 0..26u32 {
        if mask & (1 << n) == 0 {
            continue;
        }
        let root: Vec<u16> = format!("{}:\\", (b'A' + n as u8) as char)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut name = [0u16; 256];
        let ok = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                name.as_mut_ptr(),
                name.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };
        if ok != 0 {
            let len = name.iter().position(|c| *c == 0).unwrap_or(name.len());
            let label = String::from_utf16_lossy(&name[..len]);
            if !label.is_empty() {
                labels.push(label);
            }
        }
    }
    labels
}

#[cfg(not(windows))]
fn volume_labels() -> Vec<String> {
    // Removable media is mounted under the label's own name on Linux and macOS.
    let mut roots = vec![
        std::path::PathBuf::from("/Volumes"),
        std::path::PathBuf::from("/media"),
    ];
    if let Ok(user) = std::env::var("USER") {
        roots.push(std::path::PathBuf::from(format!("/media/{user}")));
        roots.push(std::path::PathBuf::from(format!("/run/media/{user}")));
    }

    let mut labels = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                labels.push(name.to_string());
            }
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::chip_from_volume_label;

    #[test]
    fn reads_nucleo_and_discovery_labels() {
        assert_eq!(
            chip_from_volume_label("NOD_U575ZI").as_deref(),
            Some("STM32U575ZI")
        );
        assert_eq!(
            chip_from_volume_label("NODE_H753ZI").as_deref(),
            Some("STM32H753ZI")
        );
        assert_eq!(
            chip_from_volume_label("DIS_F407VG").as_deref(),
            Some("STM32F407VG")
        );
    }

    #[test]
    fn ignores_unrelated_volumes() {
        assert_eq!(chip_from_volume_label("CCCOMA_X64FRE"), None);
        assert_eq!(chip_from_volume_label("NOD_DEMO"), None);
        assert_eq!(chip_from_volume_label(""), None);
    }
}
