//! Exact historical material for one actual live consumer. No retired WorkRef,
//! whole-Pending attachment or mutable-world projection is constructed here.
//! A source header aliases across consumers; placement in each actual record
//! material set supplies the consumer binding without duplicating source bodies.
use super::*;
use super::palw_claim_local_source_origin_v8::SourceOriginV8;
use super::palw_claim_local_legacy_terminal_v18 as owner;
use super::palw_claim_local_execution_cohort_archive_v15 as cohort;
use super::palw_claim_local_carrier_lineage_v9 as carrier;
use crate::palw_native_resource_v1::{NativeRetainedV1,NativeDemandLeaseV1,TirNativeRequestsV1,BTreeNodeEnvelopeV1,reserve_native_requests_v1};
use crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
use borsh::BorshSerialize;
fn bad(why:&str)->PalwStateV2Error{PalwStateV2Error::CarriageInconsistent(why.into())}
#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord,BorshSerialize)]
struct Subject{id:Hash64,block:BlockHash,daa:u64,blue:u64}
impl Subject{fn of(id:Hash64,c:&PalwClaimStateV2)->Self{Self{id,block:c.accepted_block,daa:c.accepted_daa,blue:c.accepted_blue_score}}}
fn emit<T:BorshSerialize>(v:&T,f:&mut impl FnMut(&[u8])->Result<(),PalwStateV2Error>)->Result<(),PalwStateV2Error>{
 let n=borsh::object_length(v).map_err(|_|bad("native historical material encoding"))?;let mut q=TirNativeRequestsV1::new();q.exact_v1::<u8>(n as u64).map_err(PalwStateV2Error::NativeResource)?;let _lease=reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;let mut raw=Vec::with_capacity(n);v.serialize(&mut raw).map_err(|_|bad("native historical material serialization"))?;f(&raw)
}
fn immutable_identity(subject:Subject,c:&PalwClaimStateV2)->Result<Hash64,PalwStateV2Error>{
 let lane=match c.source{PalwClaimSourceV2::Attempt=>(0u8,0u32),PalwClaimSourceV2::FreePrompt{quanta,..}=>(1u8,quanta)};
 crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/retired-consumer-subject/v1",&(subject,c.class_id,c.bond,c.trace_root,c.output_root,c.execution_root,c.job_identity,lane)).map_err(|_|bad("historical material source identity encoding"))
}
/// Data closure only, matching the existing exact-birth S1 compatibility read.
/// Native A5 is preferred. A different current raw-ID S1 birth cannot supply
/// either the original body or availability for this source instance.
fn closed_material_source_v26(s:&PalwChainStateV2,id:Hash64,original:&PalwClaimStateV2)->Result<Option<NativeRetainedV1<super::palw_claim_local_compact_sources_v1::PalwClosedClaimSourceV1>>,PalwStateV2Error>{
 let subject=Subject::of(id,original);
 let source=if let Some(row)=super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(s,id,original)?{Some(row)}else{super::palw_claim_local_compact_sources_v1::read(s,id)?.filter(|row|Subject::of(id,&row.original)==subject)};
 if let Some(row)=&source{if immutable_identity(subject,&row.original)?!=immutable_identity(subject,original)?{return Err(bad("historical closed source changed immutable source identity"))}}
 Ok(source)
}
struct Walk{seen:BTreeMap<Subject,Hash64>,queue:BTreeMap<Subject,NativeRetainedV1<PalwClaimStateV2>>,lease:Option<NativeDemandLeaseV1>}
impl Walk{
 fn new()->Result<Self,PalwStateV2Error>{Ok(Self{seen:BTreeMap::new(),queue:BTreeMap::new(),lease:reserve_native_requests_v1(0).map_err(PalwStateV2Error::NativeResource)?})}
 fn insert(&mut self,id:Hash64,c:&PalwClaimStateV2)->Result<(),PalwStateV2Error>{
  let key=Subject::of(id,c);let identity=immutable_identity(key,c)?;
  if let Some(old)=self.seen.get(&key){if *old!=identity{return Err(bad("historical material revisited another immutable source at one native birth"))}return Ok(())}
  let mut q=TirNativeRequestsV1::new();q.exact_v1::<BTreeNodeEnvelopeV1<Subject,Hash64>>(1).map_err(PalwStateV2Error::NativeResource)?;q.exact_v1::<BTreeNodeEnvelopeV1<Subject,NativeRetainedV1<PalwClaimStateV2>>>(1).map_err(PalwStateV2Error::NativeResource)?;
  if let Some(lease)=&mut self.lease{lease.grow_memory_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;}
  let row=NativeRetainedV1::try_from_clone_v1(c).map_err(PalwStateV2Error::NativeResource)?;self.seen.insert(key,identity);self.queue.insert(key,row);Ok(())
 }
 fn stored(&mut self,s:&PalwChainStateV2,p:&PalwStateParamsV2,id:Hash64,block:BlockHash,daa:u64,blue:u64)->Result<bool,PalwStateV2Error>{
  if let Some(source)=super::palw_claim_local_source_admission_v8::for_stored_birth_v24(s,id,block,daa,blue)?{self.insert(id,source.original_claim())?;return Ok(true)}
  if let Some(current)=s.claims.get(&id).filter(|c|c.accepted_block==block&&c.accepted_daa==daa&&c.accepted_blue_score==blue){if let Some(origin)=SourceOriginV8::for_claim(s,p,id,current)?{self.insert(id,origin.original_claim())?;return Ok(true)}}
  if owner::with_pending_stored_birth_v24(s,p,id,block,daa,blue,|source|self.insert(id,source.original_v18()))?.is_some(){return Ok(true)}
  if let Some(closed)=super::palw_claim_local_exact_closed_source_v5::read_original_coordinates_v24(s,id,block,daa,blue)?{self.insert(id,&closed.original)?;return Ok(true)}
  if let Some(closed)=super::palw_claim_local_compact_sources_v1::read(s,id)?.filter(|row|row.original.accepted_block==block&&row.original.accepted_daa==daa&&row.original.accepted_blue_score==blue){self.insert(id,&closed.original)?;return Ok(true)}Ok(false)
 }
}
/// Emits material only for the actual consumer's own immutable birth scope and
/// transitively named same-root/prefix/carrier sources. Missing provenance is
/// returned explicitly; Hold/Denied are preserved by the existing readiness
/// consumer. The caller must not turn `false` into empty independent coverage.
pub(super) fn visit_consumer_material_v24(s:&PalwChainStateV2,p:&PalwStateParamsV2,id:Hash64,current:&PalwClaimStateV2,mut f:impl FnMut(&[u8])->Result<(),PalwStateV2Error>)->Result<bool,PalwStateV2Error>{
 let Some(initial)=SourceOriginV8::for_claim(s,p,id,current)?else{return Ok(false)};
 let mut walk=Walk::new()?;walk.insert(id,initial.original_claim())?;drop(initial);
 visit_seeded_material_v26(s,p,walk,f)
}
/// Shared walk over owner-authenticated EXISTING source originals. A quote can
/// seed a real parent cohort without projecting a not-yet-accepted Claim.
fn visit_seeded_material_v26(s:&PalwChainStateV2,p:&PalwStateParamsV2,mut walk:Walk,mut f:impl FnMut(&[u8])->Result<(),PalwStateV2Error>)->Result<bool,PalwStateV2Error>{
 let mut complete=true;
 while let Some((subject,original))=walk.queue.pop_first(){
  emit(&(b"misaka-palw/retired-consumer-material/v1".as_slice(),&subject),&mut f)?;
  let pending=owner::with_pending_checkpoint_source_v18(s,p,subject.id,&original,|source|source.visit_owned_material_v18(s,&mut f))?.is_some();
  if !pending&&let Some(closed)=closed_material_source_v26(s,subject.id,&original)?{
   emit(&*closed,&mut f)?;continue
  }
  let Some(origin)=SourceOriginV8::for_claim(s,p,subject.id,&original)?else{
   if let Some(closed)=closed_material_source_v26(s,subject.id,&original)?{emit(&*closed,&mut f)?;continue}
   complete=false;continue
  };
  if !pending{
   if origin.scoped_admission_v13().is_some(){super::palw_claim_local_source_admission_v8::visit_checkpoint_material_v10(s,subject.id,&original,&mut f)?;}
   else if !origin.visit_native_legacy_material_v24(&mut f)?{return Err(bad("related immutable native source material unavailable"))}
  }
  // This source's immutable birth cohort, never the current global population.
  if original.execution_root!=Hash64::default(){
   let mut captured=owner::visit_cohort_members_v18(s,p,subject.id,&original,|peer,c|{walk.insert(peer,c)?;Ok(std::ops::ControlFlow::Continue(()))})?;
   if !captured{captured=cohort::visit_snapshot_members_until_v15(s,subject.id,&original,|peer,c|{walk.insert(peer,c)?;Ok(std::ops::ControlFlow::Continue(()))})?;}
   if !captured{captured=super::palw_claim_local_execution_cohort_census_v15::visit_birth_eligible_members_v15(s,original.execution_root,origin.accepting_point(),|peer,c|walk.insert(peer,c))?;}
   if !captured{complete=false;}
  }
  if let Some(scoped)=origin.scoped_admission_v13(){scoped.visit_dependencies(|source,block,daa,blue|{if source!=subject.id&&!walk.stored(s,p,source,block,daa,blue)?{complete=false;}Ok(())})?;}
  if let Some(used)=origin.prefix_use()?{let source=used.intent.source;if source.source_claim!=subject.id&&!walk.stored(s,p,source.source_claim,source.accepted_block,source.accepted_daa,source.accepted_blue_score)?{complete=false;}}
  let point=origin.accepting_point();let mut visit=|fact:&carrier::CarrierFactV9,source:carrier::CarrierSourceViewV9|{
   if fact.consumer()!=subject.id||fact.consumer_point()!=point||fact.source()!=source.source()||fact.source_point()!=source.accepting_point()||fact.spent_txid()!=source.winning_txid()||source.accepting_point().blue_score>=point.blue_score||source.accepting_point().daa_score>point.daa_score{return Err(bad("historical material carrier differs from actual causal source"))}
   emit(fact,&mut f)?;source.visit_native_material_v24(&mut f)?;
   if let Some(c)=source.original_claim(){walk.insert(source.source(),c)?;}else if let Some(closed)=source.closed_source_proof(){walk.insert(source.source(),&closed.original)?;}
   // An irreversible native compact Summary has no unresolved original duty.
   // It is represented above by its actual authenticated Summary body.
   Ok(())
  };
  if !carrier::visit_consumer_facts_v9(s,subject.id,point,&mut visit)?&&!origin.visit_legacy_carrier_facts_v9(s,p,&mut visit)?{complete=false;}
 }Ok(complete)
}
/// Exact normal constructor predicate for whether the Chain vertex retains
/// material. This is shared by the independent targeted capacity pass; it does
/// not mint a selected approval or change recorded readiness.
pub(super) fn consumer_material_open_v24(s:&PalwChainStateV2,p:&PalwStateParamsV2,now:u64,id:Hash64,c:&PalwClaimStateV2)->Result<bool,PalwStateV2Error>{
 match c.phase{
  PalwClaimPhaseV2::Final{..}=>Ok(super::palw_claim_local_rewards_v1::recorded_reward_readiness_v1(s,p,now,id,c,false)?!=super::palw_claim_local_rewards_v1::RewardReadinessV1::Ready),
  PalwClaimPhaseV2::Voided{..}=>Ok(s.unresolved.contains(&(c.accepted_blue_score,id))||s.court_sessions.values().any(|session|session.claim==id)),
  _=>Ok(true),
 }
}

/// Canonical fixed header grammar only; no actual/future native birth asserted.
pub(super) fn binding_header_bytes_v24()->Result<u64,PalwStateV2Error>{
 let zero=Hash64::default();let subject=(&zero,&zero,0u64,0u64);
 borsh::object_length(&(b"misaka-palw/retired-consumer-material/v1".as_slice(),&subject)).map(|n|n as u64).map_err(|_|bad("historical binding header shape"))
}

/// Ephemeral complete-source availability, minted only from the actual owning
/// builder and its sealed executing point. This has no payment/credit method;
/// the immutable borrow prevents changing the captured state behind the seal.
pub(super) struct NativeConsumerMaterialCoverageV24<'a>{state:&'a PalwChainStateV2,subject:Subject,executing:PalwBlockContextV2}
impl NativeConsumerMaterialCoverageV24<'_>{
 pub(super) fn covers_claim_v24(&self,ctx:&PalwBlockContextV2,id:Hash64,current:&PalwClaimStateV2)->bool{self.executing==*ctx&&self.subject==Subject::of(id,current)&&self.state.claims.get(&id)==Some(current)}
}
pub(super) fn prepare_consumer_material_coverage_v24<'a>(b:&'a TransitionBuilder<'_>,ctx:&PalwBlockContextV2,id:Hash64,current:&PalwClaimStateV2)->Result<Option<NativeConsumerMaterialCoverageV24<'a>>,PalwStateV2Error>{
 if b.claim_local_lock_point_v6!=Some(*ctx)||b.state.claims.get(&id)!=Some(current){return Err(bad("consumer material coverage lacks actual owning point/row"))}
 if !visit_consumer_material_v24(&b.state,b.params,id,current,|_|Ok(()))?{return Ok(None)}
 Ok(Some(NativeConsumerMaterialCoverageV24{state:&b.state,subject:Subject::of(id,current),executing:*ctx}))
}

/// Parent-only transitive availability, never payment, accepted-source standing,
/// selected approval or final no-F1 authority. The actual new birth and any
/// same-block peers are deliberately absent and rechecked after real capture.
pub(super) struct NativeParentCohortMaterialV26<'a>{parent:&'a PalwChainStateV2,execution_root:Hash64,candidate:PalwBlockContextV2}
impl NativeParentCohortMaterialV26<'_>{
 pub(super) fn execution_root_v26(&self)->Hash64{self.execution_root}
 pub(super) fn parent_point_v26(&self)->Option<PalwBlockContextV2>{self.parent.last_point}
 pub(super) fn candidate_point_v26(&self)->PalwBlockContextV2{self.candidate}
}
pub(super) fn preflight_parent_cohort_material_v26<'a>(parent:&'a PalwChainStateV2,params:&PalwStateParamsV2,ctx:&PalwBlockContextV2,execution_root:Hash64)->Result<Option<NativeParentCohortMaterialV26<'a>>,PalwStateV2Error>{
 let Some(_native)=super::palw_claim_local_native_cohort_baseline_v22::preflight_native_cohort_context_v22(parent,params,ctx,execution_root)?else{return Ok(None)};
 let mut walk=Walk::new()?;
 if !super::palw_claim_local_execution_cohort_census_v15::visit_birth_eligible_members_v15(parent,execution_root,*ctx,|id,original|walk.insert(id,original))?{return Ok(None)}
 if !visit_seeded_material_v26(parent,params,walk,|_|Ok(()))?{return Ok(None)}
 Ok(Some(NativeParentCohortMaterialV26{parent,execution_root,candidate:*ctx}))
}
