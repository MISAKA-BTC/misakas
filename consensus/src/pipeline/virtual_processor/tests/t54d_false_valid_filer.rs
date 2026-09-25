//! **ADR-0152 v3.1 N10 / §7.3 P2-8c, T54d at processor level: the automatic `PanelFalseValidV2`
//! filer on real claims.**
//!
//! T46's harness, doors and assertions — this module is that suite's child, so nothing here is a
//! second copy of them: testnet-12's shipped ruleset with harness keys on its eight cards, the
//! producer's own claims, the licences the cards signed, and every object through the gate, the
//! acceptance walk and the fold, weighed in one 0x4b carrier.
//!
//! What runs is the node's own path from a proof to the objects its seam queues: the licence's
//! `Valid` receipts in the form each was licensed in (`palw_false_valid_receipts_of_licence_v1`, what
//! kaspad's chain walk reads), their admission (`palw_false_valid_admit_receipts_v1` over the
//! processor's `palw_false_valid_receipt_relied_v1_at`, what kaspad asks through
//! `palw_false_valid_relied_receipts_v1`), the selection (`palw_false_valid_filings_v1`) and, per
//! receipt, the processor's read (`palw_false_valid_filing_check_v1_at`: the ledger key, the
//! admission, the adjudicator, the gate, the fold on the tip) — the same reads kaspad asks through
//! `ConsensusApi`. The proof is the capture arm's: the floor's `refutation_for_index` of the lying
//! capture at the faulted leaf, with its operand openings, in the network's carriage (`H::carried`,
//! the sampler's `palw_refutation_prompt_carriage_v1`) — or a court close's arithmetic form of the
//! same refutation (`PalwFalseValidProofV1::of_court_close_v1`). What does NOT run here is kaspad
//! itself — the book, the chain walk and the door to P2-8's reporter filer are
//! `kaspad::palw_filer_false_valid`'s unit tests (the scan over a stubbed chain included); the door
//! hands the filing's `object` unchanged, keyed as the filer keys it, which those tests pin. Since the
//! integration of the three Phase 2 lanes every filing takes the reporter filer's road, so the main
//! test here does too (P2-8's `Filing`, `commit_then_file_all` and `reveal_and_paid`): the bystander's
//! commitments root, the evidence folds, and its reward R arrives after the reveal. A node e2e on a
//! devnet preset is post-launch (the operator moved every drill and node launch after the t12 launch,
//! 2026-09-24).
use super::*;
use kaspa_consensus_core::palw_false_valid_filing_v1::{
    PALW_FALSE_VALID_RECEIPTS_PER_SEAT_V1, PalwFalseValidFilingCheckV1 as Check, PalwFalseValidFilingV1, PalwFalseValidPolicyV1,
    PalwFalseValidProofV1, palw_false_valid_admit_receipts_v1, palw_false_valid_filings_v1,
};
use kaspa_consensus_core::palw_offence_attribution_v1::palw_false_valid_receipts_of_licence_v1;

use super::t46_p2_8_reporter_filer::{Filing, commit_then_file_all, reveal_and_paid};

/// The proof the capture arm holds for `claim`: its contradiction in the network's carriage.
fn capture_proof(h: &H, claim: &RealClaim) -> PalwFalseValidProofV1 {
    let (contradiction, prompt_ids_opening) = h.carried(claim.contradiction());
    PalwFalseValidProofV1 { claim_id: claim.claim_id, contradiction, prompt_ids_opening }
}

/// What the chain walk reads off a Verification V2 licence: every seat's V3 receipt, `Segmented`.
fn v2_licence_receipts(
    claim: Hash64,
    licence: &Licence,
) -> Vec<kaspa_consensus_core::palw_offence_attribution_v1::PalwFalseValidReceiptV1> {
    let object = Obj::ReceiptLicensedV2 { claim, receipts: licence.receipts.iter().map(|(_, r)| r.clone()).collect() };
    let (named, receipts) = palw_false_valid_receipts_of_licence_v1(&object).expect("a licence");
    assert_eq!(named, claim);
    assert_eq!(receipts.len(), licence.receipts.len(), "every seat signed Valid");
    receipts
}

/// The node's filings of `proof` as `own` at the walk's next block — the processor's read per signer.
fn filings_at(
    h: &H,
    walk: &Walk,
    proof: &PalwFalseValidProofV1,
    receipts: &[PalwFalseValidReceiptV1],
    own: usize,
) -> Vec<PalwFalseValidFilingV1> {
    let point = walk.next();
    palw_false_valid_filings_v1(
        proof,
        receipts,
        &h.cards[own],
        |_| false,
        |_| false,
        |object| h.vp().palw_false_valid_filing_check_v1_at(&walk.state, &point, object),
    )
}

/// What kaspad's scan keeps of `offered` on the walk's state: `palw_false_valid_admit_receipts_v1`
/// over the processor's admission, into `kept`. Returns how many were admitted.
fn admit(h: &H, walk: &Walk, kept: &mut Vec<PalwFalseValidReceiptV1>, claim: Hash64, offered: Vec<PalwFalseValidReceiptV1>) -> usize {
    palw_false_valid_admit_receipts_v1(kept, claim, offered, |batch| {
        batch.iter().map(|r| h.vp().palw_false_valid_receipt_relied_v1_at(&walk.state, r).is_ok()).collect()
    })
}

/// Card `card`'s receipt of `claim` over `mask` at `signed_daa`, with a signature that verifies under
/// nothing — what a junk licence object carries.
fn junk_v3(h: &H, card: usize, claim: Hash64, mask: PalwSegmentMaskV2, signed_daa: u64) -> PalwSeatReceiptV3 {
    PalwSeatReceiptV3 {
        receipt: PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: h.cards[card],
            signed_daa,
            signature: vec![0u8; 4],
        },
        segments: mask,
    }
}

fn filed_cards(h: &H, filings: &[PalwFalseValidFilingV1]) -> Vec<usize> {
    let mut cards: Vec<usize> = filings.iter().filter(|f| f.files()).map(|f| h.card_of(&f.accused)).collect();
    cards.sort();
    cards
}

fn sorted(mut cards: Vec<usize>) -> Vec<usize> {
    cards.sort();
    cards
}

/// **T54d: the capture arm's proof, filed automatically before `Final`, convicts exactly the signers
/// the audit's rule makes liable, through S-4's funnel.** The injected step fault sits at a leaf whose
/// step reads only its own segment (T46b's lie), so the full seat and that segment's holder are
/// liable; the three other partial seats are answered `NotLiable(SiteNotAttested)` and never named.
/// Each object the node builds is byte for byte the suite's canonical V3-receipt evidence (`H::v2`),
/// the two ride one block through the gate, the walk and the fold, and the conviction is T46b's: the
/// seats' locks taken plus `min(25% · C₀, 3 G)`, the claim voided `CourtFraud` with S2 on its
/// producer. Both go the reporter filer's road (R-3): the bystander that proved the claim false
/// commits to each (keyed as the chain keys it), the commitments root, the evidence folds two DAA
/// later; asked again, both are `ConvictedBefore` — the node never files an offence twice — and each
/// reward pends on the evidence the node filed, is revealed by the bystander, and arrives at its
/// payout when the window closes.
#[tokio::test]
async fn t54d_the_capture_proof_files_every_liable_signer_and_the_funnel_charges_them() {
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let leaf = claim.fault_leaf.expect("a step fault");
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, licence.assignment.segments, leaf).expect("in the cut");
    let (full, holder) = (licence.full_card(), licence.holder_of(segment));
    let proof = capture_proof(&h, &claim);
    let receipts = v2_licence_receipts(id, &licence);
    let filings = filings_at(&h, &walk, &proof, &receipts, BYSTANDER);
    assert_eq!(filings.len(), 5, "every Valid signer is asked, once");
    assert_eq!(filed_cards(&h, &filings), sorted(vec![full, holder]), "exactly the liable signers are filed");
    for filing in &filings {
        let card = h.card_of(&filing.accused);
        assert_eq!(filing.offence_id, palw_false_valid_offence_id_v2(&filing.accused.0, &id));
        assert_eq!(
            filing.object,
            h.v2(card, id, licence.segmented(card), claim.contradiction()),
            "card {card}: the canonical evidence"
        );
        if card == full || card == holder {
            let Check::File { claim_id, site, acts_on_claim, class_held, full_attestation, producer, .. } = &filing.check else {
                panic!("card {card} files: {:?}", filing.check)
            };
            assert_eq!((*claim_id, *producer, *acts_on_claim, *class_held), (id, h.cards[EXECUTOR], true, false));
            assert_eq!(*full_attestation, card == full, "the full seat attested the whole job, the holder its segment");
            assert!(matches!(site, PalwFaultSiteV1::Leaf { leaf: at, .. } if *at == leaf), "a located fault: {site:?}");
        } else {
            assert_eq!(filing.check, Check::NotLiable(E::SiteNotAttested), "card {card}: never named");
            assert!(!filing.files() && !filing.declined());
            assert_eq!(filing.not_liable, vec![filing.evidence_id], "recorded per receipt");
        }
    }
    // The reporter filer's road: each filing keyed as the filer keys it, committed, rooted, filed.
    let reported: Vec<Filing> = filings
        .iter()
        .filter(|f| f.files())
        .zip(0xD0u8..)
        .map(|(f, salt)| {
            let reported = Filing::of(f.object.clone(), [salt; 32]);
            assert_eq!((reported.key, reported.evidence_id), (f.offence_id, f.evidence_id), "the filer keys it as the chain does");
            reported
        })
        .collect();
    let (licensed, committed_at) = commit_then_file_all(&h, &mut walk, &reported, BYSTANDER);
    let filed: Vec<usize> = filings.iter().filter(|f| f.files()).map(|f| h.card_of(&f.accused)).collect();
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &filed, walk.daa);
    // Asked again: the chain has both; the rest are still not liable. Nothing files twice.
    let again = filings_at(&h, &walk, &proof, &receipts, BYSTANDER);
    assert!(again.iter().all(|f| !f.files()), "{:?}", again.iter().map(|f| &f.check).collect::<Vec<_>>());
    for filing in &again {
        let card = h.card_of(&filing.accused);
        if card == full || card == holder {
            assert_eq!(filing.check, Check::ConvictedBefore { offence_id: filing.offence_id, accepted_daa: walk.daa }, "card {card}");
        }
    }
    // R-3/R-4: each reward rests on the evidence the node filed (N12), and arrives after the reveal.
    let paid = reveal_and_paid(&h, &mut walk, &reported, BYSTANDER, committed_at);
    assert_eq!(paid.len(), 2, "one reward a conviction, both the bystander's");
}

/// **T54d after `Final`: the same proof, filed once the claim finalised, reverses the `Final`** and
/// charges each liable seat S4 through the funnel; the unliable partial seats are not named.
#[tokio::test]
async fn t54d_after_final_the_filing_reverses_the_final_and_charges_the_liable_seats() {
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let leaf = claim.fault_leaf.expect("a step fault");
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, licence.assignment.segments, leaf).expect("in the cut");
    let (full, holder) = (licence.full_card(), licence.holder_of(segment));
    h.sweep_to_final(&mut walk, id);
    assert!(matches!(walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    let proof = capture_proof(&h, &claim);
    let filings = filings_at(&h, &walk, &proof, &v2_licence_receipts(id, &licence), BYSTANDER);
    assert_eq!(filed_cards(&h, &filings), sorted(vec![full, holder]));
    let before = walk.state.clone();
    let objects: Vec<Obj> = filings.iter().filter(|f| f.files()).map(|f| f.object.clone()).collect();
    h.carry(&mut walk, objects);
    let (s, daa) = (&walk.state, walk.daa);
    assert!(
        matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud } if voided_daa == daa),
        "the Final is reversed: {:?}",
        s.claim(&id).unwrap().phase
    );
    let g = g_of(s, id);
    for card in [full, holder] {
        let seat = h.cards[card];
        let lock = *before.slashable_lock(seat, id).expect("the lock stands after Final");
        let (_, debit) = s4_charge(&h, &before, seat, lock.amount, g, daa);
        assert!(s.slashable_lock(seat, id).is_none(), "card {card}'s lock is taken");
        assert_eq!(
            s.bond(&seat).unwrap().collateral as u128,
            before.bond(&seat).unwrap().collateral as u128 - debit,
            "card {card}: S4 — the lock and min(25% · C₀, 3 G)"
        );
        let row = s.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &id)).expect("one kind-3 row");
        assert_eq!((row.kind, row.claim_id), (PalwOffenceKindV1::PanelFalseValidV2, id));
    }
    for card in licence.partials().into_iter().filter(|c| *c != holder) {
        assert_eq!(
            s.bond(&h.cards[card]).unwrap().collateral,
            before.bond(&h.cards[card]).unwrap().collateral,
            "card {card} untouched"
        );
        assert!(s.slashable_lock(h.cards[card], id).is_some(), "card {card} keeps its lock");
    }
    assert!(s.palw_execution_root_is_forfeited_v1(&claim.envelope.attempt.execution_root), "the proven-false root is forfeit");
    h.reloads(s);
}

/// **C-3 both ways: a V1 licence's `Full` receipts are filed as `Full`, and every one is liable.** The
/// same claim licensed through the V1 door (`ReceiptLicensed`, five V2 receipts): the licence walk
/// reads each receipt `Full` — signed over the V2 message — and every seat is filed and convicted.
#[tokio::test]
async fn t54d_a_v1_licences_full_receipts_are_filed_and_fold() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    h.bind(&mut walk, id);
    let signed = h.license_v1(&mut walk, id);
    let object = Obj::ReceiptLicensed { claim: id, receipts: signed.iter().map(|(_, r)| r.clone()).collect() };
    let (_, receipts) = palw_false_valid_receipts_of_licence_v1(&object).expect("a licence");
    assert!(receipts.iter().all(|r| matches!(r, PalwFalseValidReceiptV1::Full(_))), "the form the seats signed");
    let filings = filings_at(&h, &walk, &capture_proof(&h, &claim), &receipts, BYSTANDER);
    assert_eq!(filed_cards(&h, &filings), sorted(PANEL.to_vec()), "a Full receipt is always liable");
    for filing in &filings {
        let Check::File { full_attestation, .. } = filing.check else { panic!("{:?}", filing.check) };
        assert!(full_attestation);
    }
    let objects: Vec<Obj> = filings.iter().map(|f| f.object.clone()).collect();
    let (licensed, _) = h.carry(&mut walk, objects);
    let order: Vec<usize> = filings.iter().map(|f| h.card_of(&f.accused)).collect();
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &order, walk.daa);
}

/// **A fault at `Whole` names the full seat alone.** A forged decode token (T46c's claim) is a
/// `ForgedOutput`, whose site is `Whole`: the full-mask seat is filed and convicted; every partial
/// seat is `NotLiable` and is never named — and one of them being this node changes nothing but that
/// its own receipt is never built into evidence.
#[tokio::test]
async fn t54d_a_whole_fault_names_the_full_seat_alone() {
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Forged);
    let id = claim.claim_id;
    let full = licence.full_card();
    let me = licence.partials()[0];
    let filings = filings_at(&h, &walk, &capture_proof(&h, &claim), &v2_licence_receipts(id, &licence), me);
    assert_eq!(filings.len(), 4, "this node's own receipt is not asked about");
    assert!(filings.iter().all(|f| f.accused != h.cards[me]));
    assert_eq!(filed_cards(&h, &filings), vec![full]);
    for filing in filings.iter().filter(|f| f.accused != h.cards[full]) {
        assert_eq!(filing.check, Check::NotLiable(E::SiteNotAttested), "a partial seat at Whole");
    }
    let (licensed, _) = h.carry(&mut walk, filings.iter().filter(|f| f.files()).map(|f| f.object.clone()).collect());
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &[full], walk.daa);
}

/// **An honest licence produces no filing.** On an honest claim the capture arm clears (no proof is
/// ever noted); even a node that held a refutation of it — the floor's own opening of the honest
/// capture at the middle leaf, with its rows, in the network's carriage — files against nobody: the
/// adjudicator answers every signer `PanelFalseValidNeedsContradiction`: a refusal, filed against
/// nobody (asked again at a later walk, it refuses again).
#[tokio::test]
async fn t54d_an_honest_licence_produces_no_filing() {
    let h = harness(true);
    let (walk, claim, licence) = h.licensed(Fault::Honest);
    let id = claim.claim_id;
    let leaf = claim.binding.step_leaf_count / 2;
    let refutation = h.backend.refutation_for_index(&claim.material, leaf).expect("the honest capture opens");
    let operand_openings = h.backend.operand_openings_for(&refutation).expect("the class opens the rows");
    let (contradiction, prompt_ids_opening) = h.carried(C::StepArithmetic { refutation, operand_openings });
    let proof = PalwFalseValidProofV1 { claim_id: id, contradiction, prompt_ids_opening };
    let filings = filings_at(&h, &walk, &proof, &v2_licence_receipts(id, &licence), BYSTANDER);
    assert_eq!(filings.len(), 5);
    for filing in &filings {
        assert_eq!(filing.check, Check::Refused(E::PanelFalseValidNeedsContradiction.to_string()));
        assert!(!filing.files() && !filing.declined());
    }
}

/// **Node policy: never against this node's own claim.** Were the producer's node to hold a proof of
/// its own claim, the chain would convict (the finding acts on the claim and charges its producer),
/// and the filer declines every filing (`OwnClaim`).
#[tokio::test]
async fn t54d_the_producer_never_files_on_its_own_claim() {
    let h = harness(true);
    let (walk, claim, licence) = h.licensed(Fault::Forged);
    let filings = filings_at(&h, &walk, &capture_proof(&h, &claim), &v2_licence_receipts(claim.claim_id, &licence), EXECUTOR);
    let full = filings.iter().find(|f| f.accused == h.cards[licence.full_card()]).expect("the full seat is asked");
    assert!(matches!(full.check, Check::File { .. }), "the chain would convict");
    assert_eq!(full.policy, Some(PalwFalseValidPolicyV1::OwnClaim));
    assert!(filings.iter().all(|f| !f.files()), "the producer files on its own claim against nobody");
}

/// **The fence-off twins: below `palw_rcore_plus` the read is `Dormant` and nothing files** — with
/// `palw_offence_attribution` unset (kind 3 does not exist: the gate refuses it `AttributionDormant`)
/// and with the attribution armed but R-core+ off (P2-8c, like all of Phase 2, lives past R-core+).
/// A dormant answer settles nothing, so a book that crossed the fence would still hold the proof.
#[tokio::test]
async fn t54d_fence_off_twins_file_nothing() {
    for (label, h) in [("attribution off", harness(false)), ("R-core+ off", harness_rcore_off())] {
        let (walk, claim, licence) = h.licensed(Fault::Step);
        let filings = filings_at(&h, &walk, &capture_proof(&h, &claim), &v2_licence_receipts(claim.claim_id, &licence), BYSTANDER);
        assert_eq!(filings.len(), 5, "{label}");
        for filing in &filings {
            assert_eq!(filing.check, Check::Dormant, "{label}");
            assert!(!filing.files() && !filing.declined(), "{label}");
        }
    }
}

/// **The review's high finding, on the real harness: junk licences before and after the genuine one
/// never hide a liable signer.** Before the licence, a `ReceiptLicensedV2` of every seat's receipt
/// over its ASSIGNED mask with a signature that verifies under nothing rides statelessly, the gate
/// refuses it and the walk drops it — but its carrier stays accepted, so a node's walk reads it. The
/// processor's admission keeps none of it (asked directly, each junk receipt is `Unrelied`, never a
/// `NotLiable` or `Refused` that could speak for the seat). The genuine licence lands; then 32 junk
/// receipts — the real seats' with varied `signed_daa`, strangers' bonds — and a colluding holder's
/// VALIDLY signed receipt over a mask it was never locked for are read after it. The book keeps
/// exactly the five genuine receipts, and the full seat and the fault segment's holder are filed and
/// convicted through the funnel.
#[tokio::test]
async fn t54d_junk_licences_before_and_after_the_genuine_one_never_hide_a_liable_signer() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    h.bind(&mut walk, id);
    let panel = walk.state.panel(&id).expect("bound").clone();
    let assignment = palw_segment_assignment_v2(panel.anchor, id, panel.seats.len() as u16);
    let before: Vec<PalwSeatReceiptV3> = panel
        .seats
        .iter()
        .enumerate()
        .map(|(i, seat)| junk_v3(&h, h.card_of(&seat.bond), id, assignment.mask_of(i as u16), walk.daa))
        .collect();
    let junk_licence = Obj::ReceiptLicensedV2 { claim: id, receipts: before.clone() };
    assert!(
        kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&junk_licence).is_ok(),
        "a junk licence rides on stateless admission"
    );
    let point = walk.next();
    assert!(h.validate(&walk.state, &point, &junk_licence).is_err(), "the gate refuses it");
    assert!(h.accepted(&walk.state, &point, std::slice::from_ref(&junk_licence)).is_empty(), "and the walk drops it");
    let (_, read) = palw_false_valid_receipts_of_licence_v1(&junk_licence).expect("a licence object");
    let proof = capture_proof(&h, &claim);
    for filing in filings_at(&h, &walk, &proof, &read, BYSTANDER) {
        assert!(matches!(filing.check, Check::Unrelied(_)), "junk is unrelied, whatever its mask: {:?}", filing.check);
        assert!(filing.not_liable.is_empty() && !filing.files() && !filing.declined());
    }
    let mut kept = Vec::new();
    assert_eq!(admit(&h, &walk, &mut kept, id, read.clone()), 0, "nothing of the junk licence takes a slot");

    let licence = h.license_v2(&mut walk, id);
    let leaf = claim.fault_leaf.expect("a step fault");
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, licence.assignment.segments, leaf).expect("in the cut");
    let (full, holder) = (licence.full_card(), licence.holder_of(segment));
    // Read newest first, as the walk reads: the junk after the licence, the licence, the junk before.
    let mut offered: Vec<PalwFalseValidReceiptV1> = Vec::new();
    for n in 0..32u64 {
        let (card, mask) = if n % 2 == 0 {
            let (card, r) = &licence.receipts[(n / 2) as usize % licence.receipts.len()];
            (*card, r.segments)
        } else {
            (BYSTANDER + (n as usize % 2), PalwSegmentMaskV2::full(4))
        };
        offered.push(PalwFalseValidReceiptV1::Segmented(junk_v3(&h, card, id, mask, walk.daa + n)));
    }
    let colluding = h.v3_receipt(holder, id, h.domain, walk.daa, PalwSegmentMaskV2::full(4));
    offered.push(PalwFalseValidReceiptV1::Segmented(colluding.clone()));
    offered.extend(v2_licence_receipts(id, &licence));
    offered.extend(read);
    assert_eq!(
        h.vp().palw_false_valid_receipt_relied_v1_at(&walk.state, &PalwFalseValidReceiptV1::Segmented(colluding)),
        Err("the chain holds no Valid of this seat on the claim in this receipt's form (no lock over its mask, no liability row, no \
             conviction)"),
        "a validly signed receipt over a mask the seat was never locked for is not the one the chain relied on"
    );
    assert_eq!(admit(&h, &walk, &mut kept, id, offered), 5);
    let mut genuine = v2_licence_receipts(id, &licence);
    genuine.sort_by_key(|r| r.inner().seat_bond);
    kept.sort_by_key(|r| r.inner().seat_bond);
    assert_eq!(kept, genuine, "the book holds the five genuine receipts and nothing else");
    assert!(PALW_FALSE_VALID_RECEIPTS_PER_SEAT_V1 >= 1);
    let filings = filings_at(&h, &walk, &proof, &kept, BYSTANDER);
    assert_eq!(filed_cards(&h, &filings), sorted(vec![full, holder]), "both liable seats are filed");
    let (licensed, _) = h.carry(&mut walk, filings.iter().filter(|f| f.files()).map(|f| f.object.clone()).collect());
    let order: Vec<usize> = filings.iter().filter(|f| f.files()).map(|f| h.card_of(&f.accused)).collect();
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &order, walk.daa);
    // Convicted — the two seats' locks taken — every receipt stays admitted: the convicted seats' by
    // their conviction (a filer keeps watching it through a reorg), the others' by lock or row.
    let mut again = Vec::new();
    assert_eq!(admit(&h, &walk, &mut again, id, kept.clone()), kept.len(), "a convicted seat's receipt is still relied on");
    for card in [full, holder] {
        assert!(walk.state.slashable_lock(h.cards[card], id).is_none(), "card {card}'s lock is gone, its conviction stands");
    }
}

/// **The realistic order (the review's R3): the capture arm's one-move accusation lands first.** The
/// `ShardCourtAccused` the same node files voids the licensed claim `CourtFraud`; the kind-3 filing of
/// the same refutation after it still names the full seat and the fault segment's holder only, and
/// both fold: each seat's lock is taken and its (seat, claim) row written, the partial seats keep
/// theirs.
#[tokio::test]
async fn t54d_after_the_one_move_court_void_the_filing_still_files_and_folds() {
    use kaspa_consensus_core::palw_shard_court_v1::{
        PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT, PALW_SHARD_COURT_VERSION_V1, PalwShardCourtAccusationV1,
        palw_shard_court_session_id_v1,
    };
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let leaf = claim.fault_leaf.expect("a step fault");
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, licence.assignment.segments, leaf).expect("in the cut");
    let (full, holder) = (licence.full_card(), licence.holder_of(segment));
    let (carried, prompt_ids_opening) = h.carried(claim.contradiction());
    let C::StepArithmetic { refutation, operand_openings } = carried else { panic!("a step refutation") };
    let mut accusation = PalwShardCourtAccusationV1 {
        version: PALW_SHARD_COURT_VERSION_V1,
        claim: id,
        execution_root: claim.envelope.attempt.execution_root,
        trace_root: claim.envelope.attempt.trace_root,
        executor_bond: h.cards[EXECUTOR],
        accuser_bond: h.cards[BYSTANDER],
        leaf_index: leaf,
        refutation,
        artifact_openings: operand_openings,
        prompt_ids_opening,
        signature: Vec::new(),
    };
    accusation.signature = sign(
        BYSTANDER,
        palw_shard_court_session_id_v1(h.domain.as_byte_slice(), &accusation).as_byte_slice(),
        PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT,
    );
    h.carry(&mut walk, vec![Obj::ShardCourtAccused { accusation: Box::new(accusation) }]);
    assert!(
        matches!(walk.state.claim(&id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. })),
        "the one-move court voids the claim first"
    );
    let filings = filings_at(&h, &walk, &capture_proof(&h, &claim), &v2_licence_receipts(id, &licence), BYSTANDER);
    assert_eq!(filed_cards(&h, &filings), sorted(vec![full, holder]));
    let before = walk.state.clone();
    h.carry(&mut walk, filings.iter().filter(|f| f.files()).map(|f| f.object.clone()).collect());
    for card in [full, holder] {
        let seat = h.cards[card];
        assert!(before.slashable_lock(seat, id).is_some() && walk.state.slashable_lock(seat, id).is_none(), "card {card}'s lock");
        let row = walk.state.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &id)).expect("one kind-3 row");
        assert_eq!((row.kind, row.claim_id, row.accepted_daa), (PalwOffenceKindV1::PanelFalseValidV2, id, walk.daa));
        assert!(walk.state.bond(&seat).unwrap().collateral < before.bond(&seat).unwrap().collateral, "card {card} is charged");
    }
    for card in licence.partials().into_iter().filter(|c| *c != holder) {
        assert_eq!(walk.state.bond(&h.cards[card]).unwrap().collateral, before.bond(&h.cards[card]).unwrap().collateral);
    }
}

/// **A court close's proof, while the court is open and after** (the review's court-close case). The
/// close's arithmetic form of the refutation (`ArithmeticOpened`, the network's carriage) is turned
/// into a proof by the one converter kaspad calls (`PalwFalseValidProofV1::of_court_close_v1`), and
/// is the capture proof exactly. While a court is open on the claim every liable signer is `Wait`
/// (asked again soon, never settled); with no court open the same proof files the full seat and the
/// holder, and the conviction folds.
#[tokio::test]
async fn t54d_a_court_close_proof_waits_on_an_open_court_and_then_files() {
    use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let leaf = claim.fault_leaf.expect("a step fault");
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, licence.assignment.segments, leaf).expect("in the cut");
    let (full, holder) = (licence.full_card(), licence.holder_of(segment));
    let (carried, opening) = h.carried(claim.contradiction());
    let C::StepArithmetic { refutation, operand_openings } = carried else { panic!("a step refutation") };
    let close = PalwCourtVerdictProofV2::ArithmeticOpened {
        refutation,
        operand_openings,
        prompt_ids_opening: opening.expect("testnet-12 commits its prompts as a Merkle root"),
    };
    let proof = PalwFalseValidProofV1::of_court_close_v1(id, &close).expect("an arithmetic close is a proof");
    assert_eq!(proof, capture_proof(&h, &claim), "the close's refutation, in the carriage it carried");
    let receipts = v2_licence_receipts(id, &licence);

    // A court open on the claim (the row written through the carriage, as T46's session case does).
    let mut court_walk = walk.clone();
    let at = court_walk.daa;
    let (challenger, executor) = (h.cards[BYSTANDER], h.cards[EXECUTOR]);
    let ladder = kaspa_consensus_core::palw_bisect::PalwBisectLadderV1::open(
        &id,
        &claim.envelope.attempt.trace_root,
        &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&challenger),
        &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&executor),
        kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
        h.bundle.court.max_step_leaf_count(),
        at,
        at + 50,
    )
    .expect("a ladder opens");
    let session_id = ladder.session_id();
    court_walk.state = h.rebuilt(&court_walk.state, |carriage| {
        carriage.court_sessions.insert(
            session_id,
            kaspa_consensus_core::palw_state_v2::PalwCourtSessionStateV2 {
                claim: id,
                challenger_bond: challenger,
                opened_daa: at,
                deadline_daa: at + h.sp().window_court(),
                ladder,
                dissection: None,
            },
        );
    });
    assert_eq!(court_walk.state.open_courts_of(&id), 1);
    let waiting = filings_at(&h, &court_walk, &proof, &receipts, BYSTANDER);
    for filing in waiting.iter().filter(|f| [full, holder].contains(&h.card_of(&f.accused))) {
        assert_eq!(filing.check, Check::Wait(E::ClaimUnderSession), "a liable signer waits on the court");
        assert!(!filing.files() && !filing.declined());
    }

    let filings = filings_at(&h, &walk, &proof, &receipts, BYSTANDER);
    assert_eq!(filed_cards(&h, &filings), sorted(vec![full, holder]));
    let (licensed, _) = h.carry(&mut walk, filings.iter().filter(|f| f.files()).map(|f| f.object.clone()).collect());
    let order: Vec<usize> = filings.iter().filter(|f| f.files()).map(|f| h.card_of(&f.accused)).collect();
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &order, walk.daa);
}

/// **An S2 licence (`OptimisticLicensed`)**: the full seat's and the fault segment's holder's V3
/// receipts license the claim through the optimistic door; the walk reads both `Segmented`, both are
/// admitted, filed and convicted — and the three partial seats that signed nothing the chain locked
/// are never asked.
#[tokio::test]
async fn t54d_an_optimistic_licences_receipts_are_filed_and_fold() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    h.bind(&mut walk, id);
    let panel = walk.state.panel(&id).expect("bound").clone();
    let assignment = palw_segment_assignment_v2(panel.anchor, id, panel.seats.len() as u16);
    let leaf = claim.fault_leaf.expect("a step fault");
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, assignment.segments, leaf).expect("in the cut");
    let index_of = |pick: &dyn Fn(u16) -> bool| (0..panel.seats.len() as u16).find(|i| pick(*i)).expect("a seat");
    let full_index = assignment.full_seat;
    let holder_index = index_of(&|i| i != full_index && assignment.mask_of(i).covers(segment));
    let point = walk.next();
    let signed: Vec<PalwSeatReceiptV3> = [full_index, holder_index]
        .iter()
        .map(|i| {
            let card = h.card_of(&panel.seats[*i as usize].bond);
            h.v3_receipt(card, id, h.domain, point.daa_score, assignment.mask_of(*i))
        })
        .collect();
    let object = Obj::OptimisticLicensed { claim: id, receipts: signed };
    h.carry(&mut walk, vec![object.clone()]);
    assert!(matches!(walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "S2 licenses the claim");
    let (full, holder) = (h.card_of(&panel.seats[full_index as usize].bond), h.card_of(&panel.seats[holder_index as usize].bond));
    let (_, read) = palw_false_valid_receipts_of_licence_v1(&object).expect("a licence");
    assert!(read.iter().all(|r| matches!(r, PalwFalseValidReceiptV1::Segmented(_))), "C-3: the form they were signed in");
    let mut kept = Vec::new();
    assert_eq!(admit(&h, &walk, &mut kept, id, read), 2);
    let filings = filings_at(&h, &walk, &capture_proof(&h, &claim), &kept, BYSTANDER);
    assert_eq!(filings.len(), 2, "only the two seats the chain locked are asked");
    assert_eq!(filed_cards(&h, &filings), sorted(vec![full, holder]));
    let (licensed, _) = h.carry(&mut walk, filings.iter().map(|f| f.object.clone()).collect());
    let order: Vec<usize> = filings.iter().map(|f| h.card_of(&f.accused)).collect();
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &order, walk.daa);
}

/// **A held class (the review's medium finding): the filer follows the adjudicator.** The harness
/// has no held-class backend, so the floor is re-marked held through the carriage with the very ladder
/// the adjudicator already reads for it (`class_step_ladder_v1`'s network default): the verdict is
/// unchanged and `class_is_held_v1` is true. The full seat (a full attestation) and the fault
/// segment's partial holder (not a full attestation) are both `File` with `class_held`, node policy
/// names both, and both are convicted — the operator's held-attention decision binds no partial seat
/// through a dissection or a DA default, which the adjudicator already refuses, and says nothing
/// against a located step fault inside the seat's own segment.
#[tokio::test]
async fn t54d_on_a_held_class_the_liable_partial_holder_is_filed_too() {
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let ladder = walk.state.class_step_ladder_v1(&h.floor(), PALW_FALSE_VALID_NETWORK_LADDER_V1);
    assert!(!walk.state.class_is_held_v1(&h.floor()), "the floor is not held");
    walk.state = h.rebuilt(&walk.state, |carriage| {
        carriage.class_step_ladders.insert(h.floor(), ladder);
    });
    assert!(walk.state.class_is_held_v1(&h.floor()), "re-marked held, at the ladder the adjudicator reads");
    let leaf = claim.fault_leaf.expect("a step fault");
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, licence.assignment.segments, leaf).expect("in the cut");
    let (full, holder) = (licence.full_card(), licence.holder_of(segment));
    let filings = filings_at(&h, &walk, &capture_proof(&h, &claim), &v2_licence_receipts(id, &licence), BYSTANDER);
    for filing in filings.iter().filter(|f| [full, holder].contains(&h.card_of(&f.accused))) {
        let card = h.card_of(&filing.accused);
        let Check::File { class_held, full_attestation, .. } = filing.check else { panic!("card {card}: {:?}", filing.check) };
        assert!(class_held, "card {card}: the class is held");
        assert_eq!(full_attestation, card == full);
        assert_eq!(filing.policy, None, "card {card}: named");
    }
    assert_eq!(filed_cards(&h, &filings), sorted(vec![full, holder]));
    let (licensed, _) = h.carry(&mut walk, filings.iter().filter(|f| f.files()).map(|f| f.object.clone()).collect());
    let order: Vec<usize> = filings.iter().filter(|f| f.files()).map(|f| h.card_of(&f.accused)).collect();
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &order, walk.daa);
}
