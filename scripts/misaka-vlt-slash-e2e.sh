#!/usr/bin/env bash
# MISAKA — BFTLLMTOKEN PR 5: the slashing E2E, on a live weighted-finality devnet.
#
# §9 says a bond may be burned only for an offence PROVABLE from the transaction itself, and that
# a slash must not move the CURRENT epoch's denominator. This script proves both, for each
# provable offence, against a running 5-validator devnet whose frozen plan is 8/5/3/2/2:
#
#   1. double prevote      — two attestations, one (bond, validator, epoch), different anchors
#   2. double precommit    — two precommits, one epoch, different anchors        (Equivocation)
#   3. contradictory lock  — two precommits declaring one locked_epoch, two lock anchors
#
# Each is filed by `kaspa-pq-validator equivocate`, which signs BOTH payloads with a key this
# devnet owns and submits the self-contained evidence transaction. The equivocating votes never
# need to reach a block: the proof is the two signatures, which is exactly what makes the offence
# provable rather than adjudicated (see the memory note on ForgedReceipt).
#
# After each filing the script asserts, in order:
#
#   a. the accused bond reads Slashed                          (the offence is settled)
#   b. the frozen snapshot of the epoch in force at filing time is UNCHANGED, and any finality
#      certificate already persisted for it is unchanged        (§9: same denominator mid-epoch)
#   c. a later epoch's frozen snapshot drops EXACTLY the accused's weight (the slash takes effect
#      at a boundary, not retroactively). The wait is for the drop itself, not for a fixed number
#      of epochs: a snapshot is pinned at a canonical LAGGED anchor, so the first snapshot whose
#      pin post-dates the evidence is ~5 epochs after the newest TALLIED epoch at filing time
#      (~2 for the tally's own lag, ~2-3 for the pin's). Measuring the offset instead of assuming
#      it is what turns "the slash is late" into a number.
#   d. re-filing the identical evidence slashes nothing a second time  (replay is inert)
#
# Re-runnable: a victim whose bond is ALREADY Slashed when its case starts is treated as a
# re-run of that case — the settlement assertions are already answered by the chain, so only the
# replay check (d) runs. A first run against a live devnet still exercises every step, because
# there every bond starts Active.
#
# Usage:
#   scripts/misaka-vlt-slash-e2e.sh [--work-dir DIR] [--victims 4,3,2]
#
#   --victims  node indices to slash, one per case, in case order. Defaults to the three
#              smallest-weight validators (4, 3, 2) so the surviving weight stays above the
#              floor and the chain keeps finalizing while the experiment runs.
#
# Env: VALIDATOR_BIN, MISAMINER_BIN (defaults ./target/release/...)

set -euo pipefail

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
WORK_DIR="$REPO_ROOT/.misaka-vlt-quorum-e2e"
VICTIMS=4,3,2
while [ $# -gt 0 ]; do
  case "$1" in
    --work-dir) WORK_DIR="$2"; shift 2 ;;
    --victims)  VICTIMS="$2"; shift 2 ;;
    -h|--help)  sed -n '2,33p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

# The 8/5/3/2/2 plan in uRTE, by node index — the exact amount each victim's slash must remove
# from the denominator. Asserting the DELTA (not merely "it fell") is what distinguishes a slash
# from an unrelated weight change, e.g. a job ageing out of the credit window mid-case.
WEIGHTS=(400000000 250000000 150000000 100000000 100000000)

VALIDATOR_BIN="${VALIDATOR_BIN:-$REPO_ROOT/target/release/kaspa-pq-validator}"
MISAMINER_BIN="${MISAMINER_BIN:-$REPO_ROOT/target/release/misaminer}"
BASE_RPC="${MISAKA_DEVNET_BASE_RPC:-17310}"
[ -x "$VALIDATOR_BIN" ] || { echo "missing $VALIDATOR_BIN" >&2; exit 1; }
[ -d "$WORK_DIR/node-0" ] || { echo "no devnet at $WORK_DIR" >&2; exit 1; }

grpc_of() { echo $((BASE_RPC + $1 * 10)); }
wrpc_of() { echo $((BASE_RPC + $1 * 10 + 2)); }
log_of()  { echo "$WORK_DIR/node-$1/kaspad.log"; }

bond_of() { # node index -> outpoint from its recorded args
  tr ' ' '\n' < "$WORK_DIR/node-$1/run.args" | sed "s/^'//;s/'$//" | { grep -E '^--stake-bond=' || true; } | tail -1 | cut -d= -f2
}

# The frozen snapshot line for one epoch, as recorded by the OBSERVER node (0). Empty until
# that epoch's boundary recompute has run.
frozen_line() { # epoch
  { grep -oE "\[vlt-voting-snapshot\] frozen epoch=$1 [^(]*" "$(log_of 0)" || true; } | head -1
}
certificate_line() { # epoch
  { grep -oE "\[dns-finality-certificate\] persisted epoch=$1 [^(]*" "$(log_of 0)" || true; } | head -1
}
newest_frozen_epoch() {
  { { grep -oE "\[vlt-voting-snapshot\] frozen epoch=[0-9]+" "$(log_of 0)" || true; } | grep -oE "[0-9]+$" | sort -n | tail -1; } || true
}
newest_frozen_total() {
  { { grep -oE "\[vlt-voting-snapshot\] frozen epoch=[0-9]+ .*total_weight=[0-9]+" "$(log_of 0)" || true; } |
    tail -1 | grep -oE "total_weight=[0-9]+" | cut -d= -f2; } || true
}
current_epoch() {
  { grep -oE "\[vlt-quorum\] epoch=[0-9]+" "$(log_of 0)" || true; } | tail -1 | cut -d= -f2
}
wait_for_epoch() { # target epoch, timeout secs
  local target="$1" deadline=$(( $(date +%s) + $2 ))
  while :; do
    local e; e=$(current_epoch)
    [ -n "$e" ] && [ "$e" -ge "$target" ] && return 0
    [ "$(date +%s)" -ge "$deadline" ] && { echo "FAIL: epoch $target not reached in $2s (at ${e:-?})" >&2; return 1; }
    sleep 10
  done
}
bond_status_of() { # node index — the node's own heartbeat is the authority it acts on
  { grep -oE "bond=[A-Za-z]+" "$(log_of "$1")" || true; } | tail -1 | cut -d= -f2
}

VICS=()
IFS=, read -r -a VICS <<< "$VICTIMS"
KINDS=(prevote precommit lock)
[ "${#VICS[@]}" -eq "${#KINDS[@]}" ] || { echo "--victims needs ${#KINDS[@]} node indices" >&2; exit 2; }

# The reporter is node 0 throughout: it holds a funded key, and paying ITSELF the reporter reward
# keeps the accounting in one place.
REPORTER=0

# The whole plan. Every case ends by asserting one invariant:
#
#     frozen denominator  ==  TOTAL_PLAN - Σ(weight of every bond that currently reads Slashed)
#
# Derived from the CHAIN's view of which bonds are slashed, not from a loop counter, because
# slashes settle into the denominator several epochs after the bond flips — so while one case is
# running, an earlier case's slash may still be landing. A counter would read that as this case
# burning twice; the invariant reads it as what it is. It also makes the harness re-runnable and
# order-independent, and it still catches the two failures worth catching: a slash that never
# reduces the denominator, and one that reduces it twice.
TOTAL_PLAN=0
for w in "${WEIGHTS[@]}"; do TOTAL_PLAN=$((TOTAL_PLAN + w)); done

expected_total_now() {
  local burned=0 i
  for i in "${VICS[@]}"; do
    [ "$(bond_status_of "$i")" = "Slashed" ] && burned=$((burned + WEIGHTS[i]))
  done
  echo $((TOTAL_PLAN - burned))
}

echo "MISAKA slashing E2E on $WORK_DIR"
echo "  reporter : node-$REPORTER"
echo "  cases    : ${KINDS[*]} against nodes ${VICS[*]}"
echo

for idx in "${!KINDS[@]}"; do
  kind="${KINDS[$idx]}"
  victim="${VICS[$idx]}"
  vbond=$(bond_of "$victim")
  [ -n "$vbond" ] || { echo "FAIL: node-$victim has no recorded --stake-bond" >&2; exit 1; }

  epoch_at_filing=$(current_epoch)
  [ -n "$epoch_at_filing" ] || { echo "FAIL: no [vlt-quorum] line yet — is the weight fence open?" >&2; exit 1; }
  before_frozen=$(frozen_line "$epoch_at_filing")
  before_cert=$(certificate_line "$epoch_at_filing")

  already_slashed=0
  [ "$(bond_status_of "$victim")" = "Slashed" ] && already_slashed=1
  if [ "$already_slashed" -eq 1 ]; then
    echo "=== case $((idx + 1)): $kind — node-$victim is already Slashed; running the replay check only (re-run)"
  else
    echo "=== case $((idx + 1)): $kind equivocation by node-$victim (bond ${vbond:0:16}…) at epoch $epoch_at_filing"
  fi

  # The accused signs both conflicting payloads with its OWN key — that is the whole proof.
  if ! "$VALIDATOR_BIN" equivocate \
      --node-wrpc-borsh="127.0.0.1:$(wrpc_of "$victim")" \
      --validator-key="$WORK_DIR/node-$victim/validator.key" \
      --stake-bond="$vbond" \
      --kind="$kind" \
      --reporter-key="$WORK_DIR/node-$REPORTER/validator.key" \
      --reporter-node-wrpc-borsh="127.0.0.1:$(wrpc_of "$REPORTER")" \
      2>&1 | tee -a "$WORK_DIR/slash-e2e.log" | tail -3; then
    echo "FAIL case $((idx + 1)): could not file $kind evidence" >&2
    exit 1
  fi

  if [ "$already_slashed" -eq 1 ]; then
    echo "SKIP $kind (a-b): settled on an earlier run"
  else

  # (a) the bond settles as Slashed.
  deadline=$(( $(date +%s) + 600 ))
  while :; do
    st=$(bond_status_of "$victim")
    [ "$st" = "Slashed" ] && break
    if [ "$(date +%s)" -ge "$deadline" ]; then
      echo "FAIL case $((idx + 1)): node-$victim bond is '$st', never Slashed" >&2
      exit 1
    fi
    sleep 10
  done
  echo "PASS $kind (a): node-$victim bond settled as Slashed"

  # (b) §9: the epoch in force when the evidence landed keeps its denominator and its certificate.
  after_frozen=$(frozen_line "$epoch_at_filing")
  after_cert=$(certificate_line "$epoch_at_filing")
  if [ "$before_frozen" != "$after_frozen" ]; then
    echo "FAIL case $((idx + 1)): epoch $epoch_at_filing's frozen snapshot CHANGED across the slash" >&2
    echo "  before: $before_frozen" >&2
    echo "  after : $after_frozen" >&2
    exit 1
  fi
  if [ -n "$before_cert" ] && [ "$before_cert" != "$after_cert" ]; then
    echo "FAIL case $((idx + 1)): epoch $epoch_at_filing's finality certificate CHANGED across the slash" >&2
    exit 1
  fi
  echo "PASS $kind (b): epoch $epoch_at_filing's denominator and certificate unchanged"
  fi

  # (c) the denominator settles at exactly `plan - everything burned so far`. A snapshot is pinned
  # at a canonical LAGGED anchor, so this lands some epochs after the filing rather than at the
  # next boundary — the wait is for the value, and the lag it took is reported rather than assumed.
  deadline=$(( $(date +%s) + 1800 ))
  while :; do
    expected_after=$(expected_total_now)
    now_total=$(newest_frozen_total)
    [ "$now_total" = "$expected_after" ] && break
    if [ "$(date +%s)" -ge "$deadline" ]; then
      echo "FAIL case $((idx + 1)): denominator is ${now_total:-?}, expected $expected_after (plan $TOTAL_PLAN less every Slashed bond's weight)" >&2
      exit 1
    fi
    sleep 20
  done
  settle_epoch=$(newest_frozen_epoch)
  echo "PASS $kind (c): denominator settled at $expected_after (= $TOTAL_PLAN less every Slashed bond) by epoch $settle_epoch, $((settle_epoch - epoch_at_filing)) epoch(s) after filing"

  # (d) replay: the identical evidence must settle nothing the second time.
  slashes_before=$(grep -c "slashing" "$(log_of 0)" 2>/dev/null || echo 0)
  "$VALIDATOR_BIN" equivocate \
    --node-wrpc-borsh="127.0.0.1:$(wrpc_of "$victim")" \
    --validator-key="$WORK_DIR/node-$victim/validator.key" \
    --stake-bond="$vbond" \
    --kind="$kind" \
    --reporter-key="$WORK_DIR/node-$REPORTER/validator.key" \
    --reporter-node-wrpc-borsh="127.0.0.1:$(wrpc_of "$REPORTER")" \
    >>"$WORK_DIR/slash-e2e.log" 2>&1 || true
  sleep 120
  st=$(bond_status_of "$victim")
  if [ "$st" != "Slashed" ]; then
    echo "FAIL case $((idx + 1)): after a replayed filing the bond reads '$st'" >&2
    exit 1
  fi
  # A second settlement would remove this bond's weight twice, breaking the invariant downward.
  expected_after=$(expected_total_now)
  now_total=$(newest_frozen_total)
  if [ "$now_total" != "$expected_after" ]; then
    echo "FAIL case $((idx + 1)): after the replay the denominator is $now_total, expected $expected_after — the stake was removed more than once" >&2
    exit 1
  fi
  echo "PASS $kind (d): replayed evidence settled nothing further (bond stays Slashed, denominator holds at $expected_after)"
  echo
done

echo "PR-5 slashing E2E PASSED"
