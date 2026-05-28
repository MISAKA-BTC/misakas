# ADR-0013: Validator Reward Distribution

Status: Accepted (Phase 13 design freeze; implementation deferred to Phase 10 PR series)
Date: 2026-05-28
Supersedes: —
Depends on: [ADR-0002](0002-mldsa65-p2pkh.md) (the address payload
            that receives rewards), [ADR-0008](0008-hash64-consensus-identity.md)
            (Hash64 for owner identification),
            [ADR-0009](0009-dns-probabilistic-finality.md) (DNS
            overlay; this ADR funds the validators who service it
            and pins the equivocation-slashing distribution that
            ADR-0009 left as "reporter reward + burn" without
            quantifying), [ADR-0012](0012-mainnet-validator-sortition-commit-reveal.md)
            (sortition; the per-validator selection probability
            this ADR's APY analysis assumes).

## Context

The DNS Probabilistic Finality Overlay (ADR-0009) creates a new
class of network participant — the PoS validator. ADR-0010 and
ADR-0011 specify how to run one. ADR-0012 specifies how the
network picks which ones get to attest each epoch. None of them
specify **why** an operator would want to do this work in the
first place.

A validator's costs are non-trivial:
- A full node + validator sidecar host (ADR-0011 §"Hardware
  sizing" sets the floor at 8 vCPU / 16 GB / 1 TB NVMe for
  mainnet);
- An ML-DSA-65 hot signing key that is a slashing risk if
  compromised;
- A bonded stake locked for `unbonding_period_blocks ≥ R + E`
  (ADR-0009 §"Long-range bound").

Without a reward, the only motivation to validate is altruistic
network support, which does not scale. This ADR pins the
reward-distribution mechanics so the bond ROI economics close.

The ADR also closes two loose ends inherited from earlier ADRs:
- ADR-0009 §"`SlashingEvidencePayload`" says "reporter reward +
  burn" without quantifying the split;
- ADR-0012 §"Slashing rule" sets `unreveal_reporter_reward_sompi`
  as a parameter but leaves the equivocation reward unspecified.

This ADR consolidates both under one reward / slash distribution
table.

## Decision

### Reward source: inflation only

Validators are paid from **inflation** (new minting), not from
transaction fees. The split:

| Source | Recipient | Mechanism |
|---|---|---|
| Block coinbase, miner share | PoW miner who produced the block | Existing upstream Kaspa coinbase output. **Unchanged** by this ADR. |
| Block coinbase, validator share | Each validator whose attestation was included on-chain in this block | New coinbase outputs added by this ADR (one per included attestation). |
| Transaction fees | PoW miner who produced the block | Existing upstream Kaspa behaviour. **100% to miner.** Validators get none. |

Rationale for the "tx fees stay with miners" choice:
- Miners do the inclusion work (selecting txs from the mempool,
  building the merkle, paying for the PoW); they should keep the
  fee surface they always have kept.
- Validators are paid for attestation work, not block-production
  work; their reward source should be independent of
  block-by-block fee volatility.
- Simpler accounting at the consensus layer — every coinbase
  output is either "miner = base + fees" or "validator =
  per-attestation reward × included-count", and the two streams
  do not mix.

### Per-attestation flat reward

Each `StakeAttestation` that is included on-chain via a
`StakeAttestationShardPayload` tx pays its signing validator
**`per_attestation_reward_sompi`** — a per-network constant
(`RewardParams::per_attestation_reward_sompi`).

The reward is **flat per attestation**, not stake-proportional.
This is the right choice because:
1. ADR-0012 sortition is already stake-weighted (larger stake →
   more frequent committee membership → more attestations →
   more rewards). Adding a stake-proportional per-attestation
   reward on top would double-count and create a
   "rich-get-richer" effect within the eligible committee.
2. Flat-per-attestation gives every staked sompi a **uniform
   expected APY**, independent of validator size. A 1000 sompi
   bond and a 1 000 000 sompi bond earn the same yield per
   bonded sompi in expectation — they just earn it at different
   per-validator absolute amounts.

The reward lands at the address derived from
`StakeBondPayload::owner_pubkey_hash` (the **owner** key,
ADR-0011 §"Key separation policy"). The validator (hot) key
never receives funds; only the owner (cold) key does. Operators
who follow the ADR-0011 key-separation policy get the
"signing-key compromise is recoverable" property for free —
even if the validator key is stolen, the attacker cannot
withdraw any earned rewards because they are paid to the owner
address.

### Coinbase fan-out

A block that includes `N` validator attestations pays:

```text
coinbase_outputs(block) =
    [
        // Miner share — unchanged from upstream Kaspa.
        Output {
            value: miner_block_subsidy_sompi(daa_score) + sum(tx_fees),
            script_public_key: miner_pay_to_address(block_template),
        },
        // Validator shares — one per included attestation,
        // canonically sorted by (shard_index, attestation_index)
        // so coinbase tx serialisation is deterministic.
        for each included attestation a (in canonical order):
            Output {
                value: per_attestation_reward_sompi,
                script_public_key:
                    script_public_key_for_p2pkh_mldsa65(
                        a.bond.owner_pubkey_hash,
                    ),
            },
    ]
```

The coinbase tx structure is unchanged: it stays a single
transaction with a single input (the coinbase input) and `N + 1`
outputs (1 miner + `N` validator). Existing wallet and explorer
code that walks coinbase outputs continues to work; the new
outputs follow the same ML-DSA-65 P2PKH script template from
ADR-0002 and look like any other receive.

Validator outputs are deduplicated **per block** — if a single
validator's `owner_pubkey_hash` has two attestations included in
the same block (rare but possible when shards are aggregated by
a single miner), the consensus rule emits two outputs (one per
attestation) rather than one combined output. This keeps the
coinbase-output-per-attestation invariant strict and lets
explorers cross-reference outputs against included attestations
by index.

### Inflation cap

Per block, the validator-side inflation is bounded by:

```text
max_validator_inflation_per_block =
    per_attestation_reward_sompi × max_attestations_per_block
```

Per epoch, by:

```text
max_validator_inflation_per_epoch =
    max_validator_inflation_per_block × epoch_length_blocks
```

Per year (informative — depends on per-network block rate):

```text
annual_validator_inflation =
    max_validator_inflation_per_block × blocks_per_year
```

The per-network mainnet parameterisation targets
**`5–10% annual validator-inflation rate`** measured against
total active stake. The exact value is chosen at the
`commit_reveal_activation_daa_score` switchover (PR-13.5 ships
the type; PR-10.5 + PR-13.5 follow-on ships the parameter
calibration). Total inflation = miner subsidy + validator
inflation cap; the miner subsidy schedule is unchanged from
upstream Kaspa, so the validator track is a strict addition.

### Slashing distribution (binding)

Both equivocation slashing (ADR-0009 §"`SlashingEvidencePayload`")
and unreveal slashing (ADR-0012 §"Slashing rule:
commit-without-reveal") follow the same distribution rule. The
slashed bond amount `S` (the full bonded amount for equivocation,
or `commit_without_reveal_slash_sompi` for unreveal) is split:

```text
reporter_reward = S × slashing_reporter_reward_bps / 10000
burned          = S − reporter_reward
```

The `slashing_reporter_reward_bps` is a per-network parameter,
expressed in basis points (`10000 = 100%`). Mainnet
recommendation: **`1000 bps = 10%`** — large enough to make
slashing-evidence submission profitable (covers gas + a margin),
small enough that the network does not pay out most of a slashed
bond as a reward.

For the unreveal case, the reporter reward is the **smaller** of:
- The bps-derived value above, and
- The pre-existing `unreveal_reporter_reward_sompi` floor from
  ADR-0012 (`DnsParams::unreveal_reporter_reward_sompi`).

The smaller-of rule keeps the unreveal pipeline cheap for the
reporter (matches gas cost) without scaling the reporter reward
to the full bond when only a small `commit_without_reveal_slash_sompi`
fraction was burned.

The remainder of `S` is **burned** — sent to the all-zero
`script_public_key` (the existing kaspa "burn address" pattern)
or removed from supply via a `consensus/src/processes/slashing.rs`
side-effect that decrements an inflation accumulator. The exact
mechanism is a PR-10.12 implementation detail; this ADR pins the
fact that the remainder leaves the active supply.

The reporter is paid via a fresh consensus-emitted output on the
slashing transaction itself (a one-output coinbase-like
attachment), not via the block coinbase, so slashing-reward
accounting is per-transaction rather than per-block.

### Reward params type surface

Carried as a new `RewardParams` struct alongside
[`DnsParams`](../../consensus/core/src/dns_finality.rs)
(PR-13.5):

```rust
pub struct RewardParams {
    /// Flat per-included-attestation reward.
    pub per_attestation_reward_sompi: u64,

    /// Basis-points fraction of any slashed bond that goes to
    /// the reporter (10000 = 100%). Equivocation and unreveal
    /// slashes both follow this rule, modulo the unreveal
    /// `min` cap.
    pub slashing_reporter_reward_bps: u16,

    /// Hard cap on per-block validator-side coinbase outflow.
    /// Defensive — `per_attestation_reward_sompi ×
    /// max_attestations_per_block` should never exceed this; if
    /// it does, the consensus rule prefers the cap and refunds
    /// the difference (no overflow into the coinbase
    /// accumulator).
    pub max_validator_inflation_per_block_sompi: u64,
}
```

`unreveal_reporter_reward_sompi` stays in
[`DnsParams`](../../consensus/core/src/dns_finality.rs) where
ADR-0012 placed it; the slashing distribution rule above
references it explicitly as a `min` cap on the bps-derived
reward.

### Bond ROI economics (informative)

For a validator with bond `B` out of total active stake `T`:

```text
expected_attestations_per_epoch = committee_size × (B / T)
expected_reward_per_epoch       = per_attestation_reward × committee_size × (B / T)
expected_reward_per_year        = per_attestation_reward × committee_size × (B / T) × epochs_per_year

annual_APY =
    expected_reward_per_year / B
  = per_attestation_reward × committee_size × epochs_per_year / T
```

Two important properties this surfaces:
1. **APY is independent of `B`**. Every staked sompi earns the
   same expected yield regardless of which validator it is bonded
   to. This is the right incentive: operators are not pressured
   to consolidate stake under a single validator.
2. **APY is inversely proportional to `T`**. As total stake
   grows, per-sompi yield falls. This is also the right
   incentive: yield falls when validator participation is high
   (because the network is well-secured), and rises when it is
   low (incentivising new validators to join).

The miner subsidy uses the upstream halving schedule and is
unaffected; this ADR adds a **separate** inflation track for
validators that operates beside the miner subsidy.

### Public-claim discipline (binding)

The kaspa-pq Phase 13 reward-distribution claim, verbatim:

- ✅ "Validators earn per-attestation flat rewards from
  inflation."
- ✅ "Reward APY (per staked sompi) is uniform regardless of
  validator size, in expectation under sortition
  stake-weighting."
- ✅ "Transaction fees stay 100% with PoW miners; validators
  are paid entirely from inflation."
- ✅ "Validator rewards land at the bond owner address (cold
  key), never at the validator signing key (hot key)."
- ✅ "Slashing reporter receives `slashing_reporter_reward_bps`
  / 10000 of the slashed amount; the remainder is burned."
- ❌ "Validators earn from tx fees." **Not claimed.** This is
  an explicit design choice; a follow-up ADR is required to
  change it.
- ❌ "Reward rate is fixed forever." **Not claimed.**
  `per_attestation_reward_sompi` is a per-network parameter and
  is hard-fork-bumpable.
- ❌ "Validator rewards are guaranteed." **Not claimed.** A
  validator who is not sortitioned-in to a given epoch earns no
  reward that epoch; a validator who is sortitioned-in but
  whose attestation does not land on-chain (because of
  shard-inclusion competition or chain reorg) earns no reward
  for that attestation. The APY formula above is an
  **expectation**, not a guarantee.

External material **must** use the phrasings above. The "uniform
APY per sompi" claim is binding under sortition stake-weighting
as specified in [ADR-0012](0012-mainnet-validator-sortition-commit-reveal.md);
any deviation from ADR-0012's stake weighting (e.g. a future
ADR introducing per-validator caps or weighted-bonus structures)
breaks the claim and requires an explicit re-derivation.

## Consequences

### Positive

- **Bond economics close.** Operators can compute their
  expected APY from public network parameters
  (`per_attestation_reward_sompi`, `committee_size`,
  `epochs_per_year`) and the on-chain total stake without
  having to trust off-chain APR aggregators.
- **No tx-fee coupling.** Validator rewards are insulated from
  per-block tx-fee volatility, smoothing operator income.
- **Hot-key compromise is recoverable.** Rewards land at the
  owner (cold) address; even a fully compromised validator
  signing key cannot redirect earned rewards.
- **Slashing economics aligned.** The reporter reward
  guarantees that submitting evidence is profitable (covers gas
  + margin) but not so large that bad actors are incentivised
  to manufacture slashable events on themselves.
- **Closes ADR-0009 and ADR-0012 loose ends.** The
  "reporter reward + burn" wording from ADR-0009 and the
  unspecified equivocation-side of ADR-0012 are now quantified
  here.
- **Single new param struct.** `RewardParams` lives alongside
  `DnsParams`; no other consensus types need new fields.

### Negative

- **Coinbase fan-out grows.** A block including 16 attestations
  has 17 coinbase outputs (1 miner + 16 validator) versus the
  upstream 1. Coinbase tx size grows by `N × 64 B` (the address
  payload width); the consensus rule sets a per-block max via
  `max_validator_inflation_per_block_sompi`'s implicit cap on
  output count, and `max_attestations_per_block` bounds it
  further. No new consensus surface beyond outputs, but a
  noticeable bytes-per-block increase.
- **Two inflation tracks.** Total annual inflation = miner
  subsidy schedule + validator track. The two are independent
  but operators / explorers need to surface them separately so
  the headline inflation number is not surprising.
- **No vesting / lock-up.** Earned rewards are immediately
  spendable. A future ADR may add vesting for security-
  sensitive deployments (e.g. exchange-operated validators
  with regulatory reporting requirements); this ADR keeps the
  baseline simple.

### Neutral

- **Per-network parameterisation.** The exact
  `per_attestation_reward_sompi` value is per-network and
  hard-fork-bumpable, so this ADR can land before the mainnet
  number is calibrated. The shape (flat, per-attestation, owner
  address) is fixed; the magnitude is not.
- **No change to slashing detection.** Equivocation evidence
  (ADR-0009) and unreveal evidence (ADR-0012) are unchanged;
  only the distribution side is pinned.

## Phase 13 PR plan (this ADR's slot)

| PR | Title | Status |
|---|---|---|
| 13.4 | This ADR | landed |
| 13.5 | `dns_finality.rs` `RewardParams` + `compute_attestation_reward_payouts` + `compute_slashing_distribution` helpers + tests | next |
| 13.6 | Spec update (ADR-0013 + Phase 13 row 2/4 + Phase 13 acceptance criteria 2/4 + v0.7) | next |

Implementation slots (gated on Phase 1–9 baseline + PR-10.5 +
PR-10.6, layer onto PR-10.5 and PR-10.12):

| PR | Title | Layers onto | Status |
|---|---|---|---|
| 10.5′ | Coinbase fan-out for validator attestation rewards in `consensus/src/processes/coinbase.rs`; consume `RewardParams::per_attestation_reward_sompi` | PR-10.5 | deferred |
| 10.12′ | Slashing distribution in `consensus/src/processes/slashing.rs` using `compute_slashing_distribution` for both equivocation and unreveal cases | PR-10.12 | deferred |

## References

- [ADR-0002 — ML-DSA-65 P2PKH](0002-mldsa65-p2pkh.md)
  (the address template validator rewards are paid to).
- [ADR-0009 — DNS Probabilistic Finality Overlay](0009-dns-probabilistic-finality.md)
  §"`SlashingEvidencePayload`" (the "reporter reward + burn"
  pointer this ADR quantifies).
- [ADR-0010 — Validator Node Architecture](0010-validator-node-architecture.md)
  (the validator service this ADR rewards).
- [ADR-0011 — Validator Single-Host Deployment + Equivocation-Safety](0011-validator-deployment-and-equivocation-safety.md)
  §"Key separation policy" (the policy this ADR's
  "rewards-to-owner-not-validator" rule depends on).
- [ADR-0012 — Mainnet Validator Sortition](0012-mainnet-validator-sortition-commit-reveal.md)
  §"Slashing rule: commit-without-reveal" (the unreveal-slash
  case this ADR's distribution rule applies to).
