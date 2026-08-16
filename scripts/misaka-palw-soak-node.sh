#!/usr/bin/env bash
# misaka-palw-soak-node.sh — one testnet-11 (PALW-4 staging) node on a fleet host, for the
# gate-4 multi-day soak. The multi-machine counterpart of `misaka-palw-pow-e2e.sh`, on the
# public-testnet SHAPE (120 s blocks) instead of devnet's 10 s — at 10 s the fleet's measured
# 6-16 s/header replay cost exceeds the interval on the slower hosts by arithmetic.
#
# Isolation from live t10 AND live devnet, independently:
#   * testnet-11 genesis + fingerprint (62781823…) — handshake-rejects both live meshes;
#   * its own appdir and ports (37711 p2p / 37710 grpc), never the production ones;
#   * --nodnsseed + explicit --addpeer of the other soak nodes only; --disable-upnp.
#
# Resource containment (the gate-2 lesson: the failure domain must be the EXPERIMENT):
# runs as a systemd transient unit with MemoryMax — if the host is short, the soak node
# dies, never the production kaspad. On hosts without root, falls back to nohup+nice.
#
#   BIN=<kaspad>       default $HOME/palw-soak/kaspad
#   APPDIR=<dir>       default $HOME/.palw-soak
#   PEERS="ip:port …"  other soak nodes to dial
#   MEM_MAX=<size>     systemd MemoryMax for the node unit (default 5G)
#   WORKER=<palw-worker>  default $HOME/palw-class/palw-worker
#   GGUF=<model>       default $HOME/palw-class/Qwen3.5-2B-Q4_K_M.gguf (persistent, NOT /tmp)
set -euo pipefail
BIN="${BIN:-$HOME/palw-soak/kaspad}"
APPDIR="${APPDIR:-$HOME/.palw-soak}"
PEERS="${PEERS:-}"
MEM_MAX="${MEM_MAX:-5G}"
WORKER="${WORKER:-$HOME/palw-class/palw-worker}"
GGUF="${GGUF:-$HOME/palw-class/Qwen3.5-2B-Q4_K_M.gguf}"

[ -x "$BIN" ] || { echo "no kaspad at $BIN" >&2; exit 1; }
[ -x "$WORKER" ] || { echo "no palw-worker at $WORKER" >&2; exit 1; }
[ -f "$GGUF" ] || { echo "no GGUF at $GGUF (copy it out of /tmp first — reboots clear /tmp)" >&2; exit 1; }

mkdir -p "$APPDIR"
args=(--testnet --netsuffix=11 --appdir="$APPDIR"
      --listen=0.0.0.0:37711 --rpclisten=127.0.0.1:37710
      --utxoindex --unsaferpc --nodnsseed --disable-upnp --enable-unsynced-mining)
for p in $PEERS; do args+=(--addpeer="$p"); done

SUDO=""
if [ "$(id -u)" != 0 ] && sudo -n true 2>/dev/null; then SUDO="sudo"; fi
if [ "$(id -u)" = 0 ] || [ -n "$SUDO" ]; then
  $SUDO systemctl stop palw-soak-node 2>/dev/null || true
  $SUDO systemctl reset-failed palw-soak-node 2>/dev/null || true
  $SUDO systemd-run --unit=palw-soak-node \
    -p MemoryMax="$MEM_MAX" -p MemorySwapMax=0 -p CPUWeight=40 \
    -p User="$(id -un)" \
    -p StandardOutput=append:"$APPDIR/kaspad.log" -p StandardError=append:"$APPDIR/kaspad.log" \
    -E PALW_WORKER="$WORKER" -E MISAKA_PALW_GGUF="$GGUF" \
    "$BIN" "${args[@]}"
  sleep 8
  $SUDO systemctl is-active palw-soak-node >/dev/null || { echo "node died on startup:" >&2; tail -8 "$APPDIR/kaspad.log" >&2; exit 1; }
  echo "node up (systemd palw-soak-node, MemoryMax=$MEM_MAX) appdir=$APPDIR peers=[$PEERS]"
else
  pkill -f -- "--appdir=$APPDIR" 2>/dev/null || true
  sleep 1
  PALW_WORKER="$WORKER" MISAKA_PALW_GGUF="$GGUF" \
    nohup nice -n 10 "$BIN" "${args[@]}" >>"$APPDIR/kaspad.log" 2>&1 &
  echo $! > "$APPDIR/kaspad.pid"
  sleep 8
  kill -0 "$(cat "$APPDIR/kaspad.pid")" 2>/dev/null || { echo "node died on startup:" >&2; tail -8 "$APPDIR/kaspad.log" >&2; exit 1; }
  echo "node up (nohup pid $(cat "$APPDIR/kaspad.pid")) appdir=$APPDIR peers=[$PEERS]"
fi
grep -m1 "Consensus params fingerprint" "$APPDIR/kaspad.log" || true