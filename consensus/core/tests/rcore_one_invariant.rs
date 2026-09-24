//! **ADR-0152 R-core+ after the S review: one invariant at every gate** — M1 (`committed + accuser +
//! new ≤ C` at every work gate, and accusers on the free half at every accuser gate, the DA arm
//! included), L4 (the capability declaration reads the one ledger and the accuser ledger) and L5 (a
//! lock whose claim is still live is committed whatever its clocks say, so the re-date at Final moves
//! nothing), on testnet-12's own fold.
//!
//! Collateral is moved through the carriage where a test needs a bond at an exact size — the state a
//! slash leaves: collateral is primary data the load never re-derives, and every gate reads it as it
//! finds it.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_one_invariant

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::palw_admission_v2::{PalwAdmissionV2Error, PalwEpochBudgetFencesV1, check_palw_attempt_admission_v2};
use kaspa_consensus_core::palw_producer_v2::palw_producer_facts_v4;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwCourtSessionStateV2, PalwStateV2Error, palw_accuser_exposure_v1, palw_accuser_room_v1,
    palw_bond_collateral_is_locked_v6, palw_bond_committed_raw_v1, palw_claim_escrow_v1,
};

const MSK: u128 = 100_000_000;

/// `s` with `bond`'s posted collateral set to `collateral` (through the carriage: a load reads it).
fn with_collateral(sp: &PalwStateParamsV2, s: &PalwChainStateV2, bond: PalwBondKeyV2, collateral: u64) -> PalwChainStateV2 {
    edited(sp, s, |c| c.bonds.get_mut(&bond).expect("the bond").collateral = collateral)
}

fn raw_depth(p: &Params, daa: u64) -> Option<u64> {
    extras(p, daa).settled_anchor_depth
}

fn fences(p: &Params, daa: u64) -> PalwEpochBudgetFencesV1 {
    let fold = registry_fold(p, daa).expect("the registry");
    PalwEpochBudgetFencesV1 {
        audit_2026_09_23_active: p.palw_audit_2026_09_23_active_at(daa),
        canonical_work_daa: p.palw_canonical_work_daa(),
        base_known_draw: fold.genesis_works.get(&bundle(p).base_class_id).map(|w| w.economic_ccu_per_claim),
        settled_anchor_depth: raw_depth(p, daa),
        escrow_carve: extras(p, daa).escrow_carve,
        ..Default::default()
    }
}

/// A floor attempt by bond `n` with the floor's registered artifact root.
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

/// A floor claim by the genesis producer, bound to the floor seats and licensed through the V1 door.
fn licensed_floor_claim(c: &mut Chain, seed: u64) -> Hash64 {
    let id = c.floor_claim(seed);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed {
        claim: id,
        receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect(),
    }]);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    id
}

/// The court `challenger` opens on `claim` (a step-leaves bisection of 16).
fn court_on(c: &Chain, claim: Hash64, challenger: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    const SPACE: kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1 =
        kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves;
    let record = c.claim(&claim);
    PalwConsensusObjectV2::CourtOpened {
        session_id: kaspa_consensus_core::palw_court_v2::court_session_id_v2(
            &claim,
            &record.trace_root,
            &record.bond,
            &challenger,
            SPACE,
            16,
        ),
        claim,
        challenger_bond: challenger,
        space: SPACE,
        space_size: 16,
        signature: Vec::new(),
    }
}

fn committed_of(c: &Chain, s: &PalwChainStateV2, bond: &PalwBondKeyV2, at: u64) -> u128 {
    palw_bond_committed_raw_v1(s, &c.sp, bond, at, raw_depth(&c.p, at))
}

/// **M1: an accuser's own work is gated by its accuser ledger, and its accusations by its work.**
///
/// Bond 2 (130,000 MSK) accuses a licensed 2M claim (`w` = 59,742.94 MSK) on its free half. On
/// testnet-12 a 2M accusation is a held dissection (`open_held_dissection_v1`, the same accuser gate as
/// a court opening), whose leaf proof this fixture does not build, so the session is placed through the
/// carriage after the gate is asked directly (`palw_accuser_room_v1 ≥ w`). A slash then leaves the bond
/// with `C = accuser + 1.5·a` (`a` = one floor attempt's commitment `w + E`), its 500‰ ceiling still
/// above `2·a`. Its first own attempt fits (`0 + accuser + a ≤ C`); the second does not — the 500‰
/// ceiling alone would admit it, the one invariant refuses it — and admission, the producer's facts
/// and the fold agree. A data-availability accusation by the same bond (the DA arm) is refused on the
/// free half and admitted once the bond has room. `committed + accuser ≤ C` after every block.
#[test]
fn m1_work_and_accusations_share_one_invariant() {
    // ADR-0152 §4-quater (U-D1): the 2M row is closed at launch, so a live 2M claim exists only past the flag day
    // that installs its measured row — this test's premise runs there (`t12_2m_open`, measuring the derived
    // 13,995-DAA deadline).
    let p = t12_2m_open();
    let (_, id2m) = model_classes(&p);
    let mut c = model_chain(p.clone(), id2m, 1);
    let claim = model_claim(&mut c, id2m, 1, 0xB101);
    let seats = honest_seats(&c.p, 5);
    c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
    let bound = c.bind(claim, &seats);
    c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed {
        claim,
        receipts: seats.iter().map(|(k, _)| valid(claim, *k, bound)).collect(),
    }]);
    let record = c.claim(&claim);
    assert!(matches!(record.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    let accuser = bond_key(2);
    let generous = 130_000 * MSK as u64;
    c.step(&[bond_obj(2, generous)]);
    let at = c.daa;
    assert!(palw_accuser_room_v1(&c.s, &c.sp, &accuser, at, raw_depth(&p, at)) >= record.reserved, "the accuser gate admits it");
    let ladder = kaspa_consensus_core::palw_bisect::PalwBisectLadderV1::open(
        &claim,
        &record.trace_root,
        &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&accuser),
        &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&record.bond),
        kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
        16,
        at,
        at + c.sp.window_court(),
    )
    .expect("a ladder");
    let wc = c.sp.window_court();
    c.s = edited(&c.sp, &c.s, |k| {
        k.court_sessions.insert(
            ladder.session_id(),
            PalwCourtSessionStateV2 {
                claim,
                challenger_bond: accuser,
                opened_daa: at,
                deadline_daa: at + wc,
                ladder,
                dissection: None,
            },
        );
    });
    let held = palw_accuser_exposure_v1(&c.s, &accuser);
    assert_eq!(held, record.reserved, "the accuser ledger");
    assert_eq!(c.s.reserved_exposure(&accuser), 0, "A-6: nothing in reserved_exposure");

    // One own attempt's commitment, read off the fold on the unsqueezed bond.
    let (env, key, id1) = floor_attempt(&c, 2, 0xB111);
    let at = c.daa + 1;
    let ctx_at = PalwBlockContextV2 { block: h(0xB100_0000 + at), daa_score: at, blue_score: at, subsidy: T12_BLOCK_SUBSIDY_SOMPI };
    let probe = c.try_fold(&c.s, &ctx_at, &[], PalwBlockWorkV3::Attempt(&env), key).expect("admitted unsqueezed").0;
    let a = committed_of(&c, &probe, &accuser, at);
    assert!(a > 0);
    // The slash.
    let squeezed = u64::try_from(held + a + a / 2).unwrap();
    assert!(squeezed >= c.sp.min_collateral_sompi(), "the premise: above the producer floor");
    assert!(u128::from(squeezed) * u128::from(c.sp.fp_max_exposure_ratio_permille()) / 1000 >= 2 * a, "500‰ alone admits two");
    c.s = with_collateral(&c.sp, &c.s, accuser, squeezed);

    // The first own attempt fits.
    c.step_at(at, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    assert!(c.s.claim(&id1).is_some(), "0 + accuser + a ≤ C");
    let committed = committed_of(&c, &c.s, &accuser, c.daa);
    assert_eq!(committed, a);
    assert!(committed + held <= u128::from(squeezed), "the invariant");

    // The second does not, and admission, PROD v4 and the fold agree.
    let (env, key, id2) = floor_attempt(&c, 2, 0xB112);
    let at = c.daa + 1;
    let ctx_at = PalwBlockContextV2 { block: h(0xB100_0000 + at), daa_score: at, blue_score: at, subsidy: T12_BLOCK_SUBSIDY_SOMPI };
    let b = bundle(&p);
    match check_palw_attempt_admission_v2(&c.s, &c.sp, &b.admission, &ctx_at, &env, fences(&p, at)) {
        Err(PalwAdmissionV2Error::ExposureCeilingExceeded { reserved, ceiling, .. }) => {
            assert_eq!(reserved, committed, "admission names the one ledger");
            assert_eq!(ceiling, u128::from(squeezed) - held, "…and the invariant's room, not the 500‰ ceiling");
        }
        other => panic!("admission must refuse the second attempt on the accuser ledger: {other:?}"),
    }
    let (floor, _, _, _) = genesis_classes(&p)[0];
    let escrow = palw_claim_escrow_v1(&c.sp, T12_BLOCK_SUBSIDY_SOMPI, extras(&p, at).escrow_carve);
    let facts = palw_producer_facts_v4(
        &c.s,
        &c.sp,
        &b.admission,
        kaspa_consensus_core::BlockHash::from_u64_word(1),
        at,
        floor,
        Some(&accuser),
        None,
        p.palw_canonical_work_daa(),
        fences(&p, at).base_known_draw,
        true,
        escrow,
        raw_depth(&p, at),
    )
    .expect("the floor has facts");
    let bond_facts = facts.bond.expect("bond facts");
    assert_eq!(bond_facts.accuser_exposure, held, "PROD v4 carries the accuser ledger");
    assert!(bond_facts.committed + bond_facts.claim_exposure <= bond_facts.exposure_ceiling, "the 500‰ test alone passes");
    assert!(!bond_facts.has_committed_room(), "PROD v4 predicts the refusal");
    let (next, _, skips) = c.try_fold(&c.s, &ctx_at, &[], PalwBlockWorkV3::Attempt(&env), key).expect("the block stands");
    assert!(next.claim(&id2).is_none() && skips.len() == 1, "the fold skips the own attempt: {skips:?}");

    // The DA arm: an accusation by the same bond stakes on the free half, which is gone.
    let da = PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index: 0, accuser, signature: Vec::new() };
    let refused = c.try_fold(&c.s, &ctx_at, std::slice::from_ref(&da), PalwBlockWorkV3::None, Hash64::default());
    assert!(
        matches!(refused, Err(PalwStateV2Error::AccusationExposureCeiling { bond, .. }) if bond == accuser),
        "the DA arm is refused on the free half: {refused:?}"
    );
    let restored = with_collateral(&c.sp, &c.s, accuser, RICH);
    let (after, _, _) = c
        .try_fold(&restored, &ctx_at, std::slice::from_ref(&da), PalwBlockWorkV3::None, Hash64::default())
        .expect("admitted with room");
    let total = committed_of(&c, &after, &accuser, at) + palw_accuser_exposure_v1(&after, &accuser);
    assert!(total <= u128::from(RICH), "the invariant after the DA stake");
    // M3 has moved the DA stake (this assertion's interim `reserved_exposure` reading said "until M3
    // moves it"): it is the session's own exposure, on the ACCUSER ledger (DA-6, A-6) — `committed` is
    // unchanged and the accuser ledger grows by exactly the session's exposure.
    let session = after.da_session(&claim, &accuser).expect("the accusation opened a session");
    assert!(session.exposure > 0);
    assert_eq!(committed_of(&c, &after, &accuser, at), committed, "the DA stake is not in committed");
    assert_eq!(palw_accuser_exposure_v1(&after, &accuser), held + session.exposure, "it is on the accuser ledger");
    println!(
        "M1: a {:.4} MSK, accuser {:.4} MSK, slashed to C {:.4} MSK: second attempt refused; DA refused, admitted with room",
        a as f64 / MSK as f64,
        held as f64 / MSK as f64,
        squeezed as f64 / MSK as f64
    );
}

/// **L4: the capability declaration reads the one ledger and the accuser ledger at 100%.** Bond 2
/// holds a court (accuser ledger, outside `reserved_exposure` past the fence) and nothing else; at
/// `C = committed + accuser + price` one declared class fits, at one sompi less it is refused naming
/// the backing `committed + accuser` — which `reserved + registration` alone would not have seen.
#[test]
fn l4_the_capability_declaration_reads_the_one_ledger() {
    let p = t12();
    let mut c = Chain::new(p.clone());
    let accuser = bond_key(2);
    let claim = licensed_floor_claim(&mut c, 0xB401);
    c.step(&[bond_obj(2, c.sp.min_collateral_sompi() * 4)]);
    let court = court_on(&c, claim, accuser);
    c.step(&[court]);
    let at = c.daa + 1;
    let backing = committed_of(&c, &c.s, &accuser, at) + palw_accuser_exposure_v1(&c.s, &accuser);
    assert!(backing > 0 && c.s.reserved_exposure(&accuser) + c.s.registration_exposure(&accuser) < backing, "the premise");
    let (floor, _, _, _) = genesis_classes(&p)[0];
    let declare = PalwConsensusObjectV2::BondCapabilityDeclared {
        bond: accuser,
        capable_classes: [floor].into_iter().collect(),
        signature: vec![1],
    };
    let price = u128::from(kaspa_consensus_core::palw_state_v2::PALW_CAPABILITY_EXPOSURE_SOMPI);
    let ctx_at = PalwBlockContextV2 { block: h(0xB400_0000 + at), daa_score: at, blue_score: at, subsidy: 0 };
    for (collateral, fits) in [(backing + price, true), (backing + price - 1, false)] {
        let s = with_collateral(&c.sp, &c.s, accuser, u64::try_from(collateral).unwrap());
        let result = c.try_fold(&s, &ctx_at, std::slice::from_ref(&declare), PalwBlockWorkV3::None, Hash64::default());
        if fits {
            result.expect("committed + accuser + price = C fits");
        } else {
            match result {
                Err(PalwStateV2Error::CapabilityExposureUnaffordable { already, .. }) => {
                    assert_eq!(already, backing, "the declaration names committed + accuser")
                }
                other => panic!("one sompi short must be refused: {other:?}"),
            }
        }
    }
}

/// **L5: a lock whose claim outlives its clocks stays committed, and the re-date at Final moves
/// nothing.** A seat's `lock_3` is written at the licence with `expiry = max(licence, H) + window_court`
/// (ADR-0152 §4-quater V5: a floor claim licensed one DAA after its bind is still inside its 10-DAA
/// verification deadline, so its lock is dated from `H = bound + 11`, restated from `licence +
/// window_court` when the fence landed; the checkpoints below read the lock's own expiry). A
/// court whose turns keep it open past that — the carriage stands in for the played rungs: a session
/// with its backstop at `licence + 4·window_court` and its next rung after it — holds the claim
/// licensed past the DAA expiry and past the second clock's bound (`expiry + 2·window_court`). The
/// seat's committed ledger is unchanged across both, and across the Final that re-dates the lock (the
/// duty released, the lock alone): `max(duty, lock)` is `lock` throughout. The exit gate holds the
/// seat the whole time.
#[test]
fn l5_a_lock_outliving_its_clocks_is_still_committed_and_the_final_moves_nothing() {
    let p = t12();
    let mut c = Chain::new(p.clone());
    c.step(&[bond_obj(1, RICH)]);
    let claim = licensed_floor_claim(&mut c, 0xB501);
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = c.claim(&claim).phase else { unreachable!() };
    let seat = c.floor_seats()[0].0;
    let lock = *c.s.slashable_lock(seat, claim).expect("the seat's lock");
    let wc = c.sp.window_court();
    let horizon = kaspa_consensus_core::palw_state_v2::palw_claim_verify_horizon_v1(&c.s, &c.sp, &claim, &c.claim(&claim))
        .expect("testnet-12 arms the class-verify-deadline fence");
    assert_eq!(
        lock.expiry_daa,
        licensed_daa.max(horizon) + wc,
        "the premise: the lock's DAA clock is the licence's, floored at the verification horizon (V5)"
    );
    let duty = c.s.panel_duty_row_of(&claim).expect("duty row").seat_exposure;
    assert!(lock.amount > duty, "the premise: the lock tops the duty up (the whole-gain price)");
    // The court that outlives the lock's clocks.
    let record = c.claim(&claim);
    let challenger = bond_key(1);
    let keep_until = licensed_daa + 4 * wc;
    let at = c.daa;
    let ladder = kaspa_consensus_core::palw_bisect::PalwBisectLadderV1::open(
        &claim,
        &record.trace_root,
        &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&challenger),
        &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&record.bond),
        kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
        16,
        at,
        keep_until + 1,
    )
    .expect("a ladder");
    let session = ladder.session_id();
    c.s = edited(&c.sp, &c.s, |k| {
        k.court_sessions.insert(
            session,
            PalwCourtSessionStateV2 {
                claim,
                challenger_bond: challenger,
                opened_daa: at,
                deadline_daa: keep_until,
                ladder,
                dissection: None,
            },
        );
    });
    let delay = c.sp.withdrawal_delay_daa();
    let before = committed_of(&c, &c.s, &seat, c.daa);
    assert_eq!(before - c.s.reserved_exposure(&seat), lock.amount - duty, "the lock's excess over its duty");
    for at in [lock.expiry_daa + 1, licensed_daa + 3 * wc] {
        c.step_at(at, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
        assert!(matches!(c.claim(&claim).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the court holds it at {at}");
        let escaped =
            kaspa_consensus_core::palw_state_v2::palw_second_clock_depth_v1(raw_depth(&p, at), c.s.recent_anchor_daas(), at, wc);
        let live = c.s.slashable_lock(seat, claim).unwrap().is_live_v3(at, c.s.settled_attempt_finals(), escaped, wc);
        assert!(at < lock.expiry_daa + 2 * wc || !live, "the premise: by {at} both clocks have released it");
        assert_eq!(committed_of(&c, &c.s, &seat, at), before, "at {at}: still committed while the claim lives");
        let bond = c.s.bond(&seat).unwrap().clone();
        assert!(palw_bond_collateral_is_locked_v6(&c.s, &c.sp, &seat, &bond, at, delay, raw_depth(&p, at), true), "B-3 holds it");
    }
    // The backstop ends the court on the challenger's side; the claim goes Final and the lock is
    // re-dated there.
    let mut at = keep_until;
    while !matches!(c.claim(&claim).phase, PalwClaimPhaseV2::Final { .. }) {
        c.step_at(at, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
        at = c.daa + c.sp.window_challenge_at(c.daa).max(1);
        assert!(at < keep_until + 10 * wc, "the claim reaches Final");
    }
    let redated = *c.s.slashable_lock(seat, claim).expect("the lock outlives the Final");
    assert_eq!(redated.expiry_daa, c.daa + wc, "the liability begins again at the Final");
    assert_eq!(c.s.panel_duty_row_of(&claim).map(|row| row.seat_exposure).unwrap_or(0), 0, "the duty is released");
    assert_eq!(committed_of(&c, &c.s, &seat, c.daa), before, "the re-date moved nothing: max(duty, lock) = lock throughout");
}

/// **The S re-review of 0b56c4d8: `committed + accuser ≤ C` across interleavings** (the reviewer's
/// probe `review_s2_accuser`, made a test). A 65,000 MSK bond accuses a licensed 2M claim and works in
/// either order — (1) the DA accusation, then its own floor attempts until the fold skips one; (2) the
/// attempts first, then the accusation — and after every step the one invariant holds, and the room
/// the accuser gate still offers keeps it (`committed + accuser + room ≤ C`).
#[test]
fn m1_the_invariant_holds_across_interleavings() {
    // ADR-0152 §4-quater (U-D1): the 2M row is closed at launch, so a live 2M claim exists only past the flag day
    // that installs its measured row — this test's premise runs there (`t12_2m_open`, measuring the derived
    // 13,995-DAA deadline).
    let p = t12_2m_open();
    let (_, id2m) = model_classes(&p);
    let total = |c: &Chain, b: &PalwBondKeyV2, at: u64| {
        (kaspa_consensus_core::palw_state_v2::palw_bond_committed_v1(&c.s, b, at, None, c.sp.window_court()), palw_accuser_exposure_v1(&c.s, b), u128::from(c.s.bond(b).unwrap().collateral))
    };
    let work_until_refused = |c: &mut Chain, seed0: u64| -> u64 {
        let mut taken = 0;
        for i in 0..40u64 {
            let (env, key, id) = floor_attempt(c, 2, seed0 + i);
            let daa = c.daa + 1;
            let at = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                block: h(0xCA_0000 + daa),
                daa_score: daa,
                blue_score: daa,
                subsidy: T12_BLOCK_SUBSIDY_SOMPI,
            };
            let (_, _, skips) = c.try_fold(&c.s.clone(), &at, &[], PalwBlockWorkV3::Attempt(&env), key).expect("the block stands");
            if !skips.is_empty() {
                break;
            }
            c.step_at(daa, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
            assert!(c.s.claim(&id).is_some());
            taken += 1;
        }
        taken
    };
    let accuse = |c: &mut Chain, claim: Hash64, accuser: PalwBondKeyV2| -> bool {
        let object = PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index: 0, accuser, signature: Vec::new() };
        let daa = c.daa + 1;
        let at = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 { block: h(0xDA00_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 0 };
        if c.try_fold(&c.s.clone(), &at, std::slice::from_ref(&object), PalwBlockWorkV3::None, Hash64::default()).is_err() {
            return false;
        }
        c.step_at(daa, &[object], PalwBlockWorkV3::None, Hash64::default(), 0);
        true
    };
    for order in [1u8, 2] {
        let mut c = model_chain(p.clone(), id2m, 1);
        let id = model_claim(&mut c, id2m, 1, 0xDA01 + u64::from(order));
        let seats = honest_seats(&c.p, 5);
        c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
        let bound = c.bind(id, &seats);
        c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
        c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect() }]);
        c.step(&[bond_obj(2, 65_000 * MSK as u64)]);
        let accuser = bond_key(2);
        let holds = |c: &Chain, what: &str| {
            let (committed, accused, collateral) = total(c, &accuser, c.daa);
            assert!(committed + accused <= collateral, "order {order}, {what}: {committed} + {accused} > {collateral}");
        };
        let (taken, accepted) = if order == 1 {
            let accepted = accuse(&mut c, id, accuser);
            holds(&c, "after the accusation");
            (work_until_refused(&mut c, 0xB100), accepted)
        } else {
            let taken = work_until_refused(&mut c, 0xB200);
            holds(&c, "after the work");
            (taken, accuse(&mut c, id, accuser))
        };
        holds(&c, "at the end");
        let room = palw_accuser_room_v1(&c.s, &c.sp, &accuser, c.daa + 1, None);
        let (committed, accused, collateral) = total(&c, &accuser, c.daa + 1);
        assert!(committed + accused + room <= collateral, "order {order}: the accuser room keeps the invariant");
        println!("order {order}: {taken} own attempts, accusation accepted: {accepted}, accuser room left {room}");
    }
}
