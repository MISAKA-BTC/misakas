#!/usr/bin/env bash
# deploy-t12/seeders/60-join-check.sh — the END-TO-END seeder check, run AFTER the node switch:
# a throw-away node from the RELEASE kaspad, with NO --addpeer / --connect, must find the public
# chain through the four DNS names alone, handshake (network name + genesis + params id all agree)
# and IBD. This is the only check that proves "a user needs nothing but the build and --netsuffix=12".
#
# Runs on 5.104.81.23, never on .113 / ibm. After the switch 5.104 runs FIVE t12 seats (b2 b3 b4 b5 b7),
# but none is publicly reachable (ufw drops inbound, every seat listens on 127.0.0.1), so the throwaway
# node's only way in is still the four DNS names — which is the point. Because it shares the host with
# those seats it is isolated and small (release-prep review):
#   * `env -i PATH=… HOME=<throwaway>`: kaspad writes ~/.misaka/testnet-12/endpoints.json on start and
#     root's is the live seats' registry (the probe's 09-24 finding, build-release-5104.sh); no KASPAD_*
#     variable of the caller's shell reaches it;
#   * `--ram-scale=0.1`: ~0.13 GiB of consensus caches instead of the 1.34 GiB the default declares —
#     the host keeps a 4 GiB reserve beside five 3.5 GiB shares (PLAN.md §2);
#   * it refuses to start with under 2 GiB MemAvailable.
# Passive: no --palw-* flags, so no heartbeat miner, no producer, no panel (all default off).
# Everything binds 127.0.0.1; 240 s wall clock; leaves its appdir+log for you to inspect/remove.
#
#   CONFIRM=yes ./60-join-check.sh /root/t12-rel/<REV>/bin/kaspad
source "$(dirname "$0")/lib-seeders.sh"

load_seeder c5104
KASPAD="${1:?usage: CONFIRM=yes $0 <release kaspad path on 5.104.81.23>}"
: "${EXPECT_FP:=__FILL_ME__}"
if [ "$EXPECT_FP" = __FILL_ME__ ]; then
  echo "refusing: fleet.env EXPECT_FP is not filled — cannot tell the release build from any other" >&2
  exit 2
fi
need_confirm
TS=$(date -u +%Y%m%dT%H%M%SZ)
APP=/root/.t12-joincheck-$TS
LOG=/root/t12-joincheck-$TS.log

rsh "bash -s" <<EOF
set -u
for p in 26399 26396 26397 26395; do
  if ss -ltn | grep -q ":\$p "; then echo "port \$p busy — aborting"; exit 1; fi
done
[ -x '$KASPAD' ] || { echo "no kaspad at $KASPAD"; exit 1; }
avail=\$(awk '/MemAvailable/{print int(\$2/1024)}' /proc/meminfo)
[ "\$avail" -ge 2048 ] || { echo "MemAvailable \${avail} MiB < 2048 — the five seats need it; not starting a throwaway node beside them"; exit 1; }
echo "kaspad sha: \$(sha256sum '$KASPAD' | cut -c1-64)"
JHOME=\$(mktemp -d /root/t12-joincheck-home-XXXXXX)
timeout --signal=INT --kill-after=30 240 env -i PATH="\$PATH" HOME="\$JHOME" '$KASPAD' --testnet --netsuffix=12 --yes --appdir='$APP' \
  --listen=127.0.0.1:26399 --rpclisten=127.0.0.1:26396 --rpclisten-borsh=127.0.0.1:26397 \
  --rpclisten-json=127.0.0.1:26395 --disable-upnp --ram-scale=0.1 >'$LOG' 2>&1 || true
rm -rf "\$JHOME"
fail=0
grep -m1 'Consensus params fingerprint' '$LOG'
grep -q "Consensus params fingerprint: $EXPECT_FP" '$LOG' || { echo "FAIL: not the release fingerprint ($EXPECT_FP)"; fail=1; }
grep -E 'Querying DNS seeder|Retrieved [0-9]+ addresses from DNS seeder' '$LOG' | sort | uniq -c | head -12
grep -q 'Retrieved [1-9][0-9]* addresses from DNS seeder' '$LOG' || { echo "FAIL: no DNS seeder returned an address"; fail=1; }
grep -E 'P2P Connected to outgoing peer' '$LOG' | head -5
grep -q 'P2P Connected to outgoing peer' '$LOG' || { echo "FAIL: no outbound peer"; fail=1; }
grep -E 'IBD (started|with peer .* completed)' '$LOG' | head -4
if grep -E -q 'Genesis mismatch|Network mismatch|Consensus params mismatch|Fork-id mismatch' '$LOG'; then
  echo "WARN: refusals seen (a peer on another genesis/params is still being advertised):"
  grep -E 'Genesis mismatch|Network mismatch|Consensus params mismatch|Fork-id mismatch' '$LOG' | head -5 | cut -c1-240
fi
echo "log: $LOG   appdir: $APP  (remove both when done)"
exit \$fail
EOF
