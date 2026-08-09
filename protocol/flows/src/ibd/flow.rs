use crate::{
    flow_context::FlowContext,
    flow_trait::Flow,
    flowcontext::bootstrap_recovery::{
        AdoptionError, CandidateAdoptionPermit, CandidateValidationPermit, ChainReviewState, ChainTip, RecoveryRequest,
        ValidatedCandidate, VerifiedCandidate, authorize_candidate_adoption, authorize_candidate_validation,
    },
    flowcontext::ibd_candidates::{
        CHALLENGER_PROOF_TIMEOUT, CandidateId, CandidateRejectReason, CandidateValidation, ClaimedBlueWork, CommitInputs,
        CommitVerdict, decide_commit,
    },
    flowcontext::recovery_trace::{RecoveryAttemptId, RecoveryStage, describe_comparison, record_stage},
    flowcontext::verification_trace::{self, SkipReason, VerificationSkip},
    ibd::{HeadersChunkStream, TrustedEntryStream, negotiate::ChainNegotiationOutput},
};
use futures::future::{Either, join_all, select, try_join_all};
use itertools::Itertools;
use kaspa_consensus_core::BlockHash; // PR-9.5e: block hashes are Hash64
use kaspa_consensus_core::{
    BlockHashSet, BlueWorkType,
    api::BlockValidationFuture,
    block::Block,
    header::Header,
    pruning::{PruningPointProof, PruningPointsList, PruningProofMetadata},
    trusted::TrustedBlock,
    tx::Transaction,
};
use kaspa_consensusmanager::{ConsensusProxy, StagingConsensus, spawn_blocking};
use kaspa_core::{debug, info, time::unix_now, warn};
use kaspa_muhash::MuHash;
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    convert::{
        header::{HeaderFormat, Versioned},
        model::trusted::TrustedDataPackage,
    },
    dequeue_with_timeout, make_message, make_request,
    pb::{
        RequestAntipastMessage, RequestBlockBodiesMessage, RequestHeadersMessage, RequestIbdBlocksMessage,
        RequestPruningPointAndItsAnticoneMessage, RequestPruningPointEvmStateMessage, RequestPruningPointOverlaySnapshotMessage,
        RequestPruningPointProofMessage, RequestPruningPointUtxoSetMessage, kaspad_message::Payload,
    },
};
use kaspa_utils::channel::JobReceiver;
use parking_lot::Mutex;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::broadcast, time::sleep};

use super::{HeadersChunk, IBD_BATCH_SIZE, PruningPointUtxosetChunkStream, progress::ProgressReporter};
type BlockBody = Vec<Transaction>;

/// Minimum time a node holds back after an IBD before it will mine, attest, or call itself synced.
///
/// A **floor**, not a verdict. When it expires the node resumes because time passed, not because
/// anything was compared — a clock has no opinion about chain quality. It is a placeholder for the
/// pre-commit candidate comparison, and it is only tolerable meanwhile because the alternative,
/// never releasing, is a node that can never mine again.
///
/// Sized for the machinery that can still catch a bad adoption: with the latch released, the relay
/// guard stops discarding competing offers, so a heavier peer's block is requested, lands as an
/// orphan, and orphan resolution starts a fresh IBD.
const POST_IBD_CANDIDATE_REVIEW: Duration = Duration::from_secs(180);

/// Time reserved, inside the lease, for validating a proof after it arrives.
///
/// Validation runs in this same flow and inside this same lease, so a request allowed to consume
/// the whole lease would leave nothing to check the answer with — and the check is the point.
const PROOF_VALIDATION_MARGIN: Duration = Duration::from_secs(20);

/// Below this, starting a request is not worth the slot it would hold.
///
/// A request that cannot plausibly finish inside what remains of the lease will be abandoned
/// half-way, having spent the one verification slot for nothing. Better to leave the slot free for
/// the next lease, which starts with a full budget. Real-WAN fetch and validation measured
/// 0.2-1.5s, so this is generous by more than an order of magnitude.
const MIN_USEFUL_PROOF_BUDGET: Duration = Duration::from_secs(10);

/// How many times this node may abandon a sync for a verified-better candidate before giving up.
///
/// Switching on evidence is the recovery path and should not be rationed lightly — but two branches
/// trading the latch forever is a different failure that looks the same from inside. A node cannot
/// distinguish "I keep finding better chains" from "I am being played" on its own, so past this
/// count it stops and says so.
const MAX_CHAIN_SWITCHES: u32 = 5;

/// How often an idle IBD flow re-examines candidates it has already validated.
///
/// Short, because the window it covers is the gap between a proof validating and the latch becoming
/// free, and a node sitting on a chain it has evidence against should not sit there long.
const VALIDATED_CANDIDATE_RECHECK: Duration = Duration::from_secs(3);

/// Flow for managing IBD - Initial Block Download
pub struct IbdFlow {
    pub(super) ctx: FlowContext,
    pub(super) router: Arc<Router>,
    pub(super) incoming_route: IncomingRoute,
    pub(super) body_only_ibd_permitted: bool,
    header_format: HeaderFormat,

    // Receives relay blocks from relay flow which are out of orphan resolution range and hence trigger IBD
    relay_receiver: JobReceiver<Block>,

    /// The permit authorising THIS IBD to cross the node's own provisional pruning point, if it
    /// holds one. Set when `determine_ibd_type` grants one and cleared when the IBD ends, so the
    /// authority lasts exactly as long as the attempt it was issued for.
    active_recovery_permit: Option<CandidateValidationPermit>,

    /// Which connection this flow belongs to. A `PeerKey` is identical across a reconnect, so
    /// without this a decision made by a flow that has since died is indistinguishable from one
    /// made by the flow that replaced it — and the peer under diagnosis reconnects every thirty
    /// seconds.
    connection_generation: u64,

    /// The adoption permit earned at the commit barrier, carried the few steps to `commit_if` so
    /// the defender it was judged against can be re-checked under the lock that performs the swap.
    /// `authorize_commit` takes `&self`, so this is a cell rather than a return value.
    earned_adoption_permit: Mutex<Option<CandidateAdoptionPermit>>,

    /// Handoffs: a chain this node has decided to sync next. Consumed by whichever flow serves it,
    /// so a switch does not depend on the winning peer happening to relay something.
    handoff_receiver: broadcast::Receiver<CandidateId>,

    /// Nominations of chains worth verifying. An IBD flow whose peer is not the syncer is otherwise
    /// idle, and it is the only flow that can fetch that peer's pruning proof — `PruningPointProof`
    /// is routed here. So this is where a challenger gets checked.
    challenger_receiver: broadcast::Receiver<CandidateId>,
}

#[async_trait::async_trait]
impl Flow for IbdFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

pub enum IbdType {
    Sync { highest_known_syncer_chain_hash: BlockHash, is_utxo_stable: bool, is_pp_anticone_synced: bool },
    DownloadHeadersProof,
    PruningCatchUp { highest_known_syncer_chain_hash: BlockHash },
}

struct QueueChunkOutput {
    jobs: Vec<BlockValidationFuture>,
    daa_score: u64,
    timestamp: u64,
}
// TODO: define a peer banning strategy

impl IbdFlow {
    pub fn new(
        ctx: FlowContext,
        router: Arc<Router>,
        incoming_route: IncomingRoute,
        relay_receiver: JobReceiver<Block>,
        body_only_ibd_permitted: bool,
        header_format: HeaderFormat,
    ) -> Self {
        let challenger_receiver = ctx.subscribe_challenger_nominations();
        let handoff_receiver = ctx.subscribe_ibd_handoffs();
        Self {
            ctx,
            router,
            incoming_route,
            relay_receiver,
            body_only_ibd_permitted,
            header_format,
            connection_generation: verification_trace::next_connection_generation(),
            active_recovery_permit: None,
            earned_adoption_permit: Mutex::new(None),
            challenger_receiver,
            handoff_receiver,
        }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            // Two things can wake this flow. Its peer offering a chain to sync from, or a
            // nomination to go check what some peer is claiming. Only one IBD runs at a time, but
            // every other peer's flow is idle meanwhile — and idle is what let the node adopt a
            // branch without ever asking anyone else for evidence.
            // A tick, so a validated candidate cannot sit in limbo.
            //
            // `consider_post_ibd_switch` runs when a proof validates. If that moment happens to fall
            // inside a running IBD it defers to the commit barrier — and the barrier compares the
            // candidate's PRUNING-POINT work against the staged chain's TIP work, which can never
            // favour it. Measured: a soak round validated two candidates, compared both, reserved
            // none, and kept the lighter chain.
            //
            // So the same evidence is looked at again once the latch is free.
            let relay_header = tokio::select! {
                // Why the tick is a sufficient driver for the checks below, when it was NOT for
                // serving a proof request: those two act only when no IBD is running, and if no IBD
                // is running then no flow is inside `ibd()` — so every flow is here, ticking.
                // Serving a proof request is gated on the PEER rather than on global state, and the
                // one peer that owes a proof is reliably the one whose flow is busy. That is why it
                // also runs at the top of the loop, where the flow is briefly free.
                _ = tokio::time::sleep(VALIDATED_CANDIDATE_RECHECK) => {
                    self.serve_pending_nomination().await;
                    self.reconsider_validated_candidates().await;
                    continue;
                }
                block = self.relay_receiver.recv() => match block {
                    Ok(block) => block.header.clone(),
                    Err(_) => return Ok(()),
                },
                handoff = self.handoff_receiver.recv() => {
                    match handoff {
                        Ok(id) => {
                            // Recorded before the claim, so "nobody claimed it" can be told apart
                            // from "it was never delivered". Those need different fixes and the
                            // first E2E-B run could not distinguish them.
                            record_stage(
                                RecoveryStage::Rejected,
                                None,
                                Some(id),
                                Some(self.router.to_string()),
                                self.ctx.chain_participation().state().as_str(),
                                "handoff delivered to this flow; evaluating claim",
                            );
                            match self.claim_handoff(id) {
                                // This flow's peer offers the reserved chain, so it starts the IBD
                                // from the summary header — no waiting for an inv that may never
                                // come, and no window for another peer to take the latch instead.
                                Some(header) => header,
                                None => continue,
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return Ok(()),
                    }
                }
                nomination = self.challenger_receiver.recv() => {
                    match nomination {
                        Ok(id) => {
                            self.verify_challenger(id).await;
                            continue;
                        }
                        // Lagged: nominations are hints, and a missed one is re-sent on the next
                        // summary. Closed: nobody nominates any more, so just serve relay blocks.
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return Ok(()),
                    }
                }
            };

            // Answer any outstanding proof request naming this peer BEFORE syncing from it.
            //
            // This is the only moment this flow is reliably idle. A peer relays its tip within a
            // second of connecting, so the flow goes select → relay → ibd() → fail → disconnect,
            // and the idle tick below never fires for it. Measured, soak round 7: two candidates
            // nominated, one proof request ever sent, and thirty-three "nothing to nominate" while
            // the unserved request held the single verification slot. The peer that owed the proof
            // spent every one of those seconds inside an IBD that could not succeed.
            //
            // Narrow by construction: it does nothing unless participation is withheld and this
            // node has already decided it wants this peer's proof.
            self.serve_pending_nomination().await;

            if let Some(_guard) = self.ctx.try_set_ibd_running(self.router.key(), relay_header.daa_score, relay_header.blue_work) {
                info!("IBD started with peer {}", self.router);

                // Whatever happens next, the reservation has had its turn. Leaving it in place would
                // lock the node out of syncing from anyone else.
                let served_reservation =
                    self.ctx.preferred_ibd_candidate().is_some_and(|p| p.preferred_sources.contains(&self.router.key()));
                if served_reservation {
                    // It is being used, so it is not waiting. Restart the no-progress clock; the
                    // absolute lifetime keeps running, which is what bounds a retry loop.
                    self.ctx.note_preferred_candidate_claimed();
                }

                let outcome = self.ibd(relay_header).await;
                // The permit covered exactly this attempt, and so did the hold on the review.
                if self.active_recovery_permit.is_some() {
                    self.ctx.chain_participation().end_decision();
                }
                self.active_recovery_permit = None;
                // Release the reservation only when the attempt it authorised actually finished.
                // Clearing it on failure spends the handoff on one stumble and drops the node back
                // to the branch it had already decided against — measured: after the first permitted
                // attempt failed, every retry ran unpermitted.
                if served_reservation && outcome.is_ok() {
                    self.ctx.clear_preferred_ibd_candidate();
                }
                match outcome {
                    Ok(_) => {
                        info!("IBD with peer {} completed successfully", self.router);

                        self.report_unresolved_candidates();
                        self.ctx.finish_ibd_after_success(POST_IBD_CANDIDATE_REVIEW);
                        record_stage(
                            RecoveryStage::IbdStartedForPreferredCandidate,
                            None,
                            None,
                            Some(self.router.to_string()),
                            self.ctx.chain_participation().state().as_str(),
                            if served_reservation { "this IBD served a reservation" } else { "ordinary IBD" },
                        );
                    }
                    Err(e) => {
                        info!("IBD with peer {} completed with error: {}", self.router, e);
                        // `staging.commit()` runs partway through `ibd()`, so a failure here does not
                        // mean nothing happened — the active consensus may already have been replaced
                        // with a chain whose sync then failed.
                        if self.ctx.finish_ibd_after_failure() {
                            warn!(
                                "IBD with {} failed AFTER the active consensus had already been replaced. This node is now on a \
                                 chain whose sync never completed; participation is QUARANTINED until an operator intervenes.",
                                self.router
                            );
                            return Err(e);
                        }
                        // While the chain is still under review, a failed IBD does not cost the peer
                        // its connection.
                        //
                        // Returning Err here disconnects it, and the disconnect takes the candidate
                        // registry entry with it — `forget_peer` drops a candidate whose last source
                        // is gone. Measured, soak round 7: the peer offering the heavier chain
                        // connected, relayed, was handed the latch, failed at the pruning-proof
                        // comparison, and was dropped. The summary that arrived DURING that IBD had
                        // nominated its chain for verification; the disconnect deleted the
                        // nomination. Four nominations, one proof request ever sent, seven minutes
                        // on the lighter branch. The node kept destroying the evidence it needed.
                        //
                        // "Your proof does not compare against mine" is not misbehaviour. It is the
                        // exact situation the candidate machinery exists to resolve, and it cannot
                        // be resolved without the peer. The DoS argument is weaker here too: a node
                        // withholding participation is not mining or attesting, and every path this
                        // keeps open is separately rate-limited — summary cooldown, verification
                        // lease, per-peer failure count.
                        if !self.ctx.is_consensus_participation_allowed() {
                            info!(
                                "Keeping the connection to {} despite the failed IBD: this node is still reviewing its chain, and \
                                 a peer offering a different one is evidence rather than an offence.",
                                self.router
                            );
                            continue;
                        }
                        return Err(e);
                    }
                }
            }
        }
    }

    /// Whether this flow should serve a reserved chain, and the header to start from.
    ///
    /// The reservation names a chain and its sources, so several flows may see the same handoff;
    /// only one wins the latch, and the rest fall through harmlessly.
    fn claim_handoff(&self, id: CandidateId) -> Option<Arc<Header>> {
        // Every refusal is recorded separately. "The handoff was never claimed" has several
        // possible causes and they need different fixes, so the trace has to say which one — the
        // first E2E-B run could only report that nobody claimed it.
        let Some(preferred) = self.ctx.preferred_ibd_candidate() else {
            record_stage(
                RecoveryStage::Rejected,
                None,
                Some(id),
                Some(self.router.to_string()),
                self.ctx.chain_participation().state().as_str(),
                "handoff delivered but nothing is reserved any more",
            );
            return None;
        };
        if preferred.candidate_id != id {
            record_stage(
                RecoveryStage::Rejected,
                None,
                Some(id),
                Some(self.router.to_string()),
                self.ctx.chain_participation().state().as_str(),
                format!(
                    "handoff is for {} but the reservation names {}",
                    id.virtual_selected_parent, preferred.candidate_id.virtual_selected_parent
                ),
            );
            return None;
        }
        if !preferred.preferred_sources.contains(&self.router.key()) {
            record_stage(
                RecoveryStage::Rejected,
                None,
                Some(id),
                Some(self.router.to_string()),
                self.ctx.chain_participation().state().as_str(),
                format!(
                    "this flow's peer is not among the {} reserved source(s) for the candidate",
                    preferred.preferred_sources.len()
                ),
            );
            return None;
        }
        record_stage(
            RecoveryStage::HandoffReceived,
            None,
            Some(id),
            Some(self.router.to_string()),
            self.ctx.chain_participation().state().as_str(),
            "",
        );
        info!(
            "Taking over the sync: this peer ({}) offers candidate {}, which this node verified at blue work {}",
            self.router, id.virtual_selected_parent, preferred.verified_blue_work
        );
        Some(preferred.header)
    }

    /// Ask this peer to back the chain it advertised, and record what comes of it.
    ///
    /// This is the step that turns "someone claimed a heavier chain" into evidence the commit
    /// barrier can act on. Without it the barrier can only ever refuse, which is safe but means an
    /// operator has to adjudicate every partition by hand — testnet-22's problem, automated only up
    /// to the point of stopping.
    ///
    /// The work recorded is the proof's **pruning point** work, not the tip the peer claims. Proof
    /// validation takes the pruning-period work from the prover on trust (see `compare_proofs_inner`:
    /// "this work will eventually be verified if the proof is accepted"), so treating the tip claim
    /// as verified would let a peer preempt an honest sync with a number it made up — the very
    /// thing this whole path exists to prevent. What the proof genuinely establishes is the header
    /// chain down to the pruning point, so that is what gets compared.
    async fn verify_challenger(&mut self, id: CandidateId) {
        // Only a peer that actually offers this chain can be asked for its proof — and only the one
        // designated to serve it. A nomination reaches every flow, so without this every source
        // fetches and validates the same multi-megabyte proof simultaneously.
        // Both refusals below are recorded. Three separate diagnoses of one failing soak round were
        // wrong because this step declining was invisible: the trace showed nominations rising and
        // proof requests not, and every explanation for the gap had to be inferred. A step that can
        // refuse silently will be blamed for someone else's bug, or excused for its own.
        let (designated, sources, state_name) = {
            let registry = self.ctx.ibd_candidates().read();
            (registry.designated_prover(&id), registry.sources_of(&id), registry.get(&id).map(|c| c.validation.name()))
        };
        // Captures only copies, so it stays usable alongside the `&mut self` fetch below.
        let (me, conn, state_name_or, source_count, participation) = (
            self.router.key(),
            self.connection_generation,
            state_name.unwrap_or("<gone>"),
            sources.len(),
            self.ctx.chain_participation().state().as_str(),
        );
        let skip = move |reason| {
            verification_trace::record_skip(VerificationSkip {
                reason,
                candidate_id: id,
                connection_generation: conn,
                executing_peer: me,
                designated_peer: designated,
                candidate_state: state_name_or,
                live_sources: source_count,
                participation_state: participation,
            })
        };
        if state_name.is_none() {
            skip(SkipReason::CandidateNotFound);
            return;
        }
        if !sources.contains(&me) {
            skip(SkipReason::PeerNoLongerSource);
            return;
        }
        if designated != Some(me) {
            skip(if designated.is_none() { SkipReason::NoEligibleProver } else { SkipReason::NotDesignatedProver });
            return;
        }
        // The claim the peer must now back. Read here rather than inside the fetch so a candidate
        // already settled by another source's flow is skipped instead of re-verified.
        let validation = self.ctx.ibd_candidates().read().get(&id).map(|c| c.validation);
        let claimed_blue_work = match validation {
            Some(CandidateValidation::ProofRequested { claimed_blue_work, .. })
            | Some(CandidateValidation::SummaryReceived { claimed_blue_work }) => claimed_blue_work,
            _ => {
                skip(SkipReason::CandidateStateChanged);
                return;
            }
        };

        // The request may not outlive the lease that owns its slot. Not "a timeout shorter than the
        // lease" — that still lets a request started late run past the end of one. What is left of
        // THIS lease is the budget, and if too little is left the request is not started at all.
        //
        // The margin covers validating the proof after it arrives, which happens inside this same
        // flow and inside the same lease.
        let (attempt_stamp, budget) = {
            let registry = self.ctx.ibd_candidates().read();
            let stamp = registry.proof_attempt_stamp(&id);
            let budget = registry
                .proof_request_deadline(&id)
                .map(|deadline| deadline.saturating_duration_since(Instant::now()).saturating_sub(PROOF_VALIDATION_MARGIN));
            (stamp, budget)
        };
        let budget = budget.unwrap_or(CHALLENGER_PROOF_TIMEOUT).min(CHALLENGER_PROOF_TIMEOUT);
        if budget < MIN_USEFUL_PROOF_BUDGET {
            skip(SkipReason::LeaseTooShortToStart);
            return;
        }

        let attempt = RecoveryAttemptId::next();
        info!("Verifying chain candidate {} offered by {}", id.virtual_selected_parent, self.router);
        match self.fetch_and_validate_challenger_proof(attempt, id, claimed_blue_work, budget).await {
            Ok((verified_blue_work, proof_hash)) => {
                // The answer belongs to the attempt that asked for it. If this candidate has been
                // re-nominated since — a new lease, most likely to a different source — then the
                // peer whose reply this is was already judged too slow, and crediting the current
                // attempt with it would undo that judgement.
                if attempt_stamp.is_some() && self.ctx.ibd_candidates().read().proof_attempt_stamp(&id) != attempt_stamp {
                    skip(SkipReason::StaleProofResponse);
                    return;
                }
                record_stage(
                    RecoveryStage::ProofValidated,
                    Some(attempt),
                    Some(id),
                    Some(self.router.to_string()),
                    self.ctx.chain_participation().state().as_str(),
                    format!("verified_blue_work={verified_blue_work}"),
                );
                info!(
                    "Candidate {} from {} is backed by a valid pruning proof; verified blue work at its pruning point is {}",
                    id.virtual_selected_parent, self.router, verified_blue_work
                );
                self.ctx.ibd_candidates().write().set_validated(id, verified_blue_work, proof_hash);
                // A proof-backed candidate is now in play. Hold the review open past its floor: going
                // Ready while holding evidence that the chain might be wrong is the failure this
                // whole path exists to avoid. Released when the candidate is settled either way.
                self.ctx.chain_participation().begin_decision();
                self.consider_post_ibd_switch(id, verified_blue_work).await;
            }
            Err(e) => {
                // Failing to back a claim is the peer's failure. Recording the refusal is what stops
                // an unbackable claim from holding up a commit indefinitely.
                warn!(
                    "Candidate {} from {} could not be backed by a valid pruning proof: {}",
                    id.virtual_selected_parent, self.router, e
                );
                self.ctx
                    .set_ibd_candidate_validation(id, CandidateValidation::Rejected { reason: CandidateRejectReason::InvalidProof });
                // A candidate that cannot back its claim has no hold on this node's time.
                self.ctx.chain_participation().end_decision();
            }
        }
    }

    /// A permit to cross this node's own pruning point for `syncer_pruning_point`, if one is due.
    ///
    /// Gathers the facts and lets `authorize_bootstrap_recovery` judge them, so the policy is
    /// testable without a peer. Returns `None` on any refusal — including the ordinary case where
    /// the node has participated and the boundary simply stands.
    async fn bootstrap_recovery_permit_for(&self, syncer_pruning_point: BlockHash) -> Option<CandidateValidationPermit> {
        let gate = self.ctx.chain_participation();
        record_stage(
            RecoveryStage::RecoveryPermitRequested,
            None,
            None,
            Some(self.router.to_string()),
            gate.state().as_str(),
            format!("syncer_pruning_point={syncer_pruning_point}"),
        );
        let Some(reserved) = self.ctx.preferred_ibd_candidate() else {
            record_stage(
                RecoveryStage::Rejected,
                None,
                None,
                Some(self.router.to_string()),
                gate.state().as_str(),
                "no permit: nothing is reserved",
            );
            return None;
        };
        if reserved.candidate_id.pruning_point != syncer_pruning_point {
            record_stage(
                RecoveryStage::Rejected,
                None,
                Some(reserved.candidate_id),
                Some(self.router.to_string()),
                gate.state().as_str(),
                format!(
                    "no permit: reservation is for pruning point {} but this syncer offers {syncer_pruning_point}",
                    reserved.candidate_id.pruning_point
                ),
            );
            return None;
        }

        let (verified_blue_work, claimed_tip_blue_work, proof_hash) = {
            let registry = self.ctx.ibd_candidates().read();
            let candidate = registry.get(&reserved.candidate_id)?;
            let claimed = candidate.claimed_tip_blue_work()?;
            match candidate.validation {
                CandidateValidation::ProofValidated { verified_blue_work } => {
                    (Some(verified_blue_work), claimed, candidate.proof_hash?)
                }
                _ => return None,
            }
        };

        let session = self.ctx.consensus().session().await;
        let provisional_pruning_point = session.async_pruning_point().await;
        let _ = provisional_pruning_point;
        // The incumbent's TIP work, not its pruning point: the trigger asks whether the challenger
        // claims to beat the chain actually held, and pruning-point work cannot tell two branches of
        // comparable depth apart.
        let provisional_tip = session.async_get_headers_selected_tip().await;
        let provisional_blue_work = session.async_get_header(provisional_tip).await.ok().map(|h| h.blue_work)?;
        let descends_from_checkpoint = match self.ctx.config.trusted_checkpoint {
            Some(cp) => Some(
                session
                    .async_is_chain_ancestor_of(cp.block_hash, reserved.candidate_id.virtual_selected_parent)
                    .await
                    .unwrap_or(false),
            ),
            None => None,
        };
        drop(session);

        let state = ChainReviewState {
            participation: gate.state(),
            ever_ready: gate.ever_ready(),
            adoption_generation: gate.adoption_generation(),
            switch_count: gate.restored_switches(),
        };
        let candidate = VerifiedCandidate {
            id: reserved.candidate_id,
            verified_blue_work,
            claimed_tip_blue_work,
            proof_hash,
            genesis_hash: self.ctx.config.genesis.hash,
            consensus_params_id: self.ctx.config.params.consensus_params_id(),
            descends_from_checkpoint,
        };
        match authorize_candidate_validation(RecoveryRequest {
            state,
            candidate,
            provisional_blue_work,
            local_genesis: self.ctx.config.genesis.hash,
            local_consensus_params_id: self.ctx.config.params.consensus_params_id(),
            checkpoint: self.ctx.config.trusted_checkpoint.as_ref(),
            reserved_candidate: Some(reserved.candidate_id),
            switch_limit: MAX_CHAIN_SWITCHES,
        }) {
            Ok(permit) => {
                record_stage(
                    RecoveryStage::RecoveryPermitGranted,
                    None,
                    Some(reserved.candidate_id),
                    Some(self.router.to_string()),
                    gate.state().as_str(),
                    format!("adoption_generation={} switch_generation={}", permit.adoption_generation, permit.switch_generation),
                );
                Some(permit)
            }
            Err(e) => {
                record_stage(
                    RecoveryStage::Rejected,
                    None,
                    Some(reserved.candidate_id),
                    Some(self.router.to_string()),
                    gate.state().as_str(),
                    format!("permit refused: {e:?}"),
                );
                debug!("No bootstrap-recovery permit for candidate {}: {:?}", reserved.candidate_id.virtual_selected_parent, e);
                None
            }
        }
    }

    /// Pick up a nomination this flow was too busy to hear.
    ///
    /// Nominations are broadcast once. A flow inside `ibd()` does not reach its select, so the
    /// nomination waits in its receiver — and if that IBD fails, the peer is disconnected and the
    /// flow dies holding it. The candidate then sits in `ProofRequested` for a whole lease with
    /// nobody serving it, and the chain that most needed checking is the one least likely to get
    /// checked: the peer offering it is the one being disconnected every thirty seconds.
    ///
    /// So the broadcast is a wake-up, not the only delivery. The registry already holds the request;
    /// this reads it. `verify_challenger` re-checks the state and the source, so a candidate another
    /// flow is already serving is skipped rather than fetched twice.
    async fn serve_pending_nomination(&mut self) {
        if self.ctx.is_consensus_participation_allowed() {
            return;
        }
        let me = self.router.key();
        let pending = {
            let registry = self.ctx.ibd_candidates().read();
            registry.candidates_awaiting_proof().into_iter().map(|(id, _)| id).find(|id| registry.designated_prover(id) == Some(me))
        };
        if let Some(id) = pending {
            self.verify_challenger(id).await;
        }
    }

    /// Look again at candidates that validated while this node was busy.
    ///
    /// Cheap and idempotent: it returns immediately unless participation is withheld and the latch
    /// is free, and `consider_post_ibd_switch` re-applies every condition itself.
    async fn reconsider_validated_candidates(&self) {
        if self.ctx.is_consensus_participation_allowed() || self.ctx.is_ibd_running() {
            return;
        }
        // Before anything else: a reservation that has stopped making progress is holding the latch
        // shut against every other chain, including the ones examined just below. Its sources may
        // have disconnected minutes ago, and nothing else would ever notice.
        self.ctx.expire_stale_preferred_candidate();
        if self.ctx.preferred_ibd_candidate().is_some() {
            return; // a switch is already reserved
        }
        let pending: Vec<_> = self
            .ctx
            .ibd_candidates()
            .read()
            .validated_awaiting_decision()
            .iter()
            .filter_map(|c| c.verified_blue_work().map(|w| (c.id, w)))
            .collect();
        for (id, verified) in pending {
            self.consider_post_ibd_switch(id, verified).await;
            if self.ctx.preferred_ibd_candidate().is_some() {
                break;
            }
        }
    }

    /// Switch to a verified-better chain that was found AFTER an IBD already finished.
    ///
    /// The commit barrier only runs during an IBD, so without this a challenger discovered while the
    /// node is in review has nothing to act on it: it would sit on whichever branch it raced onto
    /// while the better peer retried an IBD forever. Measured against two independently pruned
    /// histories, that is exactly what happened.
    ///
    /// Only while participation is still withheld. Once the node is `Ready` it has committed to its
    /// chain, and abandoning it then is a reorg — governed by the DNS reorg gate, not by IBD source
    /// selection.
    async fn consider_post_ibd_switch(&self, id: CandidateId, verified_blue_work: BlueWorkType) {
        if self.ctx.is_consensus_participation_allowed() || self.ctx.is_ibd_running() {
            record_stage(
                RecoveryStage::Rejected,
                None,
                Some(id),
                Some(self.router.to_string()),
                self.ctx.chain_participation().state().as_str(),
                if self.ctx.is_ibd_running() {
                    "post-IBD switch skipped: an IBD is running (the commit barrier owns this decision)"
                } else {
                    "post-IBD switch skipped: node is already participating"
                },
            );
            return;
        }
        // What triggers an investigation is the peer's CLAIM about its tip, measured against this
        // node's own tip. Not the proof's pruning-point work.
        //
        // Pruning-point work is a function of depth, not of branch: two histories of comparable
        // length reach almost the same figure there. Measured across two real hosts, the challenger
        // and the incumbent came out exactly Equal — 80289507 against 80289507 — so a rule that
        // required strict superiority at this point could never fire, and the node fail-closed to
        // quarantine instead of converging.
        //
        // Using the claim here is what claims are for: deciding what to look at. It decides nothing
        // about adoption. `verified_blue_work` is still required to be present — the proof must have
        // validated — but the number that settles which chain wins is computed later, at the commit
        // barrier, from headers this node validated to the tip itself.
        let claimed_tip_work = match self.ctx.ibd_candidates().read().get(&id).map(|c| c.validation) {
            Some(CandidateValidation::ProofValidated { .. }) => {
                self.ctx.ibd_candidates().read().get(&id).and_then(|c| c.claimed_tip_blue_work())
            }
            _ => None,
        };
        let Some(claimed_tip_work) = claimed_tip_work else { return };

        let session = self.ctx.consensus().session().await;
        let our_tip = session.async_get_headers_selected_tip().await;
        let ours = session.async_get_header(our_tip).await.ok().map(|h| h.blue_work);
        drop(session);

        let Some(ours) = ours else { return };
        record_stage(
            RecoveryStage::CandidateCompared,
            None,
            Some(id),
            Some(self.router.to_string()),
            self.ctx.chain_participation().state().as_str(),
            format!(
                "trigger (claimed, decides nothing): {} | proof-backed pruning-point work {}",
                describe_comparison(claimed_tip_work, ours),
                verified_blue_work
            ),
        );
        if claimed_tip_work <= ours {
            record_stage(
                RecoveryStage::Rejected,
                None,
                Some(id),
                Some(self.router.to_string()),
                self.ctx.chain_participation().state().as_str(),
                format!("not worth investigating: {}", describe_comparison(claimed_tip_work, ours)),
            );
            return;
        }
        let switches = {
            let mut registry = self.ctx.ibd_candidates().write();
            registry.resume_switches(self.ctx.chain_participation().restored_switches());
            registry.note_switch();
            registry.switches()
        };
        self.ctx.chain_participation().record_switch(switches);
        if switches > MAX_CHAIN_SWITCHES {
            warn!(
                "Candidate {} is verified-better, but this node has already switched chains {} times; quarantining instead.",
                id.virtual_selected_parent, switches
            );
            self.ctx.chain_participation().quarantine();
            return;
        }
        if self.ctx.reserve_preferred_ibd_candidate(id, verified_blue_work) {
            warn!(
                "Candidate {} has a VALIDATED blue work of {} at its pruning point against this node's {}. Reserving the next \
                 IBD for it and handing over. (switch {} of {})",
                id.virtual_selected_parent, verified_blue_work, ours, switches, MAX_CHAIN_SWITCHES
            );
        }
    }

    async fn fetch_and_validate_challenger_proof(
        &mut self,
        attempt: RecoveryAttemptId,
        id: CandidateId,
        claimed_blue_work: ClaimedBlueWork,
        budget: Duration,
    ) -> Result<(BlueWorkType, kaspa_hashes::Hash), ProtocolError> {
        record_stage(
            RecoveryStage::ProofRequestSent,
            Some(attempt),
            Some(id),
            Some(self.router.to_string()),
            self.ctx.chain_participation().state().as_str(),
            "",
        );
        self.router.enqueue(make_message!(Payload::RequestPruningPointProof, RequestPruningPointProofMessage {})).await?;
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointProof, budget)?;
        record_stage(
            RecoveryStage::ProofReceived,
            Some(attempt),
            Some(id),
            Some(self.router.to_string()),
            self.ctx.chain_participation().state().as_str(),
            "",
        );
        let proof: PruningPointProof = Versioned(self.header_format, msg).try_into()?;

        // Validated against CURRENT consensus, exactly as a real IBD would: same rules, same
        // finality checks, no staging consensus spun up for a chain that may be refused anyway.
        //
        // The peer's own claim is what the proof is checked against — that is the point. It has to
        // produce a proof consistent with the number it asserted, so asserting a large one makes
        // the proof harder to supply, not easier.
        let proof_metadata = PruningProofMetadata::new(claimed_blue_work.for_priority_only());
        let consensus = self.ctx.consensus().session().await;
        // Validation runs on a blocking pool and can fail in ways that are not a returned Err — a
        // panic inside it takes the flow's task down with it, leaving neither a success nor a
        // recorded refusal. The previous E2E-B run showed exactly that shape (ProofReceived=1,
        // ProofValidated=0, zero rejections), so the attempt is announced before it starts.
        record_stage(
            RecoveryStage::Rejected,
            Some(attempt),
            Some(id),
            Some(self.router.to_string()),
            self.ctx.chain_participation().state().as_str(),
            format!("validating proof: levels={} claimed_blue_work={}", proof.len(), claimed_blue_work.for_priority_only()),
        );
        // Standalone: soundness only, no comparison against the chain this node is holding.
        //
        // The comparative form refuses an unrelated history outright ("no shared blocks with the
        // known level DAGs") before reporting whether it is valid, so it could never establish the
        // verified work that `decide_commit` and `authorize_bootstrap_recovery` then judge. Those
        // two ask the superiority question explicitly, on figures derived here — the check is not
        // skipped, it is moved to where it can be answered.
        let _ = &proof_metadata;
        let proof =
            consensus.clone().spawn_blocking(move |c| c.validate_pruning_proof_standalone(&proof).map(|()| proof)).await.inspect_err(
                |e| {
                    record_stage(
                        RecoveryStage::Rejected,
                        Some(attempt),
                        Some(id),
                        None,
                        "",
                        format!("validate_pruning_proof returned an error: {e}"),
                    )
                },
            )?;
        drop(consensus);

        let pruning_point_header = proof[0].last().ok_or(ProtocolError::Other("the challenger's proof has no level-0 headers"))?;
        if pruning_point_header.hash != id.pruning_point {
            return Err(ProtocolError::Other("the challenger's proof is anchored at a different pruning point than it advertised"));
        }

        // Digest the proof so a recovery permit is bound to the exact evidence that justified it.
        // A permit that merely named a chain could be redeemed against a different, weaker proof for
        // the same chain later.
        let mut hasher = kaspa_hashes::ConsensusParamsId::new();
        for level in proof.iter() {
            for header in level.iter() {
                hasher.write(header.hash.as_bytes());
            }
        }
        Ok((pruning_point_header.blue_work, hasher.finalize()))
    }

    /// Say what was on offer that this node never got to the bottom of.
    ///
    /// A candidate still sitting at `Observed` or `SummaryReceived` is one nobody verified: the
    /// commit barrier could not account for it, so the chain this node is on was not compared
    /// against it. That is worth saying out loud — the incident this work follows ran for 86
    /// minutes beside a heavier peer and produced no log line at all.
    fn report_unresolved_candidates(&self) {
        let registry = self.ctx.ibd_candidates().read();
        let unresolved = registry.unresolved();
        if unresolved.is_empty() {
            return;
        }
        warn!(
            "IBD with {} finished with {} chain candidate(s) still unverified: {}. Their claims were never backed by a \
             pruning proof, so this node's chain was not compared against them.",
            self.router,
            unresolved.len(),
            unresolved
                .iter()
                .map(|c| format!(
                    "{} (pruning point {}, {} source(s))",
                    c.id.virtual_selected_parent,
                    c.id.pruning_point,
                    c.sources.len()
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    /// The last check before this node's active consensus is replaced.
    ///
    /// `staging.commit()` is the point of no return: after it the old chain's state is gone, the
    /// new pruning point is adopted, and everything downstream — DNS overlay, validator support —
    /// follows the branch that was just installed. testnet-22 committed here with a heavier peer
    /// connected and nothing ever reconsidered.
    ///
    /// Two ways this refuses:
    ///
    /// - a candidate this node **verified** is strictly better than what staging holds. Verified,
    ///   not claimed: a peer shouting a large `blue_work` cannot cancel an IBD, or cancelling
    ///   would be free for anyone willing to lie.
    /// - a valid candidate exists that nobody could compare. Deciding between them by which peer
    ///   relayed first is the bug; quarantine and let an operator decide.
    async fn authorize_commit(&self, staging: &ConsensusProxy) -> Result<(), ProtocolError> {
        let tip = staging.async_get_headers_selected_tip().await;
        let staged_blue_work = staging.async_get_header(tip).await?.blue_work;

        // Gather the facts; `decide_commit` applies the policy. Split so the security-critical part
        // is unit-testable without a consensus, a router, or a peer.
        let (descends_from_checkpoint, checkpoint_params_match) = match self.ctx.config.trusted_checkpoint {
            Some(cp) => (
                Some(staging.async_is_chain_ancestor_of(cp.block_hash, tip).await.unwrap_or(false)),
                Some(cp.consensus_params_id == self.ctx.config.params.consensus_params_id()),
            ),
            None => (None, None),
        };

        // Separate rival branches from peers on this same chain that have simply moved on.
        //
        // Tip containment alone is not that test. A peer at B120 while staging holds B100 is the
        // same history seen further along, and its tip is unknown to staging precisely because it
        // is ahead — so "unknown tip means rival" would refuse a healthy sync every time a peer
        // produced a block during it. Which, over a multi-minute IBD, is always.
        //
        // Lineage is the test that survives that. A candidate rooted at the staged pruning point —
        // or at one of the recent pruning points staging descends through, matching the syncer-lag
        // tolerance `determine_ibd_type` already applies — is the same history whatever its tip. A
        // candidate rooted somewhere staging has never heard of is a genuinely different history,
        // and that is what must not be committed past.
        // A recovery IBD is an investigation, and this is where it is judged.
        //
        // Both figures are now this node's own: the challenger's, from headers it validated to the
        // tip in staging, and the incumbent's, from the chain it is holding. That is the comparison
        // pruning-point work could not make — measured across two real hosts, the two branches were
        // exactly Equal there, and this node quarantined rather than converging.
        //
        // Losing is an ordinary outcome, not an impasse: staging is cancelled and the incumbent
        // stays. Nothing was adopted, so there is nothing to be uncertain about, and quarantining
        // here would punish the node for having checked.
        // The second permit. A validation permit bought the right to sync and check this chain; only
        // an adoption permit lets it replace what is held, and that is earned here, from figures
        // this node computed on both sides.
        if let Some(validation_permit) = self.active_recovery_permit {
            // Read the incumbent as late as possible: it has been taking blocks from its own peers
            // for the whole of the staging validation, so the figure that mattered when this started
            // is not the one that matters now.
            let incumbent = {
                let session = self.ctx.consensus().session().await;
                let tip = session.async_get_headers_selected_tip().await;
                session.async_get_header(tip).await.ok().map(|h| ChainTip { tip, blue_work: h.blue_work })
            };
            let Some(incumbent) = incumbent else {
                return Err(ProtocolError::Other("cannot read the incumbent chain's tip work to judge a recovery candidate"));
            };

            let gate = self.ctx.chain_participation();
            let state = ChainReviewState {
                participation: gate.state(),
                ever_ready: gate.ever_ready(),
                adoption_generation: gate.adoption_generation(),
                switch_count: gate.restored_switches(),
            };
            let validated = ValidatedCandidate {
                id: validation_permit.candidate_id,
                verified_tip: tip,
                verified_blue_work: staged_blue_work,
                proof_hash: validation_permit.proof_hash,
            };
            record_stage(
                RecoveryStage::CandidateCompared,
                None,
                Some(validation_permit.candidate_id),
                Some(self.router.to_string()),
                gate.state().as_str(),
                format!("adoption verdict on VERIFIED tip work: {}", describe_comparison(staged_blue_work, incumbent.blue_work)),
            );

            match authorize_candidate_adoption(&state, &validation_permit, &validated, &incumbent) {
                Ok(adoption) => {
                    // Re-checked against the incumbent read a moment ago, so a permit cannot outlive
                    // the comparison that earned it.
                    if !adoption.still_applies(&state, &incumbent) {
                        return Err(ProtocolError::Other("the incumbent chain moved while this candidate was being validated"));
                    }
                    *self.earned_adoption_permit.lock() = Some(adoption);
                    record_stage(
                        RecoveryStage::RecoveryPermitGranted,
                        None,
                        Some(adoption.candidate_id),
                        Some(self.router.to_string()),
                        gate.state().as_str(),
                        format!("adoption permit: verified_blue_work={} tip={}", adoption.verified_blue_work, adoption.verified_tip),
                    );
                }
                // Validated and genuinely worse: an ordinary negative result. Keep the incumbent,
                // drop staging, do not quarantine — nothing was adopted, so nothing is uncertain,
                // and quarantining here would punish the node for having checked.
                Err(AdoptionError::Weaker) => {
                    warn!(
                        "Validated the chain offered by {} to its tip and it is weaker than the one held ({}). Keeping the \
                         incumbent.",
                        self.router,
                        describe_comparison(staged_blue_work, incumbent.blue_work)
                    );
                    return Err(ProtocolError::OtherOwned(format!(
                        "recovery candidate from {} is weaker once validated",
                        self.router
                    )));
                }
                // Two chains this node validated and cannot separate. Not adopted, and not waved
                // through either: choosing between equals is exactly what it must not do alone.
                Err(e) => {
                    self.ctx.chain_participation().quarantine();
                    return Err(ProtocolError::OtherOwned(format!(
                        "refusing to adopt the chain from {}: {:?} (verified {} against incumbent {}). This node cannot \
                         separate these histories and will not pick one by itself.",
                        self.router, e, staged_blue_work, incumbent.blue_work
                    )));
                }
            }
        }

        // Refuse candidates whose source never delivered a proof, before counting what is still
        // unresolved. Otherwise "advertise a chain and go quiet" would hold up every commit — the
        // denial of service that fail-closed invites if it has no deadline.
        let timed_out = self.ctx.expire_stale_verifications();
        if !timed_out.is_empty() {
            self.ctx.chain_participation().end_decision();
        }
        for id in &timed_out {
            warn!(
                "Chain candidate {} was nominated for verification but no source produced a pruning proof within its lease. \
                 It stops holding up the commit; it may still be checked again, since a source cut off mid-proof has said \
                 nothing about whether its chain is real.",
                id.virtual_selected_parent
            );
        }

        let staged_pruning_point = staging.async_pruning_point().await;
        let recent_pruning_points = staging.async_get_n_last_pruning_points(4).await;
        let unresolved_ids: Vec<_> = self.ctx.ibd_candidates().read().unresolved().iter().map(|c| c.id).collect();
        let mut unresolved_competing = 0usize;
        for id in unresolved_ids {
            let same_lineage = id.pruning_point == staged_pruning_point
                || recent_pruning_points.contains(&id.pruning_point)
                // Staging knowing the tip settles it outright — same history, at or behind us.
                || staging.async_get_block_status(id.virtual_selected_parent).await.is_some()
                // Or the candidate is rooted in staging's future along the same chain.
                || staging.async_is_chain_ancestor_of(staged_pruning_point, id.pruning_point).await.unwrap_or(false);
            if !same_lineage {
                unresolved_competing += 1;
            }
        }

        let verdict = {
            let registry = self.ctx.ibd_candidates().read();
            // Record the comparison here, where it is actually decided.
            //
            // It was only recorded in the post-IBD switch path before, which returns early while an
            // IBD is running — so a run whose outcome was decided right here reported
            // CandidateCompared=0 and looked as though no comparison had happened at all. The
            // measurement has to sit at the decision, not next to it.
            let best = registry.best_verified();
            record_stage(
                RecoveryStage::CandidateCompared,
                None,
                best.map(|c| c.id),
                Some(self.router.to_string()),
                self.ctx.chain_participation().state().as_str(),
                match best.and_then(|c| c.verified_blue_work()) {
                    Some(challenger) => {
                        format!("at commit barrier: {}", describe_comparison(challenger, staged_blue_work))
                    }
                    None => format!("at commit barrier: no verified candidate to compare; staged_work={staged_blue_work}"),
                },
            );
            decide_commit(CommitInputs {
                staged_blue_work,
                descends_from_checkpoint,
                checkpoint_params_match,
                unresolved_competing,
                registry: &registry,
            })
        };

        // A verified-better candidate is not a dead end — it is the answer.
        //
        // Abandoning this sync releases the latch, and the relay guard stops discarding that peer's
        // blocks the moment it does; its next inv triggers a fresh IBD from it. That IBD runs the
        // ordinary pipeline, so the chain finally adopted has had its headers CONTEXTUALLY
        // validated all the way to the tip — the thing a pruning proof alone cannot establish, and
        // which one staging consensus per node means can only be done for the chain being synced.
        //
        // The registry survives the handover, so the winner's verification is not repeated and the
        // loser cannot quietly win the next race.
        if let CommitVerdict::RefuseVerifiedSuperior { candidate, verified_blue_work } = verdict {
            let switches = {
                let mut registry = self.ctx.ibd_candidates().write();
                // Fold in anything carried over from a previous run before counting this one, so the
                // cap bounds the node's history rather than this process's.
                registry.resume_switches(self.ctx.chain_participation().restored_switches());
                registry.note_switch();
                registry.switches()
            };
            self.ctx.chain_participation().record_switch(switches);
            if switches <= MAX_CHAIN_SWITCHES {
                // Reserve the latch for the winner BEFORE releasing it. Without this the next peer
                // to relay anything takes it — possibly the branch just rejected — and the switch
                // would be decided by arrival order all over again.
                let reserved = self.ctx.reserve_preferred_ibd_candidate(candidate, verified_blue_work);
                if !reserved {
                    self.ctx.chain_participation().quarantine();
                    return Err(ProtocolError::OtherOwned(format!(
                        "candidate {} is verified-better than the chain synced from {}, but no connected peer still offers it, \
                         so this node cannot switch to it and will not commit the weaker branch either.",
                        candidate.virtual_selected_parent, self.router
                    )));
                }
                warn!(
                    "Abandoning the sync from {}: candidate {} (pruning point {}) has a VALIDATED blue work of {} against the \
                     staged {}. Switching to it — its headers will be contextually validated to the tip by the IBD that \
                     follows, which is the only way this node can establish that work rather than take it on trust. \
                     (switch {} of {})",
                    self.router,
                    candidate.virtual_selected_parent,
                    candidate.pruning_point,
                    verified_blue_work,
                    staged_blue_work,
                    switches,
                    MAX_CHAIN_SWITCHES
                );
                // Deliberately no quarantine: the node is not stuck, it is changing its mind on
                // evidence. Participation stays closed because the gate is still in IbdRunning.
                return Err(ProtocolError::OtherOwned(format!(
                    "switching from {} to a verified-better chain candidate {}",
                    self.router, candidate.virtual_selected_parent
                )));
            }
            self.ctx.chain_participation().quarantine();
            return Err(ProtocolError::OtherOwned(format!(
                "refusing to commit the chain synced from {}: candidate {} is verified-better, but this node has already \
                 switched chains {} times. Two branches trading the latch is a different problem from being on the wrong \
                 one, and this node cannot tell them apart on its own.",
                self.router, candidate.virtual_selected_parent, switches
            )));
        }

        let refusal = match verdict {
            CommitVerdict::Allow => return Ok(()),
            CommitVerdict::RefuseCheckpointParamsMismatch => {
                let cp = self.ctx.config.trusted_checkpoint.expect("set when this verdict is reachable");
                format!(
                    "--trusted-checkpoint was taken under consensus params {} but this node runs {}. A block hash means \
                     nothing without the rules it was validated under, so this node cannot act on that checkpoint.",
                    cp.consensus_params_id,
                    self.ctx.config.params.consensus_params_id()
                )
            }
            CommitVerdict::RefuseCheckpointMissing => {
                let cp = self.ctx.config.trusted_checkpoint.expect("set when this verdict is reachable");
                format!(
                    "refusing to commit the chain synced from {}: it does not descend from the trusted checkpoint {} at DAA \
                     {}. That is the history this operator vouched for, so a chain without it is not admissible however much \
                     work it claims.",
                    self.router, cp.block_hash, cp.daa_score
                )
            }
            // Handled above: a verified-better candidate is a switch, not a refusal.
            CommitVerdict::RefuseVerifiedSuperior { .. } => unreachable!("handled before this match"),
            CommitVerdict::RefuseUnresolved { count } => format!(
                "refusing to commit the chain synced from {}: {} other chain candidate(s) are on offer and none could be \
                 verified in time. Choosing by arrival order is what fixes a partition in place, so this node is \
                 quarantined until an operator resolves which branch is canonical.",
                self.router, count
            ),
        };

        self.ctx.chain_participation().quarantine();
        Err(ProtocolError::OtherOwned(refusal))
    }

    async fn ibd(&mut self, relay_header: Arc<Header>) -> Result<(), ProtocolError> {
        let mut session = self.ctx.consensus().session().await;

        // Claim a recovery permit up front when this peer serves the chain this node has reserved.
        //
        // It used to be requested only on the finality-conflict branch of `determine_ibd_type`, but
        // that branch is not the only way in: two histories sharing nothing but genesis can leave
        // `highest_known_syncer_chain_hash` empty, in which case the IBD takes the ordinary
        // headers-proof route, holds no permit, and then fails proof validation comparatively —
        // against the very provisional chain the permit exists to replace. Measured: E2E-B reached
        // ProofValidated and PreferredCandidateReserved with FinalityConflictDetected=0.
        //
        // The permit governs the chain, so it is claimed for the attempt, not for one code path.
        if let Some(reserved) = self.ctx.preferred_ibd_candidate()
            && reserved.preferred_sources.contains(&self.router.key())
            && self.active_recovery_permit.is_none()
        {
            self.active_recovery_permit = self.bootstrap_recovery_permit_for(reserved.candidate_id.pruning_point).await;
        }

        let negotiation_output = self.negotiate_missing_syncer_chain_segment(&session).await?;
        let ibd_type = self
            .determine_ibd_type(
                &session,
                relay_header.as_ref(),
                negotiation_output.highest_known_syncer_chain_hash,
                negotiation_output.syncer_pruning_point,
            )
            .await?;
        record_stage(
            RecoveryStage::Rejected,
            None,
            None,
            Some(self.router.to_string()),
            self.ctx.chain_participation().state().as_str(),
            format!(
                "ibd_type={} recovery_permit={}",
                match &ibd_type {
                    IbdType::Sync { .. } => "Sync",
                    IbdType::DownloadHeadersProof => "DownloadHeadersProof",
                    IbdType::PruningCatchUp { .. } => "PruningCatchUp",
                },
                self.active_recovery_permit.is_some()
            ),
        );
        match ibd_type {
            IbdType::Sync { highest_known_syncer_chain_hash, is_utxo_stable, is_pp_anticone_synced } => {
                let pruning_point = session.async_pruning_point().await;

                info!("syncing ahead from current pruning point");
                // Following IBD catchup a new pruning point is designated and finalized in consensus. Blocks from its anticone (including itself)
                // have undergone normal header verification, but contain no body yet. Processing of new blocks in the pruning point's future cannot proceed
                // since these blocks' parents are missing block data.
                // Hence we explicitly process bodies of the currently body missing anticone blocks as trusted blocks
                // Notice that this is degenerate following sync_with_headers_proof
                // but not necessarily so after sync_headers -
                // as it might sync following a previous pruning_catch_up that crashed before this stage concluded
                if !is_pp_anticone_synced {
                    self.sync_missing_trusted_bodies(&session).await?;
                }
                if !is_utxo_stable
                // Utxo might not be available even if the pruning point block data is.
                // Utxo must be synced before all so the node could function
                {
                    info!(
                        "utxoset corresponding to the current pruning point is incomplete, attempting to download it from {}",
                        self.router
                    );

                    // Imports the pruning point's utxoset AND (ADR-0022) its EVM + overlay sidecars
                    // atomically before marking the utxoset stable — see sync_new_utxo_set.
                    self.sync_new_utxo_set(&session, pruning_point).await?;
                }
                // Once utxo is valid, simply sync missing headers
                self.sync_headers(
                    &session,
                    negotiation_output.syncer_virtual_selected_parent,
                    highest_known_syncer_chain_hash,
                    &relay_header,
                )
                .await?;
            }
            IbdType::DownloadHeadersProof => {
                drop(session); // Avoid holding the previous consensus throughout the staging IBD
                let staging = self.ctx.consensus_manager.new_staging_consensus();
                match self.ibd_with_headers_proof(&staging, negotiation_output.syncer_virtual_selected_parent, &relay_header).await {
                    Ok(()) => {
                        // The commit barrier. Everything above this line is reversible by
                        // cancelling staging; nothing below it is.
                        if let Err(e) = self.authorize_commit(&staging.session().await).await {
                            warn!("{}", e);
                            staging.cancel();
                            return Err(e);
                        }
                        record_stage(
                            RecoveryStage::CandidateCommitted,
                            None,
                            None,
                            Some(self.router.to_string()),
                            self.ctx.chain_participation().state().as_str(),
                            "staging committed",
                        );
                        // The swap, with the adoption decision re-checked against the defender
                        // under the same lock that performs it.
                        //
                        // The permit was earned from a reading of the active consensus, and the
                        // active consensus kept taking blocks from its own peers for every step
                        // between that reading and here — expiring stale verifications, reading
                        // pruning points, counting unresolved candidates, returning through two
                        // stack frames. Checking before calling `commit` leaves all of that inside
                        // the window, and whatever happens in it, the swap goes through anyway.
                        //
                        // `commit_if` closes the window rather than narrowing it: the predicate runs
                        // while the manager is held for writing, so nothing can move the defender
                        // between the check and the replacement.
                        let permit = self.earned_adoption_permit.lock().take();
                        let gate = self.ctx.chain_participation();
                        let state_now = ChainReviewState {
                            participation: gate.state(),
                            ever_ready: gate.ever_ready(),
                            adoption_generation: gate.adoption_generation(),
                            switch_count: gate.restored_switches(),
                        };
                        let committed = spawn_blocking(move || {
                            staging.commit_if(|active| {
                                let Some(permit) = permit else {
                                    return true; // no adoption decision rode on the defender
                                };
                                // Blocking session: this runs inside spawn_blocking, under the
                                // manager's write lock, so it must not await.
                                let session = active.unguarded_session_blocking();
                                let tip = session.get_headers_selected_tip();
                                match session.get_header(tip) {
                                    Ok(h) => permit.still_applies(&state_now, &ChainTip { tip, blue_work: h.blue_work }),
                                    // Unreadable defender: refuse. Committing on a reading that
                                    // failed is how a permit gets applied to a chain nobody checked.
                                    Err(_) => false,
                                }
                            })
                        })
                        .await
                        .unwrap();
                        if let Err(staging) = committed {
                            staging.cancel();
                            self.ctx.chain_participation().end_decision();
                            return Err(ProtocolError::Other(
                                "the incumbent chain moved between earning the adoption permit and the swap; refusing to \
                                 replace it on a stale comparison",
                            ));
                        }
                        // From here the node runs the new chain regardless of what the rest of the
                        // IBD does, so a later failure must not be reported and forgotten.
                        self.ctx.mark_active_consensus_replaced();
                        info!(
                            "Header download stage of IBD with headers proof completed successfully from {}. Committed staging consensus.",
                            self.router
                        );

                        // This will reobtain the freshly committed staging consensus
                        session = self.ctx.consensus().session().await;
                        // Next, sync a utxoset corresponding to the new pruning point from the syncer.
                        // Note that the new pruning point's anticone need not be downloaded separately as in other IBD types
                        // as it was just downloaded as part of the headers proof.
                        // Imports the new pruning point's utxoset AND (ADR-0022) its EVM + overlay sidecars
                        // atomically before marking the utxoset stable — see sync_new_utxo_set. Without the
                        // sidecars the first post-pruning block re-executes EVM from an empty genesis state /
                        // recomputes overlay rewards from empty state and is disqualified (with all descendants).
                        self.sync_new_utxo_set(&session, negotiation_output.syncer_pruning_point).await?;
                    }
                    Err(e) => {
                        warn!("IBD with headers proof from {} was unsuccessful ({})", self.router, e);
                        staging.cancel();
                        return Err(e);
                    }
                }
            }
            IbdType::PruningCatchUp { highest_known_syncer_chain_hash } => {
                info!("catching up to new pruning point {} ", negotiation_output.syncer_pruning_point);
                match self.pruning_point_catchup(&session, &negotiation_output, &relay_header, highest_known_syncer_chain_hash).await {
                    Ok(()) => {
                        info!("header stage of pruning catchup from peer {} completed", self.router);
                        self.sync_missing_trusted_bodies(&session).await?;
                        // Imports the new pruning point's utxoset AND (ADR-0022) its EVM + overlay sidecars
                        // atomically before marking the utxoset stable — see sync_new_utxo_set.
                        self.sync_new_utxo_set(&session, negotiation_output.syncer_pruning_point).await?;
                        // Note that pruning of old data will only occur once virtual has caught up sufficiently far
                    }

                    Err(e) => {
                        warn!("IBD catchup from peer {} was unsuccessful ({})", self.router, e);
                        return Err(e);
                    }
                }
            }
        }

        // Sync missing bodies in the past of syncer sink (virtual selected parent)
        self.sync_missing_block_bodies(&session, negotiation_output.syncer_virtual_selected_parent).await?;

        // Relay block might be in the antipast of syncer sink, thus
        // check its past for missing bodies as well.
        self.sync_missing_block_bodies(&session, relay_header.hash).await?;

        // Following IBD we revalidate orphans since many of them might have been processed during the IBD
        // or are now processable
        let (queued_hashes, virtual_processing_tasks) = self.ctx.revalidate_orphans(&session).await;
        let mut unorphaned_hashes = Vec::with_capacity(queued_hashes.len());
        let results = join_all(virtual_processing_tasks).await;
        for (hash, result) in queued_hashes.into_iter().zip(results) {
            match result {
                Ok(_) => unorphaned_hashes.push(hash),
                // We do not return the error and disconnect here since we don't know
                // that this peer was the origin of the orphan block
                Err(e) => warn!("Validation failed for orphan block {}: {}", hash, e),
            }
        }
        match unorphaned_hashes.len() {
            0 => {}
            n => info!("IBD post processing: unorphaned {} blocks ...{}", n, unorphaned_hashes.last().unwrap()),
        }

        Ok(())
    }

    async fn determine_ibd_type(
        &mut self,
        consensus: &ConsensusProxy,
        relay_header: &Header,
        highest_known_syncer_chain_hash: Option<BlockHash>,
        syncer_pruning_point: BlockHash,
    ) -> Result<IbdType, ProtocolError> {
        if let Some(highest_known_syncer_chain_hash) = highest_known_syncer_chain_hash {
            let pruning_point = consensus.async_pruning_point().await;
            let sink = consensus.async_get_sink().await;
            info!("current sink is:{}", sink);
            info!("current pruning point is:{}", pruning_point);
            if consensus.async_is_chain_ancestor_of(pruning_point, highest_known_syncer_chain_hash).await? {
                /// Categorizes the syncer's pruning point position relative to local
                enum SyncerSkew {
                    Lagging,
                    Aligned,
                    Leading,
                }

                let syncer_skew = if syncer_pruning_point == pruning_point {
                    SyncerSkew::Aligned
                } else if consensus.async_is_chain_ancestor_of(pruning_point, syncer_pruning_point).await.unwrap_or(false) {
                    SyncerSkew::Leading
                } else if consensus.async_get_n_last_pruning_points(4 /*syncer lag tolerance*/).await.contains(&syncer_pruning_point) {
                    SyncerSkew::Lagging
                } else {
                    return Err(ProtocolError::Other(
                        "The syncer purports to have data in the recent future but their pruning point could not be easily recognized",
                    ));
                };

                let is_utxo_stable = consensus.async_is_pruning_utxoset_stable().await;
                let is_pp_anticone_synced = consensus.async_is_pruning_point_anticone_fully_synced().await;

                return match (syncer_skew, is_utxo_stable && is_pp_anticone_synced) {
                    (SyncerSkew::Aligned, _) => {
                        Ok(IbdType::Sync { highest_known_syncer_chain_hash, is_utxo_stable, is_pp_anticone_synced })
                    }
                    (SyncerSkew::Lagging, true) => {
                        Ok(IbdType::Sync { highest_known_syncer_chain_hash, is_utxo_stable, is_pp_anticone_synced })
                    }
                    (SyncerSkew::Lagging, false) => Err(ProtocolError::Other(
                        "Local node is in a transitional state requiring external data to stabilize, but the syncer lags behind and is unable to provide said data",
                    )),
                    (SyncerSkew::Leading, true) => {
                        if consensus.async_get_block_status(syncer_pruning_point).await.is_some_and(|b| b.has_block_body()) {
                            // While a leading syncer skew often indicates the need for catchup, in this case
                            // the node is just missing a segment in the future of its current pruning point, that is available to the syncer
                            Ok(IbdType::Sync { highest_known_syncer_chain_hash, is_utxo_stable, is_pp_anticone_synced })
                        } else {
                            Ok(IbdType::PruningCatchUp { highest_known_syncer_chain_hash })
                        }
                    }
                    (SyncerSkew::Leading, false) => Ok(IbdType::PruningCatchUp { highest_known_syncer_chain_hash }),
                };
            }

            // If the pruning point is not in the chain of `highest_known_syncer_chain_hash`, it
            // means it's in its antichain (because if `highest_known_syncer_chain_hash` was in
            // the pruning point's past the pruning point itself would be
            // `highest_known_syncer_chain_hash`). So it means there's a finality conflict.
            //
            // TODO (relaxed): consider performing additional actions on finality conflicts in addition
            // to disconnecting from the peer (e.g., banning, rpc notification)
            //
            // Unless this node holds a permit to cross its own pruning point, which it can only do
            // for a chain it has never acted on. See `bootstrap_recovery`: before the node reaches
            // `Ready`, the chain under it was chosen by a race and is provisional, so treating it as
            // a finality boundary is what makes the first peer's chain permanent. After `Ready` this
            // is unreachable and the conflict stands — reorg policy belongs to the DNS gate.
            record_stage(
                RecoveryStage::FinalityConflictDetected,
                None,
                None,
                Some(self.router.to_string()),
                self.ctx.chain_participation().state().as_str(),
                format!("syncer_pruning_point={syncer_pruning_point}"),
            );
            if let Some(permit) = self.bootstrap_recovery_permit_for(syncer_pruning_point).await {
                warn!(
                    "Crossing this node's own pruning point to adopt candidate {} from {}: the chain currently held is \
                     provisional (this node has never participated on it) and {} is verified-better. Permit is bound to \
                     adoption generation {} and switch {}.",
                    permit.candidate_id.virtual_selected_parent,
                    self.router,
                    permit.candidate_id.pruning_point,
                    permit.adoption_generation,
                    permit.switch_generation
                );
                self.active_recovery_permit = Some(permit);
                return Ok(IbdType::DownloadHeadersProof);
            }
            return Err(ProtocolError::Other("peer is in a finality conflict with the local pruning point"));
        }

        let hst_header = consensus.async_get_header(consensus.async_get_headers_selected_tip().await).await.unwrap();
        let pruning_depth = self.ctx.config.pruning_depth();
        if relay_header.blue_score >= hst_header.blue_score + pruning_depth && relay_header.blue_work > hst_header.blue_work {
            let finality_duration_in_milliseconds = self.ctx.config.finality_duration_in_milliseconds();
            if unix_now() > consensus.async_creation_timestamp().await + finality_duration_in_milliseconds {
                let fp = consensus.async_finality_point().await;
                let fp_ts = consensus.async_get_header(fp).await?.timestamp;
                if unix_now() < fp_ts + finality_duration_in_milliseconds * 3 / 2 {
                    // We reject the headers proof if the node has a relatively up-to-date finality point and current
                    // consensus has matured for long enough (and not recently synced). This is mostly a spam-protector
                    // since subsequent checks identify these violations as well
                    // TODO (relaxed): consider performing additional actions on finality conflicts in addition to disconnecting from the peer (e.g., banning, rpc notification)
                    return Err(ProtocolError::Other(
                        "peer has no known block but local consensus appears to be up to date, this is most likely a spam attempt",
                    ));
                }
            }

            // The relayed block has sufficient blue score and blue work over the current header selected tip
            Ok(IbdType::DownloadHeadersProof)
        } else {
            Err(ProtocolError::Other("peer has no known block but conditions for requesting headers proof are not met"))
        }
    }

    /// This function is triggered when the syncer's pruning point is higher
    /// than ours and we already processed its header before.
    /// so we only need to sync more headers and set it to our new pruning point before proceeding with IBD
    async fn pruning_point_catchup(
        &mut self,
        consensus: &ConsensusProxy,
        negotiation_output: &ChainNegotiationOutput,
        relay_header: &Arc<Header>,
        highest_known_syncer_chain_hash: BlockHash,
    ) -> Result<(), ProtocolError> {
        // Before attempting to update to the syncer's pruning point, sync to the latest headers of the syncer,
        // to ensure that we will locally have sufficient headers on top of the syncer's pruning point
        let syncer_pp = negotiation_output.syncer_pruning_point;
        let syncer_sink = negotiation_output.syncer_virtual_selected_parent;
        self.sync_headers(consensus, syncer_sink, highest_known_syncer_chain_hash, relay_header).await?;

        // This function's main effect is to confirm the syncer's pruning point can be finalized into the consensus, and to update
        // all the relevant stores
        consensus.async_intrusive_pruning_point_update(syncer_pp, syncer_sink).await?;

        // A sanity check to confirm that following the intrusive addition of new pruning points,
        // the latest pruning point still correctly agrees with the DAG data,
        // and is the head of a pruning points "chain" leading all the way down to genesis
        // TODO (relaxed): once the catchup functionality has sufficiently matured, consider only doing this test if sanity checks are enabled
        info!("validating pruning points consistency");
        consensus.async_validate_pruning_points(syncer_sink).await.unwrap();
        info!("pruning points consistency validated");
        Ok(())
    }

    async fn ibd_with_headers_proof(
        &mut self,
        staging: &StagingConsensus,
        syncer_virtual_selected_parent: BlockHash,
        relay_header: &Arc<Header>,
    ) -> Result<(), ProtocolError> {
        info!("Starting IBD with headers proof with peer {}", self.router);

        let staging_session = staging.session().await;

        let pruning_point = self.sync_and_validate_pruning_proof(&staging_session, relay_header).await?;
        self.sync_headers(&staging_session, syncer_virtual_selected_parent, pruning_point, relay_header).await?;
        staging_session.async_validate_pruning_points(syncer_virtual_selected_parent).await?;
        // The third guard of the same family, and the same exception applies.
        //
        // It requires the incoming chain's tip to be at least ten minutes newer than the local one —
        // a sanity check that a peer is not selling an old chain. Against a PROVISIONAL local chain
        // it asks the wrong question: a genuinely heavier branch mined in parallel is the same age,
        // not newer, and here it is the heavier one by verified work. Measured: the permitted
        // attempt cleared both finality gates and died here.
        if self.active_recovery_permit.is_none() {
            self.validate_staging_timestamps(&self.ctx.consensus().session().await, &staging_session).await?;
        }
        Ok(())
    }

    async fn sync_and_validate_pruning_proof(
        &mut self,
        staging: &ConsensusProxy,
        relay_header: &Arc<Header>,
    ) -> Result<BlockHash, ProtocolError> {
        self.router.enqueue(make_message!(Payload::RequestPruningPointProof, RequestPruningPointProofMessage {})).await?;

        // Pruning proof generation and communication might take several minutes, so we allow a long 10 minute timeout
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointProof, Duration::from_secs(600))?;
        let proof: PruningPointProof = Versioned(self.header_format, msg).try_into()?;
        info!(
            "Received headers proof with overall {} headers ({} unique)",
            proof.iter().map(|l| l.len()).sum::<usize>(),
            proof.iter().flatten().unique_by(|h| h.hash).count()
        );

        let proof_metadata = PruningProofMetadata::new(relay_header.blue_work);

        // Get a new session for current consensus (non staging)
        let consensus = self.ctx.consensus().session().await;

        // The proof is validated in the context of current consensus — comparatively, so a peer
        // cannot hand this node a worse chain than the one it already has.
        //
        // Unless this IBD holds a bootstrap-recovery permit, in which case the chain it is being
        // compared against is provisional: adopted by a race, never acted upon, and precisely what
        // the permit authorises replacing. Comparing against it would refuse an unrelated history
        // before reporting whether it is sound, which is the same wall the challenger check hit.
        // Superiority was already established on verified figures when the permit was issued.
        let recovering = self.active_recovery_permit.is_some();
        let proof = consensus
            .clone()
            .spawn_blocking(move |c| {
                if recovering {
                    c.validate_pruning_proof_standalone(&proof).map(|()| proof)
                } else {
                    c.validate_pruning_proof(&proof, &proof_metadata).map(|()| proof)
                }
            })
            .await?;

        let proof_pruning_point = proof[0].last().expect("was just ensured by validation").hash;

        if proof_pruning_point == self.ctx.config.genesis.hash {
            return Err(ProtocolError::Other("the proof pruning point is the genesis block"));
        }

        if proof_pruning_point == consensus.async_pruning_point().await {
            return Err(ProtocolError::Other("the proof pruning point is the same as the current pruning point"));
        }
        drop(consensus);

        self.router
            .enqueue(make_message!(Payload::RequestPruningPointAndItsAnticone, RequestPruningPointAndItsAnticoneMessage {}))
            .await?;
        // First, all pruning points up to the last are sent
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPoints)?;
        let pruning_points: PruningPointsList = Versioned(self.header_format, msg).try_into()?;

        if pruning_points.is_empty() || pruning_points.last().unwrap().hash != proof_pruning_point {
            return Err(ProtocolError::Other("the proof pruning point is not equal to the last pruning point in the list"));
        }

        if pruning_points.first().unwrap().hash != self.ctx.config.genesis.hash {
            return Err(ProtocolError::Other("the first pruning point in the list is expected to be genesis"));
        }

        // Check if past pruning points violate finality of current consensus
        //
        // The second finality gate, and it asks the same question as the first about the same
        // provisional chain: are this peer's pruning points compatible with mine? For two histories
        // that share only genesis the answer is no, and a permitted recovery has to be able to say
        // "that is the point" — otherwise the permit lets the IBD start and this stops it three
        // steps later. Measured: the first permitted attempt died exactly here with "pruning points
        // are violating finality".
        //
        // Without a permit this is untouched. The distinction it draws is real everywhere else.
        if !recovering
            && self.ctx.consensus().session().await.async_are_pruning_points_violating_finality(pruning_points.clone()).await
        {
            // TODO (relaxed): consider performing additional actions on finality conflicts in addition to disconnecting from the peer (e.g., banning, rpc notification)
            return Err(ProtocolError::Other("pruning points are violating finality"));
        }

        {
            // Sanity check for consistency between past pruning points and the headers proof
            let pruning_points_set: BlockHashSet = pruning_points.iter().map(|h| h.hash).collect();
            for level in proof.iter() {
                if let Some(root) = level.first()
                    && root.hash != self.ctx.config.genesis.hash
                    && !pruning_points_set.contains(&root.pruning_point)
                {
                    return Err(ProtocolError::Other("proof and past pruning points are inconsistent with each other"));
                }
            }
        }

        // Trusted data is sent in two stages:
        // The first, TrustedDataPackage, contains meta data about daa_window
        // blocks headers, and ghostdag data, which are required to verify the pruning
        // point and its anticone.
        // The latter, the trusted data entries, each represent a block (with daa) from the anticone of the pruning point
        // (including the PP itself), alongside indexing denoting the respective metadata headers or ghostdag data
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::TrustedData)?;
        let pkg: TrustedDataPackage = Versioned(self.header_format, msg).try_into()?;
        debug!("received trusted data with {} daa entries and {} ghostdag entries", pkg.daa_window.len(), pkg.ghostdag_window.len());

        let mut entry_stream = TrustedEntryStream::new(&self.router, &mut self.incoming_route, self.header_format);
        // The first entry of the trusted data is the pruning point itself.
        let Some(pruning_point_entry) = entry_stream.next().await? else {
            return Err(ProtocolError::Other("got `done` message before receiving the pruning point"));
        };

        if pruning_point_entry.block.hash() != proof_pruning_point {
            return Err(ProtocolError::Other("the proof pruning point is not equal to the expected trusted entry"));
        }

        let mut entries = vec![pruning_point_entry];
        while let Some(entry) = entry_stream.next().await? {
            entries.push(entry);
        }
        // Create a topologically ordered vector of  trusted blocks - the pruning point and its anticone,
        // and their daa windows headers
        let mut trusted_set = pkg.build_trusted_subdag(entries)?;

        if self.ctx.config.enable_sanity_checks {
            let con = self.ctx.consensus().unguarded_session_blocking();
            trusted_set = staging
                .clone()
                .spawn_blocking(move |c| {
                    let ref_proof = proof.clone();
                    c.apply_pruning_proof(proof, &trusted_set)?;
                    c.import_pruning_points(pruning_points)?;

                    info!("Building the proof which was just applied (sanity test)");
                    let built_proof = c.get_pruning_point_proof();
                    let mut mismatch_detected = false;
                    for (i, (ref_level, built_level)) in ref_proof.iter().zip(built_proof.iter()).enumerate() {
                        if ref_level.iter().map(|h| h.hash).collect::<BlockHashSet>()
                            != built_level.iter().map(|h| h.hash).collect::<BlockHashSet>()
                        {
                            mismatch_detected = true;
                            warn!("Locally built proof for level {} does not match the applied one", i);
                        }
                    }
                    if mismatch_detected {
                        info!("Validating the locally built proof (sanity test fallback #2)");
                        // Note: the proof is validated in the context of *current* consensus
                        if let Err(err) = con.validate_pruning_proof(&built_proof, &proof_metadata) {
                            panic!("Locally built proof failed validation: {}", err);
                        }
                        info!("Locally built proof was validated successfully");
                    } else {
                        info!("Proof was locally built successfully");
                    }
                    Result::<_, ProtocolError>::Ok(trusted_set)
                })
                .await?;
        } else {
            trusted_set = staging
                .clone()
                .spawn_blocking(move |c| {
                    c.apply_pruning_proof(proof, &trusted_set)?;
                    c.import_pruning_points(pruning_points)?;
                    Result::<_, ProtocolError>::Ok(trusted_set)
                })
                .await?;
        }

        // TODO (relaxed): add logs to staging commit process

        info!("Starting to process {} trusted blocks", trusted_set.len());
        let mut last_time = Instant::now();
        let mut last_index: usize = 0;
        for (i, tb) in trusted_set.into_iter().enumerate() {
            let now = Instant::now();
            let passed = now.duration_since(last_time);
            if passed > Duration::from_secs(1) {
                info!("Processed {} trusted blocks in the last {:.2}s (total {})", i - last_index, passed.as_secs_f64(), i);
                last_time = now;
                last_index = i;
            }
            // TODO (relaxed): queue and join in batches
            staging.validate_and_insert_trusted_block(tb).virtual_state_task.await?;
        }
        staging.async_clear_body_missing_anticone_set().await;
        info!("Done processing trusted blocks");
        Ok(proof_pruning_point)
    }

    async fn sync_headers(
        &mut self,
        consensus: &ConsensusProxy,
        syncer_virtual_selected_parent: BlockHash,
        highest_known_syncer_chain_hash: BlockHash,
        relay_header: &Arc<Header>,
    ) -> Result<(), ProtocolError> {
        let highest_shared_header_score = consensus.async_get_header(highest_known_syncer_chain_hash).await?.daa_score;
        let mut progress_reporter = ProgressReporter::new(highest_shared_header_score, relay_header.daa_score, "block headers");

        self.router
            .enqueue(make_message!(
                Payload::RequestHeaders,
                RequestHeadersMessage {
                    low_hash: Some(highest_known_syncer_chain_hash.into()),
                    high_hash: Some(syncer_virtual_selected_parent.into())
                }
            ))
            .await?;
        let mut chunk_stream = HeadersChunkStream::new(&self.router, &mut self.incoming_route, self.header_format);

        if let Some(chunk) = chunk_stream.next().await? {
            let (mut prev_daa_score, mut prev_timestamp) = {
                let last_header = chunk.last().expect("chunk is never empty");
                (last_header.daa_score, last_header.timestamp)
            };
            let mut prev_jobs: Vec<BlockValidationFuture> =
                chunk.into_iter().map(|h| consensus.validate_and_insert_block(Block::from_header_arc(h)).virtual_state_task).collect();

            while let Some(chunk) = chunk_stream.next().await? {
                let (current_daa_score, current_timestamp) = {
                    let last_header = chunk.last().expect("chunk is never empty");
                    (last_header.daa_score, last_header.timestamp)
                };
                let current_jobs = chunk
                    .into_iter()
                    .map(|h| consensus.validate_and_insert_block(Block::from_header_arc(h)).virtual_state_task)
                    .collect();
                let prev_chunk_len = prev_jobs.len();
                // Join the previous chunk so that we always concurrently process a chunk and receive another
                try_join_all(prev_jobs).await?;
                // Log the progress
                progress_reporter.report(prev_chunk_len, prev_daa_score, prev_timestamp);
                prev_daa_score = current_daa_score;
                prev_timestamp = current_timestamp;
                prev_jobs = current_jobs;
            }

            let prev_chunk_len = prev_jobs.len();
            try_join_all(prev_jobs).await?;
            progress_reporter.report_completion(prev_chunk_len);
        }

        if consensus.async_get_block_status(syncer_virtual_selected_parent).await.is_none() {
            // If the syncer's claimed sink header has still not been received, the peer is misbehaving
            return Err(ProtocolError::OtherOwned(format!(
                "did not receive syncer's virtual selected parent {} from peer {} during header download",
                syncer_virtual_selected_parent, self.router
            )));
        }

        self.sync_missing_relay_past_headers(consensus, syncer_virtual_selected_parent, relay_header.hash).await?;

        Ok(())
    }

    async fn sync_new_utxo_set(&mut self, consensus: &ConsensusProxy, pruning_point: BlockHash) -> Result<(), ProtocolError> {
        // A better solution could be to create a copy of the old utxo state for some sort of fallback rather than delete it.
        consensus.async_clear_pruning_utxo_set().await; // this deletes the old pruning utxoset and also sets the pruning utxo as invalidated
        self.sync_pruning_point_utxoset(consensus, pruning_point).await?;
        // kaspa-pq ADR-0022: import the pruning point's EVM execution state + DNS/PoS-v2 overlay snapshot
        // as part of the SAME "make the pruning point usable" step — BEFORE marking the utxoset stable.
        // Atomicity matters: async_set_pruning_utxoset_stable() below latches is_utxo_stable=true, and a
        // later IbdType::Sync SKIPS re-import while that flag is true. If a sidecar import failed (the
        // peer can answer not-found) or the node crashed AFTER the utxoset was marked stable but BEFORE
        // the sidecars landed, the node would be permanently missing the EVM/overlay state and would
        // disqualify every post-pruning block with no path to recover. Importing the sidecars first keeps
        // "utxoset + EVM + overlay" all-or-nothing w.r.t. the stable flag (a failure leaves the utxoset
        // unstable, so the next IBD re-runs the whole import). Skipped at genesis: there is no
        // below-genesis state and the peer has captured no snapshot (it would answer not-found and abort).
        if pruning_point != self.ctx.config.genesis.hash {
            self.sync_pruning_point_evm_state(consensus, pruning_point).await?;
            self.sync_pruning_point_overlay_snapshot(consensus, pruning_point).await?;
        }
        // Only if the function has reached here (utxoset + EVM + overlay all imported), is the utxo "final"
        consensus.async_set_pruning_utxoset_stable().await;
        // Once a new utxoset is stored, the utxoindex needs to be resynced as well. This happens through the reset handler mechanism.
        let consensus_manager = self.ctx.consensus_manager.clone();
        spawn_blocking(move || consensus_manager.invoke_consensus_reset_handlers()).await.unwrap();
        self.ctx.on_pruning_point_utxoset_override();
        Ok(())
    }

    async fn sync_missing_relay_past_headers(
        &mut self,
        consensus: &ConsensusProxy,
        syncer_virtual_selected_parent: BlockHash,
        relay_block_hash: BlockHash,
    ) -> Result<(), ProtocolError> {
        // Finished downloading syncer selected tip blocks,
        // check if we already have the triggering relay block
        if consensus.async_get_block_status(relay_block_hash).await.is_some() {
            return Ok(());
        }

        // Send a special header request for the sink antipast. This is expected to
        // be a relatively small set since virtual and relay blocks should be close topologically.
        // See server-side handling of `RequestAnticone` for further details.
        self.router
            .enqueue(make_message!(
                Payload::RequestAntipast,
                RequestAntipastMessage {
                    block_hash: Some(syncer_virtual_selected_parent.into()),
                    context_hash: Some(relay_block_hash.into())
                }
            ))
            .await?;

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::BlockHeaders)?;
        let chunk: HeadersChunk = Versioned(self.header_format, msg).try_into()?;
        let jobs: Vec<BlockValidationFuture> =
            chunk.into_iter().map(|h| consensus.validate_and_insert_block(Block::from_header_arc(h)).virtual_state_task).collect();
        try_join_all(jobs).await?;
        dequeue_with_timeout!(self.incoming_route, Payload::DoneHeaders)?;

        if consensus.async_get_block_status(relay_block_hash).await.is_none() {
            // If the relay block has still not been received, the peer is misbehaving
            Err(ProtocolError::OtherOwned(format!(
                "did not receive relay block {} from peer {} during header download",
                relay_block_hash, self.router
            )))
        } else {
            Ok(())
        }
    }

    async fn validate_staging_timestamps(
        &self,
        consensus: &ConsensusProxy,
        staging_consensus: &ConsensusProxy,
    ) -> Result<(), ProtocolError> {
        // The purpose of this check is to prevent the potential abuse explained here:
        // https://github.com/kaspanet/research/issues/3#issuecomment-895243792
        let staging_hst = staging_consensus.async_get_header(staging_consensus.async_get_headers_selected_tip().await).await.unwrap();
        let current_hst = consensus.async_get_header(consensus.async_get_headers_selected_tip().await).await.unwrap();
        // If staging is behind current or within 10 minutes ahead of it, then something is wrong and we reject the IBD
        if staging_hst.timestamp < current_hst.timestamp || staging_hst.timestamp - current_hst.timestamp < 600_000 {
            Err(ProtocolError::OtherOwned(format!(
                "The difference between the timestamp of the current selected tip ({}) and the 
staging selected tip ({}) is too small or negative. Aborting IBD...",
                current_hst.timestamp, staging_hst.timestamp
            )))
        } else {
            Ok(())
        }
    }

    async fn sync_pruning_point_utxoset(&mut self, consensus: &ConsensusProxy, pruning_point: BlockHash) -> Result<(), ProtocolError> {
        info!("downloading the pruning point utxoset, this can take a little while.");
        self.router
            .enqueue(make_message!(
                Payload::RequestPruningPointUtxoSet,
                RequestPruningPointUtxoSetMessage { pruning_point_hash: Some(pruning_point.into()) }
            ))
            .await?;
        let mut chunk_stream = PruningPointUtxosetChunkStream::new(&self.router, &mut self.incoming_route);
        let mut multiset = MuHash::new();
        while let Some(chunk) = chunk_stream.next().await? {
            multiset = consensus
                .clone()
                .spawn_blocking(move |c| {
                    c.append_imported_pruning_point_utxos(&chunk, &mut multiset);
                    multiset
                })
                .await;
        }
        consensus.clone().spawn_blocking(move |c| c.import_pruning_point_utxo_set(pruning_point, multiset)).await?;
        Ok(())
    }

    /// kaspa-pq ADR-0022: request + import the pruning point's EVM execution state. Required on an
    /// EVM-active network so the first post-pruning block re-executes against the real parent state.
    async fn sync_pruning_point_evm_state(
        &mut self,
        consensus: &ConsensusProxy,
        pruning_point: BlockHash,
    ) -> Result<(), ProtocolError> {
        let evm_active = {
            let pp = pruning_point;
            let pp_daa = consensus.clone().spawn_blocking(move |c| c.get_header(pp)).await.map(|h| h.daa_score).unwrap_or(0);
            self.ctx.config.is_evm_active(pp_daa)
        };
        self.router
            .enqueue(make_message!(
                Payload::RequestPruningPointEvmState,
                RequestPruningPointEvmStateMessage { pruning_point_hash: Some(pruning_point.into()) }
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointEvmState, Duration::from_secs(600))?;
        if !msg.found {
            if evm_active {
                return Err(ProtocolError::Other(
                    "peer cannot serve the pruning point EVM state required for pruned IBD on this network",
                ));
            }
            return Ok(()); // EVM-inactive network — no EVM state to import.
        }
        // Audit H-03: bound the accepted bytes BEFORE deserializing so a malicious
        // IBD peer cannot send a near-1-GiB (P2P decode ceiling) or gzip-bomb-
        // decompressed payload that borsh expands into a huge nested allocation.
        // These are generous ceilings for the real pruning-point state (the current
        // nets are far smaller); a chunked/streamed manifest is the deeper follow-up.
        const MAX_EVM_HEADER_BYTES: usize = 1 << 20; // 1 MiB (the header is tiny)
        const MAX_EVM_STATE_SNAPSHOT_BYTES: usize = 256 << 20; // 256 MiB
        if msg.evm_header.len() > MAX_EVM_HEADER_BYTES {
            return Err(ProtocolError::Other("PruningPointEvmState header exceeds the accepted size cap"));
        }
        if msg.evm_state_snapshot.len() > MAX_EVM_STATE_SNAPSHOT_BYTES {
            return Err(ProtocolError::Other("PruningPointEvmState snapshot exceeds the accepted size cap"));
        }
        let header: kaspa_consensus_core::evm::EvmExecutionHeader = borsh::from_slice(&msg.evm_header)
            .map_err(|_| ProtocolError::Other("invalid EVM execution header in PruningPointEvmState"))?;
        let snapshot: kaspa_consensus_core::evm::EvmStateSnapshot = borsh::from_slice(&msg.evm_state_snapshot)
            .map_err(|_| ProtocolError::Other("invalid EVM state snapshot in PruningPointEvmState"))?;
        consensus.clone().spawn_blocking(move |c| c.import_pruning_point_evm_state(pruning_point, header, snapshot)).await?;
        info!("imported the EVM state of the pruning point {}", pruning_point);
        Ok(())
    }

    /// kaspa-pq ADR-0022: request + import the pruning point's DNS/PoS-v2 overlay snapshot. Required
    /// on an overlay-active network so the first post-pruning block's coinbase `c == v` reproduces.
    async fn sync_pruning_point_overlay_snapshot(
        &mut self,
        consensus: &ConsensusProxy,
        pruning_point: BlockHash,
    ) -> Result<(), ProtocolError> {
        let overlay_active = self.ctx.config.dns_params.is_some();
        self.router
            .enqueue(make_message!(
                Payload::RequestPruningPointOverlaySnapshot,
                RequestPruningPointOverlaySnapshotMessage { pruning_point_hash: Some(pruning_point.into()) }
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointOverlaySnapshot, Duration::from_secs(600))?;
        if !msg.found {
            if overlay_active {
                return Err(ProtocolError::Other(
                    "peer cannot serve the pruning point overlay snapshot required for pruned IBD on this network",
                ));
            }
            return Ok(()); // overlay dormant — nothing to import.
        }
        // Audit H-03: bound the accepted overlay snapshot bytes before deserializing
        // (bonds + reward windows are far smaller than the EVM state).
        const MAX_OVERLAY_SNAPSHOT_BYTES: usize = 64 << 20; // 64 MiB
        if msg.overlay_snapshot.len() > MAX_OVERLAY_SNAPSHOT_BYTES {
            return Err(ProtocolError::Other("PruningPointOverlaySnapshot exceeds the accepted size cap"));
        }
        let snapshot: kaspa_consensus_core::dns_finality::OverlaySnapshot = borsh::from_slice(&msg.overlay_snapshot)
            .map_err(|_| ProtocolError::Other("invalid overlay snapshot in PruningPointOverlaySnapshot"))?;
        consensus.clone().spawn_blocking(move |c| c.import_pruning_point_overlay_snapshot(pruning_point, snapshot)).await?;
        info!("imported the overlay snapshot of the pruning point {}", pruning_point);
        Ok(())
    }

    async fn sync_missing_trusted_bodies(&mut self, consensus: &ConsensusProxy) -> Result<(), ProtocolError> {
        info!("downloading pruning point anticone missing block data");
        let diesembodied_hashes = consensus.async_get_body_missing_anticone().await;
        if self.body_only_ibd_permitted {
            self.sync_missing_trusted_bodies_no_headers(consensus, diesembodied_hashes).await?
        } else {
            self.sync_missing_trusted_bodies_full_blocks(consensus, diesembodied_hashes).await?;
        }
        consensus.async_clear_body_missing_anticone_set().await;
        Ok(())
    }
    async fn sync_missing_trusted_bodies_no_headers(
        &mut self,
        consensus: &ConsensusProxy,
        diesembodied_hashes: Vec<BlockHash>,
    ) -> Result<(), ProtocolError> {
        let iter = diesembodied_hashes.chunks(IBD_BATCH_SIZE);
        for chunk in iter {
            self.router
                .enqueue(make_message!(
                    Payload::RequestBlockBodies,
                    RequestBlockBodiesMessage { hashes: chunk.iter().map(|h| h.into()).collect() }
                ))
                .await?;
            let mut jobs = Vec::with_capacity(chunk.len());

            for &hash in chunk.iter() {
                let msg = dequeue_with_timeout!(self.incoming_route, Payload::BlockBody)?;
                // kaspa-pq EVM Lane v0.4 (§3.1): the body response carries the
                // block's own EVM payload — without it a reassembled v2 block
                // would fail its `evm_payload_hash` body rule (a VALID block
                // rejected). The header (already stored + hash-validated)
                // commits to the payload hash, so a tampered payload is caught
                // by body validation.
                let (blk_body, evm_payload): (BlockBody, kaspa_consensus_core::evm::EvmExecutionPayload) = msg.try_into()?;
                // TODO (relaxed): make header queries in a batch.
                let blk_header = consensus.async_get_header(hash).await.map_err(|err| {
                    // Conceptually this indicates local inconsistency, since we received the expected hashes via a local
                    // get_missing_block_body_hashes call. However for now we fail gracefully and only disconnect from this peer.
                    ProtocolError::OtherOwned(format!("syncee inconsistency: missing block header for {}, err: {}", hash, err))
                })?;
                if blk_body.is_empty() {
                    return Err(ProtocolError::OtherOwned(format!("sent empty block body for block {}", hash)));
                }
                let block = Block { header: blk_header, transactions: blk_body.into(), evm_payload: Arc::new(evm_payload) };
                // TODO (relaxed): sending ghostdag data may be redundant, especially when the headers were already verified.
                // Consider sending empty ghostdag data, simplifying a great deal. The result should be the same -
                // a trusted task is sent, however the header is already verified, and hence only the block body will be verified.
                jobs.push(
                    consensus
                        .validate_and_insert_trusted_block(TrustedBlock::new(block, consensus.async_get_ghostdag_data(hash).await?))
                        .virtual_state_task,
                );
            }
            try_join_all(jobs).await?; // TODO (relaxed): be more efficient with batching as done with block bodies in general
        }
        Ok(())
    }
    async fn sync_missing_trusted_bodies_full_blocks(
        &mut self,
        consensus: &ConsensusProxy,
        diesembodied_hashes: Vec<BlockHash>,
    ) -> Result<(), ProtocolError> {
        let iter = diesembodied_hashes.chunks(IBD_BATCH_SIZE);
        for chunk in iter {
            self.router
                .enqueue(make_message!(
                    Payload::RequestIbdBlocks,
                    RequestIbdBlocksMessage { hashes: chunk.iter().map(|h| h.into()).collect() }
                ))
                .await?;
            let mut jobs = Vec::with_capacity(chunk.len());

            for &hash in chunk.iter() {
                // TODO: change to BodyOnly requests when incorporated
                let msg = dequeue_with_timeout!(self.incoming_route, Payload::IbdBlock)?;
                let block: Block = Versioned(self.header_format, msg).try_into()?;
                if block.hash() != hash {
                    return Err(ProtocolError::OtherOwned(format!("expected block {} but got {}", hash, block.hash())));
                }
                if block.is_header_only() {
                    return Err(ProtocolError::OtherOwned(format!("sent header of {} where expected block with body", block.hash())));
                }
                // TODO (relaxed): sending ghostdag data may be redundant, especially when the headers were already verified.
                // Consider sending empty ghostdag data, simplifying a great deal. The result should be the same -
                // a trusted task is sent, however the header is already verified, and hence only the block body will be verified.
                jobs.push(
                    consensus
                        .validate_and_insert_trusted_block(TrustedBlock::new(block, consensus.async_get_ghostdag_data(hash).await?))
                        .virtual_state_task,
                );
            }
            try_join_all(jobs).await?; // TODO (relaxed): be more efficient with batching as done with block bodies in general
        }
        Ok(())
    }
    async fn sync_missing_block_bodies(&mut self, consensus: &ConsensusProxy, high: BlockHash) -> Result<(), ProtocolError> {
        // TODO (relaxed): query consensus in batches
        let sleep_task = sleep(Duration::from_secs(2));
        let hashes_task = consensus.async_get_missing_block_body_hashes(high);
        tokio::pin!(sleep_task);
        tokio::pin!(hashes_task);
        let hashes = match select(sleep_task, hashes_task).await {
            Either::Left((_, hashes_task)) => {
                // We select between the tasks in order to inform the user if this operation is taking too long. On full IBD
                // this operation requires traversing the full DAG which indeed might take several seconds or even minutes.
                info!(
                    "IBD: searching for missing block bodies to request from peer {}. This operation might take several seconds.",
                    self.router
                );
                // Now re-await the original task
                hashes_task.await
            }
            Either::Right((hashes_result, _)) => hashes_result,
        }?;
        if hashes.is_empty() {
            return Ok(());
        }

        // The total gives the operator the denominator for the "Processed M block bodies (P%)"
        // progress lines below — without it, a percentage that resets to a low value right after
        // the header phase reached 100% reads like a rollback when it is just a new phase. The
        // header phase and this body phase each have their OWN ProgressReporter (each starting at
        // 0% over its own DAA-score window), so a body percent below the header percent is normal.
        info!(
            "IBD: {} missing block bodies to sync from peer {} (header sync is already complete; this is the body phase)",
            hashes.len(),
            self.router
        );

        let low_header = consensus.async_get_header(*hashes.first().expect("hashes was non empty")).await?;
        let high_header = consensus.async_get_header(*hashes.last().expect("hashes was non empty")).await?;
        let mut progress_reporter = ProgressReporter::new(low_header.daa_score, high_header.daa_score, "block bodies");

        let mut iter = hashes.chunks(IBD_BATCH_SIZE);
        let QueueChunkOutput { jobs: mut prev_jobs, daa_score: mut prev_daa_score, timestamp: mut prev_timestamp } =
            self.queue_block_processing_chunk(consensus, iter.next().expect("hashes was non empty")).await?;

        for chunk in iter {
            let QueueChunkOutput { jobs: current_jobs, daa_score: current_daa_score, timestamp: current_timestamp } =
                self.queue_block_processing_chunk(consensus, chunk).await?;
            let prev_chunk_len = prev_jobs.len();
            // Join the previous chunk so that we always concurrently process a chunk and receive another
            try_join_all(prev_jobs).await?;
            // Log the progress
            progress_reporter.report(prev_chunk_len, prev_daa_score, prev_timestamp);
            prev_daa_score = current_daa_score;
            prev_timestamp = current_timestamp;
            prev_jobs = current_jobs;
        }

        let prev_chunk_len = prev_jobs.len();
        try_join_all(prev_jobs).await?;
        progress_reporter.report_completion(prev_chunk_len);

        Ok(())
    }

    async fn queue_block_processing_chunk(
        &mut self,
        consensus: &ConsensusProxy,
        chunk: &[BlockHash],
    ) -> Result<QueueChunkOutput, ProtocolError> {
        if self.body_only_ibd_permitted {
            self.queue_block_processing_chunk_body_only(consensus, chunk).await
        } else {
            self.queue_block_processing_chunk_full_block(consensus, chunk).await
        }
    }

    async fn queue_block_processing_chunk_full_block(
        &mut self,
        consensus: &ConsensusProxy,
        chunk: &[BlockHash],
    ) -> Result<QueueChunkOutput, ProtocolError> {
        let mut jobs = Vec::with_capacity(chunk.len());
        let mut current_daa_score = 0;
        let mut current_timestamp = 0;
        self.router
            .enqueue(make_message!(
                Payload::RequestIbdBlocks,
                RequestIbdBlocksMessage { hashes: chunk.iter().map(|h| h.into()).collect() }
            ))
            .await?;
        for &expected_hash in chunk {
            let msg = dequeue_with_timeout!(self.incoming_route, Payload::IbdBlock)?;
            let block: Block = Versioned(self.header_format, msg).try_into()?;
            if block.hash() != expected_hash {
                return Err(ProtocolError::OtherOwned(format!("expected block {} but got {}", expected_hash, block.hash())));
            }
            if block.is_header_only() {
                return Err(ProtocolError::OtherOwned(format!("sent header of {} where expected block with body", block.hash())));
            }
            current_daa_score = block.header.daa_score;
            current_timestamp = block.header.timestamp;
            jobs.push(consensus.validate_and_insert_block(block).virtual_state_task);
        }
        Ok(QueueChunkOutput { jobs, daa_score: current_daa_score, timestamp: current_timestamp })
    }

    async fn queue_block_processing_chunk_body_only(
        &mut self,
        consensus: &ConsensusProxy,
        chunk: &[BlockHash],
    ) -> Result<QueueChunkOutput, ProtocolError> {
        let mut jobs = Vec::with_capacity(chunk.len());
        let mut current_daa_score = 0;
        let mut current_timestamp = 0;
        self.router
            .enqueue(make_request!(
                Payload::RequestBlockBodies,
                RequestBlockBodiesMessage { hashes: chunk.iter().map(|h| h.into()).collect() },
                self.incoming_route.id()
            ))
            .await?;
        for &expected_hash in chunk {
            let msg = dequeue_with_timeout!(self.incoming_route, Payload::BlockBody)?;
            // TODO (relaxed): make header queries in a batch.
            let blk_header = consensus.async_get_header(expected_hash).await.map_err(|err| {
                // Conceptually this indicates local inconsistency, since we received the expected hashes via a local
                // get_missing_block_body_hashes call. However for now we fail gracefully and only disconnect from this peer.
                ProtocolError::OtherOwned(format!("syncee inconsistency: missing block header for {}, err: {}", expected_hash, err))
            })?;
            // kaspa-pq EVM Lane v0.4 (§3.1): the body response carries the block's
            // own EVM payload (see sync_missing_trusted_bodies_no_headers).
            let (blk_body, evm_payload): (BlockBody, kaspa_consensus_core::evm::EvmExecutionPayload) = msg.try_into()?;
            if blk_body.is_empty() {
                return Err(ProtocolError::OtherOwned(format!("sent empty block body for block {}", expected_hash)));
            }
            let block = Block { header: blk_header, transactions: blk_body.into(), evm_payload: Arc::new(evm_payload) };
            current_daa_score = block.header.daa_score;
            current_timestamp = block.header.timestamp;
            jobs.push(consensus.validate_and_insert_block(block).virtual_state_task);
        }
        Ok(QueueChunkOutput { jobs, daa_score: current_daa_score, timestamp: current_timestamp })
    }
}
