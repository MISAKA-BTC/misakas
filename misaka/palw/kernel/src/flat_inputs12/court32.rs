//! Separate V2 ordinary court and independently derived state-continuity32
//! bound. Frozen25's source wiring and court identifier are not enlarged.
use super::{composition32::CheckedStageInvocationV32,bounded28::CheckedStageClassV28};
use crate::{element::PreparedV1,hash::{Digest,object_id},flat_point25::{FlatPointFaultV25,PointContextV25,PointRelationScopeV25},flat_court5::FlatSegContextV5,verify::DismissalV1};
use borsh::BorshSerialize;
pub fn ordinary_stage_court_id_v32()->Digest{object_id(b"misaka-palw/kernel/canonical-V2-state-point-court/v32",&(32u16,crate::flat_point25::ordinary_point_court_id_v25(),b"canonicalV2-realPC/positioninputPC/master-token;postWriter-next-position;bound-originalmax;no-old25-lift"))}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct StageCourtScopeV32 {pub version:u16,pub source:Digest,pub program:Digest,pub maximum_positions:u32,pub original_scope25:Digest,pub court:Digest,pub relations:Vec<PointRelationScopeV25>,pub maximum_bytes:u64,pub maximum_work:u64,pub maximum_scratch:u64,pub derivation_work:u64}
pub struct CheckedStageCourtScopeV32 {scope:StageCourtScopeV32}
impl CheckedStageCourtScopeV32 {
    pub fn root(&self)->Digest{object_id(b"misaka-palw/kernel/original-V2-complete-court-scope/v32",&self.scope)}
    pub fn scope(&self)->&StageCourtScopeV32{&self.scope}
    pub fn paid_court_vector(&self)->(u64,u64,u64,u16){(self.scope.maximum_bytes,self.scope.maximum_work,self.scope.maximum_scratch,self.scope.maximum_bytes.div_ceil(32<<10) as u16)}
}
pub fn derive_stage_court_scope_v32(class:&CheckedStageClassV28)->Result<CheckedStageCourtScopeV32,String>{
    let source=class.source();let actual=class.template().program.clone();let view=actual.v1_view();let pr=PreparedV1::without_stage_v32(&view,&actual)?;
    let positions=class.wire().max_positions;if positions==0||positions>source.program().history_bound{return Err("stage32 independently declared maximum context differs".into());}
    let mut relations=Vec::new();let mut visits=0u64;let(mut bytes,mut work,mut scratch)=(class.maximum_scope().scope().worst_court_bytes,class.maximum_scope().scope().worst_court_work,class.maximum_scope().scope().worst_court_scratch_bytes);
    for(occ,(block,_))in pr.w.occurrences.iter().enumerate(){for(node,n)in view.blocks[*block as usize].nodes.iter().enumerate(){if matches!(n.prim,misaka_palw_tir::Prim::TopK{..}){
        for q in crate::flat_scope25::grammar_boundary_positions(&pr,positions,occ as u16,node as u16){if crate::flat_scope7::point_relation(&pr,q,occ as u16,node as u16).map_err(|e|format!("{e:?}"))?{let r=crate::flat_topk7::relation_scope_v7(&view,&pr,positions,q,occ as u16,node as u16,true).map_err(|e|format!("{e:?}"))?;bytes=bytes.max(r.filing_bytes);work=work.max(r.court_work);scratch=scratch.max(r.court_scratch_bytes);}}continue;
    }for q in crate::flat_scope25::grammar_boundary_positions(&pr,positions,occ as u16,node as u16){visits=visits.checked_add(1).ok_or("stage32 complete scope visits overflow")?;if visits>1<<20{return Err("stage32 full state-court metadata exceeds fixed derivation cap".into());}let r=crate::flat_point25::relation_priced_v25(&view,positions,&pr,q,occ as u16,node as u16,view.encode().len() as u64).map_err(|e|format!("{e:?}"))?;bytes=bytes.max(r.filing_bytes.checked_add(134).ok_or("stage32 court wrapper overflow")?);work=work.max(r.work.checked_add(134*128).ok_or("stage32 wrapper work overflow")?);scratch=scratch.max(r.scratch_bytes.checked_add(134).ok_or("stage32 wrapper scratch overflow")?);relations.push(r);}}}
    if bytes>16<<20||work>1<<34||scratch>256<<20{return Err("stage32 complete V2 state court exceeds explicit native caps".into());}
    let derivation_work=(view.encode().len() as u64).checked_add(visits.checked_mul(4096).ok_or("stage32 metadata work overflow")?).ok_or("stage32 complete metadata overflow")?;
    Ok(CheckedStageCourtScopeV32{scope:StageCourtScopeV32{version:32,source:source.id(),program:object_id(b"misaka-palw/kernel/actual-V2-court-program/v32",&actual),maximum_positions:positions,original_scope25:class.maximum_scope().root(),court:ordinary_stage_court_id_v32(),relations,maximum_bytes:bytes,maximum_work:work,maximum_scratch:scratch,derivation_work}})
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct StagePointFaultV32 {pub version:u16,pub invocation:Digest,pub input_binding:Digest,pub point25:Vec<u8>}
impl StagePointFaultV32 {
    pub fn from_bytes(raw:&[u8])->Result<Self,String>{
        if raw.len()<134||raw.len()>16<<20||raw[..2]!=32u16.to_le_bytes(){return Err("stage32 bounded point grammar".into());}let n=u32::from_le_bytes(raw[130..134].try_into().unwrap()) as usize;if raw.len()!=134+n||n>crate::flat_point25::POINT_FAULT_MAX_BYTES_V25 as usize{return Err("stage32 exact point nested extent".into());}crate::flat_point25::parse_flat_point_fault_v25(&raw[134..]).map_err(|e|format!("{e:?}"))?;Ok(Self{version:32,invocation:raw[2..66].try_into().unwrap(),input_binding:raw[66..130].try_into().unwrap(),point25:raw[134..].to_vec()})
    }
}
/// Pure fault verifier. Core supplies its authenticated original invocation,
/// source/claim/own-seal and paid complete32 bound before any body decoding.
pub fn verify_stage_point_fault_v32(invocation:&CheckedStageInvocationV32<'_>,maximum:&CheckedStageCourtScopeV32,base:FlatSegContextV5<'_>,f:&StagePointFaultV32)->Result<(),DismissalV1>{
    let bad=|s:&str|DismissalV1::NotAuthentic(s.into());
    if f.version!=32||f.invocation!=invocation.invocation_root()||f.input_binding!=invocation.input_binding_root()||maximum.scope.source!=invocation.source().id()||maximum.scope.court!=ordinary_stage_court_id_v32()||invocation.positions()>maximum.scope.maximum_positions||base.claim.positions!=invocation.positions()||base.claim.program!=&invocation.actual_program().v1_view()||base.claim.encoder.is_some(){return Err(bad("stage32 court replaces original complete invocation/source/program/actual positions"));}
    let inner:FlatPointFaultV25=crate::flat_point25::parse_flat_point_fault_v25(&f.point25).map_err(|e|bad(&format!("{e:?}")))?;let position=inner.output.node.as_ref().ok_or_else(||bad("stage32 authenticated output coordinate absent"))?.position;
    let pc=invocation.combined_parameters_at(position).map_err(|e|bad(&e))?;if base.claim.params!=&pc{return Err(bad("stage32 court input PC is not the original position-specific Master binding"));}
    let pr=PreparedV1::without_stage_v32(base.claim.program,invocation.actual_program()).map_err(|e|bad(&e))?;
    crate::flat_point25::verify_point_prepared_v25(&PointContextV25{court_id:crate::flat_point25::ordinary_point_court_id_v25(),base},&inner,&pr)
}


/// Frozen special-court grammar under NEW, independently authenticated V2
/// wiring. Existing v5/v7 entry points still prepare their original V1 wiring.
pub fn verify_stage_special_fault_v32(invocation:&CheckedStageInvocationV32<'_>,maximum:&CheckedStageCourtScopeV32,base:FlatSegContextV5<'_>,raw:&[u8])->Result<(),DismissalV1>{
    let bad=|s:&str|DismissalV1::NotAuthentic(s.into());
    if maximum.scope.source!=invocation.source().id()||maximum.scope.court!=ordinary_stage_court_id_v32()||invocation.positions()>maximum.scope.maximum_positions||base.claim.positions!=invocation.positions()||base.claim.program!=&invocation.actual_program().v1_view()||base.claim.encoder.is_some()||raw.len() as u64>maximum.scope.maximum_bytes{return Err(bad("stage32 special court changes original complete V2 binding"));}
    let pr=PreparedV1::without_stage_v32(base.claim.program,invocation.actual_program()).map_err(|s|bad(&s))?;
    match raw.get(..2).map(|v|u16::from_le_bytes(v.try_into().unwrap())) {
        Some(7)=>{let f=crate::flat_topk7::TopKFaultV7::from_bytes(raw).map_err(|e|bad(&e.to_string()))?;let q=f.output.node.as_ref().ok_or_else(||bad("stage32 TopK output absent"))?.position;let pc=invocation.combined_parameters_at(q).map_err(|e|bad(&e))?;if base.claim.params!=&pc{return Err(bad("stage32 TopK actual-position input PC differs"));}crate::flat_topk7::verify_topk_prepared_v32(&crate::flat_topk7::TopKContextV7{court_id:crate::flat_topk7::topk_court_id_v7(),base},&f,&pr)},
        Some(5)=>{let f=crate::flat_court5::FlatFaultV5::from_bytes(raw).map_err(|e|bad(&e.to_string()))?;
            // All scalar ordinary/whole state relations must use the new32
            // dependency-pricing path. Legacy is only Decode or type/header.
            if !matches!(f,crate::flat_court5::FlatFaultV5::Decode(_)|crate::flat_court5::FlatFaultV5::Malformed(_)){return Err(bad("stage32 ordinary relation requires original V2 point32 court"));}
            if matches!(f,crate::flat_court5::FlatFaultV5::Decode(_))&&!matches!(invocation.actual_program().output,misaka_palw_tir::program_v2::OutputDecl::Logits{..}){return Err(bad("stage32 Rows/Final has no decoder relation"));}
            crate::flat_court5::verify_flat_fault_prepared_v32(&base,&f,&pr).map(|_|())},
        _=>Err(bad("stage32 unknown special court grammar")),
    }
}
