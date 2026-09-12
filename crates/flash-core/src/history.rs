//! Programming history: what was written to which board, and how it went.
//!
//! Lives here rather than in either front end so the desktop application and
//! the CLI append to the same file. A board programmed from a script and one
//! programmed by hand belong in one record of what happened.
//!
//! The file is newline-delimited JSON, appended to and never rewritten. That
//! survives two processes writing at once better than a single document would,
//! and a record damaged by a crash costs one line rather than the whole
//! history. Reading tolerates a line it cannot parse for the same reason.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::BaseDirs;
use serde::{Deserialize, Serialize};

/// How many records a history file keeps before the oldest are dropped.
///
/// A production run is thousands of boards, so this is not a log of everything
/// ever done -- it is the recent past, which is what anyone asks about.
pub const MAX_RECORDS: usize = 2_000;

/// What kind of operation a record describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Flash,
    Erase,
    Verify,
}

impl Operation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Operation::Flash => "flash",
            Operation::Erase => "erase",
            Operation::Verify => "verify",
        }
    }
}

/// How an operation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Succeeded,
    Failed,
    Cancelled,
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Succeeded => "succeeded",
            Outcome::Failed => "failed",
            Outcome::Cancelled => "cancelled",
        }
    }
}

/// One thing that was done to one board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryRecord {
    /// Milliseconds since the Unix epoch, so the record is orderable without
    /// depending on how any front end formats a date.
    pub timestamp_ms: u64,
    pub operation: Operation,
    pub outcome: Outcome,
    pub target: String,
    /// Probe or port identifier, including its scheme (`esp:COM7`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// CRC32 of the image as the parser computed it.
    ///
    /// This is what makes the history worth keeping: it identifies *which*
    /// build went onto a board, which a file name does not -- the same path
    /// holds a different image after every rebuild.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_crc32: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    pub duration_ms: u64,
    /// Serial number stamped into this board, when one was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// Whether the image was verified against the target afterwards. A
    /// successful flash that was not verified is a weaker claim than one that
    /// was, and the record has to keep them apart.
    #[serde(default)]
    pub verified: bool,
    /// The message the operation ended with, including the failure reason.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

impl HistoryRecord {
    /// A record stamped with the current time.
    pub fn now(operation: Operation, outcome: Outcome, target: impl Into<String>) -> Self {
        Self {
            timestamp_ms: epoch_millis(),
            operation,
            outcome,
            target: target.into(),
            probe: None,
            file_path: None,
            image_crc32: None,
            bytes: None,
            duration_ms: 0,
            serial: None,
            verified: false,
            message: String::new(),
        }
    }
}

fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Failure while reading or writing the history file.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("Could not locate a configuration directory for the history file")]
    NoConfigDir,

    #[error("{0}")]
    Io(String),
}

/// Default path of the history file, alongside the profiles directory.
pub fn default_history_path() -> Result<PathBuf, HistoryError> {
    let dirs = BaseDirs::new().ok_or(HistoryError::NoConfigDir)?;
    Ok(dirs.config_dir().join("flashgui").join("history.jsonl"))
}

/// Appends one record, creating the file and its directory if needed.
///
/// A history that cannot be written must not fail the programming that was
/// just done, so callers are expected to report this and carry on.
pub fn append_record(record: &HistoryRecord, path: Option<&Path>) -> Result<(), HistoryError> {
    let path = match path {
        Some(p) => p.to_path_buf(),
        None => default_history_path()?,
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| HistoryError::Io(e.to_string()))?;
    }

    let line = serde_json::to_string(record).map_err(|e| HistoryError::Io(e.to_string()))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| HistoryError::Io(e.to_string()))?;
    writeln!(file, "{line}").map_err(|e| HistoryError::Io(e.to_string()))?;

    Ok(())
}

/// Reads records, newest first.
///
/// A line that will not parse is skipped rather than failing the read: a
/// truncated last line from an interrupted write must not hide the thousand
/// records in front of it.
pub fn read_records(
    path: Option<&Path>,
    limit: Option<usize>,
) -> Result<Vec<HistoryRecord>, HistoryError> {
    let path = match path {
        Some(p) => p.to_path_buf(),
        None => default_history_path()?,
    };
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(&path).map_err(|e| HistoryError::Io(e.to_string()))?;
    let mut records: Vec<HistoryRecord> = BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<HistoryRecord>(&line).ok())
        .collect();

    records.reverse();
    if let Some(limit) = limit {
        records.truncate(limit);
    }
    Ok(records)
}

/// Rewrites the file with only its most recent [`MAX_RECORDS`] entries.
///
/// Called after an append rather than on a schedule, so a long-running
/// production session cannot grow the file without bound.
pub fn trim_to_limit(path: Option<&Path>) -> Result<usize, HistoryError> {
    let path = match path {
        Some(p) => p.to_path_buf(),
        None => default_history_path()?,
    };
    if !path.exists() {
        return Ok(0);
    }

    // read_records hands them back newest first; the file is oldest first.
    let mut records = read_records(Some(&path), None)?;
    if records.len() <= MAX_RECORDS {
        return Ok(0);
    }
    let dropped = records.len() - MAX_RECORDS;
    records.truncate(MAX_RECORDS);
    records.reverse();

    let mut body = String::new();
    for record in &records {
        let line = serde_json::to_string(record).map_err(|e| HistoryError::Io(e.to_string()))?;
        body.push_str(&line);
        body.push('\n');
    }
    fs::write(&path, body).map_err(|e| HistoryError::Io(e.to_string()))?;
    Ok(dropped)
}

/// Appends a record and trims the file, reporting nothing on failure.
///
/// For call sites that have just finished programming a board: the record is
/// worth keeping but not worth failing the operation over.
pub fn record_quietly(record: &HistoryRecord, path: Option<&Path>) {
    if append_record(record, path).is_ok() {
        let _ = trim_to_limit(path);
    }
}

/// Formats epoch milliseconds as `YYYY-MM-DD HH:MM:SS` in UTC.
///
/// UTC rather than local time: a history compared between two machines, or
/// read after a daylight-saving change, has to mean one thing.
pub fn format_timestamp(timestamp_ms: u64) -> String {
    let total_seconds = timestamp_ms / 1_000;
    let seconds_of_day = total_seconds % 86_400;
    let days = (total_seconds / 86_400) as i64;

    // Days since 1970-01-01 to a civil date, by Howard Hinnant's algorithm.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], March-based
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y,
        m,
        d,
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
        seconds_of_day % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("flashgui_history_test_{name}.jsonl"))
    }

    fn record(target: &str) -> HistoryRecord {
        HistoryRecord::now(Operation::Flash, Outcome::Succeeded, target)
    }

    #[test]
    fn records_come_back_newest_first() {
        let path = temp_path("order");
        let _ = fs::remove_file(&path);

        for target in ["first", "second", "third"] {
            append_record(&record(target), Some(&path)).unwrap();
        }

        let records = read_records(Some(&path), None).unwrap();
        let targets: Vec<_> = records.iter().map(|r| r.target.as_str()).collect();
        assert_eq!(targets, vec!["third", "second", "first"]);
    }

    #[test]
    fn a_limit_takes_the_newest_rather_than_the_first_written() {
        let path = temp_path("limit");
        let _ = fs::remove_file(&path);
        for target in ["a", "b", "c"] {
            append_record(&record(target), Some(&path)).unwrap();
        }

        let records = read_records(Some(&path), Some(2)).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].target, "c");
        assert_eq!(records[1].target, "b");
    }

    #[test]
    fn a_damaged_line_costs_only_itself() {
        let path = temp_path("damaged");
        let _ = fs::remove_file(&path);
        append_record(&record("good"), Some(&path)).unwrap();

        // A write interrupted part way through leaves exactly this.
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{\"timestamp_ms\":17").unwrap();
        drop(file);
        append_record(&record("later"), Some(&path)).unwrap();

        let records = read_records(Some(&path), None).unwrap();
        let targets: Vec<_> = records.iter().map(|r| r.target.as_str()).collect();
        assert_eq!(targets, vec!["later", "good"]);
    }

    #[test]
    fn missing_history_is_an_empty_list_not_an_error() {
        let path = temp_path("absent");
        let _ = fs::remove_file(&path);
        assert!(read_records(Some(&path), None).unwrap().is_empty());
    }

    #[test]
    fn trimming_keeps_the_newest_records() {
        let path = temp_path("trim");
        let _ = fs::remove_file(&path);

        // Write past the cap by hand rather than through the cap-enforcing
        // helper, so the trim has something to do.
        let mut body = String::new();
        for i in 0..(MAX_RECORDS + 5) {
            let mut r = record(&format!("board{i}"));
            r.timestamp_ms = 1_000 + i as u64;
            body.push_str(&serde_json::to_string(&r).unwrap());
            body.push('\n');
        }
        fs::write(&path, body).unwrap();

        let dropped = trim_to_limit(Some(&path)).unwrap();
        assert_eq!(dropped, 5);

        let records = read_records(Some(&path), None).unwrap();
        assert_eq!(records.len(), MAX_RECORDS);
        assert_eq!(records[0].target, format!("board{}", MAX_RECORDS + 4));
        // The oldest five are the ones gone.
        assert_eq!(records[MAX_RECORDS - 1].target, "board5");
    }

    #[test]
    fn trimming_a_short_history_changes_nothing() {
        let path = temp_path("short");
        let _ = fs::remove_file(&path);
        append_record(&record("only"), Some(&path)).unwrap();
        assert_eq!(trim_to_limit(Some(&path)).unwrap(), 0);
        assert_eq!(read_records(Some(&path), None).unwrap().len(), 1);
    }

    #[test]
    fn a_timestamp_formats_as_utc() {
        // 2026-09-17T13:45:07Z
        assert_eq!(format_timestamp(1_789_652_707_000), "2026-09-17 13:45:07");
        assert_eq!(format_timestamp(0), "1970-01-01 00:00:00");
        // A leap day, which is where a home-made date conversion goes wrong.
        assert_eq!(format_timestamp(1_709_208_000_000), "2024-02-29 12:00:00");
    }

    #[test]
    fn an_unverified_success_is_distinguishable_from_a_verified_one() {
        let path = temp_path("verified");
        let _ = fs::remove_file(&path);
        let mut r = record("STM32F401RE");
        r.verified = true;
        append_record(&r, Some(&path)).unwrap();
        append_record(&record("STM32F401RE"), Some(&path)).unwrap();

        let records = read_records(Some(&path), None).unwrap();
        assert!(!records[0].verified);
        assert!(records[1].verified);
    }
}
