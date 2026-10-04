#!/usr/bin/env bash
# misaka-palw-accounting-v2-drill.sh — ADR-0172's drill (docs/design/palw/consensus-accounting-v2-drill-plan.md).
#
#   misaka-palw-accounting-v2-drill.sh dry        print the plan against this environment; check what can be checked; start NOTHING
#   misaka-palw-accounting-v2-drill.sh evidence   read the logs of a drill that was run (grep only; never starts or stops anything)
#
# This script has no `run`: the drill is the lead's, after the DAA-5,300 combined drill ends, one drill at a time (lanes/DRILL.lock).
# ENV: KASPAD_BIN (the shipping binary), OLD_KASPAD_BIN (the pre-fence release), WORK_DIR, SALT (else $WORK_DIR/SALT),
#      INT11_AT (default 20), F (the fence, default 80: a height no other fence uses), LOGS (default $WORK_DIR/logs/*.out)
set -euo pipefail

WORK_DIR="${WORK_DIR:-$HOME/.misaka-palw-accounting-v2-drill}"
KASPAD_BIN="${KASPAD_BIN:-}"
OLD_KASPAD_BIN="${OLD_KASPAD_BIN:-}"
INT11_AT="${INT11_AT:-20}"
F="${F:-80}"
LOGS="${LOGS:-$WORK_DIR/logs}"
LOCK="$HOME/Downloads/MISAKA-wt-b/lanes/DRILL.lock"

need() { [ -n "${!1:-}" ] || { echo "  [MISSING] $1" >&2; return 1; }; }

dry() {
  echo "== accounting v2 drill (DRY: nothing is started) =="
  echo "fence F=$F, int-11 list armed at $INT11_AT (F must be >= INT11_AT+40 and no other fence's height)"
  [ "$F" -ge $((INT11_AT + 40)) ] || { echo "  [REFUSED] F=$F is below INT11_AT+40=$((INT11_AT + 40)): the fence must be crossed with REAL work flowing" >&2; exit 1; }
  [ "$F" -gt 0 ] || { echo "  [REFUSED] F=0" >&2; exit 1; }
  free_gb=$(df -g "$HOME" | awk 'NR==2 {print $4}')
  echo "free disk: ${free_gb} GB (stop below 15)"
  [ "${free_gb:-0}" -ge 15 ] || { echo "  [REFUSED] under 15 GB free" >&2; exit 1; }
  if [ -e "$LOCK" ]; then echo "  [BLOCKED] $LOCK exists: $(cat "$LOCK"/* 2>/dev/null | head -1) — another drill holds the Mac"; else echo "  drill lock free (take it with: mkdir $LOCK && echo accounting-v2 > $LOCK/owner)"; fi
  need KASPAD_BIN || true
  need OLD_KASPAD_BIN || true
  salt="${SALT:-$(cat "$WORK_DIR/SALT" 2>/dev/null || true)}"
  [ -n "$salt" ] || echo "  [MISSING] SALT (a private chain: --palw-drill-genesis-salt)"
  cat <<PLAN

nodes (<= 4, --ram-scale, tiny classes):
  n0  archival, ARMED build, FALLBACK miner:  kaspad --palw-drill-genesis-salt=\$SALT --palw-drill-int11-at=$INT11_AT --palw-drill-accounting-v2-at=$F \\
        --palw-producer-key=<card0 key> --palw-producer-bond=<card0 bond> --palw-heartbeat-miner-address=<addr> --ram-scale=<x>
  n1  archival, ARMED, FALLBACK miner (card 1)
  n2  archival, ARMED, external REAL producer (a non-operator bond)
  n3  ARMED, started from an EMPTY datadir after DAA $F (IBD, then pruning proof)
  old pre-fence release (OLD_KASPAD_BIN): the mutual-rejection relay
post-launch prerequisites the validator names (F1 same-chain, anchor window, weight cap, lane A, panel seed) are armed through their own drill flags;
the validator refuses the chain by name if one is missing.

steps D1..D10: see docs/design/palw/consensus-accounting-v2-drill-plan.md (crossing, IBD, pruning proof, reorg across F, late REAL after N fallbacks,
120-round claim, attacks, old-vs-new, shuffled orders, emission >= 1,000 claims). Evidence is read from logs and RPC AFTER each action.
PLAN
}

evidence() {
  echo "== evidence from $LOGS (grep only) =="
  shopt -s nullglob
  logs=("$LOGS"/*.out)
  [ "${#logs[@]}" -gt 0 ] || { echo "no logs under $LOGS" >&2; exit 3; }
  fail=0
  for f in "${logs[@]}"; do
    n_dq=$(grep -c "disqualified from virtual chain" "$f" || true)
    n_fm=$(grep -c "coinbase-mismatch" "$f" || true)
    n_fork=$(grep -c "Fork-id mismatch" "$f" || true)
    n_fb=$(grep -c "FALLBACK" "$f" || true)
    printf '%-40s disqualified=%s coinbase-mismatch=%s fork-id-mismatch=%s fallback-lines=%s\n' "$(basename "$f")" "$n_dq" "$n_fm" "$n_fork" "$n_fb"
    case "$(basename "$f")" in old*) ;; *) [ "$n_dq" -eq 0 ] && [ "$n_fm" -eq 0 ] || fail=1;; esac
  done
  if [ "$fail" -eq 0 ]; then echo "PASS (log level): no armed node disqualified a block or logged a coinbase mismatch"; else echo "FAIL: an armed node disqualified a block or logged a coinbase mismatch" >&2; exit 1; fi
}

case "${1:-}" in
  dry) dry ;;
  evidence) evidence ;;
  *) echo "usage: $0 dry | evidence   (there is no 'run': the drill is the lead's, after the 5,300 drill)" >&2; exit 2 ;;
esac
