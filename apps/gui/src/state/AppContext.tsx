import React, { createContext, useContext, useReducer, useCallback } from "react";
import type {
  AppState,
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
  | { type: "ADD_RECENT_FILE"; path: string };

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
  const [state, dispatch] = useReducer(appReducer, initialState);

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
