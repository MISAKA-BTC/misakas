//! **ADR-0152 v3.1 N10 / §7.3 P2-8c: the node's half of `PanelFalseValidV2`** — what an automatic
//! filer builds from a proof its own replay or capture found, which receipts it trusts, whom it
//! names, and the one question it asks the chain before it pays a carrier.
//!
//! Node policy, never validity. Nothing here is read by the processor's gate or by the fold: the
//! verdict is [`crate::palw_offence_attribution_v1::palw_check_panel_false_valid_v2`]'s (the ONE
//! adjudicator, which the gate runs with the signature and the fold without it), and whether the
//! fold then convicts is the fold's own — the processor's read (`palw_false_valid_filing_check_v1`
//! on `ConsensusApi`) runs the gate and folds the object on the tip, the way the licence assembler
//! asks `palw_v2_object_licenses_claim_v1`. This module classifies that answer, decides which
//! receipts a filer may spend its bounded reads on, and chooses which `Valid` signers it names:
//!
//! * **only a receipt the chain relied on takes a slot** ([`palw_false_valid_receipt_relied_v1`];
//!   the P2-8c review's high finding): a licence object rides on stateless admission and the gate
//!   may drop it while its carrier stays accepted, so the chain walk sees junk — garbage signatures
//!   over the assigned masks, strangers' bonds, a colluding seat's own receipts over a mask it was
//!   never locked for. A receipt is kept only if its signature verifies under the seat's
//!   registered key in the V2 or V3 message it was signed in, and the seat's `Valid` on the claim
//!   is one the chain holds: a lock in exactly this receipt's form, a liability row that lists
//!   it, or a conviction already standing. At most [`PALW_FALSE_VALID_RECEIPTS_PER_SEAT_V1`] a seat
//!   ([`palw_false_valid_admit_receipts_v1`]), so no seat's junk crowds out another's receipt;
//! * **the liability rule is the audit's** (SPEC §3.3 step 9, Q-6): a `Full` receipt always, a
//!   `Segmented` one by its (assigned) mask and the fault's site — the filer never restates it, it
//!   asks the adjudicator per receipt and files exactly the receipts it convicts
//!   ([`PalwFalseValidFilingCheckV1::File`]); a receipt the rule does not reach is
//!   [`PalwFalseValidFilingCheckV1::NotLiable`] and never named — on a held class as on the floor
//!   ([`PALW_FALSE_VALID_HELD_NAMES_PARTIALS_V1`]);
//! * **never against this node's own bond** — neither as the accused seat nor as the claim's
//!   producer, whom a finding that `acts_on_claim` charges S2/S3 through the void or the reversal.
//!
//! What a filer does with a `File` is the kaspad module's (`palw_filer_false_valid.rs`): one
//! carrier per (seat, claim) offence, queued through its seam, and — at integration — routed
//! through P2-8's commit–reveal reporter filer, for which [`PalwFalseValidFilingV1`] carries the
//! offence key, the evidence id and the accused.

use crate::palw_offence_attribution_v1::{
    PalwFalseValidReceiptV1, PalwFaultSiteV1, PalwIdentityRulesV1, PalwPanelFalseValidEvidenceV2, palw_check_panel_false_valid_v2,
    palw_false_valid_offence_id_v2,
};
use crate::palw_offence_v1::{PalwOffenceVerifyError, PalwPanelContradictionV1};
use crate::palw_panel_v2::PalwReceiptVerdictV2;
use crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2};
use crate::palw_verification_v2::PalwSegmentMaskV2;
use kaspa_hashes::Hash64;

/// **Whether the automatic filer names a PARTIAL-mask signer of a held-regime class** (one that
/// recorded its own step ladder, [`PalwChainStateV2::class_is_held_v1`]) where the adjudicator
/// convicts it. `true`: the filer follows the audit's liability rule on every class (the P2-8c
/// review's medium finding). The operator's decision of 2026-09-24 — partial-mask signers are not
/// bound at launch "by a dissection or by a DA default" (ADR-0152 §7.3's decisions, 1; IA-11 for a
/// DA held unit) — is already the adjudicator's on both of those routes: a dissection verdict is
/// voided producer-only or proves a `Whole` site, and a DA default restates `ProducerWithholding`,
/// whose site is `Whole`, so no partial seat is liable there. What reaches a partial holder is a
/// LOCATED step fault inside its own segment (a capture sample, an arithmetic court close), which
/// is exactly what that seat replayed and signed. `false` is the operator's one-line lever to
/// under-charge those too (never an over-charge); node policy only, the adjudicator is unchanged.
pub const PALW_FALSE_VALID_HELD_NAMES_PARTIALS_V1: bool = true;

/// **The most receipts of ONE seat a filing keeps and reads.** Per seat, not per claim (the P2-8c
/// review): with only relied-on receipts admitted, every receipt a seat has in a slot is one the
/// chain locked in that very form, so a second one adds only the other form (`Full` beside a
/// `Segmented` full mask); a seat's own extra signatures cannot take another seat's slot.
pub const PALW_FALSE_VALID_RECEIPTS_PER_SEAT_V1: usize = 2;

/// **A proof this node holds that a claim's committed work is false** — the contradiction its own
/// capture sample, court close or (P2-8b) replay bisection built, in the network's carriage (a step
/// refutation's prompt taken out and its one tile opened, `palw_refutation_prompt_carriage_v1`).
/// It names no seat: every relied-on `Valid` signer of the claim is tried against it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFalseValidProofV1 {
    pub claim_id: Hash64,
    pub contradiction: PalwPanelContradictionV1,
    pub prompt_ids_opening: Option<PalwPromptIdsOpeningV1>,
}

impl PalwFalseValidProofV1 {
    /// **A step refutation as a proof** — `StepArithmetic` in the carriage the caller already built
    /// (the capture sampler's and the court close's `palw_refutation_prompt_carriage_v1`).
    pub fn step_arithmetic_v1(
        claim_id: Hash64,
        refutation: &crate::palw_step_refute::PalwExecutionStepRefutationV1,
        operand_openings: &[crate::palw_artifact::PalwArtifactOpeningV1],
        prompt_ids_opening: Option<&PalwPromptIdsOpeningV1>,
    ) -> Self {
        Self {
            claim_id,
            contradiction: PalwPanelContradictionV1::StepArithmetic {
                refutation: refutation.clone(),
                operand_openings: operand_openings.to_vec(),
            },
            prompt_ids_opening: prompt_ids_opening.cloned(),
        }
    }

    /// **A court close's arithmetic refutation as a proof** — either prompt carriage (`Arithmetic`
    /// with the ids, `ArithmeticOpened` with the one tile), exactly as the close carried it. Every
    /// other close form (a decode token, a fused-attention dissection) proves nothing a kind-3
    /// contradiction carries: `None`, and the court's own void is what those leave behind.
    pub fn of_court_close_v1(claim_id: Hash64, proof: &crate::palw_court_v2::PalwCourtVerdictProofV2) -> Option<Self> {
        use crate::palw_court_v2::PalwCourtVerdictProofV2 as P;
        match proof {
            P::Arithmetic { refutation, operand_openings } => {
                Some(Self::step_arithmetic_v1(claim_id, refutation, operand_openings, None))
            }
            P::ArithmeticOpened { refutation, operand_openings, prompt_ids_opening } => {
                Some(Self::step_arithmetic_v1(claim_id, refutation, operand_openings, Some(prompt_ids_opening)))
            }
            _ => None,
        }
    }
}

/// **What filing one `PanelFalseValidV2` comes to at the tip** — the chain's answer, read before a
/// carrier is paid (plan §5.3: only a conviction repays the filer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwFalseValidFilingCheckV1 {
    /// File it: the receipt is one the chain relied on, the adjudicator convicts it, the gate
    /// admits the object (signature included) and the fold convicts on the tip. The facts node
    /// policy reads beside it: the claim's producer (a finding that `acts_on_claim` charges it), the
    /// fault's site, whether the claim's class is held and whether the receipt attested the job.
    File {
        claim_id: Hash64,
        offence_id: Hash64,
        producer: PalwBondKeyV2,
        site: PalwFaultSiteV1,
        acts_on_claim: bool,
        class_held: bool,
        full_attestation: bool,
    },
    /// This (seat, claim) is convicted already — by this node's carrier or anyone's — in the block
    /// at `accepted_daa`. The fold would carry a second one as a no-op and the gate refuses it; a
    /// filer keeps asking until that block is past a finality depth, since a reorg can take the
    /// conviction away.
    ConvictedBefore { offence_id: Hash64, accepted_daa: u64 },
    /// The audit's liability rule does not reach this receipt (`SiteNotAttested`,
    /// `SegmentMaskNotAssigned`, `SegmentsUnknown`): its signer is not named on it. A property of
    /// the receipt and the proof, not of the seat — another receipt of the seat is asked afresh.
    NotLiable(PalwOffenceVerifyError),
    /// A court is open on the claim (`ClaimUnderSession`): the conviction waits for its close.
    Wait(PalwOffenceVerifyError),
    /// The receipt is not one the chain relied on ([`palw_false_valid_receipt_relied_v1`]): junk a
    /// dropped licence object carried, or a `Valid` whose lock is not (or no longer) in state.
    /// Never filed, and says nothing about the seat.
    Unrelied(String),
    /// Anything else the adjudicator, the gate or the fold refuses, with its reason. Never filed; a
    /// state-dependent refusal can clear, so the filer asks again at its next walk.
    Refused(String),
    /// Below `Params::palw_rcore_plus` (or off `ConsensusV2`): the filer does not run.
    Dormant,
}

/// **Whether the chain relied on `receipt`** — the admission a filer runs before a receipt it read
/// off the chain may take a slot or be asked about (the P2-8c review's high finding). In order,
/// cheapest first:
///
/// 1. it is a `Valid`;
/// 2. the seat's `Valid` on the claim is in state in this receipt's form: a conviction of the
///    (seat, claim) stands (so a filer keeps watching it through a reorg); or the seat holds its
///    lock and the lock's attested mask is this receipt's — the full cut for `Full`, its own mask
///    for `Segmented` (S-3 writes them so; a lock that recorded no cut, `segments == 0`, pins no
///    form); or, with no lock, the claim's liability row lists the seat — the rows the fold's
///    kind-3 consumer charges (`convict_false_valid_rcore_v1`);
/// 3. its signature verifies under the seat's REGISTERED key and the chain's domain in the V2 or
///    V3 message and context its form was signed in ([`PalwFalseValidReceiptV1::signed_message_v1`],
///    the adjudicator's step 4 and so the gate's).
///
/// This is not the verdict — the adjudicator, the gate and the fold still decide every filing. It
/// is what keeps a carrier-fee's worth of junk from taking the reads a genuine receipt needs.
pub fn palw_false_valid_receipt_relied_v1(
    state: &PalwChainStateV2,
    receipt: &PalwFalseValidReceiptV1,
    chain_domain: Hash64,
    verify: &dyn Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> Result<(), &'static str> {
    let inner = receipt.inner();
    if inner.verdict != PalwReceiptVerdictV2::Valid {
        return Err("not a Valid receipt");
    }
    let (seat, claim) = (inner.seat_bond, inner.claim);
    let relied = state.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &claim)).is_some()
        || match state.slashable_lock(seat, claim) {
            Some(lock) => {
                let attested = match receipt {
                    PalwFalseValidReceiptV1::Full(_) => PalwSegmentMaskV2::full(lock.segments),
                    PalwFalseValidReceiptV1::Segmented(signed) => signed.segments,
                };
                lock.segments == 0 || attested == lock.attested
            }
            None => state.panel_liability(&claim).is_some_and(|row| row.valid_signers.iter().any(|(s, _)| *s == seat.0)),
        };
    if !relied {
        return Err("the chain holds no Valid of this seat on the claim in this receipt's form (no lock over its mask, no \
                    liability row, no conviction)");
    }
    let Some(bond) = state.bond(&seat) else { return Err("the seat is not a bond on this chain") };
    let (message, context) = receipt.signed_message_v1(chain_domain);
    if !verify(&bond.pubkey, message.as_byte_slice(), &inner.signature, context) {
        return Err("the receipt's signature does not verify under the seat's registered key");
    }
    Ok(())
}

/// **Admit `offered` receipts of `claim_id` into `kept`** — only those `relied` answers `true` for
/// (the processor's [`palw_false_valid_receipt_relied_v1`], asked for the whole batch at once), at
/// most [`PALW_FALSE_VALID_RECEIPTS_PER_SEAT_V1`] a seat, first come, each once. A receipt naming
/// another claim, not `Valid`, already kept, or of a seat whose slots are full is not asked about.
/// `relied` answering short (a dormant read) admits nothing. Returns how many were admitted.
pub fn palw_false_valid_admit_receipts_v1(
    kept: &mut Vec<PalwFalseValidReceiptV1>,
    claim_id: Hash64,
    offered: impl IntoIterator<Item = PalwFalseValidReceiptV1>,
    relied: impl FnOnce(&[PalwFalseValidReceiptV1]) -> Vec<bool>,
) -> usize {
    let held = |kept: &[PalwFalseValidReceiptV1], seat: &PalwBondKeyV2| kept.iter().filter(|r| r.inner().seat_bond == *seat).count();
    let mut candidates: Vec<PalwFalseValidReceiptV1> = Vec::new();
    for receipt in offered {
        let inner = receipt.inner();
        if inner.claim != claim_id
            || inner.verdict != PalwReceiptVerdictV2::Valid
            || held(kept, &inner.seat_bond) >= PALW_FALSE_VALID_RECEIPTS_PER_SEAT_V1
            || kept.contains(&receipt)
            || candidates.contains(&receipt)
        {
            continue;
        }
        candidates.push(receipt);
    }
    if candidates.is_empty() {
        return 0;
    }
    let answers = relied(&candidates);
    let mut admitted = 0;
    for (receipt, ok) in candidates.into_iter().zip(answers) {
        if ok && held(kept, &receipt.inner().seat_bond) < PALW_FALSE_VALID_RECEIPTS_PER_SEAT_V1 {
            kept.push(receipt);
            admitted += 1;
        }
    }
    admitted
}

/// **The fold's half of the question, classified** — after the (seat, claim) ledger key the gate
/// and the fold both read first, the receipt's admission ([`palw_false_valid_receipt_relied_v1`],
/// so junk is `Unrelied` before the liability rule can call it `NotLiable`), then the ONE
/// adjudicator on `state` (the processor's read adds the gate and the fold). `rules` and
/// `fp_decode_rules_active` are the ones the gate and the fold read at the block the carrier is
/// expected in; `chain_domain` and `verify` the gate's.
pub fn palw_false_valid_filing_check_v1(
    state: &PalwChainStateV2,
    accused: &PalwBondKeyV2,
    evidence: &[u8],
    fp_decode_rules_active: bool,
    rules: PalwIdentityRulesV1,
    chain_domain: Hash64,
    verify: &dyn Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> PalwFalseValidFilingCheckV1 {
    use PalwFalseValidFilingCheckV1 as Check;
    use PalwOffenceVerifyError as E;
    let Ok(payload) = borsh::from_slice::<PalwPanelFalseValidEvidenceV2>(evidence) else {
        return Check::Refused(E::PanelFalseValidNeedsContradiction.to_string());
    };
    let offence_id = palw_false_valid_offence_id_v2(&accused.0, &payload.claim_id);
    if let Some(row) = state.consumed_offence(&offence_id) {
        return Check::ConvictedBefore { offence_id, accepted_daa: row.accepted_daa };
    }
    if let Err(why) = palw_false_valid_receipt_relied_v1(state, &payload.receipt, chain_domain, verify) {
        return Check::Unrelied(why.to_string());
    }
    // F7's slot stays empty on testnet-12, as the gate and the fold both pass it.
    match palw_check_panel_false_valid_v2(state, accused, evidence, fp_decode_rules_active, false, rules, None) {
        Ok(finding) => {
            let full_attestation = match &payload.receipt {
                PalwFalseValidReceiptV1::Full(_) => true,
                PalwFalseValidReceiptV1::Segmented(signed) => finding.target.segment_count.is_some_and(|k| signed.segments.is_full(k)),
            };
            Check::File {
                claim_id: finding.target.claim_id,
                offence_id,
                producer: finding.target.executor_bond,
                site: finding.site,
                acts_on_claim: finding.acts_on_claim,
                class_held: state.class_is_held_v1(&finding.target.class_id),
                full_attestation,
            }
        }
        Err(e @ (E::SiteNotAttested | E::SegmentMaskNotAssigned | E::SegmentsUnknown)) => Check::NotLiable(e),
        Err(e @ E::ClaimUnderSession) => Check::Wait(e),
        Err(e) => Check::Refused(e.to_string()),
    }
}

/// **Why node policy does not send a filing the chain would take.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFalseValidPolicyV1 {
    /// The claim is this node's own: a conviction that acts on it charges this node's bond as its
    /// producer.
    OwnClaim,
    /// A partial-mask signer of a held-regime class, where [`PALW_FALSE_VALID_HELD_NAMES_PARTIALS_V1`]
    /// is `false`.
    HeldPartial,
}

/// **Node policy on a `File`** — `own` the node's bond, `held_names_partials` the
/// [`PALW_FALSE_VALID_HELD_NAMES_PARTIALS_V1`] lever (a parameter so both settings are tested).
pub fn palw_false_valid_policy_v1(
    check: &PalwFalseValidFilingCheckV1,
    own: &PalwBondKeyV2,
    held_names_partials: bool,
) -> Option<PalwFalseValidPolicyV1> {
    match check {
        PalwFalseValidFilingCheckV1::File { producer, .. } if producer == own => Some(PalwFalseValidPolicyV1::OwnClaim),
        PalwFalseValidFilingCheckV1::File { class_held: true, full_attestation: false, .. } if !held_names_partials => {
            Some(PalwFalseValidPolicyV1::HeldPartial)
        }
        _ => None,
    }
}

/// **One `Valid` signer of the claim, as the filer judged it**: the object it would file (built
/// whether or not it is sent, so a caller can log or test exactly what was asked), the chain's
/// answer and node policy's. What P2-8's reporter filer needs rides with it — the offence key its
/// commitment is keyed to, the `evidence_id` the commitment binds (N12), the accused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFalseValidFilingV1 {
    pub accused: PalwBondKeyV2,
    pub claim_id: Hash64,
    /// `palw_false_valid_offence_id_v2(accused, claim)` — one per (seat, claim).
    pub offence_id: Hash64,
    pub evidence_id: Hash64,
    pub object: PalwConsensusObjectV2,
    pub check: PalwFalseValidFilingCheckV1,
    pub policy: Option<PalwFalseValidPolicyV1>,
    /// The `evidence_id`s of this seat's receipts the liability rule did not reach in this pass
    /// (the last answer's included, when it is `NotLiable`): a filer records them per receipt and
    /// never asks them again, while a new receipt of the seat is asked afresh.
    pub not_liable: Vec<Hash64>,
}

impl PalwFalseValidFilingV1 {
    /// Send it: the chain convicts and node policy agrees.
    pub fn files(&self) -> bool {
        matches!(self.check, PalwFalseValidFilingCheckV1::File { .. }) && self.policy.is_none()
    }

    /// Node policy declined a filing the chain would take — the one answer that settles the seat
    /// for the proof's life (it is about the claim and the seat's attestation, which no later
    /// receipt or block changes).
    pub fn declined(&self) -> bool {
        matches!(self.check, PalwFalseValidFilingCheckV1::File { .. }) && self.policy.is_some()
    }
}

/// **The filings a proof makes of a claim's relied-on `Valid` signers** — one entry per seat asked,
/// in the order the receipts name them. For each seat that is not this node's (`own`) and not
/// `skip_seat` (settled, or a carrier in flight), each of its receipts in turn — at most
/// [`PALW_FALSE_VALID_RECEIPTS_PER_SEAT_V1`], and none whose `evidence_id` is `skip_evidence`
/// (answered `NotLiable` before) — is built into the evidence
/// ([`PalwPanelFalseValidEvidenceV2::filed_v2`]) and asked of the chain (`check`); the first that
/// files, is convicted already or is declined by policy is the seat's entry, else its last answer.
/// A seat the liability rule does not reach is therefore never named, and a seat named once is
/// named under one (seat, claim) key whatever form its receipts took.
///
/// Receipts naming another claim, or not `Valid`, are passed over.
pub fn palw_false_valid_filings_v1(
    proof: &PalwFalseValidProofV1,
    receipts: &[PalwFalseValidReceiptV1],
    own: &PalwBondKeyV2,
    skip_seat: impl Fn(&PalwBondKeyV2) -> bool,
    skip_evidence: impl Fn(&Hash64) -> bool,
    mut check: impl FnMut(&PalwConsensusObjectV2) -> PalwFalseValidFilingCheckV1,
) -> Vec<PalwFalseValidFilingV1> {
    let read: Vec<&PalwFalseValidReceiptV1> =
        receipts.iter().filter(|r| r.inner().claim == proof.claim_id && r.inner().verdict == PalwReceiptVerdictV2::Valid).collect();
    let mut seats: Vec<PalwBondKeyV2> = Vec::new();
    for receipt in &read {
        let seat = receipt.inner().seat_bond;
        if seat != *own && !skip_seat(&seat) && !seats.contains(&seat) {
            seats.push(seat);
        }
    }
    let mut out = Vec::with_capacity(seats.len());
    for seat in seats {
        let mut last: Option<PalwFalseValidFilingV1> = None;
        let mut not_liable = Vec::new();
        for receipt in read.iter().filter(|r| r.inner().seat_bond == seat).take(PALW_FALSE_VALID_RECEIPTS_PER_SEAT_V1) {
            let evidence = PalwPanelFalseValidEvidenceV2::filed_v2(
                proof.claim_id,
                (*receipt).clone(),
                proof.contradiction.clone(),
                proof.prompt_ids_opening.clone(),
            );
            let object = evidence.object_v2();
            let PalwConsensusObjectV2::ObjectiveOffence { evidence_id, .. } = &object else {
                unreachable!("object_v2 builds an ObjectiveOffence")
            };
            let evidence_id = *evidence_id;
            if skip_evidence(&evidence_id) {
                continue;
            }
            let answer = check(&object);
            if matches!(answer, PalwFalseValidFilingCheckV1::NotLiable(_)) {
                not_liable.push(evidence_id);
            }
            let filing = PalwFalseValidFilingV1 {
                accused: seat,
                claim_id: proof.claim_id,
                offence_id: palw_false_valid_offence_id_v2(&seat.0, &proof.claim_id),
                evidence_id,
                policy: palw_false_valid_policy_v1(&answer, own, PALW_FALSE_VALID_HELD_NAMES_PARTIALS_V1),
                object,
                check: answer,
                not_liable: Vec::new(),
            };
            let done =
                filing.files() || filing.declined() || matches!(filing.check, PalwFalseValidFilingCheckV1::ConvictedBefore { .. });
            last = Some(filing);
            if done {
                break;
            }
        }
        if let Some(mut filing) = last {
            filing.not_liable = not_liable;
            out.push(filing);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_offence_attribution_v1::{PalwFalseValidReceiptV1, palw_false_valid_receipts_of_licence_v1};
    use crate::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3};
    use crate::palw_verification_v2::PalwSegmentMaskV2;
    use crate::tx::TransactionOutpoint;

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(0xB0_0000 + n), index: 0 })
    }

    fn v2(seat: u64, claim: Hash64, verdict: PalwReceiptVerdictV2) -> PalwSeatReceiptV2 {
        PalwSeatReceiptV2 { claim, verdict, seat_bond: bond(seat), signed_daa: 7, signature: vec![seat as u8; 4] }
    }

    fn v3(seat: u64, claim: Hash64, mask: u32) -> PalwFalseValidReceiptV1 {
        PalwFalseValidReceiptV1::Segmented(PalwSeatReceiptV3 {
            receipt: v2(seat, claim, PalwReceiptVerdictV2::Valid),
            segments: PalwSegmentMaskV2(mask),
        })
    }

    fn proof(claim: Hash64) -> PalwFalseValidProofV1 {
        PalwFalseValidProofV1 {
            claim_id: claim,
            contradiction: PalwPanelContradictionV1::CourtFraud { voided_daa: 9 },
            prompt_ids_opening: None,
        }
    }

    fn file(producer: PalwBondKeyV2, class_held: bool, full_attestation: bool) -> PalwFalseValidFilingCheckV1 {
        PalwFalseValidFilingCheckV1::File {
            claim_id: Hash64::from_u64_word(1),
            offence_id: Hash64::from_u64_word(2),
            producer,
            site: PalwFaultSiteV1::Whole,
            acts_on_claim: true,
            class_held,
            full_attestation,
        }
    }

    fn accused_of(object: &PalwConsensusObjectV2) -> PalwBondKeyV2 {
        let PalwConsensusObjectV2::ObjectiveOffence { accused, .. } = object else { unreachable!() };
        *accused
    }

    /// The evidence names the receipt's seat, leaves F7's slot empty, and the object's id is the
    /// digest of exactly those bytes — the id R-3's commitment binds.
    #[test]
    fn the_evidence_names_its_receipts_seat_and_its_object_carries_its_digest() {
        let claim = Hash64::from_u64_word(0xC1);
        let receipt = v3(3, claim, 0b0100);
        let evidence = PalwPanelFalseValidEvidenceV2::filed_v2(claim, receipt.clone(), proof(claim).contradiction, None);
        assert_eq!(evidence.accused_seat, bond(3).0);
        assert_eq!(evidence.version, crate::palw_offence_attribution_v1::PALW_PANEL_FALSE_VALID_VERSION_V2);
        assert!(evidence.reporter_reveal.is_empty(), "F7's slot is empty on testnet-12");
        let PalwConsensusObjectV2::ObjectiveOffence { kind, accused, evidence_id, evidence: bytes } = evidence.object_v2() else {
            panic!("an objective offence")
        };
        assert_eq!((kind, accused), (crate::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2, bond(3)));
        assert_eq!(evidence_id, crate::palw_offence_v1::palw_offence_evidence_digest_v1(&bytes));
        assert_eq!(borsh::from_slice::<PalwPanelFalseValidEvidenceV2>(&bytes).unwrap(), evidence);
    }

    /// C-3: a V1 licence's receipts are `Full`, a Verification V2 or S2 licence's `Segmented` with
    /// their masks — the form each seat signed — and only `Valid` receipts of the licence's claim.
    #[test]
    fn a_licences_receipts_keep_the_form_they_were_signed_in() {
        let claim = Hash64::from_u64_word(0xC2);
        let other = Hash64::from_u64_word(0xC3);
        let v1 = PalwConsensusObjectV2::ReceiptLicensed {
            claim,
            receipts: vec![
                v2(1, claim, PalwReceiptVerdictV2::Valid),
                v2(2, claim, PalwReceiptVerdictV2::Sampled),
                v2(3, other, PalwReceiptVerdictV2::Valid),
            ],
        };
        assert_eq!(
            palw_false_valid_receipts_of_licence_v1(&v1),
            Some((claim, vec![PalwFalseValidReceiptV1::Full(v2(1, claim, PalwReceiptVerdictV2::Valid))]))
        );
        let segmented = |receipts: Vec<PalwSeatReceiptV3>| {
            [
                PalwConsensusObjectV2::ReceiptLicensedV2 { claim, receipts: receipts.clone() },
                PalwConsensusObjectV2::OptimisticLicensed { claim, receipts },
            ]
        };
        let PalwFalseValidReceiptV1::Segmented(kept) = v3(4, claim, 0b0010) else { unreachable!() };
        let sampled = PalwSeatReceiptV3 { receipt: v2(5, claim, PalwReceiptVerdictV2::Sampled), segments: PalwSegmentMaskV2(0b0001) };
        for object in segmented(vec![kept.clone(), sampled]) {
            assert_eq!(
                palw_false_valid_receipts_of_licence_v1(&object),
                Some((claim, vec![PalwFalseValidReceiptV1::Segmented(kept.clone())]))
            );
        }
        let defaulted = PalwConsensusObjectV2::ProducerDefaulted { claim, receipts: vec![v2(6, claim, PalwReceiptVerdictV2::Valid)] };
        assert_eq!(palw_false_valid_receipts_of_licence_v1(&defaulted), None, "not a licence: no Valid it carries was locked");
    }

    /// **The review's high finding, the admission half**: only receipts `relied` accepts take a
    /// slot, at most `PER_SEAT` a seat — so 32 junk receipts of one seat (or of strangers) offered
    /// ahead of a genuine one never hide another seat's receipt, and the genuine receipt of a seat
    /// whose junk was refused still takes its slot. Nothing is asked twice; another claim's receipts
    /// and non-`Valid` ones are never asked; a dormant read (answering short) admits nothing.
    #[test]
    fn only_relied_on_receipts_take_a_slot_and_the_cap_is_per_seat() {
        let claim = Hash64::from_u64_word(0xCA);
        let genuine: Vec<PalwFalseValidReceiptV1> = (1..=5).map(|s| v3(s, claim, 1 << (s - 1))).collect();
        let junk = |seat: u64, n: u64| {
            let mut r = v3(seat, claim, 0b1111);
            let PalwFalseValidReceiptV1::Segmented(signed) = &mut r else { unreachable!() };
            signed.receipt.signed_daa = 1_000 + n;
            signed.receipt.signature = vec![0; 4];
            r
        };
        let is_genuine = |r: &PalwFalseValidReceiptV1| genuine.contains(r);
        let mut asked: Vec<PalwFalseValidReceiptV1> = Vec::new();
        let mut kept = Vec::new();
        // A junk licence ahead of the genuine one, 32 junk receipts of real seats and strangers after.
        let mut offered: Vec<PalwFalseValidReceiptV1> = (1..=5).map(|s| junk(s, 0)).collect();
        offered.extend(genuine.iter().cloned());
        offered.extend((0..32).map(|n| junk(1 + n % 5, n + 1)));
        offered.extend((0..32).map(|n| junk(100 + n, 0)));
        offered.push(v3(1, Hash64::from_u64_word(0xCB), 0b0001));
        offered.push(PalwFalseValidReceiptV1::Full(v2(2, claim, PalwReceiptVerdictV2::Sampled)));
        let admitted = palw_false_valid_admit_receipts_v1(&mut kept, claim, offered.clone(), |batch| {
            asked.extend(batch.iter().cloned());
            batch.iter().map(is_genuine).collect()
        });
        assert_eq!(admitted, 5);
        assert_eq!(kept, genuine, "every genuine receipt kept, no junk");
        assert!(asked.iter().all(|r| r.inner().claim == claim && r.inner().verdict == PalwReceiptVerdictV2::Valid));
        // Offered again (a re-walk): nothing new is asked about the kept ones.
        asked.clear();
        palw_false_valid_admit_receipts_v1(&mut kept, claim, genuine.clone(), |batch| {
            asked.extend(batch.iter().cloned());
            vec![true; batch.len()]
        });
        assert!(asked.is_empty(), "a kept receipt is not asked again");
        // A seat's slots are per seat: seat 1 fills its two, seat 2 still gets one.
        let mut kept = Vec::new();
        let many: Vec<PalwFalseValidReceiptV1> = (0..8).map(|n| junk(1, n)).chain(std::iter::once(genuine[1].clone())).collect();
        palw_false_valid_admit_receipts_v1(&mut kept, claim, many, |batch| vec![true; batch.len()]);
        assert_eq!(kept.iter().filter(|r| r.inner().seat_bond == bond(1)).count(), PALW_FALSE_VALID_RECEIPTS_PER_SEAT_V1);
        assert!(kept.contains(&genuine[1]), "one seat's receipts never take another seat's slot");
        let mut dormant = Vec::new();
        assert_eq!(palw_false_valid_admit_receipts_v1(&mut dormant, claim, genuine.clone(), |_| Vec::new()), 0);
        assert!(dormant.is_empty());
    }

    /// One entry per seat; this node's own receipt is never built into anything; a skipped seat is
    /// not asked; the first receipt of a seat that files wins over its other forms; a liability
    /// refusal is recorded per receipt, and a receipt answered before is not asked again.
    #[test]
    fn one_filing_per_seat_never_the_own_bond_and_only_what_the_chain_convicts() {
        let claim = Hash64::from_u64_word(0xC4);
        let own = bond(9);
        let producer = bond(0);
        let receipts = vec![
            v3(1, claim, 0b1111),
            v3(2, claim, 0b0001),
            PalwFalseValidReceiptV1::Full(v2(2, claim, PalwReceiptVerdictV2::Valid)),
            v3(9, claim, 0b0010),
            v3(3, claim, 0b0100),
            v3(4, claim, 0b1000),
        ];
        let mut asked: Vec<(PalwBondKeyV2, bool)> = Vec::new();
        let answer = |object: &PalwConsensusObjectV2| {
            let PalwConsensusObjectV2::ObjectiveOffence { accused, evidence, .. } = object else { unreachable!() };
            let payload: PalwPanelFalseValidEvidenceV2 = borsh::from_slice(evidence).unwrap();
            let full = matches!(payload.receipt, PalwFalseValidReceiptV1::Full(_));
            let check = match (accused, full) {
                (a, _) if *a == bond(1) => file(producer, false, true),
                (a, false) if *a == bond(2) => PalwFalseValidFilingCheckV1::NotLiable(PalwOffenceVerifyError::SiteNotAttested),
                (a, true) if *a == bond(2) => file(producer, false, true),
                _ => PalwFalseValidFilingCheckV1::NotLiable(PalwOffenceVerifyError::SiteNotAttested),
            };
            (*accused, full, check)
        };
        let filings = palw_false_valid_filings_v1(
            &proof(claim),
            &receipts,
            &own,
            |seat| *seat == bond(4),
            |_| false,
            |object| {
                let (accused, full, check) = answer(object);
                asked.push((accused, full));
                check
            },
        );
        assert!(asked.iter().all(|(seat, _)| *seat != own && *seat != bond(4)), "never the own bond, never a skipped seat: {asked:?}");
        assert_eq!(asked, vec![(bond(1), false), (bond(2), false), (bond(2), true), (bond(3), false)]);
        let summary: Vec<(PalwBondKeyV2, bool)> = filings.iter().map(|f| (f.accused, f.files())).collect();
        assert_eq!(summary, vec![(bond(1), true), (bond(2), true), (bond(3), false)]);
        for filing in &filings {
            assert_eq!(filing.offence_id, palw_false_valid_offence_id_v2(&filing.accused.0, &claim));
            let PalwConsensusObjectV2::ObjectiveOffence { evidence_id, accused, .. } = &filing.object else { unreachable!() };
            assert_eq!((*evidence_id, *accused), (filing.evidence_id, filing.accused));
        }
        assert_eq!(filings[1].not_liable.len(), 1, "seat 2's segmented receipt is recorded not liable, its Full one files");
        assert_eq!(filings[2].not_liable, vec![filings[2].evidence_id], "seat 3's one receipt, per receipt");
        assert!(!filings[2].declined(), "not liable is not a policy decline: nothing settles the seat");
        // Asked again with seat 3's answered receipt skipped: seat 3 has nothing left to ask.
        let answered = filings[2].evidence_id;
        let again = palw_false_valid_filings_v1(&proof(claim), &receipts, &own, |_| false, |id| *id == answered, |o| answer(o).2);
        assert!(again.iter().all(|f| f.accused != bond(3)), "a receipt answered not liable is not asked again");
    }

    /// Node policy: a claim this node produced is never filed on (the finding would charge its own
    /// bond as the producer); a held class's partial-mask signer IS named where the adjudicator
    /// convicts it (the lever at `true`, the review's medium finding) and is declined only at
    /// `false`; an open court is waited on, not settled.
    #[test]
    fn policy_declines_the_own_claim_and_names_held_partials_the_rule_reaches() {
        let claim = Hash64::from_u64_word(0xC5);
        let own = bond(9);
        let receipts = vec![v3(1, claim, 0b1111), v3(2, claim, 0b0001), v3(3, claim, 0b0010)];
        let filings = palw_false_valid_filings_v1(
            &proof(claim),
            &receipts,
            &own,
            |_| false,
            |_| false,
            |object| {
                let accused = accused_of(object);
                if accused == bond(1) {
                    file(bond(0), true, true)
                } else if accused == bond(2) {
                    file(bond(0), true, false)
                } else {
                    PalwFalseValidFilingCheckV1::Wait(PalwOffenceVerifyError::ClaimUnderSession)
                }
            },
        );
        assert!(PALW_FALSE_VALID_HELD_NAMES_PARTIALS_V1, "the filer follows the adjudicator on held classes");
        assert!(filings[0].files(), "a held class's full-mask signer is named");
        assert!(filings[1].files() && filings[1].policy.is_none(), "and so is a partial holder the rule reaches");
        assert!(matches!(filings[2].check, PalwFalseValidFilingCheckV1::Wait(_)) && !filings[2].declined());
        // The lever at `false`: the held partial is declined (an under-charge), the full seat is not.
        assert_eq!(palw_false_valid_policy_v1(&file(bond(0), true, false), &own, false), Some(PalwFalseValidPolicyV1::HeldPartial));
        assert_eq!(palw_false_valid_policy_v1(&file(bond(0), true, true), &own, false), None);
        assert_eq!(palw_false_valid_policy_v1(&file(bond(0), false, false), &own, false), None, "the floor is never declined");
        let own_claim =
            palw_false_valid_filings_v1(&proof(claim), &receipts[..1], &own, |_| false, |_| false, |_| file(own, false, true));
        assert_eq!(own_claim[0].policy, Some(PalwFalseValidPolicyV1::OwnClaim));
        assert!(!own_claim[0].files() && own_claim[0].declined());
    }
}
