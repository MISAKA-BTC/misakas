# RFC-0008 implementation spec v1 — unified PALW EXEC lane (DRAFT, NOT IMPLEMENTED)

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: revised design, 2026-10-10. Baseline: `MISAKA-BTC/misakas` main `282355ba9`.
No activation height, runtime change, deployment or fingerprint change is authorized by this document.
The type and fence names below are proposed names, not declarations that exist in main.

This document implements the direction of [revised RFC-0008](../../rfc/0008-palw-claim-backed-consensus-blocks.md).
The former algo-11 design body has been deleted. Only its [v0 test results and unrun cases](rfc-0008-v0-test-record.md)
are preserved; they are not v1 requirements or v1 validation evidence. Main's active heartbeat, BASE-0, clock, REAL admission and transaction-permit rules are the baseline.

**2026-10-10後続改定 acceptance boundary:** [ADR-0177](../../adr/0177-model-bond-allocation-without-availability-consensus.md)
and RFC08 §6.1 withdraw mandatory Torrent, Seeder/PoR/Full Fetch/lease/TRDC/FPR and availability-driven qualification/weight gates.
Model supply is optional off-chain activity; immutable model identity and finite claim-specific evidence/court obligations remain.
Do not turn this court into model retrieval through whole-weight or iterated range demands. Define permitted units, cumulative scope
and authenticated model operands before claiming bounded court completeness. G14 is conditional on acquiring the registered model.
Model-bond coinbase allocation uses distinct miner principal and preserves network/model/individual-bond budgets and EXEC zero weight/DAA.
Open-versus-closed economics, allocation/recovery and non-interference gates are pending; this historical implementation does not satisfy them.

**Implementation record (lane X8/X8R; non-normative — the design above wins).** Dormant code for an earlier reading of this spec
exists on branch `rfc8/x8r-review` behind the unarmable fence `palw_exec_payload_v2`. What is built, what proves it and which gates
remain open are in [rfc-0008-v2-implementation-record.md](rfc-0008-v2-implementation-record.md) and in the labelled section 10 at the
end of this document. That record does not close any gate of section 9; every gate stays open until the gate's own evidence exists.

## 1. Invariants and rollout boundary

One EXEC class contains `EXEC_TX` and `EXEC_SLICE`. Extend the existing algo-10 execution lane under a new
versioned fence; introduce no algo-11 selected-parent candidate. Do not silently reinterpret `PalwExecEnvelopeV1`
or its `PXR1` bytes. Existing blocks keep their original validation and hashing.

For every accepted EXEC carrier, including after its root reaches Final:

```text
selected-parent eligibility = false
raw blue_work contribution = 0
PALW safe_weight / immature_weight contribution = 0
consensus blue_score contribution = 0
DAA tick / retarget contribution = 0
pruning block-level contribution = 0
normal consensus k-anticone contribution = 0
```

EXEC work settlement does not feed the fork-choice ledgers. A REAL root retains only the contribution its
own admitted useful work earns under REAL's existing rules. It does not import its EXEC slices' work as additional weight.
No round weight pool is carved from the root: ADR-0168 §10.3's positive `ρ` is outside this design.

Heartbeat/BASE-0 validity, work/weight, production policy, clock cursor, fallback timers, anchor duty and 120-second
cadence remain as in baseline main. EXEC presence, backlog, class strings and node-local observations are not inputs
to those rules. No `palw_ws_clock_v1` or RFC8 floor-exclusion fence is part of this release.

A proposed `palw_exec_payload_v2` fence must be absent/dormant on all shipped presets until every gate in §9 passes.
Its parameters, canonical wire bytes, hash domains, fingerprint/schedule hashing and legacy dispatch must be specified
and reviewed together. Do not reuse v0's object tags/state deltas without a fresh registry audit. Dormant parameters
must preserve shipped fingerprints and state roots; no height is selected here.

## 2. Envelope and atomic subtype dispatch

```rust
PalwExecV2 {
    version,
    network,
    anchor,                 // carrier's chain anchor
    subtype: EXEC_TX | EXEC_SLICE,
    tx_permit: Option<RoundPermitV1>,
    work_slice: Option<ClaimSliceV1>,
    payload_root,
    executor_bond,
    signature,
}
```

Initial dispatch is exhaustive:

| subtype | tx_permit | work_slice | payload | authorization |
| --- | --- | --- | --- | --- |
| EXEC_TX | Some | None | Existing transaction batch | Existing finalized-credit schedule/permit |
| EXEC_SLICE | None | Some | Work/evidence commitments, no user transaction batch | Active root, authorized bond, canonical next range |
| Either | Some | Some | Any | Reject entire carrier |
| Either | None | None | Any | Reject entire carrier |
| Unknown/version mismatch | Any | Any | Any | Reject entire carrier |

No partial-block acceptance. Reject mismatched subtype/field/payload, unknown versions and noncanonical encodings.
A future dual-payload subtype needs a separate version and both independent authorizations; it cannot turn a
WorkSlice into a TxPermit or vice versa.

The signature binds network, wire version, subtype, carrier anchor and payload commitment, bond, and the full
type-specific content. Use distinct TX/SLICE signing and hashing domains. Both types are bound to their carrier
position; no cross-network, cross-version, cross-anchor or cross-subtype replay. Pin bytes and golden vectors before coding activation.

Preserve the existing algo-10 constant-target header price (`PALW_ROUND_WORK_LOG2`) for both subtypes in the initial
release, alongside their independent authorization and resource caps. WorkSlice is not a free header or a new
chain-work lottery. Root REAL retains the existing ticket rules; slice splitting must not improve its ticket odds.

## 3. Root REAL and ClaimSlice

Root state commits canonical class/job/input identity, active kernel/plan/suite versions, total bounded work,
deterministic slice plan, initial boundary, DA/evidence policy, authorized executor bond(s), exposure/escrow,
expiry and the REAL chain anchor. A session-open declaration earns no credit by itself.

The root REAL must satisfy main's REAL work/ticket/admission rules. If its actual work is a prefix of this job,
reserve that range in the same budget used by slices. Do not assign whole-session declared work to the root.
If existing REAL admission cannot bind an open long claim without inventing work, the root-lifecycle gate stays
closed until the versioned binding and its tests exist; a tx-carried root declaration is not a substitute REAL block.

```rust
ClaimSliceV1 {
    root_claim_id,
    slice_index,
    class_id,
    canonical_job_id,
    kernel_version,
    plan_root,
    canonical_range,
    predecessor_state_root,
    result_state_root,
    evidence_root,
    da_root,
    executor_bond,
    signature,
}
```

Range is derived from the class's canonical IR/kernel cost and approved checkpoint boundaries. The plan partitions
the bounded job into disjoint ranges, including any root prefix. Slice count and target work are resource parameters,
not reward multipliers. Reject profiles whose canonical range cost or boundary commitments cannot be derived.
Do not carry an authoritative declared CCU/PWU. Verify class/job/kernel/plan against the root rather than trusting repeated fields.

Slice admission reads the canonical parent state plus earlier accepted carriers in the declared deterministic order:

1. Root is accepted, active and anchored on this selected-chain history; not complete/Final/expired/voided.
2. Bond is eligible, authorized and has the required funded exposure; signature and payload commitment match.
3. Index/range is the next canonical one, not previously used and not overlapping a root prefix or accepted slice.
4. Predecessor is the committed initial/root-prefix boundary or the previous accepted slice's result boundary.
5. Evidence/DA commitments and active kernel/plan/suite match; all required public material has the specified availability.
6. Per-root, per-bond and lane pending depth/work/bytes limits hold. Pending does not mean verified or Final.

The bounded pending depth may allow the next slice after predecessor acceptance, before verification. Invalid
predecessor arithmetic voids its dependent suffix; positive whole-claim verification is still required before settlement.
Duplicate/replayed/malformed carriers create no row, fee entitlement, work reward or chain-position contribution.

## 4. Separate ledgers and conservation

```text
TxPermitUse[(span, round, permit_index)]
WorkSliceUse[(root_claim_id, slice_index)] -> range, roots, carrier, stage, deadlines
RootWorkBudget[root_claim_id] -> job identity, prefix, plan, accepted/verified work,
                              escrow allocation, lifecycle, settled marker
JobWorkUse[canonical_job/work identity] -> the applicable one-use history rule
```

The exact job/work identity must bind class, canonical inputs, committed computation range and execution context,
so copying a job into a new root, identity or branch cannot earn the same work twice in a canonical history.
Define legitimate repeated-job semantics with the existing ticket rules, rather than treating a new root id as proof
of fresh work. Across competing histories ledgers are branch-local; reorg restores them atomically, and only the
canonical history's spendable reward/permit state survives.

Use checked integer arithmetic. Required properties:

```text
root_credited_work + sum(credited_slice_work) <= canonical_claim_work
intersection(credited_range_i, credited_range_j) = empty, for i != j
sum(root_and_slice_reward_paid) <= root.funded_reward_allocation
one settlement per root; one accepted use per (root, index) in a history
one permit consumption per existing TxPermitUse key
all EXEC chain-position / fork-choice / DAA terms = 0
```

Preserve main's issuance/allocation caps and funded escrow rules. EXEC_SLICE receives no carrier coinbase subsidy.
Never escrow the whole root allocation again for each slice. The chain's shared reward accumulator and the payout
builder must read the same root settlement result. Transaction fee income remains in the existing EXEC_TX accounting.

Initial lifecycle aggregates at the root:

```text
Open -> SlicesAccepted -> Complete -> PositivelyVerified -> WindowClosed -> Final -> Settled
                        \-> Disputed -> objective verdict / dismissal
                        \-> expiry / DA default -> defined timeout or void outcome
```

These are design states, not current enum names. The required positive checks, closed challenge window, no accepted
unresolved dispute, DA/retention obligations and funded settlement all apply. A complete claim is not Final.
No slice becomes an independently reward-bearing Final claim. Slice status becomes final only as part of root Final.
Final is processed idempotently with the one-time settlement marker, including restart and reorg.

If Final work qualifies for a later execution schedule, derive one aggregate credit from the root through the existing
eligibility/credit/cap rules and snapshot it once. Never insert root plus N slices as N+1 finalized attempts.
WorkSlice admission spends no round permit and grants no immediate permit. TxPermit consumption creates no work credit.

## 5. Common lane attachment and deterministic acceptance

For v2, lane edges/heads and their bounded acceptance closure are execution data, not consensus parents. A normal
chain block must not name either v2 EXEC subtype as a consensus parent. Both subtypes share the same lane classifier,
parent filtering, zero-work/level predicates and DAA exclusion. Same-class integration must hold through sync,
pruning-proof validation, template construction and RPC; a new display label alone is insufficient.

The carrier anchor must be on the accepting chain and inside its versioned attachment window. The root anchor is
separately checked. Define a header/body-committed execution-head/closure commitment and a bounded canonical order
that includes subtype and respects predecessor dependencies. Pin encoding, root/count, max heads, walk/byte limits,
window and legacy-v1 coexistence in the compatibility gate; ADR-0168's proposed trailer is a reference, not shipped code.

Build and validate the same covered set from parent state. Compute subtype verdicts without mixing ledgers. The
template filters invalid/stale heads and resumes from the latest accepted lane checkpoint. Missing lane material
delays that lane acceptance; it does not force chain production to wait or disable heartbeat/BASE-0. Invalid carrier
data is rejected/skipped according to its explicit lane verdict; a false commitment declared by the chain block
itself remains a block commitment fault. Require exact no-write behavior on refusal.

Legacy v1 blocks retain their existing mergeset/permit validation below the new fence. The upgrade must define
whether and for how long v1 continues alongside v2 and prevent accepting one carrier through both paths. Do not
activate a partial v2 where slices still enter the consensus mergeset while TX blocks use isolated closure.

## 6. Verification, failure and independent transaction acceptance

Bind the class/root/slice `challenge_policy_id` and use [RFC07 Part VI](../../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)
as the only source/seed/sampling/transcript protocol. A WORK_SLICE binds root/index/range, states and complete evidence
before its own future source window; a seed exposed at root-open cannot test arbitrarily later-created slice statements.
Whole-root checking follows the same scope/boundary composition. No alternative seed formula or per-slice independent
Final credit is introduced. Missing qualifying sources remains pending/deadline-limited; EXEC_TX/heartbeat/BASE-0
liveness and all EXEC zero-weight/zero-clock invariants remain unchanged.

Apply ADR-0173 / RFC14 public-bond prosecution and RFC07 Part V / RFC11 §15's active kernel/constraint rules.
Each slice and boundary has public authenticated material for complete bounded localization and exact court.
Compose whole-root soundness over every required range/boundary; repeated carriers/receipts are not independent tests.
Missing evidence, silent verifier and unresolved challenges cannot yield Final. Retention must cover reorg/pruning,
dispute deadlines and collectible liability after Final. Panel=0 remains subject to RFC14 and RFC15's separate gates.

A proved false predecessor voids the dependent suffix and prevents whole-root settlement in the initial release.
No independent prefix payment is introduced here. Use evidence-bound offence/liability rules; timeout and verifier
unavailability are not arithmetic convictions. Do not invent heuristic equivocation slashing for honest reattachment.

An EXEC_TX's acceptance depends on its own canonical anchor, permit and transaction rules. It does not depend on
a co-located session's later success. Work fraud does not retroactively invalidate independent accepted transactions;
ordinary canonical reorg handling still applies. Raw execution acceptance is not proof of work Final or EVM settlement.

## 7. Resource bounds and liveness

Set and meter maximum roots per bond, slices per root, pending depth/work/bytes, evidence and DA bytes, retention
storage, closure heads/walk, signature checks, verification queues and court exposure. Price each root/obligation
from funded resources; do not keep an unpriced forever-growing job ledger. A bounded deduplication checkpoint must
remain sufficient to prevent replay after pruning, or the gate remains open.

EXEC_SLICE has its own quotas and queue budget, independent from existing TX round width/gas/permit budgets.
Reserve processing/relay resources for REAL, heartbeat, BASE-0 and EXEC_TX. Node-local backpressure controls production
and relay only; it cannot change consensus validity. State-based caps must be deterministic under every arrival order.
Prevent a stale/invalid EXEC head from wedging a chain template or the other EXEC subtype.

## 8. Observability

Expose REAL / EXEC / HEARTBEAT / BASE-0 and EXEC subtype; distinguish carrier accepted, slice pending/verified/voided,
root complete/Final/settled, credited work, TxPermit consumption, TX fees, escrow/reward, evidence availability,
queue/backlog and chain clock advancement. Do not report EXEC as consensus BLUE or include it in selected-chain share.
Surface required public evidence commitments and the reason for lane refusal without implying proof of physical execution time.

## 9. Gates, validation plan and current status

**Every v1 implementation gate is open.** This documentation change runs no new consensus code and has no v1 drill results.

| Gate | Required evidence before activation |
| --- | --- |
| Root lifecycle / REAL eligibility | Actual admitted root work, disjoint prefix, bounded open session, complete positive verification, root Final and funded single settlement through the real pipeline |
| Wire / compatibility | Canonical v2 encoding, domain vectors, registry audit, fence validation/fingerprints, below/at/above activation, unknown versions, legacy coexistence and old-node behavior |
| Lane isolation | Both subtypes never selected parents; zero raw/PALW weight, blue score, DAA/retarget and pruning contribution; no consensus anticone effects, including forged headers and bursts |
| Accounting | Independent permit/work ledgers; duplicate/overlap/skip, root/job/branch replay, checked overflow, repeated Final, root-prefix conservation, snapshot credit once, no carrier subsidy |
| Public verification | Real verifier and public-bond slice/boundary prosecution, DA default, false Valid, authenticated localization, bounded exact verdict, retention and post-Final liability |
| Liveness / capacity | Paired baseline/v2 DAGs and stopped producer/verifier/DA/court runs; identical heartbeat/BASE-0/clock/anchor-duty outcomes; bounded queues and no TX starvation under slice flood |
| Deterministic state / recovery | Reorg across root/slice/Final/activation, restart, IBD, pruned import, shuffled arrival, missing material, stale heads and no double acceptance |

Use baseline executions without slices for equality of liveness/fork-choice/clock outputs, then insert valid and invalid
slice bursts while holding chain inputs fixed. Adding valid reward state can change escrow balances, but cannot add a
fork-choice or clock term. Check independent TX results except ordinary transaction conflicts/canonical reorg.
Property-test event histories and compare archival/fresh/pruned nodes; drill the full funded path with actual verification.

The v0 tests describe a different branch design and prove none of these v1 gates. Keep unresolved items named;
do not substitute a placeholder or harness-signed receipt for implemented verification. Choose activation and capacity
parameters only after independent review, measured evidence and a separate coordinated release.

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../../rfc/0015-panel-free-permissionless-verification.md)を適用する。

新reward/REAL/root経路は受理時に同じproducerのclaim/block/reward/Final weight配分を予約し、全権利行使・Final/retired/reversalとRPC/EVMで同じ会計を読む。
容量倍率の拡大で総creditを増やさず、共通DAA拘束・残存責任と旧新ruleset混在を検査する。EXECのweight/DAA=0、既存発行量、unique-work条件は維持する。実装・activationは未完了である。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](../../adr/0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。

## 10. Implementation record — X8R amendment 1 (2026-10-08/09): the slice verification route and the five design decisions

> **Label (2026-10-10 merge).** This section is lane X8R's implementation record, kept as it was written. It is **not normative**: where it
and the user's 2026-10-10 revision of sections 1–9 and the two 2026-10-10 sections above differ, the revision wins. Section 10.9 maps the
revision onto the code.

This amendment closed the choices sections 1–7 left open, for the implementation on `rfc8/x8r-review`; it changes no
activation height, preset or fingerprint (the fence stays unarmable on every shipped ruleset). Each decision states its rationale; none
needed a policy value from the user (the numbers it touches are existing protocol constants or the G14 route's own policy, whose
production values stay on hold per the 2026-10-08 Panel=0 rulings).

### 10.1 The verification route is the G14 kernel route (closes the section-6 route and gap "no production writer of `Verified`")

**A slice is verified only through a claim of the probabilistic-constraint kernel route (`palw_probabilistic_constraints_v1`, tag 110),
and only that route can verify, convict or default it.** Concretely:

1. **The root's class must be kernel-bound.** A root declaration is refused (`ClassNotKernelBound`) unless the root claim's V2 class
   has a kernel binding (onboarding tag 106: the V2 class bound to a kernel class that registered after `PUBLIC_PROSECUTION_COMPLETE`),
   and its `plan_root` must be that binding's `plan_root` (the kernel class's `VerificationPlanV1` root; `PlanNotKernels`). So a work
   session exists only for a G14-complete class — the class whose claims an ordinary outside bond can already convict or default from
   public material (the kernel route's G14 cases, `g14-node-e2e-record.md`).
2. **Each slice names its verification claim.** `slice.evidence_root` is the id of a kernel-route *program* claim, and admission rule
   5 gains the **verification binding** (refused by name, no write): the route holds that claim; it is not failed (convicted,
   unavailable, timed out); its producer is the slice executor's kernel bond (`palw_kernel_bond_id_v1`); its job's class is the
   bound kernel class; its job's `nonce` uses the **retention-bound slice job nonce** of §15.6. Its v1 base is
   `H("misaka-palw/exec-v2/slice-job-nonce/v1"; root claim ‖ index ‖
   range ‖ canonical job ‖ plan root)` — so one kernel job, hence one kernel claim, can back exactly one
   `(root, index, range)`; `slice.predecessor_state_root` is the **token state** `H("misaka-palw/exec-v2/token-state/v1"; n ‖ tokens)`
   of the job's prompt; `slice.result_state_root` is the token state of the prompt followed by the claim's generated tokens;
   `slice.output_root` is the token state of the generated tokens alone; and `slice.da_root` is the kernel claim's own evidence root.
   Rule 4 then makes consecutive slices one token stream: slice `i+1`'s prompt is slice `i`'s prompt and output.
3. **The slice's stage follows its claim, every block, after the kernel route's closing tick** (so a Final, conviction or default the
   route decided in the block is read the same block): a kernel claim **Final** makes its slice `Verified` (a root whose every slice is
   verified releases its claim's `Final` hold); a claim **convicted** makes its slice *proven false*; a claim **unavailable or timed
   out** (withheld material, no timely check) makes it *defaulted*. A claim the route no longer holds while its slice is pending is
   treated as defaulted (it cannot be verified).
4. **Sampled checks** are the kernel claim's own, under its committed subject and the one post-commit challenge contract
   (`misaka-palw-challenge`); the slice's `WORK_SLICE` subject binding (`challenge_binding`) is recorded with the slice so a future
   supplemental check has its own seed domain. No seed or court is added for slices.

**Why.** RFC-0008 §6 requires that an ordinary non-Panel bond can verify and prosecute every slice and boundary from public material,
with an exact court and DA default. The kernel route is the one place this repository has that property on the real node (G14 cases
PASS: an outsider convicts a covered lie, a withheld position defaults, OPV windows finalize Panel-independently). Binding the slice
statement to a kernel claim of the same token stream gives every slice that property without a second court, a second seed formula or
a second DA protocol, which RFC-0007 Part VI and RFC-0014 forbid. **Limits (named, not hidden):** pipeline (K2-TIR-v3) classes are
refused for slices until the K2-at-scale lane defines a segment state for them (`VerificationKindUnsupported`); the initial boundary
(`initial_state_root`, slice 0's predecessor) is the root bond's declaration and its link to the REAL claim's verified output is not
checked — every slice after it is verified as a continuation of it.

### 10.2 Suffix void on a proven-false or defaulted slice (closes the section-6 CODE gap)

A proven-false or defaulted slice voids **itself and every later accepted slice of its root** (they chain from its result), the root
becomes `Voided { from_index }` and the root claim is voided — `WorkSliceProvenFalse` (void reason 131) or `WorkSliceDefaulted` (132).
Neither reason charges at the V2 level: the evidence-bound liability is the kernel route's conviction or default penalty on the slice
executor's kernel reservation, already collected; a second V2 forfeit would charge the root bond for another bond's lie. No prefix or
partial payment is made (section 6: "no independent prefix payment"); the extra executors' exposure returns.
Active execution rows stay until claim retirement; the independent authenticated job-use history
then survives until the original authorization closes (§12.6). A slice that was `Verified` and whose claim is convicted later (inside the route's liability
horizon), or whose claim forfeits its reservation to a post-Final default (10.5's revision), is handled the same way while its root has
not settled.

### 10.3 Flood residual (DESIGN gap 4) — decided: bounded by relay admission, not by a consensus rule

Consensus already bounds what is *covered* (8 heads, the lane leaf bound, 8 slices folded per block, the pending and root quotas) and
a flood moves no consensus number (P9). The residual — unanchored lane blocks occupying relay and storage until pruned — is bounded
**node-locally**, as section 7 requires ("node-local backpressure controls production and relay only"): past the fence a node relays an
`EXEC_SLICE` block only when, against its sink state, the root is open, the executor is authorised, the index is in
`[next_index, next_index + PALW_EXEC_V2_MAX_PENDING_DEPTH)`, the named verification claim exists, and the executor has relayed fewer
than `PALW_EXEC_V2_MAX_PENDING_PER_BOND` slice blocks in the anchor's span; it relays the first `EXEC_TX` block per
`(span, round, permit index)` and keeps (does not announce) a second. A block outside these rules is still validated and stored if a peer
sends it — validity never depends on relay policy. **Why:** the price of a lane header is the constant algo-10 PoW plus one ML-DSA-87
check, both paid by the sender; the only unpriced cost was gossip amplification, which the state-aware filter removes for every
non-executor and the per-bond quota bounds for an authorised one.

### 10.4 Schedule credit of settled slice work (DESIGN gap 6) — decided: no raise in the initial release

The root claim's REAL `Final` earns exactly the credit the existing eligibility/credit/cap rules give its own admitted work, snapshot
once; settled slice work adds **no** schedule credit, weight or permit. Section 4's "derive one aggregate credit from the root" is read
as this single existing credit (section 1: a REAL root "does not import its EXEC slices' work as additional weight"). **Why:** the plan's
boundaries are the root bond's declaration and slice work is verified as kernel claims that themselves earn no fork-choice or schedule
credit; raising the root's credit by declared slice work would let a root bond mint schedule credit by declaring a longer plan. A raise
needs a measured, verified work unit for slices — a later versioned rule.

### 10.5 Post-Final liability for slice legs (DESIGN gap 2) — decided: the kernel reservation is the collectible liability, and caps the leg

Each slice's liability after the root's `Final` is its kernel claim's: the route holds the slice executor's reservation until the claim's
liability horizon, and a post-Final conviction slashes it (G14 case "convicted after Final within the liability horizon"). So the leg is
always backed. **Each slice's contribution to its executor's leg is capped at its leg cap, fixed when the slice verifies**: the reservation
its kernel claim holds at that moment, less the route's own `Final` reward on that claim (`palw_exec_v2_leg_cap_v1`). An executor's leg at
the root's settlement is capped at the sum of its slices' caps; the excess stays with the root executor's leg (nothing minted,
`sum(paid) == allocation` still holds). **Why:** section 6 requires retention "to cover … collectible liability after Final"; an
uncapped leg larger than the collateral that backs it would be an uncollectible gain. The route's own invariant is `claim_reward <
claim_collateral` (what one claim gains stays below what it puts at risk); a slice leg is a second gain on the same claim, so the two
together are held under the same reservation. Section 15 supersedes the original immediate-payment design: an independently
funded EXEC reward row holds slice and fee legs without inventing an additional collateral offence against another actor.

**Revision (X8R round 2, 2026-10-09).** The first text capped each leg at the reservation the route held *at the root's settlement*. The
route releases a Final claim's reservation once its liability horizon passes, so that cap fell to zero for every slice verified long
enough before the root's `Final` — and a root bond, itself an authorised executor, could hold back the last slice until the earlier
executors' horizons had passed and keep their whole legs. The cap is now fixed at verification, when the liability it stands for exists;
its later release changes neither the risk nor the work. The same review found the converse hole: the route answers a demand left
unserved inside the horizon after `Final` by forfeiting the whole reservation while the claim stays `Final` (`PostFinalDefault`). The
settlement-time read had zeroed such a leg incidentally; the outcome rule now reads a `Final` claim that reserves nothing at or before
its horizon as **defaulted** (`palw_exec_v2_claim_outcome_v1`), so the slice is defaulted and, while its root has not settled, voids the
suffix and the root as 10.2 says. A release *past* the horizon is not a default.

### 10.6 Permit equivocation evidence (DESIGN gap 8) — decided: no offence for v2 permits

A `PXE2` `EXEC_TX` permit signed twice is not slashable. The covered set is judged in the canonical order `(round, permit index, hash)`:
the first block of a permit takes it, every other is `PermitAlreadyUsed`, so a double-signed permit can never be spent twice. **Why:**
v2 binds the carrier's anchor into the signature, so an honest executor whose carrier was stranded by a reorg *must* re-sign the same
permit at a new anchor; section 6 forbids "heuristic equivocation slashing for honest reattachment". The v1 evidence path
(`round_equivocated`) is not extended to `PXE2`; relay keeps the second block un-announced (10.3).

### 10.7 Anchoring-window strand (DESIGN gap 5) — decided: keep the two-span window; the producer republishes

The covered set stays bounded by the two-span window (a span is one DAA on testnet-12). A lane block anchored on a branch that is
reorganised away and outside the window on the new branch is not covered there; nothing is invalidated or double-credited (P10). The
node's `EXEC_SLICE` producer republishes a slice whose carrier is not anchored on its sink chain once the carrier has left the window —
re-signed at the current anchor, the honest reattachment 10.6 protects. **Why:** a longer window is a parameter, not code, but it
lengthens the consensus closure walk for every anchoring block; republishing costs the executor one header.

### 10.8 Fence prerequisites added

`validate_palw_exec_payload_v2` additionally requires, at or below the fence: `palw_probabilistic_constraints_v1` (the verification
route, 10.1) and the declaration of `palw_audit_2026_09_11` (A-2's tolerance of undecodable lifecycle payloads, which keeps a tag-130
carrier block-valid on a fleet that mixes builds — the X8R review, record section 10).

### 10.9 X8R round 3 (2026-10-10): the user's revision applied, G14 completion (GAP-60 to GAP-63) and ADR-0176 D2

Like 10.1–10.8, this subsection records the implementation's choices and is **not normative**; sections 1–9 and the two 2026-10-10
sections win. It maps the revision onto the code, closes the G14 completion matrix's EXEC-slice gaps, and names what lane BUDGET must
provide. Nothing here arms a fence or moves a shipped id.

**The revision, row by row.**

| Revision item | Where the code stands |
| --- | --- |
| EXEC weight / DAA / blue score / selected parent = 0, settlement included (§1, ADR-0176 D3) | unchanged: P1, P3, P9 pin it; no settlement path writes weight |
| One root settlement, unique work (§4) | unchanged: `JobWorkUse`, the one `Settled` marker, `sum(paid) == allocation`; now net of the bound claims' route rewards (D2 below) |
| ADR-0177 non-interference | the slice route demands only claim-specific material (one committed position of one kernel claim); no model bytes, no availability condition. The outsider computes with its own model copy; G14 for a slice is conditional on the verifier having acquired the registered model |
| ADR-0176 D2: no double rights from root/slice splitting | the D2 rule and hooks below |
| Rule E (ADR-0178) | EXEC carriers contribute no fork-choice term, so rule E reads nothing from this lane; the root REAL claim is an ordinary REAL claim to it |

**GAP-62 — the initial boundary is linked to the REAL claim's output (the prefix binding).** Before this round the initial boundary was
the root bond's unchecked declaration. Now, wherever the kernel route is in force, the declaration names a **prefix claim**: a
kernel-route program claim, by the root bond's kernel bond, of the root class's bound kernel class, whose job carries the **prefix job
nonce** `H("misaka-palw/exec-v2/prefix-job-nonce/v1"; root claim ‖ prefix work ‖ canonical job ‖ plan root)`. The fold checks, from
chain state alone (refused by name, nothing written):

1. the job's prompt is the prompt the REAL claim's execution anchor names (`palw_attempt_prompt_ids_v1` at the class's canonical
   prefill — the IR class record's facts);
2. the claim's generated tokens are the REAL claim's committed output: `palw_attempt_output_root_v1(ctx, generated) ==
   claim.output_root`, `ctx` the canonical attempt context J5a/J5b derive (`palw_tir_attempt_context_v1`);
3. the initial boundary is `token_state(prompt ‖ generated)` (`InitialBoundaryNotLinked` otherwise);
4. the claim has not failed.

So a wrong initial boundary cannot enter the chain at all, and the REAL claim's own output is now public material that the kernel route
adjudicates: an outsider with the model convicts a lying prefix claim exactly as a slice's, and the conviction (or a default of its
withheld material, or a forfeit after its `Final`) voids the whole session — every slice, the root (`Voided { from_index: 0 }`) and the
REAL claim (`WorkSliceProvenFalse` / `WorkSliceDefaulted`, the prefix being the session's first verified piece). A pending prefix holds
the root's `Final` like a pending slice. This is the slice binding of 10.1 applied to the root's own work, so it adds no court, seed or
DA protocol. **Limit:** it binds the REAL claim's *output*; the REAL claim's V2 step trace is still adjudicated on its own route.
That is G14C's GAP-81, and this binding is the shape GAP-81's option "bind the V2 claim 1:1 to a kernel claim of the same job" would
take.

**ADR-0176 D2 — one work, one right, one budget.** The round-3 advance/netting design recorded here is superseded by §13.
Bound kernel claims are verification vehicles: their computation Final cannot pay the producer or spend the poster's job escrow.
At root Final, their actual fee payments replace native issuance inside the executor's share of the root's one allocation.
Already-paid standalone work cannot attach later. EXEC weight remains 0 and carriers mint nothing.

| Hook | Current consumer | Conservation |
| --- | --- | --- |
| H-3 | `EXEC_SLICE` carrier | no block/reward issuance from the carrier |
| H-3a | root declaration / slice admission → `bind_exec_reward_v1` | rooted table-27 hold; no additional root reward reservation, no early fee debit |
| H-3b | native `finalize_claim` → `bond_budget_final_reward_v1` → `settle_at_final_v2` | clip the root's allocation once before splitting; actual fee payments reduce native issuance by the same amount |

**GAP-61 — superseded by the Lead's GAP-81 decision; the seam is re-scoped, not replaced.** OPVB's reward gate is now per claim
verification route (`opv/bootstrap-beacon` `c3221bc34`, unbuilt at this writing): the gate's `Onboarded` grants a V2-root claim nothing
beyond the old rules, and only kernel-route claims earn the new rewards, each through `opv_gate_v1`. A session's REAL root is a V2
claim, so its admission is the legacy channel's (registry lifecycle, Panel room, seating, bond share — Panel seat readiness), never the
gate's. The composed real-node run cannot prove seat readiness (possession proofs) on its harness, so `exec_v2_test_admit_class_v1`
stays and now stands for that channel's admission; it still lives only in `cfg(test)` builds. The per-route split is already the
session's: the root earns on the legacy channel, every prefix and slice claim is a kernel-route claim the route gates and adjudicates,
and the legs are a re-division of the root's legacy-channel allocation (D2 above). No code of this lane calls the gate; when OPVB
integrates, `PalwRewardGateV1::channel()` names the channel each leg draws from.

**GAP-60 — the slice DA default on the real node.** A slice whose kernel claim's committed position is withheld is demanded by an
outside bond from the fresh verifier's finding; at the demand's deadline the route defaults the claim (`Unavailable`), and the
same block's sync defaults the slice, voids its suffix, the root and the REAL claim (`WorkSliceDefaulted`) and charges the root bond
nothing. Rule 4 (continuity) is exercised on the node too: a slice whose kernel job's prompt does not continue the stream is carried,
anchored and refused `PredecessorMismatch`, crediting nothing.

## 11. The user's Round / EXEC additional acceptance conditions (2026-10-10) — the activation gates they add

Source: `docs/palw-round-exec-additional-acceptance-2026-10-10.md` (adopted by the user; integrated at `f325d6696`). None of the eight
gates passes today, and the unified EXEC stays unarmable (`PALW_EXEC_PAYLOAD_V2_ARMABLE = false`) until all eight and every earlier gate hold.

| Gate | Owner | What this branch already shows (implemented; verified only where a run is named) | Open |
| --- | --- | --- | --- |
| SLICE | X8R (the Lead) | a plan is a partition with no gap or overlap (`a_plan_is_a_partition_with_no_gap_and_no_overlap`); more slices pay no more (`splitting_a_range_into_more_slices_pays_no_more`); a used index, a skip, an overlap and a replay credit nothing (`a_used_index_a_skip_an_overlap_and_a_replay_credit_nothing`); the root settles once and the allocation is conserved (`the_root_settles_once_…`); one job work id per root (`JobWorkAlreadyUsed`); the prefix nonce binds the root claim id (GAP-62); one budget per work, legs net of route reward (ADR-0176 D2, §10.9); node controls for two REAL roots, cross-index claim reuse and signed reveal replay after computation Final (§15.8, measured 2026-10-11); native Final/payment/recovery controls exist (§15.10; fresh execution pending) | a second composed write of the same Final claim row; fresh native root Final, all positive funds, material retention and restart/reorg acceptance; the controls' REAL trace seam and missing pruned/P2P evidence remain |
| RECOVERY (EXEC half) | X8R | restart / replay / IBD / reorg of the carriage and the ledgers (`t12_exec_v2_carriage`), the salted-seal and slice-verification reorg cases | the prefix stage across a reorg |
| BUDGET, WORK, WINDOW, NEUTRALITY, SPLIT, RECOVERY (budget half) | BUDGET (`palw_bond_budget_v1`) | EXEC carriers mint nothing (`B` = 0 per carrier); standalone kernel Q/R reservation and recipient reclassification, capped candidate draw and atomic ticket reservation are connected to the native fold (§15.10); fresh compiled core budget selection: 64 passed | composed consumer/resource/economic acceptance of the shared pre-draw cap and explicit fee-only execution cap (POLICY), split invariance and role-swap neutrality; finite signed material service across actual delayed payment; leaf/fold PASS does not close these gates |
| ECON | ECON | — | small/large model economics before/after the cap and window saturation, with criteria fixed before the run |

EXEC_TX / EXEC_SLICE weight, blue score and DAA contribution stay 0; a slice's verification and reward aggregate to its root.

## 12. WORK design correction — execution cost is not lottery weight (2026-10-10 audit)

This section resolves the unit ambiguity in §§3–4 and §10's prefix/range accounting.
`palw_exec_work_plan_v1` now feeds declaration acceptance, slice admission, node statement generation
and root-plan restart consistency through `palw_exec_v2_fold`. These rules remain behind the dormant
EXEC fence. The pruning / authorization history and release resource evidence below is still required for WORK.

### 12.1 Two quantities and a fixed cost basis

`W_REAL` is the REAL claim's accepted canonical lottery weight, including the attempt's lottery
scaling. `C_EXEC` is the registered IR's useful execution cost for one frozen semantic job. Never
compare a prefix `C_EXEC` with `W_REAL`, add the two, or multiply a suffix by the prefix's lottery
factor. Root chain-position credit continues to be exactly `W_REAL`; EXEC adds zero weight / DAA.
Work partitioning affects the allocation of the root's one funded reward, not the allocation itself.

Use the existing ten-dimensional canonical TIR work vector. The reference carries its seven
arithmetic dimensions in MAC-equivalents and its three traffic dimensions in bytes separately.
It uses the existing arithmetic sum for range coordinates; this is **not** approval of a final
economic price or a new byte-to-MAC coefficient. The accepted class fixes `AllNodesV1` or the
existing `MinSelectArmsV2` policy from the class's registration DAA; the producer cannot choose or switch it between
cells. Any future scalar price must have a versioned, checked, additive conversion of the whole
vector and pass the existing cost/arbitrage gates. Do not round each cell before aggregating.

### 12.2 Absolute positions and exact job scope

Freeze prompt length `P > 0`, REAL prefix decode `G_p > 0`, whole decode `G > G_p`, and strictly
increasing cumulative suffix decode ends `G_p < G_1 < … < G_n = G`. Obtain `P` and `G_p` from the
REAL context, and bound `P + G - 1` by both the accepted class's position limit and the registered IR's history bound. Bound `n` before allocation
by the existing 128-slice limit, and the cumulative arithmetic cost by the existing `2^48` root bound.
Whole job inputs and exact `G` must be authenticated before the declaration is accepted; the
producer's MAC interval alone is not that authentication.

All costs use **one** `PalwCanonicalExecutionFactsV1::uncached(P, G)` and the registered IR:

```text
prefix positions: [0, P + G_p - 1)
suffix i positions: [P + G_(i-1) - 1, P + G_i - 1), with G_0 = G_p
prefix vector + Σ suffix vector = whole-job vector, in every dimension
boundaries[0] = C(prefix); boundaries[i+1] = C(prefix) + Σ_(j<=i) C(suffix j)
total_work = C(whole job)
```

`post` / logits execute only where the original whole-job schedule requires them. Attention's
prefill/decode split remains at original `P`, and KV/history length remains the absolute position
with the registered history window. Recomputing the preceding prompt or history to build a slice
trace is physical overhead, not newly credited work. A slice that follows cumulative decode
`G_prev` must have exactly `P + G_prev` prompt tokens and generate exactly `G_end - G_prev` tokens,
in addition to the existing token-state, nonce, class, executor, output and evidence bindings.
The prefix job's maximum and actual generated-token counts must both equal the REAL context's
nonzero exact decode. A matching output commitment alone does not validate those counts.

### 12.3 Identity, plan material and refusal obligations

The authenticated semantic job must commit the accepted class / immutable computation identity,
canonical full inputs and any admitted cache state, the exact whole decode and execution/cost
versions. Keep its useful-work identity independent of output values, slice cuts, executor/bond,
carrier, root id and nonsemantic scheduling nonce. `job_work_id` now binds the immutable class,
the REAL job authorization and its authenticated input. It excludes cuts, output state, verification
metadata and continuation extent: one REAL job authorization opens at most one root, even if the
producer requests more or fewer continuation tokens. Declaration acceptance derives its input
commitment from the REAL job's public prompt. A new root or a different partition must not authorize
fresh useful credit. Legitimate repeated jobs require existing admission/ticket
authorization that binds fresh semantic work; neither a new root id nor an owner label proves it.

The existing signed cumulative-cost plan authenticates its generated-token ends without new wire
fields. All credited arithmetic coefficients are nonnegative and each accepted prefix has positive
cost. The committed logits node is never in a discounted Select arm, has positive arithmetic cost,
and runs once for each emitted token. Every subsequent token runs the same recurring occurrences
with nonnegative coefficients and nondecreasing capped history, so cumulative arithmetic cost is
strictly increasing after the prefix. Binary search
over the accepted context finds the unique decode end for each boundary; reject an off-grid value.
The search is bounded by `min(class_max_positions, program.history_bound)`, at most `2^21` positions,
and the plan by 128 suffixes. `palw_exec_work_plan_from_boundaries_v1` reconstructs the full vector and
scalar plan once these ends are found. The signature already binds all boundaries and total cost.

At slice admission and statement generation, the actual prompt and generated-token lengths derive
the absolute preceding / ending decode positions. Their cumulative costs must equal the root's two
boundaries. The slice's input commitment must match the actual prompt's token-state commitment;
repeated prompt execution earns no extra range cost. Restart consistency repeats the registered-cost
derivation and useful-work key from REAL inputs, and checks the actual lengths of retained slice jobs
against that one reconstructed plan. This does not solve missing pruned dependency material.
Reject missing admission material, unavailable cost version, changed shape,
inflated costs, out-of-class positions, overflow, empty/non-increasing cuts and a zero-cost cell.
No fallback to producer-declared work is permitted. The shape calculator retains its existing
saturating arithmetic; §12.5 bounds its intermediates for these validated EXEC inputs. The planner
also rejects saturated output dimensions and checked sum overflow. Wider formats or cost tables
require a new proof and version before use.

`JobWorkUse` must survive the earliest legal reuse window and pruning as authenticated history,
with an explicit finite retention / expiry rule from the canonical job authorization. Define its
cross-snapshot generation and reorg rollback with the existing carriage ledgers. Do not claim that
an indefinitely growing tombstone set is a bounded design or that a missing pruned row allows reuse.

### 12.4 Evidence and remaining implementation work

`consensus/core/tests/palw_exec_work_plan.rs` checks each cell against a position-by-position IR walk
for all seven checked-in architecture fixtures; coarse/fine partitions preserve the full vector
under both existing cost policies. It also exercises prompt replay, a tiny run claiming a large
cell, fabricated declared cost, invalid partitions/context bounds, out-of-artifact positions and a
valid 1,024-layer IR whose execution exceeds the root cost bound without enumerating its positions.
The signed-boundary roundtrip covers all seven architectures and both cost policies. The fold's
tiny-run attack passes the prior nonce/class/token/evidence binding but is refused by the new cost
scope; statement generation refuses it too. A signed tiny-run carrier also exercises node admission
without relying on the producer helper. A declaration with off-grid cost or a fabricated input
commitment is refused, while REAL lottery weight stays unchanged. The prefix exact-decode refusal
has a fold regression that accepted the wrong shape before the fix. Test results for each layer are
recorded in the design audit; these checks do not substitute for pruned history evidence. The
structural-cap test combines 16 blocks, 512 nodes per block, 1,024 layers, `2^21` positions and 128
suffixes, using small committable operations that fit the program byte cap. A separate valid
Concat/Slice DAG represents `2^200` paths to one weight with only 400 structural nodes; its work
derivation visits each node once rather than expanding those paths.

Section 12.6 implements finite authorization and retirement-independent history. The remaining WORK
work includes release measurements for the most expensive graph shapes and root populations,
source-header availability across pruning, and stronger multi-process pruned replay attack evidence.
BUDGET and WINDOW remain separate blockers: correct costs alone do not stop early payout
or retain post-Final collectible liability.

### 12.5 Arithmetic and algorithm bounds for the registered v1 IR

This proof applies to canonical normal-form programs accepted by `misaka-palw-tir::validate`,
the two existing work policies, cost table v1, and EXEC's checked `P + G - 1 <= 2^21` position bound.
The planner has compile-time guards on the structural limits and the cost-table entries below.
It does not authorize larger histories, tensor formats or prices.

At the shape calculator's samples `H = 1, 2, 3`, a computed tensor has at most `3 * 2^28` elements:
normal form bounds it at its positive window, and it has at most one H dimension. Static params
have at most `2^40` elements; every dimension is at most `2^24`, and every dtype is at most 16 bytes.
MatMul therefore has fewer than `2^54` MACs. For TopK, `input_elements * k = input_axis *
output_elements`, so its comparisons also number fewer than `2^54`, even when reading a large
param. Reduction/elementwise/transcendental counts are smaller. At most eight operands contribute
fewer than `2^47` bytes; all `node_cost` u64 products and sums are below u64 saturation.
Multiplying MACs by the weight width and the v1 arithmetic prices (1, 1, 4) leaves each sampled
work dimension below `2^58`.

The nonnegative affine coefficients `c0`, `c1` satisfy `c0 + c1 = sample(1)`. Summing at most
512 nodes gives each block coefficient sum below `2^67`. Select arm-only regions are disjoint or
nested: all uses of a member must remain in that arm until its Select. An inner region is wholly
inside an outer one that contains its Select. Ascending Select order discounts the inner region
first; the region accumulator has already included those preceding members when subtracting the
inner discount. Componentwise max removes no more than the credited arm union, and no addition
exceeds the undiscounted block sum. Thus neither the region calculations nor block accumulation
can saturate; discounting cannot increase a coefficient.

For at most `N = 2^21` positions and `H <= 2^21`, `sum H <= 2^42`. The triangular products in
`sum_h` are below `2^43`. A block dimension is below `2^67 * 2^42 = 2^109`, and fewer than
`2^11` occurrences give a whole dimension below `2^120`. Even summing all ten dimensions is
below `2^124`. Param inventory bytes are below `2^12 * 2^11 * 2^40 * 16 = 2^67`. All are within
u128. The planner separately enforces the much smaller `2^48` arithmetic allocation limit;
traffic bytes never enter that scalar without a separately approved conversion.

The boundary inverse builds one shape, performs at most 21 binary decisions and one endpoint
check per suffix, and then checks vector conservation. A vector evaluation visits at most 1,026
occurrences; it never enumerates positions or tensor elements. Structural weight-view traversal
uses a visited set, so shared Concat/Slice paths do not expand exponentially. The bounded
structural-cap measurement is evidence for this algorithm, not proof that every allowed graph
and 256-root restart fits the node's release latency budget.


### 12.6 Finite REAL authorization and retirement-independent job-use history

The original REAL header, identified by the native claim's accepted block hash, authorizes root opening
only while `max(source_daa, accepted_daa) <= opening_daa < source_daa + 100_000`. The sum is checked;
an overflow or a terminal `u64::MAX` deadline is refused. This is a fixed v1 authorization profile,
not a lease renewed by later acceptance, claim retirement or root opening. It does not shorten an
already opened root's signed execution/material terms or extend any collateral hold.

Section 12.7 captures this clock when the native claim is accepted. Root opening reads only that
authenticated state row; later header availability or caller-supplied bytes cannot change it.
The fold recomputes the carrying header's full hash, requires a REAL attempt algorithm, decodes the
attempt, and binds its attempt id, class, executor bond and recomputed execution anchor to the
native claim at capture. The anchor check applies when native attribution recorded an anchor;
a zero legacy native attribution is not itself a new native refusal. Missing or substituted bytes
are refused at capture. The cached header hash alone is insufficient. Core arithmetic fixtures
have an explicitly named test-only source-time seam at native capture; it is absent from the
production extras type. An execution anchor may repeat among same-template,
same-nonce-bucket sibling headers; uniqueness of a hash or a different claim id is not a replay proof.

`exec_v2_job_uses[job_work_id]` stores the first owner root, class, canonical REAL job, authenticated
input, source block and immutable source/expiry DAA. Native claim retirement removes the active
root/slices/job index but retains this separate authenticated history. A later root with the same
work key is refused while its recorded authorization is open. Once the record is deleted, opening
still requires a valid original authorization, so absence is never permission to renew old work.
Fresh authorization may repeat legitimate inputs; a new root id or owner label alone cannot.

The map admits at most 100,000 rows and refuses admission when full; it never evicts unexpired
history. A derived ordered expiry index contains only rows whose owner root has retired. A block
removes at most 256 expired rows, ordered by deadline and work key. Retained roots keep their rows
even after authorization closes, to authenticate their live execution ledger. Index updates are
incremental in ordinary writes and journal application/rollback; restore rebuilds the index from
primary rows. One encoded key and row uses 400 bytes before map overhead, so the structural cap is
40 MB of primary entries before carriage framing, clones, journal and allocator costs. This is
not an approved release-memory/latency budget or an economic admission cap.

The history has a conditional state-root domain `exec_v2_job_uses/v1`, optional carriage tail `0xF2`,
and inner `ExecV2Row` table 7. No new top-level object tag or existing root/slice wire field is added.
Empty history adds no bytes/domain; existing optional tails keep their encoding. The decoder refuses a count above the cap before reading rows and requires strictly ordered unique
keys. Duplicate or truncated tails and live roots without history are refused. Private armed snapshots from before
this profile cannot synthesize authorization terms; their migration/replay needs an explicit
procedure. Public activation remains refused. Snapshot authentication and journal rollback are
local recovery evidence, not proof of transport/pruning availability or multi-process P2P replay.
Local header presence must never decide root admission differently between a full node and a
pruned joiner. The original-history profile alone did not meet that requirement; the capture
mechanism below removes that lookup for newly recorded claims. Legacy migration and actual
pruned P2P transfer/replay remain required before activation.

The source header's DAA and the native claim's acceptance DAA are different authenticated facts.
No ordering between them is required: DAA is derived from the selected parent and counted mergeset,
not a wall-clock timestamp or a proof of the source/acceptance inequality. Neither fact may lie
after root opening. In particular, a source DAA above acceptance DAA waits until opening reaches
the source clock; it is not permanently excluded. Snapshot consistency checks the source clock
against the root's own opening DAA, even if the latest state point is later. The expiry continues
to derive solely from the original source DAA. Moving acceptance later cannot renew authorization,
and acceptance at or after expiry leaves no valid opening interval. Deleting expired one-use
history cannot authorize the same old source: opening at the deletion point is already expired.
This order-independent profile replaces the earlier unproven `source_daa <= accepted_daa` guard.
It does not assert that a reversed pair has been observed in a validated live DAG. Arithmetic
fixtures and actual multi-process DAG evidence must be distinguished.

### 12.7 Native capture of the original clock

`exec_v2_origins[native_claim_id] = { source_block, issued_daa }` is populated atomically with
native claim acceptance while the EXEC fence is active. The production walk retains the very
header used for admission and key derivation, for both its own attempt and merged attempts.
It supplies its serialized bytes to the capture check in §12.6 before native financial writes.
Root readers and folds never look up the original header or reconstruct its clock later.
A rider has no independent REAL carrying header and records no root authorization, as in the
prior header-backed root rule. Fresh native admission already needs its source header; recording
the clock does not introduce a new historical-header requirement at root opening.

The raw source clock is data, not a new cap on native admission, weight or income. Capture imposes
no new source/acceptance DAA ordering, authorization-expiry or overflow condition on native work.
Root authorization separately enforces the fixed finite profile in §12.6. Proving that profile's
source/acceptance ordering covers all eligible DAG work remains an activation requirement.

This map has no independent EXEC population cap: at most one row belongs to each live native
claim. Retirement deletes the row through the same claim writer and journal; the independent
job-use history continues until its original deadline. Restore rejects orphan rows, non-attempt
owners, zero source hashes and source blocks differing from the native claim. Decoding bounds
the count by the carried live-claim population before allocation and requires strictly ordered
unique keys. One primary key/row encodes to 136 bytes before allocator, clone and journal costs;
release resource approval for the live native population is still required.

The conditional root domain is `exec_v2_origins/v1`, optional carriage tail is `0xF3`, and inner
`ExecV2Row` table is 8. F3 is emitted before the existing F2 history tail. Empty origin state
adds no root domain or tail; existing root/slice wire fields and top-level object tags are unchanged.
Application, undo and authenticated carriage restore preserve the original clock. Observation
reports `authorizationOriginsTotal` separately from active roots and job-use history.

Claims accepted before this fence or restored without a recorded clock are refused for new
root opening with an explicit `AuthorizationUnrecorded` reason. A late header supplied by a full
node cannot backfill the row or renew the deadline. This is a fail-closed interim implementation,
not approval to exclude legacy eligible work permanently. Section 12.8 provides an explicit carried
proof for a missing clock; its availability and largest-header coverage remain activation requirements.
Already opened roots with F2 history keep their
authenticated original clock even when no F3 row exists; absence never authorizes a new root.

A real-node reader/fold regression removes the original header and admits the same signed root
from an authenticated snapshot, producing the same state root and delta as the full-header control.
It restores the header before further DAG processing. That proves local root-lookup independence;
it does not prove actual pruning, multi-process P2P transport, legacy migration or native Final.

### 12.8 Carried original-header proof for legacy claims

`ExecOriginRecordedV1` (object tag 131, under `palw_exec_payload_v2`) carries
`PalwExecOriginWitnessV1 { claim, header: borsh(Header) }`. It is ordinary fee-market registry
traffic and requires no new owner signature: the immutable carrying hash of the already accepted
native claim authenticates the full header. It cannot authorize a new claim or execution by itself.
The reader and fold use the same state-based admission function, without local header lookups or
clock extras. Claim presence, native attempt source, an absent clock row and the complete proof
checks of §12.6 precede the origin write. Existing recorded rows cannot be overwritten. For an
already opened F2 root, the supplied clock/source must also equal its authenticated history.
The object records raw original DAA, including when supplied late; Root opening still applies
the original finite deadline. Migration never replaces it with the migration/acceptance block's
DAA and never creates native income, weight, reservations or a longer material/collateral term.
Root signatures, job bindings, consent and every other eligibility rule remain required.

The ordinary decoder for proof headers checks all vector counts against bytes actually present.
At most 255 compressed-parent runs are possible; cumulative levels must start above zero and
strictly increase, and consecutive equal runs are rejected through `CompressedParents::try_from`.
This validation occurs before expanded hashing, which would otherwise panic on malformed Borsh
parent runs. Fixed fields follow the ordinary Header Borsh layout; the REAL commitment is bounded
by its existing 8,192-byte header rule and must consume the remainder exactly. The cached hash
remains untrusted. The same safe header decoder is used at fresh native clock capture.

The witness byte cap is the existing assembled-object budget less its 69-byte tag/key/length
overhead: `16 × 100,000 − 69 = 1,599,931` header bytes. Its decoder checks the declared length
before allocating. An empty header may decode and ride unjudged below the fence, preserving A2;
stateful acceptance rejects it after the fence. Tag 131 is in the new-kind/owner/wire-pin tables,
without changing any frozen live kind. Generic `ObjectChunk` carriage accepts this proof and
validates it only after complete assembly; partial chunks supply no authorization. Below the
EXEC fence, the completing payload is refused as the old build's unknown kind. The origin uses
the existing F3 / table 8 journal, with exact apply, undo and snapshot recovery.

This bounded implementation does not approve permanent exclusion of a legacy header too large
for that carriage. Prove coverage of every eligible historical header, or supply a bounded larger
migration transport, before activation. Compressed input size also does not approve hash latency:
one run can expand across 255 levels, so the largest proof can feed nearly 408 MB of parent bytes
to each header digest (and both full and pre-PoW digests are needed for a valid proof). Measure
that cost and the shared chunk lane's availability under load. Existing eight-slot chunk traffic
can delay a proof; no floor on timely migration is claimed. A producer/archive must supply the
original bytes before pruning destroys all copies, or an authenticated pre-activation migration
must preserve them. Actual multi-process activation/pruning/P2P recovery remains unverified.

Origin verification uses a byte-identical parent-run writer for its two digests. A repeated run
of at most 32,768 hashes serializes its hash bytes once, then writes the same length and bytes at
each expanded level. The scratch is at most 2 MiB per invocation and is discarded after that run;
larger runs use the original streaming writer. This is a performance scratch bound, not an
admission or historical-header size limit. Ordinary block identity, PoW and Round hashing keep
their original streaming parent writer; both paths share the unchanged remaining header preimage.
No representation cast or new digest domain is used. The original writer already borrowed the
compressed parents: the nearly 408 MB above describes repeated hash input, not a materialized
408-MB vector. The optimization reduces update/iteration overhead, not that cryptographic byte work.
It cannot substitute for worst-case release latency, resident-memory or shared-lane availability gates.

The origin verifier first checks the attempt algo, decodes the envelope against actual remaining
bytes, and rejects an attempt id, class or executor-bond mismatch before either expanded header
digest. Both envelope byte vectors are length-checked before allocation; no current admission
version/key/signature policy is added to this legacy provenance reader. These early checks can
only reject. A passing proof still recomputes the full accepted header identity, then the pre-PoW
digest and the existing anchor binding before exposing its source DAA. A cached header hash is
never trusted. All refusals retain `AuthorizationOrigin`, and the accepted predicate is unchanged.
An adversary can copy matching public envelope fields and still reach the identity digest, so
this ordering does not establish the worst-case CPU/fee bound or transport availability.

Resource accounting must cover all invocations, not just the origin reader once. For tag 131,
H1/rehearsal use the core's read-only Origin verdict: authenticate the exact state/proof/point
once, then run the same common object guards as the writer. Origin's writer has no additional
fallible operation after those guards and authentication. The query creates no transition builder,
state copy or discarded write journal. Extras are loaded only after authentication succeeds; the
node's no-V2-bundle refusal and the distinction between `NotAccepted` and `Refused` remain intact.
This removes the former second authentication from each successful H1/rehearsal call. It supplies
no validated flag, reusable permit or cache to a writer: actual object acceptance, block folding
and native capture continue to authenticate independently. Other object rehearsal paths retain
their writer. Any new fallible Origin writer condition must also enter the read-only verdict.

The block filter and the whole fold additionally share the same explicit Origin work-accounting
rule: at most **two eligible header decode/authentication attempts per block**, direct and
chunk-completed together. This is the initial dormant-lane ceiling, matching Origin's former
incidental two-completion limit below the grading-accounting fences; it is not a calibrated
latency, fee, throughput or availability approval. `PalwExecOriginBudgetV1` is ephemeral, not
state/carriage or an authentication hint. The node charges before the existing acceptance/writer
checks; the whole fold charges independently before its own header reader. Refused header work
is not refunded or reset by state restore. A new block starts a fresh budget. The node drops
excess attempts while the carrier block stands; a raw unfiltered fold refuses a third attempt.
A single-object rehearsal has a fresh budget, so its one proof always fits this ceiling.

Only cheap metadata is examined before charging: owning fence, nonempty bounded header bytes,
an existing Attempt claim and no clock already recorded. Passing these checks authenticates
nothing; the full identity and anchor checks still follow. Wrong nonce/class/body and other
eligible decode/authentication failures consume the attempt. The node rejects metadata failures
before cloning a writer. A completed Origin chunk has its own budget and consumes no
`FamilyCertified` grading slot. Before its owning fence, the inner remains undecodable and the
old unknown-chunk accounting remains unchanged. The inner-object classifier also refuses invalid
chunk count/index/size before assembly, matching the writer's structural limits.

Pool, template, reorg and chunk-completion repetitions, actual block folding and native capture
still need their own resource inventory. The standalone origin-reader resource probe does not
measure those composite paths. Eight active chunk groups are a concurrency bound, not by themselves
a bound on all validations/completions in a block or on unaccepted traffic. An inclusion fee does
not pay for a refused proof. Approval therefore requires bounded admission work and repeat-call
accounting at each actual consumer without weakening validation. Historical-header coverage and
permissionless migration must remain intact; a smaller arbitrary header cap is not a substitute.

For a carried header of `B <= 1,599,931` bytes, each parent hash consumes 64 encoded bytes and
each compressed run expands at most 255 times. Thus parent input is at most `255 B` per digest
(`407,982,405` bytes at this cap), `510 B` per successful reader or current H1/rehearsal query
(`815,964,810` bytes). These are conservative cryptographic input bounds, not allocated vectors,
elapsed-time bounds, per-block bounds or bounds for uncapped fresh native capture. Header footer
bytes, level length prefixes, decoding, extras, state loading and repeated consumers add work.
For **one acceptance-filter pass plus one committed fold** processing these carried proofs,
the conservative parent-input bound is `2 × (1,020B + 510B) = 3,060B`, at most
`4,895,788,860` bytes at the header cap. The direct path may check once at acceptance, once in
its rehearsal writer, then once in the committed fold; a chunk completion checks no more than
that. This bounds this particular sequence's carried-Origin cryptographic input, including
charged refusals, not raw chunk assembly/group hashing, cheap rejected metadata traffic, repeated
invocations of the sequence, fresh native capture, state clones/roots, elapsed time or node RSS.
The ceiling of two alone does not prove sufficient service rate or honest migration liveness.
The outstanding admission/rate/fee and historical coverage gates still apply.

The release origin-reader probe measures the actual public admission function on structural
fixtures. Its large pinned control is not native-admitted; its eight-refusal sequence is not
chunk/P2P or block admission evidence. A median and whole-process RSS observation cannot establish
maximum latency, incremental memory, participation probability, collectible fees or activation.

The node regression re-folds an actually admitted signed REAL header with EXEC dormant, proving
the native row is unchanged and has no clock. It then uses a legacy-shaped authenticated snapshot
with the same native data and prefix, deletes the actual source header from the node store, and
accepts the carried proof through the node reader/fold. The result equals the normal step on an
already recorded state, and undo restores the missing-clock snapshot. This is local provenance
and recovery evidence, not an actual historical activation, chunk transport or pruning drill.


## 13. BUDGET correction — root Final authorizes bound kernel fees

The kernel computation lifecycle and the EXEC reward lifecycle have different Finals. A bound kernel claim reaching
computation Final emits reward 0, remains `rewarded = false`, and retains its poster's escrow. It may verify the corresponding
prefix or slice but cannot create a producer payout before the root's REAL Final. No untrusted kernel route object can bind,
finish or cancel a hold: the EXEC consumer calls those hooks only after its execution-binding checks.

### 13.1 Rooted hold and retention

Kernel table 31 (codex used 27, G14R's verifier pay; renumbered at the R6781 merge) is `kernel claim digest → ExecRewardHoldV1 { owner_root, expires_daa }`.
Its collection domain is `misaka-palw/kernel/ledger-collection/exec-reward-holds/v1`; its root extension domain is
`misaka-palw/kernel/ledger-exec-reward-holds-extension/v1`. The extension is absent when table 31 is empty,
so all unarmed preset rows, existing wire records and historical roots are unchanged. Direct ledger roots, row roots,
restoration and the consumer's journal compose the same extension, after the beacon extension.

At most `256 × (128 + 1)` holds exist. Consumer consistency requires exactly one hold for the prefix and every accepted slice
of each open/complete bound root, no duplicate owner and no hold for a closed root. An already-paid prefix is refused at root
declaration; an already-paid slice is refused with code 29; a conflicting reward owner uses code 30. Existing refusal codes stay
pinned. Computation Final with no prior payout may attach. Binding to a different owner, finishing twice or finishing an expired,
non-Final or failed claim refuses without a write.

The escrow reservation remains on the poster's actual collateral until the root settles or cancels. Idle-job expiry cannot
release a held escrow first. The root's existing bounded expiry is the latest permitted root Final, including when all pieces
are verified but REAL Final is delayed by a court. The block's initial expiry sweep voids such a root and refunds its bound
escrows. This is a financial hold deadline, not a reduction in the native finality delay or the required fraud liability window.

The financial hold protects the entire **job escrow**, although table 31 names the bound claim. In PanelLicensed mode,
several producers can commit claims before the first Panel pass assigns the job. A pending EXEC-bound claim does not
win that assignment by binding: another producer may obtain coverage and computation Final first. That sibling Final
must emit reward 0, leave `rewarded = false`, and retain the held job escrow. It cannot bypass the hold by naming a
different claim digest. Derive the bounded set of held job IDs once per kernel closing tick and use it for both standalone
Final payouts and idle-escrow expiry. Unrelated jobs retain their normal payment path. This does not relax the one-job
Panel ownership rule; the uncovered EXEC dependency remains pending, then times out, and the root cancels/refunds
without paying either producer. Normal eligible bound work still uses the root-Final capped settlement hook.

OPV assigns job ownership earlier, at reveal. Its refusal of a competing claim proves that admission guard, not the
Panel financial guard above. These two verification modes require separate regression evidence.

### 13.2 Source and cap accounting

Let `A` be the root executor allocation after the native bond-budget clip, buyback and panel split. Aggregate each executor's
registered IR work before rounding, apply its frozen slice leg caps, and obtain gross shares `e_b` with `Σ e_b = A`.
For each bound claim, in prefix-then-slice-index order:

```
f_i = min(job escrow amount, actual poster collateral remaining, executor share remaining)
executor share remaining -= f_i
native leg_b = e_b - Σ fee payments to b
Σ native legs + Σ f_i = A
Σ actual poster debits = Σ f_i
```

A single ledger batch releases the FULL escrow reservation, debits only `f_i`, funds exactly `f_i` for the claim producer,
and releases the unused reservation to its poster. Re-sync poster collateral against the consumer's real bonds before the batch.
`apply_settlements` matches a FinalReward to PayJobEscrow of the SAME claim; another claim's debit cannot fund it.
The actual amounts, rather than `policy.claim_reward`, are saved in the existing prefix/slice `route_reward` fields at root Final.
When `A = 0`, every escrow is released and every producer leg is 0. No additional R reservation is created for a bound claim;
consuming the root's gross allocation once covers native and fee funding together.

All ledger rows, poster debits, funded vesting rows, exposure releases and the root `Settled` marker are journaled by the same transition.
The existing delta/carriage rules apply and revert them together. A second root Final returns no new payment. A voided root pays
zero; an incomplete root cannot invoke settlement. Default, conviction or expiry cancels every bound hold and releases escrow
without reward or collateral debit. A previously verified piece receives no partial payment from an unsuccessful root.

### 13.3 Remaining acceptance boundaries

Section 14 prevents a bound kernel reservation's ordinary expiry before the root's base liability/court path. Section 15 connects
settled-root verdicts and delayed native/slice/kernel payments in the fold; its remaining WINDOW boundaries still block activation.
Standalone kernel budget consumers, fee-only Round B policy,
market-fee R policy, whole-model soundness, economics, pruned authorization history and worst-case resources retain their
separate requirements in the full RFC6/7/8/12 audit. No preset or public activation validation is armed by this change.

## 14. WINDOW correction — a consumer cannot inherit expired collateral

### 14.1 Consumer floor and inclusive release

Before writing an EXEC reward hold or executor exposure, freeze the following floor in the bound prefix/slice claim's existing
`liability_until` row:

```
H_exec = root.expiry_daa
       + max(native window_court, kernel liability_daa)
       + kernel court_deadline_daa
       + kernel proof_grace_daa
liability_until = max(existing liability_until, H_exec)
```

`root.expiry_daa` is the latest possible REAL root Final, bounded by the existing root admission limits. The native window
protects the base post-root-Final evidence path. The deadline and grace allow a kernel material demand begun at the end of that
window to finish without making its public response unprosecutable. Every host addition is checked; an overflowing floor refuses
before a write. This is a retention floor on actual collateral, not a new reservation, a payout authorization or a soundness claim.

The trusted kernel hook refuses a paid, convicted, unavailable, timed-out or zero-reservation dependency. A computation-Final
claim may attach only while its existing liability is still open; an expired-but-unswept positive reservation is also refused.
Declaration, slice admission and the producer's statement helper make the same liability eligibility checks. A past expiry cannot
be reopened by attaching already released work to a later consumer.

Computation Final and a pre-Final availability default set `liability_until` to the maximum of its existing value and their normal
kernel horizon. Neither overwrites the consumer floor. A root fee settlement or cancellation removes its reward hold without
shortening this claim horizon. A healthy Final/defaulted claim's ordinary collateral release requires `daa > liability_until`;
the inclusive last day remains available for a proof. Conviction or an availability penalty can consume collateral earlier, and a
claim that times out without ever passing remains the existing terminal failure. Those outcomes cannot earn an EXEC reward.

### 14.2 Restoration and defensive failure

No row layout, wire field or new collection is added: table 31 still identifies the root and its expiry, and the existing claim row
roots the extended liability. Kernel restoration requires each reward hold's claim to retain a horizon at least as long as its
hold. EXEC restoration additionally rederives `H_exec` for every open/complete root and refuses a shorter dependency horizon.
The network's native and kernel time parameters must remain stable while such roots are live; any later parameter transition
must specify how outstanding commitments retain their originally admitted policy before that transition can be armed.

A verified piece whose reservation disappears while its liability remains open defaults the root; its frozen leg cap cannot make
lost collateral collectible. No partial native or job-fee payout follows. Release at the protected horizon remains finite.

### 14.3 Evidence scope and remaining WINDOW work

Real kernel lifecycle tests cover computation Final, post-old-horizon public conviction, a late material demand/default,
root fee settlement, row restoration, inclusive last-day retention and following-day release. The public proof in the kernel
test is constructed from the committed statement before the trusted consumer binding and filed later; it does not claim a fresh
post-binding outsider replay of the complete host. The actual-node suite verifies consumer binding, extended kernel Final
retention, existing outsider verdict propagation before root Final and replay. A fold regression refuses shortened restored
liability and an early lost reservation. The actual node still uses the existing REAL Panel admission seam and reaches ready,
not native root Final.

This floor alone does not close WINDOW. Section 15 addresses the first two items in the fold, with the stated evidence limits:

- Propagate settled-root prefix/slice conviction and post-Final availability default into native Final evidence and exact safe
  weight reversal, and burn every still-unreleased native/kernel/slice reward leg consistently.
- Hold every payout through the same collectible consumer window; current root-Final job fees and slice legs can still be paid
  immediately. Retain dependency records through retirement, pruned join, restart and reorg.
- Couple any native DA/court extension beyond the base window to dependency collateral, vesting and public material/provider
  TTL. Extending `liability_until` does not extend a provider's signed service commitment.
- Specify the worker's authorization for the consumer's longer retention and its capital cost before activation. A root's
  admission limit is a bound, not proof that the existing signed job contract has agreed to that obligation.

These requirements retain their separate acceptance evidence in the full RFC6/7/8/12 audit. Public fences remain dormant.

## 15. Composite reward vesting and settled-root retraction

This revision supersedes the immediate slice/fee payment statements in sections 10 and 13. A native root Final funds its
allocation once. Native producer/Panel/reserve legs remain in the existing native vesting row. Additional executor legs use a
separate root-keyed row whose `(bond, NativeSlice | KernelFee)` keys preserve their role and funding source. A Panel-seat-only
burn cannot accidentally treat a slice executor as a seat. Kernel fee entitlement requires the matching actual poster debit;
funding occurs at Final, while spendable payout waits. Rounded zero additional legs still retain a zero-value receipt while the
forced native row exists. A native actor-specific cancellation also retains this zero-value receipt until the remaining native
legs finish or are burned; cancelling the additional fund cannot strand that native row. This forces every composite root to
vest even if the independent rcore fence is off, while leaving
off-fence histories without an EXEC receipt unchanged.

The row freezes `final_daa`, `allocation_sompi`, fixed payout payloads and the maximum of `H_exec` and every stored dependency's
liability horizon. The native row's expiry is strictly after this inclusive horizon. Additional payments wait until that day
has passed AND the native row has left its second-clock/DA/freeze hold. An existing queue budget reserves up to two market keys;
the release planner merges existing payout keys and admits only the remaining new keys, partially draining an old additional
row before new native moves. Native and additional counter totals satisfy `created = live + moved + burned`. A retired root's
unpaid receipt still occupies the global execution quota, so admission reserves a Final slot before writing a new root.

Prefix and slice verdict sync includes Settled roots. A failed dependency voids the affected prefix/suffix and composite root,
reverses exactly the native Final's budgeted/scaled safe contribution and native evidence, forfeits this claim's minted Round
rights, and burns every unreleased native and additional reward leg once. Kernel prosecution supplies the offending executor's
collateral charge; this propagation creates no offence or extra slash against the other actors. Exposure returned at Final is
never returned again. A native Final reversal also closes its settled EXEC root and cancels the additional earnings; the native
actor-specific penalty paths retain their existing attribution rules and need the separate compatibility acceptance below.

Claim retirement waits strictly past the dependency horizon. Native Safe facts and the node's lifecycle/readiness paths use the
same effective deadline, taking the maximum with the original signed trace retention. The Final journal freezes the consumer
deadline for native evidence after live-root retirement; no producer-signed trace lease is silently rewritten. The optional
`0xF4` state-carriage tail (codex used `0xF0`, LG14-A's public FP jobs; renumbered at the R6781 merge) and a separate state-root collection carry additional rows. Existing native vesting row bytes,
existing note tags and empty EXEC state-root/carriage bytes retain their layouts. All mutations use the common transition
journal, including apply/revert and restore consistency checks.

### 15.1 Acceptance boundaries

Fold tests exercise native root Final, exact funding, prefix/middle-slice post-Final fraud/default verdicts, duplicate retraction,
inclusive horizon, bounded queue release, native retirement, frozen maturity evidence, malformed tails, shortened allocation/
horizon refusal, and per-transition apply/revert/carriage restoration. Verdict injections isolate propagation from prosecution;
real kernel lifecycle and actual-node tests establish prosecution and binding separately. Combining those tests is not an
actual-node post-native-Final public-proof demonstration. Pruned P2P provenance and worst-case release latency remain open.

The complete WINDOW gate still requires native DA/computation-court extension to retain dependency collateral and public
material/provider TTL, worker consent and capital cost, the full actor-specific/tier/B-3/RPC reader compatibility matrix, and
actual-node native root Final followed by public conviction/default and payment recovery. The complete reward RPC in §15.3 pages both native rows and independent executor funds. Standalone kernel budgets, R6/R7/R12 and economic gates
remain independent blockers. This change neither enables a public preset nor approves an activation height.


### 15.2 Additional executor funds under aggregate liability and B-3

AG-2 includes both additional `NativeSlice` and `KernelFee` funds of the forfeited bond. It burns only that actor's
unpaid legs, even after their clocks have spent while awaiting payout capacity. The unrelated root producer, Panel,
reserve, other executors and already-moved payouts remain governed by their existing attribution rules. The burn
journal identifies the actual aggregate offence key and original fund; `forfeited_sompi` includes these earnings.
A previously verified slice settling after its executor's final freeze cannot recreate either fee or issuance:
settlement accounts for creation and forfeiture atomically, retaining the allocation receipt while Final is in progress.

AG-3 extends still-unmatured additional funds with an actor-specific earliest-release DAA. It does not extend another
executor's additional funds or rearm spent clocks merely because their payout has not moved. Shared native row clocks
still apply to all funds as in §15; this extension is independent of that shared hold. The extension survives the end-of-block
freeze sweep and restart, releases at its lift, and is removed when the actor has no remaining additional fund.

B-3 reads each executor's inclusive liability horizon, actor extension and the unlatched native row's bounded V-4(a)
clocks. A halt, DA/queue presence or a spent native latch alone does not rearm collateral. Actual EXEC vesting also holds
its native and additional payees below R-core+, because composite Final forces vesting there. Without any EXEC receipt
that extra path is empty. The derived additional-payee index is maintained by reward writes and delta replay, rebuilt
on import and checked against the primary fund rows.

The optional actor-extension carriage tail is `0xF5` (codex used `0xF1`, codex G14's legacy answers; renumbered at the R6781 merge), the independent root collection is `exec_v2_reward_holds`, and delta
180 table 6 journals the extension. The earlier `0xF4` reward-row layout and action tags 0/1/2 remain unchanged; aggregate
forfeiture appends action tag 3. Missing, orphan, non-extending and malformed holds fail restore or committed-root validation.
These are dormant EXEC/capacity-liability paths; shipped activation heights remain unchanged. A funded-Final conviction-funnel
test establishes actor isolation and accounting, not admission of a public offence against the native/kernel claim.


### 15.3 Complete reward read and financial client compatibility

`getPalwRewardVesting` (op 241) takes one committed consensus snapshot. `stateRoot` identifies it;
`native` is the explicitly scoped legacy native component. Global `outstandingSompi` is native live money plus all unpaid
`NativeSlice` and `KernelFee` legs. It satisfies `created = moved + burned + outstanding` for vesting counters; reporter
income is reported separately and is never inserted into those counters. Whole-query outstanding, additional pending/ready
money, and collateral-holding sources are computed before either page is truncated.

The existing native `after` cursor is independent of `execAfter`, whose exact form is
`<final_daa>:<root>:<bond_txid>:<bond_index>:<slice|kernel_fee>`. Additional legs are ordered by those four typed values;
fund participates in the cursor so two funds of one actor cannot disappear between pages. Each page defaults to and is capped
at 500 details. `execRowsTotal`, money and hold totals always cover the entire match, including earlier/later pages.
A holding source is one native row or one additional `(root, bond)` obligation, regardless of its number of funds.
`lastLockDaa` is a DAA-clock lower bound, not a guarantee against a shared second clock. Each additional leg also exposes its
native pending/clock state, native expiry and bounded second-clock escape, own earliest DAA, B-3 hold and readiness.
Ready means the inclusive dependency horizon and own actor extension have spent and the native row is gone. It still awaits
payout capacity; it has not been minted. `inNextBlock` uses the fold's additional-fund planner after simulating the first queue
drain, with no new block objects. It is a snapshot forecast, not a promise about subsequent blocks or newly included objects.

Whole-query reporter pending and awarded amounts include details beyond the reporter cap. A chain query counts all pending/
awarded sources, a payee query retains existing reporter attribution, and a claim query does not attribute unrelated reporter
income. Decimal-string amounts preserve u128 sums on both transports and in financial client JSON.

Op 199's fixed binary layout remains unchanged. When any live additional EXEC leg exists it returns an explicit error directing
clients to op 241, rather than an incomplete financial answer. Both transports route the new typed request/response; malformed
selectors and either cursor are errors even on a non-V2 network, while a well-formed unavailable read echoes its selector.
The CLI vesting, wallet observation and operator snapshot use op 241;
financial CLI envelopes are version 2 and the dashboard adds decimal-string funds with BigInt; observed unpaid rewards are never selectable UTXOs.
A claim with no native row but unpaid additional funds is `exec_pending`, including after native retirement; operators keep it
pending until all funds move. Forced native EXEC vesting is displayed below R-core+ as well. This observation change neither
alters any consensus activation height nor changes historical op 199 field order or native monetary policy.

### 15.4 Finite native dispute admission for composite work

A native DA session or computation court cannot enlarge an EXEC dependency's frozen obligation by itself. While the root is
retained, the session bound is the minimum of the admitted base `H_exec` (§14) and every prefix/slice claim's rooted
`liability_until`. This differs intentionally from the reward/maturity reader's maximum. A longer-lived dependency cannot
substitute for another executor's collateral or material. If only a retired root's unpaid receipt remains, its frozen horizon
bounds admission; retirement already requires the inclusive dependency window to have spent.

R-core+ DA admission requires `deadline + window_challenge_at(opened_daa) <= bound`, with checked arithmetic. This includes
DA-5's post-session lock/vesting tail, rather than checking disclosure alone. The shared public admission reader and the fold
use the same predicate. Legacy DA admission and both native court-opening paths require their whole session deadline to fit.
DA re-keying checks again at Final before extending native vesting or signer locks. The network policy remains stable while
roots are retained (§14.2); this is not authority to change outstanding commitments through a parameter transition.

At the inclusive boundary a session can be accepted. One DAA beyond it is refused before accusation exposure/session writes;
no kernel collateral, liability row, additional entitlement, or signed provider commitment is enlarged. Restore also checks
open DA, legacy disputed phases and computation-court sessions against the same bound. Ordinary native work with no EXEC
root/receipt retains its existing admission and saturation behavior. No wire field, state-root collection or activation height
changes; this rule belongs to the dormant EXEC path.

This restriction prevents the extension mismatch but does not establish WINDOW or economic security. Admission of the base
consumer horizon itself still needs an explicit worker retention authorization, a sufficient signed provider TTL/public-material
contract, and its capital/retention price before activation. Supporting a session beyond that horizon requires a separately
specified, bounded authorization and funding transition covering every dependency and provider; there is currently no such
transition. A larger native class court window is not that authorization. Public native post-Final conviction, the complete
actor matrix, pruned material and worst-case resource/latency evidence remain separate acceptance requirements.

### 15.5 Provider terms and transferred post-Final material failure

A provider's tag-150 lease signs a claim-specific subject, reservation and `serve_until_daa`. Tag 153 moves material
responsibility; it does not authorize a consumer to extend that signed term. When EXEC binds an already-transferred prefix
or slice, its providers must satisfy the existing transfer predicate again through `max(H_exec, claim.liability_until)`:
at least two live leases of distinct non-producer operator ids, with enough total reserved collateral for the kernel claim.
A lapsed transfer or missing producer mapping fails admission. Operator-id distinctness is not proof of real-world independence.
Claims that have not transferred retain the producer's existing DA path; no model-artifact lease or acquisition gate is introduced.

The shared slice-admission reader uses the same provider predicate as the write hook. Refusal code 31 is
`verification_provider_horizon`; codes 0–30 are unchanged. This is a carrier lane refusal, not an error that invalidates
an otherwise valid block. The carrier reserves no slice/index or fee entitlement, and can be retried after sufficient new
signed leases are committed. Existing leases are neither rewritten nor lengthened, and a new lease alone does not move
responsibility. A transfer made after consumer binding already reads the claim's rooted extended horizon under tag 153.

Transferred material failure after computation Final can leave the kernel claim row in `Final` with its worker reservation
intact. EXEC therefore reads the provider subject's `lapsed_daa` as well as the kernel claim's outcome. The verdict sync's
journal input includes table-45 subject writes, not only claims-table writes. A transferred lapse inside the claim's
collectible horizon defaults its prefix/slice, voids the composite root, reverses native Final evidence/weight, and burns
all still-unreleased native/slice/kernel-fee entitlements through the existing single-settlement path. Computation conviction
keeps priority; provider failure does not convict or slash a worker. Restore refuses a retained live/settled root whose
provider lapse has not been propagated.

Provider expiry itself is lawful and uncharged. If a transferred claim still owes material and expiry leaves no live
provider, the claim lapses; a fully spent Final does not lapse merely because its completed leases are retired. This does
not resurrect a released reservation or promise an extended service term.

These changes do not close the complete WINDOW gate. Explicit worker authorization and the capital/retention price of
consumer binding, provider transport/retrieval through restart and pruning, full public actor/tier evidence and worst-case
resources remain required before activation. This section does not establish `p_check`, a conviction floor, economical
provider independence, or 128-bit soundness. All public activation validation and fence heights remain unchanged.


### 15.6 Worker consent to reservation and retention

An extra-executor allow-list is the root owner's permission, not the listed workers' consent. Opening a root checks that
listed bonds are distinct, canonical and active, but reserves no extra worker's collateral. An executor's first accepted
signature-bearing slice reserves the root's native reservation once, using that block's Work gate and raw second-clock
depth. Further slices by that executor reserve nothing more. Insufficient current room is lane refusal 32,
`executor_unfunded`; no slice/index, reward entitlement or consumer extension is written. Codes 0–31 are unchanged.

The node producer uses the authenticated tip's required second-clock depth, matching canonical fold admission.
`None` means the DAA-only profile; it is not an unknown-depth conservative value. A standalone legacy preflight
without that receipt uses `Some(u64::MAX)` and the existing bounded DAA escape, so it cannot release held capital
early by assuming a missing clock. The depth is the required settlement threshold, not an observed settlement count.

The accepted slice rows identify participating extra executors. Live-root exposure is re-derived from this distinct set,
not from the allow-list. Root Final/void returns exactly these reservations, including when the last slice is missing;
unused listed workers have nothing to return. Replay and carriage validation use the same projection. A retained live
root's reservation must match its native claim, its roster is canonical and each recorded executor remains authorized.
These checks do not permit a root owner to consume a third party's bond merely by naming it.

A worker's kernel job uses the v2 slice-nonce domain. Define the canonical terms before signing:

```
H_exec = expiry + max(native_window_court, kernel_liability) + kernel_court_deadline + kernel_proof_grace
policy_root = H(retention-policy/v1; native_window_court || Borsh(full kernel LedgerPolicyV1))
nonce_v2 = H(slice-job-nonce/v2; nonce_v1 || expiry || H_exec || executor_exposure_sompi || policy_root)
```

Additions are checked and reject overflow/MAX. Integers use canonical little-endian bytes (DAAs u64, exposure u128),
with full 64-byte digests. The executor's signed kernel claim commits its job id, which commits this nonce; its signed
WorkSlice names that same claim. An old v1 nonce is not a fallback. A truthful trace/valid signature cannot attach to a
different expiry, reservation or policy. Fresh consent requires a new job id and signed claim. The root bond's own prefix
retains its existing nonce: the same bond directly signs the declaration including expiry under the network's fixed policy.

A job prepared before root declaration must preserve the proposed expiry when filling the subsequently obtained
prefix claim id. Refreshing expiry after job signing changes the authorized terms and requires a fresh job and claim.

Admission, the producer statement helper and restoration check the v2 terms against the actual rooted interval and
network policy. The pure worker helpers also prepare jobs before declaration. Op 240 appends `retentionTerms` and
`nextJobNonce` from the node's actual native window. Exposure is a decimal sompi string, not a lossy JSON number.
Missing policy/window data produces unknown terms/nonce; a terminal root has no next nonce. The historical observation
wrapper cannot infer a native window and must not fabricate one. These fields do not create consensus authority.

This profile is required under the still-dormant EXEC-v2 fence. No public activation height, Round ticket rule, wire tag,
root/slice carriage layout or state collection is changed. Historical v1 hash/work-binding helpers remain for readers;
production admission uses only v2. A private harness that bypassed public activation validation must rebuild old unsigned
retention fixtures; this is not a migration of a publicly armed EXEC-v2 network.

Cryptographic consent to a finite reservation/retention term is not a sufficient price contract. Workers must inspect the
terms before signing. A minimum offered compensation, capital/retention cost, abort/timeout economics and measured class
costs remain required; neither voluntary signing nor a policy hash proves profitability or positive detection probability.
This section does not resolve the full WINDOW/ECON or pruned public-material/actor/resource acceptance gates.

### 15.7 Worker price criterion and allocation freeze

Worker computation Final is not the event that pays an EXEC-bound job. Collection requires the native root's Final and
survival of every applicable dependency/default and vesting condition through release. If the native root expires,
§13 cancels its fee holds and returns the remaining job escrow to the poster. This is no worker income, even when all
worker computations were truthful and reached kernel Final. Their retained kernel liability is not cancelled with the
root. A signature authorizing these terms proves consent; it does not establish a profitable price.

Fix the criterion before a class measurement or model comparison. For an honest worker define:

```
C_upper = compute + recomputation + material/acquisition + carriers/filing
          + capital_rate * distinct_capital_time_area + other_outcome_costs
p_collect_lower = proven lower bound of the JOINT native-Final-and-collectible-at-release event
M_net_min = funded minimum actually collectible on that joint event, after all clips and charges
p_collect_lower * M_net_min >= C_upper
required_M = ceil(C_upper / p_collect_lower)   // only if p_collect_lower > 0
```

Costs include abort/default outcomes, retention after a root's cancellation, transport and retrieval conditions, and
second-clock/exit delay. Count each locked unit of capital once at each instant, using the actual composite reservations
and holds; adding every named row's amount can double count the same collateral. A finite DAA horizon is not a proved
wall-clock bound when chain progress or material retrieval is unavailable. Cost/time assumptions and their evidence
must therefore be explicit; an unbounded or unmeasured relevant cost remains unknown.

`M_net_min` is a minimum, not the root's allocation ceiling, a displayed leg estimate or the poster's gross job escrow.
It must cover Native BUDGET clipping, each executor's share, all fee/issuance substitutions and applicable deductions.
The fee path replaces native issuance inside the one allocation (§13); it cannot be added to native income a second
time. A future minimum-price contract must bind the funded amount, work scope, worker, policy, deadline and hold/release
conditions before signature, refuse insufficient funding before reserving the worker, and make its stated minimum
enforceable at settlement. §15.6's retention nonce does not currently commit such a minimum. No new approval or money
parameter is inferred here.

`p_min` for conviction and `p_check` for checks are different quantities. Native Final and healthy-release marginal
floors cannot be multiplied without a justified independence argument. With only two marginal floors `a` and `b`, the
conservative joint floor is `max(0, a + b - 1)`. If the actual joint floor is zero and honest work has positive cost,
no finite Final-only conditional offer finances that cost. Missing cost, collection or minimum-contract evidence yields
UNKNOWN, not a positive class verdict. A zero-cost example is vacuous; it is not economic security approval. Quotes
above the u64 sompi range are unrepresentable and must not wrap or clamp into a payable offer.

An EXEC root also freezes the native claim's existing allocation basis. A Provisional lead can normally split its
escrow among riders within the rider window, but once a retained EXEC root names that lead, both the cheap rider reader
and the full writer refuse that split before financial writes. Root objects are applied before queued rider batches:
opening a root and attaching riders in the same block is refused in either object order. Refusal skips the rider batch
without invalidating the block. Legitimate rider splitting before root declaration is preserved. This prevents later
re-attribution of a root's budget; it does not turn that budget ceiling into a guaranteed minimum contract.

`scripts/sim/misaka-palw-econ-verifier-model.py` §14 checks exact-rational arithmetic, one-sompi underpricing, omitted capital
cost, zero collection, dependent outcomes and overflow using declared ASSUMED scenarios. It supplies no measured class
cost, collection floor, funded minimum or policy approval. The node expiry regression uses actual signed kernel runs
and Final transitions, with the native REAL admission harness's synthetic trace commitment/Panel seam; it proves the
worker/non-native-Final payout rule, not a production owner-only veto or an all-honest native Final probability floor.
The allocation-freeze regression exercises the core reader, writer, full fold, both audit profiles and both object orders,
including journal replay, undo and carriage restore; core header signatures remain a fixture seam.

No public fence height, live Round ticket rule, wire tag or carriage layout changes. WINDOW/ECON remains unapproved until
the evidence and enforceable contract above, native post-Final public-court paths, material/restart/pruning and resource
acceptance conditions are closed.

### 15.8 Public prefix and kernel-claim replay controls

The prefix job nonce binds the native root claim, canonical job, prefix work and kernel plan. A second REAL claim of
the same class and producer cannot name the first root's prefix. The declaration must keep the second REAL's own
input, output and initial boundary; changing those to match the stolen prefix is not a valid positive control.
`x8_g14_a_second_real_root_cannot_reuse_the_first_roots_prefix_claim` checks the named `job nonce` refusal and submits
the signed declaration through node blocks. Its positive control posts, seals and reveals a distinct prefix for the
second REAL and opens that root with its original plan. Both reward holds must retain their independent root owners,
and a second node must replay to the same state. The control posts only the needed prefix: unrelated uncarried slice
claims must not exhaust the real route's concurrent-admission caps or justify relaxing those caps.

The honest-slices node regression also tries to reuse slice 0's claim at index 2, whose authorized executor is the
same bond. It keeps the root's next predecessor to reach verification binding, checks the durable refusal for that
carrier, and then admits the genuine slice 2. Once all four kernel computations have reached Final, it carries each
genuine signed reveal twice more in fresh transactions and runs additional closing ticks. The root's verified work,
slice stages and caps, kernel claim rows, escrow reservations, reward holds and public Final receipts must remain
unchanged. The attack lane block and duplicate-reveal transactions are included in the independent node replay.

A kernel Final is generated by the closing tick; there is no external Final object. Public reveal replay and
repeated closing ticks therefore test the reachable carrier path. They do not inject a second journal write of an
identical Final claim row. `a_repeated_kernel_final_credits_a_slice_once` retains that separate fold-level control.
These regressions do not prove native root Final, a second composed Final-row write, settlement after reorg/restart,
or a pruned/P2P join. The module's native admission/Panel and synthetic native trace-commitment seams still apply.
SLICE and the other acceptance gates remain open until the full requirements are verified; no fence is activated.

On 2026-10-11 the final EVM-enabled consensus run of the two named node controls passed: **2 passed, 0 failed,
0 ignored**, build 2m08s, runtime 100.69s. The separate consensus-core prefix-reuse and repeated-Final-row controls
passed: **2 passed, 0 failed, 0 ignored**, cached build 0.31s, runtime 0.09s. Commands and intermediate fixture
failure are recorded in the four-RFC design audit. Earlier candidate runs are not additional independent coverage.

### 15.9 Computation-Final recovery and public conviction

`x8_g14_kernel_finals_holds_and_credit_survive_restart_and_two_actual_reorgs` uses an on-disk consensus database.
Its prefix deliberately commits a false matrix value; the slices execute truthfully. All four OPV computations reach
Final unrefuted, so the bound root is ready but the native REAL claim remains live. This is an optimistic fraud case,
not an honest native Final control. Capture the root/slice credit and caps, four claim/escrow/hold rows, Final facts,
each selected block's delta root, collateral, slash/bounty state and the complete virtual UTXO set.

Drop Consensus and reopen the same database. Compare the stored state before a new block can regenerate it.
Then exercise two actual selected-chain reorgs: a public cold-checker proof convicts the prefix after computation
Final and cancels the root; the unrefuted branch returns and restores its original credit and funded holds. Inspect
the actual conviction journal: the same claim keeps its Final lifecycle state while its conviction flag and remaining
reservation change. The consumer must treat that write as a failure, never as additional verified credit. Returning
to the unrefuted branch must undo the slash and queued bounty, restore every escrow/hold and leave credit unchanged.

Keep the existing strict economic reorg rule. The incumbent depth must fit its two-DAA shallow tie exception on each
reorg; extra heartbeat blue work cannot authorize a deeper all-economic tie. Exchange the competing DAG tips before
requiring full virtual-parent/UTXO agreement: common selected sink alone does not establish equal virtual state.
The UTXO read must reject truncation. Selected-chain PALW/delta comparisons retain their separate scope.

This control does not inject a duplicate identical Final row; the public proof changes its verdict/reservation fields.
The node admission/Panel and synthetic native trace-commitment seams from the module still apply. Native root Final
and capped settlement, their subsequent public-court/recovery paths, OS-crash atomicity, deep economic reorgs and
pruned/P2P joins remain acceptance requirements. No public fence or legacy Round rule changes.

On 2026-10-11 the final EVM-enabled node command containing this control, honest slices and the two existing standalone
kernel restart/reorg controls passed: **4 passed, 0 failed, 0 ignored**, build 2m02s, runtime 38.52s. The four-RFC audit
records the rejected deep-heartbeat fixture, intermediate compile repairs and candidate run separately. None substitutes
for the native-root Final and settlement requirements above.

### 15.10 Native Final, all positive funds and root-exact material clocks

The native consumer controls in `native_final.rs` retain the module's explicit REAL trace admission and lottery seam.
They use signed artifact readiness, the chain-derived native Panel, signed V3 receipts and the native coverage assembler
to reach root Final. One control compares actual poster fee debits, capped kernel fees, conserved additional/native
funds, physical DB reopen and duplicate licence application, then submits an actual public prefix proof and checks
that each unpaid fund burns once. Another reverses that public conviction through an actual shallow selected-chain
reorg, restores the funded state and reapplies the proof without another fee charge. These are settlement controls;
they are not honest trained-model execution, separate-process transport, OS-crash or deep-reorg proofs.

The positive-fund control reaches two actual native Finals, satisfies the private second clock, waits for real queue
drain into coinbase, and checks every positive native, Panel, EXEC-slice and kernel-fee amount against its exact queue
receipt. The receipt's reusable R deadline is the physical payment DAA plus the private W436, not Final or enqueue.
The test fixes its private D1, W and counterfactual whole-escrow pricing before genesis. They are unapproved mechanics
values; zero compute-priced toy rewards are not relabelled as positive production economics.

The 2026-10-13 extension enables reporter V2/material V3 before genesis in this same positive-money control. Each
native root and authenticated kernel dependency must retain material with `Some(MAX)` while actual payment remains
unknown, then use the last positive-fund payment plus W, survive physical DB reopen and release at that boundary.
Another root of the same recipients must not create a perpetual hold on the completed work. Existing exact debit,
coinbase, licence replay and independent block replay assertions remain. Fresh execution of these native controls is
pending; their source presence and formatting checks are not a PASS.

Fresh execution of the existing post-genesis-repair core binary on 2026-10-13 passed 64 budget cases and 89 EXEC-fold
cases (one existing ignored resource case). `plan_kernel_claim_budget_v1`, `hold_kernel_income_v1` and authenticated
EXEC transfer share the kernel fee's Q/R maximum without creating B/F or a second fee. `bond_budget_round_draw_v1`
caps candidates by remaining rights before drawing and atomically refuses all tickets when any reservation fails.
Those fresh leaf/fold results establish their tested scope, not complete composed native consumer acceptance. Raw logs
and the new consumer's exact evidence boundary are retained in [the material acceptance record](rfc08-native-material-acceptance-2026-10-13.md).

Root-exact payment provenance does not supply signed provider consent beyond a finite TTL, a funded minimum worker
price, worst-population resources or pruned public-material availability. The shortest-dependency native-session guard
continues to refuse an overlong path; it does not authorize a new retention extension. Full WINDOW/ECON/RECOVERY gates
remain open until their complete acceptance evidence exists. No activation height or public parameter is changed.

The material GC API must not treat unavailable state as an absent obligation. The 2026-10-13 repair pins with MAX
on read failure/missing state, readable old-tip fallback, a changing committed sink or an incomplete/changing pruning
import generation. Complete matching snapshots retain their actual absence or physical payment plus W deadline;
dormant profiles remain unchanged. Janitor and foreign-copy GC use this same oracle. Two direct production-helper
tests pass and both fail under the former `.ok().flatten()` decision; the native control's actual API, DB-reopen and
release-boundary assertions are pending integration execution. This prevents a read failure from authorizing deletion,
but does not extend provider consent or establish a finite funded storage contract.

The first fresh EVM/private-integration native selection on 2026-10-13 compiled successfully and terminated with
two passing funded-Final/recovery controls and one failing positive-fund control after 2495.04 seconds. That control
incorrectly required the full shared Panel/EXEC-slice queue aggregate on its first intermediate row. Native vesting
and EXEC release occur in separate ticks; a partially consumed queue is not a completed total. The corrected test
rejects any excess R and waits for the exact predeclared aggregate to physically drain before assigning completion.
Exact coinbase cash, all payment+W clocks, material release, DB reopen and replay remain mandatory. It additionally
logs frozen deadlines and actual heartbeat progress with finite stall guards. The corrected positive-money run is
pending; [the full failure and evidence boundary](rfc08-native-material-acceptance-2026-10-13.md) are retained.

### 15.11 Paid native material terms cannot exceed signed EXEC retention

A native paid-consumer contract freezes its `material_until` at native claim acceptance. Its service/dispute/material
spans are not the EXEC declaration's expiry. Previously root admission compared provider leases with the EXEC-derived
liability floor alone, while native material reads and retirement separately retained the work through the potentially
longer paid-consumer term. A local GC pin does not authorize workers or providers to serve beyond their signed term.

Root admission now requires `paid.material_until <= H_exec`, where `H_exec` is the existing declaration-expiry-derived
retention horizon under the fixed native/kernel policy. Refusal occurs before binding kernel fee escrow, transferring
recipient R or writing the root/work-use rows. The owner can propose and sign a sufficiently long root expiry before
admission; accepted worker jobs then commit the same existing retention terms through their v2 nonce. Transferred
provider leases still have to cover that whole horizon in the existing provider check. No lease, contract, Final or
physical-payment clock is rewritten. Retained non-void roots repeat the same check on restore/replay. Claims without
a paid native contract take the old read-only path, with no wire or root collection change.

The new fold control obtains its native paid terms from ordinary acceptance and admits production lease/transfer
objects with unchanged exact provider terms. A paid material horizon one DAA longer than the signed EXEC horizon must
refuse with no parent carriage/fee/provider mutation; an exact bound must open, preserve provider rows and round-trip
its delta, rollback and carriage. That fold harness explicitly retains its header-signature seam. A separate native
control sends ML-DSA-signed provider leases and DA transfer through real carriers, refuses a short signed declaration,
then re-signs its expiry to the exact accepted native material bound and requires successful admission with the same
provider terms. Both new controls are pending fresh compile/execution; source presence is not acceptance evidence.

This guard closes a specific admission mismatch. Physical payment plus W can still be unknown or later than a finite
contract. No maximum physical queue latency, profitable worker offer or provider renewal/funding transition is inferred.
The full consumer, pruned material and economic acceptance gates remain open and public presets remain dormant.
