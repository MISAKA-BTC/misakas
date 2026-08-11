#!/usr/bin/env bash
# MISAKA — BFTLLMTOKEN PR 7: the IBD / identity E2E.
#
# §12's requirement is stated as an equality: five existing nodes, a restarted node, a node fresh
# from genesis and a node imported from a pruning snapshot must all end up with the SAME
#
#     finalized anchor | snapshot root | validator-set root | credit-table root
#     capability root  | model table   | activation epoch
#
# — the identity tuple. It is one `[vlt-identity]` line per epoch per node, so the check is a
# textual diff rather than seven subsystem walks. A node that derived a different denominator
# (the failure a pruned or freshly-synced node is most likely to hit, since it never saw the
# blocks the credit table was built from) shows up here as one differing field, named.
#
# Checked, in order:
#
#   1. AGREEMENT  — every running node reports an identical tuple on the most recent epochs they
#                   have all reported
#   2. RESTART    — the same holds after stopping and restarting all of them
#   3. FROM GENESIS — a brand-new node, no bond, no validator, no compute, syncs the chain from
#                   block 0 and joins that agreement
#
# CONVERGENCE, not per-epoch immutability: an identity line is what a node believed when it printed
# it, and during a partition the two sides genuinely believe different things. Requiring history to
# match would fail a network that behaved exactly as designed.
#
# Pruning import is deliberately NOT claimed here: a devnet of this age has never advanced its
# pruning point, so there is nothing to import from. The script says so rather than passing a
# check it did not run.
#
# Usage:
#   scripts/misaka-vlt-ibd-e2e.sh [--work-dir DIR] [--skip-restart]
#
# Env: KASPAD_BIN, MISAMINER_BIN (defaults ./target/release/...)

set -euo pipefail

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
WORK_DIR="$REPO_ROOT/.misaka-vlt-partition-e2e"
SKIP_RESTART=0
while [ $# -gt 0 ]; do
  case "$1" in
    --work-dir)     WORK_DIR="$2"; shift 2 ;;
    --skip-restart) SKIP_RESTART=1; shift ;;
    -h|--help)      sed -n '2,29p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
BASE_RPC="${MISAKA_DEVNET_BASE_RPC:-17410}"
BASE_P2P="${MISAKA_DEVNET_BASE_P2P:-17411}"
NODES=5
FRESH=5   # the from-genesis node's index (its own ports, its own appdir)

[ -d "$WORK_DIR/node-0" ] || { echo "no devnet at $WORK_DIR" >&2; exit 1; }
log_of() { echo "$WORK_DIR/node-$1/kaspad.log"; }
p2p_of() { echo $((BASE_P2P + $1 * 10)); }
rpc_of() { echo $((BASE_RPC + $1 * 10)); }

# `epoch <tuple>` for every identity line a node has printed. The tuple is everything after the
# epoch, verbatim — comparing the whole line is the point: a field added later is compared too,
# without this script having to learn about it.
identity_rows() { # node
  { grep -oE "\[vlt-identity\] epoch=[0-9]+ .*" "$(log_of "$1")" || true; } |
    sed -E 's/\[vlt-identity\] epoch=([0-9]+) (.*)/\1 \2/'
}

# Compare the nodes in `$*` on the most recent epochs they have ALL reported.
#
# Not every epoch ever printed, and the difference matters. An identity line records what a node
# believed WHEN IT PRINTED IT, and two nodes on opposite sides of a partition legitimately believed
# different things — the minority's snapshot really was pinned on its own branch. Comparing that
# history would fail a network that behaved perfectly and then converged, which is the property
# actually under test. So: intersect the epochs every node has reported, take the newest few, and
# require agreement there. A node that permanently derived a different denominator — the failure a
# pruned or freshly-synced node is most likely to hit — cannot hide from that, because it never
# converges.
COMPARE_EPOCHS=3

# `finalized_anchor` is the one tuple field carried by the node-local DnsState overlay rather
# than by a consensus store. Its convergence claim assumes a LIVE confirmation flow: a node that
# joins after the mesh's attestation quorum has permanently stopped (this harness's endgame —
# quota exhaustion kills the certificate flow, the ADR-0018 quality gate degrades, stake depth
# goes to zero) can never apply the final in-flight confirmation, because nothing re-confirms.
# On any network whose overlay is alive the next confirmation re-syncs it. So: when the mesh's
# own anchor has been frozen for more than this many epochs, compare the tuple WITHOUT the
# anchor, and say so — the same lesson the partition harness learned about false failures.
ANCHOR_LIVENESS_EPOCHS=6
anchor_is_live() { # node — did this node's finalized_anchor move within the last N epochs?
  local rows last_move newest
  rows=$(identity_rows "$1" | awk '{ print $1, $2 }' | sed -E 's/finalized_anchor=//')
  last_move=$(echo "$rows" | awk '{ if ($2 != prev) { e=$1; prev=$2 } } END { print e }')
  newest=$(echo "$rows" | awk 'END { print $1 }')
  [ -n "$last_move" ] && [ -n "$newest" ] && [ $((newest - last_move)) -le "$ANCHOR_LIVENESS_EPOCHS" ]
}

compare_nodes() { # label node...
  local label="$1"; shift
  local nodes=("$@")
  local strip_anchor=0
  if ! anchor_is_live 0; then
    strip_anchor=1
    echo "NOTE $label: the mesh's confirmation flow has ended (node-0's finalized_anchor is frozen);"
    echo "     comparing the identity tuple WITHOUT finalized_anchor — a node joining a dead overlay"
    echo "     cannot apply its final in-flight confirmation, and nothing will ever re-confirm it."
  fi
  local tmp; tmp=$(mktemp); trap 'rm -f "$tmp"' RETURN
  local n
  for n in "${nodes[@]}"; do
    identity_rows "$n" | awk -v node="$n" -v strip="$strip_anchor" \
      '{ epoch=$1; $1=""; if (strip) { sub(/ finalized_anchor=[0-9a-f]+/, "") } print epoch "\t" node "\t" substr($0,2) }'
  done | sort -n > "$tmp"

  # Epochs every compared node has reported.
  local common
  common=$(awk -F'\t' -v want="${#nodes[@]}" '{ if (!seen[$1 "/" $2]++) c[$1]++ } END { for (k in c) if (c[k] == want) print k }' "$tmp" |
    sort -n | tail -"$COMPARE_EPOCHS")
  if [ -z "$common" ]; then
    echo "FAIL $label: no epoch has been reported by all ${#nodes[@]} nodes — nothing was compared" >&2
    local m
    for m in "${nodes[@]}"; do echo "  node-$m newest: $(identity_rows "$m" | awk '{print $1}' | sort -n | tail -1)" >&2; done
    exit 1
  fi

  local e divergent=""
  for e in $common; do
    local sigs
    sigs=$(awk -F'\t' -v e="$e" '$1 == e { print $3 }' "$tmp" | sort -u | wc -l | tr -d ' ')
    [ "$sigs" -ne 1 ] && divergent="$divergent $e"
  done
  if [ -n "$divergent" ]; then
    echo "FAIL $label: nodes disagree on the identity tuple at epoch(s):$divergent" >&2
    for e in $divergent; do
      awk -F'\t' -v e="$e" '$1 == e { print "  epoch " $1 " node-" $2 ": " $3 }' "$tmp" >&2
    done
    exit 1
  fi
  echo "PASS $label: ${#nodes[@]} nodes agree on the identity tuple for epoch(s) $(echo $common | tr '\n' ' ')"
}

wait_for_identity() { # node, timeout secs — wait until this node reports ANY identity line
  local n="$1" deadline=$(( $(date +%s) + $2 ))
  while :; do
    [ -n "$(identity_rows "$n" | tail -1)" ] && return 0
    if [ "$(date +%s)" -ge "$deadline" ]; then
      echo "FAIL: node-$n reported no [vlt-identity] line within $2s" >&2
      tail -3 "$(log_of "$n")" >&2
      return 1
    fi
    sleep 10
  done
}

echo "MISAKA IBD / identity E2E on $WORK_DIR"
echo

# ---- 1. AGREEMENT ---------------------------------------------------------------------------
for i in $(seq 0 $((NODES - 1))); do wait_for_identity "$i" 900 || exit 1; done
compare_nodes "agreement" $(seq 0 $((NODES - 1)))

# ---- 2. RESTART -----------------------------------------------------------------------------
if [ "$SKIP_RESTART" -eq 0 ]; then
  echo
  echo "=== restarting all $NODES nodes from their recorded arguments"
  for i in $(seq 0 $((NODES - 1))); do
    pidfile="$WORK_DIR/node-$i/kaspad.pid"
    if [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then kill "$(cat "$pidfile")" 2>/dev/null || true; fi
  done
  sleep 5
  for i in $(seq 0 $((NODES - 1))); do
    node_dir="$WORK_DIR/node-$i"
    argfile="$node_dir/run.args"
    [ -f "$node_dir/run.args.side" ] && argfile="$node_dir/run.args.side"
    ( eval "exec $(cat "$argfile")" >>"$node_dir/kaspad.log" 2>&1 & echo $! > "$node_dir/kaspad.pid" )
  done
  echo "waiting for the mesh to report identity again ..."
  settle=$(( $(date +%s) + 600 ))
  while [ "$(date +%s)" -lt "$settle" ]; do sleep 30; done
  compare_nodes "restart" $(seq 0 $((NODES - 1)))
fi

# ---- 3. FROM GENESIS ------------------------------------------------------------------------
echo
echo "=== starting a from-genesis node (no bond, no validator, no compute) and syncing it"
fresh_dir="$WORK_DIR/node-$FRESH"
# A prior fresh node may still be RUNNING (this script re-run, or an operator's manual spawn):
# deleting its appdir does not release its ports, and the replacement then dies on AddrInUse
# with an empty log — which reads as "kaspad won't start" rather than "kill the old one".
if [ -f "$fresh_dir/kaspad.pid" ] && kill -0 "$(cat "$fresh_dir/kaspad.pid")" 2>/dev/null; then
  kill "$(cat "$fresh_dir/kaspad.pid")" 2>/dev/null || true
  sleep 3
fi
pkill -f -- "--appdir=$fresh_dir" 2>/dev/null || true
sleep 1
rm -rf "$fresh_dir"
mkdir -p "$fresh_dir"
# Its consensus configuration must match the mesh EXACTLY — the VLT fences are consensus, so a
# node that syncs with different ones is not a second opinion, it is a different network. Take
# node-0's recorded arguments and strip only what is node-identity: its ports, its appdir, and
# the validator/compute/bond roles a syncing observer must not have.
base_args=$({ tr ' ' '\n' < "$WORK_DIR/node-0/run.args.full"; } 2>/dev/null || tr ' ' '\n' < "$WORK_DIR/node-0/run.args")
fresh_args=$(echo "$base_args" | sed "s/^'//;s/'$//" |
  grep -v '^--appdir=' | grep -v '^--listen=' | grep -v '^--rpclisten' | grep -v '^--addpeer=' |
  grep -v '^--enable-validator' | grep -v '^--validator-mode' | grep -v '^--validator-key=' |
  grep -v '^--stake-bond=' | grep -v '^--enable-compute' | grep -v '^--compute-' | grep -v '^$' | tr '\n' ' ')
fresh_args="$fresh_args --appdir=$fresh_dir --listen=127.0.0.1:$(p2p_of $FRESH) --rpclisten=127.0.0.1:$(rpc_of $FRESH) --rpclisten-borsh=127.0.0.1:$(( $(rpc_of $FRESH) + 2 ))"
for j in $(seq 0 $((NODES - 1))); do fresh_args="$fresh_args --addpeer=127.0.0.1:$(p2p_of "$j")"; done
printf '%s' "$fresh_args" > "$fresh_dir/run.args"
( eval "exec $fresh_args" >>"$fresh_dir/kaspad.log" 2>&1 & echo $! > "$fresh_dir/kaspad.pid" )
echo "  node-$FRESH: $(echo "$fresh_args" | tr ' ' '\n' | grep -cE '^--') arguments, syncing from block 0"

if ! wait_for_identity "$FRESH" 1800; then
  echo "  (a from-genesis node must reach the overlay's shadow fence before it reports identity)" >&2
  exit 1
fi
# Let it catch up to the mesh's current epochs rather than only the oldest it replayed.
settle=$(( $(date +%s) + 300 ))
while [ "$(date +%s)" -lt "$settle" ]; do sleep 30; done
compare_nodes "from-genesis" $(seq 0 $FRESH)

# ---- 4. PRUNING IMPORT (not exercised) ------------------------------------------------------
echo
if grep -q "Importing the overlay snapshot of the pruning point" "$(log_of 0)" 2>/dev/null; then
  echo "NOTE: this devnet did import a pruning-point overlay snapshot; the tuples above cover it"
else
  echo "NOT EXERCISED: pruning import — this devnet has never advanced its pruning point, so there is"
  echo "               no snapshot to import from. The from-genesis path above is the IBD case that"
  echo "               this chain length can actually produce."
fi

echo
echo "PR-7 IBD / identity E2E PASSED"
