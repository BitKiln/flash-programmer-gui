# Adversarial Challenge Report — Milestone M1, Iteration 2 (firmware-parser)

**Verdict**: `APPROVE`  
**Overall risk assessment**: LOW  
**Target Package**: `crates/firmware-parser`  
**Date**: 2026-09-10T19:58:00Z  
**Challenger**: Challenger 1 (critic, specialist)  

---

## 1. Executive Summary

In Milestone M1 Iteration 1, Challenger 1 identified three boundary failures at the 32-bit address ceiling (`0xFFFF_FFFF` / `0x1_0000_0000`) in `firmware-parser`.
In Iteration 2, Worker M1-IT2 implemented remediation across `metadata.rs`, `segment.rs`, `bin.rs`, `hex.rs`, and `checksum.rs`.

As Challenger 1 for Iteration 2, an empirical verification and stress campaign was executed against `crates/firmware-parser`:
1. Ran the complete test suite: 22 adversarial stress tests, 17 unit tests, and 14 golden vector tests (total 53 tests) — 100% pass rate.
2. Specifically re-tested and analyzed the 3 previously failing boundary conditions: all 3 are completely resolved without regressions.
3. Constructed and executed 9 additional adversarial boundary stress tests (redundant overlap at `0xFFFF_FFFF`, single byte at ceiling, 4GB overflow rejection, out-of-order merging at 4GB ceiling, partial overlap extension to ceiling, bounds validation, and padded checksums at ceiling) — all passed.
4. Confirmed clean linting with `cargo clippy -p firmware-parser --all-targets -- -D warnings` (0 warnings).

**Verdict**: **`APPROVE`**. The 4GB boundary remediation is robust, sound, and complete.

---

## 2. Empirical Verification of Previous Bug Fixes

### Bug 1: Conflicting Overlap Detection at `0xFFFF_FFFF` Ceiling
- **Previous Failure Mode**:
  `MemorySegment::end_address()` returned saturated `0xFFFF_FFFF`. When an incoming chunk targeted `0xFFFF_FFFF`, `chunk.address == current_end` evaluated to `0xFFFF_FFFF == 0xFFFF_FFFF` (`true`). The parser treated conflicting data as contiguous, appending it without error and allowing memory to wrap past 32-bit address space (`[0xAA, 0xBB]`).
- **Remediation Under Review**:
  In `crates/firmware-parser/src/segment.rs`, `consolidate_chunks` tracks segment endpoints using `end_address_u64()`. At `0xFFFF_FFFF` with length 1, `current_end_64 = 0x1_0000_0000`. The incoming chunk's address `0xFFFF_FFFF` evaluates `chunk_addr_64 == current_end_64` to `false` and branches into the overlap detector (`chunk_addr_64 < current_end_64`), comparing bytes and returning `ParseError::ConflictingDataOverlap`.
- **Empirical Proof**:
  ```powershell
  cargo test -p firmware-parser --test adversarial_stress -- test_adversarial_conflicting_overlap_at_ffffffff --nocapture
  ```
  **Result**: PASS. Conflicting data at `0xFFFF_FFFF` is correctly rejected with `ParseError::ConflictingDataOverlap { line: 3, address: 0xFFFFFFFF, existing: 0xAA, incoming: 0xBB }`.
- **Secondary Edge Case Testing**:
  - **Identical (Redundant) Overlap at `0xFFFF_FFFF`**: Parsed cleanly without error, payload preserved as 1 byte, emitting `ValidationWarning::RedundantOverlap { address: 0xFFFF_FFFF, line: Some(3) }`. (PASS)
  - **Address Overflow past 4GB**: 2 bytes declared at `0xFFFF_FFFF` returns `ParseError::AddressOverflow { line: 2, address: 0x1_0000_0001 }`. (PASS)

---

### Bug 2: Integer Truncation to Address 0 in Raw Binary Parser (`parse_bin`)
- **Previous Failure Mode**:
  `end_addr_64 as u32` truncated `0x1_0000_0000` to `0x0000_0000`. `metadata.highest_address` and `metadata.segments[0].end_address` became `0x0000_0000`, breaking the invariant `highest_address >= base_address` (`0x0000_0000 >= 0xFFFF_FFF0` was false).
- **Remediation Under Review**:
  In `crates/firmware-parser/src/bin.rs`, segment metadata is constructed via `build_segments_metadata(&segments)`, and `highest_address = segments[0].end_address()`. Clamping ensures that when `end_addr_64 == 0x1_0000_0000`, `highest_address` and `SegmentMetadata.end_address` saturate to `0xFFFF_FFFF`.
- **Empirical Proof**:
  ```powershell
  cargo test -p firmware-parser --test adversarial_stress -- test_bin_boundary_saturation --nocapture
  ```
  **Result**: PASS. `highest_address` (`0xFFFF_FFFF`) is `>= base_address` (`0xFFFF_FFF0`).
- **Secondary Edge Case Testing**:
  - Binary slice loaded at `base_address = 0xFFFF_FFFF` with 1 byte preserves `highest_address = 0xFFFF_FFFF >= base_address = 0xFFFF_FFFF`, `address_span = 1`, and `total_bytes = 1`. (PASS)
  - Binary slice loaded at `base_address = 0xFFFF_FFFF` with 2 bytes returns `ParseError::AddressOverflow { line: 0, address: 0x1_0000_0001 }`. (PASS)

---

### Bug 3: Address Span Undercount on 4GB Boundary in `parse_hex`
- **Previous Failure Mode**:
  `address_span` was computed as `(highest_address - base_address) as u64`. When `highest_address` saturated at `0xFFFF_FFFF`, span for 16 bytes was `0xFFFF_FFFF - 0xFFFF_FFF0 = 15` (1 byte short).
- **Remediation Under Review**:
  In `crates/firmware-parser/src/hex.rs`, `address_span` is computed using 64-bit endpoint arithmetic:
  ```rust
  let base_64 = segments.first().map(|s| s.start_address as u64).unwrap_or(0);
  let end_64 = segments.last().map(|s| s.end_address_u64()).unwrap_or(0);
  end_64.saturating_sub(base_64)
  ```
  For `0xFFFF_FFF0..0x1_0000_0000`, `0x1_0000_0000 - 0xFFFF_FFF0 = 16`.
- **Empirical Proof**:
  ```powershell
  cargo test -p firmware-parser --test adversarial_stress -- test_hex_boundary_4gb_span --nocapture
  ```
  **Result**: PASS. Contiguous 16-byte segment ending at `0xFFFF_FFFF` reports `address_span == 16`.
- **Secondary Edge Case Testing**:
  - Partial overlap extending a segment to `0x1_0000_0000` reports `address_span == 16` and `total_bytes == 16`. (PASS)
  - Inverted out-of-order chunks (`0xFFFF_FFF8` then `0xFFFF_FFF0`) consolidated into single 16-byte segment at `0xFFFF_FFF0` report `address_span == 16`. (PASS)

---

## 3. Stress Test Results Summary

| Target / Suite | Passed | Failed | Status |
|----------------|--------|--------|--------|
| `tests/adversarial_stress.rs` (22 tests) | 22 | 0 | **PASS** |
| `src/lib.rs` unit tests (17 tests) | 17 | 0 | **PASS** |
| `tests/golden_vectors.rs` (14 tests) | 14 | 0 | **PASS** |
| Boundary stress suite (9 edge cases) | 9 | 0 | **PASS** |
| `cargo clippy --all-targets -- -D warnings` | 0 warnings | 0 | **PASS** |

---

## 4. Final Verdict

**Verdict**: **`APPROVE`**  
All three previously identified 4GB boundary defects are completely resolved with robust 64-bit mathematical precision, proper saturation semantics, and zero regressions across the test suite.
