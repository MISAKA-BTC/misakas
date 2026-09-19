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
# **RAM_SCALE: the drill's nodes are neighbours competing for one host's memory, and a panel that
# cannot replay licenses nothing.** ADR-0136's budget is `70 % x (MemAvailable - 1 GiB)` and it is a
# `const` with no flag, so the only lever is what the NODES leave unclaimed. Run 20 measured the
# trap: eight nodes on a 12 GiB host left 1.61 GiB available, a 1.87 GiB budget against the 2.17 GiB
# a 1.67 GiB class needs, and 2,226 deferrals in four hours with the class stuck at licensed 5 /
# final 0. Fewer nodes is not the fix -- `PALW_V2_PANEL_SEATS` (5) needs `seats + 1` distinct
# operators or nothing is ever licensed, so six is the floor and six is still short. `--ram-scale`
# is: it bounds the caches the node claims for itself and hands the rest back to the replay.
# Empty keeps the node's own default, so a host with room behaves exactly as before.
RAM_SCALE="${RAM_SCALE:-}"
LANE="${LANE:-0,2,2}"
REGISTRY_AT="${REGISTRY_AT:-20}"
# ADR-0132 Upgrade C and ADR-0137: the payout and the work target, armed at a DAA or left dormant.
PAYOUT_AT="${PAYOUT_AT:-}"
WORK_TARGET_AT="${WORK_TARGET_AT:-}"
SINGLE_LOTTERY_AT="${SINGLE_LOTTERY_AT:-}"
VERIFICATION_V2_AT="${VERIFICATION_V2_AT:-}"
READINESS_V2_AT="${READINESS_V2_AT:-}"
ANCHOR_CLOCK_AT="${ANCHOR_CLOCK_AT:-}"
# ADR-0143: the drill crosses this fence too, so the build that arms it on testnet-11 has been
# through a crossing rather than only through its unit tests (the launch runbook's §5c gate).
ARTIFACT_ROOT_OWNERSHIP_AT="${ARTIFACT_ROOT_OWNERSHIP_AT:-}"
CLASS_ARTIFACT="${CLASS_ARTIFACT:-}"
STEP_WAIT="${STEP_WAIT:-14400}"
STALL_WAIT="${STALL_WAIT:-1800}"
P2P_BASE="${P2P_BASE:-16710}"
RPC_BASE="${RPC_BASE:-18010}"
# ADR-0138: past `palw_anchor_clock` only a block `bits` priced advances the DAA score, so a devnet
# whose only producers are PALW lanes has no clock at all. MINER_BIN points at a Layer-0 hash miner
# (`target/release/misaminer`); it mines against node MINER_NODE's gRPC, which is opened only for it.
MINER_BIN="${MINER_BIN:-}"
MINER_NODE="${MINER_NODE:-0}"
GRPC_PORT="${GRPC_PORT:-16610}"
MINER_INTERVAL_MS="${MINER_INTERVAL_MS:-5000}"
# testnet-11's shape: two hash miners and one bondless heartbeat miner. HEARTBEAT_NODE runs the
# second lane, so the drill exercises the stand-in rule (ADR-0138: a heartbeat advances the DAA only
# in a mergeset that carries nothing `bits` priced) instead of leaving it to the unit tests.
HEARTBEAT_NODE="${HEARTBEAT_NODE:-}"
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
        --utxoindex --nodnsseed --disable-upnp --enable-unsynced-mining
        --palw-execution-lane-devnet="$LANE" --palw-model-registry-devnet="$REGISTRY_AT"
        --palw-produce --palw-panel --palw-round-lane
        --palw-producer-key="$WORK_DIR/keys/bond-$i.seed" --palw-producer-bond="$PREMINE_TXID:$i"
        --palw-producer-pay-address="$addr" --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + i))")
  [ "$FLOOR_ONLY" = 1 ] && args+=(--palw-devnet-floor-only)
  [ -n "$RAM_SCALE" ] && args+=(--ram-scale="$RAM_SCALE")
  [ -n "$PAYOUT_AT" ] && args+=(--palw-economic-payout-devnet="$PAYOUT_AT")
  [ -n "$WORK_TARGET_AT" ] && args+=(--palw-work-target-devnet="$WORK_TARGET_AT")
  [ -n "$SINGLE_LOTTERY_AT" ] && args+=(--palw-single-lottery-devnet="$SINGLE_LOTTERY_AT")
  [ -n "$VERIFICATION_V2_AT" ] && args+=(--palw-verification-v2-devnet="$VERIFICATION_V2_AT")
  [ -n "$READINESS_V2_AT" ] && args+=(--palw-readiness-v2-devnet="$READINESS_V2_AT")
  [ -n "$ANCHOR_CLOCK_AT" ] && args+=(--palw-anchor-clock-devnet="$ANCHOR_CLOCK_AT")
  [ -n "$ARTIFACT_ROOT_OWNERSHIP_AT" ] && args+=(--palw-artifact-root-ownership-devnet="$ARTIFACT_ROOT_OWNERSHIP_AT")
  [ "$with_artifact" = 1 ] && args+=(--palw-class-artifact="$CLASS_ARTIFACT")
  args+=(--connect="127.0.0.1:$P2P_BASE")
  [ -n "$HEARTBEAT_NODE" ] && [ "$i" = "$HEARTBEAT_NODE" ] && args+=(--palw-heartbeat-miner-address="$addr")
  if [ -n "$MINER_BIN" ] && [ "$i" -eq "$MINER_NODE" ]; then args+=(--rpclisten="127.0.0.1:$GRPC_PORT"); else args+=(--nogrpc); fi
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >>"$WORK_DIR/node-$i.log" 2>&1 &
  echo $!
}

start_miner() {
  [ -n "$MINER_BIN" ] || return 0
  [ -x "$MINER_BIN" ] || die "MINER_BIN=$MINER_BIN is not executable"
  pgrep -f "misaminer .*127.0.0.1:$GRPC_PORT" >/dev/null 2>&1 && { log "a hash miner is already mining node-$MINER_NODE"; return 0; }
  local addr; addr="$(cat "$WORK_DIR/keys/bond-$MINER_NODE.address")"
  local waited=0
  while ! lsof -nP -iTCP:"$GRPC_PORT" -sTCP:LISTEN >/dev/null 2>&1; do
    waited=$((waited + 2)); [ "$waited" -gt 120 ] && die "node-$MINER_NODE never opened gRPC on $GRPC_PORT for the hash miner"
    sleep 2
  done
  "$MINER_BIN" --pool "127.0.0.1:$GRPC_PORT" --wallet "$addr" --network-id devnet --threads 1 \
    --min-block-interval-ms "$MINER_INTERVAL_MS" --mine-when-not-synced >>"$WORK_DIR/miner.log" 2>&1 &
  local mp=$!
  echo "$mp" > "$WORK_DIR/miner.pid"
  pids+=("$mp")
  log "hash miner pid $mp on node-$MINER_NODE gRPC $GRPC_PORT, one block per ${MINER_INTERVAL_MS} ms — the anchor lane the DAA clock counts"
}
start_miner

# **"On the nodes phase two left running" was never true, and this phase inherited the lie.**
# `run20-all.sh` runs the phases in sequence, and finals.sh ends with `trap cleanup EXIT` killing
# every node it started — on PASS and on FATAL alike. So by the time this script runs there is no
# fleet, and every check below reports an absence it caused. That is why phase 3 has never run in
# any generation of this drill: not a chain fault, not a timing fault, a hand-off that does not exist.
#
# A phase that depends on its predecessor's processes is a phase that can only be run one way. This
# one now brings the fleet up from the datadirs itself, exactly as phase 2 does — and ATTACHES to
# whatever is already listening, so running it straight after a live phase 2 still works.
wait_for_free_datadir() {
  local i="$1" lock="$WORK_DIR/node-$i/misaka-devnet/datadir/meta/LOCK" waited=0 holders=""
  [ -e "$lock" ] || return 0
  while lsof -nP -- "$lock" >/dev/null 2>&1; do
    waited=$((waited + 2))
    if [ "$waited" -gt 180 ]; then
      holders="$(lsof -nP -t -- "$lock" 2>/dev/null | tr '\n' ' ')"
      log "    node-$i's datadir is still held after ${waited}s by pid(s): ${holders:-none}; SIGKILL"
      for pid in $holders; do kill -9 "$pid" 2>/dev/null || true; done
      sleep 5
      lsof -nP -- "$lock" >/dev/null 2>&1 && die "node-$i's datadir lock survives a SIGKILL of ${holders}"
      return 0
    fi
    sleep 2
  done
}
started=0; attached=0
for ((i=0; i<NODES; i++)); do
  if lsof -nP -iTCP:"$((RPC_BASE + i))" -sTCP:LISTEN >/dev/null 2>&1; then attached=$((attached + 1)); continue; fi
  wait_for_free_datadir "$i"
  start_node "$i" 1 >/dev/null
  started=$((started + 1))
done
log "fleet: $attached node(s) already listening, $started started from their datadirs"

# **The only thing that waited for the nodes was the hash miner, and this drill has no hash lane.**
# `start_miner` returns at its first line when MINER_BIN is empty -- which is the configuration that
# matches testnet-11 (no bits-priced lane) -- and the gRPC wait it would otherwise have done was the
# sole reason the read below ever found a node up. Run 20's phase 3 died in the SAME SECOND it
# started, and blamed the chain: `reg` swallows a connection failure into an empty string, so a node
# that has not opened its RPC yet is indistinguishable from a registry with no class in it.
# Wait for the answer, and say which of the two failed.
registry_answers() {
  local node="$1" out
  out="$(cli "$node" palw registry --output json 2>/dev/null)" || return 1
  [ -n "$out" ] || return 1
  printf '%s' "$out" | python3 -c "import json,sys; json.load(sys.stdin)['registry']['classes']" 2>/dev/null
}
waited=0
until registry_answers 1; do
  waited=$((waited + 3))
  [ "$waited" -gt 240 ] && die "node-1 never answered op 186 in ${waited}s — the nodes were not up, which is not the same as a registry with no class"
  sleep 3
done
log "node-1 answers op 186 after ${waited}s"

CLASS_ID="$(reg 1 "[c['classId'] for c in v.get('classes', []) if not c.get('isBaseClass') and c.get('hasRow')][0]")"
[ -n "$CLASS_ID" ] || die "node-1 answers op 186 but lists no non-base class with a row — the registry is genuinely empty of registered classes"
before="$(reg 1 "[(c['state'], c['readySeatsNow'], c['requiredReadySeats']) for c in v['classes'] if c['classId']=='$CLASS_ID'][0]")"
log "class $CLASS_ID before the faults: (state, ready, required) = $before"

log "1/5 seats $STOPPED stop; their proofs age out and the class alone is HELD"
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
log "2/5 seat $without restarts without its artifact: no proof, fail-closed"
start_node "$without" 0 >/dev/null
sleep 60
grep -q "readiness for class.*no proof — this node holds no artifact" "$WORK_DIR/node-$without.log" && log "    node-$without: $(grep 'no proof — this node holds no artifact' "$WORK_DIR/node-$without.log" | tail -1 | cut -c1-160)" || log "    (node-$without has not logged its refusal yet)"

log "3/5 seats $* restart with their artifacts and re-prove; the class recovers to PROBATION"
for i in "$@"; do start_node "$i" 1 >/dev/null; done
wait_reg 1 "any(c.get('classId') == '$CLASS_ID' and c.get('state') in ('Probation', 'ActiveLimited', 'Active') for c in v.get('classes', []))" "the class to recover"
log "    $(reg 1 "[(c['state'], c['readySeatsNow'], c['requiredReadySeats'], c['reason']) for c in v['classes'] if c['classId']=='$CLASS_ID'][0]")"
[ "$(reg 1 "any(r.get('classId') == '$CLASS_ID' and r.get('fresh') for r in v.get('readiness', []))")" = "True" ] || die "no fresh proof after the restarts"
for n in 1; do cli "$n" palw registry --output json > "$WORK_DIR/out/registry-faults-node-$n.json" 2>/dev/null || true; done
log "4/5 the hash lane stops: the heartbeat stands in and the DAA clock keeps ticking (ADR-0138)"
if [ -n "$MINER_BIN" ] && [ -n "$HEARTBEAT_NODE" ] && [ -f "$WORK_DIR/miner.pid" ]; then
  mp="$(cat "$WORK_DIR/miner.pid")"
  kill "$mp" 2>/dev/null || true
  sleep 10
  kill -0 "$mp" 2>/dev/null && die "the hash miner (pid $mp) would not stop"
  hb_before="$(daa_of 1)"
  log "    the hash miner is down; DAA $hb_before — every mergeset from here carries nothing \`bits\` priced"
  moved=0
  for _ in $(seq 1 30); do
    sleep 20
    hb_after="$(daa_of 1)"
    [ -n "$hb_after" ] && [ -n "$hb_before" ] && [ "$hb_after" != "$hb_before" ] && { moved=1; break; }
  done
  [ "$moved" = 1 ] || die "the DAA froze with the hash lane down ($hb_before -> ${hb_after:-?}): the heartbeat did not stand in"
  log "    DAA $hb_before -> $hb_after on heartbeats alone — the stand-in is live, not just a unit test"
  start_miner
  sleep 20
  hb_back="$(daa_of 1)"
  [ -n "$hb_back" ] && [ "$hb_back" != "$hb_after" ] || die "the DAA stopped when the hash lane came back ($hb_after -> ${hb_back:-?})"
  log "    the hash lane is back and priced blocks pace the clock again: DAA $hb_after -> $hb_back"
else
  log "    (skipped: needs MINER_BIN and HEARTBEAT_NODE)"
fi

log "5/5 PASS — an operator outage held the class alone with the chain producing, a seat without its artifact proved nothing, the returning seats brought the class back through probation, and the DAA clock survived the hash lane going down"
exit 0
