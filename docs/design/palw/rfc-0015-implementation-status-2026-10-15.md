# RFC-0015 (Panel=0) — implementation status, 2026-10-15

Branch `integ/rfc02-rfc06-12-g14` (RFC02 + RFC06/07/08/12 + G14 integration). **Implemented, dormant, unverified.** Nothing was
built, checked, tested or run for this record (user rule: code first, validation later); no line below is a PASS. Panel=0 stays
**deferred** (RFC-0015 amendment 2026-10-12, ruling N5): `palw_panel_free_v1` is `None` on every preset and validation refuses
every armed height outside an isolated calibration genesis, now through an authenticated activation gate record whose pin list
is empty. Live testnet-12 behaviour is unchanged (no preset value, no params / schedule id, no wire tag moves).

Earlier record (kernel reference level, 2026-10-08): [rfc-0015-panel-free-record.md](rfc-0015-panel-free-record.md). Values
that need the user: [rfc-0006-0012-user-decisions-2026-10-15.md §RFC15](rfc-0006-0012-user-decisions-2026-10-15.md).

Status words: **implemented** = code exists on this branch (dormant, unbuilt in this round, unverified); **test source only** =
a test exists as source, not run here; **evidence-only** = needs measurement, review or a real network outside the repository;
**user decision** = a value or ruling; **GAP** = repository-implementable and not done.

Where the OPV lifecycle lives: the kernel route (`misaka-palw-kernel`, folded into `PalwChainStateV2` by
`palw_kernel_route_fold_v1`). The V2 Attempt route (`palw_state_v2`'s `Provisional → PanelBound → ReceiptLicensed → Final`) is
**not** given a Panel-free path: V2 claims keep Panel rules (§11.2), and V2 REAL work earns new rewards only through the
RFC-0008 slice to the kernel route (X8R, GAP-62 / GAP-81).

## 1. This round's changes (2026-10-15)

| Change | Where |
| --- | --- |
| **§13.3 `ACTIVATION_ALLOWED` as code**: `PalwPanelFreeGateRecordV1` — 12 conjuncts (the ten of §13.3, PESG 2026-10-10, the N5 lift), each PASS with a unique non-zero evidence digest and independent review digest; bound to the genesis hash, the fence height, the fence's terms digest, the `palw_bond_budget_v1` and `palw_model_bond_allocation_v1` policy digests; requires `palw_probabilistic_constraints_v1`, `palw_bond_budget_v1`, `palw_model_bond_allocation_v1`, `palw_court_model_scope_v1`, `palw_fork_choice_rule_e_v1` armed at or below; its digest must be in `PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1` (empty) | `consensus/core/src/palw_panel_free_gate_v1.rs:39` (pins), `:49` (conjuncts), `:281` (`verify_v1`) |
| The fence's value carries the record (`activation_gate`, `None` everywhere; hashed Some-only) and the refusal runs it | `consensus/core/src/palw_panel_free_v1.rs:66`, `:178` |
| **§8.3 at the OPV door**: where a gate record is present, an OPV claim with no `palw_bond_budget_v1` plan is refused (budget refusal path) | `consensus/core/src/palw_kernel_route_fold_v1.rs:1417`; `PalwKernelOpvExtrasV1::budget_required` (`palw_kernel_route_v1.rs`), set in `consensus/src/pipeline/virtual_processor/processor/palw.rs` |
| Public kernel verifier (the shared filer of the kernel route) also runs where `palw_panel_free_v1` is in force | `kaspad/src/palw_kernel_verifier.rs:212` |
| §13.1 negative gate tests | `consensus/core/tests/rfc0015_activation_gate.rs` (test source only) |
| §13.2 node E2E additions: old Panel claim across the fence, §8.3 budget on the OPV door (`d + W`, Q=1, no B/F, liability collateral kept past `d + W`), unbudgeted OPV claim refused under a gate record, direct proof over another bond's open demand, concurrent claims of one bond, a self-prosecuted lie (fake bounty loop) | `consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/rfc15_panel_free.rs` (test source only) |

Allocations: **none** — no object tag, kernel inner kind, aux table, RPC op, proto field, p2p version, funded action, carriage tail
or delta. The record is a value of an existing fence; its hash domains are `misaka-palw/panel-free/{activation-gate-record,terms,
allocation-policy}/v1` and `palw_panel_free_v1/activation-gate-record/v1` (identity hasher).

## 1b. Lead defaults and gap closure (2026-10-15, second round; lead defaults pending user review)

| Change | Where |
| --- | --- |
| Gate also requires `palw_staged_public_court_v1` (G14 public prosecution) and `palw_verifier_pay_v1` (monitoring economics); `palw_staged_tir_route_v1` / `palw_staged_float_constraint_v1` only when the record's bound rewarded profile set (`PalwPanelFreeRewardedProfilesV1`) includes TIR / Float32 classes | `consensus/core/src/palw_panel_free_gate_v1.rs` |
| **Every network with an OPV policy budgets every Panel-free claim** (calibration and test-armed worlds included): `budget_required` is always set by the processor; unbudgeted OPV kernel claims and native funded v11 / text29 / stage14 / stage28 work are refused (`unbudgeted_panel_free_v1`) | `palw_bond_budget_fold_v1.rs`, `palw_kernel_route_fold_v1.rs`, `palw_flat_stage_fold_v14.rs`, `palw_flat_stage_fold_v28.rs`, processor `palw.rs` |
| Isolated calibration34 arms `palw_bond_budget_v1 = always + opv_mechanics_probe_v1` (binary constant; snapshot encoding unchanged; **calibration params identity changes**). Calibration deviation: the budget's `palw_audit_2026_09_23` / `palw_economic_safety` prerequisites are not required there | `palw_native_calibration_v34.rs`, `palw_bond_budget_v1.rs` (`validate_palw_bond_budget_v1`) |
| Private complete-check profile arms the mechanics budget (the paid profile replaces it) | `kaspad/src/private_integration.rs` |
| Harnesses arm the mechanics budget beside `palw_panel_free_v1` (`Params::arm_palw_opv_mechanics_budget_v1`, `with_opv_budget`) | g14 E2E suite (all OPV configs), `r4x_typed_roots_e2e.rs` |
| **One settlement predicate for every reader of a kernel `Final`** (`pending` / `final-revocable` / `settled` / `convicted` / `defaulted`; fraud-expiry = the claim's `liability_until`); the EXEC work-slice outcome now uses it (behaviour-preserving); the claim read and RPC carry it | `consensus/core/src/palw_kernel_settlement_v1.rs`; `palw_exec_v2_verify.rs` (`palw_exec_v2_claim_outcome_v1`); `KernelClaimReadV1.settlement/settled_from_daa`; RPC `GetPalwKernelClaimResponse.settlement/settled_from_daa` (Borsh wire v2, proto fields 35/36 of that message) |
| Bond duty gate (collateral lock and conviction opening) also on every network with an OPV policy, so an OPV reservation, the budget window `d + W` and the liability hold `H_L` hold the collateral even below `palw_audit_2026_09_23` | processor `palw.rs` (two lock sites), `transition_builder.rs` (`open_conviction_v1`) |
| Web: `#/claim/<id>` per-claim view (mode, challenge window, settlement, "No challenge ≠ proof of correctness") | `web/misaka-options/claim-assurance.js`, `app.js` (`pageClaim`), `index.html`; `claim-assurance.test.cjs` (not run) |
| Node E2E sources: collateral exit / re-registration wait for `d + W` and the liability horizon; slash-then-new-claim fits the remaining capital beside the old consumption | `g14_kernel_route_e2e/rfc15_panel_free.rs` |

Work-slice / native settlement readers (§9): an EXEC slice verified by an OPV kernel claim follows the shared predicate (Final at
the window rule verifies it; a conviction within liability proves it false; its leg is capped at the claim's reservation and held
to the EXEC horizon, which includes every dependency's `liability_until`). Kernel-route claims reserve `B = F = 0`, so an OPV
`Final` adds no consensus weight and no native-settlement work of its own; RFC-0012 maturity of the V2 root already folds in the
kernel horizons (`palw_exec_v2_rewards_v1::horizon`). The native EVM has no direct reader of kernel claims.

## 2. §12 implementation map

| §12 row | Required check | Code | Tests | Status |
| --- | --- | --- | --- | --- |
| mode / claim lifecycle | new Challengeable + ruleset, never an empty-quorum licence | `misaka/palw/kernel/src/mode.rs:27` (`VerificationModeV1`), `:50` (`class_id_for_mode_v1`: mode in class id); `misaka/palw/kernel/src/lifecycle.rs:78` (`Challengeable`), `:169` (`OpenChallengeWindow`, tally in it = error); `misaka/palw/kernel/src/ledger.rs:1043` (`apply_panel_tally` refuses OPV); `consensus/core/src/palw_kernel_route_fold_v1.rs:1452` (no seats / assignment for OPV) | `k2_opv.rs` `empty_receipts_a_zero_quorum…`, `an_honest_optimistic_claim_finalizes…`; node `g14_opv_an_honest_claim_finalizes_with_no_panel…` | implemented |
| public admission | G14 for every reward profile, non-operator qualification, material | derived eligibility `consensus/core/src/palw_opv_bootstrap_v1.rs:557` (`opv_eligibility_v1`, E1–E7) at the door `palw_kernel_route_fold_v1.rs:403` (`opv_gate_v1`); kernel `opv_register_gate` / `opv_class_economics` / `opv_claim_capacity` (`ledger.rs:1719`, `:1736`, `:2116`); deny-list + 128-bit floor in the fence | `k2_opv.rs` (registration gates), `opv_bootstrap.rs` E2E; `canonical.rs` post-genesis bond | implemented; G14 on every rewarded profile = **evidence-only** |
| court surface | licence-less claim prosecutable; no pre-emption | the kernel court is mode-independent: `FileProof`, `FileDemand`, `Respond`, chunked filings; G14R sealed bounty (copyist never takes the bounty) | `g14_opv_a_lying_claim_is_convicted_by_a_fresh_outsider_before_final`, `…_a_lie_that_finalized…`, `c4r3.rs` `opv_relayed_lies…`, `rfc02t_sealed_bounty.rs`; new `rfc15_e2e_a_direct_proof_lands_over_another_bonds_open_demand` | implemented; new case test source only |
| shared filer | fresh ordinary bond, public data only, to proof / court | `kaspad/src/palw_kernel_verifier.rs` (discover → re-execute → `misaka/palw/remote/src/pursuit.rs:234` `plan_kernel_pursuit_on_v1` → send), mode-agnostic; armed under Panel=0 this round (`:212`); SDK/RPC reads op 211 (`from_served_rows_v1`) | `recovery.rs` (`…opv_a_pruned_import_node…`), `socket_rpc.rs`, `canonical.rs` `opv_*` | implemented; real-network run = **evidence-only** |
| Final / DA / locks | admitted DA, hard deadline, unresolved dispute blocks Final, producer collateral | kernel `Challengeable → WindowClosed → Final`, `ProofGrace` holds Final for an open service (`ledger.rs:3086`); hard deadline `misaka/palw/kernel/src/opv.rs:174`, budget sum `OpvPolicyV1::validate` (`opv.rs:199`); DA demand → default with burn; liability horizon convicts after Final | `k2_opv.rs` `spam_demands…`, `a_proof_in_the_windows_last_block…`, `withheld_material…`; node `g14_opv_spam_demands_cannot_hold_final…`, `g14_opv_withheld_material_defaults…` | implemented |
| economics | no signer lock, collectible ≠ nominal, §8.3 class-independent caps + common `W` on every path | OPV reservation `opv.rs:98` / `:163` (`required_reservation`), slash clamp, bounty share (`PALW_KERNEL_REPORTER_SHARE_PERMILLE_V1`, 490 ‰), default burn, admission fee; ADR-0176 engine `consensus/core/src/palw_bond_budget_v1.rs` (caps, `reuse_not_before = d + W`, liability hold apart), kernel hook `palw_bond_budget_fold_v1.rs:1503` (`Q = 1`, `R` = escrow, `B = F = 0`) and `:1546` (Final reward cap); **gate couples the budget fence and refuses unbudgeted OPV claims** | `k2_opv.rs` economics tests; `palw_bond_budget_v1.rs` unit tests (`split_capital_never_raises_caps`, `release_only_at_d_plus_w…`, `honest_and_forger_fill_to_the_same_ceilings`); `palw_bond_budget_e2e.rs`; new `rfc15_e2e_an_opv_claim_draws_the_bond_budget…` | implemented; values = **user decision**; collateral review = **evidence-only** |
| block / settlement | stage weight, unique work, safe = Final | kernel carriers create no reward block or consensus weight (B = F = 0 reserved); one claim per job (`canonical_work_id`); OPV Finals exported as `PanelIndependent` beacon facts (`palw_kernel_route_v1.rs:714`), withdrawn on post-Final conviction; RFC-0010 V3 does not read them (GAP-B3); V2 REAL ↔ kernel 1:1 binding open (GAP-62/81) | `g14_opv_an_honest_claim_finalizes…`, `g14_opv_beacon_sources_keep_the_known_payer…`, `…_a_lie_that_finalized…` | implemented: the kernel settlement predicate is shared by the EXEC slice outcome and the claim read/RPC (§1b); RFC-0010 V3 beacon reader of OPV Finals = **GAP** (GAP-B3) |
| network upgrade | gate unmet ⇒ no activation; old history unchanged | fence `None` on presets, Some-only fingerprints, `for_each_fence`, fork-id probe; A-2 rows for inner 13/14/20… (`consensus/core/src/palw_lifecycle_objects_v2.rs:1743`, row table); **activation gate record** (`palw_panel_free_gate_v1.rs`) | `consensus/core/tests/rfc0015_panel_free.rs`; new `rfc0015_activation_gate.rs` | implemented; gate tests source only |
| UI / docs | mode & assurance; no-challenge ≠ proof | `FinalAssuranceV1::statement` (`opv.rs:344`); `KernelClaimReadV1.mode/opv` (`palw_kernel_route_v1.rs:630`, `:762`); RPC `GetPalwKernelClaimResponse.mode/opv_*` (`rpc/core/src/model/message/palw_kernel_route_reads.rs:163`) and finals' `final_path` / `statement`; `misaka-options` web UI shows no per-claim view | `opv.rs` `the_assurance_statement_never_claims_correctness`; new E2E asserts the statement | implemented (RPC/SDK; web `#/claim/<id>` view, §1b) |

## 3. §13.1 negative gate tests

All in `consensus/core/tests/rfc0015_activation_gate.rs` (test source only). Each builds a complete record and breaks one item.

| §13.1 defect | How the gate refuses | Test |
| --- | --- | --- |
| operator-only proofs / public bond qualification or API missing | G14 conjunct FAIL → `NotPass(G14…)` | `rfc0015_gate_each_section_13_1_defect_is_refused_by_name` |
| producer heap / drill instance / private endpoint needed | G14 conjunct UNKNOWN (never PASS) | same |
| canonical held/fused cap / prefix / committed tile gaps | rewarded-profile dispute/resource conjunct FAIL | same |
| garbage / borrowed trace answering DA ends prosecution | RFC14 claim-specific prosecution conjunct FAIL | same |
| Final during non-seat pursuit; direct proof refused for an open court | lifecycle/court/Final conjunct FAIL | same (+ node E2E `rfc15_e2e_a_direct_proof_lands_over_another_bonds_open_demand`) |
| max profile / cold material / resource / inclusion unverified | public-material/watcher/inclusion conjunct UNKNOWN | same |
| class/plan completion record missing; staged upgrade identity mismatch | `Coverage`; `WrongNetwork`, `WrongActivation`, `WrongTerms` | same; `rfc0015_gate_the_record_form_is_strict` |
| Block / Reward / Final-weight caps or common hold missing, undetermined or bypassed | caps conjunct UNKNOWN; `FenceNotArmedAtOrBelow("palw_bond_budget_v1")` (absent / later / never); `BondBudgetPolicy` (another `W`); fold refuses unbudgeted OPV claims under a record | same; `rfc0015_gate_every_conjunct_and_every_coupled_fence_is_required`; node `rfc15_e2e_with_a_gate_record_an_unbudgeted_opv_claim_is_refused` |
| (this binary) | `NotPinned` for even a complete record; `validate_palw_panel_free_v1` / `validate_palw_v2` refuse | `rfc0015_gate_this_binary_pins_no_record_and_refuses_even_a_complete_one`; `rfc0015_panel_free.rs` |

## 4. §13.2 node E2E cases

"existing" tests were written and recorded by earlier lanes; they were **not re-run** in this round.

| §13.2 case | Test(s) | Status |
| --- | --- | --- |
| honest claim, no Panel → Challengeable → window/DA → Final; no fabricated licence | `g14_kernel_route_e2e.rs` `g14_opv_an_honest_claim_finalizes_with_no_panel_and_exports_a_panel_independent_beacon_fact`, `g14_opv_survives_a_node_restart_and_finalizes_after_it`; new `rfc15_e2e_an_opv_claim_draws_the_bond_budget…` (no seat/quorum/receipt) | existing; new = test source only |
| false claim, producer/operator collusion → fresh verifier convicts → slash / Final blocked | `g14_opv_a_lying_claim_is_convicted_by_a_fresh_outsider_before_final`, `…_a_lie_that_finalized_is_convicted_within_liability…`, `canonical.rs` `g14_canonical_opv_a_post_genesis_bond_on_{a_pruned_import,an_ibd}_node…` | existing |
| withholding / root-opening fake trace / borrowed trace | `g14_opv_withheld_material_defaults…`, `canonical.rs` `…opv_withheld_material…`, `…opv_a_self_consistent_garbage_trace…`, `c4r3.rs` `…opv_relayed_lies…` | existing |
| valid proof while another court leads | `c4r3.rs` `g14_c4r3_gap_r7_a_lifted_proof_pays_its_earliest_sealer…`, `rfc02t_sealed_bounty.rs`; new `rfc15_e2e_a_direct_proof_lands_over_another_bonds_open_demand` | existing (Panel route) + new OPV case test source only |
| malformed proof / honest producer / faulty checker | `canonical.rs` `g14_canonical_opv_a_wrong_challenge_of_an_honest_claim…_is_dismissed_and_it_finalizes`; `k2_opv.rs` `an_invalid_challenge_is_dismissed…` | existing |
| watcher exit / return / absence | `recovery.rs` (verifier restart, node restart, IBD, pruned import); absence: `…_a_lie_that_finalized…` shows an unwatched lie reaching Final (the optimistic limit, §7.1) | existing; **absence is never reported as safe** |
| spam / all interactive slots / carrier delay | `g14_opv_spam_demands_cannot_hold_final_past_the_hard_deadline`, `c4r3.rs` `…opv_two_bonds_must_not_be_able_to_hold_the_whole_opv_lane`, `…eight_junk_chunk_groups…` | existing; adversarial carrier delay on a real network = **evidence-only** |
| concurrent claims / collateral exit / fake fraud bounty | `k2_opv.rs` `the_reservation_is_the_policys_not_a_panel_lock…`, `live_claim_caps…`, `a_collateral_another_subsystem_took…`; new node `rfc15_e2e_concurrent_opv_claims_reserve_separately_and_a_conviction_takes_only_its_own`, `rfc15_e2e_a_self_prosecuted_lie_costs_the_producer_and_its_sybil_together` | kernel reference + test source only (collateral exit: `rfc15_e2e_collateral_exit_and_re_registration_wait_for_d_plus_w_and_the_liability_horizon`) |
| same bond, different model/class/PWU/token, honest vs forged | ADR-0176 engine: `palw_bond_budget_e2e.rs` `same_bond_honest_and_forger_reach_equal_ceilings_on_the_real_fold` (V2 route); OPV kernel claims reserve `Q = 1`, `R` = escrow, `B = F = 0` whatever the class (new E2E asserts Q/B/F) | V2: existing; OPV: test source only (one class fixture) |
| early Final / void / resend → no recovery before `d + W` | `palw_bond_budget_e2e.rs` `early_final_void_and_retry_recover_nothing_before_d_plus_w`; new `rfc15_e2e_an_opv_claim_draws_the_bond_budget_and_its_slot_returns_only_at_d_plus_w` | existing (V2) + test source only (OPV) |
| Attempt / Free Prompt / multi-receipt / slice / rider / batch | `palw_bond_budget_e2e.rs` (riders, `no_payout_before_final_*`), `round_draws_reserve_each_bonds_allocation_and_replay` | existing (Panel routes); not under Panel=0 (no OPV Attempt path) |
| period boundaries / delayed payout / bond split / re-registration / key change | `palw_bond_budget_v1.rs` `split_capital_never_raises_caps`, `ten_bonds_of_a_tenth_never_beat_the_whole` (E2E), `splitting_bonds_or_claims_never_raises_candidates` | existing (engine) + test source only at node level (`rfc15_e2e_collateral_exit_and_re_registration…`: the capital cannot leave or be re-registered before the last clock); a bond's key is its identity, so a key change is a new bond |
| `d + W` reached with unresolved court/DA/retention; slash then new claim | `palw_bond_budget_v1.rs` `the_liability_hold_is_apart_from_the_issuance_window`, `open_claims_stop_at_capital_over_reservation`; kernel `liability_until`; new node `rfc15_e2e_an_opv_claim_draws_the_bond_budget…` (slot released at `d + W`, liability collateral still held) | engine-level + test source only (`rfc15_e2e_after_a_slash_the_next_claim_fits_the_remaining_capital_beside_the_old_consumption`) |
| reservation reduction / multiplier change / more concurrent claims (§8.3.7) | `rho_1_100_1000_move_q_and_never_b_r_or_f` | engine; joint evaluation = **evidence-only** + **user decision** |
| capacity ×1/×100/×1000, Final/retired/reversal, rounding | `palw_bond_budget_e2e.rs` `rho_1_100_1000…`, `palw_bond_budget_v1.rs` `rider_block_attribution_sums_to_one_block` | existing (engine) |
| restart / IBD / reorg / duplicate proof | `g14_opv_replay_and_reorg_reach_the_same_roots`, `g14_opv_the_salted_seal_rows_roll_back…`, `recovery.rs`, `k2_opv.rs` `replay_and_reorg…` | existing |
| largest admitted class / context / layout | `canonical.rs` `g14_canonical_a_routing_and_history_class_is_refused_by_the_node_carrier_fit…` (refusal, by design) | **evidence-only** (finding 2 of the 10-08 record: held / long-context classes do not fit the carriers) |
| work-slice and native settlement | shared predicate unit tests (`palw_kernel_settlement_v1.rs`); slice outcome behaviour-preserving refactor | implemented; node E2E of an OPV-verified slice = **GAP** (needs an EXEC × OPV world) |
| upgrade across old claims | new `rfc15_e2e_a_panel_licensed_claim_admitted_below_the_fence_finishes_under_panel_rules`; `g14_opv_registration_is_dropped_without_the_fence…_and_the_legacy_route_coexists`; `k2_opv.rs` `panel_licensed_classes_behave_exactly_as_before…` | existing + test source only |

## 5. Compile risks of this round (unbuilt)

* `PalwPanelFreeFenceV1` gained a field: every struct literal was updated (`palw_panel_free_v1.rs`, `palw_native_calibration_v34.rs`);
  a literal elsewhere would fail to compile (grep found none).
* `PalwKernelOpvExtrasV1` gained `budget_required`: literals updated in the processor, `consensus/core/tests/rfc0010_production_fold.rs`
  (3) and `consensus/core/src/palw_state_v2/tests/reward_gate_genesis.rs` (1).
* `palw_panel_free_gate_v1.rs` uses `blake2b_simd` (already a consensus-core dependency) and Borsh derives on `Hash64` (existing).
* The new E2E module uses the parent suite's private helpers through `use super::*` (as `canonical.rs` does) and reads the budget's
  rows by `origin == KernelRoute` (no visibility change). The budget-armed OPV world (`rfc15_config`) is new: the kernel claim's
  `require_kernel_monetary_material_scope_v1` / `hold_kernel_income_v1` under an armed budget has not been exercised on an OPV world.

## 6. What is still not done

* Every **evidence-only** row (§1 G14 on real nodes for every rewarded profile, measurements, independent reviews, PESG, the
  adversarial network run) and every **user decision** (values, the pin, the N5 lift).
* GAPs: node E2E of an EXEC slice verified by an OPV kernel claim; RFC-0010 V3 reading OPV Finals (GAP-B3).
