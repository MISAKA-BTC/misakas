#!/usr/bin/env bash
# The RFC-0001 drill's claim producer wrapper: one floor-class claim of the given KIND, produced on this machine and submitted through the
# executor rail. Used as SUBMIT_*_CMD by scripts/misaka-palw-rfc1-drill.sh (BELOW=1: the node must refuse it by name, so a submit
# error is tolerated; BELOW=0: a failure is a failure).
#
#   misaka-palw-rfc1-drill-claims.sh constraint|constraint2|prefix|inherit
#
# ENV: BOND_KEY_SEED (a 0600 seed file of the executor bond key; required), NEW0_RPC (57200), SALT (else $WORK_DIR/SALT),
#   WORK_DIR (~/.misaka-palw-rfc1-drill), CLASS_ID (the floor class id, 128 hex; required unless IDENTITY_JSON exists),
#   IDENTITY_JSON (default $WORK_DIR/identity.json, written with `misaka-palw-fp-rail --print-identity` when absent),
#   PRODUCER_BIN (misaka-palw-rfc1-drill-claim) and RAIL_BIN (misaka-palw-fp-rail), both next to KASPAD_BIN by default.
# The constraint kinds write the floor's token table to $WORK_DIR/floor.palwtokens; every node that seats the claim must hold it
# (--palw-token-table, or $WORK_DIR/floor.palwtokens copied beside the class artifact as <artifact>.palwtokens), else it abstains.
set -euo pipefail
KIND="${1:?usage: $0 constraint|constraint2|prefix|inherit}"
WORK_DIR="${WORK_DIR:-$HOME/.misaka-palw-rfc1-drill}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$(dirname "${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}")"
PRODUCER_BIN="${PRODUCER_BIN:-$BIN_DIR/misaka-palw-rfc1-drill-claim}"
RAIL_BIN="${RAIL_BIN:-$BIN_DIR/misaka-palw-fp-rail}"
NEW0_RPC="${NEW0_RPC:-57200}"
BELOW="${BELOW:-0}"
IDENTITY_JSON="${IDENTITY_JSON:-$WORK_DIR/identity.json}"
OUTBOX="$WORK_DIR/outbox-$KIND"
log() { printf '[rfc1-claim:%s] %s\n' "$KIND" "$*" >&2; }
[ -x "$PRODUCER_BIN" ] || { log "no producer at $PRODUCER_BIN"; exit 3; }
[ -x "$RAIL_BIN" ] || { log "no rail at $RAIL_BIN"; exit 3; }
[ -n "${BOND_KEY_SEED:-}" ] && [ -f "$BOND_KEY_SEED" ] || { log "BOND_KEY_SEED is not a file"; exit 3; }
SALT="${SALT:-$(tr -d ' \n' < "$WORK_DIR/SALT" 2>/dev/null || true)}"
mkdir -p "$WORK_DIR" "$OUTBOX"
if [ ! -s "$IDENTITY_JSON" ]; then
  : "${CLASS_ID:?CLASS_ID (the floor class id) is needed to write identity.json}"
  "$RAIL_BIN" --print-identity --bond-key-seed "$BOND_KEY_SEED" --rpc "127.0.0.1:$NEW0_RPC" --class-id "$CLASS_ID" \
    --palw-drill-genesis-salt "$SALT" > "$IDENTITY_JSON"
fi
rm -f "$OUTBOX"/fp-job-*   # one claim per call: the rail's watcher takes whatever is in the outbox
STEM="$("$PRODUCER_BIN" --kind "$KIND" --identity "$IDENTITY_JSON" --outbox "$OUTBOX" --rpc "127.0.0.1:$NEW0_RPC" \
  --emit-token-table "$WORK_DIR/floor.palwtokens")"
log "produced $STEM"
set +e
"$RAIL_BIN" --watch "$OUTBOX" --once --coinbase-funding-only --bond-key-seed "$BOND_KEY_SEED" --rpc "127.0.0.1:$NEW0_RPC" \
  --palw-drill-genesis-salt "$SALT" --retention-dir "$OUTBOX"
rc=$?
set -e
if [ "$rc" -ne 0 ] && [ "$BELOW" != 1 ]; then log "the rail failed ($rc) past the fence"; exit "$rc"; fi
exit 0
