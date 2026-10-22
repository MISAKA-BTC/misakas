//! Accepted-claim execution context. This is an own-state runtime owner, never
//! a deserializable provider credential or selected-verifier approval.
use super::*;
use crate::{
    BlockHash,
    config::params::Params,
    palw_native_resource_v1::{NativeChainStateV1, NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1},
    palw_state_v2::{PalwClaimSourceV2, PalwStateV2Error},
    palw_v2::PalwJobContextV2,
};
use std::sync::Arc;

/// Distinct from prospective block authority and from material authority. A
/// native claim stamp is mandatory: current class declarations never apply to
/// an old unstamped claim. All source owners survive the replay/publication.
pub struct PalwNativeRoutingClaimAuthorityV2 {
    snapshot: Arc<NativeChainStateV1>,
    row: NativeRetainedV1<PalwReaderRouterRowV2>,
    job: NativeRetainedV1<PalwJobContextV2>,
    checkpoint: BlockHash,
    checkpoint_root: Hash64,
    import_epoch: u64,
    claim: Hash64,
    execution: Hash64,
    accepted_daa: u64,
    network: Hash64,
    free_prompt: bool,
    _shell: Option<NativeDemandLeaseV1>,
}
impl std::fmt::Debug for PalwNativeRoutingClaimAuthorityV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PalwNativeRoutingClaimAuthorityV2")
            .field("claim", &self.claim)
            .field("checkpoint", &self.checkpoint)
            .field("accepted_daa", &self.accepted_daa)
            .finish_non_exhaustive()
    }
}
impl PalwNativeRoutingClaimAuthorityV2 {
    /// The processor must supply its own exact imported snapshot in a stable
    /// sink/import bracket. A network record cannot reconstruct this owner.
    pub fn from_native_claim_v2(
        snapshot: Arc<NativeChainStateV1>,
        params: &Params,
        checkpoint: BlockHash,
        claim_id: Hash64,
        job: &PalwJobContextV2,
        network_id: &[u8],
        import_epoch: u64,
    ) -> Result<Option<Self>, PalwReaderRouterErrorV2> {
        let point = snapshot.last_point().ok_or_else(|| bad("routing replay own imported point absent"))?;
        if point.block != checkpoint || checkpoint == BlockHash::default() {
            return Err(bad("routing replay own point differs from checkpoint"));
        }
        let claim = snapshot.claim(&claim_id).ok_or_else(|| bad("routing replay native claim absent"))?;
        let Some(row) = snapshot.reader_router_of_claim_v2(claim_id).map_err(|e| match e {
            PalwStateV2Error::NativeResource(e) => PalwReaderRouterErrorV2::Resource(e),
            _ => bad("routing replay native stamp unreadable"),
        })?
        else {
            return Ok(None); // Historical V1 interpretation, even after a new declaration.
        };
        let active = params
            .palw_memory_routing_court_v2
            .is_some_and(|f| f != crate::config::params::ForkActivation::never() && f.is_active(claim.accepted_daa));
        let class = snapshot.class(&claim.class_id).ok_or_else(|| bad("routing replay native class removed"))?;
        let profile = snapshot.fp_work_profile_of(&claim.class_id).ok_or_else(|| bad("routing replay native profile absent"))?;
        let network = crate::palw_attempt_v2::palw_network_domain_v2_for(network_id, Some(params.genesis.hash));
        if !active
            || row.bound_from_daa > claim.accepted_daa
            || row.declaration.class_id != claim.class_id
            || row.declaration.artifact_root != class.artifact_root
            || Some(row.registrant) != class.registrant_bond
            || job.shape_profile_id != claim.class_id
            || claim.job_identity == Hash64::default()
            || snapshot.kernel_route().is_none_or(|r| Hash64::from_bytes(r.header.policy.network_domain) != network)
        {
            return Err(bad("routing replay claim/stamp/class/job/native network differs"));
        }
        row.declaration.check_profile(profile)?;
        let free_prompt = matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. });
        if free_prompt {
            // The native FP pin covers the variable job fields, while the
            // accepted semantic row covers model/runtime/class identities.
            // Remaining ordinary-V2 fields have ONE canonical value. Prefix
            // inheritance cannot be authenticated from that pin alone: it
            // requires the original accepted descriptor, a separate consumer.
            if !ordinary_fp_context_v2(job)
                || job.network_id != network_id
                || crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(job) != claim.job_identity
                || snapshot.claim_semantics_v1(&claim_id).is_none_or(|s| {
                    !s.matches_context(job)
                        || s.artifact_root != row.declaration.artifact_root
                        || s.bound_from_daa > claim.accepted_daa
                })
            {
                return Err(bad("routing replay FP context differs from native job pin or accepted class semantics"));
            }
        } else {
            // Includes accepted riders: apply_attempt recorded their own
            // lead/index-derived anchor, not the carrying block's anchor.
            execution::check_attempt_job_v2(&snapshot, params, claim.accepted_daa, job, claim.job_identity, network_id)?;
        }
        let (execution, accepted_daa) = (claim.execution_root, claim.accepted_daa);
        let source = (**snapshot).native_heap_v1("routing replay native root source");
        let source_bytes = source.bytes().ok_or(source)?;
        let mut q = TirNativeRequestsV1::new();
        q.add_v1(source_bytes.checked_mul(2).ok_or_else(|| NativeCoverageV1::overflow::<NativeChainStateV1>("routing replay root"))?)?;
        q.vec_v1::<crate::palw_state_v2::PalwChainStateV2>(1)?;
        let _root = reserve_native_requests_v1(q.bytes_v1())?;
        let checkpoint_root =
            crate::palw_fork_choice_commitment_v1::palw_committed_state_root_v1(&snapshot, params.palw_fork_choice_commitment_v1);
        let job = NativeRetainedV1::try_from_clone_v1(job)?;
        let mut q = TirNativeRequestsV1::new();
        q.vec_v1::<Self>(1)?;
        q.vec_v1::<usize>(3)?;
        let shell = reserve_native_requests_v1(q.bytes_v1())?;
        Ok(Some(Self {
            snapshot,
            row,
            job,
            checkpoint,
            checkpoint_root,
            import_epoch,
            claim: claim_id,
            execution,
            accepted_daa,
            network,
            free_prompt,
            _shell: shell,
        }))
    }
    pub fn row(&self) -> &PalwReaderRouterRowV2 {
        &self.row
    }
    pub fn job(&self) -> &PalwJobContextV2 {
        &self.job
    }
    pub fn native_profile(&self) -> &PalwShapeProfileV3 {
        self.snapshot.fp_work_profile_of(&self.row.declaration.class_id).expect("checked immutable native profile")
    }
    pub fn network(&self) -> Hash64 {
        self.network
    }
    pub fn claim(&self) -> Hash64 {
        self.claim
    }
    pub fn execution(&self) -> Hash64 {
        self.execution
    }
    pub fn checkpoint(&self) -> BlockHash {
        self.checkpoint
    }
    pub fn is_free_prompt(&self) -> bool {
        self.free_prompt
    }
    pub fn same_claim_v2(&self, other: &Self) -> bool {
        // Normal forward progress may change the containing snapshot. Each
        // getter authenticates that snapshot separately; publication requires
        // the same accepted claim/stamp/job under the same import generation.
        self.import_epoch == other.import_epoch
            && self.claim == other.claim
            && self.execution == other.execution
            && self.accepted_daa == other.accepted_daa
            && self.network == other.network
            && self.free_prompt == other.free_prompt
            && *self.row == *other.row
            && *self.job == *other.job
    }
    /// Identity of the retained own AFTER(checkpoint) source, for audit only.
    pub fn checkpoint_root(&self) -> Hash64 {
        self.checkpoint_root
    }
}

fn ordinary_fp_context_v2(job: &PalwJobContextV2) -> bool {
    job.version == crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2
        && job.job_nullifier == Hash64::default()
        && job.assignment_id == Hash64::default()
        && job.trace_scheme_id == crate::palw_v2::trace_scheme_id_v2()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fp_pin_omitted_fields_cannot_authorize_another_context() {
        let (profile, _) = super::super::tests::profile_and_mapping(false);
        let mut ctx = crate::palw_base0_profile::rc_job_context(&profile, 1, 1);
        ctx.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
        assert!(ordinary_fp_context_v2(&ctx));
        let pin = crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&ctx);
        for field in 0..4 {
            let mut changed = ctx.clone();
            match field {
                0 => changed.version = crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V3_INHERITED,
                1 => changed.job_nullifier = Hash64::from_u64_word(1),
                2 => changed.assignment_id = Hash64::from_u64_word(2),
                _ => changed.trace_scheme_id = Hash64::from_u64_word(3),
            }
            assert_eq!(crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&changed), pin);
            assert!(!ordinary_fp_context_v2(&changed), "native job pin alone omits this context field");
        }
    }
}
