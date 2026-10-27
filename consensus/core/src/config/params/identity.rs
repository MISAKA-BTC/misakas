//! **`Params`: identity methods** — moved out of `params.rs` (2026-10-13, a behaviour-preserving split: the methods are the same; each keeps the reach it had).

use super::*;

impl Params {
    /// **A value that only takes effect AT a fence must leave the identity with it** (audit3 H1).
    ///
    /// Normalising the fences is not enough on its own, because some fences cannot be armed alone.
    /// `with_registered_models` — which wraps BOTH `From<NetworkType>` and `From<NetworkId>`, so it
    /// is what every node actually runs — installs `ModelCostTable::palw_metal_registered()` the
    /// moment `vlt_shadow_activation_daa_score` stops being `u64::MAX`, and
    /// `shipped_presets_are_either_dormant_or_fully_forkable` fails the BUILD on a preset that
    /// schedules the fence over an empty table. `consensus_params_id` hashes `DnsParams` as one
    /// borsh blob, so the table is inside the identity — and the fence was normalised out of it
    /// while the four `ModelCostEntry`s it drags along were not.
    ///
    /// The consequence is precisely the one M1-6 exists to remove, on the one upgrade this file
    /// documents as the next mainnet fork: operator A schedules the VLT shadow fork at a FUTURE
    /// height H, its identity differs from every un-upgraded peer's, and `flow_context` refuses the
    /// connection outright. The mesh splits into two disjoint sets that agree about every block
    /// either can produce today — H is still in the future — until the last operator crosses over.
    ///
    /// The R-2 regression test misses it because it builds its VLT case from the raw const, so
    /// `with_registered_models` never runs and both sides keep the empty table. That is the trap
    /// `shipped_presets_have_pinned_fingerprints` records two hundred lines below it: pin the
    /// MATERIALIZED value, because the const is a number no node ever reports.
    ///
    /// The same shape applies to `TokenParams`: its fences cannot be armed without also freezing
    /// the emission constants that are `TBD` while it is inert, and those are not fences either.
    pub(super) fn normalize_values_a_scheduled_fence_drags_with_it(&mut self) {
        // **The TIR frame cost model is a value its fences drag with them** (lane ACT2, 2026-10-27). It has no height of its own;
        // only `palw_tir_public_conviction_v1` and `palw_tir_bounded_door_v1` read it. Where neither is active at genesis, a build
        // that carries the value for a SCHEDULED fence must keep the identity of a build without it, or a release scheduling the
        // frame at a future height would split from the fleet at deploy. Past the fence the value is in force and hashed in the
        // params / schedule ids as before.
        let at_genesis = |f: Option<ForkActivation>| f.is_some_and(|f| f.is_active(0));
        if !at_genesis(self.palw_tir_public_conviction_v1) && !at_genesis(self.palw_tir_bounded_door_v1) {
            self.palw_tir_frame_cost_v1 = None;
        }
        // **ADR-0064's fence must collapse to absent, not to a present `never()`.**
        //
        // `for_each_fence` has already run, so an armed height H has been rewritten to
        // `Some(ForkActivation::never())` by the identity visitor. Left like that,
        // `consensus_params_id` writes bytes for a `Some` that a build without the field (or with
        // it unset) never writes, the identities differ, and the handshake refuses — which is
        // precisely the deploy-day partition the top-level placement exists to avoid. Collapsing it
        // here restores the intent: two builds that disagree only about a FUTURE height are one
        // network. `Some(always())` survives, because a fence active at genesis is a real rule
        // difference and must separate identities.
        //
        // Deliberately ahead of the `dns_params` early return below — a network with no DNS overlay
        // still has this fence.
        if self.palw_bootstrap_activation == Some(ForkActivation::never()) {
            self.palw_bootstrap_activation = None;
        }
        // **ADR-0062 SA-6's horizon is a value a SCHEDULED fence drags with it** (testnet-11's
        // second flag day, 2026-09-06).
        //
        // `with_palw_v2_depths` widens `pruning_depth` by `accuse + disclose` where `palw_da_court`
        // is set, because a claim whose DA phase outlives the horizon is a claim whose anchor
        // header the pruning processor has deleted. The depth is not a fence — it is hashed into
        // `consensus_params_id` directly — so a build that SCHEDULES the court would otherwise
        // carry a different identity from one that does not, and two nodes that disagree only about
        // a future height must stay peers or a live fleet cannot roll one host at a time. Every
        // height below the fence is judged identically either way, which is what makes normalising
        // honest; past it the wider horizon is in force and every node must be on the new build,
        // exactly as with any other scheduled fence.
        //
        // Only where the fence is SCHEDULED: a preset that never arms it has a horizon this
        // derivation may not touch (devnet's inherited floor is wider than its own derivation, and
        // re-deriving would narrow it and move an identity for no reason). The re-derivation is
        // exact exactly where it runs — `the_v2_pruning_depth_is_the_derivation_on_every_preset_that_schedules_the_court`
        // proves the inherited floor does not bind on those presets.
        if self.palw_da_court.is_some_and(|f| !f.is_active(0)) {
            self.palw_da_court = None;
            if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
                self.blockrate.pruning_depth =
                    palw_v2_pruning_depth_v1(&self.blockrate, bundle, None, self.palw_class_verify_deadline);
            }
        } else if self.palw_da_court == Some(ForkActivation::never()) {
            self.palw_da_court = None;
        }
        // ADR-0065 D4, same shape and the same reason.
        if self.palw_unavailable_abstains == Some(ForkActivation::never()) {
            self.palw_unavailable_abstains = None;
        }
        // ADR-0065 D1. The WHOLE option collapses, not just its fence: leaving `Some` behind with
        // a normalised height would carry the window's bytes into `consensus_params_id` that an
        // unset build never writes, which is the partition the top-level placement exists to avoid.
        if self.palw_bond_maturity.is_some_and(|m| m.activation == ForkActivation::never()) {
            self.palw_bond_maturity = None;
        }
        // Lane maturity (post-launch, 2026-09-26): D1 brought forward is a bare fence hashed
        // Some-only, so the same collapse — without it a build that schedules the fence and one that
        // does not would refuse each other on deploy day over a height neither has reached.
        if self.palw_bond_maturity_early == Some(ForkActivation::never()) {
            self.palw_bond_maturity_early = None;
        }
        // ADR-0065 D2, a bare fence: same collapse, same reason.
        if self.palw_frontier_provenance == Some(ForkActivation::never()) {
            self.palw_frontier_provenance = None;
        }
        // lane: rcore/f1-forkchoice-attacks, a bare fence: same collapse, same reason. Without this
        // a scheduled `never()` writes "palw_reorg_strict_economic_win" + u64::MAX into
        // consensus_params_id while a build that never armed it writes nothing, and the two
        // identities split — the Some-only fingerprint above is only safe with this collapse.
        if self.palw_reorg_strict_economic_win == Some(ForkActivation::never()) {
            self.palw_reorg_strict_economic_win = None;
        }
        // lane: rcore/hf-pptake2, a bare fence: same collapse, same reason. Without this a scheduled
        // `never()` writes "palw_pruning_proof_strict_economic_win" + u64::MAX into consensus_params_id
        // while a build that never armed it writes nothing, and the two identities split — the
        // Some-only fingerprint below is only safe with this collapse.
        if self.palw_pruning_proof_strict_economic_win == Some(ForkActivation::never()) {
            self.palw_pruning_proof_strict_economic_win = None;
        }
        // lane: rcore/cap-weight (ADR-0160 F-W), a bare fence: same collapse, same reason
        // (a-some-only-fence-needs-its-never-collapse).
        if self.palw_capacity_weight_cap == Some(ForkActivation::never()) {
            self.palw_capacity_weight_cap = None;
        }
        // ADR-0066 Decisions 1 and 4. Both carry a value beside the fence, so both take the whole
        // option — the D1 rule, for the D1 reason. The value does not vanish: ADR-0066 SA-4 folds
        // it into [`Self::consensus_schedule_id`], which is reported and not gated, so the
        // disagreement two operators can have about it is visible without costing them the rolling
        // deploy this collapse exists to buy.
        if self.palw_heartbeat.is_some_and(|h| h.activation == ForkActivation::never()) {
            self.palw_heartbeat = None;
        }
        // ADR-0068 Phase 1: F2's fence, the D1 rule for the D1 reason.
        if self.palw_attempt_work.is_some_and(|w| w.activation == ForkActivation::never()) {
            self.palw_attempt_work = None;
        }
        // ADR-0072's fence, the D4 rule for the D4 reason: a bare `ForkActivation`, so the whole
        // option collapses rather than being left as a present `never()` that a build without the
        // field never writes.
        if self.palw_attempt_activation == Some(ForkActivation::never()) {
            self.palw_attempt_activation = None;
        }
        // ADR-0128: the WHOLE option collapses, the D1 rule for the D1 reason — a present `never()`
        // would carry the three leak numbers into `consensus_params_id` that an unset build never
        // writes, and scheduling the gate would partition the fleet at deploy.
        if self.dns_bft_gate.is_some_and(|g| g.activation == ForkActivation::never()) {
            self.dns_bft_gate = None;
        }
        if self.palw_inactivity_leak.is_some_and(|l| l.activation == ForkActivation::never()) {
            self.palw_inactivity_leak = None;
        }
        // ADR-0073 SA-1. The WHOLE option collapses, the D1 rule for the D1 reason: leaving `Some`
        // behind with a normalised height would carry `k`'s byte into `consensus_params_id` that
        // an unset build never writes, and the first operator to SCHEDULE the fold would be
        // disconnected from every un-upgraded peer at deploy rather than at the height.
        if self.palw_beacon_fold.is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_beacon_fold = None;
        }
        // ADR-0071 SA-1..SA-4, a bare fence: same collapse, same reason as D2 above.
        if self.palw_capability_bound == Some(ForkActivation::never()) {
            self.palw_capability_bound = None;
        }
        // ADR-0134, a bare fence: same collapse, same reason.
        if self.palw_compute_overlay_retired == Some(ForkActivation::never()) {
            self.palw_compute_overlay_retired = None;
        }
        // ADR-0135, a bare fence: same collapse, same reason.
        if self.palw_model_registry == Some(ForkActivation::never()) {
            self.palw_model_registry = None;
        }
        if self.palw_work_target == Some(ForkActivation::never()) {
            self.palw_work_target = None;
        }
        if self.palw_single_lottery == Some(ForkActivation::never()) {
            self.palw_single_lottery = None;
        }
        if self.palw_short_challenge_window == Some(ForkActivation::never()) {
            self.set_palw_short_challenge_window(None);
        }
        if self.palw_verification_v2 == Some(ForkActivation::never()) {
            self.palw_verification_v2 = None;
        }
        if self.palw_verification_s3 == Some(ForkActivation::never()) {
            self.palw_verification_s3 = None;
        }
        if self.palw_verification_s2 == Some(ForkActivation::never()) {
            self.palw_verification_s2 = None;
        }
        if self.palw_readiness_v2 == Some(ForkActivation::never()) {
            self.palw_readiness_v2 = None;
        }
        if self.palw_anchor_clock == Some(ForkActivation::never()) {
            self.palw_anchor_clock = None;
        }
        // ADR-0142's cursor and ADR-0143's ownership fence: the same collapse for the same reason.
        // Both are hashed Some-only, so a normalised `Some(never())` writes bytes a build without
        // the fence never writes — and the two builds would refuse each other on deploy day over a
        // height neither has reached. ADR-0143's is the first of the pair that a shipped preset
        // actually arms, which is what turned this from a latent omission into a red test.
        if self.palw_clock_cursor == Some(ForkActivation::never()) {
            self.palw_clock_cursor = None;
        }
        // The clock floor: Some-only hashed like the cursor it refines, so the same collapse.
        if self.palw_clock_floor == Some(ForkActivation::never()) {
            self.palw_clock_floor = None;
        }
        // The lead cap: Some-only hashed, so the same collapse.
        if self.palw_clock_lead_cap == Some(ForkActivation::never()) {
            self.palw_clock_lead_cap = None;
        }
        // ADR-0152 v2 F2's attribution fence: Some-only hashed, so the same collapse.
        if self.palw_offence_attribution == Some(ForkActivation::never()) {
            self.palw_offence_attribution = None;
        }
        // ADR-0152 R-core+: Some-only hashed, so the same collapse.
        if self.palw_rcore_plus == Some(ForkActivation::never()) {
            self.palw_rcore_plus = None;
        }
        // Lane F1 (the panel seed, post-launch): Some-only hashed, so the same collapse — without it
        // a build that schedules the fence and one that does not would refuse each other on deploy
        // day over a height neither has reached.
        if self.palw_panel_seed_execution == Some(ForkActivation::never()) {
            self.palw_panel_seed_execution = None;
        }
        // Lane A (the operator anchor, post-launch): Some-only hashed (with its operator list), so the
        // same collapse — the list goes with the height, or a build that schedules the fence and one
        // that does not would refuse each other over a height neither has reached.
        if self.palw_operator_anchor.as_ref().is_some_and(|rule| rule.activation == ForkActivation::never()) {
            self.palw_operator_anchor = None;
        }
        // Lane V02 (a resolved claim's lock off the work ceiling, post-launch): Some-only hashed, so
        // the same collapse — a build that schedules it and one that does not share an identity until
        // the height.
        if self.palw_final_lock_full_collateral == Some(ForkActivation::never()) {
            self.palw_final_lock_full_collateral = None;
        }
        // Lane V02 (post-launch, the shortened post-Final lock life): Some-only hashed, so the same
        // collapse — a build that schedules it and one that does not share an identity until the height.
        if self.palw_final_lock_life == Some(ForkActivation::never()) {
            self.palw_final_lock_life = None;
        }
        // ADR-0160 lane liab (F-L): Some-only hashed (the value whole), so the same collapse — the
        // steps go with the height. A step whose own height the identity visitor normalised to
        // `never()` (a FUTURE flag day's appended step on a fence armed at genesis) leaves the value
        // too, so two builds that differ only in a step nobody has reached are one network.
        if self.palw_capacity_aggregate_liability.as_ref().is_some_and(|value| value.activation == ForkActivation::never()) {
            self.palw_capacity_aggregate_liability = None;
        }
        if let Some(value) = self.palw_capacity_aggregate_liability.as_mut() {
            value.steps.retain(|step| step.from_daa != u64::MAX);
        }
        // Lane F2-lock (post-launch, the lock life applied retroactively): Some-only hashed, so the same
        // collapse — a build that schedules it and one that does not share an identity until the height.
        if self.palw_final_lock_life_retro == Some(ForkActivation::never()) {
            self.palw_final_lock_life_retro = None;
        }
        // ADR-0152 §4-quater's class-verify-deadline fence: Some-only hashed, so the same collapse.
        if self.palw_class_verify_deadline == Some(ForkActivation::never()) {
            self.palw_class_verify_deadline = None;
        }
        // Lane bind-deadlock (post-launch): Some-only hashed, so the same collapse — without it a build
        // that schedules the fence and one that does not would refuse each other on deploy day over a
        // height neither has reached.
        if self.palw_anchor_at_ceiling == Some(ForkActivation::never()) {
            self.palw_anchor_at_ceiling = None;
        }
        // Lane accept-order (post-launch): Some-only hashed, so the same collapse.
        if self.palw_lane_accept_parents_first == Some(ForkActivation::never()) {
            self.palw_lane_accept_parents_first = None;
        }
        // ADR-0152-adjacent (Activation Pool): Some-only hashed, the carve's shape — the whole option
        // collapses, so the terms beside a never-armed fence leave the identity with it.
        if self.palw_activation_pool.is_some_and(|pool| pool.activation == ForkActivation::never()) {
            self.palw_activation_pool = None;
        }
        // The readiness-V2 horizon (user decision 2026-09-25): Some-only hashed, the pool's shape — the
        // whole option collapses, so the spans beside a never-armed fence leave the identity with it.
        if self.palw_readiness_v2_max_age_spans.is_some_and(|horizon| horizon.activation == ForkActivation::never()) {
            self.palw_readiness_v2_max_age_spans = None;
        }
        // Lane F1 (registry resilience, V03/V05): Some-only hashed, so the same collapse — a build that
        // schedules it and one that does not must share an identity until the height.
        if self.palw_registry_resilience == Some(ForkActivation::never()) {
            self.palw_registry_resilience = None;
        }
        // Lane F2 (the floor-refusal retry, post-launch): Some-only hashed, so the same collapse — a build
        // that schedules it and one that does not must share an identity until the height.
        if self.palw_floor_refusal_retry == Some(ForkActivation::never()) {
            self.palw_floor_refusal_retry = None;
        }
        if self.palw_artifact_root_ownership == Some(ForkActivation::never()) {
            self.palw_artifact_root_ownership = None;
        }
        if self.palw_operator_id_unique == Some(ForkActivation::never()) {
            self.palw_operator_id_unique = None;
        }
        if self.palw_objective_offence == Some(ForkActivation::never()) {
            self.palw_objective_offence = None;
        }
        if self.palw_seat_gate_possession == Some(ForkActivation::never()) {
            self.palw_seat_gate_possession = None;
        }
        if self.palw_class_receipt_window == Some(ForkActivation::never()) {
            self.set_palw_class_receipt_window(None);
        }
        if self.palw_execution_quanta == Some(ForkActivation::never()) {
            self.palw_execution_quanta = None;
        }
        if self.palw_public_model_source_required.is_some_and(|r| r.activation == ForkActivation::never()) {
            self.palw_public_model_source_required = None;
        }
        // ADR-0145's canonical-work fence: Some-only in the fingerprint, so the same collapse for
        // the same reason — a normalised `Some(never())` writes bytes a build without the field
        // never writes, and the two builds refuse each other on deploy day over a height neither
        // has reached.
        if self.palw_canonical_work == Some(ForkActivation::never()) {
            self.palw_canonical_work = None;
        }
        if self.palw_admission_independence == Some(ForkActivation::never()) {
            self.palw_admission_independence = None;
        }
        if self.palw_economic_payout.is_some_and(|payout| payout.activation == ForkActivation::never()) {
            self.palw_economic_payout = None;
        }
        // ADR-0077 Phase B, a bare fence: same collapse, same reason.
        if self.palw_context_ladder == Some(ForkActivation::never()) {
            self.palw_context_ladder = None;
        }
        // ADR-0077 Decision 16, a bare fence: same collapse, same reason. It matters twice here,
        // because `palw_panel_da_admissible` reads `is_some()` — a `Some(never())` left standing
        // would open the transaction door on a network whose rule can never come into force.
        if self.palw_panel_da == Some(ForkActivation::never()) {
            self.palw_panel_da = None;
        }
        // ADR-0075 SA-1/SA-2, a bare fence: the D2 collapse, for the D2 reason.
        if self.palw_certification_rent == Some(ForkActivation::never()) {
            self.palw_certification_rent = None;
        }
        // ADR-0069 Decision 7, a bare fence: the D2 shape, for the D2 reason.
        if self.palw_uncertified_weightless == Some(ForkActivation::never()) {
            self.palw_uncertified_weightless = None;
        }
        // ADR-0062 (SA-1…SA-6), a bare fence: same collapse, same reason.
        if self.palw_da_court == Some(ForkActivation::never()) {
            self.palw_da_court = None;
        }
        // ADR-0084 U-08, a bare fence: the same collapse, for the same reason.
        if self.palw_court_ladder == Some(ForkActivation::never()) {
            self.palw_court_ladder = None;
        }
        // ADR-0087 Decision 6, a bare fence: the same collapse, for the same reason.
        if self.palw_model_market == Some(ForkActivation::never()) {
            self.palw_model_market = None;
        }
        // ADR-0088 Decision 11, a bare fence: the same collapse, for the same reason.
        if self.palw_model_benefits == Some(ForkActivation::never()) {
            self.palw_model_benefits = None;
        }
        // ADR-0114, a bare fence: the same collapse.
        if self.palw_model_leg_v2 == Some(ForkActivation::never()) {
            self.palw_model_leg_v2 = None;
        }
        // ADR-0120, a bare fence: the same collapse.
        if self.palw_model_seed_v2 == Some(ForkActivation::never()) {
            self.palw_model_seed_v2 = None;
        }
        // ADR-0162, a bare Some-only fence: the same collapse, so `Some(never())` is absence.
        if self.palw_model_virtual_v1 == Some(ForkActivation::never()) {
            self.palw_model_virtual_v1 = None;
        }
        if self.palw_model_immutable_v1 == Some(ForkActivation::never()) {
            self.palw_model_immutable_v1 = None;
        }
        if self.palw_class_semantics_v1 == Some(ForkActivation::never()) {
            self.palw_class_semantics_v1 = None;
        }
        if self.palw_court_commitment_scope_v1 == Some(ForkActivation::never()) {
            self.palw_court_commitment_scope_v1 = None;
        }
        if self.palw_staged_public_court_v1 == Some(ForkActivation::never()) {
            self.palw_staged_public_court_v1 = None;
        }
        if self.palw_staged_tir_route_v1 == Some(ForkActivation::never()) {
            self.palw_staged_tir_route_v1 = None;
        }
        if self.palw_tir_public_conviction_v1 == Some(ForkActivation::never()) {
            self.palw_tir_public_conviction_v1 = None;
        }
        if self.palw_tir_container_identity_v1 == Some(ForkActivation::never()) {
            self.palw_tir_container_identity_v1 = None;
        }
        if self.palw_staged_float_constraint_v1 == Some(ForkActivation::never()) {
            self.palw_staged_float_constraint_v1 = None;
        }
        if self.palw_model_lines == Some(ForkActivation::never()) {
            self.palw_model_lines = None;
        }
        // ADR-0089 Decision 9, a bare fence: the same collapse.
        if self.palw_model_evm == Some(ForkActivation::never()) {
            self.palw_model_evm = None;
        }
        // Lane sink (the model sink binding, post-launch): Some-only hashed, so the same collapse —
        // without it a build that schedules the fence and one that does not would refuse each other
        // on deploy day over a height neither has reached.
        if self.palw_model_sink_bound == Some(ForkActivation::never()) {
            self.palw_model_sink_bound = None;
        }
        // ADR-0160 lane verify (F-B, F-R): Some-only hashed, so the same collapse, each.
        if self.palw_capacity_batch_licence == Some(ForkActivation::never()) {
            self.palw_capacity_batch_licence = None;
        }
        if self.palw_capacity_verify_room == Some(ForkActivation::never()) {
            self.palw_capacity_verify_room = None;
        }
        // ADR-0160 stage 2 (F-Q, F-S): Some-only hashed, so the same collapse, each.
        if self.palw_capacity_audit_door == Some(ForkActivation::never()) {
            self.palw_capacity_audit_door = None;
        }
        if self.palw_capacity_issuance_slots == Some(ForkActivation::never()) {
            self.palw_capacity_issuance_slots = None;
        }
        // ADR-0160 stage 4 (F-N): Some-only hashed, the same collapse.
        if self.palw_capacity_network_room == Some(ForkActivation::never()) {
            self.palw_capacity_network_room = None;
        }
        // int-11: the verification term, the same collapse.
        if self.palw_capacity_network_verify == Some(ForkActivation::never()) {
            self.palw_capacity_network_verify = None;
        }
        if self.palw_panel_unavailable_expiry == Some(ForkActivation::never()) {
            self.palw_panel_unavailable_expiry = None;
        }
        if self.palw_panel_fast_switch == Some(ForkActivation::never()) {
            self.palw_panel_fast_switch = None;
        }
        if self.palw_seat_availability == Some(ForkActivation::never()) {
            self.palw_seat_availability = None;
        }
        // ADR-0164 (stages 5–7): the three bare fences, the same collapse.
        if self.palw_capacity_emission_budget == Some(ForkActivation::never()) {
            self.palw_capacity_emission_budget = None;
        }
        if self.palw_capacity_multi_claim == Some(ForkActivation::never()) {
            self.palw_capacity_multi_claim = None;
        }
        if self.palw_capacity_rho_breaker == Some(ForkActivation::never()) {
            self.palw_capacity_rho_breaker = None;
        }
        // ADR-0075 D14, a bare fence: the D2 collapse, for the D2 reason.
        if self.palw_chunk_cap_charge == Some(ForkActivation::never()) {
            self.palw_chunk_cap_charge = None;
        }
        // ADR-0081 Decision 3, a bare fence: the same collapse for the same reason. It matters
        // here for the `palw_panel_da` reason too — `palw_prompt_ids_form_at` reads the fence, and
        // a `Some(never())` left standing would be a form nothing can ever select while still
        // writing its name into `consensus_params_id`.
        if self.palw_prompt_ids_merkle == Some(ForkActivation::never()) {
            self.palw_prompt_ids_merkle = None;
        }
        // ADR-0082 Decision 3, a bare fence: the same collapse for the same reason.
        if self.palw_kary_court == Some(ForkActivation::never()) {
            self.palw_kary_court = None;
        }
        if self.palw_model_court_window == Some(ForkActivation::never()) {
            self.palw_model_court_window = None;
        }
        // The fused-terminal responder-coverage rule, a bare fence: the same collapse for the same
        // reason — `never()` is absence, and absence must not write a name into the schedule id.
        if self.palw_court_responder_coverage == Some(ForkActivation::never()) {
            self.palw_court_responder_coverage = None;
        }
        // ADR-0082 Decisions 10/11, likewise.
        if self.palw_fp_decode_rules == Some(ForkActivation::never()) {
            self.palw_fp_decode_rules = None;
        }
        // ADR-0145 §5/§6's derived-work fence, likewise. It is hashed Some-only, so without this
        // collapse a normalised `Some(never())` writes bytes a build without the field never
        // writes, and the two builds refuse each other on deploy day over a height neither has
        // reached.
        if self.palw_fp_derived_work == Some(ForkActivation::never()) {
            self.palw_fp_derived_work = None;
        }
        // ADR-0096 Decisions 6–8, likewise.
        if self.palw_fp_decode_constraint == Some(ForkActivation::never()) {
            self.palw_fp_decode_constraint = None;
        }
        // ADR-0099 Decision 5, likewise.
        if self.palw_shard_court == Some(ForkActivation::never()) {
            self.palw_shard_court = None;
        }
        if self.palw_memory_routing_court_v2 == Some(ForkActivation::never()) {
            self.palw_memory_routing_court_v2 = None;
        }
        if self.palw_class_registration_origin_v1 == Some(ForkActivation::never()) {
            self.palw_class_registration_origin_v1 = None;
        }
        if self.palw_selected_source_bootstrap_v1.is_some_and(|f| f.at == ForkActivation::never()) {
            self.palw_selected_source_bootstrap_v1 = None;
        }
        if self.palw_source_verifier_economics_v2.is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_source_verifier_economics_v2 = None;
        }
        if self.palw_paid_bootstrap_source_v6.is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_paid_bootstrap_source_v6 = None;
        }
        // ADR-0100 Decision 4, likewise.
        if self.palw_shard_licensing == Some(ForkActivation::never()) {
            self.palw_shard_licensing = None;
        }
        // ADR-0102, likewise.
        if self.palw_token_lift == Some(ForkActivation::never()) {
            self.palw_token_lift = None;
        }
        if self.palw_kimi_k3 == Some(ForkActivation::never()) {
            self.palw_kimi_k3 = None;
        }
        // RFC-0002 Phase F: the WHOLE option collapses (the D1 rule) — a present `never()` would
        // carry the prim set and the ceilings into `consensus_params_id`, bytes an unset build never
        // writes, and scheduling the IR would partition the fleet at deploy.
        if self.palw_tir_v1.is_some_and(|fence| fence.activation == ForkActivation::never()) {
            self.palw_tir_v1 = None;
        }
        // RFC-0003, likewise: the WHOLE option collapses from `Some(never())`.
        if self.palw_gen_v1.is_some_and(|fence| fence.activation == ForkActivation::never()) {
            self.palw_gen_v1 = None;
        }
        // FP Job V5, likewise.
        if self.palw_fp_job_v5 == Some(ForkActivation::never()) {
            self.palw_fp_job_v5 = None;
        }
        // The generative range twin, likewise.
        if self.palw_gen_range_twin_v1 == Some(ForkActivation::never()) {
            self.palw_gen_range_twin_v1 = None;
        }
        // The task-head profile, likewise: the WHOLE option collapses from `Some(never())`.
        if self.palw_task_heads_v1.is_some_and(|fence| fence.activation == ForkActivation::never()) {
            self.palw_task_heads_v1 = None;
        }
        if self.palw_stage_execution_v28.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_stage_execution_v28 = None;
        }
        if self.palw_native_execution_v39.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_native_execution_v39 = None;
        }
        if self.palw_native_mining_v39.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_native_mining_v39 = None;
        }
        if self.palw_native_final_v27.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_native_final_v27 = None;
        }
        if self.palw_stage_execution_v14.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_stage_execution_v14 = None;
        }
        // Collapse the entire funded execution tuple from never(), including policies.
        if self.palw_funded_execution_v11.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_funded_execution_v11 = None;
        }
        // The probabilistic-constraint fence, likewise.
        if self.palw_probabilistic_constraints_v1 == Some(ForkActivation::never()) {
            self.palw_probabilistic_constraints_v1 = None;
        }
        // Rule E's fence (ADR-0178), likewise.
        if self.palw_fork_choice_rule_e_v1 == Some(ForkActivation::never()) {
            self.palw_fork_choice_rule_e_v1 = None;
        }
        // The typed-roots fence, likewise.
        if self.palw_typed_roots_v1 == Some(ForkActivation::never()) {
            self.palw_typed_roots_v1 = None;
        }
        // The fork-choice commitment fence, likewise.
        if self.palw_fork_choice_commitment_v1 == Some(ForkActivation::never()) {
            self.palw_fork_choice_commitment_v1 = None;
        }
        // The verifier-pay fence, likewise.
        if self.palw_verifier_pay_v1 == Some(ForkActivation::never()) {
            self.palw_verifier_pay_v1 = None;
        }
        if self.palw_model_refutation_seal_v1 == Some(ForkActivation::never()) {
            self.palw_model_refutation_seal_v1 = None;
        }
        if self.palw_window_class_v37 == Some(ForkActivation::never()) {
            self.palw_window_class_v37 = None;
        }
        if self.palw_beacon_source_opt_in_v1.is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_beacon_source_opt_in_v1 = None;
        }
        if self.palw_beacon_source_fee_v1.is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_beacon_source_fee_v1 = None;
        }
        // The Panel-free fence, likewise.
        if self.palw_panel_free_v1.as_ref().is_some_and(|fence| fence.activation == ForkActivation::never()) {
            self.palw_panel_free_v1 = None;
        }
        // The signed-registration envelope's fence, likewise.
        if self.palw_signed_registration_v1 == Some(ForkActivation::never()) {
            self.palw_signed_registration_v1 = None;
        }
        // RFC-0006 per-segment pricing's fence, likewise.
        if self.palw_tir_shard_segment_v2 == Some(ForkActivation::never()) {
            self.palw_tir_shard_segment_v2 = None;
        }
        // The provider court's fence (DA16), likewise.
        if self.palw_provider_court_v1 == Some(ForkActivation::never()) {
            self.palw_provider_court_v1 = None;
        }
        // The legacy held DA fence (LG14-B), likewise.
        if self.palw_legacy_held_da_v2 == Some(ForkActivation::never()) {
            self.palw_legacy_held_da_v2 = None;
        }
        // ADR-0176 / ADR-0177's fences, collapsed whole (their policies are never read there).
        if self.palw_reporter_income_v2 == Some(ForkActivation::never()) {
            self.palw_reporter_income_v2 = None;
        }
        if self.palw_paid_consumer_lifecycle_v1.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_paid_consumer_lifecycle_v1 = None;
        }
        if self.palw_panel_lifetime_v2.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_panel_lifetime_v2 = None;
        }
        if self.palw_claim_challenge_v2.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_claim_challenge_v2 = None;
        }
        if self.palw_claim_compact_v4.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_claim_compact_v4 = None;
        }
        if self.palw_material_release_recovery_v2.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_material_release_recovery_v2 = None;
        }
        if self.palw_monetary_material_v1.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_monetary_material_v1 = None;
        }
        if self.palw_model_verification_gate_v1.is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_model_verification_gate_v1 = None;
        }
        if self.palw_horizon_extension_v1.is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_horizon_extension_v1 = None;
        }
        if self.palw_minimum_price_v1.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_minimum_price_v1 = None;
        }
        if self.palw_beacon_bootstrap_v1.is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_beacon_bootstrap_v1 = None;
        }
        if self.palw_material_income_v3 == Some(ForkActivation::never()) {
            self.palw_material_income_v3 = None;
        }
        if self.palw_settlement_once_v1 == Some(ForkActivation::never()) {
            self.palw_settlement_once_v1 = None;
        }
        // Lane FLEET: collapse to absent, as above.
        if self.palw_kernel_route_panel_one_v1 == Some(ForkActivation::never()) {
            self.palw_kernel_route_panel_one_v1 = None;
        }
        if self.palw_tir_bounded_door_v1 == Some(ForkActivation::never()) {
            self.palw_tir_bounded_door_v1 = None;
        }
        if self.palw_tir_designated_answers_v1 == Some(ForkActivation::never()) {
            self.palw_tir_designated_answers_v1 = None;
        }
        if self.palw_court_outsider_lane_v1 == Some(ForkActivation::never()) {
            self.palw_court_outsider_lane_v1 = None;
        }
        if self.palw_court_clock_v2 == Some(ForkActivation::never()) {
            self.palw_court_clock_v2 = None;
        }
        if self.palw_panel_da_disclosure_v1 == Some(ForkActivation::never()) {
            self.palw_panel_da_disclosure_v1 = None;
        }
        if self.palw_paid_claim_trap_v1 == Some(ForkActivation::never()) {
            self.palw_paid_claim_trap_v1 = None;
        }
        if self.palw_native_liability_horizon_v1 == Some(ForkActivation::never()) {
            self.palw_native_liability_horizon_v1 = None;
        }
        if self.palw_bond_budget_v1.as_ref().is_some_and(|fence| fence.activation == ForkActivation::never()) {
            self.palw_bond_budget_v1 = None;
        }
        if self.palw_model_bond_allocation_v1.as_ref().is_some_and(|fence| fence.activation == ForkActivation::never()) {
            self.palw_model_bond_allocation_v1 = None;
        }
        // ADR-0176 amendment (ユーザー判断 2026-10-15): the bond formula's fence, likewise collapsed whole.
        if self.palw_bond_formula_v2.is_some_and(|fence| fence.activation == ForkActivation::never()) {
            self.palw_bond_formula_v2 = None;
        }
        // ADR-0032's amended reporter share (lane INTF), likewise.
        if self.palw_reporter_share_v2 == Some(ForkActivation::never()) {
            self.palw_reporter_share_v2 = None;
        }
        // The legacy route's public filer (lane LG14-A), likewise.
        if self.palw_legacy_public_filer_v1 == Some(ForkActivation::never()) {
            self.palw_legacy_public_filer_v1 = None;
        }
        // The three fences of the 2026-10-12 rulings, likewise.
        if self.palw_court_model_scope_v1 == Some(ForkActivation::never()) {
            self.palw_court_model_scope_v1 = None;
        }
        if self.palw_public_evidence_reward_v1 == Some(ForkActivation::never()) {
            self.palw_public_evidence_reward_v1 = None;
        }
        // Compute-claim audit CX-01/CX-02's answer accounting, likewise.
        if self.palw_fp_answer_accounting_v1 == Some(ForkActivation::never()) {
            self.palw_fp_answer_accounting_v1 = None;
        }
        if self.palw_claim_hard_deadline_v1.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_claim_hard_deadline_v1 = None;
        }
        // RFC-0006 R6-CELL round 4's coverage closure, likewise.
        if self.palw_cell_coverage_closure_v1 == Some(ForkActivation::never()) {
            self.palw_cell_coverage_closure_v1 = None;
        }
        // The cross-lane G14 gates (`crate::palw_g14_gates_v1`), likewise.
        if self.palw_g14_route_required_v1 == Some(ForkActivation::never()) {
            self.palw_g14_route_required_v1 = None;
        }
        if self.palw_reward_liability_horizon_v1 == Some(ForkActivation::never()) {
            self.palw_reward_liability_horizon_v1 = None;
        }
        // The court-capacity fence (`crate::palw_court_capacity_v1`), likewise.
        if self.palw_court_capacity_v1.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_court_capacity_v1 = None;
        }
        if self.palw_court_transport_v1.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_court_transport_v1 = None;
        }
        // RFC-0023 stage 1 (`crate::palw_unresolved_bound_v1`), likewise.
        if self.palw_unresolved_bound_v1 == Some(ForkActivation::never()) {
            self.palw_unresolved_bound_v1 = None;
        }
        if self.palw_claim_local_settlement_v1 == Some(ForkActivation::never()) {
            self.palw_claim_local_settlement_v1 = None;
        }
        // RFC-0016's two dormant fences, likewise.
        if self.palw_model_block_rights_v1 == Some(ForkActivation::never()) {
            self.palw_model_block_rights_v1 = None;
        }
        if self.palw_elastic_final_capacity_v1 == Some(ForkActivation::never()) {
            self.palw_elastic_final_capacity_v1 = None;
        }
        // RFC-0017's model eligibility, likewise.
        if self.palw_model_eligibility_v1 == Some(ForkActivation::never()) {
            self.palw_model_eligibility_v1 = None;
        }
        // ADR-0179's model tiers, likewise.
        if self.palw_model_tier_v1.as_ref().is_some_and(|fence| fence.activation == ForkActivation::never()) {
            self.palw_model_tier_v1 = None;
        }
        // G14's per-class detection policy, likewise.
        if self.palw_detection_policy_v1.as_ref().is_some_and(|fence| fence.activation == ForkActivation::never()) {
            self.palw_detection_policy_v1 = None;
        }
        // **RFC-0019 §7.2: an annual catalog edition not yet in force leaves the identity WITH ITS CONTENTS.** `for_each_fence` has
        // rewritten every edition's height to `u64::MAX` unless it is genesis-active; left in place, the edition's number, height and
        // root would be hashed by `write_value_into`, a build carrying the NEXT edition would differ from every build that does not,
        // and the handshake would refuse it at deploy — the rolling upgrade RFC-0019 exists for would be a deploy-day partition.
        // Dropped here, old and new builds share the identity until the edition activates; from its height the fork id (which gates
        // on every edition's height) refuses the build that lacks it. Edition 0 (DAA 0, the genesis catalog) stays.
        if let Some(fence) = self.palw_model_tier_v1.as_mut() {
            fence.catalogs.drop_unscheduled_for_identity();
        }
        // RFC-0018's five dormant fences, likewise.
        if self.palw_dag_sibling_attribution_v1 == Some(ForkActivation::never()) {
            self.palw_dag_sibling_attribution_v1 = None;
        }
        if self.palw_panel_anchor_permissionless_v1 == Some(ForkActivation::never()) {
            self.palw_panel_anchor_permissionless_v1 = None;
        }
        // The liveness fix L-COURT-01's fence, likewise.
        if self.palw_court_completion_attempt_v1 == Some(ForkActivation::never()) {
            self.palw_court_completion_attempt_v1 = None;
        }
        if self.palw_rule_e_participation_v2 == Some(ForkActivation::never()) {
            self.palw_rule_e_participation_v2 = None;
        }
        if self.palw_independently_work_certified_v1 == Some(ForkActivation::never()) {
            self.palw_independently_work_certified_v1 = None;
        }
        if self.palw_gate_c_attestor_liability_v1 == Some(ForkActivation::never()) {
            self.palw_gate_c_attestor_liability_v1 = None;
        }
        if self.palw_dag_width_schedule_v1.as_ref().is_some_and(|f| f.activation == ForkActivation::never()) {
            self.palw_dag_width_schedule_v1 = None;
        }
        if self.palw_fp_prefix_inherit == Some(ForkActivation::never()) {
            self.palw_fp_prefix_inherit = None;
        }
        if self.palw_fp_prefix_state == Some(ForkActivation::never()) {
            self.palw_fp_prefix_state = None;
        }
        if self.palw_fp_tokenizer_match == Some(ForkActivation::never()) {
            self.palw_fp_tokenizer_match = None;
        }
        if self.palw_fp_constraint_v2 == Some(ForkActivation::never()) {
            self.palw_fp_constraint_v2 = None;
        }
        if self.palw_adapter_class_v1 == Some(ForkActivation::never()) {
            self.palw_adapter_class_v1 = None;
        }
        // RFC-0002 Phase F's second IR fence: Some-only hashed, so the same collapse.
        if self.palw_tir_fence2 == Some(ForkActivation::never()) {
            self.palw_tir_fence2 = None;
        }
        // RFC-0006's layer-sharded panels: Some-only hashed, so the same collapse.
        if self.palw_tir_shard_v1 == Some(ForkActivation::never()) {
            self.palw_tir_shard_v1 = None;
        }
        // ADR-0165's two fences: Some-only hashed, so the same collapse.
        if self.palw_floor_reserve_v1 == Some(ForkActivation::never()) {
            self.palw_floor_reserve_v1 = None;
        }
        if self.palw_real_clock_tick_v1 == Some(ForkActivation::never()) {
            self.palw_real_clock_tick_v1 = None;
        }
        // ADR-0170's window: Some-only hashed, so the same collapse.
        if self.palw_anchor_window_v1 == Some(ForkActivation::never()) {
            self.palw_anchor_window_v1 = None;
        }
        // RFC-0009's receipt redemption: Some-only hashed, so the same collapse.
        if self.palw_receipt_spend_v4 == Some(ForkActivation::never()) {
            self.palw_receipt_spend_v4 = None;
        }
        if self.palw_evidence_court_v1 == Some(ForkActivation::never()) {
            self.palw_evidence_court_v1 = None;
        }
        // Lane PA's audit-1004 fence: Some-only hashed, so the same collapse.
        if self.palw_audit_1004_v1 == Some(ForkActivation::never()) {
            self.palw_audit_1004_v1 = None;
        }
        // RFC-0004, likewise: the WHOLE option collapses from `Some(never())`.
        if self.palw_improvement_v1.is_some_and(|fence| fence.activation == ForkActivation::never()) {
            self.palw_improvement_v1 = None;
        }
        // RFC-0003's held leaf challenge, likewise.
        if self.palw_held_close_chunks_v1 == Some(ForkActivation::never()) {
            self.palw_held_close_chunks_v1 = None;
        }
        // RFC-0007's verification vertex, likewise.
        if self.palw_verification_vertex_v1 == Some(ForkActivation::never()) {
            self.palw_verification_vertex_v1 = None;
        }
        // RFC-0008 v2's EXEC payload, likewise.
        if self.palw_exec_payload_v2 == Some(ForkActivation::never()) {
            self.palw_exec_payload_v2 = None;
        }
        if self.palw_witness_manifest_v1 == Some(ForkActivation::never()) {
            self.palw_witness_manifest_v1 = None;
        }
        if self.palw_audit_mesh_v1 == Some(ForkActivation::never()) {
            self.palw_audit_mesh_v1 = None;
        }
        if self.palw_capped_onboarding_v1 == Some(ForkActivation::never()) {
            self.palw_capped_onboarding_v1 = None;
        }
        // RFC-0002 Part II's class-seating fence, likewise: the WHOLE option collapses from `Some(never())`.
        if self.palw_class_seating.is_some_and(|fence| fence.activation == ForkActivation::never()) {
            self.palw_class_seating = None;
        }
        // RFC-0002 Phase H's IR-only fence, likewise.
        if self.palw_tir_only_v1 == Some(ForkActivation::never()) {
            self.palw_tir_only_v1 = None;
        }
        // ADR-0093 Decision 6, likewise.
        if self.palw_fused_dissectable == Some(ForkActivation::never()) {
            self.palw_fused_dissectable = None;
        }
        // ADR-0093 Decision 8, likewise.
        if self.palw_attn_anchored_root == Some(ForkActivation::never()) {
            self.palw_attn_anchored_root = None;
        }
        // ADR-0103, likewise.
        if self.palw_held_context == Some(ForkActivation::never()) {
            self.palw_held_context = None;
        }
        // ADR-0117, likewise.
        if self.palw_prefill_draw == Some(ForkActivation::never()) {
            self.palw_prefill_draw = None;
        }
        if self.palw_audit_2026_09_11 == Some(ForkActivation::never()) {
            self.palw_audit_2026_09_11 = None;
        }
        if self.palw_audit_2026_09_11_deep == Some(ForkActivation::never()) {
            self.palw_audit_2026_09_11_deep = None;
        }
        if self.palw_audit_2026_09_23 == Some(ForkActivation::never()) {
            self.palw_audit_2026_09_23 = None;
        }
        // A zero depth is "no second clock", the same collapse the fences make: absence and the
        // no-op value must fingerprint identically or two builds would split on a spelling.
        if self.palw_settled_anchor_depth == Some(0) {
            self.palw_settled_anchor_depth = None;
        }
        // The same collapse for the admission audit period: zero is "ADR-0147's own period".
        if self.palw_admission_audit_period_daa == Some(0) {
            self.palw_admission_audit_period_daa = None;
        }
        // ADR-0151's stated maturity is a value its bundle's fence drags with it (audit3 H1): below
        // `palw_economic_safety` nothing reads it, so a scheduled — not yet active — fence takes it
        // out of the identity too, and the schedule id reports it.
        if !self.palw_economic_safety.is_some_and(|f| f.is_active(0)) {
            self.palw_exec_quantum_maturity_daa = None;
        }
        // ADR-0083 Decision 1, the same shape: a bare fence, `never()` is absence.
        if self.palw_difficulty_priced_rows == Some(ForkActivation::never()) {
            self.palw_difficulty_priced_rows = None;
        }
        if self.palw_receipt_rows_unpriced == Some(ForkActivation::never()) {
            self.palw_receipt_rows_unpriced = None;
        }
        // ADR-0072 D8 at the header stage, the same shape: a bare fence, `never()` is absence.
        if self.palw_attempt_header_pins == Some(ForkActivation::never()) {
            self.palw_attempt_header_pins = None;
        }
        // ADR-0042 Decision 11's complete context set. The `palw_prompt_ids_merkle` shape: a bare
        // fence, so the whole option collapses.
        if self.palw_signature_contexts_v2 == Some(ForkActivation::never()) {
            self.palw_signature_contexts_v2 = None;
        }
        // ADR-0072 D8 on the free-prompt lane, a bare fence: the same collapse for the same reason.
        if self.palw_fp_da_pins == Some(ForkActivation::never()) {
            self.palw_fp_da_pins = None;
        }
        // ADR-0018 §E's payout bounds (mainnet audit 2026-09-06), the same shape: a bare fence,
        // `never()` is absence.
        if self.palw_validator_payout_bounds == Some(ForkActivation::never()) {
            self.palw_validator_payout_bounds = None;
        }
        // MSK-26A: the slashing-evidence UTXO genuineness fence — a bare fence, `never()` is absence.
        if self.palw_slashing_evidence_utxo_genuine == Some(ForkActivation::never()) {
            self.palw_slashing_evidence_utxo_genuine = None;
        }
        // ADR-0045 D2's boundary budget and ADR-0044 D9's free-prompt caps (mainnet audit
        // 2026-09-06, M-2 and L-2): bare fences, same collapse, same reason.
        if self.palw_epoch_boundary_budget == Some(ForkActivation::never()) {
            self.palw_epoch_boundary_budget = None;
        }
        if self.palw_epoch_budget_release == Some(ForkActivation::never()) {
            self.palw_epoch_budget_release = None;
        }
        if self.palw_panel_economy == Some(ForkActivation::never()) {
            self.palw_panel_economy = None;
        }
        if self.palw_work_priced_reward == Some(ForkActivation::never()) {
            self.palw_work_priced_reward = None;
        }
        if self.palw_execution_lane.is_some_and(|lane| lane.activation == ForkActivation::never()) {
            self.palw_execution_lane = None;
        }
        // §7.2: a scheduled widening leaves the identity with its width, as a scheduled lane leaves
        // with its shape — the width is reported in the schedule id, and the fork-id gate separates
        // peers once the height fires.
        if let Some(lane) = self.palw_execution_lane.as_mut() {
            for stage in lane.widenings.iter_mut() {
                if stage.activation == ForkActivation::never() {
                    *stage = PalwExecWideningV1::NONE;
                }
            }
            if lane.short_span.activation == ForkActivation::never() {
                lane.short_span = PalwExecSpanShortV1::NONE;
            }
        }
        // ADR-0126, the lane's shape again: the whole option collapses, so the two numbers beside a
        // scheduled height leave the identity with it (the schedule id reports them).
        if self.palw_permissionless_panel_v1.is_some_and(|r| r.activation == ForkActivation::never()) {
            self.palw_permissionless_panel_v1 = None;
        }
        if self.palw_permissionless_panel_complete_v1.is_some_and(|r| r.activation == ForkActivation::never()) {
            self.palw_permissionless_panel_complete_v1 = None;
        }
        if self.palw_permissionless_panel_retired_v1.is_some_and(|r| r.activation == ForkActivation::never()) {
            self.palw_permissionless_panel_retired_v1 = None;
        }
        if self.palw_dns_retirement.is_some_and(|r| r.activation == ForkActivation::never()) {
            self.palw_dns_retirement = None;
        }
        if self.palw_overlay_carve.is_some_and(|carve| carve.activation == ForkActivation::never()) {
            self.palw_overlay_carve = None;
        }
        // ADR-0130, the carve's shape: the whole option collapses, so the reward multiple beside a
        // scheduled height leaves the identity with it (the schedule id reports it).
        if self.palw_panel_exposure_floor.is_some_and(|floor| floor.activation == ForkActivation::never()) {
            self.palw_panel_exposure_floor = None;
        }
        if self.palw_fp_ruleset_caps == Some(ForkActivation::never()) {
            self.palw_fp_ruleset_caps = None;
        }
        // ADR-0105, a bare fence: the same collapse for the same reason — `never()` is absence,
        // and a present `never()` would write a name an unset build never writes.
        if self.palw_heartbeat_transparent == Some(ForkActivation::never()) {
            self.palw_heartbeat_transparent = None;
        }
        // ---- F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
        // A bare Some-only fence: the same collapse, or a build that schedules it and a build that
        // does not would refuse each other on deploy day over a height neither has reached.
        if self.palw_heartbeat_transparent_same_chain == Some(ForkActivation::never()) {
            self.palw_heartbeat_transparent_same_chain = None;
        }
        // ---- end F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
        // ADR-0160 lane escrow, a bare Some-only fence: the same collapse (a-some-only-fence).
        if self.palw_capacity_escrow_at_licence == Some(ForkActivation::never()) {
            self.palw_capacity_escrow_at_licence = None;
        }
        // ADR-0107, a bare fence: the same collapse for the same reason.
        if self.palw_share_growth_final == Some(ForkActivation::never()) {
            self.palw_share_growth_final = None;
        }
        // P0a, a bare fence: the same collapse for the same reason.
        if self.palw_gdn_key_heads == Some(ForkActivation::never()) {
            self.palw_gdn_key_heads = None;
        }
        let Some(dns) = self.dns_params.as_mut() else {
            return;
        };
        // Read AFTER `for_each_fence`, so `u64::MAX` here means "not active at genesis" — which is
        // exactly the condition under which the table is a consequence of a schedule rather than a
        // rule in force.
        if dns.vlt.vlt_shadow_activation_daa_score == u64::MAX && dns.vlt.vlt_activation_daa_score == u64::MAX {
            dns.vlt.model_cost_table = crate::vlt::ModelCostTable::EMPTY;
        }
        if dns.tkn.tkn_shadow_activation_daa_score == u64::MAX
            && dns.tkn.tkn_activation_daa_score == u64::MAX
            && dns.tkn.emission_activation_epoch == u64::MAX
        {
            dns.tkn = crate::token::TokenParams::INERT;
        }
    }

    /// **Every top-level PALW activation fence, named, in ONE compiler-forced spelling.**
    ///
    /// The destructure below is exhaustive, for [`Self::for_each_fence`]'s reason: a fence added to
    /// `Params` will not compile until it is named here. Two hand-written lists needed it and both
    /// had already fallen behind when the DA-court and model-market branches merged — the test-only
    /// fence table (mainnet audit 2026-09-06, L-3) and `fork_id_v1::fork_id_gate_fences_v1` (M-6),
    /// which the merge grew four fences without touching either.
    ///
    /// Some-only fences that carry a companion value (`palw_heartbeat`, `palw_attempt_work`,
    /// `palw_bond_maturity`, `dns_bft_gate`, `palw_inactivity_leak`, `palw_beacon_fold`) contribute their
    /// `activation` and nothing else: a duration beside a fence is deliberately not a fence, for
    /// the reason `for_each_fence` gives about normalisation.
    ///
    /// Order is the declaration order of the struct and should stay stable — a caller may hash it.
    pub fn palw_fences_v1(&self) -> Vec<(&'static str, Option<ForkActivation>)> {
        // Exhaustive on purpose — see the doc block. Non-fence fields are bound and ignored.
        let Params {
            dns_seeders: _,
            net: _,
            genesis: _,
            timestamp_deviation_tolerance: _,
            max_difficulty_target: _,
            max_difficulty_target_f64: _,
            past_median_time_window_size: _,
            difficulty_window_size: _,
            min_difficulty_window_size: _,
            coinbase_payload_script_public_key_max_len: _,
            max_coinbase_payload_len: _,
            max_tx_inputs: _,
            max_tx_outputs: _,
            max_signature_script_len: _,
            max_script_public_key_len: _,
            mass_per_tx_byte: _,
            mass_per_script_pub_key_byte: _,
            mass_per_sig_op: _,
            max_block_mass: _,
            storage_mass_parameter: _,
            deflationary_phase_daa_score: _,
            pre_deflationary_phase_base_subsidy: _,
            skip_proof_of_work: _,
            max_block_level: _,
            pruning_proof_m: _,
            blockrate: _,
            pre_crescendo_target_time_per_block: _,
            crescendo_activation: _,
            dns_params: _,
            palw_credit: _,
            palw_fork_choice: _,
            palw_schedule: _,
            palw_ramp: _,
            palw_block_commitment: _,
            palw_bootstrap_activation,
            palw_unavailable_abstains,
            palw_bond_maturity,
            // Lane maturity (post-launch, 2026-09-26).
            palw_bond_maturity_early,
            palw_frontier_provenance,
            palw_reorg_strict_economic_win,
            palw_pruning_proof_strict_economic_win,
            palw_capacity_weight_cap,
            palw_heartbeat,
            palw_attempt_work,
            palw_attempt_activation,
            dns_bft_gate,
            palw_inactivity_leak,
            palw_beacon_fold,
            palw_capability_bound,
            palw_compute_overlay_retired,
            palw_model_registry,
            palw_economic_payout,
            palw_work_target,
            palw_single_lottery,
            palw_short_challenge_window,
            palw_verification_v2,
            palw_verification_s3,
            palw_verification_s2,
            palw_readiness_v2,
            palw_anchor_clock,
            palw_clock_cursor,
            palw_clock_floor,
            palw_clock_lead_cap,
            palw_offence_attribution,
            palw_rcore_plus,
            // Lane F1 (the panel seed, post-launch).
            palw_panel_seed_execution,
            // Lane A (the operator anchor, post-launch).
            palw_operator_anchor,
            // ADR-0160 lane liab (F-L).
            palw_capacity_aggregate_liability,
            palw_rcore_conservative_classes: _,
            // Lane V02 (a resolved claim's lock off the work ceiling, post-launch).
            palw_final_lock_full_collateral,
            // Lane V02 (the shortened post-Final lock life, post-launch).
            palw_final_lock_life,
            // Lane F2-lock (the lock life applied retroactively, post-launch).
            palw_final_lock_life_retro,
            palw_class_verify_deadline,
            palw_class_verify_rows: _,
            // Lane bind-deadlock (post-launch).
            palw_anchor_at_ceiling,
            // Lane accept-order (post-launch).
            palw_lane_accept_parents_first,
            palw_activation_pool,
            palw_readiness_v2_max_age_spans,
            palw_registry_resilience,
            // Lane F2 (the floor-refusal retry, post-launch).
            palw_floor_refusal_retry,
            palw_artifact_root_ownership,
            palw_operator_id_unique,
            palw_objective_offence,
            palw_seat_gate_possession,
            palw_class_receipt_window,
            palw_execution_quanta,
            palw_public_model_source_required,
            palw_economic_safety: _,
            palw_canonical_work,
            palw_admission_independence,
            palw_context_ladder,
            palw_panel_da,
            palw_certification_rent,
            palw_uncertified_weightless,
            palw_da_court,
            palw_court_ladder,
            palw_fp_da_pins,
            palw_validator_payout_bounds,
            palw_slashing_evidence_utxo_genuine,
            palw_epoch_boundary_budget,
            palw_epoch_budget_release,
            palw_panel_economy,
            palw_work_priced_reward,
            palw_execution_lane,
            palw_overlay_carve,
            palw_dns_retirement,
            palw_permissionless_panel_v1,
            palw_permissionless_panel_complete_v1,
            palw_permissionless_panel_retired_v1,
            palw_panel_exposure_floor,
            palw_fp_ruleset_caps,
            palw_model_market,
            palw_model_lines,
            palw_model_immutable_v1,
            palw_class_semantics_v1,
            palw_court_commitment_scope_v1,
            palw_staged_public_court_v1,
            palw_staged_tir_route_v1,
            palw_tir_public_conviction_v1,
            palw_tir_container_identity_v1,
            palw_tir_frame_cost_v1: _,
            palw_tir_bounded_door_v1,
            palw_tir_designated_answers_v1,
            palw_court_outsider_lane_v1,
            palw_state_row_map_v1,
            palw_kernel_route_panel_one_v1,
            palw_court_clock_v2,
            palw_panel_da_disclosure_v1,
            palw_staged_float_constraint_v1,
            palw_model_benefits,
            palw_model_leg_v2,
            palw_model_seed_v2,
            palw_model_virtual_v1,
            palw_model_evm,
            // Lane sink (the model sink binding, post-launch).
            palw_model_sink_bound,
            // ADR-0160 lane verify (F-B, F-R).
            palw_capacity_batch_licence,
            palw_capacity_verify_room,
            palw_capacity_audit_door,
            palw_capacity_issuance_slots,
            palw_capacity_network_room,
            palw_capacity_network_verify,
            palw_panel_unavailable_expiry,
            palw_panel_fast_switch,
            palw_seat_availability,
            palw_capacity_emission_budget,
            palw_capacity_multi_claim,
            palw_capacity_rho_breaker,
            palw_chunk_cap_charge,
            palw_prompt_ids_merkle,
            palw_kary_court,
            palw_model_court_window,
            palw_court_responder_coverage,
            palw_fp_decode_rules,
            palw_fp_derived_work,
            palw_fp_decode_constraint,
            palw_shard_court,
            palw_memory_routing_court_v2,
            palw_class_registration_origin_v1,
            palw_selected_source_bootstrap_v1,
            palw_source_verifier_economics_v2,
            palw_paid_bootstrap_source_v6,
            palw_shard_licensing,
            palw_token_lift,
            palw_kimi_k3,
            palw_tir_v1,
            palw_gen_v1,
            palw_fp_job_v5,
            palw_gen_range_twin_v1,
            palw_task_heads_v1,
            palw_probabilistic_constraints_v1,
            palw_funded_execution_v11,
            palw_stage_execution_v14,
            palw_native_final_v27,
            palw_stage_execution_v28,
            palw_native_execution_v39,
            palw_native_mining_v39,
            palw_fork_choice_rule_e_v1,
            palw_typed_roots_v1,
            palw_fork_choice_commitment_v1,
            palw_verifier_pay_v1,
            palw_model_refutation_seal_v1,
            palw_window_class_v37,
            palw_beacon_source_opt_in_v1,
            palw_beacon_source_fee_v1,
            palw_panel_free_v1,
            palw_signed_registration_v1,
            palw_tir_shard_segment_v2,
            palw_provider_court_v1,
            palw_legacy_held_da_v2,
            palw_bond_budget_v1,
            palw_reporter_income_v2,
            palw_material_income_v3,
            palw_settlement_once_v1,
            palw_paid_claim_trap_v1,
            palw_native_liability_horizon_v1,
            palw_panel_lifetime_v2,
            palw_claim_challenge_v2,
            palw_claim_compact_v4,
            palw_monetary_material_v1,
            palw_material_release_recovery_v2,
            palw_model_verification_gate_v1,
            palw_horizon_extension_v1,
            palw_minimum_price_v1,
            palw_beacon_bootstrap_v1,
            palw_paid_consumer_lifecycle_v1,
            palw_model_bond_allocation_v1,
            palw_bond_formula_v2,
            palw_reporter_share_v2,
            palw_legacy_public_filer_v1,
            palw_court_model_scope_v1,
            palw_public_evidence_reward_v1,
            palw_fp_answer_accounting_v1,
            palw_claim_hard_deadline_v1,
            palw_cell_coverage_closure_v1,
            palw_g14_route_required_v1,
            palw_reward_liability_horizon_v1,
            palw_court_capacity_v1,
            palw_court_transport_v1,
            palw_unresolved_bound_v1,
            palw_claim_local_settlement_v1,
            palw_model_block_rights_v1,
            palw_elastic_final_capacity_v1,
            palw_model_eligibility_v1,
            palw_model_tier_v1,
            palw_detection_policy_v1,
            palw_dag_sibling_attribution_v1,
            palw_panel_anchor_permissionless_v1,
            palw_court_completion_attempt_v1,
            palw_rule_e_participation_v2,
            palw_independently_work_certified_v1,
            palw_gate_c_attestor_liability_v1,
            palw_dag_width_schedule_v1,
            palw_fp_prefix_inherit,
            palw_fp_prefix_state,
            palw_fp_tokenizer_match,
            palw_fp_constraint_v2,
            palw_adapter_class_v1,
            palw_tir_fence2,
            palw_tir_shard_v1,
            palw_floor_reserve_v1,
            palw_real_clock_tick_v1,
            palw_anchor_window_v1,
            palw_receipt_spend_v4,
            palw_evidence_court_v1,
            palw_audit_1004_v1,
            palw_improvement_v1,
            palw_held_close_chunks_v1,
            palw_verification_vertex_v1,
            palw_exec_payload_v2,
            palw_witness_manifest_v1,
            palw_audit_mesh_v1,
            palw_capped_onboarding_v1,
            palw_class_seating,
            palw_tir_only_v1,
            palw_fused_dissectable,
            palw_attn_anchored_root,
            palw_held_context,
            palw_prefill_draw,
            palw_audit_2026_09_11,
            palw_audit_2026_09_11_deep,
            palw_audit_2026_09_23,
            // A number beside the fences, not a fence: the visitor rewrites HEIGHTS, and a
            // settled-anchor depth is neither a height nor normalisable to one.
            palw_settled_anchor_depth: _,
            // A period in DAA beside the fences, not a height — the visitor has nothing to rewrite.
            palw_admission_audit_period_daa: _,
            // A maturity in DAA beside the fences (it rides `palw_economic_safety`'s height), not a
            // height — the visitor has nothing to rewrite.
            palw_exec_quantum_maturity_daa: _,
            palw_difficulty_priced_rows,
            palw_receipt_rows_unpriced,
            palw_attempt_header_pins,
            palw_signature_contexts_v2,
            palw_heartbeat_transparent,
            // F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency)
            palw_heartbeat_transparent_same_chain,
            // ADR-0160 lane escrow (post-launch)
            palw_capacity_escrow_at_licence,
            palw_share_growth_final,
            palw_gdn_key_heads,
            palw_consensus_mode: _,
            pow_blake2b_sha3_activation: _,
            pow_palw_activation: _,
            pow_palw_ollama_activation: _,
            pq_enforcement: _,
            pq_activation_daa_score: _,
            evm_activation_daa_score: _,
            evm_gas_pool_v2_activation_daa_score: _,
            evm_f002_withdraw_cap_activation_daa_score: _,
            evm_bridge_ledger_activation_daa_score: _,
            evm_f003_mldsa_verify_activation_daa_score: _,
            evm_typed_receipt_root_activation_daa_score: _,
        } = self;
        vec![
            ("palw_bootstrap_activation", *palw_bootstrap_activation),
            ("palw_unavailable_abstains", *palw_unavailable_abstains),
            ("palw_bond_maturity", palw_bond_maturity.map(|f| f.activation)),
            // Lane maturity (post-launch, 2026-09-26): a top-level fence an un-upgraded peer does not
            // implement, so it is on the schedule and gates the fork id like every other.
            ("palw_bond_maturity_early", *palw_bond_maturity_early),
            ("palw_frontier_provenance", *palw_frontier_provenance),
            // lane: rcore/f1-forkchoice-attacks — feeds fork_id_gate_fences_v1 automatically.
            ("palw_reorg_strict_economic_win", *palw_reorg_strict_economic_win),
            // lane: rcore/hf-pptake2 — feeds fork_id_gate_fences_v1 automatically.
            ("palw_pruning_proof_strict_economic_win", *palw_pruning_proof_strict_economic_win),
            // lane: rcore/cap-weight (ADR-0160 F-W) — feeds fork_id_gate_fences_v1 automatically.
            ("palw_capacity_weight_cap", *palw_capacity_weight_cap),
            ("palw_heartbeat", palw_heartbeat.map(|f| f.activation)),
            ("palw_attempt_work", palw_attempt_work.map(|f| f.activation)),
            ("palw_attempt_activation", *palw_attempt_activation),
            ("dns_bft_gate", dns_bft_gate.map(|f| f.activation)),
            ("palw_inactivity_leak", palw_inactivity_leak.map(|f| f.activation)),
            ("palw_beacon_fold", palw_beacon_fold.map(|f| f.activation)),
            ("palw_capability_bound", *palw_capability_bound),
            ("palw_compute_overlay_retired", *palw_compute_overlay_retired),
            ("palw_model_registry", *palw_model_registry),
            ("palw_economic_payout", palw_economic_payout.map(|payout| payout.activation)),
            ("palw_work_target", *palw_work_target),
            ("palw_single_lottery", *palw_single_lottery),
            ("palw_short_challenge_window", *palw_short_challenge_window),
            ("palw_verification_v2", *palw_verification_v2),
            ("palw_verification_s3", *palw_verification_s3),
            ("palw_verification_s2", *palw_verification_s2),
            ("palw_readiness_v2", *palw_readiness_v2),
            ("palw_anchor_clock", *palw_anchor_clock),
            ("palw_clock_cursor", *palw_clock_cursor),
            ("palw_clock_floor", *palw_clock_floor),
            ("palw_clock_lead_cap", *palw_clock_lead_cap),
            ("palw_offence_attribution", *palw_offence_attribution),
            ("palw_rcore_plus", *palw_rcore_plus),
            // Lane F1 (the panel seed, post-launch): a top-level fence an un-upgraded peer does not
            // implement, so it is on the schedule and gates the fork id like every other.
            ("palw_panel_seed_execution", *palw_panel_seed_execution),
            // Lane A (the operator anchor, post-launch): the height only — on the schedule and the
            // fork-id gate like every top-level fence an un-upgraded peer does not implement.
            ("palw_operator_anchor", palw_operator_anchor.as_ref().map(|rule| rule.activation)),
            // Lane V02 (post-launch): a top-level fence an un-upgraded peer does not implement, so it
            // is on the schedule and gates the fork id like every other.
            ("palw_final_lock_full_collateral", *palw_final_lock_full_collateral),
            // Lane V02 (post-launch): a top-level fence an un-upgraded peer does not implement, so it
            // is on the schedule and gates the fork id like every other.
            ("palw_final_lock_life", *palw_final_lock_life),
            // ADR-0160 lane liab (F-L): the height only — on the schedule and the fork-id gate like every
            // top-level fence an un-upgraded peer does not implement (its later steps' heights are on the
            // schedule through `for_each_fence`).
            ("palw_capacity_aggregate_liability", palw_capacity_aggregate_liability.as_ref().map(|value| value.activation)),
            // Every later step's height as its own named slot (at most eight steps), so the fork-id gate
            // names an appended step as it names a widening of the execution lane.
            (
                "palw_capacity_aggregate_liability_step_2",
                crate::palw_aggregate_liability_v1::palw_capacity_step_fence_v1(palw_capacity_aggregate_liability.as_ref(), 2),
            ),
            (
                "palw_capacity_aggregate_liability_step_3",
                crate::palw_aggregate_liability_v1::palw_capacity_step_fence_v1(palw_capacity_aggregate_liability.as_ref(), 3),
            ),
            (
                "palw_capacity_aggregate_liability_step_4",
                crate::palw_aggregate_liability_v1::palw_capacity_step_fence_v1(palw_capacity_aggregate_liability.as_ref(), 4),
            ),
            (
                "palw_capacity_aggregate_liability_step_5",
                crate::palw_aggregate_liability_v1::palw_capacity_step_fence_v1(palw_capacity_aggregate_liability.as_ref(), 5),
            ),
            (
                "palw_capacity_aggregate_liability_step_6",
                crate::palw_aggregate_liability_v1::palw_capacity_step_fence_v1(palw_capacity_aggregate_liability.as_ref(), 6),
            ),
            (
                "palw_capacity_aggregate_liability_step_7",
                crate::palw_aggregate_liability_v1::palw_capacity_step_fence_v1(palw_capacity_aggregate_liability.as_ref(), 7),
            ),
            (
                "palw_capacity_aggregate_liability_step_8",
                crate::palw_aggregate_liability_v1::palw_capacity_step_fence_v1(palw_capacity_aggregate_liability.as_ref(), 8),
            ),
            // Lane F2-lock (post-launch): a top-level fence an un-upgraded peer does not implement, so it
            // is on the schedule and gates the fork id like every other.
            ("palw_final_lock_life_retro", *palw_final_lock_life_retro),
            ("palw_class_verify_deadline", *palw_class_verify_deadline),
            // Lane bind-deadlock (post-launch): a top-level fence an un-upgraded peer does not implement,
            // so it is on the schedule and gates the fork id like every other.
            ("palw_anchor_at_ceiling", *palw_anchor_at_ceiling),
            // Lane accept-order (post-launch): a top-level fence an un-upgraded peer does not implement,
            // so it is on the schedule and gates the fork id like every other.
            ("palw_lane_accept_parents_first", *palw_lane_accept_parents_first),
            ("palw_activation_pool", palw_activation_pool.map(|pool| pool.activation)),
            ("palw_readiness_v2_max_age_spans", palw_readiness_v2_max_age_spans.map(|horizon| horizon.activation)),
            // Lane F1 (registry resilience, V03/V05).
            ("palw_registry_resilience", *palw_registry_resilience),
            // Lane F2 (the floor-refusal retry, post-launch): a top-level fence an un-upgraded peer does
            // not implement, so it is on the schedule and gates the fork id like every other.
            ("palw_floor_refusal_retry", *palw_floor_refusal_retry),
            ("palw_artifact_root_ownership", *palw_artifact_root_ownership),
            ("palw_operator_id_unique", *palw_operator_id_unique),
            ("palw_objective_offence", *palw_objective_offence),
            ("palw_seat_gate_possession", *palw_seat_gate_possession),
            ("palw_class_receipt_window", *palw_class_receipt_window),
            ("palw_execution_quanta", *palw_execution_quanta),
            ("palw_public_model_source_required", palw_public_model_source_required.map(|r| r.activation)),
            ("palw_canonical_work", *palw_canonical_work),
            ("palw_admission_independence", *palw_admission_independence),
            ("palw_context_ladder", *palw_context_ladder),
            ("palw_panel_da", *palw_panel_da),
            ("palw_certification_rent", *palw_certification_rent),
            ("palw_uncertified_weightless", *palw_uncertified_weightless),
            ("palw_da_court", *palw_da_court),
            ("palw_court_ladder", *palw_court_ladder),
            ("palw_fp_da_pins", *palw_fp_da_pins),
            ("palw_validator_payout_bounds", *palw_validator_payout_bounds),
            ("palw_slashing_evidence_utxo_genuine", *palw_slashing_evidence_utxo_genuine),
            ("palw_epoch_boundary_budget", *palw_epoch_boundary_budget),
            ("palw_epoch_budget_release", *palw_epoch_budget_release),
            ("palw_panel_economy", *palw_panel_economy),
            ("palw_work_priced_reward", *palw_work_priced_reward),
            ("palw_execution_lane", palw_execution_lane.map(|lane| lane.activation)),
            (
                "palw_execution_lane_widening_1",
                palw_execution_lane.and_then(|lane| lane.widenings[0].is_used().then_some(lane.widenings[0].activation)),
            ),
            (
                "palw_execution_lane_widening_2",
                palw_execution_lane.and_then(|lane| lane.widenings[1].is_used().then_some(lane.widenings[1].activation)),
            ),
            (
                "palw_execution_lane_widening_3",
                palw_execution_lane.and_then(|lane| lane.widenings[2].is_used().then_some(lane.widenings[2].activation)),
            ),
            (
                "palw_execution_lane_widening_4",
                palw_execution_lane.and_then(|lane| lane.widenings[3].is_used().then_some(lane.widenings[3].activation)),
            ),
            (
                "palw_execution_lane_widening_5",
                palw_execution_lane.and_then(|lane| lane.widenings[4].is_used().then_some(lane.widenings[4].activation)),
            ),
            (
                "palw_execution_lane_widening_6",
                palw_execution_lane.and_then(|lane| lane.widenings[5].is_used().then_some(lane.widenings[5].activation)),
            ),
            (
                "palw_execution_lane_widening_7",
                palw_execution_lane.and_then(|lane| lane.widenings[6].is_used().then_some(lane.widenings[6].activation)),
            ),
            (
                "palw_execution_lane_widening_8",
                palw_execution_lane.and_then(|lane| lane.widenings[7].is_used().then_some(lane.widenings[7].activation)),
            ),
            (
                "palw_execution_lane_widening_9",
                palw_execution_lane.and_then(|lane| lane.widenings[8].is_used().then_some(lane.widenings[8].activation)),
            ),
            (
                "palw_execution_lane_span_short",
                palw_execution_lane.and_then(|lane| lane.short_span.is_used().then_some(lane.short_span.activation)),
            ),
            ("palw_overlay_carve", palw_overlay_carve.map(|carve| carve.activation)),
            ("palw_dns_retirement_v1", palw_dns_retirement.map(|r| r.activation)),
            ("palw_permissionless_panel_v1", palw_permissionless_panel_v1.map(|r| r.activation)),
            ("palw_permissionless_panel_complete_v1", palw_permissionless_panel_complete_v1.map(|r| r.activation)),
            ("palw_permissionless_panel_retired_v1", palw_permissionless_panel_retired_v1.map(|r| r.activation)),
            ("palw_panel_exposure_floor", palw_panel_exposure_floor.map(|floor| floor.activation)),
            ("palw_fp_ruleset_caps", *palw_fp_ruleset_caps),
            ("palw_model_market", *palw_model_market),
            ("palw_model_lines", *palw_model_lines),
            ("palw_model_immutable_v1", *palw_model_immutable_v1),
            ("palw_class_semantics_v1", *palw_class_semantics_v1),
            ("palw_court_commitment_scope_v1", *palw_court_commitment_scope_v1),
            ("palw_staged_public_court_v1", *palw_staged_public_court_v1),
            ("palw_staged_tir_route_v1", *palw_staged_tir_route_v1),
            ("palw_tir_public_conviction_v1", *palw_tir_public_conviction_v1),
            ("palw_tir_container_identity_v1", *palw_tir_container_identity_v1),
            ("palw_staged_float_constraint_v1", *palw_staged_float_constraint_v1),
            ("palw_model_benefits", *palw_model_benefits),
            ("palw_model_leg_v2", *palw_model_leg_v2),
            ("palw_model_seed_v2", *palw_model_seed_v2),
            ("palw_model_virtual_v1", *palw_model_virtual_v1),
            ("palw_model_evm", *palw_model_evm),
            // Lane sink (the model sink binding, post-launch): a top-level fence an un-upgraded peer
            // does not implement, so it is on the schedule and gates the fork id like every other.
            ("palw_model_sink_bound", *palw_model_sink_bound),
            // ADR-0160 lane verify (F-B, F-R): top-level fences an un-upgraded peer does not implement.
            ("palw_capacity_batch_licence", *palw_capacity_batch_licence),
            ("palw_capacity_verify_room", *palw_capacity_verify_room),
            ("palw_capacity_audit_door", *palw_capacity_audit_door),
            ("palw_capacity_issuance_slots", *palw_capacity_issuance_slots),
            ("palw_capacity_network_room", *palw_capacity_network_room),
            ("palw_capacity_network_verify", *palw_capacity_network_verify),
            ("palw_panel_unavailable_expiry", *palw_panel_unavailable_expiry),
            ("palw_panel_fast_switch", *palw_panel_fast_switch),
            ("palw_seat_availability", *palw_seat_availability),
            ("palw_capacity_emission_budget", *palw_capacity_emission_budget),
            ("palw_capacity_multi_claim", *palw_capacity_multi_claim),
            ("palw_capacity_rho_breaker", *palw_capacity_rho_breaker),
            ("palw_chunk_cap_charge", *palw_chunk_cap_charge),
            ("palw_prompt_ids_merkle", *palw_prompt_ids_merkle),
            ("palw_kary_court", *palw_kary_court),
            ("palw_model_court_window", *palw_model_court_window),
            ("palw_court_responder_coverage", *palw_court_responder_coverage),
            ("palw_fp_decode_rules", *palw_fp_decode_rules),
            ("palw_fp_derived_work", *palw_fp_derived_work),
            ("palw_fp_decode_constraint", *palw_fp_decode_constraint),
            ("palw_shard_court", *palw_shard_court),
            ("palw_memory_routing_court_v2", *palw_memory_routing_court_v2),
            ("palw_class_registration_origin_v1", *palw_class_registration_origin_v1),
            ("palw_selected_source_bootstrap_v1", palw_selected_source_bootstrap_v1.map(|f| f.at)),
            ("palw_source_verifier_economics_v2", palw_source_verifier_economics_v2.map(|f| f.activation)),
            ("palw_paid_bootstrap_source_v6", palw_paid_bootstrap_source_v6.map(|f| f.activation)),
            ("palw_shard_licensing", *palw_shard_licensing),
            ("palw_token_lift", *palw_token_lift),
            ("palw_kimi_k3", *palw_kimi_k3),
            ("palw_tir_v1", palw_tir_v1.map(|fence| fence.activation)),
            ("palw_gen_v1", palw_gen_v1.map(|fence| fence.activation)),
            ("palw_fp_job_v5", *palw_fp_job_v5),
            ("palw_gen_range_twin_v1", *palw_gen_range_twin_v1),
            ("palw_task_heads_v1", palw_task_heads_v1.map(|fence| fence.activation)),
            ("palw_probabilistic_constraints_v1", *palw_probabilistic_constraints_v1),
            ("palw_funded_execution_v11", palw_funded_execution_v11.as_ref().map(|f| f.activation)),
            ("palw_stage_execution_v14", palw_stage_execution_v14.as_ref().map(|f| f.activation)),
            ("palw_native_final_v27", palw_native_final_v27.as_ref().map(|f| f.activation)),
            ("palw_stage_execution_v28", palw_stage_execution_v28.as_ref().map(|f| f.activation)),
            ("palw_native_execution_v39", palw_native_execution_v39.as_ref().map(|f| f.activation)),
            ("palw_native_mining_v39", palw_native_mining_v39.as_ref().map(|f| f.activation)),
            ("palw_fork_choice_rule_e_v1", *palw_fork_choice_rule_e_v1),
            ("palw_typed_roots_v1", *palw_typed_roots_v1),
            ("palw_fork_choice_commitment_v1", *palw_fork_choice_commitment_v1),
            ("palw_verifier_pay_v1", *palw_verifier_pay_v1),
            ("palw_model_refutation_seal_v1", *palw_model_refutation_seal_v1),
            ("palw_window_class_v37", *palw_window_class_v37),
            ("palw_beacon_source_opt_in_v1", palw_beacon_source_opt_in_v1.map(|f| f.activation)),
            ("palw_beacon_source_fee_v1", palw_beacon_source_fee_v1.map(|f| f.activation)),
            ("palw_panel_free_v1", palw_panel_free_v1.as_ref().map(|fence| fence.activation)),
            ("palw_signed_registration_v1", *palw_signed_registration_v1),
            ("palw_tir_shard_segment_v2", *palw_tir_shard_segment_v2),
            ("palw_provider_court_v1", *palw_provider_court_v1),
            ("palw_legacy_held_da_v2", *palw_legacy_held_da_v2),
            ("palw_bond_budget_v1", palw_bond_budget_v1.as_ref().map(|fence| fence.activation)),
            ("palw_reporter_income_v2", *palw_reporter_income_v2),
            ("palw_material_income_v3", *palw_material_income_v3),
            ("palw_settlement_once_v1", *palw_settlement_once_v1),
            ("palw_paid_claim_trap_v1", *palw_paid_claim_trap_v1),
            ("palw_native_liability_horizon_v1", *palw_native_liability_horizon_v1),
            ("palw_panel_lifetime_v2", palw_panel_lifetime_v2.as_ref().map(|f| f.activation)),
            ("palw_claim_challenge_v2", palw_claim_challenge_v2.as_ref().map(|f| f.activation)),
            ("palw_claim_compact_v4", palw_claim_compact_v4.as_ref().map(|f| f.activation)),
            ("palw_monetary_material_v1", palw_monetary_material_v1.as_ref().map(|f| f.activation)),
            ("palw_material_release_recovery_v2", palw_material_release_recovery_v2.as_ref().map(|f| f.activation)),
            ("palw_model_verification_gate_v1", palw_model_verification_gate_v1.as_ref().map(|f| f.activation)),
            ("palw_horizon_extension_v1", palw_horizon_extension_v1.as_ref().map(|f| f.activation)),
            ("palw_minimum_price_v1", palw_minimum_price_v1.as_ref().map(|f| f.activation)),
            ("palw_beacon_bootstrap_v1", palw_beacon_bootstrap_v1.as_ref().map(|f| f.activation)),
            ("palw_paid_consumer_lifecycle_v1", palw_paid_consumer_lifecycle_v1.as_ref().map(|f| f.activation)),
            ("palw_model_bond_allocation_v1", palw_model_bond_allocation_v1.as_ref().map(|fence| fence.activation)),
            ("palw_bond_formula_v2", palw_bond_formula_v2.map(|fence| fence.activation)),
            ("palw_reporter_share_v2", *palw_reporter_share_v2),
            ("palw_legacy_public_filer_v1", *palw_legacy_public_filer_v1),
            ("palw_court_model_scope_v1", *palw_court_model_scope_v1),
            ("palw_public_evidence_reward_v1", *palw_public_evidence_reward_v1),
            ("palw_fp_answer_accounting_v1", *palw_fp_answer_accounting_v1),
            ("palw_claim_hard_deadline_v1", palw_claim_hard_deadline_v1.as_ref().map(|f| f.activation)),
            ("palw_cell_coverage_closure_v1", *palw_cell_coverage_closure_v1),
            ("palw_g14_route_required_v1", *palw_g14_route_required_v1),
            ("palw_reward_liability_horizon_v1", *palw_reward_liability_horizon_v1),
            ("palw_court_capacity_v1", palw_court_capacity_v1.as_ref().map(|f| f.activation)),
            ("palw_court_transport_v1", palw_court_transport_v1.as_ref().map(|f| f.activation)),
            ("palw_unresolved_bound_v1", *palw_unresolved_bound_v1),
            ("palw_claim_local_settlement_v1", *palw_claim_local_settlement_v1),
            ("palw_model_block_rights_v1", *palw_model_block_rights_v1),
            ("palw_elastic_final_capacity_v1", *palw_elastic_final_capacity_v1),
            ("palw_model_eligibility_v1", *palw_model_eligibility_v1),
            ("palw_model_tier_v1", palw_model_tier_v1.as_ref().map(|fence| fence.activation)),
            ("palw_detection_policy_v1", palw_detection_policy_v1.as_ref().map(|fence| fence.activation)),
            ("palw_dag_sibling_attribution_v1", *palw_dag_sibling_attribution_v1),
            ("palw_panel_anchor_permissionless_v1", *palw_panel_anchor_permissionless_v1),
            ("palw_court_completion_attempt_v1", *palw_court_completion_attempt_v1),
            ("palw_rule_e_participation_v2", *palw_rule_e_participation_v2),
            ("palw_independently_work_certified_v1", *palw_independently_work_certified_v1),
            ("palw_gate_c_attestor_liability_v1", *palw_gate_c_attestor_liability_v1),
            ("palw_dag_width_schedule_v1", palw_dag_width_schedule_v1.as_ref().map(|f| f.activation)),
            // lane g14p/r7-fence-16000: five hashed G14 fences that were missing from this list (so from the fork-id schedule): a
            // height they took would have been invisible to the fork id. With every preset at `None` nothing moves.
            ("palw_tir_bounded_door_v1", *palw_tir_bounded_door_v1),
            ("palw_tir_designated_answers_v1", *palw_tir_designated_answers_v1),
            ("palw_court_outsider_lane_v1", *palw_court_outsider_lane_v1),
            ("palw_state_row_map_v1", *palw_state_row_map_v1),
            ("palw_kernel_route_panel_one_v1", *palw_kernel_route_panel_one_v1),
            ("palw_court_clock_v2", *palw_court_clock_v2),
            ("palw_panel_da_disclosure_v1", *palw_panel_da_disclosure_v1),
            ("palw_fp_prefix_inherit", *palw_fp_prefix_inherit),
            ("palw_fp_prefix_state", *palw_fp_prefix_state),
            ("palw_fp_tokenizer_match", *palw_fp_tokenizer_match),
            ("palw_fp_constraint_v2", *palw_fp_constraint_v2),
            ("palw_adapter_class_v1", *palw_adapter_class_v1),
            ("palw_tir_fence2", *palw_tir_fence2),
            ("palw_tir_shard_v1", *palw_tir_shard_v1),
            ("palw_floor_reserve_v1", *palw_floor_reserve_v1),
            ("palw_real_clock_tick_v1", *palw_real_clock_tick_v1),
            ("palw_anchor_window_v1", *palw_anchor_window_v1),
            ("palw_receipt_spend_v4", *palw_receipt_spend_v4),
            ("palw_evidence_court_v1", *palw_evidence_court_v1),
            ("palw_audit_1004_v1", *palw_audit_1004_v1),
            ("palw_improvement_v1", palw_improvement_v1.map(|fence| fence.activation)),
            ("palw_held_close_chunks_v1", *palw_held_close_chunks_v1),
            ("palw_verification_vertex_v1", *palw_verification_vertex_v1),
            ("palw_exec_payload_v2", *palw_exec_payload_v2),
            ("palw_witness_manifest_v1", *palw_witness_manifest_v1),
            ("palw_audit_mesh_v1", *palw_audit_mesh_v1),
            ("palw_capped_onboarding_v1", *palw_capped_onboarding_v1),
            ("palw_class_seating", palw_class_seating.map(|fence| fence.activation)),
            ("palw_tir_only_v1", *palw_tir_only_v1),
            ("palw_fused_dissectable", *palw_fused_dissectable),
            ("palw_attn_anchored_root", *palw_attn_anchored_root),
            ("palw_held_context", *palw_held_context),
            ("palw_prefill_draw", *palw_prefill_draw),
            ("palw_audit_2026_09_11", *palw_audit_2026_09_11),
            ("palw_audit_2026_09_11_deep", *palw_audit_2026_09_11_deep),
            ("palw_audit_2026_09_23", *palw_audit_2026_09_23),
            ("palw_difficulty_priced_rows", *palw_difficulty_priced_rows),
            ("palw_receipt_rows_unpriced", *palw_receipt_rows_unpriced),
            ("palw_attempt_header_pins", *palw_attempt_header_pins),
            ("palw_signature_contexts_v2", *palw_signature_contexts_v2),
            ("palw_heartbeat_transparent", *palw_heartbeat_transparent),
            // F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency)
            ("palw_heartbeat_transparent_same_chain", *palw_heartbeat_transparent_same_chain),
            // ADR-0160 lane escrow (post-launch): a top-level fence an un-upgraded peer does not
            // implement, so it is on the schedule and gates the fork id like every other.
            ("palw_capacity_escrow_at_licence", *palw_capacity_escrow_at_licence),
            ("palw_share_growth_final", *palw_share_growth_final),
            // P0a: a top-level fence an un-upgraded peer does not implement.
            ("palw_gdn_key_heads", *palw_gdn_key_heads),
        ]
    }

    /// **The activation schedule alone**, so a mismatch can be REPORTED precisely rather than only
    /// detected. Not a gate: two nodes with the same identity and different schedules are peers,
    /// and the operator log says which build is scheduling what.
    pub fn consensus_schedule_id(&self) -> Hash {
        let mut h = ConsensusParamsId::new();
        h.write(CONSENSUS_FINGERPRINT_DOMAIN_V1);
        h.write(b"schedule");
        let mut scores = self.clone();
        scores.for_each_fence(&mut |score| h.write(score.to_le_bytes()));
        // **ADR-0065 D1's window rides with its fence HERE, and only here.**
        //
        // `for_each_fence` must never see this duration — the identity visitor normalises every
        // value it visits to `0`/`u64::MAX`, which would make two builds shipping different windows
        // fingerprint identically at every height. But this id is explicitly *not a gate*: it
        // exists so a mismatch can be REPORTED precisely, and it is what `flow_context` prints when
        // it keeps a peer whose params id differs.
        //
        // Without the window the report was blind to exactly the disagreement it is the only
        // defence against: two operators scheduling D1 at one height with different windows saw the
        // warning fire and then read two IDENTICAL schedule ids, which says "these agree" about the
        // one thing that does not. Some-only, so an unset preset's schedule id is what it was.
        if let Some(maturity) = self.palw_bond_maturity {
            h.write(b"palw_bond_maturity_window");
            h.write(maturity.window_daa.to_le_bytes());
        }
        // Lane maturity (post-launch, 2026-09-26), NAMED and Some-only: it changes which bonds a claim
        // anchored past it may seat, so an operator reading the schedule must see which fence it is.
        if let Some(activation) = self.palw_bond_maturity_early {
            h.write(b"palw_bond_maturity_early");
            h.write(activation.daa_score().to_le_bytes());
        }
        // **ADR-0073 SA-1's width rides with its fence here, and only here** — same rule, same
        // reason as D1's window directly above. `k` must never reach `for_each_fence` (a visited
        // width would collapse to `0`/`u64::MAX` and make two builds folding different numbers of
        // blocks fingerprint identically at every height), and the operator log is the only place
        // a disagreement about it can surface while the fence is merely scheduled.
        if let Some(fold) = self.palw_beacon_fold {
            h.write(b"palw_beacon_fold_k");
            h.write([fold.k]);
        }
        // **ADR-0066 SA-4 — every other value that rides a fence, for the reason above and by the
        // rule above.**
        //
        // SA-4's hazard is real and it is the D1 hazard verbatim: two operators who schedule one
        // of these at one height with DIFFERENT values peered, synced, and then priced a lane (or
        // leaked a validator) differently the moment it fired — and the only warning they got
        // printed two IDENTICAL schedule ids, which says "these agree" about the one thing they do
        // not. The amendment is answered HERE and not in `consensus_identity_id`, and that is a
        // decision with a cost on each side, so it is written down:
        //
        // * **Gating on the value** (the value in the identity) refuses the handshake between a
        //   build that schedules the fence and a build that does not carry it. That makes ARMING a
        //   flag day. It also breaks a procedure this repo executed: the ADR-0068 Phase-1 rollout
        //   recorded around `PALW_RC_PHASE1_FENCE_DAA` scheduled `palw_heartbeat` and
        //   `palw_attempt_work` at DAA 5,000 with the tip at 1,746 and rolled the live t11 fleet
        //   host by host, explicitly because "a scheduled fence is normalised out of
        //   `consensus_identity_id`". Under a gate the FIRST host to restart partitions.
        // * **Reporting the value** (here) keeps that rollout and makes the disagreement visible in
        //   the one place an operator looks: the `flow_context` warning that already fires whenever
        //   the params ids differ and the identities agree.
        //
        // The two cannot both hold — `id(None) == id(Some(H, v))` and `id(Some(H, v1)) !=
        // id(Some(H, v2))` are contradictory for any equality-compared fingerprint — so this is a
        // choice, and the choice is the one `palw_bond_maturity` already made ten lines up. The
        // leak in particular must be on this side: a rule whose entire purpose is to let finality
        // self-heal after validator loss cannot be one that can only be armed by stopping every
        // node at once, which is exactly what the moment you need it makes impossible.
        //
        // Some-only, so a preset that leaves a fence unset writes what it always wrote.
        if let Some(beat) = self.palw_heartbeat {
            h.write(b"palw_heartbeat_price");
            h.write(beat.work_log2.to_le_bytes());
            h.write(beat.max_per_mergeset.to_le_bytes());
        }
        // ADR-0125: the lane's shape beside its height — Some-only, like the heartbeat's price.
        if let Some(lane) = self.palw_execution_lane {
            h.write(b"palw_execution_lane_shape");
            // ADR-0130: and the scheduler's rule set, by the SA-4 rule above — two builds that schedule
            // the lane at one height under two scheduler rules fold different schedules the moment it
            // fires, and this id is where the operator log can name that disagreement.
            h.write(b"scheduler_v2");
            h.write(lane.permits_per_round.to_le_bytes());
            h.write(lane.max_per_mergeset.to_le_bytes());
            h.write(lane.schedule_span_daa.to_le_bytes());
            // §7.2: a widening's width is a companion value; its height is visited as a fence.
            for stage in lane.widenings.iter().filter(|stage| stage.is_used()) {
                h.write(b"widening");
                h.write(stage.permits_per_round.to_le_bytes());
            }
            // The shortened length is a companion; its height is visited as a fence.
            if lane.short_span.is_used() {
                h.write(b"span_short");
                h.write(lane.short_span.schedule_span_daa.to_le_bytes());
            }
        }
        if let Some(attempt) = self.palw_attempt_work {
            h.write(b"palw_attempt_work_price");
            h.write(attempt.work_log2.to_le_bytes());
            h.write(attempt.ticket_bucket_log2.to_le_bytes());
        }
        // ADR-0128: NAMED, with its height, because its `for_each_fence` arm is Some-only (the
        // `palw_certification_rent` reason below); and the three leak numbers beside it, by the SA-4
        // rule above — two operators scheduling the gate at one height with different silences
        // leak different validators the moment it fires, and this is where the log can say so.
        if let Some(gate) = self.dns_bft_gate {
            h.write(b"dns_bft_gate");
            h.write(gate.activation.daa_score().to_le_bytes());
            h.write(gate.t_leak_daa.to_le_bytes());
            h.write(gate.reentry_final_depth_daa.to_le_bytes());
            h.write(gate.min_retained_validators.to_le_bytes());
        }
        if let Some(leak) = self.palw_inactivity_leak {
            h.write(b"palw_inactivity_leak_grace");
            h.write(leak.t_leak_daa.to_le_bytes());
            h.write(leak.reentry_final_depth_daa.to_le_bytes());
        }
        // **ADR-0075 SA-1/SA-2's fence, NAMED, because its `for_each_fence` arm is Some-only.**
        //
        // The sentinel that arm does not write is what kept absence from aliasing a present value
        // in the untagged sequence above; the name does the same job without charging the four
        // presets that leave the fence dormant eight bytes of schedule id for a rule none of them
        // schedule. Some-only, so an unset preset's schedule id is what it was before the field
        // existed — which is the whole point.
        if let Some(rent) = self.palw_certification_rent {
            h.write(b"palw_certification_rent");
            h.write(rent.daa_score().to_le_bytes());
        }
        // ADR-0081 Decision 3's fence, NAMED for the reason directly above: its `for_each_fence`
        // arm is Some-only, so the name is what keeps absence from aliasing a present value.
        if let Some(merkle) = self.palw_prompt_ids_merkle {
            h.write(b"palw_prompt_ids_merkle");
            h.write(merkle.daa_score().to_le_bytes());
        }
        // ADR-0082 Decision 3's fence, NAMED for the same reason.
        if let Some(kary) = self.palw_kary_court {
            h.write(b"palw_kary_court");
            h.write(kary.daa_score().to_le_bytes());
        }
        // The fused-terminal responder-coverage rule, NAMED for the same reason: its
        // `for_each_fence` arm is Some-only, so the name is what keeps absence from aliasing a
        // present value.
        if let Some(coverage) = self.palw_court_responder_coverage {
            h.write(b"palw_court_responder_coverage");
            h.write(coverage.daa_score().to_le_bytes());
        }
        // ADR-0082 Decisions 10/11's fence, NAMED for the same reason: it changes what a claim
        // earns and what a token may be, so an operator reading the schedule must see it.
        if let Some(decode) = self.palw_fp_decode_rules {
            h.write(b"palw_fp_decode_rules");
            h.write(decode.daa_score().to_le_bytes());
        }
        // ADR-0145 §5/§6's derived-work fence, NAMED for the same reason: it changes what a claim
        // is paid for and what a class must publish before it may be paid at all.
        if let Some(derived) = self.palw_fp_derived_work {
            h.write(b"palw_fp_derived_work");
            h.write(derived.daa_score().to_le_bytes());
        }
        // ADR-0096 Decisions 6–8's fence, NAMED for the same reason: it changes what token a
        // class may commit and how the answer is rendered, so an operator reading the schedule
        // must see it.
        if let Some(constraint) = self.palw_fp_decode_constraint {
            h.write(b"palw_fp_decode_constraint");
            h.write(constraint.daa_score().to_le_bytes());
        }
        // ADR-0099 Decision 5's fence, NAMED for the same reason: it changes what convicts.
        if let Some(court) = self.palw_shard_court {
            h.write(b"palw_shard_court");
            h.write(court.daa_score().to_le_bytes());
        }
        if let Some(court) = self.palw_memory_routing_court_v2 {
            h.write(b"palw_memory_routing_court_v2");
            h.write(court.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_class_registration_origin_v1 {
            h.write(b"palw_class_registration_origin_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(fence) = self.palw_selected_source_bootstrap_v1 {
            h.write(b"palw_selected_source_bootstrap_v1");
            h.write(fence.at.daa_score().to_le_bytes());
            h.write(fence.policy.id().as_byte_slice());
        }
        if let Some(fence) = self.palw_source_verifier_economics_v2 {
            h.write(b"palw_source_verifier_economics_v2");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policy.id().as_byte_slice());
        }
        if let Some(fence) = self.palw_paid_bootstrap_source_v6 {
            h.write(b"palw_paid_bootstrap_source_v6");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policy.id().as_slice());
        }
        // ADR-0100 Decision 4's fence, NAMED likewise: it changes what licenses a claim.
        if let Some(licensing) = self.palw_shard_licensing {
            h.write(b"palw_shard_licensing");
            h.write(licensing.daa_score().to_le_bytes());
        }
        // ADR-0102's fence, NAMED likewise: it changes which classes the court can try.
        if let Some(lift) = self.palw_token_lift {
            h.write(b"palw_token_lift");
            h.write(lift.daa_score().to_le_bytes());
        }
        if let Some(kimi) = self.palw_kimi_k3 {
            h.write(b"palw_kimi_k3");
            h.write(kimi.daa_score().to_le_bytes());
        }
        // RFC-0002 Phase F, NAMED likewise, with its value reported (not gated): two operators
        // scheduling the IR at one height with different ceilings see different schedule ids.
        if let Some(fence) = self.palw_tir_v1 {
            h.write(b"palw_tir_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // RFC-0003, NAMED likewise, with its value reported (not gated).
        if let Some(fence) = self.palw_gen_v1 {
            h.write(b"palw_gen_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // FP Job V5, NAMED likewise.
        if let Some(v5) = self.palw_fp_job_v5 {
            h.write(b"palw_fp_job_v5");
            h.write(v5.daa_score().to_le_bytes());
        }
        // The generative range twin, NAMED likewise.
        if let Some(at) = self.palw_gen_range_twin_v1 {
            h.write(b"palw_gen_range_twin_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // The task-head profile, NAMED likewise, with its value.
        if let Some(fence) = self.palw_task_heads_v1 {
            h.write(b"palw_task_heads_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        if let Some(fence) = &self.palw_stage_execution_v14 {
            h.write(b"palw_stage_execution_v14");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policies.root());
        }
        if let Some(fence) = &self.palw_native_final_v27 {
            h.write(b"palw_native_final_v27");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policy.root());
        }
        if let Some(fence) = &self.palw_stage_execution_v28 {
            h.write(b"palw_stage_execution_v28");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policies.root());
        }
        if let Some(fence) = &self.palw_native_execution_v39 {
            h.write(b"palw_native_execution_v39");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policies.root());
        }
        if let Some(fence) = &self.palw_native_mining_v39 {
            h.write(b"palw_native_mining_v39");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policy.root());
        }
        if let Some(fence) = &self.palw_funded_execution_v11 {
            h.write(b"palw_funded_execution_v11");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policies.root());
        }
        // The probabilistic-constraint fence, NAMED likewise.
        if let Some(at) = self.palw_probabilistic_constraints_v1 {
            h.write(b"palw_probabilistic_constraints_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // Rule E's fence (ADR-0178), NAMED likewise.
        if let Some(at) = self.palw_fork_choice_rule_e_v1 {
            h.write(b"palw_fork_choice_rule_e_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // The typed-roots fence, NAMED likewise.
        if let Some(at) = self.palw_typed_roots_v1 {
            h.write(b"palw_typed_roots_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // The fork-choice commitment fence, NAMED likewise.
        if let Some(at) = self.palw_fork_choice_commitment_v1 {
            h.write(b"palw_fork_choice_commitment_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // The verifier-pay fence, NAMED likewise.
        if let Some(at) = self.palw_verifier_pay_v1 {
            h.write(b"palw_verifier_pay_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_model_refutation_seal_v1 {
            h.write(b"palw_model_refutation_seal_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_window_class_v37 {
            h.write(b"palw_window_class_v37");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_beacon_source_opt_in_v1 {
            h.write(b"palw_beacon_source_opt_in_v1");
            h.write(at.activation.daa_score().to_le_bytes());
            h.write(at.policy.id());
        }
        if let Some(at) = self.palw_beacon_source_fee_v1 {
            h.write(b"palw_beacon_source_fee_v1");
            h.write(at.activation.daa_score().to_le_bytes());
            h.write(at.policy.id());
        }
        // The Panel-free fence, NAMED likewise.
        if let Some(fence) = &self.palw_panel_free_v1 {
            h.write(b"palw_panel_free_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // The signed-registration envelope's fence, NAMED likewise.
        if let Some(at) = self.palw_signed_registration_v1 {
            h.write(b"palw_signed_registration_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // RFC-0006 per-segment pricing's fence, NAMED likewise (Some-only: every preset leaves it None).
        if let Some(at) = self.palw_tir_shard_segment_v2 {
            h.write(b"palw_tir_shard_segment_v2");
            h.write(at.daa_score().to_le_bytes());
        }
        // ADR-0175's immutable registrations, NAMED likewise (its `for_each_fence` arm is Some-only).
        if let Some(at) = self.palw_model_immutable_v1 {
            h.write(b"palw_model_immutable_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_class_semantics_v1 {
            h.write(b"palw_class_semantics_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_court_commitment_scope_v1 {
            h.write(b"palw_court_commitment_scope_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_staged_public_court_v1 {
            h.write(b"palw_staged_public_court_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_staged_tir_route_v1 {
            h.write(b"palw_staged_tir_route_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_tir_public_conviction_v1 {
            h.write(b"palw_tir_public_conviction_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_tir_container_identity_v1 {
            h.write(b"palw_tir_container_identity_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(cost) = self.palw_tir_frame_cost_v1.as_ref() {
            h.write(b"palw_tir_frame_cost_v1");
            h.write(borsh::to_vec(cost).expect("PalwTirFrameCostV1 is borsh-serializable"));
        }
        if let Some(at) = self.palw_tir_bounded_door_v1 {
            h.write(b"palw_tir_bounded_door_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_tir_designated_answers_v1 {
            h.write(b"palw_tir_designated_answers_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_court_outsider_lane_v1 {
            h.write(b"palw_court_outsider_lane_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_state_row_map_v1 {
            h.write(b"palw_state_row_map_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_kernel_route_panel_one_v1 {
            h.write(b"palw_kernel_route_panel_one_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_court_clock_v2.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_court_clock_v2");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_panel_da_disclosure_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_panel_da_disclosure_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_staged_float_constraint_v1 {
            h.write(b"palw_staged_float_constraint_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // The provider court's fence (DA16), NAMED likewise.
        if let Some(at) = self.palw_provider_court_v1 {
            h.write(b"palw_provider_court_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // The legacy held DA fence (LG14-B), NAMED likewise.
        if let Some(at) = self.palw_legacy_held_da_v2 {
            h.write(b"palw_legacy_held_da_v2");
            h.write(at.daa_score().to_le_bytes());
        }
        // ADR-0176 / ADR-0177's fences (lane BUDGET), NAMED likewise, with their policies (Some-only: every preset leaves them None).
        if let Some(at) = self.palw_reporter_income_v2 {
            h.write(b"palw_reporter_income_v2");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(f) = &self.palw_paid_consumer_lifecycle_v1 {
            h.write(b"palw_paid_consumer_lifecycle_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.digest().as_bytes());
        }
        if let Some(f) = &self.palw_panel_lifetime_v2 {
            h.write(b"palw_panel_lifetime_v2");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = &self.palw_claim_challenge_v2 {
            h.write(b"palw_claim_challenge_v2");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id());
        }
        if let Some(f) = &self.palw_claim_compact_v4 {
            h.write(b"palw_claim_compact_v4");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id());
        }
        if let Some(f) = &self.palw_material_release_recovery_v2 {
            h.write(b"palw_material_release_recovery_v2");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = &self.palw_monetary_material_v1 {
            h.write(b"palw_monetary_material_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = &self.palw_model_verification_gate_v1 {
            h.write(b"palw_model_verification_gate_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = &self.palw_horizon_extension_v1 {
            h.write(b"palw_horizon_extension_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = &self.palw_minimum_price_v1 {
            h.write(b"palw_minimum_price_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = &self.palw_beacon_bootstrap_v1 {
            h.write(b"palw_beacon_bootstrap_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(at) = self.palw_material_income_v3 {
            h.write(b"palw_material_income_v3");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_settlement_once_v1 {
            h.write(b"palw_settlement_once_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_paid_claim_trap_v1 {
            h.write(b"palw_paid_claim_trap_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_native_liability_horizon_v1 {
            h.write(b"palw_native_liability_horizon_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(fence) = &self.palw_bond_budget_v1 {
            h.write(b"palw_bond_budget_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        if let Some(fence) = &self.palw_model_bond_allocation_v1 {
            h.write(b"palw_model_bond_allocation_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // ADR-0176 amendment (ユーザー判断 2026-10-15): the bond formula, NAMED with its policy. Some-only.
        if let Some(fence) = &self.palw_bond_formula_v2 {
            h.write(b"palw_bond_formula_v2");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // ADR-0032's amended reporter share (lane INTF), NAMED likewise, with the share it arms beside it (the SA-4 rule:
        // two builds arming one height at different shares must not print one schedule id). Some-only.
        if let Some(at) = self.palw_reporter_share_v2 {
            h.write(b"palw_reporter_share_v2");
            h.write(at.daa_score().to_le_bytes());
            h.write(crate::palw_state_v2::PALW_RCORE_REPORTER_REWARD_BPS_V2.to_le_bytes());
        }
        // The legacy route's public filer (lane LG14-A), NAMED likewise. Some-only.
        if let Some(at) = self.palw_legacy_public_filer_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_legacy_public_filer_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // The three fences of the 2026-10-12 rulings, NAMED likewise. Some-only.
        if let Some(at) = self.palw_court_model_scope_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_court_model_scope_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_public_evidence_reward_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_public_evidence_reward_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // Compute-claim audit CX-01/CX-02's answer accounting, NAMED likewise. Some-only.
        if let Some(at) = self.palw_fp_answer_accounting_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_fp_answer_accounting_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(f) = self.palw_claim_hard_deadline_v1.filter(|f| f.activation != ForkActivation::never()) {
            h.write(b"palw_claim_hard_deadline_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        // RFC-0006 R6-CELL round 4's coverage closure, NAMED likewise. Some-only.
        if let Some(at) = self.palw_cell_coverage_closure_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_cell_coverage_closure_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // The cross-lane G14 gates (`crate::palw_g14_gates_v1`), NAMED likewise. Some-only.
        if let Some(at) = self.palw_g14_route_required_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_g14_route_required_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_reward_liability_horizon_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_reward_liability_horizon_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // The court-capacity fence, NAMED likewise, with its policy. Some-only.
        if let Some(f) = self.palw_court_capacity_v1.filter(|f| f.activation != ForkActivation::never()) {
            h.write(b"palw_court_capacity_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = self.palw_court_transport_v1.filter(|f| f.activation != ForkActivation::never()) {
            h.write(b"palw_court_transport_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        // RFC-0023 stage 1, NAMED, Some-only.
        if let Some(at) = self.palw_unresolved_bound_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_unresolved_bound_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_claim_local_settlement_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_claim_local_settlement_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // RFC-0016's two dormant fences, NAMED likewise. Some-only.
        if let Some(at) = self.palw_model_block_rights_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_model_block_rights_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_elastic_final_capacity_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_elastic_final_capacity_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // RFC-0017's model eligibility, NAMED likewise. Some-only.
        if let Some(at) = self.palw_model_eligibility_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_model_eligibility_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // ADR-0179's model tiers, NAMED with their values and RFC-0019's catalog chain — EVERY edition's number, height and root, the
        // scheduled ones included: this id is a report, not a gate, so two builds that carry different next editions (or one carries
        // none) read as different schedules in the handshake's warning before the edition activates. Some-only.
        if let Some(fence) = self.palw_model_tier_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) {
            h.write(b"palw_model_tier_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // G14's per-class detection policy, NAMED with every policy's id. Some-only.
        if let Some(fence) = self.palw_detection_policy_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) {
            h.write(b"palw_detection_policy_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // RFC-0018's five fences, NAMED likewise. Some-only (the schedule's id beside its height).
        if let Some(at) = self.palw_dag_sibling_attribution_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_dag_sibling_attribution_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_panel_anchor_permissionless_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_panel_anchor_permissionless_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // The liveness fix L-COURT-01's fence: Some-only, named.
        if let Some(at) = self.palw_court_completion_attempt_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_court_completion_attempt_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_rule_e_participation_v2.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_rule_e_participation_v2");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_independently_work_certified_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_independently_work_certified_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = self.palw_gate_c_attestor_liability_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_gate_c_attestor_liability_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(f) = self.palw_dag_width_schedule_v1.filter(|f| f.activation != ForkActivation::never()) {
            h.write(b"palw_dag_width_schedule_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.schedule.id().as_bytes());
        }
        // palw_fp_prefix_inherit, NAMED likewise.
        if let Some(at) = self.palw_fp_prefix_inherit {
            h.write(b"palw_fp_prefix_inherit");
            h.write(at.daa_score().to_le_bytes());
        }
        // palw_fp_prefix_state, NAMED likewise.
        if let Some(at) = self.palw_fp_prefix_state {
            h.write(b"palw_fp_prefix_state");
            h.write(at.daa_score().to_le_bytes());
        }
        // palw_fp_tokenizer_match, NAMED likewise.
        if let Some(at) = self.palw_fp_tokenizer_match {
            h.write(b"palw_fp_tokenizer_match");
            h.write(at.daa_score().to_le_bytes());
        }
        // palw_fp_constraint_v2, NAMED likewise.
        if let Some(at) = self.palw_fp_constraint_v2 {
            h.write(b"palw_fp_constraint_v2");
            h.write(at.daa_score().to_le_bytes());
        }
        // palw_adapter_class_v1, NAMED likewise.
        if let Some(at) = self.palw_adapter_class_v1 {
            h.write(b"palw_adapter_class_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // RFC-0002 Phase F's second IR fence, NAMED likewise: it changes admission, the credited work
        // and the DA court from its height.
        if let Some(activation) = self.palw_tir_fence2 {
            h.write(b"palw_tir_fence2");
            h.write(activation.daa_score().to_le_bytes());
        }
        // RFC-0006's layer-sharded panels, NAMED likewise: it changes the draw, the receipts and the licence.
        if let Some(activation) = self.palw_tir_shard_v1 {
            h.write(b"palw_tir_shard_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0165, NAMED likewise: the reserve changes which attempts are admitted from its height (the
        // ledger's constants are part of the rule, so they are hashed with it); the tick changes the DAA.
        if let Some(activation) = self.palw_floor_reserve_v1 {
            h.write(b"palw_floor_reserve_v1");
            h.write(activation.daa_score().to_le_bytes());
            for v in crate::palw_real_share_v1::palw_floor_reserve_value_v1() {
                h.write(v.to_le_bytes());
            }
        }
        if let Some(activation) = self.palw_real_clock_tick_v1 {
            h.write(b"palw_real_clock_tick_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0170, NAMED likewise: the window changes which anchor the jury and the schedule seeding read from its height, and
        // whether a merged attempt anchors (its companion values are part of the rule, so they are hashed with it).
        if let Some(activation) = self.palw_anchor_window_v1 {
            h.write(b"palw_anchor_window_v1");
            h.write(activation.daa_score().to_le_bytes());
            for v in crate::palw_anchor_window_v1::palw_anchor_window_value_v1() {
                h.write(v.to_le_bytes());
            }
        }
        // RFC-0009, NAMED likewise: it changes what a receipt block's header may carry and how its reward is paid from its height, and
        // the fee cap is part of the rule (hashed with it, so changing the cap is a new network id).
        if let Some(activation) = self.palw_receipt_spend_v4 {
            h.write(b"palw_receipt_spend_v4");
            h.write(activation.daa_score().to_le_bytes());
            for v in crate::palw_receipt_v4::palw_receipt_spend_v4_value_v1() {
                h.write(v.to_le_bytes());
            }
        }
        // RFC-0009 stage B's provider court, NAMED likewise, its response window hashed with it.
        if let Some(activation) = self.palw_evidence_court_v1 {
            h.write(b"palw_evidence_court_v1");
            h.write(activation.daa_score().to_le_bytes());
            for v in crate::palw_evidence_court_v1::palw_evidence_court_value_v1() {
                h.write(v.to_le_bytes());
            }
        }
        // Lane PA, NAMED likewise: the audit-1004 rules change what the folds accept from its height; the beacon depth is part of the rule.
        if let Some(activation) = self.palw_audit_1004_v1 {
            h.write(b"palw_audit_1004_v1");
            h.write(activation.daa_score().to_le_bytes());
            for v in crate::palw_audit_1004_v1::palw_audit_1004_value_v1() {
                h.write(v.to_le_bytes());
            }
        }
        // RFC-0004, NAMED likewise, with its value reported (not gated).
        if let Some(fence) = self.palw_improvement_v1 {
            h.write(b"palw_improvement_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // RFC-0003's held leaf challenge, NAMED likewise: it changes which court objects open a session.
        if let Some(activation) = self.palw_held_close_chunks_v1 {
            h.write(b"palw_held_close_chunks_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // RFC-0007's verification vertex, NAMED likewise: it changes which objects a block accepts and how a claim licenses.
        if let Some(activation) = self.palw_verification_vertex_v1 {
            h.write(b"palw_verification_vertex_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // RFC-0008 v2's EXEC payload, NAMED likewise: it changes which carrier an EXEC block may be and what a chain block accepts.
        if let Some(activation) = self.palw_exec_payload_v2.filter(|activation| *activation != ForkActivation::never()) {
            h.write(b"palw_exec_payload_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_witness_manifest_v1 {
            h.write(b"palw_witness_manifest_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_audit_mesh_v1 {
            h.write(b"palw_audit_mesh_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_capped_onboarding_v1 {
            h.write(b"palw_capped_onboarding_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // RFC-0002 Part II's class-seating fence, NAMED likewise, with its value (the independence floor).
        if let Some(fence) = self.palw_class_seating {
            h.write(b"palw_class_seating");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // RFC-0002 Phase H's IR-only fence, NAMED likewise: it changes which classes may register.
        if let Some(activation) = self.palw_tir_only_v1 {
            h.write(b"palw_tir_only_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0093 Decision 6's fence, NAMED likewise: it changes which classes may register.
        if let Some(dissectable) = self.palw_fused_dissectable {
            h.write(b"palw_fused_dissectable");
            h.write(dissectable.daa_score().to_le_bytes());
        }
        // ADR-0093 Decision 8's fence, NAMED likewise: it changes which root claims open a phase.
        if let Some(anchored) = self.palw_attn_anchored_root {
            h.write(b"palw_attn_anchored_root");
            h.write(anchored.daa_score().to_le_bytes());
        }
        // ADR-0103's fence, NAMED likewise: it changes which court a claim meets and what a
        // registration must prove about its own growth.
        if let Some(held) = self.palw_held_context {
            h.write(b"palw_held_context");
            h.write(held.daa_score().to_le_bytes());
        }
        // ADR-0117's fence, NAMED likewise: it changes which job every attempt past it runs.
        if let Some(draw) = self.palw_prefill_draw {
            h.write(b"palw_prefill_draw");
            h.write(draw.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_audit_2026_09_11 {
            h.write(b"palw_audit_2026_09_11");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_audit_2026_09_11_deep {
            h.write(b"palw_audit_2026_09_11_deep");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_audit_2026_09_23 {
            h.write(b"palw_audit_2026_09_23");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(depth) = self.palw_settled_anchor_depth {
            h.write(b"palw_settled_anchor_depth");
            h.write(depth.to_le_bytes());
        }
        if let Some(period) = self.palw_admission_audit_period_daa {
            h.write(b"palw_admission_audit_period_daa");
            h.write(period.to_le_bytes());
        }
        // ADR-0151's stated maturity (user decision 2026-09-25), Some-only: reported here even where
        // `consensus_identity_id` drops it with a scheduled fence.
        if let Some(maturity) = self.palw_exec_quantum_maturity_daa {
            h.write(b"palw_exec_quantum_maturity_daa");
            h.write(maturity.to_le_bytes());
        }
        // The clock floor (H3/H5), NAMED: it changes which heartbeat headers and which clock steps
        // are valid, so an operator reading the schedule must see it. Some-only.
        if let Some(activation) = self.palw_clock_floor {
            h.write(b"palw_clock_floor");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The lead cap, NAMED: it changes which clock-moving headers a node admits now, so an operator
        // reading the schedule must see it. Some-only.
        if let Some(activation) = self.palw_clock_lead_cap {
            h.write(b"palw_clock_lead_cap");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0152 v2 F2's attribution fence, NAMED: it changes which objective offences a block may
        // carry and what one convicts, so an operator reading the schedule must see it. Some-only.
        if let Some(activation) = self.palw_offence_attribution {
            h.write(b"palw_offence_attribution");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0152 R-core+, NAMED for the same reason and Some-only: every other preset's schedule
        // id is byte-identical to a build without the fence.
        if let Some(activation) = self.palw_rcore_plus {
            h.write(b"palw_rcore_plus");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Lane F1 (the panel seed, post-launch), NAMED for the same reason and Some-only: it changes
        // which panel a claim anchored past it binds, so an operator reading the schedule must see it.
        if let Some(activation) = self.palw_panel_seed_execution {
            h.write(b"palw_panel_seed_execution");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Lane A (the operator anchor, post-launch), NAMED for the same reason and Some-only: it changes
        // which block anchors a claim past it, so an operator reading the schedule must see it.
        if let Some(rule) = &self.palw_operator_anchor {
            h.write(b"palw_operator_anchor");
            h.write(rule.activation.daa_score().to_le_bytes());
        }
        // Lane V02 (a resolved claim's lock off the work ceiling, post-launch), NAMED and Some-only: it
        // changes which binds, attempts and licences a bond's collateral admits past its height, so an
        // operator reading the schedule must see it, and a preset that leaves it `None` prints the id
        // of a build without the field.
        if let Some(activation) = self.palw_final_lock_full_collateral {
            h.write(b"palw_final_lock_full_collateral");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Lane V02 (post-launch), NAMED and Some-only: it changes how long a Valid seat's collateral is
        // locked past its height, so an operator reading the schedule must see it, and a preset that
        // leaves it `None` prints the id of a build without the field.
        if let Some(activation) = self.palw_final_lock_life {
            h.write(b"palw_final_lock_life");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0160 lane liab (F-L), NAMED for the same reason and Some-only, with every step's height:
        // it changes what a conviction takes and what a seat reserves, so an operator reading the
        // schedule must see it and each flag day that appends a step.
        if let Some(value) = &self.palw_capacity_aggregate_liability {
            h.write(b"palw_capacity_aggregate_liability");
            h.write(value.activation.daa_score().to_le_bytes());
            h.write((value.steps.len() as u64).to_le_bytes());
            for step in value.steps.iter() {
                h.write(step.from_daa.to_le_bytes());
            }
        }
        // Lane F2-lock (post-launch), NAMED and Some-only: at its height the crossing block re-dates every
        // long post-Final seat lock, so an operator reading the schedule must see it, and a preset that
        // leaves it `None` prints the id of a build without the field.
        if let Some(activation) = self.palw_final_lock_life_retro {
            h.write(b"palw_final_lock_life_retro");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0152 §4-quater's class-verify-deadline fence, NAMED for the same reason and Some-only: it
        // changes which claims a block may carry and when a licensed claim may Final.
        if let Some(activation) = self.palw_class_verify_deadline {
            h.write(b"palw_class_verify_deadline");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Lane bind-deadlock (post-launch), NAMED for the same reason and Some-only: it changes which
        // chain blocks a node disqualifies, so an operator reading the schedule must see it.
        if let Some(activation) = self.palw_anchor_at_ceiling {
            h.write(b"palw_anchor_at_ceiling");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Lane accept-order (post-launch), NAMED for the same reason and Some-only: it changes the order
        // a merging block applies its mergeset in, so an operator reading the schedule must see it.
        if let Some(activation) = self.palw_lane_accept_parents_first {
            h.write(b"palw_lane_accept_parents_first");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0152-adjacent (Activation Pool), NAMED for the same reason and Some-only, with its terms
        // beside the height for the SA-4 reason the carve's numbers are: two operators arming the
        // pool with different terms must see it in the log.
        if let Some(pool) = self.palw_activation_pool {
            h.write(b"palw_activation_pool");
            h.write(pool.activation.daa_score().to_le_bytes());
            palw_activation_pool_terms_write_v1(&mut h, &pool.terms);
        }
        // The readiness-V2 horizon (user decision 2026-09-25), NAMED and Some-only, with its spans
        // beside the height for the pool's SA-4 reason: two operators arming it with different
        // horizons count different seats ready and must see it in the log.
        if let Some(horizon) = self.palw_readiness_v2_max_age_spans {
            h.write(b"palw_readiness_v2_max_age_spans");
            h.write(horizon.activation.daa_score().to_le_bytes());
            h.write(horizon.max_age_spans.to_le_bytes());
        }
        // Lane F1 (registry resilience, V03/V05), NAMED and Some-only: it changes which claims void at
        // their anchor slot and how a probation is judged, so an operator reading the schedule must see it.
        if let Some(activation) = self.palw_registry_resilience {
            h.write(b"palw_registry_resilience");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Lane F2 (the floor-refusal retry, post-launch), NAMED and Some-only: it changes which claims void
        // in their anchor block past its height, so an operator reading the schedule must see it, and a
        // preset that leaves it `None` prints the id of a build without the field.
        if let Some(activation) = self.palw_floor_refusal_retry {
            h.write(b"palw_floor_refusal_retry");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0083 Decision 1's fence, NAMED for the same reason: it changes the bits every header
        // past it must carry, so an operator reading the schedule must see it.
        if let Some(priced) = self.palw_difficulty_priced_rows {
            h.write(b"palw_difficulty_priced_rows");
            h.write(priced.daa_score().to_le_bytes());
        }
        if let Some(receipts) = self.palw_receipt_rows_unpriced {
            h.write(b"palw_receipt_rows_unpriced");
            h.write(receipts.daa_score().to_le_bytes());
        }
        // ADR-0072 D8 at the header stage. Some-only: arming it changes which headers are valid,
        // so it belongs in the fingerprint, and every shipped preset leaves it `None` and hashes
        // byte-identically to a build without the field.
        if let Some(pins) = self.palw_attempt_header_pins {
            h.write(b"palw_attempt_header_pins");
            h.write(pins.daa_score().to_le_bytes());
        }
        // ADR-0072 D8 on the free-prompt lane. Some-only, for the reason above it.
        if let Some(pins) = self.palw_fp_da_pins {
            h.write(b"palw_fp_da_pins");
            h.write(pins.daa_score().to_le_bytes());
        }
        // ADR-0018 §E's payout bounds (mainnet audit 2026-09-06). NAMED for the same reason: they
        // change the coinbase every block past them must carry, in both directions, so an operator
        // reading the schedule must see the height.
        if let Some(bounds) = self.palw_validator_payout_bounds {
            h.write(b"palw_validator_payout_bounds");
            h.write(bounds.daa_score().to_le_bytes());
        }
        // MSK-26A: the slashing-evidence UTXO genuineness fence. Some-only, for its siblings'
        // reason: a preset that leaves it `None` writes nothing, so the schedule id does not move.
        if let Some(genuine) = self.palw_slashing_evidence_utxo_genuine {
            h.write(b"palw_slashing_evidence_utxo_genuine");
            h.write(genuine.daa_score().to_le_bytes());
        }
        // Fork choice and hf-pptake2 (post-launch, the int-4 audit's LOW): the two strict-economic-win
        // fences, NAMED and Some-only like their siblings. `for_each_fence` visits their heights
        // Some-only but unnamed, so the operator log could not say WHICH rule a peer schedules; named
        // here, a preset that leaves them `None` (every one but testnet-12) writes nothing and its
        // schedule id does not move.
        if let Some(activation) = self.palw_reorg_strict_economic_win {
            h.write(b"palw_reorg_strict_economic_win");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_pruning_proof_strict_economic_win {
            h.write(b"palw_pruning_proof_strict_economic_win");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0045 D2's boundary budget and ADR-0044 D9's free-prompt caps. Some-only, at the
        // tail, for the reason their siblings are: a preset that leaves them `None` prints the
        // schedule id of a build from before the fields existed.
        if let Some(activation) = self.palw_epoch_boundary_budget {
            h.write(b"palw_epoch_boundary_budget");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_epoch_budget_release {
            h.write(b"palw_epoch_budget_release");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_panel_economy {
            h.write(b"palw_panel_economy");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_work_priced_reward {
            h.write(b"palw_work_priced_reward");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(lane) = self.palw_execution_lane {
            h.write(b"palw_execution_lane");
            h.write(lane.activation.daa_score().to_le_bytes());
            for stage in lane.widenings.iter().filter(|stage| stage.is_used()) {
                h.write(b"widening");
                h.write(stage.activation.daa_score().to_le_bytes());
            }
        }
        // ADR-0126: the height, and beside it the two numbers it moves. Some-only, like its siblings;
        // reported here and never gated, for the SA-4 reason the lane's shape is.
        if let Some(r) = self.palw_dns_retirement {
            h.write(b"palw_dns_retirement_v1");
            h.write(r.commitment_bytes());
        }
        if let Some(r) = self.palw_permissionless_panel_v1.filter(|r| r.activation != ForkActivation::never()) {
            h.write(b"palw_permissionless_panel_v1");
            h.write(r.commitment_bytes());
        }
        if let Some(r) = self.palw_permissionless_panel_complete_v1.filter(|r| r.activation != ForkActivation::never()) {
            h.write(b"palw_permissionless_panel_complete_v1");
            h.write(r.commitment_bytes());
        }
        if let Some(r) = self.palw_permissionless_panel_retired_v1.filter(|r| r.activation != ForkActivation::never()) {
            h.write(b"palw_permissionless_panel_retired_v1");
            h.write(r.commitment_bytes());
        }
        if let Some(carve) = self.palw_overlay_carve {
            h.write(b"palw_overlay_carve");
            h.write(carve.activation.daa_score().to_le_bytes());
            h.write(carve.subsidy_validator_bps.to_le_bytes());
            h.write(carve.worker_carve_permille.to_le_bytes());
        }
        // ADR-0130: the height, and beside it the reward multiple it floors seats at. Some-only, and
        // reported here and never gated, for the SA-4 reason the carve's numbers are: two operators
        // scheduling the floor at one height with different multiples must see it in the log.
        if let Some(floor) = self.palw_panel_exposure_floor {
            h.write(b"palw_panel_exposure_floor");
            h.write(floor.activation.daa_score().to_le_bytes());
            h.write(floor.reward_multiple_permille.to_le_bytes());
        }
        if let Some(activation) = self.palw_fp_ruleset_caps {
            h.write(b"palw_fp_ruleset_caps");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Lane sink (the model sink binding, post-launch), NAMED and Some-only: it changes which
        // transactions a block may carry past its height, so an operator reading the schedule must
        // see it, and a preset that leaves it `None` prints the id of a build without the field.
        if let Some(activation) = self.palw_model_sink_bound {
            h.write(b"palw_model_sink_bound");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0162, NAMED and Some-only for lane sink's reason: it changes what the fold writes into the
        // market rows past its height, so an operator reading the schedule must see it, and a preset
        // that leaves it `None` prints the id of a build without the field.
        if let Some(activation) = self.palw_model_virtual_v1 {
            h.write(b"palw_model_virtual_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0160 lane verify (F-B, F-R), NAMED and Some-only for lane sink's reason: each changes what
        // a block may carry or admit past its height.
        if let Some(activation) = self.palw_capacity_batch_licence {
            h.write(b"palw_capacity_batch_licence");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_capacity_verify_room {
            h.write(b"palw_capacity_verify_room");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0160 stage 2 (F-Q, F-S), NAMED and Some-only: each changes what a block may carry or admit.
        if let Some(activation) = self.palw_capacity_audit_door {
            h.write(b"palw_capacity_audit_door");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_capacity_issuance_slots {
            h.write(b"palw_capacity_issuance_slots");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_capacity_network_room {
            h.write(b"palw_capacity_network_room");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_capacity_network_verify {
            h.write(b"palw_capacity_network_verify");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_panel_unavailable_expiry {
            h.write(b"palw_panel_unavailable_expiry");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_panel_fast_switch {
            h.write(b"palw_panel_fast_switch");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_seat_availability {
            h.write(b"palw_seat_availability");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_capacity_emission_budget {
            h.write(b"palw_capacity_emission_budget");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_capacity_multi_claim {
            h.write(b"palw_capacity_multi_claim");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = self.palw_capacity_rho_breaker {
            h.write(b"palw_capacity_rho_breaker");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0042 Decision 11's complete context set (mainnet audit 2026-09-06, M-8). Some-only,
        // at the tail, for the reason its siblings are: it is genesis-only, so an operator reading
        // the schedule sees a rule in force from block one rather than a height to cross.
        if let Some(activation) = self.palw_signature_contexts_v2 {
            h.write(b"palw_signature_contexts_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0105's coloring fence. Some-only, at the tail, for the reason its siblings are: it
        // changes which blocks are blue past its height, so an operator reading the schedule must
        // see the height, and a preset that leaves it `None` prints the schedule id of a build from
        // before the field existed.
        if let Some(activation) = self.palw_heartbeat_transparent {
            h.write(b"palw_heartbeat_transparent");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ---- F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
        // NAMED and Some-only: it changes which blocks are blue past its height, so an operator reading
        // the schedule must see the height; a preset that leaves it `None` prints the id it printed.
        if let Some(activation) = self.palw_heartbeat_transparent_same_chain {
            h.write(b"palw_heartbeat_transparent_same_chain");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ---- end F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
        // ADR-0160 lane escrow: NAMED and Some-only — it changes what a claim holds on its bond past
        // its height, so an operator reading the schedule must see it; `None` prints the id it printed.
        if let Some(activation) = self.palw_capacity_escrow_at_licence {
            h.write(b"palw_capacity_escrow_at_licence");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0107. Some-only, at the tail: it changes which epochs grow a class's share past its
        // height, so the schedule must show the height, and a preset that leaves it `None` prints
        // the id of a build from before the field existed.
        if let Some(activation) = self.palw_share_growth_final {
            h.write(b"palw_share_growth_final");
            h.write(activation.daa_score().to_le_bytes());
        }
        // P0a. Some-only, at the tail, NAMED: it changes which classes may register, and a preset
        // that leaves it `None` prints the id of a build from before the field existed.
        if let Some(activation) = self.palw_gdn_key_heads {
            h.write(b"palw_gdn_key_heads");
            h.write(activation.daa_score().to_le_bytes());
        }
        h.finalize()
    }

    /// **Every activation fence this build carries, in ONE exhaustive description.**
    ///
    /// Both `consensus_identity_id` and `consensus_schedule_id` are derived from this and nothing
    /// else, so the two can never drift, and — the part that matters (re-audit R-2) — every
    /// destructure below is **exhaustive**. A fence added to `Params`, `DnsParams`, `VltParams` or
    /// `TokenParams` later will not compile until someone classifies it here. The previous version
    /// was a hand-written list of assignments, and it had already fallen behind: `dns.vlt.*`,
    /// `dns.tkn.*`, `full_reward_split_daa_score` and `mandatory_attestation_inclusion_daa_score`
    /// were all in the fingerprint and none were normalised, so scheduling the VLT shadow fork —
    /// the very upgrade `TESTNET_VLT_SHADOW_FORK_DAA_SCORE` exists for, and the one this file
    /// documents as the next mainnet fork — still partitioned the network at deploy time.
    ///
    /// Order is the declaration order of each struct and must stay stable: the schedule id hashes
    /// these in sequence.
    ///
    /// **Not covered, deliberately:** the fences inside a `ConsensusV2` bundle. Those live under
    /// `palw_ruleset_id_v2`, which is one atomic commitment to the whole bundle by ADR-0042
    /// Decision 11, and a V2 network's own doctrine is to re-mint rather than schedule. Normalising
    /// into it would mean reaching through a hash that is deliberately opaque here.
    pub(super) fn for_each_fence(&mut self, visit: &mut impl FnMut(&mut u64)) {
        // A `ForkActivation` is a fence wearing a different type; round-trip it through its score
        // so the visitor sees one shape for all of them.
        fn fork(activation: &mut ForkActivation, visit: &mut dyn FnMut(&mut u64)) {
            let mut score = activation.daa_score();
            visit(&mut score);
            *activation = if score == u64::MAX { ForkActivation::never() } else { ForkActivation::new(score) };
        }
        // Stands in for an absent Some-only fence, so absence cannot alias a present value. Re-set
        // at each use because `visit` is free to write through it.
        let mut absent;
        // Exhaustive on purpose — see the doc block. Non-fence fields are bound and ignored.
        let Params {
            dns_seeders: _,
            net: _,
            genesis: _,
            timestamp_deviation_tolerance: _,
            max_difficulty_target: _,
            max_difficulty_target_f64: _,
            past_median_time_window_size: _,
            difficulty_window_size: _,
            min_difficulty_window_size: _,
            coinbase_payload_script_public_key_max_len: _,
            max_coinbase_payload_len: _,
            max_tx_inputs: _,
            max_tx_outputs: _,
            max_signature_script_len: _,
            max_script_public_key_len: _,
            mass_per_tx_byte: _,
            mass_per_script_pub_key_byte: _,
            mass_per_sig_op: _,
            max_block_mass: _,
            storage_mass_parameter: _,
            deflationary_phase_daa_score: _,
            pre_deflationary_phase_base_subsidy: _,
            skip_proof_of_work: _,
            max_block_level: _,
            pruning_proof_m: _,
            blockrate: _,
            pre_crescendo_target_time_per_block: _,
            crescendo_activation,
            dns_params,
            palw_credit,
            palw_fork_choice: _,
            palw_schedule: _,
            palw_ramp: _,
            palw_block_commitment,
            palw_bootstrap_activation,
            palw_unavailable_abstains,
            palw_bond_maturity,
            // Lane maturity (post-launch, 2026-09-26).
            palw_bond_maturity_early,
            palw_frontier_provenance,
            palw_reorg_strict_economic_win,
            palw_pruning_proof_strict_economic_win,
            palw_capacity_weight_cap,
            palw_heartbeat,
            palw_attempt_work,
            palw_attempt_activation,
            dns_bft_gate,
            palw_inactivity_leak,
            palw_beacon_fold,
            palw_capability_bound,
            palw_compute_overlay_retired,
            palw_model_registry,
            palw_economic_payout,
            palw_work_target,
            palw_single_lottery,
            palw_short_challenge_window,
            palw_verification_v2,
            palw_verification_s3,
            palw_verification_s2,
            palw_readiness_v2,
            palw_anchor_clock,
            palw_clock_cursor,
            palw_clock_floor,
            palw_clock_lead_cap,
            palw_offence_attribution,
            palw_rcore_plus,
            // Lane F1 (the panel seed, post-launch).
            palw_panel_seed_execution,
            // Lane A (the operator anchor, post-launch).
            palw_operator_anchor,
            // ADR-0160 lane liab (F-L).
            palw_capacity_aggregate_liability,
            palw_rcore_conservative_classes: _,
            // Lane V02 (a resolved claim's lock off the work ceiling, post-launch).
            palw_final_lock_full_collateral,
            // Lane V02 (the shortened post-Final lock life, post-launch).
            palw_final_lock_life,
            // Lane F2-lock (the lock life applied retroactively, post-launch).
            palw_final_lock_life_retro,
            palw_class_verify_deadline,
            palw_class_verify_rows: _,
            // Lane bind-deadlock (post-launch).
            palw_anchor_at_ceiling,
            // Lane accept-order (post-launch).
            palw_lane_accept_parents_first,
            palw_activation_pool,
            palw_readiness_v2_max_age_spans,
            palw_registry_resilience,
            // Lane F2 (the floor-refusal retry, post-launch).
            palw_floor_refusal_retry,
            palw_artifact_root_ownership,
            palw_operator_id_unique,
            palw_objective_offence,
            palw_seat_gate_possession,
            palw_class_receipt_window,
            palw_execution_quanta,
            palw_public_model_source_required,
            palw_economic_safety,
            palw_canonical_work,
            palw_admission_independence,
            palw_context_ladder,
            palw_panel_da,
            palw_certification_rent,
            palw_uncertified_weightless,
            palw_da_court,
            palw_court_ladder,
            palw_model_market,
            palw_model_lines,
            palw_model_immutable_v1,
            palw_class_semantics_v1,
            palw_court_commitment_scope_v1,
            palw_staged_public_court_v1,
            palw_staged_tir_route_v1,
            palw_tir_public_conviction_v1,
            palw_tir_container_identity_v1,
            palw_tir_frame_cost_v1: _,
            palw_tir_bounded_door_v1,
            palw_tir_designated_answers_v1,
            palw_court_outsider_lane_v1,
            palw_state_row_map_v1: _,
            palw_kernel_route_panel_one_v1,
            palw_court_clock_v2,
            palw_panel_da_disclosure_v1,
            palw_staged_float_constraint_v1,
            palw_model_benefits,
            palw_model_leg_v2,
            palw_model_seed_v2,
            palw_model_virtual_v1,
            palw_model_evm,
            // Lane sink (the model sink binding, post-launch).
            palw_model_sink_bound,
            // ADR-0160 lane verify (F-B, F-R).
            palw_capacity_batch_licence,
            palw_capacity_verify_room,
            palw_capacity_audit_door,
            palw_capacity_issuance_slots,
            palw_capacity_network_room,
            palw_capacity_network_verify,
            palw_panel_unavailable_expiry,
            palw_panel_fast_switch,
            palw_seat_availability,
            palw_capacity_emission_budget,
            palw_capacity_multi_claim,
            palw_capacity_rho_breaker,
            palw_chunk_cap_charge,
            palw_prompt_ids_merkle,
            palw_kary_court,
            palw_model_court_window,
            palw_court_responder_coverage,
            palw_fp_decode_rules,
            palw_fp_derived_work,
            palw_fp_decode_constraint,
            palw_shard_court,
            palw_memory_routing_court_v2,
            palw_class_registration_origin_v1,
            palw_selected_source_bootstrap_v1,
            palw_source_verifier_economics_v2,
            palw_paid_bootstrap_source_v6,
            palw_shard_licensing,
            palw_token_lift,
            palw_kimi_k3,
            palw_tir_v1,
            palw_gen_v1,
            palw_fp_job_v5,
            palw_gen_range_twin_v1,
            palw_task_heads_v1,
            palw_probabilistic_constraints_v1,
            palw_funded_execution_v11,
            palw_stage_execution_v14,
            palw_native_final_v27,
            palw_stage_execution_v28,
            palw_native_execution_v39,
            palw_native_mining_v39,
            palw_fork_choice_rule_e_v1,
            palw_typed_roots_v1,
            palw_fork_choice_commitment_v1,
            palw_verifier_pay_v1,
            palw_model_refutation_seal_v1,
            palw_window_class_v37,
            palw_beacon_source_opt_in_v1,
            palw_beacon_source_fee_v1,
            palw_panel_free_v1,
            palw_signed_registration_v1,
            palw_tir_shard_segment_v2,
            palw_provider_court_v1,
            palw_legacy_held_da_v2,
            palw_bond_budget_v1,
            palw_reporter_income_v2,
            palw_material_income_v3,
            palw_settlement_once_v1,
            palw_paid_claim_trap_v1,
            palw_native_liability_horizon_v1,
            palw_panel_lifetime_v2,
            palw_claim_challenge_v2,
            palw_claim_compact_v4,
            palw_monetary_material_v1,
            palw_material_release_recovery_v2,
            palw_model_verification_gate_v1,
            palw_horizon_extension_v1,
            palw_minimum_price_v1,
            palw_beacon_bootstrap_v1,
            palw_paid_consumer_lifecycle_v1,
            palw_model_bond_allocation_v1,
            palw_bond_formula_v2,
            palw_reporter_share_v2,
            palw_legacy_public_filer_v1,
            palw_court_model_scope_v1,
            palw_public_evidence_reward_v1,
            palw_fp_answer_accounting_v1,
            palw_claim_hard_deadline_v1,
            palw_cell_coverage_closure_v1,
            palw_g14_route_required_v1,
            palw_reward_liability_horizon_v1,
            palw_court_capacity_v1,
            palw_court_transport_v1,
            palw_unresolved_bound_v1,
            palw_claim_local_settlement_v1,
            palw_model_block_rights_v1,
            palw_elastic_final_capacity_v1,
            palw_model_eligibility_v1,
            palw_model_tier_v1,
            palw_detection_policy_v1,
            palw_dag_sibling_attribution_v1,
            palw_panel_anchor_permissionless_v1,
            palw_court_completion_attempt_v1,
            palw_rule_e_participation_v2,
            palw_independently_work_certified_v1,
            palw_gate_c_attestor_liability_v1,
            palw_dag_width_schedule_v1,
            palw_fp_prefix_inherit,
            palw_fp_prefix_state,
            palw_fp_tokenizer_match,
            palw_fp_constraint_v2,
            palw_adapter_class_v1,
            palw_tir_fence2,
            palw_tir_shard_v1,
            palw_floor_reserve_v1,
            palw_real_clock_tick_v1,
            palw_anchor_window_v1,
            palw_receipt_spend_v4,
            palw_evidence_court_v1,
            palw_audit_1004_v1,
            palw_improvement_v1,
            palw_held_close_chunks_v1,
            palw_verification_vertex_v1,
            palw_exec_payload_v2,
            palw_witness_manifest_v1,
            palw_audit_mesh_v1,
            palw_capped_onboarding_v1,
            palw_class_seating,
            palw_tir_only_v1,
            palw_fused_dissectable,
            palw_attn_anchored_root,
            palw_held_context,
            palw_prefill_draw,
            palw_audit_2026_09_11,
            palw_audit_2026_09_11_deep,
            palw_audit_2026_09_23,
            // A number beside the fences, not a fence: the visitor rewrites HEIGHTS, and a
            // settled-anchor depth is neither a height nor normalisable to one.
            palw_settled_anchor_depth: _,
            // A period in DAA beside the fences, not a height — the visitor has nothing to rewrite.
            palw_admission_audit_period_daa: _,
            // A maturity in DAA beside the fences (it rides `palw_economic_safety`'s height), not a
            // height — the visitor has nothing to rewrite.
            palw_exec_quantum_maturity_daa: _,
            palw_difficulty_priced_rows,
            palw_receipt_rows_unpriced,
            palw_attempt_header_pins,
            palw_signature_contexts_v2,
            palw_fp_da_pins,
            palw_validator_payout_bounds,
            palw_slashing_evidence_utxo_genuine,
            palw_epoch_boundary_budget,
            palw_epoch_budget_release,
            palw_panel_economy,
            palw_work_priced_reward,
            palw_execution_lane,
            palw_overlay_carve,
            palw_dns_retirement,
            palw_permissionless_panel_v1,
            palw_permissionless_panel_complete_v1,
            palw_permissionless_panel_retired_v1,
            palw_panel_exposure_floor,
            palw_fp_ruleset_caps,
            palw_heartbeat_transparent,
            // F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency)
            palw_heartbeat_transparent_same_chain,
            // ADR-0160 lane escrow (post-launch)
            palw_capacity_escrow_at_licence,
            palw_share_growth_final,
            palw_gdn_key_heads,
            // The V2 bundle's fences are inside `palw_ruleset_id_v2` — see the doc block.
            palw_consensus_mode: _,
            pow_blake2b_sha3_activation,
            pow_palw_activation,
            pow_palw_ollama_activation,
            pq_enforcement: _,
            pq_activation_daa_score,
            evm_activation_daa_score,
            evm_gas_pool_v2_activation_daa_score,
            evm_f002_withdraw_cap_activation_daa_score,
            evm_bridge_ledger_activation_daa_score,
            evm_f003_mldsa_verify_activation_daa_score,
            evm_typed_receipt_root_activation_daa_score,
        } = self;

        fork(crescendo_activation, visit);
        fork(pow_blake2b_sha3_activation, visit);
        fork(pow_palw_activation, visit);
        fork(pow_palw_ollama_activation, visit);
        visit(pq_activation_daa_score);
        visit(evm_activation_daa_score);
        visit(evm_gas_pool_v2_activation_daa_score);
        visit(evm_f002_withdraw_cap_activation_daa_score);
        // The bridge ledger: FINITE-ONLY, the clock floor's rule for a u64 fence. Visiting an inert
        // `u64::MAX` would put a new value into every preset's schedule id, testnet-11's included;
        // skipping it leaves every network that does not arm the ledger walking exactly as before.
        if *evm_bridge_ledger_activation_daa_score != u64::MAX {
            visit(evm_bridge_ledger_activation_daa_score);
        }
        visit(evm_f003_mldsa_verify_activation_daa_score);
        visit(evm_typed_receipt_root_activation_daa_score);

        // Some-only fences, so a network that installs neither still hashes as it did before the
        // fields existed. A sentinel stands in for `None` so absence cannot alias a present value.
        match palw_credit.as_mut() {
            Some(credit) => visit(&mut credit.activation_daa),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        match palw_block_commitment.as_mut() {
            Some(commitment) => visit(&mut commitment.activation_daa_score),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0064. Round-tripped through `fork` like every other `ForkActivation`, which means an
        // ARMED height normalises to `never()` and stays `Some` — see the collapse in
        // `normalize_values_a_scheduled_fence_drags_with_it`, without which this field would
        // partition the network it was placed at top level to keep together.
        match palw_bootstrap_activation.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0065 D4. A pure fence with no duration beside it, so visiting it is safe: the
        // identity visitor normalises a height, and a height is all this field carries. (Contrast
        // `inactivity_leak_daa`, deliberately NOT visited — normalising a DURATION would make two
        // builds shipping different windows fingerprint identically and fork silently.)
        match palw_unavailable_abstains.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0065 D1. ONLY the activation is visited; `window_daa` beside it is a duration and is
        // deliberately not — normalising it to `0`/`u64::MAX` would let two builds shipping
        // different windows fingerprint identically, peer, and then seat different panels. It
        // reaches `consensus_params_id` raw instead.
        match palw_bond_maturity.as_mut() {
            Some(maturity) => fork(&mut maturity.activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // Lane maturity (post-launch, 2026-09-26): SOME-ONLY — an absent fence visits nothing, so a
        // build carrying the field fingerprints as one without it; its `Some(never())` collapses in
        // `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_bond_maturity_early.as_mut() {
            fork(activation, visit);
        }
        // ADR-0065 D2. A pure fence with no payload, so visiting it is safe.
        match palw_frontier_provenance.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // lane: rcore/f1-forkchoice-attacks. A bare fence with no payload, visited SOME-ONLY — the
        // clock floor's rule and its reason: a `None` visited as a sentinel would put a new value
        // into every preset's schedule id (testnet-12's launch release included, schedule 93da24cc),
        // so a node update carrying this fence dormant would print a schedule the release does not.
        // Some-only, a dormant build is the release in every id: params, schedule, identity, fork id.
        if let Some(activation) = palw_reorg_strict_economic_win.as_mut() {
            fork(activation, visit);
        }
        // lane: rcore/hf-pptake2. A bare fence with no payload — visited SOME-ONLY, the sibling
        // rule of palw_reorg_strict_economic_win. A `u64::MAX`-for-absence arm would put eight
        // bytes into consensus_schedule_id on every preset that leaves the field None, so a build
        // carrying this dormant fence would print a different schedule id from the launch release
        // (which has no such field at all) for schedules that are identical — inverting the "these
        // schedules agree" signal the id exists to give, and breaking the discipline that a dormant
        // fence changes nothing an operator can observe. Absence cannot alias a present value:
        // consensus_params_id writes this fence again under its own NAME when it is Some.
        if let Some(activation) = palw_pruning_proof_strict_economic_win.as_mut() {
            fork(activation, visit);
        }
        // lane: rcore/cap-weight (ADR-0160 F-W). A bare fence, visited SOME-ONLY for the same
        // reason: dormant, the release's schedule id is untouched.
        if let Some(activation) = palw_capacity_weight_cap.as_mut() {
            fork(activation, visit);
        }
        // ADR-0066 Decisions 1 and 4. **Only the activation is visited.** `work_log2` and
        // `t_leak_daa` are a price and a duration; normalising either to `0`/`u64::MAX` would make
        // two builds shipping different values fingerprint identically at every height, which is
        // the `inactivity_leak_daa` trap this pair exists to escape.
        match palw_heartbeat.as_mut() {
            Some(heartbeat) => fork(&mut heartbeat.activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0068 Phase 1 (F2): activation only, `work_log2` stays out — the same rule as its
        // two siblings above, for the same reason.
        match palw_attempt_work.as_mut() {
            Some(attempt_work) => fork(&mut attempt_work.activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0072 SA-1..SA-4. A pure fence with no payload — the D2 rule, for the D2 reason: the
        // new lane's per-class target seed and its window's first `bits` are derived at the fence
        // from the fold, so there is no companion value that normalisation could hide.
        match palw_attempt_activation.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0128: the height only, and Some-only — the three leak numbers are values beside it (the
        // D1 rule), and a `u64::MAX`-for-absence arm would put eight bytes into every preset that
        // leaves the gate `None`. The schedule id names it, which keeps absence from aliasing.
        if let Some(gate) = dns_bft_gate.as_mut() {
            fork(&mut gate.activation, visit);
        }
        match palw_inactivity_leak.as_mut() {
            Some(leak) => fork(&mut leak.activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0073 SA-1: activation only. `k` is a WIDTH — normalising it to `0`/`u64::MAX` would
        // make two builds folding different numbers of attempt blocks fingerprint identically at
        // every height, which is the `inactivity_leak_daa` trap. It reaches `consensus_params_id`
        // and `consensus_schedule_id` raw instead.
        match palw_beacon_fold.as_mut() {
            Some(fold) => fork(&mut fold.activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0071 SA-1..SA-4. A pure fence with no duration beside it, so visiting it is safe:
        // the identity visitor normalises a height, and a height is all this field carries.
        match palw_capability_bound.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0134, a bare fence: the same treatment.
        match palw_compute_overlay_retired.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0135, a bare fence: the same treatment.
        match palw_model_registry.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0132 Upgrade C: the height only, Some-only — the rate and the shares are values beside it.
        if let Some(payout) = palw_economic_payout.as_mut() {
            fork(&mut payout.activation, visit);
        }
        // ADR-0137, a bare fence: the same treatment as the registry's.
        match palw_work_target.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0132 S, a bare fence: the same treatment again.
        match palw_single_lottery.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0132 §7.6, a bare fence: the same treatment again.
        match palw_short_challenge_window.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0133 Verification V2, a bare fence: the same treatment again.
        match palw_verification_v2.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        match palw_verification_s3.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        match palw_verification_s2.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0133 §11.2 readiness V2, a bare fence: the same treatment again.
        match palw_readiness_v2.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0138 the anchor clock, a bare fence: the same treatment again.
        match palw_anchor_clock.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0142 the clock cursor, the fence the anchor clock may not be armed without.
        match palw_clock_cursor.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // The clock floor (H3/H5). SOME-ONLY, unlike the cursor above: a `None` visited as a
        // sentinel would put a new value into every preset's schedule id, testnet-11's included.
        if let Some(activation) = palw_clock_floor.as_mut() {
            fork(activation, visit);
        }
        // The lead cap: SOME-ONLY, as the floor above and for its reason.
        if let Some(activation) = palw_clock_lead_cap.as_mut() {
            fork(activation, visit);
        }
        // ADR-0152 v2 F2's attribution fence: SOME-ONLY, as the floor above and for its reason.
        if let Some(activation) = palw_offence_attribution.as_mut() {
            fork(activation, visit);
        }
        // ADR-0152 R-core+: SOME-ONLY, as the floor above and for its reason.
        if let Some(activation) = palw_rcore_plus.as_mut() {
            fork(activation, visit);
        }
        // Lane F1 (the panel seed, post-launch): SOME-ONLY, as the floor above and for its reason; its
        // `Some(never())` collapses in `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_panel_seed_execution.as_mut() {
            fork(activation, visit);
        }
        // Lane A (the operator anchor, post-launch): the height only, SOME-ONLY, for the floor's reason;
        // its `Some(never())` collapses (list and all) in `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(rule) = palw_operator_anchor.as_mut() {
            fork(&mut rule.activation, visit);
        }
        // Lane V02 (post-launch): SOME-ONLY, as R-core+ above — a `None` visited through a sentinel
        // would move every preset's schedule id — and its `Some(never())` collapses in
        // `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_final_lock_full_collateral.as_mut() {
            fork(activation, visit);
        }
        // Lane V02 (post-launch): SOME-ONLY, as R-core+ above — a `None` visited through a sentinel
        // would move every preset's schedule id — and its `Some(never())` collapses in
        // `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_final_lock_life.as_mut() {
            fork(activation, visit);
        }
        // ADR-0160 lane liab (F-L): the height and EVERY step's height, SOME-ONLY, for the floor's
        // reason — a step appended by a later flag day is a rule change at its own height, so the fork
        // id must see it (the memory rule "a fence at a scheduled height is invisible to the fork id").
        // The whole value collapses from `Some(never())` in `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(value) = palw_capacity_aggregate_liability.as_mut() {
            fork(&mut value.activation, visit);
            for step in value.steps.iter_mut() {
                let mut at = ForkActivation::new(step.from_daa);
                fork(&mut at, visit);
                step.from_daa = at.daa_score();
            }
        }
        // Lane F2-lock (post-launch): SOME-ONLY, as R-core+ above — a `None` visited through a sentinel
        // would move every preset's schedule id — and its `Some(never())` collapses in
        // `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_final_lock_life_retro.as_mut() {
            fork(activation, visit);
        }
        // ADR-0152 §4-quater's class-verify-deadline fence: SOME-ONLY, as the floor above and for its
        // reason.
        if let Some(activation) = palw_class_verify_deadline.as_mut() {
            fork(activation, visit);
        }
        // Lane bind-deadlock (post-launch): SOME-ONLY, as the floor above and for its reason; its
        // `Some(never())` collapses in `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_anchor_at_ceiling.as_mut() {
            fork(activation, visit);
        }
        // Lane accept-order (post-launch): SOME-ONLY, as the floor above and for its reason; its
        // `Some(never())` collapses in `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_lane_accept_parents_first.as_mut() {
            fork(activation, visit);
        }
        // ADR-0152-adjacent (Activation Pool): the height only, SOME-ONLY, as the floor above and for
        // its reason — the terms are a value beside it (the D1 rule).
        if let Some(pool) = palw_activation_pool.as_mut() {
            fork(&mut pool.activation, visit);
        }
        // The readiness-V2 horizon (user decision 2026-09-25): the height only, SOME-ONLY, as the pool
        // above and for its reason — the spans are a value beside it (the D1 rule).
        if let Some(horizon) = palw_readiness_v2_max_age_spans.as_mut() {
            fork(&mut horizon.activation, visit);
        }
        // Lane F1 (registry resilience, V03/V05): SOME-ONLY, as the horizon above and for its reason.
        if let Some(activation) = palw_registry_resilience.as_mut() {
            fork(activation, visit);
        }
        // Lane F2 (the floor-refusal retry, post-launch): SOME-ONLY, as the horizon above and for its
        // reason; its `Some(never())` collapses in `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_floor_refusal_retry.as_mut() {
            fork(activation, visit);
        }
        // ADR-0143 the artifact-root ownership fence.
        match palw_artifact_root_ownership.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // The 2026-09-19 audit's operator-identity fence.
        match palw_operator_id_unique.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        match palw_objective_offence.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        match palw_class_receipt_window.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        match palw_seat_gate_possession.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        match palw_execution_quanta.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        match palw_public_model_source_required.as_mut() {
            Some(rule) => fork(&mut rule.activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0145's canonical-work fence.
        match palw_economic_safety.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        match palw_canonical_work.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // The 2026-09-19 audit's admission-independence fence (F3, ADR-0145 I3/I4).
        match palw_admission_independence.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0062. A pure fence with no payload beside it, so visiting it is safe — the identity
        // visitor normalises a height, and a height is all this field carries.
        match palw_da_court.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0084 U-08. A pure fence with no payload, the D2 shape again.
        match palw_court_ladder.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0087 Decision 6. A pure fence with no payload, the D2 shape again.
        match palw_model_market.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0175's immutable registrations: Some-only, NAMED in the schedule id, like every fence added
        // after the presets' schedule ids were pinned. (As merged from `pre` this arm visited a `u64::MAX`
        // sentinel when absent, the older D2 shape, and so moved the schedule id of every preset —
        // testnet-12's live `1678e073…` included — though no preset sets the fence; lane INTF.)
        if let Some(activation) = palw_model_immutable_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_class_semantics_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_court_commitment_scope_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_staged_public_court_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_staged_tir_route_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_tir_public_conviction_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_tir_container_identity_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_staged_float_constraint_v1.as_mut() {
            fork(activation, visit);
        }
        match palw_model_lines.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0095 §4.11 as corrected. A pure fence with no payload, the same shape.
        match palw_model_benefits.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0114. A pure fence, the same shape.
        match palw_model_leg_v2.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0120. A pure fence, the same shape.
        match palw_model_seed_v2.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0089 Decision 9. The same shape.
        match palw_model_evm.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // Lane sink (the model sink binding, post-launch): SOME-ONLY, unlike its market siblings
        // above — a `None` visited through the sentinel would move every preset's schedule id — and
        // its `Some(never())` collapses in `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_model_sink_bound.as_mut() {
            fork(activation, visit);
        }
        // ADR-0162: SOME-ONLY for lane sink's reason — a `None` visited through the sentinel would move
        // every preset's schedule id — and its `Some(never())` collapses in
        // `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_model_virtual_v1.as_mut() {
            fork(activation, visit);
        }
        // ADR-0160 lane verify (F-B, F-R): SOME-ONLY, for lane sink's reason, each collapsed from
        // `Some(never())` in `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_capacity_batch_licence.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_capacity_verify_room.as_mut() {
            fork(activation, visit);
        }
        // ADR-0160 stage 2 (F-Q, F-S): Some-only, collapsed from `Some(never())`.
        if let Some(activation) = palw_capacity_audit_door.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_capacity_issuance_slots.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_capacity_network_room.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_capacity_network_verify.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_panel_unavailable_expiry.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_panel_fast_switch.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_seat_availability.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_capacity_emission_budget.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_capacity_multi_claim.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_capacity_rho_breaker.as_mut() {
            fork(activation, visit);
        }
        // ADR-0069 Decision 7. A pure fence with no payload, so visiting it is safe — the same
        // shape as D2 beside it.
        match palw_uncertified_weightless.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0077 Phase B. A pure fence with no payload beside it, so visiting it is safe: the
        // identity visitor normalises a height, and a height is all this field carries.
        match palw_context_ladder.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0077 Decision 16. A pure fence with no duration beside it, so visiting it is safe,
        // for the reason directly above. Being visited is what lets a network SCHEDULE `PanelDa`
        // and stay peers with a build that has not: the identity collapses a future height, so the
        // two agree about every block either can produce until the height arrives.
        match palw_panel_da.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0075 SA-1/SA-2. A pure fence with no payload beside it, so visiting it is safe: both
        // prices are constants this binary enforces, and there is nothing for the identity
        // visitor's `0`/`u64::MAX` normalisation to flatten.
        //
        // **Visited SOME-ONLY, unlike its siblings above, and that is the fix rather than an
        // omission.** `consensus_schedule_id` hashes every value this walk visits, in sequence, so
        // an arm that stands a `u64::MAX` in for absence puts eight bytes into that hash on the
        // four presets that leave the field `None` — and a build carrying a dormant fence then
        // prints a different schedule id from a build without the field at all, for schedules that
        // are identical. That is exactly the "these agree / these do not" signal the id exists to
        // give, inverted, and it breaks the discipline that a dormant fence changes nothing an
        // operator can observe. Absence still cannot alias a present value, because
        // `consensus_schedule_id` writes this fence again under its own NAME when it is `Some`
        // (the `palw_bond_maturity_window` precedent) — the tag is what the sentinel was standing
        // in for, and it costs the dormant case nothing.
        //
        // The identity is untouched either way: the visitor mutates in place, and there is nothing
        // to mutate when the field is `None`.
        //
        // Every sibling arm above has the same defect and the same fix is not local to any one of
        // them — it wants `for_each_fence` to hand the visitor a name. Recorded for the integrator
        // rather than done here, because that signature is the merge hotspot of this whole batch.
        if let Some(activation) = palw_certification_rent.as_mut() {
            fork(activation, visit);
        }
        // ADR-0075 D14. A pure fence with no payload, so visiting it is safe — appended AFTER its
        // siblings because the schedule id hashes these in sequence and inserting into the middle
        // would move every preset's schedule id for a field none of them set.
        match palw_chunk_cap_charge.as_mut() {
            Some(activation) => fork(activation, visit),
            None => {
                absent = u64::MAX;
                visit(&mut absent);
            }
        }
        // ADR-0081 Decision 3. Visited SOME-ONLY, like the rent fence directly above and for its
        // reason: an arm that stands a `u64::MAX` in for absence would put eight bytes into
        // `consensus_schedule_id` on the four presets that leave the field `None`, and a build
        // carrying a dormant fence would print a different schedule id from a build without the
        // field at all. `consensus_params_id` and `consensus_schedule_id` both write this fence
        // again under its own NAME when it is `Some`, which is what the sentinel was standing in
        // for.
        if let Some(activation) = palw_prompt_ids_merkle.as_mut() {
            fork(activation, visit);
        }
        // ADR-0082 Decision 3. Visited SOME-ONLY, for the reason directly above.
        if let Some(activation) = palw_kary_court.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_model_court_window.as_mut() {
            fork(activation, visit);
        }
        // The fused-terminal responder-coverage rule. Visited SOME-ONLY, for the reason above it.
        if let Some(activation) = palw_court_responder_coverage.as_mut() {
            fork(activation, visit);
        }
        // ADR-0082 Decisions 10/11. Some-only, likewise.
        if let Some(activation) = palw_fp_decode_rules.as_mut() {
            fork(activation, visit);
        }
        // ADR-0145 §5/§6's derived-work fence. Some-only, likewise.
        if let Some(activation) = palw_fp_derived_work.as_mut() {
            fork(activation, visit);
        }
        // ADR-0096 Decisions 6–8. Some-only, likewise.
        if let Some(activation) = palw_fp_decode_constraint.as_mut() {
            fork(activation, visit);
        }
        // ADR-0099 Decision 5. Some-only, likewise.
        if let Some(activation) = palw_shard_court.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_memory_routing_court_v2.as_mut() {
            fork(activation, visit);
        }
        if let Some(at) = palw_class_registration_origin_v1.as_mut() {
            fork(at, visit);
        }
        if let Some(fence) = palw_selected_source_bootstrap_v1.as_mut() {
            fork(&mut fence.at, visit);
        }
        if let Some(fence) = palw_source_verifier_economics_v2.as_mut() {
            fork(&mut fence.activation, visit);
        }
        if let Some(fence) = palw_paid_bootstrap_source_v6.as_mut() {
            fork(&mut fence.activation, visit);
        }
        // ADR-0100 Decision 4. Some-only, likewise.
        if let Some(activation) = palw_shard_licensing.as_mut() {
            fork(activation, visit);
        }
        // ADR-0102. Some-only, likewise.
        if let Some(activation) = palw_token_lift.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_kimi_k3.as_mut() {
            fork(activation, visit);
        }
        // RFC-0002 Phase F. Some-only, and the ACTIVATION only: the value (prim set, court
        // version, ceilings) is a limit set, and normalising it to 0/u64::MAX would make two builds
        // shipping different ceilings fingerprint alike (the palw_heartbeat rule).
        if let Some(fence) = palw_tir_v1.as_mut() {
            fork(&mut fence.activation, visit);
        }
        // RFC-0003. Some-only, and the ACTIVATION only (the value is a limit set).
        if let Some(fence) = palw_gen_v1.as_mut() {
            fork(&mut fence.activation, visit);
        }
        // FP Job V5. Some-only.
        if let Some(activation) = palw_fp_job_v5.as_mut() {
            fork(activation, visit);
        }
        // The generative range twin. Some-only.
        if let Some(activation) = palw_gen_range_twin_v1.as_mut() {
            fork(activation, visit);
        }
        // The task-head profile. Some-only.
        if let Some(fence) = palw_task_heads_v1.as_mut() {
            fork(&mut fence.activation, visit);
        }
        if let Some(fence) = palw_stage_execution_v14.as_mut() {
            fork(&mut fence.activation, visit);
        }
        if let Some(fence) = palw_native_final_v27.as_mut() {
            fork(&mut fence.activation, visit);
        }
        if let Some(fence) = palw_stage_execution_v28.as_mut() {
            fork(&mut fence.activation, visit);
        }
        if let Some(fence) = palw_native_execution_v39.as_mut() {
            fork(&mut fence.activation, visit);
        }
        if let Some(fence) = palw_native_mining_v39.as_mut() {
            fork(&mut fence.activation, visit);
        }
        if let Some(fence) = palw_funded_execution_v11.as_mut() {
            fork(&mut fence.activation, visit);
        }
        // The probabilistic-constraint fence. Some-only.
        if let Some(activation) = palw_probabilistic_constraints_v1.as_mut() {
            fork(activation, visit);
        }
        // Rule E's fence. Some-only.
        if let Some(activation) = palw_fork_choice_rule_e_v1.as_mut() {
            fork(activation, visit);
        }
        // The typed-roots fence. Some-only.
        if let Some(activation) = palw_typed_roots_v1.as_mut() {
            fork(activation, visit);
        }
        // The fork-choice commitment fence. Some-only.
        if let Some(activation) = palw_fork_choice_commitment_v1.as_mut() {
            fork(activation, visit);
        }
        // The verifier-pay fence. Some-only.
        if let Some(activation) = palw_verifier_pay_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_model_refutation_seal_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_window_class_v37.as_mut() {
            fork(activation, visit);
        }
        if let Some(fence) = palw_beacon_source_opt_in_v1.as_mut() {
            fork(&mut fence.activation, visit);
        }
        if let Some(fence) = palw_beacon_source_fee_v1.as_mut() {
            fork(&mut fence.activation, visit);
        }
        // The Panel-free fence. Some-only.
        if let Some(fence) = palw_panel_free_v1.as_mut() {
            fork(&mut fence.activation, visit);
        }
        // The signed-registration envelope's fence. Some-only.
        if let Some(activation) = palw_signed_registration_v1.as_mut() {
            fork(activation, visit);
        }
        // RFC-0006 per-segment pricing's fence. Some-only.
        if let Some(activation) = palw_tir_shard_segment_v2.as_mut() {
            fork(activation, visit);
        }
        // The provider court's fence (DA16). Some-only.
        if let Some(activation) = palw_provider_court_v1.as_mut() {
            fork(activation, visit);
        }
        // The legacy held DA fence (LG14-B). Some-only.
        if let Some(activation) = palw_legacy_held_da_v2.as_mut() {
            fork(activation, visit);
        }
        // ADR-0176 / ADR-0177's fences. Some-only; the activation alone.
        if let Some(at) = palw_reporter_income_v2.as_mut() {
            fork(at, visit);
        }
        if let Some(f) = palw_paid_consumer_lifecycle_v1.as_mut() {
            fork(&mut f.activation, visit);
        }
        if let Some(f) = palw_panel_lifetime_v2.as_mut() {
            fork(&mut f.activation, visit);
        }
        if let Some(f) = palw_claim_challenge_v2.as_mut() {
            fork(&mut f.activation, visit);
        }
        if let Some(f) = palw_claim_compact_v4.as_mut() {
            fork(&mut f.activation, visit);
        }
        if let Some(f) = palw_material_release_recovery_v2.as_mut() {
            fork(&mut f.activation, visit);
        }
        if let Some(f) = palw_monetary_material_v1.as_mut() {
            fork(&mut f.activation, visit);
        }
        if let Some(f) = palw_model_verification_gate_v1.as_mut() {
            fork(&mut f.activation, visit);
        }
        if let Some(f) = palw_horizon_extension_v1.as_mut() {
            fork(&mut f.activation, visit);
        }
        if let Some(f) = palw_minimum_price_v1.as_mut() {
            fork(&mut f.activation, visit);
        }
        if let Some(f) = palw_beacon_bootstrap_v1.as_mut() {
            fork(&mut f.activation, visit);
        }
        if let Some(at) = palw_material_income_v3.as_mut() {
            fork(at, visit);
        }
        if let Some(at) = palw_settlement_once_v1.as_mut() {
            fork(at, visit);
        }
        // Lane FLEET (2026-10-26): full-activation fences the visitor skipped, so an armed future height reached the handshake
        // identity and a DAA-17,000 build could not peer with the fleet below 17,000.
        if let Some(at) = palw_kernel_route_panel_one_v1.as_mut() {
            fork(at, visit);
        }
        if let Some(at) = palw_tir_bounded_door_v1.as_mut() {
            fork(at, visit);
        }
        if let Some(at) = palw_tir_designated_answers_v1.as_mut() {
            fork(at, visit);
        }
        if let Some(at) = palw_court_outsider_lane_v1.as_mut() {
            fork(at, visit);
        }
        if let Some(at) = palw_court_clock_v2.as_mut() {
            fork(at, visit);
        }
        if let Some(at) = palw_panel_da_disclosure_v1.as_mut() {
            fork(at, visit);
        }
        if let Some(at) = palw_paid_claim_trap_v1.as_mut() {
            fork(at, visit);
        }
        if let Some(at) = palw_native_liability_horizon_v1.as_mut() {
            fork(at, visit);
        }
        if let Some(fence) = palw_bond_budget_v1.as_mut() {
            fork(&mut fence.activation, visit);
        }
        if let Some(fence) = palw_model_bond_allocation_v1.as_mut() {
            fork(&mut fence.activation, visit);
        }
        // ADR-0176 amendment (ユーザー判断 2026-10-15). Some-only.
        if let Some(fence) = palw_bond_formula_v2.as_mut() {
            fork(&mut fence.activation, visit);
        }
        // ADR-0032's amended reporter share (lane INTF). Some-only.
        if let Some(activation) = palw_reporter_share_v2.as_mut() {
            fork(activation, visit);
        }
        // The legacy route's public filer (lane LG14-A). Some-only.
        if let Some(activation) = palw_legacy_public_filer_v1.as_mut() {
            fork(activation, visit);
        }
        // The three fences of the 2026-10-12 rulings. Some-only.
        if let Some(activation) = palw_court_model_scope_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_public_evidence_reward_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_fp_answer_accounting_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(f) = palw_claim_hard_deadline_v1.as_mut() {
            fork(&mut f.activation, visit);
        }
        // RFC-0006 R6-CELL round 4's coverage closure. Some-only.
        if let Some(activation) = palw_cell_coverage_closure_v1.as_mut() {
            fork(activation, visit);
        }
        // The cross-lane G14 gates. Some-only.
        if let Some(activation) = palw_g14_route_required_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_reward_liability_horizon_v1.as_mut() {
            fork(activation, visit);
        }
        // The court-capacity fence. Some-only.
        if let Some(f) = palw_court_capacity_v1.as_mut() {
            fork(&mut f.activation, visit);
        }
        if let Some(f) = palw_court_transport_v1.as_mut().filter(|f| f.activation != ForkActivation::never()) {
            fork(&mut f.activation, visit);
        }
        // RFC-0023 stage 1. Some-only.
        if let Some(activation) = palw_unresolved_bound_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_claim_local_settlement_v1.as_mut() {
            fork(activation, visit);
        }
        // RFC-0016's two dormant fences. Some-only.
        if let Some(activation) = palw_model_block_rights_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_elastic_final_capacity_v1.as_mut() {
            fork(activation, visit);
        }
        // RFC-0017's model eligibility. Some-only.
        if let Some(activation) = palw_model_eligibility_v1.as_mut() {
            fork(activation, visit);
        }
        // ADR-0179's model tiers: the fence and every RFC-0019 annual catalog edition's activation (edition 1 onward — edition 0 is
        // the genesis catalog at DAA 0, not a fence). Some-only.
        if let Some(fence) = palw_model_tier_v1.as_mut() {
            fork(&mut fence.activation, visit);
            for catalog in fence.catalogs.editions.iter_mut().skip(1) {
                visit(&mut catalog.activation_daa);
            }
        }
        // G14's per-class detection policy. Some-only.
        if let Some(fence) = palw_detection_policy_v1.as_mut() {
            fork(&mut fence.activation, visit);
        }
        // RFC-0018's five fences. Some-only.
        if let Some(activation) = palw_dag_sibling_attribution_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_panel_anchor_permissionless_v1.as_mut() {
            fork(activation, visit);
        }
        // The liveness fix L-COURT-01's fence. Some-only.
        if let Some(activation) = palw_court_completion_attempt_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_rule_e_participation_v2.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_independently_work_certified_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_gate_c_attestor_liability_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(f) = palw_dag_width_schedule_v1.as_mut() {
            fork(&mut f.activation, visit);
        }
        // palw_fp_prefix_inherit. Some-only.
        if let Some(activation) = palw_fp_prefix_inherit.as_mut() {
            fork(activation, visit);
        }
        // palw_fp_prefix_state. Some-only.
        if let Some(activation) = palw_fp_prefix_state.as_mut() {
            fork(activation, visit);
        }
        // palw_fp_tokenizer_match. Some-only.
        if let Some(activation) = palw_fp_tokenizer_match.as_mut() {
            fork(activation, visit);
        }
        // palw_fp_constraint_v2. Some-only.
        if let Some(activation) = palw_fp_constraint_v2.as_mut() {
            fork(activation, visit);
        }
        // palw_adapter_class_v1. Some-only.
        if let Some(activation) = palw_adapter_class_v1.as_mut() {
            fork(activation, visit);
        }
        // RFC-0002 Phase F's second IR fence: Some-only, a bare height.
        if let Some(activation) = palw_tir_fence2.as_mut() {
            fork(activation, visit);
        }
        // RFC-0006's layer-sharded panels: Some-only, a bare height.
        if let Some(activation) = palw_tir_shard_v1.as_mut() {
            fork(activation, visit);
        }
        // ADR-0165's two fences: Some-only, bare heights.
        if let Some(activation) = palw_floor_reserve_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_real_clock_tick_v1.as_mut() {
            fork(activation, visit);
        }
        // ADR-0170's window: Some-only, a bare height.
        if let Some(activation) = palw_anchor_window_v1.as_mut() {
            fork(activation, visit);
        }
        // RFC-0009's receipt redemption: Some-only, a bare height.
        if let Some(activation) = palw_receipt_spend_v4.as_mut() {
            fork(activation, visit);
        }
        // RFC-0009 stage B's provider court: Some-only, a bare height.
        if let Some(activation) = palw_evidence_court_v1.as_mut() {
            fork(activation, visit);
        }
        // Lane PA's audit-1004 fence: Some-only, a bare height.
        if let Some(activation) = palw_audit_1004_v1.as_mut() {
            fork(activation, visit);
        }
        // RFC-0004. Some-only, and the ACTIVATION only (the value is a limit set).
        if let Some(fence) = palw_improvement_v1.as_mut() {
            fork(&mut fence.activation, visit);
        }
        // RFC-0003's held leaf challenge. Some-only, a bare height.
        if let Some(activation) = palw_held_close_chunks_v1.as_mut() {
            fork(activation, visit);
        }
        // RFC-0007's verification vertex. Some-only, a bare height.
        if let Some(activation) = palw_verification_vertex_v1.as_mut() {
            fork(activation, visit);
        }
        // RFC-0008 v2's EXEC payload. Some-only, a bare height.
        // (`Some(never())` is not visited: it is the dormant spelling, so the schedule reports it as it reports `None`.)
        if let Some(activation) = palw_exec_payload_v2.as_mut().filter(|activation| **activation != ForkActivation::never()) {
            fork(activation, visit);
        }
        if let Some(activation) = palw_witness_manifest_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_audit_mesh_v1.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_capped_onboarding_v1.as_mut() {
            fork(activation, visit);
        }
        // RFC-0002 Part II's class-seating fence. Some-only, and the ACTIVATION only (the value is a floor).
        if let Some(fence) = palw_class_seating.as_mut() {
            fork(&mut fence.activation, visit);
            if let Some(raise) = fence.raise.as_mut() {
                fork(&mut raise.activation, visit);
            }
        }
        // RFC-0002 Phase H's IR-only fence. Some-only, a bare height.
        if let Some(activation) = palw_tir_only_v1.as_mut() {
            fork(activation, visit);
        }
        // ADR-0093 Decision 6. Some-only, likewise.
        if let Some(activation) = palw_fused_dissectable.as_mut() {
            fork(activation, visit);
        }
        // ADR-0093 Decision 8. Some-only, likewise.
        if let Some(activation) = palw_attn_anchored_root.as_mut() {
            fork(activation, visit);
        }
        // ADR-0103. Some-only, likewise.
        if let Some(activation) = palw_held_context.as_mut() {
            fork(activation, visit);
        }
        // ADR-0117. Some-only, likewise.
        if let Some(activation) = palw_prefill_draw.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_audit_2026_09_11.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_audit_2026_09_11_deep.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_audit_2026_09_23.as_mut() {
            fork(activation, visit);
        }
        // ADR-0083 Decision 1. Some-only, likewise.
        if let Some(activation) = palw_difficulty_priced_rows.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_receipt_rows_unpriced.as_mut() {
            fork(activation, visit);
        }
        // ADR-0072 D8 at the header stage. Some-only, likewise — a scheduled height must normalise
        // so an armed and an un-armed build peer until it fires.
        if let Some(activation) = palw_attempt_header_pins.as_mut() {
            fork(activation, visit);
        }
        // ADR-0072 D8 on the free-prompt lane. Some-only and appended AFTER its siblings, for the
        // reason `palw_chunk_cap_charge` states: the schedule id hashes these in sequence, and a
        // `u64::MAX`-for-absence arm would put eight bytes into every preset that leaves it `None`.
        if let Some(activation) = palw_fp_da_pins.as_mut() {
            fork(activation, visit);
        }
        // ADR-0018 §E's payout bounds (mainnet audit 2026-09-06). Some-only, like its siblings:
        // an unset fence puts no bytes into `consensus_schedule_id`, so a build carrying the field
        // prints the schedule id of a build without it.
        if let Some(activation) = palw_validator_payout_bounds.as_mut() {
            fork(activation, visit);
        }
        // MSK-26A: the slashing-evidence UTXO genuineness fence. Some-only, like its siblings.
        if let Some(activation) = palw_slashing_evidence_utxo_genuine.as_mut() {
            fork(activation, visit);
        }
        // ADR-0045 D2's boundary budget and ADR-0044 D9's free-prompt caps. Some-only and at the
        // tail, for their siblings' reason: a `u64::MAX`-for-absence arm would put eight bytes
        // into every preset that leaves them `None`.
        if let Some(activation) = palw_epoch_boundary_budget.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_epoch_budget_release.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_panel_economy.as_mut() {
            fork(activation, visit);
        }
        if let Some(activation) = palw_work_priced_reward.as_mut() {
            fork(activation, visit);
        }
        // ADR-0125: the activation and each widening's height — the lane's shape and the widths are
        // values beside the fences, not fences. An unused slot's `never()` is visited like any unset
        // height and normalises to itself. The span-short height is Some-only: unused writes nothing.
        if let Some(lane) = palw_execution_lane.as_mut() {
            fork(&mut lane.activation, visit);
            for stage in lane.widenings.iter_mut() {
                fork(&mut stage.activation, visit);
            }
            if lane.short_span.is_used() {
                fork(&mut lane.short_span.activation, visit);
            }
        }
        // ADR-0126: the height only — the validator share and the escrow carve are values beside it.
        if let Some(r) = palw_permissionless_panel_v1.as_mut().filter(|r| r.activation != ForkActivation::never()) {
            fork(&mut r.activation, visit);
        }
        if let Some(r) = palw_permissionless_panel_complete_v1.as_mut().filter(|r| r.activation != ForkActivation::never()) {
            fork(&mut r.activation, visit);
        }
        if let Some(r) = palw_permissionless_panel_retired_v1.as_mut().filter(|r| r.activation != ForkActivation::never()) {
            fork(&mut r.activation, visit);
        }
        if let Some(r) = palw_dns_retirement.as_mut() {
            fork(&mut r.activation, visit);
        }
        if let Some(carve) = palw_overlay_carve.as_mut() {
            fork(&mut carve.activation, visit);
        }
        // ADR-0130: the height only, Some-only — the reward multiple is a value beside it (the D1
        // rule), and a `u64::MAX`-for-absence arm would put eight bytes into every preset that
        // leaves the floor `None`.
        if let Some(floor) = palw_panel_exposure_floor.as_mut() {
            fork(&mut floor.activation, visit);
        }
        if let Some(activation) = palw_fp_ruleset_caps.as_mut() {
            fork(activation, visit);
        }
        // ADR-0042 Decision 11's complete context set. Some-only and at the tail, for the reason
        // `palw_chunk_cap_charge` states: the schedule id hashes these in sequence, and a
        // `u64::MAX`-for-absence arm would put eight bytes into every preset that leaves it `None`.
        if let Some(activation) = palw_signature_contexts_v2.as_mut() {
            fork(activation, visit);
        }
        // ADR-0105's coloring fence. Some-only and at the tail, for the reason
        // `palw_chunk_cap_charge` states: the schedule id hashes these in sequence, and a
        // `u64::MAX`-for-absence arm would put eight bytes into every preset that leaves it `None`.
        // Visited, so a scheduled height normalises out of the identity (a rolling deploy peers
        // until it fires) and joins `fence_schedule_v1`, which is what the fork id advertises.
        if let Some(activation) = palw_heartbeat_transparent.as_mut() {
            fork(activation, visit);
        }
        // ---- F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
        // SOME-ONLY, for the reason above: a `u64::MAX`-for-absence arm would put eight bytes into every
        // preset that leaves it `None`. Visited, so a scheduled height normalises out of the identity
        // (a rolling deploy peers until it fires) and joins `fence_schedule_v1` and the fork id.
        if let Some(activation) = palw_heartbeat_transparent_same_chain.as_mut() {
            fork(activation, visit);
        }
        // ---- end F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
        // ADR-0160 lane escrow: SOME-ONLY, for the reason above; its `Some(never())` collapses in
        // `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_capacity_escrow_at_licence.as_mut() {
            fork(activation, visit);
        }
        // ADR-0107. Some-only and at the tail, for the same reason as the fence above it.
        if let Some(activation) = palw_share_growth_final.as_mut() {
            fork(activation, visit);
        }
        // P0a. Some-only and at the tail, likewise.
        if let Some(activation) = palw_gdn_key_heads.as_mut() {
            fork(activation, visit);
        }

        let Some(dns) = dns_params.as_mut() else {
            absent = u64::MAX;
            visit(&mut absent);
            return;
        };
        let crate::dns_finality::DnsParams {
            dns_activation_daa_score,
            min_active_stake_sompi: _,
            min_active_validators: _,
            min_bond_amount_sompi: _,
            epoch_length_blocks: _,
            required_work_depth: _,
            required_stake_depth: _,
            emergency_work_margin: _,
            emergency_stake_margin: _,
            max_reorg_horizon_blocks: _,
            evidence_window_blocks: _,
            unbonding_period_blocks: _,
            max_attestations_per_block: _,
            max_attestation_shard_mass: _,
            reward_uniqueness_window_blocks: _,
            stake_event_quality_floor_bps: _,
            degraded_stake_quality_epochs: _,
            stake_censorship_floor_bps: _,
            reward_params: _,
            reorg_mode: _,
            full_reward_split_daa_score,
            pos_v2_activation_daa_score,
            attestation_epoch_length_blue_score: _,
            attestation_lag_blue_score: _,
            attestation_anchor_backoff_blue_score: _,
            stake_score_window_blue_score: _,
            finality_fee_activation_daa_score,
            bond_spend_gate_mergeset_activation_daa_score,
            mandatory_attestation_inclusion_daa_score,
            // A staleness BOUND, not a fence: it does not turn a rule on at a height.
            bridge_finality_max_staleness_daa_score: _,
            emergency_work_override_multiplier: _,
            stake_preference_max_work_deficit_multiplier: _,
            // A maturity LENGTH, not a fence.
            coinbase_settlement_long_maturity_daa: _,
            coinbase_settlement_consensus_activation_daa_score,
            vlt,
            tkn,
            vlt_credit_window_blue_score: _,
            dns_gate_horizon_blocks: _,
            dns_veto_ttl_daa_score: _,
            min_anchor_attesters: _,
            unbond_authz_mergeset_activation_daa_score,
            // **A DURATION, not a fence** — the same classification the two entries above it
            // carry, and getting it wrong is a silent fork. `consensus_identity_id` normalises
            // every visited value to `0 or u64::MAX`, which is right for "does this rule turn on
            // at a height" and meaningless for "how long may a validator be silent": under it
            // 6_048_000, 5_040 and 1 all collapse to the same identity as `u64::MAX`, so two
            // builds that disagree about the finality denominator would peer normally and find
            // out at the first confirmation. It stays inside `consensus_params_id` (the borsh
            // blob), which is where a value like this belongs.
            inactivity_leak_daa: _,
        } = dns;

        visit(dns_activation_daa_score);
        visit(full_reward_split_daa_score);
        visit(pos_v2_activation_daa_score);
        visit(finality_fee_activation_daa_score);
        visit(bond_spend_gate_mergeset_activation_daa_score);
        visit(mandatory_attestation_inclusion_daa_score);
        visit(coinbase_settlement_consensus_activation_daa_score);
        visit(unbond_authz_mergeset_activation_daa_score);

        let crate::vlt::VltParams {
            vlt_shadow_activation_daa_score,
            vlt_activation_daa_score,
            credit_window_epochs: _,
            credit_decay_bps: _,
            credit_delay_epochs: _,
            lambda_vlt_per_kas: _,
            prefill_cost_micro: _,
            decode_cost_micro: _,
            challenge_window_blocks: _,
            verifier_committee_size: _,
            min_verifier_confirmations: _,
            min_verifier_refutations: _,
            max_commitment_age_blocks: _,
            max_capability_validity_blocks: _,
            min_network_compute: _,
            min_active_validators: _,
            audit_fee_sompi: _,
            model_cost_table: _,
        } = vlt;
        visit(vlt_shadow_activation_daa_score);
        visit(vlt_activation_daa_score);

        let crate::token::TokenParams {
            tkn_shadow_activation_daa_score,
            tkn_activation_daa_score,
            // An EPOCH rather than a DAA score, but the same kind of fence: `0` is genesis-active.
            emission_activation_epoch,
            emission_epoch_budget_r0_atomic: _,
            emission_halving_epochs: _,
            settlement_delay_epochs: _,
            emission_min_network_compute: _,
        } = tkn;
        visit(tkn_shadow_activation_daa_score);
        visit(tkn_activation_daa_score);
        visit(emission_activation_epoch);
    }

    pub fn consensus_params_id(&self) -> Hash {
        // Exhaustive on purpose. If this stops compiling because `Params` gained a field, decide
        // whether that field changes block validity: if it does, hash it; if it does not, bind it
        // with a comment saying why, as below.
        let Params {
            // Excluded: where to find peers is not a rule about blocks.
            dns_seeders: _,
            net,
            genesis,
            timestamp_deviation_tolerance,
            max_difficulty_target,
            // Excluded: a lossy f64 view of `max_difficulty_target`, which is hashed above. Its bit
            // pattern is also not stable across the ways it can be computed.
            max_difficulty_target_f64: _,
            past_median_time_window_size,
            difficulty_window_size,
            min_difficulty_window_size,
            coinbase_payload_script_public_key_max_len,
            max_coinbase_payload_len,
            max_tx_inputs,
            max_tx_outputs,
            max_signature_script_len,
            max_script_public_key_len,
            mass_per_tx_byte,
            mass_per_script_pub_key_byte,
            mass_per_sig_op,
            max_block_mass,
            storage_mass_parameter,
            deflationary_phase_daa_score,
            pre_deflationary_phase_base_subsidy,
            skip_proof_of_work,
            max_block_level,
            pruning_proof_m,
            blockrate,
            pre_crescendo_target_time_per_block,
            crescendo_activation,
            dns_params,
            palw_credit,
            palw_fork_choice,
            palw_schedule,
            palw_ramp,
            palw_block_commitment,
            palw_bootstrap_activation,
            palw_unavailable_abstains,
            palw_bond_maturity,
            // Lane maturity (post-launch, 2026-09-26).
            palw_bond_maturity_early,
            palw_frontier_provenance,
            palw_reorg_strict_economic_win,
            palw_pruning_proof_strict_economic_win,
            palw_capacity_weight_cap,
            palw_heartbeat,
            palw_attempt_work,
            palw_attempt_activation,
            dns_bft_gate,
            palw_inactivity_leak,
            palw_beacon_fold,
            palw_capability_bound,
            palw_compute_overlay_retired,
            palw_model_registry,
            palw_economic_payout,
            palw_work_target,
            palw_single_lottery,
            palw_short_challenge_window,
            palw_verification_v2,
            palw_verification_s3,
            palw_verification_s2,
            palw_readiness_v2,
            palw_anchor_clock,
            palw_clock_cursor,
            palw_clock_floor,
            palw_clock_lead_cap,
            palw_offence_attribution,
            palw_rcore_plus,
            // Lane F1 (the panel seed, post-launch).
            palw_panel_seed_execution,
            // Lane A (the operator anchor, post-launch).
            palw_operator_anchor,
            // ADR-0160 lane liab (F-L).
            palw_capacity_aggregate_liability,
            palw_rcore_conservative_classes,
            // Lane V02 (a resolved claim's lock off the work ceiling, post-launch).
            palw_final_lock_full_collateral,
            // Lane V02 (the shortened post-Final lock life, post-launch).
            palw_final_lock_life,
            // Lane F2-lock (the lock life applied retroactively, post-launch).
            palw_final_lock_life_retro,
            palw_class_verify_deadline,
            palw_class_verify_rows,
            // Lane bind-deadlock (post-launch).
            palw_anchor_at_ceiling,
            // Lane accept-order (post-launch).
            palw_lane_accept_parents_first,
            palw_activation_pool,
            palw_readiness_v2_max_age_spans,
            palw_registry_resilience,
            // Lane F2 (the floor-refusal retry, post-launch).
            palw_floor_refusal_retry,
            palw_artifact_root_ownership,
            palw_operator_id_unique,
            palw_objective_offence,
            palw_seat_gate_possession,
            palw_class_receipt_window,
            palw_execution_quanta,
            palw_public_model_source_required,
            palw_economic_safety: _,
            palw_canonical_work,
            palw_admission_independence,
            palw_context_ladder,
            palw_panel_da,
            palw_certification_rent,
            palw_uncertified_weightless,
            palw_da_court,
            palw_court_ladder,
            palw_model_market,
            palw_model_lines,
            palw_model_immutable_v1,
            palw_class_semantics_v1,
            palw_court_commitment_scope_v1,
            palw_staged_public_court_v1,
            palw_staged_tir_route_v1,
            palw_tir_public_conviction_v1,
            palw_tir_container_identity_v1,
            palw_tir_frame_cost_v1,
            palw_tir_bounded_door_v1,
            palw_tir_designated_answers_v1,
            palw_court_outsider_lane_v1,
            palw_state_row_map_v1,
            palw_kernel_route_panel_one_v1,
            palw_court_clock_v2,
            palw_panel_da_disclosure_v1,
            palw_staged_float_constraint_v1,
            palw_model_benefits,
            palw_model_leg_v2,
            palw_model_seed_v2,
            palw_model_virtual_v1,
            palw_model_evm,
            // Lane sink (the model sink binding, post-launch).
            palw_model_sink_bound,
            // ADR-0160 lane verify (F-B, F-R).
            palw_capacity_batch_licence,
            palw_capacity_verify_room,
            palw_capacity_audit_door,
            palw_capacity_issuance_slots,
            palw_capacity_network_room,
            palw_capacity_network_verify,
            palw_panel_unavailable_expiry,
            palw_panel_fast_switch,
            palw_seat_availability,
            palw_capacity_emission_budget,
            palw_capacity_multi_claim,
            palw_capacity_rho_breaker,
            palw_chunk_cap_charge,
            palw_prompt_ids_merkle,
            palw_kary_court,
            palw_model_court_window,
            palw_court_responder_coverage,
            palw_fp_decode_rules,
            palw_fp_derived_work,
            palw_fp_decode_constraint,
            palw_shard_court,
            palw_memory_routing_court_v2,
            palw_class_registration_origin_v1,
            palw_selected_source_bootstrap_v1,
            palw_source_verifier_economics_v2,
            palw_paid_bootstrap_source_v6,
            palw_shard_licensing,
            palw_token_lift,
            palw_kimi_k3,
            palw_tir_v1,
            palw_gen_v1,
            palw_fp_job_v5,
            palw_gen_range_twin_v1,
            palw_task_heads_v1,
            palw_probabilistic_constraints_v1,
            palw_funded_execution_v11,
            palw_stage_execution_v14,
            palw_native_final_v27,
            palw_stage_execution_v28,
            palw_native_execution_v39,
            palw_native_mining_v39,
            palw_fork_choice_rule_e_v1,
            palw_typed_roots_v1,
            palw_fork_choice_commitment_v1,
            palw_verifier_pay_v1,
            palw_model_refutation_seal_v1,
            palw_window_class_v37,
            palw_beacon_source_opt_in_v1,
            palw_beacon_source_fee_v1,
            palw_panel_free_v1,
            palw_signed_registration_v1,
            palw_tir_shard_segment_v2,
            palw_provider_court_v1,
            palw_legacy_held_da_v2,
            palw_bond_budget_v1,
            palw_reporter_income_v2,
            palw_material_income_v3,
            palw_settlement_once_v1,
            palw_paid_claim_trap_v1,
            palw_native_liability_horizon_v1,
            palw_panel_lifetime_v2,
            palw_claim_challenge_v2,
            palw_claim_compact_v4,
            palw_monetary_material_v1,
            palw_material_release_recovery_v2,
            palw_model_verification_gate_v1,
            palw_horizon_extension_v1,
            palw_minimum_price_v1,
            palw_beacon_bootstrap_v1,
            palw_paid_consumer_lifecycle_v1,
            palw_model_bond_allocation_v1,
            palw_bond_formula_v2,
            palw_reporter_share_v2,
            palw_legacy_public_filer_v1,
            palw_court_model_scope_v1,
            palw_public_evidence_reward_v1,
            palw_fp_answer_accounting_v1,
            palw_claim_hard_deadline_v1,
            palw_cell_coverage_closure_v1,
            palw_g14_route_required_v1,
            palw_reward_liability_horizon_v1,
            palw_court_capacity_v1,
            palw_court_transport_v1,
            palw_unresolved_bound_v1,
            palw_claim_local_settlement_v1,
            palw_model_block_rights_v1,
            palw_elastic_final_capacity_v1,
            palw_model_eligibility_v1,
            palw_model_tier_v1,
            palw_detection_policy_v1,
            palw_dag_sibling_attribution_v1,
            palw_panel_anchor_permissionless_v1,
            palw_court_completion_attempt_v1,
            palw_rule_e_participation_v2,
            palw_independently_work_certified_v1,
            palw_gate_c_attestor_liability_v1,
            palw_dag_width_schedule_v1,
            palw_fp_prefix_inherit,
            palw_fp_prefix_state,
            palw_fp_tokenizer_match,
            palw_fp_constraint_v2,
            palw_adapter_class_v1,
            palw_tir_fence2,
            palw_tir_shard_v1,
            palw_floor_reserve_v1,
            palw_real_clock_tick_v1,
            palw_anchor_window_v1,
            palw_receipt_spend_v4,
            palw_evidence_court_v1,
            palw_audit_1004_v1,
            palw_improvement_v1,
            palw_held_close_chunks_v1,
            palw_verification_vertex_v1,
            palw_exec_payload_v2,
            palw_witness_manifest_v1,
            palw_audit_mesh_v1,
            palw_capped_onboarding_v1,
            palw_class_seating,
            palw_tir_only_v1,
            palw_fused_dissectable,
            palw_attn_anchored_root,
            palw_held_context,
            palw_prefill_draw,
            palw_audit_2026_09_11,
            palw_audit_2026_09_11_deep,
            palw_audit_2026_09_23,
            palw_settled_anchor_depth,
            palw_admission_audit_period_daa,
            palw_exec_quantum_maturity_daa,
            palw_difficulty_priced_rows,
            palw_receipt_rows_unpriced,
            palw_attempt_header_pins,
            palw_signature_contexts_v2,
            palw_fp_da_pins,
            palw_validator_payout_bounds,
            palw_slashing_evidence_utxo_genuine,
            palw_epoch_boundary_budget,
            palw_epoch_budget_release,
            palw_panel_economy,
            palw_work_priced_reward,
            palw_execution_lane,
            palw_overlay_carve,
            palw_dns_retirement,
            palw_permissionless_panel_v1,
            palw_permissionless_panel_complete_v1,
            palw_permissionless_panel_retired_v1,
            palw_panel_exposure_floor,
            palw_fp_ruleset_caps,
            palw_heartbeat_transparent,
            // F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency)
            palw_heartbeat_transparent_same_chain,
            // ADR-0160 lane escrow (post-launch)
            palw_capacity_escrow_at_licence,
            palw_share_growth_final,
            palw_gdn_key_heads,
            palw_consensus_mode,
            pow_blake2b_sha3_activation,
            pow_palw_activation,
            pow_palw_ollama_activation,
            pq_enforcement,
            pq_activation_daa_score,
            evm_activation_daa_score,
            evm_gas_pool_v2_activation_daa_score,
            evm_f002_withdraw_cap_activation_daa_score,
            evm_bridge_ledger_activation_daa_score,
            evm_f003_mldsa_verify_activation_daa_score,
            evm_typed_receipt_root_activation_daa_score,
        } = self;

        let mut h = ConsensusParamsId::new();
        h.write(CONSENSUS_FINGERPRINT_DOMAIN_V1);

        h.write([net.network_type as u8]);
        h.write(net.suffix.unwrap_or(u32::MAX).to_le_bytes());
        h.write(genesis.hash.as_bytes());
        h.write(timestamp_deviation_tolerance.to_le_bytes());
        h.write(max_difficulty_target.to_le_bytes());
        h.write((*past_median_time_window_size as u64).to_le_bytes());
        h.write((*difficulty_window_size as u64).to_le_bytes());
        h.write((*min_difficulty_window_size as u64).to_le_bytes());
        h.write([*coinbase_payload_script_public_key_max_len]);
        h.write((*max_coinbase_payload_len as u64).to_le_bytes());
        h.write((*max_tx_inputs as u64).to_le_bytes());
        h.write((*max_tx_outputs as u64).to_le_bytes());
        h.write((*max_signature_script_len as u64).to_le_bytes());
        h.write((*max_script_public_key_len as u64).to_le_bytes());
        h.write(mass_per_tx_byte.to_le_bytes());
        h.write(mass_per_script_pub_key_byte.to_le_bytes());
        h.write(mass_per_sig_op.to_le_bytes());
        h.write(max_block_mass.to_le_bytes());
        h.write(storage_mass_parameter.to_le_bytes());
        h.write(deflationary_phase_daa_score.to_le_bytes());
        h.write(pre_deflationary_phase_base_subsidy.to_le_bytes());
        h.write([*skip_proof_of_work as u8]);
        h.write([*max_block_level]);
        h.write(pruning_proof_m.to_le_bytes());

        h.write(blockrate.target_time_per_block.to_le_bytes());
        h.write((blockrate.ghostdag_k as u64).to_le_bytes());
        h.write(blockrate.past_median_time_sample_rate.to_le_bytes());
        h.write(blockrate.difficulty_sample_rate.to_le_bytes());
        h.write([blockrate.max_block_parents]);
        h.write(blockrate.mergeset_size_limit.to_le_bytes());
        h.write(blockrate.merge_depth.to_le_bytes());
        h.write(blockrate.finality_depth.to_le_bytes());
        h.write(blockrate.pruning_depth.to_le_bytes());
        h.write(blockrate.coinbase_maturity.to_le_bytes());

        h.write(pre_crescendo_target_time_per_block.to_le_bytes());
        h.write(crescendo_activation.daa_score().to_le_bytes());

        // Length-prefixed: `None` and an empty encoding must not hash alike.
        match dns_params {
            Some(dns) => {
                let bytes = borsh::to_vec(dns).expect("DnsParams is borsh-serializable");
                h.write((bytes.len() as u64).to_le_bytes());
                h.write(&bytes);
            }
            None => h.write(u64::MAX.to_le_bytes()),
        };

        h.write(pow_blake2b_sha3_activation.daa_score().to_le_bytes());
        h.write(pow_palw_activation.daa_score().to_le_bytes());
        h.write(pow_palw_ollama_activation.daa_score().to_le_bytes());
        h.write([*pq_enforcement as u8]);
        h.write(pq_activation_daa_score.to_le_bytes());
        h.write(evm_activation_daa_score.to_le_bytes());
        h.write(evm_gas_pool_v2_activation_daa_score.to_le_bytes());
        h.write(evm_f002_withdraw_cap_activation_daa_score.to_le_bytes());
        h.write(evm_f003_mldsa_verify_activation_daa_score.to_le_bytes());
        h.write(evm_typed_receipt_root_activation_daa_score.to_le_bytes());

        // ADR-0033 (B14): the credit fence changes coinbase validity, so an ACTIVE fence is
        // part of the fingerprint — but written Some-only, at the tail: a dormant fence
        // (`None`, every shipped network) must leave the fingerprint byte-identical to what
        // it was before this field existed, or adding the wiring would itself be a flag day.
        // Length-prefixed so distinct fences cannot collide with the fixed-width tail above.
        if let Some(credit) = palw_credit {
            let bytes = borsh::to_vec(credit).expect("PalwCreditParamsV1 is borsh-serializable");
            h.write((bytes.len() as u64).to_le_bytes());
            h.write(&bytes);
        }
        // ADR-0039 W4′ fork-choice fence, Some-only for the same reason: a network that does not
        // install one must fingerprint byte-identically to before the field existed.
        // Some-only, like every fence beside it: a network that installs neither fingerprints
        // byte-identically to one built before these fields existed.
        if let Some(schedule) = palw_schedule {
            let bytes = borsh::to_vec(schedule).expect("PalwScheduleParamsV1 is borsh-serializable");
            h.write((bytes.len() as u64).to_le_bytes());
            h.write(&bytes);
        }
        if let Some(ramp) = palw_ramp {
            let bytes = borsh::to_vec(ramp).expect("PalwWeightParamsV1 is borsh-serializable");
            h.write((bytes.len() as u64).to_le_bytes());
            h.write(&bytes);
        }
        if let Some(fork_choice) = palw_fork_choice {
            let bytes = borsh::to_vec(fork_choice).expect("PalwChainWeightParamsV1 is borsh-serializable");
            h.write((bytes.len() as u64).to_le_bytes());
            h.write(&bytes);
        }
        // ADR-0038 Decision A, Some-only for the same reason: installing it changes which headers
        // are valid, so it belongs in the fingerprint — and a network that installs none must
        // fingerprint byte-identically to before the field existed, or the wiring is a flag day.
        if let Some(commitment) = palw_block_commitment {
            let bytes = borsh::to_vec(commitment).expect("PalwBlockCommitmentParamsV1 is borsh-serializable");
            h.write((bytes.len() as u64).to_le_bytes());
            h.write(&bytes);
        }
        // ADR-0064, Some-only for the same reason: arming it changes which blocks are admissible,
        // so it belongs in the fingerprint — and every shipped preset leaves it `None` and
        // fingerprints byte-identically to before the field existed. The identity path collapses an
        // armed-in-the-future fence back to `None` first (see
        // `normalize_values_a_scheduled_fence_drags_with_it`), so arming it does not cost peering.
        if let Some(activation) = palw_bootstrap_activation {
            h.write(b"palw_bootstrap_activation");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0065 D4, Some-only for the same reason: an unset fence writes nothing, so every
        // shipped preset fingerprints byte-identically to before the field existed.
        if let Some(activation) = palw_unavailable_abstains {
            h.write(b"palw_unavailable_abstains");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0065 D1. BOTH halves, because the window decides which bonds may be seated and a
        // commitment that omitted it would let two builds claim the same rules and enforce
        // different ones. Some-only, so an unset field writes nothing.
        if let Some(maturity) = palw_bond_maturity {
            h.write(b"palw_bond_maturity");
            h.write(maturity.activation.daa_score().to_le_bytes());
            h.write(maturity.window_daa.to_le_bytes());
        }
        // Lane maturity (post-launch, 2026-09-26), Some-only (and collapsed from `Some(never())` for
        // the identity), so a build that leaves it dormant fingerprints byte-identically to one
        // without the field. No companion value: the window is `palw_bond_maturity`'s, written above.
        if let Some(activation) = palw_bond_maturity_early {
            h.write(b"palw_bond_maturity_early");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0065 D2, Some-only like its siblings.
        if let Some(activation) = palw_frontier_provenance {
            h.write(b"palw_frontier_provenance");
            h.write(activation.daa_score().to_le_bytes());
        }
        // lane: rcore/f1-forkchoice-attacks. Some-only, so a build that never arms it writes
        // nothing and the normalize collapse below keeps a scheduled `never()` from writing either.
        if let Some(activation) = palw_reorg_strict_economic_win {
            h.write(b"palw_reorg_strict_economic_win");
            h.write(activation.daa_score().to_le_bytes());
        }
        // lane: rcore/hf-pptake2. Some-only, so a build that never arms it writes nothing and the
        // normalize collapse above keeps a scheduled `never()` from writing either.
        if let Some(activation) = palw_pruning_proof_strict_economic_win {
            h.write(b"palw_pruning_proof_strict_economic_win");
            h.write(activation.daa_score().to_le_bytes());
        }
        // lane: rcore/cap-weight (ADR-0160 F-W). Some-only, with its never() collapse in
        // `normalize_values_a_scheduled_fence_drags_with_it`.
        if let Some(activation) = palw_capacity_weight_cap {
            h.write(b"palw_capacity_weight_cap");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0071 SA-1..SA-4, Some-only for the ADR-0065 D4 reason: an unset fence writes
        // nothing, so every shipped preset fingerprints byte-identically to before the field
        // existed. The bound and the price are constants rather than fields beside the fence, so
        // there is no companion value to drag into the identity (the D1 hazard).
        if let Some(activation) = palw_capability_bound {
            h.write(b"palw_capability_bound");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0134, Some-only for the same reason.
        if let Some(activation) = palw_compute_overlay_retired {
            h.write(b"palw_compute_overlay_retired");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0135, Some-only for the same reason.
        // ADR-0132 Upgrade C: the height, and beside it the five numbers it fixes. Some-only, like
        // its siblings; reported here and never gated, for the SA-4 reason the carve's numbers are.
        if let Some(payout) = palw_economic_payout {
            h.write(b"palw_economic_payout");
            h.write(payout.activation.daa_score().to_le_bytes());
            h.write(payout.rate_sompi_per_giga.to_le_bytes());
            h.write(payout.panel_share_alpha_permille.to_le_bytes());
            h.write(payout.panel_share_min_permille.to_le_bytes());
            h.write(payout.panel_share_max_permille.to_le_bytes());
            h.write(payout.cap_utilization_max_permille.to_le_bytes());
        }
        if let Some(activation) = palw_model_registry {
            h.write(b"palw_model_registry");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0137: the height only, Some-only.
        if let Some(activation) = palw_work_target {
            h.write(b"palw_work_target");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0132 S: the height only, Some-only.
        if let Some(activation) = palw_single_lottery {
            h.write(b"palw_single_lottery");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0132 §7.6: the height only, Some-only.
        if let Some(activation) = palw_short_challenge_window {
            h.write(b"palw_short_challenge_window");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0133 Verification V2: the height only, Some-only.
        if let Some(activation) = palw_verification_v2 {
            h.write(b"palw_verification_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_verification_s3 {
            h.write(b"palw_verification_s3");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_verification_s2 {
            h.write(b"palw_verification_s2");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0133 §11.2 readiness V2: the height only, Some-only.
        if let Some(activation) = palw_readiness_v2 {
            h.write(b"palw_readiness_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0138 the anchor clock: the height only, Some-only.
        if let Some(activation) = palw_anchor_clock {
            h.write(b"palw_anchor_clock");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0142 the clock cursor: the height only, Some-only, so every shipped preset — which
        // leaves it unset — fingerprints byte-identically to a build without the field.
        if let Some(activation) = palw_clock_cursor {
            h.write(b"palw_clock_cursor");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The clock floor (H3/H5): the height only, Some-only, so every preset that leaves it unset
        // — all but testnet-12 — fingerprints byte-identically to a build without the field.
        if let Some(activation) = palw_clock_floor {
            h.write(b"palw_clock_floor");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The lead cap: the height only, Some-only, for the floor's reason (the 132 s it caps at is a
        // constant of the rule, named by the field).
        if let Some(activation) = palw_clock_lead_cap {
            h.write(b"palw_clock_lead_cap");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0152 v2 F2's attribution fence: the height only, Some-only, for the floor's reason.
        if let Some(activation) = palw_offence_attribution {
            h.write(b"palw_offence_attribution");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0152 R-core+: the height only, Some-only, for the floor's reason; and C7's list only
        // when non-empty (IMPL-6), so every other preset fingerprints byte-identically to a build
        // without either field.
        if let Some(activation) = palw_rcore_plus {
            h.write(b"palw_rcore_plus");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Lane F1 (the panel seed, post-launch): the height and, beside it, the seed rule's domain —
        // the rule's version rides its fence, so two builds arming different seed rules at one height
        // announce different rulesets (and, like any armed fence, part by fork id at the height).
        // Some-only (and collapsed from `Some(never())` for the identity), so a build that leaves it
        // dormant fingerprints byte-identically to one without the field.
        if let Some(activation) = palw_panel_seed_execution {
            h.write(b"palw_panel_seed_execution");
            h.write(activation.daa_score().to_le_bytes());
            h.write(crate::palw_panel_v2::PALW_PANEL_V2_DOMAIN_DRAW_SEED_V1);
        }
        // Lane A (the operator anchor, post-launch): the height, the rule's domain beside it (the rule's
        // version rides its fence) and the operator bonds it trusts, as listed (`validate_palw_v2` holds
        // the list sorted and distinct, so one set has one spelling) — two builds trusting different
        // operators at one height announce different rulesets. The keys are not written: each is its
        // genesis registration's, already inside the ruleset id. Some-only (and collapsed from
        // `Some(never())` for the identity), so a build that leaves it dormant fingerprints
        // byte-identically to one without the field.
        if let Some(rule) = palw_operator_anchor {
            h.write(b"palw_operator_anchor");
            h.write(rule.activation.daa_score().to_le_bytes());
            h.write(crate::palw_operator_anchor_v1::PALW_OPERATOR_ANCHOR_DOMAIN_V1);
            h.write((rule.operators.len() as u64).to_le_bytes());
            for bond in rule.operators.iter() {
                h.write(bond.0.transaction_id.as_byte_slice());
                h.write(bond.0.index.to_le_bytes());
            }
        }
        // Lane V02 (post-launch): the height, Some-only (and collapsed from `Some(never())` for the
        // identity), so a build that leaves it dormant fingerprints byte-identically to one without
        // the field.
        if let Some(activation) = palw_final_lock_full_collateral {
            h.write(b"palw_final_lock_full_collateral");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0160 lane liab (F-L): the value WHOLE — the height, the rule's domain beside it (the rule's
        // version rides its fence) and every step in order (`validate_palw_v2` holds them strictly
        // increasing, so one schedule has one spelling) — so two builds arming different schedules at
        // one height announce different rulesets. Some-only (and collapsed from `Some(never())` for the
        // identity), so a build that leaves it dormant fingerprints byte-identically to one without it.
        if let Some(value) = palw_capacity_aggregate_liability {
            h.write(b"palw_capacity_aggregate_liability");
            h.write(value.activation.daa_score().to_le_bytes());
            h.write(crate::palw_aggregate_liability_v1::PALW_CAPACITY_LIABILITY_DOMAIN_V1);
            h.write((value.steps.len() as u64).to_le_bytes());
            for step in value.steps.iter() {
                h.write(step.from_daa.to_le_bytes());
                h.write(step.rho.to_le_bytes());
                h.write(step.q_credit_permille.to_le_bytes());
            }
        }
        if !palw_rcore_conservative_classes.is_empty() {
            h.write(b"palw_rcore_conservative_classes");
            h.write((palw_rcore_conservative_classes.len() as u64).to_le_bytes());
            for class in palw_rcore_conservative_classes.iter() {
                h.write(class.as_byte_slice());
            }
        }
        // Lane V02 (post-launch): the height, Some-only (and collapsed from `Some(never())` for the
        // identity), so a build that leaves it dormant fingerprints byte-identically to one without
        // the field.
        if let Some(activation) = palw_final_lock_life {
            h.write(b"palw_final_lock_life");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Lane F2-lock (post-launch): the height, Some-only (and collapsed from `Some(never())` for the
        // identity), so a build that leaves it dormant fingerprints byte-identically to one without the
        // field.
        if let Some(activation) = palw_final_lock_life_retro {
            h.write(b"palw_final_lock_life_retro");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0152 §4-quater: the fence's height only, Some-only, for the floor's reason; and the
        // measured rows only when non-empty, every field of each in declaration order, so every other
        // preset — and testnet-12 at launch, whose rows are empty — hashes no row bytes at all.
        if let Some(activation) = palw_class_verify_deadline {
            h.write(b"palw_class_verify_deadline");
            h.write(activation.daa_score().to_le_bytes());
        }
        if !palw_class_verify_rows.is_empty() {
            h.write(b"palw_class_verify_rows");
            h.write((palw_class_verify_rows.len() as u64).to_le_bytes());
            for row in palw_class_verify_rows.iter() {
                h.write(row.class_id.as_byte_slice());
                h.write(row.activation_daa.to_le_bytes());
                h.write(row.a_r_ps.to_le_bytes());
                h.write(row.b_r_ps.to_le_bytes());
                h.write(row.t_fixed_ms.to_le_bytes());
                h.write(row.canonical_positions.to_le_bytes());
                h.write(row.leaves_per_position.to_le_bytes());
            }
        }
        // Lane bind-deadlock (post-launch): the height only, Some-only (and collapsed from
        // `Some(never())` for the identity), so a build that leaves it dormant fingerprints
        // byte-identically to one without the field.
        if let Some(activation) = palw_anchor_at_ceiling {
            h.write(b"palw_anchor_at_ceiling");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Lane accept-order (post-launch): the height only, Some-only (and collapsed from
        // `Some(never())` for the identity), so a build that leaves it dormant fingerprints
        // byte-identically to one without the field.
        if let Some(activation) = palw_lane_accept_parents_first {
            h.write(b"palw_lane_accept_parents_first");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0152-adjacent (Activation Pool): the height and its terms, Some-only, so every preset
        // but testnet-12 fingerprints byte-identically to a build without the field.
        if let Some(pool) = palw_activation_pool {
            h.write(b"palw_activation_pool");
            h.write(pool.activation.daa_score().to_le_bytes());
            palw_activation_pool_terms_write_v1(&mut h, &pool.terms);
        }
        // The readiness-V2 horizon (user decision 2026-09-25): the height and its spans, Some-only, so
        // every preset but testnet-12 fingerprints byte-identically to a build without the field.
        if let Some(horizon) = palw_readiness_v2_max_age_spans {
            h.write(b"palw_readiness_v2_max_age_spans");
            h.write(horizon.activation.daa_score().to_le_bytes());
            h.write(horizon.max_age_spans.to_le_bytes());
        }
        // Lane F1 (registry resilience, V03/V05): the height only, Some-only, so every preset that leaves
        // it unset — all of them as shipped — fingerprints byte-identically to a build without the field.
        if let Some(activation) = palw_registry_resilience {
            h.write(b"palw_registry_resilience");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Lane F2 (the floor-refusal retry, post-launch): the height only, Some-only, so every preset that
        // leaves it unset — all of them as shipped, testnet-12 included — fingerprints byte-identically to
        // a build without the field.
        if let Some(activation) = palw_floor_refusal_retry {
            h.write(b"palw_floor_refusal_retry");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0143 the artifact-root ownership fence, Some-only for the same reason.
        if let Some(activation) = palw_artifact_root_ownership {
            h.write(b"palw_artifact_root_ownership");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_operator_id_unique {
            h.write(b"palw_operator_id_unique");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_objective_offence {
            h.write(b"palw_objective_offence");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_seat_gate_possession {
            h.write(b"palw_seat_gate_possession");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_class_receipt_window {
            h.write(b"palw_class_receipt_window");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_execution_quanta {
            h.write(b"palw_execution_quanta");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(rule) = palw_public_model_source_required {
            h.write(b"palw_public_model_source_required");
            h.write(rule.activation.daa_score().to_le_bytes());
        }
        // ADR-0145 the canonical-work fence, Some-only for the same reason: every shipped preset
        // leaves it unset and therefore fingerprints byte-identically to a build without the field.
        if let Some(activation) = palw_canonical_work {
            h.write(b"palw_canonical_work");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_admission_independence {
            h.write(b"palw_admission_independence");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0077 Decision 16, Some-only for the same reason: every shipped preset leaves it
        // unset and therefore fingerprints byte-identically to a build without the field at all.
        // The mode number and the payload rule are constants rather than fields beside the fence,
        // so there is no companion value to drag into the identity (the D1 hazard).
        if let Some(activation) = palw_panel_da {
            h.write(b"palw_panel_da");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0066 Decisions 1 and 4: Some-only, so every preset that leaves them unset
        // fingerprints byte-identically to a build without the fields at all.
        if let Some(heartbeat) = palw_heartbeat {
            // The label carries the WIDTH RULE's semantic version: `max_per_mergeset` began as a
            // flat member count and the ADR-0068 drill's F5 amended it to "flat bound, or one
            // chain" — same value, different rule, and a rule change with an unchanged hash is
            // this project's textbook silent fork. Only armed presets move (Some-only), which is
            // the drill devnet and testnet-11's scheduled arming; both pins moved with it.
            h.write(b"palw_heartbeat/width-chain-exempt-v2");
            h.write(heartbeat.activation.daa_score().to_le_bytes());
            h.write(heartbeat.work_log2.to_le_bytes());
            h.write(heartbeat.max_per_mergeset.to_le_bytes());
        }
        // ADR-0068 Phase 1 (F2): Some-only, like its siblings — an unarmed preset fingerprints
        // byte-identically to a build without the field at all.
        if let Some(attempt_work) = palw_attempt_work {
            h.write(b"palw_attempt_work");
            h.write(attempt_work.activation.daa_score().to_le_bytes());
            h.write(attempt_work.work_log2.to_le_bytes());
            h.write(attempt_work.ticket_bucket_log2.to_le_bytes());
        }
        // ADR-0072 SA-1..SA-4, Some-only for the reason every fence above it is: an unset field
        // writes nothing, so every shipped preset fingerprints byte-identically to a build without
        // it. Armed, it decides which algo id the attempt lane uses and which envelope version is
        // admissible at a height, which is as much a rule about block validity as a rule gets.
        if let Some(activation) = palw_attempt_activation {
            h.write(b"palw_attempt_activation");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0128: the height and all three leak numbers, Some-only — they decide which bonds an
        // epoch counts and therefore which anchors are DNS-final and vetoed, and every shipped
        // preset leaves the gate `None` and fingerprints byte-identically to a build without it.
        if let Some(gate) = dns_bft_gate {
            h.write(b"dns_bft_gate");
            h.write(gate.activation.daa_score().to_le_bytes());
            h.write(gate.t_leak_daa.to_le_bytes());
            h.write(gate.reentry_final_depth_daa.to_le_bytes());
            h.write(gate.min_retained_validators.to_le_bytes());
        }
        if let Some(leak) = palw_inactivity_leak {
            // The label carries the RULE's semantic version: ADR-0066 SA-2 added the re-entry
            // depth beside the grace, and a leak with hysteresis is not the leak without it. Only
            // armed presets move (Some-only), and no shipped preset arms this.
            h.write(b"palw_inactivity_leak/hysteresis-v2");
            h.write(leak.activation.daa_score().to_le_bytes());
            h.write(leak.t_leak_daa.to_le_bytes());
            h.write(leak.reentry_final_depth_daa.to_le_bytes());
        }
        // ADR-0073 SA-1: BOTH halves, because `k` decides the draw and a commitment that omitted
        // it would let two builds claim the same rules and fold different beacons. Some-only, so
        // every shipped preset fingerprints byte-identically to a build without the field at all.
        if let Some(fold) = palw_beacon_fold {
            h.write(b"palw_beacon_fold");
            h.write(fold.activation.daa_score().to_le_bytes());
            h.write([fold.k]);
        }
        // ADR-0077 Phase B: Some-only, like every fence above it — an unarmed preset fingerprints
        // byte-identically to a build without the field at all.
        if let Some(ladder) = palw_context_ladder {
            h.write(b"palw_context_ladder");
            h.write(ladder.daa_score().to_le_bytes());
        }
        // ADR-0075 SA-1/SA-2, Some-only like every fence above it: arming it changes which
        // lifecycle objects a block may open a chunk group with and which it may grade, so it
        // belongs in the fingerprint — and every shipped preset leaves it `None` and fingerprints
        // byte-identically to a build without the field at all.
        if let Some(activation) = palw_certification_rent {
            h.write(b"palw_certification_rent");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0069 Decision 7, Some-only like every fence beside it: an unset field writes
        // nothing, so every shipped preset fingerprints byte-identically to a build without it.
        if let Some(activation) = palw_uncertified_weightless {
            h.write(b"palw_uncertified_weightless");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0062, Some-only for the reason its siblings are: arming it changes which objects a
        // block may carry and what a claim record may hold, so it belongs in the fingerprint —
        // and every shipped preset leaves it `None` and fingerprints byte-identically to a build
        // without the field at all.
        if let Some(activation) = palw_da_court {
            h.write(b"palw_da_court");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0084 U-08: the same contract — absent is byte-identical to a build without the field.
        if let Some(activation) = palw_court_ladder {
            h.write(b"palw_court_ladder");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0087 Decision 6: the same contract.
        if let Some(activation) = palw_model_market {
            h.write(b"palw_model_market");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0088 Decision 11: the same contract.
        if let Some(activation) = palw_model_immutable_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_model_immutable_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_class_semantics_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_class_semantics_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_court_commitment_scope_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_court_commitment_scope_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_staged_public_court_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_staged_public_court_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_staged_tir_route_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_staged_tir_route_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_tir_public_conviction_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_tir_public_conviction_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_tir_container_identity_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_tir_container_identity_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The TIR frame's measured cost model (audit §10): it fixes every frame's prices and doors, so it is a rule about blocks.
        // Some-only: `None` on every shipped preset, whose identity is unchanged.
        if let Some(cost) = palw_tir_frame_cost_v1 {
            h.write(b"palw_tir_frame_cost_v1");
            h.write(borsh::to_vec(cost).expect("PalwTirFrameCostV1 is borsh-serializable"));
        }
        if let Some(at) = palw_tir_bounded_door_v1 {
            h.write(b"palw_tir_bounded_door_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = palw_tir_designated_answers_v1 {
            h.write(b"palw_tir_designated_answers_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = palw_court_outsider_lane_v1 {
            h.write(b"palw_court_outsider_lane_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = palw_state_row_map_v1 {
            h.write(b"palw_state_row_map_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = palw_kernel_route_panel_one_v1 {
            h.write(b"palw_kernel_route_panel_one_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = palw_court_clock_v2.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_court_clock_v2");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = palw_panel_da_disclosure_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_panel_da_disclosure_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_staged_float_constraint_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_staged_float_constraint_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_model_lines {
            h.write(b"palw_model_lines");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0095 (a position is a membership): the same contract. 14c453d1 bound this field in
        // the destructure above and never wrote it, so testnet-11's DAA-2400 fence was the one
        // scheduled height the printed fingerprint did not carry: the build that crossed it and
        // the build without it both printed `060e3597…` on 2026-09-09, and an operator told to
        // compare fingerprints could not see the flag day coming. The identity is unaffected —
        // `normalize_values_a_scheduled_fence_drags_with_it` collapses a scheduled benefits fence
        // to `None`, which writes nothing, exactly as before this line existed.
        if let Some(activation) = palw_model_benefits {
            h.write(b"palw_model_benefits");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0114: Some-only, so every preset that does not schedule it fingerprints as before.
        if let Some(activation) = palw_model_leg_v2 {
            h.write(b"palw_model_leg_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0120: Some-only, so every preset that does not schedule it fingerprints as before.
        if let Some(activation) = palw_model_seed_v2 {
            h.write(b"palw_model_seed_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0162: Some-only, so every preset that does not schedule it fingerprints as before.
        if let Some(activation) = palw_model_virtual_v1 {
            h.write(b"palw_model_virtual_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0089 Decision 9: the same contract.
        if let Some(activation) = palw_model_evm {
            h.write(b"palw_model_evm");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Lane sink (the model sink binding, post-launch): the height, Some-only (and collapsed from
        // `Some(never())` for the identity), so a build that leaves it dormant fingerprints
        // byte-identically to one without the field.
        if let Some(activation) = palw_model_sink_bound {
            h.write(b"palw_model_sink_bound");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0160 lane verify (F-B, F-R): the heights, Some-only and collapsed from `Some(never())`, so
        // a build that leaves them dormant fingerprints byte-identically to one without the fields.
        if let Some(activation) = palw_capacity_batch_licence {
            h.write(b"palw_capacity_batch_licence");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_capacity_verify_room {
            h.write(b"palw_capacity_verify_room");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0160 stage 2 (F-Q, F-S): Some-only.
        if let Some(activation) = palw_capacity_audit_door {
            h.write(b"palw_capacity_audit_door");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_capacity_issuance_slots {
            h.write(b"palw_capacity_issuance_slots");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_capacity_network_room {
            h.write(b"palw_capacity_network_room");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_capacity_network_verify {
            h.write(b"palw_capacity_network_verify");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_panel_unavailable_expiry {
            h.write(b"palw_panel_unavailable_expiry");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_panel_fast_switch {
            h.write(b"palw_panel_fast_switch");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_seat_availability {
            h.write(b"palw_seat_availability");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_capacity_emission_budget {
            h.write(b"palw_capacity_emission_budget");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_capacity_multi_claim {
            h.write(b"palw_capacity_multi_claim");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_capacity_rho_breaker {
            h.write(b"palw_capacity_rho_breaker");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0075 D14, Some-only like its siblings: an unset fence writes nothing, so every
        // shipped preset fingerprints byte-identically to before the field existed.
        if let Some(activation) = palw_chunk_cap_charge {
            h.write(b"palw_chunk_cap_charge");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0081 Decision 3, Some-only like every fence above it: arming it changes what every
        // job on the network is CALLED (the hash is inside `fp_job_id_v3` and therefore inside
        // every claim id), so it belongs in the fingerprint — and every shipped preset leaves it
        // `None` and fingerprints byte-identically to a build without the field at all.
        if let Some(activation) = palw_prompt_ids_merkle {
            h.write(b"palw_prompt_ids_merkle");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0082 Decision 3, Some-only like every fence above it: arming it changes what a court
        // may do and what a class may declare, so it belongs in the fingerprint — and every shipped
        // preset leaves it `None` and fingerprints byte-identically to a build without the field.
        if let Some(activation) = palw_kary_court {
            h.write(b"palw_kary_court");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_model_court_window {
            h.write(b"palw_model_court_window");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The fused-terminal responder-coverage rule, Some-only like every fence above it: arming
        // it changes which party a fused terminal's silence charges, so it belongs in the
        // fingerprint — and every shipped preset leaves it `None` and fingerprints byte-identically
        // to a build without the field.
        if let Some(activation) = palw_court_responder_coverage {
            h.write(b"palw_court_responder_coverage");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0082 Decisions 10/11, Some-only for the same reason: arming it changes what a claim
        // earns and what token a class may commit, and every shipped preset leaves it `None` and
        // fingerprints byte-identically to a build without the field.
        if let Some(activation) = palw_fp_decode_rules {
            h.write(b"palw_fp_decode_rules");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0145 §5/§6, Some-only for the same reason: arming it changes what a claim is paid
        // for, and every shipped preset leaves it `None` and fingerprints byte-identically to a
        // build without the field.
        if let Some(activation) = palw_fp_derived_work {
            h.write(b"palw_fp_derived_work");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0096 Decisions 6–8, Some-only for the same reason: arming it changes what token a
        // class may commit and what bytes a verifier renders, and every shipped preset leaves it
        // `None` and fingerprints byte-identically to a build without the field.
        if let Some(activation) = palw_fp_decode_constraint {
            h.write(b"palw_fp_decode_constraint");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0099 Decision 5, Some-only for the same reason: arming it changes what convicts a
        // producer, and every shipped preset leaves it `None` and fingerprints byte-identically
        // to a build without the field.
        if let Some(activation) = palw_shard_court {
            h.write(b"palw_shard_court");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_memory_routing_court_v2 {
            h.write(b"palw_memory_routing_court_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(at) = palw_class_registration_origin_v1 {
            h.write(b"palw_class_registration_origin_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(fence) = palw_selected_source_bootstrap_v1 {
            h.write(b"palw_selected_source_bootstrap_v1");
            h.write(fence.at.daa_score().to_le_bytes());
            h.write(fence.policy.id().as_byte_slice());
        }
        if let Some(fence) = palw_source_verifier_economics_v2 {
            h.write(b"palw_source_verifier_economics_v2");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policy.id().as_byte_slice());
        }
        if let Some(fence) = palw_paid_bootstrap_source_v6 {
            h.write(b"palw_paid_bootstrap_source_v6");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policy.id().as_slice());
        }
        // ADR-0100 Decision 4, Some-only for the same reason: every shipped preset leaves it
        // `None` and fingerprints byte-identically to a build without the field.
        if let Some(activation) = palw_shard_licensing {
            h.write(b"palw_shard_licensing");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0102, Some-only for the same reason — and the reason the kernel it arms is not in
        // `court_catalog_root`: a dormant network fingerprints byte-identically to a build without
        // the kernel or the field.
        if let Some(activation) = palw_token_lift {
            h.write(b"palw_token_lift");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_kimi_k3 {
            h.write(b"palw_kimi_k3");
            h.write(activation.daa_score().to_le_bytes());
        }
        // RFC-0002 Phase F, Some-only: every shipped preset leaves it `None` and fingerprints
        // byte-identically to a build without the field. Armed, the ruleset states its primitive set,
        // its IR court version, its ceilings and the IR court root — the "court_catalog_root gains
        // prim_set_id" of the RFC, carried here rather than in the bundle, whose root is identity on
        // every network.
        if let Some(fence) = palw_tir_v1 {
            h.write(b"palw_tir_v1/court-v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // RFC-0003, Some-only: every shipped preset leaves it `None` and fingerprints
        // byte-identically to a build without the field. Armed, the ruleset states its program
        // version, primitive, rand and output sets, generative court version, per-profile ceilings
        // and the generative court root.
        if let Some(fence) = palw_gen_v1 {
            h.write(b"palw_gen_v1/court-v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // FP Job V5, Some-only for the same reason: a dormant network fingerprints byte-identically
        // to a build without the field, and FP Job V4's own bytes never move.
        if let Some(activation) = palw_fp_job_v5 {
            h.write(b"palw_fp_job_v5");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The generative range twin, Some-only for the same reason.
        if let Some(activation) = palw_gen_range_twin_v1 {
            h.write(b"palw_gen_range_twin_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The task-head profile, Some-only for the same reason.
        if let Some(fence) = palw_task_heads_v1 {
            h.write(b"palw_task_heads_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        if let Some(fence) = palw_stage_execution_v14 {
            h.write(b"palw_stage_execution_v14");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policies.root());
        }
        if let Some(fence) = palw_native_final_v27 {
            h.write(b"palw_native_final_v27");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policy.root());
        }
        if let Some(fence) = palw_stage_execution_v28 {
            h.write(b"palw_stage_execution_v28");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policies.root());
        }
        if let Some(fence) = palw_native_execution_v39 {
            h.write(b"palw_native_execution_v39");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policies.root());
        }
        if let Some(fence) = palw_native_mining_v39 {
            h.write(b"palw_native_mining_v39");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policy.root());
        }
        if let Some(fence) = palw_funded_execution_v11 {
            h.write(b"palw_funded_execution_v11");
            h.write(fence.activation.daa_score().to_le_bytes());
            h.write(fence.policies.root());
        }
        // The probabilistic-constraint fence, Some-only for the same reason.
        if let Some(activation) = palw_probabilistic_constraints_v1 {
            h.write(b"palw_probabilistic_constraints_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // Rule E's fence, Some-only for the same reason.
        if let Some(activation) = palw_fork_choice_rule_e_v1 {
            h.write(b"palw_fork_choice_rule_e_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The typed-roots fence, Some-only for the same reason.
        if let Some(activation) = palw_typed_roots_v1 {
            h.write(b"palw_typed_roots_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The fork-choice commitment fence, Some-only for the same reason.
        if let Some(activation) = palw_fork_choice_commitment_v1 {
            h.write(b"palw_fork_choice_commitment_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The verifier-pay fence, Some-only for the same reason.
        if let Some(activation) = palw_verifier_pay_v1 {
            h.write(b"palw_verifier_pay_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_model_refutation_seal_v1 {
            h.write(b"palw_model_refutation_seal_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_window_class_v37 {
            h.write(b"palw_window_class_v37");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_beacon_source_opt_in_v1 {
            h.write(b"palw_beacon_source_opt_in_v1");
            h.write(activation.activation.daa_score().to_le_bytes());
            h.write(activation.policy.id());
        }
        if let Some(activation) = palw_beacon_source_fee_v1 {
            h.write(b"palw_beacon_source_fee_v1");
            h.write(activation.activation.daa_score().to_le_bytes());
            h.write(activation.policy.id());
        }
        // The Panel-free fence, Some-only for the same reason.
        if let Some(fence) = palw_panel_free_v1 {
            h.write(b"palw_panel_free_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // The signed-registration envelope's fence, Some-only for the same reason.
        if let Some(activation) = palw_signed_registration_v1 {
            h.write(b"palw_signed_registration_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // RFC-0006 per-segment pricing's fence, Some-only for the same reason.
        if let Some(activation) = palw_tir_shard_segment_v2 {
            h.write(b"palw_tir_shard_segment_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The provider court's fence (DA16), Some-only for the same reason.
        if let Some(at) = palw_reporter_income_v2 {
            h.write(b"palw_reporter_income_v2");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(f) = &palw_paid_consumer_lifecycle_v1 {
            h.write(b"palw_paid_consumer_lifecycle_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.digest().as_bytes());
        }
        if let Some(f) = &palw_panel_lifetime_v2 {
            h.write(b"palw_panel_lifetime_v2");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = &palw_claim_challenge_v2 {
            h.write(b"palw_claim_challenge_v2");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id());
        }
        if let Some(f) = &palw_claim_compact_v4 {
            h.write(b"palw_claim_compact_v4");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id());
        }
        if let Some(f) = &palw_material_release_recovery_v2 {
            h.write(b"palw_material_release_recovery_v2");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = &palw_monetary_material_v1 {
            h.write(b"palw_monetary_material_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = &palw_model_verification_gate_v1 {
            h.write(b"palw_model_verification_gate_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = &palw_horizon_extension_v1 {
            h.write(b"palw_horizon_extension_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = &palw_minimum_price_v1 {
            h.write(b"palw_minimum_price_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = &palw_beacon_bootstrap_v1 {
            h.write(b"palw_beacon_bootstrap_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(at) = palw_material_income_v3 {
            h.write(b"palw_material_income_v3");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = palw_settlement_once_v1 {
            h.write(b"palw_settlement_once_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = palw_paid_claim_trap_v1 {
            h.write(b"palw_paid_claim_trap_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = palw_native_liability_horizon_v1 {
            h.write(b"palw_native_liability_horizon_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(fence) = &palw_bond_budget_v1 {
            h.write(b"palw_bond_budget_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        if let Some(fence) = &palw_model_bond_allocation_v1 {
            h.write(b"palw_model_bond_allocation_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        if let Some(fence) = &palw_bond_formula_v2 {
            h.write(b"palw_bond_formula_v2");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        if let Some(activation) = palw_provider_court_v1 {
            h.write(b"palw_provider_court_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The legacy held DA fence (LG14-B), Some-only for the same reason.
        if let Some(activation) = palw_legacy_held_da_v2 {
            h.write(b"palw_legacy_held_da_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0032's amended reporter share (lane INTF), Some-only for the same reason — and the share it arms beside the
        // height, so a 10% and a 49% build never announce one ruleset (the amendment's handshake rule) while every preset
        // that leaves the fence dormant (testnet-12's live ruleset included) fingerprints byte-identically to int-12.
        if let Some(activation) = palw_reporter_share_v2.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_reporter_share_v2");
            h.write(activation.daa_score().to_le_bytes());
            h.write(crate::palw_state_v2::PALW_RCORE_REPORTER_REWARD_BPS_V2.to_le_bytes());
        }
        // The legacy route's public filer (lane LG14-A), Some-only for the same reason.
        if let Some(activation) = palw_legacy_public_filer_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_legacy_public_filer_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The three fences of the 2026-10-12 rulings, Some-only for the same reason.
        if let Some(activation) = palw_court_model_scope_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_court_model_scope_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_public_evidence_reward_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_public_evidence_reward_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_fp_answer_accounting_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_fp_answer_accounting_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(f) = palw_claim_hard_deadline_v1.filter(|f| f.activation != ForkActivation::never()) {
            h.write(b"palw_claim_hard_deadline_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        // RFC-0006 R6-CELL round 4's coverage closure, Some-only for the same reason.
        if let Some(activation) = palw_cell_coverage_closure_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_cell_coverage_closure_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The cross-lane G14 gates, Some-only for the same reason.
        if let Some(activation) = palw_g14_route_required_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_g14_route_required_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_reward_liability_horizon_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_reward_liability_horizon_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The court-capacity fence, Some-only for the same reason, with its policy.
        if let Some(f) = palw_court_capacity_v1.filter(|f| f.activation != ForkActivation::never()) {
            h.write(b"palw_court_capacity_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        if let Some(f) = palw_court_transport_v1.filter(|f| f.activation != ForkActivation::never()) {
            h.write(b"palw_court_transport_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.policy.id().as_bytes());
        }
        // RFC-0023 stage 1, Some-only.
        if let Some(at) = palw_unresolved_bound_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_unresolved_bound_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        if let Some(at) = palw_claim_local_settlement_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_claim_local_settlement_v1");
            h.write(at.daa_score().to_le_bytes());
        }
        // RFC-0016's two dormant fences, Some-only for the same reason.
        if let Some(activation) = palw_model_block_rights_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_model_block_rights_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_elastic_final_capacity_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_elastic_final_capacity_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // RFC-0017's model eligibility, Some-only for the same reason.
        if let Some(activation) = palw_model_eligibility_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_model_eligibility_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0179's model tiers, Some-only for the same reason (lists and values beside the height).
        if let Some(fence) = palw_model_tier_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) {
            h.write(b"palw_model_tier_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // G14's per-class detection policy, Some-only for the same reason (each policy id beside the height).
        if let Some(fence) = palw_detection_policy_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) {
            h.write(b"palw_detection_policy_v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // RFC-0018's five fences, Some-only for the same reason.
        if let Some(activation) = palw_dag_sibling_attribution_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_dag_sibling_attribution_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_panel_anchor_permissionless_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_panel_anchor_permissionless_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // The liveness fix L-COURT-01's fence, Some-only for the same reason.
        if let Some(activation) = palw_court_completion_attempt_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_court_completion_attempt_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_rule_e_participation_v2.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_rule_e_participation_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_independently_work_certified_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_independently_work_certified_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_gate_c_attestor_liability_v1.filter(|f| *f != ForkActivation::never()) {
            h.write(b"palw_gate_c_attestor_liability_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(f) = palw_dag_width_schedule_v1.filter(|f| f.activation != ForkActivation::never()) {
            h.write(b"palw_dag_width_schedule_v1");
            h.write(f.activation.daa_score().to_le_bytes());
            h.write(f.schedule.id().as_bytes());
        }
        // palw_fp_prefix_inherit, Some-only for the same reason.
        if let Some(activation) = palw_fp_prefix_inherit {
            h.write(b"palw_fp_prefix_inherit");
            h.write(activation.daa_score().to_le_bytes());
        }
        // palw_fp_prefix_state, Some-only for the same reason.
        if let Some(activation) = palw_fp_prefix_state {
            h.write(b"palw_fp_prefix_state");
            h.write(activation.daa_score().to_le_bytes());
        }
        // palw_fp_tokenizer_match, Some-only for the same reason.
        if let Some(activation) = palw_fp_tokenizer_match {
            h.write(b"palw_fp_tokenizer_match");
            h.write(activation.daa_score().to_le_bytes());
        }
        // palw_fp_constraint_v2, Some-only for the same reason.
        if let Some(activation) = palw_fp_constraint_v2 {
            h.write(b"palw_fp_constraint_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        // palw_adapter_class_v1, Some-only for the same reason.
        if let Some(activation) = palw_adapter_class_v1 {
            h.write(b"palw_adapter_class_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // RFC-0002 Phase F's second IR fence, Some-only: every shipped preset leaves it `None` and
        // fingerprints byte-identically to a build without the field.
        if let Some(activation) = palw_tir_fence2 {
            h.write(b"palw_tir_fence2");
            h.write(activation.daa_score().to_le_bytes());
        }
        // RFC-0006's layer-sharded panels, Some-only: a dormant network fingerprints as a build without the field.
        if let Some(activation) = palw_tir_shard_v1 {
            h.write(b"palw_tir_shard_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0165, Some-only: every shipped preset leaves both `None` and fingerprints byte-identically.
        if let Some(activation) = palw_floor_reserve_v1 {
            h.write(b"palw_floor_reserve_v1");
            h.write(activation.daa_score().to_le_bytes());
            for v in crate::palw_real_share_v1::palw_floor_reserve_value_v1() {
                h.write(v.to_le_bytes());
            }
        }
        if let Some(activation) = palw_real_clock_tick_v1 {
            h.write(b"palw_real_clock_tick_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0170, Some-only: every shipped preset leaves it `None` and fingerprints byte-identically.
        if let Some(activation) = palw_anchor_window_v1 {
            h.write(b"palw_anchor_window_v1");
            h.write(activation.daa_score().to_le_bytes());
            for v in crate::palw_anchor_window_v1::palw_anchor_window_value_v1() {
                h.write(v.to_le_bytes());
            }
        }
        // RFC-0009, Some-only: every shipped preset leaves it `None` and fingerprints byte-identically.
        if let Some(activation) = palw_receipt_spend_v4 {
            h.write(b"palw_receipt_spend_v4");
            h.write(activation.daa_score().to_le_bytes());
            for v in crate::palw_receipt_v4::palw_receipt_spend_v4_value_v1() {
                h.write(v.to_le_bytes());
            }
        }
        // RFC-0009 stage B's provider court, Some-only.
        if let Some(activation) = palw_evidence_court_v1 {
            h.write(b"palw_evidence_court_v1");
            h.write(activation.daa_score().to_le_bytes());
            for v in crate::palw_evidence_court_v1::palw_evidence_court_value_v1() {
                h.write(v.to_le_bytes());
            }
        }
        // Lane PA, Some-only: every shipped preset leaves it `None` and fingerprints byte-identically.
        if let Some(activation) = palw_audit_1004_v1 {
            h.write(b"palw_audit_1004_v1");
            h.write(activation.daa_score().to_le_bytes());
            for v in crate::palw_audit_1004_v1::palw_audit_1004_value_v1() {
                h.write(v.to_le_bytes());
            }
        }
        // RFC-0004, Some-only: every shipped preset leaves it `None` and fingerprints
        // byte-identically to a build without the field. Armed, the ruleset states its scoring
        // library, its sign-test table, the protocol version and its ceilings.
        if let Some(fence) = palw_improvement_v1 {
            h.write(b"palw_improvement_v1/protocol-v1");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // RFC-0003's held leaf challenge, Some-only for the same reason.
        if let Some(activation) = palw_held_close_chunks_v1 {
            h.write(b"palw_held_close_chunks_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // RFC-0007's verification vertex, Some-only for the same reason.
        if let Some(activation) = palw_verification_vertex_v1 {
            h.write(b"palw_verification_vertex_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // RFC-0008 v2's EXEC payload, Some-only for the same reason.
        if let Some(activation) = palw_exec_payload_v2.filter(|activation| *activation != ForkActivation::never()) {
            h.write(b"palw_exec_payload_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_witness_manifest_v1 {
            h.write(b"palw_witness_manifest_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_audit_mesh_v1 {
            h.write(b"palw_audit_mesh_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_capped_onboarding_v1 {
            h.write(b"palw_capped_onboarding_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // RFC-0002 Part II's class-seating fence, Some-only for the same reason, with its value.
        if let Some(fence) = palw_class_seating {
            h.write(b"palw_class_seating");
            h.write(fence.activation.daa_score().to_le_bytes());
            fence.write_value_into(&mut h);
        }
        // RFC-0002 Phase H's IR-only fence, NAMED likewise: it changes which classes may register.
        if let Some(activation) = palw_tir_only_v1 {
            h.write(b"palw_tir_only_v1");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0093 Decision 6, Some-only for the same reason: a dormant network fingerprints
        // byte-identically to a build without the field.
        if let Some(activation) = palw_fused_dissectable {
            h.write(b"palw_fused_dissectable");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0093 Decision 8, Some-only for the same reason.
        if let Some(activation) = palw_attn_anchored_root {
            h.write(b"palw_attn_anchored_root");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0103, Some-only for the same reason: a dormant network fingerprints byte-identically
        // to a build without the field, the map, the objects or the gate.
        if let Some(activation) = palw_held_context {
            h.write(b"palw_held_context");
            h.write(activation.daa_score().to_le_bytes());
            // ADR-0118 Decision 1: the contexts the fence commits, by their root, where the bundle
            // names an older set — a fence armed past genesis — so every context a held object is
            // verified under is in the identity two nodes compare. A bundle that states V4 (every
            // mint that takes the regime from genesis) commits them through the ruleset id already,
            // and its identity does not move.
            // ADR-0152 IMPL-7: a bundle stating V5 commits V4's contexts through the ruleset id too.
            let v4 = crate::palw_mode_v2::palw_v2_signature_contexts_root_v4();
            if !matches!(
                palw_consensus_mode,
                crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle)
                    if crate::palw_mode_v2::palw_v2_signature_contexts_root_covers_v4(&bundle.signature_contexts_root)
            ) {
                h.write(b"palw_held_context/contexts");
                h.write(v4.as_bytes());
            }
        }
        // ADR-0117, Some-only for the same reason: a dormant network fingerprints byte-identically
        // to a build without the field.
        if let Some(activation) = palw_prefill_draw {
            h.write(b"palw_prefill_draw");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_audit_2026_09_11 {
            h.write(b"palw_audit_2026_09_11");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_audit_2026_09_11_deep {
            h.write(b"palw_audit_2026_09_11_deep");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_audit_2026_09_23 {
            h.write(b"palw_audit_2026_09_23");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(depth) = palw_settled_anchor_depth {
            h.write(b"palw_settled_anchor_depth");
            h.write(depth.to_le_bytes());
        }
        if let Some(period) = palw_admission_audit_period_daa {
            h.write(b"palw_admission_audit_period_daa");
            h.write(period.to_le_bytes());
        }
        // ADR-0151's stated maturity (user decision 2026-09-25), Some-only: it decides when a Final's
        // execution rights become spendable and what the seat lock prices, so a node that states it
        // and one that does not are different rules — and every preset that leaves it `None`
        // fingerprints byte-identically to a build without the field.
        if let Some(maturity) = palw_exec_quantum_maturity_daa {
            h.write(b"palw_exec_quantum_maturity_daa");
            h.write(maturity.to_le_bytes());
        }
        // ADR-0083 Decision 1, Some-only for the same reason: arming it changes the `bits` every
        // header past the fence must carry, so it belongs in the fingerprint — and every shipped
        // preset leaves it `None` and fingerprints byte-identically to a build without the field.
        if let Some(activation) = palw_difficulty_priced_rows {
            h.write(b"palw_difficulty_priced_rows");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_receipt_rows_unpriced {
            h.write(b"palw_receipt_rows_unpriced");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0072 D8 at the header stage. Some-only, for the reason above — both ids gain it
        // together: a fence in one and not the other is two builds that agree on identity and
        // disagree on schedule.
        if let Some(activation) = palw_attempt_header_pins {
            h.write(b"palw_attempt_header_pins");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0072 D8 on the free-prompt lane. Some-only, and both ids gain it together.
        if let Some(activation) = palw_fp_da_pins {
            h.write(b"palw_fp_da_pins");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0018 §E's payout bounds (mainnet audit 2026-09-06). Some-only, at the tail, for the
        // ADR-0065 D4 reason: every shipped preset leaves it `None` and fingerprints
        // byte-identically to a build from before the field existed.
        if let Some(activation) = palw_validator_payout_bounds {
            h.write(b"palw_validator_payout_bounds");
            h.write(activation.daa_score().to_le_bytes());
        }
        // MSK-26A: the slashing-evidence UTXO genuineness fence. Some-only, for its siblings' reason.
        if let Some(activation) = palw_slashing_evidence_utxo_genuine {
            h.write(b"palw_slashing_evidence_utxo_genuine");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0045 D2's boundary budget and ADR-0044 D9's free-prompt caps (mainnet audit
        // 2026-09-06, M-2 and L-2). Some-only, at the tail, for the ADR-0065 D4 reason: every
        // shipped preset leaves them `None` and fingerprints byte-identically to a build from
        // before the fields existed.
        if let Some(activation) = palw_epoch_boundary_budget {
            h.write(b"palw_epoch_boundary_budget");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_epoch_budget_release {
            h.write(b"palw_epoch_budget_release");
            h.write(activation.daa_score().to_le_bytes());
        }
        if let Some(activation) = palw_panel_economy {
            h.write(b"palw_panel_economy");
            h.write(activation.daa_score().to_le_bytes());
            // **ADR-0130: the draw past the economy is one lottery entry per OPERATOR**, not one
            // ticket per bond — a different panel for the same claim and registry. Named here, beside
            // the fence it rides, so a build that tickets bonds and one that tickets operators never
            // print one `consensus_params_id` for the same schedule.
            h.write(b"palw_panel_draw/operator_ticket_v1");
        }
        if let Some(activation) = palw_work_priced_reward {
            h.write(b"palw_work_priced_reward");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0125: Some-only, so every preset that leaves the lane closed fingerprints exactly as a
        // build without the field.
        if let Some(lane) = palw_execution_lane {
            h.write(b"palw_execution_lane/v1");
            // ADR-0130: the scheduler's rule set is named, so a build that derives schedules by
            // ADR-0125's first rule (a span's own finals, seeded by their claim ids and roots) and
            // one on this rule print different fingerprints for the same lane shape.
            h.write(b"palw_execution_lane/scheduler_v2");
            h.write(lane.activation.daa_score().to_le_bytes());
            h.write(lane.permits_per_round.to_le_bytes());
            h.write(lane.max_per_mergeset.to_le_bytes());
            h.write(lane.schedule_span_daa.to_le_bytes());
            // §7.2: Some-only per slot, so a lane with no widening fingerprints as it did.
            for stage in lane.widenings.iter().filter(|stage| stage.is_used()) {
                h.write(b"widening");
                h.write(stage.activation.daa_score().to_le_bytes());
                h.write(stage.permits_per_round.to_le_bytes());
            }
            if lane.short_span.is_used() {
                h.write(b"span_short");
                h.write(lane.short_span.activation.daa_score().to_le_bytes());
                h.write(lane.short_span.schedule_span_daa.to_le_bytes());
            }
        }
        // ADR-0126: Some-only, so every preset that leaves the carve unset fingerprints exactly as a
        // build without the field; the two numbers ride with the height.
        if let Some(r) = palw_dns_retirement {
            h.write(b"palw_dns_retirement_v1");
            h.write(r.commitment_bytes());
        }
        if let Some(r) = palw_permissionless_panel_v1.filter(|r| r.activation != ForkActivation::never()) {
            h.write(b"palw_permissionless_panel_v1");
            h.write(r.commitment_bytes());
        }
        if let Some(r) = palw_permissionless_panel_complete_v1.filter(|r| r.activation != ForkActivation::never()) {
            h.write(b"palw_permissionless_panel_complete_v1");
            h.write(r.commitment_bytes());
        }
        if let Some(r) = palw_permissionless_panel_retired_v1.filter(|r| r.activation != ForkActivation::never()) {
            h.write(b"palw_permissionless_panel_retired_v1");
            h.write(r.commitment_bytes());
        }
        if let Some(carve) = palw_overlay_carve {
            h.write(b"palw_overlay_carve/v1");
            h.write(carve.activation.daa_score().to_le_bytes());
            h.write(carve.subsidy_validator_bps.to_le_bytes());
            h.write(carve.worker_carve_permille.to_le_bytes());
        }
        // ADR-0130: Some-only, so every preset that leaves the floor unset fingerprints exactly as a
        // build without the field; the reward multiple rides with the height.
        if let Some(floor) = palw_panel_exposure_floor {
            h.write(b"palw_panel_exposure_floor/v1");
            h.write(floor.activation.daa_score().to_le_bytes());
            h.write(floor.reward_multiple_permille.to_le_bytes());
        }
        if let Some(activation) = palw_fp_ruleset_caps {
            h.write(b"palw_fp_ruleset_caps");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0042 Decision 11's complete context set (mainnet audit 2026-09-06, M-8). Some-only,
        // like every fence above it: arming it changes what set of signing contexts the ruleset
        // commits to, so it belongs in the fingerprint — and every shipped preset leaves it `None`
        // and fingerprints byte-identically to a build without the field at all. Both ids gain it
        // together, for the reason `palw_attempt_header_pins` states.
        if let Some(activation) = palw_signature_contexts_v2 {
            h.write(b"palw_signature_contexts_v2");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0105's coloring fence. WRITTEN, not only destructured — ADR-0095's
        // `palw_model_benefits` is the fence that was bound here and never hashed (0448d955), which
        // left the printed fingerprint unable to tell the builds on either side of DAA 2400 apart.
        // Some-only, like every fence above it: arming it changes which blocks are blue and so the
        // blue work every later header carries, and a preset that leaves it `None` fingerprints
        // byte-identically to a build without the field at all.
        if let Some(activation) = palw_heartbeat_transparent {
            h.write(b"palw_heartbeat_transparent");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ---- F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
        // WRITTEN, Some-only: arming it changes which blocks are blue past its height, and every
        // preset that leaves it `None` — all of them, testnet-12 included — fingerprints byte-identically
        // to a build without the field.
        if let Some(activation) = palw_heartbeat_transparent_same_chain {
            h.write(b"palw_heartbeat_transparent_same_chain");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ---- end F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
        // ADR-0160 lane escrow: WRITTEN, Some-only — every preset leaves it `None` and fingerprints
        // byte-identically to a build without the field.
        if let Some(activation) = palw_capacity_escrow_at_licence {
            h.write(b"palw_capacity_escrow_at_licence");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0107. Some-only, like every fence above it: every preset leaves it `None` and
        // fingerprints byte-identically to a build without the field.
        if let Some(activation) = palw_share_growth_final {
            h.write(b"palw_share_growth_final");
            h.write(activation.daa_score().to_le_bytes());
        }
        // P0a. Some-only, likewise: every preset leaves it `None`.
        if let Some(activation) = palw_gdn_key_heads {
            h.write(b"palw_gdn_key_heads");
            h.write(activation.daa_score().to_le_bytes());
        }
        // ADR-0042 Decisions 1 + 11: the V2 mode decides block validity wholesale, so it is in
        // the fingerprint — through the RULESET ID, one hash for the whole atomic bundle, which
        // is the same value the V2 handshake exchanges (two commitments cannot drift when one is
        // derived from the other). Non-Disabled-only, so every shipped preset fingerprints
        // byte-identically to before the field existed. `LegacyTn11` is a tagged byte: the mode
        // marks the lineage while the legacy knobs (already hashed above) carry the values.
        match palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::Disabled => {}
            crate::palw_mode_v2::PalwConsensusMode::LegacyTn11 => {
                h.write([1u8]);
            }
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                h.write([2u8]);
                h.write(crate::palw_mode_v2::palw_ruleset_id_v2(bundle).as_byte_slice());
                // ADR-0058: the state-machine VERSION is block validity — every root is
                // domain-separated by it — and each earlier bump moved this fingerprint only
                // because it happened to ride a bundle-shape change. Hashed explicitly so a
                // semantic-only bump parts old and new nodes at handshake instead of at their
                // first disagreeing root. Inside the V2 arm, so every non-V2 preset
                // fingerprints byte-identically to before.
                h.write(crate::palw_state_v2::PALW_STATE_V2_VERSION.to_le_bytes());
            }
        }

        // The bridge ledger: finite-only, like the Some-only fences above, so a network that leaves
        // it inert (every preset but testnet-12) fingerprints byte-identically to a build without it.
        if *evm_bridge_ledger_activation_daa_score != u64::MAX {
            h.write(b"evm_bridge_ledger");
            h.write(evm_bridge_ledger_activation_daa_score.to_le_bytes());
        }

        // **ADR-0150: the rules, beside the heights.** The arms above hash a fenced rule's HEIGHT
        // and never the rule it turns on, so a build that redefines what a fence admits — at a
        // height nobody moved — fingerprints identically to one that does not, peers with it, and
        // splits from it the moment the fence fires. The line above is the same idea applied once,
        // by hand, to the state machine's version; this is that idea as a list, so the next rule
        // change is a manifest entry a reviewer reads rather than a silence nobody can see.
        //
        // Gated, not unconditional: a ruleset whose fence this network does not set contributes
        // nothing, and the identity's normalisation (a scheduled fence becomes "not yet", which
        // reads as absent) is what keeps a rollout across a FUTURE rule change a rollout.
        // Silent where every ruleset is still R1 (see the module): a build that has changed no
        // rule writes no bytes here and fingerprints exactly as the build before the manifest did,
        // so landing this partitions nobody. testnet-11 is the one preset that moves today, and it
        // moves because its readiness rule really did.
        if let Some(manifest) =
            crate::palw_rule_manifest_v1::palw_rule_manifest_digest_for_gates_v1(&self.palw_rule_manifest_gates_v1())
        {
            h.write(b"palw_rule_manifest");
            h.write(manifest.as_byte_slice());
        }

        h.finalize()
    }
}
