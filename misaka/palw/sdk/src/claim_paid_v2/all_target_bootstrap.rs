//! Real private producers for every native paid-claim target. No receipt, source
//! income or conformance result is synthesized by these offline helpers.
use super::private_bootstrap::ProgramPaidBootstrapWorkV3;
use super::producer_resources::{PrivateProgramResourceBoundV3, bound_program_work_v3};
use super::*;
use misaka_palw_kernel::spec::composite::{ComponentV1, CompositeClaimV1, QuerySourceV1, StageInputV1, stage_job_v1, stage_prompt_v1};
use misaka_palw_kernel::spec::retrieval::{RetrievalClaimV1, RetrievalItemV1, RetrievalRootV1, SnapshotDataV1};
use misaka_palw_kernel::spec::{SpecClaimV1, SpecClassKindV1, SpecJobV1};
use misaka_palw_kernel::{
    job::{DecodeRuleV1, KernelClaimV1, KernelJobV1},
    ledger::ClassRowV1,
    public::TensorWireV1,
    route::SaltedCommitV1,
    trace::{ParamCommitmentsV1, TraceV1},
};
use misaka_palw_tir::MapParams;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct NativePaidProducerInputsV3 {
    /// Artifact program (pipeline program index / composite stage), param, layer.
    pub parameters: BTreeMap<(u16, u16, Option<u16>), TensorWireV1>,
    /// Tiled segmented jobs only: authenticated against the native job root.
    pub prompt: Vec<u32>,
    /// Full public snapshots, authenticated by recomputing their native roots.
    pub snapshots: BTreeMap<Digest, Vec<RetrievalItemV1>>,
}
fn bad(e: impl ToString) -> Refusal {
    Refusal::new("BOOTSTRAP_NATIVE", e.to_string())
}
fn zero() -> PrivateProgramResourceBoundV3 {
    PrivateProgramResourceBoundV3 { workspace_bytes: 0, material_bytes: 0, execution_work: 0 }
}
fn add(total: &mut PrivateProgramResourceBoundV3, part: PrivateProgramResourceBoundV3) -> Result<(), Refusal> {
    total.workspace_bytes = total.workspace_bytes.checked_add(part.workspace_bytes).ok_or_else(|| bad("workspace overflow"))?;
    total.material_bytes = total.material_bytes.checked_add(part.material_bytes).ok_or_else(|| bad("material overflow"))?;
    total.execution_work = total.execution_work.checked_add(part.execution_work).ok_or_else(|| bad("work overflow"))?;
    if total.workspace_bytes > 256 * 1024 * 1024 || total.material_bytes > 32 * 1024 * 1024 || total.execution_work > 100_000_000 {
        return Err(bad("all-stage private resource cap exceeded"));
    }
    Ok(())
}
fn wire(input: &NativePaidProducerInputsV3, program: u16) -> BTreeMap<(u16, Option<u16>), TensorWireV1> {
    input.parameters.iter().filter(|((p, _, _), _)| *p == program).map(|((_, p, l), v)| ((*p, *l), v.clone())).collect()
}
fn decode(w: &BTreeMap<(u16, Option<u16>), TensorWireV1>, expected: &ParamCommitmentsV1, v3: bool) -> Result<MapParams, Refusal> {
    let mut out = MapParams::default();
    for (key, value) in w {
        out.tensors.insert(*key, value.decode().map_err(bad)?);
    }
    let got = if v3 { ParamCommitmentsV1::of_v3(&out) } else { ParamCommitmentsV1::of(&out) };
    if &got != expected {
        return Err(bad("artifact differs from native registered commitments"));
    }
    Ok(out)
}
fn material_params(material: &mut PaidClaimMaterialWireV2, input: &NativePaidProducerInputsV3) {
    material.params = input.parameters.clone();
}
fn trace_material(material: &mut PaidClaimMaterialWireV2, stage: u8, offset: u32, trace: &TraceV1) {
    for (p, occ) in trace.values.iter().enumerate() {
        for (o, nodes) in occ.iter().enumerate() {
            for (n, value) in nodes.iter().enumerate() {
                material.nodes.insert((stage, offset + p as u32, o as u16, n as u16), TensorWireV1::of(value));
            }
        }
    }
    for (p, inputs) in trace.inputs.iter().enumerate() {
        for (k, value) in inputs.iter().enumerate() {
            material.inputs.insert((stage, offset + p as u32, k as u16), TensorWireV1::of(value));
        }
    }
}
fn snapshot(
    root: &RetrievalRootV1,
    input: &NativePaidProducerInputsV3,
    material: &mut PaidClaimMaterialWireV2,
    total: &mut PrivateProgramResourceBoundV3,
) -> Result<SnapshotDataV1, Refusal> {
    let items = input.snapshots.get(&root.snapshot.root()).ok_or_else(|| bad("missing native public snapshot"))?;
    let n = items.len() as u128;
    if n != u128::from(root.snapshot.items) {
        return Err(bad("snapshot item count differs from native class"));
    }
    // Exact live collections: input items, SnapshotData clone, public material
    // BTree, serialized material, leaf Vec plus old/new Merkle levels, sorted
    // (i128,u64,i64) score vector at retrieve, and bounded returned payloads.
    let payload = items
        .iter()
        .try_fold(0u128, |sum, i| sum.checked_add((i.key.len() + i.payload.len()) as u128 * 4 + 512))
        .ok_or_else(|| bad("snapshot size overflow"))?;
    let work =
        n.checked_mul(u128::from(root.snapshot.dim)).and_then(|x| x.checked_mul(8)).ok_or_else(|| bad("snapshot work overflow"))?;
    add(
        total,
        PrivateProgramResourceBoundV3 {
            workspace_bytes: payload
                .checked_mul(5)
                .and_then(|x| x.checked_add(n * 512))
                .ok_or_else(|| bad("snapshot heap overflow"))?,
            material_bytes: payload,
            execution_work: work,
        },
    )?;
    let data =
        SnapshotDataV1::new(items.clone(), root.snapshot.dim, root.snapshot.max_payload, root.snapshot.slice_items).map_err(bad)?;
    if data.snapshot != root.snapshot {
        return Err(bad("public snapshot differs from native registered root"));
    }
    for (id, item) in data.items.iter().enumerate() {
        material.snapshot_items.insert((root.snapshot.root(), id as u64), item.clone());
    }
    Ok(data)
}
/// Bound every stage before executing the first one. Stage plans are the native
/// registered upper positions, so a class too large for this private fixture is
/// refused even when a particular prompt could happen to run smaller work.
pub fn produce_native_work_v3(
    route: &PalwKernelRouteStateV1,
    base_job: Digest,
    producer: Digest,
    input: NativePaidProducerInputsV3,
) -> Result<ProgramPaidBootstrapWorkV3, Refusal> {
    produce_native_work_with_resident_v3(route, base_job, producer, input, 0)
}
/// Includes the authenticated restored request/route and caller-owned file buffer
/// in the private workspace cap before any tensor decoding or execution.
pub fn produce_native_work_with_resident_v3(
    route: &PalwKernelRouteStateV1,
    base_job: Digest,
    producer: Digest,
    input: NativePaidProducerInputsV3,
    resident_bytes: u128,
) -> Result<ProgramPaidBootstrapWorkV3, Refusal> {
    validate_input_v3(&input)?;
    if producer == [0; 64] {
        return Err(bad("zero producer"));
    }
    let l = route.ledger().map_err(bad)?;
    if l.claims.values().any(|c| c.job_id == base_job) {
        return Err(bad("job already has a claim"));
    }
    if let Some(job) = l.jobs.get(&base_job)
        && l.classes.get(&job.class_binding_id).is_some_and(|c| !misaka_palw_kernel::descriptor::is_segmented_v1(&c.descriptor))
    {
        if input.parameters.keys().any(|(p, _, _)| *p != 0) || !input.prompt.is_empty() || !input.snapshots.is_empty() {
            return Err(bad("unused Program inputs"));
        }
        let parameters = wire(&input, 0);
        let class = l.classes.get(&job.class_binding_id).ok_or_else(|| bad("missing Program class"))?;
        let mut full = zero();
        full.workspace_bytes = resident_bytes;
        add(&mut full, bound_program_work_v3(class, job, &parameters)?)?;
        let mut work = super::private_bootstrap::produce_program_work_v3(route, base_job, producer, parameters)?;
        work.resource_bound = full;
        return Ok(work);
    }
    let mut total = zero();
    total.workspace_bytes = resident_bytes;
    add(&mut total, zero())?;
    let mut material = PaidClaimMaterialWireV2::default();
    material_params(&mut material, &input);
    let commit = if let Some(job) = l.pipeline_jobs.get(&base_job) {
        let class = l.pipeline_classes.get(&job.class_binding_id).ok_or_else(|| bad("missing pipeline class"))?;
        if job.max_new_tokens > 64 {
            return Err(bad("private generation token cap"));
        }
        let mut program_wires = Vec::new();
        for (p, expected) in class.param_commitments.iter().enumerate() {
            let w = wire(&input, p as u16);
            program_wires.push((w, expected));
        }
        for (s, stage) in class.pipeline.stages.iter().enumerate() {
            let p = stage.program as usize;
            let view = misaka_palw_kernel::pipeline::stage_view_v1(&class.programs[p]);
            let plan = class.plan.stages.get(s).ok_or_else(|| bad("missing native stage plan"))?.clone();
            let count = plan.max_positions;
            let scratch = ClassRowV1 {
                descriptor: misaka_palw_kernel::descriptor::k2_tir_v2_descriptor(),
                program_bytes: view.view.encode(),
                program: view.view,
                plan,
                param_commitments: class.param_commitments[p].clone(),
                network_domain: class.network_domain,
                ruleset_digest: class.ruleset_digest,
                bounds: class.bounds,
                liability_daa: 0,
                court_scope: None,
            };
            let fake = KernelJobV1 {
                class_binding_id: job.class_binding_id,
                prompt: vec![0],
                max_new_tokens: count,
                decode: DecodeRuleV1::Greedy,
                nonce: [0; 64],
            };
            let mut bound = bound_program_work_v3(&scratch, &fake, &program_wires[p].0)?;
            // Kernel trace keeps each external/random input per position; stage
            // input cache and cloned input material coexist with that trace.
            let input_bytes = class.programs[p]
                .inputs
                .iter()
                .try_fold(0u128, |n, i| {
                    let e = i.shape.iter().try_fold(1u128, |x, d| x.checked_mul(u128::from(*d)))?;
                    n.checked_add(e.checked_mul(96)?.checked_add(512)?)
                })
                .ok_or_else(|| bad("pipeline input shape overflow"))?;
            bound.workspace_bytes = bound
                .workspace_bytes
                .checked_add(input_bytes.checked_mul(count as u128).ok_or_else(|| bad("pipeline input position product overflow"))?)
                .ok_or_else(|| bad("pipeline input workspace overflow"))?;
            bound.material_bytes = bound
                .material_bytes
                .checked_add(input_bytes.checked_mul(count as u128).ok_or_else(|| bad("pipeline input position product overflow"))?)
                .ok_or_else(|| bad("pipeline material overflow"))?;
            // Text execution evaluates each position once (unlike Program's
            // prefix replay); this deliberately retains the prefix-work bound.
            add(&mut total, bound)?;
        }
        struct Parameters(Vec<MapParams>);
        impl misaka_palw_tir::pipeline::PipelineParams for Parameters {
            fn params(&self, p: u16) -> &dyn misaka_palw_tir::ParamSource {
                &self.0[p as usize]
            }
        }
        let params = Parameters(program_wires.iter().map(|(w, e)| decode(w, e, false)).collect::<Result<_, _>>()?);
        let random = job.facts.random();
        let generated = if misaka_palw_tir::pipeline::stream_stage(&class.pipeline).is_some() {
            if job.max_new_tokens == 0 {
                return Err(bad("text pipeline needs generation budget"));
            }
            let mut n = 0;
            let limit = job.max_new_tokens;
            misaka_palw_tir::pipeline::run_text_pipeline(
                &class.pipeline,
                &class.programs,
                &params,
                &random,
                &job.facts.job(&[]),
                &mut |_, logits| {
                    use misaka_palw_tir::pipeline::TextSelectV1::*;
                    match DecodeRuleV1::Greedy.select(logits) {
                        Some(t) => {
                            n += 1;
                            if n >= limit { Last(t) } else { Next(t) }
                        }
                        None => End,
                    }
                },
            )
            .map_err(bad)?
            .1
        } else {
            Vec::new()
        };
        let full = job.facts.job(&generated);
        let trace =
            misaka_palw_kernel::pipeline::trace_pipeline_v1(&class.pipeline, &class.programs, &params, &random, &full).map_err(bad)?;
        let evidence = misaka_palw_kernel::pipeline::build_pipeline_evidence_v1(
            class.header(job.class_binding_id),
            &class.descriptor,
            &class.pipeline,
            &class.programs,
            &class.plan,
            &class.param_commitments,
            &trace,
            &full,
            random.binding(),
            2,
        )
        .map_err(bad)?;
        let claim = misaka_palw_kernel::pipeline_public::PipelineClaimV1 {
            job_id: base_job,
            producer_bond: producer,
            generated,
            output_root: evidence.output_root,
            evidence_root: evidence.root(),
        };
        let stages = trace.stages.iter().map(|t| misaka_palw_kernel::pipeline_public::StageCommitmentsV1::of(&t.evidence())).collect();
        for (s, t) in trace.stages.iter().enumerate() {
            trace_material(&mut material, s as u8, 0, t);
        }
        SaltedCommitV1::Pipeline { claim, evidence, stages }
    } else if let Some(job) = l.typed.jobs.get(&base_job) {
        let class = l.typed.classes.get(&job.class()).ok_or_else(|| bad("missing typed class"))?;
        let claim = match (job, &class.kind) {
            (SpecJobV1::Memory(job), SpecClassKindV1::Memory { rule, root, writers }) => {
                let w = wire(&input, 0);
                // produce_memory retains all step traces; sum their individual
                // bounds (including copied state/artifact) before the first step.
                for (i, chunk) in job.chunks.iter().enumerate() {
                    let sub = misaka_palw_kernel::spec::memory::step_job_v1(&job.class, &base_job, i as u32, chunk);
                    add(&mut total, bound_program_work_v3(rule, &sub, &w)?)?;
                }
                let params = decode(&w, &rule.param_commitments, false)?;
                let pre = l
                    .memory_head_tensors_v1(&job.class, &material)
                    .ok_or_else(|| bad("authenticated memory line head unavailable"))?;
                let got = misaka_palw_kernel::spec::produce::produce_memory_v1(
                    &job.class,
                    rule,
                    root,
                    writers,
                    job,
                    &producer,
                    &params,
                    &pre,
                    2,
                    |_, _| {},
                )
                .map_err(bad)?;
                let mut offset = 0;
                for t in &got.traces {
                    trace_material(&mut material, 0, offset, t);
                    offset += t.values.len() as u32;
                }
                for (i, t) in pre.iter().enumerate() {
                    material.nodes.insert((misaka_palw_kernel::spec::MEMORY_PRE_STATE_STAGE_V1, 0, 0, i as u16), TensorWireV1::of(t));
                }
                SpecClaimV1::Memory(got.claim)
            }
            (SpecJobV1::Retrieval(job), SpecClassKindV1::Retrieval { root }) => {
                let data = snapshot(root, &input, &mut material, &mut total)?;
                let result = data.retrieve(root, &job.query).map_err(bad)?;
                SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: base_job, producer_bond: producer, result })
            }
            (SpecJobV1::Composite(job), SpecClassKindV1::Composite { root, components }) => {
                // A prompt can concatenate <=8 earlier generation/payload lists.
                // Root-level max sizes, not optimistic result sizes, bound every
                // model stage before any generation or retrieval executes.
                let mut output_caps = Vec::<usize>::new();
                let mut snapshots = BTreeMap::<usize, SnapshotDataV1>::new();
                for (s, (stage, component)) in root.stages.iter().zip(components).enumerate() {
                    match (&stage.input, component) {
                        (StageInputV1::Tokens { sources, max_new_tokens }, ComponentV1::Model(rule)) => {
                            if *max_new_tokens == 0 || *max_new_tokens > 64 {
                                return Err(bad("finite composite generation budget required"));
                            }
                            let mut n = 0usize;
                            for src in sources {
                                use misaka_palw_kernel::spec::composite::TokenSourceV1::*;
                                let cap = match src {
                                    JobPrompt => job.prompt.len(),
                                    StagePayloads { stage } | StageGenerated { stage } => {
                                        *output_caps.get(*stage as usize).ok_or_else(|| bad("forward composite source"))?
                                    }
                                };
                                n = n.checked_add(cap).ok_or_else(|| bad("composite prompt overflow"))?;
                            }
                            if n == 0 || n > rule.plan.max_positions as usize {
                                return Err(bad("composite prompt exceeds native plan"));
                            }
                            let sub = stage_job_v1(&stage.component, &base_job, s as u8, vec![0; n], *max_new_tokens);
                            add(&mut total, bound_program_work_v3(rule, &sub, &wire(&input, s as u16))?)?;
                            output_caps.push(*max_new_tokens as usize);
                        }
                        (StageInputV1::Query(_), ComponentV1::Retrieval(r)) => {
                            snapshots.insert(s, snapshot(r, &input, &mut material, &mut total)?);
                            output_caps.push(
                                r.result_len()
                                    .checked_mul(r.snapshot.max_payload as usize)
                                    .ok_or_else(|| bad("retrieval payload cap overflow"))?,
                            );
                        }
                        _ => return Err(bad("typed stage kind differs from native class")),
                    }
                }
                let mut stages = Vec::new();
                let mut traces = BTreeMap::<usize, TraceV1>::new();
                for (s, (stage, component)) in root.stages.iter().zip(components).enumerate() {
                    match (&stage.input, component) {
                        (StageInputV1::Tokens { sources, max_new_tokens }, ComponentV1::Model(rule)) => {
                            let prompt = stage_prompt_v1(sources, job, &stages).ok_or_else(|| bad("composite input unavailable"))?;
                            let params = decode(&wire(&input, s as u16), &rule.param_commitments, false)?;
                            let (st, t) = misaka_palw_kernel::spec::produce::produce_model_stage_v1(
                                &stage.component,
                                rule,
                                &base_job,
                                s as u8,
                                prompt,
                                *max_new_tokens,
                                &producer,
                                &params,
                                2,
                                |_| {},
                            )
                            .map_err(bad)?;
                            trace_material(&mut material, s as u8, 0, &t);
                            traces.insert(s, t);
                            stages.push(st);
                        }
                        (StageInputV1::Query(source), ComponentV1::Retrieval(r)) => {
                            let query = match source {
                                QuerySourceV1::JobQuery => job.query.clone(),
                                QuerySourceV1::StageLogits { stage } => {
                                    let ComponentV1::Model(m) = &components[*stage as usize] else {
                                        return Err(bad("query source is not model"));
                                    };
                                    let t = traces.get(&(*stage as usize)).ok_or_else(|| bad("missing upstream actual trace"))?;
                                    let (o, n) = m.logits_at();
                                    t.values
                                        .last()
                                        .and_then(|p| p.get(o as usize))
                                        .and_then(|p| p.get(n as usize))
                                        .ok_or_else(|| bad("missing query logits"))?
                                        .data
                                        .iter()
                                        .map(|v| i32::try_from(*v).map_err(bad))
                                        .collect::<Result<Vec<_>, _>>()?
                                }
                            };
                            let data = snapshots.get(&s).ok_or_else(|| bad("missing pre-admitted snapshot"))?;
                            stages.push(misaka_palw_kernel::spec::produce::produce_retrieval_stage_v1(r, data, &query).map_err(bad)?);
                        }
                        _ => return Err(bad("typed stage mismatch")),
                    }
                }
                SpecClaimV1::Composite(CompositeClaimV1 { job_id: base_job, producer_bond: producer, stages })
            }
            _ => return Err(bad("typed job and class kinds differ")),
        };
        SaltedCommitV1::Spec { claim }
    } else if let Some(job) = l.job_view_v1(&base_job) {
        let class = l.classes.get(&job.class_binding_id).ok_or_else(|| bad("missing segmented class"))?;
        if !job.complete || !misaka_palw_kernel::descriptor::is_segmented_v1(&class.descriptor) {
            return Err(bad("complete segmented job required"));
        }
        let prompt = match &job.inline_prompt {
            Some(p) => p.clone(),
            None => input.prompt.clone(),
        };
        if prompt.len() != job.prompt_len as usize
            || misaka_palw_kernel::seg::prompt_root_of_ids_v1(&prompt) != job.prompt_root
            || prompt.iter().any(|t| *t >= class.program.token_bound)
        {
            return Err(bad("prompt differs from native tiled root"));
        }
        let encoder = misaka_palw_kernel::descriptor::is_encoder_v1(&class.descriptor);
        let virtual_job = KernelJobV1 {
            class_binding_id: job.class_binding_id,
            prompt: if encoder { vec![0] } else { prompt.clone() },
            max_new_tokens: if encoder { 1 } else { job.max_new_tokens },
            decode: job.decode,
            nonce: [0; 64],
        };
        let w = wire(&input, 0);
        let mut bound = bound_program_work_v3(class, &virtual_job, &w)?;
        // SegMaterial carries a full position beside the public node map and
        // its serialization; this is one additional wire copy of each trace.
        bound.workspace_bytes =
            bound.workspace_bytes.checked_add(bound.material_bytes * 3).ok_or_else(|| bad("segmented workspace overflow"))?;
        bound.material_bytes = bound.material_bytes.checked_mul(2).ok_or_else(|| bad("segmented material overflow"))?;
        add(&mut total, bound)?;
        let params = decode(&w, &class.param_commitments, true)?;
        let mut generated = Vec::new();
        let mut stream = prompt.clone();
        let trace = if encoder {
            let binding = misaka_palw_kernel::seg_encoder::encoder_binding_v1(&class.program).map_err(bad)?;
            misaka_palw_kernel::seg_encoder::trace_encoder_v1(&class.program, &params, &binding, &prompt).map_err(bad)?
        } else {
            for i in 0..job.max_new_tokens {
                let t = misaka_palw_kernel::trace::trace_v1(&class.program, &params, &stream).map_err(bad)?;
                let (o, n) = class.logits_at();
                let tok = job
                    .decode
                    .select(&t.values[stream.len() - 1][o as usize][n as usize])
                    .ok_or_else(|| bad("segmented decode unavailable"))?;
                generated.push(tok);
                if i + 1 < job.max_new_tokens {
                    stream.push(tok);
                }
            }
            misaka_palw_kernel::trace::trace_v1(&class.program, &params, &stream).map_err(bad)?
        };
        let commits = misaka_palw_kernel::seg::seg_commitments_of_trace_v1(&class.program, &trace);
        let fed = if encoder { &[][..] } else { &generated[..generated.len() - 1] };
        let evidence = misaka_palw_kernel::seg::build_segmented_evidence_v1(
            class.header(job.class_binding_id),
            &class.descriptor,
            job.prompt_len,
            &job.prompt_root,
            fed,
            &commits,
        );
        let claim = KernelClaimV1 { job_id: base_job, producer_bond: producer, generated, evidence_root: evidence.root() };
        trace_material(&mut material, 0, 0, &trace);
        material.tokens = stream;
        for (p, occ) in trace.values.iter().enumerate() {
            material
                .segmented_positions
                .insert(p as u32, occ.iter().map(|nodes| nodes.iter().map(TensorWireV1::of).collect()).collect());
            material.segmented_siblings.insert(p as u32, commits.position_path(p as u32).1);
        }
        SaltedCommitV1::Segmented { claim, evidence, segment_roots: commits.segment_roots() }
    } else {
        return Err(bad("native posted job absent"));
    };
    // Serialize only after the whole actual output fits its pre-admitted bound.
    let actual = borsh::object_length(&material).map_err(bad)? as u128;
    if actual > total.material_bytes {
        return Err(bad("actual material exceeds derived producer bound"));
    }
    Ok(ProgramPaidBootstrapWorkV3 { version: 3, base_job, commit, material, resource_bound: total })
}

/// Caller-created SDK inputs obey the same finite allocation contract as the
/// private exporter; canonical codec preflight there runs before deserialization.
pub fn validate_input_v3(input: &NativePaidProducerInputsV3) -> Result<(), Refusal> {
    if input.parameters.len() > 16_384 || input.prompt.len() > 65_536 || input.snapshots.len() > 8 {
        return Err(bad("private input collection count exceeds contract"));
    }
    let bytes = borsh::object_length(input).map_err(bad)?;
    if bytes > 32 * 1024 * 1024 {
        return Err(bad("private all-target input exceeds 32 MiB"));
    }
    let mut heap = 0u128;
    for wire in input.parameters.values() {
        if wire.shape.len() > 12 {
            return Err(bad("tensor rank exceeds 12"));
        }
        let dtype = misaka_palw_kernel::plan::dtype_of_tag(wire.dtype).ok_or_else(|| bad("unknown artifact dtype"))?;
        let elements = wire
            .shape
            .iter()
            .try_fold(1u128, |n, d| if *d == 0 { None } else { n.checked_mul(u128::from(*d)) })
            .ok_or_else(|| bad("tensor shape overflow/zero axis"))?;
        if elements.checked_mul(dtype.width() as u128) != Some(wire.bytes.len() as u128) {
            return Err(bad("tensor byte width differs from exact shape"));
        }
        heap = heap
            .checked_add(elements.checked_mul(16).ok_or_else(|| bad("tensor heap overflow"))?)
            .ok_or_else(|| bad("artifact heap overflow"))?;
    }
    if heap > 64 * 1024 * 1024 {
        return Err(bad("decoded artifact exceeds 64 MiB"));
    }
    for items in input.snapshots.values() {
        if items.len() > 65_536 || items.iter().any(|i| i.key.len() > 4096 || i.payload.len() > 4096) {
            return Err(bad("snapshot shape/count exceeds private contract"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_target_inputs_refuse_shape_bombs_before_decode() {
        let mut input = NativePaidProducerInputsV3::default();
        input.parameters.insert((0, 0, None), TensorWireV1 { dtype: 0, shape: vec![u64::MAX; 12], bytes: vec![] });
        assert!(validate_input_v3(&input).is_err());
        input.parameters.get_mut(&(0, 0, None)).unwrap().shape = vec![1];
        assert!(validate_input_v3(&input).is_err(), "wrong exact byte width");
        input.parameters.get_mut(&(0, 0, None)).unwrap().bytes = vec![1];
        assert!(validate_input_v3(&input).is_ok());
    }
    #[test]
    fn all_target_stage_resource_union_keeps_resident_and_each_stage() {
        let mut total = zero();
        total.workspace_bytes = 64 * 1024 * 1024;
        let part = PrivateProgramResourceBoundV3 { workspace_bytes: 96 * 1024 * 1024, material_bytes: 1, execution_work: 1 };
        add(&mut total, part).unwrap();
        add(&mut total, part).unwrap();
        assert!(add(&mut total, PrivateProgramResourceBoundV3 { workspace_bytes: 1, ..zero() }).is_err());
    }
    #[test]
    fn public_snapshot_is_authenticated_before_it_becomes_producer_material() {
        // Snapshot construction is the same native Merkle root routine used by
        // registered retrieval classes; no caller-provided leaf/root is trusted.
        let data = SnapshotDataV1::new(vec![RetrievalItemV1 { key: vec![1], payload: vec![2] }], 1, 1, 1).unwrap();
        let mut input = NativePaidProducerInputsV3::default();
        input.snapshots.insert(data.snapshot.root(), vec![RetrievalItemV1 { key: vec![9], payload: vec![2] }]);
        // Check the exact root property without inventing a retrieval policy.
        let altered = SnapshotDataV1::new(input.snapshots[&data.snapshot.root()].clone(), 1, 1, 1).unwrap();
        assert_ne!(data.snapshot, altered.snapshot);
    }
}
