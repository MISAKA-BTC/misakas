# RFC-0016 — implementation status, 2026-10-15

Branch `integ/rfc0016-impl`. **Implemented dormant, not activated, unbuilt.** Nothing was built, checked, tested or run for this
record (a separate validation agent compiles a branch that merges this one); no line below is a PASS. V+ is the adopted DESIGN
(user decision 2026-10-15, RFC-0016 D-V1). Every value is a placeholder (`PALW_RFC0016_VALUES_V1`) and no measured-value record is
pinned (`PALW_RFC0016_MEASURED_RECORDS_V1` is empty), so validation refuses either fence armed. Live testnet-12 is unchanged: both
fences are `None` on every preset, hashed Some-only, so no params / schedule id moves.

Status words: **implemented** = code exists on this branch (dormant, unbuilt, unverified); **test source only** = a test exists as
source, not run; **evidence-only** = needs measurement or a real network; **not written** = no code or test yet; **other lane** = owned
by the ADR-0176/0177 or RFC-0017 lane.

Line numbers are at this branch's commit that added the record.

## 1. Fences and allocations

| Item | Where | Status |
| --- | --- | --- |
| `Params::palw_model_block_rights_v1`, `Params::palw_elastic_final_capacity_v1` | `consensus/core/src/config/params.rs:3157`, `:3162`; presets (4 literals) and `override_params` beside `palw_cell_coverage_closure_v1`; mirror sync `:12976` | implemented |
| identity: collapse `Some(never())`, both destructures, the named list, both Some-only hashes, the fence visitor | `consensus/core/src/config/params/identity.rs:625`, `:1093`, `:1408`, `:1860`, `:2598`, `:3489`, `:4026`, `:5023` | implemented |
| validation | `consensus/core/src/config/params/validation.rs:207`; refusals `consensus/core/src/palw_rfc0016_v1/values.rs:204` (rights: mirror, ConsensusV2, `palw_bond_budget_v1` and `palw_model_bond_allocation_v1` at or below, allocation epoch a whole number of rights epochs, measured-value gate, then "implemented dormant, not activated"), `:243` (capacity: mirror, ConsensusV2, `palw_model_registry` at or below, measured-value gate, not activated) | implemented |
| fork-id probe names | `consensus/core/src/fork_id_v1.rs:934` | implemented |
| V2 mirror `PalwStateParamsV2::rfc0016_v1` (borsh-skip) | `consensus/core/src/palw_state_v2.rs:1909`; constructor `consensus/core/src/palw_state_v2/state_params.rs:180`; accessors `values.rs` (`with_rfc0016_v1`, `rfc0016_rights_active_at`, `rfc0016_capacity_active_at`) | implemented |
| Allocations | **none**: no object tag, kernel inner kind, aux table, delta, carriage tail, RPC op, proto field, p2p version or funded action. V+ consumption is the existing `epoch_counters` | — |

## 2. Values and the measured-value gate (D-σk)

| Item | Where | Status |
| --- | --- | --- |
| Value set (σ, k, φ, pace margin, ε, entrant cap/min/age, capacity safety, C7 ceiling, queue length/TTL, quote margin `h`, verifier share base/slope/target, OPV checks per claim) | `consensus/core/src/palw_rfc0016_v1/values.rs:78` (PLACEHOLDER) | implemented |
| `PALW_RFC0016_SIGMA_PERMILLE_V1`, `PALW_RFC0016_TILT_K_PERMILLE_V1` (named) | `values.rs:99`, `:101` | implemented |
| Measured-value record and gate (placeholder refused; record must match exactly, evidence and review digests non-zero and distinct) | `values.rs:115` (empty list), `:119` | implemented; values **evidence-only** |
| σ/k sweep driver | `scripts/sim/palw-rfc0016-vplus-sim.py` (`sweep`, `conditions`, `scenario`) | implemented (model outputs only; not a measurement) |
| Real-node σ/k measurement harness (reference Qwen2.5-1.5B A16 profile, other CCUs, reference hardware, minimal and multi-miner, E1–E6, A1–A12) | — | **not written** / evidence-only |

## 3. V+ (pre-issued block rights)

| Item | Where | Status |
| --- | --- | --- |
| `N_exp` (the W step's own expectation) | `consensus/core/src/palw_rfc0016_v1/rights.rs` (`palw_rights_n_exp_v1`) | implemented |
| Issue `⌊(N_exp − Ent) · A_m / ΣA⌋`, entrant reserve deducted first, entrants linear and capped, ineligible share expires | `rights.rs:123` | implemented |
| Consumption = per-class epoch counters, per model | `rights.rs:173`; fold `consensus/core/src/palw_rfc0016_fold_v1.rs:69` | implemented |
| Admission: own; past φ σ-capped reissue with the others' pace protected; never past the epoch total | `rights.rs:259`, protection `:192` | implemented |
| The fold gate (Attempts only; floor class exempt) | `consensus/core/src/palw_state_v2/fold_read.rs:974` → `palw_rfc0016_fold_v1.rs:85` | implemented |
| Pre-Attempt quote (rights) | `rights.rs:316`; combined `palw_rfc0016_fold_v1.rs:174`; the producer's existing pre-check `palw_class_admits_claim_v1` asks the same gate | implemented (producer-facts field for the quote: **not written**) |
| Mid-epoch reissue weighted by `A_m` | — | not implemented (first-come among over-demand models; RFC-0016 §8.1 step 4, V2) |
| Optional tilt `k` | `rights.rs:344` (pure) | implemented, **not wired** into the lottery (`k = 1`) |
| B+ comparison `ψ` | `rights.rs:355` | implemented (pure) |
| Interface to ADR-0177 weights (`PalwRightsWeightsSourceV1`, impl for the budget engine's current allocation epoch) | `rights.rs:74` | implemented; 30-day Miner + Verifier `S̄_m` **other lane** |
| Interface to RFC-0017 eligibility (`PalwModelEligibilitySourceV1`; stand-in `PalwAllEligibleV1`) | `rights.rs:98` | implemented; real gates **other lane** (`integ/rfc0017-model-eligibility` not on origin at this record) |
| Heartbeat / clock / floor | untouched: no rights read in the heartbeat lane, the cursor, `bits`, DAA or `palw_work_target_step_v1` | by construction; H1/H2 **not written** (node) |

## 4. Elastic Final capacity

| Item | Where | Status |
| --- | --- | --- |
| Per-verifier water-filling (counted once, no cross-model subtraction) | `consensus/core/src/palw_rfc0016_v1/capacity.rs:32`; fold read `palw_rfc0016_fold_v1.rs:102` | implemented |
| Room past the fence (capacity 0 admits nothing; demand ≤ capacity) | `palw_rfc0016_fold_v1.rs:136`, hook `fold_read.rs:1008` | implemented |
| C7 concurrency derived from capacity, under the measured ceiling | `capacity.rs:67` | implemented |
| Shortage report | `capacity.rs:91` | implemented (RPC: not allocated) |
| Bounded deadline-preserving queue (one reservation per claim, per-bond share, TTL, window from dequeue) | `capacity.rs:127`, share `:187` | implemented as a data structure; **not wired** into the claim lifecycle (quote reads queue length 0) |
| Combined pre-lottery quote and post-quote race (margin `h`; refusal codes) | `capacity.rs:221` | implemented |
| OPV check budget scaled with claims; OPV capacity limit | `capacity.rs:270`, `:276` | implemented (pure); **not wired** into the kernel route |
| Verifier-share slope hook (10–30 %) | `capacity.rs:260` | implemented (pure); **not wired** into the payout split |

## 5. Telemetry / report

| Item | Where | Status |
| --- | --- | --- |
| Report row (targets apart from outcomes) and the outcome rule | `consensus/core/src/palw_rfc0016_v1/report.rs:14`, `:79` | implemented |
| Consensus reader: `P_m` (ppm), issued, physical blocks, rights consumed/unused, Finals, coinbase payout, per-miner revenue | `palw_rfc0016_fold_v1.rs:203` | implemented |
| Escrow payout, refused-for-capacity/rights, queue wait, REAL gap, heartbeat-dependent DAA | fields `None` (UNKNOWN) in the consensus reader | **not written** (node counters) |
| Report tool enforcing "no improvement from a target alone" | `scripts/sim/palw-rfc0016-report.py` (`--self-test`) | implemented |
| RPC op | — | **not allocated**: ops 245–254 / proto 1308–1329 are reserved by other lanes (`137410315`); proposal for the coordinator: op 255, proto 1330–1331 |

## 6. Tests (RFC-0016 §12 ids → sources)

All are **test source only** (not run).

| Id | Source |
| --- | --- |
| T1 | `consensus/core/tests/rfc0016_v1.rs` `t1_more_compute_raises_physical_blocks_within_rights` |
| T2 | `t2_bond_raises_target_and_physical_only_with_compute` |
| T3 | `t3_a_surge_does_not_harden_any_model` |
| T4 | `t4_a_surge_is_reported_and_does_not_shrink_b` |
| T5 | `t5_more_verifiers_more_capacity`; `capacity.rs` unit `a_verifier_is_counted_once_and_disjoint_sets_do_not_interfere` |
| T6 | `t6_rights_do_not_raise_caps_and_unused_rights_expire`; the per-bond cap side is the budget engine's (ADR-0176 lane tests) |
| T7, N1 | `t7_n1_entrant_reserve_is_inside_the_fixed_total`; `rights.rs` unit `issue_follows_weight_and_deducts_the_entrant_reserve` |
| N2, A4, A12 | `n2_a4_a12_splitting_never_gains` |
| N3 | `n3_capacity_zero_admits_nothing`; `capacity.rs` unit `capacity_zero_admits_nothing` |
| N4, A3 | `n4_a3_verifier_capacity_is_never_double_counted` |
| N5, A5 | `n5_a5_queue_reservation_and_squatting_are_bounded` |
| N6, A10 | `n6_a10_post_quote_race_is_absorbed_or_reported` |
| N7 | `n7_invalid_claims_do_not_dilute_honest_pay` |
| A1, A2 | other lane (30-day averaging and seasoning: ADR-0177 allocation) — **not written here** |
| A6 | other lane (independence / attestation): **not written** |
| A7 | `a7_griefing_does_not_inflate_rights` |
| A8, H4 | `a8_h4_rights_are_deterministic` (pure); node reorg/IBD **not written** |
| A9 | `a9_ineligible_models_get_no_rights` |
| A11 | **not written** (lazy verifier: RFC-0015 §8.5.5 open) |
| P4a | `consensus/core/src/palw_bond_budget_v1.rs:5329` |
| P4b | `palw_bond_budget_v1.rs:5363` |
| M1 | `palw_bond_budget_v1.rs:5408`; `consensus/core/tests/adr0176_mode_hold_v1.rs:190`, `:218` |
| M2, M3 | `adr0176_mode_hold_v1.rs:109` (other lane) |
| M4 | `adr0176_mode_hold_v1.rs:167` (other lane) |
| M5 | `adr0176_mode_hold_v1.rs:237`, `:264` (other lane) |
| M6 | `adr0176_mode_hold_v1.rs:292` (fixture); real reference-miner measurement **evidence-only** |
| M7 | `adr0176_mode_hold_v1.rs:139` (fixture); measurement **evidence-only** |
| H1, H2 | **not written** (need a node run with every model's rights at zero) |
| H3 | `h3_rights_do_not_depend_on_the_clock_within_an_epoch` |
| Fences | `fences_are_dormant_and_refused_when_armed`; `values.rs` units |
| C1–C7, E1–E6 | `scripts/sim/palw-rfc0016-vplus-sim.py` (`conditions`, `scenario E1`…`E6`) — model only |

## 7. Compile risks (for the validation agent)

* `palw_state_v2::palw_rfc0016_fold_v1` relies on `use super::*` reaching the parent's private items (`palw_panel_owed_v1`,
  `palw_panel_terms_v1`, `PalwFoldReadV1::outside`, the fields `epoch_counters`, `claims`, `bond_budget`, `own_attempt_class`) and on
  `fold_read`'s `pub(super)` methods (`model_registry_seat_is_ready`, `with_inflight_index`, `model_registry_inflight`).
* `rights.rs` reads `PalwBondBudgetStateV1`'s pub fields (`header.allocation`, `models`); if the ADR-0177 lane renames them, only the
  `PalwRightsWeightsSourceV1` impl changes.
* New `Params` fields are listed at every site `palw_cell_coverage_closure_v1` is. A test that enumerates fence names (e.g. an A-2 or
  fingerprint ledger) may need both names added.
* Merge with `origin/integ/adr-0162-0178` (`31d4288ad`): `preview_claim_for_origin_moded_v1` composes RFC-0016 P4 with ADR-0176's mode-aware hold.
* Merge `883e9c93e` (ADR lane `88b54e3a1`): the ADR-0177 V3 allocation stores synthetic history / block rows in the budget's
  `models` table; `PalwRightsWeightsSourceV1` skips them through `PalwBondBudgetStateV1::is_allocation_aux_row_v1`.

## 8. Overlap with the ADR-0177 V3 block opportunity

The ADR lane's V3 allocation also applies `P_m` to blocks: its `block_opportunity` term reserves each model's reward-bearing block
units against `⌊accrued_blocks · A_m / ΣA⌋` at the budget plan (`palw_bond_budget_fold_v1.rs`, `PalwBudgetRefusalV1::ModelBlockOpportunity`).
`P_m` must be applied to blocks ONCE (RFC-0015 §8.5.5), and the adopted physical-layer design is V+ (RFC-0016 D-V1). So
`validate_palw_model_block_rights_v1` refuses an allocation policy whose `block_opportunity` is on while V+ is armed (one gate, never
two). Which of the two the activation release arms — V+ (physical, pre-Attempt) or the V3 reservation (reward-bearing units at the
budget plan) — is a coordination item for the user and the ADR lane; this branch does not edit the ADR lane's code.
