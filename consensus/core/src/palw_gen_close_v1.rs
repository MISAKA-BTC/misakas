//! **RFC-0003: the generative court's consensus objects** — what pins a pipeline claim's execution
//! (its binding), and the two closes a `CourtClosed` carries for one
//! (`PalwCourtVerdictProofV2::GenCone`, tag 10; `PalwCourtVerdictProofV2::GenDecodeToken`, tag 11;
//! Phase F's allocation).
//!
//! **The binding.** A V5 claim (RFC-0003 §II.2.1) commits one step tree (`palw_gen_step_v1`: every
//! stage's leaves, a root per stage, the step root over them) and its generated ids; its commitment's
//! `execution_root` is
//!
//! ```text
//! gen_execution_root = H64(key "misaka-palw/gen/execution-root/v1",
//!                          fp_job_id_v5 ‖ class_id ‖ le64(step_leaf_count) ‖ step_root ‖ le32(|generated|) ‖ le32(id)…)
//! ```
//!
//! — the job (its id covers every field, the images' roots among them), the class, the tree and the
//! answer. [`PalwGenStepBindingV1`] carries the parts; the court recomputes the root and holds it to
//! the claim's, and holds the job to the class the chain registered (`gen_classes`), before it reads
//! anything else. A leaf count that is not the job's canonical count convicts from the binding alone
//! (`StepLeafCountNotCanonical`), as Phase F's binding does.
//!
//! **The leaf order a session narrows over** is the claim's one order: stage-major, every leaf of a
//! stage after every leaf of the stages before it ([`PalwGenStepSpaceV1::global_index`]). A cone
//! close must open the leaf the ladder narrowed to; a decode close must name the generated id whose
//! logits row the narrowed leaf is a tile of.
//!
//! **On the wire** a leaf's values ride as their 4-byte lanes (PALW-TIR-5), read back under the
//! dtype the space gives the leaf ([`PalwGenLeafOpeningV1`]). The prompt ids ride only when the
//! disputed stage reads them (the text stage, or a stage whose tokens or bindings read the prompt),
//! whole, checked against the job's `prompt_token_ids_hash` in the network's form; a dispute
//! anywhere else reveals nothing of the prompt.

use crate::Hash64;
use crate::palw_artifact::PalwArtifactOpeningV1;
use crate::palw_fp_job_v5::{PalwFreePromptJobV5, fp_job_id_v5};
use crate::palw_gen_class_v1::PalwGenClassRecordV1;
use crate::palw_gen_court_v1::{
    PalwGenCloseRefusalV1, PalwGenCloseV1, PalwGenCourtCaseV1, PalwGenDrawV1, PalwGenImageRefV1, PalwGenImageTileV1, PalwGenVerdictV1,
    palw_gen_adjudicate_leaf_v1, palw_gen_decode_door_v1,
};
use crate::palw_gen_step_v1::{PalwGenLeafCoordV1, PalwGenLeafKindV1, PalwGenOpenedLeafV1, PalwGenStepSpaceV1, palw_gen_step_root_v1};
use crate::palw_gen_worker_v1::{PalwGenClaimRootsV1, PalwGenDecodeV1};
use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use crate::palw_step_leg::PalwStepFaultV1;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::pipeline::{Binding, PipelineJob, StageJobFacts, TirPipelineV1, TokenSource, TripRule, stage_job_facts};
use misaka_palw_tir::program_v2::TirProgramV2;

/// Wire version of every object in this module.
pub const PALW_GEN_CLOSE_VERSION_V1: u16 = 1;
/// Key of [`palw_gen_execution_root_v1`] — its own, so a pipeline execution root never verifies as an
/// FP, a legacy or an IR one.
pub const PALW_GEN_EXECUTION_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/gen/execution-root/v1";
/// The evidence kind of a generative close's conviction (the §24.1 dedup key's namespace).
pub const PALW_GEN_EVIDENCE_KIND_V1: u8 = 0x47;

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **A pipeline claim's execution root** (see the module doc).
pub fn palw_gen_execution_root_v1(
    job_id: &Hash64,
    class_id: &Hash64,
    step_leaf_count: u64,
    step_root: &Hash64,
    generated: &[u32],
) -> Hash64 {
    let mut ids = Vec::with_capacity(4 + generated.len() * 4);
    ids.extend_from_slice(&(generated.len() as u32).to_le_bytes());
    for id in generated {
        ids.extend_from_slice(&id.to_le_bytes());
    }
    keyed64(
        PALW_GEN_EXECUTION_ROOT_DOMAIN_V1,
        &[job_id.as_byte_slice(), class_id.as_byte_slice(), &step_leaf_count.to_le_bytes(), step_root.as_byte_slice(), &ids],
    )
}

/// **What pins a pipeline execution** — carried by every generative court move.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenStepBindingV1 {
    /// [`PALW_GEN_CLOSE_VERSION_V1`].
    pub version: u16,
    /// The job the claim ran.
    pub job: PalwFreePromptJobV5,
    /// Every stage's root, in stage order (the step root is over them).
    pub stage_roots: Vec<Hash64>,
    pub step_leaf_count: u64,
    /// The generated ids, as committed.
    pub generated: Vec<u32>,
    pub committed_execution_root: Hash64,
}

impl PalwGenStepBindingV1 {
    /// The binding of what a worker committed.
    pub fn of(job: &PalwFreePromptJobV5, roots: &PalwGenClaimRootsV1, step_leaf_count: u64) -> Self {
        let mut binding = Self {
            version: PALW_GEN_CLOSE_VERSION_V1,
            job: job.clone(),
            stage_roots: roots.stage_roots.clone(),
            step_leaf_count,
            generated: roots.generated.clone(),
            committed_execution_root: Hash64::default(),
        };
        binding.committed_execution_root = binding.execution_root();
        binding
    }

    /// The step root over the carried stage roots.
    pub fn step_root(&self) -> Hash64 {
        palw_gen_step_root_v1(&self.stage_roots)
    }

    /// The execution root its parts produce.
    pub fn execution_root(&self) -> Hash64 {
        palw_gen_execution_root_v1(
            &fp_job_id_v5(&self.job),
            &self.job.v4.class_id,
            self.step_leaf_count,
            &self.step_root(),
            &self.generated,
        )
    }

    /// The claim's roots as the court reads them.
    pub fn claim_roots(&self) -> PalwGenClaimRootsV1 {
        PalwGenClaimRootsV1 { step_root: self.step_root(), stage_roots: self.stage_roots.clone(), generated: self.generated.clone() }
    }
}

/// **An opened leaf on the wire**: its coordinate, its values as 4-byte lanes (PALW-TIR-5), and its
/// path to its stage's Merkle root.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenLeafOpeningV1 {
    pub coord: PalwGenLeafCoordV1,
    pub lanes_le: Vec<u8>,
    pub path: Vec<Hash64>,
}

fn leaf_dtype(space: &PalwGenStepSpaceV1, coord: &PalwGenLeafCoordV1) -> Option<misaka_palw_tir::types::DType> {
    let stage = space.stages.get(coord.stage as usize)?;
    let index = stage.leaf_index(coord)?;
    Some(stage.leaves()[index as usize].dtype)
}

impl PalwGenLeafOpeningV1 {
    /// An opened leaf as it rides: `None` when its coordinate is no leaf of `space` or a value is not
    /// one of the leaf's dtype.
    pub fn of(space: &PalwGenStepSpaceV1, leaf: &PalwGenOpenedLeafV1) -> Option<Self> {
        let dtype = leaf_dtype(space, &leaf.coord)?;
        let lanes_le = crate::palw_tir_step_v1::palw_tir_lanes_le_v1(dtype, &leaf.values).ok()?;
        Some(Self { coord: leaf.coord, lanes_le, path: leaf.path.clone() })
    }

    /// The opened leaf the court reads: `None` when its coordinate is no leaf of `space` or its lanes
    /// are not whole 4-byte lanes.
    pub fn opened(&self, space: &PalwGenStepSpaceV1) -> Option<PalwGenOpenedLeafV1> {
        let dtype = leaf_dtype(space, &self.coord)?;
        let values = crate::palw_tir_step_v1::palw_tir_lane_values_v1(dtype, &self.lanes_le).ok()?;
        Some(PalwGenOpenedLeafV1 { coord: self.coord, values, path: self.path.clone() })
    }
}

/// **A generative cone close** (`GenCone`, tag 10): one leaf of a pipeline claim — the one the
/// ladder narrowed to — and every unit its cone reads (see [`PalwGenCloseV1`]).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenConeCloseV1 {
    pub version: u16,
    pub binding: PalwGenStepBindingV1,
    /// The prompt ids, whole — carried exactly when the disputed stage reads them.
    pub prompt_ids: Vec<u32>,
    pub disputed: PalwGenLeafOpeningV1,
    pub operands: Vec<PalwGenLeafOpeningV1>,
    pub image_tiles: Vec<PalwGenImageTileV1>,
    pub params: Vec<PalwArtifactOpeningV1>,
}

/// **A generative decode close** (`GenDecodeToken`, tag 11): generated id `t` against the committed
/// logits row it was selected from — every tile of that row, under the text stage's root.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenDecodeCloseV1 {
    pub version: u16,
    pub binding: PalwGenStepBindingV1,
    pub t: u32,
    pub row: Vec<PalwGenLeafOpeningV1>,
}

/// Why a generative close adjudicates nothing (the close is refused; nobody is convicted).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwGenCloseErrorV1 {
    #[error("unsupported generative close version {0}")]
    Version(u16),
    #[error("the binding is not the claim's: {0}")]
    NotTheClaims(String),
    #[error("the binding does not verify: {0}")]
    Binding(String),
    #[error("the close opens leaf {opened}; the ladder narrowed to {narrowed}")]
    NotTheNarrowedLeaf { opened: u64, narrowed: u64 },
    #[error("a carried leaf is no leaf of the claim's tree: {0:?}")]
    NoSuchLeaf(PalwGenLeafCoordV1),
    #[error("the disputed stage reads the prompt, and the close does not carry it")]
    PromptNotCarried,
    #[error("the close carries a prompt the disputed stage does not read, or one that is not the job's")]
    PromptNotTheJobs,
    #[error(transparent)]
    Refused(PalwGenCloseRefusalV1),
}

/// What verifying a binding established: the class decoded, the step space of the job, the job's
/// facts, its images and its draw.
pub struct PalwGenVerifiedBindingV1 {
    pub pipeline: TirPipelineV1,
    pub programs: Vec<TirProgramV2>,
    pub space: PalwGenStepSpaceV1,
    pub facts: Vec<StageJobFacts>,
    pub images: Vec<PalwGenImageRefV1>,
    pub draw: PalwGenDrawV1,
    pub roots: PalwGenClaimRootsV1,
    pub inventory: crate::palw_gen_artifact_v1::PalwGenInventoryIndexV1,
    pub decode: PalwGenDecodeV1,
}

/// What a binding check decided.
pub enum PalwGenBindingOutcomeV1 {
    Verified(Box<PalwGenVerifiedBindingV1>),
    /// The executor's own binding convicts it: its leaf count is not the job's canonical count.
    Convicted(PalwStepFaultV1),
}

/// **Does stage `stage` read the prompt's ids?** The text stage (its stream starts with them), a
/// stage whose token run is the prompt's, and a stage a job-token binding feeds the prompt to.
pub fn palw_gen_stage_reads_prompt_v1(pipeline: &TirPipelineV1, stage: usize) -> bool {
    let Some(st) = pipeline.stages.get(stage) else { return false };
    matches!(st.trip, TripRule::TextStream)
        || st.tokens.as_ref().is_some_and(|r| r.source == TokenSource::Prompt)
        || st.bind.iter().any(|b| matches!(b, Binding::JobTokens { rule } if rule.source == TokenSource::Prompt))
}

/// **Verify a binding against the claim and the registry**: its version; its execution root the
/// claim's; its job the claim's class's (`palw_fp_v5_resolve_class_v1` against the row); its stage
/// roots the pipeline's count; the job's facts (with `prompt` where carried, zeros of the job's
/// prompt length elsewhere — see the module doc); the step space; and the leaf count, which convicts
/// when it is not canonical.
pub fn verify_gen_binding_v1(
    binding: &PalwGenStepBindingV1,
    row: &PalwGenClassRecordV1,
    claim_class_id: &Hash64,
    claim_execution_root: &Hash64,
    prompt: Option<&[u32]>,
) -> Result<PalwGenBindingOutcomeV1, PalwGenCloseErrorV1> {
    if binding.version != PALW_GEN_CLOSE_VERSION_V1 {
        return Err(PalwGenCloseErrorV1::Version(binding.version));
    }
    if binding.job.v4.class_id != *claim_class_id || row.class_id != *claim_class_id {
        return Err(PalwGenCloseErrorV1::NotTheClaims("the job names another class".into()));
    }
    if binding.committed_execution_root != *claim_execution_root || binding.execution_root() != *claim_execution_root {
        return Err(PalwGenCloseErrorV1::NotTheClaims("its parts do not produce the claim's execution root".into()));
    }
    crate::palw_fp_job_v5::palw_fp_v5_resolve_class_v1(&binding.job, Some(row), true)
        .map_err(|e| PalwGenCloseErrorV1::Binding(format!("the job is not the class's: {e}")))?;
    let (programs, pipeline) = row.class.decode().map_err(|e| PalwGenCloseErrorV1::Binding(e.to_string()))?;
    if binding.stage_roots.len() != pipeline.stages.len() {
        return Err(PalwGenCloseErrorV1::Binding("one root per stage".into()));
    }
    let prompt_len = binding.job.v4.prompt_tokens as usize;
    let prompt_ids = match prompt {
        Some(ids) if ids.len() == prompt_len => ids.to_vec(),
        Some(_) => return Err(PalwGenCloseErrorV1::PromptNotTheJobs),
        None => vec![0; prompt_len],
    };
    let job = PipelineJob { prompt: prompt_ids, generated: binding.generated.clone(), ..PipelineJob::default() };
    let facts = stage_job_facts(&pipeline, &programs, &job).map_err(|e| PalwGenCloseErrorV1::Binding(e.to_string()))?;
    let trips: Vec<u32> = facts.iter().map(|f| f.trip).collect();
    let space = PalwGenStepSpaceV1::new(&pipeline, &programs, &row.class.layouts, &trips, prompt_len as u32)
        .map_err(|e| PalwGenCloseErrorV1::Binding(e.to_string()))?;
    if space.leaf_count() != binding.step_leaf_count {
        return Ok(PalwGenBindingOutcomeV1::Convicted(PalwStepFaultV1::StepLeafCountNotCanonical));
    }
    let images =
        binding.job.images.iter().zip(&row.class.offers.images).map(|(image, slot)| PalwGenImageRefV1::of(image, slot)).collect();
    let inventory = crate::palw_gen_artifact_v1::PalwGenInventoryIndexV1::new(&programs)
        .ok_or_else(|| PalwGenCloseErrorV1::Binding("the class's params have no inventory".into()))?;
    let decode =
        PalwGenDecodeV1::of(&binding.job).ok_or_else(|| PalwGenCloseErrorV1::Binding("a V5 job decodes under V4's rules".into()))?;
    Ok(PalwGenBindingOutcomeV1::Verified(Box::new(PalwGenVerifiedBindingV1 {
        pipeline,
        programs,
        space,
        facts,
        images,
        draw: PalwGenDrawV1 { seed: binding.job.v4.sampling_seed, item_index: 0 },
        roots: binding.claim_roots(),
        inventory,
        decode,
    })))
}

/// **A generative close's verdict**, in the court's two outcomes: `Ok(Some(fault))` convicts,
/// `Ok(None)` acquits, `Err` adjudicates nothing.
pub type PalwGenCloseOutcomeV1 = Result<Option<PalwStepFaultV1>, PalwGenCloseErrorV1>;

/// The evidence id of a generative conviction (the §24.1 dedup key).
pub fn palw_gen_evidence_id_v1(binding: &PalwGenStepBindingV1, leaf_index: u64, fault: PalwStepFaultV1) -> Hash64 {
    crate::palw_step_leg::step_refutation_evidence_id(&binding.committed_execution_root, PALW_GEN_EVIDENCE_KIND_V1, leaf_index, fault)
}

/// **Check a generative cone close** against the claim (its class's row, its class id and execution
/// root) and, in a session, the leaf the ladder narrowed to.
pub fn check_gen_cone_close_v1(
    close: &PalwGenConeCloseV1,
    row: &PalwGenClassRecordV1,
    claim_class_id: &Hash64,
    claim_execution_root: &Hash64,
    narrowed: Option<u64>,
    prompt_form: PalwPromptIdsFormV1,
    limits: &DemandLimits,
) -> PalwGenCloseOutcomeV1 {
    let (v, gen_close) = match open_cone_close_v1(close, row, claim_class_id, claim_execution_root, narrowed, prompt_form)? {
        Ok(opened) => opened,
        Err(fault) => return Ok(Some(fault)),
    };
    match palw_gen_adjudicate_leaf_v1(&v.case(row), &gen_close, limits).map_err(PalwGenCloseErrorV1::Refused)? {
        PalwGenVerdictV1::Acquitted => Ok(None),
        PalwGenVerdictV1::Convicted { fault, .. } => Ok(Some(fault)),
    }
}

impl PalwGenVerifiedBindingV1 {
    /// The court's case over this verified binding and the class's row.
    pub fn case(&self, row: &PalwGenClassRecordV1) -> PalwGenCourtCaseV1<'_> {
        PalwGenCourtCaseV1 {
            space: &self.space,
            pipeline: &self.pipeline,
            programs: &self.programs,
            artifact_root: row.artifact_root,
            inventory: &self.inventory,
            facts: &self.facts,
            images: &self.images,
            draw: self.draw,
            claim: &self.roots,
        }
    }
}

/// A cone close opened: its binding verified and its leaves read under the space — or the
/// binding's own conviction.
type OpenedConeV1 = Result<(Box<PalwGenVerifiedBindingV1>, PalwGenCloseV1), PalwStepFaultV1>;

/// **Open a cone close's carriage** (every cone-shaped move: a cone close, a root claim's finalize, a
/// dissection's bottom): its version, the prompt's carriage rule (see the module doc), the binding
/// verified against the claim and the registry, every leaf read under the space, and — in a session
/// — the disputed leaf the one the ladder narrowed to.
fn open_cone_close_v1(
    close: &PalwGenConeCloseV1,
    row: &PalwGenClassRecordV1,
    claim_class_id: &Hash64,
    claim_execution_root: &Hash64,
    narrowed: Option<u64>,
    prompt_form: PalwPromptIdsFormV1,
) -> Result<OpenedConeV1, PalwGenCloseErrorV1> {
    if close.version != PALW_GEN_CLOSE_VERSION_V1 {
        return Err(PalwGenCloseErrorV1::Version(close.version));
    }
    // The prompt: carried whole exactly when the disputed stage reads it, and then the job's.
    let (_, pipeline) = row.class.decode().map_err(|e| PalwGenCloseErrorV1::Binding(e.to_string()))?;
    let reads = palw_gen_stage_reads_prompt_v1(&pipeline, close.disputed.coord.stage as usize);
    let prompt = match (reads, close.prompt_ids.is_empty()) {
        (true, true) => return Err(PalwGenCloseErrorV1::PromptNotCarried),
        (false, false) => return Err(PalwGenCloseErrorV1::PromptNotTheJobs),
        (true, false) => {
            if !crate::palw_prompt_ids_v1::prompt_token_ids_match_v1(
                prompt_form,
                &close.prompt_ids,
                &close.binding.job.v4.prompt_token_ids_hash,
            ) {
                return Err(PalwGenCloseErrorV1::PromptNotTheJobs);
            }
            Some(close.prompt_ids.as_slice())
        }
        (false, true) => None,
    };
    let v = match verify_gen_binding_v1(&close.binding, row, claim_class_id, claim_execution_root, prompt)? {
        PalwGenBindingOutcomeV1::Verified(v) => v,
        PalwGenBindingOutcomeV1::Convicted(fault) => return Ok(Err(fault)),
    };
    let opened = |o: &PalwGenLeafOpeningV1| o.opened(&v.space).ok_or(PalwGenCloseErrorV1::NoSuchLeaf(o.coord));
    let disputed = opened(&close.disputed)?;
    let index = v.space.global_index(&disputed.coord).ok_or(PalwGenCloseErrorV1::NoSuchLeaf(disputed.coord))?;
    if let Some(narrowed) = narrowed
        && index != narrowed
    {
        return Err(PalwGenCloseErrorV1::NotTheNarrowedLeaf { opened: index, narrowed });
    }
    let gen_close = PalwGenCloseV1 {
        disputed,
        operands: close.operands.iter().map(opened).collect::<Result<_, _>>()?,
        image_tiles: close.image_tiles.clone(),
        params: close.params.clone(),
    };
    Ok(Ok((v, gen_close)))
}

// ---------------------------------------------------------------------------------------------
// RFC-0002 F7, composed: the generative root claim and the dissection's bottom
// ---------------------------------------------------------------------------------------------

/// Key of [`palw_gen_root_claim_message_v1`].
pub const PALW_GEN_DISSECT_DOMAIN_ROOT_V1: &[u8] = b"misaka-palw/gen/dissect/root/v1";

/// **The responder's root claim at a dissected leaf of a pipeline claim** (the terminal move there;
/// F7's `PalwTirRootClaimV1` with the generative close as its finalize's carriage): for each
/// reduction over `H` of the leaf's cone, the elements the finalize demands and their totals over
/// `[0, H)`. Opens F7's phase verbatim (`PalwTirDissectPhaseV1::open_parts`); the rounds and the
/// choices are F7's objects (`CourtTirDissected`, `CourtTirChildChosen`).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenRootClaimV1 {
    /// `PALW_TIR_DISSECT_OBJECT_VERSION_V1` (the phase's).
    pub version: u16,
    pub elements: Vec<Vec<u32>>,
    pub totals: crate::palw_tir_dissect_v1::PalwTirRangeClaimV1,
    /// The dissected leaf and every unit the finalize reads, in a cone close's form.
    pub finalize: Box<PalwGenConeCloseV1>,
}

/// **What the responder signs** for a root claim: the session and the claim, under their own key.
pub fn palw_gen_root_claim_message_v1(session_id: &Hash64, root: &PalwGenRootClaimV1) -> Vec<u8> {
    let bytes = borsh::to_vec(root).expect("a root claim is borsh-serializable");
    keyed64(PALW_GEN_DISSECT_DOMAIN_ROOT_V1, &[session_id.as_byte_slice(), &bytes]).as_byte_slice().to_vec()
}

/// **Admit a generative root claim** (the acceptance layer's, which holds the court's limits): its
/// finalize's carriage opens as a cone close's does at the narrowed leaf, without a conviction (a
/// binding that convicts is closed, not dissected), and F7's check over the stage
/// ([`crate::palw_gen_court_v1::palw_gen_check_root_claim_v1`]): the leaf dissected, the claim the
/// site's, the finalize the committed leaf reading exactly the claimed values. Returns the site.
pub fn check_gen_root_claim_v1(
    root: &PalwGenRootClaimV1,
    row: &PalwGenClassRecordV1,
    claim_class_id: &Hash64,
    claim_execution_root: &Hash64,
    narrowed: u64,
    prompt_form: PalwPromptIdsFormV1,
    limits: &DemandLimits,
) -> Result<crate::palw_tir_dissect_v1::PalwTirDissectSiteV1, String> {
    if root.version != crate::palw_tir_dissect_v1::PALW_TIR_DISSECT_OBJECT_VERSION_V1 {
        return Err(format!("root claim version {} is not the phase's", root.version));
    }
    let (v, finalize) =
        match open_cone_close_v1(&root.finalize, row, claim_class_id, claim_execution_root, Some(narrowed), prompt_form)
            .map_err(|e| e.to_string())?
        {
            Ok(opened) => opened,
            Err(fault) => return Err(format!("the binding convicts on its own ({fault:?}): it is closed, not dissected")),
        };
    crate::palw_gen_court_v1::palw_gen_check_root_claim_v1(&v.case(row), &finalize, &root.elements, &root.totals, limits)
}

/// **The site a root claim opens on, as the fold derives it**: the binding verified against the
/// claim and the registry, the finalize's leaf the one the ladder narrowed to, and that leaf's site
/// from the class's program — no evaluation (the finalize is the acceptance layer's).
pub fn palw_gen_root_claim_site_v1(
    root: &PalwGenRootClaimV1,
    row: &PalwGenClassRecordV1,
    claim_class_id: &Hash64,
    claim_execution_root: &Hash64,
    narrowed: u64,
) -> Result<crate::palw_tir_dissect_v1::PalwTirDissectSiteV1, String> {
    let binding = &root.finalize.binding;
    // The site needs the job's trips, never its prompt's values: zeros of its length stand in.
    let v = match verify_gen_binding_v1(binding, row, claim_class_id, claim_execution_root, None).map_err(|e| e.to_string())? {
        PalwGenBindingOutcomeV1::Verified(v) => v,
        PalwGenBindingOutcomeV1::Convicted(fault) => return Err(format!("the binding convicts on its own ({fault:?})")),
    };
    let coord = root.finalize.disputed.coord;
    let index = v.space.global_index(&coord).ok_or_else(|| format!("{coord:?} is not a leaf of this execution"))?;
    if index != narrowed {
        return Err(format!("the root claim opens leaf {index}, the ladder narrowed to {narrowed}"));
    }
    crate::palw_gen_court_v1::palw_gen_dissect_site_v1(&v.space.stages[coord.stage as usize], &coord)
        .ok_or_else(|| "the narrowed leaf is not dissected".to_string())
}

/// **Grade a generative dissection's bottom** (`PalwCourtVerdictProofV2::GenDissection`) against its
/// phase: the carriage opens as a cone close's does at the phase's leaf (the ladder's), its
/// convictions stand, and F7's bottom over the stage decides
/// ([`crate::palw_gen_court_v1::palw_gen_check_dissect_bottom_v1`]).
#[allow(clippy::too_many_arguments)]
pub fn check_gen_dissect_bottom_v1(
    phase: &crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1,
    bottom: &PalwGenConeCloseV1,
    row: &PalwGenClassRecordV1,
    claim_class_id: &Hash64,
    claim_execution_root: &Hash64,
    narrowed: u64,
    prompt_form: PalwPromptIdsFormV1,
    limits: &DemandLimits,
) -> PalwGenCloseOutcomeV1 {
    if narrowed != phase.leaf_index() {
        return Err(PalwGenCloseErrorV1::NotTheNarrowedLeaf { opened: phase.leaf_index(), narrowed });
    }
    let (v, close) = match open_cone_close_v1(bottom, row, claim_class_id, claim_execution_root, Some(narrowed), prompt_form)? {
        Ok(opened) => opened,
        Err(fault) => return Ok(Some(fault)),
    };
    match crate::palw_gen_court_v1::palw_gen_check_dissect_bottom_v1(&v.case(row), &close, phase, limits)
        .map_err(PalwGenCloseErrorV1::Refused)?
    {
        PalwGenVerdictV1::Acquitted => Ok(None),
        PalwGenVerdictV1::Convicted { fault, .. } => Ok(Some(fault)),
    }
}

/// **Check a generative decode close**: generated id `t` against its committed logits row. In a
/// session the narrowed leaf must be a tile of that row.
pub fn check_gen_decode_close_v1(
    close: &PalwGenDecodeCloseV1,
    row: &PalwGenClassRecordV1,
    claim_class_id: &Hash64,
    claim_execution_root: &Hash64,
    narrowed: Option<u64>,
) -> PalwGenCloseOutcomeV1 {
    if close.version != PALW_GEN_CLOSE_VERSION_V1 {
        return Err(PalwGenCloseErrorV1::Version(close.version));
    }
    let v = match verify_gen_binding_v1(&close.binding, row, claim_class_id, claim_execution_root, None)? {
        PalwGenBindingOutcomeV1::Verified(v) => v,
        PalwGenBindingOutcomeV1::Convicted(fault) => return Ok(Some(fault)),
    };
    let prompt_len = close.binding.job.v4.prompt_tokens;
    if let Some(narrowed) = narrowed {
        let (stage, local) = v.space.locate(narrowed).ok_or(PalwGenCloseErrorV1::NotTheNarrowedLeaf { opened: u64::MAX, narrowed })?;
        let leaf = v.space.stages[stage as usize].leaves()[local as usize];
        let out = v.pipeline.output_stage;
        let program = &v.programs[v.pipeline.stages[out as usize].program as usize];
        let post = (program.occurrences().len() - 1) as u16;
        let row_kind = PalwGenLeafKindV1::Commit { occurrence: post, node: program.output.node() };
        let at = prompt_len.saturating_sub(1) + close.t;
        if stage != out || leaf.coord.kind != row_kind || leaf.coord.pos != at {
            return Err(PalwGenCloseErrorV1::NotTheNarrowedLeaf { opened: u64::from(close.t), narrowed });
        }
    }
    let row_leaves =
        close.row.iter().map(|o| o.opened(&v.space).ok_or(PalwGenCloseErrorV1::NoSuchLeaf(o.coord))).collect::<Result<Vec<_>, _>>()?;
    match palw_gen_decode_door_v1(&v.case(row), &v.decode, prompt_len, close.t, &row_leaves).map_err(PalwGenCloseErrorV1::Refused)? {
        PalwGenVerdictV1::Acquitted => Ok(None),
        PalwGenVerdictV1::Convicted { fault, .. } => Ok(Some(fault)),
    }
}
