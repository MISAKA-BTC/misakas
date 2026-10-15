//! Prospective salted-claim replay admission. Quotes are derived from the actual
//! selected ledger rows and borrowed commit, never a wire-supplied byte estimate.
use super::*;
use misaka_palw_kernel::{
    hash::Digest,
    ledger::{ClaimRowV1, ClassRowV1, KernelLedgerV1, LedgerEventV1},
    route::{AuthV1, KernelRouteObjectV1, SaltedCommitV1},
};

struct ReplayRequestsV4 {
    typed: TirNativeRequestsV1,
    codec_peak: u64,
}
impl ReplayRequestsV4 {
    fn new() -> Self {
        Self { typed: TirNativeRequestsV1::new(), codec_peak: 0 }
    }
    fn copy_v4<T: NativeHeapV1 + ?Sized>(&mut self, actual: &T, purpose: &str) -> Result<(), NativeCoverageV1> {
        let bound = actual.native_heap_v1(purpose);
        self.typed.add_v1(bound.bytes().ok_or(bound)?)
    }
    fn codec_v4<T: borsh::BorshSerialize + ?Sized>(&mut self, actual: &T) -> Result<(), NativeCoverageV1> {
        let n = borsh::object_length(actual)
            .map_err(|e| NativeCoverageV1::unsupported::<T>("salted replay canonical length", &e.to_string()))? as u64;
        self.codec_peak = self.codec_peak.max(n);
        Ok(())
    }
    fn buffer_v4(&mut self, canonical: u64) -> Result<(), NativeCoverageV1> {
        let capacity =
            canonical.max(1024).checked_mul(2).ok_or_else(|| NativeCoverageV1::overflow::<Self>("Borsh serializer capacity"))?;
        self.typed.exact_v1::<u8>(capacity)
    }
    fn finish_v4(mut self) -> Result<u64, NativeCoverageV1> {
        // An object_id's serialization can coexist with the public-record byte
        // argument. The latter is charged separately at its concrete call site.
        self.buffer_v4(self.codec_peak)?;
        Ok(self.typed.bytes_v1())
    }
}
fn unsupported_v4(why: &str) -> NativeCoverageV1 {
    NativeCoverageV1::unsupported::<SaltedCommitV1>("prospective salted replay", why)
}
fn plus_v4(a: u64, b: u64) -> Result<u64, NativeCoverageV1> {
    a.checked_add(b).ok_or_else(|| NativeCoverageV1::overflow::<SaltedCommitV1>("prospective structural sum"))
}
fn length_v4<T: borsh::BorshSerialize + ?Sized>(value: &T) -> Result<u64, NativeCoverageV1> {
    borsh::object_length(value)
        .map(|n| n as u64)
        .map_err(|e| NativeCoverageV1::unsupported::<T>("actual canonical representation", &e.to_string()))
}

/// Exact value count from authenticated dimensions. Tensor commitments allocate
/// i128 lanes, one row/column projection and the two Merkle levels. No parameter
/// artifact or speculative full execution is loaded by this quote.
fn tensor_commitment_v4(q: &mut ReplayRequestsV4, shape: &[u32]) -> Result<(), NativeCoverageV1> {
    let count = shape
        .iter()
        .try_fold(1u64, |n, d| n.checked_mul(u64::from(*d)))
        .ok_or_else(|| NativeCoverageV1::overflow::<misaka_palw_tir::Tensor>("actual state tensor dimensions"))?;
    let (rows, row_len, cols, col_len) = match shape {
        [] | [_] => (1, count, 1, count),
        dims => {
            let n = u64::from(dims[dims.len() - 1]);
            let m = u64::from(dims[dims.len() - 2]);
            if n == 0 || m == 0 { (0, n, 0, m) } else { (count / n, n, count / m, m) }
        }
    };
    q.typed.exact_v1::<i128>(count)?; // Tensor::zeros / TensorWire decode
    q.typed.vec_v1::<usize>(shape.len() as u64)?; // Source::Zeros shape
    q.typed.vec_v1::<usize>(shape.len() as u64)?; // Tensor::zeros owned shape
    q.typed.vec_v1::<u64>(shape.len() as u64)?; // Layout::try_of
    q.typed.vec_v1::<i128>(row_len)?;
    q.typed.vec_v1::<i128>(col_len)?;
    q.typed.vec_v1::<Digest>(rows)?;
    q.typed.vec_v1::<Digest>(rows.div_ceil(2))?;
    q.typed.vec_v1::<Digest>(cols)?;
    q.typed.vec_v1::<Digest>(cols.div_ceil(2))?;
    Ok(())
}
fn program_boundaries_v4(
    q: &mut ReplayRequestsV4,
    program: &misaka_palw_tir::TirProgramV1,
    tokens: u64,
) -> Result<(), NativeCoverageV1> {
    boundary_states_v4(q, &program.states, tokens)
}
fn boundary_states_v4(
    q: &mut ReplayRequestsV4,
    states: &[misaka_palw_tir::program::StateDecl],
    tokens: u64,
) -> Result<(), NativeCoverageV1> {
    // claim_structure recomputes initial Fixed-state zero commitments and the
    // entering history's public/zero sources. Actual declared row dimensions
    // are distinct from the codec's small shape-vector allocation.
    let max_rank = states.iter().map(|s| s.shape.len()).max().unwrap_or(0) as u64;
    for state in states {
        tensor_commitment_v4(q, &state.shape)?;
        if let misaka_palw_tir::program::StateKind::Hist { window } = state.kind {
            let rows = tokens.min(u64::from(window).saturating_sub(1));
            q.typed.vec_v1::<misaka_palw_kernel::trace::SourceV1>(rows)?;
            q.typed.vec_v1::<usize>(TirNativeRequestsV1::product_v1(rows, max_rank)?)?;
            q.typed.exact_v1::<usize>(TirNativeRequestsV1::product_v1(rows, 8)?)?;
        }
    }
    // A public scalar source's commitment has rank zero and one lane.
    tensor_commitment_v4(q, &[])
}
fn model_program_v4(q: &mut ReplayRequestsV4, class: &ClassRowV1, tokens: u64) -> Result<(), NativeCoverageV1> {
    let requests = super::ledger_expansion::standalone_tir_program_requests_v1(&class.program_bytes)
        .map_err(|e| unsupported_v4(&format!("selected registered program shape: {e}")))?;
    q.typed.add_v1(requests)?;
    program_boundaries_v4(q, &class.program, tokens)?;
    q.codec_v4(&class.descriptor)?;
    q.codec_v4(&class.plan)?;
    Ok(())
}
fn record_param_bytes_v4(class: &ClassRowV1) -> Result<u64, NativeCoverageV1> {
    class
        .param_commitments
        .by_instance
        .keys()
        .try_fold(4u64, |n, (_, layer)| plus_v4(n, 2 + 1 + if layer.is_some() { 2 } else { 0 } + 64))
}
fn model_public_record_v4(
    q: &mut ReplayRequestsV4,
    class: &ClassRowV1,
    evidence: &misaka_palw_kernel::evidence::VerificationEvidenceV1,
    commitments: &Vec<Vec<Vec<Digest>>>,
    tokens: u64,
) -> Result<(), NativeCoverageV1> {
    model_program_v4(q, class, tokens)?;
    // Concrete separately owned lifetimes in commit_claim: assembled record,
    // FreshVerifier's decoded record, its trace cache and final ClaimBody.
    q.copy_v4(class, "assembled public record class fields")?;
    q.copy_v4(class, "FreshVerifier public record class fields")?;
    q.copy_v4(evidence, "assembled public record evidence")?;
    q.copy_v4(evidence, "FreshVerifier public record evidence")?;
    q.copy_v4(commitments, "EvidenceV1 commitment-root temporary")?;
    q.copy_v4(commitments, "assembled public record commitments")?;
    q.copy_v4(commitments, "FreshVerifier public record commitments")?;
    q.copy_v4(commitments, "FreshVerifier trace commitments")?;
    q.copy_v4(commitments, "retained ClaimBody commitments")?;
    q.copy_v4(evidence, "retained ClaimBody evidence")?;
    q.copy_v4(&class.descriptor, "FreshVerifier descriptor")?;
    q.copy_v4(&class.param_commitments, "FreshVerifier parameter BTree reconstruction")?;
    q.typed.vec_v1::<u32>(tokens)?; // binding_fault stream
    q.typed.vec_v1::<u32>(tokens)?; // assembled record stream
    q.typed.vec_v1::<u32>(tokens)?; // decoded record stream
    q.codec_v4(evidence)?;
    // Borsh layout of PublicClaimRecordV1, counted without constructing its
    // copied vectors. Its param map projects to a tuple Vec in exact order.
    let mut encoded = 64u64;
    for n in [
        length_v4(&class.program_bytes)?,
        length_v4(&class.plan)?,
        length_v4(evidence)?,
        length_v4(commitments)?,
        record_param_bytes_v4(class)?,
    ] {
        encoded = plus_v4(encoded, n)?;
    }
    encoded = plus_v4(encoded, plus_v4(4, TirNativeRequestsV1::product_v1(tokens, 4)?)?)?;
    encoded = plus_v4(encoded, 64)?;
    q.buffer_v4(encoded)?;
    Ok(())
}
fn ordinary_claim_v4(
    q: &mut ReplayRequestsV4,
    ledger: &KernelLedgerV1,
    claim: &misaka_palw_kernel::job::KernelClaimV1,
    evidence: &misaka_palw_kernel::evidence::VerificationEvidenceV1,
    commitments: &Vec<Vec<Vec<Digest>>>,
) -> Result<Digest, NativeCoverageV1> {
    let job = ledger.jobs.get(&claim.job_id).ok_or_else(|| unsupported_v4("selected ordinary job is absent"))?;
    let class = ledger.classes.get(&job.class_binding_id).ok_or_else(|| unsupported_v4("selected ordinary class is absent"))?;
    let tokens = plus_v4(job.prompt.len() as u64, claim.generated.len().saturating_sub(1) as u64)?;
    model_public_record_v4(q, class, evidence, commitments, tokens)?;
    q.copy_v4(claim, "retained ClaimBody delivered tokens")?;
    q.codec_v4(job)?;
    q.codec_v4(claim)?;
    Ok(job.class_binding_id)
}
fn segmented_claim_v4(
    q: &mut ReplayRequestsV4,
    ledger: &KernelLedgerV1,
    claim: &misaka_palw_kernel::job::KernelClaimV1,
    evidence: &misaka_palw_kernel::seg::SegmentedEvidenceV2,
    roots: &Vec<Digest>,
) -> Result<Digest, NativeCoverageV1> {
    let class_id = if let Some(job) = ledger.jobs.get(&claim.job_id) {
        q.copy_v4(job, "segmented inline JobView prompt")?;
        q.codec_v4(job)?;
        let prompt_tiles = (job.prompt.len() as u64).div_ceil(misaka_palw_kernel::seg::PROMPT_TILE_IDS_V1 as u64);
        q.typed.vec_v1::<Digest>(prompt_tiles)?;
        q.typed.vec_v1::<Digest>(prompt_tiles.div_ceil(2))?;
        job.class_binding_id
    } else {
        ledger
            .tiled_jobs
            .get(&claim.job_id)
            .ok_or_else(|| unsupported_v4("selected segmented/tiled job is absent"))?
            .job
            .class_binding_id
    };
    let class = ledger.classes.get(&class_id).ok_or_else(|| unsupported_v4("selected segmented class is absent"))?;
    q.copy_v4(claim, "retained segmented ClaimBody tokens")?;
    q.copy_v4(evidence, "retained segmented ClaimBody evidence")?;
    q.copy_v4(roots, "retained segmented ClaimBody roots")?;
    // claim_root owns the initial root Vec and its replacement level.
    q.typed.vec_v1::<Digest>(roots.len() as u64)?;
    q.typed.vec_v1::<Digest>((roots.len() as u64).div_ceil(2))?;
    q.codec_v4(claim)?;
    q.codec_v4(evidence)?;
    q.codec_v4(&class.descriptor)?;
    q.codec_v4(&class.plan)?;
    Ok(class_id)
}
fn pipeline_claim_v4(
    q: &mut ReplayRequestsV4,
    ledger: &KernelLedgerV1,
    claim: &misaka_palw_kernel::pipeline_public::PipelineClaimV1,
    evidence: &misaka_palw_kernel::pipeline::PipelineEvidenceV1,
    stages: &Vec<misaka_palw_kernel::pipeline_public::StageCommitmentsV1>,
) -> Result<Digest, NativeCoverageV1> {
    use misaka_palw_tir::{pipeline::StageJobFacts, program_v2::TirProgramV2};
    let job = ledger.pipeline_jobs.get(&claim.job_id).ok_or_else(|| unsupported_v4("selected pipeline job is absent"))?;
    let class =
        ledger.pipeline_classes.get(&job.class_binding_id).ok_or_else(|| unsupported_v4("selected pipeline class is absent"))?;
    q.copy_v4(class, "assembled pipeline public-record fields")?;
    q.copy_v4(class, "FreshPipelineVerifier decoded public-record fields")?;
    q.copy_v4(job, "assembled pipeline public-record facts")?;
    q.copy_v4(job, "decoded pipeline public-record facts")?;
    q.copy_v4(job, "FreshPipelineVerifier executable job facts")?;
    q.copy_v4(job, "binding-check executable job facts")?;
    q.copy_v4(claim, "retained pipeline ClaimBody tokens")?;
    q.copy_v4(evidence, "assembled pipeline evidence")?;
    q.copy_v4(evidence, "decoded pipeline evidence")?;
    q.copy_v4(evidence, "retained pipeline ClaimBody evidence")?;
    q.copy_v4(stages, "assembled pipeline public stages")?;
    q.copy_v4(stages, "decoded pipeline public stages")?;
    q.copy_v4(stages, "FreshPipelineVerifier stage traces")?;
    q.copy_v4(stages, "retained pipeline ClaimBody stages")?;
    q.copy_v4(&class.param_commitments, "FreshPipelineVerifier parameter maps")?;
    q.typed.exact_v1::<typed::BTreeNodeEnvelopeV1<(u8, u8), Vec<u32>>>(job.facts.finalized.len() as u64)?;
    q.typed.exact_v1::<StageJobFacts>(class.pipeline.stages.len() as u64)?;
    q.typed.vec_v1::<Digest>(class.programs.len() as u64)?;
    for bytes in &class.program_bytes {
        let requests = super::ledger_expansion::standalone_program_requests_v1::<TirProgramV2>(
            bytes,
            misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64,
        )
        .map_err(|e| unsupported_v4(&format!("selected pipeline program shape: {e}")))?;
        q.typed.add_v1(requests)?;
    }
    for stage in &class.pipeline.stages {
        let program =
            class.programs.get(stage.program as usize).ok_or_else(|| unsupported_v4("selected pipeline stage program is absent"))?;
        q.copy_v4(program, "pipeline stage's lifted V1 view")?;
        q.typed.vec_v1::<misaka_palw_tir::program::ParamDecl>(plus_v4(program.params.len() as u64, program.inputs.len() as u64)?)?;
        q.copy_v4(&program.inputs, "lifted stage input parameter names and shapes")?;
        let trip = u64::from(stage.max_trip);
        q.typed.vec_v1::<u32>(trip)?; // StageJobFacts.tokens
        q.typed.vec_v1::<u32>(trip)?; // run_tokens clone/zero run
        q.typed.vec_v1::<u32>(trip)?; // apply_rule temporary
        boundary_states_v4(q, &program.states, trip)?;
        for input in &program.inputs {
            tensor_commitment_v4(q, &input.shape)?;
            let elements = input
                .shape
                .iter()
                .try_fold(1u64, |n, d| n.checked_mul(u64::from(*d)))
                .ok_or_else(|| unsupported_v4("pipeline input dimensions overflow"))?;
            q.typed.vec_v1::<u32>(elements)?; // apply_rule sequence before i128 conversion
            q.typed.exact_v1::<typed::BTreeNodeEnvelopeV1<u16, misaka_palw_tir::Tensor>>(1)?;
        }
    }
    // The canonical pipeline public-record frame has the same field order as
    // its derive. Map->tuple projections use exact per-entry optional-layer tags.
    let mut params = 4u64;
    for pc in &class.param_commitments {
        let inner =
            pc.by_instance.keys().try_fold(4u64, |n, (_, layer)| plus_v4(n, 2 + 1 + if layer.is_some() { 2 } else { 0 } + 64))?;
        params = plus_v4(params, inner)?;
    }
    let mut encoded = 64u64;
    for n in [
        length_v4(&class.pipeline_bytes)?,
        length_v4(&class.program_bytes)?,
        length_v4(&class.plan)?,
        length_v4(evidence)?,
        length_v4(stages)?,
        params,
        length_v4(&job.facts)?,
        length_v4(&claim.generated)?,
        64,
    ] {
        encoded = plus_v4(encoded, n)?;
    }
    q.buffer_v4(encoded)?;
    q.codec_v4(job)?;
    q.codec_v4(claim)?;
    q.codec_v4(evidence)?;
    q.codec_v4(&class.plan)?;
    q.codec_v4(&class.binding)?;
    q.codec_v4(&class.descriptor)?;
    q.buffer_v4(length_v4(&class.pipeline_bytes)?)?; // pipeline_root encode
    Ok(job.class_binding_id)
}
fn spec_claim_v4(
    q: &mut ReplayRequestsV4,
    ledger: &KernelLedgerV1,
    claim: &misaka_palw_kernel::spec::SpecClaimV1,
) -> Result<Digest, NativeCoverageV1> {
    use misaka_palw_kernel::spec::{ComponentV1, SpecClaimV1, SpecClassKindV1, SpecJobV1, StageClaimV1, StageInputV1, TokenSourceV1};
    let job = ledger.typed.jobs.get(&claim.job_id()).ok_or_else(|| unsupported_v4("selected typed job is absent"))?;
    let class_id = job.class();
    let class = ledger.typed.classes.get(&class_id).ok_or_else(|| unsupported_v4("selected typed class is absent"))?;
    q.copy_v4(claim, "retained typed ClaimBody payload")?;
    q.typed.exact_v1::<misaka_palw_kernel::spec::SpecClaimBodyV1>(1)?;
    q.codec_v4(claim)?;
    q.codec_v4(job)?;
    q.codec_v4(&class.spec)?;
    match (claim, job, &class.kind) {
        (SpecClaimV1::Memory(c), SpecJobV1::Memory(j), SpecClassKindV1::Memory { rule, root, .. }) => {
            let line = ledger.typed.lines.get(&class_id).ok_or_else(|| unsupported_v4("selected typed memory line is absent"))?;
            let boundaries = plus_v4(c.steps.len() as u64, 1)?;
            let slots = line.head.len() as u64;
            // The two boundary derivations plus the retained pre-state are
            // separate actual allocations, with an inner Vec per boundary.
            for _ in 0..2 {
                q.typed.vec_v1::<Vec<Digest>>(boundaries)?;
                q.typed.vec_v1::<Digest>(TirNativeRequestsV1::product_v1(boundaries, slots)?)?;
                q.typed.exact_v1::<Digest>(TirNativeRequestsV1::product_v1(boundaries, 8)?)?;
            }
            q.typed.vec_v1::<Vec<Digest>>(1)?;
            q.copy_v4(&line.head, "retained typed memory pre-state")?;
            for tensor in &c.post_state {
                // TensorWire::decode materializes i128 lanes. Derive their count
                // from the actual declared shape, never raw bytes times a ratio.
                tensor_wire_commitment_v4(q, tensor)?;
            }
            for (step, chunk) in c.steps.iter().zip(&j.chunks) {
                q.copy_v4(&rule.param_commitments, "step overlay parameter map")?;
                for slot in &root.slots {
                    if !rule.param_commitments.by_instance.contains_key(&slot.param) {
                        q.typed.exact_v1::<typed::BTreeNodeEnvelopeV1<(u16, Option<u16>), Digest>>(1)?;
                    }
                }
                q.typed.vec_v1::<u32>(chunk.len() as u64)?; // step_job prompt
                q.typed.vec_v1::<u32>(1)?; // step_claim generated id
                model_public_record_v4(q, rule, &step.evidence, &step.commitments, chunk.len() as u64)?;
            }
        }
        (SpecClaimV1::Retrieval(_), SpecJobV1::Retrieval(_), SpecClassKindV1::Retrieval { .. }) => {}
        (SpecClaimV1::Composite(c), SpecJobV1::Composite(j), SpecClassKindV1::Composite { root, components }) => {
            for ((stage, carried), component) in root.stages.iter().zip(&c.stages).zip(components) {
                let (
                    StageInputV1::Tokens { sources, .. },
                    StageClaimV1::Model { generated, evidence, commitments },
                    ComponentV1::Model(model),
                ) = (&stage.input, carried, component)
                else {
                    continue;
                };
                let prompt = sources.iter().try_fold(0u64, |n, source| {
                    let count = match source {
                        TokenSourceV1::JobPrompt => j.prompt.len() as u64,
                        TokenSourceV1::StageGenerated { stage } => match c.stages.get(*stage as usize) {
                            Some(StageClaimV1::Model { generated, .. }) => generated.len() as u64,
                            _ => return Err(unsupported_v4("composite generated-token source kind is invalid")),
                        },
                        TokenSourceV1::StagePayloads { stage } => match c.stages.get(*stage as usize) {
                            Some(StageClaimV1::Retrieval { payloads, .. }) => {
                                payloads.iter().try_fold(0u64, |n, p| plus_v4(n, p.len() as u64))?
                            }
                            _ => return Err(unsupported_v4("composite payload source kind is invalid")),
                        },
                    };
                    plus_v4(n, count)
                })?;
                let tokens = plus_v4(prompt, generated.len().saturating_sub(1) as u64)?;
                q.typed.vec_v1::<u32>(prompt)?; // stage_prompt/sub_job
                q.copy_v4(generated, "composite sub-claim delivered tokens")?;
                model_public_record_v4(q, model, evidence, commitments, tokens)?;
            }
        }
        // The existing kernel refuses mismatched claim/job/class kinds before
        // those allocations. No speculative row or approval is synthesized here.
        _ => {}
    }
    Ok(class_id)
}
fn tensor_wire_commitment_v4(
    q: &mut ReplayRequestsV4,
    tensor: &misaka_palw_kernel::public::TensorWireV1,
) -> Result<(), NativeCoverageV1> {
    let dtype =
        misaka_palw_kernel::plan::dtype_of_tag(tensor.dtype).ok_or_else(|| unsupported_v4("typed post-state dtype is unknown"))?;
    let count = tensor
        .shape
        .iter()
        .try_fold(1u64, |n, d| n.checked_mul(*d))
        .ok_or_else(|| unsupported_v4("typed post-state dimensions overflow"))?;
    let bytes =
        count.checked_mul(dtype.width() as u64).ok_or_else(|| unsupported_v4("typed post-state encoded dimensions overflow"))?;
    if bytes != tensor.bytes.len() as u64 {
        return Err(unsupported_v4("typed post-state dimensions and actual material bytes disagree"));
    }
    // Shape dimensions are already carried as u64. Avoid an unreserved
    // temporary Vec<u32> merely to invoke the ordinary state helper.
    let (rows, row_len, cols, col_len) = match tensor.shape.as_slice() {
        [] | [_] => (1, count, 1, count),
        dims => {
            let n = dims[dims.len() - 1];
            let m = dims[dims.len() - 2];
            if n == 0 || m == 0 { (0, n, 0, m) } else { (count / n, n, count / m, m) }
        }
    };
    q.typed.exact_v1::<i128>(count)?;
    q.typed.vec_v1::<usize>(tensor.shape.len() as u64)?;
    q.typed.vec_v1::<usize>(tensor.shape.len() as u64)?; // declared-param shape comparison
    q.typed.vec_v1::<u64>(tensor.shape.len() as u64)?;
    q.typed.vec_v1::<i128>(row_len)?;
    q.typed.vec_v1::<i128>(col_len)?;
    q.typed.vec_v1::<Digest>(rows)?;
    q.typed.vec_v1::<Digest>(rows.div_ceil(2))?;
    q.typed.vec_v1::<Digest>(cols)?;
    q.typed.vec_v1::<Digest>(cols.div_ceil(2))?;
    Ok(())
}
fn replay_requests_v4(ledger: &KernelLedgerV1, commit: &SaltedCommitV1) -> Result<u64, NativeCoverageV1> {
    let mut q = ReplayRequestsV4::new();
    q.copy_v4(commit, "borrowed commit cloned into the authenticated route object")?;
    q.typed.exact_v1::<KernelRouteObjectV1>(1)?;
    let class = match commit {
        SaltedCommitV1::Claim { claim, evidence, commitments } => ordinary_claim_v4(&mut q, ledger, claim, evidence, commitments)?,
        SaltedCommitV1::Segmented { claim, evidence, segment_roots } => {
            segmented_claim_v4(&mut q, ledger, claim, evidence, segment_roots)?
        }
        SaltedCommitV1::Pipeline { claim, evidence, stages } => pipeline_claim_v4(&mut q, ledger, claim, evidence, stages)?,
        SaltedCommitV1::Spec { claim } => spec_claim_v4(&mut q, ledger, claim)?,
    };
    // admit writes one claim and one salt; OPV additionally owns three fixed
    // memberships. Source-reveal/source-deposit bookkeeping only edits existing
    // fixed fields or removes rows, and never creates a new source entry.
    q.typed.exact_v1::<typed::BTreeNodeEnvelopeV1<Digest, ClaimRowV1>>(1)?;
    q.typed.exact_v1::<typed::BTreeNodeEnvelopeV1<Digest, Digest>>(1)?;
    if ledger.opv.classes.contains(&class) {
        q.typed.exact_v1::<typed::BTreeNodeEnvelopeV1<Digest, misaka_palw_kernel::opv::OpvClaimRowV1>>(1)?;
        q.typed.exact_v1::<typed::BTreeNodeEnvelopeV1<(Digest, Digest), ()>>(1)?;
        q.typed.exact_v1::<typed::BTreeNodeEnvelopeV1<Digest, Digest>>(1)?;
    }
    // Four admit settlements, at most three source-deposit settlements, then
    // ClaimCommitted. Events are discarded before this API returns.
    q.typed.vec_v1::<LedgerEventV1>(8)?;
    q.codec_v4(commit)?;
    q.finish_v4()
}
impl ReservedKernelLedgerV1 {
    /// Compute the actual admitted row's subject with a distinct reservation for
    /// canonical hash buffers and the composite's component-ID projection. The
    /// returned subject has no heap; the candidate still owns its growth ticket.
    pub fn try_claim_challenge_subject_v4(
        &mut self,
        claim: &Digest,
    ) -> Result<Option<misaka_palw_challenge::ChallengeSubjectV1>, NativeCoverageV1> {
        use misaka_palw_kernel::{ledger::ClaimBodyV1, spec::SpecClassKindV1};
        if self.lease.is_some() || native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1() {
            let Some(row) = self.ledger.claims.get(claim) else { return Ok(None) };
            let mut q = ReplayRequestsV4::new();
            q.codec_v4(&row.body)?;
            // Fixed state/input/claim commitment tuples have at most two roots.
            q.codec_v4(&(*claim, *claim))?;
            match &row.body {
                ClaimBodyV1::Program { .. } | ClaimBodyV1::Segmented { .. } => {
                    let Some(class) = self.ledger.classes.get(&row.class_binding_id) else { return Ok(None) };
                    q.codec_v4(&class.descriptor)?;
                    q.codec_v4(&class.plan)?;
                }
                ClaimBodyV1::Pipeline { .. } => {
                    let Some(class) = self.ledger.pipeline_classes.get(&row.class_binding_id) else { return Ok(None) };
                    q.codec_v4(&class.descriptor)?;
                    q.codec_v4(&class.plan)?;
                    q.codec_v4(&class.binding.artifact_roots)?;
                }
                ClaimBodyV1::Spec(_) => {
                    let Some(class) = self.ledger.typed.classes.get(&row.class_binding_id) else { return Ok(None) };
                    q.codec_v4(&class.spec)?;
                    if let SpecClassKindV1::Composite { root, .. } = &class.kind {
                        q.typed.vec_v1::<Digest>(root.stages.len() as u64)?;
                    }
                    // k2_tr_v1_descriptor constructs the compiled v2 family
                    // vector, then drops it. Count its actual fixed inventory.
                    q.typed.exact_v1::<misaka_palw_kernel::descriptor::FamilySupportV1>(
                        misaka_palw_kernel::family::ConstraintFamilyV1::ALL.len() as u64,
                    )?;
                }
            }
            let requests = q.finish_v4()?;
            if let Some(lease) = &mut self.lease {
                lease.grow_memory_v1(requests)?;
            } else {
                self.lease = reserve_native_requests_v1(requests)?;
            }
        }
        Ok(self.ledger.claim_challenge_subject(claim))
    }

    /// Audited prospective replay only. No settlement is applied externally.
    /// Capacity admission precedes begin_block, route cloning and all mutation;
    /// the growth lease remains part of this owned candidate ledger.
    pub fn try_replay_salted_claim_v4(
        &mut self,
        commit: &SaltedCommitV1,
        salt: &Digest,
        producer: Digest,
        commit_daa: u64,
    ) -> Result<(), String> {
        if self.lease.is_some() || native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1() {
            let requests = replay_requests_v4(&self.ledger, commit).map_err(|e| format!("local native capacity: {e}"))?;
            if let Some(lease) = &mut self.lease {
                lease.grow_memory_v1(requests).map_err(|e| format!("local native capacity: {e}"))?;
            } else {
                self.lease = reserve_native_requests_v1(requests).map_err(|e| format!("local native capacity: {e}"))?;
            }
        }
        self.ledger.begin_block(commit_daa).map_err(|e| e.to_string())?;
        let object = KernelRouteObjectV1::CommitClaimSalted { salt: *salt, commit: commit.clone() };
        let events = self.ledger.apply_object(&object, &AuthV1 { signer_bond: producer }).map_err(|e| e.to_string())?;
        drop(events);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_dimensions_have_a_bound_distinct_from_codec_shape_length() {
        let mut tiny = ReplayRequestsV4::new();
        tensor_commitment_v4(&mut tiny, &[1, 1]).unwrap();
        let mut large = ReplayRequestsV4::new();
        tensor_commitment_v4(&mut large, &[1024, 1024]).unwrap();
        // Both are two u32 dimensions on the wire; their native i128 lane and
        // Merkle workspace sizes must still differ by the actual tensor shape.
        assert!(large.finish_v4().unwrap() > tiny.finish_v4().unwrap());
        let mut overflow = ReplayRequestsV4::new();
        assert!(tensor_commitment_v4(&mut overflow, &[u32::MAX; 4]).is_err());
    }
    #[test]
    fn absent_authoritative_selected_job_refuses_before_advancing_candidate_clock() {
        use misaka_palw_kernel::{
            evidence::{EvidenceHeaderV1, SuiteParamsV1},
            job::KernelClaimV1,
            seg::SegmentedEvidenceV2,
        };
        let policy =
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(crate::Hash64::from_u64_word(1), crate::Hash64::from_u64_word(2));
        let route = crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(policy, None, Default::default());
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 });
        let mut candidate =
            ReservedKernelLedgerV1::from_preflight_v1(route.template(), Some(checker.reserve_requests_v1(0, 0).unwrap()));
        let before = candidate.daa;
        let evidence = SegmentedEvidenceV2 {
            version: 2,
            header: EvidenceHeaderV1 {
                network_domain: [1; 64],
                ruleset_digest: [2; 64],
                class_binding_id: [3; 64],
                program_root: [4; 64],
                artifact_root: [5; 64],
                plan_root: [6; 64],
            },
            job_input_root: [7; 64],
            positions: 1,
            segment_len: 1,
            claim_root: [8; 64],
            suite: SuiteParamsV1 {
                descriptor_digest: [9; 64],
                checker_suite_id: 0,
                challenge_policy_id: 0,
                repetitions: 1,
                target_bits: 1,
                field_bits: 1,
            },
        };
        let commit = SaltedCommitV1::Segmented {
            claim: KernelClaimV1 { job_id: [10; 64], producer_bond: [11; 64], generated: vec![0], evidence_root: [12; 64] },
            evidence,
            segment_roots: vec![[13; 64]],
        };
        assert!(candidate.try_replay_salted_claim_v4(&commit, &[14; 64], [11; 64], before + 1).is_err());
        assert_eq!(candidate.daa, before);
        assert!(candidate.claims.is_empty());
        assert_eq!(checker.outstanding(), (0, 0));
    }
}
