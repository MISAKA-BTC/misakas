//! Exact outside-verifier discovery through the same public claim and reservation predicates.
use super::*;
use crate::palw_legacy_public_filer_v1::*;

#[test]
fn explicit_public_target_survives_old_claim_flood_and_preserves_native_eligibility() {
    let p = params().with_legacy_public_filer_from_daa(Some(0));
    let (mut state, target, _, _) = f2_licensed_false_execution(&p);
    let original = state.claim(&target).unwrap().clone();
    for number in 0..5000 {
        let mut row = original.clone();
        row.accepted_daa = 100;
        state.claims.insert(h64(0xC100 + number), row);
    }
    assert!(state.palw_fraud_filer_candidates_v1(&p, &bond_key(7), 104).len() > 4096);
    let selected = state.palw_fraud_filer_candidates_for_claims_v1(&p, &bond_key(7), 104, &[target]);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].claim_id, target);
    assert!(!selected[0].seat);
    assert!(state.palw_fraud_filer_candidates_for_claims_v1(&p, &original.bond, 104, &[target]).is_empty());
    assert!(
        state
            .palw_fraud_filer_candidates_for_claims_v1(
                &p.clone().with_legacy_public_filer_from_daa(None),
                &bond_key(7),
                104,
                &[target]
            )
            .is_empty()
    );
    let expired = palw_dispute_hard_deadline_v1(&original, palw_da_disclose_window_daa_v1(&p)).saturating_add(1);
    assert!(
        state.palw_fraud_filer_candidates_for_claims_v1(&p, &bond_key(7), expired, &[target]).is_empty(),
        "an expired claim cannot acquire eligibility solely from local targeting"
    );
    state.claims.get_mut(&target).unwrap().phase =
        PalwClaimPhaseV2::Voided { voided_daa: 104, reason: PalwVoidReasonV2::ReceiptTimeout };
    assert!(
        state.palw_fraud_filer_candidates_for_claims_v1(&p, &bond_key(7), 105, &[target]).is_empty(),
        "a configured ID cannot make an unreservable claim eligible"
    );
    assert!(
        state.palw_fraud_filer_candidates_for_claims_v1(&p, &bond_key(7), 105, &vec![target; 17]).is_empty(),
        "an oversized exact request is refused, never silently truncated"
    );
}

#[test]
fn explicit_lookup_reconstructs_its_own_admitted_reservations_after_carriage_restart() {
    let p = params().with_legacy_public_filer_from_daa(Some(0));
    let (licensed, claim, _, _) = f2_licensed_false_execution(&p);
    let row = licensed.claim(&claim).unwrap();
    let reserve = PalwConsensusObjectV2::DisputeReservedV1 {
        reservation: Box::new(PalwDisputeReservationV1 {
            version: 1,
            claim,
            execution_root: row.execution_root,
            trace_root: row.trace_root,
            reserver: bond_key(7),
        }),
        signature: vec![1],
    };
    let (accepted, _) = f2_apply(&licensed, &p, &ctx(4, 104, 104), &[seat_bond_reg(7, 1000), reserve], &f2_extras()).unwrap();
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&accepted)).unwrap();
    let restarted = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&p, Some(accepted.state_root())).unwrap();
    let candidates = restarted.palw_fraud_filer_candidates_for_claims_v1(&p, &bond_key(7), 105, &[h64(0xBAD)]);
    assert_eq!(candidates.len(), 1, "a new local target cannot erase an already admitted pursuit on restart");
    assert_eq!(candidates[0].claim_id, claim);
    assert_eq!(restarted.palw_fraud_filer_status_v1(&p, &bond_key(7), 105).unwrap().live[0].claim_id, claim);
}
