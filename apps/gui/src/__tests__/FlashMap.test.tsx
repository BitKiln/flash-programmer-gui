import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { AppProvider, useAppContext } from "../state/AppContext";
import {
  FlashMap,
  coverageOf,
  groupBySize,
  formatSize,
} from "../components/FlashMap";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

/// An STM32F4-like part: four small sectors, then a large one.
const sectors = [
  { index: 0, address: 0x08000000, size: 16 * 1024 },
  { index: 1, address: 0x08004000, size: 16 * 1024 },
  { index: 2, address: 0x08008000, size: 16 * 1024 },
  { index: 3, address: 0x0800c000, size: 16 * 1024 },
  { index: 4, address: 0x08010000, size: 64 * 1024 },
];

const flashMap = {
  flash_base: 0x08000000,
  flash_size: 128 * 1024,
  sectors,
  geometry_estimated: false,
};

function Connected({ withFirmware = false }: { withFirmware?: boolean }) {
  const { dispatch } = useAppContext();
  useEffect(() => {
    dispatch({ type: "SET_CONNECTION_STATUS", status: "connected" });
    dispatch({
      type: "SET_TARGET_INFO",
      info: {
        name: "STM32F401RE",
        display_name: null,
        architecture: "Armv7-M",
        flash_base: 0x08000000,
        flash_size: 128 * 1024,
        ram_base: 0x20000000,
        ram_size: 96 * 1024,
        page_size: 16 * 1024,
        sector_count: sectors.length,
        cancellable_stages: ["erasing", "programming", "verifying"],
      },
    });
    if (withFirmware) {
      dispatch({
        type: "SET_FIRMWARE",
        path: "/build/app.bin",
        firmware: {
          format: "Raw Binary",
          file_path: "/build/app.bin",
          file_size_bytes: 1024,
          total_firmware_bytes: 1024,
          base_address: 0x08000000,
          highest_address: 0x08000400,
          segment_count: 1,
          entry_point: null,
          entry_point_source: "not declared",
          crc32: 0,
          // 1 KB at the very start: one byte over a sector boundary would be
          // enough to erase the whole 16 KB sector.
          segments: [
            {
              index: 0,
              start_address: 0x08000000,
              end_address: 0x08000400,
              size_bytes: 1024,
              crc32: "0",
            },
          ],
          gaps: [],
        },
      });
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  return <FlashMap />;
}

describe("flash map arithmetic", () => {
  it("counts only the part of a segment that falls inside the sector", () => {
    // A segment straddling the boundary covers half of sector 0.
    const covered = coverageOf(sectors[0], [
      { start_address: 0x08002000, end_address: 0x08006000 },
    ]);
    expect(covered).toBeCloseTo(0.5);
  });

  it("reports no coverage for a segment elsewhere in the part", () => {
    expect(
      coverageOf(sectors[0], [
        { start_address: 0x08010000, end_address: 0x08010100 },
      ])
    ).toBe(0);
  });

  it("never reports more than full coverage when segments overlap", () => {
    expect(
      coverageOf(sectors[0], [
        { start_address: 0x08000000, end_address: 0x08004000 },
        { start_address: 0x08000000, end_address: 0x08004000 },
      ])
    ).toBe(1);
  });

  it("groups adjacent sectors of the same size into one region", () => {
    const groups = groupBySize(sectors);
    expect(groups).toHaveLength(2);
    expect(groups[0]).toMatchObject({ count: 4, size: 16 * 1024, firstIndex: 0 });
    expect(groups[1]).toMatchObject({ count: 1, size: 64 * 1024, firstIndex: 4 });
  });

  it("does not group across a gap in addresses", () => {
    const groups = groupBySize([
      { index: 0, address: 0x0, size: 4096 },
      { index: 1, address: 0x2000, size: 4096 }, // not adjacent
    ]);
    expect(groups).toHaveLength(2);
  });

  it("shows sizes in the units a datasheet uses", () => {
    expect(formatSize(4096)).toBe("4 KB");
    expect(formatSize(2 * 1024 * 1024)).toBe("2 MB");
    expect(formatSize(100)).toBe("100 B");
  });
});

describe("FlashMap", () => {
  beforeEach(() => invoke.mockReset());

  it("lists the sector regions the backend reports", async () => {
    invoke.mockImplementation((command: string) =>
      command === "flash_map" ? Promise.resolve(flashMap) : Promise.resolve(null)
    );

    render(
      <AppProvider>
        <Connected />
      </AppProvider>
    );

    expect(await screen.findByText("#0–3")).toBeTruthy();
    expect(screen.getByText("#4")).toBeTruthy();
    expect(screen.getByText("64 KB")).toBeTruthy();
  });

  it("says how much a small image actually erases", async () => {
    invoke.mockImplementation((command: string) =>
      command === "flash_map" ? Promise.resolve(flashMap) : Promise.resolve(null)
    );

    render(
      <AppProvider>
        <Connected withFirmware />
      </AppProvider>
    );

    // 1 KB of image, but a 16 KB sector is the smallest thing that can be
    // erased -- which is the whole point of the tab.
    expect(await screen.findByText("1 of 5")).toBeTruthy();
    expect(screen.getByText(/16 KB in all/)).toBeTruthy();
  });

  it("says so when the geometry is assumed rather than reported", async () => {
    invoke.mockImplementation((command: string) =>
      command === "flash_map"
        ? Promise.resolve({ ...flashMap, geometry_estimated: true })
        : Promise.resolve(null)
    );

    render(
      <AppProvider>
        <Connected />
      </AppProvider>
    );

    expect(
      await screen.findByText(/assumes a uniform\s+page-sized geometry/)
    ).toBeTruthy();
  });

  it("asks for nothing until a target is connected", async () => {
    render(
      <AppProvider>
        <FlashMap />
      </AppProvider>
    );

    expect(
      await screen.findByText(/Connect to a target to see its flash map/)
    ).toBeTruthy();
    await waitFor(() =>
      expect(invoke).not.toHaveBeenCalledWith("flash_map", expect.anything())
    );
  });
});
