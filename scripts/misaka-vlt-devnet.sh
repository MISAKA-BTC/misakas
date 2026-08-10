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
# Build kaspad WITH `--features "evm,devnet-vlt-fixture"`. `evm` because devnet has the EVM lane
# active from DAA 0, so a kaspad without it panics the moment a miner asks for a block template —
# the chain then never advances and no fence is ever crossed. `devnet-vlt-fixture` because without
# a 24 GB model on disk it is the only runtime that can claim this network's registered compute
# profile; without it every node runs verifier-only and W(E) stays at zero forever.
#
# **Asymmetric by construction.** Each node gets a job quota, and the fixture prices every job at
# exactly 50 VLT, so --job-quotas 8,5,3,2,2 is a plan of 400/250/150/100/100 VLT. Equal quotas
# would give five validators identical weight, which tests that the overlay runs but not that it
# weights — the one property the whole exercise is for.
#
# The plan lands EXACTLY because the devnet pins the credit decay flat (--vlt-devnet-flat-decay):
# under production decay (0.97/epoch) a validator that finished its quota earlier would out-decay
# one that finished late, and 8/5/3/2/2 jobs would land near — but not at — 400/250/150/100/100.
# Pass --real-decay to run the production curve instead (a decay experiment, not the weight plan).
#
# Flat decay does NOT stop the credit WINDOW from sliding: C_i(E) sums only the last K epochs, so
# the full plan is frozen only while every job is simultaneously inside one window. The measured
# fixture cadence is ~2.5-3 epochs per job (commit -> next-epoch beacon -> certificate), so the
# largest quota spans ~quota x 3 epochs and the whole plan needs
#     K >= max_quota x 3 + challenge maturity (~3 epochs) + margin
# — for 8/5/3/2/2 that is --epochs 32. The K=8 default is a fast OVERLAY soak, not a plan run: it
# proves crediting/committees/verdicts in minutes, and its W(E) will slide back down by design.
#
# Re-running this script on an existing devnet is safe: it stops the recorded pids first, keeps
# each node's validator key, and carries any --stake-bond a previous bond run recorded, so a
# bonded devnet can be restarted with new flags without redoing the bond.
#
# Usage:
#   scripts/misaka-vlt-devnet.sh [--shadow-only] [--no-mine] [--nodes N] [--shadow-daa N]
#                                [--epochs K] [--job-quotas N,N,...] [--real-decay]
#
# Env:
#   MISAKA_DEVNET_DIR   working directory (default: ./.misaka-vlt-devnet)
#   KASPAD_BIN          kaspad binary (default: ./target/release/kaspad)
#   PALW_WORKER         palw-worker binary; without it the devnet VLT fixture runs instead
#   MISAMINER_BIN       miner binary (default: ./target/release/misaminer); --no-mine skips it

set -euo pipefail

NODES=5
SHADOW_DAA=200
CREDIT_WINDOW_EPOCHS=8
SHADOW_ONLY=0
MINE=1
JOB_QUOTAS=8,5,3,2,2
FLAT_DECAY=1

while [ $# -gt 0 ]; do
  case "$1" in
    --shadow-only) SHADOW_ONLY=1; shift ;;
    --no-mine)     MINE=0; shift ;;
    --nodes)       NODES="$2"; shift 2 ;;
    --shadow-daa)  SHADOW_DAA="$2"; shift 2 ;;
    --epochs)      CREDIT_WINDOW_EPOCHS="$2"; shift 2 ;;
    --job-quotas)  JOB_QUOTAS="$2"; shift 2 ;;
    --real-decay)  FLAT_DECAY=0; shift ;;
    -h|--help)     sed -n '2,42p' "$0"; exit 0 ;;
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
  echo "  cargo build --release --bin kaspad --features \"evm,devnet-vlt-fixture\"" >&2
  exit 1
fi

if [ "$NODES" -lt 4 ]; then
  # 1 executor + min_verifier_confirmations (3) is the floor at which any job can be credited at
  # all. Below it the overlay runs and every certificate sits at zero forever, which looks like a
  # bug and is not one.
  echo "warning: $NODES nodes is below 1 executor + 3 confirmations; no job will ever be creditable" >&2
fi

# One quota per node, and refused rather than padded if the counts disagree. A missing entry
# silently defaulted would hand some validator a weight nobody chose, and the resulting plot would
# be indistinguishable from a real result.
QUOTAS=()
IFS=, read -r -a QUOTAS <<< "$JOB_QUOTAS"
if [ "${#QUOTAS[@]}" -ne "$NODES" ]; then
  echo "--job-quotas has ${#QUOTAS[@]} entries but there are $NODES nodes; pass one quota per node" >&2
  exit 2
fi
for q in "${QUOTAS[@]}"; do
  case "$q" in
    ''|*[!0-9]*) echo "--job-quotas entry '$q' is not a job count" >&2; exit 2 ;;
  esac
done

mkdir -p "$WORK_DIR"
echo "MISAKA VLT devnet"
echo "  nodes           : $NODES"
echo "  shadow fence    : DAA $SHADOW_DAA"
if [ "$SHADOW_ONLY" -eq 1 ]; then
  echo "  weight fence    : DORMANT (Shadow Mode — finality stays on bonded stake)"
else
  echo "  weight fence    : one credit window (K=$CREDIT_WINDOW_EPOCHS) above the shadow fence"
fi
if [ -n "$PALW_WORKER" ]; then
  echo "  compute         : palw-worker at $PALW_WORKER (no quota — a real-model devnet is a different experiment)"
else
  echo "  compute         : devnet VLT fixture, 50 VLT/job, quotas $JOB_QUOTAS"
fi
echo "  work dir        : $WORK_DIR"
echo

# Overridable, because the ports are the only thing stopping two devnets from coexisting. A second
# mesh started while the first holds 17110 dies on bind — every node, immediately — and the bond
# script that follows it then runs against nothing. Both failures are loud in a log nobody is
# tailing yet, so the run looks like it started.
BASE_P2P="${MISAKA_DEVNET_BASE_P2P:-17111}"
BASE_RPC="${MISAKA_DEVNET_BASE_RPC:-17110}"

# EVERY refusal below this point must come BEFORE the running nodes are stopped. A check that
# rejects the configuration after the kill loop leaves the devnet down as its side effect, and the
# operator is left worse off than if the script had never run.
#
# The one devnet mistake a restart cannot undo: opening the weight fence on an empty credit table.
#
# Past the fence the vote is VLT-weighted, so W(E) = 0 means no epoch reaches quorum, no anchor is
# DNS-confirmed, and the anchors that were confirmed before the fence slide out of the credit window
# one by one. Credit needs an anchor, an anchor needs quorum, and quorum needs credit — the overlay
# cannot climb out, and the only symptom is `[vlt-finality-inactive] reason=zero_total_weight` while
# the chain keeps advancing perfectly well on PoW.
#
# It is easy to walk into, because bonding is what fills the soak: `misaka-vlt-devnet-bond.sh` mines
# past a thousand-block coinbase maturity, and until it finishes no node has an Active bond, so no
# node originates a job. Start the fence at the same DAA twice and the entire soak is spent bonding.
#
# Both numbers come from the previous run — the fence kaspad itself reported, and the `--vlt-devnet`
# it was given — so the span is measured rather than restated here.
if [ -f "$WORK_DIR/node-0/kaspad.log" ] && [ "$SHADOW_ONLY" -eq 0 ]; then
  prior=$({ grep -oE '\[vlt-weight-fence-reached\] daa=[0-9]+ fence=[0-9]+' "$WORK_DIR/node-0/kaspad.log" || true; } | tail -1)
  prior_shadow=$({ tr ' ' '\n' < "$WORK_DIR/node-0/run.args" 2>/dev/null | sed "s/^'//;s/'$//" |
    { grep -oE '^--vlt-devnet=[0-9]+' || true; }; } | tail -1)
  if [ -n "$prior" ] && [ -n "$prior_shadow" ]; then
    seen_daa=${prior#*daa=}; seen_daa=${seen_daa%% *}
    prior_fence=${prior##*fence=}
    span=$((prior_fence - ${prior_shadow#--vlt-devnet=}))
    if [ $((SHADOW_DAA + span)) -le "$seen_daa" ]; then
      echo >&2
      echo "refusing to start: --shadow-daa $SHADOW_DAA puts the weight fence at $((SHADOW_DAA + span)), and this" >&2
      echo "chain is already at DAA $seen_daa. The vote would move onto verified compute with an empty" >&2
      echo "credit table, and the overlay cannot recover from that — see the comment above this check." >&2
      echo >&2
      echo "Give the soak somewhere to happen:" >&2
      echo "  $0 --shadow-daa $((seen_daa + 100))        # fence lands at $((seen_daa + 100 + span)), with compute running throughout" >&2
      echo "  $0 --shadow-only                    # or keep the fence dormant and watch [vlt-shadow] first" >&2
      exit 2
    fi
  fi
fi

# Stop anything this script started before, so it can be re-run to change flags. Without it the new
# nodes lose the port race, die on bind, and the script reports five exits with no hint that the
# old mesh is still up and holding the ports.
#
# The MINER counts as "anything". Stopping only the nodes leaves the old miner attached to node-0's
# port and starts a second one beside it, so every re-run adds a miner. Three of them raced a devnet
# to DAA 30279 while its certificates sat at ~3000, putting every one of them outside the
# 1250-blue-score credit window — the evidence for the bug under investigation, mined into
# unreachability by the script meant to be investigating it.
stopped=0
for i in $(seq 0 $((NODES - 1))); do
  pidfile="$WORK_DIR/node-$i/kaspad.pid"
  if [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then
    kill "$(cat "$pidfile")" 2>/dev/null || true
    stopped=$((stopped + 1))
  fi
done
if [ -f "$WORK_DIR/miner.pid" ] && kill -0 "$(cat "$WORK_DIR/miner.pid")" 2>/dev/null; then
  kill "$(cat "$WORK_DIR/miner.pid")" 2>/dev/null || true
  echo "stopped the miner from a previous run"
fi
# Belt and braces: a miner whose pid file was lost (an interrupted run, a hand-started one) is
# invisible to the check above and would keep mining beside the new one.
pkill -f "misaminer --rpc=127.0.0.1:$BASE_RPC " 2>/dev/null || true
if [ "$stopped" -gt 0 ]; then
  echo "stopped $stopped node(s) from a previous run; restarting them with the current flags"
  sleep 5
fi

# A bond outpoint does not exist until its funding transaction lands, so it is discovered by
# `misaka-vlt-devnet-bond.sh` and recorded in run.args. Carry it across a regeneration: rebonding a
# devnet costs a thousand blocks of coinbase maturity, and losing the flag silently drops the node
# out of the active set — which reads as a consensus problem, not a missing argument.
#
# `|| true` on the grep, and not because it is tidy: under `set -o pipefail` a grep that matches
# nothing makes the whole pipeline non-zero, that status becomes the function's, and `set -e` then
# kills the script at the *assignment* — on the ordinary path where no bond exists yet.
saved_bond_of() {
  [ -f "$WORK_DIR/node-$1/run.args" ] || return 0
  tr ' ' '\n' < "$WORK_DIR/node-$1/run.args" | sed "s/^'//;s/'$//" | { grep -E '^--stake-bond=' || true; } | tail -1
}

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

  # The job input. Its CONTENT does not price anything — the fixture runs one fixed shape (10
  # prefill + 5 decode = 50 VLT) whatever it reads — so this exists to give each node a job of its
  # own, not to influence any weight. Written once and then left alone: its LENGTH is part of the
  # quota's plan id, so editing it to a different length is a different plan and resets the count.
  if [ ! -f "$node_dir/compute-prompt.txt" ]; then
    printf 'MISAKA VLT devnet fixture job — node-%s\n' "$i" > "$node_dir/compute-prompt.txt"
  fi

  # Every node's own address is in the peer list; the self-dial fails harmlessly, and keeping the
  # list identical across nodes means one mesh definition rather than N-1 bespoke ones.
  args=(
    --devnet
    --appdir="$node_dir"
    --listen="127.0.0.1:$p2p"
    --rpclisten="127.0.0.1:$rpc"
    # wRPC (borsh) as well as gRPC: the miner speaks gRPC, but `kaspa-pq-validator` and
    # `misaka validator status` — the bond bootstrap and the VLT gauge read — speak wRPC.
    --rpclisten-borsh="127.0.0.1:$((rpc + 2))"
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
  [ "$FLAT_DECAY" -eq 1 ] && args+=(--vlt-devnet-flat-decay)
  # `--compute-prompt` is what makes a node an EXECUTOR; without it the compute role audits peers
  # and originates nothing, so a devnet where no node has one produces no VLT at all and every
  # weight stays at zero. That applies to the real worker as much as to the fixture.
  args+=(--enable-compute --compute-work-dir="$node_dir/compute" --compute-prompt="$node_dir/compute-prompt.txt")
  if [ -n "$PALW_WORKER" ]; then
    args+=(--compute-worker="$PALW_WORKER")
  else
    # Fixture only. The quota is what makes the weights ASYMMETRIC — without it every node
    # originates forever and the five converge, which measures nothing.
    args+=(--compute-fixture-job-limit="${QUOTAS[$i]}")
  fi
  bond=$(saved_bond_of "$i")
  if [ -n "$bond" ]; then
    args+=("$bond")
  fi
  args+=("${peers[@]}")

  if [ -n "$PALW_WORKER" ]; then
    echo "node-$i  p2p=$p2p  grpc=$rpc  wrpc=$((rpc + 2))${bond:+  ${bond#--stake-bond=}}"
  else
    echo "node-$i  p2p=$p2p  grpc=$rpc  wrpc=$((rpc + 2))  quota=${QUOTAS[$i]} job(s) = $(( ${QUOTAS[$i]} * 50 )) VLT${bond:+  ${bond#--stake-bond=}}"
  fi
  printf '  %q' "$KASPAD_BIN" "${args[@]}" > "$node_dir/run.args"
  # APPEND, never truncate. The log is the only record of which job ids a run produced, and a
  # restart is exactly when an investigation most needs the run before it. Truncating here erased
  # the certificate and commitment ids of twenty fixture jobs mid-diagnosis.
  ( "$KASPAD_BIN" "${args[@]}" >>"$node_dir/kaspad.log" 2>&1 & echo $! > "$node_dir/kaspad.pid" )
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
      >>"$WORK_DIR/miner.log" 2>&1 &
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

# The compute role resolves at startup and disables itself, with one log line, whenever the runtime
# cannot claim the consensus-registered profile — which is what a kaspad built without
# `--features devnet-vlt-fixture` does here. Every node then stays up, mines, gossips and attests,
# and credits nothing forever. Checking for the role's own "enabled" line turns that into an error
# now rather than an unexplained W(E) = 0 an hour into the run.
enabled=0
for i in $(seq 0 $((NODES - 1))); do
  if grep -q "validator-compute] enabled:" "$WORK_DIR/node-$i/kaspad.log"; then
    enabled=$((enabled + 1))
  fi
done
if [ "$enabled" -ne "$NODES" ]; then
  echo >&2
  echo "the compute role is active on only $enabled/$NODES nodes; no VLT will be credited. node-0 says:" >&2
  grep -E "validator-compute]" "$WORK_DIR/node-0/kaspad.log" | tail -3 >&2
  if [ -z "$PALW_WORKER" ]; then
    echo >&2
    echo "If that names the mock runtime, this kaspad was built without the fixture. Rebuild with:" >&2
    echo "  cargo build --release --bin kaspad --features \"evm,devnet-vlt-fixture\"" >&2
  fi
  exit 1
fi
echo
echo "compute     : role active on all $NODES nodes"

echo
echo "Started. Watch the overlay come up with:"
echo "  tail -f $WORK_DIR/node-0/kaspad.log | grep -E 'vlt-shadow|stake-score|precommit|validator-compute'"
echo
echo "Each node still needs a funded stake bond before it attests, and the bond outpoint is only"
echo "knowable after the funding transaction lands — so run scripts/misaka-vlt-devnet-bond.sh,"
echo "which bonds every validator and restarts it with its own --stake-bond (see ADR-0010). Until"
echo "then the nodes mine and gossip but produce no attestations, and the overlay stays at"
echo "W(E) = 0. Re-running THIS script afterwards keeps each bond."
echo "Verify the 8/5/3/2/2 weight plan, cross-node root equality and restart persistence with:"
echo "  scripts/misaka-vlt-devnet-verify.sh            # once bonds are Active and quotas are filling"
echo "Stop everything with:"
echo "  for p in $WORK_DIR/*/kaspad.pid $WORK_DIR/miner.pid; do kill \"\$(cat \"\$p\")\" 2>/dev/null; done"
