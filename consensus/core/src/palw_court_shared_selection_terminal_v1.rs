//! Private SelectionOnly dependency geometry + existing native adjudication.
//! Potential allocation dependencies are not material receipts or paid capacity.
use super::SharedTirBuilderStoreV1 as Store;
use crate::{Hash64,palw_state_v2::{PalwChainStateV2,PalwStateParamsV2,PalwBondKeyV2,PalwBondStatusV2}};
use crate::palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3 as Graph,SourceMaterialNodeKindV3 as Node,SourceMaterialTargetV3 as Target};
use crate::palw_tir_public_source_v1::{PalwTirPublicSourceV1 as Source,PalwTirPublicRequestV1 as Query};
use crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1 as Frame;
use crate::palw_native_resource_v1::{NativeRetainedV1,NativeDecodeErrorV1 as E,NativeCoverageV1,NativeHeapV1,TirNativeRequestsV1};
use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
use crate::palw_tir_court_v1::{PalwTirSelectionRuleV1 as Rule,PalwTirEvidenceStoreV1};
pub(crate) const SELECTION_INTENT_CONTEXT_V1:&[u8]=b"misaka-palw/shared-selection-terminal/v1/mldsa87";
fn bad(s:&str)->E<String>{E::Decode(s.into())}
fn resource(s:&str)->E<String>{E::Resource(NativeCoverageV1::unsupported::<NativeSelectionUniverseV1<'static>>("private selection family",s))}
fn hash<T:borsh::BorshSerialize>(domain:&[u8],v:&T)->Result<Hash64,String>{crate::palw_court_shared_tir_stream_v1::stream_digest(domain,v)}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum SelectionRuntimeGuardV1{ActualFullIdsAndExactCommittedAndBeatTiles}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct SelectionTargetV1 {pub position:u32,pub beat:u32}
/// Fixed-size deterministic D*V universe, no Cartesian-product Vec. Rule is
/// derived from actual original job, never a caller-supplied rank/mask.
pub(crate) struct NativeSelectionUniverseV1<'a>{frame:&'a Frame,source:&'a Source,graph:&'a NativeRetainedV1<Graph>,profile:Hash64,targets:u64}
fn frame_quote<T>(frame:&Frame)->Result<u64,E<String>>{
    let mut q=TirNativeRequestsV1::new();let clone=frame.native_heap_v1("selection frame and rule clone");
    q.add_v1(clone.bytes().ok_or(E::Resource(clone))?).map_err(E::Resource)?;
    let bytes=borsh::object_length(frame).map_err(|e|E::Decode(e.to_string()))? as u64;
    q.exact_v1::<u8>(bytes.max(1024).checked_mul(2).ok_or_else(||resource("selection frame hash writer overflow"))?).map_err(E::Resource)?;
    q.exact_v1::<T>(1).map_err(E::Resource)?;Ok(q.bytes_v1())
}
impl<'a> NativeSelectionUniverseV1<'a>{
    pub(crate) fn derive_native_v1(frame:&'a Frame,source:&'a Source,graph:&'a NativeRetainedV1<Graph>,profile:Hash64)
        ->Result<NativeRetainedV1<Self>,E<String>>{
        let lease=crate::palw_native_resource_v1::reserve_native_requests_v1(frame_quote::<Self>(frame)?).map_err(E::Resource)?;
        graph.check_frame_binding_v3(frame,source,profile).map_err(E::Decode)?;
        if !source.supports_tiled_trace_v1()||Rule::of_job_v1(&frame.original_v1().job).is_none(){
            return Err(resource("ordinary selection excludes Constraint, tails and Flat sources"));
        }
        let targets=u64::from(source.decode_count_v1()).checked_mul(u64::from(source.vocab_v1())).ok_or_else(||resource("selection target count overflow"))?;
        if targets==0||graph.role_slots_v3()==0{return Err(bad("selection empty source/role domain"));}
        Ok(NativeRetainedV1::from_owner_reserved_v1(Self{frame,source,graph,profile,targets},lease))
    }
    pub(crate) fn len_v1(&self)->u64{self.targets}
    pub(crate) fn target_at_v1(&self,n:u64)->Option<SelectionTargetV1>{
        if n>=self.targets{return None;}let v=u64::from(self.source.vocab_v1());
        Some(SelectionTargetV1{position:(n/v) as u32,beat:(n%v) as u32})
    }
    /// All potential row tiles are reserved because committed ID is not yet an
    /// authenticated input to universe derivation. Runtime still needs ONLY the
    /// exact committed/beat tiles plus ANY actual complete FullIds.
    /// max_graph_visits is a local exploration limit, never an adopted profile.
    pub(crate) fn project_native_v1(&self,target:SelectionTargetV1,max_graph_visits:u64)
        ->Result<NativeRetainedV1<NativeSelectionDutyV1>,E<String>>{
        self.project_native_reserved_v1(target,max_graph_visits,crate::palw_native_resource_v1::reserve_native_requests_v1)
    }
    pub(crate) fn project_native_reserved_v1<F>(&self,target:SelectionTargetV1,max_graph_visits:u64,reserve:F)
        ->Result<NativeRetainedV1<NativeSelectionDutyV1>,E<String>>
    where F:FnOnce(u64)->Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,NativeCoverageV1>{
        if target.position>=self.source.decode_count_v1()||target.beat>=self.source.vocab_v1(){return Err(bad("selection target outside actual D/V"));}
        let tiles=u64::from(self.source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
        let roles=self.graph.role_slots_v3();let total=tiles.checked_add(1).and_then(|n|n.checked_mul(roles)).ok_or_else(||resource("selection dependency count overflow"))?;
        let mut q=TirNativeRequestsV1::new();q.exact_v1::<NativeSelectionDutyV1>(1).map_err(E::Resource)?;
        q.exact_v1::<SelectionDependencyV1>(total).map_err(E::Resource)?;
        let lease=reserve(q.bytes_v1()).map_err(E::Resource)?;
        let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(self.frame,self.source,self.profile).map_err(E::Decode)?;
        let cap=usize::try_from(total).map_err(|_|resource("selection dependency platform overflow"))?;
        let mut deps=Vec::new();deps.try_reserve_exact(cap).map_err(|_|resource("selection dependency allocation"))?;
        for (visit,node) in self.graph.nodes_v3().enumerate(){
            if visit as u64>=max_graph_visits{return Err(resource("selection local graph visit budget"));}
            if let Node::Finish{target:material,role}=node.kind_v3(){
                let tile=match material{
                    Target::FullIds=>Some(None),
                    Target::Unit(n)=>match set.request_at(*n).map_err(E::Decode)?{
                        Query::TraceTile{row,tile} if row==target.position=>Some(Some(u64::from(tile))),_=>None,
                    },
                };
                if let Some(tile)=tile{
                    if deps.len()==cap{return Err(bad("selection graph has excess supplier finishes"));}
                    deps.push(SelectionDependencyV1{tile,role:*role,logical:node.id_v3(),absolute:self.graph.absolute_packet_id_v3(node.id_v3())});
                }
            }
        }
        deps.sort_unstable_by_key(|d|(d.tile,d.role));
        if deps.len()!=cap||deps.iter().enumerate().any(|(i,d)|{
            let group=i as u64/roles;d.role!=i as u64%roles||d.tile!=if group==0{None}else{Some(group-1)}
        }){return Err(bad("selection graph lacks exact FullIds/all-row-tile supplier set"));}
        let p=self.frame.plan_v1();
        let logical=hash(b"misaka-palw/selection-duty-logical/v1",&(self.graph.scope_v3(),p.original_claim,self.source.source_id_v1(),target.position,target.beat)).map_err(E::Decode)?;
        let absolute=hash(b"misaka-palw/selection-duty-absolute/v1",&(self.graph.absolute_scope_v3(),logical)).map_err(E::Decode)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(NativeSelectionDutyV1{claim:p.original_claim,source:self.source.source_id_v1(),profile:self.profile,
            allocation:p.allocation_id,resource:p.resource_plan_id,accepted:p.terms.accepted_daa,target,logical,absolute,deps},lease))
    }
}
#[derive(Debug,PartialEq,Eq)]
pub(crate) struct SelectionDependencyV1{pub tile:Option<u64>,pub role:u64,pub logical:Hash64,pub absolute:[u8;64]}
pub(crate) struct NativeSelectionDutyV1{claim:Hash64,source:Hash64,profile:Hash64,allocation:Hash64,resource:Hash64,accepted:u64,
    target:SelectionTargetV1,logical:Hash64,absolute:Hash64,deps:Vec<SelectionDependencyV1>}
impl NativeSelectionDutyV1{
    pub(crate) fn target_v1(&self)->SelectionTargetV1{self.target}
    pub(crate) fn logical_id_v1(&self)->Hash64{self.logical}
    pub(crate) fn absolute_id_v1(&self)->Hash64{self.absolute}
    pub(crate) fn dependencies_v1(&self)->&[SelectionDependencyV1]{&self.deps}
    pub(crate) fn runtime_guard_v1(&self)->SelectionRuntimeGuardV1{SelectionRuntimeGuardV1::ActualFullIdsAndExactCommittedAndBeatTiles}
    fn check_current(&self,frame:&Frame,source:&Source,profile:Hash64)->Result<(),String>{
        let p=frame.plan_v1();if (self.claim,self.source,self.profile,self.allocation,self.resource,self.accepted)
            !=(p.original_claim,source.source_id_v1(),profile,p.allocation_id,p.resource_plan_id,p.terms.accepted_daa)
            ||self.target.position>=source.decode_count_v1()||self.target.beat>=source.vocab_v1(){return Err("selection duty current source/frame/domain mismatch".into());}Ok(())
    }
}
#[derive(Clone,borsh::BorshSerialize)]
pub(crate) struct SignedSelectionIntentV1{pub version:u16,pub claim:Hash64,pub scope_id:Hash64,pub policy_id:Hash64,pub duty_id:Hash64,
    pub position:u32,pub beat:u32,pub material_id:Hash64,pub filer:PalwBondKeyV2,pub signature:Vec<u8>}
impl SignedSelectionIntentV1{
    pub(crate) fn signing_message(&self,network:Hash64)->Result<Hash64,String>{
        if self.version!=1{return Err("selection intent version".into());}
        hash(b"misaka-palw/shared-selection-intent/v1",&(network,self.version,self.claim,self.scope_id,self.policy_id,self.duty_id,self.position,self.beat,self.material_id,self.filer))
    }
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum SelectionDecisionV1{PredicateHolds{claim:Hash64,proof:Hash64},Fault{claim:Hash64,execution:Hash64,proof:Hash64,position:u64}}
pub(crate) struct SelectionEvidenceV1<'a,'s>{intent:SignedSelectionIntentV1,duty:&'a NativeRetainedV1<NativeSelectionDutyV1>,_store:&'a Store<'s>,decision:SelectionDecisionV1}
impl SelectionEvidenceV1<'_,'_>{
    pub(crate) fn recheck(&self,state:&PalwChainStateV2,params:&PalwStateParamsV2,rules:&NativeCourtRulesV1)->Result<SelectionDecisionV1,String>{
        let actual=self._store.replay_selection_terminal_v1(state,params,rules,&self.intent,self.duty)?;
        if actual!=self.decision{return Err("selection evidence reconstruction mismatch".into());}Ok(actual)
    }
}
impl<'s> Store<'s>{
    /// row_pin clones actual IDs/tile/opening payloads; registered binding fill,
    /// V4 config, objective proof clone and all source/signature/evidence scratch
    /// remain separately caller-admitted. Duty owner is not their paid permit.
    pub(crate) fn selection_material_id_v1(&self,position:u32,beat:u32)->Result<Hash64,String>{
        let pin=self.row_pin(position,beat).ok_or("selection exact source-affine row/tile/FullIds missing (invalid committed ID belongs to Output)")?;
        hash(b"misaka-palw/selection-material/v1",&(self.source.source_id_v1(),pin))
    }
    pub(crate) fn build_selection_terminal_v1<'a>(&'a self,state:&PalwChainStateV2,params:&PalwStateParamsV2,rules:&NativeCourtRulesV1,
        intent:&SignedSelectionIntentV1,duty:&'a NativeRetainedV1<NativeSelectionDutyV1>)->Result<SelectionEvidenceV1<'a,'s>,String> where 's:'a{
        let decision=self.replay_selection_terminal_v1(state,params,rules,intent,duty)?;
        let evidence=SelectionEvidenceV1{intent:intent.clone(),duty,_store:self,decision};evidence.recheck(state,params,rules)?;Ok(evidence)
    }
    fn replay_selection_terminal_v1(&self,state:&PalwChainStateV2,params:&PalwStateParamsV2,rules:&NativeCourtRulesV1,
        intent:&SignedSelectionIntentV1,duty:&NativeRetainedV1<NativeSelectionDutyV1>)->Result<SelectionDecisionV1,String>{
        let c=crate::palw_court_shared_tir_state_v1::context(state,intent.claim)?;let source=c.source(state)?;
        let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,intent.claim).ok_or("selection current original absent")?;
        duty.check_current(frame,&source,c.profile_id)?;
        if intent.scope_id!=c.scope_id||self.source.source_id_v1()!=source.source_id_v1()||!rules.tir_active
            ||rules.at.daa_score<c.original_accepted_daa||rules.at.daa_score>=c.hard_daa
            ||rules.step_ladder!=frame.plan_v1().owner_work.rules.max_step_leaf_count
            ||intent.policy_id!=crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules)?
            ||intent.duty_id!=duty.absolute_id_v1()||(SelectionTargetV1{position:intent.position,beat:intent.beat})!=duty.target_v1(){
            return Err("selection current original/scope/policy/clock/target mismatch".into());
        }
        let original=frame.original_v1();let rule=Rule::of_job_v1(&original.job).ok_or("selection excludes Constraint and tails")?;
        let bond=state.bond(&intent.filer).ok_or("selection current filer absent")?;
        if intent.signature.len()!=crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN||!matches!(bond.status,PalwBondStatusV2::Active)
            ||bond.collateral<params.min_collateral_sompi()||!crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(&bond.pubkey,
                intent.signing_message(rules.network_domain)?.as_byte_slice(),&intent.signature,SELECTION_INTENT_CONTEXT_V1){return Err("selection current filer authority/signature invalid".into());}
        let pin=self.row_pin(intent.position,intent.beat).ok_or("selection actual FullIds/committed/beat tile missing")?;
        if intent.material_id!=hash(b"misaka-palw/selection-material/v1",&(source.source_id_v1(),&pin))?{return Err("selection material identity mismatch".into());}
        let record=state.tir_class_v1(&original.job.class_id).ok_or("selection registered class absent")?;
        let layout=crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state,original.job.class_id).ok_or("selection native layout absent")?;
        let binding=source.header_v1().resolve_registered_layout_v1(record,layout,original.execution_root,rules.step_ladder)?;
        let mut proof=crate::palw_court_v2::PalwCourtVerdictProofV2::TirDecodeTokenTiled{binding:Box::new(binding),pin};proof.tir_strip_program_v1();
        let prompt=if rules.prompt_ids_merkle{crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1}else{crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat};
        match crate::palw_tir_objective_v1::adjudicate_original_decode_v1(state,intent.claim,&proof,&rules.court,rules.step_ladder,prompt,frame.plan_v1().terminal.maximum_witness_bytes,&rule)?{
            crate::palw_tir_objective_v1::PalwTirObjectiveOutcomeV1::PredicateHolds{claim,proof_id}=>Ok(SelectionDecisionV1::PredicateHolds{claim,proof:proof_id}),
            crate::palw_tir_objective_v1::PalwTirObjectiveOutcomeV1::Convicts(f)=>Ok(SelectionDecisionV1::Fault{claim:f.target_v1().claim_id,
                execution:f.target_v1().execution_root,proof:f.proof_id_v1(),position:f.disputed_leaf_v1()}),
        }
    }
}

impl NativeSelectionUniverseV1<'_>{
    pub(super) fn aggregate_binding_v1(&self)->super::original_aggregate::FamilyBindingV1{(self.frame.plan_v1().original_claim,self.source.source_id_v1(),self.graph.scope_v3(),self.graph.absolute_scope_v3(),self.profile)}
}
