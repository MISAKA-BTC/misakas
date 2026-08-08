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
- PoW/GHOSTDAG block production and ordering. The paper's §5 rounds are adopted as **overlay**
  rounds, not as a replacement consensus (see "Two rounds" below): canonical history is still
  decided by GHOSTDAG. There is no Proposal round because there is nothing to propose — the chain
  proposes, and the canonical lagged anchor is the proposal. What the overlay gained is the
  Prevote → Lock+Precommit pair that turns a single tally into an accountable commit.
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

Two fences, because turning the overlay on and handing it the vote are different risks:

| Fence | At and above it | Finality |
|---|---|---|
| `vlt_shadow_activation_daa_score` | certificates credited into `X_i(e)`, committees drawn, verdicts counted and paid the audit fee, settled challenges slashing, credit accumulator filling | unchanged — bonded stake, φS graded rule |
| `vlt_activation_daa_score` | `W_i(E) = min{C_i(E), λ·B_i(E)}` becomes voting weight; credit rule becomes `Q(E) = ⌊2W(E)/3⌋ + 1` | replaced |

Both are `u64::MAX` (dormant) on **every** shipped preset. Below the shadow fence the overlay
is byte-identical to its pre-VLT behaviour, so adopting this ADR is not by itself a consensus
change.

Each fence is its own coordinated hard fork. The shadow one moves coinbase value (the audit
fee) and slashes bonds, so it is a real fork — but its blast radius excludes finality, which is
the entire point of taking it first. The mesh produces and *polices* verified compute for a
while with nothing depending on the answer; only then does the second fork move the vote.

The interval between them is not slack, it is the **soak**. `C_i(E)` sums a
`credit_window_epochs` window, so flipping both at once switches voting power to a table that
is still empty: every `W_i(E)` is 0, `W(E)` is 0, no epoch reaches quorum, and DNS finality
stalls (the base ledger keeps advancing — the overlay is liveness-first). The paper's §2
"計算 bootstrap 期間" is this same caveat, and the split is what turns it from a scheduling
convention into a checkable property.

`DnsParams::vlt_params_consistent()` is the pre-flight check. It verifies the VLT knobs are
coherent (including shadow ≤ weight), that `unbonding_period_blocks` covers the §7 bound
`U ≥ credit window + max challenge period` (production's 14-day window covers the shipped
calibration with wide margin), that `vlt_credit_window_blue_score` spans
`(K + delay) × epoch_len + challenge_window + lag + backoff`, and that the soak spans that same
quantity — it is how long `C_i(E)` takes to mean anything, so it is both how far back the walk
must reach and how long the overlay must run before the vote may depend on it.

`update_dns_state` now consults it before entering `Active`, alongside
`dns_v3_params_consistent()`: a preset that would arm the reorg gate over a denominator that
has not filled stays in Bootstrap with the gate dormant. Trivially true on every shipped inert
preset, so no current network is affected.

The window requirement matters more than it looks: the credit walk resolves each certificate's
epoch against a canonical-anchor map built over the *same* window, and an epoch with no anchor is
skipped. A short window therefore does not fail loudly — it silently truncates the oldest epochs
of every validator's `C_i(E)` to zero and makes weight depend on an unrelated parameter. Both it
and the soak are asserted in both directions by
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
| Types, normalization, decay, weight, quorum, sortition, `VltEpochSnapshot` | `consensus/core/src/vlt.rs` |
| `W_i(E)` per bond, `W(E)` denominator, credit rule, credit folding | `consensus/core/src/dns_finality.rs` |
| Per-network params + fence | `consensus/core/src/config/params.rs` |
| Snapshot walk + its pin, weight-source switch, per-branch scoring | `consensus/src/pipeline/virtual_processor/processor.rs` |
| Prevote/precommit rounds, the lock chain, `PrecommitDuty` | `consensus/core/src/dns_finality.rs` + `processor.rs` |
| Stateless payload validation | `consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs` |
| Subnetwork ids `0x14`–`0x19` | `consensus/core/src/subnets.rs` |

The switch itself is one call — `DnsParams::epoch_credit_rule(daa_score)` — plus the matching
`ContributionWeight` selector, which is an explicit argument so every call site declares whether
it wants finality weight or stake-denominated inclusion policy.

`validator_voting_weight` is shared by the quorum's numerator and its denominator on purpose:
computing them separately would be a latent consensus split.

### Two rounds: prevote, then lock and precommit

One round of attestations is a tally, not a commit. It answers "did two thirds of the weight
approve anchor A for epoch E" and nothing about epoch E+1 — a validator may support A here and a
conflicting A' on another branch later, and no artefact anywhere makes that a provable fault.
Safety then rests on validators behaving rather than on their being unable to misbehave
undetected, which is the difference between a quorum and a BFT commit.

The attestation shard is now round 1, the **prevote**, unchanged on the wire. Round 2 is the
**precommit** (`SUBNETWORK_ID_STAKE_PRECOMMIT`, 0x19): signed only for an epoch whose prevote
quorum this chain already shows, and carrying `(locked_epoch, locked_hash)` — the lock the signer
held when it signed — inside the signed digest. An anchor is DNS-confirmed only once **both**
rounds clear quorum over the same pinned `W(E)`.

The declaration is what makes the lock mean something, and it is enforced twice:

- **On chain**, the walk counts a precommit only if its declared lock is exactly what this chain
  shows as that validator's previous counted precommit. The first misdeclaration drops that
  precommit *and every later one*, because the next declaration refers to a precommit that never
  counted. A validator cannot quietly forget a lock it published.
- **Across branches**, that restatement is self-contained evidence: two precommits naming one
  `locked_epoch` with different `locked_hash` prove the signer held two locks at one height, using
  only the two payloads — no reachability, no access to the losing branch's blocks.

So two conflicting anchors cannot both confirm without more than a third of the weight having
signed both, on chain, with the signature attached. `PrecommitDuty` is how a validator learns what
to sign: the lock comes from the **chain**, never from local memory, so a node that restarted,
resynced or was restored from a backup restates the lock everyone else already holds a signature
for rather than one its own state invented.

Fenced on the weight fence: below it `anchor_epoch_precommitted` is passed `true` and confirmation
is the single-round rule every current network runs today. Above the shadow fence but below the
weight fence the round is inert — validators have no lock to carry until their votes are
compute-weighted.

Not yet built: the lock-violation **slashing** path. The evidence is self-contained by
construction, but burning a bond for it needs its own evidence payload and reward, so today a
misdeclaring validator stops accumulating round-2 weight rather than losing its bond.

### The denominator is pinned, not per-branch

`Q(E) = ⌊2W(E)/3⌋ + 1` is a two-thirds threshold only if everyone arguing about epoch `E` divides
by the same `W(E)`. A branch that derives its own `W(E)` by walking its own chain does not: omit
the other side's certificates and `W(E)` falls, `Q(E)` falls with it, and the branch clears a bar
it set for itself. Two branches then each "reach quorum" for one epoch over disjoint validator
sets, which is exactly what §8.1 forbids — the intersection argument is the safety claim, and
without a shared denominator there is nothing to intersect.

So weights are read from a `VltEpochSnapshot`: the credit table plus the block it was taken at.
The reorg gate builds **one**, pinned at the selected-chain common ancestor of the two branches,
and hands the same one to both. Every DAA-stamped decision inside it — bond status, challenge
survival, epoch finalization — is taken at the pin, and the bond set is cut to what existed
there, so two branches derive a byte-identical table. A bond minted above the pin weighs zero on
both sides: admitted to the numerator alone it would manufacture quorums outright, since
`meets_bft_quorum` clamps signed weight up to the total. Pinning is also cheaper than what it
replaces — one walk from the ancestor instead of one per branch.

This subsumes `credit_delay_epochs`, which remains as the floor. The delay stops a fork from
weighting votes with VLT minted in the *same* epoch; the pin stops it from weighting votes with
VLT that exists only on itself, at any epoch distance (§8.3).

`update_dns_state` pins at its own sink, because that recompute has one chain in view — it scores
the selected chain rather than comparing it to anything. The comparison is the reorg gate, and
that is where the shared pin is load-bearing.

What is *not* pinned, and is inherited unchanged from ADR-0009/0018 rather than introduced here:
the denominator still filters by `is_bond_active_at` over the branch's own bond set, at the
branch's own canonical lagged anchor DAA. Bonds minted above the pin now weigh zero either way,
so the residue is narrower than it was — a bond that existed at the pin but was slashed or
unbonded above it, on one branch only, still moves that branch's `W(E)`. `W(E)` is therefore a
shared function of the pinned *compute*, and not yet of a shared validator set.

## Consequences

**Positive**

- Voting power tracks supplied, independently-verified compute, and decays when it stops.
- Buying stake buys no votes: `C_i = 0 ⇒ W_i = 0` regardless of bond.
- Slashing keeps its economic meaning: compute beyond `λ·B_i` is discarded, so weight is always
  backed by collateral that can be burned.
- The BFT quorum is exact and strictly above two thirds, restoring the §8.1
  quorum-intersection argument that a graded credit could not support.
- Finality is a two-round commit with an accountable lock, so a validator that supports
  conflicting anchors leaves signed evidence of having done so instead of merely being suspected
  of it.
- Existing networks are untouched until a fence moves.

**Negative / open**

- **Walk cost.** The credit walk scans `vlt_credit_window_blue_score` (10 400 on production,
  vs. 1 500 for the attestation walk) per recompute. It is skipped entirely while the fence is
  inert. A persisted per-epoch credit accumulator is the obvious optimization if activation is
  scheduled.
- **The pin costs weight at depth, and `W_min` is the part that notices.** A snapshot pinned at
  the common ancestor holds no epoch above it, so for an epoch `E` far above the fork the newest
  — and, under geometric decay, heaviest — terms of `C_i(E)` are simply absent. Every validator
  loses them equally, so the quorum *ratio* is untouched; `min_network_compute` is the one test
  that is not scale-invariant, and a deep-divergence comparison can shrink `W(E)` under it and
  earn nothing for the epochs nearest the tips. The effect grows with divergence and is bounded
  by `dns_gate_horizon_blocks`, it is symmetric between the two branches, and it fails toward
  `DominanceViolation` — keeping the canonical chain — with the work override and the
  `dns_veto_ttl_daa_score` release still available as liveness escapes. Calibrating `W_min`
  against the gate horizon, rather than against the healthy-network total alone, is part of any
  activation plan.
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
