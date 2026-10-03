#!/usr/bin/env bash
# audit-combined/dc-run.sh — the combined drill's scenario clock (run in the background by `dc.sh run`, after `dc.sh up`). Env comes from dc.sh. The order puts the fence-crossing gates
# FIRST so they can be published early:
#   H' - 6  FORK-c   new2 is PARTITIONED across the fence (restarted on a shifted P2P port, no peers: it mines its own branch past H'); it rejoins at H' + 6 and must reorganise onto the chain
#   H'      D-M5 / FORK-a (the old relay, int-10.3: the new node drops it AT the crossing), DG-1 and DG-2 (the driver and `dm.sh gen`)
#   H' + 12 FORK-b   a fresh node (empty datadir) IBDs from genesis across the fence and must reach the tip
#   up ..   leg S    (from `dc.sh up`: new6 holds every attempt STALE_DELAY_S so it lands RED, new4 is a seat only): the only REAL attempts are stale: STALE, COOLDOWN; the chain starts Idle
#   STALE_END_DAA (140, and after the head's Probation)   leg B1  new4 (fast) and new6 (normal delay) on: RED -> BLUE (RED>BLUE1), then Normal; stale.json and restart1.json written
#   RECOVERY_AT   leg A    both REAL producers stopped for STOP_DAA (K + 8) slots: floors resume only after K idle slots, the DAA keeps advancing; then back (B2): RECOVERY, RED>BLUE2
#   every 30 DAA from H' + 100 a gates snapshot (evidence); leg X (report-only, X_DAA > 0): one policy-IGNORING floor producer.
# Everything it writes goes under $EVD.
set -uo pipefail
C=$(cd "$(dirname "$0")" && pwd); WT=$(cd "$C/.." && pwd)
EVD=${EVD:-$HOME/Downloads/MISAKA-wt-b/lanes/evidence/combined-drill}; mkdir -p "$EVD"
cd "$WT/audit-improve"; . ./lib-dm.sh 2>/dev/null
H=$INT11_AT; K_SLOTS=${K_SLOTS:-20}; PROBE_SLOTS=${PROBE_SLOTS:-6}; COOLDOWN=${COOLDOWN:-20}
STALE_END_DAA=${STALE_END_DAA:-140}; STALE_DELAY_S=${STALE_DELAY_S:-1500}; REAL_DELAY_S=${REAL_SUBMIT_DELAY_S:-340}
RECOVERY_AT=${RECOVERY_AT:-258}; STOP_DAA=${STOP_DAA:-$((K_SLOTS+8))}; POST_DAA=${POST_DAA:-$((K_SLOTS+8))}; X_DAA=${X_DAA:-0}
MS=$WORK_DIR/milestones.tsv
now() { tip new3 2>/dev/null || echo 0; }
nodetip() { tip "$1" 2>/dev/null || echo 0; }
at() { while [ "$(now)" -lt "$1" ]; do sleep 20; done; }
stamp() { echo "$(date '+%F %T') DAA $(now) $*" >> "$EVD/timeline.log"; }
run() { local n=$1; shift; stamp "START $n"; "$@" > "$EVD/$n.log" 2>&1; stamp "END $n rc=$?"; }
milestone_daa() { awk -F'\t' -v k="$1" '$1==k {print $2; exit}' "$MS" 2>/dev/null; }
jset() { python3 - "$EVD/fork.json" "$1" "$2" <<'PY'
import json, os, sys
p, k, v = sys.argv[1:4]
d = json.load(open(p)) if os.path.exists(p) else {}
d[k] = json.loads(v)
json.dump(d, open(p, "w"), indent=1)
PY
}
JSON_BASE_3=$((JSON_BASE + 3))
GATE_ARGS=(--port "$JSON_BASE_3" --fence "$H" --work "$WORK_DIR" --producers new4,new6 --evd "$EVD" --state "$EVD/recovery.json" --stale "$EVD/stale.json" --restart1 "$EVD/restart1.json"
           --k "$K_SLOTS" --idle-slots "$K_SLOTS" --probe-slots "$PROBE_SLOTS" --cooldown "$COOLDOWN")
snap() { local d; d=$(now); python3 "$C/dcwatch.py" panel --work "$WORK_DIR" > "$EVD/panel-$d.txt" 2>&1
         python3 "$C/dcwatch.py" share --port "$JSON_BASE_3" --fence "$H" --work "$WORK_DIR" --producers new4,new6 > "$EVD/share-$d.txt" 2>&1
         python3 "$C/dcwatch.py" gates "${GATE_ARGS[@]}" > "$EVD/gates-$d.txt" 2>&1; }

( run dg1 bash "$WT/audit-improve/dm.sh" gen dg1 ) & P1=$!
( run dg2 bash "$WT/audit-improve/dm.sh" gen dg2 ) & P2=$!
( d=$((H + 100)); while :; do at "$d"; snap; d=$((d + 30)); [ "$d" -gt $((RECOVERY_AT + STOP_DAA + POST_DAA + CAP_WINDOW_DAA + 80)) ] && break; done ) &

# ---- FORK-c: a partition across the fence ------------------------------------------------------------------
( at $((H - 6)); P0=$(now); stamp "FORK-c: partitioning new2 (shifted P2P port, no peers) at its DAA $(nodetip new2)"
  bash nodes.sh stop new2 >> "$EVD/fork.log" 2>&1
  ISOLATE_NODE=new2 ISO_PORT_SHIFT=800 bash nodes.sh start new2 >> "$EVD/fork.log" 2>&1
  while [ "$(nodetip new2)" -lt $((H + 6)) ]; do sleep 20; done
  ISO_TIP=$(nodetip new2); stamp "FORK-c: new2 reached DAA $ISO_TIP alone (main $(now)); rejoining"
  bash nodes.sh stop new2 >> "$EVD/fork.log" 2>&1
  bash nodes.sh start new2 >> "$EVD/fork.log" 2>&1
  jset partition "{\"node\": \"new2\", \"start_daa\": $P0, \"isolated_tip_daa\": $ISO_TIP, \"rejoin_daa\": $(now)}"
  stamp "FORK-c: new2 rejoined" ) &

# ---- FORK-b: a fresh node IBDs from genesis across the fence -----------------------------------------------
( at $((H + 12)); J0=$(now); stamp "FORK-b: the fresh node starts (empty datadir, IBD from genesis)"
  rm -rf "$WORK_DIR/joiner"; bash nodes.sh start joiner >> "$EVD/fork.log" 2>&1
  for _ in $(seq 1 120); do [ "$(nodetip joiner)" -ge $(( $(now) - 2 )) ] && break; sleep 30; done
  jset joiner "{\"start_daa\": $J0, \"synced_daa\": $(nodetip joiner), \"main_daa\": $(now)}"
  stamp "FORK-b: the fresh node at DAA $(nodetip joiner) (main $(now))"
  sleep 120; bash nodes.sh stop joiner >> "$EVD/fork.log" 2>&1 ) &

# ---- leg S ends / leg B1 -----------------------------------------------------------------------------------
at "$STALE_END_DAA"
until [ -n "$(milestone_daa class:head:Probation)" ]; do sleep 20; done      # the head class must be admitted before new4 can produce
WIN_P=$(milestone_daa class:win:Probation); S0=${WIN_P:-90}; S1=$(now)
printf '{"start_daa": %s, "end_daa": %s, "delay_s": %s}\n' "$S0" "$S1" "$STALE_DELAY_S" > "$EVD/stale.json"
stamp "leg B1: new4 (fast) and new6 (delay ${REAL_DELAY_S}s) on; leg S ran DAA $S0..$S1 (head Probation at $(milestone_daa class:head:Probation))"
bash nodes.sh stop new6 new4 >> "$EVD/recovery.log" 2>&1
HEAD_PRODUCE=1 REAL_SUBMIT_DELAY_S=$REAL_DELAY_S bash nodes.sh start new6 new4 >> "$EVD/recovery.log" 2>&1
R1=$(now); printf '{"restart_daa": %s}\n' "$R1" > "$EVD/restart1.json"; stamp "leg B1: restarted at DAA $R1"

# ---- leg A (and B2) -----------------------------------------------------------------------------------------
at "$RECOVERY_AT"
STOP=$(now); stamp "leg A: stopping new4 and new6 (both REAL producers)"
bash nodes.sh stop new4 new6 >> "$EVD/recovery.log" 2>&1
at $((STOP + STOP_DAA)); stamp "leg B2: new4 and new6 back (A lasted $(( $(now) - STOP )) DAA)"
HEAD_PRODUCE=1 REAL_SUBMIT_DELAY_S=$REAL_DELAY_S bash nodes.sh start new6 new4 >> "$EVD/recovery.log" 2>&1
RESTART=$(now)
at $((RESTART + POST_DAA)); END=$(now)
printf '{"stop_daa": %s, "restart_daa": %s, "end_daa": %s}\n' "$STOP" "$RESTART" "$END" > "$EVD/recovery.json"
python3 "$C/dcwatch.py" recovery --port "$JSON_BASE_3" --state "$EVD/recovery.json" --k "$K_SLOTS" > "$EVD/recovery.verdict.txt" 2>&1; rc=$?
mkdir -p "$WORK_DIR/verdict"; { case $rc in 0) echo PASS;; 1) echo FAIL;; *) echo INCOMPLETE;; esac; tail -1 "$EVD/recovery.verdict.txt"; } | tr '\n' '\t' > "$WORK_DIR/verdict/recovery.verdict"; echo >> "$WORK_DIR/verdict/recovery.verdict"
stamp "legs A and B2 done; recovery verdict rc=$rc"

# ---- leg X (report-only, off unless X_DAA > 0): one policy-IGNORING floor producer ---------------------------
if [ "$X_DAA" -gt 0 ]; then
  EF=${EXT_FLOOR_FLAG:-}
  if [ -z "$EF" ] || ! "$KASPAD_BIN" --help 2>/dev/null | grep -q -- "$EF"; then stamp "leg X skipped: the binary lists no $EF"
  elif [ ! -s "$WORK_DIR/liars/bond-15.json" ]; then stamp "leg X skipped: bond 15 is not registered"
  else
    X0=$(now); stamp "leg X: starting the policy-ignoring floor producer ($EF)"
    bash nodes.sh start extfloor >> "$EVD/recovery.log" 2>&1
    at $((X0 + X_DAA)); X1=$(now)
    printf '{"start_daa": %s, "end_daa": %s, "flag": "%s"}\n' "$X0" "$X1" "$EF" > "$EVD/ignore.json"
    bash nodes.sh stop extfloor >> "$EVD/recovery.log" 2>&1; stamp "leg X done"
  fi
fi
wait "$P1" "$P2"
snap; python3 "$C/dcwatch.py" gates "${GATE_ARGS[@]}" > "$EVD/gates-final.txt" 2>&1; stamp "ALL DONE (gates in $EVD/gates-final.txt)"
