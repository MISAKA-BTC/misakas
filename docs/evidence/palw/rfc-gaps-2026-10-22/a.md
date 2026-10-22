# RFC gap survey — lane rfcgap-a (RFC-0001, 0002, 0006, 0007, 0009, 0010, 0011, 0013), 2026-10-22

Base: `b1eb33960` (integration of the G14/RFC lanes), branch `rfcgap/a`. Method: each RFC's status line, dated amendments and
2026-10-15 spec closures; the ADRs/register rows they cite; the activation-readiness matrix
(`docs/design/palw/integration/activation-readiness-matrix.md`); then the code (validators, folds, processor, kaspad, SDK) for every
adopted requirement. Items already owned by the survey `../g14-parallel-2026-10-22/unimplemented-survey.md` (U01–U14) and by the
lane notes in that directory are not repeated unless a part was left uncovered. Running lanes (audit/fix-a, -b, -cd; storage/*) were
not edited.

Adoption reading. RFC-0002/0006/0007/0009/0010/0011/0013 carry "Revised Draft" status lines; their **adopted** parts are the dated
user rulings (2026-10-08 coverage target, 2026-10-12 register N3/N4/D2/X3/W1, 2026-10-15 genesis/§8/§9 decisions), the 2026-10-15
spec closures of RFC-0006/0007 (normative text, values marked USER DECISION REQUIRED), ADR-0171/0175/0176/0177 and RFC-0001 §A/§7.1.
The 2026-10-12 "提案" amendments (Panel=1 modes, seating, W2/X4 text) are not adopted. Per PRINCIPLES §8 no cryptographic proof of
computation is in scope; per §9 no per-model exam.

Classes: **IC** implemented + connected · **TO** implemented, reachable from tests only · **DR** dormant, refused by name / no fence ·
**MS** missing · **MV** needs measurement or a user-approved value only · **NA** not adopted (Draft/proposal) · **DF** deferred
(RFC-0003/0004 media, model improvement) · **RJ** rejected/withdrawn · **TG** a test is missing (code connected).

## Counts

| RFC | IC | TO | DR | MS | MV | NA | DF | RJ | TG | implemented now |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0001 | 6 | 0 | 0 | 0 | 0 | 2 | 1 | 0 | 0 | — |
| 0002 | 6 | 0 | 0 | 0 | 4 | 1 | 1 | 1 | 0 | — |
| 0006 | 4 | 0 | 2 | 0 | 4 | 0 | 0 | 0 | 1 | U11 record gate; N3 second lock |
| 0007 | 6 | 0 | 0 | 0 | 6 | 1 | 0 | 1 | 0 | — |
| 0009 | 6 | 0 | 0 | 1 | 1 | 1 | 0 | 1 | 0 | — (the MS row is audit/fix-cd's) |
| 0010 | 2 | 0 | 0 | 0 | 2 | 1 | 0 | 0 | 2 | reorg-across-freeze processor test |
| 0011 | 4 | 0 | 0 | 0 | 2 | 2 | 0 | 1 | 0 | — |
| 0013 | 4 | 0 | 0 | 0 | 2 | 2 | 0 | 0 | 0 | — |

## RFC-0001 — inference surface

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| §A.2–A.4 | FP Job V4 decode pipeline (penalties, bias, stop, D10/D11), fence `palw_fp_decode_rules` | `consensus/core/src/config/params.rs:2833`; armed on t12 at 5,300 (`PALW_T12_INT11_FENCES_V1`, `params.rs:11725`) | IC | — |
| §7.1-4 / §7.3 | KV prefix stage 2 / 2b (`palw_fp_prefix_state`, `palw_fp_prefix_inherit`), inheritance-safety admission | `consensus/core/src/palw_fp_prefix_v1.rs:234` (`palw_fp_prefix_inherit_class_safe_v1`) | IC | — |
| §7.2 | constraint court third arm as a wire proof kind; `render_answer_v2` | `palw_step_refute.rs:5969` called by `palw_offence_attribution_v1.rs:2123`, `kaspad/src/palw_fraud_filer_constraint.rs:335`; `misaka/palw/base0/src/fp_worker.rs:1256` | IC | — |
| §7.3 | seat token table auto-load | `kaspad/src/palw_public_token_table.rs`, `kaspad/src/palw_backends.rs` | IC | — |
| P1 (delivery record §6) | worker cancel frame | `fp_worker.rs:1515` (`Cancelled`) | IC | — |
| P2 | F1/F2 hoisted into `validate_against_request` | delivery record §6 | IC | — |
| §7.1-5/-6, P3, P4 | embedding claims, multimodal tensors, V5 commitment door, generative Text arm | V5/image paths (`palw_fp_job_v5`) | DF | RFC-0003 media deferred (ADR-0182); text-only genesis scope (RFC-0002 §II.14.61) |
| 2026-10-12 W1/W2/X4/N4 | proposals; the user kept W1's integer-seed refusal (existing test, register line 84); N4 became `palw_public_evidence_reward_v1`, record-gated (`validation.rs:41`, counted under RFC-0006) | — | NA | — |
| §2.4–2.11 P1–P3 body | n>1, JSON-schema extension, embeddings claim, LoRA class | status line: "P1〜P3 は Draft" | NA | — |

## RFC-0002 — tensor IR

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| spec 04b / §II.14.2 | 25 frozen v1 primitives, typed bounded graph, versioned extension | `misaka/palw/tir/*`; ledger `docs/design/palw/tir/rfc0002-implementation.md` | IC | — |
| §II.14.61 | genesis 8 catalog, text-only scope refusals | `consensus/core/src/palw_genesis8_catalog_v1.rs:76,92,103` (dormant: edition 0 from supplied roots) | IC | — |
| §II.14.61 | PALWART3 root ↔ registered inventory root / tier read | `palw_tir_container_v1.rs`, `palw_artifact_identity_v1.rs` (lane rfc02-09) | IC | — |
| §II.14.61 | per-model conversion, 32K calibration, fit guard, PALWQ361 reference windows | `scripts/genesis/genesis10_pipeline.py` (fit refused without windows/scale, lane rfc09-13-19) | MV | real conversions NOT_RUN; PALWQ361 logit scale is a user/measurement value |
| §II.14.62 | Gate C approvals for consensus credit, no local-replay substitute | `palw_rfc0018_v1.rs:181` (`palw_native_independently_work_certified_v39`) | IC | — (Gate C lanes) |
| §II.7.4 | registry `blocking` | `rpc/core/src/model/palw_blocking.rs:50` | IC | — |
| remaining-work 2026-10-15 | native39 lanes, audits, payout, mining work, activation | `palw_native_mining_v39.rs:529` refuses without `PALW_NATIVE_MINING_MEASURED_RECORDS_V39` (empty) | MV | measured records; real-node validation phase |
| §II.10 / RFC-0011 §18 | ≥90 % of `D_complete` (one-sided 95 % LB) | census tooling `misaka/palw/sdk/src/census/cli.rs` | MV | census + registrations (COV-P4) |
| §II.14.63 | unsupported model → versioned hard/soft fork | policy; no code obligation until a model is named | IC (policy) | — |
| §II.13 / II.14.6 | immutable challenge-policy binding, whole-statement public verification | kernel route (record-gated by lane kernel-route) | MV | arming record (U01) |
| 2026-10-12 seating amendment | single-seat scope | "提案" | NA | — |
| image/audio classes | §II.14.61 refuses media components | `palw_genesis8_admits_input_v1` | DF | — |
| GVM / residual VM | replaced by §II.11–II.12 (ADR-0172) | — | RJ | — |

## RFC-0006 — layer-sharded panels

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| v1 | shard fold, cells, receipts by parts, exact court | `palw_tir_shard_v1.rs:1069` (t12 armed at 5,300) | IC | — |
| R6-OUTSIDE 1 | non-seat watcher | `kaspad/src/palw_panel/tir_shard_watch.rs:152` | IC | — |
| R6-CELL REQ-1/1b/2/3 | coverage closure at registration, claim gate, court close | `palw_court_v2.rs:1399`, `palw_step_leg.rs:2340`, `palw_accept.rs:2049`, `reporting.rs:158` | IC | — |
| R6-CELL | the closure's validator refused **by name** ("not built and verified") though the rule is built (survey U11) | `palw_cell_coverage_closure_v1.rs:379` (before) | DR | **implemented**: record gate (below) |
| R6-MIGRATION 4 | V3 draw / segment pricing v2 / probabilistic cells dormant until N3 holds | each validator blanket-refuses; the N3 second lock (`palw_beacon_bootstrap_v1.rs:751`) did not list them | DR (second lock missing) | **implemented**: added to `PALW_N3_GATED_REWARD_FENCES_V1` |
| ALL-MODEL 3/4 | D2 model-byte routes closed; N4 non-public paths unrewarded | `validation.rs:22,41` (record-gated) | IC | — |
| R6-DRAW 6–7 | retry bound, strata, outsider, population cap, protocol-107 transport bounds | — | MV | USER DECISION REQUIRED (named in RFC) |
| R6-PRICE 1/4 | segment pricing v2 height; worst-cell bytes/time, court capacity, watcher fee | `palw_tir_shard_segment_v2.rs:223` refusal | MV | measurement + user values |
| ALL-MODEL 5 | allocation curve | — | MV | user decision (α=1.5 provisional) |
| R6-CELL remaining | Panel false-valid routes' closure mode; arming | `PalwCellClosureModeV1::for_blame_path` exists | MV | user decision (now a named record requirement) |
| 2026-10-08 open list | processor-level reorg / duplicate-receipt test for shard receipts | fold-level only: `consensus/core/tests/palw_tir_shard_fold.rs:977` | TG | not done here: needs a sharded TIR class in the processor harness (kaspad e2e exists) |

## RFC-0007 — probabilistic detection

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| Part VI / R7-RANDOM 1,5 | single post-commit challenge protocol, canonical collection, no fallback seed | `misaka/palw/challenge` (`collect_sealed_work_beacon_v3`), used by `palw_onboarding_v1.rs:763`, `palw_econ_contracts_fold_v1.rs:397`, `palw_panel_beacon_v1.rs:389` | IC | — |
| R7-SUBJECT 2–5 | paid ClaimVerification V2, consent clocks, checker snapshot | `palw_claim_challenge_v2.rs:142`, fold `palw_claim_challenge_fold_v2.rs` | IC (dormant) | — |
| R7-RANDOM 4 | NonInteractive initial; StagedBeacon/FS only via separately versioned plan | `palw_claim_interactive_v3.rs`, `palw_panel_beacon_v1.rs:211` | IC | — |
| R7-RELATION 6–7 | SumCheck tags frozen, zero credit; Freivalds/Linear direct checks only | `misaka/palw/kernel/src/claim_compact_v1.rs:527`, `native_inventory_v2.rs:133` | IC | — |
| R7-SOURCE 4 / N3 | new reward profiles dormant until bootstrap; second lock | `validation.rs:263` → `palw_beacon_bootstrap_v1.rs:767` | IC | (extended for RFC-0006, above) |
| R7-ECON 7–8 | model verification gate: freeze → tally → pay/defer/withhold | `palw_econ_contracts_fold_v1.rs:267`; validator `palw_econ_contracts_v1.rs:861` | IC (dormant) | — |
| R7-SUBJECT 6 | `PalwClaimChallengePolicyV2` values | — | MV | USER DECISION REQUIRED |
| R7-SOURCE 6–7 | `d_src`, caps, `work_count_k`…; BootstrapExact profile/kernel | `palw_beacon_bootstrap_v1.rs:1133` refusal | MV | USER DECISION REQUIRED |
| R7-RANDOM 2,7 | effective 128-bit accounting; Q, G, retries, error allocation | — | MV | external review + user values |
| R7-ECON 5 | per-attempt deposit binding, `N_max` | — | MV | USER DECISION REQUIRED |
| R7-ECON 8 | deferred release keeps Panel/buyback split? unavailable beacon withholds or pays? | source releases to producer alone; withholds | MV | USER DECISION REQUIRED |
| R7-RELATION 5 | class-by-class public refutation of routing/history/gather | — | MV | observations; per-class evidence |
| 2026-10-12 Work Beacon bootstrap text | "提案" | — | NA | — |
| GKR / producer sum-check validity transcripts | withdrawn by PRINCIPLES §8 | — | RJ | — |

## RFC-0009 — remote miner

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| §7 A0 / §3.3 | node-less registration (prepare → sign → relay) | `misaka/apps/cli/src/operator/model_add.rs` | IC | — |
| §7 A / §6 | remote claim, L1/L3 verification, signing gates | `misaka/palw/remote` | IC | — |
| §6 L2 (2026-10-08) | full-node comparator on verified inputs (rule E) | `rule_e_remote_v2` tests (lane rfc02-09 Task B) | IC | — |
| §7 C | V4 public redemption, `SigningPurpose::PalwReceiptAuthV4` | `consensus/core/tests/palw_receipt_spend_v4_fence.rs:138` | IC | — |
| multi-relay | rail auto-resubmit | `misaka/palw/remote/src/resubmit.rs` | IC | — |
| §7 B / §4.2 | provider court (claim material only) | `palw_provider_court_v1` (record-gated, lane kernel-route) | IC (dormant) | — |
| PAV3 native-ready exchange | provider-side HTTP endpoint | client: `misaka/palw/remote/src/native_custody_exchange_v3.rs:39`; no server | MS | owned by audit/fix-cd (not edited here) |
| 2026-10-19 F04 | court signer availability vs "PC can stop"; delegation spec | — | MV | needs decision: adopt a delegation/termination-evidence spec (RFC-0026 §6) |
| §8 / discovery | provider discovery, fee competition, light-client proofs | DESIGN_GAP | NA | — |
| Seeder/Torrent | withdrawn by ADR-0177 | — | RJ | — |

## RFC-0010 — permissionless Panel

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| §3 | seal → frozen population → entropy → bind on any block; carrier not entropy | `palw_panel_v3_fold_v1.rs`; processor E2E `t12_permissionless_panel_e2e.rs` | IC (dormant) | — |
| §3.2 / release item 3 | epoch source-profile freeze at a pre-entropy point | `palw_panel_v3_fold_v1.rs:291` | IC | — |
| release item 3 | processor-level reorg across a freeze block | none (the bind reorg forks after the freeze) | TG | **implemented** (test below) |
| release item 3 | `Chain` derivation from a real OPV-eligible kernel route | `palw_panel_v3_fold_v1.rs:298` reachable only with the kernel route armed | TG | blocked: needs the kernel route's arming record (U01) or an isolated G14 genesis harness |
| §3.3 / §9 | approved production Panel beacon | `palw_permissionless_panel_v1.rs:203` (registry empty) | MV | user approval after external bias review |
| release item 2 | objective L1 sealing/finality rule, numeric economics | — | MV | needs a specification decision |
| 2026-10-12 Panel=1 / bounded replacement | "提案" (U02 panel-one fence exists, decision pending) | `palw_kernel_route_arming_v1.rs:247` | NA | — |

## RFC-0011 — permissionless onboarding

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| 2026-10-09 / ADR-0175 | permanently immutable registration | `validation.rs:2967` (`validate_palw_model_immutable_v1`) | IC | — |
| §15 / ADR-0171 | sampling-first, court on dispute (kernel route) | record-gated (`palw_kernel_route_arming_v1.rs:167`) | IC (dormant) | arming record (U01) |
| §17 / RFC-0013 §9 | beacon conformance stages, resumable records | `misaka/palw/sdk/src/runtime_pack/beacon_run.rs:21` | IC | — |
| §11.2 | node-less registration with signed objects | CLI `model_add` | IC | — |
| §11.2 | `ALREADY_REGISTERED` only after full identity; a different class over the same weight root must not be refused merely for the root | SDK `misaka/palw/sdk/src/tir_registration.rs:72`, CLI `model_add.rs:831`, `sdk.rs:396` still refuse by root (the 2026-08-28 mispairing guard); chain accepts (`g14_registration_e2e.rs:1570`) | MV | needs decision: §11.2 (Revised Draft text) conflicts with the SDK's mispairing guard |
| §18 | ≥90 % of `D_complete`, three published numbers | census | MV | measurement |
| §17 three-stage lifecycle as consensus states | "proposed semantic lifecycle" | — | NA | — |
| 2026-10-12 BeaconConformance/bootstrap type split | "提案" | — | NA | — |
| §17 acquisition gate | withdrawn (ADR-0177) | — | RJ | — |

## RFC-0013 — reproducible layouts and onboarding tools

| § | requirement | code evidence | class | plan |
| --- | --- | --- | --- | --- |
| §3 | exact layout records, `pack bind-class` without overwrite | `docs/evidence/rfc/0013-onboarding-tool-validation.json` (bind_class PASS) | IC | — |
| §5 | lazy independent ref2, no size SKIP | §8 real run (2 vectors, 8 positions) | IC | — |
| §9 | beacon-aware packs, atomic per-check completion records, resume | `beacon_run.rs:21`, `misaka/palw/sdk/tests/runtime_pack_beacon.rs:625,740` | IC | — |
| matrix | tiled range evaluator, Merkle index | `small/rfc1-13-9-items` (575d9b262) | IC | — |
| §5 / §7-2 | row/range ref2 for single tensors above worker RAM; real 512+ MiB runs | only the independent evaluator is tiled | MV | real-artifact measurement |
| §7 | release tests on real artifacts (resume, exhaustion, reorg of evidence) | — | MV | measurement |
| §4 / §6 | explicit target-snapshot/offline parity, durable stage records for resolve…observe | §8 lists them as "distinct acceptance work"; status "no runtime implementation" | NA | tool work, not adopted as a requirement |
| §9 JSON schemas | proposed tool-manifest schemas | "proposed" | NA | — |

## Implemented in this lane

1. **U11 — RFC-0006 R6-CELL closure refused by record, not by name.** `Params::validate_palw_cell_coverage_closure_v1` now checks the
   mirror, then (armed, not an isolated G14 genesis) requires a pinned `PalwCellClosureArmingRecordV1` for exactly this genesis and
   height: `PALW_CELL_COVERAGE_CLOSURE_ARMING_REQUIREMENTS_V1` (fold suite, node E2E, the blame-path review, fence-crossing drill),
   `PALW_CELL_COVERAGE_CLOSURE_ARMING_RECORDS_V1 = &[]`, `validate_palw_cell_coverage_closure_in_v1(records)`. Same form checks as the
   G14 records (`PalwG14ArmingRefusalV1`). Separate type, not a `PalwG14GatedFenceV1` variant: the G14 set implies G14-from-genesis
   arming and the court-policy completeness gate, neither of which applies to a refusal-only closure. Nothing is armed; no wire,
   kind, row or fork-id change (the fence was already in `palw_fences_v1`).
2. **RFC-0006 R6-MIGRATION 4 — N3 second lock.** `PALW_N3_GATED_REWARD_FENCES_V1` gains `palw_permissionless_panel_v1`,
   `palw_permissionless_panel_complete_v1`, `palw_permissionless_panel_retired_v1` (the V3 draw) and `palw_tir_shard_segment_v2`. Each
   is already blanket-refused by its own validator; this is REQ-N3-3's second lock. A test now also pins that every listed name is a
   fence `palw_fences_v1` reports (a misspelt name would gate nothing). "Probabilistic cells" has no fence in this binary.
3. **RFC-0010 release item 3 — processor reorg across the epoch freeze block.**
   `t12_a_reorg_across_the_freeze_block_unfreezes_and_refreezes_the_epoch_per_branch` (`consensus` crate,
   `virtual_processor/tests/t12_permissionless_panel_e2e.rs`): two branches fork below epoch 1's release (fork DAA 39; A crosses in two
   heartbeats, the lane is the DAA clock); R reorgs A → A2 → A across both freeze blocks and holds exactly each branch's PALW state and
   roots; each branch freezes the same set, dated by its own freeze block; the tip's carriage reloads after two reorgs.

## Open decisions (collected)

- RFC-0006: closure arming and the blame-path closure mode; R6-DRAW/R6-PRICE named values; allocation curve.
- RFC-0007: `PalwClaimChallengePolicyV2`, `d_src` and source caps, Q/G/error allocation, `N_max`, the R7-ECON 8 split and
  unavailable-beacon rule, BootstrapExact profile.
- RFC-0009: court-signer delegation / termination-evidence spec (F04).
- RFC-0010: production Panel beacon approval; objective seal/finality rule.
- RFC-0011 §11.2: drop the SDK/CLI shared-root refusal for a different class (vs. the 2026-08-28 mispairing guard)?
- RFC-0002: PALWQ361 logit scale for the fit guard.

## Validation (this lane)

- `cargo test --release -p kaspa-consensus-core --lib -- palw_beacon_bootstrap_v1 palw_cell_coverage_closure`: **17 passed, 0 failed**
  (new: `palw_cell_coverage_closure_v1::tests::req_cell_the_closure_is_refused_until_a_record_for_this_network_and_height_is_pinned`,
  `palw_beacon_bootstrap_v1::round3_tests::n3_every_gated_name_is_a_reported_fence_and_rfc0006_new_profiles_are_locked`).
- `cargo test --release -p kaspa-consensus --lib t12_permissionless_panel_e2e -- --test-threads=1`: **7 passed, 0 failed, 2 ignored**
  (the 100k resource drills) — new: `t12_a_reorg_across_the_freeze_block_unfreezes_and_refreezes_the_epoch_per_branch`.
- No persisted/carried struct changed, so `gen_rfc06_native_wire.py` was not needed. No new wire tag, kind, aux table or carriage tail.
- Docs: `generate_indexes.py` run; `check_docs.py` PASS (no new regressions).
