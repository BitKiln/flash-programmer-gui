import { ConnectionPanel } from "./components/ConnectionPanel";
import { FirmwarePanel } from "./components/FirmwarePanel";
import { FlashControls } from "./components/FlashControls";
import { ConsoleOutput } from "./components/ConsoleOutput";
import { MemoryViewer } from "./components/MemoryViewer";
import { BatchPanel } from "./components/BatchPanel";
import { useState } from "react";

export function App() {
  const [tab, setTab] = useState<"firmware" | "memory" | "batch">("firmware");

  return (
    <div className="h-screen w-screen flex flex-col overflow-hidden">
      {/* Header */}
      <header className="flex items-center justify-between px-4 py-2 bg-bg-secondary border-b border-gray-700 shrink-0">
        <div className="flex items-center gap-3">
          <span className="text-lg">⚡</span>
          <h1 className="text-base font-bold text-gray-100 tracking-wide">
            Flash Programmer
          </h1>
        </div>
        <span className="text-xs text-gray-500 font-mono">v0.1.0</span>
      </header>

      {/* Main Content */}
      <div className="flex-1 flex overflow-hidden">
        {/* Left Sidebar — Connection */}
        <aside className="w-64 shrink-0 overflow-hidden">
          <ConnectionPanel />
        </aside>

        {/* Center Content */}
        <main className="flex-1 flex flex-col overflow-hidden">
          {/* Top — Firmware inspector or memory viewer */}
          <div className="flex gap-1 px-4 pt-3 shrink-0">
            {(["firmware", "memory", "batch"] as const).map((name) => (
              <button
                key={name}
                onClick={() => setTab(name)}
                className={`px-3 py-1 text-xs font-semibold rounded-t transition-colors ${
                  tab === name
                    ? "bg-bg-secondary text-gray-100"
                    : "text-gray-500 hover:text-gray-300"
                }`}
              >
                {name === "firmware"
                  ? "Firmware"
                  : name === "memory"
                    ? "Memory"
                    : "Batch"}
              </button>
            ))}
          </div>
          <div className="flex-1 overflow-y-auto">
            {tab === "firmware" ? (
              <FirmwarePanel />
            ) : tab === "memory" ? (
              <MemoryViewer />
            ) : (
              <BatchPanel />
            )}
          </div>

          {/* Bottom — Flash Controls */}
          <div className="shrink-0">
            <FlashControls />
          </div>
        </main>
      </div>

      {/* Bottom — Console */}
      <div className="h-52 shrink-0 overflow-hidden">
        <ConsoleOutput />
      </div>
    </div>
  );
}
