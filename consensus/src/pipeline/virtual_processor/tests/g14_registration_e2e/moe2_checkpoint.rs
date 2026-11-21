//! Run the projected reserve and pool moves through the native state delta and
//! carriage codecs. The fixture retains these roots outside the selected chain.
use super::*;
use kaspa_consensus_core::palw_bond_budget_v1::PalwBondBudgetStateV1;
use kaspa_consensus_core::palw_model_market_v1::moe2::settlement::LockedLiquidityReceipt;
use kaspa_consensus_core::palw_state_v2::{PalwDeltaEntryV2, PalwStateCarriageV2, PalwStateDeltaV2, apply_delta_v2, revert_delta_v2};
use std::path::Path;

fn budget_entries(writes: &[PalwBudgetWriteV1]) -> impl Iterator<Item = PalwDeltaEntryV2> + '_ {
    writes.iter().map(|w| match w.clone() {
        PalwBudgetWriteV1::Row { table, key, old, new } => PalwDeltaEntryV2::BondBudgetRow { table, key, old, new },
        PalwBudgetWriteV1::Header { old, new } => PalwDeltaEntryV2::BondBudgetHeader { old, new },
    })
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
        execution: Hash64,
        reservation: &[PalwBudgetWriteV1],
        debit: &[PalwBudgetWriteV1],
        receipt: LockedLiquidityReceipt,
    ) -> Self {
        assert_eq!(receipt.position.point.block, execution);
        assert_eq!(debit.len(), 1);
        let vp = env.chain.vp();
        let store = vp.palw_state_v2_store.read();
        let (published_parent, mut enrollment) = store.delta_of(enrollment).unwrap();
        let (published_child, mut executed) = store.delta_of(execution).unwrap();
        drop(store);
        let params = &env.bundle.state;
        let canonical_parent = apply_delta_v2(registration, &enrollment, params).unwrap();
        assert_eq!(canonical_parent.state_root(), published_parent);
        assert_eq!(apply_delta_v2(&canonical_parent, &executed, params).unwrap().state_root(), published_child);
        enrollment.entries.extend(budget_entries(reservation));
        let parent = apply_delta_v2(registration, &enrollment, params).unwrap();
        executed.entries.extend(budget_entries(debit));
        executed.entries.push(PalwDeltaEntryV2::ModelMarket {
            key: receipt.model,
            old: Some(receipt.before),
            new: Some(receipt.after),
        });
        let child = apply_delta_v2(&parent, &executed, params).unwrap();
        assert_eq!(child.model_market(&receipt.model), Some(&receipt.after));
        assert_ne!(parent.state_root(), published_parent);
        assert_ne!(child.state_root(), published_child);
        std::fs::write(checkpoint_dir.join("native-carriage"), borsh::to_vec(&PalwStateCarriageV2::from_state(&child)).unwrap())
            .unwrap();
        std::fs::write(checkpoint_dir.join("enrollment.delta"), borsh::to_vec(&enrollment).unwrap()).unwrap();
        std::fs::write(checkpoint_dir.join("settlement.delta"), borsh::to_vec(&executed).unwrap()).unwrap();
        Self { store: checkpoint_dir, parent_root: parent.state_root(), child_root: child.state_root() }
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
