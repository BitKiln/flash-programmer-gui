import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent, act } from "@testing-library/react";
import { AppProvider } from "../state/AppContext";
import { BatchPanel } from "../components/BatchPanel";
import type { BatchEventDto } from "../types";

const invoke = vi.fn();
/// Handlers registered by the panel for the `batch:event` channel, so a test
/// can push events the way the Rust side does.
const batchHandlers: ((event: { payload: BatchEventDto }) => void)[] = [];

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((channel: string, handler: (event: { payload: unknown }) => void) => {
    if (channel === "batch:event") {
      batchHandlers.push(handler as (event: { payload: BatchEventDto }) => void);
    }
    return Promise.resolve(() => {});
  }),
}));

function emit(payload: BatchEventDto) {
  act(() => {
    batchHandlers.forEach((handler) => handler({ payload }));
  });
}

const report = {
  units: [],
  passed: 2,
  failed: 0,
  duration_ms: 1234,
  stop_reason: "count_reached",
  log_path: "run.csv",
};

/// Loads a firmware file into the shared state; the panel refuses to start
/// without one.
async function renderWithFirmware() {
  invoke.mockImplementation((command: string) => {
    if (command === "load_firmware") {
      return Promise.resolve({
        format: "ELF",
        file_path: "app.elf",
        file_size_bytes: 1024,
        total_firmware_bytes: 512,
        base_address: 0x08000000,
        highest_address: 0x08000200,
        segment_count: 1,
        entry_point: null,
        entry_point_source: "not declared",
        crc32: 0,
        segments: [],
        gaps: [],
      });
    }
    if (command === "start_batch") {
      return Promise.resolve(report);
    }
    return Promise.resolve([]);
  });

  const view = render(
    <AppProvider>
      <BatchPanel />
    </AppProvider>
  );
  return view;
}

describe("BatchPanel", () => {
  beforeEach(() => {
    invoke.mockReset();
    batchHandlers.length = 0;
  });

  it("blocks the run until firmware is loaded", async () => {
    await renderWithFirmware();
    expect(
      screen.getByText("Load a firmware file before starting a run.")
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Start run" }).hasAttribute("disabled")).toBe(
      true
    );
  });

  it("rejects a serial address that is not hex", async () => {
    await renderWithFirmware();
    fireEvent.change(screen.getByLabelText("Serial address"), {
      target: { value: "not-hex" },
    });
    expect(screen.getByText("Enter a hex address, e.g. 0801F800.")).toBeTruthy();
  });

  it("rejects a board count below one", async () => {
    await renderWithFirmware();
    fireEvent.change(screen.getByLabelText("Board count"), {
      target: { value: "0" },
    });
    expect(screen.getByText("Enter a count of 1 or more.")).toBeTruthy();
  });

  it("appends a row per board and shows the wait prompts", async () => {
    await renderWithFirmware();

    emit({ type: "WaitingForAttach", index: 2 });
    await waitFor(() => expect(screen.getByText("Connect board 2...")).toBeTruthy());

    emit({
      type: "UnitFinished",
      unit: {
        index: 1,
        status: "passed",
        serial: "SN-000001",
        target: "STM32U575ZITxQ",
        bytes_flashed: 512,
        verified: true,
        duration_ms: 800,
        started_unix_ms: 0,
        message: "Successfully flashed 512 bytes",
      },
    });
    emit({
      type: "UnitFinished",
      unit: {
        index: 2,
        status: "failed",
        serial: null,
        target: "STM32U575ZITxQ",
        bytes_flashed: 0,
        verified: false,
        duration_ms: 120,
        started_unix_ms: 0,
        message: "Failed to connect to target",
      },
    });

    await waitFor(() => {
      expect(screen.getByText("PASS")).toBeTruthy();
      expect(screen.getByText("FAIL")).toBeTruthy();
    });
    expect(screen.getByText("SN-000001")).toBeTruthy();
    expect(screen.getByText("1 pass")).toBeTruthy();
    expect(screen.getByText("1 fail")).toBeTruthy();

    emit({ type: "Finished", passed: 1, failed: 1, stop_reason: "count_reached" });
    await waitFor(() =>
      expect(screen.getByText("1 passed, 1 failed (count_reached)")).toBeTruthy()
    );
  });
});
