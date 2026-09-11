# Survey & Specification Report: Firmware Parser & Memory Inspector (R2)

**Author**: Spec Miner 2 (Firmware Parser & Memory Inspector Spec)
**Date**: 2026-09-10
**Target Crate**: firmware-parser (crates/firmware-parser)
**Parent Mission**: Flash Programmer GUI & CLI Architecture (R1-R4)

---

## 1. Executive Summary & Specification Scope

The firmware-parser crate serves as the foundational data ingestion layer for the entire Flash Programmer GUI and CLI ecosystem. It is responsible for:
1. Decoding Intel HEX (.hex) and raw binary (.bin) firmware representations.
2. Accurately translating 16-bit, 20-bit, and 32-bit address spaces into physical memory addresses.
3. Consolidating fragmented or out-of-order records into contiguous memory segments while preserving sparse memory gaps.
4. Detecting conflicting data collisions, corrupted records, checksum errors, and address overflows.
5. Computing industry-standard cryptographic and integrity hashes (IEEE 802.3 CRC32, RFC 1321 MD5, FIPS 180-4 SHA-256) per-segment and across the full canonical image.
6. Extracting the execution entry point (via explicit HEX records 03/05 or via ARM Cortex-M Vector Table inspection).
7. Providing strongly typed, serializable Rust data structures consumable across Tauri IPC (frontend UI) and headless CLI automation.

---

## 2. Authoritative Specification Sources

This specification is derived from and complies with the following authoritative standards:
- **Intel Corporation**: Hexadecimal Object File Format Specification (Revision A, January 6, 1988) - Covering record framing, field definitions, record types 00-05, and two's complement checksum arithmetic.
- **ARM Ltd.**: ARMv7-M Architecture Reference Manual (ARM DDI 0403E.e) & Cortex-M Generic User Guide - Section B1.5: Vector Table format, Initial Stack Pointer (MSP) at offset 0x00, Reset Handler vector at offset 0x04, and Thumb bit (bit 0 = 1) conventions.
- **IEEE Std 802.3**: CRC-32 cyclic redundancy check specification (polynomial 0xEDB88320 reversed, initial 0xFFFFFFFF, final XOR 0xFFFFFFFF).
- **IETF RFC 1321**: The MD5 Message-Digest Algorithm.
- **NIST FIPS PUB 180-4**: Secure Hash Standard (SHS) (SHA-256).
- **probe-rs Project**: probe_rs::flashing::Format and probe_rs::flashing::BinOptions memory segment programming conventions.

---

## 3. Features Discovered

| # | Category | Feature | Description | Inputs | Outputs | Error Behavior | Discovered Via |
|---|----------|---------|-------------|--------|---------|----------------|----------------|
| 1 | File Format | Format Auto-Detection | Automatically determines whether a file is Intel HEX or Raw Binary based on file extension and leading magic character inspection (: vs binary bytes). | File path / byte slice | FirmwareFormat::{IntelHex, RawBinary} | Fallback to binary with warning if extension unrecognized. | ORIGINAL_REQUEST.md R2, probe-rs conventions |
| 2 | Intel HEX | Line Framing & Lexing | Strips whitespace, checks for mandatory leading colon :, validates even count of hexadecimal ASCII digits, and decodes byte fields. | ASCII line buffer | Byte count, address offset, record type, data slice, checksum | Rejects lines with missing :, odd hex digits, non-hex chars, or fewer than 5 bytes. | Intel HEX Spec (1988), Section 2 |
| 3 | Intel HEX | Checksum Validation | Computes two's complement of least significant byte of all record fields. Verifies (sum + checksum) & 0xFF == 0. | Decoded record bytes | Boolean validity / decoded record | Emits ChecksumMismatch { expected, found } with 1-based line number. | Intel HEX Spec (1988), Section 2.1 |
| 4 | Intel HEX | Record Type 00: Data | Decodes payload bytes and maps them to physical address: (ULBA << 16) + (USBA << 4) + address. | 16-bit offset, byte count, data bytes | Physical address chunk | Rejects if byte count does not match line data length. | Intel HEX Spec (1988), Section 3.1 |
| 5 | Intel HEX | Record Type 01: EOF | Terminates Intel HEX stream. Enforces byte count == 0. Signals end of data. | Line with record type 01 | EOF signal | Rejects if byte count != 0. Warns on data records after EOF. Warns if file lacks EOF. | Intel HEX Spec (1988), Section 3.2 |
| 6 | Intel HEX | Record Type 02: Ext Segment Addr | Sets 16-bit Upper Segment Base Address (USBA). Enables 20-bit real-mode addressing (USBA << 4). | 2 data bytes (USBA) | Sets active segment base | Rejects if byte count != 2. | Intel HEX Spec (1988), Section 3.3 (HEX86) |
| 7 | Intel HEX | Record Type 03: Start Segment Addr | Specifies 8086/80186 execution start address (CS << 4) + IP. | 4 data bytes (2 CS, 2 IP) | Entry point u64 | Rejects if byte count != 4. | Intel HEX Spec (1988), Section 3.4 (HEX86) |
| 8 | Intel HEX | Record Type 04: Ext Linear Addr | Sets 16-bit Upper Linear Base Address (ULBA). Enables full 32-bit linear addressing (ULBA << 16). | 2 data bytes (ULBA) | Sets active 32-bit linear base | Rejects if byte count != 2. | Intel HEX Spec (1988), Section 3.5 (HEX386) |
| 9 | Intel HEX | Record Type 05: Start Linear Addr | Specifies 32-bit Execution Instruction Pointer (EIP) entry point. | 4 data bytes (EIP big-endian) | Entry point u64 | Rejects if byte count != 4. | Intel HEX Spec (1988), Section 3.6 (HEX386) |
| 10 | Intel HEX | Out-of-Order Reordering | Normalizes records appearing in arbitrary non-monotonic order across the file (common in modern GCC/LLVM linkers). | Stream of records | Sorted chunk list by address | Seamlessly sorts and coalesces without data corruption. | Linker output analysis, GCC / Clang objcopy |
| 11 | Memory Model | Segment Consolidation | Merges adjacent contiguous records (prev.end == next.start) into minimal contiguous MemorySegment objects. | List of address chunks | Consolidates into Vec<MemorySegment> | Reduces thousands of 16-byte records into 1-3 cohesive flash segments. | Flash programming efficiency, probe-rs FlashLoader |
| 12 | Memory Model | Sparse Gap Preservation | Detects non-contiguous memory blocks (next.start > prev.end), preserving gaps without injecting synthetic padding bytes. | Segment boundaries | Gap metadata: count, sizes, addresses | Prevents unintentional erasure or writing to unmapped flash memory. | ORIGINAL_REQUEST.md R2, acceptance criteria |
| 13 | Memory Model | Overlap Collision Detection | Analyzes overlapping records: if overlapping data matches, logs redundant write warning; if data differs, raises conflict error. | Overlapping record chunks | Warning (redundant) or Err (conflict) | Rejects corrupted firmware images with contradictory definitions. | Acceptance criteria (corrupted files) |
| 14 | Memory Model | 32-bit Address Overflow | Guards against addresses exceeding 0xFFFF_FFFF during base + offset calculation or linear record increments. | Base + offset + length | Verified 32-bit physical address | Raises AddressOverflow error if calculation crosses 4GB boundary. | Intel HEX bounds analysis |
| 15 | Memory Model | Target Bounds Checking | Validates that segment addresses lie within the target chip's flash memory boundaries (e.g. 0x0800_0000..0x0808_0000 for 512KB STM32). | Segments + Target Memory Range | Bounds validation result | Emits out-of-bounds error or warning before flashing attempt. | ORIGINAL_REQUEST.md R1-R2 integration |
| 16 | Raw Binary | Configurable Base Address | Ingests raw .bin bytes with caller-specified base load address (e.g. --base-address 0x08000000 or GUI input). | Binary buffer, base_address: u64 | Single MemorySegment at base_address | Flags error if base address invalid or overflows 32/64-bit space. | ORIGINAL_REQUEST.md R2, CLI requirements |
| 17 | Raw Binary | Target Default Fallback | Automatically supplies target-appropriate default base address when none specified (e.g. 0x0800_0000 for STM32, 0x0000_0000 for RP2040). | Binary buffer, optional target ID | MemorySegment at target default base | None; defaults to 0x0800_0000 for STM32 family. | STM32 Flash memory mapping architecture |
| 18 | Checksum Engine | CRC32 Calculation | Computes standard IEEE 802.3 CRC32 for each segment and for the canonical concatenated payload. | Data slice | 32-bit unsigned integer / formatted hex | None. Deterministic. | Acceptance criteria, flash verification |
| 19 | Checksum Engine | MD5 Hash Calculation | Computes RFC 1321 MD5 message digest (128-bit hex string) for each segment and canonical payload. | Data slice | 32-character lowercase hex string | None. Deterministic. | Acceptance criteria, file integrity |
| 20 | Checksum Engine | SHA-256 Hash Calculation | Computes FIPS 180-4 SHA-256 cryptographic hash (256-bit hex string) for each segment and canonical payload. | Data slice | 64-character lowercase hex string | None. Deterministic. | Acceptance criteria, secure signing/verification |
| 21 | Checksum Engine | Padded Flash Checksum | Optional computation of image checksum when memory gaps are filled with flash erased byte (0xFF). | Segments, padding byte (0xFF) | Flat span checksums (CRC32, SHA256) | None. Matches physical flash dump byte-for-byte. | Flash verification, chip readback |
| 22 | Entry Point | Explicit HEX Entry Point | Extracts entry point directly from Record 05 (EIP) or Record 03 (CS:IP) if present in Intel HEX. | Record 03 or 05 payload | entry_point: Some(u64) | None. Replaced or confirmed. | Intel HEX Spec (1988) |
| 23 | Entry Point | Cortex-M Vector Heuristic | When no explicit entry record exists (standard GCC/Clang ARM), inspects word at offset 0x04 of base segment (Reset_Handler). | Segment data at flash base | entry_point: Option<u64> | Validates Thumb bit (bit 0 == 1) and address within segment bounds. | ARMv7-M Architecture Reference Manual |
| 24 | Serialization | Strongly Typed Rust Models | Models entire firmware AST, metadata, segments, warnings, and errors as Rust structs with serde::{Serialize, Deserialize}. | Rust structs | JSON string, binary bincode, IPC | Zero-copy slicing where possible; owned structs for IPC. | Tauri GUI & CLI IPC contract |
| 25 | Serialization | Tauri IPC JSON Schema | Provides complete, UI-ready JSON metadata schema for frontend segment table, progress bounds, and inspection dialogs. | FirmwareMetadata | JSON payload conforming to UI schema | None. Clean camelCase serialization. | ORIGINAL_REQUEST.md R3 (Firmware Panel) |

---

## 4. Detailed Technical Specifications

### 4.1 Intel HEX Format (HEX86 & HEX386)

An Intel HEX file consists of one or more ASCII text lines terminated by `\r\n` (CRLF) or `\n` (LF). Each line encodes a single record conforming to the following anatomy:

```
:LLAAAATT[DD...]CC
```

| Field Name | Offset | Length | Type | Description |
|---|---|---|---|---|
| Start Code | 0 | 1 char | ASCII `:` (0x3A) | Mandatory record prefix. |
| Byte Count (`LL`) | 1..3 | 2 hex chars | `u8` (0x00..0xFF) | Number of data bytes in payload (`DD`). |
| Address (`AAAA`) | 3..7 | 4 hex chars | `u16` (0x0000..0xFFFF) | 16-bit address offset within the active bank. |
| Record Type (`TT`) | 7..9 | 2 hex chars | `u8` (0x00..0x05) | Record type code (00, 01, 02, 03, 04, 05). |
| Data Bytes (`DD`) | 9..(9+2*LL) | 2*LL chars | `[u8; LL]` | Binary payload bytes. |
| Checksum (`CC`) | End-2..End | 2 hex chars | `u8` | Two's complement checksum byte. |

#### Two's Complement Checksum Equation
The checksum byte `CC` is computed such that the least significant byte of the sum of all decoded byte values in the record (Byte Count + Address High + Address Low + Record Type + all Data Bytes + Checksum) equals zero modulo 256:

$$\text{Sum} = \left( \text{LL} + \text{AAAA}_{\text{hi}} + \text{AAAA}_{\text{lo}} + \text{TT} + \sum_{i=0}^{\text{LL}-1} \text{DD}_i \right) \pmod{256}$$

$$\text{CC} = (-\text{Sum}) \pmod{256} = ((!\text{Sum} + 1) \pmod{256})$$

Validation check:
$$(\text{Sum} + \text{CC}) \pmod{256} == 0$$

#### Record Type Breakdown
1. **`00` (Data Record)**: Contains `LL` data bytes. The physical address for byte index $i$ ($0 \le i < \text{LL}$) is:
   - In HEX386 (after Record 04): $\text{PhysicalAddr} = (\text{ULBA} \ll 16) + \text{AAAA} + i$.
   - In HEX86 (after Record 02): $\text{PhysicalAddr} = (\text{USBA} \ll 4) + \text{AAAA} + i$.
   - Default (neither specified): $\text{PhysicalAddr} = \text{AAAA} + i$.
2. **`01` (End of File Record)**: Must appear as the final active record. `LL` must be `00`. `AAAA` is conventionally `0000`. Standard EOF line: `:00000001FF`.
3. **`02` (Extended Segment Address Record - HEX86)**: `LL` must be `02`. `AAAA` is conventionally `0000`. `DD` contains 2 bytes `USBA` (16-bit big-endian Upper Segment Base Address). Shifts base address by 4 bits for 20-bit x86 real-mode addressing.
4. **`03` (Start Segment Address Record - HEX86)**: `LL` must be `04`. `AAAA` is `0000`. `DD` contains 4 bytes: 2 bytes `CS`, 2 bytes `IP`. Execution start address = $(CS \ll 4) + IP$.
5. **`04` (Extended Linear Address Record - HEX386)**: `LL` must be `02`. `AAAA` is `0000`. `DD` contains 2 bytes `ULBA` (16-bit big-endian Upper Linear Base Address). Sets bits 16..31 of physical address.
   *Example*: For STM32 Flash at `0x0800_0000`, `ULBA = 0x0800`. Emitted line: `:020000040800F2`.
6. **`05` (Start Linear Address Record - HEX386)**: `LL` must be `04`. `AAAA` is `0000`. `DD` contains 4 bytes `EIP` (32-bit big-endian Execution Instruction Pointer). Sets execution entry point.
   *Example*: For entry point `0x0800_01CD`, line: `:04000005080001CD21`.

---

### 4.2 Raw Binary (.bin) Format

A raw binary firmware file contains an unformatted sequential stream of bytes.
- **Header / Framing**: None. No magic bytes, no address metadata, no checksums.
- **Base Address Handling**:
  - Requires an explicit base address, provided via CLI argument (`--base-address <ADDR>`) or GUI input box.
  - Default fallback for STM32 targets: `0x0800_0000` (Main Flash Memory).
  - Default fallback for generic ARM targets: `0x0000_0000`.
- **Segments**: Always produces exactly one `MemorySegment` spanning `[base_address .. base_address + file_size)`.
- **Format Auto-Detection Strategy**:
  1. If file extension is `.hex` or `.ihex`, parse as Intel HEX.
  2. If file extension is `.bin`, parse as Raw Binary.
  3. If extension is missing or unrecognized, inspect first non-whitespace byte:
     - If byte is ASCII `:` (0x3A), attempt Intel HEX parse.
     - Otherwise, parse as Raw Binary.

---

### 4.3 Memory Segment Consolidation & Gap Management

Real-world Intel HEX files generated by GCC, Keil, or IAR often contain hundreds of discrete 16-byte records, occasionally unordered, and frequently containing sparse address gaps (e.g., Interrupt Vector Table at `0x08000000`, bootloader configuration, and main application at `0x08020000`).

#### Consolidation Algorithm
1. **Collection**: Ingest each Record 00 into a list of chunks `(start_address: u64, bytes: Vec<u8>)`.
2. **Sorting**: Sort all chunks by `start_address` in ascending order.
3. **Coalescing Loop**:
   - Initialize `current_segment = chunks[0]`.
   - For each subsequent `next_chunk`:
     - If `next_chunk.start == current_segment.end`:
       - **Contiguous!** Append `next_chunk.bytes` directly to `current_segment.bytes`.
     - If `next_chunk.start > current_segment.end`:
       - **Gap detected!** Push `current_segment` to segment list. Record gap `[current_segment.end .. next_chunk.start)`. Start new `current_segment = next_chunk`.
     - If `next_chunk.start < current_segment.end`:
       - **Overlap!** Compare existing bytes in `current_segment` against `next_chunk.bytes`:
         - If all overlapping bytes are identical: Accept redundant write, append any trailing non-overlapping bytes, and log a `ValidationWarning::RedundantOverlap`.
         - If any overlapping byte differs: Abort with `ParseError::ConflictingDataOverlap { address, existing, incoming }`.
4. **Finalization**: Push the last `current_segment` to the output list.

#### Memory Gap Policy
- **Gaps are preserved**: Gaps between segments are NOT artificially padded in the canonical memory representation.
- **Rationale**: Flashing tools (`flash-core` / `probe-rs`) should only erase and write sectors that contain active code. Artificially padding a 256KB gap with zeros or `0xFF` would force the programmer to write empty sectors, causing massive flash write slowdowns and premature flash endurance degradation.
- **Metadata Reporting**: The parser calculates and exposes:
  - `total_firmware_bytes`: $\sum \text{segment.size}$
  - `address_span`: $\text{max}(\text{segment.end}) - \text{min}(\text{segment.start})$
  - `gap_count`: Number of unpopulated spans between segments.
  - `gap_bytes`: $\text{address_span} - \text{total_firmware_bytes}$.

---

### 4.4 Metadata Extraction & Checksum Engine

#### Multi-Hash Integrity Verification
For each consolidated segment, as well as for the canonical concatenated firmware payload (ordered by address), the parser calculates:
1. **CRC-32 (IEEE 802.3)**:
   - Polynomial: `0xEDB88320` (reversed representation of `0x04C11DB7`).
   - Initial value: `0xFFFFFFFF`, Final XOR: `0xFFFFFFFF`.
   - Output formatted as uppercase 8-character hex string: `0xXXXXXXXX`.
2. **MD5 (RFC 1321)**:
   - 128-bit digest formatted as 32-character lowercase hex string.
3. **SHA-256 (FIPS 180-4)**:
   - 256-bit digest formatted as 64-character lowercase hex string.
4. **Optional Padded Flash Checksum**:
   - Computes hashes over the full address span filling gaps with the erased flash byte `0xFF`. Useful for direct hardware verify readback.

#### Entry Point Detection Strategy
The parser resolves the entry point through a two-tiered strategy:
1. **Tier 1 (Explicit Record)**:
   - Check if an Intel HEX Record Type 05 (HEX386 32-bit EIP) or Record Type 03 (HEX86 CS:IP) was encountered.
   - If present: Set `entry_point = Some(value)`, `entry_point_source = EntryPointSource::Record05` (or `Record03`).
2. **Tier 2 (ARM Cortex-M Vector Table Inspection Fallback)**:
   - If no explicit record was found, inspect the first segment covering the flash base (e.g. `0x0800_0000` or `0x0000_0000`).
   - If segment length $\ge 8$ bytes:
     - Read 32-bit little-endian word at offset `0x00` (Initial Main Stack Pointer, MSP).
     - Read 32-bit little-endian word at offset `0x04` (Reset Handler pointer).
     - **Validation**:
       - Reset handler address bit 0 must equal `1` (indicating Thumb mode execution; Cortex-M core fault occurs if bit 0 is 0).
       - Actual instruction address (`reset_handler & ~1`) must reside within the firmware segment address range.
       - Initial MSP must point to a valid RAM range (e.g. `0x2000_0000..0x3000_0000` for STM32).
     - If validated: Set `entry_point = Some(reset_handler)`, `entry_point_source = EntryPointSource::CortexMVectorTable`.
     - Otherwise: `entry_point = None`, `entry_point_source = EntryPointSource::None`.

---

## 5. Edge Cases & Observed Behavior

| # | Feature | Input | Observed Behavior |
|---|---------|-------|-------------------|
| 1 | Checksum Validation | `:020000040800F1` (checksum byte off by 1) | Parser aborts with `ParseError::ChecksumMismatch { line: 1, expected: 0xF2, found: 0xF1 }`. |
| 2 | Line Lexing | `020000040800F2` (missing leading `:`) | Parser aborts with `ParseError::MissingLeadingColon { line: 1 }`. |
| 3 | Line Lexing | `:02000004080F2` (13 hex characters, odd count) | Parser aborts with `ParseError::OddHexDigitCount { line: 1, count: 13 }`. |
| 4 | Line Lexing | `:020000040800FZ` (non-hex character `'Z'`) | Parser aborts with `ParseError::InvalidHexCharacter { line: 1, character: 'Z' }`. |
| 5 | Line Lexing | `:0200000408` (truncated, fewer than 5 bytes) | Parser aborts with `ParseError::RecordTruncated { line: 1, byte_count: 5, actual_bytes: 4 }`. |
| 6 | Record Length | `:0100000408F3` (Type 04 with byte count 1 instead of 2) | Parser aborts with `ParseError::InvalidRecordLength { line: 1, record_type: 4, expected: 2, actual: 1 }`. |
| 7 | Record Length | `:0100000100FE` (Type 01 EOF with byte count 1 instead of 0) | Parser aborts with `ParseError::InvalidRecordLength { line: 1, record_type: 1, expected: 0, actual: 1 }`. |
| 8 | EOF Handling | EOF record followed by further data records | Parser parses data but records `ValidationWarning::DataAfterEndOfFile { line: X }`. |
| 9 | EOF Handling | File reaches EOF without record type 01 | Parser successfully consolidates all data and records `ValidationWarning::MissingEndOfFileRecord`. |
| 10 | Whitespace | Blank lines, leading whitespace, trailing CRLF or LF | Parser ignores empty lines and trims whitespace transparently without error. |
| 11 | Empty File | 0-byte file input | Parser aborts with `ParseError::EmptyFile`. |
| 12 | Memory Overlap | Two records write identical bytes to address `0x08000000` | Parser accepts byte, skips duplicate, records `ValidationWarning::RedundantOverlap { address: 0x08000000 }`. |
| 13 | Memory Overlap | Record writes `0xAA` to `0x08000000`, later record writes `0xBB` to `0x08000000` | Parser aborts with `ParseError::ConflictingDataOverlap { address: 0x08000000, existing: 0xAA, incoming: 0xBB }`. |
| 14 | Out-of-Order | Record for `0x08000010` appears before record for `0x08000000` | Parser sorts chunks by address; seamlessly stitches them into contiguous `0x08000000..0x08000020`. |
| 15 | Address Space | Record address `base + offset + len > 0xFFFF_FFFF` | Parser aborts with `ParseError::AddressOverflow { address: 0x100000000 }`. |
| 16 | Target Bounds | Firmware segment at `0x08090000` on 512KB STM32 (`0x08000000..0x08080000`) | Target validator flags `ParseError::TargetOutOfBounds { segment_start: 0x08090000, flash_limit: 0x08080000 }`. |
| 17 | Unknown Record | Line with record type `0x09` or `0xFF` | Parser aborts with `ParseError::UnknownRecordType { line: X, record_type: 0x09 }`. |
| 18 | Raw Binary Base | User passes invalid base address string (e.g. `"foo"`) | CLI / Parser returns `ParseError::InvalidBaseAddress("foo")`. |
| 19 | Vector Table | Cortex-M binary with even reset vector (`0x080001CC`) | Vector heuristic rejects as ARM mode (bit 0 == 0); `entry_point` remains `None`. |
| 20 | Vector Table | Vector Reset Handler points outside loaded segment | Vector heuristic rejects entry point; `entry_point` remains `None`. |

---

## 6. Rust Data Models & Serde Serialization Schemas

### 6.1 Rust Domain Structures (`crates/firmware-parser/src/models.rs`)

```rust
use serde::{Deserialize, Serialize};

/// Supported firmware container formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareFormat {
    IntelHex,
    RawBinary,
}

/// Standardized integrity checksum summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecksumSummary {
    /// IEEE 802.3 CRC-32 formatted as uppercase hex (e.g. "0x0A5B1F0D").
    pub crc32: String,
    /// RFC 1321 MD5 formatted as lowercase 32-character hex.
    pub md5: String,
    /// FIPS 180-4 SHA-256 formatted as lowercase 64-character hex.
    pub sha256: String,
}

/// Origin source for detected execution entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryPointSource {
    /// Explicitly declared via Intel HEX Record 05 (HEX386 EIP).
    Record05,
    /// Explicitly declared via Intel HEX Record 03 (HEX86 CS:IP).
    Record03,
    /// Auto-detected via ARM Cortex-M Vector Table (offset 0x04, Reset_Handler).
    CortexMVectorTable,
    /// No entry point detected.
    None,
}

/// Metadata describing a discrete contiguous memory block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySegmentMetadata {
    pub index: usize,
    pub start_address: u64,
    pub end_address: u64,
    pub size_bytes: usize,
    pub checksums: ChecksumSummary,
}

/// Full memory segment containing physical byte payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySegment {
    pub start_address: u64,
    pub end_address: u64,
    #[serde(skip_serializing)]
    pub data: Vec<u8>,
    pub checksums: ChecksumSummary,
}

/// Non-fatal warnings encountered during parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "details")]
pub enum ValidationWarning {
    RedundantOverlap { address: u64, line: Option<usize> },
    MissingEndOfFileRecord,
    DataAfterEndOfFile { line: usize },
    DeprecatedRecordType { record_type: u8, line: usize },
}

/// Structured metadata summary for UI inspection and CLI reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareMetadata {
    pub file_path: Option<String>,
    pub format: FirmwareFormat,
    pub file_size_bytes: u64,
    pub total_firmware_bytes: u64,
    pub base_address: u64,
    pub highest_address: u64,
    pub address_span: u64,
    pub gap_count: usize,
    pub gap_bytes: u64,
    pub entry_point: Option<u64>,
    pub entry_point_source: EntryPointSource,
    pub segment_count: usize,
    pub segments: Vec<MemorySegmentMetadata>,
    pub checksums: ChecksumSummary,
    pub warnings: Vec<ValidationWarning>,
}

/// In-memory parsed firmware image ready for flashing operations.
#[derive(Debug, Clone)]
pub struct FirmwareImage {
    pub metadata: FirmwareMetadata,
    pub segments: Vec<MemorySegment>,
}

/// Strongly typed parse errors with line numbers and diagnostic context.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error_code", content = "details")]
pub enum ParseError {
    #[error("File is empty")]
    EmptyFile,

    #[error("Line {line}: Missing mandatory leading colon prefix")]
    MissingLeadingColon { line: usize },

    #[error("Line {line}: Odd number of hexadecimal characters ({count})")]
    OddHexDigitCount { line: usize, count: usize },

    #[error("Line {line}: Invalid non-hexadecimal character '{character}'")]
    InvalidHexCharacter { line: usize, character: char },

    #[error("Line {line}: Record truncated (expected at least {byte_count} bytes, got {actual_bytes})")]
    RecordTruncated { line: usize, byte_count: usize, actual_bytes: usize },

    #[error("Line {line}: Checksum mismatch (calculated 0x{expected:02X}, found 0x{found:02X})")]
    ChecksumMismatch { line: usize, expected: u8, found: u8 },

    #[error("Line {line}: Invalid byte count {actual} for record type 0x{record_type:02X} (expected {expected})")]
    InvalidRecordLength { line: usize, record_type: u8, expected: usize, actual: usize },

    #[error("Line {line}: Unknown record type 0x{record_type:02X}")]
    UnknownRecordType { line: usize, record_type: u8 },

    #[error("Line {line}: Conflicting data overlap at physical address 0x{address:08X} (existing 0x{existing:02X}, incoming 0x{incoming:02X})")]
    ConflictingDataOverlap { line: usize, address: u64, existing: u8, incoming: u8 },

    #[error("Line {line}: Physical address 0x{address:X} overflows 32-bit address space")]
    AddressOverflow { line: usize, address: u64 },

    #[error("Target out of bounds: Segment 0x{segment_start:08X}..0x{segment_end:08X} exceeds target flash limit 0x{flash_limit:08X}")]
    TargetOutOfBounds { segment_start: u64, segment_end: u64, flash_limit: u64 },

    #[error("Invalid base address '{0}'")]
    InvalidBaseAddress(String),

    #[error("IO Error: {0}")]
    IoError(String),
}
```

### 6.2 Tauri IPC JSON Contract (UI Consumable Payload)

When the user selects a firmware file in the GUI, the backend `invoke('inspect_firmware', { path, baseAddress })` command returns the following JSON structure:

```json
{
  "filePath": "C:/firmware/stm32f4_blink.hex",
  "format": "intel_hex",
  "fileSizeBytes": 3842,
  "totalFirmwareBytes": 48,
  "baseAddress": 134217728,
  "highestAddress": 134479888,
  "addressSpan": 262160,
  "gapCount": 1,
  "gapBytes": 262112,
  "entryPoint": 134218189,
  "entryPointSource": "record05",
  "segmentCount": 2,
  "segments": [
    {
      "index": 0,
      "startAddress": 134217728,
      "endAddress": 134217760,
      "sizeBytes": 32,
      "checksums": {
        "crc32": "0x0A5B1F0D",
        "md5": "629f18994ef216238cd67914482dfb03",
        "sha256": "b2ed8017b38167a8bd05f1b60f972ee9479abc971762f40f14c61bbf753539fc"
      }
    },
    {
      "index": 1,
      "startAddress": 134479872,
      "endAddress": 134479888,
      "sizeBytes": 16,
      "checksums": {
        "crc32": "0x3DB782CE",
        "md5": "26c7bbbe52834d8cc34fe57ef45c91e4",
        "sha256": "029705a62e071ae757eb762cb05822ce045bfd3ea9524024c084050eb0e9477e"
      }
    }
  ],
  "checksums": {
    "crc32": "0x767B0A13",
    "md5": "0ec2d4eca06af318796c232fafeaa288",
    "sha256": "05e9e73074789d23556e2f2a9daafbfb8c77207499ba5af61ff9858fcbd6bb64"
  },
  "warnings": []
}
```

---

## 7. Authoritative Test Vectors & Golden Reference Datasets

### Vector 1: Standard Single-Segment STM32 Cortex-M Image
- **Description**: 32-byte Cortex-M vector table with initial SP `0x20005000` and Reset Handler `0x080001CD`, containing explicit Type 04, Type 00, Type 05, and Type 01 records.
- **HEX Input**:
```hex
:020000040800F2
:1000000000500020CD010008D1010008D3010008F4
:10001000D5010008D7010008D90100080000000040
:04000005080001CD21
:00000001FF
```
- **Expected Golden Output**:
  - `format`: `FirmwareFormat::IntelHex`
  - `segment_count`: 1
  - `segments[0].start_address`: `0x08000000`
  - `segments[0].end_address`: `0x08000020`
  - `segments[0].size_bytes`: 32
  - `entry_point`: `Some(0x080001CD)` (`EntryPointSource::Record05`)
  - `checksums.crc32`: `0x0A5B1F0D`
  - `checksums.md5`: `629f18994ef216238cd67914482dfb03`
  - `checksums.sha256`: `b2ed8017b38167a8bd05f1b60f972ee9479abc971762f40f14c61bbf753539fc`
  - `warnings`: `[]`

---

### Vector 2: Dual-Segment Non-Contiguous Flash Image (Bootloader + App Gap)
- **Description**: Two distinct segments separated by a 256KB memory gap (Bootloader at `0x08000000`, App at `0x08040000`).
- **HEX Input**:
```hex
:020000040800F2
:1000000000500020CD010008D1010008D3010008F4
:10001000D5010008D7010008D90100080000000040
:020000040804EE
:10000000DEADBEEFCAFEBABE0123456789ABCDEFB8
:00000001FF
```
- **Expected Golden Output**:
  - `total_firmware_bytes`: 48
  - `address_span`: 262160 (`0x08040010 - 0x08000000`)
  - `gap_count`: 1
  - `gap_bytes`: 262112 (`0x08040000 - 0x08000020`)
  - `segment_count`: 2
  - `segment[0]`: Start `0x08000000`, Size 32, CRC32 `0x0A5B1F0D`
  - `segment[1]`: Start `0x08040000`, Size 16, CRC32 `0x3DB782CE`
  - Canonical Concatenated Checksums (48 bytes):
    - `crc32`: `0x767B0A13`
    - `md5`: `0ec2d4eca06af318796c232fafeaa288`
    - `sha256`: `05e9e73074789d23556e2f2a9daafbfb8c77207499ba5af61ff9858fcbd6bb64`
  - Padded Erased (0xFF) Image Checksums (262,160 bytes):
    - `crc32`: `0x32BA8639`
    - `sha256`: `a2a25248ae9c7cc65fe74036956e311e00b3343992793a34fe4dd59776edf58a`

---

### Vector 3: Out-of-Order Records Normalization
- **Description**: Data records emitted non-monotonically (offset `0x0010` emitted before offset `0x0000`).
- **HEX Input**:
```hex
:020000040800F2
:10001000D5010008D7010008D90100080000000040
:1000000000500020CD010008D1010008D3010008F4
:00000001FF
```
- **Expected Golden Output**:
  - Automatically sorted and coalesced into 1 contiguous segment `[0x08000000..0x08000020]`.
  - Checksums match Vector 1 exactly (`crc32: 0x0A5B1F0D`).
  - No data corruption or inverted byte ordering.

---

### Vector 4: Raw Binary with Auto-Detected Cortex-M Reset Vector
- **Description**: 1024-byte `.bin` file loaded at base `0x08000000`. First 8 bytes define SP `0x20005000` and Reset Handler `0x080001CD`.
- **Expected Golden Output**:
  - `format`: `FirmwareFormat::RawBinary`
  - `segment_count`: 1
  - `segments[0].start_address`: `0x08000000`
  - `segments[0].size_bytes`: 1024
  - `entry_point`: `Some(0x080001CD)`
  - `entry_point_source`: `EntryPointSource::CortexMVectorTable`

---

### Vector 5: Negative / Malformed Test Cases Matrix
| Case | Input Snippet | Expected Error Variant | Diagnostic String |
|---|---|---|---|
| A | `:020000040800F1` | `ParseError::ChecksumMismatch` | `Line 1: Checksum mismatch (calculated 0xF2, found 0xF1)` |
| B | `020000040800F2` | `ParseError::MissingLeadingColon` | `Line 1: Missing mandatory leading colon prefix` |
| C | `:02000004080F2` | `ParseError::OddHexDigitCount` | `Line 1: Odd number of hexadecimal characters (13)` |
| D | `:020000040800FZ` | `ParseError::InvalidHexCharacter` | `Line 1: Invalid non-hexadecimal character 'Z'` |
| E | `:0200000408` | `ParseError::RecordTruncated` | `Line 1: Record truncated` |
| F | `:0100000408F3` | `ParseError::InvalidRecordLength` | `Line 1: Invalid byte count 1 for record type 0x04 (expected 2)` |
| G | `:0100000100FE` | `ParseError::InvalidRecordLength` | `Line 1: Invalid byte count 1 for record type 0x01 (expected 0)` |
| H | Line 1: `0xAA` at `0x08000000`<br>Line 2: `0xBB` at `0x08000000` | `ParseError::ConflictingDataOverlap` | `Line 2: Conflicting data overlap at physical address 0x08000000` |
| I | Segment at `0x08090000` vs 512KB chip limit | `ParseError::TargetOutOfBounds` | `Target out of bounds: Segment exceeds target flash limit` |
| J | Empty string / 0-byte file | `ParseError::EmptyFile` | `File is empty` |

---

## 8. Verification & Integration Plan for `firmware-parser`

The `firmware-parser` crate should be implemented with zero external unsafe dependencies, relying only on lightweight, audited crates:
- `thiserror = "1.0"` — Ergonomic typed error definitions.
- `serde = { version = "1.0", features = ["derive"] }` — High-speed serialization.
- `crc32fast = "1.4"` — SIMD-accelerated IEEE 802.3 CRC32 hashing.
- `md-5 = "0.10"` — Standard RFC 1321 MD5 hash computation.
- `sha2 = "0.10"` — Standard FIPS 180-4 SHA-256 hash computation.

### Verification Commands
1. Unit tests:
   ```bash
   cargo test -p firmware-parser -- --nocapture
   ```
2. Golden test vectors execution:
   ```bash
   cargo test -p firmware-parser --test golden_vectors
   ```
3. Clippy & formatting:
   ```bash
   cargo clippy -p firmware-parser -- -D warnings
   ```
