# NUCLEO-U575ZI-Q test firmware

Source for the `valid_u575_blinky.elf` / `.bin` fixtures in `tests/fixtures/`.
The built artifacts are committed, so building this is only necessary when
changing the firmware.

## What it does

Blinks LD1 (green, PC7) using bare register writes — no HAL, no libc, no
startup library. It is deliberately small and dependency-free so it can be
rebuilt with nothing but the Arm GNU toolchain.

`main.c` includes a `filler` array purely to bring the image to roughly 19.5 KB,
so flash timings are comparable to a realistic firmware rather than to a
few hundred bytes.

## Why it exists

Two reasons the H753 test image could not cover:

1. **It runs on a U575.** Verifying the flash path end to end on that part
   needs an image that actually executes there.

2. **It separates load address from virtual address.** `link.ld` places
   `.data` in RAM (`> RAM AT > FLASH`), so the linker emits a second `PT_LOAD`
   whose physical address is in flash but whose virtual address is in RAM:

   ```
   LOAD 0x001000 0x08000000 0x08000000 0x04c0c 0x04c0c R E
   LOAD 0x006000 0x20000000 0x08004c0c 0x00008 0x00008 RW
   ```

   An ELF loader that programs `p_vaddr` instead of `p_paddr` would try to
   write initialised data to `0x20000000` — RAM, not flash — and the firmware
   would come up with uninitialised globals. `parse_elf` uses `p_paddr`, and
   `tests/fixtures` carries this ELF so the regression is covered without a
   board attached.

The two initialised globals (`blink_count` = `0xA5A5A5A5`, `marker` =
`0xDEADBEEF`) are recognisable byte patterns, so a flash dump at the load
address shows immediately whether `.data` landed where it should.

## Building

```bash
python build.py
```

Requires `arm-none-eabi-gcc` on PATH. Point at a specific installation with
`python build.py --prefix /path/to/arm-none-eabi-`. The build is reproducible:
rebuilding produces byte-identical fixtures.

## Verifying on hardware

With a NUCLEO-U575ZI-Q attached:

```bash
cargo run -p flashgui-cli -- flash tests/fixtures/valid_u575_blinky.elf -t auto
```

LD1 should blink. To confirm `.data` reached flash at its load address:

```bash
cargo run -p flash-core --example read_flash_head -- 0x08004c0c 16
```

Expect `A5 A5 A5 A5 EF BE AD DE` followed by erased bytes. Reading
`0x20000000` after the firmware has run shows `blink_count` incrementing past
its initialiser, which proves startup copied `.data` out of flash.
