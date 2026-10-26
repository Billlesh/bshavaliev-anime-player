#!/usr/bin/env python3
"""Write a tiny original play-mark icon as PNG + ICO. No third-party deps."""

from __future__ import annotations

import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "src-tauri" / "icons"


def png_rgba(width: int, height: int, pixels: bytes) -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + pixels[y * width * 4 : (y + 1) * width * 4] for y in range(height))
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def raster(size: int) -> bytes:
    px = bytearray(size * size * 4)
    bg = (18, 19, 24, 255)
    amber = (217, 160, 90, 255)
    for y in range(size):
        for x in range(size):
            i = (y * size + x) * 4
            px[i : i + 4] = bytes(bg)
    # Play triangle, inset.
    x0, x1 = int(size * 0.32), int(size * 0.78)
    for y in range(int(size * 0.22), int(size * 0.78)):
        t = (y - size * 0.22) / (size * 0.56)
        left = x0
        right = int(x0 + (x1 - x0) * (1.0 - abs(t * 2 - 1) * 0.05) * (t if t < 0.5 else 1 - t) * 2)
        # Linear triangle: left edge fixed, right interpolates.
        yn = (y - size * 0.22) / max(1, size * 0.56)
        half = yn if yn < 0.5 else 1 - yn
        right = int(x0 + (x1 - x0) * (half * 2))
        for x in range(left, max(left + 1, right)):
            i = (y * size + x) * 4
            px[i : i + 4] = bytes(amber)
    return bytes(px)


def ico_from_png(png: bytes, size: int) -> bytes:
    entry = struct.pack(
        "<BBBBHHII",
        size if size < 256 else 0,
        size if size < 256 else 0,
        0,
        0,
        1,
        32,
        len(png),
        6 + 16,
    )
    return struct.pack("<HHH", 0, 1, 1) + entry + png


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for size, name in ((32, "32x32.png"), (128, "128x128.png"), (256, "icon.png")):
        png = png_rgba(size, size, raster(size))
        (OUT / name).write_bytes(png)
    png32 = png_rgba(32, 32, raster(32))
    (OUT / "icon.ico").write_bytes(ico_from_png(png32, 32))
    print(f"wrote icons in {OUT}")


if __name__ == "__main__":
    main()
