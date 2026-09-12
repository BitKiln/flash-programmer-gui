use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use flash_core::progress::{FlashEvent, FlashStage, ProgressCallback, ProgressMetrics};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum NdJsonMessage<'a> {
    #[serde(rename = "status")]
    Status {
        stage: &'a str,
        progress: f64,
        message: &'a str,
    },
    #[serde(rename = "progress")]
    Progress {
        stage: &'a str,
        bytes_done: u64,
        total_bytes: u64,
        percentage: f64,
        speed_bps: u64,
        elapsed_ms: u64,
    },
    #[serde(rename = "complete")]
    Complete {
        status: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        bytes_flashed: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        bytes_verified: Option<u32>,
        duration_ms: u64,
        message: &'a str,
    },
    #[serde(rename = "serial")]
    Serial { serial: &'a str },
    #[serde(rename = "error")]
    Error {
        code: i32,
        message: &'a str,
    },
}

/// Thread-safe terminal and NDJSON streaming progress callback.
pub struct CliProgressCallback {
    quiet: bool,
    json: bool,
    output_buf: Arc<Mutex<Vec<u8>>>,
    cancelled: AtomicBool,
}

impl CliProgressCallback {
    pub fn new(quiet: bool, json: bool) -> (Self, Arc<Mutex<Vec<u8>>>) {
        let output_buf = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                quiet,
                json,
                output_buf: Arc::clone(&output_buf),
                cancelled: AtomicBool::new(false),
            },
            output_buf,
        )
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn write_line(&self, line: &str) {
        if let Ok(mut w) = self.output_buf.lock() {
            let _ = writeln!(w, "{}", line);
        }
    }

    /// Reports the serial number stamped into the board just programmed.
    pub fn emit_serial(&self, serial: &str) {
        if self.json {
            let msg = NdJsonMessage::Serial { serial };
            if let Ok(json_str) = serde_json::to_string(&msg) {
                self.write_line(&json_str);
            }
        } else if !self.quiet {
            self.write_line(&format!("Serial number programmed: {}", serial));
        }
    }

    pub fn emit_error(&self, code: i32, message: &str) {
        if self.json {
            let msg = NdJsonMessage::Error { code, message };
            if let Ok(json_str) = serde_json::to_string(&msg) {
                self.write_line(&json_str);
            }
        } else if !self.quiet {
            self.write_line(&format!("Error (exit code {}): {}", code, message));
        }
    }

    pub fn emit_complete(
        &self,
        bytes_flashed: Option<u32>,
        bytes_verified: Option<u32>,
        duration_ms: u64,
        message: &str,
    ) {
        if self.json {
            let msg = NdJsonMessage::Complete {
                status: "success",
                bytes_flashed,
                bytes_verified,
                duration_ms,
                message,
            };
            if let Ok(json_str) = serde_json::to_string(&msg) {
                self.write_line(&json_str);
            }
        } else if !self.quiet {
            self.write_line(&format!("✔ {}", message));
        }
    }
}

impl ProgressCallback for CliProgressCallback {
    fn on_event(&self, event: FlashEvent) {
        match event {
            FlashEvent::StageStarted {
                stage,
                total_bytes: _,
                message,
            } => {
                let stage_name = stage_to_str(stage);
                if self.json {
                    let msg = NdJsonMessage::Status {
                        stage: stage_name,
                        progress: 0.0,
                        message: &message,
                    };
                    if let Ok(json_str) = serde_json::to_string(&msg) {
                        self.write_line(&json_str);
                    }
                } else if !self.quiet {
                    self.write_line(&format!("[{}] {}", stage_name.to_uppercase(), message));
                }
            }
            FlashEvent::Progress(ProgressMetrics {
                stage,
                bytes_transferred,
                total_bytes,
                elapsed_ms,
                speed_bps,
                percentage,
                ..
            }) => {
                let stage_name = stage_to_str(stage);
                if self.json {
                    let msg = NdJsonMessage::Progress {
                        stage: stage_name,
                        bytes_done: bytes_transferred,
                        total_bytes,
                        percentage: f64::from(percentage),
                        speed_bps: speed_bps as u64,
                        elapsed_ms,
                    };
                    if let Ok(json_str) = serde_json::to_string(&msg) {
                        self.write_line(&json_str);
                    }
                } else if !self.quiet && total_bytes > 0 {
                    if bytes_transferred == total_bytes || bytes_transferred.is_multiple_of(16384) {
                        self.write_line(&format!(
                            "  ... {} / {} bytes ({:.1}%, {:.1} KB/s)",
                            bytes_transferred,
                            total_bytes,
                            percentage,
                            speed_bps / 1024.0
                        ));
                    }
                }
            }
            FlashEvent::StageCompleted { stage, duration_ms } => {
                let stage_name = stage_to_str(stage);
                if self.json {
                    let msg = NdJsonMessage::Status {
                        stage: stage_name,
                        progress: 100.0,
                        message: "Stage completed",
                    };
                    if let Ok(json_str) = serde_json::to_string(&msg) {
                        self.write_line(&json_str);
                    }
                } else if !self.quiet {
                    self.write_line(&format!(
                        "✔ {} completed in {} ms",
                        stage_name.to_uppercase(),
                        duration_ms
                    ));
                }
            }
            FlashEvent::Log {
                level: _,
                message,
                timestamp_ms: _,
            } => {
                if !self.json && !self.quiet {
                    self.write_line(&format!("  [log] {}", message));
                }
            }
            FlashEvent::Warning { message } => {
                if !self.json && !self.quiet {
                    self.write_line(&format!("  [warn] {}", message));
                }
            }
            FlashEvent::Error { stage, message } => {
                if !self.json && !self.quiet {
                    self.write_line(&format!("  [error: {}] {}", stage_to_str(stage), message));
                }
            }
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

fn stage_to_str(stage: FlashStage) -> &'static str {
    match stage {
        FlashStage::Connecting => "connecting",
        FlashStage::Erasing => "erasing",
        FlashStage::Programming => "programming",
        FlashStage::Verifying => "verifying",
        FlashStage::Resetting => "resetting",
        FlashStage::Completed => "completed",
        FlashStage::Failed => "failed",
        FlashStage::Cancelled => "cancelled",
    }
}
