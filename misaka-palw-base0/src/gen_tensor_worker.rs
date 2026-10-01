//! **The tensor worker, seat and court halves of a node** (RFC-0003 §I.3, §II.2.1) — a job of an
//! image or an embedding class (`PalwGenJobV1`), dormant until `palw_gen_v1` arms the profiles. The
//! text halves are [`crate::gen_worker`]'s; this is the same shape for a claim whose answer is a
//! canonical output, not a list of ids.
//!
//! * **The worker** ([`GenHeldClassV1::run_tensor`]): the job held to the class (its version, profile,
//!   modes and every field against the class's offers: `palw_gen_job_resolve_class_v1`), the ids to the
//!   job's commitments in the network's form and to the bound the class's stages read them under, every
//!   image to its reference; then the pipeline with `R` keyed by the job's seed at its item index, the
//!   step tree, and the canonical output cut at the output node's step tiles. Its answer is the claim's
//!   binding (`PalwGenTensorBindingV1`): the stage roots, the leaf count and the `output_root` the
//!   execution root commits.
//! * **The seat** replays the job from the material the panel received and holds the replay's execution
//!   root to the claim's.
//! * **The court**: a run's capture ([`GenTensorCaptureV1`]: the inputs, every committed leaf and the
//!   claim's canonical output, lies included) rebuilds the accused's execution; the moves a party files
//!   at the leaf a ladder narrowed to are the cone close and, at an output node's step tile, the output
//!   close ([`gen_tensor_court_candidates_v1`]); a challenger who finds the claimed output is not its
//!   own step tree's names the tile ([`gen_tensor_output_audit_v1`]).

use crate::gen_worker::{GenCourtMoveV1, GenHeldClassV1, GenSeatJudgmentV1, GenWireImageV1};
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_gen_close_v1::{
    PalwGenEvidenceV1, PalwGenTensorBindingV1, check_gen_output_close_v1, palw_gen_image_input_ref_v1, palw_gen_output_tile_at_v1,
};
use kaspa_consensus_core::palw_gen_job_v1::{
    PalwGenAcceptedJobV1, PalwGenIdsV1, PalwGenJobV1, palw_gen_job_ids_admitted_v1, palw_gen_job_resolve_class_v1,
    palw_gen_pipeline_job_v1,
};
use kaspa_consensus_core::palw_gen_step_v1::{
    PalwGenLeafKindV1, PalwGenStepSpaceV1, palw_gen_stage_root_v1, palw_gen_step_leaf_hash_v1, palw_gen_step_root_v1,
};
use kaspa_consensus_core::palw_gen_worker_v1::{PalwGenClaimRootsV1, PalwGenExecutionV1, PalwGenOutputV1, palw_gen_execute_tensor_v1};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::palw_tir_step_v1::{palw_tir_lane_values_v1, palw_tir_lanes_le_v1};
use kaspa_hashes::Hash64;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::pipeline::{JobImageV1, PipelineParams, stage_job_facts};

/// **One tensor run**: the job, its text inputs and images (the executor's inputs), the execution and
/// the binding a commitment carries.
#[derive(Clone, Debug)]
pub struct GenTensorWorkV1 {
    pub job: PalwGenJobV1,
    pub prompt: Vec<u32>,
    pub negative: Vec<u32>,
    pub images: Vec<JobImageV1>,
    pub execution: PalwGenExecutionV1,
    pub binding: PalwGenTensorBindingV1,
}

impl GenTensorWorkV1 {
    /// The evidence every court move of this run is built from.
    pub fn evidence<'a, P: PipelineParams>(&'a self, held: &'a GenHeldClassV1<P>) -> PalwGenEvidenceV1<'a> {
        PalwGenEvidenceV1 {
            row: &held.row,
            params: &held.params,
            execution: &self.execution,
            binding: self.binding.clone().into(),
            prompt: &self.prompt,
            negative: &self.negative,
            images: &self.images,
            source: &[],
        }
    }

    /// The execution root a commitment carries.
    pub fn execution_root(&self) -> Hash64 {
        self.binding.committed_execution_root
    }

    /// The claim's canonical output (a tensor run always has one).
    pub fn output(&self) -> &PalwGenOutputV1 {
        self.execution.output.as_ref().expect("a tensor run commits a canonical output")
    }
}

impl<P: PipelineParams> GenHeldClassV1<P> {
    /// **Hold the inputs a tensor job names**: the job the class's, each id list the job's commitment
    /// in the network's form and within the bound the class reads it under, and every image the
    /// reference the job carries.
    fn check_tensor_inputs(
        &self,
        job: &PalwGenJobV1,
        prompt: &[u32],
        negative: &[u32],
        images: &[JobImageV1],
        form: PalwPromptIdsFormV1,
    ) -> Result<PalwGenAcceptedJobV1, String> {
        let accepted = palw_gen_job_resolve_class_v1(job, &self.row).map_err(|e| e.to_string())?;
        palw_gen_job_ids_admitted_v1(&self.row, &accepted, PalwGenIdsV1 { prompt, negative }, form).map_err(|e| e.to_string())?;
        if images.len() != accepted.images.len() {
            return Err(format!("{} images for a job of {}", images.len(), accepted.images.len()));
        }
        for (k, ((image, reference), slot)) in images.iter().zip(&accepted.images).zip(&self.row.class.offers.images).enumerate() {
            if palw_gen_image_input_ref_v1(image, slot.tile_len)? != *reference {
                return Err(format!("image {k} is not the one the job names"));
            }
        }
        Ok(accepted)
    }

    /// **Run a tensor job** (see the module doc). The caller has resolved the fence and the job's
    /// network; the job's seed keys `R`, at the job's item index.
    pub fn run_tensor(
        &self,
        job: &PalwGenJobV1,
        prompt: &[u32],
        negative: &[u32],
        images: &[JobImageV1],
        form: PalwPromptIdsFormV1,
    ) -> Result<GenTensorWorkV1, String> {
        let accepted = self.check_tensor_inputs(job, prompt, negative, images, form)?;
        let run_job = palw_gen_pipeline_job_v1(&accepted, PalwGenIdsV1 { prompt, negative }, images.to_vec());
        let execution = palw_gen_execute_tensor_v1(
            &self.pipeline,
            &self.programs,
            &self.row.class.layouts,
            &self.params,
            &run_job,
            job.seed,
            accepted.item_index,
            &self.row.class.output,
        )
        .map_err(|e| e.to_string())?;
        let output_root = execution.claim.output_root.ok_or("a tensor run commits an output root")?;
        let binding = PalwGenTensorBindingV1::of(job, &execution.claim, execution.space.leaf_count(), output_root);
        Ok(GenTensorWorkV1 {
            job: job.clone(),
            prompt: prompt.to_vec(),
            negative: negative.to_vec(),
            images: images.to_vec(),
            execution,
            binding,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// The worker's frame: one request in, one answer out
// ---------------------------------------------------------------------------------------------

/// **A tensor request**: the job, its text ids and the images' bytes (which never ride a
/// transaction — they travel to the worker, and with the capture to the panel).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenTensorRequestV1 {
    pub job: PalwGenJobV1,
    pub prompt_ids: Vec<u32>,
    pub negative_ids: Vec<u32>,
    pub images: Vec<GenWireImageV1>,
}

/// **The worker's answer**: the binding (the commitment's execution root, the leaf count, the output
/// digest) — or a refusal naming the rule, never the prompt.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwGenTensorAnswerV1 {
    Result { binding: PalwGenTensorBindingV1 },
    Refused { why: String },
}

/// **The serving loop's one step**: a request frame's bytes in, an answer out. A request the worker
/// will not run is refused and the held class stays held.
pub fn gen_tensor_answer_v1<P: PipelineParams>(
    held: &GenHeldClassV1<P>,
    request: &[u8],
    form: PalwPromptIdsFormV1,
) -> (PalwGenTensorAnswerV1, Option<GenTensorWorkV1>) {
    let request: PalwGenTensorRequestV1 = match borsh::from_slice(request) {
        Ok(r) => r,
        Err(_) => return (PalwGenTensorAnswerV1::Refused { why: "the request does not decode".into() }, None),
    };
    let images: Vec<JobImageV1> = request.images.into_iter().map(|i| JobImageV1 { h: i.h, w: i.w, rgb: i.rgb }).collect();
    match held.run_tensor(&request.job, &request.prompt_ids, &request.negative_ids, &images, form) {
        Ok(work) => (PalwGenTensorAnswerV1::Result { binding: work.binding.clone() }, Some(work)),
        Err(why) => (PalwGenTensorAnswerV1::Refused { why }, None),
    }
}

// ---------------------------------------------------------------------------------------------
// The seat
// ---------------------------------------------------------------------------------------------

/// **Judge a tensor claim from the material the panel received**: the inputs held to the job, the job
/// replayed, and the replay's execution root held to the claim's.
pub fn gen_tensor_seat_judge_v1<P: PipelineParams>(
    held: &GenHeldClassV1<P>,
    claim_execution_root: &Hash64,
    job: &PalwGenJobV1,
    prompt: &[u32],
    negative: &[u32],
    images: &[JobImageV1],
    form: PalwPromptIdsFormV1,
) -> GenSeatJudgmentV1 {
    match held.run_tensor(job, prompt, negative, images, form) {
        Err(why) => GenSeatJudgmentV1::Unjudgeable(why),
        Ok(replay) if replay.execution_root() == *claim_execution_root => GenSeatJudgmentV1::Valid,
        Ok(replay) => {
            GenSeatJudgmentV1::Differs(format!("the replay's execution root {} is not the claim's", replay.execution_root()))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The capture and the court
// ---------------------------------------------------------------------------------------------

/// **A tensor run's capture**: its inputs, every leaf it committed (as 4-byte lanes), in the claim's
/// one order, and the claim's canonical output's values — what data availability serves and what
/// rebuilds the accused's execution, lies included.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct GenTensorCaptureV1 {
    pub job: PalwGenJobV1,
    pub prompt: Vec<u32>,
    pub negative: Vec<u32>,
    pub images: Vec<GenWireImageV1>,
    /// Per stage, every leaf's lanes, in leaf order.
    pub leaves: Vec<Vec<Vec<u8>>>,
    /// The claim's canonical output's elements, row-major.
    pub output: Vec<i64>,
}

impl GenTensorCaptureV1 {
    /// A run's capture.
    pub fn of(work: &GenTensorWorkV1) -> Result<Self, String> {
        let mut leaves = Vec::with_capacity(work.execution.space.stages.len());
        for (stage, values) in work.execution.space.stages.iter().zip(&work.execution.leaf_values) {
            let mut lanes = Vec::with_capacity(values.len());
            for (leaf, v) in stage.leaves().iter().zip(values) {
                lanes.push(palw_tir_lanes_le_v1(leaf.dtype, v).map_err(|e| e.to_string())?);
            }
            leaves.push(lanes);
        }
        Ok(Self {
            job: work.job.clone(),
            prompt: work.prompt.clone(),
            negative: work.negative.clone(),
            images: work.images.iter().map(|i| GenWireImageV1 { h: i.h, w: i.w, rgb: i.rgb.clone() }).collect(),
            leaves,
            output: work.output().values.clone(),
        })
    }

    /// **The accused's execution, rebuilt from its capture**: the space from the job's trips, every
    /// leaf's hash and every stage's root from the captured lanes, the output's root from the captured
    /// output, and the binding over them — the execution the captured commitments describe, whatever
    /// computed them.
    pub fn rebuild<P: PipelineParams>(&self, held: &GenHeldClassV1<P>) -> Result<GenTensorWorkV1, String> {
        let accepted = palw_gen_job_resolve_class_v1(&self.job, &held.row).map_err(|e| e.to_string())?;
        let images: Vec<JobImageV1> = self.images.iter().map(|i| JobImageV1 { h: i.h, w: i.w, rgb: i.rgb.clone() }).collect();
        let facts_job =
            palw_gen_pipeline_job_v1(&accepted, PalwGenIdsV1 { prompt: &self.prompt, negative: &self.negative }, images.clone());
        let facts = stage_job_facts(&held.pipeline, &held.programs, &facts_job).map_err(|e| e.to_string())?;
        let trips: Vec<u32> = facts.iter().map(|f| f.trip).collect();
        let space =
            PalwGenStepSpaceV1::new(&held.pipeline, &held.programs, &held.row.class.layouts, &trips, 0).map_err(|e| e.to_string())?;
        if self.leaves.len() != space.stages.len() {
            return Err("one leaf list per stage".into());
        }
        let (mut leaf_values, mut leaf_hashes) = (Vec::new(), Vec::new());
        for (stage, lanes) in space.stages.iter().zip(&self.leaves) {
            if lanes.len() != stage.leaves().len() {
                return Err(format!("stage {}: {} leaves captured for {}", stage.stage, lanes.len(), stage.leaves().len()));
            }
            let (mut values, mut hashes) = (Vec::new(), Vec::new());
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
        let tile_len = kaspa_consensus_core::palw_gen_class_v1::palw_gen_output_tile_len_v1(
            &held.pipeline,
            &held.programs,
            &held.row.class.layouts,
        )
        .ok_or("the class's layout has no tile for its output node")?;
        let root = Hash64::from_bytes(
            misaka_palw_gen::output_root_v1(&held.row.class.output, &self.output, tile_len)
                .map_err(|e| format!("the output: {e:?}"))?,
        );
        let claim = PalwGenClaimRootsV1 {
            step_root: palw_gen_step_root_v1(&stage_roots),
            stage_roots,
            generated: Vec::new(),
            output_root: Some(root),
        };
        let binding = PalwGenTensorBindingV1::of(&self.job, &claim, space.leaf_count(), root);
        // The capture holds commitments, not a run: the rebuilt execution's run is empty.
        let run = misaka_palw_tir::pipeline::PipelineRun {
            stages: Vec::new(),
            output: misaka_palw_tir::tensor::Tensor::zeros(misaka_palw_tir::types::DType::I32, &[0]),
        };
        let output = Some(PalwGenOutputV1 { spec: held.row.class.output.clone(), tile_len, values: self.output.clone(), root });
        let execution = PalwGenExecutionV1 { run, stop: None, space, leaf_values, leaf_hashes, claim, output };
        Ok(GenTensorWorkV1 {
            job: self.job.clone(),
            prompt: self.prompt.clone(),
            negative: self.negative.clone(),
            images,
            execution,
            binding,
        })
    }
}

/// **The moves a party may file at leaf `index` of the accused's execution**, in the order it tries
/// them, each built from the accused's evidence or refused: at a dissected leaf the responder's root
/// claim (the challenger waits for it); elsewhere a cone close, and for a challenger at the output
/// node's step tile of an output tile the output close of that tile too.
pub fn gen_tensor_court_candidates_v1<P: PipelineParams>(
    held: &GenHeldClassV1<P>,
    accused: &GenTensorWorkV1,
    index: u64,
    challenger: bool,
    limits: &DemandLimits,
) -> Vec<(&'static str, Result<GenCourtMoveV1, String>)> {
    let evidence = accused.evidence(held);
    let Some((stage, local)) = accused.execution.space.locate(index) else {
        return vec![("locate", Err(format!("{index} is no leaf of the accused's execution")))];
    };
    let sp = &accused.execution.space.stages[stage as usize];
    let leaf = sp.leaves()[local as usize];
    let dissected = match leaf.coord.kind {
        PalwGenLeafKindV1::Commit { occurrence, node } => {
            sp.occurrence_block(occurrence).is_some_and(|b| held.row.dissected.contains(&(stage, b, node)))
        }
        PalwGenLeafKindV1::State { .. } => false,
    };
    if dissected {
        if challenger {
            return Vec::new();
        }
        return vec![("root claim", evidence.root_claim(index, limits).map(GenCourtMoveV1::RootClaim))];
    }
    let mut out = vec![(
        "cone",
        evidence
            .cone_close(index, limits)
            .map(|close| GenCourtMoveV1::Close(PalwCourtVerdictProofV2::GenCone { close: Box::new(close) })),
    )];
    if challenger && stage == held.pipeline.output_stage {
        let tile_len = accused.output().tile_len;
        if let Some(tile) = palw_gen_output_tile_at_v1(sp, &leaf.coord, tile_len) {
            out.push((
                "output",
                evidence
                    .output_close(tile)
                    .map(|close| GenCourtMoveV1::Close(PalwCourtVerdictProofV2::GenOutputTile { close: Box::new(close) })),
            ));
        }
    }
    out
}

/// **The first output tile whose canonical bytes are not the accused's own committed step tile's** —
/// a challenger's audit of a claim whose step tree it has found honest: the tile (and the global index
/// of the step tile it shares its lanes with) it narrows a session to, and the fault the court finds
/// there. `None` when every output tile is the node's.
pub fn gen_tensor_output_audit_v1<P: PipelineParams>(
    held: &GenHeldClassV1<P>,
    accused: &GenTensorWorkV1,
) -> Option<(u64, u64, PalwStepFaultV1)> {
    let evidence = accused.evidence(held);
    let output = accused.output();
    let tiles = misaka_palw_gen::output::output_tile_count_v1(&output.spec, output.tile_len).ok()?;
    let stage = &accused.execution.space.stages[held.pipeline.output_stage as usize];
    for tile in 0..tiles {
        let close = evidence.output_close(tile).ok()?;
        let verdict = check_gen_output_close_v1(&close, &held.row, &held.row.class_id, &accused.execution_root(), None);
        if let Ok(Some(fault)) = verdict {
            let global = accused.execution.space.global_index(&close.step_tile.coord)?;
            let _ = stage;
            return Some((tile, global, fault));
        }
    }
    None
}
