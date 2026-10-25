//! Head-selection adapter controls. Artificial pointers are not native maturity facts.
use super::*;
use kaspa_consensus_core::{
    BlockHash,
    api::ConsensusApi,
    config::params::MAINNET_PARAMS,
    errors::consensus::{ConsensusError, ConsensusResult},
    evm::{CanonicalEvmHeads, EvmExecutionHeader, EvmStateSnapshot, model_market::*, view_owned_v2::*},
    header::Header,
    palw_native_resource_v1::*,
    palw_state_v2::PalwChainStateV2,
};
use std::{cell::Cell, sync::Mutex};
fn head() -> BlockHash {
    BlockHash::from_bytes([3; 64])
}
fn tip() -> BlockHash {
    BlockHash::from_bytes([4; 64])
}
struct Reader {
    heads: ConsensusResult<Option<CanonicalEvmHeads>>,
    market: Mutex<Option<Result<Option<NativeEvmViewAtV2>, NativeEvmViewFaultV2>>>,
    snapshot: Option<EvmStateSnapshot>,
    execution: Option<EvmExecutionHeader>,
    calls: Mutex<Vec<(&'static str, BlockHash)>>,
}
impl Reader {
    fn with_market(market: Result<Option<NativeEvmViewAtV2>, NativeEvmViewFaultV2>) -> Self {
        Self {
            heads: Ok(Some(CanonicalEvmHeads { latest: head(), ..Default::default() })),
            market: Mutex::new(Some(market)),
            snapshot: Some(EvmStateSnapshot::default()),
            execution: Some(EvmExecutionHeader { evm_number: 17, evm_timestamp_sec: 19, gas_limit: 30_000_000, ..Default::default() }),
            calls: Mutex::new(vec![]),
        }
    }
    fn record(&self, name: &'static str, target: BlockHash) {
        self.calls.lock().unwrap().push((name, target));
    }
}
impl ConsensusApi for Reader {
    fn get_sink(&self) -> BlockHash {
        tip()
    }
    fn get_evm_canonical_heads(&self) -> ConsensusResult<Option<CanonicalEvmHeads>> {
        self.record("heads", BlockHash::default());
        self.heads.clone()
    }
    fn get_header(&self, target: BlockHash) -> ConsensusResult<Arc<Header>> {
        self.record("daa", target);
        let mut header: Header = (&MAINNET_PARAMS.genesis).into();
        header.hash = target;
        header.daa_score = 0;
        Ok(Arc::new(header))
    }
    fn get_evm_state_snapshot_of(&self, target: BlockHash) -> ConsensusResult<Option<EvmStateSnapshot>> {
        self.record("snapshot", target);
        Ok(self.snapshot.clone())
    }
    fn get_evm_header_of(&self, target: BlockHash) -> ConsensusResult<Option<EvmExecutionHeader>> {
        self.record("execution", target);
        Ok(self.execution.clone())
    }
    fn palw_evm_view_at_owned_v2(&self, target: BlockHash) -> Result<Option<NativeEvmViewAtV2>, NativeEvmViewFaultV2> {
        self.record("view", target);
        self.market.lock().unwrap().take().expect("one exact-head view request")
    }
    fn palw_evm_view_v1(&self) -> Option<(Arc<PalwEvmViewV1>, PalwEvmMarketFencesV1)> {
        panic!("current-tip raw view must never supply a canonical executed-head environment")
    }
    fn evm_activation_fences(&self) -> (u64, u64, u64, u64) {
        (u64::MAX, u64::MAX, u64::MAX, 0)
    }
}
fn view() -> NativeEvmViewOwnedV2 {
    let state = NativeRetainedV1::retain_existing_v1(PalwChainStateV2::genesis()).unwrap();
    state.evm_view_owned_v2(EVM_CHAIN_ID, kaspa_consensus_core::Hash64::default(), Default::default()).unwrap()
}
#[test]
fn canonical_head_env_exact_head_differs_from_tip_and_real_sim_retains_owner() {
    let owned = view();
    let weak = Arc::downgrade(&owned);
    let fences = PalwEvmMarketFencesV1 { evm_active: true, ..Default::default() };
    let reader = Reader::with_market(Ok(Some((owned, fences))));
    assert_ne!(head(), reader.get_sink());
    let (snapshot, env) = canonical_head_snapshot_and_env_v2(&reader).unwrap();
    assert!(env.palw_view.is_none());
    assert!(env.palw_view_owned_v2.is_some());
    assert_eq!((env.number, env.timestamp, env.gas_limit), (17, 19, 30_000_000));
    assert!(env.f003_active && env.market_fences.evm_active);
    assert_eq!(
        *reader.calls.lock().unwrap(),
        [("heads", BlockHash::default()), ("daa", head()), ("snapshot", head()), ("execution", head()), ("view", head())]
    );
    drop(reader);
    let clone = env.clone();
    drop(env);
    assert!(weak.upgrade().is_some());
    let call = kaspa_evm::sim::EthCall {
        to: Some(MISAKA_MODEL_REGISTRY_PRECOMPILE),
        data: kaspa_evm::model_market::calldata("chainDaa()", &[]),
        ..Default::default()
    };
    let actual = kaspa_evm::sim::simulate_call(&snapshot, &clone, &call).unwrap();
    assert!(actual.success);
    assert_eq!(kaspa_evm::model_market::abi::read_u64(&actual.output, 0), Some(0));
    // A controlled newer-tip view would answer a different value. It is an
    // adapter contrast, not an accepted maturity or settlement fact.
    let wrong_tip = kaspa_evm::sim::EthCallEnv {
        palw_view_owned_v2: None,
        palw_view: Some(Arc::new(PalwEvmViewV1 { chain_daa: 99, chain_id: EVM_CHAIN_ID, ..Default::default() })),
        ..clone.clone()
    };
    let wrong = kaspa_evm::sim::simulate_call(&snapshot, &wrong_tip, &call).unwrap();
    assert_eq!(kaspa_evm::model_market::abi::read_u64(&wrong.output, 0), Some(99));
    assert_ne!(actual.output, wrong.output);
    drop(wrong_tip);
    drop(clone);
    assert!(weak.upgrade().is_none());
}
#[test]
fn canonical_head_env_typed_view_failures_stop_simulation_and_only_inactive_is_none() {
    let faults = [
        NativeEvmViewFaultV2::HistoryUnavailable,
        NativeEvmViewFaultV2::Store("cold row".into()),
        NativeEvmViewFaultV2::Context("import epoch".into()),
        NativeEvmViewFaultV2::CapacityUnavailable,
        NativeEvmViewFaultV2::Unavailable("reader unavailable"),
        NativeEvmViewFaultV2::Resource(NativeCoverageV1::unsupported::<PalwChainStateV2>("head test", "local quota")),
    ];
    for fault in faults {
        let reader = Reader::with_market(Err(fault));
        let downstream = Cell::new(0);
        let actual = canonical_head_snapshot_and_env_v2(&reader).and_then(|_| {
            downstream.set(downstream.get() + 1);
            Ok(())
        });
        assert!(actual.is_err());
        assert_eq!(downstream.get(), 0);
        assert_eq!(reader.calls.lock().unwrap().last(), Some(&("view", head())));
    }
    let reader = Reader::with_market(Ok(None));
    let (_, env) = canonical_head_snapshot_and_env_v2(&reader).unwrap();
    assert!(env.palw_view.is_none() && env.palw_view_owned_v2.is_none());
    assert!(!env.market_fences.any_active());
}
#[test]
fn canonical_head_env_missing_canonical_or_execution_data_never_reads_fallback_view() {
    for heads in [Err(ConsensusError::General("unreadable heads")), Ok(None), Ok(Some(CanonicalEvmHeads::default()))] {
        let mut reader = Reader::with_market(Ok(None));
        reader.heads = heads;
        assert!(canonical_head_snapshot_and_env_v2(&reader).is_err());
        assert_eq!(*reader.calls.lock().unwrap(), [("heads", BlockHash::default())]);
        assert!(reader.market.lock().unwrap().is_some());
    }
    for missing_snapshot in [true, false] {
        let mut reader = Reader::with_market(Ok(None));
        if missing_snapshot {
            reader.snapshot = None;
        } else {
            reader.execution = None;
        }
        assert!(canonical_head_snapshot_and_env_v2(&reader).is_err());
        assert!(reader.calls.lock().unwrap().iter().all(|(name, _)| *name != "view"));
        assert!(reader.market.lock().unwrap().is_some());
    }
}
