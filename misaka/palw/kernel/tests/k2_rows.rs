//! The row view of the ledger (`rows`): rebuilt from rows it is the same ledger, its root is computed from the rows alone and equals
//! the ledger's, and a block's journal (`diff_rows`) applies and reverts exactly.
mod common;
use common::chain::T;
use common::ledger_world::*;
use common::opv_world::*;
use misaka_palw_kernel::ledger::KernelLedgerV1;
use misaka_palw_kernel::rows::{LedgerRowsV1, config_root_of, diff_rows, root_of_rows};

fn check(w: &World) {
    let rows = w.l.to_rows();
    assert_eq!(
        root_of_rows(&w.l.policy, config_root_of(&w.l.schedule, &w.l.known), w.l.scalars(), w.l.opv_policy(), &rows),
        w.l.root(),
        "the rows root to the ledger's root"
    );
    let back = KernelLedgerV1::from_rows(&w.genesis, w.l.scalars(), &rows).unwrap();
    assert_eq!(back.root(), w.l.root(), "rebuilt from rows it is the same ledger");
    assert_eq!(back.to_rows(), rows);
}

#[test]
fn rows_round_trip_and_root_at_every_stage_of_a_prosecution() {
    let mut w = World::new();
    check(&w);
    let before = w.l.to_rows();
    let (id, at, da, _trace) = final_lying_claim(&mut w);
    check(&w);
    // The one-claim-per-job index is a table of its own and rooted (C4 F-C4-03).
    assert!(!w.l.job_claims.is_empty());
    assert!(w.l.to_rows().keys().any(|(t, _)| *t == misaka_palw_kernel::rows::TABLE_JOB_CLAIMS_V1));
    // A demand and a service in the mix (numeric order of the position key matters for the root).
    let _ = (at, da);
    let after = w.l.to_rows();
    let journal = diff_rows(&before, &after);
    assert!(!journal.is_empty());
    // Apply the journal to `before` and revert it from `after`.
    let mut forward: LedgerRowsV1 = before.clone();
    for (k, _old, new) in &journal {
        match new {
            Some(v) => forward.insert(k.clone(), v.clone()),
            None => forward.remove(k),
        };
    }
    assert_eq!(forward, after);
    let mut backward = after.clone();
    for (k, old, _new) in &journal {
        match old {
            Some(v) => backward.insert(k.clone(), v.clone()),
            None => backward.remove(k),
        };
    }
    assert_eq!(backward, before);
    assert!(w.l.claims.contains_key(&id));
}

#[test]
fn demand_positions_sort_numerically_not_by_their_little_endian_bytes() {
    use misaka_palw_kernel::ledger::DemandRowV1;
    let mut w = World::new();
    for position in [256u32, 1, 70_000, 2] {
        w.l.demands.insert(([7; 64], 0, position), DemandRowV1 { demanders: vec![([9; 64], 10)], filed_daa: 1, deadline_daa: 21, last: None });
    }
    check(&w);
}

/// RFC-0015: an OPV ledger's rows (the three OPV tables), its OPV root form, and the derived live index all survive the round trip,
/// at every stage of an OPV claim's life; a ledger with no OPV policy keeps the historical root form.
#[test]
fn opv_rows_round_trip_and_root_in_the_opv_root_form() {
    let mut w = World::new_opv();
    assert!(w.l.opv_policy().is_some());
    check(&w);
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying_by(&job, PRODUCER, 3);
    let id = lie.claim.id();
    w.block(10, vec![lie.tx]);
    check(&w);
    assert!(w.l.opv.claims.contains_key(&id) && w.l.opv_live_counts(&PRODUCER).0 == 1);
    let rows = w.l.to_rows();
    for table in [
        misaka_palw_kernel::rows::TABLE_OPV_ADMITTED_V1,
        misaka_palw_kernel::rows::TABLE_OPV_CLASSES_V1,
        misaka_palw_kernel::rows::TABLE_OPV_CLAIMS_V1,
    ] {
        assert!(rows.keys().any(|(t, _)| *t == table), "OPV table {table} has a row");
    }
    // The rebuilt ledger recomputes the live index from the rows.
    let back = KernelLedgerV1::from_rows(&w.genesis, w.l.scalars(), &rows).unwrap();
    assert_eq!(back.opv_live_counts(&PRODUCER), w.l.opv_live_counts(&PRODUCER));
    back.opv_invariants().unwrap();
    // The window closes: Final.
    w.block(70, vec![]);
    check(&w);
    // Without a policy the root is the historical form (the rows carry no OPV table).
    let plain = World::new();
    assert!(plain.l.to_rows().keys().all(|(t, _)| *t < misaka_palw_kernel::rows::TABLE_OPV_ADMITTED_V1));
    assert_eq!(plain.l.root(), plain.l.root_parts().root());
    let _ = T::PanelCovered { claim: id };
}
