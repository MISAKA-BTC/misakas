#!/usr/bin/env bash
# audit-combined/dc.sh — the COMBINED drill of the DAA-5,300 release candidate (rcore/int-12): ONE salted testnet-12 drill chain on this Mac that crosses the
# whole extended fence list with `--palw-drill-int11-at=H'` (every fence of the release at H', rho 100 / 250 / 1000 at H' + 95 / + 190 / + 285) on the
# binary the release ships, and measures the user's two headline verdicts under a sustained REAL-model claim load:
#   PANEL  verification does not jam        dcwatch.py panel   (bind->licence p50/p95, panel-bound backlog trend, seat oldest wait, F3 producer-hold
#                                                              events, PanelUnavailable expiries, receipts/h per seat; one row per rho window)
#   SHARE  PALW blocks dominate             dcwatch.py share   (sliding windows past H': REAL + EXEC >= 90 % of non-RED blocks, heartbeats <= 10 %,
#                                                              floor attempts only in idle windows; blockKind / laneClass from getBlock verboseData)
# It is audit-improve/dm.sh (the D-M and DG drills, the capacity sampler, the post-genesis bonds, the just-in-time liars) with INT11=1 INT12=1 and the load
# of this file: see `dc.sh plan`. Commands: plan | dry | up | run | status | verdicts | panel | share | redblue | recovery | gen <dgN> | evidence | down.
#   BIN_DIR=<dir with kaspad misaka palw-class> [OLD_KASPAD_BIN=<the fleet's release at the crossing>] bash audit-combined/dc.sh dry
set -euo pipefail
C=$(cd "$(dirname "$0")" && pwd)
WT=$(cd "$C/.." && pwd)
export INT11=1 INT12=1 INT11_AT=${INT11_AT:-150}
export TIR_AT=${TIR_AT:-18}                       # the head registers at ~19: its admission slot (91) is >= 71 DAA later; at TIR 20 it missed the slot by one DAA and waited a period
export OUTSIDER=${OUTSIDER:-1} RIDERS=${RIDERS:-4}
export GEN_PARTIAL_CLASS=${GEN_PARTIAL_CLASS-toy-embed} GEN_PARTIAL_HOLDERS=${GEN_PARTIAL_HOLDERS-"new4 new5 new6"}   # DG-2: toy-embed on three nodes only
export WORK_DIR=${WORK_DIR:-$HOME/.misaka-palw-combined-drill}
export P2P_BASE=${P2P_BASE:-51200} BORSH_BASE=${BORSH_BASE:-52200} JSON_BASE=${JSON_BASE:-53200} EVM_BASE=${EVM_BASE:-54200} GRPC_BASE=${GRPC_BASE:-50200}
export REAL_SUBMIT_DELAY_S=${REAL_SUBMIT_DELAY_S-300}   # lane RS's --palw-drill-real-submit-delay-s on ONE REAL producer (new6): emulates an 8k model's inference time; set it to the 8k p50 P2 measures
export XB_ORDER=${XB_ORDER:-"14 10 11 12 13 8 9"}          # the outsider's bond (14) first: panels have an outsider from the start; 10..13 are DG's liars, 8 and 9 D-M3's
export RECOVERY_AT=${RECOVERY_AT:-600}                     # after the rho windows (end H'+375) and after T's evaluation window (~558): the recovery leg
export GEN_DIR=${GEN_DIR:-$HOME/Downloads/MISAKA-wt-b/gen-drill-classes}
export OLD_KASPAD_BIN=${OLD_KASPAD_BIN:-$HOME/Downloads/MISAKA-wt-b/lifecycle-run/bin/2483c570cea4/kaspad}
EVD=${EVD:-$HOME/Downloads/MISAKA-wt-b/lanes/evidence/combined-drill}
DM="$WT/audit-improve/dm.sh"
cmd=${1:-plan}; shift || true

plan() {
cat <<EOF
== combined drill plan (H' = $INT11_AT; IR fence $TIR_AT; ~125 s per DAA = 29 DAA/h measured)
  chain     salted testnet-12, flag days 6/10/14, IR $TIR_AT, IR-2 24, then --palw-drill-int11-at=$INT11_AT (all fences of the candidate at H', rho 100/250/1000 at $((INT11_AT+95))/$((INT11_AT+190))/$((INT11_AT+285)))
  nodes     new0 floor+heartbeat clock | new1..new3 seats | new4 head producer (class H) | new5 evaluator | new6 evaluator + producer of class win | new9 OUTSIDER seat
            (post-genesis bond 14, registered first) | old relay (D-M5, stopped when the crossing is done) | just-in-time: liars new7/new8, registrar reg (<= 9 processes)
  load      REAL-model producers: new4 (H: the tiny llama class), new6 (win: the full-weight tiny candidate), riders --palw-riders=$RIDERS on both (F-M1), the floor producer new0
            (floor attempts only when idle: A''); the D-M epochs add evaluation claims from ~255; DG-3/DG-5 add generative claims (toy-image, toy-embed, wide-embed)
  panels    7 genesis seats + the outsider; one panel per claim; the capacity sampler (every tick, from H' to the last window) records per claim bound/licensed/final DAA,
            the panel-bound backlog, each seat's live claims, the oldest unlicensed wait
  windows   rho25 [H'+10, H'+90), rho100 [H'+105, H'+185), rho250 [H'+200, H'+280), rho1000 [H'+295, H'+375) — 80 DAA each, 10 settle after the step
  8k model  ONE REAL producer (new6) runs with --palw-drill-real-submit-delay-s=$REAL_SUBMIT_DELAY_S (lane RS; 300 s until P2 measures the 8k p50): its attempts take that long
            to submit, so fast floor attempts fill their anticone meanwhile, as on live t12. new4 (the other REAL producer) stays fast. A binary without the flag runs
            without it and 'dc.sh dry' says so (the 8k emulation is then missing).
  verdicts  PANEL (dcwatch panel: PASS only if no window diverges, bind->licence p50 <= 6 / p95 <= 12 DAA, oldest wait <= 40, PanelUnavailable expiries 0; latency also from accepted)
            [acceptance->licence includes the ~20-DAA anchor delay: bind->licence is the PANEL metric] and SHARE (dcwatch share: per-window table over ELIGIBLE windows — both REAL producers up and not held — REAL+EXEC >= 90 %, heartbeats <= 10 %, floor only idle; the
            table adds the REAL attempts that turned RED, the kinds of BLUE blocks in their anticones before vs after H', DAA per hour and the longest slot gap per window);
            RECOVERY (dc-run.sh at DAA $RECOVERY_AT: both REAL producers stopped for 18 DAA (>= 2K, K = 3 idle slots), floors must resume after K slots with the DAA advancing,
            then restarted: floors must stop and REAL turn BLUE again; dcwatch recovery); D-M1..D-M6, DG-1..DG-7b as in the int-11 drill (dm.sh verdicts)
  gaps      from the int-11 drill, fixed in the kit: operator-id manifest step, registrar parse, D-M5 reconnect, pack-verify flag, embedding seed, drop regex;
            in the kit now: toy-embed is held by new4/new5/new6 only (GenClassNotReady, DG-2 then restarts the other IR holders with it) and the head's admission slot
            (the driver logs every class's slot at registration; the head registers before DAA 20 so slot 91 is reachable)
  not here  the lane harnesses audit-vertex/dv.sh (MESH=1), rfc6 shard.sh, scripts/misaka-palw-rfc1-drill.sh, misaka-palw-rfc2r-fence-drill.sh each bring their own chain
            (df.sh based, own fence flags): run them after this chain is down, on the same binary, one at a time — they cannot share this chain
  time      D-M part ends ~DAA 1010 (~36 h); the PANEL/SHARE windows end at H'+375 = $((INT11_AT+375)) (~$(( (INT11_AT+375)*125/3600 )) h): the headline verdicts exist at ~18 h, the recovery leg at DAA $((RECOVERY_AT+50)) (~$(( (RECOVERY_AT+50)*125/3600 )) h)
EOF
}

extra_checks() {
  local k="$BIN_DIR/kaspad" ok=1
  if "$k" --help 2>/dev/null | grep -q -- "--palw-drill-real-submit-delay-s"; then echo "  ok   kaspad lists --palw-drill-real-submit-delay-s (the 8k emulation runs on new6, delay $REAL_SUBMIT_DELAY_S s)"
  else echo "  FAIL kaspad lacks --palw-drill-real-submit-delay-s: lane RS's flag is not in this binary, the drill would run WITHOUT the 8k emulation"; ok=0; fi
  python3 "$C/dcwatch.py" selftest >/dev/null && echo "  ok   dcwatch selftest (windows, anticone kinds, recovery logic)" || { echo "  FAIL dcwatch selftest"; ok=0; }
  for f in --palw-riders --palw-drill-int11-at; do "$k" --help 2>/dev/null | grep -q -- "$f" && echo "  ok   kaspad lists $f" || { echo "  FAIL kaspad lacks $f"; ok=0; }; done
  grep -aq "LEGACY_HEARTBEAT" "$k" && echo "  ok   the node reports blockKind (REAL / FALLBACK / LEGACY_*) in getBlock verboseData" || { echo "  FAIL no blockKind in the binary: SHARE cannot be read"; ok=0; }
  python3 -m py_compile "$C/dcwatch.py" && echo "  ok   dcwatch.py compiles" || ok=0
  [ -x "$OLD_KASPAD_BIN" ] && echo "  ok   old relay $OLD_KASPAD_BIN" || { echo "  FAIL OLD_KASPAD_BIN missing"; ok=0; }
  local n; n=$("$k" --testnet --netsuffix=12 --appdir="$(mktemp -d)/app" "--palw-drill-genesis-salt=$(openssl rand -hex 32)" --palw-drill-fence-at=6 --palw-drill-fence2-at=10 --palw-drill-fence3-at=14 \
     --palw-drill-tir-at=$TIR_AT --palw-drill-tir2-at=24 --palw-drill-int11-at=$INT11_AT --palw-drill-write-keyring="$(mktemp -d)/kr" 2>&1 | tr -d '\r' | grep -ci "moved from") || true
  echo "  note the keyring export with the int11 flag reports $n 'moved from' lines (one per fence the flag moves)"
  [ "$ok" = 1 ]
}

case $cmd in
  plan) plan ;;
  dry) plan; bash "$DM" dry "$@" 2>&1 | grep -vE '^  ok ' | tail -80; echo "== combined-drill checks"; extra_checks; echo "== DRY RUN done" ;;
  up|status|verdicts|down|model|keys|plan-dm) bash "$DM" "$cmd" "$@" ;;
  gen) bash "$DM" gen "$@" ;;
  run) mkdir -p "$EVD"; nohup bash "$C/dc-run.sh" > "$EVD/run.out" 2>&1 & echo "dc-run.sh started (pid $!); timeline in $EVD/timeline.log" ;;
  redblue) python3 "$C/dcwatch.py" redblue --port "$((JSON_BASE+3))" --fence "$INT11_AT" "$@" ;;
  recovery) python3 "$C/dcwatch.py" recovery --port "$((JSON_BASE+3))" --state "${1:-$EVD/recovery.json}" ;;
  panel) python3 "$C/dcwatch.py" panel --work "$WORK_DIR" "$@" ;;
  share) python3 "$C/dcwatch.py" share --port "$((JSON_BASE+3))" --fence "$INT11_AT" --work "$WORK_DIR" --producers new4,new6 "$@" ;;
  evidence) mkdir -p "$EVD"; cp -R "$WORK_DIR"/verdict "$WORK_DIR"/capacity "$WORK_DIR"/drive.log "$WORK_DIR"/drive.out "$WORK_DIR"/milestones.tsv "$WORK_DIR"/memory.tsv "$WORK_DIR"/samples.tsv "$EVD"/ 2>/dev/null || true
            python3 "$C/dcwatch.py" panel --work "$WORK_DIR" > "$EVD/panel.txt" 2>&1 || true; bash "$0" share > "$EVD/share.txt" 2>&1 || true; echo "evidence in $EVD" ;;
  *) sed -n '2,16p' "$0"; exit 2 ;;
esac
