#!/bin/bash
# deploy-t12/install-113.sh — 169.58.232.113 (hostname vmi3527497): bond 6, the PUBLIC entry node.
# Also hosts misakascan (nginx), postgres, the explorer filler/REST, the seeder for seeder1, the
# t11 validators, MTP and the miner pool — so it gets ONE node and a 4 GiB reserve.
#
#   b6  misaka-t12-node (drop-in)  0.0.0.0:26311  gRPC 26312  borsh 26313  json 26314  EVM 8545
#       floor producer + 8k seat + HEARTBEAT
#       share 8,192 MiB (R-core+, PLAN.md §2) = floor attempt 2,228 + its own floor DA answer 2,228 + one
#       8k seat duty 3,493 (+ 243)
#   gRPC 26312 → kaspa-t11-db-filler / kaspa-t11-rest-server; borsh 26313 → seeder (and the t11
#   validators' --node-wrpc-borsh); json 26314 → nginx upstream misaka_json and wallet.misakascan.com
#   /kaspa; EVM 8545 → nginx /evm.
#
# Explorer: `explorer-apply` points nginx/filler/REST at b6 and a FRESH database (the old one holds
# the old genesis's blocks); `explorer-rollback` undoes exactly that. Run it after `switch` + `check`.
#
# Run as root on .113 from $REL_ROOT/kit:  ./install-113.sh preflight | stage | switch | check | explorer-apply | rollback
. "$(dirname "$0")/lib.sh"

HOST_NAME_EXPECTED=vmi3527497
RESERVE_MIB=4096             # non-kaspad RSS 1.3 GiB measured 09-23 + postgres cache + margin (PLAN.md §2: 12,288 of 24,033 MiB)
START_GAP=10
BINARIES=(kaspad misaka palw-class)
NODES=(
  "6|misaka-t12-node|dropin|0.0.0.0:26311|26313|26314|26312|8545|8192|11|floor|1|1|169.58.39.220:26311,169.58.39.220:26321"
)
OLD_APPDIRS=(/root/.t12)
EXPLORER_DB=${EXPLORER_DB:-kaspa_t12r}
NGINX_SITE=/etc/nginx/sites-enabled/misakascan
EXPLORER_UNITS=(kaspa-t11-db-filler kaspa-t11-rest-server)

host_preflight() {
    say "  misaka-t11-node: $(systemctl show -p ActiveState --value misaka-t11-node) / $(systemctl is-enabled misaka-t11-node 2>/dev/null || true)"
    say "  t12p tunnel ports here (other session's private chain): $(ss -ltnH 'sport >= :36300 and sport <= :36600' 2>/dev/null | awk '{print $4}' | tr '\n' ' ')"
    say "  nginx private upstreams still in $NGINX_SITE: $(grep -cE '127\.0\.0\.1:(36314|36545)' "$NGINX_SITE" || true)"
    local u; for u in "${EXPLORER_UNITS[@]}"; do
        say "  $u: $(systemctl show -p Environment --value $u | tr ' ' '\n' | grep -E '^KASPAD_HOST_1=' | tail -1) db=$(systemctl show -p Environment --value $u | tr ' ' '\n' | grep -E '^SQL_URI=' | tail -1 | sed 's#.*/##')"
    done
    say "  t11 validators on this host use --node-wrpc-borsh 127.0.0.1:26313 (they already talk to the t12 node today)"
    ls -l /root/palw-class/qwen25-1.5b-a16-8k.palwart* 2>/dev/null | sed 's/^/  /' || true
}

host_guard_switch() {
    systemctl is-active --quiet misaka-t11-node && die "misaka-t11-node is running — it would fight b6 for 26311"
    return 0
}

explorer_apply() {
    systemctl is-active --quiet misaka-t12-node || die "misaka-t12-node is not running"
    python3 "$KIT_DIR/t12check.py" --port 26314 --expect-fp "$EXPECT_FP" --expect-genesis "$EXPECT_GENESIS" \
        || die "b6 does not pass its check — not pointing the public explorer at it"
    mkdir -p "$STATE_DIR"
    local bak="$STATE_DIR/misakascan.nginx.before-t12r-$TS"
    cp -p "$NGINX_SITE" "$bak"; record_state "NGINX_BAK $NGINX_SITE $bak"
    if grep -qE '127\.0\.0\.1:(36314|36545)' "$NGINX_SITE"; then
        # the three lines the route-matrix session re-pointed at its private chain (09-23 14:50), back to local ports
        sed -i -e 's#server 127\.0\.0\.1:36314 #server 127.0.0.1:26314 #' \
               -e 's#proxy_pass http://127\.0\.0\.1:36314;#proxy_pass http://127.0.0.1:28014;#' \
               -e 's#proxy_pass http://127\.0\.0\.1:36545/;#proxy_pass http://127.0.0.1:8545/;#' "$NGINX_SITE"
    fi
    if grep -qE '127\.0\.0\.1:(36312|36314|36545)' "$NGINX_SITE"; then cp -p "$bak" "$NGINX_SITE"; die "private ports still referenced after the edit — restored $bak"; fi
    nginx -t 2>&1 | tail -2 || true
    nginx -t >/dev/null 2>&1 || { cp -p "$bak" "$NGINX_SITE"; die "nginx -t failed — restored $bak"; }
    systemctl reload nginx && say "  nginx: misaka_json → 26314 (b6), /kaspa-hub → 28014 (ibm b0 via vantage tunnel), /evm → 8545 (b6)"
    if ! sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='$EXPLORER_DB'" | grep -q 1; then
        sudo -u postgres createdb -O kaspa "$EXPLORER_DB" && record_state "CREATEDB $EXPLORER_DB" && say "  created database $EXPLORER_DB (the filler creates its tables)"
    else say "  database $EXPLORER_DB exists"; fi
    local u uri
    for u in "${EXPLORER_UNITS[@]}"; do
        uri=$(systemctl show -p Environment --value "$u" | tr ' ' '\n' | grep -E '^SQL_URI=' | tail -1 | cut -d= -f2-)
        [ -n "$uri" ] || die "$u has no SQL_URI"
        mkdir -p "/etc/systemd/system/$u.service.d"
        # zz- sorts after the route-matrix session's t12p.conf, so these assignments win without deleting its file
        atomic_write "/etc/systemd/system/$u.service.d/zz-t12r.conf" 0644 <<EOF
# deploy-t12: the regenesis chain (release $REV). Delete + daemon-reload + restart to undo.
[Service]
Environment=KASPAD_HOST_1=127.0.0.1:26312
Environment=SQL_URI=${uri%/*}/$EXPLORER_DB
EOF
        record_state "EXPLORER_DROPIN $u"
    done
    systemctl daemon-reload
    systemctl restart "${EXPLORER_UNITS[@]}"
    sleep 5
    for u in "${EXPLORER_UNITS[@]}"; do say "  $u: $(systemctl is-active "$u")"; journalctl -u "$u" -n 3 --no-pager | cut -c1-180 | sed 's/^/    /'; done
    warn "the explorer JS (/var/www/misaka-explorer/app.js: PANEL_SEATS roster, LLM_CLASSES) is NOT changed by this kit — PLAN.md §7"
}

explorer_rollback() {
    local log="$STATE_DIR/switch-$REV.log" kind a b u
    for u in "${EXPLORER_UNITS[@]}"; do rm -f "/etc/systemd/system/$u.service.d/zz-t12r.conf"; done
    systemctl daemon-reload; systemctl restart "${EXPLORER_UNITS[@]}"
    b=$(grep '^NGINX_BAK ' "$log" 2>/dev/null | head -1 | awk '{print $3}')
    if [ -n "$b" ] && [ -f "$b" ]; then cp -p "$b" "$NGINX_SITE" && nginx -t && systemctl reload nginx && say "  nginx restored from $b"; fi
    say "explorer rolled back (database $EXPLORER_DB kept)"
}

host_usage() { echo "  (113) explorer-apply | explorer-rollback   EXPLORER_DB=$EXPLORER_DB"; }
host_cmd() {
    case "$1" in
        explorer-apply) explorer_apply ;;
        explorer-rollback) explorer_rollback ;;
        *) usage_common; host_usage; die "unknown command $1" ;;
    esac
}

main_dispatch "$@"
