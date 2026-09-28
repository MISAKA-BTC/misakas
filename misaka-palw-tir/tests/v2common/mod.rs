//! Shared helpers for the version-2 tests: three toy stage programs that together make a tiny
//! text-to-image pipeline — a causal encoder with a `Rows` output, a denoiser whose latent `post`
//! writes (a `Final` output), and a decoder that quantises to `u8` pixels — plus deterministic
//! params, and random inputs drawn through RFC-0003's `R` (`misaka-palw-gen`).
#![allow(dead_code)]

use std::collections::BTreeMap;

use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::interp::ParamSource;
use misaka_palw_tir::interp_v2::MapInputs;
use misaka_palw_tir::pipeline::*;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::program_v2::*;
use misaka_palw_tir::{Cmp, DType, Dim, MapParams, Ref, Rounding, Tensor, TensorType};

pub const TOK: u32 = 16;
pub const D: u32 = 4;
/// The toy latent: `[C, H, W] = [3, 2, 2]`, flattened to 12 lanes.
pub const LAT: u32 = 12;
pub const LATENT_LO: i64 = -(1 << 20);
pub const LATENT_HI: i64 = 1 << 20;
/// The denoiser's input order (its declaration order after lifting).
pub const IN_NOISE: u16 = 0;
pub const IN_COND: u16 = 1;
pub const IN_COND_LEN: u16 = 2;
pub const IN_GUIDANCE: u16 = 3;
pub const IN_STEPS_IDX: u16 = 4;
pub const IN_JITTER: u16 = 5;

fn carry_of(pb: &ProgramBuilder, block: u8) -> Vec<TensorType> {
    let b = &pb.blocks[block as usize];
    b.carry_out.iter().map(|n| b.nodes[*n as usize].out.clone()).collect()
}

fn node_of(r: Ref) -> u16 {
    let Ref::Node(n) = r else { panic!("a node") };
    n
}

/// Stage A — a causal "encoder" over `template ‖ prompt`: a running sum of token embeddings over a
/// window of 4, one row per position (`Rows`).
pub fn encoder_program() -> TirProgramV2 {
    let mut pb = ProgramBuilder::new(TOK, HISTORY_BOUND_V1_SMALL);
    let emb = pb.param("enc.embed", DType::I16, &[TOK, D], false);
    let hist = pb.hist_state("enc.window", DType::I16, &[D], 4, false);
    let pre = {
        let mut b = pb.block("enc.pre", vec![]);
        let e = b.gather(emb, Ref::Input(0), 0, 0);
        let w = b.hist_append(hist, e);
        let s = b.reduce_sum(w, 0, DType::I32);
        let s = b.reshape_fixed(s, &[D]);
        let s = b.clamp(s, i16::MIN as i64, i16::MAX as i64, DType::I16);
        b.finish(&[s])
    };
    let carry = carry_of(&pb, pre);
    let (post, row) = {
        let mut b = pb.block("enc.post", carry);
        let x = b.cast(Ref::CarryIn(0), DType::I32);
        let three = b.c(DType::I32, 3);
        let y = b.add(x, three, DType::I32);
        let r = b.clamp(y, i16::MIN as i64, i16::MAX as i64, DType::I16);
        b.commit(r);
        (b.finish(&[]), node_of(r))
    };
    let v1 = pb.finish(pre, vec![], post, row);
    TirProgramV2::from_v1_lifting_params(&v1, &[], OutputDecl::Rows { node: row }).unwrap()
}

/// Stage B — a toy denoiser over its steps (`Final`): the initial latent is RFC-0003 Normal noise
/// selected at position 0; the conditioning is the encoder's rows, masked by their count; two
/// layers; and `post` applies a guidance-scaled, table-weighted update plus a per-step jitter and
/// WRITES THE LATENT — the step's last act, and the program's output.
pub fn denoiser_program() -> TirProgramV2 {
    denoiser_variant(true, true)
}

/// `commit_write`: whether the `post` `StateWrite` is committed (it is the output when it is);
/// `output_is_write`: whether the output is the write (else a separate committed node is).
pub fn denoiser_variant(commit_write: bool, output_is_write: bool) -> TirProgramV2 {
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    // Future inputs, declared as params and lifted below, in this order.
    let noise = pb.param("den.noise", DType::I32, &[3, 2, 2], false);
    let cond = pb.param("den.cond", DType::I16, &[5, D], false);
    let cond_len = pb.param("den.cond_len", DType::Idx, &[], false);
    let guidance = pb.param("den.guidance", DType::I32, &[], false);
    let steps_idx = pb.param("den.steps_idx", DType::Idx, &[], false);
    let jitter = pb.param("den.jitter", DType::I32, &[LAT], false);
    // True params.
    let w = pb.param("den.w", DType::I8, &[LAT], true);
    let dt = pb.param("den.dt", DType::I32, &[2, 4], false);
    let latent = pb.fixed_state("den.latent", DType::I32, &[LAT], LATENT_LO, LATENT_HI, false);
    let pre = {
        let mut b = pb.block("den.pre", vec![]);
        let q12 = b.shr(noise, 12, Rounding::HalfAwayFromZero, DType::I32);
        let flat = b.reshape_fixed(q12, &[LAT]);
        let zero_idx = b.c(DType::Idx, 0);
        let is0 = b.compare(Ref::Input(1), zero_idx, Cmp::Eq);
        let x = b.select(is0, flat, Ref::State(latent), DType::I32);
        let h = b.cast(x, DType::I32);
        let iota = b.iota(DType::Idx, &[Dim::Fixed(5)], 0, 0, 1);
        let mask = b.compare(iota, cond_len, Cmp::Lt);
        let mask = b.reshape_fixed(mask, &[5, 1]);
        let c32 = b.cast(cond, DType::I32);
        let zero = b.c(DType::I32, 0);
        let masked = b.select(mask, c32, zero, DType::I32);
        let t = b.reduce_sum(masked, 0, DType::I32);
        let t = b.reshape_fixed(t, &[D]);
        b.finish(&[x, h, t])
    };
    let carry = carry_of(&pb, pre);
    let layer = {
        let mut b = pb.block("den.layer", carry.clone());
        let x = b.cast(Ref::CarryIn(0), DType::I32);
        let h64 = b.cast(Ref::CarryIn(1), DType::I64);
        let hw = b.mul(h64, w, DType::I64);
        let t2 = b.reshape_fixed(Ref::CarryIn(2), &[1, D]);
        let t64 = b.cast(t2, DType::I64);
        let tt = b.reduce_sum(t64, 1, DType::I64);
        let tt = b.reshape_fixed(tt, &[1]);
        let s = b.add(hw, tt, DType::I64);
        let d = b.shr(s, 4, Rounding::HalfAwayFromZero, DType::I64);
        let h2 = b.clamp(d, i32::MIN as i64 / 2, i32::MAX as i64 / 2, DType::I32);
        let t3 = b.cast(Ref::CarryIn(2), DType::I32);
        b.finish(&[x, h2, t3])
    };
    let (post, out) = {
        let mut b = pb.block("den.post", carry);
        let x64 = b.cast(Ref::CarryIn(0), DType::I64);
        let h64 = b.cast(Ref::CarryIn(1), DType::I64);
        let v = b.sub(h64, x64, DType::I64);
        let g64 = b.cast(guidance, DType::I64);
        let vg = b.mul(g64, v, DType::I64);
        let vg = b.shr(vg, 4, Rounding::HalfAwayFromZero, DType::I64);
        let row = b.gather(dt, steps_idx, 0, 0);
        let pc = b.clamp(Ref::Input(1), 0, 3, DType::Idx);
        let c = b.gather(row, pc, 0, 0);
        let c = b.clamp(c, -(1 << 12), 1 << 12, DType::I32);
        let c64 = b.cast(c, DType::I64);
        let upd = b.mul(vg, c64, DType::I64);
        let upd = b.shr(upd, 12, Rounding::HalfAwayFromZero, DType::I64);
        let j64 = b.cast(jitter, DType::I64);
        let jit = b.shr(j64, 16, Rounding::HalfAwayFromZero, DType::I64);
        let xn = b.add(x64, upd, DType::I64);
        let xn = b.add(xn, jit, DType::I64);
        let sw = b.state_write(latent, xn);
        if commit_write {
            b.commit(sw);
        }
        let out = if output_is_write {
            sw
        } else {
            let o = b.clamp(xn, LATENT_LO, LATENT_HI, DType::I32);
            b.commit(o)
        };
        (b.finish(&[]), node_of(out))
    };
    let v1 = pb.finish(pre, vec![layer, layer], post, out);
    TirProgramV2::from_v1_lifting_params(
        &v1,
        &[
            (0, InputSource::Random { domain: 1, dist: RandomDist::Normal, per_step: false }),
            (1, InputSource::External { lo: i16::MIN as i64, hi: i16::MAX as i64 }),
            (2, InputSource::External { lo: 0, hi: 5 }),
            (3, InputSource::External { lo: 0, hi: 160 }),
            (4, InputSource::External { lo: 0, hi: 1 }),
            (5, InputSource::Random { domain: 2, dist: RandomDist::Normal, per_step: true }),
        ],
        OutputDecl::Final { node: out },
    )
    .unwrap()
}

/// Stage C — a toy decoder at one position (`Final`): the Q12 latent in `[-1, 1]` to pixels
/// `round_half_away((z + 1)/2 · 255)`, `[C, H, W] → [H, W, C]`, proved in `[0, 255]`.
pub fn decoder_program() -> TirProgramV2 {
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let z = pb.param("dec.z", DType::I32, &[LAT], false);
    let pre = {
        let mut b = pb.block("dec.pre", vec![]);
        let zc = b.clamp(z, -(1 << 12), 1 << 12, DType::I32);
        let one = b.c(DType::I32, 1 << 12);
        let s = b.add(zc, one, DType::I32);
        let k = b.c(DType::I32, 255);
        let m = b.mul(s, k, DType::I32);
        let px = b.shr(m, 13, Rounding::HalfAwayFromZero, DType::I32);
        let chw = b.reshape_fixed(px, &[3, 2, 2]);
        let hwc = b.transpose(chw, &[1, 2, 0]);
        let p = b.clamp(hwc, 0, 255, DType::I16);
        b.finish(&[p])
    };
    let carry = carry_of(&pb, pre);
    let (post, out) = {
        let mut b = pb.block("dec.post", carry);
        let o = b.clamp(Ref::CarryIn(0), 0, 255, DType::I16);
        b.commit(o);
        (b.finish(&[]), node_of(o))
    };
    let v1 = pb.finish(pre, vec![], post, out);
    TirProgramV2::from_v1_lifting_params(
        &v1,
        &[(0, InputSource::External { lo: LATENT_LO, hi: LATENT_HI })],
        OutputDecl::Final { node: out },
    )
    .unwrap()
}

/// The three stages as one pipeline: encode `[1] ‖ prompt ‖ [2]` (≤ 6 tokens) → denoise over the
/// job's steps (≤ 4), reading the rows after the template's first, their count, the guidance and
/// the step-set index → decode once.
pub fn toy_pipeline() -> (TirPipelineV1, Vec<TirProgramV2>) {
    let programs = vec![encoder_program(), denoiser_program(), decoder_program()];
    let rule = TokenRule { prefix: vec![1], source: TokenSource::Prompt, suffix: vec![2], pad: None };
    let p = TirPipelineV1 {
        version: TIR_PIPELINE_VERSION_V1,
        stages: vec![
            StageDecl { name: "encode".into(), program: 0, trip: TripRule::TokenCount, max_trip: 6, tokens: Some(rule), bind: vec![] },
            StageDecl {
                name: "denoise".into(),
                program: 1,
                trip: TripRule::JobSteps,
                max_trip: 4,
                tokens: None,
                bind: vec![
                    Binding::StageRows { stage: 0, drop: 1, pad_to: 5 },
                    Binding::StageRowCount { stage: 0, drop: 1 },
                    Binding::JobScalar { index: 0 },
                    Binding::JobScalar { index: 1 },
                ],
            },
            StageDecl {
                name: "decode".into(),
                program: 2,
                trip: TripRule::Fixed { n: 1 },
                max_trip: 1,
                tokens: None,
                bind: vec![Binding::StageFinal { stage: 1 }],
            },
        ],
        output_stage: 2,
    };
    (p, programs)
}

/// The node of `pre` whose value is the block's `k`-th carry-out (the denoiser's `x` is carry 0).
pub fn pre_carry(p: &TirProgramV2, k: usize) -> u16 {
    p.blocks[p.schedule.pre as usize].carry_out[k]
}

pub fn toy_job() -> PipelineJob {
    PipelineJob { prompt: vec![5, 6, 7], negative: vec![], steps: 3, scalars: vec![24, 1], images: vec![], generated: vec![] }
}

/// A 64-bit LCG, so no vector depends on an RNG crate's stream.
pub struct Lcg(pub u64);
impl Lcg {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 ^ (self.0 >> 29)
    }
    pub fn range(&mut self, lo: i128, hi: i128) -> i128 {
        lo + (self.next() as u128 % (hi - lo + 1) as u128) as i128
    }
}

/// Deterministic params for a version-2 program (per layer where declared per layer).
pub fn materialize_v2(p: &TirProgramV2, seed: u64) -> MapParams {
    let mut rng = Lcg(seed);
    let mut out = MapParams::default();
    for (j, d) in p.params.iter().enumerate() {
        let layers: Vec<Option<u16>> = if d.per_layer { (0..p.schedule.layers.len() as u16).map(Some).collect() } else { vec![None] };
        for l in layers {
            let n: usize = d.shape.iter().map(|x| *x as usize).product();
            let (lo, hi) = match d.dtype {
                DType::I8 => (-128, 127),
                DType::I16 => (-3000, 3000),
                DType::I32 => (-(1 << 12), 1 << 12),
                _ => (0, 1),
            };
            let data = (0..n).map(|_| rng.range(lo, hi)).collect();
            out.tensors.insert((j as u16, l), Tensor::new(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), data).unwrap());
        }
    }
    out
}

/// Random inputs through RFC-0003's `R`, for one job seed and item index.
pub struct GenRandom {
    pub seed: [u8; 32],
    pub position: u32,
}

impl RandomSource for GenRandom {
    fn random(&self, domain: u16, dist: RandomDist, step: u32, shape: &[u32]) -> Option<Tensor> {
        let n: u64 = shape.iter().map(|d| *d as u64).product();
        let (d, dtype) = match dist {
            RandomDist::Uniform { .. } => (misaka_palw_gen::RandDistV1::Uniform, DType::Idx),
            RandomDist::Normal => (misaka_palw_gen::RandDistV1::Normal, DType::I32),
        };
        let v = misaka_palw_gen::rand_values_v1(domain, d, &self.seed, step, self.position, n).ok()?;
        Tensor::new(dtype, shape.iter().map(|x| *x as usize).collect(), v.into_iter().map(|x| x as i128).collect()).ok()
    }
}

/// Each program's params, by program index.
pub struct ProgramParams(pub Vec<MapParams>);

impl PipelineParams for ProgramParams {
    fn params(&self, program: u16) -> &dyn ParamSource {
        &self.0[program as usize]
    }
}

/// The denoiser's inputs for `steps` positions, by hand: the random ones through `R`, the external
/// ones as given.
pub fn denoiser_inputs(random: &GenRandom, cond: Tensor, cond_len: u32, guidance: i64, steps_idx: u32, steps: u32) -> MapInputs {
    let mut m = MapInputs::default();
    m.constant.insert(IN_NOISE, random.random(1, RandomDist::Normal, 0, &[3, 2, 2]).unwrap());
    m.constant.insert(IN_COND, cond);
    m.constant.insert(IN_COND_LEN, Tensor::scalar(DType::Idx, cond_len as i128).unwrap());
    m.constant.insert(IN_GUIDANCE, Tensor::scalar(DType::I32, guidance as i128).unwrap());
    m.constant.insert(IN_STEPS_IDX, Tensor::scalar(DType::Idx, steps_idx as i128).unwrap());
    for p in 0..steps {
        m.at.insert((IN_JITTER, p), random.random(2, RandomDist::Normal, p, &[LAT]).unwrap());
    }
    m
}

pub fn cond_rows(rng: &mut Lcg) -> Tensor {
    Tensor::new(DType::I16, vec![5, D as usize], (0..5 * D).map(|_| rng.range(-2000, 2000)).collect()).unwrap()
}

/// Params of a version-1 program re-indexed for a version-2 program with the same param names
/// (lifting removes params and shifts the rest).
pub fn params_by_name(from: &misaka_palw_tir::TirProgramV1, params: &MapParams, to: &TirProgramV2) -> MapParams {
    let index: BTreeMap<&str, u16> = to.params.iter().enumerate().map(|(j, d)| (d.name.as_str(), j as u16)).collect();
    let mut out = MapParams::default();
    for ((j, l), t) in &params.tensors {
        if let Some(k) = index.get(from.params[*j as usize].name.as_str()) {
            out.tensors.insert((*k, *l), t.clone());
        }
    }
    out
}

/// A one-stage bidirectional "encoder" at one position over a padded token axis: the embeddings of
/// the tokens its `JobTokenCount` mask admits, summed. The pad id must not matter.
pub fn bidirectional_pipeline(pad_id: u32) -> (TirPipelineV1, Vec<TirProgramV2>) {
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::{Cmp, DType, Dim, Ref};
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let tokens = pb.param("bi.tokens", DType::Idx, &[6], false);
    let count = pb.param("bi.count", DType::Idx, &[], false);
    let embed = pb.param("bi.embed", DType::I16, &[TOK, D], false);
    let pre = {
        let mut b = pb.block("bi.pre", vec![]);
        let e = b.gather(embed, tokens, 0, 0);
        let iota = b.iota(DType::Idx, &[Dim::Fixed(6)], 0, 0, 1);
        let mask = b.compare(iota, count, Cmp::Lt);
        let mask = b.reshape_fixed(mask, &[6, 1]);
        let e32 = b.cast(e, DType::I32);
        let zero = b.c(DType::I32, 0);
        let kept = b.select(mask, e32, zero, DType::I32);
        let s = b.reduce_sum(kept, 0, DType::I32);
        b.finish(&[s])
    };
    let carry = {
        let b = &pb.blocks[pre as usize];
        vec![b.nodes[b.carry_out[0] as usize].out.clone()]
    };
    let (post, out) = {
        let mut b = pb.block("bi.post", carry);
        let o = b.clamp(Ref::CarryIn(0), i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(o);
        let Ref::Node(n) = o else { unreachable!() };
        (b.finish(&[]), n)
    };
    let v1 = pb.finish(pre, vec![], post, out);
    let prog = misaka_palw_tir::program_v2::TirProgramV2::from_v1_lifting_params(
        &v1,
        &[(0, InputSource::External { lo: 0, hi: TOK as i64 - 1 }), (1, InputSource::External { lo: 0, hi: 6 })],
        OutputDecl::Final { node: out },
    )
    .unwrap();
    let rule =
        TokenRule { prefix: vec![1], source: TokenSource::Prompt, suffix: vec![2], pad: Some(TokenPad { id: pad_id, to_len: 6 }) };
    let p = TirPipelineV1 {
        version: TIR_PIPELINE_VERSION_V1,
        stages: vec![StageDecl {
            name: "encode".into(),
            program: 0,
            trip: TripRule::Fixed { n: 1 },
            max_trip: 1,
            tokens: None,
            bind: vec![Binding::JobTokens { rule: rule.clone() }, Binding::JobTokenCount { rule }],
        }],
        output_stage: 0,
    };
    (p, vec![prog])
}

/// The toy vision image: `2 × 3` RGB.
pub const VIS_H: u32 = 2;
pub const VIS_W: u32 = 3;

/// A toy image encoder (RFC-0003 §II.4): the job's `i16 [2, 3, 3]` image, each pixel centred
/// (`2x − 255`, the per-channel normalisation), cut into six one-pixel patches, projected by an `i8
/// [3, 4]` weight and summed: `Final [1, 4]`, an embedding.
pub fn vision_program() -> TirProgramV2 {
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let image = pb.param("vis.image", DType::I16, &[VIS_H, VIS_W, 3], false);
    let w = pb.param("vis.w", DType::I8, &[3, D], false);
    let pre = {
        let mut b = pb.block("vis.pre", vec![]);
        let x = b.cast(image, DType::I32);
        let two = b.c(DType::I32, 2);
        let x2 = b.mul(x, two, DType::I32);
        let mid = b.c(DType::I32, 255);
        let centred = b.sub(x2, mid, DType::I32);
        let patches = b.reshape_fixed(centred, &[VIS_H * VIS_W, 3]);
        let proj = b.matmul(patches, w, DType::I32);
        let pooled = b.reduce_sum(proj, 0, DType::I32);
        b.finish(&[pooled])
    };
    let carry = carry_of(&pb, pre);
    let (post, out) = {
        let mut b = pb.block("vis.post", carry);
        let o = b.clamp(Ref::CarryIn(0), i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(o);
        (b.finish(&[]), node_of(o))
    };
    let v1 = pb.finish(pre, vec![], post, out);
    TirProgramV2::from_v1_lifting_params(&v1, &[(0, InputSource::External { lo: 0, hi: 255 })], OutputDecl::Final { node: out })
        .unwrap()
}

/// Two image inputs, `i16 [2, 3, 3]` and `i16 [3, 2, 3]`, flattened and added: `Final [1, 18]`.
pub fn two_image_program() -> TirProgramV2 {
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let a = pb.param("two.a", DType::I16, &[2, 3, 3], false);
    let b2 = pb.param("two.b", DType::I16, &[3, 2, 3], false);
    let pre = {
        let mut b = pb.block("two.pre", vec![]);
        let xa = b.cast(a, DType::I32);
        let xb = b.cast(b2, DType::I32);
        let ra = b.reshape_fixed(xa, &[1, 18]);
        let rb = b.reshape_fixed(xb, &[1, 18]);
        let s = b.add(ra, rb, DType::I32);
        b.finish(&[s])
    };
    let carry = carry_of(&pb, pre);
    let (post, out) = {
        let mut b = pb.block("two.post", carry);
        let o = b.clamp(Ref::CarryIn(0), i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(o);
        (b.finish(&[]), node_of(o))
    };
    let v1 = pb.finish(pre, vec![], post, out);
    let px = InputSource::External { lo: 0, hi: 255 };
    TirProgramV2::from_v1_lifting_params(&v1, &[(0, px), (1, px)], OutputDecl::Final { node: out }).unwrap()
}

/// The toy vision pipeline: one stage, once, over job image 0.
pub fn vision_pipeline() -> (TirPipelineV1, Vec<TirProgramV2>) {
    let p = TirPipelineV1 {
        version: TIR_PIPELINE_VERSION_V1,
        stages: vec![StageDecl {
            name: "vision".into(),
            program: 0,
            trip: TripRule::Fixed { n: 1 },
            max_trip: 1,
            tokens: None,
            bind: vec![Binding::JobImage { index: 0 }],
        }],
        output_stage: 0,
    };
    (p, vec![vision_program()])
}

/// The toy vision job: one `2 × 3` image whose bytes are `(37 i + 11) mod 256`.
pub fn vision_job() -> PipelineJob {
    let rgb = (0..VIS_H * VIS_W * 3).map(|i| ((37 * i + 11) % 256) as u8).collect();
    PipelineJob { images: vec![JobImageV1 { h: VIS_H, w: VIS_W, rgb }], ..PipelineJob::default() }
}

/// The toy language model's image placeholder id.
pub const PLACEHOLDER: u32 = 15;
/// Image rows the toy vision stage gives the language model.
pub const VLM_ROWS: u32 = 2;

/// The VLM's vision stage: the toy image encoder with two rows out — the six patches' projections
/// summed in two groups of three: `Final [2, 4]`.
pub fn vlm_vision_program() -> TirProgramV2 {
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let image = pb.param("vv.image", DType::I16, &[VIS_H, VIS_W, 3], false);
    let w = pb.param("vv.w", DType::I8, &[3, D], false);
    let pre = {
        let mut b = pb.block("vv.pre", vec![]);
        let x = b.cast(image, DType::I32);
        let two = b.c(DType::I32, 2);
        let x2 = b.mul(x, two, DType::I32);
        let mid = b.c(DType::I32, 255);
        let centred = b.sub(x2, mid, DType::I32);
        let patches = b.reshape_fixed(centred, &[VIS_H * VIS_W, 3]);
        let proj = b.matmul(patches, w, DType::I32);
        let groups = b.reshape_fixed(proj, &[VLM_ROWS, 3, D]);
        let rows = b.reduce_sum(groups, 1, DType::I32);
        let rows = b.reshape_fixed(rows, &[VLM_ROWS, D]);
        b.finish(&[rows])
    };
    let carry = carry_of(&pb, pre);
    let (post, out) = {
        let mut b = pb.block("vv.post", carry);
        let o = b.clamp(Ref::CarryIn(0), i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(o);
        (b.finish(&[]), node_of(o))
    };
    let v1 = pb.finish(pre, vec![], post, out);
    TirProgramV2::from_v1_lifting_params(&v1, &[(0, InputSource::External { lo: 0, hi: 255 })], OutputDecl::Final { node: out })
        .unwrap()
}

/// A toy language model over a token stream (`Logits [1, 16]`): the token's `i8` embedding — or, with
/// `images`, at a position whose token is [`PLACEHOLDER`], the next image row (a `Fixed` cursor
/// counts the placeholders passed; RFC-0003 §II.2.1's placement) — projected to the vocabulary.
pub fn lm_program(images: bool) -> TirProgramV2 {
    let mut pb = ProgramBuilder::new(TOK, HISTORY_BOUND_V1_SMALL);
    let rows = images.then(|| pb.param("lm.image_rows", DType::I32, &[VLM_ROWS, D], false));
    let emb = pb.param("lm.emb", DType::I8, &[TOK, D], false);
    let w = pb.param("lm.w", DType::I8, &[D, TOK], false);
    let cursor = images.then(|| pb.fixed_state("lm.cursor", DType::I32, &[1], 0, VLM_ROWS as i64, false));
    let pre = {
        let mut b = pb.block("lm.pre", vec![]);
        let e = b.gather(emb, Ref::Input(0), 0, 0);
        let e = b.cast(e, DType::I32);
        let e = b.reshape_fixed(e, &[1, D]);
        let x = match (rows, cursor) {
            (Some(rows), Some(cursor)) => {
                let ph = b.c(DType::Idx, PLACEHOLDER as i128);
                let is_img = b.compare(Ref::Input(0), ph, Cmp::Eq);
                let at = b.clamp(Ref::State(cursor), 0, VLM_ROWS as i64 - 1, DType::Idx);
                let row = b.gather(rows, at, 0, 0);
                let row = b.shr(row, 16, Rounding::HalfAwayFromZero, DType::I32);
                let row = b.clamp(row, -128, 127, DType::I32);
                let x = b.select(is_img, row, e, DType::I32);
                let step = b.cast(is_img, DType::I32);
                let next = b.add(Ref::State(cursor), step, DType::I32);
                let next = b.clamp(next, 0, VLM_ROWS as i64, DType::I32);
                b.state_write(cursor, next);
                x
            }
            _ => e,
        };
        // Projected here, where `x`'s interval is proved (a carry-in reads as its dtype's range).
        let l = b.matmul(x, w, DType::I32);
        b.finish(&[l])
    };
    let carry = carry_of(&pb, pre);
    let (post, out) = {
        let mut b = pb.block("lm.post", carry);
        let l = b.clamp(Ref::CarryIn(0), i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(l);
        (b.finish(&[]), node_of(l))
    };
    let v1 = pb.finish(pre, vec![], post, out);
    let lifted: Vec<(u16, InputSource)> =
        if images { vec![(0, InputSource::External { lo: i32::MIN as i64, hi: i32::MAX as i64 })] } else { vec![] };
    TirProgramV2::from_v1_lifting_params(&v1, &lifted, OutputDecl::Logits { node: out, scheme_id: v1.logits_scheme_id }).unwrap()
}

/// A one-stage text pipeline: the language model over the text stream.
pub fn text_pipeline() -> (TirPipelineV1, Vec<TirProgramV2>) {
    let p = TirPipelineV1 {
        version: TIR_PIPELINE_VERSION_V1,
        stages: vec![StageDecl {
            name: "lm".into(),
            program: 0,
            trip: TripRule::TextStream,
            max_trip: 12,
            tokens: None,
            bind: vec![],
        }],
        output_stage: 0,
    };
    (p, vec![lm_program(false)])
}

/// The toy vision-language pipeline: the vision stage over job image 0, then the language model over
/// the text stream, its image rows through a `StageFinal` edge.
pub fn vlm_pipeline() -> (TirPipelineV1, Vec<TirProgramV2>) {
    let p = TirPipelineV1 {
        version: TIR_PIPELINE_VERSION_V1,
        stages: vec![
            StageDecl {
                name: "vision".into(),
                program: 0,
                trip: TripRule::Fixed { n: 1 },
                max_trip: 1,
                tokens: None,
                bind: vec![Binding::JobImage { index: 0 }],
            },
            StageDecl {
                name: "lm".into(),
                program: 1,
                trip: TripRule::TextStream,
                max_trip: 12,
                tokens: None,
                bind: vec![Binding::StageFinal { stage: 0 }],
            },
        ],
        output_stage: 1,
    };
    (p, vec![vlm_vision_program(), lm_program(true)])
}

/// A greedy selector (the argmax, the lowest id on ties) that ends after `limit` ids — a stand-in for
/// RFC-0001's decoder at temperature 0 with no controls.
pub fn greedy(limit: usize) -> impl FnMut(u32, &Tensor) -> TextSelectV1 {
    let mut n = 0;
    move |_, logits| {
        let (id, _) =
            logits.data.iter().enumerate().fold((0usize, i128::MIN), |best, (i, v)| if *v > best.1 { (i, *v) } else { best });
        n += 1;
        if n >= limit { TextSelectV1::Last(id as u32) } else { TextSelectV1::Next(id as u32) }
    }
}

/// A one-stage pipeline whose `Final` program gathers an `i8` embedding by the job's 8 (padded) token
/// ids and multiplies it by an `[8, 8]` weight: 512 MACs a position, three positions. The toy stages
/// have no MatMul, so this one carries the job's MACs.
pub fn matmul_pipeline() -> (TirPipelineV1, Vec<TirProgramV2>) {
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::pipeline::*;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::{DType, Ref};
    let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let tokens = pb.param("mm.tokens", DType::Idx, &[8], false);
    let emb = pb.param("mm.emb", DType::I8, &[TOK, 8], false);
    let w = pb.param("mm.w", DType::I8, &[8, 8], false);
    let pre = {
        let mut b = pb.block("mm.pre", vec![]);
        let x = b.gather(emb, tokens, 0, 0);
        let y = b.matmul(x, w, DType::I32);
        b.finish(&[y])
    };
    let carry = {
        let b = &pb.blocks[pre as usize];
        vec![b.nodes[b.carry_out[0] as usize].out.clone()]
    };
    let (post, out) = {
        let mut b = pb.block("mm.post", carry);
        let o = b.clamp(Ref::CarryIn(0), i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(o);
        let Ref::Node(n) = o else { unreachable!() };
        (b.finish(&[]), n)
    };
    let v1 = pb.finish(pre, vec![], post, out);
    let prog = TirProgramV2::from_v1_lifting_params(
        &v1,
        &[(0, InputSource::External { lo: 0, hi: TOK as i64 - 1 })],
        OutputDecl::Final { node: out },
    )
    .unwrap();
    let rule = TokenRule { prefix: vec![], source: TokenSource::Prompt, suffix: vec![], pad: Some(TokenPad { id: 0, to_len: 8 }) };
    let p = TirPipelineV1 {
        version: TIR_PIPELINE_VERSION_V1,
        stages: vec![StageDecl {
            name: "mm".into(),
            program: 0,
            trip: TripRule::Fixed { n: 3 },
            max_trip: 3,
            tokens: None,
            bind: vec![Binding::JobTokens { rule }],
        }],
        output_stage: 0,
    };
    (p, vec![prog])
}
