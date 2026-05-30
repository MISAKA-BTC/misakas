# ADR-0017: All-Active-Staker Attestation (Remove Sortition Committee)

Status: Accepted (Phase 13 design reversal; supersedes ADR-0012)
Date: 2026-05-30
Supersedes:
  - [ADR-0012](0012-mainnet-validator-sortition-commit-reveal.md)
    (Mainnet Validator Sortition via On-Chain Commit-Reveal) — **in full**.
  - [ADR-0009](0009-dns-probabilistic-finality.md) §"Validator selection
    (sortition)" mainnet pointer (the "commit-reveal or PQ-VRF, decided in a
    follow-up ADR" clause). The PoC deterministic-sortition note is also retired.
Depends on:
  - [ADR-0008](0008-hash64-consensus-identity.md) (Hash64 validator identity)
  - [ADR-0009](0009-dns-probabilistic-finality.md) (DNS overlay; attestation
    target message layout + `validator_set_commitment`)
  - [ADR-0010](0010-validator-node-architecture.md) (validator service;
    `is_eligible_this_epoch` becomes `is_active_validator`)
  - [ADR-0011](0011-validator-deployment-and-equivocation-safety.md)
    (`SignedEpochRecord` per epoch — unchanged; one signed attestation per epoch)
  - [ADR-0016](0016-stake-locked-bond-utxos.md) (stake-locked bond UTXOs — the
    bond-active predicate this ADR reuses as the sole eligibility gate)

## Context

[ADR-0009 §"Validator selection (sortition)"](0009-dns-probabilistic-finality.md)
left mainnet validator-set selection to a follow-up ADR, requiring random
stake-proportional tickets and forbidding reuse of the PoC deterministic-
sortition scheme. [ADR-0012](0012-mainnet-validator-sortition-commit-reveal.md)
froze that follow-up as on-chain **commit-reveal sortition** selecting a per-epoch
committee subset (`committee_E ⊆ active_validators`, `committee_size` validators
chosen by stake-weighted priority over a commit-reveal seed).

**This ADR reverses that decision.** The validator participation model is
**stake → participate**: every validator holding an *active stake bond* attests
every epoch. There is no per-epoch committee subset, therefore no sortition,
therefore no commit-reveal randomness pipeline.

### Why the reversal is safe — the committee was never a consensus input

Inspection of the *implemented* Phase 10/11 pipeline established that committee
selection never entered a block-validity rule:

- **The VSC is self-declared.** Every consensus attestation-verification site
  (`virtual_processor/processor.rs:888`, `virtual_processor/utxo_validation.rs:756`
  and `:801`) verifies the ML-DSA-65 signature over a message containing the
  attestation's **own** `validator_set_commitment` field. Consensus never
  recomputes a canonical committee VSC and compares it. `validator_set_commitment()`
  is invoked only at its definition and inside `get_validator_attestation_target`
  (a *read API* serving the validator service).
- **Reward eligibility** (ADR-0009 Addendum B / ADR-0013) checks bond-active +
  valid signature — *not* committee membership.
- **Slashing genuineness** (ADR-0013 Addendum C / ADR-0016 §D.4) checks bond
  resolution + signature verification — *not* committee membership.
- **StakeScore aggregation** already sums every active-bond attestation; it was
  never committee-filtered.

So the committee existed only in (i) the `get_validator_committee` /
`get_validator_attestation_target` read APIs and (ii) the validator service's
`in_committee` eligibility gate. Removing it touches **no consensus verification
rule** → low chain-split risk.

## Decision

### D.1 — Eligibility = active bond

A validator is eligible to attest in epoch `E` **iff** its `StakeBond` is active
at `epoch_start(E)` per the existing `effective_bond_status` / `is_bond_active_at`
predicate (ADR-0016). No sampling; no per-epoch subset; no commit window.

### D.2 — Attestation set = all active validators

Every active bonded validator may submit one `StakeAttestationShard` per epoch.
The StakeScore accumulator sums all *valid* (bond-active ∧ signature-valid)
attestations, stake-weighted, toward the finality threshold — **unchanged** from
today (it was already bond-based, not committee-filtered).

### D.3 — VSC scope = full active set

`validator_set_commitment(epoch, active_records)` is computed over the **full
active validator set** (was: the committee subset). It remains a self-declared
field bound into the signed attestation message. The
[ADR-0009 §"Attestation target"](0009-dns-probabilistic-finality.md) message
layout is **UNCHANGED** (`BLAKE2b-256` over the same tuple; VSC still included at
the same position). Consensus continues to verify only the ML-DSA-65 signature
over the attester's *declared* VSC — there is deliberately **no** canonical-VSC
recomputation rule (identical to today; preserves the A.2-style statelessness
that keeps shard-tx admission a non-chain-split surface).

### D.4 — Removed surface (clean removal)

None of the following was ever a block-validity input; all of it is removed:

- `SortitionMode`, `select_committee`, `select_committee_for_epoch`,
  `compute_validator_priority`
- `derive_epoch_seed_deterministic` / `_commit_reveal` / `_fallback`, `compute_commit`
- `SORTITION_{COMMIT,SEED,FALLBACK,PRIORITY,DETERMINISTIC}_KEY` consts
- `SortitionCommitPayload`, `SortitionRevealPayload`, `UnrevealSlashingEvidencePayload`
- subnetworks `SUBNETWORK_ID_{SORTITION_COMMIT,SORTITION_REVEAL,UNREVEAL_SLASHING_EVIDENCE}`
  (`0x13`/`0x14`/`0x15`) + their `DnsTxKind` variants + `dns_tx_kind` mapping +
  the three stateless payload validators + their isolation-validator dispatch arms
- `DnsParams` fields: `sortition_mode`, `committee_size`, `commit_window_blocks`,
  `reveal_window_blocks`, `min_reveal_threshold_num`, `min_reveal_threshold_denom`,
  `commit_reveal_lookahead_epochs`, `commit_without_reveal_slash_sompi`,
  `unreveal_reporter_reward_sompi`, `commit_reveal_activation_daa_score`
- commit `2293898` (the "routing + stateless" Slice 1) is undone as part of this.

The subnetwork bytes `0x13`–`0x15` return to the free pool. (`epoch_length_blocks`
is **kept** — it defines epochs generally, independent of sortition.)

### D.5 — Validator service & RPC

- Eligibility gate: `in_committee` → `is_active_validator` (bond active).
- Read API: `get_validator_committee` → `get_active_validator_set` (returns the
  active set; no selection). Struct `ValidatorCommittee` → `ActiveValidatorSet`.
- `get_validator_attestation_target` computes the VSC over the full active set (D.3).
- RPC `getValidatorStatus`: the `in_committee` field → `is_active_validator`
  (wRPC + gRPC).

## Consequences

### Positive

- Eliminates the entire commit-reveal sortition pipeline — the hardest remaining
  chain-split-critical work (stateful commit/reveal windowing, per-`(vid,epoch)`
  uniqueness, reveal-matching, on-chain seed derivation, unreveal-slashing
  side-effect) is **removed, not implemented**.
- Consensus becomes *more* consistent: StakeScore already counted all active-bond
  attestations, while the committee gated who *should* attest — that latent
  mismatch disappears.
- The bond-grinding / anchor-targeting / last-minute-withdrawal attacks ADR-0012
  was built to counter become **moot** — there is no selection to bias.

### Negative / open

- **Attestation cost is O(N) in the active validator count.** Each active
  validator's ML-DSA-65 signature (~3.3 KB) is gossiped and may be block-included
  every epoch. Mitigations (future, out of scope here): the existing per-block
  attestation budget (PR-10.11 mining policy), signature aggregation, or a hard
  cap on attestations/epoch. **Must be revisited before mainnet scale.**
- **Finality security model shifts (REQUIRED follow-up).** ADR-0009's Poisson-DNS
  theorem is stated for *random stake-proportional tickets* (sampling); it
  explicitly notes a fixed set without random sampling "does **not** satisfy the
  theorem's assumptions." Full participation is the degenerate "sample = whole
  population" case — it removes sampling-based attack surface but is no longer the
  sampling regime the theorem was stated for. Full-participation stake-weighted
  finality is a well-understood alternative (Casper / Tendermint family), but
  ADR-0009's theorem citation **no longer directly applies** and the finality
  argument must be restated for the full-participation regime. Flagged as a
  required security note; **not blocking** this implementation, which stays
  gated/inert below the activation DAA score.

## Implementation plan (gated slices; no consensus verification-rule change)

- **Slice A** — remove the dead ADR-0012 wire surface: undo `2293898`, drop the
  3 subnetworks + `DnsTxKind` variants + `dns_tx_kind` arms + stateless validators
  + commit-reveal payloads + commit-reveal `DnsParams` fields + the `SORTITION_*_KEY`
  consts. consensus-core + isolation-validator tests updated. Gated/inert.
- **Slice B** — remove the sortition logic (`SortitionMode`, `select_committee*`,
  `compute_validator_priority`, `derive_epoch_seed_*`); rewire
  `get_validator_committee` → active-set read and `get_validator_attestation_target`
  VSC over the full active set. Consensus *reads* only; no verification-rule change.
- **Slice C** — validator service + RPC rename: `in_committee` → `is_active_validator`,
  `ValidatorCommittee` → `ActiveValidatorSet`; wRPC + gRPC field rename.

Each slice compiles + keeps tests green; none alters a block-validity rule. The
attestation/reward/slashing consensus rules (ADR-0009 Addendum B, ADR-0013,
ADR-0016) are unchanged.
