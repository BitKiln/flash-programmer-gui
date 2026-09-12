# Flash Programmer GUI

A cross-platform desktop application and CLI for erasing, programming, verifying, and inspecting
microcontroller flash — without memorising a different vendor tool for every chip on your desk.

It works with the probes you already
own (ST-Link, CMSIS-DAP/DAPLink, J-Link) over SWD or JTAG, and with ESP32 boards over their
serial ROM bootloader with no probe at all. The same
engine is exposed twice: as a GUI for day-to-day bring-up, and as a headless CLI for CI and
automated firmware testing.

```
       ┌───────────────────────┐         ┌───────────────────────┐
       │    Flash Programmer   │         │      flashgui-cli     │
       │   (Tauri v2 + React)  │         │   (Clap 4 headless)   │
       └──────────┬────────────┘         └────────────┬──────────┘
                  │                                   │
                  └─────────────────┬─────────────────┘
                                    ▼
       ┌───────────────────────┐  ┌───────────────────────────────┐
       │    firmware-parser    │  │           flash-core          │
       │   (HEX / BIN / ELF)   │  │   traits · registry · batch   │
       └───────────────────────┘  └────────────────┬──────────────┘
                                                   │  BackendRegistry routes on
                                   ┌───────────────┴───────────┐  the identifier scheme
                                   ▼                           ▼
                        ┌───────────────────────┐   ┌───────────────────────┐
                        │   backends/probe-rs   │   │     backends/mock     │
                        │    probe:  ST-Link,   │   │    mock:  NOR flash   │
                        │    DAPLink, J-Link    │   │     model · faults    │
                        └───────────────────────┘   └───────────────────────┘
```

## What it does

- **Probe discovery** — lists attached debug probes with vendor, serial, protocols, and speeds.
- **Target auto-detection** — reads the target's ID registers and resolves it against the probe-rs
  registry instead of asking you to pick a chip from a hardcoded list.
- **Firmware parsing** — Intel HEX, raw binary, and ELF (`PT_LOAD` segments by physical address, so
  what gets written is what a linker script actually placed in flash). Reports segments, gaps,
  address range, entry point, and CRC32/MD5/SHA-256 per segment.
- **Flash operations** — sector-scoped or full-chip erase, program, verify, reset (optionally
  halting).
- **Telemetry** — per-stage progress with byte counts, transfer rate, elapsed time, and a
  timestamped log of everything the programmer did.
- **Profiles** — reusable TOML configurations (target, probe, interface, speed, firmware path,
  options) so a repeat flash is one command.
- **Mock backend** — a simulated NOR flash with real physics (erased `0xFF`, writes only clear
  bits), STM32 sector geometry, and deterministic fault injection, so the whole tool can be tested
  in CI with no hardware attached.

## Documentation

- [Architecture](docs/architecture.md) — crates, how a backend is chosen, the flash data flow.
- [Writing a backend](docs/backend-api.md) — the two traits and the contracts that are easy
  to get wrong.
- [Supported devices](docs/supported-devices.md) — generated from the device database.

## Requirements

- Rust (stable, 2021 edition) and a working `cargo`
- Node.js 18+ and npm, for the desktop app
- A debug probe with drivers your OS can see. On Windows, ST-Link works out of the box; on Linux
  you will need the usual probe-rs udev rules.

## Building and running

### CLI

```bash
cargo build --release -p flashgui-cli
```

The binary lands at `target/release/flashgui-cli`.

### Desktop app

```bash
cd apps/gui
npm install
cargo tauri dev
```

`cargo tauri` comes from `cargo install tauri-cli --version "^2"`, and `cargo tauri build` produces
an installer. The Vite dev server is started for you by the Tauri config; `npm run dev` on its own
serves the frontend in a browser, where the IPC commands are unavailable.

## CLI usage

```bash
flashgui-cli devices
```

```bash
flashgui-cli flash build/app.elf --verify --reset
```

```bash
flashgui-cli flash build/app.hex --target STM32U575ZITxQ --interface swd --speed 4000 --verify --reset
```

Raw binaries carry no addresses, so they need a load address:

```bash
flashgui-cli flash build/app.bin --base-address 0x08000000 --verify
```

Standalone operations:

```bash
flashgui-cli erase --target STM32U575ZITxQ --full
```

```bash
flashgui-cli erase --target STM32U575ZITxQ --address 0x08000000 --length 16384
```

```bash
flashgui-cli verify build/app.elf --target STM32U575ZITxQ
```

```bash
flashgui-cli reset --target STM32U575ZITxQ --halt
```

### Profiles

```bash
flashgui-cli profile save sensor-board --target STM32G431 --speed 4000 --firmware build/app.hex --verify --reset
```

```bash
flashgui-cli flash --profile sensor-board
```

Command-line flags still override the profile. Profiles are TOML files resolved from
`./.flashgui/profiles/` first, then your user config directory; `--profile-file <path>` points at
one directly.

### ESP32 over the serial bootloader

An ESP board needs no debug probe — just the USB cable it is already plugged in with. Put it
in download mode (hold **BOOT** while tapping **RESET**, if it does not enter by itself) and
point at the port:

```bash
flashgui-cli flash build/app.bin --port COM7 --base-address 0x10000 --verify
```

`--port COM7` is shorthand for `--probe esp:COM7`; `/dev/ttyUSB0` and `/dev/cu.usbserial-*` work
the same way. `--baud` sets the rate (default 460800). `devices` lists serial ports alongside
debug probes.

**Addresses are flash offsets, not the memory-mapped view.** On an ESP part the CPU sees flash
at 0x3C00_0000 and friends, but everything written over the bootloader is addressed from 0. An
ESP-IDF application image normally goes at `0x10000`, the bootloader at `0x0` or `0x1000`, and
the partition table at `0x8000`. Passing the memory-mapped address instead is the single most
common way to get this wrong.

Erasing and programming each run to completion inside one bootloader transaction, so neither
**can be interrupted**; a write ends by rebooting the chip out of download mode, and stopping
partway would leave it in a state nothing could describe. Verification polls and can stop.
Verification uses the chip's
own MD5 rather than reading a multi-megabyte image back over the UART, and only reads bytes back
when a digest disagrees, to report where.

Reaching an ESP over JTAG instead is possible but needs a chip description: probe-rs ships none
for Espressif. Supply one with `--target-yaml <path>`, from `probe-rs target-gen` or
[esp-rs/esp-flash-loader](https://github.com/esp-rs/esp-flash-loader). None is bundled here —
see [docs/supported-devices.md](docs/supported-devices.md).

### Batch (production) mode

Program a run of boards without restarting the tool. `batch` takes every flag `flash` takes, plus
the run controls:

```bash
flashgui-cli batch build/app.elf --profile sensor-board --count 50 --log run.csv
```

Between boards the runner waits for the programmed board to be unplugged and the next one to
appear. On a fixture that swaps boards under software control, `--rearm immediate` skips that wait.
Failures are logged and the run continues; `--stop-on-error` ends it at the first failure instead.
`--log` writes one CSV row per board (index, pass/fail, target, bytes, duration, message), or JSON
with `--log-json`. The exit code is 1 if any board failed.

The desktop application exposes the same run under its **Batch** tab: the unit table fills in as
boards are programmed, the prompts say which board to unplug or connect, and Stop ends the run.
A batch owns the probe for its whole duration, so the interactive session is dropped when one
starts and you reconnect afterwards.

### Cancelling an operation

Cancellation is cooperative: a backend can only act on it where it polls between
units of work. The mock backend polls throughout erase, programming, and verification.
`probe-rs` runs a chip erase and a flash download to completion inside one driver call, so on
real hardware only the verify pass can stop early. The desktop app asks the session which stages
it can abort and offers Stop for those alone — during the others it says the stage cannot be
interrupted rather than showing a button that would do nothing.

### Serial numbers

Both `flash` and `batch` can stamp a unique value into each board once the image
itself is on it, so a failed flash never leaves a numbered but unprogrammed unit:

```bash
flashgui-cli batch build/app.elf --count 50   --serial-address 0x0801F800 --serial-format "ACME-{n:06}" --serial-start 1000
```

`{n}` is the counter and `{n:06}` pads it; `--serial-step` sets the increment. The value is
written as ASCII padded to `--serial-width` bytes (default 16), or as a raw integer with
`--serial-encoding u32le|u32be|u64le`. It is read back after writing unless `--no-serial-verify`
is given, and it appears in the production log and in the desktop app's unit table.

The address must be a flash location the firmware image does not itself write — normally a
dedicated sector or a slot at the end of flash. A full chip erase erases it too, so a re-run of the
same board is re-stamped rather than left with the old value.

### CI and scripting

`--json` turns every status, progress, and completion message into NDJSON on stdout, and the exit
code is the contract:

| Code | Meaning |
|------|---------|
| 0 | success |
| 1 | flash or verify failure |
| 2 | target connection error |
| 3 | firmware parse error (including addresses outside the target's flash) |
| 4 | requested probe not found |
| 5 | invalid arguments or profile |

`--mock` swaps in the simulated backend, so a pipeline can exercise the whole tool without a board:

```bash
flashgui-cli --mock --json flash firmware.hex --target STM32F401RE --verify
```

## Testing

```bash
cargo test --workspace
```

```bash
cd apps/gui && npm test
```

The end-to-end suite in `crates/e2e-tests` spawns the real `flashgui-cli` binary in a sandbox with
its own temp and working directory, so nothing it asserts depends on internal APIs.

Tier 5 drives an attached board and is skipped unless you opt in. **It erases and reprograms the
connected target:**

```bash
FLASHGUI_HW_TARGET=STM32U575ZITxQ cargo test -p e2e-tests --test hardware -- --ignored
```

Set `FLASHGUI_HW_PROBE` as well when more than one probe is attached.

## Status

| Component | State |
|---|---|
| `firmware-parser` — HEX, BIN, ELF | Done |
| `flash-core` — traits, backend registry, orchestration, batch runner | Done |
| `backends/probe-rs`, `backends/mock` — separate crates behind the registry | Done |
| `device-db` — target aliases, family/backend matrix, generated support matrix | Done |
| `flashgui-cli` — devices, flash, batch, erase, verify, reset, profiles | Done |
| E2E suite — Tiers 1-4 (mock) and Tier 5 (hardware) | Done |
| Desktop GUI — connection, firmware, controls, progress, console | Done |
| Cancellation, pushed telemetry, segment inspector, GUI profiles | Done |
| Memory viewer — hex view, firmware comparison, save region | Done |
| Batch / production mode — `flash-core` runner and `batch` CLI command | Done |
| Batch mode in the desktop application — Batch tab, live unit table, log file | Done |
| Serial-number programming — CLI flags, batch integration, desktop Batch tab | Done |
| Silicon Labs EFR32/EFM32 — through probe-rs and J-Link | Untested on hardware |
| ESP32 over the serial ROM bootloader — `esp:` backend, CLI and desktop | Verified on an ESP-WROOM-32 |
| Runtime chip descriptions (`--target-yaml`) for parts probe-rs lacks | Done |
| OpenOCD backend | Planned (v0.5) |

## Licence

MIT OR Apache-2.0.
