//! **SHARD (RFC-0006 × RFC-0010) changes nothing on a chain that has not armed them** — the unarmed fold's roots and encodings
//! are pinned to the integration line the lane branched from (`b676927de`), byte for byte.
//!
//! Every change of the lane is behind a dormant fence: the permissionless Panel's strata and its accusation guard
//! (`palw_permissionless_panel_v1`, refused at every real height), the per-segment pricing (`palw_tir_shard_segment_v2`, `None` on every
//! preset). The pins below were computed on `b676927de` (the engine, the fold and the shard rules as they were) and must hold
//! unchanged on the lane:
//!
//! * **testnet-12's own fold** (`apply_palw_transition_v7`, the processor's extras): floor claims, lane-A bindings, a licence and its
//!   `Final`, a non-seat DA accusation to its default, a court on a bound claim — with the permissionless Panel unconfigured, and
//!   configured at a height the chain never reaches (the two must agree with each other and with the pin);
//! * **RFC-0006 on lane A** (an IR class with a 2-shard plan, `palw_tir_shard_v1` armed as testnet-12 arms it): the plan, a panel drawn
//!   per shard, both parts, the licence by parts.
//!
//! Pinned: every block's state root, every block's delta as borsh, and the tip's carriage as borsh, folded into one BLAKE2b-512.

#[path = "rcore_common.rs"]
mod common;
use common::*;

#[path = "palw_tir_fixture_common.rs"]
#[allow(unused)]
mod fixture;

use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_permissionless_panel_v1::{PalwPermissionlessPanelV1, PanelPolicyV1};
use kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2;

/// The pins (hex BLAKE2b-512 of the transcript), computed on the integration line `b676927de`.
const PIN_T12_FLOW: &str = "27f526cb29563e85e9a70a49f4f6df720f8e9f5f2486293b65b720167206df54af7d207e5762d3c26a282226bd1eb3017a55c1f149953da65e835ebfff9efe3f";
const PIN_IR_SHARD_FLOW: &str = "2a6cd2332a402c62d628925d4a597f58a2cb9126be909630b320c9dc53509aec49db6f7a3f9ee80f6e1a0a133b31fc7f72c2134f62edd1675cc8f637bae68e40";

struct Transcript(blake2b_simd::State);

impl Transcript {
    fn new() -> Self {
        Self(blake2b_simd::Params::new().hash_length(64).to_state())
    }
    fn block(&mut self, root: Hash64, delta: &PalwStateDeltaV2) {
        self.0.update(root.as_byte_slice());
        let bytes = borsh::to_vec(delta).expect("a delta encodes");
        self.0.update(&(bytes.len() as u64).to_le_bytes());
        self.0.update(&bytes);
    }
    fn tip(mut self, state: &PalwChainStateV2) -> String {
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(state)).expect("the carriage encodes");
        self.0.update(&(bytes.len() as u64).to_le_bytes());
        self.0.update(&bytes);
        self.0.finalize().to_hex().to_string()
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// testnet-12's own fold
// ---------------------------------------------------------------------------------------------------------------------------

/// One block on `c` at `daa`, checked as `Chain::step_at` checks one, its root and delta written to the transcript.
fn block(
    c: &mut Chain,
    t: &mut Transcript,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    key: Hash64,
    subsidy: u64,
) {
    let x = ctx(0xCA_0000 + daa, daa, daa, subsidy);
    let parent = c.s.clone();
    let (child, delta, skips) = c.try_fold(&parent, &x, objects, work, key).unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
    assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
    assert_eq!(apply_delta_v2(&parent, &delta, &c.sp).expect("re-applies"), child);
    assert_eq!(revert_delta_v2(&child, &delta, &c.sp).expect("reverts"), parent);
    t.block(child.state_root(), &delta);
    c.s = child;
    c.daa = daa;
}

fn floor_claim(c: &mut Chain, t: &mut Transcript, seed: u64) -> Hash64 {
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let (bond, pubkey, operator) = floor_producer(&c.p);
    let pwu = c.floor_pwu(c.daa + 1);
    let (env, key, id) = junk_attempt(floor, bond, pubkey, &operator, pwu, seed, 0x10C0 + seed);
    block(c, t, c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    assert!(c.s.claim(&id).is_some(), "the floor attempt is accepted");
    id
}

fn empty(c: &mut Chain, t: &mut Transcript, daa: u64) {
    block(c, t, daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
}

fn with_objects(c: &mut Chain, t: &mut Transcript, objects: &[PalwConsensusObjectV2]) {
    block(c, t, c.daa + 1, objects, PalwBlockWorkV3::None, Hash64::default(), 0);
}

fn t12_flow(p: Params) -> String {
    let mut c = Chain::new(p);
    let mut t = Transcript::new();
    let seats = c.floor_seats();
    let (producer, _, _) = floor_producer(&c.p);
    let outsider = genesis_bonds(&c.p).iter().map(|(k, _, _)| *k).find(|k| *k != producer && !seats.iter().any(|(s, _)| s == k)).unwrap();
    let a = floor_claim(&mut c, &mut t, 1);
    let b = floor_claim(&mut c, &mut t, 2);
    let d = floor_claim(&mut c, &mut t, 3);
    for claim in [a, b, d] {
        let anchor = h(0xAC_0000 + c.daa);
        with_objects(&mut c, &mut t, &[PalwConsensusObjectV2::PanelBound { claim, anchor, seats: seats_of(&seats) }]);
        assert!(matches!(c.claim(&claim).phase, PalwClaimPhaseV2::PanelBound { .. }), "lane A binds");
    }
    // A licence by a whole quorum, then its Final.
    let at = c.daa + 1;
    let receipts = seats.iter().map(|(seat, _)| valid(a, *seat, at)).collect();
    with_objects(&mut c, &mut t, &[PalwConsensusObjectV2::ReceiptLicensed { claim: a, receipts }]);
    assert!(matches!(c.claim(&a).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    // A non-seat public bond accuses `b` of withholding; a court is opened on `d` by the same bond.
    with_objects(&mut c, &mut t, &[da_accuse(b, outsider, 3)]);
    let court = court_opened(&c.s, d, outsider);
    with_objects(&mut c, &mut t, &[court]);
    // Walk to `a`'s Final and `b`'s default, a block at a time where something is due.
    let final_at = c.s.deadline_of(&a).expect("a's Final deadline") + 1;
    let default_at = c.s.da_session(&b, &outsider).expect("the session").deadline_daa + 1;
    let mut stops: Vec<u64> = vec![final_at, default_at, default_at + 1, final_at.max(default_at) + 25];
    stops.sort_unstable();
    stops.dedup();
    for daa in stops {
        if daa > c.daa {
            empty(&mut c, &mut t, daa);
        }
    }
    assert!(matches!(c.claim(&a).phase, PalwClaimPhaseV2::Final { .. }));
    assert!(matches!(c.s.claim(&b).unwrap().phase, PalwClaimPhaseV2::Voided { .. }), "{:?}", c.s.claim(&b).unwrap().phase);
    t.tip(&c.s)
}

#[test]
fn the_unarmed_t12_fold_is_byte_identical_to_the_integration_line() {
    let plain = t12_flow(t12());
    // The permissionless Panel configured at a height the chain never reaches (validation bypassed, as every V3 fixture does).
    let mut p = t12();
    let policy = PanelPolicyV1 {
        seal_depth_blocks: 1,
        seal_wait_daa: 50,
        bond_maturity_daa: 1,
        beacon_period_daa: 100,
        beacon_wait_daa: 8,
        assignment_delay_daa: 1,
        receipt_window_daa: 3,
        seat_count: 5,
        outsider_seats: 0,
        max_retries: 1,
        min_collateral: 1,
        max_candidates: 64,
        max_pending: 64,
        max_pending_per_bond: 16,
        max_assignments_per_block: 8,
        max_admissions_per_block: 8,
        max_tracked_claims: 256,
        max_beacons_per_block: 2,
        max_beacon_proof_bytes: 4096,
        beacon_scheme: h(0x5C4E),
    };
    p.palw_permissionless_panel_v1 = Some(PalwPermissionlessPanelV1 { activation: ForkActivation::new(10_000_000), policy });
    p.sync_palw_permissionless_panel_v1();
    let configured = t12_flow(p);
    assert_eq!(plain, configured, "a permissionless Panel that is never reached changes no byte");
    eprintln!("[shard-pin] t12 flow: {plain}");
    assert_eq!(plain, PIN_T12_FLOW, "the unarmed testnet-12 fold is the integration line's, byte for byte");
}

// ---------------------------------------------------------------------------------------------------------------------------
// RFC-0006 on lane A (`palw_tir_shard_fold.rs`'s fixture)
// ---------------------------------------------------------------------------------------------------------------------------

mod ir {
    use super::fixture::*;
    use kaspa_consensus_core::Hash64;
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
    };
    use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
    use kaspa_consensus_core::palw_state_v2::{
        PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwPwuRuleV2,
        PalwStateDeltaV2, PalwStateParamsV2, PalwTransitionExtrasV1, apply_delta_v2, apply_palw_transition_v2_with_extras,
        palw_operator_id_v2, revert_delta_v2,
    };
    use kaspa_consensus_core::palw_tir_class_v1::PalwTirAdmissionCarriageV1;
    use kaspa_consensus_core::palw_tir_shard_v1::{
        PalwSeatReceiptV4, PalwTirShardPartV1, palw_tir_shard_assignment_v1, palw_tir_shard_outsider_mask_v1,
    };
    use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    const PRODUCER: u64 = 1;
    const OTHER: u64 = 30;
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
    fn params() -> PalwStateParamsV2 {
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
            .with_worker_carve_permille(620)
            .unwrap()
    }
    fn bond(n: u64, collateral: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::BondRegistered {
            bond: bond_key(n),
            pubkey: pubkey(n),
            operator_pubkey: op_key(n),
            collateral,
            payout_payload: Hash64::from_u64_word(0x9A00 + n),
            capable_classes: std::iter::once(h64(1)).collect(),
            signature: Vec::new(),
        }
    }
    fn receipt(claim: Hash64, n: u64, shard: u16, segments: PalwSegmentMaskV2, signed_daa: u64) -> PalwSeatReceiptV4 {
        PalwSeatReceiptV4 {
            receipt: PalwSeatReceiptV2 { claim, verdict: PalwReceiptVerdictV2::Valid, seat_bond: bond_key(n), signed_daa, signature: vec![7; 8] },
            shard,
            segments,
        }
    }
    fn part(claim: Hash64, shard: u16, anchor: Hash64, daa: u64) -> PalwConsensusObjectV2 {
        let first = 10 + u64::from(shard) * 4;
        let masks = palw_tir_shard_assignment_v1(&anchor, &claim, shard, 1);
        let mut receipts = vec![receipt(claim, first, shard, palw_tir_shard_outsider_mask_v1(1), daa)];
        for i in 0..3u64 {
            receipts.push(receipt(claim, first + 1 + i, shard, masks[i as usize], daa));
        }
        PalwConsensusObjectV2::TirShardReceiptLicensed { part: PalwTirShardPartV1 { claim, shard, receipts } }
    }

    struct Run {
        p: PalwStateParamsV2,
        s: PalwChainStateV2,
        extras: PalwTransitionExtrasV1,
        t: super::Transcript,
    }

    impl Run {
        fn at(&mut self, daa: u64, objects: &[PalwConsensusObjectV2], att: Option<&PalwAttemptEnvelopeV2>) {
            let ctx = PalwBlockContextV2 { block: h64(0xB10C_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 1_000_000_000 };
            let (child, delta): (PalwChainStateV2, PalwStateDeltaV2) =
                apply_palw_transition_v2_with_extras(&self.s, &self.p, &ctx, objects, att, false, false, false, true, &self.extras)
                    .unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
            assert_eq!(apply_delta_v2(&self.s, &delta, &self.p).expect("re-applies"), child);
            assert_eq!(revert_delta_v2(&child, &delta, &self.p).expect("reverts"), self.s);
            self.t.block(child.state_root(), &delta);
            self.s = child;
        }
    }

    pub fn flow() -> String {
        let (name, program, params_map, tokens) =
            programs().into_iter().find(|(n, ..)| n == "dense-gqa-2layer").expect("the dense GQA corpus model");
        let f = fixture_with(name, program, params_map, tokens, 36, 12);
        let x = f.honest();
        let extras = PalwTransitionExtrasV1 { admission_independence_daa: Some(0), panel_economy_active: true, ..Default::default() };
        let mut run = Run { p: params(), s: PalwChainStateV2::genesis(), extras, t: super::Transcript::new() };
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
        run.at(SHARD_AT, &[PalwConsensusObjectV2::TirShardPlanDeclared { class_id: f.class_id, s_l: 2, s_p: 1, signature: vec![1; 8] }], None);
        let anchor = h64(77);
        let seats: Vec<PalwPanelSeatV2> = (0..8).map(|i| seat(10 + i)).collect();
        run.at(SHARD_AT + 1, &[PalwConsensusObjectV2::PanelBound { claim, anchor, seats }], None);
        assert!(run.s.tir_shard_claim(&claim).is_some(), "drawn per shard");
        run.at(SHARD_AT + 2, &[part(claim, 0, anchor, SHARD_AT + 2)], None);
        run.at(SHARD_AT + 3, &[part(claim, 1, anchor, SHARD_AT + 3)], None);
        assert!(matches!(run.s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed by parts");
        run.at(SHARD_AT + 4, &[], None);
        let s = run.s.clone();
        run.t.tip(&s)
    }
}

#[test]
fn the_unarmed_rfc0006_lane_a_fold_is_byte_identical_to_the_integration_line() {
    let digest = ir::flow();
    eprintln!("[shard-pin] IR shard flow: {digest}");
    assert_eq!(digest, PIN_IR_SHARD_FLOW, "lane A's per-shard flow is the integration line's, byte for byte");
}
