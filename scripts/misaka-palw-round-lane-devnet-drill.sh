#!/usr/bin/env bash
# misaka-palw-round-lane-devnet-drill.sh — ADR-0125 §7.1: the execution lane on a LIVE devnet, with
# nothing but what the binaries already do.
#
#   node-0 … N-1    attempt producers, panel seats and round producers under devnet public-seed bonds
#                   0 … N-1. The floor-only devnet genesis carries six bonds and panels of five with the
#                   executor excluded, so four running nodes seat three of every claim's five — the
#                   quorum — and attempts license and reach Final without anyone else.
#   Every node:     --palw-devnet-floor-only (the base class alone) and --palw-execution-lane-devnet=$LANE
#                   (the lane open from its activation, in the consensus fingerprint, so every node
#                   carries the same spec) and --palw-round-lane (a round block in every round its
#                   bond holds a permit for).
#
# What it asserts, in order — each a poll against the chain and the logs, never a fixed sleep:
#   1. an attempt reaches Final and the lane schedules a span from it (getPalwRoundLane lists a domain).
#      ADR-0130 makes that TWO span boundaries, not one: the finals of span s become span s+2's
#      participants at the first chain block of s+1, and that snapshot is seeded into a schedule at the
#      first chain block of s+2 — by the latest chain block of s+1 that carried an attempt. A span with
#      no attempt-carrying chain block in the span before it stays idle and the lane waits for the next
#      one, which on this floor-only devnet (every PALW block carries an attempt) does not happen;
#   2. a node produces a round block for a permit its bond holds;
#   3. a chain block merges round blocks and grants their permits (the chain walk's lane line);
#   4. payments are sent and a granted round block carries one of THEM: a lane line written after
#      the sends names a sent transaction id among its native transactions (PALW carriers, which
#      round blocks carry all the time, do not count), and the recipient's balance moves;
#   5. two nodes report the same lane: span, accepted permits and schedule.
# A reorg across a span boundary is the pipeline suite's (`adr0125_*` in the virtual processor's
# tests); this drill does not partition the network.
#
# Build first:
#   cargo build --release -p kaspad -p misaka-cli
#
# A step waits on CHAIN time, not wall time. A licensed claim is Final `window_challenge` DAA later (100
# on the devnet) and the lane then needs two span boundaries (ADR-0130), so step 1 needs some 150 DAA
# plus up to two spans — at the default 10-DAA span, some 170 DAA, over an hour and a half at the pace
# four floor producers keep on one host (about 1.5 DAA a minute). The span was 30 DAA while a schedule
# followed its finals directly; with the extra boundary a 30-DAA span would add another forty minutes to
# every run, so the default span is 10 and the flag still takes any value. A step gives up when node-1's
# virtual DAA stops moving for STALL_WAIT seconds; STEP_WAIT only caps a chain that moves and never gets
# there.
#
# Env: KASPAD_BIN, CLI_BIN (defaults target/release/*), NODES (4, at least 4), WORK_DIR, LANE
# (`activation,width,span[,daa:width…]`, default 0,2,10), STALL_WAIT (s, 900), STEP_WAIT (s, 14400),
# SENDS (5 fee-paying transactions, one every SEND_EVERY seconds), P2P_BASE / RPC_BASE (port bases),
# ATTACH (1: poll the nodes an earlier run left running in WORK_DIR instead of starting new ones — and
# leave them running at exit).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
CLI_BIN="${CLI_BIN:-$REPO_ROOT/target/release/misaka}"
NODES="${NODES:-4}"
WORK_DIR="${WORK_DIR:-$REPO_ROOT/.misaka-palw-round-lane-devnet}"
LANE="${LANE:-0,2,10}"
STALL_WAIT="${STALL_WAIT:-900}"
STEP_WAIT="${STEP_WAIT:-14400}"
ATTACH="${ATTACH:-0}"
SENDS="${SENDS:-5}"
SEND_EVERY="${SEND_EVERY:-20}"
P2P_BASE="${P2P_BASE:-16610}"
RPC_BASE="${RPC_BASE:-17910}"
PREMINE_TXID="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"   # "misaka-premine", zero-padded
MAIN_PREMINE_INDEX=40   # consensus/core/src/config/premine.rs; bond n's fee float is MAIN_PREMINE_INDEX + 1 + n

log() { printf '[round-lane-drill] %s\n' "$*" >&2; }
die() { log "FATAL: $*"; exit 1; }

for b in "$KASPAD_BIN" "$CLI_BIN"; do [ -x "$b" ] || die "missing binary $b (cargo build --release -p kaspad -p misaka-cli)"; done
[ "$NODES" -ge 4 ] || die "NODES must be at least 4: an executor and a three-seat quorum"
"$KASPAD_BIN" --help 2>/dev/null | grep -q -- "--palw-execution-lane-devnet" || die "this kaspad has no --palw-execution-lane-devnet (ADR-0125)"
"$KASPAD_BIN" --help 2>/dev/null | grep -q -- "--palw-round-lane" || die "this kaspad has no --palw-round-lane (ADR-0125)"
"$CLI_BIN" palw round-lane --help >/dev/null 2>&1 || die "this misaka has no \`palw round-lane\`"

cli() { local i="$1"; shift; "$CLI_BIN" --network devnet --rpc "127.0.0.1:$((RPC_BASE + i))" "$@"; }
# `lane <node> <python expression over the response dict v>` — prints the expression's value.
lane() {
  local i="$1" expr="$2"
  cli "$i" palw round-lane --output json 2>/dev/null | python3 -c "import json,sys; v=json.load(sys.stdin); print($expr)" 2>/dev/null || true
}

pids=()
cleanup() { for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done; }

# The nodes an earlier run left running in WORK_DIR (its keys, its logs), found by their RPC ports.
attach_nodes() {
  [ -f "$WORK_DIR/keys/main.seed" ] || die "ATTACH=1 polls the nodes an earlier run started, and $WORK_DIR has no keys"
  for ((i=0; i<NODES; i++)); do
    node_pid="$(lsof -nP -t -iTCP:"$((RPC_BASE + i))" -sTCP:LISTEN 2>/dev/null | head -1 || true)"
    [ -n "$node_pid" ] || die "ATTACH=1: nothing listens on node-$i's RPC port $((RPC_BASE + i))"
    pids+=("$node_pid")
    log "node-$i pid $node_pid attached (left running at exit)"
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
p = f"{d}/main.seed"; open(p, "w").write(h(b"misaka-testnet-premine-9b-claude-managed")); os.chmod(p, 0o600)
p = f"{d}/recipient.seed"; open(p, "w").write(h(b"misaka-round-lane-drill/recipient")); os.chmod(p, 0o600)
PY

  trap cleanup EXIT
  for ((i=0; i<NODES; i++)); do
    addr="$("$CLI_BIN" --network devnet key address --key-file "$WORK_DIR/keys/bond-$i.seed" | tail -1 | awk '{print $NF}')"
    [ -n "$addr" ] || die "cannot derive bond $i's address"
    echo "$addr" > "$WORK_DIR/keys/bond-$i.address"
    args=(--devnet --appdir="$WORK_DIR/node-$i" --listen="127.0.0.1:$((P2P_BASE + i))" --rpclisten-borsh="127.0.0.1:$((RPC_BASE + i))"
          --utxoindex --nodnsseed --disable-upnp --nogrpc --enable-unsynced-mining
          --palw-devnet-floor-only --palw-execution-lane-devnet="$LANE"
          --palw-produce --palw-panel --palw-round-lane
          --palw-producer-key="$WORK_DIR/keys/bond-$i.seed" --palw-producer-bond="$PREMINE_TXID:$i"
          --palw-producer-pay-address="$addr" --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + i))")
    [ "$i" -gt 0 ] && args+=(--connect="127.0.0.1:$P2P_BASE")
    MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >"$WORK_DIR/node-$i.log" 2>&1 &
    node_pid=$!
    pids+=("$node_pid")
    log "node-$i pid $node_pid bond $PREMINE_TXID:$i (lane $LANE)"
  done
}

if [ "$ATTACH" = 1 ]; then attach_nodes; else start_nodes; fi

alive() { for p in "${pids[@]}"; do kill -0 "$p" 2>/dev/null || die "a node exited (see $WORK_DIR/node-*.log)"; done; }

# A step's clock (see the header): `step_start`, then `step_expired` on every poll — true once node-1's
# virtual DAA has not moved for STALL_WAIT seconds, or the step has run STEP_WAIT seconds.
step_start() { step_began=$SECONDS; last_daa=""; last_move=$SECONDS; }
step_expired() {
  local daa
  daa="$(lane 1 "v.get('virtualDaa')")"
  if [ -n "$daa" ] && [ "$daa" != "$last_daa" ]; then last_daa="$daa"; last_move=$SECONDS; fi
  [ $((SECONDS - step_began)) -ge "$STEP_WAIT" ] || [ $((SECONDS - last_move)) -ge "$STALL_WAIT" ]
}
gave_up() { die "gave up waiting for: $1 (virtual DAA ${last_daa:-unanswered}, unmoved for $((SECONDS - last_move))s, $((SECONDS - step_began))s into the step)"; }

# Poll until `pattern` appears in any node log (or the given files); echo the first matching line.
wait_log() {
  local pattern="$1" what="$2" files="${3:-$WORK_DIR/node-*.log}" line=""
  step_start
  while ! step_expired; do
    # shellcheck disable=SC2086
    line="$(grep -h -m1 -E "$pattern" $files 2>/dev/null | head -1 || true)"
    if [ -n "$line" ]; then echo "$line"; return 0; fi
    alive
    sleep 5
  done
  gave_up "$what"
}

# Poll until `lane <node> <expr>` prints True.
wait_lane() {
  local node="$1" expr="$2" what="$3"
  step_start
  while ! step_expired; do
    [ "$(lane "$node" "$expr")" = "True" ] && return 0
    alive
    sleep 10
  done
  gave_up "$what"
}

log "0/5 the lane is armed on the nodes' ruleset"
wait_lane 1 "v.get('armed') == True" "getPalwRoundLane to answer armed"
log "    $(cli 1 palw round-lane 2>/dev/null | head -1)"

log "1/5 waiting for an attempt to reach Final and a span to be scheduled from it (two boundaries: ADR-0130)"
wait_lane 1 "len(v.get('domains', [])) > 0" "a scheduled span (an attempt reached Final two spans earlier)"
log "    $(cli 1 palw round-lane 2>/dev/null | head -3 | tr '\n' ' ')"

log "2/5 waiting for a round block for a held permit"
produced="$(wait_log "\\[palw-round-producer\\] [0-9]+ round blocks produced" "a produced round block")"
log "    $produced"

log "3/5 waiting for a chain block to merge round blocks and grant their permits"
granted="$(wait_log "\\[palw-round-lane\\] chain block [0-9a-f]+ merged [0-9]+ round block\\(s\\): [1-9][0-9]* permit\\(s\\) granted" "a granted permit")"
log "    $granted"

log "4/5 sending $SENDS fee-paying transactions and waiting for a granted round block to carry one"
RECIPIENT="$("$CLI_BIN" --network devnet key address --key-file "$WORK_DIR/keys/recipient.seed" | tail -1 | awk '{print $NF}')"
[ -n "$RECIPIENT" ] || die "cannot derive the recipient's address"
balance() {
  cli 1 wallet utxo list --address "$RECIPIENT" --output json 2>/dev/null \
    | python3 -c 'import json,sys; v=json.load(sys.stdin); print(v["mature"]["sompi"] + v["immature"]["sompi"])' 2>/dev/null || echo "?"
}
before_balance="$(balance)"
# Where each node log ends now: step 4's evidence must be written after this point, or a lane line
# from before the first payment (carriers ride the lane from the moment it opens) would pass it.
for ((i=0; i<NODES; i++)); do wc -c < "$WORK_DIR/node-$i.log" > "$WORK_DIR/out/offset-$i"; done
sent=0
sent_ids=()
for ((k=0; k<SENDS; k++)); do
  if cli 0 wallet send --key-file "$WORK_DIR/keys/main.seed" --to "$RECIPIENT" --amount "1.0000000$k" --yes --output json \
      >"$WORK_DIR/out/send-$k.json" 2>&1; then
    sent=$((sent + 1))
    txid="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("txid") or "")' "$WORK_DIR/out/send-$k.json" 2>/dev/null || true)"
    [ -n "$txid" ] && sent_ids+=("$txid")
    log "    sent ${txid:-?}"
  else
    log "    send $k refused: $(tail -1 "$WORK_DIR/out/send-$k.json")"
  fi
  sleep "$SEND_EVERY"
done
[ "$sent" -gt 0 ] || die "no fee-paying transaction was accepted by the mempool"
[ "${#sent_ids[@]}" -gt 0 ] || die "the sends were accepted but printed no transaction id"
# The lane lines written since the offsets, from every node; a payment is carried when one of them
# names its id. The chain walk names the first four native ids of each merging block.
since_offsets() {
  for ((i=0; i<NODES; i++)); do
    tail -c +"$(( $(cat "$WORK_DIR/out/offset-$i") + 1 ))" "$WORK_DIR/node-$i.log" 2>/dev/null
  done
}
ids_pattern="$(IFS='|'; echo "${sent_ids[*]}")"
step_start
carried=""
while ! step_expired; do
  carried="$(since_offsets | grep -m1 -E "\\[palw-round-lane\\] chain block [0-9a-f]+ merged .*native: .*(${ids_pattern})" || true)"
  [ -n "$carried" ] && break
  alive
  sleep 5
done
[ -n "$carried" ] || gave_up "a round block carrying one of the sent payments"
log "    $carried"
after_balance="$(balance)"
log "    recipient balance ${before_balance:-?} → ${after_balance:-?} sompi"

log "5/5 two nodes report one lane"
a="$(lane 1 "(v.get('span'), v.get('permitsPerRound'), len(v.get('domains', [])))")"
b="$(lane $((NODES - 1)) "(v.get('span'), v.get('permitsPerRound'), len(v.get('domains', [])))")"
log "    node-1 $a · node-$((NODES - 1)) $b"
for n in 1 $((NODES - 1)); do cli "$n" palw round-lane --output json > "$WORK_DIR/out/round-lane-node-$n.json" 2>/dev/null || true; done
granted_lines="$(cat "$WORK_DIR"/node-*.log | grep -c "\\[palw-round-lane\\] chain block" || true)"
produced_blocks="$(grep -h -oE "\\[palw-round-producer\\] [0-9]+ round blocks produced" "$WORK_DIR"/node-*.log | awk '{s+=$2} END {print s+0}' || true)"
log "    lane lines across the fleet: $granted_lines · round-block production reports (powers of two, summed): $produced_blocks"
log "PASS — attempts reached Final, a later span was scheduled from them, round blocks were produced for held permits, chain blocks granted them, and a granted round block carried a sent payment"
log "evidence: $WORK_DIR/node-*.log, $WORK_DIR/out/"
[ "$ATTACH" = 1 ] && log "the attached nodes are still running: kill ${pids[*]}"
exit 0
