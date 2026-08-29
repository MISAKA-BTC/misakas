#!/usr/bin/env bash
# mtp-collect.sh — the hourly MTP collection tick: scan the chain, ingest what it found.
#
# Two sources, because two different things are being claimed:
#   * chain activity (C1 mined blocks / C3 transactions) — read out of the explorer indexer's
#     PostgreSQL, address-keyed, no enrolment.
#   * validator attestations (C2) — read off the node by walking blocks for the stake-attestation
#     shard, then attributed to the address that funded each bond.
#
# Both scan a window that deliberately OVERLAPS the previous run (default 26h, run hourly) and
# ingest the result. The overlap is the point: a block's acceptance can flip after it is first
# indexed, so a window that only ever moved forward would miss activity that became accepted late.
# Re-ingesting what was already seen costs nothing — both ingest paths dedup.
#
# Collection only. It never publishes: `run-epoch` signs an artifact participants are entitled to
# rely on, so it stays an explicit operator action (see docs/testnet-participation.md).
#
# A failing stage does NOT abort the other one — a node hiccup should not cost an hour of chain
# facts — but it is reported and the tick exits non-zero, because a collector that fails quietly
# reports "no activity" for an outage and nobody finds out until the epoch is short.
#
# Install:
#   0 * * * * /opt/misaka-mtp/bin/mtp-collect.sh >> /var/log/misaka-mtp-collect.log 2>&1
set -uo pipefail

NETWORK=${MTP_NETWORK:-testnet-10}
DB=${MTP_DB:-kaspa}
DATA_DIR=${MTP_DATA_DIR:-/var/lib/misaka-mtp/data}
BIN_DIR=${MTP_BIN_DIR:-/opt/misaka-mtp/bin}
# Node wRPC Borsh endpoint the attestation/roster readers talk to. Empty disables the C2 stage.
RPC=${MTP_RPC:-}
# How far back the attestation walk starts. The indexer walks DOWN from the tip, so this bounds
# work per tick; ingestion dedups, so an overlap with the last tick is free.
ATT_MAX_BLOCKS=${MTP_ATT_MAX_BLOCKS:-50000}
# How far back each chain-activity tick looks. Must exceed the cron interval, or activity that
# lands between runs — or is accepted late — is never seen.
LOOKBACK_HOURS=${MTP_LOOKBACK_HOURS:-26}

now_ms=$(( $(date -u +%s) * 1000 ))
since_ms=$(( now_ms - LOOKBACK_HOURS * 3600 * 1000 ))

work=$(mktemp -d /tmp/mtp-collect.XXXXXX)
trap 'rm -rf "$work"' EXIT
# mktemp gives 0700 owned by whoever runs the tick; the ingest step drops to the service user and
# has to read the file it produced. Traversable dir, world-readable file — the contents are public
# chain data either way.
chmod 0755 "$work"

rc=0
echo "[$(date -u +%FT%TZ)] mtp-collect: ${NETWORK} db=${DB} lookback=${LOOKBACK_HOURS}h rpc=${RPC:-<off>}"

# --- C1 / C3: address-keyed chain activity -------------------------------------------------------
if "$BIN_DIR/mtp-scan-chain.sh" \
     --network "$NETWORK" --db "$DB" \
     --since-ms "$since_ms" --until-ms "$now_ms" > "$work/activity.jsonl"; then
  if [ -s "$work/activity.jsonl" ]; then
    chmod 0644 "$work/activity.jsonl"
    # The service owns its data dir; ingest as the service user so the fact files keep one owner.
    sudo -u misaka-mtp "$BIN_DIR/misaka-mtp-service" ingest-chain-activity \
      --data-dir "$DATA_DIR" --file "$work/activity.jsonl" --network "$NETWORK" || rc=1
  else
    echo "  nothing on chain in the window — no chain-activity facts to ingest"
  fi
else
  echo "  ERROR: chain scan failed — C1/C3 facts for this window were NOT collected" >&2
  rc=1
fi

# --- C2: validator attestations ------------------------------------------------------------------
# Skipped, loudly, when no RPC is configured: a validator whose attestations are never read looks
# exactly like a validator that never attested, and only one of those should cost points.
if [ -z "$RPC" ]; then
  echo "  NOTE: MTP_RPC unset — validator attestations (C2) are NOT being collected"
else
  att_ok=1
  "$BIN_DIR/misaka" mtp validators \
    --network "$NETWORK" --rpc "$RPC" --output json --out "$work/bonds.jsonl" >/dev/null || att_ok=0
  if [ "$att_ok" = 1 ]; then
    "$BIN_DIR/mtp-validator-roster.sh" --db "$DB" --bonds "$work/bonds.jsonl" > "$work/roster.jsonl" || att_ok=0
  fi
  if [ "$att_ok" = 1 ]; then
    "$BIN_DIR/misaka" mtp attestations \
      --network "$NETWORK" --rpc "$RPC" --output json \
      --max-blocks "$ATT_MAX_BLOCKS" --out "$work/att.jsonl" >/dev/null || att_ok=0
  fi
  if [ "$att_ok" = 1 ]; then
    chmod 0644 "$work/att.jsonl" "$work/roster.jsonl" "$work/bonds.jsonl" 2>/dev/null
    if [ -s "$work/att.jsonl" ]; then
      sudo -u misaka-mtp "$BIN_DIR/misaka-mtp-service" ingest-attestations \
        --data-dir "$DATA_DIR" --file "$work/att.jsonl" \
        --roster "$work/roster.jsonl" --bonds "$work/bonds.jsonl" --network "$NETWORK" || rc=1
    else
      echo "  no attestations in the scanned range — no C2 facts to ingest"
    fi
  else
    echo "  ERROR: attestation collection failed — C2 facts for this tick were NOT collected" >&2
    rc=1
  fi
fi

exit "$rc"
