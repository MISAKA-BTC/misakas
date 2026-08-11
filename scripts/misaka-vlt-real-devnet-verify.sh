#!/usr/bin/env bash
# MISAKA — verify that DNS finality is running on REAL verified LLM compute.
#
# The fixture verify (misaka-vlt-devnet-verify.sh) proves a quota PLAN lands exactly; this one
# proves the production claim end to end on a real model (Qwen3.5-2B palw-lite):
#
#   1. RUNTIME  — all N nodes enabled the compute role with EXACTLY the worker's own probed
#                 runtime hash (read live from `$PALW_WORKER --mode manifest`), i.e. the
#                 registered Qwen3.5-2B profile, not the fixture and not a mock.
#   2. JOBS     — every node originated (committed) at least one real job, and the mesh logged
#                 at least MIN_CONFIRMS "our replay reproduced R_j" confirmations: independent
#                 re-inference on other nodes reproduced the executor's receipt byte-for-byte.
#                 That line is the verified-LLM moment — everything downstream trades on it.
#   3. WEIGHT   — the weight fence was reached and the activation machine activated a snapshot
#                 with total_weight > 0: voting power is now W_i(E) = min{C_i, λB_i} from those
#                 replay-verified jobs.
#   4. FINALITY — at least MIN_CERT_EPOCHS distinct epochs persisted a DnsFinalityCertificate
#                 (the durable weighted-quorum proof) on node-0, with strictly ascending epochs;
#                 every node persisted at least one; for epochs persisted by 2+ nodes the quorum
#                 value is identical.
#   5. ROOTS    — every epoch frozen by 2+ nodes froze identical snapshot/validator-set/vote
#                 roots (the §5 shared-denominator property, unchanged from the fixture verify).
#
# Reads ONLY the kaspad logs plus one `--mode manifest` invocation of the worker — the same
# surface an operator greps in an incident.
#
# Usage:
#   scripts/misaka-vlt-real-devnet-verify.sh [--nodes N] [--wait SECONDS]
#
# Env: MISAKA_DEVNET_DIR (default ./.misaka-vlt-real), PALW_WORKER (required)

set -euo pipefail

NODES=5
WAIT_SECS=4800
MIN_CONFIRMS=3
MIN_CERT_EPOCHS=2

while [ $# -gt 0 ]; do
  case "$1" in
    --nodes) NODES="$2"; shift 2 ;;
    --wait)  WAIT_SECS="$2"; shift 2 ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
WORK_DIR="${MISAKA_DEVNET_DIR:-$REPO_ROOT/.misaka-vlt-real}"
[ -d "$WORK_DIR/node-0" ] || { echo "no devnet at $WORK_DIR — run misaka-vlt-devnet.sh first" >&2; exit 1; }
[ -n "${PALW_WORKER:-}" ] && [ -x "$PALW_WORKER" ] || { echo "PALW_WORKER must point at the palw-worker binary" >&2; exit 1; }

log_of() { echo "$WORK_DIR/node-$1/kaspad.log"; }

# ---- 1. RUNTIME ------------------------------------------------------------------------------
# The expectation is read from the worker itself, so this script can never drift from the pins:
# if the worker's identity moved, the nodes would have refused it and this check fails loudly.
manifest=$("$PALW_WORKER" --mode manifest)
expect_runtime=$(echo "$manifest" | grep -oE '"runtime_manifest_hash":"[0-9a-f]+"' | cut -d'"' -f4)
expect_class=$(echo "$manifest" | grep -oE '"runtime_class_id":"[0-9a-f]+"' | cut -d'"' -f4)
[ -n "$expect_runtime" ] && [ -n "$expect_class" ] || { echo "FAIL: could not read the worker manifest" >&2; exit 1; }
for i in $(seq 0 $((NODES - 1))); do
  enabled=$({ grep -oE '\[validator-compute\] enabled: runtime=[0-9a-f]+ class=[0-9a-f]+' "$(log_of "$i")" || true; } | tail -1)
  got_runtime=$(echo "$enabled" | grep -oE 'runtime=[0-9a-f]+' | cut -d= -f2)
  got_class=$(echo "$enabled" | grep -oE 'class=[0-9a-f]+' | cut -d= -f2)
  if [ "$got_runtime" != "$expect_runtime" ] || [ "$got_class" != "$expect_class" ]; then
    echo "FAIL: node-$i compute role runtime/class do not match the worker's manifest" >&2
    echo "  expected runtime=$expect_runtime class=$expect_class" >&2
    echo "  node-$i logged: ${enabled:-<no enabled line>}" >&2
    exit 1
  fi
done
echo "PASS runtime : all $NODES nodes run the worker's own probed profile (runtime=${expect_runtime:0:16}…)"

# ---- 2. JOBS ---------------------------------------------------------------------------------
echo "waiting (up to ${WAIT_SECS}s) for real jobs: every node committed >=1, mesh replay-confirmations >=$MIN_CONFIRMS ..."
deadline=$(( $(date +%s) + WAIT_SECS ))
while :; do
  committed_nodes=0
  confirms=0
  for i in $(seq 0 $((NODES - 1))); do
    if grep -q 'compute: committed to job' "$(log_of "$i")"; then committed_nodes=$((committed_nodes + 1)); fi
    n=$({ grep -c 'our replay reproduced R_j' "$(log_of "$i")" || true; } | tail -1)
    confirms=$((confirms + n))
  done
  [ "$committed_nodes" -eq "$NODES" ] && [ "$confirms" -ge "$MIN_CONFIRMS" ] && break
  if [ "$(date +%s)" -ge "$deadline" ]; then
    echo "FAIL: after ${WAIT_SECS}s: $committed_nodes/$NODES nodes committed a job, $confirms replay confirmations. node-0 compute tail:" >&2
    grep -E 'compute' "$(log_of 0)" | tail -5 >&2
    exit 1
  fi
  sleep 15
done
echo "PASS jobs    : $committed_nodes/$NODES nodes originated; $confirms independent replays reproduced an executor's R_j"

# ---- 3. WEIGHT -------------------------------------------------------------------------------
echo "waiting (up to ${WAIT_SECS}s) for the weight fence and an activated snapshot with W > 0 ..."
deadline=$(( $(date +%s) + WAIT_SECS ))
activated=""
while :; do
  if grep -q '\[vlt-weight-fence-reached\]' "$(log_of 0)"; then
    activated=$({ grep -oE '\[vlt-weight-snapshot-activated\] epoch=[0-9]+ snapshot_root=[0-9a-f]+ total_weight=[0-9]+' "$(log_of 0)" || true; } | tail -1)
    [ -n "$activated" ] && break
  fi
  if [ "$(date +%s)" -ge "$deadline" ]; then
    echo "FAIL: no activated weight snapshot within ${WAIT_SECS}s. node-0 fence/activation tail:" >&2
    grep -E 'vlt-weight|vlt-activation|vlt-finality-inactive' "$(log_of 0)" | tail -6 >&2
    exit 1
  fi
  sleep 15
done
act_epoch=$(echo "$activated" | grep -oE 'epoch=[0-9]+' | cut -d= -f2)
act_weight=$(echo "$activated" | grep -oE 'total_weight=[0-9]+' | cut -d= -f2)
if [ -z "$act_weight" ] || [ "$act_weight" -le 0 ]; then
  echo "FAIL: activated snapshot carries total_weight=$act_weight" >&2
  exit 1
fi
echo "PASS weight  : fence reached; epoch $act_epoch activated with total_weight=$act_weight uRTE of replay-verified compute"

# ---- 4. FINALITY -----------------------------------------------------------------------------
echo "waiting (up to ${WAIT_SECS}s) for >=$MIN_CERT_EPOCHS DnsFinalityCertificate epochs on node-0 and >=1 on every node ..."
deadline=$(( $(date +%s) + WAIT_SECS ))
cert_rows() {
  { grep -oE '\[dns-finality-certificate\] persisted epoch=[0-9]+ target_anchor=[0-9a-f]+ signed=[0-9]+/[0-9]+ quorum=[0-9]+' "$(log_of "$1")" || true; } |
    sed -E 's/\[dns-finality-certificate\] persisted epoch=([0-9]+) target_anchor=([0-9a-f]+) signed=([0-9]+)\/([0-9]+) quorum=([0-9]+)/\1 \2 \3 \4 \5/'
}
while :; do
  n0_epochs=$(cert_rows 0 | awk '{print $1}' | sort -un | wc -l | tr -d ' ')
  all_have=1
  for i in $(seq 0 $((NODES - 1))); do
    [ "$(cert_rows "$i" | wc -l | tr -d ' ')" -ge 1 ] || { all_have=0; break; }
  done
  [ "$n0_epochs" -ge "$MIN_CERT_EPOCHS" ] && [ "$all_have" -eq 1 ] && break
  if [ "$(date +%s)" -ge "$deadline" ]; then
    echo "FAIL: after ${WAIT_SECS}s node-0 has $n0_epochs certificate epoch(s) (need $MIN_CERT_EPOCHS), all-nodes=$all_have. node-0 quorum tail:" >&2
    grep -E 'vlt-quorum|dns-finality-certificate|vlt-finality-inactive' "$(log_of 0)" | tail -6 >&2
    exit 1
  fi
  sleep 15
done
# Strictly ascending epochs on node-0 — the PR-4 invariant, unchanged by what backs the weight.
if ! cert_rows 0 | awk '{print $1}' | awk 'NR>1 && $1 <= prev { exit 1 } { prev=$1 }'; then
  echo "FAIL: node-0 persisted certificate epochs out of order:" >&2
  cert_rows 0 | awk '{print "  epoch " $1}' >&2
  exit 1
fi
# Epochs certified by 2+ nodes must agree on the quorum value and the anchor.
tmp_c=$(mktemp)
for i in $(seq 0 $((NODES - 1))); do cert_rows "$i" | awk -v n="$i" '{print $1, $2, $5, n}'; done | sort -n > "$tmp_c"
divergent=$(awk '{ key=$1; sig=$2" "$3; if (key in seen) { if (seen[key] != sig) bad[key]=1 } else seen[key]=sig } END { for (k in bad) print k }' "$tmp_c")
if [ -n "$divergent" ]; then
  echo "FAIL: divergent finality certificates at epoch(s): $divergent" >&2
  for e in $divergent; do awk -v e="$e" '$1 == e { print "  epoch " $1 " node-" $4 ": anchor=" $2 " quorum=" $3 }' "$tmp_c" >&2; done
  rm -f "$tmp_c"
  exit 1
fi
rm -f "$tmp_c"
last_cert=$(cert_rows 0 | tail -1)
echo "PASS finality: node-0 persisted $n0_epochs certificate epoch(s), strictly ascending, cross-node identical; latest: epoch $(echo "$last_cert" | awk '{print $1}') signed=$(echo "$last_cert" | awk '{print $3}')/$(echo "$last_cert" | awk '{print $4}') quorum=$(echo "$last_cert" | awk '{print $5}')"

# ---- 5. ROOTS --------------------------------------------------------------------------------
frozen_rows() {
  { grep -oE '\[vlt-voting-snapshot\] frozen epoch=[0-9]+ snapshot_root=[0-9a-f]+ validator_set_root=[0-9a-f]+ vote_commitment=[0-9a-f]+' "$(log_of "$1")" || true; } |
    sed -E 's/\[vlt-voting-snapshot\] frozen epoch=([0-9]+) snapshot_root=([0-9a-f]+) validator_set_root=([0-9a-f]+) vote_commitment=([0-9a-f]+)/\1 \2 \3 \4/'
}
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
echo "PASS roots   : $compared epoch(s) frozen by 2+ nodes, all snapshot/validator-set/vote roots identical"

echo
echo "REAL-LLM DNS FINALITY VERIFIED: replay-checked Qwen3.5-2B compute is the voting weight behind $n0_epochs certified epoch(s)"
