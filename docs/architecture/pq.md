# Post-quantum authorization

MISAKA is **PQ-only**: ML-DSA-87 (FIPS 204) replaces secp256k1/Schnorr, and every non-PQ path is unrepresentable at the consensus,
mempool and wallet layers.

- [kaspa-pq-spec.md](../kaspa-pq-spec.md) — specification
- [kaspa-pq-design-mldsa87.md](../kaspa-pq-design-mldsa87.md) — ML-DSA-87 design
- [kaspa-pq-mldsa87-verification-runbook.md](../kaspa-pq-mldsa87-verification-runbook.md) — verification runbook
- [test-plan-kaspa-pq.md](../test-plan-kaspa-pq.md) — test plan
- [misaka-pq-deposit-strict-design-v0.1.md](../misaka-pq-deposit-strict-design-v0.1.md) — strict deposit design
