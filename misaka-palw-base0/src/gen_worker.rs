//! **The generative worker, seat and court halves of a node** (RFC-0003 §II.2.1) — FP Job V5 on a
//! registered pipeline class, dormant until the free-prompt lane opens for pipeline classes at
//! `palw_fp_job_v5` (the walk skips version 8 before it).
//!
//! * **The worker** holds a class — its `gen_classes` row and this node's weights, refused unless
//!   they hash to the class's `artifact_root` ([`GenHeldClassV1::hold`]; from a `PALWTIR2` file,
//!   [`GenHeldClassV1::hold_container`], whose pipeline, programs, class and tokenizer must be the
//!   row's) — and runs a V5 job
//!   ([`GenHeldClassV1::run_v5`]): the job held to the class, the prompt to the job's hash and to the
//!   class's forced prefix, the source to the job's source reference (RFC-0003 §II.2.2), every image
//!   to its `input_root`, then the pipeline through FP Job V4's decoder. Its answer is the
//!   claim's binding (`PalwGenStepBindingV1`): the execution root a V5 commitment carries, the leaf
//!   count and the generated ids. [`gen_worker_answer_v1`] is the serving loop's one step — one
//!   request frame in, one answer out, a refusal never dropping the held class.
//! * **The seat** replays the job from the material the panel received and holds the replay's
//!   execution root to the claim's ([`gen_seat_judge_v1`]); a difference is the court's question,
//!   never a sampled conviction.
//! * **The court**: a run's capture (its inputs and every committed leaf, [`GenCaptureV1`]) rebuilds
//!   the accused's execution; [`gen_first_divergence_v1`] names the leaf an honest challenger
//!   disputes; [`gen_court_candidates_v1`] builds the moves a party files there — a cone close, a
//!   decode close at a logits row, or, at a dissected leaf, the responder's root claim — each
//!   carrying exactly the units the court reads (`PalwGenEvidenceV1`).

use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_fp_job_v5::PalwFreePromptJobV5;
use kaspa_consensus_core::palw_gen_artifact_v1::palw_gen_artifact_matches_v1;
use kaspa_consensus_core::palw_gen_class_v1::PalwGenClassRecordV1;
use kaspa_consensus_core::palw_gen_close_v1::{
    PalwGenEvidenceV1, PalwGenRootClaimV1, PalwGenStepBindingV1, check_gen_cone_close_v1, check_gen_decode_close_v1,
    check_gen_output_close_v1, palw_gen_image_input_ref_v1, palw_gen_root_claim_message_v1,
};
use kaspa_consensus_core::palw_gen_step_v1::{
    PalwGenLeafKindV1, PalwGenStepSpaceV1, palw_gen_stage_root_v1, palw_gen_step_leaf_hash_v1, palw_gen_step_root_v1,
};
use kaspa_consensus_core::palw_gen_worker_v1::{PalwGenClaimRootsV1, PalwGenDecodeV1, PalwGenExecutionV1, palw_gen_execute_v1};
use kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1};
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwCourtVerdictV2};
use kaspa_consensus_core::palw_tir_step_v1::{palw_tir_lane_values_v1, palw_tir_lanes_wire_v1};
use kaspa_hashes::Hash64;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::pipeline::{JobImageV1, PipelineJob, PipelineParams, TirPipelineV1, stage_job_facts};
use misaka_palw_tir::program_v2::TirProgramV2;

/// **A pipeline class this node holds**: its chain row, the class decoded, and weights that hash to
/// the row's `artifact_root`.
pub struct GenHeldClassV1<P: PipelineParams> {
    pub row: PalwGenClassRecordV1,
    pub pipeline: TirPipelineV1,
    pub programs: Vec<TirProgramV2>,
    pub params: P,
}

/// **One V5 run**: the job, its prompt, images and source (the executor's inputs), the execution and
/// the binding a commitment carries.
#[derive(Clone, Debug)]
pub struct GenWorkV1 {
    pub job: PalwFreePromptJobV5,
    pub prompt: Vec<u32>,
    pub images: Vec<JobImageV1>,
    /// The source ids (RFC-0003 §II.2.2); empty for a job without a source.
    pub source: Vec<u32>,
    pub execution: PalwGenExecutionV1,
    pub binding: PalwGenStepBindingV1,
}

impl<P: PipelineParams> GenHeldClassV1<P> {
    /// **Hold a registered class with this node's weights** — refused unless they hash to the row's
    /// `artifact_root` (a copy that does not is not the registered class).
    pub fn hold(row: PalwGenClassRecordV1, params: P) -> Result<Self, String> {
        let (programs, pipeline) = row.class.decode().map_err(|e| e.to_string())?;
        palw_gen_artifact_matches_v1(&programs, &params, &row.artifact_root).map_err(|e| e.to_string())?;
        Ok(Self { row, pipeline, programs, params })
    }

    /// **Hold the inputs a V5 job names**: the job the class's (`palw_fp_v5_resolve_class_v1`), the
    /// prompt the job's hash in the network's form and starting with the class's forced prefix, the
    /// source the job's source reference (none when it carries none), and every image the reference
    /// the job carries.
    fn check_inputs(
        &self,
        job: &PalwFreePromptJobV5,
        prompt: &[u32],
        images: &[JobImageV1],
        source: &[u32],
        form: PalwPromptIdsFormV1,
    ) -> Result<(), String> {
        kaspa_consensus_core::palw_fp_job_v5::palw_fp_v5_resolve_class_v1(job, Some(&self.row), true).map_err(|e| e.to_string())?;
        if prompt.len() != job.v4.prompt_tokens as usize || !prompt_token_ids_match_v1(form, prompt, &job.v4.prompt_token_ids_hash) {
            return Err("the prompt is not the job's".into());
        }
        kaspa_consensus_core::palw_fp_job_v5::palw_fp_v5_prompt_head_admitted_v1(&self.row.class.offers, prompt)
            .map_err(|e| e.to_string())?;
        match job.source {
            None if source.is_empty() => {}
            None => return Err("a source for a job that carries none".into()),
            Some(reference) => {
                if source.len() != reference.tokens as usize || !prompt_token_ids_match_v1(form, source, &reference.token_ids_hash) {
                    return Err("the source is not the job's".into());
                }
            }
        }
        if images.len() != job.images.len() {
            return Err(format!("{} images for a job of {}", images.len(), job.images.len()));
        }
        for (k, ((image, reference), slot)) in images.iter().zip(&job.images).zip(&self.row.class.offers.images).enumerate() {
            if palw_gen_image_input_ref_v1(image, slot.tile_len)? != *reference {
                return Err(format!("image {k} is not the one the job names"));
            }
        }
        Ok(())
    }

    /// **Run a V5 job** (see the module doc). The caller has resolved the fence; the job's seed keys
    /// `R`, its V4 rules drive the decode.
    pub fn run_v5(
        &self,
        job: &PalwFreePromptJobV5,
        prompt: &[u32],
        images: &[JobImageV1],
        source: &[u32],
        form: PalwPromptIdsFormV1,
    ) -> Result<GenWorkV1, String> {
        self.check_inputs(job, prompt, images, source, form)?;
        let decode = PalwGenDecodeV1::of(job).ok_or("a V5 job decodes under V4's rules")?;
        let run_job =
            PipelineJob { prompt: prompt.to_vec(), images: images.to_vec(), source: source.to_vec(), ..PipelineJob::default() };
        let execution = palw_gen_execute_v1(
            &self.pipeline,
            &self.programs,
            &self.row.class.layouts,
            &self.params,
            &run_job,
            &decode,
            job.v4.sampling_seed,
        )
        .map_err(|e| e.to_string())?;
        let binding = PalwGenStepBindingV1::of(job, &execution.claim, execution.space.leaf_count());
        Ok(GenWorkV1 {
            job: job.clone(),
            prompt: prompt.to_vec(),
            images: images.to_vec(),
            source: source.to_vec(),
            execution,
            binding,
        })
    }
}

impl GenHeldClassV1<misaka_palw_tir_artifact::PalwTirContainerV2> {
    /// **Hold a registered class from its `PALWTIR2` file**: the file's pipeline and programs are the
    /// row's class's byte for byte, its declared class (when it declares one) is the row's, its
    /// tokenizer the row's — and then [`GenHeldClassV1::hold`]: its tensors hash to the row's
    /// `artifact_root`, streamed from the file in the inventory's order.
    pub fn hold_container(row: PalwGenClassRecordV1, path: &std::path::Path) -> Result<Self, String> {
        let container = misaka_palw_tir_artifact::PalwTirContainerV2::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let h = &container.header;
        if h.pipeline != row.class.pipeline || h.programs != row.class.programs {
            return Err("the container's pipeline or programs are not the registered class's".into());
        }
        if !h.class.is_empty() && borsh::to_vec(&*row.class).map_err(|e| e.to_string())? != h.class {
            return Err("the container declares another class".into());
        }
        if h.tokenizer_id != row.tokenizer_id.as_bytes() {
            return Err("the container's tokenizer is not the class's".into());
        }
        Self::hold(row, container)
    }
}

/// **Write a registered class's `PALWTIR2` file** from weights held in any form: the row's pipeline,
/// programs, class and tokenizer, and every tensor in the inventory's order. Returns the file digest.
pub fn gen_write_container_v1<P: PipelineParams>(
    path: &std::path::Path,
    row: &PalwGenClassRecordV1,
    params: &P,
    meta: String,
) -> Result<[u8; 64], String> {
    let (programs, pipeline) = row.class.decode().map_err(|e| e.to_string())?;
    let class = borsh::to_vec(&*row.class).map_err(|e| e.to_string())?;
    misaka_palw_tir_artifact::write_container_v2(
        path,
        &pipeline,
        &programs,
        class,
        row.tokenizer_id.as_bytes(),
        meta,
        &mut |k, j, l| {
            params
                .params(k)
                .param(j, l)
                .map(|t| t.to_le_bytes())
                .ok_or_else(|| format!("program {k} param {j} layer {l:?} is not held"))
        },
    )
    .map_err(|e| e.to_string())
}

impl GenWorkV1 {
    /// The evidence every court move of this run is built from.
    pub fn evidence<'a, P: PipelineParams>(&'a self, held: &'a GenHeldClassV1<P>) -> PalwGenEvidenceV1<'a> {
        PalwGenEvidenceV1 {
            row: &held.row,
            params: &held.params,
            execution: &self.execution,
            binding: self.binding.clone().into(),
            prompt: &self.prompt,
            negative: &[],
            images: &self.images,
            source: &self.source,
        }
    }

    /// The execution root a V5 commitment carries.
    pub fn execution_root(&self) -> Hash64 {
        self.binding.committed_execution_root
    }
}

// ---------------------------------------------------------------------------------------------
// The worker's frame: one request in, one answer out
// ---------------------------------------------------------------------------------------------

/// One job image on the worker's wire.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct GenWireImageV1 {
    pub h: u32,
    pub w: u32,
    pub rgb: Vec<u8>,
}

/// **A V5 request**: the job, the prompt's ids, the images' bytes and the source's ids (which never
/// ride a transaction — they travel to the worker, and with the capture to the panel).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenWorkerRequestV1 {
    pub job: PalwFreePromptJobV5,
    pub prompt_ids: Vec<u32>,
    pub images: Vec<GenWireImageV1>,
    /// Empty for a job without a source.
    pub source_ids: Vec<u32>,
}

/// **The worker's answer**: the binding (the commitment's execution root, the leaf count, the
/// generated ids) — or a refusal naming the rule, never the prompt.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwGenWorkerAnswerV1 {
    Result { binding: PalwGenStepBindingV1 },
    Refused { why: String },
}

/// **The serving loop's one step**: a request frame's bytes in, an answer out. A request the worker
/// will not run is refused and the held class stays held.
pub fn gen_worker_answer_v1<P: PipelineParams>(
    held: &GenHeldClassV1<P>,
    request: &[u8],
    form: PalwPromptIdsFormV1,
) -> (PalwGenWorkerAnswerV1, Option<GenWorkV1>) {
    let request: PalwGenWorkerRequestV1 = match borsh::from_slice(request) {
        Ok(r) => r,
        Err(_) => return (PalwGenWorkerAnswerV1::Refused { why: "the request does not decode".into() }, None),
    };
    let images: Vec<JobImageV1> = request.images.into_iter().map(|i| JobImageV1 { h: i.h, w: i.w, rgb: i.rgb }).collect();
    match held.run_v5(&request.job, &request.prompt_ids, &images, &request.source_ids, form) {
        Ok(work) => (PalwGenWorkerAnswerV1::Result { binding: work.binding.clone() }, Some(work)),
        Err(why) => (PalwGenWorkerAnswerV1::Refused { why }, None),
    }
}

// ---------------------------------------------------------------------------------------------
// The seat
// ---------------------------------------------------------------------------------------------

/// **A seat's judgment of a V5 claim.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenSeatJudgmentV1 {
    /// The replay's execution root is the claim's.
    Valid,
    /// The replay differs: the court's question (a seat files nothing, never a sampled conviction).
    Differs(String),
    /// The material cannot be judged (it is not the job's, or the job is not the class's).
    Unjudgeable(String),
}

/// **Judge a V5 claim from the material the panel received**: the inputs held to the job, the job
/// replayed, and the replay's execution root held to the claim's.
pub fn gen_seat_judge_v1<P: PipelineParams>(
    held: &GenHeldClassV1<P>,
    claim_execution_root: &Hash64,
    job: &PalwFreePromptJobV5,
    prompt: &[u32],
    images: &[JobImageV1],
    source: &[u32],
    form: PalwPromptIdsFormV1,
) -> GenSeatJudgmentV1 {
    match held.run_v5(job, prompt, images, source, form) {
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

/// **A run's capture**: its inputs and every leaf it committed (as 4-byte lanes), in the claim's
/// one order — what data availability serves and what rebuilds the accused's execution, lies
/// included.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct GenCaptureV1 {
    pub job: PalwFreePromptJobV5,
    pub prompt: Vec<u32>,
    pub images: Vec<GenWireImageV1>,
    pub source: Vec<u32>,
    pub generated: Vec<u32>,
    /// Per stage, every leaf's lanes, in leaf order.
    pub leaves: Vec<Vec<Vec<u8>>>,
}

impl GenCaptureV1 {
    /// A run's capture.
    pub fn of(work: &GenWorkV1) -> Result<Self, String> {
        let mut leaves = Vec::with_capacity(work.execution.space.stages.len());
        for (stage, values) in work.execution.space.stages.iter().zip(&work.execution.leaf_values) {
            let mut lanes = Vec::with_capacity(values.len());
            for (leaf, v) in stage.leaves().iter().zip(values) {
                lanes.push(palw_tir_lanes_wire_v1(leaf.dtype, v).map_err(|e| e.to_string())?);
            }
            leaves.push(lanes);
        }
        Ok(Self {
            job: work.job.clone(),
            prompt: work.prompt.clone(),
            images: work.images.iter().map(|i| GenWireImageV1 { h: i.h, w: i.w, rgb: i.rgb.clone() }).collect(),
            source: work.source.clone(),
            generated: work.execution.claim.generated.clone(),
            leaves,
        })
    }

    /// **The accused's execution, rebuilt from its capture**: the space from the job's trips, every
    /// leaf's hash and every stage's root from the captured lanes, and the binding over them — the
    /// execution the captured commitments describe, whatever computed them.
    pub fn rebuild<P: PipelineParams>(&self, held: &GenHeldClassV1<P>) -> Result<GenWorkV1, String> {
        let images: Vec<JobImageV1> = self.images.iter().map(|i| JobImageV1 { h: i.h, w: i.w, rgb: i.rgb.clone() }).collect();
        let facts_job = PipelineJob {
            prompt: self.prompt.clone(),
            generated: self.generated.clone(),
            source: self.source.clone(),
            ..PipelineJob::default()
        };
        let facts = stage_job_facts(&held.pipeline, &held.programs, &facts_job).map_err(|e| e.to_string())?;
        let trips: Vec<u32> = facts.iter().map(|f| f.trip).collect();
        let space = PalwGenStepSpaceV1::new(&held.pipeline, &held.programs, &held.row.class.layouts, &trips, self.prompt.len() as u32)
            .map_err(|e| e.to_string())?;
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
        let claim = PalwGenClaimRootsV1 {
            step_root: palw_gen_step_root_v1(&stage_roots),
            stage_roots,
            generated: self.generated.clone(),
            output_root: None,
        };
        let binding = PalwGenStepBindingV1::of(&self.job, &claim, space.leaf_count());
        // The capture holds commitments, not a run: the rebuilt execution's run is empty.
        let run = misaka_palw_tir::pipeline::PipelineRun {
            stages: Vec::new(),
            output: misaka_palw_tir::tensor::Tensor::zeros(misaka_palw_tir::types::DType::I32, &[0]),
        };
        let execution = PalwGenExecutionV1 { run, stop: None, space, leaf_values, leaf_hashes, claim, output: None };
        Ok(GenWorkV1 { job: self.job.clone(), prompt: self.prompt.clone(), images, source: self.source.clone(), execution, binding })
    }
}

/// **The first leaf, in the claim's one order, where the accused's commitments part from an honest
/// run of the same job** — the leaf a challenger disputes (the bisection's destination).
pub fn gen_first_divergence_v1(accused: &GenWorkV1, own: &GenWorkV1) -> Option<u64> {
    gen_execution_first_divergence_v1(&accused.execution, &own.execution)
}

/// [`gen_first_divergence_v1`] over two executions (a text run's or a tensor run's): the first leaf, in
/// the claim's one order, whose hashes differ.
pub fn gen_execution_first_divergence_v1(accused: &PalwGenExecutionV1, own: &PalwGenExecutionV1) -> Option<u64> {
    let mut before = 0u64;
    for (a, o) in accused.leaf_hashes.iter().zip(&own.leaf_hashes) {
        if let Some(i) = a.iter().zip(o).position(|(x, y)| x != y) {
            return Some(before + i as u64);
        }
        before += a.len() as u64;
    }
    None
}

/// **A move a party files at the narrowed leaf.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenCourtMoveV1 {
    /// A close (`GenCone`, `GenDecodeToken` or, for a tensor claim, `GenOutputTile`).
    Close(PalwCourtVerdictProofV2),
    /// The responder's root claim at a dissected leaf (`CourtGenRootClaimed`).
    RootClaim(PalwGenRootClaimV1),
}

/// **The moves a party may file at leaf `index` of the accused's execution**, in the order it tries
/// them, each built from the accused's evidence or refused: at a dissected leaf the responder's root
/// claim (the challenger waits for it); elsewhere a cone close, and for a challenger at a tile of
/// a logits row the decode close of that row's id too.
pub fn gen_court_candidates_v1<P: PipelineParams>(
    held: &GenHeldClassV1<P>,
    accused: &GenWorkV1,
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
        let program = &held.programs[held.pipeline.stages[stage as usize].program as usize];
        let post = (program.occurrences().len() - 1) as u16;
        let row_kind = PalwGenLeafKindV1::Commit { occurrence: post, node: program.output.node() };
        let first = accused.job.v4.prompt_tokens.saturating_sub(1);
        if leaf.coord.kind == row_kind && leaf.coord.pos >= first {
            let t = leaf.coord.pos - first;
            out.push((
                "decode",
                evidence
                    .decode_close(t)
                    .map(|close| GenCourtMoveV1::Close(PalwCourtVerdictProofV2::GenDecodeToken { close: Box::new(close) })),
            ));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// What a seat and a party file
// ---------------------------------------------------------------------------------------------

/// **The receipt a seat files on a judged V5 claim**: `Valid` exactly when the replay's execution
/// root is the claim's; nothing otherwise (with the reason, for the log).
pub fn gen_seat_receipt_v1(judgment: &GenSeatJudgmentV1) -> Result<PalwReceiptVerdictV2, String> {
    match judgment {
        GenSeatJudgmentV1::Valid => Ok(PalwReceiptVerdictV2::Valid),
        GenSeatJudgmentV1::Differs(why) => Err(format!("the replay differs — the court's question, not a seat's: {why}")),
        GenSeatJudgmentV1::Unjudgeable(why) => Err(format!("the material is not judgeable: {why}")),
    }
}

/// **The object a party files for one of its moves**, or `None` when the move does not win the
/// party's side. A close carries the verdict the consensus check derives from it (the challenger
/// files only a conviction, the responder only an acquittal); a root claim is the responder's,
/// signed by `sign` over its message (`palw_gen_root_claim_message_v1`).
#[allow(clippy::too_many_arguments)]
pub fn gen_court_object_v1(
    row: &PalwGenClassRecordV1,
    claim_execution_root: &Hash64,
    session_id: Hash64,
    narrowed: u64,
    arity: u8,
    challenger: bool,
    prompt_ids_form: PalwPromptIdsFormV1,
    court: &kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2,
    mv: GenCourtMoveV1,
    sign: &dyn Fn(&[u8]) -> Option<Vec<u8>>,
) -> Result<Option<PalwConsensusObjectV2>, String> {
    // The court's own work limits (the ruleset's), as the acceptance layer applies them.
    let limits = &kaspa_consensus_core::palw_court_v2::palw_tir_court_limits_v1(court);
    match mv {
        GenCourtMoveV1::Close(proof) => {
            let outcome = match &proof {
                PalwCourtVerdictProofV2::GenCone { close } => {
                    check_gen_cone_close_v1(close, row, &row.class_id, claim_execution_root, Some(narrowed), prompt_ids_form, limits)
                }
                PalwCourtVerdictProofV2::GenDecodeToken { close } => {
                    check_gen_decode_close_v1(close, row, &row.class_id, claim_execution_root, Some(narrowed))
                }
                PalwCourtVerdictProofV2::GenOutputTile { close } => {
                    check_gen_output_close_v1(close, row, &row.class_id, claim_execution_root, Some(narrowed))
                }
                _ => return Err("not a generative close a party files at a leaf".into()),
            }
            .map_err(|e| e.to_string())?;
            let verdict = match outcome {
                Some(_) => PalwCourtVerdictV2::ExecutorGuilty,
                None => PalwCourtVerdictV2::ChallengerDefeated,
            };
            let wins = (verdict == PalwCourtVerdictV2::ExecutorGuilty) == challenger;
            Ok(wins.then_some(PalwConsensusObjectV2::CourtClosed { session_id, verdict, proof }))
        }
        GenCourtMoveV1::RootClaim(root) => {
            if challenger {
                return Ok(None);
            }
            let message = palw_gen_root_claim_message_v1(&session_id, &root);
            let signature = sign(&message).ok_or("the responder's key refused to sign")?;
            Ok(Some(PalwConsensusObjectV2::CourtGenRootClaimed { session_id, root: Box::new(root), arity, signature }))
        }
    }
}
