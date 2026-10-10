# The kernel route: K2-TIR-v1/v2/v3 (ADR-0172 / versioned-kernels.md §§K.0–K.8 / RFC-0011 §§15–16 / RFC-0004 §0)

Status: reference implementation in `misaka-palw-kernel`, **dormant**. The consensus fence `palw_probabilistic_constraints_v1`
exists with **no activation height** (`None` on every preset, Some-only in both fingerprints, refused when armed). The shipped
schedule lists K2-TIR-v1, v2 and v3 as `Implemented`; every registration checked against it returns `KERNEL_NOT_ACTIVE`.
This document is the completion matrix of the RFCs' implementable items; it is not a soundness review and not an activation proposal.

Precedence: RFC-0004/0011, the [Kernel design](versioned-kernels.md), ADR-0172 and ADR-0173 are the design of record for this route. Where the existing network code says otherwise, the crate follows the RFC and the difference is listed in §4.

## 1. The kernels

| Descriptor | Families (checker / court) | What it adds |
| --- | --- | --- |
| K2-TIR-v1 | structure, exact-arithmetic, quant-range, nonlinear, selection (exact recompute / instance); dense-matrix (Freivalds over GF(2^127−1) / one scalar); recurrent-state (state continuity by wiring / instance) | every PALW-TIR v1 primitive up to an `i64` accumulator |
| K2-TIR-v2 | v1, dense-matrix by **multi-modulus Freivalds** (2^127−1, 2^107−1, 2^89−1, the fewest whose product exceeds the error span; CRT) | `i128` accumulators; per-repetition bound that of 2^89−1 |
| K2-TIR-v3 | v2 + **media-pipeline** (exact edge recompute / edge court) | RFC-0003 pipelines of TIR v2 programs: text-to-image, vision encoders, vision-language text streams, encoder–decoders, RFC-0004 evaluation pipelines |

Every descriptor is a new digest; classes bind one; v1, v2 and v3 coexist on one schedule; a claim, evidence object or receipt under
one is never judged by another.

## 2. Completion matrix

`✔` implemented and tested in this crate; `◐` implemented as far as this repository can, the remainder named; `✖` not implementable
here (an external gate: review, measurement, real-chain drill, activation).

### versioned-kernels.md §§K.0–K.8

| Item | Status | Where / evidence |
| --- | --- | --- |
| K.0 no BVM/GVM/VM fallback; unknown ids never success | ✔ | no interpreter/ISA/plugin; unknown descriptor → `KERNEL_NOT_ACTIVE`; unknown checker id does not parse (`k2_adversarial`) |
| K.1 extension ladder (new artifact/plan inside bounds = ordinary registration; missing family = extension) | ✔ | `check_plan_v1`, `KERNEL_EXTENSION_REQUIRED` naming the family; v1→v2→v3 are the ladder exercised |
| K.2 `KernelDescriptorV1`, `ModelKernelBindingV1`, new class-id domain | ✔ | `descriptor.rs` |
| K.3 dense/sparse matrix relations | ✔ | Freivalds (v1), multi-modulus (v2), batched across a scope's tokens (RFC-0007 §V.3) |
| K.3 quantization/range/rounding/nonlinear, exact integer semantics, carries/overflow | ✔ | exact recompute; **the exact-result rule**: a program whose ranges are not proven is `FRONTEND_REQUIRED`; the MatMul court recomputes under the reference partial-sum rule |
| K.3 attention/MoE/selection (TopK ties, routing, gather) | ✔ | exact recompute; swapped expert, TopK, Gather lies convicted |
| K.3 recurrent/dynamic state (init, read/write order, continuity, full context) | ✔ | wiring; forged history window, permuted window, fabricated segment entry refused |
| K.3 media and pipelines | ✔ | `pipeline.rs` (K2-TIR-v3); traces equal the reference pipeline run; stage, edge and `R` lies convicted |
| K.3 encoding/aggregation (GKR) | ◐ | direct relations only (no aggregation, so no encoding binding is needed); a GKR family would be a new descriptor with a reviewed transcript |
| K.4 step 2 deterministic static admission (grammar, activation, coverage, integer semantics, worst DA/court budgets) | ✔ | `check_plan_v1`, `check_pipeline_plan_v1` |
| K.4 step 3 bind first, then challenge; no adaptive reuse | ✔ | `challenge.rs`, `beacon.rs` (finalized window after the commitments, counted rebinds, grinding as attempts) |
| K.4 step 4 Panel checks relations; receipts; Final needs receipts, DA, window, no dispute | ✔ | `receipt.rs`, `lifecycle.rs` |
| K.4 step 5 localization to a named primitive; exact terminal court; DA ≠ fraud | ✔ | `verify_fault_proof_v1`, `verify_edge_fault_v1`, `verify_stage_fault_v1`; `Unavailable` everywhere material is missing |
| K.5 versioning, append-only meaning, deprecation stops new work only, activation boundary | ✔ | `KernelScheduleV1`; `k2_wide` coexistence/boundary tests |
| K.5 coordinated fence; unknown kernel fails closed | ✔ / ✖ | `palw_probabilistic_constraints_v1` dormant and refused when armed; activation is a separate coordinated upgrade |
| K.6 RFC04 independent of a VM; promotion test ≠ correctness | ✔ | `improve.rs` |
| K.7 release evidence: mixed-kernel replay, unknown-version fail-closed, forged/omitted constraints, wrong field encodings (alias), one-point faults, bad routing/memory/boundaries, adaptive transcripts, unavailable data, bounded disputes | ✔ | `k2_e2e`, `k2_wide`, `k2_adversarial`, `k2_pipeline`, `k2_public` |
| K.7 valid large claims to Final with small checks; real 9B / long-context cost reports | ✖ | needs real weights, hardware and a chain; the plan's budgets and the SDK preflight's shape-only route are the inputs |
| K.7 independent soundness review | ✖ | external |

### RFC-0011 §§15–16

| Item | Status | Where / evidence |
| --- | --- | --- |
| §15.1 probabilistic acceptance of the complete suite; audits never coverage | ✔ | `ScopeV1::AuditOnly` never counts in the tally |
| §15.2 constraint map: matrix checks, exact small constraints, state boundaries | ✔ | `verify.rs` |
| §15.3 `VerificationPlanV1` / `VerificationEvidenceV1` binding network, ruleset, class, program, artifact, plan, job, state, trace, output, context, segments, suite | ✔ | `evidence.rs`, `PipelineEvidenceV1` (adds job root and `R` binding) |
| §15.3 beacon after commitments, unbiased sampling, cutoff, finality, withholding, reorg, retries | ◐ | `beacon.rs`; the beacon's review as unbiased, and any multi-round (GKR) transcript, remain activation gates |
| §15.3 replay determinism (IBD, pruning) | ✔ | pure functions of bytes; run/scope/byte-verifier agreement test |
| §15.4 derived `ε_check ≤ 2^-128`, declaration never above derivation, union bound | ✔ | `derived_error_bits` (descriptor's weakest per-repetition bits); plans that overclaim are `PLAN_FORGED` |
| §15.4 network bound `min(1, Q·ε + ε_env)`, retries and grinding in `Q` | ✔ | `network_false_acceptance_bits_v1`, `grinding_attempts_v1`, `AnchorTrackerV1::attempts` |
| §15.5 lifecycle, disputes by any bond, DA/timeout outcomes, bounded dispute load | ✔ | `lifecycle.rs` |
| §15.6 Kimi K3 feasibility report | ✖ | measurement |
| §15.7 activation tests (arithmetic/binding, adaptive adversary, court bounds, lifecycle) | ✔ | test suites above |
| §15.7 dormant fence `palw_probabilistic_constraints_v1`, no height | ✔ | `consensus/core/src/palw_probabilistic_constraints_v1.rs` |
| §15.7 shadow comparison, 9B-8k / 2M / Kimi reports | ✖ | measurement |
| §16.2 outcomes `ELIGIBLE_AT`, `FRONTEND_REQUIRED`, `KERNEL_EXTENSION_REQUIRED`, `KERNEL_NOT_ACTIVE`, readiness/capacity, bounds | ✔ | `outcome.rs` |
| §16.3 extension contract (families not brands, no plugins, new versions not reinterpretation) | ✔ | descriptors v1/v2/v3 |
| §16.4 coverage buckets; success needs on-chain registration **and** measured public prosecution | ✔ | `CoverageEvidenceV1`; SDK preflight reports the route (single programs and VLM pipelines), never as coverage |
| §16.5 coexistence, absent/forged ids, cheap unsound checkers, omitted families, cross-kernel replay, activation boundary | ✔ | `k2_wide`, `k2_adversarial` |
| §16.5 real 9B-8k / long-context registration, Final, court, DA | ✖ | measurement on a chain |

### RFC-0004 §0 (and §17 on this route)

| Item | Status | Where / evidence |
| --- | --- | --- |
| Admission binds kernel, plan, model, task, context, artifact | ✔ | `ModelKernelBindingV1`, `admit_candidate_v1` |
| Evaluations execute the real task; the Panel checks relations; Final before use | ✔ | evaluation pipelines on K2-TIR-v3; `rfc04_promotion_…` test drives claims through verification and the lifecycle |
| Assurance labels per V result (exact fold / exact court / legacy / probabilistic + descriptor + bits) | ✔ | `assurance.rs` |
| Sign test ≠ computational bound; union bound reported apart | ✔ | `promotion_decision_v1` |
| Epoch pins permitted kernels and composition; no mid-epoch regrading | ✔ | `EpochKernelPolicyV1`; unpinned kernel refused |
| Tests: forged scores, omitted constraints, wrong routing/state, invalid suite changes, reorg, old/new claims, promotion after Final | ✔ | `k2_pipeline` (forged score convicted, counted as a loss), `k2_adversarial`, `k2_wide`, `improve.rs` |
| §17 A2–A9 consensus state (lines, epochs, objects, job family, rewards) | ◐ | the legacy Phase F modules exist in `consensus/core` (`palw_improve_*`); their kernel-route variants wait for the fence |
| §17 A12–A15 second implementation, drills, audits, soak | ✖ | external |

### 2026-10-07 mission-alignment amendments (RFC-0004/0005/0007/0011, ADR-0173, RFC-0015 §1.1)

| Item | Status | Where / evidence |
| --- | --- | --- |
| An ordinary non-seat public bond localizes and convicts from public authenticated bytes | ✔ | `public.rs` `FreshVerifierV1` (built from published bytes only), byte-level fault proofs and court (`k2_public`) |
| Withholding ends in objective DA/default, never an arithmetic conviction | ✔ | `MaterialDemandV1`, `settle_demand_v1`, lifecycle `Unavailable { producer_defaulted }` |
| Receipts/quorum are not arithmetic truth; Freivalds failure becomes a bounded terminal proof | ✔ | tally only licenses; every failure localizes to one instance/scalar/edge |
| No new reward until the G14 criteria are evidenced for the profile with public material | ✔ | `ProsecutionGateV1`, `reward_eligible_v1`, `improvement_reward_gate_v1` |
| Coverage, `ε_check`, source fidelity and the gate are separate metrics | ✔ | `ReleaseMetricsV1` |
| Job, input and output binding; decode court (programs and pipelines) | ✔ | `job.rs`, `pipeline_public.rs` (`R` bound to the job's public seed) |
| DA response classes; per-position demands; availability default never the fraud slash | ✔ | `public.rs` `classify_position_response_v1`, `ledger.rs` |
| `PUBLIC_PROSECUTION_COMPLETE` derived from code, with per-path bounds | ✔ | `gate.rs` (programs and pipelines) |
| Consensus properties on an in-process chain: collusion, pre-emption, Final race, liability, reorg/IBD, collateral | ✔ | `ledger.rs`; audit: [`kernel-public-prosecution-audit.md`](kernel-public-prosecution-audit.md) |
| G14 on a real chain (RPC → fee → inclusion → fold → slash → blocked Final) | ✖ | EXTERNAL_GATE_PENDING (RFC-0014/0015 drills) |

## 3. Tests (`cargo test -p misaka-palw-kernel`)

* `k2_e2e` — RFC-0007 Part II's dense GQA + SwiGLU + MoE + history fixture: outcomes, plan forgeries, honest pass, exhaustive one-scalar
  lies, every exact family, history forgery, DA, bind-first, segments, batching, receipts and tally.
* `k2_wide` — K2-TIR-v2: moduli selection, `i128` extension under v1 / eligible under v2, every scalar lie, a lie aliasing to zero
  mod 2^127−1, coexistence and deprecation, cross-kernel replay.
* `k2_adversarial` — permuted history, swapped expert, uncommitted weight, cheap unsound suites, unknown checker ids, reorg-stale
  receipts, verdict agreement across runs, scopes and a byte-only verifier.
* `k2_public` — fresh public bond from bytes, byte-level court, withholding → default, reward gates (inference and evaluation).
* `k2_ledger` — the in-process chain: full-Panel collusion before and after Final, self-consistent garbage, borrowed traces and
  substituted outputs, withheld positions demanded in one round, DA classes and defaults, pre-emption, the Final race, reorg /
  restart / IBD, collateral. Every outsider replays the chain and reads only public DA bytes, with its own salt.
* `k2_ledger_pipeline` — the same for pipelines: stage, edge and `R` lies, `R` from another seed, a vision-language decode
  substitution, a withheld vision stage demanded and served on chain.
* `k2_pipeline` — K2-TIR-v3 on the IR's own v2 fixtures (text-to-image, vision, VLM, encoder–decoder, exact-match and log-likelihood
  evaluations), stage/edge/`R` lies, withheld upstream, job and `R` binding, RFC-0004 evaluation → Final → promotion.
* Unit tests: Mersenne fields against double-and-add, challenge streams, beacon/anchor, descriptors, lifecycle, receipts, improve.

## 4. Where the RFCs and the current code differ (the RFCs lead)

* **Verification**: the live Panel replays (`palw_verification_v2`, `PalwSeatReceiptV3`); the kernel route checks constraints and
  files `PalwConstraintReceiptV1` per scope. No V3 receipt is read as a kernel receipt.
* **Admission**: live classes pass TIR admission v10 and family certification; a kernel-bound class passes `check_plan_v1` (or
  `check_pipeline_plan_v1`) under an active descriptor and binds `ModelKernelBindingV1` under a new class-id domain.
* **Improvement**: the live `palw_improvement_v1` example fixes candidates to Phase F v10; on this route an epoch pins kernels and
  composition profiles first, and grading uses the pinned set.
* **Evidence**: live claims commit captures and FOLD prefixes that only producers and bound seats read; here every node value and
  every stage input is committed and wired, so a fresh public bond can open and convict (ADR-0173).
* **Generative classes**: live `palw_gen_v1` classes use the step tree and close sizing; here a pipeline stage is a kernel claim
  over its v1 view and its edges are exact relations.

## 5. Open (external gates, in the order the RFCs place them)

1. Independent soundness review of the composition, the alias bounds and the CRT argument (RFC-0011 §15.4, ADR-0172 §6).
2. Review of the beacon as unbiased under withholding; a transcript construction if a GKR family is ever proposed (§15.3).
3. Cost on real models: producer, verifier, DA, court and dispute load for 9B-8k, validated long context and Kimi K3 (§15.6–15.7).
4. Node values carry row/column Merkle commitments (`merkle.rs`): a `MatMul` scalar court opens one row of X, one column of W and one row of Y (O(k + n) bytes). Other courts open the one instance they recompute; Merkle tiling of those (e.g. a long `ReduceSum`) is a further suite extension if measurements call for it.
5. Authenticated public availability on the real chain (who serves, retention) is RFC-0009 stage B / RFC-0014's DA path; the demand/default rules themselves are implemented and tested in `ledger.rs`.
6. Shadow comparison, the G14 drill on a real chain, audits, soak, and only then a proposal to give the fence a height.
