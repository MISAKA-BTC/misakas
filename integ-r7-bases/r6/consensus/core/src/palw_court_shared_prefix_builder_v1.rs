//! Private version11 TIR prefix material replay. No intent, owner slot, public
//! ingress, paid duty, state writer, slash or complete-original capability.
use super::SharedTirBuilderStoreV1 as Store;
use crate::{Hash64,palw_state_v2::PalwChainStateV2};
use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Q,PalwTirPublicResponseV1 as R};
use crate::palw_native_resource_v1::{NativeRetainedV1,NativeDecodeErrorV1 as E,NativeCoverageV1,
    TirNativeRequestsV1,NativeHeapV1,NativeDemandLeaseV1,reserve_native_requests_v1};
use crate::palw_tir_prefix_state_v1::PalwTirPrefixStateVerdictV1 as Verdict;
#[derive(Clone,Copy)]
pub(crate) struct Budget {pub max_leaves:u64,pub max_requests:u64}
#[derive(Debug,PartialEq,Eq)]
pub(crate) struct PrefixMaterialReplayV1 {source:Hash64,claim:Hash64,scope:Hash64,leaves:u64,verdict:Verdict}
impl PrefixMaterialReplayV1 {
    pub(crate) fn verdict(&self)->Verdict {self.verdict}
    pub(crate) fn leaves(&self)->u64 {self.leaves}
}
fn resource(why:&'static str)->E<String> {E::Resource(NativeCoverageV1::unsupported::<PrefixMaterialReplayV1>("private prefix material",why))}
pub(crate) fn prelude_requests(frame:&crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1,
    record:&crate::palw_tir_admission_v1::PalwTirClassRecordV1,layout:&crate::palw_tir_class_v1::PalwTirLayoutV1)
    ->Result<u64,E<String>> {
    let mut q=TirNativeRequestsV1::new();
    q.add_v1(crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::native_construction_requests_v1(record,layout,frame.header_v1())?)
        .map_err(E::Resource)?;
    // Only version11's fixed Prefix tail is allowed here, not a carried DFA or
    // unknown-tail parser. Existing fp_claim_id/scope writers run sequentially.
    let clone=frame.native_heap_v1("prefix current-context identity clone");
    q.add_v1(clone.bytes().ok_or(E::Resource(clone))?).map_err(E::Resource)?;
    let bytes=borsh::object_length(frame).map_err(|e|E::Decode(e.to_string()))? as u64;
    let writer=bytes.max(1024).checked_mul(2).ok_or_else(||resource("prefix frame hash writer overflow"))?;
    q.exact_v1::<u8>(writer).map_err(E::Resource)?;
    q.exact_v1::<crate::palw_staged_court_state_v1::PalwTirPrefixContextV1>(1).map_err(E::Resource)?;
    Ok(q.bytes_v1())
}
/// Borrowed guard BEFORE the legacy primitive's lane-decoder allocation.
/// Source DA authenticates even malformed committed preimages; prefix shape
/// admission must not reserve only canonical bytes then decode a larger body.
pub(crate) fn check_prefix_body_length_v1(leaf:&crate::palw_tir_step_v1::PalwTirLeafV1,
    maximum:u64,values:&[u8])->Result<(),E<String>> {
    let expected=u64::from(leaf.value_count).checked_mul(4)
        .ok_or_else(||E::Decode("prefix canonical lane byte count overflow".into()))?;
    let actual=u64::try_from(values.len()).map_err(|_|E::Decode("prefix body byte count exceeds u64".into()))?;
    if expected>maximum||actual!=expected {
        return Err(E::Decode("prefix authenticated body differs from canonical byte length".into()));
    }
    Ok(())
}
pub(crate) fn replay_requests(leaf_bytes:u64)->Result<u64,E<String>> {
    let mut q=TirNativeRequestsV1::new();
    q.vec_v1::<i128>(leaf_bytes/4).map_err(E::Resource)?; // actual lane decoder
    q.exact_v1::<u8>(leaf_bytes).map_err(E::Resource)?; // existing leaf preimage clone
    q.exact_v1::<u8>(32).map_err(E::Resource)?; // normative leaf prelude capacity
    // Existing step_tile_leaf_hash writer: 154 bytes before canonical values;
    // step-leg Writer starts at256, growth bounded by twice the larger value.
    let writer=leaf_bytes.checked_add(154).and_then(|x|x.max(256).checked_mul(2))
        .ok_or_else(||resource("prefix step hash writer overflow"))?;
    q.exact_v1::<u8>(writer).map_err(E::Resource)?;
    q.exact_v1::<PrefixMaterialReplayV1>(1).map_err(E::Resource)?;
    Ok(q.bytes_v1())
}
impl Store<'_> {
    /// Data-only replay against exact current retained original/context. Native
    /// Source/Store construction and caller-held units keep separate ownership.
    /// Does not bypass the production original's existing job-tail refusal.
    pub(crate) fn replay_prefix_material_v1(&self,state:&PalwChainStateV2,claim:Hash64,budget:Budget)
        ->Result<NativeRetainedV1<PrefixMaterialReplayV1>,E<String>> {
        self.replay_prefix_reserved_v1(state,claim,budget,reserve_native_requests_v1)
    }
    pub(crate) fn replay_prefix_reserved_v1<F>(&self,state:&PalwChainStateV2,claim:Hash64,budget:Budget,reserve:F)
        ->Result<NativeRetainedV1<PrefixMaterialReplayV1>,E<String>>
    where F:FnOnce(u64)->Result<Option<NativeDemandLeaseV1>,NativeCoverageV1> {
        let c=crate::palw_court_shared_tir_state_v1::context(state,claim).map_err(E::Decode)?;
        let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,claim)
            .ok_or_else(||E::Decode("prefix material original frame absent".into()))?;
        let job=&frame.original_v1().job;
        if job.version!=crate::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION
            || !matches!(job.tail.as_ref(),Some(crate::palw_freeprompt_v3::PalwFpJobTailV1::Prefix(_))) {
            return Err(E::Decode("private prefix replay requires actual version11 Prefix job".into()));
        }
        let record=state.tir_class_v1(&job.class_id).ok_or_else(||E::Decode("prefix registered class absent".into()))?;
        let layout=crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state,job.class_id)
            .ok_or_else(||E::Decode("prefix registered layout absent".into()))?;
        let initial=prelude_requests(frame,record,layout)?;
        if initial>budget.max_requests {return Err(resource("prefix initial local request budget"));}
        let mut lease=reserve(initial).map_err(E::Resource)?;
        {
            // Real current context/source/service validation, not a caller flag.
            // Its additional Source is dropped before constructing prefix space.
            let current=c.source(state).map_err(E::Decode)?;
            if current.source_id_v1()!=self.source.source_id_v1()||current.header_v1()!=self.source.header_v1() {
                return Err(E::Decode("prefix material Store from another original source".into()));
            }
        }
        let mut prefix=crate::palw_staged_court_state_v1::palw_tir_prefix_context_v1(record,layout,
            frame.header_v1(),job,frame.descriptor_v1().maximum_leaves).map_err(E::Decode)?;
        let named=crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job).map_err(|e|E::Decode(e.to_string()))?;
        let total=prefix.template.total_v1();
        if total>budget.max_leaves {return Err(resource("prefix local leaf budget"));}
        let additional=replay_requests(prefix.leaf_bytes)?;
        if initial.checked_add(additional).is_none_or(|sum|sum>budget.max_requests) {
            return Err(resource("prefix replay local request budget"));
        }
        if let Some(ticket)=lease.as_mut() {ticket.grow_memory_v1(additional).map_err(E::Resource)?;}
        for ordinal in 0..total {
            let leaf=prefix.space.prefix_state_leaf_v1(&prefix.job_context,named.prefix_tokens,ordinal)
                .ok_or_else(||E::Decode("prefix canonical ordinal missing".into()))?;
            let Some(R::StepLeaf{preimage,opening})=self.unit(Q::StepLeaf{index:leaf.index}) else {
                return Err(E::Decode(format!("prefix missing authenticated boundary StepLeaf {}",leaf.index)));
            };
            check_prefix_body_length_v1(&leaf,prefix.leaf_bytes,&preimage.values_le)?;
            // Existing normative stream authenticates actual full body and exact
            // old opening before folding; no local hash/declared verdict substitute.
            prefix.template.leaf_v1(&prefix.space,&prefix.job_context,&preimage.values_le,opening).map_err(E::Decode)?;
        }
        let verdict=prefix.template.verdict_v1().ok_or_else(||E::Decode("prefix native replay has no final verdict".into()))?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(PrefixMaterialReplayV1{source:self.source.source_id_v1(),
            claim,scope:c.scope_id,leaves:total,verdict},lease))
    }
}
