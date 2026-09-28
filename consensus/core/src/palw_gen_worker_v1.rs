//! **The worker and the panel of a pipeline class** (RFC-0003 §II.2.1): an FP Job V5 run on a
//! vision-language class — every stage before the text stage, then the text stage generating
//! through FP Job V4's decoder — into the claim's one step tree ([`crate::palw_gen_step_v1`]) and
//! its roots; and a seat's replay of a claim against them.
//!
//! The reference interpreter (`misaka-palw-tir`) is the meaning here: a node's typed executor
//! (`misaka-palw-tir-exec`) must produce byte-identical leaves, as it does for a Phase F class. The
//! decode is RFC-0001 §A.3's, driven exactly as every FP engine drives it
//! ([`PalwFpDecoderV1`]): one selecting row per consumed position, the committed lane fed back, the
//! first stop ending the answer. `R` (a stage's random inputs, if any) is keyed on the job's
//! `sampling_seed`, item 0 — the V5 job's one seed.
//!
//! Dormant: nothing on chain can name a generative class yet (`ClassRegisteredGenV1` is refused at
//! every height), so no node runs these for a claim until the pipeline admission lands.

use crate::Hash64;
use crate::palw_decode_pipeline_v4::{DecodeConfigV4, PalwFpDecodeStopV1, PalwFpDecoderV1};
use crate::palw_decode_select_v2::PalwDecodeSamplingV2;
use crate::palw_gen_step_v1::{
    PalwGenLeafCoordV1, PalwGenOpenedLeafV1, PalwGenStepErrorV1, PalwGenStepSpaceV1, palw_gen_leaf_path_v1, palw_gen_stage_root_v1,
    palw_gen_step_leaf_hash_v1, palw_gen_step_root_v1,
};
use crate::palw_tir_class_v1::PalwTirLayoutV1;
use misaka_palw_tir::pipeline::{
    PipelineJob, PipelineParams, PipelineRun, RandomSource, TextSelectV1, TirPipelineV1, run_pipeline, run_text_pipeline,
};
use misaka_palw_tir::program_v2::{RandomDist, TirProgramV2};
use misaka_palw_tir::tensor::Tensor;
use misaka_palw_tir::types::DType;

/// `R` for a pipeline job: `dist(R(seed, domain, step, 0, lane))`.
pub struct PalwGenRandomV1 {
    pub seed: [u8; 32],
}

impl RandomSource for PalwGenRandomV1 {
    fn random(&self, domain: u16, dist: RandomDist, step: u32, shape: &[u32]) -> Option<Tensor> {
        let n: u64 = shape.iter().map(|d| *d as u64).product();
        let (d, dtype) = match dist {
            RandomDist::Uniform { .. } => (misaka_palw_gen::RandDistV1::Uniform, DType::Idx),
            RandomDist::Normal => (misaka_palw_gen::RandDistV1::Normal, DType::I32),
        };
        let v = misaka_palw_gen::rand_values_v1(domain, d, &self.seed, step, 0, n).ok()?;
        Tensor::new(dtype, shape.iter().map(|x| *x as usize).collect(), v.into_iter().map(|x| x as i128).collect()).ok()
    }
}

/// The decode a V5 job asks: its embedded V4 job's rules, seed and budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwGenDecodeV1 {
    pub config: DecodeConfigV4,
    pub sampling: PalwDecodeSamplingV2,
    pub limit: u32,
}

impl PalwGenDecodeV1 {
    /// The decode of a V5 job.
    pub fn of(job: &crate::palw_fp_job_v5::PalwFreePromptJobV5) -> Option<Self> {
        Some(Self { config: job.v4.decode.clone()?, sampling: job.v4.sampling_v2(), limit: job.v4.decode_token_limit })
    }
}

/// **What a claim commits of an execution**: the step root, every stage's root (carried, and bound
/// by the step root), and the generated ids.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenClaimRootsV1 {
    pub step_root: Hash64,
    pub stage_roots: Vec<Hash64>,
    pub generated: Vec<u32>,
}

/// **An execution**: the run, the answer, the step tree with every leaf's values, and the roots.
#[derive(Clone, Debug)]
pub struct PalwGenExecutionV1 {
    pub run: PipelineRun,
    pub stop: Option<PalwFpDecodeStopV1>,
    pub space: PalwGenStepSpaceV1,
    /// Per stage, every leaf's values, in leaf order.
    pub leaf_values: Vec<Vec<Vec<i128>>>,
    pub leaf_hashes: Vec<Vec<Hash64>>,
    pub claim: PalwGenClaimRootsV1,
}

impl PalwGenExecutionV1 {
    /// Leaf `index` of stage `stage`, opened with its path.
    pub fn open(&self, stage: u8, index: u64) -> Option<PalwGenOpenedLeafV1> {
        let s = stage as usize;
        let leaf = self.space.stages.get(s)?.leaves().get(index as usize)?;
        Some(PalwGenOpenedLeafV1 {
            coord: leaf.coord,
            values: self.leaf_values[s][index as usize].clone(),
            path: palw_gen_leaf_path_v1(&self.leaf_hashes[s], index as usize)?,
        })
    }

    /// The leaf at `coord`, opened.
    pub fn open_at(&self, coord: &PalwGenLeafCoordV1) -> Option<PalwGenOpenedLeafV1> {
        let index = self.space.stages.get(coord.stage as usize)?.leaf_index(coord)?;
        self.open(coord.stage, index)
    }
}

/// Why a run is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwGenRunErrorV1 {
    #[error("the pipeline refuses the job: {0}")]
    Run(String),
    #[error(transparent)]
    Step(PalwGenStepErrorV1),
}

/// The tree, the leaves and the roots of a run whose text stage's prompt is `prompt_len` ids.
fn commit_run(
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
    layouts: &[PalwTirLayoutV1],
    run: PipelineRun,
    generated: Vec<u32>,
    stop: Option<PalwFpDecodeStopV1>,
    prompt_len: u32,
) -> Result<PalwGenExecutionV1, PalwGenRunErrorV1> {
    let trips: Vec<u32> = run.stages.iter().map(|s| s.trip).collect();
    let space = PalwGenStepSpaceV1::new(pipeline, programs, layouts, &trips, prompt_len).map_err(PalwGenRunErrorV1::Step)?;
    let mut leaf_values = Vec::with_capacity(space.stages.len());
    let mut leaf_hashes = Vec::with_capacity(space.stages.len());
    for (stage, stage_run) in space.stages.iter().zip(&run.stages) {
        let values = stage.leaf_values(stage_run).map_err(PalwGenRunErrorV1::Step)?;
        let hashes = stage
            .leaves()
            .iter()
            .zip(&values)
            .map(|(leaf, v)| palw_gen_step_leaf_hash_v1(leaf, v))
            .collect::<Result<Vec<_>, _>>()
            .map_err(PalwGenRunErrorV1::Step)?;
        leaf_values.push(values);
        leaf_hashes.push(hashes);
    }
    let stage_roots: Vec<Hash64> = leaf_hashes.iter().enumerate().map(|(s, h)| palw_gen_stage_root_v1(s as u8, h)).collect();
    let claim = PalwGenClaimRootsV1 { step_root: palw_gen_step_root_v1(&stage_roots), stage_roots, generated };
    Ok(PalwGenExecutionV1 { run, stop, space, leaf_values, leaf_hashes, claim })
}

/// **The worker**: run a job on a text class with image stages, generating through the V4 decoder.
/// `job` carries the prompt ids and the image bytes (the executor holds them); `seed` keys `R`.
pub fn palw_gen_execute_v1(
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
    layouts: &[PalwTirLayoutV1],
    params: &dyn PipelineParams,
    job: &PipelineJob,
    decode: &PalwGenDecodeV1,
    seed: [u8; 32],
) -> Result<PalwGenExecutionV1, PalwGenRunErrorV1> {
    let mut decoder = PalwFpDecoderV1::v4(decode.config.clone(), decode.sampling, decode.limit);
    let mut select = |_: u32, logits: &Tensor| -> TextSelectV1 {
        let row: Vec<i32> = logits.data.iter().map(|v| *v as i32).collect();
        let before = decoder.generated().len();
        let id = decoder.select(&row);
        if decoder.generated().len() == before {
            // An empty admitted set: generation ends with no id (§A.3 step 5).
            TextSelectV1::End
        } else if decoder.stop().is_some() {
            TextSelectV1::Last(id)
        } else {
            TextSelectV1::Next(id)
        }
    };
    let (run, generated) = run_text_pipeline(pipeline, programs, params, &PalwGenRandomV1 { seed }, job, &mut select)
        .map_err(|e| PalwGenRunErrorV1::Run(e.to_string()))?;
    let stop = decoder.stop();
    commit_run(pipeline, programs, layouts, run, generated, stop, job.prompt.len() as u32)
}

/// **A seat's verdict on a claim.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwGenSeatVerdictV1 {
    /// The replay commits exactly the claim's roots and answer.
    Valid,
    /// The answer is not what the decode selects from the replayed logits.
    AnswerDiffers { first: usize },
    /// A stage's root differs from the replay's (a seat holds roots, not the claimant's leaves: the
    /// first divergent leaf is the bisection's to find).
    StageDiffers { stage: u8 },
    /// The claim's roots do not bind its stage roots.
    RootsNotBound,
}

/// **The panel**: a seat re-executes the job — the same run, the same decode — and holds the claim
/// to it: the answer, then every stage's root, naming the first difference.
pub fn palw_gen_replay_v1(
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
    layouts: &[PalwTirLayoutV1],
    params: &dyn PipelineParams,
    job: &PipelineJob,
    decode: &PalwGenDecodeV1,
    seed: [u8; 32],
    claim: &PalwGenClaimRootsV1,
) -> Result<PalwGenSeatVerdictV1, PalwGenRunErrorV1> {
    if palw_gen_step_root_v1(&claim.stage_roots) != claim.step_root {
        return Ok(PalwGenSeatVerdictV1::RootsNotBound);
    }
    let replay = palw_gen_execute_v1(pipeline, programs, layouts, params, job, decode, seed)?;
    if replay.claim.generated != claim.generated {
        let first = replay.claim.generated.iter().zip(&claim.generated).position(|(a, b)| a != b);
        return Ok(PalwGenSeatVerdictV1::AnswerDiffers {
            first: first.unwrap_or(replay.claim.generated.len().min(claim.generated.len())),
        });
    }
    for (s, (mine, theirs)) in replay.claim.stage_roots.iter().zip(&claim.stage_roots).enumerate() {
        if mine != theirs {
            return Ok(PalwGenSeatVerdictV1::StageDiffers { stage: s as u8 });
        }
    }
    Ok(PalwGenSeatVerdictV1::Valid)
}

/// **A replay from the committed answer** (a court's and a seat's second path): the run with the
/// claim's generated ids fed back, without selecting — its roots are the claim's when the leaves
/// are honest, whatever the decode would have selected (the decode is the door's question).
pub fn palw_gen_replay_committed_v1(
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
    layouts: &[PalwTirLayoutV1],
    params: &dyn PipelineParams,
    job: &PipelineJob,
    seed: [u8; 32],
) -> Result<PalwGenExecutionV1, PalwGenRunErrorV1> {
    let run =
        run_pipeline(pipeline, programs, params, &PalwGenRandomV1 { seed }, job).map_err(|e| PalwGenRunErrorV1::Run(e.to_string()))?;
    commit_run(pipeline, programs, layouts, run, job.generated.clone(), None, job.prompt.len() as u32)
}
