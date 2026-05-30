#!/usr/bin/env bash
# kaspa-pq PQ-only CI guard — ADR-0019 / docs/kaspa-pq-design-mldsa87.md §14.
#
#   1) Advisory audit of dependencies (libcrux-ml-dsa et al.) — active now.
#   2) secp256k1 MUST be absent from the kaspa-consensus dependency tree.
#      This is the Phase-8 gate: it is a SOFT warning during phases 1–7
#      (secp256k1 is still pulled in until the legacy-secp256k1 feature is
#      gated out) and becomes a HARD failure once Phase 8 lands.
#
# Usage: scripts/pq-ci-guard.sh
set -uo pipefail
cd "$(dirname "$0")/.."

fail=0

echo "== [1/2] dependency advisory audit =="
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check advisories || fail=1
elif command -v cargo-audit >/dev/null 2>&1; then
  cargo audit || fail=1
else
  echo "WARN: neither cargo-deny nor cargo-audit installed; skipping advisory audit."
  echo "      install: cargo install cargo-deny  (or cargo-audit)"
fi

echo "== [2/2] secp256k1 must be absent from kaspa-consensus tree (Phase-8 gate) =="
# Set HARD_SECP_GATE=1 once Phase 8 (secp256k1 feature isolation) has landed.
HARD_SECP_GATE="${HARD_SECP_GATE:-0}"
if cargo tree -p kaspa-consensus -e normal 2>/dev/null | grep -qi secp256k1; then
  echo "secp256k1 IS present in the kaspa-consensus dependency tree."
  if [ "$HARD_SECP_GATE" = "1" ]; then
    echo "  -> FAIL: PQ-only release must not link secp256k1 into consensus."
    fail=1
  else
    echo "  -> soft warning (expected during phases 1–7; gate hardens in Phase 8)."
  fi
else
  echo "OK: no secp256k1 in the kaspa-consensus dependency tree."
fi

if [ "$fail" -ne 0 ]; then
  echo "PQ CI guard: FAIL"
else
  echo "PQ CI guard: OK"
fi
exit "$fail"
