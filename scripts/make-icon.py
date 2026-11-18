#!/usr/bin/env python3
"""Generate build/icon.png (1024×1024 RGBA) with no image dependencies.

A warm coral tile, an ink play button and three "crop row" bars, matching the
in-app mark. Run with: python3 scripts/make-icon.py
"""

from __future__ import annotations

import math
import struct
import zlib
from pathlib import Path

SIZE = 1024
CENTRE = SIZE / 2
CORNER = 224.0


def clamp(value: float, low: float = 0.0, high: float = 1.0) -> float:
    return max(low, min(high, value))


def rounded_rect_distance(x: float, y: float, cx: float, cy: float, half: float, radius: float) -> float:
    """Signed distance to a rounded rectangle (negative inside)."""
    dx = abs(x - cx) - (half - radius)
    dy = abs(y - cy) - (half - radius)
    ax, ay = max(dx, 0.0), max(dy, 0.0)
    return math.hypot(ax, ay) + min(max(dx, dy), 0.0) - radius


def coverage(distance: float) -> float:
    """One-pixel antialiased coverage from a signed distance."""
    return clamp(0.5 - distance)


def triangle(x: float, y: float) -> float:
    a = (404.0, 314.0)
    b = (404.0, 710.0)
    c = (742.0, 512.0)
    area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    orientation = 1.0 if area > 0 else -1.0

    def edge(p1: tuple[float, float], p2: tuple[float, float]) -> float:
        """Positive inside the triangle, in pixels."""
        vx, vy = p2[0] - p1[0], p2[1] - p1[1]
        length = math.hypot(vx, vy) or 1.0
        signed = (vx * (y - p1[1]) - vy * (x - p1[0])) / length
        return signed * orientation

    inside = min(edge(a, b), edge(b, c), edge(c, a))
    return clamp(0.5 + inside)


def pixel(x: float, y: float) -> tuple[int, int, int, int]:
    # Transparent outside the squircle.
    outer = rounded_rect_distance(x, y, CENTRE, CENTRE, CENTRE, CORNER)
    alpha = coverage(outer)
    if alpha <= 0:
        return (0, 0, 0, 0)

    # Diagonal coral gradient with a soft highlight in the upper left.
    t = clamp((x * 0.55 + y * 0.45) / SIZE)
    r = int(255 + (206 - 255) * t)
    g = int(124 + (56 - 124) * t)
    b = int(92 + (38 - 92) * t)
    highlight = clamp(1.0 - math.hypot(x - SIZE * 0.3, y - SIZE * 0.2) / (SIZE * 0.75))
    r = int(r + (255 - r) * 0.22 * highlight)
    g = int(g + (255 - g) * 0.22 * highlight)
    b = int(b + (255 - b) * 0.22 * highlight)

    # Glass tile.
    tile = rounded_rect_distance(x, y, CENTRE, CENTRE, 236.0, 112.0)
    tile_cov = coverage(tile)
    if tile_cov > 0:
        r = int(r * (1 - 0.16 * tile_cov) + 255 * 0.16 * tile_cov)
        g = int(g * (1 - 0.16 * tile_cov) + 255 * 0.16 * tile_cov)
        b = int(b * (1 - 0.16 * tile_cov) + 255 * 0.16 * tile_cov)
        border = coverage(abs(tile + 4.0) - 4.0)
        if border > 0:
            r = int(r + (255 - r) * 0.38 * border)
            g = int(g + (255 - g) * 0.38 * border)
            b = int(b + (255 - b) * 0.38 * border)

    # Play glyph in warm ink.
    play = triangle(x, y)
    if play > 0:
        r = int(r + (28 - r) * play)
        g = int(g + (14 - g) * play)
        b = int(b + (10 - b) * play)

    # Three crop-row bars near the bottom.
    if 860 < y < 892:
        for start, width in ((300, 150), (502, 20), (574, 150)):
            if start <= x <= start + width:
                bar = clamp(0.5 + min(x - start, start + width - x, y - 864, 888 - y))
                if bar > 0:
                    r = int(r + (28 - r) * 0.62 * bar)
                    g = int(g + (14 - g) * 0.62 * bar)
                    b = int(b + (10 - b) * 0.62 * bar)

    return (clamp(r / 255) * 255, clamp(g / 255) * 255, clamp(b / 255) * 255, alpha * 255)


def chunk(tag: bytes, payload: bytes) -> bytes:
    return (
        struct.pack(">I", len(payload))
        + tag
        + payload
        + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)
    )


def main() -> None:
    rows = bytearray()
    for y in range(SIZE):
        rows.append(0)  # PNG filter type: none
        for x in range(SIZE):
            r, g, b, a = pixel(x + 0.5, y + 0.5)
            rows.extend((int(r), int(g), int(b), int(a)))

    header = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(rows), 9))
        + chunk(b"IEND", b"")
    )
    target = Path(__file__).resolve().parent.parent / "build" / "icon.png"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(png)
    print(f"wrote {target} ({len(png) / 1024:.0f} KB)")


if __name__ == "__main__":
    main()
