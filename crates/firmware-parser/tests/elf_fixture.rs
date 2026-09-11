//! Regression tests against a real compiler-produced ELF.
//!
//! The unit tests in `src/elf.rs` build their input by hand, which keeps them
//! readable but means they only exercise the layout the test itself chose.
//! These run against `tests/fixtures/valid_u575_blinky.elf`, linked by the Arm
//! GNU toolchain for an STM32U575 (see `tests/firmware/u575/`), so real linker
//! output is covered too.

use std::path::PathBuf;

use firmware_parser::{detect_format, parse_file, EntryPointSource, FirmwareFormat};

/// Size of the loadable image: `.isr_vector` + `.text` (0x4C0C) immediately
/// followed by the `.data` load copy (8 bytes).
const IMAGE_BYTES: usize = 0x4C14;

/// Load address of `.data`, which the linker places in flash directly after
/// `.text` while its virtual address is in RAM at 0x20000000.
const DATA_LOAD_ADDRESS: u32 = 0x0800_4C0C;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

#[test]
fn parses_only_the_loadable_image_not_the_whole_file() {
    let path = fixture("valid_u575_blinky.elf");
    let image = parse_file(&path, None).expect("fixture ELF should parse");
    let meta = &image.metadata;

    assert_eq!(meta.format, FirmwareFormat::Elf);
    assert_eq!(meta.total_bytes, IMAGE_BYTES);

    // The file on disk carries section headers, the symbol table and debug
    // information on top of the loadable image. Flashing the file verbatim -
    // which is what happened before ELF was understood - would program far
    // more than the firmware.
    assert!(
        meta.file_size_bytes > IMAGE_BYTES as u64,
        "fixture should contain non-loadable data, file is {} bytes",
        meta.file_size_bytes
    );
}

#[test]
fn places_data_at_its_load_address_not_its_virtual_address() {
    let image = parse_file(fixture("valid_u575_blinky.elf"), None).expect("parse");

    // `.text` and the `.data` load copy are adjacent in flash, so they
    // coalesce into a single contiguous segment with no gap.
    assert_eq!(image.metadata.segment_count, 1);
    assert_eq!(image.metadata.gap_count, 0);

    let segment = &image.segments[0];
    assert_eq!(segment.start_address, 0x0800_0000);
    assert_eq!(segment.end_address(), 0x0800_0000 + IMAGE_BYTES as u32);

    // Nothing may be addressed at the virtual address of `.data`; writing
    // there would mean programming RAM instead of flash.
    assert!(
        image
            .segments
            .iter()
            .all(|s| s.start_address < 0x2000_0000),
        "no segment may be placed at a RAM virtual address"
    );

    // The two initialised globals, in order: blink_count = 0xA5A5A5A5 and
    // marker = 0xDEADBEEF, little-endian, at the flash load address.
    let offset = (DATA_LOAD_ADDRESS - segment.start_address) as usize;
    assert_eq!(
        &segment.data[offset..offset + 8],
        &[0xA5, 0xA5, 0xA5, 0xA5, 0xEF, 0xBE, 0xAD, 0xDE]
    );
}

#[test]
fn reads_the_vector_table_and_entry_point() {
    let image = parse_file(fixture("valid_u575_blinky.elf"), None).expect("parse");

    // Initial stack pointer at the top of the 192 KB SRAM, then the reset
    // handler with the Thumb bit set.
    assert_eq!(
        &image.segments[0].data[0..8],
        &[0x00, 0x00, 0x03, 0x20, 0x45, 0x00, 0x00, 0x08]
    );

    assert_eq!(image.metadata.entry_point, Some(0x0800_0040));
    assert_eq!(image.metadata.entry_point_source, EntryPointSource::ElfHeader);
}

#[test]
fn elf_and_objcopy_binary_describe_the_same_image() {
    let elf = parse_file(fixture("valid_u575_blinky.elf"), None).expect("parse elf");
    let bin = parse_file(fixture("valid_u575_blinky.bin"), None).expect("parse bin");

    // The .bin fixture is `objcopy -O binary` of the same ELF, so a correct
    // ELF parser must reproduce it byte for byte.
    assert_eq!(bin.metadata.format, FirmwareFormat::RawBinary);
    assert_eq!(elf.metadata.total_bytes, bin.metadata.total_bytes);
    assert_eq!(elf.segments[0].data, bin.segments[0].data);
    assert_eq!(elf.metadata.crc32, bin.metadata.crc32);
}

#[test]
fn detects_elf_by_extension_and_by_magic() {
    let bytes = std::fs::read(fixture("valid_u575_blinky.elf")).expect("read fixture");

    let path = fixture("valid_u575_blinky.elf");
    assert_eq!(detect_format(&bytes, Some(&path)), FirmwareFormat::Elf);

    // Content alone is enough; the extension is not required.
    assert_eq!(detect_format(&bytes, None), FirmwareFormat::Elf);
}
