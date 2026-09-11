//! Tier 1 — features.
//!
//! One scenario per advertised CLI capability, exercised against the mock
//! backend through the real binary.

use e2e_harness::{exit, fixture, Cli, MOCK_TARGET};

#[test]
fn devices_lists_mock_probes() {
    let cli = Cli::new();
    let out = cli.mock(&["devices"]);
    out.assert_code(exit::SUCCESS)
        .assert_stdout_contains("ST-LINK/V2 (Mock)");
}

#[test]
fn devices_json_is_machine_readable() {
    let cli = Cli::new();
    let out = cli.mock(&["devices", "--json"]);
    out.assert_code(exit::SUCCESS);

    let probes: serde_json::Value =
        serde_json::from_str(&out.stdout).expect("devices --json must emit a JSON array");
    let probes = probes.as_array().expect("expected a JSON array");
    assert!(
        probes.iter().any(|p| p["identifier"]
            .as_str()
            .is_some_and(|id| id.starts_with("mock:"))),
        "no mock probe in {:?}",
        probes
    );
}

#[test]
fn flash_hex_programs_verifies_and_resets() {
    let cli = Cli::new();
    let hex = fixture("valid_stm32_single_segment.hex");
    let out = cli.mock(&[
        "flash",
        hex.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "--verify",
        "--reset",
    ]);
    out.assert_code(exit::SUCCESS)
        .assert_stdout_contains("Successfully flashed");
}

#[test]
fn flash_bin_requires_and_honours_base_address() {
    let cli = Cli::new();
    let bin = fixture("valid_exact_sector_1kb.bin");
    let out = cli.mock(&[
        "flash",
        bin.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "--base-address",
        "0x08000000",
        "--verify",
    ]);
    out.assert_code(exit::SUCCESS);
}

#[test]
fn flash_elf_uses_load_addresses_from_the_file() {
    let cli = Cli::new();
    let elf = fixture("valid_u575_blinky.elf");
    // No --base-address: the ELF program headers carry the load addresses.
    let out = cli.mock(&[
        "flash",
        elf.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "--verify",
    ]);
    out.assert_code(exit::SUCCESS)
        .assert_stdout_contains("Successfully flashed");
}

#[test]
fn flashed_image_verifies_in_a_later_invocation() {
    let cli = Cli::new();
    let hex = fixture("valid_stm32_single_segment.hex");
    cli.mock(&["flash", hex.to_str().unwrap(), "--target", MOCK_TARGET])
        .assert_code(exit::SUCCESS);

    // Separate process: only real persisted flash content can satisfy this.
    cli.mock(&["verify", hex.to_str().unwrap(), "--target", MOCK_TARGET])
        .assert_code(exit::SUCCESS);
}

#[test]
fn erase_clears_previously_flashed_content() {
    let cli = Cli::new();
    let hex = fixture("valid_stm32_single_segment.hex");
    cli.mock(&["flash", hex.to_str().unwrap(), "--target", MOCK_TARGET])
        .assert_code(exit::SUCCESS);
    cli.mock(&["erase", "--target", MOCK_TARGET, "--full"])
        .assert_code(exit::SUCCESS);

    // Erased flash must no longer match the image.
    cli.mock(&["verify", hex.to_str().unwrap(), "--target", MOCK_TARGET])
        .assert_code(exit::FLASH_VERIFY_ERROR);
}

#[test]
fn reset_succeeds_standalone() {
    let cli = Cli::new();
    cli.mock(&["reset", "--target", MOCK_TARGET])
        .assert_code(exit::SUCCESS);
    cli.mock(&["reset", "--target", MOCK_TARGET, "--halt"])
        .assert_code(exit::SUCCESS);
}

#[test]
fn profile_round_trips_and_drives_a_flash() {
    let cli = Cli::new();
    let hex = fixture("valid_stm32_single_segment.hex");

    cli.mock(&[
        "profile",
        "save",
        "sensor-board",
        "--target",
        MOCK_TARGET,
        "--description",
        "E2E profile",
        "--firmware",
        hex.to_str().unwrap(),
        "--verify",
        "--reset",
    ])
    .assert_code(exit::SUCCESS);

    cli.mock(&["profile", "list"])
        .assert_code(exit::SUCCESS)
        .assert_stdout_contains("sensor-board");
    cli.mock(&["profile", "show", "sensor-board"])
        .assert_code(exit::SUCCESS)
        .assert_stdout_contains(MOCK_TARGET);

    // The profile alone supplies target and firmware path.
    cli.mock(&["flash", "--profile", "sensor-board"])
        .assert_code(exit::SUCCESS);

    cli.mock(&["profile", "delete", "sensor-board"])
        .assert_code(exit::SUCCESS);
    cli.mock(&["profile", "show", "sensor-board"])
        .assert_code(exit::INVALID_ARGS_OR_PROFILE);
}

#[test]
fn flash_json_stream_reports_progress_and_completion() {
    let cli = Cli::new();
    let hex = fixture("valid_stm32_single_segment.hex");
    let out = cli.mock(&[
        "--json",
        "flash",
        hex.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "--verify",
    ]);
    out.assert_code(exit::SUCCESS);

    let progress = out.ndjson_of_type("progress");
    assert!(
        !progress.is_empty(),
        "no progress messages in NDJSON stream"
    );
    assert!(
        progress.iter().all(|m| m["percentage"]
            .as_f64()
            .is_some_and(|p| (0.0..=100.0).contains(&p))),
        "percentage outside 0..=100 in {:?}",
        progress
    );

    let complete = out.ndjson_of_type("complete");
    assert_eq!(complete.len(), 1, "expected exactly one completion message");
    assert_eq!(complete[0]["status"], "success");
    assert!(
        complete[0]["bytes_flashed"].as_u64().is_some_and(|b| b > 0),
        "completion reported no bytes flashed: {:?}",
        complete[0]
    );
}
