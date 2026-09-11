# Dispatch for Worker M1: Firmware Parser Implementation

**Role**: Firmware Parser Worker
**Working Directory**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m1
**Original Request File**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md
**Project Architecture**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md
**Specification Source**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2/survey_report.md

## Mandatory Integrity Warning
> DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A teamwork_preview_auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

## Scope of Work & Exclusive File Ownership
You own:
- `Cargo.toml` (root workspace manifest with `[workspace]` and `resolver = "2"`, declaring members `crates/firmware-parser`, `crates/flash-core`, `crates/flashgui-cli`, `src-tauri` or adding them as created)
- `crates/firmware-parser/**`

## Requirements
Implement the complete `firmware-parser` crate:
1. **Intel HEX parser (`hex.rs`)**:
   - Parse standard record framing `:LLAAAATT[DD...]CC` with 1-based line reporting.
   - Two's complement checksum validation modulo 256: `(sum + cs) & 0xFF == 0`.
   - Record types: 00 (Data), 01 (EOF), 02 (Extended Segment Address, `(USBA << 4) + AAAA`), 03 (Start Segment Address CS:IP), 04 (Extended Linear Address, `(ULBA << 16) + AAAA`), 05 (Start Linear Address 32-bit EIP).
   - Support out-of-order records by sorting address chunks.
   - Preserve memory gaps without synthetic padding.
   - Collision handling: identical bytes tolerated/warned; conflicting overlapping bytes raise error.
2. **Raw Binary parser (`bin.rs`)**:
   - Parse binary slices with configurable base address (default 0x08000000).
3. **Memory Segments & Consolidation (`segment.rs`)**:
   - Interval sorting and contiguous chunk merging into `MemorySegment { start_address, data }`.
   - Extraction of `MemoryGap { start_address, end_address, size }`.
4. **Multi-algorithm Checksums (`checksum.rs`)**:
   - IEEE 802.3 CRC32 (`crc32fast`).
   - RFC 1321 MD5 (`md-5`).
   - FIPS 180-4 SHA-256 (`sha2`).
5. **Metadata & Architecture Inspection (`metadata.rs`)**:
   - Cortex-M Reset Handler detection from vector table offset 0x04 (ensuring Thumb bit 0 = 1).
   - Target flash boundary validation API (`validate_target_bounds`).
   - `FirmwareMetadata` and `FirmwareImage` models with `serde::Serialize` and `serde::Deserialize`.
6. **Comprehensive Test Suite**:
   - Unit tests covering all valid/invalid HEX formats, binary loading, corrupted checksums, non-hex characters, truncated lines, out-of-order records, memory gaps, overlaps, and Cortex-M vector table detection.
   - Include the 5 golden reference test vectors from `spec_miner_survey_2/survey_report.md`.
   - Ensure `cargo test -p firmware-parser` passes with 100% success and zero errors.

## Verification & Handoff
Execute `cargo test -p firmware-parser` and document full outputs and passing test counts in `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m1/handoff.md`.

## 2026-09-10T19:34:24Z
You are Worker M1: Firmware Parser Implementation.
Working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m1
Project Root: c:/web_applications/open-source/embedded/flash_programmer_gui
Read c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md, c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md, c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2/survey_report.md, and c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m1/DISPATCH.md.

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

Scope:
Create the root Cargo.toml workspace file and implement crates/firmware-parser (hex.rs, bin.rs, segment.rs, checksum.rs, metadata.rs, error.rs, lib.rs) and comprehensive unit tests in tests/golden_vectors.rs and inline unit tests.
Run cargo test -p firmware-parser and cargo clippy -p firmware-parser.
Ensure 100% of tests pass.
Write your completion report and test verification output to c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m1/handoff.md.
Send a message when finished.

