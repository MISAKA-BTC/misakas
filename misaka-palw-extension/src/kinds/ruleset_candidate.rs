//! **`ruleset-candidate`** (ADR-0108 Decision 6): a fence and a height, described and costed —
//! never activated. The verifier takes this build's `Params` for the network, sets the fences as
//! asked, and reports what the arming build would print beside what this build prints, whether
//! the identity moves, and whether the build refuses the combination. The classification is
//! always `RulesetChange`.

use kaspa_consensus_core::config::params::{ForkActivation, Params};

use crate::manifest::{PalwExtensionError, PalwFenceRequestV1};
use crate::report::{PalwExtensionClassificationV1, PalwFenceDeltaV1, PalwWouldPrintV1};
use crate::verify::{KindOutcomeV1, VerifyCx, fence_value_string};

/// Set one fence of `Params` by the name `palw_fences_v1` gives it. One arm per name, so a fence
/// added to `Params` reaches here only when somebody spells it (the test in this module walks the
/// build's list). A Some-only fence that carries a companion value cannot be armed from nothing:
/// its activation is set where the preset already carries the value, and refused otherwise.
pub fn set_fence_by_name(params: &mut Params, name: &str, at: ForkActivation) -> Result<(), String> {
    fn companion<T>(
        slot: &mut Option<T>,
        name: &str,
        what: &str,
        at: ForkActivation,
        set: impl FnOnce(&mut T, ForkActivation),
    ) -> Result<(), String> {
        match slot.as_mut() {
            Some(value) => {
                set(value, at);
                Ok(())
            }
            None => Err(format!(
                "`{name}` carries a companion value ({what}) this preset does not set — the candidate needs a build before it needs a height"
            )),
        }
    }
    /// A fence `validate_palw_v2` refuses anywhere but at genesis: a candidate at a height is refused
    /// by name, before any fingerprint is printed for a ruleset no build can run.
    fn genesis_only(name: &str, at: ForkActivation) -> Result<(), String> {
        if at == ForkActivation::always() {
            Ok(())
        } else {
            Err(format!(
                "`{name}` is genesis-only: it is armed at genesis or not at all, so a candidate at a height ({}) is a \
                 regenesis, not a flag day",
                at.daa_score()
            ))
        }
    }
    match name {
        "palw_bootstrap_activation" => params.palw_bootstrap_activation = Some(at),
        "palw_unavailable_abstains" => params.palw_unavailable_abstains = Some(at),
        "palw_bond_maturity" => companion(&mut params.palw_bond_maturity, name, "window_daa", at, |f, at| f.activation = at)?,
        // Lane maturity (post-launch, 2026-09-26): a bare height; `validate_palw_v2` refuses it without
        // `palw_bond_maturity` scheduled above it.
        "palw_bond_maturity_early" => params.palw_bond_maturity_early = Some(at),
        "palw_frontier_provenance" => params.palw_frontier_provenance = Some(at),
        // lane rcore/f1-forkchoice-attacks (post-launch): a bare height — a deep reorg needs a strict
        // economic win past it.
        "palw_reorg_strict_economic_win" => params.palw_reorg_strict_economic_win = Some(at),
        // rcore/hf-pptake2 (post-launch): a bare height — a pruning-proof / IBD staging commit needs a
        // strict economic win past it, read at the incumbent's DAA.
        "palw_pruning_proof_strict_economic_win" => params.palw_pruning_proof_strict_economic_win = Some(at),
        // lane rcore/cap-weight (ADR-0160 F-W, post-launch): the V2 bundle mirrors the height the fold
        // reads, and `validate_palw_v2` refuses the two apart — set together. Needs R-core+, the
        // strict-economic-win reorg rule and lane A's operator anchor at or below it.
        "palw_capacity_weight_cap" => {
            params.palw_capacity_weight_cap = Some(at);
            params.sync_palw_capacity_weight_cap();
        }
        "palw_heartbeat" => {
            companion(&mut params.palw_heartbeat, name, "work_log2 and the mergeset bound", at, |f, at| f.activation = at)?
        }
        "palw_attempt_work" => {
            companion(&mut params.palw_attempt_work, name, "work_log2 and the nonce budget", at, |f, at| f.activation = at)?
        }
        "palw_attempt_activation" => params.palw_attempt_activation = Some(at),
        "dns_bft_gate" => {
            companion(&mut params.dns_bft_gate, name, "t_leak_daa, the re-entry depth and the validator floor", at, |f, at| {
                f.activation = at
            })?
        }
        "palw_beacon_fold" => companion(&mut params.palw_beacon_fold, name, "k", at, |f, at| f.activation = at)?,
        "palw_capability_bound" => params.palw_capability_bound = Some(at),
        "palw_compute_overlay_retired" => params.palw_compute_overlay_retired = Some(at),
        "palw_context_ladder" => params.palw_context_ladder = Some(at),
        "palw_panel_da" => params.palw_panel_da = Some(at),
        "palw_certification_rent" => params.palw_certification_rent = Some(at),
        "palw_uncertified_weightless" => params.palw_uncertified_weightless = Some(at),
        "palw_da_court" => params.palw_da_court = Some(at),
        "palw_court_ladder" => params.palw_court_ladder = Some(at),
        "palw_fp_da_pins" => params.palw_fp_da_pins = Some(at),
        "palw_validator_payout_bounds" => params.palw_validator_payout_bounds = Some(at),
        "palw_slashing_evidence_utxo_genuine" => params.palw_slashing_evidence_utxo_genuine = Some(at),
        "palw_epoch_boundary_budget" => params.palw_epoch_boundary_budget = Some(at),
        "palw_epoch_budget_release" => params.palw_epoch_budget_release = Some(at),
        "palw_fp_ruleset_caps" => params.palw_fp_ruleset_caps = Some(at),
        "palw_model_market" => params.palw_model_market = Some(at),
        "palw_model_lines" => params.palw_model_lines = Some(at),
        "palw_model_benefits" => params.palw_model_benefits = Some(at),
        "palw_model_leg_v2" => params.palw_model_leg_v2 = Some(at),
        "palw_model_seed_v2" => params.palw_model_seed_v2 = Some(at),
        // ADR-0162: a bare height, folded with the market by `palw_model_virtual_v1_fence`.
        "palw_model_virtual_v1" => params.palw_model_virtual_v1 = Some(at),
        "palw_model_evm" => params.palw_model_evm = Some(at),
        // Lane sink (the model sink binding, post-launch): a bare height; `validate_palw_v2` refuses it
        // without the market and the 2026-09-23 audit fence at or below it.
        "palw_model_sink_bound" => params.palw_model_sink_bound = Some(at),
        // ADR-0160 lane verify (F-B, F-R): bare heights with the fold's mirrors.
        "palw_capacity_batch_licence" => {
            params.palw_capacity_batch_licence = Some(at);
            params.sync_palw_capacity_verify();
        }
        "palw_capacity_verify_room" => {
            params.palw_capacity_verify_room = Some(at);
            params.sync_palw_capacity_verify();
        }
        // ADR-0160 stage 2 (F-Q, F-S): bare heights with the fold's mirrors.
        "palw_capacity_audit_door" => {
            params.palw_capacity_audit_door = Some(at);
            params.sync_palw_capacity_stage2();
        }
        "palw_capacity_issuance_slots" => {
            params.palw_capacity_issuance_slots = Some(at);
            params.sync_palw_capacity_stage2();
        }
        // ADR-0160 stage 4 (F-N): a bare height with the fold's mirror.
        "palw_capacity_network_room" => {
            params.palw_capacity_network_room = Some(at);
            params.sync_palw_capacity_stage2();
        }
        // int-11: F-N's static verification term, a bare height with the fold's mirror.
        "palw_capacity_network_verify" => {
            params.palw_capacity_network_verify = Some(at);
            params.sync_palw_capacity_network_verify();
        }
        // Lane PL (ADR-0166): bare heights with the fold's mirrors.
        "palw_panel_unavailable_expiry" => {
            params.palw_panel_unavailable_expiry = Some(at);
            params.sync_palw_panel_unavailable_expiry();
        }
        "palw_panel_fast_switch" => {
            params.palw_panel_fast_switch = Some(at);
            params.sync_palw_panel_fast_switch();
        }
        "palw_seat_availability" => {
            params.palw_seat_availability = Some(at);
            params.sync_palw_seat_availability();
        }
        // ADR-0164 (stages 5–7): the three bare fences with their shared mirror.
        "palw_capacity_emission_budget" => {
            params.palw_capacity_emission_budget = Some(at);
            params.sync_palw_capacity_s567();
        }
        "palw_capacity_multi_claim" => {
            params.palw_capacity_multi_claim = Some(at);
            params.sync_palw_capacity_s567();
        }
        "palw_capacity_rho_breaker" => {
            params.palw_capacity_rho_breaker = Some(at);
            params.sync_palw_capacity_s567();
        }
        "palw_chunk_cap_charge" => params.palw_chunk_cap_charge = Some(at),
        "palw_prompt_ids_merkle" => params.palw_prompt_ids_merkle = Some(at),
        "palw_kary_court" => params.palw_kary_court = Some(at),
        "palw_court_responder_coverage" => params.palw_court_responder_coverage = Some(at),
        "palw_fp_decode_rules" => params.palw_fp_decode_rules = Some(at),
        "palw_fp_decode_constraint" => params.palw_fp_decode_constraint = Some(at),
        "palw_shard_court" => params.palw_shard_court = Some(at),
        "palw_shard_licensing" => params.palw_shard_licensing = Some(at),
        "palw_token_lift" => params.palw_token_lift = Some(at),
        "palw_kimi_k3" => params.palw_kimi_k3 = Some(at),
        "palw_gdn_key_heads" => params.palw_gdn_key_heads = Some(at),
        "palw_fused_dissectable" => params.palw_fused_dissectable = Some(at),
        "palw_attn_anchored_root" => params.palw_attn_anchored_root = Some(at),
        "palw_held_context" => params.palw_held_context = Some(at),
        "palw_prefill_draw" => params.palw_prefill_draw = Some(at),
        "palw_audit_2026_09_11" => params.palw_audit_2026_09_11 = Some(at),
        "palw_audit_2026_09_11_deep" => params.palw_audit_2026_09_11_deep = Some(at),
        "palw_difficulty_priced_rows" => params.palw_difficulty_priced_rows = Some(at),
        "palw_receipt_rows_unpriced" => params.palw_receipt_rows_unpriced = Some(at),
        "palw_attempt_header_pins" => params.palw_attempt_header_pins = Some(at),
        "palw_signature_contexts_v2" => params.palw_signature_contexts_v2 = Some(at),
        "palw_heartbeat_transparent" => params.palw_heartbeat_transparent = Some(at),
        // F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency).
        "palw_heartbeat_transparent_same_chain" => params.palw_heartbeat_transparent_same_chain = Some(at),
        // ADR-0160 lane escrow (post-launch): a bare height.
        "palw_capacity_escrow_at_licence" => params.palw_capacity_escrow_at_licence = Some(at),
        "palw_share_growth_final" => params.palw_share_growth_final = Some(at),
        "palw_model_registry" => params.palw_model_registry = Some(at),
        "palw_economic_payout" => {
            companion(&mut params.palw_economic_payout, name, "rate_sompi_per_giga", at, |f, at| f.activation = at)?
        }
        "palw_work_target" => params.palw_work_target = Some(at),
        "palw_single_lottery" => params.palw_single_lottery = Some(at),
        "palw_short_challenge_window" => params.set_palw_short_challenge_window(Some(at)),
        "palw_verification_v2" => params.palw_verification_v2 = Some(at),
        "palw_verification_s3" => params.palw_verification_s3 = Some(at),
        "palw_verification_s2" => params.palw_verification_s2 = Some(at),
        "palw_readiness_v2" => params.palw_readiness_v2 = Some(at),
        "palw_anchor_clock" => params.palw_anchor_clock = Some(at),
        "palw_clock_cursor" => params.palw_clock_cursor = Some(at),
        "palw_clock_floor" => params.palw_clock_floor = Some(at),
        "palw_clock_lead_cap" => params.palw_clock_lead_cap = Some(at),
        // ADR-0152 §4-ter: the V2 bundle mirrors the held classes this fence makes unanswerable, and
        // `validate_palw_v2` refuses the two apart — set together.
        "palw_offence_attribution" => {
            params.palw_offence_attribution = Some(at);
            params.sync_palw_held_answerability();
        }
        // ADR-0152 R-core+: the V2 bundle mirrors this height (`rcore_plus_active_at`, the bond
        // withdrawal delay, the C7 list), and `validate_palw_rcore_plus_v1` refuses the two apart —
        // set together, as `palw_audit_2026_09_23` is.
        "palw_rcore_plus" => {
            params.palw_rcore_plus = Some(at);
            params.sync_palw_rcore_plus();
        }
        // Lane F1 (the panel seed, post-launch): a bare height; `validate_palw_v2` refuses it without
        // R-core+ at or below it.
        "palw_panel_seed_execution" => params.palw_panel_seed_execution = Some(at),
        // Lane A (the operator anchor, post-launch): the height over every bond the genesis registers
        // (testnet-12's eight operator cards); `validate_palw_v2` refuses it without R-core+ at or below it
        // and off ConsensusV2.
        "palw_operator_anchor" => params.palw_operator_anchor = params.palw_operator_anchor_of_genesis_bonds_v1(at),
        // Lane V02 (a resolved claim's lock off the work ceiling, post-launch): the V2 bundle mirrors the
        // height and `validate_palw_v2` refuses the two apart — set together; refused below R-core+.
        "palw_final_lock_full_collateral" => {
            params.palw_final_lock_full_collateral = Some(at);
            params.sync_palw_final_lock_full_collateral();
        }
        // Lane V02 (the shortened post-Final lock life, post-launch): the V2 bundle mirrors the height and
        // `validate_palw_v2` refuses the two apart — set together; refused below R-core+.
        "palw_final_lock_life" => {
            params.palw_final_lock_life = Some(at);
            params.sync_palw_final_lock_life();
        }
        // Lane F2-lock (the lock life applied retroactively, post-launch): the V2 bundle mirrors the height
        // and `validate_palw_v2` refuses the two apart — set together; refused without lane V02's lock life
        // at or below it.
        "palw_final_lock_life_retro" => {
            params.palw_final_lock_life_retro = Some(at);
            params.sync_palw_final_lock_life_retro();
        }
        // ADR-0152-adjacent (Activation Pool): genesis-only (R1 and R2 change how every class is
        // reclaimed and stepped), so a height is refused here by name. At genesis the terms this preset
        // carries are kept, and a preset that carries none takes the user's illustrative scale — the
        // numbers `validate_palw_v2` checks either way.
        "palw_activation_pool" => {
            genesis_only(name, at)?;
            let terms = params.palw_activation_pool.map(|pool| pool.terms).unwrap_or_default();
            params.palw_activation_pool = Some(kaspa_consensus_core::config::params::PalwActivationPoolParamsV1 { activation: at, terms });
        }
        // The readiness-V2 horizon (user decision 2026-09-25, readiness capacity option (a)): genesis-only
        // (a crossing would count one row by two horizons), so a height is refused here by name. At
        // genesis the horizon this preset carries is kept, a preset that carries none takes
        // testnet-12's twenty-four, and the bundle's mirror is set with it — `validate_palw_v2`
        // refuses the two apart.
        "palw_readiness_v2_max_age_spans" => {
            genesis_only(name, at)?;
            let max_age_spans = params
                .palw_readiness_v2_max_age_spans
                .map(|horizon| horizon.max_age_spans)
                .unwrap_or(kaspa_consensus_core::palw_model_registry_v1::PALW_READINESS_V2_MAX_AGE_SPANS_T12_V1);
            params.palw_readiness_v2_max_age_spans =
                Some(kaspa_consensus_core::config::params::PalwReadinessV2MaxAgeParamsV1 { activation: at, max_age_spans });
            params.sync_palw_readiness_v2_max_age_spans();
        }
        // Lane F1 (registry resilience, V03/V05): the V2 bundle mirrors the height the fold reads, and
        // `validate_palw_v2` refuses the two apart — set together.
        "palw_registry_resilience" => {
            params.palw_registry_resilience = Some(at);
            params.sync_palw_registry_resilience();
        }
        // Lane F2 (the floor-refusal retry, post-launch): the V2 bundle mirrors the height the fold reads,
        // and `validate_palw_v2` refuses the two apart — set together; refused below R-core+.
        "palw_floor_refusal_retry" => {
            params.palw_floor_refusal_retry = Some(at);
            params.sync_palw_floor_refusal_retry();
        }
        "palw_artifact_root_ownership" => params.palw_artifact_root_ownership = Some(at),
        "palw_operator_id_unique" => params.palw_operator_id_unique = Some(at),
        "palw_objective_offence" => params.palw_objective_offence = Some(at),
        "palw_seat_gate_possession" => params.palw_seat_gate_possession = Some(at),
        // The V2 bundle carries this height (and the lane's span) beside the fence;
        // `validate_palw_v2` refuses the two apart, so they are set together.
        "palw_class_receipt_window" => params.set_palw_class_receipt_window(Some(at)),
        // ADR-0152 §4-quater: genesis-only (the pruning depth its deadlines need is a genesis fact), so a
        // height is refused here by name rather than left to surface as a validation failure; at
        // genesis the bundle carries the height beside the fence and `validate_palw_v2` refuses the two
        // apart, so they are set together.
        "palw_class_verify_deadline" => {
            genesis_only(name, at)?;
            params.palw_class_verify_deadline = Some(at);
            params.sync_palw_class_verify_deadline();
        }
        // Lane bind-deadlock (post-launch): a bare height; `validate_palw_v2` refuses it without
        // R-core+ and lane A (`palw_operator_anchor`) at or below it.
        "palw_anchor_at_ceiling" => params.palw_anchor_at_ceiling = Some(at),
        // Lane accept-order (post-launch): a bare height; `validate_palw_v2` refuses it off ConsensusV2 or
        // without the execution lane at or below it.
        "palw_lane_accept_parents_first" => params.palw_lane_accept_parents_first = Some(at),
        "palw_execution_quanta" => params.palw_execution_quanta = Some(at),
        "palw_public_model_source_required" => {
            params.palw_public_model_source_required =
                Some(kaspa_consensus_core::config::params::PalwPublicModelSourceRuleV1 { activation: at })
        }
        "palw_canonical_work" => params.palw_canonical_work = Some(at),
        "palw_admission_independence" => params.palw_admission_independence = Some(at),
        "palw_fp_derived_work" => params.palw_fp_derived_work = Some(at),
        // Option A (ADR-0151): the V2 bundle mirrors this height as `escrow_backed_exposure_from_daa`,
        // and `validate_palw_v2` refuses the two apart — set together.
        "palw_audit_2026_09_23" => {
            params.palw_audit_2026_09_23 = Some(at);
            params.sync_palw_escrow_backed_exposure();
        }
        "palw_panel_economy" => params.palw_panel_economy = Some(at),
        "palw_work_priced_reward" => params.palw_work_priced_reward = Some(at),
        "palw_overlay_carve" => {
            companion(&mut params.palw_overlay_carve, name, "the validator share and the escrow carve", at, |f, at| f.activation = at)?
        }
        "palw_panel_exposure_floor" => {
            companion(&mut params.palw_panel_exposure_floor, name, "the reward multiple", at, |f, at| f.activation = at)?
        }
        "palw_execution_lane" => companion(&mut params.palw_execution_lane, name, "the lane's shape", at, |f, at| f.activation = at)?,
        widening if widening.starts_with("palw_execution_lane_widening_") => {
            let slot: usize = widening["palw_execution_lane_widening_".len()..]
                .parse()
                .map_err(|_| format!("`{name}` does not name a widening slot"))?;
            let index = slot.checked_sub(1).filter(|i| *i < kaspa_consensus_core::palw_execution_lane_v1::PALW_EXEC_MAX_WIDENINGS_V1);
            let Some(index) = index else {
                return Err(format!("`{name}` does not name a widening slot this build has"));
            };
            companion(&mut params.palw_execution_lane, name, "the widened width", at, |f, at| {
                if f.widenings[index].is_used() {
                    f.widenings[index].activation = at
                }
            })?;
            if !params.palw_execution_lane.is_some_and(|lane| lane.widenings[index].is_used()) {
                return Err(format!(
                    "`{name}` carries a companion value (the widened width) this preset does not set — the candidate needs a build before it needs a height"
                ));
            }
        }
        "palw_execution_lane_span_short" => {
            companion(&mut params.palw_execution_lane, name, "the shortened span", at, |f, at| {
                if f.short_span.is_used() {
                    f.short_span.activation = at
                }
            })?;
            if !params.palw_execution_lane.is_some_and(|lane| lane.short_span.is_used()) {
                return Err(format!(
                    "`{name}` carries a companion value (the shortened span) this preset does not set — the candidate needs a build before it needs a height"
                ));
            }
        }
        // ADR-0160 lane liab (F-L, rcore/cap-s1): a value fence — its value is the ramp's schedule, so
        // the height is set where the preset carries one (and the fold's mirror re-synced); each later
        // step's slot is a step of that schedule, which a height alone cannot spell.
        "palw_capacity_aggregate_liability" => {
            companion(&mut params.palw_capacity_aggregate_liability, name, "the ramp's schedule", at, |f, at| f.activation = at)?;
            params.sync_palw_capacity_liability();
        }
        step if step.starts_with("palw_capacity_aggregate_liability_step_") => {
            return Err(format!(
                "`{step}` carries a companion value (the step's ρ and credit) this preset does not set — the candidate needs a \
                 build before it needs a height"
            ));
        }
        // **Every fence the testnet-12 flag days arm, and the ones armed nowhere (the model court window, `palw_tir_only_v1`),
        // by the fence's OWN entry** — the setter the release list, the drill and the tests use, which writes the field and the
        // fold's mirror together. One table, so a fence added to a flag-day list is spelled here by being on it; a name on no list
        // still needs its own arm above.
        other => match palw_candidate_entry_v1(other) {
            Some(entry) => (entry.set)(params, Some(at)),
            None => return Err(format!("this build has no fence `{other}`: the candidate needs a build before it needs a height")),
        },
    }
    Ok(())
}

/// The flag-day entry that spells `name`, if one does: the int-11/int-12 list, the IR flag days' lists and the two fences no
/// testnet-12 list arms.
fn palw_candidate_entry_v1(name: &str) -> Option<&'static kaspa_consensus_core::config::params::PalwPostLaunchFenceV1> {
    use kaspa_consensus_core::config::params::{
        PALW_T12_INT11_FENCES_V1, PALW_T12_MODEL_COURT_WINDOW_ENTRY, PALW_T12_TIR_FENCE2_FENCES_V1, PALW_T12_TIR_FLAG_DAY_FENCES_V1,
    };
    PALW_T12_INT11_FENCES_V1
        .iter()
        .chain(PALW_T12_TIR_FLAG_DAY_FENCES_V1)
        .chain(PALW_T12_TIR_FENCE2_FENCES_V1)
        .chain(std::iter::once(&PALW_T12_MODEL_COURT_WINDOW_ENTRY))
        .chain(kaspa_consensus_core::palw_tir_only_v1::PALW_DRILL_TIR_ONLY_FENCES_V1)
        .find(|entry| entry.name == name)
}

/// The four fingerprints of one `Params`, as the report prints them.
pub fn would_print_v1(params: &Params) -> PalwWouldPrintV1 {
    PalwWouldPrintV1 {
        params_id: params.consensus_params_id().to_string(),
        identity_id: params.consensus_identity_id().to_string(),
        schedule_id: params.consensus_schedule_id().to_string(),
        fence_schedule: params.fence_schedule_v1(),
    }
}

/// **The arming build's derived depths, re-derived after its fences are set.** A V2 network's
/// `finality_depth` and `pruning_depth` are derived from its bundle and fences, not carried: past
/// `palw_class_verify_deadline` the pruning depth is the D_cap claim lattice (ADR-0152 §4-quater
/// P-1 — 74,920 on testnet-12, against 12,002 without the fence). The arming build derives them
/// after its fences are set ([`Params::with_palw_v2_depths`], raised, never lowered), so the
/// candidate does the same — else a genesis candidate for that fence prints a params id the arming
/// build never prints, and trips K18 on a depth the arming build would never carry. A V1 network is
/// returned as it is.
pub fn with_derived_depths_v1(arming: Params) -> Params {
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &arming.palw_consensus_mode else {
        return arming;
    };
    let bundle = bundle.clone();
    arming.with_palw_v2_depths(&bundle)
}

pub(crate) fn verify(cx: &mut VerifyCx<'_>) -> Result<KindOutcomeV1, PalwExtensionError> {
    let manifest = cx.manifest();
    let requested: Vec<(String, PalwFenceRequestV1)> = manifest.requires.fences.iter().map(|(n, r)| (n.clone(), r.clone())).collect();
    if requested.is_empty() {
        return Ok(KindOutcomeV1::at(
            cx.refuse("requires.fences", "a candidate names at least one fence, at `genesis` or a height"),
            cx.depth,
        ));
    }
    let this_build = would_print_v1(&cx.params);
    cx.record("this_build.params_id", &this_build.params_id);
    cx.record("this_build.identity_id", &this_build.identity_id);
    cx.record("this_build.schedule_id", &this_build.schedule_id);

    let mut arming = cx.params.clone();
    let mut deltas = Vec::with_capacity(requested.len());
    let mut needs_build = Vec::new();
    let mut unchanged = 0usize;
    let mut at_genesis = 0usize;
    let mut at_height = 0usize;
    let known = cx.params.palw_fences_v1();
    for (name, request) in &requested {
        let check = format!("requires.fences.{name}");
        let current = known.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
        let at = match request {
            PalwFenceRequestV1::Height(h) => ForkActivation::new(*h),
            PalwFenceRequestV1::Named(word) if word == "genesis" => ForkActivation::always(),
            PalwFenceRequestV1::Named(word) => {
                return Ok(KindOutcomeV1::at(
                    cx.refuse(check, format!("a candidate names `genesis` or a height, not `{word}`")),
                    cx.depth,
                ));
            }
        };
        let Some(current) = current else {
            let why = format!("this build has no fence `{name}`: the candidate needs a build before it needs a height");
            cx.fail(&check, why.clone());
            needs_build.push(why);
            deltas.push(PalwFenceDeltaV1 { name: name.clone(), requested: request.to_string(), this_build: "absent".to_string() });
            continue;
        };
        deltas.push(PalwFenceDeltaV1 { name: name.clone(), requested: request.to_string(), this_build: fence_value_string(current) });
        match set_fence_by_name(&mut arming, name, at) {
            Ok(()) => {
                if current == Some(at) {
                    unchanged += 1;
                } else if at == ForkActivation::always() {
                    at_genesis += 1;
                } else {
                    at_height += 1;
                }
                cx.pass(&check);
            }
            Err(why) => {
                cx.fail(&check, why.clone());
                needs_build.push(why);
            }
        }
    }
    let arming = with_derived_depths_v1(arming);

    let arm = would_print_v1(&arming);
    cx.record("arming.params_id", &arm.params_id);
    cx.record("arming.identity_id", &arm.identity_id);
    cx.record("arming.schedule_id", &arm.schedule_id);
    cx.record("arming.fence_schedule", format!("{:?}", arm.fence_schedule));
    let identity_moves = arm.identity_id != this_build.identity_id;
    let params_moves = arm.params_id != this_build.params_id;
    let schedule_moves = arm.schedule_id != this_build.schedule_id;
    cx.record("identity_moves", identity_moves);
    cx.record("params_id_moves", params_moves);
    cx.record("schedule_id_moves", schedule_moves);
    let refusal = match arming.validate_palw_v2() {
        Ok(()) => {
            cx.pass("validate_palw_v2");
            None
        }
        Err(e) => {
            let text = format!("{e:?}");
            cx.fail("validate_palw_v2", text.clone());
            Some(text)
        }
    };

    let mut reasons = needs_build.clone();
    if let Some(refused) = &refusal {
        reasons.push(format!("this build refuses the combination (validate_palw_v2): {refused}"));
    }
    let flag_day;
    if identity_moves {
        flag_day = true;
        reasons.push("identity moves: refused at the handshake by every un-upgraded peer".to_string());
    } else if params_moves || schedule_moves {
        flag_day = true;
        reasons.push(
            "the fork-id gate (ADR-0072 SA-2) compares heights the moment the schedule is announced; identity unchanged, params id and schedule id move"
                .to_string(),
        );
    } else if needs_build.is_empty() && refusal.is_none() {
        flag_day = false;
        reasons.push(format!(
            "the candidate names the values this build already carries ({unchanged} unchanged); nothing would print differently"
        ));
    } else {
        flag_day = true;
    }
    if at_genesis > 0 && !identity_moves {
        // Reachable only for a fence that is already at genesis in this build (then unchanged).
        reasons.push(format!("{at_genesis} fence(s) asked at genesis"));
    }
    let _ = at_height;
    cx.record("flag_day", flag_day);
    Ok(KindOutcomeV1::at(
        PalwExtensionClassificationV1::RulesetChange { fences: deltas, would_print: Some(arm), flag_day, reason: reasons.join("; ") },
        cx.depth,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};

    /// Supported fences can be set by name; retired and unimplemented reservations are refused.
    #[test]
    fn supported_fences_can_be_set_and_reserved_fences_are_refused() {
        let params: Params = NetworkId::with_suffix(NetworkType::Testnet, 11).into();
        for (name, _) in params.palw_fences_v1() {
            let mut arming = params.clone();
            if matches!(name, "palw_inactivity_leak" | "palw_dns_retirement_v1" | "palw_permissionless_panel_v1") {
                assert!(set_fence_by_name(&mut arming, name, ForkActivation::always()).is_err());
                let after = arming.palw_fences_v1().into_iter().find(|(n, _)| *n == name).and_then(|(_, v)| v);
                assert!(after.is_none(), "{name}: refusal must leave the reservation unset");
                continue;
            }
            match set_fence_by_name(&mut arming, name, ForkActivation::new(9_000_000)) {
                Ok(()) => {
                    let after = arming.palw_fences_v1().into_iter().find(|(n, _)| *n == name).and_then(|(_, v)| v);
                    assert_eq!(
                        after,
                        Some(ForkActivation::new(9_000_000)),
                        "{name}: setting by name did not land on the fence the list reads"
                    );
                }
                // Reserved names need not have a candidate setter until their implementation lands.
                Err(why) if why.contains("this build has no fence") => {
                    let after = arming.palw_fences_v1().into_iter().find(|(n, _)| *n == name).and_then(|(_, v)| v);
                    assert!(after.is_none(), "{name}: an active fence must have a candidate setter");
                    assert!(set_fence_by_name(&mut arming, name, ForkActivation::always()).is_err());
                }
                // A genesis-only fence is refused at a height by name (ADR-0152 §4-quater).
                Err(why) => assert!(why.contains("companion value") || why.contains("is genesis-only"), "{name}: {why}"),
            }
        }
        assert!(set_fence_by_name(&mut params.clone(), "palw_no_such_fence", ForkActivation::always()).is_err());
    }

    #[test]
    fn inactivity_leak_cannot_be_set_even_when_companion_values_are_present() {
        use kaspa_consensus_core::config::params::PalwInactivityLeakV1;
        let mut params: Params = NetworkId::with_suffix(NetworkType::Testnet, 11).into();
        let reserved = PalwInactivityLeakV1 {
            activation: ForkActivation::never(),
            t_leak_daa: 100,
            reentry_final_depth_daa: 10,
        };
        params.palw_inactivity_leak = Some(reserved);
        for at in [ForkActivation::always(), ForkActivation::new(9_000_000)] {
            assert!(set_fence_by_name(&mut params, "palw_inactivity_leak", at).is_err());
            assert_eq!(params.palw_inactivity_leak, Some(reserved));
        }
    }

    /// **The class-verify-deadline fence is refused at a height and set, with its bundle mirror, at
    /// genesis** (the §4-quater review's L5): it is genesis-only, so a candidate naming a height is a
    /// regenesis and is refused by name before any fingerprint is printed.
    #[test]
    fn the_class_verify_deadline_fence_is_refused_at_a_height_and_mirrored_at_genesis() {
        let params: Params = NetworkId::with_suffix(NetworkType::Testnet, 11).into();
        let name = "palw_class_verify_deadline";
        let at_height = set_fence_by_name(&mut params.clone(), name, ForkActivation::new(9_000_000));
        assert!(at_height.as_ref().is_err_and(|why| why.contains("is genesis-only")), "{at_height:?}");
        let mut armed = params.clone();
        set_fence_by_name(&mut armed, name, ForkActivation::always()).expect("genesis is accepted");
        let after = armed.palw_fences_v1().into_iter().find(|(n, _)| *n == name).and_then(|(_, v)| v);
        assert_eq!(after, Some(ForkActivation::always()), "set by name");
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &armed.palw_consensus_mode else {
            panic!("testnet-11 is ConsensusV2")
        };
        assert_eq!(bundle.state.class_verify_deadline_from_daa(), Some(0), "the bundle mirrors the fence");
    }

    /// **The Activation Pool's fence is refused at a height and set, with its terms, at genesis**
    /// (ADR-0152-adjacent: Activation Pool, user decision 2026-09-25): it is genesis-only, so a
    /// candidate naming a height is a regenesis and is refused by name before any fingerprint is
    /// printed; at genesis a preset without terms takes the user's scale.
    #[test]
    fn the_activation_pool_fence_is_refused_at_a_height_and_set_with_its_terms_at_genesis() {
        let params: Params = NetworkId::with_suffix(NetworkType::Testnet, 11).into();
        let name = "palw_activation_pool";
        let at_height = set_fence_by_name(&mut params.clone(), name, ForkActivation::new(9_000_000));
        assert!(at_height.as_ref().is_err_and(|why| why.contains("is genesis-only")), "{at_height:?}");
        let mut armed = params.clone();
        set_fence_by_name(&mut armed, name, ForkActivation::always()).expect("genesis is accepted");
        let after = armed.palw_fences_v1().into_iter().find(|(n, _)| *n == name).and_then(|(_, v)| v);
        assert_eq!(after, Some(ForkActivation::always()), "set by name");
        assert_eq!(
            armed.palw_activation_pool.map(|pool| pool.terms),
            Some(kaspa_consensus_core::palw_activation_pool_v1::PALW_ACTIVATION_POOL_TERMS_V1),
            "a preset without terms takes the user's scale"
        );
        // testnet-11 arms none of its prerequisites at genesis, so the candidate is a refusal there.
        assert!(armed.validate_palw_v2().is_err_and(|e| e.to_string().contains("palw_activation_pool")));
    }

    /// **The readiness-V2 horizon's fence is refused at a height and set, with its spans and the
    /// bundle's mirror, at genesis** (user decision 2026-09-25, readiness capacity option (a)): it is
    /// genesis-only, so a candidate naming a height is a regenesis and is refused by name before any
    /// fingerprint is printed; at genesis a preset without a horizon takes testnet-12's twenty-four.
    #[test]
    fn the_readiness_horizon_fence_is_refused_at_a_height_and_set_with_its_spans_at_genesis() {
        let params: Params = NetworkId::with_suffix(NetworkType::Testnet, 11).into();
        let name = "palw_readiness_v2_max_age_spans";
        let at_height = set_fence_by_name(&mut params.clone(), name, ForkActivation::new(9_000_000));
        assert!(at_height.as_ref().is_err_and(|why| why.contains("is genesis-only")), "{at_height:?}");
        let mut armed = params.clone();
        set_fence_by_name(&mut armed, name, ForkActivation::always()).expect("genesis is accepted");
        let after = armed.palw_fences_v1().into_iter().find(|(n, _)| *n == name).and_then(|(_, v)| v);
        assert_eq!(after, Some(ForkActivation::always()), "set by name");
        assert_eq!(armed.palw_readiness_v2_max_age_spans.map(|horizon| horizon.max_age_spans), Some(24), "testnet-12's horizon");
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &armed.palw_consensus_mode else {
            panic!("testnet-11 is ConsensusV2")
        };
        assert_eq!(bundle.state.readiness_v2_max_age_spans(), Some(24), "the bundle mirrors the fence");
        // testnet-11 arms readiness V2 at a height, not at genesis, so the candidate is a refusal there.
        assert!(armed.validate_palw_v2().is_err_and(|e| e.to_string().contains("palw_readiness_v2_max_age_spans")));
    }
}
