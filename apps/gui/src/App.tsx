import { ConnectionPanel } from "./components/ConnectionPanel";
import { FirmwarePanel } from "./components/FirmwarePanel";
import { FlashControls } from "./components/FlashControls";
import { ConsoleOutput } from "./components/ConsoleOutput";

export function App() {
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
        <aside className="w-64 shrink-0 overflow-y-auto">
          <ConnectionPanel />
        </aside>

        {/* Center Content */}
        <main className="flex-1 flex flex-col overflow-hidden">
          {/* Top — Firmware Panel */}
          <div className="flex-1 overflow-y-auto">
            <FirmwarePanel />
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
