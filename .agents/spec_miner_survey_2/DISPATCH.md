# Dispatch for Spec Miner 2: Firmware Parser & Memory Inspector Spec

**Role**: Firmware Spec Miner
**Working Directory**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2
**Original Request File**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md

## Assignment
1. Read `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md`.
2. Analyze requirement R2 (`firmware-parser`) and related acceptance criteria.
3. Spec out the Intel HEX format parser specifications (record types 00 Data, 01 End of File, 02 Extended Segment Address, 03 Start Segment Address, 04 Extended Linear Address, 05 Start Linear Address), two's complement checksum validation, line parsing, memory segment consolidation, handling of gaps/non-contiguous blocks, and address bounds checking.
4. Spec out raw binary (`.bin`) parsing with configurable or default base load address (e.g. 0x08000000 for STM32).
5. Define metadata extraction specifications: memory segments (start address, length, data bytes), total size, calculated checksums (CRC32, MD5, SHA256), entry point detection, and structured models serializable to JSON for UI/CLI consumption.
6. Detail comprehensive edge cases, malformed record types, corrupted checksums, out-of-order records, overlapping segments, and boundary limits.
7. Write your findings to `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2/survey_report.md` and deliver a handoff in `handoff.md`.

## 2026-09-10T19:26:14Z
You are Spec Miner 2: Firmware Parser & Memory Inspector Spec.
Your assigned working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2
Read c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md and c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2/DISPATCH.md.
Investigate requirement R2 (firmware-parser) and acceptance criteria:
- Intel HEX format specification (record types 00, 01, 02, 03, 04, 05, two's complement checksum, address calculation, memory gap handling, out-of-order records)
- Raw binary format (.bin) specification with configurable base address
- Metadata extraction: memory segments (address, size, data), total bytes, checksums (CRC32, MD5, SHA256), entry point
- Edge cases, corrupted files, invalid checksums, address overflow, and bounds checking
- Structured serializable types for UI/CLI consumption and test vectors.
Write your full findings to c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2/survey_report.md and your handoff to c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2/handoff.md.
Send a message when finished.

