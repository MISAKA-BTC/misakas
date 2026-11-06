//! UNADOPTED OUT-OF-TREE CANDIDATE; no ingress, tag or checkpoint row is registered.
//! Relation only. The source owner MUST reconstruct this target from the actual
//! authenticated birth/export, admitted TIR header/program/layout and E reservation.
//! No V1 bytes, consumed-prefix boundary or offence semantics are reinterpreted.
use crate::Hash64;
use crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportIdentityV1;
use crate::palw_freeprompt_v3::PalwFpPrefixStateV1;
use crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1;
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1, step_opening_root_capped_v1, step_tile_leaf_hash_v1};
use crate::palw_tir_prefix_state_v1::{PALW_TIR_PREFIX_STATE_DIGEST_DOMAIN_V1, PalwTirPrefixStateVerdictV1, leaf_prelude_v1};
use crate::palw_tir_step_v1::PalwTirStepSpaceV1;
use crate::palw_v2::PalwJobContextV2;
use borsh::{BorshDeserialize, BorshSerialize};

/// This purpose must be encoded in the NEW source-export court, seed, domain,
/// signature and terminal scope. It cannot be supplied as a replacement for
/// NativeFamily::PrefixState or an old PalwPrefixCourtFaultV1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct TirExportTargetV2 {
    pub version: u16,
    pub source: PalwPrefixExportIdentityV1,
    pub export_ordinal: u32,
    pub prefix: PalwFpPrefixStateV1,
    pub source_execution_root: Hash64,
    pub liability_until: u64,
    pub material_until: u64,
}

/// The same canonical TIR prefix digest as V1, but a DISTINCT versioned court
/// purpose with source k<=authenticated job.positions (prefill+decode-1). Only one authenticated leaf is retained per
/// move. Caller must admit topology, hash writer, lane decode and clone scratch
/// through native request accounting BEFORE calling new/leaf.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct TirExportPrefixStreamV2 {
    target: TirExportTargetV2,
    context_hash: Hash64,
    step_merkle_root: Hash64,
    step_leaf_count: u64,
    maximum_leaves: u64,
    total: u64,
    ordinal: u64,
    hashed: u128,
    digest: LeafHashContinuationV1,
    verdict: Option<PalwTirPrefixStateVerdictV1>,
}
impl TirExportPrefixStreamV2 {
    /// TARGET IS OWNER-DERIVED, never accepted directly from a wire declaration.
    /// Before this call source header commitment/job pin, class record/layout,
    /// producer signature, E.budget_id and original point must have been checked.
    pub(crate) fn from_authenticated_v2(
        space: &PalwTirStepSpaceV1,
        header: &PalwPublicTirBindingHeaderV1,
        target: TirExportTargetV2,
        maximum_leaves: u64,
    ) -> Result<Self, String> {
        if target.version != 2
            || target.source_execution_root != header.committed_execution_root
            || target.prefix.class_id != header.job_context.shape_profile_id
            || target.source.source_claim == Hash64::default()
            || target.source.intent == Hash64::default()
            || target.liability_until < target.source.accepted_daa
            || target.material_until < target.liability_until
            || target.material_until == u64::MAX
            || header.step_leaf_count == 0
            || header.step_leaf_count > maximum_leaves
        {
            return Err("TIR export target changes authenticated source, class or finite reservation".into());
        }
        if space.leaf_count_capped(&header.job_context, maximum_leaves).map_err(|e| e.to_string())? != header.step_leaf_count {
            return Err("TIR export source step count changes registered topology".into());
        }
        let total = space
            .export_prefix_state_leaf_count_v2(&header.job_context, target.prefix.prefix_tokens)
            .filter(|n| *n != 0)
            .ok_or("TIR export boundary is unaligned, empty or beyond source committed positions")?;
        let mut digest = LeafHashContinuationV1::new(PALW_TIR_PREFIX_STATE_DIGEST_DOMAIN_V1);
        digest.update(&target.prefix.prefix_tokens.to_le_bytes());
        Ok(Self {
            target,
            context_hash: header.job_context.context_hash(),
            step_merkle_root: header.step_merkle_root,
            step_leaf_count: header.step_leaf_count,
            maximum_leaves,
            total,
            ordinal: 0,
            hashed: 4,
            digest,
            verdict: None,
        })
    }
    pub(crate) fn total(&self) -> u64 {
        self.total
    }
    pub(crate) fn ordinal(&self) -> u64 {
        self.ordinal
    }
    pub(crate) fn verdict(&self) -> Option<PalwTirPrefixStateVerdictV1> {
        self.verdict
    }
    pub(crate) fn target(&self) -> TirExportTargetV2 {
        self.target
    }
    pub(crate) fn same_contract(&self, other: &Self) -> bool {
        (self.target, self.context_hash, self.step_merkle_root, self.step_leaf_count, self.maximum_leaves, self.total)
            == (other.target, other.context_hash, other.step_merkle_root, other.step_leaf_count, other.maximum_leaves, other.total)
    }
    pub(crate) fn check(&self) -> Result<(), String> {
        let byte_count = self.hashed.checked_add(128).ok_or("TIR export digest byte counter overflows")?;
        if self.target.version != 2
            || self.target.prefix.prefix_tokens == 0
            || self.target.material_until == u64::MAX
            || self.target.liability_until < self.target.source.accepted_daa
            || self.target.material_until < self.target.liability_until
            || self.step_leaf_count == 0
            || self.step_leaf_count > self.maximum_leaves
            || self.total == 0
            || self.ordinal > self.total
            || self.verdict.is_some() != (self.ordinal == self.total)
            || self.digest.byte_count() != Some(byte_count)
        {
            return Err("TIR export continuation changes its original progress".into());
        }
        if self.ordinal == 0 {
            let mut fresh = LeafHashContinuationV1::new(PALW_TIR_PREFIX_STATE_DIGEST_DOMAIN_V1);
            fresh.update(&self.target.prefix.prefix_tokens.to_le_bytes());
            if self.hashed != 4 || self.digest != fresh {
                return Err("TIR export template is not the fresh canonical digest".into());
            }
        }
        if let Some(verdict) = self.verdict {
            let derived = crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(
                &self.target.prefix.class_id,
                self.target.prefix.prefix_tokens,
                &self.digest.finish(),
            );
            let expected = if derived == self.target.prefix.state_root {
                PalwTirPrefixStateVerdictV1::Honest
            } else {
                PalwTirPrefixStateVerdictV1::StateRootMismatch { derived }
            };
            if verdict != expected {
                return Err("TIR export completed verdict differs from its actual digest".into());
            }
        }
        Ok(())
    }
    pub(crate) fn leaf(
        &mut self,
        space: &PalwTirStepSpaceV1,
        context: &PalwJobContextV2,
        values: &[u8],
        opening: &PalwStepOpeningV1,
        maximum_leaf_bytes: u64,
    ) -> Result<(), String> {
        self.check()?;
        if context.context_hash() != self.context_hash
            || space.export_prefix_state_leaf_count_v2(context, self.target.prefix.prefix_tokens) != Some(self.total)
        {
            return Err("TIR export source context or topology changed".into());
        }
        let leaf = space
            .export_prefix_state_leaf_v2(context, self.target.prefix.prefix_tokens, self.ordinal)
            .ok_or("TIR export cursor is complete or its canonical leaf changed")?;
        let bytes = u64::from(leaf.value_count).checked_mul(4).ok_or("TIR export leaf byte count overflow")?;
        // Borrowed size check MUST precede lane decoder allocation.
        if bytes > maximum_leaf_bytes || values.len() as u64 != bytes {
            return Err("TIR export body is not one exact paid canonical leaf".into());
        }
        crate::palw_tir_step_v1::palw_tir_lane_values_v1(leaf.dtype, values).map_err(|e| e.to_string())?;
        let preimage = PalwStepTileLeafV1 {
            version: crate::palw_tir_step_v1::PALW_TIR_STEP_LEAF_VERSION_V1,
            coord: leaf.coord,
            value_count: leaf.value_count,
            values_le: values.to_vec(),
        };
        let hash = step_tile_leaf_hash_v1(&self.context_hash, &self.target.prefix.class_id, &preimage);
        if opening.leaf_index != leaf.index
            || opening.leaf_hash != hash
            || step_opening_root_capped_v1(self.step_leaf_count, opening, self.maximum_leaves).map_err(|e| e.to_string())?
                != self.step_merkle_root
        {
            return Err("TIR export leaf does not authenticate against original source step tree".into());
        }
        let prelude = leaf_prelude_v1(&leaf)?;
        let mut next = self.clone();
        next.digest.update(&prelude);
        next.digest.update(values);
        next.hashed =
            next.hashed.checked_add(prelude.len() as u128 + values.len() as u128).ok_or("TIR export digest length overflow")?;
        next.ordinal += 1;
        if next.ordinal == next.total {
            let derived = crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(
                &self.target.prefix.class_id,
                self.target.prefix.prefix_tokens,
                &next.digest.finish(),
            );
            next.verdict = Some(if derived == self.target.prefix.state_root {
                PalwTirPrefixStateVerdictV1::Honest
            } else {
                PalwTirPrefixStateVerdictV1::StateRootMismatch { derived }
            });
        }
        *self = next;
        Ok(())
    }
}

use crate::palw_claim_local_v1::prefix_v1::PalwSignedPrefixExportV1;
use crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3;
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
use crate::palw_tir_public_source_v1::PalwTirPublicSourceV1;
type GeometryFaultV2 = NativeDecodeErrorV1<String>;
fn invalid_geometry(why: &str) -> GeometryFaultV2 {
    NativeDecodeErrorV1::Decode(why.into())
}
// Promoted odd nodes have no encoded sibling in the released opening codec.
fn opening_siblings_v2(count: u64, index: u64) -> Result<u64, GeometryFaultV2> {
    if count == 0 || index >= count {
        return Err(invalid_geometry("export opening index outside native tree"));
    }
    let (mut width, mut position, mut siblings) = (count, index, 0u64);
    while width > 1 {
        siblings += u64::from((position ^ 1) < width);
        width = width.div_ceil(2);
        position /= 2;
    }
    Ok(siblings)
}
fn checked_add(a: u64, b: u64) -> Result<u64, GeometryFaultV2> {
    a.checked_add(b)
        .ok_or_else(|| NativeDecodeErrorV1::Resource(NativeCoverageV1::overflow::<TirPrefixExportSpecV2>("export geometry sum")))
}
fn checked_mul(a: u64, b: u64) -> Result<u64, GeometryFaultV2> {
    a.checked_mul(b)
        .ok_or_else(|| NativeDecodeErrorV1::Resource(NativeCoverageV1::overflow::<TirPrefixExportSpecV2>("export geometry product")))
}

/// Peak scratch of the real one-leaf relation replay, BEFORE any lane decode,
/// preimage/hash-writer clone or digest continuation is constructed. Court
/// clone/slots/maps, source-context hydration, accepted body and journal retention
/// are distinct caller-owned peaks and must be added by native admission.
pub(crate) fn leaf_replay_requests_v2(maximum_values_bytes: u64) -> Result<u64, GeometryFaultV2> {
    if maximum_values_bytes == 0
        || !maximum_values_bytes.is_multiple_of(4)
        || maximum_values_bytes > crate::palw_tir_prefix_owned_v1::PALW_TIR_PREFIX_LEAF_MAX_BYTES_V1 as u64
    {
        return Err(invalid_geometry("TIR export replay scratch needs one bounded canonical lane body"));
    }
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<i128>(maximum_values_bytes / 4).map_err(NativeDecodeErrorV1::Resource)?;
    q.exact_v1::<u8>(maximum_values_bytes).map_err(NativeDecodeErrorV1::Resource)?; // preimage values.to_vec
    q.exact_v1::<u8>(32).map_err(NativeDecodeErrorV1::Resource)?; // canonical leaf prelude Vec capacity
    // Step-leaf Writer starts at256; its fixed prelude is154 bytes. The native
    // identity writer envelope also covers old+grown capacities at a power-of-
    // two boundary (2*logical bytes alone would understate that peak).
    let hash_writer = PalwTirPublicSourceV1::native_identity_writer_requests_v3(checked_add(maximum_values_bytes, 154)?)?;
    q.exact_v1::<u8>(hash_writer).map_err(NativeDecodeErrorV1::Resource)?;
    q.exact_v1::<TirExportPrefixStreamV2>(1).map_err(NativeDecodeErrorV1::Resource)?;
    q.vec_v1::<u8>(128).map_err(NativeDecodeErrorV1::Resource)?; // cloned bounded digest continuation
    Ok(q.bytes_v1())
}

/// Native local preparation result, deliberately NOT serializable. Its local
/// workspace contains host/compiler sizes and is never canonical plan identity.
/// Borrow canonical_identity_v3() for the separate prospective identity only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TirPrefixExportSpecV2 {
    pub intent_id: Hash64,
    pub base_claim: Hash64,
    pub source_id: Hash64,
    pub job_identity: Hash64,
    pub header_context: Hash64,
    pub export_ordinal: u32,
    pub prefix: PalwFpPrefixStateV1,
    pub liability_until: u64,
    pub material_until: u64,
    pub source_step_count: u64,
    pub maximum_leaves: u64,
    pub unit_count: u64,
    pub maximum_values_bytes: u64,
    pub maximum_response_bytes: u64,
    pub total_values_bytes: u64,
    pub total_response_bytes: u64,
    /// Whole-tree height; a promoted path encodes fewer actual siblings.
    pub opening_depth: u64,
    /// Actual turn scratch; full source context and court/map/journal peaks are separate.
    pub leaf_replay_requests: u64,
}
/// Borrowed canonical projection, no allocation and no decoder/admission
/// constructor. Excludes leaf_replay_requests, which is LOCAL NativeDemand.
pub(crate) struct CanonicalTirPrefixExportSpecV3<'a>(&'a TirPrefixExportSpecV2);
impl BorshSerialize for CanonicalTirPrefixExportSpecV3<'_> {
    fn serialize<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        let s = self.0;
        s.intent_id.serialize(w)?;
        s.base_claim.serialize(w)?;
        s.source_id.serialize(w)?;
        s.job_identity.serialize(w)?;
        s.header_context.serialize(w)?;
        s.export_ordinal.serialize(w)?;
        s.prefix.serialize(w)?;
        s.liability_until.serialize(w)?;
        s.material_until.serialize(w)?;
        s.source_step_count.serialize(w)?;
        s.maximum_leaves.serialize(w)?;
        s.unit_count.serialize(w)?;
        s.maximum_values_bytes.serialize(w)?;
        s.maximum_response_bytes.serialize(w)?;
        s.total_values_bytes.serialize(w)?;
        s.total_response_bytes.serialize(w)?;
        s.opening_depth.serialize(w)?;
        Ok(())
    }
}
impl TirPrefixExportSpecV2 {
    pub(crate) fn canonical_identity_v3(&self) -> CanonicalTirPrefixExportSpecV3<'_> {
        CanonicalTirPrefixExportSpecV3(self)
    }
    /// Actual local one-leaf preflight; NOT canonical bytes or priced CPU work.
    pub(crate) fn actual_leaf_workspace_requests_v2(&self) -> u64 {
        self.leaf_replay_requests
    }
}
/// Stream the ordered borrowed slice with canonical Borsh u32 count, without a
/// Vec of projected specs. No producer value may construct an admitted set.
struct CanonicalTirExportSpecsV3<'a>(&'a [TirPrefixExportSpecV2]);
impl BorshSerialize for CanonicalTirExportSpecsV3<'_> {
    fn serialize<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        let len = u32::try_from(self.0.len())
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "canonical export count exceeds Borsh slice"))?;
        len.serialize(w)?;
        for s in self.0 {
            s.canonical_identity_v3().serialize(w)?
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TirPrefixExportUnitV2 {
    pub ordinal: u64,
    pub source_leaf_index: u64,
    pub value_count: u32,
    pub values_bytes: u64,
    pub response_bytes: u64,
}
/// One leased registered topology, borrowed by all export-spec derivations.
/// Source and original are real native preparation inputs, not wire geometry.
pub(crate) struct TirExportSourceContextV2<'a> {
    original: &'a PalwFreePromptCommitmentV3,
    source: &'a PalwTirPublicSourceV1,
    space: PalwTirStepSpaceV1,
    maximum_leaves: u64,
    base: Hash64,
    job_identity: Hash64,
    preparation_requests: u64,
}

/// No state/admission authority is created here. Caller authenticates the
/// registered record/layout and producer bond in the actual quote/fold prefix.
/// Historical callbacks pass the born job key authenticated by the opaque
/// admission context, not a possibly rotated CURRENT bond key.
/// All allocations below are charged before resolve/StepSpace/context hashes.
pub(crate) fn prepare_export_source_v2<'a>(
    original: &'a PalwFreePromptCommitmentV3,
    source: &'a PalwTirPublicSourceV1,
    registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
    registered_producer_key: &[u8],
    maximum_leaves: u64,
) -> Result<NativeRetainedV1<TirExportSourceContextV2<'a>>, GeometryFaultV2> {
    let header = source.header_v1();
    if original.job.is_prefix_inherit() {
        return Err(invalid_geometry("TIR inherited v12 export is not an inheritance-safe class path"));
    }
    if original.job.executor_pubkey != registered_producer_key
        || original.job.class_id != registered.facts.class_id
        || header.job_context.shape_profile_id != original.job.class_id
        || original.execution_root != header.committed_execution_root
        || original.trace_root != header.full_logits_trace_root
        || original.work_leaves != header.step_leaf_count
        || source.leaf_count_v1() != header.step_leaf_count
        || original.job.prompt_tokens != header.job_context.declared_prefill_tokens
    {
        return Err(invalid_geometry("TIR export source changes native producer, class, roots, counts or prompt"));
    }
    use crate::palw_native_resource_v1::{NativeHeapV1, source_tir_program_shape_v1};
    registered.check_program_v1().map_err(|e| NativeDecodeErrorV1::Decode(e.into()))?;
    if layout.commit_tiles.len() as u64 > u64::from(registered.program_bytes)
        || layout.state_tiles.len() as u64 > u64::from(registered.program_bytes)
    {
        return Err(invalid_geometry("TIR export layout exceeds native registered program geometry"));
    }
    let (shape, program_requests) = source_tir_program_shape_v1(&registered.program)?;
    let mut requests = TirNativeRequestsV1::new();
    // NEW allocation owners only: borrowed Source hydration/retained original
    // have their caller-owned leases. resolve_registered_layout verifies a
    // temporary StepSpace, then we build the retained StepSpace, sequentially.
    requests.add_v1(program_requests).map_err(NativeDecodeErrorV1::Resource)?;
    requests
        .add_v1(crate::palw_tir_step_v1::native_tir_step_space_additional_requests_v1(&shape).map_err(NativeDecodeErrorV1::Resource)?)
        .map_err(NativeDecodeErrorV1::Resource)?;
    requests.exact_v1::<u8>(registered.program.len() as u64).map_err(NativeDecodeErrorV1::Resource)?; // binding raw program clone
    for _ in 0..2 {
        // raw binding layout and live verified/retained space layout
        let bound = layout.native_heap_v1("export registered layout clone");
        requests.add_v1(bound.bytes().ok_or(NativeDecodeErrorV1::Resource(bound))?).map_err(NativeDecodeErrorV1::Resource)?;
    }
    for _ in 0..2 {
        // original-derived expected context and binding context
        let bound = header.job_context.native_heap_v1("export job context clone");
        requests.add_v1(bound.bytes().ok_or(NativeDecodeErrorV1::Resource(bound))?).map_err(NativeDecodeErrorV1::Resource)?;
    }
    requests.exact_v1::<NativeRetainedV1<TirExportSourceContextV2<'a>>>(1).map_err(NativeDecodeErrorV1::Resource)?;
    let encoded = borsh::object_length(original)
        .and_then(|a| borsh::object_length(layout).map(|b| a.max(b)))
        .and_then(|a| borsh::object_length(header).map(|b| a.max(b)))
        .map_err(|e| NativeDecodeErrorV1::Decode(e.to_string()))? as u64;
    requests
        .exact_v1::<u8>(PalwTirPublicSourceV1::native_identity_writer_requests_v3(encoded)?)
        .map_err(NativeDecodeErrorV1::Resource)?;
    let preparation_requests = requests.bytes_v1();
    let lease = reserve_native_requests_v1(preparation_requests).map_err(NativeDecodeErrorV1::Resource)?;
    let ctx = &header.job_context;
    let facts = crate::palw_fp_execution_v3::PalwFpRunFactsV3 {
        decode_tokens_executed: original.decode_tokens_executed,
        stop_reason: original.stop_reason,
        full_logits_trace_root: original.trace_root,
        activation_leg_root: Hash64::default(),
        checkpoint_leg_root: Hash64::default(),
        step_leg_root: header.step_merkle_root,
        step_leaf_count: original.work_leaves,
    };
    let class = crate::palw_fp_execution_v3::PalwFpClassFactsV3 {
        model_profile_id: ctx.model_profile_id,
        runtime_manifest_hash: ctx.runtime_manifest_hash,
        runtime_class_id: ctx.runtime_class_id,
        shape_profile_id: ctx.shape_profile_id,
        cu_ruleset_id: ctx.cu_ruleset_id,
    };
    let expected = crate::palw_fp_execution_v3::palw_fp_job_context_v3(&original.job, &class, &facts, &ctx.network_id)
        .map_err(|e| NativeDecodeErrorV1::Decode(e.to_string()))?;
    let job_identity = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original);
    if expected != *ctx || job_identity != crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(ctx) {
        return Err(invalid_geometry("TIR export source header is not the actual original job"));
    }
    let binding = header
        .resolve_registered_layout_v1(registered, layout, original.execution_root, maximum_leaves)
        .map_err(NativeDecodeErrorV1::Decode)?;
    let space = PalwTirStepSpaceV1::new(&binding.class).map_err(|e| NativeDecodeErrorV1::Decode(e.to_string()))?;
    if space.leaf_count_capped(ctx, maximum_leaves).map_err(|e| NativeDecodeErrorV1::Decode(e.to_string()))? != header.step_leaf_count
    {
        return Err(invalid_geometry("TIR export source step count differs from native topology"));
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        TirExportSourceContextV2 {
            original,
            source,
            space,
            maximum_leaves,
            base: crate::palw_freeprompt_v3::fp_claim_id_v3(original),
            job_identity,
            preparation_requests,
        },
        lease,
    ))
}
impl TirExportSourceContextV2<'_> {
    /// New coexisting topology/identity-copy peak, already admitted by this
    /// context's lease. Excludes caller-owned original/F1/Source hydration and
    /// court/slot/journal bodies. Quote preparation shares it across all export
    /// intents ONCE; each runtime turn that rehydrates this exact context needs
    /// its own native lease and paid owner-validation/profile allowance.
    pub(crate) fn preparation_requests_v2(&self) -> u64 {
        self.preparation_requests
    }
    pub(crate) fn space_v2(&self) -> &PalwTirStepSpaceV1 {
        &self.space
    }
    pub(crate) fn header_v2(&self) -> &PalwPublicTirBindingHeaderV1 {
        self.source.header_v1()
    }
    fn boundary_response_v2(&self, k: u32, ordinal: u64) -> Result<u64, GeometryFaultV2> {
        let header = self.header_v2();
        let leaf = self
            .space
            .export_prefix_state_leaf_v2(&header.job_context, k, ordinal)
            .ok_or_else(|| invalid_geometry("export boundary response leaf missing"))?;
        checked_add(
            checked_add(103, checked_mul(opening_siblings_v2(header.step_leaf_count, leaf.index)?, 64)?)?,
            checked_mul(u64::from(leaf.value_count), 4)?,
        )
    }
    // At each odd width, sibling omission affects a suffix of original leaf
    // indices. Binary search the monotonic boundary indices to count that
    // suffix, without visiting all boundary units or all committed leaves.
    fn boundary_responses_v2(&self, k: u32, total: u64, values: u64, depth: u64) -> Result<(u64, u64), GeometryFaultV2> {
        let header = self.header_v2();
        let mut response = checked_add(checked_mul(total, checked_add(103, checked_mul(depth, 64)?)?)?, values)?;
        let (mut width, mut scale) = (header.step_leaf_count, 1u64);
        while width > 1 {
            if width % 2 == 1 {
                let first = checked_mul(width - 1, scale)?;
                let (mut lo, mut hi) = (0, total);
                while lo < hi {
                    let mid = lo + (hi - lo) / 2;
                    let leaf = self
                        .space
                        .export_prefix_state_leaf_v2(&header.job_context, k, mid)
                        .ok_or_else(|| invalid_geometry("export suffix boundary leaf missing"))?;
                    if leaf.index < first {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                response = response
                    .checked_sub(checked_mul(total - lo, 64)?)
                    .ok_or_else(|| invalid_geometry("export promoted response underflow"))?;
            }
            width = width.div_ceil(2);
            if width > 1 {
                scale = checked_mul(scale, 2)?;
            }
        }
        let mut fixed = 0;
        for state in self.space.fixed_instances() {
            fixed = checked_add(fixed, state.elements.div_ceil(u64::from(state.tile_lanes)))?;
        }
        let hist = self.space.hist_leaves_per_tile_v1();
        let chunks = u64::from(k / self.space.layout.h_tile);
        let fixed_start = checked_mul(chunks - 1, hist)?;
        let mut maximum = 0;
        let mut offset = 0;
        // First tiles have the largest value count in each instance. Sibling
        // count cannot increase with index; later history chunks repeat the
        // same values. Each instance's earliest tile therefore attains its max.
        for state in self.space.fixed_instances() {
            let tiles = state.elements.div_ceil(u64::from(state.tile_lanes));
            if tiles > 0 {
                maximum = maximum.max(self.boundary_response_v2(k, checked_add(fixed_start, offset)?)?);
            }
            offset = checked_add(offset, tiles)?;
        }
        let hist_start = if chunks == 1 { fixed } else { 0 };
        offset = 0;
        for state in self.space.hist_instances() {
            let tiles = state.elements.div_ceil(u64::from(state.tile_lanes));
            if tiles > 0 {
                maximum = maximum.max(self.boundary_response_v2(k, checked_add(hist_start, offset)?)?);
            }
            offset = checked_add(offset, tiles)?;
        }
        Ok((response, maximum))
    }
    pub(crate) fn prepare_export_v2(&self, signed: &PalwSignedPrefixExportV1) -> Result<TirPrefixExportSpecV2, GeometryFaultV2> {
        let intent = &signed.intent;
        intent
            .verify_signature_v1(&self.original.job.executor_pubkey, &signed.signature)
            .map_err(|e| NativeDecodeErrorV1::Decode(e.to_string()))?;
        if intent.base_commitment != self.base
            || intent.network != self.original.job.network_domain
            || intent.prefix.class_id != self.original.job.class_id
            || intent.material_until > self.original.trace_retention_daa
            || intent.material_until == u64::MAX
        {
            return Err(invalid_geometry("TIR export intent changes prospective original scope or signed retention"));
        }
        let header = self.source.header_v1();
        let k = intent.prefix.prefix_tokens;
        let total =
            self.space.export_prefix_state_leaf_count_v2(&header.job_context, k).filter(|n| *n > 0).ok_or_else(|| {
                invalid_geometry("TIR export boundary is unaligned, empty or beyond actual source committed positions")
            })?;
        // Fixed state at k-1, Hist rows 0..k. Derive sums from actual state
        // instances without a Vec or a walk over every boundary unit.
        let mut values = 0;
        let mut maximum_values = 0;
        for state in self.space.fixed_instances() {
            values = checked_add(values, checked_mul(state.elements, 4)?)?;
            maximum_values = maximum_values.max(checked_mul(state.elements.min(u64::from(state.tile_lanes)), 4)?);
        }
        for state in self.space.hist_instances() {
            values = checked_add(values, checked_mul(checked_mul(state.elements, u64::from(k))?, 4)?)?;
            maximum_values = maximum_values.max(checked_mul(
                checked_mul(state.elements.min(u64::from(state.tile_lanes)), u64::from(self.space.layout.h_tile))?,
                4,
            )?);
        }
        if maximum_values > crate::palw_tir_prefix_owned_v1::PALW_TIR_PREFIX_LEAF_MAX_BYTES_V1 as u64 {
            return Err(invalid_geometry("TIR export boundary exceeds existing paid one-leaf carrier format"));
        }
        let depth = if header.step_leaf_count <= 1 { 0 } else { u64::from(64 - (header.step_leaf_count - 1).leading_zeros()) };
        // Existing native StepLeaf Borsh response framing, with exact canonical
        // boundary values instead of an unrelated global widest commit leaf.
        let (total_response, maximum_response) = self.boundary_responses_v2(k, total, values, depth)?;
        Ok(TirPrefixExportSpecV2 {
            intent_id: intent.id_v1(),
            base_claim: self.base,
            source_id: self.source.source_id_v1(),
            job_identity: self.job_identity,
            header_context: header.job_context.context_hash(),
            export_ordinal: intent.ordinal,
            prefix: intent.prefix,
            liability_until: intent.liability_until,
            material_until: intent.material_until,
            source_step_count: header.step_leaf_count,
            maximum_leaves: self.maximum_leaves,
            unit_count: total,
            maximum_values_bytes: maximum_values,
            maximum_response_bytes: maximum_response,
            total_values_bytes: values,
            total_response_bytes: total_response,
            opening_depth: depth,
            leaf_replay_requests: leaf_replay_requests_v2(maximum_values)?,
        })
    }
    pub(crate) fn unit_at_v2(&self, spec: &TirPrefixExportSpecV2, ordinal: u64) -> Result<TirPrefixExportUnitV2, GeometryFaultV2> {
        let header = self.source.header_v1();
        if spec.base_claim != self.base
            || spec.source_id != self.source.source_id_v1()
            || spec.header_context != header.job_context.context_hash()
            || spec.job_identity != self.job_identity
            || spec.source_step_count != header.step_leaf_count
            || spec.maximum_leaves != self.maximum_leaves
            || spec.prefix.class_id != self.original.job.class_id
            || spec.opening_depth
                != if header.step_leaf_count <= 1 { 0 } else { u64::from(64 - (header.step_leaf_count - 1).leading_zeros()) }
            || ordinal >= spec.unit_count
            || self.space.export_prefix_state_leaf_count_v2(&header.job_context, spec.prefix.prefix_tokens) != Some(spec.unit_count)
        {
            return Err(invalid_geometry("TIR export visitor changes owner-derived source or ordinal geometry"));
        }
        let leaf = self
            .space
            .export_prefix_state_leaf_v2(&header.job_context, spec.prefix.prefix_tokens, ordinal)
            .ok_or_else(|| invalid_geometry("TIR export visitor canonical boundary leaf missing"))?;
        let values = checked_mul(u64::from(leaf.value_count), 4)?;
        Ok(TirPrefixExportUnitV2 {
            ordinal,
            source_leaf_index: leaf.index,
            value_count: leaf.value_count,
            values_bytes: values,
            response_bytes: checked_add(
                checked_add(103, checked_mul(opening_siblings_v2(header.step_leaf_count, leaf.index)?, 64)?)?,
                values,
            )?,
        })
    }
    pub(crate) fn visit_units_v2(
        &self,
        spec: &TirPrefixExportSpecV2,
        mut visit: impl FnMut(TirPrefixExportUnitV2) -> Result<(), GeometryFaultV2>,
    ) -> Result<(), GeometryFaultV2> {
        for ordinal in 0..spec.unit_count {
            visit(self.unit_at_v2(spec, ordinal)?)?;
        }
        Ok(())
    }
}
crate::palw_native_resource_v1::native_struct!(TirExportTargetV2; version, source, export_ordinal, prefix, source_execution_root, liability_until, material_until);
crate::palw_native_resource_v1::native_struct!(TirExportPrefixStreamV2; target, context_hash, step_merkle_root, step_leaf_count, maximum_leaves, total, ordinal, hashed, digest, verdict);
crate::palw_native_resource_v1::wire::wire_struct!(TirExportTargetV2; u16, PalwPrefixExportIdentityV1, u32, PalwFpPrefixStateV1, Hash64, u64, u64);
crate::palw_native_resource_v1::wire::wire_struct!(TirExportPrefixStreamV2; TirExportTargetV2, Hash64, Hash64, u64, u64, u64, u64, u128, LeafHashContinuationV1, Option<PalwTirPrefixStateVerdictV1>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_claim_local_v1::prefix_v1::{PREFIX_EXPORT_CONTEXT_V1, PalwPrefixExportIntentV1};
    use crate::palw_step_leg::step_opening_from_leaf_source_capped_v1;
    use libcrux_ml_dsa::ml_dsa_87 as dsa;
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    #[test]
    fn export_opening_geometry_matches_actual_promoted_paths() {
        for count in 1..=65u64 {
            let mut previous = u64::MAX;
            for index in 0..count {
                let opening = step_opening_from_leaf_source_capped_v1(count, index, 65, h).unwrap();
                let siblings = opening_siblings_v2(count, index).unwrap();
                assert_eq!(siblings, opening.siblings.len() as u64);
                assert!(siblings <= previous, "sibling count must not increase with leaf index");
                previous = siblings;
            }
        }
        assert!(opening_siblings_v2(0, 0).is_err());
        assert!(opening_siblings_v2(3, 3).is_err());
    }
    fn intent(original: &PalwFreePromptCommitmentV3) -> PalwPrefixExportIntentV1 {
        PalwPrefixExportIntentV1 {
            version: 1,
            network: original.job.network_domain,
            source_namespace: 1,
            source_nonce: h(31),
            base_commitment: crate::palw_freeprompt_v3::fp_claim_id_v3(original),
            producer_key_id: crate::mldsa87_primitives::mldsa87_key_id(&original.job.executor_pubkey),
            ordinal: 0,
            prefix: PalwFpPrefixStateV1 { class_id: original.job.class_id, state_root: h(32), prefix_tokens: 2 },
            liability_until: original.trace_retention_daa - 1,
            material_until: original.trace_retention_daa,
        }
    }
    #[test]
    fn rfc40_tir_export_prebirth_native_source_checks_identity_signature_and_sparse_ordinal_geometry() {
        let keys = dsa::generate_key_pair([81; 32]);
        let pk = keys.verification_key.as_ref().to_vec();
        let f =
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(2, |job| job.executor_pubkey = pk.clone(), None);
        let original = f.frame.original_v1();
        let header = f.frame.header_v1();
        let maximum = f.frame.descriptor_v1().maximum_leaves;
        let source =
            PalwTirPublicSourceV1::new_native_v1(&f.record, &f.layout, header, original.execution_root, original.trace_root, maximum)
                .unwrap();
        let context = prepare_export_source_v2(original, &source, &f.record, &f.layout, &pk, maximum).unwrap();
        let sign = |intent: PalwPrefixExportIntentV1| {
            let signature = dsa::sign(&keys.signing_key, intent.id_v1().as_byte_slice(), PREFIX_EXPORT_CONTEXT_V1, [82; 32])
                .unwrap()
                .as_ref()
                .to_vec();
            PalwSignedPrefixExportV1 { intent, signature }
        };
        let ordinary = context.prepare_export_v2(&sign(intent(original))).unwrap();
        let mut sparse = intent(original);
        sparse.ordinal = u32::MAX;
        let spec = context.prepare_export_v2(&sign(sparse)).unwrap();
        assert_eq!(spec.export_ordinal, u32::MAX);
        assert_eq!(
            (spec.unit_count, spec.total_response_bytes),
            (ordinary.unit_count, ordinary.total_response_bytes),
            "ordinal holes do not allocate slots or invent service branches"
        );
        let mut totals = (0u64, 0u64, 0u64, 0u64, 0u64);
        context
            .visit_units_v2(&spec, |unit| {
                let opening = step_opening_from_leaf_source_capped_v1(header.step_leaf_count, unit.source_leaf_index, maximum, |i| {
                    f.execution.hashes[i as usize]
                })
                .unwrap();
                let body = crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
                    preimage: f.execution.preimages[unit.source_leaf_index as usize].clone(),
                    opening,
                };
                let wire = borsh::to_vec(&body).unwrap();
                assert_eq!(wire.len() as u64, unit.response_bytes, "quote uses actual bounded StepLeaf codec framing");
                totals.0 += 1;
                totals.1 += unit.values_bytes;
                totals.2 += unit.response_bytes;
                totals.3 = totals.3.max(unit.values_bytes);
                totals.4 = totals.4.max(unit.response_bytes);
                Ok(())
            })
            .unwrap();
        assert_eq!(
            totals,
            (
                spec.unit_count,
                spec.total_values_bytes,
                spec.total_response_bytes,
                spec.maximum_values_bytes,
                spec.maximum_response_bytes
            )
        );
        for change in 0..6 {
            let mut bad = intent(original);
            match change {
                0 => bad.prefix.class_id = h(401),
                1 => bad.network = h(402),
                2 => bad.base_commitment = h(403),
                3 => bad.source_namespace = 2,
                4 => bad.prefix.prefix_tokens = 1,
                _ => bad.prefix.prefix_tokens = context.space_v2().job_shape(&header.job_context).unwrap().positions + 1,
            }
            assert!(context.prepare_export_v2(&sign(bad)).is_err(), "signed bad identity/unsupported boundary {change}");
        }
        let wrong = dsa::generate_key_pair([83; 32]);
        assert!(prepare_export_source_v2(original, &source, &f.record, &f.layout, wrong.verification_key.as_ref(), maximum).is_err());
        let mut bad = sign(intent(original));
        bad.signature[0] ^= 1;
        assert!(context.prepare_export_v2(&bad).is_err());
        assert!(context.unit_at_v2(&spec, spec.unit_count).is_err());
    }
    #[test]
    fn rfc40_tir_export_prebirth_generated_boundary_has_exact_native_quote_and_authenticated_verdict() {
        let keys = dsa::generate_key_pair([85; 32]);
        let pk = keys.verification_key.as_ref().to_vec();
        let f =
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(2, |job| job.executor_pubkey = pk.clone(), None);
        let original = f.frame.original_v1();
        let header = f.frame.header_v1();
        let maximum = f.frame.descriptor_v1().maximum_leaves;
        let source =
            PalwTirPublicSourceV1::new_native_v1(&f.record, &f.layout, header, original.execution_root, original.trace_root, maximum)
                .unwrap();
        let context = prepare_export_source_v2(original, &source, &f.record, &f.layout, &pk, maximum).unwrap();
        let job = context.space_v2().job_shape(&header.job_context).unwrap();
        let k = job.positions;
        assert!(k > job.prefill, "fixture must authenticate generated state");
        assert!(context.space_v2().prefix_state_leaf_count_v1(&header.job_context, k).is_none());
        let mut declaration = intent(original);
        declaration.prefix.prefix_tokens = k;
        let signature = dsa::sign(&keys.signing_key, declaration.id_v1().as_byte_slice(), PREFIX_EXPORT_CONTEXT_V1, [86; 32])
            .unwrap()
            .as_ref()
            .to_vec();
        let spec = context.prepare_export_v2(&PalwSignedPrefixExportV1 { intent: declaration.clone(), signature }).unwrap();
        let mut sums = (0u64, 0u64, 0u64, 0u64);
        let mut generated = false;
        let mut digest = LeafHashContinuationV1::new(PALW_TIR_PREFIX_STATE_DIGEST_DOMAIN_V1);
        digest.update(&k.to_le_bytes());
        context
            .visit_units_v2(&spec, |unit| {
                let leaf = context.space_v2().export_prefix_state_leaf_v2(&header.job_context, k, unit.ordinal).unwrap();
                let preimage = &f.execution.preimages[unit.source_leaf_index as usize];
                assert_eq!(leaf.index, unit.source_leaf_index);
                assert_eq!(preimage.values_le.len() as u64, unit.values_bytes);
                generated |= leaf.position >= job.prefill;
                let opening = step_opening_from_leaf_source_capped_v1(header.step_leaf_count, leaf.index, maximum, |i| {
                    f.execution.hashes[i as usize]
                })
                .unwrap();
                let body = crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf { preimage: preimage.clone(), opening };
                assert_eq!(borsh::to_vec(&body).unwrap().len() as u64, unit.response_bytes);
                sums.0 += 1;
                sums.1 += unit.values_bytes;
                sums.2 += unit.response_bytes;
                sums.3 = sums.3.max(unit.response_bytes);
                digest.update(&leaf_prelude_v1(&leaf).unwrap());
                digest.update(&preimage.values_le);
                Ok(())
            })
            .unwrap();
        assert!(generated);
        assert_eq!(sums, (spec.unit_count, spec.total_values_bytes, spec.total_response_bytes, spec.maximum_response_bytes));
        let honest = crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(&spec.prefix.class_id, k, &digest.finish());
        for declared in [honest, h(8501)] {
            assert_ne!(honest, h(8501));
            let target = TirExportTargetV2 {
                version: 2,
                source: PalwPrefixExportIdentityV1 {
                    intent: declaration.id_v1(),
                    source_claim: spec.base_claim,
                    accepted_block: h(8502),
                    accepted_daa: 10,
                    accepted_blue_score: 10,
                },
                export_ordinal: spec.export_ordinal,
                prefix: PalwFpPrefixStateV1 { state_root: declared, ..spec.prefix },
                source_execution_root: header.committed_execution_root,
                liability_until: 30,
                material_until: 40,
            };
            let mut stream = TirExportPrefixStreamV2::from_authenticated_v2(context.space_v2(), header, target, maximum).unwrap();
            for ordinal in 0..spec.unit_count {
                let unit = context.unit_at_v2(&spec, ordinal).unwrap();
                let opening = step_opening_from_leaf_source_capped_v1(header.step_leaf_count, unit.source_leaf_index, maximum, |i| {
                    f.execution.hashes[i as usize]
                })
                .unwrap();
                stream
                    .leaf(
                        context.space_v2(),
                        &header.job_context,
                        &f.execution.preimages[unit.source_leaf_index as usize].values_le,
                        &opening,
                        80_000,
                    )
                    .unwrap();
                stream = borsh::from_slice(&borsh::to_vec(&stream).unwrap()).unwrap();
                stream.check().unwrap();
            }
            assert_eq!(
                stream.verdict(),
                Some(if declared == honest {
                    PalwTirPrefixStateVerdictV1::Honest
                } else {
                    PalwTirPrefixStateVerdictV1::StateRootMismatch { derived: honest }
                })
            );
        }
    }
    #[test]
    fn rfc40_tir_export_aligned_full_prefill_uses_authentic_source_leaves_and_preserves_consumed_boundary() {
        // Actual corpus execution with an aligned four-token prompt; the tiny
        // three-token fixture's C=2 boundary is deliberately not relaxed.
        let (name, program, params, tokens) =
            crate::palw_tir_corpus_fixture_v1::programs().into_iter().find(|(name, ..)| name == "dense-gqa-2layer").unwrap();
        let f = crate::palw_tir_corpus_fixture_v1::fixture_tiled(name, program, params, tokens, 4, 1, 64);
        let x = f.honest();
        let header = PalwPublicTirBindingHeaderV1::of_binding_v1(&x.binding);
        let space = PalwTirStepSpaceV1::new(&x.binding.class).unwrap();
        let k = header.job_context.declared_prefill_tokens;
        assert_eq!(k, 4);
        assert!(space.prefix_state_leaf_count_v1(&header.job_context, k).is_none());
        assert!(space.prefix_state_leaf_v1(&header.job_context, k, 0).is_none());
        let total = space.export_prefix_state_leaf_count_v2(&header.job_context, k).unwrap();
        let mut digest = LeafHashContinuationV1::new(PALW_TIR_PREFIX_STATE_DIGEST_DOMAIN_V1);
        digest.update(&k.to_le_bytes());
        for ordinal in 0..total {
            let leaf = space.export_prefix_state_leaf_v2(&header.job_context, k, ordinal).unwrap();
            digest.update(&leaf_prelude_v1(&leaf).unwrap());
            digest.update(&x.preimages[leaf.index as usize].values_le);
        }
        let honest = crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(&header.job_context.shape_profile_id, k, &digest.finish());
        let target = TirExportTargetV2 {
            version: 2,
            source: PalwPrefixExportIdentityV1 {
                intent: h(501),
                source_claim: h(502),
                accepted_block: h(503),
                accepted_daa: 10,
                accepted_blue_score: 10,
            },
            export_ordinal: 0,
            prefix: PalwFpPrefixStateV1 { class_id: header.job_context.shape_profile_id, prefix_tokens: k, state_root: honest },
            source_execution_root: header.committed_execution_root,
            liability_until: 30,
            material_until: 40,
        };
        assert!(
            crate::palw_tir_prefix_state_v1::PalwTirPrefixStateStreamV1::new_v1(
                &space,
                target.prefix.class_id,
                &header.job_context,
                header.step_merkle_root,
                header.step_leaf_count,
                1 << 26,
                k,
                honest
            )
            .is_err()
        );
        for declared in [honest, h(504)] {
            let mut changed = target;
            changed.prefix.state_root = declared;
            let mut stream = TirExportPrefixStreamV2::from_authenticated_v2(&space, &header, changed, 1 << 26).unwrap();
            let initial = stream.clone();
            let mut forged_initial = initial.clone();
            forged_initial.digest.update(&[7]);
            forged_initial.hashed += 1;
            assert!(forged_initial.check().is_err(), "matching byte count cannot manufacture a non-fresh initial state");
            let mut overflowed = initial.clone();
            overflowed.hashed = u128::MAX;
            assert!(overflowed.check().is_err(), "corrupt digest byte count must fail without overflow panic");
            for ordinal in 0..total {
                let leaf = space.export_prefix_state_leaf_v2(&header.job_context, k, ordinal).unwrap();
                let opening =
                    step_opening_from_leaf_source_capped_v1(header.step_leaf_count, leaf.index, 1 << 26, |i| x.hashes[i as usize])
                        .unwrap();
                let values = &x.preimages[leaf.index as usize].values_le;
                if ordinal == 0 {
                    let before = stream.clone();
                    assert!(stream.leaf(&space, &header.job_context, &values[..values.len() - 1], &opening, 80_000).is_err());
                    assert_eq!(stream, before, "malformed body must not advance digest");
                }
                stream.leaf(&space, &header.job_context, values, &opening, 80_000).unwrap();
            }
            assert!(stream.same_contract(&initial));
            assert_eq!(
                stream.verdict(),
                Some(if declared == honest {
                    PalwTirPrefixStateVerdictV1::Honest
                } else {
                    PalwTirPrefixStateVerdictV1::StateRootMismatch { derived: honest }
                })
            );
            let mut forged_verdict = stream.clone();
            forged_verdict.verdict = Some(PalwTirPrefixStateVerdictV1::StateRootMismatch { derived: h(505) });
            assert!(forged_verdict.check().is_err(), "completed verdict must come from the actual digest");
            let bytes = borsh::to_vec(&stream).unwrap();
            let restored: TirExportPrefixStreamV2 = borsh::from_slice(&bytes).unwrap();
            assert_eq!(restored, stream);
            restored.check().unwrap();
        }
        for k in [0, 1, 5] {
            assert!(space.export_prefix_state_leaf_count_v2(&header.job_context, k).is_none());
        }
    }
}

crate::palw_native_resource_v1::native_struct!(TirPrefixExportSpecV2; intent_id, base_claim, source_id, job_identity, header_context, export_ordinal, prefix, liability_until, material_until, source_step_count, maximum_leaves, unit_count, maximum_values_bytes, maximum_response_bytes, total_values_bytes, total_response_bytes, opening_depth, leaf_replay_requests);
crate::palw_native_resource_v1::native_struct!(TirPrefixExportUnitV2; ordinal, source_leaf_index, value_count, values_bytes, response_bytes);

// Proposed native-export-owner assignment12:01 append candidate only.
// No live registration/profile/tag/price/aggregate/archive changes.
type PreparedSetFaultV2 = NativeDecodeErrorV1<String>;
fn prepared_set_bad_v2(s: &str) -> PreparedSetFaultV2 {
    NativeDecodeErrorV1::Decode(s.into())
}
fn prepared_set_add_v2(a: u64, b: u64) -> Result<u64, PreparedSetFaultV2> {
    a.checked_add(b)
        .ok_or_else(|| NativeDecodeErrorV1::Resource(NativeCoverageV1::overflow::<TirPrefixExportSpecV2>("prepared export set sum")))
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeSourceExportTotalsV2 {
    pub export_count: u64,
    pub unit_count: u64,
    pub values_bytes: u64,
    pub response_bytes: u64,
    /// Peak geometry only; A4 decides priced aggregate/moves and plan binding.
    pub maximum_values_bytes: u64,
    pub maximum_response_bytes: u64,
    pub maximum_leaf_replay_requests: u64,
}
/// Opaque, not Clone, not Deserialize. One leased source topology is shared by
/// every derived spec. Only complete verified sets can be handed to A4.
pub(crate) struct NativeSourceExportSetV2<'a> {
    source: NativeRetainedV1<TirExportSourceContextV2<'a>>,
    specs: Vec<TirPrefixExportSpecV2>,
    totals: NativeSourceExportTotalsV2,
    commitment: crate::Hash64,
    commitment_requests: u64,
    storage_requests: u64,
    canonical_encoded_bytes: u64,
}
impl NativeSourceExportSetV2<'_> {
    /// Prospective ordered identity only. A4 authenticates its native plan;
    /// this digest is not accepted wire/sourcegraph/admission authority.
    pub(crate) fn canonical_commitment_v3(&self) -> crate::Hash64 {
        self.commitment
    }
    /// Byte/traversal geometry only; A4 chooses existing-rate CPU payment. Two
    /// length traversals plus one hashing serialization are performed here.
    pub(crate) fn canonical_encoded_bytes_v3(&self) -> u64 {
        self.canonical_encoded_bytes
    }
    pub(crate) fn canonical_serialization_passes_v3(&self) -> u8 {
        3
    }
    pub(crate) fn commitment_requests_v2(&self) -> u64 {
        self.commitment_requests
    }
    pub(crate) fn storage_requests_v2(&self) -> u64 {
        self.storage_requests
    }
    pub(crate) fn totals_v2(&self) -> NativeSourceExportTotalsV2 {
        self.totals
    }
    pub(crate) fn source_preparation_requests_v2(&self) -> u64 {
        self.source.preparation_requests_v2()
    }
    /// Borrowed visitor; cannot reorder/mutate specs or take a context without
    /// retaining this set's native owner lease. No per-spec source hydration.
    pub(crate) fn visit_specs_v2<E>(
        &self,
        mut visit: impl FnMut(&TirExportSourceContextV2<'_>, &TirPrefixExportSpecV2) -> Result<(), E>,
    ) -> Result<(), E> {
        for spec in &self.specs {
            visit(&self.source, spec)?
        }
        Ok(())
    }
}
/// Empty exports deliberately delegate to the EXISTING no-export path without
/// hydrating source topology, inventing a zero-owner aggregate or charging A4.
/// Caller owns the native leases for borrowed original/source/registered/layout
/// and authenticated registered producer key. Actual quote/fold authority and
/// accepted point remain caller-owned; this function cannot publish any row.
pub(crate) fn prepare_native_source_export_set_v2<'a>(
    original: &'a PalwFreePromptCommitmentV3,
    source: &'a PalwTirPublicSourceV1,
    registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
    registered_producer_key: &[u8],
    maximum_leaves: u64,
    exports: &[PalwSignedPrefixExportV1],
) -> Result<Option<NativeRetainedV1<NativeSourceExportSetV2<'a>>>, PreparedSetFaultV2> {
    if exports.is_empty() {
        return Ok(None);
    }
    // Actual input order is consensus input. Never sort/dedup silently, and do
    // not turn ordinal into a dense array index: u32::MAX is a valid last value.
    let mut previous = None;
    for signed in exports {
        let ordinal = signed.intent.ordinal;
        if previous.is_some_and(|p| ordinal <= p) {
            return Err(prepared_set_bad_v2("export ordinals are not strictly increasing"));
        }
        previous = Some(ordinal);
    }
    let count = u64::try_from(exports.len())
        .map_err(|_| NativeDecodeErrorV1::Resource(NativeCoverageV1::overflow::<TirPrefixExportSpecV2>("prepared export count")))?;
    // Exact with_capacity count, not a growing Vec and not max(ordinal)+1.
    // Reserve before the first spec allocation. Existing source preparation
    // obtains and keeps its independent topology/identity scratch lease once.
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<TirPrefixExportSpecV2>(count).map_err(NativeDecodeErrorV1::Resource)?;
    q.exact_v1::<NativeRetainedV1<NativeSourceExportSetV2<'a>>>(1).map_err(NativeDecodeErrorV1::Resource)?;
    let storage_requests = q.bytes_v1();
    let lease = reserve_native_requests_v1(storage_requests).map_err(NativeDecodeErrorV1::Resource)?;
    let context = prepare_export_source_v2(original, source, registered, layout, registered_producer_key, maximum_leaves)?;
    let mut specs = Vec::with_capacity(exports.len());
    let mut totals = NativeSourceExportTotalsV2::default();
    for signed in exports {
        // Includes EVERY producer signature and original/class/job/header/root,
        // retention/geometry check. Do not expose partial sets on later failure.
        let spec = context.prepare_export_v2(signed)?;
        totals.export_count = prepared_set_add_v2(totals.export_count, 1)?;
        totals.unit_count = prepared_set_add_v2(totals.unit_count, spec.unit_count)?;
        totals.values_bytes = prepared_set_add_v2(totals.values_bytes, spec.total_values_bytes)?;
        totals.response_bytes = prepared_set_add_v2(totals.response_bytes, spec.total_response_bytes)?;
        totals.maximum_values_bytes = totals.maximum_values_bytes.max(spec.maximum_values_bytes);
        totals.maximum_response_bytes = totals.maximum_response_bytes.max(spec.maximum_response_bytes);
        totals.maximum_leaf_replay_requests = totals.maximum_leaf_replay_requests.max(spec.leaf_replay_requests);
        specs.push(spec);
    }
    if totals.export_count != count {
        return Err(prepared_set_bad_v2("prepared export count changed"));
    }
    let (commitment, commitment_requests, canonical_encoded_bytes) = native_source_ordered_spec_commitment_v3(&specs)?;
    Ok(Some(NativeRetainedV1::from_owner_reserved_v1(
        NativeSourceExportSetV2 {
            source: context,
            specs,
            totals,
            commitment,
            commitment_requests,
            storage_requests,
            canonical_encoded_bytes,
        },
        lease,
    )))
}

/// Every canonical spec in actual signed ordinal order is committed, including
/// intent ID (authenticated signature meaning), original source identity, all H,
/// and canonical count/value/response/depth geometry. No local sizeof-derived
/// workspace is in the digest. Signature randomness itself is not copied.
/// No Deserialize/standalone admission constructor is introduced.
fn native_source_ordered_spec_commitment_v3(specs: &[TirPrefixExportSpecV2]) -> Result<(crate::Hash64, u64, u64), PreparedSetFaultV2> {
    let value = (3u16, CanonicalTirExportSpecsV3(specs));
    let encoded = borsh::object_length(&value).map_err(|e| NativeDecodeErrorV1::Decode(e.to_string()))?;
    let encoded = u64::try_from(encoded).map_err(|_| {
        NativeDecodeErrorV1::Resource(NativeCoverageV1::overflow::<TirPrefixExportSpecV2>("ordered specs identity length"))
    })?;
    let requests = PalwTirPublicSourceV1::native_identity_writer_requests_v3(encoded)?;
    let _hash_lease = reserve_native_requests_v1(requests).map_err(NativeDecodeErrorV1::Resource)?;
    let digest =
        crate::Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/native-source-export-set/canonical/v3", &value));
    Ok((digest, requests, encoded))
}

#[cfg(test)]
mod prepared_set_tests {
    use super::*;
    use crate::palw_claim_local_v1::prefix_v1::{PREFIX_EXPORT_CONTEXT_V1, PalwPrefixExportIntentV1};
    use libcrux_ml_dsa::ml_dsa_87 as dsa;
    fn h(n: u64) -> crate::Hash64 {
        crate::Hash64::from_u64_word(n)
    }
    #[test]
    fn rfc40_prepared_export_set_shares_source_checks_all_signatures_and_sparse_max_ordinal() {
        let keys = dsa::generate_key_pair([81; 32]);
        let pk = keys.verification_key.as_ref().to_vec();
        let f =
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(2, |job| job.executor_pubkey = pk.clone(), None);
        let original = f.frame.original_v1();
        let maximum = f.frame.descriptor_v1().maximum_leaves;
        let source = PalwTirPublicSourceV1::new_native_v1(
            &f.record,
            &f.layout,
            f.frame.header_v1(),
            original.execution_root,
            original.trace_root,
            maximum,
        )
        .unwrap();
        let sign = |ordinal| {
            let intent = PalwPrefixExportIntentV1 {
                version: 1,
                network: original.job.network_domain,
                source_namespace: 1,
                source_nonce: h(31),
                base_commitment: crate::palw_freeprompt_v3::fp_claim_id_v3(original),
                producer_key_id: crate::mldsa87_primitives::mldsa87_key_id(&original.job.executor_pubkey),
                ordinal,
                prefix: crate::palw_freeprompt_v3::PalwFpPrefixStateV1 {
                    class_id: original.job.class_id,
                    state_root: h(32),
                    prefix_tokens: 2,
                },
                liability_until: original.trace_retention_daa - 1,
                material_until: original.trace_retention_daa,
            };
            let signature = dsa::sign(&keys.signing_key, intent.id_v1().as_byte_slice(), PREFIX_EXPORT_CONTEXT_V1, [82; 32])
                .unwrap()
                .as_ref()
                .to_vec();
            PalwSignedPrefixExportV1 { intent, signature }
        };
        let exports = [sign(0), sign(u32::MAX)];
        let prepare = |items: &[PalwSignedPrefixExportV1]| {
            prepare_native_source_export_set_v2(original, &source, &f.record, &f.layout, &pk, maximum, items)
        };
        assert!(prepare(&[]).unwrap().is_none());
        let set = prepare(&exports).unwrap().unwrap();
        assert_eq!(set.totals_v2().export_count, 2);
        let mut sums = (0u64, 0u64, 0u64);
        let mut first_context = None;
        set.visit_specs_v2::<PreparedSetFaultV2>(|context, spec| {
            let pointer = std::ptr::from_ref(context).cast::<()>();
            if let Some(first) = first_context {
                assert_eq!(first, pointer)
            } else {
                first_context = Some(pointer)
            }
            sums.0 = prepared_set_add_v2(sums.0, spec.unit_count)?;
            sums.1 = prepared_set_add_v2(sums.1, spec.total_values_bytes)?;
            sums.2 = prepared_set_add_v2(sums.2, spec.total_response_bytes)?;
            Ok(())
        })
        .unwrap();
        assert_eq!(sums, (set.totals_v2().unit_count, set.totals_v2().values_bytes, set.totals_v2().response_bytes));
        assert_eq!(set.specs.capacity(), exports.len());
        let again = prepare(&exports).unwrap().unwrap();
        assert_eq!(set.canonical_commitment_v3(), again.canonical_commitment_v3(), "deterministic same signed ordered specs");
        assert_eq!(set.canonical_commitment_v3(), native_source_ordered_spec_commitment_v3(&set.specs).unwrap().0);
        assert!(set.commitment_requests_v2() > 0);
        let baseline = set.canonical_commitment_v3();
        let assert_change = |change: fn(&mut TirPrefixExportSpecV2)| {
            let mut specs = set.specs.clone();
            change(&mut specs[0]);
            assert_ne!(native_source_ordered_spec_commitment_v3(&specs).unwrap().0, baseline);
        };
        assert_change(|s| s.intent_id = h(900));
        assert_change(|s| s.base_claim = h(901));
        assert_change(|s| s.source_id = h(902));
        assert_change(|s| s.job_identity = h(903));
        assert_change(|s| s.header_context = h(904));
        assert_change(|s| s.export_ordinal = 1);
        assert_change(|s| s.prefix.class_id = h(950));
        assert_change(|s| s.prefix.state_root = h(950));
        assert_change(|s| s.prefix.prefix_tokens += 1);
        assert_change(|s| s.liability_until += 1);
        assert_change(|s| s.material_until += 1);
        assert_change(|s| s.source_step_count += 1);
        assert_change(|s| s.maximum_leaves += 1);
        assert_change(|s| s.unit_count += 1);
        assert_change(|s| s.maximum_values_bytes += 1);
        assert_change(|s| s.maximum_response_bytes += 1);
        assert_change(|s| s.total_values_bytes += 1);
        assert_change(|s| s.total_response_bytes += 1);
        assert_change(|s| s.opening_depth += 1);
        let mut local_only = set.specs.clone();
        local_only[0].leaf_replay_requests = local_only[0].leaf_replay_requests.checked_add(1).unwrap();
        assert_ne!(local_only[0].actual_leaf_workspace_requests_v2(), set.specs[0].actual_leaf_workspace_requests_v2());
        assert_eq!(
            native_source_ordered_spec_commitment_v3(&local_only).unwrap().0,
            baseline,
            "local ABI workspace is not canonical identity"
        );
        assert_eq!(native_source_ordered_spec_commitment_v3(&local_only).unwrap().2, set.canonical_encoded_bytes_v3());
        assert_eq!(set.canonical_serialization_passes_v3(), 3);
        // Canonical fields are fixed-width; no host layout/padding enters Borsh.
        assert_eq!(set.canonical_encoded_bytes_v3(), 6 + 536 * set.totals_v2().export_count);
        let swapped = [set.specs[1], set.specs[0]];
        assert_ne!(native_source_ordered_spec_commitment_v3(&swapped).unwrap().0, baseline, "full spec order changes digest");
        let resigned = |ordinal| {
            let mut e = sign(ordinal);
            e.signature = dsa::sign(&keys.signing_key, e.intent.id_v1().as_byte_slice(), PREFIX_EXPORT_CONTEXT_V1, [83; 32])
                .unwrap()
                .as_ref()
                .to_vec();
            e
        };
        let equivalent = [resigned(0), resigned(u32::MAX)];
        assert_eq!(
            prepare(&equivalent).unwrap().unwrap().canonical_commitment_v3(),
            baseline,
            "same authenticated intents do not bind arbitrary signature randomness"
        );

        let reversed = [exports[1].clone(), exports[0].clone()];
        assert!(prepare(&reversed).is_err());
        let duplicate = [exports[0].clone(), exports[0].clone()];
        assert!(prepare(&duplicate).is_err());
        let mut late_bad = exports.clone();
        late_bad[1].signature[0] ^= 1;
        assert!(prepare(&late_bad).is_err(), "late invalid signature cannot expose a partial set");
        assert!(prepared_set_add_v2(u64::MAX, 1).is_err());
    }
}
