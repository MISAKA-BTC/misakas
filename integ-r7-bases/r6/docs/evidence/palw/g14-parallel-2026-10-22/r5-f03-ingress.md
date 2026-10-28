# Lane r5-f03-ingress — G14 item 3: paid public court ingress (2026-10-22)

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/r5-f03-ingress`, base `64bc87dcd`.

Goal (G14 item 3 / RFC-0026 F03): one outsider completes disclosure → localization → verdict even when colluders hold and keep
refilling every slot they can reach. Scope citations: ADR-0192 (owner-independent frontier, advance reservation), ADR-0199 (no
first-come exclusive occupancy), RFC-0032 A (material intake, user-approved isolated implementation), court-owner lane rounds 1–2.
RFC-0025 is Draft: none of its numbers are used.

## Allocated consensus object tag: **255** (`PalwConsensusObjectV2::MaterialIntakeV1`)

Highest unused tag below 256 (used near the top: 245, 244, 243, 242, 241, 240, 230; the delta enum's 235 is a different enum). Chosen
to avoid the parallel lane r5-f02-wire. Owner fence: **`palw_staged_tir_route_v1`** (`PalwLifecycleKindFenceV1::StagedTirRouteV1`,
the shared TIR court's G-10 fence — no new fence). Row added to `PALW_LIFECYCLE_NEW_KINDS_V1`.

## 1. Material intake wiring (tag 255)

| site | change |
| --- | --- |
| `consensus/core/src/palw_state_v2.rs` | variant `MaterialIntakeV1 { intake: Box<SignedMaterialIntakeV1> } = 255`; mod comment (fold now dispatched) |
| `palw_lifecycle_objects_v2.rs` | may-ride shape rule (signature present; a Part is 1..=16 KiB, `PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1`); owner arm → `StagedTirRouteV1`; new-kinds row `(255, "MaterialIntakeV1", StagedTirRouteV1)` |
| `palw_heartbeat_carriers_v1.rs` | `MaterialIntakeV1 => false` (not an H-1 obligation) |
| `palw_state_v2/objects.rs` | fold dispatch → `palw_court_shared_material_intake_fold_v1::material_intake` (atomic; refusals are errors) |
| `consensus/src/.../processor.rs`, `palw_validate.rs` | name; routed to the court validator |
| `consensus/src/.../palw_validate/court.rs` | A-2: unjudged below the owning fence (`kind_in_force_at`); past it: registered Active bond + ML-DSA-87 signature over the network-bound step (`SignedMaterialIntakeV1::verify_v1`, network domain = `palw_network_domain_v2`, the same value the fold's `tir_public_network_domain_v1` extra carries) |
| `kaspad/src/palw_panel.rs` | object name |

Mempool/template: lifecycle payloads enter through the generic A-2 path (may-ride + kind fence + acceptance validator); no per-kind
template list exists, so nothing else was needed. The fold's existing gates still apply past the fence: the typed-missing storage
schedule (`material_intake_schedule_v1`) refuses every intake in production, and no `CourtSharedContext` installer exists.

## 2. Paid fair ingress (`palw_court_capacity_fold_v1.rs`, new `palw_court_paid_ingress_v1.rs`)

Gate: existing fence `palw_court_outsider_lane_v1` (court-owner lane) **and** `palw_court_ingress_schedule_v1()` —
`PalwCourtIngressScheduleV1 { fee_sompi_per_session }`, typed-missing (`PalwCourtIngressMissingV1::Schedule`) in every non-test
build; `#[cfg(test)] set_palw_court_ingress_test_schedule_v1`. Schedule missing ⇒ byte-for-byte the court-owner lane rule.

- `court_ingress_admission_v1` (fold; replaces the admission match at both session-opening sites: `CourtOpened` in `objects.rs`,
  `open_dissection_at_named_leaf_v1` in `da_sessions.rs`): calls `court_admission_v1` (the existing G-07 deadline-aware queue past
  `palw_court_capacity_v1` — queued honest prosecution granted first, unchanged), then on `Legacy` the paid cap (or, schedule
  missing, `legacy_court_cap_admits_v1`). **Fee charged once**: on the legacy path at the session's admission; past court capacity
  only when the prosecutor's court row is created (`!had_row`), never again when its ticket opens it. A refused try is not charged
  (object atomic). Burned via `slash_bond` (refused, not clamped, if the bond cannot pay) — same sink as the court occupancy fee.
- **Advance reservation (ADR-0192), court-owner risk 1**: `court_ingress_reserved_claims_v1` — in-flight claims (accepted at/past the
  lane fence, reservable phase, hard deadline `palw_claim_court_reserve_hard_v2` not passed), admission order `(accepted_daa, id)`,
  first `R = ⌊cap/2⌋` (`palw_court_ingress_reserved_claims_cap_v1`). Each holds ONE outside-side slot.
  `paid_court_cap_admits_v1`: a claim's first OUTSIDE-side session (opener neither producer nor panel seat, the round-2 identity read)
  opens on its own claim's reservation; every other session competes for `cap − #reserved` (reserved slots count as taken whether
  used or not), then the follow-on share. So a coalition filling the first-session room with its own distinct claims cannot take the
  target claim's slot: each claim's reservation is its own and only the number of claims is bounded.
- **Function for the admission coordinator**: `palw_court_ingress_claim_admits_v1(state, params, class_id, now)` (re-exported from
  `palw_state_v2`): refuses a new claim (`CourtClaimAdmissionFull { bucket: 0, cap: R }`) while `R` claims are in flight; `Ok` when
  dormant. **Not yet called** by the attempt admission path (the coordinator lane must call it; until then claims beyond `R` simply
  hold no reservation — no safety change, the guarantee holds only for the first `R`).
- `palw_court_ingress_fee_due_v1(params, now)` (re-exported) for node filers.

Design decisions (flag for review):
- `R = ⌊cap/2⌋` is structural (derived from the unchanged cap), not an adopted number.
- Every session opened through the paid path pays the fee (inside and outside alike) — the ingress is a public, role-blind price.
- A direct evidence-carrying accusation stays exempt from the follow-on share (round 2) but **not** from the reserved room;
  otherwise cone accusations could fill the reserved slots. This means past fence+schedule a cone accusation can be refused where
  round 2 admitted it (only when reserved slots are unused).

Residual (honest): a Sybil (outside-side, not producer/seat) can take the TARGET claim's own reservation first and stall; the honest
outsider is then a follow-on until that session ends (by its rung/backstop, at the Sybil's cost: fee + accuser stake). The delay per
decoy is bounded by the session's rung, but a chain of Sybils is bounded only economically (fee × count), not structurally — the
lane goal is "uneconomic", per the 10-14 final goal. A structural fix would need ≥2 reserved outside slots or a priority among
outside openers (unadopted design).

## 3. Held attention accusation (verified)

`open_held_dissection_v1` (`palw_state_v2.rs` ~15994) passes `direct_evidence = false`: the held attention accusation stays
subject to the follow-on share (decoy protection). An outsider's held accusation on a claim whose only sessions are the producer's /
seats' is the outside side's first session (round-2 `court_outsider_lane_is_followon_v1`). Test added (below).

## 4. Stage28 lifetime cap 4 (`palw_flat_stage_fold_v28.rs::owner`)

Same stalled-owner reclamation as text29 (court-owner round 2), past `palw_court_outsider_lane_v1`, cap stays 4:
`stage28_owner_catalogue_admit_v28` (pure), `stage28_owner_made_no_progress_v28` (pure on the seal: namespace 28, retired, past
horizon, no part/accept/assembly/court/conversion), `stage28_owner_reclaimable_v28` (caller row 26 retired, bound to the paid
receipt, nothing remaining, `source28::pending_owned == 0`). On eviction: `status.owners_admitted -= 1` before the admission token,
catalogue entry replaced, the evicted owner's `(29u8,job,id)` / `(26u8,job,id)` rows deleted (exactly what stage28 collection
checks per catalogued owner). The catalogue check moved BEFORE the admission token (as in text29); below the fence the refusal text
is unchanged, but when several refusals coincide the one reported may differ (outcome identical: refused).
Uncertain (same as text29): any reader of an evicted owner's archival seal after eviction (grep found collection/expiry only).

## 5. Node (kaspad)

`--palw-court-paid-ingress` (args.rs) → `PalwPanelConfig::court_paid_ingress` (daemon.rs; off in drill clones and e2e configs).
`palw_filer_held.rs`: `PalwHeldFilerHostV1::{paid_ingress_opt_in, ingress_fee_due}` (default off/None; the panel host reads the
config and `palw_court_ingress_fee_due_v1` on the bundle's state params); `palw_held_paid_ingress_step_v1` — fee due and not opted in
→ the case settles unpaid before filing; opted in → files (the fold burns the fee). Dormant in production (schedule missing ⇒ no fee
due ⇒ unchanged). Only the held dissection opener is gated; other filers (TIR public, GDN, DA) are not changed.

## Tests (written, not run)

`kaspa-consensus-core` lib:
- `palw_court_paid_ingress_v1::tests::r5_f03_ingress_schedule_is_typed_missing_until_injected`
- `palw_court_paid_ingress_v1::tests::r5_f03_reserved_claims_leave_non_reserved_room`
- `palw_state_v2::tests::r5_f03_colluders_refill_every_slot_and_one_outsider_reaches_a_verdict_in_time` — **fold-level E2E**
  (`apply` + both consistency checkers): lane armed, schedule injected, cap 8 / R 4; coalition (producer+seat bond 1) opens one court
  a block on every decoy/space until refused; refills refused; outsider opens on the target's reservation, pays the fee once;
  `CourtClosed ExecutorGuilty` inside the rung and hard deadline; target voided; no second charge.
- `palw_state_v2::tests::rcore_s1_api::court_outsider_lane_round2::`
  `r5_f03_held_attention_outsider_on_a_colluder_only_claim_counts_as_first_session`,
  `r5_f03_colluders_refilling_every_slot_cannot_take_the_target_claims_reservation` (4 refill rounds, Sybils on decoys, admission
  bound), `r5_f03_the_ingress_fee_is_charged_once_and_never_for_a_refused_try` (through `court_ingress_admission_v1` on a builder).
- `palw_state_v2::palw_flat_stage_fold_v14::next28::g14_r5_f03_stage28_reclaim_tests::` (filter `r5_f03_stage28`) — three tests.
- `palw_lifecycle_objects_v2::tests::r5_f03_material_intake_kind_255_is_owned_tagged_and_shaped`.

`kaspad` lib: `palw_panel::…::palw_filer_held::tests::r5_f03_the_filer_pays_the_court_ingress_only_when_opted_in` (filter `r5_f03`).

Run: `cargo test -p kaspa-consensus-core r5_f03` and `cargo test -p kaspad r5_f03`. Also re-run the A2U/lifecycle sweeps
(`every_kind_in_the_enum_has_exactly_one_owner`, `the_new_kind_table_is_the_owner_function`, `t12_a2u_new_kinds_uniform`) and the
court-owner tests (`g14_f03`), and `material_intake`.

## Fences needed armed (centrally, t12 DAA 16000)

- `palw_court_outsider_lane_v1` (court-owner lane; needs LG14-A at or below it) — for the paid ingress, the reservation and stage28
  reclamation. The ingress additionally needs the fee schedule ADOPTED (user decision: amount) — otherwise it stays dormant.
- `palw_staged_tir_route_v1` — owner of tag 255; intake additionally needs the storage schedule adopted and a shared-context installer.

## Central step required before it compiles

- `palw_native_resource_v1/generated.rs`: `native_enum!(PalwConsensusObjectV2; …)` must gain `MaterialIntakeV1 { intake }` (plus the
  material-intake row types from lane material-intake, already listed in its notes).
- `fork_id_v1.rs` / probe maps: nothing new (no new fence).

## Open / uncertain

- Not compiled: E2E assumptions — several `attempt(40, n)` of one bond in one epoch all admitted; one `CourtOpened` per block;
  `CourtOpened` changes the opener's `collateral` only by the ingress fee; the unarmed twin admits all 8 coalition courts.
- The admission coordinator does not yet call `palw_court_ingress_claim_admits_v1` (exposed only, per task).
- User decisions: fee amount (schedule), whether inside openers should pay, whether `R = ⌊cap/2⌋` is right, Sybil-on-target residual.
