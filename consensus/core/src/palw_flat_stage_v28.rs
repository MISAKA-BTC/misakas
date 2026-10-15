//! Explicit source/court/final contract28. Old14 policy, book bytes, consent18
//! and target7 capacities are never enlarged by this independent namespace.
use crate::{Hash64, palw_flat_execution_v11::NativeExecutionPoliciesV11};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::{Digest, object_id};
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct PalwStageExecutionFenceV28 {
    pub activation:crate::config::params::ForkActivation,
    pub policies:NativeStagePoliciesV28,
}
pub const STAGE_ACTION_FIRST_V28: u8 = 19;
pub const STAGE_ACTION_LAST_V28: u8 = 33;
pub const STAGE_COURT_MAX_BYTES_V28: u64 = 16 << 20;
pub const STAGE_PROOF_PART_MAX_V28: u64 = 32 << 10;
pub const STAGE_PROOF_PARTS_MAX_V28: u16 = 512;
/// Request25 reuses the fixed147-byte coordinate carrier under a new28 action:
/// kinds0..2 are real-PC Header/Subtree/Leaf, kinds3..6 are trace Node/flat
/// hash/flat value/legacy leaf. Trace param=occurrence,layer_code=position,
/// index=(leaf<<16)|node; all coordinates are independently bounded natively.
pub type NativeStageRequestV28=crate::palw_flat_execution_v11::NativeSourceRequestV16;
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct NativeStagePendingV28 {
    pub version:u16,pub id:Digest,pub unit:Digest,pub request:NativeStageRequestV28,
    pub original:Digest,pub consent:Digest,pub contract:Digest,pub source_plan:Digest,pub caller_plan:Digest,
    pub requester:crate::palw_state_v2::PalwBondKeyV2,pub requester_operator:Digest,pub requester_payout:Hash64,
    pub producer:crate::palw_state_v2::PalwBondKeyV2,pub producer_operator:Digest,
    pub claim:Digest,pub pc:Digest,pub bond:u64,pub filed:u64,pub deadline:u64,pub proof_until:u64,pub attempts:u8,pub settled:bool,
}
impl NativeStagePendingV28 {
    pub fn original_id(&self)->Digest{object_id(b"misaka-palw/native/stage-original-source-trace-demand/v28",&(28u16,self.unit,&self.request,self.original,self.consent,self.contract,self.requester,self.requester_operator,self.producer,self.producer_operator,self.claim,self.pc,self.bond,self.filed,self.deadline,self.proof_until))}
}
pub fn stage_source_protocol_root_v28() -> Digest {
    object_id(
        b"misaka-palw/native/stage-explicit-source-protocol/v28",
        &(
            28u16,
            26u16,
            25u16,
            16u64 << 20,
            512u16,
            32u64 << 10,
            misaka_palw_kernel::flat_quota11::source16::SOURCE_ANSWER_MAX_BYTES_V16,
        ),
    )
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStagePoliciesV28 {
    pub version: u16,
    pub network_domain: Digest,
    pub execution: NativeExecutionPoliciesV11,
    pub final_policy: Digest,
    pub physical_service: Digest,
    pub source_protocol: Digest,
    pub checker: u16,
    pub maximum_horizon: u64,
    pub catalog_cap: u8,
    pub max_input_bytes: u32,
}
impl NativeStagePoliciesV28 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/stage-independent-policy/v28", self)
    }
    pub fn check(&self, network_domain: Digest) -> Result<(), String> {
        self.execution.check(network_domain)?;
        if self.version != 28
            || self.network_domain != network_domain
            || self.checker != 25
            || self.final_policy == [0; 64]
            || self.physical_service == [0; 64]
            || self.source_protocol != stage_source_protocol_root_v28()
            || self.catalog_cap == 0
            || self.catalog_cap > 8
            || self.max_input_bytes == 0
            || self.max_input_bytes > 1 << 20
            || self.maximum_horizon == 0
            || self.maximum_horizon >= self.execution.quota.interval_daa
        {
            return Err("stage28 lacks exact source26/checker25/final27/physical policy and interval-bounded class horizon".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStageTemporalV28 {
    pub version: u16,
    pub class: Digest,
    pub maximum_scope: Digest,
    pub maximum_resources: Digest,
    pub source_requests: u32,
    pub source_rounds: u32,
    pub answer_window: u64,
    pub seal_maturity: u64,
    pub source_acquisition: u64,
    pub proof_window: u64,
    pub commit_window: u64,
    pub challenge_window: u64,
    pub tail: u64,
    pub total_horizon: u64,
    pub source_protocol: Digest,
    pub final_policy: Digest,
    pub physical_service: Digest,
}
impl NativeStageTemporalV28 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/stage-before-fund-all-deadlines/v28", self)
    }
    pub fn derive(
        p: &NativeStagePoliciesV28,
        class: Digest,
        scope: Digest,
        resources: Digest,
        independently_derived_source_requests: u32,
    ) -> Result<Self, String> {
        p.check(p.network_domain)?;
        let h = &p.execution.execution.horizons;
        let source_acquisition = (independently_derived_source_requests as u64)
            .checked_mul(h.da_answer)
            .and_then(|n| n.checked_add(h.proof_grace))
            .ok_or("stage28 source adaptive clock overflow")?;
        let proof_window = h.seal_ttl.max(source_acquisition.checked_add(h.seal_maturity).ok_or("stage28 source maturity overflow")?);
        let tail = proof_window
            .checked_add(h.da_answer)
            .and_then(|n| n.checked_add(h.proof_grace))
            .and_then(|n| n.checked_add(h.retention))
            .ok_or("stage28 tail clock overflow")?;
        let total_horizon = h
            .commit_window
            .checked_add(h.challenge_window)
            .and_then(|n| n.checked_add(tail))
            .and_then(|n| n.checked_add(1))
            .ok_or("stage28 total clock overflow")?;
        if independently_derived_source_requests == 0
            || independently_derived_source_requests > 1 << 18
            || total_horizon > p.maximum_horizon
            || class == [0; 64]
            || scope == [0; 64]
            || resources == [0; 64]
        {
            return Err("stage28 original maximum acquisition/court horizon does not fit admitted interval policy".into());
        }
        Ok(Self {
            version: 28,
            class,
            maximum_scope: scope,
            maximum_resources: resources,
            source_requests: independently_derived_source_requests,
            source_rounds: independently_derived_source_requests,
            answer_window: h.da_answer,
            seal_maturity: h.seal_maturity,
            source_acquisition,
            proof_window,
            commit_window: h.commit_window,
            challenge_window: h.challenge_window,
            tail,
            total_horizon,
            source_protocol: p.source_protocol,
            final_policy: p.final_policy,
            physical_service: p.physical_service,
        })
    }
    pub fn deadline(&self, admitted: u64, interval: u64) -> Result<u64, String> {
        let at = admitted.checked_add(self.total_horizon).ok_or("stage28 deadline overflow")?;
        let end = (admitted / interval).checked_add(1).and_then(|n| n.checked_mul(interval)).ok_or("stage28 interval overflow")?;
        if at >= end {
            return Err("stage28 promised acquisition/court/final crosses original quota interval".into());
        }
        Ok(at)
    }
}
/// New version28 wrapper explicitly contains the unchanged canonical old14
/// preparation bytes. Its roots are signed BEFORE producer resources/slots.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StageAcceptSourceV28 {
    pub version: u16,
    pub preparation14: Vec<u8>,
    pub source_plan: Digest,
    pub trace_plan: Digest,
    pub caller_source26: Digest,
    pub temporal28: Digest,
    pub checker25: Digest,
    pub whole_claim: Digest,
    pub final_policy27: Digest,
    pub physical_service: Digest,
    pub fee_plan: Digest,
    pub policy28: Digest,
    pub physical_plan:Digest,
    pub maximum_contract:Digest,
    pub queue_plan:Digest,
    pub queue_policy:Digest,
}
impl StageAcceptSourceV28 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        const TAIL: usize = 14 * 64;
        if raw.len() < 6 + TAIL || raw.len() > 2048 + 6 + TAIL || u16::from_le_bytes(raw[..2].try_into().unwrap()) != 28 {
            return Err("stage28 consent bounded explicit version".into());
        }
        let n = u32::from_le_bytes(raw[2..6].try_into().unwrap()) as usize;
        if n > 2048 || raw.len() != 6 + n + TAIL {
            return Err("stage28 original preparation count differs from exact bounded wrapper".into());
        }
        crate::palw_flat_stage_v14::StageAcceptRequestV14::from_bytes(&raw[6..6 + n])?;
        borsh::from_slice(raw).map_err(|_| "stage28 consent framing".into())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeStageConsentV28 {
    pub version: u16,
    pub id: Digest,
    pub original14: crate::palw_flat_stage_v14::NativeStageAcceptanceV14,
    pub source_plan: Digest,
    pub trace_plan: Digest,
    pub caller_source26: Digest,
    pub temporal28: Digest,
    pub checker25: Digest,
    pub whole_claim: Digest,
    pub final_policy27: Digest,
    pub physical_service: Digest,
    pub fee_plan: Digest,
    pub policy28: Digest,
    pub physical_plan:Digest,
    pub maximum_contract:Digest,
    pub queue_plan:Digest,
    pub queue_policy:Digest,
    pub accepted_block: Hash64,
}
impl NativeStageConsentV28 {
    pub fn root(&self) -> Digest {
        object_id(
            b"misaka-palw/native/stage-original-prefund-consent/v28",
            &(
                self.version,
                self.original14.content_id(),
                (
                    self.source_plan,
                    self.trace_plan,
                    self.caller_source26,
                    self.temporal28,
                    self.checker25,
                    self.whole_claim,
                    self.final_policy27,
                    self.physical_service,
                    self.fee_plan,
                    self.policy28,
                ),
                (self.physical_plan,self.maximum_contract,self.queue_plan,self.queue_policy,self.accepted_block),
            ),
        )
    }
}

#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct NativeStageMaximumV28 {
    pub version:u16,pub class:Digest,pub source:Digest,pub job:Digest,pub admission:Digest,
    pub maximum_scope:Digest,pub maximum_resources:Digest,pub actual_scope:Digest,pub actual_resources:Digest,
    pub quota:Digest,pub source_plan:Digest,pub caller_source:Digest,pub paged_plan:Digest,
    pub source_proof25:Digest,pub temporal:NativeStageTemporalV28,pub final_basis:crate::palw_native_final_v27::NativeFinalClassBasisV27,
    pub final_policy:Digest,pub physical:Digest,pub physical_plan:Digest,pub queue_plan:Digest,pub queue_policy:Digest,pub delivery_fee_terms:Digest,pub producer_delivery_fee_cap:u64,
}
impl NativeStageMaximumV28 {pub fn root(&self)->Digest {object_id(b"misaka-palw/native/stage-complete-maximum-contract/v28",self)}}
/// Public rederivation produces immutable documentary roots. Native additionally
/// authenticates trusted Params authority, the original producer signature,
/// source membership, permanent nonce and genuine coin/quota/physical receipts.
pub struct CheckedNativeStageMaximumV28 {
    record:NativeStageMaximumV28,
    source:misaka_palw_kernel::flat_quota11::source16::SourcePlanV16,
    caller:misaka_palw_kernel::flat_quota11::stage14::source26::CheckedStageSourcePlanV26,
    paged:crate::palw_native_paged_proof_v29::CheckedNativePagedProofPlanV29,
    conditions:crate::palw_native_final_v27::CheckedNativeFinalConditionsV27,
    queue:crate::palw_native_queue_v30::CheckedNativeQueuePlanV30,
}
impl CheckedNativeStageMaximumV28 {
    pub fn record(&self)->&NativeStageMaximumV28 {&self.record}
    pub fn source(&self)->&misaka_palw_kernel::flat_quota11::source16::SourcePlanV16 {&self.source}
    pub fn caller(&self)->&misaka_palw_kernel::flat_quota11::stage14::source26::CheckedStageSourcePlanV26 {&self.caller}
    pub fn paged(&self)->&crate::palw_native_paged_proof_v29::CheckedNativePagedProofPlanV29 {&self.paged}
    pub fn conditions(&self)->&crate::palw_native_final_v27::CheckedNativeFinalConditionsV27 {&self.conditions}
    pub fn carriage(&self)->&crate::palw_native_paged_carriage_v29::CheckedNativePagedCarriagePlanV29 {self.queue.paged_carriage().expect("derived original paged28 queue")}
    pub fn queue(&self)->&crate::palw_native_queue_v30::CheckedNativeQueuePlanV30 {&self.queue}
    pub fn derive(p:&NativeStagePoliciesV28,class:&misaka_palw_kernel::flat_inputs12::bounded28::CheckedStageClassV28,job:&misaka_palw_kernel::flat_inputs12::bounded28::CheckedStageJobV28,quota:&misaka_palw_kernel::flat_quota11::stage14::bounded28::CheckedStageQuotaV28,final_policy:&crate::palw_native_final_v27::NativeFinalPolicyV27,ledger:&misaka_palw_kernel::ledger::LedgerPolicyV1,service:&crate::palw_flat_execution_capacity_v22::NativeCarriageServiceV22,mass:&crate::palw_native_carriage_v22::NativeCarriageMassParamsV22)->Result<Self,String>{
        use misaka_palw_kernel::{flat_quota11::stage14::source26::derive_stage_source_caller_plan_v28,flat_resources10::ResourceAxisV10 as A};
        p.check(p.network_domain)?;
        if class.wire().policy!=p.root()||final_policy.root()!=p.final_policy||service.root()!=p.physical_service||!service.implemented{return Err("stage28 maximum lacks exact independently configured class/Final/actual delivery policy".into());}
        let source=quota.source_plan(class,&p.execution.quota,&p.execution.discovery)?;
        let caller=derive_stage_source_caller_plan_v28(class,job,quota,&p.execution.quota,&p.execution.execution,&p.execution.discovery)?;
        // Only real original weights are in this source court. Actual inputs
        // retain their independent declared namespace and immutable public PC.
        let mut real=class.source().program().v1_view();real.params.truncate(class.source().program().params.len());
        let source_bound=crate::palw_native_source_proof_v25::derive_native_source_proof_bound_v25(&real,class.source().parameters())?;
        let restore=if quota.is_invocation_v32(){quota.restore_bytes().checked_add(32768)}else{class.source_restore_bytes().checked_add(borsh::to_vec(class.wire()).map_err(|e|e.to_string())?.len() as u64).and_then(|n|n.checked_add(u64::try_from(job.input_raw_bytes()).ok()?)).and_then(|n|n.checked_add(32768))}.ok_or("stage28 exact native restore extent overflow")?;
        let scope32=if quota.is_invocation_v32(){Some(misaka_palw_kernel::flat_inputs12::court32::derive_stage_court_scope_v32(class)?)}else{None};
        let (court_bytes,court_work,court_scratch)=scope32.as_ref().map(|s|(s.scope().maximum_bytes,s.scope().maximum_work,s.scope().maximum_scratch)).unwrap_or((0,0,0));
        let paged=crate::palw_native_paged_proof_v29::CheckedNativePagedProofPlanV29::derive(class.maximum_scope(),class.maximum_resources(),&p.execution.quota,court_bytes.max(source_bound.maximum_bytes as u64),court_work.max(source_bound.work),court_scratch.max(source_bound.scratch),restore)?;
        let temporal=NativeStageTemporalV28::derive(p,class.root(),class.maximum_scope().root(),class.maximum_resources().root(),quota.requests_per_owner())?;
        let carriage=crate::palw_native_paged_carriage_v29::CheckedNativePagedCarriagePlanV29::derive(&real,class.source().parameters(),real.params.len(),&paged,class.root(),quota.plan().record().root(),temporal.root(),temporal.answer_window,temporal.proof_window.checked_sub(temporal.seal_maturity).ok_or("stage28 original proof window exhausted")?,service,mass)?;
        let queue=crate::palw_native_queue_v30::CheckedNativeQueuePlanV30::derive_paged(&final_policy.queue_policy,mass,carriage)?;
        let owned=if quota.is_invocation_v32(){Some(quota.plan().record().baseline.clone())}else{None};
        let q=owned.as_ref().map(|q|&q.0);
        let resource_quantity=|a:A|->u128{q.map(|q|q[a as usize]).unwrap_or_else(||class.maximum_resources().quantities.get(a))};
        let cold=[A::Macs,A::ScalarSteps,A::IndexSteps,A::ValidatedElements,A::ParameterHashBytes,A::TraceHashBytes,A::MetadataWork,A::CopiedReferenceBytes,A::RawParameterStorage,A::DeliveryBytes,A::StoredEvidenceBytes,A::CourtWork].into_iter().try_fold(0u128,|n,a|n.checked_add(resource_quantity(a)).ok_or("stage28 cold resource monetary extent overflow"))?;
        let acquisition=caller.request_cost().get(A::MetadataWork).checked_mul(caller.attempts() as u128).ok_or("stage28 complete acquisition monetary overflow")?;
        let work=cold.checked_add(acquisition).and_then(|n|n.checked_add(paged.record().court_work as u128*2)).and_then(|n|n.checked_add(paged.record().guarantee.get(A::MetadataWork))).ok_or("stage28 complete pursuit monetary overflow")?;
        let reference_cost=ledger.dismissal_fee_v1(u64::try_from(work).map_err(|_|"stage28 complete monetary work exceeds public range")?).checked_add(ledger.demand_bond.checked_mul(caller.requests() as u64).ok_or("stage28 complete source bonds overflow")?).ok_or("stage28 full finder cost overflow")?;
        let execution=&p.execution.execution;
        let finder=u64::try_from((execution.required_producer_reserve as u128)*(execution.finder_permille as u128)/1000).map_err(|_|"stage28 finder extent overflow")?;
        let basis=crate::palw_native_final_v27::NativeFinalClassBasisV27{class:class.root(),program:class.source().program_root(),inventory:class.source().inventory_root(),real_parameter:class.source().parameters().root(),original_max_scope:class.maximum_scope().root(),original_max_resource:class.maximum_resources().root(),execution_policy:execution.root(),quota_policy:p.execution.quota.root(),discovery_policy:p.execution.discovery.root(),source_protocol:p.source_protocol,verifier_profile:if quota.is_invocation_v32(){misaka_palw_kernel::flat_inputs12::court32::ordinary_stage_court_id_v32()}else{class.maximum_scope().court_set()},physical_service:p.physical_service,complete_reference_cost_sompi:reference_cost,maximum_producer_gain_sompi:execution.reward_cap.checked_add(execution.external_gain_cap).ok_or("stage28 gain overflow")?,original_producer_reserve_sompi:execution.required_producer_reserve,original_finder_award_sompi:finder,original_default_obligation_sompi:execution.default_obligation,maximum_reference_bytes:u64::try_from(resource_quantity(A::RawParameterStorage).checked_add(resource_quantity(A::ReferenceTensorPayload)).and_then(|n|n.checked_add(resource_quantity(A::StoredEvidenceBytes))).ok_or("stage28 complete reference byte overflow")?).map_err(|_|"stage28 complete reference byte overflow")?,maximum_court_bytes:paged.record().maximum_bytes as u64,maximum_court_work:paged.record().court_work,maximum_court_scratch:paged.record().court_scratch};
        let conditions=final_policy.check_class(p.network_domain,&basis,temporal.source_acquisition.checked_add(temporal.proof_window).ok_or("stage28 complete pursuit clock overflow")?,true,service.implemented)?;
        if execution.horizons.challenge_window<conditions.complete_path_daa(){return Err("stage28 original challenge interval shorter than measured independent complete reference/source/proof/delivery path".into());}
        let delivery_fee_terms=object_id(b"misaka-palw/native/stage-original-burned-delivery-fees/v28",&(28u16,source.root(),caller.root(),object_id(b"misaka-palw/native/stage-original-ledger-terms/v28",ledger),ledger.demand_bond,0u64));
        let record=NativeStageMaximumV28{version:28,class:class.root(),source:class.source().id(),job:job.job().root(),admission:job.admission().root(),maximum_scope:class.maximum_scope().root(),maximum_resources:class.maximum_resources().root(),actual_scope:job.scope().root(),actual_resources:job.resources().root(),quota:quota.root(),source_plan:source.root(),caller_source:caller.root(),paged_plan:paged.record().root(),source_proof25:source_bound.root(),temporal,final_basis:basis,final_policy:p.final_policy,physical:p.physical_service,physical_plan:queue.record().original_carriage,queue_plan:queue.record().root(),queue_policy:queue.record().policy,delivery_fee_terms,producer_delivery_fee_cap:0};
        Ok(Self{record,source,caller,paged,conditions,queue})
    }
}

use misaka_palw_kernel::{flat_inputs12::{bounded28::{CheckedStageBundleV28,check_stage_bundle_v28},source28::{CheckedStageSourceV28,FlatStageSourceWireV28,STAGE_SOURCE_MAX_BYTES_V28}},flat_component_source28::CompleteComponentManifestV28};
use std::io::{Cursor,Read};
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct NativeStageSourceBundleV28 {pub version:u16,pub manifest:Vec<u8>,pub sources:Vec<Vec<u8>>}
impl NativeStageSourceBundleV28 {
    pub fn from_bytes(raw:&[u8])->Result<Self,String>{
        if raw.len()>16<<20||raw.len()<10||raw[..2]!=28u16.to_le_bytes(){return Err("stage28 source bundle bound/version".into());}
        let mut r=Cursor::new(raw);let version=u16::deserialize_reader(&mut r).map_err(|e|e.to_string())?;
        fn blob(r:&mut Cursor<&[u8]>,cap:usize)->Result<Vec<u8>,String>{let n=u32::deserialize_reader(r).map_err(|e|e.to_string())? as usize;if n>cap||n>r.get_ref().len().saturating_sub(r.position() as usize){return Err("stage28 source bundle bounded nested count".into());}let mut b=vec![0;n];r.read_exact(&mut b).map_err(|e|e.to_string())?;Ok(b)}
        let manifest=blob(&mut r,4<<20)?;let n=u32::deserialize_reader(&mut r).map_err(|e|e.to_string())? as usize;if n==0||n>64{return Err("stage28 source stage count".into());}let mut sources=Vec::with_capacity(n);for _ in 0..n{sources.push(blob(&mut r,STAGE_SOURCE_MAX_BYTES_V28)?);}if r.position() as usize!=raw.len(){return Err("stage28 source bundle surplus bytes".into());}Ok(Self{version,manifest,sources})
    }
    pub fn check(&self)->Result<(CheckedStageBundleV28,Vec<CheckedStageSourceV28>),String>{
        let manifest=CompleteComponentManifestV28::from_bytes(&self.manifest).map_err(|e|format!("{e:?}"))?;
        let sources=self.sources.iter().map(|raw|FlatStageSourceWireV28::from_bytes(raw)?.check()).collect::<Result<Vec<_>,String>>()?;
        let checked=check_stage_bundle_v28(&manifest,&sources)?;
        if checked.bytes()!=borsh::to_vec(self).map_err(|e|e.to_string())?.len() as u64{return Err("stage28 original full-source canonical extent changed".into());}
        Ok((checked,sources))
    }
}

/// Dedicated Master32 carrier, inner64 actions34/35/36. The nested body is the
/// old typed Accept28/Fund14/Trace14 grammar, under a NEW explicit action only.
/// `generated` is empty for a not-yet-committed decoder at Accept/Fund, and first
/// fixed by Commit36. Earlier stream outputs must come from genuine native
/// component Finals of this exact master, never caller-supplied assertions.
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct NativeStageInvocationEnvelopeV32 {
    pub version:u16,pub master:Digest,pub stage:u16,pub generated:Vec<u32>,
    pub upstream_jobs:Vec<Digest>,pub upstream_outputs:Vec<u8>,pub body:Vec<u8>,
}
pub const MASTER_INVOCATION_MAX_BYTES_V32:usize=(4<<20)+(256<<10)+8192;
impl NativeStageInvocationEnvelopeV32 {
    pub fn root(&self)->Digest{object_id(b"misaka-palw/native/original-master-invocation-carrier/v32",self)}
    pub fn from_bytes(raw:&[u8])->Result<Self,String>{
        use std::io::{Cursor,Read};
        if raw.len()>MASTER_INVOCATION_MAX_BYTES_V32||raw.len()<84{return Err("master32 invocation bounded envelope".into());}
        let mut r=Cursor::new(raw);
        let version=u16::deserialize_reader(&mut r).map_err(|e|e.to_string())?;
        if version!=32{return Err("master32 invocation version".into());}
        let master=Digest::deserialize_reader(&mut r).map_err(|e|e.to_string())?;
        let stage=u16::deserialize_reader(&mut r).map_err(|e|e.to_string())?;
        fn n(r:&mut Cursor<&[u8]>,cap:usize,width:usize)->Result<usize,String>{let n=u32::deserialize_reader(r).map_err(|e|e.to_string())? as usize;if n>cap||n>r.get_ref().len().saturating_sub(r.position() as usize)/width{return Err("master32 nested count before allocation".into());}Ok(n)}
        fn blob(r:&mut Cursor<&[u8]>,cap:usize)->Result<Vec<u8>,String>{let size=n(r,cap,1)?;let mut v=vec![0;size];r.read_exact(&mut v).map_err(|e|e.to_string())?;Ok(v)}
        let count=n(&mut r,65536,4)?;let mut generated=Vec::with_capacity(count);for _ in 0..count{generated.push(u32::deserialize_reader(&mut r).map_err(|e|e.to_string())?);}
        let count=n(&mut r,64,64)?;let mut upstream_jobs=Vec::with_capacity(count);for _ in 0..count{upstream_jobs.push(Digest::deserialize_reader(&mut r).map_err(|e|e.to_string())?);}
        let upstream_outputs=blob(&mut r,2<<20)?;let body=blob(&mut r,2<<20)?;
        let out=Self{version,master,stage,generated,upstream_jobs,upstream_outputs,body};
        if r.position() as usize!=raw.len()||borsh::to_vec(&out).map_err(|e|e.to_string())?!=raw{return Err("master32 exact canonical invocation envelope".into());}Ok(out)
    }
}
