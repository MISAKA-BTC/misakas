#!/usr/bin/env bash
# misaka-palw-artifact-demo.sh — ADR-0078's acceptance condition, as a drill.
#
# The sentence this exists to make true or to refuse honestly:
#
#     a person asks a certified class for music or for a 3D object, KEEPS the artifact, and the
#     chain carries only the derivation — through to block production.
#
# It is the successor of `scripts/misaka-palw-fp-devnet-e2e.sh` for the artifact case, not a
# rewrite: that drill proves ADR-0077's free-prompt lane reaches a receipt block, and its stage 8
# already attempts one ADR-0078 derivation from the claim's own answer. This one takes that stage
# and makes it the subject — two kinds (music and 3D), the artifact opened by a real parser, the
# chain payload read back and shown to be hashes and sizes, and a stranger's recomputation run from
# a directory that does not contain the artifact.
#
# ---------------------------------------------------------------------------------------------
# THE THREE VERDICTS, AND WHY THEY ARE THREE
# ---------------------------------------------------------------------------------------------
#
#   FROM-A-REAL-INFERENCE   the class was asked, the class answered, the answer canonicalized
#                           under the kind's grammar, the transformer made the artifact, and the
#                           DerivedArtifactV1 naming that claim is on the chain. This is the
#                           acceptance condition and nothing else is.
#
#   NOT-FROM-AN-INFERENCE   the transformer half is proven — a hand-written DSL derives to an
#                           artifact a real parser opens, and a stranger recomputes it — and no
#                           model wrote the DSL. Reading this as the first is the category error
#                           ADR-0078 §1 exists to refuse, so the words NOT-FROM-AN-INFERENCE are
#                           printed in the verdict itself rather than left in a log.
#
#   BLOCKED-ON-WIDTH        the registered class row cannot hold the DSL: the arithmetic the chain
#                           enforces (`prompt_tokens + decode_token_limit <= max_context_tokens`,
#                           and `max_context_tokens` is the class profile's `n_ctx`) refuses the
#                           job before any model runs. Reported WITH the numbers — the row, the
#                           prompt, the DSL and the shortfall — because "the answer did not parse"
#                           is what this looks like from the outside and it is not what it is.
#
# The row is a PARAMETER (`--n-ctx`, or the gateway's own startup line on the chain path). It is
# not a constant of this script: which row is registered is an operational fact, and the whole
# value of the BLOCKED-ON-WIDTH verdict is that it becomes a pass the day a wider row arrives,
# with no edit here.
#
# ---------------------------------------------------------------------------------------------
# WHAT THIS DRILL DOES NOT PROVE, said here because a drill that overclaims is worse than none
# ---------------------------------------------------------------------------------------------
#   * it does not prosecute a court case (the certify drill and the fuzzers do that);
#   * it does not measure testnet-11's wall clock — devnet windows are not testnet-11's;
#   * `--offline` reaches at most NOT-FROM-AN-INFERENCE, by construction: there is no class to ask;
#   * a derivation credits no weight, no payment and no exposure (ADR-0078 X5), so nothing here is
#     a statement about mining.
#
# Env: KASPAD_BIN, CLI_BIN, CERTIFY_BIN, GATEWAY_BIN, WORKER_BIN, RAIL_BIN, DERIVE_BIN
#      (defaults target/release/*),
#      MISAKA_PALW_ARTIFACT   the .palwart the class is registered from  (chain path: REQUIRED)
#      MISAKA_PALW_TOKENIZER  its tokenizer.json                         (ALWAYS REQUIRED)
#      MISAKA_DEVNET_GENESIS  128 hex, consensus/core/src/config/genesis.rs (chain path: REQUIRED)
#      NODES (3), WORK_DIR, WAIT, STEP_WAIT, GATEWAY_PORT, MODEL_ID, N_CTX, KINDS
#
# Usage:
#   scripts/misaka-palw-artifact-demo.sh --offline --n-ctx 16
#   scripts/misaka-palw-artifact-demo.sh --n-ctx 16          # stands up a devnet and asks the class
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
# `misaka-cli` is the PACKAGE; `misaka` is the binary. The certify drill made that mistake once
# and it dies at the first CLI call with a 127.
CLI_BIN="${CLI_BIN:-$REPO_ROOT/target/release/misaka}"
CERTIFY_BIN="${CERTIFY_BIN:-$REPO_ROOT/target/release/palw-certify}"
GATEWAY_BIN="${GATEWAY_BIN:-$REPO_ROOT/target/release/misaka-palw-gateway}"
WORKER_BIN="${WORKER_BIN:-$REPO_ROOT/target/release/palw-a16-fp-worker}"
RAIL_BIN="${RAIL_BIN:-$REPO_ROOT/target/release/misaka-palw-fp-rail}"
DERIVE_BIN="${DERIVE_BIN:-$REPO_ROOT/target/release/palw-derive}"
NODES="${NODES:-3}"
WORK_DIR="${WORK_DIR:-$REPO_ROOT/.misaka-palw-artifact-demo}"
WAIT="${WAIT:-900}"
STEP_WAIT="${STEP_WAIT:-600}"
GATEWAY_PORT="${GATEWAY_PORT:-18797}"
MODEL_ID="${MODEL_ID:-Qwen/Qwen2.5-1.5B/graph-v2}"
# The two paths the acceptance condition names. Overridable so a widened row can be tried against
# the cheapest kind first — `KINDS="cad"` is a legitimate run and the verdict says which kinds ran.
KINDS="${KINDS:-music scene}"
N_CTX="${N_CTX:-}"
OFFLINE=0
PREMINE_TXID="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"   # "misaka-premine", zero-padded
MAIN_PREMINE_INDEX=40   # consensus/core/src/config/premine.rs; bond n's fee float is MAIN_PREMINE_INDEX + 1 + n

log() { printf '[artifact-demo] %s\n' "$*" >&2; }
# Prose belongs HERE and never inside `${VAR:?...}`: bash 3.2 (macOS) re-parses quotes inside a
# `:?` word, so one apostrophe there opens a quote that swallows the rest of the file and makes the
# failure land on a `done` fifty lines below.
die() { log "FATAL: $*"; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --offline) OFFLINE=1; shift ;;
    --n-ctx) N_CTX="${2:-}"; shift 2 ;;
    --kinds) KINDS="${2:-}"; shift 2 ;;
    --work-dir) WORK_DIR="${2:-}"; shift 2 ;;
    -h|--help) sed -n '2,60p' "$0"; exit 0 ;;
    *) die "unknown argument $1 (see --help)" ;;
  esac
done

# ---------------------------------------------------------------------------------------------
# Preflight — every one a refusal BY NAME, before anything is stood up.
# ---------------------------------------------------------------------------------------------
command -v python3 >/dev/null || die "python3 is required (the artifact parsers and the HTTP client)"
[ -x "$DERIVE_BIN" ] || die "$DERIVE_BIN is not executable. Build it: cargo build --release -p misaka-palw-derive"
# The `code` and `contract` kinds refuse rather than fall back in-process without this binary
# (ADR-0078 SA-1). Neither is on this drill's path, but a missing sibling binary is the kind of
# thing that surfaces as an unrelated failure later, so it is named here.
[ -x "$REPO_ROOT/target/release/palw-evm-runner" ] \
  || log "NOTE: target/release/palw-evm-runner is absent — the code/contract kinds would refuse (SA-1). This drill does not use them."
[ -n "${MISAKA_PALW_TOKENIZER:-}" ] \
  || die "MISAKA_PALW_TOKENIZER must name the tokenizer.json of the class being demonstrated. The width arithmetic IS the point of this drill and a token count taken with the wrong tokenizer is not a measurement. It is never defaulted and never guessed."
[ -f "$MISAKA_PALW_TOKENIZER" ] || die "MISAKA_PALW_TOKENIZER=$MISAKA_PALW_TOKENIZER does not exist"

if [ "$OFFLINE" = 1 ]; then
  [ -n "$N_CTX" ] || die "--n-ctx <n> is required in --offline: with no gateway to read it from, the registered row is the operator's to state. It is the class profile's n_ctx — the gateway prints it on its first line, and ADR-0080 §1 tabulates which rows the 80 KiB court carrier admits."
else
  for b in "$KASPAD_BIN" "$CLI_BIN" "$CERTIFY_BIN" "$GATEWAY_BIN" "$WORKER_BIN" "$RAIL_BIN"; do
    [ -x "$b" ] || die "$b is not an executable. Build it: cargo build --release -p <its crate>  (or pass --offline)"
  done
  [ -n "${MISAKA_PALW_ARTIFACT:-}" ] || die "MISAKA_PALW_ARTIFACT must name the .palwart for $MODEL_ID — the drill registers the class FROM it and the worker serves it."
  [ -f "$MISAKA_PALW_ARTIFACT" ] || die "MISAKA_PALW_ARTIFACT=$MISAKA_PALW_ARTIFACT does not exist"
  # The one value no shipped binary prints: `identity.json` binds `network_domain`, which is
  # blake2b512(key=…/network-domain/v1, u64le(len(net)) ‖ net ‖ genesis). A wrong value here does
  # not fail loudly — it produces claims whose context hash no seat reproduces.
  [ -n "${MISAKA_DEVNET_GENESIS:-}" ] || die "MISAKA_DEVNET_GENESIS must be the devnet genesis hash, 128 hex chars (consensus/core/src/config/genesis.rs, DEVNET_GENESIS). A guessed value silently produces claims no seat can replay."
  [ "${#MISAKA_DEVNET_GENESIS}" -eq 128 ] || die "MISAKA_DEVNET_GENESIS is ${#MISAKA_DEVNET_GENESIS} chars, not 128"
fi

for k in $KINDS; do
  case "$k" in
    music|scene|cad|image|map|simulation) ;;
    *) die "unknown kind $k — this drill ships hand-written DSLs for music, scene, cad, image, map and simulation; see \`$DERIVE_BIN list\` for what the build registers" ;;
  esac
done

rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR/keys" "$WORK_DIR/obj" "$WORK_DIR/outbox" "$WORK_DIR/dsl" "$WORK_DIR/derived" "$WORK_DIR/artifacts" "$WORK_DIR/stranger" "$WORK_DIR/width"

# =============================================================================================
# The hand-written DSLs — the MECHANISM proof's input, and NOT a model's output.
#
# They are written to disk with `not-an-inference` in the filename so that no later reader of
# this directory can mistake one for an answer. Each is a thing a person would actually want:
# a two-track arpeggio, a five-box table, a bracket, a flag, a room, a blinker.
# =============================================================================================
write_handwritten_dsl() {
  case "$1" in
  music) cat >"$WORK_DIR/dsl/music.not-an-inference.json" <<'DSL'
{ "v": 1, "ppq": 480, "tempo_us_per_quarter": 500000, "time_signature": [4, 4],
  "tracks": [
    { "name": "arpeggio", "channel": 0, "program": 0,
      "notes": [
        { "pitch": 60, "velocity": 96, "onset": 0,    "duration": 440 },
        { "pitch": 64, "velocity": 88, "onset": 480,  "duration": 440 },
        { "pitch": 67, "velocity": 88, "onset": 960,  "duration": 440 },
        { "pitch": 72, "velocity": 96, "onset": 1440, "duration": 440 },
        { "pitch": 67, "velocity": 80, "onset": 1920, "duration": 440 },
        { "pitch": 64, "velocity": 80, "onset": 2400, "duration": 440 },
        { "pitch": 62, "velocity": 88, "onset": 2880, "duration": 440 },
        { "pitch": 59, "velocity": 88, "onset": 3360, "duration": 440 } ] },
    { "name": "bass", "channel": 1, "program": 32,
      "notes": [
        { "pitch": 36, "velocity": 100, "onset": 0,    "duration": 1880 },
        { "pitch": 43, "velocity": 100, "onset": 1920, "duration": 1880 } ] } ] }
DSL
    ;;
  scene) cat >"$WORK_DIR/dsl/scene.not-an-inference.json" <<'DSL'
{ "v": 1, "frac_bits": 8,
  "materials": [
    { "name": "oak", "base_color": [186, 128, 74, 256], "metallic": 0, "roughness": 200, "double_sided": false }
  ],
  "nodes": [
    { "name": "table", "translation": [0, 0, 0], "rotation": [0, 0, 0, 2], "scale": [256, 256, 256],
      "shape": null, "material": null,
      "children": [
        { "name": "top", "translation": [0, 0, 0], "rotation": [0, 0, 0, 2], "scale": [256, 256, 256],
          "shape": { "shape": "box", "min": [-154, 179, -102], "max": [154, 192, 102] },
          "material": "oak", "children": [] },
        { "name": "leg-nw", "translation": [0, 0, 0], "rotation": [0, 0, 0, 2], "scale": [256, 256, 256],
          "shape": { "shape": "box", "min": [-141, 0, -89], "max": [-128, 179, -76] },
          "material": "oak", "children": [] },
        { "name": "leg-ne", "translation": [0, 0, 0], "rotation": [0, 0, 0, 2], "scale": [256, 256, 256],
          "shape": { "shape": "box", "min": [128, 0, -89], "max": [141, 179, -76] },
          "material": "oak", "children": [] },
        { "name": "leg-sw", "translation": [0, 0, 0], "rotation": [0, 0, 0, 2], "scale": [256, 256, 256],
          "shape": { "shape": "box", "min": [-141, 0, 76], "max": [-128, 179, 89] },
          "material": "oak", "children": [] },
        { "name": "leg-se", "translation": [0, 0, 0], "rotation": [0, 0, 0, 2], "scale": [256, 256, 256],
          "shape": { "shape": "box", "min": [128, 0, 76], "max": [141, 179, 89] },
          "material": "oak", "children": [] } ] } ] }
DSL
    ;;
  cad) cp "$REPO_ROOT/misaka-palw-derive/corpus/cad/01-extrude-l-bracket.json" "$WORK_DIR/dsl/cad.not-an-inference.json" ;;
  image) cp "$REPO_ROOT/misaka-palw-derive/corpus/image/02-rect-and-circle.json" "$WORK_DIR/dsl/image.not-an-inference.json" ;;
  map) cp "$REPO_ROOT/misaka-palw-derive/corpus/map/02-graph-and-attributes.json" "$WORK_DIR/dsl/map.not-an-inference.json" ;;
  simulation) cp "$REPO_ROOT/misaka-palw-derive/corpus/simulation/02-gosper-gun.json" "$WORK_DIR/dsl/simulation.not-an-inference.json" ;;
  esac
}

# The transformer each kind's path names, and the prompt a person would type for it. The prompt is
# the width report's first term, so it is a real request and not a placeholder.
transformer_of() {
  case "$1" in
    music) echo "music/smf/v1" ;;
    scene) echo "scene/glb/v1" ;;
    cad) echo "cad/stl/v1" ;;
    image) echo "image/png/v1" ;;
    map) echo "map/mmap/v1" ;;
    simulation) echo "simulation/trace/v1" ;;
  esac
}
# The smallest DSL the X3 drill corpus holds for a kind — the cheapest thing anybody could ask
# that kind for, which is the floor a row has to clear before this layer works at all.
floor_dsl_of() {
  case "$1" in
    music) echo "$REPO_ROOT/misaka-palw-derive/corpus/music/01-single-note.json" ;;
    scene) echo "$REPO_ROOT/misaka-palw-derive/corpus/scene/01-cube.json" ;;
    cad) echo "$REPO_ROOT/misaka-palw-derive/corpus/cad/07-box.json" ;;
    image) echo "$REPO_ROOT/misaka-palw-derive/corpus/image/01-flat-background.json" ;;
    map) echo "$REPO_ROOT/misaka-palw-derive/corpus/map/01-small-grid.json" ;;
    simulation) echo "$REPO_ROOT/misaka-palw-derive/corpus/simulation/01-blinker.json" ;;
  esac
}
prompt_of() {
  case "$1" in
    music) echo "Write a four-bar C major arpeggio with a bass line as music/v1 JSON." ;;
    scene) echo "Model a wooden table as scene/v1 JSON." ;;
    cad) echo "Design an L bracket as cad/v1 JSON." ;;
    image) echo "Draw a rectangle and a circle as image/v1 JSON." ;;
    map) echo "Lay out a small dungeon as map/v1 JSON." ;;
    simulation) echo "Set up a Gosper glider gun as simulation/v1 JSON." ;;
  esac
}

# =============================================================================================
# The artifact parser — "an artifact claim is a file with the tool that opened it".
#
# `file(1)` is an independent witness and is printed, but it is not the check: it reads a magic
# number. This walks the container — MIDI chunk lengths and delta-time VLQs to end-of-track, the
# GLB chunk table and every accessor against its bufferView, the STL triangle count against the
# file length, the PNG chunk CRCs — so "it opens" means the bytes are a well-formed file of that
# format and not a file that begins correctly.
# =============================================================================================
open_check() {
  python3 - "$1" <<'PY'
import json, struct, sys, zlib

path = sys.argv[1]
data = open(path, "rb").read()
ext = path.rsplit(".", 1)[-1]
facts = {"file": path, "bytes": len(data), "format": ext}

def fail(why):
    print(json.dumps({**facts, "opens": False, "why": why})); sys.exit(1)

if ext == "mid":
    # Standard MIDI File: MThd(6) then ntrks × MTrk, each walked event by event.
    if data[:4] != b"MThd" or struct.unpack(">I", data[4:8])[0] != 6:
        fail("no MThd header of length 6")
    fmt, ntrks, division = struct.unpack(">HHH", data[8:14])
    facts.update({"midi_format": fmt, "tracks": ntrks, "ticks_per_quarter": division})
    off, notes = 14, 0
    for t in range(ntrks):
        if data[off:off + 4] != b"MTrk":
            fail(f"track {t}: no MTrk at offset {off}")
        length = struct.unpack(">I", data[off + 4:off + 8])[0]
        end, p, running = off + 8 + length, off + 8, None
        if end > len(data):
            fail(f"track {t}: chunk length {length} runs past the file")
        saw_end = False
        while p < end:
            delta = 0                                   # variable-length quantity
            while True:
                b = data[p]; p += 1
                delta = (delta << 7) | (b & 0x7F)
                if not b & 0x80: break
            status = data[p]
            if status & 0x80: p += 1; running = status
            elif running is not None: status = running
            else: fail(f"track {t}: a running-status event with no status byte")
            if status == 0xFF:                          # meta
                meta = data[p]; p += 1
                ln = 0
                while True:
                    b = data[p]; p += 1
                    ln = (ln << 7) | (b & 0x7F)
                    if not b & 0x80: break
                p += ln
                if meta == 0x2F:
                    saw_end = True
                    if p != end: fail(f"track {t}: end-of-track at {p}, chunk ends at {end}")
            elif status & 0xF0 in (0x80, 0x90, 0xA0, 0xB0, 0xE0):
                if status & 0xF0 == 0x90 and data[p + 1] != 0: notes += 1
                p += 2
            elif status & 0xF0 in (0xC0, 0xD0):
                p += 1
            else:
                fail(f"track {t}: unhandled status 0x{status:02X}")
        if not saw_end: fail(f"track {t}: no end-of-track meta event")
        off = end
    if off != len(data): fail(f"{len(data) - off} trailing bytes after the last track")
    facts["note_ons"] = notes
    if notes == 0: fail("the file is well formed and contains no notes")

elif ext == "glb":
    # glTF 2.0 binary: a 12-byte header, a JSON chunk, a BIN chunk, and every accessor inside its
    # bufferView inside the BIN chunk.
    if len(data) < 12 or data[:4] != b"glTF": fail("no glTF magic")
    version, total = struct.unpack("<II", data[4:12])
    if version != 2: fail(f"glTF version {version}, not 2")
    if total != len(data): fail(f"header says {total} bytes, file is {len(data)}")
    off, js, bin_len = 12, None, 0
    while off < total:
        clen, ctype = struct.unpack("<I4s", data[off:off + 8])
        body = data[off + 8:off + 8 + clen]
        if len(body) != clen: fail("a chunk runs past the file")
        if ctype == b"JSON": js = json.loads(body.decode("utf-8"))
        elif ctype == b"BIN\x00": bin_len = clen
        off += 8 + clen + ((4 - clen % 4) % 4 if False else 0)
        if clen % 4: fail("a chunk length is not 4-byte aligned (the canonical writer pads)")
    if js is None: fail("no JSON chunk")
    if js.get("asset", {}).get("version") != "2.0": fail("asset.version is not 2.0")
    views = js.get("bufferViews", [])
    for i, v in enumerate(views):
        if v.get("byteOffset", 0) + v["byteLength"] > bin_len:
            fail(f"bufferView {i} runs past the {bin_len}-byte BIN chunk")
    prims = 0
    for m in js.get("meshes", []):
        for p in m.get("primitives", []):
            prims += 1
            for name, a in list(p.get("attributes", {}).items()) + ([("INDICES", p["indices"])] if "indices" in p else []):
                if a >= len(js.get("accessors", [])): fail(f"primitive attribute {name} names accessor {a}, which does not exist")
                if js["accessors"][a].get("bufferView", 0) >= len(views): fail(f"accessor {a} names a bufferView that does not exist")
    facts.update({"nodes": len(js.get("nodes", [])), "meshes": len(js.get("meshes", [])),
                  "primitives": prims, "accessors": len(js.get("accessors", [])),
                  "materials": len(js.get("materials", [])), "bin_bytes": bin_len})
    if prims == 0: fail("the glTF is well formed and contains no geometry")

elif ext == "stl":
    if len(data) < 84: fail("shorter than an STL header")
    tris = struct.unpack("<I", data[80:84])[0]
    if len(data) != 84 + 50 * tris: fail(f"{tris} triangles need {84 + 50 * tris} bytes, file is {len(data)}")
    facts["triangles"] = tris
    if tris == 0: fail("the STL is well formed and contains no triangles")

elif ext == "png":
    if data[:8] != b"\x89PNG\r\n\x1a\n": fail("no PNG signature")
    off, chunks, ihdr = 8, [], None
    while off < len(data):
        ln = struct.unpack(">I", data[off:off + 4])[0]
        ctype = data[off + 4:off + 8]
        body = data[off + 8:off + 8 + ln]
        crc = struct.unpack(">I", data[off + 8 + ln:off + 12 + ln])[0]
        if zlib.crc32(ctype + body) & 0xFFFFFFFF != crc: fail(f"chunk {ctype!r} fails its CRC")
        if ctype == b"IHDR": ihdr = struct.unpack(">IIBBBBB", body)
        chunks.append(ctype.decode("ascii"))
        off += 12 + ln
    if chunks[0] != "IHDR" or chunks[-1] != "IEND": fail(f"chunk order {chunks}")
    facts.update({"width": ihdr[0], "height": ihdr[1], "bit_depth": ihdr[2], "chunks": chunks})

elif ext == "msim":
    # This tree's own container (`misaka-sim-trace/1/canonical-v1`, kinds/simulation.rs): magic,
    # version, rules tag, seed, steps, width, height, trace length, then that many 32-byte step
    # hashes. There is no third-party reader for it, so the check is the layout's own arithmetic —
    # every declared section has to fit inside the file it declares itself in.
    if data[:4] != b"MSIM": fail("no MSIM magic")
    version, rules = struct.unpack("<HB", data[4:7])
    seed, steps, width, height, trace = struct.unpack("<QIIII", data[7:31])
    if version != 1: fail(f"artifact version {version}, not 1")
    if 31 + 32 * trace > len(data): fail(f"{trace} step hashes need {31 + 32 * trace} bytes, file is {len(data)}")
    if steps == 0 or width == 0 or height == 0: fail("a well-formed header describing an empty run")
    facts.update({"artifact_version": version, "rules_tag": rules, "seed": seed, "steps": steps,
                  "grid": [width, height], "trace_hashes": trace})

elif ext == "mmap":
    # `misaka-map/1/canonical-v1` (kinds/map.rs): magic, version, flags, width, height, and four
    # counts, then a palette of 4-byte entries and a width×height grid — again checked by fitting
    # the declared sections into the file rather than by trusting the header.
    if data[:4] != b"MMAP": fail("no MMAP magic")
    version, flags = struct.unpack("<HH", data[4:8])
    width, height, palette, regions, nodes, edges = struct.unpack("<IIIIII", data[8:32])
    if version != 1: fail(f"artifact version {version}, not 1")
    need = 32 + 4 * palette + width * height
    if need > len(data): fail(f"the header declares {need} bytes of palette and grid; the file is {len(data)}")
    if width == 0 or height == 0 or palette == 0: fail("a well-formed header describing an empty map")
    facts.update({"artifact_version": version, "flags": flags, "grid": [width, height],
                  "palette": palette, "regions": regions, "nodes": nodes, "edges": edges})

else:
    # A kind whose artifact this drill has no parser for says so rather than passing quietly: a
    # check that does not check is worse than no check.
    print(json.dumps({**facts, "opens": None, "why": f"this drill has no structural parser for .{ext}"})); sys.exit(2)

print(json.dumps({**facts, "opens": True}))
PY
}

# =============================================================================================
# ADR-0078 Decision 1, as a measurement rather than a promise: the consensus object is CONSTANT
# in the artifact and in the DSL.
#
# Two derivations of the same kind whose artifacts differ by orders of magnitude must produce
# byte-identical object LENGTHS, and neither artifact may occur inside its own object. A drill
# that only asserted "the object has no artifact field" would be reading the struct definition
# back to itself; this reads the bytes.
# =============================================================================================
decision_1_bytes_never_ride() {
  local small="$1" large="$2" small_art="$3" large_art="$4"
  python3 - "$small" "$large" "$small_art" "$large_art" <<'PY'
import json, os, sys
small_obj, large_obj, small_art, large_art = (open(p, "rb").read() for p in sys.argv[1:5])
out = {
    "small_object_bytes": len(small_obj), "large_object_bytes": len(large_obj),
    "small_artifact_bytes": len(small_art), "large_artifact_bytes": len(large_art),
}
problems = []
if len(small_obj) != len(large_obj):
    problems.append(f"the object grew with the artifact: {len(small_obj)} -> {len(large_obj)}")
# The artifact is not in the object, and neither is any run of it long enough to be a fragment.
for name, obj, art in (("small", small_obj, small_art), ("large", large_obj, large_art)):
    if art in obj:
        problems.append(f"the {name} artifact occurs verbatim inside its own object")
    window = 32
    if len(art) >= window and any(art[i:i + window] in obj for i in range(0, len(art) - window, 17)):
        problems.append(f"a {window}-byte run of the {name} artifact occurs inside its own object")
out["problems"] = problems
out["verdict"] = "ADR-0078 Decision 1 holds: the object is constant in the artifact and carries none of it" if not problems else "DECISION 1 VIOLATED"
print(json.dumps(out))
sys.exit(0 if not problems else 1)
PY
}

# =============================================================================================
# STAGE 1 — the width report, per kind. The row is the parameter; the DSL is the target.
# =============================================================================================
declare_width() {
  # `|| rc=$?` rather than a `set +e` / `set -e` pair: a `set -e` INSIDE a function re-enables
  # errexit for the caller too, so the pair would undo the caller's own `set +e` and make this
  # function's honest exit 6 kill the drill. That is not hypothetical — it did.
  local kind="$1" t dsl out rc=0
  t="$(transformer_of "$kind")"
  dsl="$WORK_DIR/dsl/$kind.not-an-inference.json"
  out="$WORK_DIR/width/$kind.json"
  "$DERIVE_BIN" width --tokenizer "$MISAKA_PALW_TOKENIZER" --n-ctx "$N_CTX" \
    --prompt "$(prompt_of "$kind")" --dsl "$dsl" --transformer "$t" >"$out" 2>"$out.err" || rc=$?
  # Exit 1 is the tool refusing the REQUEST (a bad tokenizer, a DSL that does not parse); exit 6 is
  # the row being too narrow, which is an answer and not a failure. Only the first is fatal here.
  if [ "$rc" != 0 ] && [ "$rc" != 6 ]; then
    cat "$out.err" >&2
    die "the width report for $kind could not be produced (exit $rc) — see $out.err"
  fi
  python3 - "$out" <<'PY'
import json, sys
w = json.load(open(sys.argv[1]))
print("  n_ctx {n_ctx}  prompt {prompt_tokens} tok  budget {decode_budget_tokens} tok  "
      "canonical DSL {canonical_dsl_bytes} B = {canonical_dsl_tokens} tok  "
      "needs n_ctx >= {required_n_ctx}  short by {shortfall_tokens}  -> {verdict}".format(**w), file=sys.stderr)
PY
  # **And the FLOOR of the kind**, measured on the smallest DSL the shipped drill corpus holds for
  # it with a one-word prompt. The demo's own DSL is a thing a person would want and therefore not
  # the cheapest thing of its kind; a row that is being sized for this layer needs both numbers —
  # what the demo costs, and what the least this kind can ask for costs.
  local floor_dsl
  floor_dsl="$(floor_dsl_of "$kind")"
  if [ -n "$floor_dsl" ] && [ -f "$floor_dsl" ]; then
    "$DERIVE_BIN" width --tokenizer "$MISAKA_PALW_TOKENIZER" --n-ctx "$N_CTX" \
      --prompt "a" --dsl "$floor_dsl" --transformer "$t" >"$WORK_DIR/width/$kind.floor.json" 2>/dev/null || true
    if [ -s "$WORK_DIR/width/$kind.floor.json" ]; then
      python3 - "$WORK_DIR/width/$kind.floor.json" "$floor_dsl" <<'PY'
import json, os, sys
w = json.load(open(sys.argv[1]))
print("  floor for this kind ({f}, one-word prompt): {canonical_dsl_bytes} B = {canonical_dsl_tokens} tok, "
      "needs n_ctx >= {required_n_ctx}".format(f=os.path.basename(sys.argv[2]), **w), file=sys.stderr)
PY
    fi
  fi
  return $rc
}

# =============================================================================================
# STAGE 2/3 — the mechanism proof and the stranger, offline. Runs on every path, always: it is
# what makes BLOCKED-ON-WIDTH a report about the CLASS and not about the transformer.
# =============================================================================================
mechanism_and_stranger() {
  local kind="$1" t dsl obj art stem strangerdir
  t="$(transformer_of "$kind")"
  dsl="$WORK_DIR/dsl/$kind.not-an-inference.json"
  "$DERIVE_BIN" derive --transformer "$t" --answer "$dsl" --out "$WORK_DIR/derived" \
    >"$WORK_DIR/derived/$kind.derive.json" 2>&1 \
    || { cat "$WORK_DIR/derived/$kind.derive.json" >&2; die "the hand-written $kind DSL did not derive — the transformer half is broken, which is a defect and not a width"; }
  stem=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["files"]["object"])' "$WORK_DIR/derived/$kind.derive.json")
  obj="$stem"
  art=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["files"]["artifact"])' "$WORK_DIR/derived/$kind.derive.json")
  cp "$art" "$WORK_DIR/artifacts/$kind.${art##*.}"
  log "  derived $(basename "$art") — $(wc -c <"$art" | tr -d ' ') bytes"
  log "  file(1): $(file -b "$art")"
  local open_rc=0
  open_check "$WORK_DIR/artifacts/$kind.${art##*.}" >"$WORK_DIR/derived/$kind.open.json" || open_rc=$?
  cat "$WORK_DIR/derived/$kind.open.json" >&2
  if [ "$open_rc" = 1 ]; then
    die "the $kind artifact is not a well-formed file of its own format — see $WORK_DIR/derived/$kind.open.json"
  fi

  # THE STRANGER. A directory holding the answer and the object and NOTHING else, verified with
  # that directory as the working directory, so "the artifact was not needed" is structural rather
  # than asserted. The listing goes in the log for the same reason.
  strangerdir="$WORK_DIR/stranger/$kind"
  mkdir -p "$strangerdir"
  cp "$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["files"]["dsl"])' "$WORK_DIR/derived/$kind.derive.json")" "$strangerdir/answer.txt"
  cp "$obj" "$strangerdir/object.borsh"
  log "  the stranger holds: $(cd "$strangerdir" && ls | tr '\n' ' ')"
  # `if` rather than `A && die`: under `set -e` a trailing `&&` list whose left side FAILS is the
  # whole statement, so the healthy case — grep finding no artifact — would abort the drill.
  if ( cd "$strangerdir" && ls | grep -qE '\.(mid|glb|stl|png|bin)$' ); then
    die "the stranger directory contains an artifact — the recomputation would prove nothing"
  fi
  ( cd "$strangerdir" && "$DERIVE_BIN" verify --object object.borsh --answer answer.txt ) \
    >"$WORK_DIR/derived/$kind.stranger.json" 2>&1 \
    || { cat "$WORK_DIR/derived/$kind.stranger.json" >&2; die "the stranger could not recompute the $kind derivation (ADR-0078 X6)"; }
  log "  stranger verdict: $(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["verdict"])' "$WORK_DIR/derived/$kind.stranger.json")"

  # AND THE CHECK HAS TEETH. One byte of the answer moved must make the same recomputation say
  # MISMATCH; a verifier that passes on anything is worse than none, and Decision 5 is exactly the
  # promise that a false object is demonstrable.
  python3 - "$strangerdir/answer.txt" "$strangerdir/tampered.txt" <<'PY'
import sys
b = bytearray(open(sys.argv[1], "rb").read())
# Move a DIGIT, so the tampered answer is still valid JSON of the same shape — a mutation that
# broke the parse would be caught by the grammar and would prove nothing about the hashes.
for i, c in enumerate(b):
    if chr(c).isdigit() and chr(b[i - 1]) not in "\"":
        b[i] = ord("9") if chr(c) != "9" else ord("8")
        break
open(sys.argv[2], "wb").write(bytes(b))
PY
  local tamper_rc=0
  ( cd "$strangerdir" && "$DERIVE_BIN" verify --object object.borsh --answer tampered.txt ) \
    >"$WORK_DIR/derived/$kind.tamper.json" 2>&1 || tamper_rc=$?
  [ "$tamper_rc" != 0 ] || die "a tampered answer still verified against the $kind object — the consumer check has no teeth (ADR-0078 Decision 5)"
  log "  tampered answer: rejected (exit $tamper_rc) — $(python3 -c '
import json,sys
try: print(json.load(open(sys.argv[1]))["verdict"])
except Exception: print("refused before it could re-run")' "$WORK_DIR/derived/$kind.tamper.json")"
  echo "$obj"
}

log "================ ADR-0078 artifact demo ================"
log "kinds: $KINDS   mode: $([ "$OFFLINE" = 1 ] && echo offline || echo "chain (devnet, $NODES validators)")"

for kind in $KINDS; do write_handwritten_dsl "$kind"; done

# ---------------------------------------------------------------------------------------------
# Stage 1 needs the row. Offline it is the flag; on the chain path the gateway prints the row its
# worker actually serves, and that is what gets used — a flag that disagreed with the running
# class would be a number the drill made up.
# ---------------------------------------------------------------------------------------------
declare -a WIDTH_VERDICT
declare -a OBJECTS

pids=()
cleanup() { for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT

# =============================================================================================
# THE CHAIN HALF. Everything above runs without one; everything below needs a running devnet.
# =============================================================================================
CHAIN_NOTE="not attempted (--offline)"
declare -a ANSWER_DERIVED
if [ "$OFFLINE" = 0 ]; then
  NETWORK_DOMAIN=$(python3 - "$MISAKA_DEVNET_GENESIS" <<'PY'
import hashlib, struct, sys
net = b"devnet"
h = hashlib.blake2b(digest_size=64, key=b"misaka-palw/attempt-v2/network-domain/v1")
h.update(struct.pack("<Q", len(net))); h.update(net); h.update(bytes.fromhex(sys.argv[1]))
print(h.hexdigest())
PY
)
  log "network domain $NETWORK_DOMAIN (devnet ‖ genesis ${MISAKA_DEVNET_GENESIS:0:16}…)"

  python3 - "$WORK_DIR/keys" "$NODES" <<'PY'
import hashlib, os, sys
d, n = sys.argv[1], int(sys.argv[2])
h = lambda b: hashlib.blake2b(b, digest_size=32).hexdigest()
for i in range(n):
    p = f"{d}/bond-{i}.seed"; open(p, "w").write(h(b"misaka-devnet-genesis-bond-v1/" + str(i).encode())); os.chmod(p, 0o600)
p = f"{d}/main.seed"; open(p, "w").write(h(b"misaka-testnet-premine-9b-claude-managed")); os.chmod(p, 0o600)
PY

  declare -a ADDRS
  for ((i=0; i<NODES; i++)); do
    addr="$("$CLI_BIN" --network devnet key address --key-file "$WORK_DIR/keys/bond-$i.seed" | tail -1 | awk '{print $NF}')"
    [ -n "$addr" ] || die "cannot derive bond $i's address"
    ADDRS[$i]="$addr"
    p2p=$((16430 + i)); rpc=$((17730 + i))
    args=(--devnet --appdir="$WORK_DIR/node-$i" --listen=127.0.0.1:$p2p --rpclisten-borsh=127.0.0.1:$rpc
          --utxoindex --nodnsseed --disable-upnp --nogrpc --enable-unsynced-mining --palw-panel
          --palw-produce --palw-producer-key="$WORK_DIR/keys/bond-$i.seed"
          --palw-producer-bond="$PREMINE_TXID:$i" --palw-producer-pay-address="$addr"
          --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + i))")
    if [ "$i" -eq 0 ]; then args+=(--palw-class-artifact="$MISAKA_PALW_ARTIFACT"); fi
    if [ "$i" -gt 0 ]; then args+=(--connect=127.0.0.1:16430); fi
    MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >"$WORK_DIR/node-$i.log" 2>&1 &
    # `$!` into a variable rather than `${pids[-1]}`: macOS ships bash 3.2, which rejects a
    # negative array index at PARSE time — the whole file fails to load, not the line.
    node_pid=$!
    pids+=("$node_pid")
    log "node-$i pid $node_pid rpc 127.0.0.1:$rpc bond $PREMINE_TXID:$i"
  done

  CLI=("$CLI_BIN" --network devnet --rpc 127.0.0.1:17730)
  blocks_of() { grep -c "produced block #" "$WORK_DIR/node-$1.log" 2>/dev/null || true; }
  advance() {
    local want="${1:-1}" from now deadline
    from=$(blocks_of 0); deadline=$((SECONDS + STEP_WAIT))
    while :; do
      now=$(blocks_of 0)
      [ $((now - from)) -ge "$want" ] && return 0
      [ $SECONDS -lt $deadline ] || { log "node-0 gained $((now - from))/$want block(s) in ${STEP_WAIT}s — continuing to the verdict"; return 0; }
      sleep 2
    done
  }
  all_nodes_logged() {
    local pattern="$1" deadline=$((SECONDS + STEP_WAIT)) i ok
    while :; do
      ok=1
      for ((i=0; i<NODES; i++)); do grep -qE "$pattern" "$WORK_DIR/node-$i.log" || ok=0; done
      [ "$ok" = 1 ] && return 0
      [ $SECONDS -lt $deadline ] || { log "not every node matched \"$pattern\" within ${STEP_WAIT}s — continuing to the verdict"; return 1; }
      sleep 3
    done
  }

  deadline=$((SECONDS + WAIT))
  until [ "$(blocks_of 0)" -ge 3 ]; do
    [ $SECONDS -lt $deadline ] || { tail -40 "$WORK_DIR/node-0.log" >&2; die "node-0 produced no blocks within ${WAIT}s"; }
    sleep 3
  done
  log "chain up — node-0 produced $(blocks_of 0) blocks"
  advance 1

  log "registering $MODEL_ID from the artifact"
  if ! MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" --devnet --appdir="$WORK_DIR/node-0-reg" \
        --rpclisten-borsh=127.0.0.1:17830 --nogrpc --nodnsseed --disable-upnp \
        --connect=127.0.0.1:16430 --utxoindex \
        --palw-register-class="$MODEL_ID" --palw-class-artifact="$MISAKA_PALW_ARTIFACT" \
        --palw-producer-key="$WORK_DIR/keys/bond-0.seed" --palw-producer-pay-address="${ADDRS[0]}" \
        --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1))" \
        >"$WORK_DIR/register-class.log" 2>&1; then
    tail -30 "$WORK_DIR/register-class.log" >&2
    die "registering $MODEL_ID failed — see $WORK_DIR/register-class.log"
  fi
  advance 2
  CLASS_ID=$(grep -oE '[0-9a-f]{128}' "$WORK_DIR/register-class.log" | tail -1 || true)
  [ -n "$CLASS_ID" ] || { tail -30 "$WORK_DIR/register-class.log" >&2; die "no class id in the registration log"; }
  log "class ${CLASS_ID:0:16}…"

  submit() {
    local f="$1"; local args=()
    if ls "$f".chunk* >/dev/null 2>&1; then
      for c in $(ls "$f".chunk* | sort -t k -k3 -n); do args+=(--object "$c"); done
    else
      args=(--object "$f")
    fi
    "${CLI[@]}" palw submit-object --key-file "$WORK_DIR/keys/main.seed" "${args[@]}" --yes
    advance 2
  }
  log "certifying the a16 family on the free-prompt lane (ADR-0075)"
  "$CERTIFY_BIN" drill --family a16 --lane fp --out "$WORK_DIR/obj/a16-fp.obj" || die "the a16 fp drill did not produce evidence"
  submit "$WORK_DIR/obj/a16-fp.obj"
  "$CERTIFY_BIN" bind --model-id "$MODEL_ID" --lane fp --out "$WORK_DIR/obj/a16-bind.obj" || die "palw-certify bind refused $MODEL_ID"
  submit "$WORK_DIR/obj/a16-bind.obj"
  all_nodes_logged "PALW lifecycle carried.*ClassLaneCertified" \
    || log "WARNING: not every node logged the class-lane binding — the commitment may be refused as uncertified"

  EXEC_PUBKEY=$("$RAIL_BIN" --bond-key-seed "$WORK_DIR/keys/bond-0.seed" --print-bond-pubkey \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["executor_pubkey"])') \
    || die "cannot read bond 0's public key from the rail"
  OPERATOR_ID=$(python3 - <<'PY'
import hashlib, struct
pk = b"misaka-devnet-operator-0"
h = hashlib.blake2b(digest_size=64, key=b"misaka-palw/state-v2/operator-id/v1")
h.update(struct.pack("<Q", len(pk))); h.update(pk)
print(h.hexdigest())
PY
)
  cat >"$WORK_DIR/identity.json" <<JSON
{
  "network_domain": "$NETWORK_DOMAIN",
  "class_id": "$CLASS_ID",
  "bond_txid": "$PREMINE_TXID",
  "bond_index": 0,
  "executor_pubkey": "$EXEC_PUBKEY",
  "operator_id": "$OPERATOR_ID"
}
JSON

  # The gateway signs the derivation itself when it holds the bond seed, which is what makes the
  # one-response delivery of Decision 6 real. The seed must live OUTSIDE --identity's directory
  # and outside --outbox, and the gateway refuses otherwise.
  mkdir -p "$WORK_DIR/derive-seed"
  cp "$WORK_DIR/keys/bond-0.seed" "$WORK_DIR/derive-seed/bond-0.seed"
  log "starting the gateway on 127.0.0.1:$GATEWAY_PORT"
  MISAKA_PALW_ARTIFACT="$MISAKA_PALW_ARTIFACT" MISAKA_PALW_TOKENIZER="$MISAKA_PALW_TOKENIZER" \
  MISAKA_PALW_NETWORK_ID="devnet" \
  "$GATEWAY_BIN" --listen "127.0.0.1:$GATEWAY_PORT" --worker "$WORKER_BIN" \
    --outbox "$WORK_DIR/outbox" --identity "$WORK_DIR/identity.json" \
    --derive-seed "$WORK_DIR/derive-seed/bond-0.seed" \
    --rpc 127.0.0.1:17730 >"$WORK_DIR/gateway.log" 2>&1 &
  pids+=($!)

  health=""
  deadline=$((SECONDS + STEP_WAIT))
  until health=$(python3 -c "
import json,urllib.request,sys
try: print(urllib.request.urlopen('http://127.0.0.1:$GATEWAY_PORT/health', timeout=3).read().decode())
except Exception: sys.exit(1)
" 2>/dev/null); do
    [ $SECONDS -lt $deadline ] || { tail -40 "$WORK_DIR/gateway.log" >&2; die "the gateway did not answer /health within ${STEP_WAIT}s"; }
    sleep 2
  done
  echo "$health" >"$WORK_DIR/health.json"
  python3 -c "
import json
h = json.load(open('$WORK_DIR/health.json')).get('chain', {})
print('  chain: ' + ' '.join(f'{k}={h.get(k)}' for k in ('registered','fp_certified','bond_active','exposure_room')))
" >&2

  # **The row, from the class that is actually running.** The gateway prints the worker manifest's
  # n_ctx on its first line; a --n-ctx flag that disagrees with it is refused rather than silently
  # preferred, because the width report would then describe a class nobody registered.
  GATEWAY_N_CTX=$(sed -n 's/.*n_ctx \([0-9][0-9]*\).*/\1/p' "$WORK_DIR/gateway.log" | head -1)
  [ -n "$GATEWAY_N_CTX" ] || die "the gateway did not print the class row (n_ctx) — see $WORK_DIR/gateway.log"
  if [ -n "$N_CTX" ] && [ "$N_CTX" != "$GATEWAY_N_CTX" ]; then
    die "--n-ctx $N_CTX disagrees with the running class, whose worker serves n_ctx $GATEWAY_N_CTX. The width report must describe the class that answers, so this is a refusal and not a preference."
  fi
  N_CTX="$GATEWAY_N_CTX"
  log "the running class serves n_ctx $N_CTX"
  CHAIN_NOTE="chain up, class ${CLASS_ID:0:16}… certified on the fp lane, gateway serving n_ctx $N_CTX"
fi

# =============================================================================================
# Now the per-kind work, with the row known.
# =============================================================================================
idx=0
for kind in $KINDS; do
  log "--- $kind ($(transformer_of "$kind")) ---"
  set +e
  declare_width "$kind"
  wrc=$?
  set -e
  if [ "$wrc" = 6 ]; then WIDTH_VERDICT[$idx]="BLOCKED-ON-WIDTH"; else WIDTH_VERDICT[$idx]="FITS"; fi
  obj=$(mechanism_and_stranger "$kind")
  OBJECTS[$idx]="$obj"
  ANSWER_DERIVED[$idx]="no"
  idx=$((idx + 1))
done

# ADR-0078 Decision 1, measured on the music path if it ran, else on the first path that did.
log "--- ADR-0078 Decision 1: the bytes never ride ---"
D1_KIND=$(echo "$KINDS" | awk '{print $1}')
D1_T="$(transformer_of "$D1_KIND")"
case "$D1_KIND" in
  music) D1_BIG="$REPO_ROOT/misaka-palw-derive/corpus/music/04-many-notes.json" ;;
  scene) D1_BIG="$REPO_ROOT/misaka-palw-derive/corpus/scene/05-tetrahedral-rotations.json" ;;
  cad) D1_BIG="$REPO_ROOT/misaka-palw-derive/corpus/cad/04-revolve-twelve-rational-directions.json" ;;
  image) D1_BIG="$REPO_ROOT/misaka-palw-derive/corpus/image/03-polygon-chart.json" ;;
  map) D1_BIG="$REPO_ROOT/misaka-palw-derive/corpus/map/05-large-dungeon.json" ;;
  simulation) D1_BIG="$REPO_ROOT/misaka-palw-derive/corpus/simulation/04-agents-jitter.json" ;;
esac
mkdir -p "$WORK_DIR/decision1"
"$DERIVE_BIN" derive --transformer "$D1_T" --answer "$D1_BIG" --out "$WORK_DIR/decision1" >"$WORK_DIR/decision1/big.json" 2>&1 \
  || { cat "$WORK_DIR/decision1/big.json" >&2; die "the large $D1_KIND corpus answer did not derive"; }
D1_BIG_OBJ=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["files"]["object"])' "$WORK_DIR/decision1/big.json")
D1_BIG_ART=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["files"]["artifact"])' "$WORK_DIR/decision1/big.json")
D1_SMALL_OBJ="${OBJECTS[0]}"
D1_SMALL_ART=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["files"]["artifact"])' "$WORK_DIR/derived/$D1_KIND.derive.json")
set +e
decision_1_bytes_never_ride "$D1_SMALL_OBJ" "$D1_BIG_OBJ" "$D1_SMALL_ART" "$D1_BIG_ART" | tee "$WORK_DIR/decision1/verdict.json" >&2
D1_RC=${PIPESTATUS[0]}
set -e
[ "$D1_RC" = 0 ] || die "ADR-0078 Decision 1 does not hold on this build — see $WORK_DIR/decision1/verdict.json"

# =============================================================================================
# THE CHAIN LEG — one request per kind, asking the class for the thing.
# =============================================================================================
CHAIN_LEG_NOTE="not attempted (--offline)"
if [ "$OFFLINE" = 0 ]; then
  CHAIN_LEG_NOTE=""
  idx=0
  for kind in $KINDS; do
    t="$(transformer_of "$kind")"
    log "--- chain: asking the class for a $kind ---"
    budget=$(python3 -c 'import json,sys; print(max(1, json.load(open(sys.argv[1]))["decode_budget_tokens"]))' "$WORK_DIR/width/$kind.json")
    set +e
    python3 - "$GATEWAY_PORT" "$(prompt_of "$kind")" "$budget" "$t" "$WORK_DIR/chat-$kind.json" <<'PY'
import json, sys, urllib.request
port, prompt, max_tokens, transformer, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4], sys.argv[5]
body = json.dumps({"messages": [{"role": "user", "content": prompt}],
                   "max_tokens": max_tokens, "derive": transformer}).encode()
req = urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions", data=body,
                             headers={"content-type": "application/json"})
payload = json.loads(urllib.request.urlopen(req, timeout=3600).read())
json.dump(payload, open(out, "w"), indent=2)
m = payload.get("misaka", {})
d = m.get("derivation") or {}
print(f"  answer: {payload['choices'][0]['message']['content']!r}", file=sys.stderr)
print(f"  claim {m.get('fp_claim_id','?')[:16]}…  derivation {d.get('status', 'absent')}", file=sys.stderr)
if d.get("status") == "refused":
    print(f"  refused because: {d.get('reason')}", file=sys.stderr)
if m.get("not_derived_because"):
    print(f"  not derived because: {m['not_derived_because']}", file=sys.stderr)
PY
    chat_rc=$?
    set -e
    if [ "$chat_rc" != 0 ]; then
      CHAIN_LEG_NOTE="$CHAIN_LEG_NOTE
  $kind: the gateway request itself failed — see $WORK_DIR/gateway.log"
      idx=$((idx + 1)); continue
    fi
    status=$(python3 -c '
import json,sys
p = json.load(open(sys.argv[1])).get("misaka", {})
d = p.get("derivation") or {}
print(d.get("status", "absent"))' "$WORK_DIR/chat-$kind.json")
    if [ "$status" != "derived" ]; then
      # X4, and at the rows registered today it is the EXPECTED outcome. The width report already
      # said whether the row could have held the DSL; that is what turns this from "the model was
      # bad at JSON" into a number.
      why=$(python3 -c '
import json,sys
p = json.load(open(sys.argv[1])).get("misaka", {})
d = p.get("derivation") or {}
print(d.get("reason") or p.get("not_derived_because") or "no derivation block in the response")' "$WORK_DIR/chat-$kind.json")
      CHAIN_LEG_NOTE="$CHAIN_LEG_NOTE
  $kind: the class answered and the answer did not derive (ADR-0078 X4) — $why"
      idx=$((idx + 1)); continue
    fi

    # THE REAL LEG. The gateway signed it; submit it, wait for every node to carry it, then read it
    # BACK from the chain and check what the chain actually holds.
    ANSWER_DERIVED[$idx]="yes"
    claim=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["misaka"]["fp_claim_id"])' "$WORK_DIR/chat-$kind.json")
    stem=$(python3 -c '
import json,sys
p = json.load(open(sys.argv[1]))["misaka"]
print(p["derivation"]["files"]["object"].rsplit(".derived-unsigned.borsh", 1)[0])' "$WORK_DIR/chat-$kind.json")
    signed="$stem.derived-object.borsh"
    if [ ! -f "$signed" ]; then
      "$RAIL_BIN" --derive-artifact "$stem" --bond-key-seed "$WORK_DIR/keys/bond-0.seed" >"$WORK_DIR/derived/$kind.sign.json" 2>&1 \
        || { cat "$WORK_DIR/derived/$kind.sign.json" >&2; die "signing the $kind derivation failed"; }
    fi
    submit "$signed"
    all_nodes_logged "DerivedArtifact" || log "  WARNING: not every node logged a DerivedArtifact"

    # WHAT THE CHAIN HOLDS. Read the payload back and assert it is hashes and sizes: no field of
    # the response may be long enough to be the artifact, and the artifact's own bytes must not
    # occur in it.
    "${CLI[@]}" palw derived "$claim" --json >"$WORK_DIR/derived/$kind.chain.json" 2>&1 \
      || { cat "$WORK_DIR/derived/$kind.chain.json" >&2; die "the chain does not hold claim $claim"; }
    artifact_file="$WORK_DIR/artifacts/$kind.from-inference.$(python3 -c '
import json,sys; print(json.load(open(sys.argv[1]))["misaka"]["derivation"]["extension"])' "$WORK_DIR/chat-$kind.json")"
    python3 - "$WORK_DIR/chat-$kind.json" "$artifact_file" <<'PY'
import base64, json, sys
d = json.load(open(sys.argv[1]))["misaka"]["derivation"]
inline = d.get("artifact", {}).get("inline_base64")
if inline is None:
    print("the artifact came back as a fetch handle, not inline; nothing written", file=sys.stderr)
    sys.exit(0)
open(sys.argv[2], "wb").write(base64.b64decode(inline))
PY
    set +e
    python3 - "$WORK_DIR/derived/$kind.chain.json" "$artifact_file" <<'PY'
import json, sys
doc = json.load(open(sys.argv[1]))
try:
    art = open(sys.argv[2], "rb").read()
except OSError:
    art = b""
raw = json.dumps(doc)
problems = []
if art and art.hex() in raw.lower():
    problems.append("the artifact occurs hex-encoded in what the chain returned")
for a in doc.get("artifacts", []):
    for k, v in a.items():
        if isinstance(v, str) and len(v) > 128:
            problems.append(f"field {k} is {len(v)} chars — too long to be a hash")
    if not isinstance(a.get("artifact_bytes"), int):
        problems.append("artifact_bytes is not an integer")
print(json.dumps({
    "claim": doc["claim_id"],
    "derivations": [{k: a[k] for k in ("kind_name", "dsl_hash", "artifact_hash", "artifact_bytes")} for a in doc.get("artifacts", [])],
    "output_token_ids_on_chain": doc.get("output_token_ids"),
    "problems": problems,
    "verdict": "the chain holds hashes and sizes only (ADR-0078 Decision 1)" if not problems else "THE CHAIN CARRIES MORE THAN A DERIVATION",
}, indent=2))
sys.exit(0 if not problems else 1)
PY
    payload_rc=$?
    set -e
    [ "$payload_rc" = 0 ] || die "the chain payload for $kind is not hashes and sizes — see $WORK_DIR/derived/$kind.chain.json"

    # THE STRANGER, AGAINST THE CHAIN. Same directory discipline: the gateway response (which
    # carries the ids, the job context hash, the family and the canonical DSL) and nothing else.
    sd="$WORK_DIR/stranger/$kind-chain"
    mkdir -p "$sd"
    cp "$WORK_DIR/chat-$kind.json" "$sd/response.json"
    log "  the chain stranger holds: $(cd "$sd" && ls | tr '\n' ' ')"
    ( cd "$sd" && "${CLI[@]}" palw derived-verify "$claim" --answer response.json --json ) \
      >"$WORK_DIR/derived/$kind.chain-verify.json" 2>&1 \
      || { cat "$WORK_DIR/derived/$kind.chain-verify.json" >&2; die "the stranger could not verify the $kind derivation against the chain"; }
    CHAIN_LEG_NOTE="$CHAIN_LEG_NOTE
  $kind: FROM-A-REAL-INFERENCE — claim ${claim:0:16}…, derivation on chain, stranger verified"
    idx=$((idx + 1))
  done
fi

# =============================================================================================
# VERDICT
# =============================================================================================
log "================ verdict ================"
log "row: n_ctx $N_CTX     tokenizer: $MISAKA_PALW_TOKENIZER"
log "chain: $CHAIN_NOTE"
fail=0
idx=0
for kind in $KINDS; do
  w="${WIDTH_VERDICT[$idx]}"
  if [ "${ANSWER_DERIVED[$idx]}" = "yes" ]; then
    log "$kind: FROM-A-REAL-INFERENCE — the class wrote the DSL, the transformer made the artifact, the chain holds the derivation"
  elif [ "$w" = "BLOCKED-ON-WIDTH" ]; then
    python3 - "$WORK_DIR/width/$kind.json" "$kind" <<'PY' >&2
import json, sys
w = json.load(open(sys.argv[1]))
print("[artifact-demo] {k}: BLOCKED-ON-WIDTH — the registered row cannot hold this DSL. "
      "n_ctx {n_ctx}; the prompt costs {prompt_tokens} tokens, leaving {decode_budget_tokens} for the answer; "
      "the canonical DSL is {canonical_dsl_bytes} bytes = {canonical_dsl_tokens} tokens; short by {shortfall_tokens}. "
      "A row of n_ctx >= {required_n_ctx} makes this path a pass with no change here.".format(k=sys.argv[2], **w))
PY
    log "$kind: the transformer half is proven NOT-FROM-AN-INFERENCE (hand-written DSL -> artifact -> stranger recomputation)"
  else
    log "$kind: NOT-FROM-AN-INFERENCE — the row admits this DSL, and no inference produced one on this run"
  fi
  idx=$((idx + 1))
done
if [ -n "${CHAIN_LEG_NOTE// /}" ]; then log "chain leg:$CHAIN_LEG_NOTE"; fi
log "artifacts a person keeps: $WORK_DIR/artifacts"
ls -la "$WORK_DIR/artifacts" >&2

any_real=0
for v in "${ANSWER_DERIVED[@]}"; do [ "$v" = "yes" ] && any_real=1; done
if [ "$any_real" = 1 ]; then
  log "PASS — ADR-0078's acceptance condition is met on at least one path: a certified class was asked, the artifact is on disk, and the chain carries the derivation and nothing else."
else
  log "The acceptance condition is NOT met on this run: no artifact came from an inference."
  log "The mechanism is proven and the blocker is named above. This exits non-zero on purpose —"
  log "a drill that returned 0 here would let a launch read the mechanism proof as the condition."
  fail=1
fi
exit $fail
