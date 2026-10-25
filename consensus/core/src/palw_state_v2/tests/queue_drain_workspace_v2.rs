//! Stored obligations exercise the real shared drain, rehearsal and block fold.
//! These are not a market-fill or source-economics activation fixture.
use super::*;
use crate::evm::{
    EvmAddress,
    model_market::{PALW_EVM_ACTION_SELL, PalwEvmSettlementOutcomeV1, PalwEvmSettlementV1},
};
use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};
use crate::palw_state_v2::block_preparation::queue_drain_workspace_v2::{
    drain_consumed_queues_v2, reserve_consumed_queue_workspace_v2,
};
use std::sync::Arc;

fn source() -> PalwChainStateV2 {
    let mut state = PalwChainStateV2::genesis();
    // Reverse insertion order must not change the canonical BTree prefix.
    for n in (1..=(PALW_V2_MAX_PAYOUTS_PER_BLOCK + 2)).rev() {
        state.pending_payouts.insert(h64(n as u64), PalwPayoutV2 { payload: h64(100 + n as u64), amount: n as u64 });
    }
    for seq in [2, 0, 1] {
        state.evm_settlements.insert(
            seq,
            PalwEvmSettlementV1 {
                seq,
                account: EvmAddress::from_bytes([1; 20]),
                line_id: h64(50),
                action: PALW_EVM_ACTION_SELL,
                escrow_sompi: 0,
                outcome: PalwEvmSettlementOutcomeV1::Refused { reason: 6 },
            },
        );
    }
    state
}
fn checker(bytes: u64) -> Arc<NativeDemandCheckerV1> {
    NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: bytes, database_bytes: 0 })
}
fn builder<'a>(
    state: &PalwChainStateV2,
    params: &'a PalwStateParamsV2,
    extras: &'a PalwTransitionExtrasV1,
    checker: &Arc<NativeDemandCheckerV1>,
) -> TransitionBuilder<'a> {
    // Parent construction is already admitted; the independent checker targets
    // the real queue phase, rather than failing before that consumer is reached.
    let mut b = TransitionBuilder::new(state, params, false, false, false, false, extras);
    b.native_reservation = NativeTransitionReservationV1::for_checker_v1(checker);
    b
}
fn old_drain(b: &mut TransitionBuilder<'_>, point: &PalwBlockContextV2) {
    // The pre-change order/collector is an independent flags-off reference.
    for key in b.state.pending_payouts.keys().copied().take(PALW_V2_MAX_PAYOUTS_PER_BLOCK).collect::<Vec<_>>() {
        b.write_payout(key, None).unwrap();
    }
    palw_source_verifier_economics_fold_v2::acknowledge_consumed_prefix(b, point).unwrap();
    for key in b.state.evm_settlements.keys().copied().collect::<Vec<_>>() {
        b.write_evm_settlement(key, None).unwrap();
    }
}

#[test]
fn queue_drain_workspace_refuses_before_the_first_consumed_row_and_retries() {
    let source = source();
    let p = params();
    let e = PalwTransitionExtrasV1::default();
    let point = ctx(1, 100, 1);
    let probe = checker(u64::MAX);
    let mut b = builder(&source, &p, &e, &probe);
    reserve_consumed_queue_workspace_v2(&mut b).unwrap();
    let q = probe.outstanding().0;
    assert_eq!(q, (PALW_V2_MAX_PAYOUTS_PER_BLOCK * std::mem::size_of::<Hash64>() + 3 * std::mem::size_of::<u32>()) as u64);
    assert!(q > 0);
    assert!(b.entries.is_empty());
    drop(b);
    assert_eq!(probe.outstanding(), (0, 0));
    for bytes in [0, q - 1] {
        let c = checker(bytes);
        let mut b = builder(&source, &p, &e, &c);
        assert!(matches!(drain_consumed_queues_v2(&mut b, &point), Err(PalwStateV2Error::NativeResource(_))));
        assert_eq!(b.state, source);
        assert_eq!(b.state.state_root(), source.state_root());
        assert!(b.entries.is_empty());
        assert_eq!(c.outstanding(), (0, 0));
    }
    // Q admits the key scratch only; it is not the row/journal/whole-block cost.
    let c = checker(q);
    let mut b = builder(&source, &p, &e, &c);
    reserve_consumed_queue_workspace_v2(&mut b).unwrap();
    assert_eq!(c.outstanding(), (q, 0));
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));

    let c = checker(u64::MAX);
    let mut b = builder(&source, &p, &e, &c);
    drain_consumed_queues_v2(&mut b, &point).unwrap();
    assert_eq!(b.state.pending_payouts.len(), 2);
    assert!(b.state.evm_settlements.is_empty());
    assert!(c.outstanding().0 > q, "each writer separately admits its journal");
    drop(b);
    assert_eq!(c.outstanding(), (0, 0));
}

#[test]
fn queue_drain_workspace_keeps_prefix_order_legacy_bytes_and_construction_lifetime() {
    let source = source();
    let p = params();
    let e = PalwTransitionExtrasV1::default();
    let point = ctx(1, 100, 1);
    let mut raw = TransitionBuilder::new(&source, &p, false, false, false, false, &e);
    old_drain(&mut raw, &point);
    let c = checker(u64::MAX);
    let mut owned = builder(&source, &p, &e, &c);
    drain_consumed_queues_v2(&mut owned, &point).unwrap();
    assert_eq!(owned.state, raw.state);
    assert_eq!(owned.state.state_root(), raw.state.state_root());
    assert_eq!(borsh::to_vec(&owned.entries).unwrap(), borsh::to_vec(&raw.entries).unwrap());
    let expected: Vec<_> = source.pending_payouts.keys().copied().take(PALW_V2_MAX_PAYOUTS_PER_BLOCK).collect();
    let consumed: Vec<_> = owned
        .entries
        .iter()
        .filter_map(|entry| match entry {
            PalwDeltaEntryV2::Payout { key, old: Some(_), new: None } => Some(*key),
            _ => None,
        })
        .collect();
    assert_eq!(consumed, expected);
    assert_eq!(
        owned.state.pending_payouts.keys().copied().collect::<Vec<_>>(),
        source.pending_payouts.keys().copied().skip(PALW_V2_MAX_PAYOUTS_PER_BLOCK).collect::<Vec<_>>()
    );
    let settlements: Vec<_> = owned
        .entries
        .iter()
        .filter_map(|entry| match entry {
            PalwDeltaEntryV2::EvmSettlement { key, old: Some(row), new: None } => {
                assert_eq!(*row, source.evm_settlements[key]);
                Some(*key)
            }
            _ => None,
        })
        .collect();
    assert_eq!(settlements, vec![0, 1, 2]);
    assert!(matches!(owned.entries[PALW_V2_MAX_PAYOUTS_PER_BLOCK], PalwDeltaEntryV2::EvmSettlement { key: 0, .. }));
    let held = c.outstanding();
    assert!(held.0 > 0);
    // Both key buffers have been consumed, while their admitted construction
    // owner and the real old-row journal are still live in this builder.
    assert_eq!(c.outstanding(), held);
    drop(owned);
    assert_eq!(c.outstanding(), (0, 0));
}

#[test]
fn queue_drain_workspace_is_shared_by_rehearsal_and_actual_fold_with_replay_undo() {
    let source = source();
    let p = params();
    let e = PalwTransitionExtrasV1::default();
    let point = ctx(1, 100, 1);
    let base = palw_v2_pre_object_base_v1(&source, &p, &point, false, false, false, false, &e).unwrap();
    let (after, delta) = apply_palw_transition_v2(&source, &p, &point, &[], None).unwrap();
    assert_eq!(base.pending_payouts, after.pending_payouts);
    assert_eq!(base.evm_settlements, after.evm_settlements);
    assert_eq!(after.pending_payouts.len(), 2);
    assert!(after.evm_settlements.is_empty());
    assert_eq!(apply_delta_v2(&source, &delta, &p).unwrap(), after);
    assert_eq!(revert_delta_v2(&after, &delta, &p).unwrap(), source);
    assert_eq!(apply_delta_v2(&source, &delta, &p).unwrap().state_root(), after.state_root());
    assert_eq!(source.pending_payouts.len(), PALW_V2_MAX_PAYOUTS_PER_BLOCK + 2);
    assert_eq!(source.evm_settlements.len(), 3);
    // Empty queues do not require even one key slot under zero local quota.
    let c = checker(0);
    let mut b = builder(&PalwChainStateV2::genesis(), &p, &e, &c);
    drain_consumed_queues_v2(&mut b, &point).unwrap();
    assert!(b.entries.is_empty());
    assert_eq!(c.outstanding(), (0, 0));
}
