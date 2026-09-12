//! Serial-number rendering, encoding, and the write-plus-read-back path.

use flash_backend_mock::MockProbeBackend;
use flash_core::serial::{program_serial, SerialAllocator, SerialConfig, SerialEncoding};
use flash_core::traits::FlashBackend;
use flash_core::types::ConnectionConfig;
use flash_core::FlashError;

fn config() -> SerialConfig {
    SerialConfig {
        address: 0x0801_0000,
        format: "SN-{n:06}".to_string(),
        start: 41,
        step: 1,
        encoding: SerialEncoding::Ascii,
        width: 16,
        pad: 0xFF,
        verify: true,
    }
}

fn session() -> Box<dyn flash_core::traits::FlashSession> {
    let backend = MockProbeBackend::new();
    backend
        .open_session(&ConnectionConfig {
            probe_id: Some("mock:stlink-stm32f401re".to_string()),
            target_name: "STM32F401RE".to_string(),
            ..ConnectionConfig::default()
        })
        .expect("mock session")
}

#[test]
fn renders_padded_and_unpadded_placeholders() {
    let mut cfg = config();
    assert_eq!(cfg.render(42), "SN-000042");

    cfg.format = "{n}".to_string();
    assert_eq!(cfg.render(7), "7");

    cfg.format = "ACME-{n:04}-REVB".to_string();
    assert_eq!(cfg.render(9), "ACME-0009-REVB");

    // A value wider than the pad width is not truncated.
    cfg.format = "{n:03}".to_string();
    assert_eq!(cfg.render(123456), "123456");

    // An unrecognised placeholder is left alone rather than silently dropped.
    cfg.format = "{oops}-{n}".to_string();
    assert_eq!(cfg.render(5), "{oops}-5");
}

#[test]
fn encodes_each_width_and_rejects_an_oversized_ascii_field() {
    let mut cfg = config();
    let bytes = cfg.encode(42, &cfg.render(42)).expect("ascii encode");
    assert_eq!(bytes.len(), 16);
    assert_eq!(&bytes[..9], b"SN-000042");
    assert!(bytes[9..].iter().all(|b| *b == 0xFF), "field must be padded");

    cfg.width = 4;
    let err = cfg.encode(42, &cfg.render(42)).expect_err("must not fit");
    assert!(matches!(err, FlashError::ProgramError(_)), "got {:?}", err);

    let numeric = SerialConfig {
        encoding: SerialEncoding::U32Le,
        ..config()
    };
    assert_eq!(numeric.encode(0x0102_0304, "").unwrap(), vec![4, 3, 2, 1]);
    assert_eq!(numeric.byte_width(), 4);

    let big_endian = SerialConfig {
        encoding: SerialEncoding::U32Be,
        ..config()
    };
    assert_eq!(big_endian.encode(0x0102_0304, "").unwrap(), vec![1, 2, 3, 4]);

    let wide = SerialConfig {
        encoding: SerialEncoding::U64Le,
        ..config()
    };
    assert_eq!(wide.encode(1, "").unwrap().len(), 8);

    // A counter past 32 bits is an error, not a silent truncation.
    assert!(numeric.encode(u64::from(u32::MAX) + 1, "").is_err());
}

#[test]
fn the_allocator_hands_out_one_value_per_board() {
    let allocator = SerialAllocator::new(SerialConfig {
        start: 100,
        step: 5,
        ..config()
    });

    assert_eq!(allocator.peek(), 100);
    assert_eq!(allocator.take(), 100);
    assert_eq!(allocator.take(), 105);
    assert_eq!(allocator.take(), 110);
    assert_eq!(allocator.peek(), 115);

    // A zero step would stamp every board the same; it advances by one instead.
    let stuck = SerialAllocator::new(SerialConfig {
        start: 1,
        step: 0,
        ..config()
    });
    assert_eq!(stuck.take(), 1);
    assert_eq!(stuck.take(), 2);
}

#[test]
fn writes_the_serial_to_the_target_and_reads_it_back() {
    let mut session = session();
    let cfg = config();

    let rendered = program_serial(session.as_mut(), &cfg, 42).expect("serial write");
    assert_eq!(rendered, "SN-000042");

    let read_back = session
        .read_memory(cfg.address, cfg.byte_width() as u32)
        .expect("read back");
    assert_eq!(&read_back[..9], b"SN-000042");
    assert!(read_back[9..].iter().all(|b| *b == 0xFF));
}

#[test]
fn restamping_a_board_replaces_the_previous_serial() {
    let mut session = session();
    let cfg = config();

    program_serial(session.as_mut(), &cfg, 1).expect("first serial");
    let second = program_serial(session.as_mut(), &cfg, 2).expect("second serial");
    assert_eq!(second, "SN-000002");

    let read_back = session
        .read_memory(cfg.address, cfg.byte_width() as u32)
        .expect("read back");
    assert_eq!(&read_back[..9], b"SN-000002");
}

#[test]
fn a_serial_outside_the_target_flash_is_refused() {
    let mut session = session();
    let cfg = SerialConfig {
        address: 0x2000_0000, // RAM, not flash
        ..config()
    };

    let err = program_serial(session.as_mut(), &cfg, 1).expect_err("must be refused");
    assert!(
        matches!(err, FlashError::AddressOutOfBounds { .. }),
        "got {:?}",
        err
    );
}
