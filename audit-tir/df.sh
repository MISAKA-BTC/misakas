#!/usr/bin/env bash
# audit-tir/df.sh — the D-F drills' runner (RFC-0002 Phase F), ONE salted testnet-12 drill chain on this Mac.
#
# **THE int-10 KIT (df-kit-int10, 2026-10-01) — the DAA-3,600 flag day's drill, on the SHIPPING binary.**
#   `palw_tir_fence2` ALONE crosses at TIR2_AT (default 50; `--palw-drill-tir2-at` on every new node; the model court
#   window stays dormant on testnet-12, the coordinator's decision of 2026-10-01 — nothing here arms it), over the IR
#   fence at TIR_AT (20) and the post-launch flag days (6/10/14). The old relay is the fleet's int-8 release
#   (OLD_KASPAD_BIN=lifecycle-run/bin/4ca695b98/kaspad): it knows the IR fence, not the new flag day, so it follows the
#   new nodes exactly to TIR2_AT − 1 and the fork id refuses it at TIR2_AT.
#   The A16 class (new0, D-F1) registers BELOW the flag day (the release's sizing); the small class (new4, D-F2 and the
#   B/D/C piece) registers PAST it — `df.sh register-past` — admitted under fence2's sizing (admission v10's range
#   twin) by every node. Commands added: `flagday [step]`, `register-past` (PAST_CLASS=small|ir picks the class that
#   registers past the flag day).
#   STAGE 1 = the ROLLOUT GATE, run on the final binary (`df.sh stage1`): below / cross of fence2, the old release
#   refused by the fork id at the fence, the small class registered past the fence and admitted, and df3 (the red-team's
#   8 forged outputs). STAGE 2 = after the rollout, before Phase H's phaseh2 (`df.sh stage2`): `bdc` (the DA ladder's
#   evidence transport on the small class), D-F1 (the A16 class: Active → claims → Final) and D-F2 (the court battery).
#
# (the D-F kit as it was follows)
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
#   commands: dry | class | small | up | restart-old | register-past | flagday [old|below|cross|classes] | stage1 | stage2 | df1 | df2 | df3 | df4 | bdc | status | down
#   [WORK_DIR=~/.misaka-palw-int10-drill] [TIR2_AT=50]  (the DAA-3,600 flag day's drill height; fixed for the chain's life)
#
#   THE B/D/C PIECE (RFC-0002's evidence transport, docs/design/palw/tir/evidence-transport-scope.md), on its
#   own chain: `DF1=0 TIR2_AT=30 df.sh up` (palw_tir_fence2 armed at `up` — a stored chain keeps its height;
#   DF1=0: no node loads the 1.5B class, the seven holders hold the small class alone), then `df.sh bdc` once
#   the small class is Active. new4 answers only (--palw-drill-answer-only: its answer envelope, never its
#   capture — no seat holds it, the 1.5B class's condition on a class small enough to drill live), and lies:
#     B   at an undissected leaf: the seats pursue it through new4's served annexes → convicted (CourtFraud)
#     D   at the dissected kind: the named leaf opens F7's dissection, the challenger's bottom built from new4's
#         root claim ON CHAIN → convicted
#     C   B's lie, no annex served (--palw-drill-refuse-leaf-evidence): the seats demand on chain (the root,
#         the first frontier node their own tree disputes — ten levels a session — then the leaf), new4's node
#         answers each, the seats read the disclosures back and convict
#     C0  C, and new4 stopped once a demand is on chain: its claim defaults (ProducerWithholding)
#   Verdict in $WORK_DIR/bdc.verdict. BDC_LEAF / BDC_DISSECTED_LEAF override the leaves (small-leaves.txt).
#
#   STAGE 1 (the rollout gate, ~2-3 h from `up`): `stage1` = flagday old / below (the old relay is int-8 and agrees with the new
#   binary below the fence: one sink at one DAA) + register-past (the small class's registrant restarted past the fence)
#   + flagday cross (the fork id refuses the old release at the fence; it stops) + flagday classes (the small class is
#   admitted by every node under fence2's sizing, the A16 class below it the twin; no court-window line anywhere) + df3
#   (8 of 8 forged-output attacks refused, run past the fence on the final rules). Its verdict is printed and written to
#   $WORK_DIR/stage1.verdict on its own.
#   STAGE 2 (the same chain and the same release, after the fleet rollout and before Phase H's phaseh2): `stage2` = bdc (B/D/C
#   on the small class once it is Active: the DA ladder's evidence transport) + df1 (D-F1: Active → claims → Final; new0
#   produces D-F1 from Probation on) + df2 (every commit-point kind of the SMALL class live, the node playing F7's
#   dissection at its dissected kind — a 1.5B class's capture cannot reach a seat under the 16 MiB material cap, so D-F1's
#   kinds are certified offline with palw-class certify).
#
#   dry     preflight (binaries, flags, tools, artifacts, ports, memory, disk, another drill) and the plan with
#           every node's argv (salt redacted, a throwaway keyring stub) — nothing is created, nothing started
#   class   OFFLINE: A16 → PALW-TIR (palw-a16-to-tir, unwindowed), declare-layout at IR_CONTEXT with the
#           logits at IR_LOGITS_TILE (ADMISSIBLE required), drill-leaves, every terminal close as carried
#           under the cap (palw-class close-sizes), logits vs the legacy class (palw-tir-equiv: EQUAL);
#           writes ir-class.id and ir-artifact.root; then the small class (D-F2's live battery): the tiny HF llama
#           fixture lowered unwindowed (palw-tir-fidelity), declared at SMALL_CONTEXT (ADMISSIBLE), its drill leaves
#           and closes, its capture under the 16 MiB material cap; writes small-class.id and small-artifact.root
#   up      salt (0600, never printed), keyring (the release kaspad writes it), every node (clocks first, the old
#           relay last), the signed below-the-fence IR registration for D-F4 (ir-registration.obj), the sampler
#   df1     waits for Active and the first Final after it (dfwatch.py); PASS 0 / FAIL 1 / INCOMPLETE 3
#   df2     the live court battery on the SMALL class (a 1.5B class's capture cannot reach a seat: 16 MiB cap),
#           one commit-point kind at a time: new4 restarted with --palw-drill-tamper-leaf; PASS when every kind is
#           convicted and new4's honest claims went Final — in one move (CourtFraud), or at the dissected kind by
#           F7: the seats' named-leaf challenge opens the dissection, and the liar, whose root claim cannot
#           finalize over its lie, defaults at the rung (CourtDefault)
#   df3/4   Phase F's step scripts, `run`, with the layout exported
#   down    SIGINT every node (never SIGKILL)
set -euo pipefail
A=$(cd "$(dirname "$0")" && pwd)
cmd=${1:-dry}
shift || true
# --bin-dir <dir>: the release under test (before lib-df.sh derives every binary from it).
STEP=""   # `df.sh flagday <old|below|cross|classes|run>`: one positional step after the command
while [ $# -gt 0 ]; do
    case $1 in
        --bin-dir) BIN_DIR=$2; export BIN_DIR; shift 2 ;;
        --bin-dir=*) BIN_DIR=${1#--bin-dir=}; export BIN_DIR; shift ;;
        -*) echo "unknown argument $1"; exit 2 ;;
        *) [ -z "$STEP" ] || { echo "unknown argument $1"; exit 2; }; STEP=$1; shift ;;
    esac
done
. "$A/lib-df.sh"
FAILED=0
ok() { echo "  ok   $*"; }
bad() { echo "  FAIL $*"; FAILED=1; }
note() { echo "  note $*"; }

preflight() {
    local real=$1
    echo "== D-F preflight ($(date '+%F %T')): flag days $FENCE_AT/$FENCE2_AT/$FENCE3_AT, IR fence $TIR_AT, the DAA-3,600 flag day (palw_tir_fence2) at ${TIR2_AT:-<none>}, work dir $WORK_DIR, ports ${P2P_BASE}+/${BORSH_BASE}+/${JSON_BASE}+/${GRPC_BASE}+"
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
    if [ -n "$TIR2_AT" ] && [ -x "${KASPAD_BIN:-/nonexistent}" ]; then
        # The DAA-3,600 flag day (palw_tir_fence2): the drill flag, the answer-only and refuse-evidence drills (the B/D/C
        # piece), and C's node half.
        local H2; H2=$("$KASPAD_BIN" --help 2>/dev/null || true)
        for f in --palw-drill-tir2-at --palw-drill-answer-only --palw-drill-refuse-leaf-evidence; do
            grep -q -- "$f" <<<"$H2" && ok "kaspad lists $f" || bad "kaspad lacks $f (the second IR fence is crossed: the B/D/C piece)"
        done
        grep -aqF "RFC-0002 evidence transport C" "$KASPAD_BIN" && ok "kaspad carries C's node half (the demand and the answer)" \
            || bad "kaspad has no evidence transport C (the B/D/C piece needs tir/node past ba20ee56f)"
        [ "$TIR2_AT" -gt "$TIR_AT" ] 2>/dev/null && ok "the flag day's fence at $TIR2_AT, past palw_tir_v1 at $TIR_AT" \
            || bad "the flag day's height $TIR2_AT must be past TIR_AT=$TIR_AT (the ruleset refuses it at or below)"
        [ "$TIR2_AT" -gt "$FENCE3_AT" ] 2>/dev/null || bad "TIR2_AT=$TIR2_AT must be past the post-launch flag days ($FENCE_AT/$FENCE2_AT/$FENCE3_AT)"
    fi
    if [ -x "${KASPAD_BIN:-/nonexistent}" ]; then
        # F7's node side ships in the DAA-2,000 release: Stage 2 needs the node's own dissection play, and a build
        # carrying the interim guard (never released) would hold D-F1 and file nothing at a dissected leaf.
        if grep -aqF "this node cannot play the dissection yet" "$KASPAD_BIN"; then
            bad "kaspad carries the interim F7 guard (a pre-F7 build): the release under test must play the IR dissection"
        elif grep -aqF "filing the IR dissection's" "$KASPAD_BIN"; then
            ok "kaspad plays the IR history dissection (F7's node side)"
        else
            bad "kaspad has no F7 node side (no IR dissection moves): not the DAA-2,000 release"
        fi
    fi
    if [ -x "${OLD_KASPAD_BIN:-/nonexistent}" ]; then
        local O; O=$("$OLD_KASPAD_BIN" --help 2>/dev/null || true)
        grep -q -- "--palw-drill-fence3-at" <<<"$O" && ok "the old release lists the flag days" || bad "the old release lacks --palw-drill-fence3-at"
        grep -q -- "--palw-drill-tir-at" <<<"$O" && ok "the old release has the IR fence (int-8: it follows the new nodes through TIR_AT)" \
            || bad "the old release lacks --palw-drill-tir-at (the int-10 drill's old relay is int-8, which has the IR fence)"
        for f in --palw-drill-tir2-at --palw-drill-model-court-at; do
            grep -q -- "$f" <<<"$O" && bad "the old release lists $f: it is not the fleet's int-8 (the flag day's other side)" || true
        done
        grep -q -- "--palw-drill-tir2-at" <<<"$O" || ok "the old release has no DAA-3,600 flag day (its fork id parts at TIR2_AT)"
    fi
    if [ -x "${CLI_BIN:-/nonexistent}" ]; then
        HOME=${TMPDIR:-/tmp} "$CLI_BIN" palw tir-registration --help >/dev/null 2>&1 && ok "misaka palw tir-registration" || bad "misaka lacks palw tir-registration"
    fi
    for t in palw-class palw-a16-to-tir palw-tir-equiv palw-tir-fidelity; do
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
    if [ "$DF1" != 1 ]; then
        note "DF1=0: the 1.5B class is not loaded (the B/D/C piece's chain)"
    elif [ -s "$IR_ARTIFACT" ] && [ -s "$WORK_DIR/ir-class.id" ]; then
        ok "IR class built: $IR_ARTIFACT (class $(cut -c1-16 "$WORK_DIR/ir-class.id")…)"
    else
        [ -n "$A16_ARTIFACT" ] && [ -s "$A16_ARTIFACT" ] && ok "A16 artifact $(basename "$A16_ARTIFACT") ($(du -h "$A16_ARTIFACT" | cut -f1))" \
            || bad "A16_ARTIFACT unset or missing (the Qwen2.5-1.5B A16 .palwart the IR class is converted from)"

    fi
    if [ -s "$SMALL_ARTIFACT" ] && [ -s "$WORK_DIR/small-class.id" ]; then
        ok "small class built: $SMALL_ARTIFACT (class $(cut -c1-16 "$WORK_DIR/small-class.id")…)"
    else
        [ -s "$SMALL_FIXTURE/model.safetensors" ] && ok "the small class's fixture: $SMALL_FIXTURE" \
            || bad "the small class's HF fixture is missing: $SMALL_FIXTURE (SMALL_FIXTURE)"
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
== plan — the int-10 DAA-3,600 flag day (one chain; the drill runs the release under test on the shipping rules)
  flag days $FENCE_AT/$FENCE2_AT/$FENCE3_AT, IR fence $TIR_AT, the DAA-3,600 flag day (palw_tir_fence2 ALONE; the court window is dormant) at ${TIR2_AT:-<none>}
  1. up              keyring + new0..new7 + old (the fleet's int-8: it has the IR fence, not the flag day); new0 registers the A16
                     class (below ${TIR2_AT:-the flag day}: the release's sizing); new4, the small class's registrant, only produces
                     until step 3; new0, new1..new6 hold both IR artifacts (seven ready seats, t12's 5 + 2)
  == STAGE 1 — the ROLLOUT GATE, on the final binary — \`df.sh stage1\` runs 2 to 4 in order and writes \$WORK_DIR/stage1.verdict ==
  2. flagday old     the old relay runs the drill's flag days and the IR fence, not palw_tir_fence2; new0's banner names palw_tir_fence2
                     at ${TIR2_AT:-?} and no court window
     flagday below   new0 DAA < $((${TIR2_AT:-$TIR_AT} - 2)): new0 and the old relay report one sink at one DAA (the old release
                     validated every block the new binary built below the fence: the rules below it are unchanged)
     restart-old     the tip past ${TIR_AT} and before ${TIR2_AT:-?}: the old relay is restarted once, so its handshake stores the refusal height the new
                     nodes rejudge it at (without it, it follows the chain past the fence until a fresh handshake)
  3. register-past   the tip past ${TIR2_AT:-?}: the $PAST_CLASS class's registrant restarted with --palw-register-class — it registers PAST the flag day
     flagday cross   new0 past ${TIR2_AT:-?} + 5: 'Fork-id mismatch … crossed fence ${TIR2_AT:-?}'; the old relay stops (DAA <= ${TIR2_AT:-?} + 3)
     flagday classes the $PAST_CLASS class (registered past the fence) is listed by every running new node (getPalwClasses): one registeredDaa
                     (>= ${TIR2_AT:-?}), canonicalLeaves and artifactRoot, the artifact's — admitted under fence2's sizing (the range twin);
                     the other class (registered below) is the twin under the release's; no "PALW model court window" line on any node
  4. df3             the red-team's 8 forged outputs against new0, past the fence (the producer on the A16 class)
  == STAGE 2 — after the rollout, before Phase H's phaseh2 — \`df.sh stage2\` ==
  5. bdc             fence2's B/D/C on the small class once it is Active (B served annexes, D the root claim on chain, C the demand on
                     chain at the class's DA ladder, C0 the silent executor), with palw_tir_fence2 in force
  6. df1             the A16 class: Prefetching → Probation → ActiveLimited → Active → claims → Final
  7. df2             the court battery on the small class: new4 lies at every commit-point kind; each is convicted (CourtFraud, or at the
                     dissected kind F7's named-leaf challenge and the default at the rung); its honest claims went Final first
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
    small_class_build
}

# The small IR class for D-F2's live battery: the tiny HF llama, lowered unwindowed, declared at SMALL_CONTEXT.
small_class_build() {
    [ -s "$SMALL_FIXTURE/model.safetensors" ] || die "the small class's fixture is missing: $SMALL_FIXTURE"
    mkdir -p "$IR_DIR"
    say "the small class: $SMALL_FIXTURE lowered unwindowed (palw-tir-fidelity), declared at $SMALL_CONTEXT positions"
    "$TOOLS_BIN/palw-tir-fidelity" "$SMALL_FIXTURE" --artifact-out "$SMALL_LOWERED" > "$IR_DIR/small-fidelity.txt" 2>&1 \
        || die "palw-tir-fidelity failed (see $IR_DIR/small-fidelity.txt)"
    "$TOOLS_BIN/palw-class" declare-layout --network testnet-12 --max-context "$SMALL_CONTEXT" --model-id "$SMALL_MODEL_ID" \
        --out "$SMALL_ARTIFACT" "$SMALL_LOWERED" | tee "$IR_DIR/small-declare.txt"
    grep -q "ADMISSIBLE" "$IR_DIR/small-declare.txt" || die "admission v10 refuses the small class (see $IR_DIR/small-declare.txt)"
    awk '/class id/ {print $3}' "$IR_DIR/small-declare.txt" > "$WORK_DIR/small-class.id"
    awk '/inventory root/ {print $3}' "$IR_DIR/small-declare.txt" > "$WORK_DIR/small-artifact.root"
    "$TOOLS_BIN/palw-class" drill-leaves --network testnet-12 "$SMALL_ARTIFACT" > "$IR_DIR/small-leaves.txt"
    "$TOOLS_BIN/palw-class" close-sizes --network testnet-12 "$SMALL_ARTIFACT" > "$IR_DIR/small-close-sizes.txt" 2> "$IR_DIR/small-close-sizes.err"
    grep -q "every close fits" "$IR_DIR/small-close-sizes.txt" || die "a small-class close does not fit (see $IR_DIR/small-close-sizes.txt)"
    local bytes; bytes=$(sed -n 's/.*(\([0-9]*\) capture bytes).*/\1/p' "$IR_DIR/small-close-sizes.err" | head -1)
    [ -n "$bytes" ] && [ "$bytes" -lt $((16 << 20)) ] || die "the small class's capture ($bytes bytes) does not fit the 16 MiB material cap"
    say "small class $(cut -c1-16 "$WORK_DIR/small-class.id")… admitted: $(wc -l < "$IR_DIR/small-leaves.txt" | tr -d ' ') kinds, capture $bytes bytes, $(grep -c dissected "$IR_DIR/small-close-sizes.txt") dissected point(s)"
}

up() {
    preflight 1
    [ "$FAILED" = 0 ] || die "preflight failed — not starting"
    if [ "$DF1" = 1 ]; then
        [ -s "$WORK_DIR/ir-class.id" ] && [ -s "$IR_ARTIFACT" ] || die "no IR class yet: run \`df.sh class\` first"
    else
        [ -s "$WORK_DIR/small-class.id" ] && [ -s "$SMALL_ARTIFACT" ] || die "DF1=0 needs the small class: run \`df.sh small\` first"
    fi
    mkdir -p "$WORK_DIR" "$UHOME"; chmod 700 "$WORK_DIR"
    if [ -z "${SALT:-}" ] && [ ! -s "$WORK_DIR/SALT" ]; then ( umask 077; openssl rand -hex 32 > "$WORK_DIR/SALT" ); fi
    if [ ! -e "$KR/manifest.json" ]; then
        mkdir -p "$KR" "$WORK_DIR/keyring-app"; chmod 700 "$KR"
        "$KASPAD_BIN" --testnet --netsuffix=12 --appdir="$WORK_DIR/keyring-app" --palw-drill-genesis-salt="$(salt)" \
            "--palw-drill-fence-at=$FENCE_AT" "--palw-drill-fence2-at=$FENCE2_AT" "--palw-drill-fence3-at=$FENCE3_AT" \
            "--palw-drill-tir-at=$TIR_AT" ${TIR2_AT:+"--palw-drill-tir2-at=$TIR2_AT"} ${VERTEX_AT:+"--palw-drill-vertex-at=$VERTEX_AT"} \
            --palw-drill-write-keyring="$KR" 2>&1 \
            | tail -2 | sed -E "s/[0-9a-f]{64}/<64hex>/g"
        chmod 600 "$KR"/*
    fi
    manifest "'genesis', m['genesis_hash'][:16], 'params', m['consensus_params_id'][:16], 'salt_id', m['salt_id'], 'tir_at', m.get('tir_at'), 'tir2_at', m.get('tir2_at')"
    touch "$WORK_DIR/.up"
    # The B/D/C piece's chain (DF1=0) runs no old relay: D-F4 is Stage 1's.
    local order="new1 new2 new3 new0 new4 new5 new6 new7 old"; [ "$DF1" = 1 ] || order="new1 new2 new3 new0 new4 new5 new6 new7"
    for n in $order; do bash "$A/nodes.sh" start "$n"; sleep 3; done
    if [ "$DF1" = 1 ]; then
        say "the signed below-the-fence IR registration (D-F4): $WORK_DIR/ir-registration.obj"
        cli new3 palw tir-registration --artifact "$IR_ARTIFACT" --bond "$(manifest "m['seats'][3]['bond_outpoint']")" \
            --key-file "$KR/bond-3.seed" --out "$WORK_DIR/ir-registration.obj" || say "writing the IR registration failed — df4 needs it"
    fi
    ( cd "$A"; WORK_DIR=$WORK_DIR DF_PORT=$(jport new3) nohup python3 dfwatch.py >> "$WORK_DIR/dfwatch.out" 2>&1 & echo $! > "$WORK_DIR/dfwatch.pid" )
    export_layout
    say "up: tip $(tip new3); sampler pid $(cat "$WORK_DIR/dfwatch.pid"); next: df.sh flagday old, flagday below (DAA < $((${TIR2_AT:-$TIR_AT} - 2))), then register-past once the tip is past ${TIR2_AT:-the flag day}"
}

df1() {
    local deadline=${DF1_DEADLINE_DAA:-2000}
    ( cd "$A"; WORK_DIR=$WORK_DIR DF_PORT=$(jport new3) python3 dfwatch.py --until final --deadline-daa "$deadline" )
    local rc=$?
    case $rc in 0) echo "D-F1 PASS: $(tr '\n' ' ' < "$WORK_DIR/df-milestones.tsv")" ;; 3) echo "D-F1 INCOMPLETE"; return 3 ;; *) echo "D-F1 FAIL"; return 1 ;; esac
}

df2() {
    [ -s "$IR_DIR/small-leaves.txt" ] || die "no small-class leaves: run \`df.sh class\`"
    # The small class Active first (new4's honest probation claims go Final on the way).
    ( cd "$A"; WORK_DIR=$WORK_DIR DF_PORT=$(jport new3) python3 dfwatch.py --until small-active --deadline-daa "${DF2_ACTIVE_DEADLINE_DAA:-3000}" ) \
        || { echo "D-F2 INCOMPLETE: the small class is not Active"; return 3; }
    local honest; honest=$(python3 -c "import json; print(json.load(open('$WORK_DIR/df-state.json'))['small']['claims']['new4']['final'])")
    local convicted=0 kinds=0 leaf call kind before after
    while read -r leaf call kind; do
        kinds=$((kinds + 1))
        before=$(python3 -c "import json; print(json.load(open('$WORK_DIR/df-state.json'))['small']['claims']['new4']['convicted'])")
        say "D-F2 $kind ($call): new4 lies at leaf $leaf of the small class"
        echo "--palw-drill-tamper-leaf=$leaf" > "$WORK_DIR/new4/extra-args"
        bash "$A/nodes.sh" stop new4; bash "$A/nodes.sh" start new4
        after=$before
        for i in $(seq 1 "${DF2_WAIT_SAMPLES:-240}"); do
            sleep 30
            after=$(python3 -c "import json; print(json.load(open('$WORK_DIR/df-state.json'))['small']['claims']['new4']['convicted'])")
            [ "$after" -gt "$before" ] && break
        done
        if [ "$after" -gt "$before" ]; then convicted=$((convicted + 1)); echo "  CONVICTED leaf $leaf $kind"; else echo "  NOT CONVICTED leaf $leaf $kind"; fi
    done < "$IR_DIR/small-leaves.txt"
    : > "$WORK_DIR/new4/extra-args"; bash "$A/nodes.sh" stop new4; bash "$A/nodes.sh" start new4
    echo "D-F2: $convicted of $kinds commit-point kinds of the small class convicted; new4's honest Finals before the battery: $honest"
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

# The int-10 flag-day step script (scripts/misaka-palw-int10-flagday.sh): old / below / cross / classes, on this layout.
flagday_script() {
    local s=$WT/scripts/misaka-palw-int10-flagday.sh
    [ -x "$s" ] || [ -f "$s" ] || die "$s is missing (the int-10 kit's)"
    [ -n "$TIR2_AT" ] || die "the flag-day drill needs TIR2_AT (the chain's \`--palw-drill-tir2-at\`; a stored chain keeps its height)"
    export_layout
    export TIR2_AT JSON_BASE RPC_PY="$A/rpc.py" CLASSROWS_PY="$A/classrows.py"
    bash "$s" "${1:-run}"
}

# restart-old: the old relay connected at DAA 1, when its next fence and the new nodes' were the same one (the drill's own flag
# day), so the new nodes stored NO refusal height for it and never rejudge that connection at TIR2_AT: on 2026-10-01 it followed the
# chain from DAA 50 to 52 and was refused only by a hand-restarted, fresh handshake (README "Known drill behaviours"; the 09-29
# D-F4 lesson, now for fence2). Restarted once the chain is past TIR_AT and before TIR2_AT, its handshake announces the next fence the
# old release expects (1,000) against the 50 the new nodes expect, they store the refusal height, and disconnect it AT the fence —
# the case of a production int-8 node that is already connected when the new release's flag day is scheduled. Does nothing, and says
# so, when the tip is already within two DAA of the flag day or past it (UNTESTED here: the 10-01 run restarted it at tip 52).
restart_old() {
    [ -n "$TIR2_AT" ] || die "no TIR2_AT: nothing to restart the old relay for"
    local now="" i
    for i in $(seq 1 "${RESTART_OLD_WAIT_SAMPLES:-720}"); do
        now=$(tip new3)
        [[ "$now" =~ ^[0-9]+$ ]] && [ "$now" -gt $((TIR_AT + 1)) ] && break
        sleep 10
    done
    [[ "$now" =~ ^[0-9]+$ ]] && [ "$now" -gt $((TIR_AT + 1)) ] || die "the chain is still at DAA ${now:-?}, not past the IR fence $TIR_AT"
    if [ "$now" -ge $((TIR2_AT - 2)) ]; then
        say "restart-old: tip $now is within two DAA of the flag day $TIR2_AT (or past it) — the old relay is NOT restarted; it is refused only at a fresh handshake"
        return 0
    fi
    say "restart-old: tip $now is between the IR fence $TIR_AT and the flag day $TIR2_AT — restarting the old relay (a fresh handshake, so the refusal height is stored)"
    bash "$A/nodes.sh" stop old; bash "$A/nodes.sh" start old
}

# register-past: the PAST_CLASS (small by default, or ir) registers PAST the flag day. Waits for the tip to pass TIR2_AT + 1,
# creates the marker lib-df.sh reads, and restarts that class's registrant (new4 for small, new0 for ir) with --palw-register-class.
register_past() {
    [ -n "$TIR2_AT" ] || die "no TIR2_AT: classes register at once at \`up\` on a chain without the flag day"
    local who=new4 cid=small_class_id
    [ "$PAST_CLASS" = ir ] && { who=new0; cid=ir_class_id; }
    [ -s "$WORK_DIR/$([ "$PAST_CLASS" = ir ] && echo ir-class.id || echo small-class.id)" ] || die "no $PAST_CLASS class (df.sh class / small)"
    [ ! -e "$PAST_REGISTER_MARKER" ] || { say "register-past: already done ($(cat "$PAST_REGISTER_MARKER"))"; return 0; }
    local now i
    for i in $(seq 1 "${REGISTER_PAST_WAIT_SAMPLES:-720}"); do
        now=$(tip new3)
        [[ "$now" =~ ^[0-9]+$ ]] && [ "$now" -gt $((TIR2_AT + 1)) ] && break
        sleep 10
    done
    [[ "$now" =~ ^[0-9]+$ ]] && [ "$now" -gt $((TIR2_AT + 1)) ] || die "the chain is still at DAA ${now:-?}, not past the flag day $TIR2_AT"
    echo "tip $now at $(date '+%F %T')" > "$PAST_REGISTER_MARKER"
    say "register-past: tip $now is past the flag day $TIR2_AT — restarting $who with --palw-register-class ($PAST_CLASS class)"
    bash "$A/nodes.sh" stop "$who"; bash "$A/nodes.sh" start "$who"
}

# STAGE 1, the rollout gate (the int-10 flag day, on the final binary): old / below, the small class past the fence,
# cross, classes (the small class admitted under fence2's sizing, the A16 class the twin), then df3 (the red-team's 8 forged
# outputs, past the fence) — one verdict, $WORK_DIR/stage1.verdict.
stage1() {
    local rco=0 rcb=0 rcr=0 rcx=0 rcc=0 rc3=0
    flagday_script old || rco=$?
    flagday_script below || rcb=$?
    restart_old || say "restart-old failed (not a verdict item): the old relay may follow the chain past the flag day"
    register_past || rcr=$?
    flagday_script cross || rcx=$?
    flagday_script classes || rcc=$?
    phase_f df3 || rc3=$?
    local v=PASS rc
    for rc in $rco $rcb $rcr $rcx $rcc $rc3; do [ "$rc" = 0 ] || { [ "$rc" = 3 ] && [ "$v" = PASS ] && v=INCOMPLETE || v=FAIL; }; done
    {
        echo "STAGE 1 $v ($(date '+%F %T'), tip $(tip new3); the DAA-3,600 flag day (palw_tir_fence2 alone) at ${TIR2_AT:-?})"
        echo "  old relay is int-8 (IR fence, no flag day)             $(verdict_of $rco)"
        echo "  below the flag day: one sink, old = new                $(verdict_of $rcb)"
        echo "  the $PAST_CLASS class registers past the flag day              $(verdict_of $rcr)"
        echo "  cross: fork-id refusal, old stops                      $(verdict_of $rcx)"
        echo "  classes: admitted past the fence (range twin), twin below  $(verdict_of $rcc)"
        echo "  df3: 8 of 8 forged outputs refused, past the fence      $(verdict_of $rc3)"
    } | tee "$WORK_DIR/stage1.verdict"
    case $v in PASS) return 0 ;; INCOMPLETE) return 3 ;; *) return 1 ;; esac
}

# STAGE 2, on the same chain after the fleet rollout and before Phase H's phaseh2: the B/D/C piece (fence2's evidence transport
# on the small class), D-F1 (A16: Active → claims → Final) and D-F2 (the court battery on the small class).
stage2() {
    local rcd=0 rc1=0 rc2=0
    bdc || rcd=$?
    df1 || rc1=$?
    df2 || rc2=$?
    local v=PASS rc
    for rc in $rcd $rc1 $rc2; do [ "$rc" = 0 ] || { [ "$rc" = 3 ] && [ "$v" = PASS ] && v=INCOMPLETE || v=FAIL; }; done
    {
        echo "STAGE 2 $v ($(date '+%F %T'), tip $(tip new3))"
        echo "  B/D/C on the small class (fence2's DA ladder)  $(verdict_of $rcd)"
        echo "  D-F1 (A16: Active → claims → Final)            $(verdict_of $rc1)"
        echo "  D-F2 (court battery, small class)              $(verdict_of $rc2)"
    } | tee "$WORK_DIR/stage2.verdict"
    [ "$v" = PASS ]
}

# ---- THE B/D/C PIECE (see the header) — one part at a time on the small class, new4 restarted per part ----

# new4's small-class claims counted by the sampler: `convicted` (a court) or `withheld` (a DA default).
bdc_count() { python3 -c "import json; print(json.load(open('$WORK_DIR/df-state.json'))['small']['claims']['new4']['$1'])"; }

# Each node's log offset at a part's start (bash 3.2: no associative arrays), and "did <node> log <text> since".
bdc_mark() { local n; for n in $(new_nodes); do echo $(( $(wc -l < "$WORK_DIR/$n/kaspad.out" 2>/dev/null || echo 0) + 1 )) > "$WORK_DIR/bdc.off.$n"; done; }
bdc_logged() { tail -n +"$(cat "$WORK_DIR/bdc.off.$1" 2>/dev/null || echo 1)" "$WORK_DIR/$1/kaspad.out" 2>/dev/null | grep -qF -- "$2"; }
bdc_seat_logged() { local n; for n in new0 new1 new2 new3 new5 new6 new7; do bdc_logged "$n" "$1" && return 0; done; return 1; }

# Wait (30 s samples, BDC_WAIT_SAMPLES) until new4's count `$1` passes `$2`; echo it.
bdc_wait() {
    local field=$1 before=$2 now=$2 i
    for i in $(seq 1 "${BDC_WAIT_SAMPLES:-240}"); do
        sleep 30
        now=$(bdc_count "$field")
        [ "$now" -gt "$before" ] && break
    done
    echo "$now"
}

# new4 restarted answering only (its envelope, never its capture), with `$@` more of its flags.
bdc_new4() { printf '%s\n' --palw-drill-answer-only "$@" > "$WORK_DIR/new4/extra-args"; bash "$A/nodes.sh" stop new4; bash "$A/nodes.sh" start new4; }

# B's and C's leaf: an undissected leaf past 1, a power of two where one is (the descent from leaf 0 takes the
# fewest rounds there).
bdc_default_leaf() {
    python3 -c '
import sys
leaves = [int(l.split()[0]) for l in open(sys.argv[1]) if l.strip()]
dissected = {int(l.split()[2]) for l in open(sys.argv[2]) if len(l.split()) > 2 and l.split()[1] == "dissected"}
ok = [l for l in leaves if l >= 2 and l not in dissected]
pow2 = [l for l in ok if l & (l - 1) == 0]
print((pow2 or ok or [0])[0])
' "$IR_DIR/small-leaves.txt" "$IR_DIR/small-close-sizes.txt"
}

bdc() {
    local FD_AT=$TIR2_AT
    [ -n "$FD_AT" ] || die "the B/D/C piece needs palw_tir_fence2: \`TIR2_AT=50 df.sh up\` or \`DF1=0 TIR2_AT=30 df.sh up\` (a stored chain keeps its height)"
    [ -s "$IR_DIR/small-leaves.txt" ] && [ -s "$IR_DIR/small-close-sizes.txt" ] || die "no small-class leaves: run \`df.sh small\`"
    ( cd "$A"; WORK_DIR=$WORK_DIR DF_PORT=$(jport new3) python3 dfwatch.py --until small-active --deadline-daa "${BDC_ACTIVE_DEADLINE_DAA:-3000}" ) \
        || { echo "B/D/C INCOMPLETE: the small class is not Active"; return 3; }
    local now; now=$(tip new3)
    [ "$now" -ge "$FD_AT" ] 2>/dev/null || die "the chain is below palw_tir_fence2 ($now < $FD_AT)"
    # The dissected kind (small-close-sizes.txt: `<n> dissected <leaf> <call> <kind>`) for D; for B and C an
    # undissected leaf, a power of two where one is (the descent from leaf 0 takes the fewest rounds there).
    local dissected leaf
    dissected=${BDC_DISSECTED_LEAF:-$(awk '$2=="dissected" {print $3; exit}' "$IR_DIR/small-close-sizes.txt")}
    leaf=${BDC_LEAF:-$(bdc_default_leaf)}
    [ -n "$dissected" ] || die "the small class has no dissected kind in small-close-sizes.txt (D needs one)"
    say "B/D/C on the small class: B/C at leaf $leaf, D at the dissected leaf $dissected, palw_tir_fence2 at $FD_AT (tip $now)"
    local rb=1 rd=1 rcc=1 rc0=1 before after why i

    # B — served annexes.
    before=$(bdc_count convicted); bdc_mark; bdc_new4 "--palw-drill-tamper-leaf=$leaf"
    after=$(bdc_wait convicted "$before")
    why=""; [ "$after" -gt "$before" ] || why="$why no conviction;"
    bdc_logged new4 "served the IR annex of leaf" || why="$why new4 served no annex;"
    bdc_seat_logged "the annexes name leaf" || why="$why no seat named the leaf from annexes;"
    [ -z "$why" ] && rb=0; echo "  B  (annexes)          $(verdict_of $rb)${why:+ —$why}"

    # D — the dissection's bottom from the root claim on chain.
    before=$(bdc_count convicted); bdc_mark; bdc_new4 "--palw-drill-tamper-leaf=$dissected"
    after=$(bdc_wait convicted "$before")
    why=""; [ "$after" -gt "$before" ] || why="$why no conviction;"
    bdc_seat_logged "RFC-0002 evidence transport D" || why="$why no seat built the bottom from the chain;"
    [ -z "$why" ] && rd=0; echo "  D  (root claim)       $(verdict_of $rd)${why:+ —$why}"

    # C — no annex served: demanded on chain, answered, read back, convicted.
    before=$(bdc_count convicted); bdc_mark; bdc_new4 "--palw-drill-tamper-leaf=$leaf" --palw-drill-refuse-leaf-evidence
    after=$(bdc_wait convicted "$before")
    why=""; [ "$after" -gt "$before" ] || why="$why no conviction;"
    bdc_seat_logged "RFC-0002 evidence transport C" || why="$why no seat demanded on chain;"
    bdc_logged new4 "TirStepNode" || why="$why new4 answered no step-node demand;"
    bdc_logged new4 "TirStepLeaf" || why="$why new4 answered no step-leaf demand;"
    bdc_seat_logged "step-node answers move the descent" || why="$why no seat descended on the chain's node answers;"
    bdc_seat_logged "is disclosed on chain" || why="$why no seat read a disclosure back;"
    [ -z "$why" ] && rcc=0; echo "  C  (demand on chain)  $(verdict_of $rcc)${why:+ —$why}"

    # C0 — the same, and new4 silent once a demand is on chain: the claim defaults.
    before=$(bdc_count withheld); bdc_mark; bdc_new4 "--palw-drill-tamper-leaf=$leaf" --palw-drill-refuse-leaf-evidence
    for i in $(seq 1 "${BDC_WAIT_SAMPLES:-240}"); do sleep 30; bdc_seat_logged "RFC-0002 evidence transport C" && break; done
    bash "$A/nodes.sh" stop new4
    after=$(bdc_wait withheld "$before")
    why=""; [ "$after" -gt "$before" ] || why="$why no claim defaulted (ProducerWithholding);"
    [ -z "$why" ] && rc0=0; echo "  C0 (silent executor)  $(verdict_of $rc0)${why:+ —$why}"

    : > "$WORK_DIR/new4/extra-args"; bash "$A/nodes.sh" start new4
    local v=PASS r
    for r in $rb $rd $rcc $rc0; do [ "$r" = 0 ] || v=FAIL; done
    {
        echo "B/D/C $v ($(date '+%F %T'), tip $(tip new3); leaf $leaf, dissected $dissected, palw_tir_fence2 $FD_AT)"
        echo "  B  $(verdict_of $rb)   D  $(verdict_of $rd)   C  $(verdict_of $rcc)   C0  $(verdict_of $rc0)"
    } | tee "$WORK_DIR/bdc.verdict"
    [ "$v" = PASS ]
}

case $cmd in
    dry) dry ;;
    class) class_build ;;
    small) small_class_build ;;
    up) up ;;
    stage1) stage1 ;;
    stage2) stage2 ;;
    flagday) flagday_script "${STEP:-run}" ;;
    register-past|register-small) register_past ;;
    restart-old) restart_old ;;
    df1) df1 ;;
    df2) df2 ;;
    df3) phase_f df3 ;;
    df4) phase_f df4 ;;
    bdc) bdc ;;
    status) bash "$A/nodes.sh" status; [ -s "$WORK_DIR/df-state.json" ] && cat "$WORK_DIR/df-state.json" ;;
    down) bash "$A/nodes.sh" stop; [ -s "$WORK_DIR/dfwatch.pid" ] && kill "$(cat "$WORK_DIR/dfwatch.pid")" 2>/dev/null; rm -f "$WORK_DIR/.up" ;;
    *) sed -n '2,/^set -euo pipefail/p' "$0" | sed '$d'; exit 2 ;;
esac
