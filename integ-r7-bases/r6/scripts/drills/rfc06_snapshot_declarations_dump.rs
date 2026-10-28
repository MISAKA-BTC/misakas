//! Recover authentic declarations for the native four-population fixture.
//! Compile against the SAME core/hash/Borsh libraries as the producing native
//! test binary. This tool is outside final-test-v2-build-inputs.json; it neither
//! changes production sources nor substitutes for an actual server/index test.
//! Usage: dump native-carriage.borsh NEW-declarations.borsh
#![allow(dead_code)]

use borsh::BorshDeserialize;
use kaspa_consensus_core::{
    config::params::{PALW_RC_GENESIS_ARTIFACT_ROOT, palw_t12_genesis_held_class_ids_v1, palw_t12_launch_params_v1},
    palw_base0_profile::{PALW_RC_BASE0_CANONICAL, palw_rc_base0_registration_v1, rc_job_context},
    palw_mode_v2::PalwConsensusMode,
    palw_state_v2::{PalwClassAdmissionCarriageV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwStateCarriageV2},
    palw_step::step_leaf_count_capped_v1,
};
use kaspa_hashes::Hash64;
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Write},
};

const CANONICAL_INPUT_LIMIT: u64 = 512 << 20;

#[derive(Clone)]
pub(crate) struct ExpectedClass {
    id: Hash64,
    artifact_root: Hash64,
    pwu: PalwPwuRuleV2,
    slash_value_per_pwu: u64,
    canonical_leaf_cap: u64,
    admission: Option<Box<PalwClassAdmissionCarriageV2>>,
}

pub(crate) fn fixture_classes() -> Result<(Hash64, Vec<ExpectedClass>), String> {
    // t12_with_harness_cards only changes BondRegistered keys/payouts and the
    // premine/genesis identity. complete_armed adds bonds and Panel parameters.
    // Neither changes the ClassRegistered objects returned by this public API.
    let params = palw_t12_launch_params_v1();
    let PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode else {
        return Err("launch fixture is not ConsensusV2".into());
    };
    let canonical_leaf_cap = bundle.court.max_step_leaf_count();
    let classes: Vec<_> = bundle
        .genesis_objects
        .into_iter()
        .filter_map(|object| match object {
            PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, pwu_rule, slash_value_per_pwu, admission, .. } => {
                Some(ExpectedClass { id: class_id, artifact_root, pwu: pwu_rule, slash_value_per_pwu, canonical_leaf_cap, admission })
            }
            _ => None,
        })
        .collect();
    let held: BTreeSet<_> = palw_t12_genesis_held_class_ids_v1().into_iter().collect();
    let carried: BTreeSet<_> = classes.iter().filter(|class| class.admission.is_some()).map(|class| class.id).collect();
    if classes.len() != 3 || held.len() != 2 || carried != held {
        return Err("fixture must contain exactly the BASE0 floor and both authentic held declarations".into());
    }
    Ok((bundle.base_class_id, classes))
}

pub(crate) fn complete_declarations(
    carriage: &PalwStateCarriageV2,
    base: Hash64,
    expected: &[ExpectedClass],
) -> Result<Vec<(Hash64, Vec<u8>)>, String> {
    let expected_ids: BTreeSet<_> = expected.iter().map(|class| class.id).collect();
    let actual_ids: BTreeSet<_> = carriage.classes.keys().copied().collect();
    if expected.len() != 3 || expected_ids.len() != 3 || actual_ids != expected_ids {
        return Err("unknown, duplicate or missing native class: all three fixture classes must match".into());
    }
    if carriage.class_semantics.keys().any(|id| !expected_ids.contains(id)) {
        return Err("unknown class semantic identity".into());
    }
    let mut declarations = Vec::new();
    let mut builtin = 0;
    for class in expected {
        let row = carriage.classes.get(&class.id).ok_or("native class is missing")?;
        if row.artifact_root != class.artifact_root
            || row.pwu_rule != class.pwu
            || row.slash_value_per_pwu != class.slash_value_per_pwu
        {
            return Err(format!("native class {} differs in artifact root, PWU or slash value", class.id));
        }
        if row.registered_daa != 0 || row.registrant_bond.is_some() {
            return Err(format!("class {} is not the fixture's genesis registration", class.id));
        }
        let (profile, canonical) = match class.admission.as_ref() {
            Some(admission) => (&admission.profile, &admission.canonical),
            None => {
                if class.id != base || class.artifact_root != PALW_RC_GENESIS_ARTIFACT_ROOT {
                    return Err("only the pinned binary BASE0 genesis floor may omit its admission carriage".into());
                }
                let (profile, _) = palw_rc_base0_registration_v1(class.artifact_root).map_err(|e| format!("floor profile: {e:?}"))?;
                let canonical = rc_job_context(&profile, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
                verify_canonical(class, &profile, &canonical)?;
                verify_semantics(carriage, class, &profile, &canonical)?;
                builtin += 1;
                continue;
            }
        };
        verify_canonical(class, profile, canonical)?;
        verify_semantics(carriage, class, profile, canonical)?;
        declarations.push((class.id, borsh::to_vec(class.admission.as_ref().unwrap().as_ref()).map_err(|e| e.to_string())?));
    }
    if declarations.len() != 2 || builtin != 1 {
        return Err("both held declarations and exactly one binary-derived floor are required".into());
    }
    declarations.sort_by_key(|(id, _)| *id);
    Ok(declarations)
}

fn verify_semantics(
    carriage: &PalwStateCarriageV2,
    class: &ExpectedClass,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    canonical: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
) -> Result<(), String> {
    let fused =
        class.admission.is_some() && kaspa_consensus_core::palw_class_admission_v2::palw_profile_has_fused_attention_v1(profile);
    if carriage.classes[&class.id].fused_attention != fused {
        return Err(format!("class {} fused-attention binding differs", class.id));
    }
    // Historical absence remains absence. When the native snapshot DOES retain
    // semantic identities, check every identity against the authentic canonical.
    if let Some(semantics) = carriage.class_semantics.get(&class.id)
        && (!semantics.well_formed_for(&class.id, &class.artifact_root) || !semantics.matches_context(canonical))
    {
        return Err(format!("class {} immutable canonical identities differ", class.id));
    }
    Ok(())
}

fn verify_canonical(
    class: &ExpectedClass,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    canonical: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
) -> Result<(), String> {
    if profile.shape_profile_id() != class.id || canonical.shape_profile_id != class.id {
        return Err(format!("class {} profile/canonical id mismatch", class.id));
    }
    // Use this fixture's authentic held ladder, never the narrower legacy cap.
    let count =
        step_leaf_count_capped_v1(profile, canonical, class.canonical_leaf_cap).map_err(|e| format!("canonical count: {e:?}"))?;
    if class.pwu != (PalwPwuRuleV2::DerivedV1 { pwu_per_inference: count }) {
        return Err(format!("class {} canonical PWU count mismatch", class.id));
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let source = args.next().ok_or("native carriage path required")?;
    let output = args.next().ok_or("new declaration output path required")?;
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    let file = File::open(source)?;
    if file.metadata()?.len() > CANONICAL_INPUT_LIMIT {
        return Err("native source exceeds the experimental canonical byte cap".into());
    }
    let carriage = PalwStateCarriageV2::deserialize_reader(&mut BufReader::new(file))?;
    let point = carriage.last_point.as_ref().ok_or("native source has no actual block context")?;
    let (base, expected) = fixture_classes()?;
    let declarations = complete_declarations(&carriage, base, &expected)?;
    let size = borsh::object_length(&declarations)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut writer = BufWriter::with_capacity(64 << 10, options.open(output)?);
    borsh::to_writer(&mut writer, &declarations)?;
    writer.flush()?;
    println!(
        "{{\"schemaVersion\":1,\"kind\":\"rfc06-native-fixture-authentic-declarations\",\"point\":\"{}\",\"daaScore\":{},\"nativeClassesMatched\":3,\"heldDeclarationsComplete\":2,\"builtinFloorAdmissionNone\":1,\"builtinFloorProfileDerived\":true,\"builtinFloorWeightsRecomputed\":false,\"declarationVecBytes\":{size},\"declarationsCompleteForFixture\":true,\"source\":\"matching-public-launch-genesis-objects\",\"actualServerIndexChecked\":false,\"stateRootChecked\":false,\"realP2pAccepted\":false}}",
        point.block, point.daa_score,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_state_v2::{PalwChainStateV2, PalwClassStateV2, PalwClassStatusV2};

    // These are declaration-binding controls only; they create no accepted node
    // state and make no native/carriage/resource/P2P acceptance claim.
    fn control() -> (Hash64, Vec<ExpectedClass>, PalwStateCarriageV2) {
        let (base, classes) = fixture_classes().unwrap();
        let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
        for class in &classes {
            carriage.classes.insert(
                class.id,
                PalwClassStateV2 {
                    artifact_root: class.artifact_root,
                    slash_value_per_pwu: class.slash_value_per_pwu,
                    pwu_rule: class.pwu.clone(),
                    status: PalwClassStatusV2::Active,
                    registered_daa: 0,
                    registrant_bond: None,
                    fused_attention: class.admission.as_ref().is_some_and(|a| {
                        kaspa_consensus_core::palw_class_admission_v2::palw_profile_has_fused_attention_v1(&a.profile)
                    }),
                },
            );
        }
        (base, classes, carriage)
    }

    #[test]
    fn authentic_two_held_carriages_and_floor_exception_are_exact() {
        let (base, expected, carriage) = control();
        let declarations = complete_declarations(&carriage, base, &expected).unwrap();
        assert_eq!(declarations.len(), 2);
        for (id, bytes) in &declarations {
            let authentic = expected.iter().find(|class| class.id == *id).unwrap().admission.as_ref().unwrap();
            assert_eq!(*bytes, borsh::to_vec(authentic.as_ref()).unwrap());
        }
        assert_eq!(borsh::object_length(&declarations).unwrap(), 9_610);
    }

    #[test]
    fn missing_unknown_duplicate_roots_and_pwu_are_refused() {
        let (base, expected, carriage) = control();
        let mut missing = carriage.clone();
        missing.classes.remove(&base);
        assert!(complete_declarations(&missing, base, &expected).is_err());
        let mut unknown = carriage.clone();
        unknown.classes.insert(Hash64::from_bytes([0x85; 64]), carriage.classes[&base].clone());
        assert!(complete_declarations(&unknown, base, &expected).is_err());
        let mut duplicate = expected.clone();
        duplicate[1] = duplicate[0].clone();
        assert!(complete_declarations(&carriage, base, &duplicate).is_err());
        let mut wrong_root = carriage.clone();
        wrong_root.classes.get_mut(&base).unwrap().artifact_root = Hash64::from_bytes([0x86; 64]);
        assert!(complete_declarations(&wrong_root, base, &expected).is_err());
        let mut wrong_pwu = carriage.clone();
        wrong_pwu.classes.get_mut(&base).unwrap().pwu_rule = PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 };
        assert!(complete_declarations(&wrong_pwu, base, &expected).is_err());
    }

    #[test]
    fn wrong_canonical_and_unknown_admission_none_are_refused() {
        let (base, expected, carriage) = control();
        let mut bad = expected.clone();
        bad.iter_mut().find_map(|class| class.admission.as_mut()).unwrap().canonical.shape_profile_id = base;
        assert!(complete_declarations(&carriage, base, &bad).is_err());
        let mut absent = expected.clone();
        absent.iter_mut().find(|class| class.admission.is_some()).unwrap().admission = None;
        assert!(complete_declarations(&carriage, base, &absent).is_err());
        let mut wrong_count = expected.clone();
        wrong_count.iter_mut().find_map(|class| class.admission.as_mut()).unwrap().canonical.exact_decode_tokens += 1;
        assert!(complete_declarations(&carriage, base, &wrong_count).is_err());
    }

    #[test]
    fn retained_semantic_identities_and_fused_binding_are_checked() {
        let (base, expected, mut carriage) = control();
        let class = expected.iter().find(|class| class.admission.is_some()).unwrap();
        let canonical = &class.admission.as_ref().unwrap().canonical;
        let semantics = kaspa_consensus_core::palw_class_semantics_v1::PalwClassSemanticsV1::of_registration(
            class.id,
            class.artifact_root,
            canonical,
            0,
        );
        carriage.class_semantics.insert(class.id, semantics);
        assert!(complete_declarations(&carriage, base, &expected).is_ok());
        carriage.class_semantics.get_mut(&class.id).unwrap().runtime_class_id = Hash64::from_bytes([0x87; 64]);
        assert!(complete_declarations(&carriage, base, &expected).is_err());
        carriage.class_semantics.clear();
        carriage.classes.get_mut(&class.id).unwrap().fused_attention ^= true;
        assert!(complete_declarations(&carriage, base, &expected).is_err());
    }
}
