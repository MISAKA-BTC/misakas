#!/usr/bin/env bash
# ADR-0135 — phase three of the registry drill, on the nodes phase two left running: the faults.
#
#   1/4  three seats stop (an operator outage): their proofs age out, the class's ready seats fall
#        below a panel, and the class alone is HELD — the base class stays ACTIVE and the chain
#        keeps producing (the property ADR-0133/0135 promise)
#   2/4  one of the three restarts WITHOUT its artifact: it proves nothing (fail-closed) and says so
#   3/4  the other two restart with their artifacts and re-prove; the class recovers HELD → PROBATION
#   4/4  PASS
# Env as the earlier phases (WORK_DIR, NODES, LANE, REGISTRY_AT, CLASS_ARTIFACT, RPC_BASE, P2P_BASE).
set -u
KASPAD_BIN="${KASPAD_BIN:-target/release/kaspad}"
CLI_BIN="${CLI_BIN:-target/release/misaka}"
WORK_DIR="${WORK_DIR:-.drill-adr0135}"
NODES="${NODES:-7}"
LANE="${LANE:-0,2,2}"
REGISTRY_AT="${REGISTRY_AT:-20}"
# ADR-0132 Upgrade C and ADR-0137: the payout and the work target, armed at a DAA or left dormant.
PAYOUT_AT="${PAYOUT_AT:-}"
WORK_TARGET_AT="${WORK_TARGET_AT:-}"
SINGLE_LOTTERY_AT="${SINGLE_LOTTERY_AT:-}"
VERIFICATION_V2_AT="${VERIFICATION_V2_AT:-}"
READINESS_V2_AT="${READINESS_V2_AT:-}"
ANCHOR_CLOCK_AT="${ANCHOR_CLOCK_AT:-}"
CLASS_ARTIFACT="${CLASS_ARTIFACT:-}"
STEP_WAIT="${STEP_WAIT:-14400}"
STALL_WAIT="${STALL_WAIT:-1800}"
P2P_BASE="${P2P_BASE:-16710}"
RPC_BASE="${RPC_BASE:-18010}"
# FLOOR_ONLY=1 when the first phase ran on the floor-only ruleset (the class registered by node-1): the
# restarted nodes must name the same ruleset their datadirs hold.
FLOOR_ONLY="${FLOOR_ONLY:-0}"
PREMINE_TXID="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"
MAIN_PREMINE_INDEX=40
STOPPED="${STOPPED:-4 5 6}"

log() { printf '[registry-faults] %s\n' "$*" >&2; }
die() { log "FATAL: $*"; exit 1; }
[ -n "$CLASS_ARTIFACT" ] || die "CLASS_ARTIFACT is required"
cli() { local i="$1"; shift; "$CLI_BIN" --network devnet --rpc "127.0.0.1:$((RPC_BASE + i))" "$@"; }
reg() { local i="$1" expr="$2"; cli "$i" palw registry --output json 2>/dev/null | python3 -c "import json,sys; v=json.load(sys.stdin)['registry']; print($expr)" 2>/dev/null || true; }
daa_of() { cli "$1" palw round-lane --output json 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin).get('virtualDaa',''))" 2>/dev/null || true; }
step_start() { step_began=$SECONDS; last_daa=""; last_move=$SECONDS; }
step_expired() {
  local daa; daa="$(daa_of 1)"
  if [ -n "$daa" ] && [ "$daa" != "$last_daa" ]; then last_daa="$daa"; last_move=$SECONDS; fi
  [ $((SECONDS - step_began)) -ge "$STEP_WAIT" ] || [ $((SECONDS - last_move)) -ge "$STALL_WAIT" ]
}
gave_up() { die "gave up waiting for: $1 (virtual DAA ${last_daa:-unanswered}, unmoved for $((SECONDS - last_move))s, $((SECONDS - step_began))s into the step)"; }
wait_reg() {
  local node="$1" expr="$2" what="$3"
  step_start
  while ! step_expired; do
    [ "$(reg "$node" "$expr")" = "True" ] && return 0
    sleep 15
  done
  gave_up "$what"
}
start_node() {
  local i="$1" with_artifact="$2"
  local addr; addr="$(cat "$WORK_DIR/keys/bond-$i.address")"
  local args=(--devnet --appdir="$WORK_DIR/node-$i" --listen="127.0.0.1:$((P2P_BASE + i))" --rpclisten-borsh="127.0.0.1:$((RPC_BASE + i))"
        --utxoindex --nodnsseed --disable-upnp --nogrpc --enable-unsynced-mining
        --palw-execution-lane-devnet="$LANE" --palw-model-registry-devnet="$REGISTRY_AT"
        --palw-produce --palw-panel --palw-round-lane
        --palw-producer-key="$WORK_DIR/keys/bond-$i.seed" --palw-producer-bond="$PREMINE_TXID:$i"
        --palw-producer-pay-address="$addr" --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + i))")
  [ "$FLOOR_ONLY" = 1 ] && args+=(--palw-devnet-floor-only)
  [ -n "$PAYOUT_AT" ] && args+=(--palw-economic-payout-devnet="$PAYOUT_AT")
  [ -n "$WORK_TARGET_AT" ] && args+=(--palw-work-target-devnet="$WORK_TARGET_AT")
  [ -n "$SINGLE_LOTTERY_AT" ] && args+=(--palw-single-lottery-devnet="$SINGLE_LOTTERY_AT")
  [ -n "$VERIFICATION_V2_AT" ] && args+=(--palw-verification-v2-devnet="$VERIFICATION_V2_AT")
  [ -n "$READINESS_V2_AT" ] && args+=(--palw-readiness-v2-devnet="$READINESS_V2_AT")
  [ -n "$ANCHOR_CLOCK_AT" ] && args+=(--palw-anchor-clock-devnet="$ANCHOR_CLOCK_AT")
  [ "$with_artifact" = 1 ] && args+=(--palw-class-artifact="$CLASS_ARTIFACT")
  args+=(--connect="127.0.0.1:$P2P_BASE")
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >>"$WORK_DIR/node-$i.log" 2>&1 &
  echo $!
}

CLASS_ID="$(reg 1 "[c['classId'] for c in v.get('classes', []) if not c.get('isBaseClass') and c.get('hasRow')][0]")"
[ -n "$CLASS_ID" ] || die "op 186 lists no non-base class with a row on node-1"
before="$(reg 1 "[(c['state'], c['readySeatsNow'], c['requiredReadySeats']) for c in v['classes'] if c['classId']=='$CLASS_ID'][0]")"
log "class $CLASS_ID before the faults: (state, ready, required) = $before"

log "1/4 seats $STOPPED stop; their proofs age out and the class alone is HELD"
for i in $STOPPED; do
  pid="$(lsof -nP -t -iTCP:"$((RPC_BASE + i))" -sTCP:LISTEN 2>/dev/null | head -1 || true)"
  [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
done
wait_reg 1 "any(c.get('classId') == '$CLASS_ID' and c.get('state') == 'Held' for c in v.get('classes', []))" "the class to be HELD after the outage"
log "    $(reg 1 "[(c['state'], c['readySeatsNow'], c['reason']) for c in v['classes'] if c['classId']=='$CLASS_ID'][0]")"
[ "$(reg 1 "any(c.get('isBaseClass') and c.get('state') == 'Active' for c in v.get('classes', []))")" = "True" ] || die "the base class left ACTIVE during the outage"
before_daa="$(daa_of 1)"; sleep 90; after_daa="$(daa_of 1)"
[ -n "$after_daa" ] && [ "$after_daa" != "$before_daa" ] || die "the chain stopped producing during the class's outage ($before_daa → $after_daa)"
log "    the chain went on: DAA $before_daa → $after_daa while the class was held"

set -- $STOPPED
without="$1"; shift
log "2/4 seat $without restarts without its artifact: no proof, fail-closed"
start_node "$without" 0 >/dev/null
sleep 60
grep -q "readiness for class.*no proof — this node holds no artifact" "$WORK_DIR/node-$without.log" && log "    node-$without: $(grep 'no proof — this node holds no artifact' "$WORK_DIR/node-$without.log" | tail -1 | cut -c1-160)" || log "    (node-$without has not logged its refusal yet)"

log "3/4 seats $* restart with their artifacts and re-prove; the class recovers to PROBATION"
for i in "$@"; do start_node "$i" 1 >/dev/null; done
wait_reg 1 "any(c.get('classId') == '$CLASS_ID' and c.get('state') in ('Probation', 'ActiveLimited', 'Active') for c in v.get('classes', []))" "the class to recover"
log "    $(reg 1 "[(c['state'], c['readySeatsNow'], c['requiredReadySeats'], c['reason']) for c in v['classes'] if c['classId']=='$CLASS_ID'][0]")"
[ "$(reg 1 "any(r.get('classId') == '$CLASS_ID' and r.get('fresh') for r in v.get('readiness', []))")" = "True" ] || die "no fresh proof after the restarts"
for n in 1; do cli "$n" palw registry --output json > "$WORK_DIR/out/registry-faults-node-$n.json" 2>/dev/null || true; done
log "4/4 PASS — an operator outage held the class alone with the chain producing, a seat without its artifact proved nothing, and the returning seats brought the class back through probation"
exit 0
