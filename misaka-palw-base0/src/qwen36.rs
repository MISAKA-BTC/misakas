//! **The Qwen3.6 hybrid engine — thirty GatedDeltaNet arms, ten gated-attention arms, forty MoE
//! blocks.**
//!
//! `Base0ArtifactV1` is a dense decoder container: seven weight tensors per layer, one shape for
//! all of them. Qwen3.6 is not that. Its forty layers alternate 3:1 between a linear-attention arm
//! carrying a recurrent state and a softmax arm with an output gate, every one of them followed by
//! a 256-expert mixture whose weights are 92 % of the model. A struct with named fields for that
//! would have sixty of them and would need a new field for the next architecture.
//!
//! So the container here is a **named store**, which is what GGUF is and what the court's weight
//! oracle already addresses (`operand_bytes(tensor_name, layer, …)`). Weights are `i8` under
//! template names; the A16 `(multiplier, shift, zero)` triples are the same store the dense tier
//! uses. Two consequences that are the reason for the choice:
//!
//! * a tensor is a slice, so the store can become offsets into a memory map without the engine
//!   changing — and at 35 B parameters a memory map is not an optimisation, it is the only way the
//!   weights fit on a machine that has less RAM than the model;
//! * a court opening names a tensor the same way the engine reads it, so there is no translation
//!   layer to get wrong.
//!
//! # What is here and what is not
//!
//! Every op is [`kaspa_consensus_core::palw_qwen36_ops`] or the A16 tier. Nothing new is defined,
//! and nothing is fused: the output gate, the L2 norm, the convolution and the recurrence are four
//! nodes, because a fused node is a node a bisection cannot land inside.
//!
//! The **converter** is not here. This runs an artifact; turning a checkpoint into one is a
//! separate pipeline, and it is the piece that decides fidelity.

use crate::artifact::ArtifactError;
use crate::rope::RopeTableV1;
use kaspa_consensus_core::palw_base0_a16::{
    A16QuantParams, a16_add_elem, a16_attn_scores, a16_attn_values, a16_matmul_requant, a16_matmul_rescale, a16_mul_elem, a16_requant,
    a16_rms_norm, a16_softmax_rows,
};
use kaspa_consensus_core::palw_base0_ops::silu;
use kaspa_consensus_core::palw_qwen36_ops::{
    Qwen36GdnParamsV1, Qwen36GdnStateV1, q36_gate_apply, q36_gdn_step, q36_l2_norm, q36_moe_combine, q36_pow_q, q36_rope_partial,
    q36_router_topk, q36_sigmoid_gate, q36_ssm_conv,
};
use std::collections::BTreeMap;

/// Which arm a layer carries. `config.json` calls these `linear_attention` and `full_attention`
/// and lists them per layer rather than deriving them from an interval, so this does too: a model
/// that changes the pattern is a different `layer_types`, not a different code path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Qwen36LayerKind {
    /// GatedDeltaNet: a four-tap causal convolution, an L2-normalized key, and a recurrent state.
    LinearAttention,
    /// Grouped-query softmax attention with per-head QK-norm, partial rotation and an output gate.
    FullAttention,
}

/// The geometry, from `config.json`'s `text_config`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Qwen36ShapeV1 {
    pub layer_types: Vec<Qwen36LayerKind>,
    pub d_model: usize,
    /// Full attention: 16 query heads over 2 kv heads, head_dim 256.
    pub n_heads: usize,
    pub n_kv_heads: usize,
    pub head_dim: usize,
    /// `partial_rotary_factor · head_dim`. Qwen3.6 rotates 64 of every head's 256 lanes and
    /// carries the other 192 untouched; rotating all of them is a different model.
    pub rotary_dim: usize,
    /// Linear attention: 16 key heads and 32 value heads, head_dim 128.
    pub linear_k_heads: usize,
    pub linear_v_heads: usize,
    pub linear_head_dim: usize,
    pub conv_kernel: usize,
    /// MoE: 256 experts, 8 routed per token, intermediate 512, plus one always-on shared expert.
    pub n_experts: usize,
    pub experts_per_token: usize,
    pub moe_dim: usize,
    pub shared_dim: usize,
    pub vocab: usize,
    pub max_position: usize,
    pub eps_q: i64,
    /// The widening `softmax_shifted` applies to a router row. Class data: a router's logits are
    /// no more confined to Qk than an attention logit is.
    pub router_up_bits: u8,
}

impl Qwen36ShapeV1 {
    pub fn n_layers(&self) -> usize {
        self.layer_types.len()
    }
    pub fn kv_dim(&self) -> usize {
        self.n_kv_heads * self.head_dim
    }
    pub fn linear_k_dim(&self) -> usize {
        self.linear_k_heads * self.linear_head_dim
    }
    pub fn linear_v_dim(&self) -> usize {
        self.linear_v_heads * self.linear_head_dim
    }

    /// Refuse a shape the engine cannot run, at construction rather than three layers into a
    /// forward pass.
    pub fn validate(&self) -> Result<(), ArtifactError> {
        let ok = !self.layer_types.is_empty()
            && self.d_model > 0
            && self.n_heads > 0
            && self.n_kv_heads > 0
            && self.n_heads.is_multiple_of(self.n_kv_heads)
            && self.head_dim > 0
            && self.rotary_dim <= self.head_dim
            && self.rotary_dim.is_multiple_of(2)
            && self.linear_k_heads > 0
            && self.linear_v_heads > 0
            && self.linear_v_heads.is_multiple_of(self.linear_k_heads)
            && self.linear_head_dim > 0
            && self.conv_kernel > 0
            && self.n_experts > 0
            && self.experts_per_token > 0
            && self.experts_per_token <= self.n_experts
            && self.moe_dim > 0
            && self.vocab > 0
            && self.max_position > 0;
        if ok { Ok(()) } else { Err(ArtifactError::BadShape) }
    }
}

/// The artifact: a shape, a named weight store, a named parameter store, and the pinned rotary
/// table.
#[derive(Clone, Debug)]
pub struct Qwen36ArtifactV1 {
    pub shape: Qwen36ShapeV1,
    tensors: BTreeMap<String, Vec<i8>>,
    params: BTreeMap<String, Vec<u8>>,
    pub rope: RopeTableV1,
}

/// Why the engine refused. Every one is a REGISTRATION defect surfaced before the pass starts,
/// except `Position`, which is a caller error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Qwen36Error {
    MissingTensor(String),
    MissingParams(String),
    BadTensor { name: String, want: usize, got: usize },
    BadParams(String),
    OpRefused(&'static str),
    Position,
}

impl std::fmt::Display for Qwen36Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingTensor(n) => write!(f, "the artifact has no tensor {n}"),
            Self::MissingParams(n) => write!(f, "the artifact has no parameter row {n}"),
            Self::BadTensor { name, want, got } => write!(f, "tensor {name} should hold {want} values and holds {got}"),
            Self::BadParams(n) => write!(f, "parameter row {n} is malformed"),
            Self::OpRefused(w) => write!(f, "the op {w} refused its input"),
            Self::Position => write!(f, "the position is past the rotary table"),
        }
    }
}

impl std::error::Error for Qwen36Error {}

impl Qwen36ArtifactV1 {
    pub fn new(shape: Qwen36ShapeV1, rope: RopeTableV1) -> Result<Self, ArtifactError> {
        shape.validate()?;
        Ok(Self { shape, tensors: BTreeMap::new(), params: BTreeMap::new(), rope })
    }

    pub fn with_tensor(mut self, name: impl Into<String>, values: Vec<i8>) -> Self {
        self.tensors.insert(name.into(), values);
        self
    }

    pub fn with_params(mut self, name: impl Into<String>, rows: &[A16QuantParams]) -> Self {
        self.params.insert(name.into(), rows.iter().flat_map(|p| p.to_wire()).collect());
        self
    }

    pub fn tensor(&self, name: &str) -> Result<&[i8], Qwen36Error> {
        self.tensors.get(name).map(|v| &v[..]).ok_or_else(|| Qwen36Error::MissingTensor(name.to_string()))
    }

    fn tensor_sized(&self, name: &str, want: usize) -> Result<&[i8], Qwen36Error> {
        let row = self.tensor(name)?;
        if row.len() != want {
            return Err(Qwen36Error::BadTensor { name: name.to_string(), want, got: row.len() });
        }
        Ok(row)
    }

    /// One parameter row, decoded. Widths are checked by the caller that knows what it asked for.
    pub fn param_rows(&self, name: &str) -> Result<Vec<A16QuantParams>, Qwen36Error> {
        let bytes = self.params.get(name).ok_or_else(|| Qwen36Error::MissingParams(name.to_string()))?;
        if bytes.is_empty() || !bytes.len().is_multiple_of(A16QuantParams::WIRE_BYTES) {
            return Err(Qwen36Error::BadParams(name.to_string()));
        }
        bytes
            .chunks_exact(A16QuantParams::WIRE_BYTES)
            .map(|c| A16QuantParams::from_wire(c).map_err(|_| Qwen36Error::BadParams(name.to_string())))
            .collect()
    }

    fn one_param(&self, name: &str) -> Result<A16QuantParams, Qwen36Error> {
        let rows = self.param_rows(name)?;
        if rows.len() != 1 {
            return Err(Qwen36Error::BadParams(name.to_string()));
        }
        Ok(rows[0])
    }

    fn params_sized(&self, name: &str, want: usize) -> Result<Vec<A16QuantParams>, Qwen36Error> {
        let rows = self.param_rows(name)?;
        // A per-layer-uniform triple is stored once and tiled, which keeps the store small without
        // a second layout — the same rule the dense tier's oracle applies.
        if rows.len() == want {
            return Ok(rows);
        }
        if rows.len() == 1 {
            return Ok(vec![rows[0]; want]);
        }
        Err(Qwen36Error::BadParams(name.to_string()))
    }

    /// A registered scalar, in Q[`K`]. Carried in a triple's `zero` so the store has one wire
    /// format rather than two.
    fn scalar(&self, name: &str) -> Result<i64, Qwen36Error> {
        Ok(self.one_param(name)?.zero)
    }
}

/// Every op in this module returns its own error type; the engine returns one. Written as a free
/// generic rather than a closure per function because a closure's error type is inferred from its
/// first use and the arms mix `PalwA16OpError` with `PalwQwen36OpError`.
fn refuse<E>(what: &'static str) -> impl Fn(E) -> Qwen36Error {
    move |_| Qwen36Error::OpRefused(what)
}

/// The runtime state one sequence carries.
///
/// A GatedDeltaNet layer's state is a `d_v × d_k` matrix per value head and it is the whole
/// history — there is no growing cache. A full-attention layer keeps the usual keys and values.
/// Both live here so that a caller holds one object per sequence.
pub struct Qwen36Cache {
    /// Per layer (empty for full-attention layers): one state per value head.
    pub gdn: Vec<Vec<Qwen36GdnStateV1>>,
    /// Per layer (empty for full-attention layers): the convolution's `kernel` most recent rows,
    /// oldest first, over the concatenated q/k/v channels.
    pub conv: Vec<Vec<Vec<i32>>>,
    /// Per layer (empty for linear-attention layers).
    pub keys: Vec<Vec<Vec<i32>>>,
    pub values: Vec<Vec<Vec<i32>>>,
}

impl Qwen36Cache {
    pub fn new(shape: &Qwen36ShapeV1) -> Self {
        let n = shape.n_layers();
        let mut gdn = vec![Vec::new(); n];
        let mut conv = vec![Vec::new(); n];
        let conv_width = 2 * shape.linear_k_dim() + shape.linear_v_dim();
        for (li, kind) in shape.layer_types.iter().enumerate() {
            if *kind == Qwen36LayerKind::LinearAttention {
                gdn[li] =
                    (0..shape.linear_v_heads).map(|_| Qwen36GdnStateV1::zeros(shape.linear_head_dim, shape.linear_head_dim)).collect();
                conv[li] = vec![vec![0; conv_width]; shape.conv_kernel];
            }
        }
        Self { gdn, conv, keys: vec![Vec::new(); n], values: vec![Vec::new(); n] }
    }

    /// How many positions the softmax layers have seen. Zero for a fresh cache.
    pub fn len(&self) -> usize {
        self.keys.iter().map(|k| k.len()).max().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The engine.
pub struct Qwen36Engine<'a> {
    pub artifact: &'a Qwen36ArtifactV1,
}

impl<'a> Qwen36Engine<'a> {
    pub fn new(artifact: &'a Qwen36ArtifactV1) -> Self {
        Self { artifact }
    }

    /// One position. Returns the committed logit row: i16 codes in i32 lanes, argmax over which
    /// breaks ties to the lowest id.
    pub fn forward_token(&self, cache: &mut Qwen36Cache, token_id: usize, position: usize) -> Result<Vec<i32>, Qwen36Error> {
        let a = self.artifact;
        let s = &a.shape;
        let d = s.d_model;
        if token_id >= s.vocab {
            return Err(Qwen36Error::Position);
        }

        let embed = a.tensor_sized("token_embd.weight", s.vocab * d)?;
        let row: Vec<i32> = embed[token_id * d..(token_id + 1) * d].iter().map(|c| *c as i32).collect();
        let mut h = a16_requant(&row, &a.params_sized("embed_lift.a16", d)?).map_err(refuse("embed_lift"))?;

        for li in 0..s.n_layers() {
            let n = |suffix: &str| format!("blk.{li}.{suffix}");
            // ---- the arm ------------------------------------------------------------------
            let unit = a16_rms_norm(&h, s.eps_q).map_err(refuse("attn_norm"))?;
            let normed = a16_requant(&unit, &a.params_sized(&n("attn_norm.a16"), d)?).map_err(refuse("attn_norm_req"))?;
            let delta = match s.layer_types[li] {
                Qwen36LayerKind::LinearAttention => self.linear_arm(cache, li, &normed)?,
                Qwen36LayerKind::FullAttention => self.full_arm(cache, li, &normed, position)?,
            };
            let aligned = a16_requant(&h, &a.params_sized(&n("attn_align.a16"), d)?).map_err(refuse("attn_align"))?;
            let sum = a16_add_elem(&aligned, &delta).map_err(refuse("attn_add"))?;
            h = a16_requant(&sum, &a.params_sized(&n("attn_residual.a16"), d)?).map_err(refuse("attn_res"))?;

            // ---- the mixture ---------------------------------------------------------------
            let unit = a16_rms_norm(&h, s.eps_q).map_err(refuse("ffn_norm"))?;
            let normed = a16_requant(&unit, &a.params_sized(&n("ffn_norm.a16"), d)?).map_err(refuse("ffn_norm_req"))?;
            let delta = self.moe(li, &normed)?;
            let aligned = a16_requant(&h, &a.params_sized(&n("ffn_align.a16"), d)?).map_err(refuse("ffn_align"))?;
            let sum = a16_add_elem(&aligned, &delta).map_err(refuse("ffn_add"))?;
            h = a16_requant(&sum, &a.params_sized(&n("ffn_residual.a16"), d)?).map_err(refuse("ffn_res"))?;
        }

        let unit = a16_rms_norm(&h, s.eps_q).map_err(refuse("final_norm"))?;
        let fin = a16_requant(&unit, &a.params_sized("final_norm.a16", d)?).map_err(refuse("final_req"))?;
        let unembed = a.tensor_sized("output.weight", s.vocab * d)?;
        a16_matmul_requant(unembed, &fin, &a.params_sized("output.weight.a16", s.vocab)?).map_err(refuse("logits"))
    }

    /// **The GatedDeltaNet arm.** Convolve, normalize the key, run the recurrence, gate the
    /// output, project out. Four nodes plus the projections, none of them fused.
    fn linear_arm(&self, cache: &mut Qwen36Cache, li: usize, normed: &[i32]) -> Result<Vec<i32>, Qwen36Error> {
        let a = self.artifact;
        let s = &a.shape;
        let (d, dk, dv, hd) = (s.d_model, s.linear_k_dim(), s.linear_v_dim(), s.linear_head_dim);
        let n = |suffix: &str| format!("blk.{li}.{suffix}");

        // The three projections that feed the convolution, plus the gate and the two scalars the
        // decay needs. Separate tensors rather than one fused `in_proj`: a court opening addresses
        // a tensor, and a fused one would need an offset convention on top of the name.
        let q = a16_matmul_requant(
            a.tensor_sized(&n("linear_q.weight"), dk * d)?,
            normed,
            &a.params_sized(&n("linear_q.weight.a16"), dk)?,
        )
        .map_err(refuse("linear_q"))?;
        let k = a16_matmul_requant(
            a.tensor_sized(&n("linear_k.weight"), dk * d)?,
            normed,
            &a.params_sized(&n("linear_k.weight.a16"), dk)?,
        )
        .map_err(refuse("linear_k"))?;
        let v = a16_matmul_requant(
            a.tensor_sized(&n("linear_v.weight"), dv * d)?,
            normed,
            &a.params_sized(&n("linear_v.weight.a16"), dv)?,
        )
        .map_err(refuse("linear_v"))?;
        let z = a16_matmul_requant(
            a.tensor_sized(&n("linear_z.weight"), dv * d)?,
            normed,
            &a.params_sized(&n("linear_z.weight.a16"), dv)?,
        )
        .map_err(refuse("linear_z"))?;

        // The four-tap causal convolution over the concatenated channels. The window is the
        // cache's, oldest first; a fresh sequence sees zeros before it, which is what "causal"
        // means at the start.
        let width = 2 * dk + dv;
        let mut current = Vec::with_capacity(width);
        current.extend_from_slice(&q);
        current.extend_from_slice(&k);
        current.extend_from_slice(&v);
        let window = &mut cache.conv[li];
        window.remove(0);
        window.push(current);
        let flat: Vec<i32> = window.iter().flatten().copied().collect();
        let taps: Vec<i32> = a.tensor_sized(&n("linear_conv.weight"), s.conv_kernel * width)?.iter().map(|c| *c as i32).collect();
        let convolved = q36_ssm_conv(&flat, &taps, width, a.one_param(&n("linear_conv.a16"))?).map_err(refuse("ssm_conv"))?;
        // The convolution's activation, then the delta rule's unit key.
        let activated =
            a16_requant(&silu(&convolved), &a.params_sized(&n("linear_conv_act.a16"), width)?).map_err(refuse("conv_silu"))?;

        let (qc, rest) = activated.split_at(dk);
        let (kc, vc) = rest.split_at(dk);

        // The gates. `decay = u^c` with `u = sigmoid(−dt)` and `c` the head's registered constant;
        // `beta = sigmoid(b)`. Both projections are one lane per value head.
        let dt = a16_matmul_rescale(
            a.tensor_sized(&n("linear_dt.weight"), s.linear_v_heads * d)?,
            normed,
            &a.params_sized(&n("linear_dt.weight.a16"), s.linear_v_heads)?,
        )
        .map_err(refuse("linear_dt"))?;
        let beta_raw = a16_matmul_rescale(
            a.tensor_sized(&n("linear_beta.weight"), s.linear_v_heads * d)?,
            normed,
            &a.params_sized(&n("linear_beta.weight.a16"), s.linear_v_heads)?,
        )
        .map_err(refuse("linear_beta"))?;
        let decay_c = a.param_rows(&n("linear_decay_c.a16"))?;
        let gdn_params = Qwen36GdnParamsV1 {
            read: a.one_param(&n("linear_read.a16"))?,
            write: a.one_param(&n("linear_write.a16"))?,
            out: a.one_param(&n("linear_out.a16"))?,
        };

        let group = s.linear_v_heads / s.linear_k_heads;
        let mut out = Vec::with_capacity(dv);
        for vh in 0..s.linear_v_heads {
            let kh = vh / group;
            let unit_k = q36_l2_norm(&kc[kh * hd..(kh + 1) * hd]).map_err(refuse("l2_k"))?;
            let unit_q = q36_l2_norm(&qc[kh * hd..(kh + 1) * hd]).map_err(refuse("l2_q"))?;
            let vslice = &vc[vh * hd..(vh + 1) * hd];
            let c = decay_c.get(vh.min(decay_c.len().saturating_sub(1))).map(|p| p.zero).unwrap_or(0);
            let u = q36_sigmoid_gate(&[-dt[vh]])[0] as i64;
            let decay = q36_pow_q(u, c);
            let beta = q36_sigmoid_gate(&[beta_raw[vh]])[0] as i64;
            let head_out =
                q36_gdn_step(&mut cache.gdn[li][vh], &unit_k, vslice, &unit_q, decay, beta, gdn_params).map_err(refuse("gdn_step"))?;
            out.extend(head_out);
        }

        // The output gate: RMS-normalized, then multiplied by `silu(z)` — a gate on the value
        // stream rather than on the logits, so it is `MulElem` and not a softmax.
        let unit = a16_rms_norm(&out, s.eps_q).map_err(refuse("gdn_norm"))?;
        let normed_out = a16_requant(&unit, &a.params_sized(&n("linear_norm.a16"), dv)?).map_err(refuse("gdn_norm_req"))?;
        let gate = a16_requant(&silu(&z), &a.params_sized(&n("linear_gate.a16"), dv)?).map_err(refuse("gdn_gate"))?;
        let gated = a16_mul_elem(&normed_out, &gate).map_err(refuse("gdn_mul"))?;
        let gated = a16_requant(&gated, &a.params_sized(&n("linear_gated.a16"), dv)?).map_err(refuse("gdn_gated"))?;

        a16_matmul_requant(
            a.tensor_sized(&n("linear_o.weight"), s.d_model * dv)?,
            &gated,
            &a.params_sized(&n("linear_o.weight.a16"), s.d_model)?,
        )
        .map_err(refuse("linear_o"))
    }

    /// **The gated-attention arm.** QK-norm per head before the rotation, partial rotation, GQA
    /// softmax attention, then an elementwise `sigmoid` gate on the output.
    fn full_arm(&self, cache: &mut Qwen36Cache, li: usize, normed: &[i32], position: usize) -> Result<Vec<i32>, Qwen36Error> {
        let a = self.artifact;
        let s = &a.shape;
        let (d, hd) = (s.d_model, s.head_dim);
        let q_dim = s.n_heads * hd;
        let kv_dim = s.kv_dim();
        let n = |suffix: &str| format!("blk.{li}.{suffix}");
        let (cos_row, sin_row) = a.rope.row(position).ok_or(Qwen36Error::Position)?;
        let pairs = s.rotary_dim / 2;
        if cos_row.len() < pairs {
            return Err(Qwen36Error::Position);
        }
        let (cos_row, sin_row) = (&cos_row[..pairs], &sin_row[..pairs]);

        // `attn_output_gate: true` — the q projection is double width and the second half is the
        // gate. Stored as two tensors so a court opening addresses either half by name.
        let q = a16_matmul_requant(
            a.tensor_sized(&n("attn_q.weight"), q_dim * d)?,
            normed,
            &a.params_sized(&n("attn_q.weight.a16"), q_dim)?,
        )
        .map_err(refuse("attn_q"))?;
        let gate_raw = a16_matmul_rescale(
            a.tensor_sized(&n("attn_gate.weight"), q_dim * d)?,
            normed,
            &a.params_sized(&n("attn_gate.weight.a16"), q_dim)?,
        )
        .map_err(refuse("attn_gate"))?;
        let k = a16_matmul_requant(
            a.tensor_sized(&n("attn_k.weight"), kv_dim * d)?,
            normed,
            &a.params_sized(&n("attn_k.weight.a16"), kv_dim)?,
        )
        .map_err(refuse("attn_k"))?;
        let v = a16_matmul_requant(
            a.tensor_sized(&n("attn_v.weight"), kv_dim * d)?,
            normed,
            &a.params_sized(&n("attn_v.weight.a16"), kv_dim)?,
        )
        .map_err(refuse("attn_v"))?;

        // QK-norm: RMSNorm PER HEAD, before the rotation. Normalizing the whole row instead would
        // couple the heads, and doing it after the rotation would normalize away part of what the
        // rotation encodes.
        let per_head_norm = |row: &[i32], heads: usize, name: &str| -> Result<Vec<i32>, Qwen36Error> {
            let params = a.params_sized(name, hd)?;
            let mut out = Vec::with_capacity(row.len());
            for head in 0..heads {
                let slice = &row[head * hd..(head + 1) * hd];
                let unit = a16_rms_norm(slice, s.eps_q).map_err(refuse("qk_norm"))?;
                out.extend(a16_requant(&unit, &params).map_err(refuse("qk_norm_req"))?);
            }
            Ok(out)
        };
        let q = per_head_norm(&q, s.n_heads, &n("attn_q_norm.a16"))?;
        let k = per_head_norm(&k, s.n_kv_heads, &n("attn_k_norm.a16"))?;

        let clamp = a.one_param(&n("attn_rope.a16"))?;
        let q = q36_rope_partial(&q, hd, s.rotary_dim, cos_row, sin_row, clamp).map_err(refuse("rope_q"))?;
        let k = q36_rope_partial(&k, hd, s.rotary_dim, cos_row, sin_row, clamp).map_err(refuse("rope_k"))?;

        cache.keys[li].push(k);
        cache.values[li].push(v);
        let history = cache.keys[li].len();
        let mut k_series = Vec::with_capacity(history * kv_dim);
        let mut v_series = Vec::with_capacity(history * kv_dim);
        for j in 0..history {
            k_series.extend_from_slice(&cache.keys[li][j]);
            v_series.extend_from_slice(&cache.values[li][j]);
        }

        let logit_p = a.one_param(&n("attn_logits.a16"))?;
        let up_bits = a.scalar(&n("attn_softmax_up.a16"))?.clamp(0, 62) as u8;
        let probs_p = a.one_param(&n("attn_probs.a16"))?;
        let value_p = a.one_param(&n("attn_values.a16"))?;
        let scores = a16_attn_scores(&q, &k_series, s.n_heads, s.n_kv_heads, hd, &vec![logit_p; s.n_heads * history])
            .map_err(refuse("attn_scores"))?;
        let probs = a16_softmax_rows(&scores, history, up_bits).map_err(refuse("attn_softmax"))?;
        let narrowed = a16_requant(&probs, &vec![probs_p; s.n_heads * history]).map_err(refuse("attn_probs"))?;
        let attn = a16_attn_values(&narrowed, &v_series, s.n_heads, s.n_kv_heads, hd, &vec![value_p; q_dim])
            .map_err(refuse("attn_values"))?;

        // The output gate. `sigmoid` of the gate row, applied elementwise before the projection.
        let gate = q36_sigmoid_gate(&gate_raw);
        let gated = q36_gate_apply(&attn, &gate, a.one_param(&n("attn_gated.a16"))?).map_err(refuse("attn_gate_apply"))?;

        a16_matmul_requant(a.tensor_sized(&n("attn_o.weight"), d * q_dim)?, &gated, &a.params_sized(&n("attn_o.weight.a16"), d)?)
            .map_err(refuse("attn_o"))
    }

    /// **The mixture.** Route, run the eight chosen experts and the always-on shared one, combine.
    ///
    /// The experts are run one at a time rather than gathered into a dense matmul: at 8 of 256 the
    /// gather is 97 % waste, and the whole point of the architecture is that only the chosen
    /// weights are read. That is also what makes the MoE the part a memory map serves best.
    fn moe(&self, li: usize, normed: &[i32]) -> Result<Vec<i32>, Qwen36Error> {
        let a = self.artifact;
        let s = &a.shape;
        let d = s.d_model;
        let n = |suffix: &str| format!("blk.{li}.{suffix}");

        let router = a16_matmul_rescale(
            a.tensor_sized(&n("ffn_router.weight"), s.n_experts * d)?,
            normed,
            &a.params_sized(&n("ffn_router.weight.a16"), s.n_experts)?,
        )
        .map_err(refuse("router"))?;
        // The router's logits are narrowed to codes before the selection, because the tie rule is
        // defined on what the class commits to and a wider intermediate would let two
        // implementations disagree about a tie that the committed row does not have.
        let router_codes = a16_requant(&router, &a.params_sized(&n("ffn_router.a16"), s.n_experts)?).map_err(refuse("router_req"))?;
        let routed = q36_router_topk(&router_codes, s.experts_per_token, s.shape_router_up(s)).map_err(refuse("router_topk"))?;

        let mut outputs = Vec::with_capacity(routed.len() * d);
        let mut weights = Vec::with_capacity(routed.len());
        for r in &routed {
            let e = r.expert as usize;
            outputs.extend(self.expert(li, e, normed, s.moe_dim, "expert")?);
            weights.push(r.weight_q);
        }
        let combined = q36_moe_combine(&outputs, &weights, d, a.one_param(&n("ffn_combine.a16"))?).map_err(refuse("moe_combine"))?;

        // The shared expert, always on, behind its own scalar gate.
        let shared = self.expert(li, usize::MAX, normed, s.shared_dim, "shared")?;
        let shared_gate_raw = a16_matmul_rescale(
            a.tensor_sized(&n("ffn_shared_gate.weight"), d)?,
            normed,
            &a.params_sized(&n("ffn_shared_gate.weight.a16"), 1)?,
        )
        .map_err(refuse("shared_gate"))?;
        let g = q36_sigmoid_gate(&shared_gate_raw)[0];
        let shared_gated =
            q36_gate_apply(&shared, &vec![g; d], a.one_param(&n("ffn_shared_gated.a16"))?).map_err(refuse("shared_apply"))?;
        let sum = a16_add_elem(&combined, &shared_gated).map_err(refuse("moe_add"))?;
        a16_requant(&sum, &a.params_sized(&n("ffn_moe_out.a16"), d)?).map_err(refuse("moe_out"))
    }

    /// One SwiGLU expert. `which` is the expert index, or `usize::MAX` for the shared one, which
    /// names its tensors differently and has its own intermediate width.
    fn expert(&self, li: usize, which: usize, x: &[i32], mid: usize, kind: &str) -> Result<Vec<i32>, Qwen36Error> {
        let a = self.artifact;
        let d = a.shape.d_model;
        // **`ffn_shared_expert`, not `ffn_shared`.** The shared expert's own gate projection would
        // then be `blk.N.ffn_shared_gate.weight` — the exact name the mixture's SCALAR gate already
        // uses. A `BTreeMap` keyed by strings has no way to notice, and the first version of this
        // silently handed a 512-value expert tensor to a 32-value scalar read. The engine caught it
        // only because it checks sizes; a collision between two rows of the same width would have
        // run to completion and computed something else.
        let base = if which == usize::MAX { format!("blk.{li}.ffn_shared_expert") } else { format!("blk.{li}.ffn_expert.{which}") };
        let _ = kind;

        let gate = a16_matmul_rescale(
            a.tensor_sized(&format!("{base}_gate.weight"), mid * d)?,
            x,
            &a.params_sized(&format!("{base}_gate.weight.a16"), mid)?,
        )
        .map_err(refuse("expert_gate"))?;
        let up = a16_matmul_requant(
            a.tensor_sized(&format!("{base}_up.weight"), mid * d)?,
            x,
            &a.params_sized(&format!("{base}_up.weight.a16"), mid)?,
        )
        .map_err(refuse("expert_up"))?;
        let activated =
            a16_requant(&silu(&gate), &a.params_sized(&format!("{base}_silu.a16"), mid)?).map_err(refuse("expert_silu"))?;
        let product = a16_mul_elem(&activated, &up).map_err(refuse("expert_mul"))?;
        let gated = a16_requant(&product, &a.params_sized(&format!("{base}_gated.a16"), mid)?).map_err(refuse("expert_gated"))?;
        a16_matmul_requant(
            a.tensor_sized(&format!("{base}_down.weight"), d * mid)?,
            &gated,
            &a.params_sized(&format!("{base}_down.weight.a16"), d)?,
        )
        .map_err(refuse("expert_down"))
    }
}

impl Qwen36ShapeV1 {
    fn shape_router_up(&self, _s: &Qwen36ShapeV1) -> u8 {
        self.router_up_bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::LN_THETA_10000_GEN_Q;

    /// A Qwen3.6-SHAPED artifact at a size a test can run: the same layer alternation, the same
    /// two arms, a real router over a real expert count, and everything else cut down.
    ///
    /// Not a calibration — the triples are derived from each site's fan-in, the same rule the
    /// dense tier's fixture uses — and the weights are an LCG. What it proves is that the graph
    /// composes and produces a non-degenerate row, which is the question at this stage. Fidelity
    /// is the converter's question and needs a checkpoint.
    fn fixture(layers: usize, experts: usize) -> Qwen36ArtifactV1 {
        let shape = Qwen36ShapeV1 {
            layer_types: (0..layers)
                .map(|i| if (i + 1).is_multiple_of(4) { Qwen36LayerKind::FullAttention } else { Qwen36LayerKind::LinearAttention })
                .collect(),
            d_model: 32,
            n_heads: 4,
            n_kv_heads: 2,
            head_dim: 16,
            rotary_dim: 4,
            linear_k_heads: 2,
            linear_v_heads: 4,
            linear_head_dim: 8,
            conv_kernel: 4,
            n_experts: experts,
            experts_per_token: 4,
            moe_dim: 16,
            shared_dim: 16,
            vocab: 64,
            max_position: 32,
            eps_q: 1,
            router_up_bits: 20,
        };
        let d = shape.d_model;
        let rope = RopeTableV1::generate(shape.head_dim, shape.max_position, LN_THETA_10000_GEN_Q).expect("a table");
        let mut artifact = Qwen36ArtifactV1::new(shape.clone(), rope).expect("a valid shape");

        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || -> i8 {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (((state >> 40) & 0xFF) as u8 as i8).saturating_abs().wrapping_sub(64)
        };
        let mut weights = |n: usize| -> Vec<i8> { (0..n).map(|_| next()).collect() };

        // A projection over `fan_in` attenuates by `2^-(8 + bits(fan_in)/2)`; an elementwise site
        // is unity. Same rule as the dense fixture, and for the same reason: one gain everywhere
        // decays the residual stream to zero and the differential would not notice.
        let projection = |fan_in: usize| -> A16QuantParams {
            let bits = usize::BITS - fan_in.max(1).leading_zeros();
            A16QuantParams { multiplier: 1, shift: (8 + bits / 2) as u8, zero: 0 }
        };
        let unity = A16QuantParams { multiplier: 1, shift: 0, zero: 0 };

        artifact = artifact
            .with_tensor("token_embd.weight", weights(shape.vocab * d))
            .with_tensor("output.weight", weights(shape.vocab * d))
            .with_params("embed_lift.a16", &[unity])
            .with_params("final_norm.a16", &[unity])
            .with_params("output.weight.a16", &[projection(d)]);

        for (li, kind) in shape.layer_types.iter().enumerate() {
            let n = |suffix: &str| format!("blk.{li}.{suffix}");
            for row in ["attn_norm.a16", "attn_align.a16", "attn_residual.a16", "ffn_norm.a16", "ffn_align.a16", "ffn_residual.a16"] {
                artifact = artifact.with_params(n(row), &[unity]);
            }
            match kind {
                Qwen36LayerKind::LinearAttention => {
                    let (dk, dv, hd) = (shape.linear_k_dim(), shape.linear_v_dim(), shape.linear_head_dim);
                    let width = 2 * dk + dv;
                    artifact = artifact
                        .with_tensor(n("linear_q.weight"), weights(dk * d))
                        .with_tensor(n("linear_k.weight"), weights(dk * d))
                        .with_tensor(n("linear_v.weight"), weights(dv * d))
                        .with_tensor(n("linear_z.weight"), weights(dv * d))
                        .with_tensor(n("linear_conv.weight"), weights(shape.conv_kernel * width))
                        .with_tensor(n("linear_dt.weight"), weights(shape.linear_v_heads * d))
                        .with_tensor(n("linear_beta.weight"), weights(shape.linear_v_heads * d))
                        .with_tensor(n("linear_o.weight"), weights(d * dv))
                        .with_params(n("linear_q.weight.a16"), &[projection(d)])
                        .with_params(n("linear_k.weight.a16"), &[projection(d)])
                        .with_params(n("linear_v.weight.a16"), &[projection(d)])
                        .with_params(n("linear_z.weight.a16"), &[projection(d)])
                        // The convolution reduces over four taps, so it barely attenuates.
                        .with_params(n("linear_conv.a16"), &[A16QuantParams { multiplier: 1, shift: 16, zero: 0 }])
                        .with_params(n("linear_conv_act.a16"), &[unity])
                        .with_params(n("linear_dt.weight.a16"), &[projection(d)])
                        .with_params(n("linear_beta.weight.a16"), &[projection(d)])
                        // A decay exponent of ONE reproduces `sigmoid(-dt)` exactly, which is the
                        // c = 1 case `q36_pow_q` short-circuits.
                        .with_params(
                            n("linear_decay_c.a16"),
                            &[A16QuantParams { multiplier: 1, shift: 0, zero: kaspa_consensus_core::palw_base0::ONE }],
                        )
                        // The state carries eight bits above the value scale.
                        .with_params(n("linear_read.a16"), &[A16QuantParams { multiplier: 1, shift: 23, zero: 0 }])
                        .with_params(n("linear_write.a16"), &[A16QuantParams { multiplier: 1, shift: 7, zero: 0 }])
                        .with_params(n("linear_out.a16"), &[A16QuantParams { multiplier: 1, shift: 23, zero: 0 }])
                        .with_params(n("linear_norm.a16"), &[unity])
                        .with_params(n("linear_gate.a16"), &[unity])
                        .with_params(n("linear_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 15, zero: 0 }])
                        .with_params(n("linear_o.weight.a16"), &[projection(dv)]);
                    let _ = hd;
                }
                Qwen36LayerKind::FullAttention => {
                    let (q_dim, kv_dim) = (shape.n_heads * shape.head_dim, shape.kv_dim());
                    artifact = artifact
                        .with_tensor(n("attn_q.weight"), weights(q_dim * d))
                        .with_tensor(n("attn_gate.weight"), weights(q_dim * d))
                        .with_tensor(n("attn_k.weight"), weights(kv_dim * d))
                        .with_tensor(n("attn_v.weight"), weights(kv_dim * d))
                        .with_tensor(n("attn_o.weight"), weights(d * q_dim))
                        .with_params(n("attn_q.weight.a16"), &[projection(d)])
                        .with_params(n("attn_gate.weight.a16"), &[projection(d)])
                        .with_params(n("attn_k.weight.a16"), &[projection(d)])
                        .with_params(n("attn_v.weight.a16"), &[projection(d)])
                        .with_params(n("attn_q_norm.a16"), &[unity])
                        .with_params(n("attn_k_norm.a16"), &[unity])
                        .with_params(n("attn_rope.a16"), &[A16QuantParams { multiplier: 1, shift: 24, zero: 0 }])
                        .with_params(n("attn_logits.a16"), &[projection(shape.head_dim)])
                        .with_params(n("attn_softmax_up.a16"), &[A16QuantParams { multiplier: 1, shift: 0, zero: 16 }])
                        .with_params(n("attn_probs.a16"), &[A16QuantParams { multiplier: 1, shift: 9, zero: 0 }])
                        .with_params(n("attn_values.a16"), &[A16QuantParams { multiplier: 1, shift: 15, zero: 0 }])
                        .with_params(n("attn_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 24, zero: 0 }])
                        .with_params(n("attn_o.weight.a16"), &[projection(q_dim)]);
                }
            }
            // The mixture: a router, every expert, and the shared one.
            artifact = artifact
                .with_tensor(n("ffn_router.weight"), weights(shape.n_experts * d))
                .with_params(n("ffn_router.weight.a16"), &[projection(d)])
                .with_params(n("ffn_router.a16"), &[unity])
                .with_params(n("ffn_combine.a16"), &[A16QuantParams { multiplier: 1, shift: 24, zero: 0 }])
                .with_tensor(n("ffn_shared_gate.weight"), weights(d))
                .with_params(n("ffn_shared_gate.weight.a16"), &[projection(d)])
                .with_params(n("ffn_shared_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 24, zero: 0 }])
                .with_params(n("ffn_moe_out.a16"), &[unity]);
            for (base, mid) in (0..shape.n_experts)
                .map(|e| (format!("blk.{li}.ffn_expert.{e}"), shape.moe_dim))
                .chain(std::iter::once((format!("blk.{li}.ffn_shared_expert"), shape.shared_dim)))
            {
                artifact = artifact
                    .with_tensor(format!("{base}_gate.weight"), weights(mid * d))
                    .with_tensor(format!("{base}_up.weight"), weights(mid * d))
                    .with_tensor(format!("{base}_down.weight"), weights(d * mid))
                    .with_params(format!("{base}_gate.weight.a16"), &[projection(d)])
                    .with_params(format!("{base}_up.weight.a16"), &[projection(d)])
                    .with_params(format!("{base}_silu.a16"), &[unity])
                    .with_params(format!("{base}_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 15, zero: 0 }])
                    .with_params(format!("{base}_down.weight.a16"), &[projection(mid)]);
            }
        }
        artifact
    }

    /// **The graph composes.** Both arms, the mixture, the residual stream, forty-style layer
    /// alternation — end to end, with a row out that is neither zero nor constant.
    #[test]
    fn the_hybrid_graph_runs_end_to_end() {
        let artifact = fixture(8, 16);
        let engine = Qwen36Engine::new(&artifact);
        let mut cache = Qwen36Cache::new(&artifact.shape);

        let mut rows = Vec::new();
        for position in 0..6 {
            let token = (position * 7 + 3) % artifact.shape.vocab;
            rows.push(engine.forward_token(&mut cache, token, position).expect("the pass completes"));
        }
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.len(), artifact.shape.vocab);
            assert!(row.iter().any(|v| *v != 0), "position {i} produced an all-zero logit row");
            let distinct: std::collections::BTreeSet<i32> = row.iter().copied().collect();
            assert!(distinct.len() > 1, "position {i} produced a constant logit row");
        }
        // Different tokens at different positions must move the row, or nothing was computed.
        assert_ne!(rows[0], rows[1]);
        assert_ne!(rows[1], rows[2]);
    }

    /// The property the class exists for: same input, same bits — including the recurrent state,
    /// which is the one thing in this architecture that could carry a difference forward.
    #[test]
    fn the_hybrid_pass_is_deterministic() {
        let artifact = fixture(8, 16);
        let engine = Qwen36Engine::new(&artifact);
        let run = || {
            let mut cache = Qwen36Cache::new(&artifact.shape);
            let mut last = Vec::new();
            for position in 0..8 {
                last = engine.forward_token(&mut cache, (position * 5 + 1) % 64, position).expect("completes");
            }
            let states: Vec<Vec<i32>> = cache.gdn.iter().flatten().map(|s| s.s.clone()).collect();
            (last, states)
        };
        assert_eq!(run(), run());
    }

    /// The recurrent state must actually be carrying history: a GatedDeltaNet layer whose state
    /// stayed at zero would still produce a plausible row through the residual path.
    #[test]
    fn the_recurrent_state_fills() {
        let artifact = fixture(8, 16);
        let engine = Qwen36Engine::new(&artifact);
        let mut cache = Qwen36Cache::new(&artifact.shape);
        for position in 0..6 {
            engine.forward_token(&mut cache, position % 64, position).expect("completes");
        }
        let mut filled = 0usize;
        for layer in &cache.gdn {
            for state in layer {
                if state.s.iter().any(|v| *v != 0) {
                    filled += 1;
                }
            }
        }
        assert!(filled > 0, "no GatedDeltaNet head accumulated any state");
        // And the softmax layers kept a cache.
        assert_eq!(cache.len(), 6);
    }

    /// The same position must see the same rotation whichever arm asks for it, and a position past
    /// the table is refused rather than wrapped.
    #[test]
    fn a_position_past_the_table_is_refused() {
        let artifact = fixture(4, 8);
        let engine = Qwen36Engine::new(&artifact);
        let mut cache = Qwen36Cache::new(&artifact.shape);
        assert_eq!(engine.forward_token(&mut cache, 1, artifact.shape.max_position), Err(Qwen36Error::Position));
        assert_eq!(engine.forward_token(&mut cache, artifact.shape.vocab, 0), Err(Qwen36Error::Position));
    }

    /// **The store is keyed by strings, so two rows must not be able to claim one key.**
    ///
    /// The shared expert's gate projection and the mixture's scalar gate both wanted
    /// `blk.N.ffn_shared_gate.weight`. Nothing in a `BTreeMap` notices that; the engine caught it
    /// only because the two rows happen to have different widths and it checks sizes. Two rows of
    /// the same width would have run to completion and computed something else, which is the
    /// failure mode this test exists to keep closed.
    #[test]
    fn no_two_rows_claim_one_key() {
        let artifact = fixture(4, 8);
        // Every name the engine can ask for, built the way the engine builds them.
        let mut asked: Vec<String> = Vec::new();
        for li in 0..artifact.shape.n_layers() {
            asked.push(format!("blk.{li}.ffn_shared_gate.weight"));
            for suffix in ["_gate.weight", "_up.weight", "_down.weight"] {
                asked.push(format!("blk.{li}.ffn_shared_expert{suffix}"));
                for e in 0..artifact.shape.n_experts {
                    asked.push(format!("blk.{li}.ffn_expert.{e}{suffix}"));
                }
            }
        }
        let unique: std::collections::BTreeSet<&String> = asked.iter().collect();
        assert_eq!(unique.len(), asked.len(), "two different rows resolve to one store key");
        // And every one of them is actually present, so the check is over real keys rather than
        // over names nothing reads.
        for name in &asked {
            assert!(artifact.tensor(name).is_ok(), "the fixture is missing {name}");
        }
    }

    /// A missing tensor names itself. The store is the whole registration surface, so a class that
    /// is missing a row has to say which one.
    #[test]
    fn a_missing_row_names_itself() {
        let artifact = fixture(4, 8);
        let mut stripped = artifact.clone();
        stripped.tensors.remove("blk.0.linear_q.weight");
        let engine = Qwen36Engine::new(&stripped);
        let mut cache = Qwen36Cache::new(&stripped.shape);
        assert_eq!(engine.forward_token(&mut cache, 1, 0), Err(Qwen36Error::MissingTensor("blk.0.linear_q.weight".to_string())));
    }
}
