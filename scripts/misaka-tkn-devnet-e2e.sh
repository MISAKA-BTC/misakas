#!/usr/bin/env bash
# MISAKA — Compute Token Program (TOK) end-to-end devnet: emission, transfers, burns,
# void classes, and the shadow fence, on top of a live VLT compute devnet.
#
# What this drives (docs/misaka-compute-token-program-design-v0.1.md §9.2, the PR 2 wiring):
#
#   Phase 1  a five-node VLT devnet with bonds and fixture compute — the standard
#            misaka-vlt-devnet.sh + misaka-vlt-devnet-bond.sh flow, unchanged. TOK settles
#            over FINALIZED VLT credit epochs, so a token devnet is first a compute devnet.
#   Phase 2  once the weight fence is up, restart every node with --tkn-devnet: the token
#            shadow fence opens ~100 DAA later, the ledger fold + emission ~400 DAA later,
#            and node-0 carries a scripted fixture-op plan that exercises every ledger path:
#
#              amount=4000   nonce=1  in [shadow,active)  -> must stay void FOREVER
#              amount=5000   nonce=1  post-active         -> binds (first real transfer)
#              amount=9000   nonce=1  post-active         -> void (nonce replay)
#              amount=11000  nonce=2  post-active         -> binds
#              amount=10^15  nonce=3  post-active         -> void (overdraft)
#              burn=3000     nonce=3  post-active         -> binds (3 was NOT consumed by the
#                                                            void overdraft — that is the point)
#
#            The op DAAs sit ~800+ past the active fence so the first emission settlements
#            (D_settle epochs behind the wall clock) land BEFORE the first debit folds.
#
# Then scripts/misaka-tkn-devnet-verify.sh asserts the whole story from the kaspad logs alone.
#
# Build kaspad WITH `--features "evm,devnet-vlt-fixture"` (same as the VLT devnet).
#
# Usage:
#   scripts/misaka-tkn-devnet-e2e.sh [--nodes N] [--skip-phase1] [--wait SECONDS] [--no-verify]
#
# Env:
#   MISAKA_DEVNET_DIR       working directory (default: ./.misaka-tkn-devnet)
#   KASPAD_BIN              kaspad binary (default: ./target/release/kaspad)
#   MISAMINER_BIN           miner binary  (default: ./target/release/misaminer)
#   MISAKA_DEVNET_BASE_P2P  default 27111 — off the VLT devnet's 17111 so both can coexist
#   MISAKA_DEVNET_BASE_RPC  default 27110

set -euo pipefail

NODES=5
SKIP_PHASE1=0
WAIT_SECS=1800
RUN_VERIFY=1

while [ $# -gt 0 ]; do
  case "$1" in
    --nodes)       NODES="$2"; shift 2 ;;
    --skip-phase1) SKIP_PHASE1=1; shift ;;
    --wait)        WAIT_SECS="$2"; shift 2 ;;
    --no-verify)   RUN_VERIFY=0; shift ;;
    -h|--help)     sed -n '2,38p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
WORK_DIR="${MISAKA_DEVNET_DIR:-$REPO_ROOT/.misaka-tkn-devnet}"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
MISAMINER_BIN="${MISAMINER_BIN:-$REPO_ROOT/target/release/misaminer}"
BASE_P2P="${MISAKA_DEVNET_BASE_P2P:-27111}"
BASE_RPC="${MISAKA_DEVNET_BASE_RPC:-27110}"
export MISAKA_DEVNET_DIR="$WORK_DIR" MISAKA_DEVNET_BASE_P2P="$BASE_P2P" MISAKA_DEVNET_BASE_RPC="$BASE_RPC"

# Fixture-op amounts double as identifiers: the verify script finds each op's txid by its
# amount, so every amount in the plan MUST be unique.
AMT_SHADOW=4000
AMT_LIVE1=5000
AMT_REPLAY=9000
AMT_LIVE2=11000
AMT_OVERDRAFT=1000000000000000
AMT_BURN=3000

wait_for_log() { # file regex timeout_secs what
  local file="$1" regex="$2" timeout="$3" what="$4"
  local waited=0
  until grep -qE "$regex" "$file" 2>/dev/null; do
    sleep 5
    waited=$((waited + 5))
    if [ "$waited" -ge "$timeout" ]; then
      echo "timed out after ${timeout}s waiting for: $what" >&2
      echo "  (pattern '$regex' in $file)" >&2
      exit 1
    fi
  done
}

current_daa() { # from node-0's log: the newest daa= any overlay line reported
  { grep -oE '(sink_)?daa=[0-9]+' "$WORK_DIR/node-0/kaspad.log" || true; } | tail -1 | grep -oE '[0-9]+'
}

# ---- Phase 1: a bonded, computing VLT devnet ---------------------------------------------------
if [ "$SKIP_PHASE1" -eq 0 ]; then
  echo "== Phase 1: VLT devnet ($NODES nodes, ports p2p=$BASE_P2P rpc=$BASE_RPC) =="
  "$REPO_ROOT/scripts/misaka-vlt-devnet.sh" --nodes "$NODES"
  echo
  echo "== Phase 1: bonding every validator (mines through coinbase maturity — takes a while) =="
  # The bond script exits non-zero when some bonds are still awaiting acceptance at its final
  # check — a soft outcome its own output says to wait through, so do exactly that here rather
  # than dying: the authoritative signal is every node's heartbeat reaching active_validators=N.
  "$REPO_ROOT/scripts/misaka-vlt-devnet-bond.sh" || true
  echo
  echo "== Phase 1: waiting for all $NODES bonds to read Active =="
  wait_for_log "$WORK_DIR/node-0/kaspad.log" "active_validators=$NODES" "$WAIT_SECS" "all $NODES bonds Active"
else
  [ -d "$WORK_DIR/node-0" ] || { echo "--skip-phase1 but no devnet at $WORK_DIR" >&2; exit 1; }
fi

echo
echo "== Phase 1: waiting for the VLT weight fence (compute becomes the vote) =="
wait_for_log "$WORK_DIR/node-0/kaspad.log" '\[vlt-weight-fence-reached\]' "$WAIT_SECS" "the VLT weight fence"
echo "weight fence reached; letting credits finalize for 60s"
sleep 60

# ---- Phase 2: restart with the token fences + node-0's fixture-op plan -------------------------
daa_now=$(current_daa)
[ -n "$daa_now" ] || { echo "could not read a current DAA from node-0's log" >&2; exit 1; }
TKN_ACTIVE=$((daa_now + 400))
SHADOW_SPAN=300

# The recipient of every transfer is node-1's overlay identity, printed at key load.
NODE1_ID=$({ grep -oE 'validator_id=[0-9a-f]+' "$WORK_DIR/node-1/kaspad.log" || true; } | head -1 | cut -d= -f2)
[ -n "$NODE1_ID" ] || { echo "could not read node-1's validator_id from its log" >&2; exit 1; }

echo
echo "== Phase 2: token fences =="
echo "  daa now        : $daa_now"
echo "  shadow fence   : $((TKN_ACTIVE - SHADOW_SPAN))"
echo "  active fence   : $TKN_ACTIVE"
echo "  emission       : 1000 TOK/epoch, settlement D_settle behind the wall clock"
echo "  recipient      : node-1 = $NODE1_ID"

for i in $(seq 0 $((NODES - 1))); do
  node_dir="$WORK_DIR/node-$i"
  args_file="$node_dir/run.args"
  [ -f "$args_file" ] || { echo "missing $args_file" >&2; exit 1; }
  # Idempotent re-run: strip any token flags a previous phase-2 appended, then re-append.
  cleaned=$(tr ' ' '\n' < "$args_file" | grep -vE "^'?--tkn-" | tr '\n' ' ')
  # Keep the compute fixture producing THROUGH the token phase: emission settles over live
  # credits, and a quota exhausted before the active fence would settle nothing but zeros.
  cleaned=$(printf '%s' "$cleaned" | sed -E "s/--compute-fixture-job-limit=[0-9]+/--compute-fixture-job-limit=200/")
  extra=" --tkn-devnet=$TKN_ACTIVE --tkn-devnet-shadow-span=$SHADOW_SPAN --tkn-devnet-epoch-budget-tok=1000"
  if [ "$i" -eq 0 ]; then
    extra+=" --tkn-fixture-transfer=$NODE1_ID:$AMT_SHADOW:1:$((TKN_ACTIVE - 150))"
    extra+=" --tkn-fixture-transfer=$NODE1_ID:$AMT_LIVE1:1:$((TKN_ACTIVE + 800))"
    extra+=" --tkn-fixture-transfer=$NODE1_ID:$AMT_REPLAY:1:$((TKN_ACTIVE + 900))"
    extra+=" --tkn-fixture-transfer=$NODE1_ID:$AMT_LIVE2:2:$((TKN_ACTIVE + 1000))"
    extra+=" --tkn-fixture-transfer=$NODE1_ID:$AMT_OVERDRAFT:3:$((TKN_ACTIVE + 1100))"
    extra+=" --tkn-fixture-burn=$AMT_BURN:3:$((TKN_ACTIVE + 1200))"
  fi
  printf '%s%s' "$cleaned" "$extra" > "$args_file"
done

echo "restarting all $NODES node(s) with the token flags ..."
for i in $(seq 0 $((NODES - 1))); do
  pidfile="$WORK_DIR/node-$i/kaspad.pid"
  if [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then
    kill "$(cat "$pidfile")" 2>/dev/null || true
  fi
done
sleep 5
for i in $(seq 0 $((NODES - 1))); do
  node_dir="$WORK_DIR/node-$i"
  ( eval "exec $(cat "$node_dir/run.args")" >>"$node_dir/kaspad.log" 2>&1 & echo $! > "$node_dir/kaspad.pid" )
done

# The miner dies with its node's connection; restart it against node-0.
if [ -f "$WORK_DIR/miner.pid" ] && kill -0 "$(cat "$WORK_DIR/miner.pid")" 2>/dev/null; then
  kill "$(cat "$WORK_DIR/miner.pid")" 2>/dev/null || true
fi
for _ in $(seq 1 60); do
  if (exec 3<>"/dev/tcp/127.0.0.1/$BASE_RPC") 2>/dev/null; then exec 3<&- 3>&-; break; fi
  sleep 1
done
"$MISAMINER_BIN" --rpc="127.0.0.1:$BASE_RPC" --network-id=devnet --allow-burn --mine-when-not-synced --threads=2 \
  >>"$WORK_DIR/miner.log" 2>&1 &
echo $! > "$WORK_DIR/miner.pid"

sleep 8
dead=0
for i in $(seq 0 $((NODES - 1))); do
  if ! kill -0 "$(cat "$WORK_DIR/node-$i/kaspad.pid")" 2>/dev/null; then
    dead=$((dead + 1))
    echo "node-$i EXITED after the token restart. Last lines:" >&2
    tail -5 "$WORK_DIR/node-$i/kaspad.log" >&2
  fi
done
[ "$dead" -eq 0 ] || { echo "$dead node(s) down" >&2; exit 1; }

echo
echo "Phase 2 running. Watch it with:"
echo "  tail -f $WORK_DIR/node-0/kaspad.log | grep -E '\[token|token fixture'"
echo

if [ "$RUN_VERIFY" -eq 1 ]; then
  exec "$REPO_ROOT/scripts/misaka-tkn-devnet-verify.sh" --nodes "$NODES" --wait "$WAIT_SECS" --restart-check
fi
