#!/usr/bin/env bash
# misaka-palw-soak-miner.sh — one PALW-4 miner against the host's local soak node (gate 4).
# Sequential real-inference grind; the mined-block log line carries bits/ts/blue_score/coinbase
# so the soak's difficulty and emission are auditable from miner.log alone.
#
#   BIN=<misaminer>    default $HOME/palw-soak/misaminer
#   APPDIR=<dir>       default $HOME/.palw-soak (log lands here as miner.log)
#   MEM_MAX=<size>     systemd MemoryMax for the miner unit (default 3500M — one worker inside)
#   WORKER/GGUF        as in misaka-palw-soak-node.sh
set -euo pipefail
BIN="${BIN:-$HOME/palw-soak/misaminer}"
APPDIR="${APPDIR:-$HOME/.palw-soak}"
MEM_MAX="${MEM_MAX:-3500M}"
WORKER="${WORKER:-$HOME/palw-class/palw-worker}"
GGUF="${GGUF:-$HOME/palw-class/Qwen3.5-2B-Q4_K_M.gguf}"
RIG="${RIG:-$(hostname -s)}"

[ -x "$BIN" ] || { echo "no misaminer at $BIN" >&2; exit 1; }
mkdir -p "$APPDIR"

margs=(--rpc=127.0.0.1:37710 --network-id=testnet-11 --worker="$RIG"
       --allow-burn --mine-when-not-synced --min-block-interval-ms=0 --blocks=0)

SUDO=""
if [ "$(id -u)" != 0 ] && sudo -n true 2>/dev/null; then SUDO="sudo"; fi
if [ "$(id -u)" = 0 ] || [ -n "$SUDO" ]; then
  $SUDO systemctl stop palw-soak-miner 2>/dev/null || true
  $SUDO systemctl reset-failed palw-soak-miner 2>/dev/null || true
  $SUDO systemd-run --unit=palw-soak-miner \
    -p MemoryMax="$MEM_MAX" -p MemorySwapMax=0 -p CPUWeight=30 \
    -p User="$(id -un)" \
    -p StandardOutput=append:"$APPDIR/miner.log" -p StandardError=append:"$APPDIR/miner.log" \
    -E PALW_WORKER="$WORKER" -E MISAKA_PALW_GGUF="$GGUF" -E MISAMINER_LOG=info \
    "$BIN" "${margs[@]}"
  sleep 5
  $SUDO systemctl is-active palw-soak-miner >/dev/null || { echo "miner died on startup:" >&2; tail -8 "$APPDIR/miner.log" >&2; exit 1; }
  echo "miner up (systemd palw-soak-miner, rig=$RIG, MemoryMax=$MEM_MAX)"
else
  pkill -f "misaminer .*testnet-11" 2>/dev/null || true
  sleep 1
  PALW_WORKER="$WORKER" MISAKA_PALW_GGUF="$GGUF" MISAMINER_LOG=info \
    nohup nice -n 12 "$BIN" "${margs[@]}" >>"$APPDIR/miner.log" 2>&1 &
  echo $! > "$APPDIR/miner.pid"
  sleep 5
  kill -0 "$(cat "$APPDIR/miner.pid")" 2>/dev/null || { echo "miner died on startup:" >&2; tail -8 "$APPDIR/miner.log" >&2; exit 1; }
  echo "miner up (nohup pid $(cat "$APPDIR/miner.pid"), rig=$RIG)"
fi