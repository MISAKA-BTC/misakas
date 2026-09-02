#!/usr/bin/env bash
# misaka-palw-derived-proof.sh — does ADR-0078's derived leg actually work, offline?
#
# The launch's acceptance condition is not "the code is correct", it is "a person gets a thing":
# a model writes an answer, the chain carries only the DERIVATION of it (ADR-0078 Decision 1 — the
# claim, the grammar, the transformer, two hashes and a size, never the bytes), and a stranger
# holding the answer recomputes the artifact and opens it. Everything else in the lane can hold
# while that last clause is false, so this drill asks the four questions nobody had asked:
#
#   1. THE FILES ARE REAL FILES. Not "the hash matched" — the format's own invariants, and then
#      the bytes handed to tools that are NOT ours: `file(1)`'s magic tables, and (on macOS)
#      Apple's AudioToolbox, ModelIO and ImageIO, which are three shipping readers that know
#      nothing about this repository. The in-tree half is
#      `misaka-palw-derive/tests/artifact_structure.rs`, run here too when a cargo is available.
#   2. THE CONSUMER PATH WORKS FROM THE ANSWER ALONE (Decision 5 / X6). Derive, sign the object
#      with the executor's own bond key through `misaka-palw-fp-rail --derive-artifact`, and
#      verify it with `palw-derive verify` — including the claim's `output_root` recomputed from
#      the answer's token ids. Then corrupt the answer and require a refusal that NAMES the field.
#   3. X3, THE TWO-ARCHITECTURE DRILL, ACTUALLY RUN. Natively and under a second architecture,
#      then one report `--check`ed against the other.
#   4. THE SIZE CLAIM, MEASURED. "The chain holds a few hundred bytes and the GLB holds the
#      megabytes" is a promise with three numbers in it; this prints all three.
#
# WHAT THIS DRILL DOES NOT PROVE, said here because a drill that overclaims is worse than none:
#   * it runs NO model and touches NO chain. It proves the derived leg over the corpus answers,
#     not that any registered class can emit an answer wide enough to carry one (that is the row
#     width question, and it is measured elsewhere);
#   * `misaka palw derived-verify` reads the chain over wRPC, so it is exercised here only when
#     MISAKA_NODE_RPC names a node holding the claim; without one the step is a NAMED SKIP and
#     never a silent pass;
#   * a structural check plus a system reader that opened the file is not a conformance suite.
#     It is what can be honestly claimed without shipping a glTF validator into a consensus tree.
#
# Env: PALW_DERIVE_BIN, RAIL_BIN, CLI_BIN (defaults target/debug/*), CARGO (default `cargo`),
#      CROSS_TARGET (default x86_64-apple-darwin), WORK_DIR, MISAKA_NODE_RPC, MISAKA_NETWORK,
#      SKIP_CROSS=1, SKIP_CARGO=1.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PALW_DERIVE_BIN="${PALW_DERIVE_BIN:-$REPO_ROOT/target/debug/palw-derive}"
RAIL_BIN="${RAIL_BIN:-$REPO_ROOT/target/debug/misaka-palw-fp-rail}"
CLI_BIN="${CLI_BIN:-$REPO_ROOT/target/debug/misaka}"
CARGO="${CARGO:-cargo}"
CROSS_TARGET="${CROSS_TARGET:-x86_64-apple-darwin}"
WORK_DIR="${WORK_DIR:-$REPO_ROOT/.misaka-palw-derived-proof}"
CORPUS="$REPO_ROOT/misaka-palw-derive/corpus"
MISAKA_NETWORK="${MISAKA_NETWORK:-testnet-11}"

log()  { printf '[derived-proof] %s\n' "$*" >&2; }
die()  { log "FATAL: $*"; exit 1; }
# Prose belongs in `die` and never inside a `${VAR:?message}`: macOS ships bash 3.2, which
# re-parses quotes INSIDE a `:?` word, so one apostrophe there opens a quote that swallows the
# rest of the file and lands the failure somewhere unrelated. (The other bash 3.2 trap this file
# stays clear of is `${array[-1]}`, which is a PARSE error, not a runtime one.)
PASS=0; FAIL=0; SKIP=0
ok()   { PASS=$((PASS+1)); printf '  PASS  %s\n' "$*"; }
bad()  { FAIL=$((FAIL+1)); printf '  FAIL  %s\n' "$*"; }
skip() { SKIP=$((SKIP+1)); printf '  SKIP  %s\n' "$*"; }
section() { printf '\n== %s ==\n' "$*"; }

# ---------------------------------------------------------------------------------------------
# Preflight — every refusal BY NAME, before any work
# ---------------------------------------------------------------------------------------------
[ -x "$PALW_DERIVE_BIN" ] || die "PALW_DERIVE_BIN=$PALW_DERIVE_BIN is not executable. Build it: cargo build -p misaka-palw-derive --bins"
# The `code` and `contract` kinds refuse rather than run in-process (ADR-0078 SA-1), so the runner
# has to be beside the tool or the drill measures seven kinds and says eight.
[ -f "$(dirname "$PALW_DERIVE_BIN")/palw-evm-runner" ] || die "palw-evm-runner is not beside $PALW_DERIVE_BIN — build BOTH binaries: cargo build -p misaka-palw-derive --bins"
[ -x "$RAIL_BIN" ] || die "RAIL_BIN=$RAIL_BIN is not executable. Build it: cargo build -p misaka-palw-gateway --bin misaka-palw-fp-rail"
[ -d "$CORPUS" ] || die "the corpus is not at $CORPUS"
command -v python3 >/dev/null || die "python3 is required (it reads the tools' JSON and edits one byte of an answer)"
command -v file >/dev/null || die "file(1) is required — it is the out-of-tree half of question 1"

rm -rf "$WORK_DIR"; mkdir -p "$WORK_DIR/artifacts" "$WORK_DIR/consumer" "$WORK_DIR/x3"

# One JSON field, read with python3 so the drill never guesses at a tool's output shape.
jget() { python3 -c 'import json,sys; d=json.load(open(sys.argv[1]));
for k in sys.argv[2].split("."):
    d = d[k] if isinstance(d, dict) else d[int(k)]
print(d if not isinstance(d,(dict,list)) else json.dumps(d))' "$1" "$2"; }

# =============================================================================================
section "1. the files are real files"
# =============================================================================================
#
# Every one of these four kinds writes a format with a published specification and at least one
# reader that is not ours. The other four registered kinds (`map/mmap/v1`, `simulation/trace/v1`,
# `code/evm/v1`, `contract/evm/v1`) write containers this tree defines, so no out-of-tree reader
# exists to ask and none is invented here: for those the in-tree round trip is the whole claim,
# and that is stated rather than papered over.
derive_one() {  # <transformer> <corpus-relative answer>
  "$PALW_DERIVE_BIN" derive --transformer "$1" --answer "$CORPUS/$2" --out "$WORK_DIR/artifacts" > "$WORK_DIR/artifacts/last.json" 2>&1 \
    || { bad "$1 refused $2: $(cat "$WORK_DIR/artifacts/last.json")"; return 1; }
  jget "$WORK_DIR/artifacts/last.json" files.artifact
}

MIDI="$(derive_one music/smf/v1 music/02-two-track-chords.json)" || true
GLB="$(derive_one scene/glb/v1 scene/02-hierarchy.json)" || true
PNG="$(derive_one image/png/v1 image/02-rect-and-circle.json)" || true
STL="$(derive_one cad/stl/v1 cad/01-extrude-l-bracket.json)" || true

# `file(1)` reads the bytes with libmagic's own tables. For MIDI and GLB it parses fields out of
# the header, which is why the expected string carries them and is not just the format's name.
expect_file() {  # <path> <substring> <what>
  local got; got="$(file -b "$1")"
  case "$got" in
    *"$2"*) ok "file(1) on $3: $got" ;;
    *)      bad "file(1) on $3 says '$got', which does not contain '$2'" ;;
  esac
}
if [ -n "${MIDI:-}" ]; then expect_file "$MIDI" "Standard MIDI data (format 1)" "the MIDI"; else bad "no MIDI was derived"; fi
if [ -n "${GLB:-}" ]; then expect_file "$GLB" "glTF binary model, version 2" "the GLB"; else bad "no GLB was derived"; fi
if [ -n "${PNG:-}" ]; then expect_file "$PNG" "PNG image data" "the PNG"; else bad "no PNG was derived"; fi
# Binary STL is a headerless 84+50n layout with no magic number, so libmagic reports `data` and
# is right to. Saying that here is the point: an expectation of "data" would pass on a file of
# zeros, so STL's out-of-tree reader is ModelIO below and `file(1)` is not asked.
if [ -n "${STL:-}" ]; then skip "file(1) on the STL: binary STL has no magic number, so libmagic cannot speak for it (ModelIO below can)"; else bad "no STL was derived"; fi

# The system readers. Compiled here rather than shipped: three Apple frameworks that have never
# heard of this repository, opening the files and reporting what they found inside.
if command -v swiftc >/dev/null; then
  cat > "$WORK_DIR/reader.swift" <<'SWIFT'
import Foundation
import AudioToolbox
import ModelIO
import ImageIO
let a = CommandLine.arguments
guard a.count >= 3 else { exit(64) }
let url = URL(fileURLWithPath: a[2])
switch a[1] {
case "midi":
    var seq: MusicSequence? = nil
    guard NewMusicSequence(&seq) == noErr, let seq = seq else { print("NewMusicSequence failed"); exit(1) }
    let st = MusicSequenceFileLoad(seq, url as CFURL, .midiType, MusicSequenceLoadFlags())
    guard st == noErr else { print("MusicSequenceFileLoad refused it: OSStatus \(st)"); exit(1) }
    var n: UInt32 = 0; MusicSequenceGetTrackCount(seq, &n)
    var events = 0, notes = 0
    for i in 0..<n {
        var t: MusicTrack? = nil; MusicSequenceGetIndTrack(seq, i, &t)
        guard let t = t else { continue }
        var it: MusicEventIterator? = nil; NewMusicEventIterator(t, &it)
        guard let it = it else { continue }
        var has: DarwinBoolean = false; MusicEventIteratorHasCurrentEvent(it, &has)
        while has.boolValue {
            var ts: MusicTimeStamp = 0; var ty: MusicEventType = 0
            var d: UnsafeRawPointer? = nil; var sz: UInt32 = 0
            MusicEventIteratorGetEventInfo(it, &ts, &ty, &d, &sz)
            events += 1
            if ty == kMusicEventType_MIDINoteMessage { notes += 1 }
            MusicEventIteratorNextEvent(it); MusicEventIteratorHasCurrentEvent(it, &has)
        }
        DisposeMusicEventIterator(it)
    }
    print("AudioToolbox MusicSequenceFileLoad: tracks=\(n) events=\(events) notes=\(notes)")
    if n == 0 || notes == 0 { exit(1) }
case "stl":
    guard MDLAsset.canImportFileExtension("stl") else { print("ModelIO does not import stl on this OS"); exit(2) }
    let asset = MDLAsset(url: url)
    var verts = 0
    for i in 0..<asset.count { if let m = asset.object(at: i) as? MDLMesh { verts += m.vertexCount } }
    print("ModelIO MDLAsset: objects=\(asset.count) vertices=\(verts)")
    if asset.count == 0 || verts == 0 { exit(1) }
case "png":
    guard let src = CGImageSourceCreateWithURL(url as CFURL, nil),
          let img = CGImageSourceCreateImageAtIndex(src, 0, nil) else { print("ImageIO refused the file"); exit(1) }
    print("ImageIO CGImageSource: type=\(CGImageSourceGetType(src) as String? ?? "?") \(img.width)x\(img.height) bpc=\(img.bitsPerComponent) bpp=\(img.bitsPerPixel)")
case "glb":
    // Reported, not asserted: no macOS system framework imports glTF. SceneKit and ModelIO both
    // refuse a .glb, and that is a fact about the platform and not about the artifact — so this
    // arm prints what the platform says and exits 2 (a skip), never a failure.
    print("ModelIO canImportFileExtension(glb) = \(MDLAsset.canImportFileExtension("glb")) — no macOS framework imports glTF")
    exit(2)
default: exit(64)
}
SWIFT
  if swiftc -O -o "$WORK_DIR/reader" "$WORK_DIR/reader.swift" 2>"$WORK_DIR/reader.build.log"; then
    read_with() {  # <mode> <path> <what>
      local out rc
      # `set -e` and a checked exit code do not mix: an assignment whose command substitution
      # fails aborts the shell before the `case` can read it, which is how the GLB arm (a
      # deliberate exit 2) killed this drill the first time it ran.
      out="$("$WORK_DIR/reader" "$1" "$2" 2>&1)" && rc=0 || rc=$?
      case "$rc" in
        0) ok "$3 opened by a system reader: $out" ;;
        2) skip "$3: $out" ;;
        *) bad "$3 was REFUSED by a system reader (exit $rc): $out" ;;
      esac
    }
    if [ -n "${MIDI:-}" ]; then read_with midi "$MIDI" "the MIDI"; fi
    if [ -n "${STL:-}" ];  then read_with stl  "$STL"  "the STL"; fi
    if [ -n "${PNG:-}" ];  then read_with png  "$PNG"  "the PNG"; fi
    if [ -n "${GLB:-}" ];  then read_with glb  "$GLB"  "the GLB"; fi
  else
    skip "the system readers: swiftc could not build them ($(tail -1 "$WORK_DIR/reader.build.log"))"
  fi
else
  skip "the system readers: no swiftc on this host (this half of question 1 is macOS-only)"
fi

# The in-tree structural half: the format's own invariants over EVERY corpus sample, not the four
# derived above. It lives in the crate because it has to run in CI, and it is run here because a
# launch note that quotes one without the other is quoting half a claim.
if [ "${SKIP_CARGO:-0}" = "1" ]; then
  skip "the in-tree structural validators (SKIP_CARGO=1)"
elif command -v "$CARGO" >/dev/null; then
  if (cd "$REPO_ROOT" && MISAKA_PALW_POW_FIXTURE=1 "$CARGO" test -p misaka-palw-derive --test artifact_structure) > "$WORK_DIR/artifact_structure.log" 2>&1; then
    ok "the in-tree structural validators: $(grep -E '^test result:' "$WORK_DIR/artifact_structure.log" | tail -1)"
  else
    bad "the in-tree structural validators FAILED: see $WORK_DIR/artifact_structure.log"
  fi
else
  skip "the in-tree structural validators: no cargo at '$CARGO'"
fi

# =============================================================================================
section "2. the consumer path, from the answer alone (Decision 5 / X6)"
# =============================================================================================
C="$WORK_DIR/consumer"
ANSWER="$CORPUS/music/02-two-track-chords.json"
# A bond key derived from a published string, so this drill is reproducible on someone else's
# machine and the seed is worth nothing.
python3 -c "import hashlib,sys; open(sys.argv[1],'wb').write(hashlib.blake2b(b'misaka-palw-derived-proof/executor/v1',digest_size=32).digest())" "$C/bond.seed"
chmod 600 "$C/bond.seed"
"$RAIL_BIN" --print-bond-pubkey --bond-key-seed "$C/bond.seed" > "$C/pubkey.json"
PUBKEY="$(jget "$C/pubkey.json" executor_pubkey)"
CLAIM="$(python3 -c "import hashlib;print(hashlib.blake2b(b'misaka-palw-derived-proof/claim',digest_size=64).hexdigest())")"
DOMAIN="$(python3 -c "import hashlib;print(hashlib.blake2b(b'misaka-palw-derived-proof/network-domain',digest_size=64).hexdigest())")"
CTX="$(python3 -c "import hashlib;print(hashlib.blake2b(b'misaka-palw-derived-proof/job-context',digest_size=64).hexdigest())")"
FAMILY="qwen36"
printf '[7,11,13,42,1024]\n' > "$C/output-token-ids.json"

# **The output_root is the answer's, not a constant.** X6's first recomputation is only a check
# if the value it compares against was derived from the ids; a claim bound to an arbitrary hash
# would make `output_root_matches: false` the expected result and prove nothing. There is no
# shipped tool that prints the commitment on its own, so it is BOOTSTRAPPED out of the verifier:
# derive once against a placeholder, let `verify` recompute the root from the ids and report the
# mismatch, then derive again binding that value. The second object is one a real gateway would
# have produced, and the pass at the end is a real one.
derive_bound() {  # <output_root hex> <answer path> -> stem
  local out="$C/round-$2"; rm -rf "$out"; mkdir -p "$out"
  "$PALW_DERIVE_BIN" derive --transformer music/smf/v1 --answer "$3" --out "$out" \
      --claim "$CLAIM" --network-domain "$DOMAIN" --output-root "$1" --executor-pubkey "$PUBKEY" > "$out/derive.json"
  local art; art="$(jget "$out/derive.json" files.object)"
  printf '%s' "${art%.derived-unsigned.borsh}"
}
ZERO="$(python3 -c "print('0'*128)")"
STEM0="$(derive_bound "$ZERO" 0 "$ANSWER")"
"$PALW_DERIVE_BIN" verify --object "$STEM0.derived-unsigned.borsh" --answer "$ANSWER" \
    --output-token-ids "$C/output-token-ids.json" --job-context-hash "$CTX" --family "$FAMILY" > "$C/bootstrap.json" 2>&1 || true
ROOT="$(jget "$C/bootstrap.json" recomputed_output_root)"
[ "${#ROOT}" -eq 128 ] || die "the verifier did not hand back a 128-hex output_root; see $C/bootstrap.json"
STEM="$(derive_bound "$ROOT" 1 "$ANSWER")"

# The executor signs its own derivation with the bond key the claim names (Decision 4). The rail
# refuses outright if the key is not the object's `executor_pubkey`, which is why the derivation
# above was bound to this key's bytes.
"$RAIL_BIN" --derive-artifact "$STEM" --bond-key-seed "$C/bond.seed" > "$C/signed.json" 2>&1 \
  || { bad "misaka-palw-fp-rail refused to sign: $(cat "$C/signed.json")"; }
OBJECT="$STEM.derived-object.borsh"
if [ -f "$OBJECT" ]; then ok "misaka-palw-fp-rail --derive-artifact signed it: $(jget "$C/signed.json" derived_id | cut -c1-16)…"; else bad "no signed object at $OBJECT"; fi

DSL="$STEM.dsl"
ART="$(jget "$C/round-1/derive.json" files.artifact)"
"$PALW_DERIVE_BIN" verify --object "$OBJECT" --answer "$ANSWER" --artifact "$ART" \
    --output-token-ids "$C/output-token-ids.json" --job-context-hash "$CTX" --family "$FAMILY" > "$C/verify-true.json" 2>&1 \
  && VRC=0 || VRC=$?
V="$(jget "$C/verify-true.json" verdict)"
if [ "$VRC" -eq 0 ] && [ "$V" = "consistent" ]; then
  ok "palw-derive verify (signed object, exit 0): $V"
else
  bad "palw-derive verify returned $VRC and said '$V' — see $C/verify-true.json"
fi
# The verifier RECOMPUTES: it prints the values it derived itself, and they are the object's.
for f in dsl_hash artifact_hash output_root; do
  case "$f" in
    output_root) got="$(jget "$C/verify-true.json" recomputed_output_root)"; want="$ROOT" ;;
    *)           got="$(jget "$C/verify-true.json" recomputed_$f)"; want="$(jget "$C/round-1/derive.json" $f)" ;;
  esac
  if [ "$got" = "$want" ]; then ok "the verifier recomputed $f itself and got the object's value"; else bad "$f: verifier recomputed $got, object holds $want"; fi
done
for f in dsl_hash_matches artifact_hash_matches artifact_bytes_matches kind_matches artifact_file_matches output_root_matches; do
  if [ "$(jget "$C/verify-true.json" $f)" = "True" ]; then ok "X6: $f"; else bad "X6: $f is not true"; fi
done

# ---- and now the demonstration Decision 5 exists for -------------------------------------
#
# "A false object is publicly demonstrable by anyone holding the DSL." One byte of the answer,
# changed to a value the grammar still accepts, must produce a refusal that NAMES the hash that
# moved — an object that verified against a different answer would make the whole leg decorative.
python3 - "$ANSWER" "$C/answer-corrupt.json" <<'PY'
import json, sys
doc = json.load(open(sys.argv[1]))
# The last note of the last track, one tick shorter: still a legal answer under the grammar, so
# what fails is the arithmetic and not the parse.
doc["tracks"][-1]["notes"][-1]["duration"] -= 1
json.dump(doc, open(sys.argv[2], "w"))
PY
"$PALW_DERIVE_BIN" verify --object "$OBJECT" --answer "$C/answer-corrupt.json" --artifact "$ART" \
    --output-token-ids "$C/output-token-ids.json" --job-context-hash "$CTX" --family "$FAMILY" > "$C/verify-false.json" 2>&1 \
  && FRC=0 || FRC=$?
if [ "$FRC" -eq 2 ]; then ok "a one-byte-different answer: palw-derive verify exits 2"; else bad "a corrupted answer verified with exit $FRC"; fi
FV="$(jget "$C/verify-false.json" verdict)"
case "$FV" in
  *MISMATCH*) ok "and says so: $FV" ;;
  *)          bad "the verdict on a false object is '$FV'" ;;
esac
# BY NAME: which hash moved, with both values printed, or the demonstration is unusable.
for f in dsl_hash_matches artifact_hash_matches; do
  if [ "$(jget "$C/verify-false.json" $f)" = "False" ]; then ok "named the field: $f is false"; else bad "$f did not move on a corrupted answer"; fi
done
log "  on chain   dsl_hash $(jget "$C/round-1/derive.json" dsl_hash | cut -c1-32)…"
log "  recomputed dsl_hash $(jget "$C/verify-false.json" recomputed_dsl_hash | cut -c1-32)…"
# The claim's own commitment is untouched by a corrupted answer only if the ids are untouched:
# this says the three recomputations are independent, which is what makes the first one useful.
if [ "$(jget "$C/verify-false.json" output_root_matches)" = "True" ]; then
  ok "and output_root still matches — the three recomputations are independent"
else
  bad "output_root moved too, so the verdict cannot localise the falsehood"
fi

# An answer that does not parse at all is the OTHER refusal (X4: no object, claim untouched).
printf '{"v":1,"tracks":[' > "$C/answer-unparseable.json"
"$PALW_DERIVE_BIN" verify --object "$OBJECT" --answer "$C/answer-unparseable.json" > "$C/verify-unparseable.json" 2>&1 || true
if python3 -c "import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if 'derivation_rerun' in d else 1)" "$C/verify-unparseable.json"; then
  ok "an answer that does not parse: $(jget "$C/verify-unparseable.json" derivation_rerun | cut -c1-70)…"
else
  bad "an unparseable answer did not produce a re-run refusal"
fi

# `misaka palw derived-verify` is the same arithmetic against what a NODE returns, so it needs a
# node holding this claim. Without one it is skipped BY NAME with the command an operator runs.
if [ -n "${MISAKA_NODE_RPC:-}" ] && [ -x "$CLI_BIN" ]; then
  if "$CLI_BIN" --network "$MISAKA_NETWORK" --rpc "$MISAKA_NODE_RPC" palw derived-verify "$CLAIM" \
       --answer "$C/output-token-ids.json" --dsl "$DSL" --job-context-hash "$CTX" --family "$FAMILY" --json > "$C/chain-verify.json" 2>&1; then
    ok "misaka palw derived-verify against $MISAKA_NODE_RPC: $(jget "$C/chain-verify.json" verdict)"
  else
    bad "misaka palw derived-verify against $MISAKA_NODE_RPC failed: $(tail -2 "$C/chain-verify.json")"
  fi
else
  skip "misaka palw derived-verify: it reads the derivation from a node over wRPC, and this drill puts nothing on a chain. Run it where the claim exists: $CLI_BIN --network $MISAKA_NETWORK palw derived-verify <claim-id> --answer <gateway response.json>"
fi

# =============================================================================================
section "3. X3 — the same bytes on two architectures"
# =============================================================================================
"$PALW_DERIVE_BIN" drill --report "$WORK_DIR/x3/native.json" > "$WORK_DIR/x3/native.log" 2>&1 && NRC=0 || NRC=$?
NARCH="$(jget "$WORK_DIR/x3/native.json" arch)"
if [ "$NRC" -eq 0 ]; then
  ok "drill on $NARCH: exit 0, rows $(jget "$WORK_DIR/x3/native.json" rows | python3 -c 'import json,sys;print(len(json.load(sys.stdin)))'), goldens $(jget "$WORK_DIR/x3/native.json" golden.checked), uncovered $(jget "$WORK_DIR/x3/native.json" uncovered)"
else
  # 3 cross-architecture divergence / 4 a moved golden / 5 a ceiling that did not refuse /
  # 6 a registered transformer the corpus never exercised. The number is the finding.
  bad "drill on $NARCH exited $NRC: $(cat "$WORK_DIR/x3/native.log")"
fi

CROSS_BIN="$REPO_ROOT/target/$CROSS_TARGET/debug/palw-derive"
if [ "${SKIP_CROSS:-0}" = "1" ]; then
  skip "the second architecture (SKIP_CROSS=1)"
elif [ ! -x "$CROSS_BIN" ]; then
  # Said with the command rather than reported as held: "X3 could not be run here" and "X3 holds"
  # are different sentences and the launch note must not confuse them.
  skip "the second architecture: no $CROSS_BIN. Build it: rustup target add $CROSS_TARGET && MISAKA_PALW_POW_FIXTURE=1 cargo build -p misaka-palw-derive --bins --target $CROSS_TARGET"
else
  [ -f "$REPO_ROOT/target/$CROSS_TARGET/debug/palw-evm-runner" ] || die "palw-evm-runner was not built for $CROSS_TARGET — the code and contract rows would be refused there and derived here, which reads as a divergence"
  "$CROSS_BIN" drill --report "$WORK_DIR/x3/cross.json" > "$WORK_DIR/x3/cross.log" 2>&1 && XRC=0 || XRC=$?
  if [ "$XRC" -eq 0 ]; then
    ok "drill on $(jget "$WORK_DIR/x3/cross.json" arch): exit 0"
  else
    bad "drill on $CROSS_TARGET exited $XRC: $(cat "$WORK_DIR/x3/cross.log")"
  fi
  "$PALW_DERIVE_BIN" drill --check "$WORK_DIR/x3/cross.json" > "$WORK_DIR/x3/check.json" 2>&1 && CRC=0 || CRC=$?
  CV="$(jget "$WORK_DIR/x3/check.json" verdict)"
  if [ "$CRC" -eq 0 ]; then
    ok "X3 --check (exit 0): $CV over $(jget "$WORK_DIR/x3/check.json" rows_compared) rows and $(jget "$WORK_DIR/x3/check.json" refusals_compared) refusals"
  else
    bad "X3 --check exited $CRC: $CV / $(jget "$WORK_DIR/x3/check.json" diverged)"
  fi
fi

# =============================================================================================
section "4. the size claim, measured"
# =============================================================================================
# ADR-0078's promise is that the chain holds a few hundred bytes and the artifact holds the rest.
# The DerivedArtifactV1 is dominated by ONE field — the executor's ML-DSA-87 public key — so the
# honest report is the object, the key inside it, and the difference; a single number here would
# either overstate the burden or hide the key.
PUBKEY_BYTES=$(( ${#PUBKEY} / 2 ))
UNSIGNED_BYTES=$(wc -c < "$STEM.derived-unsigned.borsh" | tr -d ' ')
SIGNED_BYTES=$(wc -c < "$OBJECT" | tr -d ' ')
printf '  %-46s %10s bytes\n' "the MIDI artifact (music/smf/v1)"        "$(wc -c < "$MIDI" | tr -d ' ')"
printf '  %-46s %10s bytes\n' "the GLB artifact (scene/glb/v1)"         "$(wc -c < "$GLB" | tr -d ' ')"
printf '  %-46s %10s bytes\n' "the PNG artifact (image/png/v1)"         "$(wc -c < "$PNG" | tr -d ' ')"
printf '  %-46s %10s bytes\n' "the STL artifact (cad/stl/v1)"           "$(wc -c < "$STL" | tr -d ' ')"
printf '  %-46s %10s bytes\n' "the canonical DSL the chain does NOT hold" "$(wc -c < "$DSL" | tr -d ' ')"
printf '  %-46s %10s bytes\n' "PalwDerivedArtifactV1, unsigned"         "$UNSIGNED_BYTES"
printf '  %-46s %10s bytes\n' "  of which the ML-DSA-87 executor pubkey" "$PUBKEY_BYTES"
printf '  %-46s %10s bytes\n' "  the derivation's own fields"           "$(( UNSIGNED_BYTES - PUBKEY_BYTES ))"
printf '  %-46s %10s bytes\n' "the signed consensus object that rides"  "$SIGNED_BYTES"
# The largest artifact this corpus produces, so the claim is measured at its own ceiling and not
# at its smallest sample.
BIG="$(python3 -c "
import json,sys
d=json.load(open(sys.argv[1]))['rows']
k=max(d,key=lambda k:d[k]['artifact_bytes'])
print(k, d[k]['artifact_bytes'])" "$WORK_DIR/x3/native.json")"
printf '  %-46s %s\n' "the corpus's largest artifact" "$BIG"

# =============================================================================================
printf '\n[derived-proof] PASS %d  FAIL %d  SKIP %d   (work dir %s)\n' "$PASS" "$FAIL" "$SKIP" "$WORK_DIR"
if [ "$FAIL" -gt 0 ]; then
  log "the derived leg does NOT hold as measured here — see the FAIL lines above"
  exit 1
fi
log "the derived leg holds over the shipped corpus, offline, with no chain and no model"
exit 0
