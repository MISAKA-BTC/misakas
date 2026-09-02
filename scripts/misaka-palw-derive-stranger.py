#!/usr/bin/env python3
"""**The stranger's recomputation** — ADR-0078 Decision 5 / X6, in a language the producer does
not use.

`palw-derive verify` and `misaka palw derived-verify` already re-run a derivation and compare it
with the chain. Both call `misaka_palw_derive::verify` — the producer's own function. That is a
useful check and a weak proof: a transformer that is wrong in the same way twice agrees with
itself. Decision 5's promise is that *anyone holding the answer* can demonstrate a false object,
and the demonstration only means something when the second computation is a second computation.

So this file re-derives, from scratch and from the ADR's own preimages, everything the chain
carries about a derivation:

```text
  source_tree_sha256 = SHA-256( for each file under misaka-palw-derive/src, sorted by path:
                                 path \0 len(u64 le) \0 bytes )
  transformer_id     = H_transformer( manifest canonical bytes )
  grammar_id         = H_grammar( name )
  canonical DSL      = the grammar's canonicalizer, re-implemented here
  dsl_hash           = H_dsl( grammar_id ‖ len(u64 le) ‖ canonical DSL )
  artifact bytes     = the transformer, re-implemented here
  artifact_hash      = H_artifact( artifact bytes )
  output_root        = H_output( job_context_hash ‖ u32 count ‖ ids ‖ family rendered hash )
  derived_id         = H_id( borsh(PalwDerivedArtifactV1) )
```

Every `H_x` is BLAKE2b-512 keyed by that field's domain constant, with the length framing
`kaspa_consensus_core::palw_derived_v1` uses. Nothing here imports, links or shells out to the
Rust crate.

**What makes this trustworthy rather than merely different.** `selftest` checks it against three
oracles that already live in the tree, and needs no build to do it:

  1. `misaka-palw-derive/tests/transformer_id_pin.rs` — the pinned `source_tree_sha256` and the
     eight pinned `transformer_id`s. Recomputing two of those ids from the manifest bytes proves
     the framing, the field order and the source-tree walk are all right.
  2. `misaka-palw-derive/corpus/<kind>/golden.json` — `dsl_hash`, `artifact_hash` and
     `artifact_bytes` for every corpus answer, produced by the shipped Rust. Reproducing them
     byte for byte proves the canonicalizer and the transformer.
  3. The refusal corpus (`9x-*.json`) — answers the shipped grammar refuses, which this
     implementation must also refuse rather than quietly derive.

A `selftest` that passes is what earns this file the right to be called an independent path.

## Coverage, stated rather than implied

Two kinds are implemented — `music/smf/v1` in full, and `cad/stl/v1` for the `box`, `union`,
`intersection` and `difference` operations (the axis-aligned CSG lattice). `extrude` and
`revolve` are NOT implemented: their kernels are an ear-clipping triangulator and an exact
rational direction ring, and a half-right re-implementation of those would produce a mismatch
that reads like a false object. They are refused BY NAME (`Unimplemented`), and `verify` exits 4
on one — never a pass, and never a silent fallback to the producer's own code.

## Usage

```text
  misaka-palw-derive-stranger.py selftest [--crate-root <misaka-palw-derive>]
  misaka-palw-derive-stranger.py transformers
  misaka-palw-derive-stranger.py derive --transformer <name> --answer <file> [--out <dir>]
  misaka-palw-derive-stranger.py verify --chain <misaka-palw-derived.json> --answer <answer file>
                                        [--gateway <chat response.json>] [--artifact <file>]
```

Exit codes: 0 consistent, 2 MISMATCH (a demonstrable false object), 3 the inputs do not admit a
check, 4 this kind is outside the implemented subset.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import struct
import sys

# ---------------------------------------------------------------------------------------------
# The domains. Byte-for-byte from `consensus/core/src/palw_derived_v1.rs`,
# `consensus/core/src/palw_v2.rs` and `misaka-palw-base0/src/qwen*_backend.rs`.
# ---------------------------------------------------------------------------------------------

DOMAIN_DERIVED_ID = b"misaka-palw/derived-v1/derived-id/v1"
DOMAIN_GRAMMAR_ID = b"misaka-palw/derived-v1/grammar-id/v1"
DOMAIN_TRANSFORMER_ID = b"misaka-palw/derived-v1/transformer-id/v1"
DOMAIN_DSL_HASH = b"misaka-palw/derived-v1/dsl-hash/v1"
DOMAIN_ARTIFACT_HASH = b"misaka-palw/derived-v1/artifact-hash/v1"
DOMAIN_OUTPUT = b"misaka-palw/output/v2"
DOMAIN_QWEN25_A16_EXECUTION = b"misaka-palw/qwen25-a16/execution/v1"
DOMAIN_QWEN36_EXECUTION = b"misaka-palw/qwen36/execution/v1"

PALW_DERIVED_V1_VERSION = 1
PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN = 2592

FAMILY_DOMAINS = {
    "qwen25-a16": DOMAIN_QWEN25_A16_EXECUTION,
    "qwen36": DOMAIN_QWEN36_EXECUTION,
}


class Refused(Exception):
    """The grammar or the transformer refuses this answer (ADR-0078 X4: no object)."""


class Unimplemented(Exception):
    """Outside this file's implemented subset — never a pass, never a fallback."""


def keyed(domain: bytes) -> "hashlib._Hash":
    return hashlib.blake2b(digest_size=64, key=domain)


def canonical_id(domain: bytes, data: bytes) -> bytes:
    h = keyed(domain)
    h.update(struct.pack("<Q", len(data)))
    h.update(data)
    return h.digest()


def grammar_id_v1(name: str) -> bytes:
    return canonical_id(DOMAIN_GRAMMAR_ID, name.encode("utf-8"))


def transformer_id_v1(manifest_bytes: bytes) -> bytes:
    return canonical_id(DOMAIN_TRANSFORMER_ID, manifest_bytes)


def dsl_hash_v1(grammar_id: bytes, canonical_dsl: bytes) -> bytes:
    h = keyed(DOMAIN_DSL_HASH)
    h.update(grammar_id)
    h.update(struct.pack("<Q", len(canonical_dsl)))
    h.update(canonical_dsl)
    return h.digest()


def artifact_hash_v1(artifact: bytes) -> bytes:
    return canonical_id(DOMAIN_ARTIFACT_HASH, artifact)


def rendered_output_hash_v1(family: str, ids: list) -> bytes:
    """`misaka_palw_base0::<family>_backend::rendered_output_hash_v1` — keyed, each part framed."""
    domain = FAMILY_DOMAINS.get(family)
    if domain is None:
        raise Refused(f"unknown family {family!r}: this verifier knows {sorted(FAMILY_DOMAINS)}")
    h = keyed(domain)
    for part in (b"rendered", b"".join(struct.pack("<I", int(t)) for t in ids)):
        h.update(struct.pack("<Q", len(part)))
        h.update(part)
    return h.digest()


def output_commitment_v2(job_context_hash: bytes, ids: list, rendered: bytes) -> bytes:
    """`kaspa_consensus_core::palw_v2::output_commitment_v2` — the CanonicalWriter buffer, keyed."""
    buf = bytearray()
    buf += job_context_hash
    buf += struct.pack("<I", len(ids))
    for t in ids:
        buf += struct.pack("<I", int(t))
    buf += rendered
    return canonical_id(DOMAIN_OUTPUT, bytes(buf))


def derived_id_v1(obj: dict) -> bytes:
    """`derived_id_v1 = H(borsh(PalwDerivedArtifactV1))`.

    Borsh for this struct is field order, little-endian scalars, `Hash64` as its 64 raw bytes
    (a newtype over `[u8; 64]`), and `Vec<u8>` as a `u32` length then the bytes.
    """
    b = bytearray()
    b += struct.pack("<H", obj["version"])
    for field in ("network_domain", "claim_id", "output_root", "grammar_id", "transformer_id"):
        h = obj[field]
        if len(h) != 64:
            raise Refused(f"{field} is {len(h)} bytes, not a 64-byte Hash64")
        b += h
    b += struct.pack("<H", obj["kind"])
    for field in ("dsl_hash", "artifact_hash"):
        b += obj[field]
    b += struct.pack("<Q", obj["artifact_bytes"])
    b += struct.pack("<I", len(obj["executor_pubkey"]))
    b += obj["executor_pubkey"]
    return canonical_id(DOMAIN_DERIVED_ID, bytes(b))


# ---------------------------------------------------------------------------------------------
# The source-tree hash (`misaka-palw-derive/src/source_tree.rs`), recomputed from a checkout.
# ---------------------------------------------------------------------------------------------


def source_files(crate_root: str) -> list:
    """Every non-dot regular file under `src/`, at any depth, path relative to the crate root,
    `/`-separated, globally sorted."""
    out = []
    src = os.path.join(crate_root, "src")
    for dirpath, dirnames, filenames in os.walk(src):
        dirnames[:] = [d for d in dirnames if not d.startswith(".")]
        for name in filenames:
            if name.startswith("."):
                continue
            full = os.path.join(dirpath, name)
            if not os.path.isfile(full):
                continue
            out.append(os.path.relpath(full, crate_root).replace(os.sep, "/"))
    out.sort()
    return out


def source_tree_sha256_hex(crate_root: str) -> str:
    files = source_files(crate_root)
    if not files:
        raise Refused(f"no source files under {crate_root}/src: this is not a checkout of the crate")
    pre = bytearray()
    for rel in files:
        with open(os.path.join(crate_root, rel), "rb") as fh:
            body = fh.read()
        pre += rel.encode("utf-8")
        pre += b"\x00"
        pre += struct.pack("<Q", len(body))
        pre += b"\x00"
        pre += body
    return hashlib.sha256(bytes(pre)).hexdigest()


# ---------------------------------------------------------------------------------------------
# canon_json — `misaka-palw-derive/src/canon_json.rs`, re-implemented.
#
# Integers only (no float has a canonical text and none has an integer transformer downstream),
# no duplicate keys at any depth (serde keeps the last, which is a semantic choice this layer
# must not make), keys sorted by their UTF-8 bytes, no whitespace, RFC 8785 string escapes.
# ---------------------------------------------------------------------------------------------

I64_MIN = -(1 << 63)
U64_MAX = (1 << 64) - 1


def _no_duplicates(pairs):
    seen = set()
    out = {}
    for k, v in pairs:
        if k in seen:
            raise Refused(f"duplicate key {k!r}")
        seen.add(k)
        out[k] = v
    return out


def _no_constants(text):
    raise Refused(f"{text} has no canonical form")


def parse_canonical(data: bytes):
    """Parse to a tree of `int | str | bool | None | list | dict`, refusing what has no canonical
    form. Mirrors `parse_canonical` + `convert`."""
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        raise Refused("input is not UTF-8")
    try:
        value = json.loads(text, object_pairs_hook=_no_duplicates, parse_constant=_no_constants)
    except Refused:
        raise
    except Exception as e:  # noqa: BLE001 — any parse failure is one refusal (X4)
        raise Refused(f"json: {e}")
    _reject_non_integers(value)
    return value


def _reject_non_integers(v):
    if isinstance(v, bool) or v is None or isinstance(v, str):
        return
    if isinstance(v, float):
        raise Refused(f"non-integer number {v} has no canonical form")
    if isinstance(v, int):
        # serde_json admits `i64` then `u64`; anything outside both is not a canonical integer.
        if not (I64_MIN <= v <= U64_MAX):
            raise Refused(f"non-integer number {v} has no canonical form")
        return
    if isinstance(v, list):
        for item in v:
            _reject_non_integers(item)
        return
    if isinstance(v, dict):
        for item in v.values():
            _reject_non_integers(item)
        return
    raise Refused(f"value of type {type(v).__name__} has no canonical form")


_SHORT_ESCAPES = {0x08: b"\\b", 0x0C: b"\\f", 0x0A: b"\\n", 0x0D: b"\\r", 0x09: b"\\t"}


def write_string(s: str, out: bytearray) -> None:
    out += b'"'
    for ch in s:
        code = ord(ch)
        if ch == '"':
            out += b'\\"'
        elif ch == "\\":
            out += b"\\\\"
        elif code in _SHORT_ESCAPES:
            out += _SHORT_ESCAPES[code]
        elif code < 0x20:
            out += ("\\u%04x" % code).encode("ascii")
        else:
            out += ch.encode("utf-8")
    out += b'"'


def write_canonical(v, out: bytearray = None) -> bytes:
    if out is None:
        out = bytearray()
        write_canonical(v, out)
        return bytes(out)
    if v is None:
        out += b"null"
    elif v is True:
        out += b"true"
    elif v is False:
        out += b"false"
    elif isinstance(v, int):
        out += str(v).encode("ascii")
    elif isinstance(v, str):
        write_string(v, out)
    elif isinstance(v, list):
        out += b"["
        for n, item in enumerate(v):
            if n:
                out += b","
            write_canonical(item, out)
        out += b"]"
    elif isinstance(v, dict):
        out += b"{"
        # `BTreeMap<String, _>` orders by the key's UTF-8 bytes.
        for n, k in enumerate(sorted(v, key=lambda s: s.encode("utf-8"))):
            if n:
                out += b","
            write_string(k, out)
            out += b":"
            write_canonical(v[k], out)
        out += b"}"
    else:
        raise Refused(f"value of type {type(v).__name__} has no canonical form")
    return out


# ---------------------------------------------------------------------------------------------
# Schema helpers shared by both kinds — the `exact_keys` / `integer_in` discipline.
# ---------------------------------------------------------------------------------------------


def obj_of(v, what: str) -> dict:
    if not isinstance(v, dict):
        raise Refused(f"{what} is not an object")
    return v


def exact_keys(o: dict, expected, what: str) -> None:
    for k in o:
        if k not in expected:
            raise Refused(f"{what}: unknown key {k!r}")
    for k in expected:
        if k not in o:
            raise Refused(f"{what}: missing key {k!r}")


def integer(v, what: str) -> int:
    if isinstance(v, bool) or not isinstance(v, int):
        raise Refused(f"{what} must be an integer")
    return v


def integer_in(v, lo: int, hi: int, what: str) -> int:
    i = integer(v, what)
    if not (lo <= i <= hi):
        raise Refused(f"{what} {i} is outside {lo}..={hi}")
    return i


def integer_one_of(v, allowed, what: str) -> int:
    i = integer(v, what)
    if i not in allowed:
        raise Refused(f"{what} {i} is not one of {', '.join(str(a) for a in allowed)}")
    return i


# ---------------------------------------------------------------------------------------------
# kind `music` — `misaka-palw-derive/src/kinds/music.rs`.
# ---------------------------------------------------------------------------------------------

MUSIC_PPQ_ALLOWED = (96, 192, 480, 960)
MUSIC_TEMPO_MAX = 0xFFFFFF
MUSIC_TIME_SIG_NUMERATOR_MAX = 32
MUSIC_TIME_SIG_DENOMINATORS = (1, 2, 4, 8, 16, 32)
MUSIC_TRACKS_MAX = 64
MUSIC_NOTES_MAX_TOTAL = 65_536
MUSIC_TRACK_NAME_MAX_BYTES = 64
MUSIC_TICK_END_MAX = 1 << 28
MUSIC_MAX_DSL_BYTES = 4 << 20  # PALW_FP_DSL_V1_MAX_BYTES
MUSIC_ARTIFACT_MAX_BYTES = 16 << 20
MUSIC_VLQ_MAX = 0x0FFFFFFF
MUSIC_NOTE_OFF_VELOCITY = 0x40

META_TRACK_NAME = 0x03
META_END_OF_TRACK = 0x2F
META_TEMPO = 0x51
META_TIME_SIGNATURE = 0x58
TIME_SIGNATURE_CLOCKS_PER_CLICK = 24
TIME_SIGNATURE_32NDS_PER_QUARTER = 8
STATUS_NOTE_OFF = 0x80
STATUS_NOTE_ON = 0x90
STATUS_PROGRAM_CHANGE = 0xC0
EVENT_NOTE_OFF = 0
EVENT_NOTE_ON = 1


def music_parse_song(v):
    top = obj_of(v, "top level")
    exact_keys(top, ("v", "ppq", "tempo_us_per_quarter", "time_signature", "tracks"), "top level")
    version = integer(top["v"], "v")
    if version != 1:
        raise Refused(f"v must be 1, not {version}")
    ppq = integer_one_of(top["ppq"], MUSIC_PPQ_ALLOWED, "ppq")
    tempo = integer_in(top["tempo_us_per_quarter"], 1, MUSIC_TEMPO_MAX, "tempo_us_per_quarter")
    ts = top["time_signature"]
    if not isinstance(ts, list):
        raise Refused("time_signature must be an array")
    if len(ts) != 2:
        raise Refused(f"time_signature must be [numerator, denominator], not {len(ts)} items")
    numerator = integer_in(ts[0], 1, MUSIC_TIME_SIG_NUMERATOR_MAX, "time_signature numerator")
    denominator = integer_one_of(ts[1], MUSIC_TIME_SIG_DENOMINATORS, "time_signature denominator")
    tracks_in = top["tracks"]
    if not isinstance(tracks_in, list):
        raise Refused("tracks must be an array")
    if not tracks_in or len(tracks_in) > MUSIC_TRACKS_MAX:
        raise Refused(f"tracks must hold 1..={MUSIC_TRACKS_MAX} tracks, not {len(tracks_in)}")
    tracks = []
    notes_total = 0
    for ti, tv in enumerate(tracks_in):
        what = f"track {ti}"
        t = obj_of(tv, what)
        exact_keys(t, ("name", "channel", "program", "notes"), what)
        name = t["name"]
        if not isinstance(name, str):
            raise Refused(f"{what} name must be a string")
        if len(name.encode("utf-8")) > MUSIC_TRACK_NAME_MAX_BYTES:
            raise Refused(f"{what} name is too long")
        channel = integer_in(t["channel"], 0, 15, f"{what} channel")
        program = integer_in(t["program"], 0, 127, f"{what} program")
        notes_in = t["notes"]
        if not isinstance(notes_in, list):
            raise Refused(f"{what} notes must be an array")
        notes_total += len(notes_in)
        if notes_total > MUSIC_NOTES_MAX_TOTAL:
            raise Refused(f"more than {MUSIC_NOTES_MAX_TOTAL} notes in all")
        notes = []
        for ni, nv in enumerate(notes_in):
            nwhat = f"track {ti} note {ni}"
            n = obj_of(nv, nwhat)
            exact_keys(n, ("pitch", "velocity", "onset", "duration"), nwhat)
            pitch = integer_in(n["pitch"], 0, 127, f"{nwhat} pitch")
            velocity = integer_in(n["velocity"], 1, 127, f"{nwhat} velocity")
            onset = integer_in(n["onset"], 0, MUSIC_TICK_END_MAX - 1, f"{nwhat} onset")
            duration = integer_in(n["duration"], 1, MUSIC_TICK_END_MAX - 1, f"{nwhat} duration")
            if onset + duration > MUSIC_TICK_END_MAX:
                raise Refused(f"{nwhat} ends at tick {onset + duration}, past {MUSIC_TICK_END_MAX}")
            notes.append({"pitch": pitch, "velocity": velocity, "onset": onset, "duration": duration})
        tracks.append({"name": name, "channel": channel, "program": program, "notes": notes})
    return {
        "ppq": ppq,
        "tempo_us_per_quarter": tempo,
        "numerator": numerator,
        "denominator": denominator,
        "tracks": tracks,
    }


def encode_vlq(value: int, out: bytearray) -> None:
    groups = []
    v = value
    while True:
        groups.append(v & 0x7F)
        v >>= 7
        if v == 0:
            break
    while groups:
        g = groups.pop()
        out.append(g if not groups else (g | 0x80))


def _meta(out: bytearray, ty: int, data: bytes) -> None:
    out.append(0x00)
    out.append(0xFF)
    out.append(ty)
    encode_vlq(len(data), out)
    out += data


def music_track_events(track):
    """`(tick, class, pitch, velocity)`, sorted — a note-off precedes a note-on at one tick."""
    events = []
    for n in track["notes"]:
        events.append((n["onset"], EVENT_NOTE_ON, n["pitch"], n["velocity"]))
        events.append((n["onset"] + n["duration"], EVENT_NOTE_OFF, n["pitch"], MUSIC_NOTE_OFF_VELOCITY))
    events.sort()
    return events


def music_write_smf(song) -> bytes:
    body0 = bytearray()
    _meta(body0, META_TRACK_NAME, b"tempo")
    t = song["tempo_us_per_quarter"]
    _meta(body0, META_TEMPO, bytes([(t >> 16) & 0xFF, (t >> 8) & 0xFF, t & 0xFF]))
    dd = (song["denominator"]).bit_length() - 1  # trailing_zeros of a power of two
    _meta(
        body0,
        META_TIME_SIGNATURE,
        bytes([song["numerator"], dd, TIME_SIGNATURE_CLOCKS_PER_CLICK, TIME_SIGNATURE_32NDS_PER_QUARTER]),
    )
    _meta(body0, META_END_OF_TRACK, b"")

    out = bytearray()
    out += b"MThd"
    out += struct.pack(">I", 6)
    out += struct.pack(">H", 1)
    out += struct.pack(">H", 1 + len(song["tracks"]))
    out += struct.pack(">H", song["ppq"])
    out += b"MTrk"
    out += struct.pack(">I", len(body0))
    out += body0
    for track in song["tracks"]:
        body = bytearray()
        _meta(body, META_TRACK_NAME, track["name"].encode("utf-8"))
        body.append(0x00)
        body.append(STATUS_PROGRAM_CHANGE | track["channel"])
        body.append(track["program"])
        previous = 0
        for tick, cls, pitch, velocity in music_track_events(track):
            delta = tick - previous
            if delta > MUSIC_VLQ_MAX:
                raise Refused(f"delta time {delta} exceeds the variable-length quantity ceiling")
            encode_vlq(delta, body)
            previous = tick
            status = STATUS_NOTE_OFF if cls == EVENT_NOTE_OFF else STATUS_NOTE_ON
            body.append(status | track["channel"])
            body.append(pitch)
            body.append(velocity)
        _meta(body, META_END_OF_TRACK, b"")
        out += b"MTrk"
        out += struct.pack(">I", len(body))
        out += body
    if len(out) > MUSIC_ARTIFACT_MAX_BYTES:
        raise Refused(f"artifact is {len(out)} bytes; at most {MUSIC_ARTIFACT_MAX_BYTES}")
    return bytes(out)


def music_canonicalize(answer: bytes) -> bytes:
    if len(answer) > MUSIC_MAX_DSL_BYTES:
        raise Refused(f"the answer is {len(answer)} bytes; at most {MUSIC_MAX_DSL_BYTES} (ADR-0078 SA-2)")
    value = parse_canonical(answer)
    music_parse_song(value)
    return write_canonical(value)


def music_run(dsl: bytes) -> bytes:
    if len(dsl) > MUSIC_MAX_DSL_BYTES:
        raise Refused(f"the dsl is {len(dsl)} bytes; at most {MUSIC_MAX_DSL_BYTES} (ADR-0078 SA-2)")
    value = parse_canonical(dsl)
    song = music_parse_song(value)
    if write_canonical(value) != dsl:
        raise Refused("input is not canonical music/v1: the bytes differ from their canonical form")
    return music_write_smf(song)


# ---------------------------------------------------------------------------------------------
# kind `cad` — `misaka-palw-derive/src/kinds/cad.rs`, the axis-aligned CSG subset.
# ---------------------------------------------------------------------------------------------

CAD_FRAC_BITS_MAX = 16
CAD_COORD_MAX = (1 << 24) - 1
CAD_SKETCHES_MAX = 32
CAD_SKETCH_NAME_MAX_BYTES = 64
CAD_SKETCH_POINTS_MIN = 3
CAD_SKETCH_POINTS_MAX = 256
CAD_SOLID_NODES_MAX = 64
CAD_SOLID_DEPTH_MAX = 24
CAD_MAX_DSL_BYTES = 64 << 10
CAD_MAX_ARTIFACT_BYTES = 1 << 20
CAD_MAX_STEPS = 4_000_000
CAD_STL_HEADER_TEXT = b"misaka-palw cad/stl/v1; normals zero; right-hand winding; no timestamp"
CAD_STL_FACET_BYTES = 50
CAD_STL_PREAMBLE_BYTES = 84
CAD_TRIANGLES_MAX = (CAD_MAX_ARTIFACT_BYTES - CAD_STL_PREAMBLE_BYTES) // CAD_STL_FACET_BYTES
CAD_REVOLVE_SEGMENTS_MIN = 3
CAD_REVOLVE_SEGMENTS_MAX = 64
CAD_BOOLEAN_LEAVES_MAX = 32  # only used to name a refusal; the exact value is re-derived below


def cad_coordinate(v, what: str) -> int:
    return integer_in(v, -CAD_COORD_MAX, CAD_COORD_MAX, what)


def cad_integer_array(v, n: int, what: str):
    if not isinstance(v, list):
        raise Refused(f"{what} must be an array")
    if len(v) != n:
        raise Refused(f"{what} must hold {n} integers, not {len(v)}")
    return [cad_coordinate(item, f"{what}[{i}]") for i, item in enumerate(v)]


def cad_parse_node(v, what: str, depth: int, counter: list, sketches: dict):
    if depth > CAD_SOLID_DEPTH_MAX:
        raise Refused(f"{what} nests deeper than {CAD_SOLID_DEPTH_MAX}")
    counter[0] += 1
    if counter[0] > CAD_SOLID_NODES_MAX:
        raise Refused(f"the solid tree holds more than {CAD_SOLID_NODES_MAX} nodes")
    o = obj_of(v, what)
    op = o.get("op")
    if not isinstance(op, str):
        raise Refused(f"{what}: op must be a string")
    if op == "box":
        exact_keys(o, ("op", "min", "max"), what)
        mn = cad_integer_array(o["min"], 3, f"{what} min")
        mx = cad_integer_array(o["max"], 3, f"{what} max")
        for axis, name in enumerate(("x", "y", "z")):
            if mn[axis] >= mx[axis]:
                raise Refused(f"{what}: min {name} {mn[axis]} is not below max {name} {mx[axis]}")
        return {"op": "box", "min": mn, "max": mx}
    if op in ("union", "difference", "intersection"):
        exact_keys(o, ("op", "a", "b"), what)
        a = cad_parse_node(o["a"], f"{what}.a", depth + 1, counter, sketches)
        b = cad_parse_node(o["b"], f"{what}.b", depth + 1, counter, sketches)
        return {"op": op, "a": a, "b": b}
    if op in ("extrude", "revolve"):
        # Parsed far enough to REFUSE by name; the kernel is deliberately not re-implemented.
        raise Unimplemented(
            f"cad op {op!r} is outside this verifier's implemented subset (the ear-clipping "
            f"triangulator and the exact rational direction ring are not re-implemented here). "
            f"Verify this derivation with `palw-derive verify`, and read the result as the WEAKER "
            f"claim it is: the producer's own code agreeing with itself."
        )
    raise Refused(f"{what}: unknown op {op!r}")


def cad_parse_model(v):
    top = obj_of(v, "top level")
    exact_keys(top, ("v", "frac_bits", "sketches", "solid"), "top level")
    version = integer(top["v"], "v")
    if version != 1:
        raise Refused(f"v must be 1, not {version}")
    frac_bits = integer_in(top["frac_bits"], 0, CAD_FRAC_BITS_MAX, "frac_bits")
    sketches_in = obj_of(top["sketches"], "sketches")
    if len(sketches_in) > CAD_SKETCHES_MAX:
        raise Refused(f"sketches holds {len(sketches_in)} sketches; at most {CAD_SKETCHES_MAX}")
    sketches = {}
    for name, points_v in sketches_in.items():
        if not name or len(name.encode("utf-8")) > CAD_SKETCH_NAME_MAX_BYTES:
            raise Refused(f"sketch name {name!r} has an illegal length")
        what = f"sketch {name!r}"
        if not isinstance(points_v, list):
            raise Refused(f"{what} must be an array of points")
        if not (CAD_SKETCH_POINTS_MIN <= len(points_v) <= CAD_SKETCH_POINTS_MAX):
            raise Refused(f"{what} holds {len(points_v)} points")
        sketches[name] = [cad_integer_array(p, 2, f"{what} point {i}") for i, p in enumerate(points_v)]
    counter = [0]
    solid = cad_parse_node(top["solid"], "solid", 0, counter, sketches)
    return {"frac_bits": frac_bits, "sketches": sketches, "solid": solid}


def _cad_csg_of(node, boxes: list):
    op = node["op"]
    if op == "box":
        boxes.append((node["min"], node["max"]))
        return ("leaf", len(boxes) - 1)
    return (op, _cad_csg_of(node["a"], boxes), _cad_csg_of(node["b"], boxes))


def _cad_inside(csg, boxes, centre2):
    tag = csg[0]
    if tag == "leaf":
        mn, mx = boxes[csg[1]]
        return all(2 * mn[k] < centre2[k] < 2 * mx[k] for k in range(3))
    a = _cad_inside(csg[1], boxes, centre2)
    if tag == "union":
        return a or _cad_inside(csg[2], boxes, centre2)
    if tag == "intersection":
        return a and _cad_inside(csg[2], boxes, centre2)
    return a and not _cad_inside(csg[2], boxes, centre2)


def _cad_push_quad(out, p0, p1, p2, p3):
    out.append((p0, p1, p2))
    out.append((p0, p2, p3))


def _cad_push_lattice_face(out, axis, positive, lo, hi):
    u, v = (axis + 1) % 3, (axis + 2) % 3
    plane = hi[axis] if positive else lo[axis]

    def corner(uu, vv):
        p = [0, 0, 0]
        p[axis] = plane
        p[u] = uu
        p[v] = vv
        return tuple(p)

    p0, p1, p2, p3 = corner(lo[u], lo[v]), corner(hi[u], lo[v]), corner(hi[u], hi[v]), corner(lo[u], hi[v])
    if positive:
        _cad_push_quad(out, p0, p1, p2, p3)
    else:
        _cad_push_quad(out, p0, p3, p2, p1)


def _cad_csg_mesh(node):
    boxes = []
    csg = _cad_csg_of(node, boxes)
    planes = []
    for axis in range(3):
        vals = sorted({b[0][axis] for b in boxes} | {b[1][axis] for b in boxes})
        planes.append(vals)
    counts = [len(planes[a]) - 1 for a in range(3)]
    inside = {}
    for i in range(counts[0]):
        for j in range(counts[1]):
            for k in range(counts[2]):
                centre2 = [
                    planes[0][i] + planes[0][i + 1],
                    planes[1][j] + planes[1][j + 1],
                    planes[2][k] + planes[2][k + 1],
                ]
                inside[(i, j, k)] = _cad_inside(csg, boxes, centre2)
    if not any(inside.values()):
        raise Refused("the boolean's result is empty; there is no solid to write")
    out = []
    for i in range(counts[0]):
        for j in range(counts[1]):
            for k in range(counts[2]):
                if not inside[(i, j, k)]:
                    continue
                cell = (i, j, k)
                lo = [planes[0][i], planes[1][j], planes[2][k]]
                hi = [planes[0][i + 1], planes[1][j + 1], planes[2][k + 1]]
                for axis in range(3):
                    for positive in (False, True):
                        neighbour = None
                        if positive:
                            if cell[axis] + 1 < counts[axis]:
                                neighbour = cell[axis] + 1
                        elif cell[axis] > 0:
                            neighbour = cell[axis] - 1
                        filled = False
                        if neighbour is not None:
                            c = list(cell)
                            c[axis] = neighbour
                            filled = inside[tuple(c)]
                        if not filled:
                            _cad_push_lattice_face(out, axis, positive, lo, hi)
    return out


def _cad_triangle_cross(t):
    a, b, c = t
    u = [b[i] - a[i] for i in range(3)]
    v = [c[i] - a[i] for i in range(3)]
    return (u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0])


def _cad_canonical_rotation(t):
    if t[0] <= t[1] and t[0] <= t[2]:
        start = 0
    elif t[1] <= t[2]:
        start = 1
    else:
        start = 2
    return (t[start], t[(start + 1) % 3], t[(start + 2) % 3])


def _cad_canonical_mesh(raw):
    if not raw:
        raise Refused("the kernel produced no triangles; there is no solid to write")
    if len(raw) > CAD_TRIANGLES_MAX:
        raise Refused(f"the mesh holds {len(raw)} triangles; at most {CAD_TRIANGLES_MAX}")
    out = []
    for t in raw:
        if _cad_triangle_cross(t) == (0, 0, 0):
            raise Refused(f"triangle {t} has no area")
        out.append(_cad_canonical_rotation(t))
    out.sort()
    for a, b in zip(out, out[1:]):
        if a == b:
            raise Refused(f"triangle {a} appears twice; the mesh holds an internal wall")
    return out


def _cad_check_closed_oriented(triangles):
    edges = {}
    for t in triangles:
        for k in range(3):
            e = (t[k], t[(k + 1) % 3])
            edges[e] = edges.get(e, 0) + 1
    for (a, b), count in edges.items():
        if count != 1:
            raise Refused(f"the directed edge {a} → {b} appears {count} times")
        if edges.get((b, a), 0) != 1:
            raise Refused(f"the edge {a} → {b} has no matching reverse; the surface is not closed")


def _cad_six_times_volume(triangles) -> int:
    total = 0
    for a, b, c in triangles:
        total += (
            a[0] * (b[1] * c[2] - b[2] * c[1])
            - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0])
        )
    return total


def f32_bits_exact(value: int, frac_bits: int) -> int:
    """`misaka-palw-derive/src/fixed.rs` — the binary32 bit pattern of `value / 2^frac_bits`,
    built with integers only, refusing anything the format cannot hold exactly."""
    if value == 0:
        return 0
    sign = 1 << 31 if value < 0 else 0
    mag = abs(value)
    tz = (mag & -mag).bit_length() - 1
    msb = mag.bit_length() - 1
    significant = msb - tz + 1
    if significant > 24:
        raise Refused(f"{value}/2^{frac_bits} needs {significant} significant bits; binary32 holds 24")
    exp = msb - frac_bits
    biased = exp + 127
    if not (1 <= biased <= 254):
        raise Refused(f"{value}/2^{frac_bits} exponent {exp} is outside the normal binary32 range")
    if msb >= 23:
        mant = (mag >> (msb - 23)) & 0x7FFFFF
    else:
        mant = (mag << (23 - msb)) & 0x7FFFFF
    return sign | (biased << 23) | mant


def cad_write_stl(triangles, frac_bits: int) -> bytes:
    if len(triangles) > CAD_TRIANGLES_MAX:
        raise Refused(f"the mesh holds {len(triangles)} triangles; at most {CAD_TRIANGLES_MAX}")
    out = bytearray()
    header = bytearray(80)
    header[: len(CAD_STL_HEADER_TEXT)] = CAD_STL_HEADER_TEXT
    out += header
    out += struct.pack("<I", len(triangles))
    for t in triangles:
        out += b"\x00" * 12
        for vertex in t:
            for coordinate in vertex:
                out += struct.pack("<I", f32_bits_exact(coordinate, frac_bits))
        out += struct.pack("<H", 0)
    if len(out) > CAD_MAX_ARTIFACT_BYTES:
        raise Refused(f"artifact is {len(out)} bytes; at most {CAD_MAX_ARTIFACT_BYTES} (ADR-0078 SA-2)")
    return bytes(out)


def cad_mesh(model):
    raw = _cad_csg_mesh(model["solid"])
    triangles = _cad_canonical_mesh(raw)
    _cad_check_closed_oriented(triangles)
    six_v = _cad_six_times_volume(triangles)
    if six_v <= 0:
        raise Refused(f"the mesh's exact signed volume is {six_v}/6, which is not positive")
    return triangles


def cad_canonicalize(answer: bytes) -> bytes:
    if len(answer) > CAD_MAX_DSL_BYTES:
        raise Refused(f"the answer is {len(answer)} bytes; at most {CAD_MAX_DSL_BYTES} (ADR-0078 SA-2)")
    value = parse_canonical(answer)
    cad_parse_model(value)
    return write_canonical(value)


def cad_run(dsl: bytes) -> bytes:
    if len(dsl) > CAD_MAX_DSL_BYTES:
        raise Refused(f"the dsl is {len(dsl)} bytes; at most {CAD_MAX_DSL_BYTES} (ADR-0078 SA-2)")
    value = parse_canonical(dsl)
    model = cad_parse_model(value)
    if write_canonical(value) != dsl:
        raise Refused("input is not canonical cad/v1: the bytes differ from their canonical form")
    return cad_write_stl(cad_mesh(model), model["frac_bits"])


# ---------------------------------------------------------------------------------------------
# The registry: the two manifests this verifier re-implements, verbatim from their `manifest()`.
# ---------------------------------------------------------------------------------------------

KIND_CAD = 3
KIND_MUSIC = 6

TRANSFORMERS = {
    "cad/stl/v1": {
        "name": "cad/stl/v1",
        "kind": KIND_CAD,
        "grammar": "cad/v1",
        "discipline": "exact-rational",
        "writer": "stl-binary/1.0/zero-normal-rh-winding-sorted-v1",
        "max_dsl_bytes": CAD_MAX_DSL_BYTES,
        "max_artifact_bytes": CAD_MAX_ARTIFACT_BYTES,
        "max_steps": CAD_MAX_STEPS,
        "canonicalize": cad_canonicalize,
        "run": cad_run,
        "extension": "stl",
    },
    "music/smf/v1": {
        "name": "music/smf/v1",
        "kind": KIND_MUSIC,
        "grammar": "music/v1",
        "discipline": "integer",
        "writer": "standard-midi-file/1.0/canonical-v1",
        "max_dsl_bytes": MUSIC_MAX_DSL_BYTES,
        "max_artifact_bytes": MUSIC_ARTIFACT_MAX_BYTES,
        "max_steps": MUSIC_NOTES_MAX_TOTAL,
        "canonicalize": music_canonicalize,
        "run": music_run,
        "extension": "mid",
    },
}


def manifest_bytes(m: dict, source_tree_sha256: str) -> bytes:
    """`ids::transformer_manifest_bytes` — five length-prefixed strings, then the kind and the
    three SA-2 ceilings. The ceilings are the TAIL, so a loosened bound cannot keep the id."""
    out = bytearray()
    for field in (m["name"], m["grammar"], m["discipline"], m["writer"], source_tree_sha256):
        b = field.encode("utf-8")
        out += struct.pack("<Q", len(b))
        out += b
    out += struct.pack("<H", m["kind"])
    out += struct.pack("<Q", m["max_dsl_bytes"])
    out += struct.pack("<Q", m["max_artifact_bytes"])
    out += struct.pack("<Q", m["max_steps"])
    return bytes(out)


def derive(transformer_name: str, answer: bytes, source_tree_sha256: str) -> dict:
    m = TRANSFORMERS.get(transformer_name)
    if m is None:
        raise Unimplemented(
            f"{transformer_name!r} is outside this verifier's implemented subset "
            f"({', '.join(sorted(TRANSFORMERS))})"
        )
    canonical = m["canonicalize"](answer)
    artifact = m["run"](canonical)
    gid = grammar_id_v1(m["grammar"])
    return {
        "transformer": m["name"],
        "kind": m["kind"],
        "grammar": m["grammar"],
        "grammar_id": gid,
        "transformer_id": transformer_id_v1(manifest_bytes(m, source_tree_sha256)),
        "canonical_dsl": canonical,
        "dsl_hash": dsl_hash_v1(gid, canonical),
        "artifact": artifact,
        "artifact_hash": artifact_hash_v1(artifact),
        "extension": m["extension"],
    }


# ---------------------------------------------------------------------------------------------
# selftest — the three oracles already in the tree.
# ---------------------------------------------------------------------------------------------


def _pinned_from_test_file(path: str):
    """Read `SOURCE_TREE` and the eight `(name, id)` pins out of `transformer_id_pin.rs`.

    Parsed rather than copied: a pin this file duplicated would be a second spelling, and this
    repository's own record is that a second spelling is where two truths diverge.
    """
    import re

    with open(path, "r", encoding="utf-8") as fh:
        text = fh.read()
    m = re.search(r'const SOURCE_TREE: &str = "([0-9a-f]{64})"', text)
    if not m:
        raise Refused(f"{path} does not spell `const SOURCE_TREE`")
    pins = dict(re.findall(r'\("([a-z0-9/]+)",\s*"([0-9a-f]{128})"\)', text))
    if not pins:
        raise Refused(f"{path} carries no transformer id pins")
    return m.group(1), pins


def cmd_selftest(args) -> int:
    root = args.crate_root
    pin_file = os.path.join(root, "tests", "transformer_id_pin.rs")
    failures = []
    checked = 0

    print("== oracle 1: the source-tree hash and the transformer id pins ==")
    pinned_tree, pinned_ids = _pinned_from_test_file(pin_file)
    computed_tree = source_tree_sha256_hex(root)
    checked += 1
    if computed_tree == pinned_tree:
        print(f"  source_tree_sha256  {computed_tree}  MATCHES the pin")
    else:
        failures.append(f"source_tree_sha256: recomputed {computed_tree}, pinned {pinned_tree}")
        print(f"  source_tree_sha256  recomputed {computed_tree}\n                      pinned     {pinned_tree}  MISMATCH")
    for name, m in sorted(TRANSFORMERS.items()):
        checked += 1
        got = transformer_id_v1(manifest_bytes(m, computed_tree)).hex()
        want = pinned_ids.get(name)
        if want is None:
            failures.append(f"{name}: not in the pin file")
        elif got == want:
            print(f"  {name:<14} {got[:32]}…  MATCHES the pin")
        else:
            failures.append(f"{name}: recomputed {got}, pinned {want}")
            print(f"  {name:<14} recomputed {got[:32]}…\n{'':<16} pinned     {want[:32]}…  MISMATCH")

    print("== oracle 2: the corpus goldens (dsl_hash, artifact_hash, artifact_bytes) ==")
    for name, m in sorted(TRANSFORMERS.items()):
        kind_dir = os.path.join(root, "corpus", name.split("/")[0])
        golden_path = os.path.join(kind_dir, "golden.json")
        if not os.path.isfile(golden_path):
            failures.append(f"{name}: no golden.json at {golden_path}")
            continue
        with open(golden_path, "r", encoding="utf-8") as fh:
            golden = json.load(fh)
        for entry in sorted(golden):
            path = os.path.join(kind_dir, entry)
            if not os.path.isfile(path):
                failures.append(f"{name}/{entry}: golden names a file that is not there")
                continue
            with open(path, "rb") as fh:
                answer = fh.read()
            checked += 1
            # A golden entry is either a derivation or a REFUSAL the shipped tree records by its
            # own words (`{"refused": "grammar: …"}`). Both are things this path must reproduce:
            # a verifier that derived what the shipped grammar refuses would pass an object no
            # chain would have accepted.
            expect_refusal = "refused" in golden[entry]
            try:
                d = derive(name, answer, computed_tree)
            except Unimplemented as e:
                print(f"  {entry:<44} SKIPPED (outside the subset): {str(e).split('(')[0].strip()}")
                checked -= 1
                continue
            except Refused as e:
                if not expect_refusal:
                    failures.append(f"{name}/{entry}: refused, but the golden holds a derivation: {e}")
                    print(f"  {entry:<44} REFUSED but the golden derives: {e}")
                    continue
                # The shipped `Display` prefixes each arm with its own name (`grammar: `,
                # `transformer: `); the reason behind it is what the two implementations share.
                want_text = golden[entry]["refused"].split(": ", 1)[-1]
                if str(e) == want_text:
                    print(f"  {entry:<44} refused with the SAME reason as the shipped tree")
                else:
                    failures.append(
                        f"{name}/{entry}: refused for a different reason — this path says {str(e)!r}, "
                        f"the shipped tree says {want_text!r}"
                    )
                    print(f"  {entry:<44} refused, but for a different reason")
                continue
            if expect_refusal:
                failures.append(f"{name}/{entry}: derived an answer the shipped tree refuses")
                print(f"  {entry:<44} DERIVED — the shipped tree refuses this one")
                continue
            want = golden[entry]
            ok = (
                d["dsl_hash"].hex() == want["dsl_hash"]
                and d["artifact_hash"].hex() == want["artifact_hash"]
                and len(d["artifact"]) == want["artifact_bytes"]
            )
            if ok:
                print(f"  {entry:<44} {len(d['artifact']):>7} B  dsl_hash+artifact_hash MATCH")
            else:
                failures.append(
                    f"{name}/{entry}: dsl {d['dsl_hash'].hex()[:16]} vs {want['dsl_hash'][:16]}, "
                    f"artifact {d['artifact_hash'].hex()[:16]} vs {want['artifact_hash'][:16]}, "
                    f"bytes {len(d['artifact'])} vs {want['artifact_bytes']}"
                )
                print(f"  {entry:<44} MISMATCH")

    print("== oracle 3: nothing in the corpus derives that the golden does not name ==")
    for name in sorted(TRANSFORMERS):
        kind_dir = os.path.join(root, "corpus", name.split("/")[0])
        golden_path = os.path.join(kind_dir, "golden.json")
        golden = {}
        if os.path.isfile(golden_path):
            with open(golden_path, "r", encoding="utf-8") as fh:
                golden = json.load(fh)
        stray = [e for e in sorted(os.listdir(kind_dir)) if e.endswith(".json") and e != "golden.json" and e not in golden]
        checked += 1
        if stray:
            failures.append(f"{name}: corpus files with no golden entry: {stray}")
            print(f"  {name:<14} corpus files the golden does not name: {stray}")
        else:
            print(f"  {name:<14} every corpus file has a golden entry ({len(golden)})")

    print("== oracle 4: the `verify` path itself, round-tripped and then tampered with ==")
    # `verify` is what the drill's stage 10 runs. A verifier whose arithmetic is right and whose
    # COMPARISON is wrong would pass a false object, so the comparison is exercised here on a
    # synthetic chain read built from a corpus answer: once as the chain would carry it (must be
    # `consistent`), and once with one hash moved by a byte (must be a MISMATCH, exit 2). Without
    # the second half the first proves only that the function returns zero.
    import argparse as _argparse
    import tempfile

    for name in sorted(TRANSFORMERS):
        kind_dir = os.path.join(root, "corpus", name.split("/")[0])
        answer_file = None
        for entry in sorted(os.listdir(kind_dir)):
            if entry.endswith(".json") and entry != "golden.json" and not entry.startswith("9"):
                try:
                    with open(os.path.join(kind_dir, entry), "rb") as fh:
                        derive(name, fh.read(), computed_tree)
                except (Refused, Unimplemented):
                    continue
                answer_file = os.path.join(kind_dir, entry)
                break
        if answer_file is None:
            continue
        with open(answer_file, "rb") as fh:
            answer = fh.read()
        d = derive(name, answer, computed_tree)
        # A synthetic claim: the ids and the context hash a consumer would hold, and the
        # output_root the chain would carry for them.
        ids = [1, 2, 3, 4]
        family = "qwen25-a16"
        job_ctx = bytes(range(64))
        output_root = output_commitment_v2(job_ctx, ids, rendered_output_hash_v1(family, ids))
        network_domain = bytes(64)
        claim = bytes([7]) * 64
        pubkey = bytes(PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN)
        obj = {
            "version": PALW_DERIVED_V1_VERSION,
            "network_domain": network_domain,
            "claim_id": claim,
            "output_root": output_root,
            "grammar_id": d["grammar_id"],
            "transformer_id": d["transformer_id"],
            "kind": d["kind"],
            "dsl_hash": d["dsl_hash"],
            "artifact_hash": d["artifact_hash"],
            "artifact_bytes": len(d["artifact"]),
            "executor_pubkey": pubkey,
        }
        chain_doc = {
            "found": True,
            "claim_id": claim.hex(),
            "output_root": output_root.hex(),
            "network_domain": network_domain.hex(),
            "executor_pubkey": pubkey.hex(),
            "artifacts": [{
                "transformer_id": d["transformer_id"].hex(),
                "derived_id": derived_id_v1(obj).hex(),
                "grammar_id": d["grammar_id"].hex(),
                "kind": d["kind"],
                "dsl_hash": d["dsl_hash"].hex(),
                "artifact_hash": d["artifact_hash"].hex(),
                "artifact_bytes": len(d["artifact"]),
            }],
        }
        gateway_doc = {"misaka": {"output_token_ids": ids, "job_context_hash": job_ctx.hex(), "family": family}}
        with tempfile.TemporaryDirectory() as tmp:
            def _write(stem, doc):
                p = os.path.join(tmp, stem)
                with open(p, "w", encoding="utf-8") as fh:
                    json.dump(doc, fh)
                return p

            ans_p = os.path.join(tmp, "answer")
            with open(ans_p, "wb") as fh:
                fh.write(answer)
            art_p = os.path.join(tmp, "artifact")
            with open(art_p, "wb") as fh:
                fh.write(d["artifact"])
            import contextlib, io

            def _run(doc):
                ns = _argparse.Namespace(
                    crate_root=root, chain=_write("chain.json", doc), answer=ans_p,
                    gateway=_write("gw.json", gateway_doc), artifact=art_p,
                )
                with contextlib.redirect_stdout(io.StringIO()):
                    return cmd_verify(ns)

            checked += 1
            rc = _run(chain_doc)
            if rc == 0:
                print(f"  {name:<14} a well-formed chain read verifies: consistent")
            else:
                failures.append(f"{name}: verify() refused a chain read this path itself built (exit {rc})")
                print(f"  {name:<14} verify() refused its own round-trip (exit {rc})")
            # Tamper: move one byte of `artifact_hash`. The object is now false and `verify` must
            # say so — and it must not be rescued by any other field agreeing.
            import copy

            bad = copy.deepcopy(chain_doc)
            h = bytearray(d["artifact_hash"])
            h[0] ^= 1
            bad["artifacts"][0]["artifact_hash"] = bytes(h).hex()
            checked += 1
            rc = _run(bad)
            if rc == 2:
                print(f"  {name:<14} a tampered artifact_hash is caught: MISMATCH (exit 2)")
            else:
                failures.append(f"{name}: verify() did NOT catch a tampered artifact_hash (exit {rc})")
                print(f"  {name:<14} verify() MISSED a tampered artifact_hash (exit {rc})")

    print()
    if failures:
        print(f"SELFTEST FAILED — {len(failures)} of {checked} checks disagree with the shipped tree:")
        for f in failures:
            print(f"  {f}")
        print(
            "\nThis verifier may NOT be used as an independent recomputation while it disagrees: "
            "a second implementation that is wrong proves nothing and accuses the innocent."
        )
        return 2
    print(f"SELFTEST PASSED — {checked} checks, and every one agrees with the shipped Rust.")
    print("This file is an independent recomputation path: nothing above links, imports or shells")
    print("out to `misaka-palw-derive`, and every hash is rebuilt from ADR-0078's own preimages.")
    return 0


# ---------------------------------------------------------------------------------------------
# derive / verify
# ---------------------------------------------------------------------------------------------


def cmd_transformers(args) -> int:
    tree = source_tree_sha256_hex(args.crate_root)
    print(json.dumps({
        "schema": "misaka.palw.stranger-transformers.v1",
        "source_tree_sha256": tree,
        "transformers": {
            name: {
                "kind": m["kind"],
                "grammar": m["grammar"],
                "grammar_id": grammar_id_v1(m["grammar"]).hex(),
                "transformer_id": transformer_id_v1(manifest_bytes(m, tree)).hex(),
            }
            for name, m in sorted(TRANSFORMERS.items())
        },
    }, indent=2))
    return 0


def cmd_derive(args) -> int:
    tree = source_tree_sha256_hex(args.crate_root)
    with open(args.answer, "rb") as fh:
        answer = fh.read()
    try:
        d = derive(args.transformer, answer, tree)
    except Unimplemented as e:
        print(f"UNIMPLEMENTED: {e}", file=sys.stderr)
        return 4
    except Refused as e:
        print(f"REFUSED: {e}", file=sys.stderr)
        return 3
    files = {}
    if args.out:
        os.makedirs(args.out, exist_ok=True)
        stem = os.path.join(args.out, "stranger-" + d["dsl_hash"].hex()[:16])
        with open(stem + ".dsl", "wb") as fh:
            fh.write(d["canonical_dsl"])
        with open(f"{stem}.artifact.{d['extension']}", "wb") as fh:
            fh.write(d["artifact"])
        files = {"dsl": stem + ".dsl", "artifact": f"{stem}.artifact.{d['extension']}"}
    print(json.dumps({
        "schema": "misaka.palw.stranger-derive.v1",
        "transformer": d["transformer"],
        "kind": d["kind"],
        "source_tree_sha256": tree,
        "grammar_id": d["grammar_id"].hex(),
        "transformer_id": d["transformer_id"].hex(),
        "dsl_hash": d["dsl_hash"].hex(),
        "artifact_hash": d["artifact_hash"].hex(),
        "artifact_bytes": len(d["artifact"]),
        "files": files,
    }, indent=2))
    return 0


def _hexfield(doc: dict, key: str, what: str) -> bytes:
    v = doc.get(key)
    if not isinstance(v, str):
        raise Refused(f"the chain read carries no {what}")
    return bytes.fromhex(v)


def cmd_verify(args) -> int:
    """The stranger's check: the chain's object on one side, this file's arithmetic on the other."""
    tree = source_tree_sha256_hex(args.crate_root)
    with open(args.chain, "r", encoding="utf-8") as fh:
        chain = json.load(fh)
    if not chain.get("found"):
        print("REFUSED: the chain read says this claim is not on that chain", file=sys.stderr)
        return 3
    rows = chain.get("artifacts") or []
    if not rows:
        print(
            "REFUSED: the claim is on chain and carries no derivation (ADR-0078 X4: an answer that "
            "did not parse still certifies and still mines — there is simply nothing to verify)",
            file=sys.stderr,
        )
        return 3
    with open(args.answer, "rb") as fh:
        answer = fh.read()

    # The consumer's other half: the ids, the job's context hash and the family. On NO chain
    # (ADR-0078 Decision 2), which is exactly why the comparison means something.
    ids = ctx = family = None
    if args.gateway:
        with open(args.gateway, "r", encoding="utf-8") as fh:
            doc = json.load(fh)
        block = doc.get("misaka", doc)
        ids = block.get("output_token_ids")
        ctx = block.get("job_context_hash")
        family = block.get("family")

    verdict = {
        "schema": "misaka.palw.stranger-verify.v1",
        "independent_path": True,
        "source_tree_sha256": tree,
        "claim_id": chain.get("claim_id"),
        "rows": [],
    }
    mismatches = []
    unverifiable = []

    for row in rows:
        name = None
        for candidate, m in TRANSFORMERS.items():
            if transformer_id_v1(manifest_bytes(m, tree)).hex() == row.get("transformer_id"):
                name = candidate
                break
        entry = {"transformer_id": row.get("transformer_id"), "kind": row.get("kind")}
        if name is None:
            entry["status"] = (
                "UNVERIFIABLE by this path: no transformer in this verifier's implemented subset "
                "has that transformer_id under this source tree. Either the kind is outside the "
                "subset, or the producing build's `misaka-palw-derive/src` differs from this "
                "checkout (ADR-0078 Decision 3: the id names the code)."
            )
            unverifiable.append(entry["status"])
            verdict["rows"].append(entry)
            continue
        entry["transformer"] = name
        try:
            d = derive(name, answer, tree)
        except (Refused, Unimplemented) as e:
            entry["status"] = f"UNVERIFIABLE by this path: {e}"
            unverifiable.append(entry["status"])
            verdict["rows"].append(entry)
            continue
        checks = {
            "grammar_id": (row.get("grammar_id"), d["grammar_id"].hex()),
            "dsl_hash": (row.get("dsl_hash"), d["dsl_hash"].hex()),
            "artifact_hash": (row.get("artifact_hash"), d["artifact_hash"].hex()),
            "artifact_bytes": (row.get("artifact_bytes"), len(d["artifact"])),
            "kind": (row.get("kind"), d["kind"]),
        }
        entry["checks"] = {}
        for field, (on_chain, recomputed) in checks.items():
            ok = str(on_chain) == str(recomputed)
            entry["checks"][field] = {"on_chain": on_chain, "recomputed": recomputed, "matches": ok}
            if not ok:
                mismatches.append(f"{name}.{field}: chain {on_chain}, recomputed {recomputed}")
        if args.artifact:
            with open(args.artifact, "rb") as fh:
                got = fh.read()
            ok = got == d["artifact"]
            entry["artifact_file_matches"] = ok
            if not ok:
                mismatches.append(f"{name}: the artifact file is not the bytes this path recomputed")

        # X6's first recomputation: the claim's own output_root, from ids the chain does not hold.
        if ids is not None and ctx and family:
            recomputed_root = output_commitment_v2(
                bytes.fromhex(ctx), ids, rendered_output_hash_v1(family, ids)
            ).hex()
            ok = recomputed_root == chain.get("output_root")
            entry["output_root"] = {
                "on_chain": chain.get("output_root"),
                "recomputed": recomputed_root,
                "matches": ok,
                "family": family,
                "token_count": len(ids),
            }
            if not ok:
                mismatches.append(f"output_root: chain {chain.get('output_root')}, recomputed {recomputed_root}")
        else:
            entry["output_root"] = (
                "not checked: pass --gateway with the response's `misaka` block "
                "(output_token_ids, job_context_hash, family). They are on NO chain (Decision 2)."
            )

        # The id is total over the object, so rebuilding it checks THIS READER's inputs — a
        # disagreement means the object rebuilt here is not the one the chain accepted.
        try:
            rebuilt = derived_id_v1({
                "version": PALW_DERIVED_V1_VERSION,
                "network_domain": _hexfield(chain, "network_domain", "network_domain"),
                "claim_id": bytes.fromhex(chain["claim_id"]),
                "output_root": bytes.fromhex(chain["output_root"]),
                "grammar_id": bytes.fromhex(row["grammar_id"]),
                "transformer_id": bytes.fromhex(row["transformer_id"]),
                "kind": int(row["kind"]),
                "dsl_hash": bytes.fromhex(row["dsl_hash"]),
                "artifact_hash": bytes.fromhex(row["artifact_hash"]),
                "artifact_bytes": int(row["artifact_bytes"]),
                "executor_pubkey": bytes.fromhex(chain["executor_pubkey"]),
            }).hex()
            ok = rebuilt == row.get("derived_id")
            entry["derived_id"] = {"on_chain": row.get("derived_id"), "recomputed": rebuilt, "matches": ok}
            if not ok:
                mismatches.append(
                    f"derived_id: chain {row.get('derived_id')}, recomputed {rebuilt} — the object "
                    f"rebuilt here is not the one the chain accepted (a different network domain, "
                    f"or an executor key this read did not carry)"
                )
        except Refused as e:
            entry["derived_id"] = f"not checked: {e}"

        entry["status"] = "checked"
        verdict["rows"].append(entry)

    if mismatches:
        verdict["verdict"] = "MISMATCH — a demonstrable false object (ADR-0078 Decision 5)"
        verdict["mismatches"] = mismatches
        print(json.dumps(verdict, indent=2))
        return 2
    if unverifiable and not any(r.get("status") == "checked" for r in verdict["rows"]):
        verdict["verdict"] = "NOT CHECKED — no row was inside this verifier's implemented subset"
        print(json.dumps(verdict, indent=2))
        return 4
    verdict["verdict"] = "consistent — recomputed independently of the code that produced it"
    print(json.dumps(verdict, indent=2))
    return 0


def main() -> int:
    here = os.path.dirname(os.path.abspath(__file__))
    default_root = os.path.join(os.path.dirname(here), "misaka-palw-derive")
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--crate-root", default=default_root, help="the misaka-palw-derive crate root")
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("selftest", help="check this implementation against the tree's own oracles")
    sub.add_parser("transformers", help="the implemented transformers and their recomputed ids")
    d = sub.add_parser("derive", help="derive offline, independently")
    d.add_argument("--transformer", required=True)
    d.add_argument("--answer", required=True)
    d.add_argument("--out")
    v = sub.add_parser("verify", help="the stranger's check against a chain read")
    v.add_argument("--chain", required=True, help="`misaka palw derived <claim> --json` output")
    v.add_argument("--answer", required=True, help="the answer the derivation consumed")
    v.add_argument("--gateway", help="the gateway's chat response (its `misaka` block)")
    v.add_argument("--artifact", help="the artifact file, if the consumer kept one")
    args = p.parse_args()
    try:
        if args.cmd == "selftest":
            return cmd_selftest(args)
        if args.cmd == "transformers":
            return cmd_transformers(args)
        if args.cmd == "derive":
            return cmd_derive(args)
        return cmd_verify(args)
    except Refused as e:
        print(f"REFUSED: {e}", file=sys.stderr)
        return 3
    except Unimplemented as e:
        print(f"UNIMPLEMENTED: {e}", file=sys.stderr)
        return 4


if __name__ == "__main__":
    sys.exit(main())
