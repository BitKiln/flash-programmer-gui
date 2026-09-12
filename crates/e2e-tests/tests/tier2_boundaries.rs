//! Tier 2 — boundaries.
//!
//! Malformed, empty, and out-of-range inputs, each pinned to the exit code the
//! CLI contract promises for it.

use e2e_harness::{exit, fixture, Cli, MOCK_TARGET};

/// Flashes a fixture against the mock target and returns the exit code.
fn flash_code(cli: &Cli, fixture_name: &str, extra: &[&str]) -> i32 {
    let path = fixture(fixture_name);
    let mut args = vec!["flash", path.to_str().unwrap(), "--target", MOCK_TARGET];
    args.extend_from_slice(extra);
    cli.mock(&args).code
}

#[test]
fn malformed_hex_records_are_parse_errors() {
    let cli = Cli::new();
    for name in [
        "corrupt_bad_checksum.hex",
        "corrupt_truncated.hex",
        "corrupt_invalid_hex_char.hex",
        "corrupt_missing_colon.hex",
        "corrupt_odd_hex_digits.hex",
        "corrupt_conflicting_overlap.hex",
    ] {
        assert_eq!(
            flash_code(&cli, name, &[]),
            exit::FIRMWARE_PARSE_ERROR,
            "{name} should be rejected as a parse error"
        );
    }
}

#[test]
fn empty_images_are_rejected() {
    let cli = Cli::new();
    assert_eq!(
        flash_code(&cli, "empty_file.hex", &[]),
        exit::FIRMWARE_PARSE_ERROR
    );
    assert_eq!(
        flash_code(&cli, "empty_file.bin", &["-a", "0x08000000"]),
        exit::FIRMWARE_PARSE_ERROR
    );
}

#[test]
fn addresses_outside_target_flash_are_rejected() {
    let cli = Cli::new();
    // 4 GB address, a 64 KB address on a target based at 0x08000000, and a
    // 16 KB image starting 4 KB before the end of a 512 KB flash.
    assert_eq!(
        flash_code(&cli, "extreme_address_overflow_4gb.hex", &[]),
        exit::FIRMWARE_PARSE_ERROR
    );
    assert_eq!(
        flash_code(&cli, "valid_extended_segment_type02.hex", &[]),
        exit::FIRMWARE_PARSE_ERROR
    );
    assert_eq!(
        flash_code(&cli, "valid_multi_sector_16kb.bin", &["-a", "0x0807F000"]),
        exit::FIRMWARE_PARSE_ERROR
    );
}

#[test]
fn missing_firmware_file_is_a_parse_error() {
    let cli = Cli::new();
    let missing = fixture("valid_tiny_16b.bin").with_file_name("definitely_not_here.hex");
    let out = cli.mock(&["flash", missing.to_str().unwrap(), "--target", MOCK_TARGET]);
    out.assert_code(exit::FIRMWARE_PARSE_ERROR);
}

#[test]
fn unknown_target_is_a_connection_error() {
    let cli = Cli::new();
    let hex = fixture("valid_stm32_single_segment.hex");
    cli.mock(&["flash", hex.to_str().unwrap(), "--target", "NRF52840"])
        .assert_code(exit::TARGET_CONNECTION_ERROR);
}

#[test]
fn unknown_probe_is_reported_as_probe_not_found() {
    let cli = Cli::new();
    let hex = fixture("valid_stm32_single_segment.hex");
    cli.mock(&[
        "flash",
        hex.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "--probe",
        "bogus-probe-123",
    ])
    .assert_code(exit::PROBE_NOT_FOUND);
}

#[test]
fn verify_against_erased_flash_fails_with_the_verify_code() {
    let cli = Cli::new();
    let hex = fixture("valid_stm32_single_segment.hex");
    cli.mock(&["verify", hex.to_str().unwrap(), "--target", MOCK_TARGET])
        .assert_code(exit::FLASH_VERIFY_ERROR);
}

#[test]
fn malformed_profiles_are_argument_errors() {
    let cli = Cli::new();
    for name in [
        "invalid_malformed_profile.toml",
        "missing_fields_profile.toml",
    ] {
        let path = fixture(name);
        let out = cli.mock(&[
            "--profile-file",
            path.to_str().unwrap(),
            "flash",
            "--profile",
            "whatever",
        ]);
        out.assert_code(exit::INVALID_ARGS_OR_PROFILE);
    }
}

#[test]
fn unknown_profile_name_is_an_argument_error() {
    let cli = Cli::new();
    cli.mock(&["profile", "show", "no-such-profile"])
        .assert_code(exit::INVALID_ARGS_OR_PROFILE);
    cli.mock(&["flash", "--profile", "no-such-profile"])
        .assert_code(exit::INVALID_ARGS_OR_PROFILE);
}

#[test]
fn flash_without_a_file_or_profile_is_an_argument_error() {
    let cli = Cli::new();
    cli.mock(&["flash", "--target", MOCK_TARGET])
        .assert_code(exit::INVALID_ARGS_OR_PROFILE);
}

#[test]
fn sparse_and_out_of_order_images_still_program() {
    let cli = Cli::new();
    // Gapped, overlapping-but-consistent, and descending-address records are
    // legal Intel HEX and must not be mistaken for corruption.
    for name in [
        "valid_stm32_bootloader_app_gap.hex",
        "valid_redundant_overlap.hex",
        "valid_stm32_out_of_order.hex",
        "valid_start_segment_type03.hex",
    ] {
        assert_eq!(
            flash_code(&cli, name, &["--verify"]),
            exit::SUCCESS,
            "{name} should program cleanly"
        );
    }
}

#[test]
fn exact_page_and_sector_sized_images_program() {
    let cli = Cli::new();
    for name in [
        "valid_tiny_16b.bin",
        "valid_exact_page_256b.bin",
        "valid_exact_sector_1kb.bin",
    ] {
        assert_eq!(
            flash_code(&cli, name, &["-a", "0x08000000", "--verify"]),
            exit::SUCCESS,
            "{name} should program cleanly"
        );
    }
}
