import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import React, { useEffect } from "react";
import { AppProvider, useAppContext } from "../state/AppContext";
import { ConsoleOutput } from "../components/ConsoleOutput";

describe("ConsoleOutput", () => {
  it("renders empty state message", () => {
    render(
      <AppProvider>
        <ConsoleOutput />
      </AppProvider>
    );

    expect(screen.getByText(/no log entries yet/i)).toBeTruthy();
  });

  it("renders log entries with correct content", () => {
    function LogLoader({ children }: { children: React.ReactNode }) {
      const { addLog } = useAppContext();

      useEffect(() => {
        addLog("info", "Connected to probe");
        addLog("warn", "Flash region partially erased");
        addLog("error", "Verification failed at 0x08001000");
        addLog("success", "Programming completed");
      }, []); // eslint-disable-line react-hooks/exhaustive-deps

      return <>{children}</>;
    }

    render(
      <AppProvider>
        <LogLoader>
          <ConsoleOutput />
        </LogLoader>
      </AppProvider>
    );

    expect(screen.getByText("Connected to probe")).toBeTruthy();
    expect(screen.getByText("Flash region partially erased")).toBeTruthy();
    expect(screen.getByText("Verification failed at 0x08001000")).toBeTruthy();
    expect(screen.getByText("Programming completed")).toBeTruthy();
  });

  it("shows correct entry count", () => {
    function LogLoader({ children }: { children: React.ReactNode }) {
      const { addLog } = useAppContext();

      useEffect(() => {
        addLog("info", "Message 1");
        addLog("info", "Message 2");
        addLog("info", "Message 3");
      }, []); // eslint-disable-line react-hooks/exhaustive-deps

      return <>{children}</>;
    }

    render(
      <AppProvider>
        <LogLoader>
          <ConsoleOutput />
        </LogLoader>
      </AppProvider>
    );

    expect(screen.getByText("3 entries")).toBeTruthy();
  });

  it("has a clear button", () => {
    render(
      <AppProvider>
        <ConsoleOutput />
      </AppProvider>
    );

    const clearBtn = screen.getByText("Clear");
    expect(clearBtn).toBeTruthy();
  });

  it("auto-scrolls when new logs are added", () => {
    // Mock scrollIntoView
    const scrollIntoViewMock = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoViewMock;

    function LogLoader({ children }: { children: React.ReactNode }) {
      const { addLog } = useAppContext();

      useEffect(() => {
        addLog("info", "Initial log");
      }, []); // eslint-disable-line react-hooks/exhaustive-deps

      return <>{children}</>;
    }

    render(
      <AppProvider>
        <LogLoader>
          <ConsoleOutput />
        </LogLoader>
      </AppProvider>
    );

    // scrollIntoView should have been called for auto-scroll
    expect(scrollIntoViewMock).toHaveBeenCalled();
  });

  it("displays timestamps in correct format", () => {
    function LogLoader({ children }: { children: React.ReactNode }) {
      const { addLog } = useAppContext();

      useEffect(() => {
        addLog("info", "Timestamped entry");
      }, []); // eslint-disable-line react-hooks/exhaustive-deps

      return <>{children}</>;
    }

    render(
      <AppProvider>
        <LogLoader>
          <ConsoleOutput />
        </LogLoader>
      </AppProvider>
    );

    // Timestamps should match HH:MM:SS.mmm format
    const entry = screen.getByText("Timestamped entry");
    expect(entry).toBeTruthy();

    // The timestamp sibling should exist
    const parent = entry.parentElement;
    expect(parent).toBeTruthy();
    if (parent) {
      const timestampEl = parent.querySelector("span");
      expect(timestampEl).toBeTruthy();
      if (timestampEl) {
        expect(timestampEl.textContent).toMatch(/\d{2}:\d{2}:\d{2}\.\d{3}/);
      }
    }
  });
});
