//! A4 OWNER REVIEW PROPOSAL ONLY. Borrowed native final-calendar data, not paid
//! standing. Construct inside the existing coordinator's pre-debit lifetime.
use super::*;
use crate::palw_native_resource_v1::{NativeRetainedV1,TirNativeRequestsV1,reserve_native_requests_v1,BTreeNodeEnvelopeV1};
use crate::palw_court_service_state_v2::{CourtServiceStateV2,ServiceClaimBinding};
use crate::palw_court_service_v1::ServicePacket;
type FamilyPlanV11=super::super::palw_court_family_supplier_fold_v1::FamilySupplierPlanV1;
type RefusalV11=FundedOriginalRefusalV1;
fn invalid_v11(s:&'static str)->RefusalV11{RefusalV11::Invalid(s)}
fn resource_v11(e:crate::palw_native_resource_v1::NativeCoverageV1)->RefusalV11{RefusalV11::State(PalwStateV2Error::NativeResource(e))}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum ActualArchiveActorRoleV11 {
    LegacySourceProducer,CreditedSourceCustodian,
    FamilySupplier(crate::palw_court_family_supplier_v1::FamilySupplierRoleV1),
    FamilySponsor(crate::palw_court_family_supplier_v1::FamilySupplierRoleV1),
}
/// Existing actual signed codecs only; no invented whole-actor persisted row.
pub(super) enum ActualArchiveActorLiteralV11<'a> {
    SourcePledge(&'a crate::palw_tir_public_custody_v1::PalwTirPublicCustodyPledgeV1),
    SourceGrant(&'a crate::palw_tir_public_custody_v1::PalwTirPublicCustodyGrantV1),
    FamilyConsent(&'a crate::palw_court_family_supplier_v1::SignedFamilySupplierV1),
}
pub(super) struct ActualArchiveNamedActorV11<'a> {
    role:ActualArchiveActorRoleV11,bond:PalwBondKeyV2,credited_daa:Option<u64>,current:&'a PalwBondStateV2,literal:ActualArchiveActorLiteralV11<'a>,
}
impl<'a> ActualArchiveNamedActorV11<'a> {
    pub(super) fn role_v11(&self)->ActualArchiveActorRoleV11{self.role}
    pub(super) fn bond_v11(&self)->PalwBondKeyV2{self.bond}
    pub(super) fn actual_grant_credited_daa_v11(&self)->Option<u64>{self.credited_daa}
    pub(super) fn current_bond_v11(&self)->&'a PalwBondStateV2{self.current}
    pub(super) fn actual_literal_v11(&self)->&ActualArchiveActorLiteralV11<'a>{&self.literal}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum MissingFinalArchiveActorScopeV11 {
    Source16ArchiveStorageFunding,ProducerFpLiabilityCoverage,
    NativeMetadataWorkTariff,FutureSourceLifecycleCoverage,
}
/// Private fields, no Clone/Deserialize/raw scalar constructor or readiness.
/// The actual complete native book remains borrowed and cannot escape callback.
pub(super) struct PreparedOriginalArchiveServiceViewV11<'a> {
    requirements:&'a PreparedOriginalRequirementsV1,state:&'a PalwChainStateV2,
    binding:&'a ServiceClaimBinding,book:&'a crate::palw_court_service_v1::ServiceReservations,
    family:&'a [FamilyPlanV11],derivation:&'a crate::palw_funded_original_admission_v1::FundedAdmissionDerivationV1,scope:(Hash64,Hash64,Hash64,Hash64,Hash64,Hash64,Hash64),native_requests:u64,
}
impl<'a> PreparedOriginalArchiveServiceViewV11<'a> {
    pub(super) fn from_native_calendar_v11(
        requirements:&'a PreparedOriginalRequirementsV1,b:&'a TransitionBuilder<'_>,ctx:&PalwBlockContextV2,
        family:&'a [FamilyPlanV11],derivation:&'a crate::palw_funded_original_admission_v1::FundedAdmissionDerivationV1,
        complete:&'a CourtServiceStateV2,
    )->Result<NativeRetainedV1<Self>,RefusalV11> {
        let p=requirements.frame_v1().plan_v1();
        requirements.geometry.prepared.check_preclaim_context_v1(b,ctx).map_err(geometry_refusal)?;
        let graph=requirements.source_graph_v1().map_err(requirements_refusal)?;
        let source=requirements.geometry.source_v1().source_id_v1();
        let binding=complete.claims.get(&p.original_claim).ok_or_else(||invalid_v11("archive view missing actual final claim binding"))?;
        if (binding.claim,binding.allocation_id,binding.source_id,binding.original_accepted_daa,binding.hard_deadline_daa)
            !=(p.original_claim,p.allocation_id,source,ctx.daa_score,derivation.hard_daa)
            ||p.source_id!=source||graph.source_v3()!=source {
            return Err(invalid_v11("archive view substitutes actual prepared source/allocation/calendar"));
        }
        let mut count=derivation.signer_packets.len() as u64;
        for n in [derivation.source_packets.len() as u64,1,derivation.retention_tail_v2.len() as u64] {
            count=count.checked_add(n).ok_or_else(||resource_v11(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<Self>("archive view packet count")))?;
        }
        for f in family {count=count.checked_add(f.packets.len() as u64).ok_or_else(||resource_v11(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<Self>("archive view family count")))?;}
        let mut q=TirNativeRequestsV1::new();q.exact_v1::<NativeRetainedV1<Self>>(1).map_err(resource_v11)?;
        q.exact_v1::<BTreeNodeEnvelopeV1<[u8;64],()>>(count).map_err(resource_v11)?;
        let owner_requests=q.bytes_v1();let lease=reserve_native_requests_v1(owner_requests).map_err(resource_v11)?;
        let windows=family.iter().try_fold(0u64,|n,f|n.checked_add(f.windows.len() as u64))
            .ok_or_else(||resource_v11(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<Self>("archive view material windows")))?;
        let native_requests=windows.checked_mul(std::mem::size_of::<(u64,u64)>() as u64).and_then(|n|n.checked_add(owner_requests))
            .ok_or_else(||resource_v11(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<Self>("archive view material scratch")))?;
        let mut expected=std::collections::BTreeSet::new();
        let barrier=super::super::palw_court_family_supplier_fold_v1::family_material_barrier_v2(family).map_err(RefusalV11::State)?;
        for packet in derivation.signer_packets.iter()
            .chain(derivation.source_packets.iter())
            .chain(std::iter::once(&derivation.retention_packet)).chain(derivation.retention_tail_v2.iter()) {
            if !expected.insert(packet.id){return Err(invalid_v11("archive view duplicates native baseline packet"));}
            let actual=complete.book.packet(&packet.id).ok_or_else(||invalid_v11("archive view lost final native packet"))?;
            let release=if barrier!=0&&packet.cost.io>0{packet.release_daa.max(barrier)}else{packet.release_daa};
            if (actual.id,actual.release_daa,actual.service_before,actual.cost)!=(packet.id,release,packet.service_before,packet.cost) {
                return Err(invalid_v11("archive view baseline does not match actual material-barrier calendar"));
            }
        }
        // Reuse the already-admitted set to verify the native branch-to-packet
        // bijection without allocating a second whole-universe workspace.
        for (_,id) in &derivation.branches {
            if !expected.remove(id){return Err(invalid_v11("archive view native branch is duplicated or outside baseline"));}
        }
        if !expected.is_empty(){return Err(invalid_v11("archive view native branch map omits baseline packet"));}
        // One admitted set is reused for each family and then the full union.
        expected.clear();
        for f in family {
            let t=&f.row.consent.terms;
            if t.claim!=p.original_claim||t.source!=source||t.relative_graph!=graph.scope_v3()
                ||f.row.allocation!=p.allocation_id||f.row.accepted_daa!=ctx.daa_score {
                return Err(invalid_v11("archive view family plan belongs to another actual source"));
            }
            // Reuse the admitted union workspace to validate exact signed
            // transfer membership, before rebuilding the full packet union. Capacity
            // is bounded BEFORE inserting any target IDs; no duplicate index.
            let target_count=f.row.targets.iter().try_fold(0u64,|n,t|n.checked_add(t.packets.len() as u64))
                .ok_or_else(||resource_v11(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<Self>("archive view target membership count")))?;
            if target_count>f.packets.len() as u64{return Err(invalid_v11("archive view target membership exceeds native packet universe"));}
            for target in &f.row.targets {for id in &target.packets {
                if !expected.insert(*id){return Err(invalid_v11("archive view duplicate or foreign target packet membership"));}
            }}
            for packet in &f.packets {if packet.cost.io>0&&!expected.remove(&packet.id){
                return Err(invalid_v11("archive view native transfer lacks unique signed target"));
            }}
            if !expected.is_empty(){return Err(invalid_v11("archive view orphan or holding packet in signed target membership"));}
            for packet in &f.packets {
                if !expected.insert(packet.id){return Err(invalid_v11("archive view duplicate baseline/family packet identity"));}
                if complete.book.packet(&packet.id)!=Some(packet) {
                    return Err(invalid_v11("archive view family packet is not the actual final primary calendar"));
                }
            }
            expected.clear();
        }
        for (_,id) in &derivation.branches {
            if !expected.insert(*id){return Err(invalid_v11("archive view duplicate native branch union"));}
        }
        for packet in family.iter().flat_map(|f|&f.packets) {
            if !expected.insert(packet.id){return Err(invalid_v11("archive view duplicate baseline/family packet union"));}
        }
        if expected!=binding.packet_ids{return Err(invalid_v11("archive view final binding is not exact native packet union"));}
        let source_actor=requirements.actors.source_actor_v1();
        let current=b.state.bond(&source_actor.bond).ok_or_else(||invalid_v11("archive view source actor absent"))?;
        if current.pubkey!=requirements.frame_v1().original_v1().job.executor_pubkey||current.operator_id!=source_actor.operator
            ||!matches!(current.status,PalwBondStatusV2::Active) {
            return Err(invalid_v11("archive view current source actor differs from native prepared pledge"));
        }
        Ok(NativeRetainedV1::from_owner_reserved_v1(Self {requirements,state:&b.state,binding,book:&complete.book,family,derivation,scope:(p.original_claim,source,p.allocation_id,p.resource_plan_id,p.rules_id,graph.scope_v3(),graph.absolute_scope_v3()),native_requests},lease))
    }
    pub(super) fn binding_v11(&self)->&ServiceClaimBinding{self.binding}
    pub(super) fn native_requirements_for_archive_v1(&self)->&PreparedOriginalRequirementsV1{self.requirements}
    pub(super) fn native_calendar_for_archive_v1(&self)->&crate::palw_court_service_v1::ServiceReservations{self.book}
    pub(super) fn base_capital_for_archive_v1(&self)->u64{self.derivation.capital_sompi}
    pub(super) fn original_custody_v11(&self)->&crate::palw_tir_public_custody_v1::PalwTirPublicCustodyContractV1 {
        self.requirements.frame_v1().custody_v1()
    }
    pub(super) fn actual_family_rows_v11(&self)->impl Iterator<Item=&crate::palw_court_family_supplier_v1::FamilySupplierRowV1> {
        self.family.iter().map(|f|&f.row)
    }
    pub(super) fn scope_v11(&self)->(Hash64,Hash64,Hash64,Hash64,Hash64,Hash64,Hash64) {
        self.scope
    }
    /// Exact canonical BTreeSet order used by source16's Vec<ServicePacket>.
    pub(super) fn packet_ids_v11(&self)->&std::collections::BTreeSet<[u8;64]>{&self.binding.packet_ids}
    pub(super) fn packet_v11(&self,id:&[u8;64])->Result<&ServicePacket,RefusalV11>{
        if !self.binding.packet_ids.contains(id){return Err(invalid_v11("archive view query is outside actual binding"));}
        self.book.packet(id).ok_or_else(||invalid_v11("archive view actual final packet unavailable"))
    }
    pub(super) fn native_requests_max_v11(&self)->u64{self.native_requests}
    pub(super) fn missing_actor_scope_v11(&self)->&'static [MissingFinalArchiveActorScopeV11]{
        &[MissingFinalArchiveActorScopeV11::Source16ArchiveStorageFunding,
          MissingFinalArchiveActorScopeV11::ProducerFpLiabilityCoverage,
          MissingFinalArchiveActorScopeV11::NativeMetadataWorkTariff,
          MissingFinalArchiveActorScopeV11::FutureSourceLifecycleCoverage]
    }
    pub(super) fn visit_actual_named_actors_v11(&self,mut visit:impl FnMut(ActualArchiveNamedActorV11<'_>)->Result<(),RefusalV11>)->Result<(),RefusalV11>{
        let custody=self.requirements.frame_v1().custody_v1();let source=custody.pledge_v1().producer;
        let bond=self.state.bond(&source).ok_or_else(||invalid_v11("archive view source actor missing"))?;
        visit(ActualArchiveNamedActorV11 {role:ActualArchiveActorRoleV11::LegacySourceProducer,bond:source,credited_daa:None,current:bond,literal:ActualArchiveActorLiteralV11::SourcePledge(custody.pledge_v1())})?;
        custody.visit_original_actor_grants_v11(|actor,credited,grant|{
            let bond=self.state.bond(&actor).ok_or_else(||invalid_v11("archive view credited source custodian missing"))?;
            visit(ActualArchiveNamedActorV11 {role:ActualArchiveActorRoleV11::CreditedSourceCustodian,bond:actor,credited_daa:Some(credited),current:bond,literal:ActualArchiveActorLiteralV11::SourceGrant(grant)})
        })?;
        for f in self.family {
            let consent=&f.row.consent;let t=&consent.terms;
            for (actor,role) in [(t.supplier,ActualArchiveActorRoleV11::FamilySupplier(t.role)),(t.sponsor,ActualArchiveActorRoleV11::FamilySponsor(t.role))] {
                let bond=self.state.bond(&actor).ok_or_else(||invalid_v11("archive view actual family actor absent"))?;
                visit(ActualArchiveNamedActorV11 {role,bond:actor,credited_daa:None,current:bond,literal:ActualArchiveActorLiteralV11::FamilyConsent(consent)})?;
            }
        }
        Ok(())
    }
}

include!("palw_tir_prepared_branch_obligations_v11.rs");
