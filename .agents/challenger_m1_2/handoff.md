# Handoff Report — Challenger 2: Milestone M1 (firmware-parser)

## 1. Observation

### 1.1 Command Executions & Test Failures
1. Command: `cargo test -p firmware-parser`
   Output:
   ```text
   running 19 tests
   ...
   test test_bin_boundary_saturation ... FAILED
   test test_hex_boundary_4gb_span ... FAILED
   ...
   failures:
   ---- test_bin_boundary_saturation stdout ----
   Base address: 0xFFFFFFF0
   Highest address: 0x00000000
   Segment meta end: 0x00000000
   Segment struct end: 0xFFFFFFFF
   thread 'test_bin_boundary_saturation' (35632) panicked at crates\firmware-parser\tests\adversarial_stress.rs:454:5:
   highest_address (0x00000000) must be >= base_address (0xFFFFFFF0)

   ---- test_hex_boundary_4gb_span stdout ----
   HEX Base address: 0xFFFFFFF0
   HEX Highest address: 0xFFFFFFFF
   HEX Address span: 15
   HEX Total bytes: 16
   thread 'test_hex_boundary_4gb_span' (34480) panicked at crates\firmware-parser\tests\adversarial_stress.rs:434:5:
   assertion `left == right` failed: address_span should be 16, but got 15 due to 0xFFFFFFFF saturation
     left: 15
    right: 16

   test result: FAILED. 17 passed; 2 failed; 0 ignored; finished in 5.76s
   ```

2. Command: `cargo clippy -p firmware-parser --all-targets -- -D warnings`
   Output:
   ```text
   error: useless use of `vec!`
      --> crates\firmware-parser\tests\adversarial_stress.rs:365:23
      --> crates\firmware-parser\tests\adversarial_stress.rs:390:17
      --> crates\firmware-parser\tests\adversarial_stress.rs:422:17
   error: could not compile `firmware-parser` (test "adversarial_stress") due to 8 previous errors
   ```

3. Command: Independent Challenger Oracle Execution (`cargo run` in temporary harness):
   All standards suites (IEEE 802.3 CRC32, RFC 1321 MD5, FIPS 180-4 SHA-256, ARMv7-M Cortex-M vector table heuristics, Intel HEX records 00, 01, 02, 03, 04, 05, and 64KB high-volume parsing) passed 100%.

### 1.2 Inspected Code Locations
- `crates/firmware-parser/src/bin.rs:17-44`:
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
  When `base_address = 0xFFFF_FFF0` and `bytes.len() = 16`, `end_addr_64 == 0x1_0000_0000`. Casting `end_addr_64 as u32` truncates to `0`.
- `crates/firmware-parser/src/hex.rs:230-236`:
  ```rust
  let base_address = segments.first().map(|s| s.start_address).unwrap_or(0);
  let highest_address = segments.last().map(|s| s.end_address()).unwrap_or(0);
  let address_span = if segments.is_empty() {
      0
  } else {
      (highest_address - base_address) as u64
  };
  ```
  When `s.end_address()` saturates to `0xFFFFFFFF` at the 4GB ceiling, `address_span` is computed as `0xFFFFFFFF - 0xFFFF_FFF0 = 15` (1 byte short of the true 16 bytes).

---

## 2. Logic Chain

1. **Standards Compliance Verification**:
   As observed in Section 1.1 (Oracle Execution), all mathematical models (IEEE 802.3 CRC-32 polynomial `0x04C11DB7`, RFC 1321 MD5, FIPS SHA-256) and Intel HEX 1988 specifications (records 00..05, two's complement modulo 256 sum) match authoritative reference models.
2. **Boundary Stress Testing (4GB Rollover)**:
   Per Section 1.2 (`bin.rs:38,44`), when raw binary firmware reaches the 4GB ceiling `0x1_0000_0000`, the calculation `end_addr_64 as u32` wraps modulo $2^{32}$ to `0`.
3. **Invariant Violation**:
   This produces `highest_address = 0` and `segment_meta.end_address = 0`, directly violating the invariant `highest_address >= base_address` (`0 >= 0xFFFFFFF0` is false).
4. **Discrepancy with `MemorySegment`**:
   `MemorySegment::end_address()` uses `saturating_add`, returning `0xFFFFFFFF`, while `SegmentMetadata::end_address` contains `0`.
5. **Address Span Calculation Flaw**:
   Per Section 1.2 (`hex.rs:235`), computing `address_span` via `highest_address - base_address` where `highest_address` is clamped to `0xFFFFFFFF` undercounts the physical span by 1 byte.
6. **Workspace Test Failure**:
   Running `cargo test -p firmware-parser` fails with 2 failed tests in `adversarial_stress.rs`. Per Worker M1's explicit handoff invalidation clause: *"Any failing test, any clippy warning with -D warnings, or inability to compile crates/firmware-parser invalidates this completion report"*. Therefore, Worker M1's handoff is invalidated.

---

## 3. Caveats

- **Normal Microcontroller Flash Ranges**: In realistic STM32 targets (e.g. Flash at `0x0800_0000` up to `0x0820_0000`), these 4GB boundary rollover bugs do not trigger.
- **Review-Only Constraint**: As a challenger agent with review-only constraints, no implementation source files were modified. The bug report is delivered for worker remediation.

---

## 4. Conclusion

**Verdict**: **`CHALLENGE_FAILED`**  
The firmware parser crate cannot be approved until:
1. `bin.rs` is modified to prevent `end_addr_64 as u32` from wrapping to `0`.
2. `hex.rs` is modified to calculate `address_span` using 64-bit bounds.
3. `tests/adversarial_stress.rs` clippy warnings and failing tests are resolved so that `cargo test -p firmware-parser` and `cargo clippy -p firmware-parser --all-targets -- -D warnings` pass with zero errors.

---

## 5. Verification Method

To independently reproduce and verify these findings:
1. Run `cargo test -p firmware-parser` from the workspace root:
   Observe the failure of `test_bin_boundary_saturation` and `test_hex_boundary_4gb_span`.
2. Run `cargo clippy -p firmware-parser --all-targets -- -D warnings`:
   Observe the compiler warnings in `tests/adversarial_stress.rs`.
3. Review `crates/firmware-parser/src/bin.rs` lines 38 and 44 to confirm the `end_addr_64 as u32` truncation bug.
