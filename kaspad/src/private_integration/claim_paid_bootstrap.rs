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
    let mut file = std::fs::File::open(request_file).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    if length > 64 * 1024 * 1024 {
        return Err("paid work request exceeds 64 MiB private frame bound".into());
    }
    let mut bytes = vec![0; length as usize];
    file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    let mut trailing = [0];
    if file.read(&mut trailing).map_err(|e| e.to_string())? != 0 {
        return Err("paid work request changed length during bounded read".into());
    }
    preflight_request_v3(&bytes)?;
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
    std::fs::File::open(output).and_then(|d| d.sync_all()).map_err(|e| format!("paid work directory sync: {e}"))?;
    Ok(manifest)
}

/// Exact schema walk before Borsh allocates any request-owned collection. Each
/// row uses the native codec scanner, including nested class/program and aux
/// rows. This bounds restoration; root membership and execution workspace are
/// checked separately after restoration by the pinned route and producer.
fn preflight_request_v3(bytes: &[u8]) -> Result<(), String> {
    use kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteHeaderV1;
    use kaspa_consensus_core::palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, preflight_kernel_served_row_v1};
    use std::io::Read;
    fn blob<'a>(r: &mut &'a [u8], cap: usize) -> Result<&'a [u8], String> {
        let mut word = [0; 4];
        r.read_exact(&mut word).map_err(|e| e.to_string())?;
        let n = u32::from_le_bytes(word) as usize;
        if n > cap || n > r.len() {
            return Err("private request field exceeds exact frame/cap".into());
        }
        let (value, tail) = r.split_at(n);
        *r = tail;
        Ok(value)
    }
    fn take<'a>(r: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
        if n > r.len() {
            return Err("truncated private request".into());
        }
        let (value, tail) = r.split_at(n);
        *r = tail;
        Ok(value)
    }
    fn word(r: &mut &[u8]) -> Result<usize, String> {
        Ok(u32::from_le_bytes(take(r, 4)?.try_into().unwrap()) as usize)
    }
    let mut r = bytes;
    if take(&mut r, 2)? != 3u16.to_le_bytes() {
        return Err("unknown private request version".into());
    }
    let header = blob(&mut r, 1024 * 1024)?;
    let mut scan = NativeWireReaderV1::new(header, header.len() as u64);
    let shape = PalwKernelRouteHeaderV1::scan_native_wire_v1(&mut scan).map_err(|e| e.to_string())?;
    if scan.remaining() != 0 {
        return Err("private route header has trailing codec bytes".into());
    }
    let mut heap = u128::from(shape.heap) + bytes.len() as u128;
    let count = word(&mut r)?;
    if count > 65_536 {
        return Err("private request row count exceeds 65536".into());
    }
    // One request Vec tuple and one native BTree node plus Vec metadata/key
    // copies. Row-specific decoding allocations are added by its exact scanner.
    heap += count as u128 * 512;
    for _ in 0..count {
        let table = take(&mut r, 1)?[0];
        let key = blob(&mut r, 137)?;
        let row = blob(&mut r, 32 * 1024 * 1024)?;
        let shape = preflight_kernel_served_row_v1(table, key, row).map_err(|e| e.to_string())?;
        heap = heap
            .checked_add(u128::from(shape.heap) + 2 * (key.len() + row.len()) as u128)
            .ok_or("private decode heap arithmetic overflow")?;
        if heap > 256 * 1024 * 1024 {
            return Err("private restored request exceeds 256 MiB decode workspace".into());
        }
    }
    take(&mut r, 7 * 64 + 8 + 4)?; // roots, job/class/enrollment terms, anchor, card
    let tensors = word(&mut r)?;
    if tensors > 16_384 {
        return Err("private artifact map exceeds 16384 instances".into());
    }
    let mut previous = None;
    for _ in 0..tensors {
        let param = u16::from_le_bytes(take(&mut r, 2)?.try_into().unwrap());
        let layer = match take(&mut r, 1)?[0] {
            0 => None,
            1 => Some(u16::from_le_bytes(take(&mut r, 2)?.try_into().unwrap())),
            _ => return Err("private artifact has invalid option tag".into()),
        };
        if previous.is_some_and(|p| p >= (param, layer)) {
            return Err("private artifact keys are not strictly canonical".into());
        }
        previous = Some((param, layer));
        let dtype = misaka_palw_kernel::plan::dtype_of_tag(take(&mut r, 1)?[0]).ok_or("unknown tensor dtype")?;
        let rank = word(&mut r)?;
        if rank > 12 {
            return Err("private tensor rank exceeds 12".into());
        }
        let mut elements = 1u128;
        for _ in 0..rank {
            let d = u64::from_le_bytes(take(&mut r, 8)?.try_into().unwrap());
            if d == 0 {
                return Err("private tensor has zero axis".into());
            }
            elements = elements.checked_mul(u128::from(d)).ok_or("private tensor shape overflow")?;
        }
        let data = blob(&mut r, 32 * 1024 * 1024)?;
        if elements.checked_mul(dtype.width() as u128) != Some(data.len() as u128) {
            return Err("private tensor shape/dtype does not match exact data bytes".into());
        }
        heap = heap
            .checked_add(elements.checked_mul(16).ok_or("private tensor decoded heap overflow")? + data.len() as u128 + 512)
            .ok_or("private tensor aggregate heap overflow")?;
        if heap > 256 * 1024 * 1024 {
            return Err("private artifact and restored rows exceed 256 MiB decode workspace".into());
        }
    }
    if !r.is_empty() {
        return Err("private request has trailing bytes".into());
    }
    Ok(())
}
