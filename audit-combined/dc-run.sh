#!/usr/bin/env bash
# audit-combined/dc-run.sh — the combined drill's scenario clock (run in the background by `dc.sh run`, after `dc.sh up`): DG-1 and DG-2 at the crossing, a
# PANEL / SHARE / RED-BLUE snapshot every 30 DAA (evidence), the recovery leg at RECOVERY_AT, then DG-3 .. DG-7b (their claims need new5 and new6 up, so they
# wait for the recovery leg). Everything it writes goes under $EVD. Env comes from dc.sh (WORK_DIR, INT11_AT, RECOVERY_AT, ...).
set -uo pipefail
C=$(cd "$(dirname "$0")" && pwd); WT=$(cd "$C/.." && pwd)
EVD=${EVD:-$HOME/Downloads/MISAKA-wt-b/lanes/evidence/combined-drill}; mkdir -p "$EVD"
cd "$WT/audit-improve"; . ./lib-dm.sh 2>/dev/null
H=$INT11_AT; RECOVERY_AT=${RECOVERY_AT:-566}; K_SLOTS=${K_SLOTS:-20}; STOP_DAA=${STOP_DAA:-$((2*K_SLOTS+4))}; POST_DAA=${POST_DAA:-$((K_SLOTS+16))}
now() { tip new3 2>/dev/null || echo 0; }
at() { while [ "$(now)" -lt "$1" ]; do sleep 20; done; }
stamp() { echo "$(date '+%F %T') DAA $(now) $*" >> "$EVD/timeline.log"; }
run() { local n=$1; shift; stamp "START $n"; "$@" > "$EVD/$n.log" 2>&1; stamp "END $n rc=$?"; }
snap() { local d; d=$(now); python3 "$C/dcwatch.py" panel --work "$WORK_DIR" > "$EVD/panel-$d.txt" 2>&1
         python3 "$C/dcwatch.py" share --port "$JSON_BASE_3" --fence "$H" --work "$WORK_DIR" --producers new4,new6 > "$EVD/share-$d.txt" 2>&1; }
JSON_BASE_3=$((JSON_BASE + 3))

( run dg1 bash "$WT/audit-improve/dm.sh" gen dg1 ) & P1=$!
( run dg2 bash "$WT/audit-improve/dm.sh" gen dg2 ) & P2=$!
# snapshots every 30 DAA from H' + 30 on, in the background
( d=$((H + 30)); while :; do at "$d"; snap; d=$((d + 30)); [ "$d" -gt $((RECOVERY_AT + 200)) ] && break; done ) &

at "$RECOVERY_AT"
stamp "recovery: stopping new4 and new6"
STOP=$(now); bash nodes.sh stop new4 new6 >> "$EVD/recovery.log" 2>&1
at $((STOP + STOP_DAA)); RESTART=$(now)
stamp "recovery: restarting new4 and new6 (stopped for $((RESTART - STOP)) DAA)"
bash nodes.sh start new4 new6 >> "$EVD/recovery.log" 2>&1
at $((RESTART + POST_DAA)); END=$(now)
printf '{"stop_daa": %s, "restart_daa": %s, "end_daa": %s}\n' "$STOP" "$RESTART" "$END" > "$EVD/recovery.json"
python3 "$C/dcwatch.py" recovery --port "$JSON_BASE_3" --state "$EVD/recovery.json" --k "$K_SLOTS" > "$EVD/recovery.verdict.txt" 2>&1; rc=$?
mkdir -p "$WORK_DIR/verdict"; { case $rc in 0) echo PASS;; 1) echo FAIL;; *) echo INCOMPLETE;; esac; tail -1 "$EVD/recovery.verdict.txt"; } | tr '\n' '\t' > "$WORK_DIR/verdict/recovery.verdict"; echo >> "$WORK_DIR/verdict/recovery.verdict"
stamp "recovery verdict rc=$rc"

wait "$P1" "$P2"
export DG3_DEADLINE_DAA=$(( $(now) + 300 ))
run dg3 bash "$WT/audit-improve/dm.sh" gen dg3
run dg4 bash "$WT/audit-improve/dm.sh" gen dg4
run dg5 bash "$WT/audit-improve/dm.sh" gen dg5
run dg6 env KILL=1 bash "$WT/audit-improve/dm.sh" gen dg6
run dg7b bash "$WT/audit-improve/dm.sh" gen dg7b
snap; stamp "ALL DONE"
