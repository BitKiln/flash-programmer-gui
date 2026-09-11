//! Tier 4 — workloads.
//!
//! Scale, repetition, and cross-format agreement: the cases that only fail
//! after the tool has done real work.

use std::thread;

use e2e_harness::{exit, fixture, Cli, MOCK_TARGET};

#[test]
fn largest_fixture_programs_and_verifies() {
    let cli = Cli::new();
    let elf = fixture("valid_u575_blinky.elf");
    let out = cli.mock(&[
        "--json",
        "flash",
        elf.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "--verify",
        "--reset",
    ]);
    out.assert_code(exit::SUCCESS);

    let complete = out.ndjson_of_type("complete");
    let flashed = complete[0]["bytes_flashed"].as_u64().unwrap_or(0);
    assert!(
        flashed >= 19_000,
        "expected the whole ~19 KB image, got {flashed} bytes"
    );
}

#[test]
fn elf_and_objcopy_bin_program_identical_content() {
    let cli = Cli::new();
    let elf = fixture("valid_u575_blinky.elf");
    let bin = fixture("valid_u575_blinky.bin");

    // The .bin is `objcopy -O binary` of the same build, so flashing the ELF
    // must leave flash in a state the .bin verifies against at the load base.
    cli.mock(&["flash", elf.to_str().unwrap(), "--target", MOCK_TARGET])
        .assert_code(exit::SUCCESS);
    cli.mock(&[
        "verify",
        bin.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "-a",
        "0x08000000",
    ])
    .assert_code(exit::SUCCESS);
}

#[test]
fn repeated_flash_cycles_stay_consistent() {
    let cli = Cli::new();
    let hex = fixture("valid_stm32_single_segment.hex");
    let elf = fixture("valid_u575_blinky.elf");

    // Both images load at 0x08000000, so each write replaces the previous one.
    // What must hold across cycles is that the most recently written image
    // always reads back intact and the other one no longer does.
    for cycle in 0..5 {
        cli.mock(&["erase", "--target", MOCK_TARGET, "--full"])
            .assert_code(exit::SUCCESS);

        cli.mock(&[
            "flash",
            elf.to_str().unwrap(),
            "--target",
            MOCK_TARGET,
            "--verify",
        ])
        .assert_code(exit::SUCCESS);
        assert_eq!(
            cli.mock(&["verify", elf.to_str().unwrap(), "--target", MOCK_TARGET])
                .code,
            exit::SUCCESS,
            "ELF did not read back on cycle {cycle}"
        );

        cli.mock(&[
            "flash",
            hex.to_str().unwrap(),
            "--target",
            MOCK_TARGET,
            "--verify",
        ])
        .assert_code(exit::SUCCESS);
        assert_eq!(
            cli.mock(&["verify", hex.to_str().unwrap(), "--target", MOCK_TARGET])
                .code,
            exit::SUCCESS,
            "HEX did not read back on cycle {cycle}"
        );
        assert_eq!(
            cli.mock(&["verify", elf.to_str().unwrap(), "--target", MOCK_TARGET])
                .code,
            exit::FLASH_VERIFY_ERROR,
            "overwritten ELF still verified on cycle {cycle}"
        );
    }
}

#[test]
fn reflashing_without_erase_overwrites_cleanly() {
    let cli = Cli::new();
    let a = fixture("valid_u575_blinky.bin");
    let b = fixture("valid_multi_sector_16kb.bin");

    cli.mock(&[
        "flash",
        a.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "-a",
        "0x08000000",
    ])
    .assert_code(exit::SUCCESS);
    // A smaller image over the same base must land correctly even though the
    // sectors already hold programmed data.
    cli.mock(&[
        "flash",
        b.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "-a",
        "0x08000000",
        "--verify",
    ])
    .assert_code(exit::SUCCESS);
}

#[test]
fn concurrent_sandboxes_do_not_interfere() {
    // Each sandbox owns its own mock flash backing file; parallel runs of the
    // tool must not corrupt each other's state.
    let handles: Vec<_> = (0..4)
        .map(|_| {
            thread::spawn(|| {
                let cli = Cli::new();
                let hex = fixture("valid_stm32_single_segment.hex");
                cli.mock(&[
                    "flash",
                    hex.to_str().unwrap(),
                    "--target",
                    MOCK_TARGET,
                    "--verify",
                ])
                .assert_code(exit::SUCCESS);
                cli.mock(&["verify", hex.to_str().unwrap(), "--target", MOCK_TARGET])
                    .code
            })
        })
        .collect();

    for handle in handles {
        assert_eq!(handle.join().expect("worker panicked"), exit::SUCCESS);
    }
}

#[test]
fn progress_stream_is_monotonic_and_reaches_completion() {
    let cli = Cli::new();
    let elf = fixture("valid_u575_blinky.elf");
    let out = cli.mock(&[
        "--json",
        "flash",
        elf.to_str().unwrap(),
        "--target",
        MOCK_TARGET,
        "--verify",
    ]);
    out.assert_code(exit::SUCCESS);

    let mut last_by_stage: std::collections::HashMap<String, u64> = Default::default();
    for msg in out.ndjson_of_type("progress") {
        let stage = msg["stage"].as_str().unwrap_or_default().to_string();
        let done = msg["bytes_done"].as_u64().unwrap_or_default();
        let total = msg["total_bytes"].as_u64().unwrap_or_default();
        assert!(done <= total, "bytes_done {done} exceeded total {total}");
        let previous = last_by_stage.entry(stage.clone()).or_insert(0);
        assert!(
            done >= *previous,
            "progress went backwards in stage {stage}: {previous} then {done}"
        );
        *previous = done;
    }
    assert!(
        !last_by_stage.is_empty(),
        "no progress messages for a 19 KB image"
    );
    assert_eq!(out.ndjson_of_type("complete").len(), 1);
}
