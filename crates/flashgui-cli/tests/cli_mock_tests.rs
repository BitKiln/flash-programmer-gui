use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, MutexGuard};

use assert_cmd::prelude::*;
use flashgui_cli::exit_codes::{
    EXIT_FIRMWARE_PARSE_ERROR, EXIT_FLASH_VERIFY_ERROR, EXIT_INVALID_ARGS_OR_PROFILE,
    EXIT_PROBE_NOT_FOUND, EXIT_SUCCESS, EXIT_TARGET_CONNECTION_ERROR,
};
use flashgui_cli::run_cli_with_io;
use predicates::prelude::*;
use tempfile::NamedTempFile;

/// Global mutex to serialize tests that share mock flash persistence files.
/// Each test that touches mock flash state should call `lock_mock_flash()` at
/// the beginning. The returned guard cleans the backing file and holds the lock
/// for the test's duration, preventing parallel contamination.
static MOCK_FLASH_LOCK: Mutex<()> = Mutex::new(());

fn lock_mock_flash() -> MutexGuard<'static, ()> {
    let guard = MOCK_FLASH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Clean stale state so every test starts from a fresh mock flash
    let backing = std::env::temp_dir().join("flashgui_mock_flash_stm32f401re_default.bin");
    let _ = std::fs::remove_file(&backing);
    guard
}

fn root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn fixture_path(relative: &str) -> PathBuf {
    root_dir().join(relative)
}

fn run_cli_capture(args: &[&str]) -> (i32, String, String) {
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let full_args: Vec<&str> = std::iter::once("flashgui-cli")
        .chain(args.iter().copied())
        .collect();
    let code = run_cli_with_io(full_args, &mut stdout_buf, &mut stderr_buf);
    let stdout = String::from_utf8_lossy(&stdout_buf).to_string();
    let stderr = String::from_utf8_lossy(&stderr_buf).to_string();
    (code, stdout, stderr)
}

#[test]
fn test_devices_text_output() {
    let (code, stdout, stderr) = run_cli_capture(&["--mock", "devices"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr: {}", stderr);
    assert!(
        stdout.contains("Connected Debug Probes"),
        "stdout was: {}",
        stdout
    );
    assert!(stdout.contains("ST-LINK/V2 (Mock)"));
    assert!(stdout.contains("CMSIS-DAP v2 (Mock)"));
}

#[test]
fn test_devices_json_output() {
    let (code, stdout, stderr) = run_cli_capture(&["--mock", "devices", "--json"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr: {}", stderr);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Failed to parse devices JSON");
    let arr = parsed.as_array().expect("Expected JSON array");
    assert!(!arr.is_empty(), "Probes array should not be empty");
    assert!(
        arr.iter().any(|p| p["identifier"]
            .as_str()
            .unwrap_or("")
            .contains("mock:stlink")),
        "Should contain mock ST-Link"
    );
}

#[test]
fn test_devices_without_mock_empty() {
    let (code, stdout, _) = run_cli_capture(&["devices"]);
    assert_eq!(code, EXIT_SUCCESS);
    assert!(
        stdout.contains("No debug probes detected") || stdout.contains("Connected Debug Probes"),
        "Output was: {}",
        stdout
    );
}

#[test]
fn test_flash_hex_mock_success() {
    let _guard = lock_mock_flash();
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let hex_str = hex_file.to_str().unwrap();

    let (code, stdout, stderr) = run_cli_capture(&[
        "--mock",
        "flash",
        hex_str,
        "--target",
        "STM32F401RE",
        "--verify",
        "--reset",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr: {}", stderr);
    assert!(
        stdout.contains("Successfully flashed"),
        "stdout was: {}",
        stdout
    );
}

#[test]
fn test_flash_bin_mock_success() {
    let _guard = lock_mock_flash();
    let mut temp_bin = NamedTempFile::new().unwrap();
    // 512 bytes with realistic reset handler
    let mut data = vec![0xFF; 512];
    data[0] = 0x00;
    data[1] = 0x50;
    data[2] = 0x00;
    data[3] = 0x20; // SP = 0x20005000
    data[4] = 0xC9;
    data[5] = 0x01;
    data[6] = 0x00;
    data[7] = 0x08; // Reset = 0x080001C9
    temp_bin.write_all(&data).unwrap();
    let bin_path = temp_bin.path().to_str().unwrap();

    let (code, stdout, stderr) = run_cli_capture(&[
        "--mock",
        "flash",
        bin_path,
        "--target",
        "STM32F401RE",
        "-a",
        "0x08000000",
        "--verify",
        "--reset",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr: {}", stderr);
    assert!(
        stdout.contains("Successfully flashed"),
        "stdout was: {}",
        stdout
    );
}

#[test]
fn test_flash_json_streaming() {
    let _guard = lock_mock_flash();
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let hex_str = hex_file.to_str().unwrap();

    let (code, stdout, stderr) = run_cli_capture(&[
        "--mock",
        "--json",
        "flash",
        hex_str,
        "--target",
        "STM32F401RE",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr: {}", stderr);

    let mut had_status = false;
    let mut had_complete = false;

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("Line '{}' is not valid JSON: {}", line, e));
        let typ = parsed["type"].as_str().unwrap_or("");
        if typ == "status" || typ == "progress" {
            had_status = true;
        }
        if typ == "complete" {
            had_complete = true;
            assert_eq!(parsed["status"], "success");
        }
    }

    assert!(had_status, "Should emit at least one status/progress event");
    assert!(had_complete, "Should emit complete event");
}

#[test]
fn test_erase_full_and_sector() {
    let _guard = lock_mock_flash();
    let (code, stdout, stderr) = run_cli_capture(&[
        "--mock",
        "erase",
        "--target",
        "STM32F401RE",
        "--full",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr: {}", stderr);
    assert!(
        stdout.contains("Full chip erase completed successfully"),
        "stdout was: {}",
        stdout
    );

    let (code2, stdout2, stderr2) = run_cli_capture(&[
        "--mock",
        "erase",
        "--target",
        "STM32F401RE",
        "--address",
        "0x08000000",
        "--length",
        "2048",
    ]);
    assert_eq!(code2, EXIT_SUCCESS, "stderr: {}", stderr2);
    assert!(stdout2.contains("completed successfully"));
}

#[test]
fn test_verify_command() {
    let _guard = lock_mock_flash();
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let hex_str = hex_file.to_str().unwrap();

    // 1. Flash the firmware first so flash contains matching bytes
    let (c1, _, err1) = run_cli_capture(&["--mock", "flash", hex_str, "--target", "STM32F401RE"]);
    assert_eq!(c1, EXIT_SUCCESS, "flash error: {}", err1);

    // 2. Run standalone verify
    let (code, stdout, stderr) = run_cli_capture(&[
        "--mock",
        "verify",
        hex_str,
        "--target",
        "STM32F401RE",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr: {}", stderr);
    assert!(
        stdout.contains("Memory verified successfully"),
        "stdout was: {}",
        stdout
    );
}

#[test]
fn test_reset_command() {
    let (code, stdout, stderr) = run_cli_capture(&[
        "--mock",
        "reset",
        "--target",
        "STM32F401RE",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr: {}", stderr);
    assert!(stdout.contains("reset successfully"), "stdout: {}", stdout);

    let (code_halt, stdout_halt, _) = run_cli_capture(&[
        "--mock",
        "reset",
        "--target",
        "STM32F401RE",
        "--halt",
    ]);
    assert_eq!(code_halt, EXIT_SUCCESS);
    assert!(stdout_halt.contains("halt: true"));
}

#[test]
fn test_profile_lifecycle_and_flash() {
    let temp_prof = NamedTempFile::new().unwrap();
    let prof_file_str = temp_prof.path().to_str().unwrap();
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let hex_str = hex_file.to_str().unwrap();

    // 1. Save profile to custom file
    let (c_save, out_save, err_save) = run_cli_capture(&[
        "--profile-file",
        prof_file_str,
        "profile",
        "save",
        "test_nucleo_f401",
        "--target",
        "STM32F401RE",
        "--speed",
        "4000",
        "--firmware",
        hex_str,
        "--verify",
        "--reset",
    ]);
    assert_eq!(c_save, EXIT_SUCCESS, "err: {}", err_save);
    assert!(out_save.contains("saved successfully"));

    // 2. Show profile
    let (c_show, out_show, err_show) = run_cli_capture(&[
        "--profile-file",
        prof_file_str,
        "profile",
        "show",
        "test_nucleo_f401",
    ]);
    assert_eq!(c_show, EXIT_SUCCESS, "err: {}", err_show);
    assert!(out_show.contains("STM32F401RE"));
    assert!(out_show.contains("4000"));

    // 3. List profiles
    let (c_list, out_list, _) = run_cli_capture(&[
        "--profile-file",
        prof_file_str,
        "profile",
        "list",
    ]);
    assert_eq!(c_list, EXIT_SUCCESS);
    assert!(out_list.contains("test_nucleo_f401"));

    // 4. Flash using named profile
    let (c_flash, out_flash, err_flash) = run_cli_capture(&[
        "--mock",
        "--profile-file",
        prof_file_str,
        "flash",
        "--profile",
        "test_nucleo_f401",
    ]);
    assert_eq!(c_flash, EXIT_SUCCESS, "err: {}", err_flash);
    assert!(out_flash.contains("Successfully flashed"), "out: {}", out_flash);

    // 5. Delete profile
    let (c_del, out_del, _) = run_cli_capture(&[
        "--profile-file",
        prof_file_str,
        "profile",
        "delete",
        "test_nucleo_f401",
    ]);
    assert_eq!(c_del, EXIT_SUCCESS);
    assert!(out_del.contains("deleted successfully"));
}

#[test]
fn test_exit_code_1_verify_failure() {
    let _guard = lock_mock_flash();
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let hex_str = hex_file.to_str().unwrap();

    // Erase mock memory first so it is all 0xFF
    let (c_erase, _, _) = run_cli_capture(&["--mock", "erase", "--target", "STM32F401RE", "--full"]);
    assert_eq!(c_erase, EXIT_SUCCESS);

    // Standalone verify against erased mock flash fails verification
    let (code, _, stderr) = run_cli_capture(&[
        "--mock",
        "verify",
        hex_str,
        "--target",
        "STM32F401RE",
    ]);
    assert_eq!(
        code, EXIT_FLASH_VERIFY_ERROR,
        "Expected exit code 1 on mismatch, got {}. stderr: {}",
        code, stderr
    );
}

#[test]
fn test_exit_code_2_target_connection_error() {
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let hex_str = hex_file.to_str().unwrap();

    let (code, _, stderr) = run_cli_capture(&[
        "--mock",
        "flash",
        hex_str,
        "--target",
        "UnrecognizedTargetChip999",
    ]);
    assert_eq!(
        code, EXIT_TARGET_CONNECTION_ERROR,
        "Expected exit code 2, got {}. stderr: {}",
        code, stderr
    );
}

#[test]
fn test_exit_code_3_firmware_parse_error() {
    let corrupt_hex = fixture_path("tests/fixtures/corrupt_bad_checksum.hex");
    let corrupt_str = corrupt_hex.to_str().unwrap();

    let (code, _, stderr) = run_cli_capture(&[
        "--mock",
        "flash",
        corrupt_str,
        "--target",
        "STM32F401RE",
    ]);
    assert_eq!(
        code, EXIT_FIRMWARE_PARSE_ERROR,
        "Expected exit code 3, got {}. stderr: {}",
        code, stderr
    );
}

#[test]
fn test_exit_code_4_probe_not_found() {
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let hex_str = hex_file.to_str().unwrap();

    let (code, _, stderr) = run_cli_capture(&[
        "--mock",
        "flash",
        hex_str,
        "--probe",
        "non_existent_probe_id_9999",
    ]);
    assert_eq!(
        code, EXIT_PROBE_NOT_FOUND,
        "Expected exit code 4, got {}. stderr: {}",
        code, stderr
    );
}

#[test]
fn test_exit_code_5_invalid_arguments() {
    // Missing file without profile
    let (code1, _, _) = run_cli_capture(&["--mock", "flash"]);
    assert_eq!(code1, EXIT_INVALID_ARGS_OR_PROFILE);

    // Invalid base address format
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let hex_str = hex_file.to_str().unwrap();
    let (code2, _, _) = run_cli_capture(&[
        "--mock",
        "flash",
        hex_str,
        "-a",
        "0xNOT_A_VALID_HEX",
    ]);
    assert_eq!(code2, EXIT_INVALID_ARGS_OR_PROFILE);

    // Non-existent profile
    let (code3, _, _) = run_cli_capture(&[
        "profile",
        "show",
        "totally_bogus_profile_name",
    ]);
    assert_eq!(code3, EXIT_INVALID_ARGS_OR_PROFILE);
}

#[test]
fn test_binary_execution_with_assert_cmd() {
    let mut cmd = Command::cargo_bin("flashgui-cli").unwrap();
    cmd.args(&["--mock", "devices", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mock:stlink"));

    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let hex_str = hex_file.to_str().unwrap();
    let mut cmd_flash = Command::cargo_bin("flashgui-cli").unwrap();
    cmd_flash
        .args(&[
            "--mock",
            "flash",
            hex_str,
            "--target",
            "STM32F401RE",
            "--verify",
            "--reset",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Successfully flashed"));

    let corrupt_hex = fixture_path("tests/fixtures/corrupt_bad_checksum.hex");
    let corrupt_str = corrupt_hex.to_str().unwrap();
    let mut cmd_err = Command::cargo_bin("flashgui-cli").unwrap();
    cmd_err
        .args(&["--mock", "flash", corrupt_str])
        .assert()
        .code(3);
}

#[test]
fn test_batch_programs_requested_number_of_units() {
    let _guard = lock_mock_flash();
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let (code, stdout, stderr) = run_cli_capture(&[
        "--mock",
        "batch",
        hex_file.to_str().unwrap(),
        "--target",
        "STM32F401RE",
        "--count",
        "3",
        "--rearm",
        "immediate",
    ]);

    assert_eq!(code, EXIT_SUCCESS, "stderr: {}", stderr);
    assert_eq!(
        stdout.matches("PASS").count(),
        3,
        "expected three passing units, stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("3 unit(s): 3 passed, 0 failed"),
        "missing batch summary, stdout: {}",
        stdout
    );
}

#[test]
fn test_batch_writes_csv_log() {
    let _guard = lock_mock_flash();
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let log = NamedTempFile::new().expect("temp log");
    let log_path = log.path().to_path_buf();

    let (code, _stdout, stderr) = run_cli_capture(&[
        "--mock",
        "batch",
        hex_file.to_str().unwrap(),
        "--target",
        "STM32F401RE",
        "--count",
        "2",
        "--rearm",
        "immediate",
        "--log",
        log_path.to_str().unwrap(),
    ]);

    assert_eq!(code, EXIT_SUCCESS, "stderr: {}", stderr);
    let csv = std::fs::read_to_string(&log_path).expect("log written");
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 3, "header plus two units, got: {}", csv);
    assert!(lines[0].starts_with("index,status,serial,probe_serial,target"));
    assert!(lines[1].starts_with("1,passed,"));
    assert!(lines[2].starts_with("2,passed,"));
}

#[test]
fn test_batch_json_stream_and_unsupported_target() {
    let _guard = lock_mock_flash();
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let (code, stdout, _stderr) = run_cli_capture(&[
        "--mock",
        "--json",
        "batch",
        hex_file.to_str().unwrap(),
        "--target",
        "STM32F401RE",
        "--count",
        "1",
        "--rearm",
        "immediate",
    ]);
    assert_eq!(code, EXIT_SUCCESS);
    assert!(
        stdout.contains("\"type\":\"batch_unit_finished\""),
        "stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("\"type\":\"batch_complete\""),
        "stdout: {}",
        stdout
    );

    // An unsupported target is rejected before any board is programmed.
    let (code, _stdout, _stderr) = run_cli_capture(&[
        "--mock",
        "batch",
        hex_file.to_str().unwrap(),
        "--target",
        "NOT_A_REAL_MCU",
        "--count",
        "1",
        "--rearm",
        "immediate",
    ]);
    assert_eq!(code, EXIT_TARGET_CONNECTION_ERROR);
}

#[test]
fn test_flash_stamps_a_serial_number() {
    let _guard = lock_mock_flash();
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let (code, stdout, stderr) = run_cli_capture(&[
        "--mock",
        "flash",
        hex_file.to_str().unwrap(),
        "--target",
        "STM32F401RE",
        "--serial-address",
        "0x08010000",
        "--serial-format",
        "SN-{n:06}",
        "--serial-start",
        "7",
    ]);

    assert_eq!(code, EXIT_SUCCESS, "stderr: {}", stderr);
    assert!(
        stdout.contains("Serial number programmed: SN-000007"),
        "stdout: {}",
        stdout
    );
}

#[test]
fn test_batch_stamps_a_serial_per_board_and_logs_it() {
    let _guard = lock_mock_flash();
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let log = NamedTempFile::new().expect("temp log");
    let log_path = log.path().to_path_buf();

    let (code, stdout, stderr) = run_cli_capture(&[
        "--mock",
        "batch",
        hex_file.to_str().unwrap(),
        "--target",
        "STM32F401RE",
        "--count",
        "3",
        "--rearm",
        "immediate",
        "--serial-address",
        "0x08010000",
        "--serial-format",
        "ACME-{n:04}",
        "--serial-start",
        "10",
        "--serial-step",
        "5",
        "--log",
        log_path.to_str().unwrap(),
    ]);

    assert_eq!(code, EXIT_SUCCESS, "stderr: {}", stderr);
    for serial in ["ACME-0010", "ACME-0015", "ACME-0020"] {
        assert!(stdout.contains(serial), "missing {} in: {}", serial, stdout);
    }

    let csv = std::fs::read_to_string(&log_path).expect("log written");
    assert!(csv.lines().next().unwrap().starts_with("index,status,serial,"));
    assert!(csv.contains(",ACME-0015,"), "log was: {}", csv);
}

#[test]
fn test_serial_outside_flash_is_rejected() {
    let _guard = lock_mock_flash();
    let hex_file = fixture_path("tests/fixtures/valid_stm32_single_segment.hex");
    let (code, _stdout, _stderr) = run_cli_capture(&[
        "--mock",
        "flash",
        hex_file.to_str().unwrap(),
        "--target",
        "STM32F401RE",
        "--serial-address",
        "0x20000000",
    ]);

    // An address outside the target's flash is a firmware/address error, the
    // same class the parser reports for an out-of-bounds image.
    assert_eq!(code, EXIT_FIRMWARE_PARSE_ERROR);
}
