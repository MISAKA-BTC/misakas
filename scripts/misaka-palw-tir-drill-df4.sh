#!/usr/bin/env bash
# misaka-palw-tir-drill-df4.sh — RFC-0002 Phase F drill D-F4: `palw_tir_v1` crossed on the SHIPPING binary.
#
# One step of the ONE salted testnet-12 drill chain D-F1…D-F4 run on. The runner and the node layout are
# the node lane's (audit-tir/df.sh on tir/node: `up` starts every node — the `old` relay last: the fleet's
# release before the IR flag day, keyless, the same salt and flag days, no --palw-drill-tir-at, peered to
# new0 — and writes the signed below-the-fence IR registration; `df.sh df4` calls `run` here with the
# layout exported). This script starts nothing; it submits one object and reads logs and RPC.
#
# What it shows, each from log bytes written after its own cursor and from RPC:
#   old     the old relay runs this drill (its log names the three moved flag days) and is not the release
#           under test (it moved no palw_tir_v1)
#   below   at DAA < TIR_AT the signed IR class registration ($WORK_DIR/ir-registration.obj) rides a block:
#           new0 drops it by name ("an IR object was dropped by name below palw_tir_v1, and the block
#           stands"), the old relay skips the carrier it cannot decode (A-2: "[palw-lifecycle] carrier …
#           produced no object"), and the two report the same sink at the same DAA below the fence
#   cross   new0's DAA passes TIR_AT with blocks validated above it, and the fork id separates the two:
#           "Fork-id mismatch … this node has crossed fence TIR_AT" in new0's log (or the old relay's
#           refusal of new0 in its own), after which the old relay's DAA stops while new0's goes on
#   court   (after df2) a court conviction past the fence: new0's court_fraud count in the runner's
#           $WORK_DIR/df-state.json (D-F2's one-move IR court) — this script does not duplicate D-F2
#
#   misaka-palw-tir-drill-df4.sh dry     binaries, flags, environment, heights; print the plan; start nothing
#   misaka-palw-tir-drill-df4.sh run     old, below, cross (and court when df2 has convicted)
#   misaka-palw-tir-drill-df4.sh below | cross | court    one step
#
# Exit 0 PASS, 1 FAIL, 3 INCOMPLETE (not reachable yet: `below` past TIR_AT - 2, `court` before D-F2).
#
# ENV (the drill layout, shared with D-F1/D-F2/D-F3; defaults are audit-tir/lib-df.sh's):
#   SALT (else $WORK_DIR/SALT), WORK_DIR (~/.misaka-palw-tir-drill), KASPAD_BIN (the release under test),
#   OLD_KASPAD_BIN (the fleet's release before the IR flag day, int-7 7aba8dd57 or later), CLI_BIN,
#   TIR_AT (20), FENCE_AT / FENCE2_AT / FENCE3_AT (6 / 10 / 14: the post-launch lists' drill heights, below
#   TIR_AT, on every node old included), NEW0_P2P (56100), NEW0_RPC (57100), NEW0_LOG
#   ($WORK_DIR/new0/kaspad.out), OLD_P2P (56108), OLD_RPC (57108), OLD_LOG ($WORK_DIR/old/kaspad.out),
#   FUNDING_KEY (the carrier's fee key: $WORK_DIR/keyring/main-0.seed), IR_REGISTER_CMD (overrides the
#   submission), STALL_WAIT (3600 s), STEP_WAIT (43200 s).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK_DIR="${WORK_DIR:-$HOME/.misaka-palw-tir-drill}"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
OLD_KASPAD_BIN="${OLD_KASPAD_BIN:-}"
CLI_BIN="${CLI_BIN:-$(dirname "$KASPAD_BIN")/misaka}"
TIR_AT="${TIR_AT:-20}"
FENCE_AT="${FENCE_AT:-6}"
FENCE2_AT="${FENCE2_AT:-10}"
FENCE3_AT="${FENCE3_AT:-14}"
NEW0_P2P="${NEW0_P2P:-56100}"
NEW0_RPC="${NEW0_RPC:-57100}"
NEW0_LOG="${NEW0_LOG:-$WORK_DIR/new0/kaspad.out}"
OLD_P2P="${OLD_P2P:-56108}"
OLD_RPC="${OLD_RPC:-57108}"
OLD_LOG="${OLD_LOG:-$WORK_DIR/old/kaspad.out}"
FUNDING_KEY="${FUNDING_KEY:-$WORK_DIR/keyring/main-0.seed}"
IR_REGISTER_CMD="${IR_REGISTER_CMD:-}"
STALL_WAIT="${STALL_WAIT:-3600}"
STEP_WAIT="${STEP_WAIT:-43200}"
UHOME="$WORK_DIR/userhome"
REGISTRATION="$WORK_DIR/ir-registration.obj"
DF_STATE="$WORK_DIR/df-state.json"
EVIDENCE="$WORK_DIR/evidence/df4"
# Public testnet-12's listeners and the deploy kit's: a drill never binds one (the rcore drill's list).
PUBLIC_T12_PORTS="26311 26210 27210 28210 8545 26312 26313 26314 26321 26323 26324 26331 26333 26334 26341 26343 26344 26351 26353 26354"

RC_FAIL=1
RC_INCOMPLETE=3

log() { printf '[tir-drill D-F4] %s\n' "$*" >&2; }
die() { log "FAIL: $*"; exit "$RC_FAIL"; }
incomplete() { log "INCOMPLETE: $*"; exit "$RC_INCOMPLETE"; }

# The salt: SALT from the environment, else $WORK_DIR/SALT (never printed).
load_salt() {
  SALT="${SALT:-}"
  [ -n "$SALT" ] || SALT="$(tr -d ' \n' < "$WORK_DIR/SALT" 2>/dev/null || true)"
  [[ "$SALT" =~ ^[0-9a-f]{64}$ ]] || die "no drill salt (SALT, or $WORK_DIR/SALT)"
}

need_env() {
  load_salt
  local v
  for v in TIR_AT FENCE_AT FENCE2_AT FENCE3_AT NEW0_P2P NEW0_RPC OLD_P2P OLD_RPC; do
    [[ "${!v}" =~ ^[0-9]+$ ]] || die "$v must be a number"
  done
  # Four distinct heights, the post-launch lists below the IR fence (validate_palw_v2 wants fence1 ≤ fence3,
  # and the fork id names heights, not fences).
  [ "$FENCE_AT" -gt 0 ] && [ "$FENCE_AT" -lt "$FENCE2_AT" ] && [ "$FENCE2_AT" -lt "$FENCE3_AT" ] && [ "$FENCE3_AT" -lt "$TIR_AT" ] \
    || die "the heights must satisfy 0 < FENCE_AT < FENCE2_AT < FENCE3_AT < TIR_AT (got $FENCE_AT $FENCE2_AT $FENCE3_AT $TIR_AT)"
  local p q
  for p in "$NEW0_P2P" "$NEW0_RPC" "$OLD_P2P" "$OLD_RPC"; do
    for q in $PUBLIC_T12_PORTS; do [ "$p" != "$q" ] || die "port $p is a public testnet-12 default"; done
  done
}

cli_at() { mkdir -p "$UHOME"; HOME="$UHOME" "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$1" --palw-drill-genesis-salt="$SALT" "${@:2}"; }

# `dag_field <rpc port> <field>` — one field of the node's DAG info (the rcore drill's `node dag-info` reading).
dag_field() {
  cli_at "$1" node dag-info --output json 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin)['$2'])" 2>/dev/null || true
}
new_daa() { dag_field "$NEW0_RPC" virtual_daa; }
old_daa() { dag_field "$OLD_RPC" virtual_daa; }

cursor() { { wc -c < "$1"; } 2>/dev/null | tr -d ' ' || echo 0; }

# `wait_after <file> <cursor> <regex> <what>` — the first matching line written after the cursor, on the
# chain's clock (gives up when new0's DAA stops moving for STALL_WAIT s, or after STEP_WAIT s).
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

# `wait_below <file> <cursor> <regex> <what>` — `wait_after`, but only while new0 is below TIR_AT: the
# below-the-fence half is unreachable once the chain crosses the fence without the line (INCOMPLETE).
wait_below() {
  local file="$1" from="$2" pattern="$3" what="$4" daa line
  while :; do
    line="$(tail -c +"$((from + 1))" "$file" 2>/dev/null | grep -m1 -E -- "$pattern" || true)"
    if [ -n "$line" ]; then echo "$line"; return 0; fi
    daa="$(new_daa)"
    if [[ "$daa" =~ ^[0-9]+$ ]] && [ "$daa" -ge "$TIR_AT" ]; then
      incomplete "the chain reached DAA $daa without $what: the carrier was not mined below TIR_AT=$TIR_AT (a fresh drill runs df4 right after up)"
    fi
    sleep 10
  done
}

# `wait_daa_past <n>` — until new0's virtual DAA is past n (same give-up rules).
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

submit_registration() {
  if [ -n "$IR_REGISTER_CMD" ]; then
    bash -c "$IR_REGISTER_CMD"
  else
    cli_at "$NEW0_RPC" palw submit-object --object "$REGISTRATION" --yes --key-file "$FUNDING_KEY"
  fi
}

cmd_dry() {
  need_env
  local problems=0
  if [ -x "$KASPAD_BIN" ]; then
    "$KASPAD_BIN" --help 2>/dev/null | grep -q -- "--palw-drill-tir-at" \
      || { log "dry: $KASPAD_BIN has no --palw-drill-tir-at: not the release under test"; problems=1; }
  else
    log "dry: no release under test at $KASPAD_BIN"; problems=1
  fi
  if [ -n "$OLD_KASPAD_BIN" ] && [ -x "$OLD_KASPAD_BIN" ]; then
    if "$OLD_KASPAD_BIN" --help 2>/dev/null | grep -q -- "--palw-drill-tir-at"; then
      log "dry: $OLD_KASPAD_BIN knows --palw-drill-tir-at: it is not a pre-IR release"; problems=1
    fi
    "$OLD_KASPAD_BIN" --help 2>/dev/null | grep -q -- "--palw-drill-fence3-at" \
      || { log "dry: $OLD_KASPAD_BIN has no --palw-drill-fence3-at: older than int-6, it cannot run this drill's lists"; problems=1; }
  else
    log "dry: OLD_KASPAD_BIN is not set to an executable (the fleet's release before the IR flag day)"; problems=1
  fi
  [ -x "$CLI_BIN" ] || { log "dry: no misaka CLI at $CLI_BIN"; problems=1; }
  if [ -z "$IR_REGISTER_CMD" ]; then
    [ -s "$REGISTRATION" ] || log "dry: $REGISTRATION is not written yet (df.sh up writes it)"
    [ -s "$FUNDING_KEY" ] || log "dry: the funding key $FUNDING_KEY is not written yet (the keyring, df.sh up)"
  fi
  log "dry: the plan —"
  log "  old      $OLD_LOG names the drill's three moved flag days ($FENCE_AT, $FENCE2_AT, $FENCE3_AT) and no palw_tir_v1"
  log "  below    at DAA < $((TIR_AT - 2)): misaka palw submit-object --object $REGISTRATION --yes --key-file $FUNDING_KEY"
  log "           → new0 drops it by name; old skips the carrier (A-2); one sink at one DAA below $TIR_AT"
  log "  cross    new0 past DAA $((TIR_AT + 5)); 'Fork-id mismatch … crossed fence $TIR_AT' on new0 (or old's refusal);"
  log "           old's DAA stops after the refusal while new0's goes on"
  log "  court    after df2: new0's court_fraud > 0 in $DF_STATE"
  if [ "$problems" -eq 0 ]; then
    log "dry: OK"
  else
    log "dry: the plan is well-formed; resolve the items above before the run"
  fi
}

step_old() {
  [ -s "$OLD_LOG" ] || incomplete "no old relay log at $OLD_LOG (df.sh up starts the old relay last)"
  local banner
  banner="$(grep -m1 "PALW DRILL FLAG DAY (" "$OLD_LOG" || true)"
  [ -n "$banner" ] || die "the old relay's log names no drill flag day: it is not running this drill's lists"
  if grep -q "PALW DRILL FLAG DAY: .*palw_tir_v1" "$OLD_LOG"; then
    die "the old relay moved palw_tir_v1: it is the release under test, not the fleet's"
  fi
  mkdir -p "$EVIDENCE"
  grep "PALW DRILL FLAG DAY" "$OLD_LOG" | head -n 40 > "$EVIDENCE/old-flag-days.log" || true
  log "PASS: the old relay runs this drill's flag days and no IR fence ($(grep -c "PALW DRILL FLAG DAY: " "$EVIDENCE/old-flag-days.log" || true) moved fences)"
}

step_below() {
  if [ -s "$EVIDENCE/below.pass" ]; then
    log "PASS (recorded): $(cat "$EVIDENCE/below.pass")"; return 0
  fi
  if [ -z "$IR_REGISTER_CMD" ]; then
    [ -s "$REGISTRATION" ] || incomplete "no signed IR registration at $REGISTRATION (df.sh up writes it)"
    [ -s "$FUNDING_KEY" ] || incomplete "no funding key at $FUNDING_KEY"
  fi
  local daa; daa="$(new_daa)"
  [[ "$daa" =~ ^[0-9]+$ ]] || incomplete "new0's wRPC does not answer"
  [ "$daa" -lt "$((TIR_AT - 2))" ] || incomplete "DAA $daa is too close to TIR_AT=$TIR_AT for the below-the-fence half (a fresh drill runs df4 right after up)"
  mkdir -p "$EVIDENCE"
  local from_new from_old; from_new="$(cursor "$NEW0_LOG")"; from_old="$(cursor "$OLD_LOG")"
  log "at DAA $daa: submitting the signed IR class registration below palw_tir_v1"
  submit_registration > "$EVIDENCE/below-register.out" 2>&1 || die "the submission failed ($EVIDENCE/below-register.out)"
  wait_below "$NEW0_LOG" "$from_new" "an IR object was dropped by name below palw_tir_v1, and the block stands" \
    "new0 dropping the IR registration by name" > "$EVIDENCE/below-drop.log"
  log "new0: $(cat "$EVIDENCE/below-drop.log")"
  wait_below "$OLD_LOG" "$from_old" "\[palw-lifecycle\] carrier .* produced no object" \
    "the old relay skipping the IR carrier (A-2)" > "$EVIDENCE/below-skip.log"
  log "old: $(cat "$EVIDENCE/below-skip.log")"
  # Identical tips: the carrier's block is on both chains and both folded it to the same state (the old
  # build skipped the payload it cannot decode, the new one dropped it by name). Read at one DAA.
  local i n_sink o_sink n_daa o_daa
  for i in $(seq 1 120); do
    n_daa="$(new_daa)"; o_daa="$(old_daa)"
    n_sink="$(dag_field "$NEW0_RPC" sink)"; o_sink="$(dag_field "$OLD_RPC" sink)"
    if [ -n "$n_sink" ] && [ "$n_sink" = "$o_sink" ] && [ "$n_daa" = "$o_daa" ]; then
      [ "$n_daa" -lt "$TIR_AT" ] || die "the tips agreed only at DAA $n_daa, not below TIR_AT=$TIR_AT"
      printf 'daa %s sink %s\n' "$n_daa" "$n_sink" | tee "$EVIDENCE/below-tips.txt" > /dev/null
      echo "the IR registration dropped by name on new0, skipped by old, one sink ${n_sink:0:16}… at DAA $n_daa" > "$EVIDENCE/below.pass"
      log "PASS: $(cat "$EVIDENCE/below.pass")"
      return 0
    fi
    sleep 5
  done
  die "new (DAA ${n_daa:-?}, sink ${n_sink:0:16}) and old (DAA ${o_daa:-?}, sink ${o_sink:0:16}) never agreed below the fence"
}

step_cross() {
  mkdir -p "$EVIDENCE"
  local daa; daa="$(new_daa)"
  [[ "$daa" =~ ^[0-9]+$ ]] || incomplete "new0's wRPC does not answer"
  # The refusal is logged once new0 reaches the fence; a cursor at 0 reads the whole log, so this holds
  # both before and after the crossing. Either side's refusal of the other is the fork id at work.
  local pattern="Fork-id mismatch on network [^ ]+ at DAA [0-9]+ - this node has crossed fence $TIR_AT"
  local line
  line="$(grep -m1 -E -- "$pattern" "$NEW0_LOG" 2>/dev/null || true)"
  if [ -z "$line" ]; then
    wait_after "$NEW0_LOG" 0 "$pattern|judged connected peer .* disconnecting it: Fork-id mismatch" \
      "new0 to refuse the old relay by the fork id at TIR_AT=$TIR_AT" > "$EVIDENCE/cross-refusal.log"
  else
    echo "$line" > "$EVIDENCE/cross-refusal.log"
  fi
  log "new0: $(cat "$EVIDENCE/cross-refusal.log")"
  grep -m1 -E "Fork-id mismatch" "$OLD_LOG" > "$EVIDENCE/cross-old-refusal.log" 2>/dev/null || true
  [ -s "$EVIDENCE/cross-old-refusal.log" ] && log "old: $(cat "$EVIDENCE/cross-old-refusal.log")"
  # After the refusal the old relay (peered to new0 alone) is cut off: its DAA stops while new0's goes on.
  local o1; o1="$(old_daa)"
  local after; after="$(wait_daa_past "$(( ${o1:-$TIR_AT} > TIR_AT ? ${o1:-$TIR_AT} + 5 : TIR_AT + 5 ))")"
  local o2; o2="$(old_daa)"
  printf 'new %s old %s → %s\n' "$after" "${o1:-?}" "${o2:-?}" > "$EVIDENCE/cross-daa.txt"
  if [[ "$o1" =~ ^[0-9]+$ ]] && [[ "$o2" =~ ^[0-9]+$ ]]; then
    [ "$o2" -eq "$o1" ] || die "the old relay's DAA moved $o1 → $o2 after the refusal: it is still following the chain"
    [ "$o2" -lt "$after" ] || die "the old relay is at DAA $o2, not behind new0's $after"
    [ "$o2" -le "$((TIR_AT + 3))" ] || die "the old relay followed the chain to DAA $o2, past the fence $TIR_AT"
  else
    log "note: the old relay's wRPC does not answer; judged on new0's refusal alone"
  fi
  log "PASS: new0 validated past the fence (DAA $after) and refused the old relay by the fork id; the old relay stopped at ${o2:-?}"
}

step_court() {
  local n
  n="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["claims"]["new0"]["court_fraud"])' "$DF_STATE" 2>/dev/null || true)"
  [[ "$n" =~ ^[0-9]+$ ]] && [ "$n" -gt 0 ] || incomplete "no court conviction of new0 in $DF_STATE yet (D-F2 plants them; run df4 court after df2)"
  mkdir -p "$EVIDENCE"
  echo "new0 court_fraud $n" > "$EVIDENCE/court.txt"
  log "PASS: $n court conviction(s) of new0's IR claims past the fence (D-F2's one-move IR court)"
}

cmd_run() {
  need_env
  step_old
  step_below
  step_cross
  local rc=0
  ( step_court ) || rc=$?
  if [ "$rc" -eq "$RC_INCOMPLETE" ]; then
    log "PASS: old, below and cross; the court half follows D-F2 (misaka-palw-tir-drill-df4.sh court)"
  elif [ "$rc" -ne 0 ]; then
    exit "$rc"
  else
    log "PASS: old, below, cross and court"
  fi
}

case "${1:-help}" in
  dry) cmd_dry ;;
  run) cmd_run ;;
  below) need_env; step_below ;;
  cross) need_env; step_cross ;;
  court) step_court ;;
  help | -h | --help | *) sed -n '2,39p' "$0"; [ "${1:-help}" = help ] || [ "${1:-}" = -h ] || [ "${1:-}" = --help ] || exit 2 ;;
esac
