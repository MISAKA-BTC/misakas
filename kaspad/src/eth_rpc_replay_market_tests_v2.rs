//! Actual replay continuation helper, separate from native/executor controls.
use super::*;
use kaspa_consensus_core::{
    api::ConsensusApi,
    evm::{EvmTraceReplayBodyV1, view_owned_v2::*},
    palw_native_resource_v1::*,
    palw_state_v2::PalwChainStateV2,
};
use std::{cell::Cell, sync::Mutex};
struct Reader(Mutex<Option<Result<Option<NativeEvmReplayMarketV2>, NativeEvmViewFaultV2>>>);
impl ConsensusApi for Reader {
    fn palw_evm_replay_market_owned_v2(
        &self,
        _: kaspa_consensus_core::BlockHash,
    ) -> Result<Option<NativeEvmReplayMarketV2>, NativeEvmViewFaultV2> {
        self.0.lock().unwrap().take().unwrap()
    }
}
fn body() -> EvmTraceReplayBodyV1 {
    let mut body = EvmTraceReplayBodyV1 {
        selected_parent: kaspa_hashes::Hash64::from_u64_word(7),
        env: Default::default(),
        system_ops: vec![],
        txs: vec![],
    };
    body.env.daa_score = 11;
    body
}
fn accepting() -> kaspa_consensus_core::BlockHash {
    kaspa_hashes::Hash64::from_u64_word(8)
}
fn context() -> NativeEvmReplayMarketV2 {
    let state = NativeRetainedV1::retain_existing_v1(PalwChainStateV2::genesis()).unwrap();
    NativeEvmReplayMarketV2 {
        accepting: accepting(),
        selected_parent: body().selected_parent,
        accepting_daa: body().env.daa_score,
        view: state.evm_view_owned_v2(EVM_CHAIN_ID, Default::default(), Default::default()).unwrap(),
        fences: kaspa_consensus_core::evm::model_market::PalwEvmMarketFencesV1 { evm_active: true, ..Default::default() },
        settlements: state.evm_replay_settlements_owned_v2().unwrap(),
    }
}
#[test]
fn replay_market_rpc_read_faults_and_identity_mismatch_stop_before_trace() {
    for fault in [
        NativeEvmViewFaultV2::HistoryUnavailable,
        NativeEvmViewFaultV2::Store("bad row".into()),
        NativeEvmViewFaultV2::Context("generation".into()),
        NativeEvmViewFaultV2::Resource(NativeCoverageV1::unsupported::<PalwChainStateV2>("test", "quota")),
    ] {
        let r = Reader(Mutex::new(Some(Err(fault))));
        let visits = Cell::new(0);
        assert!(
            with_replay_market_owned_v2(&r, accepting(), &body(), |_| {
                visits.set(visits.get() + 1);
                Ok(())
            })
            .is_err()
        );
        assert_eq!(visits.get(), 0);
    }
    for field in 0..3 {
        let mut c = context();
        match field {
            0 => c.accepting = Default::default(),
            1 => c.selected_parent = Default::default(),
            _ => c.accepting_daa += 1,
        };
        let r = Reader(Mutex::new(Some(Ok(Some(c)))));
        assert!(
            with_replay_market_owned_v2(&r, accepting(), &body(), |_| -> EthResult<()> {
                panic!("no trace after mismatched identity")
            })
            .is_err()
        );
    }
}
#[test]
fn replay_market_rpc_exact_owner_and_true_inactive_are_preserved() {
    let c = context();
    let view = c.view.clone();
    let weak = Arc::downgrade(&view);
    let r = Reader(Mutex::new(Some(Ok(Some(c)))));
    with_replay_market_owned_v2(&r, accepting(), &body(), |actual| {
        let actual = actual.unwrap();
        assert!(Arc::ptr_eq(&actual.view, &view));
        assert!(weak.upgrade().is_some());
        Ok(())
    })
    .unwrap();
    drop(view);
    assert!(weak.upgrade().is_none());
    let r = Reader(Mutex::new(Some(Ok(None))));
    with_replay_market_owned_v2(&r, accepting(), &body(), |actual| {
        assert!(actual.is_none());
        Ok(())
    })
    .unwrap();
}
