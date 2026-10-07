//! **Convolutional networks as data** (FR-19: `CONV_2D_V1`, `BN_FOLD_V1`, `POOL_MAX_2D_V1`, `RESIDUAL_ADD_ACT_V1`): a network
//! is a [`CnnSpec`] — a tree of convolutions (each with its batch norm and activation), max pools, activations and residual
//! units — read from a config by an adapter of kind `cnn`; no network's name is in this file.
//!
//! **Rows mode.** An activation is `[P, C]` — one row per spatial position, one column per channel — in `i16` codes at a
//! calibrated scale per site, as a vision tower's rows are (`vision.rs`). The input is the class's canonical image (`u8` HWC
//! at its declared size, `input.image`), whose HWC order IS the rows `[H·W, 3]`; its per-channel normalisation
//! `(x/255 − mean)/std` is one per-channel narrowing — not folded into the first convolution, because a zero-padded stem sees
//! padding in the NORMALISED space.
//!
//! **A convolution is one linear map** (`CONV_2D_V1`): the rows are padded by one zero row, a pinned `Idx` table
//! `[P_out, k·k]` names for every output position and tap the row it reads (the zero row where the tap falls in the padding),
//! one `Gather` makes `[P_out, k·k, C_in]`, and one `MatMul` against the `i8` weight `[k·k·C_in, C_out]` gives the `i64`
//! accumulators, narrowed per output channel. Stride, padding and dilation are the table's; a depthwise convolution
//! (`groups = C_in = C_out`) is a `Mul` against `[k·k, C]` and a `ReduceSum` over the taps. Other groupings are refused.
//!
//! **Batch norm is folded exactly** (`BN_FOLD_V1`): at inference `y = γ(x − μ)/√(σ² + ε) + β` is `W' = W·γ/√(σ² + ε)` and
//! `b' = β − γμ/√(σ² + ε) + (γ/√(σ² + ε))·b`, so the weight codes are those of the folded rows (the per-row scale absorbs the
//! positive factor) and the bias joins the narrowing's `z`: no BN node exists in a program.
//!
//! **A residual unit** (`RESIDUAL_ADD_ACT_V1`): both branches are narrowed into `i32` at one calibrated scale, added exactly,
//! the activation applied and the result narrowed to `i16` codes (ReLU is a clamp at 0 inside that narrowing).
//!
//! **Blocks.** A program has at most 16 blocks of 512 nodes: the top-level units are packed into blocks by an estimate of their
//! size (a unit is never split), and the activation crosses a block boundary as `i32` at the residual scale. A program has ONE
//! carry signature (every layer block reads and writes it), but a network's feature map changes shape from stage to stage, so
//! the carry is the activation FLATTENED (`[E]`, `E` the largest boundary's element count) and zero-padded: a block `Slice`s
//! its input's first elements and reshapes them, and pads its output back (two nodes each way, no arithmetic).

use super::bidir::{input_fill, note_site, rows_val, site_key};
use super::*;
use crate::float_ref::{ParamStore, SiteStat};
use crate::spec::{Act, NormKind};
use crate::weights::{Binding, Src};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The schema an adapter of kind `cnn` instantiates (`CNN_FROM_SPEC_V1`).
pub const CNN_SPEC_SCHEMA_V1: &str = "misaka.palw.cnn-spec.v1";
/// The image input's param name (the vision towers' too: the class's `vision_v2` lifts it).
pub const IMAGE_PARAM: &str = super::vision::IMAGE_PARAM;

fn one() -> usize {
    1
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A batch norm folded into the convolution before it: the checkpoint's tensors `{name}.{weight, bias, running_mean,
/// running_var}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BnOp {
    pub name: String,
    pub eps: f64,
}

/// A 2-D convolution (square kernel), its batch norm and activation: the checkpoint's `{name}.weight` is
/// `[cout, cin / groups, k, k]` and `{name}.bias` (when `bias`) `[cout]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConvOp {
    pub name: String,
    pub cin: usize,
    pub cout: usize,
    pub k: usize,
    #[serde(default = "one")]
    pub stride: usize,
    #[serde(default)]
    pub pad: usize,
    #[serde(default = "one")]
    pub dilation: usize,
    /// The WIDTH axis when it differs from the height's (a 1-D convolution over `[1, T]` is `k: 1, kw: 3`): the kernel, stride,
    /// padding and dilation above are the height's, and the width's unless these say otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kw: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stride_w: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pad_w: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dilation_w: Option<usize>,
    /// TensorFlow's "SAME" padding (MobileNet's `tf_padding`): computed from the input's size — along an axis of extent `n`,
    /// kernel `k` and stride `s`, `max(k − s, 0)` when `n` is a multiple of `s`, else `max(k − n mod s, 0)`, the smaller half
    /// before — and `pad` is ignored. Not combined with a dilation other than 1.
    #[serde(default, skip_serializing_if = "is_false")]
    pub tf_same: bool,
    /// LayerScale: the checkpoint's `[cout]` tensor that multiplies the output channels (ConvNeXt's `layer_scale_parameter`),
    /// folded exactly into the weight rows and the bias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_scale: Option<String>,
    #[serde(default = "one")]
    pub groups: usize,
    #[serde(default)]
    pub bias: bool,
    #[serde(default)]
    pub bn: Option<BnOp>,
    #[serde(default)]
    pub act: Option<Act>,
}

/// One operation of a network.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CnnOp {
    Conv(ConvOp),
    /// Max pooling (`-inf` padding).
    MaxPool { k: usize, stride: usize, pad: usize },
    /// An activation on its own (not fused into a convolution).
    Act(Act),
    /// A LayerNorm over the channels of every position (ConvNeXt's): the checkpoint's `{name}.weight` and `{name}.bias`.
    ChannelNorm { name: String, eps: f64 },
    /// `act(main(x) + shortcut(x))`; an empty shortcut is the identity.
    Residual {
        main: Vec<CnnOp>,
        #[serde(default)]
        shortcut: Vec<CnnOp>,
        #[serde(default)]
        act: Option<Act>,
    },
}

/// What the network outputs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CnnOut {
    /// The last feature map as rows `[P, C]`.
    Map,
    /// The mean over positions `[1, C]` (global average pooling).
    GlobalAvg,
    /// The mean over positions through a LayerNorm over the channels (ConvNeXt's `pooler_output`): the checkpoint's
    /// `{name}.weight` and `{name}.bias`.
    GlobalAvgNorm { name: String, eps: f64 },
}

/// A convolutional network as the lowering reads it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CnnSpec {
    pub architecture: String,
    /// The canonical input size (`u8` HWC RGB), the class's.
    pub h: u32,
    pub w: u32,
    pub mean: [f64; 3],
    pub std: [f64; 3],
    pub ops: Vec<CnnOp>,
    pub out: CnnOut,
    /// Checkpoint tensors the network never reads by design (a classifier head, `num_batches_tracked`).
    #[serde(default)]
    pub ignored: Vec<String>,
    /// Name prefixes a wrapped checkpoint adds (`resnet.` of an image classifier): `[from, to]`, tried when a name is absent.
    #[serde(default)]
    pub aliases: Vec<[String; 2]>,
}

// ───────────────────────────── the plan: ids, geometry, blocks ─────────────────────────────

/// An activation's geometry: `h × w` positions of `c` channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Geo {
    h: usize,
    w: usize,
    c: usize,
}

impl Geo {
    fn rows(&self) -> usize {
        self.h * self.w
    }
}

/// The output extent of a window of `k` (dilation `d`) at stride `s` and padding `p` over `n`.
fn window_out(n: usize, k: usize, s: usize, p_total: usize, d: usize) -> Option<usize> {
    let span = d.checked_mul(k.checked_sub(1)?)?.checked_add(1)?;
    let padded = n.checked_add(p_total)?;
    (padded >= span && s >= 1).then(|| (padded - span) / s + 1)
}

/// TensorFlow's "SAME" padding along one axis: `(before, after)`.
fn tf_pad(n: usize, k: usize, s: usize) -> (usize, usize) {
    let along = if s != 0 && n % s == 0 { k.saturating_sub(s) } else if s != 0 { k.saturating_sub(n % s) } else { 0 };
    (along / 2, along - along / 2)
}

/// A window's geometry on each axis: kernel, stride, zero padding and dilation (height, then width).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Win {
    kh: usize,
    kw: usize,
    sh: usize,
    sw: usize,
    /// Zero padding before and after, per axis.
    ph0: usize,
    ph1: usize,
    pw0: usize,
    pw1: usize,
    dh: usize,
    dw: usize,
}

impl Win {
    fn square(k: usize, stride: usize, pad: usize, dil: usize) -> Self {
        Win { kh: k, kw: k, sh: stride, sw: stride, ph0: pad, ph1: pad, pw0: pad, pw1: pad, dh: dil, dw: dil }
    }
    fn taps(&self) -> usize {
        self.kh * self.kw
    }
    /// The output extent over `inp`, `None` when the window does not fit.
    fn out(&self, inp: Geo, c: usize) -> Option<Geo> {
        Some(Geo { h: window_out(inp.h, self.kh, self.sh, self.ph0 + self.ph1, self.dh)?, w: window_out(inp.w, self.kw, self.sw, self.pw0 + self.pw1, self.dw)?, c })
    }
    /// The table's name: the geometry in and out and the window.
    fn table_name(&self, inp: Geo, out: Geo) -> String {
        format!(
            "cnn.idx.{}x{}.{}x{}k{}x{}s{}x{}p{}.{}x{}.{}d{}x{}",
            inp.h, inp.w, out.h, out.w, self.kh, self.kw, self.sh, self.sw, self.ph0, self.ph1, self.pw0, self.pw1, self.dh, self.dw
        )
    }
}

impl ConvOp {
    /// The window over an input of geometry `inp` (the padding of a `tf_same` convolution depends on it).
    fn win_at(&self, inp: Geo) -> Win {
        let (kw, sw) = (self.kw.unwrap_or(self.k), self.stride_w.unwrap_or(self.stride));
        let ((ph0, ph1), (pw0, pw1)) = if self.tf_same {
            (tf_pad(inp.h, self.k, self.stride), tf_pad(inp.w, kw, sw))
        } else {
            let pw = self.pad_w.unwrap_or(self.pad);
            ((self.pad, self.pad), (pw, pw))
        };
        Win { kh: self.k, kw, sh: self.stride, sw, ph0, ph1, pw0, pw1, dh: self.dilation, dw: self.dilation_w.unwrap_or(self.dilation) }
    }

    /// Taps per output channel and input channel (`kh · kw`).
    fn taps(&self) -> usize {
        self.k * self.kw.unwrap_or(self.k)
    }
}

/// One node of the plan: an op with its id, geometry in and out, and children for a residual unit.
#[derive(Clone, Debug)]
struct PNode {
    /// A unique id (`c0`, `p1`, `r2`, …): the HL param and site names.
    id: String,
    op: PKind,
    inp: Geo,
    out: Geo,
}

#[derive(Clone, Debug)]
enum PKind {
    Conv(ConvOp),
    MaxPool { k: usize, stride: usize, pad: usize },
    Act(Act),
    ChannelNorm { eps: f64, name: String },
    Residual { main: Vec<PNode>, shortcut: Vec<PNode>, act: Option<Act> },
}

/// What an untrusted spec may declare, bounded BEFORE anything is sized on it (every TIR dimension is at most 2^24; a refusal
/// names the bound).
const MAX_OPS: usize = 4096;
const MAX_KERNEL: usize = 63;
const MAX_STRIDE: usize = 64;
const MAX_DILATION: usize = 32;
const MAX_PAD: usize = MAX_KERNEL * MAX_DILATION;
const MAX_CHANNELS: usize = 1 << 20;
/// A feature map's positions, and a carried activation's elements (the flat carry), are one TIR dimension each.
const MAX_ELEMS: usize = 1 << 24;
/// Entries of one window table (an `Idx` param): positions × taps.
const MAX_TABLE: usize = 1 << 26;

/// An op's output extent against the bounds: positions, carried elements and window-table entries.
fn check_extent(what: &str, g: Geo, taps: usize) -> Result<()> {
    let rows = g.rows();
    if rows > MAX_ELEMS || rows.saturating_mul(g.c) > MAX_ELEMS || rows.saturating_mul(taps) > MAX_TABLE {
        return Err(LowerError::not_lowerable(format!(
            "{what}: a {}×{} map of {} channels with {taps} taps is past the bounds (2^24 positions and carried elements, 2^26 window-table entries)",
            g.h, g.w, g.c
        )));
    }
    Ok(())
}

/// The ids and geometry of every op, depth first.
fn plan_ops(ops: &[CnnOp], mut geo: Geo, counter: &mut usize) -> Result<(Vec<PNode>, Geo)> {
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        let n = *counter;
        *counter += 1;
        if *counter > MAX_OPS {
            return Err(LowerError::not_lowerable(format!("a network of more than {MAX_OPS} ops")));
        }
        let (id, kind, g_out) = match op {
            CnnOp::Conv(c) => {
                let w = c.win_at(geo);
                if c.tf_same && (w.dh != 1 || w.dw != 1) {
                    return Err(LowerError::bad(format!("convolution `{}`: TensorFlow SAME padding with a dilation other than 1", c.name)));
                }
                if [w.kh, w.kw, w.sh, w.sw, w.dh, w.dw, c.groups, c.cout, c.cin].contains(&0) || c.cin % c.groups != 0 || c.cout % c.groups != 0 {
                    return Err(LowerError::bad(format!("convolution `{}`: kernel, stride, dilation, groups and channels must be positive and divisible", c.name)));
                }
                if w.kh.max(w.kw) > MAX_KERNEL
                    || w.sh.max(w.sw) > MAX_STRIDE
                    || w.dh.max(w.dw) > MAX_DILATION
                    || w.ph0.max(w.ph1).max(w.pw0).max(w.pw1) > MAX_PAD
                    || c.cin > MAX_CHANNELS
                    || c.cout > MAX_CHANNELS
                {
                    return Err(LowerError::not_lowerable(format!(
                        "convolution `{}`: past the bounds (kernel {MAX_KERNEL}, stride {MAX_STRIDE}, dilation {MAX_DILATION}, padding {MAX_PAD}, {MAX_CHANNELS} channels)",
                        c.name
                    )));
                }
                if c.cin != geo.c {
                    return Err(LowerError::bad(format!("convolution `{}` reads {} channels, its input has {}", c.name, c.cin, geo.c)));
                }
                if c.groups != 1 && !(c.groups == c.cin && c.cin == c.cout) {
                    return Err(LowerError::not_lowerable(format!("convolution `{}`: groups = {} (only 1 and depthwise are lowered)", c.name, c.groups)));
                }
                let Some(g) = w.out(geo, c.cout) else {
                    return Err(LowerError::not_lowerable(format!("convolution `{}`: a {}×{} input is smaller than its window", c.name, geo.h, geo.w)));
                };
                check_extent(&format!("convolution `{}`", c.name), g, w.taps())?;
                // The weight matrix `[k·k·C_in, C_out]` and the gathered columns' taps are TIR dimensions too.
                if w.taps().saturating_mul(c.cin) > MAX_ELEMS {
                    return Err(LowerError::not_lowerable(format!("convolution `{}`: {} taps over {} channels is past 2^24", c.name, w.taps(), c.cin)));
                }
                (format!("c{n}"), PKind::Conv(c.clone()), g)
            }
            CnnOp::MaxPool { k, stride, pad } => {
                if *k == 0 || *stride == 0 {
                    return Err(LowerError::bad("a max pool's window and stride must be positive"));
                }
                if *k > MAX_KERNEL || *stride > MAX_STRIDE {
                    return Err(LowerError::not_lowerable(format!("a max pool past the bounds (window {MAX_KERNEL}, stride {MAX_STRIDE})")));
                }
                if *pad * 2 > *k {
                    return Err(LowerError::bad("a max pool's padding must be at most half its window"));
                }
                let Some(g) = Win::square(*k, *stride, *pad, 1).out(geo, geo.c) else {
                    return Err(LowerError::not_lowerable("a max pool window larger than its input"));
                };
                check_extent("a max pool", g, k * k)?;
                (format!("p{n}"), PKind::MaxPool { k: *k, stride: *stride, pad: *pad }, g)
            }
            CnnOp::Act(a) => (format!("a{n}"), PKind::Act(*a), geo),
            CnnOp::ChannelNorm { name, eps } => {
                if !eps.is_finite() || *eps <= 0.0 {
                    return Err(LowerError::bad(format!("channel norm `{name}`: eps {eps} is not a positive number")));
                }
                (format!("n{n}"), PKind::ChannelNorm { eps: *eps, name: name.clone() }, geo)
            }
            CnnOp::Residual { main, shortcut, act } => {
                let (m, gm) = plan_ops(main, geo, counter)?;
                let (s, gs) = plan_ops(shortcut, geo, counter)?;
                if gm != gs {
                    return Err(LowerError::bad(format!("a residual unit's branches disagree: {gm:?} and {gs:?}")));
                }
                (format!("r{n}"), PKind::Residual { main: m, shortcut: s, act: *act }, gm)
            }
        };
        out.push(PNode { id, op: kind, inp: geo, out: g_out });
        geo = g_out;
    }
    Ok((out, geo))
}

/// The nodes a unit lowers to, estimated: MEASURED about 16.5 a convolution on ResNet-18 (339 nodes for 20 convolutions with
/// the input's normalisation), so a convolution counts 22, a pool or an activation 12, a residual unit 10 more.
fn estimate(n: &PNode) -> usize {
    match &n.op {
        PKind::Conv(_) => 22,
        PKind::MaxPool { .. } | PKind::Act(_) => 12,
        PKind::ChannelNorm { .. } => 32,
        PKind::Residual { main, shortcut, .. } => 10 + main.iter().chain(shortcut).map(estimate).sum::<usize>(),
    }
}

/// The nodes a block may hold before the next top-level unit starts another block (512 is the normal form's cap).
const BLOCK_BUDGET: usize = 400;
/// The most nodes one top-level unit may be estimated at (a unit is never split across blocks).
const MAX_UNIT: usize = 440;
/// The most blocks a program has (normal form).
const MAX_BLOCKS: usize = 16;

/// The whole plan: the ops, the final geometry, and the top-level units grouped into blocks (the first group runs in the
/// `pre` block, after the input's normalisation, the last in the last layer block; the `post` block makes the output).
struct Plan {
    nodes: Vec<PNode>,
    last: Geo,
    /// Top-level node index ranges, one per block after `pre` and before `post`; group 0 is `pre`'s.
    groups: Vec<std::ops::Range<usize>>,
}

fn plan(spec: &CnnSpec) -> Result<Plan> {
    if spec.h == 0 || spec.w == 0 || spec.h > 1 << 14 || spec.w > 1 << 14 || spec.ops.is_empty() {
        return Err(LowerError::bad(format!("{}: an input of {}×{} and {} ops", spec.architecture, spec.h, spec.w, spec.ops.len())));
    }
    if spec.h as usize * spec.w as usize > MAX_ELEMS {
        return Err(LowerError::not_lowerable(format!("{}: an input of {}×{} pixels (more than 2^24 positions)", spec.architecture, spec.h, spec.w)));
    }
    if spec.std.iter().any(|s| *s <= 0.0 || !s.is_finite()) || spec.mean.iter().any(|m| !m.is_finite()) {
        return Err(LowerError::bad("a normalisation mean or std that is not finite and positive"));
    }
    let mut counter = 0;
    let (nodes, last) = plan_ops(&spec.ops, Geo { h: spec.h as usize, w: spec.w as usize, c: 3 }, &mut counter)?;
    if let Some(big) = nodes.iter().find(|n| estimate(n) > MAX_UNIT) {
        return Err(LowerError::not_lowerable(format!("{}: unit `{}` is about {} nodes — a block has 512 and a unit is never split", spec.architecture, big.id, estimate(big))));
    }
    // Pack: the pre block also holds the normalisation (about 14 nodes).
    let mut groups: Vec<std::ops::Range<usize>> = Vec::new();
    let (mut start, mut used) = (0, 14usize);
    for (i, n) in nodes.iter().enumerate() {
        let e = estimate(n);
        if used > 0 && used + e > BLOCK_BUDGET && i > start {
            groups.push(start..i);
            start = i;
            used = 0;
        }
        used += e;
    }
    groups.push(start..nodes.len());
    if groups.len() + 1 > MAX_BLOCKS {
        return Err(LowerError::not_lowerable(format!("{}: about {} blocks of 400 nodes (a program has at most {MAX_BLOCKS})", spec.architecture, groups.len() + 1)));
    }
    Ok(Plan { nodes, last, groups })
}

/// Visit every convolution of a plan (depth first, branches included) in id order.
fn convs<'a>(nodes: &'a [PNode], f: &mut impl FnMut(&'a PNode, &'a ConvOp)) {
    for n in nodes {
        match &n.op {
            PKind::Conv(c) => f(n, c),
            PKind::Residual { main, shortcut, .. } => {
                convs(main, f);
                convs(shortcut, f);
            }
            _ => {}
        }
    }
}

/// Visit every node of a plan (depth first, branches included).
fn walk_nodes<'a>(nodes: &'a [PNode], f: &mut impl FnMut(&'a PNode)) {
    for n in nodes {
        f(n);
        if let PKind::Residual { main, shortcut, .. } = &n.op {
            walk_nodes(main, f);
            walk_nodes(shortcut, f);
        }
    }
}

// ───────────────────────────── params ─────────────────────────────

/// A convolution's HL param names.
fn pn(id: &str, what: &str) -> String {
    format!("{id}.{what}")
}

/// The HL params of the network: `(name, shape, source)` for every convolution's weight (`[cout, cin/groups·k·k]`), bias
/// and batch-norm vectors, each global (a convolution runs once).
fn param_table(spec: &CnnSpec, p: &Plan) -> Vec<(String, Vec<usize>, Src)> {
    let mut v = Vec::new();
    convs(&p.nodes, &mut |n, c| {
        let taps = c.cin / c.groups * c.taps();
        v.push((pn(&n.id, "w"), vec![c.cout, taps], Src::t(format!("{}.weight", c.name)).reshape(vec![c.cout, taps])));
        if c.bias {
            v.push((pn(&n.id, "b"), vec![c.cout], Src::t(format!("{}.bias", c.name))));
        }
        if let Some(bn) = &c.bn {
            v.push((pn(&n.id, "bn.gain"), vec![c.cout], Src::t(format!("{}.weight", bn.name))));
            v.push((pn(&n.id, "bn.bias"), vec![c.cout], Src::t(format!("{}.bias", bn.name))));
            v.push((pn(&n.id, "bn.mean"), vec![c.cout], Src::t(format!("{}.running_mean", bn.name))));
            v.push((pn(&n.id, "bn.var"), vec![c.cout], Src::t(format!("{}.running_var", bn.name))));
        }
        if let Some(ls) = &c.layer_scale {
            v.push((pn(&n.id, "ls"), vec![c.cout], Src::t(ls.clone())));
        }
    });
    // A channel norm's gain and bias, and the pooled output's.
    walk_nodes(&p.nodes, &mut |n| {
        if let PKind::ChannelNorm { name, .. } = &n.op {
            v.push((pn(&n.id, "gain"), vec![n.out.c], Src::t(format!("{name}.weight"))));
            v.push((pn(&n.id, "bias"), vec![n.out.c], Src::t(format!("{name}.bias"))));
        }
    });
    if let CnnOut::GlobalAvgNorm { name, .. } = &spec.out {
        v.push(("pnorm.gain".into(), vec![p.last.c], Src::t(format!("{name}.weight"))));
        v.push(("pnorm.bias".into(), vec![p.last.c], Src::t(format!("{name}.bias"))));
    }
    v
}

/// The synthetic HL program of a network: its params, one anchor node per block referencing the params that block's
/// convolutions read, and the binding.
pub fn hl_program(spec: &CnnSpec) -> Result<(HlProgram, Binding)> {
    use crate::hl::{Block, BlockRole, CarryDecl, HlType, Init, Node, ParamDecl, Ref};
    let p = plan(spec)?;
    let table = param_table(spec, &p);
    let params: Vec<ParamDecl> = table.iter().map(|(n, sh, _)| ParamDecl { name: n.clone(), shape: sh.clone(), per_layer: false, init: Init::Normal(0.1) }).collect();
    // The params a block's nodes read: those named after the ids of its nodes (`c3.w`, `n5.gain`, `c3.ls`).
    let group_params = |range: std::ops::Range<usize>| -> Vec<Ref> {
        let mut ids = std::collections::BTreeSet::new();
        walk_nodes(&p.nodes[range], &mut |n| {
            ids.insert(n.id.clone());
        });
        table.iter().enumerate().filter(|(_, (name, _, _))| name.split('.').next().is_some_and(|id| ids.contains(id))).map(|(i, _)| Ref::Param(i as u32)).collect()
    };
    let post_params: Vec<Ref> = table.iter().enumerate().filter(|(_, (name, _, _))| name.starts_with("pnorm.")).map(|(i, _)| Ref::Param(i as u32)).collect();
    let anchor = |inputs: Vec<Ref>| Node {
        op: crate::hl::Op::Scale { c: 1.0 },
        inputs,
        outs: vec![vec![1]],
        out_types: vec![HlType::F32],
        site: None,
        writes: vec![],
    };
    let mut blocks = vec![Block { name: "pre".into(), role: BlockRole::Pre, nodes: vec![anchor(group_params(p.groups[0].clone()))], outputs: vec![Ref::Node(0, 0)] }];
    let mut schedule = Vec::new();
    for (g, range) in p.groups.iter().enumerate().skip(1) {
        blocks.push(Block { name: format!("g{g}"), role: BlockRole::Layer, nodes: vec![anchor(group_params(range.clone()))], outputs: vec![Ref::Node(0, 0)] });
        schedule.push((blocks.len() - 1) as u16);
    }
    blocks.push(Block { name: "post".into(), role: BlockRole::Post, nodes: vec![anchor(post_params)], outputs: vec![Ref::Node(0, 0)] });
    let post = blocks.len() - 1;
    let hl = HlProgram {
        architecture: spec.architecture.clone(),
        output: crate::hl::HlOutput::Embedding { normalized: false },
        vocab: 1,
        hidden: p.last.c,
        carries: vec![CarryDecl { name: "rows".into(), shape: vec![1, 1], resid: false }],
        params,
        states: vec![],
        rope_tables: vec![],
        blocks,
        pre: 0,
        post,
        layer_of: (0..schedule.len()).collect(),
        schedule,
    };
    hl.validate().map_err(|e| LowerError::eval(format!("internal: the network's HL program: {e}")))?;
    let mut ignored: Vec<String> = spec.ignored.clone();
    convs(&p.nodes, &mut |_, c| {
        if let Some(bn) = &c.bn {
            // A batch norm's step counter is training state: unread by design, under the name a wrapped checkpoint gives it too
            // (`mobilenet_v2.conv_1x1.normalization.num_batches_tracked` of an image classifier).
            let t = format!("{}.num_batches_tracked", bn.name);
            for [from, to] in &spec.aliases {
                if let Some(rest) = t.strip_prefix(from.as_str()) {
                    ignored.push(format!("{to}{rest}"));
                }
            }
            ignored.push(t);
        }
    });
    let aliases = spec.aliases.iter().map(|[a, b]| (a.clone(), b.clone())).collect();
    let binding = Binding { srcs: table.into_iter().map(|(_, _, s)| s).collect(), aliases, ignored_prefixes: ignored };
    Ok((hl, binding))
}

// ───────────────────────────── the im2col tables ─────────────────────────────

/// For every output position `(oh, ow)` and tap `(kh, kw)` the input row `ih·W + iw` it reads, `rows` (the index of the padding
/// row) where the tap falls outside.
fn window_index(inp: Geo, out: Geo, w: &Win) -> Vec<u32> {
    let pad_row = inp.rows() as u32;
    let mut v = Vec::with_capacity(out.rows() * w.taps());
    for oh in 0..out.h {
        for ow in 0..out.w {
            for kh in 0..w.kh {
                for kw in 0..w.kw {
                    let (ih, iw) = ((oh * w.sh + kh * w.dh) as isize - w.ph0 as isize, (ow * w.sw + kw * w.dw) as isize - w.pw0 as isize);
                    v.push(if ih < 0 || iw < 0 || ih >= inp.h as isize || iw >= inp.w as isize { pad_row } else { (ih as usize * inp.w + iw as usize) as u32 });
                }
            }
        }
    }
    v
}

// ───────────────────────────── the float reference ─────────────────────────────

/// A convolution's folded float weights `[cout][taps]` (`(cin/groups, kh, kw)` order) and bias `[cout]`: the batch norm folded
/// exactly (`BN_FOLD_V1`).
fn folded(c: &ConvOp, w: &[f32], bias: Option<&[f32]>, bn: Option<(&[f32], &[f32], &[f32], &[f32])>, layer_scale: Option<&[f32]>) -> (Vec<f32>, Vec<f64>) {
    let taps = c.cin / c.groups * c.taps();
    let mut out = w.to_vec();
    let mut b = vec![0f64; c.cout];
    for o in 0..c.cout {
        let (mut s, mut shift) = (1.0f64, 0.0f64);
        if let (Some((g, be, mu, var)), Some(op)) = (bn, &c.bn) {
            s = g[o] as f64 / (var[o] as f64 + op.eps).sqrt();
            shift = be[o] as f64 - s * mu[o] as f64;
        }
        for t in 0..taps {
            out[o * taps + t] = (w[o * taps + t] as f64 * s) as f32;
        }
        b[o] = shift + s * bias.map_or(0.0, |b| b[o] as f64);
        // LayerScale multiplies the whole output channel, bias included: exact.
        if let Some(ls) = layer_scale {
            let g = ls[o] as f64;
            for t in 0..taps {
                out[o * taps + t] = (out[o * taps + t] as f64 * g) as f32;
            }
            b[o] *= g;
        }
    }
    (out, b)
}

fn apply_act(a: Act, x: f64) -> f64 {
    crate::float_ref::act(a, x as f32) as f64
}

/// LayerNorm of one row over its channels: `(x − μ)/√(σ² + ε)·γ + β`.
fn layer_norm_row(x: &[f64], g: &[f32], b: &[f32], eps: f64) -> Vec<f64> {
    let n = x.len() as f64;
    let mu = x.iter().sum::<f64>() / n;
    let var = x.iter().map(|v| (v - mu) * (v - mu)).sum::<f64>() / n;
    let inv = 1.0 / (var + eps).sqrt();
    x.iter().enumerate().map(|(i, v)| (v - mu) * inv * g[i] as f64 + b[i] as f64).collect()
}

/// A convolution over rows in `f64`: `x` is `[P_in][C_in]`, the weight `fw` `[C_out][C_in/groups · kh · kw]` (batch norm already
/// folded) and the offsets `fb`, the result `[P_out][C_out]` before the activation.
fn conv_float(x: &[Vec<f64>], inp: Geo, out: Geo, c: &ConvOp, fw: &[f32], fb: &[f64]) -> Vec<Vec<f64>> {
    let win = c.win_at(inp);
    let kk = win.taps();
    let taps = c.cin / c.groups * kk;
    let idx = window_index(inp, out, &win);
    let cin_g = c.cin / c.groups;
    (0..out.rows())
        .map(|po| {
            (0..c.cout)
                .map(|o| {
                    let g = o / (c.cout / c.groups);
                    let mut acc = fb[o];
                    for t in 0..kk {
                        let src = idx[po * kk + t] as usize;
                        if src >= x.len() {
                            continue;
                        }
                        for ci in 0..cin_g {
                            acc += x[src][g * cin_g + ci] * fw[o * taps + ci * kk + t] as f64;
                        }
                    }
                    acc
                })
                .collect()
        })
        .collect()
}

impl Default for ConvOp {
    fn default() -> Self {
        ConvOp {
            name: String::new(),
            cin: 1,
            cout: 1,
            k: 1,
            stride: 1,
            pad: 0,
            dilation: 1,
            kw: None,
            stride_w: None,
            pad_w: None,
            dilation_w: None,
            tf_same: false,
            layer_scale: None,
            groups: 1,
            bias: false,
            bn: None,
            act: None,
        }
    }
}

impl ConvOp {
    /// A 1-D convolution over `[1, T]` (kernel `k`, stride and zero padding along the width), with a bias and no batch norm: what
    /// an audio front end's stem is (`Conv1d`).
    pub fn conv1d(name: &str, cin: usize, cout: usize, k: usize, stride: usize, pad: usize, bias: bool) -> Self {
        ConvOp {
            name: name.into(),
            cin,
            cout,
            k: 1,
            stride: 1,
            pad: 0,
            dilation: 1,
            kw: Some(k),
            stride_w: Some(stride),
            pad_w: Some(pad),
            dilation_w: None,
            tf_same: false,
            layer_scale: None,
            groups: 1,
            bias,
            bn: None,
            act: None,
        }
    }
}

/// The length of a 1-D convolution's output over `t` rows (`None` when the window does not fit).
pub(super) fn conv1d_out_len(t: usize, c: &ConvOp) -> Option<usize> {
    let inp = Geo { h: 1, w: t, c: c.cin };
    c.win_at(inp).out(inp, c.cout).map(|g| g.w)
}

/// A 1-D convolution of `x` (`[T][C_in]` rows) in `f64`: the checkpoint's `weight` (`[C_out, C_in, k]`, flat) and `bias`.
pub(super) fn conv1d_float(x: &[Vec<f64>], c: &ConvOp, w: &[f32], bias: Option<&[f32]>) -> Result<Vec<Vec<f64>>> {
    let inp = Geo { h: 1, w: x.len(), c: c.cin };
    let out = c.win_at(inp).out(inp, c.cout).ok_or_else(|| LowerError::bad(format!("convolution `{}`: {} frames are shorter than its window", c.name, x.len())))?;
    let (fw, fb) = folded(c, w, bias, None, None);
    Ok(conv_float(x, inp, out, c, &fw, &fb))
}

/// A 1-D convolution as the program: `x` is `[T, C_in]` rows of codes, the result `[T_out, C_out]` narrowed as `want` says. The
/// HL params are `{id}.w` (`[C_out, C_in·k]`) and, with a bias, `{id}.b`.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_conv1d(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, x: &Val, id: &str, c: &ConvOp, t: usize, want: &Want, relu: bool) -> Result<Val> {
    let inp = Geo { h: 1, w: t, c: c.cin };
    let out = c.win_at(inp).out(inp, c.cout).ok_or_else(|| LowerError::bad(format!("convolution `{}`: {t} frames are shorter than its window", c.name)))?;
    let n = PNode { id: id.to_string(), op: PKind::Conv(c.clone()), inp, out };
    lower_conv(b, cx, lb, x, &n, c, want, relu)
}

/// The float network over one canonical image (`u8` HWC): the output rows, and, when `stats` is given, every site's statistics
/// under the lowering's names (`pre.in`, `pre.c0`, `L0.r5.sum`, `post.out`, …).
pub fn float_forward(hl: &HlProgram, spec: &CnnSpec, params: &ParamStore, image: &[u8], mut stats: Option<&mut BTreeMap<String, SiteStat>>) -> Result<Vec<Vec<f64>>> {
    let p = plan(spec)?;
    let (h, w) = (spec.h as usize, spec.w as usize);
    if image.len() != h * w * 3 {
        return Err(LowerError::eval(format!("an image of {} bytes for {h}×{w}×3", image.len())));
    }
    let get = |name: &str| -> Result<Vec<f32>> { Ok(params.get(super::bidir::hl_param(hl, name)?, None)?.data.clone()) };
    let mut observe = |key: String, rows: &[Vec<f64>]| {
        if let Some(st) = stats.as_deref_mut() {
            let e = st.entry(key).or_default();
            let width = rows.first().map_or(0, |r| r.len());
            if e.count == 0 {
                e.chan_absmax = vec![0.0; width];
            }
            for (i, r) in rows.iter().enumerate() {
                let mut row = 0f64;
                for (c, x) in r.iter().enumerate() {
                    let ax = x.abs();
                    row = row.max(ax);
                    e.sum_sq += ax * ax;
                    if let Some(ch) = e.chan_absmax.get_mut(c) {
                        *ch = ch.max(ax as f32);
                    }
                }
                e.absmax = e.absmax.max(row);
                if i == 0 { e.pos0_absmax = e.pos0_absmax.max(row) } else { e.rest_absmax = e.rest_absmax.max(row) }
                e.count += r.len() as u64;
            }
        }
    };
    // The input's per-channel normalisation.
    let mut x: Vec<Vec<f64>> = (0..h * w).map(|i| (0..3).map(|c| (image[i * 3 + c] as f64 / 255.0 - spec.mean[c]) / spec.std[c]).collect()).collect();
    observe("pre.in".into(), &x);
    /// A value and its activation, observed under the names the lowering reads: a ReLU is a clamp inside the narrowing
    /// (one site: the activated value), a table activation is a site of its own after the pre-activation one.
    fn activated(pre: &[Vec<f64>], act: Option<Act>, id: &str, prefix: &str, observe: &mut dyn FnMut(String, &[Vec<f64>])) -> Vec<Vec<f64>> {
        let post: Vec<Vec<f64>> = pre.iter().map(|r| r.iter().map(|v| act.map_or(*v, |a| apply_act(a, *v))).collect()).collect();
        match act {
            Some(a) if !matches!(a, Act::Relu | Act::Identity) => {
                observe(format!("{prefix}{id}"), pre);
                observe(format!("{prefix}{id}.act"), &post);
            }
            _ => observe(format!("{prefix}{id}"), &post),
        }
        post
    }
    fn run(
        nodes: &[PNode],
        mut x: Vec<Vec<f64>>,
        prefix: &str,
        get: &dyn Fn(&str) -> Result<Vec<f32>>,
        observe: &mut dyn FnMut(String, &[Vec<f64>]),
    ) -> Result<Vec<Vec<f64>>> {
        for n in nodes {
            x = match &n.op {
                PKind::Conv(c) => {
                    let w = get(&pn(&n.id, "w"))?;
                    let bias = if c.bias { Some(get(&pn(&n.id, "b"))?) } else { None };
                    let bn = match &c.bn {
                        Some(_) => Some((get(&pn(&n.id, "bn.gain"))?, get(&pn(&n.id, "bn.bias"))?, get(&pn(&n.id, "bn.mean"))?, get(&pn(&n.id, "bn.var"))?)),
                        None => None,
                    };
                    let ls = if c.layer_scale.is_some() { Some(get(&pn(&n.id, "ls"))?) } else { None };
                    let (fw, fb) = folded(c, &w, bias.as_deref(), bn.as_ref().map(|(a, b, m, v)| (&a[..], &b[..], &m[..], &v[..])), ls.as_deref());
                    let pre = conv_float(&x, n.inp, n.out, c, &fw, &fb);
                    activated(&pre, c.act, &n.id, prefix, observe)
                }
                PKind::MaxPool { k, stride, pad } => {
                    let idx = window_index(n.inp, n.out, &Win::square(*k, *stride, *pad, 1));
                    let kk = k * k;
                    let rows: Vec<Vec<f64>> = (0..n.out.rows())
                        .map(|po| (0..n.inp.c).map(|ch| (0..kk).filter_map(|t| x.get(idx[po * kk + t] as usize).map(|r| r[ch])).fold(f64::NEG_INFINITY, f64::max)).collect())
                        .collect();
                    observe(format!("{prefix}{}", n.id), &rows);
                    rows
                }
                PKind::Act(a) => {
                    let rows: Vec<Vec<f64>> = x.iter().map(|r| r.iter().map(|v| apply_act(*a, *v)).collect()).collect();
                    observe(format!("{prefix}{}", n.id), &rows);
                    rows
                }
                PKind::ChannelNorm { eps, .. } => {
                    let (g, b) = (get(&pn(&n.id, "gain"))?, get(&pn(&n.id, "bias"))?);
                    let rows: Vec<Vec<f64>> = x.iter().map(|r| layer_norm_row(r, &g, &b, *eps)).collect();
                    observe(format!("{prefix}{}", n.id), &rows);
                    rows
                }
                PKind::Residual { main, shortcut, act } => {
                    let m = run(main, x.clone(), prefix, get, observe)?;
                    let s = run(shortcut, x.clone(), prefix, get, observe)?;
                    let sum: Vec<Vec<f64>> = m.iter().zip(&s).map(|(a, b)| a.iter().zip(b).map(|(a, b)| a + b).collect()).collect();
                    observe(format!("{prefix}{}.sum", n.id), &sum);
                    activated(&sum, *act, &n.id, prefix, observe)
                }
            };
        }
        Ok(x)
    }
    for (g, range) in p.groups.iter().enumerate() {
        let prefix = if g == 0 { "pre.".to_string() } else { format!("L{}.", g - 1) };
        if g > 0 {
            observe(format!("{prefix}carry{g}"), &x);
        }
        x = run(&p.nodes[range.clone()], x, &prefix, &get, &mut observe)?;
        // The activation crossing the block boundary is a residual-scale value (the last group's too: the post block reads it).
        observe(format!("{prefix}out{g}"), &x);
    }
    observe("post.carry0".into(), &x);
    let out: Vec<Vec<f64>> = match &spec.out {
        CnnOut::Map => x,
        CnnOut::GlobalAvg => {
            let n = x.len() as f64;
            vec![(0..p.last.c).map(|c| x.iter().map(|r| r[c]).sum::<f64>() / n).collect()]
        }
        CnnOut::GlobalAvgNorm { eps, .. } => {
            let n = x.len() as f64;
            let mean: Vec<f64> = (0..p.last.c).map(|c| x.iter().map(|r| r[c]).sum::<f64>() / n).collect();
            observe("post.mean".into(), std::slice::from_ref(&mean));
            vec![layer_norm_row(&mean, &get("pnorm.gain")?, &get("pnorm.bias")?, *eps)]
        }
    };
    observe("post.out".into(), &out);
    Ok(out)
}

// ───────────────────────────── the lowering ─────────────────────────────

fn out_key() -> ScaleKey {
    ScaleKey { base: Base::Pow2Site { names: vec!["out".into()] }, factor: 1.0 }
}

fn codes_want(site: &str) -> Want {
    Want { dt: DType::I16, key: site_key(site) }
}

/// The narrowing `N(x; m, s, z)` into `[lo, hi]` of `dt`.
fn narrow_into(b: &mut BlockBuilder<'_>, x: tir::Ref, m: tir::Ref, s: tir::Ref, z: Option<tir::Ref>, lo: i64, hi: i64, dt: DType) -> tir::Ref {
    b.narrow(x, &tir::library::Narrowing::new(m, s, z), lo, hi, dt)
}

/// A convolution's quantised weight rows: the codes of every output row (`i8` ones widened, or `i16`) and each row's scale. Which
/// width a program stores is [`LowerOpts::conv_weight_bits`]; the fill makes the tensor of the dtype the program declared.
struct WeightCodes {
    codes: Vec<i16>,
    scales: Vec<f64>,
    /// The width the program declared for the weight parameter: 16 bits when set, else 8.
    wide: bool,
}

impl WeightCodes {
    /// The matmul operand of `shape` over `t` (the rows' codes laid out for it) in the declared width.
    fn tensor(&self, shape: Vec<usize>, t: Vec<i16>) -> IntTensor {
        if self.wide { IntTensor::i16(shape, t) } else { IntTensor::i8(shape, t.into_iter().map(|c| c as i8).collect()) }
    }
}

/// A convolution: rows `x` (`i16` codes, `[P_in, C_in]`) to rows `[P_out, C_out]` narrowed as `want` says (`lo` 0 under a ReLU).
fn lower_conv(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, x: &Val, n: &PNode, c: &ConvOp, want: &Want, relu: bool) -> Result<Val> {
    let hl = cx.hl;
    let (inp, out) = (n.inp, n.out);
    let id = n.id.clone();
    let xc = super::bidir::codes_rows(b, cx, lb, x)?;
    if xc.len != c.cin {
        return Err(LowerError::eval(format!("internal: `{id}` reads {} columns, its weight has {}", xc.len, c.cin)));
    }
    let win = c.win_at(inp);
    let kk = win.taps();
    let pad_row = b.pb.konst(DType::I16, &[1, c.cin as u32], &vec![0i128; c.cin]);
    let padded = b.concat(&[xc.r, pad_row], 0);
    let ix = window_index(inp, out, &win);
    let (po, rows_in) = (out.rows(), inp.rows());
    let iname = win.table_name(inp, out);
    let idx = decl(b, cx, lb, &iname, DType::Idx, &[po, kk], false, Arc::new(move |_| Ok(IntTensor::idx(vec![po, kk], ix.clone()))))?;
    let idx = b.clamp(idx, 0, rows_in as i64, DType::Idx); // the padding row is the last (never fires: the table is data the lowering wrote)
    let cols = b.gather(padded, idx, 0, 0); // [P_out, k·k, C_in]
    let depthwise = c.groups > 1;
    let taps = c.cin / c.groups * kk;
    let (wp, bp) = (super::bidir::hl_param(hl, &pn(&id, "w"))?, if c.bias { Some(super::bidir::hl_param(hl, &pn(&id, "b"))?) } else { None });
    let bnp = match &c.bn {
        Some(_) => Some([pn(&id, "bn.gain"), pn(&id, "bn.bias"), pn(&id, "bn.mean"), pn(&id, "bn.var")].map(|s| super::bidir::hl_param(hl, &s)).into_iter().collect::<Result<Vec<u32>>>()?),
        None => None,
    };
    // The folded, quantised weight in the matmul's layout, and the per-row scales: made from the float params by every fill
    // that needs them (the same function, so the same bytes).
    let lsp = if c.layer_scale.is_some() { Some(super::bidir::hl_param(hl, &pn(&id, "ls"))?) } else { None };
    let (cc, cin, cout) = (c.clone(), c.cin, c.cout);
    // The registrant's width for the weight codes: i8 per output row (the default) or i16 per output row. Both are the same
    // matmul (or product and sum) over wider or narrower integers; only the weight parameter's dtype differs.
    let wide = cx.conv_weight_bits == 16;
    let wdt = if wide { DType::I16 } else { DType::I8 };
    let fold = Arc::new(move |fc: &FillCtx<'_>| -> Result<(WeightCodes, Vec<f64>)> {
        let w = fc.f(wp)?.data.clone();
        let ls = match lsp {
            Some(p) => Some(fc.f(p)?.data.clone()),
            None => None,
        };
        let bias = match bp {
            Some(p) => Some(fc.f(p)?.data.clone()),
            None => None,
        };
        let bn = match &bnp {
            Some(ids) => Some([fc.f(ids[0])?.data.clone(), fc.f(ids[1])?.data.clone(), fc.f(ids[2])?.data.clone(), fc.f(ids[3])?.data.clone()]),
            None => None,
        };
        let (fw, fb) = folded(&cc, &w, bias.as_deref(), bn.as_ref().map(|v| (&v[0][..], &v[1][..], &v[2][..], &v[3][..])), ls.as_deref());
        let codes = if wide {
            let q = crate::quant::quantize_rows16(&fw, cout, taps);
            WeightCodes { codes: q.codes, scales: q.scales, wide: true }
        } else {
            let q = crate::quant::quantize_rows(&fw, cout, taps, None);
            WeightCodes { codes: q.codes.iter().map(|c| *c as i16).collect(), scales: q.scales, wide: false }
        };
        Ok((codes, fb))
    });
    let f1 = fold.clone();
    let wname = format!("{id}.w.t");
    let acc = if depthwise {
        // [k·k, C] codes: channel `ch`'s taps are its row.
        let wd = decl(
            b,
            cx,
            lb,
            &wname,
            wdt,
            &[kk, cin],
            false,
            Arc::new(move |fc| {
                let (rc, _) = f1(fc)?;
                let mut t = vec![0i16; kk * cin];
                for ch in 0..cin {
                    for tap in 0..kk {
                        t[tap * cin + ch] = rc.codes[ch * kk + tap];
                    }
                }
                Ok(rc.tensor(vec![kk, cin], t))
            }),
        )?;
        let prod = b.mul(cols, wd, DType::I64); // [P_out, k·k, C]
        let sum = b.reduce_sum(prod, 1, DType::I64); // [P_out, 1, C]
        b.reshape_fixed(sum, &[po as u32, c.cout as u32])
    } else {
        // [k·k·C_in, C_out] codes: row `(tap, ci)` of the matmul's right operand.
        let cin_g = c.cin;
        let wt = decl(
            b,
            cx,
            lb,
            &wname,
            wdt,
            &[kk * cin_g, cout],
            false,
            Arc::new(move |fc| {
                let (rc, _) = f1(fc)?;
                let mut t = vec![0i16; kk * cin_g * cout];
                for o in 0..cout {
                    for ci in 0..cin_g {
                        for tap in 0..kk {
                            t[(tap * cin_g + ci) * cout + o] = rc.codes[o * taps + ci * kk + tap];
                        }
                    }
                }
                Ok(rc.tensor(vec![kk * cin_g, cout], t))
            }),
        )?;
        let flat = b.reshape_fixed(cols, &[po as u32, (kk * cin_g) as u32]);
        b.matmul(flat, wt, DType::I64)
    };
    let (kx, ky) = (xc.key.clone(), want.key.clone());
    let f2 = fold.clone();
    let (m, s) = decl_ms(
        b,
        cx,
        lb,
        &id,
        cout,
        Arc::new(move |fc| {
            let (rc, _) = f2(fc)?;
            let (sx, sy) = (fc.scale(&kx)?, fc.scale_vec(&ky, cout)?);
            Ok(rc.scales.iter().zip(&sy).map(|(sw, sy)| sw * sx / sy).collect())
        }),
    )?;
    let ky = want.key.clone();
    let f3 = fold.clone();
    let z = decl(
        b,
        cx,
        lb,
        &format!("{id}.z"),
        DType::I64,
        &[cout],
        false,
        Arc::new(move |fc| {
            let (_, fb) = f3(fc)?;
            let sy = fc.scale_vec(&ky, cout)?;
            Ok(IntTensor::i64(vec![cout], fb.iter().zip(&sy).map(|(v, s)| (*v / s).round() as i64).collect()))
        }),
    )?;
    let (lo, hi) = super::code_bounds(want.dt);
    let r = narrow_into(b, acc, m, s, Some(z), if relu { 0 } else { lo }, hi, want.dt);
    if want.dt == DType::I16 {
        b.commit(r);
    }
    let v = rows_val(r, want.dt, want.key.clone(), cout, &id);
    note_resid(cx, lb, &v);
    Ok(v)
}

/// Max pooling: the windows gathered from the rows and a padding row of the code floor, `ReduceMax` over the taps.
fn lower_maxpool(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, x: &Val, n: &PNode, k: usize, stride: usize, pad: usize) -> Result<Val> {
    let xc = super::bidir::codes_rows(b, cx, lb, x)?;
    let kk = k * k;
    let c = xc.len;
    let floor_row = b.pb.konst(DType::I16, &[1, c as u32], &vec![-32767i128; c]);
    let padded = b.concat(&[xc.r, floor_row], 0);
    let win = Win::square(k, stride, pad, 1);
    let ix = window_index(n.inp, n.out, &win);
    let (po, rows_in) = (n.out.rows(), n.inp.rows());
    let iname = win.table_name(n.inp, n.out);
    let idx = decl(b, cx, lb, &iname, DType::Idx, &[po, kk], false, Arc::new(move |_| Ok(IntTensor::idx(vec![po, kk], ix.clone()))))?;
    let idx = b.clamp(idx, 0, rows_in as i64, DType::Idx);
    let cols = b.gather(padded, idx, 0, 0); // [P_out, k·k, C]
    let m = b.reduce_max(cols, 1); // [P_out, 1, C]
    let r = b.reshape_fixed(m, &[po as u32, c as u32]);
    b.commit(r);
    // The maximum of codes is a code at the same scale.
    Ok(rows_val(r, DType::I16, xc.key.clone(), c, &n.id))
}

/// ReLU6's output unit: its range is exactly `[0, 6]`, so the `i16` codes are `6/32767` apiece and the narrowing's own clamp at
/// 0 and at 32767 IS `min(max(x, 0), 6)` — no node of its own, and no new primitive.
fn relu6_key() -> ScaleKey {
    ScaleKey { base: Base::Fixed(6.0 / 32767.0), factor: 1.0 }
}

/// An activation a narrowing's clamp performs (ReLU: at 0; ReLU6: at 0 and 6, in its fixed unit) — every other is a table.
fn is_clamp_act(a: Act) -> bool {
    matches!(a, Act::Relu | Act::Relu6)
}

/// The activation `a` over rows (`ReLU` is exact on codes: a clamp; `ReLU6` is a requantisation into its fixed unit; every other
/// is a table).
pub(super) fn lower_act(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, x: &Val, a: Act, site: &str) -> Result<Val> {
    let xc = super::bidir::codes_rows(b, cx, lb, x)?;
    match a {
        Act::Identity => Ok(xc),
        Act::Relu => {
            let r = b.clamp(xc.r, 0, 32767, DType::I16);
            b.commit(r);
            Ok(rows_val(r, DType::I16, xc.key.clone(), xc.len, site))
        }
        Act::Relu6 => {
            let (kx, ky) = (xc.key.clone(), relu6_key());
            let (m, s) = decl_ms(b, cx, lb, site, 1, Arc::new(move |fc| Ok(vec![fc.scale(&kx)? / fc.scale(&ky)?])))?;
            let r = narrow_into(b, xc.r, m, s, None, 0, 32767, DType::I16);
            b.commit(r);
            Ok(rows_val(r, DType::I16, relu6_key(), xc.len, site))
        }
        other => lower_table_named(b, cx, lb, &xc, TableFn::Act(other), site),
    }
}

/// A channel LayerNorm over rows: the input's codes (or `i32` rows), the unit row and one per-channel narrowing with the gain.
fn lower_channel_norm(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, x: &Val, n: &PNode, eps: f64) -> Result<Val> {
    let xc = super::bidir::codes_rows(b, cx, lb, x)?;
    super::bidir::norm_rows_kind(b, cx, lb, &xc, NormKind::Layer, eps, &n.id, true, &codes_want(&n.id))
}

/// One node of a branch, from `x`; `last_want` is where the branch's final value should land when it is a convolution's.
fn lower_nodes(b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, nodes: &[PNode], mut x: Val, last_want: Option<&Want>) -> Result<Val> {
    for (i, n) in nodes.iter().enumerate() {
        let last = i + 1 == nodes.len();
        x = match &n.op {
            PKind::Conv(c) => {
                let clamp = c.act.is_some_and(is_clamp_act);
                let relu6 = c.act == Some(Act::Relu6);
                let table_act = c.act.filter(|a| !is_clamp_act(*a) && *a != Act::Identity);
                match (last, last_want) {
                    // The branch's last convolution lands on the unit's sum scale (ReLU6's fixed unit cannot).
                    (true, Some(w)) if table_act.is_none() && !relu6 => lower_conv(b, cx, lb, &x, n, c, w, clamp)?,
                    _ => {
                        let want = if relu6 { Want { dt: DType::I16, key: relu6_key() } } else { codes_want(&n.id) };
                        let v = lower_conv(b, cx, lb, &x, n, c, &want, clamp)?;
                        match table_act {
                            Some(a) => lower_act(b, cx, lb, &v, a, &format!("{}.act", n.id))?,
                            None => v,
                        }
                    }
                }
            }
            PKind::ChannelNorm { eps, .. } => lower_channel_norm(b, cx, lb, &x, n, *eps)?,
            PKind::MaxPool { k, stride, pad } => lower_maxpool(b, cx, lb, &x, n, *k, *stride, *pad)?,
            PKind::Act(a) => lower_act(b, cx, lb, &x, *a, &n.id)?,
            PKind::Residual { main, shortcut, act } => {
                // Both branches into i32 at the unit's own wide scale, added exactly, activated and narrowed to codes.
                let sum_key = ScaleKey::site(vec![format!("{}.sum", n.id)], true);
                let sum_want = Want { dt: DType::I32, key: sum_key.clone() };
                let xin = super::bidir::codes_rows(b, cx, lb, &x)?;
                let to_sum = |b: &mut BlockBuilder<'_>, cx: &mut Cx<'_>, lb: &mut Lb, v: Val| -> Result<Val> { coerce(b, cx, lb, &v, DType::I32, &sum_key) };
                let m = lower_nodes(b, cx, lb, main, xin.clone(), Some(&sum_want))?;
                let m = if m.dt == DType::I32 && m.key.same(&sum_key) { m } else { to_sum(b, cx, lb, m)? };
                let s = if shortcut.is_empty() { to_sum(b, cx, lb, xin)? } else {
                    let s = lower_nodes(b, cx, lb, shortcut, x.clone(), Some(&sum_want))?;
                    if s.dt == DType::I32 && s.key.same(&sum_key) { s } else { to_sum(b, cx, lb, s)? }
                };
                let t = b.add(m.r, s.r, DType::I64);
                let t = b.clamp(t, i32::MIN as i64, i32::MAX as i64, DType::I32);
                let sum = rows_val(t, DType::I32, sum_key.clone(), m.len, &format!("{}.sum", n.id));
                let relu = act.is_some_and(is_clamp_act);
                let table_act = act.filter(|a| !is_clamp_act(*a) && *a != Act::Identity);
                let out_key = if *act == Some(Act::Relu6) { relu6_key() } else { site_key(&n.id) };
                let (kk, ko) = (sum_key.clone(), out_key.clone());
                let (mm, ss) = decl_ms(b, cx, lb, &format!("{}.out", n.id), 1, Arc::new(move |fc| Ok(vec![fc.scale(&kk)? / fc.scale(&ko)?])))?;
                let (lo, hi) = super::code_bounds(DType::I16);
                let r = narrow_into(b, sum.r, mm, ss, None, if relu { 0 } else { lo }, hi, DType::I16);
                b.commit(r);
                let v = rows_val(r, DType::I16, out_key, sum.len, &n.id);
                match table_act {
                    Some(a) => lower_act(b, cx, lb, &v, a, &format!("{}.act", n.id))?,
                    None => v,
                }
            }
        };
    }
    Ok(x)
}

/// The carry's element count: the largest activation any block boundary carries (a group's output / the next one's input,
/// and the last feature map the `post` block reads).
fn carry_elems(p: &Plan) -> usize {
    let mut e = p.last.rows() * p.last.c;
    for g in p.groups.iter().skip(1) {
        let geo = p.nodes[g.start].inp;
        e = e.max(geo.rows() * geo.c);
    }
    e
}

/// The activation a block reads from the carry: its first `rows·c` elements as `[rows, c]` rows at the residual scale. `site`
/// names the value (and its calibration statistics): it must be UNIQUE per block, because a requantisation's params are named
/// after it and a param of one name, dtype and shape is shared by every block that declares it (`decl`).
fn unpack(b: &mut BlockBuilder<'_>, g: Geo, e: usize, site: &str) -> Val {
    let n = g.rows() * g.c;
    let flat = if n == e { tir::Ref::CarryIn(0) } else { b.slice(tir::Ref::CarryIn(0), 0, 0, n as u32) };
    let rows = b.reshape_fixed(flat, &[g.rows() as u32, g.c as u32]);
    rows_val(rows, DType::I32, ScaleKey::resid(), g.c, site)
}

/// The carry a block writes: the activation flattened and padded with zeros to the carry's `e` elements.
fn pack(b: &mut BlockBuilder<'_>, x: tir::Ref, n: usize, e: usize) -> tir::Ref {
    let flat = b.reshape_fixed(x, &[n as u32]);
    if n == e {
        return flat;
    }
    let zero = b.c(DType::I32, 0);
    let pad = b.broadcast(zero, &[Dim::Fixed((e - n) as u32)]);
    b.concat(&[flat, pad], 0)
}

/// One block of the network: `pre` (the input's normalisation and the first group), a layer block (a group) or `post` (the
/// output).
fn cnn_block(pb: &mut ProgramBuilder, cx: &mut Cx<'_>, hbk: usize, spec: &CnnSpec, p: &Plan) -> Result<(u8, Option<u16>)> {
    let hl = cx.hl;
    let role = hl.blocks[hbk].role;
    let group = match role {
        BlockRole::Pre => 0,
        BlockRole::Layer => 1 + hl.schedule.iter().position(|k| *k as usize == hbk).unwrap_or(0),
        BlockRole::Post => usize::MAX,
    };
    let mut lb = super::vision::new_lb(hl, hbk);
    let in_geo = |g: usize| if g == 0 { Geo { h: spec.h as usize, w: spec.w as usize, c: 3 } } else { p.nodes[p.groups[g].start].inp };
    let carry_geo = match role {
        BlockRole::Pre => None,
        BlockRole::Layer => Some(in_geo(group)),
        BlockRole::Post => Some(p.last),
    };
    let e = carry_elems(p);
    let carry_sig = carry_geo.map(|_| vec![TensorType::fixed(DType::I32, &[e as u32])]).unwrap_or_default();
    let tb = pb.blocks.len() as u8;
    let mut b = pb.block(&hl.blocks[hbk].name, carry_sig);
    let resid = ScaleKey::resid();
    match role {
        BlockRole::Post => {
            let g = p.last;
            let x = unpack(&mut b, g, e, "carry0");
            let ok = out_key();
            let out = match &spec.out {
                CnnOut::Map => coerce(&mut b, cx, &mut lb, &Val { site: "out".into(), ..x }, DType::I32, &ok)?,
                CnnOut::GlobalAvgNorm { eps, .. } => {
                    // The mean over positions at the residual unit (the sum is exact, one rounding), then the pooled row's
                    // LayerNorm into the output's unit.
                    let sum = b.reduce_sum(x.r, 0, DType::I64);
                    let n = b.c(DType::I64, g.rows() as i128);
                    let mean = b.div(sum, n, Rounding::HalfAwayFromZero, DType::I64);
                    let mean = b.clamp(mean, i32::MIN as i64, i32::MAX as i64, DType::I32);
                    let mean = rows_val(mean, DType::I32, x.key.clone(), g.c, "pool");
                    let v = super::bidir::norm_rows_kind(&mut b, cx, &mut lb, &mean, NormKind::Layer, *eps, "pnorm", true, &Want { dt: DType::I32, key: ok.clone() })?;
                    Val { site: "out".into(), ..v }
                }
                CnnOut::GlobalAvg => {
                    // The mean over positions, at the output's unit: the rows narrowed to it, summed exactly and divided.
                    let rows = coerce(&mut b, cx, &mut lb, &Val { site: "out".into(), ..x }, DType::I32, &ok)?;
                    let sum = b.reduce_sum(rows.r, 0, DType::I64);
                    let n = b.c(DType::I64, g.rows() as i128);
                    let mean = b.div(sum, n, Rounding::HalfAwayFromZero, DType::I64);
                    let mean = b.clamp(mean, i32::MIN as i64, i32::MAX as i64, DType::I32);
                    rows_val(mean, DType::I32, ok.clone(), g.c, "out")
                }
            };
            let out = ensure_node(&mut b, &out);
            let tir::Ref::Node(oi) = out.r else { unreachable!("ensure_node") };
            b.commit(out.r);
            note_site(cx, tb, &out);
            cx.logits_key = Some(out.key.clone());
            Ok((b.finish(&[]), Some(oi)))
        }
        _ => {
            let range = p.groups[group].clone();
            let mut x = match role {
                BlockRole::Pre => {
                    let (h, w) = (spec.h as usize, spec.w as usize);
                    let img = decl(&mut b, cx, &lb, IMAGE_PARAM, DType::I16, &[h, w, 3], false, input_fill(DType::I16, vec![h, w, 3]))?;
                    let rows = b.reshape_fixed(img, &[(h * w) as u32, 3]);
                    // (px/255 − mean)/std per channel, to codes at the input's calibrated scale.
                    let (mean, std) = (spec.mean, spec.std);
                    let key = site_key("in");
                    let k1 = key.clone();
                    let (m, s) = decl_ms(&mut b, cx, &lb, "in", 3, Arc::new(move |fc| {
                        let sy = fc.scale(&k1)?;
                        Ok((0..3).map(|c| 1.0 / (255.0 * std[c]) / sy).collect())
                    }))?;
                    let k2 = key.clone();
                    let z = decl(&mut b, cx, &lb, "in.z", DType::I64, &[3], false, Arc::new(move |fc| {
                        let sy = fc.scale(&k2)?;
                        Ok(IntTensor::i64(vec![3], (0..3).map(|c| (-mean[c] / std[c] / sy).round() as i64).collect()))
                    }))?;
                    let (lo, hi) = super::code_bounds(DType::I16);
                    let q = narrow_into(&mut b, rows, m, s, Some(z), lo, hi, DType::I16);
                    b.commit(q);
                    rows_val(q, DType::I16, key, 3, "in")
                }
                _ => unpack(&mut b, carry_geo.expect("a layer block has a carry"), e, &format!("carry{group}")),
            };
            x = lower_nodes(&mut b, cx, &mut lb, &p.nodes[range], x, None)?;
            // The activation crosses the block boundary as `i32` at the residual scale.
            let out = coerce(&mut b, cx, &mut lb, &Val { site: format!("out{group}"), ..x }, DType::I32, &resid)?;
            let out = ensure_node(&mut b, &out);
            note_resid(cx, &lb, &out);
            note_site(cx, tb, &out);
            let n = b.shape(out.r).iter().map(|d| match d { Dim::Fixed(n) => *n as usize, Dim::H => 0 }).product::<usize>();
            let carried = pack(&mut b, out.r, n, e);
            Ok((b.finish(&[carried]), None))
        }
    }
}

/// Lower a network: `pre` (normalisation and the first group), a block per further group, `post` (the output). The program's
/// output is the last feature map as rows (`[P, C]`) or its mean (`[1, C]`) in the class's fixed point (`2^-q`).
pub fn lower_cnn(hl: &HlProgram, spec: &CnnSpec) -> Result<Lowered> {
    lower_cnn_opts(hl, spec, &LowerOpts::default())
}

/// [`lower_cnn`] with the registrant's options: [`LowerOpts::conv_weight_bits`] (8, the default, or 16) is the width of the
/// convolutions' weight codes.
pub fn lower_cnn_opts(hl: &HlProgram, spec: &CnnSpec, opts: &LowerOpts) -> Result<Lowered> {
    if !matches!(opts.conv_weight_bits, 8 | 16) {
        return Err(LowerError::not_lowerable(format!(
            "conv_weight_bits = {}: a convolution's weight codes are 8 bits (i8 per output row, the default) or 16 (i16 per output row)",
            opts.conv_weight_bits
        )));
    }
    let p = plan(spec)?;
    let hb = tir::program::HISTORY_BOUND_V1_SMALL;
    let mut pb = ProgramBuilder::new(1, hb);
    let mut cx = Cx {
        hl,
        table_shift: 0,
        fills: Vec::new(),
        row_params: BTreeMap::new(),
        resid_sites: BTreeMap::new(),
        tstate: BTreeMap::new(),
        history_bound: hb,
        max_window: hb,
        logits_key: None,
        site_nodes: BTreeMap::new(),
        shared: Default::default(),
        tables: Default::default(),
        image_rows: None,
        image_cursor: None,
        image_cursor_layer: None,
        split_max_readers: 0,
        quant: BTreeMap::new(),
        carry_keys: BTreeMap::new(),
        table_chunk: 1 << 24,
        conv_weight_bits: opts.conv_weight_bits,
        out_major_rows: false,
    };
    let mut block_map = vec![u8::MAX; hl.blocks.len()];
    let mut order: Vec<usize> = vec![hl.pre];
    order.extend(hl.schedule.iter().map(|k| *k as usize));
    order.push(hl.post);
    let mut out_node = None;
    for &hbk in &order {
        let (tb, o) = cnn_block(&mut pb, &mut cx, hbk, spec, &p)?;
        block_map[hbk] = tb;
        if o.is_some() {
            out_node = o;
        }
    }
    let out_node = out_node.ok_or_else(|| LowerError::eval("internal: no output node"))?;
    let layers: Vec<u8> = hl.schedule.iter().map(|k| block_map[*k as usize]).collect();
    let program = pb.finish(block_map[hl.pre], layers, block_map[hl.post], out_node);
    tir::validate::validate(&program).map_err(|e| LowerError::not_lowerable(format!("{}: the network's program is not in normal form: {e}", spec.architecture)))?;
    let resid_sites = cx.resid_sites.into_iter().map(|((k, _), f)| (k, f)).collect();
    let logits_key = cx.logits_key.ok_or_else(|| LowerError::eval("internal: no output scale"))?;
    Ok(Lowered { program, fills: cx.fills, row_params: cx.row_params, resid_sites, logits_key, block_map, site_nodes: cx.site_nodes, budget_fallbacks: vec![] })
}

// ───────────────────────────── from a spec value ─────────────────────────────

impl CnnSpec {
    /// A spec an adapter built is untrusted: every size is bounded and the plan must hold together BEFORE anything is sized on
    /// it. A refusal names the problem.
    pub fn validate(&self) -> Result<()> {
        let p = plan(self)?;
        let mut total_w = 0usize;
        convs(&p.nodes, &mut |_, c| total_w = total_w.saturating_add(c.cout.saturating_mul(c.cin / c.groups).saturating_mul(c.taps())));
        if total_w > 1 << 36 {
            return Err(LowerError::bad(format!("{}: {total_w} convolution weights", self.architecture)));
        }
        Ok(())
    }
}

/// An adapter's spec, with the class's input size and the processor's normalisation applied (they are not in `config.json`),
/// validated.
pub fn cnn_spec_from_value(mut v: Value, size: Option<(u32, u32)>, mean_std: Option<([f64; 3], [f64; 3])>) -> Result<CnnSpec> {
    let o = v.as_object_mut().ok_or_else(|| LowerError::bad(format!("an invalid {CNN_SPEC_SCHEMA_V1}: not an object")))?;
    if let Some((h, w)) = size {
        o.insert("h".into(), Value::from(h));
        o.insert("w".into(), Value::from(w));
    }
    if let Some((m, s)) = mean_std {
        o.insert("mean".into(), serde_json::to_value(m).unwrap_or(Value::Null));
        o.insert("std".into(), serde_json::to_value(s).unwrap_or(Value::Null));
    }
    if o.get("h").is_none_or(Value::is_null) || o.get("w").is_none_or(Value::is_null) {
        return Err(LowerError::bad("a convolutional network needs the class's declared input size (its config has none)"));
    }
    let s: CnnSpec = serde_json::from_value(v).map_err(|e| LowerError::bad(format!("an invalid {CNN_SPEC_SCHEMA_V1}: {e}")))?;
    s.validate()?;
    Ok(s)
}

/// Parse a network's `config.json` through the adapter (kind `cnn`) that claims its architecture. `size` is the class's declared
/// input size (a config has none) and `mean_std` the processor's normalisation (`preprocessor_config.json`; the adapter's
/// default when `None`).
pub fn parse_cnn(config: &str, size: Option<(u32, u32)>, mean_std: Option<([f64; 3], [f64; 3])>) -> Result<CnnSpec> {
    let root: Value = serde_json::from_str(config).map_err(|e| LowerError::bad(format!("config.json: {e}")))?;
    let arch = root["architectures"][0].as_str().unwrap_or("");
    let model_type = root.get("model_type").and_then(Value::as_str);
    match crate::adapter::builtin::find_cnn_for(arch, model_type) {
        Some(a) => {
            let built = crate::adapter::eval::build_cnn_spec(a, &root)?;
            cnn_spec_from_value(built.spec, size, mean_std)
        }
        None => Err(LowerError::not_lowerable(format!("`{arch}` is not a convolutional network any adapter of kind `cnn` claims"))),
    }
}
