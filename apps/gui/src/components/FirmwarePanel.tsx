import { useCallback, useEffect, useRef, useState } from "react";
import { useAppContext } from "../state/AppContext";
import { useFlashProgrammer } from "../hooks/useFlashProgrammer";

/** A raw .bin is the only format that carries no addresses of its own. */
function isRawBinary(format: string): boolean {
  return format.toLowerCase().includes("raw");
}

export function FirmwarePanel() {
  const { state, dispatch } = useAppContext();
  const { loadFirmware } = useFlashProgrammer();
  const [baseInput, setBaseInput] = useState("");
  const [baseError, setBaseError] = useState<string | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);
  const applyBase = async () => {
    const path = state.firmwarePath;
    if (!path) {
      return;
    }
    const text = baseInput.trim();
    if (text === "") {
      setBaseError(null);
      await loadFirmware(path);
      return;
    }
    const parsed = Number(text.startsWith("0x") || text.startsWith("0X") ? text : `0x${text}`);
    if (!Number.isInteger(parsed) || parsed < 0) {
      setBaseError(`"${text}" is not an address. Try 0x10000.`);
      return;
    }
    setBaseError(null);
    await loadFirmware(path, parsed);
  };

  const loadFirmwareRef = useRef(loadFirmware);
  loadFirmwareRef.current = loadFirmware;

  // A file dropped onto a webview exposes no filesystem path (`File.path` is
  // undefined under Tauri v2), so the drop has to come from Tauri's own
  // drag-drop event, which carries real paths. The HTML5 handlers below only
  // suppress the webview's default "open the file" behaviour.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    (async () => {
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        const stop = await getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type === "over") {
            setIsDragOver(true);
          } else if (event.payload.type === "drop") {
            setIsDragOver(false);
            const [path] = event.payload.paths;
            if (path) {
              void loadFirmwareRef.current(path);
            }
          } else {
            setIsDragOver(false);
          }
        });
        if (cancelled) {
          stop();
        } else {
          unlisten = stop;
        }
      } catch {
        // Not running inside a Tauri webview (browser preview or tests):
        // the browse button remains the way to load firmware.
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

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
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => e.preventDefault()}
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

      {/* A raw binary carries no addresses, so the assumed one is editable. */}
      {state.firmware && isRawBinary(state.firmware.format) && (
        <div className="bg-bg-primary rounded-lg p-3 border border-gray-700 space-y-1.5">
          <label
            htmlFor="base-address"
            className="block text-xs text-gray-400 uppercase tracking-wider"
          >
            Base address
          </label>
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void applyBase();
            }}
          >
            <input
              id="base-address"
              className="flex-1 min-w-0 bg-bg-secondary border border-gray-600 rounded px-2 py-1 text-sm font-mono text-gray-200 focus:border-accent-red focus:outline-none"
              value={baseInput}
              placeholder={formatAddress(state.firmware.base_address)}
              onChange={(e) => setBaseInput(e.target.value)}
            />
            <button
              type="submit"
              className="shrink-0 px-2.5 py-1 bg-bg-tertiary border border-gray-600 rounded text-sm text-gray-300 hover:bg-gray-600 transition-colors"
            >
              Apply
            </button>
          </form>
          {baseError ? (
            <p className="text-xs text-red-400">{baseError}</p>
          ) : (
            <p className="text-xs text-gray-500">
              A .bin has no addresses of its own, so this one is assumed. It
              defaults to the connected target&apos;s flash base.
              {state.targetInfo?.flash_base === 0 && (
                <>
                  {" "}
                  On an ESP part: <span className="font-mono">0x1000</span>{" "}
                  bootloader or combined image,{" "}
                  <span className="font-mono">0x10000</span> ESP-IDF
                  application, <span className="font-mono">0x0</span> merged
                  Arduino export.
                </>
              )}
            </p>
          )}
        </div>
      )}

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
                <p title={`Source: ${state.firmware.entry_point_source}`}>
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

      {/* Segment Inspector */}
      {state.firmware && state.firmware.segments.length > 0 && (
        <div className="bg-bg-primary rounded-lg border border-gray-700">
          <h3 className="text-xs text-gray-400 uppercase tracking-wider px-3 pt-3 pb-2">
            Segments
          </h3>
          <div className="max-h-48 overflow-auto">
            <table className="w-full text-xs font-mono">
              <thead className="sticky top-0 bg-bg-primary text-gray-500">
                <tr className="text-left">
                  <th className="px-3 py-1 font-normal">#</th>
                  <th className="px-3 py-1 font-normal">Start</th>
                  <th className="px-3 py-1 font-normal">End</th>
                  <th className="px-3 py-1 font-normal text-right">Size</th>
                  <th className="px-3 py-1 font-normal text-right">CRC32</th>
                </tr>
              </thead>
              <tbody className="text-gray-300">
                {state.firmware.segments.map((segment) => (
                  <tr key={segment.index} className="border-t border-gray-800">
                    <td className="px-3 py-1 text-gray-500">{segment.index}</td>
                    <td className="px-3 py-1">{formatAddress(segment.start_address)}</td>
                    <td className="px-3 py-1">{formatAddress(segment.end_address)}</td>
                    <td className="px-3 py-1 text-right">{formatSize(segment.size_bytes)}</td>
                    <td className="px-3 py-1 text-right text-gray-400">{segment.crc32}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {state.firmware.gaps.length > 0 && (
            <p className="px-3 py-2 text-xs text-gray-500 border-t border-gray-800">
              {state.firmware.gaps.length} gap(s),{" "}
              {formatSize(
                state.firmware.gaps.reduce((total, gap) => total + gap.size, 0)
              )}{" "}
              unwritten between segments
            </p>
          )}
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
