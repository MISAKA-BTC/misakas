//! Signed new-path Program work fixture from authenticated live native rows.
//! Immutable complete-v1 exporter artifacts are never read or modified here.
use kaspa_consensus_core::{config::Config, palw_state_v2::PalwBondKeyV2};
use kaspa_hashes::Hash64;
use misaka_palw_kernel::{hash::Digest, public::TensorWireV1};
use misaka_palw_sdk::claim_paid_v2::private_bootstrap::produce_program_work_v3;
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
};

/// Each root pin is supplied separately by the fixture operator from selected
/// chain authority. A consistent snapshot file alone is never treated as chain
/// membership, native conformance, free collateral or an acceptance receipt.
#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PrivatePaidProgramRequestV3 {
    pub version: u16,
    pub header: Vec<u8>,
    pub rows: Vec<(u8, Vec<u8>, Vec<u8>)>,
    pub ledger_root: Hash64,
    pub aux_root: Hash64,
    pub base_job: Digest,
    pub paid_class: Digest,
    pub paid_job: Digest,
    pub material_consent: Digest,
    pub not_before_daa: u64,
    pub salt: Digest,
    pub producer_card: u32,
    pub parameters: BTreeMap<(u16, Option<u16>), TensorWireV1>,
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
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())
}
/// Export real producer work and its native signed new enrollment to a fresh
/// directory. Monetary/provider enrollment, source locks, assigned verifier
/// receipts and Final are obtained through later live CLI steps, never invented.
pub fn export_private_paid_program_work_v3(config: &Config, request_file: &Path, output: &Path) -> Result<serde_json::Value, String> {
    let salt = config.palw_drill_genesis_salt.as_ref().ok_or("paid private work requires explicit salt")?;
    if config.private_population.is_some() || config.params.skip_proof_of_work {
        return Err("paid private work requires a real PoW private chain without synthetic population".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(request_file)
        .map_err(|e| e.to_string())?
        .take((256 * 1024 * 1024 + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 256 * 1024 * 1024 {
        return Err("paid work request exceeds fixed file bound".into());
    }
    let request: PrivatePaidProgramRequestV3 = borsh::from_slice(&bytes).map_err(|e| e.to_string())?;
    if request.version != 3 {
        return Err("unknown private paid work contract".into());
    }
    let route = kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1::from_served_rows_v1(
        &request.header,
        request.rows,
        &request.ledger_root,
        &request.aux_root,
    )?;
    let domain = misaka_palw_sdk::onboarding_chain::network_domain_v1(&config.params);
    if route.header.policy.network_domain != domain.as_bytes() {
        return Err("snapshot pins name another private chain".into());
    }
    let card = kaspa_consensus_core::config::drill::palw_t12_drill_cards_v1(salt)
        .into_iter()
        .nth(request.producer_card as usize)
        .ok_or("unknown private producer card")?;
    let bond = PalwBondKeyV2(kaspa_consensus_core::config::drill::palw_t12_drill_premine_outpoint_v1(salt, card.premine_index));
    let producer = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond);
    let key = kaspa_pq_validator_core::ValidatorKey::from_seed(card.bond.seed);
    let work = produce_program_work_v3(&route, request.base_job, producer, request.parameters).map_err(|e| e.to_string())?;
    let enrollment = work
        .signed_enrollment(
            &route,
            request.paid_class,
            request.paid_job,
            request.salt,
            request.material_consent,
            request.not_before_daa,
            &key,
            domain,
            bond,
        )
        .map_err(|e| e.to_string())?;
    std::fs::create_dir(output).map_err(|e| format!("fresh paid work directory: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(output, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    write_new(&output.join("program-work-v3.borsh"), &borsh::to_vec(&work).map_err(|e| e.to_string())?)?;
    write_new(&output.join("salted-commit-v1.borsh"), &borsh::to_vec(&work.commit).map_err(|e| e.to_string())?)?;
    write_new(&output.join("public-material-v2.borsh"), &borsh::to_vec(&work.material).map_err(|e| e.to_string())?)?;
    write_new(&output.join("signed-paid-enrollment-v2.borsh"), &borsh::to_vec(&enrollment).map_err(|e| e.to_string())?)?;
    let manifest = serde_json::json!({"schema":"misaka.private-paid-program-work.v3","baseJob":faster_hex::hex_string(&request.base_job),
        "paidClass":faster_hex::hex_string(&request.paid_class),"paidJob":faster_hex::hex_string(&request.paid_job),
        "ledgerRoot":request.ledger_root.to_string(),"auxRoot":request.aux_root.to_string(),
        "producer":faster_hex::hex_string(&producer),"work":"program-work-v3.borsh","material":"public-material-v2.borsh",
        "signedEnrollment":"signed-paid-enrollment-v2.borsh","realTirExecution":true,
        "privateWorkspaceBoundBytes":work.resource_bound.workspace_bytes.to_string(),
        "privateMaterialBoundBytes":work.resource_bound.material_bytes.to_string(),
        "privateExecutionWorkBound":work.resource_bound.execution_work.to_string(),
        "resourceApproval":false,"nativeEnrollmentIncluded":false,
        "sourceLocked":false,"receiptQuorumIncluded":false,"nativeFinalObserved":false,"releaseApproval":false});
    write_new(&output.join("manifest.json"), &serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?)?;
    Ok(manifest)
}
