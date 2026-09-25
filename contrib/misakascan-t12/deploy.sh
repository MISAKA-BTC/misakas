#!/usr/bin/env bash
# misakascan -> the R-core+ testnet-12 genesis (fleet.env EXPECT_GENESIS). PREPARED, NEVER RUN. Read DEPLOY.md first.
#
#   ./deploy.sh plan                 print what would happen (no remote writes)
#   ./deploy.sh preflight            read-only checks, local + .113
#   ./deploy.sh backup               copies of site files, nginx vhost, unit drop-ins, kaspa_t12p dump
#   ./deploy.sh db                   CREATE DATABASE $NEW_DB, create tables, seed the filler cursor at genesis
#   ./deploy.sh units                filler + REST onto $NODE_GRPC / $NEW_DB (drop-in t12g.conf), restart them
#   ./deploy.sh nginx                /kaspa, /kaspa-seed, /kaspa-hub, /evm onto the chosen node; nginx -t; reload
#   ./deploy.sh files                stage app.js + index.html (new ?v=), upload, install; reset llm-jobs.json
#   ./deploy.sh census               (optional) point the peer census at the t12 node's log
#   ./deploy.sh verify               curl / wRPC / DB checks against the public site
#   ./deploy.sh all                  preflight -> confirm -> backup -> db -> units -> nginx -> files -> verify
#   ./deploy.sh rollback [TS]        undo one deploy (TS defaults to the last one this script recorded)
#
# It never touches a kaspad unit, an appdir, the MTP ledger, the faucet or any other host.
set -euo pipefail

HOST=${HOST:-root@169.58.232.113}
SSH_KEY=${SSH_KEY:-$HOME/.ssh/claude_key}
SSH=(ssh -o IdentitiesOnly=yes -i "$SSH_KEY" -o ConnectTimeout=20 "$HOST")
SCP=(scp -q -o IdentitiesOnly=yes -i "$SSH_KEY" -o ConnectTimeout=20)
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
STATE="$HERE/.last-deploy-ts"
SITE_URL=${SITE_URL:-https://misakascan.com}

# The chain: the release the fleet runs, read from the deploy kit's fleet.env (EXPECT_GENESIS /
# EXPECT_FP, pinned from the SHIPPING commit's probe — docs/t12-rcore-launch-checklist.md §4) unless
# GENESIS / EXPECT_FP are given. There is no default genesis any more: the one this script used to
# default to (f6cc9576…, the private t12's) is in FORBIDDEN_GENESIS, and so is every retired or drill
# genesis — the script refuses them.
FLEET_ENV=${FLEET_ENV:-$HERE/../t12-deploy-kit/fleet.env}
FORBIDDEN_GENESIS=""; DRILL_GENESES=""
if [ -f "$FLEET_ENV" ]; then
  # shellcheck source=../t12-deploy-kit/fleet.env.example
  FLEET_GENESIS=$(. "$FLEET_ENV" && echo "$EXPECT_GENESIS"); FLEET_FP=$(. "$FLEET_ENV" && echo "$EXPECT_FP")
  FORBIDDEN_GENESIS=$(. "$FLEET_ENV" && echo "$FORBIDDEN_GENESIS"); DRILL_GENESES=$(. "$FLEET_ENV" && echo "${DRILL_GENESES:-}")
  GENESIS=${GENESIS:-$FLEET_GENESIS}; EXPECT_FP=${EXPECT_FP:-$FLEET_FP}
elif [ -f "$HERE/../t12-deploy-kit/fleet.env.example" ]; then   # the forbidden list only; no genesis from a template
  FORBIDDEN_GENESIS=$(. "$HERE/../t12-deploy-kit/fleet.env.example" && echo "$FORBIDDEN_GENESIS")
fi
GENESIS=${GENESIS:-}
EXPECT_FP=${EXPECT_FP:-}
[ "$GENESIS" != __FILL_ME__ ] || GENESIS=""
[ "$EXPECT_FP" != __FILL_ME__ ] || EXPECT_FP=""

# The node the explorer reads. Default: the public node on .113 itself (misaka-t12-node:
# gRPC 26312, JSON wRPC 26314, EVM 8545) — which must already be ON the new genesis. To keep reading
# the 5.104 follower through the t12p tunnel instead: NODE_GRPC=127.0.0.1:36312
# NODE_JSON=127.0.0.1:36314 NODE_EVM=127.0.0.1:36545.
NODE_GRPC=${NODE_GRPC:-127.0.0.1:26312}
NODE_JSON=${NODE_JSON:-127.0.0.1:26314}
NODE_EVM=${NODE_EVM:-127.0.0.1:8545}
# /kaspa-hub was ibm's node via the vantage tunnel (.113:28014 -> ibm 127.0.0.1:26314), but ibm's t12
# node answers JSON on 26324, so 28014 is dead. Default: the same node as /kaspa. Set HUB_JSON=127.0.0.1:28014
# only after ibm's /etc/misaka-explorer/vantage-tunnel.env says REMOTE_RPC_PORT=<its t12 JSON port>.
HUB_JSON=${HUB_JSON:-$NODE_JSON}
NODE_LOG=${NODE_LOG:-/root/.t12r-b6/misaka-testnet-12/logs/rusty-kaspa.log}   # the deploy kit's b6 appdir (APPDIR_PREFIX /root/.t12r-b)

# One deploy = one TS. `backup` / `all` mint it and record "TS BUSTER" in .last-deploy-ts; every
# later step run on its own reuses that record, so backups, drop-ins and rollback all name one deploy.
STATE_BUSTER=
if [ -z "${TS:-}" ]; then
  case "${1:-plan}" in
    plan|preflight|backup|all) TS=$(date -u +%Y%m%dT%H%M%SZ) ;;
    *) if [ -f "$STATE" ]; then read -r TS STATE_BUSTER < "$STATE" || true; fi
       TS=${TS:-$(date -u +%Y%m%dT%H%M%SZ)} ;;
  esac
fi
BUSTER=${BUSTER:-${STATE_BUSTER:-t12g-$TS}}
NEW_DB=${NEW_DB:-kaspa_t12}
NOTICE=${NOTICE:-strip}        # strip: drop index.html's testnet-11 operator notice | keep
RESET_JOBS=${RESET_JOBS:-1}    # 1: replace the testnet-11 llm-jobs.json with an empty feed
SITE=/var/www/misaka-explorer
NGX=/etc/nginx/sites-enabled/misakascan
BK=/root/misakascan-config-backups

die(){ echo "deploy.sh: $*" >&2; exit 1; }
say(){ printf '\n== %s\n' "$*"; }
sha(){ shasum -a 256 "$1" | awk '{print $1}'; }
remote_env(){
  printf 'env'
  local v
  for v in GENESIS EXPECT_FP TS NEW_DB NODE_GRPC NODE_JSON NODE_EVM HUB_JSON NODE_LOG SITE NGX BK RESET_JOBS; do
    printf ' %s=%q' "$v" "${!v}"
  done
}
remote(){ "${SSH[@]}" "$(remote_env) bash -s"; }   # script on stdin
case "$NEW_DB" in *[!a-z0-9_]*|"") die "NEW_DB must be [a-z0-9_]+";; esac
[[ "$GENESIS" =~ ^[0-9a-f]{128}$ ]] || die "GENESIS must be 128 hex (fill the deploy kit's fleet.env EXPECT_GENESIS, or pass GENESIS=)"
for g in $FORBIDDEN_GENESIS $DRILL_GENESES; do [ "$GENESIS" != "$g" ] || die "GENESIS ${GENESIS:0:16}… is a retired/private/drill genesis (fleet.env FORBIDDEN_GENESIS / DRILL_GENESES)"; done
[[ -z "$EXPECT_FP" || "$EXPECT_FP" =~ ^[0-9a-f]{64}$ ]] || die "EXPECT_FP must be 64 hex"

plan(){
  cat <<EOF
host            $HOST
genesis         ${GENESIS:0:16}…
expect fp       ${EXPECT_FP:-<unset — required by 'all'>}
explorer node   gRPC $NODE_GRPC · JSON $NODE_JSON · EVM $NODE_EVM · hub $HUB_JSON
new database    $NEW_DB  (kaspa_t11 and kaspa_t12p are left as they are)
cache buster    app.js?v=$BUSTER
index notice    $NOTICE
llm-jobs reset  $RESET_JOBS
timestamp       $TS
EOF
}

preflight(){
  say "local files"
  for f in app.js index.html probe.mjs; do [ -f "$HERE/$f" ] || die "missing $HERE/$f"; done
  node --check "$HERE/app.js" || die "app.js does not parse"
  [ "$(grep -c '/app.js?v=' "$HERE/index.html")" = 1 ] || die "index.html must reference /app.js?v= exactly once"
  grep -q 'ebf44d0aa09ff7d1' "$HERE/app.js" || die "app.js lacks the 8k class row"
  echo "app.js $(sha "$HERE/app.js") ok"
  say "remote (read-only)"
  remote <<'EOF'
set -uo pipefail
df -h / | tail -1
free -h | sed -n 2p
printf 'misaka-t12-node: %s\n' "$(systemctl is-active misaka-t12-node)"
if [ -f "$NODE_LOG" ]; then
  fp=$(grep -a 'Consensus params fingerprint:' "$NODE_LOG" | tail -1 | sed -E 's/.*fingerprint: ([0-9a-f]{64}).*/\1/')
  echo "node log fingerprint: ${fp:-<none>}"
  if [ -n "$EXPECT_FP" ] && [ "$fp" != "$EXPECT_FP" ]; then echo "PREFLIGHT FAIL: node log says ${fp:-nothing}, expected $EXPECT_FP"; rc=1; fi
else
  echo "node log $NODE_LOG absent (fine when NODE_GRPC is the tunnel; verify checks the fingerprint over wRPC)"
fi
cd /opt/explorer-stack/kaspa-db-filler
/opt/venv-filler/bin/python - "$NODE_GRPC" "$GENESIS" <<'PY'
import asyncio, sys
from kaspad.KaspadMultiClient import KaspadMultiClient
host, genesis = sys.argv[1], sys.argv[2]
async def main():
    c = KaspadMultiClient([host])
    await c.initialize_all()
    k = c.kaspads[0]
    if not (k.is_synced and k.is_utxo_indexed):
        print(f"PREFLIGHT FAIL: {host} synced={k.is_synced} utxoindex={k.is_utxo_indexed}")
        return 3
    d = (await c.request("getBlockDagInfoRequest", {}))["getBlockDagInfoResponse"]
    b = (await c.request("getBlockRequest", {"hash": genesis, "includeTransactions": False})).get("getBlockResponse", {})
    found = "block" in b
    print(f"{host}: network={d.get('networkName')} blocks={d.get('blockCount')} daa={d.get('virtualDaaScore')} genesis {genesis[:16]} found={found}")
    if not found or d.get("networkName") != "misaka-testnet-12":
        print("PREFLIGHT FAIL: that node is not on the new testnet-12 genesis")
        return 4
    return 0
try:
    sys.exit(asyncio.run(main()))
except Exception as e:
    print(f"PREFLIGHT FAIL: {host}: {e!r}")
    sys.exit(5)
PY
[ $? -eq 0 ] || rc=1
cd /tmp
if [ -n "$(sudo -u postgres psql -Atc "select 1 from pg_database where datname='$NEW_DB'")" ]; then
  echo "PREFLIGHT FAIL: database $NEW_DB already exists (pick NEW_DB=… or rename it aside first)"; rc=1
fi
echo "--- current explorer wiring"
grep -nE 'server 127\.0\.0\.1:|proxy_pass http://127\.0\.0\.1:(2|3|8)[0-9]{3}' "$NGX" | sed 's/^/nginx: /'
for u in kaspa-t11-db-filler kaspa-t11-rest-server; do
  printf '%s: %s; drop-ins: %s\n' "$u" "$(systemctl is-active $u)" "$(ls /etc/systemd/system/$u.service.d 2>/dev/null | tr '\n' ' ')"
  # The deploy kit's other wiring (install-113.sh explorer-apply) leaves zz-t12r.conf, which sorts after
  # this script's t12g.conf and would silently win: the two are exclusive (DEPLOY.md §9).
  if [ -e "/etc/systemd/system/$u.service.d/zz-t12r.conf" ]; then
    echo "PREFLIGHT FAIL: $u carries install-113.sh explorer-apply's zz-t12r.conf — the explorer is already wired the other way (DEPLOY.md §9); run install-113.sh explorer-rollback first, or keep that wiring"; rc=1
  fi
done
grep -o '/app.js?v=[^"]*' "$SITE/index.html" | sed 's/^/live index: /'
exit ${rc:-0}
EOF
}

backup(){
  say "backup ($TS)"
  echo "$TS $BUSTER" > "$STATE"
  remote <<'EOF'
set -euo pipefail
install -d -m 0755 "$BK"
cd "$SITE"
cp -a app.js "app.js.bak-before-t12g-$TS"
cp -a index.html "index.html.bak-before-t12g-$TS"
[ -f llm-jobs.json ] && cp -a llm-jobs.json "llm-jobs.json.bak-before-t12g-$TS"
cp -a "$NGX" "/root/misakascan.nginx.bak-before-t12g-$TS"
tar -C /etc/systemd/system -czf "$BK/units-before-t12g-$TS.tgz" \
  kaspa-t11-db-filler.service.d kaspa-t11-rest-server.service.d
cd /tmp
if [ -n "$(sudo -u postgres psql -Atc "select 1 from pg_database where datname='kaspa_t12p'")" ]; then
  sudo -u postgres pg_dump -Fc kaspa_t12p > "$BK/kaspa_t12p-before-t12g-$TS.dump"
fi
ls -l "$SITE"/*bak-before-t12g-"$TS" "/root/misakascan.nginx.bak-before-t12g-$TS" "$BK"/*"$TS"*
EOF
}

db(){
  say "database $NEW_DB, cursor seeded at genesis ${GENESIS:0:16}…"
  remote <<'EOF'
set -euo pipefail
cd /tmp
[ -z "$(sudo -u postgres psql -Atc "select 1 from pg_database where datname='$NEW_DB'")" ] || { echo "refusing: $NEW_DB exists"; exit 1; }
sudo -u postgres psql -v ON_ERROR_STOP=1 -qc "CREATE DATABASE $NEW_DB OWNER kaspa;"
base=$(sed -n 's/^Environment=SQL_URI=//p' /etc/systemd/system/kaspa-t11-db-filler.service | head -1)
[ -n "$base" ] || { echo "no SQL_URI in the filler's base unit"; exit 1; }
uri=$(printf '%s' "$base" | sed -E "s#/[A-Za-z0-9_]+\$#/$NEW_DB#")
cd /opt/explorer-stack/kaspa-db-filler
SQL_URI="$uri" /opt/venv-filler/bin/python -c 'import models.Block, models.Transaction, models.TxAddrMapping, models.Variable
from dbsession import create_all
create_all(drop=False)'
cd /tmp
# The filler starts at the TIP when the cursor is empty (main.py: "if there is nothing in the db, just get
# latest block"). Seeding the legacy cursor with the genesis makes it index from block 0, as 5f did.
sudo -u postgres psql -d "$NEW_DB" -v ON_ERROR_STOP=1 -qc \
  "INSERT INTO vars(key, value) VALUES ('vspc_last_start_hash', '$GENESIS');"
tables=$(sudo -u postgres psql -d "$NEW_DB" -Atc "select string_agg(c.relname, ' ' order by c.relname) from pg_class c join pg_namespace n on n.oid=c.relnamespace where n.nspname='public' and c.relkind='r'")
echo "tables: $tables"
[ "$tables" = "blocks transactions transactions_inputs transactions_outputs tx_id_address_mapping vars" ] || { echo "unexpected table set"; exit 1; }
sudo -u postgres psql -d "$NEW_DB" -Atc "select key, left(value, 16) from vars"
EOF
}

units(){
  say "filler + REST -> $NODE_GRPC / $NEW_DB"
  remote <<'EOF'
set -euo pipefail
for u in kaspa-t11-db-filler kaspa-t11-rest-server; do
  [ ! -e "/etc/systemd/system/$u.service.d/zz-t12r.conf" ] || { echo "refusing: $u carries install-113.sh explorer-apply's zz-t12r.conf (it would override t12g.conf) — one wiring only, DEPLOY.md §9"; exit 1; }
done
systemctl stop kaspa-t11-db-filler
for u in kaspa-t11-db-filler kaspa-t11-rest-server; do
  d=/etc/systemd/system/$u.service.d
  install -d -m 0755 "$d"
  base=$(sed -n 's/^Environment=SQL_URI=//p' /etc/systemd/system/$u.service | head -1)
  [ -n "$base" ] || { echo "no SQL_URI in $u"; exit 1; }
  uri=$(printf '%s' "$base" | sed -E "s#/[A-Za-z0-9_]+\$#/$NEW_DB#")
  # The private-t12 view's override is moved aside, not deleted: rollback puts it back.
  [ -f "$d/t12p.conf" ] && mv "$d/t12p.conf" "$BK/$u.t12p.conf.moved-$TS"
  cat > "$d/t12g.conf" <<CONF
# testnet-12, genesis ${GENESIS:0:16}… — misakascan deploy.sh $TS. Rollback: deploy.sh rollback $TS
[Service]
Environment=KASPAD_HOST_1=$NODE_GRPC
Environment=SQL_URI=$uri
CONF
  chmod 0644 "$d/t12g.conf"
done
systemctl daemon-reload
systemctl restart kaspa-t11-rest-server
systemctl start kaspa-t11-db-filler
for u in kaspa-t11-db-filler kaspa-t11-rest-server; do
  systemctl show "$u" -p ActiveState -p Environment --value | sed -E 's#(//[^:]+:)[^@]+@#\1***@#g; s/^/'"$u"': /'
done
sleep 30
journalctl -u kaspa-t11-db-filler --since "-2 min" --no-pager | grep -E 'Start hash|Exception|Error' | tail -5 || true
systemctl is-active --quiet kaspa-t11-db-filler || { echo "filler is not running"; exit 1; }
EOF
}

nginx_step(){
  say "nginx: /kaspa,/kaspa-seed -> $NODE_JSON · /kaspa-hub -> $HUB_JSON · /evm -> $NODE_EVM"
  remote <<'EOF'
set -euo pipefail
[ -f "/root/misakascan.nginx.bak-before-t12g-$TS" ] || cp -a "$NGX" "/root/misakascan.nginx.bak-before-t12g-$TS"
python3 - "$NGX" "$NODE_JSON" "$HUB_JSON" "$NODE_EVM" "$GENESIS" <<'PY'
import re, sys
path, json_, hub, evm, genesis = sys.argv[1:6]
s = open(path).read()
note = f"testnet-12 genesis {genesis[:16]}"
def in_block(head, pattern, repl):
    """Edit exactly one line inside the block that opens with `head` (up to its closing brace)."""
    global s
    if s.count(head) != 1:
        sys.exit(f"nginx edit: {head!r} occurs {s.count(head)} times — refusing")
    a = s.index(head)
    b = s.index("\n    }", a) if head.startswith("    location") else s.index("\n}", a)
    body, n = re.subn(pattern, repl, s[a:b], flags=re.M)
    if n != 1:
        sys.exit(f"nginx edit: {pattern!r} matched {n} times in {head!r} — refusing")
    s = s[:a] + body + s[b:]
in_block("upstream misaka_json {", r"^(\s*server )127\.0\.0\.1:\d+( max_fails=2 fail_timeout=10s;).*$",
         rf"\g<1>{json_}\g<2>   # {note} wRPC-JSON")
in_block("    location /kaspa-hub {", r"^(\s*proxy_pass http://)127\.0\.0\.1:\d+(;).*$",
         rf"\g<1>{hub}\g<2>   # {note} hub vantage")
in_block("    location = /evm {", r"^(\s*proxy_pass http://)127\.0\.0\.1:\d+(/;).*$",
         rf"\g<1>{evm}\g<2>   # {note} EVM")
open(path, "w").write(s)
PY
grep -nE 'server 127\.0\.0\.1:|proxy_pass http://127\.0\.0\.1:(2|3|8)[0-9]{3}' "$NGX"
if nginx -t; then
  systemctl reload nginx
else
  echo "nginx -t failed — restoring the backup"; cp -a "/root/misakascan.nginx.bak-before-t12g-$TS" "$NGX"; nginx -t; exit 1
fi
EOF
}

files(){
  local stage="$HERE/stage-$TS"
  say "stage $stage (?v=$BUSTER)"
  mkdir -p "$stage"
  cp "$HERE/app.js" "$HERE/index.html" "$stage/"
  node --check "$stage/app.js"
  BUSTER="$BUSTER" perl -pi -e 's{/app\.js\?v=[^"]*}{/app.js?v=$ENV{BUSTER}}g' "$stage/index.html"
  [ "$(grep -c "/app.js?v=$BUSTER\"" "$stage/index.html")" = 1 ] || die "buster not applied exactly once"
  case "$NOTICE" in
    strip) perl -0pi -e 's{<details class="upgrade-note".*?</details>\s*}{}s' "$stage/index.html" ;;
    keep)  : ;;
    *)     die "NOTICE must be strip|keep" ;;
  esac
  if grep -q 'testnet11-6701-upgrade-announcement' "$stage/index.html" && [ "$NOTICE" != keep ]; then
    die "index.html still carries the testnet-11 operator notice"
  fi
  printf '{"rows": [], "fp_rows": [], "note": "reset for testnet-12 genesis %s at %s"}\n' "${GENESIS:0:16}" "$TS" > "$stage/llm-jobs.json"
  sha "$stage/app.js" > "$stage/app.js.sha256"
  echo "app.js $(cat "$stage/app.js.sha256")"
  say "upload + install (app.js first, then index.html)"
  "${SSH[@]}" "install -d -m 0700 /tmp/misakascan-t12g-$TS"
  "${SCP[@]}" "$stage/app.js" "$stage/index.html" "$stage/llm-jobs.json" "$HOST:/tmp/misakascan-t12g-$TS/"
  remote <<'EOF'
set -euo pipefail
src=/tmp/misakascan-t12g-$TS
install -m 0644 -o root -g root "$src/app.js" "$SITE/app.js"
install -m 0644 -o root -g root "$src/index.html" "$SITE/index.html"
if [ "$RESET_JOBS" = 1 ]; then install -m 0644 -o root -g root "$src/llm-jobs.json" "$SITE/llm-jobs.json"; fi
sha256sum "$SITE/app.js" "$SITE/index.html"
grep -o '/app.js?v=[^"]*' "$SITE/index.html"
EOF
}

census(){
  say "peer census -> $NODE_LOG"
  remote <<'EOF'
set -euo pipefail
[ -f "$NODE_LOG" ] || { echo "no $NODE_LOG on this host — census left alone"; exit 0; }
d=/etc/systemd/system/misakascan-peer-census.service.d
install -d -m 0755 "$d"
printf '[Service]\nEnvironment=CENSUS_LOG=%s\n' "$NODE_LOG" > "$d/t12g.conf"
systemctl daemon-reload
EOF
}

verify(){
  local ts=${1:-$TS} stage fail=0 idx
  stage="$HERE/stage-$ts"
  [ "$ts" = "$TS" ] || BUSTER=t12g-$ts
  say "public site"
  idx=$(curl -fsS -m 20 "$SITE_URL/" | grep -o 'app.js?v=[^"]*' || true)
  echo "index: $idx"; [ "$idx" = "app.js?v=$BUSTER" ] || { echo "VERIFY FAIL: expected app.js?v=$BUSTER"; fail=1; }
  if [ -f "$stage/app.js" ]; then
    local got; got=$(curl -fsS -m 30 "$SITE_URL/app.js" | shasum -a 256 | awk '{print $1}')
    [ "$got" = "$(sha "$stage/app.js")" ] && echo "app.js sha256 matches the staged file" || { echo "VERIFY FAIL: app.js sha256 $got"; fail=1; }
  fi
  curl -fsSI -m 20 "$SITE_URL/app.js" | grep -iE '^(HTTP|etag|cache-control)' || fail=1
  curl -fsS -m 20 "$SITE_URL/info/network" | grep -q '"networkName":"misaka-testnet-12"' \
    && echo "REST /info/network: misaka-testnet-12" || { echo "VERIFY FAIL: REST network"; fail=1; }
  curl -fsS -m 20 "$SITE_URL/info/health" | grep -q '"isSynced":true' \
    && echo "REST /info/health: synced" || { echo "VERIFY FAIL: REST health"; fail=1; }
  curl -fsS -m 20 "$SITE_URL/info/stake-bonds" >/dev/null && echo "REST /info/stake-bonds: 200" || fail=1
  curl -fsS -m 20 -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}' \
    "$SITE_URL/evm" && echo || { echo "VERIFY FAIL: /evm"; fail=1; }
  say "wRPC through nginx"
  local path out
  for path in /kaspa /kaspa-seed /kaspa-hub; do
    out=$(node "$HERE/probe.mjs" "wss://${SITE_URL#https://}$path" "$GENESIS" || true)
    GEN_OK=$(printf '%s' "$out" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{try{const o=JSON.parse(s);console.log([o.dag&&o.dag.network,o.genesisBlock&&o.genesisBlock.found,o.nodeStatus&&o.nodeStatus.consensusParamsId].join(" "))}catch{console.log("unparsable")}})')
    echo "$path: $GEN_OK"
    case "$GEN_OK" in
      "testnet-12 true "*) ;;
      *) echo "VERIFY FAIL: $path is not on genesis ${GENESIS:0:16}"; fail=1 ;;
    esac
    if [ -n "$EXPECT_FP" ] && [ "${GEN_OK##* }" != "$EXPECT_FP" ]; then echo "VERIFY FAIL: $path fingerprint"; fail=1; fi
  done
  say "indexer"
  remote <<'EOF' || fail=1
set -uo pipefail
cd /tmp
sudo -u postgres psql -d "$NEW_DB" -Atc "select 'genesis rows', count(*) from blocks where hash='$GENESIS'"
sudo -u postgres psql -d "$NEW_DB" -Atc "select 'blocks', count(*), 'max blue', max(blue_score) from blocks"
journalctl -u kaspa-t11-db-filler --since "-10 min" --no-pager | grep -E 'Start hash|Exception' | tail -3
[ "$(sudo -u postgres psql -d "$NEW_DB" -Atc "select count(*) from blocks where hash='$GENESIS'")" = 1 ]
EOF
  [ $fail = 0 ] && echo "VERIFY OK" || die "verification failed — see above; './deploy.sh rollback $ts' undoes it"
}

rollback(){
  local ts=${1:-}
  if [ -z "$ts" ] && [ -f "$STATE" ]; then read -r ts _ < "$STATE" || true; fi
  [ -n "$ts" ] || die "rollback needs the deploy's TS"
  say "rollback $ts"
  TS="$ts" remote <<'EOF'
set -euo pipefail
cd "$SITE"
for f in app.js index.html llm-jobs.json; do
  [ -f "$f.bak-before-t12g-$TS" ] && cp -a "$f.bak-before-t12g-$TS" "$f"
done
if [ -f "/root/misakascan.nginx.bak-before-t12g-$TS" ]; then
  cp -a "/root/misakascan.nginx.bak-before-t12g-$TS" "$NGX"
  nginx -t && systemctl reload nginx
fi
for u in kaspa-t11-db-filler kaspa-t11-rest-server; do
  d=/etc/systemd/system/$u.service.d
  rm -f "$d/t12g.conf"
  [ -f "$BK/$u.t12p.conf.moved-$TS" ] && mv "$BK/$u.t12p.conf.moved-$TS" "$d/t12p.conf"
done
rm -f /etc/systemd/system/misakascan-peer-census.service.d/t12g.conf
systemctl daemon-reload
systemctl restart kaspa-t11-rest-server kaspa-t11-db-filler
echo "rolled back to the state before $TS. $NEW_DB is left in place (ALTER DATABASE $NEW_DB RENAME TO ${NEW_DB}_abandoned_$TS to park it)."
EOF
}

cmd=${1:-plan}; shift || true
case "$cmd" in
  plan)      plan ;;
  preflight) plan; preflight ;;
  backup)    backup ;;
  db)        db ;;
  units)     units ;;
  nginx)     nginx_step ;;
  files)     files ;;
  census)    census ;;
  verify)    verify "${1:-$TS}" ;;
  rollback)  rollback "${1:-}" ;;
  all)
    [ -n "$EXPECT_FP" ] || die "set EXPECT_FP to the release build's params fingerprint first"
    plan; preflight
    read -r -p "type DEPLOY to change $HOST: " ok; [ "$ok" = DEPLOY ] || die "aborted"
    backup; db; units; nginx_step; files
    [ "${CENSUS:-0}" = 1 ] && census
    echo "waiting 60 s for the filler's first batch"; sleep 60
    verify "$TS" ;;
  *) die "unknown step $cmd" ;;
esac
