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
  /** Registry-resolvable chip name; safe to feed back into connect. */
  name: string;
  /** Extra identification read from the chip, display only. */
  display_name: string | null;
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
  entry_point_source: string;
  segments: SegmentInfo[];
  gaps: MemoryGap[];
}

/** One contiguous block the image will write. */
export interface SegmentInfo {
  index: number;
  start_address: number;
  end_address: number;
  size_bytes: number;
  /** Uppercase hex string, as the parser formats it. */
  crc32: string;
}

/** An unwritten span between two segments. */
export interface MemoryGap {
  start_address: number;
  end_address: number;
  size: number;
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
  | "cancelling"
  | "cancelled"
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

// ── Profiles ─────────────────────────────────────────────────────────────────

/** A saved programming profile, shared with the CLI's TOML store. */
export interface Profile {
  name: string;
  description: string | null;
  target: string;
  probe_id: string | null;
  interface: string;
  speed_khz: number;
  firmware_path: string | null;
  base_address: string | null;
  verify: boolean;
  reset: boolean;
  full_chip_erase: boolean;
}

export interface ProfileSummary {
  name: string;
  description: string | null;
  target: string;
  file_path: string;
}

// ── Memory viewer ────────────────────────────────────────────────────────────

/** A block of target memory read back from the device. */
export interface MemoryRead {
  address: number;
  bytes: number[];
}

// ── Batch (production) mode ──────────────────────────────────────────────────

/** One board of a batch run. */
export interface BatchUnit {
  index: number;
  status: "passed" | "failed";
  target: string | null;
  bytes_flashed: number;
  verified: boolean;
  duration_ms: number;
  started_unix_ms: number;
  message: string;
}

export interface BatchReport {
  units: BatchUnit[];
  passed: number;
  failed: number;
  duration_ms: number;
  stop_reason: string;
  log_path: string | null;
}

/** Batch progress pushed on the `batch:event` channel. */
export type BatchEventDto =
  | { type: "WaitingForDetach"; index: number }
  | { type: "WaitingForAttach"; index: number }
  | { type: "UnitStarted"; index: number }
  | { type: "UnitFinished"; unit: BatchUnit }
  | { type: "Finished"; passed: number; failed: number; stop_reason: string };

/** What the Batch panel sends to `start_batch`. */
export interface BatchOptions {
  path: string;
  baseAddress: number | null;
  probeId: string | null;
  target: string;
  protocol: string;
  speed: number;
  verify: boolean;
  reset: boolean;
  chipErase: boolean;
  count: number | null;
  rearm: "detach" | "immediate";
  stopOnError: boolean;
  delayMs: number;
  logPath: string | null;
  logJson: boolean;
}
