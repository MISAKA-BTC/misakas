#!/usr/bin/env python3
"""Generate MISAKA Studio's application icons.

The icons are *generated* rather than committed as opaque binaries so that the mark can be
changed by editing four numbers instead of by opening a design tool, and so a reviewer can see
what is in the PNGs without decoding them. Run from this directory:

    python3 generate.py

It writes 32x32.png, 128x128.png, 128x128@2x.png and icon.png (1024, the source for
`npm run tauri icon`, which produces the .ico and .icns that Windows and macOS bundles need).

No third-party imaging library: a rounded square and three strokes is a hundred lines of
arithmetic, and the alternative is a build-time dependency on Pillow for an asset that changes
once a year.
"""

import struct
import zlib

# The mark: a rounded square in MISAKA's accent, with a white M.
BACKGROUND = (14, 138, 138)  # oklch(0.58 0.14 197) in sRGB, near enough
FOREGROUND = (255, 255, 255)
CORNER_RADIUS = 0.22  # of the side
STROKE_WIDTH = 0.085  # of the side
SUPERSAMPLE = 3  # 3x3 samples per pixel — enough for a clean edge at 32 px


def rounded_square_alpha(x: float, y: float, radius: float) -> bool:
    """True when (x, y), in a unit square, is inside the rounded square."""
    cx = min(max(x, radius), 1.0 - radius)
    cy = min(max(y, radius), 1.0 - radius)
    return (x - cx) ** 2 + (y - cy) ** 2 <= radius**2


def distance_to_segment(px: float, py: float, ax: float, ay: float, bx: float, by: float) -> float:
    dx, dy = bx - ax, by - ay
    length_squared = dx * dx + dy * dy
    if length_squared == 0:
        return ((px - ax) ** 2 + (py - ay) ** 2) ** 0.5
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / length_squared))
    return ((px - (ax + t * dx)) ** 2 + (py - (ay + t * dy)) ** 2) ** 0.5


# The four strokes of an M, in unit-square coordinates: up-left, down-middle, up-right.
M_STROKES = [
    ((0.26, 0.74), (0.26, 0.26)),
    ((0.26, 0.26), (0.50, 0.56)),
    ((0.50, 0.56), (0.74, 0.26)),
    ((0.74, 0.26), (0.74, 0.74)),
]


def sample(x: float, y: float) -> tuple[int, int, int, int]:
    if not rounded_square_alpha(x, y, CORNER_RADIUS):
        return (0, 0, 0, 0)
    for (ax, ay), (bx, by) in M_STROKES:
        if distance_to_segment(x, y, ax, ay, bx, by) <= STROKE_WIDTH / 2:
            return (*FOREGROUND, 255)
    return (*BACKGROUND, 255)


def render(size: int) -> bytes:
    """RGBA rows, supersampled."""
    rows = []
    step = 1.0 / (size * SUPERSAMPLE)
    for py in range(size):
        row = bytearray()
        for px in range(size):
            r = g = b = a = 0
            for sy in range(SUPERSAMPLE):
                for sx in range(SUPERSAMPLE):
                    x = (px * SUPERSAMPLE + sx + 0.5) * step
                    y = (py * SUPERSAMPLE + sy + 0.5) * step
                    sr, sg, sb, sa = sample(x, y)
                    # Premultiply so a transparent sample does not darken the average.
                    r += sr * sa
                    g += sg * sa
                    b += sb * sa
                    a += sa
            samples = SUPERSAMPLE * SUPERSAMPLE
            if a == 0:
                row.extend((0, 0, 0, 0))
            else:
                row.extend((round(r / a), round(g / a), round(b / a), round(a / samples)))
        rows.append(bytes(row))
    return b"".join(b"\x00" + row for row in rows)


def write_png(path: str, size: int) -> None:
    raw = render(size)

    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)  # 8-bit RGBA
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as handle:
        handle.write(png)
    print(f"wrote {path} ({size}x{size}, {len(png)} bytes)")


if __name__ == "__main__":
    write_png("32x32.png", 32)
    write_png("128x128.png", 128)
    write_png("128x128@2x.png", 256)
    write_png("icon.png", 1024)
