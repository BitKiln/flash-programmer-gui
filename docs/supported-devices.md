# Supported devices

Generated from `crates/device-db`. Do not edit by hand — run
`cargo run -p device-db --bin gen-supported-devices` instead.

Chip geometry (flash size, sector map, core) is **not** listed here: it comes
from the probe-rs target registry at runtime, which knows far more parts than
this table names. A part missing from this list is usually still programmable
— type its exact name into the target field.

| Vendor | Family | Example part | Backends |
|---|---|---|---|
| STMicroelectronics | STM32C0 | `STM32C031C6Tx` | `probe:` |
| STMicroelectronics | STM32F0 | `STM32F030C8Tx` | `probe:` |
| STMicroelectronics | STM32F1 | `STM32F103C8` | `probe:` |
| STMicroelectronics | STM32F2 | `STM32F205RGTx` | `probe:` |
| STMicroelectronics | STM32F3 | `STM32F303RETx` | `probe:` |
| STMicroelectronics | STM32F4 | `STM32F401RE` | `probe:` |
| STMicroelectronics | STM32F7 | `STM32F746ZGTx` | `probe:` |
| STMicroelectronics | STM32G0 | `STM32G071RB` | `probe:` |
| STMicroelectronics | STM32G4 | `STM32G474RE` | `probe:` |
| STMicroelectronics | STM32H5 | `STM32H563ZITx` | `probe:` |
| STMicroelectronics | STM32H7 | `STM32H753ZI` | `probe:` |
| STMicroelectronics | STM32L0 | `STM32L053R8Tx` | `probe:` |
| STMicroelectronics | STM32L4 | `STM32L476RG` | `probe:` |
| STMicroelectronics | STM32L5 | `STM32L552ZETx` | `probe:` |
| STMicroelectronics | STM32U0 | `STM32U083RCTx` | `probe:` |
| STMicroelectronics | STM32U5 | `STM32U575ZITx` | `probe:` |
| STMicroelectronics | STM32WB | `STM32WB55RGVx` | `probe:` |
| STMicroelectronics | STM32WL | `STM32WL55JCIx` | `probe:` |
| Silicon Labs | EFM32 | `EFM32PG22C200F512IM40` | `probe:` |
| Silicon Labs | EFR32 Blue Gecko | `EFR32BG22C224F512IM40` | `probe:` |
| Silicon Labs | EFR32 Flex Gecko | `EFR32FG23B010F512IM48` | `probe:` |
| Silicon Labs | EFR32 Mighty Gecko | `EFR32MG24B210F1536IM48` | `probe:` |
| Espressif | ESP32-C2 / ESP8684 | `esp32c2` | `esp:`, `probe:` |
| Espressif | ESP32-C3 / ESP8685 | `esp32c3` | `esp:`, `probe:` |
| Espressif | ESP32-C6 | `esp32c6` | `esp:`, `probe:` |
| Espressif | ESP32-H2 | `esp32h2` | `esp:`, `probe:` |
| Espressif | ESP32-P4 | `esp32p4` | `esp:`, `probe:` |
| Espressif | ESP32-S2 | `esp32s2` | `esp:`, `probe:` |
| Espressif | ESP32-S3 | `esp32s3` | `esp:`, `probe:` |
| Espressif | ESP32 | `esp32` | `esp:`, `probe:` |
| Raspberry Pi | RP2040 | `RP2040` | `probe:` |

## Backends

| Scheme | Backend | Reaches |
|---|---|---|
| `probe:` | probe-rs | ST-Link, CMSIS-DAP/DAPLink, and J-Link probes over SWD or JTAG |
| `esp:` | esp-serial | Espressif parts over the serial/USB ROM bootloader — no probe needed |
| `mock:` | simulated | Nothing physical — a NOR flash model for tests and demos |

## Notes

Espressif parts are listed against `probe:` as well as `esp:`, but probe-rs ships **no**
ESP chip descriptions, so the JTAG route needs one supplied at runtime with
`--target-yaml <path>` (from `probe-rs target-gen`, or esp-rs/esp-flash-loader). None is
bundled with this tool. The serial bootloader route needs nothing extra and is the
supported path.

Silicon Labs support has not yet been exercised on hardware.
