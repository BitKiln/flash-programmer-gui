use serde::Serialize;
use tauri::State;

use flash_core::{
    ConnectionConfig, FlashEvent, FlashManager, FlashStage, LogLevel, ProgramOptions,
    WireProtocol,
};
use firmware_parser::FirmwareFormat;

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
    pub crc32: u32,
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

fn format_to_string(format: &FirmwareFormat) -> String {
    match format {
        FirmwareFormat::IntelHex => "Intel HEX".to_string(),
        FirmwareFormat::RawBinary => "Raw Binary".to_string(),
        FirmwareFormat::Elf => "ELF".to_string(),
    }
}

fn parse_protocol(protocol: &str) -> WireProtocol {
    match protocol.to_lowercase().as_str() {
        "jtag" => WireProtocol::Jtag,
        _ => WireProtocol::Swd,
    }
}

// ── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_probes(state: State<'_, AppState>) -> Result<Vec<ProbeInfoDto>, String> {
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
}

#[tauri::command]
pub fn connect_probe(
    state: State<'_, AppState>,
    probe_id: Option<String>,
    target: String,
    protocol: String,
    speed: u32,
) -> Result<TargetInfoDto, String> {
    // Close any existing session first
    {
        let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
        if let Some(ref mut s) = *session_guard {
            let _ = s.close();
        }
        *session_guard = None;
    }

    let config = ConnectionConfig {
        probe_id,
        target_name: target,
        protocol: parse_protocol(&protocol),
        speed_khz: speed,
        connect_under_reset: false,
        reset_type: None,
    };

    let backend = state.backend.lock().map_err(|e| e.to_string())?;
    let session = backend.open_session(&config).map_err(|e| e.to_string())?;

    let target_info = session.target_info().cloned();

    let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
    *session_guard = Some(session);

    match target_info {
        Some(info) => Ok(TargetInfoDto {
            name: info.name,
            display_name: info.display_name,
            architecture: info.architecture,
            flash_base: info.flash_base,
            flash_size: info.flash_size,
            ram_base: info.ram_base,
            ram_size: info.ram_size,
            page_size: info.page_size,
            sector_count: info.sectors.len(),
        }),
        None => Err("Connected but target info not available".to_string()),
    }
}

#[tauri::command]
pub fn auto_detect_target(
    state: State<'_, AppState>,
    probe_id: Option<String>,
    protocol: String,
    speed: u32,
) -> Result<TargetInfoDto, String> {
    // Close any existing session first
    {
        let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
        if let Some(ref mut s) = *session_guard {
            let _ = s.close();
        }
        *session_guard = None;
    }

    let config = ConnectionConfig {
        probe_id,
        target_name: "auto".to_string(),
        protocol: parse_protocol(&protocol),
        speed_khz: speed,
        connect_under_reset: false,
        reset_type: None,
    };

    let backend = state.backend.lock().map_err(|e| e.to_string())?;
    let session = backend.open_session(&config).map_err(|e| e.to_string())?;

    let target_info = session.target_info().cloned().ok_or_else(|| {
        "Could not detect target MCU information from connected probe".to_string()
    })?;

    let dto = TargetInfoDto {
        name: target_info.name,
        display_name: target_info.display_name,
        architecture: target_info.architecture,
        flash_base: target_info.flash_base,
        flash_size: target_info.flash_size,
        ram_base: target_info.ram_base,
        ram_size: target_info.ram_size,
        page_size: target_info.page_size,
        sector_count: target_info.sectors.len(),
    };

    let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
    *session_guard = Some(session);

    Ok(dto)
}

#[tauri::command]
pub fn load_firmware(
    path: String,
    base_address: Option<u32>,
) -> Result<FirmwareInfoDto, String> {
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
        crc32: image.metadata.crc32,
    })
}

#[tauri::command]
pub fn flash_firmware(
    state: State<'_, AppState>,
    path: String,
    base_address: Option<u32>,
    verify: bool,
    reset: bool,
    chip_erase: bool,
) -> Result<FlashResultDto, String> {
    let image = firmware_parser::parse_file(&path, base_address).map_err(|e| e.to_string())?;

    let options = ProgramOptions {
        verify_after: verify,
        reset_after: reset,
        chip_erase,
        chunk_size: 1024,
    };

    // Create a progress callback that pushes events into shared state
    let events_ref = &state.events;
    let callback = move |event: FlashEvent| {
        if let Ok(mut events) = events_ref.lock() {
            events.push(event);
        }
    };

    let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = session_guard
        .as_mut()
        .ok_or_else(|| "No active session. Connect to a probe first.".to_string())?;

    let result = FlashManager::execute_flash(session.as_mut(), &image, &options, Some(&callback))
        .map_err(|e| e.to_string())?;

    Ok(FlashResultDto {
        success: result.success,
        bytes_flashed: result.bytes_flashed,
        duration_ms: result.duration_ms,
        verify_passed: result.verify_report.as_ref().map(|r| r.success),
        reset_performed: result.reset_performed,
        message: result.message,
    })
}

#[tauri::command]
pub fn erase_chip(state: State<'_, AppState>) -> Result<String, String> {
    let events_ref = &state.events;
    let callback = move |event: FlashEvent| {
        if let Ok(mut events) = events_ref.lock() {
            events.push(event);
        }
    };

    let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = session_guard
        .as_mut()
        .ok_or_else(|| "No active session. Connect to a probe first.".to_string())?;

    session
        .erase_all(Some(&callback))
        .map_err(|e| e.to_string())?;

    Ok("Chip erased successfully".to_string())
}

#[tauri::command]
pub fn verify_firmware(
    state: State<'_, AppState>,
    path: String,
    base_address: Option<u32>,
) -> Result<VerifyResultDto, String> {
    let image = firmware_parser::parse_file(&path, base_address).map_err(|e| e.to_string())?;

    let events_ref = &state.events;
    let callback = move |event: FlashEvent| {
        if let Ok(mut events) = events_ref.lock() {
            events.push(event);
        }
    };

    let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
    let session = session_guard
        .as_mut()
        .ok_or_else(|| "No active session. Connect to a probe first.".to_string())?;

    let report = session
        .verify(&image.segments, Some(&callback))
        .map_err(|e| e.to_string())?;

    Ok(VerifyResultDto {
        success: report.success,
        bytes_verified: report.bytes_verified,
        mismatch_count: report.mismatches.len(),
        checksum_expected: report.checksum_expected,
        checksum_actual: report.checksum_actual,
    })
}

#[tauri::command]
pub fn reset_target(state: State<'_, AppState>, halt: bool) -> Result<String, String> {
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
}

/// Closes the active session and releases the debug probe.
///
/// Leaving the probe held blocks other tools (STM32CubeProgrammer, OpenOCD)
/// and a second run of this app, so disconnecting must be explicit.
#[tauri::command]
pub fn disconnect_probe(state: State<'_, AppState>) -> Result<String, String> {
    let mut session_guard = state.session.lock().map_err(|e| e.to_string())?;
    match session_guard.take() {
        Some(mut session) => {
            session.close().map_err(|e| e.to_string())?;
            Ok("Disconnected from target".to_string())
        }
        None => Ok("No active session".to_string()),
    }
}

#[tauri::command]
pub fn get_flash_events(state: State<'_, AppState>) -> Result<Vec<FlashEventDto>, String> {
    let events = state.drain_events();
    Ok(events.iter().map(flash_event_to_dto).collect())
}
