use crate::mempool::{Mempool, errors::RuleResult, model::pool::Pool};
use kaspa_consensus_core::{
    api::{
        ConsensusApi,
        args::{TransactionValidationArgs, TransactionValidationBatchArgs},
    },
    constants::UNACCEPTED_DAA_SCORE,
    tx::{MutableTransaction, UtxoEntry},
};
use kaspa_mining_errors::mempool::RuleError;

/// **G14 r5 (lane r5-escrow-settlement; RFC-0035 §7.3/§7.6): the first input of `tx` that spends a still-pending parent's escrow
/// output** — `escrow_of(parent_id)` names the one output a pending parent locks (a paired TFO1 carrier's own funding output,
/// `palw_tfo1_provisional_lock_v1`). Such a chained spend can only be mined after its parent; if both land in one mergeset the
/// merge refuses it by the provisional lock, and from the next block on the rooted row (or, for a refused contract, nothing) decides
/// it. Refusing it here — instead of relaying a transaction every merge with its parent would skip — keeps mempool and merge on one
/// rule; once the parent is accepted the child is judged by the consensus gate (`PalwEscrowSpendLocksV1`) like any other spend.
pub(crate) fn chained_escrow_spend_v1(
    tx: &kaspa_consensus_core::tx::Transaction,
    escrow_of: impl Fn(&kaspa_consensus_core::tx::TransactionId) -> Option<kaspa_consensus_core::tx::TransactionOutpoint>,
) -> Option<kaspa_consensus_core::tx::TransactionOutpoint> {
    tx.inputs
        .iter()
        .map(|input| input.previous_outpoint)
        .find(|outpoint| escrow_of(&outpoint.transaction_id).is_some_and(|locked| locked == *outpoint))
}

impl Mempool {
    /// [`chained_escrow_spend_v1`] over this pool's pending transactions. Node policy (no consensus effect); below
    /// `palw_tir_public_conviction_v1` a paired TFO1 carrier's output is an ordinary payer output, and a chained spend of it is only
    /// delayed until its parent is accepted.
    pub(crate) fn refuse_chained_escrow_spend_v1(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        let pending_escrow = |parent: &kaspa_consensus_core::tx::TransactionId| {
            self.transaction_pool
                .get(parent)
                .and_then(|parent| kaspa_consensus_core::palw_funded_escrow_v1::palw_tfo1_provisional_lock_v1(&parent.mtx.tx))
        };
        match chained_escrow_spend_v1(&transaction.tx, pending_escrow) {
            Some(outpoint) => Err(RuleError::RejectNonStandard(
                transaction.id(),
                format!("spends {outpoint}, the funding output of a pending funded-original (TFO1) carrier, before it is accepted"),
            )),
            None => Ok(()),
        }
    }

    pub(crate) fn populate_mempool_entries(&self, transaction: &mut MutableTransaction) {
        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            if let Some(parent) = self.transaction_pool.get(&input.previous_outpoint.transaction_id) {
                let output = &parent.mtx.tx.outputs[input.previous_outpoint.index as usize];
                transaction.entries[i] =
                    Some(UtxoEntry::new(output.value, output.script_public_key.clone(), UNACCEPTED_DAA_SCORE, false));
            }
        }
    }
}

pub(crate) fn validate_mempool_transaction(
    consensus: &dyn ConsensusApi,
    transaction: &mut MutableTransaction,
    args: &TransactionValidationArgs,
) -> RuleResult<()> {
    Ok(consensus.validate_mempool_transaction(transaction, args)?)
}

pub(crate) fn validate_mempool_transactions_in_parallel(
    consensus: &dyn ConsensusApi,
    transactions: &mut [MutableTransaction],
    args: &TransactionValidationBatchArgs,
) -> Vec<RuleResult<()>> {
    consensus.validate_mempool_transactions_in_parallel(transactions, args).into_iter().map(|x| x.map_err(RuleError::from)).collect()
}

pub(crate) fn populate_mempool_transactions_in_parallel(
    consensus: &dyn ConsensusApi,
    transactions: &mut [MutableTransaction],
) -> Vec<RuleResult<()>> {
    consensus.populate_mempool_transactions_in_parallel(transactions).into_iter().map(|x| x.map_err(RuleError::from)).collect()
}

#[cfg(test)]
mod chained_escrow_tests {
    use super::chained_escrow_spend_v1;
    use kaspa_consensus_core::{
        Hash64,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{Transaction, TransactionInput, TransactionOutpoint},
    };

    /// G14 r5 item 4: only the exact escrow output a pending parent locks refuses a chained spend; its other outputs, other parents
    /// and confirmed outpoints do not.
    #[test]
    fn mempool_refuses_only_a_chained_spend_of_a_pending_escrow_output() {
        let parent = Hash64::from_u64_word(7);
        let locked = TransactionOutpoint::new(parent, 1);
        let escrow_of = |id: &Hash64| (*id == parent).then_some(locked);
        let child = |outpoints: Vec<TransactionOutpoint>| {
            Transaction::new(
                0,
                outpoints.into_iter().map(|o| TransactionInput::new(o, vec![], 0, 0)).collect(),
                vec![],
                0,
                SUBNETWORK_ID_NATIVE,
                0,
                vec![],
            )
        };
        assert_eq!(chained_escrow_spend_v1(&child(vec![TransactionOutpoint::new(Hash64::from_u64_word(9), 0), locked]), escrow_of), Some(locked));
        assert_eq!(chained_escrow_spend_v1(&child(vec![TransactionOutpoint::new(parent, 0)]), escrow_of), None);
        assert_eq!(chained_escrow_spend_v1(&child(vec![TransactionOutpoint::new(Hash64::from_u64_word(9), 1)]), escrow_of), None);
    }
}
