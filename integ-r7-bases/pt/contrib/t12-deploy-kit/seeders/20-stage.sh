#!/usr/bin/env bash
# deploy-t12/seeders/20-stage.sh — upload a seeder binary NEXT TO the live one, verify its sha,
# and (on hosts without a public node) smoke-test it on loopback. Does NOT touch the running unit.
#
#   CONFIRM=yes ./20-stage.sh <c5104|seeder3|ibm|seeder1> <local x86-64 binary>
#
# Result on the host: <binary path>.new-<sha8>   (the live binary and unit are untouched)
source "$(dirname "$0")/lib-seeders.sh"

load_seeder "${1:-}"
local_bin="${2:?usage: CONFIRM=yes $0 <seeder> <local binary>}"
[ -f "$local_bin" ] || { echo "no such file: $local_bin" >&2; exit 2; }
file "$local_bin" | grep -q 'ELF 64-bit LSB.*x86-64' || { echo "not an x86-64 ELF: $local_bin" >&2; exit 2; }
need_confirm

sha=$(shasum -a 256 "$local_bin" | cut -c1-64)
if [ -n "${SEEDER_SHA256:-}" ] && [ "$SEEDER_SHA256" != KEEP ] && [ "$SEEDER_SHA256" != __FILL_ME__ ] \
   && [ "$SEEDER_SHA256" != "$sha" ]; then
  echo "refusing: $local_bin is $sha but fleet.env SEEDER_SHA256=$SEEDER_SHA256" >&2
  exit 2
fi
staged="$S_BIN.new-${sha:0:8}"
echo "staging $local_bin ($sha) -> $S_SSH:$staged"

scp -q "${SSH_OPTS[@]}" "$local_bin" "$S_SSH:$staged.part"
rsh "set -e
got=\$(sha256sum '$staged.part' | cut -c1-64)
[ \"\$got\" = '$sha' ] || { echo \"sha mismatch after upload: \$got\"; rm -f '$staged.part'; exit 1; }
chmod 755 '$staged.part' && mv -f '$staged.part' '$staged'
ls -l '$staged' '$S_BIN'
'$staged' --version"

if [ "$S_PUBLIC" = yes ]; then
  echo "smoke skipped on $S_NAME: it hosts a PUBLIC node (rule: no demos on public-node hosts)."
  echo "the bytes are identical to what was smoke-tested on c5104/seeder3 (sha $sha)."
  exit 0
fi

# Smoke: loopback listener, an anchor that can never answer (TEST-NET-1), a backing port nothing
# serves. Expect the egress-fallback line naming :26311 — the one line that proves which P2P port
# this build probes for testnet-12 (the pre-regenesis build said :26411 and nobody listens there).
rsh "bash -s" <<EOF
set -u
port=15353
if ss -lun | grep -q ":\$port "; then echo "SMOKE SKIP: udp \$port busy"; exit 1; fi
out=\$(timeout 15 '$staged' --network-id testnet-12 --listen 127.0.0.1:\$port --anchors 192.0.2.1 \
        --anchors-only --node-wrpc-borsh 127.0.0.1:9 2>&1 || true)
echo "\$out" | tail -4 | cut -c1-200
if ! echo "\$out" | grep -q 'dial on :26311'; then echo 'SMOKE FAIL: no "dial on :26311" line'; exit 1; fi
if echo "\$out" | grep -q ':26411'; then echo 'SMOKE FAIL: probes :26411'; exit 1; fi
if ! echo "\$out" | grep -q 'authoritative A-record server'; then echo 'SMOKE FAIL: DNS listener did not come up'; exit 1; fi
echo "SMOKE OK ($S_NAME)"
EOF

echo "staged. swap with: CONFIRM=yes ./30-swap.sh $S_NAME --binary $sha"
