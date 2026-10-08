# Full-activation readiness — what stands between each fence and arming it

Owner: Lead. Started 2026-10-08 after the user's decision: **no DAA-9,000 flag day; the next public testnet-12 release enables
RFC-0008, RFC-0010, RFC-0011's K2 route, RFC-0012, RFC-0014 and RFC-0015 (and the former int-13 four) in one activation, and ships only
after every RFC implementation is complete.** Until then the integration tree's shipped t12 ids stay the live int-12 release's
(params `5ee7fd8e…`, schedule `1678e073…`); every new rule is a dormant fence.

Kinds of blocker: **CODE** (a lane can write it) · **DESIGN** (the rule is not decided in the RFC/ADR) · **POLICY** (a value or an
economic choice the user decides) · **EXTERNAL** (review, measurement, drill, hardware). A fence is armable only with none left.

## 1. Fences

| Fence (RFC) | Today | Blockers | Owner / next step |
| --- | --- | --- | --- |
| `palw_audit_1004_v1`, `palw_gen_range_twin_v1`, `palw_model_court_window`, `palw_receipt_spend_v4` (former int-13 list) | ready; processor + chain-block E2E (`t12_int13_flag_day_crossing`, `rfc9_v4_chain_e2e`) | EXTERNAL: the drill that crosses them (template: `t12-daa9000-drill-plan.md`, cancelled for 9,000) | rides the full-activation release |
| `palw_probabilistic_constraints_v1` (RFC-0011 K2 route + RFC-0014 G14) | real node, dormant; validation refuses arming; 63 `g14_` tests | CODE: C4 round-3 F-05 (OPV lane cap), GAP-R7 accuser seal, GAP-11 flake, GAP-5 FinalReward funding (G14-R4); cached ledger per block instead of O(rows) rebuild (GAP 8); mempool acceptance gate (GAP 10); pipeline claims' header wire form (GAP 6); conformance: a court for vector logits/commit digests, lying-claim griefing of conformance. DESIGN: K2 at real scale — per-position roots, segments ≤ 1,024, a per-prosecution public-byte bound, row-tiled court openings, authenticated prompt tiles (9B-8k refuses on 12.6 TB public bytes, 6.99 GB retained, 8,192 sessions, a 2 GB opening); artifact binding equality needs RFC-0014 §16 availability (GAP 1). POLICY: the challenge policy's security level (interim 2 bits), interim terms. EXTERNAL: soundness review of the composition, 9B-8k measurement | G14-R4 (running) for the CODE items it owns; a K2-scale lane next |
| `palw_panel_free_v1` (RFC-0015 OPV) | real node, dormant (struct fence with admission list + terms) | everything of the K2 route above, plus POLICY: the OPV admission list and terms | with the K2 route |
| `palw_permissionless_panel_v1` (RFC-0010 V3 Panel) | production fold dormant; validation refuses arming | POLICY: an approved beacon scheme (the policy registry's approval list is empty); CODE: the structural guard for V3 receipt/retry expiry vs a non-seat DA default; DESIGN: per-shard V3 draw for sharded classes (RFC-0006). By construction the beacon is unavailable until Panel-independent (OPV) Finals exist, so V3 binds nothing until OPV has run | after the K2/OPV bundle |
| `palw_dns_retirement_v1` (RFC-0012) | dormant; zero-DNS matrix x0–x15 | POLICY: D1 (first `safe` ≥ 5,400 DAA — analysis in `rfc-0012-policy-proposal.md` §10), D, W, operator/class caps, horizon. CODE: a claim reaching Final through the processor with EVM on, real pruned IBD with EVM state, attack tests and `safe` readiness reasons (X12b running). EXTERNAL: wallets/tooling, runbook, drill | X12b (running) |
| `palw_exec_payload_v2` (RFC-0008 v2) | branch `rfc8/x8-exec-v2`, held out of integration; `PALW_EXEC_PAYLOAD_V2_ARMABLE = false` | CODE: the verification route for slices (G14 on slices), suffix void, relay admission/backpressure; review of every un-gated pipeline path it changes (header validation, `deps_manager`, sync, orphan pool, coinbase). DESIGN: flood residual, schedule credit of settled slice work, post-Final liability for slice legs, permit equivocation evidence, anchoring-window strand. EXTERNAL: capacity/liveness drills, recovery drills | a review + integration lane next |

## 2. Not fences, but "every RFC implemented"

| RFC | Open | Kind |
| --- | --- | --- |
| RFC-0004 Part II (2026-10-08, in the gate by the user) | computation specification with typed roots (`Weights` / `Memory` + update rule / `Retrieval` snapshot + deterministic rule / `Composite`); real-node E2E per kind with outsider conviction and DA default; bounds; census counting | CODE + DESIGN |
| RFC-0009 | L2 PALW fork-choice verification for remote clients (needs a state-root version bump and transition verification); the provider court (objective DA responsibility transfer); rail auto-resubmit; the RDA4 `SigningPurpose` | DESIGN / CODE |
| RFC-0006 | non-seat watcher, per-segment pricing, per-shard V3 draw | CODE / DESIGN |
| RFC-0013 | tiled range evaluator, Merkle index | CODE |
| RFC-0002 / 0011 | the HF-majority acceptance bar (RFC-0011: one-sided 95% lower bound ≥ 90% over all public repos); census re-measurement running (COV-P4) | CODE + measurement |
| RFC-0001 | proposals P1–P4 | CODE |
| G14-for-rewards (lane D GAP 6/3) | H1 on a running devnet: a class whose kernel is KERNEL_NOT_ACTIVE and that has no PUBLIC_PROSECUTION_COMPLETE moved Candidate → Probation once 8 seats proved readiness — the V2 lifecycle consults seats/readiness only, and only classes that began onboarding are gated. From the full-activation release every NEW class must pass the onboarding/G14 gate before it can earn; whether live Panel-route classes are grandfathered is a user decision | CODE + POLICY |
| fork-choice safety (FINX finding V6) | a fork-choice safety finding confirmed on t12's rules; details are internal (FINX branch, not for publication). The fix (C3 or rule E, ADR-0175 draft) ships in the full-activation release (user) and is a **hard prerequisite for arming `palw_dns_retirement_v1`** (the DNS layer currently bounds it) | CODE (fence) + POLICY (C3 vs E) |
| liveness | a minority that was partitioned ~41 min never rejoins (DNS reorg gate DominanceViolation) and one node deadlocks after "Chain participation held" — found by H1 on the devnet; the code is in int-12 too (LIVE-R1 investigating) | CODE (HIGH) |

## 3. Decisions for the user (collected, not urgent until the code is ready)

1. RFC-0012: D1 and the policy values (D, W, caps, horizon).
2. RFC-0015: the OPV admission list and terms.
3. RFC-0007 Part VI: the challenge policy's security level; approval of a beacon scheme for RFC-0010.
4. GAP-5: the FinalReward funding source.
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
