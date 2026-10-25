//! Exact registered/layout/prompt copies retained from planning through work.
use kaspa_consensus_core::{
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1, reserve_native_requests_v1,
    },
    palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
    palw_tir_admission_v1::PalwTirClassRecordV1,
    palw_tir_class_v1::PalwTirLayoutV1,
};
pub(super) struct ConeInputsV2 {
    pub(super) registered: PalwTirClassRecordV1,
    pub(super) layout: PalwTirLayoutV1,
    pub(super) prompt: Vec<u32>,
}
/// The complete boxed payload drops before its request ticket, including the
/// program Arc retained independently of the original accepted-state lifetime.
pub(super) struct OwnedConeInputsV2 {
    payload: Box<ConeInputsV2>,
    _lease: Option<NativeDemandLeaseV1>,
}
impl std::ops::Deref for OwnedConeInputsV2 {
    type Target = ConeInputsV2;
    fn deref(&self) -> &Self::Target {
        &self.payload
    }
}
#[cfg(test)]
thread_local! {static STAGES:std::cell::Cell<(u64,u64)>=const {std::cell::Cell::new((0,0))};}
impl OwnedConeInputsV2 {
    pub(super) fn requests_v2(
        registered: &PalwTirClassRecordV1,
        layout: &PalwTirLayoutV1,
        prompt: &Vec<u32>,
    ) -> Result<u64, NativeCoverageV1> {
        let mut q = PalwMaterialConstructionRequestsV2::new();
        q.add_v1(std::mem::size_of::<ConeInputsV2>() as u64)?;
        q.add_v1(std::mem::size_of::<Self>() as u64)?;
        for bound in [
            registered.native_heap_v1("Cone registered copy/shared program"),
            layout.native_heap_v1("Cone layout copy"),
            prompt.native_heap_v1("Cone prompt copy"),
        ] {
            let bytes = bound.bytes().ok_or(bound)?;
            q.add_v1(bytes)?;
        }
        Ok(q.bytes_v1())
    }
    pub(super) fn try_clone_v2(
        registered: &PalwTirClassRecordV1,
        layout: &PalwTirLayoutV1,
        prompt: &Vec<u32>,
    ) -> Result<Box<Self>, NativeCoverageV1> {
        let bytes = Self::requests_v2(registered, layout, prompt)?;
        #[cfg(test)]
        STAGES.with(|s| {
            let (a, b) = s.get();
            s.set((a + 1, b));
        });
        let lease = reserve_native_requests_v1(bytes)?;
        let value = Box::new(Self {
            payload: Box::new(ConeInputsV2 { registered: registered.clone(), layout: layout.clone(), prompt: prompt.clone() }),
            _lease: lease,
        });
        #[cfg(test)]
        STAGES.with(|s| {
            let (a, b) = s.get();
            s.set((a, b + 1));
        });
        Ok(value)
    }
}
/// The real planning door preserves a typed local refusal and starts no work.
pub(super) fn prepare_for_case_v2(
    registered: &PalwTirClassRecordV1,
    layout: &PalwTirLayoutV1,
    case: &super::OwnerCaseV1,
    refusal: &mut Option<NativeDecodeErrorV1<String>>,
) -> Option<Box<OwnedConeInputsV2>> {
    match OwnedConeInputsV2::try_clone_v2(registered, layout, &case.prompt) {
        Ok(inputs) => Some(inputs),
        Err(why) => {
            *refusal = Some(NativeDecodeErrorV1::Resource(why));
            None
        }
    }
}
pub(super) async fn run_owned_inputs_v2<T: Send + 'static, R: Send + 'static>(
    source: NativeRetainedV1<T>,
    inputs: Box<OwnedConeInputsV2>,
    work: impl FnOnce(&T, &ConeInputsV2) -> R + Send + 'static,
) -> Result<R, tokio::task::JoinError> {
    super::source_owned::run_owned_source_v2(source, move |source| {
        let inputs = inputs; // whole capture, including ticket, under cancellation
        work(source, &inputs)
    })
    .await
}
#[cfg(test)]
#[path = "palw_tir_cone_inputs_owned_tests_v2.rs"]
mod tests;
