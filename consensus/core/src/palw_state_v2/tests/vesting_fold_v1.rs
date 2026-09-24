//! **ADR-0152 V-1…V-8 through the fold** (the vesting work — B, in S's window; testnet-12 only).
//!
//! Named by the ADR's test ids where one exists (§8.1): T03 (the fold half of the coinbase
//! identity), T04, T11, T12, T13, T16/T30, T37, T41, T43, T44, T47, T82, plus the reorg twin (a block
//! that latches, moves and burns, reverted) and the fence-off twins (`palw_rcore_plus` unarmed: no
//! row is ever written and the payouts are exactly the pre-fence ones).
//!
//! Two harnesses. The ECONOMY harness (`economy_bound`, ADR-0124's) runs a real claim through
//! binding, licence and `Final` with the R-core+ mirror set on the state params, so `finalize_claim`
//! writes the row and step 3d moves it. The SEEDED harness installs rows through a `Vesting` delta
//! (so the derived indexes are built by the delta path itself) and sets the two clocks directly,
//! which is how the second clock, the halt and the backlog are driven without a licence lattice.

use super::*;
use crate::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use crate::palw_offence_v1::PalwOffenceKindV1;
use crate::palw_vesting_v1::*;

// ---- harness ----------------------------------------------------------------------------------

/// The economy fixture's params with `Params::palw_rcore_plus` mirrored from genesis — the fold's
/// only reading of the fence.
fn vp() -> PalwStateParamsV2 {
    params().with_worker_carve_permille(620).unwrap().with_rcore_plus_mirrors(Some(0), 0, Vec::new())
}

fn payload(n: u64) -> Hash64 {
    Hash64::from_u64_word(0x9A00 + n)
}

fn market_key(i: u64) -> Hash64 {
    let mut bytes = h64(0x4D00_0000 + i).as_bytes();
    bytes[0] = PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX;
    Hash64::from_bytes(bytes)
}

/// A hand-built row: the producer 500, each seat 100 (50 past five seats), the rest the reserve,
/// out of an escrow of 1,000.
fn row(claim_id: Hash64, producer: u64, seats: &[u64], expiry_daa: u64, settled_at_final: u64) -> PalwVestingRowV1 {
    let per_seat = if seats.len() > 5 { 50 } else { 100 };
    PalwVestingRowV1 {
        claim_id,
        producer_bond: bond_key(producer),
        class_id: h64(1),
        execution_root: h64(0xE0),
        artifact_root: h64(11),
        job_identity: Hash64::default(),
        free_prompt: false,
        trace_root: Hash64::default(),
        segment_count: 0,
        licence_door: PalwLicenceDoorTagV1::Quorum,
        basis_k: 0,
        escrowed_reward: 1_000,
        buyback_bound: 0,
        producer: PalwPayoutV2 { payload: payload(producer), amount: 500 },
        seats: seats.iter().map(|s| (bond_key(*s), PalwPayoutV2 { payload: payload(*s), amount: per_seat })).collect(),
        reserve: 500 - per_seat * seats.len() as u64,
        final_daa: expiry_daa.saturating_sub(500),
        expiry_daa,
        settled_at_final,
        matured_at: None,
    }
}

/// `rows` installed through one `Vesting` delta (the delta path builds the indexes), with the
/// counters' `created` equal to their sum.
fn seeded(rows: &[PalwVestingRowV1]) -> PalwChainStateV2 {
    let mut entries: Vec<PalwDeltaEntryV2> =
        rows.iter().map(|r| PalwDeltaEntryV2::Vesting { key: r.claim_id, old: None, new: Some(r.clone()) }).collect();
    let created: u128 = rows.iter().map(PalwVestingRowV1::total_sompi_u128).sum();
    entries.push(PalwDeltaEntryV2::VestingCounters {
        old: PalwVestingCountersV1::default(),
        new: PalwVestingCountersV1 { created, ..Default::default() },
    });
    apply_delta_v2(&PalwChainStateV2::genesis(), &PalwStateDeltaV2 { point: ctx(1, 1, 1), entries }, &vp()).unwrap()
}

/// The second clock armed at `depth` (the fold reads the raw depth only past the audit fence).
fn two_clock(depth: u64) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 { audit_2026_09_23_active: true, settled_anchor_depth: Some(depth), ..Default::default() }
}

/// The four derived vesting indexes: the move order, the unlatched rows by expiry and by settled
/// count, and the payees.
type VestingIndexes =
    (BTreeSet<(u64, Hash64)>, BTreeSet<(u64, Hash64)>, BTreeSet<(u64, Hash64)>, BTreeSet<(PalwBondKeyV2, bool, u64, Hash64)>);

fn indexes(s: &PalwChainStateV2) -> VestingIndexes {
    (s.vesting_order.clone(), s.vesting_unlatched.clone(), s.vesting_unlatched_by_settled.clone(), s.vesting_payees.clone())
}

/// Everything a vesting block must satisfy: internal consistency (the indexes against the rows),
/// V-3's consistency, the delta reproducing the child and reverting to the parent — roots AND the
/// derived indexes — and the carriage re-importing under the committed root with the indexes
/// rebuilt identically.
fn checked(parent: &PalwChainStateV2, child: &PalwChainStateV2, delta: &PalwStateDeltaV2, p: &PalwStateParamsV2) {
    child.assert_internal_consistency(p).expect("internal consistency (the vesting indexes included)");
    palw_vesting_consistency_v1(child).expect("V-3 consistency");
    let applied = apply_delta_v2(parent, delta, p).unwrap();
    assert_eq!(applied.state_root(), child.state_root(), "the delta reproduces the fold");
    assert_eq!(indexes(&applied), indexes(child), "and its vesting indexes");
    let back = revert_delta_v2(child, delta, p).unwrap();
    assert_eq!(back.state_root(), parent.state_root(), "the delta reverts to the parent");
    assert_eq!(indexes(&back), indexes(parent), "and its vesting indexes");
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(child)).unwrap();
    let imported = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(p, Some(child.state_root())).unwrap();
    assert_eq!(indexes(&imported), indexes(child), "import rebuilds the indexes");
    assert_eq!(imported.vesting, child.vesting);
}

/// One empty block at `daa` on `parent`, checked.
fn fold(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    daa: u64,
    extras: &PalwTransitionExtrasV1,
) -> (PalwChainStateV2, PalwStateDeltaV2) {
    let blue = parent.last_point.map(|point| point.blue_score + 1).unwrap_or(2);
    fold_objects(parent, p, &ctx(blue, daa, blue), &[], None, extras)
}

/// One block carrying `objects` (and an attempt) under `extras`, checked.
fn fold_objects(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    c: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    att: Option<&PalwAttemptEnvelopeV2>,
    extras: &PalwTransitionExtrasV1,
) -> (PalwChainStateV2, PalwStateDeltaV2) {
    let (child, delta) = apply_palw_transition_v2_with_extras(parent, p, c, objects, att, false, false, false, false, extras)
        .expect("the fold applies");
    checked(parent, &child, &delta, p);
    (child, delta)
}

/// One empty block, unchecked beyond V-3 — for the long backlog simulations.
fn fold_fast(parent: &PalwChainStateV2, p: &PalwStateParamsV2, daa: u64) -> (PalwChainStateV2, PalwStateDeltaV2) {
    let blue = parent.last_point.map(|point| point.blue_score + 1).unwrap_or(2);
    let (child, delta) = apply_palw_transition_v2_with_extras(
        parent,
        p,
        &ctx(blue, daa, blue),
        &[],
        None,
        false,
        false,
        false,
        false,
        &PalwTransitionExtrasV1::default(),
    )
    .expect("the fold applies");
    palw_vesting_counters_consistent_v1(&child).expect("V-3 consistency");
    (child, delta)
}

fn notes(delta: &PalwStateDeltaV2) -> Vec<PalwVestingNoteV1> {
    palw_vesting_notes_of_delta_v1(delta).cloned().collect()
}

fn moved_rows(delta: &PalwStateDeltaV2) -> usize {
    notes(delta).iter().filter(|n| matches!(n, PalwVestingNoteV1::Moved { source: PalwVestingSourceV1::Row { .. }, .. })).count()
}

fn non_market(s: &PalwChainStateV2) -> usize {
    palw_vesting_non_market_rows_waiting_v1(s)
}

/// The economy claim licensed by all three seats at DAA 103 under `p`, one sweep short of `Final`.
///
/// Three, not two: past `palw_rcore_plus` S-3's quorum door licenses only on
/// `PALW_PANEL_COLLUDING_QUORUM_V1` (3) BACKED `Valid` signers (SR-6, `license_rcore_v1`), and a
/// set short of it is inert — the claim stays `PanelBound`. Each of the three backs its lock
/// (`lock_3` = 94 on a 1,000-sompi bond, committed 66–266, under the 500‰ ceiling).
fn economy_licensed(p: &PalwStateParamsV2) -> (PalwChainStateV2, Hash64) {
    let (s3, claim_id) = economy_bound(p);
    let receipts: Vec<_> = (1..=3u64).map(|n| receipt_at(claim_id, bond_key(n), true, 103)).collect();
    let (s4, _) =
        apply_economy(&s3, p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts }], None);
    (s4, claim_id)
}

/// The economy claim finalized at DAA 124 under `p` (licensed by its three seats at 103).
fn economy_final(p: &PalwStateParamsV2) -> (PalwChainStateV2, PalwStateDeltaV2, Hash64) {
    let (s4, claim_id) = economy_licensed(p);
    let (s5, d5) = apply_economy(&s4, p, &ctx(5, 124, 5), &[], None);
    assert!(matches!(s5.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { final_daa: 124 }));
    (s5, d5, claim_id)
}

// ---- V-2 / T13: what vests ------------------------------------------------------------------

/// **T13 / V-2: a Final names a row, not a payout.** ADR-0124's claim (620 escrowed; producer 496,
/// three credited seats 41 each, the reserve 1 — S-3's quorum door licenses on all three) finalizes
/// past `palw_rcore_plus`: nothing enters
/// `pending_payouts`, the reserve is not credited, and one row holds exactly those amounts — the
/// producer's under its A-KEY key, each credited seat's under its payee key, the reserve — with
/// the lock's two clocks (`final + window_court`, the settled count at Final). The row is the
/// claim's liability's twin: the door `Quorum` and `basis_k` 3 that S-3's licence recorded on the
/// claim (the recount of its three backed signers), no attribution recorded (objective offences are dormant in this fixture, so there is no
/// liability record to copy from).
#[test]
fn t13_a_final_names_a_row_with_the_payout_amounts_and_writes_no_payout() {
    let p = vp();
    let (s5, d5, claim_id) = economy_final(&p);
    assert!(s5.pending_payouts_iter().next().is_none(), "no payout is named at Final");
    assert_eq!(s5.panel_reserve_sompi(), 0, "the reserve vests with the row");
    assert!(!d5.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::Payout { .. } | PalwDeltaEntryV2::PanelReserve { .. })));
    let row = s5.vesting_row(&claim_id).expect("one row per Final claim").clone();
    assert_eq!(row.producer, PalwPayoutV2 { payload: Hash64::from_u64_word(0x9A11), amount: 496 });
    assert_eq!(
        row.seats,
        vec![
            (bond_key(1), PalwPayoutV2 { payload: Hash64::from_u64_word(0x9A11), amount: 41 }),
            (bond_key(2), PalwPayoutV2 { payload: payload(2), amount: 41 }),
            (bond_key(3), PalwPayoutV2 { payload: payload(3), amount: 41 }),
        ],
        "each credited seat, in seat order"
    );
    assert_eq!(row.reserve, 1);
    assert_eq!((row.escrowed_reward, row.buyback_bound, row.total_sompi()), (620, 0, 620), "the escrow, whole, and no buyback");
    // The lock's second clock: the settled count the claim's liability began at. Below the audit
    // fence (this fixture) the Final settles its own anchor after the row and the lock are written,
    // so the row records the count before it; past the fence (t12) the anchor ticks at the licence.
    assert_eq!((row.final_daa, row.expiry_daa, row.matured_at), (124, 624, None));
    assert_eq!(row.settled_at_final + 1, s5.settled_attempt_finals());
    assert_eq!((row.class_id, row.artifact_root, row.producer_bond), (h64(1), h64(11), bond_key(1)));
    assert_eq!((row.licence_door, row.basis_k, row.job_identity), (PalwLicenceDoorTagV1::Quorum, 3, Hash64::default()));
    assert_eq!(s5.vesting_counters(), PalwVestingCountersV1 { created: 620, moved: 0, burned: 0 });
    let legs: Vec<_> = row.legs().collect();
    assert_eq!(legs.len(), 5, "producer, three seats, the reserve");
    assert_eq!(legs[0].queue_key, Some(palw_vesting_payout_key_v1(&claim_id)), "A-KEY, never the raw claim id");
    assert_eq!(legs[1].queue_key, Some(palw_panel_payout_key_v1(&Hash64::from_u64_word(0x9A11))));
    assert_eq!((legs[4].kind, legs[4].queue_key, legs[4].amount), (PalwVestingLegKindV1::Reserve, None, 1));
    assert_eq!(row.leg_count(), 4, "the reserve is not a leg");
    // The journal: the row and the counters, and no buyback note (no pair took a slice).
    assert!(d5.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::Vesting { key, old: None, new: Some(_) } if *key == claim_id)));
    assert!(d5.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::VestingCounters { .. })));
    assert!(notes(&d5).is_empty());
    // The row is keyed by payee for B-3: bond 1 (producer and seat) once, and each other seat.
    assert_eq!(s5.vesting_rows_of_payee(&bond_key(1)).count(), 1);
    assert_eq!(s5.vesting_rows_of_payee(&bond_key(2)).count(), 1);
    assert_eq!(s5.vesting_rows_of_payee(&bond_key(3)).count(), 1);
    assert_eq!(s5.vesting_rows_of_payee(&bond_key(4)).count(), 0, "a bond off the panel pays nothing");
}

/// **T13 / V-2: the work-price remainder is never vested.** ADR-0124 Decision 6's lighter class
/// is paid 40/400 of its 620 escrow (producer 50, each of its three seats 4): its row holds 62 and
/// records the 620 it withheld; the 558 in between is named nowhere — neither row, nor payout, nor
/// reserve. The floor's claim in the same sweep vests whole.
#[test]
fn t13_the_work_price_remainder_is_named_nowhere() {
    let (fin, floor_id, light_id) = two_class_finals(&vp());
    let floor = fin.vesting_row(&floor_id).unwrap();
    let light = fin.vesting_row(&light_id).unwrap();
    assert_eq!((floor.total_sompi(), floor.producer.amount), (620, 496));
    assert_eq!((light.total_sompi(), light.producer.amount, light.seats[0].1.amount), (62, 50, 4));
    assert_eq!(light.escrowed_reward, 620, "the row records what was withheld");
    assert!(fin.pending_payouts_iter().next().is_none());
    assert_eq!(fin.vesting_counters().created, 620 + 62, "the remainder is not created");
}

/// ADR-0124 Decision 6's two-class fixture (a floor claim and a class-3 claim priced 40/400), both
/// finalized at DAA 140 under `p`. Each is judged by seats 2, 3 and 4, all three signing: past
/// `palw_rcore_plus` S-3's quorum door needs `PALW_PANEL_COLLUDING_QUORUM_V1` (3) backed `Valid`
/// signers (SR-6), so the one-seat panel this fixture was first written with never licenses there.
fn two_class_finals(p: &PalwStateParamsV2) -> (PalwChainStateV2, Hash64, Hash64) {
    let genesis = PalwChainStateV2::genesis();
    let mut objects = register_class_and_bond();
    for n in 2..=4u64 {
        objects.push(PalwConsensusObjectV2::BondRegistered {
            bond: bond_key(n),
            pubkey: vec![7, n as u8],
            operator_pubkey: op_key(20 + n),
            collateral: 1_000,
            payout_payload: payload(n),
            capable_classes: Default::default(),
            signature: Vec::new(),
        });
    }
    for (class, cap, share) in [(2u64, 400u64, 300u16), (3, 160, 200)] {
        objects.push(PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(class),
            artifact_root: h64(10 + class),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(cap),
            initial_target: u128::MAX / 2,
            share_permille: share,
            activation_daa: 0,
            admission: None,
        });
    }
    let (s1, _) = apply_economy(&genesis, p, &ctx(1, 100, 1), &objects, None);
    let on_floor = attempt(40, 1);
    let on_light = attempt_for_class(40, 2, h64(3), bond_key(1), vec![7; 4], op_id(21), h64(13));
    let floor_id = attempt_id_v2(&on_floor.attempt);
    let light_id = attempt_id_v2(&on_light.attempt);
    let (s2, _) = apply_economy(&s1, p, &PalwBlockContextV2 { subsidy: 1_000, ..ctx(2, 101, 2) }, &[], Some(&on_floor));
    let (s3, _) = apply_economy(&s2, p, &PalwBlockContextV2 { subsidy: 1_000, ..ctx(3, 102, 3) }, &[], Some(&on_light));
    let mut s = s3;
    for (n, claim_id) in [(4u64, floor_id), (5, light_id)] {
        let (bound, _) = apply_economy(
            &s,
            p,
            &ctx(n * 10, 100 + n, n * 10),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(70 + n), seats: (2..=4).map(seat_n).collect() }],
            None,
        );
        let receipts: Vec<_> = (2..=4u64).map(|seat| receipt_at(claim_id, bond_key(seat), true, 101 + n)).collect();
        let (licensed, _) = apply_economy(
            &bound,
            p,
            &ctx(n * 10 + 1, 101 + n, n * 10 + 1),
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts }],
            None,
        );
        s = licensed;
    }
    let (fin, _) = apply_economy(&s, p, &ctx(90, 140, 90), &[], None);
    (fin, floor_id, light_id)
}

/// ADR-0091's lattice (the `model_market` module's `finalized_claim`, past `palw_rcore_plus`): the
/// founding line seeded, one attempt at a 1,000 subsidy (620 escrowed), a panel of three (the
/// producer and bonds 2 and 3, S-3's quorum of backed signers) bound below the panel-economy fence —
/// so the claim has NO duty row — licensed by all three at 104 and `Final` at 125. Returns the
/// state before and at `Final`, the `Final` block's delta, the claim id and the extras every block
/// ran under.
fn buyback_final(p: &PalwStateParamsV2) -> (PalwChainStateV2, PalwChainStateV2, PalwStateDeltaV2, Hash64, PalwTransitionExtrasV1) {
    let extras = PalwTransitionExtrasV1 { model_lines_active: true, ..Default::default() };
    let class = h64(1);
    // Seats 2 and 3 beside the producer: S-3's quorum door licenses on three backed signers.
    let mut registry = register_class_and_bond();
    for n in 2..=3u64 {
        registry.push(PalwConsensusObjectV2::BondRegistered {
            bond: bond_key(n),
            pubkey: vec![7, n as u8],
            operator_pubkey: op_key(20 + n),
            collateral: 1_000,
            payout_payload: payload(n),
            capable_classes: Default::default(),
            signature: Vec::new(),
        });
    }
    let (s1, _) = fold_objects(&PalwChainStateV2::genesis(), p, &ctx(1, 100, 1), &registry, None, &extras);
    let seed = PalwConsensusObjectV2::ModelSeed {
        line_id: class,
        seeder: Hash64::from_u64_word(0xB0_0009),
        msk_seed: crate::palw_model_market_v1::PALW_MODEL_SEED_MIN_SOMPI_V1,
        sink_index: 1,
    };
    let (s1, _) = fold_objects(&s1, p, &ctx(2, 101, 2), &[seed], None, &extras);
    let env = attempt(40, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let (s2, _) = fold_objects(&s1, p, &PalwBlockContextV2 { subsidy: 1_000, ..ctx(3, 102, 3) }, &[], Some(&env), &extras);
    let seats = (1..=3).map(seat_n).collect();
    let bound = PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats };
    let (s3, _) = fold_objects(&s2, p, &ctx(4, 103, 4), &[bound], None, &extras);
    let receipts = (1..=3u64).map(|n| receipt_at(claim_id, bond_key(n), true, 104)).collect();
    let licensed = PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts };
    let (s4, _) = fold_objects(&s3, p, &ctx(5, 104, 5), &[licensed], None, &extras);
    let (s5, d5) = fold_objects(&s4, p, &PalwBlockContextV2 { subsidy: 9_999_999, ..ctx(6, 125, 6) }, &[], None, &extras);
    assert!(matches!(s5.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    (s4, s5, d5, claim_id, extras)
}

/// **T13 / T03 / V-2–V-3: a Final that buys the pair vests the rest and names its slice** (review
/// of the vesting work, finding 6 — the buyback term and the no-duty-row arm, never emitted by the
/// fold before). ADR-0091's slice (31 of 620) still executes at `Final`: the pair's reserve takes
/// it at once, as below the fence, and the row records it as `buyback_bound` beside a
/// `BuybackAtFinal` note. The 589 left vests whole in the producer's leg — the `None if vests`
/// arm: no seat, no reserve. At the row's expiry it moves, the next block mints it, and T03 closes
/// with a non-zero buyback term.
#[test]
fn t03_t13_a_buyback_final_vests_the_rest_and_names_its_slice() {
    let p = vp();
    let (s4, s5, d5, claim_id, extras) = buyback_final(&p);
    assert!(!s4.panel_duties.contains_key(&claim_id), "no duty row: the whole reward is the producer's");
    assert!(s5.pending_payouts_iter().next().is_none(), "nothing is named for the coinbase at Final");
    let row = s5.vesting_row(&claim_id).expect("one row").clone();
    assert_eq!(row.producer, PalwPayoutV2 { payload: Hash64::from_u64_word(0x9A11), amount: 589 });
    assert!(row.seats.is_empty());
    assert_eq!((row.reserve, row.escrowed_reward, row.buyback_bound, row.final_daa), (0, 620, 31, 125));
    assert_eq!(notes(&d5), vec![PalwVestingNoteV1::BuybackAtFinal { claim_id, sompi: 31 }]);
    let market = s5.model_market(&h64(1)).expect("the pair");
    assert_eq!(
        market.msk_reserve,
        crate::palw_model_market_v1::PALW_MODEL_SEED_MIN_SOMPI_V1 + 31,
        "the slice left at Final, into the pair — it is not vested"
    );
    palw_vesting_consistency_v1(&s5).expect("V-3: 589 ≤ 620 − 31");

    // T03 over the claim's life, from the deltas alone.
    let withheld = s5.claim(&claim_id).unwrap().escrowed_reward as u128;
    let buyback: u128 = notes(&d5)
        .iter()
        .map(|note| match note {
            PalwVestingNoteV1::BuybackAtFinal { sompi, .. } => *sompi as u128,
            _ => 0,
        })
        .sum();
    let unnamed = (row.escrowed_reward - row.buyback_bound) as u128 - row.total_sompi_u128();
    let (quiet, quiet_delta) = fold(&s5, &p, row.expiry_daa - 1, &extras);
    assert!(notes(&quiet_delta).is_empty(), "nothing matures before the DAA clock runs out");
    let (moved, moved_delta) = fold(&quiet, &p, row.expiry_daa, &extras);
    let minted_from_rows: u128 = notes(&moved_delta)
        .iter()
        .map(|note| match note {
            PalwVestingNoteV1::Moved { legs, .. } => {
                legs.iter().filter(|leg| leg.queue_key.is_some()).map(|leg| leg.amount as u128).sum()
            }
            _ => 0,
        })
        .sum();
    let live: u128 = moved.vesting_iter_by_expiry().map(PalwVestingRowV1::total_sompi_u128).sum();
    let burned = moved.vesting_counters().burned;
    let reserve_credited = (moved.panel_reserve_sompi() - s5.panel_reserve_sompi()) as u128;
    assert_eq!((minted_from_rows, buyback, unnamed, live, burned, reserve_credited), (589, 31, 0, 0, 0, 0));
    assert_eq!(withheld, minted_from_rows + live + burned + unnamed + buyback + reserve_credited, "T03, the buyback term non-zero");
    let key = palw_vesting_payout_key_v1(&claim_id);
    let (_, paid_delta) = fold(&moved, &p, row.expiry_daa + 1, &extras);
    assert!(
        paid_delta.entries.iter().any(|e| matches!(
            e,
            PalwDeltaEntryV2::Payout { key: drained, old: Some(old), new: None } if *drained == key && old.amount == 589
        )),
        "minted by the block after the move"
    );
}

/// **V-2 / N8: the row copies its attribution from the liability record `persist_panel_liability`
/// has just written** — the `Some(record)` branch, the one that runs on testnet-12, where objective
/// offences are active (review finding 6). At the fold (offences armed at the `Final` block): the
/// record exists, the row's four attribution fields are the record's (zero until M2 writes them),
/// and the row runs on the record's two clocks (V-4(a)). At the writer, with a record that does
/// carry M2's four fields (simulated), the row copies them field for field; with no record it
/// falls back to the claim's own `job_identity` and zeros.
#[test]
fn v2_the_row_copies_the_liability_records_attribution() {
    let p = vp();
    let (s4, claim_id) = economy_licensed(&p);
    let extras = PalwTransitionExtrasV1 { objective_offence_daa: Some(0), ..economy_extras() };
    let (s5, _) = fold(&s4, &p, 124, &extras);
    assert!(matches!(s5.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { final_daa: 124 }));
    let record = s5.panel_liabilities.get(&claim_id).expect("objective offences armed: the Final writes a liability").clone();
    let row = s5.vesting_row(&claim_id).expect("and a row").clone();
    assert_eq!(
        (row.job_identity, row.free_prompt, row.trace_root, row.segment_count),
        (record.job_identity, record.free_prompt, record.trace_root, record.segment_count)
    );
    assert_eq!((row.expiry_daa, row.settled_at_final), (record.expiry_daa, record.settled_at_final), "the lock's two clocks");

    // The writer alone, the record carrying M2's four fields.
    let claim = s4.claim(&claim_id).unwrap().clone();
    let vested =
        || PalwVestedRewardV1 { producer: PalwPayoutV2 { payload: payload(1), amount: 7 }, seats: Vec::new(), reserve: 0, buyback: 0 };
    let attributed = crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1 {
        job_identity: h64(0x10B),
        free_prompt: true,
        trace_root: h64(0x7EA),
        segment_count: 3,
        ..record
    };
    let mut b = TransitionBuilder::new(&s4, &p, false, false, false, false, &extras);
    b.state.panel_liabilities.insert(claim_id, attributed);
    b.write_vesting_row_at_final(claim_id, &claim, 124, vested()).unwrap();
    let copied = b.state.vesting_row(&claim_id).unwrap();
    assert_eq!((copied.job_identity, copied.free_prompt, copied.trace_root, copied.segment_count), (h64(0x10B), true, h64(0x7EA), 3));
    // No record: the claim's own identity, and zeros.
    let mut b = TransitionBuilder::new(&s4, &p, false, false, false, false, &extras);
    assert!(!b.state.panel_liabilities.contains_key(&claim_id));
    b.write_vesting_row_at_final(claim_id, &claim, 124, vested()).unwrap();
    let fallback = b.state.vesting_row(&claim_id).unwrap();
    assert_eq!(
        (fallback.job_identity, fallback.free_prompt, fallback.trace_root, fallback.segment_count),
        (claim.job_identity, false, Hash64::default(), 0)
    );
}

// ---- T03: the escrow leaves whole, through the row ---------------------------------------------

/// **T03 (the fold half): Σ withheld = Σ minted-from-rows + Σ live + Σ burned + Σ unnamed + Σ buyback
/// + Δ panel_reserve**, closed from the deltas alone on a simulated chain: two claims (620 each)
/// finalize at 140 and vest (620 and 62); the next blocks carry nothing; at the rows' expiry (640)
/// step 3d latches and moves both in one block — five new keys (two producer keys, and the one
/// accumulated key of each of seats 2, 3 and 4) and the floor's reserve 1 — and the block after
/// drains exactly those keys, which is what its coinbase pays (`palw_v2_payout_outputs` renders the
/// same prefix). Minted-from-rows is attributed by the parent queue's keys, as Phase 2's coinbase
/// test will by position.
#[test]
fn t03_the_withheld_escrow_is_minted_burned_or_named_nowhere_from_the_deltas_alone() {
    let p = vp();
    let (fin, floor_id, light_id) = two_class_finals(&p);
    let withheld = fin.claim(&floor_id).unwrap().escrowed_reward as u128 + fin.claim(&light_id).unwrap().escrowed_reward as u128;
    let unnamed: u128 = [floor_id, light_id]
        .iter()
        .map(|id| {
            let row = fin.vesting_row(id).unwrap();
            (row.escrowed_reward - row.buyback_bound) as u128 - row.total_sompi_u128()
        })
        .sum();
    let reserve_before = fin.panel_reserve_sompi();
    // Nothing matures before the DAA clock runs out (raw depth `None`: DAA clock only).
    let (quiet, quiet_delta) = fold(&fin, &p, 639, &Default::default());
    assert!(notes(&quiet_delta).is_empty() && quiet.vesting_len() == 2);
    let (moved, moved_delta) = fold(&quiet, &p, 640, &Default::default());
    assert_eq!(moved.vesting_len(), 0, "both rows latched and moved in their expiry block");
    let mut minted_from_rows = 0u128;
    let mut reserve_credited = 0u128;
    let mut moved_keys = BTreeSet::new();
    for note in notes(&moved_delta) {
        match note {
            PalwVestingNoteV1::Moved { legs, .. } => {
                for leg in legs {
                    match leg.queue_key {
                        Some(key) if leg.amount > 0 => {
                            minted_from_rows += leg.amount as u128;
                            moved_keys.insert(key);
                        }
                        None => reserve_credited += leg.amount as u128,
                        _ => {}
                    }
                }
            }
            PalwVestingNoteV1::ReserveCredited { sompi, .. } => assert!(sompi > 0),
            PalwVestingNoteV1::Latched { matured_at, .. } => assert_eq!(matured_at, 640),
            other => panic!("unexpected note {other:?}"),
        }
    }
    assert_eq!(moved_keys.len(), 5, "two producer keys and each seat's one key (seats 2, 3 and 4 judged both)");
    assert_eq!(moved.panel_reserve_sompi() as u128 - reserve_before as u128, reserve_credited);
    let c = moved.vesting_counters();
    assert_eq!(c.moved, minted_from_rows + reserve_credited);
    // The identity's other terms, read off the chain rather than written as literals: both rows
    // moved (nothing live), no pair took a slice at either Final (the buyback term is carried by
    // `t03_t13_a_buyback_final_vests_the_rest_and_names_its_slice`), nothing burned.
    let live: u128 = moved.vesting_iter_by_expiry().map(PalwVestingRowV1::total_sompi_u128).sum();
    let buyback: u128 = [floor_id, light_id].iter().map(|id| fin.vesting_row(id).unwrap().buyback_bound as u128).sum();
    assert_eq!((live, buyback, c.burned), (0, 0, 0));
    assert_eq!(withheld, minted_from_rows + live + c.burned + unnamed + buyback + reserve_credited, "T03");
    // The next block drains exactly the moved keys: the coinbase that block carries pays them.
    let (paid, paid_delta) = fold(&moved, &p, 641, &Default::default());
    let drained: u128 = paid_delta
        .entries
        .iter()
        .filter_map(|e| match e {
            PalwDeltaEntryV2::Payout { key, old: Some(old), new: None } if moved_keys.contains(key) => Some(old.amount as u128),
            _ => None,
        })
        .sum();
    assert_eq!(drained, minted_from_rows, "minted one block after the move, every sompi");
    assert!(paid.pending_payouts_iter().next().is_none());
}

// ---- fence-off twins ---------------------------------------------------------------------------

/// **The fence-off twin: no row is ever written and the payouts are exactly the pre-fence ones.**
/// The same claim under the fixture without the mirror pays at Final (ADR-0124's numbers), and a
/// mirror armed at a later height is byte-identical to no mirror at every block — the fence is
/// what decides, and the mirror is never hashed.
#[test]
fn fence_off_twin_a_final_pays_as_before_and_no_row_exists() {
    let off = params().with_worker_carve_permille(620).unwrap();
    let later = off.clone().with_rcore_plus_mirrors(Some(1_000_000), 0, Vec::new());
    let (s5, d5, claim_id) = economy_final(&off);
    // The later twin shares the licence and diverges only where the vesting fence would act (the
    // `Final` block and after): S-3's load invariants read an armed mirror as armed from genesis,
    // the only way `validate_palw_rcore_plus_v1` lets a network arm it, so a licensed claim without
    // a licence door (one licensed below the mirror's height) is no state they load.
    let (s4, _) = economy_licensed(&off);
    let (t5, _) = apply_economy(&s4, &later, &ctx(5, 124, 5), &[], None);
    assert_eq!(s5.state_root(), t5.state_root(), "an unreached fence moves nothing");
    assert_eq!(s5.vesting_len(), 0);
    assert!(s5.vesting_counters().is_zero() && !s5.has_rcore_plus_data());
    assert!(!d5.entries.iter().any(|e| matches!(
        e,
        PalwDeltaEntryV2::Vesting { .. } | PalwDeltaEntryV2::VestingNote(_) | PalwDeltaEntryV2::VestingCounters { .. }
    )));
    let rows: BTreeMap<Hash64, PalwPayoutV2> = s5.pending_payouts_iter().map(|(k, v)| (*k, *v)).collect();
    assert_eq!(rows[&claim_id], PalwPayoutV2 { payload: Hash64::from_u64_word(0x9A11), amount: 496 });
    assert_eq!(rows[&palw_panel_payout_key_v1(&payload(2))].amount, 41);
    assert_eq!(s5.panel_reserve_sompi(), 1);
    // Well past the would-be expiry, still nothing vests or moves on either twin.
    let (s6, d6) = fold(&s5, &off, 700, &Default::default());
    let (t6, _) = fold(&t5, &later, 700, &Default::default());
    assert_eq!(s6.state_root(), t6.state_root());
    assert!(notes(&d6).is_empty() && s6.pending_payouts_iter().next().is_none(), "the drain paid them at 125's successor");
}

/// **The fence-off twin of step 3d**: a state holding a mature row (which no unarmed chain can
/// hold) folds with the fence off and moves nothing — 3d is skipped whole, not run empty.
#[test]
fn fence_off_twin_step_3d_never_runs() {
    let off = params().with_worker_carve_permille(620).unwrap();
    let s = seeded(&[row(h64(0x51), 11, &[12, 13], 100, 0)]);
    let (child, delta) = fold(&s, &off, 5_000, &Default::default());
    assert!(notes(&delta).is_empty());
    assert_eq!(child.vesting_row(&h64(0x51)).unwrap().matured_at, None, "not even latched");
    assert!(child.pending_payouts_iter().next().is_none());
    // The same state with the fence on moves it at once.
    let (armed, armed_delta) = fold(&s, &vp(), 5_000, &Default::default());
    assert_eq!(moved_rows(&armed_delta), 1);
    assert_eq!(armed.vesting_len(), 0);
}

// ---- T11: the row outlives the claim -------------------------------------------------------------

/// **T11: a row outlives claim retirement.** With `claim_retirement_daa` 100 the claim record
/// leaves at the first block past 224, 399 DAA before its row's expiry: the row stays (a retired claim is one V-3
/// allows), stays burnable and payee-indexed, and moves at 624 exactly as it would have.
#[test]
fn t11_a_row_outlives_claim_retirement() {
    let p = vp().with_claim_retirement_daa(100).unwrap();
    let (s5, _, claim_id) = economy_final(&p);
    let (retired, _) = apply_economy(&s5, &p, &ctx(6, 225, 6), &[], None);
    assert!(retired.claim(&claim_id).is_none(), "the claim record retired");
    let row = retired.vesting_row(&claim_id).expect("the row did not").clone();
    palw_vesting_consistency_v1(&retired).expect("a row for a retired claim is consistent");
    assert!(palw_bond_is_payee_of_unmatured_row_v1(&retired, &p, &bond_key(2), 300, None), "and still holds its payees (B-3)");
    let (moved, delta) = fold(&retired, &p, row.expiry_daa, &Default::default());
    assert_eq!(moved_rows(&delta), 1);
    assert_eq!(moved.pending_payout(&palw_vesting_payout_key_v1(&claim_id)).unwrap().amount, 496);
}

// ---- T04 / T12: the burn -----------------------------------------------------------------------

/// **T04: burn before move; burn after move does nothing.** A row is burnable while it waits —
/// latched and carried behind an unlatched head included — and the burn returns its whole
/// amount once (S3's marker), journals `Burned` with the offence and kind, and counts it burned.
/// Once the row has moved to the queue the hook finds nothing: `None`, no entry.
#[test]
fn t04_a_row_burns_until_it_moves_and_never_after() {
    let p = vp();
    let extras = two_clock(5);
    // Head (expiry 100) held by the second clock; the next row (expiry 101) latched behind it.
    let mut s = seeded(&[row(h64(0xA1), 11, &[12], 100, 10), row(h64(0xA2), 13, &[14], 101, 0)]);
    s.settled_attempt_finals = 12;
    s.recent_anchor_daas = vec![199];
    let (carried, delta) = fold(&s, &p, 200, &extras);
    assert_eq!(moved_rows(&delta), 0, "stop at the unlatched head, never skip");
    assert_eq!(carried.vesting_row(&h64(0xA2)).unwrap().matured_at, Some(200), "latched");
    assert_eq!(carried.vesting_row(&h64(0xA1)).unwrap().matured_at, None);
    let plan = palw_vesting_mint_plan_v1(&carried, PALW_V2_VESTING_LEGS_PER_BLOCK, 0);
    assert_eq!(
        (plan.stopped, plan.stopped_at),
        (PalwVestingStopV1::NotLatched, Some(PalwVestingSourceV1::Row { claim_id: h64(0xA1) }))
    );

    let mut b = TransitionBuilder::new(&carried, &p, false, false, false, false, &extras);
    assert_eq!(b.burn_vesting_row(h64(0xA2), h64(0x0FF), PalwOffenceKindV1::PanelFalseValidV2).unwrap(), Some(1_000));
    assert_eq!(b.burn_vesting_row(h64(0xA2), h64(0x0FF), PalwOffenceKindV1::PanelFalseValidV2).unwrap(), None, "once");
    assert!(b.state.vesting_row(&h64(0xA2)).is_none());
    assert_eq!(b.state.vesting_counters().burned, 1_000);
    let entries = b.entries.clone();
    assert!(entries.iter().any(|e| matches!(
        e,
        PalwDeltaEntryV2::VestingNote(PalwVestingNoteV1::Burned { claim_id, sompi: 1_000, kind: PalwOffenceKindV1::PanelFalseValidV2, legs, .. })
            if *claim_id == h64(0xA2) && legs.len() == 3
    )));
    let burned = b.state.clone();
    palw_vesting_consistency_v1(&burned).unwrap();
    let delta = PalwStateDeltaV2 { point: ctx(9, 200, 9), entries };
    assert_eq!(revert_delta_v2(&burned, &delta, &p).unwrap().state_root(), carried.state_root(), "a burn reverts");

    // After the move: nothing to burn.
    let (moved, _) = fold(&seeded(&[row(h64(0xA3), 11, &[12], 100, 0)]), &p, 200, &Default::default());
    assert!(moved.vesting_row(&h64(0xA3)).is_none());
    let mut b = TransitionBuilder::new(&moved, &p, false, false, false, false, &extras);
    assert_eq!(b.burn_vesting_row(h64(0xA3), h64(0x0FF), PalwOffenceKindV1::ExecutorRefuted).unwrap(), None);
    assert!(b.entries.is_empty(), "a moved row leaves nothing to journal");
    assert_eq!(b.state.pending_payout(&palw_vesting_payout_key_v1(&h64(0xA3))).unwrap().amount, 500, "and the queue keeps its leg");
}

/// **T12: a same-block conviction and maturity — the burn comes first.** Step 3 (where S-4's
/// funnel calls the hook) runs before 3d, so a row maturing in the very block that convicts it is
/// burned, never moved; the control without the conviction moves it.
#[test]
fn t12_a_same_block_conviction_burns_before_maturity_moves() {
    let p = vp();
    let extras = PalwTransitionExtrasV1::default();
    let s = seeded(&[row(h64(0xB1), 11, &[12, 13], 100, 0)]);
    let point = ctx(2, 100, 2);
    let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
    assert_eq!(b.burn_vesting_row(h64(0xB1), h64(0xCC), PalwOffenceKindV1::PanelFalseValidV2).unwrap(), Some(1_000));
    apply_vesting_maturity(&mut b, &point).unwrap();
    assert!(b.state.pending_payouts_iter().next().is_none(), "nothing moved");
    assert_eq!(b.state.vesting_counters(), PalwVestingCountersV1 { created: 1_000, moved: 0, burned: 1_000 });
    let mut control = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
    apply_vesting_maturity(&mut control, &point).unwrap();
    assert_eq!(control.state.vesting_counters().moved, 1_000, "without the conviction it matures and moves");
}

// ---- V-4 / T12 (clocks), T37, T43, T44 -----------------------------------------------------------

/// **V-4's clocks, pure.** With no second clock configured (raw `None`) a row matures on the DAA
/// clock alone and is never "halted" (F11). With the second clock (raw `Some(d)`): it matures once
/// `d` anchors settled since Final, or at the per-obligation bound `expiry + 2 × window_court`;
/// during a licence halt (raw `Some`, escaped `None`) it never matures, whatever the DAA says; a
/// latched row is mature whatever the clocks say later. The row's lock predicate is the lock's
/// `is_live_v3`, pinned over a grid.
#[test]
fn t12_the_maturity_clocks_and_the_halt() {
    let p = vp();
    let wc = p.window_court();
    let r = row(h64(0xC1), 11, &[12], 1_000, 10);
    let mut s = seeded(std::slice::from_ref(&r));
    let m = |s: &PalwChainStateV2, now: u64, raw: Option<u64>| palw_vesting_row_maturity_v1(s, &p, &r, now, raw);
    // DAA clock only.
    assert!(!m(&s, 999, None).mature_now && m(&s, 1_000, None).mature_now);
    assert!(!m(&s, 5_000, None).halted, "no second clock configured is not a halt");
    // The second clock, a licence 1 DAA ago (no escape).
    s.recent_anchor_daas = vec![1_000];
    s.settled_attempt_finals = 14;
    let held = m(&s, 1_001, Some(5));
    assert!(held.daa_clock_met && !held.mature_now && !held.halted);
    assert_eq!((held.licences_since_final, held.licences_needed, held.second_clock_bound_daa), (4, Some(5), Some(1_000 + 2 * wc)));
    s.settled_attempt_finals = 15;
    assert!(m(&s, 1_001, Some(5)).mature_now, "the fifth anchor since Final releases it");
    s.settled_attempt_finals = 11;
    s.recent_anchor_daas = vec![1_000 + 2 * wc - 1];
    assert!(!m(&s, 1_000 + 2 * wc - 1, Some(5)).mature_now);
    assert!(m(&s, 1_000 + 2 * wc, Some(5)).mature_now, "the bound releases it without the count");
    // A halt: no anchor for 2 × window_court.
    s.recent_anchor_daas = vec![100];
    let halted = m(&s, 100 + 2 * wc, Some(5));
    assert!(halted.halted && !halted.mature_now, "V-4(b): never during a halt");
    assert!(palw_chain_vesting_halted_v1(&s, Some(5), 100 + 2 * wc, wc) && !palw_chain_vesting_halted_v1(&s, None, 100 + 2 * wc, wc));
    assert!(m(&s, 50_000, Some(5)).halted && !m(&s, 50_000, Some(5)).mature_now, "however long the halt");
    // A latched row is mature whatever the clocks say.
    let latched = PalwVestingRowV1 { matured_at: Some(1_200), ..r.clone() };
    assert!(palw_vesting_row_maturity_v1(&s, &p, &latched, 50_000, Some(5)).mature_now);
    // The row's (a) is the lock's `is_live_v3`, over a grid.
    for expiry in [0u64, 7, 1_000] {
        for settled_at in [0u64, 3, 9] {
            for now in [0u64, 6, 7, 8, 999, 1_000, 1_000 + 2 * wc - 1, 1_000 + 2 * wc, 9_999] {
                for settled_now in [0u64, 3, 8, 12, 20] {
                    for depth in [None, Some(0), Some(1), Some(5)] {
                        let lock = crate::palw_panel_var_v1::PalwSlashableLockV1 {
                            claim: h64(1),
                            amount: 1,
                            expiry_daa: expiry,
                            settled_at_final: settled_at,
                            attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                            segments: 0,
                        };
                        assert_eq!(
                            palw_vesting_lock_is_live_v1(expiry, settled_at, now, settled_now, depth, wc),
                            lock.is_live_v3(now, settled_now, depth, wc),
                            "{expiry} {settled_at} {now} {settled_now} {depth:?}"
                        );
                    }
                }
            }
        }
    }
}

/// **T37: no maturity during a licence halt.** Rows long past their DAA clock and their anchor
/// count, on a chain with no licence for `2 × window_court`: step 3d latches nothing and moves
/// nothing, block after block, and B-3 does not hold their payees for it (the halt is not
/// V-4(a)). The first licence after the halt (a fresh anchor in the ring) matures and moves them in
/// that block.
#[test]
fn t37_rows_never_mature_during_a_licence_halt() {
    let p = vp();
    let extras = two_clock(5);
    let mut s = seeded(&[row(h64(0xD1), 11, &[12], 1_000, 0), row(h64(0xD2), 13, &[14], 1_001, 0)]);
    s.settled_attempt_finals = 50;
    s.recent_anchor_daas = vec![900];
    let (h1, d1) = fold(&s, &p, 5_000, &extras);
    let (h2, d2) = fold(&h1, &p, 6_000, &extras);
    assert!(notes(&d1).is_empty() && notes(&d2).is_empty(), "nothing latches during the halt");
    assert!(h2.vesting_iter_by_expiry().all(|r| r.matured_at.is_none()));
    assert!(!palw_bond_is_payee_of_unmatured_row_v1(&h2, &p, &bond_key(11), 6_000, Some(5)), "the halt holds no collateral");
    let mut licensed = h2.clone();
    licensed.recent_anchor_daas = vec![6_000];
    licensed.settled_attempt_finals = 51;
    let (after, d) = fold(&licensed, &p, 6_001, &extras);
    assert_eq!(moved_rows(&d), 2, "the first licence after the halt releases both");
    assert_eq!(after.vesting_len(), 0);
}

/// **T43: the trickle regime — a row matures at `F + window_court + 2 × window_court`.** A licence
/// just often enough that the escape never fires, and too few anchors since Final: the second
/// clock holds the row until its per-obligation bound, and not one DAA longer (t12: F + 9,000).
#[test]
fn t43_the_trickle_regime_matures_at_the_bound() {
    let p = vp();
    let wc = p.window_court();
    let extras = two_clock(30);
    let r = row(h64(0xE1), 11, &[12], 1_000, 7);
    let bound = r.expiry_daa + 2 * wc;
    assert_eq!(bound, r.final_daa + 3 * wc, "F + window_court + 2 × window_court");
    let mut s = seeded(&[r]);
    s.settled_attempt_finals = 10;
    s.recent_anchor_daas = vec![bound - 2];
    let (held, d) = fold(&s, &p, bound - 1, &extras);
    assert!(notes(&d).is_empty(), "one DAA short of the bound");
    assert!(palw_bond_is_payee_of_unmatured_row_v1(&held, &p, &bond_key(11), bound - 1, Some(30)), "B-3 holds it too");
    let (released, d) = fold(&held, &p, bound, &extras);
    assert_eq!(moved_rows(&d), 1, "released at the bound");
    assert!(released.vesting_len() == 0);
}

/// **The latch walk latches exactly the rows V-4 calls mature** (review of the vesting work,
/// finding 5: the walk was rewritten to pay for what latches, not for what waits). Over a grid of
/// block DAAs, raw depths (none, zero, three, eight), anchor rings (none, recent, a halt) and settled
/// counts, and over rows both co-monotone (as `finalize_claim` writes them) and scrambled (as M3's
/// DA-5 re-key can leave them), with some rows already latched: the rows `latch_matured_vesting_rows`
/// latches are exactly the unlatched rows `palw_vesting_row_maturity_v1` calls mature — no more,
/// no fewer — each at `now`, and the indexes stay exact.
#[test]
fn the_latch_walk_latches_exactly_the_rows_v4_calls_mature() {
    let p = vp();
    let extras = PalwTransitionExtrasV1::default();
    let mut rows = Vec::new();
    for i in 0..32u64 {
        // Even rows co-monotone (expiry and count both grow with i); odd rows scrambled.
        let (expiry, settled) = if i % 2 == 0 { (1_000 + 60 * i, i / 2) } else { (1_000 + (97 * i) % 1_900, (11 * i) % 17) };
        let mut r = row(h64(0x700 + i), 1 + i % 4, &[5 + i % 3], expiry, settled);
        if i % 7 == 3 {
            r.matured_at = Some(1);
        }
        rows.push(r);
    }
    let base = seeded(&rows);
    let mut latching_cases = 0usize;
    for ring in [vec![], vec![1_500], vec![2_900], vec![4_400]] {
        for settled_now in [0u64, 5, 9, 16, 40] {
            let mut s = base.clone();
            s.recent_anchor_daas = ring.clone();
            s.settled_attempt_finals = settled_now;
            for raw in [None, Some(0u64), Some(3), Some(8)] {
                for now in [0u64, 999, 1_000, 1_500, 2_000, 2_450, 2_900, 3_300, 4_000, 4_500, 9_000] {
                    let want: BTreeSet<Hash64> = s
                        .vesting_iter_by_expiry()
                        .filter(|r| r.matured_at.is_none() && palw_vesting_row_maturity_v1(&s, &p, r, now, raw).mature_now)
                        .map(|r| r.claim_id)
                        .collect();
                    let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
                    b.latch_matured_vesting_rows(now, raw);
                    let mut got = BTreeSet::new();
                    for r in b.state.vesting_iter_by_expiry() {
                        let before = s.vesting_row(&r.claim_id).unwrap();
                        if before.matured_at != r.matured_at {
                            assert_eq!((before.matured_at, r.matured_at), (None, Some(now)), "latched once, at now");
                            got.insert(r.claim_id);
                        }
                    }
                    assert_eq!(got, want, "ring {ring:?} settled {settled_now} raw {raw:?} now {now}");
                    assert!(b.state.vesting_indices_are_consistent(), "ring {ring:?} settled {settled_now} raw {raw:?} now {now}");
                    latching_cases += usize::from(!want.is_empty());
                }
            }
        }
    }
    assert!(latching_cases > 50, "the grid latches something in {latching_cases} cases");
}

/// **T44: Mainnet Decision A is not applied to rows, and the latch survives a re-arm.** The row
/// matures on the lock's clocks at exactly its expiry — no coinbase-maturity term (the minted
/// output obeys Decision A on its own, Phase 2's T25) — and once latched a rewrite that would make
/// (a) hold again (DA-5's re-key, simulated through the one writer) does not un-mature it: the
/// plan still moves it, and B-3 no longer holds its payees.
#[test]
fn t44_no_decision_a_term_and_the_latch_survives_a_re_arm() {
    let p = vp();
    let extras = PalwTransitionExtrasV1::default();
    let r = row(h64(0xF1), 11, &[12], 1_000, 0);
    let s = seeded(std::slice::from_ref(&r));
    assert!(palw_vesting_row_maturity_v1(&s, &p, &r, 1_000, None).mature_now, "at the expiry, not 600 DAA after it");
    // Latch it without moving it (a market-full budget of 0 is not reachable; use the builder's
    // latch alone), then push its expiry past `now`.
    let mut b = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
    b.latch_matured_vesting_rows(1_000, None);
    let latched = b.state.vesting_row(&h64(0xF1)).unwrap().clone();
    assert_eq!(latched.matured_at, Some(1_000));
    b.write_vesting(h64(0xF1), Some(PalwVestingRowV1 { expiry_daa: 9_000, ..latched }));
    let rekeyed = b.state.clone();
    let rekeyed_row = rekeyed.vesting_row(&h64(0xF1)).unwrap();
    assert!(palw_vesting_row_maturity_v1(&rekeyed, &p, rekeyed_row, 1_001, None).mature_now, "the latch holds");
    assert!(!palw_bond_is_payee_of_unmatured_row_v1(&rekeyed, &p, &bond_key(11), 1_001, None), "a latched row holds no payee");
    assert_eq!(palw_vesting_next_block_plan_v1(&rekeyed, &p, 1_001, None).moves.len(), 1, "and it still moves");
    assert!(rekeyed.vesting_indices_are_consistent(), "the rewrite moved every index");
}

// ---- T16 / T30 / T58 / T82: the budget ---------------------------------------------------------

/// **T16: the budget counts NEW QUEUE KEYS, with the market reserve; stop, never skip; a 6-key row
/// always fits.** The planner in ADR §7.3's three-argument form, which reads the rows and never
/// the queue.
#[test]
fn t16_the_budget_is_new_keys_with_the_market_reserve_and_stops_never_skips() {
    let latched = |r: PalwVestingRowV1| PalwVestingRowV1 { matured_at: Some(1), ..r };
    // Two 6-key rows sharing four of five seat payees: 6 + (producer + 1 seat) = 8 ≤ 8.
    let a = latched(row(h64(0x11), 1, &[11, 12, 13, 14, 15], 100, 0));
    let b = latched(row(h64(0x12), 2, &[11, 12, 13, 14, 16], 101, 0));
    let c = latched(row(h64(0x13), 3, &[17, 18, 19, 20, 21], 102, 0));
    assert_eq!((a.leg_count(), a.key_count()), (6, 6));
    let s = seeded(&[a.clone(), b.clone(), c.clone()]);
    assert_eq!(palw_vesting_market_rows_waiting_v1(&s), 0);
    assert_eq!(palw_vesting_budget_v1(PALW_V2_VESTING_LEGS_PER_BLOCK, 0), 8);
    let plan = palw_vesting_mint_plan_v1(&s, PALW_V2_VESTING_LEGS_PER_BLOCK, 0);
    assert_eq!((plan.moves.len(), plan.new_keys, plan.legs), (2, 8, 12), "seat legs to one payee share one key");
    assert_eq!(
        (plan.stopped, plan.stopped_at.clone()),
        (PalwVestingStopV1::BudgetFull, Some(PalwVestingSourceV1::Row { claim_id: h64(0x13) }))
    );
    // Market rows waiting: one → budget 7, two or more → 6 (the reserve), and a 6-key row fits.
    let mut m1 = s.clone();
    m1.pending_payouts.insert(market_key(1), PalwPayoutV2 { payload: h64(1), amount: 1 });
    assert_eq!(palw_vesting_market_rows_waiting_v1(&m1), 1);
    assert_eq!(palw_vesting_budget_v1(PALW_V2_VESTING_LEGS_PER_BLOCK, 1), 7);
    let mut m = m1.clone();
    for i in 2..40 {
        m.pending_payouts.insert(market_key(i), PalwPayoutV2 { payload: h64(1), amount: 1 });
    }
    let waiting = palw_vesting_market_rows_waiting_v1(&m);
    assert_eq!(waiting, PALW_V2_VESTING_MARKET_RESERVE, "counted up to the reserve");
    assert_eq!(palw_vesting_budget_v1(PALW_V2_VESTING_LEGS_PER_BLOCK, waiting), 6);
    let plan = palw_vesting_mint_plan_v1(&m, PALW_V2_VESTING_LEGS_PER_BLOCK, waiting);
    assert_eq!((plan.moves.len(), plan.new_keys), (1, 6), "one 6-key row: the market keeps two slots");
    // The queue is not an input (review finding 1): a seat key the queue already holds costs the
    // same as a fresh one, so the plan is the same on any state holding these rows.
    let only_c = seeded(std::slice::from_ref(&c));
    let mut only_c_held = only_c.clone();
    only_c_held.pending_payouts.insert(palw_panel_payout_key_v1(&payload(17)), PalwPayoutV2 { payload: payload(17), amount: 1 });
    let held_plan = palw_vesting_mint_plan_v1(&only_c_held, PALW_V2_VESTING_LEGS_PER_BLOCK, 0);
    assert_eq!(held_plan.new_keys, 6);
    assert_eq!(held_plan, palw_vesting_mint_plan_v1(&only_c, PALW_V2_VESTING_LEGS_PER_BLOCK, 0));
    // Two credited seats paid at one payload share one key within a row too (IMPL-10: in keys).
    let mut shared = row(h64(0x15), 6, &[23, 24], 99, 0);
    shared.seats[1].1.payload = shared.seats[0].1.payload;
    assert_eq!((shared.leg_count(), shared.key_count()), (3, 2), "two seats, one payload, one key");
    // Stop, never skip: an unlatched head stops the plan even with latched rows behind it.
    let head = row(h64(0x10), 4, &[22], 99, 0);
    let stuck = seeded(&[head, a.clone(), b.clone()]);
    let plan = palw_vesting_mint_plan_v1(&stuck, PALW_V2_VESTING_LEGS_PER_BLOCK, 0);
    assert!(plan.moves.is_empty());
    assert_eq!(
        (plan.stopped, plan.stopped_at),
        (PalwVestingStopV1::NotLatched, Some(PalwVestingSourceV1::Row { claim_id: h64(0x10) }))
    );
    assert_eq!(palw_vesting_matured_moves_v1(&stuck).count(), 0, "the iterator ends at the unlatched head");
    // Empty.
    let plan = palw_vesting_mint_plan_v1(&PalwChainStateV2::genesis(), PALW_V2_VESTING_LEGS_PER_BLOCK, 0);
    assert_eq!((plan.moves.len(), plan.stopped, plan.stopped_at), (0, PalwVestingStopV1::Empty, None));
    // Reporter rewards move first, one key each.
    let mut rep = s.clone();
    rep.reporter_rewards.insert(h64(0x77), PalwPayoutV2 { payload: payload(30), amount: 9 });
    let plan = palw_vesting_mint_plan_v1(&rep, PALW_V2_VESTING_LEGS_PER_BLOCK, 0);
    assert_eq!(plan.moves[0].source, PalwVestingSourceV1::Reporter { offence_id: h64(0x77) });
    assert_eq!(plan.moves[0].legs[0].queue_key, Some(palw_reporter_payout_key_v1(&h64(0x77))));
    assert_eq!((plan.moves.len(), plan.new_keys), (2, 7), "the reporter's key, then row a; b no longer fits");
    // Position (RPC), in keys (IMPL-10): moves and keys ahead.
    assert_eq!(palw_vesting_mint_position_v1(&rep, &h64(0x12)), Some((2, 1 + 6)));
    assert_eq!(palw_vesting_mint_position_v1(&rep, &h64(0x99)), None);
    // The head exception: a row wider than the budget moves alone when the caller passes the full
    // width — which the fold does only while nothing non-market waits after the drain.
    let wide = latched(row(h64(0x14), 5, &[11, 12, 13, 14, 15, 16, 17, 18], 99, 0));
    let w = seeded(&[wide.clone(), a.clone()]);
    let plan = palw_vesting_mint_plan_v1(&w, PALW_V2_VESTING_LEGS_PER_BLOCK, 0);
    assert_eq!((plan.moves.len(), plan.new_keys), (1, 9), "the head moves; nothing follows it");
    let mut w_waiting = w.clone();
    w_waiting.pending_payouts.insert(palw_vesting_payout_key_v1(&h64(0x99)), PalwPayoutV2 { payload: h64(1), amount: 1 });
    assert_eq!(palw_vesting_non_market_rows_waiting_v1(&w_waiting), 1);
    let belt = PALW_V2_VESTING_LEGS_PER_BLOCK - palw_vesting_non_market_rows_waiting_v1(&w_waiting);
    assert!(palw_vesting_mint_plan_v1(&w_waiting, belt, 0).moves.is_empty(), "not below the full width (the fold's belt)");
}

/// **The planner answers the same on a committed state as the next block's step 3d** (review of
/// the vesting work, finding 1). Block N, with the market waiting (budget 6), moves row A (six
/// keys); row B shares four of A's seat payees and row C is disjoint. On state N the queue still
/// holds A's keys — a cost that excluded held keys priced B at 2 there, and 6 in the next fold.
/// Now `palw_vesting_next_block_plan_v1(N, …, N + 1)`, which replays the next drain (the full width,
/// the market rows it leaves) and the next latch, is exactly what block N + 1's step 3d moves (B,
/// alone, at six keys; C stops it), and the moved legs are the planned legs.
#[test]
fn the_next_block_plan_on_a_committed_state_is_what_the_next_fold_moves() {
    let p = vp();
    let latched = |r: PalwVestingRowV1| PalwVestingRowV1 { matured_at: Some(1), ..r };
    let a = latched(row(h64(0x21), 1, &[11, 12, 13, 14, 15], 100, 0));
    let b = latched(row(h64(0x22), 2, &[11, 12, 13, 14, 16], 101, 0));
    let c = latched(row(h64(0x23), 3, &[17], 102, 0));
    let mut s = seeded(&[a, b, c]);
    for i in 0..40 {
        s.pending_payouts.insert(market_key(i), PalwPayoutV2 { payload: h64(1), amount: 1 });
    }
    let (n, dn) = fold(&s, &p, 200, &Default::default());
    assert_eq!(moved_rows(&dn), 1, "block N: A alone (6 of a budget of 6)");
    assert!(n.vesting_row(&h64(0x21)).is_none() && n.vesting_row(&h64(0x22)).is_some());
    assert_eq!(palw_vesting_non_market_rows_waiting_v1(&n), 6, "state N still holds A's six keys");
    let predicted = palw_vesting_next_block_plan_v1(&n, &p, 201, None);
    assert_eq!(palw_vesting_market_rows_waiting_after_drain_v1(&n), PALW_V2_VESTING_MARKET_RESERVE);
    assert_eq!(predicted.moves.len(), 1);
    assert_eq!(predicted.moves[0].source, PalwVestingSourceV1::Row { claim_id: h64(0x22) });
    assert_eq!(predicted.new_keys, 6, "B at six keys, not the two a held-key exclusion made it on state N");
    assert_eq!(
        (predicted.stopped, predicted.stopped_at.clone()),
        (PalwVestingStopV1::BudgetFull, Some(PalwVestingSourceV1::Row { claim_id: h64(0x23) }))
    );
    let (next, d_next) = fold(&n, &p, 201, &Default::default());
    let moved: Vec<(PalwVestingSourceV1, Vec<PalwVestingLegV1>)> = notes(&d_next)
        .into_iter()
        .filter_map(|note| match note {
            PalwVestingNoteV1::Moved { source, legs } => Some((source, legs)),
            _ => None,
        })
        .collect();
    let planned: Vec<(PalwVestingSourceV1, Vec<PalwVestingLegV1>)> =
        predicted.moves.iter().map(|m| (m.source.clone(), m.legs.clone())).collect();
    assert_eq!(moved, planned, "the fold moved exactly the committed state's plan");
    assert!(next.vesting_row(&h64(0x23)).is_some(), "and C waits, as planned");
}

/// The moves a block's step 3d made, off its notes, in order.
fn moves_of(delta: &PalwStateDeltaV2) -> Vec<(PalwVestingSourceV1, Vec<PalwVestingLegV1>)> {
    notes(delta)
        .into_iter()
        .filter_map(|note| match note {
            PalwVestingNoteV1::Moved { source, legs } => Some((source, legs)),
            _ => None,
        })
        .collect()
}

/// **The next-block plan is the next fold's plan, block after block** (review of the vesting work,
/// finding 1: the committed-state question must answer what step 3d does NEXT, including the rows
/// that block's latch adds). A simulated chain of empty blocks over 40 rows, most of them unlatched
/// on the committed state and matured by the next block's clocks: co-monotone rows as `finalize_claim`
/// writes them, a few re-keyed later (M3's DA-5), a few latched from the start, five-seat panels
/// drawn from eight payees, two reporter rewards, and 40 market rows the reserve protects until they
/// drain. Run with no second clock, and with one at depth 4 whose anchors tick between blocks (on the
/// committed state, so each block itself carries no object) and stop for a halt midway. At every
/// block, `palw_vesting_next_block_plan_v1(parent, next_daa, raw)` equals the moves the fold makes,
/// leg for leg. Most rows move in the very block that latches them, and there the fold's planner
/// handed the parent without the latch replay answers wrong.
#[test]
fn the_next_block_plan_is_the_next_folds_plan_over_a_simulated_chain() {
    let p = vp();
    let wc = p.window_court();
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let rows: Vec<PalwVestingRowV1> = (0..40u64)
        .map(|i| {
            let mut pool: Vec<u64> = (1..=8).collect();
            let mut seats = Vec::new();
            for _ in 0..if i % 5 == 4 { 2 } else { 5 } {
                let at = (next() % pool.len() as u64) as usize;
                seats.push(200 + pool.remove(at));
            }
            // Co-monotone: expiry and settled count both grow with i, the count so that with the
            // second clock a row waits a few blocks past its DAA expiry for its fourth anchor. Every
            // seventh is re-keyed 900 DAA later with its old count; every eleventh is latched already.
            let expiry = 1_000 + 40 * i + if i % 7 == 6 { 900 } else { 0 };
            let mut r = row(h64(0x8_0000 + i), 100 + i, &seats, expiry, 7 * i / 4);
            if i % 11 == 10 {
                r.matured_at = Some(1);
            }
            r
        })
        .collect();
    for (raw, extras) in [(None, PalwTransitionExtrasV1::default()), (Some(4u64), two_clock(4))] {
        let mut s = seeded(&rows);
        for i in 0..40 {
            s.pending_payouts.insert(market_key(i), PalwPayoutV2 { payload: h64(1), amount: 1 });
        }
        s.reporter_rewards.insert(h64(0x5E1), PalwPayoutV2 { payload: payload(301), amount: 7 });
        s.reporter_rewards.insert(h64(0x5E2), PalwPayoutV2 { payload: payload(302), amount: 9 });
        s.recent_anchor_daas = vec![990];
        s.settled_attempt_finals = 0;
        let mut daa = 990u64;
        let (mut blocks, mut newly_latched_moved, mut compared_moves, mut unreplayed_differs) = (0usize, 0usize, 0usize, 0usize);
        let mut halted_blocks = 0usize;
        let halt = (1_700u64, 1_700 + 2 * wc + 100);
        while s.vesting_len() > 0 {
            daa += 23;
            assert!(blocks < 2_000, "the rows drain");
            if raw.is_some() && !(halt.0..halt.1).contains(&daa) {
                // A licence landed after the parent: the next block sees one more anchor.
                s.recent_anchor_daas = vec![daa - 1];
                s.settled_attempt_finals += 1;
            }
            halted_blocks += usize::from(palw_chain_vesting_halted_v1(&s, raw, daa, wc));
            let predicted = palw_vesting_next_block_plan_v1(&s, &p, daa, raw);
            let (child, delta) = apply_palw_transition_v2_with_extras(
                &s,
                &p,
                &ctx(blocks as u64 + 2, daa, blocks as u64 + 2),
                &[],
                None,
                false,
                false,
                false,
                false,
                &extras,
            )
            .expect("the fold applies");
            palw_vesting_counters_consistent_v1(&child).expect("V-3");
            let planned: Vec<(PalwVestingSourceV1, Vec<PalwVestingLegV1>)> =
                predicted.moves.iter().map(|m| (m.source.clone(), m.legs.clone())).collect();
            assert_eq!(moves_of(&delta), planned, "raw {raw:?} daa {daa}: the fold moved the parent's next-block plan");
            compared_moves += planned.len();
            newly_latched_moved += planned
                .iter()
                .filter(|(source, _)| {
                    matches!(source, PalwVestingSourceV1::Row { claim_id } if s.vesting_row(claim_id).is_some_and(|r| r.matured_at.is_none()))
                })
                .count();
            // The fold's planner handed the committed parent with the drained width but without
            // the latch replay: it misses every row the next block latches (finding 1's residue).
            let unreplayed =
                palw_vesting_mint_plan_v1(&s, PALW_V2_VESTING_LEGS_PER_BLOCK, palw_vesting_market_rows_waiting_after_drain_v1(&s));
            unreplayed_differs += usize::from(unreplayed.moves != predicted.moves);
            s = child;
            blocks += 1;
        }
        println!(
            "raw {raw:?}: {blocks} blocks ({halted_blocks} halted), {compared_moves} moves, {newly_latched_moved} in their latch \
             block, {unreplayed_differs} blocks the unreplayed plan misread"
        );
        assert_eq!(halted_blocks > 0, raw.is_some(), "the second-clock run crosses a licence halt");
        assert_eq!(compared_moves, 42, "raw {raw:?}: every row and both reporter rewards moved through a predicted plan");
        assert!(newly_latched_moved > 10, "raw {raw:?}: {newly_latched_moved} rows moved in the block that latched them");
        assert!(unreplayed_differs > 10, "raw {raw:?}: without the latch replay the plan is wrong in {unreplayed_differs} blocks");
    }
}

/// **T30 / T58: market rows fill the queue — at least one row moves per block while mature, the
/// queue stays ≤ 1,024 + 8, the non-market part ≤ 8 after every fold, and every moved key is
/// drained (minted) by the very next block.** 1,024 market rows and a backlog of latched 6-key
/// rows: each block moves exactly one row (budget 6) and the drain takes that row's six keys and two
/// market rows.
#[test]
fn t30_t58_market_rows_waiting_still_drain_one_row_per_block() {
    let p = vp();
    let rows: Vec<PalwVestingRowV1> = (0..12u64)
        .map(|i| PalwVestingRowV1 {
            matured_at: Some(1),
            ..row(h64(0x300 + i), 40 + i, &[11 + i, 30 + i, 50 + i, 70 + i, 90 + i], 100 + i, 0)
        })
        .collect();
    let mut s = seeded(&rows);
    for i in 0..PALW_V2_MAX_PENDING_PAYOUTS as u64 {
        s.pending_payouts.insert(market_key(i), PalwPayoutV2 { payload: h64(1), amount: 1 });
    }
    let mut daa = 200;
    let mut previous_moved_keys: BTreeSet<Hash64> = BTreeSet::new();
    for _ in 0..12 {
        let market_before = s.pending_payouts_iter().filter(|(k, _)| k.as_byte_slice()[0] == 0xFF).count();
        let (child, delta) = fold_fast(&s, &p, daa);
        // The drain paid last block's moved keys first, then market rows.
        for key in &previous_moved_keys {
            assert!(child.pending_payout(key).is_none(), "moved one block ago, minted now");
        }
        let market_after_drain = market_before - (PALW_V2_MAX_PAYOUTS_PER_BLOCK - previous_moved_keys.len());
        assert_eq!(child.pending_payouts_iter().filter(|(k, _)| k.as_byte_slice()[0] == 0xFF).count(), market_after_drain);
        assert_eq!(moved_rows(&delta), 1, "exactly one row per block while the market waits");
        assert!(non_market(&child) <= PALW_V2_MAX_PAYOUTS_PER_BLOCK, "the queue lemma");
        assert!(child.pending_payouts_iter().count() <= PALW_V2_MAX_PENDING_PAYOUTS + PALW_V2_MAX_PAYOUTS_PER_BLOCK);
        previous_moved_keys = child.pending_payouts_iter().filter(|(k, _)| k.as_byte_slice()[0] != 0xFF).map(|(k, _)| *k).collect();
        assert_eq!(previous_moved_keys.len(), 6);
        s = child;
        daa += 1;
    }
    assert_eq!(s.vesting_len(), 0, "twelve rows, twelve blocks");
}

/// **T82: V-7 after a long halt with 8 genesis payees.** A backlog of latched floor rows whose five
/// credited seats are drawn uniformly from eight payees drains at ≥ 1 row every block; with no
/// market row waiting the mean is about 1.29 (the model's 1.286: a second row fits only when its
/// seats overlap the first's in at least four of five), and a backlog of N rows drains in ≤ N
/// blocks. While the market waits, exactly one row a block and the market takes the other two
/// slots (T30).
#[test]
fn t82_a_post_halt_backlog_drains_at_least_one_row_per_block() {
    let p = vp();
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let n = 240u64;
    let rows: Vec<PalwVestingRowV1> = (0..n)
        .map(|i| {
            let mut pool: Vec<u64> = (1..=8).collect();
            let mut seats = Vec::new();
            for _ in 0..5 {
                let at = (next() % pool.len() as u64) as usize;
                seats.push(100 + pool.remove(at));
            }
            PalwVestingRowV1 { matured_at: None, ..row(h64(0x1_0000 + i), 1_000 + i, &seats, 100 + i, 0) }
        })
        .collect();
    let mut s = seeded(&rows);
    let mut blocks = 0u64;
    let mut two_or_more = 0u64;
    let mut daa = 1_000;
    while s.vesting_len() > 0 {
        let (child, delta) = fold_fast(&s, &p, daa);
        let moved = moved_rows(&delta);
        assert!(moved >= 1, "at least one row every block");
        assert!(non_market(&child) <= PALW_V2_MAX_PAYOUTS_PER_BLOCK, "the queue lemma");
        two_or_more += u64::from(moved >= 2);
        blocks += 1;
        s = child;
        daa += 1;
    }
    assert!(blocks <= n, "N rows in at most N blocks");
    let mean = n as f64 / blocks as f64;
    assert!((1.2..1.4).contains(&mean), "mean {mean:.3} rows per block (model 1.286)");
    let share = two_or_more as f64 / blocks as f64;
    assert!((0.18..0.40).contains(&share), "P(≥ 2) {share:.3} (model 0.285)");
    println!("T82: {n} rows drained in {blocks} blocks, mean {mean:.3} rows/block, P(>=2) {share:.3}");
}

// ---- T47: A-KEY ----------------------------------------------------------------------------------

/// **T47: a claim whose raw id starts `0xFF` still keys `0x00` and is minted next block behind a
/// full market.** Keyed by its raw id the producer leg would sort among the 1,024 market rows and
/// wait behind them (M-10); under A-KEY it sorts first and the next block drains it.
#[test]
fn t47_an_0xff_claim_id_keys_0x00_and_mints_behind_a_full_market() {
    let claim_id = Hash64::from_bytes([0xFF; 64]);
    let key = palw_vesting_payout_key_v1(&claim_id);
    assert_eq!(key.as_byte_slice()[0], PALW_STATE_V2_VESTING_PAYOUT_KEY_PREFIX);
    assert_ne!(key, claim_id);
    assert_eq!(palw_reporter_payout_key_v1(&claim_id).as_byte_slice()[0], 0x00);
    assert_ne!(palw_reporter_payout_key_v1(&claim_id), key, "two domains");
    assert_ne!(palw_vesting_payout_key_v1(&h64(1)), palw_vesting_payout_key_v1(&h64(2)));
    for domain in [PALW_STATE_V2_DOMAIN_VESTING_PAYOUT, PALW_STATE_V2_DOMAIN_REPORTER_PAYOUT] {
        assert!(!PALW_STATE_V2_ALL_DOMAINS.contains(&domain), "a row-key domain stays out of the list");
        for other in [PALW_STATE_V2_DOMAIN_MODEL_PAYOUT, PALW_STATE_V2_DOMAIN_MODEL_REFUND, PALW_STATE_V2_DOMAIN_PANEL_PAYOUT] {
            assert_ne!(domain, other);
        }
        assert!(PALW_STATE_V2_ALL_DOMAINS.iter().all(|d| *d != domain));
    }

    let p = vp();
    let mut s = seeded(&[PalwVestingRowV1 { matured_at: Some(1), ..row(claim_id, 11, &[12], 100, 0) }]);
    for i in 0..PALW_V2_MAX_PENDING_PAYOUTS as u64 {
        s.pending_payouts.insert(market_key(i), PalwPayoutV2 { payload: h64(1), amount: 1 });
    }
    assert!(s.pending_payouts_iter().take(PALW_V2_MAX_PAYOUTS_PER_BLOCK).all(|(k, _)| *k < claim_id), "the raw id would wait");
    let (moved, _) = fold(&s, &p, 200, &Default::default());
    let first: Vec<Hash64> = moved.pending_payouts_iter().take(PALW_V2_MAX_PAYOUTS_PER_BLOCK).map(|(k, _)| *k).collect();
    assert_eq!(first[0], key, "A-KEY sorts first");
    let (paid, _) = fold(&moved, &p, 201, &Default::default());
    assert!(paid.pending_payout(&key).is_none(), "minted by the next block");
}

// ---- T41 / the reorg twin ---------------------------------------------------------------------

/// **T41 and the reorg twin: one block that burns, latches and moves, applied and reverted.** Built
/// as the fold builds it — the burn at step 3 (S-4's call), then step 3d — its delta reproduces the
/// child and reverts to the exact parent (roots, rows, latches, counters, indexes), and the child's
/// carriage re-imports under its root. A reorg that removes the block restores the burned row and
/// un-latches the moved ones (I-9), so the other branch can convict it instead.
#[test]
fn t41_a_block_that_burns_latches_and_moves_reverts_to_its_parent() {
    let p = vp();
    let extras = PalwTransitionExtrasV1::default();
    let parent = seeded(&[
        row(h64(0x401), 11, &[12, 13], 100, 0),
        row(h64(0x402), 14, &[12, 15], 101, 0),
        row(h64(0x403), 16, &[17], 5_000, 0),
    ]);
    let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    assert_eq!(b.burn_vesting_row(h64(0x403), h64(0xBAD), PalwOffenceKindV1::CourtConviction).unwrap(), Some(1_000));
    apply_vesting_maturity(&mut b, &ctx(2, 200, 2)).unwrap();
    let child = b.state.clone();
    let delta = PalwStateDeltaV2 { point: ctx(2, 200, 2), entries: b.entries.clone() };
    let kinds: Vec<&str> = notes(&delta)
        .iter()
        .map(|n| match n {
            PalwVestingNoteV1::Burned { .. } => "burned",
            PalwVestingNoteV1::Latched { .. } => "latched",
            PalwVestingNoteV1::Moved { .. } => "moved",
            PalwVestingNoteV1::ReserveCredited { .. } => "reserve",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["burned", "latched", "latched", "moved", "reserve", "moved", "reserve"]);
    assert_eq!(palw_vesting_burned_by_delta_v1(&delta), 1_000);
    assert_eq!(child.vesting_len(), 0);
    assert_eq!(child.vesting_counters(), PalwVestingCountersV1 { created: 3_000, moved: 2_000, burned: 1_000 });
    checked(&parent, &child, &delta, &p);
    let back = revert_delta_v2(&child, &delta, &p).unwrap();
    assert_eq!(back, parent, "the parent, field for field — rows un-latched, the burned row back");
    // The carriage of a state holding latched and unlatched rows round-trips under its root.
    let mut half = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
    half.latch_matured_vesting_rows(100, None);
    let latched = half.state.clone();
    assert_eq!(latched.vesting_iter_by_expiry().filter(|r| r.matured_at.is_some()).count(), 1);
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&latched)).unwrap();
    let imported = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&p, Some(latched.state_root())).unwrap();
    assert_eq!(imported, latched, "import rebuilds the rows, the latches and the indexes");
}

// ---- B-3's payee question ------------------------------------------------------------------------

/// **B-3's vesting term through the payee index agrees with the brute force** over a grid of rows,
/// clocks, depths and halts — and it never counts a latched row.
#[test]
fn the_payee_index_answers_b3_as_the_walk_does() {
    let p = vp();
    let wc = p.window_court();
    let mut rows = Vec::new();
    for i in 0..24u64 {
        let mut r = row(h64(0x600 + i), 1 + i % 3, &[4 + i % 4, 9], 1_000 + 97 * i, i % 7);
        if i % 5 == 0 {
            r.matured_at = Some(1);
        }
        rows.push(r);
    }
    let base = seeded(&rows);
    for ring in [vec![], vec![1_500], vec![3_900]] {
        for settled_now in [0u64, 4, 9, 40] {
            let mut s = base.clone();
            s.recent_anchor_daas = ring.clone();
            s.settled_attempt_finals = settled_now;
            for raw in [None, Some(3u64)] {
                for now in [0u64, 999, 1_000, 1_500, 2_200, 3_300, 4_000, 5_000, 9_000] {
                    let escaped = palw_second_clock_depth_v1(raw, &s.recent_anchor_daas, now, wc);
                    for bond in 1..=10u64 {
                        let key = bond_key(bond);
                        let brute = s.vesting_iter_by_expiry().any(|r| {
                            r.matured_at.is_none()
                                && r.payee_bonds().any(|b| b == key)
                                && palw_vesting_lock_is_live_v1(r.expiry_daa, r.settled_at_final, now, settled_now, escaped, wc)
                        });
                        assert_eq!(
                            palw_bond_is_payee_of_unmatured_row_v1(&s, &p, &key, now, raw),
                            brute,
                            "bond {bond} now {now} raw {raw:?}"
                        );
                    }
                }
            }
        }
    }
    // The RPC view: every leg a bond is payee of.
    let legs: Vec<_> = base.vesting_legs_of_payee(&bond_key(9)).collect();
    assert_eq!(legs.len(), 24, "bond 9 sits on every row");
    assert!(legs.iter().all(|(_, leg)| leg.kind == PalwVestingLegKindV1::Seat && leg.payee_bond == Some(bond_key(9))));
}

