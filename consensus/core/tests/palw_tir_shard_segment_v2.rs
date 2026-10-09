//! **RFC-0006 per-segment pricing — the dormant fence `palw_tir_shard_segment_v2`** (agent SHARD,
//! `docs/design/palw/shard-rfc6-10.md` §4).
//!
//! * **The fence**: `None` on every preset, invisible to the handshake identity while `None` or `Some(never())`, committed to both
//!   fingerprints once it carries a height, and refused when armed — by name for a missing mirror, a missing `palw_tir_shard_v1`
//!   at or below it, and (until the full-activation release names its height) for any arming at all.
//! * **The resident table**: what each cell of a plan must hold (its shard's weights and `Fixed` state, its layers' history up to
//!   the segment's end) sums to exactly 1,000 permille, a later segment of a shard holds more than an earlier one, and with one
//!   segment a cell holds exactly its shard's weight.
//! * **The rules past the fence** (a lane-A claim of the dense GQA corpus model, two shards, two segments): the pay weights the
//!   bind fixes are `max(work, resident)` over each seat's cells and every counted signer locks at least what it locks below the
//!   fence — more where its cells hold more than they compute; below the fence nothing moves (the armed rules, byte for byte).
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_tir_shard_segment_v2`

#[path = "palw_tir_fixture_common.rs"]
#[allow(unused)]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{
    ForkActivation, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
};
use kaspa_consensus_core::palw_mode_v2::PalwModeV2Error;
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwPwuRuleV2,
    PalwStateCarriageV2, PalwStateParamsV2, PalwTransitionExtrasV1, apply_delta_v2, apply_palw_transition_v2_with_extras,
    palw_operator_id_v2, revert_delta_v2,
};
use kaspa_consensus_core::palw_tir_class_v1::PalwTirAdmissionCarriageV1;
use kaspa_consensus_core::palw_tir_shard_segment_v2::{
    palw_tir_shard_cell_resident_bytes_v2, palw_tir_shard_cell_resident_permille_v2, palw_tir_shard_drawn_price_permille_v2,
    palw_tir_shard_price_share_v2, palw_tir_shard_seat_need_bytes_v2,
};
use kaspa_consensus_core::palw_tir_shard_v1::{
    PalwSeatReceiptV4, PalwTirShardPartV1, palw_tir_cells_share_permille_v1, palw_tir_shard_assignment_v1,
    palw_tir_shard_drawn_permille_v1, palw_tir_shard_outsider_mask_v1, palw_tir_shard_partition_v1, palw_tir_shard_weights_v1,
};
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use misaka_palw_tir::TirProgramV1;

// ---------------------------------------------------------------------------------------------------------------------------
// The fence
// ---------------------------------------------------------------------------------------------------------------------------

fn presets() -> Vec<Params> {
    vec![
        mainnet_shipped_params(),
        devnet_shipped_params(),
        palw_rc_shipped_params(),
        palw_t12_shipped_params(),
        Params::from(TESTNET_PARAMS.net),
        Params::from(SIMNET_PARAMS.net),
    ]
}

fn refusal(p: &Params) -> &'static str {
    match p.validate_palw_tir_shard_segment_v2() {
        Err(PalwModeV2Error::Invalid(why)) => why,
        other => panic!("refused by name: {other:?}"),
    }
}

#[test]
fn the_segment_fence_is_dormant_hashed_when_set_and_refused_when_armed() {
    for p in presets() {
        assert!(p.palw_tir_shard_segment_v2.is_none() && !p.palw_tir_shard_segment_active_at(u64::MAX));
        assert_eq!(p.palw_tir_shard_segment_fence(), None);
        p.validate_palw_tir_shard_segment_v2().unwrap();
        let mut armed = p.clone();
        armed.palw_tir_shard_segment_v2 = Some(ForkActivation::new(1_000));
        armed.sync_palw_tir_shard_segment_v2();
        assert!(armed.validate_palw_tir_shard_segment_v2().is_err() && armed.validate_palw_v2().is_err());
    }
    let p = palw_t12_shipped_params();
    assert!(p.palw_tir_shard_v1.is_some(), "testnet-12 arms layer-sharded panels (DAA 5,300)");
    let mut never = p.clone();
    never.palw_tir_shard_segment_v2 = Some(ForkActivation::never());
    never.sync_palw_tir_shard_segment_v2();
    never.validate_palw_tir_shard_segment_v2().unwrap();
    assert!(!never.palw_tir_shard_segment_active_at(u64::MAX), "a never() value is dormant");
    assert_eq!(p.consensus_identity_id(), never.consensus_identity_id(), "Some(never()) collapses whole in the handshake identity");
    let mut armed = p.clone();
    armed.palw_tir_shard_segment_v2 = Some(ForkActivation::new(9_000));
    let mut other = p.clone();
    other.palw_tir_shard_segment_v2 = Some(ForkActivation::new(9_001));
    assert_ne!(p.consensus_params_id(), armed.consensus_params_id());
    assert_ne!(armed.consensus_params_id(), other.consensus_params_id());
    assert_ne!(p.consensus_schedule_id(), armed.consensus_schedule_id());
    // The refusals, by name: the mirror first, then the prerequisite, then the arming itself.
    assert!(refusal(&armed).contains("mirror"), "{}", refusal(&armed));
    armed.sync_palw_tir_shard_segment_v2();
    assert!(refusal(&armed).contains("cannot be armed yet"), "{}", refusal(&armed));
    let mut early = p.clone();
    early.palw_tir_shard_segment_v2 = Some(ForkActivation::new(100));
    early.sync_palw_tir_shard_segment_v2();
    assert!(refusal(&early).contains("needs palw_tir_shard_v1"), "{}", refusal(&early));
}

// ---------------------------------------------------------------------------------------------------------------------------
// The resident table
// ---------------------------------------------------------------------------------------------------------------------------

fn gqa() -> (TirProgramV1, Fixture) {
    let (name, program, params, tokens) =
        programs().into_iter().find(|(n, ..)| n == "dense-gqa-2layer").expect("the dense GQA corpus model");
    (program.clone(), fixture_with(name, program, params, tokens, 36, 12))
}

#[test]
fn the_resident_table_sums_to_a_thousand_and_a_later_segment_holds_more() {
    let (program, f) = gqa();
    let max_context = f.class.layout.max_context;
    for s_p in [1u16, 2] {
        let bytes = palw_tir_shard_cell_resident_bytes_v2(&program, max_context, 2, s_p).expect("a plan of two shards");
        let table = palw_tir_shard_cell_resident_permille_v2(&program, max_context, 2, s_p).unwrap();
        assert_eq!((bytes.len(), table.len()), (2 * usize::from(s_p), 2 * usize::from(s_p)));
        assert_eq!(table.iter().map(|x| u32::from(*x)).sum::<u32>(), 1_000, "S_P = {s_p}: {table:?}");
        let mut grows = s_p == 1;
        for shard in 0..2usize {
            let row = &bytes[shard * usize::from(s_p)..(shard + 1) * usize::from(s_p)];
            assert!(row.windows(2).all(|w| w[0] <= w[1]), "S_P = {s_p}, shard {shard}: a later segment never holds less: {row:?}");
            grows |= row.windows(2).any(|w| w[0] < w[1]);
            assert_eq!(
                palw_tir_shard_seat_need_bytes_v2(&program, max_context, 2, s_p, shard as u16).unwrap(),
                *row.last().unwrap(),
                "a ready seat must hold the shard's heaviest cell: its last segment's"
            );
        }
        assert!(grows, "S_P = {s_p}: the history held grows with the segment somewhere: {bytes:?}");
    }
    // One segment: a cell is its whole shard, so it holds exactly the shard's weight (`palw_tir_shard_weights_v1`).
    let weights = palw_tir_shard_weights_v1(&program, max_context);
    let parts = palw_tir_shard_partition_v1(&weights, 2).unwrap();
    let whole: Vec<u128> = parts.iter().map(|r| weights.of_range(r.start, r.end)).collect();
    assert_eq!(palw_tir_shard_cell_resident_bytes_v2(&program, max_context, 2, 1).unwrap(), whole);
    // A plan the program cannot carry is refused by name, never a panic.
    assert!(palw_tir_shard_cell_resident_bytes_v2(&program, max_context, 2, 0).is_err());
    assert!(palw_tir_shard_cell_resident_bytes_v2(&program, max_context, 64, 1).is_err());
}

// ---------------------------------------------------------------------------------------------------------------------------
// The rules past the fence, on lane A's flow
// ---------------------------------------------------------------------------------------------------------------------------

const PRODUCER: u64 = 1;
const SHARD_AT: u64 = 5;

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

fn params(segment_at: Option<u64>) -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        .with_fp_exposure_ceiling(500)
        .unwrap()
        .with_tir_from_daa(Some(0))
        .with_rcore_plus_mirrors(Some(0), 0, Vec::new())
        .with_tir_fence2_from_daa(Some(2))
        .with_tir_shard_from_daa(Some(SHARD_AT))
        .with_tir_shard_segment_from_daa(segment_at)
        .with_worker_carve_permille(620)
        .unwrap()
}

fn bond(n: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: pubkey(n),
        operator_pubkey: op_key(n),
        collateral: 100_000_000_000,
        payout_payload: Hash64::from_u64_word(0x9A00 + n),
        capable_classes: std::iter::once(h64(1)).collect(),
        signature: Vec::new(),
    }
}

struct Run {
    p: PalwStateParamsV2,
    s: PalwChainStateV2,
    extras: PalwTransitionExtrasV1,
}

impl Run {
    fn at(&mut self, daa: u64, objects: &[PalwConsensusObjectV2], att: Option<&PalwAttemptEnvelopeV2>) {
        let ctx = PalwBlockContextV2 { block: h64(0xB10C_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 1_000_000_000 };
        let (child, delta) =
            apply_palw_transition_v2_with_extras(&self.s, &self.p, &ctx, objects, att, false, false, false, true, &self.extras)
                .unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
        assert_eq!(apply_delta_v2(&self.s, &delta, &self.p).expect("re-applies"), child);
        assert_eq!(revert_delta_v2(&child, &delta, &self.p).expect("reverts"), self.s);
        let reloaded = PalwStateCarriageV2::from_state(&child).into_state(&self.p, Some(child.state_root())).expect("reloads");
        assert_eq!(reloaded, child);
        self.s = child;
    }
}

fn receipt(claim: Hash64, n: u64, shard: u16, segments: PalwSegmentMaskV2, signed_daa: u64) -> PalwSeatReceiptV4 {
    PalwSeatReceiptV4 {
        receipt: PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: bond_key(n),
            signed_daa,
            signature: vec![7; 8],
        },
        shard,
        segments,
    }
}

/// The part of `shard` under `S_P = 2`: its outsider over the whole shard and each class seat over its assigned mask.
fn part(claim: Hash64, shard: u16, anchor: Hash64, daa: u64) -> PalwConsensusObjectV2 {
    let first = 10 + u64::from(shard) * 4;
    let masks = palw_tir_shard_assignment_v1(&anchor, &claim, shard, 2);
    let mut receipts = vec![receipt(claim, first, shard, palw_tir_shard_outsider_mask_v1(2), daa)];
    for i in 0..3u64 {
        receipts.push(receipt(claim, first + 1 + i, shard, masks[i as usize], daa));
    }
    PalwConsensusObjectV2::TirShardReceiptLicensed { part: PalwTirShardPartV1 { claim, shard, receipts } }
}

struct Licensed {
    claim: Hash64,
    anchor: Hash64,
    drawn: Vec<u32>,
    locks: Vec<u128>,
    s: PalwChainStateV2,
}

/// Lane A's per-shard flow under a 2 × 2 plan: the IR class and ten bonds, the claim, the plan, the per-shard panel
/// `[10 | 11 12 13] [14 | 15 16 17]`, both parts. With `segment_at` the per-segment fence is mirrored at that height.
fn licensed(segment_at: Option<u64>) -> Licensed {
    let (_, f) = gqa();
    let x = f.honest();
    let extras = PalwTransitionExtrasV1 { admission_independence_daa: Some(0), panel_economy_active: true, ..Default::default() };
    let mut run = Run { p: params(segment_at), s: PalwChainStateV2::genesis(), extras };
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
        bond(PRODUCER),
    ];
    objects.extend((10..18).map(bond));
    objects.push(PalwConsensusObjectV2::ClassRegisteredTirV1 {
        class_id: f.class_id,
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
            challenge: challenge_v2(network_domain, h64(5), 1_700, 1, f.class_id, &producer),
            class_id: f.class_id,
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
    let claim = attempt_id_v2(&env.attempt);
    run.at(3, &[], Some(&env));
    run.at(
        SHARD_AT,
        &[PalwConsensusObjectV2::TirShardPlanDeclared { class_id: f.class_id, s_l: 2, s_p: 2, signature: vec![1; 8] }],
        None,
    );
    let anchor = h64(77);
    run.at(SHARD_AT + 1, &[PalwConsensusObjectV2::PanelBound { claim, anchor, seats: (10..18).map(seat).collect() }], None);
    let drawn = run.s.tir_shard_claim(&claim).expect("drawn per shard").drawn_permille.clone();
    run.at(SHARD_AT + 2, &[part(claim, 0, anchor, SHARD_AT + 2)], None);
    run.at(SHARD_AT + 3, &[part(claim, 1, anchor, SHARD_AT + 3)], None);
    assert!(matches!(run.s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed by parts");
    let locks = (10..18).map(|n| run.s.slashable_lock(bond_key(n), claim).expect("every seat locked").amount).collect();
    Licensed { claim, anchor, drawn, locks, s: run.s }
}

#[test]
fn past_the_fence_a_seat_is_priced_by_the_larger_of_work_and_residency_and_below_it_nothing_moves() {
    let below = licensed(None);
    let past = licensed(Some(0));
    let unarmed_far = licensed(Some(1_000_000));
    assert_eq!(below.s.state_root(), unarmed_far.s.state_root(), "a fence the chain never reaches changes no byte");
    let plan = below.s.tir_shard_plan(&below.s.claim(&below.claim).unwrap().class_id).expect("the plan").clone();
    let record = below.s.tir_class_v1(&below.s.claim(&below.claim).unwrap().class_id).expect("the IR class").clone();
    let program = TirProgramV1::decode_canonical(&record.program).unwrap();
    let resident = palw_tir_shard_cell_resident_permille_v2(&program, record.facts.max_context, plan.s_l, plan.s_p).unwrap();
    // Below the fence: the armed work shares, byte for byte.
    assert_eq!(below.drawn, palw_tir_shard_drawn_permille_v1(&plan, &below.anchor, &below.claim, true));
    // Past it: `max(work, resident)` over each seat's cells.
    assert_eq!(past.drawn, palw_tir_shard_drawn_price_permille_v2(&plan, &resident, &past.anchor, &past.claim, true));
    assert_ne!(past.drawn, below.drawn, "a late segment holds more than it computes: some seat's price moves");
    // Every counted signer locks at least what it locks below the fence, and more where its cells hold more than they compute.
    let mut moved = 0;
    for (i, n) in (10..18u64).enumerate() {
        let shard = (i / 4) as u16;
        let mask = if i % 4 == 0 {
            palw_tir_shard_outsider_mask_v1(2)
        } else {
            palw_tir_shard_assignment_v1(&below.anchor, &below.claim, shard, 2)[i % 4 - 1]
        };
        let work = palw_tir_cells_share_permille_v1(&plan.cell_permille, 2, shard, mask);
        let price = palw_tir_shard_price_share_v2(&plan.cell_permille, &resident, 2, shard, mask);
        assert!(price >= work);
        assert!(past.locks[i] >= below.locks[i], "seat {n}: {} -> {}", below.locks[i], past.locks[i]);
        if price > work.max(125) {
            assert!(past.locks[i] > below.locks[i], "seat {n}: its price share {price} > work share {work} raises its lock");
            moved += 1;
        } else if price.max(125) == work.max(125) {
            assert_eq!(past.locks[i], below.locks[i], "seat {n}: the same share, the same lock");
        }
    }
    assert!(moved > 0, "the residency term priced some seat above its work");
}
