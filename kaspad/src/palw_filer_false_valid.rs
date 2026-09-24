//! **ADR-0152 v3.1 N10 / §7.3 P2-8c: the automatic `PanelFalseValidV2` filer** (C-3; T54d).
//!
//! # What it does
//!
//! When this node's own work proves a claim false — the free-prompt capture arm's sample of a leaf
//! that does not recompute (ADR-0098 Decision 2), the refutation its court close filed as the
//! challenger with the executor found guilty, and (at integration) P2-8b's replay-mismatch
//! contradiction — the proof is noted here ([`PalwFalseValidFilerV1::note_proof_v1`]). From then on,
//! off the panel tick and a bounded number of claims a tick, the filer:
//!
//! 1. **reads the licence off the chain, keeping only what the chain relied on** — every accepted
//!    `ReceiptLicensed`, `ReceiptLicensedV2` and `OptimisticLicensed` naming the claim, walked back
//!    from the sink to the claim's bind (or a bounded look-back) and then incrementally, so a
//!    supplementary receipt is seen too. A licence object rides on stateless admission and the gate
//!    can drop it while its carrier stays accepted, so what the walk reads is not what the chain
//!    licensed on: each `Valid` receipt (in the form it was signed in — `Full` for a V2 receipt,
//!    `Segmented` with its mask for a V3 one: C-3) is admitted only when the consensus read
//!    `palw_false_valid_relied_receipts_v1` says the chain relied on it — its signature under this
//!    chain's domain and the seat's registered key, and a lock over exactly its mask, a liability
//!    row or a standing conviction of the seat — at most `PALW_FALSE_VALID_RECEIPTS_PER_SEAT_V1` a
//!    seat. The receipts held are re-admitted at every scan, so a licence a reorg took away leaves;
//! 2. **asks the chain, per receipt, what a filing comes to** (`palw_false_valid_filings_v1` over the
//!    consensus read `palw_false_valid_filing_check_v1`): the ONE adjudicator
//!    (`palw_check_panel_false_valid_v2`), the processor's gate with the receipt's signature under
//!    the V2 or V3 message, and the fold itself on the tip. So the liability rule is the audit's —
//!    a located fault names exactly the signers it makes liable, never a partial seat outside its
//!    segment — the conviction is S-4's funnel, pre-`Final` a void and post-`Final` the reversal and
//!    the row's burn, whichever the fold's target resolves to, and this node never pays a carrier
//!    for an object the fold refuses;
//! 3. **files each conviction once** through ONE seam, [`false_valid_file_seam_v1`], which queues it
//!    on the panel's court queue under a stable per-(seat, claim) key and records what P2-8's
//!    reporter filer needs (the offence key, the `evidence_id`, the accused).
//!
//! # What settles a seat, and what does not
//!
//! A seat is done with for the proof's life only when nothing the chain later does can change the
//! answer: node policy declined it (this node's own claim), a conviction of it stands past the
//! finality depth, or [`PALW_FALSE_VALID_MAX_SENDS_V1`] carriers did not land. Everything else is
//! asked again (the P2-8c review's high finding: a first answer that settled a seat let one junk
//! carrier shield it): a receipt the liability rule does not reach is recorded BY RECEIPT, so a
//! new receipt of the seat is asked afresh; a refusal, an unrelied receipt, a dormant read or an
//! open court are asked again at the next walk; and a conviction seen is watched until it is final,
//! so a reorg that takes it away is filed again with a fresh send budget.
//!
//! # What it never does
//!
//! * **File against this node's own bond** — its own receipt is never built into evidence, and a
//!   claim this node produced is never filed on (a conviction that acts on the claim charges its
//!   producer S2/S3).
//! * **Name a signer the audit's liability rule does not reach** — on the floor and on a held class
//!   alike (`PALW_FALSE_VALID_HELD_NAMES_PARTIALS_V1`: the operator's held-attention decision binds
//!   no partial seat through a dissection or a DA default, and the adjudicator already refuses
//!   both; a located step fault inside a partial seat's own segment is what it signed).
//! * **File twice for one offence.** One queue entry per (seat, claim) key; a filing sent is not asked
//!   about again for [`PALW_FALSE_VALID_REFILE_DAA_V1`], after which the CHAIN decides — convicted
//!   (watched), or still convictable (the carrier was lost: sent again, at most
//!   [`PALW_FALSE_VALID_MAX_SENDS_V1`] times). The ledger's key is the consensus half of the same
//!   rule: a second object for one (seat, claim) folds as a no-op.
//! * **Replay, bisect or re-make a capture.** The proof arrives built (the capture arm and the court
//!   reserved their replay bytes through `reserve_replay_v1` when they ran); what runs here is a
//!   chain walk, a signature check per receipt admitted and the adjudicator's own check of one step,
//!   so it takes no memory-ledger reservation — and it runs on a blocking thread, not on the tick.
//!
//! # What a restart loses
//!
//! The book lives in the panel loop's memory, as P2-6's accusation book does: a restart forgets the
//! proofs held, the sends and the seam's records, and `court_moved` with them. A capture sample or
//! court close noted before the restart is not re-noted (its duty is gone), so its filings that had
//! not landed are not made; and a filing whose carrier was still in the mempool can be queued again
//! from a proof noted after the restart — the gate drops the second copy once the first lands, so
//! the cost is one carrier fee. Persisting the book (or re-deriving proofs from the chain's accepted
//! `CourtClosed` objects) is a later change; no conviction is ever wrong because of it.
//!
//! # The seam (P2-8)
//!
//! P2-8's commit–reveal reporter filer is written in parallel. Until it lands, the seam queues the
//! object directly — a conviction without a commitment still lands; only its reporter reward goes
//! unclaimed (R-3). At integration the seam routes the same record through `PalwConvictionFilingV1`:
//! commit over the offence key and the `evidence_id`, wait for acceptance, file, reveal.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use kaspa_consensus_core::palw_false_valid_filing_v1::{
    PalwFalseValidFilingCheckV1, PalwFalseValidFilingV1, PalwFalseValidProofV1, palw_false_valid_admit_receipts_v1,
    palw_false_valid_filings_v1,
};
use kaspa_consensus_core::palw_offence_attribution_v1::{PalwFalseValidReceiptV1, palw_false_valid_receipts_of_licence_v1};
use kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_core::{debug, info, warn};
use kaspa_hashes::Hash64;

const PALW_PANEL: &str = "palw-panel";

/// The panel's court queue: `(key, round, responder, object)`, drained by `carry_priority_v1` on the
/// priority lane of P2-6's one scheduler.
pub(crate) type PalwCourtQueueV1 = Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>;

/// **The round a P2-8c filing is queued under** — a value no court move (small rounds), no
/// data-availability accusation (`u32::MAX`, P2-6) and no answer uses. With the offence key as the
/// entry's id the key is unique on its own; the round marks whose entry it is.
pub(crate) const PALW_FALSE_VALID_QUEUE_ROUND_V1: u32 = u32::MAX - 1;

/// The most proofs held at once. A proof is one contradiction (at most one carrier's bytes,
/// `PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES`), so the book stays under ~8 MB; the oldest leaves first.
pub(crate) const PALW_FALSE_VALID_MAX_CASES_V1: usize = 64;

/// Claims scanned a tick: each scan is a chain walk, a signature check per receipt admitted and one
/// adjudication per signer, on a blocking thread — one a tick keeps a burst of faults from taking
/// the panel's blocking pool.
pub(crate) const PALW_FALSE_VALID_CASES_PER_TICK_V1: usize = 1;

/// How often a proof's claim is walked again for receipts (a supplementary licence, a licence that
/// landed late) and every seat not settled asked again.
pub(crate) const PALW_FALSE_VALID_REWALK_DAA_V1: u64 = 100;

/// How soon a signer behind an open court is asked again.
pub(crate) const PALW_FALSE_VALID_WAIT_RECHECK_DAA_V1: u64 = 25;

/// **How long a queued or sent filing is left alone before the chain is asked about it again** — six
/// of the court queue's re-plans (`COURT_MOVE_REPLAN_DAA` = 10): long enough that a carrier still in
/// the mempool is not doubled, short enough that a lost one is sent again inside the claim's life.
pub(crate) const PALW_FALSE_VALID_REFILE_DAA_V1: u64 = 60;

/// A filing the chain still does not hold after this many carriers is given up (logged): a carrier
/// that keeps vanishing is a node problem no fourth fee fixes.
pub(crate) const PALW_FALSE_VALID_MAX_SENDS_V1: u32 = 3;

/// How far below the last walk's DAA the next walk starts, so a reorg that replaced the last blocks
/// walked is walked again.
pub(crate) const PALW_FALSE_VALID_REORG_MARGIN_DAA_V1: u64 = 64;

/// The most chain blocks one walk reads (the court's own walk bound, `attn_root_filings_from_chain_v1`).
pub(crate) const PALW_FALSE_VALID_MAX_WALK_BLOCKS_V1: u64 = 1 << 16;

/// **The court queue's key of the filing against `offence_id`** — one per (seat, claim), stable across
/// ticks (`court_moved` keys the carrier's send by it).
pub(crate) fn palw_false_valid_queue_key_v1(offence_id: Hash64) -> (Hash64, u32, bool) {
    (offence_id, PALW_FALSE_VALID_QUEUE_ROUND_V1, false)
}

/// Whether a court-queue entry is one of this filer's ([`palw_false_valid_queue_key_v1`]).
pub(crate) fn palw_false_valid_queued_v1(round: u32, responder: bool, object: &PalwConsensusObjectV2) -> bool {
    (round, responder) == (PALW_FALSE_VALID_QUEUE_ROUND_V1, false)
        && matches!(object, PalwConsensusObjectV2::ObjectiveOffence { kind: PalwOffenceKindV1::PanelFalseValidV2, .. })
}

/// **Whether the filer runs at `daa`**: past `palw_rcore_plus` (everything Phase 2 adds lives past it;
/// the fence requires `palw_offence_attribution`, so kind 3 exists), and on a node that carries
/// (`--palw-fee-outpoint`) — a receipts-only node's book stays empty.
pub(crate) fn palw_false_valid_armed_v1(params: &kaspa_consensus_core::config::params::Params, carries: bool, daa: u64) -> bool {
    carries && params.palw_rcore_plus_active_at(daa)
}

/// Where a proof came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwFalseValidSourceV1 {
    /// The free-prompt capture arm sampled this leaf of the claim's own capture and it did not
    /// recompute (ADR-0098 Decision 2): the refutation it ran, with the rows it proved.
    CaptureSample { leaf: u64 },
    /// This node closed a court on the claim as its challenger, and the chain read the close as
    /// `ExecutorGuilty`: the close's refutation of this leaf, which this node's own replay built.
    CourtClose { leaf: u64 },
}

/// **What the seam hands P2-8's reporter filer** — and the book keeps, per filing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PalwFalseValidSeamRecordV1 {
    /// `palw_false_valid_offence_id_v2(accused, claim)`: the ledger key the conviction is written
    /// under and a reporter commitment is keyed to (R-3).
    pub offence_key: Hash64,
    /// The `ObjectiveOffence`'s digest of its evidence bytes — what the commitment binds (N12).
    pub evidence_id: Hash64,
    pub accused: PalwBondKeyV2,
    pub claim_id: Hash64,
    pub queued_daa: u64,
}

/// **P2-8 seam: routed through the reporter filer (PalwConvictionFilingV1) at integration.**
///
/// The ONE place a P2-8c conviction leaves this module: `filing`'s object is pushed onto the panel's
/// court queue (the priority lane, `carry_priority_v1`) under its stable per-(seat, claim) key —
/// once: an entry already queued under the key is left as it is — and the record returned is what
/// the reporter filer will need (the offence key, the evidence id, the accused). `None` when the
/// filing does not file (the chain or node policy declined it) or is queued already.
///
/// Until integration the object goes out with no commitment before it, so no reporter reward is
/// claimed for it (R-3 pays only a commitment made strictly before the conviction's block,
/// `apply_reporter_revealed`). A watcher that copies the evidence out of the mempool can commit
/// only in a block after it saw the carrier, so it wins the reward only if the carrier then waits
/// at least one more block — the conviction itself lands either way.
pub(crate) fn false_valid_file_seam_v1(
    court_pending: &mut PalwCourtQueueV1,
    filing: &PalwFalseValidFilingV1,
    current_daa: u64,
) -> Option<PalwFalseValidSeamRecordV1> {
    if !filing.files() {
        return None;
    }
    let key = palw_false_valid_queue_key_v1(filing.offence_id);
    if court_pending.iter().any(|(id, round, responder, _)| (*id, *round, *responder) == key) {
        return None;
    }
    court_pending.push((key.0, key.1, key.2, filing.object.clone()));
    Some(PalwFalseValidSeamRecordV1 {
        offence_key: filing.offence_id,
        evidence_id: filing.evidence_id,
        accused: filing.accused,
        claim_id: filing.claim_id,
        queued_daa: current_daa,
    })
}

/// One claim this node holds a proof against.
#[derive(Clone, Debug)]
struct PalwFalseValidCaseV1 {
    proof: Arc<PalwFalseValidProofV1>,
    source: PalwFalseValidSourceV1,
    /// The lowest DAA a licence of the claim can sit at: the claim's bind, or a bounded look-back.
    not_before_daa: u64,
    expires_daa: u64,
    /// The relied-on receipts the last scan kept (re-admitted at every scan).
    receipts: Vec<PalwFalseValidReceiptV1>,
    /// The DAA the last walk ran at (`None`: never walked).
    walked_daa: Option<u64>,
    next_daa: u64,
    /// Seats done with for the proof's life: node policy declined, convicted past the finality
    /// depth, or given up after `MAX_SENDS` carriers.
    settled: BTreeSet<PalwBondKeyV2>,
    /// The `evidence_id`s of receipts the liability rule does not reach — per receipt, never per
    /// seat, so a new receipt of the seat is asked afresh.
    not_liable: BTreeSet<Hash64>,
    /// Convictions seen and not yet final: the seat and the DAA of the conviction's block.
    convicted: BTreeMap<PalwBondKeyV2, u64>,
    /// Signers whose filing this node queued: the DAA it was last queued and how many times.
    sent: BTreeMap<PalwBondKeyV2, (u64, u32)>,
}

/// **One scan's input**, handed to a blocking thread.
#[derive(Clone, Debug)]
pub(crate) struct PalwFalseValidScanV1 {
    pub claim_id: Hash64,
    pub proof: Arc<PalwFalseValidProofV1>,
    /// The receipts held, re-admitted before the walk.
    pub receipts: Vec<PalwFalseValidReceiptV1>,
    /// Walk the chain down to this DAA for licence receipts.
    pub walk_from_daa: u64,
    /// Signers not to ask about: settled, or queued/sent less than a re-file ago.
    pub skip: BTreeSet<PalwBondKeyV2>,
    /// Receipts not to ask about again: answered `NotLiable`.
    pub not_liable: BTreeSet<Hash64>,
}

/// **One scan's result**: the relied-on receipts kept and each asked signer's filing.
#[derive(Clone, Debug)]
pub(crate) struct PalwFalseValidScanOutcomeV1 {
    pub claim_id: Hash64,
    pub receipts: Vec<PalwFalseValidReceiptV1>,
    pub filings: Vec<PalwFalseValidFilingV1>,
    /// Whether the chain could be read at all (a tip state past the fence). A scan that could not
    /// leaves the book as it was: the receipts held stay, the walk mark does not move.
    pub walked: bool,
}

/// **The claims this node holds a proof against, and what it filed** (P2-8c). Bounded
/// ([`PALW_FALSE_VALID_MAX_CASES_V1`]); a proof lives until its TTL — long enough for the licence to
/// land and the claim's liability horizon to pass — and is forgotten after.
#[derive(Clone, Debug)]
pub(crate) struct PalwFalseValidFilerV1 {
    cases: BTreeMap<Hash64, PalwFalseValidCaseV1>,
    /// The seam's records, by offence key, pruned with their cases.
    records: BTreeMap<Hash64, PalwFalseValidSeamRecordV1>,
    ttl_daa: u64,
    lookback_daa: u64,
    /// How deep a conviction's block must be before the seat is settled (the ruleset's finality
    /// depth): above it a reorg can still take the conviction away.
    conviction_depth_daa: u64,
}

impl PalwFalseValidFilerV1 {
    pub(crate) fn new(ttl_daa: u64, lookback_daa: u64, conviction_depth_daa: u64) -> Self {
        Self { cases: BTreeMap::new(), records: BTreeMap::new(), ttl_daa, lookback_daa, conviction_depth_daa }
    }

    /// **Sized from the ruleset's windows.** A licence precedes a court's close by at most the
    /// receipt and challenge windows and a court's life, so the look-back is those (the court's four
    /// times over, its rungs); a proof then lives one more court window — the evidence horizon a
    /// `Final`'s locks and liability row keep (`palw_panel_liability_expiry_v1`) — past it. A
    /// conviction is final at the ruleset's finality depth (half the challenge window on a V2
    /// network, `with_palw_v2_depths`).
    pub(crate) fn for_params(params: &kaspa_consensus_core::config::params::Params) -> Self {
        let (receipt, challenge, court) = match &params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                (bundle.state.window_receipt(), bundle.state.window_challenge(), bundle.state.window_court())
            }
            _ => (0, 0, 0),
        };
        let lookback = receipt.saturating_add(challenge).saturating_add(court.saturating_mul(4));
        Self::new(lookback.saturating_add(court).max(PALW_FALSE_VALID_REWALK_DAA_V1), lookback, params.finality_depth())
    }

    /// Forget everything (the fence is down, or this node carries nothing).
    pub(crate) fn clear(&mut self) {
        self.cases.clear();
        self.records.clear();
    }

    pub(crate) fn holds(&self, claim: &Hash64) -> bool {
        self.cases.contains_key(claim)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.cases.is_empty()
    }

    /// The seam's records — what P2-8's reporter filer reads at integration (until then only the
    /// tests read them; the tick prunes `court_moved` by them).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn records(&self) -> impl Iterator<Item = &PalwFalseValidSeamRecordV1> {
        self.records.values()
    }

    /// **Note a proof against `proof.claim_id`** — the first proof of a claim is kept (one is enough:
    /// the adjudicator decides per signer, and every signer is asked against it). `bound_daa` is the
    /// claim's bind where the caller knows it, the lowest DAA its licence can sit at; otherwise the
    /// look-back from `current_daa`. Returns whether it was kept.
    pub(crate) fn note_proof_v1(
        &mut self,
        proof: PalwFalseValidProofV1,
        source: PalwFalseValidSourceV1,
        bound_daa: Option<u64>,
        current_daa: u64,
    ) -> bool {
        let claim_id = proof.claim_id;
        if self.cases.contains_key(&claim_id) {
            return false;
        }
        if self.cases.len() >= PALW_FALSE_VALID_MAX_CASES_V1
            && let Some(oldest) = self.cases.iter().min_by_key(|(_, case)| case.expires_daa).map(|(claim, _)| *claim)
        {
            warn!("[{PALW_PANEL}] false-Valid filer: {PALW_FALSE_VALID_MAX_CASES_V1} proofs held — forgetting claim {oldest}'s");
            self.forget(&oldest);
        }
        let not_before_daa = bound_daa.unwrap_or_else(|| current_daa.saturating_sub(self.lookback_daa));
        self.cases.insert(
            claim_id,
            PalwFalseValidCaseV1 {
                proof: Arc::new(proof),
                source,
                not_before_daa,
                expires_daa: current_daa.saturating_add(self.ttl_daa),
                receipts: Vec::new(),
                walked_daa: None,
                next_daa: current_daa,
                settled: BTreeSet::new(),
                not_liable: BTreeSet::new(),
                convicted: BTreeMap::new(),
                sent: BTreeMap::new(),
            },
        );
        true
    }

    /// **A step refutation this node built, as a proof** (`PalwFalseValidProofV1::step_arithmetic_v1`,
    /// in the carriage the caller already built — the capture sampler's
    /// `palw_refutation_prompt_carriage_v1`). The panel's call sites are one line each: nothing is
    /// cloned unless the filer is `armed` ([`palw_false_valid_armed_v1`]) and holds no proof of the
    /// claim yet.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn note_step_refutation_v1(
        &mut self,
        armed: bool,
        claim_id: Hash64,
        refutation: &kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1,
        operand_openings: &[kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1],
        prompt_ids_opening: Option<&kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsOpeningV1>,
        source: PalwFalseValidSourceV1,
        bound_daa: Option<u64>,
        current_daa: u64,
    ) -> bool {
        if !armed || self.holds(&claim_id) {
            return false;
        }
        let proof = PalwFalseValidProofV1::step_arithmetic_v1(claim_id, refutation, operand_openings, prompt_ids_opening);
        self.note_proof_v1(proof, source, bound_daa, current_daa)
    }

    /// **A court close this node sent as the challenger, read `ExecutorGuilty` by the chain** — its
    /// arithmetic refutation of `leaf` is the proof (`PalwFalseValidProofV1::of_court_close_v1`,
    /// either prompt carriage); every other close form proves nothing a kind-3 contradiction carries
    /// and is passed over.
    pub(crate) fn note_court_close_v1(
        &mut self,
        armed: bool,
        claim_id: Hash64,
        proof: &kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2,
        leaf: u64,
        current_daa: u64,
    ) -> bool {
        if !armed || self.holds(&claim_id) {
            return false;
        }
        let Some(proof) = PalwFalseValidProofV1::of_court_close_v1(claim_id, proof) else { return false };
        self.note_proof_v1(proof, PalwFalseValidSourceV1::CourtClose { leaf }, None, current_daa)
    }

    fn forget(&mut self, claim: &Hash64) {
        self.cases.remove(claim);
        self.records.retain(|_, record| record.claim_id != *claim);
    }

    /// **The scans due at `current_daa`** — at most [`PALW_FALSE_VALID_CASES_PER_TICK_V1`], earliest
    /// first; expired proofs are forgotten here. A signer is skipped while it is settled, or while
    /// its filing was queued or sent less than [`PALW_FALSE_VALID_REFILE_DAA_V1`] ago (`sent_at`: the
    /// court queue's `court_moved` DAA of its key), so a carrier in flight is never doubled.
    pub(crate) fn due_v1(&mut self, current_daa: u64, sent_at: impl Fn(&Hash64) -> Option<u64>) -> Vec<PalwFalseValidScanV1> {
        let expired: Vec<Hash64> =
            self.cases.iter().filter(|(_, case)| current_daa > case.expires_daa).map(|(claim, _)| *claim).collect();
        for claim in expired {
            self.forget(&claim);
        }
        let mut due: Vec<(&Hash64, &PalwFalseValidCaseV1)> =
            self.cases.iter().filter(|(_, case)| case.next_daa <= current_daa).collect();
        due.sort_by_key(|(claim, case)| (case.next_daa, **claim));
        due.into_iter()
            .take(PALW_FALSE_VALID_CASES_PER_TICK_V1)
            .map(|(claim, case)| {
                let fresh = |at: u64| current_daa < at.saturating_add(PALW_FALSE_VALID_REFILE_DAA_V1);
                let mut skip = case.settled.clone();
                for (seat, (queued, _)) in &case.sent {
                    let key = kaspa_consensus_core::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(&seat.0, claim);
                    if fresh(*queued) || sent_at(&key).is_some_and(fresh) {
                        skip.insert(*seat);
                    }
                }
                PalwFalseValidScanV1 {
                    claim_id: *claim,
                    proof: case.proof.clone(),
                    receipts: case.receipts.clone(),
                    walk_from_daa: case.walked_daa.map_or(case.not_before_daa, |at| {
                        at.saturating_sub(PALW_FALSE_VALID_REORG_MARGIN_DAA_V1).max(case.not_before_daa)
                    }),
                    skip,
                    not_liable: case.not_liable.clone(),
                }
            })
            .collect()
    }

    /// **Take a scan's result**: keep the receipts, file what files through the seam, record what
    /// cannot change for that receipt or seat, and schedule the claim's next scan. What settles and
    /// what is asked again is the module's "What settles a seat" rule. A queued filing whose signer
    /// the chain now answers otherwise (convicted by another carrier, refused, behind a court that
    /// opened) leaves the queue unsent. Returns the seam's records of what was queued now.
    pub(crate) fn apply_v1(
        &mut self,
        outcome: PalwFalseValidScanOutcomeV1,
        current_daa: u64,
        court_pending: &mut PalwCourtQueueV1,
    ) -> Vec<PalwFalseValidSeamRecordV1> {
        let depth = self.conviction_depth_daa;
        let Some(case) = self.cases.get_mut(&outcome.claim_id) else { return Vec::new() };
        if !outcome.walked {
            case.next_daa = current_daa.saturating_add(PALW_FALSE_VALID_WAIT_RECHECK_DAA_V1);
            return Vec::new();
        }
        case.receipts = outcome.receipts;
        case.walked_daa = Some(current_daa);
        let mut queued_now = Vec::new();
        let (mut filed, mut waiting) = (false, false);
        for filing in &outcome.filings {
            let seat = filing.accused;
            for id in &filing.not_liable {
                if case.not_liable.insert(*id) {
                    info!(
                        "[{PALW_PANEL}] claim {}: {seat:?}'s receipt (evidence {id}) is not one the liability rule reaches — not \
                         named on it (P2-8c)",
                        filing.claim_id
                    );
                }
            }
            let key = palw_false_valid_queue_key_v1(filing.offence_id);
            let in_queue = court_pending.iter().any(|(id, round, responder, _)| (*id, *round, *responder) == key);
            if filing.files() {
                filed = true;
                if let Some(at) = case.convicted.remove(&seat) {
                    warn!(
                        "[{PALW_PANEL}] claim {}: {seat:?}'s conviction at DAA {at} is no longer on the chain (a reorg) — filing \
                         again (P2-8c)",
                        filing.claim_id
                    );
                    case.sent.remove(&seat);
                }
                if in_queue {
                    continue;
                }
                let sends = case.sent.get(&seat).map_or(0, |(_, n)| *n);
                if sends >= PALW_FALSE_VALID_MAX_SENDS_V1 {
                    warn!(
                        "[{PALW_PANEL}] claim {}: the false-Valid filing against {seat:?} is still convictable after {sends} \
                         carriers — given up (P2-8c)",
                        filing.claim_id
                    );
                    case.settled.insert(seat);
                    continue;
                }
                if let Some(record) = false_valid_file_seam_v1(court_pending, filing, current_daa) {
                    info!(
                        "[{PALW_PANEL}] claim {}: filing PanelFalseValidV2 against the Valid signer {seat:?} ({:?} proof; offence {}, \
                         evidence {}) — ADR-0152 N10, P2-8c",
                        filing.claim_id, case.source, record.offence_key, record.evidence_id
                    );
                    case.sent.insert(seat, (current_daa, sends + 1));
                    self.records.insert(record.offence_key, record.clone());
                    queued_now.push(record);
                }
                continue;
            }
            // Not (or no longer) a filing: whatever of it is still queued leaves the queue unsent.
            if in_queue {
                court_pending.retain(|(id, round, responder, _)| (*id, *round, *responder) != key);
            }
            match &filing.check {
                PalwFalseValidFilingCheckV1::ConvictedBefore { accepted_daa, .. } => {
                    // The carrier landed (ours or anyone's): nothing is in flight.
                    case.sent.remove(&seat);
                    if current_daa >= accepted_daa.saturating_add(depth) {
                        case.convicted.remove(&seat);
                        if case.settled.insert(seat) {
                            info!(
                                "[{PALW_PANEL}] claim {}: {seat:?}'s false Valid is convicted on chain at DAA {accepted_daa}, past \
                                 the finality depth (P2-8c)",
                                filing.claim_id
                            );
                        }
                    } else if case.convicted.insert(seat, *accepted_daa).is_none() {
                        info!(
                            "[{PALW_PANEL}] claim {}: {seat:?}'s false Valid is convicted on chain at DAA {accepted_daa} — watched \
                             until final (P2-8c)",
                            filing.claim_id
                        );
                    }
                }
                PalwFalseValidFilingCheckV1::Wait(_) | PalwFalseValidFilingCheckV1::Dormant => waiting = true,
                _ if filing.declined() => {
                    if case.settled.insert(seat) {
                        info!(
                            "[{PALW_PANEL}] claim {}: not filing against {seat:?} — node policy {:?} (P2-8c)",
                            filing.claim_id, filing.policy
                        );
                    }
                }
                // Not liable (recorded per receipt above), unrelied or refused: nothing about the
                // seat is settled — its next receipt, or the next block, is asked at the next walk.
                check => debug!("[{PALW_PANEL}] claim {}: not filing against {seat:?} now — {check:?} (P2-8c)", filing.claim_id),
            }
        }
        case.next_daa = current_daa.saturating_add(if filed {
            PALW_FALSE_VALID_REFILE_DAA_V1
        } else if waiting {
            PALW_FALSE_VALID_WAIT_RECHECK_DAA_V1
        } else {
            PALW_FALSE_VALID_REWALK_DAA_V1
        });
        queued_now
    }
}

/// **One scan, on a blocking thread**: the receipts held re-admitted, then the claim's licences read
/// off the accepted chain down to `scan.walk_from_daa`, each receipt admitted only as the chain
/// relied on it (`palw_false_valid_admit_receipts_v1` over the consensus read
/// `palw_false_valid_relied_receipts_v1`, per seat), then every unskipped seat's receipts asked of
/// the chain (`palw_false_valid_filing_check_v1`: ledger key, admission, adjudicator, gate, fold)
/// through `palw_false_valid_filings_v1`. A read that finds no tip state past the fence reports the
/// scan not `walked`, which changes nothing in the book.
pub(crate) fn palw_false_valid_scan_v1(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    scan: PalwFalseValidScanV1,
    own: PalwBondKeyV2,
    current_daa: u64,
) -> PalwFalseValidScanOutcomeV1 {
    let claim_id = scan.claim_id;
    // No tip state past the fence: the book stays as it was (an empty ask costs one cached tip load).
    if consensus.palw_false_valid_relied_receipts_v1(Vec::new()).is_none() {
        return PalwFalseValidScanOutcomeV1 { claim_id, receipts: scan.receipts, filings: Vec::new(), walked: false };
    }
    let relied = |batch: &[PalwFalseValidReceiptV1]| consensus.palw_false_valid_relied_receipts_v1(batch.to_vec()).unwrap_or_default();
    // The receipts held first: a reorg that took a licence's lock away takes its receipts too.
    let mut receipts = Vec::new();
    palw_false_valid_admit_receipts_v1(&mut receipts, claim_id, scan.receipts, relied);
    let span = current_daa.saturating_sub(scan.walk_from_daa).saturating_add(64).min(PALW_FALSE_VALID_MAX_WALK_BLOCKS_V1) as usize;
    crate::palw_panel::walk_accepted_lifecycle_objects_v1(consensus, scan.walk_from_daa, span, &mut |object| {
        if let Some((claim, found)) = palw_false_valid_receipts_of_licence_v1(&object)
            && claim == claim_id
        {
            palw_false_valid_admit_receipts_v1(&mut receipts, claim_id, found, relied);
        }
    });
    let filings = palw_false_valid_filings_v1(
        &scan.proof,
        &receipts,
        &own,
        |seat| scan.skip.contains(seat),
        |evidence| scan.not_liable.contains(evidence),
        |object| consensus.palw_false_valid_filing_check_v1(object.clone()).unwrap_or(PalwFalseValidFilingCheckV1::Dormant),
    );
    PalwFalseValidScanOutcomeV1 { claim_id, receipts, filings, walked: true }
}

/// **The tick's step** (called by the panel once a tick, past the fence, on a node that carries): the
/// due scans run off the tick, and their filings go through the seam onto `court_pending`, which the
/// priority lane drains. `court_moved` is the queue's record of what was sent when; this filer's
/// entries leave it with their records (nothing else prunes it, as P2-6's accusations found).
pub(crate) async fn palw_false_valid_tick_v1(
    filer: &mut PalwFalseValidFilerV1,
    session: &kaspa_consensusmanager::ConsensusProxy,
    own: PalwBondKeyV2,
    current_daa: u64,
    court_pending: &mut PalwCourtQueueV1,
    court_moved: &mut HashMap<(Hash64, u32, bool), u64>,
) {
    if !filer.is_empty() {
        let scans = filer.due_v1(current_daa, |offence| court_moved.get(&palw_false_valid_queue_key_v1(*offence)).copied());
        for scan in scans {
            let outcome = session.clone().spawn_blocking(move |c| palw_false_valid_scan_v1(c, scan, own, current_daa)).await;
            filer.apply_v1(outcome, current_daa, court_pending);
        }
    }
    court_moved.retain(|key, _| key.1 != PALW_FALSE_VALID_QUEUE_ROUND_V1 || key.2 || filer.records.contains_key(&key.0));
}

/// The offence keys of this filer's entries in `court_pending`.
#[cfg(test)]
pub(crate) fn palw_false_valid_queued_keys_v1(court_pending: &PalwCourtQueueV1) -> std::collections::HashSet<Hash64> {
    court_pending
        .iter()
        .filter(|(_, round, responder, object)| palw_false_valid_queued_v1(*round, *responder, object))
        .map(|(id, _, _, _)| *id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_false_valid_filing_v1::PalwFalseValidPolicyV1;
    use kaspa_consensus_core::palw_offence_attribution_v1::{
        PalwFaultSiteV1, PalwPanelFalseValidEvidenceV2, palw_false_valid_offence_id_v2,
    };
    use kaspa_consensus_core::palw_offence_v1::{PalwOffenceVerifyError, PalwPanelContradictionV1};
    use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3};
    use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    const CLAIM: u64 = 0xC1A1;
    const DEPTH: u64 = 600;

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xB0_0000 + n), index: 0 })
    }

    fn claim() -> Hash64 {
        Hash64::from_u64_word(CLAIM)
    }

    fn proof() -> PalwFalseValidProofV1 {
        PalwFalseValidProofV1 {
            claim_id: claim(),
            contradiction: PalwPanelContradictionV1::CourtFraud { voided_daa: 9 },
            prompt_ids_opening: None,
        }
    }

    fn receipt_at(seat: u64, mask: u32, signed_daa: u64) -> PalwFalseValidReceiptV1 {
        PalwFalseValidReceiptV1::Segmented(PalwSeatReceiptV3 {
            receipt: PalwSeatReceiptV2 {
                claim: claim(),
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: bond(seat),
                signed_daa,
                signature: vec![1; 4],
            },
            segments: PalwSegmentMaskV2(mask),
        })
    }

    fn receipt(seat: u64, mask: u32) -> PalwFalseValidReceiptV1 {
        receipt_at(seat, mask, 7)
    }

    /// A filing of `seat` whose chain answer is `check` — built by the ONE builder.
    fn filing(seat: u64, check: PalwFalseValidFilingCheckV1) -> PalwFalseValidFilingV1 {
        palw_false_valid_filings_v1(&proof(), &[receipt(seat, 0b1111)], &bond(99), |_| false, |_| false, |_| check.clone()).remove(0)
    }

    fn file() -> PalwFalseValidFilingCheckV1 {
        PalwFalseValidFilingCheckV1::File {
            claim_id: claim(),
            offence_id: Hash64::default(),
            producer: bond(0),
            site: PalwFaultSiteV1::Whole,
            acts_on_claim: true,
            class_held: false,
            full_attestation: true,
        }
    }

    fn convicted(at: u64) -> PalwFalseValidFilingCheckV1 {
        PalwFalseValidFilingCheckV1::ConvictedBefore { offence_id: Hash64::default(), accepted_daa: at }
    }

    fn outcome(filings: Vec<PalwFalseValidFilingV1>) -> PalwFalseValidScanOutcomeV1 {
        PalwFalseValidScanOutcomeV1 { claim_id: claim(), receipts: vec![receipt(1, 0b1111)], filings, walked: true }
    }

    fn filer_with_proof(daa: u64) -> PalwFalseValidFilerV1 {
        let mut filer = PalwFalseValidFilerV1::new(10_000, 500, DEPTH);
        assert!(filer.note_proof_v1(proof(), PalwFalseValidSourceV1::CaptureSample { leaf: 3 }, Some(40), daa));
        filer
    }

    /// The scan the filer asks at `daa` (exactly one is due), with the chain answering `answer` per
    /// receipt — `palw_false_valid_filings_v1` over the scan's own skips, as `palw_false_valid_scan_v1`
    /// runs it — applied back. Returns (the receipts asked, what was queued).
    fn scan_and_apply(
        filer: &mut PalwFalseValidFilerV1,
        daa: u64,
        receipts: &[PalwFalseValidReceiptV1],
        queue: &mut PalwCourtQueueV1,
        answer: impl Fn(&PalwFalseValidReceiptV1) -> PalwFalseValidFilingCheckV1,
    ) -> (Vec<PalwFalseValidReceiptV1>, Vec<PalwFalseValidSeamRecordV1>) {
        let scans = filer.due_v1(daa, |_| None);
        assert_eq!(scans.len(), 1, "a scan is due at {daa}");
        let scan = &scans[0];
        let mut asked = Vec::new();
        let filings = palw_false_valid_filings_v1(
            &scan.proof,
            receipts,
            &bond(99),
            |seat| scan.skip.contains(seat),
            |id| scan.not_liable.contains(id),
            |object| {
                let PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } = object else { unreachable!() };
                let payload: PalwPanelFalseValidEvidenceV2 = borsh::from_slice(evidence).unwrap();
                asked.push(payload.receipt.clone());
                answer(&payload.receipt)
            },
        );
        let queued = filer.apply_v1(
            PalwFalseValidScanOutcomeV1 { claim_id: claim(), receipts: receipts.to_vec(), filings, walked: true },
            daa,
            queue,
        );
        (asked, queued)
    }

    /// **The seam**: the object queued under the (seat, claim) key with this filer's round, once; the
    /// record is what the reporter filer needs — the offence key, the evidence id of exactly the bytes
    /// queued, the accused.
    #[test]
    fn the_seam_queues_once_under_the_offence_key_and_records_what_the_reporter_needs() {
        let filing = filing(1, file());
        let mut queue: PalwCourtQueueV1 = Vec::new();
        let record = false_valid_file_seam_v1(&mut queue, &filing, 100).expect("queued");
        assert_eq!(record.offence_key, palw_false_valid_offence_id_v2(&bond(1).0, &claim()));
        assert_eq!((record.accused, record.claim_id, record.queued_daa), (bond(1), claim(), 100));
        let (id, round, responder, object) = &queue[0].clone();
        assert_eq!((*id, *round, *responder), palw_false_valid_queue_key_v1(record.offence_key));
        assert!(palw_false_valid_queued_v1(*round, *responder, object));
        let PalwConsensusObjectV2::ObjectiveOffence { evidence_id, evidence, accused, .. } = object else { panic!("kind 3") };
        assert_eq!(*evidence_id, record.evidence_id);
        assert_eq!(*evidence_id, kaspa_consensus_core::palw_offence_v1::palw_offence_evidence_digest_v1(evidence));
        assert_eq!(*accused, bond(1));
        let payload: PalwPanelFalseValidEvidenceV2 = borsh::from_slice(evidence).unwrap();
        assert!(payload.reporter_reveal.is_empty(), "F7's slot stays empty: R-3 files through 53/54");
        assert_eq!(false_valid_file_seam_v1(&mut queue, &filing, 101), None, "never twice for one offence");
        assert_eq!(queue.len(), 1);
        let declined = self::filing(2, PalwFalseValidFilingCheckV1::NotLiable(PalwOffenceVerifyError::SiteNotAttested));
        assert_eq!(false_valid_file_seam_v1(&mut queue, &declined, 101), None, "a signer the rule does not reach is never queued");
        assert_eq!(palw_false_valid_queued_keys_v1(&queue), std::collections::HashSet::from([record.offence_key]));
        // P2-6's accusation marker and a court move are not this filer's.
        assert!(!palw_false_valid_queued_v1(u32::MAX, false, object));
        assert!(!palw_false_valid_queued_v1(PALW_FALSE_VALID_QUEUE_ROUND_V1, true, object));
    }

    /// **Dedup across ticks**: a filed signer is skipped while its carrier is plausibly in flight
    /// (queued, or sent per `court_moved`, less than a re-file ago); after that the chain decides —
    /// convicted is watched, still convictable re-queues it, at most `MAX_SENDS` times.
    #[test]
    fn a_filed_signer_is_left_alone_while_in_flight_and_then_the_chain_decides() {
        let mut filer = filer_with_proof(100);
        let mut queue: PalwCourtQueueV1 = Vec::new();
        let scans = filer.due_v1(100, |_| None);
        assert_eq!(scans.len(), 1);
        assert_eq!(scans[0].walk_from_daa, 40, "the first walk starts at the claim's bind");
        assert!(scans[0].skip.is_empty());
        let queued = filer.apply_v1(outcome(vec![filing(1, file())]), 100, &mut queue);
        assert_eq!(queued.len(), 1);
        assert_eq!(filer.records().count(), 1);
        let key = queued[0].offence_key;
        assert!(filer.due_v1(100 + PALW_FALSE_VALID_REFILE_DAA_V1 - 1, |_| None).is_empty(), "not due before the re-file");
        // The carrier went out at 130: still in flight at 160 by `court_moved`.
        queue.clear();
        let scans = filer.due_v1(100 + PALW_FALSE_VALID_REFILE_DAA_V1, |k| (*k == key).then_some(130));
        assert!(scans[0].skip.contains(&bond(1)), "sent 30 DAA ago: skipped");
        assert!(scans[0].walk_from_daa >= 40 && scans[0].walk_from_daa <= 100, "incremental, with the reorg margin");
        // Past the re-file: asked; still convictable → queued again (a lost carrier).
        let later = 130 + PALW_FALSE_VALID_REFILE_DAA_V1;
        let scans = filer.due_v1(later, |k| (*k == key).then_some(130));
        assert!(!scans[0].skip.contains(&bond(1)));
        assert_eq!(filer.apply_v1(outcome(vec![filing(1, file())]), later, &mut queue).len(), 1, "re-sent");
        // Convicted on chain: watched (asked again), and settled once past the finality depth.
        queue.clear();
        let at = later + PALW_FALSE_VALID_REFILE_DAA_V1;
        filer.due_v1(at, |_| None);
        assert!(filer.apply_v1(outcome(vec![filing(1, convicted(at - 5))]), at, &mut queue).is_empty());
        let rewalk = at + PALW_FALSE_VALID_REWALK_DAA_V1;
        let scans = filer.due_v1(rewalk, |_| None);
        assert!(!scans[0].skip.contains(&bond(1)), "a conviction above the finality depth is watched");
        filer.apply_v1(outcome(vec![filing(1, convicted(at - 5))]), at + DEPTH, &mut queue);
        let scans = filer.due_v1(at + DEPTH + PALW_FALSE_VALID_REWALK_DAA_V1, |_| None);
        assert!(scans[0].skip.contains(&bond(1)), "settled past the finality depth");
    }

    /// A carrier that keeps vanishing is given up after `MAX_SENDS`.
    #[test]
    fn a_filing_is_sent_at_most_max_sends_times() {
        let mut filer = filer_with_proof(0);
        let mut daa = 0;
        let mut sent = 0;
        for _ in 0..(PALW_FALSE_VALID_MAX_SENDS_V1 + 2) {
            let mut queue: PalwCourtQueueV1 = Vec::new();
            if filer.due_v1(daa, |_| None).is_empty() {
                daa += PALW_FALSE_VALID_REFILE_DAA_V1;
                continue;
            }
            sent += filer.apply_v1(outcome(vec![filing(1, file())]), daa, &mut queue).len() as u32;
            daa += PALW_FALSE_VALID_REFILE_DAA_V1;
        }
        assert_eq!(sent, PALW_FALSE_VALID_MAX_SENDS_V1);
    }

    /// **The review's high finding, the book half (K1)**: no chain answer but a node-policy decline,
    /// a final conviction or the send budget settles a seat. A seat answered `Refused` or `Unrelied`
    /// (a junk receipt read before the licence, a state that later changes) is asked again at the
    /// next walk and files once the chain convicts; a receipt answered `NotLiable` is not asked again
    /// but the seat is — its NEW receipt files.
    #[test]
    fn no_answer_but_a_decline_a_final_conviction_or_the_send_budget_settles_a_seat() {
        let mut filer = filer_with_proof(100);
        let mut queue: PalwCourtQueueV1 = Vec::new();
        let old = receipt_at(2, 0b0001, 7);
        let new = receipt_at(2, 0b1111, 150);
        // First scan: seat 1 is refused (its lock is not in state yet), seat 3 unrelied (junk), seat
        // 2's receipt is not liable.
        let first = [receipt(1, 0b1111), old.clone(), receipt(3, 0b0100)];
        let (asked, queued) = scan_and_apply(&mut filer, 100, &first, &mut queue, |r| match r.inner().seat_bond {
            s if s == bond(1) => PalwFalseValidFilingCheckV1::Refused("the accused holds no Valid lock".into()),
            s if s == bond(2) => PalwFalseValidFilingCheckV1::NotLiable(PalwOffenceVerifyError::SiteNotAttested),
            _ => PalwFalseValidFilingCheckV1::Unrelied("the receipt's signature does not verify".into()),
        });
        assert_eq!(asked.len(), 3);
        assert!(queued.is_empty());
        // Every later walk asks again; the chain now convicts: seats 1 and 3 file, seat 2's old
        // receipt is not asked again, its new one files.
        let mut filed: BTreeSet<PalwBondKeyV2> = BTreeSet::new();
        let later = [receipt(1, 0b1111), old.clone(), new.clone(), receipt(3, 0b0100)];
        let (asked, queued) = scan_and_apply(&mut filer, 100 + PALW_FALSE_VALID_REWALK_DAA_V1, &later, &mut queue, |_| file());
        assert!(!asked.contains(&old), "a receipt answered not liable is not asked again");
        assert!(asked.contains(&new), "the seat's new receipt is");
        filed.extend(queued.iter().map(|r| r.accused));
        assert_eq!(filed, BTreeSet::from([bond(1), bond(2), bond(3)]), "every seat files once the chain convicts");
        // A node-policy decline is the one chain-side answer that settles.
        let mut filer = filer_with_proof(0);
        let mut declined = filing(4, file());
        declined.policy = Some(PalwFalseValidPolicyV1::OwnClaim);
        filer.due_v1(0, |_| None);
        filer.apply_v1(outcome(vec![declined]), 0, &mut queue);
        assert!(filer.due_v1(PALW_FALSE_VALID_REWALK_DAA_V1, |_| None)[0].skip.contains(&bond(4)));
    }

    /// **The review's reorg finding**: a conviction seen is watched until its block is past the
    /// finality depth. A reorg that takes it away (the chain answers `File` again) re-queues the
    /// filing with a fresh send budget, even after `MAX_SENDS` carriers.
    #[test]
    fn a_conviction_is_watched_until_final_and_a_reorg_that_takes_it_is_filed_again() {
        let mut filer = filer_with_proof(0);
        let mut queue: PalwCourtQueueV1 = Vec::new();
        let mut daa = 0;
        for _ in 0..PALW_FALSE_VALID_MAX_SENDS_V1 {
            filer.due_v1(daa, |_| None);
            assert_eq!(filer.apply_v1(outcome(vec![filing(1, file())]), daa, &mut queue).len(), 1);
            queue.clear();
            daa += PALW_FALSE_VALID_REFILE_DAA_V1;
        }
        // The third carrier landed at `daa - 10`.
        filer.due_v1(daa, |_| None);
        filer.apply_v1(outcome(vec![filing(1, convicted(daa - 10))]), daa, &mut queue);
        // A reorg took the block away: the chain convicts again and the filer sends again.
        daa += PALW_FALSE_VALID_REWALK_DAA_V1;
        let scans = filer.due_v1(daa, |_| None);
        assert!(!scans[0].skip.contains(&bond(1)));
        let queued = filer.apply_v1(outcome(vec![filing(1, file())]), daa, &mut queue);
        assert_eq!(queued.len(), 1, "a conviction a reorg took away is filed again, whatever was sent before");
    }

    /// A queued filing the chain no longer convicts (another carrier convicted the seat, or a court
    /// opened) leaves the queue unsent; an open court is asked again soon; a policy decline settles
    /// its seat; a conviction above the finality depth and a liability refusal do not settle theirs.
    #[test]
    fn a_queued_filing_the_chain_now_refuses_leaves_the_queue() {
        let mut filer = filer_with_proof(0);
        let mut queue: PalwCourtQueueV1 = Vec::new();
        filer.due_v1(0, |_| None);
        filer.apply_v1(outcome(vec![filing(1, file()), filing(2, file())]), 0, &mut queue);
        assert_eq!(queue.len(), 2);
        let at = PALW_FALSE_VALID_REFILE_DAA_V1;
        filer.due_v1(at, |_| None);
        let wait = filing(1, PalwFalseValidFilingCheckV1::Wait(PalwOffenceVerifyError::ClaimUnderSession));
        let convicted = filing(2, convicted(at - 1));
        let mut held = filing(3, file());
        held.policy = Some(PalwFalseValidPolicyV1::HeldPartial);
        let not_liable = filing(4, PalwFalseValidFilingCheckV1::NotLiable(PalwOffenceVerifyError::SiteNotAttested));
        assert!(filer.apply_v1(outcome(vec![wait, convicted, held, not_liable]), at, &mut queue).is_empty());
        assert!(queue.is_empty(), "both queued entries left unsent");
        let scans = filer.due_v1(at + PALW_FALSE_VALID_WAIT_RECHECK_DAA_V1, |_| None);
        assert_eq!(scans.len(), 1, "an open court is asked again soon");
        assert!(!scans[0].skip.contains(&bond(1)));
        assert!(scans[0].skip.contains(&bond(3)), "a policy decline settles");
        assert!(!scans[0].skip.contains(&bond(2)), "a conviction above the finality depth is watched");
        assert!(!scans[0].skip.contains(&bond(4)), "a liability refusal is per receipt");
        assert_eq!(scans[0].not_liable.len(), 1);
    }

    /// Bounded: one proof a claim (the first kept), at most `MAX_CASES` (the oldest forgotten), one
    /// scan a tick, and a proof forgotten past its TTL.
    #[test]
    fn the_book_is_bounded() {
        let mut filer = PalwFalseValidFilerV1::new(1_000, 500, DEPTH);
        let source = PalwFalseValidSourceV1::CourtClose { leaf: 1 };
        for n in 0..(PALW_FALSE_VALID_MAX_CASES_V1 as u64 + 3) {
            let mut p = proof();
            p.claim_id = Hash64::from_u64_word(0xA000 + n);
            assert!(filer.note_proof_v1(p, source, None, n));
        }
        assert_eq!(filer.cases.len(), PALW_FALSE_VALID_MAX_CASES_V1);
        assert!(!filer.holds(&Hash64::from_u64_word(0xA000)), "the oldest left first");
        let mut again = proof();
        again.claim_id = Hash64::from_u64_word(0xA000 + 5);
        assert!(!filer.note_proof_v1(again, source, None, 10), "the first proof of a claim is kept");
        assert_eq!(filer.cases[&Hash64::from_u64_word(0xA000 + 5)].not_before_daa, 0, "the look-back, floored at 0");
        assert_eq!(filer.due_v1(100, |_| None).len(), PALW_FALSE_VALID_CASES_PER_TICK_V1);
        assert!(filer.due_v1(2_000, |_| None).is_empty() && filer.is_empty(), "past the TTL, forgotten");
    }

    /// The fence and the funding: below `palw_rcore_plus` nothing runs; a receipts-only node never
    /// files. testnet-12 arms R-core+ at genesis; testnet-11 never. The conviction depth is the
    /// ruleset's finality depth, inside a proof's life.
    #[test]
    fn the_filer_is_armed_past_rcore_plus_on_a_carrying_node_only() {
        use kaspa_consensus_core::config::params::Params;
        use kaspa_consensus_core::network::{NetworkId, NetworkType};
        let t12 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
        assert!(palw_false_valid_armed_v1(&t12, true, 0));
        assert!(!palw_false_valid_armed_v1(&t12, false, 0), "a receipts-only node files nothing");
        let t11 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11));
        assert!(!palw_false_valid_armed_v1(&t11, true, 1_000_000), "below the fence: dormant");
        let filer = PalwFalseValidFilerV1::for_params(&t12);
        assert!(filer.lookback_daa > 0 && filer.ttl_daa > filer.lookback_daa, "sized from testnet-12's windows");
        assert_eq!(filer.conviction_depth_daa, t12.finality_depth());
        assert!(filer.conviction_depth_daa > 0 && filer.conviction_depth_daa < filer.ttl_daa);
    }

    /// **The review's scan finding: `palw_false_valid_scan_v1` over a stubbed chain.** Four chain
    /// blocks: a junk licence (garbage receipts of the real seats over their masks) lands before the
    /// genuine licence, and 32 junk receipts of the real seats and of strangers after it. The walk
    /// reads all of them; only the receipts the chain relied on are admitted (the stub's relied read
    /// knows the genuine five), so both liable seats (4 and 5) are asked on their genuine receipts
    /// and filed. A receipt the chain stops relying on (a reorg) leaves at the next scan; a read
    /// with no tip state leaves the scan un-walked, so the book keeps what it held.
    #[test]
    fn the_scan_admits_only_what_the_chain_relied_on_and_files_the_liable_seats() {
        use kaspa_consensus_core::BlockHash;
        use kaspa_consensus_core::BlockHashMap;
        use kaspa_consensus_core::acceptance_data::{AcceptanceData, AcceptedTxEntry, MergesetBlockAcceptanceData};
        use kaspa_consensus_core::block::Block;
        use kaspa_consensus_core::errors::consensus::{ConsensusError, ConsensusResult};
        use kaspa_consensus_core::header::Header;
        use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        use kaspa_consensus_core::trusted::ExternalGhostdagData;
        use kaspa_consensus_core::tx::Transaction;
        use std::sync::Mutex;

        struct Chain {
            sink: BlockHash,
            headers: HashMap<BlockHash, Arc<Header>>,
            parents: HashMap<BlockHash, BlockHash>,
            acceptance: HashMap<BlockHash, Arc<AcceptanceData>>,
            blocks: HashMap<BlockHash, Block>,
            relied: Mutex<Vec<PalwFalseValidReceiptV1>>,
            dormant: bool,
            asked: Mutex<Vec<PalwFalseValidReceiptV1>>,
        }
        impl kaspa_consensus_core::api::ConsensusApi for Chain {
            fn get_sink(&self) -> BlockHash {
                self.sink
            }
            fn get_header(&self, hash: BlockHash) -> ConsensusResult<Arc<Header>> {
                self.headers.get(&hash).cloned().ok_or(ConsensusError::HeaderNotFound(hash))
            }
            fn get_ghostdag_data(&self, hash: BlockHash) -> ConsensusResult<ExternalGhostdagData> {
                let selected_parent = *self.parents.get(&hash).ok_or(ConsensusError::HeaderNotFound(hash))?;
                Ok(ExternalGhostdagData {
                    blue_score: 0,
                    blue_work: 0.into(),
                    selected_parent,
                    mergeset_blues: vec![],
                    mergeset_reds: vec![],
                    blues_anticone_sizes: BlockHashMap::default(),
                })
            }
            fn get_block_acceptance_data(&self, hash: BlockHash) -> ConsensusResult<Arc<AcceptanceData>> {
                self.acceptance.get(&hash).cloned().ok_or(ConsensusError::HeaderNotFound(hash))
            }
            fn get_block(&self, hash: BlockHash) -> ConsensusResult<Block> {
                self.blocks.get(&hash).cloned().ok_or(ConsensusError::HeaderNotFound(hash))
            }
            fn palw_false_valid_relied_receipts_v1(&self, receipts: Vec<PalwFalseValidReceiptV1>) -> Option<Vec<bool>> {
                if self.dormant {
                    return None;
                }
                let relied = self.relied.lock().unwrap();
                Some(receipts.iter().map(|r| relied.contains(r)).collect())
            }
            fn palw_false_valid_filing_check_v1(&self, object: PalwConsensusObjectV2) -> Option<PalwFalseValidFilingCheckV1> {
                let PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } = object else { return None };
                let payload: PalwPanelFalseValidEvidenceV2 = borsh::from_slice(&evidence).unwrap();
                self.asked.lock().unwrap().push(payload.receipt.clone());
                // The liable seats of the located fault: the full seat (4) and the fault segment's
                // holder (5); the other partial seats are not reached.
                let seat = payload.receipt.inner().seat_bond;
                Some(if seat == bond(4) || seat == bond(5) {
                    file()
                } else {
                    PalwFalseValidFilingCheckV1::NotLiable(PalwOffenceVerifyError::SiteNotAttested)
                })
            }
        }

        let hash = BlockHash::from_u64_word;
        let masks = [0b0001u32, 0b0010, 0b0100, 0b1111, 0b1000];
        let genuine: Vec<PalwFalseValidReceiptV1> = (1..=5).map(|s| receipt_at(s, masks[s as usize - 1], 30)).collect();
        let junk = |seat: u64, mask: u32, n: u64| {
            let mut r = receipt_at(seat, mask, 1_000 + n);
            let PalwFalseValidReceiptV1::Segmented(signed) = &mut r else { unreachable!() };
            signed.receipt.signature = vec![0; 4];
            r
        };
        let licence = |receipts: Vec<PalwFalseValidReceiptV1>| {
            let receipts = receipts
                .into_iter()
                .map(|r| match r {
                    PalwFalseValidReceiptV1::Segmented(signed) => signed,
                    PalwFalseValidReceiptV1::Full(_) => unreachable!(),
                })
                .collect();
            let object = PalwConsensusObjectV2::ReceiptLicensedV2 { claim: claim(), receipts };
            let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
            Transaction::new(
                kaspa_consensus_core::constants::TX_VERSION,
                vec![],
                vec![],
                0,
                kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
                0,
                payload,
            )
        };
        // Chain blocks 1..4 at DAA 10..40; side blocks 11 (junk licence), 12 (the genuine licence),
        // 13 (16 junk of the real seats), 14 (16 of the real seats and 16 of strangers).
        let mut chain = Chain {
            sink: hash(4),
            headers: HashMap::new(),
            parents: HashMap::new(),
            acceptance: HashMap::new(),
            blocks: HashMap::new(),
            relied: Mutex::new(genuine.clone()),
            dormant: false,
            asked: Mutex::new(Vec::new()),
        };
        for (block, daa, parent) in [(1u64, 10u64, 1u64), (2, 20, 1), (3, 30, 2), (4, 40, 3)] {
            let mut header = Header::from_precomputed_hash(hash(block), vec![hash(parent)]);
            header.daa_score = daa;
            chain.headers.insert(hash(block), Arc::new(header));
            chain.parents.insert(hash(block), hash(parent));
        }
        let side = |n: u64, txs: Vec<Transaction>| Block::new(Header::from_precomputed_hash(hash(n), vec![]), txs);
        chain.blocks.insert(hash(11), side(11, vec![licence((1..=5).map(|s| junk(s, masks[s as usize - 1], 0)).collect())]));
        chain.blocks.insert(hash(12), side(12, vec![licence(genuine.clone())]));
        chain.blocks.insert(hash(13), side(13, vec![licence((0..16).map(|n| junk(1 + n % 5, 0b1111, n)).collect())]));
        let strangers: Vec<PalwFalseValidReceiptV1> =
            (0..16).map(|n| junk(1 + n % 5, 0b1111, 100 + n)).chain((0..16).map(|n| junk(200 + n, 0b1111, n))).collect();
        chain.blocks.insert(hash(14), side(14, vec![licence(strangers)]));
        let accepted = |block: u64| MergesetBlockAcceptanceData {
            block_hash: hash(block),
            accepted_transactions: vec![AcceptedTxEntry { transaction_id: Default::default(), index_within_block: 0 }],
        };
        chain.acceptance.insert(hash(1), Arc::new(vec![accepted(11)]));
        chain.acceptance.insert(hash(2), Arc::new(vec![accepted(12)]));
        chain.acceptance.insert(hash(3), Arc::new(vec![accepted(13)]));
        chain.acceptance.insert(hash(4), Arc::new(vec![accepted(14)]));

        let mut filer = PalwFalseValidFilerV1::new(10_000, 500, DEPTH);
        assert!(filer.note_proof_v1(proof(), PalwFalseValidSourceV1::CaptureSample { leaf: 3 }, Some(5), 40));
        let mut queue: PalwCourtQueueV1 = Vec::new();
        let scan = filer.due_v1(40, |_| None).remove(0);
        let outcome = palw_false_valid_scan_v1(&chain, scan, bond(99), 40);
        assert!(outcome.walked);
        let mut kept = outcome.receipts.clone();
        kept.sort_by_key(|r| r.inner().seat_bond);
        assert_eq!(kept, genuine, "the genuine five kept, none of the 37 junk");
        assert!(chain.asked.lock().unwrap().iter().all(|r| genuine.contains(r)), "no junk receipt is ever asked about");
        let queued = filer.apply_v1(outcome, 40, &mut queue);
        let filed: BTreeSet<PalwBondKeyV2> = queued.iter().map(|r| r.accused).collect();
        assert_eq!(filed, BTreeSet::from([bond(4), bond(5)]), "both liable seats filed");
        assert_eq!(queue.len(), 2);

        // A reorg takes seat 1's licence away: at the next scan its receipt leaves the book.
        chain.relied.lock().unwrap().retain(|r| r.inner().seat_bond != bond(1));
        let at = 40 + PALW_FALSE_VALID_REFILE_DAA_V1;
        let scan = filer.due_v1(at, |_| None).remove(0);
        let outcome = palw_false_valid_scan_v1(&chain, scan, bond(99), at);
        assert!(outcome.receipts.iter().all(|r| r.inner().seat_bond != bond(1)), "an unrelied receipt leaves");
        filer.apply_v1(outcome, at, &mut queue);

        // No tip state: nothing walked, nothing lost.
        chain.dormant = true;
        let held = filer.cases[&claim()].receipts.clone();
        let at = at + PALW_FALSE_VALID_REWALK_DAA_V1;
        let scan = filer.due_v1(at, |_| None).remove(0);
        let outcome = palw_false_valid_scan_v1(&chain, scan, bond(99), at);
        assert!(!outcome.walked && outcome.filings.is_empty());
        filer.apply_v1(outcome, at, &mut queue);
        assert_eq!(filer.cases[&claim()].receipts, held, "the book keeps what it held");
        assert_eq!(filer.cases[&claim()].walked_daa, Some(40 + PALW_FALSE_VALID_REFILE_DAA_V1), "and the walk mark stays");
    }

    /// **The panel's call sites, pinned where they live** (thin, as the lanes agreed): the capture
    /// arm notes its refutation right after recording the fault and before its one-move accusation
    /// takes the refutation; a court close notes its proof only as the challenger of an
    /// `ExecutorGuilty` close; every note and the tick's step are gated by the one arming predicate;
    /// the step runs after P2-6's accusations and before the submitter, so a filing queued this tick
    /// rides this tick's priority slot; and the submitter logs the filer's entries by their marker.
    #[test]
    fn the_panel_notes_proofs_and_runs_the_step_where_it_should() {
        let whole = include_str!("palw_panel.rs");
        let source = &whole[..whole.find("#[cfg(test)]\nmod tests {").expect("the test module")];
        let fault = source.find("CaptureSamplesV1::FaultAt { leaf, refutation, openings, prompt_opening } => {").expect("the arm");
        let recorded = fault + source[fault..].find("self.note_seat_fault_v1(duty.claim_id, leaf, 1);").expect("recorded");
        let noted = fault + source[fault..].find("false_valid.note_step_refutation_v1(").expect("noted");
        let accused = fault + source[fault..].find("PalwConsensusObjectV2::ShardCourtAccused {").expect("the accusation");
        assert!(recorded < noted && noted < accused, "noted after the record, before the accusation moves the refutation");
        assert!(source[noted..accused].contains("PalwFalseValidSourceV1::CaptureSample { leaf },"));
        assert!(source[noted..accused].contains("Some(duty.bound_daa),"), "the licence walk starts at the bind");
        let close = source.find("false_valid.note_court_close_v1(").expect("the court close notes");
        let guard = source[..close]
            .rfind("if verdict == kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty && !duty.i_am_responder {");
        assert!(guard.is_some_and(|g| close - g < 200), "only the challenger's ExecutorGuilty close");
        assert_eq!(source.matches("crate::palw_filer_false_valid::palw_false_valid_armed_v1(").count(), 3, "every site is gated");
        let p2_6 = source.find("// --- P2-6: this seat's accusations of withholding ---").expect("P2-6's step");
        let step = source.find("crate::palw_filer_false_valid::palw_false_valid_tick_v1(").expect("the step");
        let submitter = source.find("// --- the collector + submitter's half ---").expect("the submitter");
        assert!(p2_6 < step && step < submitter);
        assert!(source[step..submitter].contains("false_valid.clear();"), "the book is emptied when unarmed");
        assert!(
            source
                .contains("} else if crate::palw_filer_false_valid::palw_false_valid_queued_v1(round, mine_is_responder, &object) {")
        );
    }
}
