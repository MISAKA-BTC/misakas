//! Deterministic TIR registration routing under a chain-selected fence.
//! No environment or object-supplied permission selects the wide path. This
//! shares existing tag61, class IDs and signature messages without new wire data.

use crate::{
    Hash64,
    config::params::{ForkActivation, Params},
    palw_class_admission_v2::PalwClassAdmissionError,
    palw_e2e_adjudicability::PalwE2eFamilyV1,
    palw_mode_v2::{PalwClassCatalogEntryV2, PalwConsensusParamsV2},
    palw_state_v2::PalwConsensusObjectV2,
    palw_tir_admission_v1::{PalwTirAdmissionRulesV1, PalwTirClassRecordV1},
    palw_tir_class_record_v2::{ProfiledTirClassRecordV2, TirClassRecordProfileLimitsV2},
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1},
    palw_tir_v1::PalwTirFenceV1,
};
use misaka_palw_tir::{
    DType, TirProgramV1,
    program::{HISTORY_BOUND_V1_HELD, LOGITS_I64_Q24_SCHEME_V2},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirRegistrationDispatchV1 {
    LegacyV10,
    WideV2,
}

// Routing hint only: canonical TIR stores its scheme as the fixed final64 bytes.
// This is NOT a parser or positive authentication. A matching malformed input
// still has to pass every strict wide check; every nonmatch stays with legacy.
fn has_wide_footer(class: &PalwTirClassV1) -> bool {
    class.program.ends_with(&LOGITS_I64_Q24_SCHEME_V2)
}

/// Route all non-wide-footer inputs directly to the old gate/factory without
/// parsing or new refusal ordering. Only a known-wide candidate is decoded and
/// checked against the owner's active fence. This is not signature/admission.
pub fn select_tir_registration_v1(
    class: &PalwTirClassV1,
    selected: &PalwTirFenceV1,
    daa_score: u64,
) -> Result<TirRegistrationDispatchV1, PalwClassAdmissionError> {
    if !has_wide_footer(class) {
        return Ok(TirRegistrationDispatchV1::LegacyV10);
    }
    if selected.activation == ForkActivation::never() || !selected.activation.is_active(daa_score) {
        return Err(PalwClassAdmissionError::TirNeedsItsFence);
    }
    crate::palw_tir_admission_v1::exceeds("IR program bytes", class.program.len() as u64, selected.ceilings.max_program_bytes as u64)?;
    if class.version != PALW_TIR_CLASS_VERSION_V1 || class.layout.version != PALW_TIR_LAYOUT_VERSION_V1 {
        return Err(PalwClassAdmissionError::TirLayout("the class or its layout is not version 1".into()));
    }
    let program = class.decode_program().map_err(crate::palw_tir_admission_v1::tir_program_error)?;
    let logits = &program.blocks[program.schedule.post as usize].nodes[program.logits as usize];
    if program.wide_logits_q24() {
        let prim_set = Hash64::from_bytes(program.prim_set_id);
        if prim_set != selected.prim_set_id {
            return Err(PalwClassAdmissionError::TirPrimSet { program: prim_set, fence: selected.prim_set_id });
        }
        if selected.court_version != 2 || logits.out.dtype != DType::I64 || program.history_bound == HISTORY_BOUND_V1_HELD {
            return Err(PalwClassAdmissionError::TirLayout(
                "wide registration requires court2, final I64 logits and non-held history".into(),
            ));
        }
        return Ok(TirRegistrationDispatchV1::WideV2);
    }
    Err(PalwClassAdmissionError::TirLayout("wide routing footer is not the canonical wide profile".into()))
}

/// Preserve legacy v10 for every supported narrow dtype; only strict wide under
/// the active selected court2 fence reaches V2's complete range/cost gate.
pub fn verify_selected_tir_class_admission_v1(
    bundle: &PalwConsensusParamsV2,
    rules: &PalwTirAdmissionRulesV1,
    object: &PalwConsensusObjectV2,
    certified: &[PalwE2eFamilyV1],
    chain_certified: &[PalwE2eFamilyV1],
    daa_score: u64,
) -> Result<(PalwClassCatalogEntryV2, PalwTirClassRecordV1), PalwClassAdmissionError> {
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = object else {
        return Err(PalwClassAdmissionError::NotARegistration);
    };
    match select_tir_registration_v1(&admission.class, &rules.fence, daa_score)? {
        TirRegistrationDispatchV1::LegacyV10 => {
            crate::palw_tir_admission_v1::verify_class_admission_v10(bundle, rules, object, certified, chain_certified)
        }
        TirRegistrationDispatchV1::WideV2 => {
            if bundle.state.tir_wide_registration_fence_at(daa_score) != Some(rules.fence) {
                return Err(PalwClassAdmissionError::TirLayout("selected wide fence disagrees with the fold runtime mirror".into()));
            }
            crate::palw_tir_admission_v2::verify_tir_class_admission_v2(bundle, rules, object, certified, chain_certified)
        }
    }
}

/// Processor-same profile selection at this DAA. Signature/collateral, target
/// and the processor's exact share policy remain outside this preflight.
pub fn palw_tir_registration_preflight_selected_at_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    object: &PalwConsensusObjectV2,
    daa_score: u64,
    chain_certified: &[PalwE2eFamilyV1],
) -> Result<(PalwClassCatalogEntryV2, PalwTirClassRecordV1), PalwClassAdmissionError> {
    let rules = PalwTirAdmissionRulesV1::at(params, daa_score).ok_or(PalwClassAdmissionError::TirNeedsItsFence)?;
    if matches!(object, PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } if has_wide_footer(&admission.class)) {
        params.validate_palw_tir_v1().map_err(|e| PalwClassAdmissionError::TirLayout(e.to_string()))?;
    }
    let certified = crate::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    verify_selected_tir_class_admission_v1(bundle, &rules, object, &certified, chain_certified, daa_score)
}

/// Fold metadata derivation only, after the native owner has checked admission.
/// This does not rerun range/court sizing or replace any native signature gate.
pub fn palw_tir_class_record_selected_v1(
    class: &PalwTirClassV1,
    artifact_root: &Hash64,
    selected: &PalwTirFenceV1,
    daa_score: u64,
) -> Result<(PalwTirClassRecordV1, TirProgramV1), PalwClassAdmissionError> {
    match select_tir_registration_v1(class, selected, daa_score)? {
        TirRegistrationDispatchV1::LegacyV10 => crate::palw_tir_admission_v1::palw_tir_class_record_v1(class, artifact_root)
            .map_err(crate::palw_tir_admission_v1::tir_program_error),
        TirRegistrationDispatchV1::WideV2 => {
            // Each commit/state needs encoded program fields, so raw byte count
            // is a conservative metadata-cardinality cap. The existing full
            // resolver then checks EXACT commit/state lengths before class-id
            // serialization or the storage program copy; no layout clone here.
            let row = ProfiledTirClassRecordV2::build(
                class,
                *artifact_root,
                TirClassRecordProfileLimitsV2 {
                    max_program_bytes: selected.ceilings.max_program_bytes as usize,
                    max_layout_entries: class.program.len(),
                },
            )
            .map_err(|e| PalwClassAdmissionError::TirLayout(format!("wide record metadata refused: {e:?}")))?
            .into_storage_row();
            let program = class.decode_program().map_err(crate::palw_tir_admission_v1::tir_program_error)?;
            Ok((row, program))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::params::palw_t12_shipped_params, palw_mode_v2::PalwConsensusMode};

    fn class(dtype: DType) -> PalwTirClassV1 {
        let mut class = crate::palw_tir_court_v1::test_support::tiny_execution(None).binding.class;
        let mut p = class.decode_program().unwrap();
        let post = &mut p.blocks[p.schedule.post as usize];
        let mut out = post.nodes[p.logits as usize].out.clone();
        out.dtype = dtype;
        post.nodes.push(misaka_palw_tir::Node {
            prim: misaka_palw_tir::Prim::Cast,
            inputs: vec![misaka_palw_tir::Ref::Node(p.logits)],
            out,
            commit: true,
        });
        p.logits = (post.nodes.len() - 1) as u16;
        if dtype == DType::I64 {
            p.logits_scheme_id = misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2;
        }
        class.program = p.encode();
        class.layout.commit_tiles.push(4);
        class
    }
    fn fence(version: u16) -> PalwTirFenceV1 {
        PalwTirFenceV1 { court_version: version, ..PalwTirFenceV1::testnet12_v1(ForkActivation::new(100)) }
    }

    #[test]
    fn exact_profile_and_selected_fence_route_without_narrow_reinterpretation() {
        for dt in [DType::I8, DType::I16, DType::I32, DType::Idx] {
            for version in [1, 2] {
                assert_eq!(
                    select_tir_registration_v1(&class(dt), &fence(version), 100).unwrap(),
                    TirRegistrationDispatchV1::LegacyV10
                );
            }
        }
        assert_eq!(select_tir_registration_v1(&class(DType::I64), &fence(2), 100).unwrap(), TirRegistrationDispatchV1::WideV2);
        assert!(select_tir_registration_v1(&class(DType::I64), &fence(1), 100).is_err());
        assert!(select_tir_registration_v1(&class(DType::I64), &fence(2), 99).is_err());
        assert_eq!(select_tir_registration_v1(&class(DType::I32), &fence(3), 100).unwrap(), TirRegistrationDispatchV1::LegacyV10);
        assert!(select_tir_registration_v1(&class(DType::I64), &fence(3), 100).is_err());
        let mut unknown = class(DType::I32);
        let mut p = unknown.decode_program().unwrap();
        p.logits_scheme_id = [0x55; 64];
        unknown.program = p.encode();
        assert_eq!(select_tir_registration_v1(&unknown, &fence(2), 100).unwrap(), TirRegistrationDispatchV1::LegacyV10);
        let mut malformed = class(DType::I32);
        malformed.program = vec![0xff];
        let mut inactive = fence(3);
        inactive.activation = ForkActivation::never();
        inactive.ceilings.max_program_bytes = 0;
        assert_eq!(select_tir_registration_v1(&malformed, &inactive, 0).unwrap(), TirRegistrationDispatchV1::LegacyV10);
    }

    #[test]
    fn legacy_errors_and_missing_runtime_mirror_keep_old_preflight_order() {
        let mut params = palw_t12_shipped_params();
        let at = params.palw_tir_v1.unwrap().activation.daa_score();
        let PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else { unreachable!() };
        bundle.state = bundle.state.clone().with_tir_from_daa(None);
        let bundle = bundle.clone();
        assert!(params.validate_palw_tir_v1().is_err());
        for invalid_encoding in [false, true] {
            let mut class = class(DType::I32);
            if invalid_encoding {
                class.program = vec![0xff];
            } else {
                let mut program = class.decode_program().unwrap();
                program.logits_scheme_id = [0x55; 64];
                class.program = program.encode();
            }
            let fixture = crate::palw_tir_court_v1::test_support::tiny_execution(None);
            let object = PalwConsensusObjectV2::ClassRegisteredTirV1 {
                class_id: class.class_id(&fixture.binding.artifact_root),
                artifact_root: fixture.binding.artifact_root,
                slash_value_per_pwu: 1,
                pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 },
                initial_target: 1,
                share_permille: 0,
                activation_daa: at,
                admission: Box::new(crate::palw_tir_class_v1::PalwTirAdmissionCarriageV1 {
                    class,
                    canonical: fixture.binding.job_context,
                    registrant_bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint {
                        transaction_id: crate::tx::TransactionId::from_bytes([9; 64]),
                        index: 0,
                    }),
                    signature: vec![],
                }),
            };
            let old =
                crate::palw_tir_admission_v1::palw_tir_registration_preflight_at_v1(&params, &bundle, &object, at, &[]).unwrap_err();
            let selected = palw_tir_registration_preflight_selected_at_v1(&params, &bundle, &object, at, &[]).unwrap_err();
            assert_eq!(selected, old);
            if invalid_encoding {
                assert!(matches!(selected, PalwClassAdmissionError::TirProgram(_)));
            } else {
                assert!(matches!(selected, PalwClassAdmissionError::TirLayout(message) if message.contains("scheme")));
            }
        }
    }

    #[test]
    fn full_runtime_mirror_is_skipped_and_old_height_setter_clears_wide() {
        let params = palw_t12_shipped_params();
        let PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode else { unreachable!() };
        let old = bundle.state.with_tir_from_daa(Some(100));
        let bytes = borsh::to_vec(&old).unwrap();
        let wide = old.clone().with_tir_registration_fence(Some(fence(2)));
        assert_eq!(borsh::to_vec(&wide).unwrap(), bytes);
        let restored: crate::palw_state_v2::PalwStateParamsV2 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(restored.tir_wide_registration_fence(), None);
        assert_eq!(restored.tir_from_daa(), None);
        assert_eq!(wide.tir_wide_registration_fence_at(99), None);
        assert_eq!(wide.tir_wide_registration_fence_at(100), Some(fence(2)));
        assert_eq!(wide.clone().with_tir_from_daa(Some(100)).tir_wide_registration_fence(), None);
        assert_eq!(wide.clone().with_tir_registration_fence(Some(fence(1))).tir_wide_registration_fence(), None);
        assert_eq!(wide.with_tir_registration_fence(None).tir_from_daa(), None);
    }

    #[test]
    fn params_reject_every_full_mirror_substitution() {
        let mut params = palw_t12_shipped_params();
        let mut selected = params.palw_tir_v1.expect("shipped T12 has its IR flag day");
        selected.court_version = 2;
        params.palw_tir_v1 = Some(selected);
        params.sync_palw_tir_v1();
        assert!(params.validate_palw_tir_v1().is_ok());
        // The full mirror is restored from trusted Params after Borsh loading;
        // validating a missing runtime mirror must fail until sync runs.
        let mut restored = params.clone();
        let PalwConsensusMode::ConsensusV2(bundle) = &mut restored.palw_consensus_mode else { unreachable!() };
        bundle.state = borsh::from_slice(&borsh::to_vec(&bundle.state).unwrap()).unwrap();
        assert!(restored.validate_palw_tir_v1().is_err());
        restored.sync_palw_tir_v1();
        assert!(restored.validate_palw_tir_v1().is_ok());
        for mutation in 0..4 {
            let mut changed = params.clone();
            let mut other = selected;
            match mutation {
                0 => other.activation = ForkActivation::new(selected.activation.daa_score() + 1),
                1 => other.prim_set_id = Hash64::from_u64_word(9),
                2 => other.ceilings.max_program_bytes -= 1,
                _ => other.court_version = 1,
            }
            let PalwConsensusMode::ConsensusV2(bundle) = &mut changed.palw_consensus_mode else { unreachable!() };
            bundle.state = bundle.state.clone().with_tir_registration_fence(Some(other));
            assert!(changed.validate_palw_tir_v1().is_err());
        }
    }
}
