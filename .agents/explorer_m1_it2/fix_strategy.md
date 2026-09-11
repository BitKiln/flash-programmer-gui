# Firmware Parser 4GB Boundary Remediation Fix Strategy

**Milestone**: M1, Iteration 2 (Firmware Parser Remediation)  
**Author**: Explorer M1-IT2  
**Date**: 2026-09-10T19:52:00Z  
**Status**: PROPOSED FIX STRATEGY  
**Target Package**: `crates/firmware-parser`  

---

## 1. Executive Summary

Milestone M1 adversarial testing in `crates/firmware-parser/tests/adversarial_stress.rs` uncovered three critical defects at the 32-bit address boundary (`0xFFFF_FFFF` / `0x1_0000_0000` / 4GB ceiling):

1. **Bug 1 (`segment.rs`)**: Conflicting overlap detection bypassed at `0xFFFF_FFFF` ceiling. `MemorySegment::end_address()` saturates to `0xFFFF_FFFF`, falsely equating `chunk.address == current_end` and causing conflicting data at `0xFFFF_FFFF` to be appended as contiguous rather than flagged as an overlap collision (`ParseError::ConflictingDataOverlap`).
2. **Bug 2 (`bin.rs`)**: Integer truncation to `0x0000_0000` when `end_addr_64 == 0x1_0000_0000`. Casting `end_addr_64 as u32` wraps modulo $2^{32}$, creating an invalid invariant where `highest_address` (`0x0000_0000`) is strictly less than `base_address` (`0xFFFF_FFF0`).
3. **Bug 3 (`hex.rs` / `metadata.rs`)**: Address span undercount on 4GB ceiling. Because `highest_address` is clamped at `0xFFFF_FFFF`, `(highest_address - base_address) as u64` calculates 15 bytes instead of 16 for a 16-byte contiguous payload touching `0xFFFF_FFFF`.

All three defects stem from attempting to express the exclusive upper boundary of the 32-bit address space ($2^{32} = 4,294,967,296 = \text{0x1\_0000\_0000}$) inside a 32-bit integer register (`u32::MAX = \text{0xFFFF\_FFFF} = 4,294,967,295$).

This document specifies the precise, airtight remediation strategy using **64-bit internal arithmetic (`u64`)** across `metadata.rs`, `segment.rs`, `bin.rs`, `hex.rs`, and `checksum.rs`.

---

## 2. Mathematical Boundary Principles

In 32-bit microcontroller flash memory architectures:
- Address space is the half-open interval $[0, 2^{32}) = [0\text{x}0000\_0000, 0\text{x}1\_0000\_0000)$.
- Physical address coordinates for individual bytes are within $[0\text{x}0000\_0000, 0\text{x}FFFF\_FFFF]$ and fit in `u32`.
- The **exclusive end address** of a range $[start, start + length)$ where a payload touches `0xFFFF_FFFF` equals $2^{32} = 0\text{x}1\_0000\_0000$, which **cannot** be represented in `u32` without truncation or saturation.
- Any calculation involving $start + length$, span differences, or gap calculations must be performed in **`u64`** to prevent:
  - **Modulo $2^{32}$ wraparound** (`0x1_0000_0000 as u32 == 0x0000_0000`).
  - **Premature saturation collision** (`0xFFFF_FFFF.saturating_add(1) == 0xFFFF_FFFF == chunk.address`).

---

## 3. Detailed Root Cause Analysis & Empirical Failure Traces

### 3.1 Bug 1: Conflicting Overlap Bypassed at `0xFFFF_FFFF` Ceiling (`segment.rs`)

#### Root Cause
In `crates/firmware-parser/src/segment.rs:33-38`:
```rust
let current_end = current_segment.end_address();

if chunk.address == current_end {
    // Contiguous slice: append directly
    current_segment.data.extend_from_slice(&chunk.data);
```
In `crates/firmware-parser/src/metadata.rs:117`:
```rust
pub fn end_address(&self) -> u32 {
    self.start_address.saturating_add(self.data.len() as u32)
}
```
When `current_segment` begins at `0xFFFF_FFFF` with 1 byte `[0xAA]`:
- `self.start_address = 0xFFFF_FFFF`
- `self.data.len() = 1`
- `0xFFFF_FFFF.saturating_add(1) == 0xFFFF_FFFF` (saturates!)
- Therefore, `current_end = 0xFFFF_FFFF`.

When an adversarial chunk arrives at the same physical address `chunk.address = 0xFFFF_FFFF` with conflicting byte `[0xBB]`:
- `chunk.address == current_end` evaluates to `0xFFFF_FFFF == 0xFFFF_FFFF`, which is **`true`**!
- Instead of taking the `else` branch (overlap detection), the consolidation engine enters the contiguous branch and appends `[0xBB]` to `current_segment.data`.
- Result: `current_segment` now contains `[0xAA, 0xBB]` mapped to `0xFFFF_FFFF` and `0x1_0000_0000`. Conflicting data overlap error is completely bypassed, and data spills past 32-bit physical memory.

#### Empirical Failure Output
```text
---- test_adversarial_conflicting_overlap_at_ffffffff stdout ----
thread 'test_adversarial_conflicting_overlap_at_ffffffff' (38352) panicked at crates\firmware-parser\tests\adversarial_stress.rs:570:5:
Conflicting data at 0xFFFFFFFF must be rejected with ConflictingDataOverlap, got: Ok(FirmwareImage { metadata: FirmwareMetadata { ... segments: [SegmentMetadata { index: 0, start_address: 4294967295, end_address: 4294967295, size_bytes: 2, ... }] }, segments: [MemorySegment { start_address: 4294967295, data: [170, 187] }] })
```

---

### 3.2 Bug 2: Integer Truncation to Address 0 in Raw Binary Parser (`bin.rs`)

#### Root Cause
In `crates/firmware-parser/src/bin.rs:17-45`:
```rust
let end_addr_64 = (base_address as u64) + (bytes.len() as u64);
if end_addr_64 > 0x1_0000_0000 {
    return Err(ParseError::AddressOverflow { line: 0, address: end_addr_64 });
}
...
let segment_meta = SegmentMetadata {
    index: 0,
    start_address: base_address,
    end_address: end_addr_64 as u32, // Truncation!
    size_bytes: bytes.len(),
    checksums: checksums.clone(),
};
let highest_address = end_addr_64 as u32; // Truncation!
```
When `base_address = 0xFFFF_FFF0` with 16 bytes:
- `end_addr_64 = 0xFFFF_FFF0 + 16 = 0x1_0000_0000`.
- The overflow check `end_addr_64 > 0x1_0000_0000` evaluates to `false` (valid firmware).
- However, `0x1_0000_0000 as u32` drops bit 32, evaluating to `0x0000_0000`.
- `metadata.highest_address = 0x0000_0000` and `segment_meta.end_address = 0x0000_0000`.
- This inverts the fundamental architectural invariant `highest_address >= base_address` (`0x0000_0000 < 0xFFFF_FFF0`).
- Furthermore, `image.segments[0].end_address()` returns `0xFFFF_FFFF` (via `saturating_add`), causing direct divergence between segment runtime methods and metadata fields.

#### Empirical Failure Output
```text
---- test_bin_boundary_saturation stdout ----
thread 'test_bin_boundary_saturation' (6392) panicked at crates\firmware-parser\tests\adversarial_stress.rs:594:5:
highest_address (0x00000000) must be >= base_address (0xFFFFFFF0)
```

---

### 3.3 Bug 3: Address Span Undercount on 4GB Boundary (`hex.rs` / `metadata.rs`)

#### Root Cause
In `crates/firmware-parser/src/hex.rs:231-236`:
```rust
let highest_address = segments.last().map(|s| s.end_address()).unwrap_or(0);
let address_span = if segments.is_empty() {
    0
} else {
    (highest_address - base_address) as u64
};
```
When a segment occupies `0xFFFF_FFF0` to `0xFFFF_FFFF` (16 bytes):
- `highest_address` is queried from `s.end_address()`, which saturates to `0xFFFF_FFFF`.
- `address_span` is computed as:
  $$\text{address\_span} = \text{0xFFFF\_FFFF} - \text{0xFFFF\_FFF0} = 15$$
- For a single contiguous 16-byte segment with zero gaps, `address_span` reports 15 bytes while `total_bytes` reports 16 bytes.
- This creates an off-by-one undercount for any image touching the 4GB ceiling.

#### Empirical Failure Output
```text
---- test_hex_boundary_4gb_span stdout ----
thread 'test_hex_boundary_4gb_span' (32816) panicked at crates\firmware-parser\tests\adversarial_stress.rs:616:5:
assertion `left == right` failed: address_span should be 16 for a 16-byte contiguous segment, but got 15 due to 0xFFFFFFFF saturation
  left: 15
 right: 16
```

---

## 4. Remediation Strategy & Exact Code Solutions

### 4.1 Changes to `crates/firmware-parser/src/metadata.rs`

#### 4.1.1 Add `end_address_64(&self) -> u64` and harden `end_address(&self) -> u32`
Add a dedicated 64-bit end address calculation helper to `MemorySegment`, and implement `end_address()` by delegating to `end_address_64()` and clamping at `u32::MAX`:

```rust
impl MemorySegment {
    pub fn new(start_address: u32, data: Vec<u8>) -> Self {
        Self {
            start_address,
            data,
        }
    }

    /// Returns the exact mathematical exclusive end address in 64-bit space.
    #[inline]
    pub fn end_address_64(&self) -> u64 {
        (self.start_address as u64) + (self.data.len() as u64)
    }

    /// Returns the 32-bit exclusive end address, saturating at `u32::MAX` (0xFFFFFFFF)
    /// if the segment reaches or exceeds the 4GB ceiling.
    #[inline]
    pub fn end_address(&self) -> u32 {
        let end_64 = self.end_address_64();
        if end_64 >= 0x1_0000_0000 {
            u32::MAX
        } else {
            end_64 as u32
        }
    }
...
```

*Rationale*: Prevents `(self.data.len() as u32)` integer truncation when `len >= 0x1_0000_0000`, and provides a zero-cost 64-bit endpoint accessor for other modules.

#### 4.1.2 Harden `validate_target_bounds` with 64-bit arithmetic
Update `validate_target_bounds` to use 64-bit endpoints:
```rust
pub fn validate_target_bounds(
    image: &FirmwareImage,
    flash_start: u32,
    flash_size: u32,
) -> Result<(), ParseError> {
    let flash_start_64 = flash_start as u64;
    let flash_limit_64 = flash_start_64 + (flash_size as u64);
    for seg in &image.segments {
        let seg_start_64 = seg.start_address as u64;
        let seg_end_64 = seg.end_address_64();
        if seg_start_64 < flash_start_64 || seg_end_64 > flash_limit_64 {
            return Err(ParseError::TargetOutOfBounds {
                segment_start: seg.start_address,
                segment_end: seg.end_address(),
                flash_limit: flash_limit_64.min(u32::MAX as u64) as u32,
            });
        }
    }
    Ok(())
}
```

*Rationale*: When flash memory extends up to 4GB (`flash_start = 0xFFFF_0000`, `flash_size = 0x0001_0000`), `flash_limit_64 = 0x1_0000_0000`. Using `saturating_add` in `u32` would clamp both `seg_end` and `flash_limit` to `0xFFFF_FFFF`, falsely masking an out-of-bounds segment that overruns flash size by 1 or more bytes.

#### 4.1.3 Harden `detect_cortex_m_reset_vector`
```rust
            if (reset_handler & 1) == 1 {
                let code_addr_64 = (reset_handler & !1) as u64;
                let in_bounds = segments
                    .iter()
                    .any(|s| code_addr_64 >= (s.start_address as u64) && code_addr_64 < s.end_address_64());
                let msp_valid = msp != 0 && (msp % 4 == 0);

                if in_bounds && msp_valid {
                    return (Some(reset_handler), EntryPointSource::CortexMVectorTable);
                }
            }
```

*Rationale*: Ensures that code addresses near `0xFFFF_FFFF` are tested against the strict 64-bit half-open interval $[start, end)$ without premature clamping.

---

### 4.2 Changes to `crates/firmware-parser/src/segment.rs`

#### 4.2.1 64-bit Internal Endpoint Tracking in `consolidate_chunks`
Replace `current_segment.end_address()` with `current_segment.end_address_64()`. Use `u64` for all interval boundaries:

```rust
pub fn consolidate_chunks(mut chunks: Vec<RawChunk>) -> Result<ConsolidatedResult, ParseError> {
    if chunks.is_empty() {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    }

    // Sort chunks in ascending order by start address, breaking ties by line number
    chunks.sort_by(|a, b| a.address.cmp(&b.address).then_with(|| a.line.cmp(&b.line)));

    let mut segments = Vec::new();
    let mut gaps = Vec::new();
    let mut warnings = Vec::new();

    let mut current_segment = MemorySegment::new(chunks[0].address, chunks[0].data.clone());

    for chunk in chunks.into_iter().skip(1) {
        let current_end_64 = current_segment.end_address_64();
        let chunk_addr_64 = chunk.address as u64;

        if chunk_addr_64 == current_end_64 {
            // Contiguous slice: append directly
            let new_end_64 = current_end_64 + (chunk.data.len() as u64);
            if new_end_64 > 0x1_0000_0000 {
                return Err(ParseError::AddressOverflow {
                    line: chunk.line,
                    address: new_end_64,
                });
            }
            current_segment.data.extend_from_slice(&chunk.data);
        } else if chunk_addr_64 > current_end_64 {
            // Gap detected: record gap and begin new segment
            let gap_size = (chunk_addr_64 - current_end_64) as u32;
            gaps.push(MemoryGap {
                start_address: current_end_64 as u32,
                end_address: chunk.address,
                size: gap_size,
            });
            segments.push(current_segment);
            current_segment = MemorySegment::new(chunk.address, chunk.data);
        } else {
            // Overlapping slice: chunk_addr_64 < current_end_64
            let overlap_offset = (chunk_addr_64 - (current_segment.start_address as u64)) as usize;
            let overlap_len = std::cmp::min(
                current_segment.data.len() - overlap_offset,
                chunk.data.len(),
            );

            for i in 0..overlap_len {
                let existing = current_segment.data[overlap_offset + i];
                let incoming = chunk.data[i];
                if existing != incoming {
                    return Err(ParseError::ConflictingDataOverlap {
                        line: chunk.line,
                        address: (chunk.address as u64 + i as u64) as u32,
                        existing,
                        incoming,
                    });
                }
            }

            warnings.push(ValidationWarning::RedundantOverlap {
                address: chunk.address,
                line: Some(chunk.line),
            });

            if chunk.data.len() > overlap_len {
                let extra_len = (chunk.data.len() - overlap_len) as u64;
                let new_end_64 = current_end_64 + extra_len;
                if new_end_64 > 0x1_0000_0000 {
                    return Err(ParseError::AddressOverflow {
                        line: chunk.line,
                        address: new_end_64,
                    });
                }
                current_segment
                    .data
                    .extend_from_slice(&chunk.data[overlap_len..]);
            }
        }
    }

    segments.push(current_segment);

    Ok((segments, gaps, warnings))
}
```

*Proof of Fix for Bug 1*:
1. Chunk 0 at `0xFFFF_FFFF`, len 1 `[0xAA]`. `current_segment.start_address = 0xFFFF_FFFF`. `current_end_64 = 0x1_0000_0000`.
2. Chunk 1 at `0xFFFF_FFFF`, len 1 `[0xBB]`. `chunk_addr_64 = 0xFFFF_FFFF`.
3. Check `chunk_addr_64 == current_end_64`: `0xFFFF_FFFF == 0x1_0000_0000` is **`false`**.
4. Check `chunk_addr_64 > current_end_64`: `0xFFFF_FFFF > 0x1_0000_0000` is **`false`**.
5. Check `else` branch: `chunk_addr_64 < current_end_64` (`0xFFFF_FFFF < 0x1_0000_0000`) is **`true`**.
6. `overlap_offset = 0`, `overlap_len = 1`.
7. `existing = 0xAA`, `incoming = 0xBB`.
8. `existing != incoming` triggers `ParseError::ConflictingDataOverlap { line: chunk.line, address: 0xFFFFFFFF, existing: 0xAA, incoming: 0xBB }`.
9. The collision is strictly caught and rejected.

---

### 4.3 Changes to `crates/firmware-parser/src/bin.rs`

#### 4.3.1 Eliminate integer wrap and standardize metadata generation
In `bin.rs`:
1. Use `segments[0].end_address()` for `highest_address` (clamped to `0xFFFF_FFFF` when `end_addr_64 == 0x1_0000_0000`).
2. Use `build_segments_metadata(&segments)` to eliminate manual, error-prone duplicate segment metadata construction.

```rust
use crate::checksum::compute_checksums;
use crate::error::ParseError;
use crate::metadata::{
    detect_cortex_m_reset_vector, FirmwareFormat, FirmwareImage, FirmwareMetadata, MemorySegment,
};
use crate::segment::build_segments_metadata;

/// Default flash base address for STM32 microcontrollers.
pub const DEFAULT_FLASH_BASE_STM32: u32 = 0x0800_0000;

/// Parses a raw binary byte slice loaded at the specified base address.
pub fn parse_bin(bytes: &[u8], base_address: u32) -> Result<FirmwareImage, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::EmptyFile);
    }

    let end_addr_64 = (base_address as u64) + (bytes.len() as u64);
    if end_addr_64 > 0x1_0000_0000 {
        return Err(ParseError::AddressOverflow {
            line: 0,
            address: end_addr_64,
        });
    }

    let segment = MemorySegment::new(base_address, bytes.to_vec());
    let (crc32, checksums) = compute_checksums(bytes);
    let md5 = checksums.md5.clone();
    let sha256 = checksums.sha256.clone();

    let segments = vec![segment];

    // Detect Cortex-M reset handler from vector table if present
    let (entry_point, entry_point_source) = detect_cortex_m_reset_vector(&segments);

    let segment_metas = build_segments_metadata(&segments);

    let total_bytes = bytes.len();
    let highest_address = segments[0].end_address();

    let metadata = FirmwareMetadata {
        file_path: None,
        format: FirmwareFormat::RawBinary,
        file_size_bytes: total_bytes as u64,
        total_bytes,
        total_firmware_bytes: total_bytes as u64,
        base_address,
        highest_address,
        address_span: total_bytes as u64,
        gap_count: 0,
        gap_bytes: 0,
        entry_point,
        entry_point_source,
        segment_count: 1,
        segments: segment_metas,
        memory_gaps: Vec::new(),
        crc32,
        md5,
        sha256,
        checksums,
        warnings: Vec::new(),
    };

    Ok(FirmwareImage { metadata, segments })
}
```

*Proof of Fix for Bug 2*:
1. For `base_address = 0xFFFF_FFF0` with 16 bytes, `end_addr_64 = 0x1_0000_0000`.
2. `segments[0].end_address()` returns `0xFFFF_FFFF`.
3. `metadata.highest_address = 0xFFFF_FFFF`.
4. `metadata.segments[0].end_address = 0xFFFF_FFFF`.
5. `highest_address >= base_address` evaluates to `0xFFFF_FFFF >= 0xFFFF_FFF0` (**`true`**).
6. Invariant is preserved, and test `test_bin_boundary_saturation` passes.

---

### 4.4 Changes to `crates/firmware-parser/src/hex.rs`

#### 4.4.1 64-bit Address Span Computation
In `hex.rs:229-236`, compute `address_span` in the 64-bit domain from the first segment's start address to the last segment's 64-bit end address:

```rust
    let total_bytes: usize = segments.iter().map(|s| s.data.len()).sum();
    let base_address = segments.first().map(|s| s.start_address).unwrap_or(0);
    let highest_address = segments.last().map(|s| s.end_address()).unwrap_or(0);
    let address_span = if segments.is_empty() {
        0
    } else {
        let base_64 = segments.first().map(|s| s.start_address as u64).unwrap_or(0);
        let end_64 = segments.last().map(|s| s.end_address_64()).unwrap_or(0);
        end_64.saturating_sub(base_64)
    };
```

*Proof of Fix for Bug 3*:
1. For a 16-byte segment at `0xFFFF_FFF0`:
   - `base_64 = 0xFFFF_FFF0`
   - `end_64 = 0xFFFF_FFF0 + 16 = 0x1_0000_0000`
   - `address_span = 0x1_0000_0000 - 0xFFFF_FFF0 = 16`
2. `img.metadata.address_span == 16` matches `img.metadata.total_bytes == 16`.
3. Test `test_hex_boundary_4gb_span` passes.

---

### 4.5 Additional Hardening: `crates/firmware-parser/src/checksum.rs`

#### 4.5.1 64-bit Cursor Tracking in `compute_padded_checksums`
In `checksum.rs:76-95`:
```rust
    let mut current_addr_64 = segments[0].start_address as u64;

    for segment in segments {
        let seg_start_64 = segment.start_address as u64;
        if seg_start_64 > current_addr_64 {
            let mut gap_remaining = (seg_start_64 - current_addr_64) as usize;
            while gap_remaining > 0 {
                let to_write = std::cmp::min(gap_remaining, pad_chunk.len());
                crc_hasher.update(&pad_chunk[..to_write]);
                md5_hasher.update(&pad_chunk[..to_write]);
                sha_hasher.update(&pad_chunk[..to_write]);
                gap_remaining -= to_write;
            }
        }

        crc_hasher.update(&segment.data);
        md5_hasher.update(&segment.data);
        sha_hasher.update(&segment.data);

        current_addr_64 = segment.end_address_64();
    }
```

*Rationale*: Prevents `current_addr` from wrapping or saturating when padding spans between segments touching the 4GB ceiling.

---

## 5. Side-by-Side Comparison Matrix

| Module | Location | Current Implementation (Defective) | Proposed Implementation (Remediated) |
|---|---|---|---|
| `metadata.rs` | `MemorySegment::end_address` | `self.start_address.saturating_add(self.data.len() as u32)` | Provides `end_address_64() -> u64` and clamps `end_address() -> u32` safely without `len as u32` truncation |
| `metadata.rs` | `validate_target_bounds` | `flash_start.saturating_add(flash_size)` (32-bit) | Evaluates in `u64`: `(flash_start as u64) + (flash_size as u64)` |
| `metadata.rs` | `detect_cortex_m_reset_vector` | `code_addr >= s.start_address && code_addr < s.end_address()` | Evaluates in `u64`: `code_addr_64 < s.end_address_64()` |
| `segment.rs` | `consolidate_chunks` | `let current_end = current_segment.end_address();` (32-bit) | `let current_end_64 = current_segment.end_address_64();` (64-bit) |
| `bin.rs` | `parse_bin` metadata | `end_address: end_addr_64 as u32` (wraps to 0) | `build_segments_metadata(&segments)` and `highest_address = segments[0].end_address()` |
| `hex.rs` | `parse_hex` span | `(highest_address - base_address) as u64` (undercounts 15 vs 16) | `end_64.saturating_sub(base_64)` (exact 16) |
| `checksum.rs` | `compute_padded_checksums` | `current_addr = segment.end_address()` (32-bit) | `current_addr_64 = segment.end_address_64()` (64-bit) |

---

## 6. Verification Method & Acceptance Oracles

To verify the implementation once applied by Worker M1:

1. **Adversarial Stress Suite Verification**:
   ```powershell
   cargo test -p firmware-parser --test adversarial_stress
   ```
   *Expected Result*: All 22 tests pass with 0 failures:
   - `test_adversarial_conflicting_overlap_at_ffffffff ... ok`
   - `test_bin_boundary_saturation ... ok`
   - `test_hex_boundary_4gb_span ... ok`

2. **Golden Vectors Regression Verification**:
   ```powershell
   cargo test -p firmware-parser --test golden_vectors
   ```
   *Expected Result*: All 14 golden vector tests pass with 0 failures.

3. **Full Workspace Unit & Integration Test Suite**:
   ```powershell
   cargo test -p firmware-parser
   ```
   *Expected Result*: 36 tests pass across all suites.

4. **Clippy Linter Verification**:
   ```powershell
   cargo clippy -p firmware-parser --all-targets -- -D warnings
   ```
   *Expected Result*: Zero warnings emitted.
