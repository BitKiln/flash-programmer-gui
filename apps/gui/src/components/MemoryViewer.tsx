import { useCallback, useEffect, useState } from "react";
import { useAppContext } from "../state/AppContext";
import { useFlashProgrammer } from "../hooks/useFlashProgrammer";

const BYTES_PER_ROW = 16;
const PAGE_SIZE = 256;

const formatAddress = (address: number): string =>
  "0x" + (address >>> 0).toString(16).toUpperCase().padStart(8, "0");

const formatByte = (byte: number): string =>
  byte.toString(16).toUpperCase().padStart(2, "0");

const printable = (byte: number): string =>
  byte >= 0x20 && byte <= 0x7e ? String.fromCharCode(byte) : ".";

/** Parses a hex (0x…) or decimal address, rejecting anything else. */
function parseAddress(value: string): number | null {
  const text = value.trim();
  if (!text) return null;
  const parsed = /^0[xX][0-9a-fA-F]+$/.test(text)
    ? Number.parseInt(text.slice(2), 16)
    : /^\d+$/.test(text)
      ? Number.parseInt(text, 10)
      : NaN;
  return Number.isFinite(parsed) && parsed >= 0 && parsed <= 0xffffffff
    ? parsed
    : null;
}

/**
 * Hex view of target memory, with an optional comparison against the loaded
 * firmware so a mismatch is visible byte by byte rather than only as a failed
 * verify.
 */
export function MemoryViewer() {
  const { state } = useAppContext();
  const { readMemory, readFirmwareWindow, saveMemoryRegion } = useFlashProgrammer();

  const [addressInput, setAddressInput] = useState("");
  const [address, setAddress] = useState<number | null>(null);
  const [bytes, setBytes] = useState<number[] | null>(null);
  const [isReading, setIsReading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [compare, setCompare] = useState(false);
  const [firmwareBytes, setFirmwareBytes] = useState<(number | null)[] | null>(null);

  const isConnected = state.connectionStatus === "connected";

  // Default to the start of the target's flash once a target is known.
  useEffect(() => {
    if (address === null && state.targetInfo) {
      const base = state.targetInfo.flash_base;
      setAddress(base);
      setAddressInput(formatAddress(base));
    }
  }, [state.targetInfo, address]);

  const read = useCallback(
    async (at: number) => {
      setIsReading(true);
      setError(null);
      try {
        const result = await readMemory(at, PAGE_SIZE);
        if (result) {
          setBytes(result.bytes);
          setAddress(result.address);
          setAddressInput(formatAddress(result.address));
        } else {
          setBytes(null);
          setError("Read failed; see the console for details.");
        }
      } finally {
        setIsReading(false);
      }
    },
    [readMemory]
  );

  // The comparison asks the parser what the image places at these addresses, so
  // a gap in the image reads as "nothing expected" rather than as zeros.
  useEffect(() => {
    if (!compare || address === null || !state.firmwarePath) {
      setFirmwareBytes(null);
      return;
    }
    let cancelled = false;
    void (async () => {
      const window = await readFirmwareWindow(address, PAGE_SIZE);
      if (!cancelled) setFirmwareBytes(window);
    })();
    return () => {
      cancelled = true;
    };
  }, [compare, address, state.firmwarePath, readFirmwareWindow]);

  const handleSave = async () => {
    if (address === null || !bytes) return;
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({
        defaultPath: `memory_${formatAddress(address)}.bin`,
        filters: [{ name: "Raw binary", extensions: ["bin"] }],
      });
      if (path) {
        await saveMemoryRegion(path as string, address, PAGE_SIZE);
      }
    } catch {
      setError("Saving is only available in the desktop application.");
    }
  };

  const handleGo = () => {
    const parsed = parseAddress(addressInput);
    if (parsed === null) {
      setError("Enter an address such as 0x08000000.");
      return;
    }
    void read(parsed);
  };

  const step = (delta: number) => {
    if (address === null) return;
    const next = address + delta;
    if (next < 0 || next > 0xffffffff) return;
    void read(next);
  };

  const rowCount = bytes ? Math.ceil(bytes.length / BYTES_PER_ROW) : 0;

  return (
    <div className="flex flex-col h-full p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider">
          Memory
        </h2>
        {state.firmwarePath && (
          <label className="flex items-center gap-2 text-xs text-gray-400 cursor-pointer">
            <input
              type="checkbox"
              checked={compare}
              onChange={(e) => setCompare(e.target.checked)}
              className="accent-accent-red"
            />
            Compare with firmware
          </label>
        )}
      </div>

      <div className="flex gap-2">
        <input
          aria-label="Address"
          value={addressInput}
          onChange={(e) => setAddressInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleGo()}
          placeholder="0x08000000"
          className="flex-1 min-w-0 bg-bg-primary border border-gray-600 rounded px-2 py-1.5 text-sm font-mono text-gray-200 focus:border-accent-red focus:outline-none"
        />
        <button
          onClick={handleGo}
          disabled={!isConnected || isReading}
          className="shrink-0 px-3 py-1.5 bg-bg-tertiary border border-gray-600 rounded text-sm text-gray-200 hover:bg-gray-600 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {isReading ? "Reading..." : "Read"}
        </button>
        <button
          onClick={() => step(-PAGE_SIZE)}
          disabled={!isConnected || isReading || address === null}
          className="shrink-0 px-2.5 py-1.5 bg-bg-tertiary border border-gray-600 rounded text-sm text-gray-300 hover:bg-gray-600 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          title="Previous page"
        >
          ↑
        </button>
        <button
          onClick={handleSave}
          disabled={!bytes}
          className="shrink-0 px-2.5 py-1.5 bg-bg-tertiary border border-gray-600 rounded text-sm text-gray-300 hover:bg-gray-600 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          title="Save this page to a .bin file"
        >
          ⤓
        </button>
        <button
          onClick={() => step(PAGE_SIZE)}
          disabled={!isConnected || isReading || address === null}
          className="shrink-0 px-2.5 py-1.5 bg-bg-tertiary border border-gray-600 rounded text-sm text-gray-300 hover:bg-gray-600 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          title="Next page"
        >
          ↓
        </button>
      </div>

      {!isConnected && (
        <p className="text-xs text-gray-500">
          Connect to a target to read its memory.
        </p>
      )}
      {error && <p className="text-xs text-accent-red">{error}</p>}

      {bytes && address !== null && (
        <div className="flex-1 overflow-auto bg-bg-primary rounded border border-gray-700">
          <table className="text-xs font-mono">
            <tbody>
              {Array.from({ length: rowCount }, (_, row) => {
                const rowAddress = address + row * BYTES_PER_ROW;
                const slice = bytes.slice(
                  row * BYTES_PER_ROW,
                  row * BYTES_PER_ROW + BYTES_PER_ROW
                );
                return (
                  <tr key={rowAddress} className="border-b border-gray-800 last:border-0">
                    <td className="px-3 py-0.5 text-gray-500 whitespace-nowrap align-top">
                      {formatAddress(rowAddress)}
                    </td>
                    <td className="px-3 py-0.5 whitespace-nowrap">
                      {slice.map((byte, column) => {
                        const index = row * BYTES_PER_ROW + column;
                        const expected = firmwareBytes?.[index] ?? null;
                        const differs = expected !== null && expected !== byte;
                        return (
                          <span
                            key={column}
                            className={
                              differs
                                ? "text-accent-red font-bold mr-1"
                                : "text-gray-300 mr-1"
                            }
                            title={
                              differs
                                ? `Firmware has ${formatByte(expected as number)}`
                                : undefined
                            }
                          >
                            {formatByte(byte)}
                          </span>
                        );
                      })}
                    </td>
                    <td className="px-3 py-0.5 text-gray-500 whitespace-pre">
                      {slice.map(printable).join("")}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
