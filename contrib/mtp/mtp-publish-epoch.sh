#!/usr/bin/env bash
# mtp-publish-epoch.sh — sign and publish one MTP epoch ledger, with the checks that make it safe.
#
# `run-epoch` signs an artifact participants are entitled to rely on, so it stays an explicit
# operator action — this wrapper does not schedule it, it only makes the one command hard to get
# wrong. It refuses to publish a window that has not closed yet (a signed ledger covering 1 day of a
# 7-day epoch reads as the week's total to anyone who opens it) and refuses to overwrite an epoch
# that is already in the index (a re-issue is a `supersedes`, a deliberate act, not a re-run).
#
# It computes into a COPY first and prints the score rows. Nothing is signed with the production key
# until that preview succeeds, so a bad window or an empty fact store is caught before it is public.
#
#   mtp-publish-epoch.sh --epoch 3 --start 2026-08-28T00:00:00Z --end 2026-09-04T00:00:00Z
#   mtp-publish-epoch.sh --epoch 3 --start … --end … --preview-only
set -euo pipefail

NETWORK=${MTP_NETWORK:-testnet-11}
DATA_DIR=${MTP_DATA_DIR:-/var/lib/misaka-mtp/data}
BIN_DIR=${MTP_BIN_DIR:-/opt/misaka-mtp/bin}
KEY=${MTP_OPERATOR_KEY:-/etc/misaka-mtp/operator.seed}
API=${MTP_API:-http://127.0.0.1:8790/mtp/v1}
EPOCH= ; START= ; END= ; PREVIEW_ONLY=0

while [ $# -gt 0 ]; do
  case "$1" in
    --epoch) EPOCH=${2:?}; shift 2 ;;
    --start) START=${2:?}; shift 2 ;;
    --end) END=${2:?}; shift 2 ;;
    --preview-only) PREVIEW_ONLY=1; shift ;;
    -h|--help) echo "usage: mtp-publish-epoch.sh --epoch N --start RFC3339 --end RFC3339 [--preview-only]" >&2; exit 2 ;;
    *) echo "mtp-publish-epoch.sh: unknown argument '$1'" >&2; exit 2 ;;
  esac
done
[ -n "$EPOCH" ] && [ -n "$START" ] && [ -n "$END" ] || { echo "mtp-publish-epoch.sh: --epoch, --start and --end are all required" >&2; exit 2; }

now_s=$(date -u +%s)
end_s=$(date -u -d "$END" +%s)
if [ "$end_s" -gt "$now_s" ]; then
  echo "REFUSING: epoch $EPOCH ends $END, which is $(( (end_s - now_s) / 3600 ))h away. A signed ledger" >&2
  echo "          over an open window reads as the whole epoch to whoever opens it. Wait, or publish" >&2
  echo "          deliberately as a provisional issue that a later one supersedes." >&2
  exit 3
fi

if [ -f "$DATA_DIR/points/index.json" ] && \
   python3 -c "import json,sys; d=json.load(open('$DATA_DIR/points/index.json')); sys.exit(0 if any(e['epoch']==$EPOCH for e in d['entries']) else 1)"; then
  echo "REFUSING: epoch $EPOCH is already in the index. A re-issue is a supersede — do it on purpose." >&2
  exit 3
fi

# --- preview on a copy, with a throwaway key: nothing production-signed until this passes ---------
work=$(mktemp -d /tmp/mtp-publish.XXXXXX)
trap 'rm -rf "$work"' EXIT
chmod 0700 "$work"
cp -a "$DATA_DIR" "$work/data"
head -c 32 /dev/urandom | xxd -p -c 64 > "$work/throwaway.seed"
chmod 600 "$work/throwaway.seed"

echo "== preview: epoch $EPOCH [$START, $END) on $NETWORK =="
"$BIN_DIR/misaka-mtp-service" run-epoch --data-dir "$work/data" --operator-key "$work/throwaway.seed" \
  --epoch "$EPOCH" --start "$START" --end "$END" --network "$NETWORK"

python3 - "$work/data/points/epoch-$EPOCH.0.jsonl" <<'PY'
import json,sys
d=json.loads(open(sys.argv[1]).read().split("\n")[0])
rows=sorted(d["scores"], key=lambda r: -sum(r[c] for c in ("c1","c2","c3","c4","c5")))
print(f"  range {d['range'][0]} -> {d['range'][1]}   {len(rows)} scored id(s)")
for i,r in enumerate(rows,1):
    t=sum(r[c] for c in ("c1","c2","c3","c4","c5"))
    print(f"  {i:3d}  {t/1000:8.1f} pt   c1={r['c1']/1000:.0f} c2={r['c2']/1000:.0f} c3={r['c3']/1000:.0f} c4={r['c4']/1000:.0f} c5={r['c5']/1000:.0f}   {r['id']}")
PY

if [ "$PREVIEW_ONLY" = 1 ]; then
  echo "== preview only — nothing published =="
  exit 0
fi

# --- the real thing ------------------------------------------------------------------------------
echo "== publishing epoch $EPOCH with the production operator key =="
sudo -u misaka-mtp "$BIN_DIR/misaka-mtp-service" run-epoch --data-dir "$DATA_DIR" --operator-key "$KEY" \
  --epoch "$EPOCH" --start "$START" --end "$END" --network "$NETWORK"

# The API re-opens the archive per request, so a publish should be visible with no restart. Assert
# it rather than assume it: a ledger nobody can fetch is not published.
served=$(curl -s --max-time 20 "$API/epoch/$EPOCH" | head -c 200 || true)
if printf '%s' "$served" | grep -q "\"epoch\":$EPOCH"; then
  echo "== served: $API/epoch/$EPOCH is live =="
else
  echo "WARNING: $API/epoch/$EPOCH did not return epoch $EPOCH — the file is signed on disk but the" >&2
  echo "         running service is not serving it. Restart misaka-mtp and re-check." >&2
  exit 4
fi
