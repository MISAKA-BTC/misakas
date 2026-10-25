//! Actual RPC continuation boundary. Receipt payloads here are adapter inputs,
//! not evidence that a bridge transaction executed or a market filled.
use super::*;
use kaspa_consensus_core::{
    BlockHash,
    api::ConsensusApi,
    errors::consensus::{ConsensusError, ConsensusResult},
    evm::{EvmLog, EvmReceipt, EvmTxReceiptView},
};
use std::{cell::Cell, sync::Mutex};
struct Reader {
    result: ConsensusResult<Option<EvmTxReceiptView>>,
    reads: Mutex<Vec<EvmH256>>,
}
impl ConsensusApi for Reader {
    fn get_evm_tx_receipt(&self, hash: EvmH256) -> ConsensusResult<Option<EvmTxReceiptView>> {
        self.reads.lock().unwrap().push(hash);
        self.result.clone()
    }
}
fn tx() -> EvmH256 {
    EvmH256::from_bytes([3; 32])
}
#[test]
fn receipt_rpc_fault_stops_enrichment_rendering_instead_of_unknown_null() {
    for error in [
        ConsensusError::GeneralOwned("cold receipt row malformed".into()),
        ConsensusError::General("canonical evidence unavailable"),
        ConsensusError::HeaderNotFound(BlockHash::from_bytes([4; 64])),
    ] {
        let reader = Reader { result: Err(error), reads: Mutex::new(vec![]) };
        let downstream = Cell::new(0);
        let actual = with_canonical_receipt_v2(&reader, tx(), |_| {
            downstream.set(downstream.get() + 1);
            Ok(())
        });
        assert!(actual.is_err());
        assert_eq!(downstream.get(), 0);
        assert_eq!(*reader.reads.lock().unwrap(), [tx()]);
    }
}
#[test]
fn receipt_rpc_true_absence_stays_null_without_unused_enrichment() {
    let reader = Reader { result: Ok(None), reads: Mutex::new(vec![]) };
    let downstream = Cell::new(0);
    let actual = with_canonical_receipt_v2(&reader, tx(), |_| {
        downstream.set(downstream.get() + 1);
        Ok(())
    })
    .unwrap();
    assert!(actual.is_none());
    assert_eq!(downstream.get(), 0);
    assert_eq!(*reader.reads.lock().unwrap(), [tx()]);
}
#[test]
fn receipt_rpc_positive_preserves_exact_accepting_identity_logs_and_metadata() {
    let expected = EvmTxReceiptView {
        accepting_block: BlockHash::from_bytes([7; 64]),
        evm_number: 19,
        receipt_index: 2,
        log_index_offset: 5,
        receipt: EvmReceipt {
            succeeded: true,
            gas_used: 21_123,
            cumulative_gas_used: 44_555,
            logs: vec![EvmLog { data: vec![9, 8, 7], ..Default::default() }],
        },
    };
    let reader = Reader { result: Ok(Some(expected.clone())), reads: Mutex::new(vec![]) };
    let downstream = Cell::new(0);
    let actual = with_canonical_receipt_v2(&reader, tx(), |view| {
        downstream.set(downstream.get() + 1);
        Ok(view)
    })
    .unwrap()
    .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(downstream.get(), 1);
    assert_eq!(*reader.reads.lock().unwrap(), [tx()]);
}
