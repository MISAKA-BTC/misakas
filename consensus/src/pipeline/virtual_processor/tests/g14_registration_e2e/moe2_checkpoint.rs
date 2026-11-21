//! Run the projected reserve and pool moves through the native state delta and
//! carriage codecs. The fixture retains these roots outside the selected chain.
use super::*;
use kaspa_consensus_core::palw_bond_budget_v1::PalwBondBudgetStateV1;
use kaspa_consensus_core::palw_model_market_v1::moe2::settlement::{
    Moe2SettledReward, Moe2SettlementBook, Moe2SettlementUndo, Moe2WorkerRelease,
};
use kaspa_consensus_core::palw_state_v2::{PalwDeltaEntryV2, PalwStateCarriageV2, PalwStateDeltaV2, apply_delta_v2, revert_delta_v2};
use std::path::Path;

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
