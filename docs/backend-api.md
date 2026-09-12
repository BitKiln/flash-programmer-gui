# Writing a backend

A backend teaches the tool to reach one more kind of target. Everything above it
— the GUI, the CLI, batch production runs, serial-number stamping, profiles,
progress reporting, cancellation — comes for free once the two traits are
implemented, because nothing above the traits names a concrete backend.

If you find yourself needing to change `flash-core` to make your backend fit,
that is a bug in the seam. Say so on the issue tracker rather than working
around it.

## The two traits

```rust
pub trait FlashBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn scheme(&self) -> &'static str;
    fn list_probes(&self) -> Result<Vec<ProbeInfo>, FlashError>;
    fn open_session(&self, config: &ConnectionConfig) -> Result<Box<dyn FlashSession>, FlashError>;
    fn detect_target(&self, config: &ConnectionConfig) -> Result<TargetInfo, FlashError> { /* default */ }
}

pub trait FlashSession: Send {
    fn target_info(&self) -> Option<&TargetInfo> { None }
    fn can_interrupt(&self, stage: FlashStage) -> bool { /* default */ }
    fn program_erases_target(&self) -> bool { false }

    fn erase_all(&mut self, cb: Option<&dyn ProgressCallback>) -> Result<(), FlashError>;
    fn erase_range(&mut self, start: u32, length: u32, cb: Option<&dyn ProgressCallback>) -> Result<(), FlashError>;
    fn program(&mut self, segments: &[MemorySegment], options: &ProgramOptions, cb: Option<&dyn ProgressCallback>) -> Result<(), FlashError>;
    fn verify(&mut self, segments: &[MemorySegment], cb: Option<&dyn ProgressCallback>) -> Result<VerifyReport, FlashError>;
    fn read_memory(&mut self, address: u32, length: u32) -> Result<Vec<u8>, FlashError>;
    fn reset(&mut self, halt: bool) -> Result<(), FlashError>;
    fn close(&mut self) -> Result<(), FlashError>;
}
```

`FlashBackend` is discovery and connection; `FlashSession` is one open
connection. Nothing in either trait touches CPU registers or a debug wire, so a
serial ROM bootloader or a remote programming server fits as well as a debug
probe does.

## Steps

1. **Create the crate** at `crates/backends/<name>`, depending on `flash-core`
   (and `device-db` if you need target aliases). Do not depend on another
   backend.
2. **Pick a scheme.** Short, lowercase, stable — it ends up in saved profiles
   and in people's scripts. `probe`, `mock`, `esp`, `openocd`.
3. **Make `list_probes` return identifiers prefixed with your scheme**, e.g.
   `esp:COM7`. This is what routes later calls back to you. A backend that finds
   nothing returns an empty `Vec`, not an error: the registry concatenates every
   backend's list, and an error from one transport must not hide another's
   probes.
4. **Read your parameters from `ConnectionConfig::transport`**, not from the
   flat `protocol` / `speed_khz` / `connect_under_reset` / `reset_type` fields.
   Those four exist for `Transport::DebugProbe` and mean nothing to anything
   else; `config.debug_params()` returns `None` when the transport is not a
   debug probe, which is the check to write. The flat fields will be removed in
   a future major version.
5. **Register it** in `crates/flash-backends/src/lib.rs`, behind a Cargo
   feature, and add the feature to the two applications' manifests.
6. **Add the family rows to `crates/device-db`** with your scheme in
   `backends`, then regenerate the support matrix:
   `cargo run -p device-db --bin gen-supported-devices`. A test fails if the
   checked-in document has drifted.

## Contracts that are easy to get wrong

**Erase leaves `0xFF`, and a write can only clear bits.** If your transport
cannot honour that, say so in `program_erases_target` rather than pretending.

**`can_interrupt` must tell the truth.** Return `true` only for stages where you
actually poll the cancel flag between units of work. Returning `true` for a
stage that runs to completion inside one driver call gives the user a Stop
button that does nothing — that was issue #8, and it is worse than having no
button at all.

**Report progress in bytes you have really written**, through the
`ProgressCallback`. The UI derives transfer rate and remaining time from it.

**`close` must be idempotent.** It is called on drop and again on explicit
disconnect.

**Unsupported is a first-class answer.** `FlashError::Unsupported` with a
sentence explaining why is far better than a silent no-op. A serial ROM
bootloader has no notion of halting a core; return an error from
`reset(halt: true)` and say that.

## Testing

Implement enough that the backend conformance suite in
`crates/flash-core/tests` passes against your backend: erase leaves `0xFF`,
program-then-read round-trips, verify catches a corrupted byte, `can_interrupt`
matches actual interruptibility, `close` is idempotent.

Then add a fake transport so CI can exercise your backend with no hardware
attached, the way `backends/mock` covers the probe path. A backend that can only
be tested with a board on the desk will be broken by the next refactor and
nobody will notice.
