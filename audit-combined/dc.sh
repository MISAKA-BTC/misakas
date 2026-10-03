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
export OUTSIDER=${OUTSIDER:-1} RIDERS=${RIDERS:-4}
export GEN_PARTIAL_CLASS=${GEN_PARTIAL_CLASS-toy-embed} GEN_PARTIAL_HOLDERS=${GEN_PARTIAL_HOLDERS-"new4 new5 new6"}   # DG-2: toy-embed on three nodes only
export WORK_DIR=${WORK_DIR:-$HOME/.misaka-palw-combined-drill}
export P2P_BASE=${P2P_BASE:-51200} BORSH_BASE=${BORSH_BASE:-52200} JSON_BASE=${JSON_BASE:-53200} EVM_BASE=${EVM_BASE:-54200} GRPC_BASE=${GRPC_BASE:-50200}
export REAL_SUBMIT_DELAY_S=${REAL_SUBMIT_DELAY_S-340}   # lane RS's --palw-drill-real-submit-delay-s on ONE REAL producer (new6): the 8k model's inference time (P2 live: p50 342 s, p95 418 s; floors 3 per slot)
export K_SLOTS=${K_SLOTS:-20} PROBE_SLOTS=${PROBE_SLOTS:-6} COOLDOWN=${COOLDOWN:-20}   # the redesigned floor rule: floor_idle_slots 20, probe_slots 6, probe_cooldown 20 (RED never extends)
export XB_ORDER=${XB_ORDER:-"14 15"}                       # the outsider's bond (14), then the external floor producer's (15); no DG liars, no D-M3 liars in this drill
LAST_WINDOW_END=$((INT11_AT + 285 + CAP_SETTLE_DAA + CAP_WINDOW_DAA))
export RECOVERY_AT=${RECOVERY_AT:-$((LAST_WINDOW_END + 2))}   # the legs start right after the last rho window
export STOP_DAA=${STOP_DAA:-$((2*K_SLOTS+2))}              # leg A: both REAL producers down >= 2K slots; floors must resume only after K idle slots
export STALE_DAA=${STALE_DAA:-$((COOLDOWN+PROBE_SLOTS+20))} STALE_DELAY_S=${STALE_DELAY_S:-1500}   # leg S: only new6, every REAL attempt held 1,500 s (~12 slots) so it lands RED; >= 2 probes
export POST_DAA=${POST_DAA:-$((K_SLOTS+8))} X_DAA=${X_DAA:-24}                # leg B: both back at the normal delay; RED -> BLUE recovery observed
export GEN_DIR=${GEN_DIR:-$HOME/Downloads/MISAKA-wt-b/gen-drill-classes}
export OLD_KASPAD_BIN=${OLD_KASPAD_BIN:-$HOME/Downloads/MISAKA-wt-b/lifecycle-run/bin/2483c570cea4/kaspad}
EVD=${EVD:-$HOME/Downloads/MISAKA-wt-b/lanes/evidence/combined-drill}
DM="$WT/audit-improve/dm.sh"
cmd=${1:-plan}; shift || true

plan() {
local R100=$((INT11_AT+95)) R250=$((INT11_AT+190)) R1000=$((INT11_AT+285))
local END_DAA=$((RECOVERY_AT + STOP_DAA + STALE_DAA + POST_DAA + X_DAA + 6))
cat <<EOF
== combined drill plan (H' = $INT11_AT; IR fence $TIR_AT; ~125 s per DAA = ~29 DAA/h measured)
  chain     salted testnet-12, flag days 6/10/14, IR $TIR_AT, IR-2 24, then --palw-drill-int11-at=$INT11_AT (every fence of the candidate at H'; rho 100 / 250 / 1000 at $R100 / $R250 / $R1000)
  nodes     new0 floor + heartbeat clock | new1..new3 seats | new4 head producer (REAL, class H, fast) | new5 evaluator | new6 evaluator + REAL producer of class win (the 8k emulation) |
            new9 OUTSIDER seat (post-genesis bond 14) | extfloor EXTERNAL floor producer (bond 15; a floor producer that ignores the idle rule where the build has a drill flag for it) |
            the old relay for D-M5's crossing (stopped when it is done) | registrar JIT (<= 9 processes)
  load      REAL-model producers new4 + new6 with --palw-riders=$RIDERS; new6 holds every attempt --palw-drill-real-submit-delay-s=$REAL_SUBMIT_DELAY_S (P2's live 8k p50 342 s, p95 418 s; floors 3 per slot) so
            fast floors fill its anticone while it infers, as on live t12 (the flag is added only if the binary lists it; 'dc.sh dry' says); D-M's epochs run as background load
  windows   PANEL is measured at rho100 [$((R100+CAP_SETTLE_DAA)), $((R100+CAP_SETTLE_DAA+CAP_WINDOW_DAA))), rho250 [$((R250+CAP_SETTLE_DAA)), $((R250+CAP_SETTLE_DAA+CAP_WINDOW_DAA))), rho1000 [$((R1000+CAP_SETTLE_DAA)), $((R1000+CAP_SETTLE_DAA+CAP_WINDOW_DAA))) (rho25: no REAL load yet; measured by the int-11 drill)
  legs      from DAA $RECOVERY_AT: A both REAL producers down $STOP_DAA slots (>= 2K, K = floor_idle_slots $K_SLOTS) | S only new6, back with every REAL attempt held ${STALE_DELAY_S}s (stale, always RED) for $STALE_DAA slots |
            B both back at the normal delay, $POST_DAA slots observed | X report-only: a policy-ignoring floor producer, $X_DAA slots; the drill ends at ~DAA $END_DAA (~$((END_DAA*125/3600)).$(( (END_DAA*125%3600)*10/3600 )) h after the chain starts: it must be up by ~15:00 to finish by 08:00)
  the floor rule (5,300 as RS has it): floors are NOT rejected at the header. The fold refuses a floor unless the per-branch state is Idle (no claim, no reward, no weight) and honest floor producers hold by policy.
            States Idle / Probe / Normal: floor_idle_slots $K_SLOTS, probe_slots $PROBE_SLOTS, probe_cooldown $COOLDOWN; RED never extends. dcwatch carries ONE model of it (floor_states: Normal after a BLUE REAL, Idle after
            $K_SLOTS quiet slots, a REAL attempt in Idle opens a $PROBE_SLOTS-slot probe if the last began >= $COOLDOWN ago): the gates read the DAG and the claim rows against it, so check the model against the frozen text.
  RELEASE GATES (all must PASS to ship; 'dc.sh gates', exit 0 / 1 / 3):
            1 PANEL (no window diverges, bind->licence p50 <= 6 / p95 <= 12 DAA, oldest wait <= 40, PanelUnavailable expiries 0; acceptance->licence includes the ~20-DAA anchor delay: bind->licence is the metric)
            2 BLUE (REAL attempts BLUE >= 90 % in every eligible window past H' — both REAL producers up and not held — under the 340 s delay, all producers on the release policy) and DELAY (the injection is on new6's argv)
            3 FLOOR (floors outside Idle earn nothing: no claim row names such a floor block; the compliant producers' logs hold lines — HOLD_PATTERN sets the wording) | RED>BLUE (leg B: after the idle stretch the first
              REAL attempt may be RED, a BLUE one follows inside the probe) | STALE (leg S: RED-only REAL attempts cannot keep floors refused beyond probe_slots per cooldown; >= 2 probes spaced >= cooldown)
            4 RECOVERY (leg A: floors resume only after K idle slots) | DAA and STATES (a block at every DAA, no slot gap over 4 slots, through every state and through the stop / restart)
            5 FORK-a old and new refuse each other past the fence: the new node drops the old peer AT the crossing (the int-10.7 re-judgement; a restart is not what shows it) | FORK-b a fresh node IBDs from genesis across
              the fence | FORK-c new2 is partitioned across the fence and rejoins: it reorganises onto the chain | FORK-d IBD via the pruning proof: NOT REACHABLE here (pruning depth is thousands of blocks)
            + per-lane verdicts: D-M5's crossing, DG-1, DG-2 here; the lane pre-checks' verdict.txt (rfc7v, rfc6s, rfc1, rfc2r) from tonight
  report    (never a gate) leg X: ONE policy-IGNORING floor producer (the extfloor node on bond 15, a drill flag RS may add: EXT_FLOOR_FLAG) for $X_DAA slots — how many REAL attempts it turns RED against the slots before;
            the user's metrics: REAL attempts BLUE rate, REAL share of the selected chain, REAL work reaching Final (claims >= 200 DAA old), recovery time (slots after the restart to the first REAL / BLUE REAL attempt and
            to the last floor standing; slots after the stop to the first floor)
  goal      (supply-bound, reported, never a gate) SHARE: REAL + EXEC >= 90 % / heartbeat <= 10 % of the consensus blocks per eligible window, PASS / FAIL as measured; REAL vs heartbeat vs floor per window
  dropped   D-M1..D-M4, D-M6 and DG-3..DG-7b need ~1,000 DAA: informational only (no liars, no DG liars); the lane harnesses (audit-vertex dv.sh MESH=1, rfc6 shard.sh, rfc1, rfc2r) cannot share this chain
  needs RS  (1) a drill flag that makes a floor producer ignore the policy (leg X only; EXT_FLOOR_FLAG), (2) the real-submit-delay flag, (3) the producer's hold wording (HOLD_PATTERN), (4) K = floor_idle_slots 20 in the
            binary ('dc.sh dry' compares the source tree's constant to K_SLOTS)
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
  dry) plan; bash "$DM" dry "$@" 2>&1 | grep -vE '^  ok ' | tail -80; echo "== combined-drill checks"; extra_checks; echo "== DRY RUN done" ;;
  up|status|verdicts|down|model|keys|plan-dm) bash "$DM" "$cmd" "$@" ;;
  gen) bash "$DM" gen "$@" ;;
  run) mkdir -p "$EVD"; nohup bash "$C/dc-run.sh" > "$EVD/run.out" 2>&1 & echo "dc-run.sh started (pid $!); timeline in $EVD/timeline.log" ;;
  redblue) python3 "$C/dcwatch.py" redblue --port "$((JSON_BASE+3))" --fence "$INT11_AT" "$@" ;;
  gates) python3 "$C/dcwatch.py" gates --port "$((JSON_BASE+3))" --fence "$INT11_AT" --work "$WORK_DIR" --producers new4,new6 --evd "$EVD" --state "$EVD/recovery.json" --stale "$EVD/stale.json" --k "$K_SLOTS" --idle-slots "$K_SLOTS" --probe-slots "$PROBE_SLOTS" --cooldown "$COOLDOWN" "$@" ;;
  recovery) python3 "$C/dcwatch.py" recovery --port "$((JSON_BASE+3))" --state "${1:-$EVD/recovery.json}" --k "$K_SLOTS" ;;
  panel) python3 "$C/dcwatch.py" panel --work "$WORK_DIR" "$@" ;;
  share) python3 "$C/dcwatch.py" share --port "$((JSON_BASE+3))" --fence "$INT11_AT" --work "$WORK_DIR" --producers new4,new6 "$@" ;;
  evidence) mkdir -p "$EVD"; cp -R "$WORK_DIR"/verdict "$WORK_DIR"/capacity "$WORK_DIR"/drive.log "$WORK_DIR"/drive.out "$WORK_DIR"/milestones.tsv "$WORK_DIR"/memory.tsv "$WORK_DIR"/samples.tsv "$EVD"/ 2>/dev/null || true
            python3 "$C/dcwatch.py" panel --work "$WORK_DIR" > "$EVD/panel.txt" 2>&1 || true; bash "$0" share > "$EVD/share.txt" 2>&1 || true; bash "$0" gates > "$EVD/gates.txt" 2>&1 || true; echo "evidence in $EVD" ;;
  *) sed -n '2,16p' "$0"; exit 2 ;;
esac
