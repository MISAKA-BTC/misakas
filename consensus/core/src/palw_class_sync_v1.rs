//! Local pruning-sidecar material validation; no state/root/codec changes.
//! Class ids authenticate profiles, not canonical jobs. Retained PWU, fused
//! geometry and semantic identities must also agree before indexing a declaration.
use crate::{
    Hash64,
    palw_class_semantics_v1::PalwClassSemanticsV1,
    palw_state_v2::{PalwClassAdmissionCarriageV2, PalwClassStateV2, PalwPwuRuleV2},
};

// Testnet-12's pre-origin registration. The archived block body and acceptance
// records match the live chain's transaction and accepted-ID Merkle commitments.
// Carrier: dcfcb2536802d9b16c4ef4f903149f654f49e6ccca84e3eb9b157ea71f4861908b9287f6cb252f1c68e6a5af499ae30d12c93bc1db12a66aa637362b0711c33b
// Accepting header at DAA 508: 998d00b0a77fe9dddce3089364539e0f3ed2b795200bc9931da451ea69bfc6d3cc860496d0de3fc876ad467d27adf7aea30a2b6e90aa6dc89b6928a258f67fd0
const T12_LEGACY_CLASS_ID: Hash64 = Hash64::from_bytes([
    0x7d, 0x7e, 0x3d, 0x05, 0x23, 0x24, 0x66, 0x9a, 0xcc, 0xe2, 0x4d, 0xc9, 0xd9, 0x31, 0x6d, 0x79, 0x1c, 0x71, 0x52, 0x34, 0x14,
    0xe8, 0x99, 0x24, 0x9d, 0xc0, 0x40, 0xcd, 0x0a, 0xd1, 0xfa, 0xfd, 0x70, 0x5e, 0xb1, 0xe5, 0xb3, 0xa4, 0xce, 0x7e, 0x54, 0x05,
    0x07, 0x34, 0xe6, 0xdb, 0xc7, 0xdf, 0x62, 0xe6, 0xcf, 0xff, 0x84, 0x9c, 0xe2, 0xc7, 0xe1, 0xf7, 0x35, 0xdf, 0x3b, 0x29, 0x22,
    0xfc,
]);
const T12_LEGACY_CLASS_ARTIFACT: Hash64 = Hash64::from_bytes([
    0xcf, 0x4a, 0xfc, 0x69, 0x44, 0xd7, 0x82, 0xf3, 0xc5, 0x4e, 0xd0, 0x41, 0xd9, 0xa5, 0x75, 0xfe, 0xbf, 0xb8, 0xf7, 0xfa, 0x14,
    0x84, 0x60, 0x06, 0x6b, 0x39, 0xf4, 0xbb, 0x3d, 0xd0, 0xfd, 0x81, 0x83, 0xfc, 0x84, 0xe3, 0xb9, 0xd9, 0xef, 0x1f, 0x43, 0xa3,
    0x97, 0x84, 0xf2, 0x85, 0x76, 0xbb, 0xfd, 0xe9, 0x95, 0xf8, 0xa6, 0xe9, 0xef, 0x94, 0x9c, 0xa0, 0xc1, 0xe4, 0xfa, 0x2d, 0xb2,
    0x4f,
]);
const T12_LEGACY_ADMISSION_DIGEST: Hash64 = Hash64::from_bytes([
    0x09, 0x8d, 0x31, 0xb9, 0x04, 0x07, 0x8d, 0x64, 0x08, 0x47, 0x5f, 0x3e, 0xc5, 0x25, 0x2b, 0x2f, 0x6c, 0x3b, 0x5a, 0x67, 0x81,
    0xf9, 0xd7, 0xc9, 0x10, 0x2c, 0x66, 0xac, 0xad, 0x35, 0xcd, 0x75, 0x62, 0xc5, 0x7d, 0x4b, 0xa7, 0x8c, 0x72, 0xc5, 0x39, 0x16,
    0xce, 0x34, 0xa5, 0x85, 0xcc, 0xb3, 0x3a, 0xe0, 0x0c, 0x28, 0x82, 0xbd, 0xc6, 0x7d, 0x3c, 0x58, 0xba, 0xb9, 0x44, 0x98, 0x05,
    0xe8,
]);

/// Recognize the exact declaration of the historical testnet-12 registration
/// when the snapshot predates rooted registration origins.
pub fn t12_legacy_class_declaration_matches_v1(
    network_id: &[u8],
    genesis: Hash64,
    class_id: Hash64,
    class: &PalwClassStateV2,
    bytes: &[u8],
) -> bool {
    network_id == b"testnet-12"
        && genesis == crate::config::genesis::PALW_T12_GENESIS.hash
        && class_id == T12_LEGACY_CLASS_ID
        && class.artifact_root == T12_LEGACY_CLASS_ARTIFACT
        && class.registered_daa == 508
        && crate::palw_class_registration_origin_v1::palw_class_admission_bytes_digest_v1(bytes) == T12_LEGACY_ADMISSION_DIGEST
}

/// Recover an omitted genesis declaration from this network's compiled genesis
/// objects. Only the exact original registration is eligible; a matching class
/// label, a later registration, or a changed economic/semantic row is not enough.
/// The snapshot's consensus root is still checked by its importer.
pub fn snapshot_genesis_declaration_v1<'a>(
    objects: &'a [crate::palw_state_v2::PalwConsensusObjectV2],
    class_id: Hash64,
    class: &PalwClassStateV2,
    semantics: Option<&PalwClassSemanticsV1>,
) -> Result<Option<&'a PalwClassAdmissionCarriageV2>, String> {
    use crate::palw_state_v2::PalwConsensusObjectV2;
    if class.registered_daa != 0 || class.registrant_bond.is_some() {
        return Ok(None);
    }
    for object in objects {
        if let PalwConsensusObjectV2::ClassRegistered {
            class_id: id,
            artifact_root,
            pwu_rule,
            slash_value_per_pwu,
            admission: Some(admission),
            ..
        } = object
            && *id == class_id
            && *artifact_root == class.artifact_root
            && *pwu_rule == class.pwu_rule
            && *slash_value_per_pwu == class.slash_value_per_pwu
        {
            validate_class_context_v1(class_id, class, semantics, &admission.profile, &admission.canonical)?;
            return Ok(Some(admission.as_ref()));
        }
    }
    Ok(None)
}

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
    #[test]
    fn snapshot_genesis_recovery_requires_exact_registration_and_semantics() {
        let params = crate::config::params::palw_t12_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode else { panic!("V2") };
        let mut recovered = 0;
        for object in &bundle.genesis_objects {
            let crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
                class_id,
                artifact_root,
                pwu_rule,
                slash_value_per_pwu,
                admission: Some(admission),
                ..
            } = object
            else {
                continue;
            };
            let class = PalwClassStateV2 {
                artifact_root: *artifact_root,
                pwu_rule: *pwu_rule,
                slash_value_per_pwu: *slash_value_per_pwu,
                status: crate::palw_state_v2::PalwClassStatusV2::Active,
                registered_daa: 0,
                registrant_bond: None,
                fused_attention: crate::palw_class_admission_v2::palw_profile_has_fused_attention_v1(&admission.profile),
            };
            let read = |id, row: &PalwClassStateV2, semantics: Option<&PalwClassSemanticsV1>| {
                snapshot_genesis_declaration_v1(&bundle.genesis_objects, id, row, semantics)
                    .map(|v| v.map(|a| borsh::to_vec(a).unwrap()))
            };
            let expected = Some(borsh::to_vec(admission.as_ref()).unwrap());
            assert_eq!(read(*class_id, &class, None).unwrap(), expected);
            let mut semantics = PalwClassSemanticsV1::of_registration(*class_id, *artifact_root, &admission.canonical, 0);
            assert_eq!(read(*class_id, &class, Some(&semantics)).unwrap(), expected);
            semantics.tokenizer_id = Hash64::from_u64_word(999);
            assert!(read(*class_id, &class, Some(&semantics)).is_err());
            assert_eq!(read(Hash64::default(), &class, None).unwrap(), None);
            assert!(snapshot_genesis_declaration_v1(&[], *class_id, &class, None).unwrap().is_none());
            for change in 0..5 {
                let mut row = class.clone();
                match change {
                    0 => row.registered_daa = 1,
                    1 => row.artifact_root = Hash64::default(),
                    2 => row.slash_value_per_pwu += 1,
                    3 => row.pwu_rule = PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 },
                    _ => {
                        row.registrant_bond = Some(crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint {
                            transaction_id: Hash64::default(),
                            index: 0,
                        }))
                    }
                }
                assert_eq!(read(*class_id, &row, None).unwrap(), None);
            }
            let mut row = class.clone();
            row.fused_attention = !row.fused_attention;
            assert!(read(*class_id, &row, None).is_err());
            // Frozen economic status does not erase authentic sync material.
            // Execution remains gated by the existing status checks.
            recovered += 1;
        }
        assert_eq!(recovered, 2, "both held genesis declarations, including the reported 2M class");
    }

    #[test]
    fn t12_legacy_pin_matches_only_the_archived_registration() {
        let bytes = include_bytes!("../tests/corpus/t12-daa508-class-admission.borsh");
        let genesis = crate::config::genesis::PALW_T12_GENESIS.hash;
        let (_, mut class, _) = authentic();
        class.artifact_root = T12_LEGACY_CLASS_ARTIFACT;
        class.registered_daa = 508;
        assert!(t12_legacy_class_declaration_matches_v1(b"testnet-12", genesis, T12_LEGACY_CLASS_ID, &class, bytes));
        assert!(!t12_legacy_class_declaration_matches_v1(b"testnet-11", genesis, T12_LEGACY_CLASS_ID, &class, bytes));
        assert!(!t12_legacy_class_declaration_matches_v1(b"testnet-12", Hash64::default(), T12_LEGACY_CLASS_ID, &class, bytes));
        assert!(!t12_legacy_class_declaration_matches_v1(b"testnet-12", genesis, Hash64::default(), &class, bytes));
        let mut altered = bytes.to_vec();
        altered[0] ^= 1;
        assert!(!t12_legacy_class_declaration_matches_v1(b"testnet-12", genesis, T12_LEGACY_CLASS_ID, &class, &altered));
        class.registered_daa += 1;
        assert!(!t12_legacy_class_declaration_matches_v1(b"testnet-12", genesis, T12_LEGACY_CLASS_ID, &class, bytes));
        class.registered_daa -= 1;
        class.artifact_root = Hash64::default();
        assert!(!t12_legacy_class_declaration_matches_v1(b"testnet-12", genesis, T12_LEGACY_CLASS_ID, &class, bytes));
    }

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
