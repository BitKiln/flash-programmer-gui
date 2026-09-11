use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use firmware_parser::{EntryPointSource, FirmwareFormat};
use flash_core::{
    ConnectionConfig, FlashEvent, FlashManager, FlashStage, LogLevel, ProgramOptions, WireProtocol,
};

use crate::state::AppState;

// ── DTO types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ProbeInfoDto {
    pub identifier: String,
    pub vendor_name: String,
    pub product_name: String,
    pub serial_number: Option<String>,
    pub probe_type: String,
    pub supported_protocols: Vec<String>,
    pub default_speed_khz: u32,
    pub max_speed_khz: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TargetInfoDto {
    pub name: String,
    pub display_name: Option<String>,
    pub architecture: String,
    pub flash_base: u32,
    pub flash_size: u32,
    pub ram_base: u32,
    pub ram_size: u32,
    pub page_size: u32,
    pub sector_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FirmwareInfoDto {
    pub format: String,
    pub file_path: Option<String>,
    pub file_size_bytes: u64,
    pub total_firmware_bytes: u64,
    pub base_address: u32,
    pub highest_address: u32,
    pub segment_count: usize,
    pub entry_point: Option<u32>,
    /// How the entry point was determined, so the user can tell a declared
    /// entry point from one inferred from a Cortex-M vector table.
    pub entry_point_source: String,
    pub crc32: u32,
    pub segments: Vec<SegmentInfoDto>,
    pub gaps: Vec<MemoryGapDto>,
}

/// One contiguous block the image will write.
#[derive(Debug, Clone, Serialize)]
pub struct SegmentInfoDto {
    pub index: usize,
    pub start_address: u32,
    pub end_address: u32,
    pub size_bytes: usize,
    /// Uppercase hex, as the parser formats it (e.g. "0x0A5B1F0D").
    pub crc32: String,
}

/// An unwritten span between two segments.
#[derive(Debug, Clone, Serialize)]
pub struct MemoryGapDto {
    pub start_address: u32,
    pub end_address: u32,
    pub size: u32,
}

/// A saved programming profile, as the frontend sees it.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ProfileDto {
    pub name: String,
    pub description: Option<String>,
    pub target: String,
    pub probe_id: Option<String>,
    pub interface: String,
    pub speed_khz: u32,
    pub firmware_path: Option<String>,
    pub base_address: Option<String>,
    pub verify: bool,
    pub reset: bool,
    pub full_chip_erase: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileSummaryDto {
    pub name: String,
    pub description: Option<String>,
    pub target: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlashResultDto {
    pub success: bool,
    pub bytes_flashed: u32,
    pub duration_ms: u64,
    pub verify_passed: Option<bool>,
    pub reset_performed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyResultDto {
    pub success: bool,
    pub bytes_verified: u32,
    pub mismatch_count: usize,
    pub checksum_expected: u32,
    pub checksum_actual: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum FlashEventDto {
    StageStarted {
        stage: String,
        total_bytes: u64,
        message: String,
    },
    Progress {
        stage: String,
        bytes_transferred: u64,
        total_bytes: u64,
        percentage: f32,
        speed_bps: f64,
        elapsed_ms: u64,
        current_address: u32,
        message: String,
    },
    StageCompleted {
        stage: String,
        duration_ms: u64,
    },
    Log {
        level: String,
        message: String,
        timestamp_ms: u64,
    },
    Warning {
        message: String,
    },
    Error {
        stage: String,
        message: String,
    },
}

fn stage_to_string(stage: &FlashStage) -> String {
    match stage {
        FlashStage::Connecting => "connecting".to_string(),
        FlashStage::Erasing => "erasing".to_string(),
        FlashStage::Programming => "programming".to_string(),
        FlashStage::Verifying => "verifying".to_string(),
        FlashStage::Resetting => "resetting".to_string(),
        FlashStage::Completed => "completed".to_string(),
        FlashStage::Failed => "failed".to_string(),
        FlashStage::Cancelled => "cancelled".to_string(),
    }
}

fn level_to_string(level: &LogLevel) -> String {
    match level {
        LogLevel::Trace => "trace".to_string(),
        LogLevel::Debug => "debug".to_string(),
        LogLevel::Info => "info".to_string(),
        LogLevel::Warn => "warn".to_string(),
        LogLevel::Error => "error".to_string(),
    }
}

fn flash_event_to_dto(event: &FlashEvent) -> FlashEventDto {
    match event {
        FlashEvent::StageStarted {
            stage,
            total_bytes,
            message,
        } => FlashEventDto::StageStarted {
            stage: stage_to_string(stage),
            total_bytes: *total_bytes,
            message: message.clone(),
        },
        FlashEvent::Progress(metrics) => FlashEventDto::Progress {
            stage: stage_to_string(&metrics.stage),
            bytes_transferred: metrics.bytes_transferred,
            total_bytes: metrics.total_bytes,
            percentage: metrics.percentage,
            speed_bps: metrics.speed_bps,
            elapsed_ms: metrics.elapsed_ms,
            current_address: metrics.current_address,
            message: metrics.message.clone(),
        },
        FlashEvent::StageCompleted { stage, duration_ms } => FlashEventDto::StageCompleted {
            stage: stage_to_string(stage),
            duration_ms: *duration_ms,
        },
        FlashEvent::Log {
            level,
            message,
            timestamp_ms,
        } => FlashEventDto::Log {
            level: level_to_string(level),
            message: message.clone(),
            timestamp_ms: *timestamp_ms,
        },
        FlashEvent::Warning { message } => FlashEventDto::Warning {
            message: message.clone(),
        },
        FlashEvent::Error { stage, message } => FlashEventDto::Error {
            stage: stage_to_string(stage),
            message: message.clone(),
        },
    }
}

/// Channel an event is published on, per the IPC contract.
fn event_channel(event: &FlashEvent) -> &'static str {
    match event {
        FlashEvent::Progress(_) => "flash:progress",
        FlashEvent::StageStarted { .. } | FlashEvent::StageCompleted { .. } => "flash:status",
        FlashEvent::Log { .. } | FlashEvent::Warning { .. } | FlashEvent::Error { .. } => {
            "flash:log"
        }
    }
}

/// Publishes telemetry to the frontend as it happens.
///
/// Emission is best-effort: a failed emit must not abort a flash that is
/// already writing to a target, and the event is buffered for
/// `get_flash_events` regardless.
fn emit_event(app: &AppHandle, event: &FlashEvent) {
    let _ = app.emit(event_channel(event), flash_event_to_dto(event));
}

fn entry_point_source_to_string(source: &EntryPointSource) -> String {
    match source {
        EntryPointSource::Record05 => "Intel HEX record 05".to_string(),
        EntryPointSource::Record03 => "Intel HEX record 03".to_string(),
        EntryPointSource::CortexMVectorTable => "Cortex-M vector table".to_string(),
        EntryPointSource::ElfHeader => "ELF header".to_string(),
        EntryPointSource::None => "not declared".to_string(),
    }
}

fn format_to_string(format: &FirmwareFormat) -> String {
    match format {
        FirmwareFormat::IntelHex => "Intel HEX".to_string(),
        FirmwareFormat::RawBinary => "Raw Binary".to_string(),
        FirmwareFormat::Elf => "ELF".to_string(),
    }
}

/// Maps core target metadata to the frontend DTO.
fn target_info_dto(info: &flash_core::types::TargetInfo) -> TargetInfoDto {
    TargetInfoDto {
        name: info.name.clone(),
        display_name: info.display_name.clone(),
        architecture: info.architecture.clone(),
        flash_base: info.flash_base,
        flash_size: info.flash_size,
        ram_base: info.ram_base,
        ram_size: info.ram_size,
        page_size: info.page_size,
        sector_count: info.sectors.len(),
    }
}

/// Closes and drops the active session, if any, before opening a new one.
fn close_active_session(state: &AppState) -> Result<(), String> {
    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    if let Some(mut session) = guard.take() {
        let _ = session.close();
    }
    Ok(())
}

fn parse_protocol(protocol: &str) -> WireProtocol {
    match protocol.to_lowercase().as_str() {
        "jtag" => WireProtocol::Jtag,
        _ => WireProtocol::Swd,
    }
}

/// Runs a blocking probe operation off the Tauri command thread.
///
/// Flashing a large image takes seconds to minutes and holds the session lock
/// for its whole duration. Running that on the command thread would leave the
/// frontend unable to poll events or ask for cancellation until it finished, so
/// the work goes to a blocking worker and the command awaits it.
async fn in_background<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| format!("Operation thread failed: {}", e))?
}

// ── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_probes(state: State<'_, AppState>) -> Result<Vec<ProbeInfoDto>, String> {
    let state = (*state).clone();
    in_background(move || {
        let backend = state.backend.lock().map_err(|e| e.to_string())?;
        let probes = backend.list_probes().map_err(|e| e.to_string())?;

        Ok(probes
            .into_iter()
            .map(|p| ProbeInfoDto {
                identifier: p.identifier,
                vendor_name: p.vendor_name,
                product_name: p.product_name,
                serial_number: p.serial_number,
                probe_type: format!("{:?}", p.probe_type),
                supported_protocols: p
                    .supported_protocols
                    .iter()
                    .map(|pr| format!("{:?}", pr))
                    .collect(),
                default_speed_khz: p.default_speed_khz,
                max_speed_khz: p.max_speed_khz,
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn connect_probe(
    state: State<'_, AppState>,
    probe_id: Option<String>,
    target: String,
    protocol: String,
    speed: u32,
) -> Result<TargetInfoDto, String> {
    let state = (*state).clone();
    in_background(move || {
        close_active_session(&state)?;

        let config = ConnectionConfig {
            probe_id,
            target_name: target,
            protocol: parse_protocol(&protocol),
            speed_khz: speed,
            connect_under_reset: false,
            reset_type: None,
        };

        let session = {
            let backend = state.backend.lock().map_err(|e| e.to_string())?;
            backend.open_session(&config).map_err(|e| e.to_string())?
        };

        let target_info = session.target_info().cloned();
        *state.session.lock().map_err(|e| e.to_string())? = Some(session);

        match target_info {
            Some(info) => Ok(target_info_dto(&info)),
            None => Err("Connected but target info not available".to_string()),
        }
    })
    .await
}

#[tauri::command]
pub async fn auto_detect_target(
    state: State<'_, AppState>,
    probe_id: Option<String>,
    protocol: String,
    speed: u32,
) -> Result<TargetInfoDto, String> {
    let state = (*state).clone();
    in_background(move || {
        close_active_session(&state)?;

        let config = ConnectionConfig {
            probe_id,
            target_name: "auto".to_string(),
            protocol: parse_protocol(&protocol),
            speed_khz: speed,
            connect_under_reset: false,
            reset_type: None,
        };

        let session = {
            let backend = state.backend.lock().map_err(|e| e.to_string())?;
            backend.open_session(&config).map_err(|e| e.to_string())?
        };

        let target_info = session.target_info().cloned().ok_or_else(|| {
            "Could not detect target MCU information from connected probe".to_string()
        })?;

        *state.session.lock().map_err(|e| e.to_string())? = Some(session);
        Ok(target_info_dto(&target_info))
    })
    .await
}

#[tauri::command]
pub fn load_firmware(path: String, base_address: Option<u32>) -> Result<FirmwareInfoDto, String> {
    let image = firmware_parser::parse_file(&path, base_address).map_err(|e| e.to_string())?;

    Ok(FirmwareInfoDto {
        format: format_to_string(&image.metadata.format),
        file_path: image.metadata.file_path,
        file_size_bytes: image.metadata.file_size_bytes,
        total_firmware_bytes: image.metadata.total_firmware_bytes,
        base_address: image.metadata.base_address,
        highest_address: image.metadata.highest_address,
        segment_count: image.metadata.segment_count,
        entry_point: image.metadata.entry_point,
        entry_point_source: entry_point_source_to_string(&image.metadata.entry_point_source),
        crc32: image.metadata.crc32,
        segments: image
            .metadata
            .segments
            .iter()
            .map(|segment| SegmentInfoDto {
                index: segment.index,
                start_address: segment.start_address,
                end_address: segment.end_address,
                size_bytes: segment.size_bytes,
                crc32: segment.checksums.crc32.clone(),
            })
            .collect(),
        gaps: image
            .metadata
            .memory_gaps
            .iter()
            .map(|gap| MemoryGapDto {
                start_address: gap.start_address,
                end_address: gap.end_address,
                size: gap.size,
            })
            .collect(),
    })
}

#[tauri::command]
pub async fn flash_firmware(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    base_address: Option<u32>,
    verify: bool,
    reset: bool,
    chip_erase: bool,
) -> Result<FlashResultDto, String> {
    let state = (*state).clone();
    in_background(move || {
        let image = firmware_parser::parse_file(&path, base_address).map_err(|e| e.to_string())?;

        let options = ProgramOptions {
            verify_after: verify,
            reset_after: reset,
            chip_erase,
            chunk_size: 1024,
        };

        state.arm();
        let emitter = app.clone();
        let callback =
            state.progress_callback(move |event: &FlashEvent| emit_event(&emitter, event));

        let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
        let session = session_guard
            .as_mut()
            .ok_or_else(|| "No active session. Connect to a probe first.".to_string())?;

        let result =
            FlashManager::execute_flash(session.as_mut(), &image, &options, Some(&callback))
                .map_err(|e| describe_failure(&app, &state, e))?;

        Ok(FlashResultDto {
            success: result.success,
            bytes_flashed: result.bytes_flashed,
            duration_ms: result.duration_ms,
            verify_passed: result.verify_report.as_ref().map(|r| r.success),
            reset_performed: result.reset_performed,
            message: result.message,
        })
    })
    .await
}

#[tauri::command]
pub async fn erase_chip(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let state = (*state).clone();
    in_background(move || {
        state.arm();
        let emitter = app.clone();
        let callback =
            state.progress_callback(move |event: &FlashEvent| emit_event(&emitter, event));

        let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
        let session = session_guard
            .as_mut()
            .ok_or_else(|| "No active session. Connect to a probe first.".to_string())?;

        session
            .erase_all(Some(&callback))
            .map_err(|e| describe_failure(&app, &state, e))?;

        Ok("Chip erased successfully".to_string())
    })
    .await
}

#[tauri::command]
pub async fn verify_firmware(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    base_address: Option<u32>,
) -> Result<VerifyResultDto, String> {
    let state = (*state).clone();
    in_background(move || {
        let image = firmware_parser::parse_file(&path, base_address).map_err(|e| e.to_string())?;

        state.arm();
        let emitter = app.clone();
        let callback =
            state.progress_callback(move |event: &FlashEvent| emit_event(&emitter, event));

        let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
        let session = session_guard
            .as_mut()
            .ok_or_else(|| "No active session. Connect to a probe first.".to_string())?;

        let report = session
            .verify(&image.segments, Some(&callback))
            .map_err(|e| describe_failure(&app, &state, e))?;

        Ok(VerifyResultDto {
            success: report.success,
            bytes_verified: report.bytes_verified,
            mismatch_count: report.mismatches.len(),
            checksum_expected: report.checksum_expected,
            checksum_actual: report.checksum_actual,
        })
    })
    .await
}

#[tauri::command]
pub async fn reset_target(state: State<'_, AppState>, halt: bool) -> Result<String, String> {
    let state = (*state).clone();
    in_background(move || {
        let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
        let session = session_guard
            .as_mut()
            .ok_or_else(|| "No active session. Connect to a probe first.".to_string())?;

        session.reset(halt).map_err(|e| e.to_string())?;

        Ok(if halt {
            "Target reset and halted".to_string()
        } else {
            "Target reset successfully".to_string()
        })
    })
    .await
}

/// Closes the active session and releases the debug probe.
///
/// Leaving the probe held blocks other tools (STM32CubeProgrammer, OpenOCD)
/// and a second run of this app, so disconnecting must be explicit.
#[tauri::command]
pub async fn disconnect_probe(state: State<'_, AppState>) -> Result<String, String> {
    let state = (*state).clone();
    in_background(move || {
        let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
        match session_guard.take() {
            Some(mut session) => {
                session.close().map_err(|e| e.to_string())?;
                Ok("Disconnected from target".to_string())
            }
            None => Ok("No active session".to_string()),
        }
    })
    .await
}

#[tauri::command]
pub fn get_flash_events(state: State<'_, AppState>) -> Result<Vec<FlashEventDto>, String> {
    let events = state.drain_events();
    Ok(events.iter().map(flash_event_to_dto).collect())
}

/// Turns a failure into a message, distinguishing a user cancellation from a
/// genuine error: the backends surface cancellation as an ordinary error, so
/// without this the console would report a cancelled flash as a fault.
fn describe_failure(app: &AppHandle, state: &AppState, error: flash_core::FlashError) -> String {
    if state.is_cancelled() {
        let event = FlashEvent::Error {
            stage: FlashStage::Cancelled,
            message: "Operation cancelled".to_string(),
        };
        emit_event(app, &event);
        state.push_event(event);
        "Operation cancelled".to_string()
    } else {
        error.to_string()
    }
}

/// Requests cancellation of the operation currently in flight.
///
/// The backends poll this at block boundaries, so the target is left in a
/// defined state: a cancelled program stops between chunks rather than part way
/// through a write. Returns immediately; the in-flight command reports the
/// cancellation to the frontend.
#[tauri::command]
pub fn cancel_operation(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    state.cancel();
    let event = FlashEvent::Warning {
        message: "Cancellation requested; stopping at the next block boundary...".to_string(),
    };
    emit_event(&app, &event);
    state.push_event(event);
    Ok("Cancellation requested".to_string())
}

// ── Profiles ─────────────────────────────────────────────────────────────────
//
// The store lives in `flash-core`, so a profile saved here is the same file the
// CLI reads, and vice versa.

fn profile_to_dto(profile: &flash_core::FlashProfile) -> ProfileDto {
    ProfileDto {
        name: profile.name().to_string(),
        description: profile.description().map(ToString::to_string),
        target: profile.target().to_string(),
        probe_id: profile.probe_id().map(ToString::to_string),
        interface: profile.interface().to_string(),
        speed_khz: profile.speed_khz(),
        firmware_path: profile.default_path().map(ToString::to_string),
        base_address: profile.base_address().map(ToString::to_string),
        verify: profile.verify_after(),
        reset: profile.reset_after(),
        full_chip_erase: profile.full_chip_erase(),
    }
}

#[tauri::command]
pub fn list_profiles() -> Result<Vec<ProfileSummaryDto>, String> {
    flash_core::list_profiles(None)
        .map(|profiles| {
            profiles
                .into_iter()
                .map(|p| ProfileSummaryDto {
                    name: p.name,
                    description: p.description,
                    target: p.target,
                    file_path: p.file_path,
                })
                .collect()
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_profile(name: String) -> Result<ProfileDto, String> {
    flash_core::load_profile(&name, None)
        .map(|profile| profile_to_dto(&profile))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_profile(profile: ProfileDto) -> Result<String, String> {
    let stored = flash_core::FlashProfile::new(
        profile.name,
        profile.description,
        profile.target,
        profile.interface,
        profile.speed_khz,
        profile.probe_id,
        profile.firmware_path,
        profile.base_address,
        profile.verify,
        profile.reset,
        profile.full_chip_erase,
    );
    flash_core::save_profile(&stored, None)
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_profile(name: String) -> Result<String, String> {
    flash_core::delete_profile(&name, None)
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}
