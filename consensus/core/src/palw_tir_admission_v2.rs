//! Explicit WIDE-ONLY, TIR-only local registration admissibility.
//!
//! The old v10 verifier/builders/factories and all native dispatch remain closed
//! to wide programs. This additive API returns DATA under supplied rules, not a
//! rooted registration, signature check, native resource lease or activation.
//! Exact RFC-0043 wide semantics and court version 2 are required here, while the
//! existing Params validator and public/private arming are unchanged.
//!
//! Original program/layout/context are never rewritten. All old structural,
//! universal range, replay, F7, canonical, PWU, identity and share gates remain.
//! Ordinary Multiproof TIR is supported; held/GEN/composite pricing is not.
//!
//! Caller owners reserve original input storage, repeated bounded decode/range
//! and per-tile/cone analysis, layout/geometry, full-traversal sizing, read maps,
//! inventory metadata, hashes and row copies BEFORE calling. Raw format/network
//! caps and sizing-work counters are not a native RAM/work reservation or a fee.

use crate::{
    Hash64,
    palw_class_admission_v2::{PalwClassAdmissionError, PalwCourtCostV1},
    palw_mode_v2::{PalwClassCatalogEntryV2, PalwConsensusParamsV2, PalwCourtParamsV2},
    palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2},
    palw_tir_admission_v1::{
        PALW_TIR_CLOSE_FRAME_BYTES_V1, PALW_TIR_DISSECT_CARRIER_BYTES_V1, PALW_TIR_MAX_DISTINCT_TILES_V1, PalwTirAdmissionRulesV1,
        PalwTirClassRecordV1, exceeds, palw_tir_carriable_close_bytes_v1, palw_tir_dissected_cone_admits_v1,
        palw_tir_reachable_prims_v1, tir_program_error,
    },
    palw_tir_attempt_v1::PalwTirJobFactsV1,
    palw_tir_class_record_v2::{ProfiledTirClassRecordV2, TirClassRecordProfileLimitsV2},
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1},
    palw_tir_close_range_v1::PalwTirCloseTwinV1,
    palw_tir_close_size_v1::{PALW_TIR_CLOSE_SIZING_OVER_CAP_V1, PALW_TIR_CLOSE_SIZING_WORK_CAP_V1, PalwTirParamFormV1},
    palw_tir_close_size_v2::{
        PalwTirCloseBoundV2, PalwTirCloseSizingV2, palw_tir_worst_closes_range_work_v2, palw_tir_worst_closes_work_v2,
    },
    palw_tir_step_v1::PalwTirStepSpaceV2,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::admit::{TirAdmissionV1, TirAdmitError, TirAdmitInputsV1, TirCeilingsV1};
use std::collections::BTreeSet;
mod registration;
pub use registration::{palw_tir_post_genesis_registration_v2, palw_tir_registration_preflight_at_v2};

/// The additive pure verifier for the original exact-wide tag-61 carrier.
/// Nonzero share uses the existing certified-family rule; no model review,
/// coefficient change or promotion is added. Processor signature/collateral,
/// target, share policy and every native gate are outside this local result.
pub fn verify_tir_class_admission_v2(
    bundle: &PalwConsensusParamsV2,
    rules: &PalwTirAdmissionRulesV1,
    registration: &PalwConsensusObjectV2,
    certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
    chain_certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
) -> Result<(PalwClassCatalogEntryV2, PalwTirClassRecordV1), PalwClassAdmissionError> {
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, artifact_root, pwu_rule, share_permille, admission, .. } =
        registration
    else {
        return Err(PalwClassAdmissionError::NotARegistration);
    };
    let class = &admission.class;
    let canonical = &admission.canonical;
    let ceilings = &rules.fence.ceilings;
    // RFC-0043's adopted semantics selector, for this EXPLICIT local API only.
    // No Params validator, fence schedule or native dispatch is armed here.
    if rules.fence.court_version != 2 {
        return Err(PalwClassAdmissionError::TirLayout("explicit wide admission requires court semantics version 2".into()));
    }

    // 1. Versions, bytes, decoding.
    if class.version != PALW_TIR_CLASS_VERSION_V1 || class.layout.version != PALW_TIR_LAYOUT_VERSION_V1 {
        return Err(PalwClassAdmissionError::TirLayout("the class or its layout is not version 1".into()));
    }
    exceeds("IR program bytes", class.program.len() as u64, ceilings.max_program_bytes as u64)?;
    let profile = registration::resolve_wide_profile(class, *artifact_root, ceilings.max_program_bytes as usize)?;
    let program = class.decode_program().map_err(tir_program_error)?;

    let mut model_rules = *rules;
    if let Some(k) = rules.court {
        // The window the rules hand in IS the network window here (`PalwTirAdmissionRulesV1::at` writes the bundle's):
        // with the model window inactive the derivation returns it untouched, so a rule set that carries another window
        // (the dissection tests' bisection of it) is judged at that one, as it was before the fence existed.
        let network_window = k.window_court_daa;
        let derived = crate::palw_court_v2::palw_court_params_held_at_v2(bundle, true, rules.held.armed)
            .map_err(|e| PalwClassAdmissionError::Profile(format!("the IR model court has no shape: {e}")))?;
        let window = crate::palw_class_admission_v2::palw_court_window_for_history_v1(
            network_window,
            rules.model_court_window_active,
            rules.held.armed,
            &derived,
            class.layout.max_context as u64,
            class.layout.h_tile.max(1),
        )
        .map_err(|why| PalwClassAdmissionError::Profile(format!("the IR model court window is not finite: {why}")))?;
        model_rules.court = Some(crate::palw_class_admission_v2::PalwKaryCourtV1 { window_court_daa: window, ..k });
    }

    // 2. The primitive set, the scheme, the token bound, the history bound.
    let prim_set = Hash64::from_bytes(program.prim_set_id);
    if prim_set != rules.fence.prim_set_id {
        return Err(PalwClassAdmissionError::TirPrimSet { program: prim_set, fence: rules.fence.prim_set_id });
    }
    // The explicit resolver has already pinned the original exact wide scheme
    // and final-post I64 invariant; it never reinterprets the old tiled bit.
    let post = &program.blocks[program.schedule.post as usize];
    let logits_vocab = post.nodes[program.logits as usize].out.elements_at(1);
    if logits_vocab == 0 || logits_vocab > program.token_bound as u64 {
        return Err(PalwClassAdmissionError::TirLayout(format!(
            "the logits row is {logits_vocab} wide and the token bound is {}: a selected token must be an input the program \
             accepts",
            program.token_bound
        )));
    }
    if program.history_bound == misaka_palw_tir::program::HISTORY_BOUND_V1_HELD {
        return Err(PalwClassAdmissionError::TirLayout(
            "the explicit wide TIR admission does not support the held history regime".into(),
        ));
    }

    // 3. The layout and the unrolled size.
    let layout = &class.layout;
    if layout.max_context > program.history_bound || layout.max_context > ceilings.max_context {
        return Err(PalwClassAdmissionError::TirLayout(format!(
            "max_context {} exceeds the program's history bound {} or the network's {}",
            layout.max_context, program.history_bound, ceilings.max_context
        )));
    }
    let space = PalwTirStepSpaceV2::new(class).map_err(|e| PalwClassAdmissionError::TirLayout(e.to_string()))?;
    let unrolled: u64 = program.occurrences().iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u64).sum();
    exceeds("IR unrolled nodes", unrolled, ceilings.max_unrolled_nodes as u64)?;

    // 4. `tir_admit_v1`, once per distinct commit tile length, at the fence's ceilings; the court's
    //    own tile bounds are applied per commit point below, at the point's own length.
    let tile_lens: BTreeSet<u32> = layout.commit_tiles.iter().copied().collect();
    if tile_lens.len() > PALW_TIR_MAX_DISTINCT_TILES_V1 {
        return Err(PalwClassAdmissionError::TirLayout(format!(
            "{} distinct commit tile lengths; at most {PALW_TIR_MAX_DISTINCT_TILES_V1}",
            tile_lens.len()
        )));
    }
    let admit_ceilings = TirCeilingsV1 {
        max_tile_macs: u64::MAX,
        max_tile_transcendentals: u64::MAX,
        max_tile_opened_bytes: u64::MAX,
        max_tile_operands: u64::MAX,
        max_position_macs: ceilings.max_macs_per_position,
        max_position_transcendentals: u64::MAX,
        max_state_bytes: ceilings.max_state_bytes,
        max_step_leaves: u64::MAX,
        max_checkpoint_interval: layout.checkpoint_interval.max(1),
        max_cone_work: ceilings.max_cone_work,
    };
    let mut runs: Vec<(u32, TirAdmissionV1)> = Vec::with_capacity(tile_lens.len());
    for &tile_len in &tile_lens {
        let inputs = TirAdmitInputsV1 { tile_len, h_chunk: layout.h_tile, ceilings: admit_ceilings };
        let admitted =
            misaka_palw_tir::admit::tir_admit_with_rules_v1(&class.program, &inputs, rules.demand).map_err(|e| match e {
                TirAdmitError::Program(e) => tir_program_error(e),
                TirAdmitError::Exceeds { limit, at, value, cap } => PalwClassAdmissionError::TirExceeds { limit, at, value, cap },
                TirAdmitError::Inputs(why) => PalwClassAdmissionError::TirLayout(why.into()),
            })?;
        runs.push((tile_len, admitted));
    }
    let first = &runs.first().expect("a program has a logits commit point, so a tile length").1;
    exceeds("IR peak live bytes", first.position.peak_live_bytes, ceilings.max_peak_live_bytes)?;

    // 5. Every commit point's cone at its own tile length, against the court.
    let limits = crate::palw_court_v2::palw_tir_court_limits_v1(&bundle.court);
    let work_limit = limits.max_elements.min(limits.max_terms);
    let checkpoint = layout.checkpoint_interval as u64;
    let mut commit_tiles = layout.commit_tiles.iter();
    let (mut worst_close, mut worst_macs, mut worst_operands) = (0u64, 0u64, 0u64);
    for (bi, block) in program.blocks.iter().enumerate() {
        for (ni, node) in block.nodes.iter().enumerate() {
            if !node.commit {
                continue;
            }
            let tile_len = *commit_tiles.next().expect("the step space checked one tile per committed node");
            let run = &runs.iter().find(|(t, _)| *t == tile_len).expect("a run per distinct length").1;
            let cone = run
                .cones
                .iter()
                .find(|c| c.block as usize == bi && c.node as usize == ni)
                .expect("tir_admit_v1 costs every commit point");
            // **A cone that reduces over the history is DISSECTED under the k-ary court** (spec 04b
            // §9.5, RFC-0002 F7): its terminal is one `h_tile` chunk (`cone.terminal()`), and what
            // admission owes the court is that the exchange can be played — the obligations of
            // §9.5.6, a round and a root claim that each fit one carrier, the whole dispute inside
            // `window_court`. Without the court nothing dissects, so such a cone is adjudicated whole
            // and its tile at `H = W` must fit like any other.
            let dissected = !cone.h_reductions.is_empty() && model_rules.court.is_some();
            if !cone.h_reductions.is_empty() {
                match model_rules.court {
                    None => {
                        let whole = cone.tile.macs.saturating_add(cone.tile.elementwise).saturating_add(cone.tile.transcendentals);
                        if cone.tile.macs > bundle.court.max_terminal_macs() || whole > work_limit {
                            return Err(PalwClassAdmissionError::TirNeedsDissection { block: bi as u8, node: ni as u16 });
                        }
                    }
                    Some(k) => palw_tir_dissected_cone_admits_v1(
                        bundle,
                        &model_rules,
                        class,
                        &program,
                        block,
                        bi as u8,
                        ni as u16,
                        tile_len,
                        k,
                    )?,
                }
            }
            let (tile, opened) =
                if dissected { (cone.terminal(), cone.terminal_opened_bytes()) } else { (&cone.tile, cone.tile_opened_bytes) };
            exceeds("IR tile multiply-accumulates", tile.macs, bundle.court.max_terminal_macs())?;
            let mut work = tile.macs.saturating_add(tile.elementwise).saturating_add(tile.transcendentals);
            for leaf in &cone.leaves {
                if let misaka_palw_tir::admit::LeafV1::State(j) = leaf
                    && let Some(state) = run.states.iter().find(|s| s.state == *j)
                {
                    let per = state.per_position;
                    let per = per.macs.saturating_add(per.elementwise).saturating_add(per.transcendentals);
                    work = work.saturating_add(checkpoint.saturating_sub(1).saturating_mul(per));
                }
            }
            exceeds("IR cone evaluation work (tile and state replay)", work, work_limit)?;
            let close = coarse_close_bytes_v2(class.program.len() as u64, opened)?;
            exceeds("IR close bytes", close, bundle.court.max_close_bytes())?;
            worst_close = worst_close.max(close);
            worst_macs = worst_macs.max(tile.macs);
            worst_operands = worst_operands.max(cone.operands);
        }
    }

    // The returned wide catalog entry must also fit the existing operand-count
    // ceiling checked by catalog validation. This changes no V1 derivation.
    exceeds("operand count", worst_operands, bundle.court.max_operand_count() as u64)?;

    // 6. The canonical job, the ladder, the counted pwu.
    let derived_id = profile.class_id();
    let facts = PalwTirJobFactsV1::of(class, &program, derived_id);
    let formula = profile.canonical().ok_or_else(|| {
        PalwClassAdmissionError::TirCanonicalNotTheFormula(format!(
            "max_context {} is too narrow for the attempt formula (f − 1, 2), f = max_context / 8 ≥ 2",
            layout.max_context
        ))
    })?;
    let expected = profile.ctx().ok_or_else(|| {
        PalwClassAdmissionError::TirCanonicalNotTheFormula("the original wide profile has no canonical yardstick".into())
    })?;
    if *canonical != expected {
        return Err(PalwClassAdmissionError::TirCanonicalNotTheFormula(format!(
            "the canonical job must be the original wide attempt formula's yardstick context at ({}, {})",
            formula.0, formula.1
        )));
    }
    // **Every IR claim's prompt is attributable.** The identity rule recomputes the anchor's prompt
    // root inline (J5b) up to 4,096 ids, and no IR route opens a longer prompt tile by tile (the
    // legacy `PromptNotAnchored` carries a legacy binding): so an IR class's canonical prompt is at
    // most the inline bound — `max_context` ≤ 32,783 — until such a route exists.
    if formula.0 > crate::palw_attempt_rules_v1::PALW_J5_INLINE_PROMPT_IDS_V1 {
        return Err(PalwClassAdmissionError::TirCanonicalNotTheFormula(format!(
            "a {}-id canonical prompt is past J5b's inline bound of {} ids, and no IR route attributes a longer one \
             (max_context at most 32,783)",
            formula.0,
            crate::palw_attempt_rules_v1::PALW_J5_INLINE_PROMPT_IDS_V1
        )));
    }
    let _ = rules.prompt_ids_form;
    let ladder = bundle.court.max_step_leaf_count();
    let deepest = PalwJobContextV2 {
        declared_prefill_tokens: 1,
        exact_decode_tokens: layout.max_context,
        max_context_tokens: u32::MAX,
        ..expected.clone()
    };
    let worst = space
        .leaf_count_capped(&deepest, ladder)
        .map_err(|_| PalwClassAdmissionError::DeeperThanTheLadder { worst: u64::MAX, ladder })?;
    let counted = space.leaf_count_capped(canonical, ladder).map_err(|e| PalwClassAdmissionError::TirLayout(e.to_string()))?;
    if counted > worst {
        return Err(PalwClassAdmissionError::CanonicalDeeperThanWorstCase { canonical: counted, worst });
    }
    // **Past the second IR fence: one seat reaches every leaf of every attempt.** A seat's four
    // data-availability sessions descend ten levels each and then open the leaf; an attempt committing
    // more leaves than that could withhold one no single seat can demand.
    let reach = crate::palw_tir_court_v1::PALW_TIR_DA_SEAT_REACH_LEAVES_V1;
    if crate::palw_tir_fence2_v1::palw_tir_fence2_in_force_v1(rules.demand) && counted > reach {
        return Err(PalwClassAdmissionError::TirExceeds {
            limit: "IR DA seat reach",
            at: "the canonical job".into(),
            value: counted,
            cap: reach,
        });
    }
    match pwu_rule {
        PalwPwuRuleV2::MaxPerAttempt(_) => return Err(PalwClassAdmissionError::ClassIsNotDerived),
        PalwPwuRuleV2::DerivedV1 { pwu_per_inference } if *pwu_per_inference != counted => {
            return Err(PalwClassAdmissionError::PwuPerInferenceMismatch { declared: *pwu_per_inference, counted });
        }
        PalwPwuRuleV2::DerivedV1 { .. } => {}
    }

    // 7. The id.
    if *class_id != derived_id {
        return Err(PalwClassAdmissionError::TirClassIdIsNotDerived { declared: *class_id, derived: derived_id });
    }

    // 8. Weight.
    let reachable = palw_tir_reachable_prims_v1(&program);
    if *share_permille > 0 {
        let covered = crate::palw_e2e_adjudicability::family_certified_for_weight_v2(
            bundle.court_e2e_root,
            certified,
            chain_certified,
            &reachable,
        )
        .map_err(|e| PalwClassAdmissionError::Profile(e.to_string()))?;
        if covered.is_none() {
            return Err(PalwClassAdmissionError::NotEndToEndCertified { share: *share_permille });
        }
    }

    // 9. **Every terminal close an executor can be clocked for is CARRIABLE** (decision (1) of
    // 2026-09-28; spec 04b §10.3, PALW-TIR-38). An IR class with a dissected point clocks its
    // executor at every terminal leaf (ADR-0082 C-5), so its acquitting close there — a whole tile's,
    // a dissected cone's bottom — must be one the chain can carry: at most the chunks the fold
    // assembles, of one carrier each, the program referenced (never carried); and a dissected
    // cone's root claim rides one carrier. Quoted as a conservative all-step-8B bound: the units
    // the court's own evaluation reads, over every job of the layout's longest, the parameters in
    // the multiproof the close carries. The ruleset's close ceiling above may be wider than that;
    // both byte ceilings apply to the envelope of this bound and the old coarse measure.
    // Last, as the costliest check (at most the sizing's work cap).
    let sizing = PalwTirCloseSizingV2 {
        // Ordinary TIR only. An opaque artifact root does not identify a
        // composite inventory, and no Composite/GEN pricing path is exposed.
        form: PalwTirParamFormV1::Multiproof,
        court: model_rules.court.is_some(),
        cap: PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
        // Full traversal: a successful stopped prefix is not all-point admission.
        stop_above: None,
    };
    let sized = match crate::palw_tir_fence2_v1::palw_tir_close_twin_v1(rules.demand) {
        PalwTirCloseTwinV1::Element => palw_tir_worst_closes_work_v2(&space, &deepest, &sizing),
        PalwTirCloseTwinV1::Range => palw_tir_worst_closes_range_work_v2(&space, &deepest, &sizing),
    }
    .map_err(sizing_error_v2)?;
    let quoted_close = close_budget_v2(worst_close, sized.bounds(), &bundle.court)?;

    let entry = PalwClassCatalogEntryV2 {
        class_id: derived_id,
        artifact_root: *artifact_root,
        max_step_leaf_count: worst,
        canonical_step_leaf_count: counted,
        reachable_kernels: reachable,
        court_cost: PalwCourtCostV1 {
            max_close_bytes: quoted_close,
            max_terminal_macs: worst_macs,
            max_operand_count: u32::try_from(worst_operands).unwrap_or(u32::MAX),
        },
    };
    // Original full-class schema1 derivation, returned as data, never installed.
    let max_layout_entries = layout
        .commit_tiles
        .len()
        .checked_add(layout.state_tiles.len())
        .ok_or_else(|| PalwClassAdmissionError::TirLayout("wide record layout cardinality overflows".into()))?;
    let record = ProfiledTirClassRecordV2::build(
        class,
        *artifact_root,
        TirClassRecordProfileLimitsV2 {
            max_program_bytes: ceilings.max_program_bytes as usize,
            // Exact already-validated geometry, not a newly chosen network ceiling.
            max_layout_entries,
        },
    )
    .map_err(|e| PalwClassAdmissionError::TirLayout(format!("wide class record derivation refused: {e:?}")))?
    .into_storage_row();
    debug_assert_eq!(record.facts, facts);
    Ok((entry, record))
}

/// Retain the old step5 coarse condition using checked arithmetic. It does not
/// claim to be a serialized measurement: the original program is referenced on
/// wire and paths/whole units/output bytes require the separate typed sizing.
fn coarse_close_bytes_v2(program_bytes: u64, opened: u64) -> Result<u64, PalwClassAdmissionError> {
    program_bytes
        .checked_add(PALW_TIR_CLOSE_FRAME_BYTES_V1)
        .and_then(|n| n.checked_add(opened))
        .ok_or_else(|| PalwClassAdmissionError::TirLayout("wide coarse close byte addition overflows".into()))
}

/// Only this verifier's complete typed traversal reaches this private helper.
/// M is a deliberately conservative envelope, not an exact actual-wire quote.
/// The root move has its own signed DTO and carrier cap, kept independent of M.
fn close_budget_v2(
    old_coarse: u64,
    bounds: &[PalwTirCloseBoundV2],
    court: &PalwCourtParamsV2,
) -> Result<u64, PalwClassAdmissionError> {
    let serialized = bounds
        .iter()
        .map(|b| b.close_bytes)
        .max()
        .ok_or_else(|| PalwClassAdmissionError::TirLayout("wide close sizing returned no terminal bounds".into()))?;
    for b in bounds {
        if b.dissected {
            exceeds("IR dissection root claim bytes", b.root_claim_bytes, PALW_TIR_DISSECT_CARRIER_BYTES_V1)?;
        }
    }
    let combined = old_coarse.max(serialized);
    exceeds("IR close bytes", combined, court.max_close_bytes())?;
    exceeds("IR terminal close bytes as carried", combined, palw_tir_carriable_close_bytes_v1(court))?;
    // Preserve the existing catalog's byte AND legacy chunk policy. Do not add
    // a new 1.2 factor to serialized bytes or divide away framing before gating.
    if !crate::palw_mode_v2::palw_close_fits_court_v1(combined, court) {
        return Err(PalwClassAdmissionError::CourtCostExceedsCeiling {
            what: "court close chunks",
            got: crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(combined),
            ceiling: court.max_close_chunks(),
        });
    }
    Ok(combined)
}

fn sizing_error_v2(error: String) -> PalwClassAdmissionError {
    if error == PALW_TIR_CLOSE_SIZING_OVER_CAP_V1 {
        PalwClassAdmissionError::TirExceeds {
            limit: "IR close sizing work",
            at: "the class's terminal closes".into(),
            value: PALW_TIR_CLOSE_SIZING_WORK_CAP_V1.saturating_add(1),
            cap: PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
        }
    } else {
        PalwClassAdmissionError::TirLayout(format!("the explicit wide close sizing refuses the class: {error}"))
    }
}

#[cfg(test)]
mod tests;
