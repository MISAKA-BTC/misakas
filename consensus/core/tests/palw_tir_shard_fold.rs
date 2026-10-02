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
}

struct Run {
    p: PalwStateParamsV2,
    s: PalwChainStateV2,
    daa: u64,
    extras: PalwTransitionExtrasV1,
}

impl Run {
    fn ctx(daa: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h64(0xB10C_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 0 }
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
        capable_classes: Default::default(),
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
        bond(PRODUCER, 1_000_000),
        bond(OTHER, 1_000_000),
    ];
    for n in 10..18 {
        objects.push(bond(n, 1_000_000));
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
