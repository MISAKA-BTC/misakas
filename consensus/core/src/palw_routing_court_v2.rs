//! Versioned native per-reader routing accusation. Merkle evidence authenticates data;
//! deterministic local recomputation decides the offence. A matching result
//! never supplies randomly selected validator approval or consensus credit.
use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOpeningV1, PalwArtifactOperandV1, PalwProvenOperandsV1};
use crate::palw_native_resource_v1::{NativeHeapV1, TirNativeRequestsV1, reserve_native_requests_v1};
use crate::palw_shard_court_v1::{PalwOneMoveClaimV2, PalwShardCourtVerdictV1};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step_refute::{
    PalwStepRefuteError,
    routing_v2::{PalwRoutedStepRefutationV2, PalwRoutingCourtErrorV2, check_routed_step_refutation_v2},
};

pub const PALW_ROUTING_COURT_VERSION_V2: u16 = 2;
pub const PALW_ROUTING_COURT_SESSION_DOMAIN_V2: &[u8] = b"misaka-palw/routing-court/session/v2";
/// Same existing objective-accuser signature role, a different versioned
/// message domain. This requires no uncommitted signature context set; an old
/// shard-court signature cannot sign the new domain's message by replay.
pub const PALW_ROUTING_COURT_MLDSA87_CONTEXT_V2: &[u8] = crate::palw_shard_court_v1::PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT;

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwRoutingCourtAccusationV2 {
    pub version: u16,
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    pub accuser_bond: PalwBondKeyV2,
    pub leaf_index: u64,
    pub refutation: PalwRoutedStepRefutationV2,
    pub artifact_openings: Vec<PalwArtifactOpeningV1>,
    pub signature: Vec<u8>,
}

impl PalwRoutingCourtAccusationV2 {
    /// Only framing/identity checks; no arithmetic or reserved workspace is
    /// consumed by the String-only outer admission walk. The typed fold always
    /// runs the full terminal and preserves local resource refusal separately.
    pub fn validate_shape(&self, ladder: u64) -> Result<(), &'static str> {
        if self.version != PALW_ROUTING_COURT_VERSION_V2 || self.refutation.version != 2 {
            return Err("unsupported routing court/evidence version");
        }
        if self.leaf_index >= ladder || self.refutation.step.output_opening.leaf_index != self.leaf_index {
            return Err("routing accusation leaf does not match its opening/ladder");
        }
        if self.executor_bond == self.accuser_bond {
            return Err("routing accusation accuser is its executor");
        }
        if self.refutation.step.binding.committed_execution_root != self.execution_root {
            return Err("routing accusation binding names another execution");
        }
        Ok(())
    }
    /// Count the complete accusation, including identity and signature framing,
    /// against the existing close ceiling. No temporary canonical Vec needed.
    pub fn encoded_bytes(&self) -> Result<u64, &'static str> {
        borsh::object_length(self).map(|n| n as u64).map_err(|_| "routing accusation length cannot be represented")
    }
}

pub fn palw_routing_court_session_id_v2(network: Hash64, a: &PalwRoutingCourtAccusationV2) -> Hash64 {
    struct Stream(blake2b_simd::State);
    impl std::io::Write for Stream {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut stream = Stream(blake2b_simd::Params::new().hash_length(64).key(PALW_ROUTING_COURT_SESSION_DOMAIN_V2).to_state());
    // Only fixed-size fields are encoded into the hash stream, never a copied
    // refutation/signature Vec. The evidence is bound by the committed roots.
    borsh::to_writer(
        &mut stream,
        &(network, a.version, a.claim, a.execution_root, a.trace_root, a.executor_bond, a.accuser_bond, a.leaf_index),
    )
    .expect("hash stream of fixed owned fields is infallible");
    Hash64::from_bytes(stream.0.finalize().as_bytes().try_into().expect("64-byte hash"))
}

pub fn palw_routing_court_verdict_v2(
    accusation: &PalwRoutingCourtAccusationV2,
    claim: &PalwOneMoveClaimV2,
    ladder: u64,
    declaration: &crate::palw_reader_router_v2::PalwReaderRouterDeclarationV2,
) -> Result<PalwShardCourtVerdictV1, PalwRoutingCourtErrorV2> {
    let refused = |message| PalwRoutingCourtErrorV2::Evidence(PalwStepRefuteError::InputSetNotCanonical(message));
    accusation.validate_shape(ladder).map_err(refused)?;
    if declaration.class_id != claim.class_id || declaration.artifact_root != claim.artifact_root {
        return Err(refused("routing declaration is not the native claim's class/artifact"));
    }
    if accusation.execution_root != claim.execution_root {
        return Err(refused("routing accusation is not the actual native claim's execution"));
    }
    // ProvenOperands clones only the opened operand records. Reserve their
    // actual typed ownership before cloning; membership hashing is streaming.
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<PalwArtifactOperandV1>(accusation.artifact_openings.len() as u64)?;
    for opening in &accusation.artifact_openings {
        let heap = opening.operand.native_heap_v1("routing court authenticated artifact operand clone");
        q.add_v1(heap.bytes().ok_or(heap)?)?;
        q.vec_v1::<u8>((opening.operand.bytes.len() as u64 + opening.operand.tensor_name.len() as u64 + 128).max(256))?;
    }
    let _operands = reserve_native_requests_v1(q.bytes_v1())?;
    let proven = PalwProvenOperandsV1::from_openings_v1(&accusation.artifact_openings, claim.artifact_root)
        .map_err(|_| refused("routing artifact operands do not belong to the registered class artifact"))?;
    match check_routed_step_refutation_v2(&accusation.refutation, claim.execution_root, declaration, &proven, ladder) {
        Ok(_) => Ok(PalwShardCourtVerdictV1::ExecutorGuilty),
        Err(PalwRoutingCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound)) => Ok(PalwShardCourtVerdictV1::FalseAccusation),
        Err(other) => Err(other),
    }
}
