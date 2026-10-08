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
| RFC-0009 | L2 PALW fork-choice verification for remote clients (needs a state-root version bump and transition verification); the provider court (objective DA responsibility transfer); rail auto-resubmit; the RDA4 `SigningPurpose` | DESIGN / CODE |
| RFC-0006 | non-seat watcher, per-segment pricing, per-shard V3 draw | CODE / DESIGN |
| RFC-0013 | tiled range evaluator, Merkle index | CODE |
| RFC-0002 / 0011 | the HF-majority acceptance bar (RFC-0011: one-sided 95% lower bound ≥ 90% over all public repos); census re-measurement running (COV-P4) | CODE + measurement |
| RFC-0001 | proposals P1–P4 | CODE |
| liveness | a minority that was partitioned ~41 min never rejoins (DNS reorg gate DominanceViolation) and one node deadlocks after "Chain participation held" — found by H1 on the devnet; the code is in int-12 too (LIVE-R1 investigating) | CODE (HIGH) |

## 3. Decisions for the user (collected, not urgent until the code is ready)

1. RFC-0012: D1 and the policy values (D, W, caps, horizon).
2. RFC-0015: the OPV admission list and terms.
3. RFC-0007 Part VI: the challenge policy's security level; approval of a beacon scheme for RFC-0010.
4. GAP-5: the FinalReward funding source.
5. The activation height of the single release, once every row above is clear.

## 4. Change log

* 2026-10-08 — created on the user's decision; rc1 (`release/t12-int13-rc1`) abandoned; `PALW_T12_INT13_DAA` → None (UNSCHED).
