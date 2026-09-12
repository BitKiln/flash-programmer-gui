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
  TargetSuggestion,
  MemoryRead,
  FlashMapInfo,
  BatchOptions,
  BatchReport,
} from "../types";

/**
 * Custom hook wrapping all Tauri IPC invoke calls and flash telemetry.
 *
 * Provides imperative functions for probe discovery, connection, firmware
 * loading, flashing, erasing, verification, reset, and cancellation. Telemetry
 * arrives as `flash:progress` / `flash:status` / `flash:log` events pushed by
 * the backend.
 */
/**
 * Subscribes to the backend's pushed telemetry.
 *
 * Call this **once**, at the root. The events land in the shared reducer, so a
 * subscription per consuming component would add every log line once per
 * mounted component -- three copies of each line on the firmware tab.
 */
export function useFlashTelemetry() {
  const { dispatch, addLog } = useAppContext();

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
}

/**
 * Custom hook wrapping all Tauri IPC invoke calls.
 *
 * Telemetry is not subscribed to here: see `useFlashTelemetry`.
 */
export function useFlashProgrammer() {
  const { state, dispatch, addLog } = useAppContext();

  // A raw binary was parsed at an address the user can choose, so every later
  // command has to be told the same one. Re-parsing without it would put the
  // image back at the default and flash it to the wrong place.
  const loadedBaseAddress = useCallback(
    () =>
      state.firmware && state.firmware.format.toLowerCase().includes("raw")
        ? state.firmware.base_address
        : null,
    [state.firmware]
  );

  // ── Probe Discovery ──────────────────────────────────────────────────────

  const refreshProbes = useCallback(async () => {
    try {
      addLog("info", "Scanning for debug probes and serial ports...");
      const probes = await invoke<ProbeInfo[]>("list_probes");
      dispatch({ type: "SET_PROBES", probes });

      const hardwareProbes = probes.filter(
        (p) => !p.identifier.startsWith("mock:") && p.probe_type !== "VirtualMock"
      );
      // A serial port is not a debug probe, and calling one a probe in the log
      // makes it look as though a board has a debugger attached when it does
      // not.
      const serialPorts = hardwareProbes.filter((p) =>
        p.identifier.startsWith("esp:")
      );
      const debugProbes = hardwareProbes.filter(
        (p) => !p.identifier.startsWith("esp:")
      );
      const found = [
        debugProbes.length > 0
          ? `${debugProbes.length} debug probe(s): ${debugProbes.map((p) => p.product_name).join(", ")}`
          : null,
        serialPorts.length > 0
          ? `${serialPorts.length} serial port(s): ${serialPorts.map((p) => p.product_name).join(", ")}`
          : null,
      ].filter(Boolean);
      if (found.length > 0) {
        addLog("success", `Detected ${found.join("; ")}`);
      } else {
        addLog(
          "info",
          "No debug probe or serial port detected. (Simulator mode is available)"
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
      speed: number,
      /// Serial bootloader rate. Undefined for a debug probe, which has none.
      baud?: number
    ): Promise<string | null> => {
      dispatch({ type: "SET_CONNECTION_STATUS", status: "connecting" });
      addLog(
        "info",
        baud === undefined
          ? `Connecting to ${target} via ${protocol}...`
          : `Connecting to ${target} over the serial bootloader at ${baud} baud...`
      );

      try {
        const info = await invoke<TargetInfo>("connect_probe", {
          probeId,
          target,
          protocol,
          speed,
          baud,
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
      speed: number,
      baud?: number
    ): Promise<{ info: TargetInfo | null; error: string | null }> => {
      dispatch({ type: "SET_CONNECTION_STATUS", status: "connecting" });
      addLog(
        "info",
        baud === undefined
          ? `Auto-detecting connected MCU board via ${protocol}...`
          : "Asking the ESP bootloader which chip it is running on..."
      );

      try {
        const info = await invoke<TargetInfo>("auto_detect_target", {
          probeId,
          protocol,
          speed,
          baud,
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

        // A raw binary has no address of its own, so one was assumed. On an
        // ESP part every plausible offset is a valid address, which means a
        // wrong guess flashes cleanly and boots to nothing -- worth saying out
        // loud, since the alternative is a silent success.
        if (
          baseAddress === undefined &&
          info.format.toLowerCase().includes("raw") &&
          state.targetInfo?.flash_base === 0
        ) {
          addLog(
            "warn",
            `No address given for a raw binary, so it will be written at ` +
              `0x${info.base_address.toString(16).toUpperCase()}. On an ESP part a ` +
              `bootloader or combined image usually goes at 0x1000, an ESP-IDF ` +
              `application at 0x10000, and a merged Arduino export at 0x0. Set the ` +
              `base address above if that is not the one you want.`
          );
        }
      } catch (err) {
        addLog("error", `Failed to load firmware: ${err}`);
      }
    },
    [dispatch, addLog, state.targetInfo]
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

  /// The connected target's flash geometry, sector by sector.
  const readFlashMap = useCallback(async (): Promise<FlashMapInfo | null> => {
    try {
      return await invoke<FlashMapInfo>("flash_map");
    } catch (err) {
      addLog("error", `Could not read the flash map: ${err}`);
      return null;
    }
  }, [addLog]);

  /// Writes bytes straight onto the target bus: RAM, registers, option bytes.
  ///
  /// Not a flash path. Nothing is erased first, so the backend refuses an
  /// address inside flash rather than leaving a half-written sector.
  const writeMemory = useCallback(
    async (address: number, bytes: number[]): Promise<boolean> => {
      try {
        const message = await invoke<string>("write_memory", { address, bytes });
        addLog("success", message);
        return true;
      } catch (err) {
        addLog("error", `Memory write failed: ${err}`);
        return false;
      }
    },
    [addLog]
  );

  /// Whether the connected session can write memory at all. Asked before the
  /// editor is offered, so nobody types a value whose write would be refused.
  const canWriteMemory = useCallback(async (): Promise<boolean> => {
    try {
      return await invoke<boolean>("can_write_memory");
    } catch {
      return false;
    }
  }, []);

  /// Bytes the loaded image places in a window, `null` where it covers nothing.
  const readFirmwareWindow = useCallback(
    async (address: number, length: number): Promise<(number | null)[] | null> => {
      if (!state.firmwarePath) return null;
      try {
        return await invoke<(number | null)[]>("read_firmware_window", {
          path: state.firmwarePath,
          baseAddress: loadedBaseAddress(),
          address,
          length,
        });
      } catch (err) {
        addLog("error", `Firmware comparison failed: ${err}`);
        return null;
      }
    },
    [state.firmwarePath, loadedBaseAddress, addLog]
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

  // ── Device database ──────────────────────────────────────────────────────

  /// Target suggestions come from the backend's device database rather than a
  /// list baked into the UI, so a newly supported family needs no frontend
  /// change. An empty list is fine — the target field is free text.
  const listTargetSuggestions = useCallback(async (): Promise<TargetSuggestion[]> => {
    try {
      return await invoke<TargetSuggestion[]>("list_target_suggestions");
    } catch {
      // No Tauri backend (browser preview or tests).
      return [];
    }
  }, []);

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
        baseAddress: loadedBaseAddress(),
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
    }
  }, [
    state.firmwarePath,
    state.flashOptions,
    loadedBaseAddress,
    dispatch,
    addLog,
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
    }
  }, [dispatch, addLog, reportFailure]);

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
        baseAddress: loadedBaseAddress(),
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
    }
  }, [
    state.firmwarePath,
    loadedBaseAddress,
    dispatch,
    addLog,
    reportFailure,
  ]);

  // ── Batch (production) mode ──────────────────────────────────────────────

  /// Programs a run of boards. The backend owns the probe for the whole run, so
  /// the interactive session is dropped and the caller reconnects afterwards.
  const startBatch = useCallback(
    async (options: BatchOptions): Promise<BatchReport | null> => {
      dispatch({ type: "SET_FLASH_STATUS", status: "programming" });
      dispatch({ type: "RESET_PROGRESS" });
      addLog("info", "Starting batch run...");

      try {
        const report = await invoke<BatchReport>("start_batch", {
          path: options.path,
          baseAddress: options.baseAddress ?? loadedBaseAddress(),
          probeId: options.probeId,
          target: options.target,
          protocol: options.protocol,
          speed: options.speed,
          verify: options.verify,
          reset: options.reset,
          chipErase: options.chipErase,
          count: options.count,
          rearm: options.rearm,
          stopOnError: options.stopOnError,
          delayMs: options.delayMs,
          logPath: options.logPath,
          logJson: options.logJson,
          serialAddress: options.serialAddress,
          serialFormat: options.serialFormat,
          serialStart: options.serialStart,
          serialStep: options.serialStep,
          serialEncoding: options.serialEncoding,
          serialWidth: options.serialWidth,
        });

        dispatch({ type: "SET_CONNECTION_STATUS", status: "disconnected" });
        dispatch({ type: "SET_FLASH_STATUS", status: report.failed > 0 ? "error" : "completed" });
        addLog(
          report.failed > 0 ? "warn" : "success",
          `Batch finished: ${report.passed} passed, ${report.failed} failed (${report.stop_reason})`
        );
        if (report.log_path) {
          addLog("info", `Batch log written to ${report.log_path}`);
        }
        setTimeout(() => {
          dispatch({ type: "SET_FLASH_STATUS", status: "idle" });
        }, 3000);
        return report;
      } catch (err) {
        reportFailure(err, "Batch failed");
        return null;
      }
    },
    [loadedBaseAddress, dispatch, addLog, reportFailure]
  );

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
    startBatch,
    eraseChip,
    verifyFirmware,
    resetTarget,
    cancelOperation,
    readMemory,
    readFlashMap,
    writeMemory,
    canWriteMemory,
    readFirmwareWindow,
    saveMemoryRegion,
    listTargetSuggestions,
    listProfiles,
    loadProfile,
    saveProfile,
    deleteProfile,
  };
}
