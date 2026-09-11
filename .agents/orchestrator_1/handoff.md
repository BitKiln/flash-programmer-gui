# Orchestrator Soft Handoff: Flash Programmer GUI & CLI (Generation 1 -> Generation 2)

**Handoff Type**: Soft Handoff (Context & Spawn Threshold Succession at 17 spawns)
**Predecessor**: Orchestrator Gen 1 (`conv: 1e3c0803-34e2-4843-8182-dc12e429430f`)
**Parent**: Sentinel (`conv: 33b4c6f7-dfa9-458e-95e6-8949afef5996`)
**Project Root**: `c:/web_applications/open-source/embedded/flash_programmer_gui`
**Working Directory**: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/orchestrator_1`

---

## 1. Milestone State

| Milestone | Name | Status | Key Artifacts / Outputs |
|---|---|---|---|
| **Phase 0** | Full Requirements Survey (R1-R4) | **DONE** | `.agents/explorer_survey_1/survey_report.md`, `.agents/spec_miner_survey_2/survey_report.md`, `.agents/explorer_survey_3/survey_report.md` |
| **Phase 1** | Project Spec & Architecture | **DONE** | `.agents/PROJECT.md` with 42 inventoried features, contracts, and code layout |
| **E2E Track** | Opaque-Box E2E Test Suite | **DONE** | `TEST_READY.md`, `TEST_INFRA.md`, `tests/` directory with 95/95 passing tests across Tiers 1-4 |
| **M1** | Firmware Parser (`crates/firmware-parser`) | **DONE** | `crates/firmware-parser/` fully implemented and verified. 53/53 tests passing (17 unit + 14 golden + 22 adversarial stress). Approved by 2 Reviewers, 2 Challengers, and Forensic Auditor (CLEAN). |
| **M2** | Flash Core & Probe Abstraction (`crates/flash-core`) | **PLANNED** | Ready for immediate implementation. Explorer 1 survey report provides full trait and mock architecture. Depends on M1 (which is complete). |
| **M3** | CLI Companion & Profiles (`crates/flashgui-cli`) | **PLANNED** | Dependent on M1 and M2. Explorer 3 survey report provides Clap 4 commands, profiles, and headless mock integration. |
| **M4** | Desktop Application GUI (Tauri v2 + React/TS) | **PLANNED** | Dependent on M1 and M2. Explorer 3 survey report provides IPC commands, Zustand store, panels, and telemetry HUD. |
| **M5** | Final E2E Integration & Hardening | **PLANNED** | Phase 1: 100% E2E test suite pass; Phase 2: Tier 5 adversarial coverage hardening. |

---

## 2. Active Subagents
All 17 subagents spawned by Generation 1 have delivered their handoffs and are idle/completed. There are zero pending tasks.

---

## 3. Pending Decisions & Technical Context
1. **`firmware-parser` is complete and verified**: Crate exports `parse_hex`, `parse_bin`, `MemorySegment`, `FirmwareMetadata`, `FirmwareImage`, and `validate_target_bounds`. All 4GB boundary conditions and overlap checks are enforced via 64-bit endpoint arithmetic (`end_address_u64()`).
2. **`flash-core` (M2) Ready to Build**:
   - Needs:
     - `crates/flash-core/Cargo.toml` (adding to workspace `Cargo.toml`).
     - Traits `FlashBackend` (probe listing and session factory) and `FlashSession` (erase, program, verify, reset, read_memory, close).
     - Live probe backend via `probe-rs = "0.32"` (feature gated under `live-probe`).
     - In-memory Virtual/Mock Probe backend simulating NOR flash physics (`0xFF` erased, `1 -> 0` programming bit flips), STM32F1 (1KB uniform) & STM32F4 (asymmetric) sector profiles, timing simulation modes, and `FaultInjector`.
     - Event-driven progress callback contracts (`FlashEvent`, `FlashStage`, `ProgressMetrics`).
     - High-level `FlashManager::execute_flash`.
     - Integration tests verifying probe discovery, sector erasing, block programming, byte verification, and reset cycles under normal and error-injected conditions.
3. **Dual-Track E2E Test Suite**:
   - Fully operational. Can be executed with `python tests/run_e2e.py` (or PowerShell `tests/run_e2e.ps1` or Cargo `tests/e2e_runner.rs`).

---

## 4. Remaining Work (Concrete Next Steps for Successor)
1. **Step 1: Milestone M2 (Flash Core & Probe Abstraction)**
   - Dispatch Worker M2 (`teamwork_preview_worker`) to implement `crates/flash-core` and update root `Cargo.toml`.
   - Provide worker with `ORIGINAL_REQUEST.md`, `PROJECT.md`, and `.agents/explorer_survey_1/survey_report.md`. Include mandatory integrity warning.
   - Run verification gate on M2: 2 Reviewers, 2 Challengers, and Forensic Auditor (`teamwork_preview_auditor`).
   - When gate passes, update `PROJECT.md` M2 status to `DONE`.
2. **Step 2: Milestone M3 (CLI Companion & Profiles)**
   - Dispatch Worker M3 (`teamwork_preview_worker`) to implement `crates/flashgui-cli`.
   - Provide worker with `ORIGINAL_REQUEST.md`, `PROJECT.md`, and `.agents/explorer_survey_3/survey_report.md`.
   - Run verification gate on M3: Reviewers, Challengers, and Forensic Auditor.
   - When gate passes, update `PROJECT.md` M3 status to `DONE`.
3. **Step 3: Milestone M4 (Desktop Application GUI)**
   - Dispatch Worker M4 (`teamwork_preview_worker`) to implement `src-tauri` IPC and `frontend/` (React + TS, Vite, Vitest, panels, controls, timestamped console).
   - Provide worker with `ORIGINAL_REQUEST.md`, `PROJECT.md`, and `.agents/explorer_survey_3/survey_report.md`.
   - Run verification gate on M4 (including `npm run build` and `npm test` / Vitest).
   - When gate passes, update `PROJECT.md` M4 status to `DONE`.
4. **Step 4: Milestone M5 (Final Integration & Hardening)**
   - Phase 1: Run complete E2E test suite (`python tests/run_e2e.py` / `cargo test`) ensuring 100% pass rate.
   - Phase 2: Tier 5 Adversarial Coverage Hardening.
   - Final Forensic Audit across entire repository.
5. **Step 5: Final Completion Report**
   - Synthesize results and report completion to Sentinel (`33b4c6f7-dfa9-458e-95e6-8949afef5996`).

---

## 5. Key Artifacts
- User Request: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md`
- Project Blueprint: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md`
- E2E Test Suite Ready: `c:/web_applications/open-source/embedded/flash_programmer_gui/TEST_READY.md`
- E2E Test Infra: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/TEST_INFRA.md`
- R1 Survey: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_1/survey_report.md`
- R2 Survey: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/spec_miner_survey_2/survey_report.md`
- R3/R4 Survey: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/explorer_survey_3/survey_report.md`
- Gate Tracking: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/orchestrator_1/GATE_STATUS.md`
- Briefing: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/orchestrator_1/BRIEFING.md`
- Progress: `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/orchestrator_1/progress.md`
