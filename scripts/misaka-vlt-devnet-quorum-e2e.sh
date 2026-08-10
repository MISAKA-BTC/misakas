#!/usr/bin/env bash
# MISAKA — BFTLLMTOKEN PR 4: the §8 four-case weighted-quorum E2E, end to end.
#
# On a fresh 5-validator devnet whose frozen denominator is exactly the 8/5/3/2/2 plan
# (A=400 B=250 C=150 D=100 E=100 VLT, W=1000, Q=floor(2W/3)+1), prove that DNS finality is
# decided by WEIGHT and not by validator count, by controlling who signs:
#
#   case 1  B+C+D+E   4 signers   600 VLT  -> must NOT finalize (no certificate)
#   case 2  A+B       2 signers   650 VLT  -> must NOT finalize
#   case 3  A+B+C     3 signers   800 VLT  -> MUST finalize (certificate, signed=800)
#   case 4  A+C+D+E   4 signers   750 VLT  -> MUST finalize (certificate, signed=750)
#
# "Finalize" is read off the §7.2 artifact itself: a `[dns-finality-certificate] persisted`
# line with the exact signed weight, plus `[vlt-quorum]` lines showing the round arithmetic
# (signed/total/quorum, prevote/precommit met or not). Certificate epochs must be strictly
# ascending across the whole run — the finalized sequence is monotone.
#
# # Why the timeline looks the way it does
#
# The credit window slides: C_i(E) sums the last K epochs, so the full plan is only the
# denominator while every job sits in one window. The weight fence sits one full window above
# the shadow fence, which means the usable overlap between "full plan frozen" and "round live"
# is (job_start - shadow) - ~4.5 epochs. Jobs start when bonds activate, so this script makes
# the overlap by DELAYING THE BOND: shadow opens at DAA 200 with nobody bonded, the chain is
# burst-mined to ~2400, and only then are the validators bonded (~epoch 37 once maturity
# passes). With K=52 that yields ~20+ epochs in which W=1000 is frozen AND the precommit round
# is live — room for all four cases.
#
# Wall clock: ~100 minutes. Everything is asserted from logs; progress is printed per phase.
#
# Usage:
#   scripts/misaka-vlt-devnet-quorum-e2e.sh [--work-dir DIR] [--skip-setup]
#
#   --skip-setup   assume the devnet is already running with the full plan frozen and the
#                  weight fence crossed (re-run just the four cases).
#
# Env: KASPAD_BIN, MISAMINER_BIN (defaults ./target/release/...)

set -euo pipefail

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
WORK_DIR="$REPO_ROOT/.misaka-vlt-quorum-e2e"
SKIP_SETUP=0
while [ $# -gt 0 ]; do
  case "$1" in
    --work-dir)   WORK_DIR="$2"; shift 2 ;;
    --skip-setup) SKIP_SETUP=1; shift ;;
    -h|--help)    sed -n '2,38p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

MISAMINER_BIN="${MISAMINER_BIN:-$REPO_ROOT/target/release/misaminer}"
export MISAKA_DEVNET_DIR="$WORK_DIR"
# Away from both the PR-3 devnet (171xx) and any reorg soak (172xx).
export MISAKA_DEVNET_BASE_P2P="${MISAKA_DEVNET_BASE_P2P:-17311}"
export MISAKA_DEVNET_BASE_RPC="${MISAKA_DEVNET_BASE_RPC:-17310}"

NODES=5
SHADOW_DAA=200
EPOCHS_K=52
PRE_BOND_DAA=2400
EPOCH_BLOCKS=100
# Plan: node index -> weight (uRTE). A=0.
WEIGHTS=(400000000 250000000 150000000 100000000 100000000)
TOTAL=1000000000
QUORUM=666666667

rpc_of() { echo $((MISAKA_DEVNET_BASE_RPC + $1 * 10)); }
log_of() { echo "$WORK_DIR/node-$1/kaspad.log"; }

burst_mine() { # node_index blocks
  "$MISAMINER_BIN" --rpc="127.0.0.1:$(rpc_of "$1")" --network-id=devnet --allow-burn --threads=2 \
    --blocks="$2" --min-block-interval-ms=0 >>"$WORK_DIR/e2e-miner.log" 2>&1 || true
}

daa_now() { # from any running node's vlt-shadow / heartbeat lines
  { grep -oE "sink_daa=[0-9]+" "$(log_of "$1")" || true; } | tail -1 | cut -d= -f2
}

start_continuous_miner() { # node_index
  if [ -f "$WORK_DIR/miner.pid" ] && kill -0 "$(cat "$WORK_DIR/miner.pid")" 2>/dev/null; then
    kill "$(cat "$WORK_DIR/miner.pid")" 2>/dev/null || true
    sleep 1
  fi
  "$MISAMINER_BIN" --rpc="127.0.0.1:$(rpc_of "$1")" --network-id=devnet --allow-burn --threads=2 \
    >>"$WORK_DIR/miner.log" 2>&1 &
  echo $! > "$WORK_DIR/miner.pid"
}

stop_node() { # index
  local pidfile="$WORK_DIR/node-$1/kaspad.pid"
  if [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then
    kill "$(cat "$pidfile")" 2>/dev/null || true
  fi
}

start_node() { # index — relaunch from its own recorded args
  local node_dir="$WORK_DIR/node-$1"
  if [ -f "$node_dir/kaspad.pid" ] && kill -0 "$(cat "$node_dir/kaspad.pid")" 2>/dev/null; then
    return 0
  fi
  ( eval "exec $(cat "$node_dir/run.args")" >>"$node_dir/kaspad.log" 2>&1 & echo $! > "$node_dir/kaspad.pid" )
}

# ---------------------------------------------------------------- setup -----
if [ "$SKIP_SETUP" -eq 0 ]; then
  echo "=== phase 1: fresh devnet (shadow=$SHADOW_DAA, K=$EPOCHS_K, quotas 8/5/3/2/2, nobody bonded yet)"
  rm -rf "$WORK_DIR"
  "$REPO_ROOT/scripts/misaka-vlt-devnet.sh" --shadow-daa "$SHADOW_DAA" --epochs "$EPOCHS_K"

  echo
  echo "=== phase 2: burst-mine to DAA ~$PRE_BOND_DAA BEFORE bonding (this delay IS the case window)"
  burst_mine 0 "$PRE_BOND_DAA"

  echo
  echo "=== phase 3: bond all validators (their jobs, and the case window, start here)"
  # `|| true`: the bond script's own 40-second Active check is impatient — a bond reads Active a
  # couple of epochs after the restart, and its exit 1 for "not yet" must not abort the E2E.
  "$REPO_ROOT/scripts/misaka-vlt-devnet-bond.sh" || true
  deadline=$(( $(date +%s) + 900 ))
  while :; do
    active=0
    for i in $(seq 0 $((NODES - 1))); do
      if grep -qE "bond=Active" "$(log_of "$i")"; then active=$((active + 1)); fi
    done
    [ "$active" -eq "$NODES" ] && break
    if [ "$(date +%s)" -ge "$deadline" ]; then echo "FAIL: only $active/$NODES bonds Active after bonding" >&2; exit 1; fi
    sleep 10
  done
  echo "all $NODES bonds Active"
fi

echo
echo "=== phase 4: wait for the full plan (W=$TOTAL) to freeze, then for the weight fence"
deadline=$(( $(date +%s) + 4800 ))
while :; do
  if grep -qE "frozen epoch=[0-9]+ .*total_weight=$TOTAL" "$(log_of 0)"; then break; fi
  if [ "$(date +%s)" -ge "$deadline" ]; then echo "FAIL: full plan never froze" >&2; exit 1; fi
  sleep 20
done
echo "full plan frozen: $(grep -oE 'frozen epoch=[0-9]+ .*total_weight='$TOTAL "$(log_of 0)" | head -1 | grep -oE 'epoch=[0-9]+')"
while :; do
  if grep -qE "\[vlt-quorum\]" "$(log_of 0)"; then break; fi
  if [ "$(date +%s)" -ge "$deadline" ]; then echo "FAIL: the weight fence never opened (no [vlt-quorum] line)" >&2; exit 1; fi
  sleep 20
done
echo "the precommit round is live"

# ------------------------------------------------------------- the cases ----
# Per case: stop the complement, re-point the miner at the lowest ACTIVE node, wait for the
# verdict signal, assert, restart everyone, let two epochs settle. Every assertion is against
# lines that appeared AFTER the case began (byte offset of the observer log).
run_case() { # name actives(csv) expected_signed expect_finalize(1|0)
  local name="$1" actives="$2" expected="$3" expect="$4"
  local -a ACT=()
  IFS=, read -r -a ACT <<< "$actives"
  local observer="${ACT[0]}"
  local obs_log; obs_log="$(log_of "$observer")"

  echo
  echo "=== case $name: signers {$actives} = $expected uRTE vs Q=$QUORUM — expect $([ "$expect" -eq 1 ] && echo FINALIZE || echo 'NO finality')"
  # Mark where this case starts in the observer's log.
  local start_ofs; start_ofs=$(wc -c < "$obs_log")

  for i in $(seq 0 $((NODES - 1))); do
    case ",$actives," in
      *,"$i",*) start_node "$i" ;;
      *) stop_node "$i" ;;
    esac
  done
  start_continuous_miner "$observer"

  # A case verdict needs the votes of a FRESH epoch (one that became ready entirely inside the
  # case) to have settled: give it up to ~8 epochs of wall clock.
  local case_deadline=$(( $(date +%s) + 900 ))
  local verdict=""
  while [ -z "$verdict" ]; do
    if [ "$(date +%s)" -ge "$case_deadline" ]; then verdict="timeout"; break; fi
    sleep 15
    # Certificates that appeared since the case began, with the exact expected signed weight.
    local certs
    certs=$(tail -c +"$((start_ofs + 1))" "$obs_log" | { grep -oE "\[dns-finality-certificate\] persisted epoch=[0-9]+ [^ ]+ signed=[0-9]+/$TOTAL" || true; })
    if [ "$expect" -eq 1 ]; then
      if echo "$certs" | grep -q "signed=$expected/$TOTAL"; then verdict="pass"; fi
      # A certificate with the WRONG signed weight inside a controlled case is a failure worth
      # naming (it means somebody signed who should have been down).
      if [ -n "$certs" ] && ! echo "$certs" | grep -q "signed=$expected/$TOTAL"; then
        # Tolerate transition-epoch certificates only within the first 2 epochs of the case.
        if [ "$(( $(date +%s) + 900 - case_deadline ))" -gt 300 ]; then verdict="wrong-cert"; fi
      fi
    else
      if [ -n "$certs" ]; then verdict="unexpected-cert"; break; fi
      # The negative verdict: a settled quorum line showing exactly the expected signed weight,
      # a full-plan denominator, and prevote NOT met.
      if tail -c +"$((start_ofs + 1))" "$obs_log" |
        grep -qE "\[vlt-quorum\] epoch=[0-9]+ signed=$expected total=$TOTAL quorum=$QUORUM prevote=no precommit=no"; then
        verdict="pass"
      fi
    fi
  done

  # Restart everything for the next case (and so the mesh recovers between cases).
  for i in $(seq 0 $((NODES - 1))); do start_node "$i"; done
  start_continuous_miner 0

  case "$verdict" in
    pass)
      if [ "$expect" -eq 1 ]; then
        echo "PASS case $name: certificate persisted with signed=$expected/$TOTAL (>= Q=$QUORUM)"
      else
        echo "PASS case $name: signed=$expected < Q=$QUORUM — prevote never met, no certificate"
      fi
      ;;
    unexpected-cert) echo "FAIL case $name: a finality certificate appeared with only $expected uRTE signing" >&2; exit 1 ;;
    wrong-cert)      echo "FAIL case $name: certificate appeared with a signed weight other than $expected" >&2; exit 1 ;;
    timeout)         echo "FAIL case $name: no settled verdict within the case window — see $obs_log" >&2; exit 1 ;;
  esac
  # Two epochs of settle so the next case's epochs are cleanly attributable.
  sleep $((2 * EPOCH_BLOCKS))
}

#            name        actives    signed      finalize?
run_case  "B+C+D+E=600"  "1,2,3,4"  600000000   0
run_case  "A+B=650"      "0,1"      650000000   0
run_case  "A+B+C=800"    "0,1,2"    800000000   1
run_case  "A+C+D+E=750"  "0,2,3,4"  750000000   1

# ------------------------------------------------------- monotonicity -------
echo
echo "=== finalized-sequence monotonicity (certificate epochs strictly ascending, all nodes)"
for i in $(seq 0 $((NODES - 1))); do
  seq_epochs=$({ grep -oE "\[dns-finality-certificate\] persisted epoch=[0-9]+" "$(log_of "$i")" || true; } | grep -oE "[0-9]+$")
  if [ -n "$seq_epochs" ]; then
    if ! echo "$seq_epochs" | awk 'NR>1 && $1 <= prev { exit 1 } { prev=$1 }'; then
      echo "FAIL: node-$i persisted certificate epochs out of order" >&2
      exit 1
    fi
  fi
done
echo "PASS: certificate epochs strictly ascending on every node that persisted any"

echo
echo "PR-4 four-case weighted-quorum E2E PASSED"
