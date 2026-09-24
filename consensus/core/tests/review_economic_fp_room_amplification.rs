//! Adversarial review (economic / DoS re-exploit lens) of bc7f02dd (#11): a free-prompt claim
//! counts as ONE WHOLE claim of its class on the registry's budget (`PalwInflightTallyV1::counted`,
//! `palw_state_v2.rs` ~:8840; `panel_room_v1` multiplies it by `economic_ccu_per_claim x seats`),
//! whatever its size — while its cost (the exposure it reserves) scales with its quanta. The
//! smallest commitment therefore buys a full attempt's slot on a held class, and a few of them
//! close the class's ATTEMPT lane (`ClassInflightCapped` here, `PanelRoomExhausted` past the work
//! target) at a fraction of an attempt's collateral, with no draw to win and no escrow at stake.
//!
//! Helpers copied verbatim from `dos_l1_collateral_atomicity.rs` (t12 params, real fold).
//!
//! **Fixed (2026-09-24 review):** the class gate counts a free-prompt claim in whole canonical jobs
//! of the quanta it committed, rounded up over the class (`palw_inflight_claims_counted_v1`), so
//! `cap` one-quantum commitments take `⌈cap / 8⌉` of the class's slots and the attempt lane stays
//! open; filling a slot through the free-prompt lane now costs what an attempt's slot costs. The
//! defect cannot be re-measured below the fence (below it the gate does not count free-prompt claims
//! at all), so this test asserts only the fixed behaviour.
#![allow(dead_code)]

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{PALW_T12_SETTLED_ANCHOR_DEPTH, Params};
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_admission_v2::{PalwAdmissionParamsV2, PalwEpochBudgetFencesV1};
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_trace_manifest_root_v1,
    challenge_v2,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwMergedWorkV1, PalwPwuRuleV2,
    PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v7,
    palw_operator_id_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn bundle(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("testnet-12 is ConsensusV2"),
    }
}

/// `(class_id, declared leaves, initial target, slash per pwu)` of t12's liveness floor.
fn floor_row(b: &PalwConsensusParamsV2) -> (Hash64, u64, u128, u64) {
    for o in b.genesis_objects.iter() {
        if let PalwConsensusObjectV2::ClassRegistered {
            class_id,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
            initial_target,
            slash_value_per_pwu,
            ..
        } = o
            && *class_id == b.base_class_id
        {
            return (*class_id, *pwu_per_inference, *initial_target, *slash_value_per_pwu);
        }
    }
    panic!("t12 registers a DerivedV1 floor")
}

const NET: u64 = 0xD05_0012;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xB0_0000 + n), index: 0 })
}

fn pubkey(n: u64) -> Vec<u8> {
    vec![(n as u8).wrapping_add(1); 32]
}

fn op_pubkey(n: u64) -> Vec<u8> {
    vec![(n as u8).wrapping_add(101); 32]
}

fn register_bond(n: u64, collateral: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: pubkey(n),
        operator_pubkey: op_pubkey(n),
        collateral,
        payout_payload: h(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

/// One distinct inference (`seed` moves every root) by bond `n`.
fn attempt(class_id: Hash64, pwu: u64, n: u64, seed: u64) -> PalwAttemptEnvelopeV2 {
    let bond = bond_key(n).0;
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(NET),
            challenge: challenge_v2(h(NET), h(0x5EED_0000 + seed), 1_700_000_000 + seed, seed, class_id, &bond),
            class_id,
            executor_bond: bond,
            executor_pubkey: pubkey(n),
            operator_id: palw_operator_id_v2(&op_pubkey(n)),
            artifact_root: h(0xA27),
            trace_root: h(0x1700_0000 + seed),
            output_root: h(0x2700_0000 + seed),
            pwu,
            trace_manifest_root: attempt_trace_manifest_root_v1(h(0x1700_0000 + seed), PALW_ATTEMPT_V2_TRACE_CHUNKS),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: 9_999_999,
            execution_root: h(0x3700_0000 + seed),
        },
        signature: vec![0u8; MLDSA87_SIGNATURE_LEN],
    }
}

fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(0xC000_0000 + block), daa_score: daa, blue_score: blue, subsidy: 0 }
}

fn armed() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        audit_2026_09_11_deep_active: true,
        audit_2026_09_23_active: true,
        panel_economy_active: true,
        objective_offence_daa: Some(0),
        settled_anchor_depth: Some(PALW_T12_SETTLED_ANCHOR_DEPTH),
        ..Default::default()
    }
}

fn fences() -> PalwEpochBudgetFencesV1 {
    PalwEpochBudgetFencesV1 { audit_2026_09_23_active: true, ..Default::default() }
}

struct Fold<'a> {
    p: &'a PalwStateParamsV2,
    admission: &'a PalwAdmissionParamsV2,
    extras: PalwTransitionExtrasV1,
}

impl Fold<'_> {
    fn go(
        &self,
        s: &PalwChainStateV2,
        c: &PalwBlockContextV2,
        objects: &[PalwConsensusObjectV2],
        own: PalwBlockWorkV3<'_>,
        merged: &[PalwMergedWorkV1<'_>],
        own_key: Hash64,
    ) -> (PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>) {
        apply_palw_transition_v7(
            s,
            self.p,
            Some(self.admission),
            c,
            objects,
            own,
            merged,
            own_key,
            false,
            false,
            false,
            false,
            &self.extras,
        )
        .unwrap_or_else(|e| panic!("fold at daa {} refused: {e:?}", c.daa_score))
    }
}

fn merged(env: &PalwAttemptEnvelopeV2, carrying: u64, key: u64) -> PalwMergedWorkV1<'_> {
    PalwMergedWorkV1 {
        carrying_block: h(0xCB00_0000 + carrying),
        work: PalwBlockWorkV3::Attempt(env),
        execution_key: h(0xE000_0000 + key),
        subsidy: 0,
        escrow_carve: None,
        bits: 0,
        job_anchor: kaspa_hashes::Hash64::default(),
    }
}

fn setup(class: (Hash64, u64, u128, u64), bonds: &[(u64, u64)]) -> Vec<PalwConsensusObjectV2> {
    let (class_id, leaves, target, slash) = class;
    let mut v = vec![PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root: h(0xA27),
        slash_value_per_pwu: slash,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: leaves },
        initial_target: target,
        share_permille: 1000,
        activation_daa: 0,
        admission: None,
    }];
    for (n, c) in bonds {
        v.push(register_bond(*n, *c));
    }
    v
}

fn fp_commitment(class_id: Hash64, leaves: u64, n: u64, seed: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::FreePromptCommitted {
        job_pin: kaspa_hashes::Hash64::default(),
        claim: h(0xF0_0000 + seed),
        class_id,
        bond: bond_key(n),
        executor_pubkey: pubkey(n),
        work_leaves: leaves,
        prompt_token_ids_hash: h(0x7E_0000 + seed),
        prompt_tokens: 0,
        prompt_token_ids: Vec::new(),
        decode_tokens_executed: 1,
        trace_root: h(0x1F00_0000 + seed),
        output_root: h(0x2F00_0000 + seed),
        execution_root: h(0x3F00_0000 + seed),
        trace_chunk_count: 1,
        trace_retention_daa: 9_999_999,
        consumed_prefix_state: kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(class_id),
    }
}


#[test]
fn review_economic_a_one_quantum_free_prompt_takes_an_eighth_of_a_held_class_slot() {
    use kaspa_consensus_core::palw_execution_lane_v1::PalwExecLaneFoldV1;
    use kaspa_consensus_core::palw_model_registry_v1::{PALW_REGISTRY_GLOBALS_V1, PalwModelLifecycleV1, PalwModelRegistryFoldV1, PalwModelWorkV1};
    let p = t12();
    let b = bundle(&p);
    let sp = &b.state;
    let floor = floor_row(&b);
    let certified = sp.fp_certified_classes().expect("t12 lane-certifies its classes");
    let (held_id, held_rule, held_target) = b
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, initial_target, .. }
                if *class_id != b.base_class_id && certified.contains(class_id) =>
            {
                Some((*class_id, pwu_rule.clone(), *initial_target))
            }
            _ => None,
        })
        .expect("t12 lane-certifies a held class");
    let span = 10;
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = b.panel.seat_count();
    let work = PalwModelWorkV1 { verification_ccu: 1_000, economic_ccu_per_claim: 500, ops_supported: true, ..Default::default() };
    let registry = PalwModelRegistryFoldV1 {
        globals,
        span_daa: span,
        genesis_works: [(floor.0, work), (held_id, work)].into_iter().collect(),
        grace_until_daa: 0,
        admission_audit_period_daa: p.palw_admission_audit_period_daa,
        readiness_v2_active: p.palw_readiness_v2_at(0),
    };
    let with_registry = |audit: bool| PalwTransitionExtrasV1 {
        audit_2026_09_23_active: audit,
        model_registry: Some(registry.clone()),
        round_lane: Some(PalwExecLaneFoldV1 { schedule_span_daa: span, ..Default::default() }),
        ..armed()
    };
    let big = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let mut objects = setup(floor, &[(1, big), (2, big)]);
    objects.push(PalwConsensusObjectV2::ClassRegistered {
        class_id: held_id,
        artifact_root: h(0xB27),
        slash_value_per_pwu: floor.3,
        pwu_rule: held_rule.clone(),
        initial_target: held_target,
        share_permille: 0,
        activation_daa: 0,
        admission: None,
    });
    let fold = |s: &PalwChainStateV2, daa: u64, objs: &[PalwConsensusObjectV2], own: PalwBlockWorkV3<'_>, audit: bool| {
        apply_palw_transition_v7(s, sp, Some(&b.admission), &ctx(daa, if daa == 2 { 110 } else if daa == 1 { 101 } else { 108 + daa }, daa), objs, own, &[], h(0xE0 + daa), false, false, false, false, &with_registry(audit))
    };
    let (s1, _, _) = fold(&PalwChainStateV2::genesis(), 1, &objects, PalwBlockWorkV3::None, true).expect("genesis");
    let (s2, _, _) = fold(&s1, 2, &[], PalwBlockWorkV3::None, true).expect("span boundary");
    // The class has reached Ready and is admitting (Active) — the state the user's matrix walks
    // into. Written through the carriage (as dos_g2 / dos_repro_1 write the floor's row).
    let mut c = PalwStateCarriageV2::from_state(&s2);
    let row = c.model_lifecycles.get_mut(&held_id).expect("held row");
    row.state = PalwModelLifecycleV1::Active;
    // The genesis works above derive an unbounded cap; a real held class's budget is a handful of
    // claims (Qwen3.6 draws ~20 per span on t12). The COUNTING under test is the same in both the
    // inflight-cap branch and ADR-0137's panel_room branch (`model_registry_inflight`).
    row.profile.max_inflight_claims = 3;
    let cap = row.profile.max_inflight_claims;
    let s = c.into_state_v3(sp, None, false, None).expect("rebuilds");
    println!("held class {held_id}: Active, inflight cap {cap} claims");
    assert!(cap >= 1 && cap <= 64, "a small cap to fill ({cap})");

    // What ONE whole claim of this class reserves (a canonical-size commitment), vs the smallest.
    let canonical = held_rule.canonical_leaves_v1();
    // The smallest commitment the lane prices: ONE quantum of the class's canonical job.
    let one = kaspa_consensus_core::palw_freeprompt_v3::fp_class_quantum_leaves_v1(canonical, sp.fp_quanta_per_canonical_job());
    println!("canonical job {canonical} leaves; one quantum = {one} leaves ({} quanta per job)", sp.fp_quanta_per_canonical_job());
    let (probe, _, _) = fold(&s, 3, &[fp_commitment(held_id, canonical, 1, 0x100)], PalwBlockWorkV3::None, true).expect("a whole-job commitment");
    let whole = probe.claim(&h(0xF0_0100)).unwrap().reserved;
    let (probe, _, _) = fold(&s, 3, &[fp_commitment(held_id, one, 1, 0x101)], PalwBlockWorkV3::None, true).expect("a one-quantum commitment");
    let tiny = probe.claim(&h(0xF0_0101)).unwrap().reserved;
    println!("reserved: whole-job commitment {whole} sompi; one-quantum commitment {tiny} sompi ({:.0}x cheaper)", whole as f64 / tiny.max(1) as f64);

    // Fill the cap with one-quantum commitments, one per block.
    let mut st = s.clone();
    for i in 0..cap as u64 {
        let (next, _, _) = fold(&st, 3 + i, &[fp_commitment(held_id, one, 1, 0x200 + i)], PalwBlockWorkV3::None, true)
            .unwrap_or_else(|e| panic!("commitment {i}: {e:?}"));
        st = next;
    }
    let spent: u128 = (0..cap as u64).map(|i| st.claim(&h(0xF0_0200 + i)).unwrap().reserved).sum();
    // Now an ATTEMPT on the class by an unrelated producer (bond 2).
    let target = st.class_target(&held_id).expect("held target").target;
    let env = attempt(held_id, kaspa_consensus_core::palw_pwu::palw_pwu_v1(target, canonical), 2, 0x77);
    let daa = 3 + cap as u64;
    let admitted = fold(&st, daa, &[], PalwBlockWorkV3::Attempt(&env), true);
    println!("after {cap} one-quantum commitments ({spent} sompi reserved in total), an attempt on the class: {:?}", admitted.as_ref().map(|_| "accepted"));
    assert!(
        admitted.is_ok(),
        "one-quantum commitments take an eighth of a slot each, so the attempt lane stays open: {admitted:?}"
    );
    // ...and the lane closes only when the commitments add up to the cap in whole jobs. A commitment
    // enters only while a whole slot is free, so the last one to enter is the one that starts the
    // cap's last job: `(cap − 1) × 8 + 1` one-quantum commitments count `cap` claims, the next one
    // and the next attempt are refused by the cap. (In one block, so the fill does not cross the
    // registry's span boundary.)
    let per_job = sp.fp_quanta_per_canonical_job() as u64;
    let total = (cap as u64 - 1) * per_job + 1;
    let rest: Vec<_> = (cap as u64..total).map(|i| fp_commitment(held_id, one, 1, 0x200 + i)).collect();
    let (full, _, _) = fold(&st, daa, &rest, PalwBlockWorkV3::None, true).expect("the rest of the cap in one-quantum commitments");
    let one_more = fold(&full, daa + 1, &[fp_commitment(held_id, one, 1, 0x200 + total)], PalwBlockWorkV3::None, true);
    assert!(matches!(&one_more, Err(PalwStateV2Error::ClassInflightCapped { .. })), "{one_more:?}");
    let refused = fold(&full, daa + 1, &[], PalwBlockWorkV3::Attempt(&env), true);
    assert!(
        matches!(&refused, Err(PalwStateV2Error::ClassInflightCapped { class, inflight, .. }) if *class == held_id && *inflight == cap),
        "{total} one-quantum commitments are {cap} whole jobs: {refused:?}"
    );
    // What the ATTEMPT itself would have reserved for the same slot (on the state with room).
    let (with_room, _, _) = fold(&s, daa, &[], PalwBlockWorkV3::Attempt(&env), true).expect("the attempt with room is accepted");
    let attempt_claim = with_room
        .claims_iter()
        .find(|(_, c)| c.class_id == held_id && matches!(c.source, kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2::Attempt))
        .map(|(_, c)| c.clone())
        .expect("the attempt's claim");
    println!(
        "an attempt on the same class reserves {} sompi (+ escrow {} sompi); one slot via the free-prompt lane costs {tiny} sompi reserved, 0 escrow",
        attempt_claim.reserved, attempt_claim.escrowed_reward
    );
    assert!(tiny < whole && tiny * 8 <= whole + 8, "one quantum is 1/8 of a whole-job commitment");
    assert!(tiny < attempt_claim.reserved, "the slot is cheaper through the free-prompt lane");
}
