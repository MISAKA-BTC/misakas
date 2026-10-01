#!/usr/bin/env bash
# misaka-palw-int10-flagday.sh — the DAA-3,600 flag day (palw_tir_fence2 ALONE: the model court window stays dormant on
# testnet-12, the coordinator's decision of 2026-10-01) crossed on the SHIPPING binary, in the ONE salted testnet-12
# drill chain audit-tir/df.sh runs (the int-10 kit, 2026-10-01).
#
# Derived from Phase F's D-F4 step script (misaka-palw-tir-drill-df4.sh), whose crossing is the IR fence's: here the
# crossing is TIR2_AT (`--palw-drill-tir2-at`), and the old relay is the fleet's int-8 release — it HAS the IR fence
# (--palw-drill-tir-at), so it follows the new nodes through TIR_AT and up to TIR2_AT − 1; its fork id parts from theirs
# at TIR2_AT. This script starts nothing; it reads logs and RPC.
#
# What it shows, each from log bytes and RPC (STAGE 1, the rollout gate):
#   old      the old relay runs the drill's flag days and the IR fence (its log names palw_tir_v1 moved) and NOT
#            palw_tir_fence2; the new nodes' banners name palw_tir_fence2 at TIR2_AT and NO palw_model_court_window
#   below    at DAA < TIR2_AT − 2 new0 and the old relay report ONE sink at ONE DAA: the old release validated every
#            block the new binary built below the fence, so the new binary's rules below it are the old release's
#   cross    new0's DAA passes TIR2_AT + 5 with blocks validated above it, and the fork id separates the two:
#            "Fork-id mismatch … this node has crossed fence TIR2_AT" (or the old relay's refusal of new0), after which
#            the old relay's DAA stops (≤ TIR2_AT + 3) while new0's goes on
#   classes  a small IR class registered PAST the fence is ADMITTED under fence2's sizing (admission v10's range twin:
#            the same bounds as the release's element twin, in far fewer steps) by EVERY running new node:
#            getPalwClasses on each lists it with registeredDaa >= TIR2_AT, and ONE registeredDaa, canonicalLeaves
#            and artifactRoot everywhere, the root the artifact declared; the class registered BELOW the fence is the
#            twin (registeredDaa < TIR2_AT, the release's sizing), accepted the same way; no node logged a
#            "PALW model court window" line (the window is armed nowhere). Its DA ladder as fence2 defines it — the
#            class's own, 2^40 on testnet-12, where the release checked at 2^22 — is read by `df.sh bdc` in Stage 2
#            (a class this small cannot tell the two ladders apart; the consensus tests hold the rest)
#
#   misaka-palw-int10-flagday.sh dry | run | old | below | cross | classes
# Exit 0 PASS, 1 FAIL, 3 INCOMPLETE (not reachable yet: `below` past TIR2_AT − 2; `classes` before the small class is
# registered past the fence).
#
# ENV (the drill layout, audit-tir/lib-df.sh's): SALT (else $WORK_DIR/SALT), WORK_DIR, KASPAD_BIN, OLD_KASPAD_BIN, CLI_BIN,
#   TIR_AT, TIR2_AT, FENCE_AT/FENCE2_AT/FENCE3_AT, NEW0_P2P/NEW0_RPC/NEW0_LOG, OLD_P2P/OLD_RPC/OLD_LOG, JSON_BASE,
#   PAST_CLASS (small|ir: which class registers past the fence; the other registers below it), STALL_WAIT (3600 s),
#   STEP_WAIT (43200 s), CLASSES_WAIT (1800 s: how long `classes` waits for the registration to reach every node).
set -euo pipefail

WORK_DIR="${WORK_DIR:-$HOME/.misaka-palw-int10-drill}"
KASPAD_BIN="${KASPAD_BIN:-}"
OLD_KASPAD_BIN="${OLD_KASPAD_BIN:-}"
CLI_BIN="${CLI_BIN:-$(dirname "${KASPAD_BIN:-/nonexistent/kaspad}")/misaka}"
TIR_AT="${TIR_AT:-20}"
TIR2_AT="${TIR2_AT:-50}"
FENCE_AT="${FENCE_AT:-6}"
FENCE2_AT="${FENCE2_AT:-10}"
FENCE3_AT="${FENCE3_AT:-14}"
NEW0_P2P="${NEW0_P2P:-56100}"
NEW0_RPC="${NEW0_RPC:-57100}"
NEW0_LOG="${NEW0_LOG:-$WORK_DIR/new0/kaspad.out}"
OLD_P2P="${OLD_P2P:-56108}"
OLD_RPC="${OLD_RPC:-57108}"
OLD_LOG="${OLD_LOG:-$WORK_DIR/old/kaspad.out}"
JSON_BASE="${JSON_BASE:-58100}"
RPC_PY="${RPC_PY:-$(cd "$(dirname "$0")/.." && pwd)/audit-tir/rpc.py}"
CLASSROWS_PY="${CLASSROWS_PY:-$(dirname "$RPC_PY")/classrows.py}"
STALL_WAIT="${STALL_WAIT:-3600}"
STEP_WAIT="${STEP_WAIT:-43200}"
UHOME="$WORK_DIR/userhome"
MILESTONES="$WORK_DIR/df-milestones.tsv"
EVIDENCE="$WORK_DIR/evidence/flagday"
PUBLIC_T12_PORTS="26311 26210 27210 28210 8545 26312 26313 26314 26321 26323 26324 26331 26333 26334 26341 26343 26344 26351 26353 26354"

RC_FAIL=1
RC_INCOMPLETE=3

log() { printf '[int10 flagday] %s\n' "$*" >&2; }
die() { log "FAIL: $*"; exit "$RC_FAIL"; }
incomplete() { log "INCOMPLETE: $*"; exit "$RC_INCOMPLETE"; }

load_salt() {
  SALT="${SALT:-}"
  [ -n "$SALT" ] || SALT="$(tr -d ' \n' < "$WORK_DIR/SALT" 2>/dev/null || true)"
  [[ "$SALT" =~ ^[0-9a-f]{64}$ ]] || die "no drill salt (SALT, or $WORK_DIR/SALT)"
}

need_env() {
  load_salt
  local v
  for v in TIR_AT TIR2_AT FENCE_AT FENCE2_AT FENCE3_AT NEW0_P2P NEW0_RPC OLD_P2P OLD_RPC JSON_BASE; do
    [[ "${!v}" =~ ^[0-9]+$ ]] || die "$v must be a number"
  done
  [ "$FENCE_AT" -gt 0 ] && [ "$FENCE_AT" -lt "$FENCE2_AT" ] && [ "$FENCE2_AT" -lt "$FENCE3_AT" ] && [ "$FENCE3_AT" -lt "$TIR_AT" ] \
    && [ "$TIR_AT" -lt "$TIR2_AT" ] \
    || die "the heights must satisfy 0 < FENCE_AT < FENCE2_AT < FENCE3_AT < TIR_AT < TIR2_AT (got $FENCE_AT $FENCE2_AT $FENCE3_AT $TIR_AT $TIR2_AT)"
  local p q
  for p in "$NEW0_P2P" "$NEW0_RPC" "$OLD_P2P" "$OLD_RPC" "$JSON_BASE"; do
    for q in $PUBLIC_T12_PORTS; do [ "$p" != "$q" ] || die "port $p is a public testnet-12 default"; done
  done
}

cli_at() { mkdir -p "$UHOME"; HOME="$UHOME" "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$1" --palw-drill-genesis-salt="$SALT" "${@:2}"; }
dag_field() {
  cli_at "$1" node dag-info --output json 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin)['$2'])" 2>/dev/null || true
}
new_daa() { dag_field "$NEW0_RPC" virtual_daa; }
old_daa() { dag_field "$OLD_RPC" virtual_daa; }
cursor() { { wc -c < "$1"; } 2>/dev/null | tr -d ' ' || echo 0; }

wait_after() {
  local file="$1" from="$2" pattern="$3" what="$4" began=$SECONDS last_move=$SECONDS last="" daa line
  while :; do
    line="$(tail -c +"$((from + 1))" "$file" 2>/dev/null | grep -m1 -E -- "$pattern" || true)"
    if [ -n "$line" ]; then echo "$line"; return 0; fi
    daa="$(new_daa)"
    if [ -n "$daa" ] && [ "$daa" != "$last" ]; then last="$daa"; last_move=$SECONDS; fi
    [ $((SECONDS - began)) -lt "$STEP_WAIT" ] || die "gave up waiting for: $what (DAA ${last:-?})"
    [ $((SECONDS - last_move)) -lt "$STALL_WAIT" ] || die "gave up waiting for: $what (DAA ${last:-?} unmoved for $STALL_WAIT s)"
    sleep 10
  done
}

wait_daa_past() {
  local n="$1" began=$SECONDS last_move=$SECONDS last="" daa
  while :; do
    daa="$(new_daa)"
    if [[ "$daa" =~ ^[0-9]+$ ]] && [ "$daa" -gt "$n" ]; then echo "$daa"; return 0; fi
    if [ -n "$daa" ] && [ "$daa" != "$last" ]; then last="$daa"; last_move=$SECONDS; fi
    [ $((SECONDS - began)) -lt "$STEP_WAIT" ] || die "gave up waiting for DAA > $n (at ${last:-?})"
    [ $((SECONDS - last_move)) -lt "$STALL_WAIT" ] || die "DAA ${last:-?} unmoved for $STALL_WAIT s waiting for DAA > $n"
    sleep 10
  done
}

# The new nodes (every drill node but the old relay): the logs the classes step reads.
new_logs() { local n; for n in "$WORK_DIR"/new*/kaspad.out; do [ -s "$n" ] && echo "$n"; done; }

cmd_dry() {
  need_env
  local problems=0
  if [ -x "$KASPAD_BIN" ]; then
    local H; H="$("$KASPAD_BIN" --help 2>/dev/null || true)"
    grep -q -- "--palw-drill-tir2-at" <<<"$H" || { log "dry: $KASPAD_BIN has no --palw-drill-tir2-at: not the int-10 release"; problems=1; }
  else
    log "dry: no release under test at '$KASPAD_BIN'"; problems=1
  fi
  if [ -n "$OLD_KASPAD_BIN" ] && [ -x "$OLD_KASPAD_BIN" ]; then
    local O; O="$("$OLD_KASPAD_BIN" --help 2>/dev/null || true)"
    grep -q -- "--palw-drill-tir-at" <<<"$O" || { log "dry: $OLD_KASPAD_BIN lacks --palw-drill-tir-at: it is not int-8"; problems=1; }
    ! grep -q -- "--palw-drill-tir2-at" <<<"$O" || { log "dry: $OLD_KASPAD_BIN knows the DAA-3,600 flag day: it is not the fleet's int-8"; problems=1; }
  else
    log "dry: OLD_KASPAD_BIN is not an executable (the fleet's int-8: lifecycle-run/bin/4ca695b98/kaspad)"; problems=1
  fi
  [ -x "$CLI_BIN" ] || { log "dry: no misaka CLI at $CLI_BIN"; problems=1; }
  [ -f "$RPC_PY" ] || { log "dry: no rpc.py at $RPC_PY (RPC_PY)"; problems=1; }
  [ -f "$CLASSROWS_PY" ] || { log "dry: no classrows.py at $CLASSROWS_PY (CLASSROWS_PY)"; problems=1; }
  log "dry: the plan —"
  log "  old      $OLD_LOG names flag days $FENCE_AT/$FENCE2_AT/$FENCE3_AT and palw_tir_v1 at $TIR_AT, and no palw_tir_fence2;"
  log "           new0's banner names palw_tir_fence2 at $TIR2_AT and no palw_model_court_window"
  log "  below    new0 DAA < $((TIR2_AT - 2)): one sink at one DAA on new0 and the old relay"
  log "  cross    new0 past DAA $((TIR2_AT + 5)): 'Fork-id mismatch … crossed fence $TIR2_AT'; the old relay stops at DAA <= $((TIR2_AT + 3))"
  log "  classes  the small class registered at >= $TIR2_AT is admitted by every running new node (getPalwClasses: one registeredDaa,"
  log "           canonicalLeaves and artifactRoot, the artifact's); the A16 class registered below $TIR2_AT is the twin;"
  log "           no 'PALW model court window' line on any node"
  [ "$problems" -eq 0 ] && log "dry: OK" || log "dry: the plan is well-formed; resolve the items above before the run"
}

step_old() {
  [ -s "$OLD_LOG" ] || incomplete "no old relay log at $OLD_LOG (df.sh up starts the old relay last)"
  [ -s "$NEW0_LOG" ] || incomplete "no new0 log at $NEW0_LOG"
  grep -q "PALW DRILL FLAG DAY (" "$OLD_LOG" || die "the old relay's log names no drill flag day: it is not running this drill's lists"
  grep -q "PALW DRILL FLAG DAY: .*palw_tir_v1" "$OLD_LOG" || die "the old relay did not move palw_tir_v1: it is not int-8 (the IR fence is int-8's)"
  if grep -q "PALW DRILL FLAG DAY: .*\(palw_tir_fence2\|palw_model_court_window\)" "$OLD_LOG"; then
    die "the old relay moved a DAA-3,600 fence: it is the release under test, not the fleet's int-8"
  fi
  grep -q "PALW DRILL FLAG DAY: palw_tir_fence2 .*to DAA $TIR2_AT\b" "$NEW0_LOG" \
    || die "new0's banner does not name palw_tir_fence2 moved to DAA $TIR2_AT"
  if grep -q "PALW DRILL FLAG DAY: .*palw_model_court_window" "$NEW0_LOG"; then
    die "new0's banner names palw_model_court_window: the court window is armed nowhere on testnet-12 (coordinator, 2026-10-01)"
  fi
  mkdir -p "$EVIDENCE"
  { echo "# old (int-8)"; grep "PALW DRILL FLAG DAY" "$OLD_LOG" | head -n 40
    echo "# new0"; grep "PALW DRILL FLAG DAY" "$NEW0_LOG" | head -n 40; } > "$EVIDENCE/flag-days.log" || true
  log "PASS: the old relay runs the flag days and the IR fence and not palw_tir_fence2; new0 names palw_tir_fence2 at $TIR2_AT and no court window"
}

step_below() {
  if [ -s "$EVIDENCE/below.pass" ]; then log "PASS (recorded): $(cat "$EVIDENCE/below.pass")"; return 0; fi
  local daa; daa="$(new_daa)"
  [[ "$daa" =~ ^[0-9]+$ ]] || incomplete "new0's wRPC does not answer"
  [ "$daa" -lt "$((TIR2_AT - 2))" ] || incomplete "DAA $daa is too close to TIR2_AT=$TIR2_AT for the below-the-fence half (a fresh drill runs it right after up)"
  mkdir -p "$EVIDENCE"
  local i n_sink o_sink n_daa o_daa
  for i in $(seq 1 120); do
    n_daa="$(new_daa)"; o_daa="$(old_daa)"
    n_sink="$(dag_field "$NEW0_RPC" sink)"; o_sink="$(dag_field "$OLD_RPC" sink)"
    if [ -n "$n_sink" ] && [ "$n_sink" = "$o_sink" ] && [ "$n_daa" = "$o_daa" ]; then
      [ "$n_daa" -lt "$TIR2_AT" ] || die "the tips agreed only at DAA $n_daa, not below TIR2_AT=$TIR2_AT"
      printf 'daa %s sink %s\n' "$n_daa" "$n_sink" > "$EVIDENCE/below-tips.txt"
      echo "new0 and the old relay (int-8) report one sink ${n_sink:0:16}… at DAA $n_daa, below the flag day at $TIR2_AT" > "$EVIDENCE/below.pass"
      log "PASS: $(cat "$EVIDENCE/below.pass")"
      return 0
    fi
    sleep 5
  done
  die "new0 (DAA ${n_daa:-?}, sink ${n_sink:0:16}) and the old relay (DAA ${o_daa:-?}, sink ${o_sink:0:16}) never agreed below the flag day"
}

step_cross() {
  mkdir -p "$EVIDENCE"
  local daa; daa="$(new_daa)"
  [[ "$daa" =~ ^[0-9]+$ ]] || incomplete "new0's wRPC does not answer"
  local pattern="Fork-id mismatch on network [^ ]+ at DAA [0-9]+ - this node has crossed fence $TIR2_AT"
  local line
  line="$(grep -m1 -E -- "$pattern" "$NEW0_LOG" 2>/dev/null || true)"
  if [ -z "$line" ]; then
    wait_after "$NEW0_LOG" 0 "$pattern|judged connected peer .* disconnecting it: Fork-id mismatch" \
      "new0 to refuse the old relay by the fork id at the flag day $TIR2_AT" > "$EVIDENCE/cross-refusal.log"
  else
    echo "$line" > "$EVIDENCE/cross-refusal.log"
  fi
  log "new0: $(cat "$EVIDENCE/cross-refusal.log")"
  grep -m1 -E "Fork-id mismatch" "$OLD_LOG" > "$EVIDENCE/cross-old-refusal.log" 2>/dev/null || true
  [ -s "$EVIDENCE/cross-old-refusal.log" ] && log "old: $(cat "$EVIDENCE/cross-old-refusal.log")"
  local o1; o1="$(old_daa)"
  local after; after="$(wait_daa_past "$(( ${o1:-$TIR2_AT} > TIR2_AT ? ${o1:-$TIR2_AT} + 5 : TIR2_AT + 5 ))")"
  local o2; o2="$(old_daa)"
  printf 'new %s old %s → %s\n' "$after" "${o1:-?}" "${o2:-?}" > "$EVIDENCE/cross-daa.txt"
  if [[ "$o1" =~ ^[0-9]+$ ]] && [[ "$o2" =~ ^[0-9]+$ ]]; then
    [ "$o2" -eq "$o1" ] || die "the old relay's DAA moved $o1 → $o2 after the refusal: it is still following the chain"
    [ "$o2" -lt "$after" ] || die "the old relay is at DAA $o2, not behind new0's $after"
    [ "$o2" -le "$((TIR2_AT + 3))" ] || die "the old relay followed the chain to DAA $o2, past the flag day $TIR2_AT"
  else
    log "note: the old relay's wRPC does not answer; judged on new0's refusal alone"
  fi
  log "PASS: new0 validated past the flag day (DAA $after) and refused the old relay by the fork id; the old relay stopped at ${o2:-?}"
}

# The registration DAA of a class from the sampler's milestones (`small-Candidate <daa> <time>`, `Candidate <daa> <time>`).
milestone_daa() { awk -F'\t' -v m="$1" '$1==m {print $2; exit}' "$MILESTONES" 2>/dev/null || true; }

# One class on every running new node: `class_rows <class id> <out file>` writes, per node that answers and lists the
# class, `<node> <registeredDaa> <canonicalLeaves> <artifactRoot> <status>` (audit-tir/classrows.py), and sets
# NODES_ANSWERING to the number of nodes whose JSON-RPC answered.
class_rows() {
  local cid="$1" out="$2" k line
  : > "$out"; NODES_ANSWERING=0
  for k in 0 1 2 3 4 5 6 7; do
    line="$(python3 "$CLASSROWS_PY" "$((JSON_BASE + k))" "$cid" "new$k" 2>/dev/null || true)"
    printf '%s\n' "$line" | grep -q '^ANSWERS$' || continue
    NODES_ANSWERING=$((NODES_ANSWERING + 1))
    printf '%s\n' "$line" | grep -v '^ANSWERS$' >> "$out" || true
  done
}

# Judge one class: it must be listed by every answering node, with ONE (registeredDaa, canonicalLeaves, artifactRoot)
# everywhere and the artifact's root, past or below the fence as asked. Sets JUDGED_DAA to its registeredDaa.
# `judge_class <label> <class id> <artifact root file> <below|past>` (runs in this shell: its die / incomplete exit the script).
judge_class() {
  local label="$1" cid="$2" rootfile="$3" side="$4" rows listed distinct d leaves root want
  class_rows "$cid" "$EVIDENCE/class-rows.tmp"
  rows="$(cat "$EVIDENCE/class-rows.tmp")"
  [ "${NODES_ANSWERING:-0}" -ge 1 ] || incomplete "no new node's JSON-RPC answers (JSON_BASE=$JSON_BASE)"
  listed="$(printf '%s\n' "$rows" | grep -c . || true)"
  [ "$listed" -ge 1 ] || incomplete "no node lists the $label class yet (registered, not yet folded?)"
  [ "$listed" -eq "$NODES_ANSWERING" ] || die "only $listed of $NODES_ANSWERING answering nodes list the $label class: the others did not admit it
$rows"
  distinct="$(printf '%s\n' "$rows" | awk '{print $2, $3, $4}' | sort -u | wc -l | tr -d ' ')"
  [ "$distinct" -eq 1 ] || die "the nodes disagree about the $label class (registeredDaa canonicalLeaves artifactRoot):
$rows"
  read -r d leaves root <<<"$(printf '%s\n' "$rows" | awk 'NR==1 {print $2, $3, $4}')"
  [[ "$d" =~ ^[0-9]+$ ]] && [[ "$leaves" =~ ^[0-9]+$ ]] && [ "$leaves" -gt 0 ] || die "unreadable $label class row: $rows"
  case "$side" in
    past) [ "$d" -ge "$TIR2_AT" ] || die "the $label class registered at DAA $d, BELOW the flag day $TIR2_AT: the past-the-fence half did not happen" ;;
    below) [ "$d" -lt "$TIR2_AT" ] || die "the $label class registered at DAA $d, not below the flag day $TIR2_AT: the below half did not happen" ;;
  esac
  want="$(tr -d ' \n' < "$rootfile" 2>/dev/null || true)"
  [ -z "$want" ] || [ "$root" = "$want" ] || die "the $label class's registered root $root is not the artifact's $want"
  echo "$label class ${cid:0:16}… ($side the fence): registered at DAA $d, $leaves canonical leaves, root ${root:0:16}…, listed by $listed of $NODES_ANSWERING answering nodes" >> "$EVIDENCE/classes.txt"
  JUDGED_DAA="$d"
}

step_classes() {
  mkdir -p "$EVIDENCE"; : > "$EVIDENCE/classes.txt"
  local ir_id small_id past_id below_id past_label below_label past_root below_root
  ir_id="$(tr -d ' \n' < "$WORK_DIR/ir-class.id" 2>/dev/null || true)"
  small_id="$(tr -d ' \n' < "$WORK_DIR/small-class.id" 2>/dev/null || true)"
  case "${PAST_CLASS:-small}" in
    ir)    past_id="$ir_id";    below_id="$small_id"; past_label=A16;   below_label=small
           past_root="$WORK_DIR/ir-artifact.root";    below_root="$WORK_DIR/small-artifact.root" ;;
    small) past_id="$small_id"; below_id="$ir_id";    past_label=small; below_label=A16
           past_root="$WORK_DIR/small-artifact.root"; below_root="$WORK_DIR/ir-artifact.root" ;;
    *) die "PAST_CLASS must be small or ir" ;;
  esac
  [ -n "$past_id" ] || die "no $PAST_CLASS class id under $WORK_DIR"
  [ -e "$WORK_DIR/past-register.ok" ] || incomplete "the $past_label class is not registered yet: df.sh register-past first"
  # The registration needs a few blocks to be folded and to reach every node: wait (CLASSES_WAIT s) until every answering
  # node lists the class, then judge — a node that still does not list it afterwards did not admit it.
  local began=$SECONDS listed
  while :; do
    class_rows "$past_id" "$EVIDENCE/class-rows.tmp"
    listed="$(grep -c . "$EVIDENCE/class-rows.tmp" || true)"
    { [ "${NODES_ANSWERING:-0}" -ge 1 ] && [ "$listed" -eq "$NODES_ANSWERING" ]; } && break
    [ $((SECONDS - began)) -lt "${CLASSES_WAIT:-1800}" ] || break
    sleep 15
  done
  # The class registered PAST the fence: admitted by every node, one row everywhere, the artifact's root.
  judge_class "$past_label" "$past_id" "$past_root" past
  log "the $past_label class registered at DAA $JUDGED_DAA, past the flag day $TIR2_AT, is admitted on every new node under fence2's sizing"
  # The twin: the class registered BELOW the fence, under the release's sizing, accepted the same way (when it is on this chain).
  if [ -n "$below_id" ] && [ -s "$below_root" ]; then
    judge_class "$below_label" "$below_id" "$below_root" below
    log "the $below_label class registered at DAA $JUDGED_DAA, below $TIR2_AT, is the twin (the release's sizing), accepted on every new node"
  else
    log "note: the other class is not on this chain (DF1=0, or its artifact is not built): its half is not judged"
  fi
  rm -f "$EVIDENCE/class-rows.tmp"
  # The dormant window: no node folded a window row for any class.
  if new_logs | xargs grep -h "PALW model court window: class" >/dev/null 2>&1; then
    die "a node logged a model court window: the window is armed nowhere on testnet-12 (coordinator, 2026-10-01)"
  fi
  echo "no node logged a 'PALW model court window' line (the window is dormant)" >> "$EVIDENCE/classes.txt"
  log "PASS: $(wc -l < "$EVIDENCE/classes.txt" | tr -d ' ') evidence lines in $EVIDENCE/classes.txt; no court-window line on any node; the DA ladder is read by df.sh bdc (Stage 2)"
}

cmd_run() {
  need_env
  step_old
  step_below
  step_cross
  step_classes
}

case "${1:-help}" in
  dry) cmd_dry ;;
  run) cmd_run ;;
  old) need_env; step_old ;;
  below) need_env; step_below ;;
  cross) need_env; step_cross ;;
  classes) need_env; step_classes ;;
  help | -h | --help | *) sed -n '2,40p' "$0"; [ "${1:-help}" = help ] || [ "${1:-}" = -h ] || [ "${1:-}" = --help ] || exit 2 ;;
esac
