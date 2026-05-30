# ADR-0012: Mainnet Validator Sortition via On-Chain Commit-Reveal

Status: **Superseded by [ADR-0017](0017-all-active-staker-attestation.md)** — the
        commit-reveal sortition committee was removed; PoS participation is now
        permissionless-by-stake with every active bond attesting (no committee,
        no sortition). Retained for history. (Originally: Accepted, Phase 13 design freeze.)
Date: 2026-05-28
Supersedes: ADR-0009 §"Sortition" mainnet "TBD" pointer.
Depends on: [ADR-0008](0008-hash64-consensus-identity.md) (Hash64 throughout the
            sortition pipeline), [ADR-0009](0009-dns-probabilistic-finality.md)
            (DNS overlay; `validator_set_commitment` consumes the
            sortition output), [ADR-0010](0010-validator-node-architecture.md)
            (validator service runs `is_eligible_this_epoch`),
            [ADR-0011](0011-validator-deployment-and-equivocation-safety.md)
            (`SignedEpochRecord` per epoch; this ADR specifies which
            epochs a validator is eligible to sign in the first place).

## Context

[ADR-0009 §"Sortition"](0009-dns-probabilistic-finality.md) leaves the
mainnet validator-set selection unspecified:

> PoC: deterministic stake-weighted sortition seeded by epoch number
> alone. Mainnet: TBD (commit-reveal in a follow-up ADR).

PR-10.9 in the refined Phase 10 PR plan
([ADR-0010 §"Phase 10 PR plan"](0010-validator-node-architecture.md)) is
the implementation placeholder for that follow-up. This ADR is the
design freeze.

Why a deterministic-seeded sortition is insufficient for mainnet:

- **Bond grinding at creation time.** A deterministic seed function
  `f(epoch, validator_id)` lets an attacker grind their
  `validator_id` (which is `BLAKE2b-512(validator_pubkey)`) at
  keygen time to maximise their selection probability across many
  future epochs. Adversary picks the public key that yields the
  most "in-committee" epochs in the next year.
- **Anchor-targeted attacks.** A coalition that can predict which
  anchors they will be in the committee for can coordinate with
  PoW miners to attest a fork they prefer. Predictability at bond
  creation time is enough — they do not need real-time
  manipulation.
- **No protection against last-minute withdrawal.** A validator
  who learns they would be sortitioned-into a committee but
  predicts that committee will sign a fork they oppose has no
  built-in disincentive to simply go offline. With on-chain
  commit-reveal randomness, withdrawing is detectable and
  slashable.

The standard cryptographic counter is commit-reveal: each round of
sortition consumes a seed that is **fixed** before validators can
know what their selection outcome will be, but **derived from**
validator contributions so no single party (including the
sortition algorithm author) can pre-compute the seed.

This is the same family of construction used by Algorand
(cryptographic sortition via VRF over a public seed) and by
Ethereum's RANDAO + VDF stack. The kaspa-pq variant is simpler:
plain commit-reveal over BLAKE2b-512 with a Byzantine-fault-
tolerant fallback for liveness, rather than VRF + VDF. The price
is one extra epoch of latency between commit and use, and the
known commit-reveal bias of ~`2^(-128)` per epoch from selective
reveal withholding by an adversarial subset (analysis in
§"Public-claim discipline" below).

## Decision

### Two sortition modes (per-network)

The sortition pipeline supports two modes selected at network
configuration time:

| Mode | Networks that use it | Seed derivation |
|---|---|---|
| `Deterministic` | simnet at launch; testnet at launch (switches per below) | `epoch_seed_E = BLAKE2b-512(key=SORTITION_DETERMINISTIC_KEY, input=epoch.to_le_bytes())` |
| `CommitReveal` | mainnet from launch; testnet at `commit_reveal_activation_daa_score` | `epoch_seed_E = BLAKE2b-512(key=SORTITION_SEED_KEY, input=sorted_reveals_E)` with fallback (see §"Fallback rule") |

Both modes feed the **same** downstream
[sortition function](#sortition-function), so a single
implementation path serves both networks. The mode is a per-
network constant in `consensus/core::config::params::Params`,
defaulting to `Deterministic` so a fresh node on an unknown
network behaves as the simnet does (loud about the missing
production seed).

Mode transitions are one-way (`Deterministic → CommitReveal`)
and gated on a hard-fork DAA score; this ADR does not specify a
mode rollback path.

### Commit-reveal protocol

Three-phase pipeline per target epoch `E`:

```text
epoch E-2 (commit window)        epoch E-1 (reveal window)        epoch E (sortition used)
┌───────────────────────────┐   ┌─────────────────────────────┐   ┌───────────────────────┐
│ active validators submit  │   │ committed validators reveal │   │ epoch_seed_E available │
│ SortitionCommitPayload    │→  │ SortitionRevealPayload      │→  │ committee_E computed   │
│ commit = H(r||E||vid)     │   │ payload contains r          │   │ attestations gossiped  │
└───────────────────────────┘   └─────────────────────────────┘   └───────────────────────┘
```

#### Commit window (epoch `E−2`)

Active validators MAY submit one
[`SortitionCommitPayload`](#payload-types) per epoch they want
to be considered for. The payload is wrapped in a transaction
with the dedicated subnetwork id `SUBNETWORK_ID_SORTITION_COMMIT`
(consensus rule in PR-10.9). Constraints:

- **Window**: the parent block's DAA score must satisfy
  `epoch_start(E−2) ≤ daa_score < epoch_start(E−2) + commit_window_blocks`.
- **Uniqueness**: at most one commit per
  `(validator_id, target_epoch)` is accepted on-chain. Duplicates
  are rejected at tx-validation time (not slashable — duplicate
  is a rebroadcast, not equivocation).
- **Validator must be active**: `validator_id` resolves to an
  active bond in the stake registry, `daa_score ≥ activation_daa_score`.
- **Commit value**:
  `commit = BLAKE2b-512(key=SORTITION_COMMIT_KEY, input=r || target_epoch.to_le_bytes() || validator_id.as_bytes())`.
  The `r` is a fresh 32-byte secret. The keyed BLAKE2b-512 plus the
  epoch and validator id make the commitment domain-separated and
  binding.

A validator that did not commit cannot reveal, and is not
sortitioned-in for `E`. Skipping a commit is **not** slashable —
it is voluntary participation.

#### Reveal window (epoch `E−1`)

Each committed validator submits a
[`SortitionRevealPayload`](#payload-types) carrying the 32-byte
`r`. Constraints:

- **Window**: `epoch_start(E−1) ≤ daa_score < epoch_start(E−1) + reveal_window_blocks`.
- **Uniqueness**: at most one reveal per
  `(validator_id, target_epoch)`.
- **Must match a prior commit**: a `SortitionCommitPayload` for
  the same `(validator_id, target_epoch)` must exist, and
  `BLAKE2b-512(key=SORTITION_COMMIT_KEY, input=r || target_epoch.to_le_bytes() || validator_id.as_bytes()) == commit`
  must hold. Otherwise the reveal is rejected at tx-validation.

A validator that committed but did not reveal within the window
is **slashed** at the end of the reveal window (see §"Slashing
rule: commit-without-reveal" below).

#### Sortition use (epoch `E`)

At `epoch_start(E)`, every full node:

1. Collects the set of reveals on-chain for target epoch `E`
   (those committed to in `E−2` and revealed in `E−1`).
2. Derives `epoch_seed_E` from them (see §"Seed derivation").
3. Runs the sortition function (§"Sortition function") to
   compute the committee `committee_E ⊆ active_validators`.
4. From this point, the validator service in ADR-0010 §"Validator
   service runtime" loop uses `committee_E` membership as one of
   its eligibility predicates.

### Payload types

Both payloads round-trip through Borsh and follow the same
versioning convention as the existing
[`StakeBondPayload`](../../consensus/core/src/dns_finality.rs):

```rust
/// Submitted on-chain during epoch E−2.
pub struct SortitionCommitPayload {
    pub version: u16,
    pub validator_id: Hash64,
    pub target_epoch: u64,
    /// commit = BLAKE2b-512(key=SORTITION_COMMIT_KEY,
    ///                      input = r || target_epoch.to_le_bytes()
    ///                           || validator_id.as_bytes())
    pub commit: Hash64,
}

/// Submitted on-chain during epoch E−1.
pub struct SortitionRevealPayload {
    pub version: u16,
    pub validator_id: Hash64,
    pub target_epoch: u64,
    /// 32-byte secret. The commitment is the keyed hash of this
    /// concatenated with target_epoch and validator_id.
    pub reveal: [u8; 32],
}
```

### Seed derivation

Once the reveal window closes, every node derives `epoch_seed_E`
deterministically. The byte layout is consensus-fixed:

```text
sorted_reveals_E =
    reveals_for_target_epoch(E)
        .filter(reveal_matches_prior_commit)
        .sorted_by(|a, b| a.validator_id.cmp(&b.validator_id))

input_bytes =
    target_epoch.to_le_bytes()
    || (sorted_reveals_E.len() as u32).to_le_bytes()
    || for each rev in sorted_reveals_E:
           rev.validator_id.as_bytes()      (64 B)
        || rev.reveal                       (32 B)

if sorted_reveals_E.len() * 3 >= commits_for_target_epoch(E).len() * 2 {
    // Reveals cleared the ≥ 2/3 threshold; use the derived seed.
    epoch_seed_E = BLAKE2b-512(key=SORTITION_SEED_KEY,
                               input=input_bytes)
} else {
    // Fallback — see next subsection.
}
```

The keyed BLAKE2b-512 with `SORTITION_SEED_KEY` is consensus-
fixed; the `-v1` suffix in the key string is the hard-fork
boundary.

### Fallback rule

If the reveal threshold `≥ 2/3 of commits` is **not** met for
epoch `E` (because too many committers withheld their reveal):

```text
epoch_seed_E = BLAKE2b-512(
    key   = SORTITION_FALLBACK_KEY,
    input = epoch_seed_{E-1}.as_bytes() || target_epoch.to_le_bytes(),
)
```

The fallback preserves liveness: the chain continues to produce
attestations using a bias-degraded but well-defined seed. The
withholding validators are independently slashed (§"Slashing
rule: commit-without-reveal" below), so withholding to bias the
seed costs more than the bias is worth in any reasonable
parameterisation.

`SORTITION_FALLBACK_KEY ≠ SORTITION_SEED_KEY` so a node cannot
confuse a fallback seed for a regular one. The two keys differ in
domain only:

```text
SORTITION_SEED_KEY      = b"kaspa-pq-sortition-seed-v1"
SORTITION_FALLBACK_KEY  = b"kaspa-pq-sortition-fallback-v1"
SORTITION_COMMIT_KEY    = b"kaspa-pq-sortition-commit-v1"
SORTITION_DETERMINISTIC_KEY = b"kaspa-pq-sortition-deterministic-v1"
```

All four are consensus-fixed, bumped only by a hard-fork ADR.

### Sortition function

Given `epoch_seed_E` and the active validator set
`active_E = {(vid, stake_v) : v is active at epoch_start(E)}`, the
committee for epoch `E` is the `committee_size_E` validators with
the **lowest priority value**:

```text
priority_v(E) =
    BLAKE2b-512(
        key   = SORTITION_PRIORITY_KEY,
        input = epoch_seed_E.as_bytes() || vid.as_bytes(),
    )
    .first_u128()
    /
    stake_v.max(1)
```

Selection:

```text
committee_E =
    active_E
        .map(|v| (priority_v(E), v))
        .sorted_by_key(|p| p.0)
        .take(min(committee_size_E, active_E.len()))
        .map(|p| p.1)
        .collect()
```

The integer division by `stake_v` makes a 2×-larger-stake
validator twice as likely to land in the bottom-K (i.e. selected),
giving stake-weighted Bernoulli-like sortition. The `.max(1)`
guard prevents division by zero for the (unreachable, but
defensively-coded) zero-stake case.

```text
SORTITION_PRIORITY_KEY = b"kaspa-pq-sortition-priority-v1"
```

Consensus-fixed; hard-fork-bumped.

### Slashing rule: commit-without-reveal

A validator that submitted a `SortitionCommitPayload` for target
epoch `E` but failed to submit a matching
`SortitionRevealPayload` within the reveal window is slashed by
an amount equal to **`commit_without_reveal_slash_sompi`** (a
network parameter). The slash is independent of the existing
ADR-0009 equivocation slashing — a validator can be slashed for
both equivocation and unreveal in the same epoch.

The detection rule is fully on-chain and deterministic: the same
data every node uses to derive `epoch_seed_E` (commits in `E−2`,
reveals in `E−1`) tells every node which validators are
unrevealed. Slashing tx is submitted by any reporter
(`UnrevealSlashingEvidencePayload`, PR-10.9) carrying:

```rust
pub struct UnrevealSlashingEvidencePayload {
    pub version: u16,
    pub target_epoch: u64,
    pub validator_id: Hash64,
    /// The original commit tx outpoint; consensus rule checks
    /// that no matching reveal landed in the reveal window.
    pub commit_outpoint: TransactionOutpoint,
}
```

The reporter receives `unreveal_reporter_reward_sompi` (parameter,
typically a small constant — enough to cover gas, not large enough
to incentivise spurious reports). The remainder of the slash is
burned, matching the equivocation-slashing economics.

Calibration intent: `commit_without_reveal_slash_sompi` should be
≥ `committee_size_E × per_attestation_reward_sompi × epochs_until_unbond`
so deliberate withholding is economically irrational. The exact
value is mainnet parameter §"Parameters" below.

### Activation

| Network | Initial mode | Switchover |
|---|---|---|
| simnet | `Deterministic` | never (test convenience) |
| devnet | `Deterministic` | never |
| testnet | `Deterministic` | switches to `CommitReveal` at `commit_reveal_activation_daa_score` (testnet hard fork) |
| mainnet | `CommitReveal` | from genesis (no `Deterministic` epoch) |

`commit_reveal_activation_daa_score` is a testnet-only parameter;
mainnet is `CommitReveal` from launch. The two-epoch lookahead
(commit at `E−2`, reveal at `E−1`, use at `E`) means the first
two mainnet epochs have **no** committee and use only the
deterministic fallback seed (the `epoch_seed_{E-1}` formula
degrades naturally to the all-zero / genesis case for `E < 2`):

```text
epoch 0: epoch_seed_0 = BLAKE2b-512(SORTITION_FALLBACK_KEY, ZERO_HASH64 || 0u64.to_le_bytes())
epoch 1: epoch_seed_1 = BLAKE2b-512(SORTITION_FALLBACK_KEY, epoch_seed_0 || 1u64.to_le_bytes())
epoch 2: first epoch a committee can use commit-reveal output (if commits at E=0 and reveals at E=1 happened)
```

This means mainnet at launch operates in the
[ADR-0009 §"Three-stage rollout"](0009-dns-probabilistic-finality.md)
`Launch` stage until the commit-reveal pipeline has produced two
full epochs of data. The activation gate to `Active` already
requires `daa_score ≥ dns_activation_daa_score`, which any
parameterisation respecting `dns_activation_daa_score ≥ 3 ×
epoch_length_blocks` satisfies cleanly.

### Parameters

Added to [`DnsParams`](../../consensus/core/src/dns_finality.rs)
in PR-13.2 alongside the existing DNS overlay parameters:

| Parameter | Type | Mainnet recommendation | Description |
|---|---|---|---|
| `sortition_mode` | `SortitionMode` | `CommitReveal` | `Deterministic` (simnet/devnet/testnet-initial) or `CommitReveal` (mainnet). |
| `commit_window_blocks` | `u64` | `epoch_length_blocks / 3` | Block window in epoch `E−2` during which commits are accepted. |
| `reveal_window_blocks` | `u64` | `epoch_length_blocks / 3` | Block window in epoch `E−1` during which reveals are accepted. |
| `min_reveal_threshold_num` | `u32` | `2` | Numerator of the reveal-threshold fraction (default 2/3). |
| `min_reveal_threshold_denom` | `u32` | `3` | Denominator. |
| `committee_size` | `u32` | implementation-tuned at activation; PR-10.9 freezes the value | Per-epoch committee size; bounded so the per-epoch attestation total fits in `MAX_ATTESTATIONS_PER_SHARD × shards_per_epoch`. |
| `commit_reveal_lookahead_epochs` | `u8` | `2` | Number of epochs between commit and sortition use. Larger lookaheads are safer (more time for finality on the commit / reveal txs themselves) but reduce committee responsiveness to bond changes. `2` is the minimum that supports the three-phase pipeline. |
| `commit_without_reveal_slash_sompi` | `u64` | network-tuned; see §"Slashing rule" | Slash amount per unrevealed commit. |
| `unreveal_reporter_reward_sompi` | `u64` | small fixed cost | Paid to whoever submits the `UnrevealSlashingEvidencePayload`. |
| `commit_reveal_activation_daa_score` | `Option<u64>` | `None` (mainnet) / per-testnet value | If `Some`, the `Deterministic → CommitReveal` switchover DAA score. `None` on mainnet (always CommitReveal). |

### Public-claim discipline (binding)

The kaspa-pq Phase 13 sortition claim, verbatim:

- ✅ "Bias-resistant stake-weighted sortition with on-chain
  commit-reveal."
- ✅ "Resistant to last-block grinding: epoch_seed_E is fixed
  one full epoch before sortition use."
- ✅ "Resistant to bond-creation grinding: priority depends on
  epoch_seed_E, which an attacker cannot predict at bond-creation
  time."
- ✅ "Liveness-preserving under reveal sabotage: fallback rule
  guarantees a deterministic seed exists even when reveals fall
  below the 2/3 threshold."
- ✅ "Withholding reveals after committing is slashable."
- ❌ "Unbiased random oracle." **Not claimed.** Commit-reveal has
  a known bias of ~`2^(−128)` per epoch from selective reveal
  withholding by an adversarial subset (an adversary controlling
  K validators can choose to reveal or withhold any subset of
  their K reveals, biasing the seed by `O(K · 2^(−128))`). The
  slashing rule makes withholding costly; the public-claim
  discipline forbids over-claiming the residual bias as zero.
- ❌ "VRF-based" / "VDF-based." **Not claimed.** This ADR does
  **not** introduce a VRF (verifiable random function) or VDF
  (verifiable delay function). The construction is plain
  commit-reveal over keyed BLAKE2b-512. A future ADR may swap in
  a VRF for cryptographic non-malleability strengthening; this
  ADR does not require it.
- ❌ "Constant-time sortition." **Not claimed.** Sortition is
  O(N) hashes per epoch where N = `|active_validators|`. Mainnet
  sizing assumes `N ≤ 10000` and a sortition wall-time budget of
  < 100 ms; both targets are PR-10.9 acceptance criteria.

External material **must** use the phrasings above and **must
not** over-claim cryptographic guarantees beyond commit-reveal.

## Consequences

### Positive

- **Grinding resistance.** A validator who chooses
  `validator_pubkey` to maximise its sortition probability gains
  no advantage past the first two epochs after `CommitReveal`
  activates: the `epoch_seed_E` is unpredictable at keygen
  time.
- **Liveness-preserving.** The 2/3-fallback rule guarantees the
  chain produces a sortition seed every epoch even under
  adversarial reveal-withholding, at the cost of one epoch of
  bias-degraded seed plus the per-validator slash.
- **Same downstream function for both modes.** The sortition
  function (priority computation + top-K) is identical between
  `Deterministic` and `CommitReveal`; only the seed source
  differs. PR-10.9 ships one sortition-function implementation
  with two seed-derivation paths.
- **On-chain auditable.** Every step of the pipeline (commit,
  reveal, slash) is a normal transaction; an outside observer
  can independently recompute `epoch_seed_E` from the public
  chain and verify the committee.
- **Stake-weighted but not stake-monopolistic.** A 51%-stake
  validator does not get 100% of committee slots — they get
  `priority_v / stake_v`-weighted slots and still must commit and
  reveal in time. The largest stake still has the largest single
  probability, but every smaller validator with a fresh reveal
  has a non-zero chance every epoch.

### Negative

- **Two-epoch latency.** The commit-reveal pipeline adds two
  epochs of latency between bond activation and first eligible
  attestation. ADR-0010's runbook step 5 ("wait for activation")
  already includes this latency in its acceptance criteria but
  this ADR makes it explicit: bond activates at `E`, validator
  can first attest at epoch `E + 2`.
- **Reveal-withholding economic attack surface.** An adversary
  controlling `K` validators can withhold `0..K` of their
  reveals each epoch, biasing `epoch_seed_E` by `O(K · 2^(−128))`
  and forcing the slashing pipeline to fire. The slashing
  amount is calibrated to make this economically irrational, but
  it is a non-zero residual surface.
- **Extra on-chain mass.** Two payload types per validator per
  epoch (commit + reveal) plus one unreveal-evidence type as
  needed. Each payload is small (~120 B) but the per-epoch
  multiplier matters for parameterisation. `committee_size` and
  `commit_window_blocks` must be tuned together so commits + 
  reveals + attestation shards fit inside per-block mass.
- **Mode switchover is a hard fork.** Testnet `Deterministic →
  CommitReveal` activation requires coordinated network upgrade
  at the chosen DAA score. This is a known-good pattern (Kaspa
  upstream uses DAA-score gating for analogous activations) but
  worth flagging as an extra fork event.

### Neutral

- **No new keys.** Sortition uses the existing validator's
  ML-DSA-65 key to sign the commit and reveal transactions —
  same key as attestations. No additional key-management ceremony
  is required.
- **No VRF / VDF dependency.** This ADR is implementable on
  exactly the cryptographic primitives kaspa-pq already ships:
  BLAKE2b-512 (`crypto/hashes`), ML-DSA-65 (`crypto/txscript`),
  Borsh, the existing transaction-validation pipeline. A future
  VRF ADR can strengthen specific properties without rewriting
  the protocol.

## Phase 13 PR plan (this ADR's slot)

| PR | Title | Status |
|---|---|---|
| 13.1 | This ADR | landed |
| 13.2 | `dns_finality.rs` SortitionMode + CommitRevealParams + SortitionCommitPayload + SortitionRevealPayload + UnrevealSlashingEvidencePayload + derive_epoch_seed + compute_validator_priority + select_committee helpers + tests | next |
| 13.3 | Spec update (ADR-0012 + Phase 13 row + Phase 13 acceptance criteria 1/4 + v0.6) | next |

Implementation slot (gated on Phase 1–9 baseline + PR-10.5 +
PR-10.6, layers onto PR-10.9):

| PR | Title | Layers onto | Status |
|---|---|---|---|
| 10.9 (refined) | Validator sortition — `SortitionMode::CommitReveal` consensus rules; `consensus/src/processes/validator_sortition.rs`; subnetwork-id tx routing for commit/reveal/unreveal-evidence payloads; on-chain commit-reveal slashing pipeline | PR-10.9 (was: "PoC deterministic; mainnet TBD") | deferred |

PR-10.9's deferred slot in [ADR-0010 §"Phase 10 PR plan"](0010-validator-node-architecture.md)
is now no longer a "TBD" — it implements this ADR.

## References

- [ADR-0009 — DNS Probabilistic Finality Overlay](0009-dns-probabilistic-finality.md)
  §"Sortition" (the line this ADR closes), §"`SlashingEvidencePayload`"
  (the existing equivocation-slashing pipeline this ADR runs
  alongside).
- [ADR-0010 — Validator Node Architecture](0010-validator-node-architecture.md)
  §"Validator service runtime" (the eligibility predicate this
  ADR feeds into).
- [ADR-0011 — Validator Single-Host Deployment + Equivocation-Safety](0011-validator-deployment-and-equivocation-safety.md)
  (operational layer; this ADR's commit/reveal txs are signed by
  the same validator key the equivocation guard protects).
- Algorand sortition (Chen & Micali, 2017) — closest published
  precedent for cryptographic-sortition-with-stake-weighting. The
  kaspa-pq variant differs by using plain on-chain commit-reveal
  instead of VRFs.
- Ethereum RANDAO + VDF (post-merge beacon chain) — closest
  published precedent for on-chain commit-reveal randomness.
  The kaspa-pq variant omits the VDF post-processing to keep the
  consensus surface minimal; a future ADR may add a VDF.
