//! Replacing a chain this node has not yet committed to.
//!
//! Once an IBD commits, the local pruning point becomes a safety boundary and any other history is
//! a finality conflict. That is correct, and IBD source selection has no business overriding it —
//! reorg policy belongs to the DNS gate.
//!
//! But there is a window before that where the boundary means something weaker than it looks. A
//! node that has just finished its first IBD has not *agreed* with anyone about which chain is
//! canonical; it adopted whichever peer won a race, and is holding participation back precisely
//! because nothing has confirmed that choice. Treating that chain as finalized is what makes the
//! first peer's chain permanent — the loop this whole effort exists to break.
//!
//! So the chain adopted during review is **provisional**, and a verified-better candidate may
//! replace it. Once the node reaches `Ready` it has committed, and this stops being available
//! forever: later disagreements go through DNS TTL, the reorg horizon, work override, or an
//! operator. A safety mechanism left unattended tends to get promoted, so the permit is one-shot,
//! bound to the exact candidate and generation it was issued for, and refuses on every condition
//! independently.
//!
//! **This decision never consults DNS state.** The chain adopted during review brings its own
//! overlay with it, and letting that overlay veto its own replacement is exactly the
//! self-justification being removed: adopt a peer, import its DNS, and have that DNS refuse the
//! alternative. During review the operator's trusted checkpoint is the only authority; imported DNS
//! state is an observation.

use kaspa_consensus_core::{BlockHash, BlueWorkType, config::trusted_checkpoint::TrustedCheckpoint};
use kaspa_core::chain_participation::ChainParticipation;
use kaspa_hashes::Hash;

use super::ibd_candidates::CandidateId;

/// What this node has done with chains so far. Persisted, because every field here is something a
/// restart must not be able to launder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainReviewState {
    pub participation: ChainParticipation,
    /// Whether this node has ever finished a review and started participating.
    ///
    /// The one-way door. Before it, the adopted chain is provisional; after it, the node has acted
    /// on that chain — mined on it, attested it, told peers it was synced — and withdrawing is a
    /// reorg rather than a correction.
    pub ever_ready: bool,
    /// Increments each time a chain is provisionally adopted. Binds a permit to the situation it
    /// was issued for, so one that arrives late cannot be applied to a different chain.
    pub adoption_generation: u64,
    pub switch_count: u32,
}

/// A candidate that has been verified, as far as this node can verify anything pre-adoption.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifiedCandidate {
    pub id: CandidateId,
    /// Blue work at the candidate's pruning point, established by its validated pruning proof.
    pub verified_blue_work: BlueWorkType,
    /// Digest of the proof that established it, so a permit cannot be reused for a different proof.
    pub proof_hash: Hash,
    pub genesis_hash: BlockHash,
    pub consensus_params_id: Hash,
    /// Whether the candidate descends from the operator's checkpoint. `None` when none is set.
    pub descends_from_checkpoint: Option<bool>,
}

/// Authority to cross the provisional pruning point exactly once, for exactly this chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapRecoveryPermit {
    pub candidate_id: CandidateId,
    pub pruning_point: BlockHash,
    pub adoption_generation: u64,
    pub switch_generation: u32,
    pub proof_hash: Hash,
    pub trusted_checkpoint: Option<TrustedCheckpoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryError {
    /// The node is participating, so the chain under it is not provisional any more.
    AlreadyParticipating,
    /// The node has been `Ready` at some point in its life. The door does not reopen.
    HasBeenReady,
    WrongGenesis,
    WrongConsensusParams,
    /// Does not descend from the operator's trusted checkpoint.
    CheckpointIncompatible,
    /// Not strictly better than what is provisionally held. Equality keeps the incumbent.
    NotStrictlySuperior,
    /// No reservation, or one naming a different chain.
    NoReservation,
    SwitchLimitReached,
}

/// The facts a recovery decision is made from. Gathered by the caller; judged here.
pub struct RecoveryRequest<'a> {
    pub state: ChainReviewState,
    pub candidate: VerifiedCandidate,
    pub provisional_blue_work: BlueWorkType,
    pub local_genesis: BlockHash,
    pub local_consensus_params_id: Hash,
    pub checkpoint: Option<&'a TrustedCheckpoint>,
    /// The chain currently reserved by the handoff, if any.
    pub reserved_candidate: Option<CandidateId>,
    pub switch_limit: u32,
}

/// Decide whether this node may replace the chain it provisionally holds.
///
/// Every condition is checked independently and the first failure is returned, so a refusal names
/// one reason rather than a conjunction. Ordered from "you may not do this at all" outward, because
/// that is the order an operator reads them in.
pub fn authorize_bootstrap_recovery(request: RecoveryRequest<'_>) -> Result<BootstrapRecoveryPermit, RecoveryError> {
    // The one-way door first. Nothing about a better chain reopens it.
    if request.state.ever_ready {
        return Err(RecoveryError::HasBeenReady);
    }
    if !matches!(
        request.state.participation,
        ChainParticipation::IbdRunning | ChainParticipation::CandidateReview | ChainParticipation::Quarantined
    ) {
        return Err(RecoveryError::AlreadyParticipating);
    }

    // Rules before quality: a chain under different rules is not a candidate at all.
    if request.candidate.genesis_hash != request.local_genesis {
        return Err(RecoveryError::WrongGenesis);
    }
    if request.candidate.consensus_params_id != request.local_consensus_params_id {
        return Err(RecoveryError::WrongConsensusParams);
    }
    if let Some(checkpoint) = request.checkpoint {
        if checkpoint.consensus_params_id != request.local_consensus_params_id {
            return Err(RecoveryError::WrongConsensusParams);
        }
        if request.candidate.descends_from_checkpoint != Some(true) {
            return Err(RecoveryError::CheckpointIncompatible);
        }
    }

    // Strictly. Equal work is not a reason to abandon what is already synced.
    if request.candidate.verified_blue_work <= request.provisional_blue_work {
        return Err(RecoveryError::NotStrictlySuperior);
    }

    // The reservation is what makes this a handoff rather than a free-for-all; a permit without one
    // would let any verified candidate cross the boundary at any moment.
    if request.reserved_candidate != Some(request.candidate.id) {
        return Err(RecoveryError::NoReservation);
    }

    if request.state.switch_count >= request.switch_limit {
        return Err(RecoveryError::SwitchLimitReached);
    }

    Ok(BootstrapRecoveryPermit {
        candidate_id: request.candidate.id,
        pruning_point: request.candidate.id.pruning_point,
        adoption_generation: request.state.adoption_generation,
        switch_generation: request.state.switch_count,
        proof_hash: request.candidate.proof_hash,
        trusted_checkpoint: request.checkpoint.copied(),
    })
}

impl BootstrapRecoveryPermit {
    /// Whether this permit still applies. A permit issued for one situation must not be redeemed in
    /// another: chains move, reservations change, and a stale permit is indistinguishable from a
    /// forged one at the point of use.
    pub fn is_valid_for(&self, state: &ChainReviewState, candidate_id: CandidateId, proof_hash: Hash) -> bool {
        !state.ever_ready
            && self.candidate_id == candidate_id
            && self.proof_hash == proof_hash
            && self.adoption_generation == state.adoption_generation
            && self.switch_generation == state.switch_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(n: u64) -> Hash {
        Hash::from_u64_word(n)
    }
    fn bhash(n: u64) -> BlockHash {
        BlockHash::from_u64_word(n)
    }
    fn work(n: u64) -> BlueWorkType {
        BlueWorkType::from_u64(n)
    }

    const LOCAL_GENESIS: u64 = 1;
    const LOCAL_PARAMS: u64 = 2;

    fn candidate() -> VerifiedCandidate {
        VerifiedCandidate {
            id: CandidateId { pruning_point: bhash(10), virtual_selected_parent: bhash(11) },
            verified_blue_work: work(500),
            proof_hash: hash(99),
            genesis_hash: bhash(LOCAL_GENESIS),
            consensus_params_id: hash(LOCAL_PARAMS),
            descends_from_checkpoint: None,
        }
    }

    fn reviewing() -> ChainReviewState {
        ChainReviewState {
            participation: ChainParticipation::CandidateReview,
            ever_ready: false,
            adoption_generation: 1,
            switch_count: 0,
        }
    }

    fn request<'a>(state: ChainReviewState, candidate: VerifiedCandidate) -> RecoveryRequest<'a> {
        let reserved = Some(candidate.id);
        RecoveryRequest {
            state,
            candidate,
            provisional_blue_work: work(100),
            local_genesis: bhash(LOCAL_GENESIS),
            local_consensus_params_id: hash(LOCAL_PARAMS),
            checkpoint: None,
            reserved_candidate: reserved,
            switch_limit: 5,
        }
    }

    #[test]
    fn a_provisional_chain_can_be_replaced_by_a_verified_better_one() {
        let permit = authorize_bootstrap_recovery(request(reviewing(), candidate())).unwrap();
        assert_eq!(permit.candidate_id, candidate().id);
        assert_eq!(permit.pruning_point, candidate().id.pruning_point);
        assert_eq!(permit.proof_hash, candidate().proof_hash);
    }

    #[test]
    fn the_door_does_not_reopen_once_the_node_has_participated() {
        // The whole safety argument. Before Ready the node only ever raced onto a chain; after it,
        // it has mined on that chain, attested it, and told peers it was synced. Withdrawing then is
        // a reorg, and reorg policy is not IBD's to make.
        let mut state = reviewing();
        state.ever_ready = true;
        assert_eq!(authorize_bootstrap_recovery(request(state, candidate())), Err(RecoveryError::HasBeenReady));

        // Not even from a state that otherwise qualifies.
        state.participation = ChainParticipation::Quarantined;
        assert_eq!(authorize_bootstrap_recovery(request(state, candidate())), Err(RecoveryError::HasBeenReady));
    }

    #[test]
    fn a_participating_node_is_refused() {
        let mut state = reviewing();
        state.participation = ChainParticipation::Ready;
        assert_eq!(authorize_bootstrap_recovery(request(state, candidate())), Err(RecoveryError::AlreadyParticipating));
    }

    #[test]
    fn every_state_that_withholds_participation_qualifies() {
        for participation in [ChainParticipation::IbdRunning, ChainParticipation::CandidateReview, ChainParticipation::Quarantined] {
            let mut state = reviewing();
            state.participation = participation;
            assert!(
                authorize_bootstrap_recovery(request(state, candidate())).is_ok(),
                "{participation:?} withholds participation, so its chain is still provisional"
            );
        }
    }

    #[test]
    fn rules_are_checked_before_quality() {
        let mut wrong_genesis = candidate();
        wrong_genesis.genesis_hash = bhash(777);
        wrong_genesis.verified_blue_work = work(u64::MAX);
        assert_eq!(authorize_bootstrap_recovery(request(reviewing(), wrong_genesis)), Err(RecoveryError::WrongGenesis));

        let mut wrong_params = candidate();
        wrong_params.consensus_params_id = hash(777);
        wrong_params.verified_blue_work = work(u64::MAX);
        assert_eq!(authorize_bootstrap_recovery(request(reviewing(), wrong_params)), Err(RecoveryError::WrongConsensusParams));
    }

    #[test]
    fn superiority_must_be_strict() {
        let mut equal = candidate();
        equal.verified_blue_work = work(100);
        assert_eq!(authorize_bootstrap_recovery(request(reviewing(), equal)), Err(RecoveryError::NotStrictlySuperior));

        let mut worse = candidate();
        worse.verified_blue_work = work(99);
        assert_eq!(authorize_bootstrap_recovery(request(reviewing(), worse)), Err(RecoveryError::NotStrictlySuperior));
    }

    #[test]
    fn a_permit_requires_a_reservation_naming_this_chain() {
        // Without this, any verified candidate could cross the boundary whenever it liked, and the
        // handoff would stop being a handoff.
        let mut req = request(reviewing(), candidate());
        req.reserved_candidate = None;
        assert_eq!(authorize_bootstrap_recovery(req), Err(RecoveryError::NoReservation));

        let mut req = request(reviewing(), candidate());
        req.reserved_candidate = Some(CandidateId { pruning_point: bhash(90), virtual_selected_parent: bhash(91) });
        assert_eq!(authorize_bootstrap_recovery(req), Err(RecoveryError::NoReservation));
    }

    #[test]
    fn the_switch_limit_is_enforced_here_too() {
        let mut state = reviewing();
        state.switch_count = 5;
        assert_eq!(authorize_bootstrap_recovery(request(state, candidate())), Err(RecoveryError::SwitchLimitReached));
    }

    #[test]
    fn a_checkpoint_must_be_satisfied_not_merely_present() {
        let checkpoint = TrustedCheckpoint { daa_score: 10, block_hash: bhash(42), consensus_params_id: hash(LOCAL_PARAMS) };

        let mut req = request(reviewing(), candidate());
        req.checkpoint = Some(&checkpoint);
        assert_eq!(authorize_bootstrap_recovery(req), Err(RecoveryError::CheckpointIncompatible), "unknown ancestry is not consent");

        let mut descending = candidate();
        descending.descends_from_checkpoint = Some(false);
        let mut req = request(reviewing(), descending);
        req.checkpoint = Some(&checkpoint);
        assert_eq!(authorize_bootstrap_recovery(req), Err(RecoveryError::CheckpointIncompatible));

        let mut descending = candidate();
        descending.descends_from_checkpoint = Some(true);
        let mut req = request(reviewing(), descending);
        req.checkpoint = Some(&checkpoint);
        assert!(authorize_bootstrap_recovery(req).is_ok());
    }

    #[test]
    fn a_checkpoint_for_other_rules_refuses_rather_than_being_ignored() {
        let checkpoint = TrustedCheckpoint { daa_score: 10, block_hash: bhash(42), consensus_params_id: hash(4242) };
        let mut descending = candidate();
        descending.descends_from_checkpoint = Some(true);
        let mut req = request(reviewing(), descending);
        req.checkpoint = Some(&checkpoint);
        assert_eq!(authorize_bootstrap_recovery(req), Err(RecoveryError::WrongConsensusParams));
    }

    #[test]
    fn a_permit_is_bound_to_the_situation_that_issued_it() {
        let state = reviewing();
        let permit = authorize_bootstrap_recovery(request(state, candidate())).unwrap();
        let c = candidate();
        assert!(permit.is_valid_for(&state, c.id, c.proof_hash));

        // A different chain, a different proof, a later adoption, a later switch: all stale.
        let other = CandidateId { pruning_point: bhash(90), virtual_selected_parent: bhash(91) };
        assert!(!permit.is_valid_for(&state, other, c.proof_hash));
        assert!(!permit.is_valid_for(&state, c.id, hash(1234)));

        let mut moved_on = state;
        moved_on.adoption_generation += 1;
        assert!(!permit.is_valid_for(&moved_on, c.id, c.proof_hash));

        let mut switched = state;
        switched.switch_count += 1;
        assert!(!permit.is_valid_for(&switched, c.id, c.proof_hash));

        let mut participated = state;
        participated.ever_ready = true;
        assert!(!permit.is_valid_for(&participated, c.id, c.proof_hash), "a permit must not survive the one-way door");
    }
}
