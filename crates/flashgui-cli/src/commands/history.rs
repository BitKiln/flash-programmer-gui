//! What has been programmed, most recent first.

use std::io::Write;

use flash_core::history::{self, format_timestamp, HistoryRecord};

use crate::cli::{Cli, HistoryArgs};
use crate::exit_codes::CliError;

/// One line per record: when, what, onto what, and how it went.
fn summarise(record: &HistoryRecord) -> String {
    let mut line = format!(
        "{}  {:7}  {:9}  {}",
        format_timestamp(record.timestamp_ms),
        record.operation.as_str(),
        record.outcome.as_str(),
        record.target
    );
    if let Some(bytes) = record.bytes {
        line.push_str(&format!("  {bytes} bytes"));
    }
    // The CRC is what says which build this was; a path does not, since the
    // same path holds a different image after every rebuild.
    if let Some(crc) = record.image_crc32 {
        line.push_str(&format!("  crc32 0x{crc:08X}"));
    }
    if record.verified {
        line.push_str("  verified");
    }
    if let Some(ref serial) = record.serial {
        line.push_str(&format!("  serial {serial}"));
    }
    if record.duration_ms > 0 {
        line.push_str(&format!("  {} ms", record.duration_ms));
    }
    if !record.message.is_empty() && record.outcome != flash_core::Outcome::Succeeded {
        line.push_str(&format!("  -- {}", record.message));
    }
    line
}

pub fn handle_history(
    cli: &Cli,
    args: &HistoryArgs,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<(), CliError> {
    let path = cli.history_file.as_deref().map(std::path::Path::new);
    // Filters apply before the limit, so `--limit 5 --failures` means the five
    // most recent failures rather than the failures among the five most recent.
    let records = history::read_records(path, None)
        .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;

    let filtered: Vec<_> = records
        .into_iter()
        .filter(|r| {
            args.target
                .as_deref()
                .is_none_or(|t| r.target.eq_ignore_ascii_case(t))
        })
        .filter(|r| !args.failures || r.outcome != flash_core::Outcome::Succeeded)
        .take(args.limit)
        .collect();

    if cli.json {
        for record in &filtered {
            let line = serde_json::to_string(record)
                .map_err(|e| CliError::InvalidArgsOrProfile(e.to_string()))?;
            writeln!(stdout, "{line}").ok();
        }
        return Ok(());
    }

    if filtered.is_empty() {
        writeln!(
            stdout,
            "No programming history yet. Records are written when a real target \
             is flashed, erased or verified; simulated runs are not recorded."
        )
        .ok();
        return Ok(());
    }

    for record in &filtered {
        writeln!(stdout, "{}", summarise(record)).ok();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flash_core::{Operation, Outcome};

    fn record() -> HistoryRecord {
        let mut r = HistoryRecord::now(Operation::Flash, Outcome::Succeeded, "STM32F401RE");
        r.timestamp_ms = 1_789_652_707_000;
        r.bytes = Some(1024);
        r.image_crc32 = Some(0xDEADBEEF);
        r.duration_ms = 350;
        r.verified = true;
        r
    }

    #[test]
    fn a_summary_names_the_build_by_its_checksum() {
        let line = summarise(&record());
        assert!(line.contains("crc32 0xDEADBEEF"), "got {line}");
        assert!(line.contains("2026-09-17"), "got {line}");
        assert!(line.contains("verified"), "got {line}");
    }

    #[test]
    fn a_failure_carries_its_reason_and_a_success_does_not() {
        let mut failed = record();
        failed.outcome = Outcome::Failed;
        failed.message = "target did not answer".to_string();
        assert!(summarise(&failed).contains("-- target did not answer"));

        let mut succeeded = record();
        succeeded.message = "Successfully flashed".to_string();
        assert!(!summarise(&succeeded).contains("--"));
    }
}
