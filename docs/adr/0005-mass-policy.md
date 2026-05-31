# ADR-0005: Mass / DoS policy for ML-DSA P2PKH transactions

Status: Accepted — **PR-19-S7 (Phase 7) recalibrated `mass_per_sig_op = 10_000`
for ML-DSA-87** (Phase 1 froze the shape; the earlier ML-DSA-65 PoC set 6000)
Date: 2026-05-28 (rev 2026-05-31)
Supersedes: —

## PR-19-S7 (Phase 7) recalibration — ML-DSA-87 (supersedes the ML-DSA-65 result below)

The kaspa-pq signature migrated from ML-DSA-65 to **ML-DSA-87** (ADR-0019).
ML-DSA-87 `verify` is meaningfully slower, so the sigop weight was re-measured
and raised. Same harness (`crypto/txscript/benches/bench.rs`, now over
`ml_dsa_87::verify`), same reference hardware (Apple Silicon arm64), same
"pin against the slowest variant" rule:

| Primitive | Variant | Median |
|---|---|---|
| `secp256k1::schnorr::Signature::verify` | (single impl) | **12.74 µs** |
| `libcrux_ml_dsa::ml_dsa_87::verify`     | default (NEON/AVX2 multiplexed) | **63.88 µs** |
| `libcrux_ml_dsa::ml_dsa_87::portable::verify` | portable, no SIMD | **76.52 µs** |

Ratios:

- ML-DSA-87 default / Schnorr = 63.88 / 12.74 = **5.01×**
- ML-DSA-87 portable / Schnorr = 76.52 / 12.74 = **6.01×** ← slowest

Calibration formula (unchanged):

```
1000 (upstream mass_per_sig_op)  ×  6.01 (slowest ratio)  ×  1.59 (safety)  =  9548  →  10_000
```

→ kaspa-pq `mass_per_sig_op = 10_000` (rounded up to the nearest 1000, matching
the ML-DSA-65 convention; effective safety factor becomes 10000 / (1000 × 6.01)
= **1.66 ≥ 1.5**), locked across all four `*_PARAMS` constants in
[consensus/core/src/config/params.rs](../../consensus/core/src/config/params.rs).
The earlier ML-DSA-65 value of 6000 encoded a 6.0× multiplier (3.78 × 1.59),
which is *below* the bare ML-DSA-87 portable ratio (6.01×) — i.e. ML-DSA-87 with
6000 had **no** safety margin, which is why the bump was required.

Wallet-side consequence: raising the sigop weight shrinks the tx-generator's
per-relay input batches (≈16 → ≈10 inputs on testnet-10), so the
`kaspa-wallet-core` generator tests were re-recalibrated in the same PR.

Pre-mainnet reinforcement (unchanged intent): re-run the bench on the production
low-end reference image and compare its portable median against 76.52 µs; bump
again if it exceeds.

## (historical) Phase 6 calibration result — ML-DSA-65 PoC, superseded by the above

## Phase 6 calibration result

Measured on the reference hardware (Apple Silicon arm64) via
`crypto/txscript/benches/bench.rs`. The bench exposes three variants so
the calibration can pin against the slowest:

| Primitive | Variant | Median |
|---|---|---|
| `secp256k1::schnorr::Signature::verify` | (single impl) | **12.71 µs** |
| `libcrux_ml_dsa::ml_dsa_65::verify`     | default (NEON multiplexed) | **40.75 µs** |
| `libcrux_ml_dsa::ml_dsa_65::portable::verify` | portable, no SIMD | **48.02 µs** |

Ratios:

- ML-DSA default / Schnorr = 40.75 / 12.71 = **3.21×**
- ML-DSA portable / Schnorr = 48.02 / 12.71 = **3.78×** ← slowest

The kaspa-pq policy calibrates against the slowest variant so that
no-SIMD low-end reference platforms remain safely budgeted:

```
1000 (upstream mass_per_sig_op)  ×  3.78 (slowest ratio)  ×  1.59 (safety)  ≈  6000
```

→ kaspa-pq `mass_per_sig_op = 6000`, locked in
[consensus/core/src/config/params.rs](../../consensus/core/src/config/params.rs)
across all four `*_PARAMS` constants. Reinforcement on a true low-end
cloud instance (e.g. a single-vCPU bursty VM with no SIMD enabled in
the OS) before mainnet launch may further tighten — re-run the bench
above on the production reference image and compare its portable
median against 48.02 µs; bump if it exceeds.

## Context

The upstream `Params` defaults (verified in
[consensus/core/src/config/params.rs](../../consensus/core/src/config/params.rs)):

```text
max_signature_script_len: 10_000
max_script_public_key_len: 10_000
mass_per_tx_byte:          1
mass_per_script_pub_key_byte: 10
mass_per_sig_op:           1000
max_block_mass:            500_000
```

These constants were tuned for Schnorr-secp256k1 inputs (~64-byte
signatures, ~32-byte public keys) and ECDSA-secp256k1 inputs (~71-byte
signatures, ~33-byte public keys). The 1000-mass-per-sigop weight was
calibrated against the cost of a secp256k1 verify.

ML-DSA-65 changes both the size and the cost of a "sigop":

- Signature ~3309 bytes (+1 sighash byte).
- Public key 1952 bytes.
- A full signatureScript is ~5267 bytes.
- A full input (outpoint + script + sequence) is ~5319 bytes.
- ML-DSA-65 `verify` is meaningfully more expensive than
  secp256k1 Schnorr `verify`, in the high-tens-of-µs range
  depending on platform and AVX2 availability.

Two failure modes if we ship upstream defaults unchanged:

1. **Byte-mass underweight on inputs.** A block packed with
   ML-DSA P2PKH spends has ~94× more byte mass per input than the
   upstream secp256k1 case. Even at `mass_per_tx_byte = 1`, this
   eats `max_block_mass` quickly — but not quickly enough to bound
   the verify time per block.
2. **Sigop-mass underweight on inputs.** A naive attacker constructs
   a block with as many ML-DSA verifies as fit, then verify time
   blows past the 100ms block budget.

## Decision

Phase 1 freezes the **shape** of the mass policy; the **values** are
measured and frozen in Phase 6.

### Frozen shape

- The existing `mass = byte_mass + script_pub_key_mass + sigop_mass`
  formula is preserved.
- A new `mass_per_sig_op` value is calibrated for ML-DSA-65 verify.
- `MAX_SCRIPT_ELEMENT_SIZE` is widened to `4096`. Keeping the
  upstream `520` is incompatible with a 3310-byte signature push.
- `max_signature_script_len = 10_000` is preserved as a first
  approximation. It admits exactly one ML-DSA-65 P2PKH input with
  slack. Multi-sig signatureScripts are not yet allowed (see
  [ADR-0002](0002-mldsa65-p2pkh.md)).
- `max_script_public_key_len = 10_000` is preserved.

### Calibration formula

```
new_mass_per_sig_op =
    ceil(
        old_mass_per_sig_op
          * median_mldsa65_verify_time
          / median_schnorr_verify_time
          * safety_factor
    )
```

- `old_mass_per_sig_op = 1000`.
- `median_*_verify_time` measured on devnet/simnet on at least two
  reference hardware classes (one "modern desktop", one "low-end
  cloud instance").
- `safety_factor ≥ 1.5`.

### Pre-verify rejection rules

Before any ML-DSA `verify` call, the script engine **must** reject:

- public-key item whose length ≠ 1952,
- signature item whose length ∉ {3309, 3310},
- non-canonical push (the upstream "minimal push" rule applies
  unchanged),
- script-element length > `MAX_SCRIPT_ELEMENT_SIZE`,
- script size > `MAX_SCRIPTS_SIZE`,
- stack depth > `MAX_STACK_SIZE`.

Rejection here is cheap. The cost we are budgeting against is the
ML-DSA `verify` itself; any failure that does not actually exercise
`verify` must not consume sigop-mass.

### SigCache shape

`Mldsa65SigCacheKey` (see `kaspa-pq-spec.md` §7) is mandatory. The
cache must not hold raw 1952-byte / 3309-byte material by value.

## Consequences

### Positive

- The mass formula keeps the same overall structure as upstream, so
  the existing fee-estimation, block-template, and mempool-eviction
  logic continues to work with only the constants changed.
- Pre-verify length and shape checks bound the cost of a
  malformed-tx flood, because they reject without entering ML-DSA.

### Negative

- We cannot ship a number for `mass_per_sig_op` from Phase 1; the
  Phase 6 benchmark gates the value. Until then, the PoC runs with
  upstream `1000` and we accept that the simnet is dramatically
  over-budget on verify cost. This is only acceptable because the
  PoC simnet is not adversarial.
- A single calibrated value will not be optimal across all
  hardware. We pick the lowest common denominator (slowest of
  the reference platforms) so that the mass cap protects honest
  validators on slow boxes.

### Neutral

- The block-template builder will likely produce blocks with fewer
  inputs than the byte budget alone would allow, because the new
  sigop weight will dominate. This is intentional.

## Alternatives considered

1. **Keep `mass_per_sig_op = 1000`.** Rejected: under-budgets ML-DSA
   verify by an order of magnitude.
2. **Use a separate "pq sigop" counter with its own cap, in addition
   to byte and script-pubkey mass.** Rejected for the PoC: adds a
   third axis to the mass formula and a new acceptance condition
   in the validator. Revisit if Phase 6 finds that the byte-budget
   and sigop-budget cannot be reconciled with a single
   `mass_per_sig_op`.
3. **Drop `mass_per_sig_op` and price purely by byte mass.** Rejected:
   verify time and tx size are correlated but not perfectly, and the
   correlation breaks under adversarial inputs (e.g. an input that
   pre-fails the cheap length checks would be over-priced).

## Implementation notes for Phase 6

Bench harness location: `consensus/core/benches/mldsa65_verify.rs`
(to be created in Phase 6). It must:

- Use Criterion or equivalent stable-noise bench harness.
- Report median, p95, p99 for `verify`.
- Run with and without AVX2 / NEON, recording the slower number as
  the calibration baseline.
- Run on at least one "low-end" reference image.

Outputs:

- Phase 6 PR sets `mass_per_sig_op` in
  [consensus/core/src/config/params.rs](../../consensus/core/src/config/params.rs).
- Phase 6 PR re-runs `max_block_mass` headroom math and tightens
  if needed.
- Phase 6 PR updates `kaspa-pq-spec.md` §6 with the locked-in values.

## Acceptance criteria (Phase 6)

1. `mass_per_sig_op` is set from a measured ratio with safety
   factor ≥ 1.5.
2. Mempool admission survives a flood of 10× normal traffic in
   malformed ML-DSA signatures without exceeding O(traffic)
   verify-time.
3. A full-tx ML-DSA verify is exercised exactly once per accepted
   input (no double-verify due to a sigcache miss path).
4. Block-template generation latency stays inside the
   per-block budget at the calibrated `mass_per_sig_op`.

## References

- Upstream [consensus/core/src/config/params.rs](../../consensus/core/src/config/params.rs).
- [ADR-0002 — ML-DSA-65 P2PKH](0002-mldsa65-p2pkh.md).
- `libcrux-ml-dsa` 0.0.9 docs.rs (verify portable/AVX2 split).
