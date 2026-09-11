import { useEffect, useState } from "react";
import { useAppContext } from "../state/AppContext";
import { useFlashProgrammer } from "../hooks/useFlashProgrammer";

export function ConnectionPanel() {
  const { state, dispatch } = useAppContext();
  const { refreshProbes, connectProbe } = useFlashProgrammer();

  const [target, setTarget] = useState("stm32f401re");
  const [protocol, setProtocol] = useState("Swd");
  const [speed, setSpeed] = useState(4000);

  // Auto-refresh probes on mount
  useEffect(() => {
    refreshProbes();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const handleConnect = async () => {
    await connectProbe(state.selectedProbe, target, protocol, speed);
  };

  const isConnected = state.connectionStatus === "connected";
  const isConnecting = state.connectionStatus === "connecting";

  return (
    <div className="flex flex-col h-full p-4 space-y-4 bg-bg-secondary border-r border-gray-700">
      <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider">
        Connection
      </h2>

      {/* Probe Selection */}
      <div>
        <label className="block text-xs text-gray-400 mb-1">Debug Probe</label>
        <div className="flex gap-2">
          <select
            className="flex-1 bg-bg-primary border border-gray-600 rounded px-2 py-1.5 text-sm text-gray-200 focus:border-accent-red focus:outline-none"
            value={state.selectedProbe ?? ""}
            onChange={(e) =>
              dispatch({ type: "SELECT_PROBE", probeId: e.target.value || null })
            }
          >
            {state.probes.length === 0 && (
              <option value="">No probes found</option>
            )}
            {state.probes.map((p) => (
              <option key={p.identifier} value={p.identifier}>
                {p.product_name}
              </option>
            ))}
          </select>
          <button
            onClick={refreshProbes}
            className="px-2 py-1.5 bg-bg-tertiary border border-gray-600 rounded text-sm text-gray-300 hover:bg-gray-600 transition-colors"
            title="Refresh probes"
          >
            ⟳
          </button>
        </div>
        {state.selectedProbe && (
          <p className="text-xs text-gray-500 mt-1 font-mono truncate">
            {state.selectedProbe}
          </p>
        )}
      </div>

      {/* Target MCU */}
      <div>
        <label className="block text-xs text-gray-400 mb-1">Target MCU</label>
        <input
          type="text"
          className="w-full bg-bg-primary border border-gray-600 rounded px-2 py-1.5 text-sm text-gray-200 font-mono focus:border-accent-red focus:outline-none"
          value={target}
          onChange={(e) => setTarget(e.target.value)}
          placeholder="e.g. STM32F401RE"
        />
      </div>

      {/* Protocol */}
      <div>
        <label className="block text-xs text-gray-400 mb-1">Interface</label>
        <div className="flex gap-4">
          <label className="flex items-center gap-1.5 text-sm text-gray-300 cursor-pointer">
            <input
              type="radio"
              name="protocol"
              value="Swd"
              checked={protocol === "Swd"}
              onChange={(e) => setProtocol(e.target.value)}
              className="accent-accent-red"
            />
            SWD
          </label>
          <label className="flex items-center gap-1.5 text-sm text-gray-300 cursor-pointer">
            <input
              type="radio"
              name="protocol"
              value="Jtag"
              checked={protocol === "Jtag"}
              onChange={(e) => setProtocol(e.target.value)}
              className="accent-accent-red"
            />
            JTAG
          </label>
        </div>
      </div>

      {/* Speed */}
      <div>
        <label className="block text-xs text-gray-400 mb-1">Speed (kHz)</label>
        <select
          className="w-full bg-bg-primary border border-gray-600 rounded px-2 py-1.5 text-sm text-gray-200 focus:border-accent-red focus:outline-none"
          value={speed}
          onChange={(e) => setSpeed(Number(e.target.value))}
        >
          <option value={1000}>1000 kHz</option>
          <option value={2000}>2000 kHz</option>
          <option value={4000}>4000 kHz</option>
          <option value={8000}>8000 kHz</option>
        </select>
      </div>

      {/* Connect Button */}
      <button
        onClick={handleConnect}
        disabled={isConnecting || !state.selectedProbe}
        className={`w-full py-2 rounded text-sm font-medium transition-colors ${
          isConnected
            ? "bg-green-700 hover:bg-green-600 text-white"
            : isConnecting
              ? "bg-yellow-700 text-white cursor-wait"
              : "bg-accent-red hover:bg-red-500 text-white"
        } disabled:opacity-50 disabled:cursor-not-allowed`}
      >
        {isConnecting
          ? "Connecting..."
          : isConnected
            ? "Reconnect"
            : "Connect"}
      </button>

      {/* Status Indicator */}
      <div className="flex items-center gap-2">
        <div
          className={`w-2.5 h-2.5 rounded-full ${
            isConnected
              ? "bg-green-400 shadow-[0_0_6px_rgba(74,222,128,0.6)]"
              : state.connectionStatus === "error"
                ? "bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.6)]"
                : "bg-gray-500"
          }`}
        />
        <span className="text-xs text-gray-400">
          {isConnected
            ? `Connected — ${state.targetInfo?.name ?? "Unknown"}`
            : state.connectionStatus === "error"
              ? "Connection error"
              : state.connectionStatus === "connecting"
                ? "Connecting..."
                : "Disconnected"}
        </span>
      </div>

      {/* Target Info */}
      {state.targetInfo && (
        <div className="mt-auto pt-4 border-t border-gray-700 space-y-1">
          <h3 className="text-xs text-gray-400 uppercase tracking-wider">
            Target Info
          </h3>
          <div className="text-xs text-gray-300 font-mono space-y-0.5">
            <p>
              Arch: <span className="text-gray-100">{state.targetInfo.architecture}</span>
            </p>
            <p>
              Flash: <span className="text-gray-100">0x{state.targetInfo.flash_base.toString(16).toUpperCase().padStart(8, "0")}</span>{" "}
              ({(state.targetInfo.flash_size / 1024).toFixed(0)} KB)
            </p>
            <p>
              RAM: <span className="text-gray-100">0x{state.targetInfo.ram_base.toString(16).toUpperCase().padStart(8, "0")}</span>{" "}
              ({(state.targetInfo.ram_size / 1024).toFixed(0)} KB)
            </p>
            <p>
              Page: <span className="text-gray-100">{state.targetInfo.page_size}</span> B / {state.targetInfo.sector_count} sectors
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
