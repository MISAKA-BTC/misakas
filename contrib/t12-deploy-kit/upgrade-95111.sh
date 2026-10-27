#!/usr/bin/env bash
# deploy-t12/upgrade-95111.sh — O-SEED3 (lane DEPLOY-PREP, 17,400): upgrade seeder3's backing node `kaspad-t12` on 95.111.236.186
# (hostname vmi3225101) to the release. Run ON THE MAC; it ssh's to 95.111 only. 95.111 is NOT a kit host: no bond, no seat, no
# install-*.sh. It also runs t10 misaka-validator-b, ollama, docker web apps (highroller-web, roulette-webgl, pm2), nginx,
# palw-liveness-watch and misaka-dnsseeder-t12 (seeder3) — this script touches NONE of them, only kaspad-t12's launch script.
#
# Measured read-only 2026-10-27 21:11 CET:
#   unit      /etc/systemd/system/kaspad-t12.service  ExecStart=/root/t12-sync/node.sh  (WorkingDirectory /root/t12-sync)
#   drop-ins  system.control/…/50-MemoryMax.conf (9663676416) + 50-MemorySwapMax.conf (0) + kaspad-t12.service.d/zz-memory.conf (9G)
#   binary    /root/t12-rel/247ade1f90b6/bin/kaspad  sha c57c8c13…  (the OLD release; fleet runs ddf5485d88df)
#   appdir    /root/.t12-247ade1f90b6 (7.6 GB)  listen 0.0.0.0:26311, rpc 26312/26313/26314, evm 8545
#   args      --testnet --netsuffix=12 --appdir=… --yes --listen=0.0.0.0:26311 --rpclisten=127.0.0.1:26312 --rpclisten-borsh=127.0.0.1:26313
#             --rpclisten-json=127.0.0.1:26314 --evm-rpc-listen=127.0.0.1:8545 --utxoindex --unsaferpc --disable-upnp --nodnsseed
#             --addpeer=169.58.39.220:26311 --addpeer=169.58.39.220:26321 --addpeer=169.58.232.113:26311 --ram-scale=0.3 --min-disk-free-percent=5
#   memory    cgroup anon 7.68 GB, file 0.40 GB, memory.events max=18,033 (pinned at the 9G cap), oom_kill 0; host 11.96 GB, 6 cores
#
# THE ARGS DIFF IS ONLY THE BINARY PATH. The appdir keeps its old name (/root/.t12-247ade1f90b6): the chain data is the same chain;
# renaming it would be a resync. MemoryMax is left at 9G (raise only by explicit decision: MEMMAX_GIB=10 below; the host's other
# services share 12 GB). The new node.sh checks the binary's sha.
#
#   ./upgrade-95111.sh preflight            read-only
#   CONFIRM=yes ./upgrade-95111.sh stage    $INCOMING/$REV/kaspad -> /root/t12-rel/$REV/bin/kaspad (sha), node.sh.new-$REV written beside
#   CONFIRM=yes ./upgrade-95111.sh swap     backup node.sh -> node.sh.bak-247ade1f90b6, install node.sh.new, restart, verify fp/sync
#   ./upgrade-95111.sh verify               read-only: running exe sha, fp line, DAA advancing, peers
#   CONFIRM=yes ./upgrade-95111.sh rollback node.sh.bak-247ade1f90b6 back, restart (BEFORE 17,400 only — after it the old binary
#                                           follows the old rules: stop the node instead, see G14-17400-ROLLOUT.md §5)
# Optional: MEMMAX_GIB=<n> with swap rewrites zz-memory.conf AND removes system.control's 50-MemoryMax.conf (a `systemctl
# set-property` drop-in; /etc/systemd/system.control takes precedence over /etc/systemd/system, so it alone would keep 9G).
# Both are backed up under /root/t12-rel/state/ and restored by rollback.
set -euo pipefail
KIT=$(cd "$(dirname "$0")" && pwd)
. "$KIT/fleet.env"
H=root@95.111.236.186
SSH=(ssh -o IdentitiesOnly=yes -i "$HOME/.ssh/claude_key" -o ConnectTimeout=10 -o BatchMode=yes "$H")
OLD_REV=247ade1f90b6
OLD_SHA=c57c8c13744aa29a8295b05bb7c495ef91bf1be4d5c755ecaa638e957fa403b8
UNIT=kaspad-t12
NODE_SH=/root/t12-sync/node.sh
APPDIR=/root/.t12-$OLD_REV
say() { printf '[%s] %s\n' "$(date -u +%H:%M:%S)" "$*"; }
die() { say "ABORT: $*"; exit 1; }
need_confirm() { [ "${CONFIRM:-}" = yes ] || die "this step WRITES on 95.111 — CONFIRM=yes"; }
[ "$REV" != __FILL_ME__ ] && [ "$KASPAD_SHA256" != __FILL_ME__ ] || die "fill REV / KASPAD_SHA256 in fleet.env"
case "$EXPECT_FP" in __FILL_ME__|*PLACEHOLDER*|"") die "EXPECT_FP is not filled (post re-arm value)";; esac

new_node_sh() { cat <<EOF
#!/usr/bin/env bash
# kaspad-t12 (seeder3's backing node, 95.111) — release $REV. Written by deploy-t12/upgrade-95111.sh on $(date -u +%Y%m%dT%H%M%SZ).
# Previous: $NODE_SH.bak-$OLD_REV (release $OLD_REV). Only the binary changed; the appdir keeps its name (same chain).
set -euo pipefail
BIN=/root/t12-rel/$REV/bin/kaspad
got=\$(sha256sum "\$BIN" | cut -d' ' -f1)
[ "\$got" = "$KASPAD_SHA256" ] || { echo "[kaspad-t12] \$BIN sha \$got != $KASPAD_SHA256 — refusing"; exit 78; }
exec "\$BIN" \\
  --testnet \\
  --netsuffix=12 \\
  --appdir=$APPDIR \\
  --yes \\
  --listen=0.0.0.0:26311 \\
  --rpclisten=127.0.0.1:26312 \\
  --rpclisten-borsh=127.0.0.1:26313 \\
  --rpclisten-json=127.0.0.1:26314 \\
  --evm-rpc-listen=127.0.0.1:8545 \\
  --utxoindex \\
  --unsaferpc \\
  --disable-upnp \\
  --nodnsseed \\
  --addpeer=169.58.39.220:26311 \\
  --addpeer=169.58.39.220:26321 \\
  --addpeer=169.58.232.113:26311 \\
  --ram-scale=0.3 \\
  --min-disk-free-percent=5
EOF
}

remote_verify() { # <since epoch>
    "${SSH[@]}" bash -s -- "$1" "$EXPECT_FP" "$KASPAD_SHA256" <<'EOF'
since=$1 fp=$2 sha=$3
p=$(systemctl show -p MainPID --value kaspad-t12); echo "active=$(systemctl is-active kaspad-t12) pid=$p NRestarts=$(systemctl show -p NRestarts --value kaspad-t12)"
exe=$(readlink /proc/$p/exe 2>/dev/null); echo "exe=$exe sha=$(sha256sum "$exe" 2>/dev/null | cut -c1-16)…"; [ "$(sha256sum "$exe" | cut -d' ' -f1)" = "$sha" ] && echo "SHA OK" || echo "SHA MISMATCH"
journalctl -u kaspad-t12 --since "@$since" -o cat --no-pager | grep -m1 'Consensus params fingerprint' | grep -q "$fp" && echo "FP OK" || echo "FP NOT SEEN (yet?)"
journalctl -u kaspad-t12 --since "@$since" -o cat --no-pager | grep -cE 'Deleting databases|WrongGenesis|WrongConsensusParams|panicked' | sed 's/^/bad lines: /'
journalctl -u kaspad-t12 --since "-5min" -o cat --no-pager | grep -oE 'DAA (score )?[0-9]{5}' | sed -n '1p;$p' | tr '\n' ' '; echo
grep -E '^(anon|file) ' /sys/fs/cgroup/system.slice/kaspad-t12.service/memory.stat | tr '\n' ' '; grep oom_kill /sys/fs/cgroup/system.slice/kaspad-t12.service/memory.events
ss -tnH state established '( sport = :26311 or dport = :26311 )' | wc -l | sed 's/^/p2p established: /'
EOF
}

case "${1:-}" in
preflight)
    "${SSH[@]}" bash -s -- "$OLD_REV" "$INCOMING/$REV" "$OLD_SHA" <<'EOF'
old=$1 inc=$2 oldsha=$3
hostname; systemctl show kaspad-t12 -p ActiveState,MainPID,MemoryMax,MemoryCurrent,NRestarts | tr '\n' ' '; echo
p=$(systemctl show -p MainPID --value kaspad-t12); readlink /proc/$p/exe; sha256sum "$(readlink /proc/$p/exe)" | cut -c1-16
[ "$(sha256sum "$(readlink /proc/$p/exe)" | cut -d' ' -f1)" = "$oldsha" ] || echo "WARN: the running binary is not $old's — re-measure before swap"
ls -l "$inc" 2>/dev/null || echo "no $inc yet — ./distribute-from-mac.sh 95111"
df -h / | tail -1; free -m | sed -n 2p
systemctl is-active misaka-dnsseeder-t12 misaka-validator-b ollama docker palw-liveness-watch | paste -sd' '
EOF
    ;;
stage)
    need_confirm
    "${SSH[@]}" bash -s -- "$INCOMING/$REV" "/root/t12-rel/$REV/bin" "$KASPAD_SHA256" <<'EOF'
set -e; inc=$1 bin=$2 sha=$3
[ "$(sha256sum "$inc/kaspad" | cut -d' ' -f1)" = "$sha" ] || { echo "incoming kaspad sha mismatch"; exit 1; }
mkdir -p "$bin"; install -m 0755 "$inc/kaspad" "$bin/kaspad.tmp"; mv -f "$bin/kaspad.tmp" "$bin/kaspad"
[ "$(sha256sum "$bin/kaspad" | cut -d' ' -f1)" = "$sha" ] && echo "staged $bin/kaspad"
"$bin/kaspad" --help >/dev/null 2>&1 && echo "--help OK"
EOF
    new_node_sh | "${SSH[@]}" "cat > $NODE_SH.new-$REV && chmod 0755 $NODE_SH.new-$REV && diff $NODE_SH $NODE_SH.new-$REV || true"
    say "staged; the diff above must show only the header, the sha check and BIN (the appdir keeps its name)" ;;
swap)
    need_confirm
    since=$(date +%s)
    "${SSH[@]}" bash -s -- "$NODE_SH" "$REV" "$OLD_REV" "${MEMMAX_GIB:-}" <<'EOF'
set -e; n=$1 rev=$2 old=$3 mm=$4
test -x "$n.new-$rev"
[ -f "$n.bak-$old" ] || cp -p "$n" "$n.bak-$old"
if [ -n "$mm" ]; then
  mkdir -p /root/t12-rel/state; cp -a /etc/systemd/system.control/kaspad-t12.service.d /root/t12-rel/state/kaspad-t12.control.bak-$old 2>/dev/null || true
  cp -p /etc/systemd/system/kaspad-t12.service.d/zz-memory.conf /root/t12-rel/state/zz-memory.conf.bak-$old
  rm -f /etc/systemd/system.control/kaspad-t12.service.d/50-MemoryMax.conf
  printf '[Service]\nMemoryMax=%sG\nMemorySwapMax=0\n' "$mm" > /etc/systemd/system/kaspad-t12.service.d/zz-memory.conf
  systemctl daemon-reload
fi
systemctl stop kaspad-t12
install -m 0755 "$n.new-$rev" "$n"
systemctl start kaspad-t12
echo "restarted: $(systemctl show -p MemoryMax --value kaspad-t12) MemoryMax"
EOF
    say "waiting 120 s for the fp line and IBD catch-up"; sleep 120
    remote_verify "$since" ;;
verify) remote_verify "$(( $(date +%s) - 3600 ))" ;;
rollback)
    need_confirm
    "${SSH[@]}" bash -s -- "$NODE_SH" "$OLD_REV" <<'EOF'
set -e; n=$1 old=$2
[ -f "$n.bak-$old" ] || { echo "no $n.bak-$old"; exit 1; }
systemctl stop kaspad-t12; cp -p "$n.bak-$old" "$n"
if [ -f /root/t12-rel/state/zz-memory.conf.bak-$old ]; then
  cp -p /root/t12-rel/state/zz-memory.conf.bak-$old /etc/systemd/system/kaspad-t12.service.d/zz-memory.conf
  [ -d /root/t12-rel/state/kaspad-t12.control.bak-$old ] && cp -a /root/t12-rel/state/kaspad-t12.control.bak-$old/. /etc/systemd/system.control/kaspad-t12.service.d/
  systemctl daemon-reload
fi
systemctl start kaspad-t12; echo "rolled back to $old"
EOF
    ;;
*) sed -n '2,28p' "$0"; exit 2 ;;
esac
