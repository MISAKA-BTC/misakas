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

// ---- Milestone 3: the reward paths through the fold --------------------------------------------------------------------------

fn armed_with(policy: PalwBondBudgetPolicyV1, allocation_from: Option<u64>) -> PalwStateParamsV2 {
    params().with_worker_carve_permille(620).unwrap().with_bond_budget(Some(PalwBondBudgetMirrorV1 {
        from_daa: 100,
        policy,
        allocation: allocation_from.map(|at| (at, test_allocation())),
    }))
}

fn sliced(rho: u32) -> PalwBondBudgetPolicyV1 {
    PalwBondBudgetPolicyV1 { slice_rights_by_rho: true, ..test_policy(rho) }
}

/// Every step of a walk: the delta re-applies to the child and reverts to the parent, and the child's carriage reloads under its root.
fn assert_replayable(p: &PalwStateParamsV2, parent: &PalwChainStateV2, child: &PalwChainStateV2, delta: &PalwStateDeltaV2) {
    assert_eq!(&apply_delta_v2(parent, delta, p).unwrap(), child, "the delta is the transition");
    assert_eq!(&revert_delta_v2(child, delta, p).unwrap(), parent, "the delta reverts");
    let reloaded = PalwStateCarriageV2::from_state(child).into_state(p, Some(child.state_root())).expect("the carriage reloads");
    assert_eq!(&reloaded, child, "reload is the state");
}

/// **An attempt reserves `(Q 1, B one block, R its escrow, F its contribution)`, its immature weight is held within F, its Final credits
/// and pays only what it reserved — the rest of the withheld escrow is never named (never minted).** ρ = 250 with rights sliced: each
/// claim's ceilings are R 4 and F 2 (2,000 and 1,000 per unit over q·ρ = 500), against an escrow of 620 and a contribution of 40.
#[test]
fn an_attempts_reservation_bounds_its_immature_weight_its_final_weight_and_its_pay() {
    let p = armed_with(sliced(250), None);
    let walked = walk(&p);
    for pair in walked.windows(2) {
        assert_replayable(&p, &pair[0].0, &pair[1].0, &pair[1].1);
    }
    let claim_id = attempt_id_v2(&attempt(40, 1).attempt);
    let (s2, _) = &walked[1];
    let claim = s2.claim(&claim_id).expect("admitted");
    assert_eq!(claim.escrowed_reward, 620, "the coinbase withholds the whole carve, as before");
    assert_eq!(claim.immature_contribution, 2, "β·pwu = 4, held within the reserved F = 2");
    assert_eq!(s2.bounded_immature(), 2);
    let row = *s2.bond_budget().unwrap().claim_row(&claim_id).expect("a budgeted claim");
    assert_eq!(
        row.reserved,
        PalwBudgetVectorV1 { claims: 1, block_units: PALW_BUDGET_BLOCK_UNIT_V1, reward_sompi: 4, final_weight: 2 }
    );
    assert_eq!(row.consumed.block_units, PALW_BUDGET_BLOCK_UNIT_V1, "its block was consumed at acceptance");
    assert_eq!((row.accepted_daa, row.reuse_not_before), (101, 151));
    let (s5, _) = &walked[4];
    assert!(matches!(s5.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    let paid: u64 = s5.pending_payouts_iter().map(|(_, pay)| pay.amount).sum();
    assert_eq!(paid, 4, "Final pays the reservation, not the escrow");
    assert_eq!(
        620 - paid,
        616,
        "the rest of the withheld carve is named nowhere: never minted (conservation: withheld = paid + unnamed)"
    );
    assert_eq!(s5.safe_weight(), 2, "Final credits the reserved F, not the contribution of 40");
    assert_eq!(s5.bond_budget_final_weight(&claim_id), Some(2), "what rule E reads (hook H-4)");
    let row = *s5.bond_budget().unwrap().claim_row(&claim_id).unwrap();
    assert!(!row.open && row.in_window, "closed at Final, still in its window");
    assert_eq!(
        (row.consumed.block_units, row.consumed.reward_sompi, row.consumed.final_weight),
        (row.reserved.block_units, row.reserved.reward_sompi, row.reserved.final_weight),
        "every reserved block, reward and weight unit was drawn — and no more"
    );
    // The unarmed twin pays the escrow and credits the contribution: the clip is the fence's alone.
    let unarmed = walk(&params().with_worker_carve_permille(620).unwrap());
    assert_eq!(unarmed[4].0.pending_payouts_iter().map(|(_, pay)| pay.amount).sum::<u64>(), 620);
    assert_eq!(unarmed[4].0.safe_weight(), 40);
}

fn attempt_at(
    p: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    c: PalwBlockContextV2,
    nonce: u64,
) -> (PalwChainStateV2, Option<Hash64>) {
    let env = attempt(40, nonce);
    let id = attempt_id_v2(&env.attempt);
    let (state, delta) = apply(parent, p, &c, &[], Some(&env));
    assert_replayable(p, parent, &state, &delta);
    let admitted = state.claim(&id).is_some();
    (state, admitted.then_some(id))
}

/// **No early recovery (ADR-0176 D4)**: a bond with Q = 2 holds two claims; one reaches Final early, the other voids (bind timeout); a
/// retry — a new attempt — is refused (skipped, the block standing) until the first claim's `accepted + W`, and admitted at it.
#[test]
fn early_final_void_and_retry_recover_nothing_before_d_plus_w() {
    let p = armed_with(test_policy(1), None);
    let genesis = PalwChainStateV2::genesis();
    let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
    let (s2, a) = attempt_at(&p, &s1, PalwBlockContextV2 { subsidy: 1_000, ..ctx(2, 101, 2) }, 1);
    let (s3, b) = attempt_at(&p, &s2, PalwBlockContextV2 { subsidy: 1_000, ..ctx(3, 102, 3) }, 2);
    let (a, b) = (a.expect("the first claim"), b.expect("the second claim"));
    let (s4, refused) = attempt_at(&p, &s3, PalwBlockContextV2 { subsidy: 1_000, ..ctx(4, 103, 4) }, 3);
    assert_eq!(refused, None, "Q = 2: the third is skipped and the block stands");
    // `a` is bound, licensed and Final at 124; `b` is never bound and voids at its bind deadline.
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: op_id(21) }];
    let (s5, _) = apply(&s4, &p, &ctx(5, 104, 5), &[PalwConsensusObjectV2::PanelBound { claim: a, anchor: h64(77), seats }], None);
    let (s6, _) =
        apply(&s5, &p, &ctx(6, 105, 6), &[PalwConsensusObjectV2::ReceiptLicensed { claim: a, receipts: seat_says(true) }], None);
    let (s7, _) = apply(&s6, &p, &ctx(7, 126, 7), &[], None);
    assert!(matches!(s7.claim(&a).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "early Final");
    assert!(matches!(s7.claim(&b).unwrap().phase, PalwClaimPhaseV2::Voided { .. }), "void");
    let budget = s7.bond_budget().unwrap();
    assert!(!budget.claim_row(&a).unwrap().open && !budget.claim_row(&b).unwrap().open, "both closed");
    assert_eq!(budget.bond_row(&bond_key(1)).unwrap().window.claims, 2, "closing returned nothing");
    // The retry before d + W: refused. (A Final producer holds no reservation, so only the window holds it.)
    let (s8, retry) = attempt_at(&p, &s7, PalwBlockContextV2 { subsidy: 1_000, ..ctx(8, 150, 8) }, 4);
    assert_eq!(retry, None, "no early recovery after an early Final and a void");
    assert!(palw_bond_backs_live_duty_v1(&s8, &bond_key(1), 150, None), "and the window holds the capital");
    // At a's d + W = 151 one claim's room returns — exactly one.
    let (s9, first) = attempt_at(&p, &s8, PalwBlockContextV2 { subsidy: 1_000, ..ctx(9, 151, 9) }, 5);
    assert!(first.is_some(), "a's room returns at 151");
    let (s10, second) = attempt_at(&p, &s9, PalwBlockContextV2 { subsidy: 1_000, ..ctx(10, 151, 10) }, 6);
    assert_eq!(second, None, "b's room returns at 152, not before");
    let (_, third) = attempt_at(&p, &s10, PalwBlockContextV2 { subsidy: 1_000, ..ctx(11, 152, 11) }, 7);
    assert!(third.is_some(), "b's room at 152");
    // The unarmed twin admits the third attempt at once (the budget alone refused it).
    let plain = params().with_worker_carve_permille(620).unwrap();
    let (u1, _) = apply(&genesis, &plain, &ctx(1, 100, 1), &register_class_and_bond(), None);
    let (u2, _) = attempt_at(&plain, &u1, PalwBlockContextV2 { subsidy: 1_000, ..ctx(2, 101, 2) }, 1);
    let (u3, _) = attempt_at(&plain, &u2, PalwBlockContextV2 { subsidy: 1_000, ..ctx(3, 102, 3) }, 2);
    assert!(attempt_at(&plain, &u3, PalwBlockContextV2 { subsidy: 1_000, ..ctx(4, 103, 4) }, 3).1.is_some());
}

/// **Same bond, same window, same ρ: a fast producer and a slow one reach the same ceilings** (ADR-0176 D1). Two bonds of equal capital;
/// one attempts every DAA (the forger's pace, no compute), the other every fifth (the honest pace). Both hold exactly Q = 2 claims per
/// window, the same B, R and F in it, and the fast one's extra attempts are skipped.
#[test]
fn same_bond_honest_and_forger_reach_equal_ceilings() {
    let p = armed_with(test_policy(1), None);
    let genesis = PalwChainStateV2::genesis();
    let mut objects = register_class_and_bond();
    objects.push(PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(2),
        pubkey: vec![8; 4],
        operator_pubkey: op_key(22),
        collateral: 1_000,
        payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A12),
        capable_classes: Default::default(),
        signature: Vec::new(),
    });
    let (mut s, _) = apply(&genesis, &p, &ctx(1, 100, 1), &objects, None);
    let (mut admitted, mut blue) = ([0u32; 2], 1u64);
    for d in 1..=40u64 {
        for (who, bond, pubkey, op) in [(0usize, 1u64, 7u8, 21u64), (1, 2, 8, 22)] {
            if who == 1 && d % 5 != 1 {
                continue;
            }
            blue += 1;
            let env = attempt_for_class(40, d * 10 + who as u64, h64(1), bond_key(bond), vec![pubkey; 4], op_id(op), h64(11));
            let id = attempt_id_v2(&env.attempt);
            let (next, delta) = apply(&s, &p, &PalwBlockContextV2 { subsidy: 1_000, ..ctx(blue, 100 + d, blue) }, &[], Some(&env));
            assert_replayable(&p, &s, &next, &delta);
            s = next;
            admitted[who] += u32::from(s.claim(&id).is_some());
        }
    }
    assert_eq!(admitted, [2, 2], "both bonds stop at Q = 2 inside one window, whatever their pace");
    let budget = s.bond_budget().unwrap();
    let (fast, slow) = (budget.bond_row(&bond_key(1)).unwrap().window, budget.bond_row(&bond_key(2)).unwrap().window);
    assert_eq!(fast, slow, "the same B, R and F in the window");
    let caps = palw_bond_budget_caps_v1(&test_policy(1), 1_000);
    assert!(fast.fits_within(&caps));
    assert_eq!(fast.block_units, 2 * PALW_BUDGET_BLOCK_UNIT_V1);
    assert_eq!(fast.reward_sompi, 2 * 620);
}

/// **ρ ×1/×100/×1000 move Q only** (ADR-0176 D1/D2): with blocks the binding dimension (lead attempts each take one), every ρ admits the
/// same two claims and holds the same B, R and F; the caps differ in Q alone.
#[test]
fn rho_sweep_keeps_b_r_and_f() {
    let mut seen = Vec::new();
    for rho in [1u32, 100, 1_000] {
        let policy = test_policy(rho);
        let p = armed_with(policy.clone(), None);
        let genesis = PalwChainStateV2::genesis();
        let (mut s, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let mut admitted = 0;
        for k in 0..6u64 {
            let (next, id) = attempt_at(&p, &s, PalwBlockContextV2 { subsidy: 1_000, ..ctx(2 + k, 101 + k, 2 + k) }, 10 + k);
            s = next;
            admitted += u32::from(id.is_some());
        }
        let caps = palw_bond_budget_caps_v1(&policy, 1_000);
        let window = s.bond_budget().unwrap().bond_row(&bond_key(1)).unwrap().window;
        assert_eq!(caps.claims, 2 * rho as u64, "Q = q·ρ");
        seen.push((
            admitted,
            window.block_units,
            window.reward_sompi,
            window.final_weight,
            caps.block_units,
            caps.reward_sompi,
            caps.final_weight,
        ));
    }
    assert!(seen.windows(2).all(|w| w[0] == w[1]), "B, R and F (held and capped) are invariant in ρ: {seen:?}");
    assert_eq!(seen[0].0, 2, "two blocks of B: two lead attempts");
}

/// **A free-prompt commitment reserves `quanta` blocks, carves and weights; its spends draw on that reservation** — B and R strictly
/// (a receipt block is paid whole by the coinbase), F clipped — and the commitment is refused outright when its blocks do not fit.
#[test]
fn free_prompt_spends_draw_the_commitments_reservation() {
    // ρ = 25 with rights sliced: the claim's F ceiling is 1,000 / 50 = 20 — one quantum's weight of the three.
    let p = armed_with(sliced(25), None);
    let certified = certify_fp_claim(&p, 40, 2);
    let row = *certified.bond_budget().unwrap().claim_row(&h64(0xFC)).expect("a budgeted commitment");
    assert_eq!(row.origin, PalwBudgetOriginV1::FreePrompt);
    assert_eq!(row.reserved.block_units, 2 * PALW_BUDGET_BLOCK_UNIT_V1, "one receipt block a quantum");
    assert_eq!(row.reserved.final_weight, 20, "2 × 20 asked (40 leaves, quanta of 20), the per-claim ceiling 20");
    assert_eq!(row.reserved.reward_sompi, 0, "the commit block's carve (a zero-subsidy fixture block)");
    let (s6, d6) = apply_work(&certified, &p, &ctx(6, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 0)));
    assert_replayable(&p, &certified, &s6, &d6);
    assert_eq!(s6.safe_weight(), 20, "the first quantum takes the whole reserved F");
    assert_eq!(s6.receipt_epoch_counter(&h64(1)).unwrap().produced_pwu, 20, "the census counts the work, not the grant");
    let (s7, _) = apply_work(&s6, &p, &ctx(7, 131, 7), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 1)));
    assert_eq!(s7.safe_weight(), 20, "the second quantum's weight is clipped to nothing");
    assert_eq!(s7.bond_budget_final_weight(&h64(0xFC)), Some(20));
    assert!(!s7.bond_budget().unwrap().claim_row(&h64(0xFC)).unwrap().open, "the last quantum closes the claim");
    // Strict R: a receipt block whose carve exceeds what is left refuses the spend (the block, not a skip).
    let refused = spend_at(&certified, &p, &PalwBlockContextV2 { subsidy: 1_000, ..ctx(6, 130, 6) }, &fp_spend(0xFC, 0));
    assert!(matches!(refused, Err(PalwStateV2Error::BondBudgetExhausted { .. })), "{refused:?}");
    assert!(!certified.bond_budget_spend_fits_v1(&h64(0xFC), || 620), "the processor's pre-check (hook H-5) agrees");
    assert!(certified.bond_budget_spend_fits_v1(&h64(0xFC), || 0));
    // Blocks: a commitment of three quanta (60 leaves) does not fit B = 2 blocks — refused by name.
    let genesis = PalwChainStateV2::genesis();
    let (s1, _) = apply(&genesis, &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
    let three = apply_palw_transition_v2(&s1, &p, &ctx(2, 101, 2), &[fp_commit(0xFD, 60, 3)], None);
    assert!(matches!(three, Err(PalwStateV2Error::BondBudgetExhausted { .. })), "{three:?}");
}

fn spend_at(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    c: &PalwBlockContextV2,
    spend: &PalwReceiptSpendUnsignedV3,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    apply_palw_transition_v3(parent, p, c, &[], PalwBlockWorkV3::ReceiptSpend(spend))
}

/// **The model allocation clips reward only (ADR-0177 D4–D5)**: the same attempt, admitted with the same weight, is paid its carve with the
/// allocation off, half of it when its bond splits its capital evenly between two registered models (each model half the realized
/// carve), and nothing when no capital is assigned anywhere (`Σ A = 0`: the v1 rule, never minted).
#[test]
fn the_model_allocation_clips_reward_only_and_unassigned_budget_is_never_minted() {
    let run = |allocation: Option<u64>, assign: Option<Vec<(Hash64, u64)>>| -> (u64, u128, bool) {
        let p = armed_with(test_policy(1), allocation);
        let genesis = PalwChainStateV2::genesis();
        let mut objects = register_class_and_bond();
        objects.push(PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(2),
            artifact_root: h64(12),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 0,
            activation_daa: 0,
            admission: None,
        });
        let (mut s, _) = apply(&genesis, &p, &ctx(1, 100, 1), &objects, None);
        if let Some(assignments) = assign {
            let (next, _) = apply(&s, &p, &ctx(2, 101, 2), &[assignment(bond_key(1), assignments, 1)], None);
            s = next;
        }
        // Epoch 2 opens at 120: the assignment is seasoned. The attempt's own carve is the epoch's first.
        let env = attempt(40, 1);
        let id = attempt_id_v2(&env.attempt);
        let (s3, _) = apply(&s, &p, &PalwBlockContextV2 { subsidy: 1_000, ..ctx(3, 121, 3) }, &[], Some(&env));
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: op_id(21) }];
        let (s4, _) =
            apply(&s3, &p, &ctx(4, 122, 4), &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h64(77), seats }], None);
        let (s5, _) =
            apply(&s4, &p, &ctx(5, 123, 5), &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seat_says(true) }], None);
        let (s6, _) = apply(&s5, &p, &ctx(6, 144, 6), &[], None);
        assert!(matches!(s6.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
        (s6.pending_payouts_iter().map(|(_, pay)| pay.amount).sum(), s6.safe_weight(), s3.claim(&id).is_some())
    };
    let off = run(None, None);
    let mut halves = vec![(h64(1), 500), (h64(2), 500)];
    halves.sort();
    let even = run(Some(100), Some(halves));
    let none = run(Some(100), None);
    assert_eq!(off, (620, 40, true), "the allocation off: the carve");
    assert_eq!(even, (310, 40, true), "half the capital on the claim's model: half the realized carve");
    assert_eq!(none, (0, 40, true), "no capital assigned: Σ A = 0, nothing allocated, nothing minted");
}

/// **Retirement and reversal refund nothing** and keep the weight books exact: a budgeted Final claim retires the weight it was granted.
#[test]
fn retirement_moves_the_granted_weight_and_the_row_leaves_after_its_window() {
    // Retirement 50 DAA after Final (the fixture `a_retired_claim_leaves_the_state_without_taking_its_weight` uses): Final at 124,
    // retired by the walk's block at 200, which is also past the claim's `accepted + W` = 151.
    let p = armed_with(sliced(250), None).with_claim_retirement_daa(50).unwrap();
    let walked = walk(&p);
    for pair in walked.windows(2) {
        assert_replayable(&p, &pair[0].0, &pair[1].0, &pair[1].1);
    }
    let claim_id = attempt_id_v2(&attempt(40, 1).attempt);
    let (s7, _) = &walked[6];
    assert!(s7.claim(&claim_id).is_none(), "retired");
    assert_eq!(s7.safe_weight(), 2, "the running total keeps what Final credited");
    assert_eq!(s7.retired_safe_weight(), 2, "the granted F moved, not the contribution of 40");
    assert!(s7.bond_budget().unwrap().claim_row(&claim_id).is_none(), "forgotten, and out of its window");
}
