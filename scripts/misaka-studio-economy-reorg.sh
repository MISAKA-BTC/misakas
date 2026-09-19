#!/usr/bin/env bash
# misaka-studio-economy-reorg.sh — the chain-level REORG the economy's state has to survive, on the
# devnet a finished `misaka-studio-economy-e2e.sh` run left on disk.
#
# The unit property (`audit_a_reorg_and_an_ibd_agree_with_a_fresh_walk_past_the_economic_bundle`)
# reverts deltas by hand. This runs the node's own reorg: the eight validators restart from the E2E's
# datadirs split into two partitions of four, both sides extend their own selected chain past the
# bundle, then the partitions are joined and one side's selected chain is replaced by the other's —
# every PALW delta of the losing branch reverted and the winning branch applied, with the losing
# branch's attempt blocks merged rather than chained (ADR-0058; ADR-0149 §6 is what lets those
# merged attempts be folded past the bundle at all).
#
#   1/4  partition A (nodes 0–3) and partition B (nodes 4–7) from the same datadirs, each on its own hub
#   2/4  both sides produce past the split; their sinks differ (a real fork)
#   3/4  node-4 re-joins node-0; every node converges on ONE sink, and at least one side's old sink is
#        no longer on the selected chain (`misaka node dag-info --chain-from`: chain blocks removed)
#   4/4  every node holds the same registry rows and the same state for the E2E's free-prompt claim,
#        and no merged attempt past the bundle was skipped as "not the derived" pwu
#
# Env: the E2E's own (WORK_DIR, BIN_DIR, NODES=8, fence heights, RAM_SCALE, MISAKA_PALW_ARTIFACT,
# MODEL_ID, P2P_BASE, RPC_BASE) — node arguments must be the E2E's byte for byte, or the partitions
# would be running a different ruleset from the chain they restart. PART_DAA (default 8): how far
# each side advances alone. JOIN_WAIT (default 1800 s).
set -u
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${BIN_DIR:-$REPO_ROOT/target/release}"
KASPAD_BIN="${KASPAD_BIN:-$BIN_DIR/kaspad}"
CLI_BIN="${CLI_BIN:-$BIN_DIR/misaka}"
MODEL_ID="${MODEL_ID:-Qwen/Qwen2.5-1.5B/graph-v5@512}"
NODES="${NODES:-8}"
ECONOMY_AT="${ECONOMY_AT:-30}"
REGISTRY_AT="${REGISTRY_AT:-$ECONOMY_AT}"
WORK_TARGET_AT="${WORK_TARGET_AT:-$REGISTRY_AT}"
PAYOUT_AT="${PAYOUT_AT:-$REGISTRY_AT}"
OWNERSHIP_AT="${OWNERSHIP_AT:-$ECONOMY_AT}"
LANE="${LANE:-0,2,2}"
RAM_SCALE="${RAM_SCALE:-0.3}"
WORK_DIR="${WORK_DIR:?WORK_DIR must name a finished misaka-studio-economy-e2e.sh run}"
P2P_BASE="${P2P_BASE:-17010}"
RPC_BASE="${RPC_BASE:-18310}"
PART_DAA="${PART_DAA:-8}"
JOIN_WAIT="${JOIN_WAIT:-1800}"
PREMINE_TXID="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"
MAIN_PREMINE_INDEX=40
HALF=$((NODES / 2))

log() { printf '[studio-reorg %s] %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die() { log "FATAL: $*"; exit 1; }

[ -x "$KASPAD_BIN" ] && [ -x "$CLI_BIN" ] || die "missing binaries in $BIN_DIR"
"$CLI_BIN" node dag-info --help >/dev/null 2>&1 || die "this misaka has no \`node dag-info\`"
[ -n "${MISAKA_PALW_ARTIFACT:-}" ] && [ -f "$MISAKA_PALW_ARTIFACT" ] || die "MISAKA_PALW_ARTIFACT must be the E2E's artifact"
for ((i=0; i<NODES; i++)); do [ -d "$WORK_DIR/node-$i" ] || die "$WORK_DIR/node-$i is not there: run the E2E first"; done
CLAIM_ID="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("claim_id",""))' "$WORK_DIR/out/claim-final.json" 2>/dev/null || true)"
[ -n "$CLAIM_ID" ] || die "the E2E left no claim id in $WORK_DIR/out/claim-final.json"

pids=()
cleanup() { for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT
cli() { local i="$1"; shift; "$CLI_BIN" --network devnet --rpc "127.0.0.1:$((RPC_BASE + i))" "$@"; }
dag() { cli "$1" node dag-info --output json ${2:+--chain-from "$2"} 2>/dev/null; }
sink_of() { dag "$1" | python3 -c "import json,sys; print(json.load(sys.stdin)['sink'])" 2>/dev/null || true; }
daa_of() { dag "$1" | python3 -c "import json,sys; print(json.load(sys.stdin)['virtual_daa'])" 2>/dev/null || true; }
rows() {
  cli "$1" palw registry --output json 2>/dev/null | python3 -c "
import json,sys
v=json.load(sys.stdin)['registry']
print(sorted((c['classId'], c['state'] if c['hasRow'] else 'legacy', c['economicCcuPerClaim']) for c in v.get('classes', [])))" 2>/dev/null || true
}
claim_state() {
  cli "$1" palw derived --json "$CLAIM_ID" 2>/dev/null | python3 -c "
import json,sys
d=json.load(sys.stdin)
print((d.get('claim_phase'), d.get('quanta'), d.get('quanta_spent'), d.get('work_leaves')))" 2>/dev/null || true
}

# The E2E's node arguments, byte for byte, but for WHICH hub a node connects to.
node_args() {
  local i="$1" hub="$2" addr
  addr="$(cat "$WORK_DIR/keys/bond-$i.address")"
  local args=(--devnet --appdir="$WORK_DIR/node-$i" --listen="127.0.0.1:$((P2P_BASE + i))" --rpclisten-borsh="127.0.0.1:$((RPC_BASE + i))"
        --utxoindex --nodnsseed --disable-upnp --nogrpc --enable-unsynced-mining --palw-devnet-floor-only --ram-scale="$RAM_SCALE"
        --palw-execution-lane-devnet="$LANE" --palw-model-registry-devnet="$REGISTRY_AT"
        --palw-economic-payout-devnet="$PAYOUT_AT" --palw-work-target-devnet="$WORK_TARGET_AT"
        --palw-artifact-root-ownership-devnet="$OWNERSHIP_AT"
        --palw-canonical-work-devnet="$ECONOMY_AT" --palw-admission-independence-devnet="$ECONOMY_AT"
        --palw-fp-derived-work-devnet="$ECONOMY_AT"
        --palw-produce --palw-panel --palw-round-lane --palw-class-artifact="$MISAKA_PALW_ARTIFACT"
        --palw-producer-key="$WORK_DIR/keys/bond-$i.seed" --palw-producer-bond="$PREMINE_TXID:$i"
        --palw-producer-pay-address="$addr" --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + i))")
  [ "$i" -eq 1 ] && args+=(--palw-register-class "$MODEL_ID")
  # EVERY node runs connect-only: a node with no --connect dials whatever its address book learned
  # during the E2E, which is the other partition. The hub dials a member of its own side.
  if [ "$i" -ne "$hub" ]; then args+=(--connect="127.0.0.1:$((P2P_BASE + hub))"); else args+=(--connect="127.0.0.1:$((P2P_BASE + hub + 1))"); fi
  printf '%s\n' "${args[@]}"
}
start_node() {
  local i="$1" hub="$2"
  local args=(); while IFS= read -r a; do args+=("$a"); done < <(node_args "$i" "$hub")
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >>"$WORK_DIR/node-$i.log" 2>&1 &
  echo $!
}
wait_until() {  # $1 predicate, $2 seconds, $3 what
  local deadline=$((SECONDS + $2))
  while [ $SECONDS -lt "$deadline" ]; do eval "$1" && return 0; sleep 10; done
  die "gave up waiting for: $3"
}

# ---------------------------------------------------------------------------------------------
log "1/4 partition A = nodes 0–$((HALF - 1)) (hub node-0), partition B = nodes $HALF–$((NODES - 1)) (hub node-$HALF)"
for ((i=0; i<NODES; i++)); do
  hub=0; [ "$i" -ge "$HALF" ] && hub="$HALF"
  pids[$i]="$(start_node "$i" "$hub")"
done
wait_until "[ -n \"\$(daa_of 0)\" ] && [ -n \"\$(daa_of $HALF)\" ]" 600 "both hubs to answer"
split_a="$(daa_of 0)"; split_b="$(daa_of "$HALF")"
log "    both partitions up: A at DAA $split_a, B at DAA $split_b"

log "2/4 each side advances $PART_DAA DAA alone"
wait_until "[ \"\$(daa_of 0)\" -ge $((split_a + PART_DAA)) ] && [ \"\$(daa_of $HALF)\" -ge $((split_b + PART_DAA)) ]" "$JOIN_WAIT" "both partitions to advance"
sink_a="$(sink_of 0)"; sink_b="$(sink_of "$HALF")"
[ -n "$sink_a" ] && [ -n "$sink_b" ] || die "a hub did not report its sink"
[ "$sink_a" != "$sink_b" ] || die "the two partitions report the same sink — there was no fork to reorganise"
log "    fork: A sink ${sink_a:0:16}… at DAA $(daa_of 0), B sink ${sink_b:0:16}… at DAA $(daa_of "$HALF")"

log "3/4 node-$HALF re-joins node-0; every node must converge on one sink"
kill "${pids[$HALF]}" 2>/dev/null || true; sleep 3
pids[$HALF]="$(start_node "$HALF" 0)"
converged() {
  local first i s
  first="$(sink_of 0)"; [ -n "$first" ] || return 1
  for ((i=1; i<NODES; i++)); do s="$(sink_of "$i")"; [ "$s" = "$first" ] || return 1; done
  return 0
}
wait_until converged "$JOIN_WAIT" "all $NODES nodes to agree on one sink"
final="$(sink_of 0)"
removed_a="$(dag 0 "$sink_a" | python3 -c "import json,sys; print(json.load(sys.stdin).get('removed_chain_blocks') or 0)" 2>/dev/null || echo 0)"
removed_b="$(dag "$HALF" "$sink_b" | python3 -c "import json,sys; print(json.load(sys.stdin).get('removed_chain_blocks') or 0)" 2>/dev/null || echo 0)"
log "    converged on ${final:0:16}… at DAA $(daa_of 0); chain blocks removed since A's sink: $removed_a, since B's sink: $removed_b"
[ "${removed_a:-0}" -gt 0 ] || [ "${removed_b:-0}" -gt 0 ] || die "neither side's selected chain was replaced — the join merged without a reorg, so this run proved nothing about one"

log "4/4 every node holds the same registry rows and the same free-prompt claim"
ref_rows="$(rows 0)"; ref_claim="$(claim_state 0)"
[ -n "$ref_rows" ] && [ -n "$ref_claim" ] || die "node-0 did not answer the registry or the claim"
for ((i=1; i<NODES; i++)); do
  wait_until "[ \"\$(rows $i)\" = \"\$ref_rows\" ] && [ \"\$(claim_state $i)\" = \"\$ref_claim\" ]" 600 "node-$i to agree with node-0 (rows and claim)"
done
skipped="$(grep -h "is not the derived" "$WORK_DIR"/node-*.log 2>/dev/null | wc -l | tr -d ' ')"
[ "$skipped" = 0 ] || die "$skipped merged-work skip(s) name a pwu that is 'not the derived' — ADR-0149 §6 regressed"
log "PASS — the partitions forked, one side's selected chain was replaced ($removed_a / $removed_b chain blocks), all $NODES nodes"
log "       converged on one sink and agree on the registry rows and the chat's claim $ref_claim; no merged attempt was skipped"
