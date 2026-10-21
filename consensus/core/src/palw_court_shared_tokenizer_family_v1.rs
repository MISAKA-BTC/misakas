//! Compact all-ID Tokenizer requirement inventory, not public supply/admission.
use super::{SharedTirBuilderStoreV1 as Store,semantic_material::SemanticMaterialInventoryV1 as Material,
    tokenizer_terminal::{SignedTokenizerTerminalIntentV1 as Intent,TokenizerTerminalEvidenceV1 as Evidence,TokenizerMaterialV1}};
use crate::{Hash64,palw_state_v2::{PalwChainStateV2,PalwStateParamsV2},palw_native_resource_v1::{NativeRetainedV1 as Held,NativeDecodeErrorV1 as E,NativeCoverageV1 as C,NativeHeapV1,TirNativeRequestsV1}};
use crate::palw_tir_public_source_v1::PalwTirPublicSourceV1 as Source;
use crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1 as Frame;
use crate::palw_tokenizer_table_stream_v1::{producer::Generated,canonical_replay::Budget};
use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
fn bad(s:&str)->E<String>{E::Decode(s.into())}
fn missing(why:&str)->E<String>{E::Resource(C::unsupported::<TokenizerFamilyV1<'static>>("Tokenizer required retained material",why))}
fn resource()->E<String>{E::Resource(C::unsupported::<TokenizerFamilyV1<'static>>("private Tokenizer family","checked native inventory allocation"))}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum TokenizerContextRequirementV1 {ActualCurrentSignedIntentContext}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct UnresolvedTokenizerSupplyV1 {pub table:Hash64,pub tokenizer:Hash64,pub inventory:Hash64,pub vocabulary:u32,pub raw_bytes:u64,pub key_proofs:u64}
/// Borrows the complete authenticated table/raw and generated proof owners. No
/// caller token subset or Cartesian context Vec can construct this owner.
pub(crate) struct TokenizerFamilyV1<'a>{frame:&'a Frame,source:&'a Held<Source>,material:&'a Held<Material>,proofs:&'a Held<Generated>,scope:Hash64,semantics:Hash64,inventory:Hash64,table:Hash64,vocabulary:u32,tokenizer:Hash64,raw_bytes:u64}
impl<'a> TokenizerFamilyV1<'a>{
    pub(crate) fn derive_native_v1(state:&PalwChainStateV2,frame:&'a Frame,source:&'a Held<Source>,material:&'a Held<Material>,proofs:&'a Held<Generated>)
        ->Result<Held<Self>,E<String>>{Self::derive_reserved_v1(state,frame,source,material,proofs,crate::palw_native_resource_v1::reserve_native_requests_v1)}
    pub(crate) fn derive_reserved_v1<F>(state:&PalwChainStateV2,frame:&'a Frame,source:&'a Held<Source>,material:&'a Held<Material>,proofs:&'a Held<Generated>,reserve:F)
        ->Result<Held<Self>,E<String>> where F:FnOnce(u64)->Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,C>{
        let claim=frame.plan_v1().original_claim;let job=&frame.original_v1().job;
        if crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,claim)!=Some(frame){return Err(bad("Tokenizer family current frame mismatch"));}
        let record=state.tir_class_v1(&job.class_id).ok_or_else(||missing("Tokenizer family current class absent"))?;
        let layout=crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state,job.class_id).ok_or_else(||missing("Tokenizer family current layout absent"))?;
        let mut q=TirNativeRequestsV1::new();q.add_v1(Source::native_construction_requests_v1(record,layout,frame.header_v1())?).map_err(E::Resource)?;
        let clone=frame.native_heap_v1("Tokenizer family current identity scratch");q.add_v1(clone.bytes().ok_or(E::Resource(clone))?).map_err(E::Resource)?;
        let bytes=borsh::object_length(frame).map_err(|e|bad(&e.to_string()))? as u64;q.exact_v1::<u8>(bytes.max(1024).checked_mul(2).ok_or_else(resource)?).map_err(E::Resource)?;
        q.exact_v1::<Self>(1).map_err(E::Resource)?;let lease=reserve(q.bytes_v1()).map_err(E::Resource)?;
        let c=crate::palw_court_shared_tir_state_v1::context(state,claim).map_err(E::Decode)?;
        {let actual=c.source(state).map_err(E::Decode)?;if actual.source_id_v1()!=source.source_id_v1()||actual.header_v1()!=source.header_v1(){return Err(bad("Tokenizer family source changed"));}}
        let sem=state.claim_semantics_v1(&claim).ok_or_else(||missing("Tokenizer family immutable semantics absent"))?;
        if !sem.matches_context(&source.header_v1().job_context)||!sem.well_formed_for(&job.class_id,&source.header_v1().artifact_root)||sem.bound_from_daa>c.original_accepted_daa{return Err(bad("Tokenizer family native immutable semantics mismatch"));}
        let table=sem.token_table_root.ok_or_else(||missing("Tokenizer family table root absent"))?;
        // Require the prospective native stage, not merely any legacy metadata.
        if !frame.plan_v1().semantic.as_ref().is_some_and(|p|p.stages.iter().any(|s|s.family==crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Tokenizer)){return Err(missing("Tokenizer family actual native stage absent"));}
        let eof=state.staged_tokenizer_inventory_v1(sem.tokenizer_id).ok_or_else(||missing("Tokenizer family rooted EOF absent"))?;eof.check_v1().map_err(E::Decode)?;
        let key_bound=sem.tokenizer_key_events.ok_or_else(||missing("Tokenizer family immutable key-work bound absent"))?;
        let counts=material.counts_v1();let semantics=sem.commitment();
        material.check_scope_v1(source.source_id_v1(),claim,c.scope_id,semantics,eof.inventory_id)?;
        if counts.vocabulary!=source.vocab_v1()||counts.vocabulary==0||eof.artifact_hash!=sem.tokenizer_id||eof.completed_daa>c.original_accepted_daa
            ||counts.raw_bytes!=eof.actual_bytes||proofs.key_work_bound_v1()!=key_bound||proofs.artifact_binding_v1()!=(sem.tokenizer_id,eof.actual_bytes){return Err(bad("Tokenizer family all-ID/raw/producer provenance mismatch"));}
        Ok(Held::from_owner_reserved_v1(Self{frame,source,material,proofs,scope:c.scope_id,semantics,inventory:eof.inventory_id,table,vocabulary:counts.vocabulary,tokenizer:sem.tokenizer_id,raw_bytes:eof.actual_bytes},lease))
    }
    pub(crate) fn len_v1(&self)->u64{u64::from(self.vocabulary)}
    pub(crate) fn token_at_v1(&self,n:u64)->Option<u32>{(n<self.len_v1()).then_some(n as u32)}
    pub(crate) fn context_requirement_v1(&self)->TokenizerContextRequirementV1{TokenizerContextRequirementV1::ActualCurrentSignedIntentContext}
    pub(crate) fn unresolved_supply_v1(&self)->UnresolvedTokenizerSupplyV1{UnresolvedTokenizerSupplyV1{table:self.table,tokenizer:self.tokenizer,inventory:self.inventory,vocabulary:self.vocabulary,raw_bytes:self.raw_bytes,key_proofs:self.proofs.proofs().len() as u64}}
    pub(crate) fn material_id_v1(&self,id:u32,budget:Budget)->Result<Hash64,String>{
        let rendering=self.material.rendering_v1(id).ok_or("Tokenizer family target outside complete actual vocabulary")?;
        TokenizerMaterialV1{raw:self.material.raw_v1(),proofs:self.proofs.proofs(),rendering}.identity(budget)
    }
    /// Exact current signed context is chosen at runtime, never a caller promise
    /// that one successful target certifies the remaining token IDs.
    pub(crate) fn build_v1(&self,store:&Store<'_>,state:&PalwChainStateV2,params:&PalwStateParamsV2,rules:&NativeCourtRulesV1,intent:&Intent,budget:Budget)
        ->Result<Evidence<'a>,String>{
        if intent.claim!=self.frame.plan_v1().original_claim||intent.scope_id!=self.scope||intent.semantics_id!=self.semantics||intent.inventory_id!=self.inventory||intent.token_id>=self.vocabulary
            ||crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,intent.claim)!=Some(self.frame)
            ||store.source.source_id_v1()!=self.source.source_id_v1()||store.source.header_v1()!=self.source.header_v1(){return Err("Tokenizer family runtime current frame/target mismatch".into());}
        // Uses actual raw + rooted EOF + rendering + proof operands and native
        // current actor/parser. Returned evidence borrows their retained owners.
        let evidence=store.build_tokenizer_terminal_retained_v1(state,params,rules,intent,self.material,self.proofs.proofs(),budget)?;
        evidence.recheck(store,state,params,rules,budget)?;Ok(evidence)
    }
}

impl TokenizerFamilyV1<'_>{
    pub(super) fn aggregate_binding_v1(&self)->super::original_aggregate::FamilyBindingV1{(self.frame.plan_v1().original_claim,self.source.source_id_v1(),self.scope,self.semantics,self.inventory)}
}
