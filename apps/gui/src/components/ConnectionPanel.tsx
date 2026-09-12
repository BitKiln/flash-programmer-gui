import { useCallback, useEffect, useState } from "react";
import { useAppContext } from "../state/AppContext";
import { useFlashProgrammer } from "../hooks/useFlashProgrammer";
import type { ProfileSummary, TargetSuggestion } from "../types";

const RECENT_TARGETS_KEY = "flashgui.recentTargets";
const RECENT_LIMIT = 5;

/** Reads the recently connected parts; storage may be unavailable or stale. */
function loadRecentTargets(): string[] {
  try {
    const raw = window.localStorage.getItem(RECENT_TARGETS_KEY);
    const parsed = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed)
      ? parsed.filter((t): t is string => typeof t === "string").slice(0, RECENT_LIMIT)
      : [];
  } catch {
    return [];
  }
}

function saveRecentTargets(targets: string[]) {
  try {
    window.localStorage.setItem(RECENT_TARGETS_KEY, JSON.stringify(targets));
  } catch {
    // Storage disabled - recents are a convenience, not state we depend on.
  }
}

export function ConnectionPanel() {
  const { state, dispatch } = useAppContext();
  const {
    refreshProbes,
    connectProbe,
    disconnectProbe,
    autoDetectTarget,
    listTargetSuggestions,
    listProfiles,
    loadProfile,
    saveProfile,
  } = useFlashProgrammer();

  const [mode, setMode] = useState<
    "hardware" | "serial" | "openocd" | "simulator"
  >("hardware");

  // Where a running OpenOCD is listening. Only meaningful in "openocd" mode;
  // 6666 is the TCL port OpenOCD serves unless its configuration says
  // otherwise.
  const [endpoint, setEndpoint] = useState("127.0.0.1:6666");
  // Empty means "identify the chip on connect". Never guess a part number:
  // a wrong one attaches happily and only misbehaves when erasing or writing.
  const [target, setTarget] = useState("");
  // Parts this user actually connected to, most recent first.
  const [recentTargets, setRecentTargets] = useState<string[]>(loadRecentTargets);
  const [protocol, setProtocol] = useState("Swd");
  const [speed, setSpeed] = useState(4000);
  // Serial bootloader rate. Only meaningful in "serial" mode; 460800 is what
  // esptool and ESP-IDF default to.
  const [baud, setBaud] = useState(460800);
  const [isDetecting, setIsDetecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [targetSuggestions, setTargetSuggestions] = useState<TargetSuggestion[]>([]);
  const [selectedProfile, setSelectedProfile] = useState("");

  const refreshProfiles = useCallback(async () => {
    setProfiles(await listProfiles());
  }, [listProfiles]);

  useEffect(() => {
    void refreshProfiles();
  }, [refreshProfiles]);

  useEffect(() => {
    void listTargetSuggestions().then(setTargetSuggestions);
  }, [listTargetSuggestions]);

  const handleApplyProfile = async (name: string) => {
    setSelectedProfile(name);
    if (!name) return;
    const profile = await loadProfile(name);
    if (profile) {
      setTarget(profile.target === "auto" ? "" : profile.target);
      setProtocol(profile.interface.toLowerCase() === "jtag" ? "Jtag" : "Swd");
      setSpeed(profile.speed_khz);
    }
  };

  const handleSaveProfile = async () => {
    const name = window.prompt("Save current settings as profile named:");
    if (!name?.trim()) return;
    const saved = await saveProfile({
      name: name.trim(),
      description: null,
      target: target.trim() || "auto",
      probe_id: state.selectedProbe,
      interface: protocol.toLowerCase() === "jtag" ? "JTAG" : "SWD",
      speed_khz: speed,
      firmware_path: state.firmwarePath,
      base_address: null,
      verify: state.flashOptions.verify,
      reset: state.flashOptions.reset,
      full_chip_erase: state.flashOptions.chipErase,
    });
    if (saved) {
      setSelectedProfile(name.trim());
      await refreshProfiles();
    }
  };

  const rememberTarget = (name: string) => {
    setRecentTargets((previous) => {
      const next = [name, ...previous.filter((t) => t !== name)].slice(0, RECENT_LIMIT);
      saveRecentTargets(next);
      return next;
    });
  };

  // Auto-refresh probes on mount
  useEffect(() => {
    refreshProbes();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const handleAutoDetect = async () => {
    setIsDetecting(true);
    setError(null);
    try {
      const { info, error: detectError } = await autoDetectTarget(
        state.selectedProbe,
        protocol,
        speed,
        isSerial ? baud : undefined
      );
      // info.name is always a registry chip name, so Connect can reuse it as-is.
      if (info) {
        setTarget(info.name);
        rememberTarget(info.name);
      }
      setError(detectError);
    } finally {
      setIsDetecting(false);
    }
  };

  // Three kinds of thing can be on the other end now, and the identifier's
  // scheme is what says which: "mock:" a simulation, "esp:" a serial
  // bootloader, anything else a debug probe.
  const hardwareProbes = state.probes.filter(
    (p) =>
      !p.identifier.startsWith("mock:") &&
      !p.identifier.startsWith("esp:") &&
      !p.identifier.startsWith("openocd:") &&
      p.probe_type !== "VirtualMock"
  );
  const serialProbes = state.probes.filter((p) =>
    p.identifier.startsWith("esp:")
  );
  const simulatorProbes = state.probes.filter(
    (p) => p.identifier.startsWith("mock:") || p.probe_type === "VirtualMock"
  );

  const displayedProbes =
    mode === "hardware"
      ? hardwareProbes
      : mode === "serial"
        ? serialProbes
        : simulatorProbes;

  // A serial bootloader has no wire protocol and no debug clock; it has a baud
  // rate. Showing SWD/JTAG and a kHz field for one would be a lie about what is
  // being configured.
  const isSerial = mode === "serial";

  // OpenOCD's own configuration chooses the adapter, the wire and the clock
  // before it starts listening, so this program has nothing to configure
  // about them and says so by not offering the controls.
  const isOpenOcd = mode === "openocd";

  // In OpenOCD mode the identifier comes from the endpoint field rather than
  // from a list: nothing can enumerate OpenOCD processes, and a socket that
  // answers is not proof an adapter is attached.
  useEffect(() => {
    if (!isOpenOcd) return;
    const identifier = `openocd:${endpoint.trim()}`;
    if (state.selectedProbe !== identifier) {
      dispatch({ type: "SELECT_PROBE", probeId: identifier });
    }
  }, [isOpenOcd, endpoint, state.selectedProbe, dispatch]);

  // Auto-select first probe when switching modes or when probes update
  useEffect(() => {
    if (isOpenOcd) return;
    if (displayedProbes.length > 0) {
      const currentInList = displayedProbes.some(
        (p) => p.identifier === state.selectedProbe
      );
      if (!currentInList) {
        dispatch({
          type: "SELECT_PROBE",
          probeId: displayedProbes[0].identifier,
        });
      }
    } else if (mode === "hardware" && state.selectedProbe !== null) {
      // Only when it actually changes. `displayedProbes` is rebuilt every
      // render, so an unconditional dispatch here re-renders, re-runs this
      // effect, and spins forever -- which is what happens with no probe
      // attached.
      dispatch({ type: "SELECT_PROBE", probeId: null });
    }
  }, [mode, isOpenOcd, displayedProbes, state.selectedProbe, dispatch]);

  const handleConnect = async () => {
    setError(null);
    const failure = await connectProbe(
      state.selectedProbe,
      target,
      protocol,
      speed,
      isSerial ? baud : undefined
    );
    setError(failure);
    if (!failure && target.trim()) {
      rememberTarget(target.trim());
    }
  };

  const handleDisconnect = async () => {
    setError(await disconnectProbe());
  };

  const isConnected = state.connectionStatus === "connected";
  const isConnecting = state.connectionStatus === "connecting";

  return (
    <div className="flex flex-col h-full min-w-0 overflow-y-auto overflow-x-hidden p-4 space-y-4 bg-bg-secondary border-r border-gray-700">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider">
          Connection
        </h2>
        <span
          className={`text-[10px] font-mono px-1.5 py-0.5 rounded ${
            mode === "hardware"
              ? "bg-green-950 text-green-400 border border-green-800"
              : mode === "serial"
                ? "bg-sky-950 text-sky-400 border border-sky-800"
                : mode === "openocd"
                  ? "bg-violet-950 text-violet-400 border border-violet-800"
                  : "bg-amber-950 text-amber-400 border border-amber-800"
          }`}
        >
          {mode === "hardware"
            ? "PROBE-RS HARDWARE"
            : mode === "serial"
              ? "ESP SERIAL"
              : mode === "openocd"
                ? "OPENOCD"
                : "SIMULATOR"}
        </span>
      </div>

      {/* Profiles — the same TOML store the CLI reads */}
      <div>
        <label className="block text-xs text-gray-400 mb-1">Profile</label>
        <div className="flex gap-2">
          <select
            aria-label="Profile"
            className="flex-1 min-w-0 truncate bg-bg-primary border border-gray-600 rounded px-2 py-1.5 text-sm text-gray-200 focus:border-accent-red focus:outline-none"
            value={selectedProfile}
            onChange={(e) => void handleApplyProfile(e.target.value)}
          >
            <option value="">
              {profiles.length === 0 ? "No saved profiles" : "Select a profile..."}
            </option>
            {profiles.map((profile) => (
              <option key={profile.name} value={profile.name}>
                {profile.name} ({profile.target})
              </option>
            ))}
          </select>
          <button
            onClick={handleSaveProfile}
            className="shrink-0 px-2.5 py-1.5 bg-bg-tertiary border border-gray-600 rounded text-sm text-gray-300 hover:bg-gray-600 transition-colors"
            title="Save the current probe, target, interface, speed and options as a profile"
          >
            Save
          </button>
        </div>
      </div>

      {/* Mode Switcher: what is on the other end */}
      <div className="grid grid-cols-2 gap-1 p-1 bg-bg-primary rounded border border-gray-700">
        <button
          type="button"
          onClick={() => setMode("hardware")}
          className={`py-1 text-xs font-semibold rounded transition-colors ${
            mode === "hardware"
              ? "bg-accent-red text-white shadow-sm"
              : "text-gray-400 hover:text-gray-200"
          }`}
          title="ST-Link, CMSIS-DAP or J-Link over SWD/JTAG"
        >
          ⚡ Probe
        </button>
        <button
          type="button"
          onClick={() => setMode("serial")}
          className={`py-1 text-xs font-semibold rounded transition-colors ${
            mode === "serial"
              ? "bg-sky-600 text-white shadow-sm"
              : "text-gray-400 hover:text-gray-200"
          }`}
          title="ESP32 over the serial/USB ROM bootloader, with no debug probe"
        >
          🔌 ESP Serial
        </button>
        <button
          type="button"
          onClick={() => setMode("openocd")}
          className={`py-1 text-xs font-semibold rounded transition-colors ${
            mode === "openocd"
              ? "bg-violet-600 text-white shadow-sm"
              : "text-gray-400 hover:text-gray-200"
          }`}
          title="A running OpenOCD, over its TCL port — whatever adapter and target it is configured for"
        >
          🔗 OpenOCD
        </button>
        <button
          type="button"
          onClick={() => setMode("simulator")}
          className={`py-1 text-xs font-semibold rounded transition-colors ${
            mode === "simulator"
              ? "bg-amber-600 text-white shadow-sm"
              : "text-gray-400 hover:text-gray-200"
          }`}
          title="A simulated target, for trying the tool with no board"
        >
          🧪 Simulator
        </button>
      </div>
      {/* Where to connect. In OpenOCD mode this is an address rather than a
          choice from a list, because nothing can enumerate OpenOCD
          processes. */}
      {isOpenOcd ? (
        <div>
          <label className="block text-xs text-gray-400 mb-1" htmlFor="openocd-endpoint">
            OpenOCD TCL endpoint
          </label>
          <input
            id="openocd-endpoint"
            type="text"
            value={endpoint}
            onChange={(e) => setEndpoint(e.target.value)}
            placeholder="127.0.0.1:6666"
            className="w-full bg-bg-primary border border-gray-600 rounded px-2 py-1.5 text-sm text-gray-200 font-mono focus:border-accent-red focus:outline-none"
          />
          <p className="text-xs text-gray-500 mt-1.5">
            Start OpenOCD with its TCL port enabled — it listens on{" "}
            <code>6666</code> by default. The adapter, the wire and the clock
            come from OpenOCD&apos;s own configuration.
          </p>
          <p className="text-xs text-gray-500 mt-1.5">
            Programming needs OpenOCD on <strong>this machine</strong>: it
            opens the image file itself, so a path here means nothing to a
            process elsewhere. Reading and erasing work over the network.
          </p>
          {state.selectedProbe && (
            <p className="text-xs text-gray-500 mt-1.5 font-mono truncate">
              ID: {state.selectedProbe}
            </p>
          )}
        </div>
      ) : (
      <div>
        <label className="block text-xs text-gray-400 mb-1">
          {mode === "hardware"
            ? "Physical Debug Probe"
            : mode === "serial"
              ? "Serial Port"
              : "Virtual Probe Model"}
        </label>
        <div className="flex gap-2">
          <select
            className="flex-1 min-w-0 truncate bg-bg-primary border border-gray-600 rounded px-2 py-1.5 text-sm text-gray-200 focus:border-accent-red focus:outline-none"
            value={state.selectedProbe ?? ""}
            onChange={(e) =>
              dispatch({
                type: "SELECT_PROBE",
                probeId: e.target.value || null,
              })
            }
          >
            {displayedProbes.length === 0 && (
              <option value="">
                {mode === "hardware"
                  ? "No hardware probe detected"
                  : mode === "serial"
                    ? "No serial ports found"
                    : "No simulation probes"}
              </option>
            )}
            {displayedProbes.map((p) => (
              <option key={p.identifier} value={p.identifier}>
                {mode === "hardware" ? `⚡ ${p.product_name}` : p.product_name}
              </option>
            ))}
          </select>
          <button
            onClick={refreshProbes}
            className="shrink-0 px-2.5 py-1.5 bg-bg-tertiary border border-gray-600 rounded text-sm text-gray-300 hover:bg-gray-600 transition-colors"
            title="Scan for connected probes and serial ports"
          >
            ⟳
          </button>
        </div>

        {mode === "serial" && displayedProbes.length === 0 && (
          <div className="mt-2.5 p-2.5 bg-bg-primary border border-sky-900/60 rounded text-xs space-y-1.5">
            <div className="flex items-center gap-1.5 text-sky-400 font-medium">
              <span>⚠</span> No serial port detected
            </div>
            <p className="text-gray-300">
              Connect an ESP board over USB and click <strong>⟳</strong>. The board must
              be in <strong>download mode</strong>: hold <strong>BOOT</strong> while tapping
              <strong> RESET</strong> if it does not enter it by itself.
            </p>
            <p className="text-gray-400">
              On Linux you also need to be in the group that owns the port, usually
              <code> dialout</code>.
            </p>
          </div>
        )}

        {mode === "hardware" && displayedProbes.length === 0 && (
          <div className="mt-2.5 p-2.5 bg-bg-primary border border-amber-900/60 rounded text-xs space-y-1.5">
            <div className="flex items-center gap-1.5 text-amber-400 font-medium">
              <span>⚠</span> No physical probe detected
            </div>
            <p className="text-gray-300">
              Connect an <strong>ST-Link (V2/V3)</strong>, <strong>CMSIS-DAP</strong>, <strong>Raspberry Pi PicoProbe</strong>, or <strong>J-Link</strong> via USB and click <strong>⟳</strong>.
            </p>
            <p className="text-gray-400">
              Want to test without a board? Switch to{" "}
              <button
                type="button"
                onClick={() => setMode("simulator")}
                className="text-accent-red hover:underline font-medium"
              >
                Virtual Simulator
              </button>
              .
            </p>
          </div>
        )}

        {state.selectedProbe && (
          <p className="text-xs text-gray-500 mt-1 font-mono truncate">
            ID: {state.selectedProbe}
          </p>
        )}
      </div>
      )}

      {/* Target MCU */}
      <div>
        <div className="flex items-center justify-between mb-1">
          <label className="block text-xs text-gray-400">Target MCU</label>
          <button
            type="button"
            onClick={handleAutoDetect}
            disabled={isDetecting || !state.selectedProbe}
            className="text-xs text-accent-red hover:underline flex items-center gap-1 font-medium disabled:opacity-50 disabled:hover:no-underline transition-colors"
            title={
              isSerial
                ? "Ask the ESP bootloader which chip it is running on"
                : "Auto-detect connected MCU chip via SWD/JTAG IDCODE"
            }
          >
            {isDetecting ? "Detecting..." : "🔍 Auto-Detect"}
          </button>
        </div>
        <input
          type="text"
          list="target-presets"
          className="w-full bg-bg-primary border border-gray-600 rounded px-2 py-1.5 text-sm text-gray-200 font-mono focus:border-accent-red focus:outline-none"
          value={target}
          onChange={(e) => setTarget(e.target.value)}
          placeholder="empty = auto-detect"
        />
        <datalist id="target-presets">
          <option value="auto">auto (identify the connected chip)</option>
          {targetSuggestions.map((suggestion) => (
            <option key={suggestion.value} value={suggestion.value}>
              {suggestion.label}
            </option>
          ))}
        </datalist>
        {recentTargets.length > 0 && (
          <div className="flex flex-wrap gap-1 mt-1.5">
            {recentTargets.map((preset) => (
              <button
                key={preset}
                type="button"
                onClick={() => setTarget(preset)}
                title="Recently connected target"
                className={`text-[10px] px-1.5 py-0.5 rounded border transition-colors ${
                  target === preset
                    ? "bg-accent-red/20 text-accent-red border-accent-red/40 font-semibold"
                    : "bg-bg-tertiary text-gray-400 border-gray-700 hover:text-gray-200 hover:border-gray-500"
                }`}
              >
                {preset}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* What the wire actually is. A serial bootloader has no wire
          protocol and no debug clock, so those controls are replaced rather
          than left on screen configuring nothing. */}
      {isOpenOcd ? null : isSerial ? (
        <div>
          <label className="block text-xs text-gray-400 mb-1">Baud rate</label>
          <select
            className="w-full bg-bg-primary border border-gray-600 rounded px-2 py-1.5 text-sm text-gray-200 focus:border-accent-red focus:outline-none"
            value={baud}
            onChange={(e) => setBaud(Number(e.target.value))}
          >
            <option value={115200}>115200 (slowest, most reliable)</option>
            <option value={230400}>230400</option>
            <option value={460800}>460800 (default)</option>
            <option value={921600}>921600 (fastest, needs a good bridge)</option>
          </select>
          <p className="text-xs text-gray-500 mt-1">
            Flash is addressed by <strong>offset</strong> on an ESP part, not by the
            memory-mapped address. A raw <code>.bin</code> application image usually
            goes at <code>0x10000</code>.
          </p>
        </div>
      ) : (
        <>
          {/* Protocol */}
          <div>
            <label className="block text-xs text-gray-400 mb-1">Interface</label>
            <div className="flex gap-4">
              <label className="flex items-center gap-1.5 text-sm text-gray-300 cursor-pointer">
                <input
                  type="radio"
                  name="protocol"
                  value="Swd"
                  checked={protocol === "Swd"}
                  onChange={(e) => setProtocol(e.target.value)}
                  className="accent-accent-red"
                />
                SWD
              </label>
              <label className="flex items-center gap-1.5 text-sm text-gray-300 cursor-pointer">
                <input
                  type="radio"
                  name="protocol"
                  value="Jtag"
                  checked={protocol === "Jtag"}
                  onChange={(e) => setProtocol(e.target.value)}
                  className="accent-accent-red"
                />
                JTAG
              </label>
            </div>
          </div>

          {/* Speed */}
          <div>
            <label className="block text-xs text-gray-400 mb-1">Speed (kHz)</label>
            <select
              className="w-full bg-bg-primary border border-gray-600 rounded px-2 py-1.5 text-sm text-gray-200 focus:border-accent-red focus:outline-none"
              value={speed}
              onChange={(e) => setSpeed(Number(e.target.value))}
            >
              <option value={1000}>1000 kHz</option>
              <option value={2000}>2000 kHz</option>
              <option value={4000}>4000 kHz</option>
              <option value={8000}>8000 kHz</option>
            </select>
          </div>
        </>
      )}
      {/* One button, two states: connecting holds the probe, disconnecting
          releases it for other tools. */}
      <button
        onClick={isConnected ? handleDisconnect : handleConnect}
        disabled={isConnecting || (!isConnected && !state.selectedProbe)}
        title={
          isConnected
            ? "Close the session and release the probe for other tools"
            : "Open a session on the selected probe"
        }
        className={`w-full py-2 rounded text-sm font-medium transition-colors ${
          isConnected
            ? "bg-bg-tertiary border border-gray-600 text-gray-200 hover:bg-gray-600"
            : isConnecting
              ? "bg-yellow-700 text-white cursor-wait"
              : "bg-accent-red hover:bg-red-500 text-white"
        } disabled:opacity-50 disabled:cursor-not-allowed`}
      >
        {isConnecting ? "Connecting..." : isConnected ? "Disconnect" : "Connect"}
      </button>

      {/* Status Indicator */}
      <div className="flex items-center gap-2">
        <div
          className={`w-2.5 h-2.5 rounded-full ${
            isConnected
              ? "bg-green-400 shadow-[0_0_6px_rgba(74,222,128,0.6)]"
              : state.connectionStatus === "error"
                ? "bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.6)]"
                : "bg-gray-500"
          }`}
        />
        <span className="text-xs text-gray-400">
          {isConnected
            ? `Connected — ${state.targetInfo?.name ?? "Unknown"}`
            : state.connectionStatus === "error"
              ? "Connection error"
              : state.connectionStatus === "connecting"
                ? "Connecting..."
                : "Disconnected"}
        </span>
      </div>

      {error && (
        <div className="p-2 bg-red-950/50 border border-red-800 rounded text-xs text-red-300 break-words">
          {error}
        </div>
      )}

      {/* Target Info */}
      {state.targetInfo && (
        <div className="mt-auto pt-4 border-t border-gray-700 space-y-1">
          <h3 className="text-xs text-gray-400 uppercase tracking-wider">
            Target Info
          </h3>
          <div className="text-xs text-gray-300 font-mono space-y-0.5">
            <p>
              Arch: <span className="text-gray-100">{state.targetInfo.architecture}</span>
            </p>
            {state.targetInfo.display_name && (
              <p>
                ID: <span className="text-gray-100">{state.targetInfo.display_name}</span>
              </p>
            )}
            <p>
              Flash: <span className="text-gray-100">0x{state.targetInfo.flash_base.toString(16).toUpperCase().padStart(8, "0")}</span>{" "}
              ({(state.targetInfo.flash_size / 1024).toFixed(0)} KB)
            </p>
            <p>
              RAM: <span className="text-gray-100">0x{state.targetInfo.ram_base.toString(16).toUpperCase().padStart(8, "0")}</span>{" "}
              ({(state.targetInfo.ram_size / 1024).toFixed(0)} KB)
            </p>
            <p>
              Page: <span className="text-gray-100">{state.targetInfo.page_size}</span> B / {state.targetInfo.sector_count} sectors
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
