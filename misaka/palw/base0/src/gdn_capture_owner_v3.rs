//! Before-allocation admission for the production GDN accuser's actual capture.
//! Logical construction bounds only; no physical RSS or calibrated fee claim.
use super::*;
use crate::gdn_evidence_v2::Base0GdnOwnedMaterialV2;
use crate::produce::{Base0FpMaterialV2, Base0RetainedMaterialV1};
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeCoverageV1 as C, NativeDecodeErrorV1 as E, NativeHeapV1, NativeRetainedV1, reserve_native_requests_v1,
    retain_native_value_decode_classified_v1,
};

fn invalid(why: impl Into<String>) -> E<String> {
    E::Decode(why.into())
}
fn capacity(why: &str) -> E<String> {
    E::Resource(C::unsupported::<Base0GdnOwnedMaterialV2>("GDN capture construction", why))
}
enum Capture {
    Dense(NativeRetainedV1<Base0RetainedMaterialV1>),
    Folded(NativeRetainedV1<Base0FpMaterialV2>),
}
impl Capture {
    fn binding(&self) -> &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2 {
        match self {
            Self::Dense(m) => &m.0,
            Self::Folded(m) => &m.binding,
        }
    }
    fn heap(&self) -> Result<u64, E<String>> {
        let h = match self {
            Self::Dense(m) => m.native_heap_v1("GDN decoded dense capture"),
            Self::Folded(m) => m.native_heap_v1("GDN decoded folded capture"),
        };
        h.bytes().ok_or(E::Resource(h))
    }
}
fn decode(bytes: &[u8]) -> Result<Capture, E<String>> {
    if let Some(body) = bytes.strip_prefix(&crate::produce::PALW_BASE0_FP_MATERIAL_MAGIC_V2) {
        let m = retain_native_value_decode_classified_v1::<Base0FpMaterialV2, String>(body, || {
            borsh::from_slice(body).map_err(|e| e.to_string())
        })?;
        if m.version != crate::produce::PALW_BASE0_FP_MATERIAL_VERSION_V2 {
            return Err(invalid("GDN folded capture version"));
        }
        m.step_tree.validate_v1().map_err(|e| invalid(format!("GDN folded tree: {e:?}")))?;
        Ok(Capture::Folded(m))
    } else {
        retain_native_value_decode_classified_v1::<Base0RetainedMaterialV1, String>(bytes, || {
            borsh::from_slice(bytes).map_err(|e| e.to_string())
        })
        .map(Capture::Dense)
    }
}
/// A cheap retained binding read for selecting a capture before copying it.
/// Malformed folded input never falls through to the dense decoder.
pub fn binding_from_capture_v3(
    bytes: &[u8],
) -> Result<NativeRetainedV1<kaspa_consensus_core::palw_step_leg::PalwStepBindingV2>, E<String>> {
    let input = decode(bytes)?;
    verify_binding(input.binding())?;
    NativeRetainedV1::try_from_clone_v1(input.binding()).map_err(E::Resource)
}
fn verify_binding(binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2) -> Result<(), E<String>> {
    let bytes = borsh::object_length(binding).map_err(|e| invalid(e.to_string()))? as u64;
    let _hash = reserve_native_requests_v1(
        bytes.checked_mul(2).and_then(|n| n.checked_add(2048)).ok_or_else(|| capacity("GDN binding hash overflow"))?,
    )
    .map_err(E::Resource)?;
    kaspa_consensus_core::palw_step_leg::verify_binding_v1(binding).map(|_| ()).map_err(|e| invalid(format!("GDN capture binding: {e:?}")))
}
impl NativeHeapV1 for Base0GdnOwnedMaterialV2 {
    fn native_heap_v1(&self, path: &str) -> C {
        self.binding
            .native_heap_v1(path)
            .add::<Self>(self.tiles.leaves.native_heap_v1(path), path)
            .add::<Self>(self.tiles.tiles.native_heap_v1(path), path)
            .add::<Self>(self.checkpoint_leaves.native_heap_v1(path), path)
            .add::<Self>(self.served_chunks.native_heap_v1(path), path)
            .add::<Self>(self.prompt_token_ids.native_heap_v1(path), path)
            .add::<Self>(self.output_token_ids.native_heap_v1(path), path)
    }
}

impl Qwen36Backend {
    /// Own-native execution/class identities are supplied by the consumer's
    /// accepted claim view, never by the served capture. The input remains
    /// reserved while projections, optional folded replay and output handoff run.
    pub fn gdn_accuser_view_retained_v3(
        &self,
        bytes: &[u8],
        carried: Option<&[u32]>,
        execution: Hash64,
        class: Hash64,
    ) -> Result<NativeRetainedV1<Base0GdnOwnedMaterialV2>, E<String>> {
        let input = decode(bytes)?;
        let b = input.binding();
        verify_binding(b)?;
        let profile = self.profile.as_ref().ok_or_else(|| invalid("GDN backend has no registered profile"))?;
        if self.plan.is_none()
            || *profile != b.shape_profile
            || profile.shape_profile_id() != class
            || b.committed_execution_root != execution
        {
            return Err(invalid("GDN capture substitutes own native execution/class or registered backend profile"));
        }
        crate::fp_interval::base0_whole_capture_refusal_v1(b.step_leaf_count, self.materialize_cap(), self.step_ladder_cap())
            .map_or(Ok(()), |why| Err(capacity(&why)))?;
        let positions = u64::from(b.job_context.declared_prefill_tokens)
            .checked_add(u64::from(b.job_context.exact_decode_tokens.saturating_sub(1)))
            .ok_or_else(|| capacity("GDN position count overflow"))?;
        if positions > u64::from(profile.n_ctx) || b.job_context.declared_prefill_tokens == 0 || b.job_context.exact_decode_tokens == 0
        {
            return Err(invalid("GDN job exceeds its registered context or is empty"));
        }
        let prompt = carried.unwrap_or(match &input {
            Capture::Folded(m) => &m.prompt_token_ids,
            Capture::Dense(_) => &[],
        });
        if !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
            self.prompt_ids_form,
            prompt,
            &b.job_context.prompt_token_ids_hash,
        ) {
            return Err(invalid("GDN capture prompt is not the committed job"));
        }
        let mut q = kaspa_consensus_core::palw_routed_material_v2::PalwMaterialConstructionRequestsV2::new();
        // Source projections coexist during the legacy dense preparation and
        // during folded replay's immutable binding and checkpoint comparison.
        q.add_v1(input.heap()?.checked_mul(6).ok_or_else(|| capacity("GDN capture projection overflow"))?).map_err(E::Resource)?;
        q.vec_v1::<Hash64>(b.step_leaf_count).map_err(E::Resource)?;
        q.vec_v1::<u32>(prompt.len() as u64).map_err(E::Resource)?;
        q.vec_v1::<Base0GdnOwnedMaterialV2>(1).map_err(E::Resource)?;
        if matches!(&input, Capture::Folded(_)) {
            use kaspa_consensus_core::palw_resource_profile_v1::*;
            // The existing backend Producer quote chooses Fold for held
            // classes. This path deliberately materializes DENSE tiles, so
            // derive that actual sink instead, under the same two leaf caps.
            let run_positions = if kaspa_consensus_core::palw_state_chunk_map::palw_map_addresses_history_tiles_v1(profile) {
                1
            } else {
                b.job_context.declared_prefill_tokens.max(1)
            };
            let r = palw_resource_profile_v1(
                profile,
                &b.job_context,
                b.step_leaf_count,
                PalwRuntimeProfileV1::Q36KvI32,
                PalwResourceRoleV1::FullSeat,
                PalwRuntimeLimitsV1 { threads: rayon::current_num_threads().max(1) as u32, prefill_run_positions: run_positions },
                PalwCaptureRetentionV1::DenseTiles { tile_len: palw_profile_max_tile_len_v1(profile) },
            )
            .ok_or_else(|| capacity("GDN forced-dense replay geometry is not bounded"))?;
            let runtime = [
                r.kv_resident_bytes,
                r.opening_bytes,
                r.retained_checkpoint_bytes,
                r.attention_scratch_bytes,
                r.trace_scratch_bytes,
                r.capture_retained_bytes,
                r.checkpoint_leg_bytes,
                r.gdn_state_bytes,
            ]
            .into_iter()
            .try_fold(0u64, |sum, n| if n == u64::MAX { None } else { sum.checked_add(n) })
            .ok_or_else(|| capacity("GDN replay request overflow"))?;
            q.add_v1(runtime).map_err(E::Resource)?;
            // Typed trace row descriptors coexist with their i32 lanes. The
            // runtime trace quote counts the lanes, not these Vec headers.
            let rows = u64::from(profile.global_node_count())
                .checked_mul(u64::from(run_positions))
                .and_then(|n| n.checked_mul(3))
                .ok_or_else(|| capacity("GDN trace rows overflow"))?;
            q.vec_v1::<Vec<i32>>(rows).map_err(E::Resource)?;
            q.vec_v1::<Vec<i32>>(u64::from(b.job_context.exact_decode_tokens)).map_err(E::Resource)?;
            q.vec_v1::<u32>(u64::from(b.job_context.exact_decode_tokens)).map_err(E::Resource)?;
            q.vec_v1::<usize>(prompt.len() as u64).map_err(E::Resource)?;
            // A byte-bearing chunk has at least one byte. This intentionally
            // broad count also covers per-chunk Vec metadata during encoding.
            q.vec_v1::<Vec<u8>>(r.retained_checkpoint_bytes).map_err(E::Resource)?;
            // Kernels stream parameter rows. Admit the actual entire artifact
            // inventory as conservative simultaneous extraction scratch; do not
            // infer it from attacker-provided tensor dimensions.
            let params = self
                .artifact
                .params_map()
                .values()
                .try_fold(0u64, |sum, p| sum.checked_add(p.len() as u64))
                .ok_or_else(|| capacity("GDN parameter inventory overflow"))?;
            let weights = u64::try_from(self.artifact.weight_bytes()).map_err(|_| capacity("GDN weight inventory overflow"))?;
            q.add_v1(
                weights
                    .checked_add(params)
                    .and_then(|n| n.checked_mul(std::mem::size_of::<kaspa_consensus_core::palw_base0_a16::A16QuantParams>() as u64))
                    .ok_or_else(|| capacity("GDN parameter scratch overflow"))?,
            )
            .map_err(E::Resource)?;
        }
        let _construction = reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
        // All earlier construction is covered before entering the existing
        // capture/replay implementation. Its output acquires an independent
        // exact owner before this temporary construction ticket is released.
        let value = self.gdn_accuser_view_v2(bytes, Some(prompt)).map_err(invalid)?;
        NativeRetainedV1::retain_existing_v1(value).map_err(E::Resource)
    }
}
