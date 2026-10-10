# ADR-0037: PALW asynchronous job state machine — hash-floor direction withdrawn

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


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
   `PalwUnavailable` (`consensus/pow/src/lib.rs`). A single inference-runtime fault was a
   chain-liveness fault. The proposed permanent hash floor was later withdrawn by ADR-0039.

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
The exclusions that survive are q-of-n committees as final truth, tolerance-based comparison as
a slash basis, service/admin per-job overrides, and self-declared capacity in the safety argument.
The original exclusion of PALW block production was withdrawn by ADR-0038.

## Decision 1 — Three layers; PALW is never block-critical on a value-bearing network

**Withdrawn / 不採用。** hash PoW を本体、PALW を任意の credit overlay にする案は、有用計算を consensus work にする目的と一致しないため。[ADR-0038](0038-palw-is-the-consensus-work.md) が置き換える。Decision 2 以降の job state・escrow・court 規則は後続 ADR の修正範囲を除き維持する。

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

* **Withdrawn tracks:** PALW を任意 overlay にする Track A と hash floor の導入は、有用計算が consensus work を担う目的に合わないため継続しない（ADR-0038/0039）。worker failure の hardening は別の有効な変更として維持する。

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
M3. The former hash-floor migration is withdrawn by ADR-0038/0039.

## Summary

job state・escrow・budget・objective court は後続 ADR の修正に従って維持する。hash floor 上の任意 PALW overlay に移す旧方針は継続しない。

## Mission alignment amendment — 2026-10-07

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
