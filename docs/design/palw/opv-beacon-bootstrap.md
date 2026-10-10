# OPV ↔ PALW Work Beacon: the startup cycle, a non-circular bootstrap, derived eligibility, grinding

Agents OPV-BOOT, then OPVB (§6.3, §6.4, §12), branch `opv/bootstrap-beacon` (from the Lead's `9f777c3ca`). Everything here is **dormant** behind the existing fences
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
| RFC-0010 V3 Panel assignment (`PanelAssignment`) | `palw_panel_beacon_v1::verify_panel_beacon_v1`, `palw_panel_v3_fold_v1::ChainPanelBeaconHistoryV1` | `PanelIndependent` Finals only (contract rule); approved scheme required | dormant: `approved_panel_beacon_policies_v1() = []`, the chain history exports only V2-lattice (Panel-licensed) Finals and `eligible_profiles = ∅` → always unavailable. Feeding it the route's OPV Finals of derived-eligible classes is a later wiring (§9 GAP-B3). Its source set is the derived-eligible set FROZEN once per epoch at the commitment (`release_daa`, from the first reaching block's parent; R10F, delta 174), like 107's row, never re-derived at the lock |
| K2 per-claim check (`ClaimVerification`) | `misaka-palw-kernel::claim_subject` | any qualifying Final | **not wired**: the route's interim seats are seeded by the claim id (`palw_kernel_interim_seed_v1`, grindable, stated); an OPV claim has no Panel check and draws nothing |
| RFC-0008 work slices (`WorkSlice`) | spec §6; branch `rfc8/x8-exec-v2` held out | `RealUsefulWork` only — an EXEC work slice is refused **by kind** as a source, so slices can never seed slices | not on the integration tree |
| Public prosecution (`PublicProsecution`) | contract only | — | supplemental checks only; never a prerequisite of an exact proof |

Never a source anywhere: heartbeat, BASE-0, EXEC tx/slices, receipt-only material, provisional attempts, Panel receipts, block hashes,
signatures, DNS/BFT/validator randomness (`misaka-palw-challenge` `eligibility_v1`, by kind).

**What OPV eligibility and rewardability need** (§5): Active kernel; conformance passed (G14_ELIGIBLE or later) for the V2 class bound to
the class under either mode; the kernel class stands (it registered only after `PUBLIC_PROSECUTION_COMPLETE`); a standing (Matured/Final,
unrefuted) artifact binding — an identity fact, never an availability one (§14.1); the class's prosecution bounds fit the OPV carriers, court budget and censorship-cost rule; the
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
4. the artifact is small enough to carry whole in ONE carrier: `≤ 1,024` inventory leaves and `≤ 64 KiB`;
5. the complete check fits the fold: `N × forward work (§8 node costs at h = 1) ≤ 2^26`, charged to the block's adjudication budget.

Under (2) a greedy job's whole output is determined by the per-input map, so a check of every input is a check of **every job the class
can ever serve** (with or without position); a class with history has `Σ T^l` prefixes and never qualifies.

**The complete check** (`ConformanceEvidenceActionV1::PostComplete`, tag 109, judged in the fold by `judge_complete_check_v1`):

* the post carries **the whole inventory** (every leaf, in inventory order) and six result roots: the reference / independent / backend
  implementations' roots over every input (`(token, position, logits digest, greedy next)`) and over every leaf (its decoded-values
  digest);
* the fold re-roots every leaf to the V2 class's registered `artifact_root` (the whole artifact is carried on chain in the post, so this class's material is in the block itself — §14.2), rebuilds the
  tensors, recomputes `ParamCommitmentsV1` and requires its root to be the kernel binding's `kernel_param_root` — **binding equality
  proven, not bonded** (closes onboarding GAP 1 for this class);
* it runs the chain's reference TIR interpreter on every input and requires every posted implementation result, and every leaf's
  decoded digest, to equal the chain's own; anything else is `CONFORMANCE_FAILED` (counted);
* a pass is `CONFORMANCE_PASSED` at once (nothing is left to refute: the chain computed every check) and then the public-prosecution
  step; no window, no seed, no beacon. A commitment with no evidence by commit + 60 DAA is a default (`Withheld`, counted). Its
  in-fold cost and carriage: §8.

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
| E4 | `V`'s artifact binding to `X`'s commitments root stands: Matured or Final and not refuted at `t`. An identity fact (ADR-0175/0177): maturity means the refutation window passed, NOT that anyone could fetch the bytes (§14.1) | `BindingNotStanding` |
| E5 | the class's prosecution bounds fit the OPV carriers, the block court budget, and saturating the court for the claim's exposure costs more than its maximum gain | `ResourceUnbounded` |
| E6 | the attempt's challenge policy is one of the network's, structurally valid, **the complete check** (GAP-70: in the release a sampled conformance, v2 or v3, gates no reward until a digest court exists), and its effective bits (§7) — `Complete` for a complete check — reach the fence's `min_effective_bits` | `PolicyNotVerified` |
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
  class as a source — when the artifact binding is refuted (a proof of inequality, E4; never a failure to serve the bytes), the kernel stops being Active (E1), the deny-list names it (E7),
  or the statement no longer matches (E2: another plan or artifact is another class with no passed conformance).

### 5.4 The test seam

`extras.opv.test_eligible` (filled only by the processor under `cfg(test)`, like the artifact-attestation hook; empty in any build that
can run a network) lets the pre-existing OPV mechanics tests (`g14_opv_*`, `g14_conformance_*`, written against the manual list) keep
their worlds. The bootstrap E2E (§8) uses **no** hook.

## 6. Grinding analysis

A beacon of a subject committed at `c` mixes the first `k` eligible Finals (canonical order: settlement position, occurrence, work id)
accepted at or after `S = c + anchor_delay_slots` and settled in `[S, S + beacon_window_slots)`, chosen under the policy's **source
rule**, and locks `settlement_depth_d` past the `k`-th settlement. Every source is an OPV claim: it costs a reservation (1,000 BILI
interim) from commitment until its liability horizon, occupies one of `max_live_claims_total` (32) ledger-wide live slots (3 per
producer), and is withheld only by a default (penalty 100 BILI, 10 % burned) or a conviction (the reservation). Because
`beacon_window_slots ≤ OPV window + liability` (120 ≤ 50 + 200), every work that can settle in one window is live at once.

### 6.1 Source rules (implemented)

| Rule (`source_eligibility_policy_id`) | What a source must satisfy beside eligibility | Used by |
| --- | --- | --- |
| `…/v1` (plain) | nothing | the contract's reference policy, RFC-0010's (unapproved) panel scheme |
| `…/distinct-producer-consumer/v2` | its producer bond differs from every source already taken; its consumer (job payer) too, where the route records one | **the interim onboarding policy** |
| `…/distinct-producer-consumer-class-capped/v2` | the distinct rule, and at most `⌈k/2⌉` sources from one class | available; not interim (it needs two bootstrap classes) |

Attribution (`AttributedWorkV1 { event, attribution: { producer_id, consumer_id } }`) is built by the consumer from authenticated state
(`finals_read_v1`: the Final claim's producer bond; the consumer is `Absent` until G14-R4's user-pays escrow records a job's payer, so
the consumer clause is a no-op until then — GAP-B2). The plain collector refuses a policy whose rule needs attribution.

### 6.2 The levers, in bits

Interim terms: `k = 2`, `R = 2`, `F = 1`, and `C = 96` competing works per window. The live cap is 32, but a slot refills at Final
(G14-R4's admission fix), so a 120-DAA window against the 50-DAA OPV window carries `32 × 3 = 96` works (`competing_works_bound_v1`,
C4 F-C4R4-08). "Bits" is what the lever subtracts from a sampled conformance's effective bound.

| Lever | Plain rule | Distinct rule (interim) | Sealed-source beacon v3 (§6.3; needs salted seals) | Bounded by |
| --- | --- | --- | --- | --- |
| **Last contributor** — after `k − 1` sources are public (an OPV claim's execution commitment is public at commitment, 50 DAA before it settles), post ONE job and grind its nonce offline (`canonical_work_id = H(class, job)`; a few hashes per try), then let it settle `k`-th | `h` bits (all offline work; unbounded by any on-chain count) | `h` bits — one work needs one bond and one payer | 0: every mixed contribution is sealed, with a secret salt, before any mixed salt is public | v3: the seal window; today nothing |
| Output selection among works whose inputs are fixed | `⌈log2 P(C, k)⌉ = 14` (`P(96, 2) = 9,120`) | 14 (and `k` distinct bonds; bonds are not parties, F-C4R4-01) | 0: every qualifying seal of the window is mixed (no selection exists) | `work_count_k`, `anchor_delay_slots` (nothing committed before the subject counts), `beacon_window_slots` with the live cap |
| Withholding / withdrawing a Final | before the lock: inside the selection count; after: a veto ending the attempt `BEACON_CHANGED` | same | a veto (`BEACON_VETOED`, a counted retry): a withheld seal or an abandoned source is never dropped | `retry_limit` (counted), `settlement_depth_d`, `abort_policy_id`; a default penalty or a reservation per veto |
| Final timing | ordering, inside the selection count | same | 0: the mix is in seal order, fixed before any reveal | canonical order, `beacon_window_slots` |
| Fork choice | `⌈log2 F⌉`; `F = 1` under the settlement assumption | same | same | `settlement_depth_d`, `reorg_policy_id` |
| Work concentration — one party owns `j` of `k` sources | one bond can own every position (3 live claims per producer ≥ k) | `j` bonds and `j` payers | irrelevant to bias (one honest salt suffices); `k` is a quorum of distinct producers | `work_count_k`, the live caps, collateral, the distinct rule |
| Retries | `⌈log2 (R+1)⌉ = 2` | 2 | 2 | `retry_limit` |
| Adaptive statements (Sybil classes of one model) | `⌈log2 Q⌉` | same | same | registration price, binding reservation |

**Effective bits of the interim conformance** (two families of 2.885 bits, one repetition): `2 − 1 − 2 − h → 0` under every rule — and
`0` even with `G = 9,120` (no last contributor). The interim policy is a drill. `misaka-palw-challenge/tests/grinding.rs` demonstrates the
last contributor: with one honest source settled, 4,096 offline nonces of ONE attacker job give 4,096 distinct beacons (an 8-bit target
is hit), under the distinct rule — far past the live-cap bound of 32.

**What reaches 128 effective bits.** (a) Against the last contributor as it stands, a sampled scope must out-bit the adversary's
offline work: `−log2 ε ≥ 128 + h + ⌈log2 m⌉ + ⌈log2 (R+1)⌉`; with `h = 128` and the chain's whole check bound (4,096 leaves at scope
v1's 62,500 ppm) a single family reaches `369 − 2 − 128 = 239` bits (golden vector), but any family with fewer bits (vectors at 50 %)
caps the bound. (b) With the sealed-source beacon v3 (§6.3), `G = F` and the same target needs `128 + ⌈log2 m⌉ + ⌈log2 (R+1)⌉ +
⌈log2 Q⌉` algorithmic bits, AND `ε_src` must reach 128 bits on its own (the union of the two costs one bit). v3 is written as a pure
collector; it is a **DESIGN blocker for any sampled approval and for Panel=0 activation** until the ledger seals a secret salt
(GAP-B1a). A complete check has nothing to grind (`ε = 0`).

**Cost to own `j` of `k` sources** (interim numbers; `f_job` is G14-R4's non-refundable fee of a self-posted job, not yet fixed):
`j · (f_job + 3 carrier fees ≈ 0.06 BILI)` burned, `j · 1,000 BILI` reserved for ~250 DAA (refunded if honest), `j` bonds (distinct rule),
`j` payers once recorded, and with the class cap `⌈2j/k⌉` classes (each a registration burn, a 100 BILI binding reservation for 200
DAA, a 1 BILI complete-check fee). A post-lock veto costs a default penalty (100 BILI) or a reservation. The kernel splits a default
penalty like a slash (C4 F-C4R3-02): the demanders take the accuser's share (`accuser_reward_permille`, 500 ‰ today) and the rest is
burned, at least `default_burn_permille`. So an attacker who demands its own source recovers 50 BILI and loses 50 today. Under the
future ruleset's 49 % reporter share (ADR-0032, 2026-10-10), it recovers at most 49 BILI and loses at least 51 BILI per veto. Each source is also a claim
of its producer bond and draws that bond's ADR-0176 claim ceiling `Q` over the window `W` (§14.3). None of this prices the last
contributor, which needs `j = 1`.

### 6.3 The sealed-source beacon v3 (SOUND SG-01, SG-01a)

`misaka-palw-challenge::sealed` (`collect_sealed_work_beacon_v3`, randomness source `palw-work-beacon/sealed-source/v3`, the
distinct source rule only) reads seal facts that a consumer derives from authenticated state.

**Wired on the node, dormant.**
- The network's third onboarding policy is `palw_onboarding_sealed_policy_v1` (W = 40). Tag 106 accepts it only if
  `2W ≤ seal_ttl_daa`.
- The route serves the seal facts as `beacon_sealed_sources_v1`, built from G14-R4's tables 25–26 and `claim_beacon_seals_v1`. The
  profile comes from the sealed job's class, the fate from the claim row and op 212.
- One read, `attempt_beacon_v1`, serves the fold, the tick, the chunk lane and op 231. A veto ends the attempt `BEACON_VETOED`,
  counted.
- E6 accounts a v3 attempt with `G = F = 1` and combines it with `ε_src` from the interim `δ = 10`, `ρ = ½`.
- The SDK's fresh verifier refuses a v3 attempt (`SEALED_SOURCE`) until a public read serves the seals.
- The node tests are `g14_opv_bootstrap_a_sealed_source_v3_beacon_locks_on_salted_seals_and_the_class_passes` and
  `g14_opv_bootstrap_a_withheld_v3_seal_vetoes_the_attempt_and_is_counted`.

```text
S = commitment + anchor delay;  W = beacon_window_slots
seal window   [S, S + W)      a claim seal binds a SECRET salt (claim seal = H(claim id ‖ salt)), bonded by seal_deposit
reveal window [S + W, S + 2W) the claim and its salt
mixed         EVERY seal in the seal window on a profile frozen eligible (never the candidate or an excluded profile), except one
              revealed before S + W; then, in seal order, the first seal of each producer and of each known consumer
quorum        fewer than k mixed at S + W  → BEACON_UNAVAILABLE (known when the seal window closes)
veto          a mixed seal not revealed in [S + W, S + 2W), or a mixed source that ends without a standing Final
              → BEACON_VETOED: the beacon never locks without it; the attempt ends as a counted retry
lock          every mixed source Final (real useful work of the sealed profile, revealed after S + W, standing, DA-satisfied,
              independent, Panel-independent for a Panel draw) and the last settlement D deep; output = the accumulator over
              (seal position, seal, producer, profile, work id, execution commitment, salt) in seal order
```

**Why each SG-01a lever is closed or bounded.**

* **Last contributor (SG-01).** Every mixed contribution is fixed when it is sealed, and no mixed salt is public before `S + W`: a
  reveal earlier than that is not a source. So no contribution is chosen after another mixed one is seen.
* **(i) Withholding.** After the reveals, the adversary's only move is to withhold or abandon, and either one vetoes. Its choice per
  attempt is {lock, veto}. That is the retry term `⌈log2 (R + 1)⌉`, with vetoes counted, and not `2^a`. So `G = F`
  (`sealed_beacon_grinding_choices_v3`). The test `after_the_reveals_the_adversary_can_only_veto_never_choose` runs all 2^3 subsets
  of three attacker seals: one lockable output, seven vetoes.
* **(ii) First-`k` capture.** Nothing is "the first `k`": every qualifying seal is mixed, and `k` is only a quorum. Sealing early or
  sealing many cannot push an honest seal out. The test is `sealing_first_or_sealing_many_never_pushes_an_honest_seal_out`.
* **(ii) Censorship.** A censored honest **reveal** is a veto, never an exclusion. The one path to a LOCKED beacon without an honest
  salt is to keep every honest **seal** out of the seal window.
* **(iii) Fork choice.** Unchanged: `F`, bounded by `D` under the settlement assumption.

**`ε_src`, quantified** (`sealed_source_censorship_bits_v3`). An honest producer broadcasts its seal at the window's start. The seal
is accepted in the window unless every block that could carry it before `S + W − δ` is the adversary's. Here `δ` bounds how late a
carrying block is merged. With `b` blocks per DAA and an adversary block share `ρ`:

```text
ε_src ≤ ρ^((W − δ)·b)        bits_src = ⌊(W − δ)·b·(−log2 ρ)⌋
```

The bound holds under three assumptions (A-B2): an honest producer seals an eligible source in the window (participation), honest
blocks are not filled by fee-paying spam, and `2W ≤` the ledger's seal TTL, so every in-window reveal is legal. An honest producer
reveals exactly `W` after sealing, which makes its seal a source of every subject whose seal window contains it.

**Participation needs the source class's model (ADR-0177).** Only a party that holds a source class's registered model can produce a
claim of it, and the chain guarantees no one can acquire a model. So "an honest producer seals" is conditional on an honest party
having acquired an eligible source class's model (A-ACQ, §14.2). A complete-check source carried its whole artifact in its `PostComplete`
(≤ 64 KiB, §8), so any node that keeps that block holds the model; that is a property of the bootstrap, not a requirement on anyone.
A sampled-eligible source of a closed model may have no honest producer at all. This is why `PALW_ONBOARDING_SEALED_PARTICIPATION_BITS_V1`
is 0 and why no count of bonds can raise it. A review must supply it as a statement about honest parties that hold some eligible
source's model.

The union with the algorithmic bound costs one bit (`combine_failure_bits_v1`). Golden vectors:

* the interim window `W = 40`, `δ = 10`, `b = 1`, `ρ = ½` gives **30 bits**;
* 128 bits needs `W − δ ≥ 128` blocks at `ρ = ½` (81 at `ρ = ⅓`), and so a seal TTL of at least `2W`. G14-R4's interim TTL of 100
  allows `W ≤ 50`;
* a production-shaped tuple (4 × 64 bits × 3 reps, `R = 2`, `Q = 2^10`) gives 178 algorithmic bits, 29 combined at the interim
  window, and 129 at `W − δ = 130`.

**Liveness, the price of the veto** (GAP-B12; the user's ruling of 2026-10-09: the deposit is not chosen here — lane ECON derives it
from the maximum stall `R + 1` abandonments buy, the attack cost and the damage value, and demonstrates the attack cost against this
collector; the deposit stays a named parameter, G14-R4's `LedgerPolicyV1::seal_deposit`). Anyone who seals in the window and withholds vetoes the attempt. The cost is one
forfeited `seal_deposit` (1 BILI interim). Vetoes are counted, so `R + 1` vetoes exhaust a class's attempts. That is a cheap DoS on
onboarding, and it is traded for soundness: if vetoes were uncounted, a registrant colluding with a sealer would re-roll at the price
of a deposit each.

Options, a policy decision:
* a deposit for beacon-mixed seals priced for the DoS;
* uncounted vetoes bounded by time, each attempt taking at least `2W` DAA, stated as `R_eff = ⌈T_life / 2W⌉` in the retry term.

**GAP-B1a — the salt (DESIGN, blocks v3 on chain).** G14-R4's claim seal is `claim_seal_v1(claim id)`. A claim of a deterministic
class is a function of its public job and its producer: `KernelClaimV1 { job_id, producer_bond, generated, evidence_root }`, greedy
decode, no producer randomness. Anyone who can run the model can therefore compute the seal's preimage when it is posted, so the seal
hides nothing. A v3 beacon over those seals is the v2 last contributor again.

v3 on chain needs two changes. Both are in the kernel ledger and route (G14-R4's) and need a Lead allocation:
* `claim_seal_v2(claim id, salt) = H("misaka-palw/kernel/claim-seal/v2"; claim id ‖ salt)` with a 64-byte salt from the producer's
  CSPRNG;
* a reveal that carries the salt: a salted `CommitClaim` / `CommitPipelineClaim`, or one new inner kind. The ledger checks the salted
  seal and keeps `beacon_salt` and `sealed_daa` on `ClaimRowV1`.

The route then serves each seal fact for op 212: `SealedSourceV3` from the seal rows (live and forfeited) and the claim rows.

### 6.4 SG-05: the per-claim check's coins are private, stated

**What 128 bits means** (the user's ruling of 2026-10-09):

* An effective 128 bits is the DESIGN TARGET. A scheme is approved only after external review, and nothing is approved until its
  effective soundness is audited.
* The 128 bits bound the mathematical check and the manipulation of its challenges: the false-accept probability given that a check
  runs on unpredictable coins. They are NOT a 128-bit guarantee that an honest watcher exists, holds the model, or checks in time.
  Those are the environment assumptions A-ACQ and A-WATCH below, with their own probability (0 for a closed model), and deterrence
  prices them (SG-06). The 128 bits also cover a conformance check's math and the manipulation of its challenge (grinding, retries,
  adaptive statements, `ε_src`) — never the existence of a refuter that holds the model (§14.2).

No public-coin per-claim check is wired, and this branch does not wire one. **SG-05 is resolved by the formal statement below: OPV
per-claim detection rests on an honest outsider's PRIVATE salt, and that is sufficient for G14** (producer and every Panel seat colluding,
one non-Panel bonded verifier convicting from public authenticated material). A public seed would add nothing to that statement.

**Proposition SG-05 (private-coin per-claim detection).** Fix a claim `c` of an OPV class, committed at DAA `t_c`; its execution
commitment `E_c` (trace roots, generated tokens, evidence root) is fixed in the block that admits it. Let `O` be an outsider that is not
`c`'s producer and holds no Panel seat. Assume:

* **A-ACQ** — `O` holds the class's registered model, authenticated against the registered root (ADR-0177: the chain does not
  guarantee this; for a closed model it may never hold);
* **A-SALT** — `O` draws a salt `s` from its own CSPRNG after `t_c`, and no other party learns `s` before `O`'s filing is included;
* **A-WATCH** — `O` runs the salted check (`FreshVerifierV1::check_salted`) inside the claim's window, and its filing is included before
  the window closes (bounded by E5's censorship cost and the OPV budgets);
* **A-H** — the kernel's draw stream from `s` is a random oracle (SOUND SG-07).

Suppose a fraction `f_i` of relation family `i`'s draw space exposes a fault of `c`, and `O` makes `n_i` draws of family `i`. Then:

```text
Pr[ O misses every fault of c ]  ≤  Π_i (1 − f_i)^(n_i)        (over s alone)
```

This holds whatever the producer and every Panel seat do. There are three reasons:

1. `E_c` is fixed before `s` exists, so no colluding party can adapt the commitment to the draws.
2. `s` is never input to anything the colluders control. No beacon, seat, Panel receipt or block hash enters `O`'s draws.
3. A filed fault is adjudicated deterministically from public authenticated material (the claim's committed values and the court's
   exact relation), so the court never needs to know how `O` chose where to look. A localized fault is an objective statement, and
   the selection's coins need not be public for the conviction to stand.

The per-claim detection probability of the network is therefore

```text
p  =  Pr[some honest outsider satisfies A-ACQ, A-SALT and A-WATCH for c]  ·  (1 − Π_i (1 − f_i)^(n_i))
```

**`p` can be 0 for a closed model** (no honest outsider satisfies A-ACQ). No count of claims, bonds or Panel seats raises it.

**What a public per-claim seed would and would not add.** It would make a check reproducible by third parties. That is needed only to
pay or credit a check as such (watcher rewards per check performed), or to rely on a seat's check under Panel > 0, where a seat could
claim a check it never ran. G14 relies on neither. It would also cost the claim `2W + D` DAA of latency under v3, as a
`ClaimVerification` subject whose seal window opens at the claim's commitment. **Decision recorded:** no public per-claim seed is
wired, and SG-05 is closed by this statement.

**What would break it.**

* **Predictable coins.** The unsalted entry points `FreshVerifierV1::check` and `PipelineFreshVerifierV1::check` draw from the
  record's public `beacon`, which is `[0; 64]`. A producer can place a fault outside every predictable draw, so those entry points
  have **no soundness**. Only kernel tests call them (`k2_public.rs`, `k2_adversarial.rs`); no outsider, seat, SDK or node path does.
  SOUND's Q-13 asks for them to be unreachable outside tests (CODE, the kernel owner's).
* **Grindable seeds.** The interim Panel route's seats are seeded by the claim id (`palw_kernel_interim_seed_v1`, grindable, stated).
  That is a Panel = 1 path. It gives no OPV soundness, and G14 does not count it.
* **No acquisition.** A-ACQ failing makes `p = 0`; §14.2.

## 7. Effective false-accept accounting

`misaka-palw-challenge::soundness::effective_false_accept_bits_v1`, pure, integer:

```text
eff = min_i (r · s_i)  −  ⌈log2 m⌉  −  ⌈log2 (R+1)⌉  −  β · ⌈log2 G⌉  −  ⌈log2 Q⌉        (s_i in millibits; losses in whole bits)
```

`s_i` the per-repetition soundness of relation family `i` (or `Complete`), `m` the sampled families (union bound), `r` =
`repetition_count`, `R` = `retry_limit`, `G` the grinding choices per beacon, `β` beacons per attempt (1 non-interactive; rounds for a
staged beacon), `Q` adaptive statements. All relations complete ⇒ `Complete`. Every logarithm is rounded up and charged separately,
so the result never overstates the bound.

* **Approval** (`approved_v1`): a matching tuple now also carries the reviewed statement (`relations`, `grinding_choices_per_beacon`,
  `beacons_per_attempt`, `adaptive_queries`); the policy's `security_bits` must be ≥ 128 (`TargetBelowFloor`) and the effective bits ≥
  `security_bits` (`BelowTarget`). The shipped registry (`shipped_registry_v1`) is empty; nothing is approved.
* **Eligibility** (E6): a passed attempt's effective bits (`attempt_effective_bits_v1`) — `Complete` for a complete check; for a sampled
  one the committed scope's families under the policy's repetitions and retries, `G = 2^128` (the last contributor,
  `palw_onboarding_grinding_choices_v1`), one beacon, one statement — must reach the fence's `min_effective_bits` (interim 128). The
  bits bound the check's math and the manipulation of its challenge. Whether a refuter that holds the model exists is A-ACQ, which
  they do not cover (§14.2).
* Golden vectors (`misaka-palw-challenge/tests/soundness.rs`): the interim drill = 0; a production-shaped tuple (4 × 64 bits × 3 reps,
  `G = 992`, `Q = 2^20`) = 158; exactly 128 at the target and 127 one statement later; complete = `Complete`; a staged beacon of three
  rounds at `G = 2^10` loses 30; the chain's whole check bound against `h = 128` = 239; saturation never wraps.

## 8. In-fold cost and carriage of the complete check

* **Bounds** (interim): ≤ 1,024 inputs, ≤ 1,024 leaves, ≤ 64 KiB of artifact, ≤ 2^26 work units (§8 node costs at `h = 1` × inputs +
  artifact bytes). The post carries the whole inventory and six result roots (reference / independent / backend over every input and
  over every leaf), never per-input lists, so it is ≤ 90,000 bytes: **one carrier, no chunk lane** — a junk flood of the legacy chunk
  lane (F-C4R3-03) cannot keep a bootstrap from completing.
* **Order in the fold** (`apply_complete_check_v1`): free structural checks (`Err`, dropped: the registrant, an open complete-check
  attempt with nothing judged, the post bound to THIS attempt's statement root, one carrier's size, the class still qualifies, the fee in
  free collateral) → the block's cap of **2 judged complete checks** (counted from the attempt rows; the rest wait uncharged) → the
  class's whole work charged to the block's adjudication budget **before the post is read** (a function of the program: junk costs what
  a real check costs) → a **1 BILI fee** burned from the registrant's bond → the judgement, cheap checks first, every path a `Result`. A
  judged check always closes its attempt (pass, or a counted failure), so an attempt is judged once and a class at most `R + 1 = 3`
  times; each attempt also sits behind a class registration and a 100 BILI binding reservation. `2 × 2^26 = 2^27` of the block's
  `2^30` court work is the most complete checks can take, so prosecutions always have room.
* **Sampled evidence** (tag 109 `Post`) still needs multi-part carriage for production-sized scopes; it rides the legacy chunk lane
  today. Requested from G14-R4 through the Lead: a target kind "conformance attempt of V2 class C" on the capture-proof tag-113 lane
  (opener = C's registrant, TTL ≤ the attempt's evidence deadline or refutation window, refused at the first chunk when the attempt is
  not open) — GAP-B5.

## 9. Proof (tests)

Real node (`consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/opv_bootstrap.rs`; fences test-armed through
`Config::new`, **no eligibility hook**, fixtures not attested by any hook):

| Case | Test |
| --- | --- |
| From zero Finals: nothing eligible, an early OPV registration refused; B's complete check passes in the fold (fee burned, work charged); B eligible by the derived rule (drill floor and 128); B registers, its claims reach OPV Final from distinct producers; C's sampled commitment freezes exactly `[B]` as sources (C under every mode excluded); the beacon locks on B's Finals; C passes its window; C eligible (drill floor) and NOT under 128 (`PolicyNotVerified`, 0 effective bits); C registers and its own claim reaches OPV Final, attributed to its producer; the eligible set is `{B, C}`; replay | `g14_opv_bootstrap_from_zero_finals_a_complete_check_seeds_the_beacon_and_a_sampled_class_becomes_eligible` |
| No bootstrap class: the sources are empty, BEACON_UNAVAILABLE (counted), a re-commitment again freezes nothing, C never eligible, its OPV registration refused; the chain keeps producing blocks and moving state; replay | `g14_opv_bootstrap_without_a_complete_check_class_the_beacon_never_comes_and_the_chain_lives` |
| A stateless class with 1,100 inputs: 106 under the complete-check policy refused (rows untouched), the sampled policy accepted; a PostComplete for a sampled attempt dropped | `g14_opv_bootstrap_a_class_that_cannot_be_checked_whole_is_refused_the_complete_check` |
| A block of hostile complete checks (late-failing result roots, a junk inventory, an honest one, an outsider's): the outsider's dropped free, exactly 2 judged, each charged its whole work and the fee, the third waits uncharged; failures counted; the waiting one judged next block; a failed class re-commits and passes; replay | `g14_opv_bootstrap_a_block_of_hostile_complete_checks_spends_budget_and_never_stops_the_chain` |
| Loss: C on a FALSE binding passes a sampled conformance, becomes eligible, registers; the binding is refuted (two disagreeing openings) → `BindingNotStanding`, C's next claim dropped at the door; B (binding proven by its complete check) stays eligible; another plan of the program is `NotOnboarded`; replay | `g14_opv_bootstrap_eligibility_is_lost_when_the_artifact_binding_is_refuted` |
| The deny-list takes eligibility away (a passed bootstrap is not eligible, its registration refused) | `g14_opv_bootstrap_the_deny_list_takes_eligibility_away_and_never_grants_it` |
| The test seam exists only under `cfg(test)` and is `Vec::new()` otherwise | `opv_test_eligibility_hook_is_test_only` |
| Qualification (stateless small class qualifies; history never) and the complete check's judgement on every lie | `opv_bootstrap_a_stateless_small_class_qualifies_…`, `opv_bootstrap_the_complete_check_passes_the_truth_…` |

Pure: the graph (`palw_opv_bootstrap_v1::tests`: reachable only through the complete check; a bootstrap that needs the beacon closes
the cycle; every eligibility reason is an edge and every edge a reason; the complete-check policy draws no beacon); the effective
bound, approval and the complete-check shape (`misaka-palw-challenge/tests/soundness.rs`); the grinding attacks
(`misaka-palw-challenge/tests/grinding.rs`); the sealed-source beacon v3 against SG-01/SG-01a and its accounting
(`misaka-palw-challenge/tests/sealed_beacon.rs`); the fence (`consensus/core/tests/rfc0015_panel_free.rs`). The pre-derivation OPV and
conformance worlds (`g14_opv_*`, `g14_conformance_*`, `g14_c4r3_opv_*`) run on the `cfg(test)` seam, and the conformance worlds on the
drill floor (§12).

## 10. GAPs

* **GAP-B1 (DESIGN, blocker for any sampled approval and Panel=0): the last contributor.** §6.2. Closed for attempts committed under
  the v3 policy (§6.3, wired). The v2 sampled policy stays grindable and accounted at `G = 2^128`.
* **GAP-B1a: the salt, closed by G14-R4.** It provides `claim_seal_v2`, inner kind 20 `CommitClaimSalted` (unsalted reveals refused
  past the fence), tables 25–26 and `claim_beacon_seals_v1`.
* **GAP-B17: v3 has no public seal read.** Op 212 serves Finals, not seals, so the SDK's fresh verifier refuses a v3 attempt. A
  seal-facts read (an RPC op) is an allocation.
* **GAP-B12 (POLICY): v3's liveness price.** A withheld seal vetoes, counted; `R + 1` seal deposits exhaust a class's attempts (§6.3).
* **GAP-B2: consumer distinctness is a no-op** until the route records a job's payer (G14-R4's user-pays escrow); then `finals_read_v1`
  fills `consumer_id`.
* **GAP-B3: RFC-0010 V3** does not read the route's OPV Finals or the derived eligible set (`ChainPanelBeaconHistoryV1` serves V2-lattice
  Finals and `eligible_profiles = ∅`), and no panel scheme is approved: V3 stays unavailable.
* **GAP-B4: pipelines** have no onboarding path, so no OPV pipeline class is ever derived-eligible (only the test seam names one).
* **GAP-B5: sampled evidence carriage** — the tag-113 target kind (§8).
* **GAP-B6: vector refutations of a new class.** An OPV class cannot have Finals before it is eligible, and a Panel-licensed sibling has
  no Panel under Panel=0, so a `VectorTokens` refutation of a NEW class's sampled evidence has no Final to cite; vectors rest on
  off-chain re-execution (OB-P0 GAPs 2 and 9, widened). The complete check has no such residual (the fold runs every input).
* **GAP-B7: implementation results are self-reported** in both paths (the registrant posts its independent and backend results); the
  complete check makes the chain's REFERENCE check complete and proves the binding, which the sampled path cannot.
* **GAP-B8: cost** — eligibility is evaluated by rebuilding the ledger from the rows (onboarding GAP 8) and scanning the kernel bindings
  at every OPV registration, claim and commitment.
* **GAP-B9: INTERIM numbers** — the complete check's bounds, fee, deadline and per-block cap; 2^26 units must be measured on reference
  hardware against the block validation budget.
* **GAP-B10: the SDK's fresh verifier** refuses a complete-check attempt (nothing to re-derive); a fresh complete verifier (the artifact
  re-run off chain) is a small SDK addition.
* **GAP-B11: no switch, by design** — bootstrap classes stay sources; the class-capped rule exists if the network wants no single class
  to own a beacon once two bootstrap classes exist.
* **GAP-B13: a public-coin per-claim check is not wired** (SG-05): OPV soundness rests on outsiders' private salts, stated in §6.4.
* **GAP-B14 (G14-for-rewards): the REAL attempt's Final.** §12 opens admission; an admitted attempt of a G14 class still reaches
  Final (and pays) only through its verification route: a Panel licence, or the RFC-0008 slice to the kernel route (X8R). A class
  with no seats has no Panel licence.
* **GAP-B18 (ADR-0177, CODE, DA16b): DA16's artifact half.** Where `palw_provider_court_v1` is armed, availability is still a
  condition of tag 104, of the attested roots and of `onboarding_gate_v1` (§14.1). The reward gate and its activation no longer read it.
* **GAP-B19 (DESIGN/POLICY): a conformance refutation earns no bounty, and evidence carries no stake** (§14.4). A forged sampled post
  costs only a counted retry. A refuter that proves the forgery gains nothing, and pays `dismissed_proof_fee` when it is wrong. Proposed:
  an evidence bond with the 49 % refuter share. Its size is ECON's.
* **GAP-B20 (ADR-0176, CODE: BUDGET): rewards not budgeted.** The channels name their legs and the hook exists (§14.3). No engine
  implements the hook, so no door draws from a budget.
* **GAP-B21 (ADR-0177, DESIGN): sampled eligibility of a closed model.** Its refutation rests entirely on A-ACQ, which the chain may not
  test (§14.2). For the release it is moot: GAP-70 lets no sampled conformance gate a reward. Any digest court must state it.
* **GAP-B15 (G14-for-rewards): what is not gated.** The market (seed / buy) and the work price unit read the registry lifecycle, not
  the reward gate. A non-G14 class past the fence is Registered with no share, so it bears no weight and holds no budget. A pre-fence
  class stays on the OLD Panel route in full (§12).

## 11. Allocations and identity

No new consensus object tag, delta entry, carriage tail, root block or aux table: `PostComplete` is a variant of tag 109's action; the
complete-check judgement lives in the attempt row (aux 39); eligibility is derived, never stored. `PalwPanelFreeFenceV1` changes shape
(`admitted_classes` → `denied_classes` + `min_effective_bits`) and its identity bytes take a domain tag; it is `None` on every preset and
hashed Some-only, so **no testnet-12 params or schedule id moves** (`palw_t12_flag_day_9000`, `rfc0015_*`). The interim onboarding
challenge policy's id changes (its source rule is now the distinct one); it is a constant of the never-armed route, in no params.

The v3 beacon is a new randomness-source id in
the challenge crate; it uses no tag. On chain, v3 needs the allocations named in GAP-B1a. `PalwStateV2Error::ClassNotRewardable` is a
new error (not encoded).

## 12. G14-for-rewards (`docs/PRINCIPLES.md` §6)

**The gap** (H1's devnet; the Lead's 2026-10-09 scope). The V2 lifecycle let a class with `KERNEL_NOT_ACTIVE` and no
PUBLIC_PROSECUTION_COMPLETE move Candidate → Probation once eight seats proved readiness. `activate_due_classes` asked the onboarding
gate only of classes that began onboarding, and the registry's lifecycle reads seats and readiness alone. In the other direction, an
onboarded class (registered with share 0) could take no REAL attempt: there was no epoch budget, and the registry lifecycle, the
Panel room, the verify deadline, seating and the bond share all stood in the way.

**GAP-81 (the Lead's decision, 2026-10-10, from G14C's matrix F17): the new rewards are per CLAIM verification route.** An earlier
version of this gate returned `Passed` for a post-fence V2 class on the strength of its bound KERNEL class. It then admitted that
class's V2 REAL work past the Panel's requirements. But that work is V2 claims with V2 step-tree roots, which the kernel route cannot
convict, so under all-seats collusion only the legacy path could prosecute it, and the legacy path is not G14. Now:

* **A claim's reward channel is its verification route** (`PalwRewardChannelV1`, §14.3). Only a kernel-route claim earns the NEW
  rewards, and each OPV claim is gated by itself (`opv_gate_v1`, E1–E7 at its commit).
* **A V2-root claim rides the legacy channel under the old rules in full**, whatever class it belongs to. It goes through the registry
  lifecycle, the Panel room, the verify deadline, seating, the bond-share split, and the share its class registered with. It never
  earns the new rewards.

**The gate** (`palw_opv_bootstrap_v1::palw_reward_gate_v1`) is now a class's footing for its V2-root claims. It is armed from
`palw_panel_free_v1`'s activation on, through `PalwKernelOpvExtrasV1::reward_gate`; below that it is `Unarmed` and the fold is
byte-identical. Its verdicts:

* `Exempt`: the base class (BASE-0 is the bonded fallback, not useful-computation reward), or a class registered before the fence
  (`LEGACY_PANEL_ROUTE`);
* `Onboarded` (formerly `Passed`): the onboarding gate's identity part is `Ready`, E1–E7 hold through the class's own kernel binding
  (E3 re-derives the bound plan's PUBLIC_PROSECUTION_COMPLETE over its whole context, equal to the registered bounds), and its
  task/context lies inside the plan's `max_positions`. **It grants its V2-root claims nothing beyond the old rules;**
* `Refused { code }` otherwise: a post-fence class that never passed takes no claim at all. The code names the first unmet
  condition: `NOT_ONBOARDED`, the onboarding hold's code, or an E code.

| Door | Armed behaviour |
| --- | --- |
| Registration (`apply_class_registration_v1`) | every post-fence class is written `Registered` (no share written at registration) |
| Activation (`activate_due_classes`) | `Refused` stays `Registered` (no share, no weight, no budget). Otherwise the old path: the onboarding gate, then the share the class registered with. No grant from the gate |
| Claim gate (`check_class_admits_claim`; fold and producer pre-check) | `Refused`: `ClassNotRewardable { code }` on every lane. Otherwise the old rules in full |
| Seating, bond share | the old rules in full (no G14 bypass) |

The graph's node is `V2Onboarded` (formerly `V2Rewardable`). Reaching it means "may take V2-root claims under the old rules", never a
new reward.

**The real-node test** (`g14_rewards_a_v2_claim_of_a_kernel_bound_class_never_earns_the_new_reward`) runs on the drill terms. Before
its conformance, the class is refused by the gate. After it, its kernel class meets E1–E7, and yet its V2-root REAL attempt gets
nothing the old rules do not give: no share beyond the one it registered with, and its pre-check refused by the old rules, since no
seat proves readiness. The helper `Cw::onboarded_v2_claims_on_the_legacy_channel` replaces X8R's `active_admitting_real`, whose premise
(V2 REAL work admitted through the gate) GAP-81 withdraws. Binding a V2 REAL claim 1:1 to a kernel claim of the same job is X8R's
(GAP-62).

**The two channels never mix** (the user's ruling of 2026-10-09, sharpened by GAP-81).

* **Past rights are protected.** A class registered before the fence keeps earning through the OLD Panel route, whose verification
  stays in force in full. The gate exempts it, and no G14 bypass applies to it.
* **New rewards need the new route.** Only kernel-route claims earn them, each through `opv_gate_v1` (E1–E7), whoever registered the
  class.
* **One budget for both channels** (ADR-0176, §14.3). The channels differ in eligibility, never in budget: both draw the producer
  bond's single `Q`/`B`/`R`/`F` budget over `W` (`PalwRewardChannelV1::draws`).
* **Tested both ways** (`g14_rewards_a_legacy_panel_route_class_keeps_the_old_route_and_never_earns_opv_without_the_gate`):
  * a legacy class is not refused by the gate, its program under OPV is `NOT_ONBOARDED`, and its OPV registration is refused;
  * a post-fence class that never onboarded stays Registered, `NOT_ONBOARDED`.

**OPV scope and the interim window** (the same rulings). OPV holds only for a class, plan and task/context for which G14 fully holds
(E3 above). The 50-DAA OPV window (`PalwPanelFreeFenceV1::interim_v1`) is INTERIM and NOT approved for production. Nothing for
rewards, consensus weight or Panel=0 is armed.

**GAP-70 (the Lead's decision, 2026-10-10): only the complete check gates rewards in the release.** A sampled conformance (v2 or v3) is
a non-reward signal. It still runs and records G14_ELIGIBLE, but it satisfies E6 only once a digest court exists: a court for a
vector's logits and commit digests (OB-P0 GAP 2), a vector refutation for a new class (GAP-B6), and closure of the self-reported
implementation results (GAP-B7). That legacy court stays on the DESIGN list. RFC02 §14.18 now has a separate typed candidate
trace statement and bounded element/type/decode refutation, requiring no execution Final.
It does not reinterpret these legacy flat `a`/`b` digests, prove implementation independence,
provide a trace demand/default policy, or satisfy E6. The sampled reward switch remains
refused in production. In code:

* E6 refuses a sampled attempt with `POLICY_NOT_VERIFIED`, citing GAP-70, whatever its effective bits;
* the switch is the fence's `sampled_conformance_gates_reward`. It is `false` in `interim_v1` and hashed Some-only, and
  `validate_value` refuses `true`;
* the graph's `PolicyVerified` now requires `CompleteEvidence`.

**The drill.** The bootstrap's mechanics worlds (`boot_config`) and the conformance worlds on the drill floor (`Cw::over`) set the
floor to 0 and the switch to `true`. That is the drill: they show the mechanism a digest court would unlock. Each world states it,
and validation refuses it on any real network. The release terms are asserted where they decide:
- `eligibility_release`: floor 128, no sampled conformance;
- `g14_rewards_on_the_release_terms_a_sampled_conformance_earns_nothing`.

| Case | Test |
| --- | --- |
| GAP-81: before its conformance the class is refused; after it, its kernel class meets E1–E7, yet its V2-root REAL attempt gets no share grant and no bypass (refused by the old rules); replay | `g14_rewards_a_v2_claim_of_a_kernel_bound_class_never_earns_the_new_reward` |
| GAP-70: on the release terms a sampled conformance passes (G14_ELIGIBLE) yet earns nothing (`POLICY_NOT_VERIFIED`, GAP-70; held Registered, no share) | `g14_rewards_on_the_release_terms_a_sampled_conformance_earns_nothing` |
| A class that never began onboarding never activates (`NOT_ONBOARDED`) | `g14_rewards_a_class_that_never_began_onboarding_never_activates` |
| The channels never mix: a legacy class keeps the old route, and its OPV needs the gate; a post-fence class needs the gate | `g14_rewards_a_legacy_panel_route_class_keeps_the_old_route_and_never_earns_opv_without_the_gate` |
| The graph: `V2Onboarded` and `PolicyVerified` reachable only through the complete check | `palw_opv_bootstrap_v1::tests` |

## 13. C4 round 4 (C4R4b) findings in this lane

**F-C4R4-11 (P1 where armed): fixed.** Free junk refutations exhausted an attempt's per-block runs through the evidence window, so
forged evidence passed unrefuted. Through §12's reward gate, a class could then reach OPV eligibility, with an unbounded payoff. The fix
(`palw_onboarding_fold_v1::apply_conformance_evidence_v1` / `judge_refutation_v1`) has three parts:

1. `charge_route_budget_v1(…, may_spend_reserve)`. A conformance refutation is a proof, so it may spend the runs the kernel reserves
   for proofs (`prosecution_reserved_runs`). A `Post` and a `PostComplete` stop short of them; that is C4R4's F-C4R4-10 rule.
2. One judged refutation per (attempt, refuter bond) per evidence window. The attempt row keeps `refuters_judged` (sorted). It holds at
   most the block's runs × the window's blocks entries, each of which spent a run. A repeat is refused before the charge, at no cost.
3. A charged refutation that proves nothing, on every path, burns `dismissed_proof_fee` from the refuter's bond, which must hold it in
   free collateral at filing.

Junk from `B` bonds can therefore hold a valid refutation off for at most `⌈B / runs⌉` blocks, and pays `B` fees. Carrying forged
evidence through an 80-DAA window at 4 runs a block costs about `320 · dismissed_proof_fee` and 320 bonds. That is a price, not a
bound.

- **Follow-up (not done):** a bounded window extension when valid-looking refutations were crowded out.
- **Restated test:** C4R4's F-C4R4-10 PoC now asserts that every reserved run a junk refutation took was paid for, and that a repeat
  is refused free. Its round-4 assertion (junk stops short of the reserve) contradicts (1).
- **Pinned by:** `g14_c4r4_free_junk_refutations_must_not_carry_forged_evidence_through_its_window` (un-ignored),
  `g14_c4r4_junk_conformance_refutations_must_not_spend_the_runs_reserved_for_proofs`, and the hostile-evidence and forged-evidence
  conformance tests.
- **Row and reads:** `ConformanceAttemptRowV1` gained a field. Op 231 serves the row raw, and the SDK decodes it with the same type.

**F-C4R4-01 (P2, design): recorded.** The distinct source rules tell BONDS apart, not parties. Two Sybil producer bonds and two
Sybil posters own every v2 source position and grind all of them offline (`misaka-palw-challenge/tests/c4r4_grinding.rs` on
`adv/c4r4`).

- No bound in this lane credits distinctness. v2's onboarding accounting already states `G = 2^128`
  (`palw_onboarding_grinding_choices_v1`), as if every position were one party's.
- v3's `G = F` assumes nothing about who holds the positions. It rests on one honest seal's salt staying secret until the seal
  window closes (`ε_src`, §6.3).
- The distinct rules are a cost to an attacker, never a bit.

**Closed inside the bound** (the user's ruling of 2026-10-09: F-01 and F-08 must be closed inside the bound before any audit).

* **v2 (sampled):** E6 charges `G = 2^128` (`palw_onboarding_grinding_choices_v1`). That subsumes any number of competing works
  (F-08) and any party structure (F-01).
* **v3 (sealed):** E6 charges `G = F = 1`, and `ε_src` is the union of two terms. The first is censorship of every honest seal
  (`sealed_source_censorship_bits_v3`). The second is that no honest PARTY seals at all, `PALW_ONBOARDING_SEALED_PARTICIPATION_BITS_V1`.
  That second term is a stated network assumption which no count of bonds can raise. Its interim value is 0, so a v3 attempt is 0
  effective bits until a review supplies it.
* **Never a bit:** the distinct rules and the consumer clause are a cost to an attacker, never a bit.
* **Selection lever:** any statement of a tuple's selection lever must use `competing_works_bound_v1` (96), never the live cap (32).

**F-C4R4-08 (P2, with G14R): re-derived, and the poster agreed.**

- **The bound.** A slot refills at Final, so the competing works per window are `live_cap × ⌈W / OPV window⌉`: 96, not 32. Output
  selection is therefore 14 bits, not 10 (§6.2, `competing_works_bound_v1`, golden vector).
- **The poster.** Once a job's escrow is spent, no ledger state recorded its poster, so the consumer clause never bound. G14R and OPVB
  agreed kernel ledger table 18 `job_posters` (job → poster bond), written past `palw_panel_free_v1` and permanent like job rows, with
  `KernelLedgerV1::job_poster` and `ClaimBeaconSealV1::poster`.
- **Once it lands (GAP-B2):** op 212's attribution and v3's seals carry `consumer = Present(poster)` for post-fence jobs.
- **GAP-B16:** a typed `Spec` claim (R4X) passes `opv_gate_v1`. Its class's eligibility path is RFC-0004 Part II's.

## 14. The user's design changes of 2026-10-10 (ADR-0175, ADR-0176, ADR-0177, ADR-0032 49 %)

OPVB's successor applies them here after merging the integration head `b8ae9412b`.

### 14.1 ADR-0177: no condition here depends on acquiring a model

Every condition of OPV eligibility, of the beacon and of the reward gate, and what it reads:

| Condition | Reads | Acquisition condition? |
| --- | --- | --- |
| E1 kernel Active | the route's schedule | no |
| E2 conformance passed for this statement | the attempt row: the chain's own judgement, or a window that closed unrefuted | no. The condition never asks whether anyone holds the model. Its SOUNDNESS for a sampled attempt is acquisition-conditional (§14.2) |
| E3 the class stands, PUBLIC_PROSECUTION_COMPLETE over its context | the ledger's class row, bounds re-derived now | no |
| E4 the binding stands | the binding row (`ArtifactBindingRowV1::state_at`): Matured or Final, unrefuted | no. A refutation is a proof of inequality (tag 105, two disagreeing openings). Maturity is a clock. A binding nobody could check matures exactly as one everybody checked; that is the soundness gap of §14.2, not a condition |
| E5 bounded resources | the class's bounds and the OPV terms | no |
| E6 effective bits | the attempt's policy and scope | no |
| E7 not denied | the fence | no |
| beacon sources (v2 and v3) | OPV Finals of classes eligible at the commitment (`finals_read_v1`), seal rows | no. A source's own claim material is claim-specific DA, which ADR-0177 keeps |
| v3 veto | a mixed seal unrevealed, or a mixed source that ends without a standing Final | no: both are the sealer's own acts |
| the reward gate (§12) | `onboarding_identity_gate_v1`, then E1–E7 and the task/context check | no. It no longer reads DA16's lapse hold (this branch) |
| V2 activation (`activate_due_classes`) | the old onboarding gate, then the reward gate's refusal | no new condition. Since GAP-81 this door belongs to the legacy channel's old rules, which still include DA16's lapse hold where `palw_provider_court_v1` is armed (the residual below) |

**The one residual is DA16's artifact half** (`palw_provider_court_v1`, dormant). When that fence is armed, artifact availability is
still a condition in four places:

* tag 104 requires at least 2 live leases of distinct operators (the `V2Registered → ArtifactMatured` edge);
* `onboarding_attested_roots_v1` drops a lapsed pair, so the kernel class (and through it the OPV class) cannot register;
* `onboarding_gate_v1` holds a lapsed binding with `AVAILABILITY_REQUIRED`, for V2 activation where the reward gate is unarmed or exempt;
* a lapse lets the binder replace the binding, which turns E4 back to Pending.

ADR-0177 withdraws that half, and readiness §3c gives the re-scope to DA16b. This branch makes the reward gate lapse-blind, and leaves
DA16's own code to DA16b; the legacy channel's V2 activation keeps the hold until DA16b removes it.

- **Arming blocker (CODE, DA16b):** remove `palw_provider_court_v1`'s artifact subject before any OPV or reward fence arms, or never
  arm the two together.

**Names.**
- E4's refusal `DaLapsed` / `DA_LAPSED` is now `BindingNotStanding` / `BINDING_NOT_STANDING`.
- The lifecycle's `AVAILABILITY_REQUIRED` hold for a binding inside its horizon (`misaka-palw-challenge`
  `OnboardingFailureV1::AvailabilityRequired`, op 231, RPC) is a historical name of the same clock. It means "the binding's refutation
  horizon has not passed", never "the bytes could not be obtained". Renaming it changes the wire, so it is left to the release.

### 14.2 G14 is conditional on the verifier holding the model (A-ACQ)

ADR-0177 D7: G14 holds for a verifier that has acquired the registered model; the chain guarantees no such verifier. Nothing in this
design assumes watchers hold the model. Per path:

| Path | What detection needs | A closed model |
| --- | --- | --- |
| complete-check conformance (the bootstrap) | nothing off chain: the post carries the whole artifact, the fold re-roots it (binding equality PROVEN) and runs every input | detection 1, `ε = 0`, unconditional |
| sampled conformance (v2, v3) | a refuter: `LeafDecode` needs the artifact's bytes; `VectorTokens` needs a Final of the bound kernel class, which needs a producer holding the model | **unrefutable**: the window closes "unrefuted" and E2 holds with effective detection 0. E6's bits do not see this |
| an OPV claim of an eligible class | an honest outsider with the model and a private salt, in time (Proposition SG-05, §6.4) | `p = 0` |
| v3's `ε_src` | an honest party that holds an eligible SOURCE class's model and seals (§6.3) | the participation term is 0 (interim 0 anyway) |
| an artifact binding (tags 104 / 105) | a refuter that holds the bytes | the binding matures unrefuted, whatever the bytes are |

Consequences:

* **E6's 128 bits cover check math and challenge manipulation only** (§6.4, §7): grinding, retries, adaptive statements and
  `ε_src`. They never cover the existence of a refuter or watcher that holds the model.
* **The OPV reservation assumes a detection probability.** The interim reservation divides the gain by `assumed_detection_permille =
  500`, which is an assumption about A-ACQ and A-WATCH. It is false for a closed model. ADR-0177 D7's
  `p · (R_risk + L_collectible_net) > C_saved` holds only at the effective `p` (POLICY / ECON; MEAS measures the acquisition-conditional
  `p`).
* **Only a complete-check class is eligible in the release** (GAP-70, decided by the Lead on 2026-10-10, §12). That class's soundness
  needs no acquisition. A sampled conformance is a non-reward signal until a digest court exists. Even then, its refutation would rest
  entirely on A-ACQ, which the chain may not test (ADR-0177 D1). So a digest court alone does not make a closed model's sampled
  conformance sound (GAP-B21).

### 14.3 ADR-0176: the two reward channels and the per-bond budget they draw from

Every reward this lane opens or keeps draws from ONE per-bond budget, the four ADR-0176 ceilings over the common window `W`: `Q`
(claims), `B` (reward blocks), `R` (attributed rewards) and `F` (Final weight). The channels never mix for ELIGIBILITY (§12), but they
share the bond's budget, so earning in both channels never doubles a ceiling.

A claim's channel is its verification route (GAP-81, §12):

| Channel | Which claims | Draws | Reserved at | Re-checked and consumed at |
| --- | --- | --- | --- | --- |
| `LegacyPanelRoute` (the OLD channel) | every V2-root claim: a pre-fence class's on the Panel route, and an onboarded post-fence class's under the same old rules | `Q` (the attempt), `B` (each reward block it derives), `R` (the attempt reward, the subsidy carve), `F` (its Final weight) | attempt admission | each reward block, each payout, maturity, Final |
| `KernelRoute` (the NEW channel) | every kernel-route claim; an OPV class's only while E1–E7 hold at its commit (`opv_gate_v1`) | `Q` (the claim); `R` (the `FinalReward` from the job's escrow); `F` = 0 today | claim admission (`CommitClaim` / `CommitClaimSalted`) | the Final |

Notes on the new channel:
- The `FinalReward` is user-paid from the job's escrow, not new issuance, but it is still attributed to the producer bond, so it
  counts against `R_max`.
- An OPV Final carries no consensus weight today: `work_credit_per_claim` is only a gain bound. Any future weight must be reserved at
  admission.

Rules (ADR-0176 D1–D4):
* Reserve the maximum at acceptance. Re-check at every payout, reward block, maturity and Final.
* `reuse_not_before = d + W`. There is no early recovery on Final, void, default, conviction, `BEACON_VETOED` or a retry.
* Raising `rho` never raises `B`, `R` or `F`.
* Liability collateral (the OPV reservation, the binding reservation, the seal deposit) is separate from the issuance clock.
* **A beacon source is a claim.** It draws `Q` from its producer bond, and owning a source earns nothing beyond that claim's own `R`.

**The hook** is in `consensus/core/src/palw_opv_bootstrap_v1.rs`, section VI:
- `PalwBondBudgetLegV1` and `PalwBudgetEventV1`;
- `PalwRewardChannelV1::draws()`, the table above as data;
- `PalwBudgetRequestV1` and the trait `PalwBondBudgetHookV1` (reserve at acceptance, consume at an event, never more than reserved,
  `d + W`);
- no verdict of `PalwRewardGateV1` names a channel: a claim's route does.

BUDGET's engine (`palw_bond_budget_v1`, dormant) implements the trait. This branch wires no implementation, so no door draws yet.

- **Arming blocker, "rewards not budgeted":** CODE (BUDGET) and POLICY (`W`, the `rho` mapping, the cap values). The refusal of
  `validate_palw_panel_free_v1` names it.

### 14.4 ADR-0032: 49 %, net of self-return

| Bounty path in this lane | Code today | Under the future ruleset |
| --- | --- | --- |
| binding refutation (tag 105) | `PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1 = 500` (OB-P0's constant) | 49 %. Operator ids are self-declared, so a binder can refute itself through a Sybil operator: it recovers at most 49 BILI of a 100 BILI reservation and loses at least 51 BILI. CODE: the constant should follow `palw_reporter_share_v2` (INTF) |
| kernel accuser reward | `accuser_reward_permille: 500` (G14-R4's policy) | the same: G14R / INTF |
| a veto by an abandoned source (the default penalty) | `DemanderShare`: the accuser's share, 50 % to the demanders, the rest burned | at least 51 BILI net per veto (§6.2). The demanders' share follows `accuser_reward_permille`, so it follows the accuser fix above |
| a forfeited seal deposit | burned whole | no bounty: the whole 1 BILI is lost |
| the complete-check fee | burned | no bounty |
| conformance refutation (tag 109 `Refute`) | **pays nothing, and forged evidence forfeits nothing** (the attempt ends REFUTED, a counted retry) | DESIGN blocker (readiness §6.6, verifier incentive). Proposal: an evidence bond `E`, reserved at `Post` and slashed on REFUTED, with 49 % to the refuter. A self-refuting registrant then loses at least 51 % of `E`. `E` is POLICY (ECON) |

### 14.5 ADR-0175: nothing here moves a registration

The bootstrap only READS registrations. Eligibility (derived), the deny-list and conformance attempts are facts about an immutable class
id. Specifically:
- A commitment naming another artifact, program or plan root is `ConformanceOfAnotherStatement` and is never eligible.
- Tag 106 binds once ("a plan or program cannot be substituted afterwards").
- An improvement is a new registration. It onboards on its own conformance, and its parent's eligibility is untouched.

## 15. Arming blockers of this lane's fences (end of lane, 2026-10-10)

Nothing here is armable. `palw_panel_free_v1` (OPV, derived eligibility, the reward gate, the v3 beacon) stays refused by validation.
Its refusal names the blockers below.

| Blocker | Kind | Owner |
| --- | --- | --- |
| **Rewards not budgeted** (ADR-0176): no engine implements `PalwBondBudgetHookV1`, so no door reserves or re-checks `Q`/`B`/`R`/`F` | CODE | BUDGET (`palw_bond_budget_v1`) |
| `W`, the `rho` ↔ `Q`/`B`/`R`/`F` mapping, the cap values | POLICY | Lead / ECON |
| DA16's artifact half still makes availability a condition of tag 104, the attested roots and the legacy channel's V2 activation (ADR-0177, GAP-B18) | CODE | DA16b |
| A digest court for sampled conformance (GAP-70: until it exists, only the complete check gates rewards) | DESIGN | Lead / G14C |
| A conformance refutation earns no bounty, and evidence carries no stake (GAP-B19); an evidence bond with the 49 % refuter share | DESIGN + POLICY | Lead / ECON |
| A fresh complete-check verifier (GAP-B10): the post is in no served row | CODE + allocation | G14C (GAP-71) |
| The 49 % rule for the binding challenger (`PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1 = 500`) and the kernel accuser (`accuser_reward_permille: 500`, which the default's demander share follows) | CODE | INTF / G14R |
| v3's liveness price (GAP-B12): a withheld seal vetoes, counted; `seal_deposit` | POLICY | ECON |
| v3's participation term and `ε_src` at production parameters (`W − δ ≥ 128` blocks at `ρ = ½`, a seal TTL ≥ `2W`) | POLICY + EXTERNAL (review) | Lead / SOUND |
| The OPV reservation's `assumed_detection_permille` (500) is an assumption about A-ACQ and A-WATCH; 0 for a closed model (§14.2) | POLICY / EXTERNAL (MEAS) | ECON / MEAS |
| The interim numbers: the complete check's bounds, fee and per-block cap; 2^26 work units against the block validation budget; the 50-DAA OPV window | POLICY + EXTERNAL (measurement) | MEAS |
| An external soundness review of the effective-bits accounting (`shipped_registry_v1` is empty) | EXTERNAL | SOUND / reviewers |
| Pipelines (GAP-B4) and typed roots (GAP-B16) have no onboarding path; both are eligible only through the `cfg(test)` seam | CODE | G14C (GAP-21, GAP-50) |
| Binding a V2 REAL claim 1:1 to a kernel claim (GAP-62); until then V2-root work never earns the new rewards (GAP-81) | CODE / DESIGN | X8R |
| RFC-0010 V3 does not read the route's OPV Finals, and no Panel beacon scheme is approved (GAP-B3) | CODE + POLICY | Lead |
| Rule E (ADR-0178) armed at or below, for reorg consistency of every Final the beacon reads | CODE | FINX |

## 16. G14 condition 9 — post-commit challenge completeness, this lane's part

G14 (the user's priority, 2026-10-10): with the producer and EVERY Panel seat colluding, one non-Panel public bonded verifier convicts
from public authenticated material alone. RFC-0014 §3.4 requires that verifier to reproduce the whole post-commit path from public
reads, with no producer cache, private API, Panel seed distribution or operator signature. The lane G14C keeps the authoritative gap
matrix. This section is the onboarding conformance path's row of it.

| §3.4 step | Complete check (bootstrap) | Sampled (v2) | Sealed-source (v3) |
| --- | --- | --- | --- |
| policy resolution | `ConformanceAttemptRowV1::policy()` from the commitment's id (op 231) | same | same |
| statement commitment and its position | tag 107's row (op 231) | same | same |
| future source eligibility, independent Final | none: no beacon | op 212's attributed OPV Finals, against the eligible set frozen at 107 | the seal facts (seal position, salt, fate), derived from op 211's rows by the chain's own function |
| ordered source set, branch-relative lock | — | `collect_attributed_work_beacon_v1` | `collect_sealed_work_beacon_v3` |
| seed, identical queries | — (every input is checked) | `challenge_seed_v1`, `derive_selection_v1` | same |
| localization, then exact court or default | the fold computes every check; a failure is `CONFORMANCE_FAILED` in the fold | `LeafDecode` (an opening against the root) or `VectorTokens` (a Final claim): exact; no evidence means a default (`Withheld`) | same |
| the fresh verifier (`fresh_verify_from_reads_v1`, `misaka model onboard verify`) | **refused** (`COMPLETE_CHECK`, GAP-B10) | ops 231 and 212 | ops 231, 212 and 211 (this branch). Before, it was refused (`SEALED_SOURCE`, GAP-B17) |

**GAP-B17 closes without a new RPC op.** Op 211 already serves every route row with the committed ledger and aux roots. The reader
rebuilds its own copy of the route (`PalwKernelRouteStateV1::from_served_rows_v1`, which refuses rows that do not root to the served
roots) and derives the seal facts with `beacon_sealed_sources_v1`, the function the fold uses. The SDK
(`sealed_sources_from_kernel_rows_v1`) and the CLI (`kernel_sealed_sources`, every page of op 211, refused if the roots move between
pages) do that. The node test runs the reader over small pages, checks that it reproduces the chain's v3 beacon output, seed and
passing verdict, both inside and after the window, and checks that one forged row is refused
(`g14_opv_bootstrap_a_sealed_source_v3_beacon_locks_on_salted_seals_and_the_class_passes`).

How §3.4's other rules hold here:

* **Seeds and ε are recomputed, never trusted.** The verifier re-derives the beacon and the seed. E6's bits (`attempt_effective_bits_v1`)
  are a pure function of public rows.
* **Many watchers are not many repetitions.** The bound counts the policy's `r`, never the number of watchers.
* **An exact proof never waits for a new beacon.** A conformance refutation names a selected check of the posted evidence, under a seed
  already locked. A per-claim OPV proof needs no beacon at all (Proposition SG-05).
* **A source shortage never stops the chain.** It ends the attempt `BEACON_UNAVAILABLE` (counted); `…without_a_complete_check_class_the_beacon_never_comes_and_the_chain_lives`
  tests this.
* **No BFT/DNS beacon, no seed-timeout fallback.** The source kinds exclude both, and no fallback exists.

**What remains for condition 9 in this lane.**

| Item | Kind |
| --- | --- |
| A fresh complete-check verifier (GAP-B10). The `PostComplete` lives only in its block, and op 231's evidence row is typed for the sampled post. Fix: keep the post in an aux row op 231 serves (an allocation), then re-run `judge_complete_check_v1` off chain | CODE + allocation (Lead) |
| The served roots are trusted as the node's. Authenticating them against the header-committed state is the state proof's job, shared by every public read (ops 211, 212, 231) | CODE, shared (G14C) |
| Restart, IBD and pruning for a v3 attempt on a real node. The v2 restart test exists (`the restarted node passes the window`); a v3 one does not | CODE (tests) |
| A fake source and a source shortage under v3 on a real node. Both are tested only in the challenge crate's pure tests | CODE (tests) |
| GKR, sum-check or a staged beacon: no interactive suite exists (SOUND SG-10) | EXTERNAL, not applicable yet |
| A sampled conformance of a model no honest party holds is unrefutable (§14.2, GAP-B21). Moot for the release (GAP-70) | DESIGN |
