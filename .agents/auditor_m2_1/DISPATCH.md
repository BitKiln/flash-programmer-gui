# Task Assignment: Forensic Auditor for Milestone M2 (`crates/flash-core`)

## Objective
Perform independent forensic integrity audit of `crates/flash-core` to verify that all flash simulation, probe abstraction, live drivers, and test verifications are authentic, genuine, and un-compromised.

## Context & Inputs
- Working directory: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/auditor_m2_1`
- ORIGINAL_REQUEST.md: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md`
- PROJECT.md: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md`
- Worker handoff: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2_gen2/handoff.md`
- Codebase under audit: `crates/flash-core`

## Forensic Audit Protocol
Examine code and tests against the following anti-patterns:
1. Hardcoded verification returns: Does `verify` or `execute_flash` return synthetic `success = true` without reading memory?
2. Facade mock memory: Does `MockFlashMemory` actually store bytes and perform bitwise AND operations, or does it merely echo inputs?
3. Fault injector bypass: Does `FaultInjector` actually intercept calls and return configured errors, or is it decorative?
4. Test result fabrication: Do tests actually exercise the mock memory and probe sessions, or are assertions trivial (`assert!(true)`)?
5. Live backend stubbing: Does `ProbeRsLiveBackend` genuinely bind to `probe_rs` types when feature is enabled, or is it an empty facade?
6. Run `cargo test -p flash-core` and inspect actual test count and runtime behavior.
7. Issue binary verdict: `CLEAN` or `INTEGRITY VIOLATION` with comprehensive evidence.
8. Write `handoff.md` in your working directory and notify parent via `send_message`.

## 2026-09-11T02:31:03Z
You are Forensic Auditor (teamwork_preview_auditor) for Milestone M2 (crates/flash-core).
Read DISPATCH.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/auditor_m2_1/DISPATCH.md.
Read ORIGINAL_REQUEST.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md.
Read PROJECT.md at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md.
Read Worker handoff at c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m2_gen2/handoff.md.
Assigned working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/auditor_m2_1.
Perform forensic integrity audit for hardcoded results, dummy facades, test fabrication, and stubbing.
Write handoff.md with binary verdict (CLEAN or INTEGRITY VIOLATION) and notify parent via send_message.
