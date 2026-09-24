//! **ADR-0152 v3.1 N10 / §7.3 P2-8c: the node's half of `PanelFalseValidV2`** — what an automatic
//! filer builds from a proof its own replay or capture found, whom it names, and the one question it
//! asks the chain before it pays a carrier.
//!
//! Node policy, never validity. Nothing here is read by the processor's gate or by the fold: the
//! verdict is [`crate::palw_offence_attribution_v1::palw_check_panel_false_valid_v2`]'s (the ONE
//! adjudicator, which the gate runs with the signature and the fold without it), and whether the
//! fold then convicts is the fold's own — the processor's read (`palw_false_valid_filing_check_v1`
//! on `ConsensusApi`) runs the gate and folds the object on the tip, the way the licence assembler
//! asks `palw_v2_object_licenses_claim_v1`. This module only classifies that answer and chooses
//! which of a licence's `Valid` signers a filer names:
//!
//! * **the liability rule is the audit's** (SPEC §3.3 step 9, Q-6): a `Full` receipt always, a
//!   `Segmented` one by its (assigned) mask and the fault's site — the filer never restates it, it
//!   asks the adjudicator per signer and files exactly the receipts it convicts
//!   ([`PalwFalseValidFilingCheckV1::File`]); a signer the rule does not reach is
//!   [`PalwFalseValidFilingCheckV1::NotLiable`] and is never named;
//! * **a held-regime class names full masks only** ([`PALW_FALSE_VALID_HELD_NAMES_PARTIALS_V1`]): the
//!   operator's decision that partial-mask signers are not bound at launch for held units (IA-11,
//!   §3.9 "no `site_leaf`"), applied to the filer as an under-charge, never an over-charge;
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
use crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2};
use kaspa_hashes::Hash64;

/// **Whether the automatic filer names a PARTIAL-mask signer of a held-regime class** (one that
/// recorded its own step ladder, [`PalwChainStateV2::class_is_held_v1`]). `false` at launch: the
/// operator decided partial-mask signers are not bound for held units (ADR-0152 IA-11, §3.9's
/// "partial-mask signers are not bound at launch, so there is no `site_leaf`"), and the filer
/// applies that decision to kind 3 as well — a colluding partial seat of an 8k/2M claim is
/// under-charged, an honest one is never over-charged. Full-mask signers (a `Full` receipt, or a
/// `Segmented` one over the full mask) are named on every class. A one-line flip, node policy only:
/// the adjudicator's rule is unchanged either way.
pub const PALW_FALSE_VALID_HELD_NAMES_PARTIALS_V1: bool = false;

/// **The most licence receipts one claim's filing reads.** A panel seats five; a supplementary
/// receipt and a second licence form add at most one a seat. A licence walk that finds more is
/// reading junk it did not ask for, and the rest are not tried.
pub const PALW_FALSE_VALID_RECEIPTS_PER_CLAIM_V1: usize = 32;

/// **A proof this node holds that a claim's committed work is false** — the contradiction its own
/// capture sample, court close or (P2-8b) replay bisection built, in the network's carriage (a step
/// refutation's prompt taken out and its one tile opened, `palw_refutation_prompt_carriage_v1`).
/// It names no seat: every `Valid` signer of the claim is tried against it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFalseValidProofV1 {
    pub claim_id: Hash64,
    pub contradiction: PalwPanelContradictionV1,
    pub prompt_ids_opening: Option<PalwPromptIdsOpeningV1>,
}

/// **What filing one `PanelFalseValidV2` comes to at the tip** — the chain's answer, read before a
/// carrier is paid (plan §5.3: only a conviction repays the filer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwFalseValidFilingCheckV1 {
    /// File it: the adjudicator convicts this receipt, the gate admits the object (signature
    /// included) and the fold convicts on the tip. The facts node policy reads beside it: the
    /// claim's producer (a finding that `acts_on_claim` charges it), the fault's site, whether the
    /// claim's class is held and whether the receipt attested the whole job.
    File {
        claim_id: Hash64,
        offence_id: Hash64,
        producer: PalwBondKeyV2,
        site: PalwFaultSiteV1,
        acts_on_claim: bool,
        class_held: bool,
        full_attestation: bool,
    },
    /// This (seat, claim) is convicted already — by this node's carrier or anyone's. The fold would
    /// carry a second one as a no-op and the gate refuses it.
    ConvictedBefore { offence_id: Hash64 },
    /// The audit's liability rule does not reach this receipt (`SiteNotAttested`,
    /// `SegmentMaskNotAssigned`, `SegmentsUnknown`): the signer is never named.
    NotLiable(PalwOffenceVerifyError),
    /// A court is open on the claim (`ClaimUnderSession`): the conviction waits for its close.
    Wait(PalwOffenceVerifyError),
    /// Anything else the adjudicator, the gate or the fold refuses, with its reason. Never filed.
    Refused(String),
    /// Below `Params::palw_rcore_plus` (or off `ConsensusV2`): the filer does not run.
    Dormant,
}

/// **The fold's half of the question, classified** — the ONE adjudicator on `state` without the
/// signature (the processor's read adds the gate and the fold), after the (seat, claim) ledger key
/// the gate and the fold both read first-or-last. `rules` and `fp_decode_rules_active` are the ones
/// the gate and the fold read at the block the carrier is expected in.
pub fn palw_false_valid_filing_check_v1(
    state: &PalwChainStateV2,
    accused: &PalwBondKeyV2,
    evidence: &[u8],
    fp_decode_rules_active: bool,
    rules: PalwIdentityRulesV1,
) -> PalwFalseValidFilingCheckV1 {
    use PalwFalseValidFilingCheckV1 as Check;
    use PalwOffenceVerifyError as E;
    let Ok(payload) = borsh::from_slice::<PalwPanelFalseValidEvidenceV2>(evidence) else {
        return Check::Refused(E::PanelFalseValidNeedsContradiction.to_string());
    };
    let offence_id = palw_false_valid_offence_id_v2(&accused.0, &payload.claim_id);
    if state.consumed_offence(&offence_id).is_some() {
        return Check::ConvictedBefore { offence_id };
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
    /// A partial-mask signer of a held-regime class ([`PALW_FALSE_VALID_HELD_NAMES_PARTIALS_V1`]).
    HeldPartial,
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
}

impl PalwFalseValidFilingV1 {
    /// Send it: the chain convicts and node policy agrees.
    pub fn files(&self) -> bool {
        matches!(self.check, PalwFalseValidFilingCheckV1::File { .. }) && self.policy.is_none()
    }

    /// Nothing about this signer can change by waiting: filed (and pending the chain), convicted,
    /// not liable, refused or declined. Only an open court ([`PalwFalseValidFilingCheckV1::Wait`])
    /// and a dormant read are asked again.
    pub fn settles(&self) -> bool {
        !matches!(self.check, PalwFalseValidFilingCheckV1::Wait(_) | PalwFalseValidFilingCheckV1::Dormant)
    }
}

/// **The filings a proof makes of a licence's `Valid` signers** — one entry per seat, in the order
/// the receipts name them. For each seat that is not this node's (`own`) and not already done with
/// (`skip`: settled, or a carrier in flight), each of its receipts in turn is built into the
/// evidence ([`PalwPanelFalseValidEvidenceV2::filed_v2`]) and asked of the chain (`check`); the
/// first that files — or is convicted already — is the seat's entry, else its last answer. A seat
/// the liability rule does not reach is therefore never named, and a seat named once is named
/// under one (seat, claim) key whatever form its receipts took.
///
/// Receipts naming another claim, or not `Valid`, are passed over (the licence walk hands every
/// receipt it found); at most [`PALW_FALSE_VALID_RECEIPTS_PER_CLAIM_V1`] are read.
pub fn palw_false_valid_filings_v1(
    proof: &PalwFalseValidProofV1,
    receipts: &[PalwFalseValidReceiptV1],
    own: &PalwBondKeyV2,
    skip: impl Fn(&PalwBondKeyV2) -> bool,
    mut check: impl FnMut(&PalwConsensusObjectV2) -> PalwFalseValidFilingCheckV1,
) -> Vec<PalwFalseValidFilingV1> {
    let read: Vec<&PalwFalseValidReceiptV1> = receipts
        .iter()
        .filter(|r| r.inner().claim == proof.claim_id && r.inner().verdict == crate::palw_panel_v2::PalwReceiptVerdictV2::Valid)
        .take(PALW_FALSE_VALID_RECEIPTS_PER_CLAIM_V1)
        .collect();
    let mut seats: Vec<PalwBondKeyV2> = Vec::new();
    for receipt in &read {
        let seat = receipt.inner().seat_bond;
        if seat != *own && !skip(&seat) && !seats.contains(&seat) {
            seats.push(seat);
        }
    }
    let mut out = Vec::with_capacity(seats.len());
    for seat in seats {
        let mut last: Option<PalwFalseValidFilingV1> = None;
        for receipt in read.iter().filter(|r| r.inner().seat_bond == seat) {
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
            let answer = check(&object);
            let policy = match &answer {
                PalwFalseValidFilingCheckV1::File { producer, .. } if producer == own => Some(PalwFalseValidPolicyV1::OwnClaim),
                PalwFalseValidFilingCheckV1::File { class_held: true, full_attestation: false, .. }
                    if !PALW_FALSE_VALID_HELD_NAMES_PARTIALS_V1 =>
                {
                    Some(PalwFalseValidPolicyV1::HeldPartial)
                }
                _ => None,
            };
            let filing = PalwFalseValidFilingV1 {
                accused: seat,
                claim_id: proof.claim_id,
                offence_id: palw_false_valid_offence_id_v2(&seat.0, &proof.claim_id),
                evidence_id,
                object,
                check: answer,
                policy,
            };
            let done = filing.files() || matches!(filing.check, PalwFalseValidFilingCheckV1::ConvictedBefore { .. });
            last = Some(filing);
            if done {
                break;
            }
        }
        out.extend(last);
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

    /// One entry per seat; this node's own receipt is never built into anything; a skipped seat is
    /// not asked; the first receipt of a seat that files wins over its other forms; a liability
    /// refusal is recorded and never files.
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
        let filings = palw_false_valid_filings_v1(
            &proof(claim),
            &receipts,
            &own,
            |seat| *seat == bond(4),
            |object| {
                let PalwConsensusObjectV2::ObjectiveOffence { accused, evidence, .. } = object else { unreachable!() };
                let payload: PalwPanelFalseValidEvidenceV2 = borsh::from_slice(evidence).unwrap();
                let full = matches!(payload.receipt, PalwFalseValidReceiptV1::Full(_));
                asked.push((*accused, full));
                match (accused, full) {
                    (a, _) if *a == bond(1) => file(producer, false, true),
                    (a, false) if *a == bond(2) => PalwFalseValidFilingCheckV1::NotLiable(PalwOffenceVerifyError::SiteNotAttested),
                    (a, true) if *a == bond(2) => file(producer, false, true),
                    _ => PalwFalseValidFilingCheckV1::NotLiable(PalwOffenceVerifyError::SiteNotAttested),
                }
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
        assert!(filings[2].settles(), "a signer the rule does not reach is done with");
    }

    /// Node policy: a claim this node produced is never filed on (the finding would charge its own
    /// bond as the producer), and a held class's partial-mask signer is not named while a full-mask
    /// one is; an open court is asked again, not settled.
    #[test]
    fn policy_declines_the_own_claim_and_held_partials_and_waits_on_a_court() {
        let claim = Hash64::from_u64_word(0xC5);
        let own = bond(9);
        let receipts = vec![v3(1, claim, 0b1111), v3(2, claim, 0b0001), v3(3, claim, 0b0010)];
        let filings = palw_false_valid_filings_v1(
            &proof(claim),
            &receipts,
            &own,
            |_| false,
            |object| {
                let PalwConsensusObjectV2::ObjectiveOffence { accused, .. } = object else { unreachable!() };
                if *accused == bond(1) {
                    file(bond(0), true, true)
                } else if *accused == bond(2) {
                    file(bond(0), true, false)
                } else {
                    PalwFalseValidFilingCheckV1::Wait(PalwOffenceVerifyError::ClaimUnderSession)
                }
            },
        );
        assert!(filings[0].files(), "a held class's full-mask signer is named");
        assert_eq!(filings[1].policy, Some(PalwFalseValidPolicyV1::HeldPartial));
        assert!(!filings[1].files() && filings[1].settles());
        assert!(!filings[2].settles(), "an open court is asked again");
        let own_claim = palw_false_valid_filings_v1(&proof(claim), &receipts[..1], &own, |_| false, |_| file(own, false, true));
        assert_eq!(own_claim[0].policy, Some(PalwFalseValidPolicyV1::OwnClaim));
        assert!(!own_claim[0].files());
    }
}
