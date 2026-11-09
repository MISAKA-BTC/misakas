//! Fresh transition construction owner. Every admitted growth request must be
//! derived at its concrete writer before allocation. This module does not turn
//! a post-construction measurement into permission to allocate.
use super::retained::{tracked_value_requests_v1, value_requests_v1};
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
    /// Reserve the typed carriage copy before stream-counting canonical bytes,
    /// then admit root workspace before root hashing allocates its buffers.
    pub(crate) fn state_root_v2(&mut self, state: &PalwChainStateV2) -> Result<crate::Hash64, NativeCoverageV1> {
        let copy_bound = state.native_heap_v1("executed-prefix carriage copy").add::<PalwChainStateV2>(
            NativeCoverageV1::Bounded { bytes: size_of::<crate::palw_state_v2::PalwStateCarriageV2>() as u64 },
            "executed-prefix carriage owner",
        );
        let _copy_lease = reserve_native_requests_v1(copy_bound.bytes().ok_or(copy_bound)?)?;
        let carriage = crate::palw_state_v2::PalwStateCarriageV2::from_state(state);
        let canonical = borsh::object_length(&carriage)
            .map_err(|e| NativeCoverageV1::unsupported::<PalwChainStateV2>("native executed-prefix root quote", &e.to_string()))?
            as u64;
        let requests = super::restore_workspace::native_restore_root_workspace_requests_v1(canonical)?;
        self.grow_v1(requests)?;
        Ok(state.state_root())
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
    /// Admit the fixed output frames before the fold builds their contents.
    pub(crate) fn reserve_output_owners_v1(
        &mut self,
        point: &crate::palw_state_v2::PalwBlockContextV2,
    ) -> Result<(), NativeCoverageV1> {
        if !self.is_tracking_v1() {
            return Ok(());
        }
        let delta = value_requests_v1(&PalwStateDeltaV2 { point: *point, entries: Vec::new() })?;
        let skips = value_requests_v1(&Vec::<(crate::BlockHash, String)>::new())?;
        let requests = delta.checked_add(skips).ok_or_else(|| NativeCoverageV1::overflow::<Self>("fresh output owners"))?;
        self.grow_v1(requests)
    }
    fn take_value_v1<T: NativeHeapV1>(&mut self, value: T) -> Result<NativeRetainedV1<T>, NativeCoverageV1> {
        let requests = if self.is_tracking_v1() { tracked_value_requests_v1(&value)? } else { 0 };
        self.split_owner_v1(value, requests, "fresh transition output")
    }
    fn split_owner_v1<T>(&mut self, value: T, requests: u64, component: &str) -> Result<NativeRetainedV1<T>, NativeCoverageV1> {
        if requests > self.requests {
            return Err(NativeCoverageV1::unsupported::<T>(
                component,
                &format!("output requires {requests} bytes; writer has {} bytes of pre-construction reservation", self.requests),
            ));
        }
        let lease = self.lease.as_mut().map(|l| l.split_memory_v1(requests)).transpose()?;
        self.requests -= requests;
        Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
    }
    pub(crate) fn finish_state_v1(mut self, state: PalwChainStateV2, component: &str) -> Result<NativeChainStateV1, NativeCoverageV1> {
        let requests = if self.is_tracking_v1() { tracked_value_requests_v1(&state)? } else { 0 };
        self.split_owner_v1(state, requests, component)
    }
    pub(crate) fn finish_transition_with_assignment_materials_v2(
        mut self,
        state: PalwChainStateV2,
        delta: PalwStateDeltaV2,
        skips: Vec<(crate::BlockHash, String)>,
        materials: crate::palw_state_v2::PositiveAssignmentMaterialOutcomeV2,
    ) -> Result<
        (
            NativeChainStateV1,
            NativeStateDeltaV1,
            NativeRetainedV1<Vec<(crate::BlockHash, String)>>,
            NativeRetainedV1<crate::palw_state_v2::PositiveAssignmentMaterialOutcomeV2>,
        ),
        NativeCoverageV1,
    > {
        if materials.is_empty() {
            // Empty Assignment path preserves the exact old output split and
            // carries no allocated record buffer into a fictitious new owner.
            let (state, delta, skips) = self.finish_transition_v1(state, delta, skips)?;
            return Ok((state, delta, skips, crate::palw_state_v2::PositiveAssignmentMaterialOutcomeV2::empty_retained_v2()));
        }
        // Every split occurs while the aggregate construction ticket is alive;
        // no output is approved by a reservation after buffers were allocated.
        let state = self.take_value_v1(state)?;
        let delta = self.take_value_v1(delta)?;
        let skips = self.take_value_v1(skips)?;
        let materials = self.take_value_v1(materials)?;
        Ok((state, delta, skips, materials))
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
        let failure = construction.split_owner_v1(0u8, 65, "test output").err().unwrap();
        assert!(matches!(
            failure,
            NativeCoverageV1::Unsupported { component, reason, .. }
                if component == "test output" && reason.contains("65 bytes") && reason.contains("64 bytes")
        ));
        assert_eq!(checker.outstanding(), (64, 0));
        let state = construction.split_owner_v1(1u8, 32, "state").unwrap();
        let delta = construction.split_owner_v1(2u8, 16, "delta").unwrap();
        drop(construction);
        assert_eq!(checker.outstanding(), (48, 0));
        drop(delta);
        assert_eq!(checker.outstanding(), (32, 0));
        drop(state);
        assert_eq!(checker.outstanding(), (0, 0));
    }
    #[test]
    fn default_raw_transition_keeps_zero_quotes_but_explicit_owner_requires_coverage() {
        if native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1() {
            return;
        }
        let state = PalwChainStateV2::genesis();
        assert_eq!(value_requests_v1(&state).unwrap(), 0);
        assert!(tracked_value_requests_v1(&state).unwrap() > 0);
        let (copy, construction) = NativeTransitionReservationV1::copy_parent_v1(&state).unwrap();
        assert!(!construction.is_tracking_v1());
        let finished = construction.finish_state_v1(copy, "untracked test state").unwrap();
        assert_eq!(finished.state_root(), state.state_root());
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 1 << 20, database_bytes: 0 });
        let tracked = NativeTransitionReservationV1::for_checker_v1(&checker);
        assert!(
            tracked.finish_state_v1(state, "tracked test state").is_err(),
            "a zero-sized explicit ticket cannot approve an unreserved output"
        );
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
