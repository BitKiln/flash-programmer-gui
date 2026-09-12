import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import { useEffect } from "react";
import { AppProvider, useAppContext } from "../state/AppContext";
import { FlashControls } from "../components/FlashControls";
import type { FlashStatus, TargetInfo } from "../types";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

function targetInfo(cancellable: string[]): TargetInfo {
  return {
    name: "STM32U575ZI",
    display_name: null,
    architecture: "Armv8-M",
    flash_base: 0x08000000,
    flash_size: 2 * 1024 * 1024,
    ram_base: 0x20000000,
    ram_size: 786432,
    page_size: 1024,
    sector_count: 256,
    cancellable_stages: cancellable,
  };
}

/// Puts the app in a connected state, in the given stage, against a target
/// that can abort only the listed stages.
function Harness({
  cancellable,
  status,
}: {
  cancellable: string[];
  status: FlashStatus;
}) {
  const { dispatch } = useAppContext();
  useEffect(() => {
    dispatch({ type: "SET_CONNECTION_STATUS", status: "connected" });
    dispatch({ type: "SET_TARGET_INFO", info: targetInfo(cancellable) });
    dispatch({ type: "SET_FLASH_STATUS", status });
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  return <FlashControls />;
}

function renderControls(cancellable: string[], status: FlashStatus) {
  return render(
    <AppProvider>
      <Harness cancellable={cancellable} status={status} />
    </AppProvider>
  );
}

describe("FlashControls cancellation", () => {
  beforeEach(() => invoke.mockReset());

  it("offers Cancel for a stage the backend can abort", () => {
    renderControls(["erasing", "programming", "verifying"], "programming");

    const button = screen.getByRole("button", { name: /Cancel Programming/i });
    expect(button.hasAttribute("disabled")).toBe(false);
  });

  it("says so instead of offering a Stop that cannot act", () => {
    // probe-rs erases inside one driver call: nothing polls the flag until it
    // has finished, so a Cancel button here would be a lie.
    renderControls(["verifying"], "erasing");

    expect(screen.queryByRole("button", { name: /Cancel/i })).toBeNull();
    const button = screen.getByRole("button", {
      name: /cannot be interrupted/i,
    });
    expect(button.hasAttribute("disabled")).toBe(true);
  });

  it("still offers Cancel during verify on the same backend", () => {
    renderControls(["verifying"], "verifying");

    expect(
      screen.getByRole("button", { name: /Cancel Verifying/i }).hasAttribute("disabled")
    ).toBe(false);
  });

  it("does not ask the backend to cancel a stage it cannot stop", async () => {
    renderControls(["verifying"], "erasing");

    await act(async () => {
      screen.getByRole("button", { name: /cannot be interrupted/i }).click();
    });

    expect(invoke).not.toHaveBeenCalledWith("cancel_operation");
  });
});
