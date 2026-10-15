//! Fresh transition construction owner. Every admitted growth request must be
//! derived at its concrete writer before allocation. This module does not turn
//! a post-construction measurement into permission to allocate.
use super::retained::value_requests_v1;
use super::*;
use crate::palw_state_v2::{PalwChainStateV2, PalwStateDeltaV2};

pub(crate) struct NativeTransitionReservationV1 {
    lease: Option<NativeDemandLeaseV1>,
    requests: u64,
}
impl NativeTransitionReservationV1 {
    #[cfg(test)]
    pub(crate) fn for_checker_v1(checker: &std::sync::Arc<NativeDemandCheckerV1>) -> Self {
        Self { lease: Some(checker.reserve_requests_v1(0, 0).unwrap()), requests: 0 }
    }
    pub(crate) fn is_tracking_v1(&self) -> bool {
        self.lease.is_some() || native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1()
    }
    pub(crate) fn copy_parent_v1(parent: &PalwChainStateV2) -> Result<(PalwChainStateV2, Self), NativeCoverageV1> {
        let requests = value_requests_v1(parent)?;
        let owner = NativeRetainedV1::try_from_clone_v1(parent)?;
        let (value, lease) = owner.into_parts_v1();
        let mut construction = Self { lease, requests };
        // EXEC's native approval view uses one exact-size 64/65-byte lookup
        // Vec at a time. Keep its maximum scratch live for this transition,
        // before any admission/tick/Final-deadline read can allocate that key.
        // Fixed prefix decoding and raw enrollment hashing allocate no body.
        construction.grow_v1(crate::palw_exec_v2_verify::PALW_EXEC_APPROVAL_KEY_BYTES_V1 as u64)?;
        Ok((value, construction))
    }
    pub(crate) fn grow_v1(&mut self, additional: u64) -> Result<(), NativeCoverageV1> {
        if self.lease.is_none() && !native_logical_admission_enabled_v1() && !native_resource_admission_enabled_v1() {
            return Ok(());
        }
        let requests = self
            .requests
            .checked_add(additional)
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>("fresh transition construction"))?;
        if let Some(lease) = &mut self.lease {
            lease.grow_memory_v1(additional)?;
        } else {
            self.lease = reserve_native_requests_v1(additional)?;
        }
        self.requests = requests;
        Ok(())
    }
    fn take_value_v1<T: NativeHeapV1>(&mut self, value: T) -> Result<NativeRetainedV1<T>, NativeCoverageV1> {
        let requests = value_requests_v1(&value)?;
        self.split_owner_v1(value, requests)
    }
    fn split_owner_v1<T>(&mut self, value: T, requests: u64) -> Result<NativeRetainedV1<T>, NativeCoverageV1> {
        if requests > self.requests {
            return Err(NativeCoverageV1::unsupported::<T>(
                "fresh transition output",
                "an actual output exceeds the writer's pre-construction reservations; uncovered growth cannot be approved afterwards",
            ));
        }
        let lease = self.lease.as_mut().map(|l| l.split_memory_v1(requests)).transpose()?;
        self.requests -= requests;
        Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
    }
    pub(crate) fn finish_state_v1(mut self, state: PalwChainStateV2) -> Result<NativeChainStateV1, NativeCoverageV1> {
        self.take_value_v1(state)
    }
    pub(crate) fn finish_transition_v1(
        mut self,
        state: PalwChainStateV2,
        delta: PalwStateDeltaV2,
        skips: Vec<(crate::BlockHash, String)>,
    ) -> Result<(NativeChainStateV1, NativeStateDeltaV1, NativeRetainedV1<Vec<(crate::BlockHash, String)>>), NativeCoverageV1> {
        // Split while the aggregate ticket still exists. A subsequent output
        // refusal drops every split owner; no state has been published yet.
        let state = self.take_value_v1(state)?;
        let delta = self.take_value_v1(delta)?;
        let skips = self.take_value_v1(skips)?;
        Ok((state, delta, skips))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transition_output_split_keeps_each_payloads_ownership_and_never_reserves_afterwards() {
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 64, database_bytes: 0 });
        let mut construction =
            NativeTransitionReservationV1 { lease: Some(checker.reserve_requests_v1(64, 0).unwrap()), requests: 64 };
        assert!(construction.split_owner_v1(0u8, 65).is_err());
        assert_eq!(checker.outstanding(), (64, 0));
        let state = construction.split_owner_v1(1u8, 32).unwrap();
        let delta = construction.split_owner_v1(2u8, 16).unwrap();
        drop(construction);
        assert_eq!(checker.outstanding(), (48, 0));
        drop(delta);
        assert_eq!(checker.outstanding(), (32, 0));
        drop(state);
        assert_eq!(checker.outstanding(), (0, 0));
    }
    #[test]
    fn construction_growth_refusal_leaves_its_existing_ticket_unchanged() {
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 32, database_bytes: 0 });
        let mut construction =
            NativeTransitionReservationV1 { lease: Some(checker.reserve_requests_v1(16, 0).unwrap()), requests: 16 };
        assert!(construction.grow_v1(17).is_err());
        assert_eq!(construction.requests, 16);
        assert_eq!(checker.outstanding(), (16, 0));
        construction.grow_v1(16).unwrap();
        assert_eq!(construction.requests, 32);
        assert_eq!(checker.outstanding(), (32, 0));
    }
}
