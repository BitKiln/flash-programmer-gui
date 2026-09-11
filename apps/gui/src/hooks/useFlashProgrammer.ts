import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useRef, useEffect } from "react";
import { useAppContext } from "../state/AppContext";
import type {
  ProbeInfo,
  TargetInfo,
  FirmwareInfo,
  FlashResult,
  VerifyResult,
  FlashEventDto,
  Profile,
  ProfileSummary,
  MemoryRead,
} from "../types";

/**
 * Custom hook wrapping all Tauri IPC invoke calls and flash telemetry.
 *
 * Provides imperative functions for probe discovery, connection, firmware
 * loading, flashing, erasing, verification, reset, and cancellation. Telemetry
 * arrives as `flash:progress` / `flash:status` / `flash:log` events pushed by
 * the backend; `get_flash_events` remains as a drain for anything buffered
 * before the subscription was established.
 */
export function useFlashProgrammer() {
  const { state, dispatch, addLog } = useAppContext();

  // ── Flash telemetry ──────────────────────────────────────────────────────

  const handleEvent = useCallback(
    (event: FlashEventDto) => {
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
          addLog("success", `[${event.stage}] Completed in ${event.duration_ms}ms`);
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
    },
    [dispatch, addLog]
  );

  const handleEventRef = useRef(handleEvent);
  handleEventRef.current = handleEvent;

  // Subscribe once for the lifetime of the hook: telemetry is pushed, so the
  // frontend no longer polls on a timer while an operation runs.
  useEffect(() => {
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    (async () => {
      for (const channel of ["flash:progress", "flash:status", "flash:log"]) {
        try {
          const stop = await listen<FlashEventDto>(channel, (event) => {
            handleEventRef.current(event.payload);
          });
          if (cancelled) {
            stop();
          } else {
            unlisteners.push(stop);
          }
        } catch {
          // Not running inside Tauri (browser preview or tests).
        }
      }
    })();

    return () => {
      cancelled = true;
      unlisteners.forEach((stop) => stop());
    };
  }, []);

  /// Drains events buffered before the subscription was established, or by a
  /// backend that could not emit.
  const drainBufferedEvents = useCallback(async () => {
    try {
      const events = await invoke<FlashEventDto[]>("get_flash_events");
      for (const event of events) {
        handleEventRef.current(event);
      }
    } catch {
      // No Tauri backend to drain.
    }
  }, []);

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

  // ── Cancellation ─────────────────────────────────────────────────────────

  /// Distinguishes a cancellation the user asked for from a genuine failure,
  /// which the backend reports through the same error channel.
  const reportFailure = useCallback(
    (err: unknown, prefix: string) => {
      const message = `${err}`;
      if (message.toLowerCase().includes("cancel")) {
        dispatch({ type: "SET_FLASH_STATUS", status: "cancelled" });
        addLog("warn", "Operation cancelled");
        setTimeout(() => {
          dispatch({ type: "SET_FLASH_STATUS", status: "idle" });
        }, 3000);
      } else {
        dispatch({ type: "SET_FLASH_STATUS", status: "error" });
        addLog("error", `${prefix}: ${message}`);
      }
    },
    [dispatch, addLog]
  );

  const cancelOperation = useCallback(async () => {
    dispatch({ type: "SET_FLASH_STATUS", status: "cancelling" });
    try {
      await invoke<string>("cancel_operation");
    } catch (err) {
      addLog("error", `Cancel request failed: ${err}`);
    }
  }, [dispatch, addLog]);

  // ── Memory ───────────────────────────────────────────────────────────────

  const readMemory = useCallback(
    async (address: number, length: number): Promise<MemoryRead | null> => {
      try {
        return await invoke<MemoryRead>("read_memory", { address, length });
      } catch (err) {
        addLog("error", `Memory read failed: ${err}`);
        return null;
      }
    },
    [addLog]
  );

  /// Bytes the loaded image places in a window, `null` where it covers nothing.
  const readFirmwareWindow = useCallback(
    async (address: number, length: number): Promise<(number | null)[] | null> => {
      if (!state.firmwarePath) return null;
      try {
        return await invoke<(number | null)[]>("read_firmware_window", {
          path: state.firmwarePath,
          baseAddress: null,
          address,
          length,
        });
      } catch (err) {
        addLog("error", `Firmware comparison failed: ${err}`);
        return null;
      }
    },
    [state.firmwarePath, addLog]
  );

  const saveMemoryRegion = useCallback(
    async (path: string, address: number, length: number): Promise<boolean> => {
      try {
        const message = await invoke<string>("save_memory_region", {
          path,
          address,
          length,
        });
        addLog("success", message);
        return true;
      } catch (err) {
        addLog("error", `Failed to save region: ${err}`);
        return false;
      }
    },
    [addLog]
  );

  // ── Profiles ─────────────────────────────────────────────────────────────
  //
  // Backed by the same TOML store the CLI uses, so a profile saved here works
  // from the command line too.

  const listProfiles = useCallback(async (): Promise<ProfileSummary[]> => {
    try {
      return await invoke<ProfileSummary[]>("list_profiles");
    } catch (err) {
      addLog("error", `Failed to list profiles: ${err}`);
      return [];
    }
  }, [addLog]);

  const loadProfile = useCallback(
    async (name: string): Promise<Profile | null> => {
      try {
        const profile = await invoke<Profile>("load_profile", { name });
        addLog("info", `Loaded profile "${profile.name}" (${profile.target})`);
        if (profile.firmware_path) {
          await loadFirmware(profile.firmware_path);
        }
        dispatch({
          type: "SET_FLASH_OPTIONS",
          options: {
            verify: profile.verify,
            reset: profile.reset,
            chipErase: profile.full_chip_erase,
          },
        });
        return profile;
      } catch (err) {
        addLog("error", `Failed to load profile: ${err}`);
        return null;
      }
    },
    [addLog, dispatch, loadFirmware]
  );

  const saveProfile = useCallback(
    async (profile: Profile): Promise<boolean> => {
      try {
        const path = await invoke<string>("save_profile", { profile });
        addLog("success", `Saved profile "${profile.name}" to ${path}`);
        return true;
      } catch (err) {
        addLog("error", `Failed to save profile: ${err}`);
        return false;
      }
    },
    [addLog]
  );

  const deleteProfile = useCallback(
    async (name: string): Promise<boolean> => {
      try {
        await invoke<string>("delete_profile", { name });
        addLog("info", `Deleted profile "${name}"`);
        return true;
      } catch (err) {
        addLog("error", `Failed to delete profile: ${err}`);
        return false;
      }
    },
    [addLog]
  );

  // ── Flash ────────────────────────────────────────────────────────────────

  const flashFirmware = useCallback(async () => {
    if (!state.firmwarePath) {
      addLog("error", "No firmware loaded");
      return;
    }

    dispatch({ type: "SET_FLASH_STATUS", status: "programming" });
    dispatch({ type: "RESET_PROGRESS" });
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
      reportFailure(err, "Flash failed");
    } finally {
      await drainBufferedEvents();
    }
  }, [
    state.firmwarePath,
    state.flashOptions,
    dispatch,
    addLog,
    drainBufferedEvents,
    reportFailure,
  ]);

  // ── Erase ────────────────────────────────────────────────────────────────

  const eraseChip = useCallback(async () => {
    dispatch({ type: "SET_FLASH_STATUS", status: "erasing" });
    dispatch({ type: "RESET_PROGRESS" });
    addLog("info", "Starting chip erase...");

    try {
      const msg = await invoke<string>("erase_chip");
      dispatch({ type: "SET_FLASH_STATUS", status: "completed" });
      addLog("success", msg);
      setTimeout(() => {
        dispatch({ type: "SET_FLASH_STATUS", status: "idle" });
      }, 3000);
    } catch (err) {
      reportFailure(err, "Erase failed");
    } finally {
      await drainBufferedEvents();
    }
  }, [dispatch, addLog, drainBufferedEvents, reportFailure]);

  // ── Verify ───────────────────────────────────────────────────────────────

  const verifyFirmware = useCallback(async () => {
    if (!state.firmwarePath) {
      addLog("error", "No firmware loaded");
      return;
    }

    dispatch({ type: "SET_FLASH_STATUS", status: "verifying" });
    dispatch({ type: "RESET_PROGRESS" });
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
      reportFailure(err, "Verify failed");
    } finally {
      await drainBufferedEvents();
    }
  }, [
    state.firmwarePath,
    dispatch,
    addLog,
    drainBufferedEvents,
    reportFailure,
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
    cancelOperation,
    readMemory,
    readFirmwareWindow,
    saveMemoryRegion,
    listProfiles,
    loadProfile,
    saveProfile,
    deleteProfile,
  };
}
