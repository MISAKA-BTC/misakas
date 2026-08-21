//! The `PALW-BASE-0` forward pass — a decoder-only transformer written strictly through the
//! closed catalog.
//!
//! # Every arithmetic step goes through `palw_base0_ops`
//!
//! There is no `+` on an activation in this file, and that is the point rather than a style
//! choice. The class's whole claim is that its execution is a walk over a *closed, enumerated*
//! set of kernels, so that the ADR-0030..0033 court can bisect a disputed inference down to one
//! step and decide it arithmetically. An addition written inline here would be a step the court
//! has no rule for — it would still compute, and the disagreement it caused would be
//! unadjudicable.
//!
//! # The scale convention
//!
//! Activations live as `int8` codes at **Q7**: 127 reads as ≈ 1.0. Two scales appear between ops:
//!
//! * accumulator scale — whatever `MatMulQuant` produced, which depends on the fan-in;
//! * Qk (Q24) — the domain `SoftMax` and `Silu` are defined on.
//!
//! Moving *down* is `Requantize`. Moving *up* is [`kaspa_consensus_core::palw_base0::rescale_q`],
//! which exists because `Requantize` cannot: see the crate docs and ADR-0040 Decision H.
//!
//! [`ACTIVATION_BITS`] is a property of the class rather than of an artifact, so it is a constant
//! here and not a registration field. Making it a field would put a knob in the digest that every
//! real artifact would set identically, splitting one class into many that compute the same way —
//! and each split halves the panel that can be drawn to audit any of them.
//!
//! # Attention is quantised too
//!
//! The softmax probabilities are narrowed to `int8` before being applied to the values, so that
//! the weighted sum is an ordinary `DotI8` from the catalog rather than a mixed-scale reduction
//! the catalog has no op for. This costs precision in the attention weights and buys the property
//! that every reduction in the graph is the same op.

use kaspa_consensus_core::palw_base0::K;
use kaspa_consensus_core::palw_base0_ops::{
    self as ops, PalwBase0OpError, ScaleParams, add_elem, dot_i8, embed_lookup, matmul_quant, mul_elem,
    requantize_row, requantize_row_uniform, rescale_row, rms_norm, rope_table, silu, softmax,
};

use crate::artifact::Base0ArtifactV1;

/// Fractional bits in an `int8` activation code: 127 ≈ 1.0.
pub const ACTIVATION_BITS: u8 = 7;

// **The three narrowings moved into the artifact** (ADR-0049 Decision F, audit C-05/C-06).
//
// `BASE0_LAYER_IR` names each of them as a registered tensor, because a parameter the court
// cannot open is a step it cannot adjudicate — and a `const` in this binary is exactly a
// parameter nothing can open. `Base0ArtifactV1::CLASS_NARROWINGS` holds the same values, so no
// activation moved; what changed is that they are now data an inventory carries and an opening
// addresses. The engine reads them through the artifact so there is one source, not two.

/// Why a forward pass can be refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    Op(PalwBase0OpError),
    /// The position is at or past the artifact's `max_position`, so there is no rotary row for it.
    /// Refused rather than reusing a row: a reused rotation would make two different positions
    /// indistinguishable to attention, silently.
    PositionOutOfRange { got: usize, max: usize },
    /// The cache belongs to a different artifact than the one being run.
    CacheShapeMismatch,
}

impl From<PalwBase0OpError> for EngineError {
    fn from(e: PalwBase0OpError) -> Self {
        EngineError::Op(e)
    }
}

/// Per-layer key/value history, as `int8` codes.
///
/// Bound to a class id at construction: a cache filled under one artifact and reused under another
/// would silently mix two models' activations, and the mismatch would look like a bad model rather
/// than like a bug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvCache {
    /// The ARTIFACT digest, not the class id (ADR-0049 Decision G): what this cache is asking is
    /// "were you filled from these exact bytes", which is the digest's job. A class id would be
    /// the wrong question — two artifacts of one class differ in weights and their caches must
    /// never be interchanged.
    artifact_digest: kaspa_hashes::Hash64,
    d_model: usize,
    /// `[layer][position][d_model]`.
    keys: Vec<Vec<Vec<i8>>>,
    values: Vec<Vec<Vec<i8>>>,
}

impl KvCache {
    pub fn new(artifact: &Base0ArtifactV1) -> Self {
        Self {
            artifact_digest: artifact.artifact_digest(),
            d_model: artifact.shape.d_model(),
            keys: vec![Vec::new(); artifact.shape.n_layers],
            values: vec![Vec::new(); artifact.shape.n_layers],
        }
    }

    /// Number of positions already written.
    pub fn len(&self) -> usize {
        self.keys.first().map(|l| l.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// What a forward pass looked like on the inside.
///
/// # Why the engine is instrumented at all
///
/// The ADR-0040 H defect — attention flat, SwiGLU's gate linear — is invisible from the outside.
/// A degenerate pass still returns logits, still returns the *same* logits every run, and still
/// returns different logits for different weights, so determinism tests and
/// different-artifact tests both pass on a model that cannot compute. Worse, a badly calibrated
/// artifact drives every activation to zero and *those* tests still pass.
///
/// So the properties that separate "runs" from "works" are measured rather than assumed:
/// [`attention_spread`](Self::attention_spread) is what a flat softmax destroys, and
/// [`residual_peak`](Self::residual_peak) is what a miscalibrated requantisation destroys.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForwardProbe {
    /// Per `(layer, head)` in order: `max − min` of the softmax distribution, in Qk. Zero means
    /// the distribution is uniform and attention is selecting nothing.
    pub attention_spread: Vec<i32>,
    /// Per layer: the largest `|code|` in the residual stream after the block. Zero means the
    /// stream has collapsed and every downstream projection is reading zeros.
    pub residual_peak: Vec<i32>,
    /// Per layer: `(most negative, most positive)` code out of the SiLU gate.
    ///
    /// BOTH ends, because the peak alone cannot see the defect. When `Silu` is fed below its Qk
    /// domain, `IntSigmoid` returns ≈ 0.5 and `Silu` becomes the linear `x/2` — whose output is
    /// still large, still weight-dependent, and still symmetric. What actually distinguishes the
    /// real SiLU is its **asymmetry**: it floors at −0.278 while passing positives through, so a
    /// working gate has `|min| ≪ max` and a degenerate one has `|min| ≈ max`.
    pub gate_extremes: Vec<(i32, i32)>,
    /// **The step rows this pass produced, by `BASE0_LAYER_IR` slot** (C-01).
    ///
    /// The court adjudicates a tile of one node's output at one coordinate, and until something
    /// captured those rows there was no path from an execution to a step leg — the worker's own
    /// comments said so, and every leg in the tree was synthesised by a test. These are the whole-row
    /// steps: the ones the engine computes as a row rather than inside a per-head loop.
    ///
    /// `(layer, slot, row)`, in execution order. Values ride as `i32` lanes, which is what a BASE-0
    /// tile leaf carries.
    pub steps: Vec<(u16, u16, Vec<i32>)>,
    /// **The `Pre` table's rows** — the embedding gather. One node, and it is the input every
    /// layer's first norm reads.
    ///
    /// Kept apart from [`Self::steps`] because a row's TABLE is what decides its global slot:
    /// `steps` is indexed by `BASE0_LAYER_IR` and these are not. Merging them under one `(layer,
    /// slot)` pair is exactly the confusion that put every layer's rows on top of layer 0's the
    /// first time this was tried.
    pub pre_steps: Vec<(u16, Vec<i32>)>,
    /// **The `Post` table's rows** — the final norm, its narrowing, and the logits head.
    ///
    /// Until these were captured a step leg committed ZERO leaves for all three, so the head — the
    /// node that decides what the model actually said — was the one part of the graph no
    /// refutation could open. `(slot, row)`.
    pub post_steps: Vec<(u16, Vec<i32>)>,
}

impl ForwardProbe {
    /// The uniform-distribution spread for `n` keys is 0; this is the scale to compare against.
    pub fn uniform_probability(n: usize) -> i32 {
        ((1i64 << K) / n.max(1) as i64) as i32
    }
}

/// A forward pass bound to one artifact.
pub struct Base0Engine<'a> {
    artifact: &'a Base0ArtifactV1,
    artifact_digest: kaspa_hashes::Hash64,
}

impl<'a> Base0Engine<'a> {
    pub fn new(artifact: &'a Base0ArtifactV1) -> Self {
        Self { artifact, artifact_digest: artifact.artifact_digest() }
    }

    pub fn artifact(&self) -> &Base0ArtifactV1 {
        self.artifact
    }

    /// One token through the whole stack, returning `vocab` logits at accumulator scale.
    ///
    /// `position` must equal `cache.len()`: the cache is append-only because attention reads every
    /// earlier position, so a gap or an overwrite would change the result of *previous* tokens'
    /// attention as well as this one's.
    pub fn forward_token(&self, cache: &mut KvCache, token_id: usize, position: usize) -> Result<Vec<i32>, EngineError> {
        self.forward_token_probed(cache, token_id, position).map(|(logits, _)| logits)
    }

    /// [`forward_token`](Self::forward_token) plus a [`ForwardProbe`] of the intermediates. Same
    /// arithmetic, same result — the probe only records.
    pub fn forward_token_probed(
        &self,
        cache: &mut KvCache,
        token_id: usize,
        position: usize,
    ) -> Result<(Vec<i32>, ForwardProbe), EngineError> {
        let mut probe = ForwardProbe::default();
        let shape = &self.artifact.shape;
        let d = shape.d_model();
        if cache.artifact_digest != self.artifact_digest || cache.d_model != d || cache.keys.len() != shape.n_layers {
            return Err(EngineError::CacheShapeMismatch);
        }
        if position >= shape.max_position || position != cache.len() {
            return Err(EngineError::PositionOutOfRange { got: position, max: shape.max_position });
        }
        let (cos_row, sin_row) =
            self.artifact.rope.row(position).ok_or(EngineError::PositionOutOfRange { got: position, max: shape.max_position })?;

        let mut h: Vec<i8> = embed_lookup(&self.artifact.embed, shape.vocab, d, token_id)?.to_vec();
        probe.pre_steps.push((0, h.iter().map(|c| *c as i32).collect()));

        for li in 0..shape.n_layers {
            let layer = &self.artifact.layers[li];

            // ---- attention ------------------------------------------------------------------
            // **Every IR slot is captured** (audit C-01/C-05). The probe used to record ten of the
            // thirty-six, so a step leg could commit only the rows the capture happened to keep and
            // `execution_root` was, for the other twenty-six, whatever the miner wrote.
            let norm_q = rms_norm(&h, self.artifact.shape.eps_q)?;
            probe.steps.push((li as u16, 0, norm_q.clone()));
            let normed = requantize_row_uniform(&norm_q, self.artifact.norm_requant);
            probe.steps.push((li as u16, 1, normed.iter().map(|c| *c as i32).collect()));
            // Per-channel when the artifact supplies it — that is where a projection BIAS lives,
            // in the `zero` of each channel's triple — and tensor-wide otherwise, which is what
            // every artifact built before grouped-query attention meant.
            let narrow = |acc: &[i32], idx: usize| -> Result<Vec<i8>, EngineError> {
                match &layer.qkv_channel_requant {
                    Some(per) => requantize_row(acc, &per[idx]).map_err(EngineError::from),
                    None => Ok(requantize_row_uniform(acc, layer.requant[idx])),
                }
            };
            let kv = shape.kv_dim();
            let q_acc = matmul_quant(&layer.wq, &normed, d)?;
            probe.steps.push((li as u16, 2, q_acc.clone()));
            let q = narrow(&q_acc, 0)?;
            probe.steps.push((li as u16, 3, q.iter().map(|c| *c as i32).collect()));
            let k_acc = matmul_quant(&layer.wk, &normed, kv)?;
            probe.steps.push((li as u16, 4, k_acc.clone()));
            let k = narrow(&k_acc, 1)?;
            probe.steps.push((li as u16, 5, k.iter().map(|c| *c as i32).collect()));
            let v_acc = matmul_quant(&layer.wv, &normed, kv)?;
            probe.steps.push((li as u16, 6, v_acc.clone()));
            let v = narrow(&v_acc, 2)?;
            probe.steps.push((li as u16, 7, v.iter().map(|c| *c as i32).collect()));

            // Rotate each head's q and k in place. `RopeTable` preserves the scale it is handed,
            // so the widening here is a reinterpretation and the narrowing after is only a clamp.
            let mut q_rot = Vec::with_capacity(d);
            let mut q_rot_raw: Vec<i32> = Vec::with_capacity(d);
            for head in 0..shape.n_heads {
                let r = head * shape.d_head..(head + 1) * shape.d_head;
                let qh: Vec<i32> = q[r].iter().map(|c| *c as i32).collect();
                let rotated = rope_table(&qh, cos_row, sin_row)?;
                q_rot_raw.extend(rotated.iter().copied());
                q_rot.extend(requantize_row_uniform(&rotated, self.artifact.rope_clamp()));
            }
            probe.steps.push((li as u16, 8, q_rot_raw));
            probe.steps.push((li as u16, 9, q_rot.iter().map(|c| *c as i32).collect()));
            // The KEY heads are rotated over `n_kv_heads`, not `n_heads`: under grouped-query
            // attention there are fewer of them, and rotating `n_heads` of them would read past
            // the projection.
            let mut k_rot = Vec::with_capacity(kv);
            let mut k_rot_raw: Vec<i32> = Vec::with_capacity(kv);
            for head in 0..shape.n_kv_heads {
                let r = head * shape.d_head..(head + 1) * shape.d_head;
                let kh: Vec<i32> = k[r].iter().map(|c| *c as i32).collect();
                let rotated = rope_table(&kh, cos_row, sin_row)?;
                k_rot_raw.extend(rotated.iter().copied());
                k_rot.extend(requantize_row_uniform(&rotated, self.artifact.rope_clamp()));
            }
            probe.steps.push((li as u16, 10, k_rot_raw));
            probe.steps.push((li as u16, 11, k_rot.iter().map(|c| *c as i32).collect()));

            cache.keys[li].push(k_rot);
            cache.values[li].push(v);
            let history = cache.keys[li].len();

            let mut attn = vec![0i8; d];
            // Slots 12..=15 of the IR, accumulated head-major across the loop below.
            let mut per_head: [Vec<i32>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
            let mut weighted_raw: Vec<i32> = Vec::with_capacity(d);
            let group = shape.gqa_group();
            for head in 0..shape.n_heads {
                let off = head * shape.d_head;
                // Which kv head this query head reads. Contiguous grouping — query heads
                // `[g·group, (g+1)·group)` share kv head `g` — which is the layout every
                // Qwen2/LLaMA-family checkpoint stores; an interleaved reading would pair each
                // query with the wrong key and produce a model that is wrong everywhere without
                // being wrong anywhere in particular.
                let kv_off = (head / group) * shape.d_head;
                let qh = &q_rot[off..off + shape.d_head];

                // Logits: one DotI8 per key, then the amplification that makes softmax
                // discriminate. Without `rescale_row` here the distribution is uniform to four
                // decimals regardless of the keys — see ADR-0040 H.
                let raw: Vec<i32> = (0..history)
                    .map(|j| dot_i8(qh, &cache.keys[li][j][kv_off..kv_off + shape.d_head]))
                    .collect::<Result<_, _>>()?;
                let amplified = rescale_row(&raw, layer.attn_logit_scale);
                let probs = softmax(&amplified)?;
                probe.attention_spread.push(probs.iter().max().copied().unwrap_or(0) - probs.iter().min().copied().unwrap_or(0));
                // Narrowed so the value-weighted sum is an ordinary DotI8 rather than a
                // mixed-scale reduction with no op in the catalog.
                let p8 = requantize_row_uniform(&probs, self.artifact.qk_to_code());
                // **The four per-head steps, captured** (ADR-0049 Decision F, audit C-01/C-05).
                //
                // The engine runs these once per QUERY HEAD and the probe recorded none of them,
                // so no step leg could carry the arithmetic attention actually happens in — the
                // capture went straight from the rotated key to the output projection. They are
                // appended head-major into one row per slot, which is exactly the `KvPerHead`
                // width the IR declares: `attn_heads x kv_len`, one tile index space over the
                // concatenation.
                per_head[0].extend(raw.iter().copied());
                per_head[1].extend(amplified.iter().copied());
                per_head[2].extend(probs.iter().copied());
                per_head[3].extend(p8.iter().map(|c| *c as i32));

                for i in 0..shape.d_head {
                    let column: Vec<i8> = (0..history).map(|j| cache.values[li][j][kv_off + i]).collect();
                    let weighted = dot_i8(&p8, &column)?;
                    attn[off + i] = requantize_row_uniform(&[weighted], self.artifact.code_product())[0];
                }
                weighted_raw.extend((0..shape.d_head).map(|i| {
                    let column: Vec<i8> = (0..history).map(|j| cache.values[li][j][kv_off + i]).collect();
                    dot_i8(&p8, &column).unwrap_or(0)
                }));
            }
            for (offset, row) in per_head.iter().enumerate() {
                probe.steps.push((li as u16, 12 + offset as u16, row.clone()));
            }
            probe.steps.push((li as u16, 16, weighted_raw));
            probe.steps.push((li as u16, 17, attn.iter().map(|c| *c as i32).collect()));

            let projected_acc = matmul_quant(&layer.wo, &attn, d)?;
            probe.steps.push((li as u16, 18, projected_acc.clone()));
            let projected = requantize_row_uniform(&projected_acc, layer.requant[3]);
            probe.steps.push((li as u16, 19, projected.iter().map(|c| *c as i32).collect()));
            let residual = add_elem(&h, &projected)?;
            probe.steps.push((li as u16, 20, residual.clone()));
            // **ADR-0050 Decision B: the residual may amplify.** A requantization can only reduce,
            // so a stream that has decayed has nothing to be given — measured on the real
            // checkpoint at 5 of 127, with every layer already at `shift = 0`. Unity when the
            // artifact declares no gain, which is what BASE-0's own class is: the arithmetic does
            // not move until a calibration sets one.
            let lifted = rescale_row(&residual, self.artifact.residual_scale_at(li, 0));
            probe.steps.push((li as u16, 21, lifted.clone()));
            h = requantize_row_uniform(&lifted, self.artifact.residual_requant_at(li, 0));
            probe.steps.push((li as u16, 22, h.iter().map(|c| *c as i32).collect()));

            // ---- SwiGLU feed-forward --------------------------------------------------------
            let ffn_norm_q = rms_norm(&h, self.artifact.shape.eps_q)?;
            probe.steps.push((li as u16, 23, ffn_norm_q.clone()));
            let normed = requantize_row_uniform(&ffn_norm_q, self.artifact.norm_requant);
            probe.steps.push((li as u16, 24, normed.iter().map(|c| *c as i32).collect()));
            let gate_acc = matmul_quant(&layer.w_gate, &normed, shape.d_ff)?;
            probe.steps.push((li as u16, 25, gate_acc.clone()));
            let gate_q = rescale_row(&gate_acc, layer.ffn_gate_scale);
            probe.steps.push((li as u16, 26, gate_q.clone()));
            let activated = silu(&gate_q);
            probe.steps.push((li as u16, 27, activated.clone()));
            let gate = requantize_row_uniform(&activated, self.artifact.qk_to_code());
            probe.steps.push((li as u16, 28, gate.iter().map(|c| *c as i32).collect()));
            probe.gate_extremes.push((
                gate.iter().map(|c| *c as i32).min().unwrap_or(0),
                gate.iter().map(|c| *c as i32).max().unwrap_or(0),
            ));
            let up_acc = matmul_quant(&layer.w_up, &normed, shape.d_ff)?;
            probe.steps.push((li as u16, 29, up_acc.clone()));
            let up = requantize_row_uniform(&up_acc, layer.requant[5]);
            probe.steps.push((li as u16, 30, up.iter().map(|c| *c as i32).collect()));
            let product = mul_elem(&gate, &up)?;
            probe.steps.push((li as u16, 31, product.clone()));
            let gated = requantize_row_uniform(&product, self.artifact.code_product());
            probe.steps.push((li as u16, 32, gated.iter().map(|c| *c as i32).collect()));
            let down_acc = matmul_quant(&layer.w_down, &gated, d)?;
            probe.steps.push((li as u16, 33, down_acc.clone()));
            let down = requantize_row_uniform(&down_acc, layer.requant[6]);
            probe.steps.push((li as u16, 34, down.iter().map(|c| *c as i32).collect()));
            let ffn_residual = add_elem(&h, &down)?;
            probe.steps.push((li as u16, 35, ffn_residual.clone()));
            let ffn_lifted = rescale_row(&ffn_residual, self.artifact.residual_scale_at(li, 1));
            probe.steps.push((li as u16, 36, ffn_lifted.clone()));
            h = requantize_row_uniform(&ffn_lifted, self.artifact.residual_requant_at(li, 1));
            probe.steps.push((li as u16, 37, h.iter().map(|c| *c as i32).collect()));
            probe.residual_peak.push(h.iter().map(|c| (*c as i32).abs()).max().unwrap_or(0));
        }

        // Inlined rather than `norm_to_code`, because the post table declares the norm and its
        // narrowing as TWO nodes and a court recomputing the head opens them separately: a capture
        // that only kept the pair's output could not answer for either.
        let final_norm_q = rms_norm(&h, self.artifact.shape.eps_q)?;
        probe.post_steps.push((0, final_norm_q.clone()));
        let final_state = requantize_row_uniform(&final_norm_q, self.artifact.norm_requant);
        probe.post_steps.push((1, final_state.iter().map(|c| *c as i32).collect()));
        let logits = matmul_quant(&self.artifact.unembed, &final_state, shape.vocab)?;
        probe.post_steps.push((2, logits.clone()));
        Ok((logits, probe))
    }

    /// `RmsNorm` followed by the narrowing back to activation codes. `rms_norm` returns Qk, so the
    /// narrowing is not optional bookkeeping — it is what makes the result a `MatMulQuant` operand.
    fn norm_to_code(&self, h: &[i8]) -> Result<Vec<i8>, EngineError> {
        Ok(requantize_row_uniform(&rms_norm(h, self.artifact.shape.eps_q)?, self.artifact.norm_requant))
    }

    /// Greedy decode: `prompt` is consumed, then `new_tokens` are generated by taking the argmax
    /// of the logits each step.
    ///
    /// Ties break to the LOWEST token id. An argmax that broke ties by iteration order would make
    /// the output depend on how the logits happened to be laid out, which is exactly the kind of
    /// unstated tie-break that ADR-0030's court cannot adjudicate.
    pub fn generate(&self, prompt: &[usize], new_tokens: usize) -> Result<Vec<usize>, EngineError> {
        let mut cache = KvCache::new(self.artifact);
        let mut out = Vec::with_capacity(new_tokens);
        let mut position = 0usize;
        let mut token = *prompt.first().unwrap_or(&0);
        for step in 0..prompt.len() + new_tokens {
            let logits = self.forward_token(&mut cache, token, position)?;
            position += 1;
            let next = argmax_lowest(&logits);
            if step + 1 < prompt.len() {
                token = prompt[step + 1];
            } else {
                token = next;
                out.push(next);
                if out.len() == new_tokens {
                    break;
                }
            }
        }
        Ok(out)
    }
}

/// Argmax with ties broken to the lowest index. Separate and named so the tie rule is a thing that
/// can be pointed at rather than an accident of `max_by_key`.
pub fn argmax_lowest(values: &[i32]) -> usize {
    let mut best = 0usize;
    for (i, v) in values.iter().enumerate() {
        if *v > values[best] {
            best = i;
        }
    }
    best
}

/// Re-exported so a caller can name the op set the engine is restricted to without depending on
/// `consensus-core` directly.
pub use ops::PalwBase0OpError as OpError;

/// The scale parameters an artifact must carry for the engine's two amplification points.
pub type EngineScale = ScaleParams;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};

    fn shape() -> Base0ShapeV1 {
        Base0ShapeV1 {
            n_layers: 2,
            n_heads: 2,
            // Multi-head: the pre-GQA meaning, so this fixture is BASE-0 exactly as it was.
            n_kv_heads: 2,
            d_head: 8,
            d_ff: 32,
            vocab: 16,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1 << 8,
        }
    }

    fn artifact(seed: u64) -> Base0ArtifactV1 {
        Base0ArtifactV1::derive_deterministic(shape(), seed).unwrap()
    }

    /// The property the whole class exists for: the same input produces the identical trace on
    /// every run. Anything less and there is nothing for the court to adjudicate.
    #[test]
    fn inference_is_deterministic() {
        let a = artifact(11);
        let engine = Base0Engine::new(&a);
        let first = engine.generate(&[1, 2, 3], 5).unwrap();
        for _ in 0..4 {
            assert_eq!(engine.generate(&[1, 2, 3], 5).unwrap(), first);
        }
        // And a second, independently derived artifact of the same seed agrees with the first.
        let b = artifact(11);
        assert_eq!(Base0Engine::new(&b).generate(&[1, 2, 3], 5).unwrap(), first);
    }

    /// Different weights must produce different behaviour. A forward pass that had degenerated
    /// into a constant — which is exactly what the flat-attention defect would produce — passes
    /// the determinism test above and fails this one.
    #[test]
    fn different_artifacts_produce_different_output() {
        let outputs: Vec<Vec<i32>> = (0..6)
            .map(|seed| {
                let a = artifact(seed * 7 + 1);
                let engine = Base0Engine::new(&a);
                let mut cache = KvCache::new(&a);
                engine.forward_token(&mut cache, 3, 0).unwrap()
            })
            .collect();
        for i in 1..outputs.len() {
            assert_ne!(outputs[0], outputs[i], "artifacts {} and {i} computed the same logits", 0);
        }
    }

    /// The logits must depend on the token. If `EmbedLookup` were ignored, or the residual stream
    /// were being flattened by the norm, every token would produce the same row.
    #[test]
    fn the_logits_depend_on_the_input_token() {
        let a = artifact(21);
        let engine = Base0Engine::new(&a);
        let rows: Vec<Vec<i32>> = (0..6)
            .map(|t| {
                let mut cache = KvCache::new(&a);
                engine.forward_token(&mut cache, t, 0).unwrap()
            })
            .collect();
        for i in 1..rows.len() {
            assert_ne!(rows[0], rows[i], "tokens 0 and {i} produced identical logits");
        }
    }

    /// Attention must actually attend: the logits at position 1 must depend on what was at
    /// position 0. This is the end-to-end form of the ADR-0040 H defect — with flat attention the
    /// history is averaged uniformly and a changed prefix moves the output by almost nothing.
    #[test]
    fn history_changes_the_next_position() {
        let a = artifact(31);
        let engine = Base0Engine::new(&a);
        let with_prefix = |first: usize| {
            let mut cache = KvCache::new(&a);
            engine.forward_token(&mut cache, first, 0).unwrap();
            engine.forward_token(&mut cache, 5, 1).unwrap()
        };
        let x = with_prefix(1);
        let y = with_prefix(9);
        assert_ne!(x, y, "the prefix did not reach position 1 — attention is not attending");
    }

    /// Position must matter independently of content: the same token at position 0 and at
    /// position 1 (after an identical token) must not produce identical logits, or RoPE is inert.
    #[test]
    fn position_changes_the_result() {
        let a = artifact(41);
        let engine = Base0Engine::new(&a);
        let mut cache = KvCache::new(&a);
        let at0 = engine.forward_token(&mut cache, 4, 0).unwrap();
        let at1 = engine.forward_token(&mut cache, 4, 1).unwrap();
        assert_ne!(at0, at1, "the same token at two positions produced identical logits");
    }

    /// The cache is append-only and bound to its artifact. Both refusals matter: a skipped
    /// position would leave attention reading a stale row, and a foreign cache would mix two
    /// models' activations while still producing plausible numbers.
    #[test]
    fn the_cache_refuses_gaps_and_foreign_artifacts() {
        let a = artifact(51);
        let engine = Base0Engine::new(&a);
        let mut cache = KvCache::new(&a);
        assert_eq!(
            engine.forward_token(&mut cache, 0, 1),
            Err(EngineError::PositionOutOfRange { got: 1, max: 32 }),
            "a gap must be refused"
        );
        engine.forward_token(&mut cache, 0, 0).unwrap();
        assert!(engine.forward_token(&mut cache, 0, 0).is_err(), "an overwrite must be refused");

        let other = artifact(52);
        let mut foreign = KvCache::new(&other);
        assert_eq!(engine.forward_token(&mut foreign, 0, 0), Err(EngineError::CacheShapeMismatch));
    }

    /// Past `max_position` there is no rotary row, so the pass is refused rather than reusing one.
    #[test]
    fn running_past_max_position_is_refused() {
        let a = Base0ArtifactV1::derive_deterministic(Base0ShapeV1 { max_position: 3, ..shape() }, 61).unwrap();
        let engine = Base0Engine::new(&a);
        let mut cache = KvCache::new(&a);
        for p in 0..3 {
            engine.forward_token(&mut cache, 1, p).unwrap();
        }
        assert_eq!(engine.forward_token(&mut cache, 1, 3), Err(EngineError::PositionOutOfRange { got: 3, max: 3 }));
    }

    /// An out-of-range token is an op error, not a panic. A panic here would be reachable from a
    /// peer-supplied job and would be a remote halt.
    #[test]
    fn an_out_of_range_token_is_an_error_not_a_panic() {
        let a = artifact(71);
        let engine = Base0Engine::new(&a);
        let mut cache = KvCache::new(&a);
        assert_eq!(
            engine.forward_token(&mut cache, 999, 0),
            Err(EngineError::Op(PalwBase0OpError::TokenOutOfRange { got: 999, rows: 16 }))
        );
    }

    /// Ties go to the lowest id, always. Left unstated this is the kind of detail two
    /// implementations settle differently and the court cannot decide between.
    #[test]
    fn argmax_breaks_ties_to_the_lowest_id() {
        assert_eq!(argmax_lowest(&[5, 5, 5]), 0);
        assert_eq!(argmax_lowest(&[1, 9, 9, 2]), 1);
        assert_eq!(argmax_lowest(&[-3, -3]), 0);
        assert_eq!(argmax_lowest(&[i32::MIN, i32::MIN, 0]), 2);
    }

    /// The forward pass must actually compute, not merely run. Both quantities are measured
    /// because both have a degenerate mode that every other test in this file survives: a
    /// collapsed residual stream still produces stable, weight-dependent logits, and a uniform
    /// softmax still attends to something.
    #[test]
    fn the_pass_is_not_degenerate() {
        let a = artifact(101);
        let engine = Base0Engine::new(&a);
        let mut cache = KvCache::new(&a);
        for p in 0..4 {
            engine.forward_token(&mut cache, p + 1, p).unwrap();
        }
        let (_, probe) = engine.forward_token_probed(&mut cache, 2, 4).unwrap();

        assert!(
            probe.residual_peak.iter().all(|p| *p > 8),
            "the residual stream collapsed — every projection downstream is reading near-zeros: {:?}",
            probe.residual_peak
        );
        for (i, (lo, hi)) in probe.gate_extremes.iter().enumerate() {
            assert!(*hi > 0, "layer {i}'s SwiGLU gate produced nothing positive: {lo}..{hi}");
            // SiLU floors at −0.278 and passes positives through, so a working gate is markedly
            // asymmetric. `x/2` — what SiLU degenerates to when fed below its Qk domain — is
            // symmetric, so this is the assertion that separates the two.
            assert!(
                lo.abs() * 2 < *hi,
                "layer {i}'s gate is symmetric ({lo}..{hi}) — SiLU has degenerated to a linear x/2"
            );
        }
        let uniform = ForwardProbe::uniform_probability(5);
        assert!(
            probe.attention_spread.iter().any(|s| *s > uniform / 2),
            "no head is selecting; the widest spread was {:?} against a uniform probability of {uniform}",
            probe.attention_spread.iter().max()
        );
    }

    /// ADR-0040 H, pinned end to end.
    ///
    /// The mutation is exactly the state the class was in before `rescale_q` existed: unity gain
    /// is the STRONGEST thing `QuantParams` could ever have expressed, since `SRDHM` bakes in a
    /// `>> 31`. Under it the softmax must go flat. If this test ever passes without the assertion
    /// on `flat` failing first, the amplification has stopped being load-bearing and Decision H
    /// should be re-examined rather than the test relaxed.
    #[test]
    fn the_amplification_is_load_bearing() {
        let good = artifact(111);
        let mut flat = good.clone();
        for l in flat.layers.iter_mut() {
            l.attn_logit_scale = ScaleParams { multiplier: i32::MAX, shift: ScaleParams::UNITY_SHIFT };
            l.ffn_gate_scale = ScaleParams { multiplier: i32::MAX, shift: ScaleParams::UNITY_SHIFT };
        }

        let run = |a: &Base0ArtifactV1| {
            let engine = Base0Engine::new(a);
            let mut cache = KvCache::new(a);
            for p in 0..4 {
                engine.forward_token(&mut cache, p + 1, p).unwrap();
            }
            engine.forward_token_probed(&mut cache, 2, 4).unwrap().1
        };

        let uniform = ForwardProbe::uniform_probability(5);
        let calibrated = run(&good);
        let degenerate = run(&flat);

        let best_calibrated = *calibrated.attention_spread.iter().max().unwrap();
        let worst_degenerate = *degenerate.attention_spread.iter().max().unwrap();
        assert!(best_calibrated > uniform / 2, "the calibrated artifact should select: {best_calibrated} vs {uniform}");
        assert!(
            worst_degenerate * 50 < uniform,
            "at unity gain every head must be indistinguishable from uniform; the widest was {worst_degenerate} \
             against {uniform}. If this now discriminates, the accumulator scale has changed and the ADR-0040 H \
             argument needs re-measuring."
        );
        // The gate degenerates in the same breath, and by the same cause. MEASURED: the working
        // gate is −36..127 (|min|/max = 0.28, SiLU's own floor); at unity gain it is symmetric,
        // which is the signature of `x · 0.5`.
        for (i, ((glo, ghi), (flo, fhi))) in calibrated.gate_extremes.iter().zip(&degenerate.gate_extremes).enumerate() {
            assert!(glo.abs() * 2 < *ghi, "layer {i}: the calibrated gate should be asymmetric ({glo}..{ghi})");
            assert!(flo.abs() * 2 >= *fhi, "layer {i}: at unity gain the gate should be symmetric ({flo}..{fhi})");
        }

        // And the class id must separate the two, or an executor could retune the model in place.
        assert_ne!(good.artifact_digest(), flat.artifact_digest());
    }

    /// The same final token over the same prefix *content* in a different ORDER must give
    /// different logits.
    ///
    /// This does **not** isolate `RopeTable`, though the first draft of it claimed to. The
    /// argument was that softmax-then-weighted-sum is permutation invariant, so only the rotation
    /// could break the symmetry — but the causal mask breaks it first: the token at position 0
    /// attends only to itself while the token at position 1 attends to both, so swapping them
    /// changes each one's own key and value before attention at position 2 ever runs. Verified by
    /// mutation: with `rope_table` bypassed on both `q` and `k` this test still passes.
    ///
    /// What actually pins the rotation is `rope`'s own tests for the table and
    /// `the_engine_matches_its_golden_trace` for the engine's use of it — an off-by-one in the
    /// row index and a fully inert rotation both land there.
    #[test]
    fn the_prefix_order_changes_the_result() {
        let a = artifact(121);
        let engine = Base0Engine::new(&a);
        let run = |prefix: [usize; 2]| {
            let mut cache = KvCache::new(&a);
            engine.forward_token(&mut cache, prefix[0], 0).unwrap();
            engine.forward_token(&mut cache, prefix[1], 1).unwrap();
            engine.forward_token(&mut cache, 7, 2).unwrap()
        };
        assert_ne!(run([2, 11]), run([11, 2]), "the prefix order did not reach the final position");
    }

    /// The frozen numbers. An `ExecutionClass` is a promise that a given artifact and input
    /// produce a given output *forever*: the class id pins the weights, and this pins the engine
    /// that reads them. Any change to the op order, the scale bookkeeping, the residual structure
    /// or a primitive lands here, including the ones the semantic tests above cannot see — a
    /// dropped residual connection passes every other test in this file and fails this one.
    ///
    /// If this test fails, the question is not how to update the numbers. It is whether the change
    /// was meant to redefine BASE-0, which requires a new class id and a new registration, because
    /// every block already mined under the old one claimed the old numbers.
    ///
    /// The CLASS ID has since moved a second time, while these numbers did not. `digest_bytes`
    /// gained a length prefix for `cos_q`/`sin_q` so a table with mismatched halves can no longer
    /// alias a well-formed shorter one (audit 2.4). That changes what the artifact HASHES to
    /// without changing what the engine COMPUTES — all four rows below are byte-identical across
    /// that change, which is exactly the evidence that the digest fix was confined to the digest.
    ///
    /// These numbers HAVE been reset once, and the precedent should be read narrowly. The
    /// ADR-0040 C1/C2 repair — `RoundingShiftRight` was not round-half-away-from-zero and `SRDHM`
    /// disagreed with gemmlowp on half its inputs — moved every negative activation by a unit, so
    /// the trace moved with it. That was allowed because BASE-0 is registered nowhere and no block
    /// has ever claimed these numbers. The class id did not change, which is exactly the situation
    /// that would have been unacceptable after registration: same id, different arithmetic.
    /// **Phase 1: a Qwen2.5-shaped layer executes deterministically.**
    ///
    /// Grouped-query attention is the structural difference from BASE-0 — every Qwen2.5 dense
    /// member has 2 kv heads against 12 or 16 query heads — and it is not a detail that can be
    /// folded away: with `n_kv_heads == n_heads` this engine runs multi-head attention, which is
    /// a different model.
    ///
    /// Weights are DERIVED from a seed rather than converted, because Phase 1 is about the
    /// execution and Phase 2 owns the artifact. What is asserted is what Phase 1 owes: the shape
    /// is accepted, the pass completes, and it is reproducible.
    #[test]
    fn a_qwen25_shaped_layer_executes_deterministically() {
        // Qwen2.5-1.5B's proportions at one layer and a small vocabulary — the head geometry is
        // the real thing (12 query heads over 2 kv heads, 128-wide), which is what GQA is about.
        let qwen = Base0ShapeV1 {
            n_layers: 1,
            n_heads: 12,
            n_kv_heads: 2,
            d_head: 128,
            d_ff: 8_960,
            vocab: 512,
            max_position: 64,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1 << 8,
        };
        assert_eq!(qwen.d_model(), 1_536, "the measured hidden width");
        assert_eq!(qwen.kv_dim(), 256, "and the measured kv width — not the hidden one");
        assert_eq!(qwen.gqa_group(), 6, "six query heads per kv head");
        qwen.validate().expect("a grouped-query shape is a valid shape");

        let a = Base0ArtifactV1::derive_deterministic(qwen, 20_260_821).unwrap();
        let engine = Base0Engine::new(&a);

        let run = || {
            let mut cache = KvCache::new(&a);
            let mut logits = Vec::new();
            for (position, token) in [7usize, 19, 3].iter().enumerate() {
                logits.push(engine.forward_token(&mut cache, *token, position).expect("the pass completes"));
            }
            logits
        };
        let first = run();
        assert_eq!(first.len(), 3);
        assert!(first.iter().all(|l| l.len() == qwen.vocab), "one logit per vocabulary entry");
        // Not degenerate: a pass that returned a constant row would "complete" and mean nothing.
        assert!(first.iter().all(|l| l.iter().any(|v| *v != l[0])), "the logits vary across the vocabulary");
        // Deterministic: the same inputs, the same bits. This is the property the whole class
        // rests on, and integer arithmetic is why it holds without pinning a reduction order.
        assert_eq!(run(), first, "two runs of one artifact must agree bit for bit");

        // The kv cache really is the narrow one — 256 wide, not 1536. A cache sized at the query
        // width would work arithmetically and silently be a different (much larger) model.
        let mut cache = KvCache::new(&a);
        engine.forward_token(&mut cache, 7, 0).unwrap();
        assert_eq!(cache.keys[0][0].len(), 256, "the cached key is one kv head set, not one query head set");
        assert_eq!(cache.values[0][0].len(), 256);
    }

    #[test]
    fn the_engine_matches_its_golden_trace() {
        let a = Base0ArtifactV1::derive_deterministic(shape(), 20_260_817).unwrap();
        assert_eq!(
            a.artifact_digest().to_string(),
            // Re-frozen 2026-08-21, six times: the shape digest gained `n_kv_heads`, the
            // artifact digest gained the requantization ZERO POINTS and the per-channel triples
            // (without which two artifacts whose every bias differs shared one class id), then the
            // TOKENIZER commitment (without which two classes computing different things from the
            // same ids shared one), then the per-layer residual narrowing, and now the three CLASS
            // NARROWINGS the engine used to hold as `const` (ADR-0049 F: a parameter the court
            // cannot open is a step it cannot adjudicate, and a `const` in this binary is exactly
            // that), and now the RESIDUAL GAINS (ADR-0050 B) — `None`, which is unity, so the
            // arithmetic does not move until a calibration sets one.
            //
            // The LOGITS below have not moved once across the six, which is the assertion that
            // matters: each was a renaming, not a new model. This one most of all — the narrowings
            // moved from a `const` to a field holding the same value, so a moved trace would have
            // meant the move changed the arithmetic.
            concat!(
                "59359ac199a78c23c5888a7229f0e31e24a773e318bd9313edc4d9c6db81bd5d",
                "e6a97dc89fbedb4244bf3f6e58864a5526b326d6897d0c6df7faab08770f8410"
            ),
            "the artifact itself changed, so the trace below is about a different model"
        );
        let engine = Base0Engine::new(&a);
        let mut cache = KvCache::new(&a);
        let golden: [[i32; 16]; 4] = [
            [-4813, 23680, 2567, 1711, -17100, -16931, -1634, -10283, -285, 5990, -772, 13827, -3332, 1043, 22085, 10572],
            [-2464, -2477, -4101, 11787, 7715, 10135, 5846, -9800, -10815, -6606, 11852, -1424, 13586, 11268, 9417, 740],
            [-10519, 21105, 1050, 12475, 9437, 29971, -989, -3329, 4319, 11861, 2239, 11824, 17851, 9288, 270, -5377],
            [-9868, -8608, -10523, 4689, 6480, 6731, -14468, -4733, 4236, -78, 6275, -7267, 11591, 5497, 12565, 20501],
        ];
        for (position, (token, want)) in [3usize, 9, 1, 14].iter().zip(golden.iter()).enumerate() {
            let got = engine.forward_token(&mut cache, *token, position).unwrap();
            assert_eq!(got, want.to_vec(), "the trace diverged at position {position}");
        }
    }

    /// The engine must run at a shape where the fan-in is large enough that a real model's
    /// accumulators behave, not only at the toy shape the other tests use.
    #[test]
    fn a_wider_shape_runs_end_to_end() {
        let wide = Base0ShapeV1 { n_layers: 1, n_heads: 4, d_head: 32, d_ff: 256, vocab: 64, max_position: 8, ..shape() };
        let a = Base0ArtifactV1::derive_deterministic(wide, 81).unwrap();
        let engine = Base0Engine::new(&a);
        let out = engine.generate(&[1, 2], 3).unwrap();
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|t| *t < 64));
    }
}




