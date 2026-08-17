//! The PALW-BASE-0 artifact: shape, weights, rotary table, and the digest that pins all three.
//!
//! # What an artifact is for
//!
//! ADR-0039 made BASE-0 the class that replaces the hash floor, and gave the reason: it is the
//! only class whose kernel catalog can *close*. A closed catalog is worth nothing on its own —
//! two nodes still need to agree on which weights the catalog was applied to. The artifact is
//! that agreement, and [`Base0ArtifactV1::execution_class_id`] is the single 64-byte value the
//! rest of the system quotes.
//!
//! # Everything that changes the output is inside the digest
//!
//! The digest covers the shape, the quantisation parameters, the rotary table, and every weight
//! byte. It deliberately does NOT cover anything else — no build date, no producer name, no
//! comment field — because a field inside the digest that does not change the output splits one
//! class into two that compute identically, and each split halves the panel that can be drawn to
//! audit either half.
//!
//! The converse is the failure this ordering is built to prevent: a field that changes the output
//! and is *not* in the digest gives two artifacts one id, and the court then cannot say which one
//! an executor ran. Adding a field to [`Base0ShapeV1`] without extending
//! [`Base0ShapeV1::digest_bytes`] is exactly that bug, and `shape_digest_covers_every_field`
//! is the test that fails when someone does it.
//!
//! # Weights are carried, not trained
//!
//! This crate defines the container and the engine that reads it. Producing weights that are
//! *good* — quantising a trained model into `i8` at these shapes — is a separate data pipeline
//! and is not in this crate. [`Base0ArtifactV1::from_parts`] accepts any weights of the right
//! shape, and [`Base0ArtifactV1::derive_deterministic`] fills them from a seeded integer sequence
//! so the engine, the digest, and the tests have something concrete to run on. A derived artifact
//! is a *test fixture*, and [`Base0ArtifactV1::is_derived`] says so, so it can never be mistaken
//! for a registered class in a log.

use kaspa_consensus_core::palw_base0_ops::{QuantParams, ScaleParams};
use kaspa_hashes::Hash64;

use crate::rope::{RopeGenError, RopeTableV1};

/// Domain separator for the artifact digest. Distinct from every block-commitment domain so an
/// artifact digest can never be replayed as a commitment or a challenge.
pub const PALW_BASE0_ARTIFACT_DOMAIN: &[u8] = b"MISAKA/PALW/BASE0/ARTIFACT/V1\0\0\0";

/// `ln 10000` at `rope::GEN_Q` — the conventional RoPE base, carried as the default.
pub const LN_THETA_10000_GEN_Q: i128 = 2_592_480_341_699_211;

/// A gain that lifts a `fan_in`-long `int8` dot product into the Qk band `SoftMax` and `Silu` are
/// defined on.
///
/// MEASURED against the fixture's own distributions: its weights come out with σ = 37 and its
/// activations sit near σ = 45, so an `n`-term dot has σ ≈ `√n · 37 · 45`, i.e. `2^(10.7 +
/// log2(n)/2)`. Target `2^22` — a quarter of Qk, leaving headroom before `rescale_q` saturates.
///
/// Used only by [`Base0ArtifactV1::derive_deterministic`]. A real artifact's scales come from
/// calibrating against its own activation statistics, which is what this stands in for.
fn amplify_for(fan_in: usize) -> ScaleParams {
    let ilog2 = (usize::BITS - 1 - fan_in.leading_zeros()) as i32;
    let bits = (12 - ilog2 / 2).clamp(-31, 31) as i8;
    ScaleParams::gain_pow2(bits).expect("the clamp keeps `bits` inside the representable range")
}

/// Why an artifact can be refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactError {
    /// A shape field is zero, or `d_model` is not `n_heads × d_head`.
    BadShape,
    /// A weight tensor's length does not match the shape that declares it.
    WeightLen { tensor: &'static str, want: usize, got: usize },
    /// The rotary table could not be generated for this shape.
    Rope(RopeGenError),
    /// A single dot product would exceed `palw_base0::MAX_DOT_LEN`, past which ADR-0040's
    /// free-reduction-order proof does not hold. Refused at construction because the alternative
    /// is an accumulator that silently overflows mid-inference.
    DotTooLong { got: usize },
}

impl From<RopeGenError> for ArtifactError {
    fn from(e: RopeGenError) -> Self {
        ArtifactError::Rope(e)
    }
}

/// The architecture. Every field here changes the output, so every field is in the digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Base0ShapeV1 {
    pub n_layers: usize,
    pub n_heads: usize,
    pub d_head: usize,
    pub d_ff: usize,
    pub vocab: usize,
    pub max_position: usize,
    /// `ln θ` at `rope::GEN_Q`, the rotary base. Carried rather than hardcoded so the artifact
    /// records which base its table was derived from.
    pub ln_theta_gen_q: i128,
    /// `ε` added inside the RMS norm, at Q`K`. Inside the digest because it moves every
    /// activation.
    pub eps_q: i64,
}

impl Base0ShapeV1 {
    pub fn d_model(&self) -> usize {
        self.n_heads * self.d_head
    }

    pub fn validate(&self) -> Result<(), ArtifactError> {
        if self.n_layers == 0
            || self.n_heads == 0
            || self.d_head == 0
            || self.d_ff == 0
            || self.vocab == 0
            || self.max_position == 0
            || !self.d_head.is_multiple_of(2)
        {
            return Err(ArtifactError::BadShape);
        }
        // Bound every dimension BEFORE any product is formed. `d_model()` is `n_heads · d_head` and
        // the weight lengths are `vocab · d_model` / `d_ff · d_model`; on a shape supplied as data
        // those multiplications overflow `usize` and wrap to a small number, which would then pass
        // the reduction bound below and mis-size every tensor check in `from_parts`. Refusing
        // absurd dimensions outright is cheaper than auditing each product (audit 2.4). No real
        // shape approaches this: MAX_DOT_LEN is 133_144 and every dimension must fit under it.
        let bound = kaspa_consensus_core::palw_base0::MAX_DOT_LEN;
        for got in [self.n_layers, self.n_heads, self.d_head, self.d_ff, self.vocab, self.max_position] {
            if got > bound {
                return Err(ArtifactError::DotTooLong { got });
            }
        }
        // The longest reduction in the graph. `d_ff` feeds the down-projection, `d_model` feeds
        // every other matmul, and attention reduces over `d_head`.
        let longest = self.d_model().max(self.d_ff);
        if longest > bound {
            return Err(ArtifactError::DotTooLong { got: longest });
        }
        Ok(())
    }

    /// Little-endian, fixed-width, in declaration order. Fixed width matters: a varint encoding
    /// would let two different shapes produce the same bytes at a field boundary.
    pub fn digest_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 * 6 + 16 + 8);
        for v in [self.n_layers, self.n_heads, self.d_head, self.d_ff, self.vocab, self.max_position] {
            out.extend_from_slice(&(v as u64).to_le_bytes());
        }
        out.extend_from_slice(&self.ln_theta_gen_q.to_le_bytes());
        out.extend_from_slice(&self.eps_q.to_le_bytes());
        out
    }
}

/// One transformer block's weights, row-major `[out][in]` `i8`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base0LayerWeightsV1 {
    pub wq: Vec<i8>,
    pub wk: Vec<i8>,
    pub wv: Vec<i8>,
    pub wo: Vec<i8>,
    /// SwiGLU gate projection, `[d_ff][d_model]`.
    pub w_gate: Vec<i8>,
    /// SwiGLU up projection, `[d_ff][d_model]`.
    pub w_up: Vec<i8>,
    /// Down projection, `[d_model][d_ff]`.
    pub w_down: Vec<i8>,
    /// Requantisation applied after each projection, in the order the engine applies them:
    /// q, k, v, o, gate, up, down. Index 4 is unused — the gate path amplifies through
    /// [`Base0LayerWeightsV1::ffn_gate_scale`] instead of narrowing — and is kept so the array
    /// index matches the projection index everywhere else.
    pub requant: [QuantParams; 7],
    /// Gain applied to the attention logits before `SoftMax`. **Amplifying**, so it cannot be a
    /// `QuantParams`: a `d_head`-long `DotI8` lands around 0.002 in Qk and `SoftMax` returns
    /// uniform there. ADR-0040 H.
    pub attn_logit_scale: ScaleParams,
    /// Gain applied to the SwiGLU gate pre-activation before `Silu`, for the same reason: at the
    /// accumulator's natural scale `IntSigmoid` returns 0.5 and the gate degenerates to `x/2`.
    pub ffn_gate_scale: ScaleParams,
}

/// The full artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base0ArtifactV1 {
    pub shape: Base0ShapeV1,
    /// `[vocab][d_model]`.
    pub embed: Vec<i8>,
    /// `[vocab][d_model]`, the output projection. Kept separate from `embed` rather than tied,
    /// so an artifact that ties them does so by carrying equal bytes and the digest still sees it.
    pub unembed: Vec<i8>,
    pub layers: Vec<Base0LayerWeightsV1>,
    pub rope: RopeTableV1,
    /// Requantisation for the embedding-normalisation step and for the final norm.
    pub norm_requant: QuantParams,
    /// Narrowing applied after each residual add. `AddElem` widens two `int8` codes to `i32`, so
    /// something must bring the stream back to `int8`; this is the parameter that says by how
    /// much, rather than leaving it to an implicit cast.
    pub residual_requant: QuantParams,
    derived_seed: Option<u64>,
}

impl Base0ArtifactV1 {
    /// Build from supplied weights. Every length is checked against the shape here, because the
    /// engine indexes with arithmetic that would otherwise read a plausible wrong row.
    pub fn from_parts(
        shape: Base0ShapeV1,
        embed: Vec<i8>,
        unembed: Vec<i8>,
        layers: Vec<Base0LayerWeightsV1>,
        norm_requant: QuantParams,
        residual_requant: QuantParams,
    ) -> Result<Self, ArtifactError> {
        shape.validate()?;
        let d = shape.d_model();
        let checks: [(&'static str, usize, usize); 2] =
            [("embed", shape.vocab * d, embed.len()), ("unembed", shape.vocab * d, unembed.len())];
        for (tensor, want, got) in checks {
            if want != got {
                return Err(ArtifactError::WeightLen { tensor, want, got });
            }
        }
        if layers.len() != shape.n_layers {
            return Err(ArtifactError::WeightLen { tensor: "layers", want: shape.n_layers, got: layers.len() });
        }
        for l in layers.iter() {
            let per: [(&'static str, usize, usize); 7] = [
                ("wq", d * d, l.wq.len()),
                ("wk", d * d, l.wk.len()),
                ("wv", d * d, l.wv.len()),
                ("wo", d * d, l.wo.len()),
                ("w_gate", shape.d_ff * d, l.w_gate.len()),
                ("w_up", shape.d_ff * d, l.w_up.len()),
                ("w_down", d * shape.d_ff, l.w_down.len()),
            ];
            for (tensor, want, got) in per {
                if want != got {
                    return Err(ArtifactError::WeightLen { tensor, want, got });
                }
            }
        }
        let rope = RopeTableV1::generate(shape.d_head, shape.max_position, shape.ln_theta_gen_q)?;
        Ok(Self { shape, embed, unembed, layers, rope, norm_requant, residual_requant, derived_seed: None })
    }

    /// A concrete artifact derived from `seed` alone, for exercising the engine and the digest.
    ///
    /// The sequence is a plain 64-bit LCG reduced to `i8`, chosen because it is trivially
    /// re-derivable in another language — a differential harness in Python or C can rebuild the
    /// same artifact without parsing a blob. Marked derived, and [`is_derived`] keeps it
    /// distinguishable from a registered class for the whole life of the value.
    pub fn derive_deterministic(shape: Base0ShapeV1, seed: u64) -> Result<Self, ArtifactError> {
        shape.validate()?;
        let d = shape.d_model();
        let mut state = seed | 1;
        let mut next = move || -> i8 {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            // Take a high byte: the low bits of an LCG have short periods.
            (((state >> 40) & 0xFF) as u8 as i8).saturating_abs().wrapping_sub(64)
        };
        let mut fill = |n: usize| -> Vec<i8> { (0..n).map(|_| next()).collect() };
        let embed = fill(shape.vocab * d);
        let unembed = fill(shape.vocab * d);
        // Calibrated on the TYPICAL accumulator, not the worst case. Worst-case calibration —
        // "d·127·127 must fit in an int8" — is the first thing this fixture did, and it drove
        // every projection to exactly zero: the worst case is √d times the typical magnitude, so
        // sizing for it discards the entire signal. The measured typical is `√n · 37 · 45`, and
        // this maps it back to the activations' own σ = 45.
        let shift_for = |n: usize| -> u8 { 5 + ((usize::BITS - 1 - n.leading_zeros()) / 2) as u8 };
        let layers = (0..shape.n_layers)
            .map(|_| Base0LayerWeightsV1 {
                wq: fill(d * d),
                wk: fill(d * d),
                wv: fill(d * d),
                wo: fill(d * d),
                w_gate: fill(shape.d_ff * d),
                w_up: fill(shape.d_ff * d),
                w_down: fill(d * shape.d_ff),
                requant: [
                    QuantParams { multiplier: i32::MAX, shift: shift_for(d) },
                    QuantParams { multiplier: i32::MAX, shift: shift_for(d) },
                    QuantParams { multiplier: i32::MAX, shift: shift_for(d) },
                    QuantParams { multiplier: i32::MAX, shift: shift_for(d) },
                    QuantParams { multiplier: i32::MAX, shift: shift_for(d) },
                    QuantParams { multiplier: i32::MAX, shift: shift_for(d) },
                    QuantParams { multiplier: i32::MAX, shift: shift_for(shape.d_ff) },
                ],
                attn_logit_scale: amplify_for(shape.d_head),
                ffn_gate_scale: amplify_for(d),
            })
            .collect();
        let mut artifact = Self::from_parts(
            shape,
            embed,
            unembed,
            layers,
            // Qk → an activation code: 1.0 must land on 127, not on 1.
            QuantParams { multiplier: i32::MAX, shift: (kaspa_consensus_core::palw_base0::K as u8) - 7 },
            // Halve on each residual add, so two `int8` codes summed into `i32` come back to
            // `int8` without saturating — the standard int8-residual convention.
            QuantParams { multiplier: i32::MAX, shift: 1 },
        )?;
        artifact.derived_seed = Some(seed);
        Ok(artifact)
    }

    /// True when the weights came from [`derive_deterministic`] rather than from a real model.
    /// Load-bearing: a derived artifact must never be reported as a registered class.
    pub fn is_derived(&self) -> bool {
        self.derived_seed.is_some()
    }

    pub fn derived_seed(&self) -> Option<u64> {
        self.derived_seed
    }

    /// The class id: a 64-byte digest over shape, weights, quantisation, and the rotary table.
    ///
    /// `derived_seed` is NOT covered, and that is the correct side of the rule stated in the
    /// module docs: it does not change any output, so covering it would split one computed class
    /// into two.
    pub fn execution_class_id(&self) -> Hash64 {
        let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_BASE0_ARTIFACT_DOMAIN).to_state();
        state.update(&self.shape.digest_bytes());
        state.update(&self.rope.digest_bytes());
        absorb_tensor(&mut state, b"embed", &self.embed);
        absorb_tensor(&mut state, b"unembed", &self.unembed);
        absorb_scale(&mut state, self.norm_requant.multiplier, self.norm_requant.shift);
        absorb_scale(&mut state, self.residual_requant.multiplier, self.residual_requant.shift);
        for (i, l) in self.layers.iter().enumerate() {
            state.update(&(i as u64).to_le_bytes());
            for (label, w) in [
                (b"wq".as_slice(), &l.wq),
                (b"wk".as_slice(), &l.wk),
                (b"wv".as_slice(), &l.wv),
                (b"wo".as_slice(), &l.wo),
                (b"w_gate".as_slice(), &l.w_gate),
                (b"w_up".as_slice(), &l.w_up),
                (b"w_down".as_slice(), &l.w_down),
            ] {
                absorb_tensor(&mut state, label, w);
            }
            for p in l.requant.iter() {
                absorb_scale(&mut state, p.multiplier, p.shift);
            }
            absorb_scale(&mut state, l.attn_logit_scale.multiplier, l.attn_logit_scale.shift);
            absorb_scale(&mut state, l.ffn_gate_scale.multiplier, l.ffn_gate_scale.shift);
        }
        let out = state.finalize();
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(out.as_bytes());
        Hash64::from_bytes(bytes)
    }
}

/// One tensor into the digest: label, then length, then bytes.
///
/// Injectivity comes from the **fixed field order plus the length prefix**: with both, no two
/// distinct artifacts can produce the same stream, and swapping two equal-length tensors already
/// reorders the bytes. The labels do not add to that — they make the stream self-describing, so a
/// future reordering of the fields is visible in the encoding rather than only in this function's
/// control flow. Verified by mutation: removing the labels leaves every digest test passing, which
/// is the honest status of the field and the reason this comment does not claim more.
fn absorb_tensor(state: &mut blake2b_simd::State, label: &[u8], w: &[i8]) {
    state.update(label);
    state.update(&(w.len() as u64).to_le_bytes());
    // `i8` and `u8` share a layout; the digest only needs the bytes to be a function of the values.
    let bytes: &[u8] = unsafe { std::slice::from_raw_parts(w.as_ptr() as *const u8, w.len()) };
    state.update(bytes);
}

/// One `(multiplier, shift)` pair into the digest. Shared by [`QuantParams`] and [`ScaleParams`]
/// because both are the same two numbers; keeping one absorber means a new scale field cannot be
/// added with a subtly different encoding.
fn absorb_scale(state: &mut blake2b_simd::State, multiplier: i32, shift: u8) {
    state.update(&multiplier.to_le_bytes());
    state.update(&[shift]);
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn tiny() -> Base0ShapeV1 {
        Base0ShapeV1 {
            n_layers: 2,
            n_heads: 2,
            d_head: 8,
            d_ff: 32,
            vocab: 16,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1 << 8,
        }
    }

    /// The digest is a pure function of the artifact: two derivations of the same seed agree.
    #[test]
    fn the_class_id_is_reproducible() {
        let a = Base0ArtifactV1::derive_deterministic(tiny(), 7).unwrap();
        let b = Base0ArtifactV1::derive_deterministic(tiny(), 7).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.execution_class_id(), b.execution_class_id());
    }

    /// **Every** shape field must move the class id. This is the test the module docs name: it
    /// fails when a field is added to the shape and not added to `digest_bytes`, which is the bug
    /// that would give two differently-computing artifacts one id.
    #[test]
    fn shape_digest_covers_every_field() {
        let base = tiny();
        let baseline = Base0ArtifactV1::derive_deterministic(base, 1).unwrap().execution_class_id();
        let mutations: [(&str, Base0ShapeV1); 8] = [
            ("n_layers", Base0ShapeV1 { n_layers: 3, ..base }),
            ("n_heads", Base0ShapeV1 { n_heads: 4, ..base }),
            ("d_head", Base0ShapeV1 { d_head: 16, ..base }),
            ("d_ff", Base0ShapeV1 { d_ff: 64, ..base }),
            ("vocab", Base0ShapeV1 { vocab: 17, ..base }),
            ("max_position", Base0ShapeV1 { max_position: 33, ..base }),
            ("ln_theta_gen_q", Base0ShapeV1 { ln_theta_gen_q: base.ln_theta_gen_q / 2, ..base }),
            ("eps_q", Base0ShapeV1 { eps_q: 1 << 9, ..base }),
        ];
        for (field, shape) in mutations {
            let id = Base0ArtifactV1::derive_deterministic(shape, 1).unwrap().execution_class_id();
            assert_ne!(baseline, id, "changing `{field}` did not change the class id");
        }
        // And the fixed-width encoding must not let two shapes collide at a field boundary.
        let a = Base0ShapeV1 { n_layers: 1, n_heads: 2, ..base };
        let b = Base0ShapeV1 { n_layers: 2, n_heads: 1, ..base };
        assert_ne!(a.digest_bytes(), b.digest_bytes());
    }

    /// A single weight byte must move the class id, in every tensor. A digest that skipped a
    /// tensor would let an executor swap it and still claim the class.
    #[test]
    fn every_tensor_is_inside_the_digest() {
        let base = Base0ArtifactV1::derive_deterministic(tiny(), 3).unwrap();
        let id = base.execution_class_id();
        let flip = |v: &Vec<i8>| -> Vec<i8> {
            let mut v = v.clone();
            v[0] = v[0].wrapping_add(1);
            v
        };
        let mut a = base.clone();
        a.embed = flip(&a.embed);
        assert_ne!(id, a.execution_class_id(), "embed is outside the digest");
        let mut a = base.clone();
        a.unembed = flip(&a.unembed);
        assert_ne!(id, a.execution_class_id(), "unembed is outside the digest");
        for li in 0..base.shape.n_layers {
            for (name, pick) in [
                ("wq", 0usize),
                ("wk", 1),
                ("wv", 2),
                ("wo", 3),
                ("w_gate", 4),
                ("w_up", 5),
                ("w_down", 6),
            ] {
                let mut a = base.clone();
                let l = &mut a.layers[li];
                let t = match pick {
                    0 => &mut l.wq,
                    1 => &mut l.wk,
                    2 => &mut l.wv,
                    3 => &mut l.wo,
                    4 => &mut l.w_gate,
                    5 => &mut l.w_up,
                    _ => &mut l.w_down,
                };
                t[0] = t[0].wrapping_add(1);
                assert_ne!(id, a.execution_class_id(), "layer {li} {name} is outside the digest");
                // And the requantisation, which changes the output without changing a weight.
                let mut a = base.clone();
                a.layers[li].requant[pick].shift += 1;
                assert_ne!(id, a.execution_class_id(), "layer {li} requant[{pick}] is outside the digest");
            }
        }
        let mut a = base.clone();
        a.norm_requant.multiplier -= 1;
        assert_ne!(id, a.execution_class_id(), "norm_requant is outside the digest");
        let mut a = base.clone();
        a.residual_requant.shift += 1;
        assert_ne!(id, a.execution_class_id(), "residual_requant is outside the digest");
        // The two amplifying scales are the parameters ADR-0040 H added. They move every logit
        // and every gate, so a digest that missed them would let an executor retune the model
        // while still claiming the class.
        for li in 0..base.shape.n_layers {
            let mut a = base.clone();
            a.layers[li].attn_logit_scale.shift += 1;
            assert_ne!(id, a.execution_class_id(), "layer {li} attn_logit_scale is outside the digest");
            let mut a = base.clone();
            a.layers[li].ffn_gate_scale.multiplier -= 1;
            assert_ne!(id, a.execution_class_id(), "layer {li} ffn_gate_scale is outside the digest");
        }
    }

    /// Swapping two tensors of equal length must change the id. `wq` and `wk` are the same length
    /// by construction, so this is the case where a digest that hashed a bare concatenation with
    /// no ordering guarantee would collide.
    #[test]
    fn equal_length_tensors_are_not_interchangeable() {
        let base = Base0ArtifactV1::derive_deterministic(tiny(), 5).unwrap();
        let mut swapped = base.clone();
        let l = &mut swapped.layers[0];
        std::mem::swap(&mut l.wq, &mut l.wk);
        assert_ne!(base.execution_class_id(), swapped.execution_class_id());
    }

    /// The derived marker is metadata, not an output: it must NOT be in the digest, or the same
    /// weights would carry two class ids depending on how they were obtained.
    #[test]
    fn the_derived_marker_is_outside_the_digest() {
        let derived = Base0ArtifactV1::derive_deterministic(tiny(), 9).unwrap();
        assert!(derived.is_derived());
        assert_eq!(derived.derived_seed(), Some(9));
        let carried = Base0ArtifactV1::from_parts(
            derived.shape,
            derived.embed.clone(),
            derived.unembed.clone(),
            derived.layers.clone(),
            derived.norm_requant,
            derived.residual_requant,
        )
        .unwrap();
        assert!(!carried.is_derived(), "weights supplied by hand are not derived");
        assert_eq!(derived.execution_class_id(), carried.execution_class_id());
    }

    /// A tensor of the wrong length is refused at construction rather than read past at inference.
    #[test]
    fn wrong_shaped_weights_are_refused() {
        let s = tiny();
        let d = s.d_model();
        let good = Base0ArtifactV1::derive_deterministic(s, 1).unwrap();
        let mk = |embed: Vec<i8>, layers: Vec<Base0LayerWeightsV1>| {
            Base0ArtifactV1::from_parts(s, embed, good.unembed.clone(), layers, good.norm_requant, good.residual_requant)
        };
        let err = mk(vec![0; 3], good.layers.clone());
        assert_eq!(err, Err(ArtifactError::WeightLen { tensor: "embed", want: s.vocab * d, got: 3 }));
        let mut short = good.layers.clone();
        short.pop();
        let err = mk(good.embed.clone(), short);
        assert_eq!(err, Err(ArtifactError::WeightLen { tensor: "layers", want: 2, got: 1 }));
        let mut bad = good.layers.clone();
        bad[0].wq.pop();
        let err = mk(good.embed.clone(), bad);
        assert_eq!(err, Err(ArtifactError::WeightLen { tensor: "wq", want: d * d, got: d * d - 1 }));
    }

    /// Shapes whose reductions run past `MAX_DOT_LEN` are refused, because past it ADR-0040's
    /// free-reduction-order proof — the thing that lets two implementations sum in any order —
    /// no longer holds.
    #[test]
    fn shapes_past_the_reduction_bound_are_refused() {
        let s = Base0ShapeV1 { d_ff: kaspa_consensus_core::palw_base0::MAX_DOT_LEN + 1, ..tiny() };
        assert_eq!(s.validate(), Err(ArtifactError::DotTooLong { got: kaspa_consensus_core::palw_base0::MAX_DOT_LEN + 1 }));
        assert!(Base0ShapeV1 { d_ff: kaspa_consensus_core::palw_base0::MAX_DOT_LEN, ..tiny() }.validate().is_ok());
        assert_eq!(Base0ShapeV1 { d_head: 7, ..tiny() }.validate(), Err(ArtifactError::BadShape));
        assert_eq!(Base0ShapeV1 { n_layers: 0, ..tiny() }.validate(), Err(ArtifactError::BadShape));
    }
}
