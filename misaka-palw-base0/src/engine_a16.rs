//! **The W8A16 engine, as a composition of catalogued ops (ADR-0040 W / ADR-0047).**
//!
//! Every runtime parameter is an integer `(m, shift, zero)` triple read from
//! [`Base0ArtifactV1::a16_params`] — the SAME store the dispute oracle serves — and every
//! arithmetic step delegates to `kaspa_consensus_core::palw_base0_a16` (plus the reused BASE-0
//! rows: `silu` and the embedding gather). The adjudicator must run the same code a conforming
//! implementation runs; here the implementation runs the adjudicator's.
//!
//! The attention arms are the GQA ops (`a16_attn_scores` / `a16_softmax_rows` /
//! `a16_attn_values`) over the SERIES layout the court's canonical input set concatenates —
//! full `kv_dim` cache rows, position-major — so the engine's committed rows and the court's
//! recomputations are the same shapes by construction, which
//! [`A16Engine::forward_token_traced`] exposes and the full-job replay verifies node by node.
//!
//! Position zero is the sink lane: the seams whose scales the attention-sink token sets resolve
//! their parameters with the `.sink0` suffix, exactly the rule `a16_row` applies in court.
//!
//! The output row is the committed one: i16 LOGIT CODES (the tile lane is 4 bytes), so the
//! class's argmax — lowest index on ties — is defined over the narrowed codes here and in any
//! dispute alike.

use crate::artifact::Base0ArtifactV1;
use kaspa_consensus_core::palw_base0_a16::{
    A16QuantParams, a16_add_elem, a16_attn_scores, a16_attn_values, a16_matmul_requant, a16_matmul_rescale, a16_mul_elem, a16_requant,
    a16_rms_norm, a16_rope, a16_softmax_rows,
};
use kaspa_consensus_core::palw_base0_ops::silu;

/// Why the engine refused to run. Everything here is a REGISTRATION defect — a missing or
/// malformed parameter row — surfaced at construction, never mid-forward.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum A16EngineError {
    MissingParams(&'static str),
    MalformedParams(&'static str),
    OpRefused(&'static str),
    PositionOutOfRange,
}

/// One position's forward, every node's committed row, in the shape profile's numbering: the
/// replay surface. `pre` and `post` are per node; `attn` is per layer, per node.
#[derive(Clone, Debug, Default)]
pub struct A16TraceV1 {
    pub pre: Vec<Vec<i32>>,
    pub attn: Vec<Vec<Vec<i32>>>,
    pub post: Vec<Vec<i32>>,
}

/// One layer's pre-resolved parameter tables (generic and, where the class carries them, sink).
struct LayerParams {
    attn_norm: Vec<A16QuantParams>,
    q: Vec<A16QuantParams>,
    k: Vec<A16QuantParams>,
    v: Vec<A16QuantParams>,
    logits: A16QuantParams,
    softmax_up: u8,
    probs: A16QuantParams,
    values: A16QuantParams,
    wo: Vec<A16QuantParams>,
    wo_sink: Vec<A16QuantParams>,
    attn_align: A16QuantParams,
    attn_align_sink: A16QuantParams,
    attn_residual: A16QuantParams,
    ffn_norm: Vec<A16QuantParams>,
    gate: Vec<A16QuantParams>,
    silu_q: A16QuantParams,
    silu_sink: A16QuantParams,
    up: Vec<A16QuantParams>,
    up_sink: Vec<A16QuantParams>,
    gated: A16QuantParams,
    gated_sink: A16QuantParams,
    down: Vec<A16QuantParams>,
    down_sink: Vec<A16QuantParams>,
    ffn_align: A16QuantParams,
    ffn_align_sink: A16QuantParams,
    ffn_residual: A16QuantParams,
}

pub struct A16Engine<'a> {
    pub artifact: &'a Base0ArtifactV1,
    embed_lift: A16QuantParams,
    final_norm: Vec<A16QuantParams>,
    logits_out: A16QuantParams,
    layers: Vec<LayerParams>,
}

pub struct A16Cache {
    keys: Vec<Vec<Vec<i32>>>,
    values: Vec<Vec<Vec<i32>>>,
}

impl A16Cache {
    pub fn new(layers: usize) -> Self {
        Self { keys: vec![Vec::new(); layers], values: vec![Vec::new(); layers] }
    }
    pub fn len(&self) -> usize {
        self.keys.first().map_or(0, |k| k.len())
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn parse_rows(bytes: &[u8]) -> Result<Vec<A16QuantParams>, A16EngineError> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(A16QuantParams::WIRE_BYTES) {
        return Err(A16EngineError::MalformedParams("params bytes are not whole 17-byte triples"));
    }
    bytes
        .chunks_exact(A16QuantParams::WIRE_BYTES)
        .map(|c| A16QuantParams::from_wire(c).map_err(|_| A16EngineError::MalformedParams("triple out of domain")))
        .collect()
}

impl<'a> A16Engine<'a> {
    /// Resolve every parameter table up front: a class whose registration is missing a row is
    /// refused HERE, not discovered as a wrong number three layers into a forward pass.
    pub fn new(artifact: &'a Base0ArtifactV1) -> Result<Self, A16EngineError> {
        let one = |template: &str, layer: Option<u16>, what: &'static str| -> Result<A16QuantParams, A16EngineError> {
            let rows = parse_rows(artifact.a16_param(template, layer).ok_or(A16EngineError::MissingParams(what))?)?;
            if rows.len() != 1 {
                return Err(A16EngineError::MalformedParams(what));
            }
            Ok(rows[0])
        };
        let many =
            |template: &str, layer: Option<u16>, want: usize, what: &'static str| -> Result<Vec<A16QuantParams>, A16EngineError> {
                let rows = parse_rows(artifact.a16_param(template, layer).ok_or(A16EngineError::MissingParams(what))?)?;
                if rows.len() != want {
                    return Err(A16EngineError::MalformedParams(what));
                }
                Ok(rows)
            };
        let shape = &artifact.shape;
        let (d, kv, ff) = (shape.d_model(), shape.kv_dim(), shape.d_ff);
        let mut layers = Vec::with_capacity(shape.n_layers);
        for li in 0..shape.n_layers {
            let l = Some(li as u16);
            let up_bits = artifact.a16_param("blk.{layer}.attn_softmax_up", l).ok_or(A16EngineError::MissingParams("softmax_up"))?;
            if up_bits.len() != 1 || up_bits[0] > 62 {
                return Err(A16EngineError::MalformedParams("softmax_up"));
            }
            layers.push(LayerParams {
                attn_norm: many("blk.{layer}.attn_norm.a16", l, d, "attn_norm")?,
                q: many("blk.{layer}.attn_q.weight.a16", l, d, "q")?,
                k: many("blk.{layer}.attn_k.weight.a16", l, kv, "k")?,
                v: many("blk.{layer}.attn_v.weight.a16", l, kv, "v")?,
                logits: one("blk.{layer}.attn_logits.a16", l, "logits")?,
                softmax_up: up_bits[0],
                probs: one("blk.{layer}.attn_probs.a16", l, "probs")?,
                values: one("blk.{layer}.attn_values.a16", l, "values")?,
                wo: many("blk.{layer}.attn_output.weight.a16", l, d, "wo")?,
                wo_sink: many("blk.{layer}.attn_output.weight.a16.sink0", l, d, "wo sink")?,
                attn_align: one("blk.{layer}.attn_align.a16", l, "attn_align")?,
                attn_align_sink: one("blk.{layer}.attn_align.a16.sink0", l, "attn_align sink")?,
                attn_residual: one("blk.{layer}.attn_residual.a16", l, "attn_residual")?,
                ffn_norm: many("blk.{layer}.ffn_norm.a16", l, d, "ffn_norm")?,
                gate: many("blk.{layer}.ffn_gate.weight.a16", l, ff, "gate")?,
                silu_q: one("blk.{layer}.ffn_silu.a16", l, "silu")?,
                silu_sink: one("blk.{layer}.ffn_silu.a16.sink0", l, "silu sink")?,
                up: many("blk.{layer}.ffn_up.weight.a16", l, ff, "up")?,
                up_sink: many("blk.{layer}.ffn_up.weight.a16.sink0", l, ff, "up sink")?,
                gated: one("blk.{layer}.ffn_gated.a16", l, "gated")?,
                gated_sink: one("blk.{layer}.ffn_gated.a16.sink0", l, "gated sink")?,
                down: many("blk.{layer}.ffn_down.weight.a16", l, d, "down")?,
                down_sink: many("blk.{layer}.ffn_down.weight.a16.sink0", l, d, "down sink")?,
                ffn_align: one("blk.{layer}.ffn_align.a16", l, "ffn_align")?,
                ffn_align_sink: one("blk.{layer}.ffn_align.a16.sink0", l, "ffn_align sink")?,
                ffn_residual: one("blk.{layer}.ffn_residual.a16", l, "ffn_residual")?,
            });
        }
        Ok(Self {
            artifact,
            embed_lift: one("embed_lift.a16", None, "embed_lift")?,
            final_norm: many("final_norm.a16", None, d, "final_norm")?,
            logits_out: one("token_embd.weight.a16", None, "logits_out")?,
            layers,
        })
    }

    /// One token; returns the COMMITTED logit row: i16 codes in i32 lanes. Ties in any argmax
    /// over this row break to the lowest index, here and in court alike.
    pub fn forward_token(&self, cache: &mut A16Cache, token_id: usize, position: usize) -> Result<Vec<i32>, A16EngineError> {
        self.forward_token_traced(cache, token_id, position).map(|(l, _)| l)
    }

    /// As [`forward_token`], plus the per-layer residual streams (measurement).
    pub fn forward_token_probed(
        &self,
        cache: &mut A16Cache,
        token_id: usize,
        position: usize,
    ) -> Result<(Vec<i32>, Vec<Vec<i32>>), A16EngineError> {
        let (logits, trace) = self.forward_token_traced(cache, token_id, position)?;
        let streams = trace.attn.iter().map(|nodes| nodes.last().cloned().unwrap_or_default()).collect();
        Ok((logits, streams))
    }

    /// **The replay surface: one position's forward with EVERY node's committed row recorded,
    /// in the shape profile's numbering.** The full-job replay adjudicates each of these rows
    /// through the court's own dispatch and demands bit equality.
    pub fn forward_token_traced(
        &self,
        cache: &mut A16Cache,
        token_id: usize,
        position: usize,
    ) -> Result<(Vec<i32>, A16TraceV1), A16EngineError> {
        let shape = &self.artifact.shape;
        let d = shape.d_model();
        let kv_dim = shape.kv_dim();
        let refuse =
            |what: &'static str| move |_e: kaspa_consensus_core::palw_base0_a16::PalwA16OpError| A16EngineError::OpRefused(what);
        let (cos_row, sin_row) = self.artifact.rope.row(position).ok_or(A16EngineError::PositionOutOfRange)?;
        let sink = position == 0;
        let tile = |p: A16QuantParams, n: usize| -> Vec<A16QuantParams> { vec![p; n] };
        let mut trace = A16TraceV1::default();

        // ---- pre: the gather (node 0) and the lift onto the A16 stream (node 1) -------------
        let embed_row: Vec<i32> = self.artifact.embed[token_id * d..(token_id + 1) * d].iter().map(|c| *c as i32).collect();
        trace.pre.push(embed_row.clone());
        let mut h = a16_requant(&embed_row, &tile(self.embed_lift, d)).map_err(refuse("embed_lift"))?;
        trace.pre.push(h.clone());

        for (li, lp) in self.layers.iter().enumerate() {
            let lw = &self.artifact.layers[li];
            let mut nodes: Vec<Vec<i32>> = Vec::with_capacity(27);
            let push = |nodes: &mut Vec<Vec<i32>>, row: Vec<i32>| -> Vec<i32> {
                nodes.push(row.clone());
                row
            };

            // ---- attention (nodes 0..=14) ---------------------------------------------------
            let unit = push(&mut nodes, a16_rms_norm(&h, shape.eps_q).map_err(refuse("norm1"))?);
            let normed = push(&mut nodes, a16_requant(&unit, &lp.attn_norm).map_err(refuse("norm1_req"))?);
            let q = push(&mut nodes, a16_matmul_requant(&lw.wq, &normed, &lp.q).map_err(refuse("q"))?);
            let k = push(&mut nodes, a16_matmul_requant(&lw.wk, &normed, &lp.k).map_err(refuse("k"))?);
            let v = push(&mut nodes, a16_matmul_requant(&lw.wv, &normed, &lp.v).map_err(refuse("v"))?);
            let rope_heads = |row: &[i32], heads: usize, what: &'static str| -> Result<Vec<i32>, A16EngineError> {
                let mut out = Vec::with_capacity(row.len());
                for hd in 0..heads {
                    let slice = &row[hd * shape.d_head..(hd + 1) * shape.d_head];
                    out.extend(a16_rope(slice, cos_row, sin_row).map_err(|_| A16EngineError::OpRefused(what))?);
                }
                Ok(out)
            };
            let q_rot = push(&mut nodes, rope_heads(&q, shape.n_heads, "rope_q")?);
            let k_rot = push(&mut nodes, rope_heads(&k, shape.n_kv_heads, "rope_k")?);
            cache.keys[li].push(k_rot);
            cache.values[li].push(v.clone());
            let history = cache.keys[li].len();

            // The cache series, EXACTLY as the court's canonical input set concatenates them:
            // full kv_dim rows, position-major.
            let mut k_series = Vec::with_capacity(history * kv_dim);
            let mut v_series = Vec::with_capacity(history * kv_dim);
            for j in 0..history {
                k_series.extend_from_slice(&cache.keys[li][j]);
                v_series.extend_from_slice(&cache.values[li][j]);
            }

            let logits_row = push(
                &mut nodes,
                a16_attn_scores(
                    &q_rot,
                    &k_series,
                    shape.n_heads,
                    shape.n_kv_heads,
                    shape.d_head,
                    &tile(lp.logits, shape.n_heads * history),
                )
                .map_err(refuse("logits"))?,
            );
            let probs_row = push(&mut nodes, a16_softmax_rows(&logits_row, history, lp.softmax_up).map_err(refuse("softmax"))?);
            let p15 = push(&mut nodes, a16_requant(&probs_row, &tile(lp.probs, shape.n_heads * history)).map_err(refuse("p15"))?);
            let attn = push(
                &mut nodes,
                a16_attn_values(
                    &p15,
                    &v_series,
                    shape.n_heads,
                    shape.n_kv_heads,
                    shape.d_head,
                    &tile(lp.values, shape.n_heads * shape.d_head),
                )
                .map_err(refuse("values"))?,
            );

            let wo_params = if sink { &lp.wo_sink } else { &lp.wo };
            let delta = push(&mut nodes, a16_matmul_requant(&lw.wo, &attn, wo_params).map_err(refuse("wo"))?);
            let align = if sink { lp.attn_align_sink } else { lp.attn_align };
            let aligned = push(&mut nodes, a16_requant(&h, &tile(align, d)).map_err(refuse("attn_align"))?);
            let sum = push(&mut nodes, a16_add_elem(&aligned, &delta).map_err(refuse("attn_add"))?);
            h = push(&mut nodes, a16_requant(&sum, &tile(lp.attn_residual, d)).map_err(refuse("attn_res"))?);

            // ---- SwiGLU (nodes 15..=26) -----------------------------------------------------
            let unit = push(&mut nodes, a16_rms_norm(&h, shape.eps_q).map_err(refuse("norm2"))?);
            let normed = push(&mut nodes, a16_requant(&unit, &lp.ffn_norm).map_err(refuse("norm2_req"))?);
            let gate_q = push(&mut nodes, a16_matmul_rescale(&lw.w_gate, &normed, &lp.gate).map_err(refuse("gate"))?);
            let silu_q = push(&mut nodes, silu(&gate_q));
            let s_p = if sink { lp.silu_sink } else { lp.silu_q };
            let s16 = push(&mut nodes, a16_requant(&silu_q, &tile(s_p, shape.d_ff)).map_err(refuse("silu16"))?);
            let up_params = if sink { &lp.up_sink } else { &lp.up };
            let up = push(&mut nodes, a16_matmul_requant(&lw.w_up, &normed, up_params).map_err(refuse("up"))?);
            let prod = push(&mut nodes, a16_mul_elem(&s16, &up).map_err(refuse("mul"))?);
            let g_p = if sink { lp.gated_sink } else { lp.gated };
            let gated = push(&mut nodes, a16_requant(&prod, &tile(g_p, shape.d_ff)).map_err(refuse("gated"))?);
            let down_params = if sink { &lp.down_sink } else { &lp.down };
            let delta = push(&mut nodes, a16_matmul_requant(&lw.w_down, &gated, down_params).map_err(refuse("down"))?);
            let align = if sink { lp.ffn_align_sink } else { lp.ffn_align };
            let aligned = push(&mut nodes, a16_requant(&h, &tile(align, d)).map_err(refuse("ffn_align"))?);
            let sum = push(&mut nodes, a16_add_elem(&aligned, &delta).map_err(refuse("ffn_add"))?);
            h = push(&mut nodes, a16_requant(&sum, &tile(lp.ffn_residual, d)).map_err(refuse("ffn_res"))?);
            trace.attn.push(nodes);
        }

        // ---- post: final norm and the TIED logits, committed as i16 codes -------------------
        let unit = a16_rms_norm(&h, shape.eps_q).map_err(refuse("final_norm"))?;
        trace.post.push(unit.clone());
        let fin = a16_requant(&unit, &self.final_norm).map_err(refuse("final_req"))?;
        trace.post.push(fin.clone());
        let logits =
            a16_matmul_requant(&self.artifact.unembed, &fin, &tile(self.logits_out, shape.vocab)).map_err(refuse("logits_out"))?;
        trace.post.push(logits.clone());
        Ok((logits, trace))
    }
}
