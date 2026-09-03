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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
    /// **This cache's bytes for one state chunk, encoded the way the MAP says — or nothing.**
    ///
    /// The A16 analogue of `KvCache::state_chunk_bytes`, and deliberately not a copy of it. That
    /// one reinterprets each element as a byte, which is exact for a `Vec<i8>` cache and silent
    /// truncation for this one; its length guard does not catch the difference, because for this
    /// class the element COUNT and the map's declared byte count are the same number.
    ///
    /// So the width is read from the entry rather than assumed, and a row that does not fit the
    /// declared width is refused instead of narrowed:
    ///
    /// * `row_bytes == row.len()` — one byte per element. Encoded only if every value is an `i8`;
    ///   otherwise `None`, because a checkpoint that opens to a state the producer never held is
    ///   worse than a missing one, and the producer has signed for it.
    /// * `row_bytes == 4 × row.len()` — little-endian `i32`, which is what this cache holds.
    /// * anything else — `None`. A map that describes neither is a map for a different class.
    ///
    /// Written this way because the class's map is currently the one-byte one and its state does
    /// not fit (see `docs/palw-fp-on-registered-classes.md`): whichever way that is resolved —
    /// narrowing the cache, or registering a class with a four-byte map — this function is already
    /// correct for it, and refuses in the meantime rather than committing a lie.
    pub fn state_chunk_bytes_v1(&self, entry: &kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkEntryV1) -> Option<Vec<u8>> {
        use kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1;
        let side = match entry.kind {
            PalwStateChunkKindV1::Key => &self.keys,
            PalwStateChunkKindV1::Value => &self.values,
        };
        let layer = side.get(entry.attn_layer as usize)?;
        let mut out = Vec::with_capacity((entry.position_count as usize) * (entry.row_bytes as usize));
        for p in entry.position_start..entry.position_start + entry.position_count {
            let row = layer.get(p as usize)?;
            let declared = entry.row_bytes as usize;
            if declared == row.len() {
                for value in row {
                    out.push(i8::try_from(*value).ok()? as u8);
                }
            } else if declared == row.len().checked_mul(4)? {
                for value in row {
                    out.extend_from_slice(&value.to_le_bytes());
                }
            } else {
                return None;
            }
        }
        Some(out)
    }

    /// **The inverse of [`Self::state_chunk_bytes_v1`]: a cache rebuilt from committed chunks.**
    ///
    /// The restore half of ADR-0077 Decision 10 for this tier. Without it the dense class could
    /// commit a checkpoint leg and nothing could resume from one, so every dispute and every
    /// interval opening ran genesis-anchored — which is the cost the checkpoint leg exists to
    /// remove.
    ///
    /// Both widths the encoder writes are read back, decided by the ENTRY rather than guessed:
    /// `row_bytes == elements` is the one-byte map (each byte an `i8`), `row_bytes == 4 ×
    /// elements` is this cache's own `i32`. `row_bytes` is a whole multiple of neither for a map
    /// that describes another class, and that is a refusal.
    ///
    /// Every refusal is a refusal to replay, never a partial cache — the rule
    /// `KvCache::from_state_chunks` states: a cache assembled from material that does not cover
    /// the state replays against zeros, and zeros are indistinguishable from computed rows once
    /// they are in a commitment.
    pub fn from_state_chunks_v1(
        layers: usize,
        row_elements: usize,
        geometry: &kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkGeometryV1,
        chunks: &[Vec<u8>],
    ) -> Result<Self, A16EngineError> {
        use kaspa_consensus_core::palw_state_chunk_map::{
            PalwStateChunkKindV1, integer_kv_state_chunk_entry_v1, integer_kv_state_row_v1,
        };
        if chunks.len() as u64 != geometry.chunk_count() || row_elements == 0 {
            return Err(A16EngineError::OpRefused("the served chunks are not the map's own count"));
        }
        let positions = geometry.positions as usize;
        let mut cache = Self::new(layers);
        for side in [&mut cache.keys, &mut cache.values] {
            for layer in side.iter_mut() {
                layer.resize(positions, Vec::new());
            }
        }
        for (index, bytes) in chunks.iter().enumerate() {
            let entry = integer_kv_state_chunk_entry_v1(geometry, index as u64)
                .ok_or(A16EngineError::OpRefused("the map has no entry for a chunk it counted"))?;
            let width = entry.row_bytes as usize;
            let per_element = if width == row_elements {
                1
            } else if width == row_elements.checked_mul(4).ok_or(A16EngineError::OpRefused("the map's row width overflows"))? {
                4
            } else {
                return Err(A16EngineError::OpRefused("the map describes a row this cache does not hold"));
            };
            for p in entry.position_start..entry.position_start + entry.position_count {
                let row =
                    integer_kv_state_row_v1(&entry, bytes, p).ok_or(A16EngineError::OpRefused("a chunk is not its own length"))?;
                let values: Vec<i32> = if per_element == 1 {
                    row.iter().map(|b| *b as i8 as i32).collect()
                } else {
                    row.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
                };
                let side = match entry.kind {
                    PalwStateChunkKindV1::Key => &mut cache.keys,
                    PalwStateChunkKindV1::Value => &mut cache.values,
                };
                let layer = side
                    .get_mut(entry.attn_layer as usize)
                    .ok_or(A16EngineError::OpRefused("the map names a layer this cache lacks"))?;
                layer[p as usize] = values;
            }
        }
        // Every attention layer the map named must now be full at the declared width. A layer it
        // never named keeps its empty rows, and replaying over those is the zero-state failure.
        for side in [&cache.keys, &cache.values] {
            for layer in geometry.attn_layers.iter() {
                let rows = side.get(*layer as usize).ok_or(A16EngineError::OpRefused("the map names a layer this cache lacks"))?;
                if rows.len() != positions || rows.iter().any(|r| r.len() != row_elements) {
                    return Err(A16EngineError::OpRefused("the served chunks do not cover the state they declare"));
                }
            }
        }
        Ok(cache)
    }

    /// The key rows this cache holds, for tests that need to measure the STATE rather than reason
    /// about its type — `a16_kv_state_does_not_fit_the_one_byte_map_its_class_declares` is the
    /// caller, and what it measures decides whether a checkpoint map is sound for this family.
    #[cfg(test)]
    pub(crate) fn key_rows_for_test(&self) -> Vec<Vec<i32>> {
        self.keys.iter().flatten().cloned().collect()
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

    /// **The compiled GRAPH-V2 program: one position's forward with every node's committed row
    /// recorded, in the v2 shape profile's numbering.** The full-job replay adjudicates each of
    /// these rows through the court's own dispatch and demands bit equality.
    ///
    /// **It is a v2 reference and nothing wider.** The row count is written into this function —
    /// twenty-seven nodes a layer, the attention site spelled as `ATTN_SCORES`, the row `SoftMax`,
    /// the probability requantization and `ATTN_VALUES` — so it describes exactly the graph
    /// `qwen25_a16_profile_v2` declares. ADR-0082's graph v5 replaces those four nodes with one
    /// fused node and declares twenty-four, and this route MUST NOT learn the fusion: the whole
    /// point of ADR-0067 Decision 2 is that [`Self::plan_from_profile`] is the single authority on
    /// what a declaration executes, and a second hand-written program that also knew the fused
    /// site would be a second authority to keep in step. A v5 class is served by
    /// [`Self::forward_token_planned`]; a caller that reaches this route with a v5 profile is
    /// refused by name, by the Decision-F probe in `a16_execute_for_attempt_v1` ("per-layer
    /// declares 24 against 27 recorded") and pinned by
    /// `the_plan_less_route_is_the_v2_reference_and_refuses_a_fused_row`.
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
            // **In the DECLARED order: up before the silu chain** (ADR-0067). The step-leg
            // capture places rows at profile coordinates BY POSITION (`a16_captured_rows_v1`
            // reorders nothing), so a trace emitted in any other order commits the silu row at
            // the slot the class declares as the up-projection — and a court bisecting there
            // recomputes the declaration, convicting an honest producer. Caught by the
            // interpreter differential before any claim of this class reached a chain; the
            // dataflow is unchanged, only the emission order conforms to the declaration.
            let up_params = if sink { &lp.up_sink } else { &lp.up };
            let up = push(&mut nodes, a16_matmul_requant(self.fast, &lw.w_up, &normed, up_params).map_err(refuse("up"))?);
            let silu_q = push(&mut nodes, silu(&gate_q));
            let s_p = if sink { lp.silu_sink } else { lp.silu_q };
            let s16 = push(&mut nodes, a16_requant(&silu_q, &tile(s_p, shape.d_ff)).map_err(refuse("silu16"))?);
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

// =============================================================================================
// ADR-0067: execution FROM the registered profile
// =============================================================================================
//
// Everything above executes a HARDCODED op sequence that the class's profile merely describes —
// which is why ADR-0049 Decision F needs a correspondence check at all: two authorities, one
// arithmetic. This half inverts the authority. A plan is compiled from the `PalwShapeProfileV3`
// the CHAIN registered: each declared node is bound to a kernel this build serves and to the
// named operand in the artifact's store, and execution walks the declaration. An engine built
// from the profile cannot perform a narrowing the profile does not name — Decision F stops being
// a check and becomes the constructor — and a class whose graph this build cannot serve is
// refused AT PLAN TIME with the node named, which is ADR-0067 Decision 3's kernel boundary
// surfacing exactly where it is crossed.
//
// The dispatch below is deliberately a closed vocabulary: (op kind, kernel semantics id, operand
// name shape) triples this build's kernels serve. It is NOT a general dataflow VM — width rules,
// input arities and the position-0 sink convention are the A16 family's kernel semantics, and a
// profile is served only where its declaration lands inside them. Anything else is a named
// refusal, because "almost servable" executed approximately is how an honest producer gets
// convicted.

use kaspa_consensus_core::palw_step::{
    PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN, PALW_STEP_INPUT_SENTINEL_MIN, PalwShapeProfileV3,
    PalwStepLaneV1, PalwStepNodeV1, PalwStepOutLenV1, kernel_semantics_id_v1,
};
use kaspa_consensus_core::palw_step_refute::{
    KDESC_A16_ADD_ELEM, KDESC_A16_ATTN_FUSED, KDESC_A16_ATTN_SCORES, KDESC_A16_ATTN_VALUES, KDESC_A16_EMBED,
    KDESC_A16_MATMUL_REQUANT, KDESC_A16_MATMUL_RESCALE, KDESC_A16_MUL_ELEM, KDESC_A16_REQUANTIZE, KDESC_A16_RMS_NORM,
    KDESC_A16_ROPE, KDESC_A16_SOFTMAX, KDESC_Q36_SILU, palw_attn_fused_tensors_v1,
};

/// Why a profile could not be compiled to a plan. Every variant names the boundary it found —
/// a plan error is the kernel-set boundary of ADR-0067 Decision 3 speaking, so it must say
/// WHICH declaration this build cannot serve, not merely that one exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum A16PlanErrorV1 {
    /// The profile's declared geometry is not this artifact's. The pairing is wrong at the root;
    /// no node-level answer would mean anything.
    GeometryMismatch { what: &'static str, profile: u64, artifact: u64 },
    /// The profile's lane is not the integer lane this family commits.
    NotAnIntegerLane,
    /// A declared node is outside this build's served vocabulary. `table` is "pre" / "layer" /
    /// "post"; the reason names the missing piece (kernel, operand, width, arity or dtype).
    UnservedNode { table: &'static str, index: usize, reason: String },
    /// **ADR-0067 SA-1: the declaration would materialise more than the interpreted path is
    /// allowed to hold.** A chain-registered profile is a stranger's program, and one token's
    /// walk commits one row per declared node — so the row widths and the node counts in a
    /// registration are an allocation the registrant chose. Refused at PLAN time, before a byte
    /// is allocated, because a ceiling that notices after the allocation is not a ceiling.
    OverMemoryCeiling { bytes: u64, ceiling: u64 },
}

/// **The interpreted path's memory ceiling (ADR-0067 SA-1), in bytes of one token's committed
/// trace.**
///
/// What it bounds is exactly what a registration controls: the number of declared nodes and the
/// width of each one's committed row, times the layers the layer table is walked for. It does not
/// bound the artifact or the KV cache, because those are sized by the WEIGHTS this operator chose
/// to hold, not by the stranger who registered the graph.
///
/// **64 MiB, and it is a MEASURED number rather than a chosen one.**
/// `the_interpreter_ceiling_is_derived_from_what_this_build_actually_serves` runs
/// [`interpreted_trace_bytes_v1`] over every class this build ships and fails if the constant
/// drifts away from them. What it measures today:
///
/// | class          | context | one token's committed trace |
/// |----------------|---------|-----------------------------|
/// | BASE-0         | 12      | 182,272 B                   |
/// | QWEN25-A16     | 16      | 9,384,448 B                 |
/// | QWEN36         | 8       | 17,638,208 B                |
/// | QWEN36, stress | 4,096   | 49,034,048 B                |
///
/// So the ceiling is 3.8x the largest class this build serves at its registered context, and still
/// above that same graph stretched to a 4,096-position context no admission gate accepts. The
/// first shipped value was 1 GiB, which was 60x the largest measured class and 40,000x the largest
/// gate-accepted profile in the adversarial corpus — a number nothing had produced and nothing
/// could reach, i.e. a bound with no evidence behind it. The margin is now stated and tested in
/// both directions: raise a class past it and the derivation test says so, and set the ceiling
/// somewhere arbitrary and it says that too.
///
/// **What actually refuses a hostile profile first, said plainly**, because SA-1 should not be read
/// as more than it is: on the shipped admission gate the leaf bound and the per-node width checks
/// throw out every oversized shape the adversarial corpus can generate — 372 of 400, with the
/// largest gate-ACCEPTED profile costing 26,624 bytes. This ceiling is the second line, and it is
/// the line that survives a gate whose node counts or row widths are ever loosened. It is checked
/// after the scalar geometry comparisons (which are free) and before the first allocation, so a
/// profile that is merely the wrong shape is reported as the wrong shape.
///
/// **And this is a NODE CAPACITY limit, not a bound the chain's admission gate implies — which
/// matters because classes are permissionless (ADR-0054) and the band above is measured over the
/// three classes THIS BUILD compiles.** The consensus shape caps do not bound a declared row's
/// width at all: `validate_shape` asks for a non-zero width and a tile inside
/// `[PALW_STEP_MIN_TILE_LEN, PALW_STEP_MAX_TILE_LEN]`, so at the widest admitted tile a single
/// extra node of 20 M elements costs 306 leaves per position — nowhere near the leaf cap — and
/// 80 MB of committed trace, which is over this ceiling.
/// `the_consensus_shape_caps_admit_more_than_this_build_will_materialise` constructs exactly that
/// profile and drives it, so the gap is a demonstration rather than a hope. Deriving the ceiling
/// from the caps instead would put it at `PALW_STEP_MAX_LEAVES × PALW_STEP_MAX_TILE_LEN × 4` — a
/// terabyte, i.e. back to a number nothing measured chose and nothing can reach.
///
/// So the honest statement, and the one the REFUSAL carries to whoever reads a node's log
/// (`from_registered_profile` in both backends): a class between this ceiling and what the chain
/// admits is registered, valid, and adjudicable — this node simply will not materialise it, a node
/// built with a larger ceiling will, and the divergence is node-local servability (who produces and
/// who judges), never block validity. Raising the constant is an operator's call about memory; it
/// is not a consensus change and it never was.
pub const PALW_INTERPRETER_TRACE_BYTES_CEILING_V1: u64 = 64 << 20;

/// How many bytes one token's committed trace costs under `profile`, counted the way
/// `forward_token_planned` actually spends them: one `i32` row per declared node, the layer table
/// once per layer, and `max_kv_len` standing in for a kv-scaled row's longest form.
///
/// Saturating throughout: this is called ON adversarial input, so an overflow that wrapped to a
/// small number would be the exact failure the ceiling exists to prevent.
pub fn interpreted_trace_bytes_v1(profile: &PalwShapeProfileV3, max_kv_len: u64) -> u64 {
    let row_elems = |node: &PalwStepNodeV1| -> u64 {
        match node.out_len {
            PalwStepOutLenV1::Fixed { elements } => elements as u64,
            PalwStepOutLenV1::KvScaled { multiplier } => (multiplier as u64).saturating_mul(max_kv_len),
        }
    };
    let table = |nodes: &[PalwStepNodeV1]| -> u64 { nodes.iter().fold(0u64, |acc, n| acc.saturating_add(row_elems(n))) };
    let elems = table(&profile.pre_nodes)
        .saturating_add(table(&profile.attn_nodes).saturating_mul(profile.layer_count as u64))
        .saturating_add(table(&profile.post_nodes));
    elems.saturating_mul(std::mem::size_of::<i32>() as u64)
}

/// A node's data input, resolved at plan time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanInput {
    /// An earlier node's committed row in the same table.
    Row(usize),
    /// The table's input stream: the pre output for the layer table (and the running residual
    /// between layers), the last layer's output for the post table.
    LayerIn,
    /// The rotated-key series, position-major, full `kv_dim` rows — the court's canonical
    /// concatenation.
    CachedK,
    /// The value series, likewise.
    CachedV,
}

/// The per-layer A16 requant table a planned node reads. Slots, not names, because the names
/// were resolved at plan time — execution must not re-parse strings per token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReqSlot {
    EmbedLift,
    AttnNorm,
    Probs,
    AttnAlign,
    AttnResidual,
    FfnNorm,
    SiluQ,
    Gated,
    FfnAlign,
    FfnResidual,
    FinalNorm,
}

/// The weight-bearing matmul sites. Each carries both the tensor and the requant/rescale table
/// the engine's parameter store associates with that site (with the position-0 sink variant
/// where the store declares one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatSlot {
    Q,
    K,
    V,
    Wo,
    Gate,
    Up,
    Down,
    Head,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanOp {
    EmbedGather,
    RmsNorm,
    Requant(ReqSlot),
    MatMulRequant(MatSlot),
    MatMulRescale(MatSlot),
    Rope { kv: bool },
    AttnScores,
    Softmax,
    AttnValues,
    /// **ADR-0082 Decision 1: the whole attention site as one op.** Scores, the row softmax, the
    /// probability requantization and the value reduction, computed here and committed as ONE
    /// row: the output. The three context-wide rows the four separate ops commit are internal —
    /// never a plan row, never a leaf, never carried.
    AttnFused,
    AddElem,
    MulElem,
    Silu,
}

#[derive(Clone, Debug)]
struct PlanNode {
    op: PlanOp,
    inputs: Vec<PlanInput>,
    role: kaspa_consensus_core::palw_step::PalwStepNodeRoleV1,
}

/// A compiled execution plan: the registered profile, validated against this build's kernel
/// vocabulary and this artifact's operand store, ready to walk. Holding one is the proof that
/// every declared node is servable — construction is the admission check.
#[derive(Clone, Debug)]
pub struct A16ProfilePlanV1 {
    pre: Vec<PlanNode>,
    layer: Vec<PlanNode>,
    post: Vec<PlanNode>,
    layer_count: usize,
}

impl<'a> A16Engine<'a> {
    /// Compile the registered profile into a plan this engine can walk.
    ///
    /// Refusals here are the ADR-0067 kernel boundary: the class declared arithmetic this build
    /// does not serve, and the error names the node. A `Ok` is a structural Decision-F proof —
    /// execution will emit exactly one row per declared node, in the declared order, from the
    /// declared operands, because the declaration is the program.
    pub fn plan_from_profile(&self, profile: &PalwShapeProfileV3) -> Result<A16ProfilePlanV1, A16PlanErrorV1> {
        self.plan_from_profile_within(profile, PALW_INTERPRETER_TRACE_BYTES_CEILING_V1)
    }

    /// [`Self::plan_from_profile`] under a caller-chosen ceiling — ADR-0067 SA-1.
    ///
    /// The ceiling is a parameter so it can be PROVEN to bind: a test that only ever runs at the
    /// shipped value can show that nothing crashed, which is not the same claim. Node code takes
    /// the default; the fuzz gate and the ceiling's own test drive it down until it refuses, which
    /// is the evidence the amendment asks for.
    pub fn plan_from_profile_within(
        &self,
        profile: &PalwShapeProfileV3,
        ceiling_bytes: u64,
    ) -> Result<A16ProfilePlanV1, A16PlanErrorV1> {
        let shape = &self.artifact.shape;
        let check = |what: &'static str, p: u64, a: u64| -> Result<(), A16PlanErrorV1> {
            if p != a { Err(A16PlanErrorV1::GeometryMismatch { what, profile: p, artifact: a }) } else { Ok(()) }
        };
        if profile.lane != PalwStepLaneV1::Int32 {
            return Err(A16PlanErrorV1::NotAnIntegerLane);
        }
        check("layer_count", profile.layer_count as u64, shape.n_layers as u64)?;
        check("hidden_dim", profile.hidden_dim as u64, shape.d_model() as u64)?;
        check("ffn_dim", profile.ffn_dim as u64, shape.d_ff as u64)?;
        check("attn_heads", profile.attn_heads as u64, shape.n_heads as u64)?;
        check("attn_kv_heads", profile.attn_kv_heads as u64, shape.n_kv_heads as u64)?;
        check("attn_head_dim", profile.attn_head_dim as u64, shape.d_head as u64)?;
        check("vocab_size", profile.vocab_size as u64, shape.vocab as u64)?;
        // The eps is an artifact field AND a profile field, and it moves every activation.
        check("rms_eps_q", profile.base0_rms_eps_q as u64, shape.eps_q as u64)?;

        // **The memory ceiling: after the free comparisons above, before the first allocation
        // below** (ADR-0067 SA-1). Ahead of `plan_table`, which is where bytes are first spent, so
        // the refusal still lands before the plan materialises anything — and behind the scalar
        // geometry checks, so a profile that is merely the wrong shape for this artifact is
        // reported as the wrong shape instead of as an oversized one. The earlier ordering put this
        // first and made `OverMemoryCeiling` the answer to a question the caller had not asked.
        // `max_position` is the artifact's own bound on a kv-scaled row.
        let bytes = interpreted_trace_bytes_v1(profile, shape.max_position as u64);
        if bytes > ceiling_bytes {
            return Err(A16PlanErrorV1::OverMemoryCeiling { bytes, ceiling: ceiling_bytes });
        }

        // Each table's terminal width is the next table's input, and both must be the hidden
        // stream: the residual is what flows pre -> layer -> layer -> post. A declaration whose
        // table ends on something else is refused HERE rather than mis-executed later.
        let terminal = |nodes: &[PalwStepNodeV1], table: &'static str| -> Result<u32, A16PlanErrorV1> {
            match nodes.last().map(|n| n.out_len) {
                Some(PalwStepOutLenV1::Fixed { elements }) => Ok(elements),
                _ => Err(A16PlanErrorV1::UnservedNode {
                    table,
                    index: nodes.len().saturating_sub(1),
                    reason: "the table's last node must produce a fixed-width row — it is the stream the next table reads".to_string(),
                }),
            }
        };
        let hidden = shape.d_model() as u32;
        let pre = plan_table(&profile.pre_nodes, "pre", shape, None)?;
        let pre_out = terminal(&profile.pre_nodes, "pre")?;
        if pre_out != hidden {
            return Err(A16PlanErrorV1::UnservedNode {
                table: "pre",
                index: profile.pre_nodes.len().saturating_sub(1),
                reason: format!("the pre table ends at width {pre_out}, and a layer reads the hidden stream ({hidden})"),
            });
        }
        let layer = plan_table(&profile.attn_nodes, "layer", shape, Some(hidden))?;
        let layer_out = terminal(&profile.attn_nodes, "layer")?;
        if layer_out != hidden {
            return Err(A16PlanErrorV1::UnservedNode {
                table: "layer",
                index: profile.attn_nodes.len().saturating_sub(1),
                reason: format!(
                    "the layer table ends at width {layer_out}, and the residual it feeds is the hidden stream ({hidden})"
                ),
            });
        }
        let post = plan_table(&profile.post_nodes, "post", shape, Some(hidden))?;
        Ok(A16ProfilePlanV1 { pre, layer, post, layer_count: shape.n_layers })
    }

    /// One position's forward, EXECUTED FROM THE PLAN: one committed row per declared node, in
    /// the declared order. This is the route that serves EVERY declared graph, the fused
    /// attention site of ADR-0082 included (`PlanOp::AttnFused`).
    ///
    /// Bit-compatible with [`Self::forward_token_traced`] for a plan compiled from a GRAPH-V2
    /// profile — pinned by `the_interpreter_and_the_compiled_engine_agree_bit_for_bit` below,
    /// which is stated over `qwen25_a16_profile_v2` and claims nothing wider. There is no such
    /// correspondence for graph v5 and there is not meant to be one: the traced route is the
    /// twenty-seven-row v2 program, a v5 layer declares twenty-four nodes, and the fused arm's
    /// equality is proven against the ARITHMETIC instead — `a16_attn_fused_reference_v1` and
    /// `a16_attn_fused_via_tiles_v1`, in `the_fused_arm_is_the_reference_composition`.
    ///
    /// Faithful to the DECLARATION wherever a declaration and a hand-written program could
    /// differ, which is the point of ADR-0067: the court adjudicates what was declared, so an
    /// interpreter must execute exactly that.
    pub fn forward_token_planned(
        &self,
        plan: &A16ProfilePlanV1,
        cache: &mut A16Cache,
        token_id: usize,
        position: usize,
    ) -> Result<(Vec<i32>, A16TraceV1), A16EngineError> {
        if plan.layer_count != self.artifact.shape.n_layers {
            return Err(A16EngineError::MalformedParams("plan/artifact layer count"));
        }
        let (cos_row, sin_row) = self.artifact.rope.row(position).ok_or(A16EngineError::PositionOutOfRange)?;
        let sink = position == 0;
        let mut trace = A16TraceV1::default();

        let mut h: Vec<i32> = Vec::new();
        // ---- pre --------------------------------------------------------------------------
        let rows = self.walk_table(&plan.pre, None, token_id, sink, cos_row, sin_row, None)?;
        if let Some(last) = rows.last() {
            h = last.clone();
        }
        trace.pre = rows;

        // ---- layers -----------------------------------------------------------------------
        for li in 0..plan.layer_count {
            let rows = self.walk_table(&plan.layer, Some(&h), token_id, sink, cos_row, sin_row, Some((li, cache)))?;
            h = rows.last().cloned().ok_or(A16EngineError::MalformedParams("an empty layer table"))?;
            trace.attn.push(rows);
        }

        // ---- post -------------------------------------------------------------------------
        let rows = self.walk_table(&plan.post, Some(&h), token_id, sink, cos_row, sin_row, None)?;
        let logits = rows.last().cloned().ok_or(A16EngineError::MalformedParams("an empty post table"))?;
        trace.post = rows;
        Ok((logits, trace))
    }

    /// Walk one table of the plan. `layer` is `Some((index, cache))` for the layer table — the
    /// only table with cache reads and writes — and `layer_in` is the table's input stream.
    #[allow(clippy::too_many_arguments)]
    fn walk_table(
        &self,
        table: &[PlanNode],
        layer_in: Option<&Vec<i32>>,
        token_id: usize,
        sink: bool,
        cos_row: &[i32],
        sin_row: &[i32],
        mut layer: Option<(usize, &mut A16Cache)>,
    ) -> Result<Vec<Vec<i32>>, A16EngineError> {
        let shape = &self.artifact.shape;
        let d = shape.d_model();
        let kv_dim = shape.kv_dim();
        let refuse =
            |what: &'static str| move |_e: kaspa_consensus_core::palw_base0_a16::PalwA16OpError| A16EngineError::OpRefused(what);
        let tile = |p: A16QuantParams, n: usize| -> Vec<A16QuantParams> { vec![p; n] };

        let mut rows: Vec<Vec<i32>> = Vec::with_capacity(table.len());
        for node in table {
            // Resolve the declared inputs against what this walk holds.
            let resolve = |input: &PlanInput, rows: &Vec<Vec<i32>>| -> Result<Vec<i32>, A16EngineError> {
                match input {
                    PlanInput::Row(i) => rows.get(*i).cloned().ok_or(A16EngineError::MalformedParams("a forward input ref")),
                    PlanInput::LayerIn => layer_in.cloned().ok_or(A16EngineError::MalformedParams("layer input outside a layer")),
                    // The series are built at USE, so a read after this position's cache write
                    // sees the same history the compiled engine hands the kernels.
                    PlanInput::CachedK | PlanInput::CachedV => {
                        let Some((li, cache)) = layer.as_ref() else {
                            return Err(A16EngineError::MalformedParams("a cache read outside a layer"));
                        };
                        let series = match input {
                            PlanInput::CachedK => &cache.keys[*li],
                            _ => &cache.values[*li],
                        };
                        let mut out = Vec::with_capacity(series.len() * kv_dim);
                        for row in series {
                            out.extend_from_slice(row);
                        }
                        Ok(out)
                    }
                }
            };

            let lp = |li: usize| -> &LayerParams { &self.layers[li] };
            let out: Vec<i32> = match node.op {
                PlanOp::EmbedGather => {
                    if token_id >= self.artifact.shape.vocab {
                        return Err(A16EngineError::OpRefused("a token outside the vocabulary"));
                    }
                    self.artifact.embed[token_id * d..(token_id + 1) * d].iter().map(|c| *c as i32).collect()
                }
                PlanOp::RmsNorm => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    a16_rms_norm(&x, shape.eps_q).map_err(refuse("rms_norm"))?
                }
                PlanOp::Requant(slot) => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let li = layer.as_ref().map(|(li, _)| *li).unwrap_or(0);
                    let params: Vec<A16QuantParams> = match slot {
                        ReqSlot::EmbedLift => tile(self.embed_lift, d),
                        ReqSlot::AttnNorm => lp(li).attn_norm.clone(),
                        ReqSlot::Probs => {
                            let history = x.len() / shape.n_heads.max(1);
                            tile(lp(li).probs, shape.n_heads * history)
                        }
                        ReqSlot::AttnAlign => tile(if sink { lp(li).attn_align_sink } else { lp(li).attn_align }, d),
                        ReqSlot::AttnResidual => tile(lp(li).attn_residual, d),
                        ReqSlot::FfnNorm => lp(li).ffn_norm.clone(),
                        ReqSlot::SiluQ => tile(if sink { lp(li).silu_sink } else { lp(li).silu_q }, shape.d_ff),
                        ReqSlot::Gated => tile(if sink { lp(li).gated_sink } else { lp(li).gated }, shape.d_ff),
                        ReqSlot::FfnAlign => tile(if sink { lp(li).ffn_align_sink } else { lp(li).ffn_align }, d),
                        ReqSlot::FfnResidual => tile(lp(li).ffn_residual, d),
                        ReqSlot::FinalNorm => self.final_norm.clone(),
                    };
                    a16_requant(&x, &params).map_err(refuse("requant"))?
                }
                PlanOp::MatMulRequant(slot) => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let li = layer.as_ref().map(|(li, _)| *li).unwrap_or(0);
                    let (w, params): (&[i8], Vec<A16QuantParams>) = match slot {
                        MatSlot::Q => (&self.artifact.layers[li].wq, lp(li).q.clone()),
                        MatSlot::K => (&self.artifact.layers[li].wk, lp(li).k.clone()),
                        MatSlot::V => (&self.artifact.layers[li].wv, lp(li).v.clone()),
                        MatSlot::Wo => (&self.artifact.layers[li].wo, if sink { lp(li).wo_sink.clone() } else { lp(li).wo.clone() }),
                        MatSlot::Up => (&self.artifact.layers[li].w_up, if sink { lp(li).up_sink.clone() } else { lp(li).up.clone() }),
                        MatSlot::Down => {
                            (&self.artifact.layers[li].w_down, if sink { lp(li).down_sink.clone() } else { lp(li).down.clone() })
                        }
                        MatSlot::Head => (&self.artifact.unembed, tile(self.logits_out, shape.vocab)),
                        MatSlot::Gate => return Err(A16EngineError::MalformedParams("gate is a rescale site")),
                    };
                    a16_matmul_requant(self.fast, w, &x, &params).map_err(refuse("matmul_requant"))?
                }
                PlanOp::MatMulRescale(slot) => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let li = layer.as_ref().map(|(li, _)| *li).unwrap_or(0);
                    let (w, params) = match slot {
                        MatSlot::Gate => (&self.artifact.layers[li].w_gate, lp(li).gate.clone()),
                        _ => return Err(A16EngineError::MalformedParams("a rescale site that is not the gate")),
                    };
                    a16_matmul_rescale(self.fast, w, &x, &params).map_err(refuse("matmul_rescale"))?
                }
                PlanOp::Rope { kv } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let heads = if kv { shape.n_kv_heads } else { shape.n_heads };
                    if x.len() != heads * shape.d_head {
                        return Err(A16EngineError::OpRefused("a rotation whose input is not its declared width"));
                    }
                    let mut out = Vec::with_capacity(x.len());
                    for hd in 0..heads {
                        let slice = &x[hd * shape.d_head..(hd + 1) * shape.d_head];
                        out.extend(a16_rope(slice, cos_row, sin_row).map_err(|_| A16EngineError::OpRefused("rope"))?);
                    }
                    out
                }
                PlanOp::AttnScores => {
                    let q = resolve(&node.inputs[0], &rows)?;
                    let k_series = resolve(&node.inputs[1], &rows)?;
                    let li = layer.as_ref().map(|(li, _)| *li).unwrap_or(0);
                    let history = k_series.len() / kv_dim.max(1);
                    a16_attn_scores(
                        self.fast,
                        &q,
                        &k_series,
                        shape.n_heads,
                        shape.n_kv_heads,
                        shape.d_head,
                        &tile(lp(li).logits, shape.n_heads * history),
                    )
                    .map_err(refuse("attn_scores"))?
                }
                PlanOp::Softmax => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let li = layer.as_ref().map(|(li, _)| *li).unwrap_or(0);
                    let history = x.len() / shape.n_heads.max(1);
                    a16_softmax_rows(&x, history, lp(li).softmax_up).map_err(refuse("softmax"))?
                }
                PlanOp::AttnValues => {
                    let p = resolve(&node.inputs[0], &rows)?;
                    let v_series = resolve(&node.inputs[1], &rows)?;
                    let li = layer.as_ref().map(|(li, _)| *li).unwrap_or(0);
                    a16_attn_values(
                        self.fast,
                        &p,
                        &v_series,
                        shape.n_heads,
                        shape.n_kv_heads,
                        shape.d_head,
                        &tile(lp(li).values, shape.n_heads * shape.d_head),
                    )
                    .map_err(refuse("attn_values"))?
                }
                // **The fused attention site** (ADR-0082 Decision 1). The four shipped kernels
                // composed — W9, W11, the probability requantization, W10 — with the three
                // intermediates living only in this frame. The row pushed below is the OUTPUT
                // row, so the site commits `heads x d_head` codes at every context instead of
                // three rows that grow with the position.
                //
                // Composed from the engine's OWN kernels rather than from
                // `a16_attn_fused_via_tiles_v1`: the two are proven equal at every history
                // length and tile width (`palw_base0_a16::fused::the_tile_route_is_the
                // _composition`), and the composition is what the fast projections are asserted
                // bit-identical against, so this keeps the executor at the runtime's speed while
                // computing exactly what `a16_attn_fused_reference_v1` defines. The equality is
                // held by `the_fused_arm_is_the_reference_composition` below.
                PlanOp::AttnFused => {
                    let q = resolve(&node.inputs[0], &rows)?;
                    let k_series = resolve(&node.inputs[1], &rows)?;
                    let v_series = resolve(&node.inputs[2], &rows)?;
                    let li = layer.as_ref().map(|(li, _)| *li).unwrap_or(0);
                    let history = k_series.len() / kv_dim.max(1);
                    let scores = a16_attn_scores(
                        self.fast,
                        &q,
                        &k_series,
                        shape.n_heads,
                        shape.n_kv_heads,
                        shape.d_head,
                        &tile(lp(li).logits, shape.n_heads * history),
                    )
                    .map_err(refuse("attn_scores"))?;
                    let probs = a16_softmax_rows(&scores, history, lp(li).softmax_up).map_err(refuse("softmax"))?;
                    let codes = a16_requant(&probs, &tile(lp(li).probs, probs.len())).map_err(refuse("requant"))?;
                    a16_attn_values(
                        self.fast,
                        &codes,
                        &v_series,
                        shape.n_heads,
                        shape.n_kv_heads,
                        shape.d_head,
                        &tile(lp(li).values, shape.n_heads * shape.d_head),
                    )
                    .map_err(refuse("attn_values"))?
                }
                PlanOp::AddElem => {
                    let a = resolve(&node.inputs[0], &rows)?;
                    let b = resolve(&node.inputs[1], &rows)?;
                    a16_add_elem(&a, &b).map_err(refuse("add_elem"))?
                }
                PlanOp::MulElem => {
                    let a = resolve(&node.inputs[0], &rows)?;
                    let b = resolve(&node.inputs[1], &rows)?;
                    a16_mul_elem(&a, &b).map_err(refuse("mul_elem"))?
                }
                PlanOp::Silu => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    silu(&x)
                }
            };

            // The declared cache write, honored where declared — the ROTATED key and the raw V
            // are conventions of the DECLARATION (the IR carries the role on those nodes), so a
            // profile that declared them elsewhere would cache elsewhere, and its court would
            // read the same declaration.
            match node.role {
                kaspa_consensus_core::palw_step::PalwStepNodeRoleV1::KCacheWrite => {
                    let (li, cache) = layer.as_mut().ok_or(A16EngineError::MalformedParams("a cache write outside a layer"))?;
                    cache.keys[*li].push(out.clone());
                }
                kaspa_consensus_core::palw_step::PalwStepNodeRoleV1::VCacheWrite => {
                    let (li, cache) = layer.as_mut().ok_or(A16EngineError::MalformedParams("a cache write outside a layer"))?;
                    cache.values[*li].push(out.clone());
                }
                kaspa_consensus_core::palw_step::PalwStepNodeRoleV1::Plain => {}
            }
            rows.push(out);
        }
        Ok(rows)
    }
}

/// Compile one declared table. Every refusal names the node and the reason — this function IS
/// the kernel-set boundary of ADR-0067 Decision 3.
fn plan_table(
    nodes: &[PalwStepNodeV1],
    table: &'static str,
    shape: &Base0ShapeV1,
    layer_in: Option<u32>,
) -> Result<Vec<PlanNode>, A16PlanErrorV1> {
    use kaspa_consensus_core::palw_step::PalwStepOpKindV1 as Op;

    // **Only the layer table has a layer.** A `blk.{layer}.*` operand names a per-layer parameter
    // row, and `walk_table` resolves the layer as `layer.unwrap_or(0)` — so a pre or post node
    // carrying one would silently execute under layer 0's parameters. The class would run and
    // certify; its every dispute would then be unadjudicable, because the court walks the
    // DECLARED graph and the declaration says nothing about which layer that node meant.
    let per_layer_ok = table == "layer";

    let k_embed = kernel_semantics_id_v1(KDESC_A16_EMBED);
    let k_req = kernel_semantics_id_v1(KDESC_A16_REQUANTIZE);
    let k_mm = kernel_semantics_id_v1(KDESC_A16_MATMUL_REQUANT);
    let k_rs = kernel_semantics_id_v1(KDESC_A16_MATMUL_RESCALE);
    let k_rms = kernel_semantics_id_v1(KDESC_A16_RMS_NORM);
    let k_rope = kernel_semantics_id_v1(KDESC_A16_ROPE);
    let k_scores = kernel_semantics_id_v1(KDESC_A16_ATTN_SCORES);
    let k_soft = kernel_semantics_id_v1(KDESC_A16_SOFTMAX);
    let k_vals = kernel_semantics_id_v1(KDESC_A16_ATTN_VALUES);
    let k_fused = kernel_semantics_id_v1(KDESC_A16_ATTN_FUSED);
    let k_add = kernel_semantics_id_v1(KDESC_A16_ADD_ELEM);
    let k_mul = kernel_semantics_id_v1(KDESC_A16_MUL_ELEM);
    let k_silu = kernel_semantics_id_v1(KDESC_Q36_SILU);

    let d = shape.d_model() as u32;
    let kv_dim = shape.kv_dim() as u32;
    let ffn = shape.d_ff as u32;
    let vocab = shape.vocab as u32;
    let heads = shape.n_heads as u32;

    let refuse = |index: usize, reason: String| A16PlanErrorV1::UnservedNode { table, index, reason };

    /// What a node's output IS, statically: a fixed element count, or a kv-scaled row family.
    /// Tracked so every consumer's input width is checked AT PLAN TIME — the fuzz gate's first
    /// find was a gate-and-plan-accepted profile whose rewired input refs fed a kv-width row to
    /// the q-rope, and the head slicing walked off the end mid-forward. A width-sound plan makes
    /// that whole class of profile unplannable instead of un-panickable.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum W {
        Fixed(u32),
        KvScaled(u32),
        Series,
    }

    let mut widths: Vec<W> = Vec::with_capacity(nodes.len());
    let mut out = Vec::with_capacity(nodes.len());
    for (index, node) in nodes.iter().enumerate() {
        // The declared inputs, resolved first — every op checks its arity against them.
        let mut inputs = Vec::with_capacity(node.input_refs.len());
        for r in &node.input_refs {
            let input = match *r {
                PALW_STEP_INPUT_LAYER_IN => PlanInput::LayerIn,
                PALW_STEP_INPUT_KV_K => PlanInput::CachedK,
                PALW_STEP_INPUT_KV_V => PlanInput::CachedV,
                i if i >= PALW_STEP_INPUT_SENTINEL_MIN => {
                    return Err(refuse(index, format!("input sentinel {i:#x} is not one this family serves")));
                }
                i => {
                    if (i as usize) >= index {
                        return Err(refuse(index, format!("input ref {i} is not an earlier node of this table")));
                    }
                    PlanInput::Row(i as usize)
                }
            };
            inputs.push(input);
        }
        let arity = |n: usize| -> Result<(), A16PlanErrorV1> {
            if inputs.len() != n { Err(refuse(index, format!("arity {} where the kernel takes {n}", inputs.len()))) } else { Ok(()) }
        };
        let width_of = |input: &PlanInput| -> W {
            match input {
                PlanInput::Row(i) => widths[*i],
                // NOT an assumption: the caller passes the producing table's terminal width, because
                // `forward_token_planned` feeds this table whatever the previous table's LAST declared
                // node produced. Assuming hidden width here let a gate-accepted profile hand a
                // kv-width residual to a hidden-width consumer, and `PlanOp::Rope` slices by hand —
                // an out-of-bounds panic where a refusal belonged.
                PlanInput::LayerIn => layer_in.map(W::Fixed).unwrap_or(W::Series),
                PlanInput::CachedK | PlanInput::CachedV => W::Series,
            }
        };
        let need = |slot: usize, want: W, what: &str| -> Result<(), A16PlanErrorV1> {
            let got = width_of(&inputs[slot]);
            if got != want {
                return Err(refuse(index, format!("input {slot} is {got:?} where {what} takes {want:?}")));
            }
            Ok(())
        };
        let width = |want: u32, what: &str| -> Result<(), A16PlanErrorV1> {
            match node.out_len {
                PalwStepOutLenV1::Fixed { elements } if elements == want => Ok(()),
                other => Err(refuse(index, format!("out width {other:?} where {what} is {want}"))),
            }
        };
        let kv_scaled = |want_mult: u32| -> Result<(), A16PlanErrorV1> {
            match node.out_len {
                PalwStepOutLenV1::KvScaled { multiplier } if multiplier == want_mult => Ok(()),
                other => Err(refuse(index, format!("out width {other:?} where the kv-scaled multiplier is {want_mult}"))),
            }
        };
        // Weight-bearing nodes must be the integer dtype this family's matmuls read. Dtype IS
        // arithmetic (the profile's own field doc), so a foreign byte is an unserved node, not a
        // detail.
        let dtype_i8 = || -> Result<(), A16PlanErrorV1> {
            if node.weight_dtypes.iter().all(|b| *b == kaspa_consensus_core::palw_qwen25_profile::QWEN25_WEIGHT_DTYPE_I8) {
                Ok(())
            } else {
                Err(refuse(index, "a weight dtype this family's kernels do not read".to_string()))
            }
        };

        let name = node.weight_name.as_str();
        if !per_layer_ok && strip_layer(name).is_some() {
            return Err(refuse(index, format!("operand {name:?} names a per-layer row, and the {table} table has no layer")));
        }
        let kid = node.kernel_semantics_id;
        let op = match (node.op_kind, name) {
            (Op::EmbedLookup, "token_embd.weight") if kid == k_embed => {
                arity(0)?;
                width(d, "hidden")?;
                dtype_i8()?;
                PlanOp::EmbedGather
            }
            (Op::RmsNorm, "") if kid == k_rms => {
                arity(1)?;
                width(d, "hidden")?;
                need(0, W::Fixed(d), "the norm")?;
                PlanOp::RmsNorm
            }
            (Op::MulElem, n) if kid == k_req => {
                arity(1)?;
                // Probs is the one requant whose width scales with the kv history; every other
                // slot is fixed. One arm, two width rules, stated rather than special-cased.
                let (slot, fixed) = match strip_layer(n) {
                    Some("attn_norm.a16") => (ReqSlot::AttnNorm, Some(d)),
                    Some("attn_probs.a16") => (ReqSlot::Probs, None),
                    Some("attn_align.a16") => (ReqSlot::AttnAlign, Some(d)),
                    Some("attn_residual.a16") => (ReqSlot::AttnResidual, Some(d)),
                    Some("ffn_norm.a16") => (ReqSlot::FfnNorm, Some(d)),
                    Some("ffn_silu.a16") => (ReqSlot::SiluQ, Some(ffn)),
                    Some("ffn_gated.a16") => (ReqSlot::Gated, Some(ffn)),
                    Some("ffn_align.a16") => (ReqSlot::FfnAlign, Some(d)),
                    Some("ffn_residual.a16") => (ReqSlot::FfnResidual, Some(d)),
                    None if n == "embed_lift.a16" => (ReqSlot::EmbedLift, Some(d)),
                    None if n == "final_norm.a16" => (ReqSlot::FinalNorm, Some(d)),
                    _ => return Err(refuse(index, format!("requant operand {n:?} is not one this store names"))),
                };
                match fixed {
                    Some(want) => {
                        width(want, "the slot's width")?;
                        need(0, W::Fixed(want), "a width-preserving requant")?;
                    }
                    None => {
                        kv_scaled(heads)?;
                        need(0, W::KvScaled(heads), "the probs requant")?;
                    }
                }
                PlanOp::Requant(slot)
            }
            (Op::MatMulQuant, n) if kid == k_mm => {
                arity(1)?;
                dtype_i8()?;
                let slot = match strip_layer(n) {
                    Some("attn_q.weight") => (MatSlot::Q, d),
                    Some("attn_k.weight") => (MatSlot::K, kv_dim),
                    Some("attn_v.weight") => (MatSlot::V, kv_dim),
                    Some("attn_output.weight") => (MatSlot::Wo, d),
                    Some("ffn_up.weight") => (MatSlot::Up, ffn),
                    Some("ffn_down.weight") => (MatSlot::Down, d),
                    // Both head spellings resolve to the same slot: the v1 class ties the head to
                    // the embedding by NAME, the v2 class names the engine's own head view so the
                    // gather's rows and the matmul's tiles stop colliding in the inventory
                    // (`QWEN25_A16_HEAD_TENSOR_V2`'s doc). The bytes are `artifact.unembed` either
                    // way — tying remains a fact about bytes.
                    None if n == "token_embd.weight" || n == "output.weight" => (MatSlot::Head, vocab),
                    _ => return Err(refuse(index, format!("matmul operand {n:?} is not one this store names"))),
                };
                width(slot.1, "the slot's width")?;
                let in_width = match slot.0 {
                    MatSlot::Down => ffn,
                    _ => d,
                };
                need(0, W::Fixed(in_width), "this matmul's fan-in")?;
                PlanOp::MatMulRequant(slot.0)
            }
            (Op::MatMulQuant, n) if kid == k_rs => {
                arity(1)?;
                dtype_i8()?;
                match strip_layer(n) {
                    Some("ffn_gate.weight") => {
                        width(ffn, "ffn")?;
                        need(0, W::Fixed(d), "the gate's fan-in")?;
                        PlanOp::MatMulRescale(MatSlot::Gate)
                    }
                    _ => return Err(refuse(index, format!("rescale operand {n:?} is not one this store names"))),
                }
            }
            (Op::MatMulQuant, n) if kid == k_scores => {
                arity(2)?;
                if strip_layer(n) != Some("attn_logits.a16") {
                    return Err(refuse(index, format!("scores operand {n:?} is not one this store names")));
                }
                kv_scaled(heads)?;
                need(0, W::Fixed(d), "the query")?;
                if inputs.get(1) != Some(&PlanInput::CachedK) {
                    return Err(refuse(index, "scores read something other than the key series".to_string()));
                }
                PlanOp::AttnScores
            }
            (Op::SoftMax, n) if kid == k_soft => {
                arity(1)?;
                if strip_layer(n) != Some("attn_softmax_up") {
                    return Err(refuse(index, format!("softmax operand {n:?} is not one this store names")));
                }
                kv_scaled(heads)?;
                need(0, W::KvScaled(heads), "the row softmax")?;
                PlanOp::Softmax
            }
            (Op::MatMulQuant, n) if kid == k_vals => {
                arity(2)?;
                if strip_layer(n) != Some("attn_values.a16") {
                    return Err(refuse(index, format!("values operand {n:?} is not one this store names")));
                }
                width(d, "hidden")?;
                need(0, W::KvScaled(heads), "the probability rows")?;
                if inputs.get(1) != Some(&PlanInput::CachedV) {
                    return Err(refuse(index, "values read something other than the value series".to_string()));
                }
                PlanOp::AttnValues
            }
            // **ADR-0082 Decision 1**, the fused site. Its four registered operands come from the
            // ONE the node names, through `palw_attn_fused_tensors_v1` — the same function the
            // adjudicator reads, so the engine and the court cannot resolve different tensors —
            // and the derived names are then checked against the ones this store actually holds.
            (Op::AttnFused, n) if kid == k_fused => {
                arity(3)?;
                let t = palw_attn_fused_tensors_v1(n)
                    .ok_or_else(|| refuse(index, format!("fused operand {n:?} is not a softmax store this family registers")))?;
                for (what, got, want) in [
                    ("softmax", t.softmax_up.as_str(), "attn_softmax_up"),
                    ("scores", t.scores.as_str(), "attn_logits.a16"),
                    ("probs", t.probs.as_str(), "attn_probs.a16"),
                    ("values", t.values.as_str(), "attn_values.a16"),
                ] {
                    if strip_layer(got) != Some(want) {
                        let why = format!("the fused site's {what} operand derives to {got:?}, which is not one this store names");
                        return Err(refuse(index, why));
                    }
                }
                // The committed row is the OUTPUT row (Z0's first half); the query is the rotated
                // one and the two series are the caches, in the order the court reads them.
                width(d, "hidden")?;
                need(0, W::Fixed(d), "the query")?;
                if inputs.get(1) != Some(&PlanInput::CachedK) {
                    return Err(refuse(index, "a fused attention site read something other than the key series".to_string()));
                }
                if inputs.get(2) != Some(&PlanInput::CachedV) {
                    return Err(refuse(index, "a fused attention site read something other than the value series".to_string()));
                }
                PlanOp::AttnFused
            }
            (Op::RopeImrope, "rope") if kid == k_rope => {
                arity(1)?;
                use kaspa_consensus_core::palw_step::PalwStepNodeRoleV1 as Role;
                let kv = match (node.role, node.out_len) {
                    (Role::KCacheWrite, _) => true,
                    (_, PalwStepOutLenV1::Fixed { elements }) if elements == kv_dim && kv_dim != d => true,
                    (_, PalwStepOutLenV1::Fixed { elements }) if elements == d => false,
                    (_, other) => return Err(refuse(index, format!("rope out width {other:?} fits neither q nor k"))),
                };
                width(if kv { kv_dim } else { d }, "the rotated width")?;
                need(0, W::Fixed(if kv { kv_dim } else { d }), "the rotation")?;
                PlanOp::Rope { kv }
            }
            (Op::AddElem, "") if kid == k_add => {
                arity(2)?;
                width(d, "hidden")?;
                need(0, W::Fixed(d), "the residual add")?;
                need(1, W::Fixed(d), "the residual add")?;
                PlanOp::AddElem
            }
            (Op::MulElem, "") if kid == k_mul => {
                arity(2)?;
                width(ffn, "ffn")?;
                need(0, W::Fixed(ffn), "the gated product")?;
                need(1, W::Fixed(ffn), "the gated product")?;
                PlanOp::MulElem
            }
            (Op::Silu, "") if kid == k_silu => {
                arity(1)?;
                width(ffn, "ffn")?;
                need(0, W::Fixed(ffn), "the nonlinearity")?;
                PlanOp::Silu
            }
            (op, n) => {
                return Err(refuse(
                    index,
                    format!("op {op:?} with kernel {kid} and operand {n:?} is outside this build's served vocabulary"),
                ));
            }
        };
        widths.push(match node.out_len {
            PalwStepOutLenV1::Fixed { elements } => W::Fixed(elements),
            PalwStepOutLenV1::KvScaled { multiplier } => W::KvScaled(multiplier),
        });
        out.push(PlanNode { op, inputs, role: node.role });
    }
    Ok(out)
}

/// `blk.{layer}.suffix` → `suffix`. The `{layer}` template survives lowering (the profile's own
/// field doc: substituted at interpretation time), so the ABI here is the literal template.
fn strip_layer(name: &str) -> Option<&str> {
    name.strip_prefix("blk.{layer}.")
}

#[cfg(test)]
mod profile_plan_tests {
    use super::*;
    use crate::artifact::LN_THETA_10000_GEN_Q;
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v1, qwen25_a16_profile_v2};

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

    fn geometry(a: &Base0ArtifactV1) -> PalwQwen25GeometryV1 {
        PalwQwen25GeometryV1 {
            layer_count: a.shape.n_layers as u16,
            hidden_dim: a.shape.d_model() as u32,
            ffn_dim: a.shape.d_ff as u32,
            attn_heads: a.shape.n_heads as u16,
            attn_kv_heads: a.shape.n_kv_heads as u16,
            attn_head_dim: a.shape.d_head as u32,
            vocab_size: a.shape.vocab as u32,
            n_ctx: 16,
            n_threads: 1,
            rms_eps_q: a.shape.eps_q,
            tile_len: 4,
        }
    }

    /// **ADR-0082 Decision 1: the fused arm IS the four shipped kernels, and the graph around it
    /// did not move.**
    ///
    /// Three claims in one walk, at every layer and every position including the sink:
    ///
    /// * the v5 plan's committed row at the fused site equals the v2 plan's `ATTN_VALUES` row —
    ///   the site commits the OUTPUT row and the three context-wide rows simply stop existing;
    /// * that row equals `a16_attn_fused_reference_v1`, which is the kernel descriptor's declared
    ///   semantics and exactly what the court's arm recomputes, AND
    ///   `a16_attn_fused_via_tiles_v1`, which is what a dissection folds to — so the executor,
    ///   the whole-row court and the tile route are one number (invariant Z1);
    /// * the logits and the cache the whole forward leaves behind are bit-identical between the
    ///   two graphs, which is the statement that graph v5 computes the same model.
    #[test]
    fn the_fused_arm_is_the_reference_composition() {
        use kaspa_consensus_core::palw_base0_a16::{A16AttnFusedParamsV1, a16_attn_fused_reference_v1, a16_attn_fused_via_tiles_v1};
        use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v5;
        use kaspa_consensus_core::palw_step::PalwStepOutLenV1 as OutLen;

        for (layers, d_head, d_ff) in [(1usize, 4usize, 12usize), (2, 8, 16)] {
            let artifact = artifact(layers, d_head, d_ff);
            let engine = A16Engine::new(&artifact).expect("the store resolves");
            let g = geometry(&artifact);
            let v2 = qwen25_a16_profile_v2(g).expect("the v2 profile builds");
            let v5 = qwen25_a16_profile_v5(g).expect("the v5 profile builds");
            // Z0's first half, on the row this build will actually execute.
            assert!(
                v5.attn_nodes.iter().all(|n| !matches!(n.out_len, OutLen::KvScaled { .. })),
                "a v5 layer table still commits a context-shaped row"
            );
            let fused_at = v5
                .attn_nodes
                .iter()
                .position(|n| n.op_kind == kaspa_consensus_core::palw_step::PalwStepOpKindV1::AttnFused)
                .expect("the v5 layer table has a fused site");
            let plan_v2 = engine.plan_from_profile(&v2).expect("v2 is servable");
            let plan_v5 = engine.plan_from_profile(&v5).expect("v5 is servable");

            let mut cache_v2 = A16Cache::new(layers);
            let mut cache_v5 = A16Cache::new(layers);
            for position in 0..6usize {
                let token = (position * 7 + 3) % artifact.shape.vocab;
                let (a, ta) = engine.forward_token_planned(&plan_v2, &mut cache_v2, token, position).expect("v2 walks");
                let (b, tb) = engine.forward_token_planned(&plan_v5, &mut cache_v5, token, position).expect("v5 walks");
                assert_eq!(a, b, "the logits moved at position {position}");
                assert_eq!(ta.pre, tb.pre, "pre rows at position {position}");
                assert_eq!(ta.post, tb.post, "post rows at position {position}");
                for li in 0..layers {
                    let v2_rows = &ta.attn[li];
                    let v5_rows = &tb.attn[li];
                    assert_eq!(v5_rows.len() + 3, v2_rows.len(), "layer {li}: four rows became one");
                    // The fused row is the values row, and every row after it is unchanged.
                    assert_eq!(v5_rows[fused_at], v2_rows[fused_at + 3], "layer {li} position {position}: the fused row");
                    assert_eq!(&v5_rows[..fused_at], &v2_rows[..fused_at], "layer {li}: the rows before the site");
                    assert_eq!(&v5_rows[fused_at + 1..], &v2_rows[fused_at + 4..], "layer {li}: the rows after the site");

                    // …and it is the catalogued composition, and the tile route, to the bit.
                    let lp = &engine.layers[li];
                    let params = A16AttnFusedParamsV1 {
                        scores: lp.logits,
                        probs: lp.probs,
                        values: lp.values,
                        up_bits: lp.softmax_up,
                    };
                    let q = &v5_rows[v5.attn_nodes[fused_at].input_refs[0] as usize];
                    let flat = |series: &Vec<Vec<i32>>| -> Vec<i32> { series.iter().flatten().copied().collect() };
                    let k = flat(&cache_v5.keys[li]);
                    let v = flat(&cache_v5.values[li]);
                    let (h, kvh, dh) = (artifact.shape.n_heads, artifact.shape.n_kv_heads, artifact.shape.d_head);
                    let reference = a16_attn_fused_reference_v1(q, &k, &v, h, kvh, dh, params).expect("the composition runs");
                    assert_eq!(v5_rows[fused_at], reference, "layer {li} position {position}: the engine parted from the reference");
                    for tile in [1usize, 4, 16] {
                        let tiled = a16_attn_fused_via_tiles_v1(q, &k, &v, h, kvh, dh, params, tile).expect("the tile route runs");
                        assert_eq!(v5_rows[fused_at], tiled, "layer {li} position {position} tile {tile}: the tile route parted");
                    }
                }
            }
            assert_eq!(cache_v2.keys, cache_v5.keys, "the two graphs must leave the same cache");
            assert_eq!(cache_v2.values, cache_v5.values);
        }
    }

    /// **ADR-0067's differential gate, in miniature: the compiled rows are the interpreter's
    /// reference vectors.** The plan is compiled from the CORRECTED profile — the graph that
    /// names what the engine does — so walking it must land on the compiled engine's exact bits:
    /// logits, every committed row of every table, and the cache left behind, across positions
    /// (including position 0, where the sink-variant parameters switch in).
    #[test]
    fn the_interpreter_and_the_compiled_engine_agree_bit_for_bit() {
        for (layers, d_head, d_ff) in [(1usize, 4usize, 12usize), (2, 8, 16)] {
            let artifact = artifact(layers, d_head, d_ff);
            let engine = A16Engine::new(&artifact).expect("the store resolves");
            let profile = qwen25_a16_profile_v2(geometry(&artifact)).expect("the corrected profile builds");
            let plan = engine.plan_from_profile(&profile).expect("the corrected profile is servable");

            let mut compiled_cache = A16Cache::new(layers);
            let mut planned_cache = A16Cache::new(layers);
            for position in 0..6usize {
                let token = (position * 7 + 3) % artifact.shape.vocab;
                let (a, ta) = engine.forward_token_traced(&mut compiled_cache, token, position).expect("compiled");
                let (b, tb) = engine.forward_token_planned(&plan, &mut planned_cache, token, position).expect("planned");
                assert_eq!(a, b, "logits at position {position}");
                assert_eq!(ta.pre, tb.pre, "pre rows at position {position}");
                assert_eq!(ta.attn, tb.attn, "layer rows at position {position}");
                assert_eq!(ta.post, tb.post, "post rows at position {position}");
            }
            assert_eq!(compiled_cache.keys, planned_cache.keys, "the caches must be the same state");
            assert_eq!(compiled_cache.values, planned_cache.values);
        }
    }

    /// **ADR-0067 Decision 5, clause (b): the differential over the classes THIS BUILD CARRIES.**
    ///
    /// The differential above proves the two engines agree on two synthetic geometries. That is
    /// the mechanism; it is not the claim Decision 5 makes, which is about the rows a node
    /// actually ships — because those are the graphs a chain registers, and a class the build
    /// carries that the interpreter cannot serve is a node that admits what it cannot run.
    ///
    /// Two halves, both over the REAL catalog:
    ///
    /// * every A16-family row's real profile is compiled, and the planner's answer is pinned. A
    ///   `graph-v2` row must be servable; a v1 row must be REFUSED, and refused for its own
    ///   documented reason (its pre table omits the embed-lift requant, so the interpreter
    ///   executing the declaration is a different arithmetic from the compiled engine — which is
    ///   exactly what makes v1 unfit for the free-prompt lane).
    /// * for every row the planner serves, the SAME graph is built at a runnable geometry and the
    ///   two engines are compared row for row. The node tables are generated from one IR and one
    ///   geometry, so a reduced geometry walks the identical node sequence; what it cannot do is
    ///   hold 1.7 GiB of weights in a unit test, which is why the artifact is derived.
    #[test]
    fn the_interpreter_serves_every_a16_class_this_build_carries() {
        use crate::classes::canonical_classes_v1;
        use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;

        let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court");
        let a16: Vec<_> = canonical_classes_v1(&court)
            .into_iter()
            .filter(|c| matches!(c.source, crate::classes::ArtifactSourceV1::ConvertedA16))
            .collect();
        assert!(!a16.is_empty(), "the build carries A16 rows, or this test gates nothing");

        let mut served = 0usize;
        for entry in &a16 {
            // The row's REAL profile — the graph a chain would register for it.
            let probe_artifact = artifact(1, 4, 12);
            let probe_engine = A16Engine::new(&probe_artifact).expect("the store resolves");
            let real = probe_engine.plan_from_profile(&entry.profile);
            // **Derived from the GRAPH, not matched on the NAME.** This read
            // `entry.model_id.ends_with("/graph-v2")`, which is the row's label rather than its
            // content — and the moment a second corrected row arrived under a different label
            // (`/graph-v3`, the row that declares the epsilon its artifact executes) the test
            // classified it as uncorrected, built its comparison profile from the v1 tables, and
            // failed asserting a v1 property of a v2 graph. The name is a fact about what we called
            // the row; what the assertions below are about is whether its pre table NAMES the
            // embed-lift requant, which is a fact about the graph. Same rule as everywhere else in
            // this tree: derive, never declare.
            let corrected = entry.profile.pre_nodes.len() == kaspa_consensus_core::palw_base0_profile::QWEN25_A16_PRE_IR_V2.len();
            match (&real, corrected) {
                // A real profile against a MISMATCHED artifact must refuse on geometry — that is
                // the root check doing its job, and it tells us the planner reached the geometry
                // gate rather than accepting a graph it cannot size.
                (Err(A16PlanErrorV1::GeometryMismatch { .. }), _) => {}
                (Err(A16PlanErrorV1::UnservedNode { table, index, reason }), false) => {
                    assert!(
                        reason.contains("requant") || reason.contains("vocabulary") || !reason.is_empty(),
                        "a v1 row's refusal must name something: {table}[{index}] {reason}"
                    );
                }
                (other, _) => panic!("{}: unexpected plan answer {other:?}", entry.model_id),
            }

            // The same graph at a runnable geometry, both engines, row for row.
            let g = kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
                layer_count: 2,
                hidden_dim: 16,
                ffn_dim: 12,
                attn_heads: 4,
                attn_kv_heads: 2,
                attn_head_dim: 4,
                vocab_size: 64,
                n_ctx: entry.profile.n_ctx,
                n_threads: 1,
                rms_eps_q: 1,
                tile_len: 4,
            };
            let small = if corrected {
                kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2(g)
            } else {
                kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v1(g)
            };
            let Ok(small) = small else { continue };
            let small_artifact = artifact(2, 4, 12);
            let engine = A16Engine::new(&small_artifact).expect("the store resolves");
            let Ok(plan) = engine.plan_from_profile(&small) else {
                // A v1 row is refused here for its own reason; that IS the pinned answer.
                assert!(!corrected, "{}: a corrected row must be servable at a runnable geometry", entry.model_id);
                continue;
            };
            served += 1;
            let (mut a, mut b) = (A16Cache::new(2), A16Cache::new(2));
            for position in 0..4usize {
                let token = (position * 11 + 5) % small_artifact.shape.vocab;
                let (la, ta) = engine.forward_token_traced(&mut a, token, position).expect("compiled");
                let (lb, tb) = engine.forward_token_planned(&plan, &mut b, token, position).expect("planned");
                if corrected {
                    // The corrected graph names what the engine does, so the two must agree
                    // everywhere — this is the property the whole ADR turns on.
                    assert_eq!(la, lb, "{} logits at {position}", entry.model_id);
                    assert_eq!(ta.pre, tb.pre, "{} pre rows at {position}", entry.model_id);
                    assert_eq!(ta.attn, tb.attn, "{} layer rows at {position}", entry.model_id);
                    assert_eq!(ta.post, tb.post, "{} post rows at {position}", entry.model_id);
                } else {
                    // **A v1 row is servable and DIFFERENT, and that is the finding, not a bug in
                    // this test.** Its pre table declares one node where the engine performs two
                    // (the gather, then the embed-lift requant), so an interpreter executing the
                    // declaration commits one row where the compiled engine commits two. A
                    // producer running the compiled engine under a v1 class therefore commits
                    // rows at coordinates the court does not have — which is precisely why
                    // ADR-0049 Decision F refuses v1 on the free-prompt lane, and precisely what
                    // the interpreter makes structurally impossible for a class built from its
                    // own declaration.
                    assert_eq!(
                        tb.pre.len(),
                        entry.profile.pre_nodes.len(),
                        "{} commits one row per DECLARED pre node",
                        entry.model_id
                    );
                    assert!(
                        ta.pre.len() > tb.pre.len(),
                        "{}: the compiled engine performs a narrowing this graph does not name — if this ever stops \
                         being true, the v1 rows have become servable and Decision F's refusal needs revisiting",
                        entry.model_id
                    );
                }
            }
        }
        assert!(served > 0, "at least one carried class must be servable, or the interpreter serves nothing this build ships");
    }

    /// **The audit's own findings, as tests.** Each of these was a gate-accepted profile that the
    /// planner let through and the forward then panicked on, or executed under the wrong
    /// parameters — found by adversarial review of this file (2026-08-31), and each is now a
    /// named refusal at PLAN time.
    #[test]
    fn a_gate_accepted_profile_cannot_reach_a_panic_through_the_plan() {
        let artifact = artifact(1, 4, 12);
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let good = qwen25_a16_profile_v2(geometry(&artifact)).expect("builds");
        let hidden = artifact.shape.d_model() as u32;
        let kv_dim = artifact.shape.kv_dim() as u32;

        // (1) A pre table that ends at a width no layer reads. `forward_token_planned` feeds the
        // layer table `rows.last()`, so this used to hand a kv-width residual to a hidden-width
        // consumer — and `PlanOp::Rope` slices by hand, which is an out-of-bounds panic.
        let mut narrow_pre = good.clone();
        narrow_pre.pre_nodes[1].out_len = PalwStepOutLenV1::Fixed { elements: kv_dim };
        assert_ne!(kv_dim, hidden, "the fixture must actually be GQA or this proves nothing");
        match engine.plan_from_profile(&narrow_pre) {
            Err(A16PlanErrorV1::UnservedNode { table: "pre", .. }) => {}
            other => panic!("a pre table that does not end on the hidden stream must be refused, got {other:?}"),
        }

        // (2) A per-layer operand in a table that HAS no layer. The walk resolves the layer as
        // `unwrap_or(0)`, so this used to execute silently under layer 0's parameters: a class
        // that runs and certifies, and whose every dispute is unadjudicable because the court
        // walks a declaration that never said which layer it meant.
        let mut per_layer_in_post = good.clone();
        per_layer_in_post.post_nodes[1].weight_name = "blk.{layer}.attn_norm.a16".into();
        match engine.plan_from_profile(&per_layer_in_post) {
            Err(A16PlanErrorV1::UnservedNode { table: "post", reason, .. }) => {
                assert!(reason.contains("per-layer"), "the refusal names why: {reason}");
            }
            other => panic!("a per-layer operand outside the layer table must be refused, got {other:?}"),
        }
    }

    /// **The refusals name the boundary** (ADR-0067 Decision 3). A foreign kernel, a forward
    /// input reference, a stranger's operand name and a wrong geometry each fail at PLAN time,
    /// each naming what this build cannot serve — never a mid-forward surprise.
    #[test]
    fn an_unservable_profile_is_refused_at_plan_time_by_name() {
        let artifact = artifact(1, 4, 12);
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let good = qwen25_a16_profile_v2(geometry(&artifact)).expect("builds");

        let mut foreign_kernel = good.clone();
        foreign_kernel.attn_nodes[0].kernel_semantics_id = kernel_semantics_id_v1("a16/some-future-kernel/v9");
        match engine.plan_from_profile(&foreign_kernel) {
            Err(A16PlanErrorV1::UnservedNode { table: "layer", index: 0, .. }) => {}
            other => panic!("a foreign kernel must be an unserved node, got {other:?}"),
        }

        let mut forward_ref = good.clone();
        forward_ref.attn_nodes[0].input_refs = vec![5];
        match engine.plan_from_profile(&forward_ref) {
            Err(A16PlanErrorV1::UnservedNode { table: "layer", index: 0, .. }) => {}
            other => panic!("a forward input ref must be refused, got {other:?}"),
        }

        let mut stranger = good.clone();
        stranger.attn_nodes[1].weight_name = "blk.{layer}.someone_elses.a16".into();
        match engine.plan_from_profile(&stranger) {
            Err(A16PlanErrorV1::UnservedNode { table: "layer", index: 1, .. }) => {}
            other => panic!("a stranger's operand must be refused, got {other:?}"),
        }

        let mut wrong = good;
        wrong.hidden_dim += 1;
        match engine.plan_from_profile(&wrong) {
            Err(A16PlanErrorV1::GeometryMismatch { what: "hidden_dim", .. }) => {}
            other => panic!("a wrong geometry must be refused at the root, got {other:?}"),
        }
    }

    /// **The interpreter executes the DECLARATION, not the family habit.** The v1 profile omits
    /// the embed-lift requant (the Decision F defect that keeps the v1 class off the free-prompt
    /// lane). An interpreter serving it must run exactly the declared graph — one pre row, no
    /// lift — and therefore land on DIFFERENT logits than the compiled engine, which always
    /// lifts. That difference is the honest outcome: a court adjudicates the declared graph, and
    /// an interpreter that quietly "fixed" the declaration would commit arithmetic the court
    /// recomputes differently — the exact conviction Decision F exists to prevent.
    #[test]
    fn the_interpreter_executes_the_declared_graph_not_the_family_habit() {
        // The derived store's embed lift is unity, under which "lift" and "no lift" are the
        // same arithmetic. A shift would not do either (the first layer node is a
        // scale-invariant RMS norm), and a small zero offset drowns in the derived store's
        // saturation. A LARGE zero offset moves the residual stream itself, which nothing
        // downstream can launder — so that is the narrowing this test declares.
        let shape = artifact(1, 4, 12).shape;
        let mut store = derived_a16_store(&shape);
        for (name, bytes) in store.iter_mut() {
            if name == "embed_lift.a16" {
                *bytes = A16QuantParams { multiplier: 1, shift: 0, zero: 20_000 }.to_wire().to_vec();
            }
        }
        let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(store)
            .expect("sorted and unique");
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let v1 = qwen25_a16_profile_v1(geometry(&artifact)).expect("the v1 profile builds");
        let plan = engine.plan_from_profile(&v1).expect("every v1 node is individually servable");

        let mut planned_cache = A16Cache::new(1);
        let (planned, trace) = engine.forward_token_planned(&plan, &mut planned_cache, 3, 0).expect("the declared graph runs");
        assert_eq!(trace.pre.len(), 1, "the v1 declaration has ONE pre node, and one row was committed for it");

        let mut compiled_cache = A16Cache::new(1);
        let (compiled, _) = engine.forward_token_traced(&mut compiled_cache, 3, 0).expect("compiled");
        assert_ne!(planned, compiled, "the lift the v1 graph does not declare must not be executed for it");
    }
}
