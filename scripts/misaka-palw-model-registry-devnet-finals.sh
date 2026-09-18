#!/usr/bin/env bash
# ADR-0135 — phase two of the registry drill, on the datadirs the first phase left behind (its
# nodes stop at its exit): every node restarts from its own state — a restart/persistence check in
# itself — one of them as a producer for the artifact's class, then the class's claims go through
# the registry's panels to `Final`, and the lifecycle leaves PROBATION.
#
#   1/4  the nodes restart from their datadirs, node-3 as a producer for the class (`--palw-producer-class`)
#   2/4  the class's first claim is accepted (op 186 `inflightNow` ≥ 1 or a Final)
#   3/4  the class's first claim reaches `Final` (`misaka palw economics`: claims_final ≥ 1 for the class)
#   4/5  the class leaves PROBATION (ACTIVE_LIMITED or ACTIVE) — needs `probationClaims` finals
#   5/5  a fresh node (NODES, an empty datadir) syncs from the others and holds the same registry rows
#
# Env as the first phase (WORK_DIR, NODES, LANE, REGISTRY_AT, CLASS_ARTIFACT, RPC_BASE, P2P_BASE,
# STEP_WAIT, STALL_WAIT), plus PRODUCER_NODE (default 3).
set -u
KASPAD_BIN="${KASPAD_BIN:-target/release/kaspad}"
CLI_BIN="${CLI_BIN:-target/release/misaka}"
WORK_DIR="${WORK_DIR:-.drill-adr0135}"
NODES="${NODES:-8}"
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
PRODUCER_NODE="${PRODUCER_NODE:-3}"
STEP_WAIT="${STEP_WAIT:-14400}"
STALL_WAIT="${STALL_WAIT:-1800}"
P2P_BASE="${P2P_BASE:-16710}"
RPC_BASE="${RPC_BASE:-18010}"
# FLOOR_ONLY=1 when the first phase ran on the floor-only ruleset (the class registered by node-1): the
# restarted nodes must name the same ruleset their datadirs hold.
FLOOR_ONLY="${FLOOR_ONLY:-0}"
PREMINE_TXID="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"
MAIN_PREMINE_INDEX=40

log() { printf '[registry-finals] %s\n' "$*" >&2; }
die() { log "FATAL: $*"; exit 1; }
[ -n "$CLASS_ARTIFACT" ] || die "CLASS_ARTIFACT is required: the class to produce for is the artifact's"
cli() { local i="$1"; shift; "$CLI_BIN" --network devnet --rpc "127.0.0.1:$((RPC_BASE + i))" "$@"; }
reg() { local i="$1" expr="$2"; cli "$i" palw registry --output json 2>/dev/null | python3 -c "import json,sys; v=json.load(sys.stdin)['registry']; print($expr)" 2>/dev/null || true; }
eco() { local i="$1" expr="$2"; cli "$i" palw economics --output json 2>/dev/null | python3 -c "import json,sys; v=json.load(sys.stdin); print($expr)" 2>/dev/null || true; }
daa_of() { cli "$1" palw round-lane --output json 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin).get('virtualDaa',''))" 2>/dev/null || true; }
step_start() { step_began=$SECONDS; last_daa=""; last_move=$SECONDS; }
step_expired() {
  local daa; daa="$(daa_of 1)"
  if [ -n "$daa" ] && [ "$daa" != "$last_daa" ]; then last_daa="$daa"; last_move=$SECONDS; fi
  [ $((SECONDS - step_began)) -ge "$STEP_WAIT" ] || [ $((SECONDS - last_move)) -ge "$STALL_WAIT" ]
}
gave_up() { die "gave up waiting for: $1 (virtual DAA ${last_daa:-unanswered}, unmoved for $((SECONDS - last_move))s, $((SECONDS - step_began))s into the step)"; }
wait_for() {
  local node="$1" kind="$2" expr="$3" what="$4"
  step_start
  while ! step_expired; do
    if [ "$kind" = reg ]; then [ "$(reg "$node" "$expr")" = "True" ] && return 0; else [ "$(eco "$node" "$expr")" = "True" ] && return 0; fi
    sleep 15
  done
  gave_up "$what"
}

[ -d "$WORK_DIR/node-0" ] && [ -f "$WORK_DIR/keys/bond-0.address" ] || die "$WORK_DIR holds no first-phase datadirs"
pids=()
cleanup() { for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT
for ((i=0; i<NODES; i++)); do
  for port in $((P2P_BASE + i)) $((RPC_BASE + i)); do
    pid="$(lsof -nP -t -iTCP:"$port" -sTCP:LISTEN 2>/dev/null | head -1 || true)"
    [ -n "$pid" ] && { kill "$pid" 2>/dev/null || true; }
  done
done
sleep 4
start_node() {
  local i="$1" extra="$2"
  local addr; addr="$(cat "$WORK_DIR/keys/bond-$i.address")"
  local args=(--devnet --appdir="$WORK_DIR/node-$i" --listen="127.0.0.1:$((P2P_BASE + i))" --rpclisten-borsh="127.0.0.1:$((RPC_BASE + i))"
        --utxoindex --nodnsseed --disable-upnp --nogrpc --enable-unsynced-mining
        --palw-execution-lane-devnet="$LANE" --palw-model-registry-devnet="$REGISTRY_AT"
        --palw-produce --palw-panel --palw-round-lane --palw-class-artifact="$CLASS_ARTIFACT"
        --palw-producer-key="$WORK_DIR/keys/bond-$i.seed" --palw-producer-bond="$PREMINE_TXID:$i"
        --palw-producer-pay-address="$addr" --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + i))")
  [ "$FLOOR_ONLY" = 1 ] && args+=(--palw-devnet-floor-only)
  [ -n "$PAYOUT_AT" ] && args+=(--palw-economic-payout-devnet="$PAYOUT_AT")
  [ -n "$WORK_TARGET_AT" ] && args+=(--palw-work-target-devnet="$WORK_TARGET_AT")
  [ -n "$SINGLE_LOTTERY_AT" ] && args+=(--palw-single-lottery-devnet="$SINGLE_LOTTERY_AT")
  [ -n "$VERIFICATION_V2_AT" ] && args+=(--palw-verification-v2-devnet="$VERIFICATION_V2_AT")
  [ -n "$READINESS_V2_AT" ] && args+=(--palw-readiness-v2-devnet="$READINESS_V2_AT")
  [ -n "$ANCHOR_CLOCK_AT" ] && args+=(--palw-anchor-clock-devnet="$ANCHOR_CLOCK_AT")
  [ -n "$extra" ] && args+=("$extra")
  [ "$i" -gt 0 ] && args+=(--connect="127.0.0.1:$P2P_BASE")
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >>"$WORK_DIR/node-$i.log" 2>&1 &
  echo $!
}
log "1/4 the nodes restart from their datadirs; node-$PRODUCER_NODE as a producer for the class"
for ((i=0; i<NODES; i++)); do
  [ "$i" -eq "$PRODUCER_NODE" ] && continue
  pids+=("$(start_node "$i" "")")
done
sleep 15
step_start
while ! step_expired; do
  CLASS_ID="$(reg 1 "[c['classId'] for c in v.get('classes', []) if not c.get('isBaseClass') and c.get('hasRow')][0]")"
  [ -n "$CLASS_ID" ] && break
  sleep 10
done
[ -n "$CLASS_ID" ] || die "op 186 lists no non-base class with a row on node-1 after the restart"
log "    class $CLASS_ID · $(reg 1 "[(c['state'], c['readySeatsNow'], c['requiredReadySeats'], c['inflightNow'], c['maxInflightClaims'], c['sharePermille']) for c in v['classes'] if c['classId']=='$CLASS_ID'][0]")"
pids+=("$(start_node "$PRODUCER_NODE" "--palw-producer-class=$CLASS_ID")")
log "    node-$PRODUCER_NODE pid ${pids[-1]} producing for the class"

log "2/4 the class's first claim is accepted"
wait_for 1 eco "any(c.get('class_id') == '$CLASS_ID' and c.get('claims', {}).get('accepted', 0) >= 1 for c in v.get('census', []))" "the class's first accepted claim"
log "    $(eco 1 "[(c['claims']) for c in v['census'] if c['class_id']=='$CLASS_ID'][0]")"

log "3/4 the class's first claim reaches Final"
wait_for 1 eco "any(c.get('class_id') == '$CLASS_ID' and c.get('claims', {}).get('final', 0) >= 1 for c in v.get('census', []))" "the class's first Final"
log "    $(eco 1 "[(c['claims']) for c in v['census'] if c['class_id']=='$CLASS_ID'][0]") · registry $(reg 1 "[(c['state'], c['probesPassed'], c['probesFailed']) for c in v['classes'] if c['classId']=='$CLASS_ID'][0]")"

log "4/5 the class leaves PROBATION"
wait_for 1 reg "any(c.get('classId') == '$CLASS_ID' and c.get('state') in ('ActiveLimited', 'Active') for c in v.get('classes', []))" "the class past probation"
log "    $(reg 1 "[(c['state'], c['readySeatsNow'], c['inflightNow'], c['sharePermille'], c['reason']) for c in v['classes'] if c['classId']=='$CLASS_ID'][0]")"
for n in 1 2; do cli "$n" palw registry --output json > "$WORK_DIR/out/registry-finals-node-$n.json" 2>/dev/null || true; cli "$n" palw economics --output json > "$WORK_DIR/out/economics-finals-node-$n.json" 2>/dev/null || true; done
log "5/5 a fresh node syncs and holds the same rows"
f="$NODES"
python3 - "$WORK_DIR/keys" "$f" <<'PYSEED'
import hashlib, os, sys
d, i = sys.argv[1], int(sys.argv[2])
h = lambda b: hashlib.blake2b(b, digest_size=32).hexdigest()
p = f"{d}/bond-{i}.seed"; open(p, "w").write(h(b"misaka-devnet-genesis-bond-v1/" + str(i).encode())); os.chmod(p, 0o600)
PYSEED
addr="$("$CLI_BIN" --network devnet key address --key-file "$WORK_DIR/keys/bond-$f.seed" | tail -1 | awk '{print $NF}')"
echo "$addr" > "$WORK_DIR/keys/bond-$f.address"
rm -rf "$WORK_DIR/node-$f"
MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" --devnet --appdir="$WORK_DIR/node-$f" --listen="127.0.0.1:$((P2P_BASE + f))" --rpclisten-borsh="127.0.0.1:$((RPC_BASE + f))" \
  --utxoindex --nodnsseed --disable-upnp --nogrpc \
  --palw-execution-lane-devnet="$LANE" --palw-model-registry-devnet="$REGISTRY_AT" --palw-panel --palw-class-artifact="$CLASS_ARTIFACT" \
  ${PAYOUT_AT:+--palw-economic-payout-devnet=$PAYOUT_AT} ${WORK_TARGET_AT:+--palw-work-target-devnet=$WORK_TARGET_AT} ${SINGLE_LOTTERY_AT:+--palw-single-lottery-devnet=$SINGLE_LOTTERY_AT} ${VERIFICATION_V2_AT:+--palw-verification-v2-devnet=$VERIFICATION_V2_AT} ${READINESS_V2_AT:+--palw-readiness-v2-devnet=$READINESS_V2_AT} ${ANCHOR_CLOCK_AT:+--palw-anchor-clock-devnet=$ANCHOR_CLOCK_AT} \
  --palw-producer-key="$WORK_DIR/keys/bond-$f.seed" --palw-producer-bond="$PREMINE_TXID:$f" --palw-producer-pay-address="$addr" \
  --connect="127.0.0.1:$P2P_BASE" >>"$WORK_DIR/node-$f.log" 2>&1 &
pids+=("$!")
log "    node-$f pid ${pids[-1]} syncing from an empty datadir"
step_start
while ! step_expired; do
  a="$(reg 1 "sorted((c['classId'], c['state'] if c['hasRow'] else 'legacy', c['sinceSpan']) for c in v.get('classes', []))")"
  b="$(reg "$f" "sorted((c['classId'], c['state'] if c['hasRow'] else 'legacy', c['sinceSpan']) for c in v.get('classes', []))")"
  ta="$(reg 1 "v.get('tipDaa')")"; tb="$(reg "$f" "v.get('tipDaa')")"
  if [ -n "$a" ] && [ "$a" = "$b" ] && [ -n "$tb" ] && [ "$tb" -ge $(( ${ta:-0} - 4 )) ]; then log "    the fresh node holds node-1's rows at DAA $tb: $a"; break; fi
  sleep 15
done
[ "$a" = "$b" ] || gave_up "the fresh node to hold node-1's rows ($a vs $b)"
log "PASS — the nodes restarted from their datadirs, the class produced, its claim reached Final through the registry's panels, the lifecycle left probation, and a fresh node synced to the same rows"
log "the nodes are left running for the operator (kill them with: pkill -f '$WORK_DIR/node-')"
trap - EXIT
exit 0
