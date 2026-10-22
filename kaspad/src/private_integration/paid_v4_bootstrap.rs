//! Separate V4 source commitment: old paid_bootstrap.rs is deliberately not edited.
//! This exporter prepares primitive artifacts under the actual V4 identity.
//! Compact contracts and signed recovery enrollment still require live work.
//! Versioned paid-v4 bootstrap files. Reuses the unchanged real primitive/code
//! artifact generator, then authenticates and signs all policy-bound objects
//! against the new exact private factory. Old v5 files remain separate inputs.
use kaspa_consensus_core::{
    config::Config,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
};
use misaka_palw_sdk::{
    bootstrap_check::{CompleteBootstrapArtifactV1, CompleteBootstrapCostsV1, ConformanceCommitmentV1, RootV1},
    onboarding_chain::{EnvelopeSigner, SignedRegistrationRequestV1, network_domain_v1},
};
use std::{fs::OpenOptions, io::Write, path::Path};
struct PaidSigner(kaspa_pq_validator_core::ValidatorKey);
impl EnvelopeSigner for PaidSigner {
    fn public_key(&self) -> Vec<u8> {
        self.0.public_key().to_vec()
    }
    fn sign_with_context(&self, m: &[u8], c: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.0.sign_with_context(m, c).to_vec())
    }
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut f = o.open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(bytes).and_then(|_| f.sync_all()).map_err(|e| e.to_string())
}
pub fn export_private_paid_bootstrap_v4(config: &Config, output: &Path) -> Result<serde_json::Value, String> {
    let salt = config.palw_drill_genesis_salt.as_ref().ok_or("private paid bootstrap needs explicit salt")?;
    // RFC-0012 §4.4: the only permitted departure from the exact factory is a later private retirement height.
    let expected = super::private_expected_retirement_v1(super::paid_v4_factory::private_paid_claim_params_v4(salt)?, config)?;
    if config.private_population.is_some()
        || config.params.consensus_params_id() != expected.consensus_params_id()
        || config.params.skip_proof_of_work
    {
        return Err("paid bootstrap requires its exact real-PoW private paid-v4 factory".into());
    }
    super::paid_v4_factory::validate_private_paid_claim_relations_v4(&config.params)?;
    export_paid_bootstrap_for_validated_params(config, output)
}

/// Separate source-paid profile: artifacts are signed under its actual combined
/// Params identity. The old paid-v4 entry point still requires its exact factory.
pub fn export_private_source_paid_bootstrap_v1(config: &Config, output: &Path) -> Result<serde_json::Value, String> {
    let salt = config.palw_drill_genesis_salt.as_ref().ok_or("source-paid bootstrap salted identity absent")?;
    if config.private_population.is_some()
        || config.params.skip_proof_of_work
        || config.params.palw_claim_compact_v4.is_none()
        || config.params.palw_material_release_recovery_v2.is_none()
    {
        return Err("source-paid bootstrap requires its exact real-PoW paid-v4 base and explicit source policy".into());
    }
    super::source_bootstrap_factory_v1::validate_private_source_bootstrap_relations_v1(&config.params, salt)?;
    export_paid_bootstrap_for_validated_params(config, output)
}

fn export_paid_bootstrap_for_validated_params(config: &Config, output: &Path) -> Result<serde_json::Value, String> {
    let salt = config.palw_drill_genesis_salt.as_ref().ok_or("private paid bootstrap salted identity absent")?;
    let expected = &config.params;
    // The original exporter retains its exact complete-v1 guard and source bytes.
    // This base artifact is a deterministic public file fixture, not an old
    // conformance or receipt silently promoted to a new ruleset.
    let mut base = config.clone();
    base.params = super::private_complete_check_params_v1(salt)?;
    if base.params.genesis.hash != expected.genesis.hash {
        return Err("base artifact has another genesis".into());
    }
    std::fs::create_dir(output).map_err(|e| format!("fresh paid bootstrap directory: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(output, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    let base_dir = output.join("base-material");
    let base_manifest = super::bootstrap::export_private_complete_bootstrap_v1(&base, &base_dir)?;
    let registration: PalwConsensusObjectV2 =
        borsh::from_slice(&std::fs::read(base_dir.join("native-registration.borsh")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &expected.palw_consensus_mode else {
        return Err("paid factory registry missing".into());
    };
    let from = base_manifest["envelopeValidFromDaa"].as_u64().ok_or("base fixture lacks actual signed from DAA")?;
    let until = base_manifest["envelopeValidUntilDaa"].as_u64().ok_or("base fixture missing finite signed-registration interval")?;
    misaka_palw_sdk::tir_registration::tir_registration_preflight_v1(&expected, bundle, &registration, from, &[])?;
    let artifact = CompleteBootstrapArtifactV1::from_registration_file(
        &registration,
        &base_dir.join("wide128-code-fixture.palwtir"),
        64,
        CompleteBootstrapCostsV1 {
            maximum_binding_reservation_sompi: kaspa_consensus_core::palw_onboarding_v1::PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
            maximum_check_fee_sompi: kaspa_consensus_core::palw_opv_bootstrap_v1::PALW_COMPLETE_CHECK_FEE_SOMPI_V1,
        },
    )
    .map_err(|e| e.to_string())?;
    let card = kaspa_consensus_core::config::drill::palw_t12_drill_cards_v1(salt)
        .into_iter()
        .next()
        .ok_or("private producer card missing")?;
    let bond = PalwBondKeyV2(kaspa_consensus_core::config::drill::palw_t12_drill_premine_outpoint_v1(salt, card.premine_index));
    let signer = PaidSigner(kaspa_pq_validator_core::ValidatorKey::from_seed(card.bond.seed));
    let domain = network_domain_v1(&expected);
    let message = misaka_palw_sdk::tir_registration::tir_registration_message_v1(domain, &registration)
        .ok_or("native registration message missing")?;
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = &registration else {
        return Err("base native registration kind changed".into());
    };
    if !kaspa_txscript::verify_mldsa87_with_context(
        &signer.public_key(),
        message.as_byte_slice(),
        &admission.signature,
        kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1,
    )
    .map_err(|e| e.to_string())?
    {
        return Err("base primitive registration does not authenticate under actual paid chain authority".into());
    }
    let envelope = SignedRegistrationRequestV1::for_network(registration.clone(), bond, from, until, &expected)
        .map_err(|e| e.to_string())?
        .sign(&signer)
        .map_err(|e| e.to_string())?;
    let binding = artifact.signed_artifact_binding(domain, bond, &signer).map_err(|e| e.to_string())?;
    let mut statement: ConformanceCommitmentV1 =
        borsh::from_slice(&std::fs::read(base_dir.join("code-fixture-commitment.borsh")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut ruleset = [0; 64];
    ruleset[..32].copy_from_slice(&expected.consensus_identity_id().as_bytes());
    statement.ruleset_id = ruleset;
    statement.commitment_object_id = None;
    statement.canonical_commitment_position = None;
    statement.source_root = RootV1::Present(kaspa_consensus_core::palw_conformance_evidence_v1::tool_root(
        b"MISAKA/PRIVATE/PAID-BOOTSTRAP-SOURCE/V4",
        &(statement.source_root, include_str!("paid_v4_bootstrap.rs"), expected.consensus_params_id().as_bytes()),
    ));
    for (name, object) in
        [("native-registration.borsh", registration), ("signed-registration-v4.borsh", envelope), ("artifact-binding.borsh", binding)]
    {
        write_new(&output.join(name), &borsh::to_vec(&object).map_err(|e| e.to_string())?)?;
    }
    write_new(&output.join("code-fixture-commitment-v4.borsh"), &borsh::to_vec(&statement).map_err(|e| e.to_string())?)?;
    let mut manifest = serde_json::json!({"schema":"misaka.private-paid-bootstrap.v4","version":4,"kind":"rfc12-private-paid-bootstrap-file-fixture-v4",
        "network":expected.net.to_string(),"skipProofOfWork":false,"artifactBindingIncluded":false,"completeCheckPassed":false,
        "consensusParamsId":expected.consensus_params_id().to_string(),
        "rulesetId":faster_hex::hex_string(&ruleset),"genesis":expected.genesis.hash.to_string(),
        "baseMaterial":"base-material/wide128-code-fixture.palwtir","baseMaterialManifest":"base-material/bootstrap.json",
        "nativeRegistration":"native-registration.borsh","signedRegistration":"signed-registration-v4.borsh",
        "artifactBinding":"artifact-binding.borsh","statement":"code-fixture-commitment-v4.borsh",
        "statementRoot":faster_hex::hex_string(&statement.statement_root()),"kernelClass":artifact.kernel_class().to_string(),
        "completeScopeRevalidated":true,"nativeRegistrationIncluded":false,"conformanceObserved":false,"paidClaimIncluded":false,
        "sourceLocked":false,"nativeFinalObserved":false,"releaseApproval":false,"securitySoundnessMeasured":false,
        "trainedModelVerified":false,"independentImplementationClaim":false,
        "baseArtifactsAreControlReceipts":false,"envelopeValidFromDaa":from,"envelopeValidUntilDaa":until,
        "bondKeyFile":"base-material/bond.seed","fundingKeyFile":"base-material/funding.seed",
        "seats":base_manifest["seats"],"initialTarget":base_manifest["initialTarget"],"slashValuePerPwu":base_manifest["slashValuePerPwu"]});
    manifest["privateProbeProfile"] = super::private_probe_manifest(config);
    if config.params.palw_selected_source_bootstrap_v1.is_some() {
        manifest["schema"] = serde_json::json!("misaka.private-source-paid-bootstrap.v1");
        manifest["kind"] = serde_json::json!("rfc-private-source-paid-bootstrap-file-fixture-v1");
        manifest["sourceBootstrapApprovalsObserved"] = serde_json::json!(false);
    }
    manifest["compactContractIncluded"] = serde_json::json!(false);
    manifest["recoveryEnrollmentIncluded"] = serde_json::json!(false);
    // Copy only primitive/material/genesis-money fields whose bytes and scope
    // were revalidated above. Policy identity, statement and signed envelope
    // are new files; no control/receipt/acceptance field is inherited.
    for field in [
        "class",
        "artifactRoot",
        "kernelParamRoot",
        "maxPositions",
        "verificationPlanRoot",
        "floorClass",
        "bond",
        "producerPayAddress",
        "producerFeeOutpoint",
        "fundingAddress",
        "initialTarget",
        "slashValuePerPwu",
        "bindingReservationSompi",
        "completeCheckFeeSompi",
        "artifactFileDigest",
    ] {
        let value = base_manifest.get(field).ok_or_else(|| format!("base artifact lacks required material field {field}"))?;
        manifest[field] = value.clone();
    }
    manifest["artifact"] = serde_json::json!("base-material/wide128-code-fixture.palwtir");
    manifest["commitment"] = serde_json::json!("code-fixture-commitment-v4.borsh");
    manifest["commitmentScope"] = serde_json::json!("code-fixture-provenance-v4.json");
    manifest["networkDomain"] = serde_json::json!(domain.to_string());
    manifest["saltId"] = serde_json::json!(salt.id());
    let mut seats = base_manifest["seats"].as_array().ok_or("base artifact has no exact funded seat list")?.clone();
    let cards = kaspa_consensus_core::config::drill::palw_t12_drill_cards_v1(salt);
    for (index, seat) in seats.iter_mut().enumerate() {
        let key = seat["keyFile"].as_str().ok_or("base seat key file missing")?;
        if key.contains('/') || key.contains('\\') || key == "." || key == ".." {
            return Err("base seat key file is not a canonical fixture basename".into());
        }
        seat["keyFile"] = serde_json::json!(format!("base-material/{key}"));
        // RFC-0008 PAV3 (2026-10-15 round 3): the seat's card operator, which a prospective native session names for its
        // executor and the fold compares against every provider's operator. Derived from the same salt card as the bond.
        let card = cards.get(index).ok_or("a base seat has no salt card")?;
        seat["operatorId"] =
            serde_json::json!(kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&card.operator.pubkey).to_string());
    }
    manifest["seats"] = serde_json::json!(seats);
    // The paid factory's monetary-material policy terms a native session enrollment must sign (`check_enrollment`). Copied
    // from the exact factory params this exporter validated above; values, not an approval.
    let material = &expected.palw_monetary_material_v1.as_ref().ok_or("paid factory monetary material policy missing")?.policy;
    manifest["monetaryMaterialPolicy"] = serde_json::json!({
        "id": material.id().to_string(), "windowDaa": material.window_daa,
        "minimumCollateralSompi": material.minimum_collateral_sompi, "minimumFeeSompi": material.minimum_fee_sompi,
        "maxEnrollmentDaa": material.max_enrollment_daa, "maxBytesPerContract": material.max_bytes_per_contract,
        "nativeSessionMaxUnits": kaspa_consensus_core::palw_material_capacity_v1::PALW_MATERIAL_MAX_UNITS_V1,
        "approval": false});
    let provenance = serde_json::json!({"schema":"misaka.private-paid-bootstrap-provenance.v4",
        "actualConsensusParamsId":expected.consensus_params_id().to_string(),"actualRulesetId":faster_hex::hex_string(&ruleset),
        "source":"unchanged complete-v1 primitive artifact, revalidated native scope and fresh paid-v4 statement/envelope",
        "scope":"full finite complete-check input/leaf domain from the actual retained file",
        "independentImplementationClaim":false,"trainedModelVerified":false,"releaseApproval":false,
        "conformanceObserved":false,"oldControlReceiptsPromoted":false});
    write_new(&output.join("code-fixture-provenance-v4.json"), &serde_json::to_vec_pretty(&provenance).map_err(|e| e.to_string())?)?;
    write_new(&output.join("manifest.json"), &serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?)?;
    std::fs::File::open(&base_dir).and_then(|d| d.sync_all()).map_err(|e| format!("base material directory sync: {e}"))?;
    std::fs::File::open(output).and_then(|d| d.sync_all()).map_err(|e| format!("paid fixture directory sync: {e}"))?;
    Ok(manifest)
}
