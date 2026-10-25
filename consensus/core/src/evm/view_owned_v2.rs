//! Local historical EVM read failures; none of these authenticate computation.
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeRetainedV1};
use std::sync::Arc;
#[path = "view_native_heap_v2.rs"]
mod native_heap;

pub type NativeEvmViewOwnedV2 = Arc<NativeRetainedV1<super::model_market::PalwEvmViewV1>>;
pub type NativeEvmViewAtV2 = (NativeEvmViewOwnedV2, super::model_market::PalwEvmMarketFencesV1);
/// Non-wire replay inputs: post-parent rows interpreted at the accepting block's
/// fences, with the exact settlements the parent's fold scheduled for its child.
#[derive(Debug)]
pub struct NativeEvmReplayMarketV2 {
    pub accepting: crate::Hash64,
    pub selected_parent: crate::Hash64,
    pub accepting_daa: u64,
    pub view: NativeEvmViewOwnedV2,
    pub fences: super::model_market::PalwEvmMarketFencesV1,
    pub settlements: NativeRetainedV1<Vec<super::model_market::PalwEvmSettlementV1>>,
}
#[derive(Debug, thiserror::Error)]
pub enum NativeEvmViewFaultV2 {
    #[error("historical EVM local capacity: {0}")]
    Resource(NativeCoverageV1),
    #[error("historical EVM capacity unavailable without numeric quote")]
    CapacityUnavailable,
    #[error("historical EVM native history unavailable")]
    HistoryUnavailable,
    #[error("historical EVM store read: {0}")]
    Store(String),
    #[error("historical EVM native context: {0}")]
    Context(String),
    #[error("historical EVM reader unavailable: {0}")]
    Unavailable(&'static str),
}
impl crate::palw_state_v2::NativeEvmMarketGateInputsV2 {
    /// Same flags as the transition resolver, without copying registry works.
    pub fn at_params_v2(params: &crate::config::params::Params, daa: u64) -> Self {
        Self {
            audit_active: params.palw_audit_2026_09_23_active_at(daa),
            virtual_active: params.palw_model_virtual_v1_from_at(daa).is_some(),
            registry_present: params.palw_model_registry_at(daa) && params.palw_execution_lane_at(daa).is_some(),
            work_target_active: params.palw_work_target_at(daa),
        }
    }
}
