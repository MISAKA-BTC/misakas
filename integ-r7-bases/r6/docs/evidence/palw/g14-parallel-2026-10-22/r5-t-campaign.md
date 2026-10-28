# Lane r5-t-campaign — RFC-0026 §10 T01–T15 node-level campaign (2026-10-22)

**Status: Rust tests WRITTEN, NOT COMPILED, NOT RUN** (lane rule: no cargo). Python drill plan/judgement unit tests RUN: 16/16 pass
(`python3 -m unittest scripts/drills/tests/test_g14_t12_16000_fence_drill.py`). The drill itself was NOT run (no binaries built).
Base `64bc87dcd`, branch `g14p/r5-t-campaign`. Nothing here is a T-item completion claim (RFC-0026 §8.1): each active test covers
the part of its T item this base carries; the rest is an `#[ignore]`d test written against the intended API of a parallel lane.

## Where

- Module: `consensus/src/pipeline/virtual_processor/tests/g14_t_campaign/` (`mod.rs`, `t01_t03.rs`, `t04_t05.rs`, `t06_t07.rs`,
  `t08_t09.rs`, `t10_t11.rs`, `t12_t13.rs`, `t14.rs`, `t15.rs`), declared as a CHILD of `g14_kernel_route_e2e` (via
  `#[path = "g14_t_campaign/mod.rs"] mod g14_t_campaign;` at the end of `g14_kernel_route_e2e.rs`) so it reuses that harness's
  private `Net` / `World` / `Fresh` (real mempool -> node template -> chain-block fold, Full-UTXO carriers signed ML-DSA-87, every
  route object signed by the bond the fold names, verifiers built only from the read API). No fold-only shortcut.
- Test path prefix: `pipeline::virtual_processor::tests::g14_kernel_route_e2e::g14_t_campaign::<file>::<fn>` in crate
  `kaspa-consensus` (lib tests). Run: `cargo test -p kaspa-consensus g14_t_campaign`.
- Cargo feature added: `consensus/Cargo.toml` `g14-t-campaign-merged = []` — the ignored lane-dependent bodies compile only with it,
  so the crate builds before the lanes land. Without it, an ignored test run panics with `DEPENDS ON lane …`.
- Drill: `scripts/drills/g14_t12_16000_fence_drill.py` (+ `scripts/drills/tests/test_g14_t12_16000_fence_drill.py`).

## T item -> test -> dependencies

| T | Test (fn) | State | Depends on |
|---|---|---|---|
| T01 | `t01_t03::t01_honest_miner_and_wrong_challenger_route_outcomes_are_funded_and_exact` | active | — |
| T01 | `t01_t03::t01_honest_miner_and_wrong_challenger_through_shared_original_duties_and_funded_outcomes` | ignored | r5-original-admission, r5-escrow-settlement, r5-f03-ingress |
| T02 | `t01_t03::t02_permanent_miner_silence_is_a_correct_eligible_default_after_an_actual_opportunity` | active | — |
| T02 | `t01_t03::t02_permanent_silence_on_the_shared_original_uses_current_duty_authority` | ignored | r5-f02-wire, r5-clock-deps-relay |
| T03 | `t01_t03::t03_forged_signatures_and_fake_panel_completion_have_no_culpability_or_monetary_effect` | active | — |
| T04 | `t04_t05::t04_invalid_codec_and_paid_court_floods_are_bounded_separately_and_never_starve_a_proof` | active | — |
| T04 | `t04_t05::t04_native_intake_separates_normal_paid_court_and_invalid_codec_floods_with_calibrated_costs` | ignored | r5-f02-wire, r5-f03-ingress |
| T05 | `t04_t05::t05_every_owner_slot_refilled_by_colluders_and_exactly_one_outsider_finishes` | active (route owner = demand session) | — |
| T05 | `t04_t05::t05_all_owners_refilled_by_colluders_one_outsider_finishes_through_fair_paid_public_ingress` | ignored | r5-f03-ingress, r5-f02-wire, r5-original-admission |
| T06 | `t06_t07::t06_last_inclusive_notice_and_latest_lawful_response_fit_the_hard_bound` | active | — |
| T06 | `t06_t07::t06_every_remaining_legal_branch_fits_the_original_hard_bound` | ignored | r5-clock-deps-relay, r5-original-admission |
| T07 | `t06_t07::t07_demand_window_minus_one_equal_plus_one_at_the_node` | active | — |
| T07 | `t06_t07::t07_court_deadline_minus_one_equal_plus_one_at_the_node` | active | — |
| T07 | `t06_t07::t07_liability_horizon_minus_one_equal_plus_one_at_the_node` | active | — |
| T07 | `t06_t07::t07_every_connected_native_deadline_minus_one_equal_plus_one` | ignored | r5-clock-deps-relay, r5-escrow-settlement, r5-p2-terminal, r5-original-admission |
| T08 | `t08_t09::t08_miner_offline_third_party_relay_carries_the_current_duty_signature` | active | — |
| T08 | `t08_t09::t08_distinct_court_signer_under_current_duty_authority_and_relay` | ignored | r5-clock-deps-relay |
| T09 | `t08_t09::t09_multiple_claims_sharing_one_bond_reserve_in_aggregate_and_settle_independently` | active | — |
| T09 | `t08_t09::t09_shortest_signed_dependency_leases_and_shared_bond_aggregate_reservation` | ignored | r5-clock-deps-relay, r5-original-admission |
| T10 | `t10_t11::t10_an_internal_fold_only_lie_is_convicted_by_the_fold_arithmetic` | active | — |
| T10 | `t10_t11::t10_fixture_coverage_does_not_cover_every_paid_original` | ignored | r5-original-admission |
| T11 | `t10_t11::t11_n_concurrent_pursuits_only_the_last_false_with_full_retained_liabilities` | active, `G14_T11_N` (default 3; full 10,000) | — (full N needs escrow capacity: r5-escrow-settlement) |
| T12 | `t12_t13::t12_concentration_at_one_boundary_every_accepted_duty_funded_and_exact` | active, `G14_T12_N` (default 3) | — |
| T12 | `t12_t13::t12_hard_boundary_concentration_at_10000_daa_every_duty_funded` | ignored | r5-original-admission, r5-escrow-settlement |
| T13 | `t12_t13::t13_oldest_claim_loses_service_independent_claim_progresses_and_nothing_is_falsely_slashed` | active | — |
| T14 | `t14::t14_shared_path_survives_a_restart_with_an_open_demand` | active | — |
| T14 | `t14::t14_shared_path_reorgs_between_answered_and_defaulted_branches_exactly` | active | — |
| T14 | `t14::t14_shared_path_continues_on_a_pruned_import_node_and_an_ibd_node` | active | — |
| T14 | existing `lg14a_legacy_filer_e2e::g14r_resume::g14r_lg14a_a_pruned_import_after_the_reservation_and_binding_carries_the_pursuit_to_the_conviction` | run by name (legacy pruning regression) | — |
| T14 | `t14::t14_native_shared_original_rows_survive_reorg_restart_pruning_and_pruned_ibd` | ignored | r5-f02-wire, r5-f03-ingress, r5-escrow-settlement, r5-p2-terminal |
| T15 | `t15::t15_old_claims_keep_old_rules_across_the_fence_and_new_claims_take_the_new_rule` | active (`palw_panel_free_v1` salted-seal rule at H=60) | — |
| T15 | `t15::t15_shipping_identity_unchanged_and_the_armed_ruleset_is_a_different_identity` (`#[test]`) | active | + existing `consensus_params_id_tests::shipped_presets_have_pinned_fingerprints` |
| T15 | `t15::t15_full_activation_list_crossed_with_old_and_new_claims` | ignored | every r5 lane + central `palw_t12_arm_g14_full_activation_at_v1` (intended name) |
| T15/T14 (processes) | `scripts/drills/g14_t12_16000_fence_drill.py` rows F0–F7 | written, plan tests run | built release binaries with `kaspad/private-integration-config` |

## Intended lane APIs (names to be fixed at merge; all behind `g14-t-campaign-merged`)

`K::PaidIngressOpen`, `palw_paid_ingress_quote_v1/_owners_v1/_finishers_v1/_v1` (r5-f03-ingress); `K::SharedCourtOp`,
`misaka_palw_kernel::court::{SharedOpV1, SharedOpKindV1}`, `palw_shared_duty_v1`, `palw_native_court_intake_v1` (r5-f02-wire);
`palw_original_escrow_outcome_v1` (r5-escrow-settlement); `palw_position_terminal_v1` (r5-p2-terminal); `K::DelegateDuty`,
`palw_dependency_leases_v1`, `palw_connected_deadlines_v1`, `palw_bond_aggregate_reservation_v1` (r5-clock-deps-relay);
`palw_funded_original_v1`, `palw_funded_original_admission_preview_v1` (r5-original-admission). These are GUESSES at the lanes'
shapes; central verification must rename to what landed.

## Drill (`g14_t12_16000_fence_drill.py`)

Runs the shipped `misaka-rfc12-private-node --private-probe-profile=g14-v1 --private-probe-g14-at=H --private-probe-g14-set=full`
(the existing G14 drill factory: shipped testnet-12 rules below H, G14 full set from H, arming records naming the salted drill
genesis; H > 28 = the factory's last flag day), plus optional `--extra-node-flag=--palw-drill-*-at=H` (kaspad's own flag-day movers,
e.g. `--palw-drill-int13-at`). Reuses `g14_collusion_drill.py`'s node plumbing (keyring, producer/seat flags, wRPC). Phases: F0
identity (profile + binary sha256 identical start/end, genesis = keyring), F1 pre-H claims, F2 partition with restart of both
producers below H, F3 reorg across H, F4 restart past H, F5 post-H claims, F6 old claims terminal and never voided by a post-H rule,
F7 fresh IBD agreement. Missing observation -> BLOCKED, never PASS. `--plan-only` prints the plan without starting anything.

## Fences needed (for the central DAA-16,000 arming)

This lane adds no fence. Its active tests arm (harness-only, validation bypassed as the parent harness already does)
`palw_probabilistic_constraints_v1` and, in T15, `palw_panel_free_v1` at H=60. The ignored tests need every r5 lane's fence plus the
central full-activation list.

## Uncertain / risks (honest)

- Not compiled: type/field mismatches are likely in a few places (e.g. `max_adjudications_per_block` integer type, whether a
  forged-signature carrier is refused at the pool or dropped at the fold — `offer_forged` tolerates both).
- Assumed semantics to confirm at run: T01 wrong-demand bond is burned (not refunded) at the unconvicted horizon (kernel comment
  "G14-R4"); T07 court deadline: a response folding at `deadline` loses to the deadline tick (`daa >= deadline_daa` defaults first);
  T15 a plain (non-OPV) class under an armed `palw_panel_free_v1` still follows the salted-seal rule (it is ledger-wide via
  `salted_seals_from`); T14b the B branch with `a_len + 4` heartbeats out-works A.
- T05 active variant interprets "owner" as the route's per-position demand session; the shared-original 8-owner semantics is the
  ignored test.
- T11 full N = 10,000 is not feasible on this harness (one poster escrow per job on card 1); the env var exists for the central run
  once escrow capacity lands.
- The drill's partition relies on `addPeer isPermanent` being process-local and `--outpeers=0`; if peers reconnect, F3 FAILs
  (observed, not assumed).
