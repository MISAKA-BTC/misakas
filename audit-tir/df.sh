#!/usr/bin/env bash
# audit-tir/df.sh — the D-F drills' runner (RFC-0002 Phase F), ONE salted testnet-12 drill chain on this Mac:
#   D-F1  the Qwen2.5-A16 IR class end to end: registered past the IR fence → Candidate → the admission jury
#         (readiness V2) → Prefetching → Probation (10 probe claims) → ActiveLimited → Active → claims → Final,
#         and its logits equal to the legacy class's (palw-tir-equiv);
#   D-F2  its court battery: a fault planted at every commit-point kind (palw-class drill-leaves) is convicted
#         (TirShardCourtAccused, the seat that refuted it or a challenger), and honest claims go Final;
#   D-F3/D-F4 are Phase F's step scripts (scripts/misaka-palw-tir-drill-df{3,4}.sh), called with the layout.
#
#   bash audit-tir/df.sh <command> --bin-dir <release dir>      (or BIN_DIR=<dir>; KASPAD_BIN etc. still win)
#     OLD_KASPAD_BIN=<the fleet's release kaspad>  A16_ARTIFACT=<qwen2.5-1.5B A16 .palwart>
#     [WORK_DIR=~/.misaka-palw-tir-drill] [TIR_AT=20] [IR_CONTEXT=512] [IR_LOGITS_TILE=1024]
#   commands: dry | class | up | stage1 | stage2 | df1 | df2 | df3 | df4 | status | down
#
#   STAGE 1 (the arming gate, ~2-3 h from `up`): `stage1` = df4 (below the fence the IR registration dropped
#   by name by the release and skipped by the old relay at identical tips; blocks above; the old peer refused
#   past the fence) + the registration past the fence reaching Prefetching with seats proving readiness
#   (Candidate → the admission jury → Prefetching) + df3 (8 of 8 forged-output attacks refused, the producer
#   on the IR class). Its verdict is printed and written to $WORK_DIR/stage1.verdict on its own.
#   STAGE 2 (on the same chain, after the fleet rollout): `stage2` = df1 (Active → claims → Final) + df2
#   (representative commit-point kinds live, every kind offline with palw-class certify) + df4 court.
#
#   dry     preflight (binaries, flags, tools, artifacts, ports, memory, disk, another drill) and the plan with
#           every node's argv (salt redacted, a throwaway keyring stub) — nothing is created, nothing started
#   class   OFFLINE: A16 → PALW-TIR (palw-a16-to-tir, unwindowed), declare-layout at IR_CONTEXT with the
#           logits at IR_LOGITS_TILE (ADMISSIBLE required), drill-leaves, every terminal close as carried
#           under the cap (palw-class close-sizes), logits vs the legacy class (palw-tir-equiv: EQUAL);
#           writes ir-class.id and ir-artifact.root
#   up      salt (0600, never printed), keyring (the release kaspad writes it), every node (clocks first, the old
#           relay last), the signed below-the-fence IR registration for D-F4 (ir-registration.obj), the sampler
#   df1     waits for Active and the first Final after it (dfwatch.py); PASS 0 / FAIL 1 / INCOMPLETE 3
#   df2     the court battery, one commit-point kind at a time: new0 restarted with --palw-drill-tamper-leaf,
#           new4 producing honestly; PASS when every kind is convicted and new4's claims go Final
#   df3/4   Phase F's step scripts, `run`, with the layout exported
#   down    SIGINT every node (never SIGKILL)
set -euo pipefail
A=$(cd "$(dirname "$0")" && pwd)
cmd=${1:-dry}
shift || true
# --bin-dir <dir>: the release under test (before lib-df.sh derives every binary from it).
while [ $# -gt 0 ]; do
    case $1 in
        --bin-dir) BIN_DIR=$2; export BIN_DIR; shift 2 ;;
        --bin-dir=*) BIN_DIR=${1#--bin-dir=}; export BIN_DIR; shift ;;
        *) echo "unknown argument $1"; exit 2 ;;
    esac
done
. "$A/lib-df.sh"
FAILED=0
ok() { echo "  ok   $*"; }
bad() { echo "  FAIL $*"; FAILED=1; }
note() { echo "  note $*"; }

preflight() {
    local real=$1
    echo "== D-F preflight ($(date '+%F %T')): flag days $FENCE_AT/$FENCE2_AT/$FENCE3_AT, IR fence $TIR_AT, work dir $WORK_DIR, ports ${P2P_BASE}+/${BORSH_BASE}+/${JSON_BASE}+/${GRPC_BASE}+"
    local b
    for b in "$KASPAD_BIN" "$CLI_BIN" "$OLD_KASPAD_BIN"; do
        [ -n "$b" ] && [ -x "$b" ] && ok "$b ($(shasum -a 256 "$b" | cut -c1-16))" || bad "binary missing: '${b}' (KASPAD_BIN, CLI_BIN, OLD_KASPAD_BIN)"
    done
    if [ -x "${KASPAD_BIN:-/nonexistent}" ]; then
        local H; H=$("$KASPAD_BIN" --help 2>/dev/null || true)
        for f in --palw-drill-tir-at --palw-drill-fence-at --palw-drill-fence2-at --palw-drill-fence3-at --palw-drill-write-keyring \
                 --palw-drill-tamper-leaf --palw-register-class --palw-producer-class --palw-class-artifact --palw-tir-fused-kernels; do
            grep -q -- "$f" <<<"$H" && ok "kaspad lists $f" || bad "kaspad lacks $f"
        done
    fi
    if [ -x "${OLD_KASPAD_BIN:-/nonexistent}" ]; then
        local O; O=$("$OLD_KASPAD_BIN" --help 2>/dev/null || true)
        grep -q -- "--palw-drill-fence3-at" <<<"$O" && ok "the old release lists the flag days" || bad "the old release lacks --palw-drill-fence3-at"
        grep -q -- "--palw-drill-tir-at" <<<"$O" && bad "the old release lists --palw-drill-tir-at (D-F4 needs a release without the IR fence)" || ok "the old release has no IR fence (D-F4's other side)"
    fi
    if [ -x "${CLI_BIN:-/nonexistent}" ]; then
        HOME=${TMPDIR:-/tmp} "$CLI_BIN" palw tir-registration --help >/dev/null 2>&1 && ok "misaka palw tir-registration" || bad "misaka lacks palw tir-registration"
    fi
    for t in palw-class palw-a16-to-tir palw-tir-equiv; do
        [ -x "$TOOLS_BIN/$t" ] && ok "$TOOLS_BIN/$t" || bad "tool missing: $TOOLS_BIN/$t (TOOLS_BIN)"
    done
    if [ -x "$TOOLS_BIN/palw-class" ]; then
        local U; U=$("$TOOLS_BIN/palw-class" 2>&1 || true)
        for s in declare-layout drill-leaves close-sizes certify; do grep -q "palw-class $s" <<<"$U" && ok "palw-class $s" || bad "palw-class lacks $s"; done
    fi
    [ -x "$(dirname "${KASPAD_BIN:-/nonexistent/kaspad}")/redteam" ] && ok "redteam beside kaspad (D-F3)" \
        || bad "no redteam beside kaspad (D-F3: cargo build --release -p misaminer --bin redteam)"
    for step in df3 df4; do
        [ -f "$WT/scripts/misaka-palw-tir-drill-$step.sh" ] && ok "Phase F's $step script" || bad "scripts/misaka-palw-tir-drill-$step.sh missing (Phase F's)"
    done
    if [ -s "$IR_ARTIFACT" ] && [ -s "$WORK_DIR/ir-class.id" ]; then
        ok "IR class built: $IR_ARTIFACT (class $(cut -c1-16 "$WORK_DIR/ir-class.id")…)"
    else
        [ -n "$A16_ARTIFACT" ] && [ -s "$A16_ARTIFACT" ] && ok "A16 artifact $(basename "$A16_ARTIFACT") ($(du -h "$A16_ARTIFACT" | cut -f1))" \
            || bad "A16_ARTIFACT unset or missing (the Qwen2.5-1.5B A16 .palwart the IR class is converted from)"

    fi
    for n in $(all_nodes); do [ -d "$WORK_DIR/$n/app" ] && { [ "$real" = 1 ] && [ -s "$WORK_DIR/.up" ] || bad "$WORK_DIR/$n/app exists (a drill already created here — refusing to reuse)"; }; done
    local busy="" k base
    for n in $(all_nodes); do k=$(kof "$n"); for base in $P2P_BASE $BORSH_BASE $JSON_BASE $EVM_BASE $GRPC_BASE; do
        lsof -nP -iTCP:$((base + k)) -sTCP:LISTEN >/dev/null 2>&1 && busy="$busy $((base + k))"; done; done
    [ -z "$busy" ] && ok "ports free" || { [ -s "$WORK_DIR/.up" ] && note "ports in use:$busy (this drill is up)" || bad "ports in use:$busy"; }
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
}

plan() {
    cat <<EOF
== plan (one chain; the drill runs the release under test, Phase F's scripts read the same layout)
  1. up      keyring + new0..new7 (new1, new2 heartbeat clocks; new3 floor producer; new0 the IR registrant and producer;
             new0, new1..new6 hold the IR artifact: seven ready seats, t12's 5 + 2) + old (the fleet's release, keyless,
             peered to new0); the signed below-the-fence IR registration → \$WORK_DIR/ir-registration.obj
  2. df4     (Phase F) DAA < $TIR_AT: the IR registration dropped by name by the new nodes, skipped by the old one;
             crossing $TIR_AT: WrongForkId between new0 and old
  3. df1     past $TIR_AT new0 registers the class → Candidate → jury readiness → Prefetching → Probation (10 probes)
             → ActiveLimited → Active → claims → Final; logits already compared offline (class, palw-tir-equiv)
  4. df2     per commit-point kind (ir/leaves.txt): new0 lies at that leaf, the refuting seats accuse it at once
             (TirShardCourtAccused, one move past the fence), new4 produces honestly and its claims go Final
  5. df3     (Phase F) the red-team against new0's gRPC, 8 of 8 refused, no node panics
EOF
}

dry() {
    preflight 0
    plan
    echo "== per-node argv (redacted; a throwaway salt and keyring stub, nothing written under $WORK_DIR)"
    local T; T=$(mktemp -d)
    ( export SALT; SALT=$(openssl rand -hex 32)
      KR=$T/keyring; mkdir -p "$KR"
      python3 - "$KR/manifest.json" <<'PY'
import json, sys
json.dump({"seats": [{"bond_outpoint": "<bond-%d>" % i, "fee_float_outpoint": "<fee-%d>" % i} for i in range(16)],
           "heartbeat": [{"address": "<hb-address-%d>" % i} for i in range(16)], "genesis_hash": "<genesis>"}, open(sys.argv[1], "w"))
PY
      [ -s "$WORK_DIR/ir-class.id" ] || ir_class_id() { echo "<ir-class-id>"; }
      for n in $(all_nodes); do
          echo "-- $n ($(field "$n" 4)): $(node_args "$n" | sed 's/salt=[0-9a-f]*/salt=<SALT>/' \
              | grep -E -- '--palw-drill|--listen=|--rpclisten=|--palw-produce$|--palw-producer-class|--palw-register-class|--palw-class-artifact|heartbeat-miner|--appdir|--ram-scale' | tr '\n' ' ')"
      done )
    rm -rf "$T"
    echo "== DRY RUN done (preflight failures: $FAILED)"
    [ "$FAILED" = 0 ]
}

class_build() {
    [ -n "$A16_ARTIFACT" ] && [ -s "$A16_ARTIFACT" ] || die "A16_ARTIFACT unset or missing"
    mkdir -p "$IR_DIR"
    say "A16 → PALW-TIR (the mirror program, unwindowed: F7 dissects its history cones)"
    "$TOOLS_BIN/palw-a16-to-tir" --artifact "$A16_ARTIFACT" --out "$IR_LOWERED"
    say "declare-layout at $IR_CONTEXT positions, logits tile $IR_LOGITS_TILE"
    "$TOOLS_BIN/palw-class" declare-layout --network testnet-12 --max-context "$IR_CONTEXT" --logits-tile "$IR_LOGITS_TILE" \
        --model-id "$IR_MODEL_ID" --out "$IR_ARTIFACT" "$IR_LOWERED" | tee "$IR_DIR/declare.txt"
    grep -q "ADMISSIBLE" "$IR_DIR/declare.txt" || die "admission v10 refuses the declared class (see $IR_DIR/declare.txt)"
    awk '/class id/ {print $3}' "$IR_DIR/declare.txt" > "$WORK_DIR/ir-class.id"
    awk '/inventory root/ {print $3}' "$IR_DIR/declare.txt" > "$WORK_DIR/ir-artifact.root"
    "$TOOLS_BIN/palw-class" drill-leaves --network testnet-12 "$IR_ARTIFACT" > "$IR_DIR/leaves.txt"
    say "$(wc -l < "$IR_DIR/leaves.txt" | tr -d ' ') commit-point kinds for D-F2"
    say "every terminal close as carried, against the cap (palw-class close-sizes)"
    "$TOOLS_BIN/palw-class" close-sizes --network testnet-12 "$IR_ARTIFACT" | tee "$IR_DIR/close-sizes.txt"
    grep -q "every close fits" "$IR_DIR/close-sizes.txt" || die "a terminal close does not fit what the chain can carry (see $IR_DIR/close-sizes.txt)"
    say "logits against the legacy class (palw-tir-equiv, $EQUIV_PROMPTS prompts)"
    "$TOOLS_BIN/palw-tir-equiv" --network testnet-12 --artifact "$A16_ARTIFACT" --tir "$IR_ARTIFACT" \
        --prompts "$EQUIV_PROMPTS" --max-prefill "$((IR_CONTEXT - 1))" | tee "$IR_DIR/equiv.txt"
    grep -q "EQUAL" "$IR_DIR/equiv.txt" || die "the IR class's logits are not the legacy class's (see $IR_DIR/equiv.txt)"
    say "class $(cut -c1-16 "$WORK_DIR/ir-class.id")… built, admitted, logits equal"
}

up() {
    preflight 1
    [ "$FAILED" = 0 ] || die "preflight failed — not starting"
    [ -s "$WORK_DIR/ir-class.id" ] && [ -s "$IR_ARTIFACT" ] || die "no IR class yet: run \`df.sh class\` first"
    mkdir -p "$WORK_DIR" "$UHOME"; chmod 700 "$WORK_DIR"
    if [ -z "${SALT:-}" ] && [ ! -s "$WORK_DIR/SALT" ]; then ( umask 077; openssl rand -hex 32 > "$WORK_DIR/SALT" ); fi
    if [ ! -e "$KR/manifest.json" ]; then
        mkdir -p "$KR" "$WORK_DIR/keyring-app"; chmod 700 "$KR"
        "$KASPAD_BIN" --testnet --netsuffix=12 --appdir="$WORK_DIR/keyring-app" --palw-drill-genesis-salt="$(salt)" \
            "--palw-drill-fence-at=$FENCE_AT" "--palw-drill-fence2-at=$FENCE2_AT" "--palw-drill-fence3-at=$FENCE3_AT" \
            "--palw-drill-tir-at=$TIR_AT" --palw-drill-write-keyring="$KR" 2>&1 | tail -2 | sed -E "s/[0-9a-f]{64}/<64hex>/g"
        chmod 600 "$KR"/*
    fi
    manifest "'genesis', m['genesis_hash'][:16], 'params', m['consensus_params_id'][:16], 'salt_id', m['salt_id'], 'tir_at', m.get('tir_at')"
    touch "$WORK_DIR/.up"
    for n in new1 new2 new3 new0 new4 new5 new6 new7 old; do bash "$A/nodes.sh" start "$n"; sleep 3; done
    say "the signed below-the-fence IR registration (D-F4): $WORK_DIR/ir-registration.obj"
    cli new3 palw tir-registration --artifact "$IR_ARTIFACT" --bond "$(manifest "m['seats'][3]['bond_outpoint']")" \
        --key-file "$KR/bond-3.seed" --out "$WORK_DIR/ir-registration.obj" || say "writing the IR registration failed — df4 needs it"
    ( cd "$A"; WORK_DIR=$WORK_DIR DF_PORT=$(jport new3) nohup python3 dfwatch.py >> "$WORK_DIR/dfwatch.out" 2>&1 & echo $! > "$WORK_DIR/dfwatch.pid" )
    export_layout
    say "up: tip $(tip new3); sampler pid $(cat "$WORK_DIR/dfwatch.pid"); next: df.sh df4 (below $TIR_AT), then df1"
}

df1() {
    local deadline=${DF1_DEADLINE_DAA:-2000}
    ( cd "$A"; WORK_DIR=$WORK_DIR DF_PORT=$(jport new3) python3 dfwatch.py --until final --deadline-daa "$deadline" )
    local rc=$?
    case $rc in 0) echo "D-F1 PASS: $(tr '\n' ' ' < "$WORK_DIR/df-milestones.tsv")" ;; 3) echo "D-F1 INCOMPLETE"; return 3 ;; *) echo "D-F1 FAIL"; return 1 ;; esac
}

df2() {
    [ -s "$IR_DIR/leaves.txt" ] || die "no leaves: run \`df.sh class\`"
    touch "$WORK_DIR/new4/produce-ir"; bash "$A/nodes.sh" stop new4; bash "$A/nodes.sh" start new4
    local convicted=0 kinds=0 leaf call kind before after
    while read -r leaf call kind; do
        kinds=$((kinds + 1))
        before=$(python3 -c "import json; print(json.load(open('$WORK_DIR/df-state.json'))['claims']['new0']['court_fraud'])")
        say "D-F2 $kind ($call): new0 lies at leaf $leaf"
        echo "--palw-drill-tamper-leaf=$leaf" > "$WORK_DIR/new0/extra-args"
        bash "$A/nodes.sh" stop new0; bash "$A/nodes.sh" start new0
        after=$before
        for i in $(seq 1 "${DF2_WAIT_SAMPLES:-240}"); do
            sleep 30
            after=$(python3 -c "import json; print(json.load(open('$WORK_DIR/df-state.json'))['claims']['new0']['court_fraud'])")
            [ "$after" -gt "$before" ] && break
        done
        if [ "$after" -gt "$before" ]; then convicted=$((convicted + 1)); echo "  CONVICTED leaf $leaf $kind"; else echo "  NOT CONVICTED leaf $leaf $kind"; fi
    done < "$IR_DIR/leaves.txt"
    : > "$WORK_DIR/new0/extra-args"; bash "$A/nodes.sh" stop new0; bash "$A/nodes.sh" start new0
    local honest; honest=$(python3 -c "import json; print(json.load(open('$WORK_DIR/df-state.json'))['claims']['new4']['final'])")
    echo "D-F2: $convicted of $kinds commit-point kinds convicted; new4's honest Finals: $honest"
    [ "$convicted" = "$kinds" ] && [ "$honest" -gt 0 ] && { echo "D-F2 PASS"; return 0; }
    echo "D-F2 FAIL"; return 1
}

phase_f() {
    local s=$WT/scripts/misaka-palw-tir-drill-$1.sh
    [ -x "$s" ] || [ -f "$s" ] || die "$s is Phase F's and is not on this branch yet"
    export_layout
    bash "$s" "${2:-run}"
}

# One part's verdict: PASS (0), FAIL (1), INCOMPLETE (3), written beside the rest.
verdict_of() { case $1 in 0) echo PASS ;; 3) echo INCOMPLETE ;; *) echo FAIL ;; esac; }

# STAGE 1, the arming gate: df4 (below, cross) + the registration to Prefetching with ready seats + df3.
stage1() {
    local rc4=0 rcr=0 rc3=0
    phase_f df4 run || rc4=$?
    ( cd "$A"; WORK_DIR=$WORK_DIR DF_PORT=$(jport new3) python3 dfwatch.py --until prefetching --deadline-daa "${STAGE1_DEADLINE_DAA:-400}" ) || rcr=$?
    phase_f df3 run || rc3=$?
    local v=PASS
    for rc in $rc4 $rcr $rc3; do [ "$rc" = 0 ] || { [ "$rc" = 3 ] && [ "$v" = PASS ] && v=INCOMPLETE || v=FAIL; }; done
    {
        echo "STAGE 1 $v ($(date '+%F %T'), tip $(tip new3))"
        echo "  df4 (below/cross)                  $(verdict_of $rc4)"
        echo "  registration → Prefetching, ready  $(verdict_of $rcr) $(tr '\n' ' ' < "$WORK_DIR/df-milestones.tsv" 2>/dev/null)"
        echo "  df3 (8 forged outputs)             $(verdict_of $rc3)"
    } | tee "$WORK_DIR/stage1.verdict"
    [ "$v" = PASS ]
}

# STAGE 2, on the same chain after the fleet rollout: df1 (Active → Final), df2, df4 court.
stage2() {
    local rc1=0 rc2=0 rcc=0
    df1 || rc1=$?
    df2 || rc2=$?
    phase_f df4 court || rcc=$?
    local v=PASS
    for rc in $rc1 $rc2 $rcc; do [ "$rc" = 0 ] || { [ "$rc" = 3 ] && [ "$v" = PASS ] && v=INCOMPLETE || v=FAIL; }; done
    {
        echo "STAGE 2 $v ($(date '+%F %T'), tip $(tip new3))"
        echo "  df1 (Active → claims → Final)      $(verdict_of $rc1)"
        echo "  df2 (court battery)                $(verdict_of $rc2)"
        echo "  df4 court (a close past the fence) $(verdict_of $rcc)"
    } | tee "$WORK_DIR/stage2.verdict"
    [ "$v" = PASS ]
}

case $cmd in
    dry) dry ;;
    class) class_build ;;
    up) up ;;
    stage1) stage1 ;;
    stage2) stage2 ;;
    df1) df1 ;;
    df2) df2 ;;
    df3) phase_f df3 ;;
    df4) phase_f df4 ;;
    status) bash "$A/nodes.sh" status; [ -s "$WORK_DIR/df-state.json" ] && cat "$WORK_DIR/df-state.json" ;;
    down) bash "$A/nodes.sh" stop; [ -s "$WORK_DIR/dfwatch.pid" ] && kill "$(cat "$WORK_DIR/dfwatch.pid")" 2>/dev/null; rm -f "$WORK_DIR/.up" ;;
    *) sed -n '2,45p' "$0"; exit 2 ;;
esac
