//! File and signature fixture for the exact private complete-only constructor.
//! Nothing is installed into consensus: all objects must still be carried by
//! real funded transactions, included by a validating producer, and replayed.

use super::private_complete_check_params_v1;
use kaspa_consensus_core::{
    config::{
        Config,
        drill::{
            palw_t12_drill_cards_v1, palw_t12_drill_fee_float_outpoint_v1, palw_t12_drill_main_key_v1,
            palw_t12_drill_premine_outpoint_v1,
        },
    },
    palw_mode_v2::PalwConsensusMode,
    palw_onboarding_v1::PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
    palw_opv_bootstrap_v1::PALW_COMPLETE_CHECK_FEE_SOMPI_V1,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    palw_tir_admission_v1::palw_tir_post_genesis_registration_v1,
    palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1},
    palw_tir_class_v1::{
        PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1, PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1,
        PalwTirLayoutV1,
    },
};
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::ValidatorKey;
use misaka_palw_sdk::{
    bootstrap_check::{CompleteBootstrapArtifactV1, CompleteBootstrapCostsV1},
    onboarding_chain::{EnvelopeSigner, SignedRegistrationRequestV1, network_domain_v1},
    tir_manifest::PalwTirManifestV1,
    tir_registration::{tir_registration_message_v1, tir_registration_preflight_v1},
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

struct PrivateSigner(ValidatorKey);
impl EnvelopeSigner for PrivateSigner {
    fn public_key(&self) -> Vec<u8> {
        self.0.public_key().to_vec()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.0.sign_with_context(message, context).to_vec())
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Export into a NEW directory. Existing directories (including symlinks) are
/// refused. Seeds are only private salt-derived fixture keys, written as 0600
/// files; the returned public manifest never contains them.
pub fn export_private_complete_bootstrap_v1(config: &Config, output: &Path) -> Result<serde_json::Value, String> {
    let salt = config.palw_drill_genesis_salt.as_ref().ok_or("private export requires an explicit salt")?;
    let expected = private_complete_check_params_v1(salt)?;
    if config.private_population.is_some()
        || config.params.consensus_params_id() != expected.consensus_params_id()
        || config.params.skip_proof_of_work
    {
        return Err("bootstrap export requires the exact complete-v1 factory with PoW validation enabled".into());
    }
    let PalwConsensusMode::ConsensusV2(bundle) = &expected.palw_consensus_mode else {
        return Err("private factory has no native PALW registry".into());
    };
    // These are the exact genesis prices, not a guessed target or slash value.
    // A live chain must read its current terms before submitting registrations
    // after retargeting; this fixture is for its initial, bounded bootstrap.
    let (initial_target, slash_value_per_pwu) = bundle
        .genesis_objects
        .iter()
        .find_map(|object| match object {
            PalwConsensusObjectV2::ClassRegistered { class_id, initial_target, slash_value_per_pwu, .. }
                if *class_id == bundle.base_class_id =>
            {
                Some((*initial_target, *slash_value_per_pwu))
            }
            _ => None,
        })
        .ok_or("private factory has no exact base-class genesis pricing")?;
    let card = palw_t12_drill_cards_v1(salt).into_iter().next().ok_or("private factory has no funded bond")?;
    let bond = PalwBondKeyV2(palw_t12_drill_premine_outpoint_v1(salt, card.premine_index));
    let fixture = misaka_palw_tir_sketch::fixture::wide128_v1(37);
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: fixture.program.encode(),
        tokenizer_id: Hash64::default(),
        layout: PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 64,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: fixture.program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).map(|_| 4).collect(),
            state_tiles: vec![],
        },
    };
    // Allocation and work bounds are the same complete-check bounds the SDK
    // subsequently re-derives from this actual public file.
    kaspa_consensus_core::palw_opv_bootstrap_v1::palw_complete_check_domain_v1(&fixture.program, 64)?;
    fs::create_dir(output).map_err(|e| format!("fresh private fixture directory {}: {e}", output.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(output, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    let artifact_path = output.join("wide128-code-fixture.palwtir");
    let file_digest = misaka_palw_tir_artifact::write_container_v1(
        &artifact_path,
        &fixture.program,
        borsh::to_vec(&class.layout).map_err(|e| e.to_string())?,
        [0; 64],
        "seed 37 wide128 typed code fixture; no trained model or useful-work claim".into(),
        &mut |parameter, layer| {
            fixture.params.tensors.get(&(parameter, layer)).map(|tensor| tensor.to_le_bytes()).ok_or("fixture tensor missing".into())
        },
    )
    .map_err(|e| e.to_string())?;
    let root = PalwTirManifestV1::derive_streamed(&artifact_path).map_err(|e| e.to_string())?.inventory_root;
    let facts = PalwTirJobFactsV1::of(&class, &fixture.program, class.class_id(&root));
    let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&class).ok_or("fixture canonical context missing")?);
    let mut registration = palw_tir_post_genesis_registration_v1(
        class,
        canonical,
        root,
        0,
        initial_target,
        slash_value_per_pwu,
        0,
        bond,
        vec![],
        bundle.court.max_step_leaf_count(),
    )
    .map_err(|e| format!("native registration refused: {e}"))?;
    // The shipped native admission gate must accept the actual program and
    // layout before any key is loaded or signature is produced.
    tir_registration_preflight_v1(&expected, bundle, &registration, 1, &[])?;
    let artifact = CompleteBootstrapArtifactV1::from_registration_file(
        &registration,
        &artifact_path,
        64,
        CompleteBootstrapCostsV1 {
            maximum_binding_reservation_sompi: PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
            maximum_check_fee_sompi: PALW_COMPLETE_CHECK_FEE_SOMPI_V1,
        },
    )
    .map_err(|e| e.to_string())?;
    let signer = PrivateSigner(ValidatorKey::from_seed(card.bond.seed));
    let domain = network_domain_v1(&expected);
    let message = tir_registration_message_v1(domain, &registration).ok_or("fixture registration message absent")?;
    let signature = signer.sign_with_context(message.as_byte_slice(), PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1)?;
    if !kaspa_txscript::verify_mldsa87_with_context(
        &signer.public_key(),
        message.as_byte_slice(),
        &signature,
        PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1,
    )
    .map_err(|e| e.to_string())?
    {
        return Err("fixture native registration signature failed verification".into());
    }
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = &mut registration else { unreachable!() };
    admission.signature = signature;
    // The DAA-1 retirement fork is part of this factory's actual fired digest.
    // This finite envelope is deliberately not a permanent permission slip.
    let request = SignedRegistrationRequestV1::for_network(registration.clone(), bond, 1, 32, &expected).map_err(|e| e.to_string())?;
    let envelope = request.sign(&signer).map_err(|e| e.to_string())?;
    let binding = artifact.signed_artifact_binding(domain, bond, &signer).map_err(|e| e.to_string())?;
    for (name, object) in
        [("native-registration.borsh", registration), ("signed-registration.borsh", envelope), ("artifact-binding.borsh", binding)]
    {
        write_new(&output.join(name), &borsh::to_vec(&object).map_err(|e| e.to_string())?)?;
    }
    write_new(&output.join("bond.seed"), faster_hex::hex_string(&card.bond.seed).as_bytes())?;
    let main = palw_t12_drill_main_key_v1(salt);
    write_new(&output.join("funding.seed"), faster_hex::hex_string(&main.seed).as_bytes())?;
    let mut seats = Vec::new();
    for (index, seat) in palw_t12_drill_cards_v1(salt).into_iter().enumerate() {
        let key_file = if index == 0 { "bond.seed".into() } else { format!("bond-{index}.seed") };
        if index != 0 {
            write_new(&output.join(&key_file), faster_hex::hex_string(&seat.bond.seed).as_bytes())?;
        }
        let point = palw_t12_drill_premine_outpoint_v1(salt, seat.premine_index);
        let fee = palw_t12_drill_fee_float_outpoint_v1(salt, index as u32);
        seats.push(serde_json::json!({
            "keyFile": key_file, "bond": format!("{}:{}", point.transaction_id, point.index),
            "payAddress": seat.bond.address(kaspa_addresses::Prefix::Testnet).to_string(),
            "feeOutpoint": format!("{}:{}", fee.transaction_id, fee.index),
        }));
    }
    let manifest = serde_json::json!({
        "version": 1, "kind": "rfc12-private-complete-bootstrap-file-fixture",
        "consensusParamsId": expected.consensus_params_id().to_string(), "genesis": expected.genesis.hash.to_string(),
        "networkDomain": domain.to_string(), "saltId": salt.id(), "maxPositions": 64,
        "artifact": "wide128-code-fixture.palwtir", "artifactFileDigest": faster_hex::hex_string(&file_digest),
        "class": artifact.class().to_string(), "artifactRoot": artifact.artifact_root().to_string(),
        "kernelClass": artifact.kernel_class().to_string(), "kernelParamRoot": artifact.kernel_param_root().to_string(),
        "nativeRegistration": "native-registration.borsh", "signedRegistration": "signed-registration.borsh",
        "artifactBinding": "artifact-binding.borsh", "bondKeyFile": "bond.seed", "fundingKeyFile": "funding.seed",
        "bond": format!("{}:{}", bond.0.transaction_id, bond.0.index),
        "producerPayAddress": card.bond.address(kaspa_addresses::Prefix::Testnet).to_string(),
        "producerFeeOutpoint": format!("{}:{}", palw_t12_drill_fee_float_outpoint_v1(salt, 0).transaction_id,
            palw_t12_drill_fee_float_outpoint_v1(salt, 0).index),
        "fundingAddress": main.address(kaspa_addresses::Prefix::Testnet).to_string(),
        "seats": seats,
        "floorClass": bundle.base_class_id.to_string(), "pricingScope": "exact factory genesis; no live retarget claim",
        "initialTarget": initial_target.to_string(), "slashValuePerPwu": slash_value_per_pwu.to_string(),
        "envelopeValidFromDaa": 1, "envelopeValidUntilDaa": 32,
        "bindingReservationSompi": PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1.to_string(),
        "completeCheckFeeSompi": PALW_COMPLETE_CHECK_FEE_SOMPI_V1.to_string(),
        "skipProofOfWork": false, "nativeRegistrationIncluded": false, "artifactBindingIncluded": false,
        "completeCheckPassed": false, "matureUsefulWorkMeasured": false, "trainedModelVerified": false,
        "release100kMeasured": false, "securitySoundnessMeasured": false, "releaseApproval": false,
    });
    write_new(&output.join("bootstrap.json"), &serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::config::drill::PalwDrillSaltV1;

    fn config() -> Config {
        let salt = PalwDrillSaltV1::from_hex(&"42".repeat(32)).unwrap();
        let mut config = Config::new(private_complete_check_params_v1(&salt).unwrap());
        config.palw_drill_genesis_salt = Some(salt);
        config
    }
    #[test]
    fn private_bootstrap_exports_actual_files_and_salt_bound_signatures() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("new-owned-fixture");
        let config = config();
        let manifest = export_private_complete_bootstrap_v1(&config, &output).unwrap();
        assert_eq!(manifest["consensusParamsId"], config.params.consensus_params_id().to_string());
        assert_eq!(manifest["nativeRegistrationIncluded"], false);
        assert_eq!(manifest["skipProofOfWork"], false);
        let envelope: PalwConsensusObjectV2 = borsh::from_slice(&fs::read(output.join("signed-registration.borsh")).unwrap()).unwrap();
        let card = palw_t12_drill_cards_v1(config.palw_drill_genesis_salt.as_ref().unwrap()).remove(0);
        misaka_palw_sdk::onboarding_chain::verify_signed_registration_v1(
            &envelope,
            network_domain_v1(&config.params),
            &card.bond.pubkey,
        )
        .unwrap();
        assert!(
            misaka_palw_sdk::onboarding_chain::verify_signed_registration_v1(&envelope, Hash64::from_u64_word(99), &card.bond.pubkey)
                .is_err()
        );
        let seed = kaspa_pq_validator_core::load_validator_seed(output.join("bond.seed").to_str().unwrap()).unwrap();
        assert_eq!(seed, card.bond.seed);
        assert!(export_private_complete_bootstrap_v1(&config, &output).is_err());
    }

    #[test]
    fn private_bootstrap_refuses_mutated_factory_before_writing() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("refused");
        let mut config = config();
        config.params.skip_proof_of_work = true;
        assert!(export_private_complete_bootstrap_v1(&config, &output).is_err());
        assert!(!output.exists());
        config.params.skip_proof_of_work = false;
        config.params.palw_panel_free_v1.as_mut().unwrap().min_effective_bits = 1;
        assert!(export_private_complete_bootstrap_v1(&config, &output).is_err());
        assert!(!output.exists());
    }
}
