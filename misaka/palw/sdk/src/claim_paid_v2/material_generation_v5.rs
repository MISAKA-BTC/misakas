//! Explicitly re-sign a replacement binding, preserving the original signed
//! V2 commit. Callers must publish a NEW WAL with separately declared funding;
//! neither this builder nor transport success settles any fees or approvals.
use kaspa_consensus_core::{
    Hash64,
    palw_kernel_route_v1::{
        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, PalwKernelRouteStateV1, palw_kernel_bond_id_v1, palw_kernel_route_message_v1,
    },
    palw_native_resource_v1::{
        NativeRetainedV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_paid_material_generation_v5::{original_paid_commit_message_v5, paid_material_generation_from_route_v5},
    palw_state_v2::PalwConsensusObjectV2,
};
use misaka_palw_kernel::{claim_paid_v2::PaidClaimObjectV2, route::KernelRouteObjectV1};

/// This builder accepts an exact old signed V2 Commit only. It refuses any
/// already accepted base claim so an accepted source/anchor/seat snapshot is
/// monitored, never re-enrolled or re-funded under a different generation.
pub fn paid_claim_material_generation_object_v5(
    route: &PalwKernelRouteStateV1,
    original: &PalwConsensusObjectV2,
    producer_pubkey: &[u8],
) -> Result<NativeRetainedV1<KernelRouteObjectV1>, String> {
    let PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, signature } = original else {
        return Err("V5 original WAL is not a signed native kernel object".into());
    };
    let network = Hash64::from_bytes(route.header.policy.network_domain);
    if !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
        producer_pubkey,
        palw_kernel_route_message_v1(network, signer, bytes).as_byte_slice(),
        signature,
        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
    ) {
        return Err("V5 original WAL signature/network/producer differs".into());
    }
    let raw = bytes.get(1..).ok_or("V5 original route version absent")?;
    preflight_native_value_v1::<KernelRouteObjectV1>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
    let decoded = retain_native_value_decode_v1(raw, || KernelRouteObjectV1::decode(bytes)).map_err(|e| e.to_string())?;
    let KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::Commit { enrollment, commit, salt } } = &*decoded else {
        return Err("V5 refuses a wrapper/nested object or non-original paid commit".into());
    };
    if enrollment.producer != palw_kernel_bond_id_v1(signer) || commit.producer() != enrollment.producer {
        return Err("V5 original WAL changes its actor".into());
    }
    if route.paid_claim_for_base_v2(&enrollment.base_claim)?.is_some() {
        return Err("V5 base already accepted; retain its source/anchor/checker snapshot and monitor".into());
    }
    let expected = original_paid_commit_message_v5(network, signer, enrollment, commit, salt)?;
    if expected != palw_kernel_route_message_v1(network, signer, bytes) {
        return Err("V5 original canonical signed bytes differ".into());
    }
    let generation = paid_material_generation_from_route_v5(route, enrollment, *signer).map_err(|e| e.to_string())?;
    let _assembly = reserve_native_requests_v1(
        (std::mem::size_of::<KernelRouteObjectV1>() as u64)
            .checked_add(signature.len() as u64)
            .ok_or("V5 wrapper assembly overflow")?,
    )
    .map_err(|e| e.to_string())?;
    let (decoded, _decoded_owner) = decoded.into_parts_v1();
    let KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::Commit { enrollment, commit, salt } } = decoded else {
        unreachable!("original commit shape checked")
    };
    NativeRetainedV1::retain_existing_v1(KernelRouteObjectV1::PaidClaimV2 {
        object: PaidClaimObjectV2::CommitWithMaterialGenerationV5 {
            enrollment,
            commit,
            salt,
            generation,
            original_signature: signature.clone(),
        },
    })
    .map_err(|e| e.to_string())
}
