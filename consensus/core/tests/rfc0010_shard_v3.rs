//! **RFC-0006 × RFC-0010: a sharded IR class under the permissionless Panel** (agent SHARD, `docs/design/palw/shard-rfc6-10.md`
//! §1 and §3). Before this lane a V3-rule claim of a class with a shard plan ended at admission (`PermissionlessNoCapablePanel`):
//! the engine drew one flat Panel, and a flat Panel cannot license by parts. Now the engine draws in strata:
//!
//! * **the per-shard V3 draw** — an IR class (the corpus's dense GQA model, two layers) declares a 2-shard plan; its claim enters
//!   the engine with `PanelStrataV1 { 2, 3, outsider }`, is sealed, certified (a REFERENCE beacon history — the only way to exercise
//!   the path, no Panel-independent Final exists on a real chain) and drawn stratum by stratum: each shard's three class seats from
//!   the bonds that declared the shard's readiness class, its outsider from the base class; the binding writes the V2 panel record
//!   (anchor = the V3 seed, shard-major) and RFC-0006's per-shard record, and two parts license it by cells;
//! * **a thin stratum** ends the claim `NoCapablePanel` (non-fraud) — never a flat Panel;
//! * **the non-seat cell watcher (G14)** — a bond outside every seat lists the claim's shards as watch duties, and from the claim's
//!   public material alone convicts a lie (`TirShardCourtAccused`, its own bond the accuser); a non-seat `TirStepRun` demand holds
//!   the engine's receipt window and defaults `ProducerWithholding`.
//!
//! **These tests bypass `validate_palw_v2`, and say so**: `palw_permissionless_panel_v1` is refused at every real height. Every block
//! goes through the transition and is checked three ways (the delta re-applies and reverts, the carriage reloads under its root) and
//! against the internal and deadline consistency checks (the engine's included).
//!
//! Run: `cargo test -p kaspa-consensus-core --test rfc0010_shard_v3`

#[path = "palw_tir_fixture_common.rs"]
#[allow(unused)]
mod fixture;
use fixture::*;

use std::collections::BTreeSet;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
};
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaUnitV1, PalwTirStepAccusationV1};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_panel_beacon_v1::{panel_beacon_context_v1, panel_beacon_scheme_of_v1};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_permissionless_panel_v1::{
    BeaconProofV1, BeaconRequestV1, BondIdV1, ClaimPhaseV3, NonFraudReasonV1, PalwPanelV3ParamsV1, PanelPolicyV1, PanelStrataV1,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwCourtVerdictV2,
    PalwPanelV3BeaconSourceV1, PalwPanelV3InputsV1, PalwPwuRuleV2, PalwStateCarriageV2, PalwStateParamsV2, PalwTransitionExtrasV1,
    PalwVoidReasonV2, apply_delta_v2, apply_palw_transition_v7, palw_operator_id_v2, revert_delta_v2,
};
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::palw_tir_class_v1::PalwTirAdmissionCarriageV1;
use kaspa_consensus_core::palw_tir_court_v1::{build_tir_cone_refutation_v1, check_tir_cone_refutation_v1};
use kaspa_consensus_core::palw_tir_one_move_v1::{palw_tir_one_move_accusation_v1, palw_tir_one_move_verdict_v1};
use kaspa_consensus_core::palw_tir_shard_v1::{
    PalwSeatReceiptV4, PalwTirShardPartV1, palw_tir_shard_assignment_v1, palw_tir_shard_outsider_mask_v1,
    palw_tir_shard_ready_class_v1,
};
use kaspa_consensus_core::palw_tir_shard_watch_v1::{PALW_TIR_SHARD_WATCH_NO_SEAT_V1, palw_tir_shard_watch_duties_v1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use misaka_palw_challenge::{FinalPathV1, PostCommitChallengePolicyV1, WorkBeaconStateV1, WorkFinalEventV1, WorkSourceKindV1};
use misaka_palw_challenge::{collect_work_beacon_v1, policy::reference_policy_v1};

const PRODUCER: u64 = 1;
/// Shard 0's ready bonds, shard 1's, the base class's (the outsiders), and a bond that holds nothing a draw reads.
const SHARD0: [u64; 4] = [10, 11, 12, 13];
const SHARD1: [u64; 4] = [14, 15, 16, 17];
const OUTSIDERS: [u64; 4] = [20, 21, 22, 23];
const WATCHER: u64 = 30;
const V3_AT: u64 = 1;
const SHARD_AT: u64 = 5;
const CLAIM_AT: u64 = 6;
const LADDER: u64 = 1 << 26;

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
fn id_of(b: &PalwBondKeyV2) -> u64 {
    (1..=40).find(|n| bond_key(*n) == *b).expect("a fixture bond")
}

fn challenge_policy() -> PostCommitChallengePolicyV1 {
    reference_policy_v1(1, 1, 10, 1, 1)
}

fn engine_policy() -> PanelPolicyV1 {
    PanelPolicyV1 {
        seal_depth_blocks: 1,
        seal_wait_daa: 50,
        bond_maturity_daa: 1,
        beacon_period_daa: 100,
        beacon_wait_daa: 8,
        assignment_delay_daa: 1,
        receipt_window_daa: 3,
        seat_count: 5,
        outsider_seats: 1,
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
        beacon_scheme: panel_beacon_scheme_of_v1(&challenge_policy()),
    }
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
        .with_panel_v3(Some(PalwPanelV3ParamsV1 {
            from_daa: V3_AT,
            policy: engine_policy(),
            network: h64(0x4E37),
            ruleset: h64(0x5255),
        }))
}

fn bond(n: u64, capable: &[Hash64]) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: pubkey(n),
        operator_pubkey: op_key(n),
        collateral: 100_000_000_000,
        payout_payload: Hash64::from_u64_word(0x9A00 + n),
        capable_classes: capable.iter().copied().collect(),
        signature: Vec::new(),
    }
}

struct Run {
    p: PalwStateParamsV2,
    s: PalwChainStateV2,
    daa: u64,
    extras: PalwTransitionExtrasV1,
}

impl Run {
    fn at(&mut self, daa: u64, objects: &[PalwConsensusObjectV2], att: Option<&PalwAttemptEnvelopeV2>) {
        assert!(daa > self.daa, "DAA moves forward");
        let ctx = PalwBlockContextV2 { block: h64(0xB10C_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 1_000_000_000 };
        let (work, key) = match att {
            Some(env) => (PalwBlockWorkV3::Attempt(env), h64(0xE7E7_0000 + daa)),
            None => (PalwBlockWorkV3::None, Hash64::default()),
        };
        let (child, delta, skips) =
            apply_palw_transition_v7(&self.s, &self.p, None, &ctx, objects, work, &[], key, false, false, false, true, &self.extras)
                .unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
        assert!(skips.is_empty(), "DAA {daa}: nothing skipped: {skips:?}");
        assert_eq!(apply_delta_v2(&self.s, &delta, &self.p).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&child, &delta, &self.p).expect("reverts"), self.s, "DAA {daa}: the delta reverts");
        let reloaded = PalwStateCarriageV2::from_state(&child)
            .into_state(&self.p, Some(child.state_root()))
            .unwrap_or_else(|e| panic!("DAA {daa}: the carriage reloads: {e}"));
        assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        child.assert_internal_consistency(&self.p).unwrap_or_else(|e| panic!("DAA {daa}: internal consistency: {e}"));
        child.assert_deadline_consistency(&self.p).unwrap_or_else(|e| panic!("DAA {daa}: deadline consistency: {e}"));
        self.s = child;
        self.daa = daa;
    }

    fn step(&mut self, objects: &[PalwConsensusObjectV2]) {
        self.at(self.daa + 1, objects, None);
    }

    fn record(&self, id: &Hash64) -> kaspa_consensus_core::palw_permissionless_panel_v1::ClaimRecordV3 {
        self.s.panel_v3().expect("the engine").claim_rows().get(id).expect("tracked").clone()
    }

    fn phase(&self, id: &Hash64) -> PalwClaimPhaseV2 {
        self.s.claim(id).expect("the claim").phase.clone()
    }

    fn seats(&self, id: &Hash64) -> Vec<PalwBondKeyV2> {
        self.s.panel(id).expect("a panel").seats.iter().map(|seat| seat.bond).collect()
    }

    /// The reference history's one Panel-independent source for the epoch `id` sealed against, and the proof it locks.
    fn proof_for(&mut self, id: &Hash64) -> BeaconProofV1 {
        let seal = self.record(id).seal.expect("sealed");
        let (release, epoch) = (seal.anchor_slot, seal.beacon_epoch);
        let mirror = *self.p.panel_v3().expect("the mirror");
        let profile = [0x77u8; 64];
        let event = WorkFinalEventV1 {
            kind: WorkSourceKindV1::RealUsefulWork,
            source_profile_id: profile,
            canonical_work_id: [0x42; 64],
            execution_commitment: [0x42; 64],
            accepted_position: release + 1,
            settlement_position: release + 2,
            occurrence_index: 0,
            claim_final: true,
            da_satisfied: true,
            validity_independent: true,
            depends_on_profiles: Vec::new(),
            final_path: FinalPathV1::PanelIndependent,
        };
        if let Some(inputs) = self.extras.panel_v3.as_mut() {
            inputs.beacon_source = PalwPanelV3BeaconSourceV1::Reference {
                events: vec![event.clone()],
                eligible_profiles: BTreeSet::from([profile]),
                works: Vec::new(),
                sealed: Vec::new(),
            };
        }
        let request = BeaconRequestV1 {
            network: mirror.network,
            ruleset: mirror.ruleset,
            scheme: mirror.policy.beacon_scheme,
            epoch,
            release_daa: release,
            deadline_daa: release + mirror.policy.beacon_wait_daa,
        };
        let context = panel_beacon_context_v1(&request, &challenge_policy(), BTreeSet::from([profile]));
        let WorkBeaconStateV1::Locked(beacon) = collect_work_beacon_v1(&context, &[event], release + 5).expect("a valid policy")
        else {
            panic!("the reference source locks a beacon");
        };
        BeaconProofV1 { epoch, output: Hash64::from_bytes(beacon.output), proof: borsh::to_vec(beacon.beacon()).unwrap() }
    }
}

fn fixture_gqa() -> Fixture {
    let (name, program, params, tokens) =
        programs().into_iter().find(|(n, ..)| n == "dense-gqa-2layer").expect("the dense GQA corpus model");
    fixture_with(name, program, params, tokens, 36, 12)
}

/// The chain up to a V3 claim of the IR class committing `x`, sealed: the base class, the producer (the IR class's registrant),
/// shard 0's and shard 1's ready bonds (`shard0`, `shard1`: each declares its shard's readiness class), the base class's outsiders and
/// the watcher at DAA 1 (the engine is created there); the plan at `SHARD_AT`; the claim at `CLAIM_AT`; the seal two blocks later.
fn sealed(f: &Fixture, x: &Execution, shard0: &[u64], shard1: &[u64]) -> (Run, Hash64) {
    sealed_with_policy(f, x, shard0, shard1, engine_policy())
}

fn sealed_with_policy(f: &Fixture, x: &Execution, shard0: &[u64], shard1: &[u64], policy: PanelPolicyV1) -> (Run, Hash64) {
    let extras = PalwTransitionExtrasV1 {
        admission_independence_daa: Some(0),
        panel_economy_active: true,
        audit_2026_09_23_active: true,
        panel_v3: Some(PalwPanelV3InputsV1 {
            draw: Default::default(),
            capability_proof: false,
            floor_class: h64(1),
            approved_beacons: vec![challenge_policy()],
            beacon_source: PalwPanelV3BeaconSourceV1::Reference {
                events: Vec::new(),
                eligible_profiles: BTreeSet::new(),
                works: Vec::new(),
                sealed: Vec::new(),
            },
        }),
        ..Default::default()
    };
    let mut p = params();
    let mut mirror = *p.panel_v3().unwrap();
    mirror.policy = policy;
    p = p.with_panel_v3(Some(mirror));
    let mut run = Run { p, s: PalwChainStateV2::genesis(), daa: 0, extras };
    let ready = |s: u16| palw_tir_shard_ready_class_v1(&f.class_id, 2, s);
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
        bond(PRODUCER, &[h64(1)]),
        bond(WATCHER, &[]),
    ];
    for n in shard0 {
        objects.push(bond(*n, &[ready(0)]));
    }
    for n in shard1 {
        objects.push(bond(*n, &[ready(1)]));
    }
    for n in OUTSIDERS {
        objects.push(bond(n, &[h64(1)]));
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
    assert!(run.s.panel_v3().is_some(), "the engine exists past the fence");
    run.at(
        SHARD_AT,
        &[PalwConsensusObjectV2::TirShardPlanDeclared { class_id: f.class_id, s_l: 2, s_p: 1, signature: vec![1; 8] }],
        None,
    );
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
    run.at(CLAIM_AT, &[], Some(&env));
    assert_eq!(run.phase(&claim), PalwClaimPhaseV2::Provisional, "accepted under the V3 rule, not ended at admission");
    assert_eq!(
        run.record(&claim).strata,
        Some(PanelStrataV1 { count: 2, class_seats: 3, outsider: true }),
        "a class with a shard plan enters the engine in strata: one a shard, the outsider-judged claim's outsider first"
    );
    // The checkpoint block, then the seal against it (`seal_depth_blocks` 1).
    run.step(&[]);
    run.step(&[]);
    assert_eq!(run.record(&claim).phase, ClaimPhaseV3::Sealed);
    (run, claim)
}

/// [`sealed`], then certified and drawn: returns the claim and its bound DAA.
fn drawn(f: &Fixture, x: &Execution, shard0: &[u64], shard1: &[u64]) -> (Run, Hash64, u64) {
    let (mut run, claim) = sealed(f, x, shard0, shard1);
    let release = run.record(&claim).seal.unwrap().anchor_slot;
    let proof = run.proof_for(&claim);
    run.at(release + 4, &[], None);
    run.at(release + 5, &[PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(proof) }], None);
    run.at(release + 10, &[], None);
    (run, claim, release + 10)
}

fn part(run: &Run, claim: Hash64, shard: u16, signed_daa: u64) -> PalwConsensusObjectV2 {
    let panel = run.s.panel(&claim).expect("a panel").clone();
    let slice = &panel.seats[usize::from(shard) * 4..usize::from(shard) * 4 + 4];
    let masks = palw_tir_shard_assignment_v1(&panel.anchor, &claim, shard, 1);
    let receipt = |bond: PalwBondKeyV2, segments| PalwSeatReceiptV4 {
        receipt: PalwSeatReceiptV2 { claim, verdict: PalwReceiptVerdictV2::Valid, seat_bond: bond, signed_daa, signature: vec![7; 8] },
        shard,
        segments,
    };
    let mut receipts = vec![receipt(slice[0].bond, palw_tir_shard_outsider_mask_v1(1))];
    for (i, seat) in slice[1..].iter().enumerate() {
        receipts.push(receipt(seat.bond, masks[i]));
    }
    PalwConsensusObjectV2::TirShardReceiptLicensed { part: PalwTirShardPartV1 { claim, shard, receipts } }
}

#[test]
fn a_sharded_class_is_drawn_per_shard_by_the_permissionless_panel_and_licensed_by_its_parts() {
    let f = fixture_gqa();
    let x = f.honest();
    let (mut run, claim, bound) = drawn(&f, &x, &SHARD0, &SHARD1);
    let ClaimPhaseV3::Bound(binding) = run.record(&claim).phase else { panic!("bound: {:?}", run.record(&claim).phase) };
    // The handoff: one V2 panel record, anchored at the V3 seed, stratum-major `[outsider] ++ 3 class seats` a shard.
    let panel = run.s.panel(&claim).expect("the V2 panel record").clone();
    assert_eq!(panel.anchor, binding.panel_seed_v3);
    assert_eq!(run.phase(&claim), PalwClaimPhaseV2::PanelBound { bound_daa: bound });
    let seats: Vec<u64> = panel.seats.iter().map(|seat| id_of(&seat.bond)).collect();
    assert_eq!(seats.len(), 8);
    for (shard, members) in [(0usize, SHARD0), (1, SHARD1)] {
        let slice = &seats[shard * 4..shard * 4 + 4];
        assert!(OUTSIDERS.contains(&slice[0]), "shard {shard}: its outsider is the base class's: {slice:?}");
        assert!(slice[1..].iter().all(|n| members.contains(n)), "shard {shard}: its class seats proved the shard: {slice:?}");
        assert_eq!(slice.iter().collect::<BTreeSet<_>>().len(), 4);
    }
    assert_ne!(seats[0], seats[4], "an operator holds at most one outsider seat of the claim");
    assert!(!seats.contains(&PRODUCER) && !seats.contains(&WATCHER));
    // RFC-0006's per-shard record, written by the V3 bind: the armed machinery runs from here.
    let record = run.s.tir_shard_claim(&claim).expect("the per-shard record").clone();
    assert_eq!((record.s_l, record.s_p, record.outsider, record.drawn_permille.len()), (2, 1, true, 8));
    // One ledger: the V2 duty row holds each bond once, at the engine's exposure.
    let row = run.s.panel_duties_of(&claim).expect("a duty row").clone();
    assert_eq!(row.len(), 8);
    for seat in &panel.seats {
        assert_eq!(run.s.reserved_exposure(&seat.bond), binding.exposure as u128);
        assert_eq!(run.s.panel_v3().unwrap().reserved(&BondIdV1::from(seat.bond)), binding.exposure as u128);
    }
    // A flat licence is refused by name; two parts license it by cells.
    assert!(
        apply_palw_transition_v7(
            &run.s,
            &run.p,
            None,
            &PalwBlockContextV2 { block: h64(0xB10C_FFFF), daa_score: bound + 1, blue_score: bound + 1, subsidy: 1_000_000_000 },
            &[PalwConsensusObjectV2::ReceiptLicensed { claim, receipts: vec![] }],
            PalwBlockWorkV3::None,
            &[],
            Hash64::default(),
            false,
            false,
            false,
            true,
            &run.extras,
        )
        .is_err(),
        "a claim drawn per shard licenses by its parts only"
    );
    let first = part(&run, claim, 0, bound + 1);
    run.at(bound + 1, &[first], None);
    assert!(matches!(run.phase(&claim), PalwClaimPhaseV2::PanelBound { .. }), "one part licenses no claim");
    let second = part(&run, claim, 1, bound + 1);
    run.at(bound + 2, &[second], None);
    let licensed = run.s.claim(&claim).unwrap().clone();
    assert!(matches!(licensed.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "{:?}", licensed.phase);
    assert_eq!(licensed.rcore.basis_k, 3, "every cell: its class seats and its outsider");
    for seat in &panel.seats {
        assert!(run.s.slashable_lock(seat.bond, claim).is_some(), "seat {} locked its scaled price", id_of(&seat.bond));
    }
    // The engine releases the claim on the next block (it left its Panel).
    run.step(&[]);
    assert!(matches!(run.record(&claim).phase, ClaimPhaseV3::Released { .. }));
}

#[test]
fn a_stratum_short_of_ready_bonds_ends_the_claim_no_capable_panel_never_a_flat_panel() {
    let f = fixture_gqa();
    let x = f.honest();
    // Shard 1 has two ready bonds: no stratum of three class seats.
    let (mut run, claim) = sealed(&f, &x, &SHARD0, &SHARD1[..2]);
    let release = run.record(&claim).seal.unwrap().anchor_slot;
    let proof = run.proof_for(&claim);
    run.at(release + 4, &[], None);
    run.at(release + 5, &[PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(proof) }], None);
    let before = run.s.bond(&bond_key(PRODUCER)).unwrap().clone();
    run.at(release + 10, &[], None);
    assert!(
        matches!(run.record(&claim).phase, ClaimPhaseV3::Voided { reason: NonFraudReasonV1::NoCapablePanel, .. }),
        "{:?}",
        run.record(&claim).phase
    );
    assert!(
        matches!(run.phase(&claim), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::PermissionlessNoCapablePanel, .. }),
        "non-fraud: {:?}",
        run.phase(&claim)
    );
    assert!(run.s.panel(&claim).is_none() && run.s.tir_shard_claim(&claim).is_none(), "no flat Panel was drawn in its place");
    let after = run.s.bond(&bond_key(PRODUCER)).unwrap();
    assert_eq!((after.slashed, after.collateral), (before.slashed, before.collateral), "a non-fraud end charges the producer nothing");
    assert!(run.s.withholding_strikes(&bond_key(PRODUCER)).is_none(), "no strike");
}

#[test]
fn an_oversized_shard_population_cannot_prioritize_large_bonds_or_a_partial_panel() {
    use kaspa_consensus_core::palw_permissionless_panel_v1::{PanelErrorV1, panel_stratified_candidates_v1};
    let f = fixture_gqa();
    let x = f.honest();
    let (run, claim) = sealed(&f, &x, &SHARD0, &SHARD1);
    let record = run.record(&claim);
    let strata = record.strata.unwrap();
    let population = record.snapshot.unwrap().candidates.len() as u32;
    assert!(population > strata.seat_count() as u32);
    let mut policy = engine_policy();
    policy.max_candidates = population - 1;
    let inputs = run.extras.panel_v3.as_ref().unwrap();
    assert_eq!(
        panel_stratified_candidates_v1(&run.s, claim, inputs.floor_class, run.daa, policy, inputs.draw, false, &strata),
        Err(PanelErrorV1::ResourceLimit),
        "capacity never keeps just the largest bonds"
    );
    // Re-run from genesis with the smaller capacity committed in the policy. The harness checks
    // the full fold's deltas and reloads its carriage, including the non-fraud terminal outcome.
    let (mut limited, claim) = sealed_with_policy(&f, &x, &SHARD0, &SHARD1, policy);
    assert!(limited.record(&claim).snapshot.unwrap().candidates.is_empty(), "no chosen subset");
    let release = limited.record(&claim).seal.unwrap().anchor_slot;
    let proof = limited.proof_for(&claim);
    limited.at(release + 4, &[], None);
    limited.at(release + 5, &[PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(proof) }], None);
    let before = limited.s.bond(&bond_key(PRODUCER)).unwrap().clone();
    limited.at(release + 10, &[], None);
    assert!(matches!(limited.phase(&claim), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::PermissionlessNoCapablePanel, .. }));
    assert!(limited.s.panel(&claim).is_none());
    let after = limited.s.bond(&bond_key(PRODUCER)).unwrap();
    assert_eq!((before.collateral, before.slashed), (after.collateral, after.slashed));
}

/// A lane of leaf `i` moved by one inside its proven interval, if it can be.
fn forged_lane(f: &Fixture, values: &mut [Vec<i128>], i: usize) -> Option<usize> {
    let leaf = &f.leaves[i];
    let lane = leaf.value_count as usize / 2;
    let v = f.values[i][lane];
    let iv = f.interval(leaf);
    let forged = [v + 1, v - 1].into_iter().find(|w| iv.contains(*w))?;
    values[i][lane] = forged;
    Some(lane)
}

/// **The liar**: one lane of one leaf forged, a leaf whose cone close convicts it in one move.
fn liar(f: &Fixture) -> (u64, Execution) {
    let count = f.leaves.len();
    for i in (count * 2 / 3..count).chain(count / 3..count * 2 / 3) {
        let mut values = f.values.clone();
        let Some(lane) = forged_lane(f, &mut values, i) else { continue };
        let x = f.commit(&values, &f.rows, &f.generated);
        let Ok(r) = build_tir_cone_refutation_v1(&x.binding, i as u64, &Store { f, x: &x }, &RULES) else { continue };
        if matches!(check_tir_cone_refutation_v1(&r, &RULES), Ok(v) if v.fault == PalwStepFaultV1::ComputationMismatch { value_index: lane as u32 })
        {
            return (i as u64, x);
        }
    }
    panic!("{}: no leaf to forge", f.name);
}

#[test]
fn a_bond_outside_every_seat_convicts_a_v3_sharded_lie_from_public_material() {
    let f = fixture_gqa();
    let (leaf, lie) = liar(&f);
    let (mut run, claim, bound) = drawn(&f, &lie, &SHARD0, &SHARD1);
    let seats = run.seats(&claim);
    let watcher = bond_key(WATCHER);
    assert!(!seats.contains(&watcher));
    // The watcher's targets: every shard of the claim (it seats none), each over its outsider span, no seat.
    let watch = palw_tir_shard_watch_duties_v1(&run.s, &run.p, &watcher);
    assert_eq!(watch.len(), 2, "both shards of the one live sharded claim");
    for (i, d) in watch.iter().enumerate() {
        let place = d.tir_shard.expect("a shard place");
        assert_eq!((d.claim_id, place.shard, place.s_l, place.s_p, place.outsider), (claim, i as u16, 2, 1, true));
        assert_eq!((d.seat_bond, d.seat_index), (watcher, PALW_TIR_SHARD_WATCH_NO_SEAT_V1));
        assert_eq!((d.execution_root, d.trace_root), (lie.binding.committed_execution_root, lie.binding.full_logits_trace_root));
    }
    // A seat of shard 0 watches only shard 1; the producer watches nothing of its own claim.
    let seat0 = seats[1];
    assert_eq!(
        palw_tir_shard_watch_duties_v1(&run.s, &run.p, &seat0).iter().map(|d| d.tir_shard.unwrap().shard).collect::<Vec<_>>(),
        vec![1]
    );
    assert!(palw_tir_shard_watch_duties_v1(&run.s, &run.p, &bond_key(PRODUCER)).is_empty());
    // From the claim's public material (the capture the liar served) alone: the cone close, the watcher's bond the accuser.
    let refutation = refute(&f, &lie, leaf);
    let record = run.s.claim(&claim).unwrap().clone();
    let mut accusation = palw_tir_one_move_accusation_v1(
        claim,
        &record,
        watcher,
        PalwCourtVerdictV2::ExecutorGuilty,
        PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) },
    );
    accusation.signature = vec![9; 8];
    let court = PalwCourtParamsV2::new(LADDER, 20, 2).expect("a court");
    assert_eq!(
        palw_tir_one_move_verdict_v1(&run.s, &record, &accusation, &court, LADDER, PalwPromptIdsFormV1::Flat),
        Ok(PalwCourtVerdictV2::ExecutorGuilty),
        "the acceptance layer re-derives the verdict"
    );
    let collateral = run.s.bond(&bond_key(PRODUCER)).unwrap().collateral;
    run.at(bound + 1, &[PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(accusation) }], None);
    assert!(
        matches!(run.phase(&claim), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
        "a non-seat bond convicts the V3-bound sharded claim: {:?}",
        run.phase(&claim)
    );
    assert!(run.s.bond(&bond_key(PRODUCER)).unwrap().collateral < collateral, "the producer is charged");
    for seat in &seats {
        assert_eq!(run.s.bond(seat).unwrap().slashed, 0, "no seat signed anything, none is charged");
    }
    run.step(&[]);
    assert!(palw_tir_shard_watch_duties_v1(&run.s, &run.p, &watcher).is_empty(), "a voided claim is watched no more");
}

#[test]
fn a_non_seat_run_demand_holds_the_v3_window_and_defaults_producer_withholding() {
    let f = fixture_gqa();
    let x = f.honest();
    let (mut run, claim, bound) = drawn(&f, &x, &SHARD0, &SHARD1);
    let watcher = bond_key(WATCHER);
    let demand = PalwConsensusObjectV2::DefaultAccusedTirStep {
        accusation: Box::new(PalwTirStepAccusationV1 {
            claim,
            unit: PalwDaUnitV1::TirStepRun { first: 0, count: 4 },
            accuser: watcher,
            signature: vec![1; 8],
        }),
    };
    run.at(bound + 1, &[demand], None);
    let session = run.s.da_session(&claim, &watcher).expect("the watcher's session").clone();
    assert!(!session.accuser_is_seat, "a non-seat session, on the non-seat budget");
    assert!(run.s.palw_accusation_pending_v1(&claim));
    // The engine's window (3 DAA, one redraw) is long past: the pending demand holds it — no redraw, no expiry.
    run.at(bound + 20, &[], None);
    assert_eq!(run.record(&claim).binding_history.len(), 1, "no redraw while the demand is open");
    assert!(matches!(run.phase(&claim), PalwClaimPhaseV2::PanelBound { .. }));
    // Nobody answers: the default convicts the producer.
    run.at(session.deadline_daa + 1, &[], None);
    assert!(
        matches!(run.phase(&claim), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }),
        "{:?}",
        run.phase(&claim)
    );
}
