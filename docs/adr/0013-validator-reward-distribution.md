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

The reward lands at the owner's declared ML-DSA-65 P2PKH spend
payload (the **owner** key, ADR-0011 §"Key separation policy").
> **Amended by Addendum B:** the recipient is
> `StakeBondPayload::owner_reward_spk_payload` (32-byte
> `BLAKE2b-256(owner_public_key)`), **not** the 64-byte
> `owner_pubkey_hash` identity hash this paragraph originally
> named. See Addendum B for the rationale.
The validator (hot) key
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
                    // Amended by Addendum B — pay to the 32-byte
                    // declared spend payload, not the 64-byte
                    // identity hash:
                    p2pkh_mldsa65_spk(
                        a.bond.owner_reward_spk_payload,
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

## Addendum B — Reward-recipient address resolution (binding)

Status: Accepted
Date: 2026-05-29
Amends: this ADR's §"Per-attestation flat reward" and
        §"Coinbase fan-out".

### The gap

The §"Coinbase fan-out" pseudo-code originally paid each
validator reward to

```text
script_public_key_for_p2pkh_mldsa65(a.bond.owner_pubkey_hash)
```

This does not type-check against the rest of the kaspa-pq stack
and cannot be implemented as written:

- `StakeBondPayload::owner_pubkey_hash` is a **64-byte**
  `Hash64` = `BLAKE2b-512(owner_public_key)`, the ADR-0008
  consensus *identity* hash.
- A spendable ML-DSA-65 P2PKH output (ADR-0002) commits to a
  **32-byte** payload = `BLAKE2b-256(owner_public_key)`; the
  script is
  `[OpDup, OpBlake2b, OpData32, <32-byte payload>, OpEqualVerify, OpCheckSigMlDsa65]`
  and `OpBlake2b` (0xaa) re-derives a **32-byte** digest from the
  pushed key at spend time, then `OpEqualVerify` compares it to
  the committed 32 bytes.

A 64-byte BLAKE2b-512 identity hash is **not** the 32-byte
BLAKE2b-256 the spend script will recompute, and the 64→32
reduction is not derivable (you cannot truncate one BLAKE2b
digest into another and keep it spendable). Worse, the bond
record as it stood stored **neither** the owner public key
**nor** any 32-byte spend payload — so there was no on-chain data
from which a payable script could be built at coinbase-assembly
time.

### Decision: declare the spend payload in the bond

Add one field to the bond wire format and its derived record:

```rust
// StakeBondPayload  (consensus/core/src/dns_finality.rs)
// StakeBondRecord   (same module)
//
// The owner's *declared* ML-DSA-65 P2PKH spend payload:
//   owner_reward_spk_payload == BLAKE2b-256(owner_public_key)   (ADR-0002)
// i.e. the 32-byte `Address { version: PubKeyHashMlDsa65 }`
// payload of the cold owner key that earned rewards are paid to.
pub owner_reward_spk_payload: [u8; 32],
```

- `owner_pubkey_hash` (64-byte `Hash64`) is **unchanged** and
  keeps its sole job: consensus *identity* (bond uniqueness,
  owner-key matching, equivocation/dedup). It is **not** a
  payable target.
- `owner_reward_spk_payload` (32-byte) is the **only** field
  rewards are paid to. It is supplied by the bond creator and
  copied verbatim by
  [`stake_bond_record_from_payload`](../../consensus/core/src/dns_finality.rs).
- Both derive from the same owner public key
  (`BLAKE2b-512` → identity, `BLAKE2b-256` → spend payload), so
  an honest bond creator computes both from one cold key. The
  bond does **not** store the raw owner public key — only an
  attestation/spend would reveal it — keeping the bond compact.

The §"Coinbase fan-out" recipient line is amended to:

```text
script_public_key:
    p2pkh_mldsa65_spk(a.bond.owner_reward_spk_payload)   // 32-byte payload
```

### Canonical reward script (binding byte layout)

`p2pkh_mldsa65_spk(payload32)` produces a `ScriptPublicKey` with
`version = 0` (`MAX_SCRIPT_PUBLIC_KEY_VERSION`) and the 37-byte
script

```text
0x76 (OpDup) ‖ 0xaa (OpBlake2b) ‖ 0x20 (OpData32)
            ‖ payload32 (32 bytes) ‖ 0x88 (OpEqualVerify)
            ‖ 0xa6 (OpCheckSigMlDsa65)
```

This is byte-identical to
`kaspa_txscript::pay_to_address_script(&Address::new(prefix, Version::PubKeyHashMlDsa65, &payload32))`
— the `ScriptPublicKey` bytes are **prefix-independent**, so the
coinbase construction and validation paths need not agree on a
network prefix, only on the 32-byte payload. `consensus`
(the crate holding `processes/coinbase.rs`) depends on full
`kaspa-txscript` and uses `pay_to_address_script`;
`consensus-core` (which only depends on `kaspa-txscript-errors`)
builds the same bytes from the opcode literals above for its
unit tests. The two **must** stay byte-equal — a parity test
pins it.

### Security analysis

A bond creator who declares a wrong `owner_reward_spk_payload`
only misdirects **their own** future rewards (to a script they
may not control, i.e. self-griefing). They cannot:

- redirect any **other** validator's rewards (each attestation's
  reward is keyed to *its own* bond's payload);
- create or inflate value (the per-attestation amount and
  per-block cap from this ADR are unchanged);
- affect consensus safety, sortition, or slashing (those key on
  `owner_pubkey_hash` / `validator_id`, not on the spend
  payload).

Because the only party harmed by a malformed payload is the
declarer, consensus bond-acceptance imposes **no** check on the
payload beyond its fixed 32-byte width (guaranteed by the
`[u8; 32]` type). No proof that the payload matches
`owner_pubkey_hash` is required or possible at bond time (the raw
owner key is not on-chain). Wallets SHOULD derive both values
from the same cold key and MAY warn if a user supplies them
independently.

### Determinism, dedup, and the cap (unchanged)

- Coinbase outputs remain **one per included attestation**, in
  the §"Coinbase fan-out" canonical order
  (`(shard_index, attestation_index)`). Two attestations from the
  same owner in one block still emit two outputs — dedup is
  per-attestation, never combined-by-owner, so introducing a
  spend payload changes nothing here.
- The per-block inflation cap
  (`max_validator_inflation_per_block_sompi`) and
  [`compute_attestation_reward_payouts`](../../consensus/core/src/dns_finality.rs)
  are arithmetic-only and unaffected; they bound the *total*
  validator outflow, and the new field only decides *where* each
  already-bounded output is sent.

### Wire-format compatibility

This widens `StakeBondPayload`. No `StakeBond` transaction exists
on any live kaspa-pq network (the overlay is dormant/gated behind
an unset activation height on every net), so this is a
**pre-activation wire change**, not a migration: there is no
deployed serialised bond to upgrade. The field is appended after
`unbonding_period_blocks` as the struct's last member so the
borsh layout change is localized. Any future post-activation
change to this field would require a versioned payload bump
(`StakeBondPayload::version`).

### Implementation slots (supersedes the PR-10.5′ row above)

| Sub-PR | Title | Gated? |
|---|---|---|
| 10.5′-a | Add `owner_reward_spk_payload` to `StakeBondPayload` + `StakeBondRecord` + `stake_bond_record_from_payload`; add the pure `p2pkh_mldsa65_spk` + reward-outputs helper in `dns_finality.rs`; parity test vs `pay_to_address_script`. Inert — no caller on any path. | n/a (dormant type + pure helper) |
| 10.5′-b1 | Plumb `RewardParams` into `DnsParams` (gated). Done. | n/a (dormant param) |
| 10.5′-b2/b3 | Wire the reward outputs into `CoinbaseManager::expected_coinbase_transaction` (construction **and** validation, byte-for-byte). **Depends on the per-block active-bond view** specified in [ADR-0009 Addendum B](0009-dns-probabilistic-finality.md#addendum-b--per-block-active-bond-view--reward-eligibility-binding) — the coinbase is validated per-block, so bond resolution must be a deterministic function of the block's own view, not the virtual-commit-time global `StakeBonds` store. Behind the overlay activation gate; no behaviour change on any current network. | yes (activation height) |

## Addendum C — Slashing reporter-reward recipient + distribution mechanism (binding)

Status: Accepted
Date: 2026-05-29
Amends: this ADR's §"Slashing distribution" (which left the
        reporter recipient and the emit/burn mechanism unspecified).
Depends on: Addendum B (the `p2pkh_mldsa65_spk` reward-script
        helper this reuses), [ADR-0009 Addendum B](0009-dns-probabilistic-finality.md#addendum-b--per-block-active-bond-view--reward-eligibility-binding)
        (the per-block active-bond view that supplies the slashed
        amount `S` deterministically), ADR-0009 §"`SlashingEvidencePayload`"
        + [ADR-0012](0012-mainnet-validator-sortition-commit-reveal.md)
        §"Slashing rule" (the two evidence kinds).

### The gap

§"Slashing distribution" pins the split (`reporter_reward = S ×
bps / 10000`, remainder burned) and says the reporter is paid via
"a fresh consensus-emitted output on the slashing transaction
itself," but **neither evidence payload carries a reporter
recipient**:

```rust
SlashingEvidencePayload         { version, bond_outpoint, attestation_a, attestation_b }
UnrevealSlashingEvidencePayload { version, target_epoch, validator_id, commit_outpoint }
```

This is the same class of gap Addendum B closed for validator
rewards: consensus cannot pay a reporter it has no address for.

### Decision: declare the reporter spend payload in the evidence

Add one field to each evidence payload:

```rust
// SlashingEvidencePayload + UnrevealSlashingEvidencePayload
//   reporter_reward_spk_payload == BLAKE2b-256(reporter_public_key)  (ADR-0002)
// i.e. the 32-byte P2PKH-ML-DSA spend payload of the cold key the
// reporter wants the reward paid to. Appended last (pre-activation
// wire change — no live evidence tx exists).
pub reporter_reward_spk_payload: [u8; 32],
```

The reporter-reward output's `script_public_key` is
`p2pkh_mldsa65_spk(reporter_reward_spk_payload)` (Addendum B's
canonical 37-byte ML-DSA-65 P2PKH script). A malformed payload
only misdirects the reporter's **own** reward (self-griefing), so
consensus imposes no check beyond the fixed 32-byte width — exactly
as in Addendum B.

### The distribution is a consensus side-effect (not a script spend)

The slashed stake is the bond's locking output
(`bond_outpoint = (stake_bond_tx, 0)`, value `S = bond.amount`, ADR-0009
Addendum A.1). It **cannot** be redistributed by a normal UTXO spend
whose script authorises slashing, because `txscript` cannot verify
equivocation evidence (compare two attestation anchors **and** verify
two ML-DSA-65 signatures over derived digests inside a script). The
genuineness is therefore established by the **consensus rule** (the
PR-10.12-a stateful check) and the redistribution is a **consensus
side-effect** of accepting a genuine evidence on the selected chain —
exactly mirroring how the block coinbase mints validator rewards
rather than spending an input:

On accepting a genuine `SlashingEvidence` whose target bond resolves
to `Active` (or `Unbonding`) in the block's selected-parent
[active-bond view](0009-dns-probabilistic-finality.md#addendum-b--per-block-active-bond-view--reward-eligibility-binding):

1. **`S` is fixed deterministically** = the bond's `amount` from that
   view (equivocation) or `commit_without_reveal_slash_sompi`
   (unreveal). Same as-of-block determinism property as Addendum B's
   reward — construction and validation read the identical `S`.
2. **The bond's output-0 UTXO is removed** from the UTXO set (the
   owner can no longer reclaim the locked stake), as a reorg-safe
   side-effect tied to the `Active/Unbonding → Slashed` mutation that
   §A.4 already stages (and reverts).
3. **A single consensus-emitted reporter-reward output is required on
   the slashing transaction**: `value = compute_slashing_distribution(
   S, slashing_reporter_reward_bps).reporter_reward_sompi`, `script_public_key
   = p2pkh_mldsa65_spk(reporter_reward_spk_payload)`. Consensus
   validates its presence, value, and spk (construction == validation,
   byte-for-byte, like the coinbase fan-out). For the **unreveal**
   case the reward is first passed through
   [`apply_unreveal_reporter_min_cap`](../../consensus/core/src/dns_finality.rs)
   with the `DnsParams::unreveal_reporter_reward_sompi` floor (ADR-0012).
4. **The remainder burns implicitly**: `S` left the supply in step 2
   and only `reporter_reward_sompi` was re-minted in step 3, so
   `burned_sompi = S − reporter_reward_sompi` leaves the active supply
   with no explicit burn output — satisfying §"Slashing distribution"'s
   "the remainder leaves the active supply."

Net supply change = `reporter_reward_sompi − S = −burned_sompi`.

### Determinism, reorg, and gating

- `S` and the reporter output are pure functions of the block's own
  selected-parent bond view + the evidence payload, so the slashing
  transaction is reproducible by construction and validation byte-for-
  byte (same discipline as Addendum B's coinbase fan-out).
- The bond-UTXO removal and the `Slashed` mutation are staged/reverted
  together in `stage_dns_bond_mutations` (§A.4), so a reorg that drops
  the slashing block restores both the bond and its locking UTXO.
- Inert unless the overlay is configured **and** the block is past
  `dns_activation_daa_score` (`u64::MAX` on every current network), so
  no evidence is ever processed for distribution today.

### Public-claim discipline (unchanged)

The §"Slashing distribution" claims stand; Addendum C only pins the
recipient + the emit/burn mechanism. The reporter reward still equals
`slashing_reporter_reward_bps / 10000` of the slashed amount (modulo
the unreveal `min`-cap), and the remainder still leaves supply.

## Addendum C.1 — Reporter-output mint exemption (binding)

Status: Accepted
Date: 2026-05-30
Amends: Addendum C step 3, which pinned the reporter output's `value`
        and `script_public_key` but **not** how the shared
        per-transaction validator — also used by the mempool — exempts
        that minted output from the `outputs ≤ inputs` rule, nor the
        fee / `R == 0` mechanics.

Addendum C step 3's reporter output is **minted**: its value `R` comes
from the slashed stake `S`, not from the slashing transaction's inputs.
The transaction *cannot* fund `R` by spending the bond's output-0 —
ADR-0016 §D.2 forbids spending a non-releasable bond, and the reporter
does not hold the owner's key. So `check_transaction_output_values`
(`total_in < total_out ⇒ SpendTooHigh`) would reject the slashing
transaction, and `fee = total_in − total_out` would underflow.
Consensus resolves this as follows.

### C.1.1 The reporter output is the slashing transaction's output-0

A genuine slashing transaction carries its consensus-mandated reporter
output at **output index 0** — mirroring ADR-0009 A.1, where a
`StakeBond` transaction's output-0 is the bond. The remaining outputs
(change, etc.) are the reporter's own, funded normally by the
transaction's inputs.

### C.1.2 Per-transaction exemption (shared validator)

In `validate_populated_transaction_and_get_fee`, for a transaction on
`SUBNETWORK_ID_SLASHING_EVIDENCE`, **only when** the overlay is active
(configured **and** `pov_daa_score ≥ dns_activation_daa_score`) **and**
the transaction mints (`total_out > total_in`):

- output-0 is treated as the minted reporter output and **excluded**
  from value conservation and the fee. The rest of the outputs must
  satisfy `Σ outputs[1..] ≤ total_in`, and
  `fee = total_in − Σ outputs[1..]` — an ordinary mass-based fee paid
  to the miner.

A slashing transaction that does **not** mint (`total_out ≤ total_in` —
e.g. `slashing_reporter_reward_bps == 0`, so `R == 0` and there is no
reporter output) is validated **normally** (no exemption, ordinary
`fee = total_in − total_out`). Non-slashing transactions, and every
transaction on every current network (overlay dormant), keep the strict
`outputs ≤ inputs` rule unchanged — so this is inert today and nothing
may mint outside this exemption.

Because the non-reporter outputs must still cover a normal fee, a
slashing transaction is as costly to relay as any other, so the
exemption gives the mempool **no free-minting spam vector** (a forged
evidence is rejected by the genuineness rule at block validation, but
must pay its way into the mempool regardless).

### C.1.3 Block-validity rule (the exact mint constraint)

The per-transaction exemption is deliberately permissive — it cannot
see the bond view. The **exact** reporter amount is enforced as a
**block-validity** rule in `verify_expected_utxo_state`, resolved
against the block's selected-parent active-bond view (Addendum B), the
same as-of-block determinism the coinbase fan-out uses. For each
genuine evidence whose bond resolves to `Active`/`Unbonding`
(`resolve_slashing_side_effects`):

- if `R > 0`: the slashing transaction's **output-0** must equal the
  expected reporter output **exactly** (`value == R`,
  `script_public_key == p2pkh_mldsa65_spk(reporter_reward_spk_payload)`);
- if `R == 0`: the transaction must carry **no** minted reporter output
  (and must not mint).

So a block is invalid unless every genuine slashing transaction mints
**exactly** its reporter reward at output-0 — construction and
validation byte-for-byte (Addendum C's determinism).

### C.1.4 Implementation sub-slices (refines the 10.12-b2 row below)

| Sub-PR | Title | Gated? |
|---|---|---|
| 16.4-a / -b1 | Pure aggregator + per-block resolver in `dns_finality.rs` (`slashing_side_effects_from_evidence`, `resolve_slashing_side_effects`). Done, inert. | n/a (pure) |
| 16.4-b2-i | Thread the selected-parent `ActiveBondView` into `calculate_utxo_state`. Done, inert plumbing. | n/a (no consumer) |
| 16.4-b2-ii | The §C.1.2 per-transaction mint exemption (output-0, only when minting, overlay-active). | yes (activation) |
| 16.4-b2-iii | The §C.1.3 block-validity reporter-output rule. | yes (activation) |
| 16.4-b2-iv | The §C step-2 bond output-0 UTXO removal side-effect in `calculate_utxo_state` (construction == validation, reorg-safe). | yes (activation) |

Equivocation only; the unreveal case fixes `S =
commit_without_reveal_slash_sompi` and is tied to the sortition
machinery (ADR-0012).

### C.1.5 Strict inclusion discipline (binding — sharpens C.1.3)

C.1.2 makes the per-transaction validator *permissive*: any
`SUBNETWORK_ID_SLASHING_EVIDENCE` transaction may mint at output-0 once
the overlay is active. C.1.3 pins what an **effective** slash must mint.
The gap between them — a genuine-but-ineffective slashing transaction
that nonetheless mints — is the supply-safety boundary, because a mint
with no matching stake removal inflates supply. C.1.5 closes it by
making **inclusion** itself strict, mirroring the §B.4 attestation
reward-eligibility rule ("included ⇒ effective, else the block is
invalid").

A `SUBNETWORK_ID_SLASHING_EVIDENCE` transaction is **effective** in a
block iff **all** hold:

1. it is **genuine** — its bond resolves in the block's selected-parent
   bond view and both equivocating attestations ML-DSA-verify (the
   existing `slashing_evidence_genuine` rule, run first);
2. its bond resolves to **`Active` or `Unbonding`** (per
   `effective_bond_status` at the block's `daa_score`) — the only
   states still holding a removable locked output-0; and
3. it is the **first** slashing transaction, in canonical block order,
   targeting that `bond_outpoint` — **no two slashing transactions in
   one block may target the same bond** (a bond is slashed at most once
   per block; the would-be second removal/mint is rejected, not
   silently collapsed).

**A block that contains any non-effective slashing transaction is
invalid** (a new `RuleError`). For each effective transaction, C.1.3
applies unchanged: its output-0 must equal the expected reporter output
exactly (`value == R`, `script_public_key ==
p2pkh_mldsa65_spk(payload)`); `R == 0` ⇒ no minted output.

This makes "included ⇒ effective ∧ mints exactly `R` at output-0" a
consensus invariant, so the C step-2 removal (one per bond, C.1.4
row b2-iv) maps **1:1** to a minted reporter — no inflation, and the
block rule needs **no transaction input amounts** (it never has to ask
"did this transaction mint?", because an included slashing transaction
that is not an exact-`R` effective slash is simply rejected). An honest
block template therefore drops stale, duplicate, or
non-`Active`/`Unbonding`-bond slashing transactions before assembly
(a pre-filter mirroring `retain_reward_eligible_attestation_shards`),
so honest templates always yield valid blocks.

### Implementation slots (PR-10.12)

| Sub-PR | Title | Gated? |
|---|---|---|
| 10.12-a | Stateful slashing-evidence **genuineness** rule (forged evidence cannot slash). Done. | yes (activation) |
| 10.12-b1 | Add `reporter_reward_spk_payload` to both evidence payloads + a pure `slashing_distribution_output(S, bps, payload, unreveal_floor) -> (reporter TxOutput, burned)` helper in `dns_finality.rs`. Inert. | n/a (dormant field + pure helper) |
| 10.12-b2 | Wire the consensus side-effect: bond-UTXO removal + required reporter-reward output validation in the virtual processor (construction **and** validation, byte-for-byte), behind the activation gate. | yes (activation) |

## Addendum C.2 — Reporter reward as an atomic side-effect (binding)

Status: Accepted
Date: 2026-05-30
Supersedes: Addendum C step 3 ("a reporter-reward output **on the
        slashing transaction**"), and C.1 in full (C.1.1 output-0
        placement, C.1.2 per-transaction mint exemption, C.1.3 + C.1.5
        the exact-output / strict-inclusion block rules). The Addendum C
        step-2 bond output-0 removal, the §"Slashing distribution" split,
        the recipient payload (Addendum C decision), and the net-supply
        identity are all **unchanged**.

### The gap C.1 left open (why this supersedes "reporter on the tx")

C.1 pays the reporter by a **minted output on the slashing transaction**
(`output-0`). That output is created by ordinary transaction application
in `calculate_utxo_state` — which applies the txs of **every** mergeset
block (the selected parent **and** every merged blue block), not just the
block's own txs. The bond output-0 **removal** (Addendum C step 2,
`resolve_slashing_side_effects`), by contrast, is single-shot per bond:
it dedups within a block and skips a bond that is already `Slashed` in
the selected-parent bond view (the view **retains** slashed records).

These two facts do not compose across a GHOSTDAG merge. A single
equivocation can be reported by **two different** slashing transactions
on parallel branches; each is genuine (same bond, same valid
attestations) and each is effective when validated **standalone**, so
C.1.5's per-block strict-inclusion rule (which sees only a block's *own*
txs) admits both. When a later block merges both — or one is on the
selected chain and the other arrives in a merged blue block — both
mint `R` (one per accepted tx), but the bond's `S` is removed **once**
(the second resolves against an already-`Slashed` view and is skipped):

```
net supply change  =  k·R − S    (k = number of accepted, genuine
                                   slashing txs for the one bond merged
                                   into the chain; C.1 intended k = 1)
```

With `k ≥ ⌈10000 / slashing_reporter_reward_bps⌉` merged duplicate
reports (= 10 at the default 10%), `k·R ≥ S` and the slash becomes
**net-inflationary**; for any `k ≥ 2` it over-pays reporters and leaks
supply. The mempool cannot prevent it (each tx is independently valid)
and no per-block rule can (the duplicates live in *different* blocks).
This is **inert on every current network** (`dns_activation_daa_score =
u64::MAX`), but it is a pre-activation supply-safety defect.

### Decision: the reporter reward is a side-effect, not a transaction output

The reporter reward is **removed from the slashing transaction entirely**
and minted by consensus as a side-effect UTXO, in the **same**
`resolve_slashing_side_effects` pass that removes the bond's output-0 —
so the mint and the removal are one atomic, per-bond operation:

1. **A slashing transaction declares no outputs.** A
   `SUBNETWORK_ID_SLASHING_EVIDENCE` transaction is a pure evidence
   carrier: `outputs` is **empty**. (It also has no inputs, as today, so
   it is value-trivial — `Σ in = Σ out = 0`, `fee = 0` — and the C.1.2
   per-transaction mint exemption is **deleted**: nothing on a slashing
   transaction ever mints, so the strict `Σ outputs ≤ Σ inputs` rule
   applies to it unchanged, like every other transaction.) Leaving
   `output index 0` undeclared frees it for the side-effect mint below.
2. **For each bond actually slashed** (one per bond, in canonical
   mergeset order — the *first* effective slashing tx for a bond wins;
   later duplicates resolve to nothing), consensus, atomically:
   - **removes** the bond's locked output-0 UTXO (value `S`) from the
     UTXO set — Addendum C step 2, unchanged; and
   - **mints** the reporter-reward UTXO at outpoint
     **`(slashing_tx_id, 0)`** — `value =
     slashing_distribution_output(S, bps, payload, floor).reporter`,
     `script_public_key = p2pkh_mldsa65_spk(reporter_reward_spk_payload)`,
     `is_coinbase = false`, `block_daa_score = pov_daa_score`
     (immediately spendable; reorged in lockstep with the slash via the
     mergeset-diff machinery). If `R == 0` (micro-bond), **no** UTXO is
     minted and the whole `S` burns.

Because the mint is emitted **iff** the removal is — both keyed on the
same deduped, `Active`/`Unbonding`-gated, per-bond resolution — the net
supply change is **exactly `R − S` per slashed bond, for any `k`**:

```
net supply change  =  R − S    (independent of how many duplicate
                                 slashing txs are merged; k duplicates ⇒
                                 1 mint + 1 removal, k−1 inert)
```

The cross-merge gap is closed **structurally**, not by an auxiliary
rule: there is no longer a free-floating mint to over-count.

### What this resolution requires of `calculate_utxo_state`

The removal+mint resolves over the block's **mergeset** accepted slashing
txs (via `ctx.mergeset_acceptance_data`), against the **selected-parent**
bond view at `pov_daa_score` — the same view and as-of-block determinism
the §A.4 `Slash` mutation already uses — and is applied to
`ctx.mergeset_diff` + `ctx.multiset_hash` (hence the `utxo_commitment`)
**after** the mergeset transaction loop. This is the single shared
construction == validation chokepoint, so a template and a validator
produce byte-identical side-effects. The per-bond dedup that makes the
mint single-shot is the **mergeset-level** analogue of C.1.5 — it
subsumes C.1.5's per-block uniqueness, which is therefore removed.

`resolve_slashing_side_effects` / `SlashingSideEffect` carry the winning
`slashing_tx_id` so the mint outpoint `(slashing_tx_id, 0)` is fixed
deterministically.

### Block-validity rule (replaces C.1.3 / C.1.5)

The only validity rule a slashing transaction must satisfy is: **it
declares no outputs** (so `(slashing_tx_id, 0)` is free for the
side-effect mint and the transaction mints nothing on-chain). A
`SUBNETWORK_ID_SLASHING_EVIDENCE` transaction with a non-empty `outputs`
is invalid.

**This rule is enforced in the stateless transaction-isolation validator
(body processing), not as a stateful rule in `verify_expected_utxo_state`
— and the distinction is load-bearing.** "No outputs" is purely
structural (it needs neither the bond view nor the DAA score), and it
**must hold for every block that can ever be merged**, because the
side-effect (next section) resolves over a chain block's **mergeset**
(its selected parent *and* every merged blue block) while
`verify_expected_utxo_state` runs only for **chain blocks**, over their
**own** transactions. If "no outputs" lived there, a slashing transaction
carrying a **zero-value** `output[0]` in a *non-chain merged-blue* block
would pass body validation, satisfy the (reverted, b2-ii) `Σ out ≤ Σ in`
rule (`0 ≤ 0`), be accepted when merged — creating a UTXO at
`(slashing_tx_id, 0)` — and then collide with the side-effect mint at the
same outpoint (a `DoubleAddCall`), crashing consensus. Enforcing it in
isolation rejects such a transaction in **every** block (chain or
merged-blue) before it can enter the DAG, so the mint outpoint is always
free. This mirrors the ADR-0016 §D.1 StakeBond output rule, which
likewise validates a DNS-overlay transaction's outputs structurally in
the isolation validator. Implementation: a new `DnsTxError` variant
surfaced as `TxRuleError::InvalidDnsOverlayPayload`, checked after the
payload decode in the `SlashingEvidence` arm.

Genuineness is still enforced separately (`slashing_evidence_genuine`).
An **ineffective-but-genuine** slashing transaction (bond unknown,
already-`Slashed`, `Pending`, or a within-mergeset duplicate) is **no
longer block-invalidating** — it simply resolves to no side-effect (mints
nothing, removes nothing), so it is inert and harmless to supply. The
strict-inclusion rejection (C.1.5) is dropped: with the mint gone from
the transaction, an ineffective slash can no longer inflate, so a merging
miner is never forced to exclude a redundant slash to keep a block valid
(no liveness wrinkle).

### Known limitation: genuineness/`Slash` resolve over the mergeset, but their rules run on own-txs (pre-existing, inert)

The side-effect, like the §A.4 `Slash` bond-store mutation it accompanies,
resolves over the chain block's **mergeset**. But the genuineness rule
(`slashing_evidence_genuine`) — and the other overlay block-validity rules
(reward-eligibility, the §D.2 spend-gate) — run in
`verify_expected_utxo_state` over a chain block's **own** transactions
only. So a slashing transaction in a *non-chain merged-blue* block is
**not** genuineness-checked, yet (if its bond resolves `Active`/
`Unbonding`) drives both the §A.4 `Slash` mutation and this side-effect
when merged. The side-effect therefore **inherits** §A.4's existing trust
of mergeset evidence — it does not make anything less sound than the
already-shipped `Slash` pipeline, and `resolve_slashing_side_effects`
documents that it *assumes* genuineness. Closing this seam means moving
the whole overlay block-validity family to resolve over the mergeset
(out of scope for b2-iv, and inert on every current network at
`dns_activation_daa_score = u64::MAX`). Recorded here so it is not
mistaken for a regression introduced by the side-effect.

### Determinism, reorg, and gating (unchanged)

`S`, `R`, the recipient, and the mint outpoint are pure functions of the
selected-parent bond view + the accepted evidence, so construction and
validation match byte-for-byte. The mint and removal live in
`ctx.mergeset_diff` / `ctx.multiset_hash` and are reverted by the
existing mergeset-diff reversal on reorg, in lockstep with the §A.4
`Slashed` mutation. Inert unless the overlay is configured **and** the
block is past `dns_activation_daa_score` (`u64::MAX` on every current
network).

### Revised C.1.4 implementation row

| Sub-PR | Title | Gated? |
|---|---|---|
| 16.4-b2-iv | The atomic side-effect in `calculate_utxo_state`: per slashed bond (mergeset-resolved, deduped, `Active`/`Unbonding`-gated), **remove** output-0 (`S`) **and mint** the reporter UTXO `R` at `(slashing_tx_id, 0)` — construction == validation, reorg-safe. Reverts the b2-ii mint exemption (a slashing tx is value-trivial), and replaces the b2-iii `verify_expected_utxo_state` reporter-output rule with a **stateless "slashing tx declares no outputs" rule in the isolation validator** (every block, so the mint outpoint is always free). | yes (activation) |
