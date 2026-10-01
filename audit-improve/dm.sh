#!/usr/bin/env bash
# audit-improve/dm.sh — the D-M drills' runner (RFC-0004 A13), ONE salted testnet-12 drill chain on this Mac:
#   D-M1  the whole epoch: the usage trigger → OPEN → material (a dataset, a hard case) → submission → the candidate → the
#         evaluation claims → the keys' reveal → scoring → promotion over the sign test → the rewards, and their vesting;
#   D-M2  a candidate that must not win (it breaks what the parent passes; and a winner held to one setter's half of the draw)
#         → NoChange, every bond refunded;
#   D-M3  the court battery on evaluation claims (a planted wrong output or score convicted, honest claims survive) —
#         NOT RUNNABLE until the evaluation court exists (see `plan`);
#   D-M4  rollback: by the owner within rollback_epochs (W2), and by proof — the next epoch's regression check showing the
#         predecessor beats the promoted head (W1's second epoch);
#   D-M5  the fence crossing on the shipping binary: an improvement object below the fence dropped by name (new) / skipped (old),
#         identical tips; past it the old release is refused by the fork id;
#   D-M6  copying (a candidate already entered), a candidate past t_close, the keys revealed early, the hold-out pool flooded
#         (fee DoS / grinding: the pool is bounded at 4n, a case never revealed is dropped from the draw).
#
#   bash audit-improve/dm.sh <command> --bin-dir <release dir>       (or BIN_DIR=<dir>; KASPAD_BIN etc. still win)
#     OLD_KASPAD_BIN=<the release before the fence>  [WORK_DIR=~/.misaka-palw-improve-drill]
#   commands: dry | plan | model | keys | up | status | verdicts | once | down
#
#   dry       preflight (binaries, flags, tools, model files, ports, memory, disk, another drill) and the plan with every node's
#             argv (salt redacted, a throwaway keyring) — and every object the drill builds, built offline with a throwaway key
#             against the real tools: nothing is created under $WORK_DIR, nothing is started
#   plan      the timeline in DAA and hours, the identities, the epochs
#   model     OFFLINE: the head class H (byte-identical to D-F's small class), the synthetic exact-match pools, a LoRA adapter that
#             wins, one that loses, their merges into full-weight candidate classes, the composite of the winner, and the check
#             with the integer executor that each does what it was made to do (palw-class improve eval)
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
    python3 - "$A/drill.json" "$FENCE_AT" "$FENCE2_AT" "$FENCE3_AT" "$TIR_AT" "$TIR2_AT" "$GEN_AT" "$DECODE_AT" "$IMPROVE_AT" <<'PY'
import json, sys
p = json.load(open(sys.argv[1])); f = list(map(int, sys.argv[2:]))
w = p["policy"]["windows"]; le = sum(w[k] for k in ("w_collect", "w_submit", "w_holdout", "w_eval", "court_margin")); g = w["grid"]
H = lambda daa: f"{daa / 27.7:5.1f} h"
print(f"== plan (one chain; ~133 s per DAA = 27.7 DAA/h, measured on D-F's chain)")
print(f"  fences  flag days {f[0]}/{f[1]}/{f[2]}, IR {f[3]}, IR-2 {f[4]}, generative {f[5]}, decode rules {f[6]} (if the build has the flag), improvement {f[7]}")
print(f"  epoch   grid {g}, L_e = {le} DAA = {H(le)}  (collect {w['w_collect']}, submit {w['w_submit']}, hold-out {w['w_holdout']}, eval {w['w_eval']}, court margin {w['court_margin']}; beacon {w['beacon_delay']})")
print(f"          the court margin is the claims' time to Final: a claim is licensed in a few tens of DAA and Final 120 DAA after its licence")
print(f"          (the short challenge window), ~150-170 DAA from the claim; a shorter margin leaves the last claims missing, which count for the incumbent")
print( "  lines   W1 (H's founding line, owner seat 4), W2 and L (founded by seat 7 on H): all three open at the same boundary")
print( "  who     seat 7 = the drills' own bond (no node): setter, candidates, registrant, owner of W2 and L; seat 3 a second setter and the dataset contributor;")
print( "          new4 registers and produces H; new5 and new6 evaluate; new6 also produces W (the usage of W1 once W heads it); the old relay peers new0")
t_open = g
t_fix = t_open + w["w_collect"]; t_close = t_fix + w["w_submit"]; t_draw = t_close + w["w_holdout"]; t_ev = t_draw + w["w_eval"]; t_score = t_ev + w["court_margin"]
print(f"  DAA     {f[3]:>5}  IR fence: new4 registers H;  seat 7 registers win, lose, winc (CAND_FORM=full: win/lose are full-weight classes)")
print(f"          {f[7]:>5}  improvement fence; seat 7 founds W2 and L; the three policies (opt-in); the material (a dataset, a hard case)")
print(f"          ~{f[3] + 72:>4}  H and the candidates reach Probation (D-F's small class: 71 DAA from registration, jury and readiness)")
print(f"          ~{f[3] + 72 + 160:>4}  H's first Final claim — the usage that opens an epoch (needs the grid boundary {g} to be later: {'ok' if f[3] + 72 + 160 < g else 'TOO LATE'})")
print(f"          {t_open:>5}  OPEN (epoch 1, all three lines)   {H(t_open)}")
print(f"          {t_fix:>5}  Submission: candidates, setter sets   {t_close:>5} HoldOut   {t_draw:>5} Drawing   {t_draw + w['beacon_delay']:>5} Evaluating (draw): prompts revealed")
print(f"          {t_ev:>5}  Closing: no more claims; every claim Final by ~{t_ev + 160}; the keys revealed; scoring")
print(f"          ~{t_ev + 170:>4}  epoch 1 decided (latest {t_score}) {H(t_ev + 170)}: W1 and W2 promote `win`, L is NoChange; W2's owner rolls back")
print(f"          ~{t_ev + 170 + le:>4}  the grants' first vesting unit (vest_epochs 1, unit L_e)   {H(t_ev + 170 + le)}")
nb = ((t_ev + 170) // g + 1) * g
print(f"          ~{nb + g:>4}  W1's epoch 2 (the regression check; `Previous` = H vs the promoted `win` on items W regresses) opens at the first boundary after W's")
print(f"                 own claims are Final: ~{nb + g + le - 40}  it is decided  {H(nb + g + le - 40)}: the rollback by proof")
print("  claims  3 lines x 8 items x (parent + candidates): ~56 evaluation claims in the 70-DAA window, one carrier per node per block")
PY
    cat <<'EOF'
== what blocks, and what the drills stand in for
  composite candidates  A composite class (parent + PALWTIRS adapter) cannot prove readiness: the V2 possession proof opens the
                        registered artifact root with a multiproof over an inventory, and a composite's root is
                        H(parent_class ‖ parent_root ‖ adapter_root ‖ P), which no inventory has. So no composite class can leave
                        Candidate/Prefetching, and no evaluation claim of one is ever admitted (ClassNotAdmitting). The drills run
                        their candidates as full-weight classes (the adapter merged: an ordinary IR class); `winc` (a composite) rides
                        along on line L to be prefetched by the seats and to show the gap in the registry and the nodes' logs.
  D-M3                  needs the court over evaluation claims; the seat's half (a replay that differs files nothing) is a unit test.
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
    if [ -n "$OLD_KASPAD_BIN" ] && [ -x "$OLD_KASPAD_BIN" ]; then ok "old release $OLD_KASPAD_BIN ($(shasum -a 256 "$OLD_KASPAD_BIN" | cut -c1-16))"
    else bad "OLD_KASPAD_BIN is unset or missing: D-M5 needs the release before the improvement fence (the fleet's), per the coordinator"; fi
    local H=""
    if [ -x "${KASPAD_BIN:-/nonexistent}" ]; then
        H=$(bin_help "$KASPAD_BIN")
        local f
        for f in --palw-drill-write-keyring --palw-improve-evaluate --palw-improve-artifact-dir --palw-class-artifact --palw-register-class \
                 --palw-producer-class --palw-drill-genesis-salt; do
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
    fi
    if [ -n "$OLD_KASPAD_BIN" ] && [ -x "$OLD_KASPAD_BIN" ]; then
        local O lacks; O=$(bin_help "$OLD_KASPAD_BIN")
        lacks=$(fence_args "$O" lacks | tr '\n' ' ')
        if [ -z "$(fence_args "$O" lacks)" ]; then bad "the old release lists every fence flag: it is not older than the release under test"
        else ok "the old release lacks: $lacks— D-M5's crossing is the first of them"; fi
        grep -q -- "--palw-drill-improve-at" <<<"$O" && bad "the old release lists --palw-drill-improve-at (D-M5 needs a release without the improvement fence)" \
            || ok "the old release has no improvement fence (D-M5's other side)"
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
    for t in palw-class palw-tir-fidelity; do [ -x "$TOOLS_BIN/$t" ] && ok "$TOOLS_BIN/$t" || bad "tool missing: $TOOLS_BIN/$t (TOOLS_BIN)"; done
    if [ -x "$TOOLS_BIN/palw-class" ]; then
        local U; U=$("$TOOLS_BIN/palw-class" 2>&1 || true)
        local s
        for s in "improve eval" "improve <policy" "composite" "declare-layout"; do grep -q "palw-class $s" <<<"$U" && ok "palw-class $s" || bad "palw-class lacks $s"; done
        grep -q -- "--parent <the parent's declared" <<<"$U" && ok "palw-class declare-layout --parent (a composite's class, RFC-0004 §6.3)" || bad "palw-class declare-layout has no --parent"
    fi
    [ -x "$VENV_PY" ] && "$VENV_PY" -c "import torch, transformers" 2>/dev/null && ok "torch + transformers in $VENV_PY" || note "no torch in $VENV_PY: \`dm.sh model\` cannot run (the model files may already exist)"
    [ -s "$FIXTURE/model.safetensors" ] && ok "the head's fixture: $FIXTURE" || bad "the HF fixture is missing: $FIXTURE (FIXTURE)"
    local m missing=""
    for m in head.class.palwtir win.class.palwtir lose.class.palwtir pool-a.json pool-b.json pool-reg.json ids/head.class ids/win.class ids/lose.class; do
        [ -s "$MODEL_DIR/$m" ] || missing="$missing $m"
    done
    if [ -z "$missing" ]; then ok "model files built: head $(model_id head | cut -c1-16)… win $(model_id win | cut -c1-16)… lose $(model_id lose | cut -c1-16)…"
    else bad "model files missing:$missing — run \`dm.sh model\`"; fi
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
    python3 - "$A/drill.json" "$T" "$line" <<'PY'
import json, sys, os, copy
plan = json.load(open(sys.argv[1])); T = sys.argv[2]; line = sys.argv[3]
for name, l in plan["lines"].items():
    p = copy.deepcopy(plan["policy"])
    for k, v in (l.get("policy") or {}).items():
        p.setdefault(k, {}).update(v) if isinstance(v, dict) else p.__setitem__(k, v)
    json.dump({"line": line, "sequence": 1, "policy": p}, open(f"{T}/policy-{name}.json", "w"))
PY
    local name okc=1
    for name in W1 W2 L; do
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
    local cls
    for cls in win lose; do
        if [ -s "$MODEL_DIR/$cls.class.palwtir" ]; then
            if out=$("$TOOLS_BIN/palw-class" improve candidate --network testnet-12 --drill-salt "$DM_SALT_DRY" --key-file "$T/seed" --bond "$bond" \
                     --spec "$T/cand.json" --artifact "$MODEL_DIR/$cls.class.palwtir" --out "$T/cand-$cls.obj" 2>&1); then
                [ "$(awk '/candidate class/ {print $3}' <<<"$out")" = "$(model_id "$cls")" ] && ok "candidate object over $cls names class $(model_id "$cls" | cut -c1-16)…" \
                    || bad "candidate object over $cls names another class than ids/$cls.class"
            else bad "candidate over $cls: $(tail -1 <<<"$out")"; fi
        fi
    done
    if [ -s "$MODEL_DIR/winc.palwtirs" ]; then
        if out=$("$TOOLS_BIN/palw-class" improve candidate --network testnet-12 --drill-salt "$DM_SALT_DRY" --key-file "$T/seed" --bond "$bond" \
                 --spec "$T/cand.json" --parent "$MODEL_DIR/head.class.palwtir" --section "$MODEL_DIR/winc.palwtirs" --out "$T/cand-winc.obj" 2>&1); then
            ok "composite candidate over winc names class $(awk '/candidate class/ {print $3}' <<<"$out" | cut -c1-16)…"
        else bad "composite candidate: $(tail -1 <<<"$out")"; fi
    fi
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

model() {
    [ -x "$TOOLS_BIN/palw-tir-fidelity" ] && [ -x "$TOOLS_BIN/palw-class" ] || die "palw-tir-fidelity / palw-class not found beside $TOOLS_BIN (BIN_DIR, TOOLS_BIN)"
    [ -x "$VENV_PY" ] && "$VENV_PY" -c "import torch, transformers" 2>/dev/null || die "no torch + transformers in $VENV_PY (VENV_PY)"
    [ -s "$FIXTURE/model.safetensors" ] || die "no HF fixture at $FIXTURE"
    if [ -s "$MODEL_DIR/ids/win.class" ] && [ "$FORCE" != 1 ]; then die "$MODEL_DIR already holds a model (--force rebuilds it; a rebuilt model has other class ids)"; fi
    export HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1
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

    say "winc: the winner's adapter as a composite candidate (PALWTIRS section over the head), for the prefetch and the readiness gap"
    local PID; PID=$(model_id head)
    "$PF" "$FIXTURE" --adapter "$MODEL_DIR/adapters/win" --parent-stats "$W/head-stats.json" --artifact-out "$W/winc.lowered.palwtir" > "$W/winc-fidelity.txt" 2>&1 || die "palw-tir-fidelity --adapter failed ($W/winc-fidelity.txt)"
    "$PC" composite --parent "$MODEL_DIR/head.class.palwtir" --parent-class "$PID" --out "$W/winc.rec.palwtir" "$W/winc.lowered.palwtir" > "$W/winc-composite.txt"
    "$PC" declare-layout --network testnet-12 --max-context "$HEAD_CONTEXT" --model-id "$HEAD_MODEL_ID/winc" --parent "$MODEL_DIR/head.class.palwtir" \
        --out "$MODEL_DIR/winc.class.palwtir" "$W/winc.rec.palwtir" | tee "$W/winc-declare.txt" | grep -E "class id|ADMISSIBLE|REFUSED|artifact root"
    grep -q ADMISSIBLE "$W/winc-declare.txt" || die "the composite class is not admissible"
    awk '/class id/ {print $3}' "$W/winc-declare.txt" > "$MODEL_DIR/ids/winc.class"
    awk '/artifact root/ {print $3}' "$W/winc-declare.txt" > "$MODEL_DIR/ids/winc.root"
    "$PC" composite --parent "$MODEL_DIR/head.class.palwtir" --parent-class "$PID" --section-out "$MODEL_DIR/winc.palwtirs" "$MODEL_DIR/winc.class.palwtir" > /dev/null
    cp "$MODEL_DIR/winc.palwtirs" "$MODEL_DIR/drop/winc.palwtirs"
    r=$("$PC" improve eval --parent "$MODEL_DIR/head.class.palwtir" --section "$MODEL_DIR/winc.palwtirs" --items "$MODEL_DIR/pool-a.json" --max-new 3 | eval_passes) || r="? ?"
    say "winc on pool A: $r (informational)"

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
rep = {k: open(f"{m}/ids/{k}.class").read().strip() for k in ("head", "win", "lose", "winc")}
rep["roots"] = {k: open(f"{m}/ids/{k}.root").read().strip() for k in ("head", "win", "lose", "winc")}
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

up() {
    mkdir -p "$WORK_DIR"
    WRITE_LACKS=1 preflight 1
    [ "$FAILED" = 0 ] || die "preflight failed — not starting"
    keys
    touch "$WORK_DIR/.up"; mkdir -p "$VERDICT_DIR"
    local n
    for n in new1 new2 new3 new0 new4 new5 new6 old; do bash "$A/nodes.sh" start "$n"; sleep 3; done
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
    keys) keys ;;
    up) up ;;
    status) status ;;
    verdicts) verdicts ;;
    once) export_env; export SALT; SALT=$(salt); ( cd "$A"; python3 dmdrive.py once ) ;;
    down) down ;;
    *) sed -n '2,42p' "$0"; exit 2 ;;
esac
