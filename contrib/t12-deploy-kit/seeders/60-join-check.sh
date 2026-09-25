#!/usr/bin/env bash
# deploy-t12/seeders/60-join-check.sh — the END-TO-END seeder check, run AFTER the node switch:
# a throw-away node from the RELEASE kaspad, with NO --addpeer / --connect, must find the public
# chain through the four DNS names alone, handshake (network name + genesis + params id all agree)
# and IBD. This is the only check that proves "a user needs nothing but the build and --netsuffix=12".
#
# Runs on 5.104.81.23 — the one fleet host with no public node — never on .113 / ibm.
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
echo "kaspad sha: \$(sha256sum '$KASPAD' | cut -c1-64)"
timeout --signal=INT --kill-after=30 240 '$KASPAD' --testnet --netsuffix=12 --yes --appdir='$APP' \
  --listen=127.0.0.1:26399 --rpclisten=127.0.0.1:26396 --rpclisten-borsh=127.0.0.1:26397 \
  --rpclisten-json=127.0.0.1:26395 --disable-upnp >'$LOG' 2>&1 || true
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
