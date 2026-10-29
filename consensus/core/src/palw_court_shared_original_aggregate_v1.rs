//! Immutable original-wide dependency draft. Readiness names adopted packet and
//! funding pins a caller can reserve against. It does not admit, escrow, or arm a fence.
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
    /// Graph descriptors are not packet or funding terms. Present only when the
    /// adopted `palw_g14_values_v1` pins are not the terms in force.
    NativeAllBranchPacketAndFundingTerms,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum AggregateNotReadyV1 {UnresolvedSupplyContract(UnresolvedSupplyContractV1),UnsupportedNativeGrammar(UnsupportedNativeGrammarV1),ForeignBinding}
/// Adopted all-branch packet and funding pins. Callers reserve against these
/// terms; this value does not write a book row, collect funds, or arm a fence.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct OriginalReservationReadyV1 {
    pub service_profile:crate::palw_court_service_binding_v1::CourtServiceProfileV1,
    pub source_packet_cost:crate::palw_court_service_v1::ServiceCost,
    pub packet_bytes:u64,
    pub follow_up_branches_per_turn:u64,
    pub packet_carriage_sompi:u64,
    pub fee_sompi:u64,
    pub capital_per_turn_sompi:u64,
    pub capital_per_source_node_sompi:u64,
    pub capital_floor_sompi:u64,
    pub heap_bytes_per_branch:u64,
    pub retention_bytes_per_branch:u64,
}
impl OriginalReservationReadyV1 {
    /// Schedule a caller passes to all-branch admission, built from these terms.
    pub(crate) fn funded_admission_schedule_v1(self)->Result<crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1,&'static str>{
        crate::palw_funded_original_admission_v1::FundedAdmissionScheduleV1::new_v1(
            self.service_profile,self.source_packet_cost,self.capital_per_turn_sompi,self.capital_per_source_node_sompi,self.heap_bytes_per_branch,self.retention_bytes_per_branch)
    }
    fn adopted_v1()->Option<Self>{
        use crate::palw_g14_values_v1 as g;
        let pinned=crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1?;
        let kernel=crate::palw_kernel_court_admission_v1::PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1?;
        let ready=Self{
            service_profile:g::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1,
            source_packet_cost:g::PALW_G14_SOURCE_PACKET_COST_V1,
            packet_bytes:g::PALW_G14_PACKET_BYTES_V1,
            follow_up_branches_per_turn:g::PALW_G14_FOLLOW_UP_BRANCHES_PER_TURN_ESTIMATE_V1,
            packet_carriage_sompi:g::PALW_G14_PACKET_CARRIAGE_SOMPI_V1,
            fee_sompi:g::PALW_G14_FUNDED_FEE_SOMPI_V1,
            capital_per_turn_sompi:g::PALW_G14_CAPITAL_PER_TURN_SOMPI_V1,
            capital_per_source_node_sompi:g::PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1,
            capital_floor_sompi:g::PALW_G14_FUNDED_CAPITAL_FLOOR_SOMPI_V1,
            heap_bytes_per_branch:g::PALW_G14_HEAP_BYTES_PER_BRANCH_V1,
            retention_bytes_per_branch:g::PALW_G14_RETENTION_BYTES_PER_BRANCH_V1,
        };
        if ready.service_profile!=pinned||kernel.profile!=ready.service_profile||kernel.follow_up_branches_per_turn!=ready.follow_up_branches_per_turn
            ||kernel.capital_per_packet_sompi!=ready.capital_per_turn_sompi||kernel.version!=1||!ready.pins_match_g14_v1(){return None;}
        ready.funded_admission_schedule_v1().ok()?;
        Some(ready)
    }
    fn pins_match_g14_v1(self)->bool{
        use crate::palw_g14_values_v1 as g;
        let budget=g::PALW_G14_PACKET_BYTES_V1.checked_mul(g::PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1);
        let carriage=g::PALW_G14_PACKET_BYTES_V1.checked_mul(g::PALW_G14_CARRIAGE_SOMPI_PER_BYTE_V1);
        self.service_profile==g::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1
            &&self.packet_bytes==g::PALW_G14_PACKET_BYTES_V1
            &&self.service_profile.packet_cost.bytes==self.packet_bytes
            &&self.service_profile.packets_per_signer==g::PALW_G14_PACKETS_PER_SIGNER_V1
            &&self.service_profile.service_span_daa==g::PALW_G14_SERVICE_SPAN_DAA_V1
            &&budget.is_some_and(|bytes|self.service_profile.budget.bytes==bytes)
            &&self.source_packet_cost==g::PALW_G14_SOURCE_PACKET_COST_V1
            &&self.follow_up_branches_per_turn==g::PALW_G14_FOLLOW_UP_BRANCHES_PER_TURN_ESTIMATE_V1
            &&self.follow_up_branches_per_turn>0
            &&carriage.is_some_and(|sompi|self.packet_carriage_sompi==sompi)
            &&self.packet_carriage_sompi==g::PALW_G14_PACKET_CARRIAGE_SOMPI_V1
            &&self.fee_sompi==g::PALW_G14_FUNDED_FEE_SOMPI_V1&&self.fee_sompi>0
            &&self.capital_per_turn_sompi==g::PALW_G14_CAPITAL_PER_TURN_SOMPI_V1&&self.capital_per_turn_sompi>0
            &&self.capital_per_source_node_sompi==g::PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1&&self.capital_per_source_node_sompi>0
            &&self.capital_floor_sompi==g::PALW_G14_FUNDED_CAPITAL_FLOOR_SOMPI_V1
            &&self.heap_bytes_per_branch==g::PALW_G14_HEAP_BYTES_PER_BRANCH_V1&&self.heap_bytes_per_branch>0
            &&self.retention_bytes_per_branch==g::PALW_G14_RETENTION_BYTES_PER_BRANCH_V1&&self.retention_bytes_per_branch>0
    }
}
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
        let constraint=match &self.families{
            OriginalFamilyOwnersV1::Constraint{constraint}=>match constraint.semantic_supply_v1(){
                supply@(ConstraintSemanticSupplyV1::UnresolvedTableSupply{..}|ConstraintSemanticSupplyV1::UnresolvedEogOnlySupply{..})=>Some(UnresolvedSupplyContractV1::Constraint(supply)),
            },
            _=>None,
        };
        let tokenizer=match (&self.tokenizer,self.shape.tokenizer){
            (TokenizerOwnersV1::Required(t),TokenizerApplicabilityV1::Required)=>Some(UnresolvedSupplyContractV1::Tokenizer(t.unresolved_supply_v1())),
            _=>None,
        };
        let packet=OriginalReservationReadyV1::adopted_v1().is_none().then_some(UnresolvedSupplyContractV1::NativeAllBranchPacketAndFundingTerms);
        self.numeric.unresolved_v1().map(UnresolvedSupplyContractV1::Numerical).chain(constraint).chain(tokenizer).chain(packet)
    }
    /// `Ok` when every present family supply is satisfied, grammar is not Prefix,
    /// bindings agree, and packet/funding terms are the adopted g14 pins.
    /// Does not install a reservation.
    pub(crate) fn reservation_readiness_v1(&self)->Result<OriginalReservationReadyV1,AggregateNotReadyV1>{
        if self.shape.grammar==OriginalGrammarV1::Prefix{return Err(AggregateNotReadyV1::UnsupportedNativeGrammar(UnsupportedNativeGrammarV1::PrefixGeneratedSelectionAndStop));}
        if !self.families_bound_v1(){return Err(AggregateNotReadyV1::ForeignBinding);}
        if let Some(supply)=self.unresolved_supply_v1().next(){return Err(AggregateNotReadyV1::UnresolvedSupplyContract(supply));}
        match OriginalReservationReadyV1::adopted_v1(){
            Some(ready)=>Ok(ready),
            None=>Err(AggregateNotReadyV1::UnresolvedSupplyContract(UnresolvedSupplyContractV1::NativeAllBranchPacketAndFundingTerms)),
        }
    }
    fn families_bound_v1(&self)->bool{
        let expected=self.numeric.aggregate_binding_v1();
        let families=match (&self.families,self.shape.grammar){
            (OriginalFamilyOwnersV1::Ordinary{output_and_stop,selection},OriginalGrammarV1::Ordinary)=>
                output_and_stop.aggregate_binding_v1()==expected&&selection.aggregate_binding_v1()==expected&&output_and_stop.scope_v1()==OutputDutyScopeV1::OriginalOutputAndNativeRegularStopOnly,
            (OriginalFamilyOwnersV1::Constraint{constraint},OriginalGrammarV1::Constraint)=>constraint.aggregate_binding_v1()==expected,
            (OriginalFamilyOwnersV1::Prefix{output,prefix},OriginalGrammarV1::Prefix)=>output.aggregate_binding_v1()==expected&&prefix.aggregate_binding_v1()==expected,
            _=>false,
        };
        let tokenizer=match (&self.tokenizer,self.shape.tokenizer){
            (TokenizerOwnersV1::NotApplicable,TokenizerApplicabilityV1::NotApplicable)=>true,
            (TokenizerOwnersV1::Required(t),TokenizerApplicabilityV1::Required)=>{let b=t.aggregate_binding_v1();b.0==expected.0&&b.1==expected.1},
            _=>false,
        };
        families&&tokenizer
    }
}
