//! Bound, no-capture TIR inference using A0's fixed r1 field mapping.
//! Registration data and the admission context come from the caller's rooted
//! job state. Parameter authentication below establishes DATA IDENTITY only.
use super::*;
use kaspa_consensus_core::{Hash64, palw_prompt_ids_v1::prompt_token_ids_match_v1};
use kaspa_consensus_core::{
    palw_artifact::{PalwArtifactMerkleFrontierV1, artifact_leaf_parts_v1},
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_step_refute::tiled_logits_scheme_id_v1,
    palw_tir_artifact_v1::{palw_tir_inventory_leaf_count_v1, palw_tir_param_instances_v1, palw_tir_visit_inventory_rows_v1},
    palw_tir_class_v1::PalwTirClassV1,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::{
    public_one_honest::{ManifestEntryV1, ManifestScopeRules, tir_program_material_codec_v1},
    tir::greedy_selector_id,
};

#[derive(Clone, Copy, Debug)]
pub struct ParameterIdentityLimitsV2 {
    pub max_bytes: u64,
    pub max_leaves: u32,
    /// Conservative unrolled operation and state-table entry caps, each.
    pub max_compiled_nodes: u64,
    pub max_parameter_instances: u64,
}

/// Immutable in-memory params authenticated ONCE at process/model loading.
/// No execution trace, full opening tree, or per-claim weight rehash is built.
/// Row-backed mutable/external sources require their own authenticated loader;
/// this initial adapter refuses them rather than trusting a root declaration.
pub struct AuthenticatedTirParamsV2<'a, 'weights> {
    plan: TirPlan,
    params: &'a TirParams<'weights>,
    artifact: Digest,
    bytes_hashed: u64,
}
impl<'a, 'weights> AuthenticatedTirParamsV2<'a, 'weights> {
    pub fn authenticate(
        supplied: &TirPlan,
        params: &'a TirParams<'weights>,
        expected_artifact: Digest,
        limits: ParameterIdentityLimitsV2,
    ) -> Result<Self, MinerErrorV2> {
        // TirPlan's compiled fields are publicly mutable. Never use a caller's
        // operations/index tables as authenticated execution. Recompile the
        // bounded canonical program once, then keep that plan private.
        struct BoundedSize {
            remaining: usize,
        }
        impl std::io::Write for BoundedSize {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.remaining = self
                    .remaining
                    .checked_sub(bytes.len())
                    .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "TIR program exceeds its codec cap"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        // Stop the serialization walk as soon as the byte cap is exceeded;
        // object_length would traverse an entire hostile oversized Vec first.
        borsh::BorshSerialize::serialize(
            &supplied.program,
            &mut BoundedSize { remaining: misaka_palw_tir::program::MAX_PROGRAM_BYTES },
        )
        .map_err(|_| MinerErrorV2::Budget)?;

        let p = &supplied.program;
        let layers = p.schedule.layers.len() as u64;
        let nodes = std::iter::once(p.schedule.pre)
            .chain(p.schedule.layers.iter().copied())
            .chain(std::iter::once(p.schedule.post))
            .try_fold(0u64, |n, block| n.checked_add(p.blocks.get(block as usize)?.nodes.len() as u64))
            .ok_or(MinerErrorV2::Binding)?;
        let state_instances = p
            .states
            .iter()
            .try_fold(0u64, |n, state| n.checked_add(if state.per_layer { layers } else { 1 }))
            .ok_or(MinerErrorV2::Budget)?;
        if nodes > limits.max_compiled_nodes || state_instances > limits.max_compiled_nodes {
            return Err(MinerErrorV2::Budget);
        }
        let mut instance_ceiling = 0u64;
        let mut byte_ceiling = 0u64;
        let mut leaf_ceiling = 0u64;
        for decl in &p.params {
            // A conservative bound before compiling or expanding per-layer maps.
            // Some layer-specific params are read less often; that can only make
            // this operator limit stricter, never create a hidden extra budget.
            let instances = if decl.per_layer { layers } else { 1 };
            instance_ceiling = instance_ceiling.checked_add(instances).ok_or(MinerErrorV2::Budget)?;
            let product = |shape: &[u32]| shape.iter().try_fold(decl.dtype.width() as u64, |n, d| n.checked_mul(u64::from(*d)));
            let bytes = product(&decl.shape).ok_or(MinerErrorV2::Budget)?;
            let row = product(if decl.shape.len() >= 2 { &decl.shape[1..] } else { &decl.shape })
                .filter(|n| *n > 0)
                .ok_or(MinerErrorV2::Binding)?;
            byte_ceiling = bytes.checked_mul(instances).and_then(|n| byte_ceiling.checked_add(n)).ok_or(MinerErrorV2::Budget)?;
            let per_instance = (bytes / row)
                .checked_mul(row.div_ceil(kaspa_consensus_core::palw_tir_artifact_v1::PALW_TIR_ROW_PIECE_BYTES_V1))
                .ok_or(MinerErrorV2::Budget)?;
            leaf_ceiling =
                per_instance.checked_mul(instances).and_then(|n| leaf_ceiling.checked_add(n)).ok_or(MinerErrorV2::Budget)?;
            if instance_ceiling > limits.max_parameter_instances
                || byte_ceiling > limits.max_bytes
                || leaf_ceiling > u64::from(limits.max_leaves)
            {
                return Err(MinerErrorV2::Budget);
            }
        }
        let plan = TirPlan::compile(&supplied.program).map_err(|_| MinerErrorV2::Binding)?;
        if params.row_source().is_some() {
            return Err(ContractError::UnsupportedProfile.into());
        }
        let leaves = palw_tir_inventory_leaf_count_v1(&plan.program).map_err(|_| MinerErrorV2::Binding)?;
        if leaves == 0 || leaves > limits.max_leaves {
            return Err(MinerErrorV2::Budget);
        }
        params.check_complete(&plan).map_err(|_| MinerErrorV2::Binding)?;
        let mut bytes_hashed = 0u64;
        for (j, instances) in palw_tir_param_instances_v1(&plan.program).into_iter().enumerate() {
            let decl = &plan.program.params[j];
            let elements = decl.shape.iter().try_fold(1u64, |n, d| n.checked_mul(u64::from(*d))).ok_or(MinerErrorV2::Budget)?;
            let bytes = elements.checked_mul(decl.dtype.width() as u64).ok_or(MinerErrorV2::Budget)?;
            for layer in instances {
                bytes_hashed = bytes_hashed.checked_add(bytes).filter(|n| *n <= limits.max_bytes).ok_or(MinerErrorV2::Budget)?;
                let held = params.get(j as u16, layer).ok_or(MinerErrorV2::Binding)?;
                if held.dtype() != decl.dtype || held.len() as u64 != elements {
                    return Err(MinerErrorV2::Binding);
                }
            }
        }
        let mut frontier = PalwArtifactMerkleFrontierV1::new();
        palw_tir_visit_inventory_rows_v1(&plan.program, &mut |row| {
            // Metadata/length/type checked above; one canonical <=32KiB piece.
            let decl = &plan.program.params[row.param as usize];
            let held = params.get(row.param, row.layer).expect("authenticated immutable instance");
            let width = decl.dtype.width();
            let bytes = held.sub(row.row_start as usize / width, row.len as usize / width).to_le_bytes();
            frontier.push(artifact_leaf_parts_v1(&decl.name, row.layer, row.row_start, &bytes));
        })
        .map_err(|_| MinerErrorV2::Binding)?;
        if frontier.leaf_count() != u64::from(leaves) || frontier.root().map(|h| Digest(h.as_bytes())) != Some(expected_artifact) {
            return Err(MinerErrorV2::Binding);
        }
        Ok(Self { plan, params, artifact: expected_artifact, bytes_hashed })
    }
    pub fn artifact(&self) -> Digest {
        self.artifact
    }
    /// One-time identity load cost; ordinary inference remains the baseline.
    pub fn bytes_hashed(&self) -> u64 {
        self.bytes_hashed
    }
}

pub struct TirMinerJobV2<'a> {
    pub class: &'a PalwTirClassV1,
    pub context: &'a PalwJobContextV2,
    pub prompt: &'a [u32],
    pub prompt_form: PalwPromptIdsFormV1,
}

impl TirMinerJobV2<'_> {
    /// Describe the same registered program bytes used by the execution.
    pub fn public_program_material(&self, rules: ManifestScopeRules) -> Result<ManifestEntryV1, ContractError> {
        let total_bytes = self.class.program.len() as u64;
        if rules.codec != tir_program_material_codec_v1() {
            return Err(ContractError::UnsupportedProfile);
        }
        if total_bytes == 0
            || total_bytes > u64::from(rules.chunk_bytes)
            || total_bytes > rules.max_bytes
            || total_bytes > misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64
        {
            return Err(ContractError::Budget);
        }
        let subject = Digest(self.class.graph_ir_root().as_bytes());
        Ok(ManifestEntryV1 { subject, content_root: subject, codec: rules.codec, total_bytes, chunk_bytes: rules.chunk_bytes })
    }
}

/// The concrete A2 inference→result path. Every job/class/input/selector field
/// is checked before execution, and the output field is derived from the run.
/// It signs only the Miner result; Panel witness generation is never called.
#[allow(clippy::too_many_arguments)]
pub fn infer_and_sign_tir_result_v2(
    admission: &AdmissionContext,
    mut result: MinerResultCommit,
    contract: &MinerServiceContract,
    held: &AuthenticatedTirParamsV2<'_, '_>,
    job: TirMinerJobV2<'_>,
    limits: InferenceLimitsV2,
    now: u64,
    key: &[u8; PUBKEY_BYTES],
    sign: impl FnOnce(&[u8], &[u8]) -> Result<[u8; SIGNATURE_BYTES], String>,
) -> Result<(MinerResultCommit, [u8; SIGNATURE_BYTES], Vec<u32>), MinerErrorV2> {
    admission.check(ClaimForm::PositiveResultV2, &result)?;
    contract.validate(&result, now)?;
    if misaka_palw_positive_contract::identity::principal(key) != result.miner {
        return Err(ContractError::Identity.into());
    }
    let ctx = job.context;
    let class = job.class;
    let program = &held.plan.program;
    let positions =
        result.prefill.checked_add(result.decode.checked_sub(1).ok_or(MinerErrorV2::Binding)?).ok_or(MinerErrorV2::Budget)?;
    if result.prefill == 0
        || result.prefill > limits.max_prefill
        || result.decode > limits.max_decode
        || positions > limits.max_positions
        || positions > class.layout.max_context
        || positions >= ctx.max_context_tokens
        || ctx.network_id.is_empty()
        || ctx.network_id.len() > kaspa_consensus_core::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES
    {
        return Err(MinerErrorV2::Budget);
    }
    let expected_program = program.encode();
    let tiles = program.blocks.iter().flat_map(|b| &b.nodes).filter(|node| node.commit).count();
    if class.program.len() != expected_program.len()
        || class.program != expected_program
        || class.layout.commit_tiles.len() != tiles
        || class.layout.state_tiles.len() != program.states.len()
    {
        return Err(MinerErrorV2::Binding);
    }
    // The registered tokenizer is bound by the class ID. The Result names its
    // job's tokenizer instead: existing canonical attempt contexts use zero.
    // Actual native origin/assignment is still the admission owner's duty.
    if ctx.version != 2
        || held.artifact != result.artifact
        || class.class_id(&Hash64::from_bytes(held.artifact.0)).as_bytes() != result.registered_class.0
        || class.graph_ir_root().as_bytes() != result.program.0
        || result.tokenizer.0 != ctx.tokenizer_id.as_bytes()
        || result.job.0 != ctx.job_id.as_bytes()
        || result.execution.0 != ctx.context_hash().as_bytes()
        || result.input.0 != ctx.prompt_token_ids_hash.as_bytes()
        || result.registered_class.0 != ctx.shape_profile_id.as_bytes()
        || result.execution_rules.0 != ctx.cu_ruleset_id.as_bytes()
        || result.generation_seed != ctx.execution_seed
        || result.prefill != ctx.declared_prefill_tokens
        || result.decode != ctx.exact_decode_tokens
        || result.max_context != ctx.max_context_tokens
        || job.prompt.len() != result.prefill as usize
        || !prompt_token_ids_match_v1(job.prompt_form, job.prompt, &ctx.prompt_token_ids_hash)
    {
        return Err(MinerErrorV2::Binding);
    }
    // Use A0's one selector/profile, without inventing another domain.
    if result.selector != greedy_selector_id()
        || result.temperature_q24 != 0
        || Hash64::from_bytes(program.logits_scheme_id) != tiled_logits_scheme_id_v1()
        || ctx.trace_scheme_id != tiled_logits_scheme_id_v1()
        || program.blocks.iter().any(|block| block.nodes.iter().any(|node| !node.commit))
        || program.blocks[program.schedule.post as usize].nodes[program.logits as usize].out.dtype != misaka_palw_tir::DType::I32
    {
        return Err(ContractError::UnsupportedProfile.into());
    }
    let generated = infer_tokens_v2(&held.plan, held.params, job.prompt, result.decode, limits)?;
    result.output = Digest(palw_attempt_output_root_v1(ctx, &generated).as_bytes());
    let (result, signature) = sign_result_v2(admission, result, contract, now, key, sign)?;
    Ok((result, signature, generated))
}

#[cfg(test)]
#[path = "positive_miner_v2_inference_tests.rs"]
mod tests;
