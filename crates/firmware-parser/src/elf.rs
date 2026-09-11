use crate::checksum::compute_canonical_checksums;
use crate::error::ParseError;
use crate::metadata::{
    EntryPointSource, FirmwareFormat, FirmwareImage, FirmwareMetadata,
};
use crate::segment::{build_segments_metadata, consolidate_chunks, RawChunk};

/// ELF magic number: 0x7F 'E' 'L' 'F'.
pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];

const ELFCLASS32: u8 = 1;
const ELFDATA2LSB: u8 = 1;
const PT_LOAD: u32 = 1;

/// Returns true when `bytes` starts with the ELF magic number.
pub fn is_elf(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0..4] == ELF_MAGIC
}

fn invalid(reason: impl Into<String>) -> ParseError {
    ParseError::InvalidElf {
        reason: reason.into(),
    }
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, ParseError> {
    bytes
        .get(offset..offset + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| invalid(format!("truncated header at offset 0x{offset:X}")))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    bytes
        .get(offset..offset + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| invalid(format!("truncated header at offset 0x{offset:X}")))
}

/// Parses a 32-bit little-endian ELF executable into its loadable segments.
///
/// Only `PT_LOAD` program headers with a non-zero file size contribute data, and
/// each is placed at its physical address (`p_paddr`, the load address) rather
/// than its virtual address, so initialised data destined for RAM is written to
/// its flash copy. Section headers, the symbol table and debug information are
/// not part of the image.
pub fn parse_elf(bytes: &[u8]) -> Result<FirmwareImage, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::EmptyFile);
    }
    if !is_elf(bytes) {
        return Err(invalid("missing ELF magic number"));
    }
    if bytes.len() < 52 {
        return Err(invalid("file shorter than an ELF32 header"));
    }
    if bytes[4] != ELFCLASS32 {
        return Err(invalid(
            "only 32-bit ELF files are supported (ELFCLASS64 found)",
        ));
    }
    if bytes[5] != ELFDATA2LSB {
        return Err(invalid(
            "only little-endian ELF files are supported (ELFDATA2MSB found)",
        ));
    }

    let entry = u32_at(bytes, 0x18)?;
    let phoff = u32_at(bytes, 0x1C)? as usize;
    let phentsize = u16_at(bytes, 0x2A)? as usize;
    let phnum = u16_at(bytes, 0x2C)? as usize;

    if phnum == 0 {
        return Err(invalid("no program headers: not a linked executable"));
    }
    if phentsize < 32 {
        return Err(invalid(format!(
            "program header entry size {phentsize} is smaller than ELF32_Phdr"
        )));
    }

    let mut chunks: Vec<RawChunk> = Vec::new();
    for index in 0..phnum {
        let base = phoff
            .checked_add(index * phentsize)
            .ok_or_else(|| invalid("program header table offset overflows"))?;

        if u32_at(bytes, base)? != PT_LOAD {
            continue;
        }

        let offset = u32_at(bytes, base + 4)? as usize;
        let paddr = u32_at(bytes, base + 12)?;
        let filesz = u32_at(bytes, base + 16)? as usize;

        // `.bss` and other NOBITS segments occupy memory but carry no file
        // content; there is nothing to program for them.
        if filesz == 0 {
            continue;
        }

        let end = offset
            .checked_add(filesz)
            .ok_or_else(|| invalid("segment file range overflows"))?;
        let data = bytes
            .get(offset..end)
            .ok_or_else(|| {
                invalid(format!(
                    "segment {index} data (offset 0x{offset:X}, {filesz} bytes) extends past end of file"
                ))
            })?
            .to_vec();

        chunks.push(RawChunk {
            address: paddr,
            data,
            line: index + 1,
        });
    }

    if chunks.is_empty() {
        return Err(invalid("no loadable PT_LOAD segments with file content"));
    }

    let (segments, gaps, warnings) = consolidate_chunks(chunks)?;

    let (crc32, checksums) = compute_canonical_checksums(&segments);
    let md5 = checksums.md5.clone();
    let sha256 = checksums.sha256.clone();

    let total_bytes: usize = segments.iter().map(|s| s.data.len()).sum();
    let base_address = segments.first().map(|s| s.start_address).unwrap_or(0);
    let highest_address = segments.last().map(|s| s.end_address()).unwrap_or(0);
    let address_span = {
        let start = segments.first().map(|s| s.start_address as u64).unwrap_or(0);
        let end = segments.last().map(|s| s.end_address_u64()).unwrap_or(0);
        end.saturating_sub(start)
    };
    let gap_count = gaps.len();
    let gap_bytes = gaps.iter().map(|g| g.size as u64).sum();
    let segment_metas = build_segments_metadata(&segments);

    let metadata = FirmwareMetadata {
        file_path: None,
        format: FirmwareFormat::Elf,
        file_size_bytes: bytes.len() as u64,
        total_bytes,
        total_firmware_bytes: total_bytes as u64,
        base_address,
        highest_address,
        address_span,
        gap_count,
        gap_bytes,
        entry_point: Some(entry),
        entry_point_source: EntryPointSource::ElfHeader,
        segment_count: segments.len(),
        segments: segment_metas,
        memory_gaps: gaps,
        crc32,
        md5,
        sha256,
        checksums,
        warnings,
    };

    Ok(FirmwareImage { metadata, segments })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal ELF32 LE file with the given PT_LOAD segments
    /// (p_paddr, data) plus one PT_LOAD NOBITS segment and one non-LOAD header.
    fn synth_elf(entry: u32, loads: &[(u32, &[u8])]) -> Vec<u8> {
        const EHSIZE: usize = 52;
        const PHENTSIZE: usize = 32;
        let phnum = loads.len() + 2;
        let data_off = EHSIZE + PHENTSIZE * phnum;

        let mut header = vec![0u8; EHSIZE];
        header[0..4].copy_from_slice(&ELF_MAGIC);
        header[4] = ELFCLASS32;
        header[5] = ELFDATA2LSB;
        header[6] = 1; // EV_CURRENT
        header[0x10..0x12].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        header[0x12..0x14].copy_from_slice(&40u16.to_le_bytes()); // EM_ARM
        header[0x18..0x1C].copy_from_slice(&entry.to_le_bytes());
        header[0x1C..0x20].copy_from_slice(&(EHSIZE as u32).to_le_bytes());
        header[0x2A..0x2C].copy_from_slice(&(PHENTSIZE as u16).to_le_bytes());
        header[0x2C..0x2E].copy_from_slice(&(phnum as u16).to_le_bytes());

        let mut phdrs = Vec::new();
        let mut blob = Vec::new();
        for (paddr, data) in loads {
            let offset = data_off + blob.len();
            let mut ph = vec![0u8; PHENTSIZE];
            ph[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
            ph[4..8].copy_from_slice(&(offset as u32).to_le_bytes());
            ph[8..12].copy_from_slice(&paddr.to_le_bytes()); // p_vaddr
            ph[12..16].copy_from_slice(&paddr.to_le_bytes()); // p_paddr
            ph[16..20].copy_from_slice(&(data.len() as u32).to_le_bytes());
            ph[20..24].copy_from_slice(&(data.len() as u32).to_le_bytes());
            phdrs.extend_from_slice(&ph);
            blob.extend_from_slice(data);
        }

        // PT_LOAD with filesz 0, memsz 1024 - a .bss segment in RAM.
        let mut bss = vec![0u8; PHENTSIZE];
        bss[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
        bss[8..12].copy_from_slice(&0x2000_0000u32.to_le_bytes());
        bss[12..16].copy_from_slice(&0x2000_0000u32.to_le_bytes());
        bss[20..24].copy_from_slice(&1024u32.to_le_bytes());
        phdrs.extend_from_slice(&bss);

        // PT_ARM_EXIDX-like non-loadable header.
        let mut other = vec![0u8; PHENTSIZE];
        other[0..4].copy_from_slice(&0x7000_0001u32.to_le_bytes());
        phdrs.extend_from_slice(&other);

        let mut out = header;
        out.extend_from_slice(&phdrs);
        out.extend_from_slice(&blob);
        // Trailing section headers / symbols / DWARF that must not be flashed.
        out.extend_from_slice(&[0xAB; 4096]);
        out
    }

    #[test]
    fn loads_only_pt_load_file_content() {
        let text = [0x11u8; 64];
        let elf = synth_elf(0x0800_0101, &[(0x0800_0000, &text)]);
        let image = parse_elf(&elf).unwrap();

        assert_eq!(image.segments.len(), 1);
        assert_eq!(image.segments[0].start_address, 0x0800_0000);
        assert_eq!(image.segments[0].data, text);
        // The 4096 trailing bytes and the .bss segment are excluded.
        assert_eq!(image.metadata.total_bytes, 64);
        assert!(image.metadata.file_size_bytes > 4096);
        assert_eq!(image.metadata.entry_point, Some(0x0800_0101));
        assert_eq!(image.metadata.format, FirmwareFormat::Elf);
    }

    #[test]
    fn keeps_separate_segments_and_records_the_gap() {
        let a = [0x01u8; 16];
        let b = [0x02u8; 16];
        let elf = synth_elf(0x0800_0000, &[(0x0800_0000, &a), (0x0801_0000, &b)]);
        let image = parse_elf(&elf).unwrap();

        assert_eq!(image.segments.len(), 2);
        assert_eq!(image.segments[1].start_address, 0x0801_0000);
        assert_eq!(image.metadata.total_bytes, 32);
        assert_eq!(image.metadata.gap_count, 1);
    }

    #[test]
    fn rejects_elf64() {
        let mut elf = synth_elf(0, &[(0x0800_0000, &[0u8; 8])]);
        elf[4] = 2;
        assert!(matches!(
            parse_elf(&elf).unwrap_err(),
            ParseError::InvalidElf { .. }
        ));
    }

    #[test]
    fn rejects_object_file_without_program_headers() {
        let mut elf = synth_elf(0, &[(0x0800_0000, &[0u8; 8])]);
        elf[0x2C..0x2E].copy_from_slice(&0u16.to_le_bytes());
        assert!(matches!(
            parse_elf(&elf).unwrap_err(),
            ParseError::InvalidElf { .. }
        ));
    }

    #[test]
    fn is_elf_detects_magic() {
        assert!(is_elf(&[0x7F, b'E', b'L', b'F', 1]));
        assert!(!is_elf(b":020000040800F2"));
        assert!(!is_elf(&[0x7F]));
    }
}
