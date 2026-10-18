# Consolidated validation, 2026-10-15/16

Branch `integ/validation` (own worktree and `./target`), toolchain 1.93.0, `-j 8`, one cargo invocation at a time,
`CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_INCREMENTAL=0`. Logs are under
`target/consolidated-validation/` (local, not committed). Integration heads folded in: `7d0eaa6b2`, `f244ed92d`, `12fb03fd4`,
`e72c1870d`, `5ccc3a234` (last merge `1fc876ebd`; head `dcd255e93`). Only green runs are labelled PASS.

## Step 0: generated sources are reproducible

`python3 scripts/drills/gen_rfc06_native_wire.py`, rustfmt (repo config, edition 2024) on the eight generated files,
`git diff --exit-code`.

| Source | Result |
| --- | --- |
| `3787e600e`, `08ac332ab`, `3730427fe`, `8d568dca4` | PASS |
| `31186b2f8` (e72c1870d merge) | FAIL: aux-143 rows hand-added to generated files; fixed in the generator inputs (`417f3dcff`) |
| `417f3dcff`, `1fc876ebd` | PASS |

## Step 1: check ladder (20 rungs, `--locked`)

Rungs: tir, kernel, challenge, consensus-core, consensus, mining, rpc×7, p2p-flows, consensusmanager, tir-exec `node`,
remote `rpc`, sdk, tir-artifact, tir-lower `remote`, base0/gateway/pq-validator, wallet-core, wallet-core wasm32,
misaka-cli, kaspad (lib/bins/tests), testing-integration.

| Run | Source | Result |
| --- | --- | --- |
| L7 (`L7-r*.log`) | `3730427fe` | PASS 20/20 |
| L9 (`L9-r*.log`) | `417f3dcff` (rungs 1–7), `f2b8b05b8` (8–20) | PASS 20/20 |

## Step 2/3: tests (nextest, `--no-fail-fast`)

| Run (log) | Source | Run | Pass | Fail | Notes |
| --- | --- | --- | --- | --- | --- |
| focused tir/kernel/challenge/panel/tir-artifact/tir-lower/base0 (`v7-focused`) | `3730427fe` | 2481 | 2453 | 28 | fixed 18 (`b8d8ab1a3`), 7 pre-existing on base `64699e960`, 3 lane-open (below) |
| consensus-core full (`v7-core`) | `b8d8ab1a3` | 6059 | 6038 | 21 | fixes `664efaa6d`, `e95166f12`, `b57628bdd`; G14 conviction tests fixed by the lane at 12fb03fd4 |
| kaspa-consensus full (`v7-consensus`) | `664efaa6d` | 1050 | 987 | 63 | fixes `2f63cd963`, `e95166f12`; open below |
| kaspad, mining, p2p-flows, sdk, remote (`v7-affected`) | `e95166f12` | 1685 | 1658 | 27 | 4 = debug-runtime tests stopped (SIGTERM); fixes `b57628bdd`, `8275f2188` |
| testing-integration rpc_tests (`v9-misc`) | `f2b8b05b8` | 1 | 1 | 0 | PASS |
| G14 lanes (`v9-g14`) | `417f3dcff` | 141 | 138 | 3 | 3 = conformance beacon (below) |
| requested set (`v10-req`) | `1fc876ebd` | 150 | 150 | 0 | PASS: free_prompt 5, g14_close_legacy 8, g14_close_gates 12, lg14a_legacy_dispute_fold 9, tir_fp_lane 6, palw_tir_public 69, palw_legacy_public_filer_v1 12, palw_memory_routing_v2 9, lane_closure 5, lg14a_legacy_filer_e2e 14 |
| `python3 -m pytest scripts/drills/tests` | `3730427fe` | 129 | 129 | 0 | PASS |
| python `--self-test`s (10 scripts) | `3730427fe` | 10 | 9 | 1 | `t12_repin.py` needs harvested logs (environment) |
| node `--test` web/contrib | `3730427fe` | 11 | 11 | 0 | PASS |

## t12 identity (all new fences dormant)

`palw_t12_flag_day_9000` (int-12 params id `5ee7fd8e…`, identity, schedule, fork id), `the_int12_kind_list_is_frozen`,
`every_int12_wire_type_is_unchanged_or_classified`: PASS (`pins1`, `08ac332ab`); `g14_close_gates::testnet12_identity_did_not_move`:
PASS at `8d568dca4` and `1fc876ebd`. The int-12 variant digest `0x01d641040ea4a142` is unchanged. No drift.
A-2 new-kind pins re-taken from the Rust test (`664efaa6d`^): 240 `0x91362a7791c31616`, 241 `0x5a27c8d5601adf8e`,
242 `0x3afba613f2ab70a6`; 170 `0xb68d62123647ff28` and 171 `0x68623b7dc7d30b55` equal the Python port.

## Still failing (not fixed here)

- **Kernel-route beacon cannot lock (≈45 consensus tests, `g14_kernel_route_e2e::{conformance,opv_bootstrap}`, core
  `rfc0010_production_fold::paid_panel_sources…`).** Since `0a11986ed` no kernel Final exports a beacon event
  (`palw_kernel_route_v1.rs` ~808: every path `None` "unless an authenticated selected-approval adapter supplies an event").
  The fixtures need that adapter (RFC07 remaining work); no fixture change can lock the beacon.
- RFC-0018 `rfc0018_dag_siblings` ×3 (unbuilt lane source: the armed fence is refused by the binary; card 2's claim unrecorded).
- `rfc9_v4_chain_e2e` offline miner: hash-dependent count of winning quanta (1, test needs 2).
- `rfc0016_e2e::t12_rfc0016_zero_rights_heartbeat_keeps_the_cadence` (floor-class harness, exempt by design).
- Pre-existing on base `64699e960`: rfc12 ×4, `t12_a2u_mixed_verdicts` ×2, SDK ×3 (qwen25 pilot bytes, recorded 32-position,
  corpus preflight), tir-lower ×4 (three_way, corpus_v2, admission, quant_decode_pins), base0 float_free ×2 and prefix cache.
- kaspad service E2E cluster (16: pruned-import mismatch, view None, honest job flagged, socket/wall deadline; SIGABRTs are a
  DB-lifetime panic during cleanup after those).
- Lane-open: base0 `gdn_evidence_v2_hybrid` forged checkpoint (now reaches `PromptIdsAreNotTheJobs`), kernel
  `native_invocation32_stream` (program violates TIR NF-15/NF-19).
- Debug-runtime, stopped: SDK `k2s_v5_the_worst_geometry…`, `adv_c4_conformance` byte flips, `gen_sd3_class`, one preflight.

## Clippy: NOT green

`cargo clippy --workspace --all-targets --features evm,private-integration-config,legacy-secp256k1 --keep-going`
(`v9-clippy-list`, `8275f2188`): `misaka-palw-worker` build script needs `MISAKA_LLAMA_SRC` (not a default member;
must be excluded); `simpa` did not compile (fixed `dcd255e93`); 557 warnings (222 consensus-core, 74 kernel, 56 SDK,
60 kaspad TIR acquisition/scheduler, …): collapsible ifs 67, needless borrows 35, never-used items ≈80, unused imports 37,
unnecessary parentheses 33, duplicated attributes 24, unused mut 23, items after test module 17, … Not fixed.

## Fix commits (this phase)

`305373db9`, `78cb8cfb2`, `b03945c66`, `08ac332ab`, A-2 re-pin, `b8d8ab1a3`, `664efaa6d`, `2f63cd963`, `e95166f12`, `b57628bdd`,
`417f3dcff`, `f2b8b05b8`, `8275f2188`, `dcd255e93`.

## 2026-10-17: validation round 2 (`integ/validation-r2`, after the RFC07 adapter)

Branch `integ/validation-r2` (own worktree and `./target`), base `integ/rfc02-rfc06-12-g14` `a5638398b` merged (`3eec46526`), toolchain
1.93.0, `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_{DEV,TEST}_DEBUG=line-tables-only`, `--build-jobs 3`, targeted nextest filters only (no
full suite, no clippy). Host load average 20–75 during the kaspad runs (four lanes share the host). Logs `target/vr2/` (local).

### 1. RFC07 fixture migration: NOT possible on the real path — the beacon cannot start (real bug, needs a decision)

The task was to move the ~45 beacon-dependent fixtures from OPV-only sources to paid claims carrying the selected-checker approval,
without bypassing the adapter. Reading the real path shows that no beacon event can ever be created from a state that holds none:

1. **Events exist only for approved paid claims.** `PalwKernelRouteStateV1::finals_read_v1` (via its export decision in
   `palw_kernel_route_v1.rs`) exports `Some(event)` only for a claim in `paid_claim_consumers_v2` that passes
   `palw_exec_v2_selected_approval_v1` (`palw_exec_v2_verify.rs:322`), i.e. that has a `PAID_CLAIM_FINALS_V2` row with
   `complete_receipts > 0` and `completed_rounds > 0`. `OptimisticUnapproved` and `PanelLicensed` Finals export `None`.
2. **That row needs a lock, the lock needs a locked paid beacon.** `tick_paid_claim_final_projection_v2` refuses a paid Final without
   `PAID_CLAIM_LOCKS_V2` ("paid Final has no locked seed"); `lock()` (`palw_claim_challenge_fold_v2.rs:552`, and `round_lock` for staged
   plans) requires `collect_paid_opt_in_work_beacon_v5(...) == Locked`, which needs `work_count_k ≥ 1` sources whose fate is
   `SourceFateV3::Final(event)` with `validity_independent` (`misaka/palw/challenge/src/sealed.rs`, `final_qualifies_v3`, "k ≥ 1").
3. **The source pool excludes exactly the claims that can carry an event.** `beacon_opt_in_sources_v4`
   (`palw_beacon_source_opt_in_v1.rs:212`) drops every Final whose claim is in `paid_claim_consumers_v2` before reading its event — the
   very set (1) exports. Every other Final exports `None`, so every revealed source is `Live`/`Failed`, never `Final`.

Hence: no paid claim can lock until some source is `Final`; no source can be `Final` while only paid claims export events and the pool
filters paid claims out. The conformance beacon (`collect_attributed_work_beacon_v1` over all events), sealed v3, opt-in v4, paid v5,
Panel-source and paid-claim lock all stay `Unavailable`/`Vetoed`. Observed on the node: `Unavailable { have: 0, need: 2 }`;
`Vetoed { withheld: 0, failed: 2 }` with both source claims `Final { final_daa: 143 }` and `event = false` (the previous agent's
diagnostic in `run_paid_v5_source_budget`, kept). Even without the filter in (3), the first lock still needs an event that predates it.

**Consequence for G14.** The 12 EXEC node tests (`x8_g14_*`, among them "an outsider convicts a slice's kernel claim … on the real
node" and "a lying real run is convicted through its prefix claim") and the 4 native-Final tests cannot run at all: their class becomes
active only after a sampled conformance passes, and that needs a locked beacon. They are blocked, not failing on their own logic.

**Needs a Lead/user decision (not changed here; consensus, dormant fences):** what randomness seeds the FIRST selected-checker
assignment when no approved Final exists — e.g. a genesis/annual-fork fixed seed or a block-hash/VRF beacon used as randomness only
(allowed by PRINCIPLES §8), under the explicit honest-fraction premise — and whether an approved `PaidClaimV2` Final may be a beacon
source (the filter in (3)). No fixture was changed to bypass the adapter or weaken its gate.

Blocked tests (36, all `g14_kernel_route_e2e::`; failure "the beacon must not run out of window here" unless noted):

| module | n | tests |
| --- | --- | --- |
| `conformance` | 9 | `g14_canonical_ops_231_and_212_served_by_an_ibd_node_rebuild_the_conformance_verdict`, `g14_conformance_a_forged_outcome_list_fails_the_attempt`, `g14_conformance_evidence_against_another_beacon_policy_scope_or_commitment_is_refused_and_stale_evidence_is_invalid`, `g14_conformance_evidence_passes_only_after_an_unrefuted_window_and_the_class_activates`, `g14_conformance_forged_evidence_is_refuted_withheld_evidence_defaults_and_attempts_are_exhausted`, `g14_conformance_hostile_evidence_is_dismissed_or_failed_spends_budget_and_never_stops_the_chain`, `g14_conformance_rows_survive_reorg_restart_and_pruned_import`, `g14_rewards_a_v2_claim_of_a_kernel_bound_class_never_earns_the_new_reward`, `g14_rewards_on_the_release_terms_a_sampled_conformance_earns_nothing` |
| `conformance::c4r4` | 2 | `g14_c4r4_free_junk_refutations_must_not_carry_forged_evidence_through_its_window`, `g14_c4r4_junk_conformance_refutations_must_not_spend_the_runs_reserved_for_proofs` |
| `conformance::exec_slices` | 12 | `x8_g14_a_competing_claim_cannot_spend_a_bound_jobs_escrow_before_native_root_final`, `x8_g14_a_lying_real_run_is_convicted_through_its_prefix_claim_and_voids_the_whole_session_on_the_real_node`, `x8_g14_a_second_real_root_cannot_reuse_the_first_roots_prefix_claim`, `x8_g14_a_signed_legacy_job_cannot_attach_without_worker_retention_terms_on_the_real_node`, `x8_g14_a_signed_tiny_run_is_refused_as_a_larger_work_cell_on_the_real_node`, `x8_g14_a_withheld_slice_is_demanded_by_an_outsider_and_defaults_its_suffix_and_root_on_the_real_node`, `x8_g14_an_outsider_convicts_a_slices_kernel_claim_and_its_suffix_and_root_void_on_the_real_node`, `x8_g14_completed_worker_claims_earn_nothing_when_the_root_expires_on_the_real_node`, `x8_g14_honest_slices_verify_through_kernel_finals_and_the_root_is_ready_on_the_real_node`, `x8_g14_kernel_claims_accept_a_canonical_prefix_and_its_continuations`, `x8_g14_kernel_finals_holds_and_credit_survive_restart_and_two_actual_reorgs` (stack overflow at the default 2 MiB stack; with `RUST_MIN_STACK=64MiB` it reaches the beacon wait), `x8_g14_root_admission_uses_snapshot_origin_when_original_header_is_absent` |
| `conformance::exec_slices::native_final` | 4 | `x8_g14_native_final_funds_recover_after_actual_reorg_and_public_proof_reapplies_once`, `x8_g14_native_happy_final_pays_all_positive_funds_once_and_keeps_each_queue_r_through_actual_payment_plus_w`, `x8_g14_native_root_final_funds_each_leg_once_and_public_conviction_cancels_all_unpaid_funds`, `x8_g14_native_paid_material_rejects_one_daa_beyond_signed_exec_and_accepts_exact_provider_terms` (fails earlier: "the claim committed" — a source claim is refused under this test's own budget policy; not investigated further because the beacon blocks it anyway) |
| `conformance::exec_slices::opv_settlement` | 3 | `x8_g14_opv_settlement_never_substitutes_for_selected_exec_approval`, `x8_g14_rfc15_a_conviction_revokes_the_slice_through_the_settlement_predicate`, `x8_g14_rfc15_a_default_revokes_the_slice_through_the_settlement_predicate` |
| `opv_bootstrap` | 6 | `g14_opv_bootstrap_a_sealed_source_v3_beacon_locks_on_salted_seals_and_the_class_passes` ("the v3 beacon locked"), `g14_opv_bootstrap_eligibility_is_lost_when_the_artifact_binding_is_refuted` and `…from_zero_finals_a_complete_check_seeds_the_beacon…` (`Unavailable { have: 0, need: 2 }`), `…paid_v5_requires_prepaid_signed_fees…` and `…paid_v5_pre_fence_promise_migrates…` ("paid timely sources must lock: Vetoed { failed: 2 }"), `…v4_ordinary_withholding_is_excluded_and_rooted_opt_ins_lock_replay_restart` ("Vetoed { failed: 2 }") |

Fixed in this group (2): `opv_bootstrap::g14_conformance_candidate_grants_no_rights_then_promotes_only_after_a_real_complete_check`
FAIL → PASS — stale: it asserted one attributed event for an OPV-timeout Final; the adapter exports none by design; the test now
asserts 0 events and the `OptimisticUnapproved` Final, the reward assertions unchanged (`7e4a970c3`).
`opv_bootstrap::g14_opv_bootstrap_source_fee_prepaid_final_horizon_db_replay_and_two_reorgs_pay_once` SIGABRT → PASS with
`RUST_MIN_STACK=67108864` (environment: debug-build stack). Passing before and after (16): the other `opv_bootstrap` /
`conformance` tests that do not need a locked beacon.

Also blocked, same cause: core `rfc0010_production_fold::paid_panel_sources_hold_the_same_native_r_until_actual_final_fee_queue_and_survive_replay`
("actual source Finals must lock the paid Panel beacon: Vetoed { withheld: 0, failed: 2 }"); the other 25 tests of that file PASS.
Total blocked by the bootstrap deadlock: **37**.

### 2. The red list of remaining-work §4, classified

Status after this round (FIX = fixed here; the commit is on `integ/validation-r2`).

| item (red at `37cf3fa89`) | result now | class | what |
| --- | --- | --- | --- |
| kaspad `kernel::g14k_the_kernel_service_convicts_covered_lies_pre_and_post_final_and_the_honest_twin_finalizes` | FAIL → **PASS** (FIX `35386169d`) | stale test | Card 0 lost exactly one G-K1 signer lock (183,333,333 sompi) beyond its reservations: producers are seats on the other producers' claims; the producer assertion predated G-K1 (5bdd427fb). Now `before − n·claim_collateral − covered·lock`. All six lies convicted, honest twin Final. |
| kaspad `recovery::automatic_producer_multipart_and_fresh_verifier_resume_after_partial_transport` | FAIL → PASS on rerun | environment | Passed at load ≈ 8; failed at load ≈ 75 ("must actually restart the service while its case is live"). |
| kaspad `fresh_public_bond_service_registered_prefix_{initial,inherited}_after_pruned_import`, `recovery::pruned::fresh_pruning_carriage_verifier_{acquires_automatic_multipart_and_releases_after_independent_replay,defaults_when_the_automatic_producer_withholds_material}` (4) | FAIL (later stage) | stale fixture, partly fixed; then open | `ImportedPalwPruningPointMismatch(point, genesis)`: since 761dcb1f3 every sidecar import needs its target established first; the kaspad fixture never did, so its forged-carriage refusal also proved nothing. Fixed (`7e4a970c3`, then the node's own pruning-sample history, `TestConsensus::pruning_sample_from_pov_for_tests`): the import now installs ("authenticated native carriage restored … at DAA 1200"). **Open:** the fresh verifier then opens no session in 1,600 DAA (`deadlines []`) and the wall deadline expires. |
| kaspad `fresh_public_bond_service_reacquires_pruned_fp_input_replays_and_releases_an_honest_job`, `…reacquires_an_unretained_job_and_input_then_replays_and_releases`, `…reconstructs_a_multipart_unretained_job_then_replays_and_releases`, `…replays_an_admitted_large_constraint_job_from_public_multipart_history` (4) | FAIL | **open, G14-relevant** | The honest claim ends `Voided { reason: ProducerWithholding }`, `da_defaulted = true` (assertion now prints the flags). On 10-10 (`docs/audit/g14-review-2026-10-10/evidence/pruned-fp-input-honest-repair.log`) the filer replayed after input chunk 0 and released ("honest, nothing filed"). Now it never replays the honest claim: after the binding answer it queues a G14-L round of 17 lanes ("k-ary descent") on it; the harness producer answers only the input chunks, so the claim defaults. Either the filer must replay before descending once the input is acquirable (the old property), or the honest producer in the harness must run the custodians' demand responder (fa7da8157). Owner decision; nothing slashed by the court, but an honest-but-silent producer is defaulted. |
| kaspad `recovery::fresh_verifier_reacquires_one_pruned_parallel_input_lane_from_the_automatic_producer` | FAIL | open (same cause family) | "only the existing sixteen-lane round is drained": the round now has 17 lanes. |
| kaspad `fresh_public_bond_service_native_public_prefix_{initial,inherited}_convicts_all_custodians`, `staged_tokenizer::registered_a16_{false_tokenizer_length,understated_tokenizer_key_work}_restarts_and_collects_full_panel_through_rooted_court19` (4) | FAIL | open | `palw_legacy_dispute_v1(claim)` is `None` right after the block that carries the FP job (`palw_service_public_e2e.rs:1242`): the job of the newly registered tiled class is not admitted; the class is `Probation { probes_passed: 0 }` at that point. |
| kaspad `recovery::network::{automatic_public_job_multipart_is_authenticated_after_socket_p2p_block_relay, sole_socket_receiver_verifier_acquires_automatic_multipart_and_independently_releases, sole_socket_receiver_verifier_carries_requests_through_real_tx_relay_to_objective_default}` (3) | FAIL/SIGABRT | environment suspected, unconfirmed | "socket relay never validated the complete public suffix", "wall deadline expired while waiting for the native clock lead cap"; both runs were under load. The SIGABRT is the DB-lifetime panic during cleanup after the first panic. |
| RFC-0018 `rfc0018_dag_siblings` ×3 | 2 deterministic causes FIXED; intermittent | stale test + **real design conflict** | (a) The sibling-fence fixture is refused by `validate_palw_v2` — built directly, as other dormant-fence harnesses do (previous agent's WIP, kept). (b) testnet-12's second model row is the 2M row: its attempt is refused `ClassDeadlineUnmeasured` (13,995 > 600 DAA, ADR-0152 §4-quater V2), so card 2's claim was never recorded — now opened with the R-core suite's measured row (`t12_2m_flag_day_row`). (c) Remaining, ~6 of 8 runs: one sibling is not in the merger's past although `StatusUTXOValid` and a tip. The sink search refuses it as sink ("reason DominanceViolation … palw_reorg_strict_economic_win", same blue work) and that drops it from the virtual-parent candidates, so the next block does not merge it (a later one does). RFC-0018 T-1 expects every sibling merged by the merger. Not changed (fork choice on a live network); decision needed whether a sink-refused valid tip must stay a merge candidate. |
| `rfc9_v4_chain_e2e_the_offline_miner_is_paid_by_another_builders_block` | **PASS** | hash-dependent | Two other winning quanta on this base. |
| `rfc0016_e2e::t12_rfc0016_zero_rights_heartbeat_keeps_the_cadence` | FAIL → **PASS** (FIX `7e4a970c3`) | stale test | `rfc0016_rights_admit_v1` exempts the floor class by rule; the unbuilt source asserted `held == 0` for card 0's floor attempts (41 held). Now asserts the cadence and the exemption (armed == unarmed claims). The refusal of a NON-floor attempt is not covered at node level (harness has no other class) — open. |
| rfc12 ×4 | not run | environment | `rfc12_c_items` (4) and `rfc12_zero_dns_matrix` are `#[cfg(feature = "evm")]`; `rfc12_utxo_import_target_and_completion_are_generation_bound` fails "built without the `evm` feature". An `--features evm` consensus build was not started (disk shared, 35 GB free). The other 6 rfc12 tests PASS. |
| `t12_a2u_mixed_verdicts_every_new_kind_below_its_fence_{launch,release}_ruleset` ×2 | FAIL (later stage) | stale harness, partly fixed; then open | Fixed (`7e4a970c3`, `35386169d`): six landed A-2 rows the harness did not arm/list (`palw_material_release_recovery_v2`, `palw_memory_routing_court_v2`, `palw_tir_public_conviction_v1`, `palw_native_mining_v39`, `palw_settlement_once_v1`, `palw_paid_claim_trap_v1`, `palw_class_registration_origin_v1`), and hand-built samples for kinds 170/171 (no near-zero body decodes). **Open:** the template refuses the batch of zero-filled tags 160–167: `PalwH1CarrierRefused("objective offence is not armed at this chain point (ADR-0144 §9)")` ×4 and "new staged seed requires its literal live reservation" ×1 — the node-local H-1 gate (`palw_mempool_h1_carrier_refusal`, also asked at the template) judges objective-evidence objects of kinds that are below their own fence. Not a block rule (no split), but the harness can only carry through the template. Decide whether the H-1 gate skips below-fence new kinds (as the fold rides them) or the harness needs a non-template block builder. |
| SDK ×3 | lib 1 FAIL; `corpus_preflight` 1 FAIL | pre-existing stale pins | `preflight::kernel::tests::real_qwen25_pilot_tir_fits_unchanged_node_proof_reservation_but_remains_inactive` (776,366 vs pinned 629,606); `corpus_preflight::every_corpus_entry_has_its_pinned_preflight` (idefics3 verdict gained `adapter_file: true`; "UPDATE_PINS=1 after review"). Re-pinning is a review act, not done. The other SDK lib tests PASS (822/824 across SDK, tir-lower, base0 libs). |
| tir-lower ×4 | 1 FIXED, 3 FAIL | stale test / pre-existing pins | `corpus_v2::no_court_or_ir_source_names_a_model` FAIL → **PASS**: since the 10-12 reorganisation `crate_dir()/..` is `misaka/palw/tir`, so the tripwire scanned 0 files; now the workspace root, 140 files scanned, no offender. Open: `admission::every_lowerable_real_configuration_is_admitted` (`glm-5.json` newly in the unlowerable set), `quant_decode_pins` (`mlx_b3_g64_bf16: no pins`), `three_way::every_prequantised_fixture_is_the_same_on_all_three_implementations` (fixture count 36 vs 33). |
| base0 ×3 | 3 FAIL | pre-existing | `float_free::every_source_file_is_classified` (six sources in no ADR-0040 Decision A list), `float_free::the_execution_path_contains_no_float` (`src/tokenizer.rs` raw string unsupported by the scanner), lib `prefix_cache_serving::public_prefix_court_reconstructs_the_actual_a16_cache_and_refutes_a_false_named_root` ("the non-held dense fixture retains public step tiles"). Classification needs the owner. |

Counts (tests): **now PASS after a fix 4** (g14k, rfc0016 cadence, corpus_v2 tripwire, OPV-candidate event); **PASS without a code
change 3** (rfc9_v4, partial-transport recovery, source-fee horizon with a 64 MiB stack); **blocked by the bootstrap deadlock 37**;
**still failing with a diagnosis 29** (kaspad 16 — of which the 4 pruned-import tests are past their first, fixed cause; RFC-0018 3,
intermittent after two fixed causes; a2u 2, past their fixed causes; SDK 2; tir-lower 3; base0 3); **not run 5** (rfc12 under the
`evm` feature). No test was deleted or skipped; no gate was weakened; consensus code is unchanged (test sources, one `TestConsensus`
helper and a test-visibility change only).

Fix commits (this round): `7e4a970c3`, `35386169d`, and the corpus_v2 / pruned-history / doc commit after them.

### 3. Coordinator follow-up items (same day)

| item | result | what |
| --- | --- | --- |
| kaspad honest reacquired job ×4 | **PASS** (4/4) | Production already answers G14-L lanes: the producer's DA service builds every tag-158 legacy held answer from its retained capture (`palw_da_legacy_answer_v2` → `palw_legacy_held_capture_answer_v2`). The harness producer answered only the input chunks; it now answers every demanded StepNode lane through that same production builder, signed by the producer and carried through ordinary H1/PQ. The filer's descent is unchanged. |
| kaspad pruned import ×4 | **PASS** (4/4) | Two fixture causes: (1) no import target was established (761dcb1f3) — the fixture now names the node's own pruning-sample history; (2) `import_pruning_point_palw_state` clears the pruning-UTXO stable marker (production completes it after IBD's UTXO step), so the node stayed in the transitional IBD state and the panel service loop idled (`is_consensus_in_transitional_ibd_state`). The node already holds the canonical UTXO set (it validated the prefix), so the fixture marks the import complete (`TestConsensus::mark_pruning_utxoset_stable_for_tests`). |
| kaspad view None ×4 (native prefix ×2, staged tokenizer ×2) | FAIL, cause found | NOT a probation rule: a registry `Probation` class admits claims (`admits_claims`), and past `palw_model_tier_v1` RFC-0017's review is unreachable (ADR-0179); there is no probation-based claim refusal under the tier rules. These four fixtures arm `palw_staged_public_court_v1`, which makes the FP acceptance walk require the original-public HFP1 envelope (custody pledge + original binding, `PalwHistoryFpEnvelopeV1`; `prepare_required_bundle_v1` refuses a required-family claim without it). The fixture carries a plain FP payload. Fix (not done): build the envelope as `build_fp_commitment_tx_with_original_public_v1` does. The harness now names an unadmitted job explicitly. |
| kaspad parallel lane 16→17 | test updated | The 17 accepted demands are the 16-lane StepNode round plus one resend of lane (9, 6) (the filer resends a lane whose landing it did not see within `PALW_FRAUD_FILER_RESEND_DAA_V1`). The test now counts DISTINCT lanes (16, one level, contiguous). Not rerun after the change. |
| t12_a2u_mixed ×2 | **PASS** (2/2) | Block validity never judges a below-fence new kind; the node-local H-1 template policy may hold back the carriers of zero-filled tags 160–167 (it rehearses the objective evidence they assemble). The harness keeps that template behaviour, asserts that only a new kind is held back by it, returns the held carrier's coin, and carries the rest. Also fixed: the PAV3 section shadowed the PFS4 refusal, so the replay compared the PFS4 block with the PAV3 message (both headers are now compared with their own refusals). |
| rfc12 ×4 (`evm`) | not run | Disk 30–35 GB with four lanes building; an `--features evm` consensus build was not started. |

### 4. User decision 10-17: the bootstrap seed for the first checker draws (implemented, dormant)

Implemented (`integ/validation-r2`, t12 identity unchanged — the paid-claim fence `palw_claim_challenge_v2` is `None` on every preset):

1. `PalwClaimChallengePolicyV2::bootstrap_seed: Option<Digest>` (`None` in the probe; an all-zero seed is refused). While no approved
   paid Final exists (`PalwKernelRouteStateV1::paid_claim_approved_final_exists_v2`), a non-interactive paid lock draws its checkers
   from `paid_claim_bootstrap_seed_v2(seed, context, statement)` instead of a locked v5 beacon, and writes the fixed bootstrap anchor
   (`paid_claim_bootstrap_anchor_v2`) into the lock row; every reader (`paid_claim_check_authority_v2`, receipts, the SDK's source
   reader) re-derives the authority from that row (`PaidClaimCheckAuthorityV2::verify_bootstrap_v2`). From the first approved paid
   Final on, a lock needs the beacon exactly as before.
2. `beacon_opt_in_sources_v4` no longer excludes paid claims: a source's fate is `Final` exactly when the adapter exports its event.
3. Reduced reward, smallest change: a bootstrap-locked claim earns **no EXEC credit** (`palw_exec_v2_credit_approval_v1` = selected
   approval minus bootstrap locks, used at the three EXEC sites); its event is still exported. The kernel claim's Final payment is the
   payer's escrow (GAP-5), not an issued reward, and is unchanged.

Unit tests (core, PASS 3/3): the window closes only at an APPROVED paid Final; a bootstrap-locked approved claim exports its event
but earns no EXEC credit, a beacon-locked one earns it; the seed is bound to the network seed, the context and the statement.
Neighbouring suites: 166/167 PASS; the one failure (`exec_v2_fold_v1::amendment_1::a_settled_prefix_or_middle_slice_verdict_…`,
"the consumer adds no collateral punishment") reads a predicate this change leaves identical for every non-bootstrap claim — not
caused by it, not investigated.

**Migration of the 37 tests: not achieved — three findings need decisions.**

* **(F1) Forward regress after the window.** A v5 beacon's sources must be sealed AFTER the target's commitment and be `Final` with an
  event, i.e. approved paid claims, each of whose own lock needs a later beacon. With no fixed-seed draw after the first approved
  Final, the latest-committed paid claim can never lock, hence none after it. The EXEC node tests (`x8_g14_*`) and native-Final tests
  need EXEC credit on kernel claims committed after the conformance beacon locked — i.e. after the window — so they cannot pass under
  the decision as stated, independent of fixtures.
* **(F2) Paid jobs cannot be opt-in sources.** The route refuses `OptInBeaconSource` / `OptInPaidBeaconSource` for a paid-enrolled job
  ("paid checker-enrolled jobs cannot be Panel-independent beacon sources") and a paid commit refuses a job with a live source opt-in.
  Dropping the exclusion in (2) therefore does not let an approved paid claim seed sealed v3 / opt-in v4 / paid v5 beacons; only the
  conformance attempt's DISTINCT v1 beacon (events, no opt-in) can use them. The 6 `opv_bootstrap` tests and core
  `rfc0010_production_fold::paid_panel_sources…` use opt-in sources.
* **(F3) The paid commit's real path is the full RFC08 material pipeline.** Driving it step by step (`conformance/paid_sources.rs`,
  behind the `PALW_PAID_SOURCES` probe): paid class (OK), payer's job enrollment (OK), the producer's signed monetary covenant
  (needs `palw_provider_court_v1`; OK), seal + paid commit: refused in turn by (a) the commitment DAA (fixed), (b)
  `COURT_SCOPE_INCOMPLETE` — past `palw_provider_court_v1` the kernel's clear budget refuses every K2-TIR-v1/v2 claim unless
  `palw_court_commitment_scope_v1` is in force (armed; OK), (c) "kernel material complete codec capacity is unenrolled" — the covenant
  must carry a material manifest whose scope is the claim body's commitment and the verified `PalwKernelMaterialBundleV1` of every
  committed position. Not built yet. Lock (bootstrap), receipts and Final are written but not yet reached.

For the 29 `Cw` tests (conformance, c4r4, exec_slices) the remaining work is (F3c) plus the paid-source switch; for the EXEC and
native-Final assertions (F1) must be decided first; for the 7 opt-in tests (F2).

### 5. User decision 10-17 on F1/F2 (implemented, dormant; type-checked, not yet run)

* **F1 — lock randomness from later seals.** With `bootstrap_seed` set (the same dormant policy), a paid lock is seeded by
  `misaka_palw_challenge::paid_opt_in::collect_paid_seal_reveal_seed_v6`: the commitments of the opted-in, prepaid sources sealed
  after the target's commitment, mixed once every one revealed inside the reveal window; sources need NOT be Final. Lock anchor
  `paid_claim_seal_anchor_v2`; readers re-derive from the row. The fixed seed remains only for a lock with no seal-reveal seed while no
  approved paid Final exists. An approved Final is still required on the credit side (selected approval reads the canonical paid
  Final row). No-grinding argument (in the function's doc): the target commits before every source seal; a seal fixes content and
  salt, opened only after the seal window; the source set closes at the seal window and excludes the target's producer, its base job
  and mutual pairs; the only remaining lever, a withheld reveal, vetoes the attempt (deposit forfeited) and never selects an outcome.
* **F2.** Under the same policy a paid-enrolled job may opt in as a source, and a live source opt-in no longer blocks a paid commit
  (fold); the SDK pre-checks defer to the fold. With `bootstrap_seed` `None` both refusals hold byte for byte.
* **F3** (RFC08 material manifest on the covenant) and the migration of the 37 tests: not done yet.

### 6. F3 and the paid-source migration (2026-10-17, continued)

* Unit tests: seal-reveal seed (challenge `paid_opt_in_beacon` 6/6, incl. veto on withheld/forfeited and the k floor), core paid /
  source / route suites 41/41.
* F3 built in the harness on the real path: the covenant carries a `PalwMaterialManifestV1` whose scope is the verified
  `PalwKernelMaterialBundleV1` of every committed position (the ledger's own derived mask, which under the commitment-only route
  also withholds model-dependent values), signed byte cap = bundle + manifest framing. Fold fixes found on the way (dormant path):
  a seeded (bootstrap / seal-reveal) lock tolerates an unfunded source target, `refresh_pending_paid_source_locks_v2` no longer
  re-judges a seeded lock against a v5 beacon (it disqualified the block), and the check authority of a seeded lock no longer needs a
  funded target.
* Result (`PALW_PAID_SOURCES=1`, first conformance test): paid class, job enrollment, covenant with manifest, seal, paid commit,
  bootstrap lock and the three drawn seats' receipts all land; the paid claims are complete. **Blocked next by a parameter
  mismatch (user decision):** a paid class's signed lifetime must cover its source windows (`PaidClaimClassV2::validate`; checking
  until commit + 364, Final ≈ commit + 444 here), while the conformance attempt's DISTINCT beacon accepts Finals only inside
  `PALW_ONBOARDING_POLICY_WINDOW_DAA_V1` = 120 DAA (INTERIM). No paid source can be Final inside that window, so the beacon reports
  UNAVAILABLE. Either the onboarding window grows to the paid lifetime, or the paid lifetime of a source class shrinks (its windows
  are interim values too); both are design values under ADR-0181.

### 7. Window widened (user decision 10-17) and the migration run

`PALW_ONBOARDING_POLICY_WINDOW_DAA_V1` 120 → 1,024 (ADR-0185 §4). The `Cw` harness now uses approved paid sources by default.
`g14_kernel_route_e2e::{conformance, opv_bootstrap}`: **25 / 54 PASS** (16 before), including
`g14_conformance_evidence_passes_only_after_an_unrefuted_window_and_the_class_activates` (beacon locked on two bootstrap-locked
paid Finals, evidence passed; its state assertion now also accepts a direct ACTIVE_REWARDABLE when the binding is already Final).

Remaining 29, by cause:

| cause | tests | next step |
| --- | --- | --- |
| x8 REAL attempt is not the sink — the paid path arms `palw_monetary_material_v1`, past which only the PAV3 native custody attempt form is admitted; the x8 harness builds PAV2 | 15 x8 + 3 native_final | build the REAL attempt as PAV3 with a provider exchange in the x8 harness |
| a SECOND beacon in one test: after the first approved paid Final the bootstrap window is closed, and the next paid lock needs the seal-reveal seed, i.e. funded sources that opted in after it | 3 conformance (another-policy / forged-evidence / hostile) | fund each later paid source claim (FundBeaconSourceFee) and have other producers opt in their seals (F2 allows paid jobs) |
| native_final covenant "signed material window differs from the physical monetary window" | 1 | that test arms its own monetary policy; align `arm_paid_sources` with it |
| x8 kernel claim not committed | 1 | not investigated |
| pruned import history mismatch (consensus `conformance_rows_survive_reorg_restart_and_pruned_import`) | 1 | same fix as the kaspad fixture (history from the node's own pruning samples) |
| opt-in source tests (v3 / v4 / v5 and `from_zero` / `eligibility_lost`) still use OPV sources | 6 | move their sources to paid claims (F2) |

### 8. Migration round 2 (2026-10-18): PAV3, covenants, seal-reveal sources, x8 on paid kernel claims

Real path only; no adapter, gate or seam was weakened. Fences stay dormant; t12 identity unchanged.

**Two consensus bugs, both past the dormant `palw_monetary_material_v1` only** (commit `b6bb15419`):

- `palw_exec_v2_origin_v1::decode_header` bounded the carried `palw_commitment` by the PAV2 cap (8 KiB). A PAV3 carriage (two provider
  signatures, the executor key and seal) exceeds it, so every PAV3 REAL claim's original header was unreadable and its block
  disqualified (`AuthorizationOrigin`). The PAV3 magic now selects `PALW_ATTEMPT_MATERIAL_MAX_BYTES_V3`.
- `require_monetary_material_enrollment_v1` looked a covenant up by `terms.root` only. A native PAV3 claim's covenant is its prior
  session, keyed by the session root and bound to the claim at acceptance (`native_binding`), so the claim's own budget commit was
  refused ("new paid material has no explicit signed monetary covenant"). It now reads the binding, as
  `require_native_monetary_material_scope_v1` already did.

**Harness on the real path:**

- x8 REAL attempts mount the owned PAV3 form: the executor and two independent providers enroll a prospective native session
  through a block (`MonetaryMaterialEnrolledV1` with `native_session`), the lottery prices the provisional PAV3 envelope, the
  providers sign the Ready and the executor seals it.
- Every kernel claim past the monetary fence commits under its producer's signed covenant over the complete served material
  (`kernel_covenant`, shared by the conformance world and `paid_path`).
- A later paid target (after the first approved paid Final) locks on the seal-reveal seed: the payer funds the future source fee
  right after the commit, bonds that are no live source target's producer (RFC07-3) seal on the eligible class inside the window,
  opt in, reveal under covenants; the lock follows the reveal window. One sealed source may seed several targets.
- `paid_path.rs`: the same steps on any `Net` world (paid class, payer enrollment before any seal, paid commit as the salted
  reveal, lock, the drawn seats' receipts). Two paid commits in one block land only the first; the helpers send one a block.
- Since the RFC07 adapter an EXEC slice verifies only through an approved, non-bootstrap paid Final of its kernel claim
  (`palw_exec_v2_selected_approval_v1`, `paid_claim_bootstrap_locked_v2`): the x8 prefix/slice claims now run that path.
- The pass assertions accept `ACTIVE_REWARDABLE` only past the artifact binding's horizon (the paid lifetimes outlast it).
- The harness establishes a pruning import target with the node's own pruning-sample history (as the kaspad fixture).

**Results.** Full `conformance` + `opv_bootstrap` run before the x8 paid migration: **37 / 54** (25 before this round). Targeted runs
after it, passing: `opv_bootstrap` sealed-source v3, from-zero; `conformance` another-beacon, forged-evidence, hostile-evidence;
x8 snapshot origin, canonical prefix, outsider conviction, tiny run, lying REAL run, signed legacy job, RFC15 conviction and
default. `conformance_rows_survive_reorg_restart_and_pruned_import` passes its import now; its pass assertion was fixed after the
run (not re-run).

**Still failing, by cause:**

| cause | tests | next step |
| --- | --- | --- |
| **design conflict (user decision):** a v4/v5 opt-in target needs its sources Final by `final_by = reveal_end + source_final_slack_daa` (120 in the probe), but an approved paid source's own lifetime contains a source window of the same policy plus check/challenge/court windows, so it is Final after `final_by` by construction | `opv_bootstrap` v4 ordinary-withholding, paid-v5 ×2; core `rfc0010_production_fold::paid_panel_sources…` (Panel v5 beacon, same collector) | decide how v4/v5 (and the Panel v5 beacon) read approved paid sources — e.g. the seal-reveal seed as F1 does, or a `final_by` measured from the source's own lifetime |
| **design value (user decision):** `PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1` = 200 DAA, shorter than the paid-source lock (≈ 450 DAA after the commit), so the binding is Final before the conformance can pass; a DA lapse after the pass can no longer refute it | `opv_bootstrap` eligibility-lost | raise the binding liability to at least the paid lifetime + the evidence window (ADR-0181 path), or drop that scenario |
| x8 assertions written for OPV timeout Finals: signed root expiries shorter than the paid Final, collateral deltas that now include covenant and source reservations, Final due later | completed-worker, withheld-slice, kernel-finals restart/reorg, second-real-root, honest-slices (`WindowClosed` at the paid window end), OPV-settlement (the slice leaves `Pending` earlier), native-Final ×3; competing-claim: a paid job holds its one claim, so the competing claim is refused at commit (the property the test guards holds more strongly; its inclusion assertion needs rewriting) | per-test: lengthen signed lifetimes inside the max, recompute the economic deltas from the rows, wait for the paid Final |
| no seam (RFC08 08-C): the class is `Candidate` under the model registry | native paid-material window | the no-seam EXEC fixture (item 2) |
| not diagnosed | native happy-Final (`card 1's attempt block` disqualified) | read its disqualification (`G14_LOG=…virtual_processor=info`) |


## Codex handover 3 — 2026-10-18 (in progress)

User authorized base `042e5e759` and branch `codex/rfc08-09-10-12`, with RFC09/RFC12 first. RFC08 08-B/08-C and RFC10 remain
waiting for a newer `integ/validation-r2` commit to merge. Validation-lane changes win any conflict; the touched paths must be
identified in the merge/fix commit message. No preset, activation height, identity or frozen design is changed.

**RFC09 implementation in progress:** op 203 builds the committed leaf using the full node's V1/V2/V3 grammar and exposes bounded,
versioned window/registry sub-openings. Empty proof requests retain request-v1 bytes; below the commitment fence the RPC retains
its legacy diagnostic leaf separately from authenticated openings. The issuer checks openings through the common reader before
signing. The remote Rule E path uses that reader, selected-parent walks, authenticated window suffixes and fork-registry
membership/absence/capital, the shared Rule E accumulator/decision functions, and the finality/provenance guards. Missing or
invalid evidence refuses the Rule E path before signing.

**Verification:** tests added, not yet reported as passing: core grammar/root/point tampering and sub-opening completeness;
RPC v1 compatibility / v2 round trips; remote issuer grammar/version binding; actual-node op-203 V2/V3 reads; a two-branch node
comparison under Rule E + Gate C (the no-certified-credit case), including missing and tampered evidence. Initial targeted core
build is in progress. No RFC completion claim is made. Socket-level RPC, populated certified-work fork behavior and the miner's
signing refusal still require verification; RFC12's four capabilities, signed refund CLI and acceptance run are not implemented
by this change.

**RFC12 partial implementation (same handover, unverified):** added the non-default `rfc12-import-failpoints` build feature,
which reuses the environment-and-owned-directory armed OS-kill rendezvous previously available only in unit tests. Existing
UTXO clear/copy and import-completion checkpoints are included, together with before/after synchronous barriers for session
begin and PALW/EVM/legacy sidecars. The private profile manifest advertises whether the feature is compiled. Default node builds
retain no pause/environment path. Acceptance-driver wiring and the crash/restart run remain outstanding; this does not close
checkbox 3 or the other three node-tool gaps.

**Verified core subset (2026-10-18):** `palw_fork_choice_rule_e_leaf_v3::tests` **5/5 pass**, including the two new RFC09 tests
and the three existing V3 root/certification/compatibility tests. Command-local core test optimization is disabled; the first
execution at the default 2 MiB test-thread stack aborted with stack overflow. The unchanged test binary passes with
`RUST_MIN_STACK=67108864` (the validation lane's documented debug-stack setting). RPC/remote/node tests remain unverified at
this checkpoint; no end-to-end completion is claimed.


### Codex handover 3 continuation — 2026-10-18 (implementation, acceptance pending)

- RFC09: core leaf/opening tests 5/5 and RPC V1/V2 transport tests 2/2 passed. A check of RPC service, gRPC core, remote
  binaries (`rpc` feature) and CLI passed before the subsequent RFC12 additions; current-tip verification is still required.
  The real-node tests have not run yet: their first compile found a missing `VirtualStateStoreReader` trait import in the new
  RFC12 read, now corrected. No socket-level or populated certified-work fork result is claimed.
- RFC12: added read-only op 214 (`getRfc12State`) for block reserve balances and up to 64 exact UTXO points, retaining a single
  virtual-generation lock for all requested points and propagating store errors. A missing reserve row is a known zero only
  with retained validation data/genesis, or is read from the matching pruning snapshot; unavailable history stays unknown.
  Added `evm deposit-refund` and an owner/timeout-bound ML-DSA builder; script-engine and tamper tests are written, not yet
  reported passing. The request generator runs inside the exact private paid-v4 factory, checks node genesis/params identity
  and authenticates the complete carriage against an explicitly pinned live header before creating a V4 request. The V4
  request codec is shared with the exporter without changing its bytes.
- RFC12 acceptance driver: exact WAL rendezvous is armed after genesis initialization and before the owned import link;
  restart clears arming. Bridge checks now read original inputs and materialized outputs by outpoint, link the refund to its
  original input/body and check value conservation, and compare those rights after restart/reorg/join. Python controls:
  existing acceptance tests 52/52, new point-read controls 4/4, exact-WAL protocol controls 4/4 pass (synthetic unit controls,
  not real-process observations). Full acceptance, reserve accrual/drip reconciliation and all runtime tests remain open.
- All changes remain dormant with respect to shipped consensus presets. No RFC08/RFC10 implementation has started; the fetched
  validation branch still names `042e5e759`, so the required newer validation commit has not yet been merged.

### 2026-10-18 — RFC09/RFC12 follow-up verification

- Actual consensus harness `t12_fork_choice_commitment`: **4/4 pass** (V1 fence compatibility, V2/V3 op203
  openings/subproofs, two real no-credit forks under Rule E + Gate C + capital, exact UTXO and known-zero reserve reads).
  The two-fork fixture now copies each full node's served DNS gate facts into the issuer's attestation. Missing subproofs
  and substituted committed roots refuse. This is an API harness, not socket-RPC or populated certified-credit coverage.
- `rfc12_refund_is_timeout_bound_owned_and_validates_with_the_real_script_engine`: **1/1 pass** through the real script engine,
  with changed output/amount/timeout, wrong key, early refund and excessive-fee refusal controls. No private-node
  broadcast/reorg/restart refund run has been recorded yet.
- Private paid-v4 node with EVM and `rfc12-import-failpoints`: compile check passed. The generator additionally shares the
  exporter's producer-input allocation preflight and receives the acceptance driver's pinned `{REQUEST_BLOCK}`. Current-tip
  CLI/gRPC checks, standalone input preflight tests, V3 miner signing-refusal controls and positive reserve/drip API assertions
  are still being verified; compilation alone is not runtime acceptance.
- Python exact-WAL controls **5/5 pass**; an exact rendezvous cannot share its launch with a coarse import-phase kill.
  The earlier 52 acceptance controls and four point-read controls remain unit controls only. Full acceptance and independent
  reserve-flow reconciliation remain open. Documentation checks report zero new issues (43 existing baseline occurrences).
- A fresh fetch still names validation base `042e5e759`; RFC08 08-B/08-C and RFC10 remain gated on the required newer merge.

- Follow-up results: current private node + CLI + gRPC client compile check passed. The actual-chain
  `pos_v2_reserve_drip_pays_finalized_epoch` test passes **1/1**, checking op214's consensus read against the
  independently calculated positive slash split, its later coinbase drip, and the unchanged historical balance.
  The retirement acceptance phase now records bounded per-block reserve reads; independent process-level flow
  reconstruction remains open. Retirement reconciliation controls pass **13/13**; acceptance controls remain **52/52**.

- Remote results: `l2::tests` **17 pass, 1 ignored** (the ignored item is the existing cost benchmark), and
  `miner::tests` **17/17 pass**. The latter includes the new V3 successful-publication control and missing/tampered
  opening refusals before execution/signing. These are scripted remote-node controls, separate from the real-chain
  two-fork API test above. Relevant legacy DNS anchors are walked through authenticated selected-parent openings,
  including anchors before the Rule E fork point. A mixed pre-Rule-E fork still fails closed rather than using a
  cross-activation comparator; no activation or complete migration acceptance is claimed.
