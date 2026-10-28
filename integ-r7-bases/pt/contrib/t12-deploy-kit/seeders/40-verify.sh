#!/usr/bin/env bash
# deploy-t12/seeders/40-verify.sh — READ-ONLY. Is each seeder running the expected bytes with the
# t12 flags, has it refreshed SINCE the given moment (evidence must postdate the action), does it
# answer over UDP and TCP, and does every IP it hands out accept TCP on :26311 from here?
#
#   ./40-verify.sh                          # all four, evidence from the last 2 minutes
#   ./40-verify.sh seeder3                  # one
#   ./40-verify.sh seeder3 <epoch> <sha256> # as called by 30-swap.sh / 50-rollback.sh
#
# Exit 0 only if every checked seeder passes. "Advertises an IP whose :26311 is closed" is a WARN,
# not a FAIL: the seeder cannot tell a dead anchor from its own filtered egress (seeder3), and
# during the node switch the public node is legitimately down for a while.
source "$(dirname "$0")/lib-seeders.sh"

names="${1:-$(seeder_names)}"
since="${2:-$(( $(date +%s) - 120 ))}"
exp="${3:-$(expected_seeder_sha)}"
rc=0

for n in $names; do
  load_seeder "$n"
  echo "=== verify $S_NAME ($S_IP) expect sha ${exp:0:8}… evidence since @$since"
  if ! rsh "bash -s" <<EOF
set -u
fail=0
st=\$(systemctl is-active '$UNIT' || true)
[ "\$st" = active ] || { echo "  FAIL unit is \$st"; fail=1; }
echo "  NRestarts=\$(systemctl show '$UNIT' -p NRestarts --value)"
pid=\$(systemctl show '$UNIT' -p MainPID --value)
if [ -n "\$pid" ] && [ "\$pid" != 0 ]; then
  sha=\$(sha256sum /proc/\$pid/exe | cut -c1-64)
  [ "\$sha" = '$exp' ] && echo "  ok  running sha \${sha:0:8}…" || { echo "  FAIL running sha \$sha != $exp"; fail=1; }
  cmd=\$(tr '\0' ' ' </proc/\$pid/cmdline)
  echo "  cmdline: \$cmd"
  case "\$cmd" in *"--network-id testnet-12 "*) ;; *) echo "  FAIL not --network-id testnet-12"; fail=1 ;; esac
  case "\$cmd" in *"--anchors-only"*) ;; *) echo "  FAIL not --anchors-only"; fail=1 ;; esac
else
  echo "  FAIL no MainPID"; fail=1
fi
j=\$(journalctl -u '$UNIT' --since @$since --no-pager -o cat 2>/dev/null)
echo "\$j" | grep -E 'refreshed|not reachable|dial on|refresh failed' | tail -3 | cut -c1-200 | sed 's/^/  log: /'
echo "\$j" | grep -q 'verified peer set refreshed' || { echo "  FAIL no 'verified peer set refreshed' since @$since"; fail=1; }
if echo "\$j" | grep -q ':26411'; then echo "  FAIL probes :26411 (pre-regenesis port fallback)"; fail=1; fi
exit \$fail
EOF
  then rc=1; fi

  udp=$(dig @"$S_IP" +time=3 +tries=2 +short seeder-verify.misakascan.com A 2>/dev/null | grep -E '^[0-9.]+$' || true)
  tcp=$(dig @"$S_IP" +tcp +time=3 +tries=1 +short seeder-verify.misakascan.com A 2>/dev/null | grep -E '^[0-9.]+$' || true)
  echo "  answers udp: $(echo $udp)   tcp: $(echo $tcp)"
  if [ -z "$udp" ]; then echo "  FAIL empty UDP answer from $S_IP"; rc=1; fi
  if [ -z "$tcp" ]; then echo "  WARN empty TCP answer from $S_IP"; fi
  for ip in $udp; do
    if nc -z -G 3 "$ip" "$P2P_PORT" 2>/dev/null; then echo "  ok  $ip:$P2P_PORT accepts TCP"
    else echo "  WARN advertises $ip but $ip:$P2P_PORT is closed from this Mac"; fi
  done
  if [ "$S_DNS" != - ]; then
    echo "  public resolver $S_DNS: $(dig @8.8.8.8 +time=4 +tries=1 +short "$S_DNS" A 2>/dev/null | tr '\n' ' ')"
  fi
done

[ "$rc" = 0 ] && echo "VERIFY PASS" || echo "VERIFY FAIL"
exit "$rc"
