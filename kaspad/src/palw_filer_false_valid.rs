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
//! 1. **reads the licence off the chain** — every accepted `ReceiptLicensed`, `ReceiptLicensedV2` and
//!    `OptimisticLicensed` naming the claim, walked back from the sink to the claim's bind (or a
//!    bounded look-back) and then incrementally, so a supplementary receipt is seen too — and keeps
//!    each `Valid` receipt in the form it was LICENSED in (`Full` for a V2 receipt, `Segmented` with
//!    its mask for a V3 one: C-3, `palw_false_valid_receipts_of_licence_v1`);
//! 2. **asks the chain, per signer, what a filing comes to** (`palw_false_valid_filings_v1` over the
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
//! # What it never does
//!
//! * **File against this node's own bond** — its own receipt is never built into evidence, and a
//!   claim this node produced is never filed on (a conviction that acts on the claim charges its
//!   producer S2/S3).
//! * **Name a partial-mask signer of a held-regime class** (`PALW_FALSE_VALID_HELD_NAMES_PARTIALS_V1`
//!   = `false`: the operator's decision that partial seats are not bound at launch for held units,
//!   IA-11) — full-mask signers are named on every class.
//! * **File twice for one offence.** One queue entry per (seat, claim) key; a filing sent is not asked
//!   about again for [`PALW_FALSE_VALID_REFILE_DAA_V1`], after which the CHAIN decides — convicted
//!   (settled), or still convictable (the carrier was lost: sent again, at most
//!   [`PALW_FALSE_VALID_MAX_SENDS_V1`] times). The ledger's key is the consensus half of the same
//!   rule: a second object for one (seat, claim) folds as a no-op.
//! * **Replay, bisect or re-make a capture.** The proof arrives built (the capture arm and the court
//!   reserved their replay bytes through `reserve_replay_v1` when they ran); what runs here is a
//!   chain walk and the adjudicator's own check of one step, so it takes no memory-ledger
//!   reservation — and it runs on a blocking thread, not on the tick.
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
    PALW_FALSE_VALID_RECEIPTS_PER_CLAIM_V1, PalwFalseValidFilingCheckV1, PalwFalseValidFilingV1, PalwFalseValidProofV1,
    palw_false_valid_filings_v1,
};
use kaspa_consensus_core::palw_offence_attribution_v1::{PalwFalseValidReceiptV1, palw_false_valid_receipts_of_licence_v1};
use kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_core::{info, warn};
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

/// Claims scanned a tick: each scan is a chain walk and one adjudication per signer, on a blocking
/// thread — one a tick keeps a burst of faults from taking the panel's blocking pool.
pub(crate) const PALW_FALSE_VALID_CASES_PER_TICK_V1: usize = 1;

/// How often a proof's claim is walked again for receipts (a supplementary licence) and its
/// unsettled signers asked again.
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
/// ticks and restarts of the book (`court_moved` keys the carrier's send by it).
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
    receipts: Vec<PalwFalseValidReceiptV1>,
    /// The DAA the last walk ran at (`None`: never walked).
    walked_daa: Option<u64>,
    next_daa: u64,
    /// Signers done with: convicted, not liable, refused, declined, or given up on.
    settled: BTreeSet<PalwBondKeyV2>,
    /// Signers whose filing this node queued: the DAA it was last queued and how many times.
    sent: BTreeMap<PalwBondKeyV2, (u64, u32)>,
}

/// **One scan's input**, handed to a blocking thread.
#[derive(Clone, Debug)]
pub(crate) struct PalwFalseValidScanV1 {
    pub claim_id: Hash64,
    pub proof: Arc<PalwFalseValidProofV1>,
    pub receipts: Vec<PalwFalseValidReceiptV1>,
    /// Walk the chain down to this DAA for licence receipts.
    pub walk_from_daa: u64,
    /// Signers not to ask about: settled, or queued/sent less than a re-file ago.
    pub skip: BTreeSet<PalwBondKeyV2>,
}

/// **One scan's result**: the receipts the claim's licences carried and each asked signer's filing.
#[derive(Clone, Debug)]
pub(crate) struct PalwFalseValidScanOutcomeV1 {
    pub claim_id: Hash64,
    pub receipts: Vec<PalwFalseValidReceiptV1>,
    pub filings: Vec<PalwFalseValidFilingV1>,
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
}

impl PalwFalseValidFilerV1 {
    pub(crate) fn new(ttl_daa: u64, lookback_daa: u64) -> Self {
        Self { cases: BTreeMap::new(), records: BTreeMap::new(), ttl_daa, lookback_daa }
    }

    /// **Sized from the ruleset's windows.** A licence precedes a court's close by at most the
    /// receipt and challenge windows and a court's life, so the look-back is those (the court's four
    /// times over, its rungs); a proof then lives one more court window — the evidence horizon a
    /// `Final`'s locks and liability row keep (`palw_panel_liability_expiry_v1`) — past it.
    pub(crate) fn for_params(params: &kaspa_consensus_core::config::params::Params) -> Self {
        let (receipt, challenge, court) = match &params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                (bundle.state.window_receipt(), bundle.state.window_challenge(), bundle.state.window_court())
            }
            _ => (0, 0, 0),
        };
        let lookback = receipt.saturating_add(challenge).saturating_add(court.saturating_mul(4));
        Self::new(lookback.saturating_add(court).max(PALW_FALSE_VALID_REWALK_DAA_V1), lookback)
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
                sent: BTreeMap::new(),
            },
        );
        true
    }

    /// **A step refutation this node built, as a proof** (`StepArithmetic` in the carriage the
    /// caller already built — the capture sampler's and the court close's
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
        let proof = PalwFalseValidProofV1 {
            claim_id,
            contradiction: kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::StepArithmetic {
                refutation: refutation.clone(),
                operand_openings: operand_openings.to_vec(),
            },
            prompt_ids_opening: prompt_ids_opening.cloned(),
        };
        self.note_proof_v1(proof, source, bound_daa, current_daa)
    }

    /// **A court close this node sent as the challenger, read `ExecutorGuilty` by the chain** — its
    /// arithmetic refutation of `leaf` (either prompt carriage) is the proof. Every other close form
    /// (a decode token, a fused-attention dissection) proves nothing a kind-3 contradiction carries,
    /// and is passed over: the court's own `CourtFraud` void is what those leave behind.
    pub(crate) fn note_court_close_v1(
        &mut self,
        armed: bool,
        claim_id: Hash64,
        proof: &kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2,
        leaf: u64,
        current_daa: u64,
    ) -> bool {
        use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2 as P;
        let source = PalwFalseValidSourceV1::CourtClose { leaf };
        match proof {
            P::Arithmetic { refutation, operand_openings } => {
                self.note_step_refutation_v1(armed, claim_id, refutation, operand_openings, None, source, None, current_daa)
            }
            P::ArithmeticOpened { refutation, operand_openings, prompt_ids_opening } => self.note_step_refutation_v1(
                armed,
                claim_id,
                refutation,
                operand_openings,
                Some(prompt_ids_opening),
                source,
                None,
                current_daa,
            ),
            _ => false,
        }
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
                }
            })
            .collect()
    }

    /// **Take a scan's result**: keep the receipts, file what files through the seam, settle what
    /// cannot change, and schedule the claim's next scan. A queued filing whose signer the chain now
    /// answers otherwise (convicted by another carrier, refused, behind a court that opened) leaves
    /// the queue unsent. Returns the seam's records of what was queued now.
    pub(crate) fn apply_v1(
        &mut self,
        outcome: PalwFalseValidScanOutcomeV1,
        current_daa: u64,
        court_pending: &mut PalwCourtQueueV1,
    ) -> Vec<PalwFalseValidSeamRecordV1> {
        let Some(case) = self.cases.get_mut(&outcome.claim_id) else { return Vec::new() };
        for receipt in outcome.receipts {
            if case.receipts.len() < PALW_FALSE_VALID_RECEIPTS_PER_CLAIM_V1 && !case.receipts.contains(&receipt) {
                case.receipts.push(receipt);
            }
        }
        case.walked_daa = Some(current_daa);
        let mut queued_now = Vec::new();
        let (mut filed, mut waiting) = (false, false);
        for filing in &outcome.filings {
            let key = palw_false_valid_queue_key_v1(filing.offence_id);
            let in_queue = court_pending.iter().any(|(id, round, responder, _)| (*id, *round, *responder) == key);
            if filing.files() {
                filed = true;
                if in_queue {
                    continue;
                }
                let sends = case.sent.get(&filing.accused).map_or(0, |(_, n)| *n);
                if sends >= PALW_FALSE_VALID_MAX_SENDS_V1 {
                    warn!(
                        "[{PALW_PANEL}] claim {}: the false-Valid filing against {:?} is still convictable after {sends} carriers — \
                         given up (P2-8c)",
                        filing.claim_id, filing.accused
                    );
                    case.settled.insert(filing.accused);
                    continue;
                }
                if let Some(record) = false_valid_file_seam_v1(court_pending, filing, current_daa) {
                    info!(
                        "[{PALW_PANEL}] claim {}: filing PanelFalseValidV2 against the Valid signer {:?} ({:?} proof; offence {}, \
                         evidence {}) — ADR-0152 N10, P2-8c",
                        filing.claim_id, filing.accused, case.source, record.offence_key, record.evidence_id
                    );
                    case.sent.insert(filing.accused, (current_daa, sends + 1));
                    self.records.insert(record.offence_key, record.clone());
                    queued_now.push(record);
                }
                continue;
            }
            // Not (or no longer) a filing: whatever of it is still queued leaves the queue unsent.
            if in_queue {
                court_pending.retain(|(id, round, responder, _)| (*id, *round, *responder) != key);
            }
            if filing.settles() {
                if case.settled.insert(filing.accused) {
                    match (&filing.check, &filing.policy) {
                        (PalwFalseValidFilingCheckV1::ConvictedBefore { .. }, _) => info!(
                            "[{PALW_PANEL}] claim {}: {:?}'s false Valid is convicted on chain (P2-8c)",
                            filing.claim_id, filing.accused
                        ),
                        (check, policy) => info!(
                            "[{PALW_PANEL}] claim {}: not filing against {:?} — {check:?}{}",
                            filing.claim_id,
                            filing.accused,
                            policy.map(|p| format!(", node policy {p:?}")).unwrap_or_default()
                        ),
                    }
                }
            } else {
                waiting = true;
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

/// **One scan, on a blocking thread**: the claim's licences read off the accepted chain down to
/// `scan.walk_from_daa`, merged with the receipts already found, then every unskipped `Valid`
/// signer asked of the chain (`palw_false_valid_filing_check_v1`: adjudicator, gate, fold) through
/// `palw_false_valid_filings_v1`. A read that finds no tip state answers `Dormant`, which settles
/// nothing.
pub(crate) fn palw_false_valid_scan_v1(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    scan: PalwFalseValidScanV1,
    own: PalwBondKeyV2,
    current_daa: u64,
) -> PalwFalseValidScanOutcomeV1 {
    let mut receipts = scan.receipts;
    let span = current_daa.saturating_sub(scan.walk_from_daa).saturating_add(64).min(PALW_FALSE_VALID_MAX_WALK_BLOCKS_V1) as usize;
    crate::palw_panel::walk_accepted_lifecycle_objects_v1(consensus, scan.walk_from_daa, span, &mut |object| {
        if let Some((claim, found)) = palw_false_valid_receipts_of_licence_v1(&object)
            && claim == scan.claim_id
        {
            for receipt in found {
                if receipts.len() < PALW_FALSE_VALID_RECEIPTS_PER_CLAIM_V1 && !receipts.contains(&receipt) {
                    receipts.push(receipt);
                }
            }
        }
    });
    let filings = palw_false_valid_filings_v1(
        &scan.proof,
        &receipts,
        &own,
        |seat| scan.skip.contains(seat),
        |object| consensus.palw_false_valid_filing_check_v1(object.clone()).unwrap_or(PalwFalseValidFilingCheckV1::Dormant),
    );
    PalwFalseValidScanOutcomeV1 { claim_id: scan.claim_id, receipts, filings }
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

    fn receipt(seat: u64, mask: u32) -> PalwFalseValidReceiptV1 {
        PalwFalseValidReceiptV1::Segmented(PalwSeatReceiptV3 {
            receipt: PalwSeatReceiptV2 {
                claim: claim(),
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: bond(seat),
                signed_daa: 7,
                signature: vec![1; 4],
            },
            segments: PalwSegmentMaskV2(mask),
        })
    }

    /// A filing of `seat` whose chain answer is `check` — built by the ONE builder.
    fn filing(seat: u64, check: PalwFalseValidFilingCheckV1) -> PalwFalseValidFilingV1 {
        palw_false_valid_filings_v1(&proof(), &[receipt(seat, 0b1111)], &bond(99), |_| false, |_| check.clone()).remove(0)
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

    fn outcome(filings: Vec<PalwFalseValidFilingV1>) -> PalwFalseValidScanOutcomeV1 {
        PalwFalseValidScanOutcomeV1 { claim_id: claim(), receipts: vec![receipt(1, 0b1111)], filings }
    }

    fn filer_with_proof(daa: u64) -> PalwFalseValidFilerV1 {
        let mut filer = PalwFalseValidFilerV1::new(1_000, 500);
        assert!(filer.note_proof_v1(proof(), PalwFalseValidSourceV1::CaptureSample { leaf: 3 }, Some(40), daa));
        filer
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
    /// convicted settles it, still convictable re-queues it, at most `MAX_SENDS` times.
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
        // Convicted on chain: settled, and never asked again.
        queue.clear();
        let convicted = filing(1, PalwFalseValidFilingCheckV1::ConvictedBefore { offence_id: key });
        let at = later + PALW_FALSE_VALID_REFILE_DAA_V1;
        filer.due_v1(at, |_| None);
        assert!(filer.apply_v1(outcome(vec![convicted]), at, &mut queue).is_empty());
        let scans = filer.due_v1(at + PALW_FALSE_VALID_REWALK_DAA_V1, |_| None);
        assert!(scans[0].skip.contains(&bond(1)), "settled");
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

    /// A queued filing the chain no longer convicts (another carrier convicted the seat, or a court
    /// opened) leaves the queue unsent; an open court is asked again soon, not settled; what the
    /// liability rule or node policy declines is settled and never queued.
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
        let convicted = filing(2, PalwFalseValidFilingCheckV1::ConvictedBefore { offence_id: Hash64::default() });
        let mut held = filing(3, file());
        held.policy = Some(PalwFalseValidPolicyV1::HeldPartial);
        let not_liable = filing(4, PalwFalseValidFilingCheckV1::NotLiable(PalwOffenceVerifyError::SiteNotAttested));
        assert!(filer.apply_v1(outcome(vec![wait, convicted, held, not_liable]), at, &mut queue).is_empty());
        assert!(queue.is_empty(), "both queued entries left unsent");
        let scans = filer.due_v1(at + PALW_FALSE_VALID_WAIT_RECHECK_DAA_V1, |_| None);
        assert_eq!(scans.len(), 1, "an open court is asked again soon");
        assert!(!scans[0].skip.contains(&bond(1)));
        for settled in [2, 3, 4] {
            assert!(scans[0].skip.contains(&bond(settled)), "seat {settled} settled");
        }
    }

    /// Bounded: one proof a claim (the first kept), at most `MAX_CASES` (the oldest forgotten), one
    /// scan a tick, and a proof forgotten past its TTL.
    #[test]
    fn the_book_is_bounded() {
        let mut filer = PalwFalseValidFilerV1::new(1_000, 500);
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
    /// files. testnet-12 arms R-core+ at genesis; testnet-11 never.
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
