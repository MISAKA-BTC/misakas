#!/usr/bin/env bash
# Commit → open → verify, across processes, plus the two refusals that make the seam safe.
#
# For every golden job:
#   1. v2-golden-envelope        — the canonical envelope frame
#   2. v2-legs-job               — execute, commit (PalwLegsJobResultV1)
#   3. v2-legs-open-request      — build the opening CALL (envelope + deterministic request)
#   4. v2-legs-open              — a SECOND process re-executes the job and answers
#   5. v2-legs-open-verify       — a model-free process adjudicates the answer
#
# Step 4 is the property under test: the answering process never saw the committing process's
# tree — it reproduces it, which is what a determinism class means operationally.
#
# Then the negative half:
#   * a result frame with one flipped bit in its committed root cannot be laundered through the
#     harness — the answering runtime re-derives the true root and REFUSES (an honest runtime
#     never fabricates openings for a tree it cannot reproduce);
#   * an answer with a flipped byte must fail the model-free verifier.
#
# Every stream carries exactly one frame (the v2 contract); multi-input modes take file args.
#
# Env: MISAKA_PALW_GGUF (pinned model), MISAKA_PALW_GOLDEN (the golden set of this class),
#      WORKER (optional; defaults to target/release/palw-worker).
set -euo pipefail

WORKER=${WORKER:-target/release/palw-worker}
: "${MISAKA_PALW_GGUF:?point MISAKA_PALW_GGUF at the pinned Qwen3.5-2B GGUF}"
: "${MISAKA_PALW_GOLDEN:?point MISAKA_PALW_GOLDEN at the golden set of this class}"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

JOBS=(golden-min-1tok-d1 golden-probe-12tok-d16 golden-prefill96-d16 golden-repeat8-d2)

for name in "${JOBS[@]}"; do
  "$WORKER" --mode v2-golden-envelope --name "$name" > "$TMP/$name.env"
  "$WORKER" --mode v2-legs-job < "$TMP/$name.env" > "$TMP/$name.result" 2> "$TMP/$name.job.err"
  "$WORKER" --mode v2-legs-open-request --envelope "$TMP/$name.env" < "$TMP/$name.result" > "$TMP/$name.call"
  "$WORKER" --mode v2-legs-open < "$TMP/$name.call" > "$TMP/$name.answer" 2> "$TMP/$name.open.err"
  "$WORKER" --mode v2-legs-open-verify --call "$TMP/$name.call" < "$TMP/$name.answer" > "$TMP/$name.verdict"
  grep -q '"answer_valid": true' "$TMP/$name.verdict"
  echo "[legs-open-e2e] $name: commit → open (fresh process) → verify OK"
done

# --- refusal 1: a corrupted commitment cannot be laundered into openings -------------------
# The committed execution root is the last 64 bytes of the result frame (the binding's final
# field). Flip its last byte, rebuild the call from the tampered result, and the answering
# runtime — which re-derives the true root by re-execution — must refuse with nothing on
# stdout.
name=golden-repeat8-d2
python3 - "$TMP/$name.result" "$TMP/$name.result.tampered" <<'PY'
import sys
data = bytearray(open(sys.argv[1], "rb").read())
data[-1] ^= 0xFF
open(sys.argv[2], "wb").write(bytes(data))
PY
"$WORKER" --mode v2-legs-open-request --envelope "$TMP/$name.env" < "$TMP/$name.result.tampered" > "$TMP/$name.call.tampered"
if "$WORKER" --mode v2-legs-open < "$TMP/$name.call.tampered" > "$TMP/refused.answer" 2> "$TMP/refused.err"; then
  echo "[legs-open-e2e] FAIL: the worker opened a commitment it cannot reproduce" >&2
  exit 1
fi
test ! -s "$TMP/refused.answer"
grep -q "refusing to open" "$TMP/refused.err"
echo "[legs-open-e2e] tampered commitment: answering runtime refused, nothing on stdout (correct)"

# --- refusal 2: a tampered answer must fail the model-free verifier ------------------------
# golden-probe-12tok-d16 carries a checkpoint; flip the answer's last byte (inside the final
# opened leaf) and the verifier must reject — whether the flip breaks decoding or an opening.
name=golden-probe-12tok-d16
python3 - "$TMP/$name.answer" "$TMP/$name.answer.tampered" <<'PY'
import sys
data = bytearray(open(sys.argv[1], "rb").read())
data[-1] ^= 0xFF
open(sys.argv[2], "wb").write(bytes(data))
PY
if "$WORKER" --mode v2-legs-open-verify --call "$TMP/$name.call" < "$TMP/$name.answer.tampered" > "$TMP/tampered.verdict" 2>/dev/null; then
  echo "[legs-open-e2e] FAIL: the verifier accepted a tampered answer" >&2
  exit 1
fi
echo "[legs-open-e2e] tampered answer: verifier rejected (correct)"

echo "[legs-open-e2e] PASS: ${#JOBS[@]} jobs opened and verified; both refusals held"
