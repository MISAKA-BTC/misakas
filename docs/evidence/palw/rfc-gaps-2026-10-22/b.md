# RFC gap table — lane rfcgap-b (2026-10-22)

Worktree `MISAKA-rfcgap-b`, branch `rfcgap/b`, base `b1eb33960`. RFCs: 0008, 0012, 0015, 0016, 0017 (two-tier rule only),
0018, 0019, 0020. User decision 2026-10-22: bond allocation average = 7 days (code already 7).

Method: RFC status line and decision catalog for adoption; the RFC's own implementation-status table; then each named symbol in
code, followed to its production caller (fold, processor, RPC). Items already handed to other lanes (U01–U14 of
`g14-parallel-2026-10-22/unimplemented-survey.md`, the lane notes in that folder) are not repeated unless a part was uncovered.
Line numbers are at `b1eb33960` plus this lane's commits.

Classes: **IC** implemented and connected (production caller), **TO** implemented but test-only, **DR** dormant and refused
without a record / unconditionally refused, **M** missing, **V** needs measurement or a user-approved value only, **NA** not adopted
(Draft), **DF** deferred (RFC-0003/0004 scope), **RJ** rejected/abolished.

## Summary (counts by class)

| RFC | IC | TO | DR (gap) | M | V | NA | RJ | implemented here |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0008 | 6 | 0 | 0 (U12 lane X) | 0 | 6 | 1 | 1 | — |
| 0012 | 5 | 0 | 0 | 0 | 4 | 1 | 0 | — |
| 0015 | 4 | 0 | 0 | 0 | 2 | 0 | 1 | doc note (7-day) |
| 0016 | 8 | 0 | **2** | 0 | 4 | 0 | 0 | §14 record gate (2 fences); stale 30-day texts |
| 0017 | 4 | 0 | **1** | 0 | 1 | 0 | 1 | tier record gate |
| 0018 | 9 | 0 | 0 | 0 | 6 | 1 | 0 | — |
| 0019 | 4 | 0 | (shared with 0017) | 0 | 3 | 1 | 0 | tier record gate binds the catalog tip |
| 0020 | 4 | 0 | 0 | 0 | 0 | 2 | 0 | — (one decision question) |

## RFC-0008 — EXEC lane, claim-backed slices (Revised Draft; R8-* spec closure 2026-10-15)

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| §2, R8-LANE | exclusive TX/SLICE envelope; mixed/empty refused | `consensus/core/src/palw_exec_v2.rs:609` (validator), fold inputs `consensus/src/pipeline/virtual_processor/processor.rs:7572` | IC (dormant) | — |
| §2 / §9 gates | EXEC payload v2 armable | `palw_exec_v2.rs:609-625` fixed refusal "section 9 gates" | DR | skip: U12 (lane X), needs the user's split of §9 gates into acceptance vs measurement |
| R8-IDENTITY 3 | `L_auth`, job-use history caps | v1 structure profile in code | V | USER DECISION REQUIRED (named in RFC) |
| R8-BUDGET 4/6 | `ExecutionCap` ratio, `W_budget` production value | drill value 400 only | V | user value |
| R8-WINDOW 6/7 | `PalwPanelLifetimePolicyV2`, `PalwMonetaryMaterialPolicyV1` | `palw_panel_lifetime_v2.rs:189`, `palw_bond_budget_v1.rs:806` refusals | V | user values / review |
| R8-WINDOW 10 | consented horizon extension (inner 68, aux 126) | `palw_econ_contracts_v1.rs:894` | IC dormant, V for arming | user value |
| R8-ECON 3 | minimum price contract (inner 69, aux 127), settlement floor | `palw_econ_contracts_v1.rs:910` | IC dormant, V | measured class costs |
| 2026-10-19 note | original acceptance refuses dependencies not covering `a + 10,000` (F07) | `palw_court_clock_v2.rs`, `palw_legacy_public_filer_fold_v1.rs` (F07) | IC (lane r5-clock-deps-relay) | — |
| R8-LANE 3 | `PalwMaterialSessionMinProviders` | structural minimum 2 in code | V | user value |
| ALL-MODEL | court never demands model bytes; N4 excluded from new reward/weight | RFC-0006 2026-10-15 fences (lanes) | IC dormant | — |
| §0 | old algo-11 slice-clock design | removed | RJ | — |
| R8-* "Remaining" | real-node closure, restart/reorg, pruned join | validation-only | V (evidence) | — |
| 2026-10-12 | model allocation ↔ block opportunity "proposal" | superseded by RFC-0016 V+ | NA | — |

## RFC-0012 — PALW-only consensus and native EVM settlement (Draft, dormant)

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| R12-SETTLE 1 | one shared open-dispute inventory (DA + courts) | `consensus/core/src/palw_native_settlement_v1.rs:232` `native_open_sessions_v1` | IC | — |
| R12-SETTLE 2–5 | unique work from carrying delta, maturity, prefix | `consensus/src/pipeline/virtual_processor/native_settlement.rs` | IC (dormant) | — |
| R12-SETTLE 6 | D, W, concentration caps, `M_maturity`, horizon | drill-provisional only | V | user/census |
| R12-OPS 1 | retired DNS kinds refused after retirement | `palw_dns_retirement` checks, `consensus/core/src/config/params/validation.rs:1016` | IC (unassigned fence) | — |
| R12-OPS 2/7 | wallet preflight capability | `kaspa-wallet-core::misaka_preflight` | IC | 1,000 DAA margin: USER REVIEW |
| R12-OPS 4 | release observation bounds | none in code by design | V | user values |
| R12-MONEY 1 | retirement subsidy split / fee schedule | committed only inside unarmed policy | NA (`92/8/0` not adopted) | — |
| 2026-10-15 amendment | settlement means SecurityCertified credit only | `palw_native_settlement_v1.rs:161`/`:338`/`:404` | IC (Gate C fence) | — |
| acceptance (lane rfc23-12) | refund-once / reserve across cold reopen | `scripts/drills/rfc12_acceptance_judgement.py` | IC (driver) | — |

## RFC-0015 — Panel=0 permissionless verification

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| §13.3 | ACTIVATION_ALLOWED AND contract via an authenticated gate record | `consensus/core/src/palw_panel_free_v1.rs:178-214`, `palw_panel_free_gate_v1.rs` (pins empty) | IC (record-gated) | — |
| §12 rows | OPV lifecycle, court, Final, producer reservation, budget coupling, RPC | status table `docs/design/palw/integration/rfc-0015-implementation-status-2026-10-15.md` ("Code gaps: none known"); verifier `kaspad/src/palw_kernel_verifier.rs:221` | IC (dormant) | — |
| §8.5.5 | registrant earns no coinbase leg (Position fee only) | ADR-0091/0114 rules unchanged; no registrant coinbase leg in allocation | IC | — |
| §8.5.5 | α = 1.5, miner + verifier capital, average window | `palw_bond_budget_v1/allocation_v3.rs:30-32,94-101` (7 days) | IC | doc note added (30 → 7 days, 2026-10-16/10-22) |
| §8.5.5 | verifier participation / bond-without-checking rules | open in the RFC itself | V | user design |
| §13 | Panel=0 deferred (N5) | gate pins empty | V | — |
| old Seeder/TRDC/FPR gates | withdrawn | — | RJ | — |

## RFC-0016 — V+ block opportunity and elastic Final capacity (Accepted design, values pending)

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| §8 V+ issue / admission / reissue | rights `⌊(N_exp−Ent)·A_m/ΣA⌋`, own then σ-capped reissue, quote | `palw_rfc0016_v1/rights.rs:138`; fold `palw_rfc0016_fold_v1.rs:270-293`; hook `palw_state_v2/fold_read.rs:1021` | IC (dormant) | — |
| §10 elastic capacity, queue | water-filled capacity, bounded queue, TTL, C7 | `palw_rfc0016_fold_v1.rs:301,547`; hook `fold_read.rs:1055` | IC (dormant) | — |
| V8 | signed capacity attestation (inner 76, aux 135 rows) | `palw_rfc0016_fold_v1.rs:480` `apply_attest_capacity_v1` | IC (dormant) | per-class loads unmeasured (V) |
| §11 report / RPC op 255 | outcome-first report | `rpc/service/src/service.rs:3895` | IC | — |
| §13 C1 | mode-specific `W_hold` (Panel=0 7d / Panel=1 1d) | `palw_bond_budget_v1.rs:501` `PalwHoldModeV1`; `consensus/core/tests/adr0176_mode_hold_v1.rs` | IC (RFC row "未実装" is stale) | — |
| §13 C2/C3 | `S_m` = averaged miner + verifier; α = 1.5 decided | `palw_bond_budget_v1/allocation_v3.rs:4,50,94-101` | IC (RFC rows stale) | — |
| §13 C4/C5 | escrow never clipped; R shortage refused before acceptance | `eee1710ce` code, tests `rfc0016_p4_*` | IC | — |
| ADR-0179 interplay | ε = 0 past the tier fence | `palw_rfc0016_fold_v1.rs:140-145` | IC | — |
| §14 | `ACTIVATION_ALLOWED` AND contract | `palw_rfc0016_v1/values.rs` validators ended in a FIXED "not adopted for activation" refusal after the measured gate | **DR → implemented** | §14 arm-readiness record gate (`PALW_RFC0016_ARMING_RECORDS_V1`, empty) |
| §14.1 V2–V9 | σ, φ, pace, ε, queue, `h`, γ, `N`/`m`, V8 loads | `values.rs` placeholders + `PALW_RFC0016_MEASURED_RECORDS_V1` (empty) | V | measurement |
| V5 / 2026-10-22 | 7-day average | `values.rs` constant 5,040 DAA; three comments and one refusal text still said 30 / 21,600 | **stale text → fixed** | — |
| §12.3 K1a | "no coinbase to a Kernel claim on any path" test | pure check only (`values.rs` `palw_budget_origin_is_user_escrow_v1(KernelRoute)` test) | V (acceptance evidence) | validation lane (node E2E) |
| §8.6 tilt `k` | lottery wiring | pure fn only, `k = 1` by decision V2 | V | after measurement |

## RFC-0017 — model tiers (two-tier rule only; per-model exam abolished)

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| 1/2 | genesis + annual = 1.0; registered = 0.3 fixed, no review | `consensus/core/src/palw_model_tier_v1.rs:263` `palw_model_tier_at_v1` | IC (dormant) | — |
| 4 | `c_m` applied once in `A_m`; κ cap | `palw_bond_budget_v1/allocation_v3.rs:209` `roll_epoch_v3_tiered`, called from `palw_bond_budget_fold_v1.rs:2338` | IC | — |
| RPC | registry row `modelTier`, `modelTierPermille` | `rpc/service/src/service.rs:2320` | IC | — |
| 5 | per-claim Gate C for every tier | `palw_rfc0018_v1.rs` validator needs the tier fence at or below Gate C | IC | — |
| arming | tier fence refusal | `palw_model_tier_v1.rs:359-363` FIXED refusal | **DR → implemented** | record gate `PALW_MODEL_TIER_ARMING_RECORDS_V1` (empty), binds c, κ, catalog tip |
| values | κ (and c's measured record) | design 300 ‰ | V | USER: approve κ |
| old exam | Registered/Probation/Eligible/Lapsed | `palw_model_eligibility_v1.rs:1358-1362` refuses; unreachable under tier fence | RJ (kept dormant as history) | — |

## RFC-0018 — DAG-parallel multi-model consensus (Draft with individually adopted items)

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| §4 | canonical sibling order (merge-order independent) | `palw_state_v2.rs:15008-15013` | IC (dormant) | — |
| §5 | permissionless anchor (excludes producer, committed fold output) | `palw_rfc0018_v1.rs:554`; processor `processor/palw.rs:10500`; fold `palw_state_v2.rs:18076` | IC | — |
| §7 Gate C (adopted) | commit / draw / attest / decide / sweep / credit | `palw_security_certification_fold_v1.rs:52,155,613` | IC (dormant) | — |
| §7.5/7.5a | frontier single rule + rollback | `palw_rfc0018_v1.rs:270` | IC | — |
| §7.7 | pruning progress ceiling | `palw_security_certification_v1.rs:522` | IC | — |
| §7 receipts | one receipt predicate (incl. accusation pending) | `palw_rfc0018_v1.rs:312` | IC (lane gatec-capacity) | — |
| §8 | DAG width schedule readers | `palw_rfc0018_v1.rs:888` `ghostdag_k_history` etc.; GHOSTDAG `with_dag_width` | IC (status-doc "stub" is stale) | values U-4 (V) |
| arming | five fences | `palw_rfc0018_v1.rs:841` record list (empty), `:945` | IC (record-gated, lane gatec-decouple) | — |
| §12 U-1..U-8 | activation order, U-2 issuance-leg split, U-3 FP carve (GAP-C3), U-5 m/k, U-6 veto, U-7 tie, U-8 public-check classes | — | V | user decisions |
| §6 rule E v2 | ships with rule E (ADR-0178 full-activation) | `palw_fork_choice_rule_e_v1.rs:102` | V | full-activation release |
| U-6 veto | not adopted | — | NA | — |

## RFC-0019 — annual model upgrade (Draft; §1 design and 2026-10-15 user policy binding)

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| §5 | catalog type, root, add-only chain | `consensus/core/src/palw_annual_catalog_v1.rs:164-293` | IC | — |
| §6 | `model_tier_permille` through the tier fence | `palw_model_tier_v1.rs:263` | IC | — |
| §7.7 | RPC `annualCatalog` | `rpc/service/src/service.rs:2405` | IC | — |
| §8/§12.1 | release manifest tools; R-7/R-8 drill modes | `scripts/release/*`, `scripts/drills/rfc0019_rolling_upgrade_drill.py` (lane rfc09-13-19) | IC | — |
| §16 | per-edition activation gate (manifest, drills) | was folded into the tier fence's fixed refusal | **DR → implemented** (with RFC-0017) | record binds the catalog tip edition and root |
| §13/§14 | timeline, signer set, edition contents, U-5 | — | V | user |
| §4.2 U-1 | abort handling | — | NA | — |

## RFC-0020 — long-context calibration memory (Draft / partly implemented; no consensus surface)

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| §2 | RAM accounting mode | `misaka/palw/tir/lower/src/ram_account.rs`; `--ram-accounting` in `palw-tir-convert`/`palw-tir-fidelity`; `scripts/genesis/genesis10_pipeline.py:1179-1186` | IC (unverified) | — |
| §3 | carry double-hold removed, bit-identical | `misaka/palw/tir/lower/tests/position_blocks.rs` | IC | — |
| §4 | lazy weight defaults + warning | converter flags | IC | — |
| §6.2 | carry disk spill (opt-in) | `--spill-carries-dir` in both bins | IC | — |
| §5.2/§5.4 | tiled exact attention, chunk-aware observer | spec only; §5.1 audit: no full-matrix path on the genesis text route (user policy item 5 conditions it on such a path) | NA (condition not met) | — |
| §6.3 / user policy 7 | KV history disk spill for >32K | spec only; the RFC argues one layer's KV fits (1 GiB at 128K) and per-query disk reads are O(N²) I/O | NA in the RFC's own plan | **decision question**: does policy item 7 still require KV spill now, or only when a >32K target exceeds RAM? |

## Implemented in this lane

1. **RFC-0016 §14 arm-readiness record gate** (`consensus/core/src/palw_rfc0016_v1/values.rs`): `PalwRfc0016GatedFenceV1`,
   `PalwRfc0016ArmingRecordV1`, `PALW_RFC0016_ARMING_RECORDS_V1 = &[]`, `palw_rfc0016_arming_record_admits_v1`,
   `Params::palw_rfc0016_value_and_arm_gate_v1`. Both validators keep every structural refusal, the calibration exception and the
   measured-value gate, then require a record for exactly (fence, genesis, height, value set) with one evidence digest per §14
   conjunct. Empty list: same outcome as before (refused), now naming the missing record. No params/wire/identity change.
2. **Model tier arm-readiness record gate** (`consensus/core/src/palw_model_tier_v1.rs`): `PalwModelTierArmingRecordV1`,
   `PALW_MODEL_TIER_ARMING_REQUIREMENTS_V1` (8: ADR-0179 approval of c/κ, tests, RFC-0019 §11 and R-1…R-8 drills, signed manifests,
   readiness report), `PALW_MODEL_TIER_ARMING_RECORDS_V1 = &[]`, `palw_model_tier_arming_record_admits_v1`,
   `Params::validate_palw_model_tier_with_records_v1`. The record must name the fence's own c, κ and catalog tip edition/root, so each
   new annual edition needs its own record. Empty list: refused.
3. **7-day consistency**: stale 30-day / 21,600-DAA comments and one refusal text in `palw_rfc0016_v1/values.rs` and `rights.rs`;
   notes in RFC-0015 §8.5.5, RFC-0016 §14.1, RFC-0017.

No new fence, wire tag, aux row, carriage tail, RPC op or borsh layout. No preset changes; fork id and identities unchanged.

### Runs (shared lock, `CARGO_TARGET_DIR=/Users/wata/Downloads/MISAKA-g14/target`)

- `cargo test --release -p kaspa-consensus-core --lib -- rfcgap_b_ palw_rfc0016_v1:: palw_model_tier_v1:: palw_rfc0018_v1::tests::gatec_decouple`:
  `test result: ok. 49 passed; 0 failed`, including
  `palw_rfc0016_v1::values::tests::rfcgap_b_rfc0016_arm_record_gate_is_exact_and_the_pinned_list_is_empty ... ok` and
  `palw_model_tier_v1::tests::rfcgap_b_tier_arm_record_gate_is_exact_and_the_pinned_list_is_empty ... ok`.
- `cargo test --release -p kaspa-consensus-core --test rfc0016_v1`: `test result: ok. 19 passed; 0 failed`.
- `cargo check --release --workspace --tests --exclude misaka-palw-worker`: `Finished`. The first attempt failed in `kaspa-grpc-core`
  on request variants (`GetPalwClaimDetailRequest`, …) that do not exist in this tree: the shared target held `protowire` generated
  from another worktree's `.proto` (workspace members share OUT_DIR across worktrees and the build script reruns only on mtime).
  Touching this tree's two `.proto` files regenerated it; no source change was needed.

## Open decisions (not implemented: need a user value)

- RFC-0017/ADR-0179: approve κ (design 300 ‰) and c's measured record; RFC-0019 U-3/U-4/U-6 (signers, edition contents, activation).
- RFC-0016 §14.1 V2–V9 production values (measurement), V8 per-class loads.
- RFC-0018 U-1…U-8 (notably U-2 issuance-leg split, U-3 free-prompt pre-certification carve = GAP-C3, U-5 m/k).
- RFC-0008 USER DECISION REQUIRED parameters (R8-IDENTITY 3, R8-BUDGET 4/6, R8-WINDOW 6/7, R8-ECON 3) and U12's §9 gate split.
- RFC-0012 R12-SETTLE 6 values, R12-OPS 4 bounds, R12-MONEY split, wallet margin review.
- RFC-0020 policy item 7: whether KV-history spill is required before a >32K target needs it.
