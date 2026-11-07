//! **ADR-0152 Phase 2, T29's processor half: a same-block conviction and maturity** — a child of
//! F2's suite (`t46_false_valid_real_claim`), so the claim is a REAL producer-built floor claim,
//! licensed by real V3 receipts, and the conviction a real `PanelFalseValidV2` that takes the whole
//! path an object takes into a block: the gate, the acceptance rehearsal and the fold (`H::carry`).
//!
//! The claim is swept to `Final`, so a vesting row names its reward (V-2); the row is then re-keyed
//! through the carriage (its DAA clock runs out at the next block, the second clock's licences in)
//! so that the next block's step 3d would latch and move it. Two twins of that next block:
//!
//! * **no object** — step 3d latches the row and moves it: its producer leg lands on its A-KEY key;
//! * **the conviction** — the rehearsal accepts it (its pre-object base is the fold's step 2, where
//!   the row still stands: phase2-plan F4's first invariant), and the fold burns the row at step 3,
//!   through S-4's funnel, BEFORE 3d runs: no latch, no move, no queue row, the whole row counted
//!   `burned` (T12's order, V-5).
//!
//! The fold half of T12 is Phase 1's (`vesting_fold_v1`); this is the processor's rehearsal and fold
//! agreeing on it.
use super::*;
use kaspa_consensus_core::palw_vesting_v1::{
    PalwVestingNoteV1, PalwVestingSourceV1, palw_vesting_notes_of_delta_v1, palw_vesting_payout_key_v1, palw_vesting_row_maturity_v1,
};

/// **T29 / T12 at the processor: a conviction and a maturity in one block — the burn comes first,
/// and the rehearsal and the fold agree on it.**
#[tokio::test]
async fn p2_t29_a_same_block_conviction_burns_the_row_its_maturity_would_have_moved() {
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    h.sweep_to_final(&mut walk, id);
    let at_final = walk.state.clone();
    let row = at_final.vesting_row(&id).expect("past palw_rcore_plus a Final names a vesting row").clone();
    assert!(row.matured_at.is_none() && row.expiry_daa > walk.daa, "the row is young: its DAA clock runs to F + window_court");

    // Re-key the row so that the next block matures it: its DAA clock out at that block, and the
    // settled count past `settled_at_final + depth`.
    let next = walk.next();
    let depth = h.config.params.palw_settled_anchor_depth.expect("testnet-12 runs the second clock");
    let settled = at_final.settled_attempt_finals().max(depth);
    walk.state = h.rebuilt(&at_final, |c| {
        c.settled_attempt_finals = settled;
        let r = c.vesting.get_mut(&id).expect("the row");
        r.expiry_daa = next.daa_score;
        r.settled_at_final = settled - depth;
    });
    let parent = walk.state.clone();
    let rekeyed = parent.vesting_row(&id).expect("the row").clone();
    assert!(
        palw_vesting_row_maturity_v1(&parent, h.sp(), &rekeyed, next.daa_score, Some(depth)).mature_now,
        "the next block's step 3d matures the row"
    );
    let a_key = palw_vesting_payout_key_v1(&id);

    // Twin 1: an empty block — latched and moved in one step 3d.
    let mut quiet = walk.clone();
    let (_, delta) = h.carry(&mut quiet, Vec::new());
    let notes: Vec<PalwVestingNoteV1> = palw_vesting_notes_of_delta_v1(&delta).cloned().collect();
    assert!(notes.contains(&PalwVestingNoteV1::Latched { claim_id: id, matured_at: next.daa_score }), "latched at 3d");
    assert!(
        notes
            .iter()
            .any(|n| matches!(n, PalwVestingNoteV1::Moved { source: PalwVestingSourceV1::Row { claim_id }, .. } if *claim_id == id)),
        "and moved in the same step"
    );
    assert!(quiet.state.vesting_row(&id).is_none());
    assert_eq!(quiet.state.pending_payout(&a_key).map(|p| p.amount), Some(rekeyed.producer.amount), "the producer leg is queued");

    // Twin 2: the same block carrying a real kind-3 conviction of the claim.
    let full = licence.full_card();
    let conviction = h.v2(full, id, licence.segmented(full), claim.contradiction());
    let mut convicted = walk.clone();
    let (_, delta) = h.carry(&mut convicted, vec![conviction]);
    let notes: Vec<PalwVestingNoteV1> = palw_vesting_notes_of_delta_v1(&delta).cloned().collect();
    let burned = notes
        .iter()
        .position(|n| matches!(n, PalwVestingNoteV1::Burned { claim_id, .. } if *claim_id == id))
        .expect("the conviction burned the row (S3 through S-4's funnel)");
    match &notes[burned] {
        PalwVestingNoteV1::Burned { sompi, .. } => assert_eq!(*sompi, rekeyed.total_sompi(), "the whole row"),
        _ => unreachable!(),
    }
    assert!(
        !notes.iter().any(|n| matches!(
            n,
            PalwVestingNoteV1::Latched { claim_id, .. } | PalwVestingNoteV1::Moved { source: PalwVestingSourceV1::Row { claim_id }, .. }
                if *claim_id == id
        )),
        "step 3d never saw the row: the burn at step 3 comes first"
    );
    let s = &convicted.state;
    assert!(s.vesting_row(&id).is_none() && s.pending_payout(&a_key).is_none(), "neither a row nor a queued leg");
    assert_eq!(
        s.vesting_counters().burned - parent.vesting_counters().burned,
        rekeyed.total_sompi_u128(),
        "counted burned, never moved"
    );
    assert_eq!(s.vesting_counters().moved, parent.vesting_counters().moved);
    assert!(matches!(s.claim(&id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Voided { .. })), "the Final is reversed");
    h.reloads(s);
}
