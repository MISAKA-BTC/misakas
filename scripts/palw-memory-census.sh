#!/usr/bin/env bash
# What every kaspad on this host holds, as the kernel accounts it: PSS split into anonymous and
# file-backed pages, and swap — the reading that tells a mapped artifact (file-backed, shared) from
# a copied one (anonymous, per process). Linux only (/proc/<pid>/smaps_rollup).
#
#   scripts/palw-memory-census.sh            # every kaspad
#   scripts/palw-memory-census.sh 1234 5678  # these pids
set -u
pids=("$@")
[ ${#pids[@]} -gt 0 ] || mapfile -t pids < <(pgrep -x kaspad)
printf '%-8s %10s %10s %10s %10s %10s\n' pid rss_mb pss_mb anon_mb file_mb swap_mb
total_anon=0; total_file=0; total_swap=0
for pid in "${pids[@]}"; do
  [ -r "/proc/$pid/smaps_rollup" ] || continue
  read -r rss pss anon file swap < <(awk '/^Rss:/{r=$2} /^Pss:/{p=$2} /^Pss_Anon:/{a=$2} /^Pss_File:/{f=$2} /^Swap:/{s=$2} END{print r, p, a, f, s}' "/proc/$pid/smaps_rollup")
  printf '%-8s %10d %10d %10d %10d %10d\n' "$pid" $((rss/1024)) $((pss/1024)) $((anon/1024)) $((file/1024)) $((swap/1024))
  total_anon=$((total_anon + anon)); total_file=$((total_file + file)); total_swap=$((total_swap + swap))
done
printf '%-8s %10s %10s %10d %10d %10d\n' total - - $((total_anon/1024)) $((total_file/1024)) $((total_swap/1024))
awk '/MemTotal|MemAvailable|AnonPages|^Cached|SwapTotal|SwapFree/{printf "%s %d MB  ", $1, $2/1024} END{print ""}' /proc/meminfo
