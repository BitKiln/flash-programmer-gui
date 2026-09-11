# Dispatch for Challenger 2 (Milestone M1: Firmware Parser)

**Role**: Parser Correctness Challenger 2
**Working Directory**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/challenger_m1_2
**Original Request File**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md
**Project Architecture**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md
**Worker Handoff**: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m1/handoff.md

## Assignment
1. Empirically verify `firmware-parser` correctness against authoritative specifications (Intel HEX 1988, ARMv7-M vector tables, IEEE 802.3 CRC32, RFC 1321 MD5, SHA256).
2. Write independent verification checks / oracles comparing parsed output against reference mathematical models.
3. Test edge conditions: address rollover at 4GB boundary, extended linear vs extended segment records, conflicting overlaps vs redundant identical overlaps.
4. Issue a clear verdict: `APPROVE` (correctness confirmed) or `CHALLENGE_FAILED` (bugs exposed).
5. Write your report to `c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/challenger_m1_2/challenge_report.md` and handoff in `handoff.md`.

## 2026-09-10T19:42:17Z
You are Challenger 2 for Milestone M1 (firmware-parser).
Working directory: c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/challenger_m1_2
Read c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/ORIGINAL_REQUEST.md, c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/PROJECT.md, c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/worker_m1/handoff.md, and c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/challenger_m1_2/DISPATCH.md.

Verify mathematical and architectural correctness against authoritative standards (Intel HEX 1988, ARMv7-M Cortex-M vector tables, IEEE 802.3 CRC32, RFC 1321 MD5, SHA256).
Run independent checks and comparison with oracles.
Deliver your challenge report and clear verdict (APPROVE or CHALLENGE_FAILED) in c:/web_applications/open-source/embedded/flash_programmer_gui/.agents/challenger_m1_2/challenge_report.md and handoff.md.
Send a message when finished.

