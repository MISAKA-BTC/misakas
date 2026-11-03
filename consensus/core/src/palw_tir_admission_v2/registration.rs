//! Explicit wide-only registration construction and local preflight.
//!
//! The existing tag-61 carrier, class identity and signature message are retained.
//! These functions do not arm a fence, install a row, verify a registrant signature
//! or grant native execution, issuance or credit authority.

use crate::{
    Hash64,
    palw_class_admission_v2::PalwClassAdmissionError,
    palw_mode_v2::{PalwClassCatalogEntryV2, PalwConsensusParamsV2},
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwPwuRuleV2},
    palw_tir_admission_v1::{PalwTirAdmissionRulesV1, PalwTirClassRecordV1},
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirAdmissionCarriageV1, PalwTirClassV1},
    palw_tir_profile_resolver_v2::{ResolvedTirJobProfileV2, TirProfileResolveErrorV2, TirTraceProfileV2},
    palw_tir_step_v1::PalwTirStepSpaceV2,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::program::{HISTORY_BOUND_V1_HELD, MAX_PROGRAM_BYTES};

/// Resolve the original strict-wide profile before any layout clone or class-id
/// serialization. This is bounded metadata validation, not full admission or a
/// native heap/work reservation. Held, narrow and unknown profiles stay refused.
pub(super) fn resolve_wide_profile(
    class: &PalwTirClassV1,
    artifact: Hash64,
    max_program_bytes: usize,
) -> Result<ResolvedTirJobProfileV2, PalwClassAdmissionError> {
    let cap = max_program_bytes.min(MAX_PROGRAM_BYTES);
    if cap == 0 || class.program.len() > cap {
        return Err(PalwClassAdmissionError::CourtCostExceedsCeiling {
            what: "IR program bytes",
            got: class.program.len() as u64,
            ceiling: cap as u64,
        });
    }
    if class.version != PALW_TIR_CLASS_VERSION_V1 || class.layout.version != PALW_TIR_LAYOUT_VERSION_V1 {
        return Err(PalwClassAdmissionError::TirLayout("the class or its layout is not version 1".into()));
    }
    let program = class.decode_program().map_err(|e| PalwClassAdmissionError::TirProgram(e.to_string()))?;
    if program.history_bound == HISTORY_BOUND_V1_HELD {
        return Err(PalwClassAdmissionError::TirLayout("wide TIR admission does not support the held history regime".into()));
    }
    if !program.wide_logits_q24() {
        return Err(PalwClassAdmissionError::TirLayout("explicit V2 registration requires the exact wide Q24 profile".into()));
    }
    let committed = program
        .blocks
        .iter()
        .flat_map(|block| &block.nodes)
        .filter(|node| node.commit)
        .try_fold(0usize, |n, _| n.checked_add(1))
        .ok_or_else(|| PalwClassAdmissionError::TirLayout("committed-node count overflows".into()))?;
    if class.layout.commit_tiles.len() != committed || class.layout.state_tiles.len() != program.states.len() {
        return Err(PalwClassAdmissionError::TirLayout(
            "the original layout does not have one tile entry per commit and state".into(),
        ));
    }
    // The full resolver decodes again; do not retain both decoded programs.
    drop(program);
    let profile = ResolvedTirJobProfileV2::new(class, artifact, cap).map_err(|e| match e {
        TirProfileResolveErrorV2::Program(e) => PalwClassAdmissionError::TirProgram(e.to_string()),
        other => PalwClassAdmissionError::TirLayout(format!("wide profile resolution refused: {other:?}")),
    })?;
    if profile.profile() != TirTraceProfileV2::TiledI64Q24 {
        return Err(PalwClassAdmissionError::TirLayout("explicit V2 registration requires the exact wide Q24 profile".into()));
    }
    Ok(profile)
}

/// Construct the existing tag-61 carrier using the original wide class and its
/// complete canonical yardstick context. PWU is the canonical leaf count, with
/// no byte-width multiplier. The caller supplies the signature unchanged: as in
/// V1, construct with an empty signature to obtain the existing signing message,
/// then construct with that signature. Construction is not admission or signature
/// verification; the caller must reserve bounded metadata/geometry storage first.
#[allow(clippy::too_many_arguments)]
pub fn palw_tir_post_genesis_registration_v2(
    class: PalwTirClassV1,
    canonical: PalwJobContextV2,
    artifact_root: Hash64,
    share_permille: u16,
    initial_target: u128,
    slash_value_per_pwu: u64,
    activation_daa: u64,
    registrant_bond: PalwBondKeyV2,
    signature: Vec<u8>,
    ladder: u64,
) -> Result<PalwConsensusObjectV2, PalwClassAdmissionError> {
    let profile = resolve_wide_profile(&class, artifact_root, MAX_PROGRAM_BYTES)?;
    let expected = profile.ctx().ok_or_else(|| {
        PalwClassAdmissionError::TirCanonicalNotTheFormula("the original wide class has no canonical attempt yardstick".into())
    })?;
    if canonical != expected {
        return Err(PalwClassAdmissionError::TirCanonicalNotTheFormula(
            "the complete canonical context is not the original wide profile's attempt yardstick".into(),
        ));
    }
    let space = PalwTirStepSpaceV2::new(&class).map_err(|e| PalwClassAdmissionError::TirLayout(e.to_string()))?;
    let counted = space.leaf_count_capped(&canonical, ladder).map_err(|e| PalwClassAdmissionError::TirLayout(e.to_string()))?;
    Ok(PalwConsensusObjectV2::ClassRegisteredTirV1 {
        class_id: profile.class_id(),
        artifact_root,
        slash_value_per_pwu,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target,
        share_permille,
        activation_daa,
        admission: Box::new(PalwTirAdmissionCarriageV1 { class, canonical, registrant_bond, signature }),
    })
}

/// Explicit local wide-profile preflight under the existing rules at this DAA.
/// Resolving an existing fence does not arm wide native admission or change the
/// Params validator/dispatch. Like V1 preflight, this excludes registrant signature
/// and collateral, the chain's target, and the processor's share-policy check.
pub fn palw_tir_registration_preflight_at_v2(
    params: &crate::config::params::Params,
    bundle: &PalwConsensusParamsV2,
    object: &PalwConsensusObjectV2,
    daa_score: u64,
    chain_certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
) -> Result<(PalwClassCatalogEntryV2, PalwTirClassRecordV1), PalwClassAdmissionError> {
    let rules = PalwTirAdmissionRulesV1::at(params, daa_score).ok_or(PalwClassAdmissionError::TirNeedsItsFence)?;
    let certified = crate::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    super::verify_tir_class_admission_v2(bundle, &rules, object, &certified, chain_certified)
}
