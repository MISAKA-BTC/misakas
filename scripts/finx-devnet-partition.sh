#!/usr/bin/env bash
# scripts/finx-devnet-partition.sh — FINX (2026-10-08): the finality guard against the PALW fork choice on H1's private devnet.
#
# Designed for H1's harness (tests/hf-onboarding on branch onboard/h1-hf-closed-loop: lib.sh's node layout, the ISOLATED mark,
# start_node/stop_node, rpc/tip). It starts no devnet itself: run it against one `devnet.sh up` already brought up, and only when
# the Lead schedules it. Record: docs/design/palw/finality-palw-consistency.md §6.
#
#   finx-devnet-partition.sh split <ticks> [group]   isolate <group> (default "B D6") for <ticks> DAA on A, rejoin, watch the sinks
#                                                     for WATCH_S seconds (default 1800); writes $OUT/split-<ticks>.json
#   finx-devnet-partition.sh seal [group]            after `split`: keep watching the group's first node until its log shows the first
#                                                     "Finality Violation Detected" (the seal) or the sinks agree; SEAL_HOURS (default 14)
#   finx-devnet-partition.sh fresh <first-peer>      during an isolation: start Z (empty app dir) whose only peers are <first-peer>'s
#                                                     side ("minority" = the isolated group, "majority" = the steady nodes); after the
#                                                     rejoin, report which side's sink Z ends on
#
# What each run decides (the pipeline tests named are the in-process versions):
#   split 2   — control: a two-tick tie is GHOSTDAG's; the sinks must agree within a few minutes (finx_p0_a, k = 2).
#   split 3   — V2: nothing economic on either side (3 ticks is under the anchor delay, so no licence lands inside the split); the
#               status quo predicts a PERMANENT split: B logs "after refusing the heavier candidate … all-economic tie deeper than
#               the shallow window" on every resolve and A never weighs B (finx_p0_a, k = 3).
#   seal      — V3: the minority's own heartbeats move its finality point past the fork; the first "Finality Violation Detected" in
#               B's log after the rejoin is the seal. Predicted: 600 blue score / B's blue score a tick (finx_p0_facts measures 2 and
#               3 a slot) ≈ 200–300 DAA ≈ 7–10 h on testnet-12's depth (the drill's depth is whatever `devnet.sh env` prints).
#   fresh     — V5: Z joins during the split and ends on whichever side served it first (finx_p0_c's fresh nodes).
set -euo pipefail
H1=${H1:-/Users/wata/Downloads/MISAKA-wt-b/wh-h1/tests/hf-onboarding}
H1_SNAPSHOT=${H1_SNAPSHOT:-finx}   # lib.sh's snapshot re-exec is for H1's own scripts
. "$H1/lib.sh"
OUT=${OUT:-$WORK_DIR/finx}; mkdir -p "$OUT"
WATCH_S=${WATCH_S:-1800}
cmd=${1:-}; shift || true

sink_of() { rpc "$1" getBlockDagInfo '{}' 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["sink"])' 2>/dev/null || echo "?"; }
dag_of() { rpc "$1" getBlockDagInfo '{}' 2>/dev/null || echo '{}'; }
log_since() { local n=$1 c; c=$(cat "$WORK_DIR/$n/start.cursor" 2>/dev/null || echo 0); tail -c +$((c + 1)) "$WORK_DIR/$n/kaspad.out"; }

isolate() { local g=$1 m; for m in $g; do echo "$g" > "$WORK_DIR/$m/ISOLATED"; done; for m in $g; do stop_node "$m"; done; for m in $g; do start_node "$m"; done; }
rejoin() { local g=$1 m; for m in $g; do rm -f "$WORK_DIR/$m/ISOLATED"; stop_node "$m"; start_node "$m"; done; }

do_split() {
    local ticks=${1:?ticks}; local g=${2:-"B D6"}; local first=${g%% *}
    isolate "$g"
    say "finx split: {$g} isolated; waiting $ticks DAA on A"
    local a0; a0=$(tip A); while [ "$(tip A)" -lt $((a0 + ticks)) ] 2>/dev/null; do sleep 15; done
    local before_a before_b; before_a=$(dag_of A); before_b=$(dag_of "$first")
    rejoin "$g"
    local t0=$SECONDS agreed_at="" sa sb
    while [ $((SECONDS - t0)) -lt "$WATCH_S" ]; do
        sa=$(sink_of A); sb=$(sink_of "$first")
        if [ "$sa" = "$sb" ] && [ "$sa" != "?" ]; then agreed_at=$((SECONDS - t0)); break; fi
        sleep 10
    done
    local refusals violations explained
    refusals=$(log_since "$first" | grep -c "after refusing the heavier candidate" || true)
    violations=$(log_since "$first" | grep -c "Finality Violation Detected" || true)
    explained=$(log_since "$first" | grep -o "this is the PALW deep-reorg rule[^.]*" | tail -1 || true)
    python3 - "$OUT/split-$ticks.json" "$ticks" "$g" "$before_a" "$before_b" "$(dag_of A)" "$(dag_of "$first")" "${agreed_at:-}" "$refusals" "$violations" "$explained" <<'PY'
import json, sys
o, ticks, g, ba, bb, aa, ab, agreed, refusals, violations, explained = sys.argv[1:]
J = lambda x: json.loads(x) if x else {}
d = lambda x: {k: J(x).get(k) for k in ("virtualDaaScore", "sink", "blockCount", "pruningPointHash")}
r = {"ticks": int(ticks), "group": g, "before_rejoin": {"A": d(ba), "minority": d(bb)}, "after_watch": {"A": d(aa), "minority": d(ab)},
     "sinks_agreed_after_s": int(agreed) if agreed else None, "minority_refusals_logged": int(refusals),
     "minority_finality_violations_logged": int(violations), "last_refusal_explained": explained or None}
r["verdict"] = "CONVERGED" if agreed else "SPLIT"
json.dump(r, open(o, "w"), indent=1)
print(json.dumps({k: r[k] for k in ("ticks", "verdict", "sinks_agreed_after_s", "minority_refusals_logged", "last_refusal_explained")}))
PY
}

do_seal() {
    local g=${1:-"B D6"}; local first=${g%% *}
    local hours=${SEAL_HOURS:-14} t0=$SECONDS d0; d0=$(tip "$first")
    while [ $((SECONDS - t0)) -lt $((hours * 3600)) ]; do
        if [ "$(sink_of A)" = "$(sink_of "$first")" ]; then
            echo "{\"verdict\": \"CONVERGED before any seal\", \"after_s\": $((SECONDS - t0))}" | tee "$OUT/seal.json"; return 0
        fi
        if log_since "$first" | grep -q "Finality Violation Detected"; then
            echo "{\"verdict\": \"SEALED\", \"after_s\": $((SECONDS - t0)), \"minority_daa_advance\": $(( $(tip "$first") - d0 ))}" | tee "$OUT/seal.json"; return 0
        fi
        sleep 60
    done
    echo "{\"verdict\": \"neither within ${hours} h\"}" | tee "$OUT/seal.json"
}

do_fresh() {
    local side=${1:?minority|majority}; local g=${2:-"B D6"}
    stop_node Z || true; rm -rf "$WORK_DIR/Z/app"
    case $side in
        minority) echo "$g Z" > "$WORK_DIR/Z/ISOLATED" ;;   # Z peers only with the isolated group's shifted ports
        majority) rm -f "$WORK_DIR/Z/ISOLATED" ;;           # Z peers with the steady nodes
        *) die "first peer: minority or majority" ;;
    esac
    mkdir -p "$WORK_DIR/Z"; start_node Z
    say "finx fresh: Z syncing from the $side; rejoin the group (and restart Z without ISOLATED) when ready, then: $0 fresh-report"
}

do_fresh_report() {
    local sz sa sb; sz=$(sink_of Z); sa=$(sink_of A); sb=$(sink_of B)
    local on; if [ "$sz" = "$sa" ]; then on=majority; elif [ "$sz" = "$sb" ]; then on=minority; else on="neither (in flight)"; fi
    echo "{\"Z_sink\": \"$sz\", \"Z_on\": \"$on\"}" | tee "$OUT/fresh.json"
}

case $cmd in
    split) do_split "$@" ;;
    seal) do_seal "$@" ;;
    fresh) do_fresh "$@" ;;
    fresh-report) do_fresh_report ;;
    *) sed -n '2,24p' "$0"; exit 2 ;;
esac
