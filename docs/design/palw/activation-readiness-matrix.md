# Full-activation readiness — what stands between each fence and arming it

Owner: Lead. Started 2026-10-08 after the user's decision: **no DAA-9,000 flag day; the next public testnet-12 release enables
RFC-0008, RFC-0010, RFC-0011's K2 route, RFC-0012, RFC-0014 and RFC-0015 (and the former int-13 four) in one activation, and ships only
after every RFC implementation is complete.** Until then the integration tree's shipped t12 ids stay the live int-12 release's
(params `5ee7fd8e…`, schedule `1678e073…`); every new rule is a dormant fence.

Kinds of blocker: **CODE** (a lane can write it) · **DESIGN** (the rule is not decided in the RFC/ADR) · **POLICY** (a value or an
economic choice the user decides) · **EXTERNAL** (review, measurement, drill, hardware). A fence is armable only with none left.

**Status vocabulary (user, 2026-10-09).** Every row and every report uses exactly three levels and never conflates them:
- **Implemented** — the code exists.
- **Verified** — on the target branch it compiles, and its unit tests, real-node E2E and required attack tests pass.
- **Armable** — verified, and the economic parameters, the external review and the activation conditions are also in place.

Today **no fence is armable**. Integration continues; no new fence for rewards, consensus weight or Panel=0 is armed.

## 1. Fences

| Fence (RFC) | Today | Blockers | Owner / next step |
| --- | --- | --- | --- |
| `palw_audit_1004_v1`, `palw_gen_range_twin_v1`, `palw_model_court_window`, `palw_receipt_spend_v4` (former int-13 list) | ready; processor + chain-block E2E (`t12_int13_flag_day_crossing`, `rfc9_v4_chain_e2e`) | EXTERNAL: the drill that crosses them (template: `t12-daa9000-drill-plan.md`, cancelled for 9,000) | rides the full-activation release |
| `palw_probabilistic_constraints_v1` (RFC-0011 K2 route + RFC-0014 G14) | real node, dormant; validation refuses arming; 63 `g14_` tests | CODE: C4 round-3 F-05 (OPV lane cap), GAP-R7 accuser seal, GAP-11 flake, GAP-5 FinalReward funding (G14-R4); cached ledger per block instead of O(rows) rebuild (GAP 8); mempool acceptance gate (GAP 10); pipeline claims' header wire form (GAP 6); conformance: a court for vector logits/commit digests, lying-claim griefing of conformance. DESIGN: K2 at real scale — per-position roots, segments ≤ 1,024, a per-prosecution public-byte bound, row-tiled court openings, authenticated prompt tiles (9B-8k refuses on 12.6 TB public bytes, 6.99 GB retained, 8,192 sessions, a 2 GB opening); artifact binding equality — **GAP 1 withdrawn by ADR-0177** (refutable from bytes by any verifier that acquired the model; no availability guarantee). POLICY: the challenge policy's security level (interim 2 bits), interim terms. EXTERNAL: soundness review of the composition, 9B-8k measurement | G14-R4 (running) for the CODE items it owns; a K2-scale lane next |
| `palw_panel_free_v1` (RFC-0015 OPV) | real node, dormant (struct fence with admission list + terms) | everything of the K2 route above, plus POLICY: the OPV admission list and terms | with the K2 route |
| `palw_permissionless_panel_v1` (RFC-0010 V3 Panel) | production fold, shard draw, non-seat accusation guard and RPC/CLI implemented, dormant; validation refuses arming | POLICY/EXTERNAL: approved production beacon scheme, bias/withholding/participation and economics (approval registry empty). DESIGN/CODE: objective seal finality and an immutable epoch source-profile snapshot (verification currently reads mutable eligibility). EXTERNAL: fresh cohort with the fleet removed, production-beacon → binding → prosecution → Final/redemption, real pruning-proof recovery. Reference-source node tests cannot pass those gates. See [2026-10-10 audit](../../rfc/0010-dormant-implementation.md) | after the K2/OPV bundle |
| `palw_dns_retirement_v1` (RFC-0012) | dormant; zero-DNS matrix x0–x15 | POLICY: D1 (first `safe` ≥ 5,400 DAA — analysis in `rfc-0012-policy-proposal.md` §10), D, W, operator/class caps, horizon. CODE: a claim reaching Final through the processor with EVM on, real pruned IBD with EVM state, attack tests and `safe` readiness reasons (X12b running). EXTERNAL: wallets/tooling, runbook, drill | X12b (running) |
| `palw_exec_payload_v2` (RFC-0008 v2) | Lead (b8ae9412b): **verified on its branch** `rfc8/x8r-review` @6246e7942 (x8r-m8: consensus 13 incl. the composed slice × G14 real-node run, core 71, fence 7, sdk 3; no drift); `PALW_EXEC_PAYLOAD_V2_ARMABLE = false`; queued for integration after A2U. Open: the REAL root's own Final + capped settlement on the real node (= the G14-for-rewards gap), pipeline-class slices (K2S), the initial boundary not linked to the REAL output (DESIGN_GAP, record §12.3/§13); test seam `exec_v2_test_admit_class_v1` (cfg(test) only) <br> X8R record: branch `rfc8/x8r-review` (X8 + X8R review/integration/completion, merged with this line at `35a9ae1c8`); `PALW_EXEC_PAYLOAD_V2_ARMABLE = false`, armable only on a salted t12 drill | Lead (b8ae9412b): CODE: the verification route for slices (G14 on slices), suffix void, relay admission/backpressure; review of every un-gated pipeline path it changes (header validation, `deps_manager`, sync, orphan pool, coinbase). DESIGN: flood residual, schedule credit of settled slice work, post-Final liability for slice legs, permit equivocation evidence, anchoring-window strand. EXTERNAL: capacity/liveness drills, recovery drills <br> X8R record: CODE: none of this lane's. The composed real-node run is built (record §12): a REAL root on the onboarded kernel-bound class, kernel-claimed slices, an outsider's conviction voiding the suffix and the root, and the honest twin verified through OPV Finals, with a replay. It uses one `cfg(test)` seam that admits the root's class: an onboarded class admitting REAL work under its real budget = the G14-for-rewards gap (OPVB). Pipeline-class slices await a segment state (K2S). Round 2 fixed the leg cap's timing and the post-Final forfeit (record §10b). Done: review of every un-gated path (7 bugs fixed, P11), the verification route through the G14 kernel route, suffix void, relay admission / production backpressure, RPC op 240 + refusal record, `EXEC_SLICE` producer. DESIGN: all five decided (spec amendment 1 §10.3–10.7). Prerequisite: the kernel route fence (`palw_probabilistic_constraints_v1`) — itself refused. EXTERNAL: capacity/liveness and recovery drills (record §7) | Lead (b8ae9412b): a review + integration lane next <br> X8R record: Lead: integrate `rfc8/x8r-review`; swap the seam for OPVB's derived eligibility when it lands |

## 2. Not fences, but "every RFC implemented"

| RFC | Open | Kind |
| --- | --- | --- |
| RFC-0004 Part II (2026-10-08, in the gate by the user) | computation specification with typed roots (`Weights` / `Memory` + update rule / `Retrieval` snapshot + deterministic rule / `Composite`); real-node E2E per kind with outsider conviction and DA default; bounds; census counting | CODE + DESIGN |
| RFC-0009 | L2 PALW fork-choice verification for remote clients (needs a state-root version bump and transition verification); the provider court (objective DA responsibility transfer) — implemented, dormant behind `palw_provider_court_v1` (DA16, merged 501c979ab; open: shared adjudication budget with `Respond` → MEAS, provider Sybil independence, INTERIM numbers); rail auto-resubmit and the RDA4 `SigningPurpose` — **verified on `small/rfc1-13-9-items`** (3607798d1, 94c89f6ea; `SigningPurpose::PalwReceiptAuthV4 = 8`; operator flag `--palw-receipt-spend-v4`, off by default) | DESIGN / CODE |
| RFC-0006 | non-seat watcher, per-segment pricing, per-shard V3 draw — implemented, dormant (SHARD, merged 3c3c25e7b); left: the `palw_tir_shard_segment_v2` height at the release, the PL part C hold (SHARD2) | FENCE |
| RFC-0013 | tiled range evaluator, Merkle index — **verified on `small/rfc1-13-9-items`** (575d9b262: stored `.merkleidx`, `StreamTilingV1`, `palw-class pack index`, beacon opening via a verified sidecar). Limits: only the independent evaluator is tiled (reference/typed/eval_cone are not); nothing measured on a real artifact | MEASUREMENT |
| RFC-0002 / 0011 | the HF-majority acceptance bar (RFC-0011: one-sided 95% lower bound ≥ 90% over all public repos); census re-measurement running (COV-P4) | CODE + measurement |
| RFC-0001 | proposals P1–P4 — **verified on `small/rfc1-13-9-items`** (5c7f7b397, f01425068, 3c16eee55). Limits: P3's canonical door is not called by the node; when it is wired it needs its OWN dormant fence, because `palw_fp_job_v5` is ARMED on t12. P4 has no concrete tokenizer and no gateway route for the Text arm. The cancel frame `MPCX` needs gateway and worker shipped together | CODE (P3 wiring, P4 route) |
| G14-for-rewards (lane D GAP 6/3) | H1 on a running devnet: a class whose kernel is KERNEL_NOT_ACTIVE and that has no PUBLIC_PROSECUTION_COMPLETE moved Candidate → Probation once 8 seats proved readiness — the V2 lifecycle consults seats/readiness only, and only classes that began onboarding are gated. From the full-activation release every NEW class must pass the onboarding/G14 gate before it can earn; whether live Panel-route classes are grandfathered is a user decision | CODE + POLICY |
| fork-choice safety + partition healing (FINX, ADR-0178 — renumbered from 0175, 2026-10-10) | **rule E adopted (user, 2026-10-09)**: search every tip, count claims over each tip's exclusive past, bonded participation first once the fork is deep; details internal (FINX branch). Ships in the full-activation release and is a **hard prerequisite for arming `palw_dns_retirement_v1`** | CODE (fence; FINX implementing) |
| verifier incentive (PRINCIPLES §6 condition 6; C4R4 round 4) | proof bytes don't depend on the verifier's salt, so the liar's own Sybil can seal first and take the whole bounty of its own conviction: deterrence holds, but honest verifiers earn nothing. Timing alone can't fix it. Sketch (G14R, node record §8, end-of-lane item 14): pre-committed watcher salts, with a bounty paid only for a fault inside the watcher's salted sample, split equally among every covering watcher, and priced against MEAS's T_check. Needs a new inner kind and a per-class sampler | DESIGN (blocks rewards under §6) |
| liveness | a minority that was partitioned ~41 min never rejoins (DNS reorg gate DominanceViolation) and one node deadlocks after "Chain participation held" — found by H1 on the devnet; the code is in int-12 too (LIVE-R1 investigating) | CODE (HIGH) |

## 3. Decisions for the user (collected, not urgent until the code is ready)

1. RFC-0012: D1 and the policy values (D, W, caps, horizon).
2. RFC-0015: the OPV admission list and terms.
3. RFC-0007 Part VI: the challenge policy's security level; approval of a beacon scheme for RFC-0010.
4. GAP-5: the FinalReward funding source (decided: user-pays escrow; subsidy carves later).
4a. The claim seal deposit size versus an honest producer's race-loss cost (a losing seal is forfeited by design: a refund would let free Sybil seals choose a beacon claim id).
4b. GAP-B12 (OPV beacon v3): anyone can veto one sealed-source attempt by forfeiting one seal deposit, so R+1 deposits exhaust a
    class's attempts. This is a liveness price set together with 4a. Design: `opv-beacon-bootstrap.md` §6.3.
4c. `grandfather_panel_route_classes` (G14-for-rewards, under `palw_panel_free_v1`; default false). Should Panel-route classes
    registered before the fence keep earning without passing the reward gate?
4d. RFC-0004 Part II memory classes: a lie that reaches Final after the liability horizon taints the memory line permanently,
    so the collateral bar's max gain must include the line value at risk. Options: a per-class declared cap, or a horizon
    stretched to match the line. Source: `rfc-0004-part2-typed-roots.md` §10 and §13.
4e. K2 v4 (real-scale) detection policy: the drawn share q of claims re-executed after commit, and how P_run (the chance a drawn
    watcher actually runs and files) is derived. Per-claim detection is q·P_run; until these are set, v4 classes do not earn.
    Source: `k2-real-scale.md` §11 (SG-06). This is Q-01 made concrete.
4f. ADR-0175 clarifications (INTF audit, 2026-10-10; text proposals for the user, no code change needed):
    - RFC-0004 §II.3's "promoted … with its root recorded on the line" means a new independent registration (`CandidateSelected`).
    - ADR-0175 should state that a Memory registration fixes its update rule and initial memory state, while the line's current
      state evolves as independent material.
    The full-activation release must arm `palw_model_immutable_v1`. `palw_improvement_v1` is already ARMED on t12, so the
    ordering is a release-checklist item, not a validation rule. `palw_typed_roots_v1` (dormant) requires
    `palw_model_immutable_v1` at or below it.
5. The activation height of the single release, once every row above is clear.

## 3a. User rulings on the Panel=0 parameters (2026-10-08 ~20:30)

**Policy: user-pays rewards; an effective 128-bit target; OPV only for G14-complete classes; production numbers and the beacon scheme's
production approval on hold. Panel=0 activation deferred** — it needs measurements, economic safety and beacon independence, not only code.

* **OPV admission** — eligibility DERIVED, not a manual allowlist: Active Kernel + conformance + G14-complete + public DA + bounded
  resource/deadline + a verified beacon policy (the empty set stays the initial value). Window
  `T_challenge ≥ T_beacon + T_fetch + T_check + T_localize + T_file + T_margin`, all measured; a valid prosecution accepted in time halts
  Final until the exact court ends, with bounded accept / reserve / continuation deadlines. The interim 50 / 37 DAA, 1,000 BILI collateral
  and 10 % burn are not production values; collateral from the false claim's maximum gain, concurrent exposure, detection probability
  and the collectable slash (detection probability 0 ⇒ no finite collateral is enough). The 32-slot capture (F-C4R3-05) must be fixed
  against Sybil splitting: reservation cost, collateral exposure, bounded release, prosecution capacity.
* **Challenge policy / beacon** — target an effective 128-bit false-accept bound (retries, grinding, multiple relations, adaptive
  attacks); 2 bits is drill-only; the approval registry stays empty until an external review and bootstrap/grinding attack tests.
  OPV still assumes at least one capable honest verifier within the deadline (RFC-0015).
* **Final reward** — option A, user-pays escrow; invariant `payouts ≤ existing issuance + collected fees + pre-funded`, no double
  counting, no double payment across reorg / re-application / duplicate redemption; a self-posted job pays a non-refundable cost.
  Subsidy carves (B/C) later, separately designed and reviewed.
* **Units** — the coin is Misaka, ticker BILI (ADR-0174); `SOMPI_PER_KASPA` = 10^8 is a legacy constant name for 1 BILI; "KAS" in
  older reports is that legacy label; amounts and units unchanged.

**Bootstrap cycle (checked first, 2026-10-08).** In the code today OPV admission is the fence's manual `admitted_classes`, and an
outsider's check uses its own salt, so an OPV claim needs no beacon: no hard cycle yet. The cycle appears with the ruled eligibility:
conformance needs the PALW Work Beacon, which OB-P0 sources only from Panel-independent (OPV) Finals of other classes; OPV eligibility
and ACTIVE_REWARDABLE need conformance. A non-circular bootstrap source must be designed before derived eligibility is armed — e.g. a
small class whose conformance is a complete deterministic check (no sampling, so no beacon), reaching Final by an independent rule, whose
own activation needs no beacon — and its grinding surface analysed (output selection, withdrawal, Final timing, fork choice, work
concentration).

**Beacon grinding finding (OPV-BOOT, 2026-10-08).** With today's PALW Work Beacon the LAST contributor can steer the output: once
k−1 sources are public, one funded job's nonce is ground offline (`canonical_work_id` changes per nonce) and committed as source k, so a
sampled conformance keeps only `scope bits − h` against an adversary with 2^h offline hashes. Fix (DESIGN blocker for any sampled
approval and for Panel=0): a sealed-source beacon v3 — sources ordered by their claim SEAL position, sealed inside the window, revealed
after it closes, seals bonded (ledger primitives from G14-R4; beacon from OPV-BOOT). The complete-check bootstrap is unaffected (ε = 0,
no randomness).

**Order:** (1) GAP-5 by escrow + coinbase/escrow/reorg accounting tests (G14-R4); (2) the dependency graph and an explicit non-circular
bootstrap path; (3) fresh-verifier time per class measured on real hardware → window and collateral recomputed; (4) C4: grinding,
watcher absence, 32-slot capture, Final race, economic attacks; (5) the production challenge policy and the Panel=0 activation decided
separately.

## 3b. User rulings (2026-10-09): continue integrating; arm nothing for rewards, consensus weight or Panel=0

The user's verdict: MISAKA has moved from RFC design and implementation to verifying whether activation is safe. Integration
continues, but **no new fence for rewards, consensus weight or Panel=0 is armed** until PRINCIPLES (probabilistic detection, public
localization, deterministic adjudication, economic deterrence) is fully closed. The release gate: **for every class and every
Panel configuration, an outside verifier who found a fault can carry an objective adjudication to completion.** Economic problems
(bounties, seal deposits, the beacon, Final rewards) block arming even when the code passes.

**P0 / P1**

| Pri | Problem | Ruling |
| --- | --- | --- |
| P0 | G14 verifier bounty capture | An unsolved economic-safety problem: rewards may not be enabled in production. Proposal A (pay only inside the watcher's salted range) falls to a producer who knows the fault and grinds salts or registers many bonds. Proposal B (equal split) falls to an attacker diluting the honest share with many verifier bonds. **Before choosing, prove that self-dealing does not pay when the producer and the verifier are one economic party (self-fraud, self-accusation, bounty receipt), and that an honest verifier recovers the cost of finding evidence, fetching DA and filing in court.** An attacker merely losing money does not show that honest watchers will participate. |
| P0 | GAP-B12: class exhaustion by abandoned seals | The attack works even at the cost of collateral whenever stalling is worth more than the deposits. The attack cost needs a demonstration. |
| P0 | Beacon grinding / Sybil resistance | Keep the 128-bit target; no approval until the effective soundness is audited. |
| P0 | Fork-choice rule E and partition rejoin | Finish the attack tests; they are the prerequisite for retiring DNS and enabling RFC-0012. |
| P0 | G14R and OPVB builds and real-node integration | Not complete until they compile, pass an independent E2E and pass Final/reorg verification. |
| P1 | K2-TIR-v5 on real models | Prove the court's resource bounds on real encoders, 9B and the maximum geometry. |

**The nine user decisions: recommended policy**

1. **RFC-0012 D1 etc.** — no numbers approved until the safety analysis and the measurements exist; 5,400 DAA is also provisional.
2. **OPV conditions** — only classes, plans and task/context where G14 fully holds; the 50-DAA interim value is not approved.
3. **Beacon** — the effective 128-bit level is approved as the design target; the scheme itself is approved only after external review.
   The 128 bits are the bound to prove over the mathematical check and the challenge manipulation; they are **not** a 128-bit
   guarantee that an honest watcher exists.
4. **Seal deposit / GAP-B12** — do not set the value first. Derive it from the maximum stall time that R+1 abandonments buy, the
   attack cost and the damage value.
5. **Existing Panel classes** — past legitimate rights are protected. Earning the new OPV rewards requires the new gate. If the old
   Panel route remains, it keeps the old verification in full and stays clearly separated.
6. **Memory line collateral** — derived from the maximum exposure, the DA retention period and the objective liability range;
   unmeasured values are not armed.
7. **q · P_run** — computed from real hardware, watcher participation and a grinding evaluation; no unfounded independence assumption.
8. **Activation height** — stays unset; nothing is added at DAA 9,000.
9. **Hotfix deployment** — an independent x86 build, regression tests and fingerprint confirmation first; then, depending on the
   live failure risk, deployment is decided separately from the single release.

**Further instructions**
- When a new fee changes a test expectation (e.g. the 1 BILI non-refundable OPV admission fee), do not just move the expected
  number: assert that the fee is actually collected and that escrow, burn and coinbase agree (conservation). This guards against a
  GAP-5 recurrence.
- HF coverage: D_complete's 37.23% is shape-ready, not the full-task registration rate. Keep reporting real registrations (0 today)
  next to it.

## 3c. User design changes (2026-10-10, merged from `pre` at a224c63f4) and what they change here

| Change | What it means for the release | Lead's application |
| --- | --- | --- |
| **ADR-0175** — registered models are permanently immutable; improvements register independently; Position/AMM never retarget | `palw_model_immutable_v1` (pre's code: lifecycle objects that would mutate a registration are refused past it, the carrying block stands; `model_registration_id_v1`) is a new dormant fence of the release | in the fence inventory; A2U2 adds the refused lifecycle objects to the central table; INTF audits RFC-0004 Part II (memory lines, II.3 "promotion") against it — a memory line is independent material and never moves a registration's head/version/root/Position |
| **ADR-0032 amendment** — the PALW reporter share 10% → **49%** (R-core R-1 and DA-6 exposure) | as merged from `pre` it is unfenced and commits the share into the R-core+ params id, so testnet-12's params id moves and the live 10% history would be re-read at 49% | the ADR itself requires "a versioned migration that does not reinterpret the past 10% accounting": **INTF puts it behind the dormant fence `palw_reporter_share_v2`** (10% below, 49% at/after; Some-only hashed), so the t12 ids stay int-12's and 49% rides the single release |
| **ADR-0176** — bond and a common DAA window bound claims, reward blocks, rewards and Final weight (Q/B/R/F); capacity multiplication preserves total credit; `reuse_not_before = d + W`; reservation at acceptance re-checked at every payout/Final | a new accounting rule over EVERY reward-bearing path (Attempt, Free Prompt, claim-backed blocks, receipts, EXEC slices/riders, escrow, settlement readers, fork choice) — **every reward / consensus-weight fence is blocked until it exists** | new lane **BUDGET** (opus): dormant engine `palw_bond_budget_v1`; W, rho mapping, cap values are POLICY (not set) |
| **ADR-0177** — the chain does not interfere with model acquisition: MISAKA Torrent, Bonded Seeders, Seeder rewards, PoR / Full Fetch / leases / TRDC / FPR and availability-based reward/weight suspension are **withdrawn**; model coinbase allocated by distinct locked miner capital `S_m` through `f(S_m)` inside the existing budget; **G14 is conditional on acquiring the registered model**; court demands are claim-specific (no weight-file/range requests, a cumulative scope) | (a) DA16's artifact-availability half (the `Artifact` lease subject, §2.4 READY gating of tag 104, `AVAILABILITY_REQUIRED`, provider slashing for artifact unavailability) leaves consensus; the claim-material provider court (RFC-0009) stays. (b) "GAP 1: artifact binding equality needs RFC-0014 §16 availability" is **withdrawn** — the binding is refutable by any verifier that has the bytes (tag 105), with no availability guarantee. (c) a new allocation engine | DA16b re-scopes and inventories court units (D2) with K2S2; BUDGET implements `C_{b,m}` / `S_m` / `R_m` behind `palw_model_bond_allocation_v1`; ECON evaluates `f` and open-vs-closed economics (D6); MEAS measures acquisition-conditional `p` |
| ADR numbering | the user's ADR-0175 is "immutable registrations"; next free is **ADR-0178** | rule E (FINX's `0175-fork-choice-…`) is renumbered **ADR-0178** |

New POLICY items for §3: ADR-0176 `W`, the rho ↔ Q/B/R/F mapping and the cap values; ADR-0177 `f`, its range, the allocation epoch/snapshot and its tie to `W`, and the numeric bar for "publication is overwhelmingly better than closed self-funding" (to be fixed before ECON's evaluation); the activation of `palw_reporter_share_v2` with the single release.

## 3d. G14 completion — the user's top priority (2026-10-10)

The user, 2026-10-10: finish RFC-0015's Panel=0 precondition first. Even if the producer and ALL Panel seats collude, then for
computation fraud and for job / input / output / state / DA violations covered by an active plan, ONE public bonded verifier outside
the Panel reaches an objective conviction (or the correct DA default, or the dismissal of an honest claim) from public authenticated
material only, never the producer's secret state. ADR-0177 makes this conditional on the verifier having the registered model.

The authoritative gap matrix is lane G14C's `docs/design/palw/g14-completion-matrix.md` (branch `g14/completion`, 82a176a77).
**No reward-bearing plan family meets G14 today.**

Common to every family:
- no prosecution by a bond registered after genesis (condition 1);
- no fresh node that prosecutes (condition 2);
- the RPC leg of the chain path is untested, because ops 210–212 and 231 are never served by a running node (condition 7).

| Owner | Gaps |
| --- | --- |
| G14R | GAP-00: carry C4R4's fixes; the salted-seal reorg test; green on `b8ae9412b`. It heads the integration order: G14R + A2U, then OPVB, C4R4, K2S, X8R |
| G14C | GAP-01..04: one canonical node harness (post-genesis bond, fresh node / IBD / pruned import, RPC, all seats collude, ADR-0177 non-interference); GAP-10 (other K2 lie types); GAP-71b; GAP-50/51 typed roots; GAP-20/21 pipelines |
| K2S | GAP-32 (merge r4-fixes), GAP-30 (v4 node recovery and lie types), GAP-31 (canonical 8k held), GAP-40 (v5) |
| DA16 | GAP-05 (artifact availability out of consensus), GAP-06 (claim-specific scope), GAP-52 (snapshot binding) |
| X8R | GAP-60 (slice DA default on the node), GAP-61 (the real reward gate in place of the test seam), GAP-62 (initial boundary), GAP-63 (merge and rebuild) |
| OPVB | GAP-71a (public seal read, condition 9), GAP-81 (see below) |
| MEAS | GAP-07: measured worst-case deadlines (condition 8) |

**Lead decisions (2026-10-10):**
- **GAP-81.** The new reward gate is per CLAIM verification route. Only claims verified on a G14-complete route pass `palw_reward_gate_v1`. V2-root claims of a kernel-bound class stay on the legacy channel and never earn the new rewards.
- **GAP-70.** For the release, only the complete conformance check gates rewards. Sampled conformance is a non-reward signal until a digest court exists (DESIGN).

**User decision GAP-80 (2026-10-10): the legacy V2 Panel route must also meet G14.** It is RFC-0014's own core (§4–§7). Two lanes
behind new dormant fences, because the route is ARMED on testnet-12 and the live int-12 rules must not move:
- LG14-A: the common seat/non-seat fraud filer, the non-seat dispute reservation and its Final condition, no pre-emption by a session or
  a court, `ExecutorRefuted` reachable while a court is open, a bystander's accusation never outrun by timeouts.
- LG14-B: hierarchical commitment and independent localization, the three canonical 8k held/fused DA gaps [C12], and the
  consistent-garbage-trace (row 0) / borrowed-trace / state / routing / checkpoint / output cases.
Until both lanes' fences are verified, legacy classes are "not Panel=0, not new-reward eligible".

## 3e. Round / EXEC additional acceptance conditions (user, 2026-10-10)

The user adopted `docs/palw-round-exec-additional-acceptance-2026-10-10.md`:
- compute-proportional Round tickets from verified CanonicalWork and a shared window (120 is a window's capacity, not a per-claim grant);
- the bond's remaining Round rights capped BEFORE the draw's candidate set (`T_candidate <= min(T_earned, BondRemainingRoundRights)`);
- no amplification by bond/operator/claim splitting, resubmission or root/slice repackaging;
- no status priority for operators, genesis bonds or registration order;
- small/large model economics evaluated before and after the cap;
- fee-only Rounds bound either by B_max or by an explicit execution cap.

Eight gates: BUDGET, WORK, WINDOW, NEUTRALITY, SPLIT, SLICE, RECOVERY, ECON. All are UNVERIFIED, and they block the
unified EXEC and the new economic rules. Owners: BUDGET (BUDGET, WORK, WINDOW, NEUTRALITY, SPLIT, RECOVERY's budget half),
X8R (SLICE, EXEC's RECOVERY), ECON (ECON). POLICY for the user: whether fee-only Rounds count against B_max or get their own
execution cap, and whether market fee income counts in R_max.

## 3f. User rulings on ECON's results (2026-10-10)

- **Verifier pay: M\*-49 adopted.** Users pay a per-check fee from the job's escrow, and drawn verifiers are paid first out
  of the unchanged 49%. It is implemented behind a new dormant fence. The lazy-verifier question stays open.
- **The default's demander share is held until the liability horizon.** This closes the 441-vs-490 residual of the single
  49% cap.
- **Model allocation `f(S_m)` stays in the design** (the model leg is not dropped). Keep searching for an answer and parameters
  that make publishing overwhelmingly better. Work on the premise that **bond gathered = users gathered**. An adversary able to
  post more bond than that is read like the same outcome under BFT, outside the security assumption, as a stake majority is
  for BFT. ECON re-evaluates `f` under that honest-capital-majority assumption.
- **W is not fixed now.** ECON's derived range is 280–436 DAA at the interim terms.
- **Follow-up rulings (2026-10-10), on ECON round 2:**
  - **The design: linear `f` with a verification-attestation gate.** Per model-epoch, stake-drawn verifiers (m = 47, k = 24, both an
    unapproved interim policy) each check a sampled Final claim with their own copy, and the allocation is paid only when k attest.
  - **The gate is consistent with ADR-0177 D1.** It withholds only the model-allocation subsidy, and the reason is "verification
    was not attested", never "the model was not served". Claims, escrow, Final, weight and slashing are untouched, and there is no
    availability audit and no slash.
  - **The adversary bound is 1/3** of capital and of verifier stake, as in BFT. The bars fail at 0.40 / 0.44.
  - **BUDGET implements it now**, as an extension of `palw_model_bond_allocation_v1`. The values are an unapproved policy.

## 3g. ADR-0177 goal changed: bond-aggregation advantage (user, 2026-10-10)

The goal is now "strongly favour models that gather more effective locked miner bond", no longer "favour publication itself".
Publication is the means by which other miners can join; equal capital is treated equally whoever owns it.
- "Publish always beats equal self-funding" is removed from the release requirements. ECON's counter-examples stay as the record
  of the old goal.
- The curve candidate is `A_m = S_m^α` (α > 1, compare α = 2) inside the fixed PALW budget. α is POLICY. The new acceptance
  requirement quantifies the advantage: capital range, multiplier, and whether it holds for allocation, actual payment or net
  profit, below the individual caps.
- Large-capital concentration, including adversarial capital, is accepted as residual risk. Inflating small capital
  (double counting, epoch-edge moves, key or claim splitting) stays forbidden.
- **`p = 0` compute-skipping is a separate, unresolved safety gate.** The curve is never its evidence. Reports split "honest
  verifiers function" from "every owner closes, p = 0".
- Unchanged: Q/B/R/F individual caps, the total budget, distinct capital and the common hold. Block issuance, beacon, Final
  weight and fork choice are not scaled by bond aggregation.
- The verification-attestation gate (ECON round 2) may continue as a dormant subsidy condition. It is not a p = 0 resolution.
- **α = 1.5 is the interim policy value (user, 2026-10-10, on ECON round 3).** It passes 52/54 cells against the fixed 2× bar and
  never caps in the 4–30× range. Concentration is 1.4–1.8× reward share per capital share; after migration the top model has 55% of
  capital and 81% of rewards. The value is unapproved for production.

## 3h. The Probabilistic–Economic Security Gate (user, 2026-10-10)

`docs/design/palw/probabilistic-economic-security-gate.md` is a release gate across RFC-0007, RFC-0011, RFC-0014 and RFC-0015.
It applies before any profile becomes ActiveRewardable and before any reward, consensus-weight or Panel=0 fence is armed.

- **Four conditions:** every rewarded relation has a verification path; a proven lower bound on detecting the hardest fault;
  one non-Panel verifier completes objective adjudication; and a negative EV for the coalition of producer, every Panel seat
  and related bonds, `E = (1−p)G − pL − C < 0`, with L counted as collectable net of the 49% self-return.
- **Six test families** T1–T6 and **lane specs** A–F, with ECON, PESG-B, G14 (codex), MEAS and BUDGET as owners.
- **Three counter-examples, any one of which blocks activation:** a single-site fault that is almost never detected;
  collateral or profit that escapes before the slash; honest verifiers who cannot participate economically.
- **Safety bounds independent of economics:** maximum unsettled work weight, maximum external loss, no payout before Final,
  and the d+W hold kept separate from liability collateral.
- **Verdicts are PASS / FAIL / UNKNOWN only.** An unknown G, p or L is never PASS.
- The 128-bit checker soundness `ε_check` and the real conviction probability `p_convict` are never conflated.

**PESG-B result (lane B, merged 96b2060de; `docs/design/palw/pesg-b-detection-bounds.md`).** Exhaustive single-fault search
on a real 5-position claim (3,570 placements per mode, isolated and self-consistent), run through the real verifiers and courts.

| Profile | `p_check` lower bound | Verdict |
|---|---|---|
| v1 / v2 / v3 pipelines, whole-claim read with a private salt | 226 / 150 bits | PASS only if the whole-claim read fits `P_run` (MEAS) |
| v5, retrieval, complete-check bootstrap; v4 route B on every claim | binding only | PASS (route B costs one re-execution per claim) |
| v4 route A (sample `m` of `P` positions) | `m/P` (2^-10 at 9B-8k, m=8) | FAIL |
| Sampled conformance; legacy V2 under full collusion | ~0 / 0 | FAIL |
| v4 route B on a `q`-sample of claims | `q·(1−ε_src)` at best; the draw is not wired | UNKNOWN |

**F-B1 is fixed in the Codex review branch**: `generation_length_matches_v1` requires exactly the job's positive
`max_new_tokens` at admission for generative v1–v4 and pipelines. A short correct prefix is refused before reward eligibility;
non-generative zero-output routes keep their own rule. The historical wire name is unchanged, but this dormant semantic
tightening still requires consensus review. This closes the length-binding counterexample only. Also open: N1 (conformance logits and commit digests have no court),
F-B3 (grinding uses the exact escape `1−(1−ε_sel)^G`), F-B4 (interim Panel seeds can be ground from the claim id),
F-B5 (legacy samplers are slightly biased) and F-B6 (sampled conformance has no power against sparse faults).

## 5. Lanes for every remaining item (user, 2026-10-08 ~20:40: 「未完了の残りに対してもエージェントを立てて完了して」)

Waves, because the Mac (32 GiB, ~40 GB free disk) cannot build a dozen targets at once, and builds are batched (user rule): each lane
designs and writes first and builds at milestones with one cargo invocation; a new wave starts as a running lane ends.

| Wave | Lane | Model | Covers |
| --- | --- | --- | --- |
| running | G14-R4 | opus | RFC-0014/0015: escrow Final reward (GAP-5), 32-slot capture (Sybil-robust), accuser seal (GAP-R7), GAP-11; arming-blocker list |
| running | OPV-BOOT | opus | RFC-0015/0010/0007: dependency graph, non-circular beacon bootstrap, derived OPV eligibility, grinding table, effective-bits function |
| running | LIVE-R1 | opus | liveness: the IBD-candidates lock deadlock (in int-12 → node-only hotfix) and the post-partition economic-comparator split |
| running | UNSCHED | sonnet | no DAA-9,000 flag day; ids back to int-12 |
| running | COV-P4 | sonnet | RFC-0002/0011 census (two denominators, eight buckets); T5 preflight verdict |
| running | H1 | — | real-checkpoint devnet loop (9B, Llama, Mitsuba …) |
| 1 | X8R | opus | RFC-0008: review every un-gated pipeline path of `rfc8/x8-exec-v2`, integrate it, then the slice verification route (G14 on slices), suffix void, relay backpressure, the five design gaps |
| 1 | K2S | opus | RFC-0011 K2 at real scale: per-position roots, segments, per-prosecution public-byte bound, row-tiled court openings, authenticated prompt tiles (262k / 2M), cached ledger per block, mempool gate, pipeline header wire |
| 2 | X12 (resume) | opus | RFC-0012 C1–C11 code items |
| 2 | DA16 | opus | RFC-0014 §16 transport (artifact availability → binding equality, held 8k) and RFC-0009's provider court (objective DA responsibility transfer) |
| 2 | SMALL | sonnet | RFC-0001 P1–P4, RFC-0013 tiled range evaluator + Merkle index, RFC-0009 rail auto-resubmit and the RDA4 SigningPurpose |
| 3 | SHARD | opus | RFC-0006 non-seat watcher, per-segment pricing, per-shard V3 draw; RFC-0010 V3 receipt/retry-vs-DA-default guard |
| 3 | L2FC | opus | RFC-0009 L2: verifiable PALW fork choice for remote clients (state-root version, transition verification) |
| 3 | MEAS | sonnet | RFC-0015 fresh-verifier timings per class on real hardware → window and collateral formulas |
| 3 | HFX | opus | RFC-0002/0011 the census's top software-closable blockers as generic features |
| 2 | R4X | opus | RFC-0004 Part II: typed-root computation specifications — `Memory`, `Retrieval`, `Composite` classes end to end under G14 |
| 4 | C4R4 | opus | independent attack round: grinding, watcher absence, 32-slot capture, Final race, economics, partition rejoin |
| 4 | SOUND | opus | RFC-0007/0015 review dossier for the external soundness review (the review itself stays EXTERNAL) |

## 4. Change log

* 2026-10-10 — the user's design changes merged from `pre` (§3c); SMALL, SHARD2 and X12N merged; lanes restarted with the changes; new lanes INTF and BUDGET.

* 2026-10-08 ~20:30 — the user's Panel=0 rulings (§3a).
* 2026-10-08 — created on the user's decision; rc1 (`release/t12-int13-rc1`) abandoned; `PALW_T12_INT13_DAA` → None (UNSCHED).
