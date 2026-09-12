pub mod batch;
pub mod devices;
pub mod erase;
pub mod flash;
pub mod profile;
pub mod reset;
pub mod verify;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::cli::{parse_address, Cli, FlashArgs, Protocol};
use crate::exit_codes::CliError;
use flash_core::error::FlashError;
use flash_core::mock::{FaultInjector, MockFlashMemory, MockFlashSession};
use flash_core::traits::{FlashBackend, FlashSession};
use flash_core::types::ConnectionConfig;
use flash_core::MockProbeBackend;

#[cfg(feature = "live-probe")]
use flash_core::ProbeRsLiveBackend;

/// Flash parameters after the CLI-over-profile-over-default hierarchy has been
/// applied. `flash` and `batch` accept the same flags, so they resolve them the
/// same way.
#[derive(Debug, Clone)]
pub struct ResolvedFlash {
    pub file_path: String,
    pub target: String,
    pub probe: Option<String>,
    pub interface: Protocol,
    pub speed: u32,
    pub base_address: Option<u32>,
    pub verify: bool,
    pub reset: bool,
    pub full_erase: bool,
}

/// Resolves firmware, connection, and option values from explicit flags, the
/// named profile, and the built-in defaults, in that order of precedence.
pub fn resolve_flash_params(cli: &Cli, args: &FlashArgs) -> Result<ResolvedFlash, CliError> {
    let profile = if let Some(ref prof_name) = args.profile {
        let custom_file = cli.profile_file.as_deref().map(std::path::Path::new);
        Some(flash_core::profile::load_profile(prof_name, custom_file)?)
    } else {
        None
    };

    let file_path = args
        .file
        .clone()
        .or_else(|| {
            profile
                .as_ref()
                .and_then(|p| p.default_path().map(ToString::to_string))
        })
        .ok_or_else(|| {
            CliError::InvalidArgsOrProfile(
                "No firmware file specified. Provide a file path or specify a profile with default_path."
                    .to_string(),
            )
        })?;

    let target = args
        .target
        .clone()
        .or_else(|| profile.as_ref().map(|p| p.target().to_string()))
        .unwrap_or_else(|| "auto".to_string());

    if !is_supported_target(&target, cli.mock) {
        return Err(CliError::TargetConnection(format!(
            "Target MCU '{}' is not supported by probe backend",
            target
        )));
    }

    let probe = args.probe.clone().or_else(|| {
        profile
            .as_ref()
            .and_then(|p| p.probe_id().map(ToString::to_string))
    });

    let interface = args
        .interface
        .or_else(|| {
            profile
                .as_ref()
                .and_then(|p| match p.interface().to_lowercase().as_str() {
                    "swd" => Some(Protocol::Swd),
                    "jtag" => Some(Protocol::Jtag),
                    _ => None,
                })
        })
        .unwrap_or(Protocol::Swd);

    let speed = args
        .speed
        .or_else(|| profile.as_ref().map(|p| p.speed_khz()))
        .unwrap_or(2000);

    let base_address_str = args.base_address.clone().or_else(|| {
        profile
            .as_ref()
            .and_then(|p| p.base_address().map(ToString::to_string))
    });
    let base_address = match base_address_str {
        Some(ref addr_s) => Some(parse_address(addr_s)?),
        None => None,
    };

    let verify = if args.no_verify {
        false
    } else {
        args.verify
            .or_else(|| profile.as_ref().map(|p| p.verify_after()))
            .unwrap_or(true)
    };

    let reset = if args.no_reset {
        false
    } else {
        args.reset
            .or_else(|| profile.as_ref().map(|p| p.reset_after()))
            .unwrap_or(true)
    };

    let full_erase =
        args.full_erase || profile.as_ref().map(|p| p.full_chip_erase()).unwrap_or(false);

    Ok(ResolvedFlash {
        file_path,
        target,
        probe,
        interface,
        speed,
        base_address,
        verify,
        reset,
        full_erase,
    })
}

/// Returns the appropriate probe backend based on the `--mock` CLI flag.
pub fn get_backend(mock: bool) -> Box<dyn FlashBackend> {
    if mock {
        Box::new(MockProbeBackend::new())
    } else {
        #[cfg(feature = "live-probe")]
        {
            Box::new(ProbeRsLiveBackend::new())
        }
        #[cfg(not(feature = "live-probe"))]
        {
            // Fallback when live-probe is not compiled: empty live backend simulator
            Box::new(MockProbeBackend::with_probes(Vec::new()))
        }
    }
}

/// Checks whether the target MCU is recognized / supported.
pub fn is_supported_target(target: &str, mock: bool) -> bool {
    if mock {
        let normalized = target.to_lowercase().replace(['-', '_'], "");
        // The mock backend resolves these to its default profile.
        normalized.is_empty()
            || normalized == "auto"
            || normalized == "default"
            || normalized.contains("f103")
            || normalized.contains("f401")
            || normalized.contains("f411")
            || normalized.contains("stm32")
            || normalized.contains("cortex")
            || normalized.contains("generic")
    } else {
        true
    }
}

/// Computes the persistent backing file path for mock flash simulation across CLI invocations.
pub fn mock_backing_file_path(target_name: &str, probe_id: Option<&str>) -> PathBuf {
    let probe_clean = probe_id
        .unwrap_or("default")
        .to_lowercase()
        .replace(['-', '_', ' ', ':', '/', '\\'], "");
    let target_clean = target_name
        .to_lowercase()
        .replace(['-', '_', ' ', ':', '/', '\\'], "");
    std::env::temp_dir().join(format!(
        "flashgui_mock_flash_{}_{}.bin",
        target_clean, probe_clean
    ))
}

/// Establishes target connection session, restoring non-volatile mock flash memory if in mock mode.
pub fn open_session(
    backend: &dyn FlashBackend,
    config: &ConnectionConfig,
    mock: bool,
) -> Result<Box<dyn FlashSession>, FlashError> {
    if mock {
        let session = backend.open_session(config)?;
        if let Some(target) = session.target_info() {
            let backing = mock_backing_file_path(&target.name, config.probe_id.as_deref());
            if backing.is_file() {
                if let Ok(data) = fs::read(&backing) {
                    if data.len() == target.flash_size as usize {
                        let mut mem = MockFlashMemory::from_target(target);
                        mem.data = data;
                        let fault_injector = Arc::new(Mutex::new(FaultInjector::new()));
                        let mut s = MockFlashSession::new_with_memory(
                            target.clone(),
                            mem,
                            fault_injector,
                        );
                        if let Ok(probes) = backend.list_probes() {
                            s.probe_info = probes.into_iter().find(|p| {
                                config.probe_id.as_deref() == Some(&p.identifier)
                            });
                        }
                        return Ok(Box::new(s));
                    }
                }
            }
        }
        Ok(session)
    } else {
        backend.open_session(config)
    }
}

/// Persists non-volatile mock flash memory to disk after erase or program operations.
pub fn persist_mock_session(
    session: &mut dyn FlashSession,
    mock: bool,
    probe_id: Option<&str>,
) {
    if mock {
        let target_meta =
            session.target_info().map(|t| (t.flash_base, t.flash_size, t.name.clone()));
        if let Some((flash_base, flash_size, target_name)) = target_meta {
            if let Ok(data) = session.read_memory(flash_base, flash_size) {
                let backing = mock_backing_file_path(&target_name, probe_id);
                let _ = fs::write(&backing, data);
            }
        }
    }
}
