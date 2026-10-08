# RFC-0008 implementation spec v1 — unified PALW EXEC lane (DRAFT, NOT IMPLEMENTED)

Status: revised design, 2026-10-08. Baseline: `MISAKA-BTC/misakas` main `282355ba9`.
No activation height, runtime change, deployment or fingerprint change is authorized by this document.
The type and fence names below are proposed names, not declarations that exist in main.

This document implements the direction of [revised RFC-0008](../../rfc/0008-palw-claim-backed-consensus-blocks.md).
The former algo-11 design body has been deleted. Only its [v0 test results and unrun cases](rfc-0008-v0-test-record.md)
are preserved; they are not v1 requirements or v1 validation evidence. Main's active heartbeat, BASE-0, clock, REAL admission and transaction-permit rules are the baseline.

Implementation record (2026-10-08, branch `rfc8/x8-exec-v2`): dormant code for this spec exists behind the unarmable fence
`palw_exec_payload_v2`. What is built, what proves it and which gates remain open are in
[rfc-0008-v2-implementation-record.md](rfc-0008-v2-implementation-record.md). This spec stays normative and every gate in section 9 is
still open.

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

## 10. Amendment 1 (X8R, 2026-10-08): the slice verification route and the five design decisions

This amendment closes the choices sections 1–7 left open. It is normative for the implementation on `rfc8/x8r-review`; it changes no
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
   bound kernel class; its job's `nonce` is the **slice job nonce** `H("misaka-palw/exec-v2/slice-job-nonce/v1"; root claim ‖ index ‖
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
partial payment is made (section 6: "no independent prefix payment"); the extra executors' exposure returns; the job's one-use
tombstone stays until the claim retires. A slice that was `Verified` and whose claim is convicted later (inside the route's liability
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
together are held under the same reservation. The legs stay unvested (`add_panel_payout`): vesting would add a second, V2-side forfeiture
path for a fault the kernel route already convicts.

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
