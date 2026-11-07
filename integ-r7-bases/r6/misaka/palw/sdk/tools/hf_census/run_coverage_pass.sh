#!/bin/sh
# The shape-depth gates of the sample and the three bucket cohorts (+ the partial-task cohort) at one height, for `coverage_report.py`.
#   run_coverage_pass.sh SNAPSHOT_DIR PALW_CLASS_BIN TREE RULESET HEIGHT
# e.g. run_coverage_pass.sh $S/ $B d368707e0 a 6900   and   … b 9000. Needs `shadow_dirs.py` to have written $S/p4/*.dirs.
# Writes $S/p4/rows/<ruleset>.<set>.jsonl, a judge cache per ruleset (a judgment is a function of the build: one cache file per tree) and
# $S/p4/rows/<ruleset>.progress. No network.
set -e
S=${1%/}; B=$2; T=$3; R=$4; H=$5
export RAYON_NUM_THREADS=1
mkdir -p "$S/p4/rows"
for set in sample cohort_adapters cohort_task_unknown cohort_base_unpinned cohort_partial; do
  nice -n 15 "$B" census gates --snapshot "$(basename "$S")" --tree "$T" --threads 2 --depth shape --height "$H" \
    --judge-budget-secs 600 --judge-cache "$S/p4/rows/judge-cache-$R.jsonl" --dirs "$S/p4/$set.dirs" \
    > "$S/p4/rows/$R.$set.jsonl" 2> "$S/p4/rows/$R.$set.err"
  echo "$set done $(date +%H:%M:%S)" >> "$S/p4/rows/$R.progress"
done
echo finished >> "$S/p4/rows/$R.progress"
