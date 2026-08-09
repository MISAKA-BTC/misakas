#!/usr/bin/env bash
# One regression round against the two-history VPS fixture.
#
# A fresh follower is offered the LIGHT branch first (loopback, so it wins the race every time) and
# the HEAVY branch a few seconds later from 267 ms away. The property under test is that the closer,
# lighter chain does not decide the outcome — which is exactly what decided testnet-22.
#
# Usage: regress_round.sh <round> <heavy_host:port> <light_pruning_point> <heavy_pruning_point> <heavy_score>
#
# Prints one verdict line per round. Nothing here touches production: simnet, own params file, own
# data dir under /tmp, ports in the 412xx range, and every process stop goes through
# stop_regress_pid, which refuses anything whose /proc/<pid>/exe is outside the regression tree.
set -uo pipefail

ROUND=$1
HEAVY_ADDR=$2
LIGHT_PP=$3
HEAVY_PP=$4
HEAVY_SCORE=$5

BASE=${BASE:-/tmp/misaka-regress}
# shellcheck source=regress_lib.sh
source "$BASE/regress_lib.sh"

LIGHT_P2P=41211
FOLLOWER_P2P=41231
FOLLOWER_GRPC=41241
TIMEOUT=${TIMEOUT:-600}

stop_regress_pid "$BASE/follower.pid" || exit 1
rm -rf "$BASE/follower" "$BASE/follower.log"
mkdir -p "$BASE/follower"

nohup "$BIN" --simnet \
  --override-params-file="$BASE/shallow_preset.json" \
  --appdir="$BASE/follower" \
  --listen=0.0.0.0:$FOLLOWER_P2P \
  --rpclisten=127.0.0.1:$FOLLOWER_GRPC \
  --rpclisten-borsh=127.0.0.1:$((FOLLOWER_GRPC+1000)) \
  --rpclisten-json=127.0.0.1:$((FOLLOWER_GRPC+2000)) \
  --disable-upnp --unsaferpc --enable-unsynced-mining --enforce-chain-participation \
  --nologfiles \
  > "$BASE/follower.log" 2>&1 &
echo $! > "$BASE/follower.pid"
sleep 10

# Deliberately NOT --connect: that flag means "connect only to these", which would either lock the
# heavy peer out entirely or make what happens when it is added an open question. Both peers are
# introduced the same way the local end-to-end tests introduce them — addPeer, permanent — so the
# only thing differing between the two harnesses is the network between the nodes.
"$RPC" "127.0.0.1:$FOLLOWER_GRPC" connect "127.0.0.1:$LIGHT_P2P" || {
  echo "VPS-ROUND round=$ROUND FAILED to introduce the light peer" >&2
  stop_regress_pid "$BASE/follower.pid" || true
  exit 2
}

# The heavy peer arrives late and from 267 ms away — the disadvantage it has to overcome on
# evidence alone, since the light one has already had the latch to itself for several seconds.
sleep "${SECOND_PEER_DELAY:-8}"
"$RPC" "127.0.0.1:$FOLLOWER_GRPC" connect "$HEAVY_ADDR" || {
  echo "VPS-ROUND round=$ROUND FAILED to introduce the heavy peer" >&2
  stop_regress_pid "$BASE/follower.pid" || true
  exit 2
}

RESULT=$(wait_for_score "$FOLLOWER_GRPC" "$HEAVY_SCORE" "$TIMEOUT")
SETTLED=$?

PP=$(sed -n 's/.*pruning_point=\([0-9a-f]*\).*/\1/p' <<<"$RESULT")
SYNCED=$(sed -n 's/.*is_synced=\([a-z]*\).*/\1/p' <<<"$RESULT")
ON_HEAVY=false; [ "$PP" = "$HEAVY_PP" ] && ON_HEAVY=true
ON_LIGHT=false; [ "$PP" = "$LIGHT_PP" ] && ON_LIGHT=true

echo "VPS-ROUND round=$ROUND settled=$([ $SETTLED -eq 0 ] && echo true || echo false) on_heavy=$ON_HEAVY on_light=$ON_LIGHT is_synced=$SYNCED heavy=$HEAVY_ADDR"
echo "  probe: $RESULT"

stop_regress_pid "$BASE/follower.pid" || true
[ "$ON_LIGHT" = true ] && [ "$SYNCED" = true ] && exit 1
exit 0
