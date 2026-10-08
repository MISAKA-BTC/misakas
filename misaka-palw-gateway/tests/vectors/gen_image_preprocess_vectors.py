#!/usr/bin/env python3
"""Independent second implementation of misaka.palw.image-preprocess.v1 (RFC-0003 section II.4: the gateway resizes or letterboxes a
decoded image to a class slot's size with a fixed INTEGER algorithm). Writes image-preprocess-v1.json, the golden vectors the Rust
implementation (misaka-palw-gateway/src/preprocess.rs) is held to. No floats anywhere; Python ints are arbitrary precision, so there is no
overflow behaviour to agree with.

    python3 gen_image_preprocess_vectors.py > image-preprocess-v1.json
"""
import hashlib, json, sys

Q = 65536

def bilinear(src, sh, sw, dh, dw):
    out = bytearray(dh * dw * 3)
    ys = []
    for oy in range(dh):
        s = ((2 * oy + 1) * sh * Q) // (2 * dh) - Q // 2
        s = min(max(s, 0), (sh - 1) * Q)
        ys.append((s >> 16, min((s >> 16) + 1, sh - 1), s & 0xFFFF))
    xs = []
    for ox in range(dw):
        s = ((2 * ox + 1) * sw * Q) // (2 * dw) - Q // 2
        s = min(max(s, 0), (sw - 1) * Q)
        xs.append((s >> 16, min((s >> 16) + 1, sw - 1), s & 0xFFFF))
    for oy, (y0, y1, fy) in enumerate(ys):
        for ox, (x0, x1, fx) in enumerate(xs):
            for c in range(3):
                p00 = src[(y0 * sw + x0) * 3 + c]; p01 = src[(y0 * sw + x1) * 3 + c]
                p10 = src[(y1 * sw + x0) * 3 + c]; p11 = src[(y1 * sw + x1) * 3 + c]
                total = (p00 * (Q - fx) * (Q - fy) + p01 * fx * (Q - fy) + p10 * (Q - fx) * fy + p11 * fx * fy)
                out[(oy * dw + ox) * 3 + c] = (total + (1 << 31)) >> 32
    return bytes(out)

def stretch(src, sh, sw, dh, dw):
    return bilinear(src, sh, sw, dh, dw), (0, 0, dh, dw)

def letterbox(src, sh, sw, dh, dw, pad):
    if sw * dh >= sh * dw:
        nw = dw; nh = (sh * dw * 2 + sw) // (2 * sw)
    else:
        nh = dh; nw = (sw * dh * 2 + sh) // (2 * sh)
    nh = min(max(nh, 1), dh); nw = min(max(nw, 1), dw)
    oy = (dh - nh) // 2; ox = (dw - nw) // 2
    inner = bilinear(src, sh, sw, nh, nw)
    out = bytearray(bytes(pad) * (dh * dw))
    for y in range(nh):
        row = inner[y * nw * 3:(y + 1) * nw * 3]
        o = ((oy + y) * dw + ox) * 3
        out[o:o + nw * 3] = row
    return bytes(out), (oy, ox, nh, nw)

def pattern(h, w, seed):
    # A fixed, non-trivial pixel pattern (an LCG over the byte index): no randomness, no libraries.
    state = seed
    px = bytearray()
    for _ in range(h * w * 3):
        state = (state * 1103515245 + 12345) & 0x7FFFFFFF
        px.append((state >> 16) & 0xFF)
    return bytes(px)

def case(name, fit, sh, sw, dh, dw, src, pad=(0, 0, 0), digest_only=False):
    if fit == "stretch":
        out, placed = stretch(src, sh, sw, dh, dw)
    else:
        out, placed = letterbox(src, sh, sw, dh, dw, pad)
    c = {"name": name, "fit": fit, "pad": list(pad), "src_h": sh, "src_w": sw, "dst_h": dh, "dst_w": dw,
         "src_rgb_hex": src.hex(), "placed": list(placed),
         "out_sha256": hashlib.sha256(out).hexdigest()}
    if not digest_only:
        c["out_rgb_hex"] = out.hex()
    return c

def gray(values):
    return bytes(v for v in values for _ in range(3))

cases = [
    case("identity 2x3 stretch", "stretch", 2, 3, 2, 3, pattern(2, 3, 7)),
    case("identity 2x3 letterbox", "letterbox", 2, 3, 2, 3, pattern(2, 3, 7)),
    # Hand-derived (see the Rust test): [0,255] widened to four columns.
    case("1x2 gray widened to 1x4", "stretch", 1, 2, 1, 4, gray([0, 255])),
    # Hand-derived: four pixels averaged into one.
    case("2x2 gray averaged to 1x1", "stretch", 2, 2, 1, 1, gray([10, 20, 30, 40])),
    case("4x2 into 4x4 letterbox (pillarbox columns 1..3)", "letterbox", 4, 2, 4, 4, pattern(4, 2, 11), pad=(7, 8, 9)),
    case("2x4 into 4x4 letterbox (letterbox rows 1..3)", "letterbox", 2, 4, 4, 4, pattern(2, 4, 13), pad=(0, 0, 0)),
    case("3x5 into 4x4 letterbox (rounding 2.4 -> 2)", "letterbox", 3, 5, 4, 4, pattern(3, 5, 17), pad=(114, 114, 114)),
    case("5x3 up to 2x3 stretch (down)", "stretch", 5, 3, 2, 3, pattern(5, 3, 19)),
    case("2x3 up to 5x7 stretch (up)", "stretch", 2, 3, 5, 7, pattern(2, 3, 23)),
    case("37x53 to 16x16 letterbox (digest)", "letterbox", 37, 53, 16, 16, pattern(37, 53, 29), pad=(0, 0, 0), digest_only=False),
    case("224x224 to 56x56 stretch (digest only)", "stretch", 224, 224, 56, 56, pattern(224, 224, 31), digest_only=True),
    case("100x30 to 64x64 letterbox (digest only)", "letterbox", 100, 30, 64, 64, pattern(100, 30, 37), pad=(128, 128, 128), digest_only=True),
]
json.dump({"format": "misaka.palw.image-preprocess.vectors.v1", "algorithm": "misaka.palw.image-preprocess.v1", "cases": cases}, sys.stdout, indent=1)
print()
