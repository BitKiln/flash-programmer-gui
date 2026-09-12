//! Tier 5 — real hardware.
//!
//! These tests drive an attached debug probe and **erase and reprogram the
//! connected board**. They are `#[ignore]`d and additionally require an
//! explicit opt-in, so neither CI nor a plain `cargo test` can flash a board by
//! accident:
//!
//! ```text
//! FLASHGUI_HW_TARGET=STM32U575ZITxQ cargo test -p e2e-tests --test hardware -- --ignored
//! ```
//!
//! Optionally set `FLASHGUI_HW_PROBE` to pin a specific probe when more than
//! one is attached.

use e2e_harness::{exit, fixture, Cli, Output};

/// Reads the opt-in target name, skipping the body when it is absent.
fn hw_target() -> Option<String> {
    std::env::var("FLASHGUI_HW_TARGET")
        .ok()
        .filter(|t| !t.trim().is_empty())
}

fn hw_probe() -> Option<String> {
    std::env::var("FLASHGUI_HW_PROBE")
        .ok()
        .filter(|p| !p.trim().is_empty())
}

/// Runs the CLI against real hardware with the configured target and probe.
fn hw(cli: &Cli, target: &str, args: &[&str]) -> Output {
    let mut full: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    full.push("--target".into());
    full.push(target.into());
    if let Some(probe) = hw_probe() {
        full.push("--probe".into());
        full.push(probe);
    }
    let refs: Vec<&str> = full.iter().map(String::as_str).collect();
    cli.run(&refs)
}

#[test]
#[ignore = "requires an attached debug probe and target board"]
fn probe_is_visible() {
    let Some(_target) = hw_target() else {
        eprintln!("FLASHGUI_HW_TARGET not set; skipping");
        return;
    };
    let cli = Cli::new();
    let out = cli.run(&["devices"]);
    out.assert_code(exit::SUCCESS);
    assert!(
        !out.stdout.contains("No debug probes or ESP serial ports detected"),
        "no probe detected:\n{}",
        out.stdout
    );
}

#[test]
#[ignore = "erases and reprograms the attached board"]
fn flash_verify_and_reset_a_real_board() {
    let Some(target) = hw_target() else {
        eprintln!("FLASHGUI_HW_TARGET not set; skipping");
        return;
    };
    let cli = Cli::new();
    let elf = fixture("valid_u575_blinky.elf");

    hw(&cli, &target, &["erase", "--full"]).assert_code(exit::SUCCESS);

    // Erased flash must not match the image; this proves the erase took effect
    // on the device rather than the verify simply always passing.
    assert_eq!(
        hw(&cli, &target, &["verify", elf.to_str().unwrap()]).code,
        exit::FLASH_VERIFY_ERROR,
        "erased board still verified against the firmware"
    );

    hw(
        &cli,
        &target,
        &["flash", elf.to_str().unwrap(), "--verify", "--reset"],
    )
    .assert_code(exit::SUCCESS);

    // Independent verify in a fresh process reads the device back.
    hw(&cli, &target, &["verify", elf.to_str().unwrap()]).assert_code(exit::SUCCESS);

    hw(&cli, &target, &["reset"]).assert_code(exit::SUCCESS);
    hw(&cli, &target, &["reset", "--halt"]).assert_code(exit::SUCCESS);
}

#[test]
#[ignore = "erases and reprograms the attached board"]
fn auto_detection_identifies_the_attached_target() {
    let Some(_target) = hw_target() else {
        eprintln!("FLASHGUI_HW_TARGET not set; skipping");
        return;
    };
    let cli = Cli::new();
    let elf = fixture("valid_u575_blinky.elf");

    // "auto" is the default target: the tool must identify the chip itself.
    let mut args = vec![
        "flash",
        elf.to_str().unwrap(),
        "--target",
        "auto",
        "--verify",
    ];
    let probe = hw_probe();
    if let Some(ref p) = probe {
        args.extend_from_slice(&["--probe", p]);
    }
    cli.run(&args).assert_code(exit::SUCCESS);
}
