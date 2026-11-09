//! Bounded PALWTRC2 data capture of an existing wide run, without model execution.
//! No generic BorshDeserialize, V1 dispatch, native admission or arithmetic verdict.
//! Caps are logical request limits, not host leases. Independently retained input
//! frames/runs/expected metadata/spaces/exports must be included by the caller in
//! external_retained_bytes when the current operation does not already count them.
pub(super) mod budget;
pub use budget::{WideCaptureAuthLimitsV2, WideCaptureLimitsV2, WideCaptureQuoteV2};

use super::{capture_codec, memory, run_wide_q24_v2::TirJobRunWideQ24V2};
use borsh::BorshSerialize;
use kaspa_consensus_core::{
    palw_logits_q24_v2 as wide,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1},
    palw_step_leg::{PalwStepTileLeafV1, step_merkle_root_capped_v1, step_tile_leaf_hash_v1},
    palw_step_refute::PALW_LOGITS_TILE_LANES,
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_step_v1::{PalwTirStepBindingV1, PalwTirVerifiedBindingV2, palw_tir_execution_root_v1, palw_tir_leaf_values_v2},
    palw_v2::PalwJobContextV2,
};
use std::io::{self, Write};

pub const TIR_WIDE_CAPTURE_MAGIC_V2: [u8; 8] = *b"PALWTRC2";

/// Wire body: existing binding DTO version1, prompt, eight-byte rows, IDs, leaves.
/// Parsing and authentication are separate. Bounded false evidence is retained.
#[derive(Debug, PartialEq, Eq, BorshSerialize)]
pub struct TirWideCaptureV2 {
    pub(crate) binding: PalwTirStepBindingV1,
    pub(crate) prompt: Vec<u32>,
    pub(crate) logits_rows: Vec<Vec<i64>>,
    pub(crate) generated: Vec<u32>,
    pub(crate) leaves: Vec<PalwStepTileLeafV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideCaptureAuthQuoteV2 {
    pub work: u64,
    pub scratch_bytes: u64,
    pub capture_bytes: u64,
    pub external_retained_bytes: u64,
    pub operation_peak_bytes: u64,
}

fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "wide capture arithmetic overflow".into())
}
fn mul(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or_else(|| "wide capture arithmetic overflow".into())
}
fn count(n: usize) -> Result<u64, String> {
    u64::try_from(n).map_err(|_| "wide capture index width".into())
}
fn reserve<T>(n: usize) -> Result<Vec<T>, String> {
    let mut out = Vec::new();
    out.try_reserve_exact(n).map_err(|_| "wide capture allocation refused")?;
    if out.capacity() != n {
        return Err("wide capture allocator exceeded exact request".into());
    }
    Ok(out)
}
fn copy<T: Copy>(values: &[T]) -> Result<Vec<T>, String> {
    let mut out = reserve(values.len())?;
    out.extend_from_slice(values);
    Ok(out)
}
fn copy_binding(b: &PalwTirStepBindingV1) -> Result<PalwTirStepBindingV1, String> {
    let j = &b.job_context;
    let l = &b.class.layout;
    let job_context = PalwJobContextV2 {
        version: j.version,
        network_id: copy(&j.network_id)?,
        job_id: j.job_id,
        job_nullifier: j.job_nullifier,
        assignment_id: j.assignment_id,
        execution_seed: j.execution_seed,
        model_profile_id: j.model_profile_id,
        runtime_manifest_hash: j.runtime_manifest_hash,
        runtime_class_id: j.runtime_class_id,
        shape_profile_id: j.shape_profile_id,
        trace_scheme_id: j.trace_scheme_id,
        cu_ruleset_id: j.cu_ruleset_id,
        tokenizer_id: j.tokenizer_id,
        prompt_token_ids_hash: j.prompt_token_ids_hash,
        declared_prefill_tokens: j.declared_prefill_tokens,
        exact_decode_tokens: j.exact_decode_tokens,
        max_context_tokens: j.max_context_tokens,
    };
    let class = PalwTirClassV1 {
        version: b.class.version,
        program: copy(&b.class.program)?,
        tokenizer_id: b.class.tokenizer_id,
        layout: PalwTirLayoutV1 {
            version: l.version,
            max_context: l.max_context,
            checkpoint_interval: l.checkpoint_interval,
            h_tile: l.h_tile,
            commit_tiles: copy(&l.commit_tiles)?,
            state_tiles: copy(&l.state_tiles)?,
        },
    };
    Ok(PalwTirStepBindingV1 {
        version: b.version,
        job_context,
        class,
        artifact_root: b.artifact_root,
        full_logits_trace_root: b.full_logits_trace_root,
        step_leaf_count: b.step_leaf_count,
        step_merkle_root: b.step_merkle_root,
        committed_execution_root: b.committed_execution_root,
    })
}

impl TirWideCaptureV2 {
    pub const MAGIC: [u8; 8] = TIR_WIDE_CAPTURE_MAGIC_V2;
    pub fn binding(&self) -> &PalwTirStepBindingV1 {
        &self.binding
    }
    pub fn prompt(&self) -> &[u32] {
        &self.prompt
    }
    pub fn logits_rows(&self) -> &[Vec<i64>] {
        &self.logits_rows
    }
    pub fn generated(&self) -> &[u32] {
        &self.generated
    }
    pub fn leaves(&self) -> &[PalwStepTileLeafV1] {
        &self.leaves
    }
    pub fn event(&self, row: u32, tile: u32) -> Option<wide::WideLogitsEventV2> {
        super::wide_q24::event(&self.binding.job_context, &self.logits_rows, &self.generated, row, tile)
    }
    pub fn is_dense(&self) -> bool {
        !self.leaves.is_empty()
    }
    pub fn backing_bytes(&self) -> Result<u64, String> {
        budget::capture_backing(self)
    }

    /// The actual capacity of the incoming owned leaf Vec matters. It is moved,
    /// not copied, and must have been admitted by its acquiring caller already.
    #[allow(clippy::ptr_arg)]
    pub fn quote_from_run(
        expected: &PalwTirStepBindingV1,
        prompt: &[u32],
        run: &TirJobRunWideQ24V2,
        leaves: &Vec<PalwStepTileLeafV1>,
        limits: WideCaptureLimitsV2,
    ) -> Result<WideCaptureQuoteV2, String> {
        budget::quote_build(expected, prompt, run, leaves, limits)
    }

    /// Copy the existing run, never execute/replay it. Dense leaf buffers are
    /// caller-acquired data; authenticate_expected must verify their membership.
    pub fn from_run(
        expected: &PalwTirStepBindingV1,
        prompt: &[u32],
        run: &TirJobRunWideQ24V2,
        leaves: Vec<PalwStepTileLeafV1>,
        limits: WideCaptureLimitsV2,
    ) -> Result<Self, String> {
        let quote = Self::quote_from_run(expected, prompt, run, &leaves, limits)?;
        let ctx = &expected.job_context;
        let class_id = expected.class.class_id(&expected.artifact_root);
        let context = ctx.context_hash();
        if ctx.trace_scheme_id != wide::scheme_id()
            || ctx.shape_profile_id != class_id
            || prompt.len() as u64 != u64::from(ctx.declared_prefill_tokens)
            || run.generated().len() as u64 != u64::from(ctx.exact_decode_tokens)
            || !prompt_token_ids_match_v1(PalwPromptIdsFormV1::Flat, prompt, &ctx.prompt_token_ids_hash)
            || run.leaf_count() != expected.step_leaf_count
            || run.step_merkle_root() != expected.step_merkle_root
            || run.trace_root() != expected.full_logits_trace_root
            || run.execution_root() != expected.committed_execution_root
            || palw_tir_execution_root_v1(&context, &run.trace_root(), &class_id, run.leaf_count(), &run.step_merkle_root())
                != run.execution_root()
            || (!leaves.is_empty() && leaves.len() as u64 != run.leaf_count())
        {
            return Err("wide capture source run differs from independent original binding".into());
        }
        let binding = copy_binding(expected)?;
        let prompt = copy(prompt)?;
        let generated = copy(run.generated())?;
        let mut logits_rows = reserve(run.logits_rows().len())?;
        for row in run.logits_rows() {
            logits_rows.push(copy(row)?);
        }
        let capture = Self { binding, prompt, logits_rows, generated, leaves };
        if capture.backing_bytes()? != quote.capture_bytes {
            return Err("wide capture private copy backing differs from quote".into());
        }
        Ok(capture)
    }

    pub fn decode_bounded(frame: &[u8], limits: WideCaptureLimitsV2) -> Result<Self, String> {
        let body = frame.strip_prefix(&Self::MAGIC).ok_or("not a PALWTRC2 capture")?;
        capture_codec::decode_wide(body, frame.len(), limits)
    }

    pub fn quote_encode(&self, limits: WideCaptureLimitsV2) -> Result<WideCaptureQuoteV2, String> {
        budget::quote_encode(self, limits)
    }

    /// One new frame. Earlier frames/explicit copies retained by the caller must
    /// be added to external_retained_bytes; this cap is not a live host lease.
    pub fn encode_bounded(&self, limits: WideCaptureLimitsV2) -> Result<Vec<u8>, String> {
        let quote = self.quote_encode(limits)?;
        let mut frame = reserve(usize::try_from(quote.encoded_frame_bytes).map_err(|_| "wide frame allocation width")?)?;
        frame.extend_from_slice(&Self::MAGIC);
        self.serialize(&mut frame).map_err(|e| format!("wide capture serialization: {e}"))?;
        if count(frame.len())? != quote.encoded_frame_bytes || frame.capacity() != frame.len() {
            return Err("wide frame size differs from quote".into());
        }
        Ok(frame)
    }

    /// Borrow an independently pre-admitted opaque V2 space. This function never
    /// decodes a program or constructs another StepSpace. Its external owner,
    /// expected binding, source frame/run and prior exports must be pre-summed
    /// into external_retained_bytes when retained concurrently by the caller.
    pub fn quote_auth_expected(
        &self,
        expected: &PalwTirStepBindingV1,
        verified: &PalwTirVerifiedBindingV2,
        limits: WideCaptureAuthLimitsV2,
    ) -> Result<WideCaptureAuthQuoteV2, String> {
        let metadata = budget::metadata_scratch(expected, self.prompt.len())?;
        // The independently owned binding is a known part of the caller's
        // declared external inventory. Its pre-admitted opaque space and all
        // other retained inputs must also be included by that caller.
        if limits.external_retained_bytes < budget::binding_backing(expected)? {
            return Err("wide capture external expected-binding reservation is too small".into());
        }
        let space = verified.space();
        let ctx = &expected.job_context;
        let job = space.job_shape(ctx).map_err(|e| e.to_string())?;
        let n = space.leaf_count_capped(ctx, limits.max_step_leaves).map_err(|e| e.to_string())?;
        let p = space.program();
        let vocab = p.blocks[p.schedule.post as usize].nodes[p.logits as usize].out.elements_at(1);
        if expected.step_leaf_count != n
            || (self.leaves.len() as u64 != n && !self.leaves.is_empty())
            || self.prompt.len() as u64 != u64::from(job.prefill)
            || self.generated.len() as u64 != u64::from(ctx.exact_decode_tokens)
            || self.logits_rows.len() != self.generated.len()
            || vocab == 0
            || vocab > u64::from(u32::MAX)
        {
            return Err("wide capture original prompt/row/leaf shape".into());
        }
        let dense = if self.leaves.is_empty() { 0 } else { n };
        if add(dense, u64::from(ctx.exact_decode_tokens))? > limits.max_work {
            return Err("wide capture authentication minimum visit ceiling".into());
        }
        if self.logits_rows.iter().any(|r| r.len() as u64 != vocab) {
            return Err("wide capture row width differs from original program".into());
        }
        let capture_bytes = self.backing_bytes()?;
        let payload = space.maximum_leaf_payload_bytes_v2(ctx).map_err(|e| e.to_string())?;
        let scratch_bytes = [
            metadata,
            memory::tir_fold_hash_storage_need_v1(dense)?,
            memory::tir_fold_hash_storage_need_v1(u64::from(ctx.exact_decode_tokens))?,
            memory::tir_fold_hash_storage_need_v1(vocab.div_ceil(PALW_LOGITS_TILE_LANES as u64))?,
            add(mul(payload, 6)?, 2048)?,
        ]
        .into_iter()
        .try_fold(0, add)?;
        let geometry = space.occurrences().iter().try_fold(
            add(
                count(expected.class.program.len())?,
                add(count(space.fixed_instances().len())?, count(space.hist_instances().len())?)?,
            )?,
            |g, (b, _)| add(g, add(1, count(p.blocks[*b as usize].nodes.len())?)?),
        )?;
        let work = add(
            add(mul(add(dense, 1)?, mul(add(geometry, 1024)?, 128)?)?, mul(mul(dense, payload)?, 8)?)?,
            add(mul(mul(vocab, u64::from(ctx.exact_decode_tokens))?, 128)?, mul(u64::from(job.prefill), 64)?)?,
        )?;
        let operation_peak_bytes = add(capture_bytes, add(scratch_bytes, limits.external_retained_bytes)?)?;
        if work > limits.max_work || scratch_bytes > limits.max_scratch_bytes || operation_peak_bytes > limits.max_operation_bytes {
            return Err("wide capture authentication work/scratch/operation ceiling".into());
        }
        // Literal vector comparison is admitted by the metadata/work ceiling;
        // it precedes all private authentication allocations and hashing.
        if self.binding != *expected {
            return Err("wide capture binding is not the independently expected binding".into());
        }
        Ok(WideCaptureAuthQuoteV2 {
            work,
            scratch_bytes,
            capture_bytes,
            external_retained_bytes: limits.external_retained_bytes,
            operation_peak_bytes,
        })
    }

    /// Authenticate data identity only. Invalid generated IDs, wrong selection,
    /// and disagreement between two separately committed values remain evidence
    /// for deterministic checking. No selector evaluation or model replay occurs.
    pub fn authenticate_expected(
        &self,
        expected: &PalwTirStepBindingV1,
        verified: &PalwTirVerifiedBindingV2,
        limits: WideCaptureAuthLimitsV2,
    ) -> Result<(), String> {
        self.quote_auth_expected(expected, verified, limits)?;
        let space = verified.space();
        let ctx = &expected.job_context;
        let class_id = expected.class.class_id(&expected.artifact_root);
        let context = ctx.context_hash();
        if class_id != verified.class_id()
            || class_id != ctx.shape_profile_id
            || context != verified.context_hash()
            || space.layout() != &expected.class.layout
            || verified.job() != space.job_shape(ctx).map_err(|e| e.to_string())?
            || !same_program(space.program(), &expected.class.program)?
            || palw_tir_execution_root_v1(
                &context,
                &expected.full_logits_trace_root,
                &class_id,
                expected.step_leaf_count,
                &expected.step_merkle_root,
            ) != expected.committed_execution_root
        {
            return Err("wide capture expected binding and pre-admitted V2 space differ".into());
        }
        if !prompt_token_ids_match_v1(PalwPromptIdsFormV1::Flat, &self.prompt, &ctx.prompt_token_ids_hash)
            || wide::trace_root(ctx, &self.logits_rows, &self.generated) != Some(expected.full_logits_trace_root)
        {
            return Err("wide capture prompt/trace does not match original committed data".into());
        }
        if self.leaves.is_empty() {
            return Ok(());
        }
        let mut hashes = reserve(self.leaves.len())?;
        for (i, preimage) in self.leaves.iter().enumerate() {
            let canonical = space.leaf_at(ctx, count(i)?).ok_or("wide capture extra canonical leaf")?;
            // Decode geometry/width only, without a narrow-dtype interval check.
            // Drop its i128 scratch before the hash writer is built.
            drop(palw_tir_leaf_values_v2(space, ctx, &canonical, preimage).map_err(|e| e.to_string())?);
            hashes.push(step_tile_leaf_hash_v1(&context, &class_id, preimage));
        }
        if step_merkle_root_capped_v1(&hashes, limits.max_step_leaves).map_err(|e| e.to_string())? != expected.step_merkle_root {
            return Err("wide capture dense leaves do not match original step root".into());
        }
        Ok(())
    }
}

// Compare the actual borrowed program to original bytes without encoding it to
// another Vec and without generic deserialization of any delivered program.
fn same_program(program: &misaka_palw_tir::TirProgramV1, bytes: &[u8]) -> Result<bool, String> {
    if borsh::object_length(program).map_err(|e| e.to_string())? != bytes.len() {
        return Ok(false);
    }
    struct Compare<'a>(&'a [u8]);
    impl Write for Compare<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.0.starts_with(bytes) {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "wide program bytes differ"));
            }
            self.0 = &self.0[bytes.len()..];
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut compare = Compare(bytes);
    Ok(program.serialize(&mut compare).is_ok() && compare.0.is_empty())
}
