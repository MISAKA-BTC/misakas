//! DEPENDENT A4 OWNER REVIEW ONLY. Actual final-calendar descriptive facts.
//! No wire/row codec, tariff, payment permit, actor assignment or paid standing.
use super::*;
use super::archive_service_view_v11::{PreparedOriginalArchiveServiceViewV11,ActualArchiveActorRoleV11,ActualArchiveActorLiteralV11,MissingFinalArchiveActorScopeV11};
use crate::palw_native_resource_v1::{NativeRetainedV1,TirNativeRequestsV1,NativeCoverageV1,NativeDemandLeaseV1,reserve_native_requests_v1};
use crate::palw_court_service_v1::ServicePacket;
type RefusalV11=FundedOriginalRefusalV1;
fn bad(s:&'static str)->RefusalV11{RefusalV11::Invalid(s)}
fn overflow()->RefusalV11{RefusalV11::Resource(NativeCoverageV1::overflow::<NativePreparedFinalCalendarArchiveFactsV11>("final archive facts"))}
fn add(a:u64,b:u64)->Result<u64,RefusalV11>{a.checked_add(b).ok_or_else(overflow)}
/// Existing canonical body measurement. Domain is descriptive, not an admitted
/// material key. Signature bytes are included by the actual existing serializer.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) struct ActualCanonicalBodyV11 {pub canonical_bytes:u64,pub descriptive_digest:Hash64}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) struct ActualNamedRoleFactV11 {
    pub role:ActualArchiveActorRoleV11,pub bond:PalwBondKeyV2,pub current_operator:Hash64,
    pub credited_daa:Option<u64>,pub existing_literal:ActualCanonicalBodyV11,
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub(super) struct FinalArchiveDescriptiveWorkV11 {
    pub actor_count_passes:u64,pub actual_actor_role_records:u64,pub actual_final_packet_visits:u64,
    pub actual_family_rows:u64,pub existing_canonical_serializer_invocations:u64,
    pub existing_canonical_bytes_serialized_and_hashed:u64,
}
/// Deliberately distinct from BeforeFamilyCalendar facts. No Borsh/Deserialize,
/// Clone, caller-scalar constructor, paid/funded/default permission methods.
pub(super) struct NativePreparedFinalCalendarArchiveFactsV11 {
    scope:(Hash64,Hash64,Hash64,Hash64,Hash64,Hash64,Hash64),accepted_daa:u64,hard_daa:u64,
    packets:Vec<ServicePacket>,actors:Vec<ActualNamedRoleFactV11>,family_rows:Vec<ActualCanonicalBodyV11>,
    binding_body:ActualCanonicalBodyV11,packet_vec_body:ActualCanonicalBodyV11,custody_body:ActualCanonicalBodyV11,
    native_requests_max:u64,work:FinalArchiveDescriptiveWorkV11,
}
/// Fixed-memory streaming: no temporary canonical body Vec or full book copy.
struct CanonicalSinkV11 {state:blake2b_simd::State,bytes:u64}
impl std::io::Write for CanonicalSinkV11 {
        fn write(&mut self,b:&[u8])->std::io::Result<usize>{
            self.bytes=self.bytes.checked_add(b.len() as u64).ok_or_else(||std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
            self.state.update(b);Ok(b.len())
        }
        fn flush(&mut self)->std::io::Result<()>{Ok(())}
}
fn measure<T:borsh::BorshSerialize>(v:&T)->Result<ActualCanonicalBodyV11,RefusalV11>{
    let mut s=CanonicalSinkV11{state:blake2b_simd::Params::new().hash_length(64).key(b"MISAKA-final-archive-facts-v11").to_state(),bytes:0};
    borsh::BorshSerialize::serialize(v,&mut s).map_err(|_|bad("final archive actual canonical serializer"))?;
    let mut digest=[0u8;64];digest.copy_from_slice(s.state.finalize().as_bytes());
    Ok(ActualCanonicalBodyV11{canonical_bytes:s.bytes,descriptive_digest:Hash64::from_bytes(digest)})
}
impl NativePreparedFinalCalendarArchiveFactsV11 {
    pub(super) fn from_native_final_view_v11(view:&PreparedOriginalArchiveServiceViewV11<'_>)->Result<NativeRetainedV1<Self>,RefusalV11>{
        Self::with_reserve(view,reserve_native_requests_v1)
    }
    #[cfg(test)]
    pub(super) fn with_checker_v11(view:&PreparedOriginalArchiveServiceViewV11<'_>,checker:&std::sync::Arc<crate::palw_native_resource_v1::NativeDemandCheckerV1>)->Result<NativeRetainedV1<Self>,RefusalV11>{
        Self::with_reserve(view,|n|checker.reserve_requests_v1(n,0).map(Some))
    }
    fn with_reserve(view:&PreparedOriginalArchiveServiceViewV11<'_>,mut reserve:impl FnMut(u64)->Result<Option<NativeDemandLeaseV1>,NativeCoverageV1>)->Result<NativeRetainedV1<Self>,RefusalV11>{
        // Counting borrows actual sealed native roles; it does not serialize,
        // allocate actor bodies or invent occupants of potential source slots.
        let mut actor_count=0u64;
        view.visit_actual_named_actors_v11(|_|{actor_count=add(actor_count,1)?;Ok(())})?;
        let packet_count=u64::try_from(view.packet_ids_v11().len()).map_err(|_|overflow())?;
        let family_count=u64::try_from(view.actual_family_rows_v11().count()).map_err(|_|overflow())?;
        let mut q=TirNativeRequestsV1::new();
        q.exact_v1::<NativeRetainedV1<Self>>(1).map_err(RefusalV11::Resource)?;
        q.exact_v1::<ServicePacket>(packet_count).map_err(RefusalV11::Resource)?;
        q.exact_v1::<ActualNamedRoleFactV11>(actor_count).map_err(RefusalV11::Resource)?;
        q.exact_v1::<ActualCanonicalBodyV11>(family_count).map_err(RefusalV11::Resource)?;
        // One serializer sink/hash state is alive at a time, no body-sized buffer.
        q.exact_v1::<CanonicalSinkV11>(1).map_err(RefusalV11::Resource)?;
        q.exact_v1::<blake2b_simd::Params>(1).map_err(RefusalV11::Resource)?;
        q.exact_v1::<blake2b_simd::Hash>(1).map_err(RefusalV11::Resource)?;
        q.exact_v1::<[u8;64]>(1).map_err(RefusalV11::Resource)?;
        let own=q.bytes_v1();let lease=reserve(own).map_err(RefusalV11::Resource)?;
        let mut packets=Vec::with_capacity(usize::try_from(packet_count).map_err(|_|overflow())?);
        let mut actors=Vec::with_capacity(usize::try_from(actor_count).map_err(|_|overflow())?);
        let mut family_rows=Vec::with_capacity(usize::try_from(family_count).map_err(|_|overflow())?);
        for id in view.packet_ids_v11(){packets.push(view.packet_v11(id)?.clone());}
        let mut bytes=0u64;
        view.visit_actual_named_actors_v11(|a|{
            let body=match a.actual_literal_v11(){
                ActualArchiveActorLiteralV11::SourcePledge(v)=>measure(v)?,
                ActualArchiveActorLiteralV11::SourceGrant(v)=>measure(v)?,
                ActualArchiveActorLiteralV11::FamilyConsent(v)=>measure(v)?,
            };
            bytes=add(bytes,body.canonical_bytes)?;
            actors.push(ActualNamedRoleFactV11{role:a.role_v11(),bond:a.bond_v11(),current_operator:a.current_bond_v11().operator_id,credited_daa:a.actual_grant_credited_daa_v11(),existing_literal:body});Ok(())
        })?;
        for row in view.actual_family_rows_v11(){let body=measure(row)?;bytes=add(bytes,body.canonical_bytes)?;family_rows.push(body);}
        if actors.len() as u64!=actor_count||family_rows.len() as u64!=family_count{return Err(bad("final archive sealed visitor changed between passes"));}
        let binding_body=measure(view.binding_v11())?;let packet_vec_body=measure(&packets)?;let custody_body=measure(view.original_custody_v11())?;
        for n in [binding_body.canonical_bytes,packet_vec_body.canonical_bytes,custody_body.canonical_bytes]{bytes=add(bytes,n)?;}
        // View-owned requests plus adapter requests only. Other simultaneously
        // held requirements/source/transition leases MUST be added by caller;
        // this is not the whole coordinator or protocol tariff. Upstream
        // leases stay scoped, and our own lease stays with returned facts.
        let native_requests_max=add(view.native_requests_max_v11(),own)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(Self{scope:view.scope_v11(),accepted_daa:view.binding_v11().original_accepted_daa,hard_daa:view.binding_v11().hard_deadline_daa,
            packets,actors,family_rows,binding_body,packet_vec_body,custody_body,native_requests_max,
            work:FinalArchiveDescriptiveWorkV11{actor_count_passes:1,actual_actor_role_records:actor_count,actual_final_packet_visits:packet_count,actual_family_rows:family_count,existing_canonical_serializer_invocations:add(add(actor_count,family_count)?,3)?,existing_canonical_bytes_serialized_and_hashed:bytes}},lease))
    }
    pub(super) fn scope_v11(&self)->(Hash64,Hash64,Hash64,Hash64,Hash64,Hash64,Hash64){self.scope}
    pub(super) fn native_window_v11(&self)->(u64,u64){(self.accepted_daa,self.hard_daa)}
    pub(super) fn packets_v11(&self)->&[ServicePacket]{&self.packets}
    pub(super) fn actors_v11(&self)->&[ActualNamedRoleFactV11]{&self.actors}
    /// Logical reconstructed family rows, not physical AUX192 v2 metadata/status/
    /// completion sidecar codecs and not Source16 bodies. Actor roles can name
    /// the same consent. These categories MUST NOT be summed as storage.
    pub(super) fn logical_family_row_bodies_v11(&self)->&[ActualCanonicalBodyV11]{&self.family_rows}
    pub(super) fn binding_body_v11(&self)->ActualCanonicalBodyV11{self.binding_body}
    pub(super) fn final_packet_vec_body_v11(&self)->ActualCanonicalBodyV11{self.packet_vec_body}
    pub(super) fn custody_body_v11(&self)->ActualCanonicalBodyV11{self.custody_body}
    /// Exactly the existing stored Source16 tag6/tag7 body codecs. No invented
    /// actor row or family logical row is added to the source archive schema.
    pub(super) fn source16_body_facts_v11(&self)->[(u8,ActualCanonicalBodyV11);2]{
        [(6,self.binding_body),(7,self.packet_vec_body)]
    }
    pub(super) fn work_v11(&self)->FinalArchiveDescriptiveWorkV11{self.work}
    pub(super) fn local_native_requests_max_v11(&self)->u64{self.native_requests_max}
    pub(super) fn missing_v11(&self)->&'static [MissingFinalArchiveActorScopeV11]{&[MissingFinalArchiveActorScopeV11::Source16ArchiveStorageFunding,
          MissingFinalArchiveActorScopeV11::ProducerFpLiabilityCoverage,
          MissingFinalArchiveActorScopeV11::NativeMetadataWorkTariff,
          MissingFinalArchiveActorScopeV11::FutureSourceLifecycleCoverage]}
}
/// Normal readonly early return via the proposed A4 owner API. No paid Plan is
/// manufactured, and no admitted payment token is required for this quote.
pub(super) fn quote_final_calendar_archive_facts_v11(b:&TransitionBuilder<'_>,ctx:&PalwBlockContextV2,claim:Hash64,prepared:PreparedOriginalFrameV1)->Result<NativeRetainedV1<NativePreparedFinalCalendarArchiveFactsV11>,RefusalV11>{
    super::quote_prepared_original_final_archive_v11(b,ctx,claim,prepared,NativePreparedFinalCalendarArchiveFactsV11::from_native_final_view_v11)
}
