# Architecture

Two applications, one engine, and a set of backends that can be swapped without
either application knowing.

```
       ┌───────────────────────┐         ┌─────────────────────────┐
       │   Flash Programmer    │         │      flashgui-cli       │
       │   (Tauri v2 + React)  │         │    (Clap 4 headless)    │
       └──────────┬────────────┘         └────────────┬────────────┘
                  │                                   │
                  └───────────────┬───────────────────┘
                                  ▼
                      ┌───────────────────────┐
                      │    flash-backends     │  which backends this build has
                      └───────────┬───────────┘
                                  ▼
       ┌──────────────────────────────────────────────────────┐
       │                      flash-core                      │
       │  FlashBackend / FlashSession · BackendRegistry        │
       │  FlashManager · batch runner · serial stamping        │
       │  profiles · progress + cancellation                   │
       └───┬───────────────────┬──────────────────┬────────────┘
           │                   │                  │
           ▼                   ▼                  ▼
  ┌─────────────────┐  ┌────────────────┐  ┌────────────────┐
  │ backends/       │  │ backends/mock  │  │  device-db     │
  │ probe-rs        │  │                │  │  (vendor data) │
  └─────────────────┘  └────────────────┘  └────────────────┘
           │
           ▼
  ST-Link · CMSIS-DAP · J-Link          firmware-parser (HEX / BIN / ELF)
```

## Crates

| Crate | Holds | Depends on |
|---|---|---|
| `firmware-parser` | Intel HEX, raw binary, and ELF parsing into address-tagged segments | — |
| `flash-core` | The two traits, the registry, orchestration, batch running, serial stamping, profiles, programming history, progress and cancellation | `firmware-parser` |
| `device-db` | Target aliases, the family/backend capability matrix, the generated support matrix | — |
| `backends/probe-rs` | Arm targets through probe-rs | `flash-core`, `device-db`, `probe-rs` |
| `backends/mock` | Simulated NOR flash with fault injection | `flash-core` |
| `backends/esp-serial` | ESP32 over the serial/USB ROM bootloader | `flash-core`, `espflash` |
| `flash-backends` | Aggregates the backends this build enables into a registry | `flash-core`, the backend crates |
| `flashgui-cli` | The headless CLI | `flash-backends`, `flash-core`, `device-db` |
| `apps/gui/src-tauri` | Tauri commands and app state | the same three |

The aggregator exists to keep the dependency graph acyclic. `flash-core` defines
`BackendRegistry` but holds only `Box<dyn FlashBackend>` and names no concrete
backend; each backend depends on `flash-core`; `flash-backends` is the single
crate that depends on both directions, so the applications take one dependency
and Cargo features decide what is compiled in.

## Choosing a backend

There is one selection site: `flash_core::BackendRegistry`. It routes on the
scheme of the probe identifier.

| Identifier | Goes to |
|---|---|
| `mock:stm32f401re` | the simulated backend |
| `esp:COM7` | the ESP serial backend |
| `openocd:localhost:6666` | the OpenOCD backend |
| `0483:374f:002E0037…` | the first backend registered — probe-rs |
| *(none given)* | the first backend registered — probe-rs |

A colon-separated probe identifier is deliberately *not* read as a scheme: a
scheme is lowercase letters, `-`, and `_` only, so `0483:…` falls through to the
fallback rather than looking for a backend called `0483`.

## Data flow of a flash

1. The application resolves connection parameters (CLI flags over profile over
   defaults in `flashgui-cli`; the connection panel in the GUI) into a
   `ConnectionConfig`.
2. `BackendRegistry::open_session` routes to a backend, which returns a
   `Box<dyn FlashSession>`.
3. `firmware-parser` turns the file into `MemorySegment`s carrying real
   addresses. An ELF contributes its `PT_LOAD` segments by physical address, so
   what gets written is what the linker script placed in flash.
4. `FlashManager::execute_flash` runs validate → erase → program → verify →
   reset against the session, emitting `FlashEvent`s as it goes.
5. The GUI pushes those events to the frontend as `flash:progress`,
   `flash:status`, and `flash:log`; the CLI renders them, or emits NDJSON under
   `--json`.

Batch mode wraps step 2–5 in a loop with detach/attach re-arming between boards;
it takes a session-opening closure rather than a backend type, so it works with
every backend without changes.

## Cancellation

Cancellation is cooperative. `AtomicBool` set by the application, polled by the
backend between units of work. A backend declares where it can actually act
through `FlashSession::can_interrupt(stage)`, and the UI offers Stop only for
those stages — probe-rs runs chip erase and flash download to completion inside
one driver call, so on real hardware only the verify pass can stop early. See
issue #8 for what happens when a UI offers a Stop the backend cannot honour.

## Adding a backend

See [backend-api.md](backend-api.md).
