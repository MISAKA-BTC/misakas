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
//! * **Outcomes** ([`palw_improve_item_outcome_v1`], the core's, spec 17 §17.9): a missing evaluation always
//!   favours the incumbent — a missing candidate score is a loss, a missing parent score a parent win.
//!   Missing data can block a promotion, never make one.
//! * **Cost** ([`palw_improve_eval_reservation_v1`]; the jobs and escrow per subject are the core's,
//!   `palw_improvement_jobs_per_subject_v1`): a claim reserves capacity by ADR-0160's stage-1
//!   rule; an epoch's positions are bounded; a candidate escrows its own jobs' fees.

use crate::Hash64;
use crate::palw_decode_pipeline_v4::DecodeConfigV4;
use crate::palw_decode_select_v2::PalwDecodeSamplingV2;
use crate::palw_freeprompt_v3::{PalwFpCommitmentTxPayloadV3, PalwFpJobTailV1, PalwFreePromptCommitmentV3, PalwFreePromptJobV3};
use crate::palw_gen_worker_v1::PalwGenDecodeV1;
use crate::palw_improve_state_v1::{PalwEvalSpecV1, PalwEvalSubjectV1, PalwScoringKindV1};
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
/// Key of [`palw_improve_eval_prompt_root_v1`].
pub const PALW_IMPROVE_EVAL_PROMPT_DOMAIN_V1: &[u8] = b"misaka-palw/improve/eval-prompt/v1";
/// Key of [`palw_improve_answer_span_hash_v1`].
pub const PALW_IMPROVE_ANSWER_SPAN_DOMAIN_V1: &[u8] = b"misaka-palw/improve/answer-span/v1";
/// **The most ids an evaluation job's stream carries** — a generating job's budget, a teacher-forced
/// job's reference: a format cap, so a binding with a pairwise job's two finalized reads fits one
/// carrier (80 KiB) whatever the policy says.
pub const PALW_IMPROVE_EVAL_MAX_STREAM_IDS_V1: u32 = 4096;

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
/// `PalwScoringParamsV1` (rfc4/core) less the revealed key's length bound, which no context reads.
/// Committed in the claim's execution root, so a court or a data-availability answer derives the
/// context from what the claim bound, whatever the policy says later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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

/// **The prompt's root** — `H64(key, le32 n ‖ le32 ids)` over the item's disclosed prompt, in the
/// claim's execution root: a court or a data-availability answer carries the ids a cone reads and
/// proves them under it.
pub fn palw_improve_eval_prompt_root_v1(prompt: &[u32]) -> Hash64 {
    keyed64(PALW_IMPROVE_EVAL_PROMPT_DOMAIN_V1, &[&ids_bytes(prompt)])
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
/// step_root ‖ prompt_root ‖ le32 prompt_tokens ‖ borsh(params) ‖ generated_root ‖ finalized_root ‖
/// le32 |score| ‖ le32 lanes)` — the job, the class it ran, the tree, the prompt (and its length, so a
/// step space derives without its ids), the stage's parameters, the answer, what it read and the
/// score: every input a context is derived from beside the class row, so the claim alone fixes its
/// context.
#[allow(clippy::too_many_arguments)]
pub fn palw_improve_eval_execution_root_v1(
    job_id: &Hash64,
    subject_class: &Hash64,
    step_leaf_count: u64,
    step_root: &Hash64,
    prompt_root: &Hash64,
    prompt_tokens: u32,
    params: &PalwEvalStageParamsV1,
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
            prompt_root.as_byte_slice(),
            &prompt_tokens.to_le_bytes(),
            &borsh::to_vec(params).expect("stage params serialize"),
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
    /// The item's prompt's root ([`palw_improve_eval_prompt_root_v1`]); the ids ride where a cone
    /// reads them, proven under it.
    pub prompt_root: Hash64,
    /// The prompt's length: what the step space's trip counts read, without the ids.
    pub prompt_tokens: u32,
    /// The scoring stage's parameters the context was derived with.
    pub params: PalwEvalStageParamsV1,
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
    #[allow(clippy::too_many_arguments)]
    pub fn of(
        job: &PalwEvalJobV1,
        subject_class: Hash64,
        subject_layout: &PalwTirLayoutV1,
        roots: &crate::palw_gen_worker_v1::PalwGenClaimRootsV1,
        step_leaf_count: u64,
        prompt: &[u32],
        params: PalwEvalStageParamsV1,
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
            prompt_root: palw_improve_eval_prompt_root_v1(prompt),
            prompt_tokens: prompt.len() as u32,
            params,
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
            &self.prompt_root,
            self.prompt_tokens,
            &self.params,
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

/// **Is a committed score in its kind's range?** (spec 17 §17.8.3, the record's own rule): Judge inside
/// the stage's `[lo, hi]`, Pairwise −1, 0 or 1, RefLogLik any value; ExactMatch commits none.
pub fn palw_improve_eval_score_in_range_v1(params: &PalwEvalStageParamsV1, value: i64) -> bool {
    match params {
        PalwEvalStageParamsV1::ExactMatch { .. } => false,
        PalwEvalStageParamsV1::RefLogLik { .. } => true,
        PalwEvalStageParamsV1::Judge { lo, hi } => (*lo as i64..=*hi as i64).contains(&value),
        PalwEvalStageParamsV1::Pairwise { .. } => (-1..=1).contains(&value),
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
        PalwEvalSubjectV1::Candidate(class) | PalwEvalSubjectV1::Previous(class) => Some(*class),
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
    if job.decode_token_limit > PALW_IMPROVE_EVAL_MAX_STREAM_IDS_V1 {
        return Err(PalwEvalErrorV1::JobField("an evaluation job's stream is at most 4,096 ids"));
    }
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
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalClaimTailV1 {
    pub generated: Vec<u32>,
    pub score: Vec<i32>,
    /// The subject's layout — the class row keeps only its digest (`layout_digest`), which the fold
    /// checks it against — so acceptance derives the claim's leaf count, and its reservation, from
    /// the chain's context rather than the executor's word.
    pub subject_layout: PalwTirLayoutV1,
    /// The scoring stage's parameters the claim ran with: the fold holds them to the policy's, and
    /// the execution root binds them.
    pub params: PalwEvalStageParamsV1,
}

/// **What an evaluation claim carries into the fold** (the `FreePromptCommitted` object's `eval`):
/// its evaluation job and its payload's tail, built by the extractor from the payload it decoded and
/// checked ([`palw_fp_eval_claim_check_v1`]) — never carried to a peer, never hashed into a root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFpEvalCarriageV1 {
    pub job: PalwEvalJobV1,
    pub tail: PalwEvalClaimTailV1,
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

/// **Is this FP payload an evaluation claim's?** Its job's version word — the payload's bytes 2..4,
/// after the payload's own version — is [`PALW_FP_EVAL_VERSION`]. Nothing else is read.
pub fn palw_fp_payload_is_eval_v1(payload: &[u8]) -> bool {
    payload.get(2..4) == Some(&PALW_FP_EVAL_VERSION.to_le_bytes()[..])
}

/// **The evaluation claim's FP stand-in**: the same payload at FP Job V4 (version 7, no tail), so
/// every rule the lane applies to a V4 commitment — the network, the signer's shape, the prompt ids
/// against their hash and form, the context, the executed count and stop, the ladder, the ruleset's
/// caps — applies to the evaluation claim unchanged, and only the job-version rule is the lane's.
fn palw_fp_eval_stand_in_v1(payload: &PalwFpCommitmentTxPayloadV3) -> PalwFpCommitmentTxPayloadV3 {
    let mut stand_in = payload.clone();
    stand_in.commitment.job.version = crate::palw_freeprompt_v3::PALW_FP_V4_VERSION;
    stand_in.commitment.job.tail = None;
    stand_in
}

fn eval_refused(e: PalwEvalErrorV1) -> crate::palw_freeprompt_v3::PalwFpV3Error {
    crate::palw_freeprompt_v3::PalwFpV3Error::EvaluationClaim(e.to_string())
}

/// **The isolation door for an evaluation claim** (RFC-0004 A6), height-free as isolation is: the
/// payload decodes as an evaluation claim's; its FP stand-in passes the lane's shape rules under the
/// same door arguments an FP commitment meets; and the claim's own stateless rules hold
/// ([`palw_fp_eval_claim_check_v1`], no finalized reads — judged kinds wait). Only where the ruleset
/// carries `Params::palw_improvement_v1`; the header-context door decides the height
/// ([`palw_fp_eval_refusal_at_v1`]).
pub fn validate_palw_fp_eval_commitment_tx_v1(
    payload: &[u8],
    panel_da_admissible: bool,
    prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    work_leaves_cap: u64,
    decode_rules: crate::palw_freeprompt_v3::PalwFpDecodeRulesV1,
) -> Result<(), crate::palw_freeprompt_v3::PalwFpV3Error> {
    let (fp, tail) = palw_fp_eval_payload_decode_v1(payload).map_err(eval_refused)?;
    palw_fp_eval_stand_in_v1(&fp).validate_shape_under_ruleset_v4(
        panel_da_admissible,
        work_leaves_cap,
        None,
        prompt_ids_form,
        decode_rules,
    )?;
    palw_fp_eval_claim_check_v1(&fp.commitment, &fp.prompt_token_ids, &tail, &[]).map_err(eval_refused)?;
    Ok(())
}

/// **The free-prompt door with evaluation claims** (RFC-0004 A6): an evaluation claim's payload goes to
/// [`validate_palw_fp_eval_commitment_tx_v1`] where `improvement_door` (the ruleset carries
/// `palw_improvement_v1`), and every other payload — an evaluation claim's too, where the door is shut —
/// to the lane's own door, which refuses its bytes as undecodable. So a build that carries the fence
/// and one that does not agree on every transaction below it.
pub fn validate_palw_fp_commitment_tx_under_v6(
    payload: &[u8],
    panel_da_admissible: bool,
    prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    work_leaves_cap: u64,
    decode_rules: crate::palw_freeprompt_v3::PalwFpDecodeRulesV1,
    improvement_door: bool,
) -> Result<(), crate::palw_freeprompt_v3::PalwFpV3Error> {
    if improvement_door && palw_fp_payload_is_eval_v1(payload) {
        return validate_palw_fp_eval_commitment_tx_v1(payload, panel_da_admissible, prompt_ids_form, work_leaves_cap, decode_rules);
    }
    crate::palw_fp_objects_v3::validate_palw_fp_commitment_tx_under_v5(
        payload,
        panel_da_admissible,
        prompt_ids_form,
        work_leaves_cap,
        decode_rules,
    )
}

/// **Why the containing block's height refuses an evaluation claim** — the header-context half of the
/// evaluation door ([`palw_fp_eval_refusal_at_v1`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFpEvalHeightRefusalV1 {
    /// Below `Params::palw_improvement_v1`: no evaluation claim exists yet.
    BelowImprovement,
    /// Below `Params::palw_fp_decode_rules`, whose V4 rules the job carries.
    BelowDecodeRules,
    /// Past the structural work-leaves cap (carrying so many leaves) below the held regime — the
    /// FP lane's own height rule (ADR-0119 Decision 6), which reads an FP payload and so never sees
    /// this one.
    WorkLeavesBeforeHeld(u64),
}

impl PalwFpEvalHeightRefusalV1 {
    /// The refusal's name, as the transaction rule error carries it.
    pub fn why(self) -> &'static str {
        match self {
            Self::BelowImprovement => "an evaluation claim (FP job version 9) below Params::palw_improvement_v1",
            Self::BelowDecodeRules => "an evaluation claim below Params::palw_fp_decode_rules, whose V4 rules its job carries",
            Self::WorkLeavesBeforeHeld(_) => "an evaluation claim past the structural work cap below the held regime",
        }
    }
}

/// **The header-context half of the evaluation door**: at the containing block's height, why an
/// evaluation claim is refused — below `palw_improvement_v1`, below the V4 decode rules its job
/// carries, or past the structural work cap below the held regime (the FP lane's own height rules,
/// which read an FP payload and so never see this one). `None` for an FP payload and for one the
/// height admits. With the isolation door's height-free answer this makes a build that schedules the
/// fence and one that does not agree on every transaction below it: the one refuses the claim here,
/// the other at its isolation door (the bytes are undecodable to it), and a block carrying it is
/// invalid to both.
pub fn palw_fp_eval_refusal_at_v1(
    payload: &[u8],
    improvement_active: bool,
    decode_rules_active: bool,
    held_active: bool,
) -> Option<PalwFpEvalHeightRefusalV1> {
    if !palw_fp_payload_is_eval_v1(payload) {
        return None;
    }
    if !improvement_active {
        return Some(PalwFpEvalHeightRefusalV1::BelowImprovement);
    }
    if !decode_rules_active {
        return Some(PalwFpEvalHeightRefusalV1::BelowDecodeRules);
    }
    if !held_active
        && let Ok((fp, _)) = palw_fp_eval_payload_decode_v1(payload)
        && fp.commitment.work_leaves > crate::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP
    {
        return Some(PalwFpEvalHeightRefusalV1::WorkLeavesBeforeHeld(fp.commitment.work_leaves));
    }
    None
}

/// **Extract one chain block's evaluation claims** (RFC-0004 A6) — the free-prompt walk's twin for
/// version-9 payloads, which the lane's walk skips as undecodable
/// (`palw_fp_objects_from_accepted_txs_by_class_v1`). Its arguments and the order of its checks are the
/// FP walk's, so an evaluation claim meets every rule an FP commitment meets, at the same bounds:
///
/// * the payload decodes as an evaluation claim's;
/// * its FP stand-in passes the stateless rules at the claim's CLASS's ladder and the ruleset's caps,
///   under the block's decode rules ([`palw_fp_eval_stand_in_v1`]);
/// * ADR-0103 Decision 4: under the held regime no `PublicDa` carrier's ids ride above one standard
///   transaction;
/// * the claim's own stateless rules hold ([`palw_fp_eval_claim_check_v1`], no finalized reads: judged
///   kinds wait) — and its signature verifies under the key it carries.
///
/// Total over whatever was accepted: a payload that fails any of them is skipped with its reason, never
/// rejected, so a peer's payload cannot invalidate the block that carried it. `derived_work` of the
/// class's caps is not read: an evaluation claim's leaves are the chain's count of its derived context
/// (the fold's rule), not the FP lane's shape profile. Each claim becomes the `FreePromptCommitted` its
/// fold branch reads, its job and tail in `eval`. The caller runs it only past `palw_improvement_v1`
/// and appends its objects after the free-prompt walk's.
#[allow(clippy::too_many_arguments)]
pub fn palw_fp_eval_objects_from_accepted_txs_v1<'a, V, C>(
    txs: &[crate::tx::Transaction],
    network_domain: Hash64,
    freeprompt: &crate::palw_freeprompt_v3::PalwFreePromptParamsV3,
    panel_da_armed: bool,
    class_caps: C,
    ruleset_caps_armed: bool,
    held_armed: bool,
    prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    decode_rules: crate::palw_freeprompt_v3::PalwFpDecodeRulesV1,
    verify_mldsa87: V,
) -> crate::palw_fp_objects_v3::PalwFpExtractionV3
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    C: Fn(&Hash64) -> crate::palw_fp_objects_v3::PalwFpClassCapsV1<'a>,
{
    let mut out = crate::palw_fp_objects_v3::PalwFpExtractionV3::default();
    for tx in txs {
        if tx.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT || !palw_fp_payload_is_eval_v1(&tx.payload) {
            continue;
        }
        let id = tx.id();
        let Ok((payload, tail)) = palw_fp_eval_payload_decode_v1(&tx.payload) else {
            out.skipped.push((id, "evaluation payload does not decode"));
            continue;
        };
        // The same bounds the FP walk applies, at the commitment's own class.
        let caps = class_caps(&payload.commitment.job.class_id);
        let ruleset_caps = ruleset_caps_armed.then(|| {
            let prompt_cap = if caps.held {
                crate::palw_freeprompt_v3::PALW_FP_HELD_MAX_PROMPT_TOKENS_V1
            } else {
                freeprompt.max_prompt_tokens()
            };
            (prompt_cap, freeprompt.max_decode_tokens())
        });
        if palw_fp_eval_stand_in_v1(&payload)
            .validate_stateless_under_ruleset_v4(
                network_domain,
                panel_da_armed,
                caps.step_ladder,
                ruleset_caps,
                prompt_ids_form,
                decode_rules,
            )
            .is_err()
        {
            out.skipped.push((id, "evaluation payload is not stateless-admissible"));
            continue;
        }
        if held_armed && crate::palw_fp_objects_v3::palw_fp_public_ids_exceed_one_transaction_v1(&payload) {
            out.skipped.push((id, "the held regime carries no prompt above one standard transaction"));
            continue;
        }
        let Ok(job) = palw_fp_eval_claim_check_v1(&payload.commitment, &payload.prompt_token_ids, &tail, &[]).cloned() else {
            out.skipped.push((id, "evaluation claim's tail is not its commitment's"));
            continue;
        };
        if payload.validate_signature_v3(&verify_mldsa87).is_err() {
            out.skipped.push((id, "commitment signature does not verify under the carried key"));
            continue;
        }
        let commitment = &payload.commitment;
        out.objects.push(crate::palw_fp_objects_v3::PalwConsensusObjectV3Carrier {
            carrier: id,
            object: crate::palw_state_v2::PalwConsensusObjectV2::FreePromptCommitted {
                claim: payload.claim_id(),
                class_id: commitment.job.class_id,
                bond: crate::palw_state_v2::PalwBondKeyV2(commitment.job.executor_bond),
                executor_pubkey: commitment.job.executor_pubkey.clone(),
                work_leaves: commitment.work_leaves,
                prompt_token_ids_hash: commitment.job.prompt_token_ids_hash,
                prompt_tokens: commitment.job.prompt_tokens,
                prompt_token_ids: payload.prompt_token_ids.clone(),
                decode_tokens_executed: commitment.decode_tokens_executed,
                trace_root: commitment.trace_root,
                output_root: commitment.output_root,
                execution_root: commitment.execution_root,
                trace_chunk_count: commitment.trace_chunk_count,
                trace_retention_daa: commitment.trace_retention_daa,
                consumed_prefix_state: payload.consumed_prefix_state_v1(),
                job_pin: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(commitment),
                eval: Some(Box::new(PalwFpEvalCarriageV1 { job, tail })),
            },
        });
    }
    out
}

/// **An evaluation claim's commitment against its tail** — the job's shape
/// ([`palw_fp_eval_job_shape_v1`]), then: the tail's stage parameters are its kind's; its ids hash to
/// the committed `output_root`, one per executed decode position and within the job's limit; the
/// score has the kind's lanes; no schedule root and the DA trio; and the committed `execution_root` is
/// the one the job, the class, the leaves, the step root (`trace_root`), the payload's `prompt`, the
/// parameters, the ids, `finalized_roots` and the score produce. `finalized_roots` is the output
/// roots of the final claims the job reads, as the fold derives them (empty for a job that reads
/// none).
pub fn palw_fp_eval_claim_check_v1<'a>(
    commitment: &'a PalwFreePromptCommitmentV3,
    prompt: &[u32],
    tail: &PalwEvalClaimTailV1,
    finalized_roots: &[Hash64],
) -> Result<&'a PalwEvalJobV1, PalwEvalErrorV1> {
    let eval = palw_fp_eval_job_shape_v1(&commitment.job)?;
    if tail.params.kind() != eval.kind {
        return Err(PalwEvalErrorV1::KindMismatch);
    }
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
    // The DA trio (Phase F's pipeline-claim units): one chunk and no manifest — the units open step
    // leaves and nodes under `trace_root`; retention is the chain's (`accepted_daa` + the minimum).
    if commitment.trace_chunk_count != 1 || commitment.trace_manifest_root != Hash64::default() {
        return Err(PalwEvalErrorV1::Carriage("an evaluation claim serves one chunk and names no manifest"));
    }
    let root = palw_improve_eval_execution_root_v1(
        &eval.id(),
        &job.class_id,
        commitment.work_leaves,
        &commitment.trace_root,
        &palw_improve_eval_prompt_root_v1(prompt),
        prompt.len() as u32,
        &tail.params,
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

/// **A job's row** in `improvement_eval_jobs`, keyed by [`PalwEvalJobKeyV1`] and written at its
/// first claim (lazily: a draw writes none). Its fee is the subject's escrow's, paid at `Final`
/// (spec 17 §17.11.2), so the row holds none.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalJobStateV1 {
    pub job: PalwEvalJobV1,
    pub claim: Option<PalwEvalClaimRefV1>,
}

/// **An evaluation job's key in `improvement_eval_jobs`**: `(line, epoch, item, subject, kind)` — the
/// id's inputs, in an order whose ranges are an epoch's and an item's jobs.
pub type PalwEvalJobKeyV1 = (Hash64, u64, u32, PalwEvalSubjectV1, PalwScoringKindV1);

impl PalwEvalJobV1 {
    /// The job's key in `improvement_eval_jobs`.
    pub fn key(&self) -> PalwEvalJobKeyV1 {
        (self.line_id, self.epoch, self.item, self.subject, self.kind)
    }
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

/// **A row whose claim is gone** (voided, or retired before `Final`) takes a new claim: the row's
/// claim is cleared when the chain no longer holds it live or final, so a dead claim never holds a
/// job past its own life. `live` answers for the claim id.
pub fn palw_improve_eval_row_release_dead_v1(row: &mut PalwEvalJobStateV1, live: impl Fn(&Hash64) -> bool) {
    if let Some(c) = &row.claim
        && c.final_daa.is_none()
        && !live(&c.claim_id)
    {
        row.claim = None;
    }
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

/// **An item's outcome** is the core's one function (spec 17 §17.9.1: a missing evaluation counts for
/// the incumbent; Pairwise's recorded value is already the outcome), re-exported here where the
/// evaluation lane defined it first.
pub use crate::palw_improve_promotion_v1::palw_improve_item_outcome_v1;

// ---------------------------------------------------------------------------------------------
// Cost: capacity, the epoch's budget, the fees a candidate escrows
// ---------------------------------------------------------------------------------------------

/// **An evaluation claim's reservation** (RFC-0004 §13, PALW-MIP-20): what a claim of the same work
/// on the class reserves — its step leaves at the class's `slash_value_per_pwu`, the leaves era's
/// `pwu × slash` — under ADR-0160's stage-1 rule, `⌈w / ρ⌉` (`ρ = 0` reads as 1). An evaluation claim
/// earns no weight; it holds collateral as any claim does, so a false one is slashed like any other.
pub fn palw_improve_eval_reservation_v1(step_leaves: u64, slash_value_per_pwu: u64, rho: u32) -> u128 {
    (step_leaves as u128).saturating_mul(slash_value_per_pwu as u128).div_ceil(rho.max(1) as u128)
}

/// **An epoch's evaluation budget, in positions** (spec 17 §17.8.2, PALW-MIP-20; RFC-0004 §13 and open
/// question 7): the smaller of the policy's `max_eval_positions` and the network's ceiling
/// `max_eval_positions_per_epoch` taken at its `max_eval_budget_permille` — the one reading of "a share
/// of the span's claim capacity" the chain can compute, because no chapter defines a capacity in positions:
/// v1 takes the ceiling itself as the span's capacity. A network that wants the evaluation budget tighter
/// lowers either ceiling.
pub fn palw_improve_eval_budget_positions_v1(
    policy_max_eval_positions: u64,
    ceilings: &crate::palw_improve_v1::PalwImprovementCeilingsV1,
) -> u64 {
    let share = (ceilings.max_eval_positions_per_epoch as u128 * ceilings.max_eval_budget_permille as u128 / 1_000) as u64;
    policy_max_eval_positions.min(ceilings.max_eval_positions_per_epoch).min(share)
}

/// **The most evaluation jobs an epoch can hold**: the claimable ones — one per drawn item for each
/// subject's primary kind (an item is exact-key or likelihood, never both), one more per item for a Judge
/// stage, and one per item and non-parent subject for a Pairwise stage — over `subjects` subjects (the
/// parent counts). An ExactMatch key's scoring is the fold's, not a claim, and a suite's items have no
/// disclosed prompt (§17.8.4), so neither adds a job; `n` bounds the drawn items.
pub fn palw_improve_eval_epoch_jobs_v1(eval: &PalwEvalSpecV1, subjects: usize) -> u64 {
    use crate::palw_improve_policy_v1::palw_improvement_has_stage_v1 as has;
    let n = eval.n as u64;
    let judge = has(eval, PalwScoringKindV1::Judge) as u64;
    let pairwise = has(eval, PalwScoringKindV1::Pairwise) as u64;
    let subjects = subjects as u64;
    n.saturating_mul(1 + judge)
        .saturating_mul(subjects)
        .saturating_add(n.saturating_mul(pairwise).saturating_mul(subjects.saturating_sub(1)))
}

/// **The positions one evaluation job may take**: the epoch's budget shared equally among the epoch's
/// jobs (rounded down; a job of at least one position always fits a budget of at least one job's worth).
/// Each job's cap is independent of every other claim — no order of claims can spend another job's share,
/// and a voided claim returns nothing to anyone — so the epoch's total never exceeds its budget:
/// `jobs × (budget / jobs) ≤ budget`.
pub fn palw_improve_eval_job_position_cap_v1(budget_positions: u64, epoch_jobs: u64) -> u64 {
    budget_positions / epoch_jobs.max(1)
}

/// **An evaluation claim's positions** (RFC-0004 §13's worked size): the prompt's, then the stream stage's
/// — the ids it generated, or the reference it was given.
pub fn palw_improve_eval_positions_v1(prompt_len: usize, stream_len: usize) -> u64 {
    (prompt_len as u64).saturating_add(stream_len as u64)
}

/// **A layout's digest**, as an IR class id binds it (`PalwTirClassV1::layout_digest`): what the fold
/// holds a claim's carried layout to, against the class row's `layout_digest`.
pub fn palw_improve_eval_layout_digest_v1(layout: &PalwTirLayoutV1) -> Hash64 {
    keyed64(crate::palw_tir_class_v1::PALW_TIR_LAYOUT_DOMAIN_V1, &[&borsh::to_vec(layout).expect("a layout is borsh-serializable")])
}

/// **The job facts of an evaluation claim**: the item's prompt, the stream's ids, what the job read,
/// and the context's scalars — the job a court, a seat and acceptance run the context over.
pub fn palw_improve_eval_pipeline_job_v1(
    ctx: &PalwEvalContextV1,
    prompt: &[u32],
    generated: &[u32],
    finalized: &[Vec<u32>],
) -> misaka_palw_tir::pipeline::PipelineJob {
    misaka_palw_tir::pipeline::PipelineJob {
        prompt: prompt.to_vec(),
        generated: generated.to_vec(),
        scalars: ctx.scalars.clone(),
        finalized: finalized.iter().enumerate().map(|(c, ids)| ((c as u8, 0u8), ids.clone())).collect(),
        ..Default::default()
    }
}

/// **An evaluation claim's step space** (spec 04b §15; Phase F's pipeline-claim units descend it): the
/// context's stages at the trips the job's facts give, the stream stage's logits consumed from
/// `|prompt| − 1`.
pub fn palw_improve_eval_step_space_v1(
    ctx: &PalwEvalContextV1,
    prompt: &[u32],
    generated: &[u32],
    finalized: &[Vec<u32>],
) -> Result<crate::palw_gen_step_v1::PalwGenStepSpaceV1, PalwEvalErrorV1> {
    let job = palw_improve_eval_pipeline_job_v1(ctx, prompt, generated, finalized);
    let facts = misaka_palw_tir::pipeline::stage_job_facts(&ctx.pipeline, &ctx.programs, &job)
        .map_err(|e| PalwEvalErrorV1::Binding(e.to_string()))?;
    let trips: Vec<u32> = facts.iter().map(|f| f.trip).collect();
    crate::palw_gen_step_v1::PalwGenStepSpaceV1::new(&ctx.pipeline, &ctx.programs, &ctx.layouts, &trips, prompt.len() as u32)
        .map_err(|e| PalwEvalErrorV1::Binding(e.to_string()))
}

/// **An evaluation claim's step space from its binding alone**: the prompt enters the space only by
/// its length (the stream stage's trip, its first consumed row), so a data-availability answer derives
/// it from `binding.prompt_tokens` without carrying the ids (any ids of that length give one space).
pub fn palw_improve_eval_step_space_of_binding_v1(
    ctx: &PalwEvalContextV1,
    binding: &PalwEvalBindingV1,
) -> Result<crate::palw_gen_step_v1::PalwGenStepSpaceV1, PalwEvalErrorV1> {
    let placeholder = vec![0u32; binding.prompt_tokens as usize];
    palw_improve_eval_step_space_v1(ctx, &placeholder, &binding.generated, &binding.finalized)
}

/// **An evaluation claim's leaf count, in closed form** — what [`palw_improve_eval_step_space_v1`]
/// enumerates, without enumerating (`PalwGenStepSpaceV1::leaf_count_v1`): acceptance holds a claim's
/// `work_leaves` to it, so its reservation is the chain's number.
pub fn palw_improve_eval_step_leaves_v1(
    ctx: &PalwEvalContextV1,
    prompt: &[u32],
    generated: &[u32],
    finalized: &[Vec<u32>],
) -> Result<u128, PalwEvalErrorV1> {
    let job = palw_improve_eval_pipeline_job_v1(ctx, prompt, generated, finalized);
    let facts = misaka_palw_tir::pipeline::stage_job_facts(&ctx.pipeline, &ctx.programs, &job)
        .map_err(|e| PalwEvalErrorV1::Binding(e.to_string()))?;
    let trips: Vec<u32> = facts.iter().map(|f| f.trip).collect();
    crate::palw_gen_step_v1::PalwGenStepSpaceV1::leaf_count_v1(
        &ctx.pipeline,
        &ctx.programs,
        &ctx.layouts,
        &trips,
        None,
        prompt.len() as u32,
    )
    .map_err(|e| PalwEvalErrorV1::Binding(e.to_string()))
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
        use crate::palw_improve_state_v1::PalwItemOutcomeV1::*;
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
        let mut row = PalwEvalJobStateV1 { job, claim: None };
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
        assert_eq!(palw_improve_eval_reservation_v1(10, 1, 3), 4, "⌈10 / 3⌉");
        assert_eq!(palw_improve_eval_reservation_v1(10, 7, 3), 24, "⌈10 · 7 / 3⌉");
        assert_eq!(palw_improve_eval_reservation_v1(10, 1, 0), 10, "ρ = 0 reads as 1");
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
        let mut row = PalwEvalJobStateV1 { job: job.clone(), claim: None };
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
            PalwEvalSubjectV1::Candidate(c) | PalwEvalSubjectV1::Previous(c) => c,
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
        let layout = crate::palw_tir_class_v1::PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 8,
            checkpoint_interval: 1,
            h_tile: 16,
            commit_tiles: vec![4],
            state_tiles: vec![],
        };
        let params = PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 };
        let prompt = vec![1u32, 2, 3];
        let tail_of =
            |generated: Vec<u32>, score: Vec<i32>| PalwEvalClaimTailV1 { generated, score, subject_layout: layout.clone(), params };
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
                &palw_improve_eval_prompt_root_v1(&prompt),
                prompt.len() as u32,
                &params,
                &palw_improve_eval_generated_root_v1(generated),
                &palw_improve_eval_finalized_root_v1(&[]),
                score,
            ),
            decode_tokens_executed: generated.len() as u32,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            work_leaves: 40,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: 1,
            trace_retention_daa: 1000,
        };
        let c = commitment(&generated, &[]);
        let tail = tail_of(generated.clone(), vec![]);
        assert_eq!(palw_fp_eval_claim_check_v1(&c, &prompt, &tail, &[]).map(|e| e.id()), Ok(eval.id()));
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
            matches!(palw_fp_eval_claim_check_v1(c, &prompt, t, f), Err(PalwEvalErrorV1::Carriage(_)))
        };
        assert!(
            matches!(palw_fp_eval_claim_check_v1(&c, &[1, 2, 4], &tail, &[]), Err(PalwEvalErrorV1::Carriage(_))),
            "the prompt is in the root"
        );
        let other_params = PalwEvalClaimTailV1 { params: PalwEvalStageParamsV1::ExactMatch { open: 7, close: -1 }, ..tail.clone() };
        assert!(refused(&c, &other_params, &[]), "the stage parameters are in the root");
        let other_kind = PalwEvalClaimTailV1 { params: PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 }, ..tail.clone() };
        assert_eq!(palw_fp_eval_claim_check_v1(&c, &prompt, &other_kind, &[]).err(), Some(PalwEvalErrorV1::KindMismatch));
        assert!(refused(&c, &tail_of(vec![5, 6, 8], vec![]), &[]), "ids not the output_root's");
        assert!(refused(&c, &tail_of(vec![5, 6], vec![]), &[]), "ids not the executed count");
        assert!(refused(&c, &tail_of(generated.clone(), vec![1]), &[]), "ExactMatch commits no score");
        let mut chunks = c.clone();
        chunks.trace_chunk_count = 2;
        assert!(refused(&chunks, &tail, &[]), "one chunk");
        let mut manifest = c.clone();
        manifest.trace_manifest_root = Hash64::from_bytes([0x32; 64]);
        assert!(refused(&manifest, &tail, &[]), "no manifest");
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
        let binding = PalwEvalBindingV1::of(&eval, job.class_id, &layout, &roots, 40, &prompt, params, vec![], vec![]);
        let r = binding.claim_roots();
        let bound = PalwFreePromptCommitmentV3 {
            trace_root: r.trace_root,
            output_root: r.output_root,
            execution_root: r.execution_root,
            work_leaves: r.work_leaves,
            ..c.clone()
        };
        assert_eq!(palw_fp_eval_claim_check_v1(&bound, &prompt, &tail, &[]).map(|e| e.id()), Ok(eval.id()));
        let read = PalwEvalBindingV1 { finalized: vec![vec![1, 2]], ..binding.clone() };
        assert_ne!(read.execution_root(), binding.execution_root(), "a finalized read is in the root");
        assert_eq!(read.finalized_root(), palw_improve_eval_finalized_root_v1(&[palw_improve_eval_generated_root_v1(&[1, 2])]));
    }

    // ---- the door: isolation, the header context, the walk ---------------------------------------------

    use crate::palw_freeprompt_v3::{PalwFpDecodeRulesV1, PalwFpStopReasonV3, PalwFpV3Error, PalwFreePromptParamsV3};
    use crate::palw_fp_objects_v3::{PalwFpClassCapsV1, PalwFpDerivedWorkCapV1};
    use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;

    const DOOR_PROMPT: [u32; 3] = [1, 2, 3];

    /// A complete evaluation claim as its executor would commit it: an ExactMatch generation of three
    /// ids under the evaluation job's own rules, over a public three-id prompt, with the roots a binding
    /// produces and a signature of the right shape.
    fn door_claim() -> (PalwFpCommitmentTxPayloadV3, PalwEvalClaimTailV1) {
        let mut job = eval_fp_job(generating(PalwEvalSubjectV1::Parent));
        job.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&DOOR_PROMPT);
        let eval = palw_fp_eval_job_v1(&job).unwrap().clone();
        let generated = vec![5u32, 6, 7];
        let step_root = Hash64::from_bytes([0x31; 64]);
        let params = PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 };
        let layout = PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 8,
            checkpoint_interval: 1,
            h_tile: 16,
            commit_tiles: vec![4],
            state_tiles: vec![],
        };
        let output_root = palw_improve_eval_generated_root_v1(&generated);
        let commitment = PalwFreePromptCommitmentV3 {
            trace_root: step_root,
            output_root,
            schedule_root: Hash64::default(),
            execution_root: palw_improve_eval_execution_root_v1(
                &eval.id(),
                &job.class_id,
                40,
                &step_root,
                &palw_improve_eval_prompt_root_v1(&DOOR_PROMPT),
                DOOR_PROMPT.len() as u32,
                &params,
                &output_root,
                &palw_improve_eval_finalized_root_v1(&[]),
                &[],
            ),
            decode_tokens_executed: generated.len() as u32,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            work_leaves: 40,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: 1,
            trace_retention_daa: 1000,
            job,
        };
        let payload = PalwFpCommitmentTxPayloadV3 {
            version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            commitment,
            prompt_token_ids: DOOR_PROMPT.to_vec(),
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        (payload, PalwEvalClaimTailV1 { generated, score: vec![], subject_layout: layout, params })
    }

    fn door_bytes() -> Vec<u8> {
        let (payload, tail) = door_claim();
        palw_fp_eval_payload_encode_v1(&payload, &tail)
    }

    /// The claim as the V4 lane would carry it, were it an FP commitment: the stand-in's bytes.
    fn stand_in_bytes(payload: &PalwFpCommitmentTxPayloadV3) -> Vec<u8> {
        borsh::to_vec(&palw_fp_eval_stand_in_v1(payload)).unwrap()
    }

    const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;
    const LADDER: u64 = 1 << 32;

    fn eval_door(bytes: &[u8], rules: PalwFpDecodeRulesV1) -> Result<(), PalwFpV3Error> {
        validate_palw_fp_eval_commitment_tx_v1(bytes, false, FORM, LADDER, rules)
    }

    fn v4_door(bytes: &[u8], rules: PalwFpDecodeRulesV1) -> Result<(), PalwFpV3Error> {
        crate::palw_fp_objects_v3::validate_palw_fp_commitment_tx_under_v5(bytes, false, FORM, LADDER, rules)
    }

    #[test]
    fn the_evaluation_door_admits_an_honest_claim_only_where_the_decode_rules_are_carried() {
        let bytes = door_bytes();
        assert!(palw_fp_payload_is_eval_v1(&bytes));
        // The V4 rules its job carries are the lane's: height-free `Scheduled` and `Active` admit; a
        // ruleset that never schedules them (`Dormant`) refuses the V4 shape by name, as it refuses V4.
        for rules in [PalwFpDecodeRulesV1::Scheduled, PalwFpDecodeRulesV1::Active] {
            assert_eq!(eval_door(&bytes, rules), Ok(()), "{rules:?}");
        }
        assert_eq!(eval_door(&bytes, PalwFpDecodeRulesV1::Dormant), Err(PalwFpV3Error::DecodeRulesNotArmed));
        // The routing: the door shut, an FP door refuses the bytes as undecodable — what a build without
        // the fence says; the door open, the evaluation lane's rules.
        let route = |door: bool, rules| validate_palw_fp_commitment_tx_under_v6(&bytes, false, FORM, LADDER, rules, door);
        assert_eq!(route(false, PalwFpDecodeRulesV1::Scheduled), Err(PalwFpV3Error::PayloadUndecodable));
        assert_eq!(route(false, PalwFpDecodeRulesV1::Dormant), Err(PalwFpV3Error::PayloadUndecodable));
        assert_eq!(route(true, PalwFpDecodeRulesV1::Scheduled), Ok(()));
        assert_eq!(route(true, PalwFpDecodeRulesV1::Dormant), Err(PalwFpV3Error::DecodeRulesNotArmed));
        // The same bytes without their tail are an FP payload at version 9: the lane refuses its version by
        // name, and the evaluation lane its missing tail.
        let (payload, _) = door_claim();
        let no_tail = borsh::to_vec(&payload).unwrap();
        assert!(palw_fp_payload_is_eval_v1(&no_tail));
        assert!(matches!(
            validate_palw_fp_commitment_tx_under_v6(&no_tail, false, FORM, LADDER, PalwFpDecodeRulesV1::Active, false),
            Err(PalwFpV3Error::UnsupportedVersion { got: PALW_FP_EVAL_VERSION, .. })
        ));
        assert!(matches!(
            validate_palw_fp_commitment_tx_under_v6(&no_tail, false, FORM, LADDER, PalwFpDecodeRulesV1::Active, true),
            Err(PalwFpV3Error::EvaluationClaim(_))
        ));
        // Anything that is not version 9 is the lane's, whichever way the door is set — byte for byte.
        let v4 = stand_in_bytes(&payload);
        assert!(!palw_fp_payload_is_eval_v1(&v4) && !palw_fp_payload_is_eval_v1(b"") && !palw_fp_payload_is_eval_v1(&[5, 0, 9]));
        for rules in [PalwFpDecodeRulesV1::Dormant, PalwFpDecodeRulesV1::Scheduled, PalwFpDecodeRulesV1::Active] {
            for input in [&v4[..], &b"junk"[..], &[][..]] {
                let lane = v4_door(input, rules);
                assert_eq!(validate_palw_fp_commitment_tx_under_v6(input, false, FORM, LADDER, rules, false), lane);
                assert_eq!(validate_palw_fp_commitment_tx_under_v6(input, false, FORM, LADDER, rules, true), lane);
            }
        }
    }

    /// **The stand-in meets every V4 rule**: whatever the V4 door refuses of a commitment, the evaluation
    /// door refuses of the same claim, by the same name and before its own rules read anything; and a
    /// claim the V4 door admits is refused only by the evaluation lane's own rules.
    #[test]
    fn the_stand_in_meets_every_v4_rule_the_lane_applies() {
        let (base, tail) = door_claim();
        // The honest claim passes both doors: the stand-in is exactly an FP Job V4 commitment.
        assert_eq!(v4_door(&stand_in_bytes(&base), PalwFpDecodeRulesV1::Active), Ok(()));
        assert_eq!(eval_door(&palw_fp_eval_payload_encode_v1(&base, &tail), PalwFpDecodeRulesV1::Active), Ok(()));
        type Mutation = (&'static str, fn(&mut PalwFpCommitmentTxPayloadV3));
        let mutations: Vec<Mutation> = vec![
            ("the payload's own version", |p| p.version = 6),
            ("no executor key", |p| p.commitment.job.executor_pubkey.clear()),
            ("a signature of the wrong length", |p| p.signature.truncate(10)),
            ("a mode nothing implements", |p| p.commitment.job.privacy_mode = 9),
            ("a prompt mode nothing implements", |p| p.commitment.job.prompt_mode = 9),
            ("no prompt", |p| p.commitment.job.prompt_tokens = 0),
            ("no decode limit", |p| p.commitment.job.decode_token_limit = 0),
            ("a context the prompt and limit overflow", |p| p.commitment.job.max_context_tokens = 5),
            ("nothing executed", |p| p.commitment.decode_tokens_executed = 0),
            ("past the limit", |p| p.commitment.decode_tokens_executed = 99),
            ("a stop reason that is not the count's", |p| p.commitment.stop_reason = PalwFpStopReasonV3::ExactBudgetReached),
            ("no work", |p| p.commitment.work_leaves = 0),
            ("work past the ladder", |p| p.commitment.work_leaves = LADDER + 1),
            ("no chunks", |p| p.commitment.trace_chunk_count = 0),
            ("a carried prompt of another length", |p| p.prompt_token_ids.push(4)),
            ("carried ids another prompt hashes to", |p| p.prompt_token_ids[0] = 9),
            ("a prompt hash the ids do not match", |p| p.commitment.job.prompt_token_ids_hash = Hash64::from_bytes([1; 64])),
            ("a prompt that rides under another mode", |p| p.commitment.job.privacy_mode = crate::palw_freeprompt_v3::PALW_FP_PRIVACY_PANEL_DA),
        ];
        for (what, mutate) in mutations {
            let mut mutated = base.clone();
            mutate(&mut mutated);
            let lane = v4_door(&stand_in_bytes(&mutated), PalwFpDecodeRulesV1::Active);
            let evaluation = eval_door(&palw_fp_eval_payload_encode_v1(&mutated, &tail), PalwFpDecodeRulesV1::Active);
            // A mutation the V4 lane refuses, the evaluation lane refuses with the lane's own error,
            // before its own rules read anything; one the V4 lane admits, only its own rules may refuse.
            match lane {
                Err(e) => assert_eq!(evaluation, Err(e), "{what}"),
                Ok(()) => assert!(evaluation.is_err(), "{what}: the evaluation lane's own rules refuse it"),
            }
        }
        // The rules the evaluation lane adds on top, each by its own name.
        let named = |f: &dyn Fn(&mut PalwFpCommitmentTxPayloadV3, &mut PalwEvalClaimTailV1)| {
            let (mut p, mut t) = door_claim();
            f(&mut p, &mut t);
            eval_door(&palw_fp_eval_payload_encode_v1(&p, &t), PalwFpDecodeRulesV1::Active)
        };
        let refused = |r: Result<(), PalwFpV3Error>| matches!(r, Err(PalwFpV3Error::EvaluationClaim(_)));
        assert!(refused(named(&|p, _| p.commitment.job.temperature_q = 1 << 24)), "greedy");
        assert!(refused(named(&|p, _| p.commitment.job.job_nonce = [1; 32])), "a zero nonce");
        assert!(refused(named(&|_, t| t.generated[0] ^= 1)), "the ids are the output root's");
        assert!(refused(named(&|p, _| p.commitment.execution_root = Hash64::from_bytes([2; 64]))), "the execution root is the parts'");
        assert!(refused(named(&|p, _| p.commitment.trace_chunk_count = 2)), "one chunk");
        assert!(refused(named(&|_, t| t.score = vec![1])), "ExactMatch commits no score");
    }

    #[test]
    fn the_evaluation_door_is_refused_below_its_heights_by_name() {
        let bytes = door_bytes();
        let at = |improvement, decode, held| palw_fp_eval_refusal_at_v1(&bytes, improvement, decode, held);
        use PalwFpEvalHeightRefusalV1::*;
        assert_eq!(at(false, true, true), Some(BelowImprovement), "below palw_improvement_v1");
        assert_eq!(at(false, false, false), Some(BelowImprovement), "…whatever else is in force");
        assert_eq!(at(true, false, true), Some(BelowDecodeRules), "below the V4 rules its job carries");
        assert_eq!(at(true, true, false), None, "inside the structural cap the held regime is no business of it");
        assert_eq!(at(true, true, true), None, "past the fence, with the V4 rules: admitted");
        for why in [BelowImprovement, BelowDecodeRules, WorkLeavesBeforeHeld(1)] {
            assert!(!why.why().is_empty());
        }
        assert!(BelowImprovement.why().contains("palw_improvement_v1") && BelowDecodeRules.why().contains("palw_fp_decode_rules"));
        // The held regime's ladder: a claim past the structural cap needs the regime in force.
        let (mut wide, tail) = door_claim();
        wide.commitment.work_leaves = crate::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP + 1;
        let wide_bytes = palw_fp_eval_payload_encode_v1(&wide, &tail);
        assert_eq!(
            palw_fp_eval_refusal_at_v1(&wide_bytes, true, true, false),
            Some(WorkLeavesBeforeHeld(crate::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP + 1))
        );
        assert_eq!(palw_fp_eval_refusal_at_v1(&wide_bytes, true, true, true), None);
        // An FP payload, and bytes that are neither, are never this door's.
        let (payload, _) = door_claim();
        let fp = stand_in_bytes(&payload);
        for input in [&fp[..], &b"junk"[..], &[][..]] {
            assert_eq!(palw_fp_eval_refusal_at_v1(input, false, false, false), None);
        }
    }

    fn freeprompt() -> PalwFreePromptParamsV3 {
        crate::palw_fp_devnet_v3::palw_fp_devnet_bundle_for_tests(
            Hash64::from_u64_word(1),
            Hash64::from_u64_word(0xCA7),
            Hash64::from_u64_word(0xC0757),
        )
        .unwrap()
        .freeprompt
    }

    fn carrier(subnetwork: crate::subnets::SubnetworkId, payload: Vec<u8>) -> crate::tx::Transaction {
        crate::tx::Transaction::new(crate::constants::TX_VERSION, vec![], vec![], 0, subnetwork, 0, payload)
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        txs: &[crate::tx::Transaction],
        class_ladder: u64,
        held: bool,
        held_armed: bool,
        ruleset_caps_armed: bool,
        rules: PalwFpDecodeRulesV1,
        verify: bool,
    ) -> crate::palw_fp_objects_v3::PalwFpExtractionV3 {
        palw_fp_eval_objects_from_accepted_txs_v1(
            txs,
            Hash64::from_bytes([0x10; 64]),
            &freeprompt(),
            false,
            |_| PalwFpClassCapsV1 { step_ladder: class_ladder, held, derived_work: PalwFpDerivedWorkCapV1::Declared, logits_q24: true },
            ruleset_caps_armed,
            held_armed,
            FORM,
            rules,
            move |_, _, _, _| verify,
        )
    }

    #[test]
    fn the_walk_extracts_the_object_the_fold_reads_and_skips_the_rest_by_name() {
        let (payload, tail) = door_claim();
        let tx = carrier(crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT, palw_fp_eval_payload_encode_v1(&payload, &tail));
        let extraction = walk(std::slice::from_ref(&tx), 1 << 26, false, false, false, PalwFpDecodeRulesV1::Active, true);
        assert!(extraction.skipped.is_empty(), "{:?}", extraction.skipped);
        let [carried] = &extraction.objects[..] else { panic!("one object") };
        assert_eq!(carried.carrier, tx.id());
        let crate::palw_state_v2::PalwConsensusObjectV2::FreePromptCommitted {
            claim, class_id, bond, work_leaves, prompt_token_ids, decode_tokens_executed, trace_root, output_root, execution_root, eval, ..
        } = &carried.object
        else {
            panic!("a free-prompt commitment")
        };
        let commitment = &payload.commitment;
        assert_eq!(*claim, payload.claim_id(), "the id is the eval-keyed commitment's");
        assert_eq!(*claim, crate::palw_freeprompt_v3::fp_claim_id_v3(commitment));
        assert_eq!((*class_id, bond.0), (commitment.job.class_id, commitment.job.executor_bond));
        assert_eq!((*work_leaves, *decode_tokens_executed), (40, 3));
        assert_eq!((*trace_root, *output_root, *execution_root), (commitment.trace_root, commitment.output_root, commitment.execution_root));
        assert_eq!(prompt_token_ids, &DOOR_PROMPT.to_vec());
        let eval = eval.as_ref().expect("an evaluation claim carries its job and tail");
        assert_eq!((&eval.job, &eval.tail), (palw_fp_eval_job_v1(&commitment.job).unwrap(), &tail));
        // An FP commitment and a non-FP carrier are the FP walk's (and a native one nobody's): not seen here.
        let fp = carrier(crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT, stand_in_bytes(&payload));
        let native = carrier(crate::subnets::SUBNETWORK_ID_NATIVE, palw_fp_eval_payload_encode_v1(&payload, &tail));
        let ignored = walk(&[fp, native], 1 << 26, false, false, false, PalwFpDecodeRulesV1::Active, true);
        assert!(ignored.objects.is_empty() && ignored.skipped.is_empty());
        // Every other refusal is a skip, with its reason — total, never a rejection of the block.
        let skipped = |tx: crate::tx::Transaction, ladder, held, held_armed, caps, rules, verify| {
            let id = tx.id();
            let out = walk(&[tx], ladder, held, held_armed, caps, rules, verify);
            assert!(out.objects.is_empty(), "no object");
            assert_eq!(out.skipped.len(), 1);
            assert_eq!(out.skipped[0].0, id);
            out.skipped[0].1
        };
        let eval_tx = |p: &PalwFpCommitmentTxPayloadV3, t: &PalwEvalClaimTailV1| {
            carrier(crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT, palw_fp_eval_payload_encode_v1(p, t))
        };
        let active = PalwFpDecodeRulesV1::Active;
        // Bytes that start like version 9 and are nothing.
        let mut junk = vec![5, 0, 9, 0];
        junk.extend([0xAB; 40]);
        assert_eq!(
            skipped(carrier(crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT, junk), 1 << 26, false, false, false, active, true),
            "evaluation payload does not decode"
        );
        // The V4 rules below their fence: the stand-in is not admitted.
        assert_eq!(
            skipped(eval_tx(&payload, &tail), 1 << 26, false, false, false, PalwFpDecodeRulesV1::Dormant, true),
            "evaluation payload is not stateless-admissible"
        );
        // Another network's claim.
        let mut foreign = payload.clone();
        foreign.commitment.job.network_domain = Hash64::from_bytes([0x11; 64]);
        assert_eq!(
            skipped(eval_tx(&foreign, &tail), 1 << 26, false, false, false, active, true),
            "evaluation payload is not stateless-admissible"
        );
        // The class's ladder: 40 leaves against a ladder of 39.
        assert_eq!(
            skipped(eval_tx(&payload, &tail), 39, false, false, false, active, true),
            "evaluation payload is not stateless-admissible"
        );
        // The ruleset's caps, past their fence: the advertised decode cap against a job's limit.
        let mut wide = payload.clone();
        wide.commitment.job.decode_token_limit = freeprompt().max_decode_tokens() + 1;
        wide.commitment.job.max_context_tokens = u32::MAX;
        assert_eq!(
            skipped(eval_tx(&wide, &tail), 1 << 26, false, false, true, active, true),
            "evaluation payload is not stateless-admissible"
        );
        // The held regime: no public ids above one standard transaction.
        let mut big = payload.clone();
        let n = (crate::palw_mode_v2::PALW_STANDARD_TX_BYTES / 4 + 1) as usize;
        big.prompt_token_ids = (0..n as u32).collect();
        big.commitment.job.prompt_tokens = n as u32;
        big.commitment.job.max_context_tokens = n as u32 + 64;
        big.commitment.job.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&big.prompt_token_ids);
        // (Its execution root names the three-id prompt: with the held rule off the claim's own rules refuse it…)
        assert_eq!(
            skipped(eval_tx(&big, &tail), 1 << 26, false, false, false, active, true),
            "evaluation claim's tail is not its commitment's"
        );
        // …and with it on, the held rule is asked first, as the FP walk asks it.
        assert_eq!(
            skipped(eval_tx(&big, &tail), 1 << 26, false, true, false, active, true),
            "the held regime carries no prompt above one standard transaction"
        );
        // The claim's own rules.
        let mut tampered = tail.clone();
        tampered.generated[0] ^= 1;
        assert_eq!(skipped(eval_tx(&payload, &tampered), 1 << 26, false, false, false, active, true), "evaluation claim's tail is not its commitment's");
        // The signature, last.
        assert_eq!(
            skipped(eval_tx(&payload, &tail), 1 << 26, false, false, false, active, false),
            "commitment signature does not verify under the carried key"
        );
    }
}
