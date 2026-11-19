#!/usr/bin/env python3
"""Generate build/icon.png (1024x1024 RGBA) — clipfarm app icon.

Brand mark: a coral squircle with a soft upper-left highlight, containing a dark
rounded "clip card" with a play triangle, and three crop-row ticks along its
bottom edge (the signature "reframe to 9:16" motif). The same lockup is drawn
in theme.rs for the in-app sidebar/header wordmark.

Run with: python3 scripts/make-icon.py
"""

from __future__ import annotations

import math
import struct
import zlib
from pathlib import Path

SIZE = 1024
CENTRE = SIZE / 2

# Palette (matches theme.rs DARK accent / ink and LIGHT accent).
CORAL_TOP = (255, 124, 74)      # light coral (upper-left highlight side)
CORAL_BOT = (224, 76, 48)       # deep coral
INK       = (26, 12, 8)         # warm near-black for the clip card + glyph
GLASS     = (255, 255, 255)     # soft glass tile highlight
TICK_INK  = (26, 12, 8)


def clamp(value: float, low: float = 0.0, high: float = 1.0) -> float:
    return max(low, min(high, value))


def rounded_rect_distance(x: float, y: float, cx: float, cy: float,
                          half: float, radius: float) -> float:
    """Signed distance to a rounded rectangle (negative inside)."""
    dx = abs(x - cx) - (half - radius)
    dy = abs(y - cy) - (half - radius)
    ax, ay = max(dx, 0.0), max(dy, 0.0)
    return math.hypot(ax, ay) + min(max(dx, dy), 0.0) - radius


def coverage(distance: float) -> float:
    """One-pixel antialiased coverage from a signed distance."""
    return clamp(0.5 - distance)


def disk_distance(x: float, y: float, cx: float, cy: float, r: float) -> float:
    return math.hypot(x - cx, y - cy) - r


def rgba_clip_card(x: float, y: float) -> float:
    """Dark rounded "clip card" centered in the icon. Negative = inside."""
    card_half = 300.0
    card_radius = 78.0
    return rounded_rect_distance(x, y, CENTRE, CENTRE - 30.0, card_half, card_radius)


def play_triangle(x: float, y: float) -> float:
    """Right-pointing play triangle, inset inside the clip card."""
    # Triangle is sized relative to the card; keep it comfortably inside.
    cx, cy = CENTRE, CENTRE - 30.0
    w = 150.0
    h = 96.0
    a = (cx - w / 2.0, cy - h / 2.0)
    b = (cx - w / 2.0, cy + h / 2.0)
    c = (cx + w / 2.0 - h / 2.0, cy)
    area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    orient = 1.0 if area > 0 else -1.0

    def edge(p1, p2):
        vx, vy = p2[0] - p1[0], p2[1] - p1[1]
        length = math.hypot(vx, vy) or 1.0
        signed = (vx * (y - p1[1]) - vy * (x - p1[0])) / length
        return signed * orient

    inside = min(edge(a, b), edge(b, c), edge(c, a))
    return clamp(0.5 + inside)


def crop_tick(x: float, y: float, start: float, width: float,
              top: float, bottom: float) -> float:
    """Antialiased coverage of a rounded crop-row tick (1 inside, 0 outside)."""
    cx = start + width / 2.0
    half = width / 2.0
    rr = min(14.0, half)
    d = rounded_rect_distance(x, y, cx, (top + bottom) / 2.0, half, rr)
    return coverage(d)


def pixel(x: float, y: float) -> tuple[int, int, int, int]:
    # Transparent outside the squircle.
    outer = rounded_rect_distance(x, y, CENTRE, CENTRE, CENTRE, 224.0)
    alpha = coverage(outer)
    if alpha <= 0:
        return (0, 0, 0, 0)

    # Coral gradient across the squircle + soft upper-left highlight.
    t = clamp((x * 0.55 + y * 0.45) / SIZE)
    r = int(CORAL_TOP[0] + (CORAL_BOT[0] - CORAL_TOP[0]) * t)
    g = int(CORAL_TOP[1] + (CORAL_BOT[1] - CORAL_TOP[1]) * t)
    b = int(CORAL_TOP[2] + (CORAL_BOT[2] - CORAL_TOP[2]) * t)
    highlight = clamp(1.0 - math.hypot(x - SIZE * 0.30, y - SIZE * 0.22) / (SIZE * 0.72))
    r = int(r + (255 - r) * 0.24 * highlight)
    g = int(g + (255 - g) * 0.24 * highlight)
    b = int(b + (255 - b) * 0.24 * highlight)

    # Subtle inner glow near the center to lift the card.
    glow = clamp(1.0 - math.hypot(x - CENTRE, y - CENTRE + 30.0) / (SIZE * 0.5))
    r = int(r + (255 - r) * 0.08 * glow)
    g = int(g + (255 - g) * 0.08 * glow)
    b = int(b + (255 - b) * 0.08 * glow)

    # Clip card: warm near-black rounded rectangle.
    card = rgba_clip_card(x, y)
    card_cov = coverage(card)  # 1 inside, 0 outside, antialiased at the edge.
    if card_cov > 0:
        r = int(r + (INK[0] - r) * card_cov)
        g = int(g + (INK[1] - g) * card_cov)
        b = int(b + (INK[2] - b) * card_cov)

    # Very thin light rim on the card for definition against coral.
    card_rim = coverage(abs(card) - 3.0)
    if card_rim > 0:
        r = int(r + (255 - r) * 0.12 * card_rim)
        g = int(g + (255 - g) * 0.12 * card_rim)
        b = int(b + (255 - b) * 0.12 * card_rim)

    # Play triangle in a warm off-white (reads on the dark card).
    play = play_triangle(x, y)
    if play > 0:
        r = int(r + (250 - r) * play)
        g = int(g + (244 - g) * play)
        b = int(b + (236 - b) * play)

    # Three crop-row ticks along the bottom edge of the clip card.
    tick_top = (CENTRE - 30.0) + 232.0
    tick_bottom = tick_top + 46.0
    for start, width in ((330.0, 150.0), (512.0, 22.0), (594.0, 150.0)):
        tick = crop_tick(x, y, start, width, tick_top, tick_bottom)
        if tick > 0:
            r = int(r + (TICK_INK[0] - r) * 0.80 * tick)
            g = int(g + (TICK_INK[1] - g) * 0.80 * tick)
            b = int(b + (TICK_INK[2] - b) * 0.80 * tick)

    return (int(clamp(r / 255) * 255), int(clamp(g / 255) * 255),
            int(clamp(b / 255) * 255), int(alpha * 255))


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
