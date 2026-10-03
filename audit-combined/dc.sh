#!/usr/bin/env bash
# audit-combined/dc.sh — the COMBINED drill of the DAA-5,300 release candidate (rcore/int-12): ONE salted testnet-12 drill chain on this Mac that crosses the
# whole extended fence list with `--palw-drill-int11-at=H'` (every fence of the release at H', rho 100 / 250 / 1000 at H' + 95 / + 190 / + 285) on the
# binary the release ships, and measures the user's two headline verdicts under a sustained REAL-model claim load:
#   PANEL  verification does not jam        dcwatch.py panel   (bind->licence p50/p95, panel-bound backlog trend, seat oldest wait, F3 producer-hold
#                                                              events, PanelUnavailable expiries, receipts/h per seat; one row per rho window)
#   SHARE  PALW blocks dominate             dcwatch.py share   (sliding windows past H': REAL + EXEC >= 90 % of non-RED blocks, heartbeats <= 10 %,
#                                                              floor attempts only in idle windows; blockKind / laneClass from getBlock verboseData)
# It is audit-improve/dm.sh (the D-M and DG drills, the capacity sampler, the post-genesis bonds, the just-in-time liars) with INT11=1 INT12=1 and the load
# of this file: see `dc.sh plan`. Commands: plan | dry | up | run | status | verdicts | gates | panel | share | redblue | recovery | gen <dgN> | evidence | down.
#   BIN_DIR=<dir with kaspad misaka palw-class> [OLD_KASPAD_BIN=<the fleet's release at the crossing>] bash audit-combined/dc.sh dry
set -euo pipefail
C=$(cd "$(dirname "$0")" && pwd)
WT=$(cd "$C/.." && pwd)
export INT11=1 INT12=1
# The combined drill is shaped by its wall clock: the candidate ~14:00 JST 10-04, the drill done by ~08:00 10-05 (~17 h = ~490 DAA at ~125 s). H' = 26 (above the IR-2
# fence at 24; the head's Probation at ~92 is before the first measured window), three rho windows of 32 DAA (rho100 / 250 / 1000: rho25 has no REAL load yet and was
# measured by the int-11 drill), then the legs. D-M's epochs (~1,000 DAA) do not fit: they run as background load and are informational; only D-M5's crossing and
# DG-1 / DG-2 are crossing checks here.
export INT11_AT=${INT11_AT:-26}
export TIR_AT=${TIR_AT:-18}                       # the head registers at ~19: its admission slot (91) is >= 71 DAA later; at TIR 20 it missed the slot by one DAA and waited a period
export CAP_RHOS=${CAP_RHOS:-"rho100 rho250 rho1000"} CAP_WINDOW_DAA=${CAP_WINDOW_DAA:-32} CAP_SETTLE_DAA=${CAP_SETTLE_DAA:-8} DM_NO_LIARS=${DM_NO_LIARS:-1}
# the PANEL windows, each inside its rho regime (rho100 from H'+95, rho250 from H'+190, rho1000 from H'+285), placed so that the floor-policy legs run BETWEEN them:
# stale-only first (from the first REAL attempt), then both producers (RED -> BLUE after the idle stretch), the rho100 window, the rho250 window, leg A, rho1000.
export CAP_AT=${CAP_AT:-"rho100:170 rho250:$((INT11_AT+190+CAP_SETTLE_DAA)) rho1000:$((INT11_AT+285+CAP_SETTLE_DAA))"}
export OUTSIDER=${OUTSIDER:-1} RIDERS=${RIDERS:-4}
export GEN_PARTIAL_CLASS=${GEN_PARTIAL_CLASS-toy-embed} GEN_PARTIAL_HOLDERS=${GEN_PARTIAL_HOLDERS-"new4 new5 new6"}   # DG-2: toy-embed on three nodes only
export WORK_DIR=${WORK_DIR:-$HOME/.misaka-palw-combined-drill}
export P2P_BASE=${P2P_BASE:-51200} BORSH_BASE=${BORSH_BASE:-52200} JSON_BASE=${JSON_BASE:-53200} EVM_BASE=${EVM_BASE:-54200} GRPC_BASE=${GRPC_BASE:-50200}
export REAL_SUBMIT_DELAY_S=${REAL_SUBMIT_DELAY_S-340}   # lane RS's --palw-drill-real-submit-delay-s on ONE REAL producer (new6): the 8k model's inference time (P2 live: p50 342 s, p95 418 s; floors 3 per slot)
export K_SLOTS=${K_SLOTS:-20} PROBE_SLOTS=${PROBE_SLOTS:-8} COOLDOWN=${COOLDOWN:-20}   # ADR-0165 (RS 083a93e76): floor_idle_slots 20, probe_slots 8, probe_cooldown_slots 20 (from the END of an unanswered probe; RED never extends)
export XB_ORDER=${XB_ORDER:-"14 15"}                       # the outsider's bond (14), then the external floor producer's (15); no DG liars, no D-M3 liars in this drill
export STALE_END_DAA=${STALE_END_DAA:-150}                 # leg S (first): only new6 makes REAL attempts, each held STALE_DELAY_S so it lands RED; new4 is a seat only; ends here (>= 2 probes)
export STALE_DELAY_S=${STALE_DELAY_S:-1500}                # ~12 slots: a stale attempt always lands RED
export RESTART1_DAA=${RESTART1_DAA:-$STALE_END_DAA}        # leg B1: both REAL producers on the normal delay (the state is Idle: RED -> BLUE, then Normal)
export RECOVERY_AT=${RECOVERY_AT:-$((INT11_AT+190+CAP_SETTLE_DAA+CAP_WINDOW_DAA+2))}   # leg A: after the rho250 window
export STOP_DAA=${STOP_DAA:-$((K_SLOTS+8))}                # leg A: both REAL producers down K + 8 slots (shortened from 2K: floors must resume only after K idle slots)
export POST_DAA=${POST_DAA:-$((K_SLOTS+8))}                # leg B2: both back; RED -> BLUE observed; ends before the rho1000 window
export X_DAA=${X_DAA:-0}                                   # leg X (a policy-ignoring floor producer, report-only): off; X_DAA=24 with EXT_FLOOR_FLAG turns it on
export REGISTER_LATE=${REGISTER_LATE:-"lose:$((STALE_END_DAA+12))"}   # G-A1: a class registered with REAL flowing and floors held (the state is Normal), Probation within 2 audit periods (200 DAA)
export O_DAA=${O_DAA:-0}                                   # leg O (report-only): the operator producers stopped, only the non-operator REAL flows; O_DAA=80 turns it on (bind wait expected near 60 slots)
# RS (int-12 @ 4d5f97d58): --palw-drill-int11-at ALREADY arms palw_floor_reserve_v1 and palw_real_clock_tick_v1 (entries 22, 23 of the 28; P2's palw_anchor_window_v1 rides it as entry 24 of 29).
# --palw-drill-useful-work-at must NOT be passed with it: the pair is not refused and it would re-move those two fences to the later call's height.
if [ -n "${USEFUL_WORK_AT:-}" ]; then echo "REFUSED: USEFUL_WORK_AT is for a drill of the two useful-work fences without the rest; the combined drill's --palw-drill-int11-at already arms them (a second flag would move them again)" >&2; exit 2; fi
export ANCHOR_DUTY_AFTER_SLOTS=${ANCHOR_DUTY_AFTER_SLOTS:-}   # RS's --palw-drill-anchor-duty-after-slots=N on the operator floor producer (release: 30); unset = the release's value (G-A3 is measured under it)
export HOLD_PATTERN=${HOLD_PATTERN:-'\[palw-producer\] holding:[^\n]*idle-only fallback'}   # RS's hold line (info level, never 'NOT PRODUCING')
export EXT_FLOOR_FLAG=${EXT_FLOOR_FLAG:---palw-drill-floor-ignore-policy}
export GEN_DIR=${GEN_DIR:-$HOME/Downloads/MISAKA-wt-b/gen-drill-classes}
export OLD_KASPAD_BIN=${OLD_KASPAD_BIN:-$HOME/Downloads/MISAKA-wt-b/lifecycle-run/bin/2483c570cea4/kaspad}
EVD=${EVD:-$HOME/Downloads/MISAKA-wt-b/lanes/evidence/combined-drill}
DM="$WT/audit-improve/dm.sh"
cmd=${1:-plan}; shift || true

plan() {
local R100=$((INT11_AT+95)) R250=$((INT11_AT+190)) R1000=$((INT11_AT+285))
local END_DAA=$((INT11_AT + 285 + CAP_SETTLE_DAA + CAP_WINDOW_DAA + 6))
cat <<EOF
== combined drill plan (H' = $INT11_AT; IR fence $TIR_AT; ~125 s per DAA = ~29 DAA/h measured; ends at ~DAA $END_DAA = ~$((END_DAA*125/3600)).$(( (END_DAA*125%3600)*10/3600 )) h after the chain starts)
  chain     salted testnet-12, flag days 6/10/14, IR $TIR_AT, IR-2 24, then --palw-drill-int11-at=$INT11_AT (every fence of the candidate at H' — the useful-work pair palw_floor_reserve_v1 / palw_real_clock_tick_v1 and
            P2's palw_anchor_window_v1 included, so no --palw-drill-useful-work-at; rho 100 / 250 / 1000 at $R100 / $R250 / $R1000)
  nodes     new0 floor + heartbeat clock | new1..new3 seats | new4 head (seat; REAL producer, class H, fast, from leg B1) | new5 evaluator | new6 evaluator + REAL producer of class win (the 8k emulation) |
            new9 OUTSIDER seat (post-genesis bond 14) | extfloor (bond 15; leg X only) | the old relay int-10.3 for FORK-a / D-M5 (stopped when done) | joiner (FORK-b) | registrar JIT (<= 9 processes)
  load      REAL producers new4 + new6 with --palw-riders=$RIDERS; new6 holds every attempt --palw-drill-real-submit-delay-s=$REAL_SUBMIT_DELAY_S (P2's live 8k p50 342 s, p95 418 s; floors 3 per slot) so fast
            floors fill its anticone while it infers, as on live t12 (added only if the binary lists the flag; 'dc.sh dry' says); D-M's epochs run as background load
  ORDER     (fence-crossing gates first, so they can be published early)
            1  DAA $((INT11_AT-6))..$((INT11_AT+12))  the crossing: D-M5 (the old relay dropped AT the crossing = FORK-a), DG-1, DG-2, FORK-c (new2 partitioned across the fence, rejoins)
            2  DAA $((INT11_AT+12))     FORK-b: a fresh node IBDs from genesis across the fence
            3  from the first REAL attempt (~DAA 90) to DAA $STALE_END_DAA   leg S: ONLY new6 produces, every attempt held ${STALE_DELAY_S}s (RED-only): STALE (first cycle: a probe, floors valid again after $PROBE_SLOTS)
               and COOLDOWN (>= 2 probes spaced >= $COOLDOWN); the chain starts Idle, so FLOOR and STATES (Idle, Probe) fill in here
            4  DAA $STALE_END_DAA   leg B1: new4 (fast) and new6 (normal delay) on: the state is Idle, so the first REAL attempt may be RED and a BLUE one follows inside the probe: RED>BLUE1; then Normal
            5  rho100 window [$(echo "$CAP_AT" | tr ' ' '\n' | sed -n 's/^rho100://p'), +$CAP_WINDOW_DAA)   rho250 window [$((R250+CAP_SETTLE_DAA)), +$CAP_WINDOW_DAA)   (PANEL, BLUE, SHARE)
            6  DAA $RECOVERY_AT   leg A: both REAL producers down $STOP_DAA slots (K = floor_idle_slots $K_SLOTS: floors must resume only after K idle slots, the DAA advances), then back: RECOVERY, RED>BLUE2
            7  rho1000 window [$((R1000+CAP_SETTLE_DAA)), +$CAP_WINDOW_DAA)
            (gates 1-4 are final by ~DAA $((STALE_END_DAA+30)) = ~$(( (STALE_END_DAA+30)*125/3600 )) h; the PANEL / BLUE windows by DAA $((R250+CAP_SETTLE_DAA+CAP_WINDOW_DAA)))
  the floor rule (5,300 as RS has it): floors are NOT rejected at the header; the fold refuses a floor unless the per-branch state is Idle (no claim, reward or weight) and honest floor producers hold by
            policy. Idle / Probe / Normal: floor_idle_slots $K_SLOTS, probe_slots $PROBE_SLOTS, probe_cooldown $COOLDOWN; a BLUE REAL from any mode -> Normal; a RED REAL accepted in Idle -> Probe iff the cooldown (counted from the END of an unanswered probe) has passed; Normal lasts to last_blue + $K_SLOTS, floors again from last_blue + $((K_SLOTS+1));
            RED never extends. The nodes' [palw-floor-state] logs are the source where they exist (a floor merged by a block with a logged line counts as outside Idle only if BOTH its time-advanced stored state and the new state say so — RS's recipe: the block's own event applies to the floors merged after it); dcwatch's model (floor_states) reconstructs it
            otherwise and cross-checks the logs.
  RELEASE GATES (all must PASS to ship; 'dc.sh gates', exit 0 / 1 / 3):
            PANEL (no window diverges, bind->licence p50 <= 6 / p95 <= 12 DAA, oldest wait <= 40, PanelUnavailable expiries 0; acceptance->licence includes the ~20-DAA anchor delay: bind->licence is the metric)
            BLUE (REAL attempts BLUE >= 90 % in every eligible window past H' — both REAL producers up and not held — under the 340 s delay, all producers on the release policy) and DELAY (injection on new6's argv)
            FLOOR (floors outside Idle earn nothing: no claim row names such a floor block; the compliant producers log holds — HOLD_PATTERN) | RED>BLUE1 and RED>BLUE2 (after an idle stretch the first REAL attempt may
            be RED, a BLUE one follows inside the probe) | STALE and COOLDOWN (RED-only REAL cannot keep floors refused beyond probe_slots; probes no more often than every cooldown) | RECOVERY (leg A)
            DAA and STATES (a block at every DAA, no slot gap over 4 slots, through every state and through the stops / restarts)
            FORK-a (old and new refuse each other past the fence: the new node drops the old peer AT the crossing — the re-judgement; a restart is not what shows it) | FORK-b (a fresh node IBDs from genesis
            across the fence) | FORK-c (new2 partitioned across the fence rejoins and reorganises onto the chain)
            + per-lane verdicts: D-M5's crossing, DG-1, DG-2 here; the lane pre-checks' verdict.txt (rfc7v, rfc6s, rfc1, rfc2r) from tonight
  anchor fix (P2, anchor/window)   G-A1: a class registered with REAL flowing and floors held (the driver registers `lose` at DAA $((STALE_END_DAA+12)), in Normal) reaches Probation within 2 audit periods (<= 200 DAA);
            G-A2: the execution lane's schedule seeding >= 95 % of the snapshots created past the fence (getPalwRoundLane per span, sampled every 30 s into roundlane.tsv; an APPROXIMATION of the fold's queue
            rule — the raw samples are kept for P2's exact replay); G-A3: the panel bind wait (claim accepted -> PanelBound) of REAL claims p95 <= 20 DAA, split operator bond / non-operator bond (new9, the
            outsider, makes REAL attempts of class win from leg B1 on; it is stopped with the operator producers in leg A). Report-only leg O (O_DAA=$O_DAA, off by default): the operator producers stopped, only the
            non-operator REAL flowing: its bind wait is expected near 60 slots (the beacon-floor policy's cap); it needs ~80 DAA beyond the ~$(( (INT11_AT+285+CAP_SETTLE_DAA+CAP_WINDOW_DAA+6)*125/3600 )) h of the rest.
  test-level, not drill   FORK-d (IBD via the pruning proof across the fence): RS's T49-style carriage test (capture in Probe/Normal -> import -> identical decisions) and the combined-fence test with the floor rule live
  report    (never a gate) the user's metrics: REAL attempts BLUE rate, REAL share of the selected chain, REAL work reaching Final (claims >= 200 DAA old), recovery time; leg X (off unless X_DAA > 0): one policy-IGNORING floor producer
            ($EXT_FLOOR_FLAG, the extfloor node) — how many REAL attempts it turns RED
  goal      (supply-bound, reported, never a gate) SHARE: REAL + EXEC >= 90 % / heartbeat <= 10 % of the consensus blocks per eligible window, PASS / FAIL as measured; REAL vs heartbeat vs floor per window
  dropped   D-M1..D-M4, D-M6 and DG-3..DG-7b need ~1,000 DAA: informational only; the lane harnesses (audit-vertex dv.sh MESH=1, rfc6 shard.sh, rfc1, rfc2r) cannot share this chain
  needs RS  (given, ADR-0165 083a93e76: the three flags, the hold line, the [palw-floor-state] line); K = floor_idle_slots 20, probe 8, cooldown 20 in the binary ('dc.sh dry' compares the source tree's constants)
EOF
}

extra_checks() {
  local k="$BIN_DIR/kaspad" ok=1
  local ef; ef=${EXT_FLOOR_FLAG:-$("$k" --help 2>/dev/null | grep -oE -- '--palw-drill-[a-z0-9-]*floor[a-z0-9-]*' | grep -vE -- '-at$|reserve' | head -1)}
  [ -n "$ef" ] && echo "  ok   kaspad lists a policy-ignoring floor flag: $ef (leg X, report-only)" || echo "  note no drill flag to make a floor producer ignore the policy: leg X (report-only) will be skipped"
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
  local ks; ks=$(grep -o "PALW_REAL_IDLE_K_SLOTS_V1: u64 = [0-9]*" "$WT/consensus/core/src/palw_real_share_v1.rs" 2>/dev/null | grep -o "[0-9]*$" || true)
  if [ -n "$ks" ] && [ "$ks" != "$K_SLOTS" ]; then echo "  note the source tree under $WT has K = $ks, this plan K_SLOTS = $K_SLOTS: the recovery leg is right only on a binary with K = $K_SLOTS"; else echo "  ok   K = ${ks:-?} matches K_SLOTS"; fi
  [ "$ok" = 1 ]
}

case $cmd in
  plan) plan ;;
  dry) plan; bash "$DM" dry "$@" 2>&1 | grep -vE '^  ok ' | cut -c1-400 | tail -80; echo "== fence heights the int11 flag sets (from the keyring export; useful-work = palw_floor_reserve_v1 / palw_real_clock_tick_v1)"; bash "$DM" dry "$@" 2>&1 | grep "keyring manifest names the fences" | cut -c1-900; echo "== combined-drill checks"; extra_checks; echo "== DRY RUN done" ;;
  up) REAL_SUBMIT_DELAY_S=$STALE_DELAY_S HEAD_PRODUCE=0 bash "$DM" up "$@" ;;      # leg S first: new6 stale, new4 a seat only
  status|verdicts|down|model|keys|plan-dm) bash "$DM" "$cmd" "$@" ;;
  gen) bash "$DM" gen "$@" ;;
  run) mkdir -p "$EVD"; nohup bash "$C/dc-run.sh" > "$EVD/run.out" 2>&1 & echo "dc-run.sh started (pid $!); timeline in $EVD/timeline.log" ;;
  redblue) python3 "$C/dcwatch.py" redblue --port "$((JSON_BASE+3))" --fence "$INT11_AT" "$@" ;;
  gates) python3 "$C/dcwatch.py" gates --port "$((JSON_BASE+3))" --fence "$INT11_AT" --work "$WORK_DIR" --producers new4,new6,new9 --evd "$EVD" --state "$EVD/recovery.json" --stale "$EVD/stale.json" --restart1 "$EVD/restart1.json" --k "$K_SLOTS" --idle-slots "$K_SLOTS" --probe-slots "$PROBE_SLOTS" --cooldown "$COOLDOWN" "$@" ;;
  recovery) python3 "$C/dcwatch.py" recovery --port "$((JSON_BASE+3))" --state "${1:-$EVD/recovery.json}" --k "$K_SLOTS" ;;
  panel) python3 "$C/dcwatch.py" panel --work "$WORK_DIR" "$@" ;;
  share) python3 "$C/dcwatch.py" share --port "$((JSON_BASE+3))" --fence "$INT11_AT" --work "$WORK_DIR" --producers new4,new6,new9 "$@" ;;
  evidence) mkdir -p "$EVD"; cp -R "$WORK_DIR"/verdict "$WORK_DIR"/capacity "$WORK_DIR"/drive.log "$WORK_DIR"/drive.out "$WORK_DIR"/milestones.tsv "$WORK_DIR"/memory.tsv "$WORK_DIR"/samples.tsv "$EVD"/ 2>/dev/null || true
            python3 "$C/dcwatch.py" panel --work "$WORK_DIR" > "$EVD/panel.txt" 2>&1 || true; bash "$0" share > "$EVD/share.txt" 2>&1 || true; bash "$0" gates > "$EVD/gates.txt" 2>&1 || true; echo "evidence in $EVD" ;;
  *) sed -n '2,16p' "$0"; exit 2 ;;
esac
