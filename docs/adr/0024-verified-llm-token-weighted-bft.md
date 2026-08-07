# ADR-0024: Verified LLM Token-Weighted BFT for DNS finality

- Status: Accepted (implemented, shipped dormant)
- Date: 2026-08-07
- Source: `MISAKA: 検証済み LLM トークン重み付き BFT — Bonded Validators による Useful Compute Consensus`, Draft v0.1
- Supersedes the *voting-weight* half of ADR-0009 / ADR-0017 / ADR-0018 §B. Everything else in
  those ADRs (bonds, attestation shards, canonical lagged anchors, rewards, the reorg gate)
  stands unchanged.

## Context

DNS finality's validator set has been weighted by **bonded capital** since ADR-0009: a
validator's voting power is its `StakeBondRecord::amount`, and an epoch earns graded StakeScore
credit above the ADR-0018 §B quality floor φS. That is the ordinary PoS mapping — *Money →
Power*.

The v0.1 paper replaces the source of that power with **verified useful compute**: a validator's
weight is the amount of independently-verified LLM inference it recently supplied, with its bond
demoted from *being* the power to *collateralizing* it. Bond stops buying votes and starts
backing them.

## Decision

Replace the weight source and the epoch-credit threshold; keep everything else.

```
x_j     = ρ(S_j)·(a·t_j^in + b·t_j^out)   if Verify(S_j, R_j, C_j) = 1, else 0     (§3.2 eq. 4)
X_i(e)  = Σ_j x_j                          over validator i's certified jobs in epoch e
C_i(E)  = Σ_{τ=1..K} d_τ · X_i(E − τ)      1 = d_1 ≥ d_2 ≥ … ≥ d_K > 0             (§4 eq. 5)
W_i(E)  = min{ C_i(E), λ·B_i(E) }                                                  (§4 eq. 6)
W(E)    = Σ_i W_i(E),   Q(E) = ⌊2W(E)/3⌋ + 1                                       (§4 eq. 7)
```

### What changed

| | Before | After (above the fence) |
|---|---|---|
| Voting weight | `bond.amount` (sompi) | `W_i(E) = min{C_i(E), λ·B_i(E)}` (µRTE) |
| Epoch credit | graded, `(f − φS)/(1 − φS)` | **binary** on `Q(E) = ⌊2W(E)/3⌋ + 1` |
| Bond's role | *is* the voting power | participation floor + slashable collateral cap |
| Slashable offences | equivocation | \+ forged receipt, invalid certificate, contradictory verification, failed challenge |

### What deliberately did **not** change

- **`min_bond_amount_sompi` and `min_active_stake_sompi` stay at 20M KAS on production.** Same
  number, different job: it is now the participation requirement and the `λ·B_i` collateral
  ceiling rather than voting power itself.
- PoW/GHOSTDAG block production and ordering. The paper's §5 round machinery (Proposal /
  Prevote / Lock+Precommit) is *not* adopted; MISAKA's finality layer is the attestation
  overlay, and it is that overlay's weight and quorum that were replaced. Canonical history is
  still decided by GHOSTDAG.
- The mandatory-attestation-inclusion gate and mining prioritization, which are
  stake-denominated block-inclusion policy and stay on `ContributionWeight::BondedStake`.
- Reward distribution (ADR-0013 / ADR-0018 §E), which remains stake-proportional.
- The reorg gate's structure, the canonical lagged anchor, and `required_stake_depth`
  calibration — a quorum epoch still earns exactly `STAKE_SCORE_SCALE`, so
  "`10 × STAKE_SCORE_SCALE` = ten confirmed epochs" still reads true.

### Verification scheme

v0.1 pins `VerificationScheme::CanonicalFullReplay` as the only consensus-eligible relation.
§6 requires the acceptance condition be deterministic in consensus code, and full replay is
deterministic by construction: the JobSpec fixes model weights, runtime, quantization, input,
sampling seed, and token limit, so an honest verifier must reproduce the executor's `R_j`
byte-for-byte. `RandomizedTraceChallenge` and `SuccinctProof` are reserved wire ids.

Acceptance is **refutation-dominant**: one `Refuted` verdict fails the job even if the
confirmation count is met. Under full replay there is no honest reading in which one fixed spec
yields two receipts, so the safe resolution is to mint nothing and let the §7 challenge path
decide who to slash.

## Activation

Everything is fenced behind `VltParams::vlt_activation_daa_score`, `u64::MAX` (dormant) on
**every** shipped preset. Below the fence the overlay is byte-identical to its pre-VLT
behaviour, so adopting this ADR is not by itself a consensus change.

Moving a network's fence is a coordinated hard fork, and it **must not be scheduled before the
active set can actually produce verified compute**: with no VLT every `W_i(E)` is 0, so `W(E)`
is 0, no epoch reaches quorum, and DNS finality stalls (the base ledger keeps advancing — the
overlay is liveness-first). The paper's §2 "計算 bootstrap 期間" is this same caveat.

`DnsParams::vlt_params_consistent()` is the pre-flight check. It verifies the VLT knobs are
coherent, that `unbonding_period_blocks` covers the §7 bound
`U ≥ credit window + max challenge period` (production's 14-day window covers the shipped
calibration with wide margin), and that `vlt_credit_window_blue_score` spans
`(K + delay) × epoch_len + challenge_window + lag + backoff`.

That last one matters more than it looks: the credit walk resolves each certificate's epoch
against a canonical-anchor map built over the *same* window, and an epoch with no anchor is
skipped. A short window therefore does not fail loudly — it silently truncates the oldest epochs
of every validator's `C_i(E)` to zero and makes weight depend on an unrelated parameter. The
requirement is asserted in both directions by
`vlt_fence_keeps_shipped_presets_on_the_legacy_rule`.

## Recommended calibration (`VltParams::INERT`'s non-fence values)

| Knob | Value | Why |
|---|---|---|
| `credit_window_epochs` (K) | 96 | ~16 min at 100-blue_score epochs / 10 bps — long enough that one slow job does not swing weight, short enough that stopped hardware loses power quickly |
| `credit_decay_bps` (d) | 9 700 | 0.97/epoch ⇒ half-life ~23 epochs (~4 min); `d_96 ≈ 0.054`, so truncating at K discards little |
| `credit_delay_epochs` | 1 | the paper's **minimum**; this is what stops fork-minted VLT weighting its own fork (§8.3) |
| `lambda_vlt_per_kas` (λ) | 1e8 µRTE/KAS | at the unchanged 20M-KAS floor this collateralizes 2e9 RTE — the cap binds concentration without throttling ordinary participation |
| `prefill_cost_micro` (a) | 1.0 | one prefill token = one reference-token-equivalent |
| `decode_cost_micro` (b) | 8.0 | decode is memory-bandwidth-bound and dominates real serving cost per token |
| `challenge_window_blocks` | 300 | matches `max_reorg_horizon_blocks`, so a certificate is challengeable at least as long as its block is reorgable |
| `verifier_committee_size` / `min_verifier_confirmations` | 3 / 2 | majority of an independently-sortitioned committee |
| `model_cost_table` | **empty** | no registered model ⇒ every job mints zero; a fence moved by accident cannot silently start crediting |

`ρ(S_j)` is a consensus parameter, never an executor input (§3.2). A job naming an unregistered
`(model_weights_hash, runtime_hash)` mints zero VLT, so nobody can invent a fictitious expensive
model. Populating `model_cost_table` is a governance action and part of any activation plan.

## Implementation

| Concern | Where |
|---|---|
| Types, normalization, decay, weight, quorum, sortition | `consensus/core/src/vlt.rs` |
| `W_i(E)` per bond, `W(E)` denominator, credit rule, credit folding | `consensus/core/src/dns_finality.rs` |
| Per-network params + fence | `consensus/core/src/config/params.rs` |
| Credit walk, weight-source switch, per-branch scoring | `consensus/src/pipeline/virtual_processor/processor.rs` |
| Stateless payload validation | `consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs` |
| Subnetwork ids `0x14` / `0x15` | `consensus/core/src/subnets.rs` |

The switch itself is one call — `DnsParams::epoch_credit_rule(daa_score)` — plus the matching
`ContributionWeight` selector, which is an explicit argument so every call site declares whether
it wants finality weight or stake-denominated inclusion policy.

`validator_voting_weight` is shared by the quorum's numerator and its denominator on purpose:
computing them separately would be a latent consensus split.

## Consequences

**Positive**

- Voting power tracks supplied, independently-verified compute, and decays when it stops.
- Buying stake buys no votes: `C_i = 0 ⇒ W_i = 0` regardless of bond.
- Slashing keeps its economic meaning: compute beyond `λ·B_i` is discarded, so weight is always
  backed by collateral that can be burned.
- The BFT quorum is exact and strictly above two thirds, restoring the §8.1
  quorum-intersection argument that a graded credit could not support.
- Existing networks are untouched until a fence moves.

**Negative / open**

- **Walk cost.** The credit walk scans `vlt_credit_window_blue_score` (10 400 on production,
  vs. 1 500 for the attestation walk) per recompute. It is skipped entirely while the fence is
  inert. A persisted per-epoch credit accumulator is the obvious optimization if activation is
  scheduled.
- **Sortition beacon (known limitation).** §6 wants verifiers drawn from randomness the executor
  could not see when it committed. The beacon used is the epoch's canonical lagged anchor:
  chain-derived, identical on every node, not chooseable by the executor — but observable before
  the executor picks a job, so `sampling_seed` grinding for a friendlier committee is not fully
  closed. Fixing it needs a two-phase commit → sortition → verify flow where the beacon comes
  from a block strictly after the executor's commitment. This does not weaken the
  executor≠verifier separation or the bonded-verifier requirement, both of which are enforced.
- **Node-side execution is out of scope.** This ADR covers the consensus surface. Running the
  model as an executor, re-running it as a verifier, and publishing certificates is node
  software that consumes `LlmJobSpec` / `ComputeReceipt`; consensus only ever checks
  commitments, never tensors.
- **Model-table governance.** `ρ` and the registered model set are now consensus security
  parameters (§8.4). They need a governance process before activation.
- Long-range and concentration caveats from §8.4 are unchanged by this work: key rotation and
  the unbonding horizon still bound long-range history, and an identity cap alone is not a
  Sybil defence.
