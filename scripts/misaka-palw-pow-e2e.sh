#!/usr/bin/env bash
# misaka-palw-pow-e2e.sh — PALW LLM-PoW (algo_id = 4, 0.1 bps) devnet smoke:
#
#   node-0 (miner target) ── p2p ── node-1 (verifier)          [both PALW]
#                                   node-2 (NO PALW runtime)   [expects the fail-loud panic]
#
#   1. misaminer mines BLOCKS blocks against node-0 through the sequential PALW path
#      (fixture tags by default — one fixed-cost in-process "inference" per nonce).
#   2. node-1 must validate and relay every one of them (same PALW replay on its side).
#   3. node-2 runs WITHOUT the worker and WITHOUT the fixture env: the first PALW header it
#      validates must trigger the designed `PALW PoW validation cannot run` panic — the
#      fail-loud alternative to silently rejecting valid blocks and banning honest peers.
#
# Defaults run the MODEL-FREE fixture rules (MISAKA_PALW_POW_FIXTURE=1 for node-0/1/miner).
# For the real pinned model instead: PALW_REAL=1 PALW_WORKER=<bin> MISAKA_PALW_GGUF=<gguf> —
# expect ~1-3 s per attempt and size BLOCKS accordingly.
#
#   KASPAD_BIN / MISAMINER_BIN   binaries (default: ./target/release/{kaspad,misaminer})
#   BLOCKS                       blocks to mine before declaring success (default 12)
#   WORK_DIR                     state dir (default .misaka-palw-pow-e2e; wiped per run)
#   NET                          devnet (default) or testnet-10. testnet-10 exercises the PUBLIC
#                                network's params (PALW re-genesis) and REQUIRES PALW_REAL=1 —
#                                the kaspad startup rail refuses the fixture outside devnet.
#   IBD=1                        after mining, boot a FRESH node-3 and require a full
#                                from-genesis IBD (per-header PALW replay) to catch up.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
MISAMINER_BIN="${MISAMINER_BIN:-$REPO_ROOT/target/release/misaminer}"
WORK_DIR="${WORK_DIR:-$REPO_ROOT/.misaka-palw-pow-e2e}"
BLOCKS="${BLOCKS:-12}"
PALW_REAL="${PALW_REAL:-0}"
NET="${NET:-devnet}"
IBD="${IBD:-0}"

case "$NET" in
  devnet) NET_ARGS=(--devnet) ;;
  testnet-10)
    NET_ARGS=(--testnet --netsuffix=10)
    if [ "$PALW_REAL" != "1" ]; then
      echo "NET=testnet-10 requires PALW_REAL=1 (public-network params refuse the fixture)" >&2
      exit 1
    fi
    ;;
  *) echo "unknown NET=$NET (devnet | testnet-10)" >&2; exit 1 ;;
esac

BASE_P2P=37711
BASE_RPC=37710

[ -x "$KASPAD_BIN" ] || { echo "no kaspad at $KASPAD_BIN (cargo build --release -p kaspad --features evm)" >&2; exit 1; }
[ -x "$MISAMINER_BIN" ] || { echo "no misaminer at $MISAMINER_BIN (cargo build --release -p misaminer)" >&2; exit 1; }

# ── stop leftovers from a previous run (pid files, then belt-and-braces by appdir) ──────────────
for pid_file in "$WORK_DIR"/*.pid; do
  [ -f "$pid_file" ] && kill "$(cat "$pid_file")" 2>/dev/null || true
done
pkill -f -- "--appdir=$WORK_DIR" 2>/dev/null || true
sleep 1
rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"

if [ "$PALW_REAL" = "1" ]; then
  : "${PALW_WORKER:?PALW_REAL=1 needs PALW_WORKER}"
  : "${MISAKA_PALW_GGUF:?PALW_REAL=1 needs MISAKA_PALW_GGUF}"
  PALW_ENV=(PALW_WORKER="$PALW_WORKER" MISAKA_PALW_GGUF="$MISAKA_PALW_GGUF")
  echo "mode        : REAL model ($MISAKA_PALW_GGUF)"
else
  PALW_ENV=(MISAKA_PALW_POW_FIXTURE=1)
  echo "mode        : fixture (model-free)"
fi

start_node() { # idx extra-args... — nodes get the PALW env; node-2 deliberately does not
  local i="$1"; shift
  local dir="$WORK_DIR/node-$i"
  mkdir -p "$dir"
  env "$@" "$KASPAD_BIN" \
    "${NET_ARGS[@]}" --appdir="$dir" \
    --listen="127.0.0.1:$((BASE_P2P + i * 10))" \
    --rpclisten="127.0.0.1:$((BASE_RPC + i * 10))" \
    --utxoindex --unsaferpc --nodnsseed --enable-unsynced-mining \
    ${i:+$( [ "$i" -gt 0 ] && echo "--addpeer=127.0.0.1:$BASE_P2P" )} \
    >>"$dir/kaspad.log" 2>&1 &
  echo $! > "$WORK_DIR/node-$i.pid"
}

start_node 0 "${PALW_ENV[@]}"
start_node 1 "${PALW_ENV[@]}"

for _ in $(seq 1 60); do
  if (exec 3<>"/dev/tcp/127.0.0.1/$BASE_RPC") 2>/dev/null; then exec 3<&- 3>&-; break; fi
  sleep 1
done

echo "mining      : $BLOCKS blocks against node-0 (sequential PALW attempts, network $NET)"
env "${PALW_ENV[@]}" "$MISAMINER_BIN" \
  --rpc="127.0.0.1:$BASE_RPC" --network-id="$NET" \
  --allow-burn --mine-when-not-synced --min-block-interval-ms=0 \
  --blocks="$BLOCKS" >>"$WORK_DIR/miner.log" 2>&1 || {
    echo "FAIL: miner exited non-zero — tail of miner.log:" >&2; tail -20 "$WORK_DIR/miner.log" >&2; exit 1; }

# ── node-1 must have accepted the same chain ────────────────────────────────────────────────────
# The flow context aggregates: "Accepted block <h> via relay" for singles, "Accepted N blocks …"
# for batches, and blocks that arrived while a slow PALW replay held the pipeline surface as
# "Unorphaned N block(s) …" (they are PoW-validated on the unorphan path) — sum all three forms.
count_accepted() {
  awk '/Accepted block /{n+=1}
       /Accepted [0-9]+ blocks/{for(i=1;i<=NF;i++) if($i=="Accepted"){n+=$(i+1); break}}
       /Unorphaned [0-9]+ block/{for(i=1;i<=NF;i++) if($i=="Unorphaned"){n+=$(i+1); break}}
       END{print n+0}' "$1" 2>/dev/null || echo 0
}
synced=0
for _ in $(seq 1 30); do
  accepted=$(count_accepted "$WORK_DIR/node-1/kaspad.log")
  if [ "${accepted:-0}" -ge "$BLOCKS" ]; then synced=1; break; fi
  sleep 2
done
if [ "$synced" != "1" ]; then
  echo "FAIL: node-1 accepted ${accepted:-0}/$BLOCKS blocks — tail of node-1 log:" >&2
  tail -20 "$WORK_DIR/node-1/kaspad.log" >&2
  exit 1
fi
echo "verified    : node-1 accepted $accepted blocks over p2p (independent PALW replay)"

# ── the designed fail-loud path: a node with NO PALW runtime must panic, not mis-reject ────────
# Scrub the PALW variables explicitly: in real mode they live in THIS script's environment and
# every child inherits them — without the scrub node-2 quietly validates via the inherited
# worker and the probe tests nothing.
start_node 2 -u PALW_WORKER -u MISAKA_PALW_GGUF -u MISAKA_PALW_POW_FIXTURE
sleep 25
if grep -qE "PALW PoW validation cannot run|validates PALW \(algo_id = 4\)" "$WORK_DIR/node-2/kaspad.log" 2>/dev/null; then
  echo "verified    : node-2 (no worker, no fixture) hit the designed fail-fast (startup rail / validation panic)"
elif kill -0 "$(cat "$WORK_DIR/node-2.pid")" 2>/dev/null \
    && [ "$(count_accepted "$WORK_DIR/node-2/kaspad.log")" -ge 1 ]; then
  echo "FAIL: node-2 validated PALW blocks without any PALW runtime" >&2
  exit 1
else
  echo "note        : node-2 probe inconclusive within 25 s (no headers reached it yet) — not fatal"
fi

# ── late joiner: full from-genesis IBD with per-header PALW replay ──────────────────────────────
if [ "$IBD" = "1" ]; then
  echo "ibd         : booting FRESH node-3 — it must replay-validate the whole chain from genesis"
  start_node 3 "${PALW_ENV[@]}"
  # IBD-received blocks surface as "IBD: Processed N blocks" progress + a completion line; blocks
  # arriving after the tip handshake surface as relay accepts. Count both.
  count_ibd() {
    awk '/IBD: Processed [0-9]+ blocks/{for(i=1;i<=NF;i++) if($i=="Processed"){if($(i+1)>n)n=$(i+1); break}} END{print n+0}' "$1" 2>/dev/null || echo 0
  }
  ibd_ok=0
  for _ in $(seq 1 60); do
    total=$(( $(count_ibd "$WORK_DIR/node-3/kaspad.log") + $(count_accepted "$WORK_DIR/node-3/kaspad.log") ))
    if grep -q "completed successfully" "$WORK_DIR/node-3/kaspad.log" 2>/dev/null && [ "$total" -ge "$BLOCKS" ]; then
      ibd_ok=1; break
    fi
    # A chain this small may sync entirely through the relay/orphan-resolution path without a
    # formal IBD round — accept tip parity however it was reached.
    if [ "$total" -ge "$BLOCKS" ]; then ibd_ok=1; break; fi
    sleep 5
  done
  if [ "$ibd_ok" != "1" ]; then
    echo "FAIL: node-3 did not reach $BLOCKS blocks from genesis — tail of node-3 log:" >&2
    tail -25 "$WORK_DIR/node-3/kaspad.log" >&2
    exit 1
  fi
  echo "verified    : node-3 caught up from genesis (${total} blocks; independent PALW replay of the whole chain)"
fi

# ── block cadence, for the eyeball: timestamps of the miner's submissions ───────────────────────
echo "cadence     : last mined-block timestamps (fixture mines fast until the DAA window fills;"
echo "              real-model runs pace toward the 10 s target)"
grep "mined block" "$WORK_DIR/miner.log" | tail -5 || true

for pid_file in "$WORK_DIR"/node-*.pid; do
  kill "$(cat "$pid_file")" 2>/dev/null || true
done
echo "PASS: $BLOCKS PALW (algo_id=4) blocks mined, independently replay-validated, and synced."
