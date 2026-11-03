// Included in native state_v2. Measurements of real candidate inputs only;
// neither accepted-work evidence nor a paid/source-ready capability.
pub struct PalwNativeOriginalInputContextV19<'a>{
    original_parent:&'a PalwChainStateV2,
    point:PalwBlockContextV2,
    basis:palw_tir_public_fold_v1::NativeFoldInputBasisV12,
}
#[allow(clippy::too_many_arguments)]
pub fn palw_native_original_input_context_v19<'a,'o>(
    original_parent:&'a PalwChainStateV2,point:&PalwBlockContextV2,
    raw_candidates:impl IntoIterator<Item=&'o PalwConsensusObjectV2>,own_work:PalwBlockWorkV3<'_>,
    merged:&[PalwMergedWorkV1<'_>],own_execution_key:Hash64,
)->Result<crate::palw_native_resource_v1::NativeRetainedV1<PalwNativeOriginalInputContextV19<'a>>,PalwStateV2Error>{
    let basis=palw_tir_public_fold_v1::NativeFoldInputBasisV12::from_native_borrowed_candidates_v19(point,raw_candidates,own_work,merged,own_execution_key)?;
    let mut q=crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    q.exact_v1::<PalwNativeOriginalInputContextV19<'a>>(1).map_err(PalwStateV2Error::NativeResource)?;
    let lease=crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    Ok(crate::palw_native_resource_v1::NativeRetainedV1::from_owner_reserved_v1(PalwNativeOriginalInputContextV19{original_parent,point:*point,basis},lease))
}
/// Singleton filtering keeps a partial native state. It cannot become a ready
/// Source8/16 source or survive full restore with an unfinished financial row.
/// Final consensus folds independently rebuild and recheck the complete basis.
#[allow(clippy::too_many_arguments)]
pub fn palw_v2_apply_one_object_with_native_original_input_context_v19(
    base:&PalwChainStateV2,params:&PalwStateParamsV2,ctx:&PalwBlockContextV2,
    object:&PalwConsensusObjectV2,unavailable_abstains:bool,capability_bound:bool,
    uncertified_weightless:bool,da_court:bool,extras:&PalwTransitionExtrasV1,
    context:&PalwNativeOriginalInputContextV19<'_>,
)->Result<crate::palw_native_resource_v1::NativeChainStateV1,PalwStateV2Error>{
    palw_v2_apply_one_object_input_context_inner_v19(base,params,ctx,object,
        unavailable_abstains,capability_bound,uncertified_weightless,da_court,extras,Some(context))
}
