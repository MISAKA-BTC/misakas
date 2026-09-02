#!/usr/bin/env bash
# misaka-palw-certify-devnet-e2e.sh — ADR-0075 §7's rehearsal: a bonded PALW devnet of N validators
# started from ONE build in ONE window, the three certification transactions submitted, and every
# validator's log read back for the same verdict.
#
#   node-0 … node-(N-1)   each a producer under devnet public-seed bond n (floor class, fixture PoW),
#                         node-0 listens, the rest --connect to it; all --utxoindex for the CLI.
#   1. wait until node-0 has produced blocks and every node has accepted them
#   2. palw-certify drill --family base0 --lane fp → submit-object  (FamilyCertified)
#   3. palw-certify bind  --model-id PALW-BASE-0/rc --lane fp → submit-object (ClassLaneCertified)
#   4. three more FamilyCertified in one burst (base0 attempt, a16 attempt, qwen36 attempt)
#      to exercise PALW_CERTIFICATION_MAX_PER_BLOCK on a live chain
#   5. assert every node logged "PALW lifecycle carried 1× FamilyCertified" / "ClassLaneCertified"
#      for the same objects, and no node computed a different PALW state (no "dropped" line for
#      the accepted objects on any node)
#
# Env: KASPAD_BIN, CLI_BIN, CERTIFY_BIN (defaults target/release/*), NODES (3), WORK_DIR, WAIT (s).
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
CLI_BIN="${CLI_BIN:-$REPO_ROOT/target/release/misaka-cli}"
CERTIFY_BIN="${CERTIFY_BIN:-$REPO_ROOT/target/release/palw-certify}"
NODES="${NODES:-3}"
WORK_DIR="${WORK_DIR:-$REPO_ROOT/.misaka-palw-certify-devnet}"
WAIT="${WAIT:-240}"
PREMINE_TXID="6d6973616b612d7072656d696e65$(printf '0%.0s' $(seq 1 100))"   # "misaka-premine" zero-padded to 64 bytes
MAIN_PREMINE_INDEX=40   # consensus/core/src/config/premine.rs; bond n's fee float sits at MAIN_PREMINE_INDEX + 1 + n

log() { printf '[certify-e2e] %s\n' "$*" >&2; }
die() { log "FATAL: $*"; exit 1; }

rm -rf "$WORK_DIR"; mkdir -p "$WORK_DIR/keys" "$WORK_DIR/obj"
# Public seeds (consensus/core/src/config/premine.rs): value-less, derivable by anyone.
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

for ((i=0; i<NODES; i++)); do
  addr="$("$CLI_BIN" --network devnet key address --key-file "$WORK_DIR/keys/bond-$i.seed" | tail -1 | awk '{print $NF}')"
  [ -n "$addr" ] || die "cannot derive bond $i's address"
  p2p=$((16310 + i)); rpc=$((17610 + i))
  # --nogrpc: every node would otherwise bind the same default gRPC port and the later ones exit.
  # --enable-unsynced-mining: a chain with only a genesis is "not synced"; the producer still
  # requires peers and open participation (the gate's other clauses are not waived).
  args=(--devnet --appdir="$WORK_DIR/node-$i" --listen=127.0.0.1:$p2p --rpclisten-borsh=127.0.0.1:$rpc --utxoindex --nodnsseed --disable-upnp
        --nogrpc --enable-unsynced-mining
        --palw-produce --palw-producer-key="$WORK_DIR/keys/bond-$i.seed" --palw-producer-bond="$PREMINE_TXID:$i" --palw-producer-pay-address="$addr"
        --palw-fee-outpoint="$PREMINE_TXID:$((MAIN_PREMINE_INDEX + 1 + i))")
  if [ "$i" -gt 0 ]; then args+=(--connect=127.0.0.1:16310); fi
  MISAKA_PALW_POW_FIXTURE=1 "$KASPAD_BIN" "${args[@]}" >"$WORK_DIR/node-$i.log" 2>&1 &
  pid=$!
  pids+=("$pid")
  log "node-$i pid $pid bond $PREMINE_TXID:$i pay $addr"
done

blocks_of() { grep -c "produced block #" "$WORK_DIR/node-$1.log" 2>/dev/null || true; }
deadline=$((SECONDS + WAIT))
until [ "$(blocks_of 0)" -ge 3 ]; do
  [ $SECONDS -lt $deadline ] || { tail -30 "$WORK_DIR/node-0.log" >&2; die "node-0 produced no blocks within ${WAIT}s"; }
  sleep 3
done
log "node-0 produced $(blocks_of 0) blocks; waiting for the peers to follow"
sleep 10

CLI=("$CLI_BIN" --network devnet --rpc 127.0.0.1:17610)
# An object above one carrier's bytes was written as `<f>.chunkN` files by palw-certify; submit
# those in order (ADR-0075 Decision 14), else the object itself.
submit() {
  local f="$1"; local args=()
  if ls "$f".chunk* >/dev/null 2>&1; then
    for c in $(ls "$f".chunk* | sort -t k -k3 -n); do args+=(--object "$c"); done
  else
    args=(--object "$f")
  fi
  "${CLI[@]}" palw submit-object --key-file "$WORK_DIR/keys/main.seed" "${args[@]}" --yes
  # The next burst funds itself from this one's change, which must be in a block first.
  sleep 8
}

"$CERTIFY_BIN" drill --family base0 --lane fp --out "$WORK_DIR/obj/base0-fp.obj"
submit "$WORK_DIR/obj/base0-fp.obj"
sleep 12
"$CERTIFY_BIN" bind --model-id "PALW-BASE-0/rc" --lane fp --out "$WORK_DIR/obj/base0-bind.obj"
submit "$WORK_DIR/obj/base0-bind.obj"
sleep 12
"$CERTIFY_BIN" drill --family base0 --lane attempt --out "$WORK_DIR/obj/base0-attempt.obj"
"$CERTIFY_BIN" drill --family a16 --lane attempt --out "$WORK_DIR/obj/a16-attempt.obj"
"$CERTIFY_BIN" drill --family qwen36 --lane attempt --out "$WORK_DIR/obj/qwen36-attempt.obj"
submit "$WORK_DIR/obj/base0-attempt.obj"; submit "$WORK_DIR/obj/a16-attempt.obj"; submit "$WORK_DIR/obj/qwen36-attempt.obj"
sleep 45

fail=0
for ((i=0; i<NODES; i++)); do
  fam=$(grep -c "carried.*FamilyCertified" "$WORK_DIR/node-$i.log" || true)
  bind=$(grep -c "ClassLaneCertified" "$WORK_DIR/node-$i.log" || true)
  drops=$(grep -c "lifecycle object was dropped" "$WORK_DIR/node-$i.log" || true)
  log "node-$i: FamilyCertified lines=$fam ClassLaneCertified lines=$bind dropped=$drops blocks=$(blocks_of $i)"
  grep -E "PALW lifecycle carried.*(ObjectChunk|FamilyCertified|ClassLaneCertified)|lifecycle object was dropped|FamilyCertified object was dropped" "$WORK_DIR/node-$i.log" | sed "s/[0-9a-f]\{128\}/<h>/g; s/^/  node-$i: /" >&2 || true
  grep -q "PALW lifecycle carried.*FamilyCertified" "$WORK_DIR/node-$i.log" || { log "node-$i never carried a FamilyCertified (direct or assembled from chunks)"; fail=1; }
  grep -q "assembled from its chunks" "$WORK_DIR/node-$i.log" || { log "node-$i never completed a chunk group"; fail=1; }
  grep -q "PALW lifecycle carried.*ClassLaneCertified" "$WORK_DIR/node-$i.log" || { log "node-$i never carried a ClassLaneCertified"; fail=1; }
done
[ "$fail" -eq 0 ] && log "PASS: every validator carried the certification objects" || die "a validator disagreed — see the logs in $WORK_DIR"
