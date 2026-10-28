//! Prospective FP/rider execution. Native owners precede weights, roots and signatures.
//! These runtime objects are never wire credentials, accepted claims or verifier approvals.
use super::*;
use crate::{
    BlockHash,
    config::params::Params,
    header::Header,
    palw_freeprompt_v3::PalwFreePromptJobV3,
    palw_native_resource_v1::{NativeChainStateV1, NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1},
    palw_state_v2::{PalwBondStatusV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClassStatusV2, PalwStateV2Error},
    palw_v2::PalwJobContextV2,
};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum PalwNativeRoutingWorkFaultV2 {
    #[error("local routing work capacity: {0}")]
    Resource(NativeCoverageV1),
    #[error("own routing work context unavailable: {0}")]
    Unavailable(&'static str),
    #[error("routing work identity refused: {0}")]
    Identity(&'static str),
}
impl From<NativeCoverageV1> for PalwNativeRoutingWorkFaultV2 {
    fn from(e: NativeCoverageV1) -> Self {
        Self::Resource(e)
    }
}
impl From<PalwReaderRouterErrorV2> for PalwNativeRoutingWorkFaultV2 {
    fn from(e: PalwReaderRouterErrorV2) -> Self {
        match e {
            PalwReaderRouterErrorV2::Resource(e) => Self::Resource(e),
            PalwReaderRouterErrorV2::Identity(e) => Self::Identity(e),
        }
    }
}
type Fault = PalwNativeRoutingWorkFaultV2;
fn invalid(s: &'static str) -> Fault {
    Fault::Identity(s)
}

pub struct PalwNativeRoutingWorkAuthorityV2 {
    snapshot: Arc<NativeChainStateV1>,
    row: NativeRetainedV1<PalwReaderRouterRowV2>,
    job: NativeRetainedV1<PalwJobContextV2>,
    fp: Option<NativeRetainedV1<PalwFreePromptJobV3>>,
    rider: Option<(Hash64, u32, u64)>,
    checkpoint: BlockHash,
    checkpoint_root: Hash64,
    import_epoch: u64,
    quote_daa: u64,
    network: Hash64,
    executor: PalwBondKeyV2,
    _shell: Option<NativeDemandLeaseV1>,
}
impl std::fmt::Debug for PalwNativeRoutingWorkAuthorityV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PalwNativeRoutingWorkAuthorityV2")
            .field("checkpoint", &self.checkpoint)
            .field("quote_daa", &self.quote_daa)
            .field("rider", &self.rider)
            .finish_non_exhaustive()
    }
}
impl PalwNativeRoutingWorkAuthorityV2 {
    pub fn from_native_fp_v2(
        snapshot: Arc<NativeChainStateV1>,
        params: &Params,
        checkpoint: &Header,
        anchor: &Header,
        quote_daa: u64,
        fp: &PalwFreePromptJobV3,
        job: &PalwJobContextV2,
        network_id: &[u8],
        import_epoch: u64,
    ) -> Result<Option<Self>, Fault> {
        let executor = PalwBondKeyV2(fp.executor_bond);
        Self::from_native_work_v2(
            snapshot,
            params,
            checkpoint,
            quote_daa,
            job,
            executor,
            network_id,
            import_epoch,
            Some((fp, anchor)),
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn from_native_rider_v2(
        snapshot: Arc<NativeChainStateV1>,
        params: &Params,
        checkpoint: &Header,
        quote_daa: u64,
        job: &PalwJobContextV2,
        executor: PalwBondKeyV2,
        lead: Hash64,
        index: u32,
        retention: u64,
        network_id: &[u8],
        import_epoch: u64,
    ) -> Result<Option<Self>, Fault> {
        Self::from_native_work_v2(
            snapshot,
            params,
            checkpoint,
            quote_daa,
            job,
            executor,
            network_id,
            import_epoch,
            None,
            Some((lead, index, retention)),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn from_native_work_v2(
        snapshot: Arc<NativeChainStateV1>,
        params: &Params,
        checkpoint: &Header,
        quote_daa: u64,
        job: &PalwJobContextV2,
        executor: PalwBondKeyV2,
        network_id: &[u8],
        import_epoch: u64,
        fp: Option<(&PalwFreePromptJobV3, &Header)>,
        rider: Option<(Hash64, u32, u64)>,
    ) -> Result<Option<Self>, Fault> {
        if !params
            .palw_memory_routing_court_v2
            .is_some_and(|f| f != crate::config::params::ForkActivation::never() && f.is_active(quote_daa))
        {
            return Ok(None);
        }
        let point = snapshot.last_point().ok_or(Fault::Unavailable("native snapshot point absent"))?;
        if point.block != checkpoint.hash
            || point.daa_score != checkpoint.daa_score
            || checkpoint.hash == Hash64::default()
            || quote_daa < point.daa_score
        {
            return Err(invalid("native snapshot/checkpoint clock differs"));
        }
        let class = snapshot.class(&job.shape_profile_id).ok_or(invalid("native class absent"))?;
        if snapshot.tir_class_v1(&job.shape_profile_id).is_some() || snapshot.gen_class_v1(&job.shape_profile_id).is_some() {
            return Ok(None);
        }
        let profile = snapshot.fp_work_profile_of(&job.shape_profile_id).ok_or(Fault::Unavailable("native legacy profile absent"))?;
        if !(0..profile.global_node_count())
            .filter_map(|slot| profile.resolve_node_slot(slot))
            .any(|(node, _)| needs_reader_router_v2(node))
        {
            return Ok(None);
        }
        let network = crate::palw_attempt_v2::palw_network_domain_v2_for(network_id, Some(params.genesis.hash));
        let bond = snapshot.bond(&executor).ok_or(invalid("native executor absent"))?;
        if !matches!(class.status, PalwClassStatusV2::Active)
            || class.registered_daa > quote_daa
            || !matches!(bond.status, PalwBondStatusV2::Active)
            || bond.pubkey.is_empty()
            || snapshot.kernel_route().is_none_or(|r| Hash64::from_bytes(r.header.policy.network_domain) != network)
        {
            return Err(invalid("native network/class/executor differs"));
        }
        if let Some((fp, anchor)) = fp {
            if fp.class_id != job.shape_profile_id
                || fp.network_domain != network
                || fp.executor_pubkey != bond.pubkey
                || fp.operator_id != bond.operator_id
                || fp.anchor_block != anchor.hash
                || fp.anchor_daa != anchor.daa_score
                || fp.anchor_daa > quote_daa
                || fp.is_prefix_state()
                || job.exact_decode_tokens != fp.decode_token_limit
                || job.exact_decode_tokens == 0
                || snapshot
                    .class_semantics_v1(&fp.class_id)
                    .is_none_or(|s| !s.matches_context(job) || s.artifact_root != class.artifact_root || s.bound_from_daa > quote_daa)
            {
                return Err(invalid("prospective FP native descriptor/semantics differs"));
            }
            check_fp_context_v2(fp, job, network_id)?;
        } else if let Some((lead, index, retention)) = rider {
            use crate::palw_capacity_s567_v1 as m;
            let lead_row = snapshot.claim(&lead).ok_or(Fault::Unavailable("rider lead is not accepted in own state"))?;
            if !params.palw_capacity_riders_active_at_v1(quote_daa)
                || index as usize >= m::PALW_RIDERS_MAX_V1
                || !matches!(lead_row.source, PalwClaimSourceV2::Attempt)
                || !matches!(lead_row.phase, PalwClaimPhaseV2::Provisional)
                || lead_row.bond != executor
                || lead_row.class_id != job.shape_profile_id
                || lead_row.trace_retention_daa != retention
                || lead_row.accepted_daa.saturating_add(m::PALW_RIDERS_WINDOW_DAA_V1) < quote_daa
                || snapshot.exec_v2_root_v1(&lead).is_some()
                || snapshot.rider_mark_of_v1(&lead).is_some()
            {
                return Err(invalid("rider native lead/index/window/retention differs"));
            }
            execution::check_attempt_job_v2(&snapshot, params, quote_daa, job, m::palw_rider_job_anchor_v1(&lead, index), network_id)?;
        } else {
            return Err(invalid("work origin absent"));
        }
        let row = snapshot
            .reader_router_declaration_v2(job.shape_profile_id)
            .map_err(|why| match why {
                PalwStateV2Error::NativeResource(e) => Fault::Resource(e),
                _ => Fault::Unavailable("native routing declaration unreadable"),
            })?
            .ok_or(invalid("native routing declaration absent"))?;
        if Some(row.registrant) != class.registrant_bond
            || row.declaration.artifact_root != class.artifact_root
            || row.bound_from_daa > quote_daa
        {
            return Err(invalid("native routing owner/artifact/bound differs"));
        }
        row.declaration.check_profile(profile)?;
        let source = (**snapshot).native_heap_v1("native routing work root source");
        let bytes = source.bytes().ok_or(source)?;
        let mut q = TirNativeRequestsV1::new();
        q.add_v1(bytes.checked_mul(2).ok_or_else(|| NativeCoverageV1::overflow::<NativeChainStateV1>("routing work root"))?)?;
        q.vec_v1::<crate::palw_state_v2::PalwChainStateV2>(1)?;
        let _root = reserve_native_requests_v1(q.bytes_v1())?;
        let root =
            crate::palw_fork_choice_commitment_v1::palw_committed_state_root_v1(&snapshot, params.palw_fork_choice_commitment_v1);
        let job = NativeRetainedV1::try_from_clone_v1(job)?;
        let fp = fp.map(|(fp, _)| NativeRetainedV1::try_from_clone_v1(fp)).transpose()?;
        let mut shell = TirNativeRequestsV1::new();
        shell.vec_v1::<Self>(1)?;
        shell.vec_v1::<usize>(3)?;
        let shell = reserve_native_requests_v1(shell.bytes_v1())?;
        Ok(Some(Self {
            snapshot,
            row,
            job,
            fp,
            rider,
            checkpoint: checkpoint.hash,
            checkpoint_root: root,
            import_epoch,
            quote_daa,
            network,
            executor,
            _shell: shell,
        }))
    }
    pub fn row(&self) -> &PalwReaderRouterRowV2 {
        &self.row
    }
    pub fn job(&self) -> &PalwJobContextV2 {
        &self.job
    }
    pub fn fp_job(&self) -> Option<&PalwFreePromptJobV3> {
        self.fp.as_deref()
    }
    pub fn rider(&self) -> Option<(Hash64, u32, u64)> {
        self.rider
    }
    pub fn checkpoint(&self) -> BlockHash {
        self.checkpoint
    }
    pub fn network(&self) -> Hash64 {
        self.network
    }
    pub fn executor(&self) -> PalwBondKeyV2 {
        self.executor
    }
    pub fn native_profile(&self) -> &PalwShapeProfileV3 {
        self.snapshot.fp_work_profile_of(&self.row.declaration.class_id).expect("checked native profile")
    }
    pub fn accepts_context_v2(&self, job: &PalwJobContextV2) -> bool {
        if self.fp.is_some() { fp_context_matches_budget_v2(&self.job, job) } else { *self.job == *job }
    }
    pub fn import_epoch(&self) -> u64 {
        self.import_epoch
    }
    /// Normal forward progress may change the current snapshot. The native
    /// refresh door separately proves canonical ancestry and current eligibility.
    /// No caller may use this comparison as a substitute for that native read.
    pub fn same_work_identity_v2(&self, other: &Self) -> bool {
        self.import_epoch == other.import_epoch
            && other.quote_daa >= self.quote_daa
            && self.network == other.network
            && self.executor == other.executor
            && *self.row == *other.row
            && *self.job == *other.job
            && self.rider == other.rider
            && self.fp.as_deref() == other.fp.as_deref()
            && self.native_profile() == other.native_profile()
    }
    pub fn same_work_v2(&self, other: &Self) -> bool {
        self.checkpoint == other.checkpoint
            && self.checkpoint_root == other.checkpoint_root
            && self.import_epoch == other.import_epoch
            && self.quote_daa == other.quote_daa
            && self.network == other.network
            && self.executor == other.executor
            && *self.row == *other.row
            && *self.job == *other.job
            && self.rider == other.rider
            && self.fp.as_deref() == other.fp.as_deref()
    }
}
fn check_fp_context_v2(fp: &PalwFreePromptJobV3, job: &PalwJobContextV2, network_id: &[u8]) -> Result<(), Fault> {
    use crate::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_job_context_v3, palw_fp_run_facts_for_executed_v1};
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<PalwJobContextV2>(1)?;
    q.vec_v1::<u8>(network_id.len() as u64)?;
    let _scratch = reserve_native_requests_v1(q.bytes_v1())?;
    let class = PalwFpClassFactsV3 {
        model_profile_id: job.model_profile_id,
        runtime_manifest_hash: job.runtime_manifest_hash,
        runtime_class_id: job.runtime_class_id,
        shape_profile_id: job.shape_profile_id,
        cu_ruleset_id: job.cu_ruleset_id,
    };
    let facts = palw_fp_run_facts_for_executed_v1(fp, fp.decode_token_limit);
    let expected =
        palw_fp_job_context_v3(fp, &class, &facts, network_id).map_err(|_| invalid("FP descriptor cannot derive native context"))?;
    if expected != *job {
        return Err(invalid("FP whole context differs from descriptor"));
    }
    Ok(())
}
/// The pipeline owns early stopping. Only its executed count may vary, within the bound job;
/// every identity, nonce/seed, runtime and native class field remains equal without a clone.
fn fp_context_matches_budget_v2(expected: &PalwJobContextV2, actual: &PalwJobContextV2) -> bool {
    actual.exact_decode_tokens > 0
        && actual.exact_decode_tokens <= expected.exact_decode_tokens
        && actual.version == expected.version
        && actual.network_id == expected.network_id
        && actual.job_id == expected.job_id
        && actual.job_nullifier == expected.job_nullifier
        && actual.assignment_id == expected.assignment_id
        && actual.execution_seed == expected.execution_seed
        && actual.model_profile_id == expected.model_profile_id
        && actual.runtime_manifest_hash == expected.runtime_manifest_hash
        && actual.runtime_class_id == expected.runtime_class_id
        && actual.shape_profile_id == expected.shape_profile_id
        && actual.trace_scheme_id == expected.trace_scheme_id
        && actual.cu_ruleset_id == expected.cu_ruleset_id
        && actual.tokenizer_id == expected.tokenizer_id
        && actual.prompt_token_ids_hash == expected.prompt_token_ids_hash
        && actual.declared_prefill_tokens == expected.declared_prefill_tokens
        && actual.max_context_tokens == expected.max_context_tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prospective_fp_stop_count_changes_never_change_fixed_native_context() {
        let (profile, _) = super::super::tests::profile_and_mapping(false);
        let mut expected = crate::palw_base0_profile::rc_job_context(&profile, 2, 4);
        expected.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
        let mut stopped = expected.clone();
        stopped.exact_decode_tokens = 1;
        assert!(fp_context_matches_budget_v2(&expected, &stopped));
        for count in [0, 5] {
            let mut changed = stopped.clone();
            changed.exact_decode_tokens = count;
            assert!(!fp_context_matches_budget_v2(&expected, &changed));
        }
        for field in 0..15 {
            let mut changed = stopped.clone();
            match field {
                0 => changed.version ^= 1,
                1 => changed.network_id.push(0),
                2 => changed.job_id = Hash64::from_u64_word(501),
                3 => changed.job_nullifier = Hash64::from_u64_word(502),
                4 => changed.assignment_id = Hash64::from_u64_word(503),
                5 => changed.execution_seed[0] ^= 1,
                6 => changed.model_profile_id = Hash64::from_u64_word(504),
                7 => changed.runtime_manifest_hash = Hash64::from_u64_word(505),
                8 => changed.runtime_class_id = Hash64::from_u64_word(506),
                9 => changed.shape_profile_id = Hash64::from_u64_word(507),
                10 => changed.trace_scheme_id = Hash64::from_u64_word(508),
                11 => changed.cu_ruleset_id = Hash64::from_u64_word(509),
                12 => changed.tokenizer_id = Hash64::from_u64_word(510),
                13 => changed.prompt_token_ids_hash = Hash64::from_u64_word(511),
                _ => changed.declared_prefill_tokens += 1,
            }
            assert!(!fp_context_matches_budget_v2(&expected, &changed), "changed fixed native context field {field}");
        }
        let mut changed = stopped;
        changed.max_context_tokens += 1;
        assert!(!fp_context_matches_budget_v2(&expected, &changed));
    }
}
