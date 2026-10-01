//! **RFC-0004 §7.2 (A6): the evaluation court** — what pins an evaluation claim's execution for a court (its
//! binding), and the closes `PalwCourtVerdictProofV2` carries for one: `EvalCone` (tag 13), `EvalDecodeToken`
//! (tag 14) and `EvalDissection` (tag 15), dormant under `palw_improvement_v1` (spec 17 §17.8.6).
//!
//! An evaluation claim is an RFC-0003 *pipeline claim* whose pipeline the chain derives (the subject's program
//! as stage 0, the scoring library's stages after it), so the court that judges it is the generative court's —
//! its leaf adjudication, decode door and F7 dissection, over the evaluation's own context
//! ([`palw_improve_eval_context_v1`]) instead of a registered pipeline class's row. What is new is the binding to
//! the claim, which is the chain's rather than a registry's:
//!
//! ```text
//! the claim        = the FP commitment the fold accepted (its execution_root, trace_root = the step root,
//!                    output_root = the generated ids' root, work_leaves = the step leaf count)
//! the job          = the job table's row for the claim (`improvement_eval_jobs`): line, epoch, item, subject, kind,
//!                    part, mode — what the chain derived the pipeline from
//! the class        = the class the claim ran: its canonical program (`tir_classes`), its layout digest and its
//!                    registered `artifact_root`
//! the binding      = `PalwEvalBindingV1` (A6): the job, the class, the layout, every stage's root, the leaf count, the
//!                    prompt's root and length, the stage parameters, the generated ids, what a judged part read and
//!                    the committed score — hashing to the claim's `execution_root`
//! ```
//!
//! [`verify_eval_binding_v1`] holds all of it to the claim and the chain before a close reads anything else: the
//! binding's job is the job table's, its parts produce the claim's roots, its layout is the one the class id
//! binds, and the context it derives has the leaf count the claim committed (the fold checked that at acceptance;
//! the court re-derives it from the binding, never from the fold's word).
//!
//! **What an executor can lie about.** The fold verified at acceptance everything it can see: the job against the
//! item, the subject and the policy, the prompt against the item's, a teacher-forced job's ids against the
//! reference, the roots against the tail, the leaf count against the closed form. What it cannot see is the
//! execution itself — the step tree's leaves, the decode's selections, the score — and those are the court's:
//!
//! * **`EvalCone`** — one leaf of the step tree, the one the accusation names, adjudicated by demand evaluation of
//!   its cone from the leaves before it, the subject's proven parameters, the job's facts and `R` recomputed
//!   ([`check_eval_cone_close_v1`]). A planted wrong value in any stage — the subject's logits, the log-probs, the
//!   score's sum — convicts here.
//! * **`EvalDecodeToken`** — the claim's COMMITTED OUTPUTS against what they were read from
//!   ([`check_eval_decode_close_v1`]): a generated id against the committed logits row it was selected from (a
//!   generating job — a wrong decode), or the committed score lanes against the score stage's committed output
//!   tile (a scoring job — a wrong score at an item, with an honest tree under it).
//! * **`EvalDissection`** — the bottom of a history dissection (F7) at a leaf whose cone reduces over the history,
//!   graded against its session's phase ([`check_eval_dissect_bottom_v1`]); the responder's root claim is
//!   [`PalwEvalRootClaimV1`].
//!
//! A wrong *binding* to the item or the subject is not a court's matter: the fold refused it at acceptance, and the
//! binding a close carries must be the one the job table and the claim already fix (above).
//!
//! **A subject's parameters** ride per leaf with their paths ([`PalwEvalParamsV1`]): under the class's
//! `artifact_root` for a class committing one inventory, and for a composite candidate (RFC-0004 §6.3) the
//! reference (it must hash to the class's artifact root) and the leaves of each section under its own sub-root.

use crate::Hash64;
use crate::palw_artifact::PalwArtifactOpeningV1;
use crate::palw_gen_artifact_v1::{PalwGenInventoryIndexV1, PalwGenInventoryNamingV1, PalwGenOpenedParamsV1};
use crate::palw_gen_close_v1::PalwGenLeafOpeningV1;
use crate::palw_gen_court_v1::{PalwGenCloseRefusalV1, PalwGenCloseV1, PalwGenCourtCaseV1, PalwGenDrawV1};
use crate::palw_gen_step_v1::{PalwGenLeafCoordV1, PalwGenLeafKindV1, PalwGenStepSpaceV1};
use crate::palw_gen_worker_v1::PalwGenClaimRootsV1;
use crate::palw_improve_composite_v1::PalwTirCompositeRefV1;
use crate::palw_improve_eval_v1::{
    PalwEvalBindingV1, PalwEvalContextV1, PalwEvalJobV1, PalwEvalSubjectClassV1, palw_improve_eval_context_v1,
    palw_improve_eval_layout_digest_v1, palw_improve_eval_pipeline_job_v1, palw_improve_eval_prompt_root_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::pipeline::{StageJobFacts, stage_job_facts};

/// Wire version of every object in this module.
pub const PALW_IMPROVE_EVAL_COURT_VERSION_V1: u16 = 1;
/// The evidence kind of an evaluation close's conviction (the §24.1 dedup key's namespace): the generative
/// court's is `0x47`, the IR court's are 0–7.
pub const PALW_IMPROVE_EVAL_EVIDENCE_KIND_V1: u8 = 0x49;
/// Key of [`palw_eval_root_claim_message_v1`].
pub const PALW_IMPROVE_EVAL_DISSECT_DOMAIN_ROOT_V1: &[u8] = b"misaka-palw/improve/eval-dissect/root/v1";

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
// The objects
// ---------------------------------------------------------------------------------------------

/// **How an evaluation close carries the subject's parameter openings**: whole inventory leaves of the subject's
/// program, ascending, each with its path ([`PalwArtifactOpeningV1`]) — under the class's `artifact_root`
/// (`Single`), or, for a composite candidate, under the section's own root (`Composite`: the reference rides with
/// the openings, and hashes to the class's artifact root, so the court believes nothing and needs no registry).
/// In `Composite`, the parent section's openings (params `0..p`) are at the leaves' own indices under
/// `parent_root` and the adapter section's (params `p..`) are rebased by the split — a leaf `v` at or past it
/// opens at `v − split` of a tree of `leaf_count − split` leaves under `adapter_root`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwEvalParamsV1 {
    Single(Vec<PalwArtifactOpeningV1>),
    Composite { artifact: PalwTirCompositeRefV1, parent: Vec<PalwArtifactOpeningV1>, adapter: Vec<PalwArtifactOpeningV1> },
}

impl PalwEvalParamsV1 {
    /// The openings carried, in carriage order: every leaf the close opens.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Single(openings) => openings.is_empty(),
            Self::Composite { parent, adapter, .. } => parent.is_empty() && adapter.is_empty(),
        }
    }
}

/// **An evaluation cone close** (`EvalCone`, tag 13): one leaf of the claim's step tree — the one the accusation
/// narrowed to — and every unit its cone reads (see [`PalwGenCloseV1`]).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalConeCloseV1 {
    /// [`PALW_IMPROVE_EVAL_COURT_VERSION_V1`].
    pub version: u16,
    pub binding: PalwEvalBindingV1,
    /// The prompt ids, whole — carried exactly when the disputed stage reads them, and then the binding's
    /// (`palw_improve_eval_prompt_root_v1`); a dispute anywhere else reveals nothing of the prompt.
    pub prompt_ids: Vec<u32>,
    pub disputed: PalwGenLeafOpeningV1,
    pub operands: Vec<PalwGenLeafOpeningV1>,
    pub params: PalwEvalParamsV1,
}

/// **What an evaluation decode close opens**: the claim's committed outputs against what they were read from.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwEvalOutputCloseV1 {
    /// A generated id `t` against the committed logits row it was selected from — every tile of that row, under
    /// the stream stage's root (a generating job: its greedy decode).
    Token { t: u32, row: Vec<PalwGenLeafOpeningV1> },
    /// The committed score lanes against the score stage's committed output tile(s) — every tile of the output
    /// node at the last position of the output stage (a RefLogLik job, or a judged part).
    Score { tiles: Vec<PalwGenLeafOpeningV1> },
}

/// **An evaluation decode close** (`EvalDecodeToken`, tag 14).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalDecodeCloseV1 {
    /// [`PALW_IMPROVE_EVAL_COURT_VERSION_V1`].
    pub version: u16,
    pub binding: PalwEvalBindingV1,
    pub output: PalwEvalOutputCloseV1,
}

/// **The responder's root claim at a dissected leaf of an evaluation claim** (the terminal move there; F7's root
/// claim with the evaluation cone close as its finalize's carriage): for each reduction over `H` of the leaf's
/// cone, the elements the finalize demands and their totals over `[0, H)`. Opens F7's phase verbatim
/// (`PalwTirDissectPhaseV1::open_parts`); the rounds and the choices are F7's objects (`CourtTirDissected`,
/// `CourtTirChildChosen`). Carried by `PalwConsensusObjectV2::CourtEvalRootClaimed`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEvalRootClaimV1 {
    /// `PALW_TIR_DISSECT_OBJECT_VERSION_V1` (the phase's).
    pub version: u16,
    pub elements: Vec<Vec<u32>>,
    pub totals: crate::palw_tir_dissect_v1::PalwTirRangeClaimV1,
    /// The dissected leaf and every unit the finalize reads, in a cone close's form.
    pub finalize: Box<PalwEvalConeCloseV1>,
}

/// **What the responder signs** for a root claim: the session and the claim, under their own key.
pub fn palw_eval_root_claim_message_v1(session_id: &Hash64, root: &PalwEvalRootClaimV1) -> Vec<u8> {
    let bytes = borsh::to_vec(root).expect("a root claim is borsh-serializable");
    keyed64(PALW_IMPROVE_EVAL_DISSECT_DOMAIN_ROOT_V1, &[session_id.as_byte_slice(), &bytes]).as_byte_slice().to_vec()
}

// ---------------------------------------------------------------------------------------------
// What the chain holds, and the binding verified against it
// ---------------------------------------------------------------------------------------------

/// **What the chain holds of an evaluation claim and its class** — the fold hands it to the court's pure checks.
#[derive(Clone, Copy, Debug)]
pub struct PalwEvalClaimFactsV1<'a> {
    /// The claim's class, and its committed roots and leaf count, as the claim state holds them.
    pub class_id: &'a Hash64,
    pub execution_root: &'a Hash64,
    pub trace_root: &'a Hash64,
    pub output_root: &'a Hash64,
    pub work_leaves: u64,
    /// The job the chain's job table gives the claim (`improvement_eval_job_of_claim`).
    pub job: &'a PalwEvalJobV1,
    /// The class's canonical program bytes (`tir_classes`), the digest its id binds its layout to, and its
    /// registered artifact root (`classes`).
    pub program: &'a [u8],
    pub layout_digest: Hash64,
    pub artifact_root: Hash64,
}

/// Why an evaluation close adjudicates nothing (the close is refused; nobody is convicted).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwEvalCourtErrorV1 {
    #[error("unsupported evaluation court version {0}")]
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
    #[error("the close carries a prompt the disputed stage does not read, or one that is not the binding's")]
    PromptNotTheBindings,
    #[error("the subject's parameter openings: {0}")]
    Params(String),
    #[error("the close is of another kind than the job: {0}")]
    NotThisJobsClose(&'static str),
    #[error(transparent)]
    Refused(PalwGenCloseRefusalV1),
}

/// What verifying a binding established: the context the chain derived, the step space of the job, the job's
/// facts, the pipeline inventory and the claim's roots as the court reads them.
pub struct PalwEvalVerifiedBindingV1 {
    pub ctx: PalwEvalContextV1,
    pub space: PalwGenStepSpaceV1,
    pub facts: Vec<StageJobFacts>,
    pub inventory: PalwGenInventoryIndexV1,
    pub roots: PalwGenClaimRootsV1,
    pub artifact_root: Hash64,
}

impl PalwEvalVerifiedBindingV1 {
    /// The generative court's case over this verified binding: the evaluation pipeline, its inventory (the
    /// subject's own, `PalwGenInventoryNamingV1::Evaluation`), no images, and `R`'s seed.
    pub fn case(&self) -> PalwGenCourtCaseV1<'_> {
        PalwGenCourtCaseV1 {
            space: &self.space,
            pipeline: &self.ctx.pipeline,
            programs: &self.ctx.programs,
            artifact_root: self.artifact_root,
            inventory: &self.inventory,
            facts: &self.facts,
            images: &[],
            draw: PalwGenDrawV1 { seed: self.ctx.seed, item_index: 0 },
            claim: &self.roots,
        }
    }
}

/// **The context a binding derives**, once held to the claim and the chain: its version; its job the job table's; its
/// class the claim's; its parts producing the claim's execution root, step root and output root and its leaf count
/// the claim's; its layout the one the class's id binds; the context the chain derives from them
/// ([`palw_improve_eval_context_v1`]); and its stage roots the pipeline's count. Cheap (no step space): a close
/// decides from the pipeline which units it must carry.
pub fn derive_eval_context_v1(
    binding: &PalwEvalBindingV1,
    facts: &PalwEvalClaimFactsV1<'_>,
) -> Result<PalwEvalContextV1, PalwEvalCourtErrorV1> {
    let not_the_claims = |why: &str| PalwEvalCourtErrorV1::NotTheClaims(why.into());
    if binding.version != crate::palw_improve_eval_v1::PALW_IMPROVE_EVAL_BINDING_VERSION_V1 {
        return Err(PalwEvalCourtErrorV1::Version(binding.version));
    }
    if binding.job != *facts.job {
        return Err(not_the_claims("its job is not the job table's for the claim"));
    }
    if binding.subject_class != *facts.class_id {
        return Err(not_the_claims("it names another class"));
    }
    if binding.committed_execution_root != *facts.execution_root || binding.execution_root() != *facts.execution_root {
        return Err(not_the_claims("its parts do not produce the claim's execution root"));
    }
    if binding.step_root() != *facts.trace_root {
        return Err(not_the_claims("its stage roots do not produce the claim's step root"));
    }
    if binding.generated_root() != *facts.output_root {
        return Err(not_the_claims("its ids do not hash to the claim's output root"));
    }
    if binding.step_leaf_count != facts.work_leaves {
        return Err(not_the_claims("its leaf count is not the claim's"));
    }
    if palw_improve_eval_layout_digest_v1(&binding.subject_layout) != facts.layout_digest {
        return Err(not_the_claims("its layout is not the one the class's id binds"));
    }
    let program = misaka_palw_tir::TirProgramV1::decode_canonical(facts.program)
        .map_err(|e| PalwEvalCourtErrorV1::Binding(format!("the class's program does not decode: {e}")))?;
    let subject = PalwEvalSubjectClassV1 {
        class_id: *facts.class_id,
        artifact_root: facts.artifact_root,
        program: &program,
        layout: &binding.subject_layout,
    };
    let ctx = palw_improve_eval_context_v1(&binding.job, &subject, binding.params)
        .map_err(|e| PalwEvalCourtErrorV1::Binding(format!("the job's context does not derive: {e}")))?;
    if binding.stage_roots.len() != ctx.pipeline.stages.len() {
        return Err(PalwEvalCourtErrorV1::Binding("one root per stage".into()));
    }
    Ok(ctx)
}

/// **Verify a binding against the claim and the chain** ([`derive_eval_context_v1`]), then the job's facts (with
/// the prompt where carried — checked against the binding's prompt root and length — and zeros of its length
/// elsewhere: the job's trips and positions depend on the length alone) and the step space, whose leaf count must be
/// the claim's.
pub fn verify_eval_binding_v1(
    binding: &PalwEvalBindingV1,
    facts: &PalwEvalClaimFactsV1<'_>,
    prompt: Option<&[u32]>,
) -> Result<PalwEvalVerifiedBindingV1, PalwEvalCourtErrorV1> {
    let ctx = derive_eval_context_v1(binding, facts)?;
    let prompt_ids = match prompt {
        Some(ids) if ids.len() == binding.prompt_tokens as usize && palw_improve_eval_prompt_root_v1(ids) == binding.prompt_root => {
            ids.to_vec()
        }
        Some(_) => return Err(PalwEvalCourtErrorV1::PromptNotTheBindings),
        None => vec![0; binding.prompt_tokens as usize],
    };
    let job = palw_improve_eval_pipeline_job_v1(&ctx, &prompt_ids, &binding.generated, &binding.finalized);
    let job_facts = stage_job_facts(&ctx.pipeline, &ctx.programs, &job).map_err(|e| PalwEvalCourtErrorV1::Binding(e.to_string()))?;
    let trips: Vec<u32> = job_facts.iter().map(|f| f.trip).collect();
    let space = PalwGenStepSpaceV1::new(&ctx.pipeline, &ctx.programs, &ctx.layouts, &trips, binding.prompt_tokens)
        .map_err(|e| PalwEvalCourtErrorV1::Binding(e.to_string()))?;
    if space.leaf_count() != binding.step_leaf_count {
        return Err(PalwEvalCourtErrorV1::Binding("the job's canonical leaf count is not the binding's".into()));
    }
    let inventory = PalwGenInventoryIndexV1::new_named(&ctx.programs, PalwGenInventoryNamingV1::Evaluation)
        .ok_or_else(|| PalwEvalCourtErrorV1::Binding("the class's params have no inventory".into()))?;
    let roots = PalwGenClaimRootsV1 {
        step_root: binding.step_root(),
        stage_roots: binding.stage_roots.clone(),
        generated: binding.generated.clone(),
        // An evaluation claim is a text claim: its output is its generated ids (RFC-0003 §I.3.2).
        output_root: None,
    };
    Ok(PalwEvalVerifiedBindingV1 { ctx, space, facts: job_facts, inventory, roots, artifact_root: facts.artifact_root })
}

// ---------------------------------------------------------------------------------------------
// The subject's parameters
// ---------------------------------------------------------------------------------------------

/// **Authenticate a close's parameter openings** against the claim's class: whole inventory leaves of the subject's
/// program, ascending, each its leaf's canonical piece and reaching its root — the class's `artifact_root`
/// (`Single`), or, for a composite candidate, the section's sub-root, the reference having hashed to the class's
/// artifact root first.
pub fn authenticate_eval_params_v1(
    params: &PalwEvalParamsV1,
    v: &PalwEvalVerifiedBindingV1,
) -> Result<PalwGenOpenedParamsV1, PalwEvalCourtErrorV1> {
    let refused = |e: crate::palw_gen_artifact_v1::PalwGenParamRefusalV1| PalwEvalCourtErrorV1::Params(e.to_string());
    match params {
        PalwEvalParamsV1::Single(openings) => {
            PalwGenOpenedParamsV1::authenticate(&v.inventory, &v.artifact_root, openings).map_err(refused)
        }
        PalwEvalParamsV1::Composite { artifact, parent, adapter } => {
            if artifact.artifact_root() != v.artifact_root {
                return Err(PalwEvalCourtErrorV1::Params("the composite reference is not of the class's artifact".into()));
            }
            let p =
                u16::try_from(artifact.p).map_err(|_| PalwEvalCourtErrorV1::Params("a composite split past every param".into()))?;
            let total = v.inventory.leaf_count();
            let split = v
                .inventory
                .leaves_before_param_v1(0, p)
                .filter(|split| *split > 0 && *split < total)
                .ok_or_else(|| PalwEvalCourtErrorV1::Params("a composite split past every param".into()))?;
            PalwGenOpenedParamsV1::authenticate_sections(
                &v.inventory,
                &[
                    (0, split, artifact.parent_root, parent.as_slice()),
                    (split, total - split, artifact.adapter_root, adapter.as_slice()),
                ],
            )
            .map_err(refused)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The closes
// ---------------------------------------------------------------------------------------------

/// **An evaluation close's verdict**, in the court's two outcomes: `Ok(Some(fault))` convicts, `Ok(None)` acquits,
/// `Err` adjudicates nothing.
pub type PalwEvalCloseOutcomeV1 = Result<Option<crate::palw_step_leg::PalwStepFaultV1>, PalwEvalCourtErrorV1>;

/// The evidence id of an evaluation conviction (the §24.1 dedup key).
pub fn palw_eval_evidence_id_v1(binding: &PalwEvalBindingV1, leaf_index: u64, fault: crate::palw_step_leg::PalwStepFaultV1) -> Hash64 {
    crate::palw_step_leg::step_refutation_evidence_id(
        &binding.committed_execution_root,
        PALW_IMPROVE_EVAL_EVIDENCE_KIND_V1,
        leaf_index,
        fault,
    )
}

/// **A cone close opened**: the binding verified against the claim and the chain, the prompt carried exactly when
/// the disputed stage reads it, every leaf read under the space, in a session the disputed leaf the one the ladder
/// narrowed to, and the parameters authenticated.
fn open_eval_cone_close_v1(
    close: &PalwEvalConeCloseV1,
    facts: &PalwEvalClaimFactsV1<'_>,
    narrowed: Option<u64>,
) -> Result<(PalwEvalVerifiedBindingV1, PalwGenCloseV1, PalwGenOpenedParamsV1), PalwEvalCourtErrorV1> {
    if close.version != PALW_IMPROVE_EVAL_COURT_VERSION_V1 {
        return Err(PalwEvalCourtErrorV1::Version(close.version));
    }
    // The prompt: carried whole exactly when the disputed stage reads it, and then the binding's.
    let ctx = derive_eval_context_v1(&close.binding, facts)?;
    let reads = crate::palw_gen_close_v1::palw_gen_stage_reads_prompt_v1(&ctx.pipeline, close.disputed.coord.stage as usize);
    let prompt = match (reads, close.prompt_ids.is_empty()) {
        (true, true) => return Err(PalwEvalCourtErrorV1::PromptNotCarried),
        (false, false) => return Err(PalwEvalCourtErrorV1::PromptNotTheBindings),
        (true, false) => Some(close.prompt_ids.as_slice()),
        (false, true) => None,
    };
    let v = verify_eval_binding_v1(&close.binding, facts, prompt)?;
    let opened = |o: &PalwGenLeafOpeningV1| o.opened(&v.space).ok_or(PalwEvalCourtErrorV1::NoSuchLeaf(o.coord));
    let disputed = opened(&close.disputed)?;
    let index = v.space.global_index(&disputed.coord).ok_or(PalwEvalCourtErrorV1::NoSuchLeaf(disputed.coord))?;
    if let Some(narrowed) = narrowed
        && index != narrowed
    {
        return Err(PalwEvalCourtErrorV1::NotTheNarrowedLeaf { opened: index, narrowed });
    }
    let params = authenticate_eval_params_v1(&close.params, &v)?;
    let gen_close = PalwGenCloseV1 {
        disputed,
        operands: close.operands.iter().map(opened).collect::<Result<_, _>>()?,
        image_tiles: Vec::new(),
        params: Vec::new(),
    };
    Ok((v, gen_close, params))
}

/// **Check an evaluation cone close** (`EvalCone`) against the claim and the chain and, in a session, the leaf the
/// ladder narrowed to: the generative court's leaf adjudication over the evaluation's own pipeline.
pub fn check_eval_cone_close_v1(
    close: &PalwEvalConeCloseV1,
    facts: &PalwEvalClaimFactsV1<'_>,
    narrowed: Option<u64>,
    limits: &misaka_palw_tir::demand::DemandLimits,
) -> PalwEvalCloseOutcomeV1 {
    use crate::palw_gen_court_v1::{PalwGenParamsV1, PalwGenVerdictV1, palw_gen_adjudicate_leaf_with_v1};
    let (v, gen_close, params) = open_eval_cone_close_v1(close, facts, narrowed)?;
    match palw_gen_adjudicate_leaf_with_v1(&v.case(), &gen_close, PalwGenParamsV1::Authenticated(&params), limits)
        .map_err(PalwEvalCourtErrorV1::Refused)?
    {
        PalwGenVerdictV1::Acquitted => Ok(None),
        PalwGenVerdictV1::Convicted { fault, .. } => Ok(Some(fault)),
    }
}

/// The output stage's output-node tile coordinates, in tile order: the stage the pipeline's output is read from,
/// the node and the last position, and how many tiles the output has.
fn output_tiles_v1(v: &PalwEvalVerifiedBindingV1) -> Option<(u8, Vec<PalwGenLeafCoordV1>)> {
    let stage = v.ctx.pipeline.output_stage;
    let sp = v.space.stages.get(stage as usize)?;
    let program = &v.ctx.programs[v.ctx.pipeline.stages[stage as usize].program as usize];
    let post = (program.occurrences().len() - 1) as u16;
    let kind = PalwGenLeafKindV1::Commit { occurrence: post, node: program.output.node() };
    let pos = sp.trip.checked_sub(1)?;
    let coords: Vec<PalwGenLeafCoordV1> =
        sp.leaves().iter().filter(|l| l.coord.pos == pos && l.coord.kind == kind).map(|l| l.coord).collect();
    (!coords.is_empty()).then_some((stage, coords))
}

/// **Check an evaluation decode close** (`EvalDecodeToken`): the claim's committed outputs against what they were
/// read from.
///
/// * `Token { t, row }` — a generating job's generated id `t` against the committed logits row of position
///   `|prompt| − 1 + t` (every tile, under the stream stage's root): the id must be the lane FP Job V4's rules select
///   from it with the committed ids before it ([`crate::palw_gen_court_v1::palw_gen_decode_door_v1`]). A job that
///   selects nothing (teacher-forced, judged) has no decode to close.
/// * `Score { tiles }` — a scoring job's committed score lanes against the output stage's committed output tile(s):
///   every tile of the output node at the stage's last position, proven under the stage's root; the first lane that is
///   not the committed score convicts (`TirOutputDigestMismatch`, at the lane). A leaf that is itself a lie is the
///   cone close's to convict, not this one's.
///
/// In a session the narrowed leaf must be a tile of what the close opens.
pub fn check_eval_decode_close_v1(
    close: &PalwEvalDecodeCloseV1,
    facts: &PalwEvalClaimFactsV1<'_>,
    narrowed: Option<u64>,
) -> PalwEvalCloseOutcomeV1 {
    use crate::palw_gen_court_v1::{PalwGenVerdictV1, palw_gen_decode_door_v1};
    use crate::palw_step_leg::PalwStepFaultV1;
    if close.version != PALW_IMPROVE_EVAL_COURT_VERSION_V1 {
        return Err(PalwEvalCourtErrorV1::Version(close.version));
    }
    let v = verify_eval_binding_v1(&close.binding, facts, None)?;
    let opened = |o: &PalwGenLeafOpeningV1| o.opened(&v.space).ok_or(PalwEvalCourtErrorV1::NoSuchLeaf(o.coord));
    let in_session = |leaves: &[PalwGenLeafCoordV1]| -> Result<(), PalwEvalCourtErrorV1> {
        let Some(narrowed) = narrowed else { return Ok(()) };
        let global = |c: &PalwGenLeafCoordV1| v.space.global_index(c);
        if leaves.iter().any(|c| global(c) == Some(narrowed)) {
            Ok(())
        } else {
            Err(PalwEvalCourtErrorV1::NotTheNarrowedLeaf { opened: leaves.first().and_then(global).unwrap_or(u64::MAX), narrowed })
        }
    };
    match &close.output {
        PalwEvalOutputCloseV1::Token { t, row } => {
            let Some(decode) = v.ctx.decode.as_ref() else {
                return Err(PalwEvalCourtErrorV1::NotThisJobsClose("a job that selects nothing has no decode to close"));
            };
            let leaves = row.iter().map(opened).collect::<Result<Vec<_>, _>>()?;
            in_session(&leaves.iter().map(|l| l.coord).collect::<Vec<_>>())?;
            match palw_gen_decode_door_v1(&v.case(), decode, close.binding.prompt_tokens, *t, &leaves)
                .map_err(PalwEvalCourtErrorV1::Refused)?
            {
                PalwGenVerdictV1::Acquitted => Ok(None),
                PalwGenVerdictV1::Convicted { fault, .. } => Ok(Some(fault)),
            }
        }
        PalwEvalOutputCloseV1::Score { tiles } => {
            if close.binding.score.is_empty() {
                return Err(PalwEvalCourtErrorV1::NotThisJobsClose("a job that commits no score has no score to close"));
            }
            let (stage, expected) =
                output_tiles_v1(&v).ok_or(PalwEvalCourtErrorV1::Binding("the pipeline has no output tile".into()))?;
            let leaves = tiles.iter().map(opened).collect::<Result<Vec<_>, _>>()?;
            if leaves.iter().map(|l| l.coord).collect::<Vec<_>>() != expected {
                return Err(PalwEvalCourtErrorV1::NotThisJobsClose(
                    "the close does not open every tile of the output node at the output stage's last position",
                ));
            }
            in_session(&expected)?;
            let sp = &v.space.stages[stage as usize];
            for leaf in &leaves {
                if !crate::palw_gen_step_v1::palw_gen_verify_leaf_v1(sp, &v.roots.stage_roots[stage as usize], leaf) {
                    return Err(PalwEvalCourtErrorV1::Refused(PalwGenCloseRefusalV1::LeafNotProven(leaf.coord)));
                }
            }
            let lanes: Vec<i128> = leaves.iter().flat_map(|l| l.values.iter().copied()).collect();
            if lanes.len() < close.binding.score.len() {
                return Err(PalwEvalCourtErrorV1::NotThisJobsClose("the output tiles hold fewer lanes than the committed score"));
            }
            match close.binding.score.iter().zip(&lanes).position(|(committed, lane)| *committed as i128 != *lane) {
                Some(i) => Ok(Some(PalwStepFaultV1::TirOutputDigestMismatch { value_index: i as u32 })),
                None => Ok(None),
            }
        }
    }
}

/// **The site a root claim opens on, as the fold derives it**: the binding verified against the claim and the chain,
/// the finalize's leaf the one the ladder narrowed to, and that leaf's site from the class's program — no
/// evaluation (the finalize is the acceptance layer's).
pub fn palw_eval_root_claim_site_v1(
    root: &PalwEvalRootClaimV1,
    facts: &PalwEvalClaimFactsV1<'_>,
    narrowed: u64,
) -> Result<crate::palw_tir_dissect_v1::PalwTirDissectSiteV1, String> {
    let v = verify_eval_binding_v1(&root.finalize.binding, facts, None).map_err(|e| e.to_string())?;
    let coord = root.finalize.disputed.coord;
    let index = v.space.global_index(&coord).ok_or_else(|| format!("{coord:?} is not a leaf of this execution"))?;
    if index != narrowed {
        return Err(format!("the root claim opens leaf {index}, the ladder narrowed to {narrowed}"));
    }
    crate::palw_gen_court_v1::palw_gen_dissect_site_v1(&v.space.stages[coord.stage as usize], &coord)
        .ok_or_else(|| "the narrowed leaf is not dissected".to_string())
}

/// **Admit an evaluation root claim** (the acceptance layer's, which holds the court's limits): its finalize's
/// carriage opens as a cone close's does at the narrowed leaf, without a conviction (a binding that convicts is
/// closed, not dissected), and F7's check over the stage
/// ([`crate::palw_gen_court_v1::palw_gen_check_root_claim_with_v1`]): the leaf dissected, the claim the site's, the
/// finalize the committed leaf reading exactly the claimed values. Returns the site.
pub fn check_eval_root_claim_v1(
    root: &PalwEvalRootClaimV1,
    facts: &PalwEvalClaimFactsV1<'_>,
    narrowed: u64,
    limits: &misaka_palw_tir::demand::DemandLimits,
) -> Result<crate::palw_tir_dissect_v1::PalwTirDissectSiteV1, String> {
    if root.version != crate::palw_tir_dissect_v1::PALW_TIR_DISSECT_OBJECT_VERSION_V1 {
        return Err(format!("root claim version {} is not the phase's", root.version));
    }
    let (v, finalize, params) = open_eval_cone_close_v1(&root.finalize, facts, Some(narrowed)).map_err(|e| e.to_string())?;
    crate::palw_gen_court_v1::palw_gen_check_root_claim_with_v1(
        &v.case(),
        &finalize,
        &root.elements,
        &root.totals,
        crate::palw_gen_court_v1::PalwGenParamsV1::Authenticated(&params),
        limits,
    )
}

/// **Grade an evaluation dissection's bottom** (`EvalDissection`) against its phase: the carriage opens as a cone
/// close's does at the phase's leaf (the ladder's), its convictions stand, and F7's bottom over the stage decides
/// ([`crate::palw_gen_court_v1::palw_gen_check_dissect_bottom_with_v1`]).
pub fn check_eval_dissect_bottom_v1(
    phase: &crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1,
    bottom: &PalwEvalConeCloseV1,
    facts: &PalwEvalClaimFactsV1<'_>,
    narrowed: u64,
    limits: &misaka_palw_tir::demand::DemandLimits,
) -> PalwEvalCloseOutcomeV1 {
    use crate::palw_gen_court_v1::{PalwGenParamsV1, PalwGenVerdictV1, palw_gen_check_dissect_bottom_with_v1};
    if narrowed != phase.leaf_index() {
        return Err(PalwEvalCourtErrorV1::NotTheNarrowedLeaf { opened: phase.leaf_index(), narrowed });
    }
    let (v, close, params) = open_eval_cone_close_v1(bottom, facts, Some(narrowed))?;
    match palw_gen_check_dissect_bottom_with_v1(&v.case(), &close, phase, PalwGenParamsV1::Authenticated(&params), limits)
        .map_err(PalwEvalCourtErrorV1::Refused)?
    {
        PalwGenVerdictV1::Acquitted => Ok(None),
        PalwGenVerdictV1::Convicted { fault, .. } => Ok(Some(fault)),
    }
}

// ---------------------------------------------------------------------------------------------
// An executor's evidence: every court move built from its own run
// ---------------------------------------------------------------------------------------------

/// **An executor's evidence** — the claim's facts, its weights, its run, its binding and the item's prompt — from
/// which it (or a challenger holding the same inputs) builds every court move, each carrying exactly the units the
/// court's evaluation reads (the builders record them).
pub struct PalwEvalEvidenceV1<'a> {
    pub facts: PalwEvalClaimFactsV1<'a>,
    /// The subject's weights, as the evaluation pipeline reads them (program 0's; the scoring programs have none).
    pub params: &'a dyn misaka_palw_tir::pipeline::PipelineParams,
    pub execution: &'a crate::palw_gen_worker_v1::PalwGenExecutionV1,
    pub binding: &'a PalwEvalBindingV1,
    pub prompt: &'a [u32],
    /// The composite reference, where the subject is a composite candidate (RFC-0004 §6.3).
    pub composite: Option<PalwTirCompositeRefV1>,
}

impl PalwEvalEvidenceV1<'_> {
    fn verified(&self) -> Result<PalwEvalVerifiedBindingV1, String> {
        verify_eval_binding_v1(self.binding, &self.facts, Some(self.prompt)).map_err(|e| e.to_string())
    }

    /// Every param leaf, as the close carries them: whole under the class's root, or per section for a composite.
    fn all_params(&self, v: &PalwEvalVerifiedBindingV1) -> Result<PalwEvalParamsV1, String> {
        use crate::palw_artifact::open_artifact_leaf_v1;
        let operands = crate::palw_gen_artifact_v1::palw_gen_inventory_operands_named_v1(
            &v.ctx.programs,
            self.params,
            PalwGenInventoryNamingV1::Evaluation,
        )
        .map_err(|e| e.to_string())?;
        let count = operands.len() as u32;
        let Some(composite) = &self.composite else {
            let openings = (0..count)
                .map(|i| open_artifact_leaf_v1(&operands, i).ok_or_else(|| format!("leaf {i} does not open")))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(PalwEvalParamsV1::Single(openings));
        };
        let p = u16::try_from(composite.p).map_err(|_| "a composite split past every param".to_string())?;
        let split = v.inventory.leaves_before_param_v1(0, p).ok_or("a composite split past every param")?;
        let (parent_ops, adapter_ops) = operands.split_at(split as usize);
        let section = |ops: &[crate::palw_artifact::PalwArtifactOperandV1]| {
            (0..ops.len() as u32)
                .map(|i| open_artifact_leaf_v1(ops, i).ok_or_else(|| format!("section leaf {i} does not open")))
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(PalwEvalParamsV1::Composite { artifact: *composite, parent: section(parent_ops)?, adapter: section(adapter_ops)? })
    }

    /// The close of leaf `index` (the claim's one order) holding EVERY unit: every leaf before it, every param leaf.
    fn full_close(&self, v: &PalwEvalVerifiedBindingV1, index: u64) -> Result<PalwGenCloseV1, String> {
        let (s, i) = v.space.locate(index).ok_or_else(|| format!("{index} is no leaf of this execution"))?;
        let open =
            |s: usize, i: usize| self.execution.open(s as u8, i as u64).ok_or_else(|| format!("stage {s} leaf {i} does not open"));
        let mut operands = Vec::new();
        for st in 0..=s as usize {
            let n = if st == s as usize { i as usize } else { v.space.stages[st].leaves().len() };
            for k in 0..n {
                operands.push(open(st, k)?);
            }
        }
        Ok(PalwGenCloseV1 { disputed: open(s as usize, i as usize)?, operands, image_tiles: Vec::new(), params: Vec::new() })
    }

    /// The params a close needs: `all` cut to the leaves `used` read.
    fn restrict_params(
        &self,
        v: &PalwEvalVerifiedBindingV1,
        all: &PalwEvalParamsV1,
        used: &crate::palw_gen_court_v1::PalwGenUsedUnitsV1,
    ) -> PalwEvalParamsV1 {
        let keep = |base: u32, openings: &[PalwArtifactOpeningV1]| -> Vec<PalwArtifactOpeningV1> {
            openings.iter().filter(|o| used.params.contains(&(base + o.leaf_index))).cloned().collect()
        };
        match all {
            PalwEvalParamsV1::Single(openings) => PalwEvalParamsV1::Single(keep(0, openings)),
            PalwEvalParamsV1::Composite { artifact, parent, adapter } => {
                let split = v.inventory.leaves_before_param_v1(0, artifact.p as u16).unwrap_or(0);
                PalwEvalParamsV1::Composite { artifact: *artifact, parent: keep(0, parent), adapter: keep(split, adapter) }
            }
        }
    }

    /// A cone close as it rides: lanes, the binding, and the prompt exactly when the stage reads it.
    fn wire(
        &self,
        v: &PalwEvalVerifiedBindingV1,
        close: &PalwGenCloseV1,
        params: PalwEvalParamsV1,
    ) -> Result<PalwEvalConeCloseV1, String> {
        let lanes = |o: &crate::palw_gen_step_v1::PalwGenOpenedLeafV1| {
            PalwGenLeafOpeningV1::of(&v.space, o).ok_or_else(|| format!("{:?} does not ride", o.coord))
        };
        let reads = crate::palw_gen_close_v1::palw_gen_stage_reads_prompt_v1(&v.ctx.pipeline, close.disputed.coord.stage as usize);
        Ok(PalwEvalConeCloseV1 {
            version: PALW_IMPROVE_EVAL_COURT_VERSION_V1,
            binding: self.binding.clone(),
            prompt_ids: if reads { self.prompt.to_vec() } else { Vec::new() },
            disputed: lanes(&close.disputed)?,
            operands: close.operands.iter().map(lanes).collect::<Result<_, _>>()?,
            params,
        })
    }

    /// **A cone close of leaf `index`**, carrying exactly what its cone reads.
    pub fn cone_close(&self, index: u64, limits: &misaka_palw_tir::demand::DemandLimits) -> Result<PalwEvalConeCloseV1, String> {
        use crate::palw_gen_court_v1::{PalwGenParamsV1, palw_gen_cone_units_with_v1, palw_gen_restrict_close_v1};
        let v = self.verified()?;
        let full = self.full_close(&v, index)?;
        let all = self.all_params(&v)?;
        let authed = authenticate_eval_params_v1(&all, &v).map_err(|e| e.to_string())?;
        let used = palw_gen_cone_units_with_v1(&v.case(), &full, PalwGenParamsV1::Authenticated(&authed), limits)
            .map_err(|e| e.to_string())?;
        let restricted = palw_gen_restrict_close_v1(&v.case(), &full, &used);
        self.wire(&v, &restricted, self.restrict_params(&v, &all, &used))
    }

    /// **A decode close of generated id `t`**: every tile of its logits row.
    pub fn decode_close(&self, t: u32) -> Result<PalwEvalDecodeCloseV1, String> {
        let v = self.verified()?;
        let out = misaka_palw_tir::pipeline::stream_stage(&v.ctx.pipeline).ok_or("the pipeline has no stream stage")?;
        let program = &v.ctx.programs[v.ctx.pipeline.stages[out].program as usize];
        let post = (program.occurrences().len() - 1) as u16;
        let kind = PalwGenLeafKindV1::Commit { occurrence: post, node: program.output.node() };
        let pos = self.binding.prompt_tokens.saturating_sub(1) + t;
        let mut row = Vec::new();
        for (i, leaf) in v.space.stages[out].leaves().iter().enumerate() {
            if leaf.coord.pos == pos && leaf.coord.kind == kind {
                let opened = self.execution.open(out as u8, i as u64).ok_or("a row tile does not open")?;
                row.push(PalwGenLeafOpeningV1::of(&v.space, &opened).ok_or("a row tile does not ride")?);
            }
        }
        Ok(PalwEvalDecodeCloseV1 {
            version: PALW_IMPROVE_EVAL_COURT_VERSION_V1,
            binding: self.binding.clone(),
            output: PalwEvalOutputCloseV1::Token { t, row },
        })
    }

    /// **A score close**: every tile of the output node at the output stage's last position.
    pub fn score_close(&self) -> Result<PalwEvalDecodeCloseV1, String> {
        let v = self.verified()?;
        let (stage, coords) = output_tiles_v1(&v).ok_or("the pipeline has no output tile")?;
        let sp = &v.space.stages[stage as usize];
        let mut tiles = Vec::new();
        for coord in coords {
            let i = sp.leaf_index(&coord).ok_or("an output tile is no leaf of the stage")?;
            let opened = self.execution.open(stage, i).ok_or("an output tile does not open")?;
            tiles.push(PalwGenLeafOpeningV1::of(&v.space, &opened).ok_or("an output tile does not ride")?);
        }
        Ok(PalwEvalDecodeCloseV1 {
            version: PALW_IMPROVE_EVAL_COURT_VERSION_V1,
            binding: self.binding.clone(),
            output: PalwEvalOutputCloseV1::Score { tiles },
        })
    }

    /// **The responder's root claim at dissected leaf `index`**: the honest totals, the closure's elements, and a
    /// finalize carrying exactly what the court reads to admit them.
    pub fn root_claim(&self, index: u64, limits: &misaka_palw_tir::demand::DemandLimits) -> Result<PalwEvalRootClaimV1, String> {
        use crate::palw_gen_court_v1::{PalwGenParamsV1, palw_gen_build_root_claim_recorded_with_v1, palw_gen_restrict_close_v1};
        let v = self.verified()?;
        let full = self.full_close(&v, index)?;
        let all = self.all_params(&v)?;
        let authed = authenticate_eval_params_v1(&all, &v).map_err(|e| e.to_string())?;
        let (elements, totals, used) =
            palw_gen_build_root_claim_recorded_with_v1(&v.case(), &full, PalwGenParamsV1::Authenticated(&authed), limits)
                .map_err(|e| e.to_string())?;
        let restricted = palw_gen_restrict_close_v1(&v.case(), &full, &used);
        let finalize = self.wire(&v, &restricted, self.restrict_params(&v, &all, &used))?;
        Ok(PalwEvalRootClaimV1 {
            version: crate::palw_tir_dissect_v1::PALW_TIR_DISSECT_OBJECT_VERSION_V1,
            elements,
            totals,
            finalize: Box::new(finalize),
        })
    }

    /// The history positions `[from, to)` of each child of the phase's disputed range.
    fn child_positions(
        &self,
        v: &PalwEvalVerifiedBindingV1,
        phase: &crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1,
    ) -> Result<Vec<(usize, usize)>, String> {
        let (s, _) = v.space.locate(phase.leaf_index()).ok_or("the phase's leaf is no leaf of this execution")?;
        let tile = v.space.stages[s as usize].layout.h_tile.max(1) as u64;
        let h = phase.history_positions() as u64;
        Ok(phase
            .child_ranges()
            .into_iter()
            .map(|(first, count)| ((first * tile) as usize, ((first + count) * tile).min(h) as usize))
            .collect())
    }

    /// **The responder's round**: every child's honest partials (the others supplied from the root).
    pub fn round(
        &self,
        phase: &crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1,
        limits: &misaka_palw_tir::demand::DemandLimits,
    ) -> Result<crate::palw_tir_dissect_v1::PalwTirDissectRoundV1, String> {
        use crate::palw_gen_court_v1::{PalwGenParamsV1, palw_gen_dissect_partials_with_v1};
        let v = self.verified()?;
        let full = self.full_close(&v, phase.leaf_index())?;
        let all = self.all_params(&v)?;
        let authed = authenticate_eval_params_v1(&all, &v).map_err(|e| e.to_string())?;
        let mut children = Vec::new();
        for range in self.child_positions(&v, phase)? {
            let claim =
                palw_gen_dissect_partials_with_v1(&v.case(), &full, phase, range, PalwGenParamsV1::Authenticated(&authed), limits)
                    .map_err(|e| e.to_string())?
                    .map_err(|v| format!("the carriage convicts its own executor: {v:?}"))?;
            children.push(claim);
        }
        Ok(crate::palw_tir_dissect_v1::PalwTirDissectRoundV1 {
            version: crate::palw_tir_dissect_v1::PALW_TIR_DISSECT_OBJECT_VERSION_V1,
            children,
        })
    }

    /// **The bottom close** over the phase's terminal tile, carrying exactly what its evaluation reads.
    pub fn bottom(
        &self,
        phase: &crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1,
        limits: &misaka_palw_tir::demand::DemandLimits,
    ) -> Result<PalwEvalConeCloseV1, String> {
        use crate::palw_gen_court_v1::{PalwGenParamsV1, palw_gen_dissect_partials_recorded_with_v1, palw_gen_restrict_close_v1};
        let v = self.verified()?;
        let full = self.full_close(&v, phase.leaf_index())?;
        let all = self.all_params(&v)?;
        let authed = authenticate_eval_params_v1(&all, &v).map_err(|e| e.to_string())?;
        let range = phase.terminal_range().ok_or("the dissection has no bottom yet")?;
        let (out, used) = palw_gen_dissect_partials_recorded_with_v1(
            &v.case(),
            &full,
            phase,
            range,
            PalwGenParamsV1::Authenticated(&authed),
            limits,
        )
        .map_err(|e| e.to_string())?;
        out.map_err(|v| format!("the carriage convicts its own executor: {v:?}"))?;
        let restricted = palw_gen_restrict_close_v1(&v.case(), &full, &used);
        self.wire(&v, &restricted, self.restrict_params(&v, &all, &used))
    }
}

/// **The dissected leaf an evaluation cone accusation names, if it names one**: the binding the claim's, the leaf a leaf
/// of this execution proven under its stage's root, and its site — `Some(global index)` when the leaf's cone reduces
/// over the history (it is dissected, never closed whole under the held regime); `None` for any other leaf. No
/// evaluation: the fold asks it too.
pub fn palw_eval_named_dissected_leaf_v1(
    close: &PalwEvalConeCloseV1,
    facts: &PalwEvalClaimFactsV1<'_>,
) -> Result<Option<u64>, String> {
    let v = verify_eval_binding_v1(&close.binding, facts, None).map_err(|e| e.to_string())?;
    let coord = close.disputed.coord;
    let index = v.space.global_index(&coord).ok_or_else(|| format!("{coord:?} is not a leaf of this execution"))?;
    let opened = close.disputed.opened(&v.space).ok_or_else(|| format!("{coord:?} does not open under the space"))?;
    let sp = &v.space.stages[coord.stage as usize];
    if !crate::palw_gen_step_v1::palw_gen_verify_leaf_v1(sp, &v.roots.stage_roots[coord.stage as usize], &opened) {
        return Err(format!("{coord:?} is not under its stage's root"));
    }
    Ok(crate::palw_gen_court_v1::palw_gen_dissect_site_v1(sp, &coord).map(|_| index))
}
