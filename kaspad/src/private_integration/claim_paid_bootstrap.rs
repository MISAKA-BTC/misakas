//! Signed new-path Program work fixture from authenticated live native rows.
//! Immutable complete-v1 exporter artifacts are never read or modified here.
use kaspa_consensus_core::{config::Config, palw_state_v2::PalwBondKeyV2};
use kaspa_hashes::Hash64;
use misaka_palw_kernel::{hash::Digest, public::TensorWireV1};
use misaka_palw_sdk::claim_paid_v2::all_target_bootstrap::{NativePaidProducerInputsV3, produce_native_work_with_resident_v3};
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
/// The existing V4 request bytes are shared with the live first-party generator.
pub use misaka_palw_sdk::claim_paid_v2::all_target_bootstrap::NativePaidRequestV4 as PrivatePaidNativeRequestV4;

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
    let resident = preflight_request_v4(&bytes)?;
    let request: PrivatePaidNativeRequestV4 = match bytes.get(..2) {
        Some([3, 0]) => {
            let old: PrivatePaidProgramRequestV3 = borsh::from_slice(&bytes).map_err(|e| e.to_string())?;
            PrivatePaidNativeRequestV4 {
                version: 3,
                header: old.header,
                rows: old.rows,
                ledger_root: old.ledger_root,
                aux_root: old.aux_root,
                base_job: old.base_job,
                paid_class: old.paid_class,
                paid_job: old.paid_job,
                material_consent: old.material_consent,
                not_before_daa: old.not_before_daa,
                salt: old.salt,
                producer_card: old.producer_card,
                inputs: NativePaidProducerInputsV3 {
                    parameters: old.parameters.into_iter().map(|((p, l), v)| ((0, p, l), v)).collect(),
                    ..Default::default()
                },
            }
        }
        Some([4, 0]) => borsh::from_slice(&bytes).map_err(|e| e.to_string())?,
        _ => return Err("unknown private paid request codec".into()),
    };
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
    let paid_class: misaka_palw_kernel::claim_paid_v2::PaidClaimClassV2 = route
        .aux_row(kaspa_consensus_core::palw_claim_challenge_v2::PAID_CLAIM_CLASSES_V2, &request.paid_class.to_vec())
        .ok_or("requested paid class is not in the pinned native snapshot")?;
    if paid_class.interactive_plan.as_ref().is_some_and(|p| p.version == 4) {
        return Err(kaspa_consensus_core::palw_claim_compact_v1::PALW_COMPACT_CONTRACT_RETIRED_REASON_V1.into());
    }
    if config.params.palw_paid_bootstrap_source_v6.is_some() && !paid_class.supports_approved_bootstrap_source_v6() {
        return Err("explicit V6 exporter supports only noninteractive consumers; no compact/staged namespace fallback".into());
    }
    let work = produce_native_work_with_resident_v3(&route, request.base_job, producer, request.inputs, resident)
        .map_err(|e| e.to_string())?;
    let unsigned = work
        .enrollment_draft(&route, request.paid_class, request.paid_job, request.salt, request.material_consent, request.not_before_daa)
        .map_err(|e| e.to_string())?;
    let (unsigned, enrollment_file) = if let Some(fence) = config.params.palw_paid_bootstrap_source_v6 {
        if !fence.activation.is_active(request.not_before_daa) {
            return Err("explicit V6 export requires a declared commit DAA in its scheduled activation; no legacy fallback".into());
        }
        (
            misaka_palw_sdk::claim_paid_v2::paid_claim_commit_object_approved_bootstrap_v6(&route, fence.policy.id(), unsigned)
                .map_err(|e| e.to_string())?,
            "signed-paid-enrollment-v6.borsh",
        )
    } else {
        (unsigned, "signed-paid-enrollment-v2.borsh")
    };
    let enrollment = misaka_palw_sdk::claim_paid_v2::sign_paid_claim_object_v2(&key, domain, bond, &unsigned)?;
    std::fs::create_dir(output).map_err(|e| format!("fresh paid work directory: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(output, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    write_new(&output.join("program-work-v3.borsh"), &borsh::to_vec(&work).map_err(|e| e.to_string())?)?;
    write_new(&output.join("salted-commit-v1.borsh"), &borsh::to_vec(&work.commit).map_err(|e| e.to_string())?)?;
    write_new(&output.join("public-material-v2.borsh"), &borsh::to_vec(&work.material).map_err(|e| e.to_string())?)?;
    write_new(&output.join(enrollment_file), &borsh::to_vec(&enrollment).map_err(|e| e.to_string())?)?;
    let mut manifest = serde_json::json!({"schema":"misaka.private-paid-native-work.v4","requestVersion":request.version,"nativeCommitTag":work.commit.commit_tag(),"baseJob":faster_hex::hex_string(&request.base_job),
        "paidClass":faster_hex::hex_string(&request.paid_class),"paidJob":faster_hex::hex_string(&request.paid_job),
        "ledgerRoot":request.ledger_root.to_string(),"auxRoot":request.aux_root.to_string(),
        "producer":faster_hex::hex_string(&producer),"work":"program-work-v3.borsh","material":"public-material-v2.borsh",
        "signedEnrollment":enrollment_file,"realTirExecution":true,
        "privateWorkspaceBoundBytes":work.resource_bound.workspace_bytes.to_string(),
        "privateMaterialBoundBytes":work.resource_bound.material_bytes.to_string(),
        "privateExecutionWorkBound":work.resource_bound.execution_work.to_string(),
        "resourceApproval":false,"nativeEnrollmentIncluded":false,
        "sourceLocked":false,"receiptQuorumIncluded":false,"nativeFinalObserved":false,"releaseApproval":false});
    if let Some(fence) = config.params.palw_paid_bootstrap_source_v6 {
        manifest["schema"] = serde_json::json!("misaka.private-paid-native-work.v6");
        manifest["sourceNamespace"] = serde_json::json!("approved-bootstrap-v6");
        manifest["sourcePolicyId"] = serde_json::json!(faster_hex::hex_string(&fence.policy.id()));
    }
    write_new(&output.join("manifest.json"), &serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?)?;
    std::fs::File::open(output).and_then(|d| d.sync_all()).map_err(|e| format!("paid work directory sync: {e}"))?;
    Ok(manifest)
}

/// Exact schema walk before Borsh allocates any request-owned collection. Each
/// row uses the native codec scanner, including nested class/program and aux
/// rows. This bounds restoration; root membership and execution workspace are
/// checked separately after restoration by the pinned route and producer.
fn preflight_request_v4(bytes: &[u8]) -> Result<u128, String> {
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
    let version = u16::from_le_bytes(take(&mut r, 2)?.try_into().unwrap());
    if version != 3 && version != 4 {
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
    preflight_producer_inputs(r, version, heap)
}

/// Allocation preflight for the separate live-generator producer-input file.
pub(super) fn preflight_paid_inputs_v4(bytes: &[u8]) -> Result<u128, String> {
    preflight_producer_inputs(bytes, 4, bytes.len() as u128)
}

fn preflight_producer_inputs(bytes: &[u8], version: u16, mut heap: u128) -> Result<u128, String> {
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
    let tensors = word(&mut r)?;
    if tensors > 16_384 {
        return Err("private artifact map exceeds 16384 instances".into());
    }
    let mut previous = None;
    for _ in 0..tensors {
        let program = if version == 4 { u16::from_le_bytes(take(&mut r, 2)?.try_into().unwrap()) } else { 0 };
        let param = u16::from_le_bytes(take(&mut r, 2)?.try_into().unwrap());
        let layer = match take(&mut r, 1)?[0] {
            0 => None,
            1 => Some(u16::from_le_bytes(take(&mut r, 2)?.try_into().unwrap())),
            _ => return Err("private artifact has invalid option tag".into()),
        };
        if previous.is_some_and(|p| p >= (program, param, layer)) {
            return Err("private artifact keys are not strictly canonical".into());
        }
        previous = Some((program, param, layer));
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
    if version == 4 {
        let prompt = word(&mut r)?;
        if prompt > 65_536 {
            return Err("private tiled prompt exceeds 65536 ids".into());
        }
        take(&mut r, prompt.checked_mul(4).ok_or("prompt size overflow")?)?;
        heap += prompt as u128 * 8;
        let snapshots = word(&mut r)?;
        if snapshots > 8 {
            return Err("private snapshots exceed eight roots".into());
        }
        let mut previous_root = None;
        for _ in 0..snapshots {
            let root = <[u8; 64]>::try_from(take(&mut r, 64)?).map_err(|_| "snapshot root width")?;
            if previous_root.is_some_and(|p| p >= root) {
                return Err("snapshot roots not strictly canonical".into());
            }
            previous_root = Some(root);
            let items = word(&mut r)?;
            if items > 65_536 {
                return Err("private snapshot item cap exceeded".into());
            }
            heap += items as u128 * 128;
            for _ in 0..items {
                for _ in 0..2 {
                    let n = word(&mut r)?;
                    if n > 4096 {
                        return Err("private snapshot key/payload shape cap exceeded".into());
                    }
                    take(&mut r, n * 4)?;
                    heap += n as u128 * 8;
                }
                if heap > 256 * 1024 * 1024 {
                    return Err("private snapshot decode workspace cap exceeded".into());
                }
            }
        }
    }
    if !r.is_empty() {
        return Err("private request has trailing bytes".into());
    }
    if heap > 256 * 1024 * 1024 {
        return Err("private restored request workspace cap exceeded".into());
    }
    Ok(heap)
}

#[cfg(test)]
mod request_preflight_tests {
    use super::*;
    #[test]
    fn standalone_live_inputs_are_preflighted_before_collection_allocation() {
        let empty = borsh::to_vec(&NativePaidProducerInputsV3::default()).unwrap();
        preflight_paid_inputs_v4(&empty).unwrap();
        for offset in [0, 4, 8] {
            let mut bad = empty.clone();
            bad[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(preflight_paid_inputs_v4(&bad).is_err());
        }
        for end in 0..empty.len() {
            assert!(preflight_paid_inputs_v4(&empty[..end]).is_err());
        }
        let mut trailing = empty;
        trailing.push(0);
        assert!(preflight_paid_inputs_v4(&trailing).is_err());
    }

    #[test]
    fn a_tiny_request_cannot_allocate_an_attacker_declared_header() {
        for version in [3u16, 4] {
            let mut bytes = version.to_le_bytes().to_vec();
            bytes.extend_from_slice(&u32::MAX.to_le_bytes());
            assert!(preflight_request_v4(&bytes).is_err());
        }
        assert!(preflight_request_v4(&[5, 0]).is_err(), "unknown codec");
        for n in 0..6 {
            assert!(preflight_request_v4(&[4, 0, 0, 0, 0, 0][..n]).is_err());
        }
    }
}
