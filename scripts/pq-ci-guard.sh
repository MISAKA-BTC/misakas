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

echo "== [1/6] dependency advisory audit =="
# HARD by default: a missing advisory tool must NOT silently pass the gate (the whole point is to
# catch a libcrux-ml-dsa / dependency advisory before release). Export HARD_ADVISORY_GATE=0 to soften
# the missing-tool case to a warning (e.g. local runs without the tool installed).
HARD_ADVISORY_GATE="${HARD_ADVISORY_GATE:-1}"
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check advisories || fail=1
elif command -v cargo-audit >/dev/null 2>&1; then
  cargo audit || fail=1
else
  echo "neither cargo-deny nor cargo-audit installed; cannot run the advisory audit."
  echo "  install: cargo install cargo-deny  (or cargo-audit)"
  if [ "$HARD_ADVISORY_GATE" = "1" ]; then
    echo "  -> FAIL (set HARD_ADVISORY_GATE=0 to soften to a warning)."
    fail=1
  else
    echo "  -> WARN: skipping advisory audit (HARD_ADVISORY_GATE=0)."
  fi
fi

echo "== [2/6] secp256k1 must be absent from the consensus + node + wallet trees (Phase-8/S9/QL-1 gate) =="
# Phase 8 (PR-19-S8a/S8b) feature-gated secp256k1 out of the consensus tree; S9
# extended this to the kaspad node binary (the RPC/SDK layer:
# rpc-core -> consensus-wasm -> consensus-client). Audit QL-1 (P10) extended the
# fence through the whole wallet stack (bip32 / wallet-keys / wallet-pskt /
# wallet-core, all default pq-only), so every production binary is now secp-free.
# The gate is HARD by default. Export HARD_SECP_GATE=0 to soften it to a warning.
HARD_SECP_GATE="${HARD_SECP_GATE:-1}"
# EVM audit C2: also forbid k256 (revm/alloy's pure-Rust secp curve, the
# ecrecover backend) in the DEFAULT trees — it may only enter via the opt-in
# `evm` cargo feature. The pattern anchors "k256 v" as a crate-name token so
# it cannot false-match secp256k1 itself.
for crate in kaspa-consensus kaspad kaspa-pq-cli kaspa-wallet kaspa-cli kaspa-daemon misaminer kaspa-pq-miner kaspa-pq-validator; do
  if cargo tree -p "$crate" -e normal 2>/dev/null | grep -qiE 'secp256k1|(^|[^a-z0-9_-])k256 v'; then
    echo "secp256k1/k256 IS present in the $crate dependency tree."
    if [ "$HARD_SECP_GATE" = "1" ]; then
      echo "  -> FAIL: PQ-only release must not link a secp curve into $crate (k256 belongs behind --features evm only)."
      fail=1
    else
      echo "  -> soft warning (HARD_SECP_GATE=0); Phase 8/S9 expects this to be empty."
    fi
  else
    echo "OK: no secp256k1/k256 in the $crate dependency tree."
  fi
done

echo "== [3/6] ML-DSA-87 FIPS-204 KAT + official NIST ACVP + verifier gate (audit H-10/H-04) =="
# The deterministic keygen/sign regression pins (kat_mldsa87_deterministic_regression),
# the OFFICIAL NIST ACVP FIPS-204 differential (acvp_mldsa87_official_nist_vectors — audit
# H-04: keygen/sign/verify cross-checked against usnistgov/ACVP-Server vectors, the
# independent-source check), the portable-vs-SIMD backend differential
# (mldsa87_portable_matches_multiplexed_verify), and the verify/roundtrip/rejection tests
# must pass before any release: a libcrux-ml-dsa version bump that changes the primitive (or
# a CPU-backend divergence, or a drift from the standard) is caught here. The `mldsa87`
# filter matches all of the above (incl. the `acvp_mldsa87_*` differential).
if cargo test -p kaspa-txscript --lib mldsa87 >/dev/null 2>&1; then
  echo "OK: ML-DSA-87 KAT + consensus verifier tests pass."
else
  echo "  -> FAIL: ML-DSA-87 KAT / verifier tests did not pass."
  fail=1
fi

echo "== [4/6] normative spec must not carry stale ML-DSA-65 copy-paste hazards (audit M-03) =="
# The current scheme is ML-DSA-87. Forbid the concrete WRONG values an external implementer could
# copy from docs/kaspa-pq-spec.md (sizes / opcode / fn / struct / keygen-context). Generic historical
# "ML-DSA-65" prose (clearly flagged in the spec header + the ADR-0002/0015 historical banners) is allowed.
if grep -nE "3309|1952|OP_CHECKSIG_MLDSA65|OP_BLAKE2B_256|OP_DATA32|calc_mldsa65|Mldsa65SigCacheKey|mldsa65/keygen" docs/kaspa-pq-spec.md; then
  echo "  -> FAIL: docs/kaspa-pq-spec.md contains a stale ML-DSA-65 concrete value (see matches above)."
  fail=1
else
  echo "OK: spec is free of stale ML-DSA-65 sizes/opcodes/fns."
fi

echo "== [5/6] ML-DSA-87 multisig helper must not be exposed outside txscript (audit L-03) =="
# multisig_redeem_script_mldsa87 is #[doc(hidden)] and P2SH is consensus-disabled in PQ-only, so
# surfacing it from wallet / CLI / RPC would let a user lock funds. It may only appear inside
# crypto/txscript (its def + re-export + tests).
if grep -rn "multisig_redeem_script_mldsa87" --include="*.rs" wallet/ cli/ rpc/ kaspa-pq-validator*/ 2>/dev/null; then
  echo "  -> FAIL: the ML-DSA-87 multisig helper is referenced outside crypto/txscript (see above)."
  fail=1
else
  echo "OK: multisig helper not exposed by wallet/CLI/RPC."
fi

echo "== [6/6] EVM spec id must stay pinned at SHANGHAI (EVM audit C1) =="
# EVM_SPEC_ID is load-bearing beyond opcode gating: the F002 SELFDESTRUCT
# residual analysis (pre-EIP-6780) and the class-2/4 skip boundary were audited
# AT SHANGHAI. A bump is a hard fork AND requires re-running the supply +
# skip-class suites and re-deciding the F002 residual policy (see
# kaspa-evm/src/lib.rs). This grep (with the const assert next to the const)
# makes a silent bump impossible.
if grep -q "pub const EVM_SPEC_ID: SpecId = SpecId::SHANGHAI;" kaspa-evm/src/lib.rs; then
  echo "OK: EVM_SPEC_ID pinned at SHANGHAI."
else
  echo "  -> FAIL: EVM_SPEC_ID is no longer SHANGHAI — follow the spec-bump checklist in kaspa-evm/src/lib.rs before changing this gate."
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "PQ CI guard: FAIL"
else
  echo "PQ CI guard: OK"
fi
exit "$fail"
