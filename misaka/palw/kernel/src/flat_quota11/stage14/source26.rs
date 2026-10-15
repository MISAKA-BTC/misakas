//! New caller-owned ORIGINAL-PC acquisition. Frozen protected11's two request
//! slots and Trace17 role1's two Court restores are never enlarged implicitly.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StageSourcePlanRecordV26 {
    version: u16,
    class: Digest,
    source: Digest,
    real_parameters: Digest,
    inventory: Digest,
    admission: Digest,
    stage_quota: Digest,
    producer_source: Digest,
    trace_restore: Digest,
    quota_policy: Digest,
    execution_policy: Digest,
    requests: u32,
    attempts: u32,
    required_horizon: u64,
    request: QuotaQuantitiesV11,
    admission_cost: QuotaQuantitiesV11,
    guarantee: QuotaQuantitiesV11,
}
pub struct CheckedStageSourcePlanV26 {
    record: StageSourcePlanRecordV26,
}
pub struct NativeStageSourceOwnerV26 {
    pub job: Digest,
    pub book: Digest,
    pub consent18: Digest,
    pub seal: Digest,
    pub actor: Digest,
    pub actor_operator: Digest,
    pub producer_operator: Digest,
    pub original_paid_seal: Digest,
    pub quote: Digest,
    pub accepted_daa: u64,
    pub horizon_daa: u64,
    pub original_contract: Digest,
    pub original_debit: u64,
    pub debit_receipt: Digest,
}
pub struct CheckedStageSourceOwnerV26 {
    binding: SubjectBindingV11,
    seal: Digest,
    actor: Digest,
    operator: Digest,
    original: Digest,
    quote: Digest,
    epoch: TrustedEpochV11,
    horizon: u64,
}
impl CheckedStageSourceOwnerV26 {
    pub fn binding(&self) -> &SubjectBindingV11 {
        &self.binding
    }
    pub fn seal(&self) -> Digest {
        self.seal
    }
    pub fn actor(&self) -> Digest {
        self.actor
    }
    pub fn operator(&self) -> Digest {
        self.operator
    }
    pub fn original(&self) -> Digest {
        self.original
    }
    pub fn quote(&self) -> Digest {
        self.quote
    }
    pub fn epoch(&self) -> &TrustedEpochV11 {
        &self.epoch
    }
    pub fn horizon(&self) -> u64 {
        self.horizon
    }
}
pub fn source_owner_contract_root_v26(n: &NativeStageSourceOwnerV26) -> Digest {
    object_id(
        b"misaka-palw/kernel/stage-caller-original-PC-contract/v26",
        &(
            (n.job, n.book, n.consent18, n.seal, n.actor, n.actor_operator, n.producer_operator, n.original_paid_seal, n.quote),
            (n.accepted_daa, n.horizon_daa, n.original_debit, n.debit_receipt),
        ),
    )
}
pub fn check_stage_source_owner_v26(
    class: &CheckedStageClassV14,
    book: &StageJobBookV14,
    stage: &CheckedStageQuotaV14,
    quota: &QuotaPolicyV11,
    execution: &ExecutionPolicyV11,
    discovery: &discovery13::DiscoveryPolicyV13,
    epoch: &TrustedEpochV11,
    plan: &CheckedStageSourcePlanV26,
    protected: &ProtectedSealSummaryV11,
    protected_root: &Digest,
    n: &NativeStageSourceOwnerV26,
) -> Result<CheckedStageSourceOwnerV26, String> {
    epoch.check(quota)?;
    book.check_restored(class, stage)?;
    let own = derive_stage_source_caller_plan_v26(class, book, stage, quota, execution, discovery)?;
    let binding = book.quota_subject(stage, quota)?;
    if own.root() != plan.root()
        || n.job != book.id()
        || n.book != book.root()
        || n.consent18 != book.acceptance
        || n.quote != plan.root()
        || n.accepted_daa != epoch.block_daa
        || epoch.network_interval != book.admitted_interval
        || protected.row_root() != *protected_root
        || protected.key.root() != n.seal
        || protected.key.job_id != book.id()
        || protected.key.vector_id != book.claim_root.ok_or("source owner claim absent")?
        || protected.subject_binding_root != binding.root()
        || protected.original_paid_seal_root != n.original_paid_seal
        || protected.key.seal_actor != n.actor
        || n.actor == book.job.producer
        || n.actor_operator != protected.seal_operator_id
        || n.producer_operator != book.producer_operator
        || protected.producer_operator_id != book.producer_operator
        || n.actor_operator == n.producer_operator
        || epoch.block_daa < protected.matures_daa
        || n.horizon_daa != protected.key.horizon_daa
        || n.horizon_daa > book.deadline_daa
        || n.accepted_daa.checked_add(plan.required_horizon()).is_none_or(|at| at > n.horizon_daa)
        || n.original_contract != source_owner_contract_root_v26(n)
        || n.original_debit == 0
        || n.debit_receipt == [0; 64]
    {
        return Err(
            "stage source owner lacks original paid seal/source/claim/operator/quote and sufficient signed adaptive horizon".into()
        );
    }
    Ok(CheckedStageSourceOwnerV26 {
        binding,
        seal: n.seal,
        actor: n.actor,
        operator: n.actor_operator,
        original: n.original_contract,
        quote: plan.root(),
        epoch: epoch.clone(),
        horizon: n.horizon_daa,
    })
}
impl CheckedStageSourcePlanV26 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/stage-caller-original-PC-plan/v26", &self.record)
    }
    pub fn record(&self) -> &StageSourcePlanRecordV26 {
        &self.record
    }
    pub fn requests(&self) -> u32 {
        self.record.requests
    }
    pub fn attempts(&self) -> u32 {
        self.record.attempts
    }
    pub fn required_horizon(&self) -> u64 {
        self.record.required_horizon
    }
    pub fn request_cost(&self) -> &QuotaQuantitiesV11 {
        &self.record.request
    }
    pub fn admission_cost(&self) -> &QuotaQuantitiesV11 {
        &self.record.admission_cost
    }
    pub fn guarantee(&self) -> &QuotaQuantitiesV11 {
        &self.record.guarantee
    }
    /// Readonly fixed cache; expected identity originates only in the original
    /// before-Fund28 contract. Full class/input geometry is rederived after its
    /// paid current caller attempt, before accepting a canonical demand.
    pub fn restore_native_record(raw:&[u8],expected:Digest,quota:&QuotaPolicyV11)->Result<Self,String>{
        quota.check()?;if raw.len()>2048{return Err("caller26 original fixed quote cap".into());}
        let record:StageSourcePlanRecordV26=borsh::from_slice(raw).map_err(|_|"caller26 fixed quote grammar")?;
        let plan=Self{record};
        if plan.root()!=expected||plan.record.version!=26||plan.record.quota_policy!=quota.root()||plan.record.requests==0||plan.record.requests>1<<18||plan.record.attempts!=plan.record.requests.checked_mul(2).ok_or("caller26 original attempts overflow")?||plan.record.required_horizon==0||plan.record.guarantee!=plan.record.request.checked_mul(plan.record.attempts)?||[plan.record.class,plan.record.source,plan.record.real_parameters,plan.record.inventory,plan.record.admission,plan.record.stage_quota,plan.record.producer_source,plan.record.trace_restore,plan.record.execution_policy].contains(&[0;64]){return Err("caller26 cache differs from original root/count/conservation".into());}
        plan.admission_cost().checked_add(plan.guarantee())?.within(&quota.plan)?;Ok(plan)
    }
}
pub fn derive_stage_source_caller_plan_v26(
    class: &CheckedStageClassV14,
    book: &StageJobBookV14,
    stage: &CheckedStageQuotaV14,
    quota: &QuotaPolicyV11,
    execution: &ExecutionPolicyV11,
    discovery: &discovery13::DiscoveryPolicyV13,
) -> Result<CheckedStageSourcePlanV26, String> {
    book.check_restored(class, stage)?;
    execution.check(quota)?;
    discovery.check(quota)?;
    if book.execution_policy != execution.root() || book.terminal != StageTerminalV14::Challengeable {
        return Err("stage caller source quote lacks original execution policy/committed book".into());
    }
    let original = derive_stage_source_plan_v14(class, stage, quota, discovery)?;
    let trace = derive_stage_trace_plan_v14(class, stage, quota)?;
    if original.root() != book.source_plan || trace.root() != book.trace_plan {
        return Err("stage caller source quote changes original18 producer source/restore contracts".into());
    }
    let remainder = original.requests().checked_sub(discovery.ordinary_lifetime_requests).ok_or("source owner count underflow")?;
    if remainder % PROTECTED_SEALS_PER_JOB_V11 != 0 {
        return Err("source owner guarantee does not divide original four retained owners".into());
    }
    let requests = remainder / PROTECTED_SEALS_PER_JOB_V11;
    if requests == 0 || requests > 1 << 18 {
        return Err("source caller request count exceeds independently derived fixed ceiling".into());
    }
    let attempts = requests.checked_mul(2).ok_or("source caller attempt overflow")?;
    // Worst adaptive header/bisection/operand/leaf sequence, plus proof grace.
    // Caller signs the full horizon before any producer-source request. An
    // older short seal or funded book cannot manufacture this time allowance.
    let required_horizon = (requests as u64)
        .checked_mul(execution.horizons.da_answer)
        .and_then(|n| n.checked_add(execution.horizons.proof_grace))
        .ok_or("source caller adaptive horizon overflow")?;
    let mut request = trace.request().checked_add(&action_envelope(16 << 10, 2048, None)?)?;
    include_native_post_io_v11(&mut request, 2048)?;
    // Each successful public PC answer may retain its ORIGINAL full leaf until
    // the signed horizon. This is separate from the producer's answer quota.
    let cache = source16::SOURCE_ANSWER_MAX_BYTES_V16.checked_add(1536).ok_or("source caller cache extent overflow")?;
    for axis in [ResourceAxisV10::StoredEvidenceBytes, ResourceAxisV10::OwnedPeakBytes, ResourceAxisV10::ContainerAllocatorAllowance] {
        request.set(axis, request.get(axis).checked_add(cache as u128).ok_or("source caller retained axis overflow")?);
    }
    let guarantee = request.checked_mul(attempts)?;
    let admission_cost = action_envelope(8192u128 * 64, 8192, None)?;
    admission_cost.checked_add(&guarantee)?.within(&quota.plan)?;
    Ok(CheckedStageSourcePlanV26 {
        record: StageSourcePlanRecordV26 {
            version: 26,
            class: class.root(),
            source: class.source().id(),
            real_parameters: class.source().parameters().root(),
            inventory: class.source().inventory_root(),
            admission: book.admission.root(),
            stage_quota: stage.root(),
            producer_source: original.root(),
            trace_restore: trace.root(),
            quota_policy: quota.root(),
            execution_policy: execution.root(),
            requests,
            attempts,
            required_horizon,
            request,
            admission_cost,
            guarantee,
        },
    })
}

/// Before-Fund source quote for the separately admitted checker25 class. This
/// derives full class/input restoration as bytes rather than treating a work
/// counter as a storage extent, and never upgrades an original18 seal.
pub fn derive_stage_source_caller_plan_v28(
    class:&crate::flat_inputs12::bounded28::CheckedStageClassV28,
    job:&crate::flat_inputs12::bounded28::CheckedStageJobV28,
    stage:&super::bounded28::CheckedStageQuotaV28,
    quota:&QuotaPolicyV11,execution:&ExecutionPolicyV11,discovery:&discovery13::DiscoveryPolicyV13,
)->Result<CheckedStageSourcePlanV26,String>{
    execution.check(quota)?;discovery.check(quota)?;
    let own=if stage.is_invocation_v32(){super::bounded28::derive_stage_quota_v32(class,job)?}else{super::bounded28::derive_stage_quota_v28(class,job)?};
    if own.root()!=stage.root(){return Err("stage28 source caller changes independent maximum/actual quotation".into());}
    let source=stage.source_plan(class,quota,discovery)?;
    let requests=stage.requests_per_owner();
    let attempts=requests.checked_mul(2).ok_or("stage28 source caller attempt overflow")?;
    let required_horizon=(requests as u64).checked_mul(execution.horizons.da_answer).and_then(|n|n.checked_add(execution.horizons.proof_grace)).ok_or("stage28 acquisition horizon overflow")?;
    let bytes=if stage.is_invocation_v32(){stage.restore_bytes().checked_add(32768)}else{class.source_restore_bytes()
        .checked_add(borsh::to_vec(class.wire()).map_err(|e|e.to_string())?.len() as u64)
        .and_then(|n|n.checked_add(u64::try_from(job.input_raw_bytes()).ok()?))
        .and_then(|n|n.checked_add(32768))}.ok_or("stage28 source caller original restore extent overflow")?;
    let work=bytes.checked_mul(bytes).and_then(|n|n.checked_mul(64)).and_then(|n|n.checked_add(bytes.checked_mul(1024)?)).ok_or("stage28 caller restore work overflow")?;
    let mut request=action_envelope(work as u128,bytes,None)?;
    include_source_court_io_v11(&mut request,class.source().parameters())?;
    include_native_post_io_v11(&mut request,stage.plan().native_post_carrier_cap())?;
    let cache=source16::SOURCE_ANSWER_MAX_BYTES_V16+4096;
    for axis in [ResourceAxisV10::StoredEvidenceBytes,ResourceAxisV10::OwnedPeakBytes,ResourceAxisV10::ContainerAllocatorAllowance]{request.set(axis,request.get(axis).checked_add(cache as u128).ok_or("stage28 caller cache overflow")?);}
    let guarantee=request.checked_mul(attempts)?;
    let admission_cost=action_envelope(work as u128,bytes,None)?;
    admission_cost.checked_add(&guarantee)?.within(&quota.plan)?;
    Ok(CheckedStageSourcePlanV26{record:StageSourcePlanRecordV26{version:26,class:class.root(),source:class.source().id(),real_parameters:class.source().parameters().root(),inventory:class.source().inventory_root(),admission:job.admission().root(),stage_quota:stage.root(),producer_source:source.root(),trace_restore:object_id(b"misaka-palw/kernel/stage28-caller-original-restore/v26",&(class.root(),job.admission().root(),stage.root(),bytes,work)),quota_policy:quota.root(),execution_policy:execution.root(),requests,attempts,required_horizon,request,admission_cost,guarantee}})
}

/// Fixed original owner proof contract already authenticated by native28,
/// including actual paid receipt and independent operators. Not a wire grant.
pub struct NativeStageSourceOwnerV28 {
    pub job:Digest,pub original_book:Digest,pub consent:Digest,pub claim:Digest,
    pub seal:Digest,pub actor:Digest,pub actor_operator:Digest,pub producer_operator:Digest,
    pub original_paid:Digest,pub original_plan:Digest,pub horizon:u64,pub paid_at:u64,
    pub actual_debit:u64,pub actual_receipt:Digest,
}
pub fn check_stage_source_owner_v28(
    class:&crate::flat_inputs12::bounded28::CheckedStageClassV28,job:&crate::flat_inputs12::bounded28::CheckedStageJobV28,
    stage:&super::bounded28::CheckedStageQuotaV28,book:&super::bounded28::StageJobBookV28,
    quota:&QuotaPolicyV11,execution:&ExecutionPolicyV11,discovery:&discovery13::DiscoveryPolicyV13,
    epoch:&TrustedEpochV11,plan:&CheckedStageSourcePlanV26,n:&NativeStageSourceOwnerV28,
)->Result<CheckedStageSourceOwnerV26,String>{
    epoch.check(quota)?;book.check(class,job,stage)?;
    let own=derive_stage_source_caller_plan_v28(class,job,stage,quota,execution,discovery)?;
    let binding=book.binding(stage,quota)?;
    if own.root()!=plan.root()||book.caller_source_root()!=plan.root()||book.terminal()!=super::bounded28::StageTerminalV28::Challengeable||n.job!=book.id()||n.original_book!=book.original_root()||n.consent!=book.consent()||Some(n.claim)!=book.claim()||n.actor==binding.producer||n.actor_operator==n.producer_operator||n.producer_operator!=book.producer_operator()||n.original_paid==[0;64]||n.original_plan!=plan.root()||n.seal==[0;64]||n.actual_debit==0||n.actual_receipt==[0;64]||n.paid_at!=epoch.block_daa||epoch.network_interval!=book.admitted_interval()||n.horizon>book.deadline()||n.paid_at.checked_add(plan.required_horizon()).is_none_or(|x|x>n.horizon){return Err("stage28 caller owner lacks exact original paid bound-PC plan/claim/operator/epoch/full horizon".into());}
    Ok(CheckedStageSourceOwnerV26{binding,seal:n.seal,actor:n.actor,operator:n.actor_operator,original:n.original_paid,quote:plan.root(),epoch:epoch.clone(),horizon:n.horizon})
}

#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct StageSourceCallerRowV26 {
    pub version:u16,pub job:Digest,pub subject:Digest,pub owner:Digest,pub actor:Digest,pub operator:Digest,
    pub original_paid:Digest,pub plan:Digest,pub interval:u64,pub horizon:u64,
    pub requests_admitted:u32,pub attempts_used:u32,pub retired:bool,
    pub committed:QuotaQuantitiesV11,pub consumed:QuotaQuantitiesV11,pub remaining:QuotaQuantitiesV11,
}
impl StageSourceCallerRowV26 {pub fn root(&self)->Digest {object_id(b"misaka-palw/kernel/stage-original-caller-PC-row/v26",self)}}
pub struct StageSourceCallerShardV26 {
    global:sharded::QuotaGlobalRowV11,actor:sharded::QuotaActorRowV11,subject:QuotaSubjectSummaryV11,row:Option<StageSourceCallerRowV26>,
}
impl StageSourceCallerShardV26 {
    pub fn from_native_rows(quota:&QuotaPolicyV11,plan:&CheckedStageSourcePlanV26,global:&sharded::QuotaGlobalRowV11,global_root:&Digest,actor:Digest,actor_row:Option<(&sharded::QuotaActorRowV11,&Digest)>,subject:(&QuotaSubjectSummaryV11,&Digest),row:Option<(&StageSourceCallerRowV26,&Digest)>)->Result<Self,String>{
        if plan.record.quota_policy!=quota.root()||subject.0.binding.class_id!=plan.record.class {return Err("caller26 original class/quota differs".into());}
        let base=sharded::CheckedQuotaShardV11::from_native_rows(quota,global,global_root,actor,subject.0.binding.job_id,None,actor_row,Some(subject),None)?;
        let actor=base.actor_row();
        if let Some((r,root))=row {
            let used=plan.request_cost().checked_mul(r.attempts_used)?;
            if r.root()!=*root||r.version!=26||r.job!=subject.0.binding.job_id||r.subject!=subject.0.binding.root()||r.actor!=actor.actor||r.plan!=plan.root()||r.original_paid==[0;64]||r.owner==[0;64]||r.operator==[0;64]||r.interval!=subject.0.binding.admitted_interval||r.horizon>subject.0.binding.deadline_daa||r.requests_admitted>plan.requests()||r.attempts_used>plan.attempts()||r.committed!=plan.admission_cost().checked_add(plan.guarantee())?||r.consumed.checked_add(&r.remaining)?!=r.committed||!r.retired&&(r.consumed!=plan.admission_cost().checked_add(&used)?||r.remaining!=plan.guarantee().checked_sub(&used)?)||r.retired&&(r.consumed!=r.committed||r.remaining!=QuotaQuantitiesV11::default()){return Err("caller26 restored original paid identity/count/conservation differs".into());}
            if r.interval==global.epoch.network_interval {r.remaining.within(&global.network.reserved)?;r.remaining.within(&actor.interval.reserved)?;}
        }
        Ok(Self{global:global.clone(),actor,subject:subject.0.clone(),row:row.map(|(r,_)|r.clone())})
    }
    pub fn global_row(&self)->&sharded::QuotaGlobalRowV11 {&self.global}
    pub fn actor_row(&self)->&sharded::QuotaActorRowV11 {&self.actor}
    pub fn caller_row(&self)->Option<&StageSourceCallerRowV26> {self.row.as_ref()}
    fn context(&self,quota:&QuotaPolicyV11,epoch:&TrustedEpochV11,plan:&CheckedStageSourcePlanV26)->Result<(),String>{
        epoch.check(quota)?;
        if self.global.policy_root!=quota.root()||plan.record.quota_policy!=quota.root()||epoch.network_interval!=self.global.epoch.network_interval||epoch.network_interval!=self.subject.binding.admitted_interval||epoch.block_daa<self.global.epoch.block_daa||epoch.block_hash==self.global.epoch.block_hash&&*epoch!=self.global.epoch {return Err("caller26 context changes original interval or rewinds".into());}Ok(())
    }
    fn block(&self,epoch:&TrustedEpochV11)->QuotaIntervalSummaryV11 {if epoch.block_hash==self.global.epoch.block_hash{self.global.block.clone()}else{QuotaIntervalSummaryV11{consumed:QuotaQuantitiesV11::default(),reserved:self.global.block.reserved.clone()}}}
    pub fn admit(&mut self,quota:&QuotaPolicyV11,epoch:&TrustedEpochV11,plan:&CheckedStageSourcePlanV26,checked:&CheckedStageSourceOwnerV26)->Result<(),String>{
        self.context(quota,epoch,plan)?;
        if self.row.is_some()||self.subject.closed||checked.binding!=self.subject.binding||checked.actor!=self.actor.actor||checked.quote!=plan.root()||checked.epoch!=*epoch||checked.horizon>self.subject.binding.deadline_daa||epoch.block_daa.checked_add(plan.required_horizon()).is_none_or(|x|x>checked.horizon){return Err("caller26 admission lacks private original28 paid owner or sufficient horizon".into());}
        let committed=plan.admission_cost().checked_add(plan.guarantee())?;committed.within(&quota.plan)?;
        let block=self.block(epoch).admit(plan.admission_cost(),plan.guarantee(),&quota.block)?;
        let network=self.global.network.admit(plan.admission_cost(),plan.guarantee(),&quota.network)?;
        let actor=self.actor.interval.admit(plan.admission_cost(),plan.guarantee(),&quota.actor)?;
        self.row=Some(StageSourceCallerRowV26{version:26,job:checked.binding.job_id,subject:checked.binding.root(),owner:checked.seal,actor:checked.actor,operator:checked.operator,original_paid:checked.original,plan:plan.root(),interval:epoch.network_interval,horizon:checked.horizon,requests_admitted:0,attempts_used:0,retired:false,committed,consumed:plan.admission_cost().clone(),remaining:plan.guarantee().clone()});self.global.block=block;self.global.network=network;self.global.epoch=epoch.clone();self.actor.interval=actor;Ok(())
    }
    /// Persist this debit before source/class/claim/input restoration or request
    /// validation. Rejected attempts retain their resource consumption.
    pub fn debit_request(&mut self,quota:&QuotaPolicyV11,epoch:&TrustedEpochV11,plan:&CheckedStageSourcePlanV26,owner:Digest,operator:Digest)->Result<(),String>{
        self.context(quota,epoch,plan)?;let r=self.row.as_ref().ok_or("caller26 original owner absent")?;
        if r.owner!=owner||r.operator!=operator||r.actor!=self.actor.actor||r.retired||self.subject.closed||epoch.block_daa>r.horizon||r.attempts_used>=plan.attempts(){return Err("caller26 request lacks original own prepaid live attempt".into());}
        let block=self.block(epoch).consume(plan.request_cost(),&quota.block)?;let network=self.global.network.consume(plan.request_cost(),&quota.network)?;let actor=self.actor.interval.consume(plan.request_cost(),&quota.actor)?;
        let r=self.row.as_mut().unwrap();r.attempts_used+=1;r.consumed=r.consumed.checked_add(plan.request_cost())?;r.remaining=r.remaining.checked_sub(plan.request_cost())?;self.global.block=block;self.global.network=network;self.global.epoch=epoch.clone();self.actor.interval=actor;Ok(())
    }
    pub fn admit_request(&mut self,plan:&CheckedStageSourcePlanV26)->Result<(),String>{let r=self.row.as_mut().ok_or("caller26 owner absent")?;if r.retired||r.requests_admitted>=plan.requests()||r.requests_admitted>=r.attempts_used{return Err("caller26 canonical successful request count exceeded".into());}r.requests_admitted+=1;Ok(())}
    /// Expiry conveys no computation/source correctness. Native checks every
    /// original pending unit settled before releasing its owner slot; unused
    /// capacity becomes consumed, and neither nonce nor interval is recreated.
    pub fn expire_owner(&mut self,quota:&QuotaPolicyV11,epoch:&TrustedEpochV11,plan:&CheckedStageSourcePlanV26,owner:Digest,original_paid:Digest)->Result<(),String>{
        epoch.check(quota)?;
        if epoch.network_interval==self.subject.binding.admitted_interval{self.context(quota,epoch,plan)?;}else if epoch.network_interval<self.subject.binding.admitted_interval||epoch.network_interval!=self.global.epoch.network_interval||plan.record.quota_policy!=quota.root()||epoch.block_daa<self.global.epoch.block_daa{return Err("caller26 expired collection rewinds original epoch".into());}
        let r=self.row.as_ref().ok_or("caller26 original owner absent")?;
        if r.retired||r.owner!=owner||r.original_paid!=original_paid||epoch.block_daa<=r.horizon||r.horizon>self.subject.binding.deadline_daa{return Err("caller26 original paid owner has not genuinely expired".into());}
        let remaining=r.remaining.clone();
        if epoch.network_interval==r.interval{
            let block=self.block(epoch).consume(&remaining,&quota.block)?;let network=self.global.network.consume(&remaining,&quota.network)?;let actor=self.actor.interval.consume(&remaining,&quota.actor)?;
            self.global.block=block;self.global.network=network;self.actor.interval=actor;
        }
        let r=self.row.as_mut().unwrap();r.retired=true;r.consumed=r.committed.clone();r.remaining=QuotaQuantitiesV11::default();self.global.epoch=epoch.clone();Ok(())
    }
    /// A discovery conversion transfers its already paid caller reserve and
    /// lifetime count; it cannot reset attempts, extend the horizon or admit a
    /// fifth producer obligation. Native authenticates both original page rows.
    pub fn transfer_owner(&mut self,quota:&QuotaPolicyV11,epoch:&TrustedEpochV11,plan:&CheckedStageSourcePlanV26,old:Digest,new:Digest,original_paid:Digest)->Result<(),String>{
        self.context(quota,epoch,plan)?;let r=self.row.as_mut().ok_or("caller26 original discovery owner absent")?;
        if r.retired||r.owner!=old||old==new||new==[0;64]||r.original_paid!=original_paid||epoch.block_daa>=r.horizon{return Err("caller26 discovery conversion changes original paid liability".into());}r.owner=new;Ok(())
    }

    /// Genuine original terminal, all pending units settled and horizon elapsed
    /// are checked by native membership before this private-root anchored call.
    pub fn retire(&mut self,quota:&QuotaPolicyV11,epoch:&TrustedEpochV11,plan:&CheckedStageSourcePlanV26,original_paid:Digest,terminal:&CheckedQuotaTerminalV11)->Result<(),String>{
        self.context(quota,epoch,plan)?;let r=self.row.as_ref().ok_or("caller26 owner absent")?;
        if r.retired||!self.subject.closed||terminal.binding!=self.subject.binding||r.original_paid!=original_paid||epoch.block_daa<=r.horizon{return Err("caller26 retirement lacks genuine closed original terminal/expiry".into());}
        let extra=r.remaining.clone();let block=self.block(epoch).consume(&extra,&quota.block)?;let network=self.global.network.consume(&extra,&quota.network)?;let actor=self.actor.interval.consume(&extra,&quota.actor)?;
        let r=self.row.as_mut().unwrap();r.retired=true;r.consumed=r.committed.clone();r.remaining=QuotaQuantitiesV11::default();self.global.block=block;self.global.network=network;self.global.epoch=epoch.clone();self.actor.interval=actor;Ok(())
    }
}

/// Separate original Text29 caller plan. The producer's answer pool is never
/// used to pay the requesting actor's full cold restoration or retained cache.
pub fn derive_text_source_caller_plan_v29(
    program:&misaka_palw_tir::program::TirProgramV1,pc:&ParamCommitmentsV1,
    text:&super::super::text29::CheckedTextQuotaV29,quota:&QuotaPolicyV11,
    execution:&ExecutionPolicyV11,discovery:&discovery13::DiscoveryPolicyV13,
)->Result<CheckedStageSourcePlanV26,String>{
    execution.check(quota)?;discovery.check(quota)?;
    let source=text.source_plan(program,pc,quota,discovery)?;
    let requests=text.requests_per_owner();if requests==0||requests>1<<18{return Err("text29 caller request bound exceeds original cap".into());}
    let attempts=requests.checked_mul(2).ok_or("text29 caller attempts overflow")?;
    let required_horizon=(requests as u64).checked_mul(execution.horizons.da_answer).and_then(|n|n.checked_add(execution.horizons.proof_grace)).ok_or("text29 caller acquisition horizon overflow")?;
    let bytes=text.restore_bytes().checked_add(32768).ok_or("text29 caller restore extent overflow")?;
    let work=bytes.checked_mul(bytes).and_then(|n|n.checked_mul(64)).and_then(|n|n.checked_add(bytes.checked_mul(1024)?)).ok_or("text29 caller restore work overflow")?;
    let mut request=action_envelope(work as u128,bytes,None)?;include_source_court_io_v11(&mut request,pc)?;include_native_post_io_v11(&mut request,text.plan().native_post_carrier_cap())?;
    let cache=source16::SOURCE_ANSWER_MAX_BYTES_V16+4096;
    for axis in [ResourceAxisV10::StoredEvidenceBytes,ResourceAxisV10::OwnedPeakBytes,ResourceAxisV10::ContainerAllocatorAllowance]{request.set(axis,request.get(axis).checked_add(cache as u128).ok_or("text29 caller cache overflow")?);}
    let guarantee=request.checked_mul(attempts)?;let admission_cost=action_envelope(work as u128,bytes,None)?;admission_cost.checked_add(&guarantee)?.within(&quota.plan)?;
    Ok(CheckedStageSourcePlanV26{record:StageSourcePlanRecordV26{version:26,class:text.class(),source:text.source(),real_parameters:pc.root(),inventory:text.source(),admission:text.plan().record().root(),stage_quota:text.root(),producer_source:source.root(),trace_restore:object_id(b"misaka-palw/kernel/text29-caller-original-restore/v26",&(text.root(),bytes,work)),quota_policy:quota.root(),execution_policy:execution.root(),requests,attempts,required_horizon,request,admission_cost,guarantee}})
}
/// Authenticated current new29 paid-owner metadata. No wire decoder or raw
/// quota vector constructor exists for the corresponding private grant.
pub struct TextNativeSourceOwnerV29 {
    pub job:Digest,pub funding:Digest,pub consent:Digest,pub claim:Digest,pub original_claim:Digest,
    pub seal:Digest,pub actor:Digest,pub actor_operator:Digest,pub producer_operator:Digest,
    pub original_paid:Digest,pub original_plan:Digest,pub original_caller_plan:Digest,
    pub horizon:u64,pub paid_at:u64,pub actual_debit:u64,pub actual_receipt:Digest,
}
pub fn check_text_source_owner_v29(
    program:&misaka_palw_tir::program::TirProgramV1,pc:&ParamCommitmentsV1,text:&super::super::text29::CheckedTextQuotaV29,
    anchor:&super::super::text29::TextNativeFundingAnchorV29,nonce:&NativeRequesterNonceWitnessV11,
    quota:&QuotaPolicyV11,execution:&ExecutionPolicyV11,discovery:&discovery13::DiscoveryPolicyV13,
    epoch:&TrustedEpochV11,plan:&CheckedStageSourcePlanV26,n:&TextNativeSourceOwnerV29,
)->Result<CheckedStageSourceOwnerV26,String>{
    let binding=super::super::text29::source_owner_binding_v29(text,anchor,quota,nonce,epoch)?;
    let own=derive_text_source_caller_plan_v29(program,pc,text,quota,execution,discovery)?;
    if own.root()!=plan.root()||n.original_caller_plan!=plan.root()||n.original_plan!=text.root()||n.job!=anchor.job||n.funding!=anchor.funding||n.consent!=anchor.consent||n.claim==[0;64]||n.claim!=n.original_claim||n.actor==anchor.producer||n.actor_operator==n.producer_operator||n.actor_operator==[0;64]||n.producer_operator!=anchor.producer_operator||n.original_paid==[0;64]||n.seal==[0;64]||n.actual_debit==0||n.actual_receipt==[0;64]||n.paid_at!=epoch.block_daa||n.horizon>anchor.deadline||n.paid_at.checked_add(plan.required_horizon()).is_none_or(|x|x>n.horizon){return Err("text29 caller lacks exact original funding/source/claim/independent paid operator/epoch/full horizon".into());}
    Ok(CheckedStageSourceOwnerV26{binding,seal:n.seal,actor:n.actor,operator:n.actor_operator,original:n.original_paid,quote:plan.root(),epoch:epoch.clone(),horizon:n.horizon})
}

// Generated source26 native inventory bridge; private fields remain private.
impl StageSourcePlanRecordV26 {
 pub fn native_inventory_parts_v1(&self)->((&u16,&Digest,&Digest,&Digest,&Digest,&Digest,),(&Digest,&Digest,&Digest,&Digest,&Digest,&u32,),(&u32,&u64,&QuotaQuantitiesV11,&QuotaQuantitiesV11,&QuotaQuantitiesV11,),) {
 let Self { version,class,source,real_parameters,inventory,admission,stage_quota,producer_source,trace_restore,quota_policy,execution_policy,requests,attempts,required_horizon,request,admission_cost,guarantee }=self;
 ((version,class,source,real_parameters,inventory,admission,),(stage_quota,producer_source,trace_restore,quota_policy,execution_policy,requests,),(attempts,required_horizon,request,admission_cost,guarantee,),)
 }
}
