#!/usr/bin/env bash
# MISAKA — bond every validator of a running private devnet (activation runbook step 4).
#
# W(E) > 0 needs two things: verified compute, and a bond to collateralize it. This script does
# the second. Until it runs, the nodes started by `misaka-vlt-devnet.sh` mine and gossip but
# attest nothing, because a validator with no bond is not in the active set.
#
# The flow is the production one, not a shortcut:
#
#   1. read each validator's own funding address (derived from its signing key)
#   2. mine to that address until it holds enough MATURE coinbase
#   3. `kaspa-pq-validator bond` — aggregates the mature UTXOs, builds and submits the real
#      StakeBond transaction, and prints the bond outpoint
#   4. restart the node with `--stake-bond=<txid>:0`, which is the only way it learns which bond
#      it speaks for (the outpoint does not exist until the funding transaction lands)
#   5. verify every bond reads Active
#
# **Bond amounts are deliberately identical.** The paper's weight is
# `W_i(E) = min{C_i(E), lambda * B_i(E)}`, so an unequal bond would cap some validators and turn
# the first weighted-quorum test into a test of the cap instead. With `lambda = 1e8 uRTE/KAS` the
# cap in uRTE equals the bond in sompi, so --amount 20 (KAS) gives every validator a 2000-VLT
# ceiling against a maximum of 400 VLT of compute — the cap never binds, and the effective weight
# IS the compute weight. Testing the cap is a separate experiment, on purpose.
#
# Usage:
#   scripts/misaka-vlt-devnet-bond.sh [--amount KAS] [--nodes N]
#
# Env: MISAKA_DEVNET_DIR, KASPAD_BIN, MISAMINER_BIN, VALIDATOR_BIN

set -euo pipefail

NODES=5
AMOUNT_KAS=20

while [ $# -gt 0 ]; do
  case "$1" in
    --amount) AMOUNT_KAS="$2"; shift 2 ;;
    --nodes)  NODES="$2"; shift 2 ;;
    -h|--help) sed -n '2,28p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
WORK_DIR="${MISAKA_DEVNET_DIR:-$REPO_ROOT/.misaka-vlt-devnet}"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
MISAMINER_BIN="${MISAMINER_BIN:-$REPO_ROOT/target/release/misaminer}"
VALIDATOR_BIN="${VALIDATOR_BIN:-$REPO_ROOT/target/release/kaspa-pq-validator}"

for bin in "$KASPAD_BIN" "$MISAMINER_BIN" "$VALIDATOR_BIN"; do
  [ -x "$bin" ] || { echo "missing binary: $bin" >&2; exit 1; }
done
[ -d "$WORK_DIR/node-0" ] || { echo "no devnet at $WORK_DIR — run misaka-vlt-devnet.sh first" >&2; exit 1; }

BASE_RPC="${MISAKA_DEVNET_BASE_RPC:-17110}"
grpc_of() { echo $((BASE_RPC + $1 * 10)); }
wrpc_of() { echo $((BASE_RPC + $1 * 10 + 2)); }

# The funding address is derived from the validator key and logged at startup. Reading it back
# rather than re-deriving it means this script and the node cannot disagree about where to send.
funding_addr_of() {
  grep -oE "funding address: [a-z0-9:]+" "$WORK_DIR/node-$1/kaspad.log" | tail -1 | awk '{print $3}'
}

# Mine `count` blocks paying `addr`. The miner exits on its own at --blocks.
mine_to() {
  local addr="$1" count="$2"
  "$MISAMINER_BIN" --rpc="127.0.0.1:$(grpc_of 0)" --network-id=devnet --wallet="$addr" \
    --threads=2 --blocks="$count" --min-block-interval-ms=0 >>"$WORK_DIR/bond-miner.log" 2>&1 || true
}

echo "MISAKA VLT devnet — bonding $NODES validators at $AMOUNT_KAS KAS each"
echo

declare -a ADDRS=()
for i in $(seq 0 $((NODES - 1))); do
  addr=$(funding_addr_of "$i")
  [ -n "$addr" ] || { echo "node-$i has not logged a funding address yet (is it running with --validator-key?)" >&2; exit 1; }
  ADDRS+=("$addr")
  echo "node-$i funding: $addr"
done
echo

# Fund every validator BEFORE waiting on maturity: coinbase maturity is measured in chain blocks,
# so five sequential waits would cost five times as long as one shared wait afterwards.
# ~3.7 KAS of subsidy per devnet block, of which the miner keeps the worker share, so ask for
# generous headroom rather than computing a tight number that a split change would invalidate.
BLOCKS_PER_VALIDATOR=$(( (AMOUNT_KAS * 100) / 100 + 20 ))
for i in $(seq 0 $((NODES - 1))); do
  echo "mining $BLOCKS_PER_VALIDATOR blocks to node-$i ..."
  mine_to "${ADDRS[$i]}" "$BLOCKS_PER_VALIDATOR"
done

# Coinbase is unspendable until `coinbase_maturity` blocks pass, and the bond command only spends
# MATURE UTXOs. Rather than hardcode the maturity (devnet's is 1000 blocks, which is not obvious
# and is a per-network constant this script has no business restating), mine in chunks and retry
# the bond until it succeeds. That self-tunes to whatever the network actually requires, and it
# fails with the validator's own diagnostic if it is something other than maturity.
try_bond() {
  "$VALIDATOR_BIN" bond \
    --node-wrpc-borsh="127.0.0.1:$(wrpc_of "$1")" \
    --validator-key="$WORK_DIR/node-$1/validator.key" \
    --amount="${AMOUNT_KAS}KAS" 2>&1 | tee -a "$WORK_DIR/bond.log" |
    grep -oE "bond_outpoint: [0-9a-f]+:[0-9]+" | awk '{print $2}'
}

echo
declare -a OUTPOINTS=()
for i in $(seq 0 $((NODES - 1))); do
  echo "bonding node-$i ..."
  out=""
  for attempt in $(seq 1 12); do
    # `|| true` on the assignment, and a real conditional for the break. Under `set -euo pipefail`
    # BOTH would otherwise kill the script on the first miss: the validator exits non-zero when the
    # funding is not yet mature (pipefail propagates it through the pipeline into the command
    # substitution), and `[ ... ] && break` returns non-zero when the test fails. A retry loop that
    # cannot survive a failure is not a retry loop.
    out=$(try_bond "$i") || true
    if [ -n "$out" ]; then break; fi
    echo "  attempt $attempt: funding not mature yet; mining 200 more blocks"
    "$MISAMINER_BIN" --rpc="127.0.0.1:$(grpc_of 0)" --network-id=devnet --allow-burn --mine-when-not-synced --threads=2 \
      --blocks=200 --min-block-interval-ms=0 >>"$WORK_DIR/bond-miner.log" 2>&1 || true
  done
  if [ -z "$out" ]; then
    echo "node-$i: bond failed after mining past maturity — see $WORK_DIR/bond.log" >&2
    tail -3 "$WORK_DIR/bond.log" >&2
    exit 1
  fi
  OUTPOINTS+=("$out")
  echo "  bond_outpoint: $out"
done

# The bond tx has to be accepted before the node can resolve the outpoint, so advance the chain
# again before the restart. A node restarted onto an unaccepted bond logs "bond=unconfigured" and
# looks like a bonding failure that is really just a race.
echo
echo "advancing the chain so the bond transactions are accepted ..."
"$MISAMINER_BIN" --rpc="127.0.0.1:$(grpc_of 0)" --network-id=devnet --allow-burn --mine-when-not-synced --threads=2 \
  --blocks=30 --min-block-interval-ms=0 >>"$WORK_DIR/bond-miner.log" 2>&1 || true

echo
for i in $(seq 0 $((NODES - 1))); do
  node_dir="$WORK_DIR/node-$i"
  echo "restarting node-$i with --stake-bond=${OUTPOINTS[$i]} ..."
  kill "$(cat "$node_dir/kaspad.pid")" 2>/dev/null || true
done
sleep 5

for i in $(seq 0 $((NODES - 1))); do
  node_dir="$WORK_DIR/node-$i"
  # Re-run the exact argv the devnet script recorded, plus the bond it now owns. Rebuilding the
  # arguments here instead would be a second definition of the node's configuration.
  #
  # Read with a while-read loop, not `mapfile`: macOS ships bash 3.2, where `mapfile` does not
  # exist — and the failure lands AFTER the nodes have been killed, so the devnet is left down.
  #
  # Any --stake-bond already in the file is dropped rather than kept, so re-running this script
  # replaces the bond instead of passing two.
  saved=()
  while IFS= read -r tok; do
    case "$tok" in
      ''|--stake-bond=*) ;;
      *) saved+=("$tok") ;;
    esac
  done < <(tr ' ' '\n' < "$node_dir/run.args" | sed "s/^'//;s/'$//")
  saved+=("--stake-bond=${OUTPOINTS[$i]}")
  # Record it too. The outpoint does not exist until the funding transaction lands, so this file is
  # the only place the node's full configuration is written down — and the devnet script carries
  # the flag forward from here when it regenerates the argv. Without this, restarting the devnet to
  # change any other flag silently drops every validator out of the active set, and rebonding costs
  # a thousand blocks of coinbase maturity to get back.
  printf '  %q' "${saved[@]}" > "$node_dir/run.args"
  ( "${saved[@]}" >>"$node_dir/kaspad.log" 2>&1 & echo $! > "$node_dir/kaspad.pid" )
done

echo
echo "waiting for the bonds to read Active ..."
sleep 40
active=0
for i in $(seq 0 $((NODES - 1))); do
  line=$(grep -oE "bond=[a-zA-Z]+" "$WORK_DIR/node-$i/kaspad.log" | tail -1)
  echo "node-$i ${line:-bond=unknown}"
  [ "$line" = "bond=Active" ] && active=$((active + 1))
done

echo
if [ "$active" -eq "$NODES" ]; then
  echo "all $NODES validators bonded and Active."
else
  echo "$active/$NODES active — the rest may still be waiting for their bond tx to be accepted." >&2
  echo "keep the miner running and re-check with: grep -o 'bond=[a-zA-Z]*' $WORK_DIR/node-*/kaspad.log | tail -5" >&2
  exit 1
fi
