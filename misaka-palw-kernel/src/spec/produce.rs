//! **An honest producer of typed claims** (and, with `lie`, a dishonest one): what a worker runs to answer a typed job, from the class
//! row the chain serves and the public material. Pure functions; nothing here is consensus.

use misaka_palw_tir::{MapParams, Tensor};

use crate::evidence::build_evidence_v1;
use crate::hash::Digest;
use crate::job::DecodeRuleV1;
use crate::ledger::ClassRowV1;
use crate::trace::{ParamCommitmentsV1, TraceV1, WiringV1, tensor_commitment, trace_v1};

use super::composite::{CompositeJobV1, StageClaimV1, stage_claim_v1, stage_job_v1};
use super::memory::{
    MemoryClaimV1, MemoryJobV1, MemoryRootV1, StepCommitV1, memory_root_v1, overlay_v1, step_claim_v1, step_header_v1, step_job_v1,
};
use super::retrieval::{RetrievalRootV1, SnapshotDataV1, payload_digest_v1};

/// A produced memory claim with the producer's private traces and the memory it leaves.
#[derive(Clone, Debug)]
pub struct MemoryProductionV1 {
    pub claim: MemoryClaimV1,
    pub traces: Vec<TraceV1>,
    /// The post-state tensors, in slot order (the next job's pre-state).
    pub post: Vec<Tensor>,
}

/// **Produce a memory claim.** `base` is the rule's artifact (its base weights; the slot params are replaced per step), `pre` the
/// pre-state tensors (the line head's), `lie(step, trace)` edits a step's committed trace before it is committed (the honest
/// producer passes a no-op). Each step's delivered token is the greedy token of the trace it commits.
#[allow(clippy::too_many_arguments)]
pub fn produce_memory_v1(
    class: &Digest,
    rule: &ClassRowV1,
    root: &MemoryRootV1,
    writers: &[(u16, u16)],
    job: &MemoryJobV1,
    producer: &Digest,
    base: &MapParams,
    pre: &[Tensor],
    segment_len: u32,
    mut lie: impl FnMut(usize, &mut TraceV1),
) -> Result<MemoryProductionV1, String> {
    let job_id = super::SpecJobV1::Memory(job.clone()).id();
    let w = WiringV1::new(&rule.program).map_err(|e| e.to_string())?;
    let (post_occ, logits) = rule.logits_at();
    let mut state: Vec<Tensor> = pre.to_vec();
    let mut roots = Vec::with_capacity(job.chunks.len() + 1);
    let (mut steps, mut traces, mut generated) = (Vec::new(), Vec::new(), Vec::new());
    for (i, chunk) in job.chunks.iter().enumerate() {
        let commitments: Vec<Digest> = state.iter().map(tensor_commitment).collect();
        roots.push(memory_root_v1(&root.slots, &commitments));
        let mut params = base.clone();
        for (slot, t) in root.slots.iter().zip(&state) {
            params.tensors.insert(slot.param, t.clone());
        }
        let overlay = overlay_v1(&rule.param_commitments, &root.slots, &commitments);
        let mut trace = trace_v1(&rule.program, &params, chunk).map_err(|e| format!("step {i}: {e}"))?;
        lie(i, &mut trace);
        let last = chunk.len() - 1;
        let token = DecodeRuleV1::Greedy.select(&trace.values[last][post_occ as usize][logits as usize]).ok_or("empty logits")?;
        let header = step_header_v1(rule, class, &overlay);
        let evidence = build_evidence_v1(&w, &trace.evidence(), chunk, header, &rule.descriptor, segment_len)
            .map_err(|e| format!("step {i}: {e}"))?;
        // The step's sub-claim is what the chain derives; nothing of it is carried but the token and the evidence.
        let _ = step_claim_v1(&step_job_v1(class, &job_id, i as u32, chunk), producer, token, evidence.root());
        state = writers.iter().map(|(s, n)| trace.values[last][*s as usize][*n as usize].clone()).collect();
        steps.push(StepCommitV1 { evidence, commitments: trace.evidence().commitments });
        generated.push(token);
        traces.push(trace);
    }
    let commitments: Vec<Digest> = state.iter().map(tensor_commitment).collect();
    roots.push(memory_root_v1(&root.slots, &commitments));
    Ok(MemoryProductionV1 {
        claim: MemoryClaimV1 { job_id, producer_bond: *producer, generated, step_roots: roots, steps },
        traces,
        post: state,
    })
}

/// **A model stage**: greedy generation of `max_new_tokens` over `prompt` with `params`, the committed trace (edited by `lie`), the
/// evidence under the component's header.
#[allow(clippy::too_many_arguments)]
pub fn produce_model_stage_v1(
    component: &Digest,
    m: &ClassRowV1,
    job_id: &Digest,
    stage: u8,
    prompt: Vec<u32>,
    max_new_tokens: u32,
    producer: &Digest,
    params: &MapParams,
    segment_len: u32,
    lie: impl FnOnce(&mut TraceV1),
) -> Result<(StageClaimV1, TraceV1), String> {
    let (post, logits) = m.logits_at();
    let mut stream = prompt.clone();
    let mut generated = Vec::new();
    for _ in 0..max_new_tokens {
        let t = trace_v1(&m.program, params, &stream).map_err(|e| e.to_string())?;
        let tok = DecodeRuleV1::Greedy.select(&t.values[stream.len() - 1][post as usize][logits as usize]).ok_or("empty logits")?;
        generated.push(tok);
        stream.push(tok);
    }
    let job = stage_job_v1(component, job_id, stage, prompt, max_new_tokens);
    let fed: Vec<u32> = job.prompt.iter().chain(&generated[..generated.len() - 1]).copied().collect();
    let mut trace = trace_v1(&m.program, params, &fed).map_err(|e| e.to_string())?;
    lie(&mut trace);
    let w = WiringV1::new(&m.program).map_err(|e| e.to_string())?;
    let evidence = build_evidence_v1(&w, &trace.evidence(), &fed, m.header(*component), &m.descriptor, segment_len)?;
    let _ = stage_claim_v1(&job, producer, &generated, &evidence);
    Ok((StageClaimV1::Model { generated, evidence, commitments: trace.evidence().commitments }, trace))
}

/// **A retrieval stage** (or a plain retrieval claim's result): the rule's result for `query` and the retrieved payloads.
pub fn produce_retrieval_stage_v1(root: &RetrievalRootV1, snapshot: &SnapshotDataV1, query: &[i32]) -> Result<StageClaimV1, String> {
    let result = snapshot.retrieve(root, query)?;
    let payloads: Vec<Vec<u32>> = result.iter().map(|e| snapshot.items[e.id as usize].payload.clone()).collect();
    debug_assert!(payloads.iter().zip(&result).all(|(p, e)| payload_digest_v1(p) == e.payload_digest));
    Ok(StageClaimV1::Retrieval { query: query.to_vec(), result, payloads })
}

/// The composite job's id (a convenience for producers building stage sub-jobs).
pub fn composite_job_id_v1(job: &CompositeJobV1) -> Digest {
    super::SpecJobV1::Composite(job.clone()).id()
}

/// The commitments of the given state tensors (a pre-state's slot commitments).
pub fn commitments_of_v1(tensors: &[Tensor]) -> Vec<Digest> {
    tensors.iter().map(tensor_commitment).collect()
}

/// The rule's base commitments with `M0` replaced (a convenience for tests that start a line from another memory).
pub fn with_memory_v1(base: &ParamCommitmentsV1, root: &MemoryRootV1, m: &[Tensor]) -> ParamCommitmentsV1 {
    overlay_v1(base, &root.slots, &commitments_of_v1(m))
}
