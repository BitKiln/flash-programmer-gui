import { useCallback, useEffect, useMemo, useState } from "react";
import { useFlashProgrammer } from "../hooks/useFlashProgrammer";
import type { HistoryRecord } from "../types";

/** Epoch milliseconds as a local date and time, for reading on this machine. */
export function formatWhen(timestampMs: number): string {
  const when = new Date(timestampMs);
  if (Number.isNaN(when.getTime())) return "unknown";
  return when.toLocaleString();
}

const outcomeClass = (outcome: string): string =>
  outcome === "succeeded"
    ? "text-green-400"
    : outcome === "cancelled"
      ? "text-yellow-400"
      : "text-accent-red";

/** The last path segment, since a full build path crowds out everything else. */
export function fileName(path: string | null): string {
  if (!path) return "";
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

/**
 * What has been programmed, most recent first.
 *
 * The records come from the same file the CLI writes, so a board programmed by
 * a script and one programmed by hand appear in one list. Simulator runs are
 * not recorded: a history that a production line reads must not contain runs
 * that never touched a board.
 */
export function HistoryPanel() {
  const { readHistory } = useFlashProgrammer();

  const [records, setRecords] = useState<HistoryRecord[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [failuresOnly, setFailuresOnly] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setRecords(await readHistory());
    } finally {
      setLoading(false);
    }
  }, [readHistory]);

  useEffect(() => {
    void load();
  }, [load]);

  const shown = useMemo(
    () =>
      (records ?? []).filter((r) => !failuresOnly || r.outcome !== "succeeded"),
    [records, failuresOnly]
  );

  return (
    <div className="flex flex-col h-full p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider">
          History
        </h2>
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-2 text-xs text-gray-400 cursor-pointer">
            <input
              type="checkbox"
              checked={failuresOnly}
              onChange={(e) => setFailuresOnly(e.target.checked)}
              className="accent-accent-red"
            />
            Failures only
          </label>
          <button
            onClick={() => void load()}
            disabled={loading}
            className="px-2.5 py-1 bg-bg-tertiary border border-gray-600 rounded text-xs text-gray-300 hover:bg-gray-600 transition-colors disabled:opacity-40"
          >
            {loading ? "Reading..." : "Refresh"}
          </button>
        </div>
      </div>

      {records !== null && records.length === 0 && (
        <p className="text-xs text-gray-500">
          Nothing recorded yet. A record is written whenever a real target is
          flashed, erased or verified — from this application or from the CLI.
          Simulator runs are left out, because they say nothing about a board.
        </p>
      )}

      {records !== null && records.length > 0 && shown.length === 0 && (
        <p className="text-xs text-gray-500">
          No failures in the {records.length} most recent records.
        </p>
      )}

      {shown.length > 0 && (
        <div className="flex-1 overflow-auto rounded border border-gray-700">
          <table className="w-full text-xs">
            <thead className="bg-bg-tertiary text-gray-400 sticky top-0">
              <tr>
                <th className="px-3 py-1.5 text-left font-medium">When</th>
                <th className="px-3 py-1.5 text-left font-medium">What</th>
                <th className="px-3 py-1.5 text-left font-medium">Target</th>
                <th className="px-3 py-1.5 text-left font-medium">Image</th>
                <th className="px-3 py-1.5 text-left font-medium">Result</th>
              </tr>
            </thead>
            <tbody className="text-gray-300">
              {shown.map((record, index) => (
                <tr
                  key={`${record.timestamp_ms}-${index}`}
                  className="border-b border-gray-800 last:border-0 align-top"
                >
                  <td className="px-3 py-1 whitespace-nowrap text-gray-500">
                    {formatWhen(record.timestamp_ms)}
                  </td>
                  <td className="px-3 py-1 whitespace-nowrap">
                    {record.operation}
                    {record.serial && (
                      <span className="text-gray-500"> · {record.serial}</span>
                    )}
                  </td>
                  <td className="px-3 py-1 whitespace-nowrap font-mono">
                    {record.target}
                    {record.probe && (
                      <span className="text-gray-600"> · {record.probe}</span>
                    )}
                  </td>
                  <td className="px-3 py-1 font-mono">
                    {/* The checksum, not the file name, is what says which
                        build this was: the same path holds a different image
                        after every rebuild. */}
                    {record.image_crc32 !== null &&
                    record.image_crc32 !== undefined ? (
                      <span title={record.file_path ?? undefined}>
                        {fileName(record.file_path)}{" "}
                        <span className="text-gray-500">
                          crc32{" "}
                          {record.image_crc32
                            .toString(16)
                            .toUpperCase()
                            .padStart(8, "0")}
                        </span>
                      </span>
                    ) : (
                      <span className="text-gray-600">—</span>
                    )}
                  </td>
                  <td className="px-3 py-1">
                    <span className={outcomeClass(record.outcome)}>
                      {record.outcome}
                    </span>
                    {record.outcome === "succeeded" && !record.verified && (
                      <span className="text-gray-500" title="Not verified against the target">
                        {" "}
                        unverified
                      </span>
                    )}
                    {record.outcome !== "succeeded" && record.message && (
                      <span className="text-gray-500"> — {record.message}</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
