import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useAppContext } from "../state/AppContext";
import { useFlashProgrammer } from "../hooks/useFlashProgrammer";
import type { BatchEventDto, BatchUnit } from "../types";

/**
 * Production mode: program the same firmware onto a run of boards.
 *
 * The run owns the probe from start to finish, so the interactive session is
 * dropped when it begins and the Connection panel shows disconnected when it
 * ends. Stopping is the same Stop the rest of the app uses: the board in flight
 * finishes or aborts at a block boundary, and the run ends with it.
 */
export function BatchPanel() {
  const { state } = useAppContext();
  const { startBatch, cancelOperation } = useFlashProgrammer();

  const [count, setCount] = useState<string>("10");
  const [rearm, setRearm] = useState<"detach" | "immediate">("detach");
  const [stopOnError, setStopOnError] = useState(false);
  const [delayMs, setDelayMs] = useState<string>("0");
  const [logPath, setLogPath] = useState("");
  const [logJson, setLogJson] = useState(false);
  const [target, setTarget] = useState("");
  const [protocol, setProtocol] = useState("Swd");
  const [speed, setSpeed] = useState(4000);

  const [units, setUnits] = useState<BatchUnit[]>([]);
  const [waiting, setWaiting] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [summary, setSummary] = useState<string | null>(null);
  const tableEnd = useRef<HTMLDivElement | null>(null);

  // Adopt the connected target once, so the operator does not retype it.
  useEffect(() => {
    if (state.targetInfo && target === "") {
      setTarget(state.targetInfo.name);
    }
  }, [state.targetInfo, target]);

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

  const parsedCount = count.trim() === "" ? null : Number.parseInt(count, 10);
  const countInvalid =
    parsedCount !== null && (Number.isNaN(parsedCount) || parsedCount < 1);
  const canStart =
    !running && state.firmwarePath !== null && target.trim() !== "" && !countInvalid;

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
            onChange={(event) => setTarget(event.target.value)}
            placeholder="auto"
            className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
          />
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-gray-400">Boards (blank = until stopped)</span>
          <input
            aria-label="Board count"
            value={count}
            onChange={(event) => setCount(event.target.value)}
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
            onChange={(event) => setProtocol(event.target.value)}
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
            onChange={(event) => setSpeed(Number.parseInt(event.target.value, 10) || 0)}
            className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
          />
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-gray-400">Next board</span>
          <select
            aria-label="Rearm policy"
            value={rearm}
            onChange={(event) => setRearm(event.target.value as "detach" | "immediate")}
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
            onChange={(event) => setDelayMs(event.target.value)}
            className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
          />
        </label>

        <label className="flex flex-col gap-1 col-span-2">
          <span className="text-gray-400">Production log file (optional)</span>
          <input
            aria-label="Log file"
            value={logPath}
            onChange={(event) => setLogPath(event.target.value)}
            placeholder="C:\\runs\\batch-2026-09-12.csv"
            className="bg-bg-primary border border-gray-700 rounded px-2 py-1 font-mono"
          />
        </label>

        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={logJson}
            onChange={(event) => setLogJson(event.target.checked)}
          />
          <span className="text-gray-400">Write the log as JSON</span>
        </label>

        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={stopOnError}
            onChange={(event) => setStopOnError(event.target.checked)}
          />
          <span className="text-gray-400">Stop on first failure</span>
        </label>
      </div>

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
              <th className="text-right px-2 py-1">Bytes</th>
              <th className="text-right px-2 py-1">Time</th>
              <th className="text-left px-2 py-1">Detail</th>
            </tr>
          </thead>
          <tbody>
            {units.length === 0 ? (
              <tr>
                <td colSpan={5} className="px-2 py-3 text-center text-gray-500">
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
