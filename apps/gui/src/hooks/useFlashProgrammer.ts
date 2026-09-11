import { invoke } from "@tauri-apps/api/core";
import { useCallback, useRef, useEffect } from "react";
import { useAppContext } from "../state/AppContext";
import type {
  ProbeInfo,
  TargetInfo,
  FirmwareInfo,
  FlashResult,
  VerifyResult,
  FlashEventDto,
} from "../types";

/**
 * Custom hook wrapping all Tauri IPC invoke calls and event polling.
 *
 * Provides imperative functions for probe discovery, connection,
 * firmware loading, flashing, erasing, verification, and reset.
 * Automatically polls for flash events during active operations.
 */
export function useFlashProgrammer() {
  const { state, dispatch, addLog } = useAppContext();
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // ── Event Polling ────────────────────────────────────────────────────────

  const processEvents = useCallback(
    async () => {
      try {
        const events = await invoke<FlashEventDto[]>("get_flash_events");
        for (const event of events) {
          switch (event.type) {
            case "StageStarted":
              addLog("info", `[${event.stage}] ${event.message}`);
              break;
            case "Progress":
              dispatch({
                type: "SET_PROGRESS",
                progress: {
                  stage: event.stage,
                  bytesTransferred: event.bytes_transferred,
                  totalBytes: event.total_bytes,
                  percentage: event.percentage,
                  speedBps: event.speed_bps,
                  elapsedMs: event.elapsed_ms,
                  currentAddress: event.current_address,
                  message: event.message,
                },
              });
              break;
            case "StageCompleted":
              addLog(
                "success",
                `[${event.stage}] Completed in ${event.duration_ms}ms`
              );
              break;
            case "Log":
              addLog(
                event.level === "warn"
                  ? "warn"
                  : event.level === "error"
                    ? "error"
                    : "info",
                event.message
              );
              break;
            case "Warning":
              addLog("warn", event.message);
              break;
            case "Error":
              addLog("error", `[${event.stage}] ${event.message}`);
              break;
          }
        }
      } catch {
        // Silently handle polling errors
      }
    },
    [dispatch, addLog]
  );

  const startPolling = useCallback(() => {
    if (pollingRef.current) return;
    pollingRef.current = setInterval(processEvents, 200);
  }, [processEvents]);

  const stopPolling = useCallback(() => {
    if (pollingRef.current) {
      clearInterval(pollingRef.current);
      pollingRef.current = null;
    }
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => stopPolling();
  }, [stopPolling]);

  // ── Probe Discovery ──────────────────────────────────────────────────────

  const refreshProbes = useCallback(async () => {
    try {
      addLog("info", "Scanning USB ports for physical debug probes (probe-rs)...");
      const probes = await invoke<ProbeInfo[]>("list_probes");
      dispatch({ type: "SET_PROBES", probes });

      const hardwareProbes = probes.filter(
        (p) => !p.identifier.startsWith("mock:") && p.probe_type !== "VirtualMock"
      );
      if (hardwareProbes.length > 0) {
        addLog(
          "success",
          `Detected ${hardwareProbes.length} physical hardware probe(s): ${hardwareProbes.map((p) => p.product_name).join(", ")}`
        );
      } else {
        addLog(
          "info",
          "No physical USB debug probe detected. (Virtual simulation mode available)"
        );
      }
      if (probes.length > 0 && !state.selectedProbe) {
        dispatch({ type: "SELECT_PROBE", probeId: probes[0].identifier });
      }
    } catch (err) {
      addLog("error", `Failed to list probes: ${err}`);
    }
  }, [dispatch, addLog, state.selectedProbe]);

  // ── Connection ───────────────────────────────────────────────────────────

  const connectProbe = useCallback(
    async (
      probeId: string | null,
      target: string,
      protocol: string,
      speed: number
    ): Promise<string | null> => {
      dispatch({ type: "SET_CONNECTION_STATUS", status: "connecting" });
      addLog("info", `Connecting to ${target} via ${protocol}...`);

      try {
        const info = await invoke<TargetInfo>("connect_probe", {
          probeId,
          target,
          protocol,
          speed,
        });
        dispatch({ type: "SET_TARGET_INFO", info });
        dispatch({ type: "SET_CONNECTION_STATUS", status: "connected" });
        addLog(
          "success",
          `Connected to ${info.name} (${info.architecture}), Flash: ${(info.flash_size / 1024).toFixed(0)}KB`
        );
        return null;
      } catch (err) {
        dispatch({ type: "SET_CONNECTION_STATUS", status: "error" });
        const message = `${err}`;
        addLog("error", `Connection failed: ${message}`);
        return message;
      }
    },
    [dispatch, addLog]
  );

  const disconnectProbe = useCallback(async () => {
    try {
      const message = await invoke<string>("disconnect_probe");
      dispatch({ type: "SET_TARGET_INFO", info: null });
      dispatch({ type: "SET_CONNECTION_STATUS", status: "disconnected" });
      addLog("info", message);
      return null;
    } catch (err) {
      const message = `${err}`;
      addLog("error", `Disconnect failed: ${message}`);
      return message;
    }
  }, [dispatch, addLog]);

  const autoDetectTarget = useCallback(
    async (
      probeId: string | null,
      protocol: string,
      speed: number
    ): Promise<{ info: TargetInfo | null; error: string | null }> => {
      dispatch({ type: "SET_CONNECTION_STATUS", status: "connecting" });
      addLog("info", `Auto-detecting connected MCU board via ${protocol}...`);

      try {
        const info = await invoke<TargetInfo>("auto_detect_target", {
          probeId,
          protocol,
          speed,
        });
        dispatch({ type: "SET_TARGET_INFO", info });
        dispatch({ type: "SET_CONNECTION_STATUS", status: "connected" });
        addLog(
          "success",
          `Auto-detected board: ${info.display_name ? `${info.name} [${info.display_name}]` : info.name} (${info.architecture}), Flash: ${(info.flash_size / 1024).toFixed(0)}KB, RAM: ${(info.ram_size / 1024).toFixed(0)}KB`
        );
        return { info, error: null };
      } catch (err) {
        dispatch({ type: "SET_CONNECTION_STATUS", status: "error" });
        const message = `${err}`;
        addLog("error", `Auto-detection failed: ${message}`);
        return { info: null, error: message };
      }
    },
    [dispatch, addLog]
  );

  // ── Firmware Loading ─────────────────────────────────────────────────────

  const loadFirmware = useCallback(
    async (path: string, baseAddress?: number) => {
      try {
        addLog("info", `Loading firmware: ${path}`);
        const info = await invoke<FirmwareInfo>("load_firmware", {
          path,
          baseAddress: baseAddress ?? null,
        });
        dispatch({ type: "SET_FIRMWARE", firmware: info, path });
        dispatch({ type: "ADD_RECENT_FILE", path });
        addLog(
          "success",
          `Loaded ${info.format} firmware: ${info.total_firmware_bytes} bytes, ${info.segment_count} segment(s)`
        );
      } catch (err) {
        addLog("error", `Failed to load firmware: ${err}`);
      }
    },
    [dispatch, addLog]
  );

  // ── Flash ────────────────────────────────────────────────────────────────

  const flashFirmware = useCallback(async () => {
    if (!state.firmwarePath) {
      addLog("error", "No firmware loaded");
      return;
    }

    dispatch({ type: "SET_FLASH_STATUS", status: "programming" });
    dispatch({ type: "RESET_PROGRESS" });
    startPolling();
    addLog("info", "Starting flash operation...");

    try {
      const result = await invoke<FlashResult>("flash_firmware", {
        path: state.firmwarePath,
        baseAddress: null,
        verify: state.flashOptions.verify,
        reset: state.flashOptions.reset,
        chipErase: state.flashOptions.chipErase,
      });

      dispatch({ type: "SET_FLASH_STATUS", status: "completed" });
      dispatch({
        type: "SET_PROGRESS",
        progress: { percentage: 100 },
      });
      addLog("success", result.message);

      // Auto-reset to idle after showing completed state
      setTimeout(() => {
        dispatch({ type: "SET_FLASH_STATUS", status: "idle" });
      }, 3000);
    } catch (err) {
      dispatch({ type: "SET_FLASH_STATUS", status: "error" });
      addLog("error", `Flash failed: ${err}`);
    } finally {
      stopPolling();
      // Drain any remaining events
      await processEvents();
    }
  }, [
    state.firmwarePath,
    state.flashOptions,
    dispatch,
    addLog,
    startPolling,
    stopPolling,
    processEvents,
  ]);

  // ── Erase ────────────────────────────────────────────────────────────────

  const eraseChip = useCallback(async () => {
    dispatch({ type: "SET_FLASH_STATUS", status: "erasing" });
    dispatch({ type: "RESET_PROGRESS" });
    startPolling();
    addLog("info", "Starting chip erase...");

    try {
      const msg = await invoke<string>("erase_chip");
      dispatch({ type: "SET_FLASH_STATUS", status: "completed" });
      addLog("success", msg);
      setTimeout(() => {
        dispatch({ type: "SET_FLASH_STATUS", status: "idle" });
      }, 3000);
    } catch (err) {
      dispatch({ type: "SET_FLASH_STATUS", status: "error" });
      addLog("error", `Erase failed: ${err}`);
    } finally {
      stopPolling();
      await processEvents();
    }
  }, [dispatch, addLog, startPolling, stopPolling, processEvents]);

  // ── Verify ───────────────────────────────────────────────────────────────

  const verifyFirmware = useCallback(async () => {
    if (!state.firmwarePath) {
      addLog("error", "No firmware loaded");
      return;
    }

    dispatch({ type: "SET_FLASH_STATUS", status: "verifying" });
    dispatch({ type: "RESET_PROGRESS" });
    startPolling();
    addLog("info", "Starting verification...");

    try {
      const result = await invoke<VerifyResult>("verify_firmware", {
        path: state.firmwarePath,
        baseAddress: null,
      });

      dispatch({ type: "SET_FLASH_STATUS", status: "completed" });
      if (result.success) {
        addLog(
          "success",
          `Verification passed: ${result.bytes_verified} bytes verified`
        );
      } else {
        addLog(
          "error",
          `Verification failed: ${result.mismatch_count} mismatches`
        );
      }
      setTimeout(() => {
        dispatch({ type: "SET_FLASH_STATUS", status: "idle" });
      }, 3000);
    } catch (err) {
      dispatch({ type: "SET_FLASH_STATUS", status: "error" });
      addLog("error", `Verify failed: ${err}`);
    } finally {
      stopPolling();
      await processEvents();
    }
  }, [
    state.firmwarePath,
    dispatch,
    addLog,
    startPolling,
    stopPolling,
    processEvents,
  ]);

  // ── Reset ────────────────────────────────────────────────────────────────

  const resetTarget = useCallback(
    async (halt: boolean = false) => {
      dispatch({ type: "SET_FLASH_STATUS", status: "resetting" });
      addLog("info", halt ? "Resetting target (halt)..." : "Resetting target...");

      try {
        const msg = await invoke<string>("reset_target", { halt });
        dispatch({ type: "SET_FLASH_STATUS", status: "idle" });
        addLog("success", msg);
      } catch (err) {
        dispatch({ type: "SET_FLASH_STATUS", status: "error" });
        addLog("error", `Reset failed: ${err}`);
      }
    },
    [dispatch, addLog]
  );

  return {
    refreshProbes,
    connectProbe,
    disconnectProbe,
    autoDetectTarget,
    loadFirmware,
    flashFirmware,
    eraseChip,
    verifyFirmware,
    resetTarget,
  };
}
