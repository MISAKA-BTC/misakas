//! Explicit step geometry and construction for the strict i64 Q24 logits profile.
//!
//! Only the actual final post logits node uses leaf version 2 and eight-byte signed lanes.
//! All preceding node, checkpoint and history leaves keep their legacy preimages. This module
//! introduces no binding version, hash domain, registry admission or native activation.
//! Leaf-count caps are logical count limits, not heap reservations: callers must separately
//! reserve the builder's hash vector and Merkle construction temporaries.

use super::*;
use crate::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1;
use crate::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PALW_V2_MAX_NETWORK_ID_BYTES};
use misaka_palw_tir::program::{MAX_BLOCKS, MAX_NODES_PER_BLOCK, MAX_PROGRAM_BYTES, MAX_STATES};

/// Existing generic step-leaf DTO version for the explicit eight-byte final logits leaves.
pub const PALW_TIR_STEP_LEAF_VERSION_V2: u16 = 2;

/// Immutable mixed-width geometry. It cannot be converted into a legacy V1 step space.
#[derive(Clone, Debug)]
pub struct PalwTirStepSpaceV2 {
    inner: PalwTirStepSpaceV1,
}

fn canonical_error(msg: impl Into<String>) -> PalwTirStepError {
    PalwTirStepError::NotCanonical(msg.into())
}

fn check_context(ctx: &PalwJobContextV2) -> Result<(), PalwTirStepError> {
    if ctx.version != PALW_TRACE_COMMITMENT_VERSION_V2 {
        return Err(PalwTirStepError::Job("wide steps require an ordinary v2 job context"));
    }
    if ctx.network_id.is_empty() || ctx.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES {
        return Err(PalwTirStepError::Job("network id is empty or over the cap"));
    }
    if ctx.declared_prefill_tokens == 0 || ctx.exact_decode_tokens == 0 {
        return Err(PalwTirStepError::Job("prefill and decode counts must be positive"));
    }
    if !matches!(ctx.declared_prefill_tokens.checked_add(ctx.exact_decode_tokens), Some(n) if n <= ctx.max_context_tokens) {
        return Err(PalwTirStepError::Job("token budget overflows or exceeds max_context_tokens"));
    }
    if ctx.trace_scheme_id != crate::palw_logits_q24_v2::scheme_id() {
        return Err(PalwTirStepError::Job("the context does not pin the exact wide Q24 scheme"));
    }
    Ok(())
}

impl PalwTirStepSpaceV2 {
    /// Decode and validate the actual strict-wide program, then check its original layout.
    pub fn new(class: &PalwTirClassV1) -> Result<Self, PalwTirStepError> {
        if class.version != PALW_TIR_CLASS_VERSION_V1 {
            return layout_err("unsupported class version");
        }
        // Format bounds precede layout cloning and later class-id serialization.
        if class.program.len() > MAX_PROGRAM_BYTES
            || class.layout.commit_tiles.len() > MAX_BLOCKS * MAX_NODES_PER_BLOCK
            || class.layout.state_tiles.len() > MAX_STATES
        {
            return layout_err("class exceeds the existing program/layout format bounds");
        }
        let program = class.decode_program().map_err(|e| PalwTirStepError::Program(e.to_string()))?;
        let info = misaka_palw_tir::validate::validate(&program).map_err(|e| PalwTirStepError::Program(e.to_string()))?;
        Self::from_validated(program, info, class.layout.clone())
    }

    /// Revalidate the actual program, refusing stale or forged validation metadata.
    pub fn from_program(program: TirProgramV1, info: ProgramInfo, layout: PalwTirLayoutV1) -> Result<Self, PalwTirStepError> {
        let actual = misaka_palw_tir::validate::validate(&program).map_err(|e| PalwTirStepError::Program(e.to_string()))?;
        if info != actual {
            return Err(PalwTirStepError::Program("validation metadata does not match the actual program".into()));
        }
        Self::from_validated(program, actual, layout)
    }

    fn from_validated(program: TirProgramV1, info: ProgramInfo, layout: PalwTirLayoutV1) -> Result<Self, PalwTirStepError> {
        if !program.wide_logits_q24() {
            return layout_err("v2 steps require the exact wide Q24 scheme");
        }
        // validate() enforces final post i64, distinct scheduled blocks, narrow state/carries,
        // and rejects every other committed wide node. Repeat the local profile pin explicitly.
        let post = &program.blocks[program.schedule.post as usize];
        let logits = &post.nodes[program.logits as usize];
        if program.logits as usize + 1 != post.nodes.len() || logits.out.dtype != DType::I64 || !logits.commit {
            return layout_err("wide logits must be the committed i64 final post node");
        }
        Ok(Self { inner: PalwTirStepSpaceV1::from_program_geometry(program, info, layout)? })
    }

    pub fn program(&self) -> &TirProgramV1 {
        &self.inner.program
    }
    pub fn info(&self) -> &ProgramInfo {
        &self.inner.info
    }
    pub fn layout(&self) -> &PalwTirLayoutV1 {
        &self.inner.layout
    }
    pub fn fixed_instances(&self) -> &[PalwTirStateInstanceV1] {
        self.inner.fixed_instances()
    }
    pub fn hist_instances(&self) -> &[PalwTirStateInstanceV1] {
        self.inner.hist_instances()
    }
    pub fn program_slots(&self) -> u32 {
        self.inner.program_slots()
    }
    pub fn commit_tile_len(&self, block: u8, node: u16) -> Option<u32> {
        self.inner.commit_tile_len(block, node)
    }
    pub fn fixed_instance_index(&self, state: u16, layer: Option<u16>) -> Option<usize> {
        self.inner.fixed_instance_index(state, layer)
    }
    pub fn hist_instance_index(&self, state: u16, layer: Option<u16>) -> Option<usize> {
        self.inner.hist_instance_index(state, layer)
    }
    pub fn occurrences(&self) -> &[(u8, Option<u16>)] {
        self.inner.occurrences()
    }
    pub fn node_slot(&self, occurrence: usize, node: u16) -> Option<u32> {
        self.inner.node_slot(occurrence, node)
    }
    pub fn resolve_slot(&self, slot: u32) -> Option<(usize, u16)> {
        self.inner.resolve_slot(slot)
    }
    pub fn job_shape(&self, ctx: &PalwJobContextV2) -> Result<PalwTirJobShapeV1, PalwTirStepError> {
        check_context(ctx)?;
        self.inner.job_shape(ctx)
    }
    pub fn leaf_count_capped(&self, ctx: &PalwJobContextV2, cap: u64) -> Result<u64, PalwTirStepError> {
        self.job_shape(ctx)?;
        self.inner.leaf_count_capped(ctx, cap)
    }
    pub fn leaf_at(&self, ctx: &PalwJobContextV2, index: u64) -> Option<PalwTirLeafV1> {
        self.job_shape(ctx).ok()?;
        self.inner.leaf_at(ctx, index)
    }
    pub fn leaf_index(&self, ctx: &PalwJobContextV2, coord: &PalwStepCoordinateV1) -> Option<u64> {
        self.job_shape(ctx).ok()?;
        self.inner.leaf_index(ctx, coord)
    }
    pub fn leaves_of_position(&self, ctx: &PalwJobContextV2, a: u32) -> Vec<PalwTirLeafV1> {
        if self.job_shape(ctx).is_err() {
            return Vec::new();
        }
        self.inner.leaves_of_position(ctx, a)
    }
    pub fn leaves_at_position(&self, job: &PalwTirJobShapeV1, a: u32) -> u64 {
        self.inner.leaves_at_position(job, a)
    }
    pub fn running_total(&self, job: &PalwTirJobShapeV1, a: u32) -> u128 {
        self.inner.running_total(job, a)
    }
    pub fn maximum_leaf_lanes_v1(&self, job: &PalwTirJobShapeV1) -> u64 {
        self.inner.maximum_leaf_lanes_v1(job)
    }
    pub fn maximum_window_lanes_v1(&self, job: &PalwTirJobShapeV1, count: u64) -> u128 {
        self.inner.maximum_window_lanes_v1(job, count)
    }
    pub fn lane_count_at_position_v1(&self, job: &PalwTirJobShapeV1, a: u32) -> u128 {
        self.inner.lane_count_at_position_v1(job, a)
    }

    /// Conservative maximum lane payload bytes, using eight bytes even for narrow leaves.
    /// This excludes the serialized leaf header, decoded DTO, openings, builder hashes,
    /// Merkle temporaries and allocator overhead; native callers reserve those separately.
    pub fn maximum_leaf_payload_bytes_v2(&self, ctx: &PalwJobContextV2) -> Result<u64, PalwTirStepError> {
        let job = self.job_shape(ctx)?;
        self.inner.maximum_leaf_lanes_v1(&job).checked_mul(8).ok_or_else(|| canonical_error("maximum leaf bytes overflow"))
    }

    // This classifier accepts only the entire freshly rederived canonical leaf, never a caller's
    // dtype or a width flag. Its result is not exposed as a generic encoding capability.
    fn leaf_width(&self, ctx: &PalwJobContextV2, leaf: &PalwTirLeafV1) -> Result<usize, PalwTirStepError> {
        if self.leaf_at(ctx, leaf.index).as_ref() != Some(leaf) {
            return Err(canonical_error("the leaf is not this space's canonical leaf at its index"));
        }
        match leaf.kind {
            PalwTirLeafKindV1::Commit { occurrence, block, layer, node, .. }
                if block == self.program().schedule.post
                    && node == self.program().logits
                    && occurrence as usize + 1 == self.occurrences().len()
                    && layer.is_none()
                    && leaf.dtype == DType::I64 =>
            {
                Ok(8)
            }
            _ if leaf.dtype.committable() => Ok(4),
            _ => Err(canonical_error("a non-final leaf is not a narrow committed dtype")),
        }
    }
}

/// Strict producer preimage, tied to the exact canonical leaf and original context.
pub fn palw_tir_leaf_preimage_v2(
    space: &PalwTirStepSpaceV2,
    ctx: &PalwJobContextV2,
    leaf: &PalwTirLeafV1,
    values: &[i128],
) -> Result<PalwStepTileLeafV1, PalwTirStepError> {
    let width = space.leaf_width(ctx, leaf)?;
    if values.len() != leaf.value_count as usize {
        return Err(canonical_error("the value count is not the canonical leaf's"));
    }
    if width == 4 {
        return palw_tir_leaf_preimage_v1(leaf, values);
    }
    let bytes = values.len().checked_mul(8).ok_or_else(|| canonical_error("leaf byte length overflow"))?;
    let mut values_le = Vec::new();
    values_le.try_reserve_exact(bytes).map_err(|_| canonical_error("leaf byte allocation failed"))?;
    for value in values {
        let lane = i64::try_from(*value).map_err(|_| canonical_error("wide logits lane is outside i64"))?;
        values_le.extend_from_slice(&lane.to_le_bytes());
    }
    Ok(PalwStepTileLeafV1 { version: PALW_TIR_STEP_LEAF_VERSION_V2, coord: leaf.coord, value_count: leaf.value_count, values_le })
}

/// Read a disclosed canonical mixed-width preimage without judging its narrow dtype interval.
/// This checks geometry/encoding only; membership and deterministic recomputation are separate.
pub fn palw_tir_leaf_values_v2(
    space: &PalwTirStepSpaceV2,
    ctx: &PalwJobContextV2,
    leaf: &PalwTirLeafV1,
    preimage: &PalwStepTileLeafV1,
) -> Result<Vec<i128>, PalwTirStepError> {
    let width = space.leaf_width(ctx, leaf)?;
    let version = if width == 8 { PALW_TIR_STEP_LEAF_VERSION_V2 } else { PALW_TIR_STEP_LEAF_VERSION_V1 };
    let bytes = (leaf.value_count as usize).checked_mul(width).ok_or_else(|| canonical_error("leaf byte length overflow"))?;
    if preimage.version != version
        || preimage.coord != leaf.coord
        || preimage.value_count != leaf.value_count
        || preimage.values_le.len() != bytes
    {
        return Err(canonical_error("disclosed preimage version, geometry or lane width is not canonical"));
    }
    if width == 4 {
        return palw_tir_lane_values_v1(leaf.dtype, &preimage.values_le);
    }
    Ok(preimage.values_le.chunks_exact(8).map(|c| i64::from_le_bytes(c.try_into().expect("eight-byte chunk")) as i128).collect())
}

/// Canonical mixed-width producer builder. The count cap does not reserve heap or native work.
pub struct PalwTirStepLegBuilderV2<'s> {
    space: &'s PalwTirStepSpaceV2,
    ctx: PalwJobContextV2,
    context_hash: Hash64,
    class_id: Hash64,
    total: u64,
    cap: u64,
    leaf_hashes: Vec<Hash64>,
}

impl<'s> PalwTirStepLegBuilderV2<'s> {
    pub fn new(space: &'s PalwTirStepSpaceV2, ctx: &PalwJobContextV2, class_id: Hash64, cap: u64) -> Result<Self, PalwTirStepError> {
        if class_id != ctx.shape_profile_id {
            return Err(PalwTirStepError::Binding("builder class id differs from the original job context"));
        }
        let total = space.leaf_count_capped(ctx, cap)?;
        Ok(Self { space, ctx: ctx.clone(), context_hash: ctx.context_hash(), class_id, total, cap, leaf_hashes: Vec::new() })
    }
    pub fn next_leaf(&self) -> Option<PalwTirLeafV1> {
        self.space.leaf_at(&self.ctx, self.leaf_hashes.len() as u64)
    }
    pub fn push(&mut self, values: &[i128]) -> Result<PalwTirLeafV1, PalwTirStepError> {
        let leaf = self.next_leaf().ok_or_else(|| canonical_error("the leg is already complete"))?;
        let preimage = palw_tir_leaf_preimage_v2(self.space, &self.ctx, &leaf, values)?;
        self.leaf_hashes.try_reserve(1).map_err(|_| canonical_error("step hash allocation failed"))?;
        self.leaf_hashes.push(step_tile_leaf_hash_v1(&self.context_hash, &self.class_id, &preimage));
        Ok(leaf)
    }
    pub fn finish(self) -> Result<(u64, Hash64), PalwTirStepError> {
        if self.leaf_hashes.len() as u64 != self.total {
            return Err(canonical_error("the leg is incomplete"));
        }
        let root = step_merkle_root_capped_v1(&self.leaf_hashes, self.cap).map_err(|e| canonical_error(e.to_string()))?;
        Ok((self.total, root))
    }
}

/// Result of the explicit V2 binding validator; no public mutable legacy-space escape.
#[derive(Clone, Debug)]
pub struct PalwTirVerifiedBindingV2 {
    space: PalwTirStepSpaceV2,
    class_id: Hash64,
    context_hash: Hash64,
    job: PalwTirJobShapeV1,
}
impl PalwTirVerifiedBindingV2 {
    pub fn space(&self) -> &PalwTirStepSpaceV2 {
        &self.space
    }
    pub fn class_id(&self) -> Hash64 {
        self.class_id
    }
    pub fn context_hash(&self) -> Hash64 {
        self.context_hash
    }
    pub fn job(&self) -> PalwTirJobShapeV1 {
        self.job
    }
}

/// Explicitly validate the existing version-1 binding DTO under the strict wide-step profile.
/// Hashes authenticate the unchanged context and committed data; they do not verify computation.
pub fn verify_tir_binding_v2(
    binding: &PalwTirStepBindingV1,
    max_step_leaf_count: u64,
) -> Result<PalwTirVerifiedBindingV2, PalwTirStepError> {
    if binding.version != PALW_TIR_STEP_BINDING_VERSION_V1 {
        return Err(PalwTirStepError::Binding("unsupported binding version"));
    }
    check_context(&binding.job_context)?;
    let space = PalwTirStepSpaceV2::new(&binding.class)?;
    let class_id = binding.class.class_id(&binding.artifact_root);
    if binding.job_context.shape_profile_id != class_id {
        return Err(PalwTirStepError::Binding("the job context names another class"));
    }
    let job = space.job_shape(&binding.job_context)?;
    if space.leaf_count_capped(&binding.job_context, max_step_leaf_count)? != binding.step_leaf_count {
        return Err(PalwTirStepError::Binding("the step leaf count is not the job's canonical count"));
    }
    let context_hash = binding.job_context.context_hash();
    let root = palw_tir_execution_root_v1(
        &context_hash,
        &binding.full_logits_trace_root,
        &class_id,
        binding.step_leaf_count,
        &binding.step_merkle_root,
    );
    if root != binding.committed_execution_root {
        return Err(PalwTirStepError::Binding("the parts do not produce the committed execution root"));
    }
    Ok(PalwTirVerifiedBindingV2 { space, class_id, context_hash, job })
}
