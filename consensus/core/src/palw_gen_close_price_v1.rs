//! **RFC-0003 PALW-GEN-20: the carried size of every close of a generative class** — the price the pipeline admission
//! ([`crate::palw_gen_admission_v1`]) holds against what the chain can carry, and the catalog's `max_close_bytes`.
//!
//! A close here is the unit a conviction rides: a cone close (`GenCone`, tag 10) carries the disputed leaf and every unit its cone
//! reads — leaves of its own stage and of EARLIER stages (each with its own path to ITS stage's root), the class's param leaves (each
//! a whole `PalwArtifactOpeningV1` with its path to the class's one artifact root) and job-image tiles — beside the binding (the job,
//! every stage's root) and the ids the stage reads. The pipeline admission used to price it by the IR admission's necessary condition
//! (the operand bytes at element granularity beside a frame constant), which sits far below what the court carries wherever a cone
//! reads many leaves: every leaf has a path, every param row a leaf of its own with a path of the CLASS's inventory depth, an edge is
//! a leaf of another stage. Priced that low a registration is admitted whose convictions cannot ride a carrier — a cost asymmetry, not a
//! proof; and a catalog entry that records the low number lies about the class.
//!
//! **The price here is an upper bound of the measured close at every leaf of every job** (the sweep that holds it to that:
//! `misaka-palw-sdk/tests/gen_sd3_class.rs`). It is the PALW-TIR-38 twin ([`crate::palw_tir_close_size_v1`]) run over every stage of
//! the pipeline: the abstract twin of the court's own evaluation over element SETS — the same contexts, the same index maps primitive by
//! primitive (the generative court's evaluator is the IR's over the stage's version-1 view, `demand_v2`), a superset where the court
//! reads by a value — with the three things a pipeline stage's court does differently supplied:
//!
//! * **inputs**: an input is not an artifact row. A random or a job-bound input opens nothing; an edge input is the commit leaf of the
//!   upstream stage's output node that holds the element, opened under the UPSTREAM stage's root; an image byte is one input tile
//!   under the job's `input_root` ([`PalwGenTwinStageV1`]);
//! * **a state `post` writes** (NF-29) is read as the committed write of the position before, never a checkpoint;
//! * **the form**: every leaf opened with its own whole path at its own stage's depth (a run shares no siblings), every param as a whole
//!   opening at the class's inventory depth, the frame the generative close object serialized at its WIDEST binding — the executor's
//!   ML-DSA-87 key, every stage root, the decode rules, the images and the source at their maxima, the ids the stage reads at the
//!   offers' maxima ([`PalwGenClosePricingV1`]).
//!
//! The sizing is bounded work (`PALW_GEN_CLOSE_SIZING_WORK_CAP_V1` for the whole class): a class whose sizing would do more is refused by
//! name, never run.

use crate::Hash64;
use crate::palw_class_admission_v2::PalwClassAdmissionError;
use crate::palw_court_v2::PalwCourtVerdictProofV2;
use crate::palw_decode_pipeline_v4::DecodeConfigV4;
use crate::palw_fp_job_v5::PalwFreePromptJobV5;
use crate::palw_freeprompt_v3::{PALW_FP_V4_VERSION, PalwFreePromptJobV3};
use crate::palw_gen_class_v1::{PalwGenClassV1, PalwGenImageInputRefV1, PalwGenSourceRefV1};
use crate::palw_gen_close_v1::{
    PALW_GEN_CLOSE_VERSION_V1, PalwGenBindingV1, PalwGenConeCloseV1, PalwGenDecodeCloseV1, PalwGenLeafOpeningV1, PalwGenOutputCloseV1,
    PalwGenRootClaimV1, PalwGenStepBindingV1, PalwGenTensorBindingV1, palw_gen_stage_reads_negative_v1, palw_gen_stage_reads_prompt_v1,
    palw_gen_stage_reads_source_v1,
};
use crate::palw_gen_job_v1::{PALW_GEN_JOB_VERSION_V1, PalwGenBodyV1, PalwGenImageBodyV1, PalwGenJobV1, PalwJobEnvelopeV1};
use crate::palw_gen_step_v1::{PalwGenLeafCoordV1, PalwGenLeafKindV1, PalwGenStepSpaceV1};
use crate::palw_gen_v1::PalwGenProfileV1;
use crate::palw_state_v2::{PalwConsensusObjectV2, PalwCourtVerdictV2};
use crate::palw_tir_class_v1::PalwTirLayoutV1;
use crate::palw_tir_close_size_v1::{
    MOVE_CARRIER_EXTRA_BYTES, MOVE_SIGNATURE_BYTES, PalwGenClosePricingV1, PalwGenTwinInputV1, PalwGenTwinStageV1, PalwTirCloseBoundV1,
    PalwTirCloseSizingV1, PalwTirParamFormV1, palw_gen_worst_closes_v1,
};
use crate::palw_tir_court_v1::PalwTirInventoryIndexV1;
use crate::palw_tir_dissect_v1::{PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirRangeClaimV1};
use crate::palw_tir_step_v1::PalwTirStepSpaceV1;
use crate::palw_v2::PalwJobContextV2;
use crate::tx::{TransactionId, TransactionOutpoint};
use misaka_palw_tir::demand_v2::post_writers_v2;
use misaka_palw_tir::pipeline::{Binding, TirPipelineV1, TripRule};
use misaka_palw_tir::program_v2::{InputSource, OutputDecl, TirProgramV2};
use misaka_palw_tir::validate_v2::validate_v2;

/// **The most work one generative class's close sizing may do**, in the twin's steps: **the IR's own cap**
/// ([`crate::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1`], `2^26`) — the generative gate is no looser than the IR one,
/// by construction. A registration's CPU is bounded before it is spent; a class whose sizing would do more is refused by name.
///
/// The reduced SD3 pipeline (13 stages, 168 commit points) sizes in **66,871,506 steps — 237,358 (0.35 %) under it**: three quarters of
/// it the ten VAE stages, a fifth the denoiser, the text stages almost nothing (the sizing walks every tile of a node at its widest
/// position, and a tile of a convolution's output reads `3 · Cin` input leaves). That margin is the drill class's, and it is pinned
/// (`misaka-palw-sdk/tests/gen_sd3_class.rs`, `the_gate_sizes_every_close_of_the_sd3_class_within_its_work_cap`): a lowering change that
/// eats it fails there, by name, before the gate refuses the class. A real-size image stack is past any cap until the sizing walks a tile
/// CLASS rather than every tile (the recorded follow-up).
pub const PALW_GEN_CLOSE_SIZING_WORK_CAP_V1: u64 = crate::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1;

/// The most elements of a decode rule's logit bias, and the stop sequences and their ids (`DecodeConfigV4`'s own bounds): the
/// widest a text job's decode config rides in a binding.
const DECODE_MAX_LOGIT_BIAS: usize = 300;
const DECODE_MAX_STOP_SEQUENCES: usize = 4;
const DECODE_MAX_STOP_IDS: usize = 16;

/// **A stage's widest job as the twin sizes it**: prefill 1 and `max_trip` positions in all — `post` runs at every position, so every
/// position's cones are sized (a text stage consumes its logits from the prompt's last position on: a subset).
pub fn palw_gen_stage_job_context_v1(max_trip: u32) -> PalwJobContextV2 {
    PalwJobContextV2 {
        version: 2,
        network_id: vec![0; 8],
        job_id: Hash64::default(),
        job_nullifier: Hash64::default(),
        assignment_id: Hash64::default(),
        execution_seed: [0; 32],
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: Hash64::default(),
        trace_scheme_id: Hash64::default(),
        cu_ruleset_id: Hash64::default(),
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: Hash64::default(),
        declared_prefill_tokens: 1,
        exact_decode_tokens: max_trip.max(1),
        max_context_tokens: u32::MAX,
    }
}

fn ceil_log2(n: u64) -> u64 {
    if n <= 1 { 0 } else { 64 - (n - 1).leading_zeros() as u64 }
}

/// **The binding of the widest job a close can carry**: every variable part at its maximum — the executor's key (ML-DSA-87), every
/// stage's root, a text job's decode rules, its images and its source, its generated ids.
fn widest_binding(class: &PalwGenClassV1, pipeline: &TirPipelineV1) -> PalwGenBindingV1 {
    let zero = Hash64::from_bytes([0; 64]);
    let stage_roots = vec![zero; pipeline.stages.len()];
    let pubkey = vec![0u8; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN];
    let bond = TransactionOutpoint::new(TransactionId::from_bytes([0; 64]), 0);
    if PalwGenProfileV1::from_tag(class.profile) == Some(PalwGenProfileV1::Text) {
        let offers = &class.offers;
        let job = PalwFreePromptJobV5 {
            v4: PalwFreePromptJobV3 {
                version: PALW_FP_V4_VERSION,
                network_domain: zero,
                class_id: zero,
                executor_bond: bond,
                executor_pubkey: pubkey,
                operator_id: zero,
                anchor_block: zero,
                anchor_daa: 0,
                job_nonce: [0; 32],
                tokenizer_id: zero,
                prompt_token_ids_hash: zero,
                prompt_tokens: 0,
                decode_token_limit: 0,
                max_context_tokens: 0,
                privacy_mode: 0,
                prompt_mode: 0,
                sampling_seed: [0; 32],
                temperature_q: 0,
                decode: Some(DecodeConfigV4 {
                    repeat_penalty_q: 0,
                    penalty_window: 0,
                    frequency_penalty_q: 0,
                    presence_penalty_q: 0,
                    logit_bias: vec![(0, 0); DECODE_MAX_LOGIT_BIAS],
                    stop_sequences: vec![vec![0; DECODE_MAX_STOP_IDS]; DECODE_MAX_STOP_SEQUENCES],
                }),
                tail: None,
            },
            // A job carries at most 16 images (FP Job V5): a registration's offers are held to it at the preflight; the bound is also
            // what keeps this allocation small whatever a class says.
            images: vec![PalwGenImageInputRefV1 { input_root: zero, h: 0, w: 0 }; offers.images.len().min(16)],
            source: (offers.max_source_tokens > 0).then_some(PalwGenSourceRefV1 { token_ids_hash: zero, tokens: 0 }),
        };
        PalwGenBindingV1::Text(PalwGenStepBindingV1 {
            version: PALW_GEN_CLOSE_VERSION_V1,
            job,
            stage_roots,
            // The generated ids ride the binding too: priced by their number (`ids_bytes`), never allocated.
            generated: Vec::new(),
            step_leaf_count: 0,
            committed_execution_root: zero,
        })
    } else {
        // An image body is the wider of the two built bodies (an embedding's is a reference and a width).
        let body = PalwGenBodyV1::Image(PalwGenImageBodyV1 {
            prompt_token_ids_hash: zero,
            prompt_tokens: 0,
            negative_token_ids_hash: zero,
            negative_tokens: 0,
            guidance_q: 0,
            image_index: 0,
            sampler_id: zero,
            steps: 0,
            width: 0,
            height: 0,
            output: 0,
        });
        PalwGenBindingV1::Tensor(PalwGenTensorBindingV1 {
            version: PALW_GEN_CLOSE_VERSION_V1,
            job: PalwGenJobV1 {
                version: PALW_GEN_JOB_VERSION_V1,
                envelope: PalwJobEnvelopeV1 {
                    network_domain: zero,
                    class_id: zero,
                    executor_bond: bond,
                    executor_pubkey: pubkey,
                    operator_id: zero,
                    anchor_block: zero,
                    anchor_daa: 0,
                    job_nonce: [0; 32],
                    privacy_mode: 0,
                    prompt_mode: 0,
                },
                seed: [0; 32],
                body,
            },
            stage_roots,
            step_leaf_count: 0,
            output_root: zero,
            committed_execution_root: zero,
        })
    }
}

/// **A cone close that opens nothing**, for stage `stage` at the widest binding: the ids the stage reads at the offers' maxima and the
/// disputed leaf's opening at the stage's depth, no lane.
fn empty_close(class: &PalwGenClassV1, pipeline: &TirPipelineV1, binding: &PalwGenBindingV1, stage: usize, depth: u64) -> PalwGenConeCloseV1 {
    let zero = Hash64::from_bytes([0; 64]);
    let _ = (class, pipeline, stage);
    PalwGenConeCloseV1 {
        version: PALW_GEN_CLOSE_VERSION_V1,
        binding: binding.clone(),
        // The ids a stage reads ride whole; they are priced by their number (`ids_bytes`), never allocated.
        prompt_ids: Vec::new(),
        negative_ids: Vec::new(),
        source_ids: Vec::new(),
        disputed: PalwGenLeafOpeningV1 {
            // The widest coordinate: a checkpoint of a per-layer state (a commit leaf's is a byte shorter).
            coord: PalwGenLeafCoordV1 {
                stage: stage as u8,
                pos: 0,
                kind: PalwGenLeafKindV1::State { state: 0, layer: Some(0) },
                tile: 0,
            },
            lanes_le: Vec::new(),
            path: vec![zero; depth as usize],
        },
        operands: Vec::new(),
        image_tiles: Vec::new(),
        params: Vec::new(),
    }
}

/// The carried bytes of `object`.
fn object_len(object: &PalwConsensusObjectV2) -> Result<u64, String> {
    borsh::to_vec(object).map(|v| v.len() as u64).map_err(|e| e.to_string())
}

/// **What the ids a close carries weigh**, beyond the empty vectors the frame serializes: the prompt (with the class's forced prefix, at
/// its head), the negative prompt and the source, each four bytes an id, exactly when the stage reads them
/// (`palw_gen_stage_reads_*`), and a text binding's generated ids, four bytes each, at the text stage's `max_trip`.
fn ids_bytes(class: &PalwGenClassV1, pipeline: &TirPipelineV1, stage: usize) -> u64 {
    let offers = &class.offers;
    let per = |reads: bool, n: u64| if reads { 4 * n } else { 0 };
    let generated = if PalwGenProfileV1::from_tag(class.profile) == Some(PalwGenProfileV1::Text) {
        pipeline.stages.iter().find(|st| matches!(st.trip, TripRule::TextStream)).map_or(0, |st| 4 * st.max_trip as u64)
    } else {
        0
    };
    per(palw_gen_stage_reads_prompt_v1(pipeline, stage), offers.max_prompt_tokens as u64 + offers.forced_prompt_prefix.len() as u64)
        + per(palw_gen_stage_reads_negative_v1(pipeline, stage), offers.max_negative_tokens as u64)
        + per(palw_gen_stage_reads_source_v1(pipeline, stage), offers.max_source_tokens as u64)
        + generated
}

/// **The frames of stage `stage`'s closes**: the close object with nothing opened, and the dissected cone's root claim (signed, with
/// its carrier's extra) around the same close.
fn frames(
    class: &PalwGenClassV1,
    pipeline: &TirPipelineV1,
    binding: &PalwGenBindingV1,
    stage: usize,
    depth: u64,
) -> Result<(u64, u64), String> {
    let zero = Hash64::from_bytes([0; 64]);
    let close = empty_close(class, pipeline, binding, stage, depth);
    let ids = ids_bytes(class, pipeline, stage);
    let frame = ids + object_len(&PalwConsensusObjectV2::CourtClosed {
        session_id: zero,
        verdict: PalwCourtVerdictV2::ChallengerDefeated,
        proof: PalwCourtVerdictProofV2::GenCone { close: Box::new(close.clone()) },
    })?;
    let root_frame = ids + object_len(&PalwConsensusObjectV2::CourtGenRootClaimed {
        session_id: zero,
        root: Box::new(PalwGenRootClaimV1 {
            version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
            elements: Vec::new(),
            totals: PalwTirRangeClaimV1 { partials: Vec::new() },
            finalize: Box::new(close),
        }),
        arity: 0,
        signature: vec![0; MOVE_SIGNATURE_BYTES],
    })? + MOVE_CARRIER_EXTRA_BYTES;
    Ok((frame, root_frame))
}

/// **The commit tile and the elements per position of stage `up`'s output node** — what an edge into it is read through (the court
/// reads the committed leaf of the output node at the stage's `post` occurrence).
fn upstream_output(programs: &[TirProgramV2], pipeline: &TirPipelineV1, layouts: &[PalwTirLayoutV1], up: usize) -> Result<(u32, u64), String> {
    let stage = pipeline.stages.get(up).ok_or_else(|| format!("an edge from stage {up}, which does not exist"))?;
    let program = &programs[stage.program as usize];
    let (post, node) = (program.schedule.post as usize, program.output.node() as usize);
    let elements = program.blocks.get(post).and_then(|b| b.nodes.get(node)).map(|n| n.out.elements_at(1)).ok_or("an output node outside its block")?;
    let layout = layouts.get(up).ok_or_else(|| format!("no layout for stage {up}"))?;
    let mut commits = layout.commit_tiles.iter();
    for (bi, block) in program.blocks.iter().enumerate() {
        for (ni, n) in block.nodes.iter().enumerate() {
            if !n.commit {
                continue;
            }
            let tile = *commits.next().ok_or("fewer commit tiles than commit points")?;
            if bi == post && ni == node {
                return Ok((tile, elements));
            }
        }
    }
    Err(format!("stage {up}'s output node is no commit point"))
}

/// **How every stage reads its inputs** (see [`PalwGenTwinStageV1`]): the class's bindings, over the court's own answers
/// (`palw_gen_stage_answers_v1`).
pub fn palw_gen_twin_stages_v1(
    class: &PalwGenClassV1,
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
) -> Result<Vec<PalwGenTwinStageV1>, String> {
    let mut out = Vec::with_capacity(pipeline.stages.len());
    for (s, st) in pipeline.stages.iter().enumerate() {
        let program = &programs[st.program as usize];
        let info = validate_v2(program).map_err(|e| format!("stage {s}: {e}"))?;
        let mut bindings = st.bind.iter();
        let mut inputs = Vec::with_capacity(program.inputs.len());
        for d in &program.inputs {
            inputs.push(match d.source {
                InputSource::Random { .. } => PalwGenTwinInputV1::Free,
                InputSource::External { .. } => match bindings.next().ok_or_else(|| format!("stage {s}: a binding per external input"))? {
                    Binding::StageRows { stage: up, drop, .. } => {
                        let (tile, elements) = upstream_output(programs, pipeline, &class.layouts, *up as usize)?;
                        let per_row: u64 = d.shape.iter().skip(1).fold(1u64, |acc, x| acc.saturating_mul(*x as u64));
                        PalwGenTwinInputV1::Edge { stage: *up, rows: Some((*drop, per_row)), tile, elements }
                    }
                    Binding::StageFinal { stage: up } => {
                        let (tile, elements) = upstream_output(programs, pipeline, &class.layouts, *up as usize)?;
                        PalwGenTwinInputV1::Edge { stage: *up, rows: None, tile, elements }
                    }
                    Binding::JobImage { index } => {
                        let slot = class.offers.images.get(*index as usize).ok_or_else(|| format!("stage {s}: the class has no image {index}"))?;
                        PalwGenTwinInputV1::Image { image: *index, tile_len: slot.tile_len }
                    }
                    // A scalar, a token run or a count: the accepted job's value, recomputed by every party.
                    _ => PalwGenTwinInputV1::Free,
                },
            });
        }
        out.push(PalwGenTwinStageV1 { first_input: info.first_input_param, inputs, post_writers: post_writers_v2(program, &info) });
    }
    Ok(out)
}

/// **Everything the twin needs to size stage `stage`'s closes**: its step space over the stage's version-1 view, the view's inventory
/// index, the stage's reading of its inputs, its pricing and its widest job.
pub struct PalwGenStageSizingV1 {
    pub space: PalwTirStepSpaceV1,
    pub inventory: PalwTirInventoryIndexV1,
    pub model: PalwGenTwinStageV1,
    pub pricing: PalwGenClosePricingV1,
    pub job: PalwJobContextV2,
}

/// The class-wide tables every stage's sizing reads: each stage's step tree depth over the widest job, each image slot's tile tree,
/// every stage's reading of its inputs and the widest binding.
pub struct PalwGenClassSizingV1 {
    stage_depths: Vec<u64>,
    image_tiles: Vec<(u64, u64)>,
    models: Vec<PalwGenTwinStageV1>,
    binding: PalwGenBindingV1,
}

impl PalwGenClassSizingV1 {
    /// The tables of a decoded class.
    pub fn new(class: &PalwGenClassV1, pipeline: &TirPipelineV1, programs: &[TirProgramV2]) -> Result<Self, PalwClassAdmissionError> {
        let refused = |why: String| PalwClassAdmissionError::GenClass(format!("the close sizing refuses the class: {why}"));
        // Every stage's tree depth over the widest job.
        let widest: Vec<u32> = pipeline.stages.iter().map(|st| st.max_trip).collect();
        let counts = PalwGenStepSpaceV1::stage_leaf_counts_v1(pipeline, programs, &class.layouts, &widest, Some(0), 1)
            .map_err(|e| PalwClassAdmissionError::GenClass(e.to_string()))?;
        let stage_depths: Vec<u64> = counts.iter().map(|n| ceil_log2(u64::try_from(*n).unwrap_or(u64::MAX))).collect();
        // Every image slot's input-tile tree.
        let image_tiles: Vec<(u64, u64)> = class
            .offers
            .images
            .iter()
            .map(|slot| {
                let bytes = (slot.h as u64).saturating_mul(slot.w as u64).saturating_mul(3);
                (ceil_log2(bytes.div_ceil(slot.tile_len.max(1) as u64)), slot.tile_len as u64)
            })
            .collect();
        let models = palw_gen_twin_stages_v1(class, pipeline, programs).map_err(refused)?;
        Ok(Self { stage_depths, image_tiles, models, binding: widest_binding(class, pipeline) })
    }

    /// **Stage `s`'s sizing inputs.**
    pub fn stage(
        &self,
        class: &PalwGenClassV1,
        pipeline: &TirPipelineV1,
        programs: &[TirProgramV2],
        s: usize,
    ) -> Result<PalwGenStageSizingV1, PalwClassAdmissionError> {
        let refused = |why: String| PalwClassAdmissionError::GenClass(format!("the close sizing refuses the class: {why}"));
        let st = &pipeline.stages[s];
        let program = &programs[st.program as usize];
        let info = validate_v2(program).map_err(|e| refused(format!("stage {s}: {e}")))?;
        let layout = class.layouts.get(s).ok_or_else(|| refused(format!("stage {s} has no layout")))?;
        let space = PalwTirStepSpaceV1::from_program(info.view.clone(), info.v1.clone(), layout.clone())
            .map_err(|e| refused(format!("stage {s}: {e}")))?;
        let inventory = PalwTirInventoryIndexV1::new(&space.program)
            .ok_or_else(|| refused(format!("stage {s} has no param or input to index")))?;
        let (frame, root_frame) = frames(class, pipeline, &self.binding, s, self.stage_depths[s]).map_err(refused)?;
        let pricing = PalwGenClosePricingV1 {
            stage_depths: self.stage_depths.clone(),
            image_tiles: self.image_tiles.clone(),
            name_extra: 2 + st.program.to_string().len() as u64,
            frame,
            root_frame,
        };
        Ok(PalwGenStageSizingV1 { space, inventory, model: self.models[s].clone(), pricing, job: palw_gen_stage_job_context_v1(st.max_trip) })
    }
}

/// **The worst close of every commit point of every stage** of a generative class, in carried bytes — over every job of the class
/// (the widest: every stage at its `max_trip`) — with the work the sizing did. `court` says whether a cone that reduces over the
/// history is dissected (the k-ary court is armed); `class_inventory_leaves` is the class's one artifact tree's leaf count.
///
/// The sizing stops at the first commit point whose worst close passes `carriable`, or whose dissection root claim passes `root_cap`
/// (the last bound of that stage is then the offending one); a sizing that would pass `work_cap` is refused by name.
#[allow(clippy::too_many_arguments)]
pub fn palw_gen_worst_closes_of_class_v1(
    class: &PalwGenClassV1,
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
    class_inventory_leaves: u32,
    court: bool,
    carriable: u64,
    root_cap: u64,
    work_cap: u64,
    twin: crate::palw_tir_close_range_v1::PalwTirCloseTwinV1,
) -> Result<(Vec<Vec<PalwTirCloseBoundV1>>, u64), PalwClassAdmissionError> {
    let tables = PalwGenClassSizingV1::new(class, pipeline, programs)?;
    let mut remaining = work_cap;
    let mut all = Vec::with_capacity(pipeline.stages.len());
    for s in 0..pipeline.stages.len() {
        let z = tables.stage(class, pipeline, programs, s)?;
        let sizing = PalwTirCloseSizingV1 { form: PalwTirParamFormV1::PerLeaf, court, cap: remaining, stop_above: Some((carriable, root_cap)) };
        // The twin at the block: the element twin, or the range twin past `palw_gen_range_twin_v1` (the same bounds, fewer steps).
        let sized = match twin {
            crate::palw_tir_close_range_v1::PalwTirCloseTwinV1::Element => {
                palw_gen_worst_closes_v1(&z.space, &z.inventory, &z.job, &sizing, s, class_inventory_leaves, &z.model, z.pricing)
            }
            crate::palw_tir_close_range_v1::PalwTirCloseTwinV1::Range => crate::palw_tir_close_range_v1::palw_gen_worst_closes_range_v1(
                &z.space,
                &z.inventory,
                &z.job,
                &sizing,
                s,
                class_inventory_leaves,
                &z.model,
                z.pricing,
            ),
        };
        let (bounds, work) =
            sized.map_err(|e| {
                if e == crate::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_OVER_CAP_V1 {
                    PalwClassAdmissionError::TirExceeds {
                        limit: "generative close sizing work",
                        at: format!("stage {s}'s terminal closes"),
                        value: work_cap.saturating_add(1),
                        cap: work_cap,
                    }
                } else {
                    PalwClassAdmissionError::GenClass(format!("the close sizing refuses the class: stage {s}: {e}"))
                }
            })?;
        remaining = remaining.saturating_sub(work);
        let past = bounds.last().is_some_and(|b| b.close_bytes > carriable || (b.dissected && b.root_claim_bytes > root_cap));
        all.push(bounds);
        if past {
            // The caller names the first refusal; the rest of the pipeline is not sized.
            break;
        }
    }
    Ok((all, work_cap - remaining))
}

// =================================================================================================
// The closes that are not cone closes
// =================================================================================================

/// **A close of a generative class that no cone close stands for** (RFC-0003 PALW-GEN-20): the other two proofs a one-move accusation
/// may carry (`palw_gen_one_move_proof_is_admissible_v1`), each a whole object of the class's own shape rather than the units one cone
/// reads — so each is priced from the class alone, in closed form, and an admitted class's every close kind is priced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwGenWholeCloseKindV1 {
    /// **`GenDecodeToken`** (tag 11), a text class's: a generated id against the committed logits row it was selected from — EVERY tile of
    /// the row, each opened under the text stage's root (the decode rule selects over all the lanes). The bytes are the vocabulary's four
    /// bytes a lane plus, per tile, a coordinate and a whole path: `4 · V + ⌈V / T⌉ · (23 + 64 · depth)` beside the frame and the
    /// binding with its generated ids.
    Decode,
    /// **`GenOutputTile`** (tag 16), a tensor class's: one output tile of the canonical output, proven under the claim's `output_root`,
    /// against the output node's committed step tile of the same lanes under its stage's root: `T · (element bytes + 4)` and two paths
    /// beside the frame and the binding.
    Output,
}

impl PalwGenWholeCloseKindV1 {
    /// The name a refusal carries.
    pub fn what(self) -> &'static str {
        match self {
            Self::Decode => "generative decode close bytes as carried",
            Self::Output => "generative output close bytes as carried",
        }
    }
}

/// **The whole closes of a class and what each weighs as carried** (the close object, serialized at the widest binding, and every unit it
/// opens at its widest): the decode close of a text class, the output close of a tensor class. Cheap — closed form, no twin — so the gate
/// asks it before the sizing.
pub fn palw_gen_whole_closes_v1(
    class: &PalwGenClassV1,
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
) -> Result<Vec<(PalwGenWholeCloseKindV1, u64)>, PalwClassAdmissionError> {
    let refused = |why: String| PalwClassAdmissionError::GenClass(format!("the close sizing refuses the class: {why}"));
    let tables = PalwGenClassSizingV1::new(class, pipeline, programs)?;
    let out = pipeline.output_stage as usize;
    let stage = pipeline.stages.get(out).ok_or_else(|| refused("no output stage".into()))?;
    let program = &programs[stage.program as usize];
    let depth = tables.stage_depths[out];
    let zero = Hash64::from_bytes([0; 64]);
    let (tile, elements) = upstream_output(programs, pipeline, &class.layouts, out).map_err(&refused)?;
    match program.output {
        OutputDecl::Logits { .. } => {
            let PalwGenBindingV1::Text(binding) = &tables.binding else {
                return Err(refused("a logits output stage is a text class's".into()));
            };
            let close = PalwGenDecodeCloseV1 { version: PALW_GEN_CLOSE_VERSION_V1, binding: binding.clone(), t: 0, row: Vec::new() };
            let frame = object_len(&PalwConsensusObjectV2::CourtClosed {
                session_id: zero,
                verdict: PalwCourtVerdictV2::ExecutorGuilty,
                proof: PalwCourtVerdictProofV2::GenDecodeToken { close: Box::new(close) },
            })
            .map_err(&refused)?;
            // The binding carries the generated ids too (up to the text stage's `max_trip`, four bytes each).
            let generated = 4 * stage.max_trip as u64;
            let tiles = elements.div_ceil(tile.max(1) as u64);
            let row = tiles.saturating_mul(23 + 64 * depth).saturating_add(4 * elements);
            Ok(vec![(PalwGenWholeCloseKindV1::Decode, frame + generated + row)])
        }
        OutputDecl::Rows { .. } | OutputDecl::Final { .. } => {
            let PalwGenBindingV1::Tensor(binding) = &tables.binding else {
                return Err(refused("a rows or final output stage is a tensor class's".into()));
            };
            let layout = class.output.layout().map_err(|e| refused(format!("the output header: {e:?}")))?;
            let step_tile = PalwGenLeafOpeningV1 {
                coord: PalwGenLeafCoordV1 { stage: out as u8, pos: 0, kind: PalwGenLeafKindV1::State { state: 0, layer: Some(0) }, tile: 0 },
                lanes_le: Vec::new(),
                path: vec![zero; depth as usize],
            };
            let close = PalwGenOutputCloseV1 {
                version: PALW_GEN_CLOSE_VERSION_V1,
                binding: binding.clone(),
                tile: 0,
                output_tile: Vec::new(),
                output_proof: Vec::new(),
                step_tile,
            };
            let frame = object_len(&PalwConsensusObjectV2::CourtClosed {
                session_id: zero,
                verdict: PalwCourtVerdictV2::ExecutorGuilty,
                proof: PalwCourtVerdictProofV2::GenOutputTile { close: Box::new(close) },
            })
            .map_err(&refused)?;
            // The output tile is the step tile's lanes (the output node's commit tile) at the kind's width; its path is the output
            // tree's, over `⌈E / T⌉` tiles.
            let out_tile = palw_gen_output_tile_len(class, pipeline, programs).unwrap_or(tile).max(1) as u64;
            let lanes = out_tile.min(layout.elements.max(1));
            let proof = 64 * ceil_log2(layout.elements.div_ceil(out_tile));
            Ok(vec![(PalwGenWholeCloseKindV1::Output, frame + lanes * (layout.element_bytes as u64 + 4) + proof)])
        }
    }
}

/// The output node's commit tile (PALW-OUT-3's alignment: the output tile is the step tile).
fn palw_gen_output_tile_len(class: &PalwGenClassV1, pipeline: &TirPipelineV1, programs: &[TirProgramV2]) -> Option<u32> {
    crate::palw_gen_class_v1::palw_gen_output_tile_len_v1(pipeline, programs, &class.layouts)
}
