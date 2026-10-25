//! Exact parent market context for actual production templates and validation.
//! All market gates retain their existing rules; no model approval is supplied.
use super::*;
use kaspa_consensus_core::{
    evm::view_owned_v2::NativeEvmViewOwnedV2,
    palw_native_resource_v1::{NativeChainStateV1, NativeCoverageV1, NativeRetainedV1},
    palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1 as Context,
};
use kaspa_database::prelude::StoreError;

pub(crate) enum EvmChainContextFaultV2 {
    Invalid(String),
    Local(PalwLocalTransitionFaultV1),
}
impl From<String> for EvmChainContextFaultV2 {
    fn from(why: String) -> Self {
        Self::Invalid(why)
    }
}
impl From<PalwLocalTransitionFaultV1> for EvmChainContextFaultV2 {
    fn from(why: PalwLocalTransitionFaultV1) -> Self {
        Self::Local(why)
    }
}
pub(crate) struct EvmNativeMarketContextV2 {
    pub view: NativeEvmViewOwnedV2,
    pub settlements: NativeRetainedV1<Vec<kaspa_consensus_core::evm::model_market::PalwEvmSettlementV1>>,
}
pub(super) fn local_store_v2(error: StoreError) -> PalwLocalTransitionFaultV1 {
    match error {
        StoreError::LocalResourceUnavailable(why) => {
            NativeCoverageV1::unsupported::<NativeChainStateV1>("EVM market parent", &why).into()
        }
        StoreError::KeyNotFound(_) => PalwLocalTransitionFaultV1::Context(Context::HistoryUnavailable),
        _ => PalwLocalTransitionFaultV1::Context(Context::StoreUnreadable),
    }
}
pub(super) fn execution_fault_v2(error: crate::processes::evm::EvmValidateError) -> EvmChainContextFaultV2 {
    use crate::processes::evm::EvmValidateError as E;
    match error {
        E::CommitmentMismatch { .. } => {
            EvmChainContextFaultV2::Invalid("evm_commitment_root mismatch (mergeset acceptance re-execution)".into())
        }
        E::Exec(why) => EvmChainContextFaultV2::Invalid(format!("evm execution: {why}")),
        E::Store(why) => local_store_v2(why).into(),
        E::Resource(why) => EvmChainContextFaultV2::Local(why.into()),
        E::Context(why) => EvmChainContextFaultV2::Local(PalwLocalTransitionFaultV1::Context(why)),
    }
}
impl VirtualStateProcessor {
    pub(in crate::pipeline::virtual_processor) fn evm_template_parent_owned_v2(
        &self,
        parent: BlockHash,
        child_daa: u64,
    ) -> Result<Option<NativeChainStateV1>, PalwLocalTransitionFaultV1> {
        // Market projection and execution-round gas are independent consumers
        // of the same exact parent. A dormant market must not erase round gas.
        if !self.palw_evm_market_fences_at(child_daa).evm_active && self.palw_execution_lane_at(child_daa).is_none() {
            return Ok(None);
        }
        let params = self.palw_state_params_v2.as_ref().ok_or(PalwLocalTransitionFaultV1::Context(Context::HistoryUnavailable))?;
        let (stored, state) = self
            .palw_state_v2_store
            .read()
            .load_tip_retained_v1(params)
            .map_err(local_store_v2)?
            .ok_or(PalwLocalTransitionFaultV1::Context(Context::HistoryUnavailable))?;
        if stored != parent {
            return Err(PalwLocalTransitionFaultV1::Context(Context::HistoryUnavailable));
        }
        self.evm_check_parent_point_v2(&state, parent)?;
        Ok(Some(state))
    }
    fn evm_check_parent_point_v2(&self, state: &NativeChainStateV1, parent: BlockHash) -> Result<(), PalwLocalTransitionFaultV1> {
        if state.last_point().map(|p| p.block).map_or(parent != self.genesis.hash, |block| block != parent) {
            return Err(PalwLocalTransitionFaultV1::Context(Context::HistoryUnavailable));
        }
        Ok(())
    }
    pub(in crate::pipeline::virtual_processor) fn evm_market_context_owned_v2(
        &self,
        state: Option<&NativeChainStateV1>,
        parent: BlockHash,
        child_daa: u64,
    ) -> Result<Option<EvmNativeMarketContextV2>, PalwLocalTransitionFaultV1> {
        if !self.palw_evm_market_fences_at(child_daa).evm_active {
            return Ok(None);
        }
        let state = state.ok_or(PalwLocalTransitionFaultV1::Context(Context::HistoryUnavailable))?;
        let params = self.palw_state_params_v2.as_ref().ok_or(PalwLocalTransitionFaultV1::Context(Context::HistoryUnavailable))?;
        self.evm_check_parent_point_v2(state, parent)?;
        let gate = kaspa_consensus_core::palw_state_v2::NativeEvmMarketGateInputsV2 {
            audit_active: self.palw_audit_2026_09_23_at(child_daa),
            virtual_active: self.palw_model_virtual_v1_from_at(child_daa).is_some(),
            registry_present: self.palw_model_registry_at(child_daa) && self.palw_execution_lane_at(child_daa).is_some(),
            work_target_active: self.palw_work_target_at(child_daa),
        };
        let view = state.evm_view_owned_v2(kaspa_consensus_core::evm::EVM_CHAIN_ID, params.base_class_id(), gate)?;
        let settlements = state.evm_replay_settlements_owned_v2()?;
        Ok(Some(EvmNativeMarketContextV2 { view, settlements }))
    }
}
