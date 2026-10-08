#!/usr/bin/env bash
# tests/hf-onboarding/onboard.sh — one model through H1's stages, as a USER would, with every result written as evidence.
#
#   onboard.sh <stage> <model-id>      (model ids and pins: models.json; RUN=<run-id> names the evidence directory)
#
#   source       the pinned revision's file list (hub metadata only) and, for a local checkpoint, every file's hash against it
#   preflight    `misaka model preflight hf://REPO@REV --depth shape` (headers only; the widest context and the declared one)
#   artifact     calibration tokens + HF reference (offline, the model's own tokenizer / transformers float32) -> `palw-class pack
#                build` (convert, lower, declare the exact layout) -> `pack verify --strict --rebuild --model` (a fresh rebuild from the
#                public source must reproduce the artifact's roots)
#   conformance  `pack commit-conformance` bound to the devnet's genesis and ruleset -> SYNTHETIC beacon facts -> `run-conformance` ->
#                `verify-conformance` in a fresh process (label SYNTHETIC_BEACON_CONFORMANCE_PASS at most; never on-chain)
#   register     client U: `misaka palw tir-registration` (pack gate on) -> `misaka palw submit-object` through B -> wait for the fold
#   observe      U's queries, then A/B/C agreement on the class row, the IR record and the registry; writes consensus-state.json
#   summary      g14-gates.json (what the chain can and cannot say), the level reached, summary.md
#
# Env: RUN (default <date>-<model>), CACHE=0 forces a clean-source artifact build (no reuse of an earlier conversion), and lib.sh's.
set -euo pipefail
. "$(cd "$(dirname "$0")" && pwd)/lib.sh"
h1_snapshot_exec onboard.sh "$@"
stage=${1:?stage}; MID=${2:?model id}
MODELS=$H1/models.json
mf() { python3 "$H1/report.py" field "$MODELS" "$MID" "$1"; }
REPO=$(mf repo); REV=$(mf revision); CTX=$(mf context); LOCAL=$(mf local); TASK=$(mf task)
RUN=${RUN:-$(date +%Y%m%d)-$MID}
EV=$WT/docs/evidence/hf-onboarding/$RUN
SCR=$RUN_ROOT/runs/$RUN                     # bulky outputs (artifact, pack, logs) — never committed
CACHE_ROOT=$RUN_ROOT/cache
VENV_PY=${VENV_PY:-/Users/wata/Downloads/MISAKA-wt-b/tir-venv/bin/python}
TOOLS=$WT/misaka-palw-tir-lower/tools
SHA=$(git -C "$WT" rev-parse HEAD)
mkdir -p "$EV" "$SCR"
FAIL=$EV/failures.json
[ -s "$FAIL" ] || echo "[]" > "$FAIL"
fail() { # fail <stage> <category|auto> <code> <command> <expected> <log> [security]
    local cat=$2 op=add; [ "$cat" = auto ] && { op=auto; cat=""; }
    python3 "$H1/classify.py" "$op" --failures "$FAIL" --model "$MID" --stage "$1" ${cat:+--category "$cat"} ${3:+--code "$3"} \
        --command "$4" --expected "$5" --log "$6" --revision "$REV" --sha "$SHA" --security "${7:-}" \
        --repro "RUN=$RUN bash tests/hf-onboarding/onboard.sh $1 $MID"
}
jmerge() { python3 "$H1/report.py" merge "$@"; }
pc() { "$PALW_CLASS_BIN" "$@"; }

model_json() {
    python3 "$H1/report.py" model "$MODELS" "$MID" > "$EV/model.json.tmp"
    jmerge "$EV/model.json.tmp" "source=$SCR/source.json" >/dev/null 2>&1 || true
    mv "$EV/model.json.tmp" "$EV/model.json"
}

env_json() {
    WORK_DIR=$WORK_DIR bash "$H1/devnet.sh" env > "$EV/environment.json"
    local free; free=$(df -g / | awk 'NR==2 {print $4}')
    jmerge "$EV/environment.json" "disk_free_gib=$free" "devnet_work_dir=$WORK_DIR" \
        "fences={\"fence_at\":$FENCE_AT,\"fence2_at\":$FENCE2_AT,\"fence3_at\":$FENCE3_AT,\"tir_at\":$TIR_AT,\"tir2_at\":$TIR2_AT,\"int11_at\":$INT11_AT,\"fence4_at\":\"${FENCE4_AT:-unmoved (5300 in the shipped schedule; int-11 carries palw_gdn_key_heads)}\"}" \
        "cli_version=$("$CLI_BIN" --version 2>/dev/null | head -1)" >/dev/null
}

do_source() {
    local rc=0
    if [ -z "$REPO" ]; then   # an in-repo fixture: its files at the tested SHA are the source
        python3 - "$LOCAL" "$SCR/source.json" "$SHA" <<'PY'
import hashlib, json, os, sys
d, out, sha = sys.argv[1:]
files = [{"path": f, "size": os.path.getsize(os.path.join(d, f)), "sha256": hashlib.sha256(open(os.path.join(d, f), "rb").read()).hexdigest()} for f in sorted(os.listdir(d))]
json.dump({"schema": "misaka.h1.source.v1", "repo": None, "revision": None, "resolved_sha": None, "fixture_dir": d, "integration_sha": sha,
           "note": "an in-repo synthetic fixture: no Hub revision exists; its bytes at the tested SHA are the source", "files": files,
           "local": {"dir": d, "verdict": "FIXTURE"}}, open(out, "w"), indent=1)
PY
        model_json; say "$MID source: in-repo fixture"; return 0
    fi
    local file; file=$(mf file); local ldir=$LOCAL
    [ -n "$file" ] && [ -n "$LOCAL" ] && ldir=${LOCAL%/$file}
    if [ -n "$file" ] && [ ! -s "$LOCAL" ]; then ldir=""; fi                # not downloaded yet: metadata only
    python3 "$H1/source_hash.py" "$REPO" "$REV" ${ldir:+--local "$ldir"} ${file:+--require "$file"} --out "$SCR/source.json" > "$SCR/source.out" 2>&1 || rc=$?
    model_json
    case $rc in
        0) say "$MID source: $(cat "$SCR/source.out")" ;;
        2) fail source HF_ACCESS_FAILED "HUB_UNREADABLE" "source_hash.py $REPO $REV" "the hub lists the pinned revision" "$SCR/source.out"; return 1 ;;
        *) fail source HF_REVISION_OR_SOURCE_MISMATCH "SOURCE_HASH_MISMATCH" "source_hash.py $REPO $REV --local $LOCAL" \
               "every required local file matches the pinned revision" "$SCR/source.out"; return 1 ;;
    esac
}

do_preflight() {
    local out=$SCR/preflight; mkdir -p "$out"; local rcw=0 rcd=0
    local input="hf://$REPO@$REV"; local file; file=$(mf file)
    if [ -z "$REPO" ]; then input=$LOCAL                                     # an in-repo fixture
    elif [ -n "$file" ]; then if [ -s "$LOCAL" ]; then input=$LOCAL; else input="https://huggingface.co/$REPO/resolve/$REV/$file"; fi; fi
    echo "$input" > "$out/input.txt"
    HOME=$OPHOME "$CLI_BIN" --network testnet-12 --output json model preflight "$input" --depth shape > "$out/widest.json" 2> "$out/widest.err" || rcw=$?
    HOME=$OPHOME "$CLI_BIN" --network testnet-12 --output json model preflight "$input" --depth shape --max-context "$CTX" > "$out/declared.json" 2> "$out/declared.err" || rcd=$?
    python3 - "$out" "$EV/preflight.json" "$rcw" "$rcd" "$CTX" "$H1" <<'PY'
import json, sys
out, dst, rcw, rcd, ctx, h1 = sys.argv[1:]
sys.path.insert(0, h1)
import report
d = {"schema": "misaka.h1.preflight.v1", "command": "misaka --network testnet-12 --output json model preflight hf://REPO@REV --depth shape [--max-context N]",
     "header_only": True, "widest": {"exit": int(rcw), **report.preflight_summary(out + "/widest.json")},
     "declared": {"exit": int(rcd), "max_context": int(ctx), **report.preflight_summary(out + "/declared.json")}}
json.dump(d, open(dst, "w"), indent=1)
v = d["declared"].get("verdict") or {}
print(json.dumps({"declared": v, "blockers": [b["code"] for b in d["declared"].get("blockers", [])]}))
PY
    # Classify on the report's own blocker codes (never on the whole JSON, whose fence list names KERNEL_NOT_ACTIVE and more).
    python3 - "$EV/preflight.json" > "$out/declared.blockers.txt" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))["declared"]
for b in d.get("blockers") or []:
    print(f"{b.get('code')} [{b.get('stage')}] {b.get('arg') or ''} — {b.get('what')}")
if not d.get("blockers"):   # a stage left unknown with no blocker is itself the finding; with blockers it is their consequence
    for k, why in (d.get("unknown_because") or {}).items():
        print(f"UNKNOWN [{k}] {why}")
for n in d.get("notes") or []:
    if "pipeline class" in n:
        print(f"NOT_RUN_PIPELINE_ADMISSION — {n}")
PY
    local reg_ok; reg_ok=$(python3 -c "import json;v=json.load(open('$EV/preflight.json'))['declared'].get('verdict') or {};print(1 if v.get('convert')=='ok' and v.get('register')=='ok' else 0)")
    if [ "$rcd" != 0 ] || [ "$reg_ok" != 1 ]; then
        # One record per blocker line (a model can stop on a frontend gap AND a resource bound at once).
        local i=0 line
        while IFS= read -r line; do
            i=$((i + 1)); printf '%s\n' "$line" > "$out/blocker-$i.txt"
            fail preflight auto "" "misaka model preflight $(cat "$out/input.txt") --depth shape --max-context $CTX" \
                "convert/register/mine ok at the declared context" "$out/blocker-$i.txt"
        done < "$out/declared.blockers.txt"
        [ "$i" -gt 0 ] || fail preflight TEST_INFRASTRUCTURE_FAILED NO_VERDICT "misaka model preflight" "a verdict" "$out/declared.err"
    fi
}

# The artifact cache key: what the conversion reads (revision, converter binary, options). A changed key is a new conversion.
artifact_key() { printf '%s|%s|%s|%s|%s' "$REPO@$REV" "$(shasum -a 256 "$PALW_CLASS_BIN" | cut -c1-64)" "$CTX" "${CALIB_SPEC}" "${BUILD_OPTS}" | shasum -a 256 | cut -c1-16; }
# Per-model calibration / layout / reference options (models.json: calib_len, calib_chunks, calib_docs, declare, hfref_streamed).
CALIB_LEN=$(mf calib_len); CALIB_LEN=${CALIB_LEN:-512}
CALIB_CHUNKS=$(mf calib_chunks); CALIB_CHUNKS=${CALIB_CHUNKS:-1}
CALIB_DOCS=$(mf calib_docs); CALIB_DOCS=${CALIB_DOCS:-"docs/archival.md docs/crescendo-guide.md CONTRIBUTING.md SECURITY.md"}
DECLARE_EXTRA=$(mf declare); DECLARE_EXTRA=${DECLARE_EXTRA:-":logits-tile=$CTX"}
HFREF_STREAMED=$(mf hfref_streamed)
CALIB_SPEC="len${CALIB_LEN}x${CALIB_CHUNKS}:$CALIB_DOCS"
BUILD_OPTS=${BUILD_OPTS:-"--prompts 1 --prefill 2 --decode 1 --stream"}
CHUNK_STORE=${CHUNK_STORE:-}      # a transient chunk store (deleted after the build) for a large conversion

do_artifact() {
    [ -n "$LOCAL" ] && [ -d "$LOCAL" ] || { fail artifact HF_ACCESS_FAILED "WEIGHTS_NOT_LOCAL" "pack build --model <checkpoint>" \
        "a full local checkpoint of the pinned revision" /dev/null "none: nothing is built"; return 1; }
    local key; key=$(artifact_key); local A=$CACHE_ROOT/$MID/$key; local log=$SCR/artifact.log
    mkdir -p "$A"; : > "$log"
    if [ "${CACHE:-1}" = 0 ]; then say "$MID: CACHE=0 — clean-source build into a fresh dir"; A=$SCR/clean-$key; rm -rf "$A"; mkdir -p "$A"; fi
    local t0=$SECONDS
    if [ ! -s "$A/calib.json" ]; then
        ( cd "$WT"; HF_HUB_OFFLINE=1 "$VENV_PY" -I "$TOOLS/tokenize_docs.py" --tokenizer "$LOCAL/tokenizer.json" --len "$CALIB_LEN" --chunks "$CALIB_CHUNKS" --out "$A/calib.json" $CALIB_DOCS ) >> "$log" 2>&1 \
            || { fail artifact TEST_INFRASTRUCTURE_FAILED CALIB_TOKENS "tokenize_docs.py" "calibration tokens" "$log"; return 1; }
        ( cd "$WT"; HF_HUB_OFFLINE=1 "$VENV_PY" -I "$TOOLS/tokenize_docs.py" --tokenizer "$LOCAL/tokenizer.json" --len 16 --chunks 2 --out "$A/eval.json" README.md ) >> "$log" 2>&1 \
            || { fail artifact TEST_INFRASTRUCTURE_FAILED EVAL_TOKENS "tokenize_docs.py" "evaluation tokens" "$log"; return 1; }
    fi
    if [ ! -s "$A/hfref/hf-reference.json" ]; then
        mkdir -p "$A/hfref"
        local refpy=$TOOLS/hf_reference.py; [ "$HFREF_STREAMED" = True ] || [ "$HFREF_STREAMED" = true ] && refpy=$H1/hf_reference_streamed.py
        /usr/bin/time -l env HF_HUB_OFFLINE=1 "$VENV_PY" -I "$refpy" "$LOCAL" "$A/hfref" --tokens "$A/eval.json" >> "$log" 2>&1 \
            || { fail artifact auto "" "hf_reference.py $LOCAL" "the transformers float32 reference logits" "$log"; return 1; }
    fi
    local build_rc=0
    if [ ! -s "$A/BUILT" ]; then
        rm -rf "$A/pack" "$A/class.palwtir"
        /usr/bin/time -l "$PALW_CLASS_BIN" pack build --model "$LOCAL" --out "$A/class.palwtir" --pack "$A/pack" --calib "$A/calib.json" \
            --context "$CTX" --name "$MID" --repo "$REPO" --revision "$REV" --hf-reference "$A/hfref" \
            --declare "testnet-12:max-context=$CTX$DECLARE_EXTRA" ${CHUNK_STORE:+--chunk-store "$CHUNK_STORE"} $BUILD_OPTS > "$A/build.out" 2> "$A/build.err" || build_rc=$?
        [ -n "$CHUNK_STORE" ] && rm -rf "$CHUNK_STORE"
        cat "$A/build.err" >> "$log"
        if [ "$build_rc" = 0 ]; then date +%s > "$A/BUILT"; fi
    fi
    if [ ! -s "$A/BUILT" ]; then
        fail artifact auto "" "palw-class pack build --model $LOCAL ... --declare testnet-12:max-context=$CTX" "a built, declared artifact and its pack" "$A/build.err"
        return 1
    fi
    # The registrable file is the DECLARED class (`--declare` writes <out>.testnet-12.palwtir: the lowered artifact plus its exact layout).
    local decl; decl=$(ls "$A"/class.palwtir.testnet-12.palwtir 2>/dev/null | head -1)
    [ -n "$decl" ] || { fail artifact LAYOUT_OR_RESOURCE_REFUSED NO_DECLARED_CLASS "palw-class pack build --declare testnet-12:max-context=$CTX" "a declared class file" "$A/build.err"; return 1; }
    echo "$decl" > "$A/DECLARED"
    local vrc=0
    /usr/bin/time -l "$PALW_CLASS_BIN" pack verify "$A/pack" --model "$LOCAL" --artifact "$decl" --rebuild --strict --json \
        > "$A/verify.json" 2> "$A/verify.err" || vrc=$?
    cat "$A/verify.err" >> "$log"
    python3 - "$A" "$EV" "$build_rc" "$vrc" "$key" "$((SECONDS - t0))" "${CACHE:-1}" <<'PY'
import json, os, re, sys, hashlib
A, EV, brc, vrc, key, secs, cache = sys.argv[1:]
def rss(path):
    try:
        m = re.findall(r"(\d+)\s+maximum resident set size", open(path, errors="replace").read())
        return [int(x) for x in m]
    except Exception:
        return []
pack = json.load(open(os.path.join(A, "pack", "pack.json"))) if os.path.exists(os.path.join(A, "pack", "pack.json")) else {}
art = open(os.path.join(A, "DECLARED")).read().strip()
lowered = os.path.join(A, "class.palwtir")
sha = None
if os.path.exists(art):
    h = hashlib.sha256()
    with open(art, "rb") as f:
        for b in iter(lambda: f.read(8 << 20), b""):
            h.update(b)
    sha = h.hexdigest()
bt = re.findall(r"(\d+\.\d+)\s+real", open(os.path.join(A, "build.err"), errors="replace").read()) if os.path.exists(os.path.join(A, "build.err")) else []
fit = re.findall(r"against the reference: ([^\n]+)", open(os.path.join(A, "build.err"), errors="replace").read()) if os.path.exists(os.path.join(A, "build.err")) else []
conv = {"schema": "misaka.h1.conversion.v1", "build_real_seconds": float(bt[0]) if bt else None, "hf_fit_at_build": fit[-1] if fit else None, "cache_key": key, "cache_dir": A, "clean_source": cache == "0", "build_exit": int(brc),
        "wall_seconds_this_invocation": int(secs), "artifact_file": art, "lowered_file": lowered, "lowered_bytes": os.path.getsize(lowered) if os.path.exists(lowered) else None, "artifact_bytes": os.path.getsize(art) if os.path.exists(art) else None,
        "artifact_sha256": sha, "max_rss_bytes_seen": rss(os.path.join(A, "build.err")),
        "pack_digest": open(os.path.join(A, "build.out")).read().strip() if os.path.exists(os.path.join(A, "build.out")) else None,
        "pack": {k: pack.get(k) for k in ("name", "model", "frontend", "features", "quant", "profile", "converter", "artifact", "declared", "conformance", "hf_fit", "layout") if k in pack}}
json.dump(conv, open(os.path.join(EV, "conversion.json"), "w"), indent=1)
try:
    v = json.load(open(os.path.join(A, "verify.json")))
except Exception as e:
    v = {"readable": False, "error": str(e)}
pv = {"schema": "misaka.h1.pack-verification.v1", "command": "palw-class pack verify <pack> --model <checkpoint> --artifact <declared class file> --rebuild --strict --json",
      "exit": int(vrc), "verified": v.get("verified"), "ok": v.get("ok"), "pack": v.get("pack"), "checks": v.get("checks"),
      "skipped": [c for c in (v.get("checks") or []) if c.get("status") == "SKIPPED"],
      "failed": [c for c in (v.get("checks") or []) if c.get("status") == "FAIL"], "max_rss_bytes": rss(os.path.join(A, "verify.err"))}
json.dump(pv, open(os.path.join(EV, "pack-verification.json"), "w"), indent=1)
print(json.dumps({"artifact_sha256": sha, "verified": pv["verified"], "exit": pv["exit"], "failed": [c.get("check") for c in pv["failed"]],
                  "skipped": [c.get("check") for c in pv["skipped"]]}))
PY
    echo "$A" > "$SCR/artifact.dir"
    if [ "$vrc" != 0 ]; then
        if grep -q '"status": "FAIL"' "$A/verify.json" 2>/dev/null; then fail artifact CONFORMANCE_FAILED PACK_VERIFY_FAILED "palw-class pack verify --strict --rebuild" "VERIFIED" "$A/verify.json"
        else fail artifact PACK_INCOMPLETE PACK_NOT_VERIFIED "palw-class pack verify --strict --rebuild" "VERIFIED (nothing skipped)" "$A/verify.json"; fi
        return 1
    fi
}


U_SEED=$UHOME/.misaka/u.seed
u_bond() { python3 -c "import json;print(json.load(open('$UHOME/.misaka/u-bond.json'))['bond_outpoint'])"; }
u_addr() { python3 -c "import json;print(json.load(open('$UHOME/.misaka/u-bond.json'))['address'])"; }
balance_of() { rpc C getUtxosByAddresses "{\"addresses\":[\"$1\"]}" | python3 -c 'import json,sys; print(sum(int(e["utxoEntry"]["amount"]) for e in json.load(sys.stdin).get("entries",[])))'; }
class_of_art() { "$PALW_CLASS_BIN" inspect --network testnet-12 "$1" 2>/dev/null | grep -oE "[0-9a-f]{128}" | head -1; }

do_register() {
    local A; A=$(cat "$SCR/artifact.dir" 2>/dev/null) || true
    [ -n "$A" ] && [ -s "$A/DECLARED" ] || { fail register TEST_INFRASTRUCTURE_FAILED NO_ARTIFACT "onboard.sh artifact" "a verified artifact" /dev/null; return 1; }
    [ -s "$UHOME/.misaka/u-bond.json" ] || { fail register TEST_INFRASTRUCTURE_FAILED NO_USER_BOND "devnet.sh user" "U's own bond" /dev/null; return 1; }
    local out=$SCR/register; mkdir -p "$out"; local bond addr; bond=$(u_bond); addr=$(u_addr)
    # The one-command UX the brief targets: `misaka model add <hf repo@rev | artifact>`. Asked with --quote (pays nothing).
    ucli --output json model add "hf://$REPO@$REV" --quote --allow-single-rpc > "$out/model-add-hf.json" 2> "$out/model-add-hf.err" || true
    ucli --output json model add --artifact "$(cat "$A/DECLARED")" --pack "$A/pack" --quote --allow-single-rpc > "$out/model-add-artifact.json" 2> "$out/model-add-artifact.err" || true
    local bal0; bal0=$(balance_of "$addr"); local daa0; daa0=$(tip C)
    local rc=0
    ucli --output json palw tir-registration --key-file "$U_SEED" --artifact "$(cat "$A/DECLARED")" --bond "$bond" --out "$out/registration.obj" \
        --pack "$A/pack" --pack-source "$LOCAL" > "$out/tir-registration.json" 2> "$out/tir-registration.err" || rc=$?
    if [ "$rc" != 0 ]; then fail register auto "" "misaka palw tir-registration --artifact ... --bond $bond --pack ..." "a signed ClassRegisteredTirV1" "$out/tir-registration.err"; return 1; fi
    local cid root; cid=$(python3 -c "import json;print(json.load(open('$out/tir-registration.json'))['class_id'])"); root=$(python3 -c "import json;print(json.load(open('$out/tir-registration.json'))['artifact_root'])")
    echo "$cid" > "$SCR/class.id"; echo "$root" > "$SCR/artifact.root"
    # The gate the node runs, before anything is paid (the live chain's admission at its tip).
    ucli --output json model preflight "$(cat "$A/DECLARED")" > "$out/live-preflight.json" 2> "$out/live-preflight.err" || true
    rc=0; ucli palw submit-object --key-file "$U_SEED" --object "$out/registration.obj" --yes > "$out/submit.out" 2> "$out/submit.err" || rc=$?
    if [ "$rc" != 0 ]; then fail register auto "" "misaka palw submit-object --object registration.obj --yes" "the carrier accepted by B's mempool" "$out/submit.err"; return 1; fi
    local txid; txid=$(grep -oE "submitted [0-9a-f]{128}" "$out/submit.out" | head -1 | cut -d" " -f2); echo "$txid" > "$SCR/carrier.txid"
    local t0=$SECONDS st=""
    while :; do
        ucli --output json model registration "$cid" > "$out/registration-status.json" 2> "$out/registration-status.err" || true
        st=$(python3 -c "import json;d=json.load(open('$out/registration-status.json'));r=d.get('registration',d);print('folded' if r.get('folded') else (r.get('rejectCode') or r.get('reject_code') or 'pending'))" 2>/dev/null || echo pending)
        [ "$st" = folded ] && break
        if [ "$st" = REGISTRATION_DROPPED ] && [ $((SECONDS - t0)) -gt 300 ]; then
            fail register CONSENSUS_REGISTRATION_REFUSED REGISTRATION_DROPPED "misaka palw submit-object (carrier $txid)" "the registration folded" "$out/registration-status.json" "the carrier fee is spent for nothing"; return 1; fi
        [ $((SECONDS - t0)) -lt "${REG_WAIT:-3600}" ] || { fail register CONSENSUS_REGISTRATION_REFUSED "NOT_FOLDED_IN_${REG_WAIT:-3600}S" "misaka model registration $cid" "folded" "$out/registration-status.json"; return 1; }
        sleep 15
    done
    local bal1; bal1=$(balance_of "$addr")
    python3 - "$out" "$EV/registration.json" "$cid" "$root" "$bond" "$addr" "$bal0" "$bal1" "$txid" "$daa0" "$(tip C)" <<'PY'
import json, os, sys
out, dst, cid, root, bond, addr, b0, b1, txid, d0, d1 = sys.argv[1:]
rd = lambda f: (json.load(open(os.path.join(out, f))) if os.path.getsize(os.path.join(out, f)) else None) if os.path.exists(os.path.join(out, f)) else None
def txt(f):
    p = os.path.join(out, f)
    return open(p, errors="replace").read()[-1500:] if os.path.exists(p) else None
reg = {"schema": "misaka.h1.registration.v1", "party": "client U (no node, no seat, own key/funds/bond)", "rpc": "node B",
       "one_command_ux": {"model add hf://": {"json": txt("model-add-hf.json"), "stderr": txt("model-add-hf.err")},
                          "model add --artifact": {"json": txt("model-add-artifact.json"), "stderr": txt("model-add-artifact.err")}},
       "path_used": "detached: misaka palw tir-registration (pack gate on) + misaka palw submit-object",
       "owner_bond_creation": {"method": "WORKAROUND: transient registrar — U ran kaspad --palw-register-bond once with its own datadir and key, stopped after the bond printed; NOT node-less (NODELESS_BOND_REGISTRATION_ABSENT, C1r2)"},
       "class_id": cid, "artifact_root": root, "owner_bond": bond, "payer_address": addr, "carrier_txid": txid,
       "tir_registration": rd("tir-registration.json"), "live_gate_preflight": rd("live-preflight.json"),
       "status_at_fold": rd("registration-status.json"),
       "u_balance_sompi": {"before": int(b0), "after": int(b1), "spent": int(b0) - int(b1)}, "daa": {"submitted_after": int(d0) if d0.isdigit() else d0, "folded_by": int(d1) if d1.isdigit() else d1}}
json.dump(reg, open(dst, "w"), indent=1)
print(json.dumps({"class_id": cid[:16], "root": root[:16], "spent_sompi": reg["u_balance_sompi"]["spent"]}))
PY
}


# Beacon conformance over the pack (lane C's tooling). Facts are SYNTHETIC: the node RPC for canonical beacon facts does not exist, so
# the best this stage can say is SYNTHETIC_BEACON_CONFORMANCE_PASS — never on-chain conformance, never G14.
CONF_POLICY=${CONF_POLICY:-"--k 3 --delay 2 --window 40 --depth 5 --repetitions 3 --security-bits 4"}
CONF_SCOPE=${CONF_SCOPE:-"--vectors 1 --prompt-len 2 --decode 1 --leaves 256 --vector-fault-ppm 1000000 --leaf-fault-ppm 62500"}
do_conformance() {
    local A; A=$(cat "$SCR/artifact.dir" 2>/dev/null) || true
    [ -n "$A" ] && [ -s "$A/DECLARED" ] || { fail conformance TEST_INFRASTRUCTURE_FAILED NO_ARTIFACT "onboard.sh artifact" "a verified artifact" /dev/null; return 1; }
    local C=$SCR/conformance; mkdir -p "$C"      # an existing state resumes (completion records are reused, never trusted blindly)
    local gen; gen=$(manifest "m['genesis_hash']"); local params; params=$(manifest "m['consensus_params_id']")
    local rc=0
    /usr/bin/time -l "$PALW_CLASS_BIN" pack commit-conformance --pack "$A/pack" --artifact "$(cat "$A/DECLARED")" --state "$C/state" --network testnet-12 \
        --chain-genesis "$gen" --ruleset-id "label:consensus_params_id=$params" $CONF_POLICY $CONF_SCOPE > "$C/commit.out" 2> "$C/commit.err" || rc=$?
    local stmt; stmt=$(grep -oE "[0-9a-f]{32,128}" "$C/commit.out" | head -1)
    [ -z "$stmt" ] && stmt=$(ls "$C/state" 2>/dev/null | grep -E "^[0-9a-f]{32}$" | head -1)
    if [ "$rc" != 0 ] || [ -z "$stmt" ]; then fail conformance auto "" "palw-class pack commit-conformance" "a ConformanceCommitmentV1" "$C/commit.err"; return 1; fi
    local w; w=$(python3 -c "print(int('${CONF_POLICY}'.split('--k ')[1].split()[0]))")
    "$PALW_CLASS_BIN" pack synthetic-facts --state "$C/state" --commitment "$stmt" --out "$C/facts-waiting.json" --works $((w - 1)) --label "h1-$RUN" 2>> "$C/facts.err" || true
    local wrc=0; "$PALW_CLASS_BIN" pack run-conformance --pack "$A/pack" --artifact "$(cat "$A/DECLARED")" --state "$C/state" --commitment "$stmt" --facts "$C/facts-waiting.json" > "$C/run-waiting.out" 2> "$C/run-waiting.err" || wrc=$?
    "$PALW_CLASS_BIN" pack synthetic-facts --state "$C/state" --commitment "$stmt" --out "$C/facts.json" --label "h1-$RUN" 2>> "$C/facts.err" || true
    local rrc=0; /usr/bin/time -l "$PALW_CLASS_BIN" pack run-conformance --pack "$A/pack" --artifact "$(cat "$A/DECLARED")" --state "$C/state" --commitment "$stmt" --facts "$C/facts.json" > "$C/run.out" 2> "$C/run.err" || rrc=$?
    local ev; ev=$(find "$C/state" -name evidence.borsh | head -1)
    local vrc=9 nrc=9
    if [ -n "$ev" ]; then
        /usr/bin/time -l "$PALW_CLASS_BIN" pack verify-conformance --pack "$A/pack" --artifact "$(cat "$A/DECLARED")" --state "$C/state" --commitment "$stmt" --facts "$C/facts.json" --evidence "$ev" > "$C/verify.out" 2> "$C/verify.err" && vrc=0 || vrc=$?
        "$PALW_CLASS_BIN" pack verify-conformance --pack "$A/pack" --artifact "$(cat "$A/DECLARED")" --state "$C/state" --commitment "$stmt" --facts "$C/facts.json" --evidence "$ev" --no-rerun > "$C/verify-norerun.out" 2> "$C/verify-norerun.err" && nrc=0 || nrc=$?
    fi
    "$PALW_CLASS_BIN" pack conformance-status --state "$C/state" > "$C/status.out" 2>&1 || true
    python3 - "$C" "$EV/pack-verification.json" "$stmt" "$wrc" "$rrc" "$vrc" "$nrc" "${ev:-}" "$CONF_POLICY" "$CONF_SCOPE" <<'PY'
import json, os, re, sys
C, pv, stmt, wrc, rrc, vrc, nrc, ev, pol, scope = sys.argv[1:]
t = lambda f: open(os.path.join(C, f), errors="replace").read()[-2500:] if os.path.exists(os.path.join(C, f)) else None
rss = lambda f: [int(x) for x in re.findall(r"(\d+)\s+maximum resident set size", t(f) or "")]
secs = lambda f: [float(x) for x in re.findall(r"(\d+\.\d+)\s+real", t(f) or "")]
d = json.load(open(pv)) if os.path.exists(pv) else {}
passed = rrc == "0" and vrc == "0" and nrc != "0"
d["beacon_conformance"] = {
    "facts": "SYNTHETIC (no canonical beacon-facts RPC exists)", "policy": "reference_policy_v1 with caller numbers — UNAPPROVED", "policy_args": pol, "scope_args": scope,
    "statement_root_prefix": stmt, "waiting_exit": int(wrc), "run_exit": int(rrc), "verify_exit": int(vrc), "verify_no_rerun_exit": int(nrc),
    "label": "SYNTHETIC_BEACON_CONFORMANCE_PASS" if passed else "NOT_PASSED",
    "evidence": ev or None, "commit": t("commit.out"), "run": t("run.out"), "run_stderr_tail": t("run.err"), "verify": t("verify.out"), "verify_no_rerun": t("verify-norerun.out"),
    "max_rss_bytes": {"commit": rss("commit.err"), "run": rss("run.err"), "verify": rss("verify.err")},
    "status": t("status.out")}
json.dump(d, open(pv, "w"), indent=1)
print(json.dumps({"label": d["beacon_conformance"]["label"], "waiting": wrc, "run": rrc, "verify": vrc, "no_rerun": nrc}))
PY
    [ "$rrc" = 0 ] && [ "$vrc" = 0 ] || { fail conformance CONFORMANCE_FAILED "RUN_${rrc}_VERIFY_${vrc}" "pack run-conformance / verify-conformance (SYNTHETIC facts)" "evidence PASSED and reproduced" "$C/run.err"; return 1; }
}


nodes_arg() { local s="" n; for n in "$@"; do s="$s${s:+,}$n=$(jport "$n")"; done; echo "$s"; }
check_set() { python3 "$H1/report.py" merge "$EV/consensus-state.json" "checks=$(python3 -c "
import json,sys
p='$EV/consensus-state.json'; d=json.load(open(p)); c=d.get('checks') or {}; c['$1']=json.loads('$2'); print(json.dumps(c))")" >/dev/null; }

do_observe() {
    local cid root bond; cid=$(cat "$SCR/class.id"); root=$(cat "$SCR/artifact.root"); bond=$(u_bond)
    local out=$SCR/observe; mkdir -p "$out"; local A; A=$(cat "$SCR/artifact.dir")
    local rc=0; python3 "$H1/observe.py" --class "$cid" --nodes "$(nodes_arg A B C)" --out "$EV/consensus-state.json" || rc=$?
    [ "$rc" = 0 ] || fail observe REGISTRY_STATE_MISMATCH "A_B_C_DISAGREE_rc$rc" "observe.py --nodes A,B,C" "A, B and C agree on the class row, the registry and the state proof at one block" "$EV/consensus-state.json"
    [ -s "$SCR/reorg/reorg.json" ] && python3 "$H1/report.py" merge "$EV/consensus-state.json" "reorg=$SCR/reorg/reorg.json" >/dev/null
    local pin; pin=$(python3 -c "import json;print(json.load(open('$EV/consensus-state.json'))['pin'])")
    # U's own reads (no node of its own: B's RPC), and a proof of the registration against the pinned block (RFC-0009 stage D).
    ucli --output json model status "$cid" > "$out/u-status.json" 2> "$out/u-status.err" || true
    ucli --output json model registration "$cid" > "$out/u-registration.json" 2> "$out/u-registration.err" || true
    ucli --output json model readiness "$cid" > "$out/u-readiness.json" 2> "$out/u-readiness.err" || true
    local vrc=0; ucli model verify --class "$cid" --root "$root" --owner "$bond" --pin "$pin" > "$out/u-verify-B.out" 2>&1 || vrc=$?
    local vrcA=0; HOME=$UHOME "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$(borsh A)" --palw-drill-genesis-salt="$(salt)" model verify --class "$cid" --root "$root" --owner "$bond" --pin "$pin" > "$out/u-verify-A.out" 2>&1 || vrcA=$?
    python3 "$H1/report.py" merge "$EV/consensus-state.json" "user_queries={\"status\":$(python3 -c "import json;print(json.dumps(open('$out/u-status.json').read()[-3000:]))"),\"registration\":$(python3 -c "import json;print(json.dumps(open('$out/u-registration.json').read()[-3000:]))"),\"readiness\":$(python3 -c "import json;print(json.dumps(open('$out/u-readiness.json').read()[-2000:] + open('$out/u-readiness.err').read()[-800:]))"),\"verify_against_pin_via_B\":{\"exit\":$vrc,\"out\":$(python3 -c "import json;print(json.dumps(open('$out/u-verify-B.out').read()[-1500:]))")},\"verify_against_pin_via_A\":{\"exit\":$vrcA,\"out\":$(python3 -c "import json;print(json.dumps(open('$out/u-verify-A.out').read()[-1500:]))")}}" >/dev/null
    check_set "u_proves_registration_at_pin" "$([ "$vrc" = 0 ] && [ "$vrcA" = 0 ] && echo true || echo false)"
    # Restart B over its own database: the registration must come back from disk, equal.
    stop_node B; start_node B
    local t0=$SECONDS; while [ "$(tip B)" = "?" ] || [ "$(tip B)" -lt "$(tip A)" ] 2>/dev/null; do [ $((SECONDS - t0)) -lt 900 ] || break; sleep 10; done
    rc=0; python3 "$H1/observe.py" --class "$cid" --nodes "$(nodes_arg A B)" --out "$out/after-restart-B.json" || rc=$?
    check_set "restart_B_agrees" "$([ "$rc" = 0 ] && echo true || echo false)"
    [ "$rc" = 0 ] || fail observe REGISTRY_STATE_MISMATCH RESTART_REPLAY "restart B, observe A,B" "B equal to A after a restart" "$out/after-restart-B.json"
    # A fresh node joins by IBD (empty app dir) and must hold the same class state.
    stop_node Z || true; rm -rf "$WORK_DIR/Z"
    rc=0; WORK_DIR=$WORK_DIR bash "$H1/devnet.sh" fresh > "$out/fresh-Z.log" 2>&1 || rc=$?
    if [ "$rc" = 0 ]; then rc=0; python3 "$H1/observe.py" --class "$cid" --nodes "$(nodes_arg A Z)" --out "$out/fresh-Z.json" || rc=$?; fi
    check_set "fresh_Z_ibd_agrees" "$([ "$rc" = 0 ] && echo true || echo false)"
    [ "$rc" = 0 ] || fail observe REGISTRY_STATE_MISMATCH FRESH_IBD "devnet.sh fresh; observe A,Z" "a fresh node equal to A" "$out/fresh-Z.log"
    stop_node Z || true
    # A modified artifact is refused: by `pack verify` and by the registration's pack gate (nothing is paid).
    cp "$(cat "$A/DECLARED")" "$out/modified.palwtir"; python3 - "$out/modified.palwtir" <<'PY'
import os, sys
p = sys.argv[1]; n = os.path.getsize(p); off = n // 2
with open(p, "r+b") as f:
    f.seek(off); b = f.read(1); f.seek(off); f.write(bytes([b[0] ^ 0x01]))
print(f"flipped byte {off} of {n}")
PY
    local mrc=0; "$PALW_CLASS_BIN" pack verify "$A/pack" --artifact "$out/modified.palwtir" --strict --json > "$out/modified-verify.json" 2> "$out/modified-verify.err" || mrc=$?
    local grc=0; ucli palw tir-registration --key-file "$U_SEED" --artifact "$out/modified.palwtir" --bond "$bond" --out "$out/modified.obj" --pack "$A/pack" > "$out/modified-reg.out" 2>&1 || grc=$?
    check_set "modified_artifact_refused" "{\"pack_verify_exit\":$mrc,\"registration_gate_exit\":$grc,\"refused\":$([ "$mrc" != 0 ] && [ "$grc" != 0 ] && [ ! -s "$out/modified.obj" ] && echo true || echo false)}"
    rm -f "$out/modified.palwtir"
}

do_reregister() {
    # The SAME registration object again, through another carrier: the chain must not write it twice nor take the burn/exposure twice.
    local cid; cid=$(cat "$SCR/class.id"); local out=$SCR/reregister; mkdir -p "$out"; local addr; addr=$(u_addr)
    rpc C getPalwClasses '{}' | python3 -c "import json,sys; print(json.dumps([c for c in json.load(sys.stdin)['classes'] if c['classId']=='$cid']))" > "$out/row-before.json"
    local b0; b0=$(balance_of "$addr")
    ucli palw submit-object --key-file "$U_SEED" --object "$SCR/register/registration.obj" > "$out/dry.out" 2>&1 || true
    local rc=0; ucli palw submit-object --key-file "$U_SEED" --object "$SCR/register/registration.obj" --yes > "$out/submit.out" 2>&1 || rc=$?
    local d0; d0=$(tip C); while [ "$(tip C)" -lt $((d0 + ${REREG_WAIT_DAA:-3})) ] 2>/dev/null; do sleep 15; done
    rpc C getPalwClasses '{}' | python3 -c "import json,sys; print(json.dumps([c for c in json.load(sys.stdin)['classes'] if c['classId']=='$cid']))" > "$out/row-after.json"
    local b1; b1=$(balance_of "$addr")
    local txid; txid=$(grep -oE "submitted [0-9a-f]{128}" "$out/submit.out" | head -1 | cut -d" " -f2)
    [ -n "$txid" ] && rpc C getPalwModelRegistrationStatus "{\"classId\":\"\",\"objectId\":\"\",\"transactionId\":\"$txid\"}" > "$out/second-carrier-status.json" 2>&1 || true
    python3 - "$out" "$EV/registration.json" "$rc" "$b0" "$b1" <<'PY'
import json, os, sys
out, dst, rc, b0, b1 = sys.argv[1:]
t = lambda f: open(os.path.join(out, f), errors="replace").read()[-2000:] if os.path.exists(os.path.join(out, f)) else None
before, after = json.load(open(os.path.join(out, "row-before.json"))), json.load(open(os.path.join(out, "row-after.json")))
d = json.load(open(dst))
d["re_registration"] = {"dry_run": t("dry.out"), "submit_exit": int(rc), "submit": t("submit.out"), "second_carrier_status": t("second-carrier-status.json"),
                        "class_row_unchanged": before == after, "rows": {"before": before, "after": after},
                        "u_balance_sompi": {"before": int(b0), "after": int(b1), "spent": int(b0) - int(b1)}}
json.dump(d, open(dst, "w"), indent=1)
print(json.dumps({"row_unchanged": before == after, "spent": int(b0) - int(b1), "submit_exit": int(rc)}))
PY
}


# L5/L6 probe: give the Panel seats (8 distinct genesis operators, none the registrant) the declared artifact and one producer for the
# class, then watch what the shipped lifecycle does. This build's lifecycle consults seats/readiness/panel only (GAP-6): a
# KERNEL_NOT_ACTIVE / G14-incomplete class reaching Probation/Active here is a fail-closed violation to route, never an L5/L6 PASS.
LC_SEATS=${LC_SEATS:-"A B D1 D2 D3 D4 D5 D6"}
LC_PRODUCER=${LC_PRODUCER:-D6}
do_lifecycle() {
    local cid A decl; cid=$(cat "$SCR/class.id"); A=$(cat "$SCR/artifact.dir"); decl=$(cat "$A/DECLARED")
    local n f
    for n in $LC_SEATS; do
        f=$WORK_DIR/$n/extra-args; touch "$f"
        grep -qxF -- "--palw-class-artifact=$decl" "$f" || echo "--palw-class-artifact=$decl" >> "$f"
        if [ "$n" = "$LC_PRODUCER" ]; then
            grep -qxF -- "--palw-produce" "$f" || echo "--palw-produce" >> "$f"
            grep -qxF -- "--palw-producer-class=$cid" "$f" || echo "--palw-producer-class=$cid" >> "$f"
        fi
        stop_node "$n"; start_node "$n" || fail lifecycle TEST_INFRASTRUCTURE_FAILED "NODE_${n}_DID_NOT_RESTART" "devnet restart $n with the artifact" "the node up" "$WORK_DIR/$n/kaspad.out"
        sleep 20
    done
    say "lifecycle: $LC_SEATS hold $(basename "$decl"); $LC_PRODUCER produces class ${cid:0:16}…; watcher started"
    nohup python3 "$H1/lifecycle_watch.py" --class "$cid" --port "$(jport C)" --out "$SCR/lifecycle" --every "${LC_EVERY:-180}" --hours "${LC_HOURS:-14}" \
        > "$SCR/lifecycle-watch.log" 2>&1 &
    echo $! > "$SCR/lifecycle-watch.pid"
}


# Registration mined on a MINORITY branch, then a reorg onto the majority: B (U's RPC) is isolated (shifted P2P port, dials nobody), U
# registers through it, B folds it on its own branch; the majority (A, the floor producer and five seats) keeps extending; B rejoins.
# Afterwards A, B and C must agree, the class must be registered at most once and U charged once.
REORG_MAJORITY_DAA=${REORG_MAJORITY_DAA:-4}
REORG_MINORITY=${REORG_MINORITY:-"B D6"}   # B = U's RPC and a heartbeat clock; D6 its peer
do_reorg_register() {
    local out=$SCR/reorg; mkdir -p "$out"
    local m; for m in $REORG_MINORITY; do echo "$REORG_MINORITY" > "$WORK_DIR/$m/ISOLATED"; done
    for m in $REORG_MINORITY; do stop_node "$m"; done; for m in $REORG_MINORITY; do start_node "$m"; done
    say "reorg: {$REORG_MINORITY} isolated on shifted P2P ports, peering only with each other; registering through B"
    local rc=0; do_register || rc=$?
    local cid; cid=$(cat "$SCR/class.id" 2>/dev/null || true)
    local snapB snapA
    snapB=$(rpc B getPalwClasses '{}' | python3 -c "import json,sys; print(json.dumps([c for c in json.load(sys.stdin)['classes'] if c['classId']=='$cid']))")
    snapA=$(rpc A getPalwClasses '{}' | python3 -c "import json,sys; print(json.dumps([c for c in json.load(sys.stdin)['classes'] if c['classId']=='$cid']))")
    local dagB dagA; dagB=$(rpc B getBlockDagInfo '{}'); dagA=$(rpc A getBlockDagInfo '{}')
    local a0; a0=$(tip A); while [ "$(tip A)" -lt $((a0 + REORG_MAJORITY_DAA)) ] 2>/dev/null; do sleep 20; done
    local dagA2; dagA2=$(rpc A getBlockDagInfo '{}'); local dagB2; dagB2=$(rpc B getBlockDagInfo '{}')
    for m in $REORG_MINORITY; do rm -f "$WORK_DIR/$m/ISOLATED"; stop_node "$m"; start_node "$m"; done
    local t0=$SECONDS sa sb
    while :; do sa=$(rpc A getBlockDagInfo '{}' | python3 -c 'import json,sys; print(json.load(sys.stdin)["sink"])'); sb=$(rpc B getBlockDagInfo '{}' | python3 -c 'import json,sys; print(json.load(sys.stdin)["sink"])' 2>/dev/null || true)
        [ -n "$sb" ] && [ "$sa" = "$sb" ] && break; [ $((SECONDS - t0)) -lt 1800 ] || break; sleep 10; done
    local d1; d1=$(tip C); while [ "$(tip C)" -lt $((d1 + 2)) ] 2>/dev/null; do sleep 20; done   # let a merge re-fold, if it does
    local afterA afterB; afterA=$(rpc A getPalwClasses '{}' | python3 -c "import json,sys; print(json.dumps([c for c in json.load(sys.stdin)['classes'] if c['classId']=='$cid']))")
    afterB=$(rpc B getPalwClasses '{}' | python3 -c "import json,sys; print(json.dumps([c for c in json.load(sys.stdin)['classes'] if c['classId']=='$cid']))")
    local txid; txid=$(cat "$SCR/carrier.txid" 2>/dev/null || true)
    local stA; stA=$(rpc A getPalwModelRegistrationStatus "{\"classId\":\"\",\"objectId\":\"\",\"transactionId\":\"$txid\"}" 2>/dev/null || echo '{}')
    python3 - "$out/reorg.json" "$rc" "$snapB" "$snapA" "$dagB" "$dagA" "$dagA2" "$dagB2" "$afterA" "$afterB" "$stA" "$((SECONDS - t0))" <<'PY'
import json, sys
o, rc, sB, sA, dB, dA, dA2, dB2, aA, aB, stA, secs = sys.argv[1:]
J = lambda x: json.loads(x) if x else None
dag = lambda x: {k: (J(x) or {}).get(k) for k in ("virtualDaaScore", "sink", "blockCount")}
r = {"register_exit": int(rc), "while_isolated": {"B_class_row": J(sB), "A_class_row": J(sA), "B_dag": dag(dB), "A_dag": dag(dA)},
     "majority_extended_to": dag(dA2), "minority_tip_before_rejoin": dag(dB2), "after_rejoin": {"A_class_row": J(aA), "B_class_row": J(aB), "seconds_to_A_sink": int(secs)},
     "carrier_status_on_A_after": (J(stA) or {}).get("registration")}
a, b = r["after_rejoin"]["A_class_row"], r["after_rejoin"]["B_class_row"]
r["verdict"] = {"B_had_it_alone_while_isolated": bool(r["while_isolated"]["B_class_row"]) and not r["while_isolated"]["A_class_row"],
                "A_equals_B_after": a == b, "registered_once_after": len(a or []) <= 1,
                "registered_daa_moved": bool(a) and bool(r["while_isolated"]["B_class_row"]) and a[0].get("registeredDaa") != r["while_isolated"]["B_class_row"][0].get("registeredDaa")}
json.dump(r, open(o, "w"), indent=1)
print(json.dumps(r["verdict"]))
PY
}

case $stage in
    source) do_source ;;
    preflight) env_json; do_preflight ;;
    artifact) do_artifact ;;
    env) env_json ;;
    register) do_register ;;
    conformance) do_conformance ;;
    observe) do_observe ;;
    lifecycle) do_lifecycle ;;
    reorg-register) do_reorg_register ;;
    reregister) do_reregister ;;
    summary) python3 "$H1/summarize.py" "$EV" "$RUN" "$MID" ;;
    *) sed -n '2,20p' "$0"; exit 2 ;;
esac
