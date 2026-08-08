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
# Usage:
#   scripts/misaka-vlt-devnet.sh [--shadow-only] [--nodes N] [--shadow-daa N] [--epochs K]
#
# Env:
#   MISAKA_DEVNET_DIR   working directory (default: ./.misaka-vlt-devnet)
#   KASPAD_BIN          kaspad binary (default: ./target/release/kaspad)
#   PALW_WORKER         palw-worker binary; without it nodes run validator-only and mint no VLT

set -euo pipefail

NODES=5
SHADOW_DAA=200
CREDIT_WINDOW_EPOCHS=8
SHADOW_ONLY=0

while [ $# -gt 0 ]; do
  case "$1" in
    --shadow-only) SHADOW_ONLY=1; shift ;;
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

if [ ! -x "$KASPAD_BIN" ]; then
  echo "kaspad not found at $KASPAD_BIN — build it first:" >&2
  echo "  cargo build --release --bin kaspad" >&2
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

peers=()
for i in $(seq 0 $((NODES - 1))); do
  peers+=("--connect=127.0.0.1:$((BASE_P2P + i * 10))")
done

for i in $(seq 0 $((NODES - 1))); do
  node_dir="$WORK_DIR/node-$i"
  mkdir -p "$node_dir"
  p2p=$((BASE_P2P + i * 10))
  rpc=$((BASE_RPC + i * 10))

  # Every node's own address is in the peer list; kaspad ignores the self-connect, and keeping the
  # list identical across nodes means one mesh definition rather than N-1 bespoke ones.
  args=(
    --devnet
    --appdir="$node_dir"
    --listen="127.0.0.1:$p2p"
    --rpclisten="127.0.0.1:$rpc"
    --utxoindex
    --unsaferpc
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

echo
echo "Started. Watch the overlay come up with:"
echo "  tail -f $WORK_DIR/node-0/kaspad.log | grep -E 'vlt-shadow|stake-score|precommit'"
echo
echo "Each node still needs a funded stake bond before it attests, and the bond outpoint is only"
echo "knowable after the funding transaction lands — so restart each node with its own"
echo "--stake-bond=<txid:index> once bonded (see ADR-0010). Until then the nodes mine and gossip"
echo "but produce no attestations, and the overlay stays at W(E) = 0."
echo "Stop everything with:"
echo "  for p in $WORK_DIR/node-*/kaspad.pid; do kill \"\$(cat \"\$p\")\"; done"
