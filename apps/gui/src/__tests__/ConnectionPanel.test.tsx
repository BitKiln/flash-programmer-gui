import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, act, fireEvent, waitFor } from "@testing-library/react";
import { AppProvider } from "../state/AppContext";
import { ConnectionPanel } from "../components/ConnectionPanel";
import type { ProbeInfo } from "../types";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

const stlink: ProbeInfo = {
  identifier: "0483:374f:002E00373438",
  vendor_name: "STMicroelectronics",
  product_name: "ST-Link V3",
  serial_number: "002E00373438",
  probe_type: "StLink",
  supported_protocols: ["Swd", "Jtag"],
  default_speed_khz: 4000,
  max_speed_khz: 10000,
};

const espPort: ProbeInfo = {
  identifier: "esp:COM7",
  vendor_name: "Espressif-compatible",
  product_name: "Silicon Labs CP210x",
  serial_number: null,
  probe_type: { Other: "esp-serial" } as unknown as ProbeInfo["probe_type"],
  supported_protocols: [],
  default_speed_khz: 460,
  max_speed_khz: 921,
};

/// Answers the commands the panel issues on mount, so every test starts from a
/// panel that has seen both a debug probe and a serial port.
function scriptBackend() {
  invoke.mockImplementation((command: string) => {
    switch (command) {
      case "list_probes":
        return Promise.resolve([stlink, espPort]);
      case "list_profiles":
      case "list_target_suggestions":
        return Promise.resolve([]);
      default:
        return Promise.resolve(null);
    }
  });
}

async function renderPanel() {
  scriptBackend();
  await act(async () => {
    render(
      <AppProvider>
        <ConnectionPanel />
      </AppProvider>
    );
  });
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("list_probes"));
}

async function switchTo(label: RegExp) {
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: label }));
  });
}

describe("ConnectionPanel transports", () => {
  beforeEach(() => invoke.mockReset());

  it("offers a wire protocol and a debug clock for a probe", async () => {
    await renderPanel();

    expect(screen.getByRole("radio", { name: "SWD" })).toBeTruthy();
    expect(screen.getByText("Speed (kHz)")).toBeTruthy();
    expect(screen.queryByText("Baud rate")).toBeNull();
  });

  it("replaces them with a baud rate for a serial bootloader", async () => {
    await renderPanel();
    await switchTo(/ESP Serial/);

    // A serial bootloader has neither, so leaving the controls on screen would
    // be configuring something that does not exist.
    expect(screen.queryByRole("radio", { name: "SWD" })).toBeNull();
    expect(screen.queryByText("Speed (kHz)")).toBeNull();
    expect(screen.getByText("Baud rate")).toBeTruthy();
  });

  it("keeps serial ports out of the probe list and probes out of the serial list", async () => {
    await renderPanel();

    const probeOptions = screen
      .getAllByRole("option")
      .map((o) => (o as HTMLOptionElement).value);
    expect(probeOptions).toContain(stlink.identifier);
    expect(probeOptions).not.toContain(espPort.identifier);

    await switchTo(/ESP Serial/);
    const serialOptions = screen
      .getAllByRole("option")
      .map((o) => (o as HTMLOptionElement).value);
    expect(serialOptions).toContain(espPort.identifier);
    expect(serialOptions).not.toContain(stlink.identifier);
  });

  it("sends the baud when connecting to a serial target", async () => {
    await renderPanel();
    await switchTo(/ESP Serial/);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /^Connect$/i }));
    });

    expect(invoke).toHaveBeenCalledWith(
      "connect_probe",
      expect.objectContaining({ probeId: "esp:COM7", baud: 460800 })
    );
  });

  it("sends no baud when connecting to a debug probe", async () => {
    await renderPanel();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /^Connect$/i }));
    });

    expect(invoke).toHaveBeenCalledWith(
      "connect_probe",
      expect.objectContaining({ baud: undefined })
    );
  });

  it("asks for an address in OpenOCD mode rather than a choice from a list", async () => {
    await renderPanel();
    await switchTo(/OpenOCD/);

    // Nothing can enumerate OpenOCD processes, and a socket that answers is
    // not proof an adapter is attached, so the endpoint is typed.
    const field = screen.getByLabelText(/OpenOCD TCL endpoint/i) as HTMLInputElement;
    expect(field.value).toBe("127.0.0.1:6666");
    expect(screen.getByText("OPENOCD")).toBeTruthy();
  });

  it("offers no wire protocol, clock or baud rate for OpenOCD", async () => {
    await renderPanel();
    await switchTo(/OpenOCD/);

    // OpenOCD's own configuration chose the adapter, the wire and the clock
    // before it started listening; offering them here would imply a control
    // this program does not have.
    expect(screen.queryByRole("radio", { name: "SWD" })).toBeNull();
    expect(screen.queryByText("Speed (kHz)")).toBeNull();
    expect(screen.queryByText("Baud rate")).toBeNull();
  });

  it("turns the endpoint into an openocd identifier", async () => {
    await renderPanel();
    await switchTo(/OpenOCD/);

    await act(async () => {
      fireEvent.change(screen.getByLabelText(/OpenOCD TCL endpoint/i), {
        target: { value: "buildbox:4444" },
      });
    });

    // The scheme is what routes the request to this backend.
    expect(await screen.findByText(/openocd:buildbox:4444/)).toBeTruthy();
  });

  it("says that programming needs a local OpenOCD", async () => {
    await renderPanel();
    await switchTo(/OpenOCD/);

    // The interlock exists in the backend; saying so where the address is
    // typed is what stops someone discovering it mid-flash.
    expect(screen.getByText(/opens the image file itself/)).toBeTruthy();
  });
});
