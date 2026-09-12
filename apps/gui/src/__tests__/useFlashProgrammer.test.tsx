import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { AppProvider, useAppContext } from "../state/AppContext";
import {
  useFlashProgrammer,
  useFlashTelemetry,
} from "../hooks/useFlashProgrammer";
import type { FlashEventDto } from "../types";

const invoke = vi.fn();
type Handler = (event: { payload: FlashEventDto }) => void;
const subscriptions: Array<{ channel: string; handler: Handler }> = [];

/** Channels with at least one subscriber. */
const channels = () => new Set(subscriptions.map((s) => s.channel));

/** Delivers an event the way the backend would: to every subscriber. */
function emit(channel: string, payload: FlashEventDto) {
  for (const s of subscriptions.filter((s) => s.channel === channel)) {
    s.handler({ payload });
  }
}

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (channel: string, handler: Handler) => {
    const entry = { channel, handler };
    subscriptions.push(entry);
    return Promise.resolve(() => {
      const at = subscriptions.indexOf(entry);
      if (at >= 0) {
        subscriptions.splice(at, 1);
      }
    });
  },
}));

/// Renders the hook and exposes the pieces of state the assertions need.
function Harness({ onReady }: { onReady?: (api: ReturnType<typeof useFlashProgrammer>) => void }) {
  const { state } = useAppContext();
  useFlashTelemetry();
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

/** Another component using the imperative hook, as the real panels do. */
function Consumer() {
  useFlashProgrammer();
  return null;
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
    subscriptions.length = 0;
  });

  it("subscribes to the three telemetry channels instead of polling", async () => {
    renderHook();
    await waitFor(() => expect(channels().size).toBe(3));
    expect([...channels()].sort()).toEqual([
      "flash:log",
      "flash:progress",
      "flash:status",
    ]);
  });

  it("subscribes once however many components consume the hook", async () => {
    render(
      <AppProvider>
        <Harness />
        <Consumer />
        <Consumer />
      </AppProvider>
    );

    await waitFor(() => expect(channels().size).toBe(3));
    // One subscription per channel. Any more and each event is handled once
    // per mounted component, which puts three copies of every line in the
    // console.
    expect(subscriptions.length).toBe(3);

    emit("flash:log", { type: "Warning", message: "Sector already erased" });

    await waitFor(() =>
      expect(
        screen.getByTestId("logs").textContent!.split("|").filter(
          (line) => line === "Sector already erased"
        ).length
      ).toBe(1)
    );
  });

  it("applies pushed progress events to state", async () => {
    renderHook();
    await waitFor(() => expect(channels().has("flash:progress")).toBe(true));

    emit("flash:progress", {
      type: "Progress",
      stage: "programming",
      bytes_transferred: 512,
      total_bytes: 1024,
      percentage: 50,
      speed_bps: 4096,
      elapsed_ms: 120,
      current_address: 0x08000200,
      message: "Programming...",
    });

    await waitFor(() =>
      expect(screen.getByTestId("percentage").textContent).toBe("50")
    );
  });

  it("logs pushed warnings", async () => {
    renderHook();
    await waitFor(() => expect(channels().has("flash:log")).toBe(true));

    emit("flash:log", { type: "Warning", message: "Sector already erased" });

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
