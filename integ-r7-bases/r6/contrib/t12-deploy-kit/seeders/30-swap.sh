#!/usr/bin/env bash
# deploy-t12/seeders/30-swap.sh — ONE seeder at a time: swap its binary (to one 20-stage.sh put
# there) and/or its anchor list, restart it, and verify. Backs up the binary AND the config file as
# *.bak-t12seed-<TS> first; 50-rollback.sh <seeder> <TS> puts both back.
#
#   CONFIRM=yes ./30-swap.sh <seeder> --binary <full sha256>          # binary only
#   CONFIRM=yes ./30-swap.sh <seeder> --anchors 169.58.232.113        # anchors only
#   CONFIRM=yes ./30-swap.sh <seeder> --binary <sha> --anchors A,B    # both, one restart
#
# Order (lib-seeders.sh row order): c5104 → seeder3 → ibm → seeder1. Run 40-verify.sh on the one
# you just swapped (this script does) and do not start the next until it passes. A restart leaves
# :53 unbound for ~1 s; resolvers retry the other delegated seeder, so do not swap seeder1 and
# seeder3 at the same time.
source "$(dirname "$0")/lib-seeders.sh"

load_seeder "${1:-}"; shift || true
BIN_SHA=""; ANCH=""
while [ $# -gt 0 ]; do
  case "$1" in
    --binary)  BIN_SHA="${2:?}"; shift 2 ;;
    --anchors) ANCH="${2:?}"; shift 2 ;;
    *) echo "usage: CONFIRM=yes $0 <seeder> [--binary <sha256>] [--anchors a.b.c.d,...]" >&2; exit 2 ;;
  esac
done
[ -n "$BIN_SHA$ANCH" ] || { echo "nothing to do: give --binary and/or --anchors" >&2; exit 2; }
[ -z "$BIN_SHA" ] || [[ "$BIN_SHA" =~ ^[0-9a-f]{64}$ ]] || { echo "--binary wants the FULL sha256" >&2; exit 2; }
[ -z "$ANCH" ] || valid_anchors "$ANCH" || { echo "bad --anchors '$ANCH' (comma-separated IPv4)" >&2; exit 2; }
need_confirm

TS=$(date -u +%Y%m%dT%H%M%SZ)
EPOCH=$(date +%s)
echo "swap $S_NAME at $TS: binary=${BIN_SHA:-unchanged} anchors=${ANCH:-unchanged}"

rsh "bash -s" <<EOF
set -euo pipefail
cp -a '$S_CONF' '$S_CONF.bak-t12seed-$TS'
cp -a '$S_BIN'  '$S_BIN.bak-t12seed-$TS'
echo "backup: $S_CONF.bak-t12seed-$TS  $S_BIN.bak-t12seed-$TS"
if [ -n '$BIN_SHA' ]; then
  staged='$S_BIN.new-${BIN_SHA:0:8}'
  [ -f "\$staged" ] || { echo "not staged: \$staged (run 20-stage.sh first)"; exit 1; }
  got=\$(sha256sum "\$staged" | cut -c1-64)
  [ "\$got" = '$BIN_SHA' ] || { echo "staged sha \$got != $BIN_SHA"; exit 1; }
  # never write into the running file: copy beside it, then rename over it (atomic).
  cp -a "\$staged" '$S_BIN.swap-$TS'
  mv -f '$S_BIN.swap-$TS' '$S_BIN'
fi
if [ -n '$ANCH' ]; then
  if [ '$S_STYLE' = env ]; then
    grep -q '^MISAKA_SEEDER_ANCHORS=' '$S_CONF' || { echo "no MISAKA_SEEDER_ANCHORS line in $S_CONF"; exit 1; }
    sed -i 's/^MISAKA_SEEDER_ANCHORS=.*/MISAKA_SEEDER_ANCHORS=$ANCH/' '$S_CONF'
  else
    [ "\$(grep -c -- '--anchors ' '$S_CONF')" = 1 ] || { echo "expected exactly one --anchors in $S_CONF"; exit 1; }
    sed -i -E 's/--anchors [^ ]+/--anchors $ANCH/' '$S_CONF'
  fi
  diff '$S_CONF.bak-t12seed-$TS' '$S_CONF' || true
fi
systemctl daemon-reload
systemctl restart '$UNIT'
sleep 2
systemctl is-active '$UNIT'
EOF

exp_sha="${BIN_SHA:-$(rsh "sha256sum '$S_BIN' | cut -c1-64")}"
echo "waiting 20 s for the first refresh cycle (wRPC 5 s + dial 3 s + margin)…"
sleep 20
if "$SEED_DIR/40-verify.sh" "$S_NAME" "$EPOCH" "$exp_sha"; then
  echo "OK $S_NAME swapped at $TS. rollback if needed: CONFIRM=yes ./50-rollback.sh $S_NAME $TS"
else
  echo "VERIFY FAILED on $S_NAME. roll back now: CONFIRM=yes ./50-rollback.sh $S_NAME $TS" >&2
  exit 1
fi
