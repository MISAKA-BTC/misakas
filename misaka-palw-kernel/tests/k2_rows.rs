//! The row view of the ledger (`rows`): rebuilt from rows it is the same ledger, its root is computed from the rows alone and equals
//! the ledger's, and a block's journal (`diff_rows`) applies and reverts exactly.
mod common;
use common::ledger_world::*;
use misaka_palw_kernel::ledger::KernelLedgerV1;
use misaka_palw_kernel::rows::{LedgerRowsV1, config_root_of, diff_rows, root_of_rows};

fn check(w: &World) {
    let rows = w.l.to_rows();
    assert_eq!(
        root_of_rows(&w.l.policy, config_root_of(&w.l.schedule, &w.l.known), w.l.scalars(), &rows),
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
