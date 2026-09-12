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
use flash_backend_mock::{FaultInjector, MockFlashMemory, MockFlashSession};
use flash_core::traits::{FlashBackend, FlashSession};
use flash_core::types::{ConnectionConfig, Transport};

/// Flash parameters after the CLI-over-profile-over-default hierarchy has been
/// applied. `flash` and `batch` accept the same flags, so they resolve them the
/// same way.
#[derive(Debug, Clone)]
pub struct ResolvedFlash {
    pub file_path: String,
    /// Serial-number stamping, when an address was given.
    pub serial: Option<flash_core::SerialConfig>,
    pub target: String,
    pub probe: Option<String>,
    /// How to reach the target. Serial when a `--port` or an `esp:`
    /// identifier was given, a debug probe otherwise.
    pub transport: Transport,
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

    let (probe, transport) =
        resolve_transport(probe.as_deref(), args.port.as_deref(), args.baud)?;

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

    let serial = match args.serial_address {
        Some(ref addr) => Some(flash_core::SerialConfig {
            address: parse_address(addr)?,
            format: args.serial_format.clone(),
            start: args.serial_start,
            step: args.serial_step,
            encoding: args.serial_encoding.into(),
            width: args.serial_width,
            pad: 0xFF,
            verify: !args.no_serial_verify,
        }),
        None => None,
    };

    Ok(ResolvedFlash {
        file_path,
        serial,
        target,
        probe,
        transport,
        interface,
        speed,
        base_address,
        verify,
        reset,
        full_erase,
    })
}

/// Resolves the probe identifier and transport from the connection flags.
///
/// `--port COM7` is shorthand for `--probe esp:COM7`: the registry routes on
/// the scheme, so a serial target needs no separate code path. `--baud` only
/// means something once the connection is a serial one, and saying so is more
/// use than silently ignoring it.
pub fn resolve_transport(
    probe: Option<&str>,
    port: Option<&str>,
    baud: Option<u32>,
) -> Result<(Option<String>, Transport), CliError> {
    let probe_id = match (port, probe) {
        (Some(port), _) => Some(format!("esp:{port}")),
        (None, Some(probe)) => Some(probe.to_string()),
        (None, None) => None,
    };

    let is_serial = probe_id
        .as_deref()
        .map(|id| id.starts_with("esp:"))
        .unwrap_or(false);

    if !is_serial {
        if baud.is_some() {
            return Err(CliError::InvalidArgsOrProfile(
                "--baud applies to a serial bootloader connection; pass --port <PORT>, \
                 or --probe esp:<PORT>"
                    .to_string(),
            ));
        }
        return Ok((probe_id, Transport::DebugProbe));
    }

    Ok((
        probe_id,
        Transport::Serial {
            baud: baud.unwrap_or(DEFAULT_SERIAL_BAUD),
            controls_reset: true,
        },
    ))
}

/// Matches the ESP backend's default, kept here so the CLI can state it in
/// `--help` without depending on the backend crate.
pub const DEFAULT_SERIAL_BAUD: u32 = 460_800;
/// The backend registry this invocation should use.
///
/// `--mock` narrows the registry to the simulated backend so that a run with no
/// hardware attached cannot accidentally reach a probe that happens to be
/// plugged in. Without it, every compiled-in backend is available and the probe
/// identifier's scheme decides which one serves the request.
pub fn get_backend(cli: &Cli) -> Box<dyn FlashBackend> {
    if cli.mock {
        Box::new(flash_backends::mock_registry())
    } else {
        Box::new(flash_backends::default_registry_with_target_descriptions(
            cli.target_yaml.clone(),
        ))
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
        device_db::backend_supports(device_db::scheme::PROBE_RS, target)
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
