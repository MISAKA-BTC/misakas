# ADR-0132 — What a model is actually paid per forward it ran, and why the gap is liveness before it is price

> **Body moved (2026-09-27).** Normative rules → [spec/palw/06](../spec/palw/06-eligibility-and-block-production.md), [spec/palw/10](../spec/palw/10-collateral-and-economics.md); the full text as written → [design/palw/archive/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md](../design/palw/archive/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md); the reasoning is summarised in [design/palw/work.md](../design/palw/work.md).

* Status: Proposed 2026-09-17, with the end-to-end shadow built the same day. **Upgrades C (the
  economic payout) and S (the single lottery, §7.5) and the short challenge window as a fence (§7.6)
  were armed together on testnet-11 at DAA 6,001**, with fingerprint `a8f99dac…`. Armed on testnet-12
  from genesis.
* Date: 2026-09-17

## Context

On testnet-11 a dense-tier claim was paid, in expectation, 0.60 MSK per 10⁹ MAC-equivalents its
producer ran, and a hybrid claim was paid nothing. The price was not the reason. No seat that could
run the hybrid existed, so the gap was liveness before it was price.

## Decision

- **Upgrade C — a claim is paid for the compute it ran, at one rate:** `min(escrow, attempted_ccu ×
  rate)`, with the panel's share derived from verification compute, and the claim's economics
  snapshotted at acceptance. → spec 10 PALW-CO-42, 08 PALW-VF-37.
- **Upgrade S — the single lottery:** the class ticket is the whole lottery. An attempt passes Layer-0
  unconditionally, and its row does not price the window. → spec 06 PALW-EL-8.
- **§7.6 — The short challenge window as a fence** (120 DAA, `palw_short_challenge_window`). → spec 07.

## Consequences

- One forward is one draw, with nothing thrown away (testnet-11's producer had discarded 79 % of its
  forwards). The hypotheses, proposals and the simpler protocol (§3–§5) are in the full text.

## Links

- Spec: [06 Eligibility and block production](../spec/palw/06-eligibility-and-block-production.md) · [10 Collateral and economics](../spec/palw/10-collateral-and-economics.md)
- Design: [design/palw/work.md](../design/palw/work.md)
- Full text as written: [design/palw/archive/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md](../design/palw/archive/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md)
