//! **Phase 2, P2-8: the reporter's commit–reveal filer on real claims** (ADR-0152 v3.1 R-3/R-4,
//! SR-8, J-4, J-6; phase2-plan §4 T18 and T39 node halves, T54c) — a child of the T46 suite, so it
//! runs on that harness's producer-built claims, its three doors (the gate, the acceptance walk,
//! the fold) and its assertions rather than copies of them.
//!
//! What kaspad's filer (`kaspad::palw_panel::reporter_filer`) does is taken the whole way here with
//! the pieces it calls: the capture sampler's refutation at the leaf it drew, graded as the sampler
//! grades it; the ONE builder of kind 4 (`palw_executor_refuted_object_v1`, C-5) and of the reporter's
//! commitment (`palw_reporter_commit_object_v1`), signed by the seat's card; the key the filer keys
//! on (`palw_filed_offence_commit_key_v1`); and the read the filer steps by
//! (`palw_reporter_filing_read_v1_at`: the rows and the processor's own gate at the next block).
//! The filer's own rule — when to commit, file, reveal, re-send and let go — is pinned in kaspad
//! (`reporter_filer::tests`); a node e2e on a devnet preset (plan §4's T54c proper) is a POST-LAUNCH
//! item (the operator deferred every drill and node launch until after the t12 launch, 2026-09-24).
use super::*;
use kaspa_consensus_core::palw_offence_attribution_v1::{palw_executor_refuted_object_v1, palw_filed_offence_commit_key_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwReporterFilingReadV1, palw_reporter_commit_object_v1, palw_reporter_commitment_v1, palw_reporter_reward_amount_v1,
};
use kaspa_consensus_core::palw_vesting_v1::palw_reporter_payout_key_v1;

/// The seat that files: a card of the claim's panel, never its executor (card 0).
const SEAT: usize = PANEL[0];

/// One filing as the node's filer holds it: the evidence object, the key it is consumed under, its
/// evidence id and the salt of its commitment. Shared with the sibling suites of the other two lanes
/// (T54d's P2-8c, T54f's P2-8b), whose filings take this same road since the integration.
pub(super) struct Filing {
    pub(super) object: Obj,
    pub(super) key: Hash64,
    pub(super) evidence_id: Hash64,
    salt: [u8; 32],
}

impl Filing {
    /// The filer's own reading of `object`, which must take a commitment.
    pub(super) fn of(object: Obj, salt: [u8; 32]) -> Self {
        let Obj::ObjectiveOffence { kind, accused, evidence_id, evidence } = &object else { panic!("an objective offence") };
        let key = palw_filed_offence_commit_key_v1(*kind, &accused.0, evidence_id, evidence).expect("a commit-reveal filing");
        let evidence_id = *evidence_id;
        Filing { object, key, evidence_id, salt }
    }

    fn commitment(&self, h: &H, card: usize) -> Hash64 {
        palw_reporter_commitment_v1(&self.key, &self.evidence_id, &h.cards[card], &self.salt)
    }

    /// The node's builder, signed by `card`.
    fn commit(&self, h: &H, card: usize) -> Obj {
        let (commitment, object) =
            palw_reporter_commit_object_v1(&h.domain, &self.key, &self.evidence_id, h.cards[card], &self.salt, |m, c| {
                Some(sign(card, m, c))
            })
            .expect("the builder builds it");
        assert_eq!(commitment, self.commitment(h, card), "one commitment function");
        object
    }

    fn reveal(&self, h: &H, card: usize) -> Obj {
        Obj::ReporterRevealed { offence_key: self.key, reporter: h.cards[card], salt: self.salt }
    }

    /// What `card`'s filer reads at the next block — with the gate on the evidence when `gate`.
    fn read(&self, h: &H, walk: &Walk, card: usize, gate: bool) -> PalwReporterFilingReadV1 {
        let point = walk.next();
        h.vp()
            .palw_reporter_filing_read_v1_at(
                &walk.state,
                point.block,
                point.daa_score,
                &self.key,
                &self.commitment(h, card),
                &h.cards[card],
                gate.then_some(&self.object),
            )
            .expect("a V2 processor reads")
    }
}

/// **The capture arm's `FaultAt` on `claim`'s served capture at `leaf`**, as the sampler builds and
/// grades it (`fp_capture_samples_clear`: the prover's refutation, the class's rows, the network's
/// prompt carriage, `check_execution_step_refutation_opened_capped_v1` at the class ladder — `Ok` is
/// the fault), then kind 4 over it by the node's one builder.
fn capture_arm_fault(h: &H, walk: &Walk, claim: &RealClaim, leaf: u64) -> Obj {
    use kaspa_consensus_core::palw_step_refute::{
        check_execution_step_refutation_opened_capped_v1, palw_refutation_prompt_carriage_v1,
    };
    let refutation = h.backend.refutation_for_index(&claim.material, leaf).expect("the served capture opens the leaf");
    let openings = h.backend.operand_openings_for(&refutation).expect("the class opens the rows");
    let (refutation, prompt_opening) =
        palw_refutation_prompt_carriage_v1(h.form(), refutation).expect("the prover's list is the job's");
    let proven = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, h.artifact_root)
        .expect("the rows prove against the class root");
    check_execution_step_refutation_opened_capped_v1(&refutation, &proven, prompt_opening.as_ref(), h.ladder(&walk.state))
        .expect("the sampler's FaultAt: the leaf does not recompute");
    palw_executor_refuted_object_v1(
        h.cards[EXECUTOR],
        claim.claim_id,
        C::StepArithmetic { refutation, operand_openings: openings },
        prompt_opening,
    )
}

/// One empty block.
fn empty(h: &H, walk: &mut Walk) {
    let point = walk.next();
    let next = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
    walk.advance(&point, next);
}

/// The fold's refusal of a reveal, by its reason.
fn reveal_refused(h: &H, walk: &Walk, reveal: &Obj) -> &'static str {
    assert!(h.accepted(&walk.state, &walk.next(), std::slice::from_ref(reveal)).is_empty(), "the walk drops it");
    match h.fold(&walk.state, &walk.next(), std::slice::from_ref(reveal)) {
        Err(PalwStateV2Error::ReporterRevealRefused { why, .. }) => why,
        other => panic!("the fold refuses the reveal: {other:?}"),
    }
}

/// **The filer's order up to the conviction**: the commitment (the seat's), rooted; one more block
/// (the filer's two-DAA depth); the evidence. Returns the state the conviction folded from and the
/// commitment's DAA.
pub(super) fn commit_then_file(h: &H, walk: &mut Walk, filing: &Filing, card: usize) -> (PalwChainStateV2, u64) {
    commit_then_file_all(h, walk, std::slice::from_ref(filing), card)
}

/// [`commit_then_file`] for every filing one tick hands the filer (P2-8c files one per liable
/// signer): each asked of the gate first, every commitment in one block, then every evidence in one
/// block two DAA later.
pub(super) fn commit_then_file_all(h: &H, walk: &mut Walk, filings: &[Filing], card: usize) -> (PalwChainStateV2, u64) {
    for filing in filings {
        let read = filing.read(h, walk, card, true);
        assert!(read.rcore_plus && read.reporter_may_commit, "R-3 is live and the seat may commit");
        assert_eq!((read.committed_daa, read.consumed.is_some()), (None, false));
        assert_eq!(read.object_gate, Some(Ok(())), "the gate admits the evidence before anything is spent");
    }
    h.carry(walk, filings.iter().map(|filing| filing.commit(h, card)).collect());
    let committed_at = walk.daa;
    for filing in filings {
        assert_eq!(filing.read(h, walk, card, false).committed_daa, Some(committed_at), "the commitment is a row");
    }
    empty(h, walk);
    let (before, _) = h.carry(walk, filings.iter().map(|filing| filing.object.clone()).collect());
    (before, committed_at)
}

/// **R-3 / R-4's tail, as the filer runs it, for every filing convicted in the last block**: each
/// conviction consumed its key on THIS evidence with `card`'s commitment rooted strictly before it
/// (at `committed_at`), and its reward pends for `window_receipt`, positive and within R-1's bound
/// `r · collected` (the exact amount is the fold's: a kind-3 conviction of a licensed claim's seat
/// deducts its share of the extraction, X); `card` reveals them all in one block and leads each; the node's state survives a reload
/// and a restart; the window closes, step 3d moves each award to `card`'s payout in the same block,
/// and the next coinbase drains it. Returns the amounts paid, in `filings`' order.
pub(super) fn reveal_and_paid(h: &H, walk: &mut Walk, filings: &[Filing], card: usize, committed_at: u64) -> Vec<u64> {
    let mut amounts = Vec::new();
    let mut until = 0;
    for filing in filings {
        let record = walk.state.consumed_offence(&filing.key).expect("the conviction's record").clone();
        let read = filing.read(h, walk, card, false);
        assert!(read.committed_daa.is_some_and(|at| at == committed_at && at < record.accepted_daa), "R-3's strict order");
        let pending = read.pending.expect("R-4: the reward pends");
        assert_eq!(
            (pending.evidence_id, pending.reveal_until, pending.best),
            (filing.evidence_id, record.accepted_daa + h.sp().window_receipt(), None),
            "on the consumed evidence (N12), for window_receipt, unrevealed"
        );
        assert!(
            pending.amount > 0 && pending.amount <= palw_reporter_reward_amount_v1(record.collected, 0),
            "R-1: positive, and at most r over the collected debit"
        );
        until = until.max(pending.reveal_until);
        amounts.push(pending.amount);
    }
    h.carry(walk, filings.iter().map(|filing| filing.reveal(h, card)).collect());
    for filing in filings {
        let best = filing.read(h, walk, card, false).pending.and_then(|p| p.best).expect("the reveal leads");
        assert_eq!((best.reporter, best.committed_daa), (h.cards[card], committed_at));
    }
    h.reloads(&walk.state);
    h.restarts(walk.next().block, &walk.state);
    // The window closes; step 3d moves each award in the same block, and the next coinbase drains it.
    let point = walk.at(until + 1);
    let next = h.fold(&walk.state, &point, &[]).expect("the sweep's block folds");
    walk.advance(&point, next);
    let payload = walk.state.bond(&h.cards[card]).unwrap().payout_payload;
    for (filing, amount) in filings.iter().zip(&amounts) {
        let read = filing.read(h, walk, card, false);
        assert!(read.pending.is_none() && read.consumed.is_some(), "swept; the conviction stands");
        let queued = walk.state.pending_payout(&palw_reporter_payout_key_v1(&filing.key)).copied().expect("moved by step 3d");
        assert_eq!((queued.payload, queued.amount), (payload, *amount), "to the reporter's payout");
    }
    h.reloads(&walk.state);
    empty(h, walk);
    for filing in filings {
        assert!(walk.state.pending_payout(&palw_reporter_payout_key_v1(&filing.key)).is_none(), "drained into the next coinbase");
    }
    amounts
}

/// **T18 (node half) + T54c + T39 (node half), at processor level: a capture-arm fault becomes a
/// folded `ExecutorRefuted` with the S2 charge, and its reporter — the seat that found it — is paid.**
/// A real floor claim with the drill's injected step fault, bound (`PanelBound`: the capture arm's
/// moment). The sampler's refutation at the faulted leaf, as kind 4, keys on the per-claim ledger id;
/// the seat commits (the gate admits the evidence first), the commitment roots, the evidence folds
/// two DAA later — the executor voided `CourtFraud` and charged S2 (the forfeit and `min(10%·C₀, 3G)`),
/// the root forfeit (execution-proving) — and the reward pends on THIS evidence for `window_receipt`.
/// A copier that commits the same key and evidence in the conviction's own block (the earliest a
/// mempool observer of the evidence can) is refused at its reveal; the seat's reveal wins, survives a
/// restart of the node's state, and when the window closes step 3d moves the award to the seat's
/// payout key, which the next coinbase drains.
#[tokio::test]
async fn t18_t54c_a_capture_arm_fault_is_committed_filed_revealed_and_paid() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    h.bind(&mut walk, id);
    let filing = Filing::of(capture_arm_fault(&h, &walk, &claim, claim.fault_leaf.expect("a step fault")), [0x5E; 32]);
    assert_eq!(filing.key, palw_executor_refuted_offence_id_v1(&h.cards[EXECUTOR].0, &id), "kind 4's per-claim key");
    // A copier holds the same evidence only once it is public: its commitment lands, at best, in the
    // conviction's own block.
    let copier = BYSTANDER;
    let read = filing.read(&h, &walk, SEAT, true);
    assert_eq!(read.object_gate, Some(Ok(())));
    h.carry(&mut walk, vec![filing.commit(&h, SEAT)]);
    let committed_at = walk.daa;
    empty(&h, &mut walk);
    let (before, _) = h.carry(&mut walk, vec![filing.commit(&h, copier), filing.object.clone()]);
    let daa = walk.daa;
    let root = claim.envelope.attempt.execution_root;
    assert_refuted_before_final(&h, &before, &walk.state, id, daa, root);
    assert!(walk.state.palw_execution_root_is_forfeited_v1(&root), "a proven-false execution's root is forfeit");

    let record = walk.state.consumed_offence(&filing.key).expect("the kind-4 record").clone();
    assert_eq!(record.accepted_daa, daa);
    let pending = filing.read(&h, &walk, SEAT, false).pending.expect("R-4: the reward pends");
    assert_eq!(pending.amount, palw_reporter_reward_amount_v1(record.collected, 0), "R-1 over the collected debit, X = 0 on kind 4");
    assert_eq!(filing.read(&h, &walk, copier, false).committed_daa, Some(daa), "the copier's row is the conviction's DAA");
    assert_eq!(reveal_refused(&h, &walk, &filing.reveal(&h, copier)), "the commitment was made at or after the conviction");
    // The seat's reveal wins, survives a restart, and the award reaches its payout.
    reveal_and_paid(&h, &mut walk, std::slice::from_ref(&filing), SEAT, committed_at);
}

/// **T39 (node half): the reporter is never the accused, and a missing reveal forfeits R only.**
/// The executor may root a commitment to its own conviction — the commitment hides its key — but its
/// reveal is refused (`reporter ≠ accused`, R-3's hygiene), even though it committed first; a seat
/// that files and never reveals leaves the reward pending until the window closes, when it is forgone
/// (`reporter_forgone_sompi` grows by it) and the executor's charge, the void and the record are
/// exactly what the conviction wrote.
#[tokio::test]
async fn t39_the_accused_never_reports_itself_and_a_missing_reveal_forfeits_r_only() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    h.bind(&mut walk, id);
    let filing = Filing::of(capture_arm_fault(&h, &walk, &claim, claim.fault_leaf.unwrap()), [0x39; 32]);
    // The executor commits first, to its own conviction.
    h.carry(&mut walk, vec![filing.commit(&h, EXECUTOR)]);
    commit_then_file(&h, &mut walk, &filing, SEAT);
    let convicted = walk.state.clone();
    let pending = *walk.state.reward_pending(&filing.key).expect("the reward pends");
    assert_eq!(reveal_refused(&h, &walk, &filing.reveal(&h, EXECUTOR)), "the accused cannot report itself");

    // Nobody reveals.
    let forgone_before = walk.state.reporter_counters().forgone_sompi;
    let point = walk.at(pending.reveal_until + 1);
    let next = h.fold(&walk.state, &point, &[]).expect("the sweep's block folds");
    walk.advance(&point, next);
    assert!(walk.state.reward_pending(&filing.key).is_none() && walk.state.reporter_reward(&filing.key).is_none(), "no award");
    assert_eq!(walk.state.reporter_counters().forgone_sompi, forgone_before + u128::from(pending.amount), "forgone");
    let executor = h.cards[EXECUTOR];
    assert_eq!(walk.state.bond(&executor).unwrap().collateral, convicted.bond(&executor).unwrap().collateral, "R only");
    assert_eq!(walk.state.consumed_offence(&filing.key), convicted.consumed_offence(&filing.key), "the record stands");
    assert!(matches!(walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }));
    h.reloads(&walk.state);
}

/// **J1 auto (§3.9's borrowed strategy, J-6) through the filer.** A borrower's claim carries the
/// lender's genuine roots under its own block's anchor; a seat of its panel holds the lender's capture
/// for it (served or gossiped). The node's detector (`reporter_filer::palw_borrowed_root_binding_v1`,
/// pinned in kaspad on the same floor) is these three calls: the capture does not reproduce the
/// claim under the claim's own job, reproduces its committed roots with no job bound, and yields its
/// binding by the out-of-range event opening. `ExecutorRefuted { IdentityMismatch }` over that binding
/// is admitted by the gate — the fold's identity check (J1: the binding answers the lender's anchor)
/// — and goes commit → evidence → reveal: the borrower is voided `CourtFraud` and charged S2, the
/// record's root is 0 (forfeiture by claim), and the lender's root and claim stand.
#[tokio::test]
async fn p2_8_j1_auto_a_borrowed_root_is_refuted_through_the_filer() {
    use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwMaterialVerdictV1};
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let lender = h.open_claim_at_nonce(&mut walk, Fault::Honest, 0);
    let borrower = h.open_borrowed_claim(&mut walk, bucket(1), &lender);
    let id = borrower.claim_id;
    h.bind(&mut walk, id);
    let claimed = walk.state.claim(&id).unwrap().clone();
    let anchored = PalwClaimRootsV1 {
        execution_root: claimed.execution_root,
        trace_root: claimed.trace_root,
        anchor: claimed.job_identity,
        attempt_draw: Some(true),
        output_root: Some(claimed.output_root),
        job_pin: None,
    };
    assert_ne!(h.backend.verify_material(&lender.material, anchored), PalwMaterialVerdictV1::Matches, "not under the claim's job");
    let unbound = PalwClaimRootsV1 { anchor: Hash64::default(), attempt_draw: None, job_pin: None, ..anchored };
    assert_eq!(h.backend.verify_material(&lender.material, unbound), PalwMaterialVerdictV1::Matches, "the claim's committed roots");
    let binding = h.backend.disclose_trace_event(&lender.material, u32::MAX, u8::MAX).expect("the binding opens").binding().clone();
    assert_eq!(binding.job_context.job_id, lender.anchor, "the lender's job");
    let object = palw_executor_refuted_object_v1(h.cards[EXECUTOR], id, C::IdentityMismatch { binding }, None);
    let filing = Filing::of(object, [0x71; 32]);
    let (before, _) = commit_then_file(&h, &mut walk, &filing, SEAT);
    let daa = walk.daa;
    assert_refuted_before_final(&h, &before, &walk.state, id, daa, Hash64::default());
    assert!(!walk.state.palw_execution_root_is_forfeited_v1(&claimed.execution_root), "the root is the lender's: not forfeit");
    assert!(walk.state.claim(&lender.claim_id).is_some_and(|c| !c.phase.is_terminal()), "the lender's claim stands");
    h.carry(&mut walk, vec![filing.reveal(&h, SEAT)]);
    assert_eq!(filing.read(&h, &walk, SEAT, false).pending.and_then(|p| p.best).map(|b| b.reporter), Some(h.cards[SEAT]));
    // The lender's own claim is no candidate: its capture reproduces it under its own job.
    let own = PalwClaimRootsV1 { anchor: lender.anchor, ..anchored };
    assert_eq!(h.backend.verify_material(&lender.material, own), PalwMaterialVerdictV1::Matches);
    h.reloads(&walk.state);
}

/// **A reorg of the commitment.** (1) Rooted, then reverted by its block's delta before the evidence:
/// the filer reads no row and re-sends the SAME commitment (same salt), which roots again later; the
/// evidence follows and the reveal wins. (2) Rooted and convicted, then both blocks reverted, and the
/// other chain carries the evidence first and the commitment after: the reveal is refused — the fold's
/// strict order — so the filer lets R go and the conviction stands.
#[tokio::test]
async fn p2_8_a_commitment_reorged_out_is_re_sent_and_one_rooted_after_the_conviction_cannot_win() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    h.bind(&mut walk, id);
    let filing = Filing::of(capture_arm_fault(&h, &walk, &claim, claim.fault_leaf.unwrap()), [0x0E; 32]);
    let fork = walk.clone();

    // (1) The commitment's block reverted before the evidence.
    let (parent, delta) = h.carry(&mut walk, vec![filing.commit(&h, SEAT)]);
    walk.state = revert_delta_v2(&walk.state, &delta, h.sp()).expect("the commitment's delta reverts");
    assert_eq!(walk.state.state_root(), parent.state_root(), "exactly");
    assert_eq!(filing.read(&h, &walk, SEAT, false).committed_daa, None, "the filer reads no row");
    let (_, committed_at) = commit_then_file(&h, &mut walk, &filing, SEAT);
    assert!(committed_at > parent.last_point().unwrap().daa_score, "re-rooted on the new chain");
    h.carry(&mut walk, vec![filing.reveal(&h, SEAT)]);
    assert!(walk.state.reward_pending(&filing.key).and_then(|p| p.best).is_some_and(|b| b.reporter == h.cards[SEAT]));

    // (2) Both reverted; the other chain convicts first and roots the commitment after.
    let mut walk = fork;
    let (_, commit_delta) = h.carry(&mut walk, vec![filing.commit(&h, SEAT)]);
    let (_, conviction_delta) = h.carry(&mut walk, vec![filing.object.clone()]);
    let reverted = revert_delta_v2(&walk.state, &conviction_delta, h.sp()).expect("the conviction reverts");
    walk.state = revert_delta_v2(&reverted, &commit_delta, h.sp()).expect("the commitment reverts");
    assert!(walk.state.consumed_offence(&filing.key).is_none() && filing.read(&h, &walk, SEAT, false).committed_daa.is_none());
    h.carry(&mut walk, vec![filing.object.clone()]);
    h.carry(&mut walk, vec![filing.commit(&h, SEAT)]);
    let read = filing.read(&h, &walk, SEAT, false);
    let accepted = read.consumed.as_ref().expect("convicted").accepted_daa;
    assert!(read.committed_daa.is_some_and(|at| at > accepted), "the row now follows the conviction");
    assert_eq!(reveal_refused(&h, &walk, &filing.reveal(&h, SEAT)), "the commitment was made at or after the conviction");
    assert!(matches!(walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }));
    h.reloads(&walk.state);
}

/// **The fence-off twins.** With `palw_rcore_plus` off (testnet-12's attribution fence still armed)
/// the filer reads R-3 dormant: the commitment is dropped by name, the capture arm's kind 4 convicts
/// on its own (the forfeit, no S2 action) and no reward opens — the filer's `FiledDirect`. With the
/// attribution fence off too, the gate refuses kind 4 itself, so the capture arm files nothing through
/// the filer and takes the v1 one-move court, byte for byte.
#[tokio::test]
async fn p2_8_fence_off_twins() {
    let h = harness_rcore_off();
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    h.bind(&mut walk, id);
    let filing = Filing::of(capture_arm_fault(&h, &walk, &claim, claim.fault_leaf.unwrap()), [0xF0; 32]);
    let read = filing.read(&h, &walk, SEAT, true);
    assert!(!read.rcore_plus, "R-3 dormant");
    assert_eq!(read.object_gate, Some(Ok(())), "kind 4 stands on the attribution fence alone");
    assert!(h.accepted(&walk.state, &walk.next(), &[filing.commit(&h, SEAT)]).is_empty(), "the commitment is dropped by name");
    let (before, _) = h.carry(&mut walk, vec![filing.object.clone()]);
    assert_refuted_before_final(&h, &before, &walk.state, id, walk.daa, claim.envelope.attempt.execution_root);
    let read = filing.read(&h, &walk, SEAT, false);
    assert!(read.consumed.is_some() && read.pending.is_none(), "convicted; no reward opens below the fence");

    let dormant = harness(false);
    let mut walk = dormant.genesis_walk();
    let claim = dormant.open_claim(&mut walk, Fault::Step);
    dormant.bind(&mut walk, claim.claim_id);
    let filing = Filing::of(capture_arm_fault(&dormant, &walk, &claim, claim.fault_leaf.unwrap()), [0xF1; 32]);
    let read = filing.read(&dormant, &walk, SEAT, true);
    assert!(!read.rcore_plus);
    assert_eq!(read.object_gate, Some(Err(E::AttributionDormant.to_string())), "the gate refuses kind 4: the court's path");
}

/// **A KNOWN GAP, pinned until consensus closes it (the review of P2-8, F1; R-3/N12, the ADR owner's):
/// a copier who re-encodes the honest evidence and gets its copy folded first takes R.** Kind 4's
/// adjudicator verifies each operand opening and takes the first match
/// (`PalwProvenOperandsV1::from_openings_v1`, `find_operand_v1`), so the capture arm's evidence with
/// one opening appended again still convicts — the same offence key, another `evidence_id`, and the
/// gate admits it. The seat commits and waits its two-DAA depth; its evidence is public in the
/// mempool; a bystander commits to the re-encoding one block later and its copy folds before the
/// seat's evidence (fee competition, or a miner leaving the honest carrier out for one block). The
/// conviction consumes the copy: the reward pends on the copy's `evidence_id`, the seat's reveal is
/// refused (its commitment binds its own evidence) and the bystander's leads. The conviction and its
/// charge are unaffected; only R moves. When the fold makes kind-3/4 evidence canonical (or R-3's
/// commitment binds a canonical digest of the contradiction), the gate refuses the copy — or the
/// copy's commitment no longer beats the seat's — and this test is flipped to say so.
#[tokio::test]
async fn p2_8_known_gap_a_re_encoded_copy_folded_first_takes_r() {
    use kaspa_consensus_core::palw_offence_attribution_v1::PalwExecutorRefutedEvidenceV1;
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    h.bind(&mut walk, id);
    let honest = Filing::of(capture_arm_fault(&h, &walk, &claim, claim.fault_leaf.unwrap()), [0xA1; 32]);
    let Obj::ObjectiveOffence { evidence, .. } = &honest.object else { panic!("an objective offence") };
    let payload: PalwExecutorRefutedEvidenceV1 = borsh::from_slice(evidence).expect("kind 4's evidence");
    let C::StepArithmetic { refutation, mut operand_openings } = payload.contradiction else { panic!("the capture arm's fault") };
    operand_openings.push(operand_openings[0].clone());
    let copy = Filing::of(
        palw_executor_refuted_object_v1(
            h.cards[EXECUTOR],
            id,
            C::StepArithmetic { refutation, operand_openings },
            payload.prompt_ids_opening,
        ),
        [0xC0; 32],
    );
    assert_eq!(copy.key, honest.key, "one offence");
    assert_ne!(copy.evidence_id, honest.evidence_id, "two encodings of it");
    assert_eq!(copy.read(&h, &walk, BYSTANDER, true).object_gate, Some(Ok(())), "the gate admits the re-encoding");

    h.carry(&mut walk, vec![honest.commit(&h, SEAT)]);
    let seat_committed_at = walk.daa;
    empty(&h, &mut walk);
    // The seat's evidence is in the mempool now; the copy commits, and folds first.
    h.carry(&mut walk, vec![copy.commit(&h, BYSTANDER)]);
    h.carry(&mut walk, vec![copy.object.clone()]);
    let pending = walk.state.reward_pending(&honest.key).copied().expect("the reward pends");
    assert_eq!(pending.evidence_id, copy.evidence_id, "on the copy's evidence");
    assert!(honest.read(&h, &walk, SEAT, false).committed_daa.is_some_and(|at| at == seat_committed_at), "the seat committed first");
    assert_eq!(
        reveal_refused(&h, &walk, &honest.reveal(&h, SEAT)),
        "no commitment of this reporter opens to this key, evidence and salt"
    );
    h.carry(&mut walk, vec![copy.reveal(&h, BYSTANDER)]);
    let best = walk.state.reward_pending(&honest.key).and_then(|p| p.best).expect("a reveal leads");
    assert_eq!(best.reporter, h.cards[BYSTANDER], "the copier's");
    assert!(matches!(walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }));
}
