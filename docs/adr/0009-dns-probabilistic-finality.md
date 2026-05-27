# ADR-0009: DNS Probabilistic Finality Overlay

Status: Accepted (Phase 10 design freeze; activation deferred until Phases 1–9 stabilise)
Date: 2026-05-28
Supersedes: —
Depends on: [ADR-0002](0002-mldsa65-p2pkh.md) (ML-DSA-65 signatures),
            [ADR-0007](0007-layered-pow.md) (Layered PoW / `blue_work`),
            [ADR-0008](0008-hash64-consensus-identity.md) (Hash64 identity).

## Context

Phases 1–9 give Kaspa-PQ a strong Pure-PQ-PoW baseline: ML-DSA-65 signatures,
LtHash16_1024 UTXO accumulator, 512-bit Layer 0 PoW finalizer, 64-byte
consensus identity. Block production stays PoW/GHOSTDAG, tip selection
stays `blue_work`-driven, and confirmation is the upstream
work-depth probabilistic statement.

Pure PQ-PoW carries the upstream risk profile: a PoW-majority adversary
can in principle deep-reorg arbitrarily far. The mitigation in upstream
Kaspa is pruning + finality depth heuristics; in a post-quantum context
the asymmetry is unchanged because the PoW itself, while quantum-hardened
against single-block grinding, still depends on a single resource
(hashing power). A two-resource confirmation overlay — where deep history
replacement requires both PoW dominance and PoS dominance — is the
DNS-paper-style answer.

The DNS paper defines the value of the overlay precisely:

> History confirmation is `WorkDepth(B) ≥ cW` **and** `StakeDepth(B) ≥ cS`.
> An attacker who controls only one resource cannot rewrite confirmed
> history; PoW surplus does not substitute for PoS deficit and vice
> versa. The reorg probabilities do **not** multiply unconditionally —
> the value is non-substitutability, not joint independence.

A previous design draft proposed a hard `dns_finality_point` cutoff
("anything before this block is consensus-final"). That is BFT-flavoured
hard finality, not DNS, and it conflicts with the DNS paper's explicit
probabilistic framing. The right shape is a `WorkScore × StakeScore`
two-dimensional dominance gate.

## Decision

Kaspa-PQ adds a **DNS Probabilistic Finality Overlay** as a separate,
post-launch consensus layer. PoW/GHOSTDAG continues to drive block
production and tip selection unchanged. PoS validators issue attestations
over selected-chain anchors; those attestations are committed on-chain as
partial certificates and contribute to a deterministic `StakeScore`. A
candidate fork that exits a DNS-confirmed prefix is rejected unless it
beats the canonical chain on **both** `WorkScore` and `StakeScore` by
explicit margins.

### What stays unchanged

The following are explicitly kept as the kaspa-pq baseline; this overlay
must not touch them:

| Component | kaspa-pq Phase 1–9 form | Kept |
|---|---|---|
| Block production | PoW with Layered PoW (ADR-0007) | Unchanged |
| Tip selection | GHOSTDAG | Unchanged |
| Work ordering | `blue_work` (`BlueWorkType = Uint576`, see PR-8.5) | Unchanged |
| DAA | `blue_work`-driven, PoW-only | Unchanged |
| Block hash / txid / merkle | `Hash64` per ADR-0008 | Unchanged |
| Mempool | as is | Unchanged |
| Short-term confirmation | upstream probabilistic confirmation against `blue_work` | Unchanged |

### What the overlay adds

Exactly four type families plus parameters and a rule:

1. **`StakeBondPayload`** — a transaction kind that locks coins to a
   validator key. The validator key is an ML-DSA-65 public key
   (1952 bytes). The bond carries an `activation_daa_score` and an
   `unbonding_period_blocks` that satisfies the long-range bound
   `U ≥ R + E` (see §"Long-range bound" below).
2. **`StakeAttestation`** — a single ML-DSA-65 signature by a validator
   over `(epoch, selected_chain_anchor, validator_set_commitment,
   bond_outpoint)`. A raw attestation is 3309 signature bytes plus
   `O(100)` metadata.
3. **`StakeAttestationShardPayload`** — a transaction kind carrying
   8–16 `StakeAttestation`s, capped per block. Multiple shards across
   multiple blocks reconstruct an epoch certificate; no single
   "huge certificate tx" is required.
4. **`SlashingEvidencePayload`** — a transaction kind carrying two
   incompatible attestations from the same `(bond_outpoint,
   validator_id, epoch)`. Burns the bond if submitted within the
   evidence window.

Plus `DnsParams` (consensus parameters), `DnsConfirmation` (RPC view
type), and one consensus rule:

```
check_dns_reorg_rule(candidate, canonical_tip):
    let confirmed_anchor = dns_store.latest_confirmed_anchor()
    if candidate ⊇ confirmed_anchor:
        return Ok(())                                 # no DNS gate triggered

    let I = common_ancestor(candidate, canonical_tip)
    let cand_W   = work_score_after(candidate,    I)
    let canon_W  = work_score_after(canonical_tip, I)
    let cand_S   = stake_score_after(candidate,    I)
    let canon_S  = stake_score_after(canonical_tip, I)

    # **Two-dimensional dominance.** Neither inequality on its own is
    # enough; a PoW-only or stake-only attacker cannot pass.
    if cand_W > canon_W + params.emergency_work_margin
       && cand_S > canon_S + params.emergency_stake_margin:
        return Ok(())                                 # rare reorg path

    Err(RuleError::DnsDominanceViolation)
```

`emergency_work_margin` and `emergency_stake_margin` are consensus
parameters set such that overcoming both at once is exponentially less
likely than overcoming either on its own.

### Phase-specific behaviour

| Tier | DNS rule shape |
|---|---|
| PoC | Hard checkpoint: candidates that exit the latest DNS-confirmed anchor are rejected outright. Acceptable for testing because failure modes are loud. |
| Testnet | Hard checkpoint + diagnostic logging that records what the 2D rule **would** have done for every rejected candidate. |
| Mainnet | Two-dimensional dominance per the rule above. No hard checkpoint. |

The PoC hard-checkpoint behaviour is intentionally **not** DNS finality —
external material describing the PoC must use phrasing like "DNS-style
checkpointing for testing" rather than "DNS probabilistic finality is
active".

### Three-stage rollout

The overlay cannot be enabled at genesis because no stake exists. The
launch sequence is:

1. **Launch phase (PoW-only).** Phases 1–9 are in force. No StakeBond /
   StakeAttestation transactions are valid on chain.
2. **Bootstrap phase.** `StakeBondPayload` transactions are accepted.
   Validators may begin issuing `StakeAttestation` gossip and submitting
   `StakeAttestationShardPayload`s on-chain. The DNS gate is **not**
   enforced; attestations are accumulated for visibility only.
3. **Activation phase.** Once
   `total_active_stake ≥ MIN_ACTIVE_STAKE` **and**
   `active_validators ≥ MIN_ACTIVE_VALIDATORS` **and**
   `daa_score ≥ dns_activation_daa_score`, the DNS gate engages.
   Subsequent reorgs that exit a DNS-confirmed anchor must satisfy the
   two-dimensional dominance rule.

`MIN_ACTIVE_STAKE`, `MIN_ACTIVE_VALIDATORS`, and
`dns_activation_daa_score` are consensus parameters tunable per network
(mainnet / testnet / simnet / devnet).

### Long-range bound

PoS introduces long-range attack surface that PoW alone does not have.
The overlay handles it with a consensus rule on the unbonding period:

```text
let R = max_reorg_horizon_blocks
let E = evidence_window_blocks
let U = unbonding_period_blocks

require: U ≥ R + E
```

Validators who attest two incompatible histories at the same epoch are
slashable for the entire `E` window after each attestation; their bond
cannot be withdrawn before `R + E` blocks have passed. This bounds the
"sell your old keys" long-range attack.

Weak subjectivity is **not** eliminated. A new node that has been offline
longer than `R` blocks must obtain a checkpoint from a trusted (or
sufficiently diverse) set of peers before it can rejoin. This is the
same trade-off all PoS designs accept and the spec calls it out
explicitly in §"Public-claim discipline".

### StakeScore mechanics

- **Validator/epoch uniqueness.** Each `(bond_outpoint, validator_id,
  epoch)` triple contributes at most once. Double-counting the same
  validator across attestation shards is forbidden by the consensus
  state-transition rule.
- **Per-epoch normalisation.** Rather than accumulating raw signed
  stake amounts, each epoch normalises:
  ```
  signed_fraction_e(anchor) = valid_signed_stake_e / total_active_stake_e
  stake_score_increment_e   = floor(signed_stake_e × STAKE_SCORE_SCALE
                                                   / total_active_stake_e)
  ```
  where `STAKE_SCORE_SCALE = 1_000_000_000` (the fixed-point integer
  scale used to avoid floats in consensus arithmetic).
- **StakeScore(H).** Sum of `stake_score_increment_e` for every epoch
  whose anchor is on the selected chain ending at `H`. Computed
  deterministically from on-chain `StakeAttestationShardPayload`
  contents — every node reaches the same number.

### Validator selection (sortition)

- **PoC.** Deterministic stake-weighted sampling seeded by
  `(epoch, lookback_anchor_hash, pruning_point)`. Easy to test but
  vulnerable to seed grinding by PoW majority. Documented as
  PoC-only.
- **Mainnet.** Either (a) commit-reveal randomness over two-epoch
  lookahead, or (b) a PQ-safe VRF-like ticket scheme once one is
  available. The choice is left to a follow-up ADR; what is fixed here
  is that the PoC scheme **must not** be reused for mainnet.

The Poisson-DNS theorem from the paper assumes large validator sets with
stake-proportional random tickets via VRF or commit-reveal. A small
fixed committee with deterministic sortition does **not** satisfy the
theorem's assumptions, and external material must not invoke it for the
PoC.

### Attestation target

Validators sign **only** selected-chain anchor blocks, not every block in
the DAG. The message layout:

```
msg = BLAKE2b-256(
    "kaspa-pq-v1/stake-attestation"
    || network_id
    || epoch
    || target_hash                       (Hash64 selected-chain anchor)
    || target_daa_score
    || validator_set_commitment          (Hash64)
    || bond_outpoint
)
```

The ML-DSA-65 context for attestation signing is
`b"kaspa-pq-v1/att/mldsa65"` — distinct from the transaction signing
context (`b"kaspa-pq-v1/tx/mldsa65"`, ADR-0002 §2) so an attestation
can never be replayed as a transaction signature or vice versa.

### Why partial certificates (8–16 attestations per block)

A naïve "1 epoch = 1 big certificate transaction" with 64 validators
produces a transaction of roughly:
```
64 × (3309-byte ML-DSA-65 signature + ~100 bytes metadata)
≈ 216 KB
```
At `mass_per_tx_byte = 1` (consensus params, see ADR-0005) this consumes
roughly 43 % of `max_block_mass = 500_000` for **byte mass alone**. Adding
`mass_per_sig_op = 6000` (Phase 6) per ML-DSA verify pushes the
single-tx mass well over the per-block budget.

Instead, each block carries at most `max_attestations_per_block` (8–16)
attestations in a `StakeAttestationShardPayload`. Nodes aggregate shards
across blocks per `(epoch, target_hash, validator_set_commitment)`. No
single block hosts a 216 KB certificate, and the per-block ML-DSA verify
cost stays inside the mass budget.

## Consequences

### Positive

- **Two-resource confirmed history.** Deep reorg of a DNS-confirmed
  prefix requires both PoW dominance and PoS dominance simultaneously.
  A PoW-majority attacker alone, or a stake-majority attacker alone,
  cannot rewrite confirmed history.
- **Pure PoW behaviour is preserved when the overlay is dormant.** Phases
  1–9 ship without DNS; the overlay only engages once activation
  conditions are met.
- **PoS is added as the smallest possible layer.** No block-producer
  responsibilities, no consensus-critical sortition for liveness, no
  Ethereum-style slot architecture.

### Negative

- **Confirmation latency.** Mainnet DNS confirmation takes
  `O(epochs × epoch_length × block_time)` — minutes, not seconds. The
  upstream PoW-only probabilistic confirmation remains available for
  applications that need second-scale finality.
- **Liveness depends on both layers when DNS is active.** If validators
  go offline, `StakeDepth` stalls; if PoW miners go offline,
  `WorkDepth` stalls. Both halt history confirmation. DNS-paper
  framing: the overlay buys non-substitutability at the cost of
  joint dependence.
- **Long-range attack surface, weak subjectivity, validator key
  management, certificate mass, sortition design** all become new
  attack faces that pure PoW does not have.
- **DNS shards consume block mass.** Each attestation is 3309 bytes
  plus metadata; 8 attestations is roughly 27 KB per block. Phase 10
  mass policy must reserve a portion of `max_block_mass` for
  attestation shards.

### Neutral

- The DNS overlay does not change `blue_work` or DAA. Existing tooling
  that reads PoW-side confirmation continues to work unchanged before
  and after activation.

## Public-claim discipline (binding)

The following phrasings are normative; external material that describes
kaspa-pq DNS finality must use them or equivalents and must not promise
properties the design does not provide:

✅ "PoW-ledger + PoS probabilistic finality."
✅ "Two-resource confirmed history."
✅ "Deep reorg of a DNS-confirmed prefix requires both WorkScore and
   StakeScore dominance."
✅ "Non-substitutability: PoW surplus does not substitute for PoS
   deficit and vice versa."
✅ "Liveness depends on both PoW miners and PoS validators while the
   overlay is active."
✅ "Weak subjectivity remains: new nodes need a recent peer-supplied
   checkpoint to safely rejoin."

❌ "BFT finality" / "hard finality" — Mainnet DNS is **probabilistic**.
   The PoC hard-checkpoint mode is a testing convenience, not a finality
   property.
❌ "Reorg probability is the product of PoW and PoS reorg probabilities"
   — The DNS paper explicitly does **not** claim this. The value is
   non-substitutability, not joint independence.
❌ "DNS gives 2^k post-quantum finality" — quantitative claims must
   accompany the actual `cW`, `cS`, `emergency_work_margin`, and
   `emergency_stake_margin` values for the network in question.

## Phase 10 implementation order

The overlay lands as a separate PR series **after** Phases 1–9 stabilise:

| PR | Title | Status |
|---|---|---|
| 10.1 | This ADR | landed |
| 10.2 | Spec update — Phase 10 row + DNS public-claim discipline | next |
| 10.3 | `consensus/core/src/dns_finality.rs` type stubs + `DnsParams` | next |
| 10.4 | `subnetwork_id`-based StakeBond / StakeAttestationShard / SlashingEvidence tx kinds (or `TxKind` migration) | deferred |
| 10.5 | `StakeScore` deterministic aggregation from on-chain shards | deferred |
| 10.6 | PoC hard-checkpoint reorg gate behind a feature flag | deferred |
| 10.7 | Mainnet two-dimensional dominance rule + tests | deferred |
| 10.8 | Validator sortition (PoC deterministic; mainnet commit-reveal in a follow-up ADR) | deferred |
| 10.9 | `DnsConfirmation` RPC type + wRPC/WASM bindings | deferred |

PR-10.1 — PR-10.3 give the design freeze and the type surface so that
downstream Phase 10 work has a stable contract to write against.
Everything from PR-10.4 onward is consensus-critical and must wait
until the Phases 1–9 baseline is shipped and stable.

## References

- [ADR-0001 — Network isolation](0001-network-isolation.md) (DNS network
  parameters scope per network).
- [ADR-0002 — ML-DSA-65 P2PKH](0002-mldsa65-p2pkh.md) (attestation
  signature scheme; distinct context string).
- [ADR-0005 — Mass / DoS policy](0005-mass-policy.md) (per-block
  attestation shard mass budget reserve).
- [ADR-0007 — Layered PoW](0007-layered-pow.md) (`blue_work` =
  `WorkScore`).
- [ADR-0008 — Hash64 consensus identity](0008-hash64-consensus-identity.md)
  (attestation target hashes are `Hash64`).
- DNS paper (user-provided, summarised inline above).
