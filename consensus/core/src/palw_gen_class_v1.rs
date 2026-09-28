//! **RFC-0003: a generative class — a pipeline of PALW-TIR version-2 programs — what it is, what
//! identifies it, and what its registration carries** (RFC-0003 §I.2.3; spec 04b §15; Phase F's
//! step F2 pattern, [`crate::palw_tir_class_v1`]).
//!
//! A generative class is a [`PalwGenClassV1`]: its profile (a `Text` class's output stage is its
//! language model, RFC-0003 §II.2.1, and its output the generated ids with no root); the canonical
//! bytes of a
//! `TirPipelineV1` (spec 04b §15.6) and of every `TirProgramV2` its stages run, in the pipeline's
//! program order; one commitment layout per stage (Phase F's [`PalwTirLayoutV1`], D5 per stage); the
//! canonical output header (`OutputSpecV1`, RFC-0003 §I.3); the job-parameter domains it offers
//! ([`PalwGenOffersV1`]: step counts, scalar intervals, prompt lengths and the image slots — each
//! image's size and input tile, §II.4); and the tokenizer its prompts are ids of. Its identity is
//!
//! ```text
//! pipeline_root            = H64(key "misaka-palw/gen/pipeline-root/v1",
//!                                le32(|pipeline|) ‖ pipeline ‖ le16(|programs|) ‖ graph_ir_root(program_0) ‖ …)
//! class_terms              = H64(key "misaka-palw/gen/class-terms/v1", borsh(layouts) ‖ borsh(output) ‖ borsh(offers))
//! tir_pipeline_class_id_v1 = H64(key "misaka-palw/tir/pipeline-class-id/v1",
//!                                le16(version) ‖ profile ‖ pipeline_root ‖ class_terms ‖ artifact_root ‖ tokenizer_id)
//! ```
//!
//! — every program (each declares `prim_set_id`), the edges, the layouts, the output header, the
//! offers, the weights (`artifact_root`) and the tokenizer; nothing node-local exists to commit.
//! `graph_ir_root` is Phase F's key over a program's bytes (a version-2 program's bytes differ from
//! any version-1 program's, so the roots never meet). The class key is neither a legacy profile's
//! nor Phase F's, so a pipeline class id never equals either.
//!
//! **No canonical job rides.** A Phase F registration carries the job its class is paid per. A
//! generative class's yardstick is a function of the class itself — its most expensive offered job
//! (the largest offered step count, the longest prompts) — from which admission counts
//! `pwu_per_inference` (RFC-0003's activation step 4); nothing a registrant says about it is believed.
//!
//! **Carriage.** The registration object is APPENDED to the lifecycle objects
//! (`PalwConsensusObjectV2::ClassRegisteredGenV1`, tag 67, from Phase F's allocation): an older build
//! on a ruleset that declared `palw_audit_2026_09_11` skips a payload it cannot decode (A-2). A class
//! larger than one carrier rides in `ObjectChunk`s (the user's decision 8, multi-carrier registration)
//! once admission admits the kind. Below `palw_gen_v1` this build drops the object by name exactly as
//! an older build skips it, and the fold refuses it as the second lock, so every network folds as
//! before the variant existed; past it the pipeline admission
//! ([`crate::palw_gen_admission_v1::verify_gen_class_admission_v1`]) decides, and the fold writes the
//! class's `gen_classes` row ([`PalwGenClassRecordV1`]).
//!
//! [`palw_gen_class_preflight_v1`] is the question a registrant asks first, and admission's first
//! half: the class's structure against the fence — versions, the profile and its ceilings, the strict
//! decoding of every program and of the pipeline (NF-P1…P9), the layouts' shape, the output header
//! against the output stage (PALW-OUT-2), the offers against the bindings — and the IR's admission of
//! the pipeline (spec 04b §15.9) under the profile's ceilings. Its step-leaf count is a NECESSARY
//! condition only (each stage admitted at its widest commit tile, so the true count is at least the
//! one checked); the one step tree that counts leaves exactly is the pipeline admission's.

use crate::Hash64;
use crate::palw_gen_v1::{PalwGenFenceV1, PalwGenProfileV1};
use crate::palw_state_v2::{PalwBondKeyV2, PalwPwuRuleV2};
use crate::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::admit::{TirAdmitError, TirAdmitInputsV1, TirCeilingsV1};
use misaka_palw_tir::admit_v2::{TirJobCeilingsV1, TirPipelineAdmissionV1, tir_admit_pipeline_staged_v1};
use misaka_palw_tir::pipeline::{Binding, TirPipelineV1, TokenRule, TokenSource, TripRule};
use misaka_palw_tir::program_v2::{InputSource, OutputDecl, TirProgramV2};
use misaka_palw_tir::types::Dim;

pub const PALW_GEN_CLASS_ID_DOMAIN_V1: &[u8] = b"misaka-palw/tir/pipeline-class-id/v1";
pub const PALW_GEN_PIPELINE_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/gen/pipeline-root/v1";
pub const PALW_GEN_CLASS_TERMS_DOMAIN_V1: &[u8] = b"misaka-palw/gen/class-terms/v1";
pub const PALW_GEN_CLASS_REGISTRATION_DOMAIN_V1: &[u8] = b"misaka-palw/gen/class-registration/message/v1";
/// The ML-DSA-87 context a registrant bond signs a generative registration under.
pub const PALW_GEN_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/gen/class-registration/mldsa87/v1";
pub const PALW_GEN_CLASS_VERSION_V1: u16 = 1;
/// The most step counts a class may offer (RFC-0003 §II.1.1: a bounded set).
pub const PALW_GEN_MAX_OFFERED_STEPS_V1: usize = 8;
/// The most images a job of a class may carry (RFC-0003 §II.4): the pipeline format's cap.
pub const PALW_GEN_MAX_IMAGES_V1: usize = misaka_palw_tir::pipeline::MAX_JOB_IMAGES;

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **One image slot of a class** (RFC-0003 §II.4): image `i` of every job is `u8` HWC RGB at exactly
/// `h × w`, committed by its `input_root` over input tiles of `tile_len` bytes (§I.3.2's construction,
/// `misaka_palw_gen::output::input_image_root_v1`). The gateway resizes or letterboxes to the declared
/// size and says so; another size is another class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenImageOfferV1 {
    pub h: u32,
    pub w: u32,
    /// Bytes per input tile, in `[4, 2^16]`: what a court opens against `input_root`.
    pub tile_len: u32,
    /// **The image's price in prompt tokens** (RFC-0003 open question 13, the recommendation —
    /// PENDING USER CONFIRMATION): a text class's image stages are charged as this many
    /// prefill-equivalent tokens per image, at the job's per-token price. Declared by the registrant
    /// and floored by admission at `⌈admitted per-image work / per-token work⌉`
    /// ([`palw_gen_image_token_floor_v1`]); `0` on a class that is not a text class (its images are
    /// its job, priced by its own profile).
    pub token_equivalents: u32,
}

/// **A job's reference to one of its images** (RFC-0003 §II.4, `ImageInputRefV1`): the chain carries
/// the image's `input_root` and size, never its bytes; the bytes travel like `PanelDa` prompt ids
/// (with the capture to the panel), and a dispute opens tiles against the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenImageInputRefV1 {
    pub input_root: Hash64,
    pub h: u32,
    pub w: u32,
}

/// Why a job's images are not the class's.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwGenJobImageErrorV1 {
    #[error("the job carries {got} images and the class takes exactly {want}")]
    ImageCount { want: usize, got: usize },
    #[error("image {index} is {got_h}×{got_w}; the class's slot {index} is {h}×{w}")]
    ImageSizeNotOffered { index: usize, h: u32, w: u32, got_h: u32, got_w: u32 },
}

/// **Are a job's images the class's?** Exactly one per slot, each at its slot's size — every bound
/// image is read (NF-P10), so none is optional; a class with fewer images is another class.
pub fn palw_gen_job_images_admitted_v1(
    offers: &PalwGenOffersV1,
    images: &[PalwGenImageInputRefV1],
) -> Result<(), PalwGenJobImageErrorV1> {
    if images.len() != offers.images.len() {
        return Err(PalwGenJobImageErrorV1::ImageCount { want: offers.images.len(), got: images.len() });
    }
    for (index, (slot, image)) in offers.images.iter().zip(images).enumerate() {
        if (slot.h, slot.w) != (image.h, image.w) {
            return Err(PalwGenJobImageErrorV1::ImageSizeNotOffered { index, h: slot.h, w: slot.w, got_h: image.h, got_w: image.w });
        }
    }
    Ok(())
}

/// One job scalar's accepted interval, inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenScalarOfferV1 {
    pub lo: i64,
    pub hi: i64,
}

/// **What a job of the class may ask** (RFC-0003 §I.2.3 `offers`): the job-parameter domains, each a
/// bounded set or interval (§Security, *ceilings*). Generic across profiles in v1; a profile's own
/// offers (an image class's sampler descriptor, guidance grid and resolution) arrive with it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenOffersV1 {
    /// The step counts a job may ask, strictly ascending, at most [`PALW_GEN_MAX_OFFERED_STEPS_V1`],
    /// each in `[1, max_trip]` of every `JobSteps` stage; empty exactly when no stage's trip is
    /// `JobSteps`.
    pub steps: Vec<u32>,
    /// Job scalar `i`'s interval, one per scalar the job carries: every `i` below the length is bound
    /// by some `JobScalar` edge and none past it, and each lies inside every input it is bound to.
    pub scalars: Vec<PalwGenScalarOfferV1>,
    /// The longest prompt (the user's ids, before the class's template) a job may carry; 0 when no
    /// rule reads the prompt.
    pub max_prompt_tokens: u32,
    /// The longest negative prompt; 0 when no rule reads it (no true CFG).
    pub max_negative_tokens: u32,
    /// The image slots, in image order: exactly the images the pipeline binds, each at its bound
    /// size (NF-P10). A job carries one image per slot.
    pub images: Vec<PalwGenImageOfferV1>,
}

/// **A generative class**: a pipeline of version-2 programs, its layouts, its output, its offers and
/// its tokenizer (RFC-0003 §I.2.3).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenClassV1 {
    /// [`PALW_GEN_CLASS_VERSION_V1`].
    pub version: u16,
    /// A [`PalwGenProfileV1`] tag.
    pub profile: u8,
    /// The canonical `TirPipelineV1` bytes. Carried, never re-encoded: the bytes ARE the pipeline.
    pub pipeline: Vec<u8>,
    /// The canonical `TirProgramV2` bytes of every program, in the pipeline's program order.
    pub programs: Vec<Vec<u8>>,
    /// One commitment layout per stage, in stage order (`max_context` is the stage's `max_trip`).
    pub layouts: Vec<PalwTirLayoutV1>,
    /// The canonical output header: the kind the profile produces and the output stage's shape.
    pub output: OutputSpecV1,
    pub offers: PalwGenOffersV1,
    /// The tokenizer the class's prompt ids are ids of.
    pub tokenizer_id: Hash64,
}

impl PalwGenClassV1 {
    /// `pipeline_root`: the pipeline's bytes and every program's `graph_ir_root`, in order.
    pub fn pipeline_root(&self) -> Hash64 {
        let mut parts: Vec<Vec<u8>> = Vec::with_capacity(4 + self.programs.len());
        parts.push((self.pipeline.len() as u32).to_le_bytes().to_vec());
        parts.push(self.pipeline.clone());
        parts.push((self.programs.len() as u16).to_le_bytes().to_vec());
        for program in &self.programs {
            parts.push(crate::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(program).as_byte_slice().to_vec());
        }
        keyed64(PALW_GEN_PIPELINE_ROOT_DOMAIN_V1, &parts.iter().map(Vec::as_slice).collect::<Vec<_>>())
    }

    /// The layouts, the output header and the offers, as the class id binds them.
    pub fn terms_digest(&self) -> Hash64 {
        let layouts = borsh::to_vec(&self.layouts).expect("layouts are borsh-serializable");
        let output = borsh::to_vec(&self.output).expect("an output header is borsh-serializable");
        let offers = borsh::to_vec(&self.offers).expect("offers are borsh-serializable");
        keyed64(PALW_GEN_CLASS_TERMS_DOMAIN_V1, &[&layouts, &output, &offers])
    }

    /// **The class id** (`tir_pipeline_class_id_v1`, PALW-GEN-4) over the weights it is registered
    /// with. Every field is a fact about the model; none is a node's choice.
    pub fn class_id(&self, artifact_root: &Hash64) -> Hash64 {
        keyed64(
            PALW_GEN_CLASS_ID_DOMAIN_V1,
            &[
                &self.version.to_le_bytes(),
                &[self.profile],
                self.pipeline_root().as_byte_slice(),
                self.terms_digest().as_byte_slice(),
                artifact_root.as_byte_slice(),
                self.tokenizer_id.as_byte_slice(),
            ],
        )
    }

    /// Every program, then the pipeline over them, decoded strictly (spec 04b §15.1, §15.6: the unique
    /// encodings in normal form, every edge proved). A network's own ceilings are the preflight's.
    pub fn decode(&self) -> misaka_palw_tir::TirResult<(Vec<TirProgramV2>, TirPipelineV1)> {
        let programs = self.programs.iter().map(|b| TirProgramV2::decode_canonical(b)).collect::<Result<Vec<_>, _>>()?;
        let pipeline = TirPipelineV1::decode_canonical(&self.pipeline, &programs)?;
        Ok((programs, pipeline))
    }

    /// Bytes of the pipeline and every program (what `max_class_bytes` bounds).
    pub fn carried_bytes(&self) -> u64 {
        self.pipeline.len() as u64 + self.programs.iter().map(|p| p.len() as u64).sum::<u64>()
    }
}

/// **What a generative registration carries** — Phase F's carriage with the pipeline class in place
/// of the program, and no canonical job (see the module doc).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenAdmissionCarriageV1 {
    pub class: PalwGenClassV1,
    /// An Active bond on this chain.
    pub registrant_bond: PalwBondKeyV2,
    /// ML-DSA-87 over [`palw_gen_class_registration_message_v1`] under
    /// [`PALW_GEN_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1`], by the registrant bond's registered key.
    pub signature: Vec<u8>,
}

/// **The message a registrant bond signs** for a generative registration: every field the object
/// carries, the class included, under the network domain and its own key, so neither a legacy nor an
/// IR registration's signature can be lifted onto it.
#[allow(clippy::too_many_arguments)]
pub fn palw_gen_class_registration_message_v1(
    network_domain: Hash64,
    class_id: Hash64,
    share_permille: u16,
    activation_daa: u64,
    registrant_bond: &PalwBondKeyV2,
    artifact_root: Hash64,
    slash_value_per_pwu: u64,
    initial_target: u128,
    pwu_rule: &PalwPwuRuleV2,
    class: &PalwGenClassV1,
) -> Hash64 {
    keyed64(
        PALW_GEN_CLASS_REGISTRATION_DOMAIN_V1,
        &[
            network_domain.as_byte_slice(),
            class_id.as_byte_slice(),
            &share_permille.to_le_bytes(),
            &activation_daa.to_le_bytes(),
            &borsh::to_vec(registrant_bond).expect("a bond key is borsh-serializable"),
            artifact_root.as_byte_slice(),
            &slash_value_per_pwu.to_le_bytes(),
            &initial_target.to_le_bytes(),
            &borsh::to_vec(pwu_rule).expect("a pwu rule is borsh-serializable"),
            &borsh::to_vec(class).expect("a class is borsh-serializable"),
        ],
    )
}

/// **Does a V2 bundle's genesis register a generative class?** RFC-0003 v1 has no genesis rows;
/// `Params::validate_palw_v2` refuses a genesis that carries one.
pub fn palw_genesis_registers_gen_class_v1(bundle: &crate::palw_mode_v2::PalwConsensusParamsV2) -> bool {
    bundle.genesis_objects.iter().any(|o| matches!(o, crate::palw_state_v2::PalwConsensusObjectV2::ClassRegisteredGenV1 { .. }))
}

// ---------------------------------------------------------------------------------------------
// The preflight
// ---------------------------------------------------------------------------------------------

/// Why the preflight refused a class — each by name.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwGenClassErrorV1 {
    #[error("the class or a layout is not a version this build reads: {0}")]
    Version(String),
    #[error("profile tag {0} is not a generative profile")]
    Profile(u8),
    #[error("{what}: {value} exceeds the profile's ceiling {cap}")]
    Exceeds { what: &'static str, value: u64, cap: u64 },
    #[error("a program or the pipeline is refused: {0}")]
    Program(String),
    #[error("the pipeline's admission refuses {limit} at {at}: {value} > {cap}")]
    AdmissionExceeds { limit: &'static str, at: String, value: u64, cap: u64 },
    #[error("the programs: {0}")]
    Programs(String),
    #[error("the layouts: {0}")]
    Layout(String),
    #[error("the output: {0}")]
    Output(String),
    #[error("the offers: {0}")]
    Offers(String),
}

/// What the preflight learned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwGenClassReportV1 {
    pub profile: PalwGenProfileV1,
    /// The IR's admission of the pipeline at each stage's widest commit tile.
    pub admission: TirPipelineAdmissionV1,
    /// The output node's commit tile in the output stage's layout: the tile the output digest is
    /// aligned with (PALW-OUT-3). `None` for a text class, whose output is its generated ids and has
    /// no output root (RFC-0003 §I.3.3).
    pub output_tile_len: Option<u32>,
    /// Whether any stage draws `R` (PALW-GEN-6: a job's seed is all zeros exactly when it does not).
    pub draws_randomness: bool,
    /// A text class's per-slot price floor in prompt tokens ([`palw_gen_image_token_floor_v1`]);
    /// empty for every other class.
    pub image_token_floor: Vec<u32>,
}

/// Work in MAC-equivalents: ADR-0131's table over §8's three arithmetic counts (a MAC 1, an
/// elementwise op 1, a transcendental 4) — the units the canonical work rule prices arithmetic in.
pub fn palw_gen_work_units_v1(cost: &misaka_palw_tir::admit::CostV1) -> u128 {
    let t = crate::palw_economic_compute_v1::PALW_ECONOMIC_COST_TABLE_V1;
    cost.macs as u128 * t.matmul_mac as u128
        + cost.elementwise as u128 * t.elementwise as u128
        + cost.transcendentals as u128 * t.transcendental as u128
}

/// **The price floor of every image slot of a text class, in prompt tokens** (RFC-0003 open question
/// 13's recommendation, PENDING USER CONFIRMATION): `⌈admitted per-image work / per-token work⌉`.
///
/// * **Per-image work.** Every stage but the text stage is image work: its admitted job work (every
///   position, [`palw_gen_work_units_v1`]) is split evenly over the images it depends on — the images
///   it binds and those of every earlier stage it reads — and slot `i`'s work is the sum of its
///   shares, each rounded up.
/// * **Per-token work.** The text stage's admitted work for one position: what a prompt token costs.
///
/// `Err` names a stage of a text class that depends on no image: prefill no rule prices (RFC-0001's
/// D10 pays decode leaves only), refused rather than carried free.
pub fn palw_gen_image_token_floor_v1(
    pipeline: &TirPipelineV1,
    admission: &TirPipelineAdmissionV1,
    images: usize,
) -> Result<Vec<u32>, PalwGenClassErrorV1> {
    let text = pipeline.output_stage as usize;
    let mut depends: Vec<std::collections::BTreeSet<u8>> = Vec::with_capacity(pipeline.stages.len());
    let mut work = vec![0u128; images];
    for (s, st) in pipeline.stages.iter().enumerate() {
        let mut set = std::collections::BTreeSet::new();
        for b in &st.bind {
            match b {
                Binding::JobImage { index } => {
                    set.insert(*index);
                }
                Binding::StageRows { stage, .. } | Binding::StageFinal { stage } | Binding::StageRowCount { stage, .. } => {
                    set.extend(depends[*stage as usize].iter().copied());
                }
                _ => {}
            }
        }
        if s != text {
            if set.is_empty() {
                return Err(PalwGenClassErrorV1::Offers(format!(
                    "stage {s} ({}) of a text class reads no image: prefill no rule prices",
                    st.name
                )));
            }
            let share = palw_gen_work_units_v1(&admission.stages[s].job_cost).div_ceil(set.len() as u128);
            for i in &set {
                if let Some(w) = work.get_mut(*i as usize) {
                    *w = w.saturating_add(share);
                }
            }
        }
        depends.push(set);
    }
    let per_token = palw_gen_work_units_v1(&admission.stages[text].admission.view.position.cost).max(1);
    Ok(work.iter().map(|w| u32::try_from(w.div_ceil(per_token)).unwrap_or(u32::MAX)).collect())
}

fn fixed_shape(dims: &[Dim]) -> Option<Vec<u32>> {
    dims.iter().map(|d| if let Dim::Fixed(n) = d { Some(*n) } else { None }).collect()
}

/// Every reader of the job's prompts: `(source, template ids around it, room)` — a token rule, or
/// the text stage, which reads the prompt bare within its `max_trip`.
fn prompt_readers(pipeline: &TirPipelineV1) -> Vec<(TokenSource, usize, u32)> {
    let mut readers = Vec::new();
    let template = |r: &TokenRule| r.prefix.len() + r.suffix.len();
    for st in &pipeline.stages {
        if let Some(rule) = &st.tokens {
            readers.push((rule.source, template(rule), st.max_trip));
        }
        if matches!(st.trip, TripRule::TextStream) {
            readers.push((TokenSource::Prompt, 0, st.max_trip));
        }
        for b in &st.bind {
            if let Binding::JobTokens { rule } | Binding::JobTokenCount { rule } = b {
                readers.push((rule.source, template(rule), rule.pad.map_or(u32::MAX, |p| p.to_len)));
            }
        }
    }
    readers
}

fn check_offers(pipeline: &TirPipelineV1, programs: &[TirProgramV2], offers: &PalwGenOffersV1) -> Result<(), PalwGenClassErrorV1> {
    let bad = |m: String| Err(PalwGenClassErrorV1::Offers(m));
    // Steps: offered exactly when a stage runs the job's steps, each within every such stage.
    let step_stages: Vec<u32> =
        pipeline.stages.iter().filter(|st| matches!(st.trip, TripRule::JobSteps)).map(|st| st.max_trip).collect();
    if step_stages.is_empty() != offers.steps.is_empty() {
        return bad("step counts are offered exactly when a stage's trip is the job's steps".into());
    }
    if offers.steps.len() > PALW_GEN_MAX_OFFERED_STEPS_V1 || offers.steps.windows(2).any(|w| w[0] >= w[1]) {
        return bad(format!("at most {PALW_GEN_MAX_OFFERED_STEPS_V1} step counts, strictly ascending"));
    }
    let widest = step_stages.iter().copied().min().unwrap_or(0);
    if offers.steps.first().is_some_and(|s| *s == 0) || offers.steps.last().is_some_and(|s| *s > widest) {
        return bad(format!("every step count is in [1, {widest}]"));
    }
    // Scalars: one interval per bound scalar, each inside every input bound to it.
    let mut bound = vec![false; offers.scalars.len()];
    for st in &pipeline.stages {
        let prog = &programs[st.program as usize];
        let externals = prog.inputs.iter().filter(|d| d.is_external());
        for (b, d) in st.bind.iter().zip(externals) {
            if let Binding::JobScalar { index } = b {
                let Some(offer) = offers.scalars.get(*index as usize) else {
                    return bad(format!("stage {} binds job scalar {index}, which is not offered", st.name));
                };
                bound[*index as usize] = true;
                let (lo, hi) = d.interval();
                if offer.lo > offer.hi || (offer.lo as i128) < lo || (offer.hi as i128) > hi {
                    return bad(format!(
                        "job scalar {index}'s [{}, {}] is not inside input {}'s [{lo}, {hi}]",
                        offer.lo, offer.hi, d.name
                    ));
                }
            }
        }
    }
    if let Some(i) = bound.iter().position(|b| !b) {
        return bad(format!("job scalar {i} is offered and bound to nothing (two job ids for one computation)"));
    }
    // Images: one slot per bound image, at its bound size, with a tile a digest can use.
    let info =
        misaka_palw_tir::pipeline::validate_pipeline(pipeline, programs).map_err(|e| PalwGenClassErrorV1::Program(e.to_string()))?;
    if offers.images.len() != info.images.len() {
        return bad(format!("{} image slots are offered and the pipeline binds {}", offers.images.len(), info.images.len()));
    }
    for (i, (slot, [h, w])) in offers.images.iter().zip(&info.images).enumerate() {
        if (slot.h, slot.w) != (*h, *w) {
            return bad(format!("image slot {i} is {}×{} and the pipeline binds {h}×{w}", slot.h, slot.w));
        }
        if !(4..=1 << 16).contains(&slot.tile_len) {
            return bad(format!("image slot {i}'s input tile {} is outside [4, 2^16]", slot.tile_len));
        }
        // Only a text class prices its images in prompt tokens; its floor is checked with admission.
        let text = pipeline.stages.iter().any(|st| matches!(st.trip, TripRule::TextStream));
        if !text && slot.token_equivalents != 0 {
            return bad(format!("image slot {i} declares a token price on a class that is not a text class"));
        }
    }
    // Prompts: an offered length fits every rule that reads it; nothing is offered that no rule reads.
    for (source, max, what) in [
        (TokenSource::Prompt, offers.max_prompt_tokens, "prompt"),
        (TokenSource::Negative, offers.max_negative_tokens, "negative prompt"),
    ] {
        let readers: Vec<(usize, u32)> =
            prompt_readers(pipeline).into_iter().filter(|(s, _, _)| *s == source).map(|(_, t, r)| (t, r)).collect();
        if readers.is_empty() && max != 0 {
            return bad(format!("a {what} is offered and no rule reads it"));
        }
        for (template, room) in readers {
            let need = template as u64 + max as u64;
            if need > room as u64 {
                return bad(format!("the longest {what} ({max} ids) and its template need {need} positions; the reader has {room}"));
            }
        }
    }
    // A text stream starts with at least one prompt id: a text class offers a prompt.
    if pipeline.stages.iter().any(|st| matches!(st.trip, TripRule::TextStream)) && offers.max_prompt_tokens == 0 {
        return bad("a text class offers a prompt of at least one id".into());
    }
    Ok(())
}

/// **The preflight of a generative class** under an armed fence's ceilings for its profile: the
/// structure, the offers, the output header, and the IR's admission of the pipeline (see the module
/// doc for what is exact and what is a necessary condition).
pub fn palw_gen_class_preflight_v1(
    class: &PalwGenClassV1,
    fence: &PalwGenFenceV1,
) -> Result<PalwGenClassReportV1, PalwGenClassErrorV1> {
    // 1. Versions, the profile, the carried bytes.
    if class.version != PALW_GEN_CLASS_VERSION_V1 {
        return Err(PalwGenClassErrorV1::Version(format!("class version {}", class.version)));
    }
    if let Some(l) = class.layouts.iter().find(|l| l.version != PALW_TIR_LAYOUT_VERSION_V1) {
        return Err(PalwGenClassErrorV1::Version(format!("layout version {}", l.version)));
    }
    let profile = PalwGenProfileV1::from_tag(class.profile).ok_or(PalwGenClassErrorV1::Profile(class.profile))?;
    let ceilings = fence.ceilings.of(profile);
    let exceeds =
        |what, value: u64, cap: u64| if value > cap { Err(PalwGenClassErrorV1::Exceeds { what, value, cap }) } else { Ok(()) };
    exceeds("class bytes", class.carried_bytes(), ceilings.max_class_bytes as u64)?;

    // 2. Every program and the pipeline, strictly; every program run, none twice, each the fence's set.
    let (programs, pipeline) = class.decode().map_err(|e| PalwGenClassErrorV1::Program(e.to_string()))?;
    exceeds("stages", pipeline.stages.len() as u64, ceilings.max_stages as u64)?;
    for (k, (p, bytes)) in programs.iter().zip(&class.programs).enumerate() {
        if Hash64::from_bytes(p.prim_set_id) != fence.prim_set_id {
            return Err(PalwGenClassErrorV1::Programs(format!("program {k} declares another primitive set")));
        }
        if class.programs[..k].contains(bytes) {
            return Err(PalwGenClassErrorV1::Programs(format!("program {k} is carried twice")));
        }
    }
    if let Some(k) = (0..programs.len()).find(|k| !pipeline.stages.iter().any(|st| st.program as usize == *k)) {
        return Err(PalwGenClassErrorV1::Programs(format!("program {k} is run by no stage")));
    }

    // 3. One layout per stage: the stage's own context, one tile per commit point, one per state.
    if class.layouts.len() != pipeline.stages.len() {
        return Err(PalwGenClassErrorV1::Layout(format!("{} layouts for {} stages", class.layouts.len(), pipeline.stages.len())));
    }
    for (s, (st, layout)) in pipeline.stages.iter().zip(&class.layouts).enumerate() {
        let prog = &programs[st.program as usize];
        let commits = prog.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
        let what = format!("stage {s} ({})", st.name);
        if layout.max_context != st.max_trip {
            return Err(PalwGenClassErrorV1::Layout(format!(
                "{what}: max_context {} is not max_trip {}",
                layout.max_context, st.max_trip
            )));
        }
        if layout.commit_tiles.len() != commits || layout.state_tiles.len() != prog.states.len() {
            return Err(PalwGenClassErrorV1::Layout(format!(
                "{what}: {} commit tiles for {commits} commit points, {} state tiles for {} states",
                layout.commit_tiles.len(),
                layout.state_tiles.len(),
                prog.states.len()
            )));
        }
        if layout.commit_tiles.iter().chain(&layout.state_tiles).any(|t| *t == 0) || layout.checkpoint_interval == 0 {
            return Err(PalwGenClassErrorV1::Layout(format!("{what}: a zero tile or checkpoint interval")));
        }
    }

    // 4. The output header against the output stage (PALW-OUT-2) and its tile (PALW-OUT-3).
    let out_stage = &pipeline.stages[pipeline.output_stage as usize];
    let out_prog = &programs[out_stage.program as usize];
    let out = |m: String| Err(PalwGenClassErrorV1::Output(m));
    let kind = misaka_palw_gen::OutputKindV1::from_tag(class.output.kind);
    if kind != Some(profile.output_kind()) {
        return out(format!("kind {} is not {profile:?}'s {}", class.output.kind, profile.output_kind().name()));
    }
    let layout = class.output.layout().map_err(|e| PalwGenClassErrorV1::Output(format!("{e:?}")))?;
    // A text class's output stage is the text stage, and only a text class has one (NF-P9′). Its
    // output is the generated ids — `Tokens [max_trip]`, the most a stream holds — with no root.
    let text_stage = matches!(out_prog.output, OutputDecl::Logits { .. });
    if text_stage != (profile == PalwGenProfileV1::Text) {
        return out(format!("a {profile:?} class's output stage {} the text stage", if text_stage { "is" } else { "is not" }));
    }
    if text_stage {
        if class.output.shape != [out_stage.max_trip] {
            return out(format!("a text class's output is Tokens [{}], its stream's most ids", out_stage.max_trip));
        }
        check_offers(&pipeline, &programs, &class.offers)?;
        let admission = admit_class(class, ceilings)?;
        let image_token_floor = palw_gen_image_token_floor_v1(&pipeline, &admission, class.offers.images.len())?;
        for (i, (slot, floor)) in class.offers.images.iter().zip(&image_token_floor).enumerate() {
            if slot.token_equivalents < (*floor).max(1) {
                return Err(PalwGenClassErrorV1::Offers(format!(
                    "image slot {i} is priced at {} prompt tokens, below its floor {} (⌈per-image work / per-token work⌉)",
                    slot.token_equivalents,
                    (*floor).max(1)
                )));
            }
        }
        return Ok(PalwGenClassReportV1 {
            profile,
            admission,
            output_tile_len: None,
            draws_randomness: draws_randomness(&programs),
            image_token_floor,
        });
    }
    let post = &out_prog.blocks[out_prog.schedule.post as usize];
    let node = out_prog.output.node();
    let Some(node_shape) = fixed_shape(&post.nodes[node as usize].out.shape) else {
        return out("the output node's shape is not static".into());
    };
    let expected = match out_prog.output {
        OutputDecl::Final { .. } => node_shape,
        OutputDecl::Rows { .. } => [vec![out_stage.max_trip], node_shape].concat(),
        OutputDecl::Logits { .. } => unreachable!("the text stage returned above"),
    };
    if class.output.shape != expected {
        return out(format!("shape {:?} is not the output stage's {expected:?}", class.output.shape));
    }
    let interval =
        misaka_palw_tir::interval_v2::output_interval_v2(out_prog).map_err(|e| PalwGenClassErrorV1::Program(e.to_string()))?;
    if interval.lo < layout.lo as i128 || interval.hi > layout.hi as i128 {
        return out(format!(
            "the output node's proven interval [{}, {}] is not inside the kind's domain [{}, {}] (PALW-OUT-2)",
            interval.lo, interval.hi, layout.lo, layout.hi
        ));
    }
    let out_layout = &class.layouts[pipeline.output_stage as usize];
    let commit_index = out_prog
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(bi, b)| b.nodes.iter().enumerate().filter(|(_, n)| n.commit).map(move |(ni, _)| (bi, ni)))
        .position(|(bi, ni)| bi == out_prog.schedule.post as usize && ni == node as usize)
        .expect("the output node is committed (NF-28)");
    let output_tile_len = out_layout.commit_tiles[commit_index];
    if !(4..=1 << 16).contains(&output_tile_len) {
        return out(format!("the output node's tile {output_tile_len} is outside the digest's [4, 2^16]"));
    }
    // A `Rows` output's tiles are its rows' step tiles end to end: a row must be whole tiles.
    let row: u64 = post.nodes[node as usize].out.shape.iter().map(|d| if let Dim::Fixed(n) = d { *n as u64 } else { 0 }).product();
    if matches!(out_prog.output, OutputDecl::Rows { .. }) && row % output_tile_len as u64 != 0 {
        return out(format!("a row of {row} elements is not whole tiles of {output_tile_len} (PALW-OUT-3's alignment)"));
    }

    // 5. The offers against the bindings.
    check_offers(&pipeline, &programs, &class.offers)?;

    // 6. The IR's admission under the profile's ceilings, each stage at its widest commit tile.
    let admission = admit_class(class, ceilings)?;
    Ok(PalwGenClassReportV1 {
        profile,
        admission,
        output_tile_len: Some(output_tile_len),
        draws_randomness: draws_randomness(&programs),
        image_token_floor: Vec::new(),
    })
}

fn draws_randomness(programs: &[TirProgramV2]) -> bool {
    programs.iter().any(|p| p.inputs.iter().any(|i| matches!(i.source, InputSource::Random { .. })))
}

/// The IR's admission of the class's pipeline under the profile's ceilings, each stage at its widest
/// commit tile (a necessary condition on leaves: see the module doc).
fn admit_class(
    class: &PalwGenClassV1,
    ceilings: &crate::palw_gen_v1::PalwGenProfileCeilingsV1,
) -> Result<TirPipelineAdmissionV1, PalwGenClassErrorV1> {
    let stage_inputs: Vec<TirAdmitInputsV1> = class
        .layouts
        .iter()
        .map(|l| TirAdmitInputsV1 {
            tile_len: l.commit_tiles.iter().copied().max().unwrap_or(1),
            h_chunk: l.h_tile,
            ceilings: TirCeilingsV1 {
                max_tile_macs: u64::MAX,
                max_tile_transcendentals: u64::MAX,
                max_tile_opened_bytes: u64::MAX,
                max_tile_operands: u64::MAX,
                max_position_macs: ceilings.max_position_macs,
                max_position_transcendentals: u64::MAX,
                max_state_bytes: ceilings.max_state_bytes,
                max_step_leaves: ceilings.max_position_step_leaves,
                max_checkpoint_interval: l.checkpoint_interval,
                max_cone_work: 1 << 20,
            },
        })
        .collect();
    let job = TirJobCeilingsV1 {
        max_job_macs: ceilings.max_job_macs,
        max_job_transcendentals: ceilings.max_job_transcendentals,
        max_job_step_leaves: ceilings.max_job_step_leaves,
        max_job_cone_work: ceilings.max_job_cone_work,
    };
    tir_admit_pipeline_staged_v1(&class.pipeline, &class.programs, &stage_inputs, &job).map_err(|e| match e {
        TirAdmitError::Program(e) => PalwGenClassErrorV1::Program(e.to_string()),
        TirAdmitError::Exceeds { limit, at, value, cap } => PalwGenClassErrorV1::AdmissionExceeds { limit, at, value, cap },
        TirAdmitError::Inputs(why) => PalwGenClassErrorV1::Layout(why.into()),
    })
}

// ---------------------------------------------------------------------------------------------
// The registry row
// ---------------------------------------------------------------------------------------------

pub const PALW_GEN_CLASS_RECORD_VERSION_V1: u16 = 1;

/// **What the chain keeps of an admitted generative class** (the `gen_classes` table's row): the
/// facts a V5 job and a court read — the class id and its artifact root, the profile, the image
/// slots, the text stage's context — and the class itself, which every generative object that
/// names the class references instead of carrying.
///
/// **Rooted without the class's bytes**: [`Self::rooted_bytes_v1`] is every field but `class`, and the
/// class is committed through `pipeline_root`, `terms_digest` and the class id; a carriage whose
/// class does not hash to them is refused at load ([`Self::check_class_v1`]).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenClassRecordV1 {
    /// [`PALW_GEN_CLASS_RECORD_VERSION_V1`].
    pub version: u16,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub profile: u8,
    pub pipeline_root: Hash64,
    pub terms_digest: Hash64,
    pub tokenizer_id: Hash64,
    /// The image slots a V5 job's images are held to.
    pub images: Vec<PalwGenImageOfferV1>,
    /// The text stage's `max_trip` — a V5 job's `prompt + decode − 1` bound — for a text class.
    pub text_max_trip: Option<u32>,
    /// The class's carried bytes, counted.
    pub class_bytes: u64,
    /// **The dissected commit points** `(stage, block, node)`, in stage then block then node order:
    /// the commit points whose cone reduces over the history (spec 04b §9.5.1, each stage's view). A
    /// class with any owes the terminal move of its court (`PalwClassStateV2::fused_attention`): a
    /// root claim at a dissected leaf (RFC-0002 F7 composed), an acquitting close at any other.
    pub dissected: Vec<(u8, u8, u16)>,
    /// The class (rooted through the three hashes above, never by its bytes).
    pub class: std::sync::Arc<PalwGenClassV1>,
}

impl PalwGenClassRecordV1 {
    /// A self-consistent row over a class that decodes to nothing — for tests that need a row to
    /// exist (its hashes are its class's, so it passes the load check).
    #[cfg(test)]
    pub(crate) fn test_row_v1(seed: u8) -> Self {
        let class = PalwGenClassV1 {
            version: PALW_GEN_CLASS_VERSION_V1,
            profile: PalwGenProfileV1::Text as u8,
            pipeline: vec![seed],
            programs: Vec::new(),
            layouts: Vec::new(),
            output: OutputSpecV1::image_rgb8(1, 1),
            offers: PalwGenOffersV1 {
                steps: Vec::new(),
                scalars: Vec::new(),
                max_prompt_tokens: 1,
                max_negative_tokens: 0,
                images: Vec::new(),
            },
            tokenizer_id: Hash64::from_bytes([seed; 64]),
        };
        let artifact_root = Hash64::from_bytes([seed ^ 0x5A; 64]);
        Self {
            version: PALW_GEN_CLASS_RECORD_VERSION_V1,
            class_id: class.class_id(&artifact_root),
            artifact_root,
            profile: class.profile,
            pipeline_root: class.pipeline_root(),
            terms_digest: class.terms_digest(),
            tokenizer_id: class.tokenizer_id,
            images: Vec::new(),
            text_max_trip: Some(8),
            class_bytes: class.carried_bytes(),
            dissected: Vec::new(),
            class: std::sync::Arc::new(class),
        }
    }

    /// **The bytes the `gen_classes` root commits for this record**: every field but the class.
    pub fn rooted_bytes_v1(&self) -> Vec<u8> {
        borsh::to_vec(&(
            self.version,
            self.class_id,
            self.artifact_root,
            self.profile,
            self.pipeline_root,
            self.terms_digest,
            self.tokenizer_id,
            &self.images,
            self.text_max_trip,
            self.class_bytes,
            &self.dissected,
        ))
        .expect("a record is borsh-serializable")
    }

    /// **The load check**: the class hashes to the recorded roots and id and is its recorded size.
    pub fn check_class_v1(&self) -> Result<(), &'static str> {
        if self.class.carried_bytes() != self.class_bytes {
            return Err("a gen_classes row's class is not its recorded size");
        }
        if self.class.pipeline_root() != self.pipeline_root || self.class.terms_digest() != self.terms_digest {
            return Err("a gen_classes row's class does not hash to its recorded roots");
        }
        if self.class.class_id(&self.artifact_root) != self.class_id {
            return Err("a gen_classes row's class does not hash to its class id");
        }
        Ok(())
    }
}

/// **The row a registration writes**, derived from the carried class alone (the class decodes, and
/// its facts are read off it); `Err` when the class does not decode.
pub fn palw_gen_class_record_v1(class: &PalwGenClassV1, artifact_root: &Hash64) -> Result<PalwGenClassRecordV1, PalwGenClassErrorV1> {
    let (programs, pipeline) = class.decode().map_err(|e| PalwGenClassErrorV1::Program(e.to_string()))?;
    let out = &pipeline.stages[pipeline.output_stage as usize];
    let dissected = pipeline
        .stages
        .iter()
        .enumerate()
        .flat_map(|(s, st)| {
            let view = programs[st.program as usize].v1_view();
            crate::palw_tir_dissect_v1::palw_tir_dissected_commit_points_v1(&view).into_iter().map(move |(b, n)| (s as u8, b, n))
        })
        .collect();
    let text_max_trip = matches!(out.trip, TripRule::TextStream).then_some(out.max_trip);
    Ok(PalwGenClassRecordV1 {
        version: PALW_GEN_CLASS_RECORD_VERSION_V1,
        class_id: class.class_id(artifact_root),
        artifact_root: *artifact_root,
        profile: class.profile,
        pipeline_root: class.pipeline_root(),
        terms_digest: class.terms_digest(),
        tokenizer_id: class.tokenizer_id,
        images: class.offers.images.clone(),
        text_max_trip,
        class_bytes: class.carried_bytes(),
        dissected,
        class: std::sync::Arc::new(class.clone()),
    })
}
