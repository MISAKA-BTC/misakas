#!/usr/bin/env bash
# Turn a soak sample file into the six answers the gate actually asks.
#
# Written so the verdict cannot be reached by looking at the file and feeling reassured. Each
# signal is computed, each has a threshold of zero, and the exit code is the verdict.
#
# Usage: soak_verdict.sh <samples.tsv>
set -uo pipefail
S=${1:?usage: soak_verdict.sh <samples.tsv>}

echo "=== soak verdict: $S ==="
echo "samples: $(($(wc -l < "$S") - 1))  window: $(sed -n 2p "$S" | cut -f1) .. $(tail -1 "$S" | cut -f1)"
echo

fail=0
report() { # name count detail
  if [ "$2" -eq 0 ]; then
    printf '  %-24s %s\n' "$1" "0"
  else
    printf '  %-24s %s   <-- FAIL\n' "$1" "$2"
    [ -n "${3:-}" ] && printf '%s\n' "$3" | sed 's/^/      /'
    fail=1
  fi
}

# 1. Panic. Counts are cumulative per node, so any node whose count ever exceeded its first
#    reading has panicked during the window.
panicked=$(awk -F'\t' 'NR>1 && $4=="yes" {n=$3; if (!(n in first)) first[n]=$10; if ($10+0 > first[n]+0) bad[n]=$10-first[n]} END {for (n in bad) print n, bad[n]}' "$S")
report "panics" "$(printf '%s' "$panicked" | grep -c . || true)" "$panicked"

# 2. Permanent quarantine: a node reporting quarantined in its LAST sample. Entering quarantine is
#    correct behaviour; still being there at the end is the outage.
quar=$(awk -F'\t' 'NR>1 && $4=="yes" {last[$3]=$9} END {for (n in last) if (last[n]=="quarantined") print n}' "$S")
report "ended quarantined" "$(printf '%s' "$quar" | grep -c . || true)" "$quar"

# 3. Signer leakage: is_synced=true while the gate was holding. The gate is the authority on
#    whether this node may act; is_synced claiming otherwise means a signer was told it could.
leak=$(awk -F'\t' 'NR>1 && $4=="yes" && $8=="true" && $9!="ready" {print $1, $3, "gate="$9}' "$S")
report "signer leakage" "$(printf '%s' "$leak" | grep -c . || true)" "$leak"

# 4. Wrong chain: a node whose pruning point differs from the majority in the same sample, for
#    more than one consecutive sample. Majority rather than a named reference, because which chain
#    is right is exactly what must not be assumed.
wrong=$(awk -F'\t' '
  NR>1 && $4=="yes" && $5!="" { key=$1; cnt[key,$5]++; seen[key]=1; row[NR]=$1"\t"$3"\t"$5 }
  END {
    for (k in seen) { best=""; bn=0; for (c in cnt) { split(c,p,SUBSEP); if (p[1]==k && cnt[c]>bn) { bn=cnt[c]; best=p[2] } } maj[k]=best }
    for (r in row) { split(row[r],f,"\t"); if (maj[f[1]] != "" && f[3] != maj[f[1]]) print f[1], f[2], "on", f[3], "majority", maj[f[1]] }
  }' "$S" | sort | awk '{c[$2]++; l[$2]=$0} END {for (n in c) if (c[n]>1) print l[n], "("c[n]" samples)"}')
report "off-majority chain" "$(printf '%s' "$wrong" | grep -c . || true)" "$wrong"

# 5. Permit reuse cannot be seen from counts alone — a rising count is normal. What is reported is
#    the rate, for a human to judge against how many switches actually happened.
permits=$(awk -F'\t' 'NR>1 && $4=="yes" {n=$3; if (!(f[n] in f)) f[n]=$12; l[n]=$12} END {for (n in l) if (l[n]+0 > f[n]+0) print n, l[n]-f[n]}' "$S")
echo "  permits granted per node (for review, not a threshold):"
printf '%s\n' "${permits:-  (none)}" | sed 's/^/      /'

# 6. Reachability, because a node that was down for the window proves nothing about the window.
echo
awk -F'\t' 'NR>1 {t[$3]++; if ($4=="yes") ok[$3]++} END {for (n in t) printf "  %-34s %d%% reachable (%d/%d)\n", n, 100*ok[n]/t[n], ok[n], t[n]}' "$S" | sort

echo
if [ "$fail" -eq 0 ]; then
  echo "VERDICT: no signal tripped. This is not the same as 'the candidate is correct' — see"
  echo "         docs/testing/public-testnet-soak.md, 'What the result can and cannot say'."
else
  echo "VERDICT: FAIL"
fi
exit "$fail"
