interface ProgressBarProps {
  percentage: number;
  stage: string;
  bytesTransferred: number;
  totalBytes: number;
  speedBps: number;
  elapsedMs: number;
}

export function ProgressBar({
  percentage,
  stage,
  bytesTransferred,
  totalBytes,
  speedBps,
  elapsedMs,
}: ProgressBarProps) {
  const formatBytes = (bytes: number): string => {
    if (bytes >= 1024 * 1024)
      return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${bytes} B`;
  };

  const formatSpeed = (bps: number): string => {
    if (bps >= 1024 * 1024) return `${(bps / (1024 * 1024)).toFixed(1)} MB/s`;
    if (bps >= 1024) return `${(bps / 1024).toFixed(1)} KB/s`;
    return `${bps.toFixed(0)} B/s`;
  };

  const formatTime = (ms: number): string => {
    if (ms < 1000) return `${ms}ms`;
    const seconds = ms / 1000;
    if (seconds < 60) return `${seconds.toFixed(1)}s`;
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}m ${secs}s`;
  };

  const clampedPercentage = Math.max(0, Math.min(100, percentage));

  const barColor =
    stage === "completed"
      ? "bg-green-500"
      : stage === "error"
        ? "bg-red-500"
        : "bg-accent-red";

  return (
    <div className="space-y-1.5">
      {/* Progress Bar */}
      <div className="w-full h-3 bg-bg-primary rounded-full overflow-hidden border border-gray-700">
        <div
          className={`h-full ${barColor} transition-all duration-300 ease-out`}
          style={{ width: `${clampedPercentage}%` }}
        />
      </div>

      {/* Stats Row */}
      <div className="flex justify-between text-xs text-gray-400 font-mono">
        <span>{clampedPercentage.toFixed(1)}%</span>
        <span>
          {formatBytes(bytesTransferred)}{" "}
          {totalBytes > 0 && `/ ${formatBytes(totalBytes)}`}
        </span>
        <span>{speedBps > 0 ? formatSpeed(speedBps) : "—"}</span>
        <span>{elapsedMs > 0 ? formatTime(elapsedMs) : "0ms"}</span>
      </div>
    </div>
  );
}
