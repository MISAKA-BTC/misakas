//! **RFC-0004 §7.2 (A6): the evaluation job family** — dormant under `palw_improvement_v1`.
//!
//! * **The job** ([`PalwEvalJobV1`]): a governed line's epoch, a drawn item, a subject (the parent, a
//!   candidate, or the head's predecessor) and a scoring kind. Its seed ([`palw_improve_eval_seed_v1`])
//!   is the same for every subject of an item, so pairing is exact; its id
//!   ([`palw_improve_eval_job_id_v1`]) binds the line, the epoch, the item, the subject and the kind.
//! * **The context** ([`palw_improve_eval_context_v1`]): the evaluation pipeline the chain derives from
//!   the job, the subject's IR class and the policy's scoring parameters. The subject's version-1
//!   program, lifted to version 2 unchanged, runs over its own layout: an ExactMatch job is that
//!   stage alone, as the pipeline's text stage (`TextStream`) — the key is hidden until every
//!   subject's outputs are final, so no claim can run the key-reading stage, and the fold scores the
//!   generation at the key's reveal ([`palw_improve_exact_match_score_v1`], the library stage's rule);
//!   a RefLogLik job is a `Decode` stage over the reference, then the scoring library's two stages at
//!   the subject's `max_context` and logits row. Executors choose nothing. The pipeline's params are
//!   the subject class's own inventory (`PalwGenInventoryNamingV1::Evaluation`): its root is the
//!   class's registered `artifact_root`.
//! * **The binding** ([`PalwEvalBindingV1`]): what an evaluation claim commits — every stage's root,
//!   the stream stage's ids, the finalized outputs it read and the score — and its execution root
//!   ([`palw_improve_eval_execution_root_v1`]). The claim's roots ([`PalwEvalClaimRootsV1`]):
//!   `trace_root` the step root, `output_root` the generated ids' root (where a later job's
//!   `FinalizedOutput` read is opened), `execution_root` the evaluation's.
//! * **The carriage** (RFC-0004 §4.1): an FP commitment whose job is version 9
//!   ([`PALW_FP_EVAL_VERSION`]) with an evaluation job as its tail, named under its own key
//!   ([`fp_job_id_eval_carried_v1`]) and admitted by this module's stateless rules only
//!   ([`palw_fp_eval_job_shape_v1`]; every FP validator refuses it by name). Its payload carries the
//!   claim's generated ids and score after the FP payload's bytes ([`PalwEvalClaimTailV1`]), bound by
//!   the committed roots ([`palw_fp_eval_claim_check_v1`]): nobody can withhold what a later job or
//!   the ExactMatch fold reads. [`palw_fp_claim_is_evaluation_v1`] is the one predicate the lane's
//!   fold asks to keep such a claim off the reward path.
//! * **The job table** (`improvement_eval_jobs`, delta entries 100–103, [`PalwEvalJobStateV1`]): the
//!   first valid claim per job in the accepting chain's order is the one ([`palw_improve_eval_take_v1`]);
//!   none after `t_eval`; a job without a final claim is missing.
//! * **Outcomes** ([`palw_improve_item_outcome_v1`], spec 17 §17.9): a missing evaluation always
//!   favours the incumbent — a missing candidate score is a loss, a missing parent score a parent win.
//!   Missing data can block a promotion, never make one.
//! * **Cost** ([`palw_improve_eval_reservation_v1`], [`palw_improve_eval_budget_positions_v1`],
//!   [`palw_improve_candidate_eval_escrow_v1`]): a claim reserves capacity by ADR-0160's stage-1
//!   rule; an epoch's positions are bounded; a candidate escrows its own jobs' fees.

use crate::Hash64;
use crate::palw_decode_pipeline_v4::DecodeConfigV4;
use crate::palw_decode_select_v2::PalwDecodeSamplingV2;
use crate::palw_freeprompt_v3::{PalwFpCommitmentTxPayloadV3, PalwFpJobTailV1, PalwFreePromptCommitmentV3, PalwFreePromptJobV3};
use crate::palw_gen_worker_v1::PalwGenDecodeV1;
use crate::palw_improve_state_v1::{PalwEvalSpecV1, PalwEvalSubjectV1, PalwImprovementFeesV1, PalwItemOutcomeV1, PalwScoringKindV1};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::pipeline::{
    Binding, StageDecl, TIR_PIPELINE_VERSION_V1, TirPipelineV1, TokenPad, TokenRule, TokenSource, TripRule, validate_pipeline,
};
use misaka_palw_tir::program_v2::{OutputDecl, TirProgramV2};

/// Key of [`palw_improve_eval_seed_v1`] (RFC-0004 §7.2).
pub const PALW_IMPROVE_EVAL_SEED_DOMAIN_V1: &[u8] = b"misaka-palw/improve/eval-seed/v1";
/// Key of [`palw_improve_eval_job_id_v1`] (RFC-0004 §7.2).
pub const PALW_IMPROVE_EVAL_JOB_DOMAIN_V1: &[u8] = b"misaka-palw/improve/eval-job/v1";
/// Key of [`palw_improve_eval_execution_root_v1`]: its own, so an evaluation claim's root never
/// verifies as an FP, IR or pipeline claim's.
pub const PALW_IMPROVE_EVAL_EXECUTION_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/improve/eval-execution-root/v1";
/// Wire version of [`PalwEvalBindingV1`].
pub const PALW_IMPROVE_EVAL_BINDING_VERSION_V1: u16 = 1;
/// The scoring stages' layout: 4-lane commit and state tiles, a checkpoint every position (they run
/// one position, or one per reference id, and read no history).
pub const PALW_IMPROVE_SCORING_TILE_V1: u32 = 4;
/// **FP job version 9** (RFC-0004 §4.1, §7.2): an evaluation job, carried by the free-prompt lane —
/// the V4 job's fields and decode rules, then a [`PalwEvalJobV1`].
pub const PALW_FP_EVAL_VERSION: u16 = 9;
/// Key of [`fp_job_id_eval_carried_v1`]: its own, so an evaluation job's id is never an FP job's.
pub const PALW_FP_EVAL_DOMAIN_JOB_ID: &[u8] = b"misaka-palw/improve/fp-eval/job-id/v1";
/// Key of [`palw_improve_eval_generated_root_v1`].
pub const PALW_IMPROVE_EVAL_GENERATED_DOMAIN_V1: &[u8] = b"misaka-palw/improve/eval-generated/v1";
/// Key of [`palw_improve_eval_finalized_root_v1`].
pub const PALW_IMPROVE_EVAL_FINALIZED_DOMAIN_V1: &[u8] = b"misaka-palw/improve/eval-finalized/v1";
/// Key of [`palw_improve_answer_span_hash_v1`].
pub const PALW_IMPROVE_ANSWER_SPAN_DOMAIN_V1: &[u8] = b"misaka-palw/improve/answer-span/v1";

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------------------------------
// The job, its seed and its id
// ---------------------------------------------------------------------------------------------

/// **How the subject stage runs.**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwEvalModeV1 {
    /// RFC-0001 §A's selection rule, R's domain 0 under `seed`.
    Generate { seed: Hash64, max_new: u32, stop_ids: Vec<u32> },
    /// The prompt and the reference continuation as prefill; logits consumed over the reference
    /// (revealed at the draw, RFC-0004 §7.1 as decided 2026-09-29).
    TeacherForced { reference_commitment: Hash64 },
    /// A judge class scores a finalized generation (Judge) or two (Pairwise): the judge the draw
    /// named for the item.
    Judged { judge: Hash64 },
}

/// **An evaluation job**: everything its context is derived from. Executors choose nothing.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalJobV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub item: u32,
    pub subject: PalwEvalSubjectV1,
    /// The scoring kind: which pipeline the context derives, and which outcome rule reads the score.
    pub kind: PalwScoringKindV1,
    pub mode: PalwEvalModeV1,
}

impl PalwEvalJobV1 {
    /// The job's id ([`palw_improve_eval_job_id_v1`]).
    pub fn id(&self) -> Hash64 {
        palw_improve_eval_job_id_v1(&self.line_id, self.epoch, self.item, &self.subject, self.kind)
    }
}

/// **An item's generation seed** (RFC-0004 §7.2): the same for every subject, so pairing is exact.
pub fn palw_improve_eval_seed_v1(epoch_seed: &Hash64, item: u32) -> Hash64 {
    keyed64(PALW_IMPROVE_EVAL_SEED_DOMAIN_V1, &[epoch_seed.as_byte_slice(), &item.to_le_bytes()])
}

/// **An evaluation job's id**: `H(line ‖ le64 epoch ‖ le32 item ‖ borsh(subject) ‖ kind)` — the kind
/// in it because a subject's primary job and its judge job share an item.
pub fn palw_improve_eval_job_id_v1(
    line_id: &Hash64,
    epoch: u64,
    item: u32,
    subject: &PalwEvalSubjectV1,
    kind: PalwScoringKindV1,
) -> Hash64 {
    let subject_bytes = borsh::to_vec(subject).expect("a subject is borsh-serializable");
    keyed64(
        PALW_IMPROVE_EVAL_JOB_DOMAIN_V1,
        &[line_id.as_byte_slice(), &epoch.to_le_bytes(), &item.to_le_bytes(), &subject_bytes, &[kind as u8]],
    )
}

/// **The mode a kind runs in**: ExactMatch generates under the item's seed and the policy's budget
/// and stops; RefLogLik is teacher-forced over the case's reference; Judge and Pairwise are judged
/// by the judge the draw named.
pub fn palw_improve_eval_mode_v1(
    kind: PalwScoringKindV1,
    spec: &PalwEvalSpecV1,
    item_seed: Hash64,
    reference_commitment: Option<Hash64>,
    judge: Option<Hash64>,
) -> Result<PalwEvalModeV1, PalwEvalErrorV1> {
    Ok(match kind {
        PalwScoringKindV1::ExactMatch => {
            PalwEvalModeV1::Generate { seed: item_seed, max_new: spec.max_new_tokens, stop_ids: spec.stop_ids.clone() }
        }
        PalwScoringKindV1::RefLogLik => PalwEvalModeV1::TeacherForced {
            reference_commitment: reference_commitment.ok_or(PalwEvalErrorV1::Item("a likelihood item has a reference"))?,
        },
        PalwScoringKindV1::Judge | PalwScoringKindV1::Pairwise => {
            PalwEvalModeV1::Judged { judge: judge.ok_or(PalwEvalErrorV1::Item("a judged item names its judge"))? }
        }
    })
}

// ---------------------------------------------------------------------------------------------
// The derived context: the evaluation pipeline
// ---------------------------------------------------------------------------------------------

/// **The scoring stage's parameters** as the context reads them — the policy's
/// `PalwScoringParamsV1` (rfc4/core), field for field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwEvalStageParamsV1 {
    /// The answer span's delimiters (−1: none) — read by the fold at the key's reveal
    /// ([`palw_improve_answer_of_v1`]); an ExactMatch job's pipeline is its generation alone.
    ExactMatch {
        open: i32,
        close: i32,
    },
    /// The subject's logits in Q24 nats per logit unit (the line's output interface).
    RefLogLik {
        logit_scale_q24: i32,
    },
    Judge {
        lo: i32,
        hi: i32,
    },
    Pairwise {
        margin: i32,
    },
}

impl PalwEvalStageParamsV1 {
    pub fn kind(&self) -> PalwScoringKindV1 {
        match self {
            Self::ExactMatch { .. } => PalwScoringKindV1::ExactMatch,
            Self::RefLogLik { .. } => PalwScoringKindV1::RefLogLik,
            Self::Judge { .. } => PalwScoringKindV1::Judge,
            Self::Pairwise { .. } => PalwScoringKindV1::Pairwise,
        }
    }
}

/// **The subject as the context reads it**: its IR class's id and `artifact_root`, its program (the
/// class row's canonical bytes, decoded) and its layout (carried by the claim, checked against the
/// row's `layout_digest`).
#[derive(Clone, Copy, Debug)]
pub struct PalwEvalSubjectClassV1<'a> {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub program: &'a TirProgramV1,
    pub layout: &'a PalwTirLayoutV1,
}

/// **An evaluation job's context**: the pipeline, its programs and layouts, the job scalars the
/// scoring stages read, the decode rules of a generating job, and what the params are proven under.
#[derive(Clone, Debug)]
pub struct PalwEvalContextV1 {
    pub job_id: Hash64,
    /// `R`'s seed: the item's seed, first 32 bytes (the decode's domain 0 under greedy selection
    /// reads none; kept for a policy that samples).
    pub seed: [u8; 32],
    pub pipeline: TirPipelineV1,
    pub programs: Vec<TirProgramV2>,
    pub layouts: Vec<PalwTirLayoutV1>,
    pub scalars: Vec<i64>,
    /// A generating job's decode: FP Job V4's rules, greedy, the policy's budget and stops.
    pub decode: Option<PalwGenDecodeV1>,
    pub subject_class: Hash64,
    /// The subject class's `artifact_root`: the pipeline's params are proven under it.
    pub artifact_root: Hash64,
    /// **The dissected commit points** `(stage, block, node)` — every commit point whose cone reduces
    /// over `H` (spec 04b §9.5.1), as a registered pipeline's row lists them: a court answers these
    /// with F7's phase (a root claim), never a whole cone. The subject stage's are its IR class's own
    /// (`PalwTirClassRecordV1::dissected`), the lifted program's view being the class's program; the
    /// scoring stages read no history and have none.
    pub dissected: Vec<(u8, u8, u16)>,
}

/// Why a context, a binding or a claim is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwEvalErrorV1 {
    #[error("the item cannot be evaluated: {0}")]
    Item(&'static str),
    #[error("the job's kind, its mode and the policy's stage disagree")]
    KindMismatch,
    #[error("the subject's program: {0}")]
    Subject(String),
    #[error("the scoring stage: {0}")]
    Scoring(String),
    #[error("judged kinds wait for the judge set's class kind (RFC-0004 §7.3, A6 open item)")]
    JudgedNotYet,
    #[error("the job was claimed at {by_daa} by claim {claim}")]
    Taken { claim: Hash64, by_daa: u64 },
    #[error("no evaluation claim is accepted at or after t_eval ({t_eval})")]
    PastEval { t_eval: u64 },
    #[error("the binding: {0}")]
    Binding(String),
    /// A carried job that is not an evaluation job: version 9, its decode rules present, an
    /// evaluation job its tail.
    #[error("job version {version} with this tail is not an evaluation job (version 9, decode rules and an evaluation job present)")]
    NotAnEvalJob { version: u16 },
    /// An FP field the evaluation job fixes, by name.
    #[error("the evaluation job's FP fields: {0}")]
    JobField(&'static str),
    /// The claim's payload tail or its roots.
    #[error("the evaluation claim's carriage: {0}")]
    Carriage(&'static str),
}

/// **The subject's program as a pipeline stage**: its version-1 program lifted to version 2
/// unchanged — no input, its logits node the `Logits` output under its own scheme.
pub fn palw_improve_subject_program_v1(program: &TirProgramV1) -> Result<TirProgramV2, PalwEvalErrorV1> {
    TirProgramV2::from_v1_lifting_params(
        program,
        &[],
        OutputDecl::Logits { node: program.logits, scheme_id: program.logits_scheme_id },
    )
    .map_err(|e| PalwEvalErrorV1::Subject(e.to_string()))
}

/// **A scoring stage's layout**: [`PALW_IMPROVE_SCORING_TILE_V1`]-lane tiles, a checkpoint every
/// position, its `max_trip` as its context.
pub fn palw_improve_scoring_layout_v1(program: &TirProgramV2, max_trip: u32) -> PalwTirLayoutV1 {
    let commits = program.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: max_trip,
        checkpoint_interval: 1,
        h_tile: 16,
        commit_tiles: vec![PALW_IMPROVE_SCORING_TILE_V1; commits],
        state_tiles: vec![PALW_IMPROVE_SCORING_TILE_V1; program.states.len()],
    }
}

fn rule(source: TokenSource, to_len: Option<u32>) -> TokenRule {
    TokenRule { prefix: vec![], source, suffix: vec![], pad: to_len.map(|to_len| TokenPad { id: 0, to_len }) }
}

/// **A generating job's decode rules** (RFC-0004 §7.2): FP Job V4's no-op rules with the policy's
/// stop ids, each a one-id stop sequence, sorted and without repeats — the one spelling the context
/// and the carried job's shape rule share.
pub fn palw_improve_eval_decode_config_v1(stop_ids: &[u32]) -> DecodeConfigV4 {
    let mut stops: Vec<Vec<u32>> = stop_ids.iter().map(|id| vec![*id]).collect();
    stops.sort();
    stops.dedup();
    DecodeConfigV4 { stop_sequences: stops, ..DecodeConfigV4::NOOP }
}

/// **The evaluation job's context** (RFC-0004 §7.2): see the module doc. An ExactMatch job is the
/// subject's generation alone — its text stage, greedy under FP Job V4's rules with the policy's
/// budget and stops — and the fold scores it at the key's reveal; RefLogLik reads the decode stage's
/// consumed rows position by position with the logit scale as job scalar 0, then sums them. Judged
/// kinds are refused until the judge set's class kind is settled.
pub fn palw_improve_eval_context_v1(
    job: &PalwEvalJobV1,
    subject: &PalwEvalSubjectClassV1<'_>,
    params: PalwEvalStageParamsV1,
) -> Result<PalwEvalContextV1, PalwEvalErrorV1> {
    if params.kind() != job.kind {
        return Err(PalwEvalErrorV1::KindMismatch);
    }
    let lifted = palw_improve_subject_program_v1(subject.program)?;
    let max_trip = subject.layout.max_context;
    let subject_stage = StageDecl { name: "subject".into(), program: 0, trip: TripRule::Decode, max_trip, tokens: None, bind: vec![] };
    let scoring = |e: misaka_palw_tir::TirError| PalwEvalErrorV1::Scoring(e.to_string());
    let (stages, programs, scalars, decode, seed) = match (params, &job.mode) {
        (PalwEvalStageParamsV1::ExactMatch { .. }, PalwEvalModeV1::Generate { seed, max_new, stop_ids }) => {
            if *max_new == 0 {
                return Err(PalwEvalErrorV1::Item("a generating job generates at least one id"));
            }
            // The generation alone, as the pipeline's text stage: the key is not disclosed until
            // every subject's outputs are final, so the fold scores the answer span then.
            let text = StageDecl { trip: TripRule::TextStream, ..subject_stage };
            let config = palw_improve_eval_decode_config_v1(stop_ids);
            let decode = PalwGenDecodeV1 { config, sampling: PalwDecodeSamplingV2::GREEDY, limit: *max_new };
            let seed: [u8; 32] = seed.as_byte_slice()[..32].try_into().expect("a 64-byte hash");
            (vec![text], vec![lifted], vec![], Some(decode), seed)
        }
        (PalwEvalStageParamsV1::RefLogLik { logit_scale_q24 }, PalwEvalModeV1::TeacherForced { .. }) => {
            if logit_scale_q24 <= 0 {
                return Err(PalwEvalErrorV1::Scoring("a logit scale is positive".into()));
            }
            let row = {
                let node = &lifted.blocks[lifted.schedule.post as usize].nodes[lifted.output.node() as usize];
                node.out.shape.iter().map(|d| if let misaka_palw_tir::Dim::Fixed(n) = d { *n } else { 0 }).collect::<Vec<u32>>()
            };
            let shape = misaka_palw_tir::scoring::RefLogLikShapeV1 { rows: max_trip, row };
            let lp = misaka_palw_tir::scoring::ref_logprob_v1(&shape).map_err(scoring)?;
            let sum = misaka_palw_tir::scoring::ref_loglik_sum_v1(max_trip).map_err(scoring)?;
            let lp_stage = StageDecl {
                name: "ref.lp".into(),
                program: 1,
                trip: TripRule::TokenCount,
                max_trip,
                tokens: Some(rule(TokenSource::Generated, None)),
                bind: vec![Binding::StageRows { stage: 0, drop: 0, pad_to: max_trip }, Binding::JobScalar { index: 0 }],
            };
            let score = StageDecl {
                name: "score".into(),
                program: 2,
                trip: TripRule::Fixed { n: 1 },
                max_trip: 1,
                tokens: None,
                bind: vec![Binding::StageRows { stage: 1, drop: 0, pad_to: max_trip }, Binding::StageRowCount { stage: 1, drop: 0 }],
            };
            (vec![subject_stage, lp_stage, score], vec![lifted, lp, sum], vec![logit_scale_q24 as i64], None, [0u8; 32])
        }
        (PalwEvalStageParamsV1::Judge { .. } | PalwEvalStageParamsV1::Pairwise { .. }, PalwEvalModeV1::Judged { .. }) => {
            return Err(PalwEvalErrorV1::JudgedNotYet);
        }
        _ => return Err(PalwEvalErrorV1::KindMismatch),
    };
    let output_stage = (stages.len() - 1) as u8;
    let pipeline = TirPipelineV1 { version: TIR_PIPELINE_VERSION_V1, stages, output_stage };
    validate_pipeline(&pipeline, &programs).map_err(|e| PalwEvalErrorV1::Scoring(e.to_string()))?;
    let mut layouts = vec![subject.layout.clone()];
    for (st, p) in pipeline.stages.iter().zip(&programs).skip(1) {
        layouts.push(palw_improve_scoring_layout_v1(p, st.max_trip));
    }
    let dissected = pipeline
        .stages
        .iter()
        .enumerate()
        .flat_map(|(s, st)| {
            let view = programs[st.program as usize].v1_view();
            crate::palw_tir_dissect_v1::palw_tir_dissected_commit_points_v1(&view).into_iter().map(move |(b, n)| (s as u8, b, n))
        })
        .collect();
    Ok(PalwEvalContextV1 {
        job_id: job.id(),
        seed,
        pipeline,
        programs,
        layouts,
        scalars,
        decode,
        subject_class: subject.class_id,
        artifact_root: subject.artifact_root,
        dissected,
    })
}

// ---------------------------------------------------------------------------------------------
// The binding and the execution root
// ---------------------------------------------------------------------------------------------

fn ids_bytes(ids: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + ids.len() * 4);
    out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
    for id in ids {
        out.extend_from_slice(&id.to_le_bytes());
    }
    out
}

/// **The generated ids' root** — `H64(key, le32 n ‖ le32 ids)`: an evaluation claim's `output_root`,
/// and so the commitment a later job's `FinalizedOutput` read of this claim is opened against.
pub fn palw_improve_eval_generated_root_v1(generated: &[u32]) -> Hash64 {
    keyed64(PALW_IMPROVE_EVAL_GENERATED_DOMAIN_V1, &[&ids_bytes(generated)])
}

/// **The finalized outputs' root** — `H64(key, le32 m ‖ root_0 ‖ …)` over the `output_root` of each
/// final claim a job reads (`FinalizedOutput { claim, .. }`), in claim-index order. The fold derives
/// the list from the job table, never from the claim; a job that reads none commits `m = 0`.
pub fn palw_improve_eval_finalized_root_v1(output_roots: &[Hash64]) -> Hash64 {
    let mut body = Vec::with_capacity(4 + output_roots.len() * 64);
    body.extend_from_slice(&(output_roots.len() as u32).to_le_bytes());
    for root in output_roots {
        body.extend_from_slice(root.as_byte_slice());
    }
    keyed64(PALW_IMPROVE_EVAL_FINALIZED_DOMAIN_V1, &[&body])
}

/// **An evaluation claim's execution root**: `H64(key, job_id ‖ subject class ‖ le64 leaves ‖
/// step_root ‖ generated_root ‖ finalized_root ‖ le32 |score| ‖ le32 lanes)` — the job, the class it
/// ran, the tree, the answer, what it read and the score.
pub fn palw_improve_eval_execution_root_v1(
    job_id: &Hash64,
    subject_class: &Hash64,
    step_leaf_count: u64,
    step_root: &Hash64,
    generated_root: &Hash64,
    finalized_root: &Hash64,
    score: &[i32],
) -> Hash64 {
    let mut lanes = Vec::with_capacity(4 + score.len() * 4);
    lanes.extend_from_slice(&(score.len() as u32).to_le_bytes());
    for v in score {
        lanes.extend_from_slice(&v.to_le_bytes());
    }
    keyed64(
        PALW_IMPROVE_EVAL_EXECUTION_ROOT_DOMAIN_V1,
        &[
            job_id.as_byte_slice(),
            subject_class.as_byte_slice(),
            &step_leaf_count.to_le_bytes(),
            step_root.as_byte_slice(),
            generated_root.as_byte_slice(),
            finalized_root.as_byte_slice(),
            &lanes,
        ],
    )
}

/// **The roots an evaluation claim's FP commitment carries** — and its claim row keeps: `trace_root`
/// the step root (PALW-GEN-3's tree; data availability opens leaves under it), `output_root` the
/// generated ids' root, `execution_root` the evaluation's, `work_leaves` the step leaf count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEvalClaimRootsV1 {
    pub trace_root: Hash64,
    pub output_root: Hash64,
    pub execution_root: Hash64,
    pub work_leaves: u64,
}

/// **What pins an evaluation execution** — carried by every court move and data-availability answer
/// about an evaluation claim: its [`Self::execution_root`] must be the claim's `execution_root` and
/// its [`Self::step_root`] the claim's `trace_root`. The chain derives the pipeline, its programs,
/// layouts and job scalars from `job`, the subject's class row and the policy
/// ([`palw_improve_eval_context_v1`]); nothing else here is trusted.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalBindingV1 {
    /// [`PALW_IMPROVE_EVAL_BINDING_VERSION_V1`].
    pub version: u16,
    pub job: PalwEvalJobV1,
    pub subject_class: Hash64,
    /// The subject's layout, as its class row's `layout_digest` names it.
    pub subject_layout: PalwTirLayoutV1,
    /// Every stage's root, in stage order.
    pub stage_roots: Vec<Hash64>,
    pub step_leaf_count: u64,
    /// The stream stage's ids: decoded (Generate) or given (the reference, TeacherForced).
    pub generated: Vec<u32>,
    /// What the job's `FinalizedOutput` bindings read: each final claim's generated ids, in
    /// claim-index order. Empty for a job that reads none.
    pub finalized: Vec<Vec<u32>>,
    /// The committed score tensor: the score stage's output (empty for a generation-only job).
    pub score: Vec<i32>,
    pub committed_execution_root: Hash64,
}

impl PalwEvalBindingV1 {
    /// The binding of what an executor committed.
    pub fn of(
        job: &PalwEvalJobV1,
        subject_class: Hash64,
        subject_layout: &PalwTirLayoutV1,
        roots: &crate::palw_gen_worker_v1::PalwGenClaimRootsV1,
        step_leaf_count: u64,
        finalized: Vec<Vec<u32>>,
        score: Vec<i32>,
    ) -> Self {
        let mut b = Self {
            version: PALW_IMPROVE_EVAL_BINDING_VERSION_V1,
            job: job.clone(),
            subject_class,
            subject_layout: subject_layout.clone(),
            stage_roots: roots.stage_roots.clone(),
            step_leaf_count,
            generated: roots.generated.clone(),
            finalized,
            score,
            committed_execution_root: Hash64::default(),
        };
        b.committed_execution_root = b.execution_root();
        b
    }

    /// The step root over the carried stage roots.
    pub fn step_root(&self) -> Hash64 {
        crate::palw_gen_step_v1::palw_gen_step_root_v1(&self.stage_roots)
    }

    /// The root of the stream stage's ids: the claim's `output_root`.
    pub fn generated_root(&self) -> Hash64 {
        palw_improve_eval_generated_root_v1(&self.generated)
    }

    /// The root over what the job read: each finalized output's root, in claim-index order.
    pub fn finalized_root(&self) -> Hash64 {
        let roots: Vec<Hash64> = self.finalized.iter().map(|ids| palw_improve_eval_generated_root_v1(ids)).collect();
        palw_improve_eval_finalized_root_v1(&roots)
    }

    /// The execution root its parts produce.
    pub fn execution_root(&self) -> Hash64 {
        palw_improve_eval_execution_root_v1(
            &self.job.id(),
            &self.subject_class,
            self.step_leaf_count,
            &self.step_root(),
            &self.generated_root(),
            &self.finalized_root(),
            &self.score,
        )
    }

    /// The roots its claim's commitment carries.
    pub fn claim_roots(&self) -> PalwEvalClaimRootsV1 {
        PalwEvalClaimRootsV1 {
            trace_root: self.step_root(),
            output_root: self.generated_root(),
            execution_root: self.execution_root(),
            work_leaves: self.step_leaf_count,
        }
    }

    /// The score the fold records (`PalwEvalScoreV1::value`): Judge and Pairwise its one lane;
    /// RefLogLik its `(hi, lo)` joined. An ExactMatch job commits none — the fold scores its
    /// generation at the key's reveal ([`palw_improve_exact_match_score_v1`]).
    pub fn score_value(&self) -> Result<i64, PalwEvalErrorV1> {
        palw_improve_eval_score_value_v1(self.job.kind, &self.score)
    }
}

/// **The score lanes a kind commits**: none for ExactMatch (scored by the fold at the key's
/// reveal), `(hi, lo)` for RefLogLik, one lane for Judge and Pairwise.
pub fn palw_improve_eval_score_lanes_v1(kind: PalwScoringKindV1) -> usize {
    match kind {
        PalwScoringKindV1::ExactMatch => 0,
        PalwScoringKindV1::RefLogLik => 2,
        PalwScoringKindV1::Judge | PalwScoringKindV1::Pairwise => 1,
    }
}

/// **A committed score as the fold records it**: RefLogLik's `(hi, lo)` joined, Judge's and
/// Pairwise's one lane; refused for a lane count the kind does not commit, and for ExactMatch.
pub fn palw_improve_eval_score_value_v1(kind: PalwScoringKindV1, score: &[i32]) -> Result<i64, PalwEvalErrorV1> {
    match (kind, score) {
        (PalwScoringKindV1::RefLogLik, [hi, lo]) => Ok(misaka_palw_tir::scoring::ref_loglik_join_v1(*hi, *lo)),
        (PalwScoringKindV1::Judge | PalwScoringKindV1::Pairwise, [v]) => Ok(*v as i64),
        _ => Err(PalwEvalErrorV1::Binding(format!("{} score lanes for a {:?} job", score.len(), kind))),
    }
}

// ---------------------------------------------------------------------------------------------
// The carriage: FP job version 9
// ---------------------------------------------------------------------------------------------

/// **`fp_job_id` of an evaluation job as the lane carries it**: `H64(key
/// "misaka-palw/improve/fp-eval/job-id/v1", le64(|bytes|) ‖ bytes)` over the whole borsh — the V3,
/// V4 and V5 ids' construction under the evaluation's own key. The lane's `fp_job_id_v3`
/// dispatches here at version 9.
pub fn fp_job_id_eval_carried_v1(job: &PalwFreePromptJobV3) -> Hash64 {
    let bytes = borsh::to_vec(job).expect("a free-prompt job is borsh-serializable");
    keyed64(PALW_FP_EVAL_DOMAIN_JOB_ID, &[&(bytes.len() as u64).to_le_bytes(), &bytes])
}

/// **The evaluation job a carried job is**, if it is one: version 9, its decode rules present, an
/// evaluation job its tail.
pub fn palw_fp_eval_job_v1(job: &PalwFreePromptJobV3) -> Option<&PalwEvalJobV1> {
    match &job.tail {
        Some(PalwFpJobTailV1::Eval(eval)) if job.version == PALW_FP_EVAL_VERSION && job.decode.is_some() => Some(eval),
        _ => None,
    }
}

/// **Is this claim an evaluation claim?** (RFC-0004 §7.2, spec 17's MIP-17) — the one predicate the
/// free-prompt fold asks, in exactly three places: at commitment (no quanta, weight, tickets or
/// receipts; the stage-1 reservation stays), at the lane's pricing (none), and at `Final` (the
/// subject's escrowed fee, not the lane's price). True for version 9 and for any job carrying an
/// evaluation tail, so a malformed job errs off the reward path, never onto it.
pub fn palw_fp_claim_is_evaluation_v1(job: &PalwFreePromptJobV3) -> bool {
    job.version == PALW_FP_EVAL_VERSION || matches!(job.tail, Some(PalwFpJobTailV1::Eval(_)))
}

/// The class a subject names, where the subject names one; the parent's is the line's head (state).
fn subject_class_v1(subject: &PalwEvalSubjectV1) -> Option<Hash64> {
    match subject {
        PalwEvalSubjectV1::Parent => None,
        PalwEvalSubjectV1::Candidate(class) => Some(*class),
    }
}

/// **An evaluation job's FP fields, statelessly** (RFC-0004 §7.2: executors choose nothing): version
/// 9 with an evaluation tail; public (`PublicDa`) with its prompt on the payload (user mode); greedy
/// (temperature 0, a zero seed) and a zero nonce — its identity is its evaluation job; canonical
/// decode rules, and exactly the ones its mode derives (a generating job's stops and budget, a
/// teacher-forced job's no-op rules); its kind's mode; the class its subject names. Judged kinds
/// wait for the judge set's class kind. The stateful half — the line, the epoch's item and subjects,
/// the prompt, the policy's budget, the class row — is the fold's.
pub fn palw_fp_eval_job_shape_v1(job: &PalwFreePromptJobV3) -> Result<&PalwEvalJobV1, PalwEvalErrorV1> {
    let eval = palw_fp_eval_job_v1(job).ok_or(PalwEvalErrorV1::NotAnEvalJob { version: job.version })?;
    let decode = job.decode.as_ref().expect("an evaluation job carries decode rules");
    if job.privacy_mode != crate::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA {
        return Err(PalwEvalErrorV1::JobField("an evaluation job is public: privacy mode PublicDa"));
    }
    if job.prompt_mode != crate::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER {
        return Err(PalwEvalErrorV1::JobField("an evaluation job's prompt rides its payload: prompt mode user"));
    }
    if job.temperature_q != crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY
        || job.sampling_seed != crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY
    {
        return Err(PalwEvalErrorV1::JobField("an evaluation job decodes greedily: temperature 0 and a zero seed"));
    }
    if job.job_nonce != [0u8; 32] {
        return Err(PalwEvalErrorV1::JobField("an evaluation job is named by its evaluation job: a zero nonce"));
    }
    decode.validate_canonical().map_err(|_| PalwEvalErrorV1::JobField("the decode rules are not canonical"))?;
    match (eval.kind, &eval.mode) {
        (PalwScoringKindV1::ExactMatch, PalwEvalModeV1::Generate { max_new, stop_ids, .. }) => {
            if *max_new == 0 || job.decode_token_limit != *max_new {
                return Err(PalwEvalErrorV1::JobField("a generating job's decode limit is its budget, at least one id"));
            }
            if *decode != palw_improve_eval_decode_config_v1(stop_ids) {
                return Err(PalwEvalErrorV1::JobField("a generating job's decode rules are the ones its stops derive"));
            }
        }
        (PalwScoringKindV1::RefLogLik, PalwEvalModeV1::TeacherForced { .. }) => {
            if !decode.is_noop() {
                return Err(PalwEvalErrorV1::JobField("a teacher-forced job selects nothing: the no-op decode rules"));
            }
        }
        (PalwScoringKindV1::Judge | PalwScoringKindV1::Pairwise, PalwEvalModeV1::Judged { .. }) => {
            return Err(PalwEvalErrorV1::JudgedNotYet);
        }
        _ => return Err(PalwEvalErrorV1::KindMismatch),
    }
    if let Some(class) = subject_class_v1(&eval.subject)
        && class != job.class_id
    {
        return Err(PalwEvalErrorV1::JobField("the job's class is the class its subject names"));
    }
    Ok(eval)
}

/// **What an evaluation claim's payload carries after the FP payload's bytes**: the stream stage's
/// ids and the committed score — on chain, outside the signature, bound by the committed roots
/// ([`palw_fp_eval_claim_check_v1`]). Public at acceptance, so nobody can withhold what a later
/// job's `FinalizedOutput` or the ExactMatch fold reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalClaimTailV1 {
    pub generated: Vec<u32>,
    pub score: Vec<i32>,
}

/// **An evaluation claim's payload bytes**: the FP payload's borsh, then the tail's. An FP
/// payload decoder (`borsh::from_slice`) refuses them as undecodable — every build below the fence
/// refuses an evaluation claim at its door.
pub fn palw_fp_eval_payload_encode_v1(payload: &PalwFpCommitmentTxPayloadV3, tail: &PalwEvalClaimTailV1) -> Vec<u8> {
    let mut out = borsh::to_vec(payload).expect("an FP payload serializes");
    out.extend(borsh::to_vec(tail).expect("a claim tail serializes"));
    out
}

/// **An evaluation claim's payload, decoded**: an FP payload whose job is an evaluation job, then
/// the tail, then nothing.
pub fn palw_fp_eval_payload_decode_v1(bytes: &[u8]) -> Result<(PalwFpCommitmentTxPayloadV3, PalwEvalClaimTailV1), PalwEvalErrorV1> {
    let mut reader = bytes;
    let payload = <PalwFpCommitmentTxPayloadV3 as BorshDeserialize>::deserialize_reader(&mut reader)
        .map_err(|_| PalwEvalErrorV1::Carriage("the FP payload does not decode"))?;
    if palw_fp_eval_job_v1(&payload.commitment.job).is_none() {
        return Err(PalwEvalErrorV1::NotAnEvalJob { version: payload.commitment.job.version });
    }
    let tail = <PalwEvalClaimTailV1 as BorshDeserialize>::deserialize_reader(&mut reader)
        .map_err(|_| PalwEvalErrorV1::Carriage("the claim tail does not decode"))?;
    if !reader.is_empty() {
        return Err(PalwEvalErrorV1::Carriage("bytes follow the claim tail"));
    }
    Ok((payload, tail))
}

/// **An evaluation claim's commitment against its tail** — the job's shape
/// ([`palw_fp_eval_job_shape_v1`]), then: the tail's ids hash to the committed `output_root`, one per
/// executed decode position and within the job's limit; the score has the kind's lanes; no schedule
/// root; and the committed `execution_root` is the one the job, the class, the leaves, the step root
/// (`trace_root`), the ids, `finalized_roots` and the score produce. `finalized_roots` is the output
/// roots of the final claims the job reads, as the fold derives them (empty for a job that reads
/// none).
pub fn palw_fp_eval_claim_check_v1<'a>(
    commitment: &'a PalwFreePromptCommitmentV3,
    tail: &PalwEvalClaimTailV1,
    finalized_roots: &[Hash64],
) -> Result<&'a PalwEvalJobV1, PalwEvalErrorV1> {
    let eval = palw_fp_eval_job_shape_v1(&commitment.job)?;
    let job = &commitment.job;
    if tail.generated.is_empty()
        || tail.generated.len() as u64 != commitment.decode_tokens_executed as u64
        || commitment.decode_tokens_executed > job.decode_token_limit
    {
        return Err(PalwEvalErrorV1::Carriage("the tail's ids are the executed positions, at least one, within the limit"));
    }
    // The FP lane's one encoding per executed count, kept.
    let at_limit = commitment.decode_tokens_executed == job.decode_token_limit;
    if at_limit != (commitment.stop_reason == crate::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached) {
        return Err(PalwEvalErrorV1::Carriage("the stop reason is not the executed count's"));
    }
    if palw_improve_eval_generated_root_v1(&tail.generated) != commitment.output_root {
        return Err(PalwEvalErrorV1::Carriage("the tail's ids do not hash to the committed output_root"));
    }
    if tail.score.len() != palw_improve_eval_score_lanes_v1(eval.kind) {
        return Err(PalwEvalErrorV1::Carriage("the tail's score has not its kind's lanes"));
    }
    if commitment.schedule_root != Hash64::default() {
        return Err(PalwEvalErrorV1::Carriage("an evaluation claim has no schedule root"));
    }
    let root = palw_improve_eval_execution_root_v1(
        &eval.id(),
        &job.class_id,
        commitment.work_leaves,
        &commitment.trace_root,
        &commitment.output_root,
        &palw_improve_eval_finalized_root_v1(finalized_roots),
        &tail.score,
    );
    if root != commitment.execution_root {
        return Err(PalwEvalErrorV1::Carriage("the committed execution_root is not the one the claim's parts produce"));
    }
    Ok(eval)
}

// ---------------------------------------------------------------------------------------------
// The job table and open claiming
// ---------------------------------------------------------------------------------------------

/// **The claim that took a job.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalClaimRefV1 {
    pub claim_id: Hash64,
    pub executor: PalwBondKeyV2,
    pub accepted_daa: u64,
    /// An ExactMatch job's answer: the hash of the generation's answer span
    /// ([`palw_improve_answer_of_v1`]), or `None` when it has none — kept at acceptance, read at the
    /// key's reveal. `None` for every other kind.
    pub answer: Option<Hash64>,
    /// Set when the claim is final: then the score is recorded.
    pub final_daa: Option<u64>,
    pub score: Option<i64>,
}

/// **A job's row** in `improvement_eval_jobs` (delta entries 100–103), keyed by the job id.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalJobStateV1 {
    pub job: PalwEvalJobV1,
    /// The job's evaluation fee, escrowed: paid to the claim at `Final`, refunded at `t_score` if none.
    pub fee: u64,
    pub claim: Option<PalwEvalClaimRefV1>,
}

/// **Open claiming** (RFC-0004 §7.2): the first valid claim per job in the accepting chain's order is
/// the one; a second is refused by name; none is accepted at or after `t_eval`.
pub fn palw_improve_eval_take_v1(
    row: &mut PalwEvalJobStateV1,
    claim_id: Hash64,
    executor: PalwBondKeyV2,
    daa: u64,
    t_eval: u64,
    answer: Option<Hash64>,
) -> Result<(), PalwEvalErrorV1> {
    if daa >= t_eval {
        return Err(PalwEvalErrorV1::PastEval { t_eval });
    }
    if let Some(c) = &row.claim {
        return Err(PalwEvalErrorV1::Taken { claim: c.claim_id, by_daa: c.accepted_daa });
    }
    row.claim = Some(PalwEvalClaimRefV1 { claim_id, executor, accepted_daa: daa, answer, final_daa: None, score: None });
    Ok(())
}

/// The job's recorded score, when its claim is final.
pub fn palw_improve_eval_score_v1(row: &PalwEvalJobStateV1) -> Option<i64> {
    row.claim.and_then(|c| c.final_daa.and(c.score))
}

// ---------------------------------------------------------------------------------------------
// ExactMatch at the key's reveal
// ---------------------------------------------------------------------------------------------

/// **The answer span of a generation** — the library's ExactMatch rule
/// (`misaka_palw_tir::scoring::exact_match_reference_v1`) up to the comparison: from after the first
/// `open` id (the start when `open < 0`) to before the first `close` id at or after it (the end when
/// `close < 0`); `None` when a delimiter the rule needs is absent.
pub fn palw_improve_answer_span_v1(generated: &[u32], open: i32, close: i32) -> Option<&[u32]> {
    let start = if open < 0 { 0 } else { generated.iter().position(|t| *t as i64 == open as i64)? + 1 };
    let end = if close < 0 { generated.len() } else { start + generated[start..].iter().position(|t| *t as i64 == close as i64)? };
    Some(&generated[start..end])
}

/// `H64(key "misaka-palw/improve/answer-span/v1", le32 n ‖ le32 ids)` — of an answer span, or of a
/// revealed key: equal exactly when the ids are.
pub fn palw_improve_answer_span_hash_v1(span: &[u32]) -> Hash64 {
    keyed64(PALW_IMPROVE_ANSWER_SPAN_DOMAIN_V1, &[&ids_bytes(span)])
}

/// **What an ExactMatch job's row keeps at acceptance** ([`PalwEvalClaimRefV1::answer`]): the answer
/// span's hash, or `None` when the generation has none — the 64 bytes the key's reveal compares with.
pub fn palw_improve_answer_of_v1(generated: &[u32], open: i32, close: i32) -> Option<Hash64> {
    palw_improve_answer_span_v1(generated, open, close).map(palw_improve_answer_span_hash_v1)
}

/// **An ExactMatch score at the key's reveal** (RFC-0004 §7.1, §7.3): 1 when the final claim's answer
/// span is the key, 0 when it is not or there is none — `exact_match_reference_v1`'s answer, which
/// the library's stage computes too (the golden vectors). `None` when the row is not an ExactMatch
/// job's or its claim is not final: a missing evaluation.
pub fn palw_improve_exact_match_score_v1(row: &PalwEvalJobStateV1, key: &[u32]) -> Option<i64> {
    if row.job.kind != PalwScoringKindV1::ExactMatch {
        return None;
    }
    let claim = row.claim.as_ref()?;
    claim.final_daa?;
    Some((claim.answer == Some(palw_improve_answer_span_hash_v1(key))) as i64)
}

// ---------------------------------------------------------------------------------------------
// Outcomes: a missing evaluation favours the incumbent
// ---------------------------------------------------------------------------------------------

/// **An item's outcome for a subject against the parent** (spec 17 §17.9; the coordinator's decision
/// of 2026-09-29): a missing subject score is a loss, and so is a missing parent score (a parent
/// win) — missing data blocks a promotion and never makes one. With both: ExactMatch pass against
/// fail; RefLogLik and Judge the sign of the difference. Pairwise's score is already the subject's
/// outcome (R's order and the margin applied in its stage), so its parent score is not read: +1 a
/// win, −1 a loss, 0 a tie; a missing one a loss.
pub fn palw_improve_item_outcome_v1(kind: PalwScoringKindV1, parent: Option<i64>, subject: Option<i64>) -> PalwItemOutcomeV1 {
    use std::cmp::Ordering;
    let Some(c) = subject else { return PalwItemOutcomeV1::Loss };
    if kind == PalwScoringKindV1::Pairwise {
        return match c.cmp(&0) {
            Ordering::Greater => PalwItemOutcomeV1::Win,
            Ordering::Less => PalwItemOutcomeV1::Loss,
            Ordering::Equal => PalwItemOutcomeV1::Tie,
        };
    }
    let Some(h) = parent else { return PalwItemOutcomeV1::Loss };
    match c.cmp(&h) {
        Ordering::Greater => PalwItemOutcomeV1::Win,
        Ordering::Less => PalwItemOutcomeV1::Loss,
        Ordering::Equal => PalwItemOutcomeV1::Tie,
    }
}

// ---------------------------------------------------------------------------------------------
// Cost: capacity, the epoch's budget, the fees a candidate escrows
// ---------------------------------------------------------------------------------------------

/// **An evaluation claim's reservation** (RFC-0004 §13, PALW-MIP-20): ADR-0160's stage-1 rule,
/// `⌈w / ρ⌉`, over the job's step leaves — the work its seats replay (`ρ = 0` reads as 1). An
/// evaluation claim earns no weight; it holds capacity as any claim does.
pub fn palw_improve_eval_reservation_v1(step_leaves: u64, rho: u32) -> u128 {
    (step_leaves as u128).div_ceil(rho.max(1) as u128)
}

/// **An epoch's evaluation positions** (RFC-0004 §13): `n` items for the parent and each of `k`
/// candidates (and the head's predecessor when `previous`), at `positions_per_job`, plus the judged
/// jobs at `judge_positions` — what the policy's cap and the network's ceiling bound.
pub fn palw_improve_eval_budget_positions_v1(
    n: u32,
    k: u32,
    previous: bool,
    positions_per_job: u64,
    judged_jobs: u64,
    judge_positions: u64,
) -> u128 {
    let subjects = 1 + k as u128 + previous as u128;
    (n as u128)
        .saturating_mul(subjects)
        .saturating_mul(positions_per_job as u128)
        .saturating_add((judged_jobs as u128).saturating_mul(judge_positions as u128))
}

/// **The evaluation fees a candidate escrows at submission** (RFC-0004 §13): its own `n` subject
/// jobs and its pairwise jobs, at the policy's fee per job. Unexecuted fees are refunded at `t_score`.
pub fn palw_improve_candidate_eval_escrow_v1(spec: &PalwEvalSpecV1, fees: &PalwImprovementFeesV1, pairwise_jobs: u32) -> u128 {
    (spec.n as u128 + pairwise_jobs as u128).saturating_mul(fees.eval_fee_per_job as u128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_eval_seed_is_the_same_for_every_subject_and_differs_by_item() {
        let seed = Hash64::from_bytes([7u8; 64]);
        assert_eq!(palw_improve_eval_seed_v1(&seed, 3), palw_improve_eval_seed_v1(&seed, 3));
        assert_ne!(palw_improve_eval_seed_v1(&seed, 3), palw_improve_eval_seed_v1(&seed, 4));
        let line = Hash64::from_bytes([1u8; 64]);
        let em = PalwScoringKindV1::ExactMatch;
        let parent = palw_improve_eval_job_id_v1(&line, 1, 3, &PalwEvalSubjectV1::Parent, em);
        let cand = palw_improve_eval_job_id_v1(&line, 1, 3, &PalwEvalSubjectV1::Candidate(Hash64::from_bytes([2u8; 64])), em);
        assert_ne!(parent, cand);
        assert_ne!(parent, palw_improve_eval_job_id_v1(&line, 1, 3, &PalwEvalSubjectV1::Parent, PalwScoringKindV1::Judge), "the kind");
        assert_ne!(parent, palw_improve_eval_job_id_v1(&line, 2, 3, &PalwEvalSubjectV1::Parent, em), "the epoch");
        assert_ne!(
            parent,
            palw_improve_eval_job_id_v1(&Hash64::from_bytes([9u8; 64]), 1, 3, &PalwEvalSubjectV1::Parent, em),
            "the line"
        );
    }

    #[test]
    fn a_missing_evaluation_never_favours_the_challenger() {
        use PalwItemOutcomeV1::*;
        use PalwScoringKindV1::*;
        assert_eq!(palw_improve_item_outcome_v1(ExactMatch, Some(0), None), Loss, "a missing candidate");
        assert_eq!(palw_improve_item_outcome_v1(ExactMatch, None, Some(1)), Loss, "a missing parent: a parent win");
        assert_eq!(palw_improve_item_outcome_v1(ExactMatch, None, None), Loss);
        assert_eq!(palw_improve_item_outcome_v1(ExactMatch, Some(0), Some(1)), Win);
        assert_eq!(palw_improve_item_outcome_v1(ExactMatch, Some(1), Some(0)), Loss);
        assert_eq!(palw_improve_item_outcome_v1(ExactMatch, Some(1), Some(1)), Tie);
        assert_eq!(palw_improve_item_outcome_v1(RefLogLik, Some(-500), Some(-400)), Win, "a higher log-likelihood");
        assert_eq!(palw_improve_item_outcome_v1(RefLogLik, Some(-400), Some(-500)), Loss);
        assert_eq!(palw_improve_item_outcome_v1(Judge, Some(3), Some(3)), Tie);
        assert_eq!(palw_improve_item_outcome_v1(Pairwise, None, Some(1)), Win, "pairwise reads no parent score");
        assert_eq!(palw_improve_item_outcome_v1(Pairwise, None, Some(-1)), Loss);
        assert_eq!(palw_improve_item_outcome_v1(Pairwise, None, Some(0)), Tie);
        assert_eq!(palw_improve_item_outcome_v1(Pairwise, None, None), Loss);
    }

    #[test]
    fn the_first_claim_takes_a_job_and_none_after_t_eval() {
        let job = PalwEvalJobV1 {
            line_id: Hash64::from_bytes([1; 64]),
            epoch: 1,
            item: 0,
            subject: PalwEvalSubjectV1::Parent,
            kind: PalwScoringKindV1::ExactMatch,
            mode: PalwEvalModeV1::Generate { seed: Hash64::from_bytes([2; 64]), max_new: 8, stop_ids: vec![] },
        };
        let mut row = PalwEvalJobStateV1 { job, fee: 10, claim: None };
        let bond = |b: u32| PalwBondKeyV2(crate::config::premine::premine_outpoint(b));
        assert_eq!(palw_improve_eval_take_v1(&mut row, Hash64::from_bytes([3; 64]), bond(1), 100, 200, None), Ok(()));
        assert_eq!(
            palw_improve_eval_take_v1(&mut row, Hash64::from_bytes([4; 64]), bond(2), 101, 200, None),
            Err(PalwEvalErrorV1::Taken { claim: Hash64::from_bytes([3; 64]), by_daa: 100 })
        );
        assert_eq!(palw_improve_eval_score_v1(&row), None, "not final yet");
        let mut late = PalwEvalJobStateV1 { claim: None, ..row.clone() };
        assert_eq!(
            palw_improve_eval_take_v1(&mut late, Hash64::from_bytes([5; 64]), bond(3), 200, 200, None),
            Err(PalwEvalErrorV1::PastEval { t_eval: 200 })
        );
        let c = row.claim.as_mut().unwrap();
        c.final_daa = Some(150);
        c.score = Some(1);
        assert_eq!(palw_improve_eval_score_v1(&row), Some(1));
    }

    #[test]
    fn cost_formulas() {
        assert_eq!(palw_improve_eval_reservation_v1(10, 3), 4, "⌈10 / 3⌉");
        assert_eq!(palw_improve_eval_reservation_v1(10, 0), 10, "ρ = 0 reads as 1");
        // RFC-0004 §13's worked size: n = 400, four candidates plus the parent, 512 positions.
        assert_eq!(palw_improve_eval_budget_positions_v1(400, 4, false, 512, 0, 0), 1_024_000);
        assert_eq!(palw_improve_eval_budget_positions_v1(400, 4, true, 512, 1_600, 100), 400 * 6 * 512 + 160_000);
    }

    #[test]
    fn the_answer_span_is_the_library_rule_and_the_reveal_scores_it() {
        use misaka_palw_tir::scoring::exact_match_reference_v1;
        // Every generation over a 4-id alphabet up to length 5, every delimiter pair and a few keys:
        // the fold's span-then-compare is the library's rule, exactly.
        let keys: [&[u32]; 5] = [&[], &[1], &[1, 2], &[2, 1], &[3, 3]];
        let mut checked = 0;
        for len in 0..=5u32 {
            for code in 0..4u32.pow(len) {
                let generated: Vec<u32> = (0..len).map(|i| (code / 4u32.pow(i)) % 4).collect();
                for open in [-1, 0, 3] {
                    for close in [-1, 0, 3] {
                        let answer = palw_improve_answer_of_v1(&generated, open, close);
                        for key in keys {
                            let fold = answer == Some(palw_improve_answer_span_hash_v1(key));
                            assert_eq!(
                                fold,
                                exact_match_reference_v1(&generated, key, open as i64, close as i64),
                                "{generated:?} {open} {close} {key:?}"
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert!(checked > 40_000, "{checked}");

        // The reveal reads a final claim's answer, and nothing else.
        let job = PalwEvalJobV1 {
            line_id: Hash64::from_bytes([1; 64]),
            epoch: 1,
            item: 0,
            subject: PalwEvalSubjectV1::Parent,
            kind: PalwScoringKindV1::ExactMatch,
            mode: PalwEvalModeV1::Generate { seed: Hash64::from_bytes([2; 64]), max_new: 8, stop_ids: vec![] },
        };
        let mut row = PalwEvalJobStateV1 { job: job.clone(), fee: 1, claim: None };
        assert_eq!(palw_improve_exact_match_score_v1(&row, &[5, 6]), None, "unclaimed: missing");
        let bond = PalwBondKeyV2(crate::config::premine::premine_outpoint(1));
        let answer = palw_improve_answer_of_v1(&[9, 5, 6, 8], 9, 8);
        palw_improve_eval_take_v1(&mut row, Hash64::from_bytes([3; 64]), bond, 10, 100, answer).unwrap();
        assert_eq!(palw_improve_exact_match_score_v1(&row, &[5, 6]), None, "not final: missing");
        row.claim.as_mut().unwrap().final_daa = Some(20);
        assert_eq!(palw_improve_exact_match_score_v1(&row, &[5, 6]), Some(1));
        assert_eq!(palw_improve_exact_match_score_v1(&row, &[5]), Some(0));
        let mut no_span = row.clone();
        no_span.claim.as_mut().unwrap().answer = None;
        assert_eq!(palw_improve_exact_match_score_v1(&no_span, &[]), Some(0), "no answer span: a fail, even for an empty key");
        let mut other = row.clone();
        other.job.kind = PalwScoringKindV1::RefLogLik;
        assert_eq!(palw_improve_exact_match_score_v1(&other, &[5, 6]), None, "not an ExactMatch job");
    }

    fn eval_fp_job(eval: PalwEvalJobV1) -> PalwFreePromptJobV3 {
        let (limit, decode) = match &eval.mode {
            PalwEvalModeV1::Generate { max_new, stop_ids, .. } => (*max_new, palw_improve_eval_decode_config_v1(stop_ids)),
            _ => (3, DecodeConfigV4::NOOP),
        };
        let class = match eval.subject {
            PalwEvalSubjectV1::Candidate(c) => c,
            PalwEvalSubjectV1::Parent => Hash64::from_bytes([0x44; 64]),
        };
        PalwFreePromptJobV3 {
            version: PALW_FP_EVAL_VERSION,
            network_domain: Hash64::from_bytes([0x10; 64]),
            class_id: class,
            executor_bond: crate::config::premine::premine_outpoint(2),
            executor_pubkey: vec![7; 16],
            operator_id: Hash64::from_bytes([0x12; 64]),
            anchor_block: Hash64::from_bytes([0x13; 64]),
            anchor_daa: 99,
            job_nonce: [0; 32],
            tokenizer_id: Hash64::from_bytes([0x14; 64]),
            prompt_token_ids_hash: Hash64::from_bytes([0x15; 64]),
            prompt_tokens: 3,
            decode_token_limit: limit,
            max_context_tokens: 64,
            privacy_mode: crate::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: crate::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: [0; 32],
            temperature_q: 0,
            decode: Some(decode),
            tail: Some(PalwFpJobTailV1::Eval(Box::new(eval))),
        }
    }

    fn generating(subject: PalwEvalSubjectV1) -> PalwEvalJobV1 {
        PalwEvalJobV1 {
            line_id: Hash64::from_bytes([1; 64]),
            epoch: 2,
            item: 5,
            subject,
            kind: PalwScoringKindV1::ExactMatch,
            mode: PalwEvalModeV1::Generate { seed: Hash64::from_bytes([2; 64]), max_new: 6, stop_ids: vec![9, 4, 9] },
        }
    }

    #[test]
    fn version_9_carries_an_evaluation_job_and_only_its_own_rules_admit_it() {
        let cand = Hash64::from_bytes([0x22; 64]);
        let job = eval_fp_job(generating(PalwEvalSubjectV1::Candidate(cand)));
        // Its bytes: the V4 job's, then the evaluation job; they round-trip, and the version names the tail.
        let bytes = borsh::to_vec(&job).unwrap();
        assert_eq!(bytes[..2], PALW_FP_EVAL_VERSION.to_le_bytes());
        let back: PalwFreePromptJobV3 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, job);
        assert!(back.is_eval() && !back.is_v4() && !back.is_v5());
        let v4_bytes =
            borsh::to_vec(&PalwFreePromptJobV3 { version: crate::palw_freeprompt_v3::PALW_FP_V4_VERSION, tail: None, ..job.clone() })
                .unwrap();
        assert_eq!(bytes[2..v4_bytes.len()], v4_bytes[2..], "the V4 job's bytes, unchanged, before the tail");
        assert_eq!(borsh::to_vec(palw_fp_eval_job_v1(&job).unwrap()).unwrap(), bytes[v4_bytes.len()..], "then the evaluation job");
        // Named under its own key.
        let id = crate::palw_freeprompt_v3::fp_job_id_v3(&job);
        assert_eq!(id, fp_job_id_eval_carried_v1(&job));
        assert_ne!(id, crate::palw_freeprompt_v3::fp_job_id_v4(&job), "never an FP job's id");
        // Every FP validator refuses it by name, whichever decode rules are in force.
        for rules in [
            crate::palw_freeprompt_v3::PalwFpDecodeRulesV1::Dormant,
            crate::palw_freeprompt_v3::PalwFpDecodeRulesV1::Scheduled,
            crate::palw_freeprompt_v3::PalwFpDecodeRulesV1::Active,
        ] {
            assert_eq!(rules.check_job(&job), Err(crate::palw_freeprompt_v3::PalwFpV3Error::EvaluationJobNotHere));
        }
        assert!(matches!(
            crate::palw_fp_job_v5::PalwFreePromptJobV5::from_carried(&job),
            Err(crate::palw_fp_job_v5::PalwFpV5Error::NotAV5Job { version: PALW_FP_EVAL_VERSION })
        ));
        // The predicate: version 9, or an evaluation tail on any version — off the reward path either way.
        assert!(palw_fp_claim_is_evaluation_v1(&job));
        let v4 = PalwFreePromptJobV3 { version: crate::palw_freeprompt_v3::PALW_FP_V4_VERSION, tail: None, ..job.clone() };
        assert!(!palw_fp_claim_is_evaluation_v1(&v4));
        assert!(palw_fp_claim_is_evaluation_v1(&PalwFreePromptJobV3 { tail: job.tail.clone(), ..v4.clone() }));
        assert!(palw_fp_claim_is_evaluation_v1(&PalwFreePromptJobV3 { tail: None, ..job.clone() }));
        assert_eq!(palw_fp_eval_job_v1(&v4), None);
        // The evaluation lane's shape rule admits it.
        assert_eq!(palw_fp_eval_job_shape_v1(&job).map(|e| e.id()), Ok(palw_fp_eval_job_v1(&job).unwrap().id()));
    }

    #[test]
    fn an_evaluation_job_s_fp_fields_are_fixed() {
        let cand = Hash64::from_bytes([0x22; 64]);
        let good = eval_fp_job(generating(PalwEvalSubjectV1::Candidate(cand)));
        assert!(palw_fp_eval_job_shape_v1(&good).is_ok());
        let field = |f: &dyn Fn(&mut PalwFreePromptJobV3)| {
            let mut j = good.clone();
            f(&mut j);
            palw_fp_eval_job_shape_v1(&j).err()
        };
        let named = |e: Option<PalwEvalErrorV1>| matches!(e, Some(PalwEvalErrorV1::JobField(_)));
        assert!(named(field(&|j| j.privacy_mode = crate::palw_freeprompt_v3::PALW_FP_PRIVACY_PANEL_DA)), "public only");
        assert!(named(field(&|j| j.prompt_mode = crate::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL)), "its prompt rides");
        assert!(named(field(&|j| j.temperature_q = 1 << 24)), "greedy");
        assert!(named(field(&|j| j.sampling_seed = [1; 32])), "a zero seed");
        assert!(named(field(&|j| j.job_nonce = [1; 32])), "a zero nonce");
        assert!(named(field(&|j| j.decode_token_limit = 5)), "the budget");
        assert!(named(field(&|j| j.decode = Some(DecodeConfigV4::NOOP))), "the stops' rules");
        assert!(named(field(&|j| j.class_id = Hash64::from_bytes([0x23; 64]))), "the subject's class");
        assert_eq!(field(&|j| j.decode = None), Some(PalwEvalErrorV1::NotAnEvalJob { version: PALW_FP_EVAL_VERSION }));
        assert_eq!(field(&|j| j.version = 7), Some(PalwEvalErrorV1::NotAnEvalJob { version: 7 }));
        // The parent's class is the line's head: the stateful half checks it.
        assert!(palw_fp_eval_job_shape_v1(&eval_fp_job(generating(PalwEvalSubjectV1::Parent))).is_ok());
        // A teacher-forced job selects nothing; judged kinds wait; a kind in another kind's mode is refused.
        let forced = PalwEvalJobV1 {
            kind: PalwScoringKindV1::RefLogLik,
            mode: PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::from_bytes([3; 64]) },
            ..generating(PalwEvalSubjectV1::Parent)
        };
        let forced_job = eval_fp_job(forced.clone());
        assert!(palw_fp_eval_job_shape_v1(&forced_job).is_ok());
        let mut stopping = forced_job.clone();
        stopping.decode = Some(palw_improve_eval_decode_config_v1(&[4]));
        assert!(named(palw_fp_eval_job_shape_v1(&stopping).err()));
        let judged = PalwEvalJobV1 {
            kind: PalwScoringKindV1::Pairwise,
            mode: PalwEvalModeV1::Judged { judge: Hash64::from_bytes([6; 64]) },
            ..generating(PalwEvalSubjectV1::Parent)
        };
        assert_eq!(palw_fp_eval_job_shape_v1(&eval_fp_job(judged)).err(), Some(PalwEvalErrorV1::JudgedNotYet));
        let crossed = PalwEvalJobV1 { kind: PalwScoringKindV1::RefLogLik, ..generating(PalwEvalSubjectV1::Parent) };
        assert_eq!(palw_fp_eval_job_shape_v1(&eval_fp_job(crossed)).err(), Some(PalwEvalErrorV1::KindMismatch));
    }

    #[test]
    fn an_evaluation_claim_carries_its_ids_and_score_bound_by_its_roots() {
        use crate::palw_freeprompt_v3::{PalwFpStopReasonV3, PalwFreePromptCommitmentV3};
        let job = eval_fp_job(generating(PalwEvalSubjectV1::Parent));
        let eval = palw_fp_eval_job_v1(&job).unwrap().clone();
        let generated = vec![5u32, 6, 7];
        let step_root = Hash64::from_bytes([0x31; 64]);
        let commitment = |generated: &[u32], score: &[i32]| PalwFreePromptCommitmentV3 {
            job: job.clone(),
            trace_root: step_root,
            output_root: palw_improve_eval_generated_root_v1(generated),
            schedule_root: Hash64::default(),
            execution_root: palw_improve_eval_execution_root_v1(
                &eval.id(),
                &job.class_id,
                40,
                &step_root,
                &palw_improve_eval_generated_root_v1(generated),
                &palw_improve_eval_finalized_root_v1(&[]),
                score,
            ),
            decode_tokens_executed: generated.len() as u32,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            work_leaves: 40,
            trace_manifest_root: Hash64::from_bytes([0x32; 64]),
            trace_chunk_count: 1,
            trace_retention_daa: 1000,
        };
        let c = commitment(&generated, &[]);
        let tail = PalwEvalClaimTailV1 { generated: generated.clone(), score: vec![] };
        assert_eq!(palw_fp_eval_claim_check_v1(&c, &tail, &[]).map(|e| e.id()), Ok(eval.id()));
        // The payload: the FP payload's bytes, then the tail — which an FP decoder refuses outright.
        let payload =
            PalwFpCommitmentTxPayloadV3 { version: 1, commitment: c.clone(), prompt_token_ids: vec![1, 2, 3], signature: vec![0; 8] };
        let bytes = palw_fp_eval_payload_encode_v1(&payload, &tail);
        assert!(borsh::from_slice::<PalwFpCommitmentTxPayloadV3>(&bytes).is_err(), "an FP door refuses an evaluation claim's bytes");
        assert_eq!(palw_fp_eval_payload_decode_v1(&bytes), Ok((payload.clone(), tail.clone())));
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(matches!(palw_fp_eval_payload_decode_v1(&longer), Err(PalwEvalErrorV1::Carriage(_))));
        let mut fp_only = payload.clone();
        fp_only.commitment.job.version = crate::palw_freeprompt_v3::PALW_FP_V4_VERSION;
        fp_only.commitment.job.tail = None;
        assert!(matches!(
            palw_fp_eval_payload_decode_v1(&palw_fp_eval_payload_encode_v1(&fp_only, &tail)),
            Err(PalwEvalErrorV1::NotAnEvalJob { version: crate::palw_freeprompt_v3::PALW_FP_V4_VERSION })
        ));
        // Every part is bound.
        let refused = |c: &PalwFreePromptCommitmentV3, t: &PalwEvalClaimTailV1, f: &[Hash64]| {
            matches!(palw_fp_eval_claim_check_v1(c, t, f), Err(PalwEvalErrorV1::Carriage(_)))
        };
        assert!(refused(&c, &PalwEvalClaimTailV1 { generated: vec![5, 6, 8], score: vec![] }, &[]), "ids not the output_root's");
        assert!(refused(&c, &PalwEvalClaimTailV1 { generated: vec![5, 6], score: vec![] }, &[]), "ids not the executed count");
        assert!(
            refused(&c, &PalwEvalClaimTailV1 { generated: generated.clone(), score: vec![1] }, &[]),
            "ExactMatch commits no score"
        );
        assert!(refused(&c, &tail, &[Hash64::from_bytes([9; 64])]), "what it read is in the root");
        let mut other_leaves = c.clone();
        other_leaves.work_leaves = 41;
        assert!(refused(&other_leaves, &tail, &[]), "the leaves are in the root");
        let mut other_tree = c.clone();
        other_tree.trace_root = Hash64::from_bytes([0x33; 64]);
        assert!(refused(&other_tree, &tail, &[]), "the step root is in the root");
        let mut scheduled = c.clone();
        scheduled.schedule_root = Hash64::from_bytes([1; 64]);
        assert!(refused(&scheduled, &tail, &[]), "no schedule root");
        let mut budget = c.clone();
        budget.stop_reason = PalwFpStopReasonV3::ExactBudgetReached;
        assert!(refused(&budget, &tail, &[]), "the stop reason is the count's");
        // A binding reproduces the claim's roots.
        let roots = crate::palw_gen_worker_v1::PalwGenClaimRootsV1 {
            step_root: Hash64::default(),
            stage_roots: vec![Hash64::from_bytes([0x41; 64])],
            generated: generated.clone(),
        };
        let layout = crate::palw_tir_class_v1::PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 8,
            checkpoint_interval: 1,
            h_tile: 16,
            commit_tiles: vec![4],
            state_tiles: vec![],
        };
        let binding = PalwEvalBindingV1::of(&eval, job.class_id, &layout, &roots, 40, vec![], vec![]);
        let r = binding.claim_roots();
        let bound = PalwFreePromptCommitmentV3 {
            trace_root: r.trace_root,
            output_root: r.output_root,
            execution_root: r.execution_root,
            work_leaves: r.work_leaves,
            ..c.clone()
        };
        assert_eq!(palw_fp_eval_claim_check_v1(&bound, &tail, &[]).map(|e| e.id()), Ok(eval.id()));
        let read = PalwEvalBindingV1 { finalized: vec![vec![1, 2]], ..binding.clone() };
        assert_ne!(read.execution_root(), binding.execution_root(), "a finalized read is in the root");
        assert_eq!(read.finalized_root(), palw_improve_eval_finalized_root_v1(&[palw_improve_eval_generated_root_v1(&[1, 2])]));
    }
}
