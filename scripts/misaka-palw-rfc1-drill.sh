#!/usr/bin/env bash
# misaka-palw-rfc1-drill.sh — RFC-0001 drill: the five dormant fences crossed on the SHIPPING binary of the branch.
#
# One salted drill chain (the runner and node layout are audit-tir/df.sh's, or the int-11 drill's: `up` starts every node —
# the `old` relay last: the release before these fences, keyless, the same salt and flag days, none of the --palw-drill-*
# flags below, peered to new0). This script starts nothing; it submits what the claim commands build and reads logs and RPC.
#
# The fences, armed on new0 (and ONLY new0) at distinct drill heights, each over its prerequisites (validate_palw_v2 refuses
# the chain otherwise, by name):
#     --palw-drill-fp-constraint-at   CONSTRAINT_AT   palw_fp_decode_constraint   (FP job version 6, ADR-0096 D6-8)
#     --palw-drill-fp-constraint2-at  CONSTRAINT2_AT  palw_fp_constraint_v2       (the second constraint form)
#     --palw-drill-fp-prefix-at       PREFIX_AT       palw_fp_prefix_state        (FP job version 11, §2.6 stage 2)
#     --palw-drill-fp-tokenizer-at    TOKENIZER_AT    palw_fp_tokenizer_match     (§2.9)
#     --palw-drill-adapter-at         ADAPTER_AT      palw_adapter_class_v1       (object tag 94, ADR-0163)
#   (a salted chain also needs the prerequisites: decode rules, derived work, the IR and improvement fences, through the
#   runner's own flag-day lists — `dry` prints what is missing.)
#
# What it shows, each from log bytes written after its own cursor and from RPC:
#   old      the old relay runs this drill and moved none of the five fences
#   below    below each fence, the claim/object the CLAIM COMMAND submits is refused by name and the block stands:
#              "[palw-fp] carrier … produced no object: a constrained claim (FP job version 6) below palw_fp_decode_constraint"
#              "[palw-fp] carrier … produced no object: a prefix-state claim (FP job version 11) below palw_fp_prefix_state"
#              "an adapter class listing was dropped by name below palw_adapter_class_v1, and the block stands"
#   cross    new0 passes the highest fence and the fork id separates the two nodes ("Fork-id mismatch … crossed fence")
#   above    past the fences: a prefix-state claim (cached prefix), a constrained claim under a second-form constraint and an
#            adapter class listing each ride a block, bind a panel, license and reach Final (the claim commands are the
#            producer's: they run the worker/gateway on the drill's tiny class and submit; see CLAIM COMMANDS)
#
#   misaka-palw-rfc1-drill.sh dry         binaries, flags, environment, heights, the lock; print the plan; start nothing
#   misaka-palw-rfc1-drill.sh wait-lock   poll DRILL.lock until it is FREE (never takes it while it exists); then take it
#   misaka-palw-rfc1-drill.sh run         old, below, cross, above (the lock must be ours)
#   misaka-palw-rfc1-drill.sh unlock      release the lock this script took
#   misaka-palw-rfc1-drill.sh old | below | cross | above    one step
#
# Exit 0 PASS, 1 FAIL, 3 INCOMPLETE (not reachable yet / a claim command is not provided).
#
# CLAIM COMMANDS (each prints nothing on success and exits 0; the step reads the chain, not the command's output):
#   SUBMIT_CONSTRAINT_CMD   submit a version-6 claim under a first-form constraint, then one under a second-form one
#   SUBMIT_PREFIX_CMD       produce a prompt with a cached prefix and submit the version-11 claim
#   SUBMIT_ADAPTER_CMD      register a composite class (a LoRA over the drill class) and list it (tag 94)
#   Each is called with BELOW=1 while below its fence (the submission must then be refused by name) and BELOW=0 past it.
#   They are the lane's producers and are NOT in this repository's drill kit yet: without them `below`/`above` are INCOMPLETE,
#   and the in-process e2e tests (misaka-palw-base0/tests/fp_job_v4_t12_e2e.rs: the V11 and V6 claims; the processor gate
#   t12_an_adapter_class_listing_is_gated_...) are the evidence of the flow until they exist.
#
# ENV: SALT (else $WORK_DIR/SALT), WORK_DIR (~/.misaka-palw-rfc1-drill), KASPAD_BIN (the branch's shipping binary),
#   OLD_KASPAD_BIN (the int-11 release or earlier), CLI_BIN, the five heights (defaults 6 / 8 / 10 / 12 / 14, all below
#   CROSS_AT = 20, none equal to another fence's height in the runner's lists: the fork id hashes sorted heights),
#   NEW0_P2P (56200) NEW0_RPC (57200) NEW0_LOG, OLD_P2P (56208) OLD_RPC (57208) OLD_LOG, LOCK_DIR
#   (~/Downloads/MISAKA-wt-b/lanes/DRILL.lock), LANE (U), LOCK_POLL (300 s), STALL_WAIT (3600 s), STEP_WAIT (43200 s).
set -euo pipefail

WORK_DIR="${WORK_DIR:-$HOME/.misaka-palw-rfc1-drill}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KASPAD_BIN="${KASPAD_BIN:-$REPO_ROOT/target/release/kaspad}"
OLD_KASPAD_BIN="${OLD_KASPAD_BIN:-}"
CLI_BIN="${CLI_BIN:-$(dirname "$KASPAD_BIN")/misaka}"
CONSTRAINT_AT="${CONSTRAINT_AT:-6}"
CONSTRAINT2_AT="${CONSTRAINT2_AT:-8}"
PREFIX_AT="${PREFIX_AT:-10}"
TOKENIZER_AT="${TOKENIZER_AT:-12}"
ADAPTER_AT="${ADAPTER_AT:-14}"
CROSS_AT="${CROSS_AT:-20}"
NEW0_P2P="${NEW0_P2P:-56200}"
NEW0_RPC="${NEW0_RPC:-57200}"
NEW0_LOG="${NEW0_LOG:-$WORK_DIR/new0/kaspad.out}"
OLD_P2P="${OLD_P2P:-56208}"
OLD_RPC="${OLD_RPC:-57208}"
OLD_LOG="${OLD_LOG:-$WORK_DIR/old/kaspad.out}"
LOCK_DIR="${LOCK_DIR:-$HOME/Downloads/MISAKA-wt-b/lanes/DRILL.lock}"
LANE="${LANE:-U}"
LOCK_POLL="${LOCK_POLL:-300}"
STALL_WAIT="${STALL_WAIT:-3600}"
STEP_WAIT="${STEP_WAIT:-43200}"
UHOME="$WORK_DIR/userhome"
EVIDENCE="$WORK_DIR/evidence/rfc1"
PUBLIC_T12_PORTS="26311 26210 27210 28210 8545 26312 26313 26314 26321 26323 26324 26331 26333 26334 26341 26343 26344 26351 26353 26354"
FLAGS="--palw-drill-fp-constraint-at --palw-drill-fp-constraint2-at --palw-drill-fp-prefix-at --palw-drill-fp-tokenizer-at --palw-drill-adapter-at"

RC_FAIL=1
RC_INCOMPLETE=3
log() { printf '[rfc1-drill] %s\n' "$*" >&2; }
die() { log "FAIL: $*"; exit "$RC_FAIL"; }
incomplete() { log "INCOMPLETE: $*"; exit "$RC_INCOMPLETE"; }

load_salt() {
  SALT="${SALT:-}"
  [ -n "$SALT" ] || SALT="$(tr -d ' \n' < "$WORK_DIR/SALT" 2>/dev/null || true)"
  [[ "$SALT" =~ ^[0-9a-f]{64}$ ]] || die "no drill salt (SALT, or $WORK_DIR/SALT)"
}

need_env() {
  load_salt
  local v p q
  for v in CONSTRAINT_AT CONSTRAINT2_AT PREFIX_AT TOKENIZER_AT ADAPTER_AT CROSS_AT NEW0_P2P NEW0_RPC OLD_P2P OLD_RPC; do
    [[ "${!v}" =~ ^[0-9]+$ ]] || die "$v must be a number"
  done
  # Five distinct heights, the first fence a prerequisite of the second (constraint ≤ constraint2), all below CROSS_AT.
  local heights="$CONSTRAINT_AT $CONSTRAINT2_AT $PREFIX_AT $TOKENIZER_AT $ADAPTER_AT"
  [ "$(printf '%s\n' $heights | sort -n | uniq | wc -l | tr -d ' ')" = 5 ] || die "the five fence heights must be distinct (got $heights)"
  [ "$CONSTRAINT_AT" -le "$CONSTRAINT2_AT" ] || die "CONSTRAINT_AT must be at or below CONSTRAINT2_AT (the second form rides the first)"
  for v in $heights; do [ "$v" -gt 0 ] && [ "$v" -lt "$CROSS_AT" ] || die "fence height $v must be in 1..CROSS_AT-1 ($CROSS_AT)"; done
  for p in "$NEW0_P2P" "$NEW0_RPC" "$OLD_P2P" "$OLD_RPC"; do
    for q in $PUBLIC_T12_PORTS; do [ "$p" != "$q" ] || die "port $p is a public testnet-12 default"; done
  done
}

cli_at() { mkdir -p "$UHOME"; HOME="$UHOME" "$CLI_BIN" --network testnet-12 --rpc "127.0.0.1:$1" --palw-drill-genesis-salt="$SALT" "${@:2}"; }
dag_field() { cli_at "$1" node dag-info --output json 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin)['$2'])" 2>/dev/null || true; }
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

# The lock is a directory (mkdir is atomic) with the lane name inside. NEVER taken while it exists.
lock_holder() { cat "$LOCK_DIR/owner" 2>/dev/null || cat "$LOCK_DIR"/* 2>/dev/null | head -n 1 || echo unknown; }
cmd_wait_lock() {
  while ! mkdir "$LOCK_DIR" 2>/dev/null; do
    log "DRILL.lock is held by $(lock_holder); polling again in ${LOCK_POLL}s (not taking it)"
    sleep "$LOCK_POLL"
  done
  echo "$LANE" > "$LOCK_DIR/owner"
  log "DRILL.lock taken for lane $LANE"
}
cmd_unlock() {
  [ -d "$LOCK_DIR" ] || { log "no lock"; return 0; }
  [ "$(lock_holder)" = "$LANE" ] || die "the lock is held by $(lock_holder), not $LANE: not releasing it"
  rm -rf "$LOCK_DIR"
  log "DRILL.lock released"
}
need_lock() { [ -d "$LOCK_DIR" ] && [ "$(lock_holder)" = "$LANE" ] || incomplete "the lock is not ours ($(lock_holder 2>/dev/null || echo none)): run wait-lock first"; }

cmd_dry() {
  need_env
  local problems=0 f
  if [ -x "$KASPAD_BIN" ]; then
    for f in $FLAGS; do
      "$KASPAD_BIN" --help 2>/dev/null | grep -q -- "$f" || { log "dry: $KASPAD_BIN has no $f: not the branch's binary"; problems=1; }
    done
  else
    log "dry: no binary under test at $KASPAD_BIN"; problems=1
  fi
  if [ -n "$OLD_KASPAD_BIN" ] && [ -x "$OLD_KASPAD_BIN" ]; then
    for f in $FLAGS; do
      if "$OLD_KASPAD_BIN" --help 2>/dev/null | grep -q -- "$f"; then log "dry: $OLD_KASPAD_BIN knows $f: it is not an older release"; problems=1; fi
    done
  else
    log "dry: OLD_KASPAD_BIN is not an executable (the int-11 release)"; problems=1
  fi
  [ -x "$CLI_BIN" ] || { log "dry: no misaka CLI at $CLI_BIN"; problems=1; }
  for f in SUBMIT_CONSTRAINT_CMD SUBMIT_PREFIX_CMD SUBMIT_ADAPTER_CMD; do
    [ -n "${!f:-}" ] || { log "dry: $f is not set: the claim producers are not in the kit yet (below/above will be INCOMPLETE)"; problems=1; }
  done
  if [ -d "$LOCK_DIR" ]; then log "dry: DRILL.lock is held by $(lock_holder) (wait-lock will poll)"; else log "dry: DRILL.lock is free"; fi
  log "dry: the plan —"
  log "  nodes    new0 gets: --palw-drill-fp-constraint-at=$CONSTRAINT_AT --palw-drill-fp-constraint2-at=$CONSTRAINT2_AT --palw-drill-fp-prefix-at=$PREFIX_AT"
  log "                      --palw-drill-fp-tokenizer-at=$TOKENIZER_AT --palw-drill-adapter-at=$ADAPTER_AT (and the runner's prerequisite lists)"
  log "           old gets none of them; the fork id separates them once new0 passes the first fence (DAA $CONSTRAINT_AT)"
  log "  old      $OLD_LOG names none of the five fences"
  log "  below    each claim command with BELOW=1 below its fence: dropped by name, block stands"
  log "  cross    new0 past DAA $CROSS_AT: 'Fork-id mismatch … crossed fence'; old stops while new0 goes on"
  log "  above    each claim command with BELOW=0: the claim licenses and reaches Final (new0's log, GetPalw* RPC)"
  if [ "$problems" -eq 0 ]; then log "dry: OK"; else log "dry: the plan is well-formed; resolve the items above before the run"; fi
}

step_old() {
  [ -s "$OLD_LOG" ] || incomplete "no old relay log at $OLD_LOG"
  mkdir -p "$EVIDENCE"
  local f
  for f in palw_fp_decode_constraint palw_fp_constraint_v2 palw_fp_prefix_state palw_fp_tokenizer_match palw_adapter_class_v1; do
    ! grep -q "PALW DRILL FLAG DAY: .*$f" "$OLD_LOG" || die "the old relay moved $f: it is the release under test"
  done
  log "PASS: the old relay moved none of the five fences"
  echo ok > "$EVIDENCE/old.pass"
}

run_claim() { # <var> <below 0|1>
  local cmd="${!1:-}"
  [ -n "$cmd" ] || incomplete "$1 is not set (the lane's claim producer is not in the kit yet)"
  BELOW="$2" bash -c "$cmd"
}

step_below() {
  need_lock; mkdir -p "$EVIDENCE"
  local daa; daa="$(new_daa)"
  [[ "$daa" =~ ^[0-9]+$ ]] || incomplete "new0's wRPC does not answer"
  [ "$daa" -lt "$((CONSTRAINT_AT - 1))" ] || incomplete "DAA $daa is too close to CONSTRAINT_AT=$CONSTRAINT_AT for the below-the-fence half"
  local from; from="$(cursor "$NEW0_LOG")"
  run_claim SUBMIT_CONSTRAINT_CMD 1
  wait_after "$NEW0_LOG" "$from" "produced no object: a constrained claim \(FP job version 6\) below palw_fp_decode_constraint" "the constrained claim refused by name" \
    > "$EVIDENCE/below-constraint.log"
  from="$(cursor "$NEW0_LOG")"; run_claim SUBMIT_PREFIX_CMD 1
  wait_after "$NEW0_LOG" "$from" "produced no object: a prefix-state claim \(FP job version 11\) below palw_fp_prefix_state" "the prefix-state claim refused by name" \
    > "$EVIDENCE/below-prefix.log"
  from="$(cursor "$NEW0_LOG")"; run_claim SUBMIT_ADAPTER_CMD 1
  wait_after "$NEW0_LOG" "$from" "an adapter class listing was dropped by name below palw_adapter_class_v1, and the block stands" "the adapter listing dropped by name" \
    > "$EVIDENCE/below-adapter.log"
  log "PASS: the three objects were refused by name below their fences"
  echo ok > "$EVIDENCE/below.pass"
}

step_cross() {
  mkdir -p "$EVIDENCE"
  local pattern="Fork-id mismatch on network [^ ]+ at DAA [0-9]+ - this node has crossed fence"
  wait_after "$NEW0_LOG" 0 "$pattern|judged connected peer .* disconnecting it: Fork-id mismatch" "new0 to refuse the old relay by the fork id" > "$EVIDENCE/cross-refusal.log"
  log "PASS: $(cat "$EVIDENCE/cross-refusal.log")"
}

step_above() {
  need_lock; mkdir -p "$EVIDENCE"
  local daa; daa="$(new_daa)"
  [[ "$daa" =~ ^[0-9]+$ ]] && [ "$daa" -gt "$ADAPTER_AT" ] || incomplete "new0 is at DAA ${daa:-?}, not yet past the last fence ($ADAPTER_AT)"
  run_claim SUBMIT_CONSTRAINT_CMD 0
  run_claim SUBMIT_PREFIX_CMD 0
  run_claim SUBMIT_ADAPTER_CMD 0
  wait_after "$NEW0_LOG" 0 "claim .* Final" "the claims reaching Final" > "$EVIDENCE/above-final.log"
  log "PASS: $(cat "$EVIDENCE/above-final.log")"
}

cmd_run() { need_env; step_old; step_below; step_cross; step_above; log "PASS: old, below, cross, above"; }

case "${1:-help}" in
  dry) cmd_dry ;;
  wait-lock) cmd_wait_lock ;;
  unlock) cmd_unlock ;;
  run) cmd_run ;;
  old) need_env; step_old ;;
  below) need_env; step_below ;;
  cross) need_env; step_cross ;;
  above) need_env; step_above ;;
  help | -h | --help | *) sed -n '2,52p' "$0"; [ "${1:-help}" = help ] || [ "${1:-}" = -h ] || [ "${1:-}" = --help ] || exit 2 ;;
esac
