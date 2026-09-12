import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useAppContext } from "../state/AppContext";
import { useFlashProgrammer } from "../hooks/useFlashProgrammer";
import type { BatchEventDto, BatchSettings, BatchUnit } from "../types";

/**
 * Production mode: program the same firmware onto a run of boards.
 *
 * The run owns the probe from start to finish, so the interactive session is
 * dropped when it begins and the Connection panel shows disconnected when it
 * ends. Stopping is the same Stop the rest of the app uses: the board in flight
 * finishes or aborts at a block boundary, and the run ends with it.
 */
export function BatchPanel() {
  const { state, dispatch } = useAppContext();
  const { startBatch, cancelOperation } = useFlashProgrammer();

  // The form is shared state: an operator who steps over to the Memory tab
  // mid-shift comes back to the run they set up, not a blank form.
  const settings = state.batchSettings;
  const update = (changed: Partial<BatchSettings>) =>
    dispatch({ type: "SET_BATCH_SETTINGS", settings: changed });

  const {
    target,
    count,
    protocol,
    speed,
    rearm,
    delayMs,
    logPath,
    logJson,
    stopOnError,
    serialAddress,
    serialFormat,
    serialStart,
    serialStep,
    serialEncoding,
    serialWidth,
  } = settings;

  const [units, setUnits] = useState<BatchUnit[]>([]);
  const [waiting, setWaiting] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [summary, setSummary] = useState<string | null>(null);
  const tableEnd = useRef<HTMLDivElement | null>(null);

  // Adopt the connected target once, so the operator does not retype it.
  useEffect(() => {
    if (state.targetInfo && target === "") {
      update({ target: state.targetInfo.name });
    }
  }, [state.targetInfo, target]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    let cancelled = false;
    let stop: UnlistenFn | undefined;

    (async () => {
      try {
        const unlisten = await listen<BatchEventDto>("batch:event", (event) => {
          const payload = event.payload;
          switch (payload.type) {
            case "WaitingForDetach":
              setWaiting(`Disconnect board ${payload.index - 1}`);
              break;
            case "WaitingForAttach":
              setWaiting(`Connect board ${payload.index}`);
              break;
            case "UnitStarted":
              setWaiting(null);
              break;
            case "UnitFinished":
              setUnits((previous) => [...previous, payload.unit]);
              break;
            case "Finished":
              setWaiting(null);
              setSummary(
                `${payload.passed} passed, ${payload.failed} failed (${payload.stop_reason})`
              );
              break;
          }
        });
        if (cancelled) {
          unlisten();
        } else {
          stop = unlisten;
        }
      } catch {
        // Not running inside Tauri (browser preview or tests).
      }
    })();

    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  useEffect(() => {
    tableEnd.current?.scrollIntoView({ block: "nearest" });
  }, [units.length]);

  const trimmedSerialAddress = serialAddress.trim();
  // Addresses are written the way the datasheet writes them: hex, with or
  // without the 0x.
  const parsedSerialAddress =
    trimmedSerialAddress === ""
      ? null
      : Number.parseInt(trimmedSerialAddress.replace(/^0[xX]/, ""), 16);
  const serialAddressInvalid =
    trimmedSerialAddress !== "" &&
    (parsedSerialAddress === null || Number.isNaN(parsedSerialAddress));

  const parsedCount = count.trim() === "" ? null : Number.parseInt(count, 10);
  const countInvalid =
    parsedCount !== null && (Number.isNaN(parsedCount) || parsedCount < 1);
  const canStart =
    !running &&
    state.firmwarePath !== null &&
    target.trim() !== "" &&
    !countInvalid &&
    !serialAddressInvalid;

  const onStart = async () => {
    if (!state.firmwarePath) {
      return;
    }
    setUnits([]);
    setSummary(null);
    setRunning(true);
    try {
      await startBatch({
        path: state.firmwarePath,
        baseAddress: null,
        probeId: state.selectedProbe,
        target: target.trim(),
        protocol,
        speed,
        verify: state.flashOptions.verify,
        reset: state.flashOptions.reset,
        chipErase: state.flashOptions.chipErase,
        count: parsedCount,
        rearm,
        stopOnError,
        delayMs: Number.parseInt(delayMs, 10) || 0,
        logPath: logPath.trim() === "" ? null : logPath.trim(),
        logJson,
        serialAddress: parsedSerialAddress,
        serialFormat,
        serialStart: Number.parseInt(serialStart, 10) || 0,
        serialStep: Number.parseInt(serialStep, 10) || 1,
        serialEncoding,
        serialWidth: Number.parseInt(serialWidth, 10) || 16,
      });
    } finally {
      setRunning(false);
      setWaiting(null);
    }
  };

  const passed = units.filter((unit) => unit.status === "passed").length;
  const failed = units.length - passed;

  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider">
          Batch / Production
        </h2>
        <div className="flex items-center gap-2 text-xs font-mono">
          <span className="px-2 py-0.5 rounded bg-green-900/50 text-green-400">
            {passed} pass
          </span>
          <span className="px-2 py-0.5 rounded bg-red-900/50 text-red-400">
            {failed} fail
          </span>
        </div>
      </div>

      {state.firmwarePath === null && (
        <p className="text-xs text-yellow-400">
          Load a firmware file before starting a run.
        </p>
      )}

      <div className="grid grid-cols-2 gap-3 text-xs">
        <label className="flex flex-col gap-1">
          <span className="text-gray-400">Target</span>
          <input
            aria-label="Batch target"
            value={target}
            onChange={(event) => update({ target: event.target.value })}
            placeholder="auto"
            className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
          />
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-gray-400">Boards (blank = until stopped)</span>
          <input
            aria-label="Board count"
            value={count}
            onChange={(event) => update({ count: event.target.value })}
            className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
          />
          {countInvalid && (
            <span className="text-red-400">Enter a count of 1 or more.</span>
          )}
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-gray-400">Interface</span>
          <select
            aria-label="Batch interface"
            value={protocol}
            onChange={(event) => update({ protocol: event.target.value })}
            className="bg-bg-primary border border-gray-700 rounded px-2 py-1"
          >
            <option value="Swd">SWD</option>
            <option value="Jtag">JTAG</option>
          </select>
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-gray-400">Speed (kHz)</span>
          <input
            aria-label="Batch speed"
            type="number"
            value={speed}
            onChange={(event) =>
              update({ speed: Number.parseInt(event.target.value, 10) || 0 })
            }
            className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
          />
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-gray-400">Next board</span>
          <select
            aria-label="Rearm policy"
            value={rearm}
            onChange={(event) =>
              update({ rearm: event.target.value as "detach" | "immediate" })
            }
            className="bg-bg-primary border border-gray-700 rounded px-2 py-1"
          >
            <option value="detach">Wait for swap</option>
            <option value="immediate">Program immediately</option>
          </select>
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-gray-400">Pause between boards (ms)</span>
          <input
            aria-label="Delay between boards"
            type="number"
            value={delayMs}
            onChange={(event) => update({ delayMs: event.target.value })}
            className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
          />
        </label>

        <label className="flex flex-col gap-1 col-span-2">
          <span className="text-gray-400">Production log file (optional)</span>
          <input
            aria-label="Log file"
            value={logPath}
            onChange={(event) => update({ logPath: event.target.value })}
            placeholder="C:\\runs\\batch-2026-09-12.csv"
            className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
          />
        </label>

        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={logJson}
            onChange={(event) => update({ logJson: event.target.checked })}
          />
          <span className="text-gray-400">Write the log as JSON</span>
        </label>

        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={stopOnError}
            onChange={(event) => update({ stopOnError: event.target.checked })}
          />
          <span className="text-gray-400">Stop on first failure</span>
        </label>
      </div>

      <fieldset className="border border-gray-700 rounded p-3 space-y-3">
        <legend className="px-1 text-xs text-gray-400 uppercase tracking-wider">
          Serial number (optional)
        </legend>
        <div className="grid grid-cols-2 gap-3 text-xs">
          <label className="flex flex-col gap-1">
            <span className="text-gray-400">Flash address (hex)</span>
            <input
              aria-label="Serial address"
              value={serialAddress}
              onChange={(event) => update({ serialAddress: event.target.value })}
              placeholder="0801F800"
              className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
            />
            {serialAddressInvalid && (
              <span className="text-red-400">Enter a hex address, e.g. 0801F800.</span>
            )}
          </label>

          <label className="flex flex-col gap-1">
            <span className="text-gray-400">Template</span>
            <input
              aria-label="Serial template"
              value={serialFormat}
              onChange={(event) => update({ serialFormat: event.target.value })}
              className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
            />
          </label>

          <label className="flex flex-col gap-1">
            <span className="text-gray-400">First value</span>
            <input
              aria-label="Serial start"
              value={serialStart}
              onChange={(event) => update({ serialStart: event.target.value })}
              className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
            />
          </label>

          <label className="flex flex-col gap-1">
            <span className="text-gray-400">Step</span>
            <input
              aria-label="Serial step"
              value={serialStep}
              onChange={(event) => update({ serialStep: event.target.value })}
              className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
            />
          </label>

          <label className="flex flex-col gap-1">
            <span className="text-gray-400">Encoding</span>
            <select
              aria-label="Serial encoding"
              value={serialEncoding}
              onChange={(event) =>
                update({
                  serialEncoding: event.target.value as
                    | "ascii"
                    | "u32le"
                    | "u32be"
                    | "u64le",
                })
              }
              className="bg-bg-primary border border-gray-700 rounded px-2 py-1"
            >
              <option value="ascii">ASCII text</option>
              <option value="u32le">u32 little-endian</option>
              <option value="u32be">u32 big-endian</option>
              <option value="u64le">u64 little-endian</option>
            </select>
          </label>

          <label className="flex flex-col gap-1">
            <span className="text-gray-400">Field width (ASCII)</span>
            <input
              aria-label="Serial width"
              value={serialWidth}
              onChange={(event) => update({ serialWidth: event.target.value })}
              className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
            />
          </label>
        </div>
      </fieldset>

      <div className="flex items-center gap-2">
        <button
          onClick={onStart}
          disabled={!canStart}
          className="px-3 py-1.5 text-xs font-semibold rounded bg-blue-700 hover:bg-blue-600 disabled:opacity-40 disabled:cursor-not-allowed"
        >
          Start run
        </button>
        <button
          onClick={cancelOperation}
          disabled={!running}
          className="px-3 py-1.5 text-xs font-semibold rounded bg-red-800 hover:bg-red-700 disabled:opacity-40 disabled:cursor-not-allowed"
        >
          Stop
        </button>
        {waiting && (
          <span className="text-xs text-yellow-400 font-medium">{waiting}...</span>
        )}
        {summary && <span className="text-xs text-gray-300">{summary}</span>}
      </div>

      <div className="border border-gray-700 rounded overflow-auto max-h-64">
        <table className="w-full text-xs font-mono">
          <thead className="bg-bg-secondary text-gray-400 sticky top-0">
            <tr>
              <th className="text-left px-2 py-1">#</th>
              <th className="text-left px-2 py-1">Result</th>
              <th className="text-left px-2 py-1">Serial</th>
              <th className="text-right px-2 py-1">Bytes</th>
              <th className="text-right px-2 py-1">Time</th>
              <th className="text-left px-2 py-1">Detail</th>
            </tr>
          </thead>
          <tbody>
            {units.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-2 py-3 text-center text-gray-500">
                  No boards programmed yet.
                </td>
              </tr>
            ) : (
              units.map((unit) => (
                <tr key={unit.index} className="border-t border-gray-800">
                  <td className="px-2 py-1">{unit.index}</td>
                  <td
                    className={`px-2 py-1 font-semibold ${
                      unit.status === "passed" ? "text-green-400" : "text-red-400"
                    }`}
                  >
                    {unit.status === "passed" ? "PASS" : "FAIL"}
                  </td>
                  <td className="px-2 py-1 text-gray-300">{unit.serial ?? "—"}</td>
                  <td className="px-2 py-1 text-right">{unit.bytes_flashed}</td>
                  <td className="px-2 py-1 text-right">{unit.duration_ms} ms</td>
                  <td className="px-2 py-1 text-gray-400 truncate max-w-xs">
                    {unit.message}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
        <div ref={tableEnd} />
      </div>
    </div>
  );
}
