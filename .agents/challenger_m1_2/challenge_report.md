# Empirical Challenge Report — Milestone M1: Firmware Parser

**Challenger**: Challenger 2 (Parser Mathematical & Architectural Correctness)  
**Date**: 2026-09-10T19:47:00Z  
**Target Crate**: `crates/firmware-parser`  
**Verdict**: **`CHALLENGE_FAILED`** (Bugs Exposed at 4GB Rollover Boundary & Workspace Test Regression)

---

## 1. Challenge Summary

**Overall Risk Assessment**: **HIGH**

While `firmware-parser` demonstrates outstanding mathematical and algorithmic compliance with authoritative specifications across normal operational ranges (Intel HEX 1988 record types 00..05, IEEE 802.3 CRC-32, RFC 1321 MD5, FIPS 180-4 SHA-256, and ARMv7-M Cortex-M vector table heuristics), empirical stress-testing against 32-bit address space boundary conditions exposed severe architectural flaws:
1. **Silent 32-bit Integer Rollover to `0x00000000` in Raw Binary Parsing (`bin.rs`)**: When raw binary firmware reaches the 4GB ceiling (`base_address + len == 0x1_0000_0000`), casting `end_addr_64 as u32` truncates to `0x00000000`. This sets `segment_meta.end_address = 0` and `metadata.highest_address = 0` while `base_address` is `0xFFFFFFF0`, violating the core architectural invariant `highest_address >= base_address`.
2. **Inconsistent Segment End Address Representations**: `MemorySegment::end_address()` in `metadata.rs` uses `saturating_add` (returning `0xFFFFFFFF`), while `SegmentMetadata::end_address` in `bin.rs` wraps to `0x00000000`.
3. **Address Span Off-by-One Underflow in Intel HEX Parsing (`hex.rs`)**: At the 4GB ceiling, `saturating_add` clamps `highest_address` to `0xFFFFFFFF`. The computation `(highest_address - base_address) as u64` calculates 15 bytes instead of 16 bytes for a 16-byte payload.
4. **Workspace Test Suite Regression**: Automated testing via `cargo test -p firmware-parser` currently fails with 2 test failures in `tests/adversarial_stress.rs` (`test_bin_boundary_saturation` and `test_hex_boundary_4gb_span`).

---

## 2. Standards Verification Matrix

| Standard / Specification | Clause / Requirement | Method / Oracle | Empirical Result | Status |
|---|---|---|---|---|
| **Intel HEX 1988 (Rev A)** | Record Type 00 (Data Record) RECLEN 1..255 | Challenger Oracle Suite 2 | Parsed variable lengths, full payload verified | **PASS** |
| **Intel HEX 1988 (Rev A)** | Record Type 01 (EOF Record) RECLEN == 0 | Challenger Oracle Suite 2 | Enforced RECLEN == 0; non-zero RECLEN rejected | **PASS** |
| **Intel HEX 1988 (Rev A)** | Record Type 02 (USBA) `(USBA << 4) + offset` | Challenger Oracle Suite 2 | Segment base 0x24000 + 0x100 = 0x24100 verified | **PASS** |
| **Intel HEX 1988 (Rev A)** | Record Type 03 (CS:IP) `(CS << 4) + IP` | Challenger Oracle Suite 2 | Entry point (0x1234 << 4) + 0x5678 = 0x179B8 verified | **PASS** |
| **Intel HEX 1988 (Rev A)** | Record Type 04 (ULBA) `(ULBA << 16) + offset` | Challenger Oracle Suite 2 | Linear base 0x08000000 + 0x100 = 0x08000100 verified | **PASS** |
| **Intel HEX 1988 (Rev A)** | Record Type 05 (EIP) 32-bit execution address | Challenger Oracle Suite 2 | Entry point 0x08000401 verified | **PASS** |
| **Intel HEX 1988 (Rev A)** | Two's Complement Checksum `(sum + cs) & 0xFF == 0` | Python independent oracle & Suite 2 | 1-bit checksum corruptions rejected with exact error | **PASS** |
| **IEEE 802.3** | CRC-32 (poly 0x04C11DB7 reflected / 0xEDB88320) | Python `zlib.crc32` & standard vector "123456789" | Matched 0xCBF43926 and golden vectors bit-for-bit | **PASS** |
| **RFC 1321** | MD5 Message-Digest Algorithm | Python `hashlib.md5` & RFC 1321 standard vectors | Matched all RFC 1321 test vectors bit-for-bit | **PASS** |
| **FIPS 180-4** | Secure Hash Standard SHA-256 | Python `hashlib.sha256` & NIST standard vectors | Matched all SHA-256 test vectors bit-for-bit | **PASS** |
| **ARMv7-M (DDI 0403E.e)** | Reset Handler Thumb mode bit (bit 0 == 1) | Challenger Oracle Suite 5 | Even address (ARM mode) rejected as entry point | **PASS** |
| **ARMv7-M (DDI 0403E.e)** | MSP Alignment (bits [1:0] == 00, msp != 0) | Challenger Oracle Suite 5 | Unaligned MSP (0x20005001..3) and zero rejected | **PASS** |
| **ARMv7-M (DDI 0403E.e)** | Reset vector bounds within flash memory | Challenger Oracle Suite 5 | Out-of-bounds reset handler rejected | **PASS** |
| **Entry Point Precedence** | Explicit Record 05 / 03 overrides heuristic | Challenger Oracle Suite 5 | Record 05 took precedence over Cortex-M vector table | **PASS** |
| **Overlap Resolution** | Identical data emits warning; conflicting data errors | Challenger Oracle Suite 4 | Identical: `ValidationWarning::RedundantOverlap`; Conflicting: `ParseError::ConflictingDataOverlap` | **PASS** |
| **Sparse Memory Gaps** | Multi-segment gap preservation without synthetic pad | Python flat array & Suite 4 | Gaps preserved; padded checksum matches flat buffer | **PASS** |
| **4GB Address Boundary** | Physical address ceiling rollover (`0x1_0000_0000`) | `tests/adversarial_stress.rs` & Oracle Suite 3 | `bin.rs` wraps to 0; `hex.rs` undercounts address span | **FAIL** |

---

## 3. Detailed Challenges & Empirical Bug Findings

### Challenge 1 (High): Silent Truncation to `0x00000000` in `bin.rs` at 4GB Boundary

- **Assumption Challenged**: That casting `end_addr_64 as u32` safely represents the end address of a firmware segment.
- **Attack Scenario**:
  A binary firmware image is loaded with `base_address = 0xFFFF_FFF0` and size 16 bytes:
  ```rust
  let data = vec![0xAA; 16];
  let img = parse_bin(&data, 0xFFFF_FFF0).unwrap();
  ```
  In `crates/firmware-parser/src/bin.rs`:
  ```rust
  let end_addr_64 = (base_address as u64) + (bytes.len() as u64);
  if end_addr_64 > 0x1_0000_0000 {
      return Err(ParseError::AddressOverflow { line: 0, address: end_addr_64 });
  }
  ...
  let segment_meta = SegmentMetadata {
      index: 0,
      start_address: base_address,
      end_address: end_addr_64 as u32,
      ...
  };
  let highest_address = end_addr_64 as u32;
  ```
  Because `end_addr_64 == 0x1_0000_0000` (exactly 4GB), `end_addr_64 > 0x1_0000_0000` is `false`.
  Then `(0x1_0000_0000 as u32)` wraps modulo $2^{32}$, yielding `0x0000_0000`!
- **Empirical Proof**:
  Executing `test_bin_boundary_saturation` produces:
  ```text
  Base address: 0xFFFFFFF0
  Highest address: 0x00000000
  Segment meta end: 0x00000000
  Segment struct end: 0xFFFFFFFF
  thread 'test_bin_boundary_saturation' panicked at:
  highest_address (0x00000000) must be >= base_address (0xFFFFFFF0)
  ```
- **Blast Radius**:
  Downstream flashing engines (`flash-core`, `tauri`, `cli`) calculating sector ranges or checking bounds (`start < flash_start || end > flash_limit`) see `end_address = 0`, causing premature aborts, infinite erase loops, or inverted flash sector calculations.
- **Mitigation**:
  Use `u64` for `end_address` across all metadata models, OR use inclusive end address (`start_address + len - 1`), OR explicitly clamp / document that 32-bit exclusive end address `0x1_0000_0000` saturates to `0xFFFFFFFF` uniformly across `bin.rs` and `hex.rs`.

---

### Challenge 2 (Medium): Off-by-One Address Span in `hex.rs` at 4GB Boundary

- **Assumption Challenged**: That `(highest_address - base_address) as u64` calculates accurate address span when `highest_address` is clamped by `saturating_add`.
- **Attack Scenario**:
  An Intel HEX file places 16 bytes at `0xFFFF_FFF0`:
  ```hex
  :02000004FFFFFC
  :10FFF000AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA60
  :00000001FF
  ```
  In `crates/firmware-parser/src/hex.rs`:
  ```rust
  let highest_address = segments.last().map(|s| s.end_address()).unwrap_or(0);
  let address_span = if segments.is_empty() {
      0
  } else {
      (highest_address - base_address) as u64
  };
  ```
  In `MemorySegment::end_address()`:
  `self.start_address.saturating_add(self.data.len() as u32)` clamps to `0xFFFFFFFF`.
  Then:
  `address_span = 0xFFFFFFFF - 0xFFFF_FFF0 = 15`.
  However, the 16 bytes occupy addresses `0xFFFF_FFF0` through `0xFFFF_FFFF`, spanning 16 bytes.
- **Empirical Proof**:
  Executing `test_hex_boundary_4gb_span` produces:
  ```text
  HEX Base address: 0xFFFFFFF0
  HEX Highest address: 0xFFFFFFFF
  HEX Address span: 15
  HEX Total bytes: 16
  thread 'test_hex_boundary_4gb_span' panicked:
  assertion `left == right` failed: address_span should be 16, but got 15 due to 0xFFFFFFFF saturation
    left: 15
   right: 16
  ```
- **Blast Radius**:
  UI progress telemetry and flash verification reports display incorrect byte counts and spans for firmware occupying the top of 32-bit flash space.
- **Mitigation**:
  Compute `address_span` using 64-bit arithmetic:
  `let highest_address_64 = segments.last().map(|s| s.start_address as u64 + s.data.len() as u64).unwrap_or(0);`
  `let address_span = highest_address_64 - base_address as u64;`

---

### Challenge 3 (High): Workspace Test Regression & Invalidation

- **Assumption Challenged**: Worker M1 handoff states: `cargo test -p firmware-parser` passes with 0 failures.
- **Attack Scenario**:
  Running `cargo test -p firmware-parser` against the current workspace executes both `golden_vectors.rs` and `adversarial_stress.rs`.
- **Empirical Result**:
  ```text
  test result: FAILED. 17 passed; 2 failed; 0 ignored; finished in 5.76s
  failures:
      test_bin_boundary_saturation
      test_hex_boundary_4gb_span
  ```
- **Invalidation Clause from Worker Handoff Section 5**:
  *"Invalidation Conditions: Any failing test, any clippy warning with -D warnings, or inability to compile crates/firmware-parser invalidates this completion report."*
  Because `cargo test -p firmware-parser` fails, the completion report is formally invalidated.

---

## 4. Stress Test Results

| # | Test Scenario | Expected Behavior | Actual Behavior | Result |
|---|---|---|---|---|
| 1 | Cryptographic Standards Vectors (IEEE 802.3, RFC 1321, FIPS 180-4) | Exact match with reference hashes | Matched bit-for-bit across all vectors | **PASS** |
| 2 | Intel HEX Record 02 & 03 (HEX86 20-bit addressing) | Segment base `(USBA << 4) + AAAA`; entry `(CS << 4) + IP` | Physical address 0x24100; entry 0x179B8 | **PASS** |
| 3 | Intel HEX Record 04 & 05 (HEX386 32-bit linear addressing) | Base `(ULBA << 16) + AAAA`; entry `EIP` | Physical address 0x08000100; entry 0x08000401 | **PASS** |
| 4 | Non-fatal Redundant Overlap Merging | Identical data coalesced; warning emitted | Segments merged cleanly; warnings emitted | **PASS** |
| 5 | Conflicting Overlap Collision Rejection | ParseError::ConflictingDataOverlap with byte values | Line 3, address 0x08000006, 0x07 != 0xEE reported | **PASS** |
| 6 | Out-of-Order Bank Switching Permutations | Multi-bank chunks normalized in address order | 128 reverse-order chunks normalized cleanly | **PASS** |
| 7 | 100MB & 3.2GB Sparse Memory Gaps | Handled without allocating dummy memory buffer | Parsed in < 50ms without RAM exhaustion | **PASS** |
| 8 | 4096-Record (64KB) High-Volume Stress Test | Parsed and consolidated within tight latency | Parsed in 26ms; hashes match oracle | **PASS** |
| 9 | Random Fuzzing (5,000 corrupt/garbage strings) | Rejects malformed input without panic | 0 panics across 5,000 iterations | **PASS** |
| 10 | 4GB Physical Address Overflow (`0x1_0000_0001`) | ParseError::AddressOverflow | Rejected with `AddressOverflow` | **PASS** |
| 11 | Exact 4GB Boundary in `bin.rs` (`end_addr_64 == 0x1_0000_0000`) | Valid end address >= start address | Wraps to 0x00000000 (highest < base) | **FAIL** |
| 12 | Exact 4GB Boundary in `hex.rs` (`end_addr_64 == 0x1_0000_0000`) | Address span == total bytes | Span undercounted as 15 bytes (expected 16) | **FAIL** |

---

## 5. Unchallenged Areas

- **Live Hardware Target Access**: Testing was performed purely against parser memory structures, synthetic hex/bin inputs, and reference mathematical models. Testing against live physical ST-Link/J-Link hardware is assigned to Milestone M2 (`flash-core`).
- **64-bit Microprocessor Addressing**: Microcontroller 32-bit address spaces (`u32`) are within scope; 64-bit extended architectures are excluded per `PROJECT.md`.

---

## 6. Final Verdict

**Verdict**: **`CHALLENGE_FAILED`**  
**Remediation Required Before Approval**:
1. In `crates/firmware-parser/src/bin.rs`, prevent `end_addr_64 as u32` from wrapping to 0 when `end_addr_64 == 0x1_0000_0000`. Use `saturating_sub(1)` for inclusive bounds or clamp `end_address` to `0xFFFFFFFF`.
2. In `crates/firmware-parser/src/hex.rs`, compute `address_span` in 64-bit space to prevent off-by-one undercounts when `highest_address` reaches `0xFFFFFFFF`.
3. Resolve the failing tests in `tests/adversarial_stress.rs` and ensure `cargo test -p firmware-parser` passes with 0 failures and `cargo clippy -p firmware-parser --all-targets -- -D warnings` exits cleanly.
