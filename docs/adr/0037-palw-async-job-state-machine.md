# ADR-0037: PALW off the block-critical path — an asynchronous, budgeted job state machine over a permanent hash floor

Status: **Superseded in part by ADR-0038 (same day).** Decision 1 (hash-PoW as the primary
consensus work; PALW never block-critical) is **reversed** by ADR-0038 — it secured the chain by
making the chain's thesis optional. Decisions 2–9 (state machine, binding, panels, court seating,
classes, mint hygiene, P_check exclusion, registry/freeze) are **carried forward** into ADR-0038
Decision G and remain normative through it. Read ADR-0038 first.

Original status: Accepted (architecture decision). Activates nothing today; binds every future
value-bearing PALW deployment. Adopted from the 2026-08-17 design review that cross-examined the
2026-08-16/17 mainnet-readiness audit (9 blockers, 15 high; the consumer layer, not the arithmetic,
is what fails) against Ambient's published auction/escrow structure.

Date: 2026-08-17
Supersedes: **ADR-0028's mainnet mechanism** (the block-coupled challenge-window walk as the
*mainnet* credit mechanism — the window walk remains the *testnet soak* mechanism and Stage-0→3
instrument). Amends **ADR-0036 Decision 4** only by adding an exit criterion to the TN11/devnet
single-algo exemption (below). Everything else in ADR-0036 (lineage, namespace, "new identity
required", land→accept→mint separation) stands and is assumed here.
Relates to: ADR-0021 (algo-4 PoW — retired from the value path by this ADR), ADR-0026 (Ambient
survey: borrow-architecture/strengthen-proof — this ADR is that borrowing, made precise),
ADR-0027 (no-BFT unilateral fraud proofs — unchanged, becomes Layer 3's admission rule),
ADR-0030–0033 (the arithmetic court and credit gate — unchanged, re-seated as Layer 3 and the
Layer-2 consumer), ADR-0034 (routing — unchanged), ADR-0035 (public testnet — unchanged).

## Context

Three measured facts force this ADR:

1. **The audit's NO-GO is about the consumer layer, not the arithmetic.** The exact-bit court
   (step space → leg → refutation → bisect → one-primitive CPU adjudication, SoftFloat second
   implementation, `Unadjudicable` three-way verdict) survived adversarial review. What failed is
   everything that connects arithmetic results to chain state and mint: fail-open lookups,
   payee-by-pubkey-hash, unbounded coinbase append, history re-scans, and `panic!` on runtime
   absence.

2. **PALW today is block-critical.** `required_algo_id` returns one mandatory id and
   `check_algo_id` rejects every other (`consensus/core/src/pow_layer0.rs`); there is no
   mixed-algo difficulty arithmetic; `calc_block_level_check_pow_layer0` panics on
   `PalwUnavailable` (`consensus/pow/src/lib.rs`). A single inference-runtime fault is a
   chain-liveness fault. ADR-0036 Decision 4 already ruled the mainnet identity MUST ship a
   permanent hash floor; this ADR supplies the architecture that makes the floor natural rather
   than grafted.

3. **PALW evidence is reconstructed, not stored.** `compute_palw_credit_outputs`
   (`consensus/src/pipeline/virtual_processor/processor.rs`) re-walks the selected-parent chain
   across the whole challenge horizon on every decided block, re-discovering commitments,
   attestations and refutations from raw carriage records. That couples PALW timing to block
   cadence (the reason the audit found 10 BPS "structurally impossible"), makes every consumer
   re-derive facts (each re-derivation is a fresh fail-open opportunity), and cannot express
   escrow, deadlines, claims, or dispute state at all.

What Ambient demonstrates and MISAKA adopts is *structural*, not cryptographic: one state object
per job (escrow, fixed payees, deadlines, claim bitmap, selected verifiers), monotone status
transitions, verification asynchronous to finality, bonded disputes with replacement panels.
What MISAKA deliberately does **not** adopt: PALW as the sole block PoW, q-of-n committees as
final truth, tolerance-based comparison as a slash basis, service/admin per-job overrides, and
self-declared capacity in the safety argument.

## Decision 1 — Three layers; PALW is never block-critical on a value-bearing network

```
Layer 1  Permanent hash-PoW BlockDAG      — validity, ordering, difficulty, liveness
Layer 2  Async PALW job/credit machine    — inference, escrow, panels, rewards
Layer 3  Deterministic dispute court      — bisection to one CPU-adjudicated primitive
```

Block validity on every value-bearing network is exactly:

```
valid_block = valid_header ∧ valid_hash_pow ∧ valid_transactions ∧ valid_state_transition
```

`PALW_runtime_available` and `PALW_full_inference_matches` are **not** conjuncts. Exclusive
`algo_id = 4` PoW is retired from the value path: PALW commitments ride as ordinary transactions
(ADR-0029 carriage) and/or a coinbase-extension receipt root. Under any PALW failure — worker
crash, missing GGUF, missing accelerator, frozen class — the outcome is `PALW credit = 0`,
block validity unchanged, hash chain continues. (Invariants I1, I2, I12.)

**TN11/devnet exemption, now with an exit.** ADR-0036 Decision 4's choice — soak networks keep
single-algo PALW because a loud halt beats a silent fork *while PALW is the object under
observation* — stands, with this added criterion: the exemption is licensed only while the
network is valueless and `palw_credit`-inert as a *PoW soak instrument*. The moment a network
carries value, third-party bonds, or nonzero PALW emission, it must be (re)born on the Layer-1/2/3
shape above. There is no upgrade path from single-algo to floored on a running identity — that is
a re-genesis, which is what ADR-0036 Decision 2 already requires for mainnet.

## Decision 2 — Jobs are state, not history

The per-block horizon walk of `compute_palw_credit_outputs` is retired for value networks.
Each job is a pruning-surviving consensus state object:

```rust
struct PalwJobStateV3 {
    job_id: Hash32,
    job_context_hash: Hash32,
    model_band_id: Hash32,
    execution_class_id: Hash32,
    requester_outpoint: Outpoint,
    executor_bond_outpoint: Outpoint,
    executor_pubkey: Mldsa87PublicKey,
    commitment_root: Hash32,
    trace_root: Hash32,
    output_root: Hash32,
    commitment_anchor_hash: Hash32,
    eligible_set_snapshot_id: Hash32,
    selected_verifier_bond_outpoints: Vec<Outpoint>,
    status: PalwJobStatus,
    deadlines: PalwDeadlines,
    user_escrow_amount: Sompi,
    max_inflation_credit: Sompi,
    verdict: PalwVerdict,
    reward_claimed_bitmap: u32,
}
```

Transitions are monotone and closed:

```
Open → Committed → PanelSelected → ProvisionalAccepted | ProvisionalRejected
     → ChallengeWindow ─ no dispute → FinalizedAccepted | FinalizedRejected
                       └ dispute   → Disputed → Adjudicating
                                                 → Convicted | NoFaultFound | Unadjudicable
```

Three rules the audit found violated, now structural:

* **A well-formed refutation locks; it never deletes.** A refutation transaction opens dispute
  state and freezes the reward; it is not an erase instruction. (I8)
* **Only exact primitive conviction or an objective no-show/deadline violation can destroy a
  reward or bond.** (I9) Committee disagreement alone cannot.
* **`Unadjudicable` slashes no one** — not the miner, not the panel, not (by default) the
  challenger. It zeroes the job's inflation credit, refunds escrow by the predetermined rule,
  and puts the execution class on the auto-freeze path, because reaching `Unadjudicable` proves
  the class's catalog-completeness claim was false. Re-activation requires a new class version
  and re-audit. Chain liveness is untouched. (I10)

## Decision 3 — Identity and signatures are fully bound, verified at every consumer entry

```
job_id = H("MISAKA/PALW/JOB/V3" || network_id || request_txid || request_output_index
           || requester_nonce || model_band_id)
```

One commitment per `job_id`, first-accepted-wins, enforced in consensus state — `committed_root`
alone is not an identity (it cannot distinguish same-root distinct jobs, duplicated carriage,
replay). Commitment and attestation signatures (ML-DSA-87) bind the full context:

```
commit_message  = H("MISAKA/PALW/COMMIT/V3" || network_id || job_id || job_context_hash
                    || execution_class_id || executor_bond_outpoint
                    || commitment_root || trace_root || output_root)
attest_message  = H("MISAKA/PALW/ATTEST/V3" || network_id || job_id || job_context_hash
                    || execution_class_id || verifier_bond_outpoint
                    || sample_indices || observed_roots || verdict)
```

Signatures are verified at carriage admission **and re-checked (or provably cache-carried) at the
credit-consumer entry**. "Another layer will verify it" is not an accepted design state — that
phrase is what produced the audit's fail-opens. (I5)

## Decision 4 — Panel selection: future anchor, real snapshot, dual deadline

`select_replay_panel_v1` (`consensus/core/src/palw_schedule.rs`) is kept; its *inputs* are fixed.
The caller may no longer hardcode eligibility:

```
panel_seed = H("MISAKA/PALW/PANEL/V3" || network_id || job_id || commitment_root
               || future_anchor_block_hash || eligible_set_snapshot_root)
```

The anchor is a block finalized *after* the commitment; the eligible set is the snapshot at the
anchor; candidates must be `Active` bonds of the exact `execution_class_id`, not frozen, not the
executor, deduplicated by `bond_outpoint` and (best-effort) operator root; `Pending / Unbonding /
Slashed` are excluded. Deadlines are dual, so one saturated mergeset cannot evaporate a replay
window:

```
action_allowed = current_daa ≥ anchor_daa + min_daa_delta
               ∧ past_median_time ≥ anchor_mtp + min_seconds
```

## Decision 5 — Sampled verification is the fast path, never the final ruling

Fast path (adopted from Ambient, role-limited): future randomness picks sample positions; q-of-n
same-class validators recompute; agreement yields `ProvisionalAccepted`. Initial shape
(simulation subject, not a mainnet constant): n=3/q=2 ordinary jobs, n=5/q=3 large jobs. An
attestation carries `job_id`, class id, sample indices, sampled checkpoint roots, observed
output/token roots, verifier bond outpoint, signature — never a bare success count.

Slow path (MISAKA's differentiator, already landed as ADR-0030–0033 machinery): on root
disagreement only, bisect token checkpoint → layer checkpoint → step-leg → kernel invocation →
primitive. The full node's final ruling needs no model and no GPU:

```
verify Merkle proofs → execute one bounded primitive on CPU (vendored SoftFloat / fixed
integer semantics, no host libm) → compare exact bits
```

For every *active* class, catalog coverage of reachable kernels must be 100% — 90% coverage is an
invitation for the remaining 10%. `Unadjudicable`-on-gap is the enforcement (Decision 2).

## Decision 6 — Two-tier hardware taxonomy; classes qualify by calibration, not self-declaration

User-facing discovery uses four bands: `CPU / METAL / CUDA / ROCM`. Consensus uses neither the
band nor "same backend" — it uses an exact environment hash:

```
ExecutionClassId = H(model_band_id || backend_family || weights_root || tokenizer_root
    || runtime_source_commit || runtime_binary_hash || compiler_id || compiler_flags
    || kernel_plan_root || math_profile || libm_build_id || driver_runtime_profile
    || fma_mode || ftz_daz_mode || quantization_profile || shape_profile
    || catalog_root || reference_arithmetic_version)
```

`ModelBandId` (what quality the requester buys) and `ExecutionClassId` (what environment
consensus can adjudicate) are distinct fields and both appear in the execution receipt. Validator
onboarding is `Unregistered → bond → Probation → deterministic calibration jobs → Qualified →
activation delay → Active`; only calibration-passing validators enter panels. **Cross-class
results are telemetry, never a slash basis** (I11) — this generalizes the measured
CPU-class rule ("cross-class refutes an honest receipt") into policy. Exact cross-class
compatibility, if ever demonstrated, arrives as a distinct `CompatibilityGroupId`, not as an
assumption.

## Decision 7 — Mint is a carve of scheduled subsidy, never an append

```
total_coinbase_outputs ≤ scheduled_block_subsidy + transaction_fees          (I6, I15)
palw_reward_block ≤ palw_block_budget      palw_reward_epoch ≤ palw_epoch_budget
hash_reward_block ≥ permanent_hash_reward_floor
```

`compute_palw_credit_outputs` is re-shaped from an unbounded `Vec<TransactionOutput>` producer
into a budgeted, deterministic batch:

```rust
fn compute_palw_credit_outputs(credit_index: &PalwFinalizedCreditIndex, block_budget: Sompi,
    max_outputs: usize, current_daa: u64) -> Result<PalwCreditBatch, PalwCreditError>;

struct PalwCreditBatch { outputs: Vec<TransactionOutput>,
    consumed_credit_ids: Vec<Hash32>, consumed_budget: Sompi }
```

Credit records are consumed in the pinned order `(finalized_daa, job_id)` — prefix-mandatory up
to `max_outputs`/budget, so miners cannot censor or reorder payees without producing an invalid
coinbase. Payees resolve **only** from the exact payout script recorded on
`executor_bond_outpoint` / `verifier_bond_outpoint` (the B14 rule, now universal — never a
`validator_pubkey_hash` lookup). (I3, I4)

Three pools never mix: **user escrow** (paid from ordinary UTXOs; winner + panel + refund + fee ≤
escrow), **PALW inflation credit** (block/epoch budget within scheduled emission), **slash bonds**
(compensation, challenger bounty, burn). One commingled "PALW reward wallet" is how a system
loses the ability to explain its own balances.

## Decision 8 — `P_check` and self-declared capacity are out of the safety argument

Whether a validator really replayed is unobservable in telemetry, so no safety inequality may
contain a telemetry-derived `P_check`. The binding limits are bond-exposure caps, enforced in
consensus state:

```
outstanding_executor_credit ≤ executor_bond × executor_leverage_limit
outstanding_attested_credit ≤ verifier_bond × verifier_leverage_limit
```

plus per-class rate state (`last_credited_daa_by_class`, `credited_amount_this_epoch`,
`active_unfinalized_exposure`) actually checked at credit time. Per-identity limits are fairness
aids only — Sybil-splittable — so **the real valve is the global epoch budget** (Decision 7).
This is the structural fix for the measured `max_leverage` 11,655× violation.

## Decision 9 — On-chain class registry; freeze halts credit, not the chain; no per-job override

`PalwExecutionClassState` (status ∈ `Inactive/Probation/Active/Frozen/Deprecated`, manifest and
artifact roots, committee shape, `activation_epoch`, `freeze_reason`) lives in pruning-surviving
chain state, not compile-time fork parameters. `class_frozen` is consulted on every path: job
admission, commitment, panel selection, attestation admission, provisional finalization, credit
generation. Freeze semantics: new jobs rejected, new credit halted, existing disputes continue,
base chain continues. Governance may freeze a class; **it may not touch an individual job's
verdict or payout** (I13) — Ambient's service-finalize override is explicitly not imported.

Artifacts (weights, tokenizer, template, runtime binary+source, compiler+flags, kernel catalog,
math/driver profile, reference arithmetic) are content-hash-pinned and re-derived from actual
bytes at startup; a manifest mismatch refuses the class. Self-attested flags
(`libm_transcribed: true` style) and CWD-cache shortcuts are banned — the B8 `libm_arithmetic_digest`
and the B15 always-recompute GGUF gate (both landed 2026-08-17) are the pattern.

## Decision 10 — Reconciliation and the 10 BPS question

* ADR-0028's ladder (Stage 0→3) remains the *qualification process*; its block-coupled window
  walk remains the *soak instrument*. Its **mainnet mechanism** is superseded by Decisions 2–9.
* ADR-0036's frame (live lineage governs; new identity required; land→accept→mint; floor binds
  mainnet) is unchanged; its testnet exemption gains the Decision-1 exit criterion.
* Because Layer 2 is asynchronous, `W_challenge`, payout finality, BlockDAG finality, and block
  interval become **independent parameters**. The audit's "10 BPS is structurally impossible"
  conclusion applied to the block-coupled design; it is *re-opened, not resolved*, for this
  architecture. The mainnet-parameter ADR must re-derive all four separately — copying any
  existing preset into the new identity is forbidden.

## Activation stages (value identity)

```
M0 Hash-only            parsing on, classes inactive, PALW cap 0, consensus influence 0
M1 User-escrow only     escrow jobs + committee verification; inflation 0; ordering impact 0
M2 Capped PALW credit   after the full drill list*; tiny epoch cap; hash floor reward intact
M3 Consensus weight     separate ADR; capped; PALW weight can never override the hash chain
```

*M2 preconditions: signature-forgery, duplicate-credit, budget-invariant, pruned-IBD,
reorg-determinism, worker-crash, class-freeze drill, bonded-dispute drill, exact primitive
conviction, `Unadjudicable`-no-slash, third-party bonded operators, external review.
**Initial mainnet stops at M2.** Rewarding useful work and dying when useful work fails are
different properties; the second is an uncompensated liability.

## Invariants (release-blocking; reviews check these, not features)

```
I1  Hash-PoW blocks build and validate with zero PALW
I2  PALW runtime failure never causes panic / fork / block rejection
I3  A job credits at most once
I4  Every payee resolves from an exact bond_outpoint script
I5  Every PALW signature binds network, job, context, class, bond
I6  Payouts never exceed escrow or emission budget
I7  Missing/pruned data is never treated as empty
I8  A well-formed refutation alone never destroys credit
I9  Slash requires exact conviction or objective no-show
I10 Unadjudicable slashes no one
I11 Class mismatch is never a slash basis
I12 Class freeze never halts the hash chain
I13 Governance cannot alter an individual verdict or payout
I14 State root is identical across reorg, IBD, and pruning
I15 Total emission never exceeds the public schedule
```

## Implementation order and current status (2026-08-17)

* **Track A — this ADR.** Done on signing: ADR-0028 mainnet mechanism superseded; PALW declared
  non-block-critical for value networks; genesis PALW cap 0; per-job override prohibited; class
  activation conditions written (Decision 9, M-stages).
* **Track B — liveness (status against the audit):** `PalwWorkerFailed` bounded retry, B15
  always-recompute GGUF gate, B8 libm digest — **landed/in-flight 2026-08-17** (ADR-0036
  Consequences). Remaining: the hash floor itself and runtime-absent block-validation tests —
  these land **with the value identity's genesis** (Decision 1), not as a retrofit on soak nets.
* **Track C — state machine + mint: one change set.** `PalwJobStateV3`, consumer-entry signature
  checks, exact-outpoint payees, `job_id` dedup, future-anchor panels, `job_context_hash`
  binding, on-chain rate state, block/epoch budgets, `PalwCreditBatch`, bonded dispute state,
  freeze state. Partial activation is forbidden — landing B1 without B3 (or B2 without B5)
  preserves a fail-open with better paperwork.
* **Track D — the court (longest path):** step-leg capture → capture neutrality → quant
  GEMV/GEMM, SoftMax, RoPE, GDN catalogs → `ExecutionStepRefutation` carriage → bisection state
  machine → primitive CPU adjudication. Much is landed (ADR-0030–0033); the gate to M2 is 100%
  reachable-kernel coverage per active class.

## What this ADR does not decide

Mainnet parameters (ADR-0036's "does not decide" stands, now including the four decoupled
timing parameters); concrete n/q, bonds, leverage limits, budgets (soak/simulation outputs);
whether/when TN11 or devnet is re-cut onto the three-layer shape (a soak-planning call, licensed
any time by Decision 1's exit rule); M3.

## Summary

```
Ambient's escrow + state machine + async validation + bonded dispute
+ MISAKA's permanent hash floor + exact-bit arithmetic + SoftFloat reference
  + primitive adjudication + Unadjudicable safety
= a hash-secured BlockDAG with asynchronously verified useful AI work
```

The near-term engineering decision is therefore **not** to improve algo-4 but to take it off the
block-critical path and move PALW into the budgeted asynchronous machine above. Polishing the
arithmetic first would perfect a precision instrument and then detonate it with a HashMap and an
unbounded coinbase append.
