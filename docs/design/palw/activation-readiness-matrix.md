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
| `palw_probabilistic_constraints_v1` (RFC-0011 K2 route + RFC-0014 G14) | real node, dormant; validation refuses arming; 63 `g14_` tests | CODE: C4 round-3 F-05 (OPV lane cap), GAP-R7 accuser seal, GAP-11 flake, GAP-5 FinalReward funding (G14-R4); cached ledger per block instead of O(rows) rebuild (GAP 8); mempool acceptance gate (GAP 10); pipeline claims' header wire form (GAP 6); conformance: a court for vector logits/commit digests, lying-claim griefing of conformance. DESIGN: K2 at real scale — per-position roots, segments ≤ 1,024, a per-prosecution public-byte bound, row-tiled court openings, authenticated prompt tiles (9B-8k refuses on 12.6 TB public bytes, 6.99 GB retained, 8,192 sessions, a 2 GB opening); artifact binding equality needs RFC-0014 §16 availability (GAP 1). POLICY: the challenge policy's security level (interim 2 bits), interim terms. EXTERNAL: soundness review of the composition, 9B-8k measurement | G14-R4 (running) for the CODE items it owns; a K2-scale lane next |
| `palw_panel_free_v1` (RFC-0015 OPV) | real node, dormant (struct fence with admission list + terms) | everything of the K2 route above, plus POLICY: the OPV admission list and terms | with the K2 route |
| `palw_permissionless_panel_v1` (RFC-0010 V3 Panel) | production fold dormant; validation refuses arming | POLICY: an approved beacon scheme (the policy registry's approval list is empty); CODE: the structural guard for V3 receipt/retry expiry vs a non-seat DA default; DESIGN: per-shard V3 draw for sharded classes (RFC-0006). By construction the beacon is unavailable until Panel-independent (OPV) Finals exist, so V3 binds nothing until OPV has run | after the K2/OPV bundle |
| `palw_dns_retirement_v1` (RFC-0012) | dormant; zero-DNS matrix x0–x15 | POLICY: D1 (first `safe` ≥ 5,400 DAA — analysis in `rfc-0012-policy-proposal.md` §10), D, W, operator/class caps, horizon. CODE: a claim reaching Final through the processor with EVM on, real pruned IBD with EVM state, attack tests and `safe` readiness reasons (X12b running). EXTERNAL: wallets/tooling, runbook, drill | X12b (running) |
| `palw_exec_payload_v2` (RFC-0008 v2) | **verified on its branch** `rfc8/x8r-review` @6246e7942 (x8r-m8: consensus 13 incl. the composed slice × G14 real-node run, core 71, fence 7, sdk 3; no drift); `PALW_EXEC_PAYLOAD_V2_ARMABLE = false`; queued for integration after A2U. Open: the REAL root's own Final + capped settlement on the real node (= the G14-for-rewards gap), pipeline-class slices (K2S), the initial boundary not linked to the REAL output (DESIGN_GAP, record §12.3/§13); test seam `exec_v2_test_admit_class_v1` (cfg(test) only) | CODE: the verification route for slices (G14 on slices), suffix void, relay admission/backpressure; review of every un-gated pipeline path it changes (header validation, `deps_manager`, sync, orphan pool, coinbase). DESIGN: flood residual, schedule credit of settled slice work, post-Final liability for slice legs, permit equivocation evidence, anchoring-window strand. EXTERNAL: capacity/liveness drills, recovery drills | a review + integration lane next |

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
| fork-choice safety + partition healing (FINX, ADR-0175) | **rule E adopted (user, 2026-10-09)**: search every tip, count claims over each tip's exclusive past, bonded participation first once the fork is deep; details internal (FINX branch). Ships in the full-activation release and is a **hard prerequisite for arming `palw_dns_retirement_v1`** | CODE (fence; FINX implementing) |
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

* 2026-10-08 ~20:30 — the user's Panel=0 rulings (§3a).
* 2026-10-08 — created on the user's decision; rc1 (`release/t12-int13-rc1`) abandoned; `PALW_T12_INT13_DAA` → None (UNSCHED).
