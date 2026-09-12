import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { AppProvider, useAppContext } from "../state/AppContext";
import { useFlashProgrammer } from "../hooks/useFlashProgrammer";
import type { FlashEventDto } from "../types";

const invoke = vi.fn();
const listeners = new Map<string, (event: { payload: FlashEventDto }) => void>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (channel: string, handler: (event: { payload: FlashEventDto }) => void) => {
    listeners.set(channel, handler);
    return Promise.resolve(() => listeners.delete(channel));
  },
}));

/// Renders the hook and exposes the pieces of state the assertions need.
function Harness({ onReady }: { onReady?: (api: ReturnType<typeof useFlashProgrammer>) => void }) {
  const { state } = useAppContext();
  const api = useFlashProgrammer();

  useEffect(() => {
    onReady?.(api);
  }, [api, onReady]);

  return (
    <div>
      <span data-testid="status">{state.flashStatus}</span>
      <span data-testid="percentage">{state.progress.percentage}</span>
      <span data-testid="logs">{state.logs.map((l) => l.message).join("|")}</span>
    </div>
  );
}

function renderHook(onReady?: (api: ReturnType<typeof useFlashProgrammer>) => void) {
  return render(
    <AppProvider>
      <Harness onReady={onReady} />
    </AppProvider>
  );
}

describe("useFlashProgrammer", () => {
  beforeEach(() => {
    invoke.mockReset();
    listeners.clear();
  });

  it("subscribes to the three telemetry channels instead of polling", async () => {
    renderHook();
    await waitFor(() => expect(listeners.size).toBe(3));
    expect([...listeners.keys()].sort()).toEqual([
      "flash:log",
      "flash:progress",
      "flash:status",
    ]);
  });

  it("applies pushed progress events to state", async () => {
    renderHook();
    await waitFor(() => expect(listeners.has("flash:progress")).toBe(true));

    listeners.get("flash:progress")!({
      payload: {
        type: "Progress",
        stage: "programming",
        bytes_transferred: 512,
        total_bytes: 1024,
        percentage: 50,
        speed_bps: 4096,
        elapsed_ms: 120,
        current_address: 0x08000200,
        message: "Programming...",
      },
    });

    await waitFor(() =>
      expect(screen.getByTestId("percentage").textContent).toBe("50")
    );
  });

  it("logs pushed warnings", async () => {
    renderHook();
    await waitFor(() => expect(listeners.has("flash:log")).toBe(true));

    listeners.get("flash:log")!({
      payload: { type: "Warning", message: "Sector already erased" },
    });

    await waitFor(() =>
      expect(screen.getByTestId("logs").textContent).toContain(
        "Sector already erased"
      )
    );
  });

  it("requests cancellation and marks the operation as cancelling", async () => {
    let api: ReturnType<typeof useFlashProgrammer> | undefined;
    renderHook((ready) => {
      api = ready;
    });
    await waitFor(() => expect(api).toBeDefined());

    invoke.mockResolvedValue("Cancellation requested");
    await api!.cancelOperation();

    expect(invoke).toHaveBeenCalledWith("cancel_operation");
    await waitFor(() =>
      expect(screen.getByTestId("status").textContent).toBe("cancelling")
    );
  });

  it("reports a cancelled flash as cancelled, not as an error", async () => {
    let api: ReturnType<typeof useFlashProgrammer> | undefined;
    renderHook((ready) => {
      api = ready;
    });
    await waitFor(() => expect(api).toBeDefined());

    // Load firmware so flashFirmware gets past its guard.
    invoke.mockResolvedValueOnce({
      format: "Intel HEX",
      file_path: "/build/app.hex",
      file_size_bytes: 16,
      total_firmware_bytes: 16,
      base_address: 0x08000000,
      highest_address: 0x08000010,
      segment_count: 1,
      entry_point: null,
      entry_point_source: "not declared",
      crc32: 0,
      segments: [],
      gaps: [],
    });
    await api!.loadFirmware("/build/app.hex");
    // Wait for the re-render so `api` closes over the loaded firmware path.
    await waitFor(() =>
      expect(screen.getByTestId("logs").textContent).toContain("Loaded Intel HEX")
    );

    // The backend surfaces a cancellation through the error channel.
    invoke.mockRejectedValueOnce("Operation cancelled");
    invoke.mockResolvedValue([]);
    await api!.flashFirmware();

    await waitFor(() =>
      expect(screen.getByTestId("status").textContent).toBe("cancelled")
    );
    expect(screen.getByTestId("logs").textContent).toContain(
      "Operation cancelled"
    );
  });
});
