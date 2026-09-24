//! **ADR-0152 v3.1 §3.11 (M3): the data-availability court, redesigned — on testnet-12's own fold.**
//!
//! Every block goes through `apply_palw_transition_v7` with the extras the processor resolves and is
//! checked three ways ([`Chain::step`]): its delta re-applies to the child and reverts to the parent,
//! and the child's carriage reloads under its committed root (`into_state`: the ledger, R-core+'s
//! load invariants, the DA records against the claims, DL-1's deadlines exactly). Each test runs
//! beside its fence-off twin (testnet-12 with `palw_rcore_plus = None`), where the v1 court stands.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_m3_da_court

#[path = "rcore_common.rs"]
mod common;
use common::*;
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaClaimV1, PalwDaSessionV1, PalwDaStageV1, PalwDaUnitV1};
use kaspa_consensus_core::palw_state_v2::{
    PalwStateV2Error, palw_accuser_exposure_v1, palw_bond_committed_v1, palw_claim_bond_reservation_v1,
    palw_da_event_index_v1,
};

/// 1 MSK in sompi.
const MSK: u64 = 100_000_000;

/// An event accusation of `(row, 0)`: the fold never reads the signature (the acceptance layer's).
fn accuse(claim: Hash64, accuser: PalwBondKeyV2, row: u32) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index: palw_da_event_index_v1(row, 0), accuser, signature: vec![] }
}

/// The next block at `c.daa + 1` folded with `objects`, without committing it — for refusals.
fn try_step(c: &Chain, objects: &[PalwConsensusObjectV2]) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let daa = c.daa + 1;
    let x = ctx(0xCA_0000 + daa, daa, daa, 0);
    let folded = if c.room {
        go(&c.p, &c.sp, &c.s, &x, objects, PalwBlockWorkV3::None, Hash64::default())
    } else {
        fold(&c.p, &c.sp, &c.s, &x, objects, PalwBlockWorkV3::None, Hash64::default())
    };
    folded.map(|(child, _, _)| child)
}

/// A floor attempt by bond `n` (`rcore_s3`'s): its own artifact root, its own execution key.
fn floor_attempt(c: &Chain, n: u64, seed: u64) -> (kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2, Hash64, Hash64) {
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let pwu = c.floor_pwu(c.daa + 1);
    let (mut env, _, _) = junk_attempt(floor, bond_key(n), pubkey_of(n), &operator_pubkey_of(n), pwu, seed, 0x10C0 + seed);
    env.attempt.artifact_root = c.s.class(&floor).expect("the floor").artifact_root;
    let anchor = kaspa_consensus_core::palw_attempt_v2::execution_anchor_v3(h(NET), h(0x10C0 + seed), floor, &bond_key(n).0, 7);
    let key = kaspa_consensus_core::palw_attempt_v2::execution_commitment_v3(&env.attempt, anchor);
    let id = kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&env.attempt);
    (env, key, id)
}

/// A floor claim, bound to the five genesis seats after its producer.
fn bound_floor_claim(c: &mut Chain, seed: u64) -> (Hash64, Vec<(PalwBondKeyV2, Hash64)>, u64) {
    let id = c.floor_claim(seed);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    (id, seats, bound)
}

/// **M3 Phase 1 (DA-2, §6 rows 14–15): the two DA maps are rooted, carried and reloaded, and the
/// accuser ledger reads them.** A session and its claim's record written through the carriage (the
/// load path) reload under their own root; the root moves with them; the A-6 ledger counts the open
/// session's exposure and the refuted exposure held; a record that miscounts its sessions is refused
/// at load. The fence-off twin refuses the same carriage: no DA record exists below the fence.
#[test]
fn m3_p1_the_da_maps_are_rooted_carried_reloaded_and_counted() {
    for armed in [true, false] {
        let p = if armed { t12() } else { twin(&t12()) };
        let mut c = Chain::new(p);
        let (id, seats, bound) = bound_floor_claim(&mut c, 0x31);
        let (seat, _) = seats[0];
        let (bystander, _, _) = genesis_bonds(&c.p)[7];
        let session = PalwDaSessionV1 {
            opened_daa: bound,
            deadline_daa: bound + c.sp.window_challenge(),
            accuser_is_seat: true,
            exposure: 320,
            units: vec![PalwDaUnitV1::Event { row: 0, tile: 0 }],
            stage: PalwDaStageV1::Live,
        };
        let record = PalwDaClaimV1 {
            open_seat_sessions: 1,
            paused_since: Some(bound),
            opened_by_seat: [(seat, 1)].into_iter().collect(),
            refuted_held: vec![(bystander, 77)],
            opened_non_seat_total: 1,
            ..Default::default()
        };
        let write = |carriage: &mut PalwStateCarriageV2| {
            carriage.da_sessions.insert((id, seat), session.clone());
            carriage.da_claims.insert(id, record.clone());
        };
        let mut carriage = PalwStateCarriageV2::from_state(&c.s);
        write(&mut carriage);
        let bytes = borsh::to_vec(&carriage).unwrap();
        assert_eq!(borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap(), carriage, "armed={armed}: the tail carries them");
        let loaded = carriage.clone().into_state(&c.sp, None);
        if !armed {
            assert!(
                matches!(&loaded, Err(PalwStateV2Error::CarriageInconsistent(why)) if why.contains("dormant")),
                "fence off: a DA record is refused at load ({loaded:?})"
            );
            continue;
        }
        // DL-1 reads the record: a seat session pauses the claim, so the loader rebuilds no deadline.
        let loaded = loaded.expect("the carriage reloads");
        assert_eq!(loaded.deadline_of(&id), None, "a seat session pauses the claim (DA-5, DL-1)");
        assert_ne!(loaded.state_root(), c.s.state_root(), "the DA maps are rooted");
        assert_eq!(loaded.da_session(&id, &seat), Some(&session));
        assert_eq!(loaded.da_claim(&id), Some(&record));
        let root = loaded.state_root();
        let again = PalwStateCarriageV2::from_state(&loaded).into_state(&c.sp, Some(root)).expect("reloads under its root");
        assert_eq!(again, loaded);
        // A-6: the open session and the refuted exposure held, each on its own accuser.
        assert_eq!(palw_accuser_exposure_v1(&loaded, &seat), 320, "the seat's open session");
        assert_eq!(palw_accuser_exposure_v1(&loaded, &bystander), 77, "the bystander's refuted exposure held");
        assert_eq!(loaded.da_deadlines_iter().copied().collect::<Vec<_>>(), vec![(session.deadline_daa, id, seat)]);
        // A record whose counts are not its sessions' is refused.
        let mut drifted = carriage.clone();
        drifted.da_claims.get_mut(&id).unwrap().open_other_sessions = 1;
        assert!(matches!(drifted.into_state(&c.sp, None), Err(PalwStateV2Error::CarriageInconsistent(_))), "miscounted sessions");
        // A session whose claim the state does not hold is refused.
        let mut orphan = carriage.clone();
        orphan.da_sessions.insert((h(0x0DEAD), seat), session.clone());
        assert!(matches!(orphan.into_state(&c.sp, None), Err(PalwStateV2Error::CarriageInconsistent(_))), "an orphan session");
    }
}

/// **T65 (core half, DA-8): no session monopoly.** A non-seat session at the licence blocks no seat;
/// seats and non-seats each open their own session, one per accuser; at most three non-seat sessions
/// are open at once; the producer cannot accuse its own claim. With the lifetime budgets exhausted —
/// sixteen non-seat sessions, or four by one seat — the non-seat and that seat are refused and every
/// other seat still accuses (the seat budget is its own). The fence-off twin runs ADR-0062's court:
/// one session per claim, the claim's phase taken.
#[test]
fn t65_da8_seats_and_non_seats_each_accuse_and_nobody_monopolizes() {
    let mut c = Chain::new(t12());
    c.step(&(1..=5).map(|n| bond_obj(n, 20_000 * MSK)).collect::<Vec<_>>());
    let (id, seats, bound) = bound_floor_claim(&mut c, 0x65);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect() }]);
    // A non-seat's session at L+1 blocks no seat (reverses probe adr0152v2_r1).
    c.step(&[accuse(id, bond_key(1), 3)]);
    c.step(&[accuse(id, seats[0].0, 3)]);
    c.step(&[accuse(id, seats[1].0, 4)]);
    assert_eq!(c.s.da_sessions_of(&id).count(), 3, "three sessions, three accusers");
    assert!(
        matches!(try_step(&c, &[accuse(id, seats[0].0, 5)]), Err(PalwStateV2Error::DaSessionAlreadyOpen { .. })),
        "one open session per accuser per claim"
    );
    c.step(&[accuse(id, bond_key(2), 0)]);
    c.step(&[accuse(id, bond_key(3), 0)]);
    assert!(
        matches!(try_step(&c, &[accuse(id, bond_key(4), 0)]), Err(PalwStateV2Error::DaSessionBudgetExhausted { .. })),
        "a fourth open non-seat session is refused"
    );
    let (producer, _, _) = floor_producer(&c.p);
    assert!(matches!(try_step(&c, &[accuse(id, producer, 0)]), Err(PalwStateV2Error::DaAccuserIsTheProducer(_))));
    let record = c.s.da_claim(&id).expect("the claim's DA record").clone();
    assert_eq!((record.open_seat_sessions, record.open_other_sessions, record.opened_non_seat_total), (2, 3, 3));
    assert_eq!(record.opened_by_seat.values().copied().collect::<Vec<_>>(), vec![1, 1]);
    // The lifetime budgets, exhausted through the carriage (the load path).
    let exhausted = edited(&c.sp, &c.s, |carriage| {
        let r = carriage.da_claims.get_mut(&id).unwrap();
        r.opened_non_seat_total = 16;
        r.opened_by_seat.insert(seats[2].0, 4);
    });
    let mut d = Chain { s: exhausted, ..Chain::new(c.p.clone()) };
    d.daa = c.daa;
    // Close one non-seat session so the open cap is not what refuses: re-derive with two open.
    d.s = edited(&d.sp, &d.s, |carriage| {
        carriage.da_sessions.remove(&(id, bond_key(3)));
        carriage.da_claims.get_mut(&id).unwrap().open_other_sessions = 2;
    });
    assert!(
        matches!(try_step(&d, &[accuse(id, bond_key(5), 0)]), Err(PalwStateV2Error::DaSessionBudgetExhausted { why, .. }) if why.contains("sixteen")),
        "the non-seat lifetime cap"
    );
    assert!(
        matches!(try_step(&d, &[accuse(id, seats[2].0, 0)]), Err(PalwStateV2Error::DaSessionBudgetExhausted { why, .. }) if why.contains("four")),
        "a seat's fifth session"
    );
    d.step(&[accuse(id, seats[3].0, 0)]);
    assert!(d.s.da_session(&id, &seats[3].0).is_some(), "after sixteen non-seat sessions a seat still accuses (DA-8)");

    // Fence off: ADR-0062 — one session per claim, and it takes the claim's phase.
    let mut t = Chain::new(twin(&t12()));
    t.step(&[bond_obj(1, 20_000 * MSK)]);
    let (tid, tseats, tbound) = bound_floor_claim(&mut t, 0x65);
    t.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: tid, receipts: tseats.iter().map(|(k, _)| valid(tid, *k, tbound)).collect() }]);
    t.step(&[accuse(tid, bond_key(1), 3)]);
    assert!(matches!(t.claim(&tid).phase, PalwClaimPhaseV2::DefaultDisputed { .. }), "fence off: the v1 session takes the phase");
    assert!(t.s.da_claim(&tid).is_none(), "and writes no side map");
    assert!(matches!(try_step(&t, &[accuse(tid, tseats[0].0, 3)]), Err(PalwStateV2Error::DaAccusationAlreadyOpen(_))));
}

/// **T64 (core half, DA-3): the drawn units lie inside the committed run.** An attempt's run past
/// `palw_rcore_plus` is one decode row (`palw_attempt_job_v1` with the prefill draw armed pins
/// `exact_decode_tokens = 1`), so an event accusation of any row draws `(0, 0)` — the one other
/// in-run unit — and naming `(0, 0)` draws nothing. A row past the accusable bound
/// (`trace_chunk_count × 256`) is refused. An event accusation of a held-context class's attempt
/// (the 8k row) draws nothing: its held bound is in a binding the event object does not carry.
#[test]
fn t64_da3_an_event_accusation_draws_inside_the_one_row_run() {
    use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
    let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor's profile");
    let canonical = rc_job_context(&profile, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    assert_eq!(canonical.exact_decode_tokens, PALW_RC_BASE0_CANONICAL.1, "the floor's canonical job decodes four");
    assert_eq!(
        kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(canonical, true).exact_decode_tokens,
        1,
        "the attempt run is one decode row once the prefill draw is armed (testnet-12: every fence at 0)"
    );
    assert!(t12().palw_prefill_draw.is_some_and(|f| f.is_active(0)), "testnet-12 arms the prefill draw from genesis");
    let mut c = Chain::new(t12());
    let (id, seats, _) = bound_floor_claim(&mut c, 0x64);
    c.step(&[accuse(id, seats[0].0, 7)]);
    assert_eq!(
        c.s.da_session(&id, &seats[0].0).unwrap().units,
        vec![PalwDaUnitV1::Event { row: 7, tile: 0 }, PalwDaUnitV1::Event { row: 0, tile: 0 }],
        "the named row, then the run's one row"
    );
    c.step(&[accuse(id, seats[1].0, 0)]);
    assert_eq!(c.s.da_session(&id, &seats[1].0).unwrap().units, vec![PalwDaUnitV1::Event { row: 0, tile: 0 }], "nothing else in the run");
    assert!(
        matches!(try_step(&c, &[accuse(id, seats[2].0, 256)]), Err(PalwStateV2Error::DaIndexOutOfRange { count: 256, .. })),
        "a row past trace_chunk_count x 256 is no unit"
    );

    let p = t12();
    let (short, _) = model_classes(&p);
    let mut m = model_chain(p, short, 1);
    let mid = model_claim(&mut m, short, 1, 0x64);
    let mseats = honest_seats(&m.p, 5);
    m.s = readied(&m.sp, &m.s, &honest(&m.p), short, m.daa);
    m.bind(mid, &mseats);
    m.step(&[accuse(mid, mseats[0].0, 3)]);
    assert_eq!(
        m.s.da_session(&mid, &mseats[0].0).unwrap().units,
        vec![PalwDaUnitV1::Event { row: 3, tile: 0 }],
        "a held-context class's attempt draws no event unit"
    );
}

/// **T67 (core half, DA-5, V3S-08): only a seat session pauses a pre-`Final` claim.** A non-seat
/// session leaves the receipt deadline where it was; a seat's first session removes it (DL-1), a
/// licence carried during the pause arms nothing, and the claim stays licensed past `L + 1,200`
/// while the seat session is open. Every block reloads with DL-1's deadlines exactly
/// (`Chain::step`).
#[test]
fn t67_da5_only_a_seat_session_pauses_the_claim() {
    let mut c = Chain::new(t12());
    c.step(&[bond_obj(1, 20_000 * MSK)]);
    let (id, seats, bound) = bound_floor_claim(&mut c, 0x67);
    let receipt_deadline = c.s.deadline_of(&id).expect("a bound claim owes its receipt deadline");
    c.step(&[accuse(id, bond_key(1), 3)]);
    assert_eq!(c.s.deadline_of(&id), Some(receipt_deadline), "a non-seat session pauses nothing");
    assert_eq!(c.s.da_claim(&id).unwrap().paused_since, None);
    c.step(&[accuse(id, seats[0].0, 3)]);
    let paused_at = c.daa;
    assert_eq!(c.s.deadline_of(&id), None, "a seat session pauses the claim");
    assert_eq!(c.s.da_claim(&id).unwrap().paused_since, Some(paused_at));
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect() }]);
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = c.claim(&id).phase else { panic!("licensed during the pause") };
    assert_eq!(c.s.deadline_of(&id), None, "a licence during the pause arms nothing");
    let past = licensed_daa + c.sp.window_challenge_at(licensed_daa) + 5;
    c.step_at(past, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "still licensed past L + 1,200 while paused");
}

/// **T69 (core half, DA-6): a session costs `min(⌈r · S_P(stage)⌉, min_collateral)`, on the
/// accuser's free half.** At `Live` and `Licensed` the base is the producer's whole commitment
/// (`w + esc + rr`), so the exposure is a tenth of it; the accuser ledger (A-6) rises by exactly that
/// and nothing enters the accuser's `reserved_exposure`.
#[test]
fn t69_da6_a_session_costs_r_times_the_stage_reward_base() {
    let mut c = Chain::new(t12());
    c.step(&[bond_obj(1, 20_000 * MSK), bond_obj(2, 20_000 * MSK)]);
    let (id, seats, bound) = bound_floor_claim(&mut c, 0x69);
    let claim = c.claim(&id);
    let full = palw_claim_bond_reservation_v1(&c.sp, &claim).expect("the reservation");
    let expected = (full * 1_000).div_ceil(10_000).min(u128::from(c.sp.min_collateral_sompi()));
    let reserved_before = c.reserved(&bond_key(1));
    c.step(&[accuse(id, bond_key(1), 3)]);
    let live = c.s.da_session(&id, &bond_key(1)).unwrap().clone();
    assert_eq!((live.stage, live.exposure), (PalwDaStageV1::Live, expected), "Live: r x (w + esc + rr)");
    assert_eq!(palw_accuser_exposure_v1(&c.s, &bond_key(1)), expected, "on the accuser ledger");
    assert_eq!(c.reserved(&bond_key(1)), reserved_before, "never in reserved_exposure (A-6)");
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect() }]);
    c.step(&[accuse(id, bond_key(2), 3)]);
    let licensed = c.s.da_session(&id, &bond_key(2)).unwrap().clone();
    assert_eq!((licensed.stage, licensed.exposure), (PalwDaStageV1::Licensed, expected), "Licensed: the same base (X7)");
    // The floor's numbers (ADR §3.11 DA-6's table): about 320.10 MSK at 13k.
    println!("floor Live/Licensed exposure: {} sompi ({} MSK)", expected, expected as f64 / MSK as f64);
}

/// **A-6 / the S-1…S-3 review's M1: a DA accusation is refused past the accuser's free half.** The
/// reviewer's scenario: a 65,000 MSK bond with two floor attempts of its own and a court open on a
/// licensed 2M claim (its stake, the 2M claim's `reserved`, on the accuser ledger) accuses a floor
/// claim — refused, where the v1 arm's `reserved + registration ≤ 500‰` never saw the court and took
/// the bond to 120.1% of its collateral. The free half itself: an identical bond with the same court
/// and NO work of its own is refused too (`max(committed, 500‰·C) + accuser + new > C`), though
/// `committed + accuser + new ≤ C` would have admitted it; an identical bond with no court accuses.
#[test]
fn m1_a_da_accusation_is_refused_past_the_accusers_free_half() {
    let p = t12();
    let (_, id2m) = model_classes(&p);
    let mut c = model_chain(p, id2m, 1);
    let two_m = model_claim(&mut c, id2m, 1, 0xA6);
    let seats = honest_seats(&c.p, 5);
    c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
    let bound2m = c.bind(two_m, &seats);
    c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: two_m, receipts: seats.iter().map(|(k, _)| valid(two_m, *k, bound2m)).collect() }]);
    let collateral = 65_000 * MSK;
    c.step(&[bond_obj(2, collateral), bond_obj(3, collateral), bond_obj(4, collateral)]);
    for seed in 0..2u64 {
        let (env, key, _) = floor_attempt(&c, 2, 0xA610 + seed);
        c.step_at(c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    }
    let (floor_id, _, _) = bound_floor_claim(&mut c, 0xA6);
    // Two courts on the 2M claim, bonds 2 and 3 challenging, through the carriage (the load path):
    // each stake is the session, read by A-6.
    let claim2m = c.claim(&two_m);
    let at = c.daa;
    let window = c.sp.window_court();
    c.s = edited(&c.sp, &c.s, |carriage| {
        for challenger in [bond_key(2), bond_key(3)] {
            let ladder = kaspa_consensus_core::palw_bisect::PalwBisectLadderV1::open(
                &two_m,
                &claim2m.trace_root,
                &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&challenger),
                &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&claim2m.bond),
                kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
                16,
                at,
                at + 50,
            )
            .expect("a ladder opens");
            carriage.court_sessions.insert(
                ladder.session_id(),
                kaspa_consensus_core::palw_state_v2::PalwCourtSessionStateV2 {
                    claim: two_m,
                    challenger_bond: challenger,
                    opened_daa: at,
                    deadline_daa: at + window,
                    ladder,
                    dissection: None,
                },
            );
        }
    });
    let court = palw_accuser_exposure_v1(&c.s, &bond_key(2));
    assert_eq!(court, claim2m.reserved, "the 2M court's stake is on the accuser ledger");
    assert_eq!(palw_accuser_exposure_v1(&c.s, &bond_key(3)), court);
    let committed = palw_bond_committed_v1(&c.s, &bond_key(2), c.daa + 1, None, c.sp.window_court());
    let half = u128::from(collateral) / 2;
    assert!(committed < half && committed > 0, "the premise: two own attempts, under the work ceiling");
    let floor_claim = c.claim(&floor_id);
    let exposure = (palw_claim_bond_reservation_v1(&c.sp, &floor_claim).unwrap() * 1_000)
        .div_ceil(10_000)
        .min(u128::from(c.sp.min_collateral_sompi()));
    println!(
        "M1: C {} MSK, committed {} MSK, 2M court {} MSK, DA {} MSK",
        collateral / MSK,
        committed / u128::from(MSK),
        court / u128::from(MSK),
        exposure as f64 / MSK as f64
    );
    let refused = |bond: u64| {
        let result = try_step(&c, &[accuse(floor_id, bond_key(bond), 3)]);
        assert!(
            matches!(result, Err(PalwStateV2Error::AccusationExposureCeiling { ceiling, .. }) if ceiling == u128::from(collateral)),
            "bond {bond}: the accusation is refused past its free half: {result:?}"
        );
    };
    // The reviewer's bond: past C on any reading.
    assert!(committed + court + exposure > u128::from(collateral), "the premise: past C");
    refused(2);
    // The free half: no work of its own, and `committed + accuser + new ≤ C` would have admitted it.
    assert!(court + exposure <= u128::from(collateral) && half + court + exposure > u128::from(collateral), "the premise");
    refused(3);
    // An identical bond with no court is not refused.
    c.step(&[accuse(floor_id, bond_key(4), 3)]);
    assert!(c.s.da_session(&floor_id, &bond_key(4)).is_some());
}

/// **DA-5 / SR-9: a redraw never closes a session, and a void releases every session.** A non-seat
/// session outlives the first receipt timeout's redraw (the claim goes `Provisional`, the session
/// stays); the second timeout voids the claim (S0′), which closes the session with its exposure
/// returned — nothing refuted, nothing held — and the record re-arms the retirement no earlier than
/// one DAA after the close; at retirement the record goes with the claim.
#[test]
fn da5_a_redraw_keeps_the_session_and_a_void_releases_it() {
    let mut c = Chain::new(t12());
    c.step(&[bond_obj(1, 20_000 * MSK)]);
    let (id, seats, _) = bound_floor_claim(&mut c, 0xD5);
    let deadline = c.s.deadline_of(&id).expect("the receipt deadline");
    // Accused on the receipt window's last DAA, so the session outlives both panels.
    c.step_at(deadline, &[accuse(id, bond_key(1), 3)], PalwBlockWorkV3::None, Hash64::default(), 0);
    let exposure = palw_accuser_exposure_v1(&c.s, &bond_key(1));
    assert!(exposure > 0);
    c.step_at(deadline + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::Provisional), "the first timeout redraws");
    assert!(c.s.da_session(&id, &bond_key(1)).is_some(), "a redraw never closes a session");
    let rebound = c.bind(id, &seats);
    let _ = rebound;
    let deadline = c.s.deadline_of(&id).expect("the second receipt deadline");
    c.step_at(deadline + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let PalwClaimPhaseV2::Voided { voided_daa, .. } = c.claim(&id).phase else { panic!("the second timeout voids") };
    assert!(c.s.da_session(&id, &bond_key(1)).is_none(), "the void closes the session");
    let record = c.s.da_claim(&id).expect("the record stays until retirement").clone();
    assert_eq!((record.open_sessions(), record.last_closed_daa, record.refuted_held.len()), (0, Some(voided_daa), 0));
    assert_eq!(palw_accuser_exposure_v1(&c.s, &bond_key(1)), 0, "the exposure is returned");
    let retire = c.s.deadline_of(&id).expect("the retirement is armed");
    assert_eq!(retire, (voided_daa + c.sp.claim_retirement_daa()).max(voided_daa + 1), "DL-1's terminal row");
    c.step_at(retire + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(c.s.claim(&id).is_none() && c.s.da_claim(&id).is_none(), "the record retires with the claim");
}
