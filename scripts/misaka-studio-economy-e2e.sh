#!/usr/bin/env bash
# misaka-studio-economy-e2e.sh — ADR-0144's claim on a chain: a person's chat in MISAKA Studio IS the
# mining work, under the ADR-0145 economy, on a model the network admitted rather than one its
# registrant declared ready.
#
# One run, in order, with every piece the claim depends on armed at ONE height (ECONOMY_AT):
#   the model registry, the work target and the payout it needs, artifact-root ownership (ADR-0143),
#   and the ADR-0145 bundle — canonical work, independent admission, the free-prompt lane's derived
#   work. The registry at the bundle's own height is deliberate: it is the case ADR-0149 §5 found
#   stopping the chain (the first blocks past the fence are admitted against a state with no rows),
#   and no heartbeat miner runs here, so a regression of that fix wedges this drill at the fence.
#
#   1/9  N floor-only validators, every one holding the class artifact; the chain crosses the fence
#        and keeps producing (the §5 gate — the floor is priced before its row exists)
#   2/9  node-1 registers MODEL_ID past the registry fence. Accepted past the independence fence
#        (ECONOMY_AT) it is a BOUGHT class and must open CANDIDATE; accepted before it, the
#        pre-independence path (grandfathered) and PREFETCHING — the stage judges which by the fence
#   3/9  the seats prove possession; a CANDIDATE waits for a jury the NETWORK drew (ADR-0147), whose
#        first audit is at the first epoch boundary (DAA 1,000 on devnet: set STEP_WAIT_DAA past
#        it); it reaches PROBATION — registration is not eligibility, and eligibility is earned
#   4/9  the free-prompt lane is certified for the class's family (ADR-0075)
#   5/9  the gateway (bond 0) and MISAKA Studio (`misaka-studiod`, Gateway backend) come up
#   6/9  ONE chat, sent to STUDIO — not to the gateway — and the commitment it produced
#   7/9  the rail signs and submits it; the claim walks provisional → panel_bound →
#        receipt_licensed → final on EVERY node, with the outsider seat a bought class must seat
#   8/9  a receipt block spends one of its quanta, accepted by every node
#   9/9  a node that joins late syncs the whole chain from genesis — across the fence, the
#        registration, the jury and the claim — and holds the same rows and the same claim
#
# What this does NOT prove, said up front: it is a devnet (minutes-scale windows, not testnet-11's),
# it does not prosecute a court case, and a PASS says the pipeline reaches a receipt block, not
# that the answer was any good.
#
# Env (defaults in brackets):
#   BIN_DIR        where the chain's release binaries are [target/release]
#   STUDIO_BIN     misaka-studiod from MISAKA-Studio [required]
#   MISAKA_PALW_ARTIFACT, MISAKA_PALW_TOKENIZER   the class artifact (tokenizer-bound) and its tokenizer
#   MODEL_ID       [Qwen/Qwen2.5-1.5B/graph-v5@512]
#   NODES [8] — every devnet genesis bond live: a panel seats 5, the registry wants seat_count + 2
#                ready seats, and a bond nobody runs can be drawn as a bought class's OUTSIDER, whose
#                silence voids the claim (ADR-0147 §2.1). Eight nodes at ~0.2 GiB each (ADR-0136).
#   ECONOMY_AT [30]  REGISTRY_AT/WORK_TARGET_AT/PAYOUT_AT/OWNERSHIP_AT [= ECONOMY_AT]  LANE [0,2,2]
#   RAM_SCALE [0.3]  MIN_FREE_GB [4] (the run stops itself below it)  WORK_DIR  P2P_BASE  RPC_BASE
#   PROMPT, MAX_TOKENS [16]  STEP_WAIT_DAA [400]  STALL_WAIT [1200]
#   GATEWAY_PUBLIC_BUDGET_PERMILLE [1000] — the share of bond 0's room the gateway's jobs may reserve
#                per 24 h. The gateway's default (200) is for a gateway strangers use; here the
#                operator IS the person chatting, as on the Studio pool (contrib/minerpool/run-fp.sh
#                runs 1000). Past the bundle one claim reserves the compute era's exposure, and 200‰
#                of a devnet genesis bond's room is smaller than ONE claim (run4, 2026-09-20:
#                147,880,590 sompi per claim against a 110,000,868 budget) — no job could commit.
set -u
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${BIN_DIR:-$REPO_ROOT/target/release}"
KASPAD_BIN="${KASPAD_BIN:-$BIN_DIR/kaspad}"
CLI_BIN="${CLI_BIN:-$BIN_DIR/misaka}"
CERTIFY_BIN="${CERTIFY_BIN:-$BIN_DIR/palw-certify}"
GATEWAY_BIN="${GATEWAY_BIN:-$BIN_DIR/misaka-palw-gateway}"
WORKER_BIN="${WORKER_BIN:-$BIN_DIR/palw-a16-fp-worker}"
RAIL_BIN="${RAIL_BIN:-$BIN_DIR/misaka-palw-fp-rail}"
CLASS_BIN="${CLASS_BIN:-$BIN_DIR/palw-class}"
STUDIO_BIN="${STUDIO_BIN:-}"
MODEL_ID="${MODEL_ID:-Qwen/Qwen2.5-1.5B/graph-v5@512}"
NODES="${NODES:-8}"
ECONOMY_AT="${ECONOMY_AT:-30}"
REGISTRY_AT="${REGISTRY_AT:-$ECONOMY_AT}"
WORK_TARGET_AT="${WORK_TARGET_AT:-$REGISTRY_AT}"
PAYOUT_AT="${PAYOUT_AT:-$REGISTRY_AT}"
OWNERSHIP_AT="${OWNERSHIP_AT:-$ECONOMY_AT}"
LANE="${LANE:-0,2,2}"
RAM_SCALE="${RAM_SCALE:-0.3}"
MIN_FREE_GB="${MIN_FREE_GB:-4}"
WORK_DIR="${WORK_DIR:-$REPO_ROOT/.misaka-studio-economy-e2e}"
P2P_BASE="${P2P_BASE:-17010}"
RPC_BASE="${RPC_BASE:-18310}"
GATEWAY_PORT="${GATEWAY_PORT:-18895}"
STUDIO_PORT="${STUDIO_PORT:-18896}"
GATEWAY_PUBLIC_BUDGET_PERMILLE="${GATEWAY_PUBLIC_BUDGET_PERMILLE:-1000}"
PROMPT="${PROMPT:-In one sentence: what does a hash function do?}"
MAX_TOKENS="${MAX_TOKENS:-16}"
STEP_WAIT_DAA="${STEP_WAIT_DAA:-400}"
STALL_WAIT="${STALL_WAIT:-1200}"
STEP_WAIT_CEILING="${STEP_WAIT_CEILING:-43200}"
PREMINE_TXID="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"
MAIN_PREMINE_INDEX=40
BOND_FEE_FLOAT_SOMPI=10000000000
IBD_NODE="$NODES"

log() { printf '[studio-e2e %s] %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die() { log "FATAL: $*"; exit 1; }

# ---------------------------------------------------------------------------------------------
# Preflight — every refusal by name, before anything starts.
# ---------------------------------------------------------------------------------------------
for b in "$KASPAD_BIN" "$CLI_BIN" "$CERTIFY_BIN" "$GATEWAY_BIN" "$WORKER_BIN" "$RAIL_BIN" "$CLASS_BIN"; do
  [ -x "$b" ] || die "$b is not an executable (cargo build --release for it)"
done
[ -n "$STUDIO_BIN" ] && [ -x "$STUDIO_BIN" ] || die "STUDIO_BIN must name misaka-studiod (cargo build --release -p misaka-studio-runtime --bin misaka-studiod in MISAKA-Studio)"
[ -n "${MISAKA_PALW_ARTIFACT:-}" ] && [ -f "$MISAKA_PALW_ARTIFACT" ] || die "MISAKA_PALW_ARTIFACT must name the tokenizer-bound class artifact"
[ -n "${MISAKA_PALW_TOKENIZER:-}" ] && [ -f "$MISAKA_PALW_TOKENIZER" ] || die "MISAKA_PALW_TOKENIZER must name the artifact's tokenizer.json"
command -v python3 >/dev/null || die "python3 is required"
for knob in palw-model-registry-devnet palw-work-target-devnet palw-economic-payout-devnet palw-artifact-root-ownership-devnet \
            palw-canonical-work-devnet palw-admission-independence-devnet palw-fp-derived-work-devnet palw-execution-lane-devnet; do
  "$KASPAD_BIN" --help 2>/dev/null | grep -q -- "--$knob" || die "this kaspad has no --$knob: the binary is older than the source tree"
done
free_gb() { df -g "$(dirname "$WORK_DIR")" | awk 'NR==2 {print $4}'; }
[ "$(free_gb)" -ge $((MIN_FREE_GB + 4)) ] || die "only $(free_gb) GiB free on the volume; this run needs $((MIN_FREE_GB + 4)) to start"
port_busy() { lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1; }
for ((i=0; i<=NODES; i++)); do for port in $((P2P_BASE + i)) $((RPC_BASE + i)); do port_busy "$port" && die "port $port is in use"; done; done
for port in "$GATEWAY_PORT" "$STUDIO_PORT"; do port_busy "$port" && die "port $port is in use"; done

EXPECTED_CLASS_ID=$("$CLASS_BIN" ledger --network devnet 2>/dev/null \
  | awk -v want="$MODEL_ID" '$1 == want {found=1; next} found && $1 == "class" && $2 == "id" {print $3; exit}')
[ -n "$EXPECTED_CLASS_ID" ] || die "palw-class ledger names no class id for $MODEL_ID on devnet"

rm -rf "$WORK_DIR"; mkdir -p "$WORK_DIR/keys" "$WORK_DIR/obj" "$WORK_DIR/outbox" "$WORK_DIR/out" "$WORK_DIR/studio"
python3 - "$WORK_DIR/keys" "$NODES" <<'PY'
import hashlib, os, sys
d, n = sys.argv[1], int(sys.argv[2])
h = lambda b: hashlib.blake2b(b, digest_size=32).hexdigest()
for i in range(n):
    p = f"{d}/bond-{i}.seed"; open(p, "w").write(h(b"misaka-devnet-genesis-bond-v1/" + str(i).encode())); os.chmod(p, 0o600)
p = f"{d}/main.seed"; open(p, "w").write(h(b"misaka-testnet-premine-9b-claude-managed")); os.chmod(p, 0o600)
PY

pids=()
cleanup() { for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT

cli() { local i="$1"; shift; "$CLI_BIN" --network devnet --rpc "127.0.0.1:$((RPC_BASE + i))" "$@"; }
reg() {
  local i="$1" expr="$2"
  cli "$i" palw registry --output json 2>/dev/null | python3 -c "import json,sys; v=json.load(sys.stdin)['registry']; print($expr)" 2>/dev/null || true
}
daa_of() { cli "$1" palw round-lane --output json 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin).get('virtualDaa',''))" 2>/dev/null || true; }

node_args() {
  local i="$1" addr="$2" producer="${3:-1}"
  local args=(--devnet --appdir="$WORK_DIR/node-$i" --listen="127.0.0.1:$((P2P_BASE + i))" --rpclisten-borsh="127.0.0.1:$((RPC_BASE + i))"
        --utxoindex --nodnsseed --disable-upnp --nogrpc --enable-unsynced-mining --palw-devnet-floor-only --ram-scale="$RAM_SCALE"
        --palw-execution-lane-devnet="$LANE" --palw-model-registry-devnet="$REGISTRY_AT"
        --palw-economic-payout-devnet="$PAYOUT_AT" --palw-work-target-devnet="$WORK_TARGET_AT"
        --palw-artifact-root-ownership-devnet="$OWNERSHIP_AT"
        --palw-canonical-work-devnet="$ECONOMY_AT" --palw-admission-independence-devnet="$ECONOMY_AT"
        --palw-fp-derived-work-devnet="$ECONOMY_AT")
  if [ "$producer" = 1 ]; then
    args+=(--palw-produce --palw-panel --palw-round-lane --palw-class-artifact="$MISAKA_PALW_ARTIFACT"
           --palw-producer-key="$WORK_DIR/keys/bond-$i.seed" --palw-producer-bond="$PREMINE_TXID:$i"
           --palw-producer-pay-address="$addr" --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + i))")
    [ "$i" -eq 1 ] && args+=(--palw-register-class "$MODEL_ID")
  fi
  [ "$i" -gt 0 ] && args+=(--connect="127.0.0.1:$P2P_BASE")
  printf '%s\n' "${args[@]}"
}
start_node() {
  local i="$1" producer="${2:-1}" addr=""
  [ "$producer" = 1 ] && addr="$(cat "$WORK_DIR/keys/bond-$i.address")"
  local args=(); while IFS= read -r a; do args+=("$a"); done < <(node_args "$i" "$addr" "$producer")
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >>"$WORK_DIR/node-$i.log" 2>&1 &
  echo $!
}
alive() { for p in "${pids[@]}"; do [ -z "$p" ] || kill -0 "$p" 2>/dev/null || die "a process exited (see $WORK_DIR/*.log)"; done; }
guard() {
  local f; f="$(free_gb)"
  [ "${f:-0}" -ge "$MIN_FREE_GB" ] || die "the volume is down to ${f} GiB free (floor $MIN_FREE_GB): stopping before the host runs out"
}

# DAA-denominated budgets with a stall guard (the registry drill's rule: a step waits in the units the
# rules measure, and only a STOPPED clock is measured in seconds).
step_start() { step_began=$SECONDS; step_began_daa="$(daa_of 1)"; last_daa="$step_began_daa"; last_move=$SECONDS; }
step_expired() {
  local daa; daa="$(daa_of 1)"
  if [ -n "$daa" ] && [ "$daa" != "$last_daa" ]; then last_daa="$daa"; last_move=$SECONDS; fi
  [ $((SECONDS - last_move)) -ge "$STALL_WAIT" ] && return 0
  if [ -n "${step_began_daa:-}" ] && [ -n "$last_daa" ]; then
    [ $((last_daa - step_began_daa)) -ge "$STEP_WAIT_DAA" ] && return 0
  fi
  [ $((SECONDS - step_began)) -ge "$STEP_WAIT_CEILING" ]
}
gave_up() { die "gave up waiting for: $1 (virtual DAA ${last_daa:-?}, unmoved for $((SECONDS - last_move))s, $((SECONDS - step_began))s into the step)"; }
wait_for() {  # $1 = shell predicate, $2 = what
  step_start
  while ! step_expired; do
    eval "$1" && return 0
    alive; guard; sleep 10
  done
  gave_up "$2"
}
wait_reg() { wait_for "[ \"\$(reg 1 \"$1\")\" = True ]" "$2"; }

# ---------------------------------------------------------------------------------------------
# 1/9  The validators, and the fence crossed with the chain still producing.
# ---------------------------------------------------------------------------------------------
log "class $MODEL_ID = ${EXPECTED_CLASS_ID:0:16}… · registry/work target/payout at $REGISTRY_AT, ownership at $OWNERSHIP_AT, bundle at $ECONOMY_AT · $NODES nodes"
for ((i=0; i<NODES; i++)); do
  addr="$("$CLI_BIN" --network devnet key address --key-file "$WORK_DIR/keys/bond-$i.seed" | tail -1 | awk '{print $NF}')"
  [ -n "$addr" ] || die "cannot derive bond $i's address"
  echo "$addr" > "$WORK_DIR/keys/bond-$i.address"
  pids+=("$(start_node "$i")")
done
log "1/9 $NODES nodes up; waiting for the registry to be scheduled and the fence to be crossed"
wait_reg "v.get('scheduled') == True and v.get('fenceDaa') == $REGISTRY_AT" "the registry scheduled at $REGISTRY_AT"
wait_reg "v.get('active') == True and any(c.get('isBaseClass') and c.get('hasRow') and c.get('state') == 'Active' for c in v.get('classes', []))" "the base class's row, ACTIVE"
cross_daa="$(daa_of 1)"
wait_for "[ -n \"\$(daa_of 1)\" ] && [ \"\$(daa_of 1)\" -ge $((ECONOMY_AT + 10)) ]" "the chain to produce 10 DAA past the bundle at $ECONOMY_AT"
for ((i=0; i<NODES; i++)); do
  d="$(daa_of "$i")"; [ -n "$d" ] && [ "$d" -gt "$ECONOMY_AT" ] || die "node-$i is at daa ${d:-?}, not past the bundle: the fence partitioned the fleet"
done
floor_attempts_past=$(grep -h "produced block #" "$WORK_DIR"/node-*.log 2>/dev/null | wc -l | tr -d ' ')
log "1/9 OK — fence crossed at $cross_daa, the chain is at $(daa_of 1) with every node past $ECONOMY_AT ($floor_attempts_past floor blocks produced so far, no heartbeat miner running)"

# ---------------------------------------------------------------------------------------------
# 2/9  The class registered past the fence: bought, so CANDIDATE.
# ---------------------------------------------------------------------------------------------
wait_reg "any(c['classId'] == '$EXPECTED_CLASS_ID' and c.get('hasRow') for c in v.get('classes', []))" "node-1's registration of $MODEL_ID to open a row"
CLASS_ID="$EXPECTED_CLASS_ID"
first_state="$(reg 1 "[c['state'] for c in v['classes'] if c['classId'] == '$CLASS_ID'][0]")"
# **Which door the class came through is the fence's, not the drill's wish.** A registration the
# chain accepted past the independence fence (the bundle, ECONOMY_AT) is a BOUGHT class and opens
# CANDIDATE — only a network-drawn jury moves it (ADR-0147). One accepted before it keeps the
# pre-independence path and opens PREFETCHING (grandfathered). The row's opening span brackets the
# acceptance DAA; a span that straddles the fence is reported, not judged.
opened_from="$(reg 1 "[c['sinceSpan'] * v['spanDaa'] for c in v['classes'] if c['classId'] == '$CLASS_ID'][0]")"
span_daa="$(reg 1 "v['spanDaa']")"
if [ "$opened_from" -ge "$ECONOMY_AT" ]; then
  case "$first_state" in
    Candidate*) door="bought past the independence fence at $ECONOMY_AT: CANDIDATE, as ADR-0147 requires" ;;
    *) die "the class was registered at DAA ≥ $opened_from, past the independence fence at $ECONOMY_AT, and opened $first_state — a bought class must open CANDIDATE" ;;
  esac
elif [ $((opened_from + span_daa)) -le "$ECONOMY_AT" ]; then
  case "$first_state" in
    Candidate*) die "the class was registered before DAA $((opened_from + span_daa)), before the independence fence at $ECONOMY_AT, and opened CANDIDATE — a pre-independence registration keeps its path" ;;
    *) door="registered before the independence fence at $ECONOMY_AT: the pre-independence path (grandfathered), no jury" ;;
  esac
else
  door="the opening span straddles the independence fence at $ECONOMY_AT; opened $first_state"
fi
log "2/9 OK — ${CLASS_ID:0:16}… is on the chain with a row, opened as $first_state ($door)"

# ---------------------------------------------------------------------------------------------
# 3/9  Possession proved, a network-drawn jury, and PROBATION.
# ---------------------------------------------------------------------------------------------
req="$(reg 1 "[c['requiredReadySeats'] for c in v['classes'] if c['classId'] == '$CLASS_ID'][0]")"
[ -n "$req" ] && [ "$req" -le "$NODES" ] || die "the class needs $req ready seats and this run has $NODES nodes: it can never leave PREFETCHING"
case "$first_state" in
  Candidate*) log "3/9 waiting for the network-drawn jury (the first audit is at the first epoch boundary) and PROBATION (needs $req ready seats)" ;;
  *) log "3/9 waiting for PROBATION (needs $req ready seats; a grandfathered class sits no jury)" ;;
esac
if [[ "$first_state" == Candidate* ]]; then
  # Only an audit moves a CANDIDATE: once per period (epoch_length / span_daa spans), seeded by the
  # execution anchor of the span before, drawn from bonds that are neither the registrant's bond nor
  # its operator's, and holding only if a majority of that jury holds the class (ADR-0147).
  wait_reg "any(c['classId'] == '$CLASS_ID' and not c['state'].startswith('Candidate') for c in v['classes'])" "the admission jury to move $MODEL_ID out of CANDIDATE"
  log "    the jury held the class: $(reg 1 "[(c['state'], 'since span %d = DAA %d' % (c['sinceSpan'], c['sinceSpan'] * v['spanDaa']), 'ready seats %d' % c['readySeatsNow']) for c in v['classes'] if c['classId'] == '$CLASS_ID'][0]")"
fi
wait_reg "any(c['classId'] == '$CLASS_ID' and c['state'].startswith(('Probation', 'ActiveLimited', 'Active')) for c in v['classes'])" "$MODEL_ID to reach PROBATION"
log "3/9 OK — $(reg 1 "[(c['state'], c['readySeatsNow'], c['admissionMilli']) for c in v['classes'] if c['classId'] == '$CLASS_ID'][0]") (state, ready seats, admission milli)"

# ---------------------------------------------------------------------------------------------
# 4/9  The free-prompt lane certified for the class's family.
# ---------------------------------------------------------------------------------------------
submit_obj() {
  local f="$1" args=()
  if ls "$f".chunk* >/dev/null 2>&1; then
    for c in $(ls "$f".chunk* | sort -t k -k3 -n); do args+=(--object "$c"); done
  else
    args=(--object "$f")
  fi
  cli 0 palw submit-object --key-file "$WORK_DIR/keys/main.seed" "${args[@]}" --yes >>"$WORK_DIR/submit.log" 2>&1 \
    || die "submitting $(basename "$f") failed (see $WORK_DIR/submit.log)"
}
log "4/9 certifying $MODEL_ID's family on the free-prompt lane"
"$CERTIFY_BIN" drill --model-id "$MODEL_ID" --lane fp --out "$WORK_DIR/obj/fp-family.obj" >"$WORK_DIR/certify.log" 2>&1 || die "the family drill failed (certify.log)"
submit_obj "$WORK_DIR/obj/fp-family.obj"
chunks=$(ls "$WORK_DIR"/obj/fp-family.obj.chunk* 2>/dev/null | wc -l | tr -d ' '); [ "$chunks" -gt 0 ] || chunks=1
# One carrier lands per block and the binding is dropped (`NoCertifiedFamilyCovers`) if it arrives
# before the family's last chunk, so the bind waits out the family's carriers first.
fam_from="$(daa_of 1)"
wait_for "[ \"\$(daa_of 1)\" -ge $((fam_from + 2 * chunks + 2)) ]" "the family's $chunks carrier(s) to land"
"$CERTIFY_BIN" bind --model-id "$MODEL_ID" --lane fp --out "$WORK_DIR/obj/fp-bind.obj" >>"$WORK_DIR/certify.log" 2>&1 || die "palw-certify bind refused (certify.log)"
submit_obj "$WORK_DIR/obj/fp-bind.obj"
wait_for "grep -q 'ClassLaneCertified' '$WORK_DIR/node-1.log'" "the class to be bound to the free-prompt lane"
log "4/9 OK — the free-prompt lane is certified for ${CLASS_ID:0:16}…"

# ---------------------------------------------------------------------------------------------
# 5/9  The gateway under bond 0, and MISAKA Studio pointed at it.
# ---------------------------------------------------------------------------------------------
# **The genesis is read off the node, never typed in.** A fresh node imports "the UTXO set of the
# pruning point" before it has any other block, and that pruning point IS the genesis. A guessed value
# does not fail loudly: it produces claims whose context hash no seat can reproduce (the FP drill's
# note), so a value the chain printed beats a constant somebody copied.
GENESIS_HASH="$(grep -m1 -oE 'Importing the UTXO set of the pruning point [0-9a-f]{128}' "$WORK_DIR/node-0.log" | grep -oE '[0-9a-f]{128}' || true)"
[ -n "$GENESIS_HASH" ] || GENESIS_HASH="${MISAKA_DEVNET_GENESIS:-}"
[ "${#GENESIS_HASH}" -eq 128 ] || die "could not read the devnet genesis hash from node-0.log; set MISAKA_DEVNET_GENESIS"
log "    genesis ${GENESIS_HASH:0:16}… (from node-0's own first pruning point)"
NETWORK_DOMAIN=$(python3 - "$GENESIS_HASH" <<'PY'
import hashlib, struct, sys
net = b"devnet"; genesis = bytes.fromhex(sys.argv[1])
h = hashlib.blake2b(digest_size=64, key=b"misaka-palw/attempt-v2/network-domain/v1")
h.update(struct.pack("<Q", len(net))); h.update(net); h.update(genesis)
print(h.hexdigest())
PY
)
EXEC_PUBKEY=$("$RAIL_BIN" --bond-key-seed "$WORK_DIR/keys/bond-0.seed" --print-bond-pubkey | python3 -c 'import json,sys; print(json.load(sys.stdin)["executor_pubkey"])') \
  || die "cannot read bond 0's public key from the rail"
OPERATOR_ID=$(python3 - <<'PY'
import hashlib, struct
pk = b"misaka-devnet-operator-0"
h = hashlib.blake2b(digest_size=64, key=b"misaka-palw/state-v2/operator-id/v1")
h.update(struct.pack("<Q", len(pk))); h.update(pk)
print(h.hexdigest())
PY
)
cat >"$WORK_DIR/identity.json" <<JSON
{"network_domain": "$NETWORK_DOMAIN", "class_id": "$CLASS_ID", "bond_txid": "$PREMINE_TXID", "bond_index": 0,
 "executor_pubkey": "$EXEC_PUBKEY", "operator_id": "$OPERATOR_ID"}
JSON
MISAKA_PALW_ARTIFACT="$MISAKA_PALW_ARTIFACT" MISAKA_PALW_TOKENIZER="$MISAKA_PALW_TOKENIZER" \
MISAKA_PALW_GATEWAY_LOG_WORKER_STDERR=1 MISAKA_PALW_NETWORK_ID="devnet" \
"$GATEWAY_BIN" --listen "127.0.0.1:$GATEWAY_PORT" --worker "$WORKER_BIN" --outbox "$WORK_DIR/outbox" \
  --identity "$WORK_DIR/identity.json" --rpc "127.0.0.1:$RPC_BASE" \
  --public-job-budget-permille "$GATEWAY_PUBLIC_BUDGET_PERMILLE" >"$WORK_DIR/gateway.log" 2>&1 &
pids+=($!)
wait_for "curl -fsS 'http://127.0.0.1:$GATEWAY_PORT/health' -o '$WORK_DIR/gateway-health.json' 2>/dev/null" "the gateway's /health"
# **The rail as a watcher, funded from what bond 0 has EARNED.** Its genesis fee float is node-0's
# own fee outpoint, which that node's panel spends on readiness proofs and receipts long before the
# chat; a one-shot rail pointed at it finds it spent. The watcher selects the bond's own spendable
# outputs — node-0's floor rewards — and submits one job at a time as the bond has room.
"$RAIL_BIN" --watch "$WORK_DIR/outbox" --bond-key-seed "$WORK_DIR/keys/bond-0.seed" --rpc "127.0.0.1:$RPC_BASE" \
  --interval 10 >"$WORK_DIR/rail.log" 2>&1 &
pids+=($!)
log "    gateway: $(python3 -c 'import json,sys; c=json.load(open(sys.argv[1])).get("chain",{}); print({k: c.get(k) for k in ("registered","fp_certified","bond_active","exposure_room")})' "$WORK_DIR/gateway-health.json")"

# Studio chats with a model from its own model list, and the gateway engine reads PALW artifacts only
# (`engine_pairing_refusal`), so the class artifact is what Studio lists.
mkdir -p "$WORK_DIR/studio/models"
ln -sf "$MISAKA_PALW_ARTIFACT" "$WORK_DIR/studio/models/$(basename "$MISAKA_PALW_ARTIFACT")"
# The engine is chosen by the settings file — `misaka-studiod --backend` names only the local
# engines (auto, llamacpp, mlx, mock); the gateway is an HTTP endpoint the settings point at.
cat >"$WORK_DIR/studio/settings.json" <<JSON
{"backend": {"kind": "gateway"}, "node": {"palw_gateway_url": "http://127.0.0.1:$GATEWAY_PORT"}}
JSON
"$STUDIO_BIN" --host 127.0.0.1 --port "$STUDIO_PORT" --data-dir "$WORK_DIR/studio" --settings "$WORK_DIR/studio/settings.json" \
  --models-dir "$WORK_DIR/studio/models" >"$WORK_DIR/studio.log" 2>&1 &
pids+=($!)
wait_for "curl -fsS 'http://127.0.0.1:$STUDIO_PORT/api/v1/health' >/dev/null 2>&1" "MISAKA Studio's runtime to answer"
log "5/9 OK — gateway on :$GATEWAY_PORT (bond 0), Studio on :$STUDIO_PORT with the Gateway backend"

# ---------------------------------------------------------------------------------------------
# 6/9  One chat, sent to STUDIO.
# ---------------------------------------------------------------------------------------------
log "6/9 one chat through Studio: \"$PROMPT\""
STUDIO_MODEL="$(curl -fsS "http://127.0.0.1:$STUDIO_PORT/api/v1/models" 2>/dev/null | python3 -c '
import json, sys
d = json.load(sys.stdin)
rows = d if isinstance(d, list) else d.get("models", d.get("data", []))
ids = [r.get("id") for r in rows if isinstance(r, dict) and str(r.get("id", "")).lower().find("palwart") >= 0 or "candidate" in str(r.get("id", ""))]
ids = ids or [r.get("id") for r in rows if isinstance(r, dict)]
print(ids[0] if ids else "")' 2>/dev/null)"
[ -n "$STUDIO_MODEL" ] || die "Studio lists no model for the artifact (studio.log) — /api/v1/models: $(curl -fsS "http://127.0.0.1:$STUDIO_PORT/api/v1/models" 2>/dev/null | head -c 300)"
log "    Studio lists the artifact as model \"$STUDIO_MODEL\""
python3 - "$STUDIO_PORT" "$PROMPT" "$MAX_TOKENS" "$WORK_DIR/chat.json" "$STUDIO_MODEL" <<'PY' || die "the chat through Studio failed (studio.log, gateway.log)"
import json, sys, urllib.request
port, prompt, max_tokens, out, model = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4], sys.argv[5]
body = json.dumps({"model": model, "messages": [{"role": "user", "content": prompt}], "max_tokens": max_tokens, "stream": False}).encode()
req = urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions", data=body, headers={"content-type": "application/json"})
payload = json.loads(urllib.request.urlopen(req, timeout=3600).read())
json.dump(payload, open(out, "w"), indent=2)
print("  answer: %r" % payload["choices"][0]["message"]["content"], file=sys.stderr)
PY
JOB_STEM="$(ls -t "$WORK_DIR"/outbox/fp-job-*.commitment-unsigned.borsh 2>/dev/null | head -1)"
if [ -z "$JOB_STEM" ]; then
  why="$(grep -h "answered, not committed" "$WORK_DIR/gateway.log" | tail -1 | sed -E 's/.*answered, not committed/answered, not committed/' | cut -c1-300)"
  die "Studio answered but the gateway queued no commitment — the chat was not mined: ${why:-see gateway.log, studio.log}"
fi
# **Studio must say what the gateway said.** Before MISAKA-Studio d1e70ec it logged "free-prompt claim
# committed" off the claim id alone, for an answer the gateway kept off the chain (run4).
if grep -q "answered, not committed" "$WORK_DIR/studio.log"; then
  die "the gateway committed a job but Studio logged this chat as not committed (studio.log)"
fi
studio_line="$(grep -h "free-prompt claim" "$WORK_DIR/studio.log" | tail -1 | sed -E $'s/\x1b\\[[0-9;]*m//g' | sed -E 's/.*(free-prompt claim)/\1/' | cut -c1-200)"
[ -n "$studio_line" ] && log "    studio: $studio_line"
JOB_STEM="${JOB_STEM%.commitment-unsigned.borsh}"
JOB_ID="$(basename "$JOB_STEM")"; JOB_ID="${JOB_ID#fp-job-}"
log "6/9 OK — Studio's chat produced a commitment: ${JOB_STEM##*/}"
# ADR-0148 §6: the gateway priced it with the chain's own expression (op 187) — past the bundle the
# quanta are the compute era's, not the leaves era's sixty-four at most.
committed_line="$(grep -h "committed claim" "$WORK_DIR/gateway.log" | tail -1 | sed -E 's/.*committed claim/committed claim/' | cut -c1-200)"
[ -n "$committed_line" ] && log "    gateway: $committed_line"

# ---------------------------------------------------------------------------------------------
# 7/9  The rail signs and submits; the claim walks to Final on every node.
# ---------------------------------------------------------------------------------------------
CLAIM_ID="$("$RAIL_BIN" --artifact "$JOB_STEM" --print-claim 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["fp_claim_id"])' 2>/dev/null || true)"
[ -n "$CLAIM_ID" ] || die "the rail cannot name the claim for ${JOB_STEM##*/}"
python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print("  commitment: prompt_tokens=%s decode=%s work_leaves=%s (the leaves are a comparand, not the price)" % (d.get("prompt_tokens"), d.get("decode_tokens_executed"), d.get("work_leaves")))' <("$RAIL_BIN" --artifact "$JOB_STEM" --print-claim 2>/dev/null) >&2 || true
wait_for "python3 -c 'import json,sys; sys.exit(0 if (json.load(open(sys.argv[1])).get(\"submitted\") or \"\") else 1)' '$JOB_STEM.rail.json' 2>/dev/null" "the rail watcher to submit the chat's commitment"
log "7/9 submitted; following claim ${CLAIM_ID:0:16}…"
phase_on() {
  cli "$1" palw derived --json "$CLAIM_ID" 2>/dev/null | python3 -c '
import json,sys
try: d = json.load(sys.stdin)
except Exception: print("absent"); raise SystemExit
print(d.get("claim_phase") or "absent" if d.get("found", False) else "absent")' 2>/dev/null || echo absent
}
all_at() {  # every node at or past $1
  local want="$1" i have
  for ((i=0; i<NODES; i++)); do
    have="$(phase_on "$i")"
    case "$have" in
      voided|default_disputed) die "node-$i holds the claim as $have — see node-$i.log";;
    esac
    case "$want:$have" in
      provisional:provisional|provisional:panel_bound|provisional:receipt_licensed|provisional:final) ;;
      panel_bound:panel_bound|panel_bound:receipt_licensed|panel_bound:final) ;;
      receipt_licensed:receipt_licensed|receipt_licensed:final) ;;
      final:final) ;;
      *) return 1;;
    esac
  done
  return 0
}
for phase in provisional panel_bound receipt_licensed final; do
  wait_for "all_at $phase" "the claim to reach $phase on every node"
  log "    $phase on every node (DAA $(daa_of 1))"
done
cli 1 palw derived --json "$CLAIM_ID" > "$WORK_DIR/out/claim-final.json" 2>/dev/null || true
# The claim's economics — quanta, the spend, the work — are `palw claim`'s (GetPalwFreePromptClaim);
# `palw derived` reads the derived artifacts and carries the phase, not the quanta.
claim_row() {  # $1 = node, $2 = python expression over the claim row `c`
  cli "$1" palw claim --json "$CLAIM_ID" 2>/dev/null | python3 -c "
import json,sys
rows = [c for c in json.load(sys.stdin).get('claims', []) if c.get('found')]
c = rows[0] if rows else None
print($2 if c else '')" 2>/dev/null || true
}
cli 1 palw claim --json "$CLAIM_ID" > "$WORK_DIR/out/claim-final-economics.json" 2>/dev/null || true
log "7/9 OK — the chat is a Final claim on every node: $(claim_row 1 "{k: c.get(k) for k in ('phase', 'work_leaves', 'quanta', 'quanta_spent', 'accepted_daa', 'class_id')}")"

# ---------------------------------------------------------------------------------------------
# 8/9  A receipt block spends one of its quanta, accepted by every node.
# ---------------------------------------------------------------------------------------------
spent_on() { local n; n="$(claim_row "$1" "c.get('quanta_spent') or 0")"; echo "${n:-0}"; }
wait_for "[ \"\$(spent_on 1)\" -ge 1 ] 2>/dev/null" "a receipt block spending one of the claim's quanta"
for ((i=0; i<NODES; i++)); do [ "$(spent_on "$i")" -ge 1 ] 2>/dev/null || wait_for "[ \"\$(spent_on $i)\" -ge 1 ] 2>/dev/null" "node-$i to hold the spend"; done
log "8/9 OK — $(spent_on 1) of the claim's quanta spent into receipt blocks, on every node"

# ---------------------------------------------------------------------------------------------
# 9/9  A late node syncs everything from genesis and holds the same rows and the same claim.
# ---------------------------------------------------------------------------------------------
log "9/9 a node that was never up syncs the chain from genesis"
pids+=("$(start_node "$IBD_NODE" 0)")
target_daa="$(daa_of 1)"
wait_for "[ -n \"\$(daa_of $IBD_NODE)\" ] && [ \"\$(daa_of $IBD_NODE)\" -ge $target_daa ]" "the late node to reach DAA $target_daa"
rows() { reg "$1" "sorted((c['classId'], c['state'] if c['hasRow'] else 'legacy', c['sinceSpan'], c['economicCcuPerClaim']) for c in v.get('classes', []))"; }
wait_for "[ -n \"\$(rows 1)\" ] && [ \"\$(rows 1)\" = \"\$(rows $IBD_NODE)\" ]" "the late node's registry rows to equal node-1's"
[ "$(phase_on "$IBD_NODE")" = final ] || die "the late node does not hold the chat's claim as final"
log "9/9 OK — the late node synced to DAA $(daa_of "$IBD_NODE"), holds the same registry rows, and the chat's claim as final"

for i in 1 "$IBD_NODE"; do cli "$i" palw registry --output json > "$WORK_DIR/out/registry-node-$i.json" 2>/dev/null || true; done
log "PASS — a chat typed into MISAKA Studio became a Final free-prompt claim and a receipt block, on a chain with the"
log "       ADR-0145 bundle, the registry and the work target armed at $ECONOMY_AT; the class was admitted by a network-"
log "       drawn jury; and a node syncing from genesis agrees. Evidence: $WORK_DIR"
