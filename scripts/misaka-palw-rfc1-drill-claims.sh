#!/usr/bin/env bash
# The RFC-0001 drill's claim producer wrapper: one floor-class claim of the given KIND, produced on this machine and submitted through the
# executor rail. Used as SUBMIT_*_CMD by scripts/misaka-palw-rfc1-drill.sh (BELOW=1: the node must refuse it by name, so a submit
# error is tolerated; BELOW=0: a failure is a failure).
#
#   misaka-palw-rfc1-drill-claims.sh constraint|constraint2|prefix|inherit|adapter
#
# `adapter` lists the RFC-0004 drill's composite candidate class (audit-improve/dm.sh `composites`: `winc` = the head class H + a PALWTIRS
# adapter section) as an adapter class (AdapterClassListed, tag 94) with `palw-class improve adapter-list`, and carries it with
# `misaka palw submit-object`. The chain must already hold H and `winc` as registered IR classes (the improve drill's registrations).
#   adapter ENV: MODEL_DIR (the improve drill's model dir; default $WORK_DIR/model), TOOLS_BIN (dir of palw-class; default BIN_DIR),
#   CLI_BIN (misaka), BOND_KEY_SEED (the lister bond's key), LISTER_BOND (txid:index, default from identity.json), FUNDING_KEY_FILE
#   (the wallet seed that pays the carrier; `--key-file` of submit-object), COMPOSITE (default winc).
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
if [ "$KIND" = adapter ]; then
  TOOLS_BIN="${TOOLS_BIN:-$BIN_DIR}"; CLI_BIN="${CLI_BIN:-$BIN_DIR/misaka}"; MODEL_DIR="${MODEL_DIR:-$WORK_DIR/model}"; COMPOSITE="${COMPOSITE:-winc}"
  [ -x "$TOOLS_BIN/palw-class" ] && [ -x "$CLI_BIN" ] || { log "palw-class (TOOLS_BIN) or misaka (CLI_BIN) missing"; exit 3; }
  [ -s "$MODEL_DIR/head.class.palwtir" ] && [ -s "$MODEL_DIR/$COMPOSITE.palwtirs" ] || { log "no composite $COMPOSITE under $MODEL_DIR (audit-improve/dm.sh composites)"; exit 3; }
  [ -n "${BOND_KEY_SEED:-}" ] && [ -f "$BOND_KEY_SEED" ] && [ -n "${FUNDING_KEY_FILE:-}" ] || { log "BOND_KEY_SEED / FUNDING_KEY_FILE not set"; exit 3; }
  SALT="${SALT:-$(tr -d ' \n' < "$WORK_DIR/SALT" 2>/dev/null || true)}"
  if [ -z "${LISTER_BOND:-}" ]; then
    LISTER_BOND="$(python3 -c "import json,sys; d=json.load(open('${IDENTITY_JSON}')); print(d['bond_txid']+':'+str(d.get('bond_index',0)))")"
  fi
  mkdir -p "$OUTBOX"; OBJ="$OUTBOX/adapter-list.obj"
  "$TOOLS_BIN/palw-class" improve adapter-list --network testnet-12 --drill-salt "$SALT" --key-file "$BOND_KEY_SEED" --bond "$LISTER_BOND" \
    --parent "$MODEL_DIR/head.class.palwtir" --section "$MODEL_DIR/$COMPOSITE.palwtirs" --out "$OBJ"
  set +e
  "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$NEW0_RPC" --palw-drill-genesis-salt="$SALT" palw submit-object --object "$OBJ" --yes --key-file "$FUNDING_KEY_FILE"
  rc=$?
  set -e
  if [ "$rc" -ne 0 ] && [ "$BELOW" != 1 ]; then log "submit-object failed ($rc) past the fence"; exit "$rc"; fi
  exit 0
fi
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
