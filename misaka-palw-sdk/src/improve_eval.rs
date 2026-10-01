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
//! **Judged kinds** wait for the judge set's class kind (A6, `PalwEvalErrorV1::JudgedNotYet`): no node
//! plans one ([`crate::improve::palw_improve_kind_runnable_v1`]).

use std::sync::Arc;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PalwFpJobTailV1, PalwFpStopReasonV3, PalwFreePromptCommitmentV3,
    PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_gen_step_v1::{
    PalwGenStepSpaceV1, palw_gen_stage_root_v1, palw_gen_step_leaf_hash_v1, palw_gen_step_root_v1,
};
use kaspa_consensus_core::palw_gen_worker_v1::{PalwGenClaimRootsV1, PalwGenExecutionV1, palw_gen_execute_v1, palw_gen_replay_committed_v1};
use kaspa_consensus_core::palw_improve_eval_v1::{
    PalwEvalBindingV1, PalwEvalClaimRootsV1, PalwEvalClaimTailV1, PalwEvalContextV1, PalwEvalJobV1, PalwEvalModeV1,
    PalwEvalStageParamsV1, PalwEvalSubjectClassV1, palw_fp_eval_claim_check_v1, palw_improve_answer_of_v1,
    palw_improve_eval_context_v1, palw_improve_eval_decode_config_v1, palw_improve_eval_pipeline_job_v1,
};
use kaspa_consensus_core::palw_improve_state_v1::PalwScoringKindV1;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_tir_artifact_v1::PalwTirTensorSourceV1;
use kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1;
use kaspa_consensus_core::palw_tir_step_v1::{palw_tir_lane_values_v1, palw_tir_lanes_le_v1};
use kaspa_consensus_core::tx::TransactionOutpoint;
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{PipelineParams, stage_job_facts};
use misaka_palw_tir::tensor::Tensor;
use misaka_palw_tir_exec::node::TirArtifactV1;

use crate::improve::PalwImproveEvalTaskV1;
use crate::lineage::PalwTirClassEntryV1;

use kaspa_consensus_core::palw_improve_eval_v1::palw_improve_subject_program_v1;
use misaka_palw_tir::pipeline::StageStepperV1;
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir_exec::{TirLockstepHubV1, TirLockstepSeatV1, TirLockstepServedV1, TirParams, TirPlan, TirStageStepperV1};
use std::cell::{OnceCell, RefCell};

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
    pub layout: PalwTirLayoutV1,
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
            layout: entry.class.layout.clone(),
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
        Self { class_id, artifact_root, tokenizer_id, program, layout, weights: PalwEvalWeightsV1::Map(params) }
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
/// (weightless: an evaluation pipeline's root is its subject class's `artifact_root`) — and the subject
/// stage's positions on the node's executor ([`PalwEvalStepperV1`]), the scoring stages on the reference.
struct PalwEvalParamsV1<'a> {
    subject: &'a dyn ParamSource,
    none: MapParams,
    stepper: PalwEvalStepperV1<'a>,
}

/// **Where the subject stage's positions are computed** (`PipelineParams::stepper`, asked once per run
/// of the stage). The executor serves the stage exactly when the stage's program is the held class's own
/// lifted unchanged (`TirStageStepperV1::serves`) — every evaluation's subject — and is then the
/// reference byte for byte (`tests/improve_eval_exec.rs`); for anything else, and for
/// [`palw_eval_run_reference_v1`], the reference interpreter runs it.
enum PalwEvalStepperV1<'a> {
    /// The reference interpreter over whole `i128` tensors.
    Reference,
    /// The executor over a held artifact's plan and params — its residency's rows where it is resident.
    Artifact(&'a TirArtifactV1),
    /// The executor over in-memory params, compiled when the stage first asks (tools, tests).
    Map { program: &'a TirProgramV1, map: &'a MapParams, compiled: Box<OnceCell<Option<(TirPlan, TirParams<'static>)>>> },
    /// A member of a lockstep batch ([`palw_eval_run_batch_v1`]): the hub steps the member's executor,
    /// built for `decl`; the seat is handed out once.
    Seat { seat: RefCell<Option<TirLockstepSeatV1>>, decl: Box<TirProgramV2> },
}

impl PipelineParams for PalwEvalParamsV1<'_> {
    fn params(&self, program: u16) -> &dyn ParamSource {
        if program == 0 { self.subject } else { &self.none }
    }

    fn stepper(&self, program: u16, decl: &TirProgramV2) -> Option<Box<dyn StageStepperV1 + '_>> {
        if program != 0 {
            return None;
        }
        fn boxed<'s>(s: TirStageStepperV1<'s>) -> Box<dyn StageStepperV1 + 's> {
            Box::new(s)
        }
        match &self.stepper {
            PalwEvalStepperV1::Reference => None,
            PalwEvalStepperV1::Artifact(artifact) => TirStageStepperV1::for_stage(artifact.plan(), artifact.params(), decl).map(boxed),
            PalwEvalStepperV1::Map { program, map, compiled } => {
                let (plan, params) = compiled.get_or_init(|| compile_map_v1(program, map)).as_ref()?;
                TirStageStepperV1::for_stage(plan, params, decl).map(boxed)
            }
            PalwEvalStepperV1::Seat { seat, decl: built } => {
                if decl != built.as_ref() {
                    return None;
                }
                seat.borrow_mut().take().map(|s| Box::new(s) as Box<dyn StageStepperV1 + '_>)
            }
        }
    }
}

/// In-memory params compiled for the executor: the program's plan and its params bound — `None` where
/// the executor refuses either (the reference then runs, and refuses them as it does).
fn compile_map_v1(program: &TirProgramV1, map: &MapParams) -> Option<(TirPlan, TirParams<'static>)> {
    let plan = TirPlan::compile(program).ok()?;
    let params = TirParams::from_map(&plan, map).ok()?;
    Some((plan, params))
}

/// **How one run computes the subject stage.**
enum PalwEvalStageV1 {
    /// The node's executor over the held weights (the default).
    Executor,
    /// The reference interpreter ([`palw_eval_run_reference_v1`]).
    Reference,
    /// A lockstep batch's member: its seat and the subject program its executor was built for.
    Seat(TirLockstepSeatV1, Box<TirProgramV2>),
}

/// Run `f` over the pipeline params of `held`.
fn with_params<R>(held: &PalwEvalHeldV1, f: impl FnOnce(&dyn PipelineParams) -> R) -> R {
    with_params_on(held, PalwEvalStageV1::Executor, f)
}

/// Run `f` over the pipeline params of `held`, its subject stage computed as `stage` says.
fn with_params_on<R>(held: &PalwEvalHeldV1, stage: PalwEvalStageV1, f: impl FnOnce(&dyn PipelineParams) -> R) -> R {
    let seat = |seat: TirLockstepSeatV1, decl: Box<TirProgramV2>| PalwEvalStepperV1::Seat { seat: RefCell::new(Some(seat)), decl };
    match &held.weights {
        PalwEvalWeightsV1::Artifact { artifact } => {
            let source = ArtifactParamsV1 { artifact, program: &held.program };
            let stepper = match stage {
                PalwEvalStageV1::Executor => PalwEvalStepperV1::Artifact(artifact),
                PalwEvalStageV1::Reference => PalwEvalStepperV1::Reference,
                PalwEvalStageV1::Seat(s, decl) => seat(s, decl),
            };
            f(&PalwEvalParamsV1 { subject: &source, none: MapParams::default(), stepper })
        }
        PalwEvalWeightsV1::Map(map) => {
            let stepper = match stage {
                PalwEvalStageV1::Executor => PalwEvalStepperV1::Map { program: &held.program, map, compiled: Box::default() },
                PalwEvalStageV1::Reference => PalwEvalStepperV1::Reference,
                PalwEvalStageV1::Seat(s, decl) => seat(s, decl),
            };
            f(&PalwEvalParamsV1 { subject: map, none: MapParams::default(), stepper })
        }
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
        PalwScoringKindV1::RefLogLik | PalwScoringKindV1::Judge | PalwScoringKindV1::Pairwise => output
            .iter()
            .map(|v| i32::try_from(*v).map_err(|_| format!("a {kind:?} score lane is not an i32: {v}")))
            .collect(),
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
///
/// The subject stage runs on the node's typed executor over the held weights (a resident artifact's
/// rows; `misaka_palw_tir_exec::stage`), byte-identical to the reference interpreter; the scoring stages,
/// weightless, run on the reference. [`palw_eval_run_reference_v1`] runs every stage on the reference.
pub fn palw_eval_run_v1(held: &PalwEvalHeldV1, task: &PalwImproveEvalTaskV1) -> Result<PalwEvalWorkV1, String> {
    eval_run_on_v1(held, task, PalwEvalStageV1::Executor)
}

/// **[`palw_eval_run_v1`] on the reference interpreter alone** — the meaning the executor is held to (the
/// differential's other side), and a tool's check.
pub fn palw_eval_run_reference_v1(held: &PalwEvalHeldV1, task: &PalwImproveEvalTaskV1) -> Result<PalwEvalWorkV1, String> {
    eval_run_on_v1(held, task, PalwEvalStageV1::Reference)
}

fn eval_run_on_v1(held: &PalwEvalHeldV1, task: &PalwImproveEvalTaskV1, stage: PalwEvalStageV1) -> Result<PalwEvalWorkV1, String> {
    if task.subject_class != held.class_id {
        return Err(format!("the task's subject is class {}, this node holds {}", task.subject_class, held.class_id));
    }
    let job = task.job();
    let ctx = held.context(&job, task.params)?;
    let execution = with_params_on(held, stage, |params| {
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
            PalwEvalModeV1::Judged { .. } => Err("judged kinds wait for the judge set's class kind (A6)".to_string()),
        }
    })?;
    let score = score_lanes_of(task.kind, &execution.run.output.data)?;
    let binding = PalwEvalBindingV1::of(
        &job,
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
    };
    Ok(PalwEvalWorkV1 { task: task.clone(), ctx, execution, binding, tail })
}

// ---------------------------------------------------------------------------------------------
// The executor, and the candidates of one parent in lockstep
// ---------------------------------------------------------------------------------------------

impl PalwEvalHeldV1 {
    /// **Does this node's executor compute this subject's evaluations?** True when the held artifact's plan
    /// is the class's program — every evaluation's subject stage then runs there — and false where the
    /// reference interpreter would run it (a log line's word; the result is the same either way). Read off
    /// the plan alone for a held artifact: no executor is built, so no weight is scanned on the caller's
    /// thread; in-memory weights (tools, tests) are compiled and bound to answer.
    pub fn evaluates_on_executor_v1(&self) -> bool {
        let Ok(decl) = palw_improve_subject_program_v1(&self.program) else { return false };
        match &self.weights {
            PalwEvalWeightsV1::Artifact { artifact } => TirStageStepperV1::serves(artifact.plan(), &decl),
            PalwEvalWeightsV1::Map(_) => with_params(self, |params| params.stepper(0, &decl).is_some()),
        }
    }

    /// **The weights this subject's executor reads**, by inventory root: a composite candidate's parent's
    /// (its params `0..p` are the parent's, served from one store per parent root — RFC-0004 §6.3), the
    /// class's own otherwise. Subjects with one weights root step in lockstep over one store.
    pub fn weights_root_v1(&self) -> Hash64 {
        match &self.weights {
            PalwEvalWeightsV1::Artifact { artifact } => artifact.composite_ref().map_or(self.artifact_root, |r| r.parent_root),
            PalwEvalWeightsV1::Map(_) => self.artifact_root,
        }
    }

    /// **What one run of `positions` positions holds beside the weights** — an upper estimate: the run's
    /// records, every position's logits, commit points and written `Fixed` states at 16 bytes an element
    /// (the pipeline keeps them for the step tree, `i128` each, a history-length extent at its longest),
    /// and the executor's own state (each history at its window, each `Fixed` state twice, every block's
    /// node values at 8 bytes an element). What a lockstep batch multiplies by its width.
    pub fn run_bytes_v1(&self, positions: u64) -> u64 {
        let p = &self.program;
        let h = positions.max(1);
        let mut per_position = 0u64;
        let mut executor = 0u64;
        let post = &p.blocks[p.schedule.post as usize].nodes;
        per_position = per_position.saturating_add(post.get(p.logits as usize).map_or(0, |n| n.out.elements_at(h)));
        for b in &p.blocks {
            for n in &b.nodes {
                executor = executor.saturating_add(n.out.elements_at(h).saturating_mul(8));
            }
        }
        for (block, _) in p.occurrences() {
            for n in &p.blocks[block as usize].nodes {
                if n.commit {
                    per_position = per_position.saturating_add(n.out.elements_at(h));
                }
                if let misaka_palw_tir::Prim::StateWrite { state } = n.prim
                    && let Some(s) = p.states.get(state as usize)
                    && matches!(s.kind, misaka_palw_tir::StateKind::Fixed { .. })
                {
                    let elems = s.shape.iter().fold(1u64, |a, d| a.saturating_mul(*d as u64));
                    per_position = per_position.saturating_add(elems);
                    executor = executor.saturating_add(elems.saturating_mul(2 * s.dtype.width() as u64));
                }
            }
        }
        let layers = p.schedule.layers.len().max(1) as u64;
        for s in &p.states {
            if let misaka_palw_tir::StateKind::Hist { window } = s.kind {
                let row = s.shape.iter().fold(1u64, |a, d| a.saturating_mul(*d as u64)).saturating_mul(s.dtype.width() as u64);
                let instances = if s.per_layer { layers } else { 1 };
                executor = executor.saturating_add(row.saturating_mul(h.min(window as u64)).saturating_mul(instances));
            }
        }
        positions.saturating_mul(per_position).saturating_mul(16).saturating_add(executor)
    }
}

/// **How wide a lockstep batch of runs of `positions` positions over `held`'s weights may be**: every
/// member's admission of a layer held at once — a resident store's routed capacity over one admission
/// in flight (`misaka_palw_tir_exec::tir_lockstep_batch_v1`; unbounded by rows where nothing is routed or
/// the weights are mapped) — and every member's run ([`PalwEvalHeldV1::run_bytes_v1`]) within `spare`
/// bytes; at most `cap`, at least one (a batch of one is a run alone).
pub fn palw_eval_lockstep_width_v1(held: &PalwEvalHeldV1, positions: u64, spare: u64, cap: usize) -> usize {
    let (capacity, in_flight) = match &held.weights {
        PalwEvalWeightsV1::Artifact { artifact } => artifact.weight_store().map_or((0, 0), |store| {
            let stats = store.stats();
            (stats.routed_capacity_bytes, stats.in_flight_bytes)
        }),
        PalwEvalWeightsV1::Map(_) => (0, 0),
    };
    misaka_palw_tir_exec::tir_lockstep_batch_v1(capacity, in_flight, held.run_bytes_v1(positions), spare).min(cap).max(1)
}

/// **What a lockstep batch came to**: every member's run, in the order given, and what the hub served.
pub struct PalwEvalBatchV1 {
    pub runs: Vec<Result<PalwEvalWorkV1, String>>,
    /// The members the hub stepped together; the others ran alone, after the batch.
    pub stepped_together: usize,
    pub served: TirLockstepServedV1,
}

/// **Run evaluation tasks of one item over the candidates of one parent, in lockstep** (RFC-0004 §7.2,
/// `docs/design/palw/tir/runtime-residency.md` §8): each member's run is [`palw_eval_run_v1`]'s — the
/// same context, pipeline and decode, on its own thread — except that one hub, on this thread, steps
/// every member's subject stage together, a layer of every member before the next layer of any, so the
/// parent's weights a layer reads serve the whole batch while they are at hand (a resident store admits
/// a layer's routed rows once for all of them). Each member's run is exactly its run alone
/// (`tests/improve_eval_exec.rs`). A member the executor does not serve — another program than its
/// class's, weights it refuses — and every member of a batch of one run alone, after the batch.
pub fn palw_eval_run_batch_v1(members: &[(&PalwEvalHeldV1, &PalwImproveEvalTaskV1)]) -> PalwEvalBatchV1 {
    let n = members.len();
    let decls: Vec<Option<TirProgramV2>> =
        members.iter().map(|(held, _)| palw_improve_subject_program_v1(&held.program).ok()).collect();
    // In-memory weights (tools, tests) are compiled once for the batch's life.
    let compiled: Vec<Option<(TirPlan, TirParams<'static>)>> = members
        .iter()
        .map(|(held, _)| match &held.weights {
            PalwEvalWeightsV1::Map(map) => compile_map_v1(&held.program, map),
            PalwEvalWeightsV1::Artifact { .. } => None,
        })
        .collect();
    let mut steppers = Vec::new();
    let mut seat_of: Vec<Option<usize>> = vec![None; n];
    for (i, (held, task)) in members.iter().enumerate() {
        let Some(decl) = decls[i].as_ref().filter(|_| task.subject_class == held.class_id) else { continue };
        let stepper = match (&held.weights, &compiled[i]) {
            (PalwEvalWeightsV1::Artifact { artifact }, _) => TirStageStepperV1::for_stage(artifact.plan(), artifact.params(), decl),
            (PalwEvalWeightsV1::Map(_), Some((plan, params))) => TirStageStepperV1::for_stage(plan, params, decl),
            (PalwEvalWeightsV1::Map(_), None) => None,
        };
        if let Some(stepper) = stepper {
            seat_of[i] = Some(steppers.len());
            steppers.push(stepper);
        }
    }
    let mut runs: Vec<Option<Result<PalwEvalWorkV1, String>>> = (0..n).map(|_| None).collect();
    let mut stepped_together = 0;
    let mut served = TirLockstepServedV1::default();
    if steppers.len() >= 2
        && let Ok((hub, seats)) = TirLockstepHubV1::new(steppers)
    {
        let mut seats: Vec<Option<TirLockstepSeatV1>> = seats.into_iter().map(Some).collect();
        served = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for (i, (held, task)) in members.iter().enumerate() {
                let (Some(k), Some(decl)) = (seat_of[i], decls[i].clone()) else { continue };
                let Some(seat) = seats[k].take() else { continue };
                let (held, task) = (*held, *task);
                // A thread that cannot start drops its seat (the hub stops waiting for it) and its member
                // runs alone below.
                if let Ok(handle) = std::thread::Builder::new()
                    .name(format!("palw-eval-lockstep-{i}"))
                    .spawn_scoped(scope, move || eval_run_on_v1(held, task, PalwEvalStageV1::Seat(seat, Box::new(decl))))
                {
                    handles.push((i, handle));
                }
            }
            // Seats no member took leave now, before the hub waits on them.
            drop(seats);
            stepped_together = handles.len();
            let served = hub.serve();
            for (i, handle) in handles {
                runs[i] = Some(handle.join().unwrap_or_else(|_| Err("the evaluation run panicked".to_string())));
            }
            served
        });
    }
    let runs = members.iter().zip(runs).map(|((held, task), run)| run.unwrap_or_else(|| palw_eval_run_v1(held, task))).collect();
    PalwEvalBatchV1 { runs, stepped_together, served }
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
        PalwEvalModeV1::Judged { .. } => return Err("judged kinds wait for the judge set's class kind (A6)".to_string()),
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
pub fn palw_eval_claim_v1(work: &PalwEvalWorkV1, held: &PalwEvalHeldV1, facts: &PalwEvalClaimFactsV1) -> Result<PalwEvalClaimV1, String> {
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
    palw_fp_eval_claim_check_v1(&claim.commitment, &claim.prompt, &claim.tail, &[]).map_err(|e| format!("the claim does not check: {e}"))?;
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
pub fn palw_eval_task_of_claim_v1(commitment: &PalwFreePromptCommitmentV3, prompt: &[u32], tail: &PalwEvalClaimTailV1) -> Option<PalwImproveEvalTaskV1> {
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
    if let Err(e) = palw_fp_eval_claim_check_v1(commitment, prompt, tail, &[]) {
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
        return J::Differs(format!("the replay's execution root {} is not the claim's {}", roots.execution_root, commitment.execution_root));
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
        return J::Differs(format!("the replay's execution root {} is not the claim's {}", roots.execution_root, claim.execution_root));
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
        let ctx = held.context(&self.job, self.params)?;
        let pjob = palw_improve_eval_pipeline_job_v1(&ctx, &self.prompt, &self.generated, &[]);
        let facts = stage_job_facts(&ctx.pipeline, &ctx.programs, &pjob).map_err(|e| e.to_string())?;
        let trips: Vec<u32> = facts.iter().map(|f| f.trip).collect();
        let space = PalwGenStepSpaceV1::new(&ctx.pipeline, &ctx.programs, &ctx.layouts, &trips, self.prompt.len() as u32)
            .map_err(|e| e.to_string())?;
        if self.leaves.len() != space.stages.len() {
            return Err("one leaf list per stage".into());
        }
        let mut leaf_hashes = Vec::with_capacity(space.stages.len());
        for (s, (stage, lanes)) in space.stages.iter().zip(&self.leaves).enumerate() {
            if lanes.len() != stage.leaves().len() {
                return Err(format!("stage {s}: {} leaves captured for {}", lanes.len(), stage.leaves().len()));
            }
            let mut hashes = Vec::with_capacity(lanes.len());
            for (leaf, bytes) in stage.leaves().iter().zip(lanes) {
                let v = palw_tir_lane_values_v1(leaf.dtype, bytes).map_err(|e| e.to_string())?;
                if v.len() != leaf.value_count as usize {
                    return Err(format!("{:?}: {} lanes for {}", leaf.coord, v.len(), leaf.value_count));
                }
                hashes.push(palw_gen_step_leaf_hash_v1(leaf, &v).map_err(|e| e.to_string())?);
            }
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
        Ok((claim, binding, leaf_hashes))
    }
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

    fn task(held: &PalwEvalHeldV1, kind: PalwScoringKindV1, mode: PalwEvalModeV1, reference: Vec<u32>, params: PalwEvalStageParamsV1) -> PalwImproveEvalTaskV1 {
        let subject = PalwEvalSubjectV1::Candidate(held.class_id);
        let job = PalwEvalJobV1 { line_id: Hash64::from_bytes([0x11; 64]), epoch: 3, item: 7, subject, kind, mode: mode.clone() };
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
            PalwEvalStageParamsV1::Judge { lo: -1, hi: 1 },
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
}
