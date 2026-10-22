//! Ephemeral execution authority, before any execution root exists. This is not
//! a wire declaration, material journal authority, or selected-verifier vote.
use super::*;
use crate::{
    BlockHash,
    config::params::Params,
    header::Header,
    palw_native_resource_v1::{NativeChainStateV1, NativeHeapV1, NativeRetainedV1},
    palw_state_v2::{PalwBondStatusV2, PalwClassStatusV2, PalwStateV2Error},
    palw_v2::PalwJobContextV2,
};
use std::sync::Arc;

/// No Deserialize, Clone or raw-row constructor. The native retained snapshot
/// remains owned throughout execution, signing and publication. The own-node
/// getter brackets this checked factory with a stable import/sink observation.
pub struct PalwNativeRoutingExecutionAuthorityV2 {
    snapshot: Arc<NativeChainStateV1>,
    row: NativeRetainedV1<PalwReaderRouterRowV2>,
    job: NativeRetainedV1<PalwJobContextV2>,
    checkpoint: BlockHash,
    /// AFTER(checkpoint), committed by the prospective selected child. The
    /// checkpoint header itself commits AFTER(its own selected parent).
    checkpoint_root: Hash64,
    import_epoch: u64,
    accepted_daa: u64,
    network: Hash64,
    executor: PalwBondKeyV2,
    _shell: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
}
impl std::fmt::Debug for PalwNativeRoutingExecutionAuthorityV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PalwNativeRoutingExecutionAuthorityV2")
            .field("checkpoint", &self.checkpoint)
            .field("import_epoch", &self.import_epoch)
            .field("accepted_daa", &self.accepted_daa)
            .field("row", &self.row)
            .finish_non_exhaustive()
    }
}
impl PalwNativeRoutingExecutionAuthorityV2 {
    /// A native snapshot, imported header and full prospective job are required;
    /// supplying an owner-signed declaration or fixture row alone grants no
    /// authority. Consensus must source every input from its own selected DB.
    pub fn from_imported_snapshot_v2(
        snapshot: Arc<NativeChainStateV1>,
        params: &Params,
        checkpoint: &Header,
        candidate: &Header,
        job: &PalwJobContextV2,
        executor: PalwBondKeyV2,
        network_id: &[u8],
        import_epoch: u64,
    ) -> Result<Option<Self>, PalwReaderRouterErrorV2> {
        if !params
            .palw_memory_routing_court_v2
            .is_some_and(|f| f != crate::config::params::ForkActivation::never() && f.is_active(candidate.daa_score))
        {
            return Ok(None);
        }
        let point = snapshot.last_point().ok_or_else(|| bad("routing execution native state has no imported point"))?;
        if checkpoint.hash == BlockHash::default()
            || point.block != checkpoint.hash
            || point.daa_score != checkpoint.daa_score
            || candidate.daa_score < point.daa_score
            || !candidate.direct_parents().contains(&checkpoint.hash)
            || job.job_id == Hash64::default()
        {
            return Err(bad("routing execution snapshot/header/job identity mismatch"));
        }
        let class = snapshot.class(&job.shape_profile_id).ok_or_else(|| bad("routing execution native class absent"))?;
        // Typed TIR/GenTensor programs retain their independent semantics. An
        // unresolved legacy profile is an error, never an implicit V1 bypass.
        if snapshot.tir_class_v1(&job.shape_profile_id).is_some() || snapshot.gen_class_v1(&job.shape_profile_id).is_some() {
            return Ok(None);
        }
        let profile =
            snapshot.fp_work_profile_of(&job.shape_profile_id).ok_or_else(|| bad("routing execution legacy profile unavailable"))?;
        if !(0..profile.global_node_count()).filter_map(|slot| profile.resolve_node_slot(slot)).any(|(n, _)| needs_reader_router_v2(n))
        {
            return Ok(None);
        }
        let network = crate::palw_attempt_v2::palw_network_domain_v2_for(network_id, Some(params.genesis.hash));
        if snapshot.kernel_route().is_none_or(|route| Hash64::from_bytes(route.header.policy.network_domain) != network)
            || !matches!(class.status, PalwClassStatusV2::Active)
            || class.registered_daa > candidate.daa_score
            || snapshot.bond(&executor).is_none_or(|bond| !matches!(bond.status, PalwBondStatusV2::Active) || bond.pubkey.is_empty())
        {
            return Err(bad("routing execution native network/class/executor is unavailable"));
        }
        let anchor = crate::palw_attempt_v2::execution_anchor_v3(
            network,
            crate::hashing::header::pre_pow_hash_64(candidate),
            job.shape_profile_id,
            &executor.0,
            candidate.nonce,
        );
        if anchor != job.job_id {
            return Err(bad("routing execution job does not belong to this candidate/executor"));
        }
        check_attempt_job_v2(&snapshot, params, candidate.daa_score, job, anchor, network_id)?;
        let row = snapshot
            .reader_router_declaration_v2(job.shape_profile_id)
            .map_err(|e| match e {
                PalwStateV2Error::NativeResource(e) => PalwReaderRouterErrorV2::Resource(e),
                _ => bad("routing execution native declaration is unreadable"),
            })?
            .ok_or_else(|| bad("routing execution native declaration absent"))?;
        if Some(row.registrant) != class.registrant_bond
            || row.declaration.artifact_root != class.artifact_root
            || row.bound_from_daa > candidate.daa_score
        {
            return Err(bad("routing execution owner/artifact/prospective bound mismatch"));
        }
        row.declaration.check_profile(profile)?;
        // The root preimage includes native maps. Bound its complete typed
        // source before hashing; this is logical allocation coverage only.
        let mut q = TirNativeRequestsV1::new();
        let source = (**snapshot).native_heap_v1("routing execution native root source");
        let source = source.bytes().ok_or(source)?;
        q.add_v1(
            source.checked_mul(2).ok_or_else(|| NativeCoverageV1::overflow::<NativeChainStateV1>("routing execution native root"))?,
        )?;
        q.vec_v1::<crate::palw_state_v2::PalwChainStateV2>(1)?;
        let _root = reserve_native_requests_v1(q.bytes_v1())?;
        let root =
            crate::palw_fork_choice_commitment_v1::palw_committed_state_root_v1(&snapshot, params.palw_fork_choice_commitment_v1);
        require_candidate_parent_state_v2(checkpoint, candidate, root)?;
        let job = NativeRetainedV1::try_from_clone_v1(job)?;
        let mut shell = TirNativeRequestsV1::new();
        shell.vec_v1::<Self>(1)?;
        shell.vec_v1::<usize>(3)?;
        let shell = reserve_native_requests_v1(shell.bytes_v1())?;
        Ok(Some(Self {
            snapshot,
            row,
            job,
            checkpoint: checkpoint.hash,
            checkpoint_root: root,
            import_epoch,
            accepted_daa: candidate.daa_score,
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
    pub fn checkpoint(&self) -> BlockHash {
        self.checkpoint
    }
    pub fn accepted_daa(&self) -> u64 {
        self.accepted_daa
    }
    pub fn network(&self) -> Hash64 {
        self.network
    }
    pub fn executor(&self) -> PalwBondKeyV2 {
        self.executor
    }
    pub fn import_epoch(&self) -> u64 {
        self.import_epoch
    }
    pub fn checkpoint_root(&self) -> Hash64 {
        self.checkpoint_root
    }
    pub fn native_profile(&self) -> &PalwShapeProfileV3 {
        self.snapshot.fp_work_profile_of(&self.row.declaration.class_id).expect("checked immutable native profile")
    }
    /// Re-reading the own API before publication must reproduce the exact
    /// prospective execution identity; an import generation change is refusal.
    pub fn same_execution_v2(&self, other: &Self) -> bool {
        self.checkpoint == other.checkpoint
            && self.checkpoint_root == other.checkpoint_root
            && self.import_epoch == other.import_epoch
            && self.accepted_daa == other.accepted_daa
            && self.network == other.network
            && self.executor == other.executor
            && *self.row == *other.row
            && *self.job == *other.job
    }
}

/// CoreV1 deliberately fixes its context network bytes to `misaka-palw-rc`:
/// the anchor, not that field, already binds the real network and genesis.
/// Re-derive the full canonical context rather than replacing those bytes.
pub(super) fn check_attempt_job_v2(
    native: &NativeChainStateV1,
    params: &Params,
    accepted_daa: u64,
    job: &PalwJobContextV2,
    anchor: Hash64,
    network_id: &[u8],
) -> Result<(), PalwReaderRouterErrorV2> {
    if job.job_id != anchor {
        return Err(bad("routing attempt anchor differs from its native identity"));
    }
    if !params.palw_offence_attribution_fence().is_some_and(|f| f.is_active(accepted_daa)) {
        return if job.network_id == network_id { Ok(()) } else { Err(bad("routing legacy attempt network differs")) };
    }
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err(bad("routing CoreV1 attempt has no native consensus parameters"));
    };
    let profile = native.fp_work_profile_of(&job.shape_profile_id).ok_or_else(|| bad("routing attempt profile absent"))?;
    let canonical = crate::palw_attempt_rules_v1::palw_attempt_canonical_v1(profile, job.shape_profile_id == bundle.base_class_id)
        .ok_or_else(|| bad("routing attempt canonical geometry absent"))?;
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<u32>(u64::from(canonical.0))?;
    q.vec_v1::<Hash64>(
        u64::from(canonical.0)
            .checked_next_power_of_two()
            .ok_or_else(|| bad("routing prompt overflows"))?
            .checked_mul(4)
            .ok_or_else(|| bad("routing prompt tree overflows"))?,
    )?;
    q.vec_v1::<PalwJobContextV2>(1)?;
    q.vec_v1::<u8>(b"misaka-palw-rc".len() as u64)?;
    let _scratch = reserve_native_requests_v1(q.bytes_v1())?;
    let prompt = crate::palw_attempt_rules_v1::palw_attempt_prompt_root_v1(
        profile,
        &anchor,
        canonical.0,
        params.palw_prompt_ids_form_at(accepted_daa),
    )
    .ok_or_else(|| bad("routing attempt canonical prompt unavailable"))?;
    let expected = crate::palw_attempt_v2::palw_attempt_job_v1(
        crate::palw_attempt_rules_v1::palw_attempt_canonical_context_v1(profile, &anchor, canonical, prompt),
        params.palw_prefill_draw_active_at(accepted_daa),
    );
    if expected != *job {
        return Err(bad("routing attempt full context differs from its native canonical job"));
    }
    Ok(())
}

/// The own-node getter has already authenticated this candidate's selected
/// parent against its current template context. Check the child's commitment
/// against the retained AFTER(checkpoint) state, never the checkpoint header's
/// one-block-older parent commitment. The root is computed from the retained
/// native snapshot above; it is not a caller-provided authority input.
fn require_candidate_parent_state_v2(
    checkpoint: &Header,
    candidate: &Header,
    after_checkpoint_root: Hash64,
) -> Result<(), PalwReaderRouterErrorV2> {
    if !candidate.direct_parents().contains(&checkpoint.hash) || candidate.palw_state_root != after_checkpoint_root {
        return Err(bad("routing execution candidate does not commit its own selected-parent native state"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_commits_after_checkpoint_not_checkpoints_parent_state() {
        let mut checkpoint = Header::from_precomputed_hash(Hash64::from_u64_word(1), vec![Hash64::from_u64_word(2)]);
        checkpoint.palw_state_root = Hash64::from_u64_word(3);
        let after_checkpoint_root = Hash64::from_u64_word(4);
        assert_ne!(checkpoint.palw_state_root, after_checkpoint_root);
        let mut candidate = Header::from_precomputed_hash(Hash64::from_u64_word(5), vec![checkpoint.hash]);
        candidate.palw_state_root = after_checkpoint_root;
        require_candidate_parent_state_v2(&checkpoint, &candidate, after_checkpoint_root).unwrap();

        candidate.palw_state_root = checkpoint.palw_state_root;
        assert!(require_candidate_parent_state_v2(&checkpoint, &candidate, after_checkpoint_root).is_err());
        candidate.palw_state_root = Hash64::from_u64_word(6);
        assert!(require_candidate_parent_state_v2(&checkpoint, &candidate, after_checkpoint_root).is_err());
        candidate.palw_state_root = after_checkpoint_root;
        candidate.parents_by_level.set_direct_parents(vec![Hash64::from_u64_word(7)]);
        assert!(require_candidate_parent_state_v2(&checkpoint, &candidate, after_checkpoint_root).is_err());
    }
}
