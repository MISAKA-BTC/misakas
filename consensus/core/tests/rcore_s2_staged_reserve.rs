//! **ADR-0152 v3.1 R-core+, S-2: the staged reserve on testnet-12's own fold** — T01, T14, T27,
//! T37, T68 and T74's V2-door half (S-SPEC §3.1–§3.5, §6), each beside its fence-off twin
//! (testnet-12 with `palw_rcore_plus = None`, C7 cleared, mirrors re-synced).
//!
//! Every block goes through `apply_palw_transition_v7` with the extras the processor resolves, and
//! every block is checked three ways ([`Chain::step`]): its delta re-applies to the child and
//! reverts to the parent, and the child's carriage reloads (`into_state`: the ledger re-derived
//! from the claims' commitments at the last point, R-core+'s load invariants, DL-1's deadlines
//! exactly). What is measured is the producer's `reserved_exposure` — the escrow term `E` leaves it
//! at the licence where SR-1 says the release is due, and at Final otherwise — and the claim's
//! `rcore` record, and the second clock (V-8).
//!
//! The floor claims are `dos_l5_4b`'s: the floor class's own pwu, the first genesis bond producing,
//! the next five genesis bonds seated. The model-class claims (8k, 2M) are `panel_room`'s: the row
//! made `Active` through the carriage, the genesis bonds proved ready, a rich producer.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_s2_staged_reserve

#[path = "rcore_common.rs"]
mod common;
use common::*;

/// **T01 (floor, V1 quorum door): a licence with every seat `Valid` releases `E`; Final releases
/// `w`.** Revert and reload twins on every block (`Chain::step`). The fence-off twin holds `E` to
/// Final and records nothing.
#[test]
fn t01_a_full_service_quorum_licence_releases_the_escrow_and_final_releases_the_weight() {
    for armed in [true, false] {
        let p = if armed { t12() } else { twin(&t12()) };
        let mut c = Chain::new(p);
        let (producer, _, _) = floor_producer(&c.p);
        let before = c.reserved(&producer);
        let id = c.floor_claim(1);
        let accepted = c.claim(&id);
        let (w, e) = (accepted.reserved, escrow(&c.sp, &accepted));
        assert!(e > 0, "the premise: option A reserves the escrow from genesis");
        assert_eq!(c.reserved(&producer), before + w + e, "armed={armed}: acceptance commits w + E");
        let seats = c.floor_seats();
        let bound = c.bind(id, &seats);
        let settled = c.s.settled_attempt_finals();
        let receipts: Vec<_> = seats.iter().map(|(k, _)| valid(id, *k, bound)).collect();
        let g_res = licence_g_res(&c, &id);
        c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
        let licensed = c.claim(&id);
        assert_eq!(c.s.settled_attempt_finals(), settled + 1, "armed={armed}: an attempt's V1 licence settles the anchor (V-8)");
        if armed {
            assert_eq!(
                licensed.rcore,
                PalwClaimRcoreV1 {
                    licence_door: Some(PalwLicenceDoorTagV1::Quorum),
                    basis_k: 3,
                    escrow_released: true,
                    served_mask: 0b1_1111,
                    unserved_seen: false,
                    g_res_sompi: g_res,
                },
                "the licence records its door, its recount, its service, the release and the G_res it priced with"
            );
            assert_eq!(c.reserved(&producer), before + w, "E leaves at the licence (SR-1)");
            assert_eq!(palw_claim_commitment_v1(&c.sp, &licensed, c.daa), Some(w));
        } else {
            assert_eq!(licensed.rcore, PalwClaimRcoreV1::default(), "fence off: nothing recorded");
            assert_eq!(c.reserved(&producer), before + w + e, "fence off: E is held to Final (option A)");
        }
        assert!(palw_rcore_counts_licensed_v1(&licensed), "a V1 licence frees the replay either way");
        c.finalize(id);
        assert_eq!(c.reserved(&producer), before, "armed={armed}: Final releases the rest");
        if armed {
            assert!(c.claim(&id).rcore.escrow_released, "T14: the flag never un-flips");
        }
    }
}

/// **T27 / T68 on the floor: only a carried `Valid` serves.** Three `Valid`s and two missing
/// receipts hold `E`; three `Valid`s and two `Unavailable`s hold it and latch `unserved_seen`. Both
/// still license (V1's quorum), tick (V-8, basis 3) and count as licensed for the room (T-2(c)).
#[test]
fn t27_t68_missing_and_unavailable_seats_hold_the_escrow_to_final() {
    for with_unavailable in [false, true] {
        let mut c = Chain::new(t12());
        let (producer, _, _) = floor_producer(&c.p);
        let before = c.reserved(&producer);
        let id = c.floor_claim(10 + with_unavailable as u64);
        let accepted = c.claim(&id);
        let (w, e) = (accepted.reserved, escrow(&c.sp, &accepted));
        let seats = c.floor_seats();
        let bound = c.bind(id, &seats);
        let mut receipts: Vec<_> = seats[..3].iter().map(|(k, _)| valid(id, *k, bound)).collect();
        if with_unavailable {
            receipts.extend(seats[3..].iter().map(|(k, _)| unavailable(id, *k, bound)));
        }
        let settled = c.s.settled_attempt_finals();
        let g_res = licence_g_res(&c, &id);
        c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
        let licensed = c.claim(&id);
        assert_eq!(
            licensed.rcore,
            PalwClaimRcoreV1 {
                licence_door: Some(PalwLicenceDoorTagV1::Quorum),
                basis_k: 3,
                escrow_released: false,
                served_mask: 0b0_0111,
                unserved_seen: with_unavailable,
                g_res_sompi: g_res,
            },
            "unavailable={with_unavailable}"
        );
        assert_eq!(c.reserved(&producer), before + w + e, "unavailable={with_unavailable}: E is held");
        assert_eq!(c.s.settled_attempt_finals(), settled + 1, "a basis-3 licence ticks");
        assert!(palw_rcore_counts_licensed_v1(&licensed));
        c.finalize(id);
        assert_eq!(c.reserved(&producer), before, "Final releases w + E");
        assert!(!c.claim(&id).rcore.escrow_released, "T14: nothing flipped late");
    }
}

/// **T27 on the 8k row and U1: `Incapable` holds (the floor refuses it), and five `Valid`s release —
/// the 8k row is released at licence like the floor; only C7 (2M) is held to Final.**
#[test]
fn t27_u1_the_8k_row_holds_on_incapable_and_releases_on_full_service() {
    let p = t12();
    let (short, _) = model_classes(&p);
    for incapable in [true, false] {
        let mut c = model_chain(p.clone(), short, 1);
        let before = c.reserved(&bond_key(1));
        let id = model_claim(&mut c, short, 1, 0x70 + incapable as u64);
        let accepted = c.claim(&id);
        let (w, e) = (accepted.reserved, escrow(&c.sp, &accepted));
        let seats = honest_seats(&c.p, 5);
        c.s = readied(&c.sp, &c.s, &honest(&c.p), short, c.daa);
        let bound = c.bind(id, &seats);
        let receipts: Vec<_> = seats
            .iter()
            .enumerate()
            .map(
                |(i, (k, _))| {
                    if incapable && i >= 3 { receipt(id, *k, PalwReceiptVerdictV2::Incapable, bound) } else { valid(id, *k, bound) }
                },
            )
            .collect();
        c.s = readied(&c.sp, &c.s, &honest(&c.p), short, c.daa);
        c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
        let licensed = c.claim(&id);
        assert_eq!(licensed.rcore.unserved_seen, incapable, "an Incapable is carried as not served");
        assert_eq!(licensed.rcore.escrow_released, !incapable, "incapable={incapable}");
        let held = if incapable { e } else { 0 };
        assert_eq!(c.reserved(&bond_key(1)), before + w + held, "incapable={incapable}: E held iff a seat did not serve");
    }
}

/// **U1 / SR-1 condition 4: a C7 (2M) claim holds `E` to Final whatever its licence** — five
/// `Valid`s, the quorum door, basis 3, every seat served, and still held; Final releases it.
#[test]
fn u1_a_c7_claim_holds_the_escrow_to_final_on_a_full_service_licence() {
    let p = t12();
    let (_, id2m) = model_classes(&p);
    assert_eq!(p.palw_rcore_conservative_classes, &[id2m], "the premise: C7 is the 2M row");
    let mut c = model_chain(p, id2m, 1);
    let before = c.reserved(&bond_key(1));
    let id = model_claim(&mut c, id2m, 1, 0x2E);
    let accepted = c.claim(&id);
    let (w, e) = (accepted.reserved, escrow(&c.sp, &accepted));
    let seats = honest_seats(&c.p, 5);
    c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
    let bound = c.bind(id, &seats);
    let receipts: Vec<_> = seats.iter().map(|(k, _)| valid(id, *k, bound)).collect();
    c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
    let licensed = c.claim(&id);
    assert_eq!(
        (licensed.rcore.licence_door, licensed.rcore.basis_k, licensed.rcore.served_mask, licensed.rcore.unserved_seen),
        (Some(PalwLicenceDoorTagV1::Quorum), 3, 0b1_1111, false),
        "every other release condition holds"
    );
    assert!(!licensed.rcore.escrow_released, "C7 holds the escrow");
    assert_eq!(c.reserved(&bond_key(1)), before + w + e);
    c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
    c.finalize(id);
    assert_eq!(c.reserved(&bond_key(1)), before, "Final releases w + E");
}

/// **T37 (S's half): the V1 and coverage doors tick, S2 does not; an S2 upgrade through the V2
/// door ticks once, records the door and — inside SR-1b's window — releases `E` (T74's V2 half).**
/// Fence off, every attempt licence ticks, as below the fence it always did.
#[test]
fn t37_t74_coverage_ticks_s2_does_not_and_its_v2_door_upgrade_ticks_and_releases() {
    for armed in [true, false] {
        let p = if armed { t12() } else { twin(&t12()) };
        // Coverage: the full seat and every partial, basis 2, all served.
        let mut c = Chain::new(p.clone());
        let (producer, _, _) = floor_producer(&c.p);
        let before = c.reserved(&producer);
        let id = c.floor_claim(0x37);
        let e = escrow(&c.sp, &c.claim(&id));
        let seats = c.floor_seats();
        let bound = c.bind(id, &seats);
        let settled = c.s.settled_attempt_finals();
        let receipts = covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound);
        c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);
        let cov = c.claim(&id);
        assert_eq!(c.s.settled_attempt_finals(), settled + 1, "armed={armed}: coverage ticks");
        if armed {
            assert_eq!(
                (cov.rcore.licence_door, cov.rcore.basis_k, cov.rcore.escrow_released),
                (Some(PalwLicenceDoorTagV1::Coverage), 2, true),
                "coverage recounts to 2 and releases on full service"
            );
            assert_eq!(c.reserved(&producer), before + cov.reserved, "E released");
        }

        // S2: the full seat and one partial, basis 1.
        let mut c = Chain::new(p.clone());
        let before = c.reserved(&producer);
        let id = c.floor_claim(0x38);
        let accepted = c.claim(&id);
        let (w, e2) = (accepted.reserved, escrow(&c.sp, &accepted));
        assert_eq!(e2, e, "the premise: same escrow");
        let seats = c.floor_seats();
        let bound = c.bind(id, &seats);
        let a = palw_segment_assignment_v2(c.anchor(&id), id, 5);
        let full = a.full_seat as usize;
        let partial = (full + 1) % 5;
        let settled = c.s.settled_attempt_finals();
        let receipts = covered(id, c.anchor(&id), &seats, &[full, partial], bound);
        c.step(&[PalwConsensusObjectV2::OptimisticLicensed { claim: id, receipts }]);
        let licensed_daa = c.daa;
        let s2 = c.claim(&id);
        if !armed {
            assert_eq!(c.s.settled_attempt_finals(), settled + 1, "fence off: an S2 attempt licence ticks as it always did");
            assert_eq!(s2.rcore, PalwClaimRcoreV1::default());
            continue;
        }
        assert_eq!(c.s.settled_attempt_finals(), settled, "V-8: an S2 licence does not settle the anchor");
        assert_eq!(
            (s2.rcore.licence_door, s2.rcore.basis_k, s2.rcore.escrow_released),
            (Some(PalwLicenceDoorTagV1::Optimistic), 1, false),
            "S2 records basis 1 and holds"
        );
        assert!(!palw_rcore_counts_licensed_v1(&s2), "T-2(c): an S2 licence keeps its replay charged");
        assert_eq!(c.reserved(&producer), before + w + e, "E held");
        // The other three seats' full-replay V2 `Valid`s through the V2 door, at L + 60 (SR-1b's
        // last DAA on testnet-12): basis 1 → 3, the door recorded Coverage (a partial counted), the
        // anchor settles once, and every seat has served, so E is released in this block.
        assert_eq!(c.sp.window_challenge_at(licensed_daa) / 2, 60, "the premise: testnet-12's SR-1b window is L + 60");
        let rest: Vec<_> = (0..5).filter(|i| *i != full && *i != partial).map(|i| valid(id, seats[i].0, bound)).collect();
        c.step_at(
            licensed_daa + 60,
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: rest }],
            PalwBlockWorkV3::None,
            Hash64::default(),
            0,
        );
        let up = c.claim(&id);
        assert!(s2.rcore.g_res_sompi > 0, "the S2 licence recorded its G_res");
        assert_eq!(
            up.rcore,
            PalwClaimRcoreV1 {
                licence_door: Some(PalwLicenceDoorTagV1::Coverage),
                basis_k: 3,
                escrow_released: true,
                served_mask: 0b1_1111,
                unserved_seen: false,
                g_res_sompi: s2.rcore.g_res_sompi,
            },
            "the upgrade (the V2 door keeps the first licence's G_res: Q-4)"
        );
        assert_eq!(c.s.settled_attempt_finals(), settled + 1, "the upgrade settles the anchor once (V-8)");
        assert!(palw_rcore_counts_licensed_v1(&up), "T-2(c): the upgrade frees the replay");
        assert_eq!(c.reserved(&producer), before + w, "SR-1b: E released in the upgrade's block");
        c.finalize(id);
        assert_eq!(c.reserved(&producer), before);
    }
}

/// **T74 (V2 door): SR-1b at `L + 60` flips, at `L + 61` it does not — and a late completion never
/// flips, whatever it serves (SR-4 / T14).** A V1 licence by three `Valid`s holds `E`; the other two
/// seats' `Valid`s complete the service through the V2 door.
#[test]
fn t74_sr1b_flips_at_l_plus_60_and_not_at_l_plus_61() {
    for (late, flips) in [(60u64, true), (61, false)] {
        let mut c = Chain::new(t12());
        let (producer, _, _) = floor_producer(&c.p);
        let before = c.reserved(&producer);
        let id = c.floor_claim(0x74 + late);
        let accepted = c.claim(&id);
        let (w, e) = (accepted.reserved, escrow(&c.sp, &accepted));
        let seats = c.floor_seats();
        let bound = c.bind(id, &seats);
        let first: Vec<_> = seats[..3].iter().map(|(k, _)| valid(id, *k, bound)).collect();
        c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: first }]);
        let licensed_daa = c.daa;
        assert!(!c.claim(&id).rcore.escrow_released, "three of five hold");
        let settled = c.s.settled_attempt_finals();
        let rest: Vec<_> = seats[3..].iter().map(|(k, _)| valid(id, *k, bound + 1)).collect();
        c.step_at(
            licensed_daa + late,
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: rest }],
            PalwBlockWorkV3::None,
            Hash64::default(),
            0,
        );
        let after = c.claim(&id);
        assert_eq!(after.rcore.served_mask, 0b1_1111, "L+{late}: the door serves");
        assert_eq!(after.rcore.basis_k, 3, "the recount stays capped at 3");
        assert_eq!(after.rcore.escrow_released, flips, "L+{late}");
        assert_eq!(c.reserved(&producer), before + w + if flips { 0 } else { e }, "L+{late}: E released iff flipped");
        assert_eq!(c.s.settled_attempt_finals(), settled, "basis 3 was already 3: no second tick");
        c.finalize(id);
        assert_eq!(c.reserved(&producer), before);
        assert_eq!(c.claim(&id).rcore.escrow_released, flips, "and no un-flip");
    }
}

/// **The staged record's load invariants (S-SPEC §3.2, §3.8)**: past the fence a load refuses a
/// licensed claim with no door, a released escrow whose release was not due, and an R-core+ record
/// on a claim that has not licensed; the ledger re-derivation refuses a flag whose escrow term the
/// ledger still holds. The fence-off twin asks none of these (its records are all `Default`).
#[test]
fn the_load_refuses_a_staged_record_the_fold_could_not_have_written() {
    let mut c = Chain::new(t12());
    let id = c.floor_claim(0x10AD);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    let bound_state = c.s.clone();
    let receipts: Vec<_> = seats[..3].iter().map(|(k, _)| valid(id, *k, bound)).collect();
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
    let held = c.s.clone();
    assert!(!held.claim(&id).unwrap().rcore.escrow_released, "the premise: three of five hold");
    let refused = |s: &PalwChainStateV2, edit: &dyn Fn(&mut PalwStateCarriageV2)| {
        let mut carriage = PalwStateCarriageV2::from_state(s);
        edit(&mut carriage);
        carriage.into_state(&c.sp, None).expect_err("the load refuses it").to_string()
    };
    let why = refused(&held, &|k| k.claims.get_mut(&id).unwrap().rcore.licence_door = None);
    assert!(why.contains("records no licence door"), "{why}");
    let why = refused(&held, &|k| k.claims.get_mut(&id).unwrap().rcore.escrow_released = true);
    assert!(why.contains("without the release being due") || why.contains("reserved_exposure"), "{why}");
    let why = refused(&bound_state, &|k| k.claims.get_mut(&id).unwrap().rcore.basis_k = 3);
    assert!(why.contains("has not licensed"), "{why}");
    // A released flag on a record whose release IS due, with the escrow term still on the ledger:
    // the ledger re-derivation (SR-3) catches the missing release.
    let why = refused(&held, &|k| {
        let r = &mut k.claims.get_mut(&id).unwrap().rcore;
        r.served_mask = 0b1_1111;
        r.escrow_released = true;
    });
    assert!(why.contains("reserved_exposure differs"), "{why}");
}
