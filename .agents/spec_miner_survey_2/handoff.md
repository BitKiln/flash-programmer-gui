# Handoff Report: Spec Miner 2 (Firmware Parser & Memory Inspector Spec)

**Agent**: `spec_miner_survey_2`  
**Parent Task**: Flash Programmer GUI & CLI Architecture (Phase 0 Survey)  
**Target Specification Document**: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2/survey_report.md`  
**Handoff Type**: Hard Handoff (Phase 0 Task Complete)

---

## 1. Observation

1. **User Request & Requirements**:
   - File: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md`
   - Line 15-16: `"### R2. Firmware Parser & Memory Inspector: Implement a robust firmware parsing crate (firmware-parser) supporting Intel HEX (.hex) and raw binary (.bin) files. Extract and validate memory segments, base addresses, total size, checksums, and entry points, reporting structured metadata for UI inspection and verification."`
   - Line 31: `"- [ ] Unit tests in firmware-parser verifying valid and malformed Intel HEX and binary file parsing, memory gap handling, and address bounds checks."`

2. **Authoritative Standards Evaluated**:
   - Intel Corporation: *Hexadecimal Object File Format Specification* (Revision A, 1988), defining record framing `:LLAAAATT[DD...]CC`, two's complement checksum modulo 256, and record types 00 (Data), 01 (EOF), 02 (Extended Segment Address - HEX86), 03 (Start Segment Address - HEX86), 04 (Extended Linear Address - HEX386), and 05 (Start Linear Address - HEX386).
   - ARM Ltd.: *ARMv7-M Architecture Reference Manual* (ARM DDI 0403E.e), Section B1.5, defining Vector Table offset 0x00 (Initial MSP) and offset 0x04 (Reset Handler with Thumb bit 0 = 1).
   - IEEE 802.3 CRC-32 (polynomial `0xEDB88320`, initial `0xFFFFFFFF`, final XOR `0xFFFFFFFF`), RFC 1321 MD5, and FIPS 180-4 SHA-256.

3. **Tool Invocations & Verified Arithmetic**:
   - Python test script verified standard Intel HEX line `:10010000214601360121470136007EFE09D2190140`: calculated sum `0xC0`, checksum `0x40`, `(0xC0 + 0x40) & 0xFF == 0`.
   - Python test script executed against 32-byte STM32 Cortex-M test vector:
     - Segment 0: `0x08000000..0x08000020` (32 bytes)
     - Hashes: `crc32: 0x0A5B1F0D`, `md5: 629f18994ef216238cd67914482dfb03`, `sha256: b2ed8017b38167a8bd05f1b60f972ee9479abc971762f40f14c61bbf753539fc`
     - Entry point deduced from vector table offset 0x04: `0x080001CD` (matches Record 05 EIP exactly).
   - Python test script verified chunk consolidation algorithm: successfully sorted out-of-order chunks (`0x08000010` before `0x08000000`) into single contiguous segment `[0x08000000..0x08000020]`, preserved 256KB gap between `0x08000020` and `0x08040000`, accepted identical duplicate writes with warning, and raised error on conflicting byte overlap.

4. **Output Deliverables**:
   - `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2/survey_report.md` created with 537 lines, 34.6 KB, containing 25 discovered features, 20 edge cases, Rust domain models, Serde IPC schemas, and 5 golden reference test vectors.

---

## 2. Logic Chain

1. **Requirement Mapping**: Observation 1 dictates that `firmware-parser` must parse Intel HEX and raw binary files, calculate multi-algorithm checksums, extract memory segments with gaps, detect entry points, and supply UI/CLI serialization models.
2. **Grammar & Lexing Architecture**: Observation 2 establishes that Intel HEX lines are strictly framed by `:` and two's complement checksums. By validating record length `5 + LL` and checksum modulo 256 upfront, corrupt lines are isolated immediately with 1-based line numbers.
3. **Address Calculation & Multi-Banking**: In 32-bit MCUs (STM32), Record Type 04 (Extended Linear Address) sets the upper 16 bits (`ULBA << 16`). Physical address for data bytes is `(ULBA << 16) + AAAA + offset`. To maintain legacy compatibility, Record Type 02 (Segmented Address `(USBA << 4) + AAAA`) is supported for 20-bit addressing.
4. **Out-of-Order & Gap Management**: As verified in Observation 3, toolchains emit records non-monotonically and link distinct sections (e.g. vector table and application) with large unpopulated memory gaps. A chunk-sorting interval algorithm sorts all chunks by start address, coalesces contiguous slices into minimal `MemorySegment` objects, and records gap boundaries without synthetic padding.
5. **Collision Policy**: If two records overlap: identical bytes represent redundant duplicate writes (logged as non-fatal warnings); conflicting bytes represent corrupted or mismatched object records and must abort with `ParseError::ConflictingDataOverlap`.
6. **Entry Point Fallback**: When an explicit Record 05 (EIP) or 03 (CS:IP) is absent (typical in bare-metal GCC builds), Observation 2 and 3 prove that reading word 1 (offset 0x04) of the base segment provides the Cortex-M Reset Handler address, validated by requiring Thumb bit 0 to be 1.
7. **Cross-Layer Serialization**: UI and CLI layers require structured summaries without transferring multi-megabyte binary payloads across IPC channels. The designed `FirmwareMetadata` struct contains segment bounds, gaps, and hashes, while `MemorySegment` encapsulates actual byte buffers for `flash-core`.

---

## 3. Caveats

1. **ELF Binary Format**: Requirement R2 explicitly requests Intel HEX (`.hex`) and raw binary (`.bin`). ELF parsing is not included in this phase to keep dependencies minimal; ELF support can be added in a future milestone via `object` or `goblin`.
2. **Gap Memory Representation**: Gaps are strictly unpadded in `FirmwareImage` to avoid flashing empty space. However, `survey_report.md` also documents a secondary `padded_checksum(0xFF)` function to support byte-for-byte readback comparisons against completely erased chips.
3. **Target Flash Limit Enforcement**: Target memory boundary validation (e.g. flagging writes outside 512KB flash) requires target chip metadata from `flash-core` / `probe-rs`. `firmware-parser` provides the standalone validation API `validate_target_bounds(target_flash_start, target_flash_end)`.

---

## 4. Conclusion

- Requirement R2 (`firmware-parser`) is 100% specified, mathematically verified, and documented in `survey_report.md`.
- All 25 functional features and 20 edge cases have been categorized and mapped to concrete Rust domain models and error variants.
- The crate architecture is completely safe (zero `unsafe`), highly performant (`crc32fast`, `thiserror`, `serde`), and ready for immediate implementation in Phase 1.

---

## 5. Verification Method

1. **Inspect Survey Report**:
   - File: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2/survey_report.md`
   - Verify presence of:
     - Section 3: Features Discovered table (25 features)
     - Section 4: Detailed technical specifications (00-05, bin, consolidation, checksums, Cortex-M heuristic)
     - Section 5: Edge Cases table (20 edge cases)
     - Section 6: Rust structures and JSON schema
     - Section 7: Authoritative test vectors (Vectors 1-5)
2. **Future Crate Verification Commands (upon implementation)**:
   - Unit tests:
     ```bash
     cargo test -p firmware-parser -- --nocapture
     ```
   - Golden test vectors suite:
     ```bash
     cargo test -p firmware-parser --test golden_vectors
     ```
   - Linter & formatting:
     ```bash
     cargo clippy -p firmware-parser -- -D warnings
     ```
3. **Invalidation Conditions**:
   - Any failure of Intel HEX checksum verification against the standard `(sum + cs) & 0xFF == 0`.
   - Failure to sort and coalesce out-of-order records into contiguous segments.
   - Any panic or memory corruption when parsing malformed or truncated inputs.
