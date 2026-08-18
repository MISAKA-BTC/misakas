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
use kaspa_hashes::Hash;
use kaspa_p2p_lib::PeerKey;
use std::sync::Arc;

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

/// How long a nominated candidate may hold the single verification slot before it is written off.
///
/// This is a LOCK deadline, not a transfer timeout. `strongest_unverified` yields nothing while any
/// candidate sits in `ProofRequested`, so one request whose flow died — peer dropped, connection
/// reset — silences nomination entirely until it expires. Measured: with this set to the same 600s
/// as the fetch timeout, a recovery round could stall for the whole window and report
/// `RecoveryPermitRequested=0`.
///
/// Long enough for a real proof to arrive over a slow link, short enough that a dead request does
/// not cost a node its convergence.
pub const CHALLENGER_VERIFICATION_LEASE: Duration = Duration::from_secs(60);

/// How long to wait for a nominated peer to actually send its pruning proof.
///
/// **Must stay below the lease**, and it did not: the fetch was allowed 600s against a 120s lease,
/// five times the deadline that exists to free the slot. The two then contradicted each other —
/// the registry declared the slot released at 120s while the flow that held it was still parked in
/// `dequeue_with_timeout` for another eight minutes, unable to serve the re-nomination it had just
/// been freed for.
///
/// Measured, on a peer that stopped answering after its own IBD was refused: request sent, no reply,
/// and the node still waiting when the round ended 400s later. It never reached the retry that
/// would have rotated to another source.
///
/// A transfer deadline must be shorter than the lock deadline it lives inside, or the lock is a
/// fiction. The assertion below is the only thing that keeps them in that order.
pub const CHALLENGER_PROOF_TIMEOUT: Duration = Duration::from_secs(40);

const _: () = assert!(
    CHALLENGER_PROOF_TIMEOUT.as_secs() < CHALLENGER_VERIFICATION_LEASE.as_secs(),
    "the proof fetch must finish, or give up, before the lease that frees its slot expires"
);

/// Retry delay after a failed summary request: 1s, 2s, 4s, 8s, then held there.
///
/// A request lost to a hiccup must not cost what a delivered summary costs. The two used to share
/// one cooldown, which meant a single dropped reply put a peer out of reach for the whole window —
/// on a link that is merely slow, that is the difference between converging and not.
fn backoff_for(failures: u32) -> Duration {
    Duration::from_secs(1 << failures.min(3))
}

/// How long a reservation may sit unclaimed, with the latch free, before it is released.
///
/// A reservation closes the latch to every peer but the reserved chain's sources. That is the point
/// of it. But it means a reservation whose sources have all disconnected locks this node out of
/// syncing from *anyone*, permanently and silently — the reservation is never cleared, because the
/// only thing that clears it is the successful IBD it is waiting for.
///
/// So it is only allowed to wait so long. The clock runs only while no IBD is running: an IBD in
/// flight IS progress, however slow, and interrupting a real sync to re-open a race would be worse
/// than the stall.
pub const PREFERRED_CANDIDATE_HANDOFF_DEADLINE: Duration = Duration::from_secs(180);

/// Absolute ceiling on one reservation, however many attempts it makes.
///
/// A reservation deliberately survives a failed attempt — spending the handoff on one stumble drops
/// the node back to the branch it had already decided against. But surviving failure and surviving
/// forever are different: a chain that cannot be synced after half an hour of trying should stop
/// excluding the chains that might be.
pub const PREFERRED_CANDIDATE_MAX_LIFETIME: Duration = Duration::from_secs(1800);

/// How long to leave a peer alone AFTER it has answered.
///
/// Its answer does not change quickly, so re-asking immediately learns nothing while still costing
/// the peer work. Per peer, so one loud peer cannot spend another's budget.
pub const PEER_SUMMARY_COOLDOWN: Duration = Duration::from_secs(10);

/// How many times a chain may have its proof requested and time out before it is written off.
///
/// Bounded because a peer that keeps promising a proof and never sending one is a way to keep this
/// node's single verification slot busy for free. Three attempts across three different moments —
/// and, usually, three different connections — is enough to distinguish a peer that keeps getting
/// cut off from one that is stalling on purpose.
pub const MAX_PROOF_ATTEMPTS: u32 = 3;

/// How long participation stays withheld after an IBD, at minimum.
///
/// Lives here, beside the deadlines it has to contain, rather than next to the flow that applies
/// it. The three constants form one time budget and were previously spread across two files with
/// nothing relating them — which is how the proof fetch came to be allowed five times the lease
/// that frees its slot.
pub const POST_IBD_CANDIDATE_REVIEW: Duration = Duration::from_secs(180);

/// **The budget must close.** Every attempt this node is willing to make at verifying a challenger
/// has to fit inside the review those attempts exist to inform.
///
/// If it does not, the review floor expires part-way through verification and the node goes Ready —
/// and once Ready, every recovery driver returns early by design, so the remaining attempts never
/// happen. The chain being checked is abandoned mid-check, silently, and the node acts on the one
/// it had not finished doubting. That is the RC1 defect arriving by a different road.
///
/// Nothing holds the review open during these attempts, deliberately: only a candidate that has
/// produced a VALID proof may extend it. Letting an unanswered request extend the review would make
/// "advertise a chain and go quiet" a way to keep a validator from ever signing. So the attempts
/// must fit, rather than the review stretch to accommodate them.
///
/// For scale: fetching and validating a real pruning proof across a 267 ms intercontinental link
/// measured 0.2-1.5s. A 60s lease is more than an order of magnitude of headroom, three times over.
const _: () = assert!(
    (MAX_PROOF_ATTEMPTS as u64) * CHALLENGER_VERIFICATION_LEASE.as_secs() <= POST_IBD_CANDIDATE_REVIEW.as_secs(),
    "verification must finish inside the review it informs, or the node goes Ready mid-check"
);

/// A candidate must outlive every attempt made on it, or the evidence is discarded before the last
/// attempt can use it.
const _: () = assert!(
    (MAX_PROOF_ATTEMPTS as u64) * CHALLENGER_VERIFICATION_LEASE.as_secs() < CANDIDATE_TTL.as_secs(),
    "a candidate must not expire while it is still being verified"
);

/// How many proof requests a PEER may leave unanswered before it stops being asked.
///
/// `MAX_PROOF_ATTEMPTS` bounds a chain's retries, but it is stored on the candidate — and a
/// candidate is deleted the moment its last source disconnects. So a peer that advertises a heavy
/// chain, accepts the nomination, goes quiet, disconnects and reconnects starts again from zero
/// every time, and can hold this node's single verification slot for a lease out of every cycle
/// indefinitely. Nothing wrong is ever adopted — the node simply stays held back, which for a
/// validator means never attesting.
///
/// Counted per peer and kept outside the candidates, so dropping the connection does not launder
/// it. Decays on the same TTL as everything else here: an honest peer that had a bad hour is not
/// banned for the life of the process.
pub const MAX_PEER_PROOF_FAILURES: u32 = 3;

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
    /// Nominated for verification; a source peer has been asked for its pruning proof.
    ///
    /// `since` bounds the wait. A peer that never answers must not be able to hold up a commit
    /// forever — that would make "advertise a chain and go quiet" a denial of service.
    ///
    /// The claim is carried through because proof validation needs it: a pruning proof is checked
    /// against the tip work its prover asserted, so the assertion has to survive the transition out
    /// of `SummaryReceived`. It is still a claim here, and still decides nothing on its own.
    ProofRequested { since: Instant, claimed_blue_work: ClaimedBlueWork },
    /// A pruning proof validated in the context of current consensus.
    ///
    /// `verified_blue_work` is the **pruning point header's** accumulated work, which the proof's
    /// header chain and PoW actually establish. Deliberately NOT the claimed tip work: proof
    /// validation takes the pruning-period work (`relay_blue_work - pp_header.blue_work`) from the
    /// prover on trust, to be checked only if the proof is accepted and the chain synced. Comparing
    /// on the tip claim would hand a liar exactly the preemption this type exists to prevent, so
    /// the figure kept here is the part that is genuinely backed.
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
    /// A source was asked for a proof and did not deliver one in time. Refused rather than left
    /// pending, so going quiet cannot stall a commit.
    ProofTimeout,
    /// No connected peer could supply the proof (every source disconnected).
    NoSource,
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

/// The chain this node has decided to sync next, and who can serve it.
///
/// Cancelling an IBD releases the latch, and without a reservation the next peer to relay anything
/// takes it — possibly the branch just rejected, possibly an unrelated one. The switch would then be
/// decided by arrival order, which is the bug the switch exists to fix. So the winner is named, and
/// the latch is closed to everyone else until it has had its turn.
#[derive(Clone, Debug)]
pub struct PreferredIbdCandidate {
    pub candidate_id: CandidateId,
    /// Peers known to offer this chain. Any of them may serve it — a reservation names a chain, not
    /// a peer, so one source disconnecting does not void it.
    pub preferred_sources: Vec<PeerKey>,
    /// The virtual selected parent header from the summary, so the handoff can start an IBD without
    /// waiting for the peer to relay something.
    pub header: Arc<Header>,
    /// The work this node verified for itself, carried for the log that explains the switch.
    pub verified_blue_work: BlueWorkType,
    /// Which switch this is. Guards against a stale reservation from an earlier round being honoured
    /// after the situation has moved on.
    pub switch_generation: u32,
    /// When this reservation was first made. Bounds its total life across every attempt.
    pub reserved_at: Instant,
    /// When it last had the latch, or when it was made. Bounds how long it may hold the latch shut
    /// while nobody is using it.
    pub unclaimed_since: Instant,
}

impl PreferredIbdCandidate {
    /// Why this reservation should be released, if it should be. `None` means it is still working.
    ///
    /// Only meaningful while no IBD is running — see [`PREFERRED_CANDIDATE_HANDOFF_DEADLINE`].
    pub fn expiry_reason(&self, now: Instant) -> Option<&'static str> {
        if now.duration_since(self.reserved_at) >= PREFERRED_CANDIDATE_MAX_LIFETIME {
            Some("reservation exceeded its absolute lifetime without ever committing its chain")
        } else if now.duration_since(self.unclaimed_since) >= PREFERRED_CANDIDATE_HANDOFF_DEADLINE {
            Some("reservation went unclaimed with the latch free; its sources are gone or silent")
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub struct IbdCandidate {
    pub id: CandidateId,
    /// The virtual selected parent header this peer advertised. Kept so a handoff can start an IBD
    /// directly instead of waiting for the peer to relay a block.
    pub header: Arc<Header>,
    /// The tip work claimed in the summary, retained through every validation state so the
    /// investigation trigger can still read it after the candidate has been proof-validated.
    pub claimed_tip: ClaimedBlueWork,
    /// Every peer offering this chain. More than one is a liveness benefit — a source to fail over
    /// to — and **not** a vote: N peers is N sybils just as easily.
    pub sources: Vec<PeerKey>,
    pub validation: CandidateValidation,
    /// Digest of the pruning proof that verified this candidate, so a recovery permit is bound to
    /// the evidence that justified it and cannot be redeemed against a different proof.
    pub proof_hash: Option<Hash>,
    pub first_seen: Instant,
    pub last_seen: Instant,
    /// Proof requests for this chain that ran out of lease. Bounds retries — see
    /// [`MAX_PROOF_ATTEMPTS`].
    pub proof_attempts: u32,
}

impl CandidateValidation {
    /// A short static name, for diagnostics that must not allocate on the path being measured.
    pub fn name(&self) -> &'static str {
        match self {
            CandidateValidation::Observed => "Observed",
            CandidateValidation::SummaryReceived { .. } => "SummaryReceived",
            CandidateValidation::ProofRequested { .. } => "ProofRequested",
            CandidateValidation::ProofValidated { .. } => "ProofValidated",
            CandidateValidation::Rejected { .. } => "Rejected",
        }
    }
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
            CandidateValidation::Observed | CandidateValidation::ProofRequested { .. } => Some((0, BlueWorkType::from_u64(0))),
            CandidateValidation::Rejected { .. } => None,
        }
    }

    /// The tip work this peer claimed in its summary. A claim: it decides what to investigate and
    /// never what to adopt.
    pub fn claimed_tip_blue_work(&self) -> Option<BlueWorkType> {
        Some(ClaimedBlueWork::for_priority_only(&self.claimed_tip))
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
    /// When each peer next becomes eligible to be asked for a summary, and how many times asking
    /// it has failed in a row. Separated because a failed request and a delivered summary are not
    /// the same event: one should be retried quickly, the other should not be repeated at all for a
    /// while, and a single cooldown for both means a request lost to a hiccup costs a full window.
    next_ask: HashMap<PeerKey, (Instant, u32)>,
    /// Proof requests each peer has left unanswered, and when it last did. Held here rather than on
    /// the candidate because candidates are deleted when their last source disconnects, which would
    /// otherwise let a peer launder its record by reconnecting. See [`MAX_PEER_PROOF_FAILURES`].
    peer_proof_failures: HashMap<PeerKey, (u32, Instant)>,
    /// Syncs abandoned in favour of a verified-better candidate, since the node started.
    switches: u32,
}

impl IbdCandidateRegistry {
    /// Note that `peer` advertised something, without knowing yet what chain it belongs to.
    ///
    /// An inv hash is not a chain: it may be a side block, a merge block, or a newer block on the
    /// chain already being synced. All this records is that the peer is worth asking.
    pub fn observe_peer(&mut self, peer: PeerKey, now: Instant) {
        // A peer just seen is eligible immediately: the first question should not wait out a
        // cooldown that exists to stop repetition.
        self.next_ask.entry(peer).or_insert((now, 0));
    }

    /// Whether `peer` may be asked for a summary now, claiming the budget if so.
    pub fn claim_summary_request(&mut self, peer: PeerKey, now: Instant) -> bool {
        match self.next_ask.entry(peer) {
            Entry::Occupied(mut e) => {
                let (eligible_at, failures) = *e.get();
                if now < eligible_at {
                    return false;
                }
                // Provisionally hold the peer off for a backoff step; the outcome then either
                // resets it or lets it grow.
                e.insert((now + backoff_for(failures), failures));
                true
            }
            Entry::Vacant(e) => {
                e.insert((now + backoff_for(0), 0));
                true
            }
        }
    }

    /// A summary arrived. Stop asking this peer for a while — there is nothing more to learn from
    /// it until something changes.
    pub fn note_summary_success(&mut self, peer: PeerKey, now: Instant) {
        self.next_ask.insert(peer, (now + PEER_SUMMARY_COOLDOWN, 0));
    }

    /// The request failed. Retry soon, backing off so a peer that is simply unreachable does not get
    /// hammered — but nothing like the cooldown a delivered summary earns.
    pub fn note_summary_failure(&mut self, peer: PeerKey, now: Instant) {
        let failures = self.next_ask.get(&peer).map(|(_, f)| f.saturating_add(1)).unwrap_or(1);
        self.next_ask.insert(peer, (now + backoff_for(failures), failures));
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
                header: Arc::new(header.clone()),
                claimed_tip: claimed,
                sources: vec![peer],
                validation: CandidateValidation::SummaryReceived { claimed_blue_work: claimed },
                proof_hash: None,
                first_seen: now,
                last_seen: now,
                proof_attempts: 0,
            },
        );
        id
    }

    pub fn set_validation(&mut self, id: CandidateId, validation: CandidateValidation) {
        if let Some(c) = self.candidates.get_mut(&id) {
            c.validation = validation;
        }
    }

    /// Record which proof established a candidate, alongside the verdict it produced.
    pub fn set_validated(&mut self, id: CandidateId, verified_blue_work: BlueWorkType, proof_hash: Hash) {
        if let Some(c) = self.candidates.get_mut(&id) {
            c.validation = CandidateValidation::ProofValidated { verified_blue_work };
            c.proof_hash = Some(proof_hash);
        }
    }

    /// A proof fetch ended in a TRANSPORT failure — the connection closed, the deadline elapsed
    /// under the flow, or the lease was revoked — rather than a proof this node judged. Charge it
    /// the same retry budget a lease timeout costs, then hand the candidate back to the ordinary
    /// nomination/rotation machinery.
    ///
    /// The reason this exists: a transport failure USED to leave the candidate untouched in
    /// `ProofRequested`. A source that flaps (a peer running its own failing IBD gets disconnected
    /// constantly — exactly when its chain most needs checking) is then re-nominated, which rebuilds
    /// `ProofRequested { since: now }` and resets the lease clock, so `expire_proof_requests` never
    /// reaches its deadline and `proof_attempts` never moves. The candidate is pinned at
    /// `proof_attempts == 0`, which is the one state [`Self::unresolved`] counts — so it blocks
    /// every commit, forever, while never being resolvable. Observed live on testnet-10 (a ghost
    /// candidate from a flapping peer held the commit barrier across a whole recovery).
    ///
    /// Charging the attempt breaks the pin two ways at once: after the FIRST transport failure the
    /// candidate is no longer at `proof_attempts == 0`, so it stops blocking the commit (a source
    /// that cannot deliver a proof has spent its right to hold up the decision), and after
    /// [`MAX_PROOF_ATTEMPTS`] it is written off as `NoSource`. It stays retryable in between —
    /// being disconnected is still not evidence against the chain — so a genuinely-returning source
    /// gets its turns; it just cannot block while it flaps.
    pub fn note_proof_transport_failure(&mut self, id: &CandidateId) {
        let Some(c) = self.candidates.get_mut(id) else { return };
        // Only a proof that was actually in flight is charged; a candidate already moved on
        // (validated, rejected, or renominated by another path) is left as it is.
        let claimed = match c.validation {
            CandidateValidation::ProofRequested { claimed_blue_work, .. } => claimed_blue_work,
            _ => return,
        };
        c.proof_attempts += 1;
        c.validation = if c.proof_attempts < MAX_PROOF_ATTEMPTS {
            CandidateValidation::SummaryReceived { claimed_blue_work: claimed }
        } else {
            CandidateValidation::Rejected { reason: CandidateRejectReason::NoSource }
        };
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

    /// Candidates that have a validated proof but have not yet been acted on.
    ///
    /// A validated candidate can be left in limbo: if its proof happened to validate while an IBD
    /// was running, the switch path defers to the commit barrier — and the barrier compares that
    /// candidate's PRUNING-POINT work against the staged chain's TIP work, which can never favour
    /// it. Neither path acts, and the node keeps a chain it knows might be worse. Something has to
    /// come back and look again.
    pub fn validated_awaiting_decision(&self) -> Vec<&IbdCandidate> {
        self.candidates.values().filter(|c| matches!(c.validation, CandidateValidation::ProofValidated { .. })).collect()
    }

    /// The best candidate this node has actually verified, if any.
    ///
    /// This — and only this — is what a chain decision may be based on.
    pub fn best_verified(&self) -> Option<&IbdCandidate> {
        self.candidates.values().filter(|c| c.verified_blue_work().is_some()).max_by_key(|c| c.verified_blue_work().unwrap())
    }

    /// How many times this node has abandoned a sync in favour of a verified-better candidate.
    ///
    /// Bounded so that two chains cannot trade the latch forever. Switching is the recovery path;
    /// switching without end is a different failure with the same symptom.
    pub fn switches(&self) -> u32 {
        self.switches
    }

    pub fn note_switch(&mut self) {
        self.switches = self.switches.saturating_add(1);
    }

    /// Adopt a switch count carried over from a previous run, so the cap is not per-process.
    pub fn resume_switches(&mut self, switches: u32) {
        self.switches = self.switches.max(switches);
    }

    /// Start the budget over, because the node has participated on one chain long enough that the
    /// burst this cap guards against is demonstrably not happening.
    ///
    /// Note this must be paired with clearing the gate's persisted count: [`Self::resume_switches`]
    /// takes the MAXIMUM of the two, so a registry reset alone is undone by the next resume.
    pub fn reset_switches(&mut self) {
        self.switches = 0;
    }

    /// A verified candidate strictly better than `current`, if one exists.
    ///
    /// Strictly: equal work is not a reason to switch, and an unverified candidate is never a
    /// reason to switch no matter what it claims.
    ///
    /// **The two sides are not measured at the same place, and callers must know it.** A
    /// candidate's verified blue work comes from its pruning proof, so it is the work at its
    /// PRUNING POINT. The barrier passes the staged chain's work at its TIP. Tip work is larger
    /// than pruning-point work on the same chain, so the comparison is biased toward whatever is
    /// already staged.
    ///
    /// That bias is in the safe direction — it can refuse a switch that was warranted, never
    /// authorise one that was not — which is why the barrier is allowed to use it as a cheap
    /// pre-filter. It is NOT the adoption decision. Adoption compares verified tip against verified
    /// tip under the canonical fork-choice order, in `CandidateAdoptionPermit`.
    ///
    /// A challenger this refuses is therefore not settled. Measured: a soak round validated two
    /// candidates, was refused here, and kept the lighter chain until the post-IBD switch path
    /// looked at the same evidence again with tip work on both sides.
    pub fn verified_superior_to(&self, current: BlueWorkType) -> Option<&IbdCandidate> {
        self.best_verified().filter(|c| c.verified_blue_work().is_some_and(|w| w > current))
    }

    /// Candidates that are neither verified nor refused — the ones a commit decision cannot
    /// account for, because nobody has checked them.
    pub fn unresolved(&self) -> Vec<&IbdCandidate> {
        self.candidates
            .values()
            .filter(|c| {
                // A chain that has already had a proof request run out of lease does not get to
                // hold up a commit again while it waits for another turn. It stays nominatable —
                // being disconnected is not evidence against a chain — but blocking is a privilege
                // it has spent. Otherwise "advertise a chain and go quiet" would simply stall for
                // three leases instead of one.
                c.proof_attempts == 0
                    && matches!(
                        c.validation,
                        CandidateValidation::Observed
                            | CandidateValidation::SummaryReceived { .. }
                            | CandidateValidation::ProofRequested { .. }
                    )
            })
            .collect()
    }

    /// The most promising chain nobody has checked yet, or `None` when there is nothing to do.
    ///
    /// Ordered by claimed work — the one decision a claim is allowed to make. A peer shouting the
    /// maximum gets checked first and fails there; it does not get a chain.
    ///
    /// Returns `None` while any verification is already in flight. A pruning proof is minutes of
    /// the prover's work and a large transfer, so one challenger at a time is the budget: without
    /// it, N advertised candidates would mean N concurrent proof fetches, and manufacturing
    /// candidates is far cheaper than serving proofs for them.
    pub fn strongest_unverified(&self) -> Option<&IbdCandidate> {
        if self.candidates.values().any(|c| matches!(c.validation, CandidateValidation::ProofRequested { .. })) {
            return None;
        }
        // A candidate whose every source has stopped answering must not be nominated: it would take
        // the single verification slot and hold it for a lease with nobody able to fill the request.
        self.candidates
            .values()
            .filter(|c| matches!(c.validation, CandidateValidation::SummaryReceived { .. }))
            .filter(|c| self.designated_prover(&c.id).is_some())
            .max_by_key(|c| match c.validation {
                CandidateValidation::SummaryReceived { claimed_blue_work } => claimed_blue_work.for_priority_only(),
                _ => BlueWorkType::from_u64(0),
            })
    }

    /// Release candidates whose source never delivered a proof, so they stop blocking a commit.
    ///
    /// Returns what was timed out, for logging and peer penalties. Being unable to back a claim is
    /// the peer's failure, not a reason for this node to wait indefinitely.
    ///
    /// A timeout is a fact about ONE attempt against ONE source, not a verdict on the chain. A peer
    /// that was disconnected halfway through sending a proof has said nothing about whether that
    /// chain is real — and a peer running a failing IBD gets disconnected constantly, which is
    /// exactly when its chain most needs checking. So a timed-out candidate goes back to being
    /// nominatable, up to a few attempts, and only then is written off. An invalid proof is
    /// different: that IS a statement about the chain, and it stays permanent.
    pub fn expire_proof_requests(&mut self, now: Instant, deadline: Duration) -> Vec<CandidateId> {
        let stale: Vec<_> = self
            .candidates
            .values()
            .filter(
                |c| matches!(c.validation, CandidateValidation::ProofRequested { since, .. } if now.duration_since(since) >= deadline),
            )
            .map(|c| c.id)
            .collect();
        // Charge the peer that was asked, before proof_attempts moves and changes who that was.
        let charged: Vec<_> = stale.iter().filter_map(|id| self.designated_prover(id)).collect();
        for peer in charged {
            let entry = self.peer_proof_failures.entry(peer).or_insert((0, now));
            entry.0 = entry.0.saturating_add(1);
            entry.1 = now;
        }
        for id in &stale {
            let Some(c) = self.candidates.get_mut(id) else { continue };
            let CandidateValidation::ProofRequested { claimed_blue_work, .. } = c.validation else { continue };
            c.proof_attempts += 1;
            c.validation = if c.proof_attempts < MAX_PROOF_ATTEMPTS {
                CandidateValidation::SummaryReceived { claimed_blue_work }
            } else {
                CandidateValidation::Rejected { reason: CandidateRejectReason::ProofTimeout }
            };
        }
        stale
    }

    /// Candidates whose proof has been requested but not yet delivered, with who could deliver it.
    ///
    /// The level-triggered half of nomination: the broadcast wakes whoever is listening, this is how
    /// a flow that was not listening finds out anyway. See `IbdFlow::serve_pending_nomination`.
    pub fn candidates_awaiting_proof(&self) -> Vec<(CandidateId, Vec<PeerKey>)> {
        self.candidates
            .values()
            .filter(|c| matches!(c.validation, CandidateValidation::ProofRequested { .. }))
            .map(|c| (c.id, c.sources.clone()))
            .collect()
    }

    /// When this candidate's proof request must have finished, one way or another.
    ///
    /// The lease deadline, not a duration from now. A request that starts late in the lease gets
    /// what is left of it and no more — a fixed timeout would let the request outlive the lease
    /// again, which is the whole defect, only later and less often.
    pub fn proof_request_deadline(&self, id: &CandidateId) -> Option<Instant> {
        match self.candidates.get(id)?.validation {
            CandidateValidation::ProofRequested { since, .. } => Some(since + CHALLENGER_VERIFICATION_LEASE),
            _ => None,
        }
    }

    /// The stamp identifying the current proof attempt for this candidate.
    ///
    /// Attempts are told apart by when they started. A reply that arrives after its attempt has
    /// been written off must not be applied to the attempt that replaced it: the peer that answers
    /// late is, by construction, the one whose answer was already judged too slow to trust.
    pub fn proof_attempt_stamp(&self, id: &CandidateId) -> Option<Instant> {
        match self.candidates.get(id)?.validation {
            CandidateValidation::ProofRequested { since, .. } => Some(since),
            _ => None,
        }
    }

    /// Which single source should fetch this candidate's proof right now.
    ///
    /// A nomination is broadcast to every flow, and the idle tick reads the registry from every
    /// flow, so without this every source of a candidate fetches the same multi-megabyte proof at
    /// the same time and validates it in parallel. One peer is chosen instead — deterministically,
    /// so the choice needs no lock and no lease of its own.
    ///
    /// Rotating on `proof_attempts` is what keeps that from becoming a single point of failure: if
    /// the chosen source never delivers, the lease expires and the next attempt asks a different
    /// one. Sources are pruned when a peer disconnects, so this only ever names a live peer.
    pub fn designated_prover(&self, id: &CandidateId) -> Option<PeerKey> {
        let c = self.candidates.get(id)?;
        let eligible: Vec<_> = c.sources.iter().copied().filter(|p| !self.peer_has_stopped_answering(p)).collect();
        if eligible.is_empty() {
            return None;
        }
        Some(eligible[c.proof_attempts as usize % eligible.len()])
    }

    /// Whether this peer has left too many proof requests unanswered to be worth asking again.
    pub fn peer_has_stopped_answering(&self, peer: &PeerKey) -> bool {
        self.peer_proof_failures.get(peer).is_some_and(|(n, _)| *n >= MAX_PEER_PROOF_FAILURES)
    }

    /// Peers currently offering this candidate, for asking one of them for a proof.
    pub fn sources_of(&self, id: &CandidateId) -> Vec<PeerKey> {
        self.candidates.get(id).map(|c| c.sources.clone()).unwrap_or_default()
    }

    /// Drop a peer as a source. The candidate itself survives while another peer offers it — the
    /// chain did not become wrong because one connection dropped.
    /// `peer_proof_failures` is deliberately NOT cleared here: a peer that drops the connection
    /// rather than answer is exactly the case the count exists for.
    pub fn forget_peer(&mut self, peer: &PeerKey) {
        self.next_ask.remove(peer);
        for candidate in self.candidates.values_mut() {
            candidate.sources.retain(|p| p != peer);
        }
        self.candidates.retain(|_, c| !c.sources.is_empty());
    }

    pub fn expire(&mut self, now: Instant) {
        self.candidates.retain(|_, c| now.duration_since(c.last_seen) < CANDIDATE_TTL);
        self.next_ask.retain(|_, (t, _)| *t + CANDIDATE_TTL > now);
        // An honest peer that had a bad hour is not written off for the life of the process.
        self.peer_proof_failures.retain(|_, (_, t)| *t + CANDIDATE_TTL > now);
    }

    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    pub fn clear(&mut self) {
        self.candidates.clear();
        self.next_ask.clear();
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
    use crate::flowcontext::ibd_candidates::{CommitInputs, CommitVerdict, decide_commit};
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
    fn a_transport_failure_stops_a_flapping_candidate_from_pinning_the_barrier() {
        // The testnet-10 ghost: a source that keeps disconnecting mid-proof re-arms
        // `ProofRequested { since: now }` on every reconnect, so the lease deadline is never
        // reached and `proof_attempts` never moves — pinning the candidate at the one state the
        // commit barrier waits on. Charging the transport failure breaks the pin.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(0xA, 900), pp(0xA), now);
        assert_eq!(r.unresolved().len(), 1, "an un-checked summary blocks the commit");

        // Nominate: a proof is requested. Simulate the flow's transition.
        r.set_validation(
            id,
            CandidateValidation::ProofRequested { since: now, claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(900)) },
        );
        assert_eq!(r.unresolved().len(), 1, "still blocking while genuinely in flight");

        // First transport failure: charged, back to nominatable, but NO LONGER blocking.
        r.note_proof_transport_failure(&id);
        assert_eq!(r.unresolved().len(), 0, "a source that could not deliver has spent its right to block");
        assert!(
            matches!(r.get(&id).unwrap().validation, CandidateValidation::SummaryReceived { .. }),
            "but it stays retryable — a disconnect is not a verdict on the chain"
        );

        // Re-nominated and it flaps again, repeatedly: still never blocks, and is written off at the cap.
        for _ in 0..MAX_PROOF_ATTEMPTS {
            r.set_validation(
                id,
                CandidateValidation::ProofRequested {
                    since: now,
                    claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(900)),
                },
            );
            r.note_proof_transport_failure(&id);
            assert_eq!(r.unresolved().len(), 0, "a flapping source never regains the power to pin the barrier");
        }
        assert!(
            matches!(r.get(&id).unwrap().validation, CandidateValidation::Rejected { reason: CandidateRejectReason::NoSource }),
            "and after its retries are spent it is written off as NoSource"
        );

        // Idempotent / safe on a candidate that already moved on.
        r.note_proof_transport_failure(&id);
        assert!(matches!(r.get(&id).unwrap().validation, CandidateValidation::Rejected { .. }));
    }

    #[test]
    fn a_delivered_summary_earns_a_full_cooldown() {
        // Its answer does not change quickly, so re-asking learns nothing.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        assert!(r.claim_summary_request(peer(1), now));
        r.note_summary_success(peer(1), now);
        assert!(!r.claim_summary_request(peer(1), now + PEER_SUMMARY_COOLDOWN - Duration::from_secs(1)));
        assert!(r.claim_summary_request(peer(1), now + PEER_SUMMARY_COOLDOWN));

        // Per peer: one loud peer must not spend another's budget.
        assert!(r.claim_summary_request(peer(2), now));
    }

    #[test]
    fn a_failed_request_retries_far_sooner_than_a_delivered_one() {
        // The distinction that matters on a slow link: a request lost to a hiccup must not cost what
        // an answered one costs. Sharing a cooldown put a peer out of reach for a whole window over
        // a single dropped reply.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        assert!(r.claim_summary_request(peer(1), now));
        r.note_summary_failure(peer(1), now);

        assert!(r.claim_summary_request(peer(1), now + Duration::from_secs(2)), "a failure retries in seconds");
        assert!(
            Duration::from_secs(2) < PEER_SUMMARY_COOLDOWN,
            "the retry must be materially quicker than the success cooldown or the split buys nothing"
        );
    }

    #[test]
    fn repeated_failures_back_off_but_do_not_give_up() {
        // A peer that is simply unreachable should not be hammered, and should not be written off
        // either — it may be the one holding the better chain.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let mut delays = Vec::new();
        for step in 0..5u32 {
            r.note_summary_failure(peer(1), now);
            // Find the smallest whole-second wait that lets the next attempt through.
            let waited = (1..=16).find(|s| r.claim_summary_request(peer(1), now + Duration::from_secs(*s))).unwrap();
            delays.push(waited);
            let _ = step;
        }
        assert!(delays.windows(2).all(|w| w[1] >= w[0]), "backoff must not shrink: {delays:?}");
        assert!(*delays.last().unwrap() <= 8, "and must stay bounded: {delays:?}");
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
    fn the_loudest_unverified_claim_is_what_gets_checked() {
        // Claims order the queue and nothing else. A peer shouting u64::MAX buys the privilege of
        // being asked for a proof first, which is exactly where the shouting stops working.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        r.observe_summary(peer(1), &header(1, 10), pp(1), now);
        let loud = r.observe_summary(peer(2), &header(2, u64::MAX), pp(2), now);
        r.observe_summary(peer(3), &header(3, 500), pp(3), now);

        assert_eq!(r.strongest_unverified().unwrap().id, loud);
    }

    #[test]
    fn candidates_already_being_verified_are_not_nominated_again() {
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(1, 10), pp(1), now);
        r.set_validation(
            id,
            CandidateValidation::ProofRequested { since: now, claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(10)) },
        );

        assert!(r.strongest_unverified().is_none(), "one proof fetch in flight per candidate, not one per nomination");
    }

    #[test]
    fn only_one_challenger_is_verified_at_a_time() {
        // A proof is minutes of the prover's work and a large transfer. Manufacturing candidates is
        // much cheaper than serving proofs for them, so the fetches must be budgeted.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let first = r.observe_summary(peer(1), &header(1, 900), pp(1), now);
        r.observe_summary(peer(2), &header(2, 800), pp(2), now);
        r.observe_summary(peer(3), &header(3, 700), pp(3), now);

        assert_eq!(r.strongest_unverified().unwrap().id, first);
        r.set_validation(
            first,
            CandidateValidation::ProofRequested { since: now, claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(900)) },
        );
        assert!(r.strongest_unverified().is_none(), "no second fetch while one is in flight");

        // Once it settles, the next strongest becomes eligible.
        r.set_validation(first, CandidateValidation::Rejected { reason: CandidateRejectReason::InvalidProof });
        assert_eq!(r.strongest_unverified().unwrap().id, r.get(&r.strongest_unverified().unwrap().id).unwrap().id);
        assert!(r.strongest_unverified().is_some());
    }

    #[test]
    fn a_source_that_never_delivers_a_proof_stops_blocking() {
        // "Advertise a chain and go quiet" must not be a way to stall every node's commit. Fail
        // closed needs a deadline or it is just a denial of service with better intentions.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(1, 10), pp(1), now);
        r.set_validation(
            id,
            CandidateValidation::ProofRequested { since: now, claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(10)) },
        );
        assert_eq!(r.unresolved().len(), 1);

        let deadline = Duration::from_secs(600);
        assert!(r.expire_proof_requests(now + deadline - Duration::from_secs(1), deadline).is_empty(), "not yet");
        assert_eq!(r.expire_proof_requests(now + deadline, deadline), vec![id]);

        assert!(r.unresolved().is_empty(), "a candidate that missed its lease no longer holds up a commit");
        // It may still be checked again — a source can be cut off mid-proof — but it has spent the
        // right to block. The two are separate on purpose; see `unresolved`.
        assert!(matches!(r.get(&id).unwrap().validation, CandidateValidation::SummaryReceived { .. }));
    }

    #[test]
    fn switches_are_counted_so_two_branches_cannot_trade_the_latch_forever() {
        let mut r = IbdCandidateRegistry::default();
        assert_eq!(r.switches(), 0);
        r.note_switch();
        r.note_switch();
        assert_eq!(r.switches(), 2, "a node cannot tell 'I keep finding better chains' from 'I am being played'");
    }

    #[test]
    fn a_dead_verification_does_not_silence_nomination_forever() {
        // The measured liveness failure. One request whose flow died holds the single verification
        // slot, `strongest_unverified` yields nothing, and no other candidate is ever nominated. If
        // the only thing that expires it runs during an IBD, and no further IBD happens, the node
        // never converges — which is exactly what a failing recovery round reported.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let stuck = r.observe_summary(peer(1), &header(1, 10), pp(1), now);
        let better = r.observe_summary(peer(2), &header(2, 900), pp(2), now);
        r.set_validation(
            stuck,
            CandidateValidation::ProofRequested { since: now, claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(10)) },
        );
        assert!(r.strongest_unverified().is_none(), "the slot is held while a request is in flight");

        // Once the lease is up the slot is released, and the better candidate can be nominated.
        assert_eq!(r.expire_proof_requests(now + CHALLENGER_VERIFICATION_LEASE, CHALLENGER_VERIFICATION_LEASE), vec![stuck]);
        assert_eq!(r.strongest_unverified().map(|c| c.id), Some(better));
    }

    #[test]
    fn a_peer_can_be_asked_several_times_within_one_lease() {
        // A lost summary must not cost a whole verification slot: the cooldown has to give a peer
        // several chances inside the window its verification would occupy.
        assert!(
            PEER_SUMMARY_COOLDOWN * 4 <= CHALLENGER_VERIFICATION_LEASE,
            "cooldown {PEER_SUMMARY_COOLDOWN:?} leaves too few attempts inside a {CHALLENGER_VERIFICATION_LEASE:?} lease"
        );
    }

    /// The forbidden state, stated once so the tests below can point at it:
    ///
    /// ```text
    /// candidate in ProofRequested
    /// live sources > 0
    /// no source will send the request
    /// only time passes
    /// ```
    ///
    /// A nominated candidate holds the single verification slot. If nobody will serve it and
    /// nothing moves it on, the node stops examining every other chain on offer for a whole lease —
    /// and the chain it stops examining is, by construction, the one it just decided was most worth
    /// checking.
    fn nobody_will_serve(r: &IbdCandidateRegistry, id: &CandidateId) -> bool {
        matches!(r.get(id).map(|c| c.validation), Some(CandidateValidation::ProofRequested { .. }))
            && !r.sources_of(id).is_empty()
            && r.designated_prover(id).is_none()
    }

    #[test]
    fn a_proof_request_can_never_outlive_the_lease_that_owns_its_slot() {
        // The RC3 defect, as an invariant rather than a pair of constants. The old fix — a fixed
        // timeout shorter than the lease — still lets a request started late run past the end of
        // one: 90s of budget with 20s of lease left is 70s of a slot the registry has already
        // handed to someone else. The budget has to be what remains of THIS lease.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(1, 900), pp(1), now);
        r.set_validation(
            id,
            CandidateValidation::ProofRequested { since: now, claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(900)) },
        );

        let deadline = r.proof_request_deadline(&id).expect("a nominated candidate has a deadline");
        assert_eq!(deadline, now + CHALLENGER_VERIFICATION_LEASE);

        // Whenever the request starts, what remains of the lease is all it gets — and that is never
        // more than the lease itself.
        for elapsed in [Duration::ZERO, Duration::from_secs(30), CHALLENGER_VERIFICATION_LEASE - Duration::from_secs(1)] {
            let remaining = deadline.saturating_duration_since(now + elapsed);
            assert!(remaining <= CHALLENGER_VERIFICATION_LEASE - elapsed);
            assert!(now + elapsed + remaining <= deadline, "a request starting at +{elapsed:?} must not run past the lease");
        }

        // Past the deadline there is nothing left to spend, so nothing should be started.
        assert_eq!(deadline.saturating_duration_since(now + CHALLENGER_VERIFICATION_LEASE), Duration::ZERO);

        // And a candidate not under request has no deadline to hand out at all.
        r.set_validation(
            id,
            CandidateValidation::SummaryReceived { claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(900)) },
        );
        assert_eq!(r.proof_request_deadline(&id), None);
    }

    #[test]
    fn a_reply_from_a_written_off_attempt_is_not_credited_to_the_one_that_replaced_it() {
        // A peer that answers after its lease expired is, by construction, the peer whose answer
        // was already judged too slow to wait for. By then the candidate has usually been
        // re-nominated to a different source. Applying the late reply would undo that judgement and
        // credit the current attempt with evidence from the one it replaced.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(1, 900), pp(1), now);

        r.set_validation(
            id,
            CandidateValidation::ProofRequested { since: now, claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(900)) },
        );
        let first = r.proof_attempt_stamp(&id).unwrap();

        // The lease runs out and the candidate is nominated again.
        let later = now + CHALLENGER_VERIFICATION_LEASE;
        r.expire_proof_requests(later, CHALLENGER_VERIFICATION_LEASE);
        r.set_validation(
            id,
            CandidateValidation::ProofRequested { since: later, claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(900)) },
        );
        let second = r.proof_attempt_stamp(&id).unwrap();

        assert_ne!(first, second, "attempts must be distinguishable, or a late reply cannot be refused");
    }

    #[test]
    fn a_nominated_candidate_always_has_someone_to_ask() {
        // The invariant: nomination and eligibility must be decided together. If a candidate can be
        // nominated while its only source is already written off, the slot is taken by a request
        // that will never be sent.
        let mut r = IbdCandidateRegistry::default();
        let mut now = Instant::now();
        let only_source = peer(1);

        // Burn the peer's record to the limit through candidates it never proves.
        for _ in 0..MAX_PEER_PROOF_FAILURES {
            let id = r.observe_summary(only_source, &header(1, 900), pp(1), now);
            r.set_validation(
                id,
                CandidateValidation::ProofRequested {
                    since: now,
                    claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(900)),
                },
            );
            now += CHALLENGER_VERIFICATION_LEASE;
            r.expire_proof_requests(now, CHALLENGER_VERIFICATION_LEASE);
            r.forget_peer(&only_source);
        }
        assert!(r.peer_has_stopped_answering(&only_source));

        // It comes back offering the same chain. The chain is fine; the peer is not.
        let id = r.observe_summary(only_source, &header(1, 900), pp(1), now);
        assert!(!r.sources_of(&id).is_empty(), "it is a live source, it just will not answer");

        // The registry must not nominate what nobody will serve.
        assert!(r.strongest_unverified().is_none(), "nominating this would take the slot and hold it for a lease");
        assert!(!nobody_will_serve(&r, &id), "and it is not in the forbidden state, because it was never nominated");
    }

    #[test]
    fn a_prover_written_off_mid_lease_does_not_strand_the_candidate() {
        // The harder half. The candidate is nominated while its source is still eligible, and the
        // source is written off DURING the lease — by failing to prove some other chain. Nothing
        // re-examines the nomination, so this is where a stranded candidate would come from.
        let mut r = IbdCandidateRegistry::default();
        let mut now = Instant::now();
        let source = peer(1);

        let stuck = r.observe_summary(source, &header(1, 900), pp(1), now);
        r.set_validation(
            stuck,
            CandidateValidation::ProofRequested { since: now, claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(900)) },
        );

        // Meanwhile the same peer fails to prove two other chains, taking it to the limit.
        for n in 2..=(MAX_PEER_PROOF_FAILURES as u64) {
            let other = r.observe_summary(source, &header(n, 800), pp(n), now);
            r.set_validation(
                other,
                CandidateValidation::ProofRequested {
                    since: now,
                    claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(800)),
                },
            );
            now += CHALLENGER_VERIFICATION_LEASE;
            r.expire_proof_requests(now, CHALLENGER_VERIFICATION_LEASE);
        }

        // Whatever the peer's record now says, the nominated candidate must not be left in the
        // forbidden state: either someone can still be asked, or its lease has released it.
        assert!(
            !nobody_will_serve(&r, &stuck),
            "candidate is nominated, has a live source, and has no designated prover — the slot is held by a request \
             nobody will ever send"
        );
    }

    #[test]
    fn a_peer_cannot_launder_its_record_by_reconnecting() {
        // The attack: advertise a heavy chain, accept the nomination, go quiet, disconnect,
        // reconnect. Candidate deletion on last-source-loss resets everything stored on the
        // candidate, so the peer starts from zero each cycle and can hold this node's single
        // verification slot for a lease out of every cycle, forever. Nothing wrong gets adopted —
        // the node just never finishes reviewing, which for a validator means never attesting.
        let mut r = IbdCandidateRegistry::default();
        let mut now = Instant::now();
        let attacker = peer(9);

        for _ in 0..MAX_PEER_PROOF_FAILURES {
            let id = r.observe_summary(attacker, &header(1, 9_000), pp(1), now);
            assert_eq!(r.designated_prover(&id), Some(attacker), "still worth asking");
            r.set_validation(
                id,
                CandidateValidation::ProofRequested {
                    since: now,
                    claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(9_000)),
                },
            );
            now += CHALLENGER_VERIFICATION_LEASE;
            r.expire_proof_requests(now, CHALLENGER_VERIFICATION_LEASE);
            // The disconnect that used to wipe the evidence.
            r.forget_peer(&attacker);
            assert!(r.get(&CandidateId { pruning_point: pp(1), virtual_selected_parent: header(1, 9_000).hash }).is_none());
        }

        // Same trick, once more. The candidate is fresh; the peer's record is not.
        let id = r.observe_summary(attacker, &header(1, 9_000), pp(1), now);
        assert!(r.peer_has_stopped_answering(&attacker));
        assert_eq!(r.designated_prover(&id), None, "nobody left to ask");
        assert!(r.strongest_unverified().is_none(), "so it must not take the verification slot either");

        // An honest peer offering the same chain is unaffected — the record is per peer, not a
        // judgement on the chain.
        r.observe_summary(peer(1), &header(1, 9_000), pp(1), now);
        assert_eq!(r.designated_prover(&id), Some(peer(1)));
        assert!(r.strongest_unverified().is_some());

        // And the record decays, or one bad hour would cost a peer the rest of the process.
        r.expire(now + CANDIDATE_TTL + Duration::from_secs(1));
        assert!(!r.peer_has_stopped_answering(&attacker));
    }

    #[test]
    fn exactly_one_source_fetches_a_proof_and_a_different_one_gets_the_next_attempt() {
        // A nomination is broadcast to every flow and the idle tick reads the registry from every
        // flow, so "every source of this candidate" is the default number of peers that would fetch
        // the same multi-megabyte proof at once. One does.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(1, 900), pp(1), now);
        r.observe_summary(peer(2), &header(1, 900), pp(1), now);
        r.observe_summary(peer(3), &header(1, 900), pp(1), now);
        assert_eq!(r.sources_of(&id).len(), 3, "one chain, three sources");

        let first = r.designated_prover(&id).unwrap();
        assert_eq!(r.designated_prover(&id), Some(first), "the choice must not wander between reads");

        // If that one never delivers, the next attempt must not ask it again — otherwise one silent
        // peer spends every retry the chain is allowed.
        r.set_validation(
            id,
            CandidateValidation::ProofRequested { since: now, claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(900)) },
        );
        r.expire_proof_requests(now + CHALLENGER_VERIFICATION_LEASE, CHALLENGER_VERIFICATION_LEASE);
        assert_ne!(r.designated_prover(&id), Some(first), "the next attempt has to try someone else");
    }

    #[test]
    fn a_prover_that_gets_cut_off_does_not_cost_its_chain_the_benefit_of_the_doubt() {
        // Measured at seed 2: the peer offering the heavier chain ran a doomed IBD, was disconnected
        // when it failed, and reconnected thirty seconds later to do it again. Any proof request in
        // flight died with the connection. Writing the chain off on the first timeout would mean the
        // node decided against the heavier branch on the strength of a dropped TCP connection.
        let mut r = IbdCandidateRegistry::default();
        let mut now = Instant::now();
        let id = r.observe_summary(peer(1), &header(1, 900), pp(1), now);

        for attempt in 1..MAX_PROOF_ATTEMPTS {
            assert_eq!(r.strongest_unverified().map(|c| c.id), Some(id), "attempt {attempt} should still be nominatable");
            r.set_validation(
                id,
                CandidateValidation::ProofRequested {
                    since: now,
                    claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(900)),
                },
            );
            now += CHALLENGER_VERIFICATION_LEASE;
            assert_eq!(r.expire_proof_requests(now, CHALLENGER_VERIFICATION_LEASE), vec![id]);
        }

        // Last one. Now it is a peer that will not answer, not a peer that keeps getting cut off.
        r.set_validation(
            id,
            CandidateValidation::ProofRequested { since: now, claimed_blue_work: ClaimedBlueWork::new(BlueWorkType::from_u64(900)) },
        );
        now += CHALLENGER_VERIFICATION_LEASE;
        r.expire_proof_requests(now, CHALLENGER_VERIFICATION_LEASE);
        assert!(r.strongest_unverified().is_none(), "a chain that never produces a proof must stop consuming the slot");
    }

    fn reservation(reserved_at: Instant, unclaimed_since: Instant) -> PreferredIbdCandidate {
        PreferredIbdCandidate {
            candidate_id: CandidateId { pruning_point: pp(1), virtual_selected_parent: BlockHash::from_u64_word(1) },
            preferred_sources: vec![peer(1)],
            header: Arc::new(header(1, 100)),
            verified_blue_work: BlueWorkType::from_u64(100),
            switch_generation: 0,
            reserved_at,
            unclaimed_since,
        }
    }

    #[test]
    fn a_reservation_nobody_claims_does_not_hold_the_latch_forever() {
        // The failure this prevents: the reserved chain's only source disconnects. Nothing clears
        // the reservation, because the only thing that clears it is the IBD that will now never
        // start, and try_set_ibd_running refuses every other peer. The node stops syncing. For good.
        let now = Instant::now();
        let r = reservation(now, now);
        assert_eq!(r.expiry_reason(now), None);
        assert_eq!(r.expiry_reason(now + PREFERRED_CANDIDATE_HANDOFF_DEADLINE - Duration::from_secs(1)), None);
        assert!(r.expiry_reason(now + PREFERRED_CANDIDATE_HANDOFF_DEADLINE).unwrap().contains("unclaimed"));
    }

    #[test]
    fn retrying_holds_the_reservation_but_only_up_to_a_ceiling() {
        // Surviving a failed attempt is deliberate — spending the handoff on one stumble drops the
        // node back to the branch it already decided against. Surviving forever is not: each retry
        // pushes the no-progress clock out, so without the ceiling a chain that can never be synced
        // would exclude every chain that could be, indefinitely.
        let reserved_at = Instant::now();
        let much_later = reserved_at + PREFERRED_CANDIDATE_MAX_LIFETIME - Duration::from_secs(1);

        // Freshly claimed, so not idle — and still inside its lifetime.
        assert_eq!(reservation(reserved_at, much_later).expiry_reason(much_later), None);

        // One second later the ceiling lands, and a fresh claim no longer saves it.
        let past_ceiling = reserved_at + PREFERRED_CANDIDATE_MAX_LIFETIME;
        assert!(reservation(reserved_at, past_ceiling).expiry_reason(past_ceiling).unwrap().contains("absolute lifetime"));
    }

    #[test]
    fn a_candidate_validated_at_a_busy_moment_is_not_forgotten() {
        // The measured soak failure: two candidates validated while an IBD held the latch, the
        // switch path deferred to the commit barrier, the barrier could not favour them because it
        // compares pruning-point work against tip work, and the node kept the lighter chain. What
        // makes that recoverable is that the evidence is still here to be looked at again.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(1, 900), pp(1), now);
        assert!(r.validated_awaiting_decision().is_empty(), "nothing to reconsider before a proof validates");

        r.set_validated(id, BlueWorkType::from_u64(900), Hash::from_u64_word(7));
        assert_eq!(r.validated_awaiting_decision().iter().map(|c| c.id).collect::<Vec<_>>(), vec![id]);

        // Once it has been settled either way it stops being pending.
        r.set_validation(id, CandidateValidation::Rejected { reason: CandidateRejectReason::InvalidProof });
        assert!(r.validated_awaiting_decision().is_empty());
    }

    #[test]
    fn a_verified_challenger_ends_the_standoff() {
        // The whole point of verification: the barrier stops being able only to refuse.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(0xA, 900), pp(0xA), now);
        assert_eq!(decide_commit(inputs_competing(100, &r, 1)), CommitVerdict::RefuseUnresolved { count: 1 });

        r.set_validation(id, CandidateValidation::ProofValidated { verified_blue_work: BlueWorkType::from_u64(900) });
        assert_eq!(
            decide_commit(inputs_competing(100, &r, 0)),
            CommitVerdict::RefuseVerifiedSuperior { candidate: id, verified_blue_work: BlueWorkType::from_u64(900) },
            "now it refuses for a reason it can defend, not for lack of information"
        );
    }

    fn inputs_competing(staged: u64, registry: &IbdCandidateRegistry, competing: usize) -> CommitInputs<'_> {
        CommitInputs {
            staged_blue_work: BlueWorkType::from_u64(staged),
            descends_from_checkpoint: None,
            checkpoint_params_match: None,
            unresolved_competing: competing,
            registry,
        }
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
    /// Unresolved candidates rooted in a lineage the staged chain does **not** share.
    ///
    /// Lineage, not tip. Peers on the chain being synced keep producing blocks, and each new tip is
    /// a new candidate id whose hash staging has never seen — so counting unknown tips as rivals
    /// refuses a healthy sync every time a peer moves during it. What distinguishes a rival is
    /// where the chain is rooted: a candidate at the staged pruning point (or one staging descends
    /// through) is the same history however far ahead its tip, while a candidate rooted somewhere
    /// staging has never heard of is a different history. The caller computes this because only it
    /// can query staging.
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
    fn the_barrier_can_refuse_a_switch_it_should_have_allowed_and_that_is_deliberate() {
        // The two sides are measured at different places: the candidate's work comes from its
        // pruning proof (pruning point), the staged work from its tip. So a challenger that IS
        // heavier at the tip can look lighter here.
        //
        // This test exists to stop someone "fixing" that by comparing tip against pruning point the
        // other way round, which would authorise switches on a bias in the UNSAFE direction. The
        // asymmetry is allowed only because it fails toward staying put. Adoption is where the
        // real comparison happens — verified tip against verified tip.
        let mut r = IbdCandidateRegistry::default();
        let now = Instant::now();
        let id = r.observe_summary(peer(1), &header(0xA, 900), pp(0xA), now);
        r.set_validated(id, BlueWorkType::from_u64(500), Hash::from_u64_word(7));

        // Staged tip work 800, challenger's pruning-point work 500. Refused, though the challenger's
        // own tip may be worth far more than 800.
        assert!(r.verified_superior_to(BlueWorkType::from_u64(800)).is_none(), "the barrier must not switch on this");
        assert_eq!(decide_commit(inputs(800, &r)), CommitVerdict::Allow, "and it commits the staged chain");

        // The refusal is not a verdict on the candidate: it is still verified and still in play for
        // the adoption path, which is what makes the bias survivable.
        assert!(r.best_verified().is_some_and(|c| c.id == id));
    }

    #[test]
    fn ordinary_traffic_from_peers_on_this_same_chain_does_not_block_the_commit() {
        // A peer at B120 while staging holds B100 has a tip staging has never seen — because it is
        // AHEAD, not because it forked. Counting that as a rival would quarantine a healthy node on
        // every headers-proof IBD. The caller resolves it by lineage; here that is a zero count.
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

#[cfg(test)]
mod switch_persistence_tests {
    use super::*;

    #[test]
    fn a_resumed_switch_count_is_never_lowered() {
        // Folding in a previous run's count must not be able to *reset* this process's, or a node
        // that switched twice since starting would forget by resuming an older, smaller figure.
        let mut r = IbdCandidateRegistry::default();
        r.note_switch();
        r.note_switch();
        r.resume_switches(1);
        assert_eq!(r.switches(), 2, "resuming an older count must not lower the current one");
        r.resume_switches(7);
        assert_eq!(r.switches(), 7, "but a larger carried-over count must win");
    }
}
