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
                    let rows =
                        facts.get(*up as usize).ok_or_else(|| missing(format!("no facts of stage {up}")))?.trip.saturating_sub(*drop);
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
}

impl<'a> PalwGenStageSourceV1<'a> {
    pub fn new(inner: &'a mut dyn DemandSourceV2, answers: Vec<PalwGenInputAnswerV1>, draw: PalwGenDrawV1) -> Self {
        Self { inner, answers, draw, images: Vec::new(), image_tiles: Default::default(), violation: None }
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
