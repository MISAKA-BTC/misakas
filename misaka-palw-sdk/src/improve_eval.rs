//! **RFC-0004's evaluation executor (work item A10)**: an evaluation job run as the RFC-0003 pipeline
//! it is — the subject stage (the subject class, generating under the item's seed or teacher-forced
//! over the reference), then the scoring stage (A7's library) — with the claim roots the run commits
//! (`palw_gen_execute_v1`, `palw_gen_replay_committed_v1`: the one step tree of a pipeline claim) and
//! the score read off the output stage by its kind.
//!
//! **The pipeline itself is A6's**: which pipeline class an evaluation job names, how the subject's
//! IR class becomes its decode stage, and which job scalars each scoring stage reads. The executor
//! takes it as a [`PalwEvalPipelineV1`] — the pipeline, its programs and layouts, the params (the
//! subject's weights: the parent's, or a composite candidate's over the parent's inventory) — and
//! maps the job and its inputs onto the pipeline's job facts ([`palw_eval_pipeline_job_v1`]).
//!
//! **Provisional until A6**: the decode a `Generate` job runs under ([`palw_eval_decode_v1`]: the
//! canonical V4 decode, greedy, the policy's stop ids as one-id stop sequences, the budget as its
//! limit) and the generation seed `R` is keyed by ([`palw_eval_gen_seed_v1`]: the item seed's first
//! 32 bytes).

use std::collections::BTreeMap;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2;
use kaspa_consensus_core::palw_gen_worker_v1::{
    PalwGenDecodeV1, PalwGenExecutionV1, palw_gen_execute_v1, palw_gen_replay_committed_v1,
};
use kaspa_consensus_core::palw_improve_state_v1::{PalwEvalJobV1, PalwEvalModeV1, PalwScoringKindV1};
use kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1;
use misaka_palw_tir::pipeline::{PipelineJob, PipelineParams, TirPipelineV1};
use misaka_palw_tir::program_v2::TirProgramV2;

/// **An evaluation pipeline class, as the executor runs it** (A6 registers and resolves these).
pub struct PalwEvalPipelineV1<'a> {
    pub pipeline: &'a TirPipelineV1,
    pub programs: &'a [TirProgramV2],
    /// One per stage (the claim's step space).
    pub layouts: &'a [PalwTirLayoutV1],
    /// The pipeline's params by program: the subject's weights and a judge's.
    pub params: &'a dyn PipelineParams,
    /// The scoring stage the output stage is — how its committed score is read.
    pub kind: PalwScoringKindV1,
}

/// **What a job's run needs besides the job**: the item's prompt; its key (ExactMatch, once revealed);
/// its reference (a teacher-forced job's continuation, given as the stream's ids); the finalized
/// generations a pairwise or judge stage reads, by `(claim, stage)`; and the job scalars the
/// pipeline's scoring stage reads (delimiters, a logit scale, a judge's range, R's order and a margin).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwEvalInputsV1 {
    pub prompt: Vec<u32>,
    pub key: Vec<u32>,
    pub reference: Vec<u32>,
    pub finalized: BTreeMap<(u8, u8), Vec<u32>>,
    pub scalars: Vec<i64>,
}

/// **A score, read off the output stage by its kind** (A7's library: each commits an `i32` tensor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwEvalScoreValueV1 {
    /// `[1] ∈ {0, 1}`: the answer span equals the key.
    ExactMatch(bool),
    /// `[2]` = `(hi, lo)`: `Σ_r log p(ref_r)` in Q24 nats, `hi · 2^31 + lo`.
    RefLogLik(i64),
    /// `[1]`: the judge's scalar, clamped to the policy's range.
    Judge(i32),
    /// `[1] ∈ {−1, 0, 1}`: the candidate's outcome against the parent.
    Pairwise(i32),
}

/// **A run**: the execution (its claim roots — the step root, every stage's root, the generated ids —
/// and every leaf's values for the court) and the score.
pub struct PalwEvalRunV1 {
    pub execution: PalwGenExecutionV1,
    pub score: PalwEvalScoreValueV1,
}

/// The generation seed `R` is keyed by (provisional: the item seed's first 32 bytes).
pub fn palw_eval_gen_seed_v1(seed: &Hash64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&seed.as_byte_slice()[..32]);
    out
}

/// **The decode a `Generate` job runs under** (provisional): the canonical V4 decode with the stop
/// ids as one-id stop sequences (canonical: ascending, no duplicates, at most the format's four),
/// greedy, `max_new` as the limit. `None` for stop ids the V4 form cannot carry.
pub fn palw_eval_decode_v1(max_new: u32, stop_ids: &[u32]) -> Option<PalwGenDecodeV1> {
    let mut stops: Vec<Vec<u32>> = stop_ids.iter().map(|id| vec![*id]).collect();
    stops.sort();
    stops.dedup();
    let config = DecodeConfigV4 { stop_sequences: stops, ..DecodeConfigV4::NOOP };
    config.validate_canonical().ok()?;
    Some(PalwGenDecodeV1 { config, sampling: PalwDecodeSamplingV2::GREEDY, limit: max_new })
}

/// **A job's pipeline facts** — the prompt, the key, the finalized generations and the scalars; a
/// teacher-forced job's reference as the stream's given ids (its decode stage consumes the reference's
/// logits rows, and its scoring stage reads the reference as `Generated`).
pub fn palw_eval_pipeline_job_v1(job: &PalwEvalJobV1, inputs: &PalwEvalInputsV1) -> PipelineJob {
    let generated = match job.mode {
        PalwEvalModeV1::Generate { .. } => Vec::new(),
        PalwEvalModeV1::TeacherForced { .. } => inputs.reference.clone(),
    };
    PipelineJob {
        prompt: inputs.prompt.clone(),
        scalars: inputs.scalars.clone(),
        generated,
        key: inputs.key.clone(),
        finalized: inputs.finalized.clone(),
        ..PipelineJob::default()
    }
}

/// **The committed score of a run's output**, by the scoring stage's kind — refused when the output is
/// not that stage's shape or value range.
pub fn palw_eval_score_of_v1(kind: PalwScoringKindV1, output: &[i128]) -> Result<PalwEvalScoreValueV1, String> {
    let i32_at = |i: usize| -> Result<i32, String> {
        let v = *output.get(i).ok_or_else(|| format!("the {kind:?} output has no lane {i}"))?;
        i32::try_from(v).map_err(|_| format!("the {kind:?} output's lane {i} is not an i32: {v}"))
    };
    let expect = |n: usize| {
        if output.len() == n { Ok(()) } else { Err(format!("the {kind:?} output has {} lanes, not {n}", output.len())) }
    };
    match kind {
        PalwScoringKindV1::ExactMatch => {
            expect(1)?;
            match i32_at(0)? {
                0 => Ok(PalwEvalScoreValueV1::ExactMatch(false)),
                1 => Ok(PalwEvalScoreValueV1::ExactMatch(true)),
                v => Err(format!("an ExactMatch score is 0 or 1, not {v}")),
            }
        }
        PalwScoringKindV1::RefLogLik => {
            expect(2)?;
            Ok(PalwEvalScoreValueV1::RefLogLik(misaka_palw_tir::scoring::ref_loglik_join_v1(i32_at(0)?, i32_at(1)?)))
        }
        PalwScoringKindV1::Judge => {
            expect(1)?;
            Ok(PalwEvalScoreValueV1::Judge(i32_at(0)?))
        }
        PalwScoringKindV1::Pairwise => {
            expect(1)?;
            match i32_at(0)? {
                v @ -1..=1 => Ok(PalwEvalScoreValueV1::Pairwise(v)),
                v => Err(format!("a Pairwise score is −1, 0 or 1, not {v}")),
            }
        }
    }
}

/// **Run an evaluation job**: generating under the item's seed through the decode stage
/// (`palw_gen_execute_v1`), or teacher-forced over the given reference (`palw_gen_replay_committed_v1`:
/// the stream's ids given, nothing selected) — the claim's roots either way — and the score read off
/// the output stage.
pub fn palw_eval_execute_v1(
    pipeline: &PalwEvalPipelineV1<'_>,
    job: &PalwEvalJobV1,
    inputs: &PalwEvalInputsV1,
) -> Result<PalwEvalRunV1, String> {
    let pjob = palw_eval_pipeline_job_v1(job, inputs);
    let execution = match &job.mode {
        PalwEvalModeV1::Generate { seed, max_new, stop_ids } => {
            let decode = palw_eval_decode_v1(*max_new, stop_ids).ok_or("the job's stop ids have no canonical V4 form")?;
            palw_gen_execute_v1(
                pipeline.pipeline,
                pipeline.programs,
                pipeline.layouts,
                pipeline.params,
                &pjob,
                &decode,
                palw_eval_gen_seed_v1(seed),
            )
        }
        PalwEvalModeV1::TeacherForced { .. } => {
            if inputs.reference.is_empty() && pipeline.kind == PalwScoringKindV1::RefLogLik {
                return Err("a teacher-forced job needs its reference".into());
            }
            palw_gen_replay_committed_v1(pipeline.pipeline, pipeline.programs, pipeline.layouts, pipeline.params, &pjob, [0; 32])
        }
    }
    .map_err(|e| e.to_string())?;
    let score = palw_eval_score_of_v1(pipeline.kind, &execution.run.output.data)?;
    Ok(PalwEvalRunV1 { execution, score })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_improve_state_v1::PalwEvalSubjectV1;
    use kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1;
    use misaka_palw_tir::interp::{MapParams, ParamSource};
    use misaka_palw_tir::scoring::{exact_match_reference_v1, pairwise_reference_v1, ref_loglik_reference_v1};
    use misaka_palw_tir::tensor::Tensor;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
    }

    fn ids(v: &serde_json::Value) -> Vec<u32> {
        v.as_array().map(|a| a.iter().map(|t| t.as_u64().unwrap() as u32).collect()).unwrap_or_default()
    }

    struct Params(Vec<MapParams>);
    impl PipelineParams for Params {
        fn params(&self, program: u16) -> &dyn ParamSource {
            &self.0[program as usize]
        }
    }

    /// One of A7's evaluation pipelines (`consensus-vectors/tir-v2/pipelines`), with its job's inputs
    /// and its score.
    struct Vector {
        pipeline: TirPipelineV1,
        programs: Vec<TirProgramV2>,
        layouts: Vec<PalwTirLayoutV1>,
        params: Params,
        inputs: PalwEvalInputsV1,
        generated: Vec<u32>,
        score: Vec<i128>,
    }

    fn vector(name: &str) -> Vector {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v2/pipelines").join(name);
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the vector")).expect("json");
        let programs: Vec<TirProgramV2> = v["programs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| TirProgramV2::decode_canonical(&unhex(p["program_borsh_hex"].as_str().unwrap())).unwrap())
            .collect();
        let pipeline = TirPipelineV1::decode_canonical(&unhex(v["pipeline_borsh_hex"].as_str().unwrap()), &programs).unwrap();
        let params = Params(
            v["programs"]
                .as_array()
                .unwrap()
                .iter()
                .zip(&programs)
                .map(|(pj, prog)| {
                    let mut m = MapParams::default();
                    for e in pj["params"].as_array().unwrap() {
                        let j = e["param"].as_u64().unwrap() as u16;
                        let decl = &prog.params[j as usize];
                        let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
                        let layer = e["layer"].as_u64().map(|l| l as u16);
                        m.tensors.insert(
                            (j, layer),
                            Tensor::from_le_bytes(decl.dtype, &shape, &unhex(e["le_hex"].as_str().unwrap())).unwrap(),
                        );
                    }
                    m
                })
                .collect(),
        );
        let layouts = pipeline
            .stages
            .iter()
            .map(|st| {
                let p = &programs[st.program as usize];
                let commits = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
                PalwTirLayoutV1 {
                    version: PALW_TIR_LAYOUT_VERSION_V1,
                    max_context: st.max_trip,
                    checkpoint_interval: 1,
                    h_tile: 16,
                    commit_tiles: vec![4; commits],
                    state_tiles: vec![4; p.states.len()],
                }
            })
            .collect();
        let j = &v["job"];
        let inputs = PalwEvalInputsV1 {
            prompt: ids(&j["prompt"]),
            key: ids(&j["key"]),
            reference: Vec::new(),
            finalized: j["finalized"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|f| ((f["claim"].as_u64().unwrap() as u8, f["stage"].as_u64().unwrap() as u8), ids(&f["ids"])))
                        .collect()
                })
                .unwrap_or_default(),
            scalars: j["scalars"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().parse().unwrap()).collect(),
        };
        let score = v["output"]["data"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().parse().unwrap()).collect();
        Vector { pipeline, programs, layouts, params, inputs, generated: ids(&j["generated"]), score }
    }

    fn job(mode: PalwEvalModeV1) -> PalwEvalJobV1 {
        PalwEvalJobV1 {
            line_id: Hash64::from_u64_word(1),
            epoch: 3,
            item: 0,
            subject: PalwEvalSubjectV1::Parent,
            mode,
            pipeline_root: Hash64::from_u64_word(0xE1),
        }
    }

    fn pipeline<'a>(v: &'a Vector, kind: PalwScoringKindV1) -> PalwEvalPipelineV1<'a> {
        PalwEvalPipelineV1 { pipeline: &v.pipeline, programs: &v.programs, layouts: &v.layouts, params: &v.params, kind }
    }

    /// **A generated job, end to end on A7's exact-match pipeline** (a toy LM decoding, then
    /// ExactMatch): the executor's greedy decode under the job's budget selects the vector's ids, its
    /// score is the vector's and the reference arithmetic's, and a teacher-forced replay of those ids
    /// commits the same claim — the roots a seat and a court check.
    #[test]
    fn a_generated_job_runs_the_decode_stage_then_scores_it_and_commits_the_claim() {
        let v = vector("eval-exact-match.json");
        let generating = job(PalwEvalModeV1::Generate { seed: Hash64::from_bytes([0; 64]), max_new: 4, stop_ids: vec![] });
        let run = palw_eval_execute_v1(&pipeline(&v, PalwScoringKindV1::ExactMatch), &generating, &v.inputs).expect("the run");
        assert_eq!(run.execution.claim.generated, v.generated, "the decode stage selects the vector's ids");
        assert_eq!(run.execution.run.output.data, v.score, "the vector's score");
        let want = exact_match_reference_v1(&v.generated, &v.inputs.key, v.inputs.scalars[0], v.inputs.scalars[1]);
        assert_eq!(run.score, PalwEvalScoreValueV1::ExactMatch(want));
        let replayed = palw_eval_execute_v1(
            &pipeline(&v, PalwScoringKindV1::ExactMatch),
            &job(PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::from_u64_word(0) }),
            &PalwEvalInputsV1 { reference: v.generated.clone(), ..v.inputs.clone() },
        )
        .expect("the replay");
        assert_eq!(replayed.execution.claim, run.execution.claim, "the generating run and the replay commit one claim");
        // Another key scores the other way, and the claim's roots move with the score leaf.
        let mut other = v.inputs.clone();
        other.key = vec![other.key.first().map_or(1, |k| k + 1)];
        let run2 = palw_eval_execute_v1(&pipeline(&v, PalwScoringKindV1::ExactMatch), &generating, &other).expect("the run");
        let want2 = exact_match_reference_v1(&v.generated, &other.key, other.scalars[0], other.scalars[1]);
        assert_eq!(run2.score, PalwEvalScoreValueV1::ExactMatch(want2));
        if want2 != want {
            assert_ne!(run2.execution.claim.step_root, run.execution.claim.step_root);
        }
    }

    /// **The other three stages**: a teacher-forced RefLogLik job's score is the reference's
    /// log-likelihood under the decode stage's consumed rows (the reference arithmetic's, joined from
    /// `(hi, lo)`); a judge's and a pairwise judge's jobs read their finalized generations and score as
    /// the vectors do (the pairwise one as the reference rule says); a score of the wrong shape or
    /// range is refused by name.
    #[test]
    fn teacher_forced_judge_and_pairwise_jobs_score_as_the_vectors_do() {
        let v = vector("eval-ref-loglik.json");
        let forced = job(PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::from_u64_word(0) });
        let inputs = PalwEvalInputsV1 { reference: v.generated.clone(), ..v.inputs.clone() };
        let run = palw_eval_execute_v1(&pipeline(&v, PalwScoringKindV1::RefLogLik), &forced, &inputs).expect("the run");
        assert_eq!(run.execution.run.output.data, v.score);
        let from = v.inputs.prompt.len() - 1;
        let decode = &run.execution.run.stages[0];
        let rows: Vec<Vec<i32>> = decode.rows()[from..].iter().map(|t| t.data.iter().map(|x| *x as i32).collect()).collect();
        let PalwEvalScoreValueV1::RefLogLik(sum) = run.score else { panic!("a RefLogLik score") };
        assert_eq!(sum, ref_loglik_reference_v1(&rows, &v.generated, v.inputs.scalars[0]), "the reference arithmetic");
        assert!(
            palw_eval_execute_v1(&pipeline(&v, PalwScoringKindV1::RefLogLik), &forced, &v.inputs).is_err(),
            "teacher-forced without the reference"
        );
        let j = vector("eval-judge.json");
        let run = palw_eval_execute_v1(&pipeline(&j, PalwScoringKindV1::Judge), &forced, &j.inputs).expect("the judge");
        assert_eq!(run.execution.run.output.data, j.score);
        assert!(matches!(run.score, PalwEvalScoreValueV1::Judge(_)));
        let p = vector("eval-pairwise.json");
        let run = palw_eval_execute_v1(&pipeline(&p, PalwScoringKindV1::Pairwise), &forced, &p.inputs).expect("the pairwise judge");
        assert_eq!(run.execution.run.output.data, p.score);
        let PalwEvalScoreValueV1::Pairwise(outcome) = run.score else { panic!("a Pairwise score") };
        assert!((-1..=1).contains(&outcome));
        let judge_stage = &run.execution.run.stages[0];
        let pref = judge_stage.last().expect("the judge's final").data[0] as i32;
        assert_eq!(outcome, pairwise_reference_v1(pref, p.inputs.scalars[0] as i32, p.inputs.scalars[1] as i32));
        // Shapes and ranges.
        assert!(palw_eval_score_of_v1(PalwScoringKindV1::ExactMatch, &[2]).is_err());
        assert!(palw_eval_score_of_v1(PalwScoringKindV1::ExactMatch, &[1, 0]).is_err());
        assert!(palw_eval_score_of_v1(PalwScoringKindV1::Pairwise, &[3]).is_err());
        assert!(palw_eval_score_of_v1(PalwScoringKindV1::RefLogLik, &[1]).is_err());
        assert_eq!(
            palw_eval_score_of_v1(PalwScoringKindV1::RefLogLik, &[1, 5]).unwrap(),
            PalwEvalScoreValueV1::RefLogLik((1 << 31) + 5)
        );
        // The provisional decode: stop ids as canonical one-id sequences, refused past the format.
        let d = palw_eval_decode_v1(8, &[9, 2, 9]).expect("canonical");
        assert_eq!(d.config.stop_sequences, vec![vec![2], vec![9]]);
        assert_eq!(d.limit, 8);
        assert!(palw_eval_decode_v1(8, &[1, 2, 3, 4, 5]).is_none(), "at most four stop sequences");
    }
}
