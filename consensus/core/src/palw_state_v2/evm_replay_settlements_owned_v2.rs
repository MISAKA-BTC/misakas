//! Exact child-settlement copy with its owner admitted before allocation.
use super::*;
use crate::evm::model_market::PalwEvmSettlementV1;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeRetainedV1, reserve_native_requests_v1};
impl NativeRetainedV1<PalwChainStateV2> {
    pub fn evm_replay_settlements_owned_v2(&self) -> Result<NativeRetainedV1<Vec<PalwEvmSettlementV1>>, NativeCoverageV1> {
        let count = self.evm_settlements.len();
        let bytes = (count as u64)
            .checked_mul(std::mem::size_of::<PalwEvmSettlementV1>() as u64)
            .ok_or_else(|| NativeCoverageV1::overflow::<Vec<PalwEvmSettlementV1>>("replay market settlements"))?;
        let lease = reserve_native_requests_v1(bytes)?;
        let mut values = Vec::new();
        values.try_reserve_exact(count).map_err(|_| {
            NativeCoverageV1::unsupported::<Vec<PalwEvmSettlementV1>>("replay market settlements", "allocation refused")
        })?;
        if values.capacity() != count {
            return Err(NativeCoverageV1::unsupported::<Vec<PalwEvmSettlementV1>>(
                "replay market settlements",
                "unexpected exact-reserve capacity",
            ));
        }
        values.extend(self.evm_settlements.values().copied());
        Ok(NativeRetainedV1::from_owner_reserved_v1(values, lease))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evm::model_market::PalwEvmSettlementOutcomeV1;
    use crate::palw_native_resource_v1::*;
    #[test]
    fn replay_market_settlement_copy_actual_order_quota_and_lifetime() {
        const CHILD: &str = "MISAKA_REPLAY_SETTLEMENT_COPY_QUOTA";
        if std::env::var_os(CHILD).is_none() {
            let full = format!("{}::replay_market_settlement_copy_actual_order_quota_and_lifetime", module_path!());
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
                    .env(CHILD, "1")
                    .status()
                    .unwrap()
                    .success()
            );
            return;
        }
        let mut state = PalwChainStateV2::genesis();
        for seq in [5, 1, 3] {
            state.evm_settlements.insert(
                seq,
                PalwEvmSettlementV1 {
                    seq,
                    account: Default::default(),
                    line_id: Hash64::from_u64_word(seq as u64 + 1),
                    action: 2,
                    escrow_sompi: 0,
                    outcome: PalwEvmSettlementOutcomeV1::Refused { reason: 1 },
                },
            );
        }
        let raw = state.evm_settlements();
        let source = NativeRetainedV1::retain_existing_v1(state).unwrap();
        let bytes = (raw.len() * std::mem::size_of::<PalwEvmSettlementV1>()) as u64;
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: bytes, database_bytes: 0 }));
        let blocker = reserve_native_requests_v1(1).unwrap();
        assert!(source.evm_replay_settlements_owned_v2().is_err());
        drop(blocker);
        let output = source.evm_replay_settlements_owned_v2().unwrap();
        assert_eq!(*output, raw);
        assert_eq!(output.iter().map(|s| s.seq).collect::<Vec<_>>(), [1, 3, 5]);
        assert!(reserve_native_requests_v1(1).is_err());
        drop(source);
        assert!(reserve_native_requests_v1(1).is_err());
        drop(output);
        assert!(reserve_native_requests_v1(bytes).is_ok());
    }
}
