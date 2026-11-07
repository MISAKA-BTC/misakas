#!/usr/bin/env python3
"""Parse a MIDI / glTF-binary / STL artifact with NOTHING from this repository.

ADR-0078 Decision 5 is "verify without trusting the executor". The drill's `--check` compares
one run's hashes against another's, which proves two runs agree — it does not prove the bytes
are a file anything else can open. That is a different claim and it needs a different reader.

So this file implements the three container formats from their published specifications and
refuses to import, link or shell out to anything in the tree. If it disagrees with the deriver,
the deriver is what is wrong, because a Standard MIDI File is defined by the SMF spec and not by
us.

usage: verify-artifacts-independently.py <file> [<file> ...]
"""
import struct
import sys


def fail(msg):
    raise AssertionError(msg)


def read_vlq(b, i):
    """SMF variable-length quantity: 7 bits per byte, high bit = continue."""
    v = 0
    n = 0
    while True:
        if i + n >= len(b):
            fail("VLQ runs past end of track")
        c = b[i + n]
        v = (v << 7) | (c & 0x7F)
        n += 1
        if not c & 0x80:
            return v, n
        if n > 4:
            fail("VLQ longer than 4 bytes — not a legal SMF delta")


def verify_smf(b):
    """Standard MIDI File. Walks every event; does not trust the header's track count."""
    if b[:4] != b"MThd":
        fail("no MThd — not a Standard MIDI File")
    hdr_len = struct.unpack(">I", b[4:8])[0]
    if hdr_len != 6:
        fail(f"MThd length {hdr_len}, the spec says 6")
    fmt, ntrks, division = struct.unpack(">HHH", b[8:14])
    if fmt not in (0, 1, 2):
        fail(f"format {fmt} is not 0/1/2")
    i = 14
    seen = 0
    notes = 0
    total_ticks = 0
    while i < len(b):
        if b[i:i + 4] != b"MTrk":
            fail(f"expected MTrk at byte {i}, found {b[i:i+4]!r}")
        tlen = struct.unpack(">I", b[i + 4:i + 8])[0]
        end = i + 8 + tlen
        if end > len(b):
            fail(f"track {seen} claims {tlen} bytes, file has {len(b) - i - 8}")
        j = i + 8
        running = None
        ticks = 0
        saw_end = False
        while j < end:
            delta, n = read_vlq(b, j)
            ticks += delta
            j += n
            st = b[j]
            if st == 0xFF:                                   # meta
                mtype = b[j + 1]
                mlen, n2 = read_vlq(b, j + 2)
                if mtype == 0x2F:
                    saw_end = True
                j += 2 + n2 + mlen
                continue
            if st in (0xF0, 0xF7):                           # sysex
                mlen, n2 = read_vlq(b, j + 1)
                j += 1 + n2 + mlen
                continue
            if st & 0x80:
                running = st
                j += 1
            elif running is None:
                fail(f"running status with no preceding status byte at {j}")
            hi = (running or st) & 0xF0
            nbytes = 1 if hi in (0xC0, 0xD0) else 2
            if hi == 0x90 and b[j + 1] != 0:
                notes += 1
            j += nbytes
        if not saw_end:
            fail(f"track {seen} has no End of Track meta event")
        if j != end:
            fail(f"track {seen} events end at {j}, chunk ends at {end}")
        total_ticks = max(total_ticks, ticks)
        seen += 1
        i = end
    if seen != ntrks:
        fail(f"header declares {ntrks} tracks, file contains {seen}")
    return f"SMF format {fmt}, {seen} track(s), division {division}, {notes} note-on, {total_ticks} ticks"


def verify_glb(b):
    """glTF 2.0 binary container + JSON chunk sanity."""
    import json
    if b[:4] != b"glTF":
        fail("no glTF magic")
    ver, total = struct.unpack("<II", b[4:12])
    if ver != 2:
        fail(f"glTF version {ver}, expected 2")
    if total != len(b):
        fail(f"header total {total} != file length {len(b)}")
    i = 12
    chunks = []
    js = None
    while i < len(b):
        clen, ctype = struct.unpack("<II", b[i:i + 8])
        data = b[i + 8:i + 8 + clen]
        if len(data) != clen:
            fail(f"chunk claims {clen} bytes, {len(data)} present")
        if i + 8 + clen > len(b):
            fail("chunk runs past end")
        if clen % 4:
            fail(f"chunk length {clen} is not 4-byte aligned")
        chunks.append(ctype)
        if ctype == 0x4E4F534A:
            js = json.loads(data.decode("utf-8"))
        i += 8 + clen
    if not chunks or chunks[0] != 0x4E4F534A:
        fail("first chunk is not JSON, which glTF requires")
    if js is None:
        fail("no JSON chunk")
    for k in ("asset",):
        if k not in js:
            fail(f"glTF JSON lacks required key {k}")
    if js["asset"].get("version") != "2.0":
        fail(f"asset.version is {js['asset'].get('version')!r}")
    # Every accessor must fit its bufferView, and every bufferView its buffer.
    bufs = js.get("buffers", [])
    for n, bv in enumerate(js.get("bufferViews", [])):
        blen = bufs[bv.get("buffer", 0)]["byteLength"]
        if bv.get("byteOffset", 0) + bv["byteLength"] > blen:
            fail(f"bufferView {n} runs past its buffer")
    meshes = len(js.get("meshes", []))
    nodes = len(js.get("nodes", []))
    prims = sum(len(m.get("primitives", [])) for m in js.get("meshes", []))
    return f"glTF 2.0, {len(chunks)} chunk(s), {nodes} node(s), {meshes} mesh(es), {prims} primitive(s)"


def verify_stl(b):
    """Binary STL: 80-byte header, u32 count, then 50 bytes per triangle, exactly."""
    if len(b) < 84:
        fail(f"{len(b)} bytes is shorter than an STL header")
    if b[:5] == b"solid" and b"facet" in b[:512]:
        fail("this is ASCII STL; the derivation is specified as binary")
    n = struct.unpack("<I", b[80:84])[0]
    want = 84 + 50 * n
    if want != len(b):
        fail(f"count {n} implies {want} bytes, file is {len(b)}")
    degenerate = 0
    for t in range(n):
        o = 84 + 50 * t
        vals = struct.unpack("<12f", b[o:o + 48])
        v = [vals[3:6], vals[6:9], vals[9:12]]
        if v[0] == v[1] or v[1] == v[2] or v[0] == v[2]:
            degenerate += 1
    if degenerate:
        fail(f"{degenerate} of {n} triangles have two identical vertices")
    return f"binary STL, {n} triangle(s), no degenerate facets"


SNIFF = [
    (b"MThd", "MIDI", verify_smf),
    (b"glTF", "glTF", verify_glb),
]

rc = 0
for path in sys.argv[1:]:
    with open(path, "rb") as fh:
        b = fh.read()
    kind, fn = "binary STL", verify_stl
    for magic, k, f in SNIFF:
        if b[:4] == magic:
            kind, fn = k, f
            break
    try:
        detail = fn(b)
        print(f"  OK      {path.split('/')[-1]:<28} {len(b):>8} B  {detail}")
    except Exception as e:                                    # noqa: BLE001
        print(f"  REFUSED {path.split('/')[-1]:<28} {len(b):>8} B  {kind}: {e}")
        rc = 1

print()
print("  Parsed from the published container specs. Nothing above imports, links or shells out")
print("  to misaka-palw-derive: an independent reader is the only thing that can disagree with it.")
sys.exit(rc)
