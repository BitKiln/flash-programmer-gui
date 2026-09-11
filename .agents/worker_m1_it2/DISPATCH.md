# Dispatch for Worker (Milestone M1, Iteration 2: Firmware Parser Remediation)

**Role**: Firmware Parser Remediation Worker
**Working Directory**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m1_it2
**Original Request File**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md
**Project Architecture**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md
**Fix Strategy**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_m1_it2/fix_strategy.md
**Adversarial Test Suite**: c:/web_applications/open-source/embedded/flash_programmer_gui/crates/firmware-parser/tests/adversarial_stress.rs

## Mandatory Integrity Warning
> DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

## Exclusive Write Ownership
You own:
- `crates/firmware-parser/src/**`
- `crates/firmware-parser/tests/**`

## Assignment
Implement the remediation described in `fix_strategy.md`:
1. In `metadata.rs`:
   - Add `pub fn end_address_u64(&self) -> u64` to `MemorySegment`.
   - Update `build_segments_metadata` to use `end_address_u64()` and clamp `end_address` to `0xFFFF_FFFF`.
2. In `segment.rs`:
   - Use `u64` interval arithmetic for `current_end_64 = current_segment.end_address_u64()` and `chunk_addr_64 = chunk.address as u64`.
   - Enforce proper overlap checking and prevent bypassed conflict detection at `0xFFFF_FFFF`.
3. In `bin.rs`:
   - Construct metadata using `build_segments_metadata` and ensure `highest_address` is clamped to `0xFFFF_FFFF` instead of wrapping to 0.
4. In `hex.rs`:
   - Compute `address_span` as `(highest_end_64 - base_address as u64)` where `highest_end_64` is from `end_address_u64()`.
5. Run verification:
   - `cargo test -p firmware-parser` (unit tests + golden_vectors + adversarial_stress)
   - Ensure 22/22 adversarial stress tests pass, 14/14 golden vector tests pass, 17/17 unit tests pass (53 total).
   - `cargo clippy -p firmware-parser --all-targets -- -D warnings` (0 warnings).
6. Write your completion report to `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m1_it2/handoff.md`.
