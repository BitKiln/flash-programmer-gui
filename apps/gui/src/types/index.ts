// ── Probe types ──────────────────────────────────────────────────────────────

export interface ProbeInfo {
  identifier: string;
  vendor_name: string;
  product_name: string;
  serial_number: string | null;
  probe_type: string;
  supported_protocols: string[];
  default_speed_khz: number;
  max_speed_khz: number;
}

// ── Target types ─────────────────────────────────────────────────────────────

export interface TargetInfo {
  name: string;
  architecture: string;
  flash_base: number;
  flash_size: number;
  ram_base: number;
  ram_size: number;
  page_size: number;
  sector_count: number;
}

// ── Firmware types ───────────────────────────────────────────────────────────

export interface FirmwareInfo {
  format: string;
  file_path: string | null;
  file_size_bytes: number;
  total_firmware_bytes: number;
  base_address: number;
  highest_address: number;
  segment_count: number;
  entry_point: number | null;
  crc32: number;
}

// ── Flash operation types ────────────────────────────────────────────────────

export interface FlashOptions {
  verify: boolean;
  reset: boolean;
  chipErase: boolean;
}

export interface FlashResult {
  success: boolean;
  bytes_flashed: number;
  duration_ms: number;
  verify_passed: boolean | null;
  reset_performed: boolean;
  message: string;
}

export interface VerifyResult {
  success: boolean;
  bytes_verified: number;
  mismatch_count: number;
  checksum_expected: number;
  checksum_actual: number;
}

// ── Progress types ───────────────────────────────────────────────────────────

export interface ProgressInfo {
  stage: string;
  bytesTransferred: number;
  totalBytes: number;
  percentage: number;
  speedBps: number;
  elapsedMs: number;
  currentAddress: number;
  message: string;
}

// ── Event types ──────────────────────────────────────────────────────────────

export type FlashEventDto =
  | {
      type: "StageStarted";
      stage: string;
      total_bytes: number;
      message: string;
    }
  | {
      type: "Progress";
      stage: string;
      bytes_transferred: number;
      total_bytes: number;
      percentage: number;
      speed_bps: number;
      elapsed_ms: number;
      current_address: number;
      message: string;
    }
  | {
      type: "StageCompleted";
      stage: string;
      duration_ms: number;
    }
  | {
      type: "Log";
      level: string;
      message: string;
      timestamp_ms: number;
    }
  | {
      type: "Warning";
      message: string;
    }
  | {
      type: "Error";
      stage: string;
      message: string;
    };

// ── Log types ────────────────────────────────────────────────────────────────

export type LogLevel = "info" | "warn" | "error" | "success" | "debug" | "trace";

export interface LogEntry {
  id: number;
  timestamp: Date;
  level: LogLevel;
  message: string;
}

// ── Connection status ────────────────────────────────────────────────────────

export type ConnectionStatus =
  | "disconnected"
  | "connecting"
  | "connected"
  | "error";

export type FlashStatus =
  | "idle"
  | "erasing"
  | "programming"
  | "verifying"
  | "resetting"
  | "completed"
  | "error";

// ── App state ────────────────────────────────────────────────────────────────

export interface AppState {
  probes: ProbeInfo[];
  selectedProbe: string | null;
  connectionStatus: ConnectionStatus;
  targetInfo: TargetInfo | null;
  firmware: FirmwareInfo | null;
  firmwarePath: string | null;
  flashOptions: FlashOptions;
  flashStatus: FlashStatus;
  progress: ProgressInfo;
  logs: LogEntry[];
  recentFiles: string[];
}
