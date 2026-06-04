# ADR-0009: DNS Probabilistic Finality Overlay

> **⚠️ ML-DSA-65-era design doc (HISTORICAL — audit M-03).** The signature scheme is now **ML-DSA-87** (pk 2592 B / sig 4627 B) per [ADR-0019](0019-mldsa87-migration.md); the `ML-DSA-65` / `1952` / `3309` values below are the original draft and are **not current consensus**. The protocol/design structure still applies, and the live finality model has since moved to the DNS-v3 canonical-lagged-anchor + two-dimensional reorg gate.

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

**Implementation note (audit H-02 — read this before quoting the predicate above).**
The shipped production presets set `cW = required_work_depth = 0`, so the confirmation
*predicate* (`is_dns_confirmed`) reduces to **stake-depth only**: it confirms a
**stake-confirmed canonical lagged anchor**, not an independent PoW-depth count. The
PoW dimension is NOT part of confirmation. Non-substitutability — the property that a
heavier PoW chain cannot rewrite a stake-confirmed anchor — is enforced by the
**two-dimensional reorg gate** (`check_dns_reorg_rule`, `TwoDimensionalDominance`),
which demands BOTH a WorkScore and a StakeScore dominance margin over canonical
*measured since the common ancestor* (deltas), not the absolute cumulative work the
predicate reads. So the accurate public description of the current design is
**"stake-confirmed canonical anchor + two-dimensional reorg gate"**, NOT "Double
Nakamoto confirmation". To make `WorkDepth` a real confirmation input, set `cW > 0`
AND change the pipeline's `work_depth` from `blue_work(sink)` (cumulative) to
`blue_work(tip) − blue_work(confirmable_anchor)` (anchor-relative depth).

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

## Addendum A — Phase 10 implementation conventions (binding)

The original decision (above) froze the design but left several
implementation conventions unstated. Implementing them by guess in
consensus-critical code risks a chain split, so this addendum pins them.
It is **binding** for PR-10.9 onward and corrects one bug in the original
§"Attestation target". Added after PR-10.4 / PR-10.4-db / PR-10.9a / the
PR-10.9 lifecycle helpers landed.

### A.1 Bond outpoint convention

The **bond outpoint** — the `StakeBonds` store key and the value an
attestation/slashing payload references — is **output index 0** of the
`StakeBondPayload` transaction (`TransactionOutpoint { transaction_id =
tx.id(), index = 0 }`). Output 0 is the bond-locking output; any further
outputs are change. One bond per stake-bond transaction. This makes the
bond outpoint deterministic and removes any "which output" ambiguity from
attestation references.

### A.2 Bond visibility = deterministic selected-chain aggregation, NOT per-tx

Bond-dependent validation (existence, `Active` status, attestation
signature, `(bond_outpoint, validator_id, epoch)` uniqueness, StakeScore
contribution) is a **deterministic state transition computed over the
selected chain** — it is **not** performed during per-transaction
UTXO-context validation. Rationale: the bond set is global derived state
(like the UTXO set), so a per-tx check against it is point-of-view
inconsistent across nodes and would split the chain.

Consequently:

- **Transaction-level validation** (isolation + mempool admission) of
  StakeBond / StakeAttestationShard / SlashingEvidence stays **stateless**:
  borsh-decodability, payload version, ML-DSA length invariants, shard
  cardinality + single-anchor tuple, equivocation well-formedness. This is
  exactly PR-10.4; **no bond store is consulted at tx-validation time**.
- A `StakeAttestationShardPayload` transaction is **accepted on-chain even
  if its referenced bonds are not (yet) on the selected chain**; it simply
  contributes nothing to `StakeScore` until they are. This matches the
  Bootstrap-phase "attestations are accumulated for visibility only".
- `StakeScore(H)` (A.5) counts an attestation's stake **only if**, on the
  selected chain ending at `H`: (a) its referenced bond exists and is
  `Active` (per `effective_bond_status`) at the attestation's
  `target_daa_score`, (b) its ML-DSA-65 signature verifies against the
  bond's `validator_pubkey` under `ATTESTATION_MLDSA65_CONTEXT`, and (c) the
  `(bond_outpoint, validator_id, epoch)` triple has not already been counted.

### A.3 Attestation message layout (corrects §"Attestation target")

The signed attestation message **MUST** bind `network_id` and
`bond_outpoint`. The current `stake_attestation_message(epoch, target_hash,
target_daa_score, validator_set_commitment)` omits both, leaving an
attestation signature unbound to any specific bond (replayable across
bonds) or network. The canonical message is:

```text
msg = BLAKE2b-256(
    key   = ATTESTATION_MESSAGE_DOMAIN ("kaspa-pq-v1/stake-attestation"),
    input = network_id
         || epoch.to_le_bytes()
         || target_hash            (Hash64)
         || target_daa_score.to_le_bytes()
         || validator_set_commitment (Hash64)
         || bond_outpoint          (transaction_id Hash64 || index u32 LE),
)
```

`stake_attestation_message` must be updated to take `network_id` and
`bond_outpoint`. This changes the signed bytes; it is a pre-activation
breaking change and therefore acceptable (no live attestations exist).

### A.4 Bond population & reorg handling (PR-10.9b)

The `StakeBonds` store is **derived state of the selected chain**, applied
exactly like the UTXO set, inside the virtual processor's chain-path
application:

- On a block **joining** the selected chain (`ChainPath.added`): for each
  accepted `StakeBondPayload` tx, insert
  `stake_bond_record_from_payload(payload, bond_outpoint)` keyed by its
  output-0 outpoint, stamped with the merging block's DAA score; for each
  accepted `SlashingEvidencePayload`, set the target bond's `slashed_at_daa_score`;
  for an unbond tx, set `unbond_request_daa_score`.
- On a block **leaving** the selected chain (`ChainPath.removed`): revert
  those mutations (delete inserted bonds, clear slash/unbond stamps).

The store therefore always reflects the selected-chain tip's bond set.
Activation (`Pending → Active`) is **not** a write — it is derived at read
time from `activation_daa_score` via `effective_bond_status`.

### A.5 StakeScore aggregation pass & uniqueness (PR-10.5 → wired)

After the bond set is updated for a new sink, aggregate per-epoch tallies
deterministically from the `StakeAttestationShardPayload`s on the selected
chain: dedup `(bond_outpoint, validator_id, epoch)`, gate each attestation
by A.2(a–c), build `EpochStakeTally { signed_stake_sompi,
total_active_stake_sompi }` (denominator = total `Active` stake at the
epoch), then `compute_stake_score`. Write the result + the last
DNS-confirmed anchor (`is_dns_confirmed`) into the `DnsState` singleton.
This is the value the reorg gate (A.6) and `getDnsConfirmation` RPC read.

### A.6 Revised implementation order (supersedes the PR table above for Phase 10)

Done: PR-10.4 (stateless tx kinds), PR-10.4-db (DnsState + StakeBonds
stores), PR-10.9a (`verify_mldsa65_with_context`), PR-10.9 lifecycle
helpers. The original "PR-10.9c per-tx bond check" is **dropped** (A.2).
Remaining, in order:

1. **A.3 fix** — rebind `stake_attestation_message` to `network_id` +
   `bond_outpoint` (+ update its tests). Pure consensus-core.
2. **PR-10.9b** — bond population + reorg revert in the virtual processor
   (A.4), behind the same `dns_params.is_some()` dormancy guard as the gate.
3. **PR-10.5-wire / A.5** — StakeScore aggregation pass writing `DnsState`.
4. **PR-10.6/10.7** — reorg gate calling `check_dns_reorg_rule` in
   `sink_search_algorithm`, guarded; `RuleError::DnsFinalityReorgRejected`.
5. **PR-10.14** — `getDnsConfirmation` RPC over `DnsState`.
6. **PR-10.11** — block-template DNS overlay inclusion policy.

Steps 2–4 only become live once a network sets `dns_params = Some(..)` and
reaches the Activation phase; on all current networks they are inert.

## Addendum B — Per-block active-bond view + reward-eligibility (binding)

Status: Accepted
Date: 2026-05-29
Extends: Addendum A (§A.2 bond-visibility model, §A.4 bond
         population/reorg, §A.5 StakeScore aggregation).
Consumed by: [ADR-0013](0013-validator-reward-distribution.md)
         §"Coinbase fan-out" + Addendum B (the validator reward
         track) and the future slashing-validation rule (PR-10.12′).
Implements: PR-10.5′-b2 / b3.

### B.0 Why this addendum exists (the gap A.2 left)

The ADR-0013 validator-reward track pays per-attestation outputs
**in the block's own coinbase**. But the coinbase is validated
**per-block**: `verify_expected_utxo_state` →
`verify_coinbase_transaction` recomputes the expected coinbase from
the block's **own selected-parent view** and rejects on hash
mismatch (`processes/coinbase.rs::expected_coinbase_transaction`,
called per-block inside `resolve_virtual`). So every coinbase output
**must be a deterministic function of the block's own view**.

§A.2/§A.4 deliberately built the `StakeBonds` store as a single
**virtual-commit-time global** (mutated only in
`stage_dns_bond_mutations` over `ChainPath`, read only by the §A.5
StakeScore walk) and made **tx-level** validation stateless. There
is **no per-block bond view** — no bond analogue of
`selected_parent_utxo_view.compose(&mergeset_diff)`. Resolving a
bond from the global store during per-block coinbase validation would
read "whichever virtual happens to be current," which is **not a
function of the block** → non-deterministic → **chain split**.

A reward output's recipient comes from
`StakeBondRecord::owner_reward_spk_payload` (ADR-0013 Addendum B);
the attestation does **not** carry it. So as-of-block bond resolution
is unavoidable. This addendum pins the missing mechanism.

### B.1 The per-block active-bond view

Introduce a per-block **active-bond view**, built exactly like the
UTXO view:

```text
bond_view(H) = bond_view(selected_parent(H)) ∘ bond_diff(H)
```

- `bond_diff(H)` = the `Vec<BondMutation>` from
  `bond_mutations_from_accepted_txs(accepted_txs(H), daa_score(H))`
  (already exists, deterministic from retained acceptance data;
  §A.4). `Insert` adds a `StakeBondRecord`; `Slash` / unbond stamp
  the existing one — reversible, so the view composes and **reorgs**
  identically to the UTXO diff.
- The **anchor** is the bond set at the **pruning point** (a snapshot
  persisted like the pruning-point UTXO set); blocks below the
  pruning point are never re-validated, so the view is only ever
  reconstructed forward from that anchor — mirroring UTXO exactly.
- `resolve_virtual` already walks the chain composing the UTXO
  `accumulated_diff`; it maintains the running `bond_view` the same
  way and passes the **selected-parent** `bond_view` into
  `verify_expected_utxo_state` alongside `selected_parent_utxo_view`.
- A bond's `Pending → Active` transition stays **read-time-derived**
  from `activation_daa_score` via `effective_bond_status` (§A.4) — it
  is never a diff entry.

`bond_view(H)` answers, deterministically as-of `H`: *does
`bond_outpoint` resolve to a record, and is it `Active` at a given
DAA score, and what is its `owner_reward_spk_payload`?*

### B.2 Relationship to §A.2 (tx-level stays stateless)

This addendum **extends, does not overturn, §A.2**. The boundary:

- **Transaction-level** validation (isolation + mempool admission)
  remains **stateless** — no bond store, no `bond_view`, exactly as
  §A.2 froze it. A `StakeAttestationShard` tx is still individually
  admissible regardless of bond state. This preserves point-of-view
  consistency at the mempool layer.
- The `bond_view` is a **block-level** consensus input (a function of
  the block's selected-chain prefix), used only inside
  `verify_expected_utxo_state`. It is the same *class* of object §A.2
  endorsed ("a deterministic state transition computed over the
  selected chain") — A.2 simply declined to materialise it per-block
  because StakeScore (its only consumer then) is computed once at
  virtual-commit. The reward track is the first **per-block**
  consumer, so the view must now be materialised per-block.

This resolves the apparent tension with the user-selected
"Model B" (tighten shard acceptance): the tightening is realised as a
**block-validity rule** over `bond_view` (B.4), **not** as a tx-level
bond check (which §A.2 correctly forbids).

### B.3 Reward-eligibility predicate (per attestation, as-of `H`)

An attestation `a` in a `StakeAttestationShard` tx included in block
`H` is **reward-eligible** iff, against `bond_view(H)`:

- **(a) Bond active.** `a.bond_outpoint` resolves to a record that is
  `Active` (via `effective_bond_status`) at `a.target_daa_score`.
- **(b) Signature valid.** `a.signature` verifies under
  `ATTESTATION_MLDSA65_CONTEXT` against the bond's `validator_pubkey`
  over `stake_attestation_message(network_id, a.epoch, a.target_hash,
  a.target_daa_score, a.validator_set_commitment, a.bond_outpoint)`
  (the §A.3 layout — byte-identical to the §A.5 score gate and the
  validator-service signer).
- **(c) Not already rewarded.** The `(bond_outpoint, epoch)` pair has
  not been rewarded by any block on the selected chain in `H`'s past.
  Enforced via a **composed `rewarded(bond,epoch)` set**, maintained
  exactly like `bond_view` (anchored at the pruning point, composed
  forward, reorg-reversible). Mirrors §A.5's
  `(bond_outpoint, validator_id, epoch)` scoring dedup, narrowed to
  `(bond_outpoint, epoch)` since the reward is per bond-epoch.

(a)+(b) are **structural**; (c) is **uniqueness**.

### B.4 Model B realised as a block-validity rule

- **Structural strictness (a,b):** a block `H` is **invalid** if any
  `StakeAttestationShard` tx it includes contains an attestation
  failing (a) or (b). Thus *every attestation in an included shard is
  structurally rewardable* → the coinbase fan-out needs **no skip
  set**, satisfying the Model-B goal. Cost: an ML-DSA-65 verify per
  included attestation at block validation, bounded by
  `max_attestations_per_block` (≤ 16) — acceptable.
- **Uniqueness (c):** a **duplicate** `(bond_outpoint, epoch)` does
  **not** invalidate the block; it is simply **not rewarded again**
  (no second output), matching §A.5's dedup-not-reject treatment. So
  re-including an already-rewarded attestation is allowed but earns
  nothing.
- **Block-template policy (PR-10.11):** the template builder must
  only select shards whose attestations are all (a)+(b)-eligible
  against the template's `bond_view`, and must skip
  already-rewarded `(bond,epoch)` for output emission — so the block
  it produces satisfies this rule by construction.

### B.5 Coinbase output construction = validation (byte-identical)

Validator reward outputs are **appended after** all existing coinbase
outputs (the mergeset-blue reward outputs + the optional red-reward
output — see `expected_coinbase_transaction`), in the canonical order
`(shard_tx_index_within_block, attestation_index_within_shard)`. For
each reward-eligible, not-yet-rewarded attestation:

```text
TransactionOutput {
    value: dns_params.reward_params.per_attestation_reward_sompi,
    script_public_key:
        p2pkh_mldsa65_spk(bond_view(H)[a.bond_outpoint].owner_reward_spk_payload),
}
```

bounded by the whole-output per-block cap
(`dns_finality::validator_reward_outputs`, PR-10.5′-a). Construction
(template) and validation (`verify_coinbase_transaction`) run the
**same** `bond_view`-based resolver over the **same** ordered
attestation list, so the coinbase is byte-for-byte reproducible on
every node. `verify_coinbase_transaction` must receive the full block
tx vector (today it is handed only `&txs[0]`) plus the selected-parent
`bond_view` + `rewarded` set.

### B.6 Gating + rollout

Everything in this addendum is inert unless `dns_params = Some(..)`
**and** `daa_score ≥ dns_activation_daa_score` (currently `u64::MAX`
on every network, incl. devnet). Below activation: no `bond_view`
materialisation is required, no reward outputs are emitted, and the
coinbase is byte-for-byte the pre-overlay coinbase. The pruning-point
bond-set/`rewarded`-set anchors are only built once a network
activates.

### B.7 Implementation order (PR-10.5′-b2 / b3)

1. **b2a** — materialise `bond_view` + `rewarded(bond,epoch)` set in
   `resolve_virtual`; thread the selected-parent view into
   `verify_expected_utxo_state`. Pure-ish; gated; no behaviour change
   (no consumer yet).
2. **b2b** — the B.4 structural block-validity rule
   (`RuleError::IneligibleAttestationInBlock` or similar), gated.
3. **b3** — coinbase fan-out: `expected_coinbase_transaction` appends
   `validator_reward_outputs` (B.5); update both real callers
   (`verify_coinbase_transaction` validation + `processor.rs` template
   build) byte-identically; update the PR-10.11 template policy
   (B.4). Gated.

All three only become live when a network sets a finite
`dns_activation_daa_score`; until then they are dead code paths
behind the `dns_params` + activation guard.
