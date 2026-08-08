//! Which chains are on offer, and how far this node has got in checking them.
//!
//! Before this, IBD adopted whichever peer took the latch first and never looked at the others —
//! their offers were discarded at the relay guard, unfetched and uncounted. This registry is what
//! makes a comparison possible: it keeps every chain peers advertise while an IBD runs, keyed by
//! the chain rather than by the peer.
//!
//! **Keyed by chain, not by peer.** Five peers on the same branch are one candidate with five
//! sources, so losing a peer is a source failover, not a reason to reconsider which chain to
//! follow. Keying by peer would restart the chain decision every time a connection dropped.
//!
//! **Claims and verdicts never share a type.** Everything a peer says arrives as
//! [`ClaimedBlueWork`], which cannot be compared against a verified blue work by accident: the
//! types do not mix. A claim may decide *what order to check candidates in* and nothing else. It
//! may not preempt a running IBD, block a commit, or select a chain — a peer that mines one valid
//! block can write any number in a header's `blue_work` field, and only contextual validation
//! (`UnexpectedHeaderBlueWork`) catches it.

use std::{
    collections::{HashMap, hash_map::Entry},
    time::{Duration, Instant},
};

use kaspa_consensus_core::{BlockHash, BlueWorkType, header::Header};
use kaspa_p2p_lib::PeerKey;

/// How long an unrefreshed candidate stays in the registry.
///
/// Peers churn and chains move; an offer nobody has repeated for this long describes a state that
/// may no longer exist, and keeping it would let stale claims influence check order forever.
pub const CANDIDATE_TTL: Duration = Duration::from_secs(600);

/// Ceiling on distinct chains tracked at once.
///
/// A real network offers a handful of candidates — one canonical chain plus stragglers and
/// short-lived splits. Many more than that means either a churn storm or a peer manufacturing
/// candidates, and neither deserves unbounded memory. When full, a new candidate may still evict
/// the least-recently-seen one, so a flood cannot pin the registry to junk it arrived with first.
pub const MAX_CANDIDATES: usize = 16;

/// Minimum gap between candidate-summary requests to the same peer.
///
/// The summary is cheap to serve, but "cheap" times "as often as a peer likes" is still a DoS.
/// This is the rate limit the goal requires; it is per peer, so one loud peer cannot spend another
/// peer's budget.
pub const PEER_SUMMARY_COOLDOWN: Duration = Duration::from_secs(30);

/// Blue work as a peer stated it, which is not evidence that it is true.
///
/// A header commits to its `blue_work` through PoW, so the value cannot be edited after mining —
/// but committing to a number is not deriving it correctly from parents. That check is contextual
/// validation, and it happens long after this type is used.
///
/// The newtype exists so the compiler enforces what a comment cannot: this value has no `Ord`
/// against a verified blue work, and no way to reach a decision reserved for verified data. To use
/// it you must say so explicitly via [`ClaimedBlueWork::for_priority_only`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClaimedBlueWork(BlueWorkType);

impl ClaimedBlueWork {
    pub fn new(value: BlueWorkType) -> Self {
        Self(value)
    }

    /// Unwrap for ordering candidates by how promising they look.
    ///
    /// The name is the documentation. Legitimate use is deciding whom to ask for a pruning proof
    /// first; a liar claiming the maximum only buys the privilege of being checked first, and
    /// fails that check. Using it to preempt, to block a commit, or to select a chain hands that
    /// liar the chain instead.
    pub fn for_priority_only(&self) -> BlueWorkType {
        self.0
    }
}

/// How far a candidate has got through verification. A candidate may only be adopted from
/// [`CandidateValidation::ProofValidated`]; every earlier state is hearsay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateValidation {
    /// A peer named a hash. Nothing has been asked or checked.
    Observed,
    /// A summary arrived: we know what the peer says its chain is worth.
    SummaryReceived { claimed_blue_work: ClaimedBlueWork },
    /// A pruning proof is being fetched and validated for this candidate.
    ProofValidating,
    /// A pruning proof validated in the context of current consensus. `verified_blue_work` is the
    /// first number about this candidate that this node computed rather than received.
    ProofValidated { verified_blue_work: BlueWorkType },
    /// Checked and refused. Kept rather than dropped so the same chain is not re-fetched on the
    /// next advertisement.
    Rejected { reason: CandidateRejectReason },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateRejectReason {
    /// Built on a different genesis.
    WrongGenesis,
    /// Built under different consensus rules.
    WrongConsensusParams,
    /// Does not descend from the operator's trusted checkpoint.
    CheckpointIncompatible,
    /// Its pruning proof failed validation — the claim was not backed.
    InvalidProof,
}

/// Identity of a chain being offered.
///
/// Pruning point plus virtual selected parent: peers on the same branch at the same tip produce
/// the same id and collapse into one candidate with several sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CandidateId {
    pub pruning_point: BlockHash,
    pub virtual_selected_parent: BlockHash,
}

#[derive(Clone, Debug)]
pub struct IbdCandidate {
    pub id: CandidateId,
    /// Every peer offering this chain. More than one is a liveness benefit — a source to fail over
    /// to — and **not** a vote: N peers is N sybils just as easily.
    pub sources: Vec<PeerKey>,
    pub validation: CandidateValidation,
    pub first_seen: Instant,
    pub last_seen: Instant,
}

impl IbdCandidate {
    /// Ordering key for which candidate to verify next. `None` for candidates already refused.
    ///
    /// Verified work outranks any claim, so a candidate this node has actually checked is never
    /// displaced in the queue by one that merely says it is heavier.
    pub fn verification_priority(&self) -> Option<(u8, BlueWorkType)> {
        match self.validation {
            CandidateValidation::ProofValidated { verified_blue_work } => Some((2, verified_blue_work)),
            CandidateValidation::SummaryReceived { claimed_blue_work } => Some((1, claimed_blue_work.for_priority_only())),
            CandidateValidation::Observed | CandidateValidation::ProofValidating => Some((0, BlueWorkType::from_u64(0))),
            CandidateValidation::Rejected { .. } => None,
        }
    }

    /// The only number about this candidate this node derived itself.
    pub fn verified_blue_work(&self) -> Option<BlueWorkType> {
        match self.validation {
            CandidateValidation::ProofValidated { verified_blue_work } => Some(verified_blue_work),
            _ => None,
        }
    }
}

/// The chains on offer and the budget for asking about them.
#[derive(Debug, Default)]
pub struct IbdCandidateRegistry {
    candidates: HashMap<CandidateId, IbdCandidate>,
    /// Last time each peer was asked for a summary, for the per-peer rate limit.
    last_asked: HashMap<PeerKey, Instant>,
}

impl IbdCandidateRegistry {
    /// Note that `peer` advertised something, without knowing yet what chain it belongs to.
    ///
    /// An inv hash is not a chain: it may be a side block, a merge block, or a newer block on the
    /// chain already being synced. All this records is that the peer is worth asking.
    pub fn observe_peer(&mut self, peer: PeerKey, now: Instant) {
        self.last_asked.entry(peer).or_insert(now - PEER_SUMMARY_COOLDOWN);
    }

    /// Whether `peer` may be asked for a summary now, claiming the budget if so.
    pub fn claim_summary_request(&mut self, peer: PeerKey, now: Instant) -> bool {
        match self.last_asked.entry(peer) {
            Entry::Occupied(mut e) => {
                if now.duration_since(*e.get()) < PEER_SUMMARY_COOLDOWN {
                    return false;
                }
                e.insert(now);
                true
            }
            Entry::Vacant(e) => {
                e.insert(now);
                true
            }
        }
    }

    /// Record what a peer says it is on. Returns the candidate's id.
    ///
    /// Merges into an existing candidate when the chain matches, so this is also how a second
    /// source is added. A summary never downgrades a candidate that has already been verified —
    /// otherwise a peer could erase a validated result by re-advertising.
    pub fn observe_summary(&mut self, peer: PeerKey, header: &Header, pruning_point: BlockHash, now: Instant) -> CandidateId {
        let id = CandidateId { pruning_point, virtual_selected_parent: header.hash };
        let claimed = ClaimedBlueWork::new(header.blue_work);

        if let Some(existing) = self.candidates.get_mut(&id) {
            existing.last_seen = now;
            if !existing.sources.contains(&peer) {
                existing.sources.push(peer);
            }
            if matches!(existing.validation, CandidateValidation::Observed) {
                existing.validation = CandidateValidation::SummaryReceived { claimed_blue_work: claimed };
            }
            return id;
        }

        self.make_room(now);
        self.candidates.insert(
            id,
            IbdCandidate {
                id,
                sources: vec![peer],
                validation: CandidateValidation::SummaryReceived { claimed_blue_work: claimed },
                first_seen: now,
                last_seen: now,
            },
        );
        id
    }

    pub fn set_validation(&mut self, id: CandidateId, validation: CandidateValidation) {
        if let Some(c) = self.candidates.get_mut(&id) {
            c.validation = validation;
        }
    }

    pub fn get(&self, id: &CandidateId) -> Option<&IbdCandidate> {
        self.candidates.get(id)
    }

    /// Candidates worth checking, most promising first. Rejected ones are omitted.
    pub fn by_verification_priority(&self) -> Vec<&IbdCandidate> {
        let mut out: Vec<_> = self.candidates.values().filter(|c| c.verification_priority().is_some()).collect();
        out.sort_by_key(|c| std::cmp::Reverse(c.verification_priority()));
        out
    }

    /// The best candidate this node has actually verified, if any.
    ///
    /// This — and only this — is what a chain decision may be based on.
    pub fn best_verified(&self) -> Option<&IbdCandidate> {
        self.candidates.values().filter(|c| c.verified_blue_work().is_some()).max_by_key(|c| c.verified_blue_work().unwrap())
    }

    /// A verified candidate strictly better than `current`, if one exists.
    ///
    /// Strictly: equal work is not a reason to switch, and an unverified candidate is never a
    /// reason to switch no matter what it claims.
    pub fn verified_superior_to(&self, current: BlueWorkType) -> Option<&IbdCandidate> {
        self.best_verified().filter(|c| c.verified_blue_work().is_some_and(|w| w > current))
    }

    /// Candidates that are neither verified nor refused — the ones a commit decision cannot
    /// account for, because nobody has checked them.
    pub fn unresolved(&self) -> Vec<&IbdCandidate> {
        self.candidates
            .values()
            .filter(|c| {
                matches!(
                    c.validation,
                    CandidateValidation::Observed | CandidateValidation::SummaryReceived { .. } | CandidateValidation::ProofValidating
                )
            })
            .collect()
    }

    /// Drop a peer as a source. The candidate itself survives while another peer offers it — the
    /// chain did not become wrong because one connection dropped.
    pub fn forget_peer(&mut self, peer: &PeerKey) {
        self.last_asked.remove(peer);
        for candidate in self.candidates.values_mut() {
            candidate.sources.retain(|p| p != peer);
        }
        self.candidates.retain(|_, c| !c.sources.is_empty());
    }

    pub fn expire(&mut self, now: Instant) {
        self.candidates.retain(|_, c| now.duration_since(c.last_seen) < CANDIDATE_TTL);
        self.last_asked.retain(|_, t| now.duration_since(*t) < CANDIDATE_TTL);
    }

    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    pub fn clear(&mut self) {
        self.candidates.clear();
        self.last_asked.clear();
    }

    /// Make space for a new candidate: expire first, then evict the least-recently-seen. Verified
    /// candidates are evicted last, since re-verifying costs a pruning proof.
    fn make_room(&mut self, now: Instant) {
        self.expire(now);
        while self.candidates.len() >= MAX_CANDIDATES {
            let victim = self
                .candidates
                .values()
                .min_by_key(|c| (c.verified_blue_work().is_some(), c.last_seen))
                .map(|c| c.id)
                .expect("non-empty while over the cap");
            self.candidates.remove(&victim);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{blockhash::ORIGIN, header::Header};
    use kaspa_utils::networking::{IpAddress, PeerId};
    use std::net::IpAddr;
    use uuid::Uuid;

    pub(super) fn peer(n: u8) -> PeerKey {
        PeerKey::new(PeerId::new(Uuid::from_u128(n as u128)), IpAddress::new(IpAddr::from([10, 0, 0, n])))
    }

    pub(super) fn header(tip: u64, blue_work: u64) -> Header {
        let mut h = Header::from_precomputed_hash(BlockHash::from_u64_word(tip), vec![ORIGIN]);
        h.blue_work = BlueWorkType::from_u64(blue_work);
        h
    }

    pub(super) fn pp(n: u64) -> BlockHash {
        BlockHash::from_u64_word(1000 + n)
    }

    #[test]
    fn peers_on_the_same_chain_are_one_candidate_with_many_sources() {
        // Keying by chain rather than by peer is what makes a lost connection a source failover
        // instead of a reason to redo the chain decision.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let h = header(7, 100);

        let a = r.observe_summary(peer(1), &h, pp(1), now);
        let b = r.observe_summary(peer(2), &h, pp(1), now);
        let c = r.observe_summary(peer(3), &h, pp(1), now);
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(r.len(), 1);
        assert_eq!(r.get(&a).unwrap().sources.len(), 3);
    }

    #[test]
    fn losing_a_source_keeps_the_candidate() {
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let h = header(7, 100);
        let id = r.observe_summary(peer(1), &h, pp(1), now);
        r.observe_summary(peer(2), &h, pp(1), now);

        r.forget_peer(&peer(1));
        assert_eq!(r.get(&id).unwrap().sources, vec![peer(2)], "failover, not a chain decision");

        // The last source going away does drop it: nothing is offering that chain any more.
        r.forget_peer(&peer(2));
        assert!(r.is_empty());
    }

    #[test]
    fn a_huge_claim_never_becomes_a_verified_candidate() {
        // The attack this shape exists to defeat: a peer that writes an enormous blue_work into a
        // header it mined. It may be checked first — and that is all it may ever buy.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let liar = r.observe_summary(peer(1), &header(1, u64::MAX), pp(1), now);
        let honest = r.observe_summary(peer(2), &header(2, 50), pp(2), now);

        assert_eq!(r.by_verification_priority()[0].id, liar, "a big claim earns first place in the queue");
        assert!(r.best_verified().is_none(), "and nothing else");
        assert!(r.verified_superior_to(BlueWorkType::from_u64(1)).is_none());

        // Only a validated proof produces a number this node may act on.
        r.set_validation(honest, CandidateValidation::ProofValidated { verified_blue_work: BlueWorkType::from_u64(50) });
        assert_eq!(r.best_verified().unwrap().id, honest);
        assert_eq!(r.verified_superior_to(BlueWorkType::from_u64(1)).unwrap().id, honest);
        // Still not the liar, whose claim dwarfs it.
        assert_ne!(r.best_verified().unwrap().id, liar);
    }

    #[test]
    fn verified_candidates_outrank_louder_unverified_ones_in_the_queue() {
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let checked = r.observe_summary(peer(1), &header(1, 10), pp(1), now);
        r.observe_summary(peer(2), &header(2, u64::MAX), pp(2), now);
        r.set_validation(checked, CandidateValidation::ProofValidated { verified_blue_work: BlueWorkType::from_u64(10) });

        assert_eq!(r.by_verification_priority()[0].id, checked, "a checked candidate is not displaced by a loud claim");
    }

    #[test]
    fn superiority_must_be_strict() {
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(1, 100), pp(1), now);
        r.set_validation(id, CandidateValidation::ProofValidated { verified_blue_work: BlueWorkType::from_u64(100) });

        // Equal work is not a reason to abandon a chain mid-sync.
        assert!(r.verified_superior_to(BlueWorkType::from_u64(100)).is_none());
        assert!(r.verified_superior_to(BlueWorkType::from_u64(99)).is_some());
    }

    #[test]
    fn rejected_candidates_leave_the_queue_but_stay_known() {
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(1, 100), pp(1), now);
        r.set_validation(id, CandidateValidation::Rejected { reason: CandidateRejectReason::InvalidProof });

        assert!(r.by_verification_priority().is_empty(), "not worth checking again");
        assert!(r.unresolved().is_empty(), "and not something the commit barrier must wait on");
        assert!(r.get(&id).is_some(), "but remembered, so re-advertising does not re-fetch it");
    }

    #[test]
    fn unresolved_counts_only_what_nobody_checked() {
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let a = r.observe_summary(peer(1), &header(1, 10), pp(1), now);
        let b = r.observe_summary(peer(2), &header(2, 20), pp(2), now);
        r.observe_summary(peer(3), &header(3, 30), pp(3), now);

        assert_eq!(r.unresolved().len(), 3);
        r.set_validation(a, CandidateValidation::ProofValidated { verified_blue_work: BlueWorkType::from_u64(10) });
        r.set_validation(b, CandidateValidation::Rejected { reason: CandidateRejectReason::InvalidProof });
        assert_eq!(r.unresolved().len(), 1, "only the one still unexamined");
    }

    #[test]
    fn a_peer_cannot_be_asked_again_before_the_cooldown() {
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        assert!(r.claim_summary_request(peer(1), now));
        assert!(!r.claim_summary_request(peer(1), now + Duration::from_secs(1)));
        assert!(r.claim_summary_request(peer(1), now + PEER_SUMMARY_COOLDOWN));
        // Per peer: one loud peer must not spend another's budget.
        assert!(r.claim_summary_request(peer(2), now + Duration::from_secs(1)));
    }

    #[test]
    fn the_candidate_count_is_capped() {
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        for n in 0..100u64 {
            r.observe_summary(peer(n as u8), &header(n, n), pp(n), now + Duration::from_millis(n));
        }
        assert_eq!(r.len(), MAX_CANDIDATES);
    }

    #[test]
    fn eviction_prefers_to_keep_what_was_verified() {
        // Re-verifying costs a pruning proof, so a checked candidate is the last thing to drop.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let precious = r.observe_summary(peer(0), &header(0, 5), pp(0), now);
        r.set_validation(precious, CandidateValidation::ProofValidated { verified_blue_work: BlueWorkType::from_u64(5) });

        for n in 1..100u64 {
            r.observe_summary(peer(n as u8), &header(n, n), pp(n), now + Duration::from_millis(n));
        }
        assert!(r.get(&precious).is_some(), "a verified candidate survives a flood of fresh claims");
    }

    #[test]
    fn stale_candidates_expire() {
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        r.observe_summary(peer(1), &header(1, 10), pp(1), now);
        r.expire(now + CANDIDATE_TTL - Duration::from_secs(1));
        assert_eq!(r.len(), 1);
        r.expire(now + CANDIDATE_TTL);
        assert!(r.is_empty(), "an offer nobody has repeated describes a state that may not exist");
    }

    #[test]
    fn a_re_advertisement_cannot_erase_a_verification() {
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let h = header(1, 10);
        let id = r.observe_summary(peer(1), &h, pp(1), now);
        r.set_validation(id, CandidateValidation::ProofValidated { verified_blue_work: BlueWorkType::from_u64(10) });

        r.observe_summary(peer(2), &h, pp(1), now + Duration::from_secs(1));
        assert!(r.get(&id).unwrap().verified_blue_work().is_some(), "a peer must not be able to undo a check by re-offering");
    }
}

/// The commit barrier's verdict: may this node replace its active consensus?
///
/// Extracted from the flow so the policy can be tested without a consensus, a router, or a peer.
/// The decision is the security-critical part; fetching the numbers it operates on is not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitVerdict {
    Allow,
    /// A chain the operator did not vouch for. Refused before any work comparison.
    RefuseCheckpointMissing,
    /// The checkpoint describes rules this build does not run, so it cannot be acted on.
    RefuseCheckpointParamsMismatch,
    /// A candidate this node VERIFIED beats what is staged.
    RefuseVerifiedSuperior {
        candidate: CandidateId,
        verified_blue_work: BlueWorkType,
    },
    /// Chains are on offer that nobody could verify. Committing would pick by arrival order.
    RefuseUnresolved {
        count: usize,
    },
}

/// What the barrier knows at the moment of decision.
pub struct CommitInputs<'a> {
    /// Blue work of the staged chain's headers-selected tip — computed here, not received.
    pub staged_blue_work: BlueWorkType,
    /// Whether the staged chain descends from the operator's checkpoint. `None` when no checkpoint
    /// is configured.
    pub descends_from_checkpoint: Option<bool>,
    /// Whether the configured checkpoint's params id matches this build. `None` when unconfigured.
    pub checkpoint_params_match: Option<bool>,
    /// Unresolved candidates whose tip the staged chain does **not** already contain.
    ///
    /// Containment is what separates a rival branch from ordinary traffic. Peers on the chain being
    /// synced keep relaying, and each new tip is a new candidate id — so counting every unverified
    /// candidate would refuse essentially every IBD on a healthy network. A candidate the staged
    /// chain already knows about is the same history seen a little further along, not a competitor.
    pub unresolved_competing: usize,
    pub registry: &'a IbdCandidateRegistry,
}

/// Decide whether the staged chain may become this node's active consensus.
///
/// Order matters and is not arbitrary:
///
/// 1. **Checkpoint rules, then checkpoint ancestry.** Work is what an attacker manufactures, so
///    admissibility is settled before quality. A checkpoint about rules this build does not run is
///    unusable, which is a different failure from a chain that simply lacks it.
/// 2. **A verified superior candidate.** Verified, never claimed — otherwise cancelling someone
///    else's IBD costs a liar nothing. Strictly superior, because equal work is not a reason to
///    abandon a sync already in progress.
/// 3. **Anything left unverified.** This is the case that fixed testnet-22 in place: a chain was on
///    offer, nobody compared it, and the node committed anyway because its peer relayed first.
///    Refusing here is the whole point — silence about a candidate is not evidence against it.
///    Counted by the caller, which alone can tell a rival branch from a peer on this same chain
///    that has simply moved on (see [`CommitInputs::unresolved_competing`]).
pub fn decide_commit(inputs: CommitInputs<'_>) -> CommitVerdict {
    if inputs.checkpoint_params_match == Some(false) {
        return CommitVerdict::RefuseCheckpointParamsMismatch;
    }
    if inputs.descends_from_checkpoint == Some(false) {
        return CommitVerdict::RefuseCheckpointMissing;
    }
    if let Some(better) = inputs.registry.verified_superior_to(inputs.staged_blue_work) {
        return CommitVerdict::RefuseVerifiedSuperior {
            candidate: better.id,
            verified_blue_work: better.verified_blue_work().expect("verified by construction"),
        };
    }
    if inputs.unresolved_competing > 0 {
        return CommitVerdict::RefuseUnresolved { count: inputs.unresolved_competing };
    }
    CommitVerdict::Allow
}

#[cfg(test)]
mod commit_barrier_tests {
    use super::{tests::*, *};

    fn inputs<'a>(staged: u64, registry: &'a IbdCandidateRegistry) -> CommitInputs<'a> {
        CommitInputs {
            staged_blue_work: BlueWorkType::from_u64(staged),
            descends_from_checkpoint: None,
            checkpoint_params_match: None,
            unresolved_competing: registry.unresolved().len(),
            registry,
        }
    }

    #[test]
    fn a_clean_sync_with_nothing_else_on_offer_commits() {
        let r = IbdCandidateRegistry::default();
        assert_eq!(decide_commit(inputs(100, &r)), CommitVerdict::Allow);
    }

    #[test]
    fn the_incident_scenario_does_not_commit() {
        // testnet-22: IBD starts from branch B, and 22 seconds later a peer on the heavier branch A
        // connects. Under the old code A was discarded unseen and B was committed. Now A is a
        // candidate nobody managed to verify, and that alone stops the commit.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        r.observe_summary(peer(1), &header(0xA, 900), pp(0xA), now + Duration::from_secs(22));

        assert_eq!(decide_commit(inputs(100, &r)), CommitVerdict::RefuseUnresolved { count: 1 });
    }

    #[test]
    fn ordinary_traffic_from_peers_on_this_same_chain_does_not_block_the_commit() {
        // Peers on the chain being synced keep relaying, and each new tip is a new candidate id.
        // If those counted as rivals, a healthy node would quarantine on every headers-proof IBD.
        // The caller resolves them by containment; here that shows up as a zero count.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        r.observe_summary(peer(1), &header(0xB1, 110), pp(0xB), now);
        r.observe_summary(peer(2), &header(0xB2, 120), pp(0xB), now);
        assert_eq!(r.unresolved().len(), 2, "both look unresolved to the registry");

        let mut i = inputs(100, &r);
        i.unresolved_competing = 0; // the staged chain already contains both tips
        assert_eq!(decide_commit(i), CommitVerdict::Allow);
    }

    #[test]
    fn a_verified_better_chain_stops_the_commit() {
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(0xA, 900), pp(0xA), now);
        r.set_validation(id, CandidateValidation::ProofValidated { verified_blue_work: BlueWorkType::from_u64(900) });

        assert_eq!(
            decide_commit(inputs(100, &r)),
            CommitVerdict::RefuseVerifiedSuperior { candidate: id, verified_blue_work: BlueWorkType::from_u64(900) }
        );
    }

    #[test]
    fn a_fake_blue_work_cannot_stop_a_commit() {
        // The other half of "claims never decide": a liar must not be able to cancel an honest
        // node's sync either, or stalling the network costs one lie.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let liar = r.observe_summary(peer(9), &header(0xF, u64::MAX), pp(0xF), now);
        r.set_validation(liar, CandidateValidation::Rejected { reason: CandidateRejectReason::InvalidProof });

        assert_eq!(decide_commit(inputs(100, &r)), CommitVerdict::Allow, "a refuted claim blocks nothing");
    }

    #[test]
    fn a_verified_worse_or_equal_chain_does_not_stop_the_commit() {
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(0xA, 100), pp(0xA), now);
        r.set_validation(id, CandidateValidation::ProofValidated { verified_blue_work: BlueWorkType::from_u64(100) });

        assert_eq!(decide_commit(inputs(100, &r)), CommitVerdict::Allow, "equal work is not a reason to switch");
        assert_eq!(decide_commit(inputs(101, &r)), CommitVerdict::Allow);
    }

    #[test]
    fn a_chain_without_the_operators_checkpoint_is_refused_before_work_is_considered() {
        // Admissibility is settled before quality, because work is the thing an attacker can make.
        let r = IbdCandidateRegistry::default();
        let mut i = inputs(u64::MAX, &r);
        i.descends_from_checkpoint = Some(false);
        assert_eq!(decide_commit(i), CommitVerdict::RefuseCheckpointMissing);
    }

    #[test]
    fn a_checkpoint_for_other_rules_is_refused_rather_than_ignored() {
        let r = IbdCandidateRegistry::default();
        let mut i = inputs(100, &r);
        i.checkpoint_params_match = Some(false);
        i.descends_from_checkpoint = Some(true);
        assert_eq!(decide_commit(i), CommitVerdict::RefuseCheckpointParamsMismatch);
    }

    #[test]
    fn a_checkpoint_that_is_satisfied_gets_out_of_the_way() {
        let r = IbdCandidateRegistry::default();
        let mut i = inputs(100, &r);
        i.checkpoint_params_match = Some(true);
        i.descends_from_checkpoint = Some(true);
        assert_eq!(decide_commit(i), CommitVerdict::Allow, "a checkpoint constrains, it does not select");
    }
}
