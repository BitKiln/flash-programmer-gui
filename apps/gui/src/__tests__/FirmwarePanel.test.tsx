import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { useEffect } from "react";
import { AppProvider, useAppContext } from "../state/AppContext";
import { FirmwarePanel } from "../components/FirmwarePanel";
import type { FirmwareInfo } from "../types";

const invoke = vi.fn();
const onDragDropEvent = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (handler: unknown) => onDragDropEvent(handler),
  }),
}));

const firmware: FirmwareInfo = {
  format: "Intel HEX",
  file_path: "/build/app.hex",
  file_size_bytes: 4096,
  total_firmware_bytes: 3072,
  base_address: 0x08000000,
  highest_address: 0x08002000,
  segment_count: 2,
  entry_point: 0x08000101,
  entry_point_source: "Cortex-M vector table",
  crc32: 0xdeadbeef,
  segments: [
    {
      index: 0,
      start_address: 0x08000000,
      end_address: 0x08000400,
      size_bytes: 1024,
      crc32: "0x0A5B1F0D",
    },
    {
      index: 1,
      start_address: 0x08001000,
      end_address: 0x08001800,
      size_bytes: 2048,
      crc32: "0x11223344",
    },
  ],
  gaps: [
    { start_address: 0x08000400, end_address: 0x08001000, size: 3072 },
  ],
};

function WithFirmware() {
  const { dispatch } = useAppContext();
  useEffect(() => {
    dispatch({ type: "SET_FIRMWARE", firmware, path: "/build/app.hex" });
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  return <FirmwarePanel />;
}

describe("FirmwarePanel", () => {
  beforeEach(() => {
    invoke.mockReset();
    onDragDropEvent.mockReset();
    onDragDropEvent.mockResolvedValue(() => {});
  });

  it("subscribes to Tauri drag-drop rather than relying on File.path", async () => {
    render(
      <AppProvider>
        <FirmwarePanel />
      </AppProvider>
    );

    // A webview File carries no filesystem path, so the panel must take paths
    // from Tauri's own drag-drop event.
    await waitFor(() => expect(onDragDropEvent).toHaveBeenCalled());
  });

  it("loads the dropped file by its real path", async () => {
    let handler: ((event: { payload: { type: string; paths?: string[] } }) => void) | undefined;
    onDragDropEvent.mockImplementation((h: typeof handler) => {
      handler = h;
      return Promise.resolve(() => {});
    });
    invoke.mockResolvedValue(firmware);

    render(
      <AppProvider>
        <FirmwarePanel />
      </AppProvider>
    );
    await waitFor(() => expect(handler).toBeDefined());

    handler!({ payload: { type: "drop", paths: ["/build/dropped.elf"] } });

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("load_firmware", {
        path: "/build/dropped.elf",
        baseAddress: null,
      })
    );
  });

  it("renders one row per segment with addresses and checksums", async () => {
    render(
      <AppProvider>
        <WithFirmware />
      </AppProvider>
    );

    await screen.findByText("0x08001000");
    expect(screen.getByText("0x0A5B1F0D")).toBeTruthy();
    expect(screen.getByText("0x11223344")).toBeTruthy();
    expect(screen.getByText("0x08000400")).toBeTruthy();
  });

  it("reports the unwritten space between segments", async () => {
    render(
      <AppProvider>
        <WithFirmware />
      </AppProvider>
    );

    expect(await screen.findByText(/1 gap\(s\)/)).toBeTruthy();
    expect(screen.getByText(/unwritten between segments/)).toBeTruthy();
  });

  it("attributes the entry point to its source", async () => {
    render(
      <AppProvider>
        <WithFirmware />
      </AppProvider>
    );

    const entry = await screen.findByTitle("Source: Cortex-M vector table");
    expect(entry.textContent).toContain("0x08000101");
  });
});

const rawBinary: FirmwareInfo = {
  ...firmware,
  format: "Raw Binary",
  file_path: "/build/mp_esp32.bin",
  base_address: 0,
  highest_address: 0x1b5250,
  segment_count: 1,
  entry_point: null,
  entry_point_source: "not declared",
  segments: [
    {
      index: 0,
      start_address: 0,
      end_address: 0x1b5250,
      size_bytes: 0x1b5250,
      crc32: "0xDEFB266B",
    },
  ],
  gaps: [],
};

function WithRawBinary() {
  const { dispatch } = useAppContext();
  useEffect(() => {
    dispatch({
      type: "SET_FIRMWARE",
      firmware: rawBinary,
      path: "/build/mp_esp32.bin",
    });
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  return <FirmwarePanel />;
}

describe("FirmwarePanel base address", () => {
  beforeEach(() => {
    invoke.mockReset();
    onDragDropEvent.mockReset();
    onDragDropEvent.mockResolvedValue(() => {});
  });

  it("offers a base address for a raw binary, which carries none of its own", async () => {
    render(
      <AppProvider>
        <WithRawBinary />
      </AppProvider>
    );

    await waitFor(() => expect(screen.getByLabelText("Base address")).toBeTruthy());
  });

  it("does not offer one for a format that already has addresses", async () => {
    render(
      <AppProvider>
        <WithFirmware />
      </AppProvider>
    );

    await waitFor(() => expect(screen.getByText("Intel HEX")).toBeTruthy());
    expect(screen.queryByLabelText("Base address")).toBeNull();
  });

  it("reloads at the address typed, so an image can go somewhere other than the default", async () => {
    invoke.mockResolvedValue(rawBinary);

    render(
      <AppProvider>
        <WithRawBinary />
      </AppProvider>
    );
    const field = await screen.findByLabelText("Base address");

    fireEvent.change(field, { target: { value: "0x1000" } });
    fireEvent.click(screen.getByText("Apply"));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("load_firmware", {
        path: "/build/mp_esp32.bin",
        baseAddress: 0x1000,
      })
    );
  });

  it("rejects something that is not an address instead of loading at NaN", async () => {
    render(
      <AppProvider>
        <WithRawBinary />
      </AppProvider>
    );
    const field = await screen.findByLabelText("Base address");

    fireEvent.change(field, { target: { value: "the start" } });
    fireEvent.click(screen.getByText("Apply"));

    expect(await screen.findByText(/is not an address/)).toBeTruthy();
    expect(invoke).not.toHaveBeenCalledWith(
      "load_firmware",
      expect.objectContaining({ baseAddress: NaN })
    );
  });
});
