#!/usr/bin/env bash
# kaspa-pq PQ-only CI guard — ADR-0019 / docs/kaspa-pq-design-mldsa87.md §14.
#
#   1) Advisory audit of dependencies (libcrux-ml-dsa et al.) — active now.
#   2) secp256k1 MUST be absent from the kaspa-consensus dependency tree.
#      Phase 8 (PR-19-S8a/S8b) gated secp256k1 behind the `legacy-secp256k1`
#      feature, so this is now a HARD failure by default. Export HARD_SECP_GATE=0
#      to soften it back to a warning (e.g. while bisecting a regression).
#   3) ML-DSA-87 FIPS-204 KAT + consensus verifier tests MUST pass (audit H-10):
#      the deterministic keygen/sign regression pins, the portable-vs-SIMD backend
#      differential, and the verify/roundtrip/rejection tests catch a
#      libcrux-ml-dsa primitive change before release.
#
# Usage: scripts/pq-ci-guard.sh
set -uo pipefail
cd "$(dirname "$0")/.."

fail=0

echo "== [1/3] dependency advisory audit =="
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check advisories || fail=1
elif command -v cargo-audit >/dev/null 2>&1; then
  cargo audit || fail=1
else
  echo "WARN: neither cargo-deny nor cargo-audit installed; skipping advisory audit."
  echo "      install: cargo install cargo-deny  (or cargo-audit)"
fi

echo "== [2/3] secp256k1 must be absent from the consensus + node + wallet trees (Phase-8/S9/QL-1 gate) =="
# Phase 8 (PR-19-S8a/S8b) feature-gated secp256k1 out of the consensus tree; S9
# extended this to the kaspad node binary (the RPC/SDK layer:
# rpc-core -> consensus-wasm -> consensus-client). Audit QL-1 (P10) extended the
# fence through the whole wallet stack (bip32 / wallet-keys / wallet-pskt /
# wallet-core, all default pq-only), so every production binary is now secp-free.
# The gate is HARD by default. Export HARD_SECP_GATE=0 to soften it to a warning.
HARD_SECP_GATE="${HARD_SECP_GATE:-1}"
for crate in kaspa-consensus kaspad kaspa-pq-cli kaspa-wallet kaspa-cli kaspa-daemon misaminer kaspa-pq-miner kaspa-pq-validator; do
  if cargo tree -p "$crate" -e normal 2>/dev/null | grep -qi secp256k1; then
    echo "secp256k1 IS present in the $crate dependency tree."
    if [ "$HARD_SECP_GATE" = "1" ]; then
      echo "  -> FAIL: PQ-only release must not link secp256k1 into $crate."
      fail=1
    else
      echo "  -> soft warning (HARD_SECP_GATE=0); Phase 8/S9 expects this to be empty."
    fi
  else
    echo "OK: no secp256k1 in the $crate dependency tree."
  fi
done

echo "== [3/3] ML-DSA-87 FIPS-204 KAT + consensus verifier gate (audit H-10) =="
# The deterministic keygen/sign regression pins (kat_mldsa87_deterministic_regression),
# the portable-vs-SIMD backend differential (mldsa87_portable_matches_multiplexed_verify),
# and the verify/roundtrip/rejection tests must pass before any release: a
# libcrux-ml-dsa version bump that changes the primitive (or a CPU-backend
# divergence) is caught here. Pre-mainnet, ALSO vendor official NIST ACVP ML-DSA-87
# vectors / an independent-impl differential (see the libcrux checklist in Cargo.toml).
if cargo test -p kaspa-txscript --lib mldsa87 >/dev/null 2>&1; then
  echo "OK: ML-DSA-87 KAT + consensus verifier tests pass."
else
  echo "  -> FAIL: ML-DSA-87 KAT / verifier tests did not pass."
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "PQ CI guard: FAIL"
else
  echo "PQ CI guard: OK"
fi
exit "$fail"
