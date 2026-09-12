//! The OpenOCD backend against the shared backend conformance suite, plus the
//! behaviour that is specific to driving a separate process.
//!
//! Everything here runs against the in-memory OpenOCD, so the session logic --
//! geometry building, alignment rules, the scratch-file dance, verification,
//! the local-only interlock -- is exercised in CI with no OpenOCD installed
//! and no board attached. What remains unverified is OpenOCD's own command
//! semantics, which only a real process can confirm.

use firmware_parser::MemorySegment;
use flash_backend_openocd::fake::{FakeOpenOcd, FLASH_BASE, RAM_BASE};
use flash_backend_openocd::{OpenOcdBackend, TclLink};
use flash_core::conformance::check_backend;
use flash_core::error::FlashError;
use flash_core::progress::FlashStage;
use flash_core::traits::{FlashBackend, FlashSession};
use flash_core::types::{ConnectionConfig, ProgramOptions, Transport};

fn config() -> ConnectionConfig {
    ConnectionConfig {
        probe_id: Some("openocd:127.0.0.1:6666".to_string()),
        target_name: "auto".to_string(),
        transport: Transport::Rpc {
            endpoint: "127.0.0.1:6666".to_string(),
        },
        ..Default::default()
    }
}

/// A session over a caller-supplied simulated OpenOCD.
fn session_over(fake: FakeOpenOcd) -> Box<dyn FlashSession> {
    let backend = OpenOcdBackend::with_link_factory(
        Box::new(move |_| {
            // One session per factory call is enough for these tests; the
            // fake is cloned by rebuilding it.
            Ok(Box::new(if fake.is_local() {
                FakeOpenOcd::new()
            } else {
                FakeOpenOcd::remote()
            }) as Box<dyn TclLink>)
        }),
        Vec::new(),
    );
    backend
        .open_session(&config())
        .expect("the simulated OpenOCD opens a session")
}

#[test]
fn openocd_backend_conforms() {
    let backend = OpenOcdBackend::simulated();
    if let Err(failure) = check_backend(&backend, &config()) {
        panic!("openocd backend breaks the backend contract -- {failure}");
    }
}

#[test]
fn the_listed_endpoint_routes_back_to_this_backend() {
    let backend = OpenOcdBackend::new();
    let probes = backend
        .list_probes()
        .expect("listing an endpoint never needs a connection");
    assert!(
        !probes.is_empty(),
        "the default endpoint is always worth offering: OpenOCD may be started \
         a second after this list is drawn"
    );
    assert!(probes.iter().all(|p| p.identifier.starts_with("openocd:")));
}

#[test]
fn a_running_target_is_halted_before_anything_is_written() {
    // OpenOCD's own failure for a running target arrives part way through a
    // write, which is a worse place to find out.
    let backend = OpenOcdBackend::simulated();
    let session = backend.open_session(&config()).unwrap();
    assert!(session.target_info().is_some());
}

#[test]
fn a_configuration_with_no_current_target_is_refused_with_the_reason() {
    let backend = OpenOcdBackend::with_link_factory(
        Box::new(|_| {
            let mut fake = FakeOpenOcd::new();
            fake.no_current_target = true;
            Ok(Box::new(fake) as Box<dyn TclLink>)
        }),
        Vec::new(),
    );

    let err = match backend.open_session(&config()) {
        Err(e) => e,
        Ok(_) => panic!("the session should not have opened"),
    };
    let message = err.to_string();
    assert!(
        message.contains("no current target"),
        "the message must say what is missing, got: {message}"
    );
}

#[test]
fn a_configuration_with_no_flash_bank_is_refused_with_the_reason() {
    let backend = OpenOcdBackend::with_link_factory(
        Box::new(|_| {
            let mut fake = FakeOpenOcd::new();
            fake.no_flash_banks = true;
            Ok(Box::new(fake) as Box<dyn TclLink>)
        }),
        Vec::new(),
    );

    let err = match backend.open_session(&config()) {
        Err(e) => e,
        Ok(_) => panic!("the session should not have opened"),
    };
    assert!(err.to_string().contains("no flash banks"), "got: {err}");
}

#[test]
fn programming_is_refused_against_a_remote_openocd() {
    // `flash write_image` makes OpenOCD open the path, so a file written here
    // means nothing to a process on another machine. Better to refuse than to
    // hand it a path it cannot see.
    let mut session = session_over(FakeOpenOcd::remote());
    let segment = MemorySegment {
        start_address: FLASH_BASE,
        data: vec![0xAA; 64],
    };

    let err = session
        .program(&[segment], &ProgramOptions::default(), None)
        .unwrap_err();
    match err {
        FlashError::Unsupported(ref message) => {
            assert!(
                message.contains("runs on this machine"),
                "the message must say why, got: {message}"
            );
        }
        other => panic!("expected an Unsupported refusal, got {other}"),
    }
}

#[test]
fn a_remote_openocd_can_still_read_and_erase() {
    // Only the file-based commands need a shared file system; the rest of the
    // session works over the socket alone, and refusing all of it would be
    // needlessly strict.
    let mut session = session_over(FakeOpenOcd::remote());
    assert!(session.erase_range(FLASH_BASE, 4096, None).is_ok());
    let bytes = session.read_memory(FLASH_BASE, 16).unwrap();
    assert!(bytes.iter().all(|b| *b == 0xFF), "got {bytes:02X?}");
}

#[test]
fn an_erase_that_would_start_mid_sector_is_refused() {
    let mut session = session_over(FakeOpenOcd::new());
    let err = session
        .erase_range(FLASH_BASE + 0x100, 4096, None)
        .unwrap_err();
    match err {
        FlashError::InvalidAddress {
            address,
            ref reason,
        } => {
            assert_eq!(address, FLASH_BASE + 0x100);
            assert!(
                reason.contains("sector boundary"),
                "the message must say what the rule is, got: {reason}"
            );
        }
        other => panic!("expected the unaligned start to be refused, got {other}"),
    }
}

#[test]
fn an_erase_past_the_end_of_flash_is_refused() {
    let mut session = session_over(FakeOpenOcd::new());
    let target = session.target_info().unwrap().clone();
    assert!(session
        .erase_range(target.flash_end() - 16, 4096, None)
        .is_err());
}

#[test]
fn what_is_programmed_is_what_reads_back() {
    let mut session = session_over(FakeOpenOcd::new());
    let data: Vec<u8> = (0..256u32).map(|i| (i as u8) ^ 0x3C).collect();
    let segment = MemorySegment {
        start_address: FLASH_BASE,
        data: data.clone(),
    };

    session
        .program(
            std::slice::from_ref(&segment),
            &ProgramOptions::default(),
            None,
        )
        .expect("a local OpenOCD writes through a scratch file");

    assert_eq!(session.read_memory(FLASH_BASE, 256).unwrap(), data);

    let report = session
        .verify(std::slice::from_ref(&segment), None)
        .unwrap();
    assert!(
        report.success,
        "verify disagreed with its own write: {report:?}"
    );
    assert_eq!(report.bytes_verified, 256);
}

#[test]
fn a_verify_says_where_the_difference_is() {
    // The reason this backend reads the region back instead of calling
    // OpenOCD's `verify_image`: that command answers only pass or fail.
    let mut session = session_over(FakeOpenOcd::new());
    let mut data = vec![0x11u8; 64];
    let segment = MemorySegment {
        start_address: FLASH_BASE,
        data: data.clone(),
    };
    session
        .program(
            std::slice::from_ref(&segment),
            &ProgramOptions::default(),
            None,
        )
        .unwrap();

    data[8] = 0x22;
    let changed = MemorySegment {
        start_address: FLASH_BASE,
        data,
    };
    let report = session.verify(&[changed], None).unwrap();

    assert!(!report.success);
    assert_eq!(
        report.mismatches.first().map(|m| m.address),
        Some(FLASH_BASE + 8)
    );
    assert_ne!(report.checksum_expected, report.checksum_actual);
}

#[test]
fn programming_does_its_own_erase() {
    // `flash write_image erase` clears exactly the sectors it writes, so the
    // manager must not erase them a second time.
    let session = session_over(FakeOpenOcd::new());
    assert!(session.program_erases_target());
}

#[test]
fn only_verification_can_be_interrupted() {
    // An erase or a write is one OpenOCD command that returns when it is
    // finished; there is no point inside it at which a stop could take effect,
    // and offering one would be a button that does nothing.
    let session = session_over(FakeOpenOcd::new());
    assert!(!session.can_interrupt(FlashStage::Erasing));
    assert!(!session.can_interrupt(FlashStage::Programming));
    assert!(session.can_interrupt(FlashStage::Verifying));
}

#[test]
fn a_memory_write_reaches_ram_and_reads_back() {
    let mut session = session_over(FakeOpenOcd::new());
    assert!(session.can_write_memory());
    session
        .write_memory(RAM_BASE + 8, &[0xDE, 0xAD, 0xBE, 0xEF])
        .expect("a RAM write goes through load_image on a local OpenOCD");
    assert_eq!(
        session.read_memory(RAM_BASE + 8, 4).unwrap(),
        vec![0xDE, 0xAD, 0xBE, 0xEF]
    );
}

#[test]
fn a_memory_write_into_flash_is_refused() {
    let mut session = session_over(FakeOpenOcd::new());
    let err = session.write_memory(FLASH_BASE, &[0x00]).unwrap_err();
    match err {
        FlashError::InvalidAddress { ref reason, .. } => assert!(
            reason.contains("program"),
            "the message must name the path that erases, got: {reason}"
        ),
        other => panic!("expected the flash address to be refused, got {other}"),
    }
}

#[test]
fn a_large_memory_write_to_a_remote_openocd_is_refused_rather_than_crawled() {
    // Without a shared file system the bytes go one command at a time. A
    // kilobyte that way is a thousand round trips, so the limit is stated
    // instead of being discovered by waiting.
    let mut session = session_over(FakeOpenOcd::remote());
    let err = session
        .write_memory(RAM_BASE, &vec![0u8; 4096])
        .unwrap_err();
    assert!(matches!(err, FlashError::Unsupported(_)), "got {err}");

    // A register-sized write still works.
    assert!(session.write_memory(RAM_BASE, &[0x5A]).is_ok());
    assert_eq!(session.read_memory(RAM_BASE, 1).unwrap(), vec![0x5A]);
}

#[test]
fn closing_leaves_openocd_running_and_is_idempotent() {
    // OpenOCD is someone else's process and may be serving GDB as well;
    // shutting it down would be taking something that was not ours.
    let mut session = session_over(FakeOpenOcd::new());
    assert!(session.close().is_ok());
    assert!(session.close().is_ok());
    // And the session refuses work afterwards rather than reconnecting
    // silently.
    assert!(session.read_memory(FLASH_BASE, 4).is_err());
}
