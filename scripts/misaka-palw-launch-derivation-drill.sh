#!/usr/bin/env bash
# misaka-palw-launch-derivation-drill.sh — **the drill that proves the public launch's acceptance
# condition**, or names, in one line, the single thing that stops it.
#
# The condition, verbatim:
#
#   > On the PUBLIC chain: Qwen3.6 and the dense tier each produce MIDI data and 3D data,
#   > verified ADR-0078 style — the chain carries the derivation, the artifact is NOT shared, and
#   > a stranger recomputes it — through to block production. Plus free prompts with long,
#   > practical output.
#
# Every clause of that sentence is a stage below, and every stage either asserts something on a
# running chain or refuses BY NAME. Nothing is stubbed and nothing is weakened: a drill that goes
# green by testing less than the announcement claims is worse than no drill.
#
#   0  the stranger's own arithmetic, checked against the tree's oracles (no chain, no build)
#   0b the GATE'S PREMISE, checked against the source it names — so stage 7's wording can
#      never outlive the defect it describes
#   1  N validators from ONE build, bonded, panel-seated, producing
#   2  the class registered from its artifact (ADR-0054's permissionless route)
#   3  the family certified and the class bound to the FREE-PROMPT lane (ADR-0075)
#   4  the gateway answering, /health naming all four chain-side reasons
#   5  one browser-shaped request → an answer, a commitment, a claim id
#   6  that claim through the lattice to `Final` on EVERY validator, and a receipt block
#   7  ** THE GATE ** — can this class emit this kind's DSL at all? (see below)
#   8  the DerivedArtifactV1 in a BLOCK, carried by every validator, readable back over RPC
#   9  ADR-0078 X1 — the artifact bytes never rode; asserted, not assumed
#  10  a STRANGER recomputes output_root, dsl_hash, artifact_hash and derived_id, with a code
#      path that never calls the producer's
#  11  a free prompt with long, practical output
#
# …for BOTH kinds (`cad/stl/v1` and `music/smf/v1`) and BOTH tiers (the dense A16 row and the
# Qwen3.6 hybrid row).
#
# ---------------------------------------------------------------------------------------------
# STAGE 7, and why this drill has the shape it does
# ---------------------------------------------------------------------------------------------
#
# Measured on this tree, on a real converted Qwen2.5-1.5B, with the shipped `execute_free_prompt`
# and the shipped transformers:
#
#   * **The binding limit is LEAVES, not `n_ctx`.** `a16_execute_for_attempt_streaming_v1` calls
#     `kaspa_consensus_core::palw_step::step_leaf_count`, which is
#     `step_leaf_count_capped_v1(profile, ctx, PALW_STEP_MAX_LEAVES)` — the cap is the HARDCODED
#     `1 << 22` at `consensus/core/src/palw_step.rs:62`, and it is NOT read from the ruleset's
#     `PalwCourtParamsV2::max_step_leaf_count`. The shipped committed path therefore admits about
#     **38 total positions**, and **widening `n_ctx` does not move that by one token.** Measured:
#     a prompt of 26 prefill positions leaves 12 decode tokens (4,074,040 leaves against the
#     4,194,304 cap); 24 leaves 14. Six of eight test prompts got ZERO decode budget even at
#     `n_ctx` 512, because the prefill alone exhausts the ladder.
#   * `n_ctx` is a REAL second bound and this drill prints it too — the registered rows are 12
#     (BASE-0 floor, `PALW_RC_BASE0_GEOMETRY`), 16 (A16 dense, `QWEN25_1_5B_A16`) and 8 (the
#     Qwen3.6 hybrid, `QWEN36_35B_A3B`), prompt and answer together. The two bounds will
#     diverge the day somebody fixes the leaf cap, and a drill that named only one would be
#     wrong again afterwards. The drill READS this number from the gateway's `/health` rather
#     than assuming it: `grammar_floor.rs`'s header says the MoE row is `n_ctx` 9, which is
#     `QWEN3_CODER_30B_A3B` and is not the row testnet-11 registers.
#   * The grammar floors, from `misaka-palw-derive/tests/grammar_floor.rs` — which this script
#     PARSES rather than restates: `cad/stl/v1` 38 tokens, `music/smf/v1` 60, `scene/glb/v1` 104.
#   * **The MODEL clears the bar.** At `n_ctx` 256 it emits DSL the shipped transformers accept
#     with no post-processing at all — a real 82-byte format-1 Standard MIDI File and a 684-byte
#     binary STL — and generation is argmax over an integer logit row with no sampler and no seed,
#     so it is deterministic and one run per prompt is the honest experiment. **The acceptance
#     condition is not a model problem.**
#
# So stage 7 subtracts in LEAVES, names the executor's cap, and prints the `n_ctx` arithmetic as a
# second line. It exits non-zero there, and that is the point: today this drill is the shortest
# true statement of what stands between this tree and the announcement.
#
# Two failure modes are prompt-side and cheap, so the prompts below already have them fixed
# rather than reproducing them: the model wraps its answer in a markdown code fence unless asked
# not to, and it does not know the names `cad/v1` / `music/v1` and invents a plausible schema
# unless the keys are named in the prompt.
#
# ---------------------------------------------------------------------------------------------
# Running it
# ---------------------------------------------------------------------------------------------
#
#   cargo build --release -p kaspad -p misaka-cli -p misaka-palw-gateway -p misaka-palw-base0 \
#               -p misaka-palw-derive -p misaka-palw-certify
#
#   MISAKA_PALW_ARTIFACT=/path/dense.palwart \
#   MISAKA_PALW_TOKENIZER=/path/dense/tokenizer.json \
#   MISAKA_DEVNET_GENESIS=<128 hex; consensus/core/src/config/genesis.rs, DEVNET_GENESIS> \
#   scripts/misaka-palw-launch-derivation-drill.sh
#
# Add the second tier when its checkpoint is on the host. Without it the drill runs the dense tier
# and says, in the verdict, that half the acceptance condition was not demonstrated:
#
#   MISAKA_PALW_ARTIFACT_QWEN36=/path/qwen36.palwart \
#   MISAKA_PALW_TOKENIZER_QWEN36=/path/qwen36/tokenizer.json
#
# Other env: NODES (3), WORK_DIR, WAIT, STEP_WAIT, GATEWAY_PORT, KINDS, LONG_PROMPT_TOKENS.
#
# Exit: 0 the whole condition is demonstrated; 1 a stage could not run or a tier was skipped;
#       2 a stage FAILED on its own terms (stage 7's gate exits 2).
set -uo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
# `misaka-cli` is the PACKAGE; `misaka` is the binary it builds. Naming the package here is the
# mistake the certify drill made once and fixed: it dies at the first CLI call with a 127.
CLI_BIN="${CLI_BIN:-$REPO_ROOT/target/release/misaka}"
CERTIFY_BIN="${CERTIFY_BIN:-$REPO_ROOT/target/release/palw-certify}"
GATEWAY_BIN="${GATEWAY_BIN:-$REPO_ROOT/target/release/misaka-palw-gateway}"
RAIL_BIN="${RAIL_BIN:-$REPO_ROOT/target/release/misaka-palw-fp-rail}"
DERIVE_BIN="${DERIVE_BIN:-$REPO_ROOT/target/release/palw-derive}"
STRANGER="${STRANGER:-$REPO_ROOT/scripts/misaka-palw-derive-stranger.py}"
FLOOR_FILE="${FLOOR_FILE:-$REPO_ROOT/misaka-palw-derive/tests/grammar_floor.rs}"

NODES="${NODES:-3}"
WORK_DIR="${WORK_DIR:-$REPO_ROOT/.misaka-palw-launch-derivation}"
WAIT="${WAIT:-900}"
STEP_WAIT="${STEP_WAIT:-600}"
GATEWAY_PORT="${GATEWAY_PORT:-18801}"
KINDS="${KINDS:-cad/stl/v1 music/smf/v1}"
LONG_PROMPT_TOKENS="${LONG_PROMPT_TOKENS:-400}"
PREMINE_TXID="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"   # "misaka-premine", zero-padded
MAIN_PREMINE_INDEX=40   # consensus/core/src/config/premine.rs; bond n's fee float is +1+n

# The two tiers the acceptance condition names, and the family binary that serves each row.
DENSE_MODEL_ID="${DENSE_MODEL_ID:-Qwen/Qwen2.5-1.5B/graph-v2}"
DENSE_WORKER="${DENSE_WORKER:-$REPO_ROOT/target/release/palw-a16-fp-worker}"
QWEN36_MODEL_ID="${QWEN36_MODEL_ID:-Qwen3.6-35B-A3B/graph-v3}"
QWEN36_WORKER="${QWEN36_WORKER:-$REPO_ROOT/target/release/palw-qwen36-fp-worker}"

log()  { printf '[launch-drill] %s\n' "$*" >&2; }
step() { printf '\n[launch-drill] ── %s\n' "$*" >&2; }
die()  { log "FATAL: $*"; exit 1; }

# Each stage records its own verdict line and the run ends by printing them all. A stage that
# could not run says so; a stage that failed says why. The exit code is the worst outcome.
VERDICT_FILE=""
WORST=0
record() { printf '%s\n' "$*" >>"$VERDICT_FILE"; log "  $*"; }
worse_than() { [ "$1" -gt "$WORST" ] && WORST="$1"; return 0; }

# ---------------------------------------------------------------------------------------------
# Preflight — each refusal names the thing, rather than failing thirty minutes in.
# ---------------------------------------------------------------------------------------------
command -v python3 >/dev/null || die "python3 is required (key derivation, the HTTP client and the stranger's arithmetic)"
for b in "$KASPAD_BIN" "$CLI_BIN" "$CERTIFY_BIN" "$GATEWAY_BIN" "$RAIL_BIN" "$DERIVE_BIN"; do
  [ -x "$b" ] || die "$b is not an executable. Build it: cargo build --release -p <its crate>"
done
[ -f "$STRANGER" ] || die "$STRANGER is missing — stage 10's independent recomputation lives there"
[ -f "$FLOOR_FILE" ] || die "$FLOOR_FILE is missing — the grammar floors are PARSED from it and never restated here"
[ -n "${MISAKA_PALW_ARTIFACT:-}" ] || die "MISAKA_PALW_ARTIFACT must name the dense tier's .palwart — the drill registers the class FROM it and the worker serves it"
[ -n "${MISAKA_PALW_TOKENIZER:-}" ] || die "MISAKA_PALW_TOKENIZER must name that artifact's tokenizer.json"
[ -f "$MISAKA_PALW_ARTIFACT" ] || die "MISAKA_PALW_ARTIFACT=$MISAKA_PALW_ARTIFACT does not exist"
[ -f "$MISAKA_PALW_TOKENIZER" ] || die "MISAKA_PALW_TOKENIZER=$MISAKA_PALW_TOKENIZER does not exist"
# The one value no running node prints: `network_domain` binds the GENESIS, and a wrong value does
# not fail loudly — it produces claims whose context hash no seat can reproduce, every one of which
# collects an `Unavailable` quorum and DEFAULTS its producer.
[ -n "${MISAKA_DEVNET_GENESIS:-}" ] || die "MISAKA_DEVNET_GENESIS must be the devnet genesis hash, 128 hex (consensus/core/src/config/genesis.rs, DEVNET_GENESIS). A guessed value silently produces claims no seat can replay."
[ "${#MISAKA_DEVNET_GENESIS}" -eq 128 ] || die "MISAKA_DEVNET_GENESIS is ${#MISAKA_DEVNET_GENESIS} chars, not 128"

rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR/keys" "$WORK_DIR/obj" "$WORK_DIR/secrets" "$WORK_DIR/derived" "$WORK_DIR/chain"
chmod 700 "$WORK_DIR/secrets"
VERDICT_FILE="$WORK_DIR/verdict.txt"; : >"$VERDICT_FILE"

# ---------------------------------------------------------------------------------------------
# The floors, PARSED from the pinned test rather than restated.
#
# `derived-sets-need-one-spelling`: a floor table written twice is a floor table that drifts. The
# authority is `tests/grammar_floor.rs`, whose shrinker MEASURES each number and whose assertions
# make a grammar edit that moves one come and say so.
# ---------------------------------------------------------------------------------------------
FLOOR_ENV=$(python3 - "$FLOOR_FILE" <<'PY'
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
cases = re.findall(r'transformer:\s*"([^"]+)".*?floor_tokens:\s*(\d+)', text, re.S)
for name, tokens in cases:
    print("FLOOR_" + re.sub(r"[^A-Za-z0-9]", "_", name).upper() + "=" + tokens)
print("FLOOR_NAMES='" + " ".join(n for n, _ in cases) + "'")
PY
) || die "could not parse the grammar floors out of $FLOOR_FILE"
eval "$FLOOR_ENV"
[ -n "${FLOOR_NAMES:-}" ] || die "no grammar floors parsed from $FLOOR_FILE — its Case table changed shape"
log "grammar floors, from $FLOOR_FILE: $(printf '%s' "$FLOOR_ENV" | grep '^FLOOR_[A-Z]' | tr '\n' ' ')"
floor_of() {
  local var
  var="FLOOR_$(printf '%s' "$1" | tr 'a-z/.-' 'A-Z____')"
  printf '%s' "${!var:-0}"
}

# ---------------------------------------------------------------------------------------------
# Stage 0 — the stranger's own arithmetic, before anything is claimed with it.
#
# Stage 10's whole value is that the recomputation is a SECOND computation. A second computation
# that is wrong proves nothing and accuses the innocent, so it is checked first — against the
# pinned transformer ids, the corpus goldens and the refusal corpus. No chain, no build.
# ---------------------------------------------------------------------------------------------
step "stage 0 — the independent verifier's selftest (no chain, no build)"
if python3 "$STRANGER" selftest >"$WORK_DIR/stranger-selftest.log" 2>&1; then
  record "stage 0 PASS — $(sed -n 's/^SELFTEST PASSED — \(.*\)$/\1/p' "$WORK_DIR/stranger-selftest.log" | head -1)"
else
  tail -20 "$WORK_DIR/stranger-selftest.log" >&2
  record "stage 0 FAIL — the independent verifier disagrees with the shipped tree; stage 10 cannot be trusted"
  die "stage 0 failed; see $WORK_DIR/stranger-selftest.log"
fi

# ---------------------------------------------------------------------------------------------
# Stage 0b — **the gate's premise, checked against the source it names.**
#
# Stage 7's refusal makes a claim about somebody else's code: that the executor counts leaves
# against a hardcoded constant and never reads the ruleset's own ceiling. The day that is fixed,
# a drill still printing it would be sending the next reader to widen something that already
# moved. So the premise is ASSERTED here, and the drill says so when it stops holding.
#
# Static, cheap and read-only: it greps the tree, builds nothing, and touches no consensus file.
# ---------------------------------------------------------------------------------------------
step "stage 0b — the gate's premise, checked against the source"
PREMISE_OK=1
premise() {
  local what="$1" file="$2" pattern="$3"
  if grep -qE "$pattern" "$REPO_ROOT/$file" 2>/dev/null; then
    log "  premise holds: $what"
  else
    PREMISE_OK=0
    record "stage 0b — PREMISE MOVED: $what is no longer true in $file."
  fi
}
premise "PALW_STEP_MAX_LEAVES is the hardcoded 1 << 22" \
        "consensus/core/src/palw_step.rs" 'pub const PALW_STEP_MAX_LEAVES: u64 = 1 << 22;'
premise "step_leaf_count counts against that CONSTANT and not against the ruleset" \
        "consensus/core/src/palw_step.rs" 'step_leaf_count_capped_v1\(profile, context, PALW_STEP_MAX_LEAVES\)'
premise "the dense executor calls the constant-capped step_leaf_count" \
        "misaka-palw-base0/src/qwen25_a16_backend.rs" 'palw_step::step_leaf_count\(profile, ctx\)'
premise "the Qwen3.6 executor calls the constant-capped step_leaf_count" \
        "misaka-palw-base0/src/qwen36_backend.rs" 'palw_step::step_leaf_count\(profile, ctx\)'
if [ "$PREMISE_OK" = 1 ] && ! grep -rq 'max_step_leaf_count()' "$REPO_ROOT/misaka-palw-base0/src/" 2>/dev/null; then
  record "stage 0b PASS — the executor's leaf cap is the hardcoded PALW_STEP_MAX_LEAVES (2^22) and NO executor path reads PalwCourtParamsV2::max_step_leaf_count. Stage 7's refusal is about code that is still there."
elif [ "$PREMISE_OK" = 1 ]; then
  record "stage 0b CHANGED — an executor path now reads max_step_leaf_count(). Re-read stage 7's message before believing it: the ruleset may now be the binding ceiling."
  worse_than 1
else
  record "stage 0b CHANGED — stage 7's refusal quotes source that has moved. Do NOT relay its wording without re-reading the files it names."
  worse_than 1
fi

# ---------------------------------------------------------------------------------------------
# Keys. Public devnet seeds (consensus/core/src/config/premine.rs): value-less and derivable by
# anyone, which is what makes a drill reproducible on somebody else's machine.
#
# TWO forms of the same seed, because the tree has two readers of it and they disagree:
#   * `keys/bond-n.seed`   — 64 ASCII hex characters, what `misaka --key-file` reads;
#   * `secrets/bond-n.raw` — the 32 RAW bytes, what `ValidatorKey::from_seed` reads through
#     `misaka-palw-fp-rail --bond-key-seed` and the gateway's `--derive-seed`
#     (`VALIDATOR_SEED_LEN = 32`; both refuse a 64-byte file by name).
# `scripts/misaka-palw-fp-devnet-e2e.sh` passes the hex file to `--bond-key-seed` and therefore
# dies at "the bond key seed is 64 bytes, not 32" — the conversion below is why this drill does
# not. The raw copies live in their own directory because the gateway refuses to boot when a
# 32-byte file is reachable in its identity directory or its outbox (ADR-0079 Decision 4).
# ---------------------------------------------------------------------------------------------
python3 - "$WORK_DIR/keys" "$WORK_DIR/secrets" "$NODES" <<'PY'
import hashlib, os, sys
keys, secrets, n = sys.argv[1], sys.argv[2], int(sys.argv[3])
h = lambda b: hashlib.blake2b(b, digest_size=32).hexdigest()
for i in range(n):
    hexed = h(b"misaka-devnet-genesis-bond-v1/" + str(i).encode())
    p = f"{keys}/bond-{i}.seed"; open(p, "w").write(hexed); os.chmod(p, 0o600)
    r = f"{secrets}/bond-{i}.raw"; open(r, "wb").write(bytes.fromhex(hexed)); os.chmod(r, 0o600)
p = f"{keys}/main.seed"; open(p, "w").write(h(b"misaka-testnet-premine-9b-claude-managed")); os.chmod(p, 0o600)
PY

# `palw_attempt_v2::palw_network_domain_v2_for` verbatim: blake2b-512 KEYED by the network-domain
# constant, over `u64le(len(net)) ‖ net ‖ genesis`. Recomputing a consensus hash in Python is only
# safe because this tree already pins the equivalence in
# `scripts/misaka-palw-fp-v3-worker-smoke.py`.
NETWORK_DOMAIN=$(python3 - "$MISAKA_DEVNET_GENESIS" <<'PY'
import hashlib, struct, sys
net = b"devnet"
h = hashlib.blake2b(digest_size=64, key=b"misaka-palw/attempt-v2/network-domain/v1")
h.update(struct.pack("<Q", len(net))); h.update(net); h.update(bytes.fromhex(sys.argv[1]))
print(h.hexdigest())
PY
)
log "network domain ${NETWORK_DOMAIN:0:16}… (devnet ‖ genesis ${MISAKA_DEVNET_GENESIS:0:16}…)"

pids=()
cleanup() { for p in "${pids[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT

# ---------------------------------------------------------------------------------------------
# Stage 1 — N validators from ONE build: each a floor producer under devnet public-seed bond n,
# each a panel seat, node-0 listening and the rest connecting to it.
# ---------------------------------------------------------------------------------------------
step "stage 1 — $NODES validators from one build"
declare -a ADDRS
for ((i=0; i<NODES; i++)); do
  addr="$("$CLI_BIN" --network devnet key address --key-file "$WORK_DIR/keys/bond-$i.seed" | tail -1 | awk '{print $NF}')"
  [ -n "$addr" ] || die "cannot derive bond $i's address"
  ADDRS[$i]="$addr"
  p2p=$((16510 + i)); rpc=$((17810 + i))
  # --nogrpc: every node would otherwise bind the same default gRPC port and the later ones exit.
  # --enable-unsynced-mining: a chain with only a genesis is "not synced"; the producer still
  # requires peers and open participation, so the gate's other clauses are not waived.
  # --palw-panel: SEATS certify a claim, so every node is a seat.
  args=(--devnet --appdir="$WORK_DIR/node-$i" --listen=127.0.0.1:$p2p --rpclisten-borsh=127.0.0.1:$rpc
        --utxoindex --nodnsseed --disable-upnp --nogrpc --enable-unsynced-mining --palw-panel
        --palw-produce --palw-producer-key="$WORK_DIR/keys/bond-$i.seed"
        --palw-producer-bond="$PREMINE_TXID:$i" --palw-producer-pay-address="$addr"
        --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + i))")
  [ "$i" -gt 0 ] && args+=(--connect=127.0.0.1:16510)
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >"$WORK_DIR/node-$i.log" 2>&1 &
  # `$!` into a variable rather than `${pids[-1]}`: macOS ships bash 3.2, which rejects a negative
  # array index at PARSE time — the whole script fails to load, not the line.
  node_pid=$!
  pids+=("$node_pid")
  log "node-$i pid $node_pid rpc 127.0.0.1:$rpc bond $PREMINE_TXID:$i"
done

CLI=("$CLI_BIN" --network devnet --rpc 127.0.0.1:17810)
blocks_of() { grep -c "produced block #" "$WORK_DIR/node-$1.log" 2>/dev/null || true; }
count_on() { grep -cE "$2" "$WORK_DIR/node-$1.log" 2>/dev/null || true; }

# Every wait is a POLL against the chain rather than a fixed sleep: the floor's draw rate is a
# seeded, per-class quantity (ADR-0076) and every fixed sleep in this repo's drills has had to be
# re-sized at least once.
advance() {
  local want="${1:-1}" from now deadline
  from=$(blocks_of 0); deadline=$((SECONDS + STEP_WAIT))
  while :; do
    now=$(blocks_of 0)
    [ $((now - from)) -ge "$want" ] && return 0
    [ $SECONDS -lt $deadline ] || { log "node-0 gained $((now - from))/$want block(s) in ${STEP_WAIT}s — continuing to the verdict"; return 1; }
    sleep 2
  done
}
all_nodes_logged() {
  local pattern="$1" deadline=$((SECONDS + STEP_WAIT)) i ok
  while :; do
    ok=1
    for ((i=0; i<NODES; i++)); do grep -qE "$pattern" "$WORK_DIR/node-$i.log" || ok=0; done
    [ "$ok" = 1 ] && return 0
    [ $SECONDS -lt $deadline ] || return 1
    sleep 3
  done
}
# Wait until EVERY node's count of `pattern` exceeds the baseline it had in `BASELINE[i]`. The
# count form is what makes a per-kind assertion possible: a second derivation on the same chain
# would otherwise be "proved" by the first one's log line.
declare -a BASELINE
baseline_of() {
  local pattern="$1" i
  for ((i=0; i<NODES; i++)); do BASELINE[$i]=$(count_on "$i" "$pattern"); done
}
all_nodes_gained() {
  local pattern="$1" deadline=$((SECONDS + STEP_WAIT)) i ok now
  while :; do
    ok=1
    for ((i=0; i<NODES; i++)); do
      now=$(count_on "$i" "$pattern")
      [ "${now:-0}" -gt "${BASELINE[$i]:-0}" ] || ok=0
    done
    [ "$ok" = 1 ] && return 0
    [ $SECONDS -lt $deadline ] || return 1
    sleep 3
  done
}

deadline=$((SECONDS + WAIT))
until [ "$(blocks_of 0)" -ge 3 ]; do
  [ $SECONDS -lt $deadline ] || { tail -40 "$WORK_DIR/node-0.log" >&2; die "node-0 produced no blocks within ${WAIT}s"; }
  sleep 3
done
advance 1 || true
record "stage 1 PASS — node-0 produced $(blocks_of 0) blocks and the peers followed ($NODES validators, one build)"

submit_object() {
  local f="$1"; local args=()
  if ls "$f".chunk* >/dev/null 2>&1; then
    for c in $(ls "$f".chunk* | sort -t k -k3 -n); do args+=(--object "$c"); done
  else
    args=(--object "$f")
  fi
  "${CLI[@]}" palw submit-object --key-file "$WORK_DIR/keys/main.seed" "${args[@]}" --yes
  local rc=$?
  # A chunked object needs one carrier per chunk, and the next burst spends this change.
  advance 2 || true
  return $rc
}

# ---------------------------------------------------------------------------------------------
# The prompts. Both known prompt-side failure modes are already fixed here:
#   * "no markdown code fence, no backticks" — the model wraps its answer otherwise (measured);
#   * the schema keys are NAMED — the model does not know `cad/v1` / `music/v1` and invents a
#     plausible schema when told only the grammar's name (measured; naming the keys fixes it).
# ---------------------------------------------------------------------------------------------
prompt_for() {
  case "$1" in
    cad/stl/v1)
      printf '%s' 'Output one JSON object and nothing else. No prose, no explanation, no markdown code fence, no backticks. Exactly these four keys: "v" set to 1, "frac_bits" set to 0, "sketches" set to {}, and "solid". "solid" has exactly three keys: "op" set to "box", "min" set to an array of three integers, and "max" set to an array of three integers, each min below its max. Every number is an integer. Make a block 4 wide, 2 deep and 3 tall at the origin.' ;;
    music/smf/v1)
      printf '%s' 'Output one JSON object and nothing else. No prose, no explanation, no markdown code fence, no backticks. Exactly these five keys: "v" set to 1, "ppq" set to 480, "tempo_us_per_quarter" set to 500000, "time_signature" set to [4,4], and "tracks". "tracks" is an array of objects with exactly the keys "name" (a string), "channel" (0), "program" (0) and "notes". Each note is an object with exactly the keys "pitch", "velocity", "onset" and "duration", all integers. Write one track: a three-note C major arpeggio in quarter notes.' ;;
    scene/glb/v1)
      printf '%s' 'Output one JSON object and nothing else. No prose, no markdown code fence, no backticks. Exactly these four keys: "v" set to 1, "frac_bits" set to 0, "materials" and "nodes". Every number is an integer. One material and one box node.' ;;
    *)
      printf 'Output one JSON object in the %s grammar and nothing else. No prose, no markdown code fence, no backticks.' "$1" ;;
  esac
}

# ---------------------------------------------------------------------------------------------
# One chat request. Writes the response to $5; on a refusal, the error body to last-error.txt.
# ---------------------------------------------------------------------------------------------
chat_request() {
  local port="$1" prompt="$2" max_tokens="$3" derive_kind="$4" out="$5"
  : >"$WORK_DIR/last-error.txt"
  python3 - "$port" "$prompt" "$max_tokens" "$derive_kind" "$out" "$WORK_DIR/last-error.txt" <<'PY'
import json, sys, urllib.error, urllib.request
port, prompt, max_tokens, derive_kind, out, errfile = sys.argv[1:7]
body = {"messages": [{"role": "user", "content": prompt}], "max_tokens": int(max_tokens)}
if derive_kind:
    body["derive"] = derive_kind
req = urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions",
                             data=json.dumps(body).encode(),
                             headers={"content-type": "application/json"})
try:
    payload = json.loads(urllib.request.urlopen(req, timeout=3600).read())
except urllib.error.HTTPError as e:
    open(errfile, "w").write(e.read().decode("utf-8", "replace"))
    sys.exit(1)
except Exception as e:
    open(errfile, "w").write(f"{type(e).__name__}: {e}")
    sys.exit(1)
json.dump(payload, open(out, "w"), indent=2)
print(f"  answer ({payload['usage']['completion_tokens']} tok): "
      f"{payload['choices'][0]['message']['content'][:110]!r}", file=sys.stderr)
PY
}

jget() { python3 -c 'import json,sys
d=json.load(open(sys.argv[1]))
for k in sys.argv[2].split("."):
    if d is None: break
    d = d.get(k) if isinstance(d, dict) else None
print("" if d is None else d)' "$1" "$2"; }

# ---------------------------------------------------------------------------------------------
# The largest decode budget this class actually admits for THIS prompt, by bisection on the
# executor's own refusal.
#
# A MEASUREMENT and not a formula: `leaves_per_position` lives in `consensus/`, and a second copy
# of it here would be a second answer to a question the executor already answers. A refused probe
# costs nothing (the leaf count is taken before the first forward pass); only the accepted ones
# cost an inference, and there are at most log2(floor) of those.
# ---------------------------------------------------------------------------------------------
largest_admissible_decode() {
  local port="$1" kind="$2" hi="$3" lo=0 mid best=0
  local probe="$WORK_DIR/bisect.json"
  while [ $((hi - lo)) -gt 1 ]; do
    mid=$(( (lo + hi) / 2 ))
    if chat_request "$port" "$(prompt_for "$kind")" "$mid" "" "$probe" >/dev/null 2>&1; then
      lo="$mid"; best="$mid"
    else
      hi="$mid"
    fi
  done
  printf '%s' "$best"
}

# ---------------------------------------------------------------------------------------------
# One kind, on one tier: the gate, the block, X1, and the stranger.
# ---------------------------------------------------------------------------------------------
run_kind() {
  local tag="$1" port="$2" kind="$3" class_id="$4" n_ctx="$5" prefill="$6"
  local floor slug out
  floor=$(floor_of "$kind")
  slug=$(printf '%s' "$kind" | tr '/.' '__')
  out="$WORK_DIR/derive-$tag-$slug.json"

  step "stage 7 [$tag/$kind] — can this class emit this kind's DSL? (floor $floor tokens)"
  if [ "$floor" -le 0 ]; then
    record "stage 7 [$tag/$kind] FAIL — no floor is pinned for $kind in $FLOOR_FILE"
    worse_than 1; return 0
  fi

  if ! chat_request "$port" "$(prompt_for "$kind")" "$floor" "$kind" "$out"; then
    local err admissible got max
    err=$(cat "$WORK_DIR/last-error.txt" 2>/dev/null)
    # ------------------------------------------------------------------------------------------
    # THE GATE. Which gate it is decides what the next reader should go and fix, so the drill
    # separates them instead of printing one generic refusal.
    # ------------------------------------------------------------------------------------------
    if printf '%s' "$err" | grep -q 'TooManyLeaves'; then
      got=$(printf '%s' "$err" | sed -n 's/.*TooManyLeaves { got: \([0-9]*\).*/\1/p' | head -1)
      max=$(printf '%s' "$err" | sed -n 's/.*max: \([0-9]*\).*/\1/p' | head -1)
      admissible=$(largest_admissible_decode "$port" "$kind" "$floor")
      record "stage 7 [$tag/$kind] FAIL — THE EXECUTOR LEAF CAP:"
      record "  class $tag (${class_id:0:16}…): prompt prefill $prefill positions costs $got of the"
      record "  executor's $max leaf cap (PALW_STEP_MAX_LEAVES, hardcoded in step_leaf_count at"
      record "  consensus/core/src/palw_step.rs:62, and NOT read from the ruleset's"
      record "  PalwCourtParamsV2::max_step_leaf_count), leaving $admissible decode tokens;"
      record "  kind $kind's floor is $floor tokens."
      record "  THIS IS THE EXECUTOR LEAF CAP, NOT n_ctx — widening the context does not move it."
      record "  Second bound, real and separate: by context alone this class admits n_ctx $n_ctx −"
      record "  prefill $prefill = $((n_ctx - prefill)) decode tokens. The two diverge once either is fixed,"
      record "  which is why the drill names both."
      record "  ADR-0080 W1 fixes the COURT's side of this literal; the EXECUTOR's side —"
      record "  step_leaf_count reading the ruleset instead of the constant — is a separate"
      record "  prerequisite and is not covered by it."
      record "  Not a broken drill, and not a model problem: at n_ctx 256 the model emits DSL the"
      record "  shipped transformers accept with no post-processing (82-byte SMF, 684-byte STL)."
      worse_than 2; return 0
    fi
    if printf '%s' "$err" | grep -qi 'max_context_tokens\|exceeds max_context\|ContextOverflow\|JobExceedsClassContext'; then
      record "stage 7 [$tag/$kind] FAIL — THE CONTEXT GATE (n_ctx), which is a different gate:"
      record "  class $tag admits n_ctx $n_ctx total positions and this prompt's prefill is $prefill,"
      record "  so the decode budget is $((n_ctx - prefill)) and kind $kind's floor is $floor tokens."
      record "  Worker refusal: $(printf '%s' "$err" | tr '\n' ' ' | head -c 240)"
      record "  Note that the executor's leaf cap is a SECOND ceiling and may bind first once this"
      record "  one is widened (see stage 7's header in this script)."
      worse_than 2; return 0
    fi
    record "stage 7 [$tag/$kind] FAIL — refused for a reason that is neither gate: $(printf '%s' "$err" | tr '\n' ' ' | head -c 300)"
    worse_than 2; return 0
  fi

  # The job ran. Did the ANSWER derive? (ADR-0078 X4: a parse failure yields no object and touches
  # the claim not at all — the inference still certifies and still mines.)
  local status emitted reason
  status=$(jget "$out" "misaka.derivation.status")
  emitted=$(jget "$out" "usage.completion_tokens")
  if [ "$status" != "derived" ]; then
    reason=$(jget "$out" "misaka.derivation.reason")
    [ -n "$reason" ] || reason=$(jget "$out" "misaka.not_derived_because")
    [ -n "$reason" ] || reason="no derivation block in the response"
    if [ "${emitted:-0}" -lt "$floor" ]; then
      record "stage 7 [$tag/$kind] FAIL — the class emitted ${emitted:-0} decode tokens against a $floor-token floor, so the answer could not have been a legal $kind sentence whatever it said. Still the width, arriving as a parse failure. Refusal: $(printf '%s' "$reason" | head -c 200)"
    else
      record "stage 7 [$tag/$kind] FAIL — the class HAD room (${emitted:-0} decode tokens against a $floor-token floor) and the answer still did not parse. THIS IS NOT THE WIDTH: it is the prompt or the model. Refusal: $(printf '%s' "$reason" | head -c 240)"
    fi
    worse_than 2; return 0
  fi
  local claim_id derived_id artifact_bytes
  claim_id=$(jget "$out" "misaka.fp_claim_id")
  derived_id=$(jget "$out" "misaka.derivation.derived_id")
  artifact_bytes=$(jget "$out" "misaka.derivation.artifact_bytes")
  record "stage 7 [$tag/$kind] PASS — the class's OWN answer ($emitted decode tokens) canonicalized under the grammar and produced a $artifact_bytes-byte artifact; derived ${derived_id:0:16}…"

  # -------------------------------------------------------------------------------------------
  # Stage 8 — the object into a BLOCK, carried by every validator, readable back over RPC.
  # -------------------------------------------------------------------------------------------
  step "stage 8 [$tag/$kind] — the DerivedArtifactV1 into a block"
  local objfile signed stem
  objfile=$(jget "$out" "misaka.derivation.files.object")
  [ -f "$objfile" ] || { record "stage 8 [$tag/$kind] FAIL — the gateway named an object file that is not there: $objfile"; worse_than 1; return 0; }
  signed=$(jget "$out" "misaka.derivation.signed")
  if [ "$signed" != "True" ] && [ "$signed" != "true" ]; then
    # No seed reached the gateway; the rail signs it instead (the two-process form).
    stem="${objfile%.derived-unsigned.borsh}"
    if ! "$RAIL_BIN" --derive-artifact "$stem" --bond-key-seed "$WORK_DIR/secrets/bond-0.raw" \
         >>"$WORK_DIR/derive-$tag-$slug.log" 2>&1; then
      record "stage 8 [$tag/$kind] FAIL — the rail could not sign the derivation (see derive-$tag-$slug.log)"
      worse_than 1; return 0
    fi
    objfile="$stem.derived-object.borsh"
  fi
  cp "$objfile" "$WORK_DIR/derived/$tag-$slug.derived-object.borsh"
  baseline_of "PALW lifecycle carried.*DerivedArtifactV1"
  if ! submit_object "$objfile" >>"$WORK_DIR/derive-$tag-$slug.log" 2>&1; then
    record "stage 8 [$tag/$kind] FAIL — submitting the derivation was refused (see derive-$tag-$slug.log)"
    worse_than 1; return 0
  fi
  if ! all_nodes_gained "PALW lifecycle carried.*DerivedArtifactV1"; then
    record "stage 8 [$tag/$kind] FAIL — not every validator carried a NEW DerivedArtifactV1 within ${STEP_WAIT}s"
    worse_than 1; return 0
  fi
  local chainread="$WORK_DIR/chain/$tag-$slug.json" rows
  if ! "${CLI[@]}" palw derived "$claim_id" --json >"$chainread" 2>"$WORK_DIR/chain/$tag-$slug.err"; then
    record "stage 8 [$tag/$kind] FAIL — the chain does not hold claim ${claim_id:0:16}…: $(head -c 200 "$WORK_DIR/chain/$tag-$slug.err")"
    worse_than 1; return 0
  fi
  rows=$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))["artifacts"]))' "$chainread")
  if [ "${rows:-0}" -lt 1 ]; then
    record "stage 8 [$tag/$kind] FAIL — the claim is on chain and carries no derivation row"
    worse_than 1; return 0
  fi
  record "stage 8 [$tag/$kind] PASS — every validator carried the object in a block; the chain returns $rows derivation row(s) for this claim"

  # -------------------------------------------------------------------------------------------
  # Stage 9 — ADR-0078 X1: the artifact never rode. Asserted on the bytes that ACTUALLY rode,
  # rather than inferred from the object's field list.
  # -------------------------------------------------------------------------------------------
  step "stage 9 [$tag/$kind] — the artifact never touched the chain"
  local artfile x1
  artfile=$(jget "$out" "misaka.derivation.files.artifact")
  if x1=$(python3 - "$objfile" "$artfile" "$chainread" <<'PY'
import json, sys
carrier = open(sys.argv[1], "rb").read()
artifact = open(sys.argv[2], "rb").read()
chain = json.load(open(sys.argv[3]))
problems = []
# 1. The bytes that rode do not contain the artifact.
if artifact and artifact in carrier:
    problems.append("the submitted carrier CONTAINS the artifact bytes")
# 2. The carrier's size does not grow with the artifact: a `DerivedArtifactV1` is eleven fixed
#    fields plus one ML-DSA-87 public key and one signature, and nothing else may ride (X1).
if len(carrier) > 16384:
    problems.append(f"the carrier is {len(carrier)} bytes — a DerivedArtifactV1 plus its signature is not")
# 3. What the chain hands back is names and a byte COUNT, never a payload.
for row in chain.get("artifacts", []):
    if not isinstance(row.get("artifact_bytes"), int):
        problems.append("the chain returned artifact_bytes as something other than a count")
    for k, v in row.items():
        if isinstance(v, str) and len(v) > 200:
            problems.append(f"the chain returned an oversized field {k!r} — a payload, not a name")
if problems:
    print("; ".join(problems)); sys.exit(1)
print(f"{len(carrier)}-byte carrier, {len(artifact)}-byte artifact, and the artifact is not inside the carrier")
PY
  ); then
    record "stage 9 [$tag/$kind] PASS — ADR-0078 X1 asserted on the bytes that rode: $x1"
  else
    record "stage 9 [$tag/$kind] FAIL — the artifact reached the chain, or the chain hands back a payload (ADR-0078 X1): $x1"
    worse_than 2; return 0
  fi

  # -------------------------------------------------------------------------------------------
  # Stage 10 — a STRANGER recomputes. Twice, and the two are not the same kind of evidence.
  #
  #   (a) the SHIPPED consumer path, `misaka palw derived-verify`, which calls
  #       `misaka_palw_derive::verify` — the producer's own function;
  #   (b) an INDEPENDENT path, `misaka-palw-derive-stranger.py`, which links nothing and rebuilds
  #       every hash from ADR-0078's own preimages. Stage 0's selftest is what makes it evidence
  #       rather than an opinion.
  #
  # Both must agree. (a) alone is a transformer agreeing with itself.
  #
  # The answer handed to the stranger is the UNTRIMMED rendering the gateway committed — what the
  # grammar actually consumed — so canonicalization is exercised and not assumed. The canonical
  # DSL from the response is the fallback when the outbox summary is unreadable.
  # -------------------------------------------------------------------------------------------
  step "stage 10 [$tag/$kind] — a stranger recomputes"
  local answer="$WORK_DIR/derived/$tag-$slug.answer.txt" summary
  summary=$(jget "$out" "misaka.artifact")
  python3 - "$out" "$summary" "$answer" <<'PY'
import json, sys
response, summary_path, answer_path = sys.argv[1:4]
m = json.load(open(response))["misaka"]
text = None
try:
    text = json.load(open(summary_path)).get("answer_untrimmed")
except Exception:
    pass
if not text:
    text = m["derivation"]["dsl"]
open(answer_path, "w").write(text)
PY
  local shipped=1 independent=1
  if "${CLI[@]}" palw derived-verify "$claim_id" --answer "$out" --json \
       >"$WORK_DIR/derived/$tag-$slug.shipped-verify.json" 2>&1; then shipped=0; fi
  if python3 "$STRANGER" verify --chain "$chainread" --answer "$answer" --gateway "$out" \
       --artifact "$artfile" >"$WORK_DIR/derived/$tag-$slug.stranger-verify.json" 2>&1; then independent=0; fi
  if [ "$independent" = 0 ] && [ "$shipped" = 0 ]; then
    record "stage 10 [$tag/$kind] PASS — output_root, dsl_hash, artifact_hash and derived_id recomputed and matched by BOTH the shipped consumer path and an independent one that never calls the producer's code"
  elif [ "$independent" = 0 ]; then
    record "stage 10 [$tag/$kind] FAIL — the INDEPENDENT path matched and the shipped one did not: $(tr '\n' ' ' <"$WORK_DIR/derived/$tag-$slug.shipped-verify.json" | head -c 240)"
    worse_than 2
  elif [ "$shipped" = 0 ]; then
    record "stage 10 [$tag/$kind] FAIL — the shipped path matched and the INDEPENDENT one did not. Read $WORK_DIR/derived/$tag-$slug.stranger-verify.json: either the object is false, or this kind is outside the independent verifier's implemented subset — which is NOT a pass and must not be read as one."
    worse_than 2
  else
    record "stage 10 [$tag/$kind] FAIL — neither path reproduced the chain's derivation; see $WORK_DIR/derived/"
    worse_than 2
  fi
}

# ---------------------------------------------------------------------------------------------
# One tier, end to end.
# ---------------------------------------------------------------------------------------------
run_tier() {
  local tier_index="$1" tag="$2" model_id="$3" worker="$4" family="$5" artifact="$6" tokenizer="$7"
  step "TIER $tag — $model_id"

  if [ ! -x "$worker" ]; then
    record "tier $tag SKIPPED — the family worker $worker is not built. HALF OF THE ACCEPTANCE CONDITION IS THEREFORE NOT DEMONSTRATED BY THIS RUN."
    worse_than 1; return 0
  fi
  if [ -z "$artifact" ] || [ ! -f "$artifact" ]; then
    record "tier $tag SKIPPED — no checkpoint for $model_id on this host (set MISAKA_PALW_ARTIFACT_QWEN36 / MISAKA_PALW_TOKENIZER_QWEN36). HALF OF THE ACCEPTANCE CONDITION IS THEREFORE NOT DEMONSTRATED BY THIS RUN."
    worse_than 1; return 0
  fi

  # -------------------------------------------------------------------------------------------
  # Stage 2 — register the class from its artifact (ADR-0054's permissionless post-genesis route).
  # A fresh devnet registers ONE class at genesis, the BASE-0 floor, and the floor has no
  # free-prompt worker, so an outside operator's route is the one this drill takes.
  # -------------------------------------------------------------------------------------------
  step "stage 2 [$tag] — registering $model_id from its artifact"
  # **The registering node does not exit when the registration lands, and waiting for it to exit is
  # how this drill spent two hours on 2026-09-03 watching a node follow a chain.** `kaspad` is a
  # daemon: the panel marks `class_registration_done` and keeps validating. So run it in the
  # background, watch the chain-side sentence the panel prints when the object is IN a block, and
  # take the node down ourselves. A timeout here is a FAILURE with the log, never a pass.
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" --devnet --appdir="$WORK_DIR/reg-$tag" \
        --rpclisten-borsh=127.0.0.1:$((17900 + tier_index)) --nogrpc --nodnsseed --disable-upnp \
        --connect=127.0.0.1:16510 --utxoindex \
        --palw-register-class="$model_id" --palw-class-artifact="$artifact" \
        --palw-producer-key="$WORK_DIR/keys/bond-0.seed" --palw-producer-pay-address="${ADDRS[0]}" \
        --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1))" \
        >"$WORK_DIR/register-$tag.log" 2>&1 &
  local reg_pid=$!
  local reg_deadline=$((SECONDS + ${REGISTER_WAIT:-900}))
  local reg_state="timeout"
  while [ $SECONDS -lt $reg_deadline ]; do
    if ! kill -0 "$reg_pid" 2>/dev/null; then reg_state="died"; break; fi
    # The panel says this only after the object is in a block the node accepted.
    if grep -q "the class registration in tx .* is on the chain" "$WORK_DIR/register-$tag.log" 2>/dev/null; then
      reg_state="landed"; break
    fi
    # The service refusing to start at all is the failure that used to be pure silence.
    if grep -q "service not started" "$WORK_DIR/register-$tag.log" 2>/dev/null; then
      reg_state="no-service"; break
    fi
    sleep 5
  done
  kill "$reg_pid" 2>/dev/null; wait "$reg_pid" 2>/dev/null || true
  if [ "$reg_state" != "landed" ]; then
    tail -30 "$WORK_DIR/register-$tag.log" >&2
    case "$reg_state" in
      no-service)
        record "stage 2 [$tag] FAIL — the node started and NO registration service was built. The flag was read by nobody: $(grep -m1 'service not started' "$WORK_DIR/register-$tag.log" | tail -c 160)" ;;
      died)
        record "stage 2 [$tag] FAIL — the registering node exited before the class reached a block (see register-$tag.log)" ;;
      *)
        record "stage 2 [$tag] FAIL — ${REGISTER_WAIT:-900}s passed and no block carried the class registration. The node was following the chain the whole time, which is what makes this failure look like patience (see register-$tag.log)" ;;
    esac
    worse_than 1; return 0
  fi
  advance 2 || true
  local class_id
  class_id=$(grep -oE '[0-9a-f]{128}' "$WORK_DIR/register-$tag.log" | tail -1)
  if [ -z "$class_id" ]; then
    record "stage 2 [$tag] FAIL — no class id in the registration log"
    worse_than 1; return 0
  fi
  record "stage 2 [$tag] PASS — class ${class_id:0:16}… registered on chain from the artifact"

  # -------------------------------------------------------------------------------------------
  # Stage 3 — certify the family and bind the class to the FREE-PROMPT lane (ADR-0075).
  # Without it the transition refuses every commitment with `FreePromptLaneUncertified` — what 5d
  # measured, where a claim was refused not by its arithmetic but by the certified set.
  # -------------------------------------------------------------------------------------------
  step "stage 3 [$tag] — certifying the $family family on the fp lane, and binding the class"
  if ! "$CERTIFY_BIN" drill --family "$family" --lane fp --out "$WORK_DIR/obj/$tag-fp.obj" \
       >"$WORK_DIR/certify-$tag.log" 2>&1; then
    record "stage 3 [$tag] FAIL — 'palw-certify drill --family $family --lane fp' produced no evidence"
    worse_than 1; return 0
  fi
  submit_object "$WORK_DIR/obj/$tag-fp.obj" >>"$WORK_DIR/certify-$tag.log" 2>&1 || true
  if ! "$CERTIFY_BIN" bind --model-id "$model_id" --lane fp --out "$WORK_DIR/obj/$tag-bind.obj" \
       >>"$WORK_DIR/certify-$tag.log" 2>&1; then
    record "stage 3 [$tag] FAIL — 'palw-certify bind' refused $model_id"
    worse_than 1; return 0
  fi
  baseline_of "PALW lifecycle carried.*ClassLaneCertified"
  submit_object "$WORK_DIR/obj/$tag-bind.obj" >>"$WORK_DIR/certify-$tag.log" 2>&1 || true
  if all_nodes_gained "PALW lifecycle carried.*ClassLaneCertified"; then
    record "stage 3 [$tag] PASS — every validator carried FamilyCertified and ClassLaneCertified for the fp lane"
  else
    record "stage 3 [$tag] FAIL — not every validator carried the class-lane binding; every commitment will be refused as uncertified"
    worse_than 1; return 0
  fi

  # -------------------------------------------------------------------------------------------
  # Stage 4 — the gateway, under bond 0, reading the chain over node-0's RPC (ADR-0077 Decision 3).
  # It holds the bond seed so it signs its own derivations (ADR-0078 Decision 6). The identity
  # lives in its OWN directory: the gateway refuses to boot when a 32-byte file is reachable in
  # the identity directory or the outbox (ADR-0079 Decision 4), and the raw seed is 32 bytes.
  # -------------------------------------------------------------------------------------------
  step "stage 4 [$tag] — the gateway"
  local exec_pubkey operator_id port idir outbox
  exec_pubkey=$("$RAIL_BIN" --bond-key-seed "$WORK_DIR/secrets/bond-0.raw" --print-bond-pubkey \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["executor_pubkey"])')
  if [ -z "$exec_pubkey" ]; then
    record "stage 4 [$tag] FAIL — cannot read bond 0's public key from the rail"
    worse_than 1; return 0
  fi
  # The operator id is DERIVED with the preimage the chain uses (`palw_operator_id_v2`): the devnet
  # registry's pubkey for bond n is the literal bytes `misaka-devnet-operator-{n}`. A plain digest
  # here surfaces as an admission refusal rather than as a bad hash, which costs an afternoon.
  operator_id=$(python3 - <<'PY'
import hashlib, struct
pk = b"misaka-devnet-operator-0"
h = hashlib.blake2b(digest_size=64, key=b"misaka-palw/state-v2/operator-id/v1")
h.update(struct.pack("<Q", len(pk))); h.update(pk)
print(h.hexdigest())
PY
)
  idir="$WORK_DIR/identity-$tag"; outbox="$WORK_DIR/outbox-$tag"
  mkdir -p "$idir" "$outbox"
  cat >"$idir/identity.json" <<JSON
{
  "network_domain": "$NETWORK_DOMAIN",
  "class_id": "$class_id",
  "bond_txid": "$PREMINE_TXID",
  "bond_index": 0,
  "executor_pubkey": "$exec_pubkey",
  "operator_id": "$operator_id"
}
JSON
  port=$((GATEWAY_PORT + tier_index))
  MISAKA_PALW_ARTIFACT="$artifact" MISAKA_PALW_TOKENIZER="$tokenizer" \
  MISAKA_PALW_NETWORK_ID="devnet" MISAKA_PALW_MODEL_ID="$model_id" \
  "$GATEWAY_BIN" --listen "127.0.0.1:$port" --worker "$worker" \
    --outbox "$outbox" --identity "$idir/identity.json" \
    --derive-seed "$WORK_DIR/secrets/bond-0.raw" \
    --rpc 127.0.0.1:17810 >"$WORK_DIR/gateway-$tag.log" 2>&1 &
  local gw_pid=$!
  pids+=("$gw_pid")
  local health="" hdeadline=$((SECONDS + STEP_WAIT))
  until health=$(python3 -c "
import urllib.request,sys
try: print(urllib.request.urlopen('http://127.0.0.1:$port/health', timeout=3).read().decode())
except Exception: sys.exit(1)" 2>/dev/null); do
    if [ $SECONDS -ge $hdeadline ]; then
      tail -40 "$WORK_DIR/gateway-$tag.log" >&2
      record "stage 4 [$tag] FAIL — the gateway did not answer /health within ${STEP_WAIT}s: $(tail -3 "$WORK_DIR/gateway-$tag.log" | tr '\n' ' ' | head -c 200)"
      worse_than 1; return 0
    fi
    sleep 2
  done
  echo "$health" >"$WORK_DIR/health-$tag.json"
  if ! python3 - "$WORK_DIR/health-$tag.json" <<'PY'
import json, sys
chain = json.load(open(sys.argv[1])).get("chain", {})
missing = [k for k in ("registered", "fp_certified", "bond_active", "exposure_room") if k not in chain]
if missing:
    print(f"missing from /health.chain: {missing}", file=sys.stderr)
    sys.exit(1)
PY
  then
    record "stage 4 [$tag] FAIL — /health does not name the four chain-side reasons (ADR-0077 Decision 3)"
    worse_than 1; return 0
  fi
  local n_ctx fp_certified
  n_ctx=$(jget "$WORK_DIR/health-$tag.json" "n_ctx"); n_ctx=${n_ctx:-0}
  fp_certified=$(jget "$WORK_DIR/health-$tag.json" "chain.fp_certified")
  if [ "$fp_certified" != "True" ] && [ "$fp_certified" != "true" ]; then
    record "stage 4 [$tag] FAIL — the chain does not certify this class on the free-prompt lane (fp_certified=$fp_certified); no commitment can be written and no derivation can name a claim"
    worse_than 1; return 0
  fi
  record "stage 4 [$tag] PASS — /health names registered/fp_certified/bond_active/exposure_room; the registered class width is n_ctx $n_ctx"

  # -------------------------------------------------------------------------------------------
  # Stage 5 — one browser-shaped request, at the MINIMAL decode budget. It costs a single decode
  # token, and it is what tells the drill the TRUE prefill of the DSL prompt on this class, which
  # stage 7's arithmetic is a function of. Two prompts against one class get different answers.
  # -------------------------------------------------------------------------------------------
  local first_kind probe prefill claim_id job_id committed
  first_kind=$(printf '%s' "$KINDS" | awk '{print $1}')
  probe="$WORK_DIR/probe-$tag.json"
  step "stage 5 [$tag] — one request, minimal decode, to measure the prefill and prove the entrance"
  if ! chat_request "$port" "$(prompt_for "$first_kind")" 1 "" "$probe"; then
    record "stage 5 [$tag] FAIL — the gateway refused even a ONE-token job: $(tr '\n' ' ' <"$WORK_DIR/last-error.txt" | head -c 300)"
    record "  If that refusal names TooManyLeaves, the prefill of this prompt ALONE exhausts the"
    record "  executor's 2^22 leaf cap — the gate stage 7 exists to name, arriving one stage early."
    worse_than 2; return 0
  fi
  prefill=$(jget "$probe" "usage.prompt_tokens")
  claim_id=$(jget "$probe" "misaka.fp_claim_id")
  job_id=$(jget "$probe" "misaka.fp_job_id")
  committed=$(jget "$probe" "misaka.committed")
  if [ "$committed" != "True" ] && [ "$committed" != "true" ]; then
    record "stage 5 [$tag] FAIL — the gateway answered but wrote no commitment: $(jget "$probe" "misaka.not_committed_because")"
    worse_than 1; return 0
  fi
  record "stage 5 [$tag] PASS — job ${job_id:0:16}… answered and committed; claim ${claim_id:0:16}…; this prompt's prefill is $prefill positions"

  # -------------------------------------------------------------------------------------------
  # Stage 6 — that claim through the lattice on EVERY validator, and a receipt block.
  #
  # The failure mode this stage exists to NAME: testnet-11 5e shipped with `final_claims` at 0 for
  # a week because every seat filed `Incapable` — a chain producing blocks the whole time and
  # certifying nothing. A drill that waited for `Final` and timed out would report that as "slow".
  # -------------------------------------------------------------------------------------------
  step "stage 6 [$tag] — the claim through the lattice, on every validator"
  local short="${claim_id:0:16}" reached="" stage6=1 phase incapable unavail
  [ -n "$short" ] || short="${job_id:0:16}"
  for phase in FreePromptCommitted PanelBound ReceiptLicensed Final; do
    if all_nodes_logged "$phase.*${short}|${short}.*$phase"; then
      reached="$phase"
    else
      stage6=0
      incapable=$(grep -ho 'Incapable' "$WORK_DIR"/node-*.log 2>/dev/null | wc -l | tr -d ' ')
      unavail=$(grep -ho 'Unavailable' "$WORK_DIR"/node-*.log 2>/dev/null | wc -l | tr -d ' ')
      record "stage 6 [$tag] FAIL — the claim reached ${reached:-nothing} and not $phase on every validator (Incapable×$incapable Unavailable×$unavail)"
      [ "${incapable:-0}" -gt 0 ] && record "  DIAGNOSIS: seats filed Incapable — the panel cannot execute this class (5e's stall, not slowness)"
      [ "${unavail:-0}" -gt 0 ] && record "  DIAGNOSIS: seats filed Unavailable — the material or an interval opening did not reach them"
      break
    fi
  done
  if [ "$stage6" = 0 ]; then
    worse_than 2
    record "  stages 7-11 [$tag] NOT RUN — a derivation names a claim, and this claim did not reach Final"
    return 0
  fi
  if all_nodes_logged "receipt block|algo 7|POW_ALGO_ID_PALW_RECEIPT"; then
    record "stage 6 [$tag] PASS — the claim reached Final on every validator and a receipt block was accepted by all"
  else
    record "stage 6 [$tag] PARTIAL — the claim reached Final on every validator; no receipt block within ${STEP_WAIT}s"
    worse_than 1
  fi

  # -------------------------------------------------------------------------------------------
  # Stages 7-10, per kind. BOTH kinds run even when the first one gates: the launch owner needs
  # to know whether the answer is the same for MIDI data and for 3D data.
  # -------------------------------------------------------------------------------------------
  local kind
  for kind in $KINDS; do
    run_kind "$tag" "$port" "$kind" "$class_id" "$n_ctx" "$prefill"
  done

  # -------------------------------------------------------------------------------------------
  # Stage 11 — "plus free prompts with long, practical output".
  # -------------------------------------------------------------------------------------------
  step "stage 11 [$tag] — a free prompt with long, practical output"
  local long_out="$WORK_DIR/long-$tag.json" long_prompt emitted chars
  long_prompt='Explain, for an operator who has never run one, what a proof-of-work node does between receiving a block and accepting it. Use short paragraphs and be concrete.'
  if chat_request "$port" "$long_prompt" "$LONG_PROMPT_TOKENS" "" "$long_out"; then
    emitted=$(jget "$long_out" "usage.completion_tokens")
    chars=$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))["choices"][0]["message"]["content"]))' "$long_out")
    if [ "${emitted:-0}" -ge 100 ]; then
      record "stage 11 [$tag] PASS — $emitted decode tokens, $chars characters of practical answer, committed on chain"
    else
      record "stage 11 [$tag] FAIL — the class emitted only ${emitted:-0} decode tokens ($chars chars) against a request for $LONG_PROMPT_TOKENS; 'long, practical output' is not demonstrated. Same ceiling stage 7 names."
      worse_than 2
    fi
  else
    record "stage 11 [$tag] FAIL — a $LONG_PROMPT_TOKENS-token request was refused: $(tr '\n' ' ' <"$WORK_DIR/last-error.txt" | head -c 300)"
    worse_than 2
  fi
}

# ---------------------------------------------------------------------------------------------
# Both tiers.
# ---------------------------------------------------------------------------------------------
run_tier 0 "dense"  "$DENSE_MODEL_ID"  "$DENSE_WORKER"  "a16"    "$MISAKA_PALW_ARTIFACT" "$MISAKA_PALW_TOKENIZER"
run_tier 1 "qwen36" "$QWEN36_MODEL_ID" "$QWEN36_WORKER" "qwen36" "${MISAKA_PALW_ARTIFACT_QWEN36:-}" "${MISAKA_PALW_TOKENIZER_QWEN36:-}"

# ---------------------------------------------------------------------------------------------
# The verdict. Every stage says what it proved or why it could not; the exit code is the worst.
# ---------------------------------------------------------------------------------------------
step "VERDICT"
cat "$VERDICT_FILE" >&2
echo >&2
if [ "$WORST" -eq 0 ]; then
  log "PASS — the acceptance condition is demonstrated on a running chain: both tiers, both kinds,"
  log "       the derivation in a block, the artifact never shared, and a stranger recomputing it"
  log "       with a code path that is not the producer's."
  exit 0
fi
log "NOT DEMONSTRATED — the evidence is in $WORK_DIR (node-*.log, gateway-*.log, derived/, chain/)."
log "Read the stage-7 lines first. If they name the executor leaf cap, that is the one thing"
log "standing between this tree and the announcement — and it is not n_ctx, and not the model."
exit "$WORST"
