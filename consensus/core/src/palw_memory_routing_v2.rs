//! **Object tag 170: RFC-0006 memory/routing consumer v2's other carriers** — owned by `palw_memory_routing_court_v2` beside Codex's
//! tag 243 (`GdnStepAccusedV2`, the anchored recurrence STEP accusation, `crate::palw_gdn_court_v2`).
//!
//! One tag, one owning fence, two kinds of payload:
//!
//! * **`RouterBindingDeclared`** — the class registrant's immutable per-reader router binding (`crate::palw_router_binding_v2`),
//!   stored once in kernel-route aux table 139 and read for claims accepted at or after its block.
//! * **`Accused`** — a one-move accusation through one of consumer v2's terminals, adjudicated in the typed fold exactly as tag 243
//!   is (an `ExecutorGuilty` voids the claim and slashes its executor; a `FalseAccusation` charges the accuser what the one-move
//!   court charges):
//!   * `GdnTransition` — a recurrence checkpoint whose delta state is not its exact predecessor's deterministic replay
//!     (`memory_v2::check_gdn_transition_refutation_v2`). An anchored step accusation stands on a checkpoint; this is how a forged
//!     checkpoint is convicted instead of anchoring an acquittal.
//!   * `GdnConvWindow` — a recurrence checkpoint whose convolution window is not the committed projection rows
//!     (`memory_v2::check_gdn_conv_window_refutation_v2`).
//!   * `RoutedStep` — a routed reader's step adjudicated under its class's bound router
//!     (`palw_step_refute::check_execution_step_refutation_routed_v2`); refused for a reader the binding does not name, or a claim
//!     accepted before the binding's block.
//!
//! Below the fence every tag-170 object is the live build's undecodable payload (A-2: skipped, charged nothing). Hashes, Merkle
//! paths and signatures here authenticate data identity, membership and authorship only; every verdict is deterministic
//! recomputation (PRINCIPLES §8), and a matching recomputation confers no validator approval or consensus credit.
use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOpeningV1, PalwArtifactOperandV1, PalwProvenOperandsV1};
use crate::palw_native_resource_v1::{NativeHeapV1, TirNativeRequestsV1, reserve_native_requests_v1};
use crate::palw_router_binding_v2::{PalwRouterBindingDeclarationV2, PalwRouterBindingRowV2, palw_bound_router_slot_v2};
use crate::palw_shard_court_v1::{PalwOneMoveClaimV2, PalwShardCourtVerdictV1};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step_refute::{
    PalwExecutionStepRefutationV1, PalwStepRefuteError,
    memory_v2::{
        PalwGdnConvWindowRefutationV2, PalwGdnCourtErrorV2, PalwGdnTransitionRefutationV2, check_gdn_conv_window_refutation_v2,
        check_gdn_transition_refutation_v2,
    },
};

/// The object tag this module's carrier rides at (the lane's allocation; `PALW_A2_TAG_ALLOCATIONS_V1`).
pub const PALW_MEMORY_ROUTING_OBJECT_TAG_V2: u8 = 170;
pub const PALW_MEMORY_ROUTING_ACCUSATION_VERSION_V2: u16 = 2;
pub const PALW_MEMORY_ROUTING_SESSION_DOMAIN_V2: &[u8] = b"misaka-palw/memory-routing-court/session/v2";
/// The accuser's signature role is the existing objective one-move accuser's; the versioned session domain above keeps an old
/// shard-court or tag-243 signature from being replayed onto this message.
pub const PALW_MEMORY_ROUTING_MLDSA87_CONTEXT_V2: &[u8] = crate::palw_shard_court_v1::PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT;

/// **The bounded adjudication cost of each public refutation route this carrier serves** (RFC-0007 item 1's "a named constant per
/// route"). The structural bound every route answers to today is the existing close-byte ceiling (`PalwCourtParamsV2::
/// max_close_bytes`, checked on the encoded accusation) plus the typed before-allocation workspace each terminal reserves; the
/// verifier-time / fee figures below are **UNKNOWN until measured** and are not read by any rule. `None` is UNKNOWN.
///
/// UNKNOWN: the routing route (a routed reader's bound router row) verifier time.
pub const PALW_ROUTE_COST_ROUTING_US_V2: Option<u64> = None;
/// UNKNOWN: the history route (recurrence checkpoint transition / convolution window, KV checkpoint rows) verifier time.
pub const PALW_ROUTE_COST_HISTORY_US_V2: Option<u64> = None;
/// UNKNOWN: the expert/index gather route verifier time (the routed reader under a binding).
pub const PALW_ROUTE_COST_GATHER_US_V2: Option<u64> = None;

/// What a tag-170 accusation claims, and the evidence that decides it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwMemoryRoutingEvidenceV2 {
    GdnTransition { refutation: PalwGdnTransitionRefutationV2 } = 0,
    GdnConvWindow { refutation: PalwGdnConvWindowRefutationV2 } = 1,
    RoutedStep {
        refutation: PalwExecutionStepRefutationV1,
        /// ADR-0081 Decision 3's prompt tile, where the network's form is Merkle and the reader gathers a prompt id; otherwise `None`.
        prompt_ids_opening: Option<crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1>,
    } = 2,
}

impl PalwMemoryRoutingEvidenceV2 {
    /// The binding the evidence is about.
    pub fn binding(&self) -> &crate::palw_step_leg::PalwStepBindingV2 {
        match self {
            Self::GdnTransition { refutation } => &refutation.binding,
            Self::GdnConvWindow { refutation } => &refutation.binding,
            Self::RoutedStep { refutation, .. } => &refutation.binding,
        }
    }

    /// `(route discriminant, subject)`: the accused checkpoint index for the two checkpoint routes, the opened leaf for a step.
    pub fn subject(&self) -> (u8, u64) {
        match self {
            Self::GdnTransition { refutation } => (0, u64::from(refutation.accused.leaf.checkpoint_index)),
            Self::GdnConvWindow { refutation } => (1, u64::from(refutation.accused.leaf.checkpoint_index)),
            Self::RoutedStep { refutation, .. } => (2, refutation.output_opening.leaf_index),
        }
    }
}

/// **A one-move accusation through consumer v2** — the shard court's framing (claim, roots, both bonds, the accuser's signature),
/// with consumer v2's evidence.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwMemoryRoutingAccusationV2 {
    pub version: u16,
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    pub accuser_bond: PalwBondKeyV2,
    pub evidence: PalwMemoryRoutingEvidenceV2,
    /// The class artifact's operand openings the terminal reads (Q36 recurrence triples, routed expert stores); empty where none.
    pub artifact_openings: Vec<PalwArtifactOpeningV1>,
    pub signature: Vec<u8>,
}

impl PalwMemoryRoutingAccusationV2 {
    /// Framing/identity checks only (the string-only acceptance walk); the typed fold runs the terminal.
    pub fn validate_shape(&self, ladder: u64) -> Result<(), &'static str> {
        if self.version != PALW_MEMORY_ROUTING_ACCUSATION_VERSION_V2 {
            return Err("unsupported memory/routing accusation version");
        }
        if self.executor_bond == self.accuser_bond {
            return Err("memory/routing accusation accuser is its executor");
        }
        if self.evidence.binding().committed_execution_root != self.execution_root {
            return Err("memory/routing accusation binding names another execution");
        }
        if let PalwMemoryRoutingEvidenceV2::RoutedStep { refutation, .. } = &self.evidence
            && refutation.output_opening.leaf_index >= ladder
        {
            return Err("routed accusation leaf is past the ladder");
        }
        Ok(())
    }

    /// The whole accusation's encoded length, against the existing close ceiling.
    pub fn encoded_bytes(&self) -> Result<u64, &'static str> {
        borsh::object_length(self).map(|n| n as u64).map_err(|_| "memory/routing accusation length cannot be represented")
    }
}

/// **Tag 170's payload.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwMemoryRoutingObjectV2 {
    /// The class registrant's immutable per-reader router binding, signed by the class's registrant bond under
    /// `PALW_ROUTER_BINDING_SIGNATURE_CONTEXT_V2` over `palw_router_binding_message_v2`.
    RouterBindingDeclared { declaration: PalwRouterBindingDeclarationV2, signature: Vec<u8> } = 0,
    /// A one-move accusation through consumer v2.
    Accused { accusation: PalwMemoryRoutingAccusationV2 } = 1,
}

impl PalwMemoryRoutingObjectV2 {
    /// The string-only acceptance shape check, against the existing close ceiling for an accusation.
    pub fn may_ride(&self, max_close_bytes: u64) -> Result<(), &'static str> {
        match self {
            Self::RouterBindingDeclared { declaration, signature } => {
                if signature.is_empty() {
                    return Err("a router binding declaration must carry its registrant's signature");
                }
                if declaration.readers.is_empty() {
                    return Err("a router binding names its routed readers");
                }
                Ok(())
            }
            Self::Accused { accusation } => {
                if accusation.signature.is_empty() {
                    return Err("a memory/routing accusation must carry its accuser's signature");
                }
                if accusation.encoded_bytes()? > max_close_bytes {
                    return Err("memory/routing accusation exceeds the close-byte ceiling");
                }
                accusation.validate_shape(u64::MAX)
            }
        }
    }
}

/// The session id the accuser signs: the network, every field naming what is accused, and the route and subject. The evidence is
/// bound by its own roots.
pub fn palw_memory_routing_session_id_v2(network: Hash64, a: &PalwMemoryRoutingAccusationV2) -> Hash64 {
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
    let mut stream = Stream(blake2b_simd::Params::new().hash_length(64).key(PALW_MEMORY_ROUTING_SESSION_DOMAIN_V2).to_state());
    let (route, subject) = a.evidence.subject();
    borsh::to_writer(
        &mut stream,
        &(network, a.version, a.claim, a.execution_root, a.trace_root, a.executor_bond, a.accuser_bond, route, subject),
    )
    .expect("hash stream of fixed owned fields is infallible");
    Hash64::from_bytes(stream.0.finalize().as_bytes().try_into().expect("64-byte hash"))
}

/// **The one move, adjudicated.** `binding_row` is the chain's aux-139 row for the claim's class (read by the fold), `accepted_daa`
/// the claim's acceptance DAA. A routed step under no binding, or under a binding declared after the claim was accepted, is refused
/// — never adjudicated under the legacy router or a different one.
pub fn palw_memory_routing_verdict_v2(
    accusation: &PalwMemoryRoutingAccusationV2,
    claim: &PalwOneMoveClaimV2,
    ladder: u64,
    binding_row: Option<&PalwRouterBindingRowV2>,
    accepted_daa: u64,
) -> Result<PalwShardCourtVerdictV1, PalwGdnCourtErrorV2> {
    let refused = |message| PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::InputSetNotCanonical(message));
    accusation.validate_shape(ladder).map_err(refused)?;
    if accusation.execution_root != claim.execution_root {
        return Err(refused("memory/routing accusation is not the actual native claim's execution"));
    }
    // ProvenOperands clones the opened operand records: reserve their typed ownership before cloning.
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<PalwArtifactOperandV1>(accusation.artifact_openings.len() as u64)?;
    for opening in &accusation.artifact_openings {
        let heap = opening.operand.native_heap_v1("memory/routing court authenticated artifact operand clone");
        q.add_v1(heap.bytes().ok_or(heap)?)?;
    }
    let _operands = reserve_native_requests_v1(q.bytes_v1())?;
    let proven = PalwProvenOperandsV1::from_openings_v1(&accusation.artifact_openings, claim.artifact_root)
        .map_err(|_| refused("memory/routing artifact operands do not belong to the registered class artifact"))?;
    let outcome: Result<(), PalwGdnCourtErrorV2> = match &accusation.evidence {
        PalwMemoryRoutingEvidenceV2::GdnTransition { refutation } => {
            check_gdn_transition_refutation_v2(refutation, claim.execution_root, claim.class_id, &proven, ladder).map(|_| ())
        }
        PalwMemoryRoutingEvidenceV2::GdnConvWindow { refutation } => {
            check_gdn_conv_window_refutation_v2(refutation, claim.execution_root, claim.class_id, ladder).map(|_| ())
        }
        PalwMemoryRoutingEvidenceV2::RoutedStep { refutation, prompt_ids_opening } => {
            let binding = &refutation.binding;
            if binding.committed_execution_root != claim.execution_root {
                return Err(refused("routed evidence names another claim execution"));
            }
            crate::palw_step_leg::verify_binding_v1(binding).map_err(|e| PalwGdnCourtErrorV2::Evidence(e.into()))?;
            if binding.shape_profile.shape_profile_id() != claim.class_id {
                return Err(refused("routed evidence is not the claim's registered class"));
            }
            let row = binding_row.ok_or_else(|| refused("the class has no router binding"))?;
            if accepted_daa < row.bound_from_daa {
                return Err(refused("the claim was accepted before its class's router binding"));
            }
            let reader = refutation.output_preimage.coord.node_slot;
            let (node, _) = binding.shape_profile.resolve_node_slot(reader).ok_or(PalwStepRefuteError::Unadjudicable)?;
            if !crate::palw_step_refute::qwen36_reads_routing_v1(node) {
                return Err(refused("the accused step is not a routed reader: the legacy one-move court serves it"));
            }
            let bound = palw_bound_router_slot_v2(&binding.shape_profile, row, reader)
                .ok_or_else(|| refused("the class's binding does not name this reader"))?;
            crate::palw_step_refute::check_execution_step_refutation_routed_v2(
                refutation,
                &proven,
                prompt_ids_opening.as_ref(),
                ladder,
                Some(bound),
            )
            .map(|_| ())
            .map_err(PalwGdnCourtErrorV2::Evidence)
        }
    };
    match outcome {
        Ok(()) => Ok(PalwShardCourtVerdictV1::ExecutorGuilty),
        Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound)) => Ok(PalwShardCourtVerdictV1::FalseAccusation),
        Err(other) => Err(other),
    }
}

#[cfg(test)]
mod tests;
