//! Offline preparation of the explicit mixed-court candidate registration.
//! The consumer authenticates the matured binding, charges the signing actor,
//! and independently rederives the scope. This metadata grants no execution,
//! fidelity, availability, reward eligibility, or Final rights.
use kaspa_consensus_core::{
    Hash64,
    palw_kernel_route_v1::{PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, palw_kernel_route_message_v1},
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
};
use kaspa_pq_validator_core::MessageSigner;
use misaka_palw_kernel::{
    hash::Digest,
    ledger::flat_candidate7::{FlatCandidatePlanV7, FlatCandidateRecordV7, derive_flat_candidate_v7},
    route::KernelRouteObjectV1 as K,
    trace::ParamCommitmentsV1,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedFlatCandidateV7 {
    pub class: Digest,
    pub plan: FlatCandidatePlanV7,
    pub object: K,
}
pub fn prepare_flat_candidate_v7(
    binding: Digest,
    descriptor: Digest,
    program_bytes: &[u8],
    params: &ParamCommitmentsV1,
    max_positions: u32,
) -> Result<PreparedFlatCandidateV7, String> {
    let view = derive_flat_candidate_v7(&descriptor, program_bytes, params, max_positions)?;
    let record = FlatCandidateRecordV7 {
        binding,
        plan: view.plan.clone(),
        program_bytes: program_bytes.to_vec(),
        param_commitments: params.clone(),
    };
    let object = K::RegisterFlatModelCandidateV7 {
        binding,
        descriptor,
        program_bytes: program_bytes.to_vec(),
        param_commitments: params.clone(),
        max_positions,
        scope_root: view.plan.scope_root,
        replay_root: view.plan.replay_root,
    };
    if object.encoded_len() > object.max_encoded_bytes() {
        return Err("flat-v7 candidate exceeds its route carrier".into());
    }
    Ok(PreparedFlatCandidateV7 { class: record.id(), plan: view.plan, object })
}
/// Recheck the declaration before signing with the existing network-specific
/// route message. A supplied binding id is not authenticated by this helper.
pub fn sign_flat_candidate_v7(
    key: &dyn MessageSigner,
    domain: Hash64,
    bond: PalwBondKeyV2,
    object: &K,
) -> Result<PalwConsensusObjectV2, String> {
    let K::RegisterFlatModelCandidateV7 { binding, descriptor, program_bytes, param_commitments, max_positions, .. } = object else {
        return Err("flat-v7 signer requires candidate registration tag 40".into());
    };
    if prepare_flat_candidate_v7(*binding, *descriptor, program_bytes, param_commitments, *max_positions)?.object != *object {
        return Err("flat-v7 declaration differs from its independent scope/replay".into());
    }
    let bytes = object.encode();
    let message = palw_kernel_route_message_v1(domain, &bond, &bytes);
    K::decode(&bytes).map_err(|e| e.to_string())?;
    let signature = key.sign_message(message.as_byte_slice(), PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1)?;
    let signed = PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond, signature };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&signed).map_err(str::to_string)?;
    Ok(signed)
}
