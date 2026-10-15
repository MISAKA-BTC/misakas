//! Two sustained receipt windows, independently reconstructed from consecutive
//! own-chain header/state openings. JSON is only frozen intent and file pointers;
//! original accepted books, Final32 receipts and joint Queue30 accounting decide.
use kaspa_consensus_core::{Hash64,config::params::Params,header::Header,
    palw_native_audit_proof_v32::{verify_native_aux_v32,verify_native_payout_rows_v32},
    palw_native_receipt_windows_v1::{derive_native_receipt_snapshot_v1,CheckedNativeReceiptSnapshotV1,NativeReceiptJobV1},
    palw_state_proof_v1::PalwFactProofV1};
use serde::{Serialize,Deserialize};
use std::collections::{BTreeMap,BTreeSet};
use misaka_palw_kernel::hash::object_id;
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntendedReceiptV1 {
    pub member:String,pub job:Hash64,pub class:Hash64,pub source:Hash64,pub inventory:Hash64,pub real_parameter:Hash64,
    pub funding:Option<Hash64>,pub consent:Hash64,pub maximum_contract:Hash64,pub maximum_scope:Hash64,pub maximum_resource:Hash64,
    pub queue_plan:Hash64,pub queue_policy:Hash64,pub source_plan:Hash64,pub physical_service:Hash64,
    pub requester_operator:Hash64,pub producer_operator:Hash64,pub nonce:Hash64,
    pub issuance_not_before:u64,pub issuance_deadline:u64,
}
impl IntendedReceiptV1 {
    fn matches(&self,j:&NativeReceiptJobV1)->bool{
        [(self.job,j.job),(self.class,j.class),(self.source,j.source),(self.inventory,j.inventory),(self.real_parameter,j.real_parameter),
            (self.consent,j.consent),(self.maximum_contract,j.maximum_contract),
            (self.maximum_scope,j.maximum_scope),(self.maximum_resource,j.maximum_resource),(self.queue_plan,j.queue_plan),
            (self.queue_policy,j.queue_policy),(self.source_plan,j.source_plan),(self.physical_service,j.physical_service),
            (self.requester_operator,j.requester_operator),(self.producer_operator,j.producer_operator),(self.nonce,j.nonce)]
            .iter().all(|(a,b)|a.as_bytes()==*b)&&self.funding.is_none_or(|f|f.as_bytes()==j.funding)
    }
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptWindowV1 {pub start_daa:u64,pub end_daa:u64,pub issuance_bin_daa:u64,pub minimum_commits_per_bin:usize,pub maximum_final_lag_daa:u64,pub intended:Vec<IntendedReceiptV1>}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptWindowContractV1 {
    pub schema:String,pub cohort_sha256:String,pub receipt_window_daa:u64,pub minimum_independent_operators:usize,
    pub original_model_roots:BTreeMap<String,Hash64>,pub windows:Vec<ReceiptWindowV1>,
}
impl ReceiptWindowContractV1 {
    pub fn check(&self)->Result<(),String>{
        if self.schema!="misaka.rfc02.native-receipt-window-contract.v1"||self.cohort_sha256.len()!=64
            ||!self.cohort_sha256.bytes().all(|b|b.is_ascii_hexdigit())||self.receipt_window_daa==0
            ||self.minimum_independent_operators<2||self.minimum_independent_operators>128||self.windows.len()!=2
            ||self.original_model_roots.is_empty()||self.original_model_roots.len()>512{return Err("receipt window frozen contract grammar".into());}
        let mut jobs=BTreeSet::new();let mut previous=None;
        for w in &self.windows {
            if w.end_daa.checked_sub(w.start_daa).is_none_or(|d|d<self.receipt_window_daa)
                ||previous.is_some_and(|d|w.start_daa<d)||w.intended.is_empty()||w.intended.len()>512
                ||w.issuance_bin_daa==0||w.issuance_bin_daa>w.end_daa.saturating_sub(w.start_daa)/2
                ||(w.end_daa-w.start_daa)%w.issuance_bin_daa!=0||w.minimum_commits_per_bin==0
                ||w.maximum_final_lag_daa==0||w.maximum_final_lag_daa>w.issuance_bin_daa||w.end_daa.checked_add(w.maximum_final_lag_daa).is_none(){return Err("receipt window original nonoverlapping clocks/intent missing".into());}
            previous=Some(w.end_daa);
            for j in &w.intended {if !jobs.insert(j.job)||self.original_model_roots.get(&j.member)!=Some(&j.inventory)
                ||j.requester_operator==j.producer_operator||[j.job,j.class,j.source,j.inventory,j.real_parameter,j.consent,j.maximum_contract,j.maximum_scope,j.maximum_resource,j.queue_plan,j.queue_policy,j.source_plan,j.physical_service,j.requester_operator,j.producer_operator].iter().any(|h|h.as_bytes()==[0;64]){
                    return Err("receipt window repeats a job, changes original model R or omits its original contract".into());}
                if j.issuance_not_before<w.start_daa||j.issuance_deadline> w.end_daa||j.issuance_deadline<=j.issuance_not_before
                    ||j.funding.is_some_and(|f|f.as_bytes()==[0;64]){return Err("receipt window original cadence/issuance deadline differs".into());}
            }
            let bins=(w.end_daa-w.start_daa)/w.issuance_bin_daa;
            if bins>1024{return Err("receipt window cadence bin cap".into());}
            for bin in 0..bins {let from=w.start_daa+bin*w.issuance_bin_daa;let to=from+w.issuance_bin_daa;
                if w.intended.iter().filter(|j|j.issuance_not_before>=from&&j.issuance_deadline<=to).count()<w.minimum_commits_per_bin{return Err("receipt window frozen cadence denominator lacks an issuance bin".into());}}
        }
        if jobs.len()>512{return Err("receipt window total intended job cap".into());}
        Ok(())
    }
    pub fn jobs(&self)->Vec<[u8;64]>{self.windows.iter().flat_map(|w|w.intended.iter().map(|j|j.job.as_bytes())).collect()}
    pub fn root(&self)->Result<String,String>{self.check()?;let bytes=serde_json::to_vec(self).map_err(|e|e.to_string())?;
        Ok(crate::runtime_pack::commit::hex(&object_id(b"misaka-palw/audit/frozen-receipt-intent/v1",&bytes)))}
}
pub struct AnchoredReceiptPointV1 {child:Hash64,parent:Hash64,snapshot:CheckedNativeReceiptSnapshotV1}
impl AnchoredReceiptPointV1 {
    pub fn derive(child:&Header,child_pin:Hash64,parent:&Header,parent_pin:Hash64,params:&Params,
        aux:&PalwFactProofV1,payouts:&PalwFactProofV1,jobs:&[[u8;64]])->Result<Self,String>{
        let state=crate::native_audit_v32::native_inner_state_of_pinned_headers_v32(child,child_pin,parent,parent_pin,params,None)?;
        let aux=verify_native_aux_v32(aux,state)?;let payouts=verify_native_payout_rows_v32(payouts,state)?;
        let snapshot=derive_native_receipt_snapshot_v1(&aux,&payouts,params,parent.daa_score,jobs)?;
        Ok(Self{child:child_pin,parent:parent_pin,snapshot})
    }
    pub fn tracked(&self)->Vec<[u8;64]>{self.snapshot.jobs().keys().copied().collect()}
}
/// Full transcript only: every consecutive selected-parent state is required,
/// so an intermediate admission/retirement cannot hide between endpoint totals.
/// A removed original book remains a named failure rather than an inferred Final.
pub fn audit_receipt_windows_v1(contract:&ReceiptWindowContractV1,points:&[AnchoredReceiptPointV1])->Result<serde_json::Value,String>{
    contract.check()?;
    if points.len()<3||points.len()>65536{return Err("receipt window complete bounded transcript required".into());}
    for pair in points.windows(2){if pair[1].parent!=pair[0].child||pair[1].snapshot.daa()<pair[0].snapshot.daa(){return Err("receipt window selected-parent state skipped/reordered/reorged".into());}}
    let mut prior:BTreeMap<[u8;64],NativeReceiptJobV1>=BTreeMap::new();
    let mut prior_queue=BTreeSet::new();
    let mut funded=BTreeSet::new();let mut issued=BTreeSet::new();let mut finalized=BTreeSet::new();let mut retired=BTreeSet::new();
    let mut cumulative=Vec::new();
    for point in points {
        let s=&point.snapshot;let queue:BTreeSet<_>=s.queue().jobs.iter().copied().collect();
        for (id,old) in &prior {
            let new=s.jobs().get(id).ok_or("receipt window original collected before cumulative audit completed")?;
            // Stable original fields and claims are compared independently of terminal roots.
            let mut a=old.clone();let mut b=new.clone();a.claim=None;b.claim=None;a.committed=None;b.committed=None;a.challenge_end=None;b.challenge_end=None;a.terminal.clear();b.terminal.clear();
            if a!=b||old.claim.is_some()&&old.claim!=new.claim||old.committed.is_some()&&old.committed!=new.committed
                ||matches!(old.terminal.as_str(),"Final"|"Convicted"|"Refunded")&&old.terminal!=new.terminal {
                return Err("receipt window original source/scope/nonce/claim or terminal changed".into());}
        }
        for id in &queue {if retired.contains(id){return Err("receipt window retired original reentered queue".into());}}
        for id in prior_queue.difference(&queue){let j=s.jobs().get(id).ok_or("receipt window queue drop lacks original terminal")?;
            if !matches!(j.terminal.as_str(),"Final"|"Convicted"|"Refunded"){return Err("receipt window queue drop before genuine terminal".into());}retired.insert(*id);}
        for (id,j) in s.jobs(){
            funded.insert(*id);
            if j.claim.is_some(){if j.committed.is_none()||j.committed.is_some_and(|d|d<j.admitted||d>=j.commit_deadline)||j.challenge_end.is_none(){return Err("receipt window claim lacks original accepted clock/opportunity".into());}issued.insert(*id);}
            if j.terminal=="Final"{let f=s.finals().get(id).ok_or("receipt window genuine independent Final32 absent")?;
                if Some(f.claim)!=j.claim||f.established_daa>s.daa()||f.established_daa<j.challenge_end.unwrap_or(u64::MAX){return Err("receipt window Final clock/claim mismatch".into());}finalized.insert(*id);}
        }
        cumulative.push(serde_json::json!({"child":point.child.to_string(),"selected_parent":point.parent.to_string(),"daa":s.daa(),
            "snapshot_root":crate::runtime_pack::commit::hex(&s.root()),"queue_root":crate::runtime_pack::commit::hex(&s.queue().root),
            "physical_root":crate::runtime_pack::commit::hex(&s.queue().physical_root),"live":s.queue().live,"packets":s.queue().packets,"owned_bytes":s.queue().owned_bytes,
            "funded_cumulative":funded.len(),"issued_cumulative":issued.len(),"finalized_cumulative":finalized.len(),"retired_cumulative":retired.len()}));
        prior=s.jobs().clone();prior_queue=queue;
    }
    let mut windows=Vec::new();
    for w in &contract.windows {
        let start=points.iter().rposition(|p|p.snapshot.daa()<=w.start_daa).ok_or("receipt window beginning unobserved")?;
        let end=points.iter().position(|p|p.snapshot.daa()>=w.end_daa).ok_or("receipt window ending unobserved")?;
        if start>=end{return Err("receipt window no sustained original chain opportunity".into());}
        let final_deadline=w.end_daa.checked_add(w.maximum_final_lag_daa).ok_or("receipt window final lag overflow")?;
        let completed=points.iter().position(|p|p.snapshot.daa()>=final_deadline).ok_or("receipt window bounded Final tail unobserved")?;
        let first=&points[start].snapshot;let last=&points[end].snapshot;let final_point=&points[completed].snapshot;
        let mut operators=BTreeSet::new();let mut claims=Vec::new();let bins=(w.end_daa-w.start_daa)/w.issuance_bin_daa;
        let mut bin_counts=vec![0usize;bins as usize];let mut final_bins=vec![0usize;bins as usize];
        for intended in &w.intended {
            let id=intended.job.as_bytes();
            if let Some(a)=first.jobs().get(&id){if !intended.matches(a)||a.claim.is_some()||a.terminal!="Funded"{
                return Err("receipt window previously funded intent is rebound or already issued".into());}}
            // Future admission is allowed. The first native Fund observed in the
            // complete transcript fixes its original root once, before Commit.
            let b=final_point.jobs().get(&id).ok_or("receipt window intended original never admitted or disappeared")?;
            let committed=b.committed.ok_or("receipt window intended original never issued")?;
            if !intended.matches(b)||b.admitted>intended.issuance_deadline||b.commit_deadline<=intended.issuance_not_before
                ||committed<intended.issuance_not_before||committed>=intended.issuance_deadline||b.terminal!="Final" {
                return Err("receipt window intended exact original R/contract, accepted cadence or opportunity failed".into());}
            let f=final_point.finals().get(&id).ok_or("receipt window intended Final32 absent")?;
            if f.established_daa<committed||committed.checked_add(w.maximum_final_lag_daa).is_none_or(|at|f.established_daa>at)||f.producer_credit==0{
                return Err("receipt window intended Final stalled beyond frozen lag".into());}
            let bin=((committed-w.start_daa)/w.issuance_bin_daa)as usize;
            if bin>=bin_counts.len(){return Err("receipt window accepted Commit outside issuance bins".into());}
            bin_counts[bin]+=1;final_bins[bin]+=1;operators.insert(b.producer_operator);
            claims.push(serde_json::json!({"member":intended.member,"job":intended.job,"original_funding":crate::runtime_pack::commit::hex(&b.funding),
                "claim":crate::runtime_pack::commit::hex(&b.claim.unwrap()),"committed_daa":committed,"final_daa":f.established_daa,
                "producer_operator":intended.producer_operator,"single_credit":crate::runtime_pack::commit::hex(&f.credit)}));
        }
        if operators.len()<contract.minimum_independent_operators||last.queue().live>first.queue().live||last.queue().packets>first.queue().packets||last.queue().owned_bytes>first.queue().owned_bytes
            ||bin_counts.iter().any(|n|*n<w.minimum_commits_per_bin)||final_bins!=bin_counts{
            return Err("receipt window sustained issuance/Final throughput, independent participation or non-growing original queue failed".into());}
        windows.push(serde_json::json!({"start_daa":w.start_daa,"end_daa":w.end_daa,"independent_operators":operators.len(),
            "issuance_bin_daa":w.issuance_bin_daa,"minimum_commits_per_bin":w.minimum_commits_per_bin,"maximum_final_lag_daa":w.maximum_final_lag_daa,
            "accepted_commits_per_bin":bin_counts,"bounded_finals_per_bin":final_bins,"final_observation_daa":final_point.daa(),
            "intended_claims":w.intended.len(),"issued_claims":claims.len(),"finalized_claims":claims.len(),"queue_start":first.queue().live,
            "queue_end":last.queue().live,"queue_packets_start":first.queue().packets,"queue_packets_end":last.queue().packets,
            "queue_owned_bytes_start":first.queue().owned_bytes,"queue_owned_bytes_end":last.queue().owned_bytes,"unaccounted_drops":0,"original_claims":claims,"start_point":start,"end_point":end}));
    }
    Ok(serde_json::json!({"schema":"misaka.rfc02.proof-derived-receipt-windows.v1","contract_root":contract.root()?,
        "cohort_sha256":contract.cohort_sha256,"original_model_roots":contract.original_model_roots,
        "receipt_window_daa":contract.receipt_window_daa,"minimum_independent_operators":contract.minimum_independent_operators,
        "windows":windows,"cumulative_accounting":cumulative,"qualified_native_profile":true,
        "intended_claims_and_queue_accounting_verified":true}))
}
