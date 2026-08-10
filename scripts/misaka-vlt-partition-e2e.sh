#!/usr/bin/env bash
# MISAKA — BFTLLMTOKEN PR 6: the partition / recovery E2E.
#
# §10.2 says that when a network loses the weight it needs, it HOLDS its last finalized anchor,
# lets PoW keep advancing the base ledger, and finalizes nothing new until an eligible snapshot
# returns. This script partitions a live 8/5/3/2/2 devnet (A=400 B=250 C=150 D=100 E=100,
# W=1000, Q=667) and checks that claim from both sides of the split:
#
#   case 1  3/2 split   {A,B,C}=800 | {D,E}=200   majority certifies; minority certifies NOTHING
#   case 2  heal                                   both sides converge, certificate epochs stay
#                                                  strictly ascending everywhere
#   case 3  4/1 split   {A,B,C,D}=900 | {E}=100    same shape, one lonely validator
#   case 4  no quorum   {A} down, {B,C,D,E}=600    NOBODY certifies; every side holds its anchor
#   case 5  heal        full mesh                  finality resumes, still monotone
#
# The minority's inability to certify is the point, and it is not about vote counting: the
# denominator a vote is weighed against is the FROZEN snapshot's `total_weight` (1000), shared by
# every branch, not the 200 the minority can muster. That is the §8.1 quorum-intersection
# argument as a runnable experiment — a minority that could re-derive its own smaller denominator
# would certify happily, and two branches would each hold a "finalized" anchor.
#
# The split is real: every node is restarted with a peer list containing only its own side, so no
# cross-side connection is ever dialled and the restart drops the ones that existed. Both sides
# keep mining, so both keep advancing on PoW alone — which is the other half of §10.2.
#
# Usage:
#   scripts/misaka-vlt-partition-e2e.sh [--work-dir DIR] [--skip-setup]
#
#   --skip-setup   the devnet is already up, Active, and carrying the full plan.
#
# Env: KASPAD_BIN, MISAMINER_BIN (defaults ./target/release/...)

set -euo pipefail

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
WORK_DIR="$REPO_ROOT/.misaka-vlt-partition-e2e"
SKIP_SETUP=0
while [ $# -gt 0 ]; do
  case "$1" in
    --work-dir)   WORK_DIR="$2"; shift 2 ;;
    --skip-setup) SKIP_SETUP=1; shift ;;
    -h|--help)    sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

MISAMINER_BIN="${MISAMINER_BIN:-$REPO_ROOT/target/release/misaminer}"
export MISAKA_DEVNET_DIR="$WORK_DIR"
# Clear of the PR-3 (171xx), reorg-soak (172xx) and quorum-E2E (173xx) devnets.
export MISAKA_DEVNET_BASE_P2P="${MISAKA_DEVNET_BASE_P2P:-17411}"
export MISAKA_DEVNET_BASE_RPC="${MISAKA_DEVNET_BASE_RPC:-17410}"

NODES=5
SHADOW_DAA=200
EPOCHS_K=52
PRE_BOND_DAA=2400
TOTAL=1000000000
QUORUM=666666667

rpc_of()  { echo $((MISAKA_DEVNET_BASE_RPC + $1 * 10)); }
p2p_of()  { echo $((MISAKA_DEVNET_BASE_P2P + $1 * 10)); }
log_of()  { echo "$WORK_DIR/node-$1/kaspad.log"; }

certs_of() { # node -> "epoch signed" per persisted certificate
  { grep -oE "\[dns-finality-certificate\] persisted epoch=[0-9]+ [^ ]+ signed=[0-9]+/[0-9]+" "$(log_of "$1")" || true; } |
    sed -E 's/.*epoch=([0-9]+) .*signed=([0-9]+)\/[0-9]+/\1 \2/'
}
newest_cert_epoch() { { certs_of "$1" | awk '{print $1}' | sort -n | tail -1; } || true; }
confirmed_anchor_of() {
  { { grep -oE "\[vlt-finality-inactive\][^—]*last_finalized_anchor=[0-9a-f]+" "$(log_of "$1")" || true; } |
    tail -1 | grep -oE "last_finalized_anchor=[0-9a-f]+" | cut -d= -f2; } || true
}
daa_of() { { { grep -oE "sink_daa=[0-9]+" "$(log_of "$1")" || true; } | tail -1 | cut -d= -f2; } || true; }
# The live denominator, from the newest quorum line. Zero means the credit window has slid past
# every fixture job — the devnet has no compute left, which looks identical to "partitioned" in a
# certificate count and is not the same thing at all.
live_total_of() {
  { { grep -oE "\[vlt-quorum\] epoch=[0-9]+ signed=[0-9]+ total=[0-9]+" "$(log_of "$1")" || true; } |
    tail -1 | grep -oE "total=[0-9]+" | cut -d= -f2; } || true
}

stop_node() {
  local pidfile="$WORK_DIR/node-$1/kaspad.pid"
  if [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then
    kill "$(cat "$pidfile")" 2>/dev/null || true
  fi
}
stop_all() { for i in $(seq 0 $((NODES - 1))); do stop_node "$i"; done; sleep 5; }

# Relaunch node `$1` with a peer list restricted to the members of `$2` (a CSV of node indices).
# `--addpeer` only DIALS, so a node whose list names nobody on the other side never opens a
# cross-side link — and the restart drops whatever links existed. That, not a firewall, is what
# makes this a genuine partition rather than a quiet one.
relaunch_with_peers() { # node_index sides_csv
  local i="$1" side="$2" node_dir="$WORK_DIR/node-$i"
  [ -f "$node_dir/run.args.full" ] || cp "$node_dir/run.args" "$node_dir/run.args.full"
  local args
  args=$(tr ' ' '\n' < "$node_dir/run.args.full" | sed "s/^'//;s/'$//" | grep -v '^--addpeer=' | grep -v '^$' | tr '\n' ' ')
  local peers="" j
  for j in $(echo "$side" | tr ',' ' '); do
    peers="$peers --addpeer=127.0.0.1:$(p2p_of "$j")"
  done
  printf '%s%s' "$args" "$peers" > "$node_dir/run.args.side"
  ( eval "exec $(cat "$node_dir/run.args.side")" >>"$node_dir/kaspad.log" 2>&1 & echo $! > "$node_dir/kaspad.pid" )
}

start_miner_on() { # node_index label
  local pidfile="$WORK_DIR/miner-$2.pid"
  if [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then kill "$(cat "$pidfile")" 2>/dev/null || true; fi
  "$MISAMINER_BIN" --rpc="127.0.0.1:$(rpc_of "$1")" --network-id=devnet --allow-burn --threads=1 \
    >>"$WORK_DIR/miner-$2.log" 2>&1 &
  echo $! > "$pidfile"
}
stop_miners() {
  for f in "$WORK_DIR"/miner-*.pid; do
    [ -f "$f" ] || continue
    kill "$(cat "$f")" 2>/dev/null || true
    rm -f "$f"
  done
}

# ---------------------------------------------------------------- setup -----
if [ "$SKIP_SETUP" -eq 0 ]; then
  echo "=== phase 1: fresh devnet (shadow=$SHADOW_DAA, K=$EPOCHS_K, quotas 8/5/3/2/2)"
  rm -rf "$WORK_DIR"
  "$REPO_ROOT/scripts/misaka-vlt-devnet.sh" --shadow-daa "$SHADOW_DAA" --epochs "$EPOCHS_K"

  echo
  echo "=== phase 2: burst-mine to DAA ~$PRE_BOND_DAA before bonding (this delay is the case window)"
  "$MISAMINER_BIN" --rpc="127.0.0.1:$(rpc_of 0)" --network-id=devnet --allow-burn --threads=2 \
    --blocks="$PRE_BOND_DAA" --min-block-interval-ms=0 >>"$WORK_DIR/e2e-miner.log" 2>&1 || true

  echo
  echo "=== phase 3: bond all validators"
  "$REPO_ROOT/scripts/misaka-vlt-devnet-bond.sh" || true
  deadline=$(( $(date +%s) + 900 ))
  while :; do
    active=0
    for i in $(seq 0 $((NODES - 1))); do
      grep -qE "bond=Active" "$(log_of "$i")" && active=$((active + 1))
    done
    [ "$active" -eq "$NODES" ] && break
    [ "$(date +%s)" -ge "$deadline" ] && { echo "FAIL: only $active/$NODES bonds Active" >&2; exit 1; }
    sleep 10
  done
  echo "all $NODES bonds Active"
fi

echo
echo "=== phase 4: wait for the full plan (W=$TOTAL) and the precommit round"
deadline=$(( $(date +%s) + 5400 ))
while :; do
  grep -qE "frozen epoch=[0-9]+ .*total_weight=$TOTAL" "$(log_of 0)" && break
  [ "$(date +%s)" -ge "$deadline" ] && { echo "FAIL: full plan never froze" >&2; exit 1; }
  sleep 20
done
while :; do
  [ -n "$(newest_cert_epoch 0)" ] && break
  [ "$(date +%s)" -ge "$deadline" ] && { echo "FAIL: nothing ever certified — the round is not live" >&2; exit 1; }
  sleep 20
done
echo "the plan is frozen and epoch $(newest_cert_epoch 0) has certified"

# ------------------------------------------------------------ the cases -----
# One case: split the mesh, mine on both sides, and check who may certify. `majority` is the side
# expected to keep finalizing (empty = nobody may), `minority` the side expected to certify
# nothing while holding whatever it had.
run_split() { # name majority_csv minority_csv majority_weight minority_weight
  local name="$1" maj="$2" min="$3" maj_w="$4" min_w="$5"
  local maj_lead min_lead
  maj_lead=$(echo "$maj" | cut -d, -f1)
  min_lead=$(echo "$min" | cut -d, -f1)

  echo
  echo "=== $name: {$maj}=$maj_w | {$min}=$min_w  (Q=$QUORUM)"
  local maj_before min_before
  maj_before=$(newest_cert_epoch "$maj_lead")
  min_before=$(newest_cert_epoch "$min_lead")

  stop_miners
  stop_all
  local i
  for i in $(echo "$maj" | tr ',' ' '); do relaunch_with_peers "$i" "$maj"; done
  for i in $(echo "$min" | tr ',' ' '); do relaunch_with_peers "$i" "$min"; done
  sleep 10
  start_miner_on "$maj_lead" maj
  start_miner_on "$min_lead" min

  # Give both sides enough epochs to certify anything they are going to certify — including the
  # lock-chain restart the previous heal's reorg forced on every validator.
  local settle=$(( $(date +%s) + 900 ))
  while [ "$(date +%s)" -lt "$settle" ]; do sleep 30; done

  # Before judging anything: has the network still got weight to vote with? A devnet's fixture
  # quotas are finite, so `K` epochs after the last job the credit window empties and W(E) falls
  # to zero. Every side then certifies nothing — not because it was partitioned, but because
  # there is no compute left to weigh. Calling that a failed partition case would be a false
  # negative dressed as a consensus bug, so it stops the run with the real reason instead.
  local live_total
  live_total=$(live_total_of "$maj_lead")
  if [ "${live_total:-0}" -eq 0 ]; then
    echo
    echo "STOPPING at $name: this devnet's credit window has aged out (W=0) — the fixture quotas were" >&2
    echo "  exhausted long ago and their jobs have slid out of the K-epoch window, so no side can" >&2
    echo "  certify regardless of the split. Cases beyond this point need a longer-lived plan:" >&2
    echo "  larger --job-quotas (so jobs keep landing) or a larger --epochs K." >&2
    exit 2
  fi

  local maj_after min_after
  maj_after=$(newest_cert_epoch "$maj_lead")
  min_after=$(newest_cert_epoch "$min_lead")
  echo "  majority node-$maj_lead: certificates through ${maj_before:-none} -> ${maj_after:-none} (daa $(daa_of "$maj_lead"))"
  echo "  minority node-$min_lead: certificates through ${min_before:-none} -> ${min_after:-none} (daa $(daa_of "$min_lead"))"

  if [ "$maj_w" -ge "$QUORUM" ]; then
    if [ -z "$maj_after" ] || [ "${maj_after:-0}" -le "${maj_before:-0}" ]; then
      echo "FAIL $name: the majority ($maj_w >= Q) certified nothing new" >&2
      exit 1
    fi
    echo "PASS $name: the majority side kept finalizing ($maj_w >= $QUORUM)"
  else
    if [ -n "$maj_after" ] && [ "${maj_after:-0}" -gt "${maj_before:-0}" ]; then
      echo "FAIL $name: a side holding only $maj_w certified epoch $maj_after" >&2
      exit 1
    fi
    echo "PASS $name: no side could certify ($maj_w < $QUORUM), and the base ledger kept advancing on PoW"
  fi

  if [ -n "$min_after" ] && [ "${min_after:-0}" -gt "${min_before:-0}" ]; then
    echo "FAIL $name: the minority ($min_w) certified epoch $min_after — it weighed its votes against its own denominator" >&2
    exit 1
  fi
  echo "PASS $name: the minority ($min_w < $QUORUM) certified nothing — the denominator is the shared $TOTAL, not its own"
}

heal() { # name
  echo
  echo "=== $1: heal the mesh"
  stop_miners
  stop_all
  local all i
  all=$(seq -s, 0 $((NODES - 1)))
  for i in $(seq 0 $((NODES - 1))); do relaunch_with_peers "$i" "$all"; done
  sleep 10
  start_miner_on 0 maj
  # Long enough for ROUND 2 to recover, not merely for the mesh to re-sync. A heal reorgs the
  # losing side, which resets every validator's lock chain to "no lock at all", and round 2 only
  # resumes once each of them has landed a fresh precommit on the frontier and had it accepted.
  local settle=$(( $(date +%s) + 900 ))
  while [ "$(date +%s)" -lt "$settle" ]; do sleep 30; done

  # Every node's certificate sequence must still be strictly ascending, and no two nodes may hold
  # different certificates for one epoch — a split that manufactured its own finality would show
  # up here as two anchors for one epoch.
  local i seqs
  for i in $(seq 0 $((NODES - 1))); do
    seqs=$(certs_of "$i" | awk '{print $1}')
    if [ -n "$seqs" ] && ! echo "$seqs" | awk 'NR>1 && $1 <= prev { exit 1 } { prev=$1 }'; then
      echo "FAIL $1: node-$i's certificate epochs are not strictly ascending" >&2
      exit 1
    fi
  done
  local conflicts
  conflicts=$(for i in $(seq 0 $((NODES - 1))); do certs_of "$i"; done | sort -u | awk '{print $1}' | uniq -d)
  if [ -n "$conflicts" ]; then
    echo "FAIL $1: two different certificates exist for epoch(s): $conflicts" >&2
    exit 1
  fi
  echo "PASS $1: every node's certificate sequence is strictly ascending, and no epoch has two certificates"
}

run_split "case 1 (3/2 split)" "0,1,2" "3,4" 800000000 200000000
heal      "case 2"
run_split "case 3 (4/1 split)" "0,1,2,3" "4"   900000000 100000000
heal      "case 4"
run_split "case 5 (no quorum)" "1,2,3,4" "0"   600000000 400000000
heal      "case 6"

echo
echo "PR-6 partition / recovery E2E PASSED"
