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
// **No kernel is called in this file** (ADR-0049 Decision F). Every arithmetic step of the forward
// pass is dispatched by `plan`, from `BASE0_LAYER_IR`; what is left here is the loop over layers,
// the cache's shape rules, and the probe.
use kaspa_consensus_core::palw_base0_ops::{self as ops, PalwBase0OpError, ScaleParams};
use kaspa_consensus_core::palw_state_chunk_map::{PalwStateChunkEntryV1, PalwStateChunkGeometryV1, PalwStateChunkKindV1};

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
    PositionOutOfRange {
        got: usize,
        max: usize,
    },
    /// The cache belongs to a different artifact than the one being run.
    CacheShapeMismatch,
    /// A checkpoint's state material does not cover the map. Refused rather than replayed over a
    /// partial cache: the uncovered rows would be zeros, and a zero row is indistinguishable from
    /// a computed one once a replay has run over it.
    CheckpointStateIncomplete {
        got: u64,
        want: u64,
    },
    /// One chunk is not the canonical bytes its map entry describes — wrong length, or a position
    /// the entry does not hold. Not adjudicable material, so not replayable material.
    CheckpointStateNotCanonical {
        chunk_index: u64,
    },
    /// **The class's graph cannot be executed by this binary** (ADR-0049 Decision F).
    ///
    /// The engine's op sequence is compiled from `BASE0_LAYER_IR` rather than written beside it, so
    /// a graph naming an operand this artifact does not carry, a kernel with no dispatch, or a step
    /// reading a step that has not run yet is refused HERE — before any arithmetic — instead of
    /// producing a row nobody can adjudicate.
    Plan(crate::plan::PlanError),
}

impl From<PalwBase0OpError> for EngineError {
    fn from(e: PalwBase0OpError) -> Self {
        EngineError::Op(e)
    }
}

impl From<crate::plan::PlanError> for EngineError {
    fn from(e: crate::plan::PlanError) -> Self {
        EngineError::Plan(e)
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

    /// How many positions one layer holds. Equal to [`Self::len`] for a cache that has only ever
    /// been advanced through [`Self::push_layer`], which is the only way a forward pass writes it.
    pub(crate) fn layer_len(&self, layer: usize) -> usize {
        self.keys.get(layer).map(|l| l.len()).unwrap_or(0)
    }

    /// One head's slice of a cached key.
    pub(crate) fn key_at(&self, layer: usize, position: usize, offset: usize, len: usize) -> &[i8] {
        &self.keys[layer][position][offset..offset + len]
    }

    /// One lane of a cached value.
    pub(crate) fn value_at(&self, layer: usize, position: usize, lane: usize) -> i8 {
        self.values[layer][position][lane]
    }

    /// **Append one position's key and value together.**
    ///
    /// Both halves or neither: a cache whose keys are one position longer than its values changes
    /// what every LATER position's attention reduces over, and nothing downstream would say so.
    pub(crate) fn push_layer(&mut self, layer: usize, key: Vec<i8>, value: Vec<i8>) {
        self.keys[layer].push(key);
        self.values[layer].push(value);
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// **Serialize one chunk of the registered state map.**
    ///
    /// The map (`palw_state_chunk_map`) says which `(kind, layer, position run)` a chunk index
    /// holds; this hands back exactly those bytes, in the map's order, one byte per `int8` code.
    /// It is the producer half of the checkpoint leg and the only place the cache's private shape
    /// is allowed to become bytes — a second serializer would be a second layout.
    ///
    /// `None` rather than a short buffer whenever the cache cannot answer the entry: a chunk of
    /// the wrong length is refused downstream anyway (`integer_kv_state_row_v1`), and a producer
    /// that emitted one would be committing to a state it did not hold.
    pub fn state_chunk_bytes(&self, entry: &PalwStateChunkEntryV1) -> Option<Vec<u8>> {
        let side = match entry.kind {
            PalwStateChunkKindV1::Key => &self.keys,
            PalwStateChunkKindV1::Value => &self.values,
        };
        let layer = side.get(entry.attn_layer as usize)?;
        let mut out = Vec::with_capacity(entry.byte_len() as usize);
        for p in entry.position_start..entry.position_start + entry.position_count {
            let row = layer.get(p as usize)?;
            // The map's row width is derived from the PROFILE; this is the ENGINE's row. They are
            // the same number for a conforming class, and a class where they differ is one whose
            // map describes a state it does not hold — which is exactly what must not be
            // committed to silently.
            if row.len() != entry.row_bytes as usize {
                return None;
            }
            out.extend(row.iter().map(|v| *v as u8));
        }
        Some(out)
    }

    /// **Rebuild a cache from an opened checkpoint** — the replay half.
    ///
    /// This is what makes a dispute cost the calls SINCE a checkpoint rather than the whole
    /// inference: a verifier hands over the chunks the producer committed and continues
    /// [`Base0Engine::forward_token`] from there.
    ///
    /// Every refusal is a refusal to replay, never a partial cache: a cache assembled from
    /// material that does not cover the state would replay against zeros, and zeros are
    /// indistinguishable from computed rows once they are in a commitment.
    pub fn from_state_chunks(
        artifact: &Base0ArtifactV1,
        geometry: &PalwStateChunkGeometryV1,
        chunks: &[Vec<u8>],
    ) -> Result<Self, EngineError> {
        if chunks.len() as u64 != geometry.chunk_count() {
            return Err(EngineError::CheckpointStateIncomplete { got: chunks.len() as u64, want: geometry.chunk_count() });
        }
        let mut cache = Self::new(artifact);
        let positions = geometry.positions as usize;
        for side in [&mut cache.keys, &mut cache.values] {
            for layer in side.iter_mut() {
                layer.resize(positions, Vec::new());
            }
        }
        for (index, bytes) in chunks.iter().enumerate() {
            let entry = kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_entry_v1(geometry, index as u64)
                .ok_or(EngineError::CheckpointStateIncomplete { got: chunks.len() as u64, want: geometry.chunk_count() })?;
            for p in entry.position_start..entry.position_start + entry.position_count {
                let row = kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_row_v1(&entry, bytes, p)
                    .ok_or(EngineError::CheckpointStateNotCanonical { chunk_index: index as u64 })?;
                let side = match entry.kind {
                    PalwStateChunkKindV1::Key => &mut cache.keys,
                    PalwStateChunkKindV1::Value => &mut cache.values,
                };
                let layer = side
                    .get_mut(entry.attn_layer as usize)
                    .ok_or(EngineError::CheckpointStateNotCanonical { chunk_index: index as u64 })?;
                layer[p as usize] = row.iter().map(|b| *b as i8).collect();
            }
        }
        // Every attention layer must now be full. A layer the map never named keeps its empty
        // rows, and replaying over those is the zero-state failure this refuses to reach.
        for side in [&cache.keys, &cache.values] {
            for layer in side.iter() {
                if layer.len() != positions || layer.iter().any(|r| r.len() != geometry.row_bytes as usize) {
                    return Err(EngineError::CheckpointStateIncomplete { got: 0, want: geometry.chunk_count() });
                }
            }
        }
        Ok(cache)
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
    /// **The layer graph, compiled from `BASE0_LAYER_IR` against this artifact** (ADR-0049 F).
    ///
    /// Compiled once per engine rather than per token: the questions it answers — does every
    /// tensor the graph names resolve, does every step read an earlier one, is every kernel
    /// dispatchable — are properties of the class, and a class this binary cannot execute should
    /// say so before the first forward pass rather than in the middle of one.
    plan: Result<crate::plan::Base0GraphPlanV1, crate::plan::PlanError>,
}

impl<'a> Base0Engine<'a> {
    pub fn new(artifact: &'a Base0ArtifactV1) -> Self {
        Self { artifact, artifact_digest: artifact.artifact_digest(), plan: crate::plan::Base0GraphPlanV1::compile(artifact) }
    }

    pub fn artifact(&self) -> &Base0ArtifactV1 {
        self.artifact
    }

    /// The compiled graph, or the reason this artifact's class cannot be executed.
    pub fn plan(&self) -> Result<&crate::plan::Base0GraphPlanV1, EngineError> {
        self.plan.as_ref().map_err(|e| EngineError::Plan(e.clone()))
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
        let plan = self.plan()?;
        let shape = &self.artifact.shape;
        let d = shape.d_model();
        if cache.artifact_digest != self.artifact_digest || cache.d_model != d || cache.keys.len() != shape.n_layers {
            return Err(EngineError::CacheShapeMismatch);
        }
        if position >= shape.max_position || position != cache.len() {
            return Err(EngineError::PositionOutOfRange { got: position, max: shape.max_position });
        }
        // The rotation resolves its own table through the operand binding; this stays because the
        // refusal must happen before any arithmetic, and a table shorter than the shape says is a
        // class that cannot run at this position at all.
        if self.artifact.rope.row(position).is_none() {
            return Err(EngineError::PositionOutOfRange { got: position, max: shape.max_position });
        }

        // The gather is a step of the graph like any other, and it is walked like one.
        let (embedded, pre_rows) = plan.pre.execute_graph(self.artifact, &[], Some(token_id), position)?;
        for (slot, row) in pre_rows.into_iter().enumerate() {
            probe.pre_steps.push((slot as u16, row));
        }
        let crate::plan::Base0RowV1::Codes(mut h) = embedded else {
            return Err(EngineError::Plan(crate::plan::PlanError::LayerOutputNotCodes { slot: 0 }));
        };

        // **The layer graph is WALKED, not written** (ADR-0049 Decision F).
        //
        // Every step below — which kernel, in what order, reading which earlier step, against which
        // operand of this artifact, producing how many values — comes from `BASE0_LAYER_IR`, the
        // same table `base0_profile_v1` projects the court's node table and the inventory's tensor
        // list from. The loop this replaced was a second description of that computation, and it
        // diverged from the first four times: eighteen of thirty-six steps declared, K never
        // rotated, the cache role on the raw projection, the per-head attention nodes declared once
        // per layer. Each was found by someone reading.
        for li in 0..shape.n_layers {
            let (next, trace) = plan.layer.execute_layer(self.artifact, li, &h, cache, position)?;
            let crate::plan::Base0LayerTraceV1 { rows, attention_spread, gate_extremes } = trace;
            for (slot, row) in rows.into_iter().enumerate() {
                probe.steps.push((li as u16, slot as u16, row));
            }
            probe.attention_spread.extend(attention_spread);
            if let Some(extremes) = gate_extremes {
                probe.gate_extremes.push(extremes);
            }
            probe.residual_peak.push(next.iter().map(|c| (*c as i32).abs()).max().unwrap_or(0));
            h = next;
        }

        // The head — the final norm, its narrowing, and the logits — walked from `BASE0_POST_IR`.
        // It used to be inlined here, three steps written beside a three-node table, and both
        // classes' tables declared the norm and not the narrowing after it for as long as that
        // lasted: a court recomputing the head would have compared a Qk value against a code.
        let (logits, post_rows) = plan.post.execute_graph(self.artifact, &h, None, position)?;
        for (slot, row) in post_rows.into_iter().enumerate() {
            probe.post_steps.push((slot as u16, row));
        }
        let crate::plan::Base0RowV1::Acc(logits) = logits else {
            return Err(EngineError::Plan(crate::plan::PlanError::LayerOutputNotCodes { slot: 2 }));
        };
        Ok((logits, probe))
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
        let mut token = *prompt.first().unwrap_or(&0);
        for step in 0..prompt.len() + new_tokens {
            // The position IS the step: it advanced once per iteration and never restarted, so a
            // separate counter was a second name for one number and a place for them to drift.
            let logits = self.forward_token(&mut cache, token, step)?;
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

/// Argmax with ties broken to the lowest index. The rule itself now lives with the COURT
/// (`base0_decode_token_select_v1`, ADR-0049 Decision E) and this delegates, so the engine that
/// selects a token and the adjudication that refutes one can never disagree about what
/// "selected" means.
pub fn argmax_lowest(values: &[i32]) -> usize {
    kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(values)
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
            assert!(lo.abs() * 2 < *hi, "layer {i}'s gate is symmetric ({lo}..{hi}) — SiLU has degenerated to a linear x/2");
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
