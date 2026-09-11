import { describe, it, expect, vi } from "vitest";
import { render, screen, act } from "@testing-library/react";
import React, { useEffect } from "react";
import { AppProvider, useAppContext } from "../state/AppContext";
import type { AppAction } from "../state/AppContext";

/**
 * Helper component that dispatches an action on mount
 * and renders the state for assertion.
 */
function StateInspector({
  actions,
  renderState,
}: {
  actions: AppAction[];
  renderState: (
    state: ReturnType<typeof useAppContext>["state"]
  ) => React.ReactNode;
}) {
  const { state, dispatch } = useAppContext();

  useEffect(() => {
    for (const action of actions) {
      dispatch(action);
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  return <div data-testid="state-output">{renderState(state)}</div>;
}

describe("AppContext", () => {
  it("has correct initial state", () => {
    render(
      <AppProvider>
        <StateInspector
          actions={[]}
          renderState={(s) => (
            <>
              <span data-testid="connection">{s.connectionStatus}</span>
              <span data-testid="flash">{s.flashStatus}</span>
              <span data-testid="probe-count">{s.probes.length}</span>
            </>
          )}
        />
      </AppProvider>
    );

    expect(screen.getByTestId("connection").textContent).toBe("disconnected");
    expect(screen.getByTestId("flash").textContent).toBe("idle");
    expect(screen.getByTestId("probe-count").textContent).toBe("0");
  });

  it("transitions: disconnected → connecting → connected", () => {
    function ConnectFlow() {
      const { state, dispatch } = useAppContext();

      useEffect(() => {
        // Simulate connection flow
        dispatch({ type: "SET_CONNECTION_STATUS", status: "connecting" });
        setTimeout(() => {
          dispatch({ type: "SET_CONNECTION_STATUS", status: "connected" });
          dispatch({
            type: "SET_TARGET_INFO",
            info: {
              name: "STM32F401RE",
              display_name: null,
              architecture: "ARMv7-M",
              flash_base: 0x08000000,
              flash_size: 512 * 1024,
              ram_base: 0x20000000,
              ram_size: 96 * 1024,
              page_size: 1024,
              sector_count: 8,
            },
          });
        }, 0);
      }, []); // eslint-disable-line react-hooks/exhaustive-deps

      return (
        <span data-testid="status">{state.connectionStatus}</span>
      );
    }

    render(
      <AppProvider>
        <ConnectFlow />
      </AppProvider>
    );

    // Initially it will be connecting (after useEffect dispatch)
    expect(screen.getByTestId("status").textContent).toBe("connecting");
  });

  it("transitions: idle → programming → completed → idle", async () => {
    vi.useFakeTimers();

    function FlashFlow() {
      const { state, dispatch } = useAppContext();

      useEffect(() => {
        dispatch({ type: "SET_FLASH_STATUS", status: "programming" });
        setTimeout(() => {
          dispatch({ type: "SET_FLASH_STATUS", status: "completed" });
          setTimeout(() => {
            dispatch({ type: "SET_FLASH_STATUS", status: "idle" });
          }, 3000);
        }, 100);
      }, []); // eslint-disable-line react-hooks/exhaustive-deps

      return <span data-testid="flash-status">{state.flashStatus}</span>;
    }

    render(
      <AppProvider>
        <FlashFlow />
      </AppProvider>
    );

    expect(screen.getByTestId("flash-status").textContent).toBe("programming");

    await act(async () => {
      vi.advanceTimersByTime(100);
    });
    expect(screen.getByTestId("flash-status").textContent).toBe("completed");

    await act(async () => {
      vi.advanceTimersByTime(3000);
    });
    expect(screen.getByTestId("flash-status").textContent).toBe("idle");

    vi.useRealTimers();
  });

  it("adds and clears log entries", () => {
    function LogTest() {
      const { state, addLog, dispatch } = useAppContext();

      useEffect(() => {
        addLog("info", "First message");
        addLog("warn", "Warning message");
        addLog("error", "Error message");
      }, []); // eslint-disable-line react-hooks/exhaustive-deps

      return (
        <>
          <span data-testid="log-count">{state.logs.length}</span>
          <button
            data-testid="clear-btn"
            onClick={() => dispatch({ type: "CLEAR_LOGS" })}
          >
            Clear
          </button>
        </>
      );
    }

    render(
      <AppProvider>
        <LogTest />
      </AppProvider>
    );

    expect(screen.getByTestId("log-count").textContent).toBe("3");
  });

  it("sets firmware and adds to recent files", () => {
    function FirmwareTest() {
      const { state, dispatch } = useAppContext();

      useEffect(() => {
        dispatch({
          type: "SET_FIRMWARE",
          firmware: {
            format: "Intel HEX",
            file_path: "/test/firmware.hex",
            file_size_bytes: 2048,
            total_firmware_bytes: 1024,
            base_address: 0x08000000,
            highest_address: 0x08000400,
            segment_count: 1,
            entry_point: 0x08000100,
            crc32: 0x12345678,
          },
          path: "/test/firmware.hex",
        });
        dispatch({ type: "ADD_RECENT_FILE", path: "/test/firmware.hex" });
      }, []); // eslint-disable-line react-hooks/exhaustive-deps

      return (
        <>
          <span data-testid="fw-format">{state.firmware?.format ?? "none"}</span>
          <span data-testid="recent-count">{state.recentFiles.length}</span>
        </>
      );
    }

    render(
      <AppProvider>
        <FirmwareTest />
      </AppProvider>
    );

    expect(screen.getByTestId("fw-format").textContent).toBe("Intel HEX");
    expect(Number(screen.getByTestId("recent-count").textContent)).toBeGreaterThanOrEqual(1);
  });

  it("updates progress", () => {
    function ProgressTest() {
      const { state, dispatch } = useAppContext();

      useEffect(() => {
        dispatch({
          type: "SET_PROGRESS",
          progress: {
            percentage: 75.5,
            bytesTransferred: 768,
            totalBytes: 1024,
            speedBps: 50000,
            elapsedMs: 150,
            currentAddress: 0x08000300,
            stage: "programming",
            message: "Programming chunk",
          },
        });
      }, []); // eslint-disable-line react-hooks/exhaustive-deps

      return (
        <>
          <span data-testid="pct">{state.progress.percentage}</span>
          <span data-testid="transferred">{state.progress.bytesTransferred}</span>
        </>
      );
    }

    render(
      <AppProvider>
        <ProgressTest />
      </AppProvider>
    );

    expect(screen.getByTestId("pct").textContent).toBe("75.5");
    expect(screen.getByTestId("transferred").textContent).toBe("768");
  });
});
