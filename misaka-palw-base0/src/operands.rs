//! **One binding from an IR tensor name to this artifact's bytes** (ADR-0049 Decision F/G).
//!
//! `BASE0_LAYER_IR` names every operand a step reads — `blk.{layer}.attn_q.weight`,
//! `blk.{layer}.qk_to_code.requant`, `blk.{layer}.attn_residual.scale`. Three separate pieces of
//! code used to know where those names live inside a [`Base0ArtifactV1`]:
//!
//! * the engine, which reached for `layer.wq` / `layer.requant[0]` / `artifact.qk_to_code()` by
//!   hand, in the order it happened to perform its steps;
//! * the inventory, which listed the same suffixes beside the same fields in a second table;
//! * the court, which asks for a name and a layer index and must be served whatever the producer
//!   actually used.
//!
//! Nothing checked that the three agreed, and one already did not: the inventory served
//! `layer.requant[0]` for `attn_q.requant` unconditionally while the engine narrows through
//! [`Base0LayerWeightsV1::qkv_channel_requant`] when the artifact carries one. For a class that
//! does — every Qwen2.5 member does, because its projections carry a bias and a bias is
//! per-channel — a court opening that tensor would recompute an honest producer's step against
//! parameters the producer never applied, and convict it. That is the one verdict this court may
//! never return.
//!
//! So the mapping exists once, here, and the engine and the inventory both read through it. An
//! operand the engine cannot resolve is a step the court cannot open, and both find out at the
//! same moment.
//!
//! # The name is the TEMPLATE, and the layer rides beside it
//!
//! `palw_step_refute` asks the weight oracle with `node.weight_name` — the IR's own
//! `blk.{layer}.…` string — and passes the layer index as a separate field. A resolver keyed on a
//! substituted name would answer no request the court ever makes, so this one is keyed the same
//! way the court asks.

use kaspa_consensus_core::palw_base0_ops::{QuantParams, ScaleParams};

use crate::artifact::Base0ArtifactV1;
use crate::rope::RopeTableV1;

/// The `blk.{layer}.` prefix every per-layer tensor name carries.
pub const BASE0_LAYER_PREFIX: &str = "blk.{layer}.";

/// **The head tensor's placeholder.**
///
/// The lm_head is the one operand that is a property of the CLASS rather than of the graph: the
/// floor reads `output.weight`, and a class with tied embeddings reads `token_embd.weight` and
/// carries no `output.weight` at all. Everything else in the IR is named outright.
pub const BASE0_IR_HEAD_TENSOR: &str = "{head}";

/// A narrowing's parameters, tensor-wide or per output channel.
///
/// Two shapes because the arithmetic has two: `requantize_row_uniform` takes one parameter set for
/// a whole row, `requantize_row` takes one per element. Which one an operand is is a property of
/// the artifact — not of the graph — which is why the graph names the tensor and this answers with
/// what is in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base0QuantOperandV1<'a> {
    Uniform(QuantParams),
    PerChannel(&'a [QuantParams]),
}

impl Base0QuantOperandV1<'_> {
    /// The nine bytes per channel `palw_step_refute` reads: `(multiplier LE, shift, zero LE)`.
    pub fn bytes(&self) -> Vec<u8> {
        let one = |q: &QuantParams, out: &mut Vec<u8>| {
            out.extend_from_slice(&q.multiplier.to_le_bytes());
            out.push(q.shift);
            out.extend_from_slice(&q.zero.to_le_bytes());
        };
        match self {
            Self::Uniform(q) => {
                let mut out = Vec::with_capacity(9);
                one(q, &mut out);
                out
            }
            Self::PerChannel(ch) => {
                let mut out = Vec::with_capacity(9 * ch.len());
                for q in *ch {
                    one(q, &mut out);
                }
                out
            }
        }
    }
}

/// What a name resolves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base0OperandV1<'a> {
    /// A projection matrix, `[out_dim][in_dim]` row-major `int8`. `in_dim` is carried because a
    /// `MatMulQuant` opening addresses a tile of OUTPUT rows and the byte offset is `tile · in_dim`.
    Matrix {
        data: &'a [i8],
        in_dim: usize,
    },
    /// A gather table, `[rows][width]` — one row per token id, opened as the row it gathered.
    Gather {
        data: &'a [i8],
        width: usize,
    },
    Quant(Base0QuantOperandV1<'a>),
    Scale(ScaleParams),
    /// The rotary table, `[position][pair]`; an opening addresses one position's row.
    Rope(&'a RopeTableV1),
}

/// Why a name resolves to nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperandError {
    /// No operand of this artifact carries that name. A step naming one is unadjudicable, so the
    /// engine refuses to execute it rather than substituting something plausible.
    UnknownTensor { name: String },
    /// A per-layer name arrived without a layer index, or with one past the stack.
    BadLayer { name: String, layer: Option<usize>, layers: usize },
}

impl std::fmt::Display for OperandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTensor { name } => write!(f, "no operand named {name}"),
            Self::BadLayer { name, layer, layers } => write!(f, "{name} at layer {layer:?} of {layers}"),
        }
    }
}

impl std::error::Error for OperandError {}

/// **Resolve one IR tensor name against one artifact.**
///
/// `name` is the IR's template (`blk.{layer}.attn_q.weight`, `output.weight`, or
/// [`BASE0_IR_HEAD_TENSOR`] for a class whose head is `head_tensor`), and `layer` is the index the
/// court passes beside it — `None` for the graph-level tensors.
pub fn base0_resolve_operand_v1<'a>(
    artifact: &'a Base0ArtifactV1,
    name: &str,
    layer: Option<usize>,
    head_tensor: &str,
) -> Result<Base0OperandV1<'a>, OperandError> {
    let shape = &artifact.shape;
    let d = shape.d_model();
    let name = if name == BASE0_IR_HEAD_TENSOR { head_tensor } else { name };

    // --- graph level -------------------------------------------------------------------------
    if let Some(op) = match name {
        "token_embd.weight" => Some(Base0OperandV1::Gather { data: &artifact.embed, width: d }),
        "output.weight" => Some(Base0OperandV1::Matrix { data: &artifact.unembed, in_dim: d }),
        "output_norm.requant" => Some(Base0OperandV1::Quant(Base0QuantOperandV1::Uniform(artifact.norm_requant))),
        _ => None,
    } {
        // A graph-level tensor asked for at a layer is a caller that thinks it is per-layer, and
        // answering anyway is how two descriptions of one operand come to exist again.
        return match layer {
            None => Ok(op),
            Some(_) => Err(OperandError::BadLayer { name: name.to_string(), layer, layers: shape.n_layers }),
        };
    }

    // --- per layer ---------------------------------------------------------------------------
    let Some(suffix) = name.strip_prefix(BASE0_LAYER_PREFIX) else {
        return Err(OperandError::UnknownTensor { name: name.to_string() });
    };
    let Some(li) = layer.filter(|l| *l < shape.n_layers) else {
        return Err(OperandError::BadLayer { name: name.to_string(), layer, layers: shape.n_layers });
    };
    let w = &artifact.layers[li];

    // The narrowing for q, k and v is per-channel when the artifact carries one — which is where a
    // projection BIAS lives, in each channel's `zero`. The engine has always narrowed this way;
    // until this function existed the inventory served the tensor-wide parameters regardless, so
    // the court would have recomputed an honest step against parameters nobody applied.
    let qkv = |idx: usize| match &w.qkv_channel_requant {
        Some(per) => Base0QuantOperandV1::PerChannel(&per[idx]),
        None => Base0QuantOperandV1::Uniform(w.requant[idx]),
    };
    let uniform = |q: QuantParams| Base0OperandV1::Quant(Base0QuantOperandV1::Uniform(q));

    Ok(match suffix {
        // Projections. `in_dim` is `d_model` for every one of them but the down projection, which
        // reads the FFN width.
        "attn_q.weight" => Base0OperandV1::Matrix { data: &w.wq, in_dim: d },
        "attn_k.weight" => Base0OperandV1::Matrix { data: &w.wk, in_dim: d },
        "attn_v.weight" => Base0OperandV1::Matrix { data: &w.wv, in_dim: d },
        "attn_output.weight" => Base0OperandV1::Matrix { data: &w.wo, in_dim: d },
        "ffn_gate.weight" => Base0OperandV1::Matrix { data: &w.w_gate, in_dim: d },
        "ffn_up.weight" => Base0OperandV1::Matrix { data: &w.w_up, in_dim: d },
        "ffn_down.weight" => Base0OperandV1::Matrix { data: &w.w_down, in_dim: shape.d_ff },

        // Narrowings.
        "attn_norm.requant" | "ffn_norm.requant" => uniform(artifact.norm_requant),
        "attn_q.requant" => Base0OperandV1::Quant(qkv(0)),
        "attn_k.requant" => Base0OperandV1::Quant(qkv(1)),
        "attn_v.requant" => Base0OperandV1::Quant(qkv(2)),
        "attn_output.requant" => uniform(w.requant[3]),
        "ffn_up.requant" => uniform(w.requant[5]),
        "ffn_down.requant" => uniform(w.requant[6]),
        // The three the engine used to hold as `const`, now artifact data an opening can address.
        "qk_to_code.requant" => uniform(artifact.qk_to_code()),
        "code_product.requant" => uniform(artifact.code_product()),
        "rope_clamp.requant" => uniform(artifact.rope_clamp()),
        "attn_residual.requant" => uniform(artifact.residual_requant_at(li, 0)),
        "ffn_residual.requant" => uniform(artifact.residual_requant_at(li, 1)),

        // Gains — the ops that are allowed to amplify (ADR-0040 H, ADR-0050 B/D).
        "attn_logit.scale" => Base0OperandV1::Scale(w.attn_logit_scale),
        "ffn_gate.scale" => Base0OperandV1::Scale(w.ffn_gate_scale),
        "attn_residual.scale" => Base0OperandV1::Scale(artifact.residual_scale_at(li, 0)),
        "ffn_residual.scale" => Base0OperandV1::Scale(artifact.residual_scale_at(li, 1)),

        "rope_table" => Base0OperandV1::Rope(&artifact.rope),

        _ => return Err(OperandError::UnknownTensor { name: name.to_string() }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use kaspa_consensus_core::palw_base0_profile::{base0_tensor_names_v1, BASE0_LAYER_IR};

    fn shape() -> Base0ShapeV1 {
        Base0ShapeV1 {
            n_layers: 2,
            n_heads: 4,
            n_kv_heads: 2,
            d_head: 8,
            d_ff: 64,
            vocab: 32,
            max_position: 8,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1 << 10,
        }
    }

    fn artifact() -> Base0ArtifactV1 {
        Base0ArtifactV1::derive_deterministic(shape(), 7).expect("a derived artifact")
    }

    /// **Every name the graph reads resolves — at every layer.**
    ///
    /// A tensor the graph names and the artifact cannot answer for is a step that adjudicates
    /// `Unadjudicable`: coverage-clean and unprosecutable, which is the exact shape ADR-0049
    /// exists to refuse. This is the assertion that the IR and the container agree, and it is
    /// checked here rather than in the engine so it holds for a class nobody has run yet.
    #[test]
    fn every_tensor_the_graph_names_resolves() {
        let a = artifact();
        for ir in BASE0_LAYER_IR {
            if ir.weight.is_empty() {
                continue;
            }
            for li in 0..a.shape.n_layers {
                base0_resolve_operand_v1(&a, ir.weight, Some(li), "output.weight")
                    .unwrap_or_else(|e| panic!("{} at layer {li}: {e}", ir.weight));
            }
        }
        for name in base0_tensor_names_v1() {
            let layer = if name.starts_with(BASE0_LAYER_PREFIX) { Some(0) } else { None };
            base0_resolve_operand_v1(&a, name, layer, "output.weight").unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }

    /// The head placeholder is the only name whose meaning is the class's rather than the graph's.
    #[test]
    fn the_head_placeholder_follows_the_class() {
        let a = artifact();
        let untied = base0_resolve_operand_v1(&a, BASE0_IR_HEAD_TENSOR, None, "output.weight").expect("the floor's head");
        assert!(matches!(untied, Base0OperandV1::Matrix { data, .. } if std::ptr::eq(data, a.unembed.as_slice())));
        let tied = base0_resolve_operand_v1(&a, BASE0_IR_HEAD_TENSOR, None, "token_embd.weight").expect("a tied head");
        assert!(matches!(tied, Base0OperandV1::Gather { data, .. } if std::ptr::eq(data, a.embed.as_slice())));
    }

    /// **The defect this module exists to make impossible.**
    ///
    /// `attn_q.requant` is per-channel exactly when the artifact carries a per-channel table, and
    /// the answer must not depend on which of the three callers asked. The inventory used to
    /// answer `requant[0]` unconditionally, so for a class with a projection bias the court would
    /// have opened parameters the producer never applied.
    #[test]
    fn a_per_channel_narrowing_resolves_per_channel() {
        let mut a = artifact();
        let d = a.shape.d_model();
        let kv = a.shape.kv_dim();
        let mark = |n: usize, zero: i32| -> Vec<QuantParams> {
            (0..n).map(|i| QuantParams { multiplier: i32::MAX, shift: 7, zero: zero + i as i32 }).collect()
        };
        for l in a.layers.iter_mut() {
            l.qkv_channel_requant = Some([mark(d, 100), mark(kv, 200), mark(kv, 300)]);
        }

        for (name, want_zero, want_len) in [("attn_q.requant", 100, d), ("attn_k.requant", 200, kv), ("attn_v.requant", 300, kv)] {
            let full = format!("{BASE0_LAYER_PREFIX}{name}");
            let got = base0_resolve_operand_v1(&a, &full, Some(1), "output.weight").expect("resolves");
            match got {
                Base0OperandV1::Quant(Base0QuantOperandV1::PerChannel(ch)) => {
                    assert_eq!(ch.len(), want_len, "{name} covers every output channel");
                    assert_eq!(ch[0].zero, want_zero, "{name} answered with another tensor's table");
                    assert_eq!(
                        Base0QuantOperandV1::PerChannel(ch).bytes().len(),
                        9 * want_len,
                        "{name} serves nine bytes per channel"
                    );
                }
                other => panic!("{name} resolved tensor-wide: {other:?}"),
            }
        }

        // Without the table, the same names are the tensor-wide narrowings — which is what every
        // artifact built before per-channel requantisation existed means.
        for l in a.layers.iter_mut() {
            l.qkv_channel_requant = None;
        }
        let got = base0_resolve_operand_v1(&a, "blk.{layer}.attn_q.requant", Some(1), "output.weight").expect("resolves");
        assert!(matches!(got, Base0OperandV1::Quant(Base0QuantOperandV1::Uniform(_))));
    }

    /// A per-layer name with no layer, and a graph-level name with one, are both refused. Either
    /// would mean the caller and the court disagree about what an operand is addressed by.
    #[test]
    fn the_layer_index_is_part_of_the_address() {
        let a = artifact();
        assert!(matches!(
            base0_resolve_operand_v1(&a, "blk.{layer}.attn_q.weight", None, "output.weight"),
            Err(OperandError::BadLayer { .. })
        ));
        assert!(matches!(
            base0_resolve_operand_v1(&a, "blk.{layer}.attn_q.weight", Some(99), "output.weight"),
            Err(OperandError::BadLayer { .. })
        ));
        assert!(matches!(base0_resolve_operand_v1(&a, "output.weight", Some(0), "output.weight"), Err(OperandError::BadLayer { .. })));
        assert!(matches!(
            base0_resolve_operand_v1(&a, "blk.{layer}.attn_norm.weight", Some(0), "output.weight"),
            Err(OperandError::UnknownTensor { .. })
        ));
    }
}
