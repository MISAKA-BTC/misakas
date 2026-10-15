//! Local pruning-sidecar material validation; no state/root/codec changes.
//! Class ids authenticate profiles, not canonical jobs. Retained PWU, fused
//! geometry and semantic identities must also agree before indexing a declaration.
use crate::{
    Hash64,
    palw_class_semantics_v1::PalwClassSemanticsV1,
    palw_state_v2::{PalwClassAdmissionCarriageV2, PalwClassStateV2, PalwPwuRuleV2},
};

pub fn validate_class_declaration_v1(
    class_id: Hash64,
    class: &PalwClassStateV2,
    semantics: Option<&PalwClassSemanticsV1>,
    bytes: &[u8],
) -> Result<PalwClassAdmissionCarriageV2, String> {
    let declaration: PalwClassAdmissionCarriageV2 =
        borsh::from_slice(bytes).map_err(|e| format!("class {class_id} declaration does not decode: {e}"))?;
    validate_class_context_v1(class_id, class, semantics, &declaration.profile, &declaration.canonical)?;
    Ok(declaration)
}

pub fn validate_class_context_v1(
    class_id: Hash64,
    class: &PalwClassStateV2,
    semantics: Option<&PalwClassSemanticsV1>,
    profile: &crate::palw_step::PalwShapeProfileV3,
    canonical: &crate::palw_v2::PalwJobContextV2,
) -> Result<(), String> {
    profile.validate_shape().map_err(|e| format!("class {class_id} shape: {e:?}"))?;
    if profile.shape_profile_id() != class_id || canonical.shape_profile_id != class_id {
        return Err(format!("class {class_id} profile/canonical identity mismatch"));
    }
    // The retained exact count is also the enumeration cap. This uses neither
    // the legacy executor constant nor a fixture's reduced dimensions.
    if let PalwPwuRuleV2::DerivedV1 { pwu_per_inference } = class.pwu_rule {
        let count = crate::palw_step::step_leaf_count_capped_v1(profile, canonical, pwu_per_inference)
            .map_err(|e| format!("class {class_id} canonical count: {e:?}"))?;
        if count != pwu_per_inference {
            return Err(format!("class {class_id} canonical PWU differs from retained count"));
        }
    }
    // MaxPerAttempt predates canonical derivation and cannot authenticate a
    // job's size. Preserve that historical qualification rather than invent it.
    if class.fused_attention != crate::palw_class_admission_v2::palw_profile_has_fused_attention_v1(profile) {
        return Err(format!("class {class_id} fused-attention geometry mismatch"));
    }
    if let Some(semantics) = semantics {
        if !semantics.well_formed_for(&class_id, &class.artifact_root) || !semantics.matches_context(canonical) {
            return Err(format!("class {class_id} retained canonical semantic identities differ"));
        }
        if let Some(relation) = semantics.prefix_state_relation {
            relation.check_profile(profile).map_err(str::to_owned)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn authentic() -> (Hash64, PalwClassStateV2, PalwClassAdmissionCarriageV2) {
        let params = crate::config::params::palw_t12_launch_params_v1();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode else { panic!("V2") };
        bundle
            .genesis_objects
            .into_iter()
            .find_map(|object| match object {
                crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
                    class_id,
                    artifact_root,
                    pwu_rule,
                    slash_value_per_pwu,
                    admission: Some(declaration),
                    ..
                } => Some((
                    class_id,
                    PalwClassStateV2 {
                        artifact_root,
                        pwu_rule,
                        slash_value_per_pwu,
                        status: crate::palw_state_v2::PalwClassStatusV2::Active,
                        registered_daa: 0,
                        registrant_bond: None,
                        fused_attention: crate::palw_class_admission_v2::palw_profile_has_fused_attention_v1(&declaration.profile),
                    },
                    *declaration,
                )),
                _ => None,
            })
            .expect("authentic held declaration")
    }
    #[test]
    fn authentic_large_canonical_uses_retained_pwu_not_legacy_executor_cap() {
        let (id, row, declaration) = authentic();
        let bytes = borsh::to_vec(&declaration).unwrap();
        let restored = validate_class_declaration_v1(id, &row, None, &bytes).unwrap();
        assert_eq!(borsh::to_vec(&restored).unwrap(), bytes);
        let mut wrong = row.clone();
        wrong.pwu_rule = PalwPwuRuleV2::DerivedV1 { pwu_per_inference: row.pwu_rule.canonical_leaves_v1() + 1 };
        assert!(validate_class_declaration_v1(id, &wrong, None, &bytes).is_err());
        wrong = row.clone();
        wrong.fused_attention = !wrong.fused_attention;
        assert!(validate_class_declaration_v1(id, &wrong, None, &bytes).is_err());
    }
    #[test]
    fn canonical_semantic_changes_with_same_profile_are_refused() {
        let (id, row, mut declaration) = authentic();
        let retained = PalwClassSemanticsV1::of_registration(id, row.artifact_root, &declaration.canonical, 0);
        validate_class_declaration_v1(id, &row, Some(&retained), &borsh::to_vec(&declaration).unwrap()).unwrap();
        declaration.canonical.runtime_class_id = Hash64::from_u64_word(999);
        assert!(validate_class_declaration_v1(id, &row, Some(&retained), &borsh::to_vec(&declaration).unwrap()).is_err());
        declaration.canonical.shape_profile_id = Hash64::from_u64_word(888);
        assert!(validate_class_declaration_v1(id, &row, None, &borsh::to_vec(&declaration).unwrap()).is_err());
    }
    #[test]
    fn malformed_retained_identity_and_trailing_bytes_are_refused() {
        let (id, row, declaration) = authentic();
        let mut retained = PalwClassSemanticsV1::of_registration(id, row.artifact_root, &declaration.canonical, 0);
        retained.artifact_root = Hash64::from_u64_word(999);
        let mut bytes = borsh::to_vec(&declaration).unwrap();
        assert!(validate_class_declaration_v1(id, &row, Some(&retained), &bytes).is_err());
        bytes.push(0);
        assert!(validate_class_declaration_v1(id, &row, None, &bytes).is_err());
    }
}
