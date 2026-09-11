# Adversarial Challenge Report — Milestone M1 (firmware-parser)

**Verdict**: `CHALLENGE_FAILED`  
**Overall risk assessment**: HIGH  
**Target Package**: `crates/firmware-parser`  
**Date**: 2026-09-10T19:50:00Z  
**Challenger**: Challenger 1 (critic, specialist)

---

## Challenge Summary

An adversarial stress test suite (`crates/firmware-parser/tests/adversarial_stress.rs`) comprising 22 rigorous stress tests was constructed and executed against `firmware-parser`. The suite tested fuzzed/corrupted checksums, truncated records, malformed ASCII/UTF-8 framing, 128-record bank switching in inverted order, massive 100MB and 3GB address gaps, random garbage fuzzing (5,000 iterations), high-throughput workloads (10,000 records), and 32-bit ceiling boundary conditions.

While `firmware-parser` demonstrates outstanding resilience against syntax corruption, non-hex characters, single-bit flips, out-of-order normalization, and sparse gap preservation, **three empirical bugs were exposed at 32-bit address boundary conditions (0xFFFF_FFFF / 0x1_0000_0000)**, one of which results in silent data corruption and bypasses collision detection entirely.

---

## Challenges & Empirical Bug Findings

### [HIGH] Challenge 1: Conflicting Overlap Detection Bypassed at 0xFFFF_FFFF Ceiling

- **Assumption Challenged**: The chunk consolidation engine (`consolidate_chunks` in `segment.rs`) assumes that if `chunk.address == current_segment.end_address()`, the incoming chunk is strictly contiguous and safe to append via `extend_from_slice`.
- **Attack Scenario**:
  An Intel HEX image defines a record containing data at physical address `0xFFFF_FFFF` (e.g. ULBA `0xFFFF`, offset `0xFFFF`, 1 byte `[0xAA]`).
  `MemorySegment::end_address(&self)` calculates `self.start_address.saturating_add(self.data.len() as u32)`.
  Because `0xFFFF_FFFF.saturating_add(1)` saturates to `0xFFFF_FFFF`, `current_segment.end_address()` returns `0xFFFF_FFFF`.
  A subsequent record declares conflicting data `[0xBB]` at the exact same physical address `0xFFFF_FFFF`.
  In `segment.rs:35`:
  ```rust
  let current_end = current_segment.end_address(); // == 0xFFFF_FFFF
  if chunk.address == current_end {                 // 0xFFFF_FFFF == 0xFFFF_FFFF -> TRUE!
      current_segment.data.extend_from_slice(&chunk.data);
  }
  ```
- **Actual Behavior**:
  The parser treats the second record as contiguous rather than overlapping! It appends `0xBB` directly to `current_segment.data`, producing `segments[0].data = [0xAA, 0xBB]` with `start_address = 0xFFFF_FFFF` and `end_address = 0xFFFF_FFFF`.
- **Blast Radius**:
  1. `ParseError::ConflictingDataOverlap` is completely bypassed; conflicting data at `0xFFFF_FFFF` is silently accepted.
  2. Redundant identical data at `0xFFFF_FFFF` is duplicated into memory instead of emitting `ValidationWarning::RedundantOverlap`.
  3. The segment payload now contains bytes mapped past 32-bit address space (`0x1_0000_0000`), which causes silent firmware corruption when flashed.
- **Empirical Proof**:
  `cargo test -p firmware-parser --test adversarial_stress -- test_adversarial_conflicting_overlap_at_ffffffff --nocapture` fails:
  ```text
  Conflicting data at 0xFFFFFFFF must be rejected with ConflictingDataOverlap, got: Ok(FirmwareImage { ... segments: [MemorySegment { start_address: 4294967295, data: [170, 187] }] })
  ```
- **Mitigation**:
  1. Represent address intervals during consolidation using `u64` for endpoint tracking (`start_address as u64 + data.len() as u64`), preventing 32-bit saturation from colliding with chunk start addresses.
  2. Or check `chunk.address < current_segment.start_address as u64 + current_segment.data.len() as u64` before checking contiguity.

---

### [HIGH] Challenge 2: Integer Truncation to Address 0 in Raw Binary Parser (`parse_bin`)

- **Assumption Challenged**: The raw binary parser (`parse_bin` in `bin.rs`) assumes `end_addr_64 as u32` is a valid representation of the image endpoint when `end_addr_64 <= 0x1_0000_0000`.
- **Attack Scenario**:
  A binary image is loaded at `base_address = 0xFFFF_FFF0` with 16 bytes (or `base_address = 0xFFFF_0000` with 64KB).
  `end_addr_64 = (base_address as u64) + (bytes.len() as u64) = 0x1_0000_0000`.
  The condition `if end_addr_64 > 0x1_0000_0000` evaluates to `false` (valid).
  However, in `bin.rs:38` and `bin.rs:45`:
  ```rust
  let segment_meta = SegmentMetadata {
      index: 0,
      start_address: base_address,
      end_address: end_addr_64 as u32, // 0x1_0000_0000 as u32 == 0x0000_0000!
      size_bytes: bytes.len(),
      checksums: checksums.clone(),
  };
  let highest_address = end_addr_64 as u32; // == 0x0000_0000!
  ```
- **Actual Behavior**:
  `metadata.highest_address` and `metadata.segments[0].end_address` become `0x0000_0000`.
- **Blast Radius**:
  1. Invariant broken: `highest_address` (`0x0000_0000`) is strictly less than `base_address` (`0xFFFF_FFF0`).
  2. State divergence: `image.segments[0].end_address()` returns `0xFFFF_FFFF` (via `saturating_add`), while `image.metadata.segments[0].end_address` reports `0x0000_0000`.
  3. UI / Downstream consumer confusion: GUI memory inspector table will display a segment start of `0xFFFF_FFF0` and end of `0x0000_0000`.
- **Empirical Proof**:
  `cargo test -p firmware-parser --test adversarial_stress -- test_bin_boundary_saturation --nocapture` fails:
  ```text
  highest_address (0x00000000) must be >= base_address (0xFFFFFFF0)
  ```
- **Mitigation**:
  In `bin.rs`, compute `end_address` as `base_address.saturating_add((bytes.len() as u32).saturating_sub(1))` if inclusive, or clamp to `0xFFFF_FFFF` if half-open interval, and ensure consistency with `MemorySegment::end_address()`.

---

### [MEDIUM] Challenge 3: Address Span Undercount on 4GB Boundary in `parse_hex`

- **Assumption Challenged**: In `hex.rs:235`, `address_span` is computed as `(highest_address - base_address) as u64`, assuming `highest_address` is a strict half-open interval limit `[base, highest)`.
- **Attack Scenario**:
  An Intel HEX file places a 16-byte payload ending at `0xFFFF_FFFF` (from `0xFFFF_FFF0` to `0xFFFF_FFFF`).
  `highest_address = segments.last().map(|s| s.end_address()).unwrap_or(0);`
  Because `MemorySegment::end_address()` saturates at `u32::MAX`, `highest_address = 0xFFFF_FFFF`.
  `address_span = (0xFFFF_FFFF - 0xFFFF_FFF0) as u64 = 15`.
- **Actual Behavior**:
  For a contiguous 16-byte segment with zero gaps, `metadata.address_span` reports 15 bytes while `metadata.total_bytes` reports 16 bytes.
- **Blast Radius**:
  Inconsistency in metadata reporting; telemetry shows `address_span < total_bytes` on full flash images touching 4GB.
- **Empirical Proof**:
  `cargo test -p firmware-parser --test adversarial_stress -- test_hex_boundary_4gb_span --nocapture` fails:
  ```text
  assertion `left == right` failed: address_span should be 16 for a 16-byte contiguous segment, but got 15 due to 0xFFFFFFFF saturation
    left: 15
   right: 16
  ```
- **Mitigation**:
  Use `u64` math for segment span: `let highest = segments.last().map(|s| s.start_address as u64 + s.data.len() as u64).unwrap_or(0); address_span = highest - base_address as u64;`.

---

## Stress Test Results

| Test Scenario | Target Module | Expected Behavior | Actual Behavior | Result |
|---|---|---|---|---|
| Single-bit flips across checksum byte (8 bits) | `hex.rs` | `ParseError::ChecksumMismatch` with exact expected/found | Exact match on all 8 bits | **PASS** |
| Zero / 0xFF corrupted checksums | `hex.rs` | `ParseError::ChecksumMismatch` | Rejected cleanly | **PASS** |
| Substring truncation at every character cut | `hex.rs` | `ParseError::RecordTruncated` or formatting error | Rejected on 100% of cuts | **PASS** |
| Declared byte count mismatch (16 vs 2, 1 vs 2) | `hex.rs` | `ParseError::RecordTruncated` | Rejected cleanly | **PASS** |
| Invalid leading prefixes (;, #, !, tabs, colons) | `hex.rs` | `ParseError::MissingLeadingColon` / formatting | Rejected cleanly | **PASS** |
| Non-hex characters (G, Z, emoji, Unicode) | `hex.rs` | `ParseError::InvalidHexCharacter` / `OddHexDigitCount` | Rejected cleanly | **PASS** |
| Odd hex digit counts (1, 3, 13, 17 digits) | `hex.rs` | `ParseError::OddHexDigitCount` | Rejected cleanly | **PASS** |
| Empty string, whitespace-only, binary empty | `hex.rs`, `bin.rs` | `ParseError::EmptyFile` | Rejected cleanly | **PASS** |
| 128 out-of-order records across 4 banks | `segment.rs` | Fully sorted, coalesced into 4 segments with 3 gaps | Normalized, all bytes verified | **PASS** |
| 100MB address gap simulation | `segment.rs`, `checksum.rs` | O(1) memory, exact gap metadata, streaming hash | Processed cleanly | **PASS** |
| 3.2GB address gap sparse efficiency | `hex.rs` | Parse in < 50ms without 3GB heap allocation | Parsed in < 5ms, 0 heap bloat | **PASS** |
| Identical redundant overlaps (duplicate records) | `segment.rs` | Emits `ValidationWarning::RedundantOverlap` | 2 warnings emitted | **PASS** |
| Partial overlap extending contiguous segment | `segment.rs` | Merges into 6-byte segment, warning emitted | Merged cleanly | **PASS** |
| Conflicting data overlap at physical boundary | `segment.rs` | `ParseError::ConflictingDataOverlap` | Error reported with address/bytes | **PASS** |
| Target flash bounds validation matrix | `metadata.rs` | Rejects bounds violations, accepts valid ranges | 100% compliant | **PASS** |
| Cortex-M Vector table heuristics stress | `metadata.rs` | Rejects unaligned MSP, zero MSP, short segments | 100% compliant | **PASS** |
| 10,000-record high-throughput workload | `hex.rs` | Parses in < 500ms without memory degradation | Parsed in 65ms | **PASS** |
| Random garbage fuzzer (5,000 iterations) | `lib.rs` | Never panics or crashes | Zero panics | **PASS** |
| Conflicting overlap at 0xFFFFFFFF ceiling | `segment.rs`, `metadata.rs` | `ParseError::ConflictingDataOverlap` | **Bypassed; falsely treated as contiguous** | **FAIL (BUG 1)** |
| 32-bit wrap to 0 in raw binary endpoint | `bin.rs` | `highest_address >= base_address` | **highest_address wrapped to 0** | **FAIL (BUG 2)** |
| Address span on segment reaching 0xFFFFFFFF | `hex.rs` | `address_span == 16` | **address_span == 15** | **FAIL (BUG 3)** |

---

## Unchallenged Areas

- **Record Type 03 (x86 Start Segment Address)**: Real-mode 8086 CS:IP execution entry point calculation verified syntactically in `golden_vectors.rs`, but deeper x86 real-mode segment wrap behavior was not stress-tested as project focus is ARM Cortex-M / STM32.
- **Hardware Probes / probe-rs Execution**: Deferred to Milestone M2 (`flash-core`).

---

## Conclusion & Verdict

**VERDICT: `CHALLENGE_FAILED`**

The implementation of `crates/firmware-parser` possesses strong algorithmic foundations, excellent performance under massive workloads, and solid parsing hygiene for typical microcontroller address spaces. However, because edge-case boundary conditions at the 32-bit ceiling (`0xFFFF_FFFF` / `0x1_0000_0000`) cause silent collision bypass (Bug 1), integer wraparound to address 0 (Bug 2), and address span undercounts (Bug 3), the crate CANNOT be approved in its current state.

These findings are documented with empirical regression tests in `crates/firmware-parser/tests/adversarial_stress.rs` for remediation by Worker M1.
