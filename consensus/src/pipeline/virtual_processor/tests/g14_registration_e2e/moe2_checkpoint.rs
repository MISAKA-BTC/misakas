//! Run the projected reserve and pool moves through the native state delta and
//! carriage codecs. The fixture retains these roots outside the selected chain.
use super::*;
use kaspa_consensus_core::palw_bond_budget_v1::PalwBondBudgetStateV1;
use kaspa_consensus_core::palw_model_market_v1::moe2::settlement::{
    Moe2ExecSlot, Moe2SettledReward, Moe2SettlementBook, Moe2SettlementUndo, Moe2WorkerRelease,
};
use kaspa_consensus_core::palw_state_v2::{PalwDeltaEntryV2, PalwStateCarriageV2, PalwStateDeltaV2, apply_delta_v2, revert_delta_v2};
use std::path::Path;

pub(super) fn check_exec_settlement_rewind(
    env: &Env,
    registration: &PalwChainStateV2,
    registered_book: &Moe2SettlementBook,
    close: Hash64,
    position: Moe2Position,
    quote: Moe2PreDrawQuote,
) {
    let params = &env.bundle.state;
    let config = &env.config.params;
    let vp = env.chain.vp();
    let store = vp.palw_state_v2_store.read();
    let (_, mut close_delta) = store.delta_of(close).unwrap();
    let (_, mut award_delta) = store.delta_of(position.point.block).unwrap();
    drop(store);
    let slot = Moe2ExecSlot { registration: [24; 64], slot: 1, close, close_daa: close_delta.point.daa_score, quote };
    let claim = Hash64::from_u64_word(642);
    let mut book = registered_book.clone();
    let unreserved = apply_delta_v2(registration, &close_delta, params).unwrap();
    let (capital, ask) = quote.budget_ask_at(config, &unreserved, slot.close_daa).unwrap();
    let mut budget = unreserved.bond_budget().unwrap().clone();
    let mut writes = Vec::new();
    budget
        .reserve_claim_v1(
            &params.bond_budget().unwrap().policy,
            capital,
            slot.reservation_key(),
            quote.funding_bond,
            Some(quote.model),
            slot.close_daa,
            ask,
            PalwBudgetOriginV1::Attempt,
            &mut writes,
        )
        .unwrap();
    close_delta.entries.extend(writes.into_iter().map(PalwDeltaEntryV2::from));
    let reserved = apply_delta_v2(registration, &close_delta, params).unwrap();
    book.admit_exec_slot(claim, &slot, reserved.bond_budget().unwrap(), |anchor, id| {
        (anchor == quote.quote_anchor && id == quote.model).then(|| *registered_book.model(&id).unwrap())
    })
    .unwrap();
    let admitted = book.clone();
    let original_delta = award_delta.clone();
    let mut child = apply_delta_v2(&reserved, &award_delta, params).unwrap();
    let before = child.state_root();
    let reward = quote.limits.max_reward;
    let release = Moe2WorkerRelease {
        reward,
        miner_now: reward * 4 / 10,
        panel_now: reward / 10,
        liability_until: quote.deadline,
        signed_hard_deadline: quote.deadline,
        material_ttl_until: quote.deadline,
        vest_until: None,
        retire_not_before: quote.deadline,
    };
    let (payment, undo) =
        child.settle_private_moe2_exec_slot(config, &mut book, &mut award_delta, claim, position, release, &slot).unwrap();
    let undo: Moe2SettlementUndo = borsh::from_slice(&borsh::to_vec(&undo).unwrap()).unwrap();
    let after = child.state_root();
    assert_eq!(apply_delta_v2(&reserved, &award_delta, params).unwrap().state_root(), after);
    let mut carried = PalwStateCarriageV2::from_state(&child);
    let carried_bytes = borsh::to_vec(&carried).unwrap();
    kaspa_consensus_core::palw_native_resource_v1::preflight_native_carriage_v1(carried_bytes.as_slice(), carried_bytes.len() as u64)
        .unwrap();
    let reopened: PalwStateCarriageV2 = borsh::from_slice(&carried_bytes).unwrap();
    child = reopened.into_state(params, Some(after)).unwrap();
    let receipt = child.private_moe2_exec_settlement(&claim).unwrap();
    assert_eq!((receipt.reservation, receipt.reward, receipt.position), (slot.reservation_key(), payment, position));
    assert_eq!(receipt.permanent_liquidity.as_ref(), book.receipt(&claim));
    carried.private_moe2_exec_settlements.clear();
    assert!(carried.into_state(params, Some(after)).is_err(), "omitting a settled worker and LP receipt changes the root");
    assert!(
        revert_delta_v2(&child, &award_delta, params).unwrap().private_moe2_exec_settlement(&claim).is_none(),
        "the normal native journal also clears the row on a disconnect",
    );
    let (book_root, bytes) = book.checkpoint(after);
    book = Moe2SettlementBook::from_checkpoint(after, book_root, &bytes, bytes.len()).unwrap();
    let paid_book = book.clone();
    let mut sibling = award_delta.clone();
    sibling.point.block = Hash64::from_u64_word(643);
    assert_eq!(
        child.revert_private_moe2_exec_slot(config, &mut book, &mut sibling, &slot, &undo),
        Err(Moe2SettlementError::UndoMismatch)
    );
    let mut truncated = award_delta.clone();
    truncated.entries.pop();
    assert_eq!(
        child.revert_private_moe2_exec_slot(config, &mut book, &mut truncated, &slot, &undo),
        Err(Moe2SettlementError::UndoMismatch)
    );
    let other = Moe2ExecSlot { close: Hash64::from_u64_word(644), ..slot };
    assert_eq!(
        child.revert_private_moe2_exec_slot(config, &mut book, &mut award_delta, &other, &undo),
        Err(Moe2SettlementError::UndoMismatch)
    );
    assert_eq!((child.state_root(), &book), (after, &paid_book));
    child.revert_private_moe2_exec_slot(config, &mut book, &mut award_delta, &slot, &undo).unwrap();
    assert_eq!((child.state_root(), &book, &award_delta.entries), (before, &admitted, &original_delta.entries));
    assert_eq!(revert_delta_v2(&child, &award_delta, params).unwrap().state_root(), reserved.state_root());
    assert!(child.revert_private_moe2_exec_slot(config, &mut book, &mut award_delta, &slot, &undo).is_err());
    let (repaid, _) =
        child.settle_private_moe2_exec_slot(config, &mut book, &mut award_delta, claim, position, release, &slot).unwrap();
    assert_eq!((repaid, child.state_root(), book), (payment, after, paid_book));
    let market = payment.allocation.market.unwrap();
    println!(
        "MOE2 EXEC rollback pid={} block={} before={before} settled={after} book={book_root} purchase={} lp={} burned={} miner={} panel={}",
        std::process::id(),
        position.point.block,
        market.split.purchase,
        market.split.liquidity,
        market.retired_units,
        payment.miner_now,
        payment.panel_now,
    );
}

pub(super) struct NativeSettlement<'a> {
    store: &'a Path,
    pub(super) parent_root: Hash64,
    pub(super) child_root: Hash64,
}

impl<'a> NativeSettlement<'a> {
    pub(super) fn project(
        checkpoint_dir: &'a Path,
        env: &Env,
        registration: &PalwChainStateV2,
        enrollment: Hash64,
        reservation: &[PalwBudgetWriteV1],
        book: &mut Moe2SettlementBook,
        terms: &Moe2SettlementTerms,
        at: Moe2Position,
        release: Moe2WorkerRelease,
    ) -> (Self, Moe2SettledReward, Moe2SettlementUndo) {
        let vp = env.chain.vp();
        let store = vp.palw_state_v2_store.read();
        let (published_parent, mut enrollment) = store.delta_of(enrollment).unwrap();
        let (published_child, mut executed) = store.delta_of(at.point.block).unwrap();
        drop(store);
        let params = &env.bundle.state;
        let canonical_parent = apply_delta_v2(registration, &enrollment, params).unwrap();
        assert_eq!(canonical_parent.state_root(), published_parent);
        let mut unfunded = apply_delta_v2(&canonical_parent, &executed, params).unwrap();
        assert_eq!(unfunded.state_root(), published_child);
        let admitted = book.clone();
        let base_len = executed.entries.len();
        assert_eq!(
            unfunded.settle_private_moe2(&env.config.params, book, &mut executed, terms.claim, at, release),
            Err(Moe2SettlementError::ReservationMismatch)
        );
        assert_eq!(unfunded.state_root(), published_child);
        assert_eq!(*book, admitted);
        assert_eq!(executed.entries.len(), base_len);

        enrollment.entries.extend(reservation.iter().cloned().map(PalwDeltaEntryV2::from));
        let parent = apply_delta_v2(registration, &enrollment, params).unwrap();
        let mut child = apply_delta_v2(&parent, &executed, params).unwrap();
        let before_settlement = child.state_root();
        let wrong_release = Moe2WorkerRelease { panel_now: release.panel_now + 1, ..release };
        assert!(child.settle_private_moe2(&env.config.params, book, &mut executed, terms.claim, at, wrong_release).is_err());
        let public = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        assert_eq!(
            child.settle_private_moe2(&public, book, &mut executed, terms.claim, at, release),
            Err(Moe2SettlementError::UnknownClaimContext)
        );
        assert_eq!((child.state_root(), executed.entries.len(), &*book), (before_settlement, base_len, &admitted));
        let (reward, undo) = child.settle_private_moe2(&env.config.params, book, &mut executed, terms.claim, at, release).unwrap();
        assert_eq!(book.entitlement(&terms.claim), Some(&reward));
        let child_root = child.state_root();
        assert_eq!(apply_delta_v2(&parent, &executed, params).unwrap().state_root(), child_root);
        assert_eq!(
            child.settle_private_moe2(&env.config.params, book, &mut executed, terms.claim, at, release),
            Err(Moe2SettlementError::AlreadySettled)
        );
        assert_eq!((child.state_root(), executed.entries.len()), (child_root, base_len + 2));
        assert_ne!(parent.state_root(), published_parent);
        assert_ne!(child.state_root(), published_child);
        std::fs::write(checkpoint_dir.join("native-carriage"), borsh::to_vec(&PalwStateCarriageV2::from_state(&child)).unwrap())
            .unwrap();
        std::fs::write(checkpoint_dir.join("enrollment.delta"), borsh::to_vec(&enrollment).unwrap()).unwrap();
        std::fs::write(checkpoint_dir.join("settlement.delta"), borsh::to_vec(&executed).unwrap()).unwrap();
        (Self { store: checkpoint_dir, parent_root: parent.state_root(), child_root }, reward, undo)
    }

    pub(super) fn reload(&self, env: &Env) -> PalwChainStateV2 {
        let carriage: PalwStateCarriageV2 = borsh::from_slice(&std::fs::read(self.store.join("native-carriage")).unwrap()).unwrap();
        carriage.into_state(&env.bundle.state, Some(self.child_root)).unwrap()
    }

    pub(super) fn disconnect_and_replay(
        &self,
        env: &Env,
        registration: &PalwChainStateV2,
        child: &PalwChainStateV2,
        reverted_budget: &PalwBondBudgetStateV1,
        inverse: &[PalwBudgetWriteV1],
    ) {
        let params = &env.bundle.state;
        let enrollment: PalwStateDeltaV2 = borsh::from_slice(&std::fs::read(self.store.join("enrollment.delta")).unwrap()).unwrap();
        let executed: PalwStateDeltaV2 = borsh::from_slice(&std::fs::read(self.store.join("settlement.delta")).unwrap()).unwrap();
        let parent = revert_delta_v2(child, &executed, params).unwrap();
        assert_eq!(parent.state_root(), self.parent_root);
        let PalwDeltaEntryV2::BondBudgetRow { table, key, old, new } = &executed.entries[executed.entries.len() - 2] else {
            panic!("native debit")
        };
        assert_eq!(inverse, &[PalwBudgetWriteV1::Row { table: *table, key: key.clone(), old: new.clone(), new: old.clone() }]);
        // The block may also have journaled ordinary accrual. Compare the
        // budgets at the same position before disconnecting the whole block.
        let mut before_move = executed.clone();
        before_move.entries.truncate(before_move.entries.len() - 2);
        assert_eq!(apply_delta_v2(&parent, &before_move, params).unwrap().bond_budget(), Some(reverted_budget));
        assert_eq!(revert_delta_v2(&parent, &enrollment, params).unwrap().state_root(), registration.state_root());
        assert_eq!(apply_delta_v2(&parent, &executed, params).unwrap().state_root(), self.child_root);
    }
}
