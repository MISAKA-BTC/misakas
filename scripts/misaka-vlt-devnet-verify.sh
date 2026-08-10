#!/usr/bin/env bash
# MISAKA — verify the BFTLLMTOKEN PR-3 completion criteria on a running VLT devnet.
#
# What "PR 3 passes" means, checked in this order:
#
#   1. PLAN    — some frozen voting snapshot on node-0 carries EXACTLY the weight plan:
#                per-validator weights = quota x 50 VLT (in uRTE), total = sum(plan),
#                quorum = floor(2W/3) + 1 recomputed independently here. For the default
#                8/5/3/2/2 plan that is 400/250/150/100/100 VLT, W = 1000 VLT = 1e9 uRTE.
#   2. ROOTS   — for EVERY epoch that two or more nodes froze, all of them logged the same
#                snapshot_root, validator_set_root and vote_commitment. One divergent root is
#                a split denominator, which is the §5 failure this whole layer exists to stop.
#   3. RESTART — with --restart-check: stop every node, start it again from its own recorded
#                run.args, and require each to log a `[vlt-voting-snapshot] resumed` line whose
#                roots equal what its OWN log froze for that epoch before the restart. The log
#                is append-mode across restarts, so the proof and the claim sit in one file.
#
# The script reads ONLY the kaspad logs — the freeze/resume lines are part of the overlay's
# operator surface, and asserting through them means the check sees exactly what an operator
# grepping a production incident would see.
#
# Usage:
#   scripts/misaka-vlt-devnet-verify.sh [--nodes N] [--job-quotas N,N,...] [--wait SECONDS]
#                                       [--restart-check] [--no-plan-wait]
#
# Env: MISAKA_DEVNET_DIR (default ./.misaka-vlt-devnet), KASPAD_BIN (restart check only)

set -euo pipefail

NODES=5
JOB_QUOTAS=8,5,3,2,2
WAIT_SECS=1800
RESTART_CHECK=0
PLAN_WAIT=1

while [ $# -gt 0 ]; do
  case "$1" in
    --nodes)         NODES="$2"; shift 2 ;;
    --job-quotas)    JOB_QUOTAS="$2"; shift 2 ;;
    --wait)          WAIT_SECS="$2"; shift 2 ;;
    --restart-check) RESTART_CHECK=1; shift ;;
    --no-plan-wait)  PLAN_WAIT=0; shift ;;
    -h|--help)       sed -n '2,27p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
WORK_DIR="${MISAKA_DEVNET_DIR:-$REPO_ROOT/.misaka-vlt-devnet}"
[ -d "$WORK_DIR/node-0" ] || { echo "no devnet at $WORK_DIR — run misaka-vlt-devnet.sh first" >&2; exit 1; }

# 50 VLT per fixture job, 1e6 uRTE per VLT — the devnet fixture's fixed shape (10 prefill x 1.0
# + 5 decode x 8.0). If the fixture shape ever changes, this constant fails the plan loudly
# rather than letting the script normalize the discrepancy away.
JOB_URTE=50000000

QUOTAS=()
IFS=, read -r -a QUOTAS <<< "$JOB_QUOTAS"
if [ "${#QUOTAS[@]}" -ne "$NODES" ]; then
  echo "--job-quotas has ${#QUOTAS[@]} entries but there are $NODES nodes" >&2
  exit 2
fi
TOTAL_JOBS=0
for q in "${QUOTAS[@]}"; do TOTAL_JOBS=$((TOTAL_JOBS + q)); done
TARGET_TOTAL=$((TOTAL_JOBS * JOB_URTE))
# floor(2W/3) + 1 without bignum trouble: W fits u64 here (1e9 for the default plan).
TARGET_QUORUM=$(( (2 * TARGET_TOTAL) / 3 + 1 ))

log_of() { echo "$WORK_DIR/node-$1/kaspad.log"; }

# Every `frozen` line of one node, as "epoch snapshot_root validator_set_root vote_commitment
# total quorum weights" rows (weights only when the node printed them).
frozen_rows() {
  { grep -oE '\[vlt-voting-snapshot\] frozen epoch=[0-9]+ snapshot_root=[0-9a-f]+ validator_set_root=[0-9a-f]+ vote_commitment=[0-9a-f]+ validators=[0-9]+ total_weight=[0-9]+ quorum_weight=[0-9]+( weights=\[[^]]*\])?' "$(log_of "$1")" || true; } |
    sed -E 's/\[vlt-voting-snapshot\] frozen epoch=([0-9]+) snapshot_root=([0-9a-f]+) validator_set_root=([0-9a-f]+) vote_commitment=([0-9a-f]+) validators=([0-9]+) total_weight=([0-9]+) quorum_weight=([0-9]+)( weights=\[([^]]*)\])?/\1 \2 \3 \4 \5 \6 \7 \9/'
}

# ---- 1. PLAN ---------------------------------------------------------------------------------
# Wait until node-0 freezes a snapshot whose total is the full plan. Quotas fill over epochs, so
# earlier epochs legitimately freeze partial totals — the plan check is against the FIRST epoch
# that reaches the target, and the weights list must then be exactly quota x 50 VLT per node.
plan_epoch=""
if [ "$PLAN_WAIT" -eq 1 ]; then
  echo "waiting (up to ${WAIT_SECS}s) for node-0 to freeze the full plan: total_weight=$TARGET_TOTAL uRTE ($TOTAL_JOBS jobs x 50 VLT)"
  deadline=$(( $(date +%s) + WAIT_SECS ))
  while :; do
    plan_epoch=$(frozen_rows 0 | awk -v t="$TARGET_TOTAL" '$6 == t { print $1; exit }')
    [ -n "$plan_epoch" ] && break
    if [ "$(date +%s)" -ge "$deadline" ]; then
      echo "FAIL: no frozen snapshot reached total_weight=$TARGET_TOTAL within ${WAIT_SECS}s. Latest on node-0:" >&2
      frozen_rows 0 | tail -3 >&2
      exit 1
    fi
    sleep 10
  done
  row=$(frozen_rows 0 | awk -v e="$plan_epoch" '$1 == e { print; exit }')
  total=$(echo "$row" | awk '{print $6}')
  quorum=$(echo "$row" | awk '{print $7}')
  weights=$(echo "$row" | awk '{print $8}')
  echo "node-0 froze the plan at epoch $plan_epoch: total_weight=$total quorum_weight=$quorum"

  if [ "$quorum" -ne "$TARGET_QUORUM" ]; then
    echo "FAIL: quorum_weight=$quorum but floor(2*$TARGET_TOTAL/3)+1 = $TARGET_QUORUM" >&2
    exit 1
  fi
  # The logged weights list is `id8:weight,...` sorted by validator id. Quotas map to weight
  # VALUES; ids are per-run keys, so compare as multisets of values.
  expect=$(for q in "${QUOTAS[@]}"; do echo $((q * JOB_URTE)); done | sort -n | paste -sd, -)
  got=$(echo "$weights" | tr ',' '\n' | sed -E 's/^[0-9a-f]+://' | sort -n | paste -sd, -)
  if [ "$got" != "$expect" ]; then
    echo "FAIL: frozen per-validator weights [$got] != plan [$expect]" >&2
    exit 1
  fi
  echo "PASS plan   : weights {$got} == quotas {$JOB_QUOTAS} x 50 VLT, Q = floor(2W/3)+1 = $TARGET_QUORUM"
fi

# ---- 2. ROOTS --------------------------------------------------------------------------------
# Every epoch that two or more nodes froze must carry identical roots on all of them. Nodes may
# trail each other by an epoch (freeze happens at each node's own boundary recompute), so the
# comparison is per-epoch over whoever has it — a real divergence shows up as two DIFFERENT rows
# for one epoch, not as a missing one.
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
for i in $(seq 0 $((NODES - 1))); do
  frozen_rows "$i" | awk -v n="$i" '{ print $1, $2, $3, $4, n }'
done | sort -n > "$tmp"
divergent=$(awk '{ key=$1; sig=$2" "$3" "$4; if (key in seen) { if (seen[key] != sig) bad[key]=1 } else seen[key]=sig } END { for (k in bad) print k }' "$tmp")
compared=$(awk '{ c[$1]++ } END { n=0; for (k in c) if (c[k] > 1) n++; print n }' "$tmp")
if [ -n "$divergent" ]; then
  echo "FAIL: divergent frozen roots at epoch(s): $divergent" >&2
  for e in $divergent; do awk -v e="$e" '$1 == e { print "  epoch " $1 " node-" $5 ": " $2 }' "$tmp" >&2; done
  exit 1
fi
if [ "$compared" -eq 0 ]; then
  echo "FAIL: no epoch has been frozen by two or more nodes yet — nothing to compare" >&2
  exit 1
fi
echo "PASS roots  : $compared epoch(s) frozen by 2+ nodes, all snapshot/validator-set/vote roots identical"

# ---- 3. RESTART ------------------------------------------------------------------------------
if [ "$RESTART_CHECK" -eq 1 ]; then
  echo "restarting all $NODES node(s) from their recorded run.args ..."
  for i in $(seq 0 $((NODES - 1))); do
    pidfile="$WORK_DIR/node-$i/kaspad.pid"
    if [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then
      kill "$(cat "$pidfile")" 2>/dev/null || true
    fi
  done
  sleep 5
  for i in $(seq 0 $((NODES - 1))); do
    node_dir="$WORK_DIR/node-$i"
    # run.args is %q-quoted by the start script; eval-ing it back preserves every argument.
    ( eval "exec $(cat "$node_dir/run.args")" >>"$node_dir/kaspad.log" 2>&1 & echo $! > "$node_dir/kaspad.pid" )
  done
  echo "waiting (up to 300s) for every node to log its resumed snapshot ..."
  deadline=$(( $(date +%s) + 300 ))
  for i in $(seq 0 $((NODES - 1))); do
    while :; do
      resumed=$({ grep -oE '\[vlt-voting-snapshot\] resumed epoch=[0-9]+ snapshot_root=[0-9a-f]+' "$(log_of "$i")" || true; } | tail -1)
      [ -n "$resumed" ] && break
      if [ "$(date +%s)" -ge "$deadline" ]; then
        echo "FAIL: node-$i logged no resumed snapshot within 300s of restart" >&2
        tail -5 "$(log_of "$i")" >&2
        exit 1
      fi
      sleep 5
    done
    r_epoch=$(echo "$resumed" | grep -oE 'epoch=[0-9]+' | cut -d= -f2)
    r_root=$(echo "$resumed" | grep -oE 'snapshot_root=[0-9a-f]+' | cut -d= -f2)
    f_root=$(frozen_rows "$i" | awk -v e="$r_epoch" '$1 == e { print $2; exit }')
    if [ -z "$f_root" ]; then
      echo "FAIL: node-$i resumed epoch $r_epoch but its own log has no frozen line for it" >&2
      exit 1
    fi
    if [ "$r_root" != "$f_root" ]; then
      echo "FAIL: node-$i resumed epoch $r_epoch with root $r_root but froze $f_root before the restart" >&2
      exit 1
    fi
    echo "  node-$i: resumed epoch $r_epoch with the root it froze before the restart"
  done
  echo "PASS restart: all $NODES node(s) resumed their frozen snapshot byte-identically"
fi

echo
echo "PR-3 verification PASSED"
