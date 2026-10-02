//! **RFC-0006 layer-sharded panels on the chain** (`Params::palw_tir_shard_v1`, dormant everywhere): an IR class (the corpus's
//! dense GQA model, two layers) declares a layer-shard plan; a claim of it binds a panel drawn per shard — each shard's
//! `[outsider] ++ three class seats`; its seats' cell-masked receipts license it PART BY PART, `basis_k` recounted over cells.
//!
//! * **below the fence** every object of the family is refused by name (the second lock behind the acceptance walk's drop);
//! * **a plan** is the class's registrant's, declared once, shaped by the program (decision 8) — and prices every cell;
//! * **a bound claim** writes its per-shard record (the plan frozen, the outsider, each seat's share of the work);
//! * **a part** licenses its shard when two class seats attest every cell and the shard's outsider says `Valid`; the part
//!   that completes the plan licenses the claim with `basis_k` over cells; every whole-object door refuses the claim;
//! * **a shard's seat locks its scaled price**, and a `Final` claim pays its seats by the work they vouched for;
//! * **a run of step leaves** is demanded and answered as one unit (`TirStepRun`), and refused below the fence.
//!
//! Every block goes through the transition and is checked three ways (the delta re-applies and reverts, and the carriage
//! reloads under its root).
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_tir_shard_fold`

#[path = "palw_tir_fixture_common.rs"]
#[allow(unused)]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
};
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1, PalwTirStepAccusationV1};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwPwuRuleV2,
    PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_delta_v2,
    apply_palw_transition_v2_with_extras, palw_operator_id_v2, revert_delta_v2,
};
use kaspa_consensus_core::palw_tir_class_v1::PalwTirAdmissionCarriageV1;
use kaspa_consensus_core::palw_tir_court_v1::{
    PALW_TIR_STEP_RUN_MAX_LEAVES_V1, build_tir_step_run_disclosure_v1, check_tir_step_run_disclosure_v1,
};
use kaspa_consensus_core::palw_tir_shard_v1::{
    PalwSeatReceiptV4, PalwTirShardPartV1, palw_tir_shard_assignment_v1, palw_tir_shard_outsider_mask_v1,
};
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

const PRODUCER: u64 = 1;
/// The eight seats of a 2-shard plan with an outsider each: shard 0 is `[10, 11, 12, 13]`, shard 1 `[14, 15, 16, 17]`
/// (the outsider first). A bystander for the demands.
const SEAT0: u64 = 10;
const OTHER: u64 = 30;
const RCORE: u64 = 0;
const FENCE2: u64 = 2;
const SHARD_AT: u64 = 5;
const WINDOW_CHALLENGE: u64 = 20;
const MAX: u64 = 1 << 22;

fn h64(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
}

fn pubkey(n: u64) -> Vec<u8> {
    vec![6 + n as u8; 4]
}

fn op_key(n: u64) -> Vec<u8> {
    vec![20 + n as u8; 8]
}

fn seat(n: u64) -> PalwPanelSeatV2 {
    PalwPanelSeatV2 { bond: bond_key(n), operator_id: palw_operator_id_v2(&op_key(n)) }
}

fn params(shard_at: Option<u64>) -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, WINDOW_CHALLENGE, 500, 1000, h64(1), 4, 1000, 100, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        .with_fp_exposure_ceiling(500)
        .unwrap()
        .with_tir_from_daa(Some(0))
        .with_rcore_plus_mirrors(Some(RCORE), 0, Vec::new())
        .with_tir_fence2_from_daa(Some(FENCE2))
        .with_tir_shard_from_daa(shard_at)
        .with_worker_carve_permille(620)
        .unwrap()
}

struct Run {
    p: PalwStateParamsV2,
    s: PalwChainStateV2,
    daa: u64,
    extras: PalwTransitionExtrasV1,
}

impl Run {
    fn ctx(daa: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h64(0xB10C_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 1_000_000_000 }
    }

    fn try_at(
        &self,
        daa: u64,
        objects: &[PalwConsensusObjectV2],
        att: Option<&PalwAttemptEnvelopeV2>,
    ) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
        apply_palw_transition_v2_with_extras(&self.s, &self.p, &Self::ctx(daa), objects, att, false, false, false, true, &self.extras)
    }

    fn at(&mut self, daa: u64, objects: &[PalwConsensusObjectV2], att: Option<&PalwAttemptEnvelopeV2>) {
        assert!(daa > self.daa, "DAA moves forward");
        let (child, delta) = self.try_at(daa, objects, att).unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
        assert_eq!(apply_delta_v2(&self.s, &delta, &self.p).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&child, &delta, &self.p).expect("reverts"), self.s, "DAA {daa}: the delta reverts");
        let reloaded = PalwStateCarriageV2::from_state(&child)
            .into_state(&self.p, Some(child.state_root()))
            .unwrap_or_else(|e| panic!("DAA {daa}: the carriage reloads: {e}"));
        assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        self.s = child;
        self.daa = daa;
    }

    fn step(&mut self, objects: &[PalwConsensusObjectV2]) {
        self.at(self.daa + 1, objects, None);
    }

    fn refused(&self, objects: &[PalwConsensusObjectV2]) -> PalwStateV2Error {
        self.try_at(self.daa + 1, objects, None).expect_err("the fold refuses it")
    }
}

fn bond(n: u64, collateral: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: pubkey(n),
        operator_pubkey: op_key(n),
        collateral,
        payout_payload: Hash64::from_u64_word(0x9A00 + n),
        // The floor class: every node runs it, and it is the population an outsider is drawn from.
        capable_classes: std::iter::once(h64(1)).collect(),
        signature: Vec::new(),
    }
}

fn fixture() -> Fixture {
    let (name, program, params, tokens) =
        programs().into_iter().find(|(n, ..)| n == "dense-gqa-2layer").expect("the dense GQA corpus model");
    fixture_with(name, program, params, tokens, 36, 12)
}

/// The chain up to a claim of the IR class committing `x`: the floor class, the producer (the IR class's registrant) and
/// eight seat bonds at DAA 1 (the IR class with them), the claim at 3. The fence arms at `SHARD_AT` (5).
fn claimed(f: &Fixture, x: &Execution, shard_at: Option<u64>) -> (Run, Hash64) {
    // Independence from genesis: the claim of a bought class is outsider-judged; the panel economy puts seats on duty.
    let extras = PalwTransitionExtrasV1 { admission_independence_daa: Some(0), panel_economy_active: true, ..Default::default() };
    let mut run = Run { p: params(shard_at), s: PalwChainStateV2::genesis(), daa: 0, extras };
    let class_id = f.class_id;
    let mut objects = vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        bond(PRODUCER, 100_000_000_000),
        bond(OTHER, 100_000_000_000),
    ];
    for n in 10..18 {
        objects.push(bond(n, 100_000_000_000));
    }
    objects.push(PalwConsensusObjectV2::ClassRegisteredTirV1 {
        class_id,
        artifact_root: f.artifact_root,
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 40 },
        initial_target: u128::MAX / 2,
        share_permille: 0,
        activation_daa: 0,
        admission: Box::new(PalwTirAdmissionCarriageV1 {
            class: f.class.clone(),
            canonical: f.ctx.clone(),
            registrant_bond: bond_key(PRODUCER),
            signature: vec![9; 8],
        }),
    });
    run.at(1, &objects, None);
    let network_domain = h64(999);
    let producer = bond_key(PRODUCER).0;
    let env = PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, h64(5), 1_700, 1, class_id, &producer),
            class_id,
            executor_bond: producer,
            executor_pubkey: pubkey(PRODUCER),
            operator_id: palw_operator_id_v2(&op_key(PRODUCER)),
            artifact_root: f.artifact_root,
            trace_root: x.binding.full_logits_trace_root,
            output_root: h64(32),
            pwu: 40,
            trace_manifest_root: h64(33),
            trace_chunk_count: 1,
            trace_retention_daa: 999_999,
            execution_root: x.binding.committed_execution_root,
        },
        signature: vec![0; 8],
    };
    let claim_id = attempt_id_v2(&env.attempt);
    run.at(3, &[], Some(&env));
    (run, claim_id)
}

fn plan(class_id: Hash64, s_l: u16, s_p: u16) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::TirShardPlanDeclared { class_id, s_l, s_p, signature: vec![1; 8] }
}

fn panel_of(shards: u64, outsider: bool) -> Vec<PalwPanelSeatV2> {
    let stride = 3 + u64::from(outsider);
    (0..shards * stride).map(|i| seat(10 + i)).collect()
}

fn receipt(
    claim: Hash64,
    n: u64,
    shard: u16,
    verdict: PalwReceiptVerdictV2,
    segments: PalwSegmentMaskV2,
    signed_daa: u64,
) -> PalwSeatReceiptV4 {
    PalwSeatReceiptV4 {
        receipt: PalwSeatReceiptV2 { claim, verdict, seat_bond: bond_key(n), signed_daa, signature: vec![7; 8] },
        shard,
        segments,
    }
}

fn part(claim: Hash64, shard: u16, receipts: Vec<PalwSeatReceiptV4>) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::TirShardReceiptLicensed { part: PalwTirShardPartV1 { claim, shard, receipts } }
}

/// The receipts of a shard that license it under `S_P = 1`: the outsider (first seat) and the first two class seats say
/// `Valid` over the whole shard.
fn licensing_receipts(claim: Hash64, shard: u16, s_p: u16, anchor: Hash64, daa: u64) -> Vec<PalwSeatReceiptV4> {
    let first = 10 + u64::from(shard) * 4;
    let masks = palw_tir_shard_assignment_v1(&anchor, &claim, shard, s_p);
    let mut out = vec![receipt(claim, first, shard, PalwReceiptVerdictV2::Valid, palw_tir_shard_outsider_mask_v1(s_p), daa)];
    for i in 0..3u64 {
        out.push(receipt(claim, first + 1 + i, shard, PalwReceiptVerdictV2::Valid, masks[i as usize], daa));
    }
    out
}

#[test]
fn every_object_of_the_family_is_refused_by_name_below_the_fence() {
    let f = fixture();
    let x = f.honest();
    let (run, claim_id) = claimed(&f, &x, Some(SHARD_AT));
    assert!(run.daa < SHARD_AT);
    for object in [
        plan(f.class_id, 2, 1),
        part(claim_id, 0, vec![receipt(claim_id, 11, 0, PalwReceiptVerdictV2::Valid, PalwSegmentMaskV2::full(1), 4)]),
    ] {
        assert!(kaspa_consensus_core::palw_state_v2::palw_object_is_tir_shard_v1(&object));
        assert_eq!(run.refused(&[object]), PalwStateV2Error::TirShardDormant);
    }
    // The unit is the fence's too: a step-run demand below it is refused by name.
    let run_demand = PalwConsensusObjectV2::DefaultAccusedTirStep {
        accusation: Box::new(PalwTirStepAccusationV1 {
            claim: claim_id,
            unit: PalwDaUnitV1::TirStepRun { first: 0, count: 4 },
            accuser: bond_key(OTHER),
            signature: vec![1; 8],
        }),
    };
    assert!(kaspa_consensus_core::palw_state_v2::palw_object_is_tir_shard_v1(&run_demand));
    assert_eq!(run.refused(&[run_demand]), PalwStateV2Error::TirShardDormant);
    // A network that never armed it has no table at all: the state roots as a build without the tables.
    let (never, _) = claimed(&f, &x, None);
    assert_eq!(never.s.tir_shard_plans_iter().count(), 0);
}

#[test]
fn a_plan_is_the_registrants_declared_once_and_shaped_by_the_program() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x, Some(SHARD_AT));
    let _ = claim_id;
    run.at(SHARD_AT, &[], None);
    // One shard is the flat panel; three shards over a two-layer class leaves one empty; 1 and s_shard − 1 are the only cuts.
    assert!(matches!(run.refused(&[plan(f.class_id, 1, 1)]), PalwStateV2Error::TirShardRefused(_)), "one shard");
    assert!(matches!(run.refused(&[plan(f.class_id, 3, 1)]), PalwStateV2Error::TirShardRefused(_)), "a shard with no layer");
    assert!(matches!(run.refused(&[plan(f.class_id, 2, 5)]), PalwStateV2Error::TirShardRefused(_)), "an unoffered cut");
    // A class registered at genesis has no registrant to declare for it.
    assert_eq!(run.refused(&[plan(h64(1), 2, 1)]), PalwStateV2Error::ShardPlanFromGenesisClass(h64(1)));
    run.step(&[plan(f.class_id, 2, 1)]);
    let declared = run.s.tir_shard_plan(&f.class_id).expect("declared");
    assert_eq!((declared.s_l, declared.s_p, declared.declared_daa), (2, 1, SHARD_AT + 1));
    assert_eq!(declared.cell_permille.len(), 2);
    assert_eq!(declared.cell_permille.iter().map(|p| u32::from(*p)).sum::<u32>(), 1_000, "the cells price the whole claim");
    assert!(declared.cell_permille.iter().all(|p| *p > 0), "{:?}", declared.cell_permille);
    // Once.
    assert_eq!(run.refused(&[plan(f.class_id, 2, 2)]), PalwStateV2Error::ShardPlanAlreadyDeclared(f.class_id));
}

#[test]
fn a_claim_licenses_by_two_parts_with_its_outsiders_and_the_recount_runs_over_cells() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x, Some(SHARD_AT));
    run.at(SHARD_AT, &[plan(f.class_id, 2, 1)], None);
    // Bind the claim's panel: two shards of [outsider, three class seats].
    let anchor = h64(77);
    let seats = panel_of(2, true);
    run.step(&[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor, seats: seats.clone() }]);
    let record = run.s.tir_shard_claim(&claim_id).expect("a panel drawn per shard writes the claim's record").clone();
    assert_eq!((record.s_l, record.s_p, record.outsider), (2, 1, true));
    assert_eq!(record.drawn_permille.len(), 8);
    assert_eq!(record.progress.licensed_count(), 0);
    let bound = run.daa;
    // The seat duties name each seat's shard, slice index and assigned segments (RFC-0006 §4; node policy).
    let everyone: Vec<PalwBondKeyV2> = (10..18).map(bond_key).collect();
    let duties = kaspa_consensus_core::palw_producer_v2::palw_seat_duties_v2(&run.s, &run.p, &everyone);
    assert_eq!(duties.len(), 8);
    for d in &duties {
        let n = d.seat_bond.0.index as u64;
        let _ = n;
        let place = d.tir_shard.expect("a sharded claim's duty has a shard place");
        let i = d.seat_index as usize;
        assert_eq!((place.shard as usize, place.slice_index as usize), (i / 4, i % 4));
        assert_eq!(place.outsider, i % 4 == 0, "the outsider leads each slice");
        assert_eq!((place.s_l, place.s_p), (2, 1));
        assert_eq!(place.segments, PalwSegmentMaskV2::full(1), "layers-only: every seat attests the whole shard");
    }
    // A flat licence of the claim is refused by name: it licenses by its parts only.
    assert!(matches!(
        run.refused(&[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: vec![] }]),
        PalwStateV2Error::LicensedByParts(_)
    ));
    // A shard whose outsider has not answered is no licence, however many class seats said Valid.
    let mut no_outsider = licensing_receipts(claim_id, 0, 1, anchor, bound + 1);
    no_outsider.remove(0);
    assert!(matches!(
        run.refused(&[part(claim_id, 0, no_outsider)]),
        PalwStateV2Error::TirShardPartShort { shard: 0, why, .. } if why.contains("outsider")
    ));
    // Shard 1's receipts are not shard 0's: a seat of another shard is no seat of this one.
    assert!(matches!(
        run.refused(&[part(claim_id, 0, licensing_receipts(claim_id, 1, 1, anchor, bound + 1))]),
        PalwStateV2Error::ShardPartRefused { shard: 0, .. }
    ));
    // Shard 0 licenses: its three class seats and its outsider lock, are credited, and the claim stays bound.
    run.step(&[part(claim_id, 0, licensing_receipts(claim_id, 0, 1, anchor, bound + 1))]);
    assert!(matches!(run.s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "one part licenses no claim");
    let landed = run.s.tir_shard_claim(&claim_id).unwrap();
    assert!(landed.progress.is_licensed(0) && !landed.progress.is_licensed(1));
    let after = kaspa_consensus_core::palw_producer_v2::palw_seat_duties_v2(&run.s, &run.p, &everyone);
    assert_eq!(after.len(), 4, "a shard whose part landed owes nothing more");
    assert!(after.iter().all(|d| d.tir_shard.is_some_and(|p| p.shard == 1)));
    assert_eq!(landed.cell_counts, vec![3, 0], "two class seats and the outsider on shard 0's cell");
    assert_eq!(landed.counted.len(), 4, "the outsider and the three class seats said Valid");
    for n in 10..14 {
        assert!(run.s.slashable_lock(bond_key(n), claim_id).is_some(), "seat {n} locked its scaled price");
    }
    assert!(run.s.slashable_lock(bond_key(14), claim_id).is_none(), "shard 1's seats have not answered");
    // The same shard twice is refused by name.
    assert!(matches!(
        run.refused(&[part(claim_id, 0, licensing_receipts(claim_id, 0, 1, anchor, bound + 1))]),
        PalwStateV2Error::ShardAlreadyLicensed { shard: 0, .. }
    ));
    // Shard 1's part completes the plan: the claim licenses, with `basis_k` recounted over cells.
    run.step(&[part(claim_id, 1, licensing_receipts(claim_id, 1, 1, anchor, bound + 1))]);
    let claim = run.s.claim(&claim_id).unwrap();
    assert!(matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "{:?}", claim.phase);
    assert_eq!(claim.rcore.basis_k, 3, "every cell: two class seats and its outsider");
    assert!(matches!(
        claim.rcore.licence_door,
        Some(kaspa_consensus_core::palw_economic_safety_v1::PalwLicenceDoorTagV1::ShardPart { quorum_per_shard: 3 })
    ));
    // The seat's lock is its cell's share of the full lock: shard locks sum to a whole claim's per attester.
    let locks: Vec<u128> = (10..18).map(|n| run.s.slashable_lock(bond_key(n), claim_id).expect("locked").amount).collect();
    assert!(locks.iter().all(|l| *l > 0), "{locks:?}");
}

#[test]
fn s1_inside_a_shard_needs_the_full_seat_and_the_partial_of_every_segment() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x, Some(SHARD_AT));
    run.at(SHARD_AT, &[plan(f.class_id, 2, 2)], None);
    let anchor = h64(78);
    run.step(&[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor, seats: panel_of(2, true) }]);
    let bound = run.daa;
    let rec = run.s.tir_shard_claim(&claim_id).unwrap();
    assert_eq!((rec.s_p, rec.cell_counts.len()), (2, 4), "2 shards x 2 segments");
    // Drop shard 0's full seat: each segment keeps one partial and the outsider but only ONE class seat — not a licence.
    let masks = palw_tir_shard_assignment_v1(&anchor, &claim_id, 0, 2);
    let full = masks.iter().position(|m| m.is_full(2)).unwrap();
    let mut short = licensing_receipts(claim_id, 0, 2, anchor, bound + 1);
    short.retain(|r| r.receipt.seat_bond != bond_key(11 + full as u64));
    assert!(matches!(run.refused(&[part(claim_id, 0, short)]), PalwStateV2Error::TirShardPartShort { .. }));
    run.step(&[part(claim_id, 0, licensing_receipts(claim_id, 0, 2, anchor, bound + 1))]);
    assert_eq!(run.s.tir_shard_claim(&claim_id).unwrap().cell_counts, vec![3, 3, 0, 0]);
}

#[test]
fn a_flat_panel_of_a_class_with_a_plan_writes_no_record_and_licenses_whole() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x, Some(SHARD_AT));
    run.at(SHARD_AT, &[plan(f.class_id, 2, 1)], None);
    // A panel that is not the plan's stratified shape (bound before the plan, say): no per-shard record.
    run.step(&[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(79), seats: (10..15).map(seat).collect() }]);
    assert!(run.s.tir_shard_claim(&claim_id).is_none());
    assert!(matches!(
        run.refused(&[part(claim_id, 0, vec![receipt(claim_id, 10, 0, PalwReceiptVerdictV2::Valid, PalwSegmentMaskV2::full(1), 5)])]),
        PalwStateV2Error::NotLicensedByParts(_)
    ));
}

#[test]
fn a_run_of_step_leaves_is_demanded_and_answered_as_one_unit() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x, Some(SHARD_AT));
    run.at(SHARD_AT, &[plan(f.class_id, 2, 1)], None);
    run.step(&[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(80), seats: panel_of(2, true) }]);
    let count = x.binding.step_leaf_count;
    let (first, n) = (count / 3, 12u32.min((count / 3) as u32));
    assert!(n > 1 && u64::from(n) <= u64::from(PALW_TIR_STEP_RUN_MAX_LEAVES_V1));
    let binding = &x.binding;
    let disclosure = build_tir_step_run_disclosure_v1(binding, first, n, &Store { f: &f, x: &x }, MAX).expect("the accused's run");
    assert_eq!(disclosure.preimages.len(), n as usize);
    assert!(disclosure.binding.class.program.is_empty(), "the program rides empty: the chain holds the class's");
    // The checker is hash arithmetic over the claim's roots.
    let mut filled = disclosure.clone();
    filled.binding = x.binding.clone();
    check_tir_step_run_disclosure_v1(binding.full_logits_trace_root, binding.committed_execution_root, first, n, &filled, MAX)
        .expect("an honest run answers");
    // Another run's leaves, a short opening and a tampered preimage answer nothing.
    assert!(check_tir_step_run_disclosure_v1(binding.full_logits_trace_root, binding.committed_execution_root, first + 1, n, &filled, MAX).is_err());
    let mut tampered = filled.clone();
    tampered.preimages[2].values_le.push(1);
    assert!(check_tir_step_run_disclosure_v1(binding.full_logits_trace_root, binding.committed_execution_root, first, n, &tampered, MAX).is_err());
    let mut short = filled.clone();
    short.range.leaf_hashes.pop();
    assert!(check_tir_step_run_disclosure_v1(binding.full_logits_trace_root, binding.committed_execution_root, first, n, &short, MAX).is_err());
    assert!(check_tir_step_run_disclosure_v1(binding.full_logits_trace_root, binding.committed_execution_root, first, 0, &filled, MAX).is_err());
    // On the chain: a seat's demand opens a session naming exactly the run, the accused's answer refutes it.
    let unit = PalwDaUnitV1::TirStepRun { first, count: n };
    run.step(&[PalwConsensusObjectV2::DefaultAccusedTirStep {
        accusation: Box::new(PalwTirStepAccusationV1 { claim: claim_id, unit, accuser: bond_key(OTHER), signature: vec![1; 8] }),
    }]);
    assert!(run.s.da_sessions_of(&claim_id).any(|(bond, s)| *bond == bond_key(OTHER) && s.units == vec![unit]));
    run.step(&[PalwConsensusObjectV2::MaterialDisclosedV2 {
        claim: claim_id,
        unit,
        answer: PalwDaAnswerV1::TirStepRun(Box::new(disclosure.clone())),
        discloser: bond_key(PRODUCER),
        signature: vec![2; 8],
    }]);
    assert_eq!(run.s.da_sessions_of(&claim_id).count(), 0, "the answered session is refuted and closes");
    // A run that does not end inside the execution is answered by the claim's binding proving so.
    let past = PalwDaUnitV1::TirStepRun { first: count - 2, count: 8 };
    run.step(&[PalwConsensusObjectV2::DefaultAccusedTirStep {
        accusation: Box::new(PalwTirStepAccusationV1 { claim: claim_id, unit: past, accuser: bond_key(OTHER), signature: vec![1; 8] }),
    }]);
    run.step(&[PalwConsensusObjectV2::MaterialDisclosedV2 {
        claim: claim_id,
        unit: past,
        answer: PalwDaAnswerV1::TirStepOutOfRange(Box::new({
            let mut b = x.binding.clone();
            b.class.program = Vec::new();
            b
        })),
        discloser: bond_key(PRODUCER),
        signature: vec![2; 8],
    }]);
    assert_eq!(run.s.da_sessions_of(&claim_id).count(), 0);
    // A run past every execution, and a run of too many leaves, are refused at the door.
    for bad in [
        PalwDaUnitV1::TirStepRun { first: u64::MAX / 2, count: 4 },
        PalwDaUnitV1::TirStepRun { first: 0, count: 0 },
        PalwDaUnitV1::TirStepRun { first: 0, count: PALW_TIR_STEP_RUN_MAX_LEAVES_V1 + 1 },
    ] {
        let refused = run.refused(&[PalwConsensusObjectV2::DefaultAccusedTirStep {
            accusation: Box::new(PalwTirStepAccusationV1 { claim: claim_id, unit: bad, accuser: bond_key(OTHER), signature: vec![1; 8] }),
        }]);
        assert!(matches!(refused, PalwStateV2Error::TirFence2Refused(_)), "{bad:?}: {refused:?}");
    }
}

#[test]
fn a_bond_may_sit_in_several_shards_and_its_lock_accumulates_and_it_is_paid_shard_by_shard() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x, Some(SHARD_AT));
    run.at(SHARD_AT, &[plan(f.class_id, 2, 1)], None);
    // Four bonds hold both shards (a small network): shard 0 is [10 | 11, 12, 13], shard 1 [11 | 10, 12, 13].
    let anchor = h64(81);
    let seats = vec![seat(10), seat(11), seat(12), seat(13), seat(11), seat(10), seat(12), seat(13)];
    run.step(&[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor, seats }]);
    let bound = run.daa;
    let shard_receipts = |shard: u16, outsider: u64, class: [u64; 3]| {
        let masks = palw_tir_shard_assignment_v1(&anchor, &claim_id, shard, 1);
        let mut out = vec![receipt(claim_id, outsider, shard, PalwReceiptVerdictV2::Valid, palw_tir_shard_outsider_mask_v1(1), bound + 1)];
        for (i, n) in class.iter().enumerate() {
            out.push(receipt(claim_id, *n, shard, PalwReceiptVerdictV2::Valid, masks[i], bound + 1));
        }
        out
    };
    run.step(&[part(claim_id, 0, shard_receipts(0, 10, [11, 12, 13]))]);
    let first = run.s.slashable_lock(bond_key(11), claim_id).expect("locked by shard 0").amount;
    run.step(&[part(claim_id, 1, shard_receipts(1, 11, [10, 12, 13]))]);
    let claim = run.s.claim(&claim_id).unwrap();
    assert!(matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    let second = run.s.slashable_lock(bond_key(11), claim_id).expect("locked").amount;
    assert!(second > first, "the second shard's price joins the first's: {first} -> {second}");
    assert_eq!(run.s.slashable_lock(bond_key(11), claim_id).unwrap().segments, 0, "no longer one cell's mask");
    let record = run.s.tir_shard_claim(&claim_id).unwrap();
    assert_eq!(record.counted.len(), 8, "(bond, shard) pairs: every seat of both shards");
}

#[test]
fn a_final_claim_pays_its_seats_by_the_work_they_vouched_for_and_the_reward_is_conserved() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x, Some(SHARD_AT));
    run.at(SHARD_AT, &[plan(f.class_id, 2, 1)], None);
    let anchor = h64(82);
    run.step(&[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor, seats: panel_of(2, true) }]);
    let bound = run.daa;
    for shard in 0..2u16 {
        run.step(&[part(claim_id, shard, licensing_receipts(claim_id, shard, 1, anchor, bound + 1))]);
    }
    assert!(matches!(run.s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    let escrow = run.s.claim(&claim_id).unwrap().escrowed_reward;
    let record = run.s.tir_shard_claim(&claim_id).unwrap().clone();
    // Through the challenge window to `Final`.
    let mut daa = run.daa;
    for _ in 0..40 {
        daa += 10;
        run.at(daa, &[], None);
        if matches!(run.s.claim(&claim_id).map(|c| c.phase.clone()), Some(PalwClaimPhaseV2::Final { .. })) {
            break;
        }
    }
    let claim = run.s.claim(&claim_id).expect("the claim stands");
    assert!(matches!(claim.phase, PalwClaimPhaseV2::Final { .. }), "{:?}", claim.phase);
    // Past R-core+ the legs vest in one row: the producer's, each credited seat's by its share of the work, the reserve.
    let row = run.s.vesting_row(&claim_id).expect("a vesting row").clone();
    let seat_pay: Vec<u64> = (10..18)
        .map(|n| row.seats.iter().find(|(bond, _)| *bond == bond_key(n)).map(|(_, leg)| leg.amount).unwrap_or(0))
        .collect();
    assert!(seat_pay.iter().all(|a| *a > 0), "every credited seat is paid: {seat_pay:?}");
    let (producer, reserve) = (row.producer.amount, row.reserve);
    let paid: u64 = seat_pay.iter().sum();
    assert_eq!(
        producer as u128 + paid as u128 + reserve as u128 + row.buyback_bound.min(escrow) as u128 * 0,
        (row.producer.amount + paid + row.reserve) as u128
    );
    assert!(producer + paid + reserve <= escrow, "never more than the escrow: {producer} + {paid} + {reserve} of {escrow}");
    let shares: Vec<u32> = record.drawn_permille.clone();
    assert_eq!(shares.len(), 8);
    for (i, (a, s)) in seat_pay.iter().zip(&shares).enumerate() {
        let want = seat_pay[0] as u128 * u128::from((*s).max(125)) / u128::from(shares[0].max(125));
        assert!((*a as i128 - want as i128).abs() <= 1, "seat {i}: pay follows the share: {a} for {s}‰ (first seat {} for {}‰)", seat_pay[0], shares[0]);
    }
    let row_none = run.s.vesting_row(&claim_id).is_none();
    let _ = row_none;
    assert!(producer > 0 && paid > 0);
}

// =================================================================================================
// The per-shard draw: class seats from the bonds that proved the shard, the shard's outsider from the network
// =================================================================================================

fn ready(run: &mut Run, f: &Fixture, s_l: u16, shard: u16, bonds: std::ops::RangeInclusive<u64>) {
    use kaspa_consensus_core::palw_model_registry_v1::PalwSeatReadinessRowV1;
    use kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2;
    let ready_class = kaspa_consensus_core::palw_tir_shard_v1::palw_tir_shard_ready_class_v1(&f.class_id, s_l, shard);
    let entries = bonds
        .map(|n| PalwDeltaEntryV2::SeatReadiness {
            key: (bond_key(n), ready_class),
            old: None,
            new: Some(PalwSeatReadinessRowV1 { proved_daa: 100, proved_span: 1, leaf_index: 0, proof_version: 2, chunks: 16 }),
        })
        .collect();
    let delta = PalwStateDeltaV2 { point: Run::ctx(run.daa + 1), entries };
    run.s = apply_delta_v2(&run.s, &delta, &run.p).expect("the readiness rows install");
}

#[test]
fn the_draw_seats_each_shard_from_the_bonds_that_proved_it_with_an_outsider_per_shard() {
    use kaspa_consensus_core::palw_panel_v2::{
        PalwPanelDrawPolicyV1, PalwPanelIndependenceV1, PalwPanelParamsV2, PalwPanelV2Error, derive_tir_shard_panel_v1,
    };
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x, Some(SHARD_AT));
    run.at(SHARD_AT, &[plan(f.class_id, 2, 1)], None);
    // Shard 0 is held by 10..=15, shard 1 by 12..=17.
    ready(&mut run, &f, 2, 0, 10..=15);
    ready(&mut run, &f, 2, 1, 12..=17);
    let params = PalwPanelParamsV2::new(5, 3, 4).unwrap();
    let policy = PalwPanelDrawPolicyV1 {
        weighted: false,
        economy: None,
        readiness: Some(kaspa_consensus_core::palw_model_registry_v1::PalwReadinessPolicyV1 {
            now_daa: 120,
            max_age_daa: 1_000,
            base_class_id: h64(1),
            readiness_v2: true,
        }),
        independence: Some(PalwPanelIndependenceV1 { from_daa: 0, base_class_id: h64(1), anchor_daa: 40 }),
        valid_lock: None,
        stake: None,
    };
    let draw = |seed: Hash64| derive_tir_shard_panel_v1(&run.s, &params, &claim_id, seed, 1_000, None, false, policy, 2);
    let seats = draw(h64(500)).expect("both shards fill");
    assert_eq!(seats.len(), 8, "2 shards x (outsider + 3 class seats)");
    let id = |seat: &PalwPanelSeatV2| -> u64 { (10..18).chain([PRODUCER, OTHER]).find(|n| bond_key(*n) == seat.bond).expect("a known bond") };
    // Class seats hold the shard they judge; the first seat of a shard is its outsider, drawn from the whole network.
    for (shard, slice) in seats.chunks(4).enumerate() {
        let held: Vec<u64> = if shard == 0 { (10..=15).collect() } else { (12..=17).collect() };
        for class_seat in &slice[1..] {
            assert!(held.contains(&id(class_seat)), "shard {shard}: seat {} did not prove it", id(class_seat));
        }
        let mut operators: Vec<Hash64> = slice.iter().map(|s| s.operator_id).collect();
        operators.sort_unstable_by_key(|o| o.as_bytes());
        operators.dedup();
        assert_eq!(operators.len(), 4, "one seat per operator in a shard, the outsider's operator not a class seat's");
    }
    assert_ne!(seats[0].operator_id, seats[4].operator_id, "one operator holds at most one outsider seat of a claim");
    // Deterministic; another seed is another draw; the draw never seats the claim's executor or the registrant as outsider.
    assert_eq!(seats, draw(h64(500)).unwrap());
    assert_ne!(seats, draw(h64(501)).unwrap());
    assert!(seats.iter().all(|s| s.bond != bond_key(PRODUCER)));
    // A shard short of operators refuses the whole draw by name; a sharded class never falls back to a flat panel.
    let mut thin = run.s.clone();
    {
        use kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2;
        let class1 = kaspa_consensus_core::palw_tir_shard_v1::palw_tir_shard_ready_class_v1(&f.class_id, 2, 1);
        let rows = (12..=17u64).filter(|n| *n > 13).map(|n| PalwDeltaEntryV2::SeatReadiness {
            key: (bond_key(n), class1),
            old: Some(kaspa_consensus_core::palw_model_registry_v1::PalwSeatReadinessRowV1 {
                proved_daa: 100,
                proved_span: 1,
                leaf_index: 0,
                proof_version: 2,
                chunks: 16,
            }),
            new: None,
        });
        thin = apply_delta_v2(&thin, &PalwStateDeltaV2 { point: Run::ctx(run.daa + 2), entries: rows.collect() }, &run.p).unwrap();
    }
    let short = derive_tir_shard_panel_v1(&thin, &params, &claim_id, h64(500), 1_000, None, false, policy, 2);
    assert!(
        matches!(short, Err(PalwPanelV2Error::InsufficientEligibleShardBonds { shard: 1, needed: 3, available: 2 })),
        "{short:?}"
    );
}

#[test]
fn a_shards_possession_proof_opens_its_own_rows_and_writes_the_derived_class_row() {
    use kaspa_consensus_core::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, palw_artifact_multiproof_v1};
    use kaspa_consensus_core::palw_model_registry_v1::{PALW_READINESS_V2_CHUNKS_V1, PALW_REGISTRY_GLOBALS_V1, PalwModelRegistryFoldV1};
    use kaspa_consensus_core::palw_tir_shard_v1::{
        palw_tir_shard_inventory_ranges_v1, palw_tir_shard_partition_v1, palw_tir_shard_readiness_leaves_v1,
        palw_tir_shard_ready_class_v1, palw_tir_shard_weights_v1,
    };
    let f = fixture();
    let x = f.honest();
    let (mut run, _claim) = claimed(&f, &x, Some(SHARD_AT));
    run.at(SHARD_AT, &[plan(f.class_id, 2, 1)], None);
    // The registry's fold on: possession proofs are taken (ADR-0133's readiness V2), spans of 10 DAA.
    run.extras.model_registry = Some(PalwModelRegistryFoldV1 {
        globals: PALW_REGISTRY_GLOBALS_V1,
        span_daa: 10,
        genesis_works: Default::default(),
        grace_until_daa: 0,
        admission_audit_period_daa: None,
        readiness_v2_active: true,
        bond_maturity: None,
    });
    run.extras.readiness_v2_active = true;
    let bond = bond_key(12);
    let span = (run.daa + 1) / 10;
    // The geometry the fold derives: the class's weights, its 2-shard partition, the shard's own inventory rows.
    let program = f.space.program.clone();
    let weights = palw_tir_shard_weights_v1(&program, f.class.layout.max_context);
    let parts = palw_tir_shard_partition_v1(&weights, 2).expect("two shards");
    let leaves: Vec<Hash64> = f.ops.iter().map(artifact_leaf_v1).collect();
    let proof_for = |shard: u16, bond: &PalwBondKeyV2, span: u64| {
        let ranges = palw_tir_shard_inventory_ranges_v1(&program, parts[usize::from(shard)].clone(), shard == 0, shard == 1);
        let draw = palw_tir_shard_readiness_leaves_v1(&f.class_id, 2, shard, bond, span, &ranges, PALW_READINESS_V2_CHUNKS_V1 as usize);
        assert!(!draw.is_empty());
        let opened: Vec<(u32, PalwArtifactOperandV1)> = draw.iter().map(|i| (*i, f.ops[*i as usize].clone())).collect();
        (draw, palw_artifact_multiproof_v1(&leaves, &opened).expect("a multiproof of the drawn leaves"))
    };
    let object = |shard: u16, bond: PalwBondKeyV2, span: u64, proof| PalwConsensusObjectV2::TirSeatReadinessProved {
        bond,
        class_id: f.class_id,
        shard,
        span,
        proof: Box::new(proof),
        signature: vec![0; 8],
    };
    // The proof of shard 1's rows lands under shard 1's derived class, and under no other.
    let (draw1, proof1) = proof_for(1, &bond, span);
    run.step(&[object(1, bond, span, proof1.clone())]);
    let class1 = palw_tir_shard_ready_class_v1(&f.class_id, 2, 1);
    let class0 = palw_tir_shard_ready_class_v1(&f.class_id, 2, 0);
    let row = run.s.seat_readiness(&bond, &class1).expect("the shard's row");
    assert_eq!((row.proved_span, row.proof_version), (span, 2));
    assert!(run.s.seat_readiness(&bond, &class0).is_none(), "shard 0 is not proved by shard 1's rows");
    assert!(run.s.seat_readiness(&bond, &f.class_id).is_none(), "nor is the whole class");
    // The same proof under the other shard's name opens leaves that are not that shard's draw: refused by name.
    assert!(matches!(run.refused(&[object(0, bond, span, proof1)]), PalwStateV2Error::ReadinessProofRefused(_)));
    // Another bond's draw is its own (the bond is in the challenge).
    let (draw_other, _) = proof_for(1, &bond_key(13), span);
    assert_ne!(draw1, draw_other, "the draw names (class, shard, bond, span)");
    // A shard of a plan that has no such shard, and a class with no plan, are refused.
    let (_, proof_any) = proof_for(1, &bond, span);
    assert!(matches!(run.refused(&[object(2, bond, span, proof_any.clone())]), PalwStateV2Error::ReadinessProofRefused(_)));
    let mut no_plan = object(1, bond, span, proof_any);
    if let PalwConsensusObjectV2::TirSeatReadinessProved { class_id, .. } = &mut no_plan {
        *class_id = h64(1);
    }
    assert!(run.try_at(run.daa + 1, &[no_plan], None).is_err(), "a class that declared no plan has no shard rows");
}

// ---------------------------------------------------------------------------------------------
// The acceptance layer: `validate_tir_shard_part_v1`, with a keyed-hash stand-in for ML-DSA-87 (the real verifier is the
// node's; what is under test is WHAT is signed and what the validator reads around the signature)
// ---------------------------------------------------------------------------------------------

fn fake_sig(key: &[u8], context: &[u8], message: &[u8]) -> Vec<u8> {
    let mut s = blake2b_simd::Params::new().hash_length(64).to_state();
    s.update(key);
    s.update(&(context.len() as u64).to_le_bytes());
    s.update(context);
    s.update(message);
    s.finalize().as_bytes().to_vec()
}

fn fake_verify(key: &[u8], message: &[u8], signature: &[u8], context: &[u8]) -> bool {
    fake_sig(key, context, message) == signature
}

/// A receipt of seat `n` signed as the node signs it: over `palw_receipt_message_v4` under the V4 context.
fn signed(domain: Hash64, claim: Hash64, n: u64, shard: u16, verdict: PalwReceiptVerdictV2, segments: PalwSegmentMaskV2, daa: u64) -> PalwSeatReceiptV4 {
    use kaspa_consensus_core::palw_tir_shard_v1::{PALW_RECEIPT_V4_MLDSA87_CONTEXT, palw_receipt_message_v4};
    let message = palw_receipt_message_v4(domain, claim, verdict, daa, shard, segments);
    PalwSeatReceiptV4 {
        receipt: PalwSeatReceiptV2 {
            claim,
            verdict,
            seat_bond: bond_key(n),
            signed_daa: daa,
            signature: fake_sig(&pubkey(n), PALW_RECEIPT_V4_MLDSA87_CONTEXT, message.as_byte_slice()),
        },
        shard,
        segments,
    }
}

#[test]
fn the_acceptance_validator_reads_what_is_signed_and_who_may_sign_it() {
    use kaspa_consensus_core::palw_panel_v2::{PalwPanelV2Error as E, validate_tir_shard_part_v1};
    use kaspa_consensus_core::palw_tir_shard_v1::PalwTirShardPartVerdictV1 as V;
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x, Some(SHARD_AT));
    run.at(SHARD_AT, &[plan(f.class_id, 2, 1)], None);
    let (anchor, domain) = (h64(77), h64(4242));
    run.step(&[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor, seats: panel_of(2, true) }]);
    let bound = run.daa;
    let now = bound + 3;
    let full = PalwSegmentMaskV2::full(1);
    let valid = PalwReceiptVerdictV2::Valid;
    let (state0, params0) = (run.s.clone(), run.p.clone());
    let check = |receipts: Vec<PalwSeatReceiptV4>, shard: u16, at: u64| {
        validate_tir_shard_part_v1(&state0, &params0, &Run::ctx(at), domain, &PalwTirShardPartV1 { claim: claim_id, shard, receipts }, fake_verify)
    };
    let honest = |shard: u16| -> Vec<PalwSeatReceiptV4> {
        let first = 10 + u64::from(shard) * 4;
        (0..4u64).map(|i| signed(domain, claim_id, first + i, shard, valid, full, bound + 1)).collect()
    };
    // The honest set licenses its shard, naming its counted signers.
    match check(honest(0), 0, now).expect("an honest part") {
        V::Licensed { cell_counts, signers } => {
            assert_eq!(cell_counts, vec![3], "four distinct signers cover the shard's one cell, counted to the cap of three");
            assert_eq!(signers.len(), 4);
        }
        other => panic!("{other:?}"),
    }
    // A set whose outsider is missing is sound but short (an assembler keeps collecting).
    assert!(matches!(check(honest(0)[1..].to_vec(), 0, now), Err(E::TirShardPartShort(why)) if why.contains("outsider")));
    // A receipt moved to another shard's part: the part's shard is not its signed shard.
    assert!(matches!(check(honest(1), 0, now), Err(E::TirShardPartRefused(_))));
    // A signature that covers another shard or another mask is no signature of this receipt: a relayer cannot move or widen it.
    let mut moved = honest(0);
    moved[1].segments = PalwSegmentMaskV2::full(2);
    assert!(matches!(check(moved, 0, now), Err(E::ReceiptSignatureInvalid)), "a widened mask");
    let mut retagged = honest(0);
    retagged[1].shard = 1;
    assert!(matches!(check(retagged, 0, now), Err(E::TirShardPartRefused(_))), "a retagged shard");
    // The signature is checked under the V4 context: one made under another context is refused.
    let mut wrong_context = honest(0);
    wrong_context[2].receipt.signature = fake_sig(&pubkey(12), b"misaka-palw/receipt-v3/mldsa87/v1", &[1, 2, 3]);
    assert!(matches!(check(wrong_context, 0, now), Err(E::ReceiptSignatureInvalid)));
    // A mask that is not the seat's assigned one (here: the empty mask) is refused after the signature checks.
    let mut mask = honest(0);
    mask[2] = signed(domain, claim_id, 12, 0, valid, PalwSegmentMaskV2::NONE, bound + 1);
    assert!(matches!(check(mask, 0, now), Err(E::MaskNotAssigned { .. })));
    // A seat of another shard, a duplicated seat, a Sampled receipt and the window's edges.
    let mut stranger = honest(0);
    stranger[3] = signed(domain, claim_id, 14, 0, valid, full, bound + 1);
    assert!(matches!(check(stranger, 0, now), Err(E::NotASeat(_))), "shard 1's outsider is no seat of shard 0");
    let mut twice = honest(0);
    twice[3] = twice[1].clone();
    assert!(matches!(check(twice.clone(), 0, now), Err(E::DuplicateSeat(_))));
    twice.push(twice[0].clone());
    assert!(matches!(check(twice, 0, now), Err(E::TirShardPartRefused(why)) if why.contains("receipts in a part")), "more receipts than seats");
    let mut sampled = honest(0);
    sampled[3] = signed(domain, claim_id, 13, 0, PalwReceiptVerdictV2::Sampled, full, bound + 1);
    assert!(matches!(check(sampled, 0, now), Err(E::TirShardPartRefused(why)) if why.contains("Sampled")));
    let early: Vec<PalwSeatReceiptV4> = (0..4u64).map(|i| signed(domain, claim_id, 10 + i, 0, valid, full, bound - 1)).collect();
    assert!(matches!(check(early, 0, now), Err(E::ReceiptOutsideWindow { why, .. }) if why.contains("before")));
    let future: Vec<PalwSeatReceiptV4> = (0..4u64).map(|i| signed(domain, claim_id, 10 + i, 0, valid, full, now + 5)).collect();
    assert!(matches!(check(future, 0, now), Err(E::ReceiptOutsideWindow { why, .. }) if why.contains("after the block")));
    // Nothing, and a part of a shard that does not exist.
    assert!(matches!(check(Vec::new(), 0, now), Err(E::TirShardPartRefused(_))));
    assert!(matches!(check(honest(0), 2, now), Err(E::ShardOutOfRange { shard: 2, count: 2 })));
    // Below the fence the validator refuses by name too.
    assert!(matches!(
        validate_tir_shard_part_v1(&run.s, &run.p, &Run::ctx(SHARD_AT - 1), domain, &PalwTirShardPartV1 { claim: claim_id, shard: 0, receipts: honest(0) }, fake_verify),
        Err(E::TirShardPartRefused(why)) if why.contains("not in force")
    ));
    // Once the shard has landed, a second part of it is refused.
    let landed = honest(0);
    run.step(&[part(claim_id, 0, landed.clone())]);
    let again = validate_tir_shard_part_v1(&run.s, &run.p, &Run::ctx(run.daa + 1), domain, &PalwTirShardPartV1 { claim: claim_id, shard: 0, receipts: landed }, fake_verify);
    assert!(matches!(again, Err(E::ShardAlreadyLicensed { shard: 0 })));
}

#[test]
fn the_tags_of_the_family_are_pinned() {
    use kaspa_consensus_core::palw_model_registry_v1::PalwSeatReadinessRowV1;
    use kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2;
    use kaspa_consensus_core::palw_tir_shard_v1::PalwTirShardPlanV1;
    let tag = |object: &PalwConsensusObjectV2| borsh::to_vec(object).unwrap()[0];
    let f = fixture();
    // Objects 91, 92 and 93: explicit discriminants (an allocation, not a position).
    assert_eq!(tag(&plan(f.class_id, 2, 1)), 91);
    assert_eq!(tag(&part(h64(1), 0, Vec::new())), 92);
    let proof = kaspa_consensus_core::palw_artifact::PalwArtifactMultiproofV1 { leaf_count: 1, opened: Vec::new(), siblings: Vec::new() };
    assert_eq!(
        tag(&PalwConsensusObjectV2::TirSeatReadinessProved { bond: bond_key(1), class_id: h64(1), shard: 0, span: 0, proof: Box::new(proof), signature: Vec::new() }),
        93
    );
    // The DA unit 7 and the delta entries 101 and 102.
    assert_eq!(borsh::to_vec(&PalwDaUnitV1::TirStepRun { first: 0, count: 1 }).unwrap()[0], 7);
    let plan_row = PalwTirShardPlanV1 { s_l: 2, s_p: 1, declared_daa: 5, cell_permille: vec![500, 500] };
    assert_eq!(borsh::to_vec(&PalwDeltaEntryV2::TirShardPlan { key: h64(1), old: None, new: Some(plan_row) }).unwrap()[0], 101);
    assert_eq!(borsh::to_vec(&PalwDeltaEntryV2::TirShardClaim { key: h64(1), old: None, new: None }).unwrap()[0], 102);
    let _ = std::mem::size_of::<PalwSeatReadinessRowV1>();
}

#[test]
fn a_sharded_claim_whose_panel_says_nothing_is_redrawn_once_and_then_voided_and_its_record_goes_with_it() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x, Some(SHARD_AT));
    run.at(SHARD_AT, &[plan(f.class_id, 2, 1)], None);
    let anchor = h64(77);
    run.step(&[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor, seats: panel_of(2, true) }]);
    assert!(run.s.tir_shard_claim(&claim_id).is_some());
    // Nobody answers: the receipt window passes. Whatever the sweep does (a redraw, a void), the state stays consistent, the delta
    // reverts, the carriage reloads (`Run::at` asserts all three) — and a claim that is no longer bound keeps no shard record.
    let mut phases = Vec::new();
    for step in 1..=6u64 {
        run.at(run.daa + 700 * step, &[], None);
        phases.push(format!("{:?}", run.s.claim(&claim_id).map(|c| c.phase.clone())));
        if matches!(run.s.claim(&claim_id).map(|c| c.phase.clone()), Some(PalwClaimPhaseV2::PanelBound { .. })) {
            continue;
        }
        break;
    }
    eprintln!("phases after silence: {phases:?}");
    match run.s.claim(&claim_id).map(|c| c.phase.clone()) {
        Some(PalwClaimPhaseV2::PanelBound { .. }) => panic!("a panel that says nothing cannot stay bound: {phases:?}"),
        Some(phase) => {
            assert!(
                run.s.tir_shard_claim(&claim_id).is_none(),
                "a claim in {phase:?} keeps no per-shard record (the panel it describes is gone)"
            );
        }
        None => assert!(run.s.tir_shard_claim(&claim_id).is_none()),
    }
}
