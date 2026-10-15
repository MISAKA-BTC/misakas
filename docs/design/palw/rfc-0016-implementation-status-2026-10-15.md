# RFC-0016 — implementation status, 2026-10-15 (completion round)

Branch `integ/rfc0016-impl`. **Implemented dormant, not activated, unbuilt.** Nothing was built, checked, tested or run for this
record (a separate validation agent compiles a branch that merges this one); no line below is a PASS. V+ is the adopted DESIGN (user
decision 2026-10-15, RFC-0016 D-V1). Every value is a placeholder (`PALW_RFC0016_VALUES_V1`), no measured-value record is pinned
(`PALW_RFC0016_MEASURED_RECORDS_V1` is empty), so validation refuses either fence armed outside an isolated calibration network. Live
testnet-12 is unchanged: both fences are `None` on every preset and hashed Some-only, so no params or schedule id moves.

Merged in: `origin/integ/adr-0162-0178` (ADR-0176 mode-aware hold, bond formula v2, ADR-0177 V3 allocation) and, through it,
`origin/integ/rfc0017-model-eligibility` and `origin/integ/rfc02-rfc06-12-g14` (merge `61e4cf12c`).

**Every row below is one of three:** **implemented** (code on this branch; dormant, unbuilt), **test source** (written, not run), or
**evidence-only** (needs a measurement or a real network outside the repository). Line numbers are at this record's commit.

## 0. Lead decisions (2026-10-15, pending user review)

| Decision | Where |
| --- | --- |
| `P_m` is applied to blocks ONCE, by V+; ADR-0177 V3 `block_opportunity` must be off whenever V+ is armed; coinbase-side `P_m` stays the existing allocation's (this also resolves the ADR lane's C4) | refusal `consensus/core/src/palw_rfc0016_v1/values.rs:295`; test source `consensus/core/tests/rfc0016_v1.rs` `lead_decision_p_m_is_applied_to_blocks_once` |
| Mid-epoch reissue is `A_m`-weighted (new rooted state); first-come stays the fallback where `A_m` is unavailable | `rights.rs:326`, `:386`; fold `palw_rfc0016_fold_v1.rs:453` |
| RPC op 255, proto 1330–1331 (confirmed) | §5 |

## 1. Fences and allocations

| Item | Where | Status |
| --- | --- | --- |
| `Params::palw_model_block_rights_v1`, `Params::palw_elastic_final_capacity_v1` | `consensus/core/src/config/params.rs:3165`, `:3170`; presets and `override_params` beside `palw_cell_coverage_closure_v1`; mirror sync `:13001` | implemented |
| identity: collapse, destructures, named list, both Some-only hashes, visitor | `consensus/core/src/config/params/identity.rs:629`, `:1102`, `:1419`, `:1878`, `:2622`, `:3518`, `:4060`, `:5063` | implemented |
| validation and refusals (mirror; ConsensusV2; budget + allocation at or below; single block gate; kernel route at or below; whole rights epochs; calibration exception; measured-value gate; "not activated") | `validation.rs:209`; `values.rs:270` (rights), `:325` (capacity), `:239` (calibration) | implemented |
| fork-id probe names | `consensus/core/src/fork_id_v1.rs:936` | implemented |
| V2 mirror `PalwStateParamsV2::rfc0016_v1` (borsh-skip) | `consensus/core/src/palw_state_v2.rs:1917`, `state_params.rs` constructor | implemented |
| **Aux tables 132–135** (131 is RFC-0017's): 132 reissue snapshot, 133 pending queue, 134 queue membership, 135 telemetry | keys `consensus/core/src/palw_rfc0016_v1/ledger.rs`; scanner arms `consensus/core/src/palw_native_resource_v1/wire_kernel_aux.rs:147` and the fallthrough; generator `scripts/drills/gen_rfc06_native_wire.py` (re-run: idempotent with the file) | implemented |
| New refusals `ModelBlockRightsRefused`, `FinalQueueRefused` (skippable for the block's own attempt) | `palw_state_v2.rs:10044`, skip arm `:14440` | implemented |
| RPC op 255 / proto 1330–1331 | §5 | implemented |
| Nothing else: no object tag, kernel inner kind, delta kind, carriage tail, p2p version or funded action | — | — |

## 2. Values, the measured-value gate and calibration (D-σk)

| Item | Where | Status |
| --- | --- | --- |
| Value set (σ, k, φ, pace, ε, entrant cap/min/age, capacity safety, C7 ceiling, queue length/TTL, `h`, verifier share base/slope/target, OPV checks per claim / per verifier-epoch) — PLACEHOLDER | `values.rs:81` | implemented |
| `PALW_RFC0016_SIGMA_PERMILLE_V1`, `PALW_RFC0016_TILT_K_PERMILLE_V1` | `values.rs` (named constants) | implemented |
| Measured-value record and gate | `values.rs:119`, `:160` | implemented; records **evidence-only** |
| Calibration-build values (`MISAKA_RFC0016_CALIBRATION_VALUES`, 18 integers; isolated calibration networks only; all nodes of a run share one build) | `values.rs:125` | implemented |
| σ/k sweep (model) | `scripts/sim/palw-rfc0016-vplus-sim.py` | implemented (model outputs, not measurements) |
| σ/k real-node measurement harness (plan / run / summarize / select; E1–E6, A1, A5, A7, A10 as node scenarios) | `scripts/drills/rfc0016_sigma_k_measure.py` (`--self-test`) | implemented; the runs and their values are **evidence-only** |
| V+ measured in calibration needs `palw_model_bond_allocation_v1` armable in calibration (the ADR lane's validation; without it `ΣA = 0` and V+ issues nothing) | — | **evidence-only** (dependency on the ADR lane) |

## 3. V+ (pre-issued block rights)

| Item | Where | Status |
| --- | --- | --- |
| Issue `⌊(N_exp − Ent) · A_m / ΣA⌋`, entrant reserve deducted first, entrants linear and capped, ineligible share expires | `rights.rs:133` | implemented |
| Consumption = per-class epoch counters, per model | `rights.rs` `palw_rights_consumed_v1`; fold `palw_rfc0016_fold_v1.rs` `palw_rfc0016_rights_consumed_now_v1` | implemented |
| Admission: own; past φ the `A_m`-weighted snapshot (aux 132) or the first-come σ fallback; never past the epoch total | `rights.rs:269`, `:386`; fold `palw_rfc0016_fold_v1.rs:204`; gate hook `fold_read.rs:988` | implemented |
| `A_m`-weighted reissue snapshot at φ (water-filling within σ; conserving; marker row; stale epochs dropped) | `rights.rs:326`; tick `palw_rfc0016_fold_v1.rs:453` | implemented |
| Pre-Attempt quote (rights + capacity + real queue length) | `palw_rfc0016_fold_v1.rs:625`; the producer's pre-check `palw_class_admits_claim_v1` asks the same gate | implemented |
| Optional tilt `k` | `rights.rs:463` (pure) | implemented; lottery wiring deliberately absent while `k = 1` (off) — its use is **evidence-only** (§8.6 measurement, then a decision) |
| B+ comparison `ψ` | `rights.rs:474` | implemented (pure) |
| ADR-0177 weights interface | `rights.rs:74` (skips the V3 engine's synthetic rows) | implemented |
| **RFC-0017 eligibility** (merged): issuance only for a non-probation block right; a Probation class passes V+ (its right is RFC-0017's probation grant); Registered/Lapsed refused by RFC-0017's own gate; entrant clock = Eligible promotion DAA | `palw_rfc0016_fold_v1.rs:42`, `:82`, probation pass-through in `:204` | implemented |
| Heartbeat / clock / floor untouched | no rights read in the heartbeat lane, the cursor, `bits`, DAA or `palw_work_target_step_v1` | implemented (by construction); H1–H3 **test source** (§6) |

## 4. Elastic Final capacity

| Item | Where | Status |
| --- | --- | --- |
| Per-verifier water-filling (counted once, no cross-model subtraction) | `capacity.rs:32`; fold `palw_rfc0016_fold_v1.rs:233` | implemented |
| Room past the fence: capacity 0 admits nothing; demand within capacity admits; otherwise the bounded queue while it has room, else refused | `palw_rfc0016_fold_v1.rs:268`, hook `fold_read.rs:1022` | implemented |
| C7 concurrency derived from capacity under the measured ceiling | `capacity.rs:67` | implemented |
| **Queue wired into the claim lifecycle**: queued at acceptance when demand > capacity (`on_attempt_accepted_rfc0016_v1`); no bind deadline while queued (`palw_provisional_bind_deadline_v1` → `u64::MAX`); `PanelBound` inert while queued; FIFO dequeue as capacity frees with `rebound_daa = now` (bind window and anchor slot start when it leaves); TTL void (`BindTimeout`, not charged); one right per claim (its epoch counter); per-bond capital share refused before any write | `palw_rfc0016_fold_v1.rs:370`, `:496`, `:322`; `palw_state_v2.rs:21156` (accept), `:17148` (deadline), `:20840` (share); `objects.rs:1716` | implemented. Note: a dequeued claim's `rebound_daa` uses the claim's one redraw (a later receipt timeout voids instead of redrawing) |
| Post-quote race: absorbed by the queue within `h`; beyond it the own attempt is skipped (`FinalQueueRefused` / `ModelBlockRightsRefused`), counted node-locally | `capacity.rs:221`; skip arm `palw_state_v2.rs:14440` | implemented |
| OPV route capacity: an OPV kernel claim is refused before acceptance while its model's open OPV claims need more checks than its ready verifiers give | `palw_rfc0016_fold_v1.rs:571`; hook `palw_kernel_route_fold_v1.rs:1439`; `capacity.rs:270`, `:276` | implemented |
| M\*-49 verifier-share slope wired: an accepted claim's panel share = `clamp(base + γ(util − u*), 10 %, 30 %)` | `palw_rfc0016_fold_v1.rs:370`; `capacity.rs:260` | implemented |
| Capacity-shortage reporting | `capacity.rs:91`; quote; report | implemented |

## 5. Telemetry, report and RPC

| Item | Where | Status |
| --- | --- | --- |
| Report rows: targets apart from outcomes | `report.rs:14` | implemented |
| Consensus-derived: `P_m`, issued, physical blocks, rights consumed/unused, Finals, coinbase payout, per-miner revenue, queue length | `palw_rfc0016_fold_v1.rs:663` | implemented |
| Consensus-derived (aux 135): escrow payout per model (noted at the kernel claim's commit, added at its Final reward), queue wait (mean/max at dequeue), heartbeat-dependent DAA (floor Idle); REAL gap from the floor state | `palw_rfc0016_fold_v1.rs:407`, `:606`; kernel hook `palw_kernel_route_fold_v1.rs:860` | implemented |
| Node-local, labelled: refusals for rights / for capacity / queue full | `consensus/core/src/palw_rfc0016_v1/metrics.rs` | implemented |
| The outcome rule (no improvement from a target alone) | `report.rs:84`; JSON `report.rs:104`; tool `scripts/sim/palw-rfc0016-report.py` (`--self-test`) | implemented |
| RPC op 255 `GetPalwRfc0016Report` (proto 1330/1331) | `rpc/core/src/api/ops.rs:333`; model `rpc/core/src/model/message/palw_rfc0016_report.rs`; `rpc/grpc/core/proto/messages.proto:135`, `rpc.proto:3402`; converters, client/server/wRPC registrations; `rpc/service/src/service.rs:3690`; consensus API `consensus/core/src/api/mod.rs:1499`; `consensus/src/pipeline/virtual_processor/processor/palw.rs:1585`; integration arm `testing/integration/src/rpc_tests.rs` | implemented |

## 6. Tests (RFC-0016 §12 ids → sources)

All **test source** (not run) unless marked evidence-only.

| Id | Source |
| --- | --- |
| T1–T7 | `consensus/core/tests/rfc0016_v1.rs` (`t1_…` … `t7_n1_…`) |
| N1–N7 | `rfc0016_v1.rs` (`t7_n1_…`, `n2_a4_a12_…`, `n3_…`, `n4_a3_…`, `n5_a5_…`, `n6_a10_…`, `n7_…`) |
| A1, A2 | node scenario A1 in `scripts/drills/rfc0016_sigma_k_measure.py`; the 30-day average and seasoning are the ADR-0177 lane's code (its tests); the measured result is **evidence-only** |
| A3, A4, A5, A7, A9, A10, A12 | `rfc0016_v1.rs` (named by id) |
| A6 | self-dealing: RFC-0017's commit-then-draw exam and the independence rule own it (RFC-0017 lane tests); RFC-0016's side (no double-counted capital) `n4_a3_…`; the adversarial run is **evidence-only** |
| A8, H4 | `rfc0016_v1.rs` `a8_h4_…` (pure); node `consensus/src/pipeline/virtual_processor/tests/rfc0016_e2e.rs` `t12_rfc0016_a_second_node_and_a_reorg_reach_the_same_state` |
| A11 | lazy verifier: **evidence-only** (RFC-0015 §8.5.5 open) |
| P4a, P4b | `consensus/core/src/palw_bond_budget_v1.rs` `rfc0016_p4_…` |
| M1 | `palw_bond_budget_v1.rs` `rfc0016_a_mode_switch_never_reacquires_issuance_capacity`; `consensus/core/tests/adr0176_mode_hold_v1.rs` (ADR lane) |
| M2–M5 | `adr0176_mode_hold_v1.rs` (ADR lane) |
| M6, M7 | fixtures in `adr0176_mode_hold_v1.rs`; the reference-miner measurement is **evidence-only** |
| H1, H3 | `rfc0016_e2e.rs` `t12_rfc0016_zero_rights_heartbeat_keeps_the_cadence`; `rfc0016_v1.rs` `h3_…` |
| H2 | `rfc0016_e2e.rs` `t12_rfc0016_heartbeat_is_not_tightened_and_earns_nothing` |
| Lead decision (single application) | `rfc0016_v1.rs` `lead_decision_p_m_is_applied_to_blocks_once` |
| Reissue snapshot | `rights.rs` unit `the_reissue_snapshot_is_weighted_capped_and_conserving` |
| Fences, values, calibration form, aux keys | `rfc0016_v1.rs` `fences_are_dormant_and_refused_when_armed`; `values.rs` units; `ledger.rs` unit |
| C1–C7, E1–E6 | `scripts/sim/palw-rfc0016-vplus-sim.py` (model); node runs `scripts/drills/rfc0016_sigma_k_measure.py` — results **evidence-only** |

## 7. Compile risks (for the validation agent)

* `palw_state_v2::palw_rfc0016_fold_v1` relies on `use super::*` reaching the parent's private items (`palw_panel_owed_v1`,
  `palw_panel_terms_v1`, `PalwFoldReadV1::outside`, the fields `epoch_counters`, `claims`, `bond_budget`, `claim_economics`,
  `own_attempt_class`, `extras`, `state`, `params`), on `fold_read` / `transition_builder` `pub(super)` methods
  (`model_registry_seat_is_ready`, `with_inflight_index`, `model_registry_inflight`, `read`, `write_kernel_row`, `void_claim`,
  `disarm_deadline`, `rearm_claim_deadline_dl1_v1`, `write_claim`, `write_claim_economics`) and on RFC-0017's `pub(super)`
  `eligibility_of_v1` / `eligibility_class_view_v1`.
* The JSON renderer formats `Hash64` and `TransactionId` with `Display`.
* `Params` gained two fields at every site `palw_cell_coverage_closure_v1` has (merged beside RFC-0017's `palw_model_eligibility_v1`);
  a test enumerating fence names may need them.
* `consensus/src/pipeline/virtual_processor/tests/rfc0016_e2e.rs` uses `T12Chain` helpers (`heartbeat`, `attempt`, `tip_state`) and
  `PalwChainStateV2::safe_weight()`.
* The regenerated native scanner also caught up ADR-0176's new `PalwBudgetOriginV1` variants (`generated.rs`, `wire_generated.rs`).
