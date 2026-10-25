//! Dispatcher continuation controls; actual native/revm path has separate tests.
use super::*;
use kaspa_consensus_core::{api::ConsensusApi, evm::view_owned_v2::*, palw_native_resource_v1::*, palw_state_v2::PalwChainStateV2};
use std::{cell::Cell, sync::Mutex};
struct Reader {
    result: Mutex<Option<Result<Option<NativeEvmViewAtV2>, NativeEvmViewFaultV2>>>,
    targets: Mutex<Vec<kaspa_consensus_core::BlockHash>>,
}
impl ConsensusApi for Reader {
    fn palw_evm_view_at_owned_v2(
        &self,
        target: kaspa_consensus_core::BlockHash,
    ) -> Result<Option<NativeEvmViewAtV2>, NativeEvmViewFaultV2> {
        self.targets.lock().unwrap().push(target);
        self.result.lock().unwrap().take().expect("one actual reader call")
    }
}
fn target() -> kaspa_consensus_core::BlockHash {
    kaspa_consensus_core::BlockHash::from_bytes([7; 64])
}
#[test]
fn historical_market_rpc_faults_stop_env_continuation_without_latest_substitution() {
    let faults = [
        NativeEvmViewFaultV2::HistoryUnavailable,
        NativeEvmViewFaultV2::Store("malformed snapshot".into()),
        NativeEvmViewFaultV2::Context("import changed".into()),
        NativeEvmViewFaultV2::CapacityUnavailable,
        NativeEvmViewFaultV2::Resource(NativeCoverageV1::unsupported::<PalwChainStateV2>("test", "local refusal")),
    ];
    for fault in faults {
        let reader = Reader { result: Mutex::new(Some(Err(fault))), targets: Mutex::new(vec![]) };
        let downstream = Cell::new(0);
        let result = with_historical_market_view_v2(&reader, target(), |_| {
            downstream.set(downstream.get() + 1);
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(downstream.get(), 0);
        assert_eq!(*reader.targets.lock().unwrap(), [target()]);
    }
    struct Unavailable;
    impl ConsensusApi for Unavailable {}
    assert!(matches!(Unavailable.palw_evm_view_at_owned_v2(target()), Err(NativeEvmViewFaultV2::Unavailable(_))));
}
#[test]
fn historical_market_rpc_exact_target_owner_and_true_inactive_continue() {
    let source = NativeRetainedV1::retain_existing_v1(PalwChainStateV2::genesis()).unwrap();
    let view = source.evm_view_owned_v2(EVM_CHAIN_ID, kaspa_consensus_core::Hash64::default(), Default::default()).unwrap();
    let fences = kaspa_consensus_core::evm::model_market::PalwEvmMarketFencesV1 { evm_active: true, ..Default::default() };
    let reader = Reader { result: Mutex::new(Some(Ok(Some((view.clone(), fences))))), targets: Mutex::new(vec![]) };
    with_historical_market_view_v2(&reader, target(), |actual| {
        let (actual, actual_fences) = actual.unwrap();
        assert!(Arc::ptr_eq(&actual, &view));
        assert!(actual_fences.evm_active);
        Ok(())
    })
    .unwrap();
    assert_eq!(*reader.targets.lock().unwrap(), [target()]);
    let inactive = Reader { result: Mutex::new(Some(Ok(None))), targets: Mutex::new(vec![]) };
    with_historical_market_view_v2(&inactive, target(), |actual| {
        assert!(actual.is_none());
        Ok(())
    })
    .unwrap();
}
