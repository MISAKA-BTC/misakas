//! Ownership of RFC0040 transaction-local metadata through rehearsal and commit.
use super::PalwPrefixDependencyEnvelopeV1;
use crate::Hash64;
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDemandLeaseV1, NativeRetainedV1, native_value_requests_v1, reserve_native_requests_v1,
};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct PalwPrefixDependenciesV1 {
    rows: BTreeMap<Hash64, (NativeRetainedV1<PalwPrefixDependencyEnvelopeV1>, Option<NativeDemandLeaseV1>)>,
}
pub struct PalwPrefixExtrasV1 {
    extras: crate::palw_fp_answer_accounting_v1::PalwFpAnswerExtrasV1,
    _prefix_lease: Option<NativeDemandLeaseV1>,
}
impl std::ops::Deref for PalwPrefixExtrasV1 {
    type Target = crate::palw_state_v2::PalwTransitionExtrasV1;
    fn deref(&self) -> &Self::Target {
        &self.extras
    }
}
impl PalwPrefixDependenciesV1 {
    pub fn insert_v1(
        &mut self,
        claim: Hash64,
        value: NativeRetainedV1<PalwPrefixDependencyEnvelopeV1>,
    ) -> Result<bool, NativeCoverageV1> {
        if let Some((old, _)) = self.rows.get(&claim) {
            return Ok(**old == *value);
        }
        let lease = reserve_native_requests_v1(crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<
            Hash64,
            (NativeRetainedV1<PalwPrefixDependencyEnvelopeV1>, Option<NativeDemandLeaseV1>),
        >() as u64)?;
        self.rows.insert(claim, (value, lease));
        Ok(true)
    }
    pub fn compatible_v1(&self, other: &Self) -> bool {
        other.rows.iter().all(|(id, (value, _))| self.rows.get(id).is_none_or(|(old, _)| **old == **value))
    }
    pub fn append_v1(&mut self, other: Self) -> Result<bool, NativeCoverageV1> {
        if !self.compatible_v1(&other) {
            return Ok(false);
        }
        for (id, (value, _membership)) in other.rows {
            self.insert_v1(id, value)?;
        }
        Ok(true)
    }
    pub fn attach_v1(
        &self,
        answers: &crate::palw_fp_answer_accounting_v1::PalwFpClaimAnswersV1,
        mut extras: crate::palw_state_v2::PalwTransitionExtrasV1,
    ) -> Result<PalwPrefixExtrasV1, NativeCoverageV1> {
        let mut demand = 0u64;
        for (value, _) in self.rows.values() {
            let value_demand = native_value_requests_v1(&**value)?;
            demand = demand
                .checked_add(
                    crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<Hash64, PalwPrefixDependencyEnvelopeV1>() as u64
                )
                .and_then(|n| n.checked_add(value_demand))
                .ok_or_else(|| NativeCoverageV1::overflow::<Self>("prefix extras copy"))?;
        }
        let lease = reserve_native_requests_v1(demand)?;
        for (id, (value, _)) in &self.rows {
            extras.prefix_dependencies_v1.insert(*id, (**value).clone());
        }
        Ok(PalwPrefixExtrasV1 { extras: answers.attach_v1(extras)?, _prefix_lease: lease })
    }
}
