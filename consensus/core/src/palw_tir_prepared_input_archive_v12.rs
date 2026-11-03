// Included in native palw_tir_public_fold_v1. Codec capacity only: no wire,
// accepted-input verdict, source authority, fee rate or paid standing.
/// Recognized fixed-width signed profile selectors request actual input measurement.
/// Selection alone never grants payment/source authority; native finance must
/// independently verify the complete profile, payer, funding and reservation.
/// Old/shipped paths perform no added global serialization work.
pub(super) fn native_input_measurement_enabled_v12(extras:&PalwTransitionExtrasV1)->bool {
    if original_archive_reservation_proposal_v1::original_archive_profiles_selected_v2(extras) {return true;}
    #[cfg(test)] { extras.original_archive_data_observer_v16 }
    #[cfg(not(test))] { false }
}
fn input_bad_v12(why:&'static str)->PalwStateV2Error { PalwStateV2Error::CarriageInconsistent(why.into()) }
fn input_add_v12(a:u64,b:u64)->Result<u64,PalwStateV2Error> { a.checked_add(b).ok_or(PalwStateV2Error::Overflow("native input footprint")) }
fn input_len_v12<T:borsh::BorshSerialize>(v:&T)->Result<u64,PalwStateV2Error> {
    u64::try_from(borsh::object_length(v).map_err(|_|input_bad_v12("native input canonical codec"))?)
        .map_err(|_|PalwStateV2Error::Overflow("native input codec host width"))
}
fn fp_native_input_len_v12(object:&PalwConsensusObjectV2)->Result<Option<u64>,PalwStateV2Error> {
    let PalwConsensusObjectV2::FreePromptCommitted{claim,job_pin,public_job,eval,..}=object else{return Ok(None)};
    // Existing Input::FreePromptNative tag6, including the object's native-only
    // job/eval fields skipped by the old object codec. Reference codecs match
    // the owned Box values; no input/envelope/object clone is constructed.
    let eval=eval.as_ref().map(|d|(&d.job,&d.tail));
    input_len_v12(&(6u8,claim,job_pin,public_job,eval)).map(Some)
}
/// Sealed from the actual full-fold arguments BEFORE any candidate object.
/// Conservative widths of every work/native-FP input: the unchanged Source8
/// filter may select several same-id candidate objects or rejected work inputs.
/// No block work is declared accepted by this measurement.
#[derive(Clone,Copy)]
pub(super) struct NativeFoldInputBasisV12 { point:PalwBlockContextV2, bytes:u64, records:u64 }
impl NativeFoldInputBasisV12 {
    pub(super) fn from_native_fold_v12(
        point:&PalwBlockContextV2,objects:&[PalwConsensusObjectV2],
        work:PalwBlockWorkV3<'_>,merged:&[PalwMergedWorkV1<'_>],own_key:Hash64,
    )->Result<Self,PalwStateV2Error> {
        Self::from_native_borrowed_candidates_v19(point,objects.iter(),work,merged,own_key)
    }
    pub(super) fn from_native_borrowed_candidates_v19<'o>(
        point:&PalwBlockContextV2,objects:impl IntoIterator<Item=&'o PalwConsensusObjectV2>,
        work:PalwBlockWorkV3<'_>,merged:&[PalwMergedWorkV1<'_>],own_key:Hash64,
    )->Result<Self,PalwStateV2Error> {
        let(mut bytes,mut records)=(0,0);
        for(block,key,work)in std::iter::once((point.block,own_key,work))
            .chain(merged.iter().map(|m|(m.carrying_block,m.execution_key,m.work))) {
            let n=match work {
                PalwBlockWorkV3::None=>None,
                PalwBlockWorkV3::Attempt(e)=>Some(input_len_v12(&(0u8,block,key,e))?),
                PalwBlockWorkV3::ReceiptSpend(s)=>Some(input_len_v12(&(1u8,block,s))?),
            };
            if let Some(n)=n {bytes=input_add_v12(bytes,n)?;records=input_add_v12(records,1)?;}
        }
        for object in objects {if let Some(n)=fp_native_input_len_v12(object)? {bytes=input_add_v12(bytes,n)?;records=input_add_v12(records,1)?;}}
        Ok(Self{point:*point,bytes,records})
    }
}
/// Opaque actual-candidate bound, minted only beside the native original Frame.
/// Exact fixed/local literals plus conservative actual-fold candidate and carrier
/// edge capacity. Never a statement that those records will all be accepted.
#[derive(Clone,Copy)]
pub(super) struct NativePreparedInputArchiveFootprintV12 {
    point:PalwBlockContextV2,claim:Hash64,source:Hash64,allocation:Hash64,resource_plan:Hash64,
    canonical_bytes_max:u64,record_count_max:u64,control_bytes:u64,serialized_literal_bytes:u64,
}
impl NativePreparedInputArchiveFootprintV12 {
    pub(super) fn canonical_bytes_max_v12(&self)->u64 {self.canonical_bytes_max}
    pub(super) fn record_count_max_v12(&self)->u64 {self.record_count_max}
    pub(super) fn control_bytes_v12(&self)->u64 {self.control_bytes}
    /// Descriptive serialized bytes examined by one preparation (includes the
    /// shared actual-fold scan conservatively). Not a measured CPU rate or a
    /// complete traversal tariff. Caller must count invocations and other work.
    pub(super) fn serialized_literal_bytes_v12(&self)->u64 {self.serialized_literal_bytes}
    pub(super) fn scope_v12(&self)->(Hash64,Hash64,Hash64,Hash64,PalwBlockContextV2) {
        (self.claim,self.source,self.allocation,self.resource_plan,self.point)
    }
    pub(super) fn recheck_actual_v12(&self,claim:Hash64,point:PalwBlockContextV2,inputs:&[crate::palw_claim_local_v1::PalwClaimLocalInputV1])->Result<(),PalwStateV2Error> {
        if (claim,point)!=(self.claim,self.point) {return Err(input_bad_v12("native input footprint actual birth/claim differs"));}
        let count=u64::try_from(inputs.len()).map_err(|_|PalwStateV2Error::Overflow("native input actual count"))?;
        if count>self.record_count_max || input_len_v12(&inputs)?>self.canonical_bytes_max {
            return Err(input_bad_v12("actual Source8 filtered input generation exceeds native candidate footprint"));
        }
        Ok(())
    }
}
fn prepare_native_input_footprint_v12(
    b:&TransitionBuilder<'_>,ctx:&PalwBlockContextV2,frame:&Frame,object:&PalwConsensusObjectV2,
)->Result<Option<NativePreparedInputArchiveFootprintV12>,PalwStateV2Error> {
    if !native_input_measurement_enabled_v12(b.extras) {return Ok(None);}
    let Some(basis)=b.claim_local_input_basis_v12 else{return Ok(None)};
    if basis.point!=*ctx {return Err(input_bad_v12("native input basis belongs to another executing point"));}
    let p=frame.plan_v1();let original=frame.original_v1();
    let PalwConsensusObjectV2::FreePromptCommitted{claim,class_id,bond,trace_root,output_root,execution_root,public_job,..}=object else {
        return Err(input_bad_v12("native original input footprint requires actual FP object"));
    };
    if *claim!=p.original_claim || p.terms.accepted_daa!=ctx.daa_score
        || *class_id!=original.job.class_id || *bond!=frame.custody_v1().pledge_v1().producer
        || (*trace_root,*output_root,*execution_root)!=(original.trace_root,original.output_root,original.execution_root)
        || public_job.as_deref()!=Some(original) {
        return Err(input_bad_v12("native input footprint changes actual original subject"));
    }
    let(mut bytes,mut records)=(basis.bytes,basis.records);
    macro_rules! one {($value:expr)=>{{bytes=input_add_v12(bytes,input_len_v12(&$value)?)?;records=input_add_v12(records,1)?;}};}
    if let Some(v)=b.extras.prefix_dependencies_v1.get(claim) {one!((7u8,claim,v));}
    if let Some(v)=b.extras.history_custody_pledges.get(claim) {one!((2u8,claim,v));}
    if let Some(v)=b.extras.public_original_bindings.get(claim) {one!((3u8,claim,v));}
    if let Some(v)=b.extras.producer_custody_delegations.get(claim) {one!((4u8,claim,v));}
    if let Some(v)=b.extras.tir_public_originals.get(claim) {one!((5u8,claim,v));}
    // One accepted edge has Input tag8 + literal consumer/source Hash64 values.
    // Parent winning-carrier ambiguity, self edges and duplicates only reduce
    // the actual filtered set. Same-block new winner rows are not assumed here.
    let control_bytes=input_len_v12(object)?;
    let serialized_literal_bytes=input_add_v12(bytes,control_bytes)?;
    let edges=b.extras.tir_funded_originals.get(claim).map_or(0,|p|p.carrier().inputs.len())
        .checked_add(b.extras.carrier_input_spends.get(claim).map_or(0,Vec::len))
        .ok_or(PalwStateV2Error::Overflow("native carrier input footprint"))?;
    let edges=u64::try_from(edges).map_err(|_|PalwStateV2Error::Overflow("native carrier input count"))?;
    bytes=input_add_v12(bytes,edges.checked_mul(129).ok_or(PalwStateV2Error::Overflow("native carrier input bytes"))?)?;
    records=input_add_v12(records,edges)?;
    if records>u64::from(u32::MAX) {return Err(input_bad_v12("native input vector exceeds existing canonical count"));}
    Ok(Some(NativePreparedInputArchiveFootprintV12{point:*ctx,claim:*claim,source:p.source_id,allocation:p.allocation_id,resource_plan:p.resource_plan_id,
        canonical_bytes_max:input_add_v12(4,bytes)?,record_count_max:records,control_bytes,serialized_literal_bytes}))
}
impl PreparedOriginalRequirementsV1 {
    pub(super) fn input_archive_footprint_v12(&self)->Option<&NativePreparedInputArchiveFootprintV12> {
        self.geometry.prepared.input_footprint_v12.as_ref()
    }
}
/// The same native function and immutable extras/basis are used before install
/// and after filtering. No new body/schema is stored and no candidate record is
/// treated as acceptance authority. Partial read-only builders remain missing.
pub(super) fn recheck_source_input_generation_v12(
    b:&TransitionBuilder<'_>,ctx:&PalwBlockContextV2,id:Hash64,object:&PalwConsensusObjectV2,
    inputs:&[crate::palw_claim_local_v1::PalwClaimLocalInputV1],
)->Result<(),PalwStateV2Error> {
    if !native_input_measurement_enabled_v12(b.extras) || b.claim_local_input_basis_v12.is_none() {return Ok(());}
    // Existing Source8 supports older/private stage2 combinations with no full
    // native frame. Extra-literal presence never creates a new frame obligation.
    let Some(Row::TirClaim(frame))=b.state.staged_court_row_v1(&Key::TirClaim(id)) else{return Ok(())};
    let p=frame.plan_v1();
    let Some(book)=b.state.court_service_state_v1() else{return Ok(())};
    let Some(binding)=book.claims.get(&id) else{return Ok(())};
    if (binding.claim,binding.source_id,binding.allocation_id,binding.original_accepted_daa)
        !=(id,p.source_id,p.allocation_id,ctx.daa_score) || p.terms.accepted_daa!=ctx.daa_score {
        return Ok(()); // Not this actual full-frame/binding generation.
    }
    let footprint=prepare_native_input_footprint_v12(b,ctx,frame,object)?.ok_or_else(||input_bad_v12("actual full fold lost native input basis"))?;
    footprint.recheck_actual_v12(id,*ctx,inputs)
}

#[cfg(test)]
fn assert_native_input_footprint_actual_v12(
    bound:&NativePreparedInputArchiveFootprintV12,id:Hash64,point:PalwBlockContextV2,
    actual:&[crate::palw_claim_local_v1::PalwClaimLocalInputV1],object:&PalwConsensusObjectV2,
) {
    assert_eq!(bound.control_bytes_v12(),borsh::object_length(object).unwrap() as u64);
    assert!(bound.record_count_max_v12()>=actual.len() as u64);
    assert!(bound.canonical_bytes_max_v12()>=borsh::object_length(&actual).unwrap() as u64);
    let mut short=*bound;short.canonical_bytes_max=borsh::object_length(&actual).unwrap() as u64-1;
    assert!(short.recheck_actual_v12(id,point,actual).is_err(),"actual filtered bytes cannot fit a shorter native capacity");
    assert!(bound.recheck_actual_v12(Hash64::from_u64_word(0xBAD1),point,actual).is_err(),"same byte count confers no foreign source authority");
    let mut wrong=point;wrong.blue_score+=1;
    assert!(bound.recheck_actual_v12(id,wrong,actual).is_err(),"footprint binds exact actual executing point");
}
