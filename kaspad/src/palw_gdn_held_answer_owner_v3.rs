//! Actual persistent answer and complete-answer-copy ownership. Authentication
//! scratch, candidate enumeration and refutation construction are separate.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1, reserve_native_requests_v1,
};
use std::{mem::size_of, ops::Deref};

pub(super) struct AnswerSlotsV3 {
    _lease: Option<NativeDemandLeaseV1>,
}
impl std::fmt::Debug for AnswerSlotsV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnswerSlotsV3")
    }
}
pub(super) fn slots(count: usize) -> Result<(Vec<Option<NativeRetainedV1<PalwHeldDisclosureV1>>>, AnswerSlotsV3), NativeCoverageV1> {
    const PATH: &str = "held GDN persistent answer slots";
    let bytes = (count as u64)
        .checked_mul(size_of::<Option<NativeRetainedV1<PalwHeldDisclosureV1>>>() as u64)
        .ok_or_else(|| NativeCoverageV1::overflow::<AnswerSlotsV3>(PATH))?;
    let lease = reserve_native_requests_v1(bytes)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| NativeCoverageV1::unsupported::<AnswerSlotsV3>(PATH, "answer slot allocation refused"))?;
    values.resize_with(count, || None);
    Ok((values, AnswerSlotsV3 { _lease: lease }))
}

pub(super) struct AnswerBatchV3 {
    values: Vec<PalwHeldDisclosureV1>,
    _lease: Option<NativeDemandLeaseV1>,
}
impl Deref for AnswerBatchV3 {
    type Target = [PalwHeldDisclosureV1];
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
/// Derive the destination from each actual authenticated disclosure and count
/// the independent Vec slots. Admit before cloning any chunk, opening or path.
pub(super) fn complete_batch(answers: &[Option<NativeRetainedV1<PalwHeldDisclosureV1>>]) -> Result<AnswerBatchV3, PalwGdnHeldErrorV2> {
    const PATH: &str = "held GDN complete answer batch";
    let bytes = (answers.len() as u64)
        .checked_mul(size_of::<PalwHeldDisclosureV1>() as u64)
        .ok_or_else(|| NativeCoverageV1::overflow::<AnswerBatchV3>(PATH))?;
    let mut request = NativeCoverageV1::Bounded { bytes };
    for answer in answers {
        let answer = answer.as_ref().ok_or("complete GDN answer batch contains a missing unit")?;
        request = request.add::<AnswerBatchV3>(answer.native_heap_v1(PATH), PATH);
    }
    let bytes = request.bytes().ok_or(request)?;
    let lease = reserve_native_requests_v1(bytes)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(answers.len())
        .map_err(|_| NativeCoverageV1::unsupported::<AnswerBatchV3>(PATH, "answer batch allocation refused"))?;
    for answer in answers {
        values.push((**answer.as_ref().expect("checked before admission")).clone());
    }
    Ok(AnswerBatchV3 { values, _lease: lease })
}
