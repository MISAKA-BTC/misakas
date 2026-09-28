//! **RFC-0003: the generative court's own questions** — what a stage's cone reads that no leaf
//! holds, what an edge between stages must satisfy, and whether a claim's output bytes are its
//! output node's committed tiles (spec 04b §15.4; RFC-0003 §I.1.7, §I.3, §II.1 5.8).
//!
//! The IR court (Phase F, [`crate::palw_tir_court_v1`]) adjudicates one committed leaf by demand
//! evaluation of its cone from leaves that precede it. A stage of a pipeline is a version-2 program,
//! whose evaluator (`misaka_palw_tir::demand_v2::eval_demanded_v2`) asks one question version 1 does
//! not — an input's element at a position. [`PalwGenStageSourceV1`] is the court's answer to it:
//!
//! * **a random input** is recomputed from the claim's job — `R(seed, domain, step, item, lane)`
//!   (RFC-0003 §I.1), `step` the stage position for a per-step domain and 0 otherwise, `item` the
//!   job's item index — never opened, never taken from either party (PALW-RND-2, PALW-RND-5);
//! * **a job-bound input** (a scalar, a token run, a token or row count) is the accepted job's value
//!   (`misaka_palw_tir::pipeline::stage_job_facts`): acceptance checked it, and nothing opens it;
//! * **a job image** (RFC-0003 §II.4) is the one job input the chain does not hold: the job carries its
//!   `input_root` and size, and the parties carry the input tiles a cone reads. Each carried tile is
//!   verified against the job's `input_root` before any lane of it is read
//!   ([`PalwGenStageSourceV1::carry_image_tile`]); a tile not proven under the root is refused — the
//!   evidence, not a party's guilt — and a lane whose tile is not carried fails the evaluation
//!   `Missing`;
//! * **an edge from an earlier stage** is the one input the parties' carriage answers. A `StageRows`
//!   element past the upstream's kept rows is the zero pad — a job fact, answered here. Every other
//!   element is an upstream committed value, and it must lie in the upstream output's proven
//!   interval: PALW-TIR-33 carried over to edges (PALW-GEN-7). The first that does not is recorded
//!   ([`PalwGenStageSourceV1::violation`]) and refused to the evaluator: the executor committed a
//!   value no execution produces, whichever leaf was disputed, and is convicted for it
//!   (`TirValueOutsideProvenInterval`) — never the challenger for a value the executor committed.
//!
//! **The output digest** (PALW-OUT-3, PALW-OUT-4). A claim's `output_root` is the tile-aligned root of
//! its output node's canonical bytes (`misaka_palw_gen::output`), each output tile the lanes of one
//! step tile of the output node ([`palw_gen_output_tile_of_v1`]). [`palw_gen_output_tile_check_v1`]
//! holds one output tile, proven under the root, against the committed step tile of the same lanes,
//! lane by lane: a lane outside the output node's proven interval is PALW-TIR-33's fault; a lane whose
//! canonical bytes differ from the tile's is `TirOutputDigestMismatch` (fault 21). Either way the
//! executor's own two statements convict it, with no recomputation. A tile not proven under the root
//! convicts nobody: the accusation's evidence is refused.
//!
//! Nothing here is reachable from a block yet: the pipeline admission, the generative step tree and
//! the close that carries these answers arrive with the fence's first armed release.

use crate::palw_step_leg::PalwStepFaultV1;
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::demand::{DemandContext, StateSupply};
use misaka_palw_tir::demand_v2::DemandSourceV2;
use misaka_palw_tir::pipeline::{Binding, StageJobFacts, TirPipelineV1};
use misaka_palw_tir::program_v2::{InputSource, OutputDecl, RandomDist, TirProgramV2};
use misaka_palw_tir::{TirError, TirErrorKind, TirResult};

/// `R`'s job coordinates (RFC-0003 §I.1.3): the claim's seed and its item index (`R`'s `position`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGenDrawV1 {
    pub seed: [u8; 32],
    pub item_index: u32,
}

/// **Lane `lane` of a random input at stage position `pos`**, recomputed from the job: the domain's
/// word at `(seed, step, item, lane)`, then `Uniform`'s word or `Normal`'s table entry (PALW-RND-4).
pub fn palw_gen_random_element_v1(
    domain: u16,
    dist: RandomDist,
    per_step: bool,
    draw: &PalwGenDrawV1,
    pos: u32,
    lane: u64,
) -> Result<i128, misaka_palw_gen::RandErrorV1> {
    let step = if per_step { pos } else { 0 };
    let word = misaka_palw_gen::rand_word_v1(domain, &draw.seed, step, draw.item_index, lane)?;
    Ok(match dist {
        RandomDist::Uniform { .. } => word as i128,
        RandomDist::Normal => misaka_palw_gen::gauss_q24_v1(word as u16) as i128,
    })
}

/// **A job image as the court knows it**: the job's `input_root` and size (`PalwGenImageInputRefV1`)
/// and the class slot's input tile length (`PalwGenImageOfferV1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGenImageRefV1 {
    pub input_root: [u8; 64],
    pub h: u32,
    pub w: u32,
    pub tile_len: u32,
}

impl PalwGenImageRefV1 {
    pub fn of(image: &crate::palw_gen_class_v1::PalwGenImageInputRefV1, slot: &crate::palw_gen_class_v1::PalwGenImageOfferV1) -> Self {
        let mut input_root = [0u8; 64];
        input_root.copy_from_slice(image.input_root.as_byte_slice());
        Self { input_root, h: image.h, w: image.w, tile_len: slot.tile_len }
    }
}

/// Why a carried image tile is refused: the evidence fails, and nobody is convicted for it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwGenImageRefusalV1 {
    #[error("the job has no image {0}")]
    UnknownImage(u8),
    #[error("image {image}'s tile {tile} is not proven under the job's input root")]
    TileNotProven { image: u8, tile: u64 },
}

/// How the court answers one input of a stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwGenInputAnswerV1 {
    /// Recomputed from the job's draw.
    Random { domain: u16, dist: RandomDist, per_step: bool },
    /// The accepted job's value, element by element.
    Job(Vec<i128>),
    /// An earlier stage's committed output: elements `0..kept` carried, each within `[lo, hi]` (the
    /// upstream output's proven interval); every later element the zero pad.
    Edge { lo: i128, hi: i128, kept: u64 },
    /// Job image `image`: every element a byte of a carried tile proven under its `input_root`.
    Image { image: u8 },
}

/// **How the court answers every input of stage `stage`** of a job whose facts are `facts`
/// (`stage_job_facts` of the same job) and whose images are `images`. `programs` and `pipeline` must
/// be a decoded class's.
pub fn palw_gen_stage_answers_v1(
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
    stage: usize,
    facts: &[StageJobFacts],
    images: &[PalwGenImageRefV1],
) -> TirResult<Vec<PalwGenInputAnswerV1>> {
    let missing = |m: String| TirError::new(TirErrorKind::Missing, m);
    let st = pipeline.stages.get(stage).ok_or_else(|| missing(format!("no stage {stage}")))?;
    let prog = &programs[st.program as usize];
    let mut bindings = st.bind.iter();
    let mut answers = Vec::with_capacity(prog.inputs.len());
    for (k, d) in prog.inputs.iter().enumerate() {
        let answer = match d.source {
            InputSource::Random { domain, dist, per_step } => PalwGenInputAnswerV1::Random { domain, dist, per_step },
            InputSource::External { .. } => match bindings.next().ok_or_else(|| missing("a binding per external input".into()))? {
                Binding::StageRows { stage: up, drop, .. } => {
                    let up_prog = &programs[pipeline.stages[*up as usize].program as usize];
                    let iv = misaka_palw_tir::interval_v2::output_interval_v2(up_prog)?;
                    let per_row: u64 = d.shape[1..].iter().map(|x| *x as u64).product();
                    // A decode stage's rows start at its first consumed position (RFC-0004 §7.2).
                    let up_facts = facts.get(*up as usize).ok_or_else(|| missing(format!("no facts of stage {up}")))?;
                    let rows = up_facts.trip.saturating_sub(up_facts.rows_from).saturating_sub(*drop);
                    PalwGenInputAnswerV1::Edge { lo: iv.lo, hi: iv.hi, kept: rows as u64 * per_row }
                }
                Binding::StageFinal { stage: up } => {
                    let up_prog = &programs[pipeline.stages[*up as usize].program as usize];
                    let iv = misaka_palw_tir::interval_v2::output_interval_v2(up_prog)?;
                    PalwGenInputAnswerV1::Edge { lo: iv.lo, hi: iv.hi, kept: d.shape.iter().map(|x| *x as u64).product() }
                }
                Binding::JobImage { index } => {
                    let image = images.get(*index as usize).ok_or_else(|| missing(format!("the job has no image {index}")))?;
                    if d.shape[..] != [image.h, image.w, 3] {
                        return Err(missing(format!(
                            "the job's image {index} is {}×{}; input {k} is {:?}",
                            image.h, image.w, d.shape
                        )));
                    }
                    PalwGenInputAnswerV1::Image { image: *index }
                }
                _ => {
                    let facts = facts.get(stage).ok_or_else(|| missing(format!("no facts of stage {stage}")))?;
                    let value = facts.inputs.get(&(k as u16)).ok_or_else(|| missing(format!("the job does not fix input {k}")))?;
                    PalwGenInputAnswerV1::Job(value.data.clone())
                }
            },
        };
        answers.push(answer);
    }
    Ok(answers)
}

/// **The court's source for one stage** — the parties' carriage (`inner`: leaves, params, states,
/// histories, tokens, and the carried edge values) with the court's own answers to every input it
/// derives (see the module doc).
pub struct PalwGenStageSourceV1<'a> {
    inner: &'a mut dyn DemandSourceV2,
    answers: Vec<PalwGenInputAnswerV1>,
    draw: PalwGenDrawV1,
    /// The job's images, by index.
    images: Vec<PalwGenImageRefV1>,
    /// Carried image tiles already proven under their image's `input_root`: `(image, tile) → bytes`.
    image_tiles: std::collections::BTreeMap<(u8, u64), Vec<u8>>,
    /// The first carried edge value outside its upstream's proven interval: `(input, pos, index,
    /// value)` — the executor's PALW-TIR-33 fault.
    pub violation: Option<(u16, u32, usize, i128)>,
    /// Every image tile a lane was read from: what a builder carries.
    pub used_image_tiles: std::collections::BTreeSet<(u8, u64)>,
}

impl<'a> PalwGenStageSourceV1<'a> {
    pub fn new(inner: &'a mut dyn DemandSourceV2, answers: Vec<PalwGenInputAnswerV1>, draw: PalwGenDrawV1) -> Self {
        Self {
            inner,
            answers,
            draw,
            images: Vec::new(),
            image_tiles: Default::default(),
            violation: None,
            used_image_tiles: Default::default(),
        }
    }

    /// The job's images (their roots, sizes and the class's tile lengths), for the image answers.
    pub fn with_images(mut self, images: Vec<PalwGenImageRefV1>) -> Self {
        self.images = images;
        self
    }

    /// **Carry one input tile of job image `image`**: verified against the job's `input_root` (the
    /// image's `ImageRgb8` header at the class's tile length) before any lane of it can be read.
    pub fn carry_image_tile(&mut self, image: u8, tile: u64, bytes: &[u8], proof: &[[u8; 64]]) -> Result<(), PalwGenImageRefusalV1> {
        let r = self.images.get(image as usize).ok_or(PalwGenImageRefusalV1::UnknownImage(image))?;
        let spec = misaka_palw_gen::output::input_image_spec_v1(r.h, r.w);
        if !misaka_palw_gen::output::verify_output_tile_v1(&r.input_root, &spec, r.tile_len, tile, bytes, proof) {
            return Err(PalwGenImageRefusalV1::TileNotProven { image, tile });
        }
        self.image_tiles.insert((image, tile), bytes.to_vec());
        Ok(())
    }
}

impl DemandSourceV2 for PalwGenStageSourceV1<'_> {
    fn node(&mut self, ctx: DemandContext, node: u16, index: usize) -> TirResult<i128> {
        self.inner.node(ctx, node, index)
    }
    fn param(&mut self, param: u16, layer: Option<u16>, index: usize) -> TirResult<i128> {
        self.inner.param(param, layer, index)
    }
    fn input(&mut self, pos: u32, input: u16, index: usize) -> TirResult<i128> {
        let answer =
            self.answers.get(input as usize).ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("no input {input}")))?;
        match answer {
            PalwGenInputAnswerV1::Random { domain, dist, per_step } => {
                palw_gen_random_element_v1(*domain, *dist, *per_step, &self.draw, pos, index as u64)
                    .map_err(|e| TirError::new(TirErrorKind::Missing, format!("R refused input {input}: {e:?}")))
            }
            PalwGenInputAnswerV1::Job(values) => values
                .get(index)
                .copied()
                .ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("input {input} has no element {index}"))),
            PalwGenInputAnswerV1::Image { image } => {
                let image = *image;
                let tile_len = self
                    .images
                    .get(image as usize)
                    .ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("the job has no image {image}")))?
                    .tile_len as u64;
                let (tile, lane) = (index as u64 / tile_len, (index as u64 % tile_len) as usize);
                self.used_image_tiles.insert((image, tile));
                self.image_tiles
                    .get(&(image, tile))
                    .and_then(|b| b.get(lane))
                    .map(|b| *b as i128)
                    .ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("image {image}'s tile {tile} is not carried")))
            }
            PalwGenInputAnswerV1::Edge { lo, hi, kept } => {
                if index as u64 >= *kept {
                    return Ok(0);
                }
                let (lo, hi) = (*lo, *hi);
                let v = self.inner.input(pos, input, index)?;
                if v < lo || v > hi {
                    self.violation.get_or_insert((input, pos, index, v));
                    return Err(TirError::new(
                        TirErrorKind::Operand,
                        format!("input {input} element {index}: a carried {v} outside the upstream's [{lo}, {hi}] (PALW-TIR-33)"),
                    ));
                }
                Ok(v)
            }
        }
    }
    fn state(&mut self, pos: u32, state: u16, layer: Option<u16>, index: usize) -> TirResult<StateSupply> {
        self.inner.state(pos, state, layer, index)
    }
    fn hist_row(&mut self, pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> TirResult<i128> {
        self.inner.hist_row(pos, state, layer, row_pos, index)
    }
    fn token(&mut self, pos: u32) -> TirResult<u32> {
        self.inner.token(pos)
    }
}

/// **Which output tile a step tile of the output node is** (PALW-OUT-3's alignment): a `Final`
/// stage's step tile `t` at its last position (`trip − 1`) is output tile `t`; a `Rows` stage's step
/// tile `t` at position `r` is output tile `r · (row / tile_len) + t` (the preflight requires a row to
/// be whole tiles). `None` for a step tile no output tile holds.
pub fn palw_gen_output_tile_of_v1(
    output: &OutputDecl,
    row_elements: u64,
    tile_len: u32,
    trip: u32,
    pos: u32,
    step_tile: u64,
) -> Option<u64> {
    let per_row = row_elements.div_ceil(tile_len as u64);
    if step_tile >= per_row || pos >= trip {
        return None;
    }
    match output {
        OutputDecl::Final { .. } => (pos + 1 == trip).then_some(step_tile),
        OutputDecl::Rows { .. } => (row_elements % tile_len as u64 == 0).then_some(pos as u64 * per_row + step_tile),
        OutputDecl::Logits { .. } => None,
    }
}

/// Why an output-tile accusation convicts nobody.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwGenOutputRefusalV1 {
    #[error("the output tile is not proven under the claim's output root")]
    TileNotProven,
    #[error("the step tile carries {got} lanes and the output tile {want}")]
    LaneCount { want: u64, got: u64 },
}

/// **One output tile against the output node's committed step tile of the same lanes**
/// (PALW-OUT-4): `Ok(None)` when every lane agrees; `Ok(Some(fault))` at the first lane that does not —
/// a lane outside `interval` (the output node's proven interval, inside the kind's domain by
/// PALW-OUT-2) is `TirValueOutsideProvenInterval`, a lane whose canonical bytes differ from the
/// tile's is `TirOutputDigestMismatch`, both at the lane's index in the tile; `Err` when the
/// evidence itself fails (the tile is not under `output_root`, or the lanes are not the tile's).
/// `step_lanes` must already be authenticated against the claim's step root by the caller.
#[allow(clippy::too_many_arguments)]
pub fn palw_gen_output_tile_check_v1(
    spec: &OutputSpecV1,
    output_root: &[u8; 64],
    tile_len: u32,
    tile: u64,
    interval: (i128, i128),
    step_lanes: &[i128],
    output_tile: &[u8],
    proof: &[[u8; 64]],
) -> Result<Option<PalwStepFaultV1>, PalwGenOutputRefusalV1> {
    if !misaka_palw_gen::output::verify_output_tile_v1(output_root, spec, tile_len, tile, output_tile, proof) {
        return Err(PalwGenOutputRefusalV1::TileNotProven);
    }
    let element_bytes = spec.layout().map_err(|_| PalwGenOutputRefusalV1::TileNotProven)?.element_bytes;
    let want = (output_tile.len() / element_bytes) as u64;
    if step_lanes.len() as u64 != want {
        return Err(PalwGenOutputRefusalV1::LaneCount { want, got: step_lanes.len() as u64 });
    }
    for (i, v) in step_lanes.iter().enumerate() {
        let value_index = i as u32;
        if *v < interval.0 || *v > interval.1 {
            return Ok(Some(PalwStepFaultV1::TirValueOutsideProvenInterval { value_index }));
        }
        // Inside the proven interval, so inside the kind's domain (PALW-OUT-2): the lane encodes.
        let Ok(bytes) = spec.lane_bytes(&[*v as i64]) else {
            return Ok(Some(PalwStepFaultV1::TirValueOutsideProvenInterval { value_index }));
        };
        if output_tile[i * element_bytes..(i + 1) * element_bytes] != bytes[..] {
            return Ok(Some(PalwStepFaultV1::TirOutputDigestMismatch { value_index }));
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// The court composition across stages (RFC-0003 §II.2.1: one step tree, stage-major)
// ---------------------------------------------------------------------------------------------

use crate::palw_gen_step_v1::{
    PalwGenLeafCoordV1, PalwGenLeafKindV1, PalwGenOpenedLeafV1, PalwGenStepSpaceV1, palw_gen_verify_leaf_v1,
};
use crate::palw_gen_worker_v1::{PalwGenClaimRootsV1, PalwGenDecodeV1};
use misaka_palw_tir::demand::{DemandError, DemandLimits, DemandRequest, DemandTarget, hist_row_node_v1};
use misaka_palw_tir::demand_v2::eval_demanded_v2;
use misaka_palw_tir::interval::Interval;
use misaka_palw_tir::program::StateKind;

/// One carried input tile of a job image, with its path under the job's `input_root`.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenImageTileV1 {
    pub image: u8,
    pub tile: u64,
    pub bytes: Vec<u8>,
    pub proof: Vec<[u8; 64]>,
}

/// **A generative close**: the disputed leaf and every unit its cone reads, opened — leaves of the
/// disputed stage that precede it, leaves of earlier stages (the edges), job-image tiles, and the
/// class's param leaves (whole leaves of its pipeline inventory, ascending, each with its path to
/// the class's `artifact_root`: [`crate::palw_gen_artifact_v1`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwGenCloseV1 {
    pub disputed: PalwGenOpenedLeafV1,
    pub operands: Vec<PalwGenOpenedLeafV1>,
    pub image_tiles: Vec<PalwGenImageTileV1>,
    pub params: Vec<crate::palw_artifact::PalwArtifactOpeningV1>,
}

/// **What the court holds of a claim**: its class (the space, the pipeline, the programs, the
/// class's `artifact_root` and its pipeline inventory's index — the params themselves are the
/// close's, authenticated against the root), the job's facts (its prompt and committed ids, never an
/// image's bytes), its images' roots, its draw, and the claim's roots.
pub struct PalwGenCourtCaseV1<'a> {
    pub space: &'a PalwGenStepSpaceV1,
    pub pipeline: &'a TirPipelineV1,
    pub programs: &'a [TirProgramV2],
    pub artifact_root: crate::Hash64,
    pub inventory: &'a crate::palw_gen_artifact_v1::PalwGenInventoryIndexV1,
    pub facts: &'a [StageJobFacts],
    pub images: &'a [PalwGenImageRefV1],
    pub draw: PalwGenDrawV1,
    pub claim: &'a PalwGenClaimRootsV1,
}

/// **A generative close's verdict.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwGenVerdictV1 {
    /// The disputed leaf is what its cone computes from the leaves before it: the accusation fails.
    Acquitted,
    /// The executor's commitments convict it: `fault` at `leaf`.
    Convicted { leaf: PalwGenLeafCoordV1, fault: PalwStepFaultV1 },
}

/// Why a close convicts nobody: the evidence fails.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwGenCloseRefusalV1 {
    #[error("the claim's step root does not bind its stage roots")]
    RootsNotBound,
    #[error("leaf {0:?} is not under its stage's root")]
    LeafNotProven(PalwGenLeafCoordV1),
    #[error("leaf {0:?} does not precede the disputed leaf")]
    NotPreceding(PalwGenLeafCoordV1),
    #[error(transparent)]
    Image(PalwGenImageRefusalV1),
    #[error(transparent)]
    Params(crate::palw_gen_artifact_v1::PalwGenParamRefusalV1),
    #[error("the evaluation reads a unit the close does not carry: {0}")]
    Incomplete(String),
    #[error("the close is unadjudicable: {0}")]
    Unadjudicable(String),
}

/// Stage `stage`'s external input `input`'s binding.
fn binding_of<'b>(case: &'b PalwGenCourtCaseV1<'_>, stage: usize, input: u16) -> Option<&'b Binding> {
    let st = &case.pipeline.stages[stage];
    let prog = &case.programs[st.program as usize];
    let k = prog.inputs.iter().take(input as usize + 1).filter(|d| d.is_external()).count().checked_sub(1)?;
    prog.inputs.get(input as usize).filter(|d| d.is_external())?;
    st.bind.get(k)
}

/// **The upstream leaf an edge element reads**: `(stage, leaf index, lane)`.
fn edge_leaf(case: &PalwGenCourtCaseV1<'_>, stage: usize, input: u16, index: usize) -> Option<(u8, u64, usize)> {
    let (up, pos, element) = match binding_of(case, stage, input)? {
        Binding::StageFinal { stage: u } => {
            let up = &case.space.stages[*u as usize];
            (*u, up.trip.checked_sub(1)?, index as u64)
        }
        Binding::StageRows { stage: u, drop, .. } => {
            let prog = &case.programs[case.pipeline.stages[stage].program as usize];
            let row: u64 = prog.inputs[input as usize].shape[1..].iter().map(|d| *d as u64).product();
            // A decode stage's rows are its consumed logits rows (RFC-0004 §7.2).
            let from = case.space.stages[*u as usize].consumed_from.unwrap_or(0);
            (*u, (index as u64 / row) as u32 + drop + from, index as u64 % row)
        }
        _ => return None,
    };
    let up_space = &case.space.stages[up as usize];
    let up_prog = &case.programs[case.pipeline.stages[up as usize].program as usize];
    let post_occ = (up_prog.occurrences().len() - 1) as u16;
    let (leaf, lane) = up_space.commit_leaf_of(DemandContext { pos, occurrence: post_occ }, up_prog.output.node(), element)?;
    Some((up, leaf, lane))
}

/// The court's source for one stage: the carried, authenticated leaves and params.
struct CarriageSource<'a, 'c> {
    case: &'a PalwGenCourtCaseV1<'c>,
    stage: usize,
    leaves: &'a std::collections::BTreeMap<(u8, u64), Vec<i128>>,
    params: &'a crate::palw_gen_artifact_v1::PalwGenOpenedParamsV1,
    /// Every leaf and every param leaf the evaluation read: what a builder carries.
    used: PalwGenUsedUnitsV1,
}

/// **The units an evaluation read** — the leaves by `(stage, index)`, the class's param leaves and
/// the job's image tiles: exactly what a close must carry for the court to repeat it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwGenUsedUnitsV1 {
    pub leaves: std::collections::BTreeSet<(u8, u64)>,
    pub params: std::collections::BTreeSet<u32>,
    pub image_tiles: std::collections::BTreeSet<(u8, u64)>,
}

fn missing(m: String) -> TirError {
    TirError::new(TirErrorKind::Missing, m)
}

impl DemandSourceV2 for CarriageSource<'_, '_> {
    fn node(&mut self, ctx: DemandContext, node: u16, index: usize) -> TirResult<i128> {
        let sp = &self.case.space.stages[self.stage];
        let (leaf, lane) = sp
            .commit_leaf_of(ctx, node, index as u64)
            .ok_or_else(|| TirError::new(TirErrorKind::Malformed, format!("node {node} at {ctx:?} is no commit leaf")))?;
        self.used.leaves.insert((self.stage as u8, leaf));
        self.leaves.get(&(self.stage as u8, leaf)).and_then(|v| v.get(lane)).copied().ok_or_else(|| missing(format!("leaf {leaf}")))
    }
    fn param(&mut self, param: u16, layer: Option<u16>, index: usize) -> TirResult<i128> {
        let program = self.case.pipeline.stages[self.stage].program;
        if let Some(leaf) = self.params.leaf_of_element(self.case.inventory, program, param, layer, index) {
            self.used.params.insert(leaf);
        }
        match self.params.element(self.case.inventory, program, param, layer, index) {
            Ok(Some(v)) => Ok(v),
            Ok(None) => Err(missing(format!("program {program} param {param} layer {layer:?} element {index}"))),
            Err(e) => Err(TirError::new(TirErrorKind::Malformed, e)),
        }
    }
    fn input(&mut self, _pos: u32, input: u16, index: usize) -> TirResult<i128> {
        let (up, leaf, lane) = edge_leaf(self.case, self.stage, input, index)
            .ok_or_else(|| missing(format!("input {input} is no edge the court reads")))?;
        self.used.leaves.insert((up, leaf));
        self.leaves.get(&(up, leaf)).and_then(|v| v.get(lane)).copied().ok_or_else(|| missing(format!("stage {up} leaf {leaf}")))
    }
    fn state(&mut self, pos: u32, state: u16, layer: Option<u16>, index: usize) -> TirResult<StateSupply> {
        if pos == 0 {
            return Ok(StateSupply::Value(0));
        }
        let sp = &self.case.space.stages[self.stage];
        if !pos.is_multiple_of(sp.layout.checkpoint_interval) {
            return Ok(StateSupply::Replay);
        }
        let tile = *sp.layout.state_tiles.get(state as usize).ok_or_else(|| missing(format!("state {state}")))? as usize;
        let coord = PalwGenLeafCoordV1 {
            stage: self.stage as u8,
            pos: pos - 1,
            kind: PalwGenLeafKindV1::State { state, layer },
            tile: (index / tile) as u32,
        };
        let leaf = sp.leaf_index(&coord).ok_or_else(|| missing(format!("{coord:?}")))?;
        self.used.leaves.insert((self.stage as u8, leaf));
        self.leaves
            .get(&(self.stage as u8, leaf))
            .and_then(|v| v.get(index % tile))
            .copied()
            .map(StateSupply::Value)
            .ok_or_else(|| missing(format!("leaf {leaf}")))
    }
    fn hist_row(&mut self, _pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> TirResult<i128> {
        let view = &self.case.space.stages[self.stage].info.view;
        let (ctx, node) = hist_row_node_v1(view, state, layer, row_pos)
            .ok_or_else(|| TirError::new(TirErrorKind::Malformed, format!("history {state} appends nothing")))?;
        self.node(ctx, node, index)
    }
    fn token(&mut self, pos: u32) -> TirResult<u32> {
        self.case.facts[self.stage].tokens.get(pos as usize).copied().ok_or_else(|| missing(format!("the token at {pos}")))
    }
}

/// The interval a leaf's every lane must lie in (PALW-TIR-33): its node's proven interval, or its
/// `Fixed` state's declared range.
fn leaf_interval(case: &PalwGenCourtCaseV1<'_>, coord: &PalwGenLeafCoordV1) -> Option<Interval> {
    let sp = &case.space.stages[coord.stage as usize];
    match coord.kind {
        PalwGenLeafKindV1::Commit { occurrence, node } => {
            let block = sp.occurrence_block(occurrence)?;
            let ranges = misaka_palw_tir::interval_v2::analyze_ranges_v2(&sp.program).ok()?;
            ranges.get(block as usize)?.get(node as usize).copied()
        }
        PalwGenLeafKindV1::State { state, .. } => match sp.program.states.get(state as usize)?.kind {
            StateKind::Fixed { lo, hi } => Some(Interval::new(lo as i128, hi as i128)),
            StateKind::Hist { .. } => None,
        },
    }
}

fn first_outside(values: &[i128], iv: Interval) -> Option<u32> {
    values.iter().position(|v| *v < iv.lo || *v > iv.hi).map(|i| i as u32)
}

/// **Adjudicate one leaf of a pipeline claim** — the composition of the IR court across stages:
///
/// 1. the claim's step root binds its stage roots, and every carried leaf is under its stage's root;
///    a leaf of the disputed stage precedes the disputed one, and a leaf of another stage is an
///    earlier stage's (every earlier stage precedes, PALW-GEN-3);
/// 2. **PALW-TIR-33** on the disputed leaf and then on every carried leaf, in (stage, index) order:
///    the first lane outside its node's proven interval convicts;
/// 3. the class's param leaves, verified against its `artifact_root` (the pipeline inventory), and the
///    image tiles, verified against the job's `input_root`s (a failure refuses the close);
/// 4. the disputed leaf's elements re-evaluated from the carried leaves alone (spec 04b §15.4): its
///    own stage's earlier leaves, the class's params from the proven leaves, `R` recomputed, the job's
///    facts, edges read from the earlier stages' leaves (PALW-TIR-33 again, as read), image lanes from
///    the proven tiles;
/// 5. the first lane that differs convicts (`ComputationMismatch`); none acquits.
///
/// A unit the evaluation reads and the close does not carry refuses the close (`Incomplete`); an
/// evaluation that fails on authenticated units is an interpreter defect (`Unadjudicable`), which
/// convicts nobody.
pub fn palw_gen_adjudicate_leaf_v1(
    case: &PalwGenCourtCaseV1<'_>,
    close: &PalwGenCloseV1,
    limits: &DemandLimits,
) -> Result<PalwGenVerdictV1, PalwGenCloseRefusalV1> {
    let p = match prove_close(case, close)? {
        ProvedV1::Convicted(verdict) => return Ok(verdict),
        ProvedV1::Proven(p) => p,
    };
    let d = &close.disputed;
    let sp = &case.space.stages[p.s];
    let leaf = sp.leaves()[p.before as usize];
    let elements: Vec<usize> = (leaf.first_element..leaf.first_element + leaf.value_count as u64).map(|e| e as usize).collect();
    let target = match d.coord.kind {
        PalwGenLeafKindV1::Commit { occurrence, node } => {
            DemandTarget::Node { ctx: DemandContext { pos: d.coord.pos, occurrence }, node }
        }
        PalwGenLeafKindV1::State { state, layer } => DemandTarget::StateAfter { pos: d.coord.pos, state, layer },
    };
    let values = match evaluate_with(case, close, &p, |source| {
        eval_demanded_v2(&sp.program, &sp.info, &DemandRequest { target, elements: &elements }, source, limits).map(|(v, _)| v)
    })? {
        Ok(values) => values,
        Err(verdict) => return Ok(verdict),
    };
    match values.iter().zip(&d.values).position(|(a, b)| a != b) {
        Some(i) => {
            Ok(PalwGenVerdictV1::Convicted { leaf: d.coord, fault: PalwStepFaultV1::ComputationMismatch { value_index: i as u32 } })
        }
        None => Ok(PalwGenVerdictV1::Acquitted),
    }
}

/// A close whose carriage is proven: the disputed leaf's stage and index, the carried leaves by
/// `(stage, index)`, the class's params and the stage's input answers.
struct ProvenCloseV1 {
    s: usize,
    before: u64,
    leaves: std::collections::BTreeMap<(u8, u64), Vec<i128>>,
    params: crate::palw_gen_artifact_v1::PalwGenOpenedParamsV1,
    answers: Vec<PalwGenInputAnswerV1>,
}

enum ProvedV1 {
    Proven(ProvenCloseV1),
    /// A carried leaf's own lanes convict (PALW-TIR-33), before anything is evaluated.
    Convicted(PalwGenVerdictV1),
}

/// **Steps 1–3 of every generative close** (see [`palw_gen_adjudicate_leaf_v1`]): the roots bound,
/// every carried leaf under its stage's root and preceding the disputed one, PALW-TIR-33 on each
/// (a conviction), the stage's inputs as the court answers them, and the params authenticated.
fn prove_close(case: &PalwGenCourtCaseV1<'_>, close: &PalwGenCloseV1) -> Result<ProvedV1, PalwGenCloseRefusalV1> {
    use PalwGenCloseRefusalV1 as R;
    if crate::palw_gen_step_v1::palw_gen_step_root_v1(&case.claim.stage_roots) != case.claim.step_root
        || case.claim.stage_roots.len() != case.space.stages.len()
    {
        return Err(R::RootsNotBound);
    }
    let d = &close.disputed;
    let s = d.coord.stage as usize;
    let sp = case.space.stages.get(s).ok_or(R::LeafNotProven(d.coord))?;
    if !palw_gen_verify_leaf_v1(sp, &case.claim.stage_roots[s], d) {
        return Err(R::LeafNotProven(d.coord));
    }
    let before = sp.leaf_index(&d.coord).expect("verified");
    let mut leaves = std::collections::BTreeMap::new();
    for o in &close.operands {
        let os = o.coord.stage as usize;
        let osp = case.space.stages.get(os).ok_or(R::LeafNotProven(o.coord))?;
        if !palw_gen_verify_leaf_v1(osp, &case.claim.stage_roots[os], o) {
            return Err(R::LeafNotProven(o.coord));
        }
        let index = osp.leaf_index(&o.coord).expect("verified");
        if os > s || (os == s && index >= before) {
            return Err(R::NotPreceding(o.coord));
        }
        leaves.insert((o.coord.stage, index), o.values.clone());
    }
    // PALW-TIR-33: the disputed leaf first, then every carried leaf in (stage, index) order.
    let fault = |i| PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: i };
    if let Some(iv) = leaf_interval(case, &d.coord)
        && let Some(i) = first_outside(&d.values, iv)
    {
        return Ok(ProvedV1::Convicted(PalwGenVerdictV1::Convicted { leaf: d.coord, fault: fault(i) }));
    }
    for ((stage, index), values) in &leaves {
        let coord = case.space.stages[*stage as usize].leaves()[*index as usize].coord;
        if let Some(iv) = leaf_interval(case, &coord)
            && let Some(i) = first_outside(values, iv)
        {
            return Ok(ProvedV1::Convicted(PalwGenVerdictV1::Convicted { leaf: coord, fault: fault(i) }));
        }
    }
    // The stage's inputs as the court answers them.
    let answers = palw_gen_stage_answers_v1(case.pipeline, case.programs, s, case.facts, case.images)
        .map_err(|e| R::Incomplete(format!("the job does not fix the stage's inputs: {e}")))?;
    // The class's params: the carried inventory leaves, each proven under its artifact root.
    let params = crate::palw_gen_artifact_v1::PalwGenOpenedParamsV1::authenticate(case.inventory, &case.artifact_root, &close.params)
        .map_err(R::Params)?;
    Ok(ProvedV1::Proven(ProvenCloseV1 { s, before, leaves, params, answers }))
}

/// **Step 4's source**: the proven carriage, the answers and `R`, the image tiles verified under
/// their roots — handed to `run`. An edge value read outside its upstream's interval convicts the
/// upstream leaf (`Ok(Err(verdict))`); a unit the evaluation reads and the close does not carry
/// refuses the close (`Incomplete`); any other evaluation failure is `Unadjudicable`.
fn evaluate_with<T>(
    case: &PalwGenCourtCaseV1<'_>,
    close: &PalwGenCloseV1,
    p: &ProvenCloseV1,
    run: impl FnOnce(&mut dyn DemandSourceV2) -> Result<T, DemandError>,
) -> Result<Result<T, PalwGenVerdictV1>, PalwGenCloseRefusalV1> {
    evaluate_recording(case, close, p, run).map(|(out, _)| out)
}

/// [`evaluate_with`], and the units the evaluation read.
fn evaluate_recording<T>(
    case: &PalwGenCourtCaseV1<'_>,
    close: &PalwGenCloseV1,
    p: &ProvenCloseV1,
    run: impl FnOnce(&mut dyn DemandSourceV2) -> Result<T, DemandError>,
) -> Result<(Result<T, PalwGenVerdictV1>, PalwGenUsedUnitsV1), PalwGenCloseRefusalV1> {
    use PalwGenCloseRefusalV1 as R;
    let mut carriage = CarriageSource { case, stage: p.s, leaves: &p.leaves, params: &p.params, used: Default::default() };
    let mut source = PalwGenStageSourceV1::new(&mut carriage, p.answers.clone(), case.draw).with_images(case.images.to_vec());
    for t in &close.image_tiles {
        source.carry_image_tile(t.image, t.tile, &t.bytes, &t.proof).map_err(R::Image)?;
    }
    let out = run(&mut source);
    let violation = source.violation;
    let image_tiles = std::mem::take(&mut source.used_image_tiles);
    drop(source);
    let mut used = std::mem::take(&mut carriage.used);
    used.image_tiles = image_tiles;
    if let Some((input, _pos, index, _value)) = violation {
        // An edge value read outside its upstream's interval — carried leaves were all checked
        // already, so this names the upstream leaf the court read.
        if let Some((up, leaf, lane)) = edge_leaf(case, p.s, input, index) {
            let coord = case.space.stages[up as usize].leaves()[leaf as usize].coord;
            let fault = PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: lane as u32 };
            return Ok((Err(PalwGenVerdictV1::Convicted { leaf: coord, fault }), used));
        }
    }
    match out {
        Ok(v) => Ok((Ok(v), used)),
        Err(DemandError::Tir(t)) if t.kind == TirErrorKind::Missing => Err(R::Incomplete(t.to_string())),
        Err(e) => Err(R::Unadjudicable(format!("{e:?}"))),
    }
}

/// **The decode-token door of a pipeline claim's text stage** (RFC-0001 §A.3 over the committed
/// logits, RFC-0003 §II.2.1): generated id `t` must be the lane the V4 decode selects from the
/// committed logits row of position `|prompt| − 1 + t` with the committed ids before it. `row` is
/// every tile of that row, opened under the text stage's root.
pub fn palw_gen_decode_door_v1(
    case: &PalwGenCourtCaseV1<'_>,
    decode: &PalwGenDecodeV1,
    prompt_len: u32,
    t: u32,
    row: &[PalwGenOpenedLeafV1],
) -> Result<PalwGenVerdictV1, PalwGenCloseRefusalV1> {
    use PalwGenCloseRefusalV1 as R;
    // The stream stage: the text stage, or a decode stage before a pipeline's scoring (RFC-0004 §7.2).
    let s = misaka_palw_tir::pipeline::stream_stage(case.pipeline)
        .ok_or_else(|| R::Incomplete("the pipeline has no stream stage".into()))?;
    let sp = &case.space.stages[s];
    let prog = &sp.program;
    let post_occ = (prog.occurrences().len() - 1) as u16;
    let pos = prompt_len.saturating_sub(1) + t;
    let node = prog.output.node();
    let kind = PalwGenLeafKindV1::Commit { occurrence: post_occ, node };
    let tiles = sp.leaves().iter().filter(|l| l.coord.pos == pos && l.coord.kind == kind).count();
    if row.len() != tiles || tiles == 0 {
        return Err(R::Incomplete(format!("the logits row at {pos} is {tiles} tiles; {} were carried", row.len())));
    }
    let mut lanes = Vec::new();
    for (k, leaf) in row.iter().enumerate() {
        let expected = PalwGenLeafCoordV1 { stage: s as u8, pos, kind, tile: k as u32 };
        if leaf.coord != expected || !palw_gen_verify_leaf_v1(sp, &case.claim.stage_roots[s], leaf) {
            return Err(R::LeafNotProven(leaf.coord));
        }
        lanes.extend(leaf.values.iter().map(|v| *v as i32));
    }
    let Some(committed) = case.claim.generated.get(t as usize) else {
        return Err(R::Incomplete(format!("the claim commits no id {t}")));
    };
    let before = &case.claim.generated[..t as usize];
    let selected = crate::palw_decode_pipeline_v4::decode_select_v4(&decode.config, &decode.sampling, before, &lanes, &|_| true);
    if selected == Some(*committed as usize) {
        Ok(PalwGenVerdictV1::Acquitted)
    } else {
        Ok(PalwGenVerdictV1::Convicted { leaf: row[0].coord, fault: PalwStepFaultV1::DecodeTokenMismatch { position: t } })
    }
}

// ---------------------------------------------------------------------------------------------
// RFC-0002 F7, composed: the history dissection of a pipeline stage (spec 04b §9.5)
// ---------------------------------------------------------------------------------------------
//
// A stage's commit leaf whose cone reduces over the history — a language-model stage's attention —
// is argued by F7's dissection, not closed whole: F7's site, its phase (`PalwTirDissectPhaseV1`,
// the `tir_dissections` row), its rounds and choices (`CourtTirDissected`, `CourtTirChildChosen`)
// verbatim; the stage's evaluation is the generative court's own carriage — the proven leaves of
// every stage before it, the params under the class's root, the job's facts, images and `R` — and
// the three evaluations are F7's over the stage's view (`eval_demanded_range_v2`): the finalize, the
// element closure, and each range's partials. Nothing here restates a rule the phase holds.

use crate::palw_gen_step_v1::PalwGenStageSpaceV1;
use crate::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_MAX_REDUCTIONS, PalwTirDissectPhaseV1, PalwTirDissectSiteV1, PalwTirFoldV1, PalwTirRangeClaimV1,
    palw_tir_cone_reductions_v1, palw_tir_dissect_check_claim_v1,
};
use misaka_palw_tir::demand::{DemandRangeRequest, history_length_v1};
use misaka_palw_tir::demand_v2::eval_demanded_range_v2;

/// **The site of a generative commit leaf, if its cone reduces over `H`** — F7's site
/// ([`crate::palw_tir_dissect_v1::palw_tir_dissect_site_v1`]) over the stage's view: the reductions
/// of the node's cone (the cone stops at the occurrence's other commit points), their folds and
/// proven intervals, `H` at the leaf's position and the stage's history tile. `None`: the leaf is
/// closed, not dissected.
pub fn palw_gen_dissect_site_v1(stage: &PalwGenStageSpaceV1, coord: &PalwGenLeafCoordV1) -> Option<PalwTirDissectSiteV1> {
    let PalwGenLeafKindV1::Commit { occurrence, node } = coord.kind else { return None };
    let block = stage.occurrence_block(occurrence)?;
    let b = stage.info.view.blocks.get(block as usize)?;
    let reductions = palw_tir_cone_reductions_v1(b, node);
    if reductions.is_empty() || reductions.len() > PALW_TIR_DISSECT_MAX_REDUCTIONS {
        return None;
    }
    let h = history_length_v1(&stage.info.v1, block, coord.pos)? as u64;
    let folds: Vec<PalwTirFoldV1> = reductions
        .iter()
        .map(|r| {
            if matches!(b.nodes[*r as usize].prim, misaka_palw_tir::Prim::ReduceMax { .. }) {
                PalwTirFoldV1::Max
            } else {
                PalwTirFoldV1::Sum
            }
        })
        .collect();
    let intervals = misaka_palw_tir::interval_v2::analyze_ranges_v2(&stage.program).ok()?;
    let bounds = reductions
        .iter()
        .map(|r| intervals.get(block as usize).and_then(|v| v.get(*r as usize)).copied())
        .collect::<Option<Vec<_>>>()?;
    let counts = reductions.iter().map(|r| b.nodes[*r as usize].out.elements_at(h)).collect();
    Some(PalwTirDissectSiteV1 {
        ctx: DemandContext { pos: coord.pos, occurrence },
        node,
        reductions,
        folds,
        bounds,
        history_positions: h as u32,
        tile_positions: stage.layout.h_tile,
        counts,
    })
}

/// A source that answers the site's reductions at its context from a claim (a leaf, as a commit
/// point is: spec 04b §9.5.2) and every other question from the close's source, recording which
/// claimed values were read.
struct SuppliedSourceV1<'a> {
    inner: &'a mut dyn DemandSourceV2,
    ctx: DemandContext,
    reductions: Vec<u16>,
    values: std::collections::BTreeMap<(u16, usize), i128>,
    used: std::collections::BTreeSet<(u16, usize)>,
}

impl DemandSourceV2 for SuppliedSourceV1<'_> {
    fn node(&mut self, ctx: DemandContext, node: u16, index: usize) -> TirResult<i128> {
        if ctx == self.ctx && self.reductions.contains(&node) {
            return match self.values.get(&(node, index)) {
                Some(v) => {
                    self.used.insert((node, index));
                    Ok(*v)
                }
                None => Err(TirError::new(TirErrorKind::Missing, format!("no claimed element {index} of reduction {node}"))),
            };
        }
        self.inner.node(ctx, node, index)
    }
    fn param(&mut self, param: u16, layer: Option<u16>, index: usize) -> TirResult<i128> {
        self.inner.param(param, layer, index)
    }
    fn input(&mut self, pos: u32, input: u16, index: usize) -> TirResult<i128> {
        self.inner.input(pos, input, index)
    }
    fn state(&mut self, pos: u32, state: u16, layer: Option<u16>, index: usize) -> TirResult<StateSupply> {
        self.inner.state(pos, state, layer, index)
    }
    fn hist_row(&mut self, pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> TirResult<i128> {
        self.inner.hist_row(pos, state, layer, row_pos, index)
    }
    fn token(&mut self, pos: u32) -> TirResult<u32> {
        self.inner.token(pos)
    }
}

/// A claim's values by `(reduction, element)`, one reduction left out (`skip`).
fn claimed_values(
    reductions: &[u16],
    elements: &[Vec<u32>],
    claim: &PalwTirRangeClaimV1,
    skip: Option<u16>,
) -> std::collections::BTreeMap<(u16, usize), i128> {
    let mut out = std::collections::BTreeMap::new();
    for ((node, es), vs) in reductions.iter().zip(elements).zip(&claim.partials) {
        if Some(*node) == skip {
            continue;
        }
        for (e, v) in es.iter().zip(vs) {
            out.insert((*node, *e as usize), *v);
        }
    }
    out
}

fn disputed_elements(stage: &PalwGenStageSpaceV1, coord: &PalwGenLeafCoordV1) -> Vec<usize> {
    stage
        .leaf_index(coord)
        .map(|i| {
            let leaf = stage.leaves()[i as usize];
            (leaf.first_element..leaf.first_element + leaf.value_count as u64).map(|e| e as usize).collect()
        })
        .unwrap_or_default()
}

/// **The finalize and the element closure** (F7's `finalize_and_closure`, spec 04b §9.5.3): the
/// leaf's elements with every reduction supplied; then, until nothing new is read, one position's
/// term (`H` range `0..1`) of each reduction's elements read so far, the others supplied.
fn finalize_and_closure(
    stage: &PalwGenStageSpaceV1,
    site: &PalwTirDissectSiteV1,
    elements: &[usize],
    source: &mut SuppliedSourceV1<'_>,
    limits: &DemandLimits,
) -> Result<Vec<i128>, DemandError> {
    // A commit point that itself reduces over `H` is its own finalize: the claimed values ARE the tile.
    let values = if site.reductions.contains(&site.node) {
        elements.iter().map(|e| source.node(site.ctx, site.node, *e).map_err(DemandError::from)).collect::<Result<Vec<_>, _>>()?
    } else {
        let request = DemandRangeRequest { ctx: site.ctx, target: site.node, elements, supplied: &site.reductions, range: None };
        eval_demanded_range_v2(&stage.program, &stage.info, &request, source, limits)?.0
    };
    loop {
        let before = source.used.len();
        for node in &site.reductions {
            let demanded: Vec<usize> = source.used.iter().filter(|(n, _)| n == node).map(|(_, e)| *e).collect();
            if demanded.is_empty() {
                continue;
            }
            let supplied: Vec<u16> = site.reductions.iter().copied().filter(|r| r != node).collect();
            let request =
                DemandRangeRequest { ctx: site.ctx, target: *node, elements: &demanded, supplied: &supplied, range: Some((0, 1)) };
            eval_demanded_range_v2(&stage.program, &stage.info, &request, source, limits)?;
        }
        if source.used.len() == before {
            return Ok(values);
        }
    }
}

/// **Admit a generative root claim** — F7's `check_tir_root_claim_v1` over a pipeline stage: the
/// close's carriage proves as a cone close's does, without a conviction (a leaf that convicts on its
/// own is closed, not dissected); the leaf is dissected; the claim's shape and values are the site's
/// (`palw_tir_dissect_check_claim_v1`); and the leaf evaluated with every reduction over `H` SUPPLIED
/// from the claim reproduces the committed leaf, reading exactly the claimed values. Returns the site
/// the phase opens on.
pub fn palw_gen_check_root_claim_v1(
    case: &PalwGenCourtCaseV1<'_>,
    close: &PalwGenCloseV1,
    elements: &[Vec<u32>],
    totals: &PalwTirRangeClaimV1,
    limits: &DemandLimits,
) -> Result<PalwTirDissectSiteV1, String> {
    let p = match prove_close(case, close).map_err(|e| e.to_string())? {
        ProvedV1::Convicted(v) => return Err(format!("the leaf convicts on its own ({v:?}): it is closed, not dissected")),
        ProvedV1::Proven(p) => p,
    };
    let sp = &case.space.stages[p.s];
    let site = palw_gen_dissect_site_v1(sp, &close.disputed.coord).ok_or("the narrowed leaf is not dissected")?;
    palw_tir_dissect_check_claim_v1(&site, elements, totals).map_err(|e| e.to_string())?;
    let supplied = claimed_values(&site.reductions, elements, totals, None);
    let expected = supplied.len();
    let wanted = disputed_elements(sp, &close.disputed.coord);
    let (values, used) = match evaluate_with(case, close, &p, |inner| {
        let mut source =
            SuppliedSourceV1 { inner, ctx: site.ctx, reductions: site.reductions.clone(), values: supplied, used: Default::default() };
        let values = finalize_and_closure(sp, &site, &wanted, &mut source, limits)?;
        Ok((values, source.used.len()))
    })
    .map_err(|e| format!("the finalize does not evaluate: {e}"))?
    {
        Ok(out) => out,
        Err(v) => return Err(format!("an edge the finalize reads convicts ({v:?}): the leaf is closed, not dissected")),
    };
    if used != expected {
        return Err("the claim carries values the dissection never reads".into());
    }
    if values != close.disputed.values {
        return Err("the root claim does not finalize to the committed leaf".into());
    }
    Ok(site)
}

/// **A range's partials**: each reduction over the history positions `range`, the others supplied
/// from the phase's ROOT totals (spec 04b §9.5.4–§9.5.5) — a round's child, or the bottom's tile.
pub fn palw_gen_dissect_partials_v1(
    case: &PalwGenCourtCaseV1<'_>,
    close: &PalwGenCloseV1,
    phase: &PalwTirDissectPhaseV1,
    range: (usize, usize),
    limits: &DemandLimits,
) -> Result<Result<PalwTirRangeClaimV1, PalwGenVerdictV1>, PalwGenCloseRefusalV1> {
    palw_gen_dissect_partials_recorded_v1(case, close, phase, range, limits).map(|(out, _)| out)
}

/// [`palw_gen_dissect_partials_v1`], and the units the evaluation read — a bottom's carriage.
pub fn palw_gen_dissect_partials_recorded_v1(
    case: &PalwGenCourtCaseV1<'_>,
    close: &PalwGenCloseV1,
    phase: &PalwTirDissectPhaseV1,
    range: (usize, usize),
    limits: &DemandLimits,
) -> Result<(Result<PalwTirRangeClaimV1, PalwGenVerdictV1>, PalwGenUsedUnitsV1), PalwGenCloseRefusalV1> {
    use PalwGenCloseRefusalV1 as R;
    let p = match prove_close(case, close)? {
        ProvedV1::Convicted(v) => return Ok((Err(v), PalwGenUsedUnitsV1::default())),
        ProvedV1::Proven(p) => p,
    };
    let sp = &case.space.stages[p.s];
    let site = palw_gen_dissect_site_v1(sp, &close.disputed.coord).ok_or(R::Unadjudicable("the leaf is not dissected".into()))?;
    if site.reductions != phase.reductions() || site.history_positions != phase.history_positions() {
        return Err(R::Unadjudicable("the leaf's site is not the phase's".into()));
    }
    evaluate_recording(case, close, &p, |inner| {
        let mut source = SuppliedSourceV1 {
            inner,
            ctx: site.ctx,
            reductions: site.reductions.clone(),
            values: Default::default(),
            used: Default::default(),
        };
        let mut partials = Vec::with_capacity(site.reductions.len());
        for (i, node) in site.reductions.iter().enumerate() {
            source.values = claimed_values(&site.reductions, phase.elements(), phase.root(), Some(*node));
            let supplied: Vec<u16> = site.reductions.iter().copied().filter(|r| r != node).collect();
            let elements: Vec<usize> = phase.elements()[i].iter().map(|e| *e as usize).collect();
            let request =
                DemandRangeRequest { ctx: site.ctx, target: *node, elements: &elements, supplied: &supplied, range: Some(range) };
            partials.push(eval_demanded_range_v2(&sp.program, &sp.info, &request, &mut source, limits)?.0);
        }
        Ok(PalwTirRangeClaimV1 { partials })
    })
}

/// **The bottom of a generative dissection** — F7's `check_tir_dissect_bottom_v1` over a pipeline
/// stage: the close proves as a cone close's does (its convictions stand); then every reduction is
/// evaluated over the terminal tile's positions only, the others supplied from the root, and compared
/// with the claim the dissection narrowed to: the first differing value convicts
/// (`ComputationMismatch`, at the dissected leaf); none acquits.
pub fn palw_gen_check_dissect_bottom_v1(
    case: &PalwGenCourtCaseV1<'_>,
    close: &PalwGenCloseV1,
    phase: &PalwTirDissectPhaseV1,
    limits: &DemandLimits,
) -> Result<PalwGenVerdictV1, PalwGenCloseRefusalV1> {
    use PalwGenCloseRefusalV1 as R;
    let range = phase.terminal_range().ok_or(R::Unadjudicable("the dissection has not narrowed to one tile".into()))?;
    let partials = match palw_gen_dissect_partials_v1(case, close, phase, range, limits)? {
        Ok(partials) => partials,
        Err(verdict) => return Ok(verdict),
    };
    let computed: Vec<i128> = partials.partials.iter().flatten().copied().collect();
    let claimed: Vec<i128> = phase.claim().partials.iter().flatten().copied().collect();
    match computed.iter().zip(&claimed).position(|(a, b)| a != b).or((computed.len() != claimed.len()).then_some(0)) {
        Some(i) => Ok(PalwGenVerdictV1::Convicted {
            leaf: close.disputed.coord,
            fault: PalwStepFaultV1::ComputationMismatch { value_index: i as u32 },
        }),
        None => Ok(PalwGenVerdictV1::Acquitted),
    }
}

/// **The responder's root claim from its own execution** (a builder, for a worker and the tests): the
/// honest totals of every reduction of the leaf's site over the whole history, then the elements
/// the finalize and the closure read — `(elements, totals)`.
pub fn palw_gen_build_root_claim_v1(
    case: &PalwGenCourtCaseV1<'_>,
    close: &PalwGenCloseV1,
    limits: &DemandLimits,
) -> Result<(Vec<Vec<u32>>, PalwTirRangeClaimV1), PalwGenCloseRefusalV1> {
    palw_gen_build_root_claim_recorded_v1(case, close, limits).map(|(elements, totals, _)| (elements, totals))
}

/// [`palw_gen_build_root_claim_v1`], and the units its finalize and closure read — the root claim's
/// carriage (the honest totals' own evaluation over the whole history is the builder's, never the
/// court's).
pub fn palw_gen_build_root_claim_recorded_v1(
    case: &PalwGenCourtCaseV1<'_>,
    close: &PalwGenCloseV1,
    limits: &DemandLimits,
) -> Result<(Vec<Vec<u32>>, PalwTirRangeClaimV1, PalwGenUsedUnitsV1), PalwGenCloseRefusalV1> {
    use PalwGenCloseRefusalV1 as R;
    let p = match prove_close(case, close)? {
        ProvedV1::Convicted(v) => return Err(R::Unadjudicable(format!("the leaf convicts on its own: {v:?}"))),
        ProvedV1::Proven(p) => p,
    };
    let sp = &case.space.stages[p.s];
    let site = palw_gen_dissect_site_v1(sp, &close.disputed.coord).ok_or(R::Unadjudicable("the leaf is not dissected".into()))?;
    let wanted = disputed_elements(sp, &close.disputed.coord);
    let edge = |v: PalwGenVerdictV1| R::Unadjudicable(format!("an edge the finalize reads convicts: {v:?}"));
    // Every reduction's every element, computed whole (no node supplied): the honest totals.
    let full = evaluate_with(case, close, &p, |inner| {
        let mut full = std::collections::BTreeMap::new();
        for (node, count) in site.reductions.iter().zip(&site.counts) {
            let elements: Vec<usize> = (0..*count as usize).collect();
            let request = DemandRangeRequest { ctx: site.ctx, target: *node, elements: &elements, supplied: &[], range: None };
            let (values, _) = eval_demanded_range_v2(&sp.program, &sp.info, &request, &mut *inner, limits)?;
            for (e, v) in values.into_iter().enumerate() {
                full.insert((*node, e), v);
            }
        }
        Ok(full)
    })?
    .map_err(edge)?;
    // The finalize and the closure, with those totals supplied: the claim's elements, and what the
    // court reads to admit it.
    let (read, used) = evaluate_recording(case, close, &p, |inner| {
        let mut source = SuppliedSourceV1 {
            inner,
            ctx: site.ctx,
            reductions: site.reductions.clone(),
            values: full.clone(),
            used: Default::default(),
        };
        finalize_and_closure(sp, &site, &wanted, &mut source, limits)?;
        Ok(source.used)
    })?;
    let read = read.map_err(edge)?;
    let mut elements = vec![Vec::new(); site.reductions.len()];
    let mut totals = vec![Vec::new(); site.reductions.len()];
    for (node, e) in &read {
        let i = site.reductions.iter().position(|r| r == node).expect("a used value is a reduction's");
        elements[i].push(*e as u32);
        totals[i].push(full[&(*node, *e)]);
    }
    Ok((elements, PalwTirRangeClaimV1 { partials: totals }, used))
}

// ---------------------------------------------------------------------------------------------
// Builders: what a close must carry, and the close that carries exactly that
// ---------------------------------------------------------------------------------------------

/// **The units a cone close of the disputed leaf reads** — the court's own evaluation (step 4 of
/// [`palw_gen_adjudicate_leaf_v1`]) over a carriage that holds everything, recorded. Refused for a
/// carriage that convicts on its face (a builder builds an honest executor's evidence).
pub fn palw_gen_cone_units_v1(
    case: &PalwGenCourtCaseV1<'_>,
    full: &PalwGenCloseV1,
    limits: &DemandLimits,
) -> Result<PalwGenUsedUnitsV1, PalwGenCloseRefusalV1> {
    use PalwGenCloseRefusalV1 as R;
    let p = match prove_close(case, full)? {
        ProvedV1::Convicted(v) => return Err(R::Unadjudicable(format!("the carriage convicts on its face: {v:?}"))),
        ProvedV1::Proven(p) => p,
    };
    let d = &full.disputed;
    let sp = &case.space.stages[p.s];
    let elements = disputed_elements(sp, &d.coord);
    let target = match d.coord.kind {
        PalwGenLeafKindV1::Commit { occurrence, node } => {
            DemandTarget::Node { ctx: DemandContext { pos: d.coord.pos, occurrence }, node }
        }
        PalwGenLeafKindV1::State { state, layer } => DemandTarget::StateAfter { pos: d.coord.pos, state, layer },
    };
    let (out, used) = evaluate_recording(case, full, &p, |source| {
        eval_demanded_v2(&sp.program, &sp.info, &DemandRequest { target, elements: &elements }, source, limits).map(|(v, _)| v)
    })?;
    out.map_err(|v| R::Unadjudicable(format!("an edge the cone reads convicts: {v:?}")))?;
    Ok(used)
}

/// **A close cut down to the units in `used`**: the disputed leaf, the carried leaves the evaluation
/// read, the param leaves it read and the image tiles it read — exactly what the court repeats it
/// from.
pub fn palw_gen_restrict_close_v1(case: &PalwGenCourtCaseV1<'_>, full: &PalwGenCloseV1, used: &PalwGenUsedUnitsV1) -> PalwGenCloseV1 {
    let index_of = |o: &PalwGenOpenedLeafV1| case.space.stages.get(o.coord.stage as usize).and_then(|s| s.leaf_index(&o.coord));
    PalwGenCloseV1 {
        disputed: full.disputed.clone(),
        operands: full
            .operands
            .iter()
            .filter(|o| index_of(o).is_some_and(|i| used.leaves.contains(&(o.coord.stage, i))))
            .cloned()
            .collect(),
        image_tiles: full.image_tiles.iter().filter(|t| used.image_tiles.contains(&(t.image, t.tile))).cloned().collect(),
        params: full.params.iter().filter(|o| used.params.contains(&o.leaf_index)).cloned().collect(),
    }
}
