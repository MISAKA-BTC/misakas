#!/usr/bin/env bash
# MISAKA — calibrate the portable CPU compute profile on ONE machine.
#
# This is the last unchecked box before the ADR-0024 step-3 (SHADOW) fence can be scheduled:
# `docs/testnet10-vlt-shadow-fork-runbook.md` requires that two machines of the same architecture
# independently reproduce a job's `gemm_trace_root`, because that digest is what a verifier's
# replay has to match byte-for-byte. Determinism ON one machine is already proven in CI-style
# reruns; what a fleet needs is determinism ACROSS machines, and no single host can show it.
#
# So: run this on every candidate fleet machine and compare the one line it prints. Identical
# lines on two machines of one architecture = that architecture's class is calibrated and the
# fence may be scheduled for it. A difference means the class is not real on that hardware and
# the fence must NOT be scheduled — an executor and its committee would refute each other while
# both were honest.
#
# Usage (on each machine):
#   MISAKA_PALW_GGUF=/path/to/Qwen3.5-2B-Q4_K_M.gguf \
#     scripts/misaka-palw-cpu-calibrate.sh [--worker ./palw-worker]
#
# Exit codes: 0 = this machine is internally consistent and printed its fingerprint;
#             1 = the worker is misconfigured, or this machine is not even self-consistent
#                 (in which case its hardware or build is the problem, not the class).

set -euo pipefail

WORKER="./target/release/palw-worker"
while [ $# -gt 0 ]; do
  case "$1" in
    --worker) WORKER="$2"; shift 2 ;;
    -h|--help) sed -n '2,26p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[ -x "$WORKER" ] || { echo "no worker at $WORKER (build with MISAKA_PALW_CPU=1)" >&2; exit 1; }
[ -n "${MISAKA_PALW_GGUF:-}" ] || { echo "MISAKA_PALW_GGUF must point at the pinned GGUF" >&2; exit 1; }

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# The job is fixed HERE, not taken from the caller: a calibration whose input differs between
# machines compares nothing. These exact bytes and this exact ceiling are the calibration.
printf 'The capital of France is\n[misaka-calibration-v1]' > "$WORK/prompt.txt"
N_PREDICT=64

manifest=$("$WORKER" --mode manifest)
class=$(echo "$manifest" | grep -oE '"runtime_class_id":"[0-9a-f]+"' | cut -d'"' -f4)
runtime=$(echo "$manifest" | grep -oE '"runtime_manifest_hash":"[0-9a-f]+"' | cut -d'"' -f4)

# Three independent executions, two of them in the mode a verifier actually uses. A machine that
# cannot reproduce its OWN result has a hardware or build problem and must be excluded before it
# is ever compared with another — otherwise a cross-machine mismatch gets blamed on the class.
for i in 1 2 3; do
  mode=$([ "$i" = 1 ] && echo self-job || echo verify)
  ( cd "$WORK" && "$OLDPWD/$WORKER" --mode "$mode" --prompt-stdin --n-predict "$N_PREDICT" \
      < "$WORK/prompt.txt" > "$WORK/run$i.json" 2> "$WORK/run$i.err" ) \
    || { echo "FAIL: the worker exited non-zero on run $i:"; tail -3 "$WORK/run$i.err"; exit 1; }
done
if ! cmp -s "$WORK/run1.json" "$WORK/run2.json" || ! cmp -s "$WORK/run1.json" "$WORK/run3.json"; then
  echo "FAIL: this machine does not reproduce its own result — exclude it from the fleet, and do"
  echo "      not read a cross-machine mismatch involving it as evidence about the class."
  exit 1
fi

trace=$(grep -oE '"gemm_trace_root":"[0-9a-f]+"' "$WORK/run1.json" | cut -d'"' -f4)
output=$(grep -oE '"output_commitment":"[0-9a-f]+"' "$WORK/run1.json" | cut -d'"' -f4)
cu=$(grep -oE '"canonical_compute_units":[0-9]+' "$WORK/run1.json" | cut -d: -f2)

# One line, copy-pasteable. `arch` is here because the class is arch-scoped: comparing an aarch64
# line with an x86_64 line proves nothing and is EXPECTED to differ — they are different classes
# (`ggml/src/ggml-cpu/arch/` has separate SIMD kernels for each).
echo
echo "MISAKA-PALW-CPU-CALIBRATION-v1 arch=$(uname -m) os=$(uname -s) class=${class:0:16} runtime=${runtime:0:16} cu=$cu output=${output:0:32} trace=${trace:0:32}"
echo
echo "Compare this line with another machine of the SAME arch. Identical => that class is"
echo "calibrated; record both lines in the runbook and the fence may be scheduled for it."
