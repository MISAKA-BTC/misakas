#!/usr/bin/env bash
# deploy-t12/seeders/50-rollback.sh — put back the binary AND the config that 30-swap.sh saved as
# *.bak-t12seed-<TS>, restart, verify. The backups are left in place (never deleted).
#
#   CONFIRM=yes ./50-rollback.sh <seeder> <TS printed by 30-swap.sh>
source "$(dirname "$0")/lib-seeders.sh"

load_seeder "${1:-}"
TS="${2:?usage: CONFIRM=yes $0 <seeder> <TS>}"
[[ "$TS" =~ ^[0-9]{8}T[0-9]{6}Z$ ]] || { echo "TS looks wrong: $TS" >&2; exit 2; }
need_confirm
EPOCH=$(date +%s)

exp_sha=$(rsh "bash -s" <<EOF
set -euo pipefail
[ -f '$S_BIN.bak-t12seed-$TS' ]  || { echo "missing $S_BIN.bak-t12seed-$TS" >&2; exit 1; }
[ -f '$S_CONF.bak-t12seed-$TS' ] || { echo "missing $S_CONF.bak-t12seed-$TS" >&2; exit 1; }
cp -a '$S_BIN.bak-t12seed-$TS' '$S_BIN.rb-$TS'
mv -f '$S_BIN.rb-$TS' '$S_BIN'
cp -a '$S_CONF.bak-t12seed-$TS' '$S_CONF'
systemctl daemon-reload
systemctl restart '$UNIT'
sleep 2
systemctl is-active '$UNIT' >&2
sha256sum '$S_BIN' | cut -c1-64
EOF
)
echo "rolled back $S_NAME to the $TS backup (sha ${exp_sha:0:8}…); waiting 20 s for a refresh…"
sleep 20
"$SEED_DIR/40-verify.sh" "$S_NAME" "$EPOCH" "$exp_sha"
