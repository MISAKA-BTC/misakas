use super::*;
use kaspa_consensus_core::{
    BlockHash,
    api::ConsensusApi,
    errors::consensus::{ConsensusError, ConsensusResult},
    evm::{CanonicalEvmHeads, EvmBlockResponse},
};
use std::sync::Mutex;

struct Reader {
    result: ConsensusResult<Option<EvmBlockResponse>>,
    canonical: ConsensusResult<bool>,
    calls: Mutex<Vec<String>>,
}
impl Reader {
    fn new(result: ConsensusResult<Option<EvmBlockResponse>>) -> Self {
        Self { result, canonical: Ok(true), calls: Mutex::new(Vec::new()) }
    }
    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
}
impl ConsensusApi for Reader {
    fn get_evm_block_by_number(&self, n: u64) -> ConsensusResult<Option<EvmBlockResponse>> {
        self.record(format!("number:{n}"));
        self.result.clone()
    }
    fn get_evm_block_by_rpc_hash(&self, _: EvmH256) -> ConsensusResult<Option<EvmBlockResponse>> {
        self.record("hash".into());
        self.result.clone()
    }
    fn is_chain_block(&self, _: BlockHash) -> ConsensusResult<bool> {
        self.record("canonical".into());
        self.canonical.clone()
    }
    fn get_evm_canonical_heads(&self) -> ConsensusResult<Option<CanonicalEvmHeads>> {
        self.record("heads".into());
        Ok(Some(CanonicalEvmHeads { latest: hash(1), safe: hash(2), finalized: hash(3) }))
    }
}
fn hash(n: u8) -> BlockHash {
    BlockHash::from_bytes([n; 64])
}
fn response() -> EvmBlockResponse {
    EvmBlockResponse { header: Default::default(), l1_hash: hash(9), tx_hashes: vec![], encoded_size: 0 }
}
fn selectors() -> Vec<BlockId> {
    vec![BlockId::Number(42), BlockId::Tag("earliest".into()), BlockId::Hash { hash: [4; 32], require_canonical: false }]
}

#[test]
fn state_selector_api_errors_stop_actual_continuation_instead_of_empty_or_unknown() {
    for selector in selectors() {
        let reader = Reader::new(Err(ConsensusError::General("selector read fault")));
        let calls = std::cell::Cell::new(0);
        let result = with_resolved_state_selector(&reader, &selector, |_| {
            calls.set(calls.get() + 1);
            Ok(())
        });
        assert_eq!(result.unwrap_err().code, -32000);
        assert_eq!(calls.get(), 0, "account/snapshot downstream must not run");
        assert_eq!(reader.calls.lock().unwrap().len(), 1);
    }
}

#[test]
fn state_selector_real_absence_and_exact_target_remain_distinct() {
    for selector in selectors() {
        for (result, expected) in [(Ok(None), None), (Ok(Some(response())), Some(hash(9)))] {
            let reader = Reader::new(result);
            assert_eq!(with_resolved_state_selector(&reader, &selector, Ok).unwrap(), expected);
            assert_eq!(reader.calls.lock().unwrap().len(), 1);
        }
    }
}

#[test]
fn state_selector_canonical_denial_is_not_a_read_error_and_opt_out_does_not_read() {
    let selector = BlockId::Hash { hash: [4; 32], require_canonical: true };
    for (canonical, code) in [(Ok(false), -32602), (Err(ConsensusError::General("canonical fault")), -32000)] {
        let mut reader = Reader::new(Ok(Some(response())));
        reader.canonical = canonical;
        let result = with_resolved_state_selector(&reader, &selector, |_| panic!("downstream after refusal"));
        let result: EthResult<()> = result;
        assert_eq!(result.unwrap_err().code, code);
        assert_eq!(*reader.calls.lock().unwrap(), vec!["hash", "canonical"]);
    }
    let mut reader = Reader::new(Ok(Some(response())));
    reader.canonical = Err(ConsensusError::General("unused canonical fault"));
    assert_eq!(
        with_resolved_state_selector(&reader, &BlockId::Hash { hash: [4; 32], require_canonical: false }, Ok).unwrap(),
        Some(hash(9))
    );
    assert_eq!(*reader.calls.lock().unwrap(), vec!["hash"]);
}

#[test]
fn state_selector_existing_tags_use_exact_canonical_head_source() {
    for (tag, expected) in [("latest", 1), ("pending", 1), ("safe", 2), ("finalized", 3)] {
        let reader = Reader::new(Err(ConsensusError::General("number/hash must not run")));
        assert_eq!(with_resolved_state_selector(&reader, &BlockId::Tag(tag.into()), Ok).unwrap(), Some(hash(expected)));
        assert_eq!(*reader.calls.lock().unwrap(), vec!["heads"]);
    }
    let reader = Reader::new(Ok(None));
    assert_eq!(with_resolved_state_selector(&reader, &BlockId::Tag("earliest".into()), Ok).unwrap(), None);
    assert_eq!(*reader.calls.lock().unwrap(), vec!["number:0"]);
}
