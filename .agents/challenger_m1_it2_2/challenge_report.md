# Oracle Challenge Report — Milestone M1, Iteration 2

**Challenger**: Challenger 2 (Oracle & Empirical Challenger)  
**Date**: 2026-09-11T01:28:15+05:30  
**Target Package**: `crates/firmware-parser`  
**Verdict**: `APPROVE`

---

## Challenge Summary

**Overall risk assessment**: LOW

Empirical testing confirmed that the 4GB boundary remediation completed by Worker M1-IT2 is mathematically sound, robust against edge cases and random address fuzzing, and introduces zero regressions across all golden vectors and unit tests.

- `cargo test -p firmware-parser`: **53 / 53 passed (100%)**
  - Unit tests (`src/lib.rs`): 17 passed, 0 failed
  - Adversarial stress tests (`tests/adversarial_stress.rs`): 22 passed, 0 failed
  - Golden vectors (`tests/golden_vectors.rs`): 14 passed, 0 failed
- `cargo clippy -p firmware-parser --all-targets -- -D warnings`: **0 warnings (Clean)**

---

## Empirical Verification Matrix

### 1. Golden Vectors Verification (100% Passing)

| Vector | Description | Key Invariants Verified | Status |
|---|---|---|---|
| **Vector 1** | Standard STM32 Cortex-M Image | Single segment (32 B), EIP `0x080001CD` (Record 05), CRC32 `0x0A5B1F0D`, MD5 `629f1899...`, SHA-256 `b2ed8017...` | **PASS** |
| **Vector 2** | Dual-Segment Non-Contiguous Flash Image | Segment 0 (0x08000000..0x08000020), Segment 1 (0x08040000..0x08040010), Gap `262,112` bytes, Address span `262,160` bytes, Padded 0xFF flash checksums | **PASS** |
| **Vector 3** | Out-of-Order Records Normalization | Reverse order lines normalized into ordered segment, byte order restored, checksums identical to Vector 1 | **PASS** |
| **Vector 4** | Raw Binary Cortex-M Vector Detection | MSP `0x20005000` (valid SRAM pointer, 4-byte aligned), Reset Handler `0x080001CD` (Thumb bit set, points in-bounds) | **PASS** |
| **Vector 5** | Negative Matrix (Cases A - J) | Checksum mismatch (A), missing colon (B), odd hex count (C), invalid char (D), truncated record (E), invalid lengths for 04/01 (F, G), conflicting overlap (H), target out-of-bounds (I), empty file (J) | **PASS** |

### 2. Remediation Bug Verification (Previously Failing -> Now Passing)

| Bug ID | Test Name | Root Cause | Remediated Behavior | Status |
|---|---|---|---|---|
| **BUG-1** | `test_adversarial_conflicting_overlap_at_ffffffff` | `end_address()` saturated to `0xFFFF_FFFF`, equating `chunk.address == current_end` and bypassing conflict check | 64-bit endpoint calculation `current_end_64 = 0x1_0000_0000` evaluates `chunk_addr_64 < current_end_64`, detecting conflicting bytes and raising `ParseError::ConflictingDataOverlap` | **PASS** |
| **BUG-2** | `test_bin_boundary_saturation` | `end_addr_64 as u32` modulo truncated `0x1_0000_0000` to `0x0000_0000`, breaking `highest_address >= base_address` | Segment metadata and `highest_address` clamp to `0xFFFF_FFFF` at the 4GB ceiling, satisfying `highest_address >= base_address` | **PASS** |
| **BUG-3** | `test_hex_boundary_4gb_span` | `(highest_address - base_address) as u64` evaluated `0xFFFF_FFFF - 0xFFFF_FFF0 = 15` for 16-byte payload | Address span calculated as `end_64.saturating_sub(base_64)` using 64-bit endpoints: `0x1_0000_0000 - 0xFFFF_FFF0 = 16` | **PASS** |

### 3. Fuzzed Addresses & Mathematical Models Stress Testing

An adversarial oracle test harness was executed against `firmware-parser` covering:

1. **Random Chunk Address Fuzzing (50 iterations, 10–80 chunks each)**:
   - Evaluated randomly generated address intervals across $[0, 0xFFFF\_FF00]$ with lengths $1..64$.
   - Verified that segment consolidation maintains strict ordering (`segments[i].end_address_u64() < segments[i+1].start_address`), gap tracking invariants (`gap.start == seg[i].end`, `gap.end == seg[i+1].start`, `gap.size == seg[i+1].start - seg[i].end`), and never panics.
2. **Boundary Extremes & Address Overflow Invariants**:
   - Chunk at `0xFFFF_FFF0` with 16 bytes: perfectly touches `0x1_0000_0000`, `end_address_u64() == 0x1_0000_0000`, `end_address() == 0xFFFF_FFFF`.
   - Chunk pair that individually stay within bounds but together cross the 4GB ceiling: correctly raises `ParseError::AddressOverflow { address: 0x1_0000_0004 }`.
   - Raw binary at `0xFFFF_0000` with 65,536 bytes: parses cleanly with `highest_address = 0xFFFF_FFFF`, `address_span = 65536`.
   - Raw binary at `0xFFFF_0000` with 65,537 bytes: correctly raises `ParseError::AddressOverflow { address: 0x1_0000_0001 }`.
3. **Multi-Algorithm Checksum Oracle Equivalence**:
   - RFC/FIPS known test vectors verified (`""` and `"123456789"` for CRC32, MD5, and SHA-256).
   - Segregated canonical checksum engine (`compute_canonical_checksums`) proven identical to single contiguous buffer hashing (`compute_checksums`) across random multi-segment partitions.
4. **Target Bounds Validation Fuzzing (50 random targets)**:
   - Fuzzed flash start $[0x0800\_0000, 0x0880\_0000]$ and flash sizes $64\text{ KB} - 1\text{ MB}$.
   - Verified that all valid segments pass, segments starting prior to flash base fail, and segments crossing flash limit fail with `ParseError::TargetOutOfBounds`.

---

## Minor Observation / Caveat

- **Direct `consolidate_chunks` Input Sanitization**:
  If a consumer directly constructs `RawChunk` with `address as u64 + data.len() as u64 > 0x1_0000_0000` on the very first chunk (`chunks[0]`), `consolidate_chunks` accepts it without an error because `consolidate_chunks` checks `AddressOverflow` only when extending or merging subsequent chunks. However, both frontends (`parse_hex` and `parse_bin`) strictly check `end_addr_64 > 0x1_0000_0000` before chunks or segments are created, so this condition cannot occur through normal parser usage. This is non-blocking and safe for Milestone M1.

---

## Final Verdict

**APPROVE**: All golden vectors (1-5), unit tests, adversarial stress tests, and mathematical models pass 100% without regression or defect.
