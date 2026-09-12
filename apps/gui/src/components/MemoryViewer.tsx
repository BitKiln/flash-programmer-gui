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
 * Parses a hex byte string such as `DEADBEEF` or `de ad be ef`.
 *
 * An odd number of digits is refused rather than padded: a missing nibble
 * means the intended value is unknown, and guessing which end it belongs on
 * would write the wrong byte.
 */
export function parseHexBytes(value: string): number[] | string {
  const cleaned = value
    .trim()
    .replace(/^0[xX]/, "")
    .replace(/[\s_,]/g, "");
  if (!cleaned) return "Enter bytes as hex digits, such as DEADBEEF.";
  if (!/^[0-9a-fA-F]+$/.test(cleaned)) {
    return `"${value.trim()}" is not hex. Use digits 0-9 and A-F.`;
  }
  if (cleaned.length % 2 !== 0) {
    return `"${value.trim()}" has an odd number of digits, so one byte is incomplete.`;
  }
  const bytes: number[] = [];
  for (let i = 0; i < cleaned.length; i += 2) {
    bytes.push(Number.parseInt(cleaned.slice(i, i + 2), 16));
  }
  return bytes;
}

/**
 * Hex view of target memory, with an optional comparison against the loaded
 * firmware so a mismatch is visible byte by byte rather than only as a failed
 * verify.
 */
export function MemoryViewer() {
  const { state } = useAppContext();
  const {
    readMemory,
    readFirmwareWindow,
    saveMemoryRegion,
    writeMemory,
    canWriteMemory,
  } = useFlashProgrammer();

  const [addressInput, setAddressInput] = useState("");
  const [address, setAddress] = useState<number | null>(null);
  const [bytes, setBytes] = useState<number[] | null>(null);
  const [isReading, setIsReading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [compare, setCompare] = useState(false);
  const [firmwareBytes, setFirmwareBytes] = useState<(number | null)[] | null>(null);
  const [canWrite, setCanWrite] = useState(false);
  const [editing, setEditing] = useState(false);
  const [editAddress, setEditAddress] = useState("");
  const [editData, setEditData] = useState("");
  const [editError, setEditError] = useState<string | null>(null);
  const [isWriting, setIsWriting] = useState(false);

  const isConnected = state.connectionStatus === "connected";

  // Whether the editor is offered at all is the backend's answer, not a guess
  // from the transport: an ESP bootloader reaches flash and nothing else, and
  // an editor that cannot write is worse than no editor.
  useEffect(() => {
    if (!isConnected) {
      setCanWrite(false);
      setEditing(false);
      return;
    }
    let cancelled = false;
    void (async () => {
      const allowed = await canWriteMemory();
      if (!cancelled) setCanWrite(allowed);
    })();
    return () => {
      cancelled = true;
    };
  }, [isConnected, canWriteMemory]);

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

  const openEditor = (at: number) => {
    setEditAddress(formatAddress(at));
    setEditData("");
    setEditError(null);
    setEditing(true);
  };

  const handleWrite = async () => {
    const at = parseAddress(editAddress);
    if (at === null) {
      setEditError("Enter an address such as 0x20000000.");
      return;
    }
    const parsed = parseHexBytes(editData);
    if (typeof parsed === "string") {
      setEditError(parsed);
      return;
    }
    // The backend refuses this too, but saying it here costs no round trip and
    // names the path that does erase.
    const target = state.targetInfo;
    if (
      target &&
      at < target.flash_base + target.flash_size &&
      at + parsed.length > target.flash_base
    ) {
      setEditError(
        "That address is in flash. A memory write does not erase, so it cannot " +
          "write flash - program the image instead."
      );
      return;
    }

    setIsWriting(true);
    setEditError(null);
    try {
      const ok = await writeMemory(at, parsed);
      if (ok) {
        setEditing(false);
        setEditData("");
        // Read back what is actually there rather than showing what was asked
        // for: a register can ignore a write, or answer with something else.
        if (address !== null) await read(address);
      } else {
        setEditError("The write was refused; see the console for details.");
      }
    } finally {
      setIsWriting(false);
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
        {canWrite && (
          <button
            onClick={() => (editing ? setEditing(false) : openEditor(address ?? 0))}
            className="shrink-0 px-2.5 py-1.5 bg-bg-tertiary border border-gray-600 rounded text-sm text-gray-300 hover:bg-gray-600 transition-colors"
            title="Write bytes to an address"
          >
            {editing ? "Cancel" : "Edit"}
          </button>
        )}
        <button
          onClick={() => step(PAGE_SIZE)}
          disabled={!isConnected || isReading || address === null}
          className="shrink-0 px-2.5 py-1.5 bg-bg-tertiary border border-gray-600 rounded text-sm text-gray-300 hover:bg-gray-600 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          title="Next page"
        >
          ↓
        </button>
      </div>

      {editing && (
        <div className="space-y-2 rounded border border-gray-700 bg-bg-primary p-2">
          <div className="flex gap-2">
            <input
              aria-label="Write address"
              value={editAddress}
              onChange={(e) => setEditAddress(e.target.value)}
              placeholder="0x20000000"
              className="w-40 bg-bg-secondary border border-gray-600 rounded px-2 py-1.5 text-sm font-mono text-gray-200 focus:border-accent-red focus:outline-none"
            />
            <input
              aria-label="Bytes to write"
              value={editData}
              onChange={(e) => setEditData(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void handleWrite()}
              placeholder="DEADBEEF"
              className="flex-1 min-w-0 bg-bg-secondary border border-gray-600 rounded px-2 py-1.5 text-sm font-mono text-gray-200 focus:border-accent-red focus:outline-none"
            />
            <button
              onClick={() => void handleWrite()}
              disabled={isWriting}
              className="shrink-0 px-3 py-1.5 bg-bg-tertiary border border-gray-600 rounded text-sm text-gray-200 hover:bg-gray-600 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              {isWriting ? "Writing..." : "Write"}
            </button>
          </div>
          <p className="text-xs text-gray-500">
            Writes straight onto the bus, with no erase and no verify: RAM,
            peripheral registers, and memory-mapped configuration such as option
            bytes. Flash goes through Flash, which erases first.
          </p>
          {editError && <p className="text-xs text-accent-red">{editError}</p>}
        </div>
      )}

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
                        const byteAddress = rowAddress + column;
                        return (
                          <span
                            key={column}
                            onClick={canWrite ? () => openEditor(byteAddress) : undefined}
                            className={[
                              differs
                                ? "text-accent-red font-bold mr-1"
                                : "text-gray-300 mr-1",
                              canWrite ? "cursor-pointer hover:bg-gray-700" : "",
                            ].join(" ")}
                            title={
                              differs
                                ? `Firmware has ${formatByte(expected as number)}`
                                : canWrite
                                  ? `Write at ${formatAddress(byteAddress)}`
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
