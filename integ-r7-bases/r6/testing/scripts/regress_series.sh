#!/usr/bin/env bash
# Run N rounds of one topology and report the series, not the round.
#
# One green round of a concurrent system is an anecdote — this work has already produced several,
# including a 24/24 that was voided by a defect found by reading the code rather than by a failure.
# So the unit of evidence is the series, and the series is reported whole: every round's verdict,
# then the count.
#
# Fixture state is checked before starting. Two independently mined branches with distinct pruning
# points and the heavy one ahead is the entire premise; if that has drifted, a green series means
# nothing and a red one is misattributed.
#
# Usage: regress_series.sh <name> <rounds> <light_host:port> <heavy_host:port> <light_grpc> <heavy_grpc>
set -uo pipefail

NAME=$1
ROUNDS=$2
LIGHT_ADDR=$3
HEAVY_ADDR=$4
LIGHT_GRPC=$5   # host:port of the light node's RPC, for reading its chain identity
HEAVY_GRPC=$6

BASE=${BASE:-/var/lib/misaka-regression}
source "$BASE/regress_lib.sh"

read_chain() {
  local rpc=$1 field=$2
  "$RPC" "$rpc" 2>/dev/null | sed -n "s/.*$field=\([0-9a-z]*\).*/\1/p"
}

# The fixture nodes bind RPC to localhost — deliberately, since opening a node's RPC to the
# internet to run a test would be a poor trade. So a host that cannot reach a branch's RPC is given
# that branch's identity instead, read by the orchestrator over ssh and passed in.
LIGHT_PP=${LIGHT_PP:-$(read_chain "$LIGHT_GRPC" pruning_point)}
HEAVY_PP=${HEAVY_PP:-$(read_chain "$HEAVY_GRPC" pruning_point)}
HEAVY_SCORE=${HEAVY_SCORE:-$(read_chain "$HEAVY_GRPC" virtual_daa_score)}
LIGHT_SCORE=${LIGHT_SCORE:-$(read_chain "$LIGHT_GRPC" virtual_daa_score)}

# The premise, checked rather than assumed.
[ -n "$LIGHT_PP" ] && [ -n "$HEAVY_PP" ] || { echo "FIXTURE: could not read both branches" >&2; exit 2; }
[ "$LIGHT_PP" != "$HEAVY_PP" ] || { echo "FIXTURE: both branches report the same pruning point — not two histories" >&2; exit 2; }
[ "$HEAVY_SCORE" -gt "$LIGHT_SCORE" ] || { echo "FIXTURE: heavy ($HEAVY_SCORE) is not ahead of light ($LIGHT_SCORE)" >&2; exit 2; }

echo "SERIES $NAME: $ROUNDS rounds"
echo "  light $LIGHT_ADDR pp=${LIGHT_PP:0:16} score=$LIGHT_SCORE"
echo "  heavy $HEAVY_ADDR pp=${HEAVY_PP:0:16} score=$HEAVY_SCORE"

pass=0
fail=0
for i in $(seq 1 "$ROUNDS"); do
  if "$BASE/regress_round.sh" "$NAME-$i" "$LIGHT_ADDR" "$HEAVY_ADDR" "$LIGHT_PP" "$HEAVY_PP" "$HEAVY_SCORE"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
  fi
done

echo "SERIES $NAME complete: $pass passed, $fail failed, out of $ROUNDS"
[ "$fail" -eq 0 ]
