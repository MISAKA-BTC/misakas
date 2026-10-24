//! Retained signed held-GDN actions. Refutation/candidate construction is an
//! input boundary here; reserve every output copy and bounded signature first.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1, reserve_native_requests_v1,
};
use std::mem::size_of;

#[derive(Debug)]
pub enum PalwGdnHeldErrorV2 {
    Resource(NativeCoverageV1),
    Operation(String),
}
impl std::fmt::Display for PalwGdnHeldErrorV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resource(e) => write!(f, "{e}"),
            Self::Operation(e) => write!(f, "{e}"),
        }
    }
}
impl From<String> for PalwGdnHeldErrorV2 {
    fn from(e: String) -> Self {
        Self::Operation(e)
    }
}
impl From<&str> for PalwGdnHeldErrorV2 {
    fn from(e: &str) -> Self {
        Self::Operation(e.into())
    }
}
impl From<NativeCoverageV1> for PalwGdnHeldErrorV2 {
    fn from(e: NativeCoverageV1) -> Self {
        Self::Resource(e)
    }
}

pub enum PalwGdnHeldNextRetainedV2 {
    Demand { index: usize, unit: PalwHeldMissingV1, object: NativeRetainedV1<PalwConsensusObjectV2> },
    Accuse { object: NativeRetainedV1<PalwConsensusObjectV2> },
    Wait,
    DefaultDue { unit: PalwHeldMissingV1 },
    Exhausted,
}
fn bounded(request: NativeCoverageV1) -> Result<u64, NativeCoverageV1> {
    request.bytes().ok_or(request)
}
fn fixed<T>() -> NativeCoverageV1 {
    NativeCoverageV1::Bounded { bytes: size_of::<T>() as u64 }
}
fn signature(signature: Vec<u8>) -> Result<Vec<u8>, PalwGdnHeldErrorV2> {
    if signature.len() > kaspa_txscript::MLDSA87_SIG_LEN || signature.capacity() > kaspa_txscript::MLDSA87_SIG_LEN {
        return Err(NativeCoverageV1::unsupported::<PalwConsensusObjectV2>(
            "held GDN signature",
            "signature exceeds native ML-DSA-87 bound",
        )
        .into());
    }
    Ok(signature)
}
fn ticket(request: NativeCoverageV1) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
    reserve_native_requests_v1(bounded(request)?)
}
pub(super) fn demand(
    claim: &PalwGdnHeldClaimV1,
    material: &Base0GdnMaterialV2<'_>,
    missing: PalwHeldMissingV1,
    accuser: &PalwGdnHeldAccuserV1<'_>,
) -> Result<NativeRetainedV1<PalwConsensusObjectV2>, PalwGdnHeldErrorV2> {
    const PATH: &str = "retained held GDN demand";
    // The existing message function creates precisely these two canonical Vecs;
    // lengths are counted without allocation, never guessed from a prefix.
    let message_bytes = borsh::object_length(&missing)
        .map_err(|e| e.to_string())?
        .checked_add(borsh::object_length(&accuser.bond).map_err(|e| e.to_string())?)
        .ok_or_else(|| NativeCoverageV1::overflow::<PalwHeldAccusationV1>(PATH))?;
    let request = fixed::<PalwConsensusObjectV2>()
        .add::<PalwHeldAccusationV1>(fixed::<PalwHeldAccusationV1>(), PATH)
        .add::<PalwHeldAccusationV1>(material.binding.native_heap_v1(PATH), PATH)
        .add::<PalwHeldAccusationV1>(NativeCoverageV1::Bounded { bytes: message_bytes as u64 }, PATH)
        .add::<PalwHeldAccusationV1>(NativeCoverageV1::Bounded { bytes: kaspa_txscript::MLDSA87_SIG_LEN as u64 }, PATH);
    let lease = ticket(request)?;
    let mut demand = PalwHeldAccusationV1 {
        version: PALW_HELD_DA_VERSION_V1,
        claim: claim.claim,
        missing,
        accuser: accuser.bond,
        binding: material.binding.clone(),
        signature: Vec::new(),
    };
    let message = palw_held_da_accusation_message_v1(accuser.network_domain.as_byte_slice(), &demand);
    demand.signature = signature(
        (accuser.sign)(message.as_byte_slice(), PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT).ok_or("no signing key for a held demand")?,
    )?;
    // Public ownership API: retain the completed value while its earlier
    // construction reservation is still live. No unchecked Core constructor
    // or detached ticket crosses this crate boundary.
    let retained = NativeRetainedV1::retain_existing_v1(PalwConsensusObjectV2::DefaultAccusedHeld { accusation: Box::new(demand) })?;
    drop(lease);
    Ok(retained)
}
pub(super) fn accusation(
    claim: &PalwGdnHeldClaimV1,
    evidence: PalwMemoryRoutingEvidenceV2,
    accuser: &PalwGdnHeldAccuserV1<'_>,
) -> Result<NativeRetainedV1<PalwConsensusObjectV2>, PalwGdnHeldErrorV2> {
    const PATH: &str = "retained held GDN accusation";
    let openings = if matches!(&evidence, PalwMemoryRoutingEvidenceV2::GdnConvWindow { .. }) { &[][..] } else { accuser.openings };
    let slots = (openings.len() as u64)
        .checked_mul(size_of::<PalwArtifactOpeningV1>() as u64)
        .ok_or_else(|| NativeCoverageV1::overflow::<PalwMemoryRoutingAccusationV2>(PATH))?;
    let mut request = fixed::<PalwConsensusObjectV2>()
        .add::<PalwMemoryRoutingAccusationV2>(fixed::<PalwMemoryRoutingObjectV2>(), PATH)
        .add::<PalwMemoryRoutingAccusationV2>(evidence.native_heap_v1(PATH), PATH)
        .add::<PalwMemoryRoutingAccusationV2>(NativeCoverageV1::Bounded { bytes: slots }, PATH)
        .add::<PalwMemoryRoutingAccusationV2>(NativeCoverageV1::Bounded { bytes: kaspa_txscript::MLDSA87_SIG_LEN as u64 }, PATH);
    for opening in openings {
        request = request.add::<PalwMemoryRoutingAccusationV2>(opening.native_heap_v1(PATH), PATH);
    }
    let lease = ticket(request)?;
    let mut accusation = PalwMemoryRoutingAccusationV2 {
        version: PALW_MEMORY_ROUTING_ACCUSATION_VERSION_V2,
        claim: claim.claim,
        execution_root: claim.execution_root,
        trace_root: claim.trace_root,
        executor_bond: claim.executor,
        accuser_bond: accuser.bond,
        evidence,
        artifact_openings: openings.to_vec(),
        signature: Vec::new(),
    };
    // The existing session helper streams only fixed fields into its hash.
    let session = palw_memory_routing_session_id_v2(accuser.network_domain, &accusation);
    accusation.signature = signature(
        (accuser.sign)(session.as_byte_slice(), PALW_MEMORY_ROUTING_MLDSA87_CONTEXT_V2)
            .ok_or("no signing key for a GDN accusation")?,
    )?;
    let retained = NativeRetainedV1::retain_existing_v1(PalwConsensusObjectV2::MemoryRoutingV2 {
        object: Box::new(PalwMemoryRoutingObjectV2::Accused { accusation }),
    })?;
    drop(lease);
    Ok(retained)
}
