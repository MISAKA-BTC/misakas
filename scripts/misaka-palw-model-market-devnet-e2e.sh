#!/usr/bin/env bash
# misaka-palw-model-market-devnet-e2e.sh — the devnet drill ADR-0087 §6 item 4, ADR-0088 §7 item 8
# and ADR-0089 §8 item 6 ask for, on one bonded PALW devnet with the three model fences ARMED FROM
# GENESIS (`--palw-model-devnet=0`, private devnets only) on the FLOOR-ONLY ruleset
# (`--palw-devnet-floor-only`: the base class alone at ADR-0076's MAX/278, ~14 s a block per producer;
# the shipped devnet's testnet-11 class set prices a fixture block at minutes per producer):
#
#   node-0 … node-(N-1)   each a producer under devnet public-seed bond n (floor class, fixture PoW),
#                         every node serving wRPC Borsh (176xx) and the eth JSON-RPC (185xx).
#   1. the founding line (ADR-0088 D1: its id is the base class id) reads through getPalwModelLine
#   1b. ADR-0090: a buy before any seed is refused; an EVM account funded through EVM_DEPOSIT_LOCK +
#      claim seeds the line with the least seed (100,000 MSK) through the writer — the market opens
#      at 0.2 MSK a position with 500,000 whole positions in the curve; a second (carrier) seed is refused
#   2. a CARRIER buy (ADR-0087): `misaka palw model-buy` from the premine key → every node folds it
#   3. (the EVM account was funded in 1b)
#   4. an EVM buy (ADR-0089 D5/D6): `misaka palw model-evm-buy` → queued in B, settled in C, the
#      position readable through the position window on every node
#   5. a REFUSED EVM buy (min positions impossible) → the escrow comes back in C
#   6. an EVM sell → the net leg is credited in C
#   7. every node agrees on the market row, the EVM balance and the EVM position
#   8. a PARTITION: side A = {0,1} takes another EVM buy through B and C while side B = {2..N-1}
#      outweighs it; the HEAL must reorg side A and leave every node on one row and one balance
#
# Env: KASPAD_BIN, CLI_BIN, PQV_BIN (defaults target/release/*), NODES (5, at least 4), WORK_DIR,
#      WAIT (s, first blocks), STEP_WAIT (s, any one poll), FENCE_DAA (0), LINE_ID (override the
#      founding line id if the node's log does not name it), EXTRA_NODE_ARGS.
set -euo pipefail
# NOTE: under `-e -o pipefail`, a `grep` that matches nothing inside a `$(…)` assignment ends the script
# WITHOUT a word (the EXIT trap then stops the nodes) — run 5 died exactly so. Guard every such
# substitution with `|| true` and let the `[ -n … ] || die` after it speak.
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
CLI_BIN="${CLI_BIN:-$REPO_ROOT/target/release/misaka}"
PQV_BIN="${PQV_BIN:-$REPO_ROOT/target/release/kaspa-pq-validator}"
NODES="${NODES:-5}"
WORK_DIR="${WORK_DIR:-$REPO_ROOT/.misaka-palw-model-market-devnet}"
WAIT="${WAIT:-600}"
STEP_WAIT="${STEP_WAIT:-420}"
FENCE_DAA="${FENCE_DAA:-0}"
PREMINE_TXID="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"   # "misaka-premine" zero-padded to 64 bytes
MAIN_PREMINE_INDEX=40   # consensus/core/src/config/premine.rs; bond n's fee float sits at MAIN_PREMINE_INDEX + 1 + n
P2P_BASE=16410; RPC_BASE=17710; EVM_BASE=18545   # clear of the certify (163xx/176xx) and VLT devnets

log() { printf '[market-e2e %s] %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die() { log "FATAL: $*"; exit 1; }
[ "$NODES" -ge 4 ] || die "NODES must be at least 4 (two on side A, two or more on side B)"
[ "$NODES" -le 6 ] || die "devnet seats six public-seed bonds; NODES must be at most 6"
for b in "$KASPAD_BIN" "$CLI_BIN" "$PQV_BIN"; do [ -x "$b" ] || die "missing binary $b"; done

rm -rf "$WORK_DIR"; mkdir -p "$WORK_DIR/keys" "$WORK_DIR/out"
python3 - "$WORK_DIR/keys" "$NODES" <<'PY'
import hashlib, os, sys
d, n = sys.argv[1], int(sys.argv[2])
h = lambda b: hashlib.blake2b(b, digest_size=32).hexdigest()
for i in range(n):
    p = f"{d}/bond-{i}.seed"; open(p, "w").write(h(b"misaka-devnet-genesis-bond-v1/" + str(i).encode())); os.chmod(p, 0o600)
p = f"{d}/main.seed"; open(p, "w").write(h(b"misaka-testnet-premine-9b-claude-managed")); os.chmod(p, 0o600)
PY

p2p_of() { echo $((P2P_BASE + $1)); }
rpc_of() { echo $((RPC_BASE + $1)); }
evm_of() { echo $((EVM_BASE + $1)); }
cli() { local i="$1"; shift; "$CLI_BIN" --network devnet --rpc "127.0.0.1:$(rpc_of "$i")" --evm-rpc "http://127.0.0.1:$(evm_of "$i")" "$@"; }

# Start (or restart) node `i` dialling only the nodes in `peers_csv` (may be empty: listen only).
# `--connect` is "these and nobody else" — the partition is made of dial lists, not firewalls, and a
# restart drops whatever links existed; a side's members dial each other so every producer keeps
# the peer its mining gate demands.
start_node() {
  local i="$1" peers="${2:-}"
  local addr
  addr="$("$CLI_BIN" --network devnet key address --key-file "$WORK_DIR/keys/bond-$i.seed" | tail -1 | awk '{print $NF}')"
  [ -n "$addr" ] || die "cannot derive bond $i's address"
  local args=(--devnet --appdir="$WORK_DIR/node-$i" --listen="127.0.0.1:$(p2p_of "$i")" --rpclisten-borsh="127.0.0.1:$(rpc_of "$i")"
        --evm-rpc-listen="127.0.0.1:$(evm_of "$i")" --utxoindex --nodnsseed --disable-upnp --nogrpc --enable-unsynced-mining
        --palw-model-devnet="$FENCE_DAA" --palw-devnet-floor-only --evm-bridge-devnet-unpaused
        --palw-produce --palw-producer-key="$WORK_DIR/keys/bond-$i.seed" --palw-producer-bond="$PREMINE_TXID:$i" --palw-producer-pay-address="$addr"
        --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + i))")
  if [ -n "${EXTRA_NODE_ARGS:-}" ]; then read -r -a extra <<<"$EXTRA_NODE_ARGS"; args+=("${extra[@]}"); fi
  local j
  for j in $(echo "$peers" | tr ',' ' '); do args+=("--connect=127.0.0.1:$(p2p_of "$j")"); done
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >>"$WORK_DIR/node-$i.log" 2>&1 &
  echo $! > "$WORK_DIR/node-$i.pid"
  log "node-$i pid $(cat "$WORK_DIR/node-$i.pid") peers {${peers:-listen only}} bond $PREMINE_TXID:$i"
}
stop_node() {
  local i="$1" pid
  pid="$(cat "$WORK_DIR/node-$i.pid" 2>/dev/null || true)"
  [ -n "$pid" ] || return 0
  kill "$pid" 2>/dev/null || true
  local d=$((SECONDS + 60)); while kill -0 "$pid" 2>/dev/null && [ $SECONDS -lt $d ]; do sleep 1; done
  kill -9 "$pid" 2>/dev/null || true
  rm -f "$WORK_DIR/node-$i.pid"
}
cleanup() { local i; for ((i=0; i<NODES; i++)); do stop_node "$i"; done; }
trap cleanup EXIT

blocks_of() { grep -c "produced block #" "$WORK_DIR/node-$1.log" 2>/dev/null || true; }
# Blocks produced by the nodes in `csv` (default: every node) — the chain's progress as the drill
# sees it. One producer's own count is a poor clock (a floor draw is ~14 s a block per producer,
# and the network draw against bits thins it further); the side's total is what the chain grew by.
blocks_total() {
  local csv="${1:-$(seq -s, 0 $((NODES - 1)))}" i t=0
  for i in $(echo "$csv" | tr ',' ' '); do t=$((t + $(blocks_of "$i"))); done
  echo "$t"
}

# Wait for the nodes in `csv` (default: all) to produce `want` more blocks than they had on entry;
# return after the deadline rather than dying — the verdict decides, with the evidence in the logs.
advance() {
  local want="${1:-1}" csv="${2:-}" from now
  from=$(blocks_total "$csv")
  local deadline=$((SECONDS + STEP_WAIT))
  while :; do
    now=$(blocks_total "$csv")
    [ $((now - from)) -ge "$want" ] && return 0
    [ $SECONDS -lt $deadline ] || { log "the chain gained $((now - from))/$want block(s) in ${STEP_WAIT}s — continuing"; return 0; }
    sleep 2
  done
}
# Poll `cmd…` until its stdout satisfies python expression `$COND` over `v` (the parsed JSON), or the
# deadline passes (then return 1). The last output is kept in $WORK_DIR/out/last.json.
until_json() {
  local cond="$1"; shift
  local deadline=$((SECONDS + STEP_WAIT))
  while :; do
    if "$@" --output json > "$WORK_DIR/out/last.json" 2>>"$WORK_DIR/out/cli.err"; then
      if python3 - "$cond" "$WORK_DIR/out/last.json" <<'PY' 2>/dev/null
import json, sys
cond, path = sys.argv[1], sys.argv[2]
try:
    v = json.load(open(path))
except Exception:
    sys.exit(1)
def find(o, key):
    # the first value under a key named `key` anywhere in the document
    if isinstance(o, dict):
        for k, x in o.items():
            if k == key: return x
        for x in o.values():
            r = find(x, key)
            if r is not None: return r
    elif isinstance(o, list):
        for x in o:
            r = find(x, key)
            if r is not None: return r
    return None
sys.exit(0 if eval(cond) else 1)
PY
      then return 0; fi
    fi
    [ $SECONDS -lt $deadline ] || return 1
    sleep 3
  done
}
jfind() { python3 - "$1" "$2" <<'PY'
import json, sys
key, path = sys.argv[1], sys.argv[2]
def find(o, key):
    if isinstance(o, dict):
        for k, x in o.items():
            if k == key: return x
        for x in o.values():
            r = find(x, key)
            if r is not None: return r
    elif isinstance(o, list):
        for x in o:
            r = find(x, key)
            if r is not None: return r
    return None
v = find(json.load(open(path)), key)
print("" if v is None else v)
PY
}
# One node's market row (reserve, sold), EVM balance and EVM position, as one line.
row_of() {
  local i="$1"
  local reserve sold bal pos
  cli "$i" palw model-show "$LINE_ID" --output json > "$WORK_DIR/out/row-$i.json" 2>/dev/null || echo '{}' > "$WORK_DIR/out/row-$i.json"
  reserve="$(jfind msk_reserve_sompi "$WORK_DIR/out/row-$i.json")"; sold="$(jfind sold_units "$WORK_DIR/out/row-$i.json")"
  cli "$i" evm balance --address "$EVM_ADDR" --output json > "$WORK_DIR/out/bal-$i.json" 2>/dev/null || echo '{}' > "$WORK_DIR/out/bal-$i.json"
  bal="$(jfind balanceWei "$WORK_DIR/out/bal-$i.json")"
  cli "$i" palw model-evm-position --line "$LINE_ID" --address "$EVM_ADDR" --output json > "$WORK_DIR/out/pos-$i.json" 2>/dev/null || echo '{}' > "$WORK_DIR/out/pos-$i.json"
  pos="$(jfind units "$WORK_DIR/out/pos-$i.json")"
  echo "reserve=${reserve:-?} sold=${sold:-?} balanceWei=${bal:-?} positionUnits=${pos:-?}"
}
# Every node must print the same row; the daa of each is shown for the reader.
all_nodes_agree() {
  local label="$1" i first row fail=0
  first="$(row_of 0)"
  for ((i=0; i<NODES; i++)); do
    row="$(row_of "$i")"
    log "  [$label] node-$i: $row"
    [ "$row" = "$first" ] || fail=1
  done
  [ "$fail" -eq 0 ] && { log "  [$label] every node agrees"; return 0; }
  log "  [$label] DISAGREEMENT"; return 1
}
# Wait until every node's row equals node-0's (a heal converges in a few blocks).
until_all_agree() {
  local label="$1" deadline=$((SECONDS + STEP_WAIT))
  while :; do
    if all_nodes_agree "$label" 2>/dev/null; then all_nodes_agree "$label"; return 0; fi
    [ $SECONDS -lt $deadline ] || { all_nodes_agree "$label" || true; return 1; }
    sleep 5
  done
}

# A restarted node answers its wRPC and its eth JSON-RPC only seconds after the process starts;
# poll both before the next step uses them (run 7 sent the partition's buy into a refused connect).
wait_rpc() {
  local i="$1" deadline=$((SECONDS + 120))
  while :; do
    if cli "$i" palw model-show "$LINE_ID" --output json >/dev/null 2>&1 && cli "$i" evm balance --address "${EVM_ADDR:-0x0000000000000000000000000000000000000001}" --output json >/dev/null 2>&1; then return 0; fi
    [ $SECONDS -lt $deadline ] || { log "node-$i's RPCs did not come up within 120s"; return 1; }
    sleep 2
  done
}

verdict=()
check() { # label ok(0/1)
  if [ "$2" -eq 0 ]; then verdict+=("PASS  $1"); log "PASS: $1"; else verdict+=("FAIL  $1"); log "FAIL: $1"; fi
}

# ---- 0. the mesh: node-0 listens, the rest dial it -----------------------------------------------
start_node 0 ""
for ((i=1; i<NODES; i++)); do start_node "$i" "0"; done
deadline=$((SECONDS + WAIT))
until [ "$(blocks_total)" -ge 3 ]; do
  [ $SECONDS -lt $deadline ] || { tail -30 "$WORK_DIR/node-0.log" >&2; die "the chain produced fewer than 3 blocks within ${WAIT}s"; }
  sleep 3
done
log "the chain has $(blocks_total) blocks ($(for ((i=0; i<NODES; i++)); do printf 'node-%s:%s ' "$i" "$(blocks_of "$i")"; done))"
advance 1
grep -q "palw-model-devnet\|PALW base class" "$WORK_DIR/node-0.log" || true

# ---- 1. the founding line --------------------------------------------------------------------------
if [ -z "${LINE_ID:-}" ]; then
  LINE_ID="$( { grep -o -E "PALW base class \(and its founding model line\): [0-9a-f]{128}" "$WORK_DIR/node-0.log" || true; } | head -1 | awk '{print $NF}')"
fi
[ -n "${LINE_ID:-}" ] || die "the node's log does not name the base class; pass LINE_ID=<128 hex> (the base class id)"
log "founding line (= base class) $LINE_ID"
if until_json "find(v,'exists') in (True, 'true') or find(v,'line') is not None" cli 0 palw line-show "$LINE_ID"; then check "1. the founding line reads through getPalwModelLine" 0; else check "1. the founding line reads through getPalwModelLine" 1; fi
cp "$WORK_DIR/out/last.json" "$WORK_DIR/out/line-show.json" 2>/dev/null || true

# ---- 3. an EVM account, funded through a deposit lock + claim ------------------------------------------
cli 0 evm wallet create --out "$WORK_DIR/keys/evm.mnemonic" --output json > "$WORK_DIR/out/evm-wallet.json"
EVM_ADDR="$(jfind address "$WORK_DIR/out/evm-wallet.json")"
[ -n "$EVM_ADDR" ] || die "no EVM address from wallet create"
log "EVM account $EVM_ADDR"
EVM_KEY=(--mnemonic-file "$WORK_DIR/keys/evm.mnemonic")
"$PQV_BIN" deposit-lock --node-wrpc-borsh "127.0.0.1:$(rpc_of 0)" --network devnet --validator-key "$WORK_DIR/keys/main.seed" \
  --evm-address "$EVM_ADDR" --amount 10003000000000 --claim-tip 0 > "$WORK_DIR/out/deposit-lock.txt" 2>&1 || { cat "$WORK_DIR/out/deposit-lock.txt" >&2; die "deposit-lock failed"; }
OUTPOINT="$( { grep -o -E "deposit_lock_outpoint: [0-9a-f]{128}:[0-9]+" "$WORK_DIR/out/deposit-lock.txt" || true; } | awk '{print $2}')"
[ -n "$OUTPOINT" ] || { cat "$WORK_DIR/out/deposit-lock.txt" >&2; die "deposit-lock printed no outpoint"; }
log "deposit lock $OUTPOINT (100,030 MSK: the seed and change); waiting for it to be mined, then claiming on node-0"
advance 2
deadline=$((SECONDS + STEP_WAIT))
until "$PQV_BIN" claim --node-wrpc-borsh "127.0.0.1:$(rpc_of 0)" --network devnet --outpoint "$OUTPOINT" > "$WORK_DIR/out/claim.txt" 2>&1; do
  [ $SECONDS -lt $deadline ] || { cat "$WORK_DIR/out/claim.txt" >&2; die "the deposit claim was never accepted"; }
  sleep 5
done
if until_json "int(find(v,'balanceWei') or 0) > 0" cli 0 evm balance --address "$EVM_ADDR"; then check "3. the EVM account is funded through EVM_DEPOSIT_LOCK + claim" 0; else check "3. the EVM account is funded through EVM_DEPOSIT_LOCK + claim" 1; fi
bal0="$(jfind balanceWei "$WORK_DIR/out/last.json")"; log "  balance $bal0 wei"

# ---- 1b. ADR-0090: no market before a seed; the EVM seed opens it; a second seed is refused ------------
cli 0 palw model-show "$LINE_ID" --output json > "$WORK_DIR/out/market-unseeded.json" 2>/dev/null || echo '{}' > "$WORK_DIR/out/market-unseeded.json"
opened0="$(jfind opened "$WORK_DIR/out/market-unseeded.json")"
if [ "$opened0" = "False" ] || [ "$opened0" = "false" ]; then check "1b. before any seed the line has no market" 0; else check "1b. before any seed the line has no market" 1; fi
log "EVM seed: 100,000 MSK through the writer (locked for good)"
cli 0 palw model-evm-seed "${EVM_KEY[@]}" --line "$LINE_ID" --msk 100000 --yes --wait > "$WORK_DIR/out/evm-seed.txt" 2>&1 || log "model-evm-seed exited non-zero: $(tail -3 "$WORK_DIR/out/evm-seed.txt")"
if until_json "int(find(v,'seed_sompi') or 0) == 10000000000000 and int(find(v,'msk_reserve_sompi') or 0) == 10000000000000" cli 0 palw model-show "$LINE_ID"; then check "1c. the EVM seed opened the market with the whole 100,000 MSK as the reserve" 0; else check "1c. the EVM seed opened the market with the whole 100,000 MSK as the reserve" 1; fi
price0="$(jfind price_sompi_per_position "$WORK_DIR/out/last.json")"; log "  first price ${price0:-?} sompi a position (0.2 MSK expected); positions in the curve: $(jfind position_units "$WORK_DIR/out/last.json")"
[ "${price0:-0}" = "20000000" ] && check "1d. the first price is seed / 500,000 = 0.2 MSK" 0 || check "1d. the first price is seed / 500,000 = 0.2 MSK" 1
cli 0 palw model-evm-position --line "$LINE_ID" --address "$EVM_ADDR" --output json > "$WORK_DIR/out/pos-seed.json" 2>/dev/null || echo '{}' > "$WORK_DIR/out/pos-seed.json"
[ "$(jfind units "$WORK_DIR/out/pos-seed.json")" = "0" ] && check "1e. the seeder holds no position" 0 || check "1e. the seeder holds no position" 1
cli 0 evm balance --address "$EVM_ADDR" --output json > "$WORK_DIR/out/bal-seed.json" 2>/dev/null || echo '{}' > "$WORK_DIR/out/bal-seed.json"
bal_seed="$(jfind balanceWei "$WORK_DIR/out/bal-seed.json")"; log "  balance after the seed ${bal_seed:-?} wei"
log "carrier seed on the seeded line: refused (a market is seeded once)"
if cli 0 palw model-seed --key-file "$WORK_DIR/keys/main.seed" --line "$LINE_ID" --msk 100000 --yes > "$WORK_DIR/out/carrier-seed.txt" 2>&1; then check "1f. a second seed is refused" 1; else check "1f. a second seed is refused" 0; fi

# ---- 2. the carrier buy (ADR-0087) -------------------------------------------------------------------
log "carrier buy: 5 MSK from the premine key"
cli 0 palw model-buy --key-file "$WORK_DIR/keys/main.seed" --line "$LINE_ID" --msk 5 --min-positions 1 --yes >"$WORK_DIR/out/carrier-buy.txt" 2>&1 || log "model-buy exited non-zero: $(tail -2 "$WORK_DIR/out/carrier-buy.txt")"
if until_json "int(find(v,'sold_units') or 0) > 0" cli 0 palw model-show "$LINE_ID"; then check "2. the carrier buy folded (sold_units > 0)" 0; else check "2. the carrier buy folded (sold_units > 0)" 1; fi
carrier_sold="$(jfind sold_units "$WORK_DIR/out/last.json")"; price1="$(jfind price_sompi_per_position "$WORK_DIR/out/last.json")"; log "  sold_units after the carrier buy: ${carrier_sold:-?} (whole positions); price ${price1:-?}"
if [ "${price1:-0}" -gt "${price0:-0}" ]; then check "2b. buying raised the price (${price0:-?} → ${price1:-?})" 0; else check "2b. buying raised the price" 1; fi


# ---- 4. the EVM buy (ADR-0089 Decisions 5 and 6) ------------------------------------------------------
log "EVM buy: 3 MSK through the writer (whole positions at ~0.2 MSK)"
cli 0 palw model-evm-buy "${EVM_KEY[@]}" --line "$LINE_ID" --msk 3 --min-positions 1 --yes --wait > "$WORK_DIR/out/evm-buy.txt" 2>&1 || log "model-evm-buy exited non-zero: $(tail -3 "$WORK_DIR/out/evm-buy.txt")"
if until_json "int(find(v,'units') or 0) > 0" cli 0 palw model-evm-position --line "$LINE_ID" --address "$EVM_ADDR"; then check "4. the EVM buy settled into a position the window shows" 0; else check "4. the EVM buy settled into a position the window shows" 1; fi
pos1="$(jfind units "$WORK_DIR/out/last.json")"; log "  position $pos1 (whole positions; 1 unit = 1 position under ADR-0090)"
until_json "int(find(v,'sold_units') or 0) > int('${carrier_sold:-0}' or 0)" cli 0 palw model-show "$LINE_ID" || true
cli 0 evm balance --address "$EVM_ADDR" --output json > "$WORK_DIR/out/bal1.json"; bal1="$(jfind balanceWei "$WORK_DIR/out/bal1.json")"; log "  balance $bal1 wei"

# ---- 5. a refused EVM buy: the escrow comes back -----------------------------------------------------
log "refused EVM buy: 1 MSK asking for a billion positions"
cli 0 palw model-evm-buy "${EVM_KEY[@]}" --line "$LINE_ID" --msk 1 --min-positions 1000000000 --yes --wait > "$WORK_DIR/out/evm-buy-refused.txt" 2>&1 || log "model-evm-buy (refused) exited non-zero: $(tail -3 "$WORK_DIR/out/evm-buy-refused.txt")"
# after B the balance is bal1 − 1 MSK − gas; after C's refund it is bal1 − gas (gas ≪ 0.5 MSK)
if until_json "int(find(v,'balanceWei') or 0) > int('${bal1:-0}' or 0) - 500000000000000000" cli 0 evm balance --address "$EVM_ADDR"; then check "5. the refused buy's escrow came back in the settling block" 0; else check "5. the refused buy's escrow came back in the settling block" 1; fi
bal2="$(jfind balanceWei "$WORK_DIR/out/last.json")"; log "  balance $bal2 wei"
cli 0 palw model-evm-position --line "$LINE_ID" --address "$EVM_ADDR" --output json > "$WORK_DIR/out/pos2.json"; pos2="$(jfind units "$WORK_DIR/out/pos2.json")"
[ "${pos2:-}" = "${pos1:-x}" ] && check "5b. the refused buy changed no position" 0 || check "5b. the refused buy changed no position" 1

# ---- 6. an EVM sell: the net leg is credited ---------------------------------------------------------
log "EVM sell: 1 position"
cli 0 palw model-evm-sell "${EVM_KEY[@]}" --line "$LINE_ID" --positions 1 --min-msk 0 --yes --wait > "$WORK_DIR/out/evm-sell.txt" 2>&1 || log "model-evm-sell exited non-zero: $(tail -3 "$WORK_DIR/out/evm-sell.txt")"
if until_json "int(find(v,'units') or 0) == int('${pos1:-0}' or 0) - 1" cli 0 palw model-evm-position --line "$LINE_ID" --address "$EVM_ADDR"; then check "6. the EVM sell debited exactly one whole position" 0; else check "6. the EVM sell debited exactly one whole position" 1; fi
if until_json "int(find(v,'balanceWei') or 0) > int('${bal2:-0}' or 0)" cli 0 evm balance --address "$EVM_ADDR"; then check "6b. the sell's net leg was credited to the account" 0; else check "6b. the sell's net leg was credited to the account" 1; fi
bal3="$(jfind balanceWei "$WORK_DIR/out/last.json")"; log "  balance $bal3 wei"

# ---- 7. every node agrees --------------------------------------------------------------------------------
advance 2
if until_all_agree "before the partition"; then check "7. every node holds the same row, balance and position" 0; else check "7. every node holds the same row, balance and position" 1; fi

# ---- 8. the partition: side A = {0,1}, side B = {2..N-1} ----------------------------------------------------
sideA="0,1"; sideB="$(seq -s, 2 $((NODES - 1)) | sed 's/,$//')"
log "partition: side A {$sideA} | side B {$sideB}"
for ((i=0; i<NODES; i++)); do stop_node "$i"; done
sleep 3
start_node 0 "1"; start_node 1 "0"
start_node 2 "$(seq -s, 3 $((NODES - 1)) | sed 's/,$//')"
for ((i=3; i<NODES; i++)); do start_node "$i" "2"; done
sleep 5; wait_rpc 0 || true; wait_rpc 2 || true
a_from=$(blocks_total "$sideA"); b_from=$(blocks_total "$sideB")
cli 0 palw model-evm-position --line "$LINE_ID" --address "$EVM_ADDR" --output json > "$WORK_DIR/out/pos-pre-partition.json" 2>/dev/null || echo '{}' > "$WORK_DIR/out/pos-pre-partition.json"
pre_units="$(jfind units "$WORK_DIR/out/pos-pre-partition.json")"; pre_units="${pre_units:-0}"
log "side A: an EVM buy of 2 MSK, to be queued in B and settled in C on the minority side (position before: $pre_units units)"
cli 0 palw model-evm-buy "${EVM_KEY[@]}" --line "$LINE_ID" --msk 2 --min-positions 1 --yes --wait > "$WORK_DIR/out/evm-buy-partition.txt" 2>&1 || log "model-evm-buy (partition) exited non-zero: $(tail -3 "$WORK_DIR/out/evm-buy-partition.txt")"
if until_json "int(find(v,'units') or 0) > int('${pre_units}' or 0)" cli 0 palw model-evm-position --line "$LINE_ID" --address "$EVM_ADDR"; then log "  side A settled the buy: $(jfind units "$WORK_DIR/out/last.json") units"; check "8a. the minority side queued and settled the EVM buy across its own B and C" 0; else log "  side A did not show the settled buy within ${STEP_WAIT}s"; check "8a. the minority side queued and settled the EVM buy across its own B and C" 1; fi
sideA_row="$(row_of 0)"; log "  side A row: $sideA_row"
sideB_row="$(row_of 2)"; log "  side B row: $sideB_row"
# let side B outweigh side A by a clear margin before healing
deadline=$((SECONDS + STEP_WAIT))
while [ $(( $(blocks_total "$sideB") - b_from )) -lt $(( $(blocks_total "$sideA") - a_from + 6 )) ]; do
  [ $SECONDS -lt $deadline ] || { log "side B did not outgrow side A by 6 blocks in ${STEP_WAIT}s (A +$(( $(blocks_total "$sideA") - a_from )), B +$(( $(blocks_total "$sideB") - b_from ))) — healing anyway"; break; }
  sleep 5
done
log "heal: A +$(( $(blocks_total "$sideA") - a_from )) blocks, B +$(( $(blocks_total "$sideB") - b_from )) blocks — restoring the mesh"
for ((i=0; i<NODES; i++)); do stop_node "$i"; done
sleep 3
start_node 0 ""
for ((i=1; i<NODES; i++)); do start_node "$i" "0"; done
sleep 5; for ((i=0; i<NODES; i++)); do wait_rpc "$i" || true; done
advance 3
if until_all_agree "after the heal"; then check "8. the heal converged every node on one row, balance and position" 0; else check "8. the heal converged every node on one row, balance and position" 1; fi
healed_row="$(row_of 0)"
if [ "$healed_row" = "$sideB_row" ]; then log "  the majority (side B) row won: the minority's B/C were dropped"; elif [ "$healed_row" = "$sideA_row" ]; then log "  the minority's row survived (the action was re-included on the winning chain)"; else log "  the healed row is neither side's pre-heal row (a re-included action settled later): $healed_row"; fi

# ---- 9. no node computed a different state or hit a settlement mismatch ----------------------------------
bad=0
for ((i=0; i<NODES; i++)); do
  n=$(grep -c -E "MarketSettlementMismatch|panicked|market settlement|lifecycle object was dropped.*Model" "$WORK_DIR/node-$i.log" || true)
  [ "$n" -eq 0 ] || { bad=1; log "node-$i logged $n suspicious line(s):"; grep -E "MarketSettlementMismatch|panicked|market settlement|lifecycle object was dropped.*Model" "$WORK_DIR/node-$i.log" | head -5 | sed "s/^/    /" >&2; }
done
check "9. no node logged a settlement mismatch, a dropped market object or a panic" "$bad"

log "==== verdict ===="
fail=0
for l in "${verdict[@]}"; do log "$l"; case "$l" in FAIL*) fail=1;; esac; done
[ "$fail" -eq 0 ] && log "PASS: the model market drill held on both lanes and across the partition" || die "the drill failed — see $WORK_DIR (node logs, out/)"
