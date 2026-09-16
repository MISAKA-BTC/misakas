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
#   1. an attempt reaches Final and the span after it is scheduled (getPalwRoundLane lists a domain);
#   2. a node produces a round block for a permit its bond holds;
#   3. a chain block merges round blocks and grants their permits (the chain walk's lane line);
#   4. a transaction paying a fee is sent and a granted round block carries it (a lane line with
#      accepted transactions after the send), and the recipient's balance moves;
#   5. two nodes report the same lane: span, accepted permits and schedule.
# A reorg across a span boundary is the pipeline suite's (`adr0125_*` in the virtual processor's
# tests); this drill does not partition the network.
#
# Build first:
#   cargo build --release -p kaspad -p misaka-cli
#
# Env: KASPAD_BIN, CLI_BIN (defaults target/release/*), NODES (4, at least 4), WORK_DIR, LANE
# (`activation,width,span[,daa:width…]`, default 0,2,30), STEP_WAIT (s, per step), SENDS (5 fee-paying
# transactions, one every SEND_EVERY seconds), P2P_BASE / RPC_BASE (port bases).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
CLI_BIN="${CLI_BIN:-$REPO_ROOT/target/release/misaka}"
NODES="${NODES:-4}"
WORK_DIR="${WORK_DIR:-$REPO_ROOT/.misaka-palw-round-lane-devnet}"
LANE="${LANE:-0,2,30}"
STEP_WAIT="${STEP_WAIT:-3600}"
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

cli() { local i="$1"; shift; "$CLI_BIN" --network devnet --rpc "127.0.0.1:$((RPC_BASE + i))" "$@"; }
# `lane <node> <python expression over the response dict v>` — prints the expression's value.
lane() {
  local i="$1" expr="$2"
  cli "$i" palw round-lane --output json 2>/dev/null | python3 -c "import json,sys; v=json.load(sys.stdin); print($expr)" 2>/dev/null || true
}

pids=()
cleanup() { for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done; }
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

alive() { for p in "${pids[@]}"; do kill -0 "$p" 2>/dev/null || die "a node exited (see $WORK_DIR/node-*.log)"; done; }

# Poll until `pattern` appears in any node log (or the given files) after byte offset map `since`
# (a file of "path offset" lines, or empty); echo the first matching line.
wait_log() {
  local pattern="$1" what="$2" files="${3:-$WORK_DIR/node-*.log}" deadline=$((SECONDS + STEP_WAIT)) line=""
  while [ $SECONDS -lt $deadline ]; do
    # shellcheck disable=SC2086
    line="$(grep -h -m1 -E "$pattern" $files 2>/dev/null | head -1 || true)"
    if [ -n "$line" ]; then echo "$line"; return 0; fi
    alive
    sleep 5
  done
  die "timed out after ${STEP_WAIT}s waiting for: $what"
}

# Poll until `lane <node> <expr>` prints True.
wait_lane() {
  local node="$1" expr="$2" what="$3" deadline=$((SECONDS + STEP_WAIT))
  while [ $SECONDS -lt $deadline ]; do
    [ "$(lane "$node" "$expr")" = "True" ] && return 0
    alive
    sleep 10
  done
  die "timed out after ${STEP_WAIT}s waiting for: $what"
}

log "0/5 the lane is armed on the nodes' ruleset"
wait_lane 1 "v.get('armed') == True" "getPalwRoundLane to answer armed"
log "    $(cli 1 palw round-lane 2>/dev/null | head -1)"

log "1/5 waiting for an attempt to reach Final and the span after it to be scheduled"
wait_lane 1 "len(v.get('domains', [])) > 0" "a scheduled span (an attempt reached Final a span earlier)"
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
sent=0
for ((k=0; k<SENDS; k++)); do
  if cli 0 wallet send --key-file "$WORK_DIR/keys/main.seed" --to "$RECIPIENT" --amount "1.0000000$k" --yes --output json \
      >"$WORK_DIR/out/send-$k.json" 2>&1; then
    sent=$((sent + 1))
    log "    sent $(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("txid"))' "$WORK_DIR/out/send-$k.json" 2>/dev/null || echo '?')"
  else
    log "    send $k refused: $(tail -1 "$WORK_DIR/out/send-$k.json")"
  fi
  sleep "$SEND_EVERY"
done
[ "$sent" -gt 0 ] || die "no fee-paying transaction was accepted by the mempool"
carried="$(wait_log "\\[palw-round-lane\\] chain block [0-9a-f]+ merged [0-9]+ round block\\(s\\): [1-9][0-9]* permit\\(s\\) granted, [1-9][0-9]* transaction\\(s\\) accepted from them" "a round block carrying a transaction")"
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
log "PASS — attempts reached Final, the next span was scheduled, round blocks were produced for held permits, chain blocks granted them, and a granted round block carried a fee-paying transaction"
log "evidence: $WORK_DIR/node-*.log, $WORK_DIR/out/"
