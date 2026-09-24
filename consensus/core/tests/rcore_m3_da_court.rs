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
use kaspa_consensus_core::palw_state_v2::{PalwStateV2Error, palw_accuser_exposure_v1};

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
        // DL-1 owns the paused claim's deadline from Phase 2 on; here the record is only carried, so
        // the claim keeps the receipt deadline it was bound with — the loader's DL-1 reads the record
        // once M3's rows land, and this test is restated then (Phase 2).
        let loaded = loaded.expect("the carriage reloads");
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
