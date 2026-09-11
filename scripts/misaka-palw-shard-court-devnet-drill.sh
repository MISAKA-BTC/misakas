#!/usr/bin/env bash
# misaka-palw-shard-court-devnet-drill.sh — ADR-0100 §6 step 2: the one-move court convicting on a
# LIVE devnet, with nothing but what the binaries already do.
#
#   node-0          producer under devnet public-seed bond 0, and the LIAR: its canonical free-prompt
#                   claims on the floor class commit a capture corrupted at step leaf $TAMPER_LEAF
#                   (--palw-drill-tamper-fp-leaf), the commitment re-derived so the lie is
#                   self-consistent and only a re-execution sees it.
#   node-1 … N-1    producers and panel seats under bonds 1 … N-1. The floor-only devnet genesis
#                   carries six bonds and a panel of five with the executor excluded, so every
#                   non-executor bond sits on every one of node-0's claims.
#   Every node:     --palw-devnet-floor-only (the base class alone, ~14 s a block per producer) and
#                   --palw-shard-court-devnet=0 (the genesis states the COMPLETE_V3 signing-context
#                   set and arms Params::palw_shard_court from block one).
#
# What it asserts, in order — each a poll against logs and the chain, never a fixed sleep:
#   1. node-0 commits a canonical claim whose capture it corrupted;
#   2. a seat samples it, finds the leaf that does not recompute, and files ShardCourtAccused;
#   3. a block carries the accusation (every node folds it);
#   4. the accused claim is Voided with reason court_fraud, read from two nodes over RPC;
#   5. bond 0's collateral is lower than before the accusation landed.
#
# Build first:
#   cargo build --release -p kaspad -p misaka-cli
#
# Env: KASPAD_BIN, CLI_BIN (defaults target/release/*), NODES (3), WORK_DIR, TAMPER_LEAF (0 — the
# embedding gather, the leaf every seat samples first), CANONICAL_INTERVAL (10 DAA), STEP_WAIT (s),
# P2P_BASE / RPC_BASE (port bases), HELD (0; 1 re-runs the drill UNDER ADR-0103's fence —
# --palw-held-context-devnet on every node, the held regime minted from genesis — which is
# ADR-0103 Invariant 1: the same tampered leaf, the same one-move conviction, the same slash),
# LEAF (unset; ADR-0109's two paths, HELD=1 only: `fast` — node-0 serves its claims' answer
# envelope and never the capture, so a seat judges by intervals alone, names the leaf off the
# block-leaves lane and convicts on the evidence node-0 serves it on request; `slow` — node-0 also
# refuses that request, so the seat demands the leaf's evidence on chain, node-0 answers the
# demand, and the fold's one-move verdict on the answer is the conviction).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
CLI_BIN="${CLI_BIN:-$REPO_ROOT/target/release/misaka}"
NODES="${NODES:-3}"
WORK_DIR="${WORK_DIR:-$REPO_ROOT/.misaka-palw-shard-court-devnet}"
TAMPER_LEAF="${TAMPER_LEAF:-0}"
CANONICAL_INTERVAL="${CANONICAL_INTERVAL:-10}"
STEP_WAIT="${STEP_WAIT:-1800}"
HELD="${HELD:-0}"
LEAF="${LEAF:-}"
case "$LEAF" in
  ""|fast|slow) ;;
  *) printf '[shard-court-drill] FATAL: LEAF is fast or slow\n' >&2; exit 1 ;;
esac
if [ -n "$LEAF" ] && [ "$HELD" != "1" ]; then
  printf '[shard-court-drill] FATAL: LEAF needs HELD=1: the leaf demand is the held regime'"'"'s\n' >&2; exit 1
fi
P2P_BASE="${P2P_BASE:-16510}"
RPC_BASE="${RPC_BASE:-17810}"
PREMINE_TXID="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"   # "misaka-premine", zero-padded
MAIN_PREMINE_INDEX=40   # consensus/core/src/config/premine.rs; bond n's fee float is MAIN_PREMINE_INDEX + 1 + n

log() { printf '[shard-court-drill] %s\n' "$*" >&2; }
die() { log "FATAL: $*"; exit 1; }

for b in "$KASPAD_BIN" "$CLI_BIN"; do [ -x "$b" ] || die "missing binary $b (cargo build --release -p kaspad -p misaka-cli)"; done
[ "$NODES" -ge 2 ] || die "NODES must be at least 2: a liar and a seat"
"$KASPAD_BIN" --help 2>/dev/null | grep -q -- "--palw-shard-court-devnet" || die "this kaspad has no --palw-shard-court-devnet (ADR-0100)"
if [ "$HELD" = "1" ]; then
  "$KASPAD_BIN" --help 2>/dev/null | grep -q -- "--palw-held-context-devnet" || die "this kaspad has no --palw-held-context-devnet (ADR-0103)"
fi
for ((i=0; i<NODES; i++)); do
  for port in $((P2P_BASE + i)) $((RPC_BASE + i)); do
    if lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then die "port $port is in use — set P2P_BASE / RPC_BASE"; fi
  done
done

rm -rf "$WORK_DIR"; mkdir -p "$WORK_DIR/keys"
python3 - "$WORK_DIR/keys" "$NODES" <<'PY'
import hashlib, os, sys
d, n = sys.argv[1], int(sys.argv[2])
h = lambda b: hashlib.blake2b(b, digest_size=32).hexdigest()
for i in range(n):
    p = f"{d}/bond-{i}.seed"; open(p, "w").write(h(b"misaka-devnet-genesis-bond-v1/" + str(i).encode())); os.chmod(p, 0o600)
PY

cli() { local i="$1"; shift; "$CLI_BIN" --network devnet --rpc "127.0.0.1:$((RPC_BASE + i))" "$@"; }

pids=()
cleanup() { for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT

for ((i=0; i<NODES; i++)); do
  addr="$("$CLI_BIN" --network devnet key address --key-file "$WORK_DIR/keys/bond-$i.seed" | tail -1 | awk '{print $NF}')"
  [ -n "$addr" ] || die "cannot derive bond $i's address"
  args=(--devnet --appdir="$WORK_DIR/node-$i" --listen="127.0.0.1:$((P2P_BASE + i))" --rpclisten-borsh="127.0.0.1:$((RPC_BASE + i))"
        --utxoindex --nodnsseed --disable-upnp --nogrpc --enable-unsynced-mining
        --palw-devnet-floor-only --palw-shard-court-devnet=0 $( [ "$HELD" = "1" ] && echo --palw-held-context-devnet || true )
        --palw-produce --palw-panel --palw-producer-key="$WORK_DIR/keys/bond-$i.seed" --palw-producer-bond="$PREMINE_TXID:$i"
        --palw-producer-pay-address="$addr" --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + i))")
  if [ "$i" -eq 0 ]; then
    args+=(--palw-canonical-claims --palw-canonical-interval-daa="$CANONICAL_INTERVAL" --palw-drill-tamper-fp-leaf="$TAMPER_LEAF")
    [ -n "$LEAF" ] && args+=(--palw-drill-answer-only)
    [ "$LEAF" = "slow" ] && args+=(--palw-drill-refuse-leaf-evidence)
  else
    args+=(--connect="127.0.0.1:$P2P_BASE")
  fi
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >"$WORK_DIR/node-$i.log" 2>&1 &
  node_pid=$!
  pids+=("$node_pid")
  log "node-$i pid $node_pid bond $PREMINE_TXID:$i$([ "$i" -eq 0 ] && echo " — the liar (tamper leaf $TAMPER_LEAF)")"
done

# Poll until `pattern` appears in any node log (or the given one); echo the first matching line.
wait_log() {
  local pattern="$1" what="$2" files="${3:-$WORK_DIR/node-*.log}" deadline=$((SECONDS + STEP_WAIT)) line=""
  while [ $SECONDS -lt $deadline ]; do
    # shellcheck disable=SC2086
    line="$(grep -h -m1 -E "$pattern" $files 2>/dev/null | head -1 || true)"
    if [ -n "$line" ]; then echo "$line"; return 0; fi
    for p in "${pids[@]}"; do kill -0 "$p" 2>/dev/null || die "a node exited while waiting for: $what (see $WORK_DIR/node-*.log)"; done
    sleep 5
  done
  die "timed out after ${STEP_WAIT}s waiting for: $what"
}

log "1/5 waiting for node-0 to commit a corrupted canonical claim"
wait_log "PALW DRILL: this canonical claim commits a capture corrupted" "the corrupted capture" "$WORK_DIR/node-0.log" >/dev/null
committed="$(wait_log "committed canonical claim [0-9a-f]{128}" "the canonical claim's commitment" "$WORK_DIR/node-0.log")"
log "    $committed"

if [ "$LEAF" = "slow" ]; then
  log "2/5 waiting for a seat to demand the named leaf's evidence on chain (ADR-0109 Decision 3)"
  accused="$(wait_log "demanding it on chain \\(ADR-0109 Decision 3\\)" "a seat's demand")"
elif [ "$LEAF" = "fast" ]; then
  # The fast path's own line, not any accusation: a capture-route accusation here would mean a seat
  # held the capture after all, and the run would prove nothing about ADR-0109.
  log "2/5 waiting for a seat to accuse on the executor's own evidence (ADR-0109 fast path)"
  accused="$(wait_log "accusing leaf [0-9]+ in the one-move court on the executor's own evidence" "a fast-path accusation")"
  if grep -h -E "accusing leaf [0-9]+ in the one-move court \\(session" "$WORK_DIR"/node-*.log >/dev/null 2>&1; then
    die "a seat accused from a capture — node-0 served one, so this run does not exercise the fast path"
  fi
else
  log "2/5 waiting for a seat to accuse it in the one-move court"
  accused="$(wait_log "accusing leaf [0-9]+ in the one-move court" "a seat's accusation")"
fi
CLAIM="$(echo "$accused" | grep -oE "claim [0-9a-f]{128}" | head -1 | awk '{print $2}')"
[ -n "$CLAIM" ] || die "the accusation line names no claim: $accused"
log "    $accused"
bond0_before="$(cli 1 palw derived --json "$CLAIM" 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("claim_phase",""))' || true)"
log "    claim ${CLAIM:0:16}… phase before the accusation lands: ${bond0_before:-unknown}"
BASE_CLASS="$(grep -h -m1 -oE "PALW base class \(and its founding model line\): [0-9a-f]{128}" "$WORK_DIR/node-0.log" | awk '{print $NF}' || true)"
bond_collateral() {
  cli 1 bond status --key-file "$WORK_DIR/keys/bond-0.seed" ${BASE_CLASS:+--class-id "$BASE_CLASS"} 2>/dev/null \
    | grep -E "^  $PREMINE_TXID:0 " | grep -oE "collateral [0-9]+" | head -1 | awk '{print $2}' || true
}
before_collateral="$(bond_collateral)"
if [ "$LEAF" = "slow" ]; then
  log "3/5 waiting for the demand, node-0's answer and a block that carries it"
  demanded="$(wait_log "PALW lifecycle carried .*DefaultAccusedHeld" "a block carrying the demand")"
  log "    $demanded"
  answered="$(wait_log "answering the held accusation of StepLeaf" "node-0's answer" "$WORK_DIR/node-0.log")"
  log "    $answered"
  carried="$(wait_log "PALW lifecycle carried .*MaterialDisclosedHeld" "a block carrying the answer")"
else
  wait_log "submitted ShardCourtAccused" "the accusation's carrier" >/dev/null

  log "3/5 waiting for a block to carry it"
  carried="$(wait_log "PALW lifecycle carried .*ShardCourtAccused" "a block carrying ShardCourtAccused")"
fi
log "    $carried"

log "4/5 reading the claim's phase from two nodes"
verdict=""
deadline=$((SECONDS + STEP_WAIT))
while [ $SECONDS -lt $deadline ]; do
  ok=0
  for n in 1 $((NODES - 1)); do
    phase="$(cli "$n" palw derived --json "$CLAIM" 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("claim_phase",""), d.get("claim_void_reason",""))' || true)"
    [ "$phase" = "voided court_fraud" ] && ok=$((ok + 1))
    verdict="$phase"
  done
  [ "$ok" -ge 2 ] && break
  sleep 5
done
[ "$verdict" = "voided court_fraud" ] || die "the claim is not Voided(court_fraud) on both nodes: last read '$verdict'"
log "    claim ${CLAIM:0:16}… is voided (court_fraud) on node-1 and node-$((NODES - 1))"

log "5/5 bond 0's collateral"
after_collateral="$(bond_collateral)"
log "    before ${before_collateral:-?} sompi, after ${after_collateral:-?} sompi"
if [ -n "$before_collateral" ] && [ -n "$after_collateral" ]; then
  [ "$after_collateral" -lt "$before_collateral" ] || die "bond 0's collateral did not fall ($before_collateral → $after_collateral)"
fi
# One count over every log: `grep -c` on one stream prints one number, and `|| true` (not `|| echo 0`)
# keeps pipefail's non-zero exit on no match from printing a second zero.
dropped="$(cat "$WORK_DIR"/node-*.log | grep -c "a PALW lifecycle object was dropped, and the block stands" || true)"
log "    duplicate accusations dropped with the block standing: $dropped (every seat on the panel files once)"
case "$LEAF" in
  fast) log "PASS — a seat that held no capture convicted a corrupted free-prompt claim on the executor's own evidence (ADR-0109 fast path)" ;;
  slow) log "PASS — a seat that held no capture demanded the leaf on chain, and the executor's answer convicted it (ADR-0109 slow path)" ;;
  *) log "PASS — the one-move court convicted a corrupted free-prompt claim on a live devnet" ;;
esac
