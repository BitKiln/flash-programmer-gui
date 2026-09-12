import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { AppProvider } from "../state/AppContext";
import { HistoryPanel, fileName, formatWhen } from "../components/HistoryPanel";
import type { HistoryRecord } from "../types";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

function record(overrides: Partial<HistoryRecord> = {}): HistoryRecord {
  return {
    timestamp_ms: 1_789_652_707_000,
    operation: "flash",
    outcome: "succeeded",
    target: "STM32F401RE",
    probe: "probe:0483:374b",
    file_path: "/home/build/app.hex",
    image_crc32: 0xdeadbeef,
    bytes: 1024,
    duration_ms: 350,
    serial: null,
    verified: true,
    message: "Successfully flashed",
    ...overrides,
  };
}

describe("history formatting", () => {
  it("shows only the file name, since a build path crowds out the rest", () => {
    expect(fileName("/home/build/app.hex")).toBe("app.hex");
    expect(fileName("C:\\builds\\app.bin")).toBe("app.bin");
    expect(fileName(null)).toBe("");
  });

  it("does not pretend to know a time it cannot read", () => {
    expect(formatWhen(Number.NaN)).toBe("unknown");
  });
});

describe("HistoryPanel", () => {
  beforeEach(() => invoke.mockReset());

  it("names the build by its checksum rather than only by file", async () => {
    invoke.mockResolvedValue([record()]);

    render(
      <AppProvider>
        <HistoryPanel />
      </AppProvider>
    );

    expect(await screen.findByText(/app\.hex/)).toBeTruthy();
    expect(screen.getByText(/crc32 DEADBEEF/)).toBeTruthy();
  });

  it("marks a success that was never verified", async () => {
    invoke.mockResolvedValue([record({ verified: false })]);

    render(
      <AppProvider>
        <HistoryPanel />
      </AppProvider>
    );

    // A flash that was not checked against the target is a weaker claim, and
    // the row has to say which kind it was.
    expect(await screen.findByText("unverified")).toBeTruthy();
  });

  it("shows why a failure failed", async () => {
    invoke.mockResolvedValue([
      record({ outcome: "failed", message: "target did not answer" }),
    ]);

    render(
      <AppProvider>
        <HistoryPanel />
      </AppProvider>
    );

    expect(await screen.findByText(/target did not answer/)).toBeTruthy();
  });

  it("filters to failures without re-reading the file", async () => {
    invoke.mockResolvedValue([
      record({ target: "GOOD_BOARD" }),
      record({ target: "BAD_BOARD", outcome: "failed" }),
    ]);

    render(
      <AppProvider>
        <HistoryPanel />
      </AppProvider>
    );
    await screen.findByText("GOOD_BOARD", { exact: false });
    const readsBefore = invoke.mock.calls.length;

    fireEvent.click(screen.getByLabelText(/Failures only/i));

    expect(await screen.findByText(/BAD_BOARD/)).toBeTruthy();
    expect(screen.queryByText(/GOOD_BOARD/)).toBeNull();
    expect(invoke.mock.calls.length).toBe(readsBefore);
  });

  it("explains an empty history rather than showing a bare table", async () => {
    invoke.mockResolvedValue([]);

    render(
      <AppProvider>
        <HistoryPanel />
      </AppProvider>
    );

    expect(await screen.findByText(/Nothing recorded yet/)).toBeTruthy();
    // The exclusion is worth stating: someone testing in simulator mode would
    // otherwise think the recording is broken.
    expect(screen.getByText(/Simulator runs are left out/)).toBeTruthy();
  });

  it("says so when there are records but no failures among them", async () => {
    invoke.mockResolvedValue([record(), record()]);

    render(
      <AppProvider>
        <HistoryPanel />
      </AppProvider>
    );
    await screen.findAllByText(/app\.hex/);

    fireEvent.click(screen.getByLabelText(/Failures only/i));

    await waitFor(() =>
      expect(screen.getByText(/No failures in the 2 most recent/)).toBeTruthy()
    );
  });
});
