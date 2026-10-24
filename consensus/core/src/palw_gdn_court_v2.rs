//! Versioned native recurrence accusation. Merkle evidence authenticates data;
//! deterministic local recomputation decides the offence. A matching result
//! never supplies randomly selected validator approval or consensus credit.
use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOpeningV1, PalwArtifactOperandV1, PalwProvenOperandsV1};
use crate::palw_native_resource_v1::{NativeHeapV1, TirNativeRequestsV1, reserve_native_requests_v1};
use crate::palw_shard_court_v1::{PalwOneMoveClaimV2, PalwShardCourtVerdictV1};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step_refute::{
    PalwStepRefuteError,
    memory_v2::{PalwGdnCourtErrorV2, PalwGdnStepRefutationV2, check_gdn_step_refutation_v2},
};

pub const PALW_GDN_COURT_VERSION_V2: u16 = 2;
pub const PALW_GDN_COURT_SESSION_DOMAIN_V2: &[u8] = b"misaka-palw/gdn-court/session/v2";
/// Same existing objective-accuser signature role, a different versioned
/// message domain. This requires no uncommitted signature context set; an old
/// shard-court signature cannot sign the new domain's message by replay.
pub const PALW_GDN_COURT_MLDSA87_CONTEXT_V2: &[u8] = crate::palw_shard_court_v1::PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT;

/// Runtime copies of the existing V2 court limits, resolved per block. These
/// are not new numerical policy defaults and never enter old state encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGdnCourtLimitsV2 {
    pub ladder: u64,
    pub max_close_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGdnCourtAccusationV2 {
    pub version: u16,
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    pub accuser_bond: PalwBondKeyV2,
    pub leaf_index: u64,
    pub refutation: PalwGdnStepRefutationV2,
    pub artifact_openings: Vec<PalwArtifactOpeningV1>,
    pub signature: Vec<u8>,
}

/// Borrowed framing/evidence for local court probes. It is not a wire object,
/// signature carrier, or selected-verifier approval.
#[derive(Clone, Copy)]
pub struct PalwGdnCourtViewV2<'a> {
    pub version: u16,
    pub execution_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    pub accuser_bond: PalwBondKeyV2,
    pub leaf_index: u64,
    pub refutation: &'a PalwGdnStepRefutationV2,
    pub artifact_openings: &'a [PalwArtifactOpeningV1],
}
impl PalwGdnCourtViewV2<'_> {
    pub fn validate_shape(&self, ladder: u64) -> Result<(), &'static str> {
        if self.version != PALW_GDN_COURT_VERSION_V2 || self.refutation.version != 2 {
            return Err("unsupported GDN court/evidence version");
        }
        if self.leaf_index >= ladder || self.refutation.step.output_opening.leaf_index != self.leaf_index {
            return Err("GDN accusation leaf does not match its opening/ladder");
        }
        if self.executor_bond == self.accuser_bond {
            return Err("GDN accusation accuser is its executor");
        }
        if self.refutation.step.binding.committed_execution_root != self.execution_root {
            return Err("GDN accusation binding names another execution");
        }
        Ok(())
    }
}
impl PalwGdnCourtAccusationV2 {
    pub fn as_view_v2(&self) -> PalwGdnCourtViewV2<'_> {
        PalwGdnCourtViewV2 {
            version: self.version,
            execution_root: self.execution_root,
            executor_bond: self.executor_bond,
            accuser_bond: self.accuser_bond,
            leaf_index: self.leaf_index,
            refutation: &self.refutation,
            artifact_openings: &self.artifact_openings,
        }
    }
    /// The wire carrier and borrowed local probe share exactly one framing walk.
    pub fn validate_shape(&self, ladder: u64) -> Result<(), &'static str> {
        self.as_view_v2().validate_shape(ladder)
    }
    /// Count the complete accusation, including identity and signature framing,
    /// against the existing close ceiling. No temporary canonical Vec needed.
    pub fn encoded_bytes(&self) -> Result<u64, &'static str> {
        borsh::object_length(self).map(|n| n as u64).map_err(|_| "GDN accusation length cannot be represented")
    }
}

pub fn palw_gdn_court_session_id_v2(network: Hash64, a: &PalwGdnCourtAccusationV2) -> Hash64 {
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
    let mut stream = Stream(blake2b_simd::Params::new().hash_length(64).key(PALW_GDN_COURT_SESSION_DOMAIN_V2).to_state());
    // Only fixed-size fields are encoded into the hash stream, never a copied
    // refutation/signature Vec. The evidence is bound by the committed roots.
    borsh::to_writer(
        &mut stream,
        &(network, a.version, a.claim, a.execution_root, a.trace_root, a.executor_bond, a.accuser_bond, a.leaf_index),
    )
    .expect("hash stream of fixed owned fields is infallible");
    Hash64::from_bytes(stream.0.finalize().as_bytes().try_into().expect("64-byte hash"))
}

pub fn palw_gdn_court_verdict_v2(
    accusation: &PalwGdnCourtAccusationV2,
    claim: &PalwOneMoveClaimV2,
    ladder: u64,
) -> Result<PalwShardCourtVerdictV1, PalwGdnCourtErrorV2> {
    palw_gdn_court_verdict_view_v2(accusation.as_view_v2(), claim, ladder)
}

/// Same terminal as the wire entry point, borrowing existing evidence/openings.
/// The authenticated oracle still reserves its own cloned operand ownership.
pub fn palw_gdn_court_verdict_view_v2(
    accusation: PalwGdnCourtViewV2<'_>,
    claim: &PalwOneMoveClaimV2,
    ladder: u64,
) -> Result<PalwShardCourtVerdictV1, PalwGdnCourtErrorV2> {
    let refused = |message| PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::InputSetNotCanonical(message));
    accusation.validate_shape(ladder).map_err(refused)?;
    if accusation.execution_root != claim.execution_root {
        return Err(refused("GDN accusation is not the actual native claim's execution"));
    }
    // ProvenOperands clones only the opened operand records. Reserve their
    // actual typed ownership before cloning; membership hashing is streaming.
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<PalwArtifactOperandV1>(accusation.artifact_openings.len() as u64)?;
    for opening in accusation.artifact_openings {
        let heap = opening.operand.native_heap_v1("GDN court authenticated artifact operand clone");
        q.add_v1(heap.bytes().ok_or(heap)?)?;
    }
    let _operands = reserve_native_requests_v1(q.bytes_v1())?;
    let proven = PalwProvenOperandsV1::from_openings_v1(accusation.artifact_openings, claim.artifact_root)
        .map_err(|_| refused("GDN artifact operands do not belong to the registered class artifact"))?;
    match check_gdn_step_refutation_v2(accusation.refutation, claim.execution_root, claim.class_id, &proven, ladder) {
        Ok(_) => Ok(PalwShardCourtVerdictV1::ExecutorGuilty),
        Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound)) => Ok(PalwShardCourtVerdictV1::FalseAccusation),
        Err(other) => Err(other),
    }
}

impl crate::config::params::Params {
    pub fn palw_memory_routing_court_v2_fence(&self) -> Option<crate::config::params::ForkActivation> {
        let own = self.palw_memory_routing_court_v2?;
        if own == crate::config::params::ForkActivation::never() {
            return None;
        }
        let parent = self.palw_shard_court_fence()?;
        if parent == crate::config::params::ForkActivation::never() {
            return None;
        }
        Some(crate::config::params::ForkActivation::new(own.daa_score().max(parent.daa_score())))
    }
    pub fn validate_palw_memory_routing_court_v2(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        // Capture/router/resource calibration and consolidated acceptance are measured and reviewed: arming needs a pinned
        // record (EMPTY: dormant; open blockers listed there). The fence has no policy, so the record pins its activation height.
        // testnet-12 DAA 14,000 release plan.
        if let Some(own) = self.palw_memory_routing_court_v2.filter(|f| *f != crate::config::params::ForkActivation::never()) {
            self.palw_rfc0608_measured_v1(
                crate::palw_rfc0608_measured_v1::PalwRfc0608FenceV1::MemoryRoutingCourtV2,
                &own.daa_score(),
            )?;
        }
        // The state-params mirror (the registration / claim-gate / binding readers) must agree with the effective fence.
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && bundle.state.memory_routing_court_from_daa() != self.palw_memory_routing_court_v2_fence().map(|f| f.daa_score())
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "palw_memory_routing_court_v2 disagrees with the V2 bundle's mirror: Params::sync_palw_memory_routing_court_v2",
            ));
        }
        Ok(())
    }

    /// **The fence's mirror** on the V2 bundle's state params (`memory_routing_court_from_daa`): the EFFECTIVE height
    /// ([`Self::palw_memory_routing_court_v2_fence`], which waits for the one-move court's own fence). Written here and nothing
    /// else; `None` where the fence is not armed (or is `never()`), so every shipped preset's bundle is unchanged.
    pub fn sync_palw_memory_routing_court_v2(&mut self) {
        let from = self.palw_memory_routing_court_v2_fence().map(|f| f.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_memory_routing_court_from_daa(from);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{ForkActivation, MAINNET_PARAMS, TESTNET_PARAMS};
    use crate::palw_lifecycle_objects_v2::{PalwLifecycleKindFenceV1, PalwLifecycleKindFencesV1};
    use crate::palw_state_v2::PalwConsensusObjectV2;
    fn accusation() -> PalwGdnCourtAccusationV2 {
        let step = crate::palw_step_refute::tests::skeleton_refutation();
        let bond = |n| {
            PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([n; 64]), index: 0 })
        };
        PalwGdnCourtAccusationV2 {
            version: 2,
            claim: Hash64::from_bytes([1; 64]),
            execution_root: step.binding.committed_execution_root,
            trace_root: Hash64::from_bytes([2; 64]),
            executor_bond: bond(3),
            accuser_bond: bond(4),
            leaf_index: step.output_opening.leaf_index,
            refutation: PalwGdnStepRefutationV2 { version: 2, step, checkpoint: None },
            artifact_openings: vec![],
            signature: vec![1],
        }
    }
    #[test]
    fn new_carrier_is_dormant_old_tags_and_payloads_keep_their_encoding() {
        let object = PalwConsensusObjectV2::GdnStepAccusedV2 { accusation: Box::new(accusation()) };
        let encoded = borsh::to_vec(&object).unwrap();
        assert_eq!(encoded[0], 243);
        assert_eq!(borsh::from_slice::<PalwConsensusObjectV2>(&encoded).unwrap(), object);
        let old = crate::palw_shard_court_v1::PalwShardCourtAccusationV1 {
            version: 1,
            claim: accusation().claim,
            execution_root: accusation().execution_root,
            trace_root: accusation().trace_root,
            executor_bond: accusation().executor_bond,
            accuser_bond: accusation().accuser_bond,
            leaf_index: accusation().leaf_index,
            refutation: accusation().refutation.step,
            artifact_openings: vec![],
            prompt_ids_opening: None,
            signature: vec![1],
        };
        assert_eq!(borsh::to_vec(&PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(old.clone()) }).unwrap()[0], 38);
        let fences = PalwLifecycleKindFencesV1::default();
        assert!(!fences.kind_in_force_at(&object, u64::MAX));
        let armed = fences.with(PalwLifecycleKindFenceV1::MemoryRoutingCourtV2, Some(ForkActivation::new(10)));
        assert!(!armed.kind_in_force_at(&object, 9));
        assert!(armed.kind_in_force_at(&object, 10));
        let network = Hash64::from_bytes([8; 64]);
        assert_ne!(
            palw_gdn_court_session_id_v2(network, &accusation()),
            crate::palw_shard_court_v1::palw_shard_court_session_id_v1(network.as_byte_slice(), &old)
        );
    }
    #[test]
    fn new_unverified_consumer_cannot_arm_a_shipped_network() {
        for mut p in [MAINNET_PARAMS, TESTNET_PARAMS] {
            assert_eq!(p.palw_memory_routing_court_v2, None);
            assert!(p.palw_fences_v1().contains(&("palw_memory_routing_court_v2", None)));
            p.validate_palw_memory_routing_court_v2().unwrap();
            p.palw_memory_routing_court_v2 = Some(ForkActivation::never());
            p.validate_palw_memory_routing_court_v2().unwrap();
            p.palw_memory_routing_court_v2 = Some(ForkActivation::new(10));
            assert!(p.validate_palw_memory_routing_court_v2().is_err());
        }
    }
    #[test]
    fn new_session_binds_network_claim_leaf_and_actor() {
        let a = accusation();
        let network = Hash64::from_bytes([8; 64]);
        let digest = palw_gdn_court_session_id_v2(network, &a);
        assert_ne!(digest, palw_gdn_court_session_id_v2(Hash64::from_bytes([9; 64]), &a));
        let mut changed = a.clone();
        changed.claim = Hash64::default();
        assert_ne!(digest, palw_gdn_court_session_id_v2(network, &changed));
        let mut changed = a.clone();
        changed.leaf_index += 1;
        assert_ne!(digest, palw_gdn_court_session_id_v2(network, &changed));
        let mut changed = a.clone();
        std::mem::swap(&mut changed.executor_bond, &mut changed.accuser_bond);
        assert_ne!(digest, palw_gdn_court_session_id_v2(network, &changed));
    }
    #[test]
    fn borrowed_view_reuses_evidence_and_preserves_shape_refusal_order() {
        let a = accusation();
        let view = a.as_view_v2();
        assert!(std::ptr::eq(view.refutation, &a.refutation));
        assert_eq!(view.artifact_openings.as_ptr(), a.artifact_openings.as_ptr());
        assert_eq!(view.validate_shape(u64::MAX), Ok(()));
        let mut changed = a.clone();
        changed.version = 0;
        assert_eq!(changed.validate_shape(0), Err("unsupported GDN court/evidence version"));
        changed = a.clone();
        changed.refutation.version = 0;
        assert_eq!(changed.validate_shape(0), Err("unsupported GDN court/evidence version"));
        assert_eq!(a.validate_shape(a.leaf_index), Err("GDN accusation leaf does not match its opening/ladder"));
        changed = a.clone();
        changed.leaf_index = changed.leaf_index.saturating_add(1);
        assert_eq!(changed.validate_shape(u64::MAX), Err("GDN accusation leaf does not match its opening/ladder"));
        changed = a.clone();
        changed.accuser_bond = changed.executor_bond;
        assert_eq!(changed.validate_shape(u64::MAX), Err("GDN accusation accuser is its executor"));
        changed = a.clone();
        changed.execution_root = Hash64::from_u64_word(999);
        assert_eq!(changed.validate_shape(u64::MAX), Err("GDN accusation binding names another execution"));
    }

    #[test]
    fn wire_and_borrowed_entry_points_preserve_actual_claim_refusal() {
        let a = accusation();
        let claim = PalwOneMoveClaimV2 {
            execution_root: Hash64::from_u64_word(999),
            class_id: Hash64::default(),
            artifact_root: Hash64::default(),
        };
        let owned = palw_gdn_court_verdict_v2(&a, &claim, u64::MAX).unwrap_err();
        let borrowed = palw_gdn_court_verdict_view_v2(a.as_view_v2(), &claim, u64::MAX).unwrap_err();
        for error in [owned, borrowed] {
            assert!(matches!(
                error,
                PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::InputSetNotCanonical(
                    "GDN accusation is not the actual native claim's execution"
                ))
            ));
        }
    }
}
