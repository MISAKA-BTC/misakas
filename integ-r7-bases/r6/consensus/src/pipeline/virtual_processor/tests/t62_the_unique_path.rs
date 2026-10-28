//! **ADR-0152 v3.1 T62 (§8.1, A [M2/S]; J-2, J-6, N8, R9): the unique path as a property.** Every
//! conviction follows one chain of chain facts — claim → committed root → job identity → challenged
//! index → accused signer → objective fault → slash target — and resolves it through the claim
//! record, the liability record (SPEC's resolver) and, past `palw_rcore_plus`, the vesting row with
//! its copied fields (N8), a retired claim included. A child of T46's suite, so every claim, panel,
//! licence, `Final`, retirement and conviction is the real one, through the gate, the acceptance walk
//! and the fold.
//!
//! **The property, checked for every conviction kind this harness produces** — kind 3 before `Final`
//! (the full seat and the fault leaf's partial holder), kind 4 by root (a step fault) and by claim
//! (an output grind), a DA default (DA-7: the producer's S1 and the covering signer's S4), a court
//! default, kind 3 after `Final`, and kind 3 after retirement:
//!
//! 1. **One resolution.** `palw_offence_target_v1` on the state the conviction folds from names the
//!    claim, its class and artifact root, its executor bond, the committed root, the recorded job
//!    identity, the trace root, the attempt lane and the five-seat panel's cut — each equal to what
//!    the producer's signed attempt committed at admission (`verify_binding` rebuilds the root; J1:
//!    the binding's job id is the recorded identity).
//! 2. **Every source agrees (J-2, N8).** Whichever of the claim record, the liability record and the
//!    vesting row the state holds, each carries those same fields; and with the higher sources gone —
//!    the claim retired, and (the fixture J-2 names, X29 keeping the liability record alive on the
//!    live chain) the liability record pruned through the carriage — the resolver reaches the same
//!    path from what is left, the vesting row's copies alone at the end.
//! 3. **The adjudicator reads the same target.** The kind-3 and kind-4 findings carry exactly the
//!    resolver's target, so the gate and the fold judge the path this test resolved.
//! 4. **Index → signer → fault → target.** A step fault's site is the faulted leaf; the resolver's cut
//!    places it in one segment, whose assigned holder and the full seat are liable and no other seat
//!    is; every record names the claim, the accused, the kind and the root (the root for a proven-false
//!    execution, zero for a by-claim or default conviction); and the only bonds charged are the
//!    accused signers and the resolver's executor bond (the producer's tier).
//!
//! Run: cargo test -p kaspa-consensus --lib -- t62_the_unique_path

use super::*;
use kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1;
use kaspa_consensus_core::palw_offence_attribution_v1::{
    PalwClaimSourceKindV1, PalwOffenceTargetV1, palw_false_valid_assigned_mask_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT, palw_da_accusation_message_v2, palw_da_event_index_v1,
};

/// What the producer's signed attempt committed at admission — the path every source must name.
struct Committed<'a> {
    claim: &'a RealClaim,
    class_id: Hash64,
    artifact_root: Hash64,
    executor: PalwBondKeyV2,
    segment_count: u16,
}

fn committed<'a>(h: &H, claim: &'a RealClaim) -> Committed<'a> {
    Committed {
        claim,
        class_id: h.floor(),
        artifact_root: h.artifact_root,
        executor: h.cards[EXECUTOR],
        segment_count: palw_segment_count_v2(PANEL.len() as u16),
    }
}

/// Which sources a state holds for a claim.
#[derive(Debug, PartialEq, Eq)]
struct Sources {
    claim: bool,
    liability: bool,
    vesting: bool,
}

/// **Steps 1 and 2 on one state**: the resolver's target is the committed path, and every source the
/// state holds carries it. Returns the target and which sources were there.
fn resolves(state: &PalwChainStateV2, want: &Committed<'_>, label: &str) -> (PalwOffenceTargetV1, Sources) {
    let claim = want.claim;
    let id = claim.claim_id;
    let attempt = &claim.envelope.attempt;
    // The committed root: the binding rebuilds it (J-3), and the attempt carried it.
    verify_binding_v1(&claim.binding).unwrap_or_else(|e| panic!("{label}: the claim's binding rebuilds its root: {e:?}"));
    assert_eq!(claim.binding.committed_execution_root, attempt.execution_root, "{label}: the binding commits the attempt's root");
    // The job: J1 — the binding answers the anchor the claim recorded.
    assert_eq!(claim.binding.job_context.job_id, claim.anchor, "{label}: the binding's job id is the anchor");

    let target = palw_offence_target_v1(state, &id).unwrap_or_else(|| panic!("{label}: the conviction's target resolves"));
    assert_eq!(
        (target.claim_id, target.class_id, target.artifact_root, target.executor_bond),
        (id, want.class_id, want.artifact_root, want.executor),
        "{label}: claim → class → executor"
    );
    assert_eq!(
        (target.execution_root, target.job_identity, target.trace_root),
        (attempt.execution_root, claim.anchor, attempt.trace_root),
        "{label}: the committed root, the recorded job identity (J-1) and the trace root (R9)"
    );
    assert_eq!(
        (target.lane, target.segment_count),
        (Some(PalwClaimSourceKindV1::Attempt), Some(want.segment_count)),
        "{label}: the attempt lane and the five-seat panel's cut"
    );

    let fields = (want.class_id, want.executor, attempt.execution_root, claim.anchor, attempt.trace_root);
    let record = state.claim(&id);
    if let Some(c) = record {
        assert_eq!((c.class_id, c.bond, c.execution_root, c.job_identity, c.trace_root), fields, "{label}: the claim record");
        assert!(matches!(c.source, PalwClaimSourceV2::Attempt), "{label}: an attempt");
    }
    let liability = state.panel_liability(&id);
    if let Some(row) = liability {
        assert_eq!(
            (row.class_id, row.executor_bond, row.execution_root, row.job_identity, row.trace_root),
            fields,
            "{label}: the liability record (F1's appended fields)"
        );
        assert_eq!((row.free_prompt, row.segment_count), (false, want.segment_count), "{label}: the liability record's lane and cut");
    }
    let vesting = state.vesting_row(&id);
    if let Some(row) = vesting {
        assert_eq!(
            (row.class_id, row.producer_bond, row.execution_root, row.job_identity, row.trace_root),
            fields,
            "{label}: the vesting row's copies (N8)"
        );
        assert_eq!(
            (row.free_prompt, row.segment_count, row.artifact_root),
            (false, want.segment_count, want.artifact_root),
            "{label}: the vesting row's lane, cut and artifact root (N8)"
        );
    }
    (target, Sources { claim: record.is_some(), liability: liability.is_some(), vesting: vesting.is_some() })
}

/// The J-6 fields of a target, without the ones a lower source does not carry (`phase`, and the
/// output root a vesting row does not record).
#[allow(clippy::type_complexity)]
fn path_of(
    t: &PalwOffenceTargetV1,
) -> (Hash64, Hash64, Hash64, PalwBondKeyV2, Hash64, Option<PalwClaimSourceKindV1>, Option<u16>, Hash64, Hash64) {
    (t.claim_id, t.class_id, t.artifact_root, t.executor_bond, t.execution_root, t.lane, t.segment_count, t.job_identity, t.trace_root)
}

/// The cards whose collateral a block took.
fn charged(h: &H, before: &PalwChainStateV2, after: &PalwChainStateV2) -> BTreeSet<usize> {
    (0..h.cards.len())
        .filter(|card| after.bond(&h.cards[*card]).unwrap().collateral < before.bond(&h.cards[*card]).unwrap().collateral)
        .collect()
}

/// The kind-3 adjudicator's target for `card`'s receipt on the claim.
fn kind3_target(h: &H, state: &PalwChainStateV2, licence: &Licence, card: usize, claim: Hash64, c: &C) -> PalwOffenceTargetV1 {
    let payload = borsh::to_vec(&h.v2_payload(card, claim, licence.segmented(card), c.clone())).unwrap();
    palw_check_panel_false_valid_v2(state, &h.cards[card], &payload, false, false, h.rules(), None)
        .unwrap_or_else(|e| panic!("card {card} is liable: {e:?}"))
        .target
}

/// Card `card`'s signed DA event accusation of `(row, 0)` on `claim` (M3's `accuse`).
fn da_accuse(h: &H, claim: Hash64, card: usize, row: u32) -> Obj {
    let index = palw_da_event_index_v1(row, 0);
    let message = palw_da_accusation_message_v2(h.domain, &claim, index, &h.cards[card]);
    Obj::DefaultAccused {
        claim,
        missing_event_index: index,
        accuser: h.cards[card],
        signature: sign(card, message.as_byte_slice(), PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT),
    }
}

/// **T62 before `Final`: kind 3, kind 4 (by root and by claim) and a DA default resolve one path.**
#[tokio::test]
async fn t62_before_final_every_conviction_kind_resolves_one_path() {
    let h = harness(true);

    // Kind 3: the full seat and the fault leaf's partial holder.
    {
        let (mut walk, claim, licence) = h.licensed(Fault::Step);
        let (id, c, want) = (claim.claim_id, claim.contradiction(), committed(&h, &claim));
        let (target, sources) = resolves(&walk.state, &want, "kind 3 before Final");
        assert_eq!(sources, Sources { claim: true, liability: false, vesting: false }, "a live, licensed claim");
        let full = licence.full_card();
        assert_eq!(kind3_target(&h, &walk.state, &licence, full, id, &c), target, "the adjudicator reads the resolver's target");
        let payload = borsh::to_vec(&h.v2_payload(full, id, licence.segmented(full), c.clone())).unwrap();
        let finding = palw_check_panel_false_valid_v2(&walk.state, &h.cards[full], &payload, false, false, h.rules(), None).unwrap();
        let PalwFaultSiteV1::Leaf { leaf, .. } = finding.site else { panic!("a step fault's site is a leaf: {:?}", finding.site) };
        assert_eq!(Some(leaf), claim.fault_leaf, "the challenged index is the faulted leaf");
        let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, target.segment_count.unwrap(), leaf)
            .expect("the resolver's cut places the leaf");
        let holder = licence.holder_of(segment);
        let mask = |card: usize| palw_false_valid_assigned_mask_v1(&walk.state, &id, &h.cards[card]).expect("a panel");
        assert!(mask(full).covers(segment) && mask(holder).covers(segment), "the full seat and the holder attested the index");
        for other in licence.partials().into_iter().filter(|card| *card != holder) {
            assert!(!mask(other).covers(segment), "card {other} did not attest segment {segment}");
        }
        let (before, _) = h.carry(
            &mut walk,
            vec![h.v2(full, id, licence.segmented(full), c.clone()), h.v2(holder, id, licence.segmented(holder), c)],
        );
        for card in [full, holder] {
            let record = walk.state.consumed_offence(&palw_false_valid_offence_id_v2(&h.cards[card].0, &id)).expect("a kind-3 record");
            assert_eq!(
                (record.kind, record.accused, record.claim_id, record.execution_root),
                (PalwOffenceKindV1::PanelFalseValidV2, h.cards[card].0, target.claim_id, target.execution_root),
                "card {card}: the record names the claim, the signer and the proven-false root"
            );
        }
        assert_eq!(
            charged(&h, &before, &walk.state),
            BTreeSet::from([EXECUTOR, full, holder]),
            "S4 on the two signers, S2 on the executor"
        );
    }

    // Kind 4 by root: the step fault refutes the executor.
    {
        let (mut walk, claim, _) = h.licensed(Fault::Step);
        let (id, c, want) = (claim.claim_id, claim.contradiction(), committed(&h, &claim));
        let (target, _) = resolves(&walk.state, &want, "kind 4 by root");
        let finding = h.judge_refuted(&walk.state, id, c.clone()).expect("the executor is refuted");
        assert_eq!((finding.target.clone(), finding.forfeit), (target.clone(), PalwForfeitScopeV1::ByRoot));
        let (before, _) = h.carry(&mut walk, vec![h.refuted(id, c)]);
        let record =
            walk.state.consumed_offence(&palw_executor_refuted_offence_id_v1(&target.executor_bond.0, &id)).expect("a kind-4 record");
        assert_eq!(
            (record.kind, record.accused, record.claim_id, record.execution_root),
            (PalwOffenceKindV1::ExecutorRefuted, target.executor_bond.0, id, target.execution_root),
            "kind 4 by root: the executor, the claim, the root"
        );
        assert_eq!(charged(&h, &before, &walk.state), BTreeSet::from([EXECUTOR]), "kind 4 charges the resolver's executor alone");
    }

    // Kind 4 by claim: the output grind (the root may be an honest lender's, so the record's is zero).
    {
        let (mut walk, claim, _) = h.licensed(Fault::OutputGrind);
        let (id, c, want) = (claim.claim_id, claim.contradiction(), committed(&h, &claim));
        let (target, _) = resolves(&walk.state, &want, "kind 4 by claim");
        let finding = h.judge_refuted(&walk.state, id, c.clone()).expect("the ground root is not the run's");
        assert_eq!((finding.target.clone(), finding.forfeit), (target.clone(), PalwForfeitScopeV1::ByClaim));
        let (before, _) = h.carry(&mut walk, vec![h.refuted(id, c)]);
        let record =
            walk.state.consumed_offence(&palw_executor_refuted_offence_id_v1(&target.executor_bond.0, &id)).expect("a kind-4 record");
        assert_eq!(
            (record.kind, record.accused, record.claim_id, record.execution_root),
            (PalwOffenceKindV1::ExecutorRefuted, target.executor_bond.0, id, Hash64::default()),
            "kind 4 by claim: the executor and the claim, no root"
        );
        assert!(!walk.state.palw_execution_root_is_forfeited_v1(&target.execution_root), "by claim: the root is not forfeit");
        assert_eq!(charged(&h, &before, &walk.state), BTreeSet::from([EXECUTOR]));
    }

    // A DA default (DA-7): a partial seat's session runs out; the producer is S1, the covering signer S4.
    {
        let (mut walk, claim, licence) = h.licensed(Fault::Honest);
        let (id, want) = (claim.claim_id, committed(&h, &claim));
        let seat = licence.partials()[0];
        h.carry(&mut walk, vec![da_accuse(&h, id, seat, 0)]);
        let deadline = walk.state.da_session(&id, &h.cards[seat]).expect("an open session").deadline_daa;
        let (target, _) = resolves(&walk.state, &want, "DA default");
        let before = walk.state.clone();
        let point = walk.at(deadline + 1);
        let next = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
        walk.advance(&point, next);
        assert!(
            matches!(
                walk.state.claim(&id).unwrap().phase,
                PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }
            ),
            "the default voids the claim"
        );
        let record = walk.state.consumed_offence(&palw_da_offence_id_v1(&target.executor_bond.0, &id)).expect("a DaDefault record");
        assert_eq!(
            (record.kind, record.accused, record.claim_id, record.execution_root),
            (PalwOffenceKindV1::DaDefault, target.executor_bond.0, id, Hash64::default()),
            "the DA default names the resolver's executor and the claim"
        );
        let full = licence.full_card();
        let signer =
            walk.state.consumed_offence(&palw_false_valid_offence_id_v2(&h.cards[full].0, &id)).expect("the covering signer's S4");
        assert_eq!((signer.accused, signer.claim_id), (h.cards[full].0, id));
        assert_eq!(
            charged(&h, &before, &walk.state),
            BTreeSet::from([EXECUTOR, full]),
            "S1 on the executor, S4 on the full mask only (C7)"
        );
        let (after, sources) = resolves(&walk.state, &want, "DA default, voided");
        assert_eq!(sources, Sources { claim: true, liability: true, vesting: false }, "the void writes the liability record");
        assert_eq!(path_of(&after), path_of(&target), "the void leaves the path where it was");
    }
}

/// **T62 after `Final` and after retirement: the three sources agree, and the lower ones resolve the
/// same path alone** — kind 3 after `Final` (claim record, liability record and vesting row), and a
/// retired claim (liability record and vesting row; then, pruned through the carriage, the vesting
/// row's copies alone).
#[tokio::test]
async fn t62_after_final_and_retirement_every_source_resolves_the_same_path() {
    let h = harness(true);
    assert!(kaspa_consensus_core::palw_state_v2::PALW_RCORE_VESTING_ROWS_LANDED_V1, "the rows landed (IA-7)");

    // After Final: three sources.
    {
        let (mut walk, claim, licence) = h.licensed(Fault::Step);
        let (id, c, want) = (claim.claim_id, claim.contradiction(), committed(&h, &claim));
        let licensed = resolves(&walk.state, &want, "licensed").0;
        h.sweep_to_final(&mut walk, id);
        let (target, sources) = resolves(&walk.state, &want, "kind 3 after Final");
        assert_eq!(sources, Sources { claim: true, liability: true, vesting: true }, "Final writes the liability record and the row");
        assert_eq!(path_of(&target), path_of(&licensed), "Final moves no step of the path");
        let full = licence.full_card();
        assert_eq!(kind3_target(&h, &walk.state, &licence, full, id, &c), target);
        let (before, _) = h.carry(&mut walk, vec![h.v2(full, id, licence.segmented(full), c)]);
        let record = walk.state.consumed_offence(&palw_false_valid_offence_id_v2(&h.cards[full].0, &id)).expect("a kind-3 record");
        assert_eq!((record.accused, record.claim_id, record.execution_root), (h.cards[full].0, id, target.execution_root));
        assert!(walk.state.vesting_row(&id).is_none(), "the row is burned");
        assert_eq!(charged(&h, &before, &walk.state), BTreeSet::from([EXECUTOR, full]), "S4 on the seat, S3 on the executor");
    }

    // After retirement: the claim record is gone; the liability record and the row (held by the
    // second clock) are left.
    {
        let (mut walk, claim, licence) = h.licensed(Fault::Step);
        let (id, c, want) = (claim.claim_id, claim.contradiction(), committed(&h, &claim));
        h.sweep_to_final(&mut walk, id);
        let at_final = resolves(&walk.state, &want, "at Final").0;
        let PalwClaimPhaseV2::Final { final_daa } = walk.state.claim(&id).unwrap().phase else { unreachable!("Final") };
        let point = walk.at(final_daa + h.sp().claim_retirement_daa() + 1);
        let retired = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
        walk.advance(&point, retired);
        let (target, sources) = resolves(&walk.state, &want, "retired");
        assert_eq!(
            sources,
            Sources { claim: false, liability: true, vesting: true },
            "retired: the liability record and the held row"
        );
        assert_eq!(
            (target.phase.clone(), target.output_root),
            (None, claim.envelope.attempt.output_root),
            "the liability record's reading"
        );
        assert_eq!(path_of(&target), path_of(&at_final), "retirement moves no step of the path");

        // The fixture J-2 names: the liability record pruned, the vesting row alone.
        let pruned = h.rebuilt(&walk.state, |carriage| {
            carriage.panel_liabilities.remove(&id).expect("the liability record");
        });
        let (from_row, sources) = resolves(&pruned, &want, "the vesting row alone");
        assert_eq!(sources, Sources { claim: false, liability: false, vesting: true });
        assert_eq!(path_of(&from_row), path_of(&target), "the row's copies resolve the same path (N8)");
        assert_eq!(from_row.output_root, Hash64::default(), "a row records no output root");

        // The conviction against the retired claim resolves from the liability record.
        let full = licence.full_card();
        assert_eq!(
            kind3_target(&h, &walk.state, &licence, full, id, &c),
            target,
            "the adjudicator reads the liability record's target"
        );
        let (before, _) = h.carry(&mut walk, vec![h.v2(full, id, licence.segmented(full), c)]);
        let record = walk.state.consumed_offence(&palw_false_valid_offence_id_v2(&h.cards[full].0, &id)).expect("a kind-3 record");
        assert_eq!(
            (record.kind, record.accused, record.claim_id, record.execution_root),
            (PalwOffenceKindV1::PanelFalseValidV2, h.cards[full].0, id, target.execution_root)
        );
        assert!(walk.state.vesting_row(&id).is_none(), "the held row is burned");
        assert_eq!(
            charged(&h, &before, &walk.state),
            BTreeSet::from([EXECUTOR, full]),
            "S4 on the seat, S3 on the resolver's executor"
        );
    }
}

/// **T62, a court default: the void lands on the path the resolver names.** An honest claim whose
/// executor stays silent in a court (T46s's run): before the court and after the void, the resolver
/// names the same path, the void binds the claim, and the resolver's executor is the only bond charged.
#[tokio::test]
async fn t62_a_court_default_lands_on_the_resolved_path() {
    let h = harness(true);
    let run = court_default_on(&h);
    let want = committed(&h, &run.claim);
    let (in_court, sources) = resolves(&run.in_court, &want, "in court");
    assert_eq!(sources, Sources { claim: true, liability: false, vesting: false });
    let (after, sources) = resolves(&run.walk.state, &want, "court default");
    assert_eq!(sources, Sources { claim: true, liability: true, vesting: false }, "the void writes the liability record");
    assert_eq!(path_of(&after), path_of(&in_court), "the court moves no step of the path");
    assert!(run.walk.state.palw_void_binds_claim_v1(&after.claim_id, run.reason, run.voided_daa), "the void binds the claim");
    assert_eq!(charged(&h, &run.in_court, &run.walk.state), BTreeSet::from([EXECUTOR]), "the resolver's executor alone pays");
}
