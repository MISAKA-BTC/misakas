//! Execution replay uses post-parent rows and accepting-block fences together.
use super::*;
use kaspa_consensus_core::evm::view_owned_v2::{NativeEvmReplayMarketV2, NativeEvmViewFaultV2 as E};
fn read_fault(error: kaspa_database::prelude::StoreError) -> E {
    use kaspa_database::prelude::StoreError;
    match error {
        StoreError::KeyNotFound(_) => E::HistoryUnavailable,
        StoreError::LocalResourceUnavailable(reason) => {
            E::Resource(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<NativeEvmReplayMarketV2>(
                "replay market history",
                &reason,
            ))
        }
        other => E::Store(other.to_string()),
    }
}
impl Consensus {
    pub(super) fn read_evm_replay_market_owned_v2(&self, accepting: BlockHash) -> Result<Option<NativeEvmReplayMarketV2>, E> {
        let params = &self.config.params;
        let bundle = match &params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle,
            _ => return Ok(None),
        };
        let _session = self.pruning_lock.blocking_read();
        let generation = self.virtual_processor.evm_asof_import_status_v2();
        let header = self.headers_store.get_header(accepting).map_err(read_fault)?;
        let fences = params.palw_evm_market_fences_at(header.daa_score);
        if !fences.evm_active {
            return Ok(None);
        }
        let (generation, stable) = generation.map_err(crate::pipeline::virtual_processor::NativeEvmAsOfFaultV2::into_public_v2)?;
        if !stable || generation.active.is_some() {
            return Err(E::Context("import incomplete".into()));
        }
        let selected_parent = self.ghostdag_store.get_data(accepting).map_err(read_fault)?.selected_parent;
        let parent_header = self.headers_store.get_header(selected_parent).map_err(read_fault)?;
        let state = self
            .virtual_processor
            .palw_evm_post_state_owned_v2(selected_parent)
            .map_err(crate::pipeline::virtual_processor::NativeEvmAsOfFaultV2::into_public_v2)?
            .ok_or(E::HistoryUnavailable)?;
        if state.last_point().is_some_and(|p| p.block != selected_parent || p.daa_score != parent_header.daa_score) {
            return Err(E::Context("replay parent state/header point differs".into()));
        }
        let gate = kaspa_consensus_core::palw_state_v2::NativeEvmMarketGateInputsV2::at_params_v2(params, header.daa_score);
        let view = state
            .evm_view_owned_v2(kaspa_consensus_core::evm::EVM_CHAIN_ID, bundle.state.base_class_id(), gate)
            .map_err(E::Resource)?;
        let settlements = state.evm_replay_settlements_owned_v2().map_err(E::Resource)?;
        #[cfg(test)]
        evm_view_projection_hook_v2::invoke(self);
        let (after, stable) = self
            .virtual_processor
            .evm_asof_import_status_v2()
            .map_err(crate::pipeline::virtual_processor::NativeEvmAsOfFaultV2::into_public_v2)?;
        if !stable || after.active.is_some() || after.epoch != generation.epoch {
            return Err(E::Context("import generation changed during replay market projection".into()));
        }
        Ok(Some(NativeEvmReplayMarketV2 { accepting, selected_parent, accepting_daa: header.daa_score, view, fences, settlements }))
    }
}
