#!/usr/bin/env bash
# One regression round against the two-history VPS fixture.
#
# A fresh follower is offered the LIGHT branch first and the HEAVY branch a few seconds later. The
# property under test is that arrival order and network distance do not decide the outcome — which
# is exactly what decided testnet-22.
#
# Both peer addresses are arguments, so the same script runs the two topologies that matter:
#
#   adversarial   follower on the light host   light ~0ms, heavy ~267ms   the worse chain is nearer
#   control       follower near the heavy host light ~251ms, heavy ~2ms   the better chain is nearer
#
# The adversarial one is the real test. The control exists so that a failure can be attributed: if
# only the adversarial topology fails, the problem is discovery, retry, or a deadline; if the
# control fails too, distance was never the issue and the state machine is wrong.
#
# Usage: regress_round.sh <round> <light_host:port> <heavy_host:port> <light_pp> <heavy_pp> <heavy_score>
#
# Prints one verdict line per round. Nothing here touches production: simnet, own params file, own
# data dir under /tmp, ports in the 412xx range, and every process stop goes through
# stop_regress_pid, which refuses anything whose /proc/<pid>/exe is outside the regression tree.
set -uo pipefail

ROUND=$1
LIGHT_ADDR=$2
HEAVY_ADDR=$3
LIGHT_PP=$4
HEAVY_PP=$5
HEAVY_SCORE=$6

BASE=${BASE:-/var/lib/misaka-regression}
# shellcheck source=regress_lib.sh
source "$BASE/regress_lib.sh"

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
"$RPC" "127.0.0.1:$FOLLOWER_GRPC" connect "$LIGHT_ADDR" || {
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

# Converging is half the property. A node that reaches the right chain and then never
# participates has failed differently, not succeeded quietly — so wait out the review floor and
# see it resume. Bounded: the floor is 180s and the whole verification budget fits inside it, so
# anything beyond this is the node still holding back for a reason worth reporting.
READY=false
for _ in $(seq 1 ${READY_WAIT_TICKS:-52}); do
  if [ "$("$RPC" "127.0.0.1:$FOLLOWER_GRPC" 2>/dev/null | sed -n 's/.*is_synced=\([a-z]*\).*/\1/p')" = "true" ]; then
    READY=true
    break
  fi
  sleep 5
done
RESULT=$("$RPC" "127.0.0.1:$FOLLOWER_GRPC" 2>/dev/null || echo "$RESULT")

PP=$(sed -n 's/.*pruning_point=\([0-9a-f]*\).*/\1/p' <<<"$RESULT")
SYNCED=$(sed -n 's/.*is_synced=\([a-z]*\).*/\1/p' <<<"$RESULT")
ON_HEAVY=false; [ "$PP" = "$HEAVY_PP" ] && ON_HEAVY=true
ON_LIGHT=false; [ "$PP" = "$LIGHT_PP" ] && ON_LIGHT=true

echo "VPS-ROUND round=$ROUND settled=$([ $SETTLED -eq 0 ] && echo true || echo false) on_heavy=$ON_HEAVY on_light=$ON_LIGHT is_synced=$SYNCED became_ready=$READY light=$LIGHT_ADDR heavy=$HEAVY_ADDR"
echo "  probe: $RESULT"

stop_regress_pid "$BASE/follower.pid" || true
# Two ways to fail: acting on the wrong chain, or reaching the right one and never acting at all.
[ "$ON_LIGHT" = true ] && [ "$SYNCED" = true ] && exit 1
[ "$ON_HEAVY" = true ] && [ "$READY" = true ] && exit 0
exit 1
