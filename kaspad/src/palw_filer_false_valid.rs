//! **ADR-0152 v3.1 N10 / §7.3 P2-8c: the automatic `PanelFalseValidV2` filer** (C-3; T54d).
//!
//! # What it does
//!
//! When this node's own work proves a claim false — the free-prompt capture arm's sample of a leaf
//! that does not recompute (ADR-0098 Decision 2), the refutation its court close filed as the
//! challenger with the executor found guilty, and P2-8b's replay-mismatch contradiction (the
//! `ExecutorRefuted` its replay filer made, [`PalwFalseValidFilerV1::note_executor_refuted_v1`]) —
//! the proof is noted here ([`PalwFalseValidFilerV1::note_proof_v1`]). From then on,
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
//! 3. **files each conviction through P2-8's reporter filer** — ONE door,
//!    [`palw_false_valid_conviction_filing_v1`] handed to `PalwConvictionDoorV1::file`: committed
//!    over this node's bond and the offence's per-(seat, claim) key and `evidence_id` (R-3, N12),
//!    rooted, then the evidence, then the reveal — the same filer, and the same dedup by offence
//!    key, as every other conviction this node files.
//!
//! # What settles a seat, and what does not
//!
//! A seat is done with for the proof's life only when nothing the chain later does can change the
//! answer: node policy declined it (this node's own claim), a conviction of it stands past the
//! finality depth, or `PALW_FILER_HAND_OFFS_PER_OFFENCE_V1` filings left the reporter filer
//! unconvicted. Everything else is
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
//! * **File twice for one offence.** The reporter filer holds one filing an offence key (the per-
//!   (seat, claim) `palw_false_valid_offence_id_v2`), and while it holds one the seat is not asked
//!   about at all; once the filing leaves the filer's book the CHAIN decides — convicted (watched),
//!   or still convictable (the filing ended unconvicted: handed once more, at most
//!   `PALW_FILER_HAND_OFFS_PER_OFFENCE_V1` filings, each of which sends each object at most
//!   `PALW_FILER_MAX_SENDS_V1` times). The ledger's key is the consensus half of the same rule: a
//!   second object for one (seat, claim) folds as a no-op.
//! * **Replay, bisect or re-make a capture.** The proof arrives built (the capture arm and the court
//!   reserved their replay bytes through `reserve_replay_v1` when they ran); what runs here is a
//!   chain walk, a signature check per receipt admitted and the adjudicator's own check of one step,
//!   so it takes no memory-ledger reservation — and it runs on a blocking thread, not on the tick.
//!
//! # What a restart loses
//!
//! This book lives in the panel loop's memory, as P2-6's accusation book does: a restart forgets the
//! proofs held and the hand-off counts. A filing already handed to the reporter filer is NOT lost —
//! that book is persisted (`palw-reporter-filer.v1`), so it still commits, files and reveals after a
//! restart. What is lost is a proof whose licence had not yet named its signers: a capture sample or
//! court close noted before the restart is not re-noted (its duty is gone), so those filings are not
//! made. Persisting this book (or re-deriving proofs from the chain's accepted `CourtClosed` objects)
//! is a later change; no conviction is ever wrong because of it.
//!
//! # Through the reporter filer (P2-8, R-3)
//!
//! Every filing is handed to P2-8's filer, which commits over the offence key, the `evidence_id` and
//! this node's bond, waits until the commitment is two DAA deep, files the evidence and reveals once
//! the conviction consumed it — so the reporter reward R is this node's, and a mempool copier of the
//! evidence commits too late (the filer's module doc says what a copier can still take). Nothing here
//! queues a carrier: the filer's gate is asked before every send, so a filing the chain has since
//! refused (another carrier convicted the seat, a court opened on the claim) waits or ends in the
//! filer, never costs a fee here. A kind-3 conviction has no deadline shorter than the filer's court
//! window (the seat is charged pre- or post-`Final` alike), so the filing is handed with no
//! `file_by`: its evidence waits only for its commitment to root.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use kaspa_consensus_core::palw_false_valid_filing_v1::{
    PalwFalseValidFilingCheckV1, PalwFalseValidFilingV1, PalwFalseValidProofV1, palw_false_valid_admit_receipts_v1,
    palw_false_valid_filings_v1,
};
use kaspa_consensus_core::palw_offence_attribution_v1::{PalwFalseValidReceiptV1, palw_false_valid_receipts_of_licence_v1};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_core::{debug, info, warn};
use kaspa_hashes::Hash64;

use crate::palw_panel::reporter_filer::{
    PALW_FILER_HAND_OFFS_PER_OFFENCE_V1, PalwConvictionDoorV1, PalwConvictionFilingV1, PalwFileOutcomeV1, PalwFilingOriginV1,
};

const PALW_PANEL: &str = "palw-panel";

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

/// **How soon a claim is scanned again after a filing was handed** — six of the court queue's
/// re-plans (`COURT_MOVE_REPLAN_DAA` = 10). The filed seat itself is skipped while the reporter filer
/// holds its filing; the rescan asks the claim's other seats (a supplementary licence) and, once the
/// filing has left the filer's book, the chain about the filed one.
pub(crate) const PALW_FALSE_VALID_REFILE_DAA_V1: u64 = 60;

/// How far below the last walk's DAA the next walk starts, so a reorg that replaced the last blocks
/// walked is walked again.
pub(crate) const PALW_FALSE_VALID_REORG_MARGIN_DAA_V1: u64 = 64;

/// The most chain blocks one walk reads (the court's own walk bound, `attn_root_filings_from_chain_v1`).
pub(crate) const PALW_FALSE_VALID_MAX_WALK_BLOCKS_V1: u64 = 1 << 16;

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
    /// This node's replay filer (P2-8b) bisected the claim's served capture against its own replay:
    /// the contradiction of the `ExecutorRefuted` it made (5 or 12).
    Replay,
}

/// **The ONE door a P2-8c conviction leaves this module through** (it replaced the pre-integration
/// seam that queued the object bare): the filing, as P2-8's reporter filer takes it — the object
/// unchanged, keyed by the filer's own reading of it (`palw_filed_offence_commit_key_v1`, which for an
/// execution- or claim-proving kind 3 is exactly the adjudicator's `palw_false_valid_offence_id_v2`,
/// the key this filing was asked under), origin [`PalwFilingOriginV1::FalseValid`], no `file_by` (the
/// module's "Through the reporter filer"). `None` when the filing does not file, or when the filer's
/// key is not the chain's (a contradiction whose conviction opens no commit–reveal reward: nothing
/// P2-8c builds — its proofs are step refutations — and never filed bare past the integration).
pub(crate) fn palw_false_valid_conviction_filing_v1(filing: &PalwFalseValidFilingV1) -> Option<PalwConvictionFilingV1> {
    if !filing.files() {
        return None;
    }
    PalwConvictionFilingV1::of_offence(filing.object.clone(), filing.claim_id, None, PalwFilingOriginV1::FalseValid)
        .filter(|conviction| conviction.offence_key == filing.offence_id && conviction.evidence_id == filing.evidence_id)
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
    /// depth, or given up after `PALW_FILER_HAND_OFFS_PER_OFFENCE_V1` filings ended unconvicted.
    settled: BTreeSet<PalwBondKeyV2>,
    /// The `evidence_id`s of receipts the liability rule does not reach — per receipt, never per
    /// seat, so a new receipt of the seat is asked afresh.
    not_liable: BTreeSet<Hash64>,
    /// Convictions seen and not yet final: the seat and the DAA of the conviction's block.
    convicted: BTreeMap<PalwBondKeyV2, u64>,
    /// Signers whose filing this node handed the reporter filer: the DAA it was last handed and how
    /// many times ([`PALW_FILER_HAND_OFFS_PER_OFFENCE_V1`]).
    sent: BTreeMap<PalwBondKeyV2, (u64, u8)>,
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
    /// Signers not to ask about: settled, or whose filing the reporter filer holds (in flight).
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
    ttl_daa: u64,
    lookback_daa: u64,
    /// How deep a conviction's block must be before the seat is settled (the ruleset's finality
    /// depth): above it a reorg can still take the conviction away.
    conviction_depth_daa: u64,
}

impl PalwFalseValidFilerV1 {
    pub(crate) fn new(ttl_daa: u64, lookback_daa: u64, conviction_depth_daa: u64) -> Self {
        Self { cases: BTreeMap::new(), ttl_daa, lookback_daa, conviction_depth_daa }
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
    }

    pub(crate) fn holds(&self, claim: &Hash64) -> bool {
        self.cases.contains_key(claim)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.cases.is_empty()
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

    /// **P2-8b's proof, noted here too** (the integration of the Phase 2 lanes): the contradiction a
    /// replay bisection filed as `ExecutorRefuted` proves every liable `Valid` signer of the claim
    /// false as well (N10). Read back from the kind-4 object the replay filer built, so the proof is
    /// exactly the bytes the fold adjudicates for the executor — nothing is re-derived. Nothing is
    /// noted unless `armed` and the object is a kind 4 that decodes, and a claim's first proof is kept.
    pub(crate) fn note_executor_refuted_v1(
        &mut self,
        armed: bool,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        bound_daa: Option<u64>,
        current_daa: u64,
    ) -> bool {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        let Obj::ObjectiveOffence {
            kind: kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted, evidence, ..
        } = object
        else {
            return false;
        };
        if !armed {
            return false;
        }
        let Ok(payload) =
            borsh::from_slice::<kaspa_consensus_core::palw_offence_attribution_v1::PalwExecutorRefutedEvidenceV1>(evidence)
        else {
            return false;
        };
        if self.holds(&payload.claim_id) {
            return false;
        }
        let proof = PalwFalseValidProofV1 {
            claim_id: payload.claim_id,
            contradiction: payload.contradiction,
            prompt_ids_opening: payload.prompt_ids_opening,
        };
        self.note_proof_v1(proof, PalwFalseValidSourceV1::Replay, bound_daa, current_daa)
    }

    fn forget(&mut self, claim: &Hash64) {
        self.cases.remove(claim);
    }

    /// **The scans due at `current_daa`** — at most [`PALW_FALSE_VALID_CASES_PER_TICK_V1`], earliest
    /// first; expired proofs are forgotten here. A signer is skipped while it is settled, or while
    /// the reporter filer holds its filing (`in_flight` of its offence key: whichever lane filed it),
    /// so a filing in flight is never doubled and costs no adjudication.
    pub(crate) fn due_v1(&mut self, current_daa: u64, in_flight: impl Fn(&Hash64) -> bool) -> Vec<PalwFalseValidScanV1> {
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
                let mut skip = case.settled.clone();
                // Every seat this case may ask about — handed by this book, or held by the reporter
                // filer from before a restart of this one (the filer's book is persisted).
                let seats = case.sent.keys().copied().chain(case.receipts.iter().map(|receipt| receipt.inner().seat_bond));
                for seat in seats {
                    if !skip.contains(&seat)
                        && in_flight(&kaspa_consensus_core::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(
                            &seat.0, claim,
                        ))
                    {
                        skip.insert(seat);
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

    /// **Take a scan's result**: keep the receipts, hand what files to the reporter filer through
    /// `door` ([`palw_false_valid_conviction_filing_v1`]), record what cannot change for that receipt
    /// or seat, and schedule the claim's next scan. What settles and what is asked again is the
    /// module's "What settles a seat" rule. A filing the filer already holds is in flight and is left
    /// to it — including one whose signer the chain now answers otherwise (convicted by another
    /// carrier, behind a court that opened): the filer asks the gate before every send and ends a
    /// filing its conviction read ends. Returns the offence keys handed now.
    pub(crate) fn apply_v1(
        &mut self,
        outcome: PalwFalseValidScanOutcomeV1,
        current_daa: u64,
        door: &mut impl PalwConvictionDoorV1,
    ) -> Vec<Hash64> {
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
                // In flight: the reporter filer holds this offence (it commits, files and reveals).
                if door.holds(&filing.offence_id) {
                    continue;
                }
                let handed = case.sent.get(&seat).map_or(0, |(_, n)| *n);
                if handed >= PALW_FILER_HAND_OFFS_PER_OFFENCE_V1 {
                    warn!(
                        "[{PALW_PANEL}] claim {}: the false-Valid filing against {seat:?} is still convictable after {handed} \
                         filings left the reporter filer unconvicted — given up (P2-8c)",
                        filing.claim_id
                    );
                    case.settled.insert(seat);
                    continue;
                }
                let Some(conviction) = palw_false_valid_conviction_filing_v1(filing) else {
                    warn!(
                        "[{PALW_PANEL}] claim {}: the filing against {seat:?} takes no commitment under its offence key — not filed \
                         (P2-8c)",
                        filing.claim_id
                    );
                    case.settled.insert(seat);
                    continue;
                };
                match door.file(conviction) {
                    PalwFileOutcomeV1::Queued { .. } | PalwFileOutcomeV1::AlreadyFiled => {
                        info!(
                            "[{PALW_PANEL}] claim {}: filing PanelFalseValidV2 against the Valid signer {seat:?} through the reporter \
                             filer ({:?} proof; offence {}, evidence {}) — ADR-0152 N10, R-3, P2-8c",
                            filing.claim_id, case.source, filing.offence_id, filing.evidence_id
                        );
                        case.sent.insert(seat, (current_daa, handed + 1));
                        queued_now.push(filing.offence_id);
                    }
                    // The chain consumed the key meanwhile: the next walk reads `ConvictedBefore`.
                    PalwFileOutcomeV1::AlreadyConvicted => waiting = true,
                    // Never this node's own bond: the adjudicator's policy already declines it.
                    PalwFileOutcomeV1::OwnBond => {
                        case.settled.insert(seat);
                    }
                    // The gate at the filer's read, a full book, no tip: nothing spent, asked again soon.
                    refused @ (PalwFileOutcomeV1::NotAdmitted(_) | PalwFileOutcomeV1::Full | PalwFileOutcomeV1::Unreadable) => {
                        debug!(
                            "[{PALW_PANEL}] claim {}: the reporter filer does not take {seat:?}'s filing now — {refused:?} (P2-8c)",
                            filing.claim_id
                        );
                        waiting = true;
                    }
                }
                continue;
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

/// **The tick's step** (called by the panel once a tick, past the fence, on a node that carries,
/// before the reporter filer's own tick — so a filing handed here commits in the same tick): the
/// due scans run off the tick, and their filings are handed to the reporter filer through `door`
/// (the panel's `PalwPanelConvictionDoorV1`). Nothing here touches the court queue or `court_moved`:
/// the filer queues, and prunes, its own.
pub(crate) async fn palw_false_valid_tick_v1(
    filer: &mut PalwFalseValidFilerV1,
    session: &kaspa_consensusmanager::ConsensusProxy,
    own: PalwBondKeyV2,
    current_daa: u64,
    door: &mut impl PalwConvictionDoorV1,
) {
    if filer.is_empty() {
        return;
    }
    let scans = filer.due_v1(current_daa, |offence| door.holds(offence));
    for scan in scans {
        let outcome = session.clone().spawn_blocking(move |c| palw_false_valid_scan_v1(c, scan, own, current_daa)).await;
        filer.apply_v1(outcome, current_daa, door);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_panel::reporter_filer::{PALW_FILER_ROUND_COMMIT_V1, PalwBookDoorV1, palw_filer_queued_v1};
    use kaspa_consensus_core::palw_false_valid_filing_v1::PalwFalseValidPolicyV1;
    use kaspa_consensus_core::palw_offence_attribution_v1::{
        PalwFaultSiteV1, PalwPanelFalseValidEvidenceV2, palw_false_valid_offence_id_v2,
    };
    use kaspa_consensus_core::palw_offence_v1::{PalwOffenceKindV1, PalwOffenceVerifyError};
    use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3};
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
    use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use std::collections::HashMap;

    const CLAIM: u64 = 0xC1A1;
    const DEPTH: u64 = 600;

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xB0_0000 + n), index: 0 })
    }

    fn claim() -> Hash64 {
        Hash64::from_u64_word(CLAIM)
    }

    /// The capture arm's proof: a real floor step refutation (the reporter filer keys a kind 3 by
    /// its contradiction, so a stand-in that is not execution-proving would take no commitment).
    fn proof() -> PalwFalseValidProofV1 {
        let (refutation, openings, prompt) = crate::palw_panel::reporter_filer::palw_floor_step_refutation_v1();
        PalwFalseValidProofV1::step_arithmetic_v1(claim(), refutation, openings, prompt.as_ref())
    }

    /// This node's reporter filer, with the chain stubbed: the book's own rule at a read that admits.
    fn door() -> PalwBookDoorV1 {
        PalwBookDoorV1::new(bond(99), 0)
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
    /// runs it — applied back through `door`. Returns (the receipts asked, the offence keys handed).
    fn scan_and_apply(
        filer: &mut PalwFalseValidFilerV1,
        daa: u64,
        receipts: &[PalwFalseValidReceiptV1],
        door: &mut PalwBookDoorV1,
        answer: impl Fn(&PalwFalseValidReceiptV1) -> PalwFalseValidFilingCheckV1,
    ) -> (Vec<PalwFalseValidReceiptV1>, Vec<Hash64>) {
        let scans = filer.due_v1(daa, |key| door.holds(key));
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
            door,
        );
        (asked, queued)
    }

    /// **The door that replaced the seam**: a filing is handed to P2-8's reporter filer as the filer
    /// keys it — the object unchanged, under the adjudicator's per-(seat, claim) key and the digest of
    /// exactly those bytes (N12), origin `FalseValid`, no `file_by` — and the filer COMMITS first (its
    /// tick queues `ReporterCommitted`, never the bare evidence). Handed once: a second hand-off of the
    /// same offence is the filer's `AlreadyFiled`; a signer the rule does not reach is never handed.
    #[test]
    fn the_door_hands_the_filing_to_the_reporter_filer_once_and_it_commits_first() {
        let filing = filing(1, file());
        let conviction = palw_false_valid_conviction_filing_v1(&filing).expect("an execution-proving kind 3 takes a commitment");
        let key = palw_false_valid_offence_id_v2(&bond(1).0, &claim());
        assert_eq!((conviction.offence_key, conviction.evidence_id), (key, filing.evidence_id));
        assert_eq!((conviction.accused, conviction.claim_id, conviction.file_by_daa), (bond(1), claim(), None));
        assert_eq!((conviction.origin, conviction.fallback.clone()), (PalwFilingOriginV1::FalseValid, None));
        assert_eq!(conviction.object, filing.object, "the object unchanged");
        let PalwConsensusObjectV2::ObjectiveOffence { kind, evidence_id, evidence, accused } = &conviction.object else { panic!() };
        assert_eq!((*kind, *accused), (PalwOffenceKindV1::PanelFalseValidV2, bond(1)));
        assert_eq!(*evidence_id, kaspa_consensus_core::palw_offence_v1::palw_offence_evidence_digest_v1(evidence));
        let payload: PalwPanelFalseValidEvidenceV2 = borsh::from_slice(evidence).unwrap();
        assert!(payload.reporter_reveal.is_empty(), "F7's slot stays empty: R-3 files through 53/54");
        let declined = self::filing(2, PalwFalseValidFilingCheckV1::NotLiable(PalwOffenceVerifyError::SiteNotAttested));
        assert_eq!(palw_false_valid_conviction_filing_v1(&declined), None, "a signer the rule does not reach is never handed");

        let mut filer = filer_with_proof(100);
        let mut door = door();
        filer.due_v1(100, |k| door.holds(k));
        assert_eq!(filer.apply_v1(outcome(vec![filing.clone(), declined]), 100, &mut door), vec![key]);
        assert_eq!(door.handed.len(), 1);
        assert!(door.holds(&key));
        assert_eq!(door.book.entry(&key).expect("in the book").reporter, bond(99), "this node's bond is the reporter");
        // The filer's tick: the commitment goes first, under the offence key; the evidence waits.
        let mut queue = Vec::new();
        let read = door.read.clone();
        door.book.tick(&mut queue, &mut HashMap::new(), &mut std::collections::HashSet::new(), |_, _| Some(read.clone()));
        assert_eq!(queue.len(), 1);
        let (id, round, responder, object) = &queue[0];
        assert_eq!((*id, *round, *responder), (key, PALW_FILER_ROUND_COMMIT_V1, false));
        assert!(palw_filer_queued_v1(*round, *responder, object));
        assert!(matches!(object, PalwConsensusObjectV2::ReporterCommitted { .. }), "committed before the evidence is public");
        // Never twice: the book holds it (and the scan skips the seat while it does).
        assert_eq!(door.file(conviction), PalwFileOutcomeV1::AlreadyFiled);
        let scans = filer.due_v1(100 + PALW_FALSE_VALID_REFILE_DAA_V1, |k| door.holds(k));
        assert!(scans[0].skip.contains(&bond(1)), "in flight: not asked");
    }

    /// **Dedup across ticks**: a handed signer is skipped while the reporter filer holds its filing;
    /// once the filing leaves the book the chain decides — still convictable hands it once more (a
    /// filing that ended unconvicted), convicted is watched and settled past the finality depth.
    #[test]
    fn a_filed_signer_is_left_alone_while_in_flight_and_then_the_chain_decides() {
        let mut filer = filer_with_proof(100);
        let mut door = door();
        let scans = filer.due_v1(100, |k| door.holds(k));
        assert_eq!(scans.len(), 1);
        assert_eq!(scans[0].walk_from_daa, 40, "the first walk starts at the claim's bind");
        assert!(scans[0].skip.is_empty());
        let queued = filer.apply_v1(outcome(vec![filing(1, file())]), 100, &mut door);
        assert_eq!(queued.len(), 1);
        let key = queued[0];
        assert!(filer.due_v1(100 + PALW_FALSE_VALID_REFILE_DAA_V1 - 1, |k| door.holds(k)).is_empty(), "not due before the re-file");
        let scans = filer.due_v1(100 + PALW_FALSE_VALID_REFILE_DAA_V1, |k| door.holds(k));
        assert!(scans[0].skip.contains(&bond(1)), "the filer holds it: skipped");
        assert!(scans[0].walk_from_daa >= 40 && scans[0].walk_from_daa <= 100, "incremental, with the reorg margin");
        // The filing ends unconvicted (Expired): asked again; still convictable → handed once more.
        assert_eq!(door.expire_all(), 1);
        let later = 100 + 2 * PALW_FALSE_VALID_REFILE_DAA_V1;
        let scans = filer.due_v1(later, |k| door.holds(k));
        assert!(!scans[0].skip.contains(&bond(1)));
        assert_eq!(filer.apply_v1(outcome(vec![filing(1, file())]), later, &mut door), vec![key], "handed again");
        assert_eq!(door.queued(), vec![key, key]);
        // Convicted on chain (the filer let it go at its sweep): watched, and settled once final.
        door.expire_all();
        let at = later + PALW_FALSE_VALID_REFILE_DAA_V1;
        filer.due_v1(at, |k| door.holds(k));
        assert!(filer.apply_v1(outcome(vec![filing(1, convicted(at - 5))]), at, &mut door).is_empty());
        let rewalk = at + PALW_FALSE_VALID_REWALK_DAA_V1;
        let scans = filer.due_v1(rewalk, |k| door.holds(k));
        assert!(!scans[0].skip.contains(&bond(1)), "a conviction above the finality depth is watched");
        filer.apply_v1(outcome(vec![filing(1, convicted(at - 5))]), at + DEPTH, &mut door);
        let scans = filer.due_v1(at + DEPTH + PALW_FALSE_VALID_REWALK_DAA_V1, |k| door.holds(k));
        assert!(scans[0].skip.contains(&bond(1)), "settled past the finality depth");
        assert_eq!(door.handed.len(), 2, "never handed while held, never after the conviction");
    }

    /// A filing that keeps ending unconvicted is given up after `PALW_FILER_HAND_OFFS_PER_OFFENCE_V1`
    /// hand-offs (each of which the filer sends at most `PALW_FILER_MAX_SENDS_V1` times); a
    /// refusal by the filer (a full book, the gate at its read) costs no hand-off.
    #[test]
    fn a_filing_is_handed_to_the_reporter_filer_a_bounded_number_of_times() {
        let mut filer = filer_with_proof(0);
        let mut door = door();
        let mut daa = 0;
        // The filer's gate refuses at its read: nothing handed, nothing counted.
        door.read.object_gate = Some(Err("a court is open on the claim".into()));
        filer.due_v1(daa, |k| door.holds(k));
        assert!(filer.apply_v1(outcome(vec![filing(1, file())]), daa, &mut door).is_empty());
        door.read.object_gate = Some(Ok(()));
        let mut handed = 0;
        for _ in 0..(PALW_FILER_HAND_OFFS_PER_OFFENCE_V1 + 3) {
            daa += PALW_FALSE_VALID_REWALK_DAA_V1;
            if filer.due_v1(daa, |k| door.holds(k)).is_empty() {
                continue;
            }
            handed += filer.apply_v1(outcome(vec![filing(1, file())]), daa, &mut door).len() as u8;
            door.expire_all();
        }
        assert_eq!(handed, PALW_FILER_HAND_OFFS_PER_OFFENCE_V1);
        assert_eq!(door.queued().len(), PALW_FILER_HAND_OFFS_PER_OFFENCE_V1 as usize);
    }

    /// **The review's high finding, the book half (K1)**: no chain answer but a node-policy decline,
    /// a final conviction or the send budget settles a seat. A seat answered `Refused` or `Unrelied`
    /// (a junk receipt read before the licence, a state that later changes) is asked again at the
    /// next walk and files once the chain convicts; a receipt answered `NotLiable` is not asked again
    /// but the seat is — its NEW receipt files.
    #[test]
    fn no_answer_but_a_decline_a_final_conviction_or_the_send_budget_settles_a_seat() {
        let mut filer = filer_with_proof(100);
        let mut door = door();
        let old = receipt_at(2, 0b0001, 7);
        let new = receipt_at(2, 0b1111, 150);
        // First scan: seat 1 is refused (its lock is not in state yet), seat 3 unrelied (junk), seat
        // 2's receipt is not liable.
        let first = [receipt(1, 0b1111), old.clone(), receipt(3, 0b0100)];
        let (asked, queued) = scan_and_apply(&mut filer, 100, &first, &mut door, |r| match r.inner().seat_bond {
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
        let (asked, queued) = scan_and_apply(&mut filer, 100 + PALW_FALSE_VALID_REWALK_DAA_V1, &later, &mut door, |_| file());
        assert!(!asked.contains(&old), "a receipt answered not liable is not asked again");
        assert!(asked.contains(&new), "the seat's new receipt is");
        filed.extend(door.handed.iter().filter(|(f, _)| queued.contains(&f.offence_key)).map(|(f, _)| f.accused));
        assert_eq!(filed, BTreeSet::from([bond(1), bond(2), bond(3)]), "every seat files once the chain convicts");
        // A node-policy decline is the one chain-side answer that settles.
        let mut filer = filer_with_proof(0);
        let mut declined = filing(4, file());
        declined.policy = Some(PalwFalseValidPolicyV1::OwnClaim);
        filer.due_v1(0, |k| door.holds(k));
        filer.apply_v1(outcome(vec![declined]), 0, &mut door);
        assert!(filer.due_v1(PALW_FALSE_VALID_REWALK_DAA_V1, |k| door.holds(k))[0].skip.contains(&bond(4)));
    }

    /// **The review's reorg finding**: a conviction seen is watched until its block is past the
    /// finality depth. A reorg that takes it away (the chain answers `File` again) hands the filing
    /// to the reporter filer again with a fresh budget, even after every hand-off was spent.
    #[test]
    fn a_conviction_is_watched_until_final_and_a_reorg_that_takes_it_is_filed_again() {
        let mut filer = filer_with_proof(0);
        let mut door = door();
        let mut daa = 0;
        for _ in 0..PALW_FILER_HAND_OFFS_PER_OFFENCE_V1 {
            filer.due_v1(daa, |k| door.holds(k));
            assert_eq!(filer.apply_v1(outcome(vec![filing(1, file())]), daa, &mut door).len(), 1);
            door.expire_all();
            daa += PALW_FALSE_VALID_REFILE_DAA_V1;
        }
        // The last filing's evidence landed at `daa - 10`.
        filer.due_v1(daa, |k| door.holds(k));
        filer.apply_v1(outcome(vec![filing(1, convicted(daa - 10))]), daa, &mut door);
        // A reorg took the block away: the chain convicts again and the filer is handed it again.
        daa += PALW_FALSE_VALID_REWALK_DAA_V1;
        let scans = filer.due_v1(daa, |k| door.holds(k));
        assert!(!scans[0].skip.contains(&bond(1)));
        let queued = filer.apply_v1(outcome(vec![filing(1, file())]), daa, &mut door);
        assert_eq!(queued.len(), 1, "a conviction a reorg took away is filed again, whatever was handed before");
    }

    /// A filing the reporter filer holds is LEFT to it when the chain now answers otherwise (another
    /// carrier convicted the seat, a court opened): the filer asks the gate before every send and ends
    /// on the conviction it reads, so nothing is withdrawn or handed here. Once it has left the book,
    /// an open court is asked again soon; a policy decline settles its seat; a conviction above the
    /// finality depth and a liability refusal do not settle theirs.
    #[test]
    fn a_filing_the_chain_now_refuses_is_left_to_the_filer_and_then_the_chain_decides() {
        let mut filer = filer_with_proof(0);
        let mut door = door();
        filer.due_v1(0, |k| door.holds(k));
        filer.apply_v1(outcome(vec![filing(1, file()), filing(2, file())]), 0, &mut door);
        assert_eq!(door.book.len(), 2);
        let at = PALW_FALSE_VALID_REFILE_DAA_V1;
        filer.due_v1(at, |k| door.holds(k));
        let wait = || filing(1, PalwFalseValidFilingCheckV1::Wait(PalwOffenceVerifyError::ClaimUnderSession));
        let convicted_ = || filing(2, convicted(at - 1));
        let held = || {
            let mut held = filing(3, file());
            held.policy = Some(PalwFalseValidPolicyV1::HeldPartial);
            held
        };
        let not_liable = || filing(4, PalwFalseValidFilingCheckV1::NotLiable(PalwOffenceVerifyError::SiteNotAttested));
        assert!(filer.apply_v1(outcome(vec![wait(), convicted_(), held(), not_liable()]), at, &mut door).is_empty());
        assert_eq!((door.book.len(), door.handed.len()), (2, 2), "left to the filer; nothing more handed");
        let scans = filer.due_v1(at + PALW_FALSE_VALID_WAIT_RECHECK_DAA_V1, |k| door.holds(k));
        assert!(scans[0].skip.contains(&bond(1)), "in flight while the filer holds it");
        door.expire_all();
        let at = at + PALW_FALSE_VALID_WAIT_RECHECK_DAA_V1;
        filer.apply_v1(outcome(vec![wait(), convicted_(), held(), not_liable()]), at, &mut door);
        let scans = filer.due_v1(at + PALW_FALSE_VALID_WAIT_RECHECK_DAA_V1, |k| door.holds(k));
        assert_eq!(scans.len(), 1, "an open court is asked again soon");
        assert!(!scans[0].skip.contains(&bond(1)));
        assert!(scans[0].skip.contains(&bond(3)), "a policy decline settles");
        assert!(!scans[0].skip.contains(&bond(2)), "a conviction above the finality depth is watched");
        assert!(!scans[0].skip.contains(&bond(4)), "a liability refusal is per receipt");
        assert_eq!(scans[0].not_liable.len(), 1);
    }

    /// **P2-8b's proof, noted for P2-8c** (the integration): the replay filer's kind 4 is read back
    /// into a proof — its contradiction and prompt tile, byte for byte — sourced `Replay`, walked from
    /// the claim's bind; a claim's first proof is kept; nothing is noted unarmed, or from another kind.
    #[test]
    fn a_replay_filers_executor_refuted_is_noted_as_the_claims_proof() {
        let (refutation, openings, prompt) = crate::palw_panel::reporter_filer::palw_floor_step_refutation_v1();
        let contradiction = kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::StepArithmetic {
            refutation: refutation.clone(),
            operand_openings: openings.clone(),
        };
        let kind4 = kaspa_consensus_core::palw_offence_attribution_v1::palw_executor_refuted_object_v1(
            bond(0),
            claim(),
            contradiction.clone(),
            prompt.clone(),
        );
        let mut filer = PalwFalseValidFilerV1::new(10_000, 500, DEPTH);
        assert!(!filer.note_executor_refuted_v1(false, &kind4, Some(40), 100), "unarmed: nothing");
        assert!(!filer.note_executor_refuted_v1(true, &filing(1, file()).object, Some(40), 100), "a kind 3 is not P2-8b's");
        assert!(filer.note_executor_refuted_v1(true, &kind4, Some(40), 100));
        let case = &filer.cases[&claim()];
        assert_eq!((case.proof.contradiction.clone(), case.proof.prompt_ids_opening.clone()), (contradiction, prompt.clone()));
        assert_eq!((case.source, case.not_before_daa), (PalwFalseValidSourceV1::Replay, 40));
        assert_eq!(*case.proof, proof(), "the capture arm's proof of the same leaf is the same proof");
        assert!(!filer.note_executor_refuted_v1(true, &kind4, Some(40), 101), "the first proof of a claim is kept");
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
        assert_eq!(filer.due_v1(100, |_| false).len(), PALW_FALSE_VALID_CASES_PER_TICK_V1);
        assert!(filer.due_v1(2_000, |_| false).is_empty() && filer.is_empty(), "past the TTL, forgotten");
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
        let mut door = door();
        let scan = filer.due_v1(40, |k| door.holds(k)).remove(0);
        let outcome = palw_false_valid_scan_v1(&chain, scan, bond(99), 40);
        assert!(outcome.walked);
        let mut kept = outcome.receipts.clone();
        kept.sort_by_key(|r| r.inner().seat_bond);
        assert_eq!(kept, genuine, "the genuine five kept, none of the 37 junk");
        assert!(chain.asked.lock().unwrap().iter().all(|r| genuine.contains(r)), "no junk receipt is ever asked about");
        let queued = filer.apply_v1(outcome, 40, &mut door);
        let filed: BTreeSet<PalwBondKeyV2> = door.handed.iter().map(|(filing, _)| filing.accused).collect();
        assert_eq!(filed, BTreeSet::from([bond(4), bond(5)]), "both liable seats filed");
        assert_eq!((queued.len(), door.book.len()), (2, 2), "each through the reporter filer");

        // A reorg takes seat 1's licence away: at the next scan its receipt leaves the book.
        chain.relied.lock().unwrap().retain(|r| r.inner().seat_bond != bond(1));
        let at = 40 + PALW_FALSE_VALID_REFILE_DAA_V1;
        let scan = filer.due_v1(at, |k| door.holds(k)).remove(0);
        let outcome = palw_false_valid_scan_v1(&chain, scan, bond(99), at);
        assert!(outcome.receipts.iter().all(|r| r.inner().seat_bond != bond(1)), "an unrelied receipt leaves");
        filer.apply_v1(outcome, at, &mut door);

        // No tip state: nothing walked, nothing lost.
        chain.dormant = true;
        let held = filer.cases[&claim()].receipts.clone();
        let at = at + PALW_FALSE_VALID_REWALK_DAA_V1;
        let scan = filer.due_v1(at, |k| door.holds(k)).remove(0);
        let outcome = palw_false_valid_scan_v1(&chain, scan, bond(99), at);
        assert!(!outcome.walked && outcome.filings.is_empty());
        filer.apply_v1(outcome, at, &mut door);
        assert_eq!(filer.cases[&claim()].receipts, held, "the book keeps what it held");
        assert_eq!(filer.cases[&claim()].walked_daa, Some(40 + PALW_FALSE_VALID_REFILE_DAA_V1), "and the walk mark stays");
    }

    /// **The panel's call sites, pinned where they live** (thin, as the lanes agreed): the capture
    /// arm notes its refutation right after recording the fault and before its one-move accusation
    /// takes the refutation; a court close notes its proof only as the challenger of an
    /// `ExecutorGuilty` close; every note and the tick's step are gated by the one arming predicate;
    /// the step runs after P2-6's accusations and BEFORE the reporter filer's tick, through the panel's
    /// door over that filer's book, so a filing handed this tick commits this tick; and nothing of this
    /// lane is queued or logged by the carrier lane itself (the filer's objects are the filer's).
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
        assert_eq!(source.matches("crate::palw_filer_false_valid::palw_false_valid_armed_v1(").count(), 4, "every site is gated");
        // P2-8b's proof: the replay filer's kind 4 of this tick, noted right after its tick.
        let replay = source.find(".replay_filer_tick_v1(").expect("the replay filer's tick");
        let noted_replay = source.find("false_valid.note_executor_refuted_v1(").expect("the replay's proof is noted");
        assert!(replay < noted_replay && source[replay..noted_replay].contains("if let Some((refuted, bound_daa)) = &replay_made {"));
        assert!(source[noted_replay..noted_replay + 400].contains("&refuted.object,\n                    Some(*bound_daa),"));
        let p2_6 = source.find("// --- P2-6: this seat's accusations of withholding ---").expect("P2-6's step");
        let step = source.find("crate::palw_filer_false_valid::palw_false_valid_tick_v1(").expect("the step");
        let submitter = source.find("// --- the collector + submitter's half ---").expect("the submitter");
        assert!(p2_6 < step && step < submitter);
        assert!(noted_replay < step, "a replay's proof is scanned for in the same tick");
        assert!(source[step..submitter].contains("false_valid.clear();"), "the book is emptied when unarmed");
        let call = &source[step..step + source[step..].find(".await;").expect("awaited")];
        assert!(
            call.contains("&mut self.conviction_door_v1(&session, &mut reporter_filer, bond_key, network_domain),"),
            "through the panel's door over the reporter filer's book: {call}"
        );
        let filer_tick = source.find("self.reporter_filer_tick_v1(").expect("the reporter filer's tick");
        assert!(step < filer_tick && filer_tick < submitter, "handed before the filer ticks");
        assert!(!source.contains("palw_false_valid_queued_v1"), "the lane queues nothing of its own");
        let this = include_str!("palw_filer_false_valid.rs");
        let body = &this[..this.find("#[cfg(test)]\nmod tests {").expect("the tests")];
        assert!(
            !body.contains("court_pending:") && !body.contains("court_pending.") && !body.contains("court_moved:"),
            "the lane never touches the court queue"
        );
    }
}
