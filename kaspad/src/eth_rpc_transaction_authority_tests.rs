//! Actual four production continuation helpers. No execution/maturity facts are
//! planted; synthetic payloads exercise the API error/absence boundary only.
use super::*;
use kaspa_consensus_core::{
    BlockHash,
    api::ConsensusApi,
    errors::consensus::{ConsensusError, ConsensusResult},
    evm::*,
};
use std::{cell::Cell, sync::Mutex};
struct Reader {
    raw: ConsensusResult<Option<Vec<u8>>>,
    locations: ConsensusResult<EvmTxLocations>,
    receipt: ConsensusResult<Option<EvmTxReceiptView>>,
    body: ConsensusResult<Option<EvmTraceReplayBodyV1>>,
    header: ConsensusResult<Option<EvmExecutionHeader>>,
    snapshot: ConsensusResult<Option<EvmStateSnapshot>>,
    calls: Mutex<Vec<&'static str>>,
}
fn tx() -> EvmH256 {
    EvmH256::from_bytes([3; 32])
}
fn receipt() -> EvmTxReceiptView {
    EvmTxReceiptView {
        accepting_block: BlockHash::from_u64_word(4),
        evm_number: 8,
        receipt_index: 1,
        log_index_offset: 2,
        receipt: EvmReceipt::default(),
    }
}
fn body() -> EvmTraceReplayBodyV1 {
    EvmTraceReplayBodyV1 {
        selected_parent: BlockHash::from_u64_word(5),
        env: EvmReplayEnv::default(),
        system_ops: vec![],
        txs: vec![],
    }
}
fn fault<T>() -> ConsensusResult<T> {
    Err(ConsensusError::GeneralOwned("actual API read failed".into()))
}
impl Reader {
    fn new() -> Self {
        Self {
            raw: Ok(Some(vec![9, 8, 7])),
            locations: Ok(EvmTxLocations { last_skip_class: Some(2), ..Default::default() }),
            receipt: Ok(Some(receipt())),
            body: Ok(Some(body())),
            header: Ok(Some(EvmExecutionHeader::default())),
            snapshot: Ok(Some(EvmStateSnapshot::default())),
            calls: Mutex::new(vec![]),
        }
    }
    fn read(&self, name: &'static str) {
        self.calls.lock().unwrap().push(name);
    }
}
impl ConsensusApi for Reader {
    fn get_evm_raw_tx(&self, _: EvmH256) -> ConsensusResult<Option<Vec<u8>>> {
        self.read("raw");
        self.raw.clone()
    }
    fn get_evm_tx_locations(&self, _: EvmH256) -> ConsensusResult<EvmTxLocations> {
        self.read("locations");
        self.locations.clone()
    }
    fn get_evm_tx_receipt(&self, _: EvmH256) -> ConsensusResult<Option<EvmTxReceiptView>> {
        self.read("receipt");
        self.receipt.clone()
    }
    fn get_evm_trace_replay_body(&self, _: BlockHash) -> ConsensusResult<Option<EvmTraceReplayBodyV1>> {
        self.read("body");
        self.body.clone()
    }
    fn get_evm_header_of(&self, _: BlockHash) -> ConsensusResult<Option<EvmExecutionHeader>> {
        self.read("header");
        self.header.clone()
    }
    fn get_evm_state_snapshot_of(&self, _: BlockHash) -> ConsensusResult<Option<EvmStateSnapshot>> {
        self.read("snapshot");
        self.snapshot.clone()
    }
}
#[test]
fn transaction_authority_candidate_read_faults_stop_before_head_or_execution() {
    for failed in ["raw", "locations", "receipt"] {
        let mut r = Reader::new();
        match failed {
            "raw" => r.raw = fault(),
            "locations" => r.locations = fault(),
            _ => r.receipt = fault(),
        }
        let executed = Cell::new(0);
        let actual = with_candidate_authority_v2(&r, tx(), None, |_, _, _| {
            executed.set(executed.get() + 1);
            Ok(())
        });
        assert!(actual.is_err());
        assert_eq!(executed.get(), 0);
        let calls = r.calls.lock().unwrap();
        assert_eq!(calls.last(), Some(&failed));
        assert!(!calls.contains(&"body") && !calls.contains(&"snapshot"));
    }
}
#[test]
fn transaction_authority_candidate_none_pending_priority_and_accepted_flag_remain_exact() {
    let mut unknown = Reader::new();
    unknown.raw = Ok(None);
    assert!(
        with_candidate_authority_v2(&unknown, tx(), None, |_, _, _| -> EthResult<()> { panic!("unknown must not reach execution") })
            .unwrap()
            .is_none()
    );
    assert_eq!(*unknown.calls.lock().unwrap(), ["raw"]);
    for accepted in [true, false] {
        let mut r = Reader::new();
        if !accepted {
            r.receipt = Ok(None);
        }
        let actual = with_candidate_authority_v2(&r, tx(), None, |raw, skip, flag| Ok((raw, skip, flag))).unwrap().unwrap();
        assert_eq!(actual, (vec![9, 8, 7], Some(2), accepted));
    }
    let mut pending = Reader::new();
    pending.raw = fault();
    let actual =
        with_candidate_authority_v2(&pending, tx(), Some(vec![6, 5]), |raw, skip, flag| Ok((raw, skip, flag))).unwrap().unwrap();
    assert_eq!(actual, (vec![6, 5], Some(2), true));
    assert_eq!(*pending.calls.lock().unwrap(), ["locations", "receipt"]);
}
#[test]
fn transaction_authority_status_errors_do_not_select_unknown_or_pending() {
    for fail_locations in [true, false] {
        let mut r = Reader::new();
        if fail_locations {
            r.locations = fault();
        } else {
            r.receipt = fault();
        }
        let selected = Cell::new(0);
        assert!(
            with_status_authority_v2(&r, tx(), |_, _| {
                selected.set(selected.get() + 1);
                Ok(())
            })
            .is_err()
        );
        assert_eq!(selected.get(), 0);
    }
    let mut absent = Reader::new();
    absent.locations = Ok(Default::default());
    absent.receipt = Ok(None);
    with_status_authority_v2(&absent, tx(), |locations, receipt| {
        assert_eq!(locations, EvmTxLocations::default());
        assert!(receipt.is_none());
        Ok(())
    })
    .unwrap();
    let positive = Reader::new();
    with_status_authority_v2(&positive, tx(), |locations, view| {
        assert_eq!(locations.last_skip_class, Some(2));
        assert_eq!(view, Some(receipt()));
        Ok(())
    })
    .unwrap();
}
#[test]
fn transaction_authority_lookup_errors_stop_pending_fallback_and_true_none_keeps_it() {
    for fail_raw in [true, false] {
        let mut r = Reader::new();
        if fail_raw {
            r.raw = fault();
        } else {
            r.receipt = fault();
        }
        let fallback = Cell::new(0);
        assert!(
            with_transaction_authority_v2(&r, tx(), |_, _| {
                fallback.set(fallback.get() + 1);
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fallback.get(), 0);
    }
    let mut absent = Reader::new();
    absent.raw = Ok(None);
    absent.receipt = Ok(None);
    with_transaction_authority_v2(&absent, tx(), |raw, view| {
        assert!(raw.is_none() && view.is_none());
        Ok(())
    })
    .unwrap();
    let positive = Reader::new();
    with_transaction_authority_v2(&positive, tx(), |raw, view| {
        assert_eq!(raw, Some(vec![9, 8, 7]));
        assert_eq!(view, Some(receipt()));
        Ok(())
    })
    .unwrap();
}
#[test]
fn transaction_authority_replay_faults_never_select_genesis_or_execute() {
    for failed in ["receipt", "body", "missingbody", "header", "snapshot", "missingsnapshot"] {
        let mut r = Reader::new();
        match failed {
            "receipt" => r.receipt = fault(),
            "body" => r.body = fault(),
            "missingbody" => r.body = Ok(None),
            "header" => r.header = fault(),
            "snapshot" => r.snapshot = fault(),
            _ => r.snapshot = Ok(None),
        }
        let executed = Cell::new(0);
        let actual = with_replay_authority_v2(&r, tx(), |_, _, _, _| {
            executed.set(executed.get() + 1);
            Ok(())
        });
        assert!(actual.is_err(), "failed phase {failed}");
        assert_eq!(executed.get(), 0);
        if failed == "header" {
            assert!(!r.calls.lock().unwrap().contains(&"snapshot"));
        }
    }
}
#[test]
fn transaction_authority_replay_true_absence_genesis_and_positive_prestate_contracts() {
    let mut absent = Reader::new();
    absent.receipt = Ok(None);
    let actual =
        with_replay_authority_v2(&absent, tx(), |_, _, _, _| -> EthResult<()> { panic!("no receipt means no replay") }).unwrap();
    assert!(actual.is_none());
    assert_eq!(*absent.calls.lock().unwrap(), ["receipt"]);
    let mut genesis = Reader::new();
    genesis.header = Ok(None);
    genesis.snapshot = fault();
    with_replay_authority_v2(&genesis, tx(), |view, replay, header, snapshot| {
        assert_eq!(view, receipt());
        assert_eq!(replay, body());
        assert!(header.is_none());
        assert_eq!(snapshot, EvmStateSnapshot::default());
        Ok(())
    })
    .unwrap()
    .unwrap();
    assert_eq!(*genesis.calls.lock().unwrap(), ["receipt", "body", "header"]);
    let positive = Reader::new();
    with_replay_authority_v2(&positive, tx(), |view, replay, header, snapshot| {
        assert_eq!(view, receipt());
        assert_eq!(replay, body());
        assert_eq!(header, Some(EvmExecutionHeader::default()));
        assert_eq!(snapshot, EvmStateSnapshot::default());
        Ok(())
    })
    .unwrap()
    .unwrap();
    assert_eq!(*positive.calls.lock().unwrap(), ["receipt", "body", "header", "snapshot"]);
}
