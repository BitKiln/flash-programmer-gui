//! Direct memory writes: where they land, and where they must be refused.
//!
//! A memory write skips the erase that programming performs, so the interesting
//! assertions are about what it declines to do.

use flash_backend_mock::MockProbeBackend;
use flash_core::error::FlashError;
use flash_core::traits::{FlashBackend, FlashSession};
use flash_core::types::ConnectionConfig;

fn session() -> Box<dyn FlashSession> {
    let backend = MockProbeBackend::new();
    let config = ConnectionConfig {
        target_name: "stm32f401re".to_string(),
        ..Default::default()
    };
    backend
        .open_session(&config)
        .expect("the mock backend opens a session")
}

#[test]
fn a_ram_write_reads_back_byte_for_byte() {
    let mut session = session();
    let base = session.target_info().expect("target").ram_base;
    session
        .write_memory(base + 4, &[0xDE, 0xAD, 0xBE, 0xEF])
        .expect("a RAM write is accepted");
    assert_eq!(
        session.read_memory(base + 4, 4).unwrap(),
        vec![0xDE, 0xAD, 0xBE, 0xEF]
    );
}

#[test]
fn ram_takes_any_value_over_any_other_without_an_erase() {
    let mut session = session();
    let base = session.target_info().expect("target").ram_base;
    session.write_memory(base, &[0xFF, 0xFF]).unwrap();
    // Flash could not make this transition; RAM can, and that difference is
    // the reason the two have separate paths.
    session.write_memory(base, &[0x00, 0x0F]).unwrap();
    assert_eq!(session.read_memory(base, 2).unwrap(), vec![0x00, 0x0F]);
}

#[test]
fn a_write_into_flash_is_refused_rather_than_attempted() {
    let mut session = session();
    let base = session.target_info().expect("target").flash_base;
    let err = session.write_memory(base, &[0x00]).unwrap_err();
    match err {
        FlashError::InvalidAddress { address, ref reason } => {
            assert_eq!(address, base);
            assert!(
                reason.contains("program"),
                "the message must name the path that does erase, got {reason:?}"
            );
        }
        other => panic!("expected the flash address to be refused, got {other}"),
    }
}

#[test]
fn a_write_that_runs_off_the_end_of_ram_is_refused() {
    let mut session = session();
    let target = session.target_info().expect("target").clone();
    let last_word = target.ram_base + target.ram_size - 2;
    assert!(
        session.write_memory(last_word, &[0u8; 4]).is_err(),
        "a write may not be truncated at the end of RAM"
    );
}

#[test]
fn a_session_that_can_write_memory_says_so() {
    let session = session();
    assert!(session.can_write_memory());
}
