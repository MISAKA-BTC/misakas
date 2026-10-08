//! **RFC-0004 Part II `Composite`: a pipeline of registered computations — models and verified tools — with exact edges.**
//!
//! A composite class lists stages; each stage's **component** is a registered class the chain already judges: a Weights class (a TIR
//! program: a model, or a tool written as a program) or a Retrieval class (a deterministic retrieval tool). There is no other kind of
//! tool: no external API, no uploaded code. A stage's input comes from the job or from earlier stages:
//!
//! * a **model stage** reads a token stream — the concatenation of [`TokenSourceV1`]s (the job's prompt, a retrieval stage's payload
//!   tokens in result order, a model stage's delivered tokens) — and is the K2 claim of the sub-job
//!   `KernelJobV1 { component, stream, max_new_tokens, Greedy, H(job, stage) }`;
//! * a **retrieval stage** reads a query ([`QuerySourceV1`]): the job's, or a model stage's committed logits at its last position.
//!
//! **Edges** have no arithmetic (RFC-0003's edge relation). When both sides are on chain (the job's prompt and query, a retrieval
//! stage's payloads — carried in the claim and checked against the result's payload digests — and a model stage's delivered tokens), the
//! edge is recomputed **at inclusion**: a model stage's evidence must commit exactly that input, so an edge fault cannot be committed.
//! When the source is a committed-only value (`StageLogits`), the **edge court** ([`super::SpecFaultV1::Edge`]) opens that tensor
//! against the upstream stage's commitments and convicts iff it is not the carried query.
//!
//! **Per-stage localisation**: every fault names its stage and is judged by that stage's component's own court on the stage's record;
//! a filing against an honest stage is dismissed, so a conviction always names the stage that lied.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::DType;

use crate::evidence::VerificationEvidenceV1;
use crate::hash::{Digest, object_id};
use crate::job::{DecodeRuleV1, KernelClaimV1, KernelJobV1};
use crate::ledger::ClassRowV1;
use crate::public::PublicClaimRecordV1;

use super::retrieval::{RetrievalRootV1, RetrievedV1};

pub const COMPOSITE_STAGE_NONCE_DOMAIN_V1: &[u8] = b"misaka-palw/spec/composite-stage-nonce/v1";
pub const COMPOSITE_STAGE_CLAIM_DOMAIN_V1: &[u8] = b"misaka-palw/spec/composite-stage-claim/v1";

pub const MIN_COMPOSITE_STAGES_V1: usize = 2;
pub const MAX_COMPOSITE_STAGES_V1: usize = 8;
pub const MAX_TOKEN_SOURCES_V1: usize = 8;

/// Where a model stage's tokens come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum TokenSourceV1 {
    JobPrompt = 0,
    /// A retrieval stage's payload tokens, in result order.
    StagePayloads {
        stage: u8,
    } = 1,
    /// A model stage's delivered tokens.
    StageGenerated {
        stage: u8,
    } = 2,
}

/// Where a retrieval stage's query comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum QuerySourceV1 {
    JobQuery = 0,
    /// A model stage's committed logits at its last position (the edge court's case).
    StageLogits {
        stage: u8,
    } = 1,
}

/// A stage's input.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum StageInputV1 {
    /// A model stage: its prompt and generation budget.
    Tokens { sources: Vec<TokenSourceV1>, max_new_tokens: u32 } = 0,
    /// A retrieval stage: its query.
    Query(QuerySourceV1) = 1,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompositeStageV1 {
    /// A registered class: a Weights class (model) or a Retrieval class (tool).
    pub component: Digest,
    pub input: StageInputV1,
}

/// **The `Composite` root kind** (version 1).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompositeRootV1 {
    /// The typed-roots extension descriptor's digest (`K2-TR-v1`).
    pub extension: Digest,
    pub stages: Vec<CompositeStageV1>,
}

/// A resolved component (derived from the ledger's rows at registration and at every rebuild).
#[derive(Clone, Debug)]
pub enum ComponentV1 {
    Model(Box<ClassRowV1>),
    Retrieval(RetrievalRootV1),
}

/// **The registration checks of a composite** over its resolved components (`None`: not a registered model or retrieval class — a
/// memory or composite component is refused by name before this).
pub fn check_composite_root_v1(root: &CompositeRootV1, components: &[ComponentV1]) -> Result<(), String> {
    let n = root.stages.len();
    if !(MIN_COMPOSITE_STAGES_V1..=MAX_COMPOSITE_STAGES_V1).contains(&n) || components.len() != n {
        return Err(format!("a composite has {MIN_COMPOSITE_STAGES_V1}..={MAX_COMPOSITE_STAGES_V1} stages"));
    }
    let is_model = |t: u8, s: usize| (t as usize) < s && matches!(components[t as usize], ComponentV1::Model(_));
    let is_retrieval = |t: u8, s: usize| (t as usize) < s && matches!(components[t as usize], ComponentV1::Retrieval(_));
    for (s, (st, c)) in root.stages.iter().zip(components).enumerate() {
        match (&st.input, c) {
            (StageInputV1::Tokens { sources, max_new_tokens }, ComponentV1::Model(_)) => {
                if sources.is_empty() || sources.len() > MAX_TOKEN_SOURCES_V1 || *max_new_tokens == 0 {
                    return Err(format!("stage {s}: a model stage reads 1..={MAX_TOKEN_SOURCES_V1} sources and generates"));
                }
                for src in sources {
                    let ok = match src {
                        TokenSourceV1::JobPrompt => true,
                        TokenSourceV1::StagePayloads { stage } => is_retrieval(*stage, s),
                        TokenSourceV1::StageGenerated { stage } => is_model(*stage, s),
                    };
                    if !ok {
                        return Err(format!("stage {s}: a source names no earlier stage of its kind"));
                    }
                }
            }
            (StageInputV1::Query(q), ComponentV1::Retrieval(r)) => {
                if let QuerySourceV1::StageLogits { stage } = q {
                    if !is_model(*stage, s) {
                        return Err(format!("stage {s}: the query names no earlier model stage"));
                    }
                    let ComponentV1::Model(m) = &components[*stage as usize] else { unreachable!("checked") };
                    let (post, logits) = m.logits_at();
                    let b = m.program.occurrences()[post as usize].0 as usize;
                    let ty = &m.program.blocks[b].nodes[logits as usize].out;
                    if ty.resolve(1) != vec![r.snapshot.dim as usize] || !matches!(ty.dtype, DType::I8 | DType::I16 | DType::I32) {
                        return Err(format!("stage {s}: the upstream logits are not an i32 vector of the snapshot's key length"));
                    }
                }
            }
            _ => {
                return Err(format!(
                    "stage {s}: the input is not the component's kind (a model reads tokens, a retrieval tool a query)"
                ));
            }
        }
    }
    Ok(())
}

/// A composite job: the prompt and the query its stages may read.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompositeJobV1 {
    pub class: Digest,
    pub prompt: Vec<u32>,
    pub query: Vec<i32>,
    pub nonce: Digest,
}

/// One stage of a composite claim.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum StageClaimV1 {
    Model {
        generated: Vec<u32>,
        evidence: VerificationEvidenceV1,
        commitments: Vec<Vec<Vec<Digest>>>,
    } = 0,
    /// The query the stage scored (the job's, or the carried upstream logits), its result and the retrieved items' payload tokens.
    Retrieval {
        query: Vec<i32>,
        result: Vec<RetrievedV1>,
        payloads: Vec<Vec<u32>>,
    } = 1,
}

/// A composite claim (signed by `producer_bond`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompositeClaimV1 {
    pub job_id: Digest,
    pub producer_bond: Digest,
    pub stages: Vec<StageClaimV1>,
}

/// **A model stage's input stream** (`None`: a source names a stage of another kind in the claim — refused at inclusion).
pub fn stage_prompt_v1(sources: &[TokenSourceV1], job: &CompositeJobV1, stages: &[StageClaimV1]) -> Option<Vec<u32>> {
    let mut out = Vec::new();
    for src in sources {
        match src {
            TokenSourceV1::JobPrompt => out.extend_from_slice(&job.prompt),
            TokenSourceV1::StagePayloads { stage } => match stages.get(*stage as usize)? {
                StageClaimV1::Retrieval { payloads, .. } => payloads.iter().for_each(|p| out.extend_from_slice(p)),
                _ => return None,
            },
            TokenSourceV1::StageGenerated { stage } => match stages.get(*stage as usize)? {
                StageClaimV1::Model { generated, .. } => out.extend_from_slice(generated),
                _ => return None,
            },
        }
    }
    Some(out)
}

/// The sub-job a model stage is a claim of.
pub fn stage_job_v1(component: &Digest, job_id: &Digest, stage: u8, prompt: Vec<u32>, max_new_tokens: u32) -> KernelJobV1 {
    KernelJobV1 {
        class_binding_id: *component,
        prompt,
        max_new_tokens,
        decode: DecodeRuleV1::Greedy,
        nonce: object_id(COMPOSITE_STAGE_NONCE_DOMAIN_V1, &(*job_id, stage)),
    }
}

pub fn stage_claim_v1(job: &KernelJobV1, producer: &Digest, generated: &[u32], evidence: &VerificationEvidenceV1) -> KernelClaimV1 {
    KernelClaimV1 { job_id: job.id(), producer_bond: *producer, generated: generated.to_vec(), evidence_root: evidence.root() }
}

pub fn stage_claim_id_v1(claim: &Digest, stage: u8) -> Digest {
    object_id(COMPOSITE_STAGE_CLAIM_DOMAIN_V1, &(*claim, stage))
}

/// A model stage's public record: the component's, over the stage's evidence and commitments.
pub fn model_stage_record_v1(
    claim_id: &Digest,
    stage: u8,
    component: &ClassRowV1,
    job: &KernelJobV1,
    claim: &KernelClaimV1,
    evidence: &VerificationEvidenceV1,
    commitments: &[Vec<Vec<Digest>>],
) -> PublicClaimRecordV1 {
    PublicClaimRecordV1 {
        claim_id: stage_claim_id_v1(claim_id, stage),
        program_bytes: component.program_bytes.clone(),
        plan: component.plan.clone(),
        evidence: evidence.clone(),
        trace_commitments: commitments.to_vec(),
        param_commitments: component.param_commitments.by_instance.iter().map(|((j, l), d)| (*j, *l, *d)).collect(),
        tokens: claim.stream(job),
        beacon: [0; 64],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_concatenates_its_sources_in_order() {
        let job = CompositeJobV1 { class: [0; 64], prompt: vec![1, 2], query: vec![], nonce: [0; 64] };
        let stages = vec![
            StageClaimV1::Retrieval { query: vec![], result: vec![], payloads: vec![vec![7], vec![8, 9]] },
            StageClaimV1::Model {
                generated: vec![4],
                evidence: crate::evidence::VerificationEvidenceV1 {
                    version: 1,
                    header: crate::evidence::EvidenceHeaderV1 {
                        network_domain: [0; 64],
                        ruleset_digest: [0; 64],
                        class_binding_id: [0; 64],
                        program_root: [0; 64],
                        artifact_root: [0; 64],
                        plan_root: [0; 64],
                    },
                    job_input_root: [0; 64],
                    positions: 0,
                    initial_state_root: [0; 64],
                    final_state_root: [0; 64],
                    trace_root: [0; 64],
                    output_root: [0; 64],
                    segments: vec![],
                    suite: crate::evidence::SuiteParamsV1::of(&crate::descriptor::k2_tir_v1_descriptor()),
                },
                commitments: vec![],
            },
        ];
        let srcs = [TokenSourceV1::JobPrompt, TokenSourceV1::StagePayloads { stage: 0 }, TokenSourceV1::StageGenerated { stage: 1 }];
        assert_eq!(stage_prompt_v1(&srcs, &job, &stages), Some(vec![1, 2, 7, 8, 9, 4]));
        assert_eq!(stage_prompt_v1(&[TokenSourceV1::StagePayloads { stage: 1 }], &job, &stages), None, "a model is no retrieval");
    }
}
