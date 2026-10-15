//! Readonly cumulative A6 observations from complete audit32 openings. These are
//! not consensus objects or admission/Final grants. Missing/collected originals
//! refuse; caller-supplied counters and success booleans are never inputs.
use crate::{Hash64, config::params::Params, palw_state_v2::PalwChainStateV2,
    palw_native_audit_proof_v32::{VerifiedNativeAuxRowsV32, VerifiedNativePayoutRowsV32,
        derive_native_execution_audit_from_proofs_v32},
    palw_native_queue_v30::{NativeQueueGlobalV30, NativeQueueLeaseV30, NATIVE_QUEUE_ROW_LIMIT_V30},
    palw_flat_execution_capacity_v22::NativeCarriageGlobalV22,
    palw_native_paged_proof_v29::NativePagedLeaseJournalV29,
    palw_flat_execution_v11::{EXECUTION_QUOTA_TABLE_V11, EXECUTION_JOB_TABLE_V11},
    palw_flat_stage_v14::STAGE_TABLE_V14};
use borsh::{BorshSerialize, BorshDeserialize};
use misaka_palw_kernel::hash::{Digest, object_id};
use std::collections::{BTreeMap,BTreeSet};
pub const MAX_RECEIPT_WINDOW_JOBS_V1:usize=512;
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct NativeReceiptJobV1 {
    pub namespace:u16,pub complete_model:bool,pub job:Digest,pub class:Digest,pub source:Digest,pub inventory:Digest,
    pub real_parameter:Digest,pub funding:Digest,pub consent:Digest,pub maximum_contract:Digest,
    pub maximum_scope:Digest,pub maximum_resource:Digest,pub queue_plan:Digest,pub queue_policy:Digest,
    pub source_plan:Digest,pub physical_service:Digest,pub physical_plan:Digest,pub carriage_version:u16,pub requester_operator:Digest,pub producer_operator:Digest,
    pub nonce:Digest,pub admitted:u64,pub commit_deadline:u64,pub deadline:u64,
    pub claim:Option<Digest>,pub committed:Option<u64>,pub challenge_end:Option<u64>,pub terminal:String,
}
impl NativeReceiptJobV1 {pub fn root(&self)->Digest{object_id(b"misaka-palw/audit/original-receipt-job/v1",self)}}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct NativeReceiptQueueV1 {
    pub root:Digest,pub physical_root:Digest,pub policy:Digest,pub service:Digest,
    pub live:u32,pub packets:u32,pub owned_bytes:u64,pub jobs:Vec<Digest>,
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct NativeReceiptFinalV1 {
    pub funding:Digest,pub claim:Digest,pub credit:Digest,pub established_daa:u64,pub producer_credit:u64,
}
#[derive(Clone)]
pub struct CheckedNativeReceiptSnapshotV1 {
    state:Hash64,aux:Hash64,daa:u64,queue:NativeReceiptQueueV1,
    jobs:BTreeMap<Digest,NativeReceiptJobV1>,finals:BTreeMap<Digest,NativeReceiptFinalV1>,
}
impl CheckedNativeReceiptSnapshotV1 {
    pub fn daa(&self)->u64{self.daa}
    pub fn queue(&self)->&NativeReceiptQueueV1{&self.queue}
    pub fn jobs(&self)->&BTreeMap<Digest,NativeReceiptJobV1>{&self.jobs}
    pub fn finals(&self)->&BTreeMap<Digest,NativeReceiptFinalV1>{&self.finals}
    /// Conservative local memory accounting, never a consensus resource grant.
    pub fn metadata_bytes(&self)->usize{
        self.jobs.len().saturating_mul(std::mem::size_of::<NativeReceiptJobV1>()+128)
            .saturating_add(self.finals.len().saturating_mul(std::mem::size_of::<NativeReceiptFinalV1>()+128))
            .saturating_add(self.queue.jobs.len().saturating_mul(64)).saturating_add(1024)
    }
    pub fn root(&self)->Digest{object_id(b"misaka-palw/audit/receipt-snapshot/v1",&(self.state,self.aux,self.daa,&self.queue,&self.jobs,&self.finals))}
}
fn decode<T:BorshDeserialize>(rows:&BTreeMap<(u8,Vec<u8>),Vec<u8>>,key:Vec<u8>,cap:usize)->Result<T,String>{
    let raw=rows.get(&(EXECUTION_QUOTA_TABLE_V11,key)).ok_or("receipt window original shared row missing")?;
    if raw.len()>cap{return Err("receipt window shared row cap".into());}
    borsh::from_slice(raw).map_err(|_|"receipt window shared row exact grammar".into())
}
/// `tracked` contains frozen intended jobs AND all previously observed jobs.
/// Every job currently occupying the global queue is added independently. No
/// dropped/retired original may disappear from a later opening unnoticed.
pub fn derive_native_receipt_snapshot_v1(aux:&VerifiedNativeAuxRowsV32<'_>,payouts:&VerifiedNativePayoutRowsV32<'_>,
    params:&Params,daa:u64,tracked:&[Digest])->Result<CheckedNativeReceiptSnapshotV1,String>{
    if aux.state_root()!=payouts.state_root()||tracked.len()>MAX_RECEIPT_WINDOW_JOBS_V1
        ||tracked.iter().collect::<BTreeSet<_>>().len()!=tracked.len()||!params.native_final_is_qualified_v34(){
        return Err("receipt window requires qualified owning rules and bounded unique original jobs".into());}
    let policy=&params.palw_native_final_v27.as_ref().filter(|p|p.activation.is_active(daa))
        .ok_or("receipt window owning Final fence inactive")?.policy.queue_policy;
    let mut rows=BTreeMap::new();
    for (key,value) in aux.rows(){let(table,key):(u8,Vec<u8>)=borsh::from_slice(key).map_err(|_|"receipt window aux key grammar")?;
        if rows.insert((table,key),value.clone()).is_some(){return Err("receipt window duplicate original row".into());}}
    // Requiring both initialized globals is intentional: an absent proof of an
    // empty pre-run queue is NOT a measured zero. Prime the native queue first.
    let(gr,g):(Digest,NativeQueueGlobalV30)=decode(&rows,borsh::to_vec(&(30u8,0u8)).unwrap(),NATIVE_QUEUE_ROW_LIMIT_V30)?;
    let(pr,physical):(Digest,NativeCarriageGlobalV22)=decode(&rows,borsh::to_vec(&(22u8,0u8)).unwrap(),512)?;
    if gr!=g.root()||pr!=physical.root(){return Err("receipt window shared roots differ".into());}
    let entries=g.audit_entries_v1(policy,&physical)?;
    let mut observed: BTreeSet<Digest>=tracked.iter().copied().collect();
    let mut queued=Vec::new();let mut density=0u64;let mut minimum=u64::MAX;
    for (job,funding,lease_root,packets,owned) in entries {
        let(root,lease):(Digest,NativeQueueLeaseV30)=decode(&rows,borsh::to_vec(&(30u8,*job,1u8)).unwrap(),1024)?;
        let(jroot,journal):(Digest,NativePagedLeaseJournalV29)=decode(&rows,borsh::to_vec(&(29u8,*job,22u8)).unwrap(),2048)?;
        if root!=*lease_root||lease.root()!=root||lease.retired||lease.job!=*job||lease.original_funding!=*funding
            ||lease.original_policy!=policy.root()||lease.packets!=packets||lease.owned_bytes!=owned
            ||jroot!=journal.root()||journal.version!=29||!matches!(journal.namespace,28|29)||journal.job!=*job||journal.funding!=*funding||journal.service!=g.service
            ||journal.lease.root()!=journal.lease_root||journal.lease_root!=lease.original_physical_lease
            ||journal.lease.retired||journal.lease.original_job!=*job||journal.lease.original_consent!=lease.original_consent {
            return Err("receipt window queue/physical/original consent lineage differs".into());}
        density=density.checked_add(journal.lease.density).ok_or("receipt window density overflow")?;
        minimum=minimum.min(journal.lease.minimum_deadline);observed.insert(*job);queued.push(*job);
    }
    if density!=physical.density||(physical.live>0&&physical.minimum_deadline>minimum)
        ||(physical.live==0&&(physical.density!=0||physical.minimum_deadline!=u64::MAX))||observed.len()>MAX_RECEIPT_WINDOW_JOBS_V1 {
        return Err("receipt window incomplete shared physical accounting".into());}
    let mut jobs=BTreeMap::new();let mut finals=BTreeMap::new();
    for id in observed {
        let text=rows.contains_key(&(EXECUTION_JOB_TABLE_V11,borsh::to_vec(&(29u8,0u8,id)).unwrap()));
        let stage=rows.contains_key(&(STAGE_TABLE_V14,borsh::to_vec(&(28u8,5u8,id)).unwrap()));
        if text&&stage{return Err("receipt window ambiguous original namespace".into());}
        let observation=if text {PalwChainStateV2::native_text_receipt_from_rows_v1(&rows,id)?}
            else if stage {let p=&params.palw_stage_execution_v28.as_ref().filter(|p|p.activation.is_active(daa)).ok_or("receipt window Stage28 fence inactive")?.policies;
                PalwChainStateV2::native_stage_receipt_from_rows_v1(&rows,p,id)?}
            else {if queued.contains(&id){return Err("receipt window queued original book collected or unsupported".into());}continue};
        if observation.job!=id||observation.admitted>daa||observation.committed.is_some_and(|at|at>daa)
            ||observation.queue_policy!=policy.root()||observation.physical_service!=g.service {
            return Err("receipt window original clocks/policy/service differ".into());}
        if observation.terminal=="Final" {
            let audit=derive_native_execution_audit_from_proofs_v32(aux,payouts,params,daa,id)?;
            if audit.record().funding!=observation.funding||Some(audit.record().claim)!=observation.claim {
                return Err("receipt window Final differs from original accepted claim".into());}
            let f=audit.record();
            finals.insert(id,NativeReceiptFinalV1{funding:f.funding,claim:f.claim,credit:f.credit,established_daa:f.established_daa,producer_credit:f.producer_credit});
        }
        jobs.insert(id,observation);
    }
    for id in &queued {let j=jobs.get(id).ok_or("receipt window live original absent")?;
        let(_,lease):(Digest,NativeQueueLeaseV30)=decode(&rows,borsh::to_vec(&(30u8,*id,1u8)).unwrap(),1024)?;
        let(_,physical):(Digest,NativePagedLeaseJournalV29)=decode(&rows,borsh::to_vec(&(29u8,*id,22u8)).unwrap(),2048)?;
        let carriage:crate::palw_native_paged_carriage_v29::NativePagedCarriageRecordV29=decode(&rows,borsh::to_vec(&(29u8,*id,23u8)).unwrap(),4096)?;
        if carriage.version!=j.carriage_version||carriage.root()!=j.physical_plan||carriage.class!=j.class||carriage.service!=j.physical_service
            ||physical.lease.plan!=j.physical_plan||physical.proof_envelope!=carriage.proof_envelope
            ||lease.original_funding!=j.funding||lease.original_consent!=j.consent||lease.original_plan!=j.queue_plan
            ||physical.namespace!=j.namespace||physical.maximum!=j.maximum_contract {
            return Err("receipt window live original max/Fund/queue rebind".into());}}
    Ok(CheckedNativeReceiptSnapshotV1{state:aux.state_root(),aux:aux.root(),daa,
        queue:NativeReceiptQueueV1{root:gr,physical_root:pr,policy:policy.root(),service:g.service,live:g.live,packets:g.packets,owned_bytes:g.owned_bytes,jobs:queued},jobs,finals})
}
