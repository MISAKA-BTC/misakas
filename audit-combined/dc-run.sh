#!/usr/bin/env bash
# audit-combined/dc-run.sh — the combined drill's scenario clock (run in the background by `dc.sh run`, after `dc.sh up`). Env comes from dc.sh.
#   at H' - 6   FORK-c   new2 (a heartbeat clock) is PARTITIONED across the fence: restarted on a shifted P2P port with no peers, it mines its own branch past H'; it rejoins at H' + 6
#               and must reorganise onto the chain (same sink within 3 DAA)
#   at H' + 60  FORK-b   a fresh node (empty datadir) IBDs from genesis across the fence and must reach the tip
#   H' ..       DG-1 and DG-2 at the crossing; D-M5's crossing (the old relay: dropped AT the crossing, FORK-a); a gates snapshot every 30 DAA from H' + 100
#   after the last rho window, the legs of the floor policy's checks:
#     A  both REAL producers (new4, new6) stopped for STOP_DAA slots (>= 2K): floors resume only after K idle slots, the DAA keeps advancing
#     S  only new6 comes back, every REAL attempt held STALE_DELAY_S so it lands RED: RED never extends, floors are valid again after probe_slots, probes no more often than every cooldown
#     B  both back at the normal delay: the first REAL attempt may be RED, a BLUE one follows inside the probe (RED -> BLUE), floors stop, REAL turns BLUE
#     X  REPORT-ONLY: one policy-IGNORING floor producer (the `extfloor` node, a drill flag RS adds) for X_DAA slots: how many REAL attempts it turns RED
# Everything it writes goes under $EVD. Knobs: RECOVERY_AT STOP_DAA STALE_DAA STALE_DELAY_S POST_DAA X_DAA K_SLOTS PROBE_SLOTS COOLDOWN DEADLINE_DAA (skip X past it).
set -uo pipefail
C=$(cd "$(dirname "$0")" && pwd); WT=$(cd "$C/.." && pwd)
EVD=${EVD:-$HOME/Downloads/MISAKA-wt-b/lanes/evidence/combined-drill}; mkdir -p "$EVD"
cd "$WT/audit-improve"; . ./lib-dm.sh 2>/dev/null
H=$INT11_AT; K_SLOTS=${K_SLOTS:-20}; PROBE_SLOTS=${PROBE_SLOTS:-6}; COOLDOWN=${COOLDOWN:-20}
RECOVERY_AT=${RECOVERY_AT:-$((H + 285 + 8 + 32 + 2))}; STOP_DAA=${STOP_DAA:-$((2*K_SLOTS+2))}; STALE_DAA=${STALE_DAA:-$((COOLDOWN+PROBE_SLOTS+20))}
STALE_DELAY_S=${STALE_DELAY_S:-1500}; POST_DAA=${POST_DAA:-$((K_SLOTS+8))}; X_DAA=${X_DAA:-24}; DEADLINE_DAA=${DEADLINE_DAA:-100000}
now() { tip new3 2>/dev/null || echo 0; }
nodetip() { tip "$1" 2>/dev/null || echo 0; }
at() { while [ "$(now)" -lt "$1" ]; do sleep 20; done; }
stamp() { echo "$(date '+%F %T') DAA $(now) $*" >> "$EVD/timeline.log"; }
run() { local n=$1; shift; stamp "START $n"; "$@" > "$EVD/$n.log" 2>&1; stamp "END $n rc=$?"; }
jset() { python3 - "$EVD/fork.json" "$1" "$2" <<'PY'
import json, os, sys
p, k, v = sys.argv[1:4]
d = json.load(open(p)) if os.path.exists(p) else {}
d[k] = json.loads(v)
json.dump(d, open(p, "w"), indent=1)
PY
}
JSON_BASE_3=$((JSON_BASE + 3))
GATE_ARGS=(--port "$JSON_BASE_3" --fence "$H" --work "$WORK_DIR" --producers new4,new6 --evd "$EVD" --state "$EVD/recovery.json" --stale "$EVD/stale.json" --k "$K_SLOTS"
           --idle-slots "$K_SLOTS" --probe-slots "$PROBE_SLOTS" --cooldown "$COOLDOWN")
snap() { local d; d=$(now); python3 "$C/dcwatch.py" panel --work "$WORK_DIR" > "$EVD/panel-$d.txt" 2>&1
         python3 "$C/dcwatch.py" share --port "$JSON_BASE_3" --fence "$H" --work "$WORK_DIR" --producers new4,new6 > "$EVD/share-$d.txt" 2>&1
         python3 "$C/dcwatch.py" gates "${GATE_ARGS[@]}" > "$EVD/gates-$d.txt" 2>&1; }

( run dg1 bash "$WT/audit-improve/dm.sh" gen dg1 ) & P1=$!
( run dg2 bash "$WT/audit-improve/dm.sh" gen dg2 ) & P2=$!
( d=$((H + 100)); while :; do at "$d"; snap; d=$((d + 30)); [ "$d" -gt $((RECOVERY_AT + STOP_DAA + STALE_DAA + POST_DAA + X_DAA + 60)) ] && break; done ) &

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
( at $((H + 60)); J0=$(now); stamp "FORK-b: the fresh node starts (empty datadir, IBD from genesis)"
  rm -rf "$WORK_DIR/joiner"; bash nodes.sh start joiner >> "$EVD/fork.log" 2>&1
  for _ in $(seq 1 120); do [ "$(nodetip joiner)" -ge $(( $(now) - 2 )) ] && break; sleep 30; done
  jset joiner "{\"start_daa\": $J0, \"synced_daa\": $(nodetip joiner), \"main_daa\": $(now)}"
  stamp "FORK-b: the fresh node at DAA $(nodetip joiner) (main $(now))"
  sleep 120; bash nodes.sh stop joiner >> "$EVD/fork.log" 2>&1 ) &

at "$RECOVERY_AT"
STOP=$(now); stamp "leg A: stopping new4 and new6 (both REAL producers)"
bash nodes.sh stop new4 new6 >> "$EVD/recovery.log" 2>&1
at $((STOP + STOP_DAA)); S0=$(now)
stamp "leg S: new6 back with --palw-drill-real-submit-delay-s=$STALE_DELAY_S (every REAL attempt stale); new4 stays down (A lasted $((S0 - STOP)) DAA)"
REAL_SUBMIT_DELAY_S=$STALE_DELAY_S bash nodes.sh start new6 >> "$EVD/recovery.log" 2>&1
at $((S0 + STALE_DAA)); S1=$(now)
printf '{"start_daa": %s, "end_daa": %s, "delay_s": %s}\n' "$S0" "$S1" "$STALE_DELAY_S" > "$EVD/stale.json"
stamp "leg B: new6 and new4 back at the normal delay (S lasted $((S1 - S0)) DAA)"
bash nodes.sh stop new6 >> "$EVD/recovery.log" 2>&1
bash nodes.sh start new6 new4 >> "$EVD/recovery.log" 2>&1
RESTART=$(now)
at $((RESTART + POST_DAA)); END=$(now)
printf '{"stop_daa": %s, "restart_daa": %s, "end_daa": %s}\n' "$STOP" "$RESTART" "$END" > "$EVD/recovery.json"
python3 "$C/dcwatch.py" recovery --port "$JSON_BASE_3" --state "$EVD/recovery.json" --k "$K_SLOTS" > "$EVD/recovery.verdict.txt" 2>&1; rc=$?
mkdir -p "$WORK_DIR/verdict"; { case $rc in 0) echo PASS;; 1) echo FAIL;; *) echo INCOMPLETE;; esac; tail -1 "$EVD/recovery.verdict.txt"; } | tr '\n' '\t' > "$WORK_DIR/verdict/recovery.verdict"; echo >> "$WORK_DIR/verdict/recovery.verdict"
stamp "legs A, S, B done; recovery verdict rc=$rc"

# ---- leg X (report-only): one policy-IGNORING floor producer ------------------------------------------------
EF=${EXT_FLOOR_FLAG:-$("$KASPAD_BIN" --help 2>/dev/null | grep -oE -- '--palw-drill-[a-z0-9-]*floor[a-z0-9-]*' | grep -vE -- '-at$|reserve' | head -1)}
if [ -z "$EF" ]; then stamp "leg X skipped: the binary lists no drill flag that makes a floor producer ignore the policy (EXT_FLOOR_FLAG)"
elif [ "$(now)" -gt "$DEADLINE_DAA" ]; then stamp "leg X skipped: past DEADLINE_DAA"
elif [ ! -s "$WORK_DIR/liars/bond-15.json" ]; then stamp "leg X skipped: bond 15 is not registered"
else
  X0=$(now); stamp "leg X: starting the policy-ignoring floor producer ($EF)"
  bash nodes.sh start extfloor >> "$EVD/recovery.log" 2>&1
  at $((X0 + X_DAA)); X1=$(now)
  printf '{"start_daa": %s, "end_daa": %s, "flag": "%s"}\n' "$X0" "$X1" "$EF" > "$EVD/ignore.json"
  bash nodes.sh stop extfloor >> "$EVD/recovery.log" 2>&1; stamp "leg X done"
fi
wait "$P1" "$P2"
snap; python3 "$C/dcwatch.py" gates "${GATE_ARGS[@]}" > "$EVD/gates-final.txt" 2>&1; stamp "ALL DONE (gates in $EVD/gates-final.txt)"
