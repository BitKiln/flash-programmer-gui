import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { useEffect } from "react";
import { AppProvider, useAppContext } from "../state/AppContext";
import { MemoryViewer } from "../components/MemoryViewer";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

/// Puts the app in the state the viewer needs: connected, with firmware loaded.
function Connected() {
  const { dispatch } = useAppContext();
  useEffect(() => {
    dispatch({ type: "SET_CONNECTION_STATUS", status: "connected" });
    dispatch({
      type: "SET_TARGET_INFO",
      info: {
        name: "STM32U575ZITxQ",
        display_name: null,
        architecture: "Armv8-M",
        flash_base: 0x08000000,
        flash_size: 2 * 1024 * 1024,
        ram_base: 0x20000000,
        ram_size: 786432,
        page_size: 8192,
        sector_count: 256,
      },
    });
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  return <MemoryViewer />;
}

const page = (fill: number) => Array.from({ length: 256 }, () => fill);

describe("MemoryViewer", () => {
  beforeEach(() => invoke.mockReset());

  it("reads the requested address and renders hex and ASCII", async () => {
    invoke.mockResolvedValue({ address: 0x08000000, bytes: page(0x41) });

    render(
      <AppProvider>
        <Connected />
      </AppProvider>
    );

    fireEvent.click(screen.getByRole("button", { name: "Read" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("read_memory", {
        address: 0x08000000,
        length: 256,
      })
    );
    expect(await screen.findByText("0x08000000")).toBeTruthy();
    // 0x41 is "A", so the ASCII column shows a run of them.
    expect(screen.getAllByText(/AAAAAAAAAAAAAAAA/).length).toBeGreaterThan(0);
  });

  it("rejects an address that is not a number", async () => {
    render(
      <AppProvider>
        <Connected />
      </AppProvider>
    );

    fireEvent.change(screen.getByLabelText("Address"), {
      target: { value: "not-an-address" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Read" }));

    expect(await screen.findByText(/Enter an address such as/)).toBeTruthy();
    expect(invoke).not.toHaveBeenCalledWith("read_memory", expect.anything());
  });

  it("pages forward by one screen", async () => {
    invoke.mockResolvedValue({ address: 0x08000000, bytes: page(0) });

    render(
      <AppProvider>
        <Connected />
      </AppProvider>
    );
    await waitFor(() => expect(screen.getByTitle("Next page")).toBeTruthy());

    fireEvent.click(screen.getByTitle("Next page"));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("read_memory", {
        address: 0x08000100,
        length: 256,
      })
    );
  });

  it("highlights bytes that differ from the loaded firmware", async () => {
    const onDevice = page(0xff);
    onDevice[0] = 0x00;
    const expected: (number | null)[] = page(0xff);
    expected[0] = 0x12; // the image disagrees about the first byte

    invoke.mockImplementation((command: string) => {
      if (command === "read_memory") {
        return Promise.resolve({ address: 0x08000000, bytes: onDevice });
      }
      if (command === "read_firmware_window") {
        return Promise.resolve(expected);
      }
      if (command === "load_firmware") {
        return Promise.resolve({
          format: "Raw Binary",
          file_path: "/build/app.bin",
          file_size_bytes: 256,
          total_firmware_bytes: 256,
          base_address: 0x08000000,
          highest_address: 0x08000100,
          segment_count: 1,
          entry_point: null,
          entry_point_source: "not declared",
          crc32: 0,
          segments: [],
          gaps: [],
        });
      }
      return Promise.resolve(null);
    });

    function WithFirmware() {
      const { dispatch } = useAppContext();
      useEffect(() => {
        dispatch({ type: "SET_FIRMWARE", firmware: null, path: "/build/app.bin" });
      }, []); // eslint-disable-line react-hooks/exhaustive-deps
      return <Connected />;
    }

    render(
      <AppProvider>
        <WithFirmware />
      </AppProvider>
    );

    fireEvent.click(screen.getByRole("button", { name: "Read" }));
    await screen.findByText("0x08000000");

    fireEvent.click(screen.getByLabelText(/Compare with firmware/i));

    const mismatch = await screen.findByTitle("Firmware has 12");
    expect(mismatch.textContent).toBe("00");
  });
});
