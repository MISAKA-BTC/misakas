//! Canonical identity of a single original's immutable service promises.
//! Physical assignments and observed supply change during recovery; neither
//! participates in this identity. No allocation or model-access proof occurs.

use super::{Tfp1PublicOriginalObligationV1, native_frame::PublicTfp1FrameV1};
use crate::{
    Hash64,
    palw_court_service_release_v1::CourtServiceRowV3,
    palw_court_service_state_v2::ServiceClaimBinding,
    palw_court_service_v1::{ServicePacket, causal::CausalServicePacketV1},
};
use misaka_palw_positive_contract::{ContractError, identity::Digest, public_one_honest::OriginalId};

fn begin(original: OriginalId, obligation: Digest, daa: u64, blue: u64, count: usize) -> Result<blake2b_simd::State, ContractError> {
    if count == 0 || original.0 == Digest([0; 64]) || obligation == Digest([0; 64]) {
        return Err(ContractError::Scope);
    }
    let mut hash = blake2b_simd::Params::new().hash_length(64).key(b"misaka/tfp1/service-graph/v1").to_state();
    hash.update(&original.0.0);
    hash.update(&obligation.0);
    hash.update(&daa.to_le_bytes());
    hash.update(&blue.to_le_bytes());
    hash.update(&(count as u64).to_le_bytes());
    Ok(hash)
}

fn packet(hash: &mut blake2b_simd::State, packet: &ServicePacket, parents: &[[u8; 64]]) -> Result<(), ContractError> {
    if parents.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(ContractError::NonCanonical);
    }
    hash.update(&packet.id);
    for n in [packet.release_daa, packet.service_before, packet.cost.bytes, packet.cost.work, packet.cost.io, packet.cost.controls] {
        hash.update(&n.to_le_bytes());
    }
    hash.update(&(parents.len() as u64).to_le_bytes());
    for parent in parents {
        hash.update(parent);
    }
    Ok(())
}

fn finish(hash: blake2b_simd::State) -> Hash64 {
    Hash64::from_bytes(hash.finalize().as_bytes().try_into().expect("64-byte service graph identity"))
}

pub(super) fn protected_graph_identity_v1(
    original: OriginalId,
    obligation: Tfp1PublicOriginalObligationV1,
    daa: u64,
    blue: u64,
    graph: &[CausalServicePacketV1],
) -> Result<Hash64, ContractError> {
    if graph.windows(2).any(|pair| pair[0].packet.id >= pair[1].packet.id) {
        return Err(ContractError::NonCanonical);
    }
    let mut hash = begin(original, obligation.obligation_v1(), daa, blue, graph.len())?;
    for node in graph {
        packet(&mut hash, &node.packet, &node.predecessors)?;
    }
    Ok(finish(hash))
}

/// Verify the exact graph without including `binding.allocation_id`, which
/// names the frame and would introduce a circular frame/graph hash.
pub(crate) fn check_frame_service_graph_v2(
    frame: &PublicTfp1FrameV1,
    row: &CourtServiceRowV3,
    binding: &ServiceClaimBinding,
) -> Result<(), ContractError> {
    let Some(expected) = frame.service_graph_v2() else { return Ok(()) };
    frame.check_service_scope_v1(binding)?;
    let origin = frame.origin_v1();
    let mut hash = begin(
        frame.original_id_v1(),
        frame.obligation_v1(),
        origin.accepted_daa,
        origin.accepted_blue_score,
        binding.packet_ids.len(),
    )?;
    for id in &binding.packet_ids {
        let promised = row.state.book.packet(id).ok_or(ContractError::Scope)?;
        if &promised.id != id {
            return Err(ContractError::Scope);
        }
        packet(&mut hash, promised, row.causal.get(id).map(Vec::as_slice).unwrap_or(&[]))?;
    }
    if finish(hash) != expected {
        return Err(ContractError::Identity);
    }
    Ok(())
}
