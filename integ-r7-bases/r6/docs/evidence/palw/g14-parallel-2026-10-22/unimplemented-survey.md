# Adopted-but-unimplemented survey (2026-10-22, worktree g14/integration-lanes @ 494ef1e5e)

Read-only survey. No code changed, no cargo run. Method: status lines of `docs/rfc/0006..0035`, `docs/adr/0150..0202`,
`docs/PRINCIPLES.md` §0/§9/§10, decision catalog; then the arming validators listed in
`consensus/core/src/config/params/validation.rs:201-345` and each named implementing symbol, checking production vs test call sites.
Items already covered by the in-flight lanes (see the brief) are not repeated. "grep found nothing" claims are grep-based and may miss
differently named code.

Most adopted rules are deliberately "implemented dormant, refused when armed" (user rule: code first, verify later). Those whose
refusal names only a missing measurement/policy value go to the approval table (§3). Gaps (§1) are refusals that name a **missing rule**,
an **abolished prerequisite**, an **interim/placeholder value contradicting an adopted decision**, or code reachable only from tests.

## 1. Gaps

| id | RFC/ADR | adopted requirement | code evidence (file:line) | gap type | suggested scope | user decision? |
|---|---|---|---|---|---|---|
| U01 | ADR-0171, RFC-0011 §15.7, PRINCIPLES §2 | probabilistic constraint checks are the normal large-model verifier (direction selected 2026-10-06) | `consensus/core/src/palw_probabilistic_constraints_v1.rs:26-36` refuses any armed height: "this binary has no kernel-route acceptance rule"; yet the fold exists, `palw_kernel_route_fold_v1.rs:1-2`, and is used by v3/v7/v9/v11/v23/v28 folds (`palw_flat_execution_da_fold_v11.rs:179`, `palw_onboarding_fold_v1.rs:11`) | unconditional refusal | Replace the name-only refusal with the G14 record-gated pattern (`palw_g14_gates_v1`), and state what the "acceptance rule" means (the kernel-route Final/settlement predicate); the message is stale against the built fold | y — the record contents and which suite counts as "approved" |
| U02 | PRINCIPLES §5, RFC-0010 §0 addendum (1 assigned seat per claim) | Panel=1 is the normal mode | `consensus/core/src/palw_kernel_route_v1.rs:116-117` `PALW_KERNEL_ROUTE_INTERIM_SEATS_V1 = 3`, `..._QUORUM_V1 = 3`; interim domains `:64-70`; fold header "Seats (INTERIM)" `palw_kernel_route_fold_v1.rs:29` | placeholder constant | Make the seat count a versioned policy field (1 for Panel=1), and route the draw through the same seat/beacon as Gate C | y — whether the kernel route follows Panel=1 or keeps a 3-of-3 |
| U03 | ADR-0175 (free registration, immutable) + RFC-0014 §16 | signed registration of a new model is permissionless | `consensus/core/src/palw_onboarding_v1.rs:156-160` "palw_signed_registration_v1 cannot be armed: ... kernel route's onboarding surface ..., which no network can arm" | unconditional refusal (inherited from U01) | Unblock together with U01; registration should be armable without kernel-route seat economics | n (follows U01) |
| U04 | RFC-0014 §16.4 (DA16 provider court, claim-scoped demands only) | court demands only claim-specific units, model bytes never | `consensus/core/src/palw_provider_court_v1.rs:99` refuses: "part of the kernel route's public-DA surface ..., which no network can arm" | unconditional refusal (inherited from U01) | Same lane as U01; gate by record, not by name | n |
| U05 | RFC-0018 §7 Gate C (adopted by user policy) vs PRINCIPLES §9.3 (per-model review abolished 2026-10-15) | Gate C `SecurityCertified` must be armable without a per-model exam | `consensus/core/src/palw_rfc0018_v1.rs:753-759`: `palw_independently_work_certified_v1` needs `palw_model_eligibility_v1` at or below it (to reuse inner object 75); `palw_model_eligibility_v1.rs:1339-1363` refuses: "RFC-0017's rule is not built and verified" and needs the tier fence because "model-level review is abolished" | unconnected (adopted gate depends on an abolished fence) | Move the attestation object (inner 75) and its A-2 gate under the Gate C fence (or under `palw_model_tier_v1`), drop the eligibility dependency; then retire `palw_model_eligibility_fold_v1.rs` per §9 | n (follows §9); coordinate with the Gate C receipt lane |
| U06 | RFC-0026 §2-3, RFC-0014 2026-10-19 §B (3,120 notice / 10,000 hard from original acceptance, adopted 10-19) | every court clock is derived from original acceptance with dependency/retention cover | `consensus/core/src/palw_court_policy_v1.rs:1-8,24-60` `CourtClockBounds::derive` ("Fold integration requires a separately versioned, reviewed ruleset"); only caller `consensus/core/tests/palw_court_policy_v1.rs:13,35`. Live hard deadline remains `trace_retention_daa - w_disclose`, `palw_legacy_public_filer_v1.rs:267-268`, read by `palw_staged_court_fold_v1.rs:1517,1637,3256`, `palw_court_capacity_fold_v1.rs:188`, `palw_legacy_public_filer_fold_v1.rs:501,565,766` | test-only | New versioned fence that switches legacy filer, staged court and capacity folds to `derive` (or an equivalent), refusing claims whose signed dependencies do not cover `a + 10,000` (F07) | y — new fence and whether old claims keep the legacy clock |
| U07 | RFC-0014 2026-10-19 §B/§D | only disputed claims extend; extension moves `collect_until`, disclosure duty, retention, Final hold and bond release atomically; per-turn deadlines stay | grep finds `NOTICE_WINDOW_DAA` only in `palw_court_policy_v1.rs:7,40,64`; no extension writer found in the folds | missing | Atomic "dispute extension" transition on the same fence as U06 | y — same as U06 |
| U08 | ADR-0188 B (R-D2-READY codec approved for isolated implementation 2026-10-21) | D2 seat-readiness digest carried on chain | `consensus/core/src/palw_lifecycle_objects_v2.rs:3024-3026` tag 230 "codec and A-2 kind-table row are not landed"; `palw_court_model_scope_routes_v1.rs:37-38,169,325` | missing | Land the tag-230 codec and A-2 kind row, wire `palw_d2_readiness_digest_v1/_refute_v1` into the fold | n for the codec; y for RFC-0033 supply (unadopted) |
| U09 | ADR-0188 (D2 node path) | D2 claims go through the claim-part route at node level | `docs/adr/0188-d2-x3-arm-records-composition-and-pause-credit.md:70-72` R-D2-NODE BLOCKED (filer demands KernelWitness on a D2 claim) | missing | Filer selects the claim-part route for D2 claims; one node-level e2e like `lg14a_legacy_filer_e2e.rs` | n |
| U10 | ADR-0188 (X3) | X3 composition must survive reorg | `docs/adr/0188-...:72` "X3 reorg replay ... not written" | missing (test) | Reorg replay test with a pre-cutoff reservation and session | n |
| U11 | RFC-0006 R6-CELL round 4 | cell coverage closure | `consensus/core/src/palw_cell_coverage_closure_v1.rs:379-383` "its rule ... is not built and verified yet" | unconditional refusal / rule missing | Build the closure rule or retire the fence | y — RFC-0006 is still marked a proposal (`docs/rfc/0006-...md` status line) |
| U12 | RFC-0008 v2 (ADR-0169: RFC8 uses the existing EXEC lane) | EXEC payload v2 | `consensus/core/src/palw_exec_v2.rs:609-625` "cannot be armed: RFC-0008 v2's section 9 gates (root lifecycle, wire and compatibility, lane ...)" | unconditional refusal | Turn the §9 gate list into checkable records; scope outside the RFC-0008 liability lane | y — which §9 gates are acceptance vs measurement |
| U13 | RFC-0032 A (user-approved isolated implementation 2026-10-21) | non-exclusive terminal material ingress | `docs/rfc/0032-...md:52` checkpoint 7de51563d is a borrowed read-only view: no prompt/row-pin persistence, restart, supply/retention/funding or multi-block submission; borrowed view used only by `palw_court_shared_constraint_builder_v1.rs` | unconnected | Persist the material view, restart/IBD replay, and the ingress queue; check overlap with the RFC-0029 lifecycle lane first | n |
| U14 | PRINCIPLES §0.1/§9.4 (`A_m = c_m · S̄_m^1.5`, 30-day average) vs RFC-0016 2026-10-16 revision | one averaging window | `consensus/core/src/palw_bond_budget_v1.rs:649` comment: "the 7-day average (2026-10-16; 30 days before)"; α interim = 1.5 matches (`:75`) | doc/code inconsistency | Make PRINCIPLES §0/§9 and the code name the same window | y — 7 or 30 days |

## 2. Proposed lanes

| lane | items | size |
|---|---|---|
| K — kernel-route arming surface | U01, U02, U03, U04 | large (validator + policy fields + records) |
| C — Gate C decoupling from RFC-0017 | U05 (+ retire eligibility fold) | medium |
| T — court clock integration | U06, U07 | large (new fence, three folds) |
| D — D2 completion | U08, U09, U10 | medium |
| X — unbuilt-rule fences | U11, U12 | medium, decisions first |
| M — material ingress + consistency | U13, U14 | small-medium |

## 3. Needs measurement / approval only (not gaps)

| fence / item | refusal (file:line) | waiting on |
|---|---|---|
| `palw_bond_budget_v1` (ADR-0176) | `palw_bond_budget_v1.rs:822+` | W, rho→Q/B/R/F, caps |
| `palw_bond_formula_v2` (ADR-0176 amend.) | `palw_bond_formula_v2.rs:541` | u, v_F, λ, κ, prices |
| `palw_model_bond_allocation_v1` (ADR-0177) | `palw_bond_budget_v1.rs:907` | f, epoch, seasoning |
| `palw_model_tier_v1` (ADR-0179) | `palw_model_tier_v1.rs:362` | 0.3/1.0 record, catalog manifest |
| `palw_fork_choice_rule_e_v1` (ADR-0178) | `palw_fork_choice_rule_e_v1.rs:99,102` ARMABLE=false | full-activation release |
| `palw_model_block_rights_v1`, `palw_elastic_final_capacity_v1` (RFC-0016) | `palw_rfc0016_v1/values.rs:385,461` | σ,k,φ; in lane |
| `palw_detection_policy_v1` (ADR-0183.4) | `palw_detection_policy_v1.rs:1386` | per-class measurement |
| D2 / X3 / N4 (ADR-0188) | `validation.rs:6-50`, empty `PALW_G14_ARMING_RECORDS_V1` | records, X3 Δ and c_retain |
| `palw_court_transport_v1` | `palw_court_transport_v1.rs:121` | transport measurement |
| `palw_unresolved_bound_v1` (RFC-0023 s1) | `palw_unresolved_bound_v1.rs:81` | lag measurement, drill |
| `palw_reporter_share_v2` (ADR-0032 49%) | `palw_reporter_share_v2.rs:68+` | approval |
| `palw_verifier_pay_v1` (M*-49) | `palw_verifier_pay_v1.rs:73` | full-activation release |
| `palw_panel_free_v1` (RFC-0015) | `palw_panel_free_v1.rs:198` | §13.3 AND contract |
| `palw_permissionless_panel_v1` (RFC-0010) | `palw_permissionless_panel_v1.rs:187-203` | approved beacon |
| `palw_beacon_bootstrap_v1` (N3) | `palw_beacon_bootstrap_v1.rs:1110+` | BootstrapExact profile |
| `palw_panel_lifetime_v2`, `palw_paid_consumer_lifecycle_v1`, `palw_reporter_income_v2`, `palw_material_income_v3` | `palw_panel_lifetime_v2.rs:185+`, `palw_bond_budget_v1/consumer.rs:102+`, `palw_bond_budget_v1.rs:776,795` | review/validation |
| `palw_minimum_price_v1`, `palw_model_verification_gate_v1`, `palw_horizon_extension_v1` | `palw_econ_contracts_v1.rs:861-911` | measured costs, gate size |
| `palw_tir_shard_segment_v2` (RFC-0006) | `palw_tir_shard_segment_v2.rs:245` | pricing approval |

## 4. Not adopted (Draft/Proposed only)

- RFC-0024 court door 3,200: Draft / Review Required.
- RFC-0025 scalable court: Draft, adoption conditions §17.2 open.
- RFC-0027 / ADR-0193 Miner Account Bond: Proposed (未採択).
- RFC-0033 / ADR-0200 D2 opening supply: Proposed (only ADR-0188 B codec approved, see U08).
- RFC-0034 / ADR-0201 archived-close discharge: Proposed, S1 pending.
- RFC-0019 annual upgrade, RFC-0020 long-context memory: Draft (RFC-0020 partly implemented).
- ADR-0189/0190/0191 TIR door options: Proposed.
- ADR-0193 sequential witness clock, ADR-0194 dormant integration/schema: Proposed.
- ADR-0170 seed-anchor window, ADR-0173 (seat holds a root), ADR-0153 2M sharding plan: Proposed/draft.
- 2026-10-12 amendments in RFC-0006/0007/0008/0010/0011/0012/0014/0015, ADR-0166/0173/0176/0177 ("設計の改定・提案").

Checked and implemented (no gap): ADR-0174 ticker (`wallet/core/src/utils.rs:56-57`), ADR-0175 fence armable (`validation.rs:2961-2975`),
ADR-0183.2 panel floor 240,000 (`palw_panel_economy_v1.rs:90`), ADR-0185 onboarding windows and pinned seed (`palw_claim_challenge_fold_v2.rs:569`),
ECON attestation gate fold (`palw_bond_budget_fold_v1.rs:2359-2362`).
