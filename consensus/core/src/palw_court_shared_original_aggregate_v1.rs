//! Immutable original-wide dependency draft. Never reservable/admitted authority.
use super::{numerical_material_join::{NumericalMaterialJoinV1 as Numeric,UnresolvedSupplyV1 as NumericSupply},
    output_terminal::{NativeOutputDutyV1 as Output,OutputDutyScopeV1},selection_terminal::NativeSelectionUniverseV1 as Selection,
    constraint_family::{ConstraintUniverseV1 as Constraint,ConstraintSemanticSupplyV1},prefix_family::NativePrefixFamilyV1 as Prefix,
    tokenizer_family::{TokenizerFamilyV1 as Tokenizer,UnresolvedTokenizerSupplyV1}};
use crate::{Hash64,palw_state_v2::PalwChainStateV2,palw_native_resource_v1::{NativeRetainedV1 as Held,NativeCoverageV1 as C,NativeDecodeErrorV1 as E,NativeHeapV1,TirNativeRequestsV1}};
use crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1 as Frame;
use crate::palw_tir_public_source_v1::PalwTirPublicSourceV1 as Source;
use crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3 as Graph;
use crate::palw_class_semantics_v1::PalwClassSemanticsV1 as Semantics;
/// Fixed read-only identities, no capability or provider receipt.
pub(super) type FamilyBindingV1=(Hash64,Hash64,Hash64,Hash64,Hash64);
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum OriginalGrammarV1 {Ordinary,Constraint,Prefix}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum UnsupportedNativeGrammarV1 {JobVersionOrTail,FlatTrace,PrefixGeneratedSelectionAndStop}
#[derive(Debug)]
pub(crate) enum AggregateErrorV1 {Resource(C),Invalid(String),UnsupportedNativeGrammar(UnsupportedNativeGrammarV1)}
fn bad(s:&str)->AggregateErrorV1{AggregateErrorV1::Invalid(s.into())}
fn native(e:E<String>)->AggregateErrorV1{match e {E::Resource(r)=>AggregateErrorV1::Resource(r),E::Decode(s)=>AggregateErrorV1::Invalid(s)}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum TokenizerApplicabilityV1 {NotApplicable,Required}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct OriginalFamilyShapeV1 {grammar:OriginalGrammarV1,tokenizer:TokenizerApplicabilityV1}
impl OriginalFamilyShapeV1 {
    /// Material-local classification only. Caller must authenticate the literal
    /// original/immutable snapshot; this is reusable by a future PreparedOriginal.
    pub(crate) fn derive_v1(frame:&Frame,semantics:Option<&Semantics>)->Result<Self,AggregateErrorV1>{
        use crate::palw_freeprompt_v3::{PalwFpJobTailV1 as Tail,PALW_FP_V3_VERSION,PALW_FP_V4_VERSION};
        let job=&frame.original_v1().job;
        let grammar=match (job.version,job.tail.as_ref()) {
            (PALW_FP_V3_VERSION,None) if job.decode.is_none()=>OriginalGrammarV1::Ordinary,
            (PALW_FP_V4_VERSION,None) if job.decode.is_some()=>OriginalGrammarV1::Ordinary,
            (crate::palw_fp_constraint_job_v1::PALW_FP_CONSTRAINT_VERSION,Some(Tail::Constraint(_))) if job.decode.is_none()=>OriginalGrammarV1::Constraint,
            (crate::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION,Some(Tail::Prefix(_))) if job.decode.is_some()=>OriginalGrammarV1::Prefix,
            _=>return Err(AggregateErrorV1::UnsupportedNativeGrammar(UnsupportedNativeGrammarV1::JobVersionOrTail)),
        };
        let stage=frame.plan_v1().semantic.as_ref().is_some_and(|p|p.stages.iter().any(|s|s.family==crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Tokenizer));
        let fields=semantics.is_some_and(|s|s.rendering_rule.is_some()||s.raw_tokenizer_bytes.is_some()||s.tokenizer_key_events.is_some());
        if stage!=fields{return Err(bad("aggregate native Tokenizer stage/immutable qualification differs"));}
        if let Some(s)=semantics {if !s.matches_context(&frame.header_v1().job_context)||!s.well_formed_for(&job.class_id,&frame.header_v1().artifact_root)||s.bound_from_daa>frame.plan_v1().terms.accepted_daa{return Err(bad("aggregate immutable semantics does not bind actual original"));}}
        if stage&&!semantics.is_some_and(|s|s.rendering_rule.is_some()&&s.raw_tokenizer_bytes.is_some()&&s.tokenizer_key_events.is_some()&&s.token_table_root.is_some()){return Err(bad("aggregate required Tokenizer metadata incomplete"));}
        Ok(Self{grammar,tokenizer:if stage{TokenizerApplicabilityV1::Required}else{TokenizerApplicabilityV1::NotApplicable}})
    }
    pub(crate) fn grammar_v1(self)->OriginalGrammarV1{self.grammar}
    pub(crate) fn tokenizer_v1(self)->TokenizerApplicabilityV1{self.tokenizer}
}
/// Structurally required families, not a configurable Vec of supported subset.
pub(crate) enum OriginalFamilyOwnersV1<'a> {
    Ordinary{output_and_stop:&'a Held<Output>,selection:&'a Held<Selection<'a>>},
    // Constraint owns the sole FullIds/output vector already; do not duplicate it.
    Constraint{constraint:&'a Held<Constraint<'a>>},
    Prefix{output:&'a Held<Output>,prefix:&'a Held<Prefix<'a>>},
}
pub(crate) enum TokenizerOwnersV1<'a>{NotApplicable,Required(&'a Held<Tokenizer<'a>>)}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum UnresolvedSupplyContractV1 {
    Numerical(NumericSupply),Constraint(ConstraintSemanticSupplyV1),Tokenizer(UnresolvedTokenizerSupplyV1),
    /// Existing graph descriptors are not actual adopted allbranch packet terms,
    /// service reservation, execution fee, source custody or financial funding.
    NativeAllBranchPacketAndFundingTerms,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum AggregateNotReadyV1 {UnresolvedSupplyContract(UnresolvedSupplyContractV1),UnsupportedNativeGrammar(UnsupportedNativeGrammarV1)}
pub(crate) struct OriginalAggregateDraftV1<'a>{shape:OriginalFamilyShapeV1,numeric:&'a Held<Numeric<'a>>,families:OriginalFamilyOwnersV1<'a>,tokenizer:TokenizerOwnersV1<'a>}
impl<'a> OriginalAggregateDraftV1<'a>{
    /// Postclaim retained-state constructor. It CANNOT stand in for pre-original
    /// native preparation: all source/material/family owners already exist here.
    pub(crate) fn derive_current_v1(state:&PalwChainStateV2,frame:&'a Frame,source:&'a Held<Source>,graph:&'a Held<Graph>,profile:Hash64,numeric:&'a Held<Numeric<'a>>,families:OriginalFamilyOwnersV1<'a>,tokenizer:TokenizerOwnersV1<'a>)
        ->Result<Held<Self>,AggregateErrorV1>{
        Self::derive_reserved_v1(state,frame,source,graph,profile,numeric,families,tokenizer,crate::palw_native_resource_v1::reserve_native_requests_v1)
    }
    pub(crate) fn derive_reserved_v1<F>(state:&PalwChainStateV2,frame:&'a Frame,source:&'a Held<Source>,graph:&'a Held<Graph>,profile:Hash64,numeric:&'a Held<Numeric<'a>>,families:OriginalFamilyOwnersV1<'a>,tokenizer:TokenizerOwnersV1<'a>,reserve:F)
        ->Result<Held<Self>,AggregateErrorV1> where F:FnOnce(u64)->Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,C>{
        let claim=frame.plan_v1().original_claim;
        if crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,claim)!=Some(frame){return Err(bad("aggregate substitutes actual current frame"));}
        if !source.supports_tiled_trace_v1(){return Err(AggregateErrorV1::UnsupportedNativeGrammar(UnsupportedNativeGrammarV1::FlatTrace));}
        let shape=OriginalFamilyShapeV1::derive_v1(frame,state.claim_semantics_v1(&claim))?;
        let record=state.tir_class_v1(&frame.original_v1().job.class_id).ok_or_else(||bad("aggregate registered class absent"))?;
        let layout=crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state,frame.original_v1().job.class_id).ok_or_else(||bad("aggregate layout absent"))?;
        let mut q=TirNativeRequestsV1::new();q.add_v1(Source::native_construction_requests_v1(record,layout,frame.header_v1()).map_err(native)?).map_err(AggregateErrorV1::Resource)?;
        let heap=frame.native_heap_v1("aggregate current identity scratch");q.add_v1(heap.bytes().ok_or_else(||AggregateErrorV1::Resource(heap))?).map_err(AggregateErrorV1::Resource)?;
        let bytes=borsh::object_length(frame).map_err(|e|bad(&e.to_string()))? as u64;
        q.exact_v1::<u8>(bytes.max(1024).checked_mul(2).ok_or_else(||AggregateErrorV1::Resource(C::overflow::<Self>("aggregate frame writer")))?).map_err(AggregateErrorV1::Resource)?;
        q.exact_v1::<Self>(1).map_err(AggregateErrorV1::Resource)?;
        let lease=reserve(q.bytes_v1()).map_err(AggregateErrorV1::Resource)?;
        graph.check_frame_binding_v3(frame,source,profile).map_err(AggregateErrorV1::Invalid)?;
        let c=crate::palw_court_shared_tir_state_v1::context(state,claim).map_err(AggregateErrorV1::Invalid)?;
        {let actual=c.source(state).map_err(AggregateErrorV1::Invalid)?;if c.profile_id!=profile||actual.source_id_v1()!=source.source_id_v1()||actual.header_v1()!=source.header_v1(){return Err(bad("aggregate current source/context changed"));}}
        let expected=(claim,source.source_id_v1(),graph.scope_v3(),graph.absolute_scope_v3(),profile);
        if numeric.aggregate_binding_v1()!=expected{return Err(bad("aggregate numeric geometry belongs to another allocation"));}
        match (&families,shape.grammar){
            (OriginalFamilyOwnersV1::Ordinary{output_and_stop,selection},OriginalGrammarV1::Ordinary)=>{
                if output_and_stop.aggregate_binding_v1()!=expected||selection.aggregate_binding_v1()!=expected||output_and_stop.scope_v1()!=OutputDutyScopeV1::OriginalOutputAndNativeRegularStopOnly{return Err(bad("aggregate ordinary output/stop/selection incomplete or foreign"));}
            },
            (OriginalFamilyOwnersV1::Constraint{constraint},OriginalGrammarV1::Constraint)=>{
                if constraint.aggregate_binding_v1()!=expected||state.claim_semantics_v1(&claim).map(|s|s.commitment())!=Some(constraint.aggregate_semantics_v1()){return Err(bad("aggregate Constraint family foreign"));}
            },
            (OriginalFamilyOwnersV1::Prefix{output,prefix},OriginalGrammarV1::Prefix)=>{if output.aggregate_binding_v1()!=expected||prefix.aggregate_binding_v1()!=expected{return Err(bad("aggregate Prefix output/boundary foreign"));}},
            _=>return Err(bad("aggregate family variant differs from actual native job")),
        }
        match (&tokenizer,shape.tokenizer){
            (TokenizerOwnersV1::NotApplicable,TokenizerApplicabilityV1::NotApplicable)=>{},
            (TokenizerOwnersV1::Required(t),TokenizerApplicabilityV1::Required)=>{
                let sem=state.claim_semantics_v1(&claim).ok_or_else(||bad("aggregate Tokenizer immutable snapshot absent"))?;
                let eof=state.staged_tokenizer_inventory_v1(sem.tokenizer_id).ok_or_else(||bad("aggregate Tokenizer rooted EOF absent"))?;
                if t.aggregate_binding_v1()!=(claim,source.source_id_v1(),c.scope_id,sem.commitment(),eof.inventory_id){return Err(bad("aggregate Tokenizer family foreign"));}
            },_=>return Err(bad("aggregate required Tokenizer cannot be omitted or invented")),
        }
        Ok(Held::from_owner_reserved_v1(Self{shape,numeric,families,tokenizer},lease))
    }
    pub(crate) fn shape_v1(&self)->OriginalFamilyShapeV1{self.shape}
    pub(crate) fn unresolved_supply_v1(&self)->impl Iterator<Item=UnresolvedSupplyContractV1>+'_ {
        let constraint=match &self.families{OriginalFamilyOwnersV1::Constraint{constraint}=>Some(UnresolvedSupplyContractV1::Constraint(constraint.semantic_supply_v1())),_=>None};
        let tokenizer=match &self.tokenizer{TokenizerOwnersV1::Required(t)=>Some(UnresolvedSupplyContractV1::Tokenizer(t.unresolved_supply_v1())),_=>None};
        self.numeric.unresolved_v1().map(UnresolvedSupplyContractV1::Numerical).chain(constraint).chain(tokenizer).chain(std::iter::once(UnresolvedSupplyContractV1::NativeAllBranchPacketAndFundingTerms))
    }
    /// An inventory is not a service/escrow capability. There is deliberately no
    /// success variant or conversion to a fully reservable/admitted original.
    pub(crate) fn reservation_readiness_v1(&self)->Result<std::convert::Infallible,AggregateNotReadyV1>{
        if self.shape.grammar==OriginalGrammarV1::Prefix{return Err(AggregateNotReadyV1::UnsupportedNativeGrammar(UnsupportedNativeGrammarV1::PrefixGeneratedSelectionAndStop));}
        Err(AggregateNotReadyV1::UnresolvedSupplyContract(self.unresolved_supply_v1().next().expect("native allbranch terms always unresolved")))
    }
}
