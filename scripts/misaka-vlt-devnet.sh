#!/usr/bin/env bash
# MISAKA — five-validator private devnet for the Verified LLM Token-Weighted BFT overlay.
#
# Five is not an arbitrary number. The shipped committee shape is 5 drawn / 3 to confirm / 3 to
# refute, so a five-validator network is the smallest one where a verifier committee is a real
# sample rather than the whole set, where one hostile verifier is neither decisive nor pivotal,
# and where the two verdict quorums genuinely cannot both be reached. Anything smaller tests the
# code paths without testing the property they exist for.
#
# What this gives you, in order:
#
#   1. Five kaspad devnet nodes in a mesh, each with its own validator key and stake bond.
#   2. The compute overlay LIVE from `--vlt-devnet <shadow_daa>`: certificates credited,
#      committees drawn, verdicts paid, settled challenges slashing. Finality still on bonded
#      stake — the [vlt-shadow] log line reports, every recompute, what the weight fence WOULD
#      decide if it opened here.
#   3. One full credit window later, the weight fence opens and voting power becomes verified
#      compute. Pass --shadow-only to stay in step 2 forever, which is what you want before
#      committing to a fence on anything public.
#
# The fences are devnet/simnet only. kaspad refuses --vlt-devnet on mainnet/testnet: moving a
# consensus fence belongs in a release, not on a command line.
#
# Build kaspad WITH `--features evm`. Devnet has the EVM lane active from DAA 0, so a kaspad
# without it panics the moment a miner asks for a block template — the chain then never advances
# and no fence is ever crossed.
#
# Usage:
#   scripts/misaka-vlt-devnet.sh [--shadow-only] [--no-mine] [--nodes N] [--shadow-daa N] [--epochs K]
#
# Env:
#   MISAKA_DEVNET_DIR   working directory (default: ./.misaka-vlt-devnet)
#   KASPAD_BIN          kaspad binary (default: ./target/release/kaspad)
#   PALW_WORKER         palw-worker binary; without it nodes run validator-only and mint no VLT
#   MISAMINER_BIN       miner binary (default: ./target/release/misaminer); --no-mine skips it

set -euo pipefail

NODES=5
SHADOW_DAA=200
CREDIT_WINDOW_EPOCHS=8
SHADOW_ONLY=0
MINE=1

while [ $# -gt 0 ]; do
  case "$1" in
    --shadow-only) SHADOW_ONLY=1; shift ;;
    --no-mine)     MINE=0; shift ;;
    --nodes)       NODES="$2"; shift 2 ;;
    --shadow-daa)  SHADOW_DAA="$2"; shift 2 ;;
    --epochs)      CREDIT_WINDOW_EPOCHS="$2"; shift 2 ;;
    -h|--help)     sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
WORK_DIR="${MISAKA_DEVNET_DIR:-$REPO_ROOT/.misaka-vlt-devnet}"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
PALW_WORKER="${PALW_WORKER:-}"
MISAMINER_BIN="${MISAMINER_BIN:-$REPO_ROOT/target/release/misaminer}"

if [ ! -x "$KASPAD_BIN" ]; then
  echo "kaspad not found at $KASPAD_BIN — build it first:" >&2
  echo "  cargo build --release --bin kaspad --features evm" >&2
  exit 1
fi

if [ "$NODES" -lt 4 ]; then
  # 1 executor + min_verifier_confirmations (3) is the floor at which any job can be credited at
  # all. Below it the overlay runs and every certificate sits at zero forever, which looks like a
  # bug and is not one.
  echo "warning: $NODES nodes is below 1 executor + 3 confirmations; no job will ever be creditable" >&2
fi

mkdir -p "$WORK_DIR"
echo "MISAKA VLT devnet"
echo "  nodes           : $NODES"
echo "  shadow fence    : DAA $SHADOW_DAA"
if [ "$SHADOW_ONLY" -eq 1 ]; then
  echo "  weight fence    : DORMANT (Shadow Mode — finality stays on bonded stake)"
else
  echo "  weight fence    : one credit window (K=$CREDIT_WINDOW_EPOCHS) above the shadow fence"
fi
echo "  work dir        : $WORK_DIR"
echo

BASE_P2P=17111
BASE_RPC=17110

# `--addpeer`, NOT `--connect`. `--connect` sets the inbound limit to zero, so a mesh where every
# node passes it is a mesh where every node refuses every connection — five nodes that mine in
# total isolation while the script reports five nodes up. `--addpeer` dials the listed peers and
# still accepts inbound. `--nodnsseed` keeps a private devnet from reaching for public seeders.
peers=(--nodnsseed)
for i in $(seq 0 $((NODES - 1))); do
  peers+=("--addpeer=127.0.0.1:$((BASE_P2P + i * 10))")
done

# A validator's signing key is a 32-byte hex seed at 0600 — `load_validator_seed` refuses a
# group- or world-readable file rather than sign with a key any local user could read. Generated
# per node and kept, so restarting the devnet keeps each node's identity (and its bond).
new_seed() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
  else
    dd if=/dev/urandom bs=32 count=1 2>/dev/null | xxd -p -c 32
  fi
}

for i in $(seq 0 $((NODES - 1))); do
  node_dir="$WORK_DIR/node-$i"
  mkdir -p "$node_dir"
  p2p=$((BASE_P2P + i * 10))
  rpc=$((BASE_RPC + i * 10))

  if [ ! -f "$node_dir/validator.key" ]; then
    new_seed > "$node_dir/validator.key"
    chmod 600 "$node_dir/validator.key"
  fi

  # Every node's own address is in the peer list; the self-dial fails harmlessly, and keeping the
  # list identical across nodes means one mesh definition rather than N-1 bespoke ones.
  args=(
    --devnet
    --appdir="$node_dir"
    --listen="127.0.0.1:$p2p"
    --rpclisten="127.0.0.1:$rpc"
    --utxoindex
    --unsaferpc
    # A fresh private devnet has nothing to sync from, so every node considers itself in IBD and
    # `submit_block` rejects with `IsInIBD` — silently, since the miner counts a rejection report
    # as a mined block. Without this the chain never leaves DAA 0 and no fence is ever crossed.
    --enable-unsynced-mining
    --enable-validator
    --validator-mode=active
    --validator-key="$node_dir/validator.key"
    --vlt-devnet="$SHADOW_DAA"
    --vlt-devnet-credit-window-epochs="$CREDIT_WINDOW_EPOCHS"
  )
  [ "$SHADOW_ONLY" -eq 1 ] && args+=(--vlt-shadow-only)
  if [ -n "$PALW_WORKER" ]; then
    args+=(--enable-compute --compute-worker="$PALW_WORKER" --compute-work-dir="$node_dir/compute")
  fi
  args+=("${peers[@]}")

  echo "node-$i  p2p=$p2p  rpc=$rpc"
  printf '  %q' "$KASPAD_BIN" "${args[@]}" > "$node_dir/run.args"
  ( "$KASPAD_BIN" "${args[@]}" >"$node_dir/kaspad.log" 2>&1 & echo $! > "$node_dir/kaspad.pid" )
done

# A devnet with no miner has a DAA score of zero forever, so no fence is ever crossed and the
# overlay never does anything — the single most confusing way for this script to "work". Mine
# against node-0 and let GHOSTDAG propagate; --allow-burn sends the coinbase to an unspendable
# placeholder, which is right here because nothing in this smoke test spends it.
if [ "$MINE" -eq 1 ]; then
  if [ ! -x "$MISAMINER_BIN" ]; then
    echo "warning: no miner at $MISAMINER_BIN — DAA will stay at 0 and no fence will ever be crossed." >&2
    echo "         build it with: cargo build --release --bin misaminer   (or pass --no-mine)" >&2
  else
    # The miner exits on a refused connection rather than retrying, and node-0's gRPC listener
    # comes up several seconds after the process does — so wait for the port instead of racing it.
    for _ in $(seq 1 60); do
      if (exec 3<>"/dev/tcp/127.0.0.1/$BASE_RPC") 2>/dev/null; then exec 3<&- 3>&-; break; fi
      sleep 1
    done
    "$MISAMINER_BIN" --rpc="127.0.0.1:$BASE_RPC" --network-id=devnet --allow-burn --threads=2 \
      >"$WORK_DIR/miner.log" 2>&1 &
    echo $! > "$WORK_DIR/miner.pid"
    echo
    echo "miner       : pid $(cat "$WORK_DIR/miner.pid") against node-0 (coinbase burned)"
  fi
fi

# Report a node that died on startup, with the reason. Without this the script's happy output is
# identical whether the mesh is running or every node panicked seconds later — which is exactly
# what a kaspad built without `--features evm` does on devnet.
sleep 8
dead=0
for i in $(seq 0 $((NODES - 1))); do
  node_dir="$WORK_DIR/node-$i"
  if ! kill -0 "$(cat "$node_dir/kaspad.pid")" 2>/dev/null; then
    dead=$((dead + 1))
    echo >&2
    echo "node-$i EXITED. Last lines of $node_dir/kaspad.log:" >&2
    tail -5 "$node_dir/kaspad.log" >&2
  fi
done
if [ "$dead" -gt 0 ]; then
  echo >&2
  echo "$dead of $NODES nodes are down; the mesh will not advance." >&2
  exit 1
fi

echo
echo "Started. Watch the overlay come up with:"
echo "  tail -f $WORK_DIR/node-0/kaspad.log | grep -E 'vlt-shadow|stake-score|precommit'"
echo
echo "Each node still needs a funded stake bond before it attests, and the bond outpoint is only"
echo "knowable after the funding transaction lands — so restart each node with its own"
echo "--stake-bond=<txid:index> once bonded (see ADR-0010). Until then the nodes mine and gossip"
echo "but produce no attestations, and the overlay stays at W(E) = 0."
echo "Stop everything with:"
echo "  for p in $WORK_DIR/*/kaspad.pid $WORK_DIR/miner.pid; do kill \"\$(cat \"\$p\")\" 2>/dev/null; done"
