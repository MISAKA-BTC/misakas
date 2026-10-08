# OPV ↔ PALW Work Beacon: the startup cycle, a non-circular bootstrap, derived eligibility, grinding

Agent OPV-BOOT, branch `opv/bootstrap-beacon` (from the Lead's `9f777c3ca`). Everything here is **dormant** behind the existing fences
`palw_probabilistic_constraints_v1`, `palw_panel_free_v1` and `palw_signed_registration_v1`, which `validate_palw_v2` refuses at every
real height; no testnet-12 params or schedule id moves (every fence is `None` on every preset and hashed Some-only). Amounts are BILI
(ADR-0174; `SOMPI_PER_KASPA` is the legacy name of 1 BILI = 10^8 sompi).

## 1. Verdict: is there a startup cycle?

**Today (at `9f777c3ca`): no hard cycle, because OPV admission is a manual list.** The fence's `admitted_classes` is the only source of
OPV eligibility, an OPV claim draws no randomness (an outsider checks with its own salt, `misaka-palw-kernel` `OutsiderV1`), and so the
first OPV Final needs nothing but the list.

**With the ruled, DERIVED eligibility the cycle is real** — and it is not a deadlock of one class but an empty least fixed point:

```text
conformance passed(V)  ⇐ sampled evidence ⇐ beacon locked(ctx of V) ⇐ k Finals of profiles in EP(V)
EP(V) = OPV-eligible classes at V's commitment          (OB-P0: palw_onboarding_fold_v1::apply_conformance_committed_v1)
OPV-eligible(X)        ⇐ conformance passed(V') for the V2 class V' bound to X     (the ruling)
OPV Final(X)           ⇐ OPV claim(X) ⇐ OPV-eligible(X)
```

From genesis no class has passed conformance, so `EP = ∅` for the first commitment, its beacon can only end `BEACON_UNAVAILABLE`, nothing
passes, nothing becomes eligible, and every later commitment sees `EP = ∅` again. The exclusion of the candidate from its own beacon
(`excluded_profiles`) removes the self-loop but not the global one. RFC-0010's V3 Panel draw (`SubjectKindV1::PanelAssignment`) waits on
the same Panel-independent Finals, so it inherits the cycle. §3 encodes this as a graph and a test proves both statements: **without a
beacon-free conformance path, `OPV_ELIGIBLE` is unreachable from genesis; with the complete-check path of §4 it is reachable, and no
node depends on itself.**

## 2. Who consumes the beacon, and from what

| Consumer (subject kind) | Where | Sources it may take | Status on the node |
| --- | --- | --- | --- |
| Model / kernel conformance (`ModelConformance`, `KernelConformance`) | `palw_onboarding_fold_v1` (107 freezes the context, 109 judges, the tick closes) | `RealUsefulWork` Finals of the OPV classes **eligible at the commitment**, minus the candidate under every mode; `PanelIndependent` | wired (OB-P0); sources now derived (§5) |
| RFC-0010 V3 Panel assignment (`PanelAssignment`) | `palw_panel_beacon_v1::verify_panel_beacon_v1`, `palw_panel_v3_fold_v1::ChainPanelBeaconHistoryV1` | `PanelIndependent` Finals only (contract rule); approved scheme required | dormant: `approved_panel_beacon_policies_v1() = []`, the chain history exports only V2-lattice (Panel-licensed) Finals and `eligible_profiles = ∅` → always unavailable. Feeding it the route's OPV Finals of derived-eligible classes is a later wiring (§9 GAP-B3) |
| K2 per-claim check (`ClaimVerification`) | `misaka-palw-kernel::claim_subject` | any qualifying Final | **not wired**: the route's interim seats are seeded by the claim id (`palw_kernel_interim_seed_v1`, grindable, stated); an OPV claim has no Panel check and draws nothing |
| RFC-0008 work slices (`WorkSlice`) | spec §6; branch `rfc8/x8-exec-v2` held out | `RealUsefulWork` only — an EXEC work slice is refused **by kind** as a source, so slices can never seed slices | not on the integration tree |
| Public prosecution (`PublicProsecution`) | contract only | — | supplemental checks only; never a prerequisite of an exact proof |

Never a source anywhere: heartbeat, BASE-0, EXEC tx/slices, receipt-only material, provisional attempts, Panel receipts, block hashes,
signatures, DNS/BFT/validator randomness (`misaka-palw-challenge` `eligibility_v1`, by kind).

**What OPV eligibility and rewardability need** (§5): Active kernel; conformance passed (G14_ELIGIBLE or later) for the V2 class bound to
the class under either mode; the kernel class stands (it registered only after `PUBLIC_PROSECUTION_COMPLETE`); a live (Matured/Final,
unrefuted) artifact binding = public DA; the class's prosecution bounds fit the OPV carriers, court budget and censorship-cost rule; the
challenge policy its conformance was decided under is verified (the network's, structurally valid, effective bits ≥ the fence's floor,
§7); not on the fence's deny-list. **Rewardability** of an OPV Final follows: a claim of a class commits only while the class is
eligible (§5.3), so every OPV Final — and the Final reward G14-R4's escrow releases — belongs to a claim committed under eligibility.

## 3. The dependency graph (encoded)

`consensus/core/src/palw_opv_bootstrap_v1.rs` holds the graph as data: `PALW_OPV_DEPENDENCIES_V1`, rows of `(node, requirement,
enforced by)`; a requirement is an AND of predecessors, a node with several rows is an OR of them. Each row names the gate function that
enforces it, and the derived-eligibility predicate's refusal reasons (`OpvIneligibleV1`) map one-to-one onto the predecessor rows of
`OpvEligible` (a test fails if a reason has no row or a row no reason). The test walks the least fixed point from `Genesis`.

```text
Genesis ─► KernelActive ─────────────────────────────────────────────────────────────────────┐
Genesis ─► V2Registered (108) ─► ArtifactMatured (104 + window) ─► ArtifactFinal                │
                                   │                                                            │
                                   └─► KernelClassStands (tag 1 / 13; PPC gate) ─► KernelBound (106)
KernelBound ─► CommittedSampled (107) ─► EligibleSourcesNonEmpty ─► BeaconLocked ─► SampledEvidence (109 Post) ─► WindowClosed ─┐
KernelBound + CompleteQualified ─► CommittedComplete (107) ─► CompleteEvidence (109 PostComplete, judged in the fold) ───────────┤
                                                                                                                                ▼
                                                                       ConformancePassed ─► G14Eligible ─► (ArtifactFinal) ─► V2Active
G14Eligible + KernelActive + ArtifactMatured(live) + BoundsFit + PolicyVerified + NotDenied ─► OpvEligible
OpvEligible ─► OpvClassRegistered (13) ─► OpvClaim ─► OpvFinal (PanelIndependent) ─┬─► EligibleSourcesNonEmpty (other classes)
                                                                                   ├─► PanelAssignmentBeacon (+ ApprovedPanelScheme: ∅) ─► V3PanelBound
                                                                                   └─► WorkSliceBeacon (RFC-0008, not wired)
```

| Node | Requires (AND) | Enforced by |
| --- | --- | --- |
| `KernelActive` | Genesis | `KernelScheduleV1::standing_at` (route template) |
| `V2Registered` | Genesis | tag 108 / V2 registration |
| `ArtifactMatured` | V2Registered | `apply_artifact_bound_v1`, `onboarding_attested_roots_v1` |
| `ArtifactFinal` | ArtifactMatured | `ArtifactBindingRowV1::state_at` |
| `KernelClassStands` | ArtifactMatured, KernelActive | kernel `register_class` (attested artifact, `public_prosecution_complete_v1`, court budget, carrier fit) |
| `KernelBound` | V2Registered, KernelClassStands | `apply_kernel_bound_v1` |
| `CompleteQualified` | KernelBound | `palw_complete_check_domain_v1` |
| `CommittedSampled` | KernelBound | `apply_conformance_committed_v1` |
| `CommittedComplete` | KernelBound, CompleteQualified | `apply_conformance_committed_v1` |
| `EligibleSourcesNonEmpty` | OpvFinal (of another class eligible at the commitment) | `opv_eligible_set_v1` frozen at 107 |
| `BeaconLocked` | CommittedSampled, EligibleSourcesNonEmpty | `collect_work_beacon_v1` |
| `SampledEvidence` | BeaconLocked | `judge_posted_evidence_v1` |
| `CompleteEvidence` | CommittedComplete | `judge_complete_check_v1` |
| `ConformancePassed` | SampledEvidence ∨ CompleteEvidence | `tick_conformance_v1` / `apply_conformance_evidence_v1` |
| `G14Eligible` | ConformancePassed, KernelClassStands | `OnboardingStepV1::PublicProsecutionGate` |
| `V2Active` | G14Eligible, ArtifactFinal | `onboarding_gate_v1` |
| `OpvEligible` | KernelActive, G14Eligible, ArtifactMatured, BoundsFit, PolicyVerified, NotDenied | `opv_eligibility_v1` |
| `OpvClassRegistered` | OpvEligible | fold admission at tag 13 + kernel `register_class` |
| `OpvClaim` | OpvClassRegistered, OpvEligible | fold gate before `CommitClaim` |
| `OpvFinal` | OpvClaim | kernel `tick` (window end, no dispute) |
| `PanelAssignmentBeacon` | OpvFinal, ApprovedPanelScheme | `verify_panel_beacon_v1` (scheme list empty) |
| `WorkSliceBeacon` | OpvFinal | RFC-0008 §6 (not wired) |

## 4. The bootstrap: a complete-check source class

**Idea.** Conformance samples because a model's behaviour is too large to check whole, and sampling needs unpredictable randomness. A
class whose **whole behaviour is a small finite function** can be checked completely and deterministically — every input, every weight —
so its conformance needs no seed and no beacon. Such a class reaches Final under OPV's own rule (fixed window, no Panel, no beacon), and
its Finals feed the beacon of every other class.

**Which classes qualify** (`palw_complete_check_domain_v1`, a pure function of the class's on-chain program and its kernel class's plan —
no list, no registrant flag):

1. a single TIR program (no pipeline);
2. **stateless**: no declared state, no `Ref::State` — so position `p`'s output depends only on `(token_p, p)`, never on the prefix;
3. an enumerable input domain `N = token_bound × (plan.max_positions if any node reads Input(POS), else 1) ≤ 1,024`;
4. the artifact is small enough to carry whole: `≤ 4,096` inventory leaves and `≤ 512 KiB`;
5. the complete check fits the fold: `N × forward work (§8 node costs at h = 1) ≤ 2^26`, charged to the block's adjudication budget.

Under (2) a greedy job's whole output is determined by the per-input map, so a check of every input is a check of **every job the class
can ever serve** (with or without position); a class with history has `Σ T^l` prefixes and never qualifies.

**The complete check** (`ConformanceEvidenceActionV1::PostComplete`, tag 109, judged in the fold by `judge_complete_check_v1`):

* the post carries **the whole inventory** (every leaf, in inventory order) and, per input, the reference / independent / backend
  results (logits digest, greedy next token) and, per leaf, each implementation's decoded-values digest;
* the fold re-roots every leaf to the V2 class's registered `artifact_root` (public DA of the whole artifact, on chain), rebuilds the
  tensors, recomputes `ParamCommitmentsV1` and requires its root to be the kernel binding's `kernel_param_root` — **binding equality
  proven, not bonded** (closes onboarding GAP 1 for this class);
* it runs the chain's reference TIR interpreter on every input and requires every posted implementation result, and every leaf's
  decoded digest, to equal the chain's own; anything else is `CONFORMANCE_FAILED` (counted);
* a pass is `CONFORMANCE_PASSED` at once (nothing is left to refute: the chain computed every check) and then the public-prosecution
  step; no window, no seed, no beacon. A commitment with no evidence by commit + 60 DAA is a default (`Withheld`, counted).

The network's complete-check policy (`palw_onboarding_complete_check_policy_v1`) is a `PostCommitChallengePolicyV1` whose randomness
source is `randomness/none-complete-check/v1` and sampler `sampler/complete-enumeration/v1` (k = delay = window = D = 0, one
repetition); `misaka-palw-challenge` validates it as such and `collect_work_beacon_v1` refuses it (a complete check has no beacon by
construction). Tag 106 accepts it only for a qualifying class (§4.1–5): **a "bootstrap" class whose activation would need the beacon
is refused at binding**, and must take the sampled policy.

**Recognition without a manual list.** A bootstrap class is any class whose conformance attempt was decided under the complete-check
policy; that policy is accepted only where the predicate above holds, and the predicate reads only chain state.

**From "bootstrap sources only" to "all eligible sources": no switch.** The source set of a commitment is the derived eligible set at
that commitment. From genesis it can only contain complete-check classes (the least fixed point); every class that passes a sampled
conformance with a beacon drawn from them joins the set for every later commitment. There is no flag, height or count at which the
rule changes, so there is no switch timing to grind. Bootstrap classes stay sources: their Finals cost the same reservation, default
penalty and live-claim slot as any other OPV work (collateral, not model size, is what a source costs — §6).

## 5. Derived OPV eligibility

### 5.1 The predicate (`opv_eligibility_v1`)

`X` (an OPV, mode-bound class id; registered or about to register) is eligible at DAA `t` iff some V2 class `V` is kernel-bound (106)
to `X` or to `X`'s legacy (Panel-licensed) sibling `L` (the same descriptor, program, plan and commitments), and:

| Code | Condition | Refusal |
| --- | --- | --- |
| E1 | the kernel descriptor of `X` is Active at `t` | `KernelNotActive` |
| E2 | `V`'s conformance record is `G14_ELIGIBLE` or `ACTIVE_REWARDABLE` and its commitment names `V`'s artifact root, `X`'s program root and `X`'s plan root | `ConformanceNotPassed` / `ConformanceOfAnotherStatement` |
| E3 | the bound kernel class stands in the route (registered only after `PUBLIC_PROSECUTION_COMPLETE`) | `NotG14Complete` |
| E4 | `V`'s artifact binding to `X`'s commitments root is Matured or Final and not refuted at `t` | `DaLapsed` |
| E5 | the class's prosecution bounds fit the OPV carriers, the block court budget, and saturating the court for the claim's exposure costs more than its maximum gain | `ResourceUnbounded` |
| E6 | the attempt's challenge policy is one of the network's two, structurally valid, and its effective bits (§7) — `Complete` for a complete check — reach the fence's `min_effective_bits` | `PolicyNotVerified` |
| E7 | `X` is not on the fence's `denied_classes` | `Denied` |

### 5.2 The fence list: a deny-list, never a source

`PalwPanelFreeFenceV1.admitted_classes` becomes **`denied_classes`** (strictly ascending, hashed Some-only as before) plus
**`min_effective_bits`** (the interim value is **128**). The list can only take eligibility away (an incident response the network
coordinates), never grant it, so a release cannot "admit" a class by editing params. Keeping a deny-list rather than nothing gives the
network a deterministic, auditable brake that needs no code change; a cap on how many classes may be eligible was rejected (it would make
eligibility depend on registration order — a race, and a grinding surface). Initial value: empty.

### 5.3 Where it bites, and loss

* **Registration** of an OPV class (tags 13/14): the fold admits the registering id into the kernel ledger's `opv.admitted` only if
  `X` is eligible at the block (a registrant still never chooses the lighter mode for an unvetted program). Pipelines have no onboarding
  path, so no OPV pipeline class is eligible (GAP-B4).
* **Every OPV claim commit** (`CommitClaim` / `CommitPipelineClaim` of an OPV class): dropped unless the class is eligible at the block.
  A claim keeps the facts it was admitted with (its window, reservation and Final are never reinterpreted).
* **Beacon sources**: 107 freezes `EP = eligible set at the commitment` minus the candidate under every mode.
* **Loss**: the predicate is re-evaluated at each of those points, so eligibility is lost — new claims refused, no new attempt takes the
  class as a source — when the artifact binding is refuted (DA lapse, E4), the kernel stops being Active (E1), the deny-list names it (E7),
  or the statement no longer matches (E2: another plan or artifact is another class with no passed conformance).

### 5.4 The test seam

`extras.opv.test_eligible` (filled only by the processor under `cfg(test)`, like the artifact-attestation hook; empty in any build that
can run a network) lets the pre-existing OPV mechanics tests (`g14_opv_*`, `g14_conformance_*`, written against the manual list) keep
their worlds. The bootstrap E2E (§8) uses **no** hook.

## 6. Grinding analysis

A beacon of a subject committed at `c` mixes the first `k` eligible Finals (canonical order: settlement position, occurrence, work id)
accepted at or after `S = c + anchor_delay_slots` and settled in `[S, S + beacon_window_slots)`; it locks `settlement_depth_d` past the
`k`-th settlement. Every source is an OPV claim: it costs a reservation (1,000 BILI interim) from Final until its liability horizon,
occupies one of `max_live_claims_total` (32) ledger-wide live slots (3 per producer), and is withheld only by a default (penalty 100
BILI, 10 % burned) or a conviction (the reservation). Because `beacon_window_slots ≤ OPV window + liability` (120 ≤ 50 + 200), every
work that can settle in one window is live at once, so **at most `C = max_live_claims_total` works can ever compete for one beacon**.

| Lever | Attacker's choices per beacon | Bits lost (interim k = 2, C = 32, R = 2) | Bounded by |
| --- | --- | --- | --- |
| Output selection (which of its works settle first) | ≤ `P(C, k) = C!/(C−k)!` ordered source lists | `⌈log2 992⌉ = 10` | `work_count_k` (exponent); `anchor_delay_slots` (no work committed before the subject is fixed can count — no precomputation); `beacon_window_slots` (with the OPV window and liability, the live cap applies) |
| Withholding / withdrawing a Final | before the lock: a different list, inside `P(C, k)`; after the lock: a veto that ends the attempt `BEACON_CHANGED`, counted | inside the retry term | `settlement_depth_d` (when the lock is final on the branch), `retry_limit`, `abort_policy_id` (counted retry); each veto costs a default penalty or a reservation |
| Choosing Final timing | reorders works: inside `P(C, k)` (order is by settlement position) | 0 extra | `beacon_window_slots`, canonical order |
| Fork choice (mine the branch whose beacon suits) | `F` branches that can win before `lock` | `⌈log2 F⌉`; `F = 1` under the settlement assumption (no reorg past `k`-th settlement + D) | `settlement_depth_d`, `reorg_policy_id` (branch-relative recompute, dependents roll back) |
| Work concentration (one party owns most sources) | the full-control worst case is the `P(C, k)` row; a party with share `ρ` of qualifying work controls ~`ρk` positions | ≤ `⌈log2 P(C, k)⌉` | `work_count_k`, live-claim caps, collateral |
| Retries (abandon and re-commit) | `R + 1` attempts per class | `⌈log2 3⌉ = 2` | `retry_limit` |
| Many subjects (adaptive: Sybil classes of one model) | `Q` registrations over the policy's life | `⌈log2 Q⌉` | registration price and binding reservation; stated in the approved tuple |

`grinding_budget_policy_id` names the stated `(C, F)` behind `G = P(C, k)·F` that the effective accounting uses
(`beacon_grinding_choices_bound_v1`). **Interim conformance: 2 scope bits − 1 (two relation families) − 2 (retries) − 10 (grinding) =
0 effective bits**: the interim policy is a drill. A production sampled policy needs, per relation family and with these `k` and `C`,
`r·s ≥ 128 + 1 + 2 + 10 + ⌈log2 Q⌉`. A complete check has nothing to grind (`ε = 0`).

Residual (stated): a party that controls every block of a window can also censor honest source claims on its branch — that is the
`F`/settlement assumption, not something the beacon removes; and the live cap bounds choices only while the OPV caps stand (F-C4R3-05).

## 7. Effective false-accept accounting

`misaka-palw-challenge::soundness::effective_false_accept_bits_v1`, pure, integer:

```text
eff = min_i (r · s_i)  −  ⌈log2 m⌉  −  ⌈log2 (R+1)⌉  −  β · ⌈log2 G⌉  −  ⌈log2 Q⌉        (s_i in millibits; losses in whole bits)
```

`s_i` the per-repetition soundness of relation family `i` (or `Complete`), `m` the sampled families (union bound), `r` =
`repetition_count`, `R` = `retry_limit`, `G` the grinding choices per beacon, `β` beacons per attempt (1 non-interactive; rounds for a
staged beacon), `Q` adaptive statements. All relations complete ⇒ `Complete`. `approved_v1` now requires the tuple's effective bits ≥
the policy's `security_bits` **and** `security_bits ≥ 128`; the shipped registry (`shipped_registry_v1`) stays empty. Golden vectors pin
the function (§8).

## 8. Proof (tests)

_Filled in after implementation._

## 9. GAPs

_Filled in after implementation._
