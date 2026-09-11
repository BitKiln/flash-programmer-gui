//! Tier 3 — flag and profile combinations.
//!
//! Options are checked by observing the stages the CLI actually runs in its
//! NDJSON stream, not merely by exit code.

use e2e_harness::{exit, fixture, Cli, Output, MOCK_TARGET};

/// Stage names present in the NDJSON `status` stream.
fn stages(out: &Output) -> Vec<String> {
    out.ndjson_of_type("status")
        .iter()
        .filter_map(|m| m["stage"].as_str().map(str::to_string))
        .collect()
}

fn flash_json(cli: &Cli, extra: &[&str]) -> Output {
    let hex = fixture("valid_stm32_single_segment.hex");
    let mut args = vec![
        "--json",
        "flash",
        hex.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
    ];
    args.extend_from_slice(extra);
    cli.mock(&args)
}

#[test]
fn verify_flag_controls_the_verify_stage() {
    let cli = Cli::new();
    let with = flash_json(&cli, &["--verify"]);
    with.assert_code(exit::SUCCESS);
    assert!(
        stages(&with).iter().any(|s| s == "verifying"),
        "--verify did not run a verify stage: {:?}",
        stages(&with)
    );

    let without = flash_json(&cli, &["--no-verify"]);
    without.assert_code(exit::SUCCESS);
    assert!(
        !stages(&without).iter().any(|s| s == "verifying"),
        "--no-verify still ran a verify stage: {:?}",
        stages(&without)
    );
}

#[test]
fn reset_flag_is_reflected_in_the_completion_message() {
    let cli = Cli::new();
    let with = flash_json(&cli, &["--reset"]);
    with.assert_code(exit::SUCCESS);
    let msg = with.ndjson_of_type("complete")[0]["message"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(msg.contains("reset"), "completion message was: {msg}");

    let without = flash_json(&cli, &["--no-reset"]);
    without.assert_code(exit::SUCCESS);
    let msg = without.ndjson_of_type("complete")[0]["message"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(!msg.contains("reset"), "completion message was: {msg}");
}

#[test]
fn full_erase_erases_beyond_the_written_range() {
    let cli = Cli::new();

    // Sector-scoped erase (the default) leaves the rest of flash untouched, so
    // an image written earlier at a different address still verifies.
    let other = fixture("valid_tiny_16b.bin");
    cli.mock(&[
        "flash",
        other.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "-a",
        "0x08040000",
    ])
    .assert_code(exit::SUCCESS);
    flash_json(&cli, &["--no-verify"]).assert_code(exit::SUCCESS);
    cli.mock(&[
        "verify",
        other.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "-a",
        "0x08040000",
    ])
    .assert_code(exit::SUCCESS);

    // --full-erase wipes the whole chip first, so it does not.
    flash_json(&cli, &["--full-erase", "--no-verify"]).assert_code(exit::SUCCESS);
    cli.mock(&[
        "verify",
        other.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "-a",
        "0x08040000",
    ])
    .assert_code(exit::FLASH_VERIFY_ERROR);
}

#[test]
fn command_line_flags_override_profile_options() {
    let cli = Cli::new();
    let hex = fixture("valid_stm32_single_segment.hex");

    cli.mock(&[
        "profile",
        "save",
        "verifying-profile",
        "--target",
        MOCK_TARGET,
        "--firmware",
        hex.to_str().unwrap(),
        "--verify",
        "true",
        "--reset",
        "true",
    ])
    .assert_code(exit::SUCCESS);

    let from_profile = cli.mock(&["--json", "flash", "--profile", "verifying-profile"]);
    from_profile.assert_code(exit::SUCCESS);
    assert!(
        stages(&from_profile).iter().any(|s| s == "verifying"),
        "profile's verify_after was ignored: {:?}",
        stages(&from_profile)
    );

    let overridden = cli.mock(&[
        "--json",
        "flash",
        "--profile",
        "verifying-profile",
        "--no-verify",
    ]);
    overridden.assert_code(exit::SUCCESS);
    assert!(
        !stages(&overridden).iter().any(|s| s == "verifying"),
        "--no-verify did not override the profile: {:?}",
        stages(&overridden)
    );
}

#[test]
fn profile_supplies_target_and_firmware_while_flags_supply_the_rest() {
    let cli = Cli::new();
    let hex = fixture("valid_stm32_single_segment.hex");
    cli.mock(&[
        "profile",
        "save",
        "board",
        "--target",
        MOCK_TARGET,
        "--firmware",
        hex.to_str().unwrap(),
        "--speed",
        "1000",
        "--interface",
        "swd",
    ])
    .assert_code(exit::SUCCESS);

    cli.mock(&["flash", "--profile", "board", "--speed", "4000", "--verify"])
        .assert_code(exit::SUCCESS);
}

#[test]
fn quiet_suppresses_human_output_but_not_the_exit_code() {
    let cli = Cli::new();
    let hex = fixture("valid_stm32_single_segment.hex");
    let out = cli.mock(&[
        "--quiet",
        "flash",
        hex.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "--verify",
    ]);
    out.assert_code(exit::SUCCESS);
    assert!(
        !out.stdout.contains("Programming"),
        "--quiet still printed progress: {}",
        out.stdout
    );
}

#[test]
fn range_erase_only_clears_the_requested_range() {
    let cli = Cli::new();
    let tiny = fixture("valid_tiny_16b.bin");

    cli.mock(&[
        "flash",
        tiny.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "-a",
        "0x08000000",
    ])
    .assert_code(exit::SUCCESS);
    cli.mock(&[
        "flash",
        tiny.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "-a",
        "0x08010000",
    ])
    .assert_code(exit::SUCCESS);

    cli.mock(&[
        "erase",
        "--target",
        MOCK_TARGET,
        "--address",
        "0x08000000",
        "--length",
        "4096",
    ])
    .assert_code(exit::SUCCESS);

    cli.mock(&[
        "verify",
        tiny.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "-a",
        "0x08000000",
    ])
    .assert_code(exit::FLASH_VERIFY_ERROR);
    cli.mock(&[
        "verify",
        tiny.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "-a",
        "0x08010000",
    ])
    .assert_code(exit::SUCCESS);
}
