//! **Lane BUDGET (ADR-0176 / ADR-0177): the bond budget through the fold** (design `docs/design/palw/bond-budget-and-model-allocation.md`).
//!
//! Milestone 2 pins the dormant layout: the unarmed fold byte for byte (fence absent, or scheduled ahead), the engine created by the fence's
//! first block and carried by delta, root and carriage, the old live claims seeded into the window, and the capital assignment (tag 140).
//! Every policy here is a TEST value, not a proposal.

use super::*;
use crate::palw_bond_budget_v1::*;

/// A TEST policy: per 1,000 base units of capital and W = 50 DAA, 2 claims, 2 blocks, 2,000 of reward and 1,000 of weight.
fn test_policy(rho: u32) -> PalwBondBudgetPolicyV1 {
    PalwBondBudgetPolicyV1 {
        version: PALW_BOND_BUDGET_POLICY_VERSION_V1,
        window_daa: 50,
        capital_unit_sompi: 1_000,
        rho,
        claims_per_unit: 2,
        block_units_per_unit: 2 * PALW_BUDGET_BLOCK_UNIT_V1,
        reward_per_unit_sompi: 2_000,
        final_weight_per_unit: 1_000,
        max_open_claims_per_bond: 1_000,
        slice_rights_by_rho: false,
    }
}

/// A TEST allocation policy: epochs of 10 DAA, one epoch of seasoning, `f` linear to 1,000 and flat after.
fn test_allocation() -> PalwModelAllocationPolicyV1 {
    PalwModelAllocationPolicyV1 {
        version: PALW_MODEL_ALLOCATION_POLICY_VERSION_V1,
        epoch_daa: 10,
        seasoning_epochs: 1,
        curve: PalwAllocationCurveV1 { points: vec![(0, 0), (1_000, 1_000)] },
        max_models_per_bond: 4,
    }
}

fn mirror(from: u64, allocation_from: Option<u64>) -> PalwBondBudgetMirrorV1 {
    PalwBondBudgetMirrorV1 { from_daa: from, policy: test_policy(1), allocation: allocation_from.map(|at| (at, test_allocation())) }
}

fn carriage_bytes(state: &PalwChainStateV2) -> Vec<u8> {
    borsh::to_vec(&PalwStateCarriageV2::from_state(state)).expect("a carriage serializes")
}

fn is_budget_entry(entry: &PalwDeltaEntryV2) -> bool {
    matches!(entry, PalwDeltaEntryV2::BondBudgetRow { .. } | PalwDeltaEntryV2::BondBudgetHeader { .. })
}

/// The escrow lattice (`palw_v2_escrow_is_carved_once_and_paid_once`'s walk): register, accept an attempt on a 1,000 subsidy, bind,
/// license, mature to Final, pay, idle — every state and delta.
fn walk(p: &PalwStateParamsV2) -> Vec<(PalwChainStateV2, PalwStateDeltaV2)> {
    let genesis = PalwChainStateV2::genesis();
    let env = attempt(40, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let mut out = Vec::new();
    let step = |out: &mut Vec<(PalwChainStateV2, PalwStateDeltaV2)>,
                c: PalwBlockContextV2,
                objects: &[PalwConsensusObjectV2],
                att: Option<&PalwAttemptEnvelopeV2>| {
        let parent = out.last().map(|(s, _): &(PalwChainStateV2, PalwStateDeltaV2)| s.clone()).unwrap_or_else(|| genesis.clone());
        out.push(apply(&parent, p, &c, objects, att));
    };
    step(&mut out, ctx(1, 100, 1), &register_class_and_bond(), None);
    step(&mut out, PalwBlockContextV2 { subsidy: 1_000, ..ctx(2, 101, 2) }, &[], Some(&env));
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: op_id(21) }];
    step(&mut out, ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
    step(&mut out, ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: seat_says(true) }], None);
    step(&mut out, PalwBlockContextV2 { subsidy: 9_999_999, ..ctx(5, 124, 5) }, &[], None);
    step(&mut out, ctx(6, 125, 6), &[], None);
    step(&mut out, ctx(7, 200, 7), &[], None);
    out
}

/// **The unarmed fold is byte-identical** — with the fence absent (every preset) and with it scheduled at a height the chain has not
/// reached: the same deltas, roots and carriage bytes, no engine, no delta 190/191, no `bond_budget/v1` block, no tail `0xEF`. (The
/// existing golden vectors pin that the absent case is the tree before this lane.)
#[test]
fn the_unarmed_fold_is_byte_identical_whether_the_fence_is_absent_or_scheduled_ahead() {
    let base = params().with_worker_carve_permille(620).unwrap();
    let ahead = base.clone().with_bond_budget(Some(mirror(1_000_000, Some(1_000_000))));
    let (a, b) = (walk(&base), walk(&ahead));
    assert_eq!(a.len(), b.len());
    for ((sa, da), (sb, db)) in a.iter().zip(&b) {
        assert_eq!(da, db, "the same delta");
        assert_eq!(sa.state_root(), sb.state_root(), "the same root");
        assert_eq!(carriage_bytes(sa), carriage_bytes(sb), "the same carriage bytes");
        assert!(sa.bond_budget().is_none() && sb.bond_budget().is_none(), "no engine below the fence");
        assert!(!da.entries.iter().any(is_budget_entry), "no budget delta below the fence");
        assert!(!sa.state_root_preimage().windows(14).any(|w| w == b"bond_budget/v1"), "no root block below the fence");
    }
    // And the escrow is paid whole, as before (the walk is the escrow test's).
    let (s5, _) = &a[4];
    assert_eq!(s5.pending_payouts_iter().map(|(_, pay)| pay.amount).sum::<u64>(), 620);
}

/// **The fence's first block creates the engine** (delta 191, `None → Some`), and from then on it is in the root (`bond_budget/v1`), in
/// the carriage (tail `0xEF`, round-tripped through an import that checks the root), and reverted exactly by its delta.
#[test]
fn the_fences_first_block_creates_the_engine_and_it_rides_delta_root_and_carriage() {
    let base = params().with_worker_carve_permille(620).unwrap();
    let armed = base.clone().with_bond_budget(Some(mirror(100, Some(100))));
    let genesis = PalwChainStateV2::genesis();
    let (u1, _) = apply(&genesis, &base, &ctx(1, 100, 1), &register_class_and_bond(), None);
    let (s1, d1) = apply(&genesis, &armed, &ctx(1, 100, 1), &register_class_and_bond(), None);
    let budget = s1.bond_budget().expect("created at the fence's first block");
    assert_eq!(budget.header.created_daa, 100);
    assert_eq!(budget.header.policy_digest, test_policy(1).digest());
    assert_eq!(budget.header.allocation.map(|a| a.index), Some(0), "the allocation's first epoch opened with it");
    assert!(d1.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::BondBudgetHeader { old: None, new: Some(_) })));
    assert_ne!(u1.state_root(), s1.state_root(), "the engine is in the root once it exists");
    assert!(s1.state_root_preimage().windows(14).any(|w| w == b"bond_budget/v1"));
    // The carriage: one more tail, and an import that re-derives the root takes it back whole.
    let bytes = carriage_bytes(&s1);
    assert!(bytes.len() > carriage_bytes(&u1).len());
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("decodes");
    assert_eq!(carriage.into_state(&armed, Some(s1.state_root())).expect("imports"), s1);
    // A second 0xEF tail is refused (the decoder takes each tail once).
    let mut twice = bytes.clone();
    twice.push(PALW_CARRIAGE_BOND_BUDGET_TAIL_V1);
    twice.extend(borsh::to_vec(budget).unwrap());
    assert!(borsh::from_slice::<PalwStateCarriageV2>(&twice).is_err());
    // The delta: forward from the parent, and back from the child.
    assert_eq!(apply_delta_v2(&genesis, &d1, &armed).unwrap(), s1);
    assert_eq!(revert_delta_v2(&s1, &d1, &armed).unwrap(), genesis);
}

/// **Migration (design §2.9): the fence's first block seeds every live old claim** into its bond's window at its OWN acceptance DAA, as a
/// Legacy reservation — counted, never consumed: the old claim is paid by the rule it was accepted under, and the bond's window frees only
/// at `accepted + W`.
#[test]
fn old_live_claims_seed_the_window_at_the_fence_and_leave_it_at_their_own_accepted_plus_w() {
    let base = params().with_worker_carve_permille(620).unwrap();
    // The fence at 102: the attempt (accepted at 101) is an old claim.
    let armed = base.clone().with_bond_budget(Some(mirror(102, None)));
    let walked = walk(&armed);
    let env = attempt(40, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let (s2, _) = &walked[1];
    assert!(s2.bond_budget().is_none(), "below the fence: nothing");
    let (s3, d3) = &walked[2];
    let budget = s3.bond_budget().expect("the fence's first block");
    let row = budget.claim_row(&claim_id).expect("the old claim is seeded");
    assert_eq!(row.origin, PalwBudgetOriginV1::Legacy);
    assert_eq!((row.accepted_daa, row.reuse_not_before), (101, 151));
    assert_eq!(row.reserved.reward_sompi, 620);
    assert_eq!(row.reserved.block_units, PALW_BUDGET_BLOCK_UNIT_V1);
    assert!(budget.window_holds(&bond_key(1), 150));
    assert!(d3.entries.iter().any(is_budget_entry));
    // Old rules pay it whole (the walk's Final at 124).
    let (s5, _) = &walked[4];
    assert_eq!(s5.pending_payouts_iter().map(|(_, pay)| pay.amount).sum::<u64>(), 620, "an old claim is paid by its old rule");
    // The window frees at 151, not before (the walk's last block is at 200).
    let (s6, _) = &walked[5];
    assert!(s6.bond_budget().unwrap().window_holds(&bond_key(1), 125));
    let (s7, _) = &walked[6];
    assert!(!s7.bond_budget().unwrap().window_holds(&bond_key(1), 200));
    assert!(s7.bond_budget().unwrap().bond_row(&bond_key(1)).is_none_or(|row| row.window.is_zero()));
}

fn assignment(bond: PalwBondKeyV2, assignments: Vec<(Hash64, u64)>, sequence: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondCapitalAssignedV1 { bond, assignments, sequence, signature: vec![1] }
}

/// **Tag 140 (ADR-0177 D3)**: refused below the allocation fence (and below the budget's), refused for an unknown model, capital beyond
/// the bond's, a replayed sequence; accepted otherwise, counted in `S_m` only once seasoned.
#[test]
fn a_capital_assignment_is_refused_below_its_fence_and_by_d3_and_counts_once_seasoned() {
    let base = params().with_worker_carve_permille(620).unwrap();
    let genesis = PalwChainStateV2::genesis();
    let (s1, _) = apply(&genesis, &base, &ctx(1, 100, 1), &register_class_and_bond(), None);
    let ok = assignment(bond_key(1), vec![(h64(1), 600)], 1);
    // No fence, the budget without the allocation: refused by name (the acceptance rehearsal drops it).
    for p in [base.clone(), base.clone().with_bond_budget(Some(mirror(100, None)))] {
        let refused = apply_palw_transition_v2(&s1, &p, &ctx(2, 101, 2), std::slice::from_ref(&ok), None);
        assert!(matches!(refused, Err(PalwStateV2Error::BondBudgetRefused(_))), "{refused:?}");
    }
    let armed = base.clone().with_bond_budget(Some(mirror(100, Some(100))));
    let (a1, _) = apply(&genesis, &armed, &ctx(1, 100, 1), &register_class_and_bond(), None);
    for (bad, why) in [
        (assignment(bond_key(1), vec![(h64(0xDEAD), 600)], 1), "an unregistered model"),
        (assignment(bond_key(1), vec![(h64(1), 1_200)], 1), "more than the bond's capital"),
        (assignment(bond_key(1), vec![(h64(1), 0)], 1), "a zero amount"),
        (assignment(bond_key(9), vec![(h64(1), 1)], 1), "a bond this chain does not have"),
    ] {
        assert!(apply_palw_transition_v2(&a1, &armed, &ctx(2, 101, 2), &[bad], None).is_err(), "{why}");
    }
    let (a2, d2) = apply(&a1, &armed, &ctx(2, 101, 2), std::slice::from_ref(&ok), None);
    assert!(d2.entries.iter().any(is_budget_entry));
    assert_eq!(a2.bond_budget().unwrap().assignment_row(&bond_key(1)).unwrap().pending, Some(vec![(h64(1), 600)]));
    assert!(apply_palw_transition_v2(&a2, &armed, &ctx(3, 102, 3), std::slice::from_ref(&ok), None).is_err(), "a replayed sequence");
    // Epoch 0 = [100, 110). Accepted in epoch 0, seasoned through epoch 1: counted from epoch 2 (DAA 120).
    let (a3, _) = apply(&a2, &armed, &ctx(3, 115, 3), &[], None);
    assert!(a3.bond_budget().unwrap().model_row(&h64(1)).is_none(), "an increase waits a full epoch");
    let (a4, _) = apply(&a3, &armed, &ctx(4, 121, 4), &[], None);
    let row = a4.bond_budget().unwrap().model_row(&h64(1)).copied().expect("seasoned");
    assert_eq!((row.epoch, row.capital, row.weight), (2, 600, 600));
    // The realized carve accrues per chain block (a zero-subsidy fixture block accrues zero).
    let (a5, _) = apply(&a4, &armed, &PalwBlockContextV2 { subsidy: 1_000, ..ctx(5, 122, 5) }, &[], None);
    assert_eq!(a5.bond_budget().unwrap().header.allocation.unwrap().accrued_sompi, 620);
    assert_eq!(a5.bond_budget().unwrap().model_available(&h64(1)), 620, "the only model holds the whole share");
    // Every step reverts to its parent.
    for (parent, (child, delta)) in [(&a4, apply(&a4, &armed, &PalwBlockContextV2 { subsidy: 1_000, ..ctx(5, 122, 5) }, &[], None))] {
        assert_eq!(&revert_delta_v2(&child, &delta, &armed).unwrap(), parent);
    }
}

/// **Tail `0xEF` is the bond budget's and nobody else's** — disjoint from the tails the neighbouring lanes took.
#[test]
fn the_bond_budget_tail_is_0xef_and_disjoint() {
    assert_eq!(PALW_CARRIAGE_BOND_BUDGET_TAIL_V1, 0xEF);
    for other in [
        crate::palw_kernel_route_v1::PALW_CARRIAGE_KERNEL_ROUTE_TAIL_V1,
        PALW_CARRIAGE_PANEL_V3_TAIL_V1,
        PALW_CARRIAGE_SEAT_ROOT_READINESS_TAIL_V1,
        PALW_CARRIAGE_FLOOR_STATE_TAIL_V1,
        PALW_CARRIAGE_SEAT_AVAILABILITY_TAIL_V1,
        PALW_CARRIAGE_TIR_SHARD_TAIL_V1,
        PALW_CARRIAGE_VERTEX_TAIL_V1,
        PALW_CARRIAGE_CLASS_COURT_WINDOWS_TAIL_V1,
    ] {
        assert_ne!(PALW_CARRIAGE_BOND_BUDGET_TAIL_V1, other);
    }
    // The object tag and the delta numbers are the Lead's allocation, pinned by value.
    let object = PalwConsensusObjectV2::BondCapitalAssignedV1 {
        bond: bond_key(1),
        assignments: Vec::new(),
        sequence: 0,
        signature: Vec::new(),
    };
    assert_eq!(borsh::to_vec(&object).unwrap()[0], PALW_CAPITAL_ASSIGNMENT_TAG_V1);
    assert!(palw_object_is_bond_budget_v1(&object));
}
