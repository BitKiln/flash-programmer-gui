import React, { createContext, useContext, useReducer, useCallback } from "react";
import type {
  AppState,
  BatchSettings,
  ProbeInfo,
  TargetInfo,
  FirmwareInfo,
  FlashOptions,
  FlashStatus,
  ConnectionStatus,
  ProgressInfo,
  LogEntry,
  LogLevel,
} from "../types";

// ── Initial State ────────────────────────────────────────────────────────────

const initialProgress: ProgressInfo = {
  stage: "",
  bytesTransferred: 0,
  totalBytes: 0,
  percentage: 0,
  speedBps: 0,
  elapsedMs: 0,
  currentAddress: 0,
  message: "",
};

const loadRecentFiles = (): string[] => {
  try {
    const stored = localStorage.getItem("flash-programmer-recent-files");
    if (stored) {
      return JSON.parse(stored) as string[];
    }
  } catch {
    // ignore parse errors
  }
  return [];
};

const BATCH_SETTINGS_KEY = "flash-programmer-batch-settings";

const defaultBatchSettings: BatchSettings = {
  target: "",
  count: "10",
  protocol: "Swd",
  speed: 4000,
  rearm: "detach",
  delayMs: "0",
  logPath: "",
  logJson: false,
  stopOnError: false,
  serialAddress: "",
  serialFormat: "SN-{n:06}",
  serialStart: "1",
  serialStep: "1",
  serialEncoding: "ascii",
  serialWidth: "16",
};

/// A production run is set up once and referred to across a shift, so the
/// settings outlive both a tab switch and a restart of the application.
const loadBatchSettings = (): BatchSettings => {
  try {
    const stored = localStorage.getItem(BATCH_SETTINGS_KEY);
    if (stored) {
      // Merge over the defaults so settings saved by an older build, which
      // may lack fields added since, still load.
      return { ...defaultBatchSettings, ...(JSON.parse(stored) as Partial<BatchSettings>) };
    }
  } catch {
    // ignore parse errors
  }
  return defaultBatchSettings;
};

const initialState: AppState = {
  probes: [],
  selectedProbe: null,
  connectionStatus: "disconnected",
  targetInfo: null,
  firmware: null,
  firmwarePath: null,
  flashOptions: {
    verify: true,
    reset: true,
    chipErase: false,
  },
  flashStatus: "idle",
  progress: initialProgress,
  logs: [],
  recentFiles: loadRecentFiles(),
  batchSettings: loadBatchSettings(),
};

// ── Actions ──────────────────────────────────────────────────────────────────

type AppAction =
  | { type: "SET_PROBES"; probes: ProbeInfo[] }
  | { type: "SELECT_PROBE"; probeId: string | null }
  | { type: "SET_CONNECTION_STATUS"; status: ConnectionStatus }
  | { type: "SET_TARGET_INFO"; info: TargetInfo | null }
  | { type: "SET_FIRMWARE"; firmware: FirmwareInfo | null; path: string | null }
  | { type: "SET_FLASH_OPTIONS"; options: Partial<FlashOptions> }
  | { type: "SET_FLASH_STATUS"; status: FlashStatus }
  | { type: "SET_PROGRESS"; progress: Partial<ProgressInfo> }
  | { type: "RESET_PROGRESS" }
  | { type: "ADD_LOG"; level: LogLevel; message: string }
  | { type: "CLEAR_LOGS" }
  | { type: "ADD_RECENT_FILE"; path: string }
  | { type: "SET_BATCH_SETTINGS"; settings: Partial<BatchSettings> };

// ── Log ID counter ───────────────────────────────────────────────────────────

let logIdCounter = 0;

// ── Reducer ──────────────────────────────────────────────────────────────────

function appReducer(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "SET_PROBES":
      return { ...state, probes: action.probes };

    case "SELECT_PROBE":
      return { ...state, selectedProbe: action.probeId };

    case "SET_CONNECTION_STATUS":
      return { ...state, connectionStatus: action.status };

    case "SET_TARGET_INFO":
      return { ...state, targetInfo: action.info };

    case "SET_FIRMWARE":
      return {
        ...state,
        firmware: action.firmware,
        firmwarePath: action.path,
      };

    case "SET_FLASH_OPTIONS":
      return {
        ...state,
        flashOptions: { ...state.flashOptions, ...action.options },
      };

    case "SET_FLASH_STATUS":
      return { ...state, flashStatus: action.status };

    case "SET_PROGRESS":
      return {
        ...state,
        progress: { ...state.progress, ...action.progress },
      };

    case "RESET_PROGRESS":
      return { ...state, progress: initialProgress };

    case "ADD_LOG": {
      const entry: LogEntry = {
        id: ++logIdCounter,
        timestamp: new Date(),
        level: action.level,
        message: action.message,
      };
      return { ...state, logs: [...state.logs, entry] };
    }

    case "CLEAR_LOGS":
      return { ...state, logs: [] };

    case "ADD_RECENT_FILE": {
      const filtered = state.recentFiles.filter((f) => f !== action.path);
      const updated = [action.path, ...filtered].slice(0, 5);
      try {
        localStorage.setItem(
          "flash-programmer-recent-files",
          JSON.stringify(updated)
        );
      } catch {
        // ignore storage errors
      }
      return { ...state, recentFiles: updated };
    }

    case "SET_BATCH_SETTINGS": {
      const updated = { ...state.batchSettings, ...action.settings };
      try {
        localStorage.setItem(BATCH_SETTINGS_KEY, JSON.stringify(updated));
      } catch {
        // ignore storage errors
      }
      return { ...state, batchSettings: updated };
    }

    default:
      return state;
  }
}

// ── Context ──────────────────────────────────────────────────────────────────

interface AppContextType {
  state: AppState;
  dispatch: React.Dispatch<AppAction>;
  addLog: (level: LogLevel, message: string) => void;
}

const AppContext = createContext<AppContextType | undefined>(undefined);

// ── Provider ─────────────────────────────────────────────────────────────────

export function AppProvider({ children }: { children: React.ReactNode }) {
  // Read the persisted slices when the provider mounts, not when this module
  // is first imported, so a restart picks up what the last session saved.
  const [state, dispatch] = useReducer(appReducer, initialState, (base) => ({
    ...base,
    recentFiles: loadRecentFiles(),
    batchSettings: loadBatchSettings(),
  }));

  const addLog = useCallback(
    (level: LogLevel, message: string) => {
      dispatch({ type: "ADD_LOG", level, message });
    },
    [dispatch]
  );

  return (
    <AppContext.Provider value={{ state, dispatch, addLog }}>
      {children}
    </AppContext.Provider>
  );
}

// ── Hook ─────────────────────────────────────────────────────────────────────

export function useAppContext(): AppContextType {
  const context = useContext(AppContext);
  if (!context) {
    throw new Error("useAppContext must be used within an AppProvider");
  }
  return context;
}

export type { AppAction };
