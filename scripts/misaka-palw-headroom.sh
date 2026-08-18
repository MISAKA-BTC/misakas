#!/usr/bin/env bash
# misaka-palw-headroom.sh — the number that decides whether a PALW network can admit new nodes.
#
# Not hours-to-sync:  block interval / cost of verifying one header  (ADR-0041).
# Below 1x a node can never finish syncing and the network is closed to newcomers. It falls if the
# block interval shortens, the model gets slower, the host gets busier, or a second PALW node is
# co-located on the machine.
#
# Deliberately interval-over-cost rather than headers-synced-over-blocks-produced: the latter is ~1
# for any node that is already caught up, which says nothing about its margin.
#
# Usage: misaka-palw-headroom.sh [appdir] [window-minutes]     (read-only; default 60 minutes)
set -u
APPDIR="${1:-$HOME/.palw-soak}"
WINDOW="${2:-60}"
LOG="$APPDIR/kaspad.log"

since=$(date -d "-${WINDOW} minutes" '+%Y-%m-%d %H:%M' 2>/dev/null || echo "0000")

# Median per-header validation time. On a PALW network `validate(A,parallelizable)` IS the inference.
# Median via `sort -n` rather than awk's `asort`, which is a gawk extension — the fleet's default awk
# is not guaranteed to be gawk, and an undefined function there would empty this field and raise a
# false HEADROOM-LOW.
hdr_p50=$(awk -v s="$since" 'substr($0,1,16) >= s && match($0, /validate\(A,parallelizable\) [0-9.]+us/) {
              t = substr($0, RSTART, RLENGTH); sub(/.* /, "", t); sub(/us$/, "", t); print t/1000000
            }' "$LOG" 2>/dev/null | sort -n | awk '{v[NR]=$1} END{if (NR) printf "%.1f", v[int(NR/2)+1]}')

# Observed seconds per block, from this node's own accepted-block timestamps. Negative deltas
# (midnight) are skipped rather than corrected: one dropped sample beats a wrong mean.
spb=$(awk -v s="$since" 'substr($0,1,16) >= s && /Accepted block /{
          split(substr($0,12,8), a, ":"); t = a[1]*3600 + a[2]*60 + a[3]
          if (p != "" && t > p) { d += t - p; n++ }
          p = t
        } END { if (n) printf "%.0f", d/n }' "$LOG" 2>/dev/null)

headroom=""
if [ -n "$hdr_p50" ] && [ -n "$spb" ] && [ "$hdr_p50" != "0.0" ]; then
  headroom=$(awk -v a="$spb" -v b="$hdr_p50" 'BEGIN{printf "%.1f", a/b}')
fi

# Concurrent PALW workers on this HOST, not in this process. Two node processes are two concurrent
# inferences whatever MISAKA_PALW_CONCURRENCY says (measured 0.38x serial throughput), so >1 here
# means the host is in that configuration. MISAKA_PALW_LEASE_DIR is what bounds it across processes.
#
# Sampled three times rather than once: a worker is a short-lived process on the one-shot path, so a
# single glance routinely reports 1 on a host that is in fact running two nodes. The max of a couple
# of seconds is still not proof of absence — CO-LOCATED firing means co-location, its silence does
# not mean isolation.
workers=0
for _ in 1 2 3; do
  n=$(pgrep -c -f 'palw-worker --mode' 2>/dev/null || echo 0)
  [ "$n" -gt "$workers" ] && workers=$n
  sleep 0.7
done

out="hdr_p50=${hdr_p50:-?}s spb=${spb:-?}s headroom=${headroom:-?}x workers=$workers"
# Flag anything under 2x, and anything we could not measure — a missing number is not reassurance.
case "${headroom:-none}" in
  none|0.*|1.*) out="$out HEADROOM-LOW" ;;
esac
[ "${workers:-0}" -gt 1 ] && out="$out CO-LOCATED"
echo "$out"
