import { useCallback, useEffect, useMemo, useState } from "react";
import { useAppContext } from "../state/AppContext";
import { useFlashProgrammer } from "../hooks/useFlashProgrammer";
import type { FlashMapInfo, SectorInfo } from "../types";

const formatAddress = (address: number): string =>
  "0x" + (address >>> 0).toString(16).toUpperCase().padStart(8, "0");

/** Bytes as KB or MB, since a sector table is unreadable in raw bytes. */
export function formatSize(bytes: number): string {
  if (bytes >= 1024 * 1024 && bytes % (1024 * 1024) === 0) {
    return `${bytes / (1024 * 1024)} MB`;
  }
  if (bytes >= 1024 && bytes % 1024 === 0) return `${bytes / 1024} KB`;
  return `${bytes} B`;
}

/** How much of `sector` the loaded image writes, as a fraction of its size. */
export function coverageOf(
  sector: SectorInfo,
  segments: { start_address: number; end_address: number }[]
): number {
  const sectorEnd = sector.address + sector.size;
  let covered = 0;
  for (const segment of segments) {
    const start = Math.max(sector.address, segment.start_address);
    const end = Math.min(sectorEnd, segment.end_address);
    if (end > start) covered += end - start;
  }
  return sector.size === 0 ? 0 : Math.min(1, covered / sector.size);
}

/**
 * Runs of adjacent sectors that share a size, so a 256-sector part reads as a
 * handful of regions rather than as 256 rows that say the same thing.
 */
export function groupBySize(sectors: SectorInfo[]): {
  size: number;
  count: number;
  address: number;
  end: number;
  firstIndex: number;
}[] {
  const groups: {
    size: number;
    count: number;
    address: number;
    end: number;
    firstIndex: number;
  }[] = [];
  for (const sector of sectors) {
    const last = groups[groups.length - 1];
    if (last && last.size === sector.size && last.end === sector.address) {
      last.count += 1;
      last.end = sector.address + sector.size;
    } else {
      groups.push({
        size: sector.size,
        count: 1,
        address: sector.address,
        end: sector.address + sector.size,
        firstIndex: sector.index,
      });
    }
  }
  return groups;
}

/**
 * The target's flash as erase units, with the loaded image laid over it.
 *
 * The point is the erase granularity: a sector is the smallest thing that can
 * be erased, so a one-byte change in a 128 KB sector rewrites all 128 KB. That
 * is invisible in a byte-level view and is what this tab exists to show.
 */
export function FlashMap() {
  const { state } = useAppContext();
  const { readFlashMap } = useFlashProgrammer();

  const [map, setMap] = useState<FlashMapInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isConnected = state.connectionStatus === "connected";

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await readFlashMap();
      setMap(result);
      if (!result) setError("Could not read the flash map; see the console.");
    } finally {
      setLoading(false);
    }
  }, [readFlashMap]);

  useEffect(() => {
    if (!isConnected) {
      setMap(null);
      return;
    }
    void load();
  }, [isConnected, state.targetInfo?.name, load]);

  const segments = useMemo(
    () => state.firmware?.segments ?? [],
    [state.firmware]
  );

  const groups = useMemo(() => (map ? groupBySize(map.sectors) : []), [map]);

  // Sectors the image touches at all: the count an erase would actually clear.
  const touched = useMemo(
    () => (map ? map.sectors.filter((s) => coverageOf(s, segments) > 0) : []),
    [map, segments]
  );

  const touchedBytes = touched.reduce((sum, s) => sum + s.size, 0);
  const imageBytes = segments.reduce(
    (sum, s) => sum + (s.end_address - s.start_address),
    0
  );

  if (!isConnected) {
    return (
      <div className="p-4">
        <p className="text-xs text-gray-500">
          Connect to a target to see its flash map.
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider">
          Flash Map
        </h2>
        <button
          onClick={() => void load()}
          disabled={loading}
          className="px-2.5 py-1 bg-bg-tertiary border border-gray-600 rounded text-xs text-gray-300 hover:bg-gray-600 transition-colors disabled:opacity-40"
        >
          {loading ? "Reading..." : "Refresh"}
        </button>
      </div>

      {error && <p className="text-xs text-accent-red">{error}</p>}

      {map && (
        <>
          <p className="text-xs text-gray-500">
            {formatSize(map.flash_size)} from {formatAddress(map.flash_base)},{" "}
            {map.sectors.length} erase {map.sectors.length === 1 ? "unit" : "units"}
            {map.geometry_estimated && (
              <span className="text-yellow-400">
                {" "}
                — the backend reports no sector list, so this assumes a uniform
                page-sized geometry
              </span>
            )}
          </p>

          {/* One strip per sector, so an image's footprint is visible against
              the whole part rather than only as a byte count. */}
          <div className="flex h-8 w-full overflow-hidden rounded border border-gray-700 bg-bg-primary">
            {map.sectors.map((sector) => {
              const coverage = coverageOf(sector, segments);
              return (
                <div
                  key={sector.address}
                  style={{ flexGrow: sector.size, flexBasis: 0 }}
                  className={
                    coverage === 0
                      ? "border-r border-gray-800 last:border-0"
                      : coverage === 1
                        ? "bg-accent-red border-r border-gray-800 last:border-0"
                        : "bg-accent-red/50 border-r border-gray-800 last:border-0"
                  }
                  title={`${formatAddress(sector.address)} — ${formatSize(sector.size)}${
                    coverage > 0
                      ? `, ${Math.round(coverage * 100)}% written by the image`
                      : ""
                  }`}
                />
              );
            })}
          </div>

          {state.firmware && (
            <p className="text-xs text-gray-400">
              The loaded image writes {formatSize(imageBytes)} and so erases{" "}
              <span className="text-gray-100">
                {touched.length} of {map.sectors.length}
              </span>{" "}
              sectors, {formatSize(touchedBytes)} in all. A sector is the
              smallest erasable unit, so every byte in a touched sector is
              cleared even where the image covers none of it.
            </p>
          )}

          <div className="flex-1 overflow-auto rounded border border-gray-700">
            <table className="w-full text-xs">
              <thead className="bg-bg-tertiary text-gray-400 sticky top-0">
                <tr>
                  <th className="px-3 py-1.5 text-left font-medium">Sectors</th>
                  <th className="px-3 py-1.5 text-left font-medium">Range</th>
                  <th className="px-3 py-1.5 text-left font-medium">Each</th>
                  <th className="px-3 py-1.5 text-left font-medium">Written</th>
                </tr>
              </thead>
              <tbody className="font-mono text-gray-300">
                {groups.map((group) => {
                  const inGroup = map.sectors.filter(
                    (s) => s.address >= group.address && s.address < group.end
                  );
                  const writtenCount = inGroup.filter(
                    (s) => coverageOf(s, segments) > 0
                  ).length;
                  return (
                    <tr
                      key={group.address}
                      className="border-b border-gray-800 last:border-0"
                    >
                      <td className="px-3 py-1 whitespace-nowrap">
                        {group.count === 1
                          ? `#${group.firstIndex}`
                          : `#${group.firstIndex}–${group.firstIndex + group.count - 1}`}
                      </td>
                      <td className="px-3 py-1 whitespace-nowrap">
                        {formatAddress(group.address)} – {formatAddress(group.end)}
                      </td>
                      <td className="px-3 py-1 whitespace-nowrap">
                        {formatSize(group.size)}
                      </td>
                      <td className="px-3 py-1 whitespace-nowrap">
                        {writtenCount === 0 ? (
                          <span className="text-gray-600">—</span>
                        ) : (
                          <span className="text-accent-red">
                            {writtenCount} of {group.count}
                          </span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  );
}
