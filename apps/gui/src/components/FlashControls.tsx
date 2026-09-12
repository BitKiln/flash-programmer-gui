import { useState } from "react";
import { useAppContext } from "../state/AppContext";
import { useFlashProgrammer } from "../hooks/useFlashProgrammer";
import { ProgressBar } from "./ProgressBar";

export function FlashControls() {
  const { state } = useAppContext();
  const { flashFirmware, eraseChip, verifyFirmware, resetTarget, cancelOperation } =
    useFlashProgrammer();

  // A full chip erase destroys whatever is on the part, with no undo and no
  // backup taken first. It asks before running; a range erase does not.
  const [confirmingErase, setConfirmingErase] = useState(false);
  const fullChipErase = state.flashOptions.chipErase;

  const handleErase = () => {
    if (fullChipErase && !confirmingErase) {
      setConfirmingErase(true);
      return;
    }
    setConfirmingErase(false);
    void eraseChip();
  };

  const isConnected = state.connectionStatus === "connected";
  const hasFirmware = state.firmware !== null;
  const isBusy =
    state.flashStatus !== "idle" &&
    state.flashStatus !== "completed" &&
    state.flashStatus !== "cancelled" &&
    state.flashStatus !== "error";
  // Reset is a single quick transaction with nothing to poll. The rest depends
  // on the backend: probe-rs runs an erase and a download to completion inside
  // one call, so a Stop during those stages would do nothing, and the target
  // tells us which stages it can actually abort.
  const cancellableStages = state.targetInfo?.cancellable_stages ?? [];
  const isCancellable = cancellableStages.includes(state.flashStatus);
  // Busy in a stage the backend cannot interrupt: say so rather than offering
  // a button that cannot act.
  const isUninterruptible =
    !isCancellable &&
    (state.flashStatus === "erasing" ||
      state.flashStatus === "programming" ||
      state.flashStatus === "verifying");

  const statusLabel = (): string => {
    switch (state.flashStatus) {
      case "erasing":
        return "Erasing...";
      case "programming":
        return "Programming...";
      case "verifying":
        return "Verifying...";
      case "resetting":
        return "Resetting...";
      case "cancelling":
        return "Cancelling...";
      case "cancelled":
        return "Cancelled";
      case "completed":
        return "✓ Completed";
      case "error":
        return "✗ Error";
      default:
        return "Ready";
    }
  };

  return (
    <div className="flex flex-col p-4 space-y-4 border-t border-gray-700">
      {/* Status */}
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider">
          Flash Controls
        </h2>
        <span
          className={`text-xs font-medium px-2 py-0.5 rounded ${
            state.flashStatus === "completed"
              ? "bg-green-900/50 text-green-400"
              : state.flashStatus === "cancelled"
                ? "bg-gray-700 text-gray-300"
              : state.flashStatus === "error"
                ? "bg-red-900/50 text-red-400"
                : isBusy
                  ? "bg-yellow-900/50 text-yellow-400"
                  : "bg-gray-700 text-gray-400"
          }`}
        >
          {statusLabel()}
        </span>
      </div>

      {/* Primary Action — becomes Cancel while an interruptible operation runs */}
      {isCancellable || state.flashStatus === "cancelling" ? (
        <button
          onClick={cancelOperation}
          disabled={state.flashStatus === "cancelling"}
          className="w-full py-3 bg-gray-700 hover:bg-gray-600 text-gray-100 rounded-lg text-base font-bold uppercase tracking-wider transition-colors disabled:opacity-40 disabled:cursor-not-allowed border border-gray-500"
        >
          {state.flashStatus === "cancelling"
            ? "Cancelling..."
            : `✕ Cancel ${statusLabel().replace("...", "")}`}
        </button>
      ) : isUninterruptible ? (
        <button
          disabled
          title="This stage runs to completion inside the probe driver and cannot be interrupted."
          className="w-full py-3 bg-gray-700 text-gray-300 rounded-lg text-base font-bold uppercase tracking-wider border border-gray-600 cursor-not-allowed"
        >
          {statusLabel().replace("...", "")} cannot be interrupted
        </button>
      ) : (
        <button
          onClick={flashFirmware}
          disabled={!isConnected || !hasFirmware || isBusy}
          className="w-full py-3 bg-accent-red hover:bg-red-500 text-white rounded-lg text-base font-bold uppercase tracking-wider transition-colors disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:bg-accent-red shadow-lg shadow-accent-red/20"
        >
          {isBusy ? statusLabel() : "⚡ Program"}
        </button>
      )}

      {confirmingErase && (
        <p className="text-xs text-red-300 bg-red-950/50 border border-red-800 rounded px-2 py-1.5">
          Full chip erase wipes <strong>everything</strong> on the part,
          including firmware already there. Nothing is backed up and it cannot
          be undone. Click Erase again to go ahead, or{" "}
          <button
            type="button"
            onClick={() => setConfirmingErase(false)}
            className="underline hover:text-red-200"
          >
            cancel
          </button>
          .
        </p>
      )}

      {/* Secondary Actions */}
      <div className="grid grid-cols-3 gap-2">
        <button
          onClick={handleErase}
          disabled={!isConnected || isBusy}
          className={`py-2 rounded text-xs font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed border ${
            confirmingErase
              ? "bg-red-700 hover:bg-red-600 text-white border-red-500"
              : "bg-bg-tertiary hover:bg-blue-800 text-gray-200 border-gray-600"
          }`}
          title={
            fullChipErase
              ? "Erases the entire chip, including firmware already on it"
              : "Erases the firmware's address range"
          }
        >
          {confirmingErase ? "Erase everything?" : "🗑 Erase"}
        </button>
        <button
          onClick={verifyFirmware}
          disabled={!isConnected || !hasFirmware || isBusy}
          className="py-2 bg-bg-tertiary hover:bg-blue-800 text-gray-200 rounded text-xs font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed border border-gray-600"
        >
          ✓ Verify
        </button>
        <button
          onClick={() => resetTarget(false)}
          disabled={!isConnected || isBusy}
          className="py-2 bg-bg-tertiary hover:bg-blue-800 text-gray-200 rounded text-xs font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed border border-gray-600"
        >
          ↺ Reset
        </button>
      </div>

      {/* Progress */}
      <ProgressBar
        percentage={state.progress.percentage}
        stage={state.flashStatus}
        bytesTransferred={state.progress.bytesTransferred}
        totalBytes={state.progress.totalBytes}
        speedBps={state.progress.speedBps}
        elapsedMs={state.progress.elapsedMs}
      />
    </div>
  );
}
