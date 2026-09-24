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
//! kaspad's chain walk hands the filer), the selection (`palw_false_valid_filings_v1`) and, per
//! signer, the processor's read (`palw_false_valid_filing_check_v1_at`: the adjudicator, the gate, the
//! fold on the tip) — the same read kaspad asks through `ConsensusApi`. The proof is the capture
//! arm's: the floor's `refutation_for_index` of the lying capture at the faulted leaf, with its
//! operand openings, in the network's carriage (`H::carried`, the sampler's
//! `palw_refutation_prompt_carriage_v1`). What does NOT run here is kaspad itself — the book, the
//! chain walk and the seam are `kaspad::palw_filer_false_valid`'s unit tests; the seam queues the
//! filing's `object` unchanged, which those tests pin. A node e2e on a devnet preset is post-launch
//! (the operator moved every drill and node launch after the t12 launch, 2026-09-24).
use super::*;
use kaspa_consensus_core::palw_false_valid_filing_v1::{
    PalwFalseValidFilingCheckV1 as Check, PalwFalseValidFilingV1, PalwFalseValidPolicyV1, PalwFalseValidProofV1,
    palw_false_valid_filings_v1,
};
use kaspa_consensus_core::palw_offence_attribution_v1::palw_false_valid_receipts_of_licence_v1;

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
        |object| h.vp().palw_false_valid_filing_check_v1_at(&walk.state, &point, object),
    )
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
/// producer. Asked again after, both are `ConvictedBefore` — the node never files an offence twice —
/// and a reward the conviction opened names the `evidence_id` the seam hands the reporter filer.
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
            assert!(filing.settles() && !filing.files());
        }
    }
    let objects: Vec<Obj> = filings.iter().filter(|f| f.files()).map(|f| f.object.clone()).collect();
    let (licensed, _) = h.carry(&mut walk, objects);
    let filed: Vec<usize> = filings.iter().filter(|f| f.files()).map(|f| h.card_of(&f.accused)).collect();
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &filed, walk.daa);
    for filing in filings.iter().filter(|f| f.files()) {
        if let Some(pending) = walk.state.reward_pending(&filing.offence_id) {
            assert_eq!(pending.evidence_id, filing.evidence_id, "the reward rests on the evidence the seam recorded (N12)");
        }
    }
    // Asked again: the chain has both; the rest are still not liable. Nothing files twice.
    let again = filings_at(&h, &walk, &proof, &receipts, BYSTANDER);
    assert!(again.iter().all(|f| !f.files()), "{:?}", again.iter().map(|f| &f.check).collect::<Vec<_>>());
    for filing in &again {
        let card = h.card_of(&filing.accused);
        if card == full || card == holder {
            assert_eq!(filing.check, Check::ConvictedBefore { offence_id: filing.offence_id }, "card {card}");
        }
    }
    h.reloads(&walk.state);
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
/// adjudicator answers every signer `PanelFalseValidNeedsContradiction`, a refusal that settles.
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
        assert!(!filing.files() && filing.settles());
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
            assert!(!filing.files() && !filing.settles(), "{label}");
        }
    }
}
