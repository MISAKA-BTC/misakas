# RFC gap survey — lane rfcgap-c (court family), 2026-10-22

Worktree `MISAKA-rfcgap-c`, branch `rfcgap/c`, base `b1eb33960` (the integration of about 35 G14/RFC lanes).
Assigned: RFC-0014, 0023, 0024, 0025, 0026 (with T01–T15), and 0028–0035.

Method: for each RFC I read the adoption status line, the RFC-0026 master contract, the RFC-0014 2026-10-19 amendment, the RFC-0023
stages, and every lane note in `docs/evidence/palw/g14-parallel-2026-10-22/`. I then followed each requirement's symbol to its
production caller (the fold, the processor and the validation). Some "no caller" findings come from grep; differently named code
could have been missed.

Overlap: the running lanes `audit/fix-a` and `audit/fix-b` were read at `20ecdddb2` and `7a67bb041`. Both branches still hold only
reproducers. The areas they will change are excluded below and marked **[fix-a]** or **[fix-b]**:
- escrow with several inputs (A-2);
- the text29 held-Final reserve (A-1);
- the mining39 demand refund (A-3);
- silent bound sessions (B-04);
- pruning of an Uncaptured archive (B-05);
- binding F02 attribution to the turn (B-03);
- the F02 guard on the V3 row and the D4 decoy (B-01/B-02);
- admission dependency horizons (B-06).

## Status classes

| class | meaning |
|---|---|
| IC | implemented and connected on a production path. Dormant behind a fence that validation refuses until its record or value exists. |
| TO | implemented, but reached only from tests |
| DR | dormant: refused because a user value or measurement is missing (typed-missing) |
| MS | missing |
| ND | needs measurement or a user-approved value only |
| NA | not adopted (Draft or Proposed). If code exists, it is an isolated dormant candidate. |
| OV | being changed by an audit fix lane, so not touched here |

## RFC-0014 (adopted parts: §16 of 2026-10-10, and the 2026-10-19 amendment A–G)

| § | requirement | code evidence | class | plan |
|---|---|---|---|---|
| 10-19 B | Notice window `a+3,120` and hard deadline `a+10,000`, both from the original's acceptance; a new filing does not reset them | `palw_court_clock_v2.rs:49-137` (`governs`, `normal_hard`, `extended_hard`, `palw_dispute_record_hard_deadline_v2`); read by the legacy filer, staged court and capacity folds | IC (fence `palw_court_clock_v2`) | — |
| 10-19 B | One atomic extension moves `collect_until`, the disclosure duty, retention, the Final hold and the bond release | Dispute-record creation in `palw_legacy_public_filer_fold_v1.rs`; route doors via `palw_route_door_open_v2` (`palw_staged_court_fold_v1.rs:2268…3174`, `palw_tir_public_fold_v1.rs:1286,1471`) | IC | Two liveness reads still read the signed door: `palw_state_v2.rs:2839` (`palw_lock_is_committed_v1`) and `palw_legacy_public_filer_fold_v1.rs:457`. This is not a gap. The extension re-keys the seat locks and `panel_liabilities.expiry_daa` through `da_rekey_v1` (`transition_builder.rs:2777`), so the disjuncts that decide liveness (`is_live_v3`, `row.expiry_daa`) already carry the extended horizon. The staged signed door is an additional OR term, never the binding one. |
| 10-19 B / F07 | Dependency contracts cover `a+10,000` at acceptance | `palw_court_clock_v2_dependencies_cover_v2` (`:248`). EXEC worker and provider lease horizons are not state-reachable. | OV [fix-b B-06] | skip |
| 10-19 C | The outcome follows the response duty: an unanswered final turn is a challenger-side loss, never a Miner slash | existing session backstop. Position round: the responder no-show (`r6-p2-shared`) | IC | — |
| 10-19 D | Disputes on the same issue are merged; the outsider gets the same route; per-turn deadlines are kept | `Coalesced` (`palw_court_shared_tir_fold_v1.rs:61-64`); outsider lane `legacy_court_cap_admits_v1`; turn deadlines unchanged | IC | — |
| 10-19 E/F1 | Reserved court capacity and limits on admitting new claims | `palw_court_capacity_v1` (placeholder policy) | DR / ND (values) | — |
| 10-19 E/F2 | Low response fee; the challenger bond pays for court resources | ingress fee `palw_court_ingress_fee_due_v1` (typed-missing schedule) | DR | — |
| 10-19 E / RFC-0026 §3.2 | **Advance reservation: admit a new claim only while its outside reservation exists (`palw_court_ingress_claim_admits_v1`)** | was exposed only. The lane note said: "Not yet called by the attempt admission path" | **TO → fixed** | **implemented now (commit 1)** |
| 10-19 E/F4 | A third party may relay a signed response | `SignedSharedDataResponseV1` (current key, network-bound); test `r5_court_response_uses_the_current_key_retiring_answers_and_a_relay_carries_it` | IC | — |
| 10-19 E/F5-6 | Missing supply is not a computation fault. Default only when a recorded opportunity existed. | `attribute_service_default_v1` (`palw_court_service_binding_v1.rs:181`) | DR (`PALW_COURT_SERVICE_PROFILE_V1 = None`) + OV [fix-b B-03] | — |
| 10-19 F7 / G | Node tests under flooding, censorship, partition and reorg | — | ND | — |
| §16.1–16.3 | The chain does not take part in model distribution; model identity is fixed | Torrent/FPR are not implemented (withdrawn) | IC (by absence) | — |
| §16.4 | Court demands only claim-scoped units, never model bytes | `palw_court_scope_v1`, `palw_provider_court_v1` (kernel-route lane U04) | IC (record-gated) | — |
| §16.5–16.7, §16.11–12 | Per-model principal `S_m`, `A_m = f(S_m)` allocation, joint caps | `palw_bond_budget_v1.rs` (ADR-0177) | ND (f, epoch, seasoning) | — |

## RFC-0023 (Accepted, ADR-0186)

| § | requirement | code evidence | class | plan |
|---|---|---|---|---|
| §3.1–3.2, §7 | Per-claim bound `L_class`; a claim past its bound no longer holds the prefix, earns no credit, is not slashed, and its evidence stays | `palw_unresolved_bound_v1.rs:55` (`palw_resolved_through_v1`), certification fold, rewards | IC; arming refused (`:81`) | ND (lag measurement, drill) |
| §3.3–3.5 | Separate measurement of Final, certification, release, payout, `settled` and native safe; sustained-attack lag | `rfc23s1_per_cause_steady_state_lag…` (simulated) | ND | — |
| §4.1 | A checkpoint that binds the unresolved set | `palw_claim_local_checkpoint_fold_v1.rs:158` (row `144/'C'`, per block, import check) | IC (fence `palw_claim_local_settlement_v1`, refused) | — |
| §4.2 | Dependency is per claim and its inputs | `inputs` is empty: there is no map from transaction output to claim; dependency = shared execution root | MS (needs design) | needs a decision: the shape of the transaction-output → claim consumption map (RFC-0023 §4.5 says it changes together with settlement) |
| §4.3–4.5 | Irreversible credit; fork choice, settlement, pruning and IBD change together | the claim-local frontier replaces `safe_frontier` past the fence; RPC `claim_local_settlement` | IC (dormant) | — |
| §6 | Multi-node campaign | — | ND | — |

## RFC-0024 (Draft) and RFC-0025 (Draft, survey only)

| item | code evidence | class |
|---|---|---|
| 0024 A position round / B lookahead / multiproof (tags 201–204, fence `palw_tir_bounded_door_v1`, ADR-0190 Proposed) | `palw_tir_position_terminal_v2.rs`, `palw_tir_dissect_v1.rs` (lanes r5-p2-terminal, r6-p2-shared) | NA (isolated dormant candidate) |
| 0024 C prefetch / forced disclosure; door ≤ 3,200 | not built; `maximum_door_daa = 3,120` stays | NA |
| 0025 congestion extension `E_i` ≤ 6,800, cross-model reservation calendar, SC64/32 KiB close, block-capacity growth, 10,000-DAA checkpoint pruning | no code; RFC-0026 §2.1 limits `E_i` to inside the hard deadline | NA (nothing implemented, by instruction) |

## RFC-0026 (Draft master contract; its §2.1 clock restates the adopted RFC-0014 10-19)

| F / § | code evidence | class | plan |
|---|---|---|---|
| F01 §2.1 clocks | `palw_court_policy_v1.rs`, `palw_court_clock_v2.rs` | IC | — |
| F09 §2.2 boundary table (form only) | ADR-0198 A inclusive notice; `collect_until` exclusive; no shared table object | ND (values not decided) | — |
| §3.1 reservation of all branches at acceptance | `derive_funded_admission_v1`, `coordinate_funded_original_v1` (`:158`); the schedule is `#[cfg(test)]` | DR | — |
| §3.2 limit new liabilities at admission | commit 1 (see RFC-0014 E above) | IC after this lane | — |
| F02 §4 | see RFC-0014 E5-6 | DR + OV | — |
| F03 §5 | outsider lane + paid ingress + reservation; arming refuses `OneOutsiderCapacityIncomplete` (`palw_g14_gates_v1.rs:498`) | DR | Sybil-on-target residual is [fix-b "outsider Sybil"] |
| F04 §6 designated court signer | not built (would change the `PalwBondStateV2` layout) | ND (design decision) | needs a decision: delegated responder key, or a termination path with no added signature |
| F05 §7.1 SC64 | — | ND (decision) | — |
| F06 §7.2 keep the numerical fold check | checked in every round (unchanged) | IC | — |
| F08 §4.3 effects per end reason | RFC-0030 archive | see RFC-0030 | — |
| T06 6,880 residual | `palw_court_clock_v2_t06_v2` (`:418`), wired into `validate_palw_court_clock_v2` | IC | — |
| T01–T15 | `consensus/src/.../g14_t_campaign/`: the active tests run. The `#[ignore]` tests use guessed lane APIs behind the feature `g14-t-campaign-merged`, and they need `#[cfg(test)]`-only core schedules that the `kaspa-consensus` crate cannot inject. | TO / ND | skip: blocked on the schedules (fee, admission, service profile), which are user values |

## RFC-0028 – RFC-0035 (Draft/Proposed; isolated implementation of A or C1 user-approved for 0030, 0031, 0032, 0033 C1, 0034 S1 and 0035 A)

| RFC | requirement | code evidence | class | plan |
|---|---|---|---|---|
| 0028 | Response-eligibility history (duty clock); RFC-0028 binding installed at admission | `palw_court_duty_clock_v1.rs`; `install_court_service_binding_v1` (`palw_court_service_lifecycle_fold_v1.rs:33`) | NA, isolated, connected | — |
| 0029 | Owner-independent shared query and data; lifecycle on the wire | tag 178 `SharedCourtOpV1` → `apply_shared_court_wire_op_v1` (`:158`) | NA, isolated, connected | — |
| 0030 A | Court-end archive (key 20 / row 26); per-reason effects; pruning only when everything is resolved and past `material_until` | `palw_court_end_archive_v1.rs`, `palw_court_archive_verify_v1.rs:178,218` | NA, isolated, connected; Uncaptured is OV [fix-b B-05] | — |
| 0031 A | `a <= d <= a+3,120` initial notice; later requests bound to it, with no reset | `classify_shared_notice_v1` (`palw_court_service_binding_v1.rs:237`) used in `palw_court_shared_tir_fold_v1.rs:71`; index `court_shared_initial_notice_daa_v1` | NA, isolated, connected | — |
| 0032 A | Authenticated terminal material, non-exclusive ingress over several blocks | tag 255 `MaterialIntakeV1` → `material_intake` (`palw_court_shared_material_intake_fold_v1.rs:34`) | DR (storage schedule typed-missing). Semantic tables are not intake targets. | needs a decision: bound for semantic tables, storage price and payer |
| 0033 C1 | Tag 230 root registry; 231/232 demand and answer | `record_d2_readiness_root_v1` (`palw_state_v2/d2_readiness.rs:44`), `palw_d2_readiness_root_admit_v1` | DR (`PALW_D2_READINESS_DRAW_CAP_V1 = None`) | — |
| 0034 S1 | Timely real acceptance discharges the archived nominal liability, once only | `ArchiveDeclarationProgressV1`, `archive_pending_nominal_v1` (court-archive-terminal lane) | NA, isolated, connected | — |
| 0035 A | Real UTXO-backed escrow; conservation; settlement paths | `palw_funded_escrow_v1.rs:436`; fee schedule typed-missing | DR + OV [fix-a A-2] | — |

## Implemented in this lane

1. RFC-0014 2026-10-19 E / RFC-0026 §3.2 (F03): `palw_court_ingress_claim_admits_v1` now applies at every original admission
   path, not only in tests:
   - `apply_attempt`, before its first write. A refusal is `CourtClaimAdmissionFull`, which is skipped like the capacity bucket.
   - the producer pre-check `palw_court_claim_admission_v1`.
   - the RFC-0035 funded install, through `court_ingress_claim_admits_excluding_v1`, which does not count the claim's own row.

   It stays dormant: the ingress schedule is typed-missing in every non-test build. No new fence, wire tag, row or value.
   Test: `rfcgap_c_ingress_claim_admission_bounds_in_flight_claims_in_the_attempt_fold`.

## Open decisions

- The design of the transaction-output → claim consumption map (RFC-0023 §4.2).
- The F04 court-signer profile.
- SC64 (F05).
- The F09 boundary values.
- The RFC-0032 bound and price for semantic tables.
- Every typed-missing value: service profile, ingress fee, admission schedule, D2 draw cap, post-Final default slash, escrow fee schedule.
