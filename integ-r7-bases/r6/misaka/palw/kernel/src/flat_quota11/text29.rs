//! Separate original text29 maximum admission over the shared21-axis algebra.
//! Frozen class9/book11 factories cannot manufacture a new29 authority.
use super::*;
use crate::{flat_resources29::{FlatResourceVectorV29,derive_flat_resources_v29},flat_scope25::CheckedResourceScopeV25,flat_execution11::{canonical_nonce_v11,native_requester_nonce_counter_root_v11}};
use misaka_palw_tir::program::TirProgramV1;
pub struct CheckedTextQuotaV29 {class:Digest,source:Digest,policy:Digest,maximum:Digest,plan:CheckedJobQuotaPlanV11,requests:u32,restore_bytes:u64}
impl CheckedTextQuotaV29 {
    pub fn root(&self)->Digest {object_id(b"misaka-palw/kernel/text-original-maximum-quota/v29",&(29u16,self.class,self.source,self.policy,self.maximum,self.plan.record().root(),self.requests,self.restore_bytes))}
    pub fn plan(&self)->&CheckedJobQuotaPlanV11 {&self.plan}
    pub fn requests_per_owner(&self)->u32 {self.requests}
    pub fn restore_bytes(&self)->u64 {self.restore_bytes}
    pub fn class(&self)->Digest {self.class}
    pub fn source(&self)->Digest {self.source}
    pub fn source_plan(&self,program:&TirProgramV1,pc:&ParamCommitmentsV1,quota:&QuotaPolicyV11,discovery:&discovery13::DiscoveryPolicyV13)->Result<source16::SourcePlanV16,String>{
        discovery.check(quota)?;
        if crate::public::program_root_v1(&program.encode())!=self.plan.program_root||pc.root()!=self.plan.parameter_root{return Err("text29 source changes original real program/PC".into());}
        let requests=self.requests.checked_mul(4).and_then(|n|n.checked_add(discovery.ordinary_lifetime_requests)).ok_or("text29 source request overflow")?;
        source16::derive_source_plan_from_text_v29(self,pc,quota,requests)
    }
}
/// Every caller vector and checked scope is independently rederived. Class
/// maximum and original requested maximum are separate immutable declarations.
pub fn derive_text_quota_v29(program:&TirProgramV1,pc:&ParamCommitmentsV1,class_scope:&CheckedResourceScopeV25,class_resource:&FlatResourceVectorV29,job_scope:&CheckedResourceScopeV25,job_resource:&FlatResourceVectorV29,class:Digest,source:Digest,policy:Digest,restore_bytes:u64)->Result<CheckedTextQuotaV29,String>{
    if [class,source,policy].contains(&[0;64])||restore_bytes==0||restore_bytes>32<<20||job_scope.max_positions()>class_scope.max_positions(){return Err("text29 original declared maximum/source/restore absent".into());}
    let max=derive_flat_resources_v29(program,pc,class_scope.max_positions(),None)?;let job=derive_flat_resources_v29(program,pc,job_scope.max_positions(),None)?;
    if max.court!=*class_scope.scope()||max.vector!=*class_resource||job.court!=*job_scope.scope()||job.vector!=*job_resource{return Err("text29 class/request maximum scope/resources differ from independent original geometry".into());}
    let source_bound=crate::flat_execution11::native_source_court_bound_v11(program.encode().len(),pc)?;
    let filing=class_scope.scope().worst_court_bytes.max(source_bound.max_proof_bytes).checked_add(4096).ok_or("text29 complete filing overflow")?;
    let work=class_scope.scope().worst_court_work.max(source_bound.work);let scratch=class_scope.scope().worst_court_scratch_bytes.max(source_bound.scratch_bytes);
    if filing>16<<20||work>1<<34||scratch>256<<20{return Err("text29 original full court25 exceeds independently versioned caps".into());}
    let restore_work=restore_bytes.checked_mul(restore_bytes).and_then(|n|n.checked_mul(64)).and_then(|n|n.checked_add(restore_bytes.checked_mul(1024)?)).ok_or("text29 full original restore work overflow")?;
    let pr=crate::element::PreparedV1::without_scope(program,None)?;let post=(job_scope.max_positions() as u64).checked_mul(crate::element::node_opening_bytes_v1(pr.node_count)).and_then(|n|n.checked_add(job_scope.max_positions() as u64*4+crate::seg::segment_count_v1(job_scope.max_positions()) as u64*64+16384)).ok_or("text29 maximum Commit carrier overflow")?;
    let full=filing.checked_add(restore_bytes).and_then(|n|n.checked_add(post)).ok_or("text29 complete native court extent overflow")?;
    let mut actions=[action_envelope(restore_work as u128+job.vector.quantities.get(ResourceAxisV10::MetadataWork),post.max(restore_bytes),None)?,action_envelope(restore_work as u128,restore_bytes+16384,None)?,action_envelope(restore_work as u128,restore_bytes+(128<<10),None)?,action_envelope(restore_work as u128,full,Some((work,filing,scratch)))?,action_envelope(restore_work as u128,full,Some((work,filing,scratch)))?,action_envelope(restore_work as u128,restore_bytes,None)?];
    for a in &mut actions{include_native_post_io_v11(a,post)?;include_source_court_io_v11(a,pc)?;}
    let requests=complete_public_callback_bound_v29(program,class_scope,pc)?;
    let plan=CheckedJobQuotaPlanV11{program_root:crate::public::program_root_v1(&program.encode()),parameter_root:pc.root(),resource_root:job.vector.root(),class_scope_root:class_scope.root(),actual_scope_root:job_scope.root(),native_restore_work:restore_work,native_post_carrier_cap:post,source_court_bound:source_bound,positions:job_scope.max_positions(),baseline:QuotaQuantitiesV11::from(&job.vector.quantities),action_envelopes:actions,action_carrier_caps:[post.max(restore_bytes),restore_bytes+16384,restore_bytes+(128<<10),full,full,restore_bytes]};
    Ok(CheckedTextQuotaV29{class,source,policy,maximum:job.vector.root(),plan,requests,restore_bytes})
}
/// Core reads these from actual original new29 class/max/book/coin/consent and
/// native principal89 counter rows. This type has no wire deserializer.
pub struct TextNativeFundingAnchorV29 {
    pub class:Digest,pub source:Digest,pub maximum:Digest,pub request:Digest,pub job:Digest,pub funding:Digest,pub consent:Digest,
    pub requester:Digest,pub producer:Digest,pub requester_operator:Digest,pub producer_operator:Digest,pub nonce:Digest,
    pub admitted:u64,pub deadline:u64,pub reward:u64,pub debit:u64,pub receipt:Digest,
}
fn binding(plan:&CheckedTextQuotaV29,a:&TextNativeFundingAnchorV29,policy:&QuotaPolicyV11,nonce:&NativeRequesterNonceWitnessV11,epoch:&TrustedEpochV11)->Result<SubjectBindingV11,String>{
    epoch.check(policy)?;let next=canonical_nonce_v11(&a.nonce)?.checked_add(1).ok_or("text29 nonce overflow")?;
    if a.class!=plan.class||a.source!=plan.source||a.maximum!=plan.maximum||a.admitted!=epoch.block_daa||a.deadline<=a.admitted||a.deadline>=epoch.interval_end(policy)?||a.reward==0||a.reward!=a.debit||a.requester==a.producer||a.requester_operator==a.producer_operator||nonce.requester!=a.requester||nonce.requester_operator_id!=a.requester_operator||nonce.next_nonce!=next||nonce.counter_root!=native_requester_nonce_counter_root_v11(&a.requester_operator,next)||[a.request,a.job,a.funding,a.consent,a.receipt,a.requester_operator,a.producer_operator].contains(&[0;64]){return Err("text29 quota admission lacks original new29 source/maximum/request/funding/consent/operators/actual coins/nonce/epoch".into());}
    Ok(SubjectBindingV11{version:11,policy_root:policy.root(),class_id:plan.class,class_scope9_root:plan.plan.class_scope_root,program_root:plan.plan.program_root,parameter_root:plan.plan.parameter_root,job_id:a.job,job_input_root:a.request,resource10_root:plan.plan.resource_root,quota_plan_root:plan.plan.record().root(),actual_positions:plan.plan.positions,requester:a.requester,producer:a.producer,admitted_interval:epoch.network_interval,deadline_daa:a.deadline})
}
pub fn check_text_quota_admission_v29(plan:&CheckedTextQuotaV29,a:&TextNativeFundingAnchorV29,policy:&QuotaPolicyV11,nonce:&NativeRequesterNonceWitnessV11,epoch:&TrustedEpochV11)->Result<CheckedQuotaAdmissionV11,String>{Ok(CheckedQuotaAdmissionV11{binding:binding(plan,a,policy,nonce,epoch)?,native_admission_epoch:Some((epoch.block_hash,epoch.block_daa))})}
pub(crate) struct CheckedTextSourceAdmissionV29 {binding:SubjectBindingV11,consent:Digest,source:Digest,operator:Digest,epoch:TrustedEpochV11}
impl CheckedTextSourceAdmissionV29 {pub(crate) fn into_parts(self)->(SubjectBindingV11,Digest,Digest,Digest,TrustedEpochV11){(self.binding,self.consent,self.source,self.operator,self.epoch)}}
pub fn check_text_source_admission_v29(plan:&CheckedTextQuotaV29,a:&TextNativeFundingAnchorV29,source:&source16::SourcePlanV16,policy:&QuotaPolicyV11,nonce:&NativeRequesterNonceWitnessV11,epoch:&TrustedEpochV11)->Result<source16::CheckedSourceAdmissionV16,String>{
    let binding=binding(plan,a,policy,nonce,epoch)?;
    if source.record().class!=plan.class||source.record().parameter!=plan.plan.parameter_root||source.record().job_plan!=plan.plan.record().root(){return Err("text29 original real-PC source contract differs".into());}
    source16::from_checked_text_admission_v29(CheckedTextSourceAdmissionV29{binding,consent:a.consent,source:source.root(),operator:a.producer_operator,epoch:epoch.clone()},source)
}

/// Successful public callbacks for one whole-claim localization, then one
/// terminal relation. Directory descent, flat localization, original PC
/// header/bisection and selected TopK rows are derived separately. Retried or
/// absent responses consume the separate twice-N attempt envelope.
pub(crate) fn complete_public_callback_bound_v29(program:&TirProgramV1,scope:&CheckedResourceScopeV25,pc:&ParamCommitmentsV1)->Result<u32,String>{
    let checked=crate::flat_scope25::derive_checked_flat_resource_scope_v25(program,scope.max_positions(),None).map_err(|e|format!("{e:?}"))?;
    if checked.root()!=scope.root(){return Err("callback scope differs from independent complete geometry".into());}
    let pr=crate::element::PreparedV1::without_scope(program,None)?;
    let log=|n:u64| if n<=1 {0u64} else {64-(n-1).leading_zeros() as u64};
    let directory=log(scope.max_positions().min(crate::seg::SEG_LEN_V4) as u64).checked_add(log(pr.node_count)).and_then(|n|n.checked_add(1)).ok_or("callback directory overflow")?;
    let mut real_pc_leaves=1u64;
    for d in &program.params {let shape=d.shape.iter().map(|n|*n as usize).collect::<Vec<_>>();let l=crate::merkle3::LayoutV3::try_of(&shape).ok_or("callback parameter geometry")?;real_pc_leaves=real_pc_leaves.max(l.leaves(0)).max(l.leaves(1));}
    if pc.by_instance.is_empty()&&!program.params.is_empty(){return Err("callback original parameter namespace absent".into());}
    let source=log(real_pc_leaves).checked_add(2).ok_or("callback PC header/leaf overflow")?;
    let mut flat=0u64;let mut topk=0u64;
    for (occ,(block,_)) in pr.w.occurrences.iter().enumerate(){for (node,n) in program.blocks[*block as usize].nodes.iter().enumerate(){for q in [0,scope.max_positions()-1]{
        let shape=pr.shapes(q,occ as u16,node as u16).out;
        let len=shape.iter().try_fold(1u64,|a,n|a.checked_mul(*n as u64)).ok_or("callback output size overflow")?;
        let values=if len<64 {len} else {let dims=shape.iter().map(|n|*n as u64).collect::<Vec<_>>();crate::merkle5::flat_material_bounds_v5(n.out.dtype,&dims).ok_or("callback flat geometry")?.localization_replies.checked_add(1).ok_or("callback flat reply overflow")?};
        flat=flat.max(values);
        if let misaka_palw_tir::Prim::TopK{k,..}=n.prim {if crate::flat_scope7::point_relation(&pr,q,occ as u16,node as u16).map_err(|e|format!("{e:?}"))?{topk=topk.max(crate::flat_topk_locator25::topk_relation_callback_bound_v25(&shape,k).ok_or("callback TopK selected-row overflow")?);}}
    }}}
    let operands=scope.scope().relations.iter().try_fold(0u64,|n,r|r.operand_leaves.iter().try_fold(0u64,|a,b|a.checked_add(*b).ok_or("callback operand count overflow")).map(|s|n.max(s)))?;
    // Each ordinary operand may need its node and typed header, original PC
    // header/path/leaf, and selected flat value. Include the three filing
    // refetches explicitly; TopK's bound already includes its own refetches.
    let ordinary=operands.checked_mul(source.checked_add(flat).and_then(|n|n.checked_add(2)).ok_or("callback per-operand overflow")?).and_then(|n|n.checked_add(flat)).and_then(|n|n.checked_add(3)).ok_or("callback ordinary relation overflow")?;
    let total=directory.checked_add(source).and_then(|n|n.checked_add(ordinary.max(topk))).ok_or("complete callback overflow")?;
    let total=u32::try_from(total).map_err(|_|"complete callback count exceeds u32")?;
    if total==0||total>1<<18{return Err("complete callback count exceeds independently versioned request cap".into());}Ok(total)
}

/// Readonly original funding binding for a newly paid independent owner. Core
/// authenticates the immutable original book and current principal counter;
/// a later owner's epoch must never replace the original admission interval.
pub(super) fn source_owner_binding_v29(plan:&CheckedTextQuotaV29,a:&TextNativeFundingAnchorV29,policy:&QuotaPolicyV11,nonce:&NativeRequesterNonceWitnessV11,epoch:&TrustedEpochV11)->Result<SubjectBindingV11,String>{
    epoch.check(policy)?;let next=canonical_nonce_v11(&a.nonce)?.checked_add(1).ok_or("text29 nonce overflow")?;
    if a.class!=plan.class||a.source!=plan.source||a.maximum!=plan.maximum||a.admitted>epoch.block_daa||a.admitted/policy.interval_daa!=epoch.network_interval||a.deadline<=epoch.block_daa||a.deadline>=epoch.interval_end(policy)?||a.reward==0||a.reward!=a.debit||a.requester==a.producer||a.requester_operator==a.producer_operator||nonce.requester!=a.requester||nonce.requester_operator_id!=a.requester_operator||nonce.next_nonce<next||nonce.counter_root!=native_requester_nonce_counter_root_v11(&a.requester_operator,nonce.next_nonce)||[a.request,a.job,a.funding,a.consent,a.receipt,a.requester_operator,a.producer_operator].contains(&[0;64]){return Err("text29 source owner changes authenticated original funding/maximum/nonce/interval".into());}
    Ok(SubjectBindingV11{version:11,policy_root:policy.root(),class_id:plan.class,class_scope9_root:plan.plan.class_scope_root,program_root:plan.plan.program_root,parameter_root:plan.plan.parameter_root,job_id:a.job,job_input_root:a.request,resource10_root:plan.plan.resource_root,quota_plan_root:plan.plan.record().root(),actual_positions:plan.plan.positions,requester:a.requester,producer:a.producer,admitted_interval:epoch.network_interval,deadline_daa:a.deadline})
}

/// Only a trusted Core adapter reads these values from the genuine original
/// new29 book, actual monetary settlement and permanent principal counter.
/// This record deliberately has no wire decoder or terminal flag enum.
pub struct TextNativeTerminalAnchorV29 {
    pub job:Digest,pub funding:Digest,pub terminal_book:Digest,pub request_key:Digest,
    pub nonce_witness:NativeRequesterNonceWitnessV11,pub producer_credit:u64,pub requester_credit:u64,
    pub actual_slash:u64,pub finder_credit:u64,pub escrow_debit:u64,pub proof:Digest,
    pub established:u64,pub closed_escrow:bool,
}
pub fn check_text_terminal_v29(plan:&CheckedTextQuotaV29,a:&TextNativeFundingAnchorV29,policy:&QuotaPolicyV11,n:&TextNativeTerminalAnchorV29,epoch:&TrustedEpochV11)->Result<CheckedQuotaTerminalV11,String>{
    epoch.check(policy)?;let nonce=&n.nonce_witness;
    if a.class!=plan.class||a.source!=plan.source||a.maximum!=plan.maximum||a.reward==0||a.reward!=a.debit||n.job!=a.job||n.funding!=a.funding||n.request_key!=a.request||[a.job,a.funding,a.request,a.consent,a.receipt,n.terminal_book].contains(&[0;64])||a.requester==a.producer||a.requester_operator==a.producer_operator||nonce.requester!=a.requester||nonce.requester_operator_id!=a.requester_operator||nonce.next_nonce<=canonical_nonce_v11(&a.nonce)?||nonce.counter_root!=native_requester_nonce_counter_root_v11(&a.requester_operator,nonce.next_nonce)||a.admitted>n.established||n.established>epoch.block_daa||a.admitted>epoch.block_daa||a.deadline<=a.admitted||!n.closed_escrow||n.escrow_debit!=a.reward||n.producer_credit.checked_add(n.requester_credit)!=Some(a.reward)||n.finder_credit>n.actual_slash{return Err("text29 terminal lacks exact original book/source/maxima/nonce/actual money conservation".into());}
    if n.producer_credit!=0&&(n.producer_credit!=a.reward||n.requester_credit!=0||n.actual_slash!=0||n.finder_credit!=0||n.proof==[0;64])||n.proof==[0;64]&&(n.producer_credit!=0||n.requester_credit!=a.reward||n.actual_slash!=0||n.finder_credit!=0||n.established<=a.deadline)||n.actual_slash!=0&&(n.producer_credit!=0||n.requester_credit!=a.reward||n.proof==[0;64]){return Err("text29 terminal outcome differs from original Final/refund/verified collateral settlement".into());}
    let binding=SubjectBindingV11{version:11,policy_root:policy.root(),class_id:plan.class,class_scope9_root:plan.plan.class_scope_root,program_root:plan.plan.program_root,parameter_root:plan.plan.parameter_root,job_id:a.job,job_input_root:a.request,resource10_root:plan.plan.resource_root,quota_plan_root:plan.plan.record().root(),actual_positions:plan.plan.positions,requester:a.requester,producer:a.producer,admitted_interval:a.admitted/policy.interval_daa,deadline_daa:a.deadline};
    Ok(CheckedQuotaTerminalV11{binding,funded_job_root:a.funding,terminal_book_root:n.terminal_book,request_key:a.request})
}
