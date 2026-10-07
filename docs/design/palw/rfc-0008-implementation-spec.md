# RFC-0008 implementation spec v1 — unified PALW EXEC lane (DRAFT, NOT IMPLEMENTED)

Status: revised design, 2026-10-08. Baseline: `MISAKA-BTC/misakas` main `282355ba9`.
No activation height, runtime change, deployment or fingerprint change is authorized by this document.
The type and fence names below are proposed names, not declarations that exist in main.

This document implements the direction of [revised RFC-0008](../../rfc/0008-palw-claim-backed-consensus-blocks.md).
The former algo-11 design body has been deleted. Only its [v0 test results and unrun cases](rfc-0008-v0-test-record.md)
are preserved; they are not v1 requirements or v1 validation evidence. Main's active heartbeat, BASE-0, clock, REAL admission and transaction-permit rules are the baseline.

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
