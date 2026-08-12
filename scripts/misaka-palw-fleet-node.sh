#!/usr/bin/env bash
# misaka-palw-fleet-node.sh — start ONE isolated PALW testnet-10 node on a fleet host.
#
# This is the multi-machine counterpart of `misaka-palw-pow-e2e.sh` (which runs every role on one
# box): it brings up a single node that peers with the rest of the fleet, so a real chain can be
# mined on host A and independently replay-validated on hosts B and C — the only test that
# exercises what the design actually claims.
#
# Isolation from a LIVE t10 fleet, three independent ways (any one suffices):
#   * its own appdir (`$HOME/.palw-fleet`), never the production one;
#   * its own ports (37711 p2p / 37710 grpc), not the 26211/26210 the live chain uses;
#   * `--nodnsseed` plus explicit `--addpeer` of the other PALW nodes only.
# Belt and braces: a PALW build's genesis and consensus fingerprint differ from the live chain's,
# so the two reject each other at the handshake even if they ever met.
#
# A host whose firewall drops inbound (e.g. ufw default-deny) can still take part — give it the
# others in PEERS and leave it out of THEIR peer lists; kaspa's p2p is bidirectional over whichever
# side dialed.
#
#   BIN=<kaspad>           default $HOME/palw-release/kaspad
#   MODEL=<ollama ref>     default misaka-palw-2b-f16 (the pinned F16 class model)
#   PEERS="ip:port ..."    the other fleet nodes this one should dial
set -euo pipefail
BIN="${BIN:-$HOME/palw-release/kaspad}"
APPDIR="${APPDIR:-$HOME/.palw-fleet}"
MODEL="${MODEL:-misaka-palw-2b-f16}"
PEERS="${PEERS:-}"

[ -x "$BIN" ] || { echo "no kaspad at $BIN" >&2; exit 1; }
pkill -f -- "--appdir=$APPDIR" 2>/dev/null || true
sleep 1
mkdir -p "$APPDIR"

args=(--testnet --netsuffix=10 --appdir="$APPDIR"
      --listen=0.0.0.0:37711 --rpclisten=0.0.0.0:37710
      --utxoindex --unsaferpc --nodnsseed --enable-unsynced-mining)
for p in $PEERS; do args+=(--addpeer="$p"); done

MISAKA_PALW_OLLAMA_MODEL="$MODEL" nohup "$BIN" "${args[@]}" >>"$APPDIR/kaspad.log" 2>&1 &
echo $! > "$APPDIR/kaspad.pid"
sleep 8
if ! kill -0 "$(cat "$APPDIR/kaspad.pid")" 2>/dev/null; then
  echo "node died on startup:" >&2; tail -8 "$APPDIR/kaspad.log" >&2; exit 1
fi
echo "node up: pid $(cat "$APPDIR/kaspad.pid")  appdir=$APPDIR  peers=[$PEERS]"
grep -m1 "PALW-Ollama runtime" "$APPDIR/kaspad.log" || true
