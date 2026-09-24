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
use kaspa_consensus_core::palw_offence_attribution_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V2, PalwFalseValidReceiptV1, PalwIdentityRulesV1, PalwPanelFalseValidEvidenceV2,
    palw_check_panel_false_valid_v2,
};
use kaspa_consensus_core::palw_offence_v1::{PalwOffenceKindV1, PalwPanelContradictionV1, palw_offence_evidence_digest_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwStateV2Error, PalwVoidReasonV2, palw_accuser_exposure_v1, palw_bond_committed_v1, palw_claim_bond_reservation_v1,
    palw_da_event_index_v1, palw_da_signer_liability_armed_v1,
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
    // ADR-0152 §4-quater (U-D1): the 2M row is closed at launch, so a live 2M claim exists only past the flag day
    // that installs its measured row — this test's premise runs there (`t12_2m_open`, measuring the derived
    // 13,995-DAA deadline).
    let p = t12_2m_open();
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
    // P2-6: the node's read (C-8) refuses exactly what the fold refuses, so a seat whose free half is
    // full never pays a carrier for an accusation the fold would drop — and waits for room.
    let read = |bond: u64| {
        let at = c.daa + 1;
        kaspa_consensus_core::palw_producer_v2::palw_da_accusation_check_v1(
            &c.s,
            &c.sp,
            &c.extras_at(at),
            &floor_id,
            &bond_key(bond),
            at,
        )
    };
    // The reviewer's bond: past C on any reading.
    assert!(committed + court + exposure > u128::from(collateral), "the premise: past C");
    refused(2);
    // The free half: no work of its own, and `committed + accuser + new ≤ C` would have admitted it.
    assert!(court + exposure <= u128::from(collateral) && half + court + exposure > u128::from(collateral), "the premise");
    refused(3);
    for bond in [2, 3] {
        assert!(
            matches!(
                read(bond),
                kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1::Refused(PalwStateV2Error::AccusationExposureCeiling { ceiling, .. })
                    if ceiling == u128::from(collateral)
            ),
            "bond {bond}: the node's read refuses it for room: {:?}",
            read(bond)
        );
    }
    assert!(matches!(read(4), kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1::File { .. }), "bond 4 files");
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

/// A floor claim licensed through the coverage door (the full seat and every partial, each with its
/// assigned mask): the full seat's lock carries the full mask, every partial's its own segment.
fn covered_floor_claim(c: &mut Chain, seed: u64) -> (Hash64, Vec<(PalwBondKeyV2, Hash64)>, u64) {
    let id = c.floor_claim(seed);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    let receipts = covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the coverage licence folds");
    (id, seats, bound)
}

/// The seats of `claim` whose recorded lock mask is full.
fn full_mask_seats(c: &Chain, claim: &Hash64, seats: &[(PalwBondKeyV2, Hash64)]) -> Vec<PalwBondKeyV2> {
    seats
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| c.s.slashable_lock(*k, *claim).is_some_and(|lock| lock.segments > 0 && lock.attested.is_full(lock.segments)))
        .collect()
}

/// Empty blocks until the session `(claim, accuser)` is gone: its deadline's block, then the first
/// past it — the default — folded with the processor's extras (the shipped
/// `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1`: signer liability armed since P2-7 landed the answers).
fn run_out(c: &mut Chain, claim: Hash64, accuser: PalwBondKeyV2) -> u64 {
    run_out_with(c, claim, accuser, kaspa_consensus_core::palw_da_rcore_v1::PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1)
}

/// [`run_out`] with `seat_da_answer_landed` set to `landed` for the default's block — the test-only
/// override of P2-7's constant (the processor always passes the constant), so both sides of the
/// covering signers' S4 stay pinned: the shipped rule (`true`) and the dormant one it replaced. The block is checked as `Chain::step` checks one: the
/// delta re-applies and reverts, the carriage reloads under its root.
fn run_out_with(c: &mut Chain, claim: Hash64, accuser: PalwBondKeyV2, landed: bool) -> u64 {
    let deadline = c.s.da_session(&claim, &accuser).expect("an open session").deadline_daa;
    if deadline > c.daa + 1 {
        c.step_at(deadline, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    }
    let daa = deadline + 1;
    let x = ctx(0xCA_0000 + daa, daa, daa, 0);
    // The chain's own extras (`Chain::extras_at`: the processor's, the execution lane's round quantum
    // included, so `G`'s `R` is the one every other block of the chain priced).
    let mut e = c.extras_at(daa);
    assert_eq!(
        e.seat_da_answer_landed,
        kaspa_consensus_core::palw_da_rcore_v1::PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1,
        "the fixtures' extras carry the shipped value"
    );
    e.seat_da_answer_landed = landed;
    let parent = c.s.clone();
    let (child, delta, skips) =
        fold_with(&c.p, &c.sp, &parent, &x, &[], PalwBlockWorkV3::None, Hash64::default(), &e).expect("the default's block folds");
    assert!(skips.is_empty());
    assert_eq!(apply_delta_v2(&parent, &delta, &c.sp).expect("re-applies"), child, "the delta is the transition");
    assert_eq!(revert_delta_v2(&child, &delta, &c.sp).expect("reverts"), parent, "the delta reverts");
    let reloaded = PalwStateCarriageV2::from_state(&child).into_state(&c.sp, Some(child.state_root())).expect("reloads");
    assert_eq!(reloaded, child);
    c.s = child;
    c.daa = daa;
    assert!(c.s.da_session(&claim, &accuser).is_none(), "the session is gone the first block past its deadline");
    daa
}

/// **DA-7 at the `Live` stage (T18's first half, pre-licence S1).** A bystander's session on a bound
/// claim runs out: the claim voids `ProducerWithholding`, the producer forfeits its whole commitment
/// (`w + esc + rr`), no seat is charged (none signed), the accuser's exposure comes back, and a
/// `DaDefault` (kind 5) is recorded once under `(producer, claim)` with root 0, `claim_id` and the
/// producer's actual debit as `collected`. Fence off: ADR-0062's default (the same void), no record.
#[test]
fn t18_da7_a_live_default_is_s1_and_writes_one_da_default_record() {
    use kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1;
    use kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1;
    for armed in [true, false] {
        let p = if armed { t12() } else { twin(&t12()) };
        let mut c = Chain::new(p);
        c.step(&[bond_obj(1, 20_000 * MSK)]);
        let (id, seats, _) = bound_floor_claim(&mut c, 0x18);
        let claim = c.claim(&id);
        let (producer, _, _) = floor_producer(&c.p);
        let full = palw_claim_bond_reservation_v1(&c.sp, &claim).unwrap();
        let producer_before = c.s.bond(&producer).unwrap().collateral;
        let seats_before: Vec<u64> = seats.iter().map(|(k, _)| c.s.bond(k).unwrap().collateral).collect();
        let accuser_before = c.s.bond(&bond_key(1)).unwrap().collateral;
        c.step(&[accuse(id, bond_key(1), 0)]);
        let voided_at = if armed {
            run_out(&mut c, id, bond_key(1))
        } else {
            let deadline = c.daa + c.sp.window_challenge();
            c.step_at(deadline + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
            deadline + 1
        };
        assert!(
            matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, voided_daa } if voided_daa == voided_at),
            "armed={armed}: the default voids the claim for withholding"
        );
        let debit = producer_before - c.s.bond(&producer).unwrap().collateral;
        assert_eq!(u128::from(debit), full, "armed={armed}: S1 forfeits the whole commitment");
        assert_eq!(seats.iter().map(|(k, _)| c.s.bond(k).unwrap().collateral).collect::<Vec<_>>(), seats_before, "no seat pays");
        assert_eq!(c.s.bond(&bond_key(1)).unwrap().collateral, accuser_before, "the accuser pays nothing");
        let record = c.s.consumed_offence(&palw_da_offence_id_v1(&producer.0, &id));
        if armed {
            let record = record.expect("one DaDefault record");
            assert_eq!(
                (record.kind, record.accused, record.accepted_daa, record.execution_root, record.collected, record.claim_id),
                (PalwOffenceKindV1::DaDefault, producer.0, voided_at, Hash64::default(), debit, id),
                "kind 5, root 0 (by claim), the actual debit collected, the claim named"
            );
            assert_eq!(palw_accuser_exposure_v1(&c.s, &bond_key(1)), 0, "the confirmed session's exposure is returned");
            // R-4 (S-7, wired at integration): the default opens the reporter reward in its own block,
            // named — the accuser, no reveal (V3S-03) — at ⌊r × collected⌋ on the producer's debit
            // (X = 0: the producer's own tier).
            let key = palw_da_offence_id_v1(&producer.0, &id);
            let pending = *c.s.reward_pending(&key).expect("the DA default's reward is pending");
            assert_eq!(pending.amount, kaspa_consensus_core::palw_state_v2::palw_reporter_reward_amount_v1(debit, 0));
            assert!(pending.amount > 0 && pending.evidence_id == Hash64::default() && !pending.accepts_reveals());
            assert_eq!(pending.best.map(|winner| winner.reporter), Some(bond_key(1)), "the accuser, named");
        } else {
            assert!(record.is_none(), "fence off: no DaDefault record");
            assert!(c.s.reward_pending_iter().next().is_none(), "fence off: no reward");
        }
    }
}

/// **DA-7 at the `Licensed` stage and C7: S1, and S4 only on the signers whose mask covers an
/// unanswered unit.** A coverage licence (the full seat, four partials); a partial seat — a seat of
/// the current panel — accuses the run's one event row, which pauses the claim (so it cannot slip into
/// `Final`), and neither the producer nor any locked signer answers. The claim voids
/// `ProducerWithholding` and the producer forfeits its commitment either way. With signer liability
/// armed (`landed`: as shipped since P2-7), the FULL seat (the only full mask) loses its lock and
/// `min(25% · C, 3 G)` under the (seat, claim) key, and every partial seat — which attested only its
/// segment and could not have answered an event row — keeps its lock and collateral (C7). The
/// dormant twin (the pre-P2-7 value, a test-only override) charges no signer at all.
#[test]
fn t32_c7_a_licensed_default_charges_s1_and_s4_on_covering_signers_only() {
    assert!(
        kaspa_consensus_core::palw_da_rcore_v1::PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1,
        "P2-7 landed: the armed twin is the shipped rule"
    );
    for landed in [false, true] {
        t32_c7_body(landed);
    }
}

fn t32_c7_body(landed: bool) {
    use kaspa_consensus_core::palw_offence_attribution_v1::palw_false_valid_offence_id_v2;
    use kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1;
    let mut c = Chain::new(t12());
    let (id, seats, _) = covered_floor_claim(&mut c, 0x32);
    let full = full_mask_seats(&c, &id, &seats);
    assert_eq!(full.len(), 1, "one full seat in the coverage geometry");
    let partials: Vec<PalwBondKeyV2> = seats.iter().map(|(k, _)| *k).filter(|k| !full.contains(k)).collect();
    let accuser = partials[0];
    let (producer, _, _) = floor_producer(&c.p);
    let producer_before = c.s.bond(&producer).unwrap().collateral;
    let claim = c.claim(&id);
    let commitment = palw_claim_bond_reservation_v1(&c.sp, &claim).unwrap();
    let full_before = c.s.bond(&full[0]).unwrap().collateral;
    let full_lock = *c.s.slashable_lock(full[0], id).expect("the full seat's lock");
    let partial_before: Vec<(u64, Option<u128>)> =
        partials.iter().map(|k| (c.s.bond(k).unwrap().collateral, c.s.slashable_lock(*k, id).map(|l| l.amount))).collect();
    c.step(&[accuse(id, accuser, 0)]);
    assert_eq!(c.s.deadline_of(&id), None, "a seat's session pauses the licensed claim");
    run_out_with(&mut c, id, accuser, landed);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }));
    assert_eq!(u128::from(producer_before - c.s.bond(&producer).unwrap().collateral), commitment, "S1: the commitment");
    if !landed {
        assert_eq!(c.s.bond(&full[0]).unwrap().collateral, full_before, "dormant: the full seat is not charged");
        assert!(c.s.consumed_offence(&palw_false_valid_offence_id_v2(&full[0].0, &id)).is_none(), "dormant: no S4 row");
        for (k, before) in partials.iter().zip(partial_before) {
            assert_eq!(c.s.bond(k).unwrap().collateral, before.0, "dormant: {k:?} untouched");
        }
        return;
    }
    let g = {
        // G = g_res + E: what the liability row recorded at the void.
        let row = c.s.panel_liability(&id).expect("the void's liability row");
        row.g_res_sompi + u128::from(row.escrowed_reward)
    };
    let action = (u128::from(full_before) / 4).min(3 * g);
    assert!(c.s.slashable_lock(full[0], id).is_none(), "the full seat's lock is taken");
    assert_eq!(u128::from(full_before - c.s.bond(&full[0]).unwrap().collateral), full_lock.amount + action, "S4: lock + min(25% C, 3G)");
    let row = c.s.consumed_offence(&palw_false_valid_offence_id_v2(&full[0].0, &id)).expect("S4 under the (seat, claim) key");
    assert_eq!((row.kind, row.claim_id, row.execution_root), (PalwOffenceKindV1::PanelFalseValidV2, id, Hash64::default()));
    for (k, before) in partials.iter().zip(partial_before) {
        assert_eq!((c.s.bond(k).unwrap().collateral, c.s.slashable_lock(*k, id).map(|l| l.amount)), before, "C7: {k:?} untouched");
        assert!(c.s.consumed_offence(&palw_false_valid_offence_id_v2(&k.0, &id)).is_none());
    }
}

/// **DA-5 / DA-7 after `Final` (T66's core half, V3S-02, V3S-04).** A `Final` claim with an unmatured
/// vesting row (the one the vesting work writes at `Final`) is accusable:
/// the session re-keys the row to at least its deadline plus the challenge window and re-dates every
/// live lock to it, so neither the row nor a lock can lapse under it. It runs out: the producer takes
/// S3 (`min(25% · C, 3 G)`), the covering (full-mask) signer S4, the `Final` is reversed — recorded as
/// the WITHHOLDING it is (`ProducerWithholding`, on the claim and on the liability row: M3 review F1),
/// never `CourtFraud` — a `DaDefault` is recorded, and the retirement — deferred while the session was
/// open — re-arms at `max(F + retirement, close + 1)`. A claim whose row is gone is not accusable
/// after `Final`.
#[test]
fn t66_da5_da7_a_final_row_is_rekeyed_and_its_default_is_s3_and_s4() {
    for landed in [false, true] {
        t66_body(landed);
    }
}

fn t66_body(landed: bool) {
    let mut c = Chain::new(t12());
    let (id, seats, _) = covered_floor_claim(&mut c, 0x66);
    c.step(&[bond_obj(1, 20_000 * MSK)]);
    let full = full_mask_seats(&c, &id, &seats);
    c.finalize(id);
    let PalwClaimPhaseV2::Final { final_daa } = c.claim(&id).phase else { panic!("Final") };
    // The vesting work wrote the claim's row at `Final` (integration: the row writer is in this line,
    // so the test reads the real row rather than writing one through the carriage).
    let (producer, _, _) = floor_producer(&c.p);
    let row = c.s.vesting_row(&id).expect("the vesting work's row, written at Final").clone();
    assert_eq!((row.producer_bond, row.final_daa), (producer, final_daa), "the claim's own row");
    let expiry = row.expiry_daa;
    // Accused late in the row's life: the session outlives the row's own expiry.
    let at = expiry - 100;
    c.step_at(at, &[accuse(id, bond_key(1), 0)], PalwBlockWorkV3::None, Hash64::default(), 0);
    let session = c.s.da_session(&id, &bond_key(1)).unwrap().clone();
    assert_eq!(session.stage, PalwDaStageV1::FinalRow);
    let until = session.deadline_daa + c.sp.window_challenge_at(at);
    assert_eq!(c.s.vesting_row(&id).unwrap().expiry_daa, until, "the row is re-keyed behind the session (V3S-02)");
    assert_eq!(c.s.slashable_lock(full[0], id).unwrap().expiry_daa, until, "and the full seat's lock follows it (V3S-04)");
    assert_eq!(c.s.deadline_of(&id), None, "retirement waits for the session");
    let producer_before = c.s.bond(&producer).unwrap().collateral;
    let full_before = c.s.bond(&full[0]).unwrap().collateral;
    let closed = run_out_with(&mut c, id, bond_key(1), landed);
    assert!(
        matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, voided_daa } if voided_daa == closed),
        "the Final is reversed (#8), as the withholding it is (M3 review F1)"
    );
    let liability = c.s.panel_liability(&id).expect("the liability row");
    assert_eq!(
        (liability.voided_daa, liability.void_reason),
        (Some(closed), Some(PalwVoidReasonV2::ProducerWithholding)),
        "the row is marked with the withholding, never CourtFraud"
    );
    // S3 = min(25% · C₀, 3 G) through the burn hook (S-4): the burned row is S3's once-per-claim
    // marker, and G is the gain the liability row recorded — `claim_g_v1` reads the record, never the
    // live facts, whose realizable-rights term shrinks with time (the gain is fixed at the fraud).
    let g = {
        let row = c.s.panel_liability(&id).expect("the liability row");
        row.g_res_sompi + u128::from(row.escrowed_reward)
    };
    let debit = u128::from(producer_before - c.s.bond(&producer).unwrap().collateral);
    assert!(c.s.vesting_row(&id).is_none(), "the row is burned: S3's marker");
    assert_eq!(debit, (u128::from(producer_before) / 4).min(3 * g), "S3");
    let record = c
        .s
        .consumed_offence(&kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1(&producer.0, &id))
        .expect("the DaDefault record")
        .clone();
    assert_eq!(u128::from(record.amount), debit, "the record's nominal tier is the S3 debit");
    if landed {
        assert!(full_before > c.s.bond(&full[0]).unwrap().collateral, "S4 on the covering signer, its lock still live");
    } else {
        assert_eq!(full_before, c.s.bond(&full[0]).unwrap().collateral, "dormant: no signer is charged");
    }
    let record = c
        .s
        .consumed_offence(&kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1(&producer.0, &id))
        .expect("one DaDefault record");
    assert_eq!(
        (u128::from(record.amount), u128::from(record.collected), record.claim_id, record.accepted_daa),
        (debit, debit, id, closed),
        "the record: the producer's nominal tier, its collected debit (its exit shut), the claim"
    );
    let retire = c.s.deadline_of(&id).expect("the retirement re-arms");
    assert_eq!(retire, (closed + c.sp.claim_retirement_daa()).max(closed + 1), "max(terminal + retirement, close + 1)");
}

/// **DA-6: refuted exposure is held — burned at retirement unless the claim is convicted, refunded
/// if it is.** Two claims each carry a refuted entry (written through the carriage — refutation needs
/// a real answer, which `t46`'s real-claim harness covers). One claim retires unconvicted: the entry is
/// burned through `slash_seat` and the accuser ledger empties. The other is convicted by a DA default:
/// the entry is refunded (no debit) in the default's block.
#[test]
fn t69_da6_refuted_exposure_is_burned_at_retirement_or_refunded_at_a_conviction() {
    let mut c = Chain::new(t12());
    c.step(&[bond_obj(1, 20_000 * MSK), bond_obj(2, 20_000 * MSK)]);
    let held = 320 * MSK as u128;
    // (a) unconvicted: licensed, finalized, retired.
    let (a, aseats, abound) = bound_floor_claim(&mut c, 0x6A);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: a, receipts: aseats.iter().map(|(k, _)| valid(a, *k, abound)).collect() }]);
    c.s = edited(&c.sp, &c.s, |carriage| {
        carriage.da_claims.insert(a, PalwDaClaimV1 { refuted_held: vec![(bond_key(1), held)], last_closed_daa: Some(c.daa), ..Default::default() });
    });
    assert_eq!(palw_accuser_exposure_v1(&c.s, &bond_key(1)), held);
    c.finalize(a);
    let before = c.s.bond(&bond_key(1)).unwrap().collateral;
    let retire = c.s.deadline_of(&a).expect("the retirement");
    c.step_at(retire + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(c.s.claim(&a).is_none() && c.s.da_claim(&a).is_none(), "the claim and its record retire");
    assert_eq!(u128::from(before - c.s.bond(&bond_key(1)).unwrap().collateral), held, "the held exposure is burned");
    assert_eq!(palw_accuser_exposure_v1(&c.s, &bond_key(1)), 0);
    // (b) convicted by a DA default: refunded.
    let (b, _, _) = bound_floor_claim(&mut c, 0x6B);
    c.s = edited(&c.sp, &c.s, |carriage| {
        carriage.da_claims.insert(b, PalwDaClaimV1 { refuted_held: vec![(bond_key(2), held)], last_closed_daa: Some(c.daa), ..Default::default() });
    });
    let before = c.s.bond(&bond_key(2)).unwrap().collateral;
    c.step(&[accuse(b, bond_key(1), 0)]);
    run_out(&mut c, b, bond_key(1));
    assert!(matches!(c.claim(&b).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }));
    assert!(c.s.da_claim(&b).unwrap().refuted_held.is_empty(), "the conviction refunds the refuted entry");
    assert_eq!(c.s.bond(&bond_key(2)).unwrap().collateral, before, "no debit");
    assert_eq!(palw_accuser_exposure_v1(&c.s, &bond_key(2)), 0);
}

/// **T27 / T68 × DA-9 (X2): only a `Valid` serves, and the unserved seats' own sessions pause the
/// claim.** Three `Valid`s and two `Unavailable`s license (V1's quorum) and hold the escrow (SR-1
/// cond. 2); both unserved seats accuse at the licence (P2-6), each its own session (no
/// serialization); the claim cannot reach `Final` while either is open, and when they run out the
/// producer takes S1 and — with signer liability armed — the three `Valid` signers (full masks) S4;
/// the two `Unavailable` seats, whose receipts are not `Valid`, pay nothing (N9: never against an
/// `Unavailable` filer) — as shipped since P2-7. The dormant twin (the pre-P2-7 value) charges no signer.
#[test]
fn t27_t68_x2_unserved_seats_accuse_and_only_valid_signers_are_charged() {
    for landed in [false, true] {
        t27_t68_body(landed);
    }
}

fn t27_t68_body(landed: bool) {
    let mut c = Chain::new(t12());
    let (id, seats, bound) = bound_floor_claim(&mut c, 0x27);
    let mut receipts: Vec<_> = seats[..3].iter().map(|(k, _)| valid(id, *k, bound)).collect();
    receipts.extend(seats[3..].iter().map(|(k, _)| unavailable(id, *k, bound)));
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
    let licensed = c.claim(&id);
    assert!(!licensed.rcore.escrow_released && licensed.rcore.unserved_seen, "Unavailable holds the escrow (SR-1 cond. 2)");
    c.step(&[accuse(id, seats[3].0, 0), accuse(id, seats[4].0, 1)]);
    assert_eq!(c.s.da_claim(&id).unwrap().open_seat_sessions, 2, "each unserved seat its own session");
    let before: Vec<u64> = seats.iter().map(|(k, _)| c.s.bond(k).unwrap().collateral).collect();
    run_out_with(&mut c, id, seats[3].0, landed);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }));
    assert!(c.s.da_session(&id, &seats[4].0).is_none(), "confirmed once: the other session closes with it");
    for (i, (k, _)) in seats.iter().enumerate() {
        let paid = before[i] - c.s.bond(k).unwrap().collateral;
        if !landed {
            assert_eq!(paid, 0, "dormant: seat {i} pays nothing");
        } else if i < 3 {
            assert!(paid > 0, "Valid signer {i} (full mask) takes S4");
        } else {
            assert_eq!(paid, 0, "Unavailable seat {i} pays nothing");
        }
    }
}

/// **T84 / T42 (DA part, A-6): an accuser uses its free half, and its exposure is on the ledger B-3
/// reads until the claim resolves.** A bond whose own claims fill its 500‰ work ceiling still
/// accuses (`max(committed, 500‰·C) + accuser + new ≤ C`), and the session's exposure is on its
/// accuser ledger (`palw_accuser_exposure_v1`, the clause `palw_bond_collateral_is_locked_v6` holds
/// exit on) until the claim's default convicts it and returns the exposure. The fence-off twin:
/// ADR-0062's accusation reserves under the 500‰ ceiling, which the full bond fails.
#[test]
fn t84_t42_an_accuser_uses_its_free_half_and_its_exposure_is_held_until_the_claim_resolves() {
    for armed in [true, false] {
        let p = if armed { t12() } else { twin(&t12()) };
        let mut c = Chain::new(p);
        c.step(&[bond_obj(1, 20_000 * MSK)]);
        for seed in 0..3u64 {
            let (env, key, _) = floor_attempt(&c, 1, 0x4210 + seed);
            c.step_at(c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
        }
        let (id, _, _) = bound_floor_claim(&mut c, 0x42);
        let committed = palw_bond_committed_v1(&c.s, &bond_key(1), c.daa + 1, None, c.sp.window_court());
        // The premise: the bond's own work fills its 500‰ ceiling (collateral set to exactly twice it).
        let full = u64::try_from(2 * committed).unwrap();
        c.s = edited(&c.sp, &c.s, |carriage| carriage.bonds.get_mut(&bond_key(1)).unwrap().collateral = full);
        let result = try_step(&c, &[accuse(id, bond_key(1), 0)]);
        if !armed {
            assert!(matches!(result, Err(PalwStateV2Error::AccusationExposureCeiling { .. })), "fence off: the 500‰ ceiling refuses");
            continue;
        }
        result.expect("the free half admits the accusation");
        c.step(&[accuse(id, bond_key(1), 0)]);
        assert!(palw_accuser_exposure_v1(&c.s, &bond_key(1)) > 0, "B-3's accuser clause reads it");
        run_out(&mut c, id, bond_key(1));
        assert_eq!(palw_accuser_exposure_v1(&c.s, &bond_key(1)), 0, "nothing held once the claim is convicted");
    }
}

/// **T34 (M3's half): DA on every class — the 8k row.** A licensed attempt claim of testnet-12's
/// short-window model class: a seat's event session pauses it, draws nothing (a held-context
/// attempt), runs out, and the producer takes S1 — the whole commitment, `w + esc + rr` — and the
/// claim voids `ProducerWithholding` with a `DaDefault` recorded. Every block reloads. The fence-off
/// twin: ADR-0062's session takes the phase and its lapse voids the same claim for the same charge.
#[test]
fn t34_da_on_the_8k_row_defaults_like_the_floor() {
    for armed in [true, false] {
        t34_body(armed);
    }
}

fn t34_body(armed: bool) {
    use kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1;
    let p = if armed { t12() } else { twin(&t12()) };
    let (short, _) = model_classes(&p);
    let mut m = model_chain(p, short, 1);
    let id = model_claim(&mut m, short, 1, 0x34);
    let seats = honest_seats(&m.p, 5);
    m.s = readied(&m.sp, &m.s, &honest(&m.p), short, m.daa);
    let bound = m.bind(id, &seats);
    m.s = readied(&m.sp, &m.s, &honest(&m.p), short, m.daa);
    m.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect() }]);
    let claim = m.claim(&id);
    let commitment = palw_claim_bond_reservation_v1(&m.sp, &claim).unwrap();
    let before = m.s.bond(&bond_key(1)).unwrap().collateral;
    m.s = readied(&m.sp, &m.s, &honest(&m.p), short, m.daa);
    m.step(&[accuse(id, seats[0].0, 0)]);
    if armed {
        assert_eq!(m.s.da_session(&id, &seats[0].0).unwrap().units.len(), 1, "a held-context attempt's event session draws nothing");
        assert_eq!(m.s.deadline_of(&id), None, "the seat's session pauses the licensed claim");
        m.s = readied(&m.sp, &m.s, &honest(&m.p), short, m.daa);
        run_out(&mut m, id, seats[0].0);
    } else {
        assert!(matches!(m.claim(&id).phase, PalwClaimPhaseV2::DefaultDisputed { .. }), "fence off: the v1 session");
        let lapse = m.daa + m.sp.window_challenge() + 1;
        m.s = readied(&m.sp, &m.s, &honest(&m.p), short, m.daa);
        m.step_at(lapse, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    }
    assert!(matches!(m.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }));
    assert_eq!(u128::from(before - m.s.bond(&bond_key(1)).unwrap().collateral), commitment, "S1: w + esc + rr");
    assert_eq!(m.s.consumed_offence(&palw_da_offence_id_v1(&bond_key(1).0, &id)).is_some(), armed, "a DaDefault past the fence only");
}

/// **DA-8 / C8: the post-`Final` window is the record's life, deferred by open sessions and bounded
/// by retention.** A `Final` claim with an unmatured row is accusable until its record retires; a
/// session opened near the end defers retirement past `F + claim_retirement_daa`; an accusation
/// whose disclose window would run past `trace_retention_daa` is refused (`DaOutsideRetention`) —
/// the one bound that holds the chain of overlapping sessions (acceptance + 4,200 on the floor's
/// retention).
#[test]
fn da8_the_post_final_window_is_the_records_life_bounded_by_retention() {
    let mut c = Chain::new(t12());
    c.step(&[bond_obj(1, 20_000 * MSK), bond_obj(2, 20_000 * MSK)]);
    let (id, seats, bound) = bound_floor_claim(&mut c, 0x08);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect() }]);
    c.finalize(id);
    let PalwClaimPhaseV2::Final { final_daa } = c.claim(&id).phase else { panic!("Final") };
    let (producer, _, _) = floor_producer(&c.p);
    // Retention pinned short (the junk attempt's is 999,999): the chain's own bound.
    let retention = final_daa + c.sp.claim_retirement_daa() + 1_500;
    c.s = edited(&c.sp, &c.s, |carriage| {
        carriage.claims.get_mut(&id).unwrap().trace_retention_daa = retention;
    });
    assert_eq!(c.s.vesting_row(&id).map(|row| row.producer_bond), Some(producer), "the vesting work's row, written at Final");
    let retire = c.s.deadline_of(&id).expect("the retirement");
    assert_eq!(retire, final_daa + c.sp.claim_retirement_daa());
    // Fence off, a Final claim is never accusable (ADR-0062: terminal claims refuse).
    {
        let mut t = Chain::new(twin(&t12()));
        t.step(&[bond_obj(1, 20_000 * MSK)]);
        let (tid, tseats, tbound) = bound_floor_claim(&mut t, 0x08);
        t.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: tid, receipts: tseats.iter().map(|(k, _)| valid(tid, *k, tbound)).collect() }]);
        t.finalize(tid);
        assert!(matches!(try_step(&t, &[accuse(tid, bond_key(1), 0)]), Err(PalwStateV2Error::WrongPhase { .. })), "fence off");
    }
    // Accused just before retirement: the session defers it.
    c.step_at(retire - 1, &[accuse(id, bond_key(1), 0)], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(c.s.deadline_of(&id), None, "retirement is deferred while the session is open");
    // A second accuser past the retention bound is refused.
    let late = retention - c.sp.window_challenge() + 1;
    c.step_at(late - 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(
        matches!(try_step(&c, &[accuse(id, bond_key(2), 0)]), Err(PalwStateV2Error::DaOutsideRetention { .. })),
        "past trace_retention_daa − W_disclose nothing opens"
    );
    run_out(&mut c, id, bond_key(1));
}

// ---------------------------------------------------------------------------------------------
// The M3 review's fixes (MERGE-WITH-FIXES): F1, F3, the court default's refund, the signer
// liability predicate's clock. The reviewer's probes (`review_m3_da_probe`), made tests.
// ---------------------------------------------------------------------------------------------

/// The full seat of a coverage licence and the receipt the licence carried for it (public on chain).
fn full_seat_receipt(
    c: &Chain,
    id: Hash64,
    seats: &[(PalwBondKeyV2, Hash64)],
    bound: u64,
) -> (PalwBondKeyV2, PalwSeatReceiptV3) {
    let r = covered(id, c.anchor(&id), seats, &[0, 1, 2, 3, 4], bound)
        .into_iter()
        .find(|r| {
            c.s.slashable_lock(r.receipt.seat_bond, id).is_some_and(|lock| lock.segments > 0 && lock.attested.is_full(lock.segments))
        })
        .expect("a full seat");
    (r.receipt.seat_bond, r)
}

/// A kind-3 object against `seat` on its own segmented receipt, with `contradiction`.
fn kind3(id: Hash64, seat: PalwBondKeyV2, receipt: PalwSeatReceiptV3, contradiction: PalwPanelContradictionV1) -> PalwConsensusObjectV2 {
    let evidence = borsh::to_vec(&PalwPanelFalseValidEvidenceV2 {
        version: PALW_PANEL_FALSE_VALID_VERSION_V2,
        claim_id: id,
        accused_seat: seat.0,
        receipt: PalwFalseValidReceiptV1::Segmented(receipt),
        contradiction,
        prompt_ids_opening: None,
        reporter_reveal: Vec::new(),
    })
    .unwrap();
    PalwConsensusObjectV2::ObjectiveOffence {
        kind: PalwOffenceKindV1::PanelFalseValidV2,
        accused: seat,
        evidence_id: palw_offence_evidence_digest_v1(&evidence),
        evidence,
    }
}

/// The next block folded with `palw_offence_attribution` armed as the processor arms it on
/// testnet-12, and signer liability `landed` (the shipped value is P2-7's `true`; `false` is the
/// dormant twin) — the block a filed kind 3 lands in.
fn try_step_f2(c: &Chain, objects: &[PalwConsensusObjectV2], landed: bool) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let daa = c.daa + 1;
    let x = ctx(0xCA_0000 + daa, daa, daa, 0);
    let mut e = c.extras_at(daa);
    e.offence_attribution_active = c.p.palw_offence_attribution_active_at(daa);
    assert!(e.offence_attribution_active, "testnet-12 arms F2");
    assert_eq!(
        e.seat_da_answer_landed,
        kaspa_consensus_core::palw_da_rcore_v1::PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1,
        "the fixtures' extras carry the shipped value"
    );
    e.seat_da_answer_landed = landed;
    fold_with(&c.p, &c.sp, &c.s, &x, objects, PalwBlockWorkV3::None, Hash64::default(), &e).map(|(child, _, _)| child)
}

/// **M3 review F1: a DA default after `Final` is the WITHHOLDING it is, and kind 3 convicts no signer
/// on top of it.** A bystander's session on a licensed coverage claim does not pause it (V3S-08), so
/// the claim goes `Final` with the session open (`window_challenge` 120 < `W_disclose` 1,200) and the
/// default lands at `FinalRow`. The reversal writes `ProducerWithholding` on the claim and on the
/// liability row — never `CourtFraud` — so kind 3 against the honest full seat is refused as
/// `CourtFraud` (no such void) by the adjudicator and by the fold alike. Before the fix the reversal
/// wrote `CourtFraud` and the fold took the seat's whole lock.
///
/// Both sides of P2-7's constant: **dormant** (`false`) — no signer is charged at all, and kind 3's
/// `ProducerWithholding` is refused too (N9's `da_confirmed` gate is shut); **shipped** (`true`,
/// since P2-7 landed the seats' answers) — the covering full seat takes DA-7's S4 in the default
/// itself (T66), and a later kind 3 `ProducerWithholding` against it charges nothing more (N9 shares
/// DA-7's key).
#[test]
fn m3r_f1_a_post_final_da_default_is_withholding_and_convicts_no_signer() {
    for landed in [false, true] {
        m3r_f1_body(landed);
    }
}

fn m3r_f1_body(landed: bool) {
    let mut c = Chain::new(t12());
    let (id, seats, bound) = covered_floor_claim(&mut c, 0x91);
    c.step(&[bond_obj(1, 20_000 * MSK)]);
    c.step(&[accuse(id, bond_key(1), 0)]);
    assert!(!c.s.da_session(&id, &bond_key(1)).unwrap().accuser_is_seat, "a bystander's session");
    c.finalize(id);
    assert!(c.s.da_session(&id, &bond_key(1)).is_some(), "the session outlives the challenge window");
    assert!(c.s.vesting_row(&id).is_some(), "FinalRow reached with the vesting work's row");
    let (full, receipt) = full_seat_receipt(&c, id, &seats, bound);
    let lock = *c.s.slashable_lock(full, id).expect("the full seat's lock");
    let full_before = c.s.bond(&full).unwrap().collateral;
    let closed = run_out_with(&mut c, id, bond_key(1), landed);
    assert!(
        matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, voided_daa } if voided_daa == closed),
        "the Final is reversed as the withholding"
    );
    assert!(c.s.vesting_row(&id).is_none(), "the producer's S3 burned the row");
    let row = c.s.panel_liability(&id).expect("the liability row");
    assert_eq!((row.voided_daa, row.void_reason), (Some(closed), Some(PalwVoidReasonV2::ProducerWithholding)));
    assert!(
        !c.s.palw_void_binds_claim_v1(&id, PalwVoidReasonV2::CourtFraud, closed),
        "no CourtFraud void binds the claim, on the record or the row"
    );
    let after_default = c.s.bond(&full).unwrap().collateral;
    if landed {
        assert!(after_default < full_before, "shipped: DA-7's S4 on the covering full seat, its lock still live (T66)");
    } else {
        assert_eq!(after_default, full_before, "dormant: the honest full seat pays nothing for the producer's silence");
    }
    for contradiction in [
        PalwPanelContradictionV1::CourtFraud { voided_daa: closed },
        PalwPanelContradictionV1::ProducerWithholding { voided_daa: closed },
    ] {
        let object = kind3(id, full, receipt.clone(), contradiction.clone());
        let PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } = &object else { unreachable!() };
        let rules = PalwIdentityRulesV1 {
            prompt_ids_form: c.p.palw_prompt_ids_form_v1(),
            base_class_id: c.sp.base_class_id(),
            da_signer_liability: palw_da_signer_liability_armed_v1(&c.sp, landed, c.daa),
        };
        assert_eq!(rules.da_signer_liability, landed, "signer liability follows P2-7's constant");
        let finding = palw_check_panel_false_valid_v2(&c.s, &full, evidence, false, false, rules, None);
        let folded = try_step_f2(&c, &[object], landed);
        let court_fraud = matches!(contradiction, PalwPanelContradictionV1::CourtFraud { .. });
        if court_fraud || !landed {
            assert!(finding.is_err(), "{contradiction:?}: the adjudicator refuses it: {finding:?}");
            assert!(
                matches!(folded, Err(PalwStateV2Error::ObjectiveOffenceRefused(..))),
                "{contradiction:?}: the fold refuses it: {folded:?}"
            );
        } else if let Ok(child) = folded {
            assert_eq!(
                child.bond(&full).unwrap().collateral,
                after_default,
                "shipped: kind 3's ProducerWithholding charges nothing on top of DA-7's S4 (N9's shared key)"
            );
        }
    }
    if !landed {
        assert_eq!(c.s.slashable_lock(full, id).map(|l| l.amount), Some(lock.amount), "dormant: the seat keeps its lock");
    }
}

/// **M3 review F3: no session opens on a unit the chain already holds the answer to — so an answered
/// accusation cannot be replayed.** The record a real `Flat` answer leaves on a floor claim's one-row
/// run (row 0 answered) is in place and no session is open: a seat's accusation of row 0 — exactly
/// the object that opened the session the answer refuted, whose signature binds only (domain, claim,
/// index, accuser) — is refused `DaUnitAlreadyAnswered` in one block and again in a later one; it
/// opens nothing, pauses nothing, holds and spends nothing of the seat's budget, and the seat's
/// collateral is whole after the claim retires. Before the fix each replay opened a session nobody
/// could end, paused the claim 1,200 DAA and burned 320 MSK of the accuser's stake at retirement.
#[test]
fn m3r_f3_an_answered_unit_opens_no_session_and_cannot_be_replayed() {
    let mut c = Chain::new(t12());
    let (id, seats, _) = covered_floor_claim(&mut c, 0x95);
    let at = c.daa;
    c.s = edited(&c.sp, &c.s, |carriage| {
        carriage.da_claims.insert(
            id,
            PalwDaClaimV1 {
                answered: [PalwDaUnitV1::Event { row: 0, tile: 0 }].into_iter().collect(),
                flat_answered: true,
                last_closed_daa: Some(at),
                ..Default::default()
            },
        );
    });
    let seat = seats[1].0;
    let before = c.s.bond(&seat).unwrap().collateral;
    let deadline = c.s.deadline_of(&id);
    for _ in 0..2 {
        let refused = try_step(&c, &[accuse(id, seat, 0)]);
        assert!(matches!(refused, Err(PalwStateV2Error::DaUnitAlreadyAnswered(claim)) if claim == id), "{refused:?}");
        c.step(&[]);
        assert!(c.s.da_session(&id, &seat).is_none(), "nothing opens");
        assert_eq!(c.s.deadline_of(&id), deadline, "nothing pauses");
        assert_eq!(c.s.da_claim(&id).unwrap().opened_by_seat.get(&seat), None, "no session budget spent");
        assert_eq!(palw_accuser_exposure_v1(&c.s, &seat), 0, "no exposure held");
    }
    c.finalize(id);
    let retire = c.s.deadline_of(&id).expect("the retirement");
    c.step_at(retire + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(c.s.claim(&id).is_none(), "the claim retires");
    assert_eq!(c.s.bond(&seat).unwrap().collateral, before, "and the seat's stake is whole");
}

/// **M3 review, item 5: a court DEFAULT refunds the refuted exposure, as a proven conviction does.**
/// A refuted DA entry is held on a licensed floor claim (the filer's garbage-path cost, DA-6); a
/// bystander's court on the claim runs out on the responder's silence, which past
/// `palw_offence_attribution` voids the claim `CourtDefault` — charged exactly as `CourtFraud` — and
/// the held entry is refunded in that block: the filer nets zero, as DA-6 promises. Before the fix it
/// stayed held and burned at retirement.
#[test]
fn m3r_a_court_default_refunds_the_refuted_exposure() {
    let mut c = Chain::new(t12());
    c.attribution = true;
    c.step(&[bond_obj(1, 20_000 * MSK), bond_obj(2, 20_000 * MSK)]);
    let (id, seats, bound) = bound_floor_claim(&mut c, 0xC5);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect() }]);
    let held = 320 * MSK as u128;
    let claim = c.claim(&id);
    let at = c.daa;
    c.s = edited(&c.sp, &c.s, |carriage| {
        carriage.da_claims.insert(id, PalwDaClaimV1 { refuted_held: vec![(bond_key(1), held)], last_closed_daa: Some(at), ..Default::default() });
        let ladder = kaspa_consensus_core::palw_bisect::PalwBisectLadderV1::open(
            &id,
            &claim.trace_root,
            &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&bond_key(2)),
            &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&claim.bond),
            kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
            16,
            at,
            at + 50,
        )
        .expect("a ladder opens");
        carriage.court_sessions.insert(
            ladder.session_id(),
            kaspa_consensus_core::palw_state_v2::PalwCourtSessionStateV2 {
                claim: id,
                challenger_bond: bond_key(2),
                opened_daa: at,
                deadline_daa: at + c.sp.window_court(),
                ladder,
                dissection: None,
            },
        );
    });
    assert_eq!(palw_accuser_exposure_v1(&c.s, &bond_key(1)), held, "the refuted entry is held");
    let filer_before = c.s.bond(&bond_key(1)).unwrap().collateral;
    // The responder never moves: step until the court ends.
    let mut guard = 0;
    while !matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { .. }) {
        c.step(&[]);
        guard += 1;
        assert!(guard < 4_000, "the court ends");
    }
    assert!(
        matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtDefault, .. }),
        "the responder's silence is a court DEFAULT past the attribution fence: {:?}",
        c.claim(&id).phase
    );
    assert!(c.s.da_claim(&id).unwrap().refuted_held.is_empty(), "the default refunds the refuted entry");
    assert_eq!(palw_accuser_exposure_v1(&c.s, &bond_key(1)), 0);
    assert_eq!(c.s.bond(&bond_key(1)).unwrap().collateral, filer_before, "the filer nets zero");
}

/// **M3 review, LOW: signer liability is read at the conviction's DAA** — `palw_rcore_plus` ACTIVE
/// there, not merely scheduled; and never without seats' answering landed.
#[test]
fn m3r_signer_liability_is_read_at_the_conviction_daa() {
    let sp = bundle(&t12()).state.clone();
    let delay = sp.withdrawal_delay_daa();
    let later = sp.clone().with_rcore_plus_mirrors(Some(500), delay, Vec::new());
    assert!(!palw_da_signer_liability_armed_v1(&later, true, 499), "scheduled, not active");
    assert!(palw_da_signer_liability_armed_v1(&later, true, 500));
    assert!(!palw_da_signer_liability_armed_v1(&later, false, 500), "never before seats answer");
    assert!(palw_da_signer_liability_armed_v1(&sp, true, 0), "testnet-12: from genesis");
    let dormant = sp.with_rcore_plus_mirrors(None, 0, Vec::new());
    assert!(!palw_da_signer_liability_armed_v1(&dormant, true, u64::MAX));
}

/// **P2-7 (ADR-0152 X7, DA-4): the R-core court's duty list — the producer's from the open sessions,
/// a signer's from its covering lock — with its fence-off twin.**
///
/// A coverage licence (the full seat, four partials); a partial seat accuses row 7, so its session
/// demands the named row and the run's one row `(0, 0)` (DA-3). `palw_disclosure_duties_v1`:
/// * **the producer** owes both units, soonest deadline first, with the session's deadline, the
///   in-run rows the fold reads and `W_disclose` — and named with the producer role even beside a
///   lock of its own bond set (one duty a unit);
/// * **the full seat** (the only full mask) owes the same two units as a covering signer, rank 0;
/// * **a partial seat** — the accuser included — and **a bystander** owe nothing: a partial mask
///   covers no unit (C7), and a bystander holds no lock;
/// * **an answered unit** leaves the list (a `Flat` on record answers `(0, 0)`, not row 7);
/// * **an expired lock** covers nothing — the fold's `is_live_v3` on both clocks, read through the
///   exported predicate — and the signer's duty goes with it, while the producer's stays;
/// * **retention** (P2-7): every bond with a live lock on the claim keeps it while a session can
///   still open (`now ≤ trace_retention_daa`), the partial seats too; the producer and a bystander
///   keep nothing by it; an expired lock keeps nothing.
///
/// **Fence off** (testnet-12 with `palw_rcore_plus = None`): ADR-0062's accusation takes the claim's
/// phase, the R-core list is empty for every bond, and the v1 list (`palw_da_duties_v2`) names the
/// claim for its producer, as before.
#[test]
fn p2_7_the_disclosure_duties_follow_the_open_sessions_and_the_covering_locks() {
    use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1 as U;
    use kaspa_consensus_core::palw_producer_v2::{PalwDisclosureRoleV1 as Role, palw_da_duties_v2, palw_disclosure_duties_v1};
    use kaspa_consensus_core::palw_state_v2::{palw_da_disclose_window_daa_v1, palw_da_lock_covers_unit_v1, palw_da_lock_live_v1};
    for armed in [true, false] {
        let p = if armed { t12() } else { twin(&t12()) };
        let mut c = Chain::new(p);
        c.step(&[bond_obj(1, 20_000 * MSK)]);
        let bystander = bond_key(1);
        let (producer, _, _) = floor_producer(&c.p);
        if !armed {
            // ADR-0062's court: the accusation takes the phase; R-core's list is empty.
            let (id, seats, _) = bound_floor_claim(&mut c, 0x27F);
            c.step(&[accuse(id, bystander, 0)]);
            assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::DefaultDisputed { .. }), "fence off: the v1 session");
            let at = c.daa + 1;
            for mine in [producer, seats[0].0, bystander] {
                let read = palw_disclosure_duties_v1(&c.s, &c.sp, &c.extras_at(at), &[mine], at);
                assert!(read.duties.is_empty() && read.retain.is_empty(), "fence off: no R-core duty for {mine:?}");
            }
            let v1 = palw_da_duties_v2(&c.s, &c.sp, &[producer]);
            assert_eq!(v1.iter().map(|d| d.claim_id).collect::<Vec<_>>(), vec![id], "fence off: the v1 list names the claim");
            continue;
        }
        let (id, seats, _) = covered_floor_claim(&mut c, 0x27E);
        let full = full_mask_seats(&c, &id, &seats);
        assert_eq!(full.len(), 1, "one full seat in the coverage geometry");
        let full = full[0];
        let partials: Vec<PalwBondKeyV2> = seats.iter().map(|(k, _)| *k).filter(|k| *k != full).collect();
        let accuser = partials[0];
        c.step(&[accuse(id, accuser, 7)]);
        let session = c.s.da_session(&id, &accuser).expect("the seat's session").clone();
        assert_eq!(
            session.units,
            vec![U::Event { row: 7, tile: 0 }, U::Event { row: 0, tile: 0 }],
            "the named row and the run's one row"
        );
        let at = c.daa + 1;
        let extras = c.extras_at(at);
        let read =
            |s: &PalwChainStateV2, mine: &[PalwBondKeyV2], at: u64| palw_disclosure_duties_v1(s, &c.sp, &c.extras_at(at), mine, at);
        let retention = c.claim(&id).trace_retention_daa;
        // The producer: both units, soonest deadline first (equal here, so in unit order).
        let own = read(&c.s, &[producer], at);
        assert_eq!(
            own.duties.iter().map(|d| d.unit).collect::<Vec<_>>(),
            vec![U::Event { row: 0, tile: 0 }, U::Event { row: 7, tile: 0 }]
        );
        for duty in &own.duties {
            assert_eq!((duty.role, duty.discloser, duty.signer_rank, duty.executor_bond), (Role::Producer, producer, 0, producer));
            assert_eq!(duty.deadline_daa, session.deadline_daa, "the session's deadline");
            assert_eq!(
                (duty.in_run_rows, duty.disclose_window_daa),
                (1, palw_da_disclose_window_daa_v1(&c.sp)),
                "the fold's rows and W"
            );
            assert_eq!((duty.claim_id, duty.execution_root, duty.free_prompt), (id, c.claim(&id).execution_root, false));
        }
        assert!(own.retain.is_empty(), "the producer holds no lock on its own claim");
        assert_eq!(read(&c.s, &[producer, full], at).duties.iter().filter(|d| d.role == Role::Producer).count(), 2, "one duty a unit");
        // The full seat: the same units, as a covering signer, rank 0.
        let signer = read(&c.s, &[full], at);
        assert_eq!(signer.duties.iter().map(|d| d.unit).collect::<Vec<_>>(), own.duties.iter().map(|d| d.unit).collect::<Vec<_>>());
        assert!(signer.duties.iter().all(|d| (d.role, d.discloser, d.signer_rank) == (Role::CoveringSigner, full, 0)));
        assert!(
            palw_da_lock_covers_unit_v1(&c.s, &c.sp, &extras, &full, &id, &U::Event { row: 0, tile: 0 }, at),
            "the fold's predicate"
        );
        // Every unit's covering signers, from one walk of the lock map (the P2-7 review's LOW): a list
        // a unit, in the units' order, each exactly the seats the per-unit predicate reads as covering.
        {
            use kaspa_consensus_core::palw_state_v2::palw_da_covering_signers_v1;
            let asked = [U::Event { row: 0, tile: 0 }, U::Event { row: 7, tile: 0 }];
            let lists = palw_da_covering_signers_v1(&c.s, &c.sp, &extras, &id, &asked, at);
            assert_eq!(lists, vec![vec![full], vec![full]], "the full seat covers both, and nobody else");
            for (unit, list) in asked.iter().zip(&lists) {
                let one: Vec<PalwBondKeyV2> = seats
                    .iter()
                    .map(|(k, _)| *k)
                    .filter(|k| palw_da_lock_covers_unit_v1(&c.s, &c.sp, &extras, k, &id, unit, at))
                    .collect();
                assert_eq!(list, &one, "{unit:?}: the per-unit predicate's set");
            }
            assert!(palw_da_covering_signers_v1(&c.s, &c.sp, &extras, &id, &[], at).is_empty());
        }
        assert_eq!(signer.retain, vec![(id, retention)], "a live lock keeps the claim's material");
        // Partial seats (the accuser included) and a bystander: nothing to answer.
        for mine in partials.iter().copied().chain([bystander]) {
            assert!(read(&c.s, &[mine], at).duties.is_empty(), "{mine:?} covers no unit");
            assert!(!palw_da_lock_covers_unit_v1(&c.s, &c.sp, &extras, &mine, &id, &U::Event { row: 0, tile: 0 }, at));
        }
        assert_eq!(read(&c.s, &[partials[1]], at).retain, vec![(id, retention)], "a partial seat's live lock keeps it too");
        assert!(read(&c.s, &[bystander], at).retain.is_empty());
        // The retention predicate `retain` and the node's janitor share (DA-8): owed through the
        // claim's retention on every phase a session can carry — `Final` too (a `FinalRow` session,
        // S3) — and never on a voided claim, whose sessions are released with it.
        {
            use kaspa_consensus_core::palw_da_rcore_v1::palw_da_material_owed_v1;
            let record = c.claim(&id);
            assert!(palw_da_material_owed_v1(&record, retention) && !palw_da_material_owed_v1(&record, retention + 1));
            let with = |phase: PalwClaimPhaseV2| kaspa_consensus_core::palw_state_v2::PalwClaimStateV2 { phase, ..record.clone() };
            assert!(palw_da_material_owed_v1(&with(PalwClaimPhaseV2::Final { final_daa: at }), at), "a Final claim is still accused");
            let voided = with(PalwClaimPhaseV2::Voided { voided_daa: at, reason: PalwVoidReasonV2::ProducerWithholding });
            assert!(!palw_da_material_owed_v1(&voided, at), "a voided claim owes nothing");
        }
        // An answered unit leaves the list: a Flat on record answers (0, 0), never row 7 (past the run).
        let flat = edited(&c.sp, &c.s, |carriage| carriage.da_claims.get_mut(&id).expect("the record").flat_answered = true);
        for mine in [producer, full] {
            assert_eq!(read(&flat, &[mine], at).duties.iter().map(|d| d.unit).collect::<Vec<_>>(), vec![U::Event { row: 7, tile: 0 }]);
        }
        // Past the deadline the session is the sweep's: nothing is owed.
        assert!(read(&c.s, &[producer], session.deadline_daa + 1).duties.is_empty(), "the block past the deadline defaults it");
        // An expired lock covers nothing: the signer's duty and its retention go; the producer's stay.
        // The lock's DAA clock released at `at − 1` (written through the carriage — a live claim's
        // lock outlives its sessions by construction, V3S-04), and the second clock has escaped (no
        // licence for 2 × window_court: `settled_anchor_depth` reads `None`), so `is_live_v3` is the
        // DAA clock alone. With the second clock still holding, the same lock is live.
        let expired = edited(&c.sp, &c.s, |carriage| {
            carriage.slashable_locks.get_mut(&(full, id)).expect("the full seat's lock").expiry_daa = at - 1;
        });
        let mut escaped = c.extras_at(at);
        escaped.settled_anchor_depth = None;
        assert!(!palw_da_lock_live_v1(&expired, &c.sp, &escaped, &full, &id, at), "the DAA clock released it");
        let expired_read = palw_disclosure_duties_v1(&expired, &c.sp, &escaped, &[full], at);
        assert!(expired_read.duties.is_empty() && expired_read.retain.is_empty(), "no duty and nothing kept by an expired lock");
        assert_eq!(
            palw_disclosure_duties_v1(&expired, &c.sp, &escaped, &[producer], at).duties.len(),
            2,
            "the producer still owes both"
        );
        if c.extras_at(at).settled_anchor_depth.is_some() {
            assert!(palw_da_lock_live_v1(&expired, &c.sp, &c.extras_at(at), &full, &id, at), "the second clock still holds it");
            assert_eq!(read(&expired, &[full], at).duties.len(), 2, "and the signer still owes both");
        }
    }
}

/// The node's read of an automatic accusation by `accuser` at the next block (P2-6).
fn auto_check(c: &Chain, id: Hash64, accuser: PalwBondKeyV2) -> kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1 {
    let at = c.daa + 1;
    kaspa_consensus_core::palw_producer_v2::palw_da_accusation_check_v1(&c.s, &c.sp, &c.extras_at(at), &id, &accuser, at)
}

/// The node's accusation: the ONE builder, the automatic unit (the fold never reads the signature).
fn auto_accusation(id: Hash64, accuser: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    kaspa_consensus_core::palw_da_rcore_v1::palw_da_accusation_object_v1(
        &h(NET),
        id,
        kaspa_consensus_core::palw_da_rcore_v1::PALW_DA_AUTO_NAMED_UNIT_V1,
        accuser,
        |_, _| Some(vec![1]),
    )
    .expect("the node's builder builds it")
}

/// **T34 (P2 half, core) and C-8: the node's read of an automatic accusation IS the fold's gate, on
/// every class, once per accuser.** On a live floor claim and on the 8k row's licensed claim the read
/// says `File` — the automatic unit (row 0, tile 0: inside the run and the fold's bound), the stage,
/// a seat of the current panel, DA-6's exposure and the deadline — and the object the ONE builder
/// makes from it opens exactly that session in the fold. From then on the read says `AccusedBefore`
/// for that accuser (the session is open), and still after the session closed (`opened_by_seat`):
/// neither a second trigger nor a restart files again. Every other seat still accuses on its own
/// (DA-6: no serialization); the producer cannot; and below `palw_rcore_plus` nothing is filed
/// (`DaCourtDormant`).
#[test]
fn t34_p2_6_the_nodes_read_is_the_folds_gate_on_every_class_once_per_accuser() {
    use kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1 as Check;
    // The floor, live (the `Unavailable`'s accusation: PanelBound).
    let mut c = Chain::new(t12());
    let (id, seats, _) = bound_floor_claim(&mut c, 0x346);
    let (producer, _, _) = floor_producer(&c.p);
    let seat = seats[0].0;
    let Check::File { unit, admission } = auto_check(&c, id, seat) else { panic!("the seat files: {:?}", auto_check(&c, id, seat)) };
    assert_eq!(unit, PalwDaUnitV1::Event { row: 0, tile: 0 }, "the unit its Unavailable names");
    assert!(admission.accuser_is_seat && admission.stage == PalwDaStageV1::Live);
    assert_eq!(
        auto_check(&c, id, producer),
        Check::Refused(PalwStateV2Error::DaAccuserIsTheProducer(producer)),
        "the producer never accuses its own claim"
    );
    c.step(&[auto_accusation(id, seat)]);
    let session = c.s.da_session(&id, &seat).expect("the fold opened the session the read promised").clone();
    assert_eq!(
        (session.units[0], session.accuser_is_seat, session.exposure, session.deadline_daa, session.stage),
        (unit, true, admission.exposure, admission.deadline_daa, admission.stage),
        "exactly what the read said"
    );
    assert_eq!(auto_check(&c, id, seat), Check::AccusedBefore, "once: the session is open");
    assert!(matches!(try_step(&c, &[auto_accusation(id, seat)]), Err(PalwStateV2Error::DaSessionAlreadyOpen { .. })));
    assert!(matches!(auto_check(&c, id, seats[1].0), Check::File { .. }), "every other seat still accuses its own");
    run_out(&mut c, id, seat);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }));
    assert_eq!(auto_check(&c, id, seat), Check::AccusedBefore, "and still after the session closed (opened_by_seat)");

    // The 8k row, licensed (the licence that landed on an unserved seat).
    let p = t12();
    let (short, _) = model_classes(&p);
    let mut m = model_chain(p, short, 1);
    let id = model_claim(&mut m, short, 1, 0x346);
    let seats = honest_seats(&m.p, 5);
    m.s = readied(&m.sp, &m.s, &honest(&m.p), short, m.daa);
    let bound = m.bind(id, &seats);
    m.s = readied(&m.sp, &m.s, &honest(&m.p), short, m.daa);
    m.step(&[PalwConsensusObjectV2::ReceiptLicensed {
        claim: id,
        receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect(),
    }]);
    m.s = readied(&m.sp, &m.s, &honest(&m.p), short, m.daa);
    let seat = seats[0].0;
    let Check::File { admission, .. } = auto_check(&m, id, seat) else { panic!("the 8k seat files: {:?}", auto_check(&m, id, seat)) };
    assert!(admission.accuser_is_seat && admission.stage == PalwDaStageV1::Licensed, "at the licence");
    m.step(&[auto_accusation(id, seat)]);
    let session = m.s.da_session(&id, &seat).expect("the 8k session");
    assert_eq!((session.exposure, session.deadline_daa), (admission.exposure, admission.deadline_daa));
    assert_eq!(auto_check(&m, id, seat), Check::AccusedBefore);

    // Fence off: nothing is filed.
    let mut off = Chain::new(twin(&t12()));
    let (id, seats, _) = bound_floor_claim(&mut off, 0x346);
    assert_eq!(auto_check(&off, id, seats[0].0), Check::Refused(PalwStateV2Error::DaCourtDormant));
}
