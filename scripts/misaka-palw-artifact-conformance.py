#!/usr/bin/env python3
"""**Does a player open it?** — the half of the ADR-0078 demonstration no golden can answer.

The tree already proves two things about a derived artifact and neither of them is this one.
`misaka-palw-derive/corpus/<kind>/golden.json` pins `artifact_bytes` produced by the shipped Rust,
so a mismatch means the transformer moved: that is DETERMINISM.
`scripts/misaka-palw-derive-stranger.py` re-implements the transformer in Python and reproduces
those same bytes from the ADR's own preimages: that is INDEPENDENT DERIVATION. Both compare the
artifact with a number. Neither opens it.

A transformer can be perfectly deterministic, reproduced byte for byte by a stranger, pinned in a
golden — and emit a file no MIDI sequencer will play and no glTF loader will load. Every check in
the tree would stay green. "The model made a thing" is a claim about a FILE, and a file is a claim
about a parser, so this reads the bytes the way the format's own spec says a reader must, and
refuses on the first violation with the reason.

**It calls no project code, imports no project module, and re-implements no transformer.** It only
knows the three formats. That is what makes a pass here mean something the other two cannot say:
the producer and the checker have nothing in common but the file.

Usage:
    misaka-palw-artifact-conformance.py check <file> [<file> ...]   # by extension
    misaka-palw-artifact-conformance.py selftest                    # prove it rejects damage
    misaka-palw-artifact-conformance.py demo --derive-bin <path> --repo <path> [--out <dir>]

`demo` is the reproducible form: it drives `palw-derive` over the shipped corpus to produce a
MIDI, a GLB and an STL, then checks all three. `--dsl <file>` beside a MIDI additionally
cross-checks the note count against the DSL the transformer was given, which is the one place this
script is allowed to know something about the input.

Exit 0 = every file conformed. Exit 2 = a file did not, and the reason is on stderr.
Exit 3 = the selftest's non-vacuity check failed, which means this script accepts damage and its
passes are worthless.
"""

import glob
import json
import os
import struct
import subprocess
import sys
import tempfile


class NotConformant(Exception):
    pass


def need(cond, msg):
    if not cond:
        raise NotConformant(msg)


# -------------------------------------------------------------------------------------------
# Standard MIDI File 1.0
# -------------------------------------------------------------------------------------------
def check_midi(b, dsl=None):
    """Walk every event. A sequencer reads variable-length quantities and running status; a
    checker that only looked at `MThd` would pass a file that dies on the first delta."""
    need(len(b) >= 14 and b[:4] == b"MThd", "no MThd header")
    need(struct.unpack(">I", b[4:8])[0] == 6, "MThd length is not 6")
    fmt, ntrk, div = struct.unpack(">HHH", b[8:14])
    need(fmt in (0, 1, 2), f"format {fmt} is not 0, 1 or 2")
    need(div != 0, "division is zero")
    off, tracks, notes = 14, 0, 0

    def vlq(p):
        v = 0
        while True:
            need(p < len(b), "a variable-length quantity ran off the end")
            c = b[p]
            p += 1
            v = (v << 7) | (c & 0x7F)
            if not c & 0x80:
                return v, p

    while off < len(b):
        need(b[off:off + 4] == b"MTrk", f"expected MTrk at byte {off}")
        ln = struct.unpack(">I", b[off + 4:off + 8])[0]
        end = off + 8 + ln
        need(end <= len(b), f"track at {off} claims {ln} bytes and the file ends at {len(b)}")
        p, run, saw_eot = off + 8, None, False
        while p < end:
            _, p = vlq(p)
            need(p < end, "an event has a delta and no status")
            s = b[p]
            if s & 0x80:
                run = s
                p += 1
            else:
                s = run
            need(s is not None, "running status with no preceding status byte")
            if s == 0xFF:
                need(p < end, "a meta event ran off the track")
                mt = b[p]
                p += 1
                l, p = vlq(p)
                p += l
                if mt == 0x2F:
                    need(p == end, f"end-of-track lands at {p}, the chunk ends at {end}")
                    saw_eot = True
            elif s in (0xF0, 0xF7):
                l, p = vlq(p)
                p += l
            else:
                hi = s & 0xF0
                n = 1 if hi in (0xC0, 0xD0) else 2
                need(p + n <= end, "a channel event ran off the track")
                if hi == 0x90 and b[p + 1] > 0:
                    notes += 1
                p += n
        need(p == end, f"track overran its own length: {p} vs {end}")
        need(saw_eot, "a track has no end-of-track meta event")
        off = end
        tracks += 1
    need(tracks == ntrk, f"the header declares {ntrk} tracks and the file has {tracks}")
    detail = f"format {fmt}, {ntrk} track(s), {div} ppq, {notes} note-on(s)"
    if dsl is not None:
        want = sum(len(t["notes"]) for t in dsl["tracks"])
        need(notes == want, f"{notes} note-ons in the file, {want} in the DSL it was made from")
        detail += " == DSL"
    return detail


# -------------------------------------------------------------------------------------------
# glTF 2.0 binary (.glb)
# -------------------------------------------------------------------------------------------
def check_glb(b):
    """Loader-grade: a loader indexes accessor -> bufferView -> buffer and will read out of
    bounds, or refuse, if any containment fails. Checking the magic proves nothing."""
    need(len(b) >= 12, "shorter than a GLB header")
    magic, ver, total = struct.unpack("<III", b[:12])
    need(magic == 0x46546C67, "no glTF magic")
    need(ver == 2, f"version {ver} is not 2")
    need(total == len(b), f"header says {total} bytes, the file is {len(b)}")
    p, js, bin_ = 12, None, None
    while p < len(b):
        need(p + 8 <= len(b), "a chunk header ran off the end")
        cl, ct = struct.unpack("<II", b[p:p + 8])
        need(cl % 4 == 0, f"chunk length {cl} is not 4-byte aligned")
        need(p + 8 + cl <= len(b), "a chunk ran off the end")
        d = b[p + 8:p + 8 + cl]
        if ct == 0x4E4F534A:
            js = json.loads(d.decode("utf-8"))
        elif ct == 0x004E4942:
            bin_ = d
        p += 8 + cl
    need(js is not None, "no JSON chunk")
    need(bin_ is not None, "no BIN chunk")
    need(js.get("asset", {}).get("version") == "2.0", "asset.version is not 2.0")
    buf = js["buffers"][0]
    need(buf["byteLength"] <= len(bin_), "buffer byteLength exceeds the BIN chunk")
    sizes = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}
    comps = {5120: 1, 5121: 1, 5122: 2, 5123: 2, 5125: 4, 5126: 4}
    for i, a in enumerate(js["accessors"]):
        bv = js["bufferViews"][a["bufferView"]]
        need(a["type"] in sizes, f"accessor {i} has type {a['type']}")
        n = a["count"] * sizes[a["type"]] * comps[a["componentType"]]
        need(a.get("byteOffset", 0) + n <= bv["byteLength"], f"accessor {i} overruns its bufferView")
        need(bv.get("byteOffset", 0) + bv["byteLength"] <= buf["byteLength"],
             f"bufferView {a['bufferView']} overruns the buffer")
    prims = [pr for m in js["meshes"] for pr in m["primitives"]]
    need(prims, "no primitives")
    for pr in prims:
        need(pr.get("mode", 4) == 4, "a primitive is not TRIANGLES")
        need("POSITION" in pr["attributes"], "a primitive has no POSITION")
        if "indices" in pr:
            ia = js["accessors"][pr["indices"]]
            need(ia["count"] % 3 == 0, "index count is not a multiple of 3")
            nv = js["accessors"][pr["attributes"]["POSITION"]]["count"]
            need(ia["max"][0] < nv if "max" in ia else True, "an index is past the last vertex")
    scene = js["scenes"][js.get("scene", 0)]
    need(scene["nodes"], "scene 0 has no nodes")
    attrs = sorted({k for pr in prims for k in pr["attributes"]})
    return f"{len(js['meshes'])} mesh(es), {len(js['nodes'])} node(s), {len(js['accessors'])} accessor(s), attrs {attrs}"


# -------------------------------------------------------------------------------------------
# Binary STL
# -------------------------------------------------------------------------------------------
def check_stl(b):
    """The layout is exact: 80-byte header, u32 count, 50 bytes per triangle, nothing after.
    A slicer that trusts the count and finds short data reads garbage."""
    need(len(b) >= 84, "shorter than an STL header")
    n = struct.unpack("<I", b[80:84])[0]
    need(len(b) == 84 + 50 * n, f"file is {len(b)} bytes, not 84 + 50 x {n} = {84 + 50 * n}")
    need(n > 0, "no triangles")
    deg = 0
    for i in range(n):
        r = b[84 + 50 * i:84 + 50 * (i + 1)]
        vs = [struct.unpack("<3f", r[12 + 12 * j:24 + 12 * j]) for j in range(3)]
        if len(set(vs)) != 3:
            deg += 1
    need(deg == 0, f"{deg} triangle(s) have coincident vertices")
    return f"{n} triangle(s), 0 degenerate, exact 84+50n layout"


CHECKS = {".mid": check_midi, ".midi": check_midi, ".glb": check_glb, ".stl": check_stl}


def check_file(path, dsl=None):
    ext = os.path.splitext(path)[1].lower()
    if ext not in CHECKS:
        raise NotConformant(f"no conformance reader for {ext!r}")
    b = open(path, "rb").read()
    return check_midi(b, dsl) if CHECKS[ext] is check_midi else CHECKS[ext](b)


def cmd_check(argv):
    dsl = None
    files = []
    i = 0
    while i < len(argv):
        if argv[i] == "--dsl":
            dsl = json.load(open(argv[i + 1]))
            i += 2
        else:
            files.append(argv[i])
            i += 1
    bad = 0
    for f in files:
        try:
            print(f"  OK   {os.path.basename(f):<24} {len(open(f,'rb').read()):>8,} B  {check_file(f, dsl)}")
        except NotConformant as e:
            print(f"  FAIL {os.path.basename(f):<24} {e}", file=sys.stderr)
            bad += 1
    return 2 if bad else 0


def cmd_selftest():
    """**A validator that accepts damage makes every pass worthless**, so this proves it does not.

    Each case is a real artifact with one specific injury, and each MUST be refused. A reader that
    only checked magic bytes would pass every one of them.
    """
    cases = []
    # MIDI: a header that promises two tracks over a file that holds one.
    mid = (b"MThd" + struct.pack(">IHHH", 6, 1, 2, 192) + b"MTrk" + struct.pack(">I", 4)
           + b"\x00\xff\x2f\x00")
    cases.append(("midi: header declares 2 tracks, file has 1", ".mid", mid))
    # MIDI: end-of-track that does not land on the chunk boundary.
    body = b"\x00\xff\x2f\x00\x00"
    cases.append(("midi: end-of-track short of the chunk end", ".mid",
                  b"MThd" + struct.pack(">IHHH", 6, 0, 1, 192) + b"MTrk" + struct.pack(">I", len(body)) + body))
    # GLB: header length that disagrees with the file.
    js = json.dumps({"asset": {"version": "2.0"}, "buffers": [{"byteLength": 4}], "bufferViews": [],
                     "accessors": [], "meshes": [], "nodes": [], "scenes": [{"nodes": [0]}]}).encode()
    js += b" " * ((4 - len(js) % 4) % 4)
    bin_ = b"\x00\x00\x00\x00"
    glb = (struct.pack("<III", 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(bin_) + 4)
           + struct.pack("<II", len(js), 0x4E4F534A) + js
           + struct.pack("<II", len(bin_), 0x004E4942) + bin_)
    cases.append(("glb: declared total length is 4 bytes over", ".glb", glb))
    # STL: a count that promises more triangles than the file carries.
    cases.append(("stl: count says 3, file holds 1", ".stl", b"\x00" * 80 + struct.pack("<I", 3) + b"\x00" * 50))
    ok = True
    with tempfile.TemporaryDirectory() as d:
        for name, ext, blob in cases:
            p = os.path.join(d, "probe" + ext)
            open(p, "wb").write(blob)
            try:
                check_file(p)
                print(f"  ACCEPTED DAMAGE — {name}", file=sys.stderr)
                ok = False
            except NotConformant as e:
                print(f"  refused  {name}\n             -> {e}")
            except Exception as e:  # a traceback is not a refusal
                print(f"  CRASHED (not a named refusal) — {name}: {e!r}", file=sys.stderr)
                ok = False
    print("selftest: every injury refused by name" if ok else "selftest: FAILED")
    return 0 if ok else 3


def cmd_demo(argv):
    derive_bin = repo = None
    out = None
    i = 0
    while i < len(argv):
        k = argv[i]
        if k == "--derive-bin":
            derive_bin = argv[i + 1]
        elif k == "--repo":
            repo = argv[i + 1]
        elif k == "--out":
            out = argv[i + 1]
        else:
            print(f"unknown argument {k!r}", file=sys.stderr)
            return 2
        i += 2
    if not derive_bin or not repo:
        print("demo needs --derive-bin <path> and --repo <path>", file=sys.stderr)
        return 2
    out = out or tempfile.mkdtemp(prefix="palw-conformance-")
    plan = [("music/smf/v1", "music/03-overlapping-melody.json", "*.mid"),
            ("scene/glb/v1", "scene/02-hierarchy.json", "*.glb"),
            ("cad/stl/v1", "cad/01-extrude-l-bracket.json", "*.stl")]
    bad = 0
    for transformer, answer, pat in plan:
        d = os.path.join(out, transformer.split("/")[0])
        os.makedirs(d, exist_ok=True)
        src = os.path.join(repo, "misaka-palw-derive/corpus", answer)
        if not os.path.exists(src):
            print(f"  SKIP {transformer}: no corpus answer at {src}", file=sys.stderr)
            bad += 1
            continue
        r = subprocess.run([derive_bin, "derive", "--transformer", transformer, "--answer", src, "--out", d],
                           capture_output=True, text=True)
        if r.returncode != 0:
            print(f"  FAIL {transformer}: palw-derive refused: {r.stderr.strip()[:200]}", file=sys.stderr)
            bad += 1
            continue
        hits = glob.glob(os.path.join(d, "*.artifact" + os.path.splitext(pat)[1]))
        if not hits:
            print(f"  FAIL {transformer}: no artifact matching {pat} under {d}", file=sys.stderr)
            bad += 1
            continue
        dsl = json.load(open(src)) if transformer.startswith("music/") else None
        try:
            print(f"  OK   {transformer:<16} {os.path.basename(hits[0]):<34} "
                  f"{len(open(hits[0],'rb').read()):>8,} B  {check_file(hits[0], dsl)}")
        except NotConformant as e:
            print(f"  FAIL {transformer}: {e}", file=sys.stderr)
            bad += 1
    print(f"artifacts under {out}")
    return 2 if bad else 0


def main():
    argv = sys.argv[1:]
    if not argv:
        print(__doc__)
        return 2
    cmd, rest = argv[0], argv[1:]
    if cmd == "check":
        return cmd_check(rest)
    if cmd == "selftest":
        return cmd_selftest()
    if cmd == "demo":
        return cmd_demo(rest)
    print(f"unknown command {cmd!r}; try check | selftest | demo", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
