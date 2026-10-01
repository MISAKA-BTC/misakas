//! **RFC-0004's evaluation executor (work item A10)**: an evaluation job run as the RFC-0003 pipeline
//! the chain derives for it — the subject class's generation under the item's seed (ExactMatch: the
//! subject's text stage alone, scored by the fold at the key's reveal) or its teacher-forced pass over
//! the disclosed reference then the scoring library's two stages (RefLogLik) — into the claim's one step
//! tree (`palw_gen_execute_v1`, `palw_gen_replay_committed_v1`) and the evaluation claim built from it.
//!
//! **Everything is the chain's, nothing is chosen.** The context is A6's
//! (`palw_improve_eval_context_v1`: the subject's program lifted unchanged, its own layout, the policy's
//! stage parameters), the binding and the roots are A6's (`PalwEvalBindingV1`), the carriage is A6's
//! (FP job version 9: [`PalwEvalClaimV1`] holds the commitment, the payload tail and the prompt the
//! payload carries, each checked by `palw_fp_eval_claim_check_v1` before a node spends a fee on it).
//! The executor adds only what a node holds: the subject's weights ([`PalwEvalHeldV1`], from a held IR
//! artifact — a composite candidate's served over its parent's — or, in tools and tests, a map).
//!
//! * **The seat's replay** ([`palw_eval_seat_judge_v1`]): the claim's job, prompt and tail replayed on
//!   this seat's own weights and held to the commitment's roots — a difference is the court's question
//!   (a seat files nothing on it, never a sampled conviction: the free-prompt seat's rule).
//! * **The capture** ([`PalwEvalCaptureV1`]): a run's every leaf as the claim committed it, in the
//!   claim's one order — what data availability serves and what rebuilds an accused's execution (lies
//!   included), with [`palw_eval_first_divergence_v1`] naming the leaf an honest challenger disputes.
//!
//! * **A lying executor and the dispute that finds it** (drill D-M3): [`palw_eval_run_faulted_v1`] runs a job and
//!   commits it with a [`PalwEvalFaultV1`] (a moved step leaf, a moved first id, a moved score) — a claim that is
//!   self-consistent, so the chain's own door takes it — and [`palw_eval_dispute_v1`] /
//!   [`palw_eval_dispute_committed_v1`] say where an honest replay parts from an accused's capture
//!   ([`PalwEvalDisputeV1`]: the first divergent leaf, else the first id, else the score), the capture held to
//!   the roots the claim committed. The court proof built from that finding ([`palw_eval_court_filing_v1`]) is the
//!   evaluation court's (spec 17 §17.8.6): `EvalCone` (court proof tag 13) at the divergent leaf — only the leaf named
//!   where its cone reduces over the history — `EvalDecodeToken` (tag 14) at a moved id or score, each built from the
//!   accused's capture ([`PalwEvalCaptureV1::rebuild_execution`]) and checked as the chain checks it.
//!
//! **Judged kinds** (A6, spec 17 §17.8.5) have a context now — the judge class's teacher-forced pass over the
//! registered template filled with the item and the final generations it reads — but this node plans none
//! ([`crate::improve::palw_improve_kind_runnable_v1`]): a judged part needs the template dataset's content, the
//! judge class and the generations' ids, which a node's loop does not read yet.

use std::sync::Arc;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PalwFpJobTailV1, PalwFpStopReasonV3, PalwFreePromptCommitmentV3,
    PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_gen_step_v1::{
    PalwGenStepSpaceV1, palw_gen_stage_root_v1, palw_gen_step_leaf_hash_v1, palw_gen_step_root_v1,
};
use kaspa_consensus_core::palw_gen_worker_v1::{
    PalwGenClaimRootsV1, PalwGenExecutionV1, palw_gen_execute_v1, palw_gen_replay_committed_v1,
};
use kaspa_consensus_core::palw_improve_composite_v1::PalwTirCompositeRefV1;
use kaspa_consensus_core::palw_improve_eval_court_v1::{
    PalwEvalClaimFactsV1 as PalwEvalCourtFactsV1, PalwEvalEvidenceV1, check_eval_cone_close_v1, check_eval_decode_close_v1,
    palw_eval_named_dissected_leaf_v1,
};
use kaspa_consensus_core::palw_improve_eval_v1::{
    PalwEvalBindingV1, PalwEvalClaimRootsV1, PalwEvalClaimTailV1, PalwEvalContextV1, PalwEvalJobV1, PalwEvalModeV1,
    PalwEvalStageParamsV1, PalwEvalSubjectClassV1, palw_fp_eval_claim_check_v1, palw_improve_answer_of_v1,
    palw_improve_eval_context_v1, palw_improve_eval_decode_config_v1, palw_improve_eval_layout_digest_v1,
    palw_improve_eval_pipeline_job_v1,
};
use kaspa_consensus_core::palw_improve_state_v1::PalwScoringKindV1;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_tir_artifact_v1::PalwTirTensorSourceV1;
use kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1;
use kaspa_consensus_core::palw_tir_step_v1::{palw_tir_lane_values_v1, palw_tir_lanes_le_v1};
use kaspa_consensus_core::tx::TransactionOutpoint;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{PipelineParams, PipelineRun, stage_job_facts};
use misaka_palw_tir::tensor::Tensor;
use misaka_palw_tir::{DType, TirProgramV1};
use misaka_palw_tir_exec::node::TirArtifactV1;

use crate::improve::PalwImproveEvalTaskV1;
use crate::lineage::PalwTirClassEntryV1;

// ---------------------------------------------------------------------------------------------
// The subject a node holds
// ---------------------------------------------------------------------------------------------

/// **The subject's weights as the interpreter reads them**: a held artifact's mapped tensors
/// (`PalwTirTensorSourceV1`, a composite candidate's params `0..p` from its parent's file), or a map.
enum PalwEvalWeightsV1 {
    Artifact { artifact: Arc<TirArtifactV1> },
    Map(MapParams),
}

/// **An IR class this node holds, as an evaluation subject**: its id, its registered `artifact_root`,
/// its program and layout (the class row's) and the weights that serve it.
pub struct PalwEvalHeldV1 {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub tokenizer_id: Hash64,
    pub program: TirProgramV1,
    /// The class's canonical program bytes, as the registry holds them (the court's facts carry the bytes, not the
    /// decoded program).
    pub program_bytes: Vec<u8>,
    pub layout: PalwTirLayoutV1,
    /// A composite candidate's reference (RFC-0004 §6.3), when the class is one: the evaluation court's parameter
    /// openings are then under the two sections' own roots.
    pub composite: Option<PalwTirCompositeRefV1>,
    weights: PalwEvalWeightsV1,
}

impl PalwEvalHeldV1 {
    /// **A held IR class** (`TirLineageV1`'s entry): the class's program decoded, its layout and the
    /// mapped artifact serving the params.
    pub fn from_entry(entry: &PalwTirClassEntryV1) -> Result<Self, String> {
        let program = TirProgramV1::decode_canonical(&entry.class.program).map_err(|e| format!("the held class's program: {e}"))?;
        Ok(Self {
            class_id: entry.class_id(),
            artifact_root: entry.artifact_root,
            tokenizer_id: entry.class.tokenizer_id,
            program,
            program_bytes: entry.class.program.clone(),
            layout: entry.class.layout.clone(),
            composite: entry.artifact.composite_ref().copied(),
            weights: PalwEvalWeightsV1::Artifact { artifact: entry.artifact.clone() },
        })
    }

    /// **A subject over in-memory weights** — tools and tests.
    pub fn from_map(
        class_id: Hash64,
        artifact_root: Hash64,
        tokenizer_id: Hash64,
        program: TirProgramV1,
        layout: PalwTirLayoutV1,
        params: MapParams,
    ) -> Self {
        let program_bytes = program.encode();
        Self {
            class_id,
            artifact_root,
            tokenizer_id,
            program,
            program_bytes,
            layout,
            composite: None,
            weights: PalwEvalWeightsV1::Map(params),
        }
    }

    fn subject(&self) -> PalwEvalSubjectClassV1<'_> {
        PalwEvalSubjectClassV1 {
            class_id: self.class_id,
            artifact_root: self.artifact_root,
            program: &self.program,
            layout: &self.layout,
        }
    }

    /// The chain's context for `job` on this subject (A6).
    pub fn context(&self, job: &PalwEvalJobV1, params: PalwEvalStageParamsV1) -> Result<PalwEvalContextV1, String> {
        palw_improve_eval_context_v1(job, &self.subject(), params).map_err(|e| e.to_string())
    }
}

/// A held artifact's params as the interpreter's `ParamSource` (an owned tensor per request: the
/// reference interpreter's door; the typed backend serves the attempt lane).
struct ArtifactParamsV1<'a> {
    artifact: &'a TirArtifactV1,
    program: &'a TirProgramV1,
}

impl ParamSource for ArtifactParamsV1<'_> {
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        let bytes = self.artifact.tensor_bytes(index, layer)?;
        let decl = self.program.params.get(index as usize)?;
        let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
        Tensor::from_le_bytes(decl.dtype, &shape, &bytes).ok()
    }
}

/// **The evaluation pipeline's params**: the subject's for program 0, none for the scoring stages
/// (weightless: an evaluation pipeline's root is its subject class's `artifact_root`).
struct PalwEvalParamsV1<'a> {
    subject: &'a dyn ParamSource,
    none: MapParams,
}

impl PipelineParams for PalwEvalParamsV1<'_> {
    fn params(&self, program: u16) -> &dyn ParamSource {
        if program == 0 { self.subject } else { &self.none }
    }
}

/// Run `f` over the pipeline params of `held`.
fn with_params<R>(held: &PalwEvalHeldV1, f: impl FnOnce(&dyn PipelineParams) -> R) -> R {
    match &held.weights {
        PalwEvalWeightsV1::Artifact { artifact } => {
            let source = ArtifactParamsV1 { artifact, program: &held.program };
            f(&PalwEvalParamsV1 { subject: &source, none: MapParams::default() })
        }
        PalwEvalWeightsV1::Map(map) => f(&PalwEvalParamsV1 { subject: map, none: MapParams::default() }),
    }
}

// ---------------------------------------------------------------------------------------------
// A run
// ---------------------------------------------------------------------------------------------

/// **What the pipeline commits besides its leaves**: the score lanes a kind commits (none for
/// ExactMatch: the fold scores its generation at the key's reveal; `(hi, lo)` for RefLogLik).
fn score_lanes_of(kind: PalwScoringKindV1, output: &[i128]) -> Result<Vec<i32>, String> {
    match kind {
        PalwScoringKindV1::ExactMatch => Ok(Vec::new()),
        PalwScoringKindV1::RefLogLik | PalwScoringKindV1::Judge | PalwScoringKindV1::Pairwise => {
            output.iter().map(|v| i32::try_from(*v).map_err(|_| format!("a {kind:?} score lane is not an i32: {v}"))).collect()
        }
    }
}

/// **A run**: the task, the context the chain derives for it, the execution (the claim's roots and
/// every leaf's values, for the court) and the binding and tail the claim carries.
pub struct PalwEvalWorkV1 {
    pub task: PalwImproveEvalTaskV1,
    pub ctx: PalwEvalContextV1,
    pub execution: PalwGenExecutionV1,
    pub binding: PalwEvalBindingV1,
    pub tail: PalwEvalClaimTailV1,
}

impl PalwEvalWorkV1 {
    /// The roots the claim's commitment carries.
    pub fn roots(&self) -> PalwEvalClaimRootsV1 {
        self.binding.claim_roots()
    }

    /// The stream stage's ids: the decoded generation, or the given reference.
    pub fn generated(&self) -> &[u32] {
        &self.execution.claim.generated
    }

    /// A generating ExactMatch job's **answer**: the hash of the generation's answer span (what the
    /// fold's job row keeps at acceptance, `PalwEvalClaimRefV1::answer`), `None` when it has none
    /// or the job is not an ExactMatch one.
    pub fn answer(&self) -> Option<Hash64> {
        match self.task.params {
            PalwEvalStageParamsV1::ExactMatch { open, close } => palw_improve_answer_of_v1(self.generated(), open, close),
            _ => None,
        }
    }

    /// A RefLogLik job's committed score (Q24 nats), joined from `(hi, lo)`; `None` for the kinds that
    /// commit none.
    pub fn score_value(&self) -> Option<i64> {
        self.binding.score_value().ok()
    }
}

/// **Run an evaluation task on a held subject.** Generating: through the decode stage under the item's
/// seed (FP Job V4's greedy rules, the policy's budget and stops); teacher-forced: the stream's ids
/// given (the disclosed reference), nothing selected. The claim's roots either way.
pub fn palw_eval_run_v1(held: &PalwEvalHeldV1, task: &PalwImproveEvalTaskV1) -> Result<PalwEvalWorkV1, String> {
    if task.subject_class != held.class_id {
        return Err(format!("the task's subject is class {}, this node holds {}", task.subject_class, held.class_id));
    }
    let job = task.job();
    let ctx = held.context(&job, task.params)?;
    let execution = with_params(held, |params| {
        let given: &[u32] = match task.mode {
            PalwEvalModeV1::TeacherForced { .. } => &task.reference_ids,
            _ => &[],
        };
        let pjob = palw_improve_eval_pipeline_job_v1(&ctx, &task.prompt_ids, given, &[]);
        match &task.mode {
            PalwEvalModeV1::Generate { .. } => {
                let decode = ctx.decode.as_ref().ok_or_else(|| "a generating job's context carries its decode".to_string())?;
                palw_gen_execute_v1(&ctx.pipeline, &ctx.programs, &ctx.layouts, params, &pjob, decode, ctx.seed)
                    .map_err(|e| e.to_string())
            }
            PalwEvalModeV1::TeacherForced { .. } => {
                if given.is_empty() {
                    return Err("a teacher-forced job needs its disclosed reference".to_string());
                }
                palw_gen_replay_committed_v1(&ctx.pipeline, &ctx.programs, &ctx.layouts, params, &pjob, ctx.seed)
                    .map_err(|e| e.to_string())
            }
            PalwEvalModeV1::Judged { .. } => Err("this build runs no judged part (spec 17 §17.8.5)".to_string()),
        }
    })?;
    let score = score_lanes_of(task.kind, &execution.run.output.data)?;
    let (binding, tail) = eval_bind_v1(held, task, &execution, score);
    Ok(PalwEvalWorkV1 { task: task.clone(), ctx, execution, binding, tail })
}

/// **The binding and the tail a claim of this execution carries**: the roots over the job, the class, the
/// layout, the execution's own roots, the prompt, the parameters and the score. One construction for the
/// honest run and for a drill's lie (the lie is a different execution or a different score, bound the same way).
fn eval_bind_v1(
    held: &PalwEvalHeldV1,
    task: &PalwImproveEvalTaskV1,
    execution: &PalwGenExecutionV1,
    score: Vec<i32>,
) -> (PalwEvalBindingV1, PalwEvalClaimTailV1) {
    let binding = PalwEvalBindingV1::of(
        &task.job(),
        held.class_id,
        &held.layout,
        &execution.claim,
        execution.space.leaf_count(),
        &task.prompt_ids,
        task.params,
        Vec::new(),
        score.clone(),
    );
    let tail = PalwEvalClaimTailV1 {
        generated: execution.claim.generated.clone(),
        score,
        subject_layout: held.layout.clone(),
        params: task.params,
        // Neither a judged part's reading nor a suite entry's opening: this node's tasks are a hold-out case's or a
        // setter item's.
        read: None,
        opening: None,
    };
    (binding, tail)
}

// ---------------------------------------------------------------------------------------------
// A lying executor, and what an honest replay finds of it (RFC-0004 drill D-M3)
// ---------------------------------------------------------------------------------------------

/// **A drill's lie**: what a deliberately faulty executor does to an evaluation it ran, before it files the
/// claim. The claim it files is self-consistent — its roots bind what it committed, so the chain's own door
/// (`palw_fp_eval_claim_check_v1`) takes it — and it parts from the honest run exactly where the lie is. Drill
/// binaries only (`--palw-drill-tamper-eval`); a real executor has no reason to hold this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwEvalFaultV1 {
    /// One lane of one leaf of the step tree (index in the claim's one leaf order) is moved: the leaf's hash, its
    /// stage's root, the step root and the roots over them change; the ids and the score stay the honest run's.
    Leaf(u64),
    /// The first generated id is another id than the decode rule selects from the committed logits: every leaf is
    /// the honest run's, the ids and the output root are not.
    Output,
    /// The committed score's low lane is moved: every leaf and every id is the honest run's, the score is not what
    /// the scoring stages' leaves commit (a likelihood job; an ExactMatch job commits no score).
    Score,
}

impl PalwEvalFaultV1 {
    /// `leaf:N`, `output` or `score`.
    pub fn parse(spec: &str) -> Result<Self, String> {
        match spec.trim() {
            "output" => Ok(Self::Output),
            "score" => Ok(Self::Score),
            other => match other.strip_prefix("leaf:") {
                Some(n) => n.parse::<u64>().map(Self::Leaf).map_err(|_| format!("`{n}` is not a leaf index")),
                None => Err(format!("`{other}` is not a fault: leaf:<index>, output or score")),
            },
        }
    }

    /// The spec [`Self::parse`] reads back.
    pub fn describe(&self) -> String {
        match self {
            Self::Leaf(n) => format!("leaf:{n}"),
            Self::Output => "output".to_string(),
            Self::Score => "score".to_string(),
        }
    }
}

/// **Run an evaluation task and lie about it** ([`PalwEvalFaultV1`]): the honest run, then the fault applied to
/// its execution and the claim's binding rebuilt over the lie. The result is what [`palw_eval_claim_v1`] files and
/// what [`PalwEvalCaptureV1::of`] captures (the lie's leaves, as the accused committed them).
pub fn palw_eval_run_faulted_v1(
    held: &PalwEvalHeldV1,
    task: &PalwImproveEvalTaskV1,
    fault: PalwEvalFaultV1,
) -> Result<PalwEvalWorkV1, String> {
    let mut work = palw_eval_run_v1(held, task)?;
    let mut score = work.tail.score.clone();
    match fault {
        PalwEvalFaultV1::Leaf(index) => {
            let total = work.execution.space.leaf_count();
            let (stage, local) =
                work.execution.space.locate(index).ok_or_else(|| format!("leaf {index} is past the run's {total} leaves"))?;
            let (s, l) = (stage as usize, local as usize);
            let mut values = work.execution.leaf_values[s][l].clone();
            *values.first_mut().ok_or_else(|| format!("leaf {index} holds no value"))? ^= 1;
            let hash = palw_gen_step_leaf_hash_v1(&work.execution.space.stages[s].leaves()[l], &values).map_err(|e| e.to_string())?;
            work.execution.leaf_values[s][l] = values;
            work.execution.leaf_hashes[s][l] = hash;
            work.execution.claim.stage_roots[s] = palw_gen_stage_root_v1(stage, &work.execution.leaf_hashes[s]);
            work.execution.claim.step_root = palw_gen_step_root_v1(&work.execution.claim.stage_roots);
        }
        PalwEvalFaultV1::Output => {
            let first = work.execution.claim.generated.first_mut().ok_or("the run generated no id to move")?;
            *first ^= 1;
        }
        PalwEvalFaultV1::Score => {
            *score.last_mut().ok_or("this job commits no score to move (an ExactMatch job's is the fold's)")? ^= 1;
        }
    }
    let (binding, tail) = eval_bind_v1(held, task, &work.execution, score);
    work.binding = binding;
    work.tail = tail;
    Ok(work)
}

/// **Dispute an accused evaluation by the roots its claim committed and the capture served for it.** A served
/// capture is the accused's word: it must rebuild to exactly the roots the chain holds for the claim (the trace,
/// the ids' output root, the execution root and the leaf count), or it is not the accused's and disputes nothing.
/// Then [`palw_eval_dispute_v1`].
pub fn palw_eval_dispute_committed_v1(
    held: &PalwEvalHeldV1,
    task: &PalwImproveEvalTaskV1,
    committed: &PalwEvalCommittedRootsV1,
    accused: &PalwEvalCaptureV1,
) -> Result<PalwEvalDisputeV1, String> {
    let (_, binding, _) = accused.rebuild(held)?;
    let roots = binding.claim_roots();
    if (roots.trace_root, roots.output_root, roots.execution_root, roots.work_leaves)
        != (committed.trace_root, committed.output_root, committed.execution_root, committed.work_leaves)
    {
        return Err("the served capture does not rebuild to the roots the claim committed: it is not the accused's".to_string());
    }
    palw_eval_dispute_v1(held, task, accused)
}

/// **What an honest replay finds of an accused evaluation's captured commitments** — where they part from the
/// honest run of the same job, in the order the court asks the questions: the step tree first (the cone of the
/// first leaf that differs), then the ids (the decode of the committed logits), then the score.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwEvalDisputeV1 {
    /// The accused's capture rebuilds to the honest run's every commitment: nothing to dispute.
    Agrees,
    /// The first leaf, in the claim's one order, where the accused's step tree parts from the honest run:
    /// `index` in that order, at `(stage, local)`.
    Leaf { index: u64, stage: u8, local: u64 },
    /// The step tree is the honest run's but the ids are not: the first position whose id differs.
    Output { position: u32, committed: u32, honest: u32 },
    /// Tree and ids are the honest run's but the committed score is not.
    Score { committed: Vec<i32>, honest: Vec<i32> },
}

impl PalwEvalDisputeV1 {
    /// A short line for a log and a status file (no prompt, no ids beyond the one that differs).
    pub fn describe(&self) -> String {
        match self {
            Self::Agrees => "agrees".to_string(),
            Self::Leaf { index, stage, local } => format!("leaf {index} (stage {stage}, leaf {local}) of the step tree"),
            Self::Output { position, committed, honest } => format!("generated id {position}: committed {committed}, honest {honest}"),
            Self::Score { committed, honest } => format!("score lanes: committed {committed:?}, honest {honest:?}"),
        }
    }

    /// The kind's name for a status file.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Agrees => "agrees",
            Self::Leaf { .. } => "leaf",
            Self::Output { .. } => "output",
            Self::Score { .. } => "score",
        }
    }
}

/// **Dispute an accused evaluation from its capture**: the honest run of the task, the accused's capture rebuilt
/// to the commitments it describes (lies included), and where the two part ([`PalwEvalDisputeV1`]). The capture
/// must be of this very job — its job, subject class, prompt and parameters are the task's — or it is no
/// evidence of it.
pub fn palw_eval_dispute_v1(
    held: &PalwEvalHeldV1,
    task: &PalwImproveEvalTaskV1,
    accused: &PalwEvalCaptureV1,
) -> Result<PalwEvalDisputeV1, String> {
    if accused.job != task.job()
        || accused.subject_class != task.subject_class
        || accused.prompt != task.prompt_ids
        || accused.params != task.params
    {
        return Err("the capture is not of this job".to_string());
    }
    let honest = palw_eval_run_v1(held, task)?;
    let (_, _, hashes) = accused.rebuild(held)?;
    if let Some(index) = palw_eval_first_divergence_v1(&hashes, &honest.execution) {
        let (stage, local) = honest.execution.space.locate(index).ok_or_else(|| format!("leaf {index} is past the honest run"))?;
        return Ok(PalwEvalDisputeV1::Leaf { index, stage, local });
    }
    let honest_ids = honest.generated();
    if accused.generated != honest_ids {
        let position = accused
            .generated
            .iter()
            .zip(honest_ids)
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| accused.generated.len().min(honest_ids.len()));
        let at = |ids: &[u32]| ids.get(position).copied().unwrap_or(u32::MAX);
        return Ok(PalwEvalDisputeV1::Output { position: position as u32, committed: at(&accused.generated), honest: at(honest_ids) });
    }
    if accused.score != honest.tail.score {
        return Ok(PalwEvalDisputeV1::Score { committed: accused.score.clone(), honest: honest.tail.score.clone() });
    }
    Ok(PalwEvalDisputeV1::Agrees)
}

// ---------------------------------------------------------------------------------------------
// The claim
// ---------------------------------------------------------------------------------------------

/// **What the chain's view of the executor adds to a claim**: the network, its bond and key, the
/// operator the bond registered, a recent anchor (the lane's freshness rule), and the retention the
/// chain holds the trace's data-availability units for.
#[derive(Clone, Debug)]
pub struct PalwEvalClaimFactsV1 {
    pub network_domain: Hash64,
    pub executor_bond: TransactionOutpoint,
    pub executor_pubkey: Vec<u8>,
    pub operator_id: Hash64,
    pub anchor_block: Hash64,
    pub anchor_daa: u64,
    pub prompt_ids_form: PalwPromptIdsFormV1,
    pub trace_retention_daa: u64,
}

/// **An evaluation claim, before its signature**: the FP commitment whose job is version 9 with the
/// evaluation job as its tail, the payload tail (the stream's ids, the committed score, the layout and
/// the parameters), and the prompt the payload carries.
#[derive(Clone, Debug)]
pub struct PalwEvalClaimV1 {
    pub commitment: PalwFreePromptCommitmentV3,
    pub tail: PalwEvalClaimTailV1,
    pub prompt: Vec<u32>,
}

impl PalwEvalClaimV1 {
    /// The claim id (`fp_claim_id_v3`), which the executor signs: total over the commitment.
    pub fn claim_id(&self) -> Hash64 {
        kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&self.commitment)
    }
}

/// **The evaluation job as the FP lane carries it** (A6's shape rule, `palw_fp_eval_job_shape_v1`):
/// version 9, public with its prompt on the payload, greedy, a zero nonce, and the decode rules its
/// mode derives — a generating job's stops and budget; a teacher-forced job's no-op rules, its limit
/// the reference's length.
fn carried_job(work: &PalwEvalWorkV1, facts: &PalwEvalClaimFactsV1, tokenizer_id: Hash64) -> Result<PalwFreePromptJobV3, String> {
    let task = &work.task;
    let (limit, decode) = match &task.mode {
        PalwEvalModeV1::Generate { max_new, stop_ids, .. } => (*max_new, palw_improve_eval_decode_config_v1(stop_ids)),
        PalwEvalModeV1::TeacherForced { .. } => (task.reference_ids.len() as u32, DecodeConfigV4::NOOP),
        PalwEvalModeV1::Judged { .. } => return Err("this build runs no judged part (spec 17 §17.8.5)".to_string()),
    };
    let prompt_hash = prompt_token_ids_commitment_v1(facts.prompt_ids_form, &task.prompt_ids).map_err(|e| e.to_string())?;
    Ok(PalwFreePromptJobV3 {
        version: kaspa_consensus_core::palw_improve_eval_v1::PALW_FP_EVAL_VERSION,
        network_domain: facts.network_domain,
        class_id: task.subject_class,
        executor_bond: facts.executor_bond,
        executor_pubkey: facts.executor_pubkey.clone(),
        operator_id: facts.operator_id,
        anchor_block: facts.anchor_block,
        anchor_daa: facts.anchor_daa,
        job_nonce: [0u8; 32],
        tokenizer_id,
        prompt_token_ids_hash: prompt_hash,
        prompt_tokens: task.prompt_ids.len() as u32,
        decode_token_limit: limit,
        max_context_tokens: work.ctx.layouts.first().map_or(0, |l| l.max_context),
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: Some(decode),
        tail: Some(PalwFpJobTailV1::Eval(Box::new(task.job()))),
    })
}

/// **Assemble an evaluation claim from a run** and check it as the chain's extractor will
/// (`palw_fp_eval_claim_check_v1`: the job's shape, the tail against the commitment, every root) — so a
/// node never spends a fee on a claim the chain would refuse at its door.
pub fn palw_eval_claim_v1(
    work: &PalwEvalWorkV1,
    held: &PalwEvalHeldV1,
    facts: &PalwEvalClaimFactsV1,
) -> Result<PalwEvalClaimV1, String> {
    let job = carried_job(work, facts, held.tokenizer_id)?;
    let roots = work.roots();
    let executed = work.tail.generated.len() as u32;
    let at_limit = executed == job.decode_token_limit;
    let commitment = PalwFreePromptCommitmentV3 {
        job,
        trace_root: roots.trace_root,
        output_root: roots.output_root,
        schedule_root: Hash64::default(),
        execution_root: roots.execution_root,
        decode_tokens_executed: executed,
        stop_reason: if at_limit { PalwFpStopReasonV3::ExactBudgetReached } else { PalwFpStopReasonV3::EndOfGeneration },
        work_leaves: roots.work_leaves,
        trace_manifest_root: Hash64::default(),
        trace_chunk_count: 1,
        trace_retention_daa: facts.trace_retention_daa,
    };
    let claim = PalwEvalClaimV1 { commitment, tail: work.tail.clone(), prompt: work.task.prompt_ids.clone() };
    palw_fp_eval_claim_check_v1(&claim.commitment, &claim.prompt, &claim.tail)
        .map_err(|e| format!("the claim does not check: {e}"))?;
    Ok(claim)
}

// ---------------------------------------------------------------------------------------------
// The seat's replay
// ---------------------------------------------------------------------------------------------

/// **A seat's judgment of an evaluation claim.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwEvalSeatJudgmentV1 {
    /// The replay's roots are the claim's, and so are its ids and score.
    Valid,
    /// The replay differs: the court's question (a seat files nothing on it).
    Differs(String),
    /// The material cannot be judged (it is not the claim's job, or the claim does not check).
    Unjudgeable(String),
}

/// **The task a claim's own job and tail describe** — what a seat replays: the job's mode, the prompt
/// the payload carries, a teacher-forced job's reference (the tail's ids), and the tail's parameters.
pub fn palw_eval_task_of_claim_v1(
    commitment: &PalwFreePromptCommitmentV3,
    prompt: &[u32],
    tail: &PalwEvalClaimTailV1,
) -> Option<PalwImproveEvalTaskV1> {
    let Some(PalwFpJobTailV1::Eval(job)) = &commitment.job.tail else { return None };
    let reference_ids = match job.mode {
        PalwEvalModeV1::TeacherForced { .. } => tail.generated.clone(),
        _ => Vec::new(),
    };
    Some(PalwImproveEvalTaskV1 {
        line_id: job.line_id,
        epoch: job.epoch,
        item: job.item,
        subject: job.subject,
        subject_class: commitment.job.class_id,
        kind: job.kind,
        mode: job.mode.clone(),
        job_id: job.id(),
        prompt_ids: prompt.to_vec(),
        reference_ids,
        params: tail.params,
    })
}

/// **Judge an evaluation claim** from the material the chain carries (the commitment, the payload's
/// prompt and tail): the claim is checked as the extractor checks it, the job replayed on this seat's
/// own weights, and the replay's roots, ids and score held to the claim's. A seat that holds another
/// class cannot judge it.
pub fn palw_eval_seat_judge_v1(
    held: &PalwEvalHeldV1,
    commitment: &PalwFreePromptCommitmentV3,
    prompt: &[u32],
    tail: &PalwEvalClaimTailV1,
) -> PalwEvalSeatJudgmentV1 {
    use PalwEvalSeatJudgmentV1 as J;
    if let Err(e) = palw_fp_eval_claim_check_v1(commitment, prompt, tail) {
        return J::Unjudgeable(format!("the claim does not check: {e}"));
    }
    let Some(task) = palw_eval_task_of_claim_v1(commitment, prompt, tail) else {
        return J::Unjudgeable("the commitment's job is not an evaluation job".into());
    };
    if commitment.job.class_id != held.class_id {
        return J::Unjudgeable(format!("this seat holds class {}, the claim is of {}", held.class_id, commitment.job.class_id));
    }
    if tail.subject_layout != held.layout {
        return J::Unjudgeable("the claim's layout is not the held class's".into());
    }
    let replay = match palw_eval_run_v1(held, &task) {
        Ok(replay) => replay,
        Err(why) => return J::Unjudgeable(format!("the replay cannot run: {why}")),
    };
    let roots = replay.roots();
    if replay.tail.generated != tail.generated {
        let first = replay.tail.generated.iter().zip(&tail.generated).position(|(a, b)| a != b);
        return J::Differs(format!(
            "the replay's ids differ from the claim's at {}",
            first.map_or_else(|| "their length".to_string(), |i| format!("position {i}"))
        ));
    }
    if replay.tail.score != tail.score {
        return J::Differs(format!("the replay scores {:?}, the claim {:?}", replay.tail.score, tail.score));
    }
    if roots.execution_root != commitment.execution_root
        || roots.trace_root != commitment.trace_root
        || roots.output_root != commitment.output_root
        || roots.work_leaves != commitment.work_leaves
    {
        return J::Differs(format!(
            "the replay's execution root {} is not the claim's {}",
            roots.execution_root, commitment.execution_root
        ));
    }
    J::Valid
}

/// **The roots an evaluation claim committed, as the chain's state holds them** (the claim row): what a
/// seat that derives the task from the state — the job, the item's prompt and reference, the policy's
/// parameters — holds its replay to, with no payload at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEvalCommittedRootsV1 {
    pub trace_root: Hash64,
    pub output_root: Hash64,
    pub execution_root: Hash64,
    pub work_leaves: u64,
}

/// **Judge an evaluation claim from the chain's state alone**: the task the state derives for the
/// claim's job, replayed on this seat's own weights, its roots held to the claim row's. The execution
/// root binds the ids, the score, the prompt, the parameters and the layout, so a replay that reproduces
/// all four roots reproduces the claim; a replay that does not is the court's question, never a seat's
/// accusation.
pub fn palw_eval_seat_judge_roots_v1(
    held: &PalwEvalHeldV1,
    task: &PalwImproveEvalTaskV1,
    claim: &PalwEvalCommittedRootsV1,
) -> PalwEvalSeatJudgmentV1 {
    use PalwEvalSeatJudgmentV1 as J;
    if task.subject_class != held.class_id {
        return J::Unjudgeable(format!("this seat holds class {}, the claim is of {}", held.class_id, task.subject_class));
    }
    let replay = match palw_eval_run_v1(held, task) {
        Ok(replay) => replay,
        Err(why) => return J::Unjudgeable(format!("the replay cannot run: {why}")),
    };
    let roots = replay.roots();
    if roots.work_leaves != claim.work_leaves {
        return J::Differs(format!("the replay's work is {} leaves, the claim's {}", roots.work_leaves, claim.work_leaves));
    }
    if roots.output_root != claim.output_root {
        return J::Differs("the replay's ids are not the claim's (output root)".into());
    }
    if roots.trace_root != claim.trace_root {
        return J::Differs("the replay's step tree is not the claim's (trace root)".into());
    }
    if roots.execution_root != claim.execution_root {
        return J::Differs(format!(
            "the replay's execution root {} is not the claim's {}",
            roots.execution_root, claim.execution_root
        ));
    }
    J::Valid
}

// ---------------------------------------------------------------------------------------------
// The capture and the divergence
// ---------------------------------------------------------------------------------------------

/// **A run's capture** — its inputs and every leaf it committed (as 4-byte lanes), in the claim's one
/// order: what data availability serves and what rebuilds the accused's execution, lies included.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwEvalCaptureV1 {
    pub job: PalwEvalJobV1,
    pub subject_class: Hash64,
    pub layout: PalwTirLayoutV1,
    pub params: PalwEvalStageParamsV1,
    pub prompt: Vec<u32>,
    pub generated: Vec<u32>,
    /// The score lanes the claim committed (public on its payload).
    pub score: Vec<i32>,
    /// Per stage, every leaf's lanes, in leaf order.
    pub leaves: Vec<Vec<Vec<u8>>>,
}

impl PalwEvalCaptureV1 {
    /// A run's capture.
    pub fn of(work: &PalwEvalWorkV1, held: &PalwEvalHeldV1) -> Result<Self, String> {
        let mut leaves = Vec::with_capacity(work.execution.space.stages.len());
        for (stage, values) in work.execution.space.stages.iter().zip(&work.execution.leaf_values) {
            let mut lanes = Vec::with_capacity(values.len());
            for (leaf, v) in stage.leaves().iter().zip(values) {
                lanes.push(palw_tir_lanes_le_v1(leaf.dtype, v).map_err(|e| e.to_string())?);
            }
            leaves.push(lanes);
        }
        Ok(Self {
            job: work.task.job(),
            subject_class: held.class_id,
            layout: held.layout.clone(),
            params: work.task.params,
            prompt: work.task.prompt_ids.clone(),
            generated: work.execution.claim.generated.clone(),
            score: work.tail.score.clone(),
            leaves,
        })
    }

    /// **The accused's execution as its capture commits it**: the space from the job's facts, every
    /// leaf's hash and every stage's root from the captured lanes, and the binding over them — the
    /// execution the captured commitments describe, whatever computed them. The roots a claim built
    /// from a lying capture carries are what a challenger's honest run is compared with.
    pub fn rebuild(&self, held: &PalwEvalHeldV1) -> Result<(PalwGenClaimRootsV1, PalwEvalBindingV1, Vec<Vec<Hash64>>), String> {
        let parts = self.rebuild_parts(held)?;
        Ok((parts.claim, parts.binding, parts.hashes))
    }

    /// **The accused's execution, whole** ([`Self::rebuild`] with every leaf's values kept): what the evaluation
    /// court's builders open leaves from (`PalwEvalEvidenceV1::execution`) — each leaf with its path under the
    /// roots the accused committed, lies included. The run's own tensors are not captured (the court opens leaves,
    /// never the run), so that field is an empty placeholder. Returns the binding the roots produce beside it.
    pub fn rebuild_execution(&self, held: &PalwEvalHeldV1) -> Result<(PalwGenExecutionV1, PalwEvalBindingV1), String> {
        let parts = self.rebuild_parts(held)?;
        let run = PipelineRun { stages: Vec::new(), output: Tensor::zeros(DType::I32, &[0]) };
        let execution = PalwGenExecutionV1 {
            run,
            stop: None,
            space: parts.space,
            leaf_values: parts.values,
            leaf_hashes: parts.hashes,
            claim: parts.claim,
            // An evaluation's execution has no tensor output (RFC-0003's tensor claims carry one).
            output: None,
        };
        Ok((execution, parts.binding))
    }

    fn rebuild_parts(&self, held: &PalwEvalHeldV1) -> Result<PalwEvalRebuiltV1, String> {
        let ctx = held.context(&self.job, self.params)?;
        let pjob = palw_improve_eval_pipeline_job_v1(&ctx, &self.prompt, &self.generated, &[]);
        let facts = stage_job_facts(&ctx.pipeline, &ctx.programs, &pjob).map_err(|e| e.to_string())?;
        let trips: Vec<u32> = facts.iter().map(|f| f.trip).collect();
        let space = PalwGenStepSpaceV1::new(&ctx.pipeline, &ctx.programs, &ctx.layouts, &trips, self.prompt.len() as u32)
            .map_err(|e| e.to_string())?;
        if self.leaves.len() != space.stages.len() {
            return Err("one leaf list per stage".into());
        }
        let mut leaf_values = Vec::with_capacity(space.stages.len());
        let mut leaf_hashes = Vec::with_capacity(space.stages.len());
        for (s, (stage, lanes)) in space.stages.iter().zip(&self.leaves).enumerate() {
            if lanes.len() != stage.leaves().len() {
                return Err(format!("stage {s}: {} leaves captured for {}", lanes.len(), stage.leaves().len()));
            }
            let mut values = Vec::with_capacity(lanes.len());
            let mut hashes = Vec::with_capacity(lanes.len());
            for (leaf, bytes) in stage.leaves().iter().zip(lanes) {
                let v = palw_tir_lane_values_v1(leaf.dtype, bytes).map_err(|e| e.to_string())?;
                if v.len() != leaf.value_count as usize {
                    return Err(format!("{:?}: {} lanes for {}", leaf.coord, v.len(), leaf.value_count));
                }
                hashes.push(palw_gen_step_leaf_hash_v1(leaf, &v).map_err(|e| e.to_string())?);
                values.push(v);
            }
            leaf_values.push(values);
            leaf_hashes.push(hashes);
        }
        let stage_roots: Vec<Hash64> = leaf_hashes.iter().enumerate().map(|(s, h)| palw_gen_stage_root_v1(s as u8, h)).collect();
        // An evaluation claim is a text claim: its output is its generated ids, so no tensor output digest.
        let claim = PalwGenClaimRootsV1 {
            step_root: palw_gen_step_root_v1(&stage_roots),
            stage_roots,
            generated: self.generated.clone(),
            output_root: None,
        };
        let binding = PalwEvalBindingV1::of(
            &self.job,
            self.subject_class,
            &self.layout,
            &claim,
            space.leaf_count(),
            &self.prompt,
            self.params,
            Vec::new(),
            self.score.clone(),
        );
        Ok(PalwEvalRebuiltV1 { space, values: leaf_values, hashes: leaf_hashes, claim, binding })
    }
}

/// What rebuilding a capture produces: the step space of the job, every leaf's values and hash, the roots the
/// hashes give and the binding over them.
struct PalwEvalRebuiltV1 {
    space: PalwGenStepSpaceV1,
    values: Vec<Vec<Vec<i128>>>,
    hashes: Vec<Vec<Hash64>>,
    claim: PalwGenClaimRootsV1,
    binding: PalwEvalBindingV1,
}

/// **The first leaf, in the claim's one order, where the accused's commitments part from an honest run
/// of the same job** — the leaf a challenger disputes (the bisection's destination).
pub fn palw_eval_first_divergence_v1(accused: &[Vec<Hash64>], honest: &PalwGenExecutionV1) -> Option<u64> {
    let mut before = 0u64;
    for (a, o) in accused.iter().zip(&honest.leaf_hashes) {
        if let Some(i) = a.iter().zip(o).position(|(x, y)| x != y) {
            return Some(before + i as u64);
        }
        before += a.len() as u64;
    }
    None
}

// ---------------------------------------------------------------------------------------------
// The court: the close that convicts an accused evaluation (spec 17 §17.8.6)
// ---------------------------------------------------------------------------------------------

/// **An accusation's proof, built from the accused's capture** — what a challenger files against an evaluation claim
/// whose replay differs: the evaluation court's close at the place [`palw_eval_dispute_v1`] found the lie, checked by
/// the court's own functions before it is offered (so a node never files an accusation the chain would refuse or, worse,
/// acquit — an accusation that does not convict charges its accuser).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwEvalFilingV1 {
    /// The close's name for a log: `named leaf` (a dissected leaf's challenge), `cone`, `decode token` or `score`.
    pub label: &'static str,
    /// The proof the one-move accusation carries: `EvalCone` (court proof tag 13) or `EvalDecodeToken` (tag 14).
    pub proof: PalwCourtVerdictProofV2,
    /// The leaf the proof names (in the claim's one order), where it names one.
    pub leaf: Option<u64>,
    /// The leaf is DISSECTED — its cone reduces over the history — and under the held regime such a leaf is never
    /// tried in one move: the accusation is only the challenge, the chain opens a dissection session at the leaf and the
    /// responder's root claim is its first move. This build files that challenge and plays no further move of the session
    /// (its answer is the clock's: an executor that cannot finalize its own lie files none).
    pub opens_dissection: bool,
}

/// **Build the proof that convicts an accused evaluation** from the accused's capture and where an honest replay found it
/// parts from the honest run ([`PalwEvalDisputeV1`]):
///
/// * a divergent step leaf — under the held regime (`held_regime`), a DISSECTED leaf is named only
///   ([`PalwEvalFilingV1::opens_dissection`]); any other leaf is closed whole: its cone, every unit it reads and the
///   subject's parameter leaves it reads, opened under the roots the accused committed (`EvalCone`, tag 13);
/// * a committed id that is not the decode rule's selection from its committed logits row (`EvalDecodeToken`,
///   `Token`, tag 14);
/// * a committed score that is not the score stage's committed output tile (`EvalDecodeToken`, `Score`).
///
/// The capture is held to the roots the claim committed (`committed`, the chain's own) and must be of the task's job, as
/// for [`palw_eval_dispute_committed_v1`]; the close is then checked as the chain checks it (the evaluation court's
/// `check_eval_cone_close_v1` / `check_eval_decode_close_v1` over the claim's facts) and offered only when it convicts.
/// `Err` names why there is nothing to file.
pub fn palw_eval_court_filing_v1(
    held: &PalwEvalHeldV1,
    task: &PalwImproveEvalTaskV1,
    committed: &PalwEvalCommittedRootsV1,
    accused: &PalwEvalCaptureV1,
    found: &PalwEvalDisputeV1,
    limits: &DemandLimits,
    held_regime: bool,
) -> Result<PalwEvalFilingV1, String> {
    let job = task.job();
    if accused.job != job
        || accused.subject_class != task.subject_class
        || accused.prompt != task.prompt_ids
        || accused.params != task.params
    {
        return Err("the capture is not of this job".to_string());
    }
    if task.subject_class != held.class_id {
        return Err(format!("the task's subject is class {}, this node holds {}", task.subject_class, held.class_id));
    }
    let (execution, binding) = accused.rebuild_execution(held)?;
    let roots = binding.claim_roots();
    if (roots.trace_root, roots.output_root, roots.execution_root, roots.work_leaves)
        != (committed.trace_root, committed.output_root, committed.execution_root, committed.work_leaves)
    {
        return Err("the served capture does not rebuild to the roots the claim committed: it is not the accused's".to_string());
    }
    let facts = PalwEvalCourtFactsV1 {
        class_id: &held.class_id,
        execution_root: &committed.execution_root,
        trace_root: &committed.trace_root,
        output_root: &committed.output_root,
        work_leaves: committed.work_leaves,
        job: &job,
        program: &held.program_bytes,
        layout_digest: palw_improve_eval_layout_digest_v1(&held.layout),
        artifact_root: held.artifact_root,
    };
    with_params(held, |params| {
        let evidence = PalwEvalEvidenceV1 {
            facts,
            params,
            execution: &execution,
            binding: &binding,
            prompt: &task.prompt_ids,
            composite: held.composite,
        };
        match found {
            PalwEvalDisputeV1::Agrees => Err("the served capture agrees with the honest run: nothing to file".to_string()),
            PalwEvalDisputeV1::Leaf { index, .. } => {
                if held_regime {
                    let named = evidence.named_leaf_close(*index)?;
                    let dissected = palw_eval_named_dissected_leaf_v1(&named, &facts)
                        .map_err(|why| format!("the named leaf does not verify against the claim: {why}"))?;
                    if let Some(leaf) = dissected {
                        return Ok(PalwEvalFilingV1 {
                            label: "named leaf",
                            proof: PalwCourtVerdictProofV2::EvalCone { close: Box::new(named) },
                            leaf: Some(leaf),
                            opens_dissection: true,
                        });
                    }
                }
                let close = evidence.cone_close(*index, limits)?;
                match check_eval_cone_close_v1(&close, &facts, None, limits) {
                    Ok(Some(_fault)) => Ok(PalwEvalFilingV1 {
                        label: "cone",
                        proof: PalwCourtVerdictProofV2::EvalCone { close: Box::new(close) },
                        leaf: Some(*index),
                        opens_dissection: false,
                    }),
                    Ok(None) => Err(format!("the court acquits leaf {index} over the accused's own operands: no conviction to file")),
                    Err(why) => Err(format!("the cone close of leaf {index} does not adjudicate: {why}")),
                }
            }
            PalwEvalDisputeV1::Output { position, .. } => {
                let close = evidence.decode_close(*position)?;
                match check_eval_decode_close_v1(&close, &facts, None) {
                    Ok(Some(_fault)) => Ok(PalwEvalFilingV1 {
                        label: "decode token",
                        proof: PalwCourtVerdictProofV2::EvalDecodeToken { close: Box::new(close) },
                        leaf: None,
                        opens_dissection: false,
                    }),
                    Ok(None) => {
                        Err(format!("the court acquits generated id {position} against its committed row: no conviction to file"))
                    }
                    Err(why) => Err(format!("the decode close of generated id {position} does not adjudicate: {why}")),
                }
            }
            PalwEvalDisputeV1::Score { .. } => {
                let close = evidence.score_close()?;
                match check_eval_decode_close_v1(&close, &facts, None) {
                    Ok(Some(_fault)) => Ok(PalwEvalFilingV1 {
                        label: "score",
                        proof: PalwCourtVerdictProofV2::EvalDecodeToken { close: Box::new(close) },
                        leaf: None,
                        opens_dissection: false,
                    }),
                    Ok(None) => {
                        Err("the court acquits the committed score against the output tile: no conviction to file".to_string())
                    }
                    Err(why) => Err(format!("the score close does not adjudicate: {why}")),
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_improve_state_v1::PalwEvalSubjectV1;
    use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_root_v1;
    use kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1;
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::scoring::ref_loglik_reference_v1;
    use misaka_palw_tir::{DType, Ref, TensorType};
    use std::borrow::Cow;

    /// A toy IR class's program (A6's `palw_improve_eval` fixture): an `i8` embedding, two layers of a
    /// per-layer `i8 [4, 4]` matrix and `i64` multiplier, and an `i16` head over 16 ids.
    fn subject_program() -> TirProgramV1 {
        let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
        let table = pb.param("embed.table", DType::I8, &[16, 4], false);
        let w = pb.param("blk.w", DType::I8, &[4, 4], true);
        let m = pb.param("blk.m", DType::I64, &[4], true);
        let head = pb.param("head.w", DType::I16, &[16, 4], false);
        let carry = vec![TensorType::fixed(DType::I32, &[4])];
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let row = b.gather(table, Ref::Input(0), 0, 0);
            let row = b.cast(row, DType::I32);
            b.finish(&[row])
        };
        let layer = {
            let mut b = pb.block("layer", carry.clone());
            let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
            let acc = b.matmul(w, x, DType::I64);
            let acc = b.reshape_fixed(acc, &[4]);
            let y = b.mul(acc, m, DType::I128);
            let y = b.shr(y, 20, misaka_palw_tir::Rounding::HalfAwayFromZero, DType::I128);
            let y = b.clamp(y, -30_000, 30_000, DType::I32);
            b.finish(&[y])
        };
        let post = {
            let mut b = pb.block("post", carry);
            let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
            let l = b.matmul(head, x, DType::I64);
            let l = b.reshape_fixed(l, &[16]);
            let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
            b.commit(l);
            b.finish(&[])
        };
        let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
        let mut p = pb.finish(pre, vec![layer, layer], post, logits);
        p.logits_scheme_id.copy_from_slice(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
        p
    }

    fn subject_params(p: &TirProgramV1, salt: usize) -> MapParams {
        let mut out = MapParams::default();
        for (j, inst) in kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_param_instances_v1(p).into_iter().enumerate() {
            let d = &p.params[j];
            for l in inst {
                let n: usize = d.shape.iter().map(|x| *x as usize).product();
                let data: Vec<i128> = (0..n)
                    .map(|i| {
                        let v = ((i * 37 + j * 11 + salt * 13 + l.map_or(0, |l| l as usize) * 5) % 200) as i128 - 100;
                        if d.dtype == DType::I64 { v.abs() * 9_000 + 1 } else { v }
                    })
                    .collect();
                out.tensors.insert((j as u16, l), Tensor::new(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), data).unwrap());
            }
        }
        out
    }

    struct Src<'a>(&'a MapParams);
    impl PalwTirTensorSourceV1 for Src<'_> {
        fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
            self.0.tensors.get(&(param, layer)).map(|t| Cow::Owned(t.to_le_bytes()))
        }
    }

    fn layout_of(p: &TirProgramV1) -> PalwTirLayoutV1 {
        let commits = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
        PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 12,
            checkpoint_interval: 1,
            h_tile: 16,
            commit_tiles: vec![4; commits],
            state_tiles: vec![4; p.states.len()],
        }
    }

    fn held(salt: usize) -> PalwEvalHeldV1 {
        let program = subject_program();
        let params = subject_params(&program, salt);
        let layout = layout_of(&program);
        let (root, _) = palw_tir_inventory_root_v1(&program, &Src(&params)).unwrap();
        PalwEvalHeldV1::from_map(Hash64::from_bytes([0x22; 64]), root, Hash64::from_bytes([0x14; 64]), program, layout, params)
    }

    fn task(
        held: &PalwEvalHeldV1,
        kind: PalwScoringKindV1,
        mode: PalwEvalModeV1,
        reference: Vec<u32>,
        params: PalwEvalStageParamsV1,
    ) -> PalwImproveEvalTaskV1 {
        let subject = PalwEvalSubjectV1::Candidate(held.class_id);
        let job =
            PalwEvalJobV1 { line_id: Hash64::from_bytes([0x11; 64]), epoch: 3, item: 7, subject, kind, part: 0, mode: mode.clone() };
        PalwImproveEvalTaskV1 {
            line_id: job.line_id,
            epoch: 3,
            item: 7,
            subject,
            subject_class: held.class_id,
            kind,
            mode,
            job_id: job.id(),
            prompt_ids: vec![3, 5, 1],
            reference_ids: reference,
            params,
        }
    }

    fn facts() -> PalwEvalClaimFactsV1 {
        PalwEvalClaimFactsV1 {
            network_domain: Hash64::from_bytes([0x10; 64]),
            executor_bond: kaspa_consensus_core::config::premine::premine_outpoint(2),
            executor_pubkey: vec![7; 16],
            operator_id: Hash64::from_bytes([0x12; 64]),
            anchor_block: Hash64::from_bytes([0x13; 64]),
            anchor_daa: 99,
            prompt_ids_form: PalwPromptIdsFormV1::Flat,
            trace_retention_daa: 5_000,
        }
    }

    fn generating(held: &PalwEvalHeldV1) -> PalwImproveEvalTaskV1 {
        let seed = kaspa_consensus_core::palw_improve_state_v1::palw_improve_eval_seed_v1(&Hash64::from_bytes([0x33; 64]), 7);
        task(
            held,
            PalwScoringKindV1::ExactMatch,
            PalwEvalModeV1::Generate { seed, max_new: 4, stop_ids: vec![] },
            vec![],
            PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 },
        )
    }

    /// **A generating job end to end**: the executor runs the chain's context (the subject's text
    /// stage alone), binds the roots, assembles the evaluation claim and the claim passes the chain's
    /// own check; its answer is the span's hash the fold's row keeps; a seat holding the same weights
    /// judges it `Valid`, a seat holding other weights finds the replay differs, a seat of another class
    /// cannot judge it, and a lying claim (another score lane or another id) is refused at the claim's
    /// own check or differs on replay.
    #[test]
    fn a_generating_job_runs_binds_and_a_seat_replays_it() {
        let held = held(0);
        let t = generating(&held);
        let work = palw_eval_run_v1(&held, &t).expect("the run");
        assert_eq!(work.generated().len(), 4);
        assert!(work.tail.score.is_empty(), "an ExactMatch claim commits no score: the fold scores it at the key's reveal");
        assert_eq!(work.answer(), palw_improve_answer_of_v1(work.generated(), -1, -1));
        let claim = palw_eval_claim_v1(&work, &held, &facts()).expect("the claim assembles and checks");
        assert_eq!(claim.commitment.job.version, kaspa_consensus_core::palw_improve_eval_v1::PALW_FP_EVAL_VERSION);
        assert_eq!(claim.commitment.decode_tokens_executed, 4);
        assert_eq!(claim.commitment.stop_reason, PalwFpStopReasonV3::ExactBudgetReached, "four ids at a budget of four");
        assert_eq!(claim.commitment.work_leaves, work.execution.space.leaf_count());
        assert_eq!(claim.commitment.execution_root, work.binding.committed_execution_root);
        // The claim is the chain's: its payload round-trips through A6's carriage.
        let payload = kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3 {
            version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            commitment: claim.commitment.clone(),
            prompt_token_ids: claim.prompt.clone(),
            signature: vec![0; 8],
        };
        let bytes = kaspa_consensus_core::palw_improve_eval_v1::palw_fp_eval_payload_encode_v1(&payload, &claim.tail);
        let (back, tail) = kaspa_consensus_core::palw_improve_eval_v1::palw_fp_eval_payload_decode_v1(&bytes).expect("decodes");
        assert_eq!((back.commitment, tail), (claim.commitment.clone(), claim.tail.clone()));
        // The seat: the same weights replay to the claim's roots.
        assert_eq!(palw_eval_seat_judge_v1(&held, &claim.commitment, &claim.prompt, &claim.tail), PalwEvalSeatJudgmentV1::Valid);
        // The same judgment from the chain's state alone: the claim row's roots, no payload.
        let committed = PalwEvalCommittedRootsV1 {
            trace_root: claim.commitment.trace_root,
            output_root: claim.commitment.output_root,
            execution_root: claim.commitment.execution_root,
            work_leaves: claim.commitment.work_leaves,
        };
        assert_eq!(palw_eval_seat_judge_roots_v1(&held, &t, &committed), PalwEvalSeatJudgmentV1::Valid);
        let lying = PalwEvalCommittedRootsV1 { output_root: Hash64::from_bytes([9; 64]), ..committed };
        assert!(matches!(palw_eval_seat_judge_roots_v1(&held, &t, &lying), PalwEvalSeatJudgmentV1::Differs(_)));
        let lying = PalwEvalCommittedRootsV1 { execution_root: Hash64::from_bytes([9; 64]), ..committed };
        assert!(matches!(palw_eval_seat_judge_roots_v1(&held, &t, &lying), PalwEvalSeatJudgmentV1::Differs(_)));
        // Other weights under the same class id: the replay differs (its roots are not the claim's).
        let other = PalwEvalHeldV1::from_map(
            held.class_id,
            held.artifact_root,
            held.tokenizer_id,
            held.program.clone(),
            held.layout.clone(),
            subject_params(&held.program, 9),
        );
        assert!(matches!(
            palw_eval_seat_judge_v1(&other, &claim.commitment, &claim.prompt, &claim.tail),
            PalwEvalSeatJudgmentV1::Differs(_)
        ));
        // Another class cannot judge.
        let stranger = PalwEvalHeldV1::from_map(
            Hash64::from_bytes([0x99; 64]),
            held.artifact_root,
            held.tokenizer_id,
            held.program.clone(),
            held.layout.clone(),
            subject_params(&held.program, 0),
        );
        assert!(matches!(
            palw_eval_seat_judge_v1(&stranger, &claim.commitment, &claim.prompt, &claim.tail),
            PalwEvalSeatJudgmentV1::Unjudgeable(_)
        ));
        // A claim that lies about its ids is refused at the chain's own check (the ids hash to the root).
        let mut lie = claim.tail.clone();
        lie.generated[0] ^= 1;
        assert!(matches!(
            palw_eval_seat_judge_v1(&held, &claim.commitment, &claim.prompt, &lie),
            PalwEvalSeatJudgmentV1::Unjudgeable(_)
        ));
        // A task of another subject's class is refused by the run.
        let mut elsewhere = t.clone();
        elsewhere.subject_class = Hash64::from_bytes([0x55; 64]);
        assert!(palw_eval_run_v1(&held, &elsewhere).is_err());
    }

    /// **A teacher-forced RefLogLik job**: the reference rides as the stream, nothing is selected, the
    /// scoring stages commit `(hi, lo)`, which is the library's reference arithmetic over the decode
    /// stage's consumed rows; the claim checks, carries the score, and a seat replays it.
    #[test]
    fn a_teacher_forced_job_commits_the_reference_log_likelihood() {
        let held = held(1);
        let reference = vec![7u32, 2, 9, 9];
        let t = task(
            &held,
            PalwScoringKindV1::RefLogLik,
            PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::from_bytes([0x44; 64]) },
            reference.clone(),
            PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 12 },
        );
        let work = palw_eval_run_v1(&held, &t).expect("the run");
        assert_eq!(work.generated(), reference.as_slice(), "teacher-forced: the stream is the reference");
        assert_eq!(work.tail.score.len(), 2, "(hi, lo)");
        let rows: Vec<Vec<i32>> = work.execution.run.stages[0].steps[t.prompt_ids.len() - 1..]
            .iter()
            .map(|s| s.output.data.iter().map(|x| *x as i32).collect())
            .collect();
        assert_eq!(work.score_value(), Some(ref_loglik_reference_v1(&rows, &reference, 1 << 12)), "the library's arithmetic");
        let claim = palw_eval_claim_v1(&work, &held, &facts()).expect("the claim");
        assert_eq!(claim.commitment.job.decode_token_limit, 4);
        assert_eq!(palw_eval_seat_judge_v1(&held, &claim.commitment, &claim.prompt, &claim.tail), PalwEvalSeatJudgmentV1::Valid);
        // A false score in the tail is caught by the execution root's binding of the score.
        let mut lie = claim.tail.clone();
        lie.score[1] ^= 1;
        assert!(matches!(
            palw_eval_seat_judge_v1(&held, &claim.commitment, &claim.prompt, &lie),
            PalwEvalSeatJudgmentV1::Unjudgeable(_)
        ));
        // Without the disclosed reference there is nothing to prefill.
        let mut bare = t.clone();
        bare.reference_ids.clear();
        assert!(palw_eval_run_v1(&held, &bare).is_err());
        // A judged job waits.
        let judged = task(
            &held,
            PalwScoringKindV1::Judge,
            PalwEvalModeV1::Judged { judge: Hash64::from_bytes([6; 64]) },
            vec![],
            PalwEvalStageParamsV1::Judge { lo: -1, hi: 1, logit_scale_q24: 1 << 12 },
        );
        assert!(palw_eval_run_v1(&held, &judged).is_err());
    }

    /// **The capture rebuilds an accused's execution, lies included**: an honest capture rebuilds to the
    /// claim's own roots; a capture with one lane of one leaf changed rebuilds to other roots, and the
    /// first divergence from the honest run is exactly that leaf.
    #[test]
    fn a_capture_rebuilds_the_accused_and_names_the_first_divergent_leaf() {
        let held = held(2);
        let work = palw_eval_run_v1(&held, &generating(&held)).expect("the run");
        let capture = PalwEvalCaptureV1::of(&work, &held).expect("the capture");
        let (claim, binding, hashes) = capture.rebuild(&held).expect("rebuilds");
        assert_eq!(claim, work.execution.claim, "an honest capture rebuilds the claim's roots");
        assert_eq!(binding.committed_execution_root, work.binding.committed_execution_root);
        assert_eq!(palw_eval_first_divergence_v1(&hashes, &work.execution), None);
        // Plant a lie at one leaf (flip a lane byte).
        let mut lying = capture.clone();
        let (stage, leaf) = (0usize, lying.leaves[0].len() / 2);
        lying.leaves[stage][leaf][0] ^= 1;
        let (lie_claim, lie_binding, lie_hashes) = lying.rebuild(&held).expect("a lying capture still rebuilds");
        assert_ne!(lie_claim.step_root, claim.step_root);
        assert_ne!(lie_binding.committed_execution_root, binding.committed_execution_root);
        assert_eq!(palw_eval_first_divergence_v1(&lie_hashes, &work.execution), Some(leaf as u64));
        // The capture is borsh: it travels.
        let bytes = borsh::to_vec(&capture).unwrap();
        assert_eq!(borsh::from_slice::<PalwEvalCaptureV1>(&bytes).unwrap(), capture);
    }

    /// **A drill's lie files a claim the chain's door takes, and an honest replay disputes it where the lie
    /// is** (D-M3): a moved leaf parts at exactly that leaf (ids and score the honest run's); a moved id leaves
    /// every leaf the honest run's and parts at the first id; a moved score leaves leaves and ids and parts at
    /// the score. Each is self-consistent (its roots bind what it committed), each differs on a seat's replay,
    /// and the honest capture agrees; a capture of another job, a score to move on an ExactMatch job and a leaf
    /// past the run are refused.
    #[test]
    fn a_lying_executor_files_a_consistent_claim_and_a_replay_disputes_it_where_the_lie_is() {
        let held = held(4);
        let t = generating(&held);
        let honest = palw_eval_run_v1(&held, &t).expect("the run");
        let honest_claim = palw_eval_claim_v1(&honest, &held, &facts()).expect("the honest claim");
        let total = honest.execution.space.leaf_count();
        let honest_capture = PalwEvalCaptureV1::of(&honest, &held).unwrap();
        assert_eq!(palw_eval_dispute_v1(&held, &t, &honest_capture), Ok(PalwEvalDisputeV1::Agrees), "the honest capture agrees");

        // A moved leaf.
        for planted in [0, total / 2, total - 1] {
            let lie = palw_eval_run_faulted_v1(&held, &t, PalwEvalFaultV1::Leaf(planted)).expect("the lie runs");
            let claim = palw_eval_claim_v1(&lie, &held, &facts()).expect("the lie is self-consistent: the chain's door takes it");
            assert_ne!(claim.commitment.trace_root, honest_claim.commitment.trace_root, "leaf {planted}: the step tree moved");
            assert_ne!(claim.commitment.execution_root, honest_claim.commitment.execution_root);
            assert_eq!(
                claim.commitment.output_root, honest_claim.commitment.output_root,
                "leaf {planted}: the ids are the honest run's"
            );
            assert!(
                matches!(
                    palw_eval_seat_judge_v1(&held, &claim.commitment, &claim.prompt, &claim.tail),
                    PalwEvalSeatJudgmentV1::Differs(_)
                ),
                "leaf {planted}: a seat's replay differs"
            );
            let capture = PalwEvalCaptureV1::of(&lie, &held).unwrap();
            let (rebuilt, binding, _) = capture.rebuild(&held).unwrap();
            assert_eq!(rebuilt, lie.execution.claim, "leaf {planted}: the capture rebuilds the lie's own roots");
            assert_eq!(binding.committed_execution_root, lie.binding.committed_execution_root);
            let found = palw_eval_dispute_v1(&held, &t, &capture).expect("disputes");
            let (stage, local) = honest.execution.space.locate(planted).unwrap();
            assert_eq!(found, PalwEvalDisputeV1::Leaf { index: planted, stage, local }, "leaf {planted}");
            assert_eq!(found.kind(), "leaf");
            // Held to the roots the chain holds for the claim, the served capture disputes the same; a capture that is not the
            // claim's (the honest run's, served for the lying claim's roots) is no evidence of it.
            let roots_of = |c: &PalwEvalClaimV1| PalwEvalCommittedRootsV1 {
                trace_root: c.commitment.trace_root,
                output_root: c.commitment.output_root,
                execution_root: c.commitment.execution_root,
                work_leaves: c.commitment.work_leaves,
            };
            assert_eq!(
                palw_eval_dispute_committed_v1(&held, &t, &roots_of(&claim), &capture),
                Ok(found),
                "leaf {planted}: by the committed roots"
            );
            assert!(
                palw_eval_dispute_committed_v1(&held, &t, &roots_of(&claim), &honest_capture).is_err(),
                "leaf {planted}: the honest capture does not rebuild to the lying claim's roots"
            );
            assert!(
                palw_eval_dispute_committed_v1(&held, &t, &roots_of(&honest_claim), &capture).is_err(),
                "leaf {planted}: the lying capture does not rebuild to the honest claim's roots"
            );
        }

        // A moved id.
        let lie = palw_eval_run_faulted_v1(&held, &t, PalwEvalFaultV1::Output).expect("the lie runs");
        let claim = palw_eval_claim_v1(&lie, &held, &facts()).expect("an id lie is self-consistent too");
        assert_eq!(claim.commitment.trace_root, honest_claim.commitment.trace_root, "the leaves are the honest run's");
        assert_ne!(claim.commitment.output_root, honest_claim.commitment.output_root);
        assert!(matches!(
            palw_eval_seat_judge_v1(&held, &claim.commitment, &claim.prompt, &claim.tail),
            PalwEvalSeatJudgmentV1::Differs(_)
        ));
        let found = palw_eval_dispute_v1(&held, &t, &PalwEvalCaptureV1::of(&lie, &held).unwrap()).expect("disputes");
        assert_eq!(
            found,
            PalwEvalDisputeV1::Output { position: 0, committed: honest.generated()[0] ^ 1, honest: honest.generated()[0] },
            "the first id"
        );

        // A moved score, on a teacher-forced likelihood job.
        let tf = task(
            &held,
            PalwScoringKindV1::RefLogLik,
            PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::from_bytes([0x44; 64]) },
            vec![7u32, 2, 9, 9],
            PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 12 },
        );
        let honest_tf = palw_eval_run_v1(&held, &tf).expect("the run");
        let honest_tf_claim = palw_eval_claim_v1(&honest_tf, &held, &facts()).expect("the claim");
        let lie = palw_eval_run_faulted_v1(&held, &tf, PalwEvalFaultV1::Score).expect("the lie runs");
        assert_ne!(lie.score_value(), honest_tf.score_value(), "the committed score moved");
        let claim = palw_eval_claim_v1(&lie, &held, &facts()).expect("a score lie is self-consistent");
        assert_eq!(
            (claim.commitment.trace_root, claim.commitment.output_root),
            (honest_tf_claim.commitment.trace_root, honest_tf_claim.commitment.output_root)
        );
        assert_ne!(claim.commitment.execution_root, honest_tf_claim.commitment.execution_root, "the execution root binds the score");
        assert!(matches!(
            palw_eval_seat_judge_v1(&held, &claim.commitment, &claim.prompt, &claim.tail),
            PalwEvalSeatJudgmentV1::Differs(_)
        ));
        let found = palw_eval_dispute_v1(&held, &tf, &PalwEvalCaptureV1::of(&lie, &held).unwrap()).expect("disputes");
        assert_eq!(found, PalwEvalDisputeV1::Score { committed: lie.tail.score.clone(), honest: honest_tf.tail.score.clone() });
        // A leaf lie of a likelihood job names its leaf, not its score (the tree comes first).
        let lie = palw_eval_run_faulted_v1(&held, &tf, PalwEvalFaultV1::Leaf(1)).expect("the lie runs");
        assert!(matches!(
            palw_eval_dispute_v1(&held, &tf, &PalwEvalCaptureV1::of(&lie, &held).unwrap()),
            Ok(PalwEvalDisputeV1::Leaf { index: 1, .. })
        ));

        // What is refused.
        assert!(palw_eval_run_faulted_v1(&held, &t, PalwEvalFaultV1::Score).is_err(), "an ExactMatch job commits no score");
        assert!(palw_eval_run_faulted_v1(&held, &t, PalwEvalFaultV1::Leaf(total)).is_err(), "a leaf past the run");
        let mut other = t.clone();
        other.item += 1;
        other.job_id = other.job().id();
        assert!(palw_eval_dispute_v1(&held, &other, &honest_capture).is_err(), "a capture of another job is no evidence");
        // The specs.
        for fault in [PalwEvalFaultV1::Leaf(17), PalwEvalFaultV1::Output, PalwEvalFaultV1::Score] {
            assert_eq!(PalwEvalFaultV1::parse(&fault.describe()), Ok(fault));
        }
        assert!(PalwEvalFaultV1::parse("leaf:x").is_err() && PalwEvalFaultV1::parse("tree").is_err());
    }

    /// **The close that convicts an accused evaluation is built from its capture and checked as the chain checks it**
    /// (D-M3, spec 17 §17.8.6): a moved leaf is convicted by the cone close at its leaf (`EvalCone`), a moved first id by
    /// the decode close of that id (`EvalDecodeToken`, `Token`), a moved score by the score close; an honest capture has
    /// nothing to file, a capture that is not the claim's (other roots, another job) is refused, and the close the node
    /// offers is the one the chain's own court function convicts on.
    #[test]
    fn an_accused_evaluation_is_convicted_by_the_close_the_node_builds_from_its_capture() {
        use kaspa_consensus_core::palw_improve_eval_court_v1::PalwEvalOutputCloseV1;
        const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };
        let held = held(5);
        let t = generating(&held);
        let honest = palw_eval_run_v1(&held, &t).expect("the run");
        let roots_of = |w: &PalwEvalWorkV1| {
            let c = palw_eval_claim_v1(w, &held, &facts()).expect("the claim");
            PalwEvalCommittedRootsV1 {
                trace_root: c.commitment.trace_root,
                output_root: c.commitment.output_root,
                execution_root: c.commitment.execution_root,
                work_leaves: c.commitment.work_leaves,
            }
        };
        let honest_roots = roots_of(&honest);
        let honest_capture = PalwEvalCaptureV1::of(&honest, &held).unwrap();
        assert!(
            palw_eval_court_filing_v1(&held, &t, &honest_roots, &honest_capture, &PalwEvalDisputeV1::Agrees, &LIMITS, true).is_err(),
            "an honest capture agrees: nothing to file"
        );

        // A moved leaf: the cone close of that leaf convicts. The toy class has no history reduction, so no leaf is dissected.
        let total = honest.execution.space.leaf_count();
        for planted in [0, total / 2, total - 1] {
            let lie = palw_eval_run_faulted_v1(&held, &t, PalwEvalFaultV1::Leaf(planted)).expect("the lie runs");
            let committed = roots_of(&lie);
            let capture = PalwEvalCaptureV1::of(&lie, &held).unwrap();
            let found = palw_eval_dispute_committed_v1(&held, &t, &committed, &capture).expect("disputes");
            for held_regime in [false, true] {
                let filing = palw_eval_court_filing_v1(&held, &t, &committed, &capture, &found, &LIMITS, held_regime)
                    .expect("a close that convicts");
                assert_eq!(
                    (filing.label, filing.leaf, filing.opens_dissection),
                    ("cone", Some(planted), false),
                    "leaf {planted}, held {held_regime}"
                );
                let PalwCourtVerdictProofV2::EvalCone { close } = &filing.proof else { panic!("a cone proof") };
                assert_eq!(close.binding.committed_execution_root, committed.execution_root, "the close is the claim's");
                assert_eq!(
                    honest.execution.space.global_index(&close.disputed.coord),
                    Some(planted),
                    "the close names the planted leaf"
                );
            }
            // Held to another claim's roots, the same capture is no evidence.
            assert!(palw_eval_court_filing_v1(&held, &t, &honest_roots, &capture, &found, &LIMITS, true).is_err());
            // A capture of another job is no evidence of this one.
            let mut other = t.clone();
            other.item += 1;
            other.job_id = other.job().id();
            assert!(palw_eval_court_filing_v1(&held, &other, &committed, &capture, &found, &LIMITS, true).is_err());
        }

        // A moved first id: the decode close of that id.
        let lie = palw_eval_run_faulted_v1(&held, &t, PalwEvalFaultV1::Output).expect("the lie runs");
        let committed = roots_of(&lie);
        let capture = PalwEvalCaptureV1::of(&lie, &held).unwrap();
        let found = palw_eval_dispute_committed_v1(&held, &t, &committed, &capture).expect("disputes");
        let filing = palw_eval_court_filing_v1(&held, &t, &committed, &capture, &found, &LIMITS, true).expect("a close that convicts");
        assert_eq!((filing.label, filing.leaf, filing.opens_dissection), ("decode token", None, false));
        let PalwCourtVerdictProofV2::EvalDecodeToken { close } = &filing.proof else { panic!("a decode proof") };
        assert!(matches!(&close.output, PalwEvalOutputCloseV1::Token { t, .. } if *t == 0), "the first id's row");

        // A moved score, on a teacher-forced likelihood job: the score close.
        let tf = task(
            &held,
            PalwScoringKindV1::RefLogLik,
            PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::from_bytes([0x44; 64]) },
            vec![7u32, 2, 9, 9],
            PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 12 },
        );
        let lie = palw_eval_run_faulted_v1(&held, &tf, PalwEvalFaultV1::Score).expect("the lie runs");
        let committed = roots_of(&lie);
        let capture = PalwEvalCaptureV1::of(&lie, &held).unwrap();
        let found = palw_eval_dispute_committed_v1(&held, &tf, &committed, &capture).expect("disputes");
        assert!(matches!(found, PalwEvalDisputeV1::Score { .. }));
        let filing =
            palw_eval_court_filing_v1(&held, &tf, &committed, &capture, &found, &LIMITS, true).expect("a close that convicts");
        assert_eq!((filing.label, filing.leaf, filing.opens_dissection), ("score", None, false));
        let PalwCourtVerdictProofV2::EvalDecodeToken { close } = &filing.proof else { panic!("a score proof") };
        assert!(matches!(&close.output, PalwEvalOutputCloseV1::Score { .. }));
        // The same run's leaf lie is a cone close at that leaf, never the score's.
        let lie = palw_eval_run_faulted_v1(&held, &tf, PalwEvalFaultV1::Leaf(1)).expect("the lie runs");
        let committed = roots_of(&lie);
        let capture = PalwEvalCaptureV1::of(&lie, &held).unwrap();
        let found = palw_eval_dispute_committed_v1(&held, &tf, &committed, &capture).expect("disputes");
        let filing =
            palw_eval_court_filing_v1(&held, &tf, &committed, &capture, &found, &LIMITS, true).expect("a close that convicts");
        assert_eq!((filing.label, filing.leaf), ("cone", Some(1)));
    }
}
