#!/usr/bin/env bash
# ADR-0135 — the permissionless model registry on a private devnet, end to end.
#
# Starts NODES fixture nodes (floor-only devnet, the execution lane open from genesis, the registry
# armed at REGISTRY_AT with `--palw-model-registry-devnet`), then waits, in order, for:
#   0/6  the registry scheduled on the nodes' ruleset (op 186 `scheduled`, `fenceDaa`)
#   1/6  the fence crossed and the base class's row open at the first span boundary past it (ACTIVE)
#   2/6  (with CLASS_ARTIFACT) a second class registered by node-1 (`--palw-register-class`)
#   3/6  (with CLASS_ARTIFACT) possession proofs on the chain for it from the seats that hold it
#   4/6  the activation grace passed with the base class still ACTIVE and the chain still producing;
#        with CLASS_ARTIFACT, the class stepped by the first governed boundary (PROBATION with enough
#        ready seats, HELD without) — the class-local verdict, named
#   5/6  node-2 restarted and its registry rows equal to node-1's (the rows are chain state)
#   6/6  PASS
# Without CLASS_ARTIFACT the drill proves the fence, the rows, the grace, the clock and the liveness of
# the base class; the proof path needs an artifact every node holds (CLASS_ARTIFACT=/path/to.palwart,
# MODEL_ID=… when the artifact's shape matches more than one class).
#
# Env: KASPAD_BIN, CLI_BIN, WORK_DIR, NODES (default 8: the registry wants seat_count + 2 ready seats),
# LANE (default 0,2,2: two permits a round, 2-DAA spans), REGISTRY_AT (default 20),
# STEP_WAIT (default 14400 s), STALL_WAIT (default 900 s), ATTACH=1 to poll nodes already running.
set -u
KASPAD_BIN="${KASPAD_BIN:-target/release/kaspad}"
CLI_BIN="${CLI_BIN:-target/release/misaka}"
WORK_DIR="${WORK_DIR:-.drill-adr0135}"
NODES="${NODES:-8}"
LANE="${LANE:-0,2,2}"
REGISTRY_AT="${REGISTRY_AT:-20}"
# ADR-0132 Upgrade C and ADR-0137: the payout and the work target, armed at a DAA or left dormant.
# The work target needs both at or below it (kaspad refuses otherwise); with it armed a model class
# draws against CCU / W0 and no share, budget or class target is read.
PAYOUT_AT="${PAYOUT_AT:-}"
WORK_TARGET_AT="${WORK_TARGET_AT:-}"
SINGLE_LOTTERY_AT="${SINGLE_LOTTERY_AT:-}"
CLASS_ARTIFACT="${CLASS_ARTIFACT:-}"
MODEL_ID="${MODEL_ID:-}"
# With an artifact the shipped devnet class set is used (the artifact's class is a genesis class the
# seats hold and prove for); REGISTER_CLASS=1 makes node-1 register the artifact's class instead, on
# the floor-only ruleset. Without an artifact the ruleset is floor-only.
FLOOR_ONLY="${FLOOR_ONLY:-$([ -n "$CLASS_ARTIFACT" ] && echo 0 || echo 1)}"
REGISTER_CLASS="${REGISTER_CLASS:-0}"
STEP_WAIT="${STEP_WAIT:-14400}"
STALL_WAIT="${STALL_WAIT:-900}"
ATTACH="${ATTACH:-0}"
P2P_BASE="${P2P_BASE:-16710}"
RPC_BASE="${RPC_BASE:-18010}"
PREMINE_TXID="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"
MAIN_PREMINE_INDEX=40

log() { printf '[registry-drill] %s\n' "$*" >&2; }
die() { log "FATAL: $*"; exit 1; }

for b in "$KASPAD_BIN" "$CLI_BIN"; do [ -x "$b" ] || die "missing binary $b (cargo build --release -p kaspad -p misaka-cli)"; done
"$KASPAD_BIN" --help 2>/dev/null | grep -q -- "--palw-model-registry-devnet" || die "this kaspad has no --palw-model-registry-devnet (ADR-0135)"
"$CLI_BIN" palw registry --help >/dev/null 2>&1 || die "this misaka has no \`palw registry\`"
[ -z "$CLASS_ARTIFACT" ] || [ -f "$CLASS_ARTIFACT" ] || die "CLASS_ARTIFACT=$CLASS_ARTIFACT is not a file"

cli() { local i="$1"; shift; "$CLI_BIN" --network devnet --rpc "127.0.0.1:$((RPC_BASE + i))" "$@"; }
# `reg N EXPR` evaluates a python expression over op 186's JSON (`v` = the registry object).
reg() {
  local i="$1" expr="$2"
  cli "$i" palw registry --output json 2>/dev/null | python3 -c "import json,sys; v=json.load(sys.stdin)['registry']; print($expr)" 2>/dev/null || true
}
daa_of() { cli "$1" palw round-lane --output json 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin).get('virtualDaa',''))" 2>/dev/null || true; }

pids=()
cleanup() { for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done; }

node_args() {
  local i="$1" addr="$2"
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
  if [ -n "$CLASS_ARTIFACT" ]; then
    args+=(--palw-class-artifact="$CLASS_ARTIFACT")
    if [ "$i" -eq 1 ] && [ "$REGISTER_CLASS" = 1 ]; then
      if [ -n "$MODEL_ID" ]; then args+=(--palw-register-class "$MODEL_ID"); else args+=(--palw-register-class); fi
    fi
  fi
  [ "$i" -gt 0 ] && args+=(--connect="127.0.0.1:$P2P_BASE")
  printf '%s\n' "${args[@]}"
}

start_node() {
  local i="$1"
  local addr; addr="$(cat "$WORK_DIR/keys/bond-$i.address")"
  local args=(); while IFS= read -r a; do args+=("$a"); done < <(node_args "$i" "$addr")
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >>"$WORK_DIR/node-$i.log" 2>&1 &
  echo $!
}

attach_nodes() {
  for ((i=0; i<NODES; i++)); do
    node_pid="$(lsof -nP -t -iTCP:"$((RPC_BASE + i))" -sTCP:LISTEN 2>/dev/null | head -1 || true)"
    [ -n "$node_pid" ] || die "ATTACH=1: nothing listens on node-$i's RPC port $((RPC_BASE + i))"
    pids+=("$node_pid")
  done
}

start_nodes() {
  for ((i=0; i<NODES; i++)); do
    for port in $((P2P_BASE + i)) $((RPC_BASE + i)); do
      if lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then die "port $port is in use — set P2P_BASE / RPC_BASE"; fi
    done
  done
  rm -rf "$WORK_DIR"; mkdir -p "$WORK_DIR/keys" "$WORK_DIR/out"
  python3 - "$WORK_DIR/keys" "$NODES" <<'PY'
import hashlib, os, sys
d, n = sys.argv[1], int(sys.argv[2])
h = lambda b: hashlib.blake2b(b, digest_size=32).hexdigest()
for i in range(n):
    p = f"{d}/bond-{i}.seed"; open(p, "w").write(h(b"misaka-devnet-genesis-bond-v1/" + str(i).encode())); os.chmod(p, 0o600)
PY
  trap cleanup EXIT
  for ((i=0; i<NODES; i++)); do
    addr="$("$CLI_BIN" --network devnet key address --key-file "$WORK_DIR/keys/bond-$i.seed" | tail -1 | awk '{print $NF}')"
    [ -n "$addr" ] || die "cannot derive bond $i's address"
    echo "$addr" > "$WORK_DIR/keys/bond-$i.address"
    pids+=("$(start_node "$i")")
    log "node-$i pid ${pids[$i]} bond $PREMINE_TXID:$i"
  done
}

if [ "$ATTACH" = 1 ]; then attach_nodes; else start_nodes; fi

alive() { for p in "${pids[@]}"; do [ -z "$p" ] || kill -0 "$p" 2>/dev/null || die "a node exited (see $WORK_DIR/node-*.log)"; done; }
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
    alive
    sleep 10
  done
  gave_up "$what"
}

log "0/6 the registry is scheduled on the nodes' ruleset"
wait_reg 1 "v.get('scheduled') == True and v.get('fenceDaa') == $REGISTRY_AT" "op 186 to answer scheduled at $REGISTRY_AT"
log "    $(reg 1 "'span %s DAA · grace until DAA %s · seats %s+%s' % (v.get('spanDaa'), v.get('graceUntilDaa'), v.get('seatCount'), v.get('spareSeats'))")"

log "1/6 the fence crossed: the base class's row opens ACTIVE at the first boundary past it"
wait_reg 1 "v.get('active') == True and any(c.get('isBaseClass') and c.get('hasRow') and c.get('state') == 'Active' for c in v.get('classes', []))" "the base class's row"
log "    $(reg 1 "'tip DAA %s · rows %s' % (v.get('tipDaa'), [(c['classId'][:12], c['state'] if c['hasRow'] else 'legacy') for c in v.get('classes', [])])")"

if [ -n "$CLASS_ARTIFACT" ]; then
  if [ "$REGISTER_CLASS" = 1 ]; then log "2/6 node-1 registers the artifact's class"; else log "2/6 the artifact's class is on the chain (a genesis class of the devnet's set)"; fi
  wait_reg 1 "any(not c.get('isBaseClass') and c.get('hasRow') for c in v.get('classes', []))" "a non-base class with a row"
  log "    $(reg 1 "[(c['classId'][:12], c['state'], c['artifactRoot'][:12]) for c in v.get('classes', []) if not c.get('isBaseClass')]")"
  log "3/6 the seats that hold the artifact prove possession of it"
  wait_reg 1 "sum(1 for r in v.get('readiness', []) if r.get('fresh')) >= 1" "the first possession proof"
  CLASS_ID="$(reg 1 "[r['classId'] for r in v.get('readiness', []) if r.get('fresh')][0]")"
  log "    $(reg 1 "'%d fresh proofs for class %s at DAA %s' % (sum(1 for r in v.get('readiness', []) if r.get('classId') == '$CLASS_ID' and r.get('fresh')), '$CLASS_ID'[:12], v.get('tipDaa'))")"
else
  log "2/6 skipped: no CLASS_ARTIFACT, so no second class to register"
  log "3/6 skipped: no second class, so no possession proofs to expect"
  CLASS_ID=""
fi

log "4/6 the activation grace passes; the base class stays ACTIVE and the chain keeps producing"
wait_reg 1 "v.get('tipDaa', 0) >= v.get('graceUntilDaa', 1 << 62) + v.get('spanDaa', 1)" "the grace to end and one governed boundary to pass"
[ "$(reg 1 "any(c.get('isBaseClass') and c.get('state') == 'Active' for c in v.get('classes', []))")" = "True" ] || die "the base class left ACTIVE"
if [ -n "$CLASS_ID" ]; then
  verdict="$(reg 1 "[ (c['state'], c['readySeatsNow'], c['requiredReadySeats']) for c in v.get('classes', []) if c['classId'] == '$CLASS_ID'][0]")"
  log "    the class after the first governed boundary: (state, ready now, required) = $verdict"
fi
log "    $(reg 1 "'tip DAA %s · %s' % (v.get('tipDaa'), [(c['classId'][:12], c['state'] if c['hasRow'] else 'legacy', c['readySeatsNow'], c['inflightNow'], c['sharePermille']) for c in v.get('classes', [])])")"
before_daa="$(daa_of 1)"; sleep 60; after_daa="$(daa_of 1)"
[ -n "$after_daa" ] && [ "$after_daa" != "$before_daa" ] || log "    (the virtual DAA did not move in 60 s: $before_daa → $after_daa; the fixture is slow, not stopped, unless the next step stalls)"

if [ "$ATTACH" != 1 ]; then
  log "5/6 node-2 restarts and re-reads the same rows from its chain state"
  kill "${pids[2]}" 2>/dev/null || true; sleep 3
  pids[2]="$(start_node 2)"
  sleep 20
  step_start
  while ! step_expired; do
    a="$(reg 1 "sorted((c['classId'], c['state'] if c['hasRow'] else 'legacy', c['sinceSpan']) for c in v.get('classes', []))")"
    b="$(reg 2 "sorted((c['classId'], c['state'] if c['hasRow'] else 'legacy', c['sinceSpan']) for c in v.get('classes', []))")"
    if [ -n "$a" ] && [ "$a" = "$b" ]; then log "    node-1 and node-2 agree: $a"; break; fi
    alive; sleep 10
  done
  [ "$a" = "$b" ] || gave_up "node-2 to answer the rows node-1 holds ($a vs $b)"
else
  log "5/6 skipped under ATTACH=1"
fi

for n in 1 2; do cli "$n" palw registry --output json > "$WORK_DIR/out/registry-node-$n.json" 2>/dev/null || true; done
log "6/6 PASS — the registry armed at $REGISTRY_AT, the rows opened at the boundary, the grace passed with the base class ACTIVE and the chain producing, and a restarted node holds the same rows$([ -n "$CLASS_ID" ] && echo "; the second class was registered and proved" || echo " (no second class: the proof path was not exercised)")"
log "evidence: $WORK_DIR/node-*.log, $WORK_DIR/out/"
exit 0
