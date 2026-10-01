#!/usr/bin/env bash
# audit-improve/dm.sh — the D-M drills' runner (RFC-0004 A13), ONE salted testnet-12 drill chain on this Mac:
#   D-M1  the whole epoch: the usage trigger → OPEN → material (a dataset, a hard case) → submission → the candidate → the
#         evaluation claims → the keys' reveal → scoring → promotion over the sign test → the rewards, and their vesting;
#   D-M2  a candidate that must not win (it breaks what the parent passes; and a winner held to one setter's half of the draw)
#         → NoChange, every bond refunded;
#   D-M3  the court battery on evaluation claims: two executors each commit one self-consistent lie on line T (a moved step leaf,
#         a moved first id), a challenger replays every claim, locates each lie from the accused's capture and files the court
#         proof; the lies are convicted (claim voided, bond slashed) and the honest claims survive — runs once the build under
#         test carries the evaluation court (spec 17 court proofs 13-15), else the verdict stays INCOMPLETE at "not convicted";
#   D-M4  rollback: by the owner within rollback_epochs (W2), and by proof — the next epoch's regression check showing the
#         predecessor beats the promoted head (W1's second epoch);
#   D-M5  the fence crossing on the shipping binary: an improvement object below the fence dropped by name (new) / skipped (old),
#         identical tips; past it the old release is refused by the fork id;
#   D-M6  copying (a candidate already entered), a candidate past t_close, the keys revealed early, the hold-out pool flooded
#         (fee DoS / grinding: the pool is bounded at 4n, a case never revealed is dropped from the draw).
#
#   bash audit-improve/dm.sh <command> --bin-dir <release dir>       (or BIN_DIR=<dir>; KASPAD_BIN etc. still win)
#     OLD_KASPAD_BIN=<the release before the fence>  [WORK_DIR=~/.misaka-palw-improve-drill]
#   commands: dry | plan | model | composites | keys | up | status | verdicts | once | selftest | down
#
#   dry       preflight (binaries, flags, tools, model files, ports, memory, disk, another drill) and the plan with every node's
#             argv (salt redacted, a throwaway keyring) — and every object the drill builds, built offline with a throwaway key
#             against the real tools: nothing is created under $WORK_DIR, nothing is started
#   plan      the timeline in DAA and hours, the identities, the epochs
#   model     OFFLINE: the head class H (byte-identical to D-F's small class), the synthetic exact-match pools, a LoRA adapter that
#             wins, one that loses, their merges into full-weight candidate classes, the composite of the winner, and the check
#             with the integer executor that each does what it was made to do (palw-class improve eval)
#   composites  OFFLINE: the composite candidate classes (winc, losec: parent + PALWTIRS adapter section) from the adapters `model` trained, and
#             the same check with the integer executor (no training, no torch; `model` does it too)
#   keys      write the drill keyring with the shipping kaspad (up does it too)
#   up        salt (0600, never printed), keyring, every node (clocks first, the old relay last), the driver
#   status    nodes, DAA, the verdicts so far
#   verdicts  every drill's verdict file
#   down      SIGINT every node (never SIGKILL), stop the driver
set -euo pipefail
A=$(cd "$(dirname "$0")" && pwd)
cmd=${1:-dry}
shift || true
FORCE=0
while [ $# -gt 0 ]; do
    case $1 in
        --bin-dir) BIN_DIR=$2; export BIN_DIR; shift 2 ;;
        --bin-dir=*) BIN_DIR=${1#--bin-dir=}; export BIN_DIR; shift ;;
        --force) FORCE=1; shift ;;
        *) echo "unknown argument $1"; exit 2 ;;
    esac
done
. "$A/lib-dm.sh"
FAILED=0
ok() { echo "  ok   $*"; }
bad() { echo "  FAIL $*"; FAILED=1; }
note() { echo "  note $*"; }

# ---------------------------------------------------------------------------------------------------------------------
# the plan
# ---------------------------------------------------------------------------------------------------------------------
plan() {
    python3 - "$A/drill.json" "$FENCE_AT" "$FENCE2_AT" "$FENCE3_AT" "$TIR_AT" "$TIR2_AT" "$GEN_AT" "$DECODE_AT" "$IMPROVE_AT" "$CAND_FORM" <<'PY'
import json, math, sys
p = json.load(open(sys.argv[1])); f = list(map(int, sys.argv[2:10])); cand_form = sys.argv[10]
base = p["policy"]["windows"]
def form_of(line): return "full" if cand_form == "full" else p["lines"][line].get("form", "composite")
def windows(line):
    w = dict(base)
    row = p["lines"][line]
    for ov in (row.get("policy"), row.get("policy_composite") if form_of(line) == "composite" else None):
        w.update((ov or {}).get("windows") or {})
    return w
def clock(line):
    w = windows(line); o = w["grid"]; fix = o + w["w_collect"]; close = fix + w["w_submit"]; draw = close + w["w_holdout"]; ev = draw + w["w_eval"]
    return dict(w=w, open=o, fix=fix, close=close, draw=draw, ev=ev, score=ev + w["court_margin"], dec=ev + 170,
                le=sum(w[k] for k in ("w_collect", "w_submit", "w_holdout", "w_eval", "court_margin")))
H = lambda daa: f"{daa / 27.7:5.1f} h"
C = {n: clock(n) for n in p["lines"]}
print(f"== plan (one chain; ~133 s per DAA = 27.7 DAA/h, measured on D-F's chain)")
print(f"  fences  flag days {f[0]}/{f[1]}/{f[2]}, IR {f[3]}, IR-2 {f[4]}, generative {f[5]}, decode rules {f[6]} (if the build has the flag), improvement {f[7]}")
print( "  lines   " + ", ".join(f"{n} ({form_of(n)})" for n in p["lines"]) + "  (CAND_FORM=" + cand_form + ")")
print( "          W1 is H's founding line (owner seat 4); W2, L and T are founded by seat 7 on H. A composite line holds its candidates as parent + adapter section and has the long")
print( "          Submission window; the full-weight lines are the extra ones, with the short windows. Every line has its own policy, so its own grid.")
for n, c in C.items():
    w = c["w"]
    print(f"  epoch   {n}: grid {w['grid']}, L_e = {c['le']} DAA = {H(c['le'])}  (collect {w['w_collect']}, submit {w['w_submit']}, hold-out {w['w_holdout']}, eval {w['w_eval']}, court margin {w['court_margin']}; beacon {w['beacon_delay']})")
print( "          the court margin is the claims' time to Final: a claim is licensed in a few tens of DAA and Final 120 DAA after its licence")
print( "          (the short challenge window), ~150-170 DAA from the claim; a shorter margin leaves the last claims missing, which count for the incumbent")
print( "  who     seat 7 = the drills' own bond (no node): setter, candidates, registrant, owner of W2, L and T; seat 3 a second setter and the dataset contributor;")
print( "          new4 registers and produces H; new5 and new6 evaluate; new6 also produces W (the usage of W1 once W heads it); the old relay peers new0")
print( "  D-M3    on line T new5 commits one moved step leaf and new6 one moved first id (--palw-drill-tamper-eval, drill chains only); new1 (--palw-challenge) replays")
print( "          every claim, locates each lie from the shared capture dir (a stand-in for the pipeline-claim data-availability units) and files the evaluation court's proof:")
print( "          the claims void, the bonds are slashed")
first_grid = min(c["w"]["grid"] for c in C.values())
print(f"  DAA     {f[3]:>5}  IR fence: new4 registers H;  seat 7 registers the candidate classes the lines need (composites winc/losec over H; the full-weight win/lose)")
print(f"          {f[7]:>5}  improvement fence; seat 7 founds W2, L and T; the four policies (opt-in); the material (a dataset, a hard case)")
print(f"          ~{f[3] + 72:>4}  H and the full-weight classes reach Probation (D-F's small class: 71 DAA from registration, jury and readiness)")
print(f"          ~{f[3] + 72 + 160:>4}  H's first Final claim — the usage that opens an epoch (needs the first grid boundary {first_grid} to be later: {'ok' if f[3] + 72 + 160 < first_grid else 'TOO LATE'})")
events = {}
for n, c in C.items():
    for at, text in ((c["open"], "OPEN epoch 1"), (c["fix"], "Submission (candidates, setter sets)"), (c["close"], "HoldOut"),
                     (c["draw"], f"Drawing, prompts revealed at {c['draw'] + c['w']['beacon_delay']}"),
                     (c["ev"], f"Closing, every claim Final by ~{c['ev'] + 160}; keys revealed; scoring"), (c["dec"], f"epoch 1 decided (latest {c['score']})")):
        events.setdefault((at, text), []).append(n)
for (at, text), names in sorted(events.items()):
    print(f"          {at:>5}  {', '.join(sorted(names))}: {text}   {H(at)}")
for n, c in C.items():
    if form_of(n) == "composite":
        print(f"  timing  {n}: a composite candidate's possession proofs wait for its record (CandidateSubmitted, from {c['fix']}); its jury sits at its class's own audit slot of the 100-DAA period")
        print(f"          (up to ~100 DAA after the record) and Probation follows: {c['fix']} + ~110 = ~{c['fix'] + 110} against the draw at {c['draw']} — {'fits inside Submission' if c['fix'] + 110 <= c['draw'] else 'TIGHT: claims may find the class not yet admitted'}")
print( "          (the driver logs each class's admission-audit slot at registration)")
l = C["L"]; nb = (l["dec"] // l["w"]["grid"] + 1) * l["w"]["grid"]
print(f"  {nb:>9}  L's epoch 2 (H's own claims keep its usage up): D-M6's hold-out flood in its HoldOut (~{nb + l['w']['w_collect'] + l['w']['w_submit'] + 1}), {H(nb + l['w']['w_collect'] + l['w']['w_submit'])}")
w1 = C["W1"]; g1 = w1["w"]["grid"]; open2 = math.ceil((w1["dec"] + 190) / g1) * g1
dec2 = open2 + w1["w"]["w_collect"] + w1["w"]["w_submit"] + w1["w"]["w_holdout"] + w1["w"]["w_eval"] + 170
print(f"  {open2:>9}  W1's epoch 2 (the regression check; `Previous` = H vs the promoted `win` on items W regresses) opens at the first boundary after W's own claims are Final")
print(f"  ~{dec2:>8}  it is decided  {H(dec2)}: the rollback by proof — the run's end; the grants' first vesting unit is due ~{w1['dec'] + w1['le']} ({H(w1['dec'] + w1['le'])})")
n_claims = sum(8 * (1 + len(p["lines"][n]["epochs"]["1"]["candidates"])) for n in p["lines"])
print(f"  claims  8 items x (parent + candidates) per line: ~{n_claims} evaluation claims over the lines' 70-DAA windows, one carrier per node per block")
PY
    cat <<'EOF'
== what blocks, and what the drills stand in for
  composite candidates  RFC-0004's own form (CAND_FORM=composite, the default; lines W1 and T, with the long Submission window): parent + PALWTIRS adapter. A seat fetches the adapter, proves
                        possession of it as a multiproof under the chain's `adapter_root` record (spec 17 §17.7.1) and of the parent as the
                        class of its own; the chain counts it ready for the composite only with both. Needs the core lane's composite
                        readiness in the build under test; below it a composite class never leaves Candidate (ClassNotAdmitting) and the
                        drill stops there: CAND_FORM=full runs the full-weight form (the adapter merged: an ordinary IR class) on every line, with
                        the short windows. W2 and L are the extra lines, with full-weight candidates, in either form.
  D-M3                  the evaluation court: a lying executor (--palw-drill-tamper-eval) is replayed by a challenger (--palw-challenge) that
                        finds where the lie is and files the court proof; the verdict reads the claim voided and the executor's bond slashed.
EOF
}

# ---------------------------------------------------------------------------------------------------------------------
# dry
# ---------------------------------------------------------------------------------------------------------------------
preflight() {
    local real=$1
    echo "== D-M preflight ($(date '+%F %T')): flag days $FENCE_AT/$FENCE2_AT/$FENCE3_AT, IR $TIR_AT, IR-2 $TIR2_AT, gen $GEN_AT, decode $DECODE_AT, improvement $IMPROVE_AT; work dir $WORK_DIR, ports ${P2P_BASE}+/${BORSH_BASE}+/${JSON_BASE}+"
    local b
    for b in "$KASPAD_BIN" "$CLI_BIN"; do
        [ -n "$b" ] && [ -x "$b" ] && ok "$b ($(shasum -a 256 "$b" | cut -c1-16))" || bad "binary missing: '${b}' (KASPAD_BIN, CLI_BIN)"
    done
    if [ "${NO_OLD:-0}" = 1 ]; then note "NO_OLD=1: no old relay, D-M5 will not run"
    elif [ -n "$OLD_KASPAD_BIN" ] && [ -x "$OLD_KASPAD_BIN" ]; then ok "old release $OLD_KASPAD_BIN ($(shasum -a 256 "$OLD_KASPAD_BIN" | cut -c1-16))"
    else bad "OLD_KASPAD_BIN is unset or missing: D-M5 needs the release before the improvement fence (the fleet's), per the coordinator"; fi
    local H=""
    if [ -x "${KASPAD_BIN:-/nonexistent}" ]; then
        H=$(bin_help "$KASPAD_BIN")
        local f
        for f in --palw-drill-write-keyring --palw-improve-evaluate --palw-improve-artifact-dir --palw-improve-capture-dir --palw-drill-tamper-eval \
                 --palw-challenge --palw-class-artifact --palw-register-class --palw-producer-class --palw-drill-genesis-salt; do
            grep -q -- "$f" <<<"$H" && ok "kaspad lists $f" || bad "kaspad lacks $f"
        done
        local name flag at dflag; dflag=${DECODE_FLAG:-$(detect_decode_flag "$H")}
        while IFS='|' read -r name flag at; do
            [ "$flag" = "@detect" ] && flag=$dflag
            if [ -n "$flag" ] && grep -q -- "$flag" <<<"$H"; then ok "kaspad lists the $name fence flag $flag"
            elif [ "$name" = decode ]; then bad "kaspad has no drill flag for palw_fp_decode_rules (DECODE_FLAG=…): evaluation claims are refused by the header context below it"
            else bad "kaspad lacks the $name fence flag ${flag:-?}"; fi
        done < <(fence_rows)
        if grep -aqF "palw-improve-status" "$KASPAD_BIN"; then ok "kaspad writes the improvement status file (the drill's watcher reads it)"
        else bad "kaspad carries no improvement status file (no A10 node loop): not the build under test"; fi
        if [ "$CAND_FORM" = composite ]; then
            # The core lane's composite readiness (spec 17 §17.7.1) adds a parent clause to the ready-seat predicate; its reason string is in
            # the binary. A build without it never lets a composite class leave Candidate: the drill would stop at the first candidate.
            if grep -aqF "parent not ready" "$KASPAD_BIN"; then ok "kaspad carries the composite readiness's parent clause (CAND_FORM=composite)"
            else note "no composite readiness in $KASPAD_BIN (no 'parent not ready' clause): a composite class would stay Candidate — CAND_FORM=full runs the full-weight form"; fi
            if grep -aqF "chain holds no composite record" "$KASPAD_BIN"; then ok "kaspad proves a composite's possession over its adapter section (node half)"
            else bad "kaspad lacks the node half of composite readiness (the possession proof over the adapter section)"; fi
        fi
    fi
    if [ "${NO_OLD:-0}" != 1 ] && [ -n "$OLD_KASPAD_BIN" ] && [ -x "$OLD_KASPAD_BIN" ]; then
        local O lacks; O=$(bin_help "$OLD_KASPAD_BIN")
        lacks=$(fence_args "$O" lacks | tr '\n' ' ')
        if [ -z "$(fence_args "$O" lacks)" ]; then bad "the old release lists every fence flag: it is not older than the release under test"
        else ok "the old release lacks: $lacks— D-M5's crossing is the first of them"; fi
        grep -q -- "--palw-drill-improve-at" <<<"$O" && bad "the old release lists --palw-drill-improve-at (D-M5 needs a release without the improvement fence)" \
            || ok "the old release has no improvement fence (D-M5's other side)"
        # The old release is the release the fleet runs; the build under test must CONTAIN it (every fence the old release knows, the new one knows),
        # or the crossing D-M5 shows is not "the next release after it".
        local only_old; only_old=$(comm -13 <({ grep -oE -- '--palw-drill-[a-z0-9-]*-at' <<<"$H" || true; } | sort -u) <({ grep -oE -- '--palw-drill-[a-z0-9-]*-at' <<<"$O" || true; } | sort -u) | tr '\n' ' ')
        if [ -z "$only_old" ]; then ok "the build under test lists every drill fence flag the old release lists (it contains the old release's fences)"
        else bad "the old release lists drill fence flags the build under test lacks: ${only_old}— the build under test must contain the old release (merge it; D-M5's new side is the release AFTER the old one)"; fi
        if [ "$real" = 1 ] || [ -n "${WRITE_LACKS:-}" ]; then echo "$lacks" > "$WORK_DIR/old-lacks.txt" 2>/dev/null || true; fi
    fi
    if [ -x "${CLI_BIN:-/nonexistent}" ]; then
        local c
        for c in "palw tir-registration" "palw submit-object" "palw line-found"; do
            HOME=${TMPDIR:-/tmp} "$CLI_BIN" $c --help >/dev/null 2>&1 && ok "misaka $c" || bad "misaka lacks $c"
        done
        HOME=${TMPDIR:-/tmp} "$CLI_BIN" palw tir-registration --help 2>&1 | grep -q -- "--parent" && ok "misaka palw tir-registration --parent" || bad "tir-registration has no --parent"
    fi
    local t
    [ -x "$TOOLS_BIN/palw-class" ] && ok "$TOOLS_BIN/palw-class" || bad "tool missing: $TOOLS_BIN/palw-class (TOOLS_BIN)"
    # palw-tir-fidelity is the model step's (offline, before the chain): a release without it is fine once the model is built.
    if [ -x "$TOOLS_BIN/palw-tir-fidelity" ]; then ok "$TOOLS_BIN/palw-tir-fidelity"
    elif [ -s "$MODEL_DIR/ids/win.class" ]; then note "no palw-tir-fidelity in $TOOLS_BIN (the model is built already; only \`dm.sh model\` needs it)"
    else bad "tool missing: $TOOLS_BIN/palw-tir-fidelity (TOOLS_BIN) — \`dm.sh model\` needs it"; fi
    if [ -x "$TOOLS_BIN/palw-class" ]; then
        local U; U=$("$TOOLS_BIN/palw-class" 2>&1 || true)
        local s
        for s in "improve eval" "improve <policy" "composite" "declare-layout"; do grep -q "palw-class $s" <<<"$U" && ok "palw-class $s" || bad "palw-class lacks $s"; done
        grep -q -- "--parent <the parent's declared" <<<"$U" && ok "palw-class declare-layout --parent (a composite's class, RFC-0004 §6.3)" || bad "palw-class declare-layout has no --parent"
    fi
    if WORK_DIR=${TMPDIR:-/tmp}/dm-selftest-$$ python3 "$A/dmdrive.py" selftest >/dev/null 2>&1 \
       && WORK_DIR=${TMPDIR:-/tmp}/dm-selftest-$$ python3 "$A/dmdrive.py" selftest-drive >/dev/null 2>&1; then
        ok "the driver's self-tests pass (the verdict logic; the whole actor against a scripted chain: every step fires in its state)"
    else bad "the driver's self-tests fail: python3 audit-improve/dmdrive.py selftest-drive"; fi
    rm -rf "${TMPDIR:-/tmp}/dm-selftest-$$"
    [ -x "$VENV_PY" ] && "$VENV_PY" -c "import torch, transformers" 2>/dev/null && ok "torch + transformers in $VENV_PY" || note "no torch in $VENV_PY: \`dm.sh model\` cannot run (the model files may already exist)"
    [ -s "$FIXTURE/model.safetensors" ] && ok "the head's fixture: $FIXTURE" || bad "the HF fixture is missing: $FIXTURE (FIXTURE)"
    local m missing=""
    for m in head.class.palwtir win.class.palwtir lose.class.palwtir pool-a.json pool-b.json pool-reg.json ids/head.class ids/win.class ids/lose.class; do
        [ -s "$MODEL_DIR/$m" ] || missing="$missing $m"
    done
    local c
    if [ "$CAND_FORM" = composite ]; then
        for c in winc losec; do
            for m in $c.class.palwtir $c.palwtirs drop/$c.palwtirs ids/$c.class ids/$c.root; do [ -s "$MODEL_DIR/$m" ] || missing="$missing $m"; done
        done
    fi
    if [ -z "$missing" ]; then ok "model files built (CAND_FORM=$CAND_FORM): head $(model_id head | cut -c1-16)… win=$(asset_of win) $(model_id "$(asset_of win)" | cut -c1-16)… lose=$(asset_of lose) $(model_id "$(asset_of lose)" | cut -c1-16)…"
    else bad "model files missing:$missing — run \`dm.sh model\` (the composites alone: \`dm.sh composites\`)"; fi
    local n k base busy=""
    for n in $(all_nodes); do k=$(kof "$n"); for base in $P2P_BASE $BORSH_BASE $JSON_BASE $EVM_BASE; do
        lsof -nP -iTCP:$((base + k)) -sTCP:LISTEN >/dev/null 2>&1 && busy="$busy $((base + k))"; done; done
    [ -z "$busy" ] && ok "ports free" || { [ -s "$WORK_DIR/.up" ] && note "ports in use:$busy (this drill is up)" || bad "ports in use:$busy"; }
    if [ -d "$WORK_DIR/new0/app" ]; then
        if [ "$real" = 1 ] && [ -s "$WORK_DIR/.up" ]; then :; else bad "$WORK_DIR/new0/app exists (a drill already created here — refusing to reuse)"; fi
    fi
    local others; others=$( { pgrep -f -- "--palw-drill-genesis-salt" 2>/dev/null || true; } | while read -r p; do
        ps -p "$p" -o command= 2>/dev/null | grep -q -- "--appdir=$WORK_DIR/" || echo "$p"; done | wc -l | tr -d ' ')
    if [ "${others:-0}" -gt 0 ]; then
        if [ "$real" = 1 ]; then bad "another drill is running ($others salted kaspad processes outside $WORK_DIR): two drills at once are unsafe on this Mac"
        else note "another drill is running ($others salted kaspad processes): a real run refuses to start until it is done"; fi
    else ok "no other drill running"; fi
    local free; free=$( { memory_pressure 2>/dev/null || true; } | awk -F': ' '/System-wide memory free percentage/ {gsub("%","",$2); print $2}')
    if [ "${free:-0}" -ge 40 ]; then ok "memory free ${free}%"; elif [ "$real" = 1 ]; then bad "memory free ${free}% < 40%"; else note "memory free ${free}% (< 40%: a real run refuses)"; fi
    local disk; disk=$(df -g "$HOME" | awk 'NR==2 {print $4}')
    [ "${disk:-0}" -ge 25 ] && ok "disk free ${disk} GiB" || bad "disk free ${disk} GiB < 25"
    if [ -n "$H" ]; then flagset_check; fi
}

# The real binary validates the fence set (every prerequisite, every distinctness) in a throwaway directory: the keyring export
# runs validate_args and the fence moves and exits; it dials nobody and signs nothing.
flagset_check() {
    local T; T=$(mktemp -d); local fl=() f out
    while read -r f; do [ -n "$f" ] && fl+=("$f"); done < <(fence_args "$(bin_help "$KASPAD_BIN")" has)
    local s; s=$(openssl rand -hex 32)
    if out=$("$KASPAD_BIN" --testnet --netsuffix=12 --appdir="$T/app" "--palw-drill-genesis-salt=$s" "${fl[@]}" --palw-drill-write-keyring="$T/kr" 2>&1); then
        ok "the shipping kaspad accepts the fence set (${fl[*]})"
        python3 - "$T/kr/manifest.json" <<'PY' 2>/dev/null && true
import json, sys
m = json.load(open(sys.argv[1]))
print("  ok   keyring manifest names the fences:", {k: v for k, v in m.items() if k.endswith("_at") or k == "extra_at"})
PY
    else
        bad "kaspad refuses the fence set: $(echo "$out" | tail -3 | sed -E 's/[0-9a-f]{64}/<64hex>/g' | tr '\n' ' ')"
    fi
    rm -rf "$T"
}

# Every object the drill builds, offline, with a throwaway key against the real tool: the specs parse, the policies pass the chain's
# own check, the candidate objects name the right classes.
objects_check() {
    [ -x "$TOOLS_BIN/palw-class" ] && [ -s "$MODEL_DIR/head.class.palwtir" ] || { note "objects: skipped (no tool or model files)"; return; }
    local T; T=$(mktemp -d)
    ( umask 077; openssl rand -hex 32 > "$T/seed" )
    local bond line out
    bond="$(printf '%0128x' 7):1"; line=$(model_id head)
    DM_SALT_DRY=$(openssl rand -hex 32)
    python3 - "$A/drill.json" "$T" "$line" "$CAND_FORM" <<'PY'
import json, sys, os, copy
plan = json.load(open(sys.argv[1])); T = sys.argv[2]; line = sys.argv[3]; cand_form = sys.argv[4]
for name, l in plan["lines"].items():
    p = copy.deepcopy(plan["policy"])
    composite = cand_form != "full" and l.get("form", "composite") == "composite"
    for overrides in (l.get("policy"), l.get("policy_composite") if composite else None):
        for k, v in (overrides or {}).items():
            p.setdefault(k, {}).update(v) if isinstance(v, dict) else p.__setitem__(k, v)
    json.dump({"line": line, "sequence": 1, "policy": p}, open(f"{T}/policy-{name}.json", "w"))
PY
    local name okc=1
    for name in W1 W2 L T; do
        if out=$("$TOOLS_BIN/palw-class" improve policy --network testnet-12 --drill-salt "$DM_SALT_DRY" --key-file "$T/seed" --bond "$bond" \
                 --spec "$T/policy-$name.json" --out "$T/policy-$name.obj" 2>&1) && grep -q "policy check      ok" <<<"$out"; then
            ok "policy $name passes the chain's own check ($(grep 'epoch length' <<<"$out" | tr -s ' '))"
        else bad "policy $name would be refused: $(tail -2 <<<"$out" | tr '\n' ' ')"; okc=0; fi
    done
    # a setter set and a candidate per class
    "$VENV_PY" - "$MODEL_DIR" "$T" "$line" <<'PY' 2>/dev/null || true
import json, sys
m, T, line = sys.argv[1:4]
for name in ("a", "b", "reg"):
    d = json.load(open(f"{m}/pool-{name}.json"))
    json.dump({"line": line, "epoch": 1, "prompts": d["prompts"][:8], "keys": d["keys"][:8]}, open(f"{T}/set-{name}.json", "w"))
json.dump({"line": line, "epoch": 1, "declarations": {}}, open(f"{T}/cand.json", "w"))
PY
    for name in a b reg; do
        if [ -s "$T/set-$name.json" ] && out=$("$TOOLS_BIN/palw-class" improve setter-set --network testnet-12 --drill-salt "$DM_SALT_DRY" --key-file "$T/seed" \
                 --bond "$bond" --spec "$T/set-$name.json" --out "$T/set-$name.obj" 2>&1); then ok "setter set from pool $name builds ($(grep 'set id' <<<"$out" | cut -c1-40)…)"
        else bad "setter set from pool $name: $(tail -1 <<<"$out")"; fi
    done
    local cls asset
    for asset in win lose winc losec; do
        cls=$asset
        if [ -s "$MODEL_DIR/$asset.palwtirs" ]; then   # a composite candidate: parent + section
            if out=$("$TOOLS_BIN/palw-class" improve candidate --network testnet-12 --drill-salt "$DM_SALT_DRY" --key-file "$T/seed" --bond "$bond" \
                     --spec "$T/cand.json" --parent "$MODEL_DIR/head.class.palwtir" --section "$MODEL_DIR/$asset.palwtirs" --out "$T/cand-$asset.obj" 2>&1); then
                [ "$(awk '/candidate class/ {print $3}' <<<"$out")" = "$(model_id "$asset")" ] && ok "composite candidate $cls ($asset) names class $(model_id "$asset" | cut -c1-16)…" \
                    || bad "composite candidate $cls ($asset) names another class than ids/$asset.class"
            else bad "composite candidate $cls ($asset): $(tail -1 <<<"$out")"; fi
        elif [ -s "$MODEL_DIR/$asset.class.palwtir" ]; then
            if out=$("$TOOLS_BIN/palw-class" improve candidate --network testnet-12 --drill-salt "$DM_SALT_DRY" --key-file "$T/seed" --bond "$bond" \
                     --spec "$T/cand.json" --artifact "$MODEL_DIR/$asset.class.palwtir" --out "$T/cand-$asset.obj" 2>&1); then
                [ "$(awk '/candidate class/ {print $3}' <<<"$out")" = "$(model_id "$asset")" ] && ok "candidate object $cls ($asset) names class $(model_id "$asset" | cut -c1-16)…" \
                    || bad "candidate object $cls ($asset) names another class than ids/$asset.class"
            else bad "candidate $cls ($asset): $(tail -1 <<<"$out")"; fi
        else bad "no model asset for the candidate $cls ($asset): run \`dm.sh model\` (or \`dm.sh composites\`)"; fi
    done
    rm -rf "$T"
}

dry() {
    preflight 0
    plan
    objects_check
    echo "== per-node argv (redacted; a throwaway salt and keyring stub, nothing written under $WORK_DIR)"
    local T; T=$(mktemp -d)
    ( export SALT; SALT=$(openssl rand -hex 32)
      KR=$T/keyring; mkdir -p "$KR"
      python3 - "$KR/manifest.json" <<'PY'
import json, sys
json.dump({"seats": [{"bond_outpoint": "<bond-%d>" % i, "fee_float_outpoint": "<fee-%d>" % i} for i in range(16)],
           "heartbeat": [{"address": "<hb-address-%d>" % i} for i in range(16)], "genesis_hash": "<genesis>"}, open(sys.argv[1], "w"))
PY
      for n in $(all_nodes); do
          [ "$(field "$n" 4)" = old ] && [ -z "$OLD_KASPAD_BIN" ] && { echo "-- $n (old): OLD_KASPAD_BIN is unset"; continue; }
          echo "-- $n ($(field "$n" 4)): $(node_args "$n" | sed 's/salt=[0-9a-f]*/salt=<SALT>/' \
              | grep -E -- '--palw-drill|--listen=|--palw-produce$|--palw-producer-class|--palw-register-class|--palw-class-artifact|--palw-improve|heartbeat-miner|--appdir|--ram-scale|--addpeer' \
              | sed -e "s#$MODEL_DIR#\$MODEL#g" -e "s#$WORK_DIR#\$WORK#g" | tr '\n' ' ')"
      done )
    rm -rf "$T"
    echo "== DRY RUN done (preflight failures: $FAILED)"
    [ "$FAILED" = 0 ]
}

# ---------------------------------------------------------------------------------------------------------------------
# model (offline)
# ---------------------------------------------------------------------------------------------------------------------
eval_passes() {  # eval_passes <class|parent+section…> <items.json> → "passes items"
    python3 -c "import json,sys; d=json.load(sys.stdin); print(d['passes'], d['items'])"
}

# composite_candidate <name> — the LoRA adapter under $MODEL_DIR/adapters/<name> as a composite candidate class <name>c (RFC-0004 §6.3, the
# candidate's own form): lowered over the head's calibration (palw-tir-fidelity --adapter), recorded as a composite of the head, declared
# under the head's layout, its PALWTIRS section written and dropped where the seats prefetch it from (§6.7). Assets: <name>c.class.palwtir,
# <name>c.palwtirs, ids/<name>c.class and ids/<name>c.root (the composite artifact root).
composite_candidate() {
    local name=$1 c=${1}c W=$MODEL_DIR/work PC=$TOOLS_BIN/palw-class PF=$TOOLS_BIN/palw-tir-fidelity PID
    PID=$(model_id head)
    [ -d "$MODEL_DIR/adapters/$name" ] && [ -s "$W/head-stats.json" ] && [ -n "$PID" ] || die "no adapter $name, head stats or head id under $MODEL_DIR (run dm.sh model)"
    say "$c: the $name adapter as a composite candidate (a PALWTIRS section over the head)"
    "$PF" "$FIXTURE" --adapter "$MODEL_DIR/adapters/$name" --parent-stats "$W/head-stats.json" --artifact-out "$W/$c.lowered.palwtir" > "$W/$c-fidelity.txt" 2>&1 \
        || die "palw-tir-fidelity --adapter failed ($W/$c-fidelity.txt)"
    "$PC" composite --parent "$MODEL_DIR/head.class.palwtir" --parent-class "$PID" --out "$W/$c.rec.palwtir" "$W/$c.lowered.palwtir" > "$W/$c-composite.txt"
    "$PC" declare-layout --network testnet-12 --max-context "$HEAD_CONTEXT" --model-id "$HEAD_MODEL_ID/$c" --parent "$MODEL_DIR/head.class.palwtir" \
        --out "$MODEL_DIR/$c.class.palwtir" "$W/$c.rec.palwtir" | tee "$W/$c-declare.txt" | grep -E "class id|ADMISSIBLE|REFUSED|artifact root"
    grep -q ADMISSIBLE "$W/$c-declare.txt" || die "the composite class $c is not admissible"
    awk '/class id/ {print $3}' "$W/$c-declare.txt" > "$MODEL_DIR/ids/$c.class"
    awk '/artifact root/ {print $3}' "$W/$c-declare.txt" > "$MODEL_DIR/ids/$c.root"
    "$PC" composite --parent "$MODEL_DIR/head.class.palwtir" --parent-class "$PID" --section-out "$MODEL_DIR/$c.palwtirs" "$MODEL_DIR/$c.class.palwtir" > /dev/null
    mkdir -p "$MODEL_DIR/drop"; cp "$MODEL_DIR/$c.palwtirs" "$MODEL_DIR/drop/$c.palwtirs"
}

# composite_checks — the composite candidates do what they were made to do under the integer executor (the parent + the section, as a node
# serves them): the winner wins pool A (the parent fails all of it), the regressing one fails pool B (the parent passes all of it).
composite_checks() {
    local PC=$TOOLS_BIN/palw-class r
    r=$("$PC" improve eval --parent "$MODEL_DIR/head.class.palwtir" --section "$MODEL_DIR/winc.palwtirs" --items "$MODEL_DIR/pool-a.json" --max-new 3 | eval_passes)
    say "winc on pool A: $r (at least 14 of 16 wanted: the sign test needs 7 of 8 items)"
    [ "${r%% *}" -ge 14 ] || die "the composite winner does not pass pool A under the integer executor"
    r=$("$PC" improve eval --parent "$MODEL_DIR/head.class.palwtir" --section "$MODEL_DIR/losec.palwtirs" --items "$MODEL_DIR/pool-b.json" --max-new 3 | eval_passes)
    say "losec on pool B: $r (at most 2 of 16 wanted: it breaks what the parent passes)"
    [ "${r%% *}" -le 2 ] || die "the composite regressing candidate passes items of pool B"
}

# composites — (re)build the composite candidate classes from the adapters already trained (no training, no torch): what the RFC's form needs.
composites() {
    [ -x "$TOOLS_BIN/palw-tir-fidelity" ] && [ -x "$TOOLS_BIN/palw-class" ] || die "palw-tir-fidelity / palw-class not found beside $TOOLS_BIN (BIN_DIR, TOOLS_BIN)"
    [ -s "$MODEL_DIR/ids/head.class" ] || die "no model under $MODEL_DIR (run dm.sh model first)"
    local name
    for name in win lose; do
        if [ -s "$MODEL_DIR/ids/${name}c.class" ] && [ -s "$MODEL_DIR/${name}c.palwtirs" ] && [ "$FORCE" != 1 ]; then say "${name}c exists (--force rebuilds it: another class id)"; continue; fi
        composite_candidate "$name"
    done
    composite_checks
}

model() {
    [ -x "$TOOLS_BIN/palw-tir-fidelity" ] && [ -x "$TOOLS_BIN/palw-class" ] || die "palw-tir-fidelity / palw-class not found beside $TOOLS_BIN (BIN_DIR, TOOLS_BIN)"
    [ -x "$VENV_PY" ] && "$VENV_PY" -c "import torch, transformers" 2>/dev/null || die "no torch + transformers in $VENV_PY (VENV_PY)"
    [ -s "$FIXTURE/model.safetensors" ] || die "no HF fixture at $FIXTURE"
    if [ -s "$MODEL_DIR/ids/win.class" ] && [ "$FORCE" != 1 ]; then die "$MODEL_DIR already holds a model (--force rebuilds it; a rebuilt model has other class ids)"; fi
    export HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 OMP_NUM_THREADS=2 MKL_NUM_THREADS=2   # the Mac is shared: two threads
    local W=$MODEL_DIR/work PC=$TOOLS_BIN/palw-class PF=$TOOLS_BIN/palw-tir-fidelity MT="$VENV_PY $A/modeltool.py"
    rm -rf "$MODEL_DIR"; mkdir -p "$MODEL_DIR/ids" "$MODEL_DIR/drop" "$MODEL_DIR/adapters" "$W"
    say "the head: $FIXTURE lowered (D-F's recipe: default calibration, unwindowed), declared at $HEAD_CONTEXT positions"
    "$PF" "$FIXTURE" --artifact-out "$W/head.lowered.palwtir" --stats-out "$W/head-stats.json" > "$W/head-fidelity.txt" 2>&1 || die "palw-tir-fidelity failed (see $W/head-fidelity.txt)"
    "$PC" declare-layout --network testnet-12 --max-context "$HEAD_CONTEXT" --model-id "$HEAD_MODEL_ID" --out "$MODEL_DIR/head.class.palwtir" "$W/head.lowered.palwtir" | tee "$W/head-declare.txt"
    grep -q ADMISSIBLE "$W/head-declare.txt" || die "the head class is not admissible"
    awk '/class id/ {print $3}' "$W/head-declare.txt" > "$MODEL_DIR/ids/head.class"
    awk '/inventory root/ {print $3}' "$W/head-declare.txt" > "$MODEL_DIR/ids/head.root"
    local dfsmall=$HOME/.misaka-palw-tir-drill/ir/small.class.palwtir
    if [ -s "$dfsmall" ]; then
        if cmp -s "$dfsmall" "$MODEL_DIR/head.class.palwtir"; then say "the head is byte-identical to D-F's small class ($(model_id head | cut -c1-16)…)"
        else say "note: the head differs from D-F's small class file (class $(model_id head | cut -c1-16)…): another build of the lowering"; fi
    fi
    say "the synthetic pools: A (16 prompts the parent fails), B (16 prompts the parent passes by construction)"
    $MT items --out "$MODEL_DIR/pool-a.json" --n 16 --seed 11 --key-len 3 --key-alphabet 8 >/dev/null
    $MT items --out "$W/pool-b-raw.json" --n 16 --seed 23 --key-len 3 --key-alphabet 8 >/dev/null
    "$PC" improve eval --artifact "$MODEL_DIR/head.class.palwtir" --items "$W/pool-b-raw.json" --max-new 3 > "$W/eval-head-braw.json"
    $MT keys-from-eval --eval "$W/eval-head-braw.json" --out "$MODEL_DIR/pool-b.json" >/dev/null
    local r
    r=$("$PC" improve eval --artifact "$MODEL_DIR/head.class.palwtir" --items "$MODEL_DIR/pool-a.json" --max-new 3 | eval_passes); say "the head on pool A: $r (must be 0 of 16)"
    [ "${r%% *}" = 0 ] || die "the head passes items of pool A"
    r=$("$PC" improve eval --artifact "$MODEL_DIR/head.class.palwtir" --items "$MODEL_DIR/pool-b.json" --max-new 3 | eval_passes); say "the head on pool B: $r (must be 16 of 16)"
    [ "${r%% *}" = 16 ] || die "the head fails items of pool B"

    cand_full() {  # cand_full <name> <mode> <pool> — train, merge, lower, declare: a full-weight candidate class
        local name=$1 mode=$2 items=$3
        say "$name: a LoRA adapter ($mode) trained on $(basename "$items"), merged, lowered, declared"
        $MT train --fixture "$FIXTURE" --items "$items" --mode "$mode" --out "$MODEL_DIR/adapters/$name" --rank 16 --alpha 32 --steps 6000 --lr 5e-3 --margin 5 | tail -1
        $MT merge --fixture "$FIXTURE" --adapter "$MODEL_DIR/adapters/$name" --out "$W/$name-merged" >/dev/null
        "$PF" "$W/$name-merged" --artifact-out "$W/$name.lowered.palwtir" > "$W/$name-fidelity.txt" 2>&1 || die "palw-tir-fidelity failed for $name ($W/$name-fidelity.txt)"
        tail -1 "$W/$name-fidelity.txt"
        "$PC" declare-layout --network testnet-12 --max-context "$HEAD_CONTEXT" --model-id "$HEAD_MODEL_ID/$name" --out "$MODEL_DIR/$name.class.palwtir" \
            "$W/$name.lowered.palwtir" | tee "$W/$name-declare.txt" | grep -E "class id|ADMISSIBLE|REFUSED"
        grep -q ADMISSIBLE "$W/$name-declare.txt" || die "the $name class is not admissible"
        awk '/class id/ {print $3}' "$W/$name-declare.txt" > "$MODEL_DIR/ids/$name.class"
        awk '/inventory root/ {print $3}' "$W/$name-declare.txt" > "$MODEL_DIR/ids/$name.root"
    }
    cand_full win win "$MODEL_DIR/pool-a.json"
    cand_full lose lose "$MODEL_DIR/pool-b.json"
    r=$("$PC" improve eval --artifact "$MODEL_DIR/win.class.palwtir" --items "$MODEL_DIR/pool-a.json" --max-new 3 | eval_passes); say "win on pool A: $r (must be 16 of 16)"
    [ "${r%% *}" = 16 ] || die "the winner does not pass pool A under the integer executor"
    r=$("$PC" improve eval --artifact "$MODEL_DIR/lose.class.palwtir" --items "$MODEL_DIR/pool-b.json" --max-new 3 | eval_passes); say "lose on pool B: $r (must be 0 of 16)"
    [ "${r%% *}" = 0 ] || die "the regressing candidate passes items of pool B"
    r=$("$PC" improve eval --artifact "$MODEL_DIR/lose.class.palwtir" --items "$MODEL_DIR/pool-a.json" --max-new 3 | eval_passes); say "lose on pool A: $r (informational: the parent fails all of A)"

    composite_candidate win
    composite_candidate lose
    composite_checks

    say "the regression pool (D-M4 by proof): prompts the head passes by construction and the winner fails"
    $MT items --out "$W/pool-c-raw.json" --n 64 --seed 41 --key-len 3 --key-alphabet 8 >/dev/null
    "$PC" improve eval --artifact "$MODEL_DIR/head.class.palwtir" --items "$W/pool-c-raw.json" --max-new 3 > "$W/eval-head-craw.json"
    $MT keys-from-eval --eval "$W/eval-head-craw.json" --out "$W/pool-c.json" >/dev/null
    "$PC" improve eval --artifact "$MODEL_DIR/win.class.palwtir" --items "$W/pool-c.json" --max-new 3 > "$W/eval-win-c.json"
    python3 - "$W/pool-c.json" "$W/eval-win-c.json" "$MODEL_DIR/pool-reg.json" <<'PY'
import json, sys
pool = json.load(open(sys.argv[1])); ev = json.load(open(sys.argv[2]))
fails = [r["item"] for r in sorted(ev["results"], key=lambda r: r["item"]) if not r["pass"]]
if len(fails) < 16:
    sys.exit(f"the winner fails only {len(fails)} of 64 of the head's own outputs: no regression pool of 16")
json.dump({"prompts": [pool["prompts"][i] for i in fails[:16]], "keys": [pool["keys"][i] for i in fails[:16]]}, open(sys.argv[3], "w"), indent=1)
print(f"regression pool: {len(fails)} of 64 items the head passes and the winner fails; 16 kept")
PY
    python3 - "$MODEL_DIR" <<'PY'
import json, os, sys
m = sys.argv[1]
names = [k for k in ("head", "win", "lose", "winc", "losec") if os.path.exists(f"{m}/ids/{k}.class")]
rep = {k: open(f"{m}/ids/{k}.class").read().strip() for k in names}
rep["roots"] = {k: open(f"{m}/ids/{k}.root").read().strip() for k in names}
rep["files"] = {f: os.path.getsize(f"{m}/{f}") for f in sorted(os.listdir(m)) if os.path.isfile(f"{m}/{f}")}
json.dump(rep, open(f"{m}/report.json", "w"), indent=1)
print("model report:", json.dumps({k: v[:16] for k, v in rep.items() if isinstance(v, str)}))
PY
    say "model built under $MODEL_DIR"
}

# ---------------------------------------------------------------------------------------------------------------------
# up / down / status
# ---------------------------------------------------------------------------------------------------------------------
keys() {
    mkdir -p "$WORK_DIR" "$UHOME"; chmod 700 "$WORK_DIR"
    if [ -z "${SALT:-}" ] && [ ! -s "$WORK_DIR/SALT" ]; then ( umask 077; openssl rand -hex 32 > "$WORK_DIR/SALT" ); fi
    if [ ! -e "$KR/manifest.json" ]; then
        mkdir -p "$KR" "$WORK_DIR/keyring-app"; chmod 700 "$KR"
        local fl=() f; while read -r f; do [ -n "$f" ] && fl+=("$f"); done < <(fence_args "$(bin_help "$KASPAD_BIN")" has)
        "$KASPAD_BIN" --testnet --netsuffix=12 --appdir="$WORK_DIR/keyring-app" --palw-drill-genesis-salt="$(salt)" "${fl[@]}" \
            --palw-drill-write-keyring="$KR" 2>&1 | tail -2 | sed -E "s/[0-9a-f]{64}/<64hex>/g"
        chmod 600 "$KR"/*
    fi
    manifest "'genesis', m['genesis_hash'][:16], 'params', m['consensus_params_id'][:16], 'salt_id', m['salt_id']"
}

# D-M3's per-node flags (node_args reads $WORK_DIR/<node>/extra-args): every new node shares one capture directory — the evidence transport of a
# drill on one machine, where an executor retains the capture of each claim it carries and a challenger reads the accused's — new5 and new6 each
# lie once on line T (a moved step leaf, a moved first id; T's id is derived before the chain exists, dmdrive.py line-id), new1 is the challenger.
dm3_args() {
    local tid prefix n
    tid=$(cd "$A" && export_env && python3 dmdrive.py line-id T) || die "cannot derive line T's id (is the model built, the keyring written?)"
    [[ "$tid" =~ ^[0-9a-f]{128}$ ]] || die "line T's id is not a 128-hex id: '$tid'"
    prefix=${tid:0:32}
    mkdir -p "$WORK_DIR/captures"
    for n in $(new_nodes); do
        mkdir -p "$WORK_DIR/$n"
        echo "--palw-improve-capture-dir=$WORK_DIR/captures" > "$WORK_DIR/$n/extra-args"
    done
    echo "--palw-drill-tamper-eval=leaf:1@$prefix" >> "$WORK_DIR/new5/extra-args"
    echo "--palw-drill-tamper-eval=output@$prefix" >> "$WORK_DIR/new6/extra-args"
    echo "--palw-challenge" >> "$WORK_DIR/new1/extra-args"
    say "D-M3 roles: new5 lies (leaf:1) and new6 lies (output) on line T ($prefix…), new1 challenges; captures in $WORK_DIR/captures"
}

up() {
    mkdir -p "$WORK_DIR"
    WRITE_LACKS=1 preflight 1
    [ "$FAILED" = 0 ] || die "preflight failed — not starting"
    keys
    dm3_args
    touch "$WORK_DIR/.up"; mkdir -p "$VERDICT_DIR"
    local n
    for n in new1 new2 new3 new0 new4 new5 new6 old; do [ -n "$(row "$n")" ] && { bash "$A/nodes.sh" start "$n"; sleep 3; }; done
    export_env; export SALT; SALT=$(salt)
    ( cd "$A"; nohup python3 dmdrive.py run >> "$WORK_DIR/drive.out" 2>&1 & echo $! > "$WORK_DIR/drive.pid" )
    say "up: tip $(tip new3); driver pid $(cat "$WORK_DIR/drive.pid"); next: dm.sh status / verdicts"
}

status() {
    bash "$A/nodes.sh" status
    verdicts
}

verdicts() {
    local d
    for d in dm5 dm1 dm2 dm3 dm4 dm6; do printf '%-4s %s\n' "$d" "$(cat "$VERDICT_DIR/$d.verdict" 2>/dev/null || echo 'not yet')"; done
    [ -s "$WORK_DIR/milestones.tsv" ] && { echo "-- milestones"; cat "$WORK_DIR/milestones.tsv"; }
    return 0
}

down() {
    bash "$A/nodes.sh" stop
    [ -s "$WORK_DIR/drive.pid" ] && kill "$(cat "$WORK_DIR/drive.pid")" 2>/dev/null || true
    rm -f "$WORK_DIR/.up"
}

case $cmd in
    dry) dry ;;
    plan) plan ;;
    model) model ;;
    composites) composites ;;
    keys) keys ;;
    up) up ;;
    status) status ;;
    verdicts) verdicts ;;
    once) export_env; export SALT; SALT=$(salt); ( cd "$A"; python3 dmdrive.py once ) ;;
    selftest) python3 "$A/dmdrive.py" selftest && python3 "$A/dmdrive.py" selftest-drive | tail -8 ;;
    down) down ;;
    *) sed -n '2,42p' "$0"; exit 2 ;;
esac
