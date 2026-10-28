//! Checked, once-only allocation of a carrier's actual input/output difference.
//! This is not UTXO validation, acceptance, a payment receipt or escrow funding.
//! Native callers must supply entries from their composed UTXO view and retain
//! ordinary script validation. In particular an escrow output is already in
//! `total_outputs`; it cannot also increase the available fee.

use crate::tx::{PopulatedTransaction, TransactionOutpoint};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarrierFeeRefusalV1 {
    StaleTransactionId,
    MissingEntries,
    DuplicateInput,
    Overflow,
    OutputsExceedInputs,
    DuplicateAllocation,
    InsufficientFee,
}

/// Local prepare result only. It never authorizes a spend or establishes that
/// the transaction was accepted. No persisted caller-supplied `paid` flag exists.
#[derive(Debug)]
pub struct CarrierFeeBudgetV1 {
    carrier: crate::TransactionId,
    actual_fee: u64,
    remaining: u64,
    allocations: BTreeMap<[u8; 64], u64>,
}

impl CarrierFeeBudgetV1 {
    pub fn from_populated_v1(tx: &PopulatedTransaction<'_>) -> Result<Self, CarrierFeeRefusalV1> {
        if !tx.tx.id_is_current() {
            return Err(CarrierFeeRefusalV1::StaleTransactionId);
        }
        if tx.entries.len() != tx.tx.inputs.len() {
            return Err(CarrierFeeRefusalV1::MissingEntries);
        }
        let mut inputs = std::collections::HashSet::<TransactionOutpoint>::new();
        let mut total_in = 0u64;
        for (input, entry) in tx.tx.inputs.iter().zip(&tx.entries) {
            if !inputs.insert(input.previous_outpoint) {
                return Err(CarrierFeeRefusalV1::DuplicateInput);
            }
            total_in = total_in.checked_add(entry.amount).ok_or(CarrierFeeRefusalV1::Overflow)?;
        }
        let total_out =
            tx.tx.outputs.iter().try_fold(0u64, |sum, output| sum.checked_add(output.value).ok_or(CarrierFeeRefusalV1::Overflow))?;
        let actual_fee = total_in.checked_sub(total_out).ok_or(CarrierFeeRefusalV1::OutputsExceedInputs)?;
        Ok(Self { carrier: tx.tx.id(), actual_fee, remaining: actual_fee, allocations: BTreeMap::new() })
    }

    /// Allocation identities must be derived from the native contract/operation
    /// and purpose. Choosing arbitrary identities does not establish eligibility.
    /// All old burn, rent and new fee assignments must share this same budget.
    pub fn allocate_v1(&mut self, identity: [u8; 64], amount: u64) -> Result<(), CarrierFeeRefusalV1> {
        if self.allocations.contains_key(&identity) {
            return Err(CarrierFeeRefusalV1::DuplicateAllocation);
        }
        let remaining = self.remaining.checked_sub(amount).ok_or(CarrierFeeRefusalV1::InsufficientFee)?;
        self.allocations.insert(identity, amount);
        self.remaining = remaining;
        Ok(())
    }

    pub fn carrier_v1(&self) -> crate::TransactionId {
        self.carrier
    }
    pub fn actual_fee_v1(&self) -> u64 {
        self.actual_fee
    }
    pub fn remaining_v1(&self) -> u64 {
        self.remaining
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutput, UtxoEntry};

    fn tx(amounts: &[u64], outputs: &[u64]) -> (Transaction, Vec<UtxoEntry>) {
        let inputs = amounts
            .iter()
            .enumerate()
            .map(|(i, _)| TransactionInput::new(TransactionOutpoint::new(crate::Hash64::from_bytes([i as u8; 64]), 0), vec![], 0, 0))
            .collect();
        let outputs = outputs.iter().map(|amount| TransactionOutput::new(*amount, ScriptPublicKey::default())).collect();
        let tx = Transaction::new(0, inputs, outputs, 0, crate::subnets::SUBNETWORK_ID_NATIVE, 0, vec![]);
        let entries = amounts.iter().map(|amount| UtxoEntry::new(*amount, ScriptPublicKey::default(), 1, false)).collect();
        (tx, entries)
    }

    #[test]
    fn carrier_fee_assignments_share_actual_difference_and_refuse_atomically() {
        // 80 is a real output (e.g. funding), never an additional fee credit.
        let (tx, entries) = tx(&[100], &[80]);
        let mut budget = CarrierFeeBudgetV1::from_populated_v1(&PopulatedTransaction::new(&tx, entries)).unwrap();
        assert_eq!(budget.actual_fee_v1(), 20);
        assert_eq!(budget.carrier_v1(), tx.id());
        budget.allocate_v1([1; 64], 12).unwrap();
        assert_eq!(budget.allocate_v1([2; 64], 12), Err(CarrierFeeRefusalV1::InsufficientFee));
        assert_eq!(budget.remaining_v1(), 8);
        assert_eq!(budget.allocate_v1([1; 64], 1), Err(CarrierFeeRefusalV1::DuplicateAllocation));
        budget.allocate_v1([2; 64], 8).unwrap();
        assert_eq!(budget.remaining_v1(), 0);
    }

    #[test]
    fn carrier_fee_rejects_overflow_underflow_missing_or_duplicate_inputs() {
        for (amounts, outputs, expected) in [
            (vec![u64::MAX, 1], vec![0], CarrierFeeRefusalV1::Overflow),
            (vec![u64::MAX], vec![u64::MAX, 1], CarrierFeeRefusalV1::Overflow),
            (vec![1], vec![2], CarrierFeeRefusalV1::OutputsExceedInputs),
        ] {
            let (tx, entries) = tx(&amounts, &outputs);
            assert_eq!(CarrierFeeBudgetV1::from_populated_v1(&PopulatedTransaction::new(&tx, entries)).unwrap_err(), expected);
        }
        let (mut tx, entries) = tx(&[10, 10], &[1]);
        assert_eq!(
            CarrierFeeBudgetV1::from_populated_v1(&PopulatedTransaction { tx: &tx, entries: vec![] }).unwrap_err(),
            CarrierFeeRefusalV1::MissingEntries
        );
        tx.inputs[1].previous_outpoint = tx.inputs[0].previous_outpoint;
        assert_eq!(
            CarrierFeeBudgetV1::from_populated_v1(&PopulatedTransaction::new(&tx, entries.clone())).unwrap_err(),
            CarrierFeeRefusalV1::StaleTransactionId
        );
        tx.finalize();
        assert_eq!(
            CarrierFeeBudgetV1::from_populated_v1(&PopulatedTransaction::new(&tx, entries)).unwrap_err(),
            CarrierFeeRefusalV1::DuplicateInput
        );
    }
}
