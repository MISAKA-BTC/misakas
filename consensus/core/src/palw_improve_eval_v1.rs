//! **RFC-0004 §7.2 (A6): the evaluation job family** — dormant under `palw_improvement_v1`.
//!
//! * **The job** ([`PalwEvalJobV1`]): a governed line's epoch, a drawn item, a subject (the parent, a
//!   candidate, or the head's predecessor) and a scoring kind. Its seed ([`palw_improve_eval_seed_v1`])
//!   is the same for every subject of an item, so pairing is exact; its id
//!   ([`palw_improve_eval_job_id_v1`]) binds the line, the epoch, the item, the subject and the kind.
//! * **The context** ([`palw_improve_eval_context_v1`]): the evaluation pipeline the chain derives from
//!   the job, the subject's IR class and the policy's scoring parameters. The subject's version-1
//!   program, lifted to version 2 unchanged, is the pipeline's `Decode` stage over its own layout;
//!   the scoring library's stages follow at shapes the job fixes (ExactMatch at the policy's
//!   `max_new` and key cap; RefLogLik at the subject's `max_context` and logits row). Executors choose
//!   nothing. The pipeline's params are the subject class's own inventory
//!   (`PalwGenInventoryNamingV1::Evaluation`): its root is the class's registered `artifact_root`.
//! * **The binding** ([`PalwEvalBindingV1`]): what an evaluation claim commits — every stage's root,
//!   the generated ids and the score — and its execution root
//!   ([`palw_improve_eval_execution_root_v1`]).
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
    /// The delimiters (−1: none) and the key's pad length.
    ExactMatch {
        open: i32,
        close: i32,
        key_cap: u32,
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

/// **The evaluation job's context** (RFC-0004 §7.2): see the module doc. ExactMatch reads the
/// generated ids and the key (`Generated`, `Key`) with the delimiters as job scalars 0 and 1;
/// RefLogLik reads the decode stage's consumed rows position by position with the logit scale as job
/// scalar 0, then sums them. Judged kinds are refused until the judge set's class kind is settled.
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
        (PalwEvalStageParamsV1::ExactMatch { open, close, key_cap }, PalwEvalModeV1::Generate { seed, max_new, stop_ids }) => {
            let em = misaka_palw_tir::scoring::exact_match_v1(misaka_palw_tir::scoring::ExactMatchShapeV1 {
                gen_len: *max_new,
                key_len: key_cap,
                token_bound: subject.program.token_bound,
            })
            .map_err(scoring)?;
            let score = StageDecl {
                name: "score".into(),
                program: 1,
                trip: TripRule::Fixed { n: 1 },
                max_trip: 1,
                tokens: None,
                bind: vec![
                    Binding::JobTokens { rule: rule(TokenSource::Generated, Some(*max_new)) },
                    Binding::JobTokenCount { rule: rule(TokenSource::Generated, Some(*max_new)) },
                    Binding::JobTokens { rule: rule(TokenSource::Key, Some(key_cap)) },
                    Binding::JobTokenCount { rule: rule(TokenSource::Key, Some(key_cap)) },
                    Binding::JobScalar { index: 0 },
                    Binding::JobScalar { index: 1 },
                ],
            };
            let mut stops: Vec<Vec<u32>> = stop_ids.iter().map(|id| vec![*id]).collect();
            stops.sort();
            stops.dedup();
            let config = DecodeConfigV4 { stop_sequences: stops, ..DecodeConfigV4::NOOP };
            let decode = PalwGenDecodeV1 { config, sampling: PalwDecodeSamplingV2::GREEDY, limit: *max_new };
            let seed: [u8; 32] = seed.as_byte_slice()[..32].try_into().expect("a 64-byte hash");
            (vec![subject_stage, score], vec![lifted, em], vec![open as i64, close as i64], Some(decode), seed)
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
    })
}

// ---------------------------------------------------------------------------------------------
// The binding and the execution root
// ---------------------------------------------------------------------------------------------

/// **An evaluation claim's execution root**: `H64(key, job_id ‖ subject class ‖ le64 leaves ‖
/// step_root ‖ le32 |generated| ‖ ids ‖ le32 |score| ‖ score lanes)` — the job, the class it ran, the
/// tree, the answer and the score.
pub fn palw_improve_eval_execution_root_v1(
    job_id: &Hash64,
    subject_class: &Hash64,
    step_leaf_count: u64,
    step_root: &Hash64,
    generated: &[u32],
    score: &[i32],
) -> Hash64 {
    let mut ids = Vec::with_capacity(4 + generated.len() * 4);
    ids.extend_from_slice(&(generated.len() as u32).to_le_bytes());
    for id in generated {
        ids.extend_from_slice(&id.to_le_bytes());
    }
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
            &ids,
            &lanes,
        ],
    )
}

/// **What pins an evaluation execution** — carried by every court move of an evaluation claim.
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
    /// The decode stage's ids: decoded (Generate) or given (the reference, TeacherForced).
    pub generated: Vec<u32>,
    /// The committed score tensor.
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

    /// The execution root its parts produce.
    pub fn execution_root(&self) -> Hash64 {
        palw_improve_eval_execution_root_v1(
            &self.job.id(),
            &self.subject_class,
            self.step_leaf_count,
            &self.step_root(),
            &self.generated,
            &self.score,
        )
    }

    /// The score the fold records (`PalwEvalScoreV1::value`): ExactMatch, Judge and Pairwise its one
    /// lane; RefLogLik its `(hi, lo)` joined.
    pub fn score_value(&self) -> Result<i64, PalwEvalErrorV1> {
        match (self.job.kind, &self.score[..]) {
            (PalwScoringKindV1::RefLogLik, [hi, lo]) => Ok(misaka_palw_tir::scoring::ref_loglik_join_v1(*hi, *lo)),
            (PalwScoringKindV1::ExactMatch | PalwScoringKindV1::Judge | PalwScoringKindV1::Pairwise, [v]) => Ok(*v as i64),
            _ => Err(PalwEvalErrorV1::Binding(format!("{} score lanes for a {:?} job", self.score.len(), self.job.kind))),
        }
    }
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
) -> Result<(), PalwEvalErrorV1> {
    if daa >= t_eval {
        return Err(PalwEvalErrorV1::PastEval { t_eval });
    }
    if let Some(c) = &row.claim {
        return Err(PalwEvalErrorV1::Taken { claim: c.claim_id, by_daa: c.accepted_daa });
    }
    row.claim = Some(PalwEvalClaimRefV1 { claim_id, executor, accepted_daa: daa, final_daa: None, score: None });
    Ok(())
}

/// The job's recorded score, when its claim is final.
pub fn palw_improve_eval_score_v1(row: &PalwEvalJobStateV1) -> Option<i64> {
    row.claim.and_then(|c| c.final_daa.and(c.score))
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
        assert_eq!(palw_improve_eval_take_v1(&mut row, Hash64::from_bytes([3; 64]), bond(1), 100, 200), Ok(()));
        assert_eq!(
            palw_improve_eval_take_v1(&mut row, Hash64::from_bytes([4; 64]), bond(2), 101, 200),
            Err(PalwEvalErrorV1::Taken { claim: Hash64::from_bytes([3; 64]), by_daa: 100 })
        );
        assert_eq!(palw_improve_eval_score_v1(&row), None, "not final yet");
        let mut late = PalwEvalJobStateV1 { claim: None, ..row.clone() };
        assert_eq!(
            palw_improve_eval_take_v1(&mut late, Hash64::from_bytes([5; 64]), bond(3), 200, 200),
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
}
