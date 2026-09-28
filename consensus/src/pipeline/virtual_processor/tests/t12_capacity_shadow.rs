//! **ADR-0160 S-T4 (consensus half) / S-T5: the capacity shadow, read through the consensus API on
//! testnet-12 chains.**
//!
//! * [`s_t4_the_shadow_read_answers_on_a_testnet12_chain`] (fast, in the suite): the processor's
//!   read (`palw_capacity_shadow_v1`, the one consensus-crate call of the shadow) answers on a real
//!   testnet-12 chain — eight genesis cards as seats, `E` from the tip's subsidy, the default display
//!   uncredited (q 0: a 13k bond still holds 2 at every ρ); v1's reference ramp, named, prices as the
//!   fold would (143‰ < q_seat: still 2, and the credited step alarms with nothing measured), v1's
//!   E-T3 figures only in the superseded column — and moves nothing (the tip's state root is the same
//!   before and after).
//! * [`s_t5_a8_the_alarm_fires_when_the_auditors_stop_and_clears_when_they_run`] (in the suite):
//!   ADR-0160's A8 drill on a real testnet-12 chain. Two claims of the named O-3 bonds (naive) resolve
//!   `Final` with nobody auditing them: measured q = 0, every step alarms (and a read naming no bond
//!   measures nothing). Then an auditor accuses one of them, nobody answers, the chain convicts its
//!   producer (`DaDefault`, a route the credit prices): q = 500‰, the bar (2 × max(q needed, q_seat,
//!   q_credit) = 500‰) is met, the alarm clears, and the producer reads frozen for good.
//! * [`s_t5_the_shadow_on_the_live_like_13k_floor_chain`] (a measurement run on the capacity
//!   harness, `--ignored`): `L1_floor_13k_live` — one 13,000 MSK producer making floor claims with the
//!   live licence mix (90.6 % all-five / 5.7 % three-seat / 3.8 % S2) and the live bind → licence
//!   delays — read every 10 DAA. Each snapshot: the subject's `N_instant` is today's 2, still 2 at
//!   v1's reference credit (q 143‰ < q_seat; E-T3's 20 / 50 / 101 / 203 / 2,030 only in the
//!   superseded column) and at testnet-12's armed first step (ρ 10, q 0), 4 at a 250‰ credit and 10
//!   at 500‰; the identity step (ρ 1, q 0) reproduces every seat's A-1; the subject's provisional
//!   weight stays within its `W_cap` (2 FCW); the chain's own gate never holds more unlicensed claims
//!   than today's `N_instant`. `CAP_SHADOW_DAA` (default 60) sets the length.

use super::p2_mint_path::{Kind, Minting, beat, mined, sign};
use super::t12_claim_capacity::{LicencePolicy, drive, expand, sim};
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::palw_capacity_formulas_v1::{PALW_CAPACITY_FCW_V1, PALW_CAPACITY_REFERENCE_STEPS_V1, PalwCapacityStepV1};
use kaspa_consensus_core::palw_capacity_shadow_v1::{
    PalwCapacityAdversaryV1, PalwCapacityShadowOptionsV1, PalwCapacityShadowV1, PalwCapacityStrategyV1,
};
use kaspa_consensus_core::palw_state_v2::{
    PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwVoidReasonV2,
    palw_da_accusation_message_v2,
};
use kaspa_hashes::Hash64;

/// v1's reference ramp (indices 0–4, q 143‰), the identity step (5: ρ 1, q 0 — today's prices under
/// the new structure), testnet-12's armed first step (6: ρ 10, q 0 — lane liab's
/// `PALW_T12_CAPACITY_STEPS_V1`, what F-L would actually give on arming), and two credited steps (7:
/// ρ 10 at q_seat, 250‰; 8: ρ 10 at 500‰, the most D-8 can credit).
fn steps_with_identity() -> Vec<PalwCapacityStepV1> {
    let mut steps = PALW_CAPACITY_REFERENCE_STEPS_V1.to_vec();
    steps.push(PalwCapacityStepV1 { from_daa: 0, rho: 1, q_credit_permille: 0 });
    steps.push(PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 0 });
    steps.push(PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 250 });
    steps.push(PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 500 });
    steps
}

/// E-T3's golden as ADR-0160 v1 priced it (the SUPERSEDED column): a 13,000 MSK bond with no other
/// commitment held 20 / 50 / 101 / 203 / 2,030 floor claims at ρ 10 / 25 / 50 / 100 / 1000 with q
/// credited at 143‰. As the fold prices those steps (143‰ < q_seat), it holds 2.
const E_T3_13K_V1_SUPERSEDED: [u64; 5] = [20, 50, 101, 203, 2_030];

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
    // The default display is the uncredited ramp (no F-L schedule on this build): m_c = E, so a
    // fresh 13k bond still holds two floor claims at every ρ, and the log line says q = 0.
    assert_eq!(
        shadow.steps.iter().map(|s| (s.step.rho, s.step.q_credit_permille, s.n_instant_13k)).collect::<Vec<_>>(),
        vec![(10, 0, 2), (25, 0, 2), (50, 0, 2), (100, 0, 2), (1000, 0, 2)]
    );
    assert!(shadow.steps.iter().all(|s| !s.seat_credit && !s.q_alarm));
    assert_eq!(shadow.carriers_per_block, 3, "three coverage carriers per 500,000-mass block");
    let card = &shadow.bonds[0];
    assert!(card.seat && card.w_cap == 144 * PALW_CAPACITY_FCW_V1, "a card is a seat with W_cap 144 FCW");
    assert!(shadow.summary().starts_with("capacity-shadow: daa="));
    assert!(shadow.summary().contains("N13k[ρ@q‰]=10@0:2,"), "{}", shadow.summary());
    // Named, v1's reference ramp (q 143‰) prices as the fold would: below q_seat nothing is
    // credited, a 13k bond still holds 2, and E-T3 is only the superseded column's; the chain's
    // attributable classes are credited with nothing measured, so every step alarms.
    let reference = PalwCapacityShadowOptionsV1 { steps: PALW_CAPACITY_REFERENCE_STEPS_V1.to_vec(), ..options.clone() };
    let conditional = chain.ctx.consensus.palw_capacity_shadow_v1(reference).expect("a ConsensusV2 node answers");
    assert_eq!(conditional.steps.iter().map(|s| s.n_instant_13k).collect::<Vec<_>>(), vec![2; 5]);
    assert_eq!(conditional.steps.iter().map(|s| s.n_instant_13k_v1_superseded).collect::<Vec<_>>(), E_T3_13K_V1_SUPERSEDED.to_vec());
    assert!(
        conditional.steps.iter().all(|s| !s.seat_credit && s.seat_credit_if_d5),
        "143‰: lane liab's locks stay, D-5's would divide"
    );
    assert!(!conditional.credited_classes.is_empty(), "testnet-12's floor is an attributable class");
    assert!(conditional.steps.iter().all(|s| s.q_alarm && s.q_alarm_unmeasured), "a credit nobody measured alarms");
    // The same answer twice: the read is a function of the committed tip.
    assert_eq!(chain.ctx.consensus.palw_capacity_shadow_v1(options), Some(shadow));
}

/// The shadow of `m`'s tip as a node reads it, measuring q on `adversary` (an O-3 run's bonds, naive:
/// roots no seat can replay).
fn shadow_of(m: &Minting, adversary: &[PalwBondKeyV2]) -> PalwCapacityShadowV1 {
    let options = PalwCapacityShadowOptionsV1 {
        adversaries: adversary
            .iter()
            .map(|bond| PalwCapacityAdversaryV1 { bond: *bond, strategy: PalwCapacityStrategyV1::Naive })
            .collect(),
        block_mass_limit: m.chain.config.params.max_block_mass,
        ..Default::default()
    };
    m.chain.ctx.consensus.palw_capacity_shadow_v1(options).expect("a ConsensusV2 node answers")
}

/// **A8 on a real testnet-12 chain** (ADR-0160 §8 A8, the drill the ADR assigns to S-T5 and gate
/// G2: "the alarm fires in the drill when auditors are stopped"; review of lane shadow, finding 2).
///
/// Cards 0 and 7 are the O-3 run's bonds (naive). Their claims A and B are licensed and reach `Final`
/// with no auditor looking: both resolved, neither caught — measured q = 0 < 500‰ at every step, so
/// every step alarms and the line ends `q-ALARM`. A read that names no bond (a node without
/// `--palw-capacity-shadow-adversary`) measures nothing and stays quiet on the uncredited display.
/// Then the auditors run: card 1 accuses B's row, nobody answers, the chain convicts card 7
/// (`DaDefault`, B voided `ProducerWithholding`). **rcore/cap-s1, the user's decision 1: S1 is
/// TIER-class**, so the default is a detection the credit cannot rely on (a first default collects the
/// commitment alone): B reads `caught_unpriced` (route "da-default"), the measured q stays 0 and the
/// alarm stays on — the credit's evidence must come from a whole-bond route (a proven verdict), or from
/// v3's audit door (stage 2). Card 7 reads frozen by a TIER freeze (lifted at since + window_court). The
/// covering signers' kind-3 records name B too: they are seat-only, counted beside.
#[tokio::test]
async fn s_t5_a8_the_alarm_fires_when_the_auditors_stop_and_clears_when_they_run() {
    let mut m = mined().await;
    let a = m.step(Kind::Attempt(0)).await.claim.expect("card 0's attempt makes claim A");
    let b = m.attempt_at_the_anchor_slot(a, 7).await;
    m.license(a, 0).await;
    let _c = m.attempt_at_the_anchor_slot(b, 6).await;
    m.license(b, 7).await;
    let is_final = |s: &PalwChainStateV2, id: &Hash64| matches!(s.claim(id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Final { .. }));
    m.beat_until(4 * m.sp().window_challenge_at(0) + 50, "A and B are Final", |s| is_final(s, &a) && is_final(s, &b)).await;
    let o3 = [m.chain.bonds[0], m.chain.bonds[7]];
    let floor = m.sp().base_class_id();
    let adversary_of = |shadow: &PalwCapacityShadowV1| {
        let row = shadow.adversary_row(&floor, PalwCapacityStrategyV1::Naive).expect("the floor's naive row");
        assert_eq!((row.caught_late, row.censored), (0, 0), "{row:?}");
        (row.claims, row.caught, row.undetected, row.in_flight, row.q_measured_permille)
    };

    // The auditors are stopped: both claims resolved Final, nobody convicted.
    let stopped = shadow_of(&m, &o3);
    assert_eq!(adversary_of(&stopped), (2, 0, 2, 0, Some(0)), "two resolved, none caught");
    assert!(stopped.steps.iter().all(|s| s.q_alarm && s.q_required_permille == 500), "q 0 < 500‰ at every step");
    assert!(stopped.summary().ends_with("q-ALARM"), "{}", stopped.summary());
    eprintln!("[A8] auditors stopped, sink DAA {}: {}", m.sink_daa(), stopped.summary());
    let unnamed = shadow_of(&m, &[]);
    assert!(unnamed.adversary.is_empty());
    assert!(unnamed.steps.iter().all(|s| !s.q_alarm), "a node that names no O-3 bond measures nothing");

    // The auditors run: card 1 accuses B's row; nobody answers.
    let accuser = m.chain.bonds[1];
    let message = palw_da_accusation_message_v2(m.domain, &b, 0, &accuser);
    let accusation = PalwConsensusObjectV2::DefaultAccused {
        claim: b,
        missing_event_index: 0,
        accuser,
        signature: sign(1, message.as_byte_slice(), PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT),
    };
    let carrier = m.carrier(1, accusation, None);
    m.step(Kind::Heartbeat(vec![carrier])).await;
    let opened = m.step(beat()).await;
    let deadline =
        opened.child.da_sessions_of(&b).find(|(who, _)| **who == accuser).expect("the accusation opens a session").1.deadline_daa;
    let convicted = |s: &PalwChainStateV2| {
        matches!(s.claim(&b).map(|c| &c.phase), Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }))
    };
    m.beat_until(2 * (deadline - m.sink_daa()) + 20, "B's default", convicted).await;

    let running = shadow_of(&m, &o3);
    // B detected by a DA default — unpriced since decision 1; A undetected while the state still holds
    // it (a Final claim retires in time, and a retired claim leaves the denominator).
    let (claims, caught, undetected, in_flight, q) = adversary_of(&running);
    assert_eq!((caught, in_flight), (0, 0), "B is not a priced catch (decision 1)");
    let naive = running.adversary_row(&floor, PalwCapacityStrategyV1::Naive).unwrap();
    assert_eq!((naive.caught_unpriced, naive.unpriced_by_route.clone()), (1, vec![("da-default", 1)]), "{naive:?}");
    assert!(
        (claims, undetected, q) == (2, 1, Some(0)) || (claims, undetected, q) == (1, 0, Some(0)),
        "A undetected or retired: {:?}",
        adversary_of(&running)
    );
    assert!(running.steps.iter().all(|s| s.q_alarm), "a DA default backs no credit: the alarm stays: {}", running.summary());
    let producer = running.bonds.iter().find(|row| row.bond == m.chain.bonds[7]).expect("card 7's row");
    let (_, tip) = m.chain.tip_state();
    let records: Vec<_> = tip.consumed_offences_iter().map(|(_, o)| (o.kind, o.accused, o.claim_id == b, o.accepted_daa)).collect();
    // A covering signer's kind-3 record on B convicts a seat, not the producer: counted beside q.
    let covering = records
        .iter()
        .filter(|(kind, _, on_b, _)| *kind == kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2 && *on_b)
        .count();
    let row = running.adversary_row(&floor, PalwCapacityStrategyV1::Naive).unwrap();
    assert_eq!(row.seat_only, u64::from(covering > 0), "B is counted seat-only once if any covering signer was charged: {row:?}");
    // The default writes its DaDefault record on card 7 and a kind-3 record on each covering signer
    // of B; only the first charges the producer.
    assert!(
        producer.frozen_would_be && !producer.freeze_final && producer.convictions == 1,
        "a DA default is TIER-class (decision 1): frozen until the lift, by its own record — {producer:?}; records {records:?}"
    );
    for (kind, accused, on_b, _) in &records {
        if *kind == kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2 && *on_b {
            let seat = running.bonds.iter().find(|row| row.bond.0 == *accused).expect("a covering signer's row");
            assert!(seat.frozen_would_be && seat.freeze_undetermined, "covering signer {accused:?}: held for window_court");
        }
    }
    eprintln!("[A8] conviction records on the chain: {records:?}");
    // Card 0 produced A and is charged by nothing of its own — unless the draw seated it on B's panel, where
    // it is a covering signer like any other (held above). Which cards B's panel drew is the chain's, not ours.
    let card0 = m.chain.bonds[0];
    let card0_covers_b = records.iter().any(|(kind, accused, on_b, _)| {
        *kind == kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2 && *on_b && *accused == card0.0
    });
    let card0_row = running.bonds.iter().find(|row| row.bond == card0).unwrap();
    assert_eq!(card0_row.frozen_would_be, card0_covers_b, "card 0 is held only as a covering signer of B: {card0_row:?}");
    eprintln!("[A8] auditors running, sink DAA {}: {}", m.sink_daa(), running.summary());
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
    // E-T3 on the live chain as the fold prices it: the subject is a pure producer, so its
    // N_instant is a fresh 13k bond's — 2 at v1's 143‰ credit (below q_seat), E-T3 only in the
    // superseded column; 4 at 250‰ and 10 at 500‰ (lane escrow's m* on the Tier(1,300) floor).
    assert_eq!(row.n_instant_today, 2, "rel {rel}: today a 13k bond holds two floor claims");
    assert_eq!(row.n_instant_new[..5], [2; 5], "rel {rel}: 143‰ credits nothing");
    assert_eq!(
        shadow.steps[..5].iter().map(|s| s.n_instant_13k_v1_superseded).collect::<Vec<_>>(),
        E_T3_13K_V1_SUPERSEDED.to_vec(),
        "rel {rel}: v1's E-T3, superseded"
    );
    assert_eq!((row.n_instant_new[7], row.n_instant_new[8]), (4, 10), "rel {rel}: the fold's credited counts");
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
    // testnet-12's armed first step credits no attribution: m_c = E, so a 13k bond still holds two.
    let armed = shadow.steps.iter().position(|s| s.step.rho == 10 && s.step.q_credit_permille == 0).expect("the armed first step");
    assert_eq!((row.n_instant_new[armed], shadow.steps[armed].n_instant_13k), (2, 2), "rel {rel}: ρ 10 at q 0 holds two");
    assert!(!shadow.steps[armed].seat_credit, "rel {rel}: no lock credit at q 0");
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
             claims today/new(ρ=10@143) {}/{} sompi, new(ρ=10@0, armed) {} sompi; seats {} duty rows {} (capped {}) locks {} sompi; \
             seatcap/DAA ×1000 today {} ρ10@143 {} (if D-5 {}) ρ10@0 {} ρ10@250 {} ρ10@500 {}",
            row.live_claims,
            row.unlicensed_claims,
            row.raw_immature_today / PALW_CAPACITY_FCW_V1,
            row.capped / PALW_CAPACITY_FCW_V1,
            row.committed_today,
            row.committed_new[5],
            shadow.claims_commitment_today,
            shadow.steps[0].claims_commitment_total,
            shadow.steps[6].claims_commitment_total,
            shadow.seats,
            shadow.duty_rows,
            shadow.duty_rows_capped,
            shadow.seat_lock_total_today,
            shadow.seat_capacity_today_milli_per_daa,
            shadow.steps[0].seat_capacity_milli_per_daa,
            shadow.steps[0].seat_capacity_if_d5_milli_per_daa,
            shadow.steps[6].seat_capacity_milli_per_daa,
            shadow.steps[7].seat_capacity_milli_per_daa,
            shadow.steps[8].seat_capacity_milli_per_daa,
        );
        check_snapshot(&shadow, &subject, rel);
    }
    eprintln!("[S-T5] {daa_len} DAA: every snapshot passed; the slowest read took {max_ms} ms");
}
