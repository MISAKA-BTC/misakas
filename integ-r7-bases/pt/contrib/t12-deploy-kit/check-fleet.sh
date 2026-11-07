#!/bin/bash
# deploy-t12/check-fleet.sh — run on the operator's Mac. READ-ONLY: runs each host's `check`, asks
# the four seeders what they answer, and dials the two public P2P anchors from outside.
#   ./check-fleet.sh            all hosts
#   CHECK_REGISTRY=1 ./check-fleet.sh   + the class lifecycle (8k: Prefetching → Probation once 7 seats are ready)
set -uo pipefail
KIT=$(cd "$(dirname "$0")" && pwd)
. "$KIT/fleet.env"
K="ssh -o IdentitiesOnly=yes -i $HOME/.ssh/claude_key -o ConnectTimeout=15"
say() { printf '\n[%s] ===== %s\n' "$(date -u +%H:%M:%S)" "$*"; }

say "ibm (b0 b1)";   ssh -o ConnectTimeout=15 misaka-ibm "cd $REL_ROOT/kit && CHECK_REGISTRY=${CHECK_REGISTRY:-} ./install-ibm.sh check"
say ".113 (b6)";     $K root@169.58.232.113 "cd $REL_ROOT/kit && CHECK_REGISTRY=${CHECK_REGISTRY:-} ./install-113.sh check"
say "5.104 (b2 b3 b4 b5 b7)"; $K root@5.104.81.23 "cd $REL_ROOT/kit && ./install-5104.sh check"
say "seeders — the seeder kit's own read-only verifier (SEEDERS.md): all four, evidence from the last 2 min"
if [ -x "$KIT/seeders/40-verify.sh" ]; then "$KIT/seeders/40-verify.sh"; else echo "  seeders/40-verify.sh not found"; fi
printf '  public resolution: seeder1 → %s | seeder3 → %s\n' "$(dig +short seeder1.misakascan.com A | tr '\n' ' ')" "$(dig +short seeder3.misakascan.com A | tr '\n' ' ')"

say "public P2P anchors, dialled from here"
for a in 169.58.232.113:26311 169.58.39.220:26311 169.58.39.220:26321; do
    if nc -z -G 5 "${a%:*}" "${a#*:}" 2>/dev/null; then echo "  $a open"; else echo "  $a CLOSED"; fi
done
