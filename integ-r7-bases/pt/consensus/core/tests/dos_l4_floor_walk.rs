//! **LANE L4 — the two-clock fix's D1 floor was an O(claims) walk run once per panel derivation.**
//!
//! The audit (finding 19): `palw_settled_anchor_floor_daa_v1` (palw_panel_v2.rs) iterated EVERY
//! claim in the state, filtered the Final attempt ones, collected and sorted them — and
//! `processor.rs` calls it through `palw_bond_maturity_window_at` once per `PanelBound` it
//! validates (palw_v2_validate_objects) and once per `Provisional` claim every time it assembles a
//! block's panels (the assembler loop). So a template cost `provisional x claims` and a block with
//! k PanelBound objects `k x claims` — measured at 3.3 ns per retained claim per walk (debug).
//!
//! Since `6bb8c844` (fix #13a) the floor reads the rooted, pruned anchor ring `recent_anchor_daas`
//! by binary search and never looks at the claim table. Asserted here on states built by the REAL
//! fold: `n` never-bound attempts (each voids at BindTimeout and is retained because retirement is
//! set long) plus a handful of licences, at two table sizes four times apart — the answers are
//! identical and the read does not grow with the table.

use std::time::Instant;

use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_settled_anchor_floor_daa_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwPanelSeatV2, PalwPwuRuleV2,
    PalwStateParamsV2, PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras, palw_operator_id_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

const PWU_PER_INFERENCE: u64 = 7_900;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn base() -> Hash64 {
    h(0xBA5E)
}

fn key() -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xB0), index: 0 })
}

fn seat() -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0x5EA7), index: 0 })
}

fn attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(0xD0),
            challenge: h(nonce ^ 0x00C0_FFEE),
            class_id: base(),
            executor_bond: key().0,
            executor_pubkey: vec![7u8; 32],
            operator_id: palw_operator_id_v2(&[21u8; 8]),
            artifact_root: h(0xA7),
            trace_root: h(nonce ^ 0x7A),
            output_root: h(nonce ^ 0x00FF),
            pwu,
            trace_manifest_root: h(0xD0),
            trace_chunk_count: 8,
            trace_retention_daa: u64::MAX,
            execution_root: h(nonce ^ 0x4E),
        },
        signature: vec![9u8; 64],
    }
}

/// `n` never-bound floor attempts, then `licences` attempts walked to `ReceiptLicensed` on a panel
/// of one, one every 5 DAA.
fn build(n: u64, licences: u64) -> PalwChainStateV2 {
    let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1_000_000, base(), 4, 1_000, 100, 1_000, 0)
        .unwrap()
        .with_claim_retirement_daa(10_000_000)
        .unwrap();
    let extras = PalwTransitionExtrasV1 { audit_2026_09_23_active: true, settled_anchor_depth: Some(30), ..Default::default() };
    let mut state = PalwChainStateV2::genesis();
    let genesis = vec![
        Obj::BondRegistered {
            bond: key(),
            pubkey: vec![7u8; 32],
            operator_pubkey: vec![21u8; 8],
            collateral: 1_000_000_000_000_000,
            payout_payload: h(0x9A4),
            capable_classes: Default::default(),
            signature: vec![9u8; 64],
        },
        Obj::BondRegistered {
            bond: seat(),
            pubkey: vec![8u8; 32],
            operator_pubkey: vec![22u8; 8],
            collateral: 1_000_000_000_000_000,
            payout_payload: h(0x9A5),
            capable_classes: Default::default(),
            signature: vec![9u8; 64],
        },
        Obj::ClassRegistered {
            class_id: base(),
            artifact_root: h(0xA7),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: PWU_PER_INFERENCE },
            initial_target: u128::MAX / 2,
            share_permille: 1_000,
            activation_daa: 0,
            admission: None,
        },
    ];
    for daa in 1..=n + 1 {
        let ctx = PalwBlockContextV2 { block: h(daa | 0x1000_0000), daa_score: daa, blue_score: daa, subsidy: 1_000_000 };
        let objects: &[Obj] = if daa == 1 { &genesis } else { &[] };
        let env = (daa > 1).then(|| {
            let target = state.class_target(&base()).unwrap().target;
            attempt(kaspa_consensus_core::palw_pwu::palw_pwu_v1(target, PWU_PER_INFERENCE), daa)
        });
        state =
            apply_palw_transition_v2_with_extras(&state, &params, &ctx, objects, env.as_ref(), false, false, false, false, &extras)
                .unwrap()
                .0;
    }
    // The licences: attempt at `d`, bound at `d + 1`, licensed at `d + 2`.
    let mut daa = n + 1;
    for _ in 0..licences {
        daa += 5;
        let target = state.class_target(&base()).unwrap().target;
        let env = attempt(kaspa_consensus_core::palw_pwu::palw_pwu_v1(target, PWU_PER_INFERENCE), daa);
        let claim = attempt_id_v2(&env.attempt);
        let ctx = |d: u64| PalwBlockContextV2 { block: h(d | 0x1000_0000), daa_score: d, blue_score: d, subsidy: 1_000_000 };
        let fold = |s: &PalwChainStateV2, d: u64, objects: &[Obj], env: Option<&PalwAttemptEnvelopeV2>| {
            apply_palw_transition_v2_with_extras(s, &params, &ctx(d), objects, env, false, false, false, false, &extras).unwrap().0
        };
        state = fold(&state, daa, &[], Some(&env));
        let seats = vec![PalwPanelSeatV2 { bond: seat(), operator_id: h(0x5EA7) }];
        state = fold(&state, daa + 1, &[Obj::PanelBound { claim, anchor: h(77), seats }], None);
        let receipts = vec![PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: seat(),
            signed_daa: 0,
            signature: Vec::new(),
        }];
        state = fold(&state, daa + 2, &[Obj::ReceiptLicensed { claim, receipts }], None);
        daa += 2;
    }
    state
}

/// **Finding 19, fixed.** The floor is the same answer, read at the same cost, whatever the claim
/// table holds. Queries ask for the 3rd most recent anchor, so 8 licences answer both the bootstrap
/// waiver (`None`, before the third) and exact floors.
#[test]
fn the_d1_floor_read_does_not_grow_with_the_claim_table() {
    const DEPTH: u64 = 3;
    let licences = 8u64;
    let mut rows = Vec::new();
    let mut answers = Vec::new();
    for n in [1_000u64, 4_000] {
        let state = build(n, licences);
        let claims = state.claims_iter().count();
        assert_eq!(state.settled_attempt_finals(), licences, "one anchor per licence");
        // The same history after the table, so the same query offsets see the same ring.
        let ring_base = n + 1;
        let queries: Vec<u64> = (0..=licences * 7 + 2).map(|i| ring_base + i).collect();
        answers.push(
            queries
                .iter()
                .map(|&q| palw_settled_anchor_floor_daa_v1(&state, q, DEPTH).map(|d| d.saturating_sub(ring_base)))
                .collect::<Vec<_>>(),
        );
        let reps = 2_000u32;
        let t = Instant::now();
        for i in 0..reps {
            std::hint::black_box(palw_settled_anchor_floor_daa_v1(&state, ring_base + u64::from(i % 64), DEPTH));
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / f64::from(reps);
        println!("retained claims {claims:>6}, ring {:?}: one floor read = {us:.3} us (debug build)", state.recent_anchor_daas());
        rows.push((claims as f64, us));
    }
    assert!(rows[1].0 > rows[0].0 * 3.0, "the second table is ~4x the first");
    assert_eq!(answers[0], answers[1], "the floor reads the anchor ring, never the claim table");
    // Four times the claims costs nothing measurable: a walk grew 3.3 ns per retained claim, i.e.
    // ~10 us more here; the read stays within noise of the small table (a generous 2x + 2 us).
    assert!(rows[1].1 < rows[0].1 * 2.0 + 2.0, "the read is independent of the claim table: {rows:?}");
}

/// The audit's measurement, kept for its numbers: the marginal cost per retained claim of a floor
/// read, which the pre-`6bb8c844` walk put at 3.3 ns and the ring puts at ~0.
#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: the D1 floor walk was linear in retained claims (finding 19); closed by 6bb8c844's anchor ring — this now only prints the marginal cost"]
fn the_d1_floor_walk_is_linear_in_retained_claims() {
    let mut rows = Vec::new();
    for n in [1_000u64, 4_000] {
        let state = build(n, 0);
        let claims = state.claims_iter().count();
        let reps = 200u32;
        let t = Instant::now();
        for i in 0..reps {
            std::hint::black_box(palw_settled_anchor_floor_daa_v1(&state, n + u64::from(i), 30));
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / f64::from(reps);
        println!("retained claims {claims:>6}: one floor read = {us:.3} us (debug build)");
        rows.push((claims as f64, us));
    }
    let per_claim_ns = (rows[1].1 - rows[0].1) * 1_000.0 / (rows[1].0 - rows[0].0);
    println!("marginal cost = {per_claim_ns:.3} ns per retained claim per read (the pre-fix walk: 3.3)");
}
