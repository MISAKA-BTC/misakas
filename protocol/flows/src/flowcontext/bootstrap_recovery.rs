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
    /// Proof-backed work at the candidate's pruning point. Establishes that the chain is real; it
    /// is deliberately NOT what decides adoption, because pruning-point work is a function of depth
    /// rather than of branch and cannot separate two histories of comparable length.
    pub verified_blue_work: Option<BlueWorkType>,
    /// The tip work the peer claims. Orders investigation; decides nothing.
    pub claimed_tip_blue_work: BlueWorkType,
    /// Digest of the proof that established it, so a permit cannot be reused for a different proof.
    pub proof_hash: Hash,
    pub genesis_hash: BlockHash,
    pub consensus_params_id: Hash,
    /// Whether the candidate descends from the operator's checkpoint. `None` when none is set.
    pub descends_from_checkpoint: Option<bool>,
}

/// Authority to **validate** a chain: sync it into staging across the provisional pruning point and
/// run it through the ordinary pipeline to its tip. Exactly once, for exactly this chain.
///
/// Carries no power to change the active consensus. Everything it allows is undone by cancelling
/// staging.
///
/// Kept separate from [`CandidateAdoptionPermit`] because collapsing the two is circular: crossing
/// the boundary needs a permit, a permit would need proven superiority, and proving superiority
/// needs tip work, which needs crossing the boundary. Splitting them cuts the loop at the only
/// place it can be cut — the expensive, reversible step is authorised on cheap evidence, and the
/// irreversible step waits for evidence this node computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateValidationPermit {
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
    /// Not worth investigating: the chain does not even claim to beat what is held. A claim decides
    /// what to look at and nothing else, so this is the cheapest possible filter, not a verdict.
    NotWorthInvestigating,
    /// Its pruning proof has not been validated, so there is nothing to distinguish it from a
    /// fabricated chain and no reason to spend a header sync on it.
    ProofNotValidated,
    /// No reservation, or one naming a different chain.
    NoReservation,
    SwitchLimitReached,
}

/// The facts a recovery decision is made from. Gathered by the caller; judged here.
pub struct RecoveryRequest<'a> {
    pub state: ChainReviewState,
    pub candidate: VerifiedCandidate,
    /// Tip work of the chain currently held. Compared against the challenger's CLAIM to decide
    /// whether investigating is worthwhile.
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
pub fn authorize_candidate_validation(request: RecoveryRequest<'_>) -> Result<CandidateValidationPermit, RecoveryError> {
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

    // The chain must be real: a validated pruning proof, so a header sync is not spent on something
    // fabricated.
    if request.candidate.verified_blue_work.is_none() {
        return Err(RecoveryError::ProofNotValidated);
    }

    // And it must at least CLAIM to beat what is held. A claim decides what to look at, never what
    // to adopt — the adoption decision happens at the commit barrier, on tip work this node
    // validated for itself.
    //
    // Superiority is deliberately not required here. Requiring it at permit time meant comparing
    // pruning-point work, which is a function of depth rather than of branch: measured across two
    // real hosts the challenger and the incumbent came out exactly Equal, so no permit could ever
    // issue and the node fail-closed to quarantine instead of converging. Verified tip work is what
    // separates them, and it cannot be obtained without syncing the chain — which is what the
    // permit authorises.
    if request.candidate.claimed_tip_blue_work <= request.provisional_blue_work {
        return Err(RecoveryError::NotWorthInvestigating);
    }

    // The reservation is what makes this a handoff rather than a free-for-all; a permit without one
    // would let any verified candidate cross the boundary at any moment.
    if request.reserved_candidate != Some(request.candidate.id) {
        return Err(RecoveryError::NoReservation);
    }

    if request.state.switch_count >= request.switch_limit {
        return Err(RecoveryError::SwitchLimitReached);
    }

    Ok(CandidateValidationPermit {
        candidate_id: request.candidate.id,
        pruning_point: request.candidate.id.pruning_point,
        adoption_generation: request.state.adoption_generation,
        switch_generation: request.state.switch_count,
        proof_hash: request.candidate.proof_hash,
        trusted_checkpoint: request.checkpoint.copied(),
    })
}

/// Authority to replace the provisional chain with one this node has validated to its tip.
///
/// Issued only after staging validation, from figures this node computed for both sides. One-shot
/// and bound to the exact validated result, so a permit earned by validating A100 cannot be spent
/// on a later, different A150.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateAdoptionPermit {
    pub candidate_id: CandidateId,
    /// The tip this node actually validated — not the one the peer advertised.
    pub verified_tip: BlockHash,
    /// Blue work recomputed by staging over that tip.
    pub verified_blue_work: BlueWorkType,
    /// The incumbent this was judged against.
    ///
    /// Bound because the incumbent MOVES: while staging validates a challenger, the provisional
    /// chain keeps taking blocks from its own peers. A permit that named only the challenger could
    /// be earned when A beat B and spent after B had overtaken it. Binding both sides makes a
    /// permit a statement about a comparison, not about a candidate.
    pub defender_tip: BlockHash,
    pub defender_blue_work: BlueWorkType,
    pub proof_hash: Hash,
    pub adoption_generation: u64,
    pub switch_generation: u32,
}

/// What staging established about a candidate after validating it to the tip. Every field is this
/// node's own computation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedCandidate {
    pub id: CandidateId,
    pub verified_tip: BlockHash,
    pub verified_blue_work: BlueWorkType,
    pub proof_hash: Hash,
}

/// Why an adoption was refused. Distinct from [`RecoveryError`]: these are verdicts on evidence,
/// not gate conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdoptionError {
    /// The node participated while the candidate was being validated. The door shut mid-flight.
    HasBeenReady,
    /// The validation permit does not correspond to this candidate, proof, or generation.
    PermitDoesNotMatch,
    /// Validated and genuinely worse. An ordinary negative result: keep the incumbent.
    Weaker,
    /// Validated and exactly equal. NOT adopted and not simply dropped either — two chains this
    /// node cannot separate is precisely the situation it must not resolve by itself.
    Equal,
}

/// Decide whether a validated candidate may replace the provisional chain.
///
/// Both figures come from this node: the candidate's from staging, the incumbent's from the chain
/// being held. This is the comparison pruning-point work could not make — measured across two real
/// hosts, two different branches had identical pruning-point work, because that figure follows
/// depth rather than branch.
pub fn authorize_candidate_adoption(
    state: &ChainReviewState,
    validation_permit: &CandidateValidationPermit,
    validated: &ValidatedCandidate,
    defender: &ChainTip,
) -> Result<CandidateAdoptionPermit, AdoptionError> {
    if state.ever_ready {
        return Err(AdoptionError::HasBeenReady);
    }
    // The adoption permit may only be earned by the validation permit that authorised the work, for
    // the same chain, the same proof, and the same moment in this node's history.
    if validation_permit.candidate_id != validated.id
        || validation_permit.proof_hash != validated.proof_hash
        || validation_permit.adoption_generation != state.adoption_generation
        || validation_permit.switch_generation != state.switch_count
    {
        return Err(AdoptionError::PermitDoesNotMatch);
    }
    // The canonical fork choice, not a private rule invented here.
    //
    // GHOSTDAG's `SortableBlock` orders by blue work and breaks ties on the block hash
    // (`consensus/src/processes/ghostdag/ordering.rs`). Comparing raw blue work alone threw that
    // deterministic tie-break away and turned every work tie into an impasse — which is not
    // hypothetical: two real branches measured identical work, and a node that should have had a
    // decisive answer quarantined instead.
    //
    // With the tie-break, `Equal` means same work AND same hash: the same chain, so there is
    // nothing to choose between. A genuine fork always resolves.
    let challenger = (validated.verified_blue_work, validated.verified_tip);
    let incumbent = (defender.blue_work, defender.tip);
    match challenger.cmp(&incumbent) {
        std::cmp::Ordering::Greater => Ok(CandidateAdoptionPermit {
            candidate_id: validated.id,
            verified_tip: validated.verified_tip,
            verified_blue_work: validated.verified_blue_work,
            defender_tip: defender.tip,
            defender_blue_work: defender.blue_work,
            proof_hash: validated.proof_hash,
            adoption_generation: state.adoption_generation,
            switch_generation: state.switch_count,
        }),
        std::cmp::Ordering::Equal => Err(AdoptionError::Equal),
        std::cmp::Ordering::Less => Err(AdoptionError::Weaker),
    }
}

/// A chain tip and its work, as this node computed them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainTip {
    pub tip: BlockHash,
    pub blue_work: BlueWorkType,
}

impl CandidateAdoptionPermit {
    /// Whether this permit still describes the situation.
    ///
    /// Checked again immediately before the irreversible step, because the incumbent can advance
    /// between the comparison and the commit.
    pub fn still_applies(&self, state: &ChainReviewState, defender_now: &ChainTip) -> bool {
        !state.ever_ready
            && self.adoption_generation == state.adoption_generation
            && self.switch_generation == state.switch_count
            && self.defender_tip == defender_now.tip
            && self.defender_blue_work == defender_now.blue_work
    }
}

impl CandidateValidationPermit {
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
            verified_blue_work: Some(work(500)),
            claimed_tip_blue_work: work(500),
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
        let permit = authorize_candidate_validation(request(reviewing(), candidate())).unwrap();
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
        assert_eq!(authorize_candidate_validation(request(state, candidate())), Err(RecoveryError::HasBeenReady));

        // Not even from a state that otherwise qualifies.
        state.participation = ChainParticipation::Quarantined;
        assert_eq!(authorize_candidate_validation(request(state, candidate())), Err(RecoveryError::HasBeenReady));
    }

    #[test]
    fn a_participating_node_is_refused() {
        let mut state = reviewing();
        state.participation = ChainParticipation::Ready;
        assert_eq!(authorize_candidate_validation(request(state, candidate())), Err(RecoveryError::AlreadyParticipating));
    }

    #[test]
    fn every_state_that_withholds_participation_qualifies() {
        for participation in [ChainParticipation::IbdRunning, ChainParticipation::CandidateReview, ChainParticipation::Quarantined] {
            let mut state = reviewing();
            state.participation = participation;
            assert!(
                authorize_candidate_validation(request(state, candidate())).is_ok(),
                "{participation:?} withholds participation, so its chain is still provisional"
            );
        }
    }

    #[test]
    fn rules_are_checked_before_quality() {
        let mut wrong_genesis = candidate();
        wrong_genesis.genesis_hash = bhash(777);
        wrong_genesis.claimed_tip_blue_work = work(u64::MAX);
        assert_eq!(authorize_candidate_validation(request(reviewing(), wrong_genesis)), Err(RecoveryError::WrongGenesis));

        let mut wrong_params = candidate();
        wrong_params.consensus_params_id = hash(777);
        wrong_params.claimed_tip_blue_work = work(u64::MAX);
        assert_eq!(authorize_candidate_validation(request(reviewing(), wrong_params)), Err(RecoveryError::WrongConsensusParams));
    }

    #[test]
    fn a_chain_that_does_not_even_claim_to_win_is_not_investigated() {
        // The cheapest filter, and the only thing a claim is allowed to decide. Adoption is settled
        // later, at the commit barrier, on tip work this node validated itself.
        let mut equal = candidate();
        equal.claimed_tip_blue_work = work(100);
        assert_eq!(authorize_candidate_validation(request(reviewing(), equal)), Err(RecoveryError::NotWorthInvestigating));

        let mut worse = candidate();
        worse.claimed_tip_blue_work = work(99);
        assert_eq!(authorize_candidate_validation(request(reviewing(), worse)), Err(RecoveryError::NotWorthInvestigating));
    }

    #[test]
    fn an_unproven_chain_is_never_investigated() {
        // A header sync is expensive. Requiring a validated pruning proof first is what stops a
        // fabricated chain from buying one.
        let mut unproven = candidate();
        unproven.verified_blue_work = None;
        assert_eq!(authorize_candidate_validation(request(reviewing(), unproven)), Err(RecoveryError::ProofNotValidated));
    }

    #[test]
    fn pruning_point_work_no_longer_gates_the_permit() {
        // The regression this ordering exists to fix: across two real hosts the challenger and the
        // incumbent had IDENTICAL pruning-point work (80289507 each), because that figure follows
        // depth rather than branch. Gating on it meant no permit could issue and the node
        // quarantined instead of converging. A candidate whose pruning-point work merely ties must
        // still be investigable.
        let mut tied = candidate();
        tied.verified_blue_work = Some(work(100));
        tied.claimed_tip_blue_work = work(101);
        assert!(authorize_candidate_validation(request(reviewing(), tied)).is_ok());
    }

    #[test]
    fn a_permit_requires_a_reservation_naming_this_chain() {
        // Without this, any verified candidate could cross the boundary whenever it liked, and the
        // handoff would stop being a handoff.
        let mut req = request(reviewing(), candidate());
        req.reserved_candidate = None;
        assert_eq!(authorize_candidate_validation(req), Err(RecoveryError::NoReservation));

        let mut req = request(reviewing(), candidate());
        req.reserved_candidate = Some(CandidateId { pruning_point: bhash(90), virtual_selected_parent: bhash(91) });
        assert_eq!(authorize_candidate_validation(req), Err(RecoveryError::NoReservation));
    }

    #[test]
    fn the_switch_limit_is_enforced_here_too() {
        let mut state = reviewing();
        state.switch_count = 5;
        assert_eq!(authorize_candidate_validation(request(state, candidate())), Err(RecoveryError::SwitchLimitReached));
    }

    #[test]
    fn a_checkpoint_must_be_satisfied_not_merely_present() {
        let checkpoint = TrustedCheckpoint { daa_score: 10, block_hash: bhash(42), consensus_params_id: hash(LOCAL_PARAMS) };

        let mut req = request(reviewing(), candidate());
        req.checkpoint = Some(&checkpoint);
        assert_eq!(authorize_candidate_validation(req), Err(RecoveryError::CheckpointIncompatible), "unknown ancestry is not consent");

        let mut descending = candidate();
        descending.descends_from_checkpoint = Some(false);
        let mut req = request(reviewing(), descending);
        req.checkpoint = Some(&checkpoint);
        assert_eq!(authorize_candidate_validation(req), Err(RecoveryError::CheckpointIncompatible));

        let mut descending = candidate();
        descending.descends_from_checkpoint = Some(true);
        let mut req = request(reviewing(), descending);
        req.checkpoint = Some(&checkpoint);
        assert!(authorize_candidate_validation(req).is_ok());
    }

    #[test]
    fn a_checkpoint_for_other_rules_refuses_rather_than_being_ignored() {
        let checkpoint = TrustedCheckpoint { daa_score: 10, block_hash: bhash(42), consensus_params_id: hash(4242) };
        let mut descending = candidate();
        descending.descends_from_checkpoint = Some(true);
        let mut req = request(reviewing(), descending);
        req.checkpoint = Some(&checkpoint);
        assert_eq!(authorize_candidate_validation(req), Err(RecoveryError::WrongConsensusParams));
    }

    fn validated(work: u64) -> ValidatedCandidate {
        ValidatedCandidate {
            id: candidate().id,
            verified_tip: bhash(555),
            verified_blue_work: super::tests::work(work),
            proof_hash: hash(99),
        }
    }

    #[test]
    fn adoption_is_earned_only_by_strictly_better_verified_work() {
        // The decision the whole two-stage split exists to make, on figures this node computed for
        // both sides rather than on anything a peer said.
        let state = reviewing();
        let permit = authorize_candidate_validation(request(state, candidate())).unwrap();

        let defender = ChainTip { tip: bhash(7), blue_work: work(100) };
        assert!(authorize_candidate_adoption(&state, &permit, &validated(200), &defender).is_ok());
        assert_eq!(
            authorize_candidate_adoption(&state, &permit, &validated(50), &defender),
            Err(AdoptionError::Weaker),
            "validated and worse is an ordinary negative result"
        );
    }

    #[test]
    fn two_chains_that_cannot_be_separated_are_not_adopted() {
        // Exactly the case the real-host run hit, where both branches measured identical. Equal is
        // not a tie to be broken by whoever asked first — it is the node's cue to stop.
        let state = reviewing();
        let permit = authorize_candidate_validation(request(state, candidate())).unwrap();
        // Equal work alone is NOT an impasse any more: the canonical fork choice breaks ties on the
        // block hash, so two different chains always resolve one way or the other. Which way is not
        // the point and is not assumed here — the hash order is derived, not guessed.
        let challenger = validated(100);
        let other_tip = bhash(9999);
        assert_ne!(other_tip, challenger.verified_tip);
        let expected = challenger.verified_tip > other_tip;
        let against_other = ChainTip { tip: other_tip, blue_work: work(100) };
        assert_eq!(
            authorize_candidate_adoption(&state, &permit, &challenger, &against_other).is_ok(),
            expected,
            "equal work must resolve on the hash, the way GHOSTDAG's SortableBlock does — not deadlock"
        );

        // Same work and same tip is the same chain — nothing to choose between, and the only case
        // that is genuinely inseparable.
        let itself = ChainTip { tip: challenger.verified_tip, blue_work: work(100) };
        assert_eq!(authorize_candidate_adoption(&state, &permit, &challenger, &itself), Err(AdoptionError::Equal));
    }

    #[test]
    fn a_permit_does_not_survive_the_incumbent_moving() {
        // The incumbent keeps taking blocks from its own peers throughout a staging validation. A
        // permit earned when A beat B must not be spendable after B has overtaken it.
        let state = reviewing();
        let vpermit = authorize_candidate_validation(request(state, candidate())).unwrap();
        let defender = ChainTip { tip: bhash(7), blue_work: work(100) };
        let adoption = authorize_candidate_adoption(&state, &vpermit, &validated(200), &defender).unwrap();

        assert!(adoption.still_applies(&state, &defender));
        let advanced = ChainTip { tip: bhash(8), blue_work: work(300) };
        assert!(!adoption.still_applies(&state, &advanced), "the comparison it recorded no longer holds");
    }

    #[test]
    fn a_validation_permit_cannot_be_spent_on_a_different_result() {
        // A permit earned by validating one chain must not adopt another, nor the same chain at a
        // different tip after the situation moved on.
        let state = reviewing();
        let defender = ChainTip { tip: bhash(7), blue_work: work(100) };
        let permit = authorize_candidate_validation(request(state, candidate())).unwrap();

        let mut other_chain = validated(200);
        other_chain.id = CandidateId { pruning_point: bhash(90), virtual_selected_parent: bhash(91) };
        assert_eq!(authorize_candidate_adoption(&state, &permit, &other_chain, &defender), Err(AdoptionError::PermitDoesNotMatch));

        let mut other_proof = validated(200);
        other_proof.proof_hash = hash(4242);
        assert_eq!(authorize_candidate_adoption(&state, &permit, &other_proof, &defender), Err(AdoptionError::PermitDoesNotMatch));

        let mut moved_on = state;
        moved_on.adoption_generation += 1;
        assert_eq!(
            authorize_candidate_adoption(&moved_on, &permit, &validated(200), &defender),
            Err(AdoptionError::PermitDoesNotMatch)
        );
    }

    #[test]
    fn adoption_stops_if_the_node_participated_while_validating() {
        // The one-way door can close mid-flight: a review floor can elapse during a long staging
        // sync. A permit issued before that must not still be spendable after.
        let state = reviewing();
        let permit = authorize_candidate_validation(request(state, candidate())).unwrap();
        let mut participated = state;
        participated.ever_ready = true;
        assert_eq!(
            authorize_candidate_adoption(
                &participated,
                &permit,
                &validated(u64::MAX),
                &ChainTip { tip: bhash(7), blue_work: work(1) }
            ),
            Err(AdoptionError::HasBeenReady)
        );
    }

    #[test]
    fn a_permit_is_bound_to_the_situation_that_issued_it() {
        let state = reviewing();
        let permit = authorize_candidate_validation(request(state, candidate())).unwrap();
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
