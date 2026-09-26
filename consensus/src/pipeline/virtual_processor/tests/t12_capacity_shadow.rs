//! **ADR-0160 S-T4 (consensus half) / S-T5: the capacity shadow, read through the consensus API on
//! testnet-12 chains.**
//!
//! * [`s_t4_the_shadow_read_answers_on_a_testnet12_chain`] (fast, in the suite): the processor's
//!   read (`palw_capacity_shadow_v1`, the one consensus-crate call of the shadow) answers on a real
//!   testnet-12 chain — eight genesis cards as seats, `E` from the tip's subsidy, the reference
//!   ramp's golden `N13k` — and moves nothing (the tip's state root is the same before and after).
//! * [`s_t5_the_shadow_on_the_live_like_13k_floor_chain`] (a measurement run on the capacity
//!   harness, `--ignored`): `L1_floor_13k_live` — one 13,000 MSK producer making floor claims with the
//!   live licence mix (90.6 % all-five / 5.7 % three-seat / 3.8 % S2) and the live bind → licence
//!   delays — read every 10 DAA. Each snapshot: the subject's `N_instant` is today's 2 and E-T3's
//!   20 / 50 / 101 / 203 / 2,030; the identity step (ρ 1, q 0) reproduces every seat's A-1; the
//!   subject's provisional weight stays within its `W_cap` (2 FCW); the chain's own gate never holds
//!   more unlicensed claims than today's `N_instant`. `CAP_SHADOW_DAA` (default 60) sets the length.

use super::t12_claim_capacity::{LicencePolicy, drive, expand, sim};
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::palw_capacity_formulas_v1::{PALW_CAPACITY_FCW_V1, PALW_CAPACITY_REFERENCE_STEPS_V1, PalwCapacityStepV1};
use kaspa_consensus_core::palw_capacity_shadow_v1::{PalwCapacityShadowOptionsV1, PalwCapacityShadowV1};

/// The reference ramp plus the identity step (ρ 1, q 0: today's prices under the new structure).
fn steps_with_identity() -> Vec<PalwCapacityStepV1> {
    let mut steps = PALW_CAPACITY_REFERENCE_STEPS_V1.to_vec();
    steps.push(PalwCapacityStepV1 { from_daa: 0, rho: 1, q_credit_permille: 0 });
    steps
}

/// E-T3's golden: a 13,000 MSK bond with no other commitment holds 20 / 50 / 101 / 203 / 2,030
/// floor claims at ρ 10 / 25 / 50 / 100 / 1000 (q credited at 143‰), and 2 today.
const E_T3_13K: [u64; 5] = [20, 50, 101, 203, 2_030];

#[tokio::test]
async fn s_t4_the_shadow_read_answers_on_a_testnet12_chain() {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    for _ in 0..3 {
        chain.heartbeat(ttpb, Vec::new()).await;
    }
    let (_, before) = chain.tip_state();
    let options = PalwCapacityShadowOptionsV1 { block_mass_limit: config.params.max_block_mass, ..Default::default() };
    let shadow = chain.ctx.consensus.palw_capacity_shadow_v1(options.clone()).expect("a ConsensusV2 node answers");
    let (_, after) = chain.tip_state();
    assert_eq!(before.state_root(), after.state_root(), "a read: the tip is untouched");
    assert_eq!(shadow.seats, 8, "the eight genesis cards are the seats");
    assert!(shadow.claims.is_empty(), "no claim yet");
    assert_eq!(shadow.bounded_immature_today, 0);
    assert_eq!(shadow.bounded_immature_new, 0);
    // E from the tip's subsidy carve (no claim to read it off): testnet-12's 3,200.85 MSK.
    assert_eq!(shadow.reference_escrow / 1_000_000, 320_084, "E = 720‰ of the 4,445.62 MSK subsidy");
    assert_eq!(shadow.steps.iter().map(|s| s.n_instant_13k).collect::<Vec<_>>(), E_T3_13K.to_vec());
    assert_eq!(shadow.carriers_per_block, 3, "three coverage carriers per 500,000-mass block");
    let card = &shadow.bonds[0];
    assert!(card.seat && card.w_cap == 144 * PALW_CAPACITY_FCW_V1, "a card is a seat with W_cap 144 FCW");
    assert!(shadow.summary().starts_with("capacity-shadow: daa="));
    // The same answer twice: the read is a function of the committed tip.
    assert_eq!(chain.ctx.consensus.palw_capacity_shadow_v1(options), Some(shadow));
}

/// Live public testnet-12 bind -> licence delays (DAA 140-170, 309 floor licences) — the
/// `live:floor` histogram of `t12_capacity_run` (`CAP_LIC_DELAY=live:floor`).
const LIVE_FLOOR_DELAYS: [(u64, usize); 23] = [
    (0, 66),
    (1, 118),
    (2, 50),
    (3, 11),
    (4, 14),
    (5, 11),
    (6, 4),
    (7, 4),
    (8, 2),
    (9, 4),
    (10, 4),
    (11, 3),
    (13, 7),
    (14, 2),
    (16, 1),
    (21, 1),
    (24, 1),
    (25, 1),
    (32, 1),
    (39, 1),
    (51, 1),
    (57, 1),
    (66, 1),
];

fn check_snapshot(shadow: &PalwCapacityShadowV1, subject: &kaspa_consensus_core::palw_state_v2::PalwBondKeyV2, rel: u64) {
    let identity = shadow.steps.iter().position(|s| s.step.rho == 1).expect("the identity step");
    let row = shadow.bonds.iter().find(|b| b.bond == *subject).expect("the subject's row");
    // E-T3 on the live chain: the subject is a pure producer, so its N_instant is the golden.
    assert_eq!(row.n_instant_today, 2, "rel {rel}: today a 13k bond holds two floor claims");
    assert_eq!(row.n_instant_new[..5], E_T3_13K, "rel {rel}: E-T3");
    assert!(row.unlicensed_claims <= row.n_instant_today, "rel {rel}: the chain's gate holds at most N_instant unlicensed");
    // J-1: the subject's provisional weight is capped at 2 FCW, whatever it holds.
    assert_eq!(row.w_cap, 2 * PALW_CAPACITY_FCW_V1);
    assert!(row.capped <= row.w_cap && row.reserved_new_total <= row.r_budget, "rel {rel}: W-I1 / W-I4");
    assert!(shadow.bounded_immature_new <= shadow.w_cap_total);
    // The identity step reproduces every seat's A-1 (duties, locks and their own claims) — up to
    // E-4, the one change that is not a price: a void of its own background claims holds its
    // obligation for h_obl under the new rule, and nothing today.
    for seat in shadow.bonds.iter().filter(|b| b.seat) {
        let void_hold: u128 = shadow
            .claims
            .iter()
            .filter(|c| c.bond == seat.bond && c.phase == "voided")
            .map(|c| c.commitment_new[identity] - c.commitment_today)
            .sum();
        assert_eq!(seat.committed_new[identity] - void_hold, seat.committed_today, "rel {rel}: seat {:?} at ρ 1, q 0", seat.bond);
    }
    // The subject's identity commitment differs from today's only by the weight J-1 no longer reserves.
    assert!(row.committed_new[identity] <= row.committed_today);
    // A floor duty is λ-bound at E/5 (the harness's 640.17 MSK).
    if shadow.duty_rows > 0 {
        let e5 = shadow.reference_escrow / 5;
        assert!(shadow.reference_duty.abs_diff(e5) * 100 <= e5, "rel {rel}: floor duty {} vs E/5 {e5}", shadow.reference_duty);
    }
}

#[tokio::test]
#[ignore = "a measurement run on the capacity harness (minutes); run with --ignored, CAP_SHADOW_DAA sets the length"]
async fn s_t5_the_shadow_on_the_live_like_13k_floor_chain() {
    let daa_len: u64 = std::env::var("CAP_SHADOW_DAA").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
    let mut s = sim("S_T5_L1_floor_13k_live", "floor", &[("13000".to_string(), 13_000)], LicencePolicy::Mix(906, 57, 38), 0).await;
    s.lic_delays = Some(expand(&LIVE_FLOOR_DELAYS));
    let subject = s.subjects[0].bond;
    let mut rel = 0;
    let mut max_ms = 0u128;
    while rel < daa_len {
        drive(&mut s, 9, 64).await;
        rel += 10;
        let options = PalwCapacityShadowOptionsV1 {
            steps: steps_with_identity(),
            block_mass_limit: s.chain.config.params.max_block_mass,
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let shadow = s.chain.ctx.consensus.palw_capacity_shadow_v1(options).expect("a ConsensusV2 node answers");
        max_ms = max_ms.max(started.elapsed().as_millis());
        eprintln!("[S-T5] rel {rel}: {} ({} ms)", shadow.summary(), started.elapsed().as_millis());
        let row = shadow.bonds.iter().find(|b| b.bond == subject).unwrap();
        eprintln!(
            "[S-T5] rel {rel}: subject live {} unlicensed {} weight today {} → capped {} FCW, committed today {} → identity {} sompi; \
             claims today/new(ρ=10) {}/{} sompi; seats {} duty rows {} (capped {}) locks {} sompi",
            row.live_claims,
            row.unlicensed_claims,
            row.raw_immature_today / PALW_CAPACITY_FCW_V1,
            row.capped / PALW_CAPACITY_FCW_V1,
            row.committed_today,
            row.committed_new[5],
            shadow.claims_commitment_today,
            shadow.steps[0].claims_commitment_total,
            shadow.seats,
            shadow.duty_rows,
            shadow.duty_rows_capped,
            shadow.seat_lock_total_today,
        );
        check_snapshot(&shadow, &subject, rel);
    }
    eprintln!("[S-T5] {daa_len} DAA: every snapshot passed; the slowest read took {max_ms} ms");
}
