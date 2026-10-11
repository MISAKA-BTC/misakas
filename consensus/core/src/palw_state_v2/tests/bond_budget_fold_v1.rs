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
        round_rights: PalwRoundRightsPolicyV1::CountAgainstBlocks,
    }
}

/// A TEST allocation policy: epochs of 10 DAA, one epoch of seasoning, `A = S` (`α = 1`, `alpha_halves = 2`).
fn test_allocation() -> PalwModelAllocationPolicyV1 {
    PalwModelAllocationPolicyV1 {
        version: PALW_MODEL_ALLOCATION_POLICY_VERSION_V2,
        epoch_daa: 10,
        seasoning_epochs: 1,
        curve: PalwAllocationCurveV1 { alpha_halves: 2 },
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
/// Legacy Q/B/F reservation plus a reclassified actual issuer income hold. Old signed money is preserved; Q/B/F use accepted+W,
/// while unpaid R stays held through actual payment+W.
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
    assert_eq!(row.reserved.reward_sompi, 0, "R is reclassified into the unpaid actual issuer hold");
    let income = budget.claim_row(&super::super::palw_bond_budget_fold_v1::native_income_id_v1(claim_id, &bond_key(1), 0)).unwrap();
    assert_eq!(income.reserved.reward_sompi, 620);
    assert!(income.live && !income.open);
    assert_eq!(row.reserved.block_units, PALW_BUDGET_BLOCK_UNIT_V1);
    assert!(budget.window_holds(&bond_key(1), 150));
    assert!(d3.entries.iter().any(is_budget_entry));
    // Old rules pay it whole (the walk's Final at 124).
    let (s5, _) = &walked[4];
    assert_eq!(s5.pending_payouts_iter().map(|(_, pay)| pay.amount).sum::<u64>(), 620, "an old claim is paid by its old rule");
    // Q/B/F free at 151, R at actual payment 125 + W = 175. Both have left by 200.
    let (s6, _) = &walked[5];
    assert!(s6.bond_budget().unwrap().window_holds(&bond_key(1), 125));
    let (s7, _) = &walked[6];
    assert!(!s7.bond_budget().unwrap().window_holds(&bond_key(1), 200));
    assert!(s7.bond_budget().unwrap().bond_row(&bond_key(1)).is_none_or(|row| row.window.is_zero()));
}

/// This is a core fold/queue test, not an execution soundness proof. It uses the
/// same fixture admission seam as `walk`, then only production transition,
/// queue-drain, carriage and delta APIs for the monetary clock.
#[test]
fn unpaid_issuer_r_survives_sparse_daa_until_actual_payment_then_uses_payment_plus_w() {
    let p = armed_with(test_policy(1), None);
    let walked = walk(&p);
    let queued = &walked[4].0;
    let claim = attempt_id_v2(&attempt(40, 1).attempt);
    let budget = queued.bond_budget().unwrap();
    assert_eq!(budget.bond_row(&bond_key(1)).unwrap().window.reward_sompi, 620);
    assert_eq!(queued.pending_payouts_iter().map(|(_, pay)| pay.amount).sum::<u64>(), 620);
    let (paid, delta) = apply(queued, &p, &ctx(6, 1_000, 6), &[], None);
    assert_replayable(&p, queued, &paid, &delta);
    assert_eq!(paid.pending_payouts_iter().count(), 0);
    let id = super::super::palw_bond_budget_fold_v1::native_queued_income_id_v1(claim, &bond_key(1));
    let paid_row = paid.bond_budget().unwrap().claim_row(&id).expect("same durable monetary hold");
    assert_eq!(paid_row.origin, PalwBudgetOriginV1::NativeIncomeQueued);
    assert_eq!((paid_row.reserved.reward_sompi, paid_row.consumed.reward_sompi), (620, 620));
    assert_eq!(paid_row.reuse_not_before, 1_050);
    assert!(!paid_row.live && !paid_row.open && paid_row.in_window);
    let (before_release, delta) = apply(&paid, &p, &ctx(7, 1_049, 7), &[], None);
    assert_replayable(&p, &paid, &before_release, &delta);
    assert_eq!(before_release.bond_budget().unwrap().bond_row(&bond_key(1)).unwrap().window.reward_sompi, 620);
    let (released, delta) = apply(&before_release, &p, &ctx(8, 1_050, 8), &[], None);
    assert_replayable(&p, &before_release, &released, &delta);
    assert!(released.bond_budget().unwrap().bond_row(&bond_key(1)).is_none());
}

/// Production real-collateral settlement, queue/drain, carriage and delta APIs.
/// The emitted settlement batch is a mechanics seam, not a FileProof verifier
/// or a native cryptographic conviction. Both public reporter consumer kinds
/// must preserve funded rights under full R and through several physical W's.
#[test]
fn funded_kernel_reporter_settlements_defer_full_r_and_redeem_exact_native_tranches() {
    use super::super::palw_bond_budget_fold_v1::{native_queued_income_id_v1, reporter_tranche_queue_v1};
    use super::super::palw_kernel_route_fold_v1::{apply_settlements, load_ledger};
    use crate::palw_kernel_route_v1::{PalwKernelRouteExtrasV1, PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1, palw_kernel_bond_id_v1};
    use misaka_palw_kernel::ledger::LedgerEventV1;
    use misaka_palw_kernel::settle::{SettlementInstructionV1, SettlementKindV1};
    for kind in [SettlementKindV1::AccuserReward, SettlementKindV1::DemanderShare] {
        let mut policy = test_policy(1);
        policy.window_daa = 280;
        policy.reward_per_unit_sompi = 50;
        let p = armed_with(policy, None).with_reporter_income_from(Some(100));
        let mut objects = register_class_and_bond();
        objects.push(bond_registration_with_collateral(2, 8, 22, 1_000));
        let (registered, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None);
        let extras = PalwTransitionExtrasV1 {
            kernel_route: Some(PalwKernelRouteExtrasV1 { network_domain: h64(0xE7), ruleset_digest: h64(0xE8), ..Default::default() }),
            ..Default::default()
        };
        let mut b = TransitionBuilder::new(&registered, &p, false, false, false, false, &extras);
        load_ledger(&mut b, &ctx(2, 100, 2)).unwrap();
        for bond in [bond_key(1), bond_key(2)] {
            b.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1, borsh::to_vec(&palw_kernel_bond_id_v1(&bond)).unwrap(),
                Some(borsh::to_vec(&bond).unwrap()));
        }
        b.reserve_bond_income_v1(&ctx(2, 100, 2), bond_key(1), 50, 50, PalwBudgetOriginV1::MarketFee).unwrap();
        let settlement = |bond, amount, kind| LedgerEventV1::Settlement(SettlementInstructionV1 {
            bond: palw_kernel_bond_id_v1(&bond), amount, kind, claim: Some(h64(0xD00).as_bytes()),
        });
        let events = [settlement(bond_key(2), 300, SettlementKindV1::SlashFraud),
            settlement(bond_key(1), 125, kind), settlement(bond_key(2), 175, SettlementKindV1::Burn)];
        apply_settlements(&mut b, &events, true, 100).unwrap();
        assert_eq!(b.state.bond(&bond_key(2)).unwrap().collateral, 700, "the immutable award is funded by a real collateral debit");
        assert!(b.state.pending_payouts.is_empty(), "full recipient R never creates uncapped cash");
        let (award, row) = b.state.bond_budget().unwrap().claims.iter().find(|(_, row)| row.origin == PalwBudgetOriginV1::DeferredReporterIncome).unwrap();
        let award = *award;
        assert_eq!((row.reserved.reward_sompi, row.consumed.reward_sompi), (125, 0));
        let staged = b.state.clone();
        let delta = PalwStateDeltaV2 { point: ctx(2, 100, 2), entries: b.entries };
        assert_replayable(&p, &registered, &staged, &delta);
        assert!(staged.bond_budget_window_holds(&bond_key(1), u64::MAX));
        let (full, delta) = apply(&staged, &p, &ctx(3, 200, 3), &[], None);
        assert_replayable(&p, &staged, &full, &delta);
        assert!(full.pending_payouts.is_empty());
        let (mut state, delta) = apply(&full, &p, &ctx(4, 380, 4), &[], None);
        assert_replayable(&p, &full, &state, &delta);
        let mut progress = 0;
        let mut queued_at = 380;
        for (n, expected) in [50, 50, 25].into_iter().enumerate() {
            let queue = reporter_tranche_queue_v1(award, progress);
            assert_eq!(state.pending_payout(&queue).unwrap().amount, expected);
            assert_eq!(state.bond_budget().unwrap().bond_row(&bond_key(1)).unwrap().window,
                PalwBudgetVectorV1 { reward_sompi: expected, ..PalwBudgetVectorV1::ZERO });
            let before_pay = state.clone();
            let payment = queued_at + 1_000; // actual sparse drain, never logical queue+W
            let (paid, delta) = apply(&before_pay, &p, &ctx(5 + n as u64 * 3, payment, 5 + n as u64 * 3), &[], None);
            assert_replayable(&p, &before_pay, &paid, &delta);
            assert!(paid.pending_payout(&queue).is_none());
            let receipt = paid.bond_budget().unwrap().claim_row(&native_queued_income_id_v1(queue, &bond_key(1))).unwrap();
            assert_eq!(receipt.reuse_not_before, payment + 280);
            assert!(!receipt.live);
            // Exact reorg/restart restores the same tranche and progress, not a
            // second award or an additional payout for already completed tranches.
            let reverted = revert_delta_v2(&paid, &delta, &p).unwrap();
            assert_eq!(reverted, before_pay);
            assert_eq!(apply_delta_v2(&reverted, &delta, &p).unwrap(), paid);
            let (held, delta) = apply(&paid, &p, &ctx(6 + n as u64 * 3, payment + 279, 6 + n as u64 * 3), &[], None);
            assert_replayable(&p, &paid, &held, &delta);
            assert!(held.pending_payouts.is_empty());
            progress += expected;
            let (next, delta) = apply(&held, &p, &ctx(7 + n as u64 * 3, payment + 280, 7 + n as u64 * 3), &[], None);
            assert_replayable(&p, &held, &next, &delta);
            assert!(next.bond_budget().unwrap().claim_row(&native_queued_income_id_v1(queue, &bond_key(1))).is_none());
            assert_eq!(next.bond_budget().unwrap().claim_row(&award).unwrap().consumed.reward_sompi,
                if n == 0 { 100 } else { 125 });
            queued_at = payment + 280;
            state = next;
        }
        assert_eq!(progress, 125);
        assert!(state.pending_payouts.is_empty());
        assert!(state.bond_budget().unwrap().bond_row(&bond_key(1)).is_none());
        assert!(!state.bond_budget_window_holds(&bond_key(1), queued_at));
        assert_eq!(state.bond(&bond_key(2)).unwrap().collateral, 700);
        let (idle, delta) = apply(&state, &p, &ctx(20, queued_at + 10_000, 20), &[], None);
        assert_replayable(&p, &state, &idle, &delta);
        assert!(idle.pending_payouts.is_empty());
        assert_eq!(idle.bond_budget().unwrap().claim_row(&award).unwrap().reserved.reward_sompi, 125);
    }
}

/// The consumed court verdict is an explicit mechanics seam; collateral debit,
/// named winner binding, reveal sweep, R-limited native queue, physical payment,
/// carriage and reorg are the production APIs. This proves no court soundness.
#[test]
fn named_native_reporter_freezes_funded_payee_and_pays_above_one_cap_in_exact_tranches() {
    use super::super::palw_bond_budget_fold_v1::{native_reporter_binding_id_v1, reporter_tranche_queue_v1};
    use crate::palw_offence_v1::{PalwConsumedOffenceV1, PalwOffenceKindV1};
    let mut policy = test_policy(1);
    policy.window_daa = 280;
    policy.reward_per_unit_sompi = 50;
    let p = armed_with(policy, None).with_reporter_income_from(Some(100)).with_rcore_plus_mirrors(Some(0), 12_900, Vec::new());
    let mut objects = register_class_and_bond();
    objects.push(bond_registration_with_collateral(2, 8, 22, 2_000));
    let (registered, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None);
    let extras = PalwTransitionExtrasV1::default();
    let point = ctx(2, 200, 2);
    let mut b = TransitionBuilder::new(&registered, &p, false, false, false, false, &extras);
    b.reserve_bond_income_v1(&point, bond_key(1), 50, 50, PalwBudgetOriginV1::MarketFee).unwrap();
    let collected = b.slash_bond(bond_key(2), 1_250).unwrap();
    assert_eq!(collected, 1_250);
    let offence = h64(0xCD08);
    b.write_consumed_offence(offence, Some(PalwConsumedOffenceV1 {
        kind: PalwOffenceKindV1::CourtConviction, accused: bond_key(2).0, amount: 1_250, accepted_daa: 200,
        execution_root: Hash64::default(), collected, claim_id: Hash64::default(),
    }));
    assert_eq!(b.open_reporter_reward(200, offence, 0, PalwConvictionBasisV1::CourtVerdict { challenger: bond_key(1) }).unwrap(), Some(125));
    let award = native_reporter_binding_id_v1(offence);
    let frozen = b.state.bond_budget().unwrap().reporter_income[&award];
    assert_eq!(frozen.recipient, Some(bond_key(1)));
    assert_eq!(frozen.payload, registered.bond(&bond_key(1)).unwrap().payout_payload);
    assert_eq!(frozen.maximum, 125);
    assert!(frozen.awarded_daa.is_none());
    assert!(b.state.bond_budget_window_holds(&bond_key(1), u64::MAX));
    // A later bond-row payout change is a private mutation seam. The actual
    // monetary entitlement must still redeem its original authenticated payee.
    let mut changed = b.state.bond(&bond_key(1)).unwrap().clone();
    changed.payout_payload = h64(0xBAD08);
    b.write_bond(bond_key(1), Some(changed));
    let reveal_until = b.state.reward_pending[&offence].reveal_until;
    b.entries.push(PalwDeltaEntryV2::LastPoint { old: registered.last_point, new: Some(point) });
    b.state.last_point = Some(point);
    let staged = b.state;
    let delta = PalwStateDeltaV2 { point, entries: b.entries };
    assert_replayable(&p, &registered, &staged, &delta);
    let (closed, delta) = apply(&staged, &p, &ctx(3, reveal_until + 1, 3), &[], None);
    assert_replayable(&p, &staged, &closed, &delta);
    assert!(closed.reporter_rewards.is_empty() && closed.pending_payouts.is_empty(), "R-full award is exact funded debt, with no uncapped cash");
    assert_eq!(closed.reporter_counters.awarded_sompi, 125);
    assert_eq!(closed.bond(&bond_key(2)).unwrap().collateral, 750);
    let debt = closed.bond_budget().unwrap().claim_row(&award).unwrap();
    assert_eq!((debt.reserved.reward_sompi, debt.consumed.reward_sompi), (125, 0));
    let (mut state, delta) = apply(&closed, &p, &ctx(4, 480, 4), &[], None);
    assert_replayable(&p, &closed, &state, &delta);
    let mut progress = 0;
    let mut queued_at = 480;
    for (i, expected) in [50, 50, 25].into_iter().enumerate() {
        let queue = reporter_tranche_queue_v1(award, progress);
        let payout = *state.pending_payout(&queue).unwrap();
        assert_eq!(payout, PalwPayoutV2 { payload: frozen.payload, amount: expected });
        assert_ne!(payout.payload, state.bond(&bond_key(1)).unwrap().payout_payload, "redemption uses the immutable entitlement payload");
        let payment = queued_at + 1_000;
        let (paid, delta) = apply(&state, &p, &ctx(5 + 2 * i as u64, payment, 5 + 2 * i as u64), &[], None);
        assert_replayable(&p, &state, &paid, &delta);
        assert!(paid.pending_payouts.is_empty());
        assert_eq!(paid.reporter_counters.awarded_sompi, 125, "tranches never classify the award again");
        progress += expected;
        queued_at = payment + 280;
        let (next, delta) = apply(&paid, &p, &ctx(6 + 2 * i as u64, queued_at, 6 + 2 * i as u64), &[], None);
        assert_replayable(&p, &paid, &next, &delta);
        state = next;
    }
    assert_eq!(progress, 125);
    assert!(state.pending_payouts.is_empty());
    assert!(!state.bond_budget_window_holds(&bond_key(1), queued_at));
    assert_eq!(state.bond_budget().unwrap().claim_row(&award).unwrap().consumed.reward_sompi, 125);
}

/// An old awarded row carries only a payload; migration preserves its exact
/// cash, never guesses a bond from that address, and starts W on real drain.
#[test]
fn reporter_v2_fence_preserves_v1_and_quarantines_old_exact_cash_with_carriage_reorg() {
    let p = armed_with(test_policy(1), None).with_rcore_plus_mirrors(Some(0), 12_900, Vec::new()).with_reporter_income_from(Some(105));
    let (registered, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
    assert_eq!(registered.bond_budget().unwrap().header.version, PALW_BOND_BUDGET_STATE_VERSION_V1);
    let extras = PalwTransitionExtrasV1::default();
    let point = ctx(2, 101, 2);
    let mut b = TransitionBuilder::new(&registered, &p, false, false, false, false, &extras);
    let offence = h64(0xAE08);
    let payout = PalwPayoutV2 { payload: registered.bond(&bond_key(1)).unwrap().payout_payload, amount: 125 };
    b.write_reporter_reward(offence, Some(payout));
    b.write_reporter_counters(PalwReporterCountersV1 { awarded_sompi: 125, forgone_sompi: 0 });
    b.entries.push(PalwDeltaEntryV2::LastPoint { old: registered.last_point, new: Some(point) });
    b.state.last_point = Some(point);
    let before = b.state;
    let delta = PalwStateDeltaV2 { point, entries: b.entries };
    assert_replayable(&p, &registered, &before, &delta);
    let old_wire = borsh::to_vec(before.bond_budget().unwrap()).unwrap();
    let (absent, _) = apply(&before, &p.clone().with_reporter_income_from(None), &ctx(3, 105, 3), &[], None);
    assert_eq!(absent.bond_budget().unwrap().header.version, PALW_BOND_BUDGET_STATE_VERSION_V1);
    assert!(absent.bond_budget().unwrap().reporter_income.is_empty());
    let (migrated, delta) = apply(&before, &p, &ctx(3, 105, 3), &[], None);
    assert_replayable(&p, &before, &migrated, &delta);
    assert_eq!(borsh::to_vec(revert_delta_v2(&migrated, &delta, &p).unwrap().bond_budget().unwrap()).unwrap(), old_wire);
    let book = migrated.bond_budget().unwrap();
    assert_eq!(book.header.version, PALW_BOND_BUDGET_STATE_VERSION_V2);
    assert!(book.has_unattributed_income_v1());
    assert!(book.reporter_income.values().all(|row| row.recipient.is_none()), "a matching actual bond payout is not authenticated ownership");
    let queue = crate::palw_vesting_v1::palw_reporter_payout_key_v1(&offence);
    assert_eq!(*migrated.pending_payout(&queue).unwrap(), payout);
    let mut b = TransitionBuilder::new(&migrated, &p, false, false, false, false, &extras);
    let held = (b.state.clone(), b.entries.len());
    assert!(b.reserve_bond_income_v1(&ctx(4, 106, 4), bond_key(1), 1, 1, PalwBudgetOriginV1::MarketFee).is_err());
    assert_eq!((b.state.clone(), b.entries.len()), held);
    let (paid, delta) = apply(&migrated, &p, &ctx(4, 1_000, 4), &[], None);
    assert_replayable(&p, &migrated, &paid, &delta);
    assert!(paid.pending_payouts.is_empty());
    assert!(paid.bond_budget().unwrap().has_unattributed_income_v1(), "late drain outlives first fence + W");
    let physical = paid.bond_budget().unwrap().reporter_income.values().find(|row| row.queue == Some(queue)).unwrap();
    assert_eq!(physical.paid_daa, Some(1_000));
    let (held, delta) = apply(&paid, &p, &ctx(5, 1_049, 5), &[], None);
    assert_replayable(&p, &paid, &held, &delta);
    assert!(held.bond_budget().unwrap().has_unattributed_income_v1());
    let (released, delta) = apply(&held, &p, &ctx(6, 1_050, 6), &[], None);
    assert_replayable(&p, &held, &released, &delta);
    assert!(!released.bond_budget().unwrap().has_unattributed_income_v1());
    assert!(released.pending_payouts.is_empty());
    assert_eq!(released.reporter_counters.awarded_sompi, 125);
}

/// Exact production recipient/queue/drain APIs; the admitted bond and queued
/// low-key backlog are private mechanics seams, not a native Final or court proof.
#[test]
fn partial_native_fd_fe_funds_use_individual_physical_payment_clocks_with_exact_delta_replay() {
    use super::super::palw_bond_budget_fold_v1::{native_income_id_v1, native_queued_income_id_v1};
    for kernel_first in [false, true] {
        let p = armed_with(test_policy(1), None);
        let (registered, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
        let extras = PalwTransitionExtrasV1::default();
        let payload = registered.bond(&bond_key(1)).unwrap().payout_payload;
        let kernel = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload);
        let native = palw_panel_payout_key_v1(&payload);
        assert!(kernel < native, "FD drains before FE under the actual pre-object ordering");
        let (first, last) = if kernel_first { (kernel, native) } else { (native, kernel) };
        let root = h64(0x8891);
        let mut b = TransitionBuilder::new(&registered, &p, false, false, false, false, &extras);
        b.hold_native_recipient_v1(root, bond_key(1), 2, 100, 500, 50, &[]).unwrap();
        b.pay_native_recipient_v1(root, bond_key(1), 2, 501, 10, first, false).unwrap();
        b.write_payout(first, Some(PalwPayoutV2 { payload, amount: 10 }));
        b.pay_native_recipient_v1(root, bond_key(1), 2, 502, 20, last, true).unwrap();
        b.write_payout(last, Some(PalwPayoutV2 { payload, amount: 20 }));
        for n in 0..7 {
            b.write_payout(h64(n), Some(PalwPayoutV2 { payload: h64(0x1234), amount: 1 }));
        }
        let queued = b.state.clone();
        let delta = PalwStateDeltaV2 { point: ctx(2, 502, 2), entries: b.entries };
        assert_replayable(&p, &registered, &queued, &delta);
        assert!(queued.bond_budget().unwrap().claim_row(&native_income_id_v1(root, &bond_key(1), 2)).is_none());
        assert_eq!(queued.bond_budget().unwrap().bond_row(&bond_key(1)).unwrap().window.reward_sompi, 50);
        let (paid_fd, delta) = apply(&queued, &p, &ctx(3, 1_000, 3), &[], None);
        assert_replayable(&p, &queued, &paid_fd, &delta);
        assert!(paid_fd.pending_payout(&kernel).is_none());
        assert!(paid_fd.pending_payout(&native).is_some());
        let fd = native_queued_income_id_v1(kernel, &bond_key(1));
        let fe = native_queued_income_id_v1(native, &bond_key(1));
        assert!(!paid_fd.bond_budget().unwrap().claim_row(&fd).unwrap().live);
        assert!(paid_fd.bond_budget().unwrap().claim_row(&fe).unwrap().live);
        let unpaid_r = if kernel_first { 40 } else { 10 };
        assert_eq!(paid_fd.bond_budget().unwrap().bond_row(&bond_key(1)).unwrap().window.reward_sompi, 50);
        // A second exact backlog keeps FE unpaid beyond FD's physical W.
        let mut b = TransitionBuilder::new(&paid_fd, &p, false, false, false, false, &extras);
        for n in 10..18 {
            b.write_payout(h64(n), Some(PalwPayoutV2 { payload: h64(0x1234), amount: 1 }));
        }
        let backlogged = b.state.clone();
        let delta = PalwStateDeltaV2 { point: ctx(4, 1_000, 4), entries: b.entries };
        assert_replayable(&p, &paid_fd, &backlogged, &delta);
        let (fd_released, delta) = apply(&backlogged, &p, &ctx(5, 1_050, 5), &[], None);
        assert_replayable(&p, &backlogged, &fd_released, &delta);
        assert!(fd_released.bond_budget().unwrap().claim_row(&fd).is_none());
        assert!(fd_released.pending_payout(&native).is_some());
        assert_eq!(fd_released.bond_budget().unwrap().bond_row(&bond_key(1)).unwrap().window.reward_sompi, unpaid_r);
        let (paid_fe, delta) = apply(&fd_released, &p, &ctx(6, 2_000, 6), &[], None);
        assert_replayable(&p, &fd_released, &paid_fe, &delta);
        assert!(paid_fe.pending_payout(&native).is_none());
        let fe_row = paid_fe.bond_budget().unwrap().claim_row(&fe).unwrap();
        assert_eq!((fe_row.live, fe_row.reuse_not_before), (false, 2_050));
        let (released, delta) = apply(&paid_fe, &p, &ctx(7, 2_050, 7), &[], None);
        assert_replayable(&p, &paid_fe, &released, &delta);
        assert!(released.bond_budget().unwrap().bond_row(&bond_key(1)).is_none());
        assert_eq!(released.pending_payouts_iter().count(), 0, "each physical cash leg drains once");
    }
}

/// Synthetic snapshot metadata tests the production draw's refusal transaction;
/// native signed Final/lottery correctness remains a separate acceptance test.
#[test]
fn round_draw_handles_overfull_migration_and_duplicate_span_without_partial_tickets_or_panic() {
    use crate::palw_execution_lane_v1::{PalwExecFinalV1, PalwExecScheduleV1};
    let mut policy = test_policy(1);
    policy.round_rights = PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 10 };
    let p = armed_with(policy.clone(), None);
    let (registered, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &register_class_and_bond(), None);
    let capital = registered.bond(&bond_key(1)).unwrap().collateral;
    for overfull in [false, true] {
        let mut parent = registered.clone();
        let budget = parent.bond_budget.as_mut().unwrap();
        let mut fixture = Vec::new();
        if overfull {
            let cap = palw_bond_budget_caps_v1(&policy, capital).reward_sompi;
            budget.hold_native_income_v1(&policy, capital, h64(0x7771), bond_key(1), 100, 500, cap + 1, &[], false, &mut fixture).unwrap();
        } else {
            budget.reserve_round_rights_v1(&policy, capital, 7, bond_key(1), 100, 1, &mut fixture).unwrap();
        }
        let mut schedule = PalwExecScheduleV1 {
            span_index: 7, seed: h64(0x7772), domains: Vec::new(), quanta: Vec::new(),
            finals: vec![PalwExecFinalV1 { domain: h64(1), bond: bond_key(1), operator_id: op_id(21),
                claim_id: h64(0x7773), execution_root: h64(0x7774), credit: 4, accepted_blue_score: 1 }],
        };
        let extras = PalwTransitionExtrasV1::default();
        let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
        assert!(b.bond_budget_round_draw_v1(&ctx(2, 200, 2), &mut schedule, 1, 100, 10, &BTreeSet::new()));
        assert!(schedule.quanta.is_empty(), "unavailable {overfull} draw creates no uncapped permit");
        assert_eq!(b.state, parent, "accepted old income and earlier tickets are unchanged");
        assert!(b.entries.is_empty(), "no partial recipient reservation journal");
    }
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
    assert_eq!((row.epoch, row.capital, row.weight), (2, 600, PalwAllocationWeightV1::from_u64(600)));
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
        PalwBudgetVectorV1 { claims: 1, block_units: PALW_BUDGET_BLOCK_UNIT_V1, reward_sompi: 0, final_weight: 2, round_rights: 0 }
    );
    assert_eq!(row.consumed.block_units, PALW_BUDGET_BLOCK_UNIT_V1, "its block was consumed at acceptance");
    let income = s2
        .bond_budget()
        .unwrap()
        .claim_row(&super::super::palw_bond_budget_fold_v1::native_income_id_v1(claim_id, &claim.bond, 0))
        .unwrap();
    assert_eq!(income.reserved.reward_sompi, 4, "the one original R cap funds the actual issuer's delayed income");

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

/// **ADR-0177 as revised (2026-10-10): `A_m = S_m^α` through the fold, replayed.** At `α = 2` a bond's capital split 400:200 over two
/// registered models is a 4:1 weight and a 4:1 division of the epoch's realized carve (620: 496 and 124, exact), and every block — the
/// assignment, the epoch roll with its 512-bit weights, the accrual — re-applies, reverts (a reorg) and reloads from its carriage under
/// its root (an IBD / pruning-point import).
#[test]
fn alpha_two_divides_the_realized_carve_four_to_one_and_every_block_replays() {
    let allocation = PalwModelAllocationPolicyV1 { curve: PalwAllocationCurveV1 { alpha_halves: 4 }, ..test_allocation() };
    let p = params().with_worker_carve_permille(620).unwrap().with_bond_budget(Some(PalwBondBudgetMirrorV1 {
        from_daa: 100,
        policy: test_policy(1),
        allocation: Some((100, allocation)),
    }));
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
    let genesis = PalwChainStateV2::genesis();
    let mut chain = vec![apply(&genesis, &p, &ctx(1, 100, 1), &objects, None)];
    let mut halves = vec![(h64(1), 400), (h64(2), 200)];
    halves.sort();
    let steps: Vec<(PalwBlockContextV2, Vec<PalwConsensusObjectV2>)> = vec![
        (ctx(2, 101, 2), vec![assignment(bond_key(1), halves, 1)]),
        (ctx(3, 121, 3), vec![]),
        (PalwBlockContextV2 { subsidy: 1_000, ..ctx(4, 122, 4) }, vec![]),
    ];
    for (c, objs) in steps {
        let parent = chain.last().unwrap().0.clone();
        let (child, delta) = apply(&parent, &p, &c, &objs, None);
        assert_replayable(&p, &parent, &child, &delta);
        chain.push((child, delta));
    }
    let budget = chain.last().unwrap().0.bond_budget().unwrap().clone();
    let (a1, a2) = (budget.model_row(&h64(1)).unwrap().weight, budget.model_row(&h64(2)).unwrap().weight);
    assert_eq!((a1, a2), (PalwAllocationWeightV1::from_u64(160_000), PalwAllocationWeightV1::from_u64(40_000)), "S^2");
    assert_eq!(budget.header.allocation.unwrap().accrued_sompi, 620);
    assert_eq!((budget.model_available(&h64(1)), budget.model_available(&h64(2))), (496, 124), "4:1 of the realized carve");
    budget.check_consistency().expect("the engine's invariants");
}


/// Physical native queue confirmation, retirement, import and journal reorg share
/// the same material receipt. These fixtures use the existing toy execution seam.
#[test]
fn material_v3_native_final_retires_only_at_actual_payment_plus_w_and_reorg_restores_the_hold() {
    for w in [280, 436] {
        for material_from in [100, 125] {
            let p = params().with_worker_carve_permille(620).unwrap()
                .with_claim_retirement_daa(50).unwrap()
                .with_bond_budget(Some(PalwBondBudgetMirrorV1 {
                    from_daa: 100, policy: PalwBondBudgetPolicyV1 { window_daa: w, ..test_policy(1) }, allocation: None,
                }))
                .with_reporter_income_from(Some(100)).with_material_income_from(Some(material_from));
            let walked = walk(&p);
            let id = attempt_id_v2(&attempt(40, 1).attempt);
            for pair in walked.windows(2) { assert_replayable(&p, &pair[0].0, &pair[1].0, &pair[1].1); }
            let (finalized, _) = &walked[4];
            assert!(matches!(finalized.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { final_daa: 124 }));
            assert_eq!(finalized.pending_payouts_iter().map(|(_, payout)| payout.amount).sum::<u64>(), 620);
            if material_from == 100 {
                assert_eq!(finalized.material_income_hold_until_v1(&p, id), Some(u64::MAX), "unpaid cash has no expiry clock");
            }
            let (paid, _) = &walked[5];
            assert!(paid.pending_payouts_iter().next().is_none(), "actual fold drains the funded native queue");
            assert_eq!(paid.material_income_hold_until_v1(&p, id), Some(125 + w));
            let (held, _) = &walked[6];
            assert!(held.claim(&id).is_some(), "ordinary retirement at 174 cannot erase collectible material");
            assert_eq!(held.material_income_hold_until_v1(&p, id), Some(125 + w));
            let (boundary, delta) = apply(held, &p, &ctx(8, 124 + w, 8), &[], None);
            assert_replayable(&p, held, &boundary, &delta);
            assert!(boundary.claim(&id).is_some());
            let (released, release) = apply(&boundary, &p, &ctx(9, 125 + w, 9), &[], None);
            assert_replayable(&p, &boundary, &released, &release);
            assert!(released.claim(&id).is_none(), "no perpetual same-bond material hold");
            assert_eq!(released.material_income_hold_until_v1(&p, id), None);
            assert!(released.bond_budget().unwrap().material_income.is_empty());
            assert_eq!(revert_delta_v2(&released, &release, &p).unwrap(), boundary);
            assert_eq!(boundary.material_income_hold_until_v1(&p, id), Some(125 + w));
            assert_eq!(released.safe_weight(), held.safe_weight(), "retirement preserves finalized weight");
        }
    }
}

#[test]
fn material_v3_future_fence_leaves_existing_v2_fold_bytes_and_roots_identical() {
    let base = params().with_worker_carve_permille(620).unwrap()
        .with_bond_budget(Some(mirror(100, None))).with_reporter_income_from(Some(100));
    let future = base.clone().with_material_income_from(Some(1_000_000));
    for ((a, da), (b, db)) in walk(&base).iter().zip(walk(&future).iter()) {
        assert_eq!(da, db);
        assert_eq!(a.state_root(), b.state_root());
        assert_eq!(carriage_bytes(a), carriage_bytes(b));
        assert_eq!(a.bond_budget().unwrap().header.version, PALW_BOND_BUDGET_STATE_VERSION_V2);
        assert!(a.bond_budget().unwrap().material_income.is_empty());
    }
}
