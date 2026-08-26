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

use crate::artifact::{Base0ArtifactV1, Base0ShapeV1};
use crate::kernels::{a16_matmul_requant_batch, a16_matmul_rescale_batch};
use kaspa_consensus_core::palw_base0_a16::{
    A16QuantParams, a16_add_elem, a16_mul_elem, a16_requant, a16_rms_norm, a16_rope, a16_softmax_rows,
};

/// Op W9 through whichever implementation this engine was built with.
#[inline]
fn a16_attn_scores(
    fast: bool,
    q: &[i32],
    k: &[i32],
    heads: usize,
    kv_heads: usize,
    d_head: usize,
    p: &[A16QuantParams],
) -> Result<Vec<i32>, PalwA16OpError> {
    if fast {
        crate::kernels::a16_attn_scores_fast(q, k, heads, kv_heads, d_head, p)
    } else {
        catalog_attn_scores(q, k, heads, kv_heads, d_head, p)
    }
}

/// Op W10 likewise.
#[inline]
fn a16_attn_values(
    fast: bool,
    probs: &[i32],
    v: &[i32],
    heads: usize,
    kv_heads: usize,
    d_head: usize,
    p: &[A16QuantParams],
) -> Result<Vec<i32>, PalwA16OpError> {
    if fast {
        crate::kernels::a16_attn_values_fast(probs, v, heads, kv_heads, d_head, p)
    } else {
        catalog_attn_values(probs, v, heads, kv_heads, d_head, p)
    }
}
use kaspa_consensus_core::palw_base0_ops::silu;

// **The two projections go through the fast kernels, and this is not a fork of the catalog.**
//
// `kernels::a16_matmul_requant_fast` and `..._rescale_fast` are asserted bit-identical to the
// catalog ops they replace — over the projection lengths this engine uses, at both code rails,
// and across the parallel/serial threshold (`kernels`' own tests, plus
// `the_fast_engine_and_the_catalog_agree_token_for_token` below, which compares whole forwards).
// They may be swapped in precisely because ADR-0040 Decision E makes lanes and threads invisible
// to the value; the day that stops being true is the day those tests fail.
use kaspa_consensus_core::palw_base0_a16::{
    PalwA16OpError, a16_attn_scores as catalog_attn_scores, a16_attn_values as catalog_attn_values,
    a16_matmul_requant as catalog_matmul_requant, a16_matmul_rescale as catalog_matmul_rescale,
};

/// Op W1 through whichever implementation this engine was built with.
#[inline]
fn a16_matmul_requant(fast: bool, w: &[i8], x: &[i32], p: &[A16QuantParams]) -> Result<Vec<i32>, PalwA16OpError> {
    if fast { crate::kernels::a16_matmul_requant_fast(w, x, p) } else { catalog_matmul_requant(w, x, p) }
}

/// Op W3 likewise.
#[inline]
fn a16_matmul_rescale(fast: bool, w: &[i8], x: &[i32], p: &[A16QuantParams]) -> Result<Vec<i32>, PalwA16OpError> {
    if fast { crate::kernels::a16_matmul_rescale_fast(w, x, p) } else { catalog_matmul_rescale(w, x, p) }
}

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
    /// Whether the two projections run through `kernels` or through the catalog ops directly.
    /// Both produce the same bits — that is what `A16Engine::new_reference` exists to keep true —
    /// and the catalog path is roughly thirteen times slower, so it is a test instrument rather
    /// than a mode anyone would run.
    fast: bool,
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
            fast: true,
            embed_lift: one("embed_lift.a16", None, "embed_lift")?,
            final_norm: many("final_norm.a16", None, d, "final_norm")?,
            logits_out: one("token_embd.weight.a16", None, "logits_out")?,
            layers,
        })
    }

    /// **Prefill a run of tokens, batched.**
    ///
    /// Decode reads the whole 1.65 GiB weight set to produce one token, so it is bandwidth-bound
    /// and no kernel can fix that — the model has to be read. A prompt does not have that excuse:
    /// every one of its tokens needs the same weight row, so reading it once and using it `batch`
    /// times raises the arithmetic per byte by `batch`.
    ///
    /// Returns the LAST position's logits, which is all a prefill is for: the earlier rows predict
    /// tokens the prompt already contains. The unembedding — 233M multiply-accumulates, 15 % of a
    /// token — is therefore computed once rather than `n` times.
    ///
    /// # Three things this must not change, and how each is held
    ///
    /// * **The KV cache.** Prefill and decode meet in it, so a batched prefill has to leave
    ///   exactly the state a token-at-a-time prefill would have left. Every op here is per row and
    ///   the batched projections are asserted bit-identical to the single-row ones.
    /// * **The sink.** Position 0 rides its own parameters at seven seams (ADR-0050), so it is not
    ///   batched with anything: when the run starts at position 0 that token goes through
    ///   [`Self::forward_token`] alone and the batch starts at position 1. Mixing it in would need
    ///   per-row parameters on three projections, for one row.
    /// * **Attention's history.** Row `i` of a batch attends to everything before the batch plus
    ///   rows `0..=i` of it — never to `i+1`. The full series is built once after all the keys are
    ///   appended and each row reads a PREFIX of it, which is the same bytes the sequential path
    ///   would have concatenated and is `batch` times less copying.
    pub fn forward_prefill(
        &self,
        cache: &mut A16Cache,
        tokens: &[usize],
        start_position: usize,
        batch: usize,
    ) -> Result<Vec<i32>, A16EngineError> {
        if tokens.is_empty() {
            return Err(A16EngineError::OpRefused("an empty prefill"));
        }
        let batch = batch.max(1);
        let mut logits = Vec::new();
        let mut at = 0usize;
        // The sink is never batched.
        if start_position == 0 {
            logits = self.forward_token(cache, tokens[0], 0)?;
            at = 1;
        }
        while at < tokens.len() {
            let end = (at + batch).min(tokens.len());
            let last = end == tokens.len();
            logits = self.forward_batch(cache, &tokens[at..end], start_position + at, last)?;
            at = end;
        }
        Ok(logits)
    }

    /// One batch of non-sink positions. `want_logits` skips the unembedding for every batch but
    /// the last.
    fn forward_batch(
        &self,
        cache: &mut A16Cache,
        tokens: &[usize],
        start_position: usize,
        want_logits: bool,
    ) -> Result<Vec<i32>, A16EngineError> {
        let shape = &self.artifact.shape;
        let d = shape.d_model();
        let kv_dim = shape.kv_dim();
        let batch = tokens.len();
        let refuse =
            |what: &'static str| move |_e: kaspa_consensus_core::palw_base0_a16::PalwA16OpError| A16EngineError::OpRefused(what);
        let tile = |p: A16QuantParams, n: usize| -> Vec<A16QuantParams> { vec![p; n] };

        let rope_rows: Vec<(&[i32], &[i32])> = (0..batch)
            .map(|i| self.artifact.rope.row(start_position + i).ok_or(A16EngineError::PositionOutOfRange))
            .collect::<Result<_, _>>()?;

        // ---- pre: the gather and the lift, per row -----------------------------------------
        let mut h: Vec<Vec<i32>> = Vec::with_capacity(batch);
        for token_id in tokens {
            let embed_row: Vec<i32> = self.artifact.embed[token_id * d..(token_id + 1) * d].iter().map(|c| *c as i32).collect();
            h.push(a16_requant(&embed_row, &tile(self.embed_lift, d)).map_err(refuse("embed_lift"))?);
        }

        for (li, lp) in self.layers.iter().enumerate() {
            let lw = &self.artifact.layers[li];

            // ---- attention ---------------------------------------------------------------
            let mut normed = Vec::with_capacity(batch);
            for row in &h {
                let unit = a16_rms_norm(row, shape.eps_q).map_err(refuse("norm1"))?;
                normed.push(a16_requant(&unit, &lp.attn_norm).map_err(refuse("norm1_req"))?);
            }
            let q = a16_matmul_requant_batch(&lw.wq, &normed, &lp.q).map_err(refuse("q"))?;
            let k = a16_matmul_requant_batch(&lw.wk, &normed, &lp.k).map_err(refuse("k"))?;
            let v = a16_matmul_requant_batch(&lw.wv, &normed, &lp.v).map_err(refuse("v"))?;

            let history_before = cache.keys[li].len();
            let mut q_rot = Vec::with_capacity(batch);
            for (i, (cos_row, sin_row)) in rope_rows.iter().enumerate() {
                let rope_heads = |row: &[i32], heads: usize, what: &'static str| -> Result<Vec<i32>, A16EngineError> {
                    let mut out = Vec::with_capacity(row.len());
                    for hd in 0..heads {
                        let slice = &row[hd * shape.d_head..(hd + 1) * shape.d_head];
                        out.extend(a16_rope(slice, cos_row, sin_row).map_err(|_| A16EngineError::OpRefused(what))?);
                    }
                    Ok(out)
                };
                q_rot.push(rope_heads(&q[i], shape.n_heads, "rope_q")?);
                cache.keys[li].push(rope_heads(&k[i], shape.n_kv_heads, "rope_k")?);
                cache.values[li].push(v[i].clone());
            }

            // The whole series once; row `i` reads the prefix that ends at its own position.
            let history = history_before + batch;
            let mut k_series = Vec::with_capacity(history * kv_dim);
            let mut v_series = Vec::with_capacity(history * kv_dim);
            for j in 0..history {
                k_series.extend_from_slice(&cache.keys[li][j]);
                v_series.extend_from_slice(&cache.values[li][j]);
            }

            let mut attn_rows = Vec::with_capacity(batch);
            for (i, q_row) in q_rot.iter().enumerate() {
                let visible = history_before + i + 1;
                let logits_row = a16_attn_scores(
                    self.fast,
                    q_row,
                    &k_series[..visible * kv_dim],
                    shape.n_heads,
                    shape.n_kv_heads,
                    shape.d_head,
                    &tile(lp.logits, shape.n_heads * visible),
                )
                .map_err(refuse("logits"))?;
                let probs_row = a16_softmax_rows(&logits_row, visible, lp.softmax_up).map_err(refuse("softmax"))?;
                let p15 = a16_requant(&probs_row, &tile(lp.probs, shape.n_heads * visible)).map_err(refuse("p15"))?;
                attn_rows.push(
                    a16_attn_values(
                        self.fast,
                        &p15,
                        &v_series[..visible * kv_dim],
                        shape.n_heads,
                        shape.n_kv_heads,
                        shape.d_head,
                        &tile(lp.values, shape.n_heads * shape.d_head),
                    )
                    .map_err(refuse("values"))?,
                );
            }

            let delta = a16_matmul_requant_batch(&lw.wo, &attn_rows, &lp.wo).map_err(refuse("wo"))?;
            for (i, row) in h.iter_mut().enumerate() {
                let aligned = a16_requant(row, &tile(lp.attn_align, d)).map_err(refuse("attn_align"))?;
                let sum = a16_add_elem(&aligned, &delta[i]).map_err(refuse("attn_add"))?;
                *row = a16_requant(&sum, &tile(lp.attn_residual, d)).map_err(refuse("attn_res"))?;
            }

            // ---- SwiGLU -------------------------------------------------------------------
            let mut normed = Vec::with_capacity(batch);
            for row in &h {
                let unit = a16_rms_norm(row, shape.eps_q).map_err(refuse("norm2"))?;
                normed.push(a16_requant(&unit, &lp.ffn_norm).map_err(refuse("norm2_req"))?);
            }
            let gate_q = a16_matmul_rescale_batch(&lw.w_gate, &normed, &lp.gate).map_err(refuse("gate"))?;
            let up = a16_matmul_requant_batch(&lw.w_up, &normed, &lp.up).map_err(refuse("up"))?;
            let mut gated_rows = Vec::with_capacity(batch);
            for i in 0..batch {
                let silu_q = silu(&gate_q[i]);
                let s16 = a16_requant(&silu_q, &tile(lp.silu_q, shape.d_ff)).map_err(refuse("silu16"))?;
                let prod = a16_mul_elem(&s16, &up[i]).map_err(refuse("mul"))?;
                gated_rows.push(a16_requant(&prod, &tile(lp.gated, shape.d_ff)).map_err(refuse("gated"))?);
            }
            let delta = a16_matmul_requant_batch(&lw.w_down, &gated_rows, &lp.down).map_err(refuse("down"))?;
            for (i, row) in h.iter_mut().enumerate() {
                let aligned = a16_requant(row, &tile(lp.ffn_align, d)).map_err(refuse("ffn_align"))?;
                let sum = a16_add_elem(&aligned, &delta[i]).map_err(refuse("ffn_add"))?;
                *row = a16_requant(&sum, &tile(lp.ffn_residual, d)).map_err(refuse("ffn_res"))?;
            }
        }

        if !want_logits {
            return Ok(Vec::new());
        }
        let last = h.last().expect("a non-empty batch");
        let unit = a16_rms_norm(last, shape.eps_q).map_err(refuse("final_norm"))?;
        let fin = a16_requant(&unit, &self.final_norm).map_err(refuse("final_req"))?;
        a16_matmul_requant(self.fast, &self.artifact.unembed, &fin, &tile(self.logits_out, shape.vocab)).map_err(refuse("logits_out"))
    }

    /// The same engine with the two projections routed through the catalog ops rather than the
    /// fast kernels. Only a test builds one: it is the other side of the differential.
    pub fn new_reference(artifact: &'a Base0ArtifactV1) -> Result<Self, A16EngineError> {
        Ok(Self { fast: false, ..Self::new(artifact)? })
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
            let q = push(&mut nodes, a16_matmul_requant(self.fast, &lw.wq, &normed, &lp.q).map_err(refuse("q"))?);
            let k = push(&mut nodes, a16_matmul_requant(self.fast, &lw.wk, &normed, &lp.k).map_err(refuse("k"))?);
            let v = push(&mut nodes, a16_matmul_requant(self.fast, &lw.wv, &normed, &lp.v).map_err(refuse("v"))?);
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
                    self.fast,
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
                    self.fast,
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
            let delta = push(&mut nodes, a16_matmul_requant(self.fast, &lw.wo, &attn, wo_params).map_err(refuse("wo"))?);
            let align = if sink { lp.attn_align_sink } else { lp.attn_align };
            let aligned = push(&mut nodes, a16_requant(&h, &tile(align, d)).map_err(refuse("attn_align"))?);
            let sum = push(&mut nodes, a16_add_elem(&aligned, &delta).map_err(refuse("attn_add"))?);
            h = push(&mut nodes, a16_requant(&sum, &tile(lp.attn_residual, d)).map_err(refuse("attn_res"))?);

            // ---- SwiGLU (nodes 15..=26) -----------------------------------------------------
            let unit = push(&mut nodes, a16_rms_norm(&h, shape.eps_q).map_err(refuse("norm2"))?);
            let normed = push(&mut nodes, a16_requant(&unit, &lp.ffn_norm).map_err(refuse("norm2_req"))?);
            let gate_q = push(&mut nodes, a16_matmul_rescale(self.fast, &lw.w_gate, &normed, &lp.gate).map_err(refuse("gate"))?);
            let silu_q = push(&mut nodes, silu(&gate_q));
            let s_p = if sink { lp.silu_sink } else { lp.silu_q };
            let s16 = push(&mut nodes, a16_requant(&silu_q, &tile(s_p, shape.d_ff)).map_err(refuse("silu16"))?);
            let up_params = if sink { &lp.up_sink } else { &lp.up };
            let up = push(&mut nodes, a16_matmul_requant(self.fast, &lw.w_up, &normed, up_params).map_err(refuse("up"))?);
            let prod = push(&mut nodes, a16_mul_elem(&s16, &up).map_err(refuse("mul"))?);
            let g_p = if sink { lp.gated_sink } else { lp.gated };
            let gated = push(&mut nodes, a16_requant(&prod, &tile(g_p, shape.d_ff)).map_err(refuse("gated"))?);
            let down_params = if sink { &lp.down_sink } else { &lp.down };
            let delta = push(&mut nodes, a16_matmul_requant(self.fast, &lw.w_down, &gated, down_params).map_err(refuse("down"))?);
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
        let logits = a16_matmul_requant(self.fast, &self.artifact.unembed, &fin, &tile(self.logits_out, shape.vocab))
            .map_err(refuse("logits_out"))?;
        trace.post.push(logits.clone());
        Ok((logits, trace))
    }
}

/// **A well-formed A16 parameter store for a shape** — the tier's analogue of
/// `Base0ArtifactV1::derive_deterministic`, and marked as sharply.
///
/// It is NOT a calibration. A converted class gets its triples from the PTQ pipeline, measured
/// from the checkpoint; these are chosen only so that every row `A16Engine::new` resolves exists
/// and a forward pass produces something other than zeros. An artifact carrying this store is
/// still `is_derived()`, so it cannot be mistaken for a registered class.
///
/// # Why the scales are split by SITE and derived from the fan-in
///
/// The first version of this used one gain everywhere and the engine returned an all-zero logit
/// row: a matmul's accumulator grows with its fan-in and an elementwise requant's does not, so an
/// attenuation big enough for the first is applied ~10 times per layer to the second and the
/// residual stream decays to nothing. That is not a subtle failure — but it is a SILENT one. It
/// passed a fast-versus-reference differential (both agree on zero) and was caught only by asking
/// whether two different tokens produce two different rows.
///
/// So a projection over `fan_in` gets `2^-(8 + bits(fan_in)/2)`, tracking the `√fan_in` growth of
/// a random dot product, and an elementwise site gets unity.
pub fn derived_a16_store(shape: &Base0ShapeV1) -> Vec<(String, Vec<u8>)> {
    let (d, kv, ff) = (shape.d_model(), shape.kv_dim(), shape.d_ff);
    let wire = |m: i64, s: u8, n: usize| -> Vec<u8> {
        A16QuantParams { multiplier: m, shift: s, zero: 0 }
            .to_wire()
            .iter()
            .cycle()
            .take(n * A16QuantParams::WIRE_BYTES)
            .copied()
            .collect()
    };
    let projection = |fan_in: usize, n: usize| -> Vec<u8> {
        let bits = usize::BITS - fan_in.max(1).leading_zeros();
        wire(1, (8 + bits / 2) as u8, n)
    };
    let unity = |n: usize| wire(1, 0, n);

    let mut store: Vec<(String, Vec<u8>)> = vec![
        ("embed_lift.a16".into(), unity(1)),
        ("final_norm.a16".into(), unity(d)),
        ("token_embd.weight.a16".into(), projection(d, 1)),
    ];
    for li in 0..shape.n_layers {
        let b = format!("blk.{li}");
        let rows: [(&str, Vec<u8>); 25] = [
            ("attn_norm.a16", unity(d)),
            ("attn_q.weight.a16", projection(d, d)),
            ("attn_k.weight.a16", projection(d, kv)),
            ("attn_v.weight.a16", projection(d, kv)),
            ("attn_logits.a16", projection(shape.d_head, 1)),
            ("attn_probs.a16", unity(1)),
            ("attn_values.a16", projection(shape.d_head, 1)),
            ("attn_output.weight.a16", projection(d, d)),
            ("attn_output.weight.a16.sink0", projection(d, d)),
            ("attn_align.a16", unity(1)),
            ("attn_align.a16.sink0", unity(1)),
            ("attn_residual.a16", unity(1)),
            ("ffn_norm.a16", unity(d)),
            ("ffn_gate.weight.a16", projection(d, ff)),
            ("ffn_silu.a16", unity(1)),
            ("ffn_silu.a16.sink0", unity(1)),
            ("ffn_up.weight.a16", projection(d, ff)),
            ("ffn_up.weight.a16.sink0", projection(d, ff)),
            ("ffn_gated.a16", unity(1)),
            ("ffn_gated.a16.sink0", unity(1)),
            ("ffn_down.weight.a16", projection(ff, d)),
            ("ffn_down.weight.a16.sink0", projection(ff, d)),
            ("ffn_align.a16", unity(1)),
            ("ffn_align.a16.sink0", unity(1)),
            ("ffn_residual.a16", unity(1)),
        ];
        for (suffix, bytes) in rows {
            store.push((format!("{b}.{suffix}"), bytes));
        }
        // Not a triple: one raw byte, the softmax widening the tier reads directly.
        store.push((format!("{b}.attn_softmax_up"), vec![24u8]));
    }
    store.sort_by(|a, b| a.0.cmp(&b.0));
    store
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::LN_THETA_10000_GEN_Q;

    fn artifact(n_layers: usize, d_head: usize, d_ff: usize) -> Base0ArtifactV1 {
        let shape = Base0ShapeV1 {
            n_layers,
            n_heads: 4,
            n_kv_heads: 2,
            d_head,
            d_ff,
            vocab: 64,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("the derived store is sorted and unique")
    }

    /// **The claim the fast kernels are allowed to exist on.**
    ///
    /// `kernels`' own tests compare one projection at a time. This compares whole forward passes,
    /// where a divergence would also have to survive the residual stream, the KV cache and the
    /// attention arms — which is the only place a kernel bug that cancels inside one matmul would
    /// still show up. Every logit of every token, exactly equal.
    #[test]
    fn the_fast_engine_and_the_catalog_agree_token_for_token() {
        // Widths on both sides of the kernel's 16-element vector block and its 64-channel
        // parallel threshold, so neither path is exercised by only one of its branches.
        for (layers, d_head, d_ff) in [(1usize, 4usize, 8usize), (2, 8, 32), (2, 32, 160)] {
            let artifact = artifact(layers, d_head, d_ff);
            let fast = A16Engine::new(&artifact).expect("the store resolves");
            let reference = A16Engine::new_reference(&artifact).expect("the store resolves");
            let (mut fast_cache, mut reference_cache) = (A16Cache::new(layers), A16Cache::new(layers));
            for position in 0..12usize {
                let token = (position * 7 + 3) % artifact.shape.vocab;
                let a = fast.forward_token(&mut fast_cache, token, position).expect("the token decodes");
                let b = reference.forward_token(&mut reference_cache, token, position).expect("the token decodes");
                assert_eq!(a, b, "layers={layers} d_head={d_head} d_ff={d_ff} position={position}");
            }
        }
    }

    /// **A batched prefill must leave exactly what a sequential one leaves.**
    ///
    /// Not "the same logits" — the same KV CACHE, because the next decode token reads it. A batch
    /// that got attention's visibility wrong by one would still produce a plausible last row and
    /// then poison every token after it.
    ///
    /// Batch sizes straddle the run length so that the last chunk is ragged, and the run starts
    /// both at 0 (where the sink is peeled off and processed alone) and mid-context.
    #[test]
    fn a_batched_prefill_leaves_the_same_state_as_a_sequential_one() {
        for (layers, d_head, d_ff) in [(1usize, 4usize, 8usize), (2, 8, 32)] {
            let artifact = artifact(layers, d_head, d_ff);
            let engine = A16Engine::new(&artifact).expect("the store resolves");
            let tokens: Vec<usize> = (0..11).map(|i| (i * 5 + 1) % artifact.shape.vocab).collect();

            for batch in [1usize, 2, 3, 4, 16] {
                for start in [0usize, 1, 5] {
                    let mut sequential = A16Cache::new(layers);
                    let mut expected = Vec::new();
                    // The sequential run needs the same history in front of it when `start` is
                    // not zero, so the leading positions are filled the same way for both.
                    for position in 0..start {
                        let _ = engine.forward_token(&mut sequential, position % artifact.shape.vocab, position).expect("decodes");
                    }
                    let mut batched = A16Cache::new(layers);
                    for position in 0..start {
                        let _ = engine.forward_token(&mut batched, position % artifact.shape.vocab, position).expect("decodes");
                    }
                    for (i, token) in tokens.iter().enumerate() {
                        expected = engine.forward_token(&mut sequential, *token, start + i).expect("decodes");
                    }
                    let got = engine.forward_prefill(&mut batched, &tokens, start, batch).expect("prefills");

                    assert_eq!(got, expected, "logits: layers={layers} batch={batch} start={start}");
                    assert_eq!(batched.len(), sequential.len(), "cache depth: batch={batch} start={start}");
                    for li in 0..layers {
                        assert_eq!(batched.keys[li], sequential.keys[li], "keys layer {li}: batch={batch} start={start}");
                        assert_eq!(batched.values[li], sequential.values[li], "values layer {li}: batch={batch} start={start}");
                    }
                }
            }
        }
    }

    /// And the cache a batched prefill leaves must carry a decode that continues from it — the
    /// property the test above is a proxy for, checked directly.
    #[test]
    fn decoding_continues_identically_from_a_batched_prefill() {
        let artifact = artifact(2, 8, 32);
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let prompt: Vec<usize> = vec![3, 9, 17, 4, 11, 2, 8];

        let mut sequential = A16Cache::new(2);
        let mut logits_a = Vec::new();
        for (i, t) in prompt.iter().enumerate() {
            logits_a = engine.forward_token(&mut sequential, *t, i).expect("decodes");
        }
        let mut batched = A16Cache::new(2);
        let mut logits_b = engine.forward_prefill(&mut batched, &prompt, 0, 4).expect("prefills");
        assert_eq!(logits_a, logits_b);

        for step in 0..6 {
            let next_a = crate::engine::argmax_lowest(&logits_a);
            let next_b = crate::engine::argmax_lowest(&logits_b);
            assert_eq!(next_a, next_b, "step {step}");
            logits_a = engine.forward_token(&mut sequential, next_a, prompt.len() + step).expect("decodes");
            logits_b = engine.forward_token(&mut batched, next_b, prompt.len() + step).expect("decodes");
            assert_eq!(logits_a, logits_b, "step {step}");
        }
    }

    /// A forward pass that returns the same row for every token would satisfy the differential
    /// above while computing nothing, so the fixture is checked for being non-degenerate.
    #[test]
    fn the_fixture_actually_computes_something() {
        let artifact = artifact(2, 8, 32);
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let mut cache = A16Cache::new(2);
        let first = engine.forward_token(&mut cache, 5, 0).expect("decodes");
        let second = engine.forward_token(&mut cache, 9, 1).expect("decodes");
        assert_ne!(first, second, "a different token at a different position must move the logits");
        assert!(first.iter().any(|v| *v != 0), "an all-zero logit row is a dead pass");
    }
}
