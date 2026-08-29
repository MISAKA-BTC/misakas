#!/usr/bin/env bash
# mtp-validator-roster.sh — join each bonded validator to the address that funded its bond.
#
# `ingest-attestations` needs `{"validator_id":…,"owner_id":…}`: it filters facts by LEDGER id, so
# a row still carrying the raw chain hash is dropped silently and the validator's attestations never
# score. Nothing on the bond record carries an address — `owner_pubkey_hash` is the validator's own
# key hash, not a payout target — so the link has to be derived, and the only thing the chain itself
# states about who a bond belongs to is who paid for it: the bond transaction's inputs.
#
# That makes the mapping permissionless (no operator table, no enrolment) and re-derivable by anyone
# from public data. A bond whose funding transaction is not in the indexer's range resolves to
# nothing; it is reported on stderr and left out rather than guessed at, because a wrong owner pays
# the wrong participant.
#
#   misaka mtp validators --output json --out bonds.jsonl
#   mtp-validator-roster.sh --db kaspa_t11 --bonds bonds.jsonl > roster.jsonl
set -euo pipefail

DB=${MTP_DB:-kaspa}
BONDS=
PSQL_USER=postgres

while [ $# -gt 0 ]; do
  case "$1" in
    --db) DB=${2:?}; shift 2 ;;
    --bonds) BONDS=${2:?}; shift 2 ;;
    --psql-user) PSQL_USER=${2:?}; shift 2 ;;
    -h|--help) echo "usage: mtp-validator-roster.sh --bonds BONDS.jsonl [--db NAME] [--psql-user USER]" >&2; exit 2 ;;
    *) echo "mtp-validator-roster.sh: unknown argument '$1'" >&2; exit 2 ;;
  esac
done
[ -n "$BONDS" ] || { echo "mtp-validator-roster.sh: --bonds is required" >&2; exit 2; }

# One query for the whole roster: a per-bond round trip would be N psql spawns and would still have
# to be re-run every tick.
ids=$(python3 -c '
import json,sys
seen=[]
for line in open(sys.argv[1]):
    line=line.strip()
    if not line: continue
    b=json.loads(line)
    seen.append((b["validator_id"], b["bond_outpoint"].split(":")[0]))
print("\n".join(f"{v}\t{t}" for v,t in seen))
' "$BONDS")

[ -n "$ids" ] || { echo "mtp-validator-roster.sh: no bonds on file" >&2; exit 0; }

txids=$(printf '%s\n' "$ids" | cut -f2 | sort -u | sed "s/^/'/;s/$/'/" | paste -sd, -)

sql="SELECT i.transaction_id, min(po.script_public_key_address)
     FROM transactions_inputs i
     JOIN transactions_outputs po
       ON po.transaction_id = i.previous_outpoint_hash
      AND po.index = i.previous_outpoint_index
     WHERE i.transaction_id IN ($txids)
     GROUP BY i.transaction_id"

funders=$(sudo -u "$PSQL_USER" psql -d "$DB" -At -F $'\t' -v ON_ERROR_STOP=1 -c "$sql")

printf '%s\n' "$ids" | python3 -c '
import json,sys
funders={}
for line in sys.argv[1].splitlines():
    if not line.strip(): continue
    tx,addr=line.split("\t",1)
    if addr: funders[tx]=addr
resolved=unresolved=0
for line in sys.stdin:
    if not line.strip(): continue
    vid,tx=line.rstrip("\n").split("\t",1)
    addr=funders.get(tx)
    if not addr:
        unresolved+=1
        print(f"  no funding input indexed for bond tx {tx[:16]}… — validator {vid[:16]}… left unattributed", file=sys.stderr)
        continue
    resolved+=1
    print(json.dumps({"validator_id":vid,"owner_id":"addr:"+addr}, separators=(",",":")))
print(f"roster: {resolved} validator(s) attributed, {unresolved} unattributed", file=sys.stderr)
' "$funders"
