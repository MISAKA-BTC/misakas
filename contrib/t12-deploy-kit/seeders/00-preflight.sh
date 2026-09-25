#!/usr/bin/env bash
# deploy-t12/seeders/00-preflight.sh — READ-ONLY snapshot of all four seeders + DNS delegation.
# Safe to run any time (before, during and after the node switch). Writes nothing anywhere.
#
#   ./00-preflight.sh            # all four
#   ./00-preflight.sh seeder3    # one
source "$(dirname "$0")/lib-seeders.sh"

names="${1:-$(seeder_names)}"
for n in $names; do
  load_seeder "$n"
  echo "=== $S_NAME  ($S_SSH)  listen $S_IP:53  delegated=$S_DNS  public-node-host=$S_PUBLIC"
  rsh "UNIT=$UNIT BIN=$S_BIN CONF=$S_CONF bash -s" <<'EOF' || echo "  !! ssh/inspection failed"
systemctl show "$UNIT" -p ActiveState -p SubState -p NRestarts -p ExecMainStartTimestamp --no-pager | sed 's/^/  /'
pid=$(systemctl show "$UNIT" -p MainPID --value)
if [ -n "$pid" ] && [ "$pid" != 0 ]; then
  echo "  cmdline:     $(tr '\0' ' ' </proc/$pid/cmdline)"
  echo "  running sha: $(sha256sum /proc/$pid/exe | cut -c1-64)"
fi
echo "  on-disk sha: $(sha256sum "$BIN" | cut -c1-64)  ($BIN)"
grep -E '^MISAKA_SEEDER_ANCHORS=|^ExecStart=' "$CONF" | sed 's/^/  conf: /'
journalctl -u "$UNIT" --since '-3min' --no-pager -o cat 2>/dev/null \
  | grep -E 'refreshed|not reachable|dial on|refresh failed|:26411' | tail -3 | cut -c1-220 | sed 's/^/  log: /'
echo "  disk /: $(df -h / | awk 'NR==2{print $5" used, "$4" free"}')"
EOF
  echo "  answers from $S_IP (udp): $(dig @"$S_IP" +time=3 +tries=1 +short seeder1.misakascan.com A 2>&1 | tr '\n' ' ')"
  echo "  answers from $S_IP (tcp): $(dig @"$S_IP" +tcp +time=3 +tries=1 +short seeder1.misakascan.com A 2>&1 | tr '\n' ' ')"
done

echo "=== parent delegation (asked of the misakascan.com parent, not inferred from a resolver)"
for i in 1 2 3 4; do
  glue=$(dig @ns1.xdomain.ne.jp +norec "seeder$i.misakascan.com" A | awk '$1 ~ /^ns-seeder/ && $4=="A" {print $5}')
  echo "  seeder$i.misakascan.com -> ns-seeder$i A ${glue:-?}"
done

echo "=== public resolver view (TTL 30 s on the seeder side)"
for i in 1 2 3 4; do
  echo "  seeder$i: $(dig @8.8.8.8 +time=4 +tries=1 +short "seeder$i.misakascan.com" A 2>&1 | tr '\n' ' ')"
done

echo "=== anchors on :$P2P_PORT from this Mac"
for ip in ${LIVE_ANCHORS//,/ }; do
  if nc -z -G 3 "$ip" "$P2P_PORT" 2>/dev/null; then echo "  $ip:$P2P_PORT open"; else echo "  $ip:$P2P_PORT CLOSED"; fi
done
