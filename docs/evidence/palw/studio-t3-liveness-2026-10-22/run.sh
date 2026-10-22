#!/usr/bin/env bash
# studio/t3-liveness: run the timed measurements directly from the built test binaries (no cargo lock, no `nice`).
# Usage (repo root): bash docs/evidence/palw/studio-t3-liveness-2026-10-22/run.sh <build-log> [label]
#   <build-log> is the log build.sh wrote (it lists the "Executable" paths).
# Each batch records uptime and hardware before/after, and first waits up to WAIT_MAX_S (default 1200 s) for the
# 1-minute load average to fall below LOAD_MAX (default 5); if it does not, the batch runs anyway and is marked UNDER_LOAD,
# and later batches no longer wait (each still records whether load1 was below LOAD_MAX when it started).
set -u
cd "$(git rev-parse --show-toplevel)"
LOG="$1"
LABEL="${2:-$(uname -s)-$(uname -m)}"
OUT=docs/evidence/palw/studio-t3-liveness-2026-10-22/raw
mkdir -p "$OUT"
LOAD_MAX="${LOAD_MAX:-5}"
WAIT_MAX_S="${WAIT_MAX_S:-1200}"

FLOOR=$(grep -o "([^)]*studio_t3_court_close_floor-[^)]*)" "$LOG" | tail -1 | tr -d '()')
HELD=$(grep -o "([^)]*palw_held_close_chunks-[^)]*)" "$LOG" | tail -1 | tr -d '()')
REPLAY=$(grep -o "([^)]*audit_liveness_court_close_replay-[^)]*)" "$LOG" | tail -1 | tr -d '()')
FENCES=$(grep -o "([^)]*audit_liveness_court_fences-[^)]*)" "$LOG" | tail -1 | tr -d '()')
LIB=$(grep -o "([^)]*kaspa_consensus_core-[^)]*)" "$LOG" | tail -1 | tr -d '()')
if [ "$(uname -s)" = Darwin ]; then TIMEV="/usr/bin/time -l"; else TIMEV="/usr/bin/time -v"; fi
[ -x /usr/bin/time ] || TIMEV=""   # Linux without GNU time (apt install time): run without the RSS line

hw() {
  if [ "$(uname -s)" = Darwin ]; then sysctl hw.model hw.memsize hw.ncpu; else
    echo "model: $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2)"; echo "memsize: $(grep MemTotal /proc/meminfo)"; echo "ncpu: $(nproc)"; fi
}
load1() { if [ "$(uname -s)" = Darwin ]; then sysctl -n vm.loadavg | awk '{print $2}'; else cut -d' ' -f1 /proc/loadavg; fi; }

batch() {
  local name="$1"; shift
  local f="$OUT/${name}-${LABEL}-$(date +%Y%m%dT%H%M%S).log"
  local waited=0 marker=QUIET
  while awk -v l="$(load1)" -v m="$LOAD_MAX" 'BEGIN{exit !(l>=m)}'; do
    # After one full wait has failed, later batches do not wait again (the load is not going to clear).
    if [ "$waited" -ge "$WAIT_MAX_S" ] || [ -n "${GAVE_UP:-}" ]; then marker=UNDER_LOAD; GAVE_UP=1; break; fi
    sleep 60; waited=$((waited+60))
  done
  {
    echo "=== batch $name ($marker; waited ${waited}s for load1<$LOAD_MAX) ==="
    echo "commit: $(git rev-parse HEAD)"; echo "binary: $1"; shasum -a 256 "$1" 2>/dev/null || sha256sum "$1"
    echo "--- before"; date -u; uptime; hw
    $TIMEV "$@"
    echo "exit=$?"
    echo "--- after"; date -u; uptime; hw
  } >"$f" 2>&1
  echo "$f ($marker)"
}

batch fences "$FENCES" --exact audit_liveness_court_print_t12_fences --nocapture --test-threads=1
batch court_replay_orig "$REPLAY" --exact audit_liveness_court_refused_completion_floor_cost --nocapture --test-threads=1
batch court_floor "$FLOOR" --ignored --exact studio_t3_court_refused_completion_floor_cost_by_size --nocapture --test-threads=1
batch court_adjudication "$HELD" --ignored --exact studio_t3_court_adjudication_cost --nocapture --test-threads=1
batch lifecycle_orig "$LIB" --exact palw_state_v2::tests::liveness_lifecycle_v1::liveness_lifecycle_v1_per_object_rehearsal_and_empty_fold_scale_with_state_rows --nocapture --test-threads=1
batch lifecycle_scaling "$LIB" --ignored --exact palw_state_v2::tests::liveness_lifecycle_v1::studio_t3_lifecycle_rehearsal_clone_fold_vs_claim_rows --nocapture --test-threads=1
