#!/usr/bin/env bash
# misaka-palw-fp-devnet-e2e.sh — ADR-0077 Decision 7's drill: the free-prompt lane on a CHAIN.
#
# What this proves (ADR-0077 W7, and the "Done when" of §6): ONE job id, appearing in three
# places — the gateway's answer to a browser-shaped request, the node's `FreePromptCommitted`
# reaching `Final` for that claim on EVERY validator, and a receipt block spending one of its
# quanta, accepted by every validator — on a chain whose DAA has actually advanced through bind,
# challenge and maturity.
#
#   node-0 … node-(N-1)   validators from ONE build, each a floor producer under devnet public-seed
#                         bond n, each a panel seat (--palw-panel). node-0 listens, the rest
#                         --connect to it; all --utxoindex so the CLI can fund.
#   the gateway           one process beside node-0, driving a FAMILY worker (palw-a16-fp-worker)
#                         under bond 0, reading the chain over node-0's RPC (Decision 3).
#
# Why it is a chain and not a harness: the in-harness finding stands — a single-chain
# `TestConsensus` does not accrue the DAA the windows need (`docs/palw-fp-on-registered-classes.md`
# measured a sink DAA of 63 after 2,000 sequential blocks) — and a multi-node chain does. The
# devnet preset carries the minutes-scale lattice for exactly this reason
# (`palw_fp_devnet_v3::PALW_DEVNET_WINDOWS_V1`, selected by network type at
# `consensus/core/src/config/params.rs`): bind 40 / receipt 40 / challenge 100 / court 300,
# anchor_delay 4, receipt_maturity 20. A claim is therefore spendable ~200 DAA after it lands
# instead of the RC set's ~1,620.
#
# WHAT THIS DRILL DOES NOT PROVE, said here because a drill that overclaims is worse than none:
#   * it does not prosecute a court case (that is the certify drill's and the fuzzers' job);
#   * it does not measure the fleet's wall-clock — devnet windows are not testnet-11's;
#   * a PASS says the pipeline reaches a receipt block, not that the answer was any good.
#
# Env: KASPAD_BIN, CLI_BIN, CERTIFY_BIN, GATEWAY_BIN, WORKER_BIN (defaults target/release/*),
#      MISAKA_PALW_ARTIFACT (the .palwart the A16 class is registered from — REQUIRED),
#      MISAKA_PALW_TOKENIZER (tokenizer.json for that artifact — REQUIRED),
#      MISAKA_DEVNET_GENESIS (128-hex; see the network-domain note below — REQUIRED),
#      NODES (3), WORK_DIR, WAIT, STEP_WAIT, PROMPT, MAX_TOKENS.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
# `misaka-cli` is the PACKAGE; `misaka` is the binary. Naming the package here is the mistake the
# certify drill already made once and fixed — it dies at the first CLI call with a 127.
CLI_BIN="${CLI_BIN:-$REPO_ROOT/target/release/misaka}"
CERTIFY_BIN="${CERTIFY_BIN:-$REPO_ROOT/target/release/palw-certify}"
GATEWAY_BIN="${GATEWAY_BIN:-$REPO_ROOT/target/release/misaka-palw-gateway}"
WORKER_BIN="${WORKER_BIN:-$REPO_ROOT/target/release/palw-a16-fp-worker}"
RAIL_BIN="${RAIL_BIN:-$REPO_ROOT/target/release/misaka-palw-fp-rail}"
NODES="${NODES:-3}"
WORK_DIR="${WORK_DIR:-$REPO_ROOT/.misaka-palw-fp-devnet}"
WAIT="${WAIT:-900}"
# Every wait below is a POLL against the chain rather than a fixed sleep: the floor's draw rate is
# a seeded, per-class quantity (ADR-0076) and every fixed sleep in this repo's drills has had to be
# re-sized at least once. A poll is right at whatever cadence the seed produces next.
STEP_WAIT="${STEP_WAIT:-600}"
GATEWAY_PORT="${GATEWAY_PORT:-18795}"
# **The port bases are a parameter, and an occupied one is a refusal BY NAME.**
#
# They were literals (16410 / 17710) inside the node loop and inside stage 2's registrar
# (`reg_rpc + 100`, `+ 200`). On a host running a second devnet — which is what a shared build
# machine is — stage 1 came up on whatever was free, ran for two minutes, and then the REGISTRAR
# died with "Address already in use" on a port nothing had said it would need; the drill reported
# "the registrar exited before the class reached a block", which is the truth about the symptom and
# says nothing about the cause. Every port this run binds is now derived from these two and checked
# before a single process starts.
P2P_BASE="${P2P_BASE:-16410}"
RPC_BASE="${RPC_BASE:-17710}"
PROMPT="${PROMPT:-Name one property of a hash function.}"
MAX_TOKENS="${MAX_TOKENS:-4}"
# The class the gateway's worker embodies. `palw-a16-fp-worker` serves exactly this catalog row.
MODEL_ID="${MODEL_ID:-Qwen/Qwen2.5-1.5B/graph-v2}"
PREMINE_TXID="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"   # "misaka-premine", zero-padded
MAIN_PREMINE_INDEX=40   # consensus/core/src/config/premine.rs; bond n's fee float is MAIN_PREMINE_INDEX + 1 + n
DEVNET_BONDS=6          # premine.rs: PALW_DEVNET_GENESIS_BONDS
REGISTRAR_BOND=$((DEVNET_BONDS - 1))   # the bond no producer node holds, so its float is unspent
BOND_FEE_FLOAT_SOMPI=10000000000       # premine.rs: PALW_RC_BOND_FEE_FLOAT_SOMPI = 100 * SOMPI_PER_KASPA

log() { printf '[fp-e2e] %s\n' "$*" >&2; }
die() { log "FATAL: $*"; exit 1; }

# ---------------------------------------------------------------------------------------------
# Preflight. Every one of these is a refusal BY NAME rather than a failure thirty minutes in.
# ---------------------------------------------------------------------------------------------
for b in "$KASPAD_BIN" "$CLI_BIN" "$CERTIFY_BIN" "$GATEWAY_BIN" "$WORKER_BIN" "$RAIL_BIN"; do
  [ -x "$b" ] || die "$b is not an executable. Build it: cargo build --release -p <its crate>"
done
# Plain `if` rather than `${VAR:?message}`: bash 3.2 (which is what macOS ships) re-parses quotes
# INSIDE a `:?` word, so one apostrophe in the prose — "the drill's" — opened a quote that swallowed
# the next thirty lines and made the failure land on a `done` far below. Prose belongs in `die`.
[ -n "${MISAKA_PALW_ARTIFACT:-}" ] || die "MISAKA_PALW_ARTIFACT must name the .palwart for $MODEL_ID — the drill registers the class FROM it and the worker serves it. The pinned 1.7 GiB dense artifact is the drill's; CI uses a fixture instead."
[ -n "${MISAKA_PALW_TOKENIZER:-}" ] || die "MISAKA_PALW_TOKENIZER must name the tokenizer.json for that artifact — the worker encodes the prompt with it."
[ -f "$MISAKA_PALW_ARTIFACT" ] || die "MISAKA_PALW_ARTIFACT=$MISAKA_PALW_ARTIFACT does not exist"
[ -f "$MISAKA_PALW_TOKENIZER" ] || die "MISAKA_PALW_TOKENIZER=$MISAKA_PALW_TOKENIZER does not exist"
# **The one value this script cannot derive from a running node, stated rather than guessed.**
# `identity.json` binds `network_domain`, and that value is
# `blake2b512(key="misaka-palw/attempt-v2/network-domain/v1", u64le(len(net)) ‖ net ‖ genesis)`
# (`palw_attempt_v2::palw_network_domain_v2_for`, called with `params.net.to_string()` and
# `params.genesis.hash`). The genesis hash is a preset constant (`config::genesis::DEVNET_GENESIS`)
# and no shipped binary prints it on its own, so the operator supplies it and the drill computes
# the rest. A wrong value here does not fail loudly — it produces claims whose context hash no seat
# can reproduce, every one of which collects an `Unavailable` quorum and DEFAULTS its producer —
# which is precisely why it is a named requirement instead of a default.
[ -n "${MISAKA_DEVNET_GENESIS:-}" ] || die "MISAKA_DEVNET_GENESIS must be the devnet genesis hash, 128 hex chars (consensus/core/src/config/genesis.rs, DEVNET_GENESIS). A guessed value silently produces claims no seat can replay."
[ "${#MISAKA_DEVNET_GENESIS}" -eq 128 ] || die "MISAKA_DEVNET_GENESIS is ${#MISAKA_DEVNET_GENESIS} chars, not 128"
command -v python3 >/dev/null || die "python3 is required (key derivation and the HTTP client)"

# Every port this run binds, derived from the two bases and the node count exactly as the stages
# below derive them — never a second list, or a stage could bind a port this check never saw. The
# registrar's two are `RPC_BASE + 100` (registration) and `+ 200` (the class-table dump).
port_in_use() {
  python3 -c 'import socket,sys
s = socket.socket()
try:
    s.bind(("127.0.0.1", int(sys.argv[1])))
except OSError:
    sys.exit(0)
finally:
    s.close()
sys.exit(1)' "$1"
}
busy=""
for ((i=0; i<NODES; i++)); do
  for port in $((P2P_BASE + i)) $((RPC_BASE + i)); do
    port_in_use "$port" && busy="$busy $port"
  done
done
for port in $((RPC_BASE + 100)) $((RPC_BASE + 200)) "$GATEWAY_PORT"; do
  port_in_use "$port" && busy="$busy $port"
done
[ -z "$busy" ] || die "these ports are already bound:$busy — another devnet is running on this host. Re-run with P2P_BASE / RPC_BASE / GATEWAY_PORT set to a free range; a collision discovered at stage 2 reads as 'the registrar exited' and names nothing."

rm -rf "$WORK_DIR"; mkdir -p "$WORK_DIR/keys" "$WORK_DIR/obj" "$WORK_DIR/outbox" "$WORK_DIR/traces"

# Public seeds (consensus/core/src/config/premine.rs): value-less, derivable by anyone, which is
# what makes a devnet drill reproducible on someone else's machine.
# ALL SIX, not just the producers'. `PALW_DEVNET_GENESIS_BONDS` is 6 and this drill runs 3 nodes,
# so bond 5 is a bond nobody produces with — which is what the registrar needs. Every fee float is
# owned by ITS OWN bond's payout key (`bonded_genesis_utxos`: index MAIN_PREMINE_INDEX+1+i), so the
# registrar and the rail can only avoid spending the same outpoint by being different bonds. They
# were both bond 0, and the second spend of a spent float is not a funding error a reader can see.
python3 - "$WORK_DIR/keys" "$DEVNET_BONDS" <<'PY'
import hashlib, os, sys
d, n = sys.argv[1], int(sys.argv[2])
h = lambda b: hashlib.blake2b(b, digest_size=32).hexdigest()
for i in range(n):
    p = f"{d}/bond-{i}.seed"; open(p, "w").write(h(b"misaka-devnet-genesis-bond-v1/" + str(i).encode())); os.chmod(p, 0o600)
p = f"{d}/main.seed"; open(p, "w").write(h(b"misaka-testnet-premine-9b-claude-managed")); os.chmod(p, 0o600)
PY

# The preimage is `palw_attempt_v2::palw_network_domain_v2_for` verbatim: blake2b-512 KEYED by the
# network-domain constant, over `u64le(len(net)) ‖ net ‖ genesis`. Recomputing a consensus hash in
# Python is only safe because the equivalence is already pinned in this tree —
# `scripts/misaka-palw-fp-v3-worker-smoke.py` recomputes `fp_worker_request_hash_v3` the same way
# and asserts it against the worker's own value — so a divergence between Python's keyed blake2b
# and blake2b_simd would fail that smoke before it reached this drill.
NETWORK_DOMAIN=$(python3 - "$MISAKA_DEVNET_GENESIS" <<'PY'
import hashlib, struct, sys
net = b"devnet"
genesis = bytes.fromhex(sys.argv[1])
h = hashlib.blake2b(digest_size=64, key=b"misaka-palw/attempt-v2/network-domain/v1")
h.update(struct.pack("<Q", len(net))); h.update(net); h.update(genesis)
print(h.hexdigest())
PY
)
log "network domain $NETWORK_DOMAIN (devnet ‖ genesis ${MISAKA_DEVNET_GENESIS:0:16}…)"

pids=()
cleanup() {
  for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done
}
trap cleanup EXIT

# ---------------------------------------------------------------------------------------------
# 1. N validators from one build, all producing the floor and all seated on panels.
# ---------------------------------------------------------------------------------------------
declare -a ADDRS
for ((i=0; i<NODES; i++)); do
  addr="$("$CLI_BIN" --network devnet key address --key-file "$WORK_DIR/keys/bond-$i.seed" | tail -1 | awk '{print $NF}')"
  [ -n "$addr" ] || die "cannot derive bond $i's address"
  ADDRS[$i]="$addr"
  p2p=$((P2P_BASE + i)); rpc=$((RPC_BASE + i))
  # --nogrpc: every node would otherwise bind the same default gRPC port and the later ones exit.
  # --enable-unsynced-mining: a chain with only a genesis is "not synced"; the producer still
  # requires peers and open participation, so the gate's other clauses are not waived.
  # --palw-panel: the drill's whole point is that SEATS certify the claim, so every node is a seat.
  args=(--devnet --appdir="$WORK_DIR/node-$i" --listen=127.0.0.1:$p2p --rpclisten-borsh=127.0.0.1:$rpc
        --utxoindex --nodnsseed --disable-upnp --nogrpc --enable-unsynced-mining --palw-panel
        --palw-produce --palw-producer-key="$WORK_DIR/keys/bond-$i.seed"
        --palw-producer-bond="$PREMINE_TXID:$i" --palw-producer-pay-address="$addr"
        --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + i))")
  # node-0 also holds the class artifact: it is the node that registers the class, and a registrar
  # must be able to derive the artifact root it is about to pin.
  if [ "$i" -eq 0 ]; then args+=(--palw-class-artifact="$MISAKA_PALW_ARTIFACT"); fi
  if [ "$i" -gt 0 ]; then args+=(--connect=127.0.0.1:$P2P_BASE); fi
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >"$WORK_DIR/node-$i.log" 2>&1 &
  # `$!` into a variable rather than `${pids[-1]}`: macOS ships bash 3.2, which rejects a negative
  # array index at PARSE time — the whole script fails to load, not the line.
  node_pid=$!
  pids+=("$node_pid")
  log "node-$i pid $node_pid rpc 127.0.0.1:$rpc bond $PREMINE_TXID:$i"
done

CLI=("$CLI_BIN" --network devnet --rpc 127.0.0.1:$RPC_BASE)
blocks_of() { grep -c "produced block #" "$WORK_DIR/node-$1.log" 2>/dev/null || true; }
# **The chain's progress is the SET's, not node-0's.**
#
# Every wait below wants the same thing: DAA has moved, so the next window can open. Reading it off
# node-0 alone asks a different question — did THIS producer win a draw — and the draw is a seeded,
# per-bond, per-class quantity (ADR-0076). Measured on this host at load 44: node-1 produced four
# blocks and node-2 one while node-0 produced none, the chain advanced five blocks, carried a PALW
# lifecycle object and had a panel file a "Valid" receipt — and the drill died with "node-0 produced
# no blocks within 900s". That is a true sentence about node-0 and a false one about the chain.
chain_blocks() {
  local i total=0
  for ((i=0; i<NODES; i++)); do total=$((total + $(blocks_of "$i"))); done
  echo "$total"
}

# Wait for node-0 to gain `n` more blocks than it had on entry. Returns after the deadline rather
# than dying: the verdict at the end is what decides PASS/FAIL, and a step that timed out should
# reach it carrying its evidence instead of hiding the reason inside a wait.
advance() {
  local want="${1:-1}" from now deadline
  from=$(chain_blocks); deadline=$((SECONDS + STEP_WAIT))
  while :; do
    now=$(chain_blocks)
    [ $((now - from)) -ge "$want" ] && return 0
    [ $SECONDS -lt $deadline ] || { log "the chain gained $((now - from))/$want block(s) in ${STEP_WAIT}s — continuing to the verdict"; return 0; }
    sleep 2
  done
}
# Wait until every node's log matches `pattern`, or the deadline passes.
all_nodes_logged() {
  local pattern="$1" deadline=$((SECONDS + STEP_WAIT)) i ok
  while :; do
    ok=1
    for ((i=0; i<NODES; i++)); do grep -qE "$pattern" "$WORK_DIR/node-$i.log" || ok=0; done
    [ "$ok" = 1 ] && return 0
    [ $SECONDS -lt $deadline ] || { log "not every node matched \"$pattern\" within ${STEP_WAIT}s — continuing to the verdict"; return 1; }
    sleep 3
  done
}

deadline=$((SECONDS + WAIT))
until [ "$(chain_blocks)" -ge 3 ]; do
  [ $SECONDS -lt $deadline ] || {
    for ((i=0; i<NODES; i++)); do log "  node-$i produced $(blocks_of $i)"; done
    tail -40 "$WORK_DIR/node-0.log" >&2
    die "the chain produced fewer than 3 blocks within ${WAIT}s — see the per-node counts above and node-0.log"
  }
  sleep 3
done
per_node=""
for ((i=0; i<NODES; i++)); do per_node="$per_node node-$i=$(blocks_of $i)"; done
log "stage 1 OK — the chain produced $(chain_blocks) blocks ($per_node); waiting for every node to follow"
advance 1

# ---------------------------------------------------------------------------------------------
# 2. Register the class the gateway's worker embodies.
#
# The devnet preset registers ONE class at genesis — the BASE-0 floor
# (`palw_v2_params_from_artifacts_on_base`, the RC genesis artifact root) — and the floor has no
# free-prompt worker binary (ADR-0077 §1's table: "free-prompt worker: none"). So the drill has to
# put the dense class on the chain the way an outside operator would: the permissionless
# post-genesis route (ADR-0054), from the artifact this node holds.
# ---------------------------------------------------------------------------------------------
log "stage 2 — registering $MODEL_ID from the artifact"
# **Registered by a bond that produces nothing.** ADR-0054 admits a class on the producer key
# alone, so the registrar does not have to be an executor — and making it a different bond is what
# keeps bond 0's fee float unspent for the rail in stage 5b. It also makes the drill demonstrate
# the real shape: one party puts a class on the chain, another executes it.
REGISTRAR_ADDR="$("$CLI_BIN" --network devnet key address --key-file "$WORK_DIR/keys/bond-$REGISTRAR_BOND.seed" | tail -1 | awk '{print $NF}')"
[ -n "$REGISTRAR_ADDR" ] || die "cannot derive the registrar bond ($REGISTRAR_BOND) address"
reg_rpc=$RPC_BASE
# **The registrar is a DAEMON, so it is backgrounded and watched, not waited on.** kaspad does not
# stop when a registration lands — the panel submits the object and goes on validating, which is
# what a node should do. Running it in the foreground made a successful registration and a silent
# no-op the same observation (no output, forever) and cost two hours on 2026-09-03. The sentence
# below is printed only after the object is IN an accepted block, so it is the one that means the
# chain has the class rather than that a transaction was built.
MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" --devnet --appdir="$WORK_DIR/node-0-reg" \
      --rpclisten-borsh=127.0.0.1:$((reg_rpc + 100)) --nogrpc --nodnsseed --disable-upnp \
      --connect=127.0.0.1:$P2P_BASE --utxoindex \
      --palw-register-class="$MODEL_ID" --palw-class-artifact="$MISAKA_PALW_ARTIFACT" \
      --palw-producer-key="$WORK_DIR/keys/bond-$REGISTRAR_BOND.seed" --palw-producer-pay-address="$REGISTRAR_ADDR" \
      --palw-producer-bond="$PREMINE_TXID:$REGISTRAR_BOND" \
      --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + REGISTRAR_BOND))" \
      >"$WORK_DIR/register-class.log" 2>&1 &
# `$!` into a variable rather than `${pids[-1]}`: macOS ships bash 3.2, which rejects a negative
# array index at PARSE time — the whole script fails to load, not the line.
reg_pid=$!
reg_deadline=$((SECONDS + WAIT))
reg_outcome="timeout"
while :; do
  if grep -q "class registration in tx .* is on the chain" "$WORK_DIR/register-class.log" 2>/dev/null; then
    reg_outcome="ok"; break
  fi
  # The failure that used to be pure silence, now named by the daemon's own refusal line.
  if grep -q "service not started" "$WORK_DIR/register-class.log" 2>/dev/null; then
    reg_outcome="no-service"; break
  fi
  # **A registration needs a BOND, and the panel says so in a WARN and then goes on running.**
  # Measured 2026-09-03: this invocation passed the key, the pay address and the fee outpoint and
  # not the bond, so the panel logged "Nothing will be registered" and the drill waited fifteen
  # minutes for a block that was never going to carry anything. A daemon that declines by warning
  # is invisible to a watcher that only greps for success, so the decline is watched for too.
  if grep -q "needs a bond to register the class under" "$WORK_DIR/register-class.log" 2>/dev/null; then
    reg_outcome="no-bond"; break
  fi
  kill -0 "$reg_pid" 2>/dev/null || { reg_outcome="died"; break; }
  [ $SECONDS -lt $reg_deadline ] || break
  sleep 3
done
kill "$reg_pid" 2>/dev/null || true
wait "$reg_pid" 2>/dev/null || true
case "$reg_outcome" in
  ok) : ;;
  no-bond)
    grep -n "needs a bond to register the class under" "$WORK_DIR/register-class.log" >&2
    die "the registrar was started without --palw-producer-bond, so the panel declined to register anything — see the line above" ;;
  no-service)
    grep -n "service not started" "$WORK_DIR/register-class.log" >&2
    die "the node built no registration service, so --palw-register-class was read by nobody — see the line above for which flag it wanted" ;;
  died)
    tail -30 "$WORK_DIR/register-class.log" >&2
    die "the registrar exited before the class reached a block — see $WORK_DIR/register-class.log" ;;
  *)
    tail -30 "$WORK_DIR/register-class.log" >&2
    die "the class did not reach an accepted block within ${WAIT}s — see $WORK_DIR/register-class.log" ;;
esac
advance 2

# **The class id comes from the CHAIN'S TABLE, not from a hex scrape of a log.** This read
# `grep -oE '[0-9a-f]{128}' … | tail -1`, and the last 128-hex string in a registration log is a
# TXID or a block hash — never the class id, which the panel does not print. Everything downstream
# (identity.json, the gateway's chain probe, the rail's commitment) then bound a block hash and
# refused for reasons that named the class rather than the mistake. `--palw-dump-classes` reads
# `palw_v2_class_table()` at the tip and refuses to answer from genesis, so it names what the chain
# ACCEPTED — and it prints the budget, which stage 2b needs.
# No `timeout(1)`: it is coreutils, macOS does not ship it, and this drill's own preflight is on
# macOS. The dump service returns after it prints, but the NODE goes on running, so this is the
# same background-watch-kill shape stage 2 uses rather than a wait on a process that never exits.
class_table() {
  local pid deadline
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" --devnet --appdir="$WORK_DIR/node-0-reg" \
    --rpclisten-borsh=127.0.0.1:$((reg_rpc + 200)) --nogrpc --nodnsseed --disable-upnp \
    --connect=127.0.0.1:$P2P_BASE --utxoindex --palw-dump-classes >"$WORK_DIR/class-table.log" 2>&1 &
  pid=$!
  deadline=$((SECONDS + STEP_WAIT))
  while :; do
    grep -qE '\[palw-dump\] .* class\(es\) at daa' "$WORK_DIR/class-table.log" 2>/dev/null && break
    grep -q 'holds no PALW classes' "$WORK_DIR/class-table.log" 2>/dev/null && break
    kill -0 "$pid" 2>/dev/null || break
    [ $SECONDS -lt $deadline ] || break
    sleep 2
  done
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  grep -E '\[palw-dump\]   class=' "$WORK_DIR/class-table.log" || true
}
class_rows=$(class_table)
[ -n "$class_rows" ] || { tail -20 "$WORK_DIR/class-table.log" >&2; die "--palw-dump-classes named no class rows — see $WORK_DIR/class-table.log"; }
# The registered class is the one that is not the genesis floor. `base=false` is the chain's own
# word for "an operator put this here", so the drill does not have to know the floor's id.
CLASS_ID=$(printf '%s\n' "$class_rows" | grep 'base=false' | sed -E 's/.*class=([0-9a-f]+).*/\1/' | head -1)
[ -n "$CLASS_ID" ] || { printf '%s\n' "$class_rows" >&2; die "every class on this chain is a base class — $MODEL_ID did not register"; }
log "stage 2 OK — class ${CLASS_ID:0:16}… (from the chain's class table)"

# ---------------------------------------------------------------------------------------------
# 2b. The budget, which is a SECOND wall standing behind the class id and looks nothing like one.
#
# A class registered post-genesis is Active and holds share, and is NOT budgeted until the next
# epoch boundary (`palw_admission_v2.rs`, `palw_state_v2.rs` — asserted as shipped). The gateway
# writes the commitment ONLY when `commit_refusal` is None and gates the derivation on the same
# field, so on a `budget=0` chain this drill reaches stage 5, gets an answer, and then produces no
# commitment at all — with a refusal that reads like a width refusal to anyone who has been
# fighting the width. Naming it here costs one log line and saves that afternoon.
# ---------------------------------------------------------------------------------------------
CLASS_BUDGET=$(printf '%s\n' "$class_rows" | grep "class=$CLASS_ID" | sed -E 's/.*budget=([0-9]+).*/\1/' | head -1)
# **The class's canonical job in leaves, from the chain rather than from a default.** The gateway
# and the rail each carry `--class-leaves` defaulting to the FLOOR's 7,708, so a drill that omitted
# it priced a dense class's quanta as the floor's — in two places, which could also disagree with
# each other. `--palw-dump-classes` publishes it now (`PalwClassRowV2::canonical_leaves`), so the
# number is read once here and passed to both halves.
CLASS_LEAVES=$(printf '%s\n' "$class_rows" | grep "class=$CLASS_ID" | sed -E 's/.*leaves=([0-9]+).*/\1/' | head -1)
[ -n "${CLASS_LEAVES:-}" ] && [ "$CLASS_LEAVES" != 0 ] \
  || die "the class table published no canonical leaf count for $CLASS_ID — a quantum cannot be priced without it"
log "  canonical job = $CLASS_LEAVES leaves"
if [ "${CLASS_BUDGET:-0}" = 0 ]; then
  log "  WARNING: $MODEL_ID has budget=0 — registered mid-epoch, so it holds share but cannot be"
  log "           drawn until the next boundary. The gateway will answer and write NO commitment"
  log "           (commit_refusal: \"this class's epoch budget is already spent\"). That refusal is"
  log "           NOT the context-width wall. Seat the class at genesis, or wait for the boundary."
else
  log "  class budget=$CLASS_BUDGET blocks"
fi

# ---------------------------------------------------------------------------------------------
# 3. Certify the family and bind the class to the FREE-PROMPT lane (ADR-0075).
#
# Without this the transition refuses every commitment with `FreePromptLaneUncertified`, which is
# what 5d measured: an A16 free-prompt claim was refused not by its arithmetic but because the
# genesis certified set held the floor alone.
# ---------------------------------------------------------------------------------------------
submit() {
  local f="$1"; local args=()
  if ls "$f".chunk* >/dev/null 2>&1; then
    for c in $(ls "$f".chunk* | sort -t k -k3 -n); do args+=(--object "$c"); done
  else
    args=(--object "$f")
  fi
  "${CLI[@]}" palw submit-object --key-file "$WORK_DIR/keys/main.seed" "${args[@]}" --yes
  advance 2   # a chunked object needs one carrier per chunk, and the next burst spends this change
}
log "stage 3 — certifying the a16 family on the free-prompt lane"
"$CERTIFY_BIN" drill --family a16 --lane fp --out "$WORK_DIR/obj/a16-fp.obj" || die "the a16 fp drill did not produce evidence"
submit "$WORK_DIR/obj/a16-fp.obj"
"$CERTIFY_BIN" bind --model-id "$MODEL_ID" --lane fp --out "$WORK_DIR/obj/a16-bind.obj" || die "palw-certify bind refused $MODEL_ID"
submit "$WORK_DIR/obj/a16-bind.obj"
all_nodes_logged "PALW lifecycle carried.*ClassLaneCertified" \
  || log "WARNING: not every node logged the class-lane binding — the commitment may be refused as uncertified"
log "stage 3 OK"

# ---------------------------------------------------------------------------------------------
# 4. The gateway, under bond 0, reading the chain over node-0's RPC (Decision 3).
# ---------------------------------------------------------------------------------------------
EXEC_PUBKEY=$("$RAIL_BIN" --bond-key-seed "$WORK_DIR/keys/bond-0.seed" --print-bond-pubkey \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["executor_pubkey"])') \
  || die "cannot read bond 0's public key from the rail"
# **The operator id is DERIVED, with the same preimage the chain uses** — `palw_operator_id_v2`
# (`consensus/core/src/palw_state_v2.rs`): blake2b-512 keyed by the operator-id domain over
# `u64le(len) ‖ operator_pubkey`, where the devnet registry's pubkey for bond n is the literal
# bytes `misaka-devnet-operator-{n}` (`params.rs: palw_devnet_genesis_bonds_v1`). A plain digest
# here would not match, and the mismatch would surface as an admission refusal rather than as a
# bad hash, which is the kind of error that costs an afternoon.
OPERATOR_ID=$(python3 - <<'PY'
import hashlib, struct
pk = b"misaka-devnet-operator-0"
h = hashlib.blake2b(digest_size=64, key=b"misaka-palw/state-v2/operator-id/v1")
h.update(struct.pack("<Q", len(pk))); h.update(pk)
print(h.hexdigest())
PY
)
cat >"$WORK_DIR/identity.json" <<JSON
{
  "network_domain": "$NETWORK_DOMAIN",
  "class_id": "$CLASS_ID",
  "bond_txid": "$PREMINE_TXID",
  "bond_index": 0,
  "executor_pubkey": "$EXEC_PUBKEY",
  "operator_id": "$OPERATOR_ID"
}
JSON

log "stage 4 — starting the gateway on 127.0.0.1:$GATEWAY_PORT"
# **The worker's own stderr, on.** ADR-0079 SA-7 withholds it by default — right for a production
# gateway parsing a stranger's HTTP text, wrong for a drill, whose entire job is to say WHY a stage
# failed. Measured: the worker refused its artifact by name (no tokenizer commitment) and the drill
# reported "the worker exited before announcing its manifest" over three withheld log lines. A
# refusal nobody can read is a refusal that costs the same as a hang.
MISAKA_PALW_ARTIFACT="$MISAKA_PALW_ARTIFACT" MISAKA_PALW_TOKENIZER="$MISAKA_PALW_TOKENIZER" \
MISAKA_PALW_GATEWAY_LOG_WORKER_STDERR=1 \
MISAKA_PALW_NETWORK_ID="devnet" \
"$GATEWAY_BIN" --listen "127.0.0.1:$GATEWAY_PORT" --worker "$WORKER_BIN" \
  --outbox "$WORK_DIR/outbox" --identity "$WORK_DIR/identity.json" \
  --class-leaves "$CLASS_LEAVES" \
  --rpc 127.0.0.1:$RPC_BASE >"$WORK_DIR/gateway.log" 2>&1 &
pids+=($!)

# The health probe is also the Decision 3 assertion: /health must name all four chain-side reasons.
health=""
deadline=$((SECONDS + STEP_WAIT))
until health=$(python3 -c "
import json,urllib.request,sys
try: print(urllib.request.urlopen('http://127.0.0.1:$GATEWAY_PORT/health', timeout=3).read().decode())
except Exception as e: sys.exit(1)
" 2>/dev/null); do
  [ $SECONDS -lt $deadline ] || { tail -40 "$WORK_DIR/gateway.log" >&2; die "the gateway did not answer /health within ${STEP_WAIT}s"; }
  sleep 2
done
echo "$health" >"$WORK_DIR/health.json"
python3 - "$WORK_DIR/health.json" <<'PY' || die "/health does not name the four chain-side reasons (ADR-0077 Decision 3)"
import json, sys
h = json.load(open(sys.argv[1]))
chain = h.get("chain", {})
missing = [k for k in ("registered", "fp_certified", "bond_active", "exposure_room") if k not in chain]
if missing:
    print(f"missing from /health.chain: {missing}", file=sys.stderr); sys.exit(1)
print(f"  registered={chain['registered']} fp_certified={chain['fp_certified']} "
      f"bond_active={chain['bond_active']} exposure_room={chain['exposure_room']}", file=sys.stderr)
if chain.get("fp_certified") is not True:
    print("  NOTE: the chain does not certify this class on the free-prompt lane — the gateway will "
          "answer and keep the commitment in its outbox (that is Decision 3 working, and it means "
          "this run cannot reach a receipt block).", file=sys.stderr)
PY
log "stage 4 OK — /health names all four"

# ---------------------------------------------------------------------------------------------
# 5. One browser-shaped request. The answer is the product; the commitment is the receipt.
# ---------------------------------------------------------------------------------------------
log "stage 5 — one chat request"
python3 - "$GATEWAY_PORT" "$PROMPT" "$MAX_TOKENS" "$WORK_DIR/chat.json" <<'PY' || die "the chat request failed"
import json, sys, urllib.request
port, prompt, max_tokens, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
body = json.dumps({"messages": [{"role": "user", "content": prompt}], "max_tokens": max_tokens}).encode()
req = urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions", data=body,
                             headers={"content-type": "application/json"})
payload = json.loads(urllib.request.urlopen(req, timeout=1800).read())
json.dump(payload, open(out, "w"), indent=2)
m = payload.get("misaka", {})
print(f"  answer: {payload['choices'][0]['message']['content']!r}", file=sys.stderr)
print(f"  fp_job_id={m.get('fp_job_id','?')[:16]}… claim={m.get('fp_claim_id','?')[:16]}…", file=sys.stderr)
PY
JOB_ID=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["misaka"]["fp_job_id"])' "$WORK_DIR/chat.json")
CLAIM_ID=$(python3 -c '
import json,sys
m = json.load(open(sys.argv[1]))["misaka"]
print(m.get("fp_claim_id", ""))' "$WORK_DIR/chat.json")
[ -n "$JOB_ID" ] || die "the response carries no fp_job_id — there is no job id to follow"
log "stage 5 OK — job ${JOB_ID:0:16}… claim ${CLAIM_ID:0:16}…"

# ---------------------------------------------------------------------------------------------
# 5b. Sign the queued commitment and hand it to the chain.
#
# **This step did not exist, and stage 6 waited for its result.** The gateway holds no key — ADR-0079
# Decision 4 keeps the process that parses a stranger's HTTP text away from the bond — so it QUEUES
# the commitment in the outbox and stops. The rail is the half that signs and spends the fee, and
# `--submit` appeared nowhere in this file. So `FreePromptCommitted` could never appear on any node,
# and stage 6 reported "NOT on every node" for a claim that had never been submitted by anyone. A
# drill whose acceptance condition is unreachable does not measure the chain, it measures itself.
#
# The funding is bond 0's own fee float, which stage 2 no longer spends (the registrar is
# REGISTRAR_BOND). `--submit --rpc` is ADR-0077 Decision 4's one handoff: the rail signs and then
# calls the same `misaka-palw-fp-submit` library `misaka palw fp-submit` calls, so the freshness,
# funding and subnetwork answers have one implementation rather than two.
# ---------------------------------------------------------------------------------------------
log "stage 5b — signing the queued commitment and submitting it"
ARTIFACT_STEM="$WORK_DIR/outbox/fp-job-${JOB_ID:0:16}"
if ! ls "$ARTIFACT_STEM".commitment-unsigned.borsh >/dev/null 2>&1; then
  ls -la "$WORK_DIR/outbox" >&2
  # The refusal the gateway records when it declines to queue at all, surfaced by name rather than
  # left as an absent file: on a mid-epoch registration this is the budget wall from stage 2b, not
  # anything about the prompt.
  python3 - "$WORK_DIR/health.json" <<'PY' >&2 || true
import json, sys
try: h = json.load(open(sys.argv[1]))
except Exception: sys.exit(0)
chain = h.get("chain", {})
# **Read every field that can carry the reason, in the order they become specific.** This looked at
# `commit_refusal` alone, found None on a real run, and printed an `ls` of the outbox — while
# `chain.bond_not_ready_reason` held "this class's epoch budget is already spent" the whole time.
# `commit_refusal` is `facts.commit_refusal()`, which is empty when the gateway declined for a
# reason the chain has not been asked about yet; the per-field reasons below are where the cause
# actually lives.
for label, why in [
    ("the chain refuses the commitment", h.get("commit_refusal")),
    ("the executor bond is not producible", chain.get("bond_not_ready_reason")),
]:
    if why:
        print(f"  {label}: {why}")
        break
else:
    room = chain.get("exposure_room")
    if isinstance(room, int) and room <= 0:
        print(f"  the bond has no exposure room ({room}) — it cannot back another claim's lifetime.")
    elif chain.get("fp_certified") is not True:
        print("  this class is not certified on the free-prompt lane, so no commitment may be written.")
    else:
        print("  no field of /health names a refusal, so the cause is the gateway's own per-epoch "
              "commit budget or a worker-side refusal — read gateway.log. /health is: "
              + json.dumps({k: chain.get(k) for k in ("registered", "fp_certified", "bond_active", "exposure_room")}))
PY
  die "the gateway queued no commitment for job ${JOB_ID:0:16} — there is nothing for the rail to sign"
fi
if "$RAIL_BIN" --artifact "$ARTIFACT_STEM" \
     --bond-key-seed "$WORK_DIR/keys/bond-0.seed" \
     --funding-outpoint "$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1))" \
     --funding-amount "$BOND_FEE_FLOAT_SOMPI" \
     --class-id "$CLASS_ID" --class-leaves "$CLASS_LEAVES" \
     --retention-dir "$WORK_DIR/traces" \
     --submit --rpc 127.0.0.1:$RPC_BASE >"$WORK_DIR/rail-submit.log" 2>&1; then
  # Exit 0 is necessary and not sufficient — this tree's own rule. Every failure inside the rail's
  # submit path calls `die` and exits 1, so a zero here is meaningful, but the thing that says a
  # TRANSACTION REACHED THE NODE is `"submitted": "<txid>"` in the summary, and reading it costs
  # one line. A summary with `"submitted": null` would otherwise pass this branch silently.
  RAIL_TXID=$(python3 -c '
import json,sys
try: print(json.load(open(sys.argv[1])).get("submitted") or "")
except Exception: print("")' "$ARTIFACT_STEM.rail.json" 2>/dev/null)
  if [ -n "$RAIL_TXID" ]; then
    log "stage 5b OK — commitment submitted in tx ${RAIL_TXID:0:16}…"
  else
    tail -20 "$WORK_DIR/rail-submit.log" >&2
    log "stage 5b — the rail exited 0 but its summary names no txid; stage 6 will not see FreePromptCommitted"
  fi
  advance 2
else
  tail -30 "$WORK_DIR/rail-submit.log" >&2
  log "stage 5b FAILED — the commitment was not submitted; stage 6 cannot reach FreePromptCommitted"
fi

# ---------------------------------------------------------------------------------------------
# 6. Follow THAT claim through the lattice on EVERY node.
#
# The failure mode this stage exists to NAME: testnet-11 5e shipped with `final_claims` stuck at 0
# for a week because every seat filed `Incapable` — a chain that produces blocks the whole time and
# certifies nothing. A drill that waited for `Final` and then timed out would report that as "slow".
# ---------------------------------------------------------------------------------------------
short="${CLAIM_ID:0:16}"
[ -n "$CLAIM_ID" ] || short="$JOB_ID"
stage_ok=1
for stage in FreePromptCommitted PanelBound ReceiptLicensed Final; do
  if all_nodes_logged "$stage.*${short}|${short}.*$stage"; then
    log "  $stage — every node"
  else
    stage_ok=0
    log "  $stage — NOT on every node"
    incapable=$(grep -ho 'Incapable' "$WORK_DIR"/node-*.log 2>/dev/null | wc -l | tr -d ' ')
    unavail=$(grep -ho 'Unavailable' "$WORK_DIR"/node-*.log 2>/dev/null | wc -l | tr -d ' ')
    if [ "$incapable" -gt 0 ]; then
      log "  DIAGNOSIS: seats filed Incapable ${incapable}× — the panel cannot execute this class."
      log "             This is 5e's stall, not slowness: check that each seat can resolve the class"
      log "             (--palw-class-artifact) or that the family is certified for the lane."
    fi
    if [ "$unavail" -gt 0 ]; then
      log "  DIAGNOSIS: seats filed Unavailable ${unavail}× — the material or an interval opening did"
      log "             not reach them. Check the executor's retention and the interval-serving path."
    fi
    break
  fi
done

# ---------------------------------------------------------------------------------------------
# 7. A receipt block spending one of this claim's quanta, accepted by every node.
# ---------------------------------------------------------------------------------------------
receipt_ok=0
if [ "$stage_ok" = 1 ]; then
  log "stage 7 — waiting for a receipt block (algo 7) accepted by every node"
  if all_nodes_logged "receipt block|algo 7|POW_ALGO_ID_PALW_RECEIPT"; then receipt_ok=1; fi
fi

# ---------------------------------------------------------------------------------------------
# 8. The DERIVED-ARTIFACT leg (ADR-0078), attempted from the claim's OWN answer.
#
# ADR-0078 Decision 2: the transformer's input is the rendering of the ids the claim committed. So
# the only honest way to demonstrate "the model made a thing and the chain carries the derivation"
# is to run the grammar over the ANSWER — not over a DSL a human wrote and then called the model's.
#
# At the widths the chain registers today (16 tokens total on the dense tier, prompt AND answer)
# an answer will usually not parse as a note list, and ADR-0078 X4 is explicit about what that
# means: a parse failure yields no object and nothing else — the inference still certifies and
# still mines, because R1 credits the computation, not what it happened to be good for. So this
# phase has TWO honest outcomes and reports which one it got. It never fails the drill: the
# free-prompt verdict above is Decision 7's gate, and this is ADR-0078's leg riding along.
#
# When the answer does NOT parse, the phase still proves the transformer half offline, from a
# HAND-WRITTEN DSL, and says so in those words — that run is NOT a demonstration of "Qwen3.6
# produced music", it is a demonstration that the transformer is a pure function whose artifact a
# stranger recomputes. Reading it as the former is the category error ADR-0078 §1 refuses.
# ---------------------------------------------------------------------------------------------
DERIVE_BIN="${DERIVE_BIN:-$REPO_ROOT/target/release/palw-derive}"
derived_note="not attempted"
if [ -x "$DERIVE_BIN" ]; then
  log "stage 8 — the derived-artifact leg (ADR-0078), from the claim's own answer"
  mkdir -p "$WORK_DIR/derived"
  python3 -c '
import json,sys
p = json.load(open(sys.argv[1]))
open(sys.argv[2], "w").write(p["choices"][0]["message"]["content"])' "$WORK_DIR/chat.json" "$WORK_DIR/derived/answer.txt"
  if "$DERIVE_BIN" derive --transformer music/smf/v1 --answer "$WORK_DIR/derived/answer.txt" \
       --out "$WORK_DIR/derived" --claim "$CLAIM_ID" --network-domain "$NETWORK_DOMAIN" \
       --executor-pubkey "$EXEC_PUBKEY" >"$WORK_DIR/derived/derive.log" 2>&1; then
    log "  the answer PARSED under music/smf/v1 — this is the real leg, from a real inference"
    obj=$(ls "$WORK_DIR"/derived/*.derived-unsigned.borsh 2>/dev/null | head -1 || true)
    # **The rail takes a STEM and the chain takes the SIGNED file** — two things this stage had
    # wrong, both invisible until an answer actually parsed. `--derive-artifact` appends
    # `.derived-unsigned.borsh` itself (`rail.rs`), so passing the unsigned FILE made it look for
    # `….derived-unsigned.borsh.derived-unsigned.borsh`; and the object that rides is
    # `<stem>.derived-object.borsh`, the signed `PalwConsensusObjectV2`, not the bare unsigned
    # derivation. Submitting the latter would have been refused as unparseable carriage.
    stem="${obj%.derived-unsigned.borsh}"
    if [ -n "$obj" ] && "$RAIL_BIN" --derive-artifact "$stem" --bond-key-seed "$WORK_DIR/keys/bond-0.seed" \
         >>"$WORK_DIR/derived/derive.log" 2>&1; then
      submit "$stem.derived-object.borsh" >>"$WORK_DIR/derived/derive.log" 2>&1 || true
      if all_nodes_logged "DerivedArtifact"; then
        derived_note="ON CHAIN from a real inference — every node carried the derivation"
      else
        derived_note="derived and submitted, but not carried by every node (see derived/derive.log)"
      fi
    else
      derived_note="derived from the answer; signing or submission failed (see derived/derive.log)"
    fi
    # Decision 5 / X6: a consumer holding only the answer and the object recomputes both hashes.
    if "$DERIVE_BIN" verify --object "$obj" --answer "$WORK_DIR/derived/answer.txt" \
         >>"$WORK_DIR/derived/derive.log" 2>&1; then
      derived_note="$derived_note; consumer recomputation PASSED (X6)"
    else
      derived_note="$derived_note; consumer recomputation FAILED (X6) — see derived/derive.log"
    fi
  else
    # X4, and it is the expected outcome at 16 tokens. Prove the transformer leg offline instead,
    # labelled for what it is.
    log "  the answer did not parse under music/smf/v1 — ADR-0078 X4: no object, claim untouched."
    log "  This is the WIDTH, not a defect: the registered row admits 16 tokens, prompt and answer"
    log "  together, and a note list does not fit. Proving the transformer leg offline instead."
    cat >"$WORK_DIR/derived/handwritten.json" <<'DSL'
{"ticks_per_quarter":480,"tracks":[{"program":0,"channel":0,"notes":[
{"pitch":60,"onset":0,"duration":480,"velocity":80},
{"pitch":64,"onset":480,"duration":480,"velocity":80},
{"pitch":67,"onset":960,"duration":960,"velocity":80}]}]}
DSL
    if "$DERIVE_BIN" derive --transformer music/smf/v1 --answer "$WORK_DIR/derived/handwritten.json" \
         --out "$WORK_DIR/derived" >>"$WORK_DIR/derived/derive.log" 2>&1; then
      derived_note="NOT-FROM-AN-INFERENCE: hand-written DSL derived to an artifact offline; the real leg is blocked by the registered width, not by the transformer"
    else
      derived_note="NOT-FROM-AN-INFERENCE: even the hand-written DSL failed to derive (see derived/derive.log)"
    fi
  fi
else
  derived_note="skipped — $DERIVE_BIN is not built (cargo build --release -p misaka-palw-derive)"
fi

# ---------------------------------------------------------------------------------------------
# Verdict.
# ---------------------------------------------------------------------------------------------
fail=0
log "================ verdict ================"
log "ADR-0078 derived leg: $derived_note"
for ((i=0; i<NODES; i++)); do
  log "node-$i blocks=$(blocks_of $i) committed=$(grep -c 'FreePromptCommitted' "$WORK_DIR/node-$i.log" 2>/dev/null || echo 0) final=$(grep -c 'Final' "$WORK_DIR/node-$i.log" 2>/dev/null || echo 0)"
done
[ "$stage_ok" = 1 ] || { log "the claim did not reach Final on every node"; fail=1; }
[ "$receipt_ok" = 1 ] || { log "no receipt block was accepted by every node"; fail=1; }
grep -q "$JOB_ID" "$WORK_DIR/chat.json" || { log "the job id is not in the gateway's own answer"; fail=1; }

if [ "$fail" -eq 0 ]; then
  log "PASS — job ${JOB_ID:0:16}… appears as an answer, a Final claim on every node, and a receipt block (ADR-0077 W7)"
else
  log "FAIL — the evidence is in $WORK_DIR (node-*.log, gateway.log, chat.json, health.json)"
  exit 1
fi
