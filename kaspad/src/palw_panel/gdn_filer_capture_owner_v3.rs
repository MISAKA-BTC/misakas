//! Retain actual served bytes/prompt before handing them to either GDN filer.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeDecodeErrorV1 as E, NativeHeapV1, NativeRetainedV1, reserve_native_requests_v1, retain_native_value_decode_classified_v1,
};
use kaspa_consensus_core::palw_routed_material_v2::PalwMaterialConstructionRequestsV2;
use misaka_palw_base0::qwen36_backend::gdn_capture_owner_v3::binding_from_capture_v3;

type Capture = (NativeRetainedV1<Vec<u8>>, NativeRetainedV1<Vec<u32>>);
type FpCapture = (kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3, Vec<u32>, Vec<u8>);
fn matches(
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    d: &kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2,
) -> bool {
    binding.committed_execution_root == d.execution_root && binding.shape_profile.shape_profile_id() == d.class_id
}
impl PalwPanelService {
    pub(super) fn gdn_capture_for_claim_v3(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        d: &kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2,
        backend: &dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1,
        pooled: &[Vec<u8>],
        domain: Hash64,
    ) -> Result<Option<Capture>, E<String>> {
        for bytes in pooled {
            if d.free_prompt {
                let Some(body) = bytes.strip_prefix(&kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_CAPTURE_V1_MAGIC) else {
                    continue;
                };
                let payload = match retain_native_value_decode_classified_v1::<FpCapture, String>(body, || {
                    borsh::from_slice(body).map_err(|e| e.to_string())
                }) {
                    Ok(value) => value,
                    Err(E::Decode(_)) => continue,
                    Err(error) => return Err(error),
                };
                if payload.0.class_id != d.class_id || payload.0.executor_bond != d.executor_bond.0 || payload.2.is_empty() {
                    continue;
                }
                let form = self.class_prompt_ids_form(d.class_id);
                if kaspa_consensus_core::palw_freeprompt_v3::palw_fp_prompt_ids_admit_v1(&payload.0, &payload.1, form).is_err() {
                    continue;
                }
                let binding = match binding_from_capture_v3(&payload.2) {
                    Ok(value) => value,
                    Err(E::Decode(_)) => continue,
                    Err(error) => return Err(error),
                };
                if !matches(&binding, d) {
                    continue;
                }
                let heap = payload.native_heap_v1("GDN free-prompt material projection");
                let mut q = PalwMaterialConstructionRequestsV2::new();
                q.add_v1(heap.bytes().ok_or_else(|| E::Resource(heap.clone()))?).map_err(E::Resource)?;
                q.vec_v1::<usize>(u64::from(binding.shape_profile.n_ctx)).map_err(E::Resource)?;
                q.vec_v1::<u32>(u64::from(binding.shape_profile.n_ctx)).map_err(E::Resource)?;
                let _prompt = reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
                let material = kaspa_consensus_core::palw_freeprompt_v3::PalwFpMaterialV1 {
                    job: payload.0.clone(),
                    prompt_token_ids: payload.1.clone(),
                };
                let Some(prompt) =
                    Self::fp_prompt_for_job(kaspa_consensus_core::palw_backend::PalwExecutionOpsV1::new(backend), &material, form)
                else {
                    continue;
                };
                let Some(expected) = backend.fp_job_context_for_executed_v1(&payload.0, binding.job_context.exact_decode_tokens)
                else {
                    continue;
                };
                if expected != binding.job_context {
                    continue;
                }
                return Ok(Some((
                    NativeRetainedV1::try_from_clone_v1(&payload.2).map_err(E::Resource)?,
                    NativeRetainedV1::retain_existing_v1(prompt).map_err(E::Resource)?,
                )));
            }
            let binding = match binding_from_capture_v3(bytes) {
                Ok(value) => value,
                Err(E::Decode(_)) => continue,
                Err(error) => return Err(error),
            };
            if !matches(&binding, d) {
                continue;
            }
            let mut q = PalwMaterialConstructionRequestsV2::new();
            q.vec_v1::<usize>(u64::from(binding.shape_profile.n_ctx)).map_err(E::Resource)?;
            q.vec_v1::<u32>(u64::from(binding.shape_profile.n_ctx)).map_err(E::Resource)?;
            let _prompt = reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
            let Some((job, prompt)) =
                self.attempt_job_for_claim(session, backend, domain, d.accepted_block, d.class_id, &d.executor_bond)
            else {
                continue;
            };
            if binding.job_context != job {
                continue;
            }
            let prompt: Vec<u32> = prompt.into_iter().map(|id| id as u32).collect();
            return Ok(Some((
                NativeRetainedV1::try_from_clone_v1(bytes).map_err(E::Resource)?,
                NativeRetainedV1::retain_existing_v1(prompt).map_err(E::Resource)?,
            )));
        }
        Ok(None)
    }
}
