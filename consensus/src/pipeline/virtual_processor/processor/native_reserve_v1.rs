//! Committed reserve reads shared by construction, validation and snapshots.
use super::*;

impl VirtualStateProcessor {
    /// Only the actual absent row encodes zero in this live committed-parent
    /// store. Decode/I/O/capacity faults cannot change the economic recurrence.
    pub(in crate::pipeline::virtual_processor) fn dns_reserve_balance_for_native_v1(
        &self,
        block: BlockHash,
    ) -> BlockProcessResult<u64> {
        self.reserve_balance_store.get(block).optional().map(|balance| balance.unwrap_or(0)).map_err(|why| match why {
            StoreError::LocalResourceUnavailable(reason) => RuleError::LocalNativeCapacityUnavailable(
                kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<u64>(
                    "committed DNS reserve read",
                    &reason,
                ),
            ),
            other => {
                warn!("Committed DNS reserve at {block} could not be read: {other}");
                RuleError::LocalNativeStateContextUnavailable(
                    kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::StoreUnreadable,
                )
            }
        })
    }
}
