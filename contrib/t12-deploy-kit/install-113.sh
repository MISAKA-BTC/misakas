#!/bin/bash
# deploy-t12/install-113.sh — 169.58.232.113 (hostname vmi3527497): bond 6, the PUBLIC entry node.
# Also hosts misakascan (nginx), postgres, the explorer filler/REST, the seeder for seeder1, the
# t11 validators, MTP and the miner pool — so it gets ONE node and a 4 GiB reserve.
#
#   b6  misaka-t12-node (drop-in)  0.0.0.0:26311  gRPC 26312  borsh 26313  json 26314  EVM 8545
#       floor producer + 8k seat + HEARTBEAT
#       share 8,192 MiB (R-core+, PLAN.md §2) = floor attempt 2,229 + its own floor DA answer 2,229 + one
#       8k seat duty 3,500 (+ 234); MemoryMax 17G = a crash guard sized so the ledger's cgroup term
#       (memory.max − memory.current, page cache included) never binds below the share (PLAN.md §2)
#   gRPC 26312 → kaspa-t11-db-filler / kaspa-t11-rest-server; borsh 26313 → seeder (and the t11
#   validators' --node-wrpc-borsh); json 26314 → nginx upstream misaka_json and wallet.misakascan.com
#   /kaspa; EVM 8545 → nginx /evm.
#
# Explorer: ONE of two wirings, never both (DEPLOY.md §9 — the operator's choice, `../misakascan-t12/
# deploy.sh` recommended): `explorer-apply` here points nginx/filler/REST at b6 and a FRESH database
# `kaspa_t12r` (drop-in zz-t12r.conf, no cursor seed); `deploy.sh` writes t12g.conf and seeds `kaspa_t12`
# at genesis. zz-t12r.conf sorts after t12g.conf and would silently win, so each refuses when the
# other's drop-in is present. `explorer-rollback` undoes explorer-apply. Run after `switch` + `check`.
#
# switch refuses while the t11 DNS-finality validators (misaka-validator, misaka-validator-2) run: they
# dial 127.0.0.1:26313 with testnet-11 settings and restart-loop against b6 (PLAN.md §10 Q7) — stop them
# first; they come back as testnet-12 validators after launch (checklist §4 step 18).
#
# Run as root on .113 from $REL_ROOT/kit:  ./install-113.sh preflight | stage | switch | check | explorer-apply | rollback
# The chain is LIVE (09-25): a new binary under it is  ./install-113.sh stage && ./install-113.sh upgrade  (PLAN.md §15),
# undone by  ./install-113.sh upgrade-rollback  — never by `rollback` (that retires the chain).
. "$(dirname "$0")/lib.sh"

HOST_NAME_EXPECTED=vmi3527497
RESERVE_MIB=4096             # non-kaspad RSS 1.3 GiB measured 09-23 + postgres cache + margin (PLAN.md §2: 12,288 of 24,033 MiB)
START_GAP=10
# upgrade: before b6 stops, ibm's b0 (the other heartbeat miner, the 8k producer) and b1 must take a TCP
# connection (PLAN.md §15)
UPGRADE_REQUIRE_UP=(169.58.39.220:26311 169.58.39.220:26321)
BINARIES=(kaspad misaka palw-class)
NODES=(
  "6|misaka-t12-node|dropin|0.0.0.0:26311|26313|26314|26312|8545|8192|17|floor|1|1|169.58.39.220:26311,169.58.39.220:26321"
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
    local v; for v in "${T11_VALIDATORS[@]}"; do say "  $v: $(systemctl show -p ActiveState --value "$v") — switch refuses while it runs (t11 settings against b6's 26313, PLAN.md §10 Q7)"; done
    [ -z "$(explorer_other_wiring)" ] || say "  explorer: deploy.sh's t12g.conf is present — explorer-apply will refuse (DEPLOY.md §9)"
    ls -l /root/palw-class/qwen25-1.5b-a16-8k.palwart* 2>/dev/null | sed 's/^/  /' || true
}

T11_VALIDATORS=(misaka-validator misaka-validator-2)

host_guard_switch() {
    systemctl is-active --quiet misaka-t11-node && die "misaka-t11-node is running — it would fight b6 for 26311"
    local v running=""
    for v in "${T11_VALIDATORS[@]}"; do systemctl is-active --quiet "$v" && running+="$v "; done
    [ -z "$running" ] || die "the t11 DNS-finality validators are running (${running% }): with testnet-11 settings they dial 127.0.0.1:26313 and restart-loop against b6 — \`systemctl stop ${T11_VALIDATORS[*]}\` first (PLAN.md §10 Q7, operator decision)"
    return 0
}

# the other explorer wiring's drop-in (DEPLOY.md §9): deploy.sh's t12g.conf
explorer_other_wiring() {
    local u
    for u in "${EXPLORER_UNITS[@]}"; do [ -e "/etc/systemd/system/$u.service.d/t12g.conf" ] && echo "/etc/systemd/system/$u.service.d/t12g.conf"; done
    return 0
}

explorer_apply() {
    local other; other=$(explorer_other_wiring)
    [ -z "$other" ] || die "misakascan deploy.sh already wired the explorer ($other) — the two wirings are exclusive (DEPLOY.md §9); use deploy.sh's rollback, or keep it"
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
