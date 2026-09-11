import { useCallback, useState, DragEvent } from "react";
import { useAppContext } from "../state/AppContext";
import { useFlashProgrammer } from "../hooks/useFlashProgrammer";

export function FirmwarePanel() {
  const { state, dispatch } = useAppContext();
  const { loadFirmware } = useFlashProgrammer();
  const [isDragOver, setIsDragOver] = useState(false);

  const handleDrop = useCallback(
    async (e: DragEvent<HTMLDivElement>) => {
      e.preventDefault();
      setIsDragOver(false);
      const files = e.dataTransfer.files;
      if (files.length > 0) {
        const path = (files[0] as File & { path?: string }).path;
        if (path) {
          await loadFirmware(path);
        }
      }
    },
    [loadFirmware]
  );

  const handleBrowse = useCallback(async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: "Firmware Files",
            extensions: ["hex", "ihex", "bin", "elf", "axf", "out"],
          },
          { name: "All Files", extensions: ["*"] },
        ],
      });
      if (selected) {
        await loadFirmware(selected as string);
      }
    } catch (err) {
      // In browser mode (no Tauri), prompt for path
      const path = prompt("Enter firmware file path:");
      if (path) {
        await loadFirmware(path);
      }
    }
  }, [loadFirmware]);

  const handleRecentFileClick = useCallback(
    async (path: string) => {
      await loadFirmware(path);
    },
    [loadFirmware]
  );

  const formatAddress = (addr: number): string =>
    "0x" + addr.toString(16).toUpperCase().padStart(8, "0");

  const formatSize = (bytes: number): string => {
    if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
    if (bytes >= 1024) return `${(bytes / 1024).toFixed(2)} KB`;
    return `${bytes} B`;
  };

  return (
    <div className="flex flex-col h-full p-4 space-y-4">
      <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider">
        Firmware
      </h2>

      {/* Drop Zone */}
      <div
        className={`border-2 border-dashed rounded-lg p-6 text-center transition-colors cursor-pointer ${
          isDragOver
            ? "border-accent-red bg-accent-red/10"
            : "border-gray-600 hover:border-gray-500"
        }`}
        onDragOver={(e) => {
          e.preventDefault();
          setIsDragOver(true);
        }}
        onDragLeave={() => setIsDragOver(false)}
        onDrop={handleDrop}
        onClick={handleBrowse}
      >
        <div className="text-3xl mb-2">📄</div>
        <p className="text-sm text-gray-300">
          Drop firmware file here or{" "}
          <span className="text-accent-red hover:underline">browse</span>
        </p>
        <p className="text-xs text-gray-500 mt-1">
          Supports .hex, .ihex, .bin, .elf
        </p>
      </div>

      {/* Firmware Info */}
      {state.firmware && (
        <div className="bg-bg-primary rounded-lg p-3 border border-gray-700 space-y-2">
          <div className="flex items-center justify-between">
            <h3 className="text-xs text-gray-400 uppercase tracking-wider">
              Loaded Firmware
            </h3>
            <span className="text-xs px-2 py-0.5 bg-bg-tertiary rounded text-gray-300">
              {state.firmware.format}
            </span>
          </div>
          <div className="text-xs text-gray-300 font-mono space-y-1">
            {state.firmware.file_path && (
              <p className="truncate text-gray-400" title={state.firmware.file_path}>
                {state.firmware.file_path.split(/[\\/]/).pop()}
              </p>
            )}
            <div className="grid grid-cols-2 gap-x-4 gap-y-1">
              <p>
                Size:{" "}
                <span className="text-gray-100">
                  {formatSize(state.firmware.total_firmware_bytes)}
                </span>
              </p>
              <p>
                Segments:{" "}
                <span className="text-gray-100">
                  {state.firmware.segment_count}
                </span>
              </p>
              <p>
                Base:{" "}
                <span className="text-gray-100">
                  {formatAddress(state.firmware.base_address)}
                </span>
              </p>
              <p>
                End:{" "}
                <span className="text-gray-100">
                  {formatAddress(state.firmware.highest_address)}
                </span>
              </p>
              {state.firmware.entry_point !== null && (
                <p>
                  Entry:{" "}
                  <span className="text-gray-100">
                    {formatAddress(state.firmware.entry_point)}
                  </span>
                </p>
              )}
              <p>
                CRC32:{" "}
                <span className="text-gray-100">
                  0x{state.firmware.crc32.toString(16).toUpperCase().padStart(8, "0")}
                </span>
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Flash Options */}
      <div className="space-y-2">
        <h3 className="text-xs text-gray-400 uppercase tracking-wider">
          Flash Options
        </h3>
        <label className="flex items-center gap-2 text-sm text-gray-300 cursor-pointer">
          <input
            type="checkbox"
            checked={state.flashOptions.verify}
            onChange={(e) =>
              dispatch({
                type: "SET_FLASH_OPTIONS",
                options: { verify: e.target.checked },
              })
            }
            className="accent-accent-red"
          />
          Verify after programming
        </label>
        <label className="flex items-center gap-2 text-sm text-gray-300 cursor-pointer">
          <input
            type="checkbox"
            checked={state.flashOptions.reset}
            onChange={(e) =>
              dispatch({
                type: "SET_FLASH_OPTIONS",
                options: { reset: e.target.checked },
              })
            }
            className="accent-accent-red"
          />
          Reset after programming
        </label>
        <label
          className="flex items-center gap-2 text-sm text-gray-300 cursor-pointer"
          title="Mass-erases every sector, including those the firmware does not use. Much slower than the default, which erases only the sectors being written."
        >
          <input
            type="checkbox"
            checked={state.flashOptions.chipErase}
            onChange={(e) =>
              dispatch({
                type: "SET_FLASH_OPTIONS",
                options: { chipErase: e.target.checked },
              })
            }
            className="accent-accent-red"
          />
          Full chip erase (slow)
        </label>
      </div>

      {/* Recent Files */}
      {state.recentFiles.length > 0 && (
        <div className="mt-auto pt-4 border-t border-gray-700">
          <h3 className="text-xs text-gray-400 uppercase tracking-wider mb-2">
            Recent Files
          </h3>
          <ul className="space-y-1">
            {state.recentFiles.map((path) => (
              <li key={path}>
                <button
                  onClick={() => handleRecentFileClick(path)}
                  className="text-xs text-gray-400 hover:text-gray-200 truncate w-full text-left transition-colors"
                  title={path}
                >
                  📁 {path.split(/[\\/]/).pop()}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
