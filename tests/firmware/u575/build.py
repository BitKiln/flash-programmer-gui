#!/usr/bin/env python3
"""
Builds the NUCLEO-U575ZI-Q test firmware fixture.

The build products are committed under tests/fixtures/, so this is only needed
when changing the firmware itself. Requires the Arm GNU toolchain; pass
--prefix to point at a specific installation.

    python build.py
    python build.py --prefix /opt/gcc-arm/bin/arm-none-eabi-
"""

import argparse
import hashlib
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.normpath(os.path.join(HERE, "..", "..", "fixtures"))
NAME = "valid_u575_blinky"

CFLAGS = [
    "-mcpu=cortex-m33",
    "-mthumb",
    "-O2",
    "-ffreestanding",
    "-nostdlib",
    "-Wall",
    "-Wextra",
]
SOURCES = ["startup.c", "main.c"]


def find_tool(prefix: str, tool: str) -> str:
    candidate = prefix + tool
    resolved = shutil.which(candidate)
    if resolved is None:
        sys.exit(
            f"error: {candidate} not found on PATH.\n"
            "Install the Arm GNU toolchain, or pass --prefix pointing at it."
        )
    return resolved


def digest(path: str) -> str:
    with open(path, "rb") as handle:
        return hashlib.sha256(handle.read()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--prefix",
        default="arm-none-eabi-",
        help="toolchain prefix (default: arm-none-eabi-)",
    )
    args = parser.parse_args()

    cc = find_tool(args.prefix, "gcc")
    objcopy = find_tool(args.prefix, "objcopy")

    elf = os.path.join(FIXTURES, NAME + ".elf")
    binary = os.path.join(FIXTURES, NAME + ".bin")

    subprocess.run(
        [cc, *CFLAGS, "-T", "link.ld", *SOURCES, "-o", elf],
        cwd=HERE,
        check=True,
    )
    subprocess.run([objcopy, "-O", "binary", elf, binary], cwd=HERE, check=True)

    for path in (elf, binary):
        print(f"{os.path.basename(path):28} {os.path.getsize(path):>7} B  sha256:{digest(path)[:16]}")


if __name__ == "__main__":
    main()
