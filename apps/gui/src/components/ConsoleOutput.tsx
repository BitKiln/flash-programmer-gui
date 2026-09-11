import { useEffect, useRef } from "react";
import { useAppContext } from "../state/AppContext";
import type { LogLevel } from "../types";

const levelColors: Record<LogLevel, string> = {
  info: "text-gray-300",
  warn: "text-yellow-400",
  error: "text-red-400",
  success: "text-green-400",
  debug: "text-blue-400",
  trace: "text-gray-500",
};

const levelBadgeColors: Record<LogLevel, string> = {
  info: "bg-gray-600",
  warn: "bg-yellow-800",
  error: "bg-red-900",
  success: "bg-green-900",
  debug: "bg-blue-900",
  trace: "bg-gray-700",
};

function formatTimestamp(date: Date): string {
  const h = date.getHours().toString().padStart(2, "0");
  const m = date.getMinutes().toString().padStart(2, "0");
  const s = date.getSeconds().toString().padStart(2, "0");
  const ms = date.getMilliseconds().toString().padStart(3, "0");
  return `${h}:${m}:${s}.${ms}`;
}

export function ConsoleOutput() {
  const { state, dispatch } = useAppContext();
  const bottomRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when new logs arrive
  useEffect(() => {
    bottomRef.current?.scrollIntoView?.({ behavior: "smooth" });
  }, [state.logs]);

  return (
    <div className="flex flex-col h-full border-t border-gray-700">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 bg-bg-secondary border-b border-gray-700">
        <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider">
          Console
        </h2>
        <div className="flex items-center gap-2">
          <span className="text-xs text-gray-500">
            {state.logs.length} entries
          </span>
          <button
            onClick={() => dispatch({ type: "CLEAR_LOGS" })}
            className="px-2 py-0.5 text-xs text-gray-400 hover:text-gray-200 bg-bg-primary border border-gray-600 rounded transition-colors"
          >
            Clear
          </button>
        </div>
      </div>

      {/* Log List */}
      <div
        ref={containerRef}
        className="flex-1 overflow-y-auto p-2 bg-bg-primary font-mono text-xs"
      >
        {state.logs.length === 0 ? (
          <p className="text-gray-600 text-center py-4">
            No log entries yet. Connect to a probe to get started.
          </p>
        ) : (
          state.logs.map((entry) => (
            <div
              key={entry.id}
              className="flex items-start gap-2 py-0.5 hover:bg-white/5 px-1 rounded"
            >
              <span className="text-gray-600 shrink-0 select-none">
                {formatTimestamp(entry.timestamp)}
              </span>
              <span
                className={`text-[10px] px-1.5 py-0 rounded uppercase font-semibold shrink-0 ${levelBadgeColors[entry.level]}`}
              >
                {entry.level === "success" ? "ok" : entry.level.slice(0, 4)}
              </span>
              <span className={`${levelColors[entry.level]} break-all`}>
                {entry.message}
              </span>
            </div>
          ))
        )}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
