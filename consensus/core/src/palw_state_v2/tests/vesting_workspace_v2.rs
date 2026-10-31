//! Actual maturity/planner/queue consumer; no activation or settlement policy change.
#![allow(warnings)]
use super::*;
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};
use crate::palw_vesting_v1::{PalwVestingNoteV1, PalwVestingRowV1};
use std::sync::Arc;

fn source() -> PalwChainStateV2 {
    let mut s = PalwChainStateV2::genesis();
    for n in 1..=3 {
        let row = PalwVestingRowV1 {
            claim_id: h64(n),
            producer_bond: bond_key(n),
            class_id: h64(40),
            execution_root: h64(41),
            artifact_root: h64(42),
            job_identity: h64(43),
            free_prompt: false,
            trace_root: h64(44),
            segment_count: 1,
            licence_door: crate::palw_economic_safety_v1::PalwLicenceDoorTagV1::Quorum,
            basis_k: 2,
            escrowed_reward: 12,
            buyback_bound: 0,
            producer: PalwPayoutV2 { payload: h64(n + 100), amount: 9 },
            // Shared payee exercises per-row distinct keys and cross-row accumulation.
            seats: vec![(bond_key(9), PalwPayoutV2 { payload: h64(109), amount: 1 })],
            reserve: 2,
            final_daa: 1,
            expiry_daa: n + 2,
            settled_at_final: 0,
            matured_at: None,
        };
        s.vesting_counters.created += row.total_sompi_u128();
        s.vesting.insert(row.claim_id, row);
    }
    s.reporter_rewards.insert(h64(90), PalwPayoutV2 { payload: h64(190), amount: 5 });
    s.rebuild_vesting_indices();
    s
}
fn checker(bytes: u64) -> Arc<NativeDemandCheckerV1> {
    NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: bytes, database_bytes: 0 })
}
fn builder<'a>(
    s: &PalwChainStateV2,
    p: &'a PalwStateParamsV2,
    e: &'a PalwTransitionExtrasV1,
    c: &Arc<NativeDemandCheckerV1>,
) -> TransitionBuilder<'a> {
    // Admit parent construction before installing the target phase's independent checker.
    let mut b = TransitionBuilder::new(s, p, false, false, false, false, e);
    b.native_reservation = NativeTransitionReservationV1::for_checker_v1(c);
    b
}

#[test]
fn vesting_workspace_actual_consumer_refuses_before_latch_and_retries_without_state_or_journal_change() {
    let s = source();
    let p = params();
    let e = PalwTransitionExtrasV1::default();
    let point = ctx(100, 100, 100);
    let c = checker(u64::MAX);
    let mut measured = builder(&s, &p, &e, &c);
    measured.reserve_vesting_workspace_v2().unwrap();
    let q = c.outstanding().0;
    assert!(q > 0);
    assert_eq!(measured.state, s);
    assert!(measured.entries.is_empty());
    drop(measured);
    assert_eq!(c.outstanding(), (0, 0));

    for cap in [0, q - 1] {
        let c = checker(cap);
        let mut b = builder(&s, &p, &e, &c);
        let root = b.state.state_root();
        assert!(matches!(apply_vesting_maturity(&mut b, &point), Err(PalwStateV2Error::NativeResource(_))));
        assert_eq!(b.state, s);
        assert_eq!(b.state.state_root(), root);
        assert!(b.entries.is_empty());
        assert_eq!(c.outstanding(), (0, 0));
    }
    let exact = checker(q);
    let mut b = builder(&s, &p, &e, &exact);
    b.reserve_vesting_workspace_v2().unwrap();
    assert_eq!(exact.outstanding(), (q, 0));
    drop(b);
    assert_eq!(exact.outstanding(), (0, 0));

    let retry = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &retry);
    apply_vesting_maturity(&mut b, &point).unwrap();
    assert!(b.state.vesting.is_empty());
    assert!(b.state.reporter_rewards.is_empty());
    assert!(b.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::VestingNote(PalwVestingNoteV1::Moved { .. }))));
    assert!(retry.outstanding().0 >= q, "construction ticket remains through queue and note consumption");
    assert_eq!(s.vesting.len(), 3, "published input is immutable");
    drop(b);
    assert_eq!(retry.outstanding(), (0, 0));
}

#[test]
fn vesting_workspace_actual_payout_queue_and_next_block_drain_preserve_flags_off_bytes() {
    let s = source();
    let p = params();
    let e = PalwTransitionExtrasV1::default();
    let point = ctx(100, 100, 100);
    let mut raw = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
    apply_vesting_maturity(&mut raw, &point).unwrap();
    let c = checker(u64::MAX);
    let mut owned = builder(&s, &p, &e, &c);
    apply_vesting_maturity(&mut owned, &point).unwrap();
    assert_eq!(owned.state, raw.state);
    assert_eq!(owned.state.state_root(), raw.state.state_root());
    assert_eq!(borsh::to_vec(&owned.entries).unwrap(), borsh::to_vec(&raw.entries).unwrap());
    let payouts: Vec<_> = owned.state.pending_payouts_iter().map(|(k, v)| (*k, *v)).collect();
    assert_eq!(payouts.len(), 5); // reporter + 3 producers + one shared Panel payee
    assert_eq!(payouts.iter().map(|(_, p)| p.amount).sum::<u64>(), 35);
    assert_eq!(owned.state.panel_reserve_sompi, 6);
    let queued = owned.state.clone();
    // Public production pre-object path drains exactly the coinbase-paid prefix.
    let drained = palw_v2_pre_object_base_v1(&queued, &p, &ctx(101, 101, 101), false, false, false, false, &e).unwrap();
    assert_eq!(drained.pending_payouts_iter().count(), 0);
    assert_eq!(drained.vesting_counters.moved, 36);
    assert_eq!(drained.panel_reserve_sompi, 6);
    drop(owned);
    assert_eq!(c.outstanding(), (0, 0));
}

fn with_pools(mut s: PalwChainStateV2) -> PalwChainStateV2 {
    use crate::palw_activation_pool_v1::{PalwActivationPoolCountersV1, PalwActivationPoolV1};
    for n in 1..=12 {
        let class = h64(1000 + n);
        s.activation_pools.insert(
            class,
            PalwActivationPoolV1 {
                funded_sompi: 10,
                scheduled_sompi: 10,
                prep_paid: vec![h64(2000 + n)],
                bonus_paid: vec![h64(3000 + n)],
                ..PalwActivationPoolV1::opened_at(1)
            },
        );
        s.activation_pool_scheduled.insert((class, h64(5000)), 10);
    }
    s.activation_pool_counters = PalwActivationPoolCountersV1::of_rows(s.activation_pools.values());
    s
}

#[test]
fn activation_pool_flush_workspace_refuses_before_actual_collection_then_retries_shared_payee() {
    let s = with_pools(PalwChainStateV2::genesis());
    let p = params();
    let e = PalwTransitionExtrasV1::default();
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    b.reserve_activation_pool_flush_workspace_v2().unwrap();
    let q = c.outstanding().0;
    assert!(q > 0);
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    for cap in [0, q - 1] {
        let c = checker(cap);
        let mut b = builder(&s, &p, &e, &c);
        assert!(matches!(apply_activation_pool_flush_v1(&mut b), Err(PalwStateV2Error::NativeResource(_))));
        assert_eq!(b.state, s);
        assert_eq!(b.state.state_root(), s.state_root());
        assert!(b.entries.is_empty());
        assert_eq!(c.outstanding(), (0, 0));
    }
    let c = checker(q);
    let mut b = builder(&s, &p, &e, &c);
    b.reserve_activation_pool_flush_workspace_v2().unwrap();
    assert_eq!(c.outstanding(), (q, 0));
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    apply_activation_pool_flush_v1(&mut b).unwrap();
    assert!(b.state.activation_pool_scheduled.is_empty());
    assert_eq!(b.state.pending_payouts.len(), 1);
    assert_eq!(b.state.pending_payouts.values().next().unwrap().amount, 120);
    assert_eq!(b.state.activation_pool_counters.scheduled_sompi, 0);
    assert_eq!(b.state.activation_pool_counters.paid_sompi, 120);
    assert!(b.state.activation_pools.values().all(|p| p.is_balanced()));
    assert!(c.outstanding().0 >= q);
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
}

#[test]
fn maturity_and_activation_pool_workspaces_coexist_through_queue_and_flags_off_journal() {
    let s = with_pools(source());
    let p = params();
    let e = PalwTransitionExtrasV1::default();
    let c = checker(u64::MAX);
    let mut b = builder(&s, &p, &e, &c);
    let point = ctx(100, 100, 100);
    apply_vesting_maturity(&mut b, &point).unwrap();
    let maturity_held = c.outstanding().0;
    apply_activation_pool_flush_v1(&mut b).unwrap();
    assert!(c.outstanding().0 > maturity_held, "both phases retain ownership until handoff");
    let mut raw = TransitionBuilder::new(&s, &p, false, false, false, false, &e);
    apply_vesting_maturity(&mut raw, &point).unwrap();
    apply_activation_pool_flush_v1(&mut raw).unwrap();
    assert_eq!(raw.state, b.state);
    assert_eq!(raw.state.state_root(), b.state.state_root());
    assert_eq!(borsh::to_vec(&raw.entries).unwrap(), borsh::to_vec(&b.entries).unwrap());
    assert_eq!(b.state.pending_payouts.len(), 6);
    assert_eq!(b.state.pending_payouts.values().map(|p| p.amount).sum::<u64>(), 155);
    // All 12 source entries fit by sharing one new key, even with five vesting
    // payout keys already waiting; no invented eight-row collection bound.
    assert!(b.state.activation_pool_scheduled.is_empty());
    let queued = b.state.clone();
    let drained = palw_v2_pre_object_base_v1(&queued, &p, &ctx(101, 101, 101), false, false, false, false, &e).unwrap();
    assert_eq!(drained.pending_payouts_iter().count(), 0);
    assert_eq!(drained.activation_pool_counters.paid_sompi, 120);
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
}
