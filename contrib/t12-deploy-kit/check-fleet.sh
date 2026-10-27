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

#   CHECK_G14=1 ./check-fleet.sh        + the DAA 17,000 G14 slice counters per node (t12check.py --g14; fields the RPC does
#                                         not carry print "unavailable") and fp/tip agreement across all three hosts
OUT=$(mktemp); trap 'rm -f "$OUT"' EXIT
say "ibm (b0 b1 b3)";   ssh -o ConnectTimeout=15 misaka-ibm "cd $REL_ROOT/kit && CHECK_REGISTRY=${CHECK_REGISTRY:-} CHECK_G14=${CHECK_G14:-} ./install-ibm.sh check" | tee -a "$OUT"
say ".113 (b6 b7)";  $K root@169.58.232.113 "cd $REL_ROOT/kit && CHECK_REGISTRY=${CHECK_REGISTRY:-} CHECK_G14=${CHECK_G14:-} ./install-113.sh check" | tee -a "$OUT"
say "5.104 (b2 b4)"; $K root@5.104.81.23 "cd $REL_ROOT/kit && CHECK_G14=${CHECK_G14:-} ./install-5104.sh check" | tee -a "$OUT"
if [ -n "${CHECK_G14:-}" ]; then
    say "G14: fp / tip agreement across the three hosts (tips can differ by a block in flight: rerun before calling it a split)"
    grep '^G14TIP' "$OUT" | sort | uniq -c
    n_fp=$(grep '^G14TIP fp=' "$OUT" | sed -E 's/.*fp=([^ ]*).*/\1/' | sort -u | wc -l | tr -d ' ')
    [ "$n_fp" -le 1 ] && echo "  fp: all nodes agree" || echo "  fp: DISAGREE ($n_fp distinct)"
fi
say "seeders — the seeder kit's own read-only verifier (SEEDERS.md): all four, evidence from the last 2 min"
if [ -x "$KIT/seeders/40-verify.sh" ]; then "$KIT/seeders/40-verify.sh"; else echo "  seeders/40-verify.sh not found"; fi
printf '  public resolution: seeder1 → %s | seeder3 → %s\n' "$(dig +short seeder1.misakascan.com A | tr '\n' ' ')" "$(dig +short seeder3.misakascan.com A | tr '\n' ' ')"

say "public P2P anchors, dialled from here"
for a in 169.58.232.113:26311 169.58.39.220:26311 169.58.39.220:26321; do
    if nc -z -G 5 "${a%:*}" "${a#*:}" 2>/dev/null; then echo "  $a open"; else echo "  $a CLOSED"; fi
done
