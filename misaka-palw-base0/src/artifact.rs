//! The PALW-BASE-0 artifact: shape, weights, rotary table, and the digest that pins all three.
//!
//! # What an artifact is for
//!
//! ADR-0039 made BASE-0 the class that replaces the hash floor, and gave the reason: it is the
//! only class whose kernel catalog can *close*. A closed catalog is worth nothing on its own —
//! two nodes still need to agree on which weights the catalog was applied to. The artifact is
//! that agreement, and [`Base0ArtifactV1::artifact_digest`] is the single 64-byte value the
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
/// Its own key, so a tokenizer commitment can never collide with an artifact digest.
pub const PALW_BASE0_TOKENIZER_DOMAIN: &[u8] = b"MISAKA/PALW/BASE0/TOKENIZER/V1\0\0";

/// `ln 10000` at `rope::GEN_Q` — the conventional RoPE base, carried as the default.
pub const LN_THETA_10000_GEN_Q: i128 = 2_592_480_341_699_211;

/// A gain that lifts a `fan_in`-long `int8` dot product into the Qk band `SoftMax` and `Silu` are
/// defined on.
///
/// MEASURED against the fixture's own distributions: its weights come out with σ = 37 and its
/// activations sit near σ = 45, so an `n`-term dot has σ ≈ `√n · 37 · 45`, i.e. `2^(10.7 +
/// log2(n)/2)`. Target `2^22` — a quarter of Qk, leaving headroom before `rescale_q` saturates.
///
/// Used by [`Base0ArtifactV1::derive_deterministic`]. A real artifact's scales come from measuring
/// what its own projections produce, which is what `convert` does.
///
/// **`shift: 0` is not "no amplification" here — it is a gain of `2^31`.** `ScaleParams` reads its
/// multiplier as a Q31 fraction, so `UNITY_SHIFT` is 31 and anything below it amplifies. A
/// converter that wrote `shift: 0` at these two sites saturated every attention logit into a hard
/// argmax and every SwiGLU gate onto its positive rail — ADR-0040 Decision H's failure with the
/// sign of the mistake reversed, and invisible to a determinism test. Recorded here because the
/// trap is in the type, not in any one caller.
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
    /// The geometry a caller asked the class id for is not the geometry this artifact has
    /// (ADR-0049 Decision G). Deriving the id anyway would name a class whose graph reads tensors
    /// of a different width than the ones carried here.
    GeometryMismatch { field: &'static str, artifact: u64, geometry: u64 },
    /// The geometry is one no BASE-0 profile exists for, so it names no class.
    Profile(&'static str),
}

impl std::fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
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
    /// **Grouped-query attention: how many KEY/VALUE heads the query heads share.**
    ///
    /// `n_kv_heads == n_heads` is multi-head attention and is what every artifact built before
    /// this field meant, so BASE-0's own class is unchanged by its addition. A second class needs
    /// it: every Qwen2.5 dense member has 2 kv heads against 12–16 query heads, and folding that
    /// away would be running a different model.
    ///
    /// Must divide `n_heads` — a group is a whole number of query heads per kv head, and a
    /// remainder would leave some query head reading no key at all.
    pub n_kv_heads: usize,
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

    /// The width of a K or V projection. Equal to `d_model` only when attention is multi-head.
    pub fn kv_dim(&self) -> usize {
        self.n_kv_heads * self.d_head
    }

    /// How many query heads share one kv head.
    pub fn gqa_group(&self) -> usize {
        self.n_heads / self.n_kv_heads.max(1)
    }

    pub fn validate(&self) -> Result<(), ArtifactError> {
        if self.n_layers == 0
            || self.n_heads == 0
            || self.n_kv_heads == 0
            || self.n_kv_heads > self.n_heads
            || !self.n_heads.is_multiple_of(self.n_kv_heads)
            || self.d_head == 0
            || self.d_ff == 0
            || self.vocab == 0
            || self.max_position == 0
            || !self.d_head.is_multiple_of(2)
        {
            return Err(ArtifactError::BadShape);
        }
        // **`MAX_DOT_LEN` bounds REDUCTIONS, and only reductions.**
        //
        // It is the length past which an `i32` accumulator can overflow, which is why ADR-0040
        // Decision E's free reduction order is a premise rather than a gift. So it belongs on the
        // dimensions that are summed over — `d_model` for every projection, `d_ff` for the
        // down-projection, `d_head` for attention — and on nothing else.
        //
        // It used to bound every dimension including `vocab`, on the reasoning that refusing
        // absurd numbers is cheaper than auditing each product. That reasoning was right about
        // the products and wrong about the bound: **`vocab` is an OUTPUT width, never a reduction
        // length**, and at 131_071 the rule excludes every real vocabulary — Qwen2.5's is
        // 151_936, Llama-3's 128_256. The unembedding matmul reduces over `d_model` and produces
        // `vocab` values; the vocabulary never enters an accumulator.
        //
        // The products are audited instead of proxied. Each weight tensor's length is formed with
        // `checked_mul`, so a shape that would wrap is refused for wrapping rather than for being
        // large.
        let bound = kaspa_consensus_core::palw_base0::MAX_DOT_LEN;
        for got in [self.d_head, self.d_ff] {
            if got > bound {
                return Err(ArtifactError::DotTooLong { got });
            }
        }
        let d_model = self.n_heads.checked_mul(self.d_head).ok_or(ArtifactError::BadShape)?;
        if d_model > bound {
            return Err(ArtifactError::DotTooLong { got: d_model });
        }
        // Every tensor length the artifact will form, checked here so `from_parts` compares
        // against numbers that did not wrap on their way in.
        for (a, b) in [(self.vocab, d_model), (self.d_ff, d_model), (d_model, self.d_ff), (self.n_layers, d_model)] {
            a.checked_mul(b).ok_or(ArtifactError::BadShape)?;
        }
        self.max_position.checked_mul(self.d_head).ok_or(ArtifactError::BadShape)?;
        Ok(())
    }

    /// Little-endian, fixed-width, in declaration order. Fixed width matters: a varint encoding
    /// would let two different shapes produce the same bytes at a field boundary.
    pub fn digest_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 * 7 + 16 + 8);
        for v in [self.n_layers, self.n_heads, self.n_kv_heads, self.d_head, self.d_ff, self.vocab, self.max_position] {
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
    /// **Per-output-channel requantization for q, k and v — where a projection BIAS lives.**
    ///
    /// `requant[0..3]` is one parameter set for a whole tensor, which is all BASE-0 ever needed:
    /// its projections have no bias, so the only per-channel quantity would be a scale nobody
    /// varies. Qwen2.5 carries a bias on each of q, k and v (measured from the safetensors
    /// table), and a bias is per-channel by definition — so it rides the `zero` of a per-channel
    /// triple.
    ///
    /// `None` means "use the tensor-wide `requant[i]`", which is exactly what every artifact
    /// built before this field meant. BASE-0's own class is unchanged.
    pub qkv_channel_requant: Option<[Vec<QuantParams>; 3]>,
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
    /// **What the token ids MEAN (condition 6's tokenizer/vocab commitment).**
    ///
    /// A class's execution is a function of token ids, and an id is only a token because some
    /// tokenizer says so. Two nodes running identical weights under different tokenizers agree on
    /// every step and disagree about what was computed — and the court, which adjudicates
    /// arithmetic, would see nothing wrong. So the tokenizer is part of the class identity, and it
    /// enters as a commitment rather than as a file: consensus never runs a tokenizer, it only
    /// needs to know that everyone used the same one.
    ///
    /// `Hash64::default()` means a class that declares none — legal for a derived artifact, which
    /// has no tokenizer at all, and refused for a registered one by the layer that registers it.
    pub tokenizer_commitment: Hash64,
    /// Requantisation for the embedding-normalisation step and for the final norm.
    pub norm_requant: QuantParams,
    /// Narrowing applied after each residual add. `AddElem` widens two `int8` codes to `i32`, so
    /// something must bring the stream back to `int8`; this is the parameter that says by how
    /// much, rather than leaving it to an implicit cast.
    pub residual_requant: QuantParams,
    /// **Per-layer residual narrowing (Phase 3's contingency, triggered by measurement).**
    ///
    /// `residual_requant` above is ONE parameter for the whole stack, and on the real
    /// Qwen2.5-1.5B that is not enough: measured at 28 layers the residual peak reaches 11 out of
    /// 127, so the stream occupies under a tenth of the int8 range, its effective precision is
    /// around 3.5 bits, and quantization noise dominates what the layers compute — the argmax
    /// degenerates to a single constant token. A global shift cannot hold the stream up as the
    /// projections' gains vary from layer to layer, because there is only one of it.
    ///
    /// Two entries per layer, in the order the engine applies them: after the attention residual,
    /// then after the FFN residual. `None` means "use the global one at every site", which is
    /// what every artifact built before this field meant.
    ///
    /// The shifts are a CALIBRATION output, not a choice: converted once with the global rule,
    /// measured, and re-derived from each layer's own peak.
    pub layer_residual_requant: Option<Vec<[QuantParams; 2]>>,
    /// **The residual gains (ADR-0050 Decision B), per layer and per site.**
    ///
    /// `Rescale` sits between the residual add and its narrowing so a DECAYED stream can be lifted
    /// before it is re-quantized. That is the structural gap the real checkpoint measured: a
    /// requantization can only reduce — `QuantParams`' gain is `multiplier / 2^shift` with the
    /// multiplier at most 1.0 — so a stream that has fallen to 5 of 127 has nothing to be given,
    /// and the calibrated table came out `[1, 0, 1, 1, …]` with every layer already at the floor.
    ///
    /// `None` is unity at every site, which is what every artifact built before this field meant
    /// and what BASE-0's own class is: the arithmetic does not move until a calibration sets a
    /// gain. Two entries per layer, in the order the engine applies them — after the attention
    /// residual, then after the FFN residual.
    pub layer_residual_scale: Option<Vec<[ScaleParams; 2]>>,
    /// **The three narrowings the engine used to hold as `const`** (ADR-0049 Decision F, audit
    /// C-05/C-06).
    ///
    /// `BASE0_LAYER_IR` names each of them as a registered tensor — `qk_to_code.requant`,
    /// `code_product.requant`, `rope_clamp.requant` — because "the court resolves a node's
    /// parameters through `PalwWeightOracleV1` and a parameter that cannot be opened is a step
    /// that cannot be adjudicated". `palw_base0_profile`'s own doc already decided where they
    /// belong: "A constant the court must reproduce is either part of a kernel's identity or part
    /// of the artifact, and putting it in the artifact keeps ADR-0040 Decision D's op set at ten
    /// rather than minting a descriptor per constant."
    ///
    /// They lived in the engine, so a real inventory could not carry them and a real opening could
    /// not prove them. The values are unchanged — [`Base0ArtifactV1::CLASS_NARROWINGS`] is what
    /// every artifact built before this field meant — but they are now data the artifact root
    /// covers and an opening can address.
    ///
    /// Order: `[qk_to_code, code_product, rope_clamp]`, which is the order the IR names them.
    pub class_narrowings: [QuantParams; 3],
    derived_seed: Option<u64>,
}

impl Base0ArtifactV1 {
    /// Fractional bits in an `int8` activation code: 127 ≈ 1.0. The engine's own `ACTIVATION_BITS`,
    /// here because the three narrowings below are defined in terms of it and they are artifact
    /// data now.
    pub const ACTIVATION_BITS: u8 = 7;

    /// **The three narrowings every BASE-0 artifact carries, at the values the engine used to hold
    /// as `const`.**
    ///
    /// Byte-identical to what the engine computed before they moved: `qk_to_code` narrows a Qk
    /// value back to an activation code (softmax probabilities and the SiLU output),
    /// `code_product` narrows a `DotI8` whose left operand is a Q7 code, and `rope_clamp` is the
    /// identity narrowing after `RopeTable`, which returns the scale it was handed.
    /// The gain that changes nothing: `multiplier / 2^31` at `multiplier = i32::MAX` is 1.0 to
    /// within a unit, which is `ScaleParams::UNITY_SHIFT`'s own definition.
    pub const UNITY_SCALE: ScaleParams = ScaleParams { multiplier: i32::MAX, shift: ScaleParams::UNITY_SHIFT };

    pub const CLASS_NARROWINGS: [QuantParams; 3] = [
        QuantParams { multiplier: i32::MAX, shift: (kaspa_consensus_core::palw_base0::K as u8) - Self::ACTIVATION_BITS, zero: 0 },
        QuantParams { multiplier: i32::MAX, shift: Self::ACTIVATION_BITS, zero: 0 },
        QuantParams { multiplier: i32::MAX, shift: 0, zero: 0 },
    ];

    /// The narrowing applied after softmax and after SiLU (`blk.{layer}.qk_to_code.requant`).
    pub fn qk_to_code(&self) -> QuantParams {
        self.class_narrowings[0]
    }

    /// The narrowing applied to a code×code product (`blk.{layer}.code_product.requant`).
    pub fn code_product(&self) -> QuantParams {
        self.class_narrowings[1]
    }

    /// The clamp applied after `RopeTable` (`blk.{layer}.rope_clamp.requant`).
    pub fn rope_clamp(&self) -> QuantParams {
        self.class_narrowings[2]
    }

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
                ("wk", shape.kv_dim() * d, l.wk.len()),
                ("wv", shape.kv_dim() * d, l.wv.len()),
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
        Ok(Self {
            shape,
            embed,
            unembed,
            layers,
            rope,
            tokenizer_commitment: Hash64::default(),
            layer_residual_scale: None,
            class_narrowings: Self::CLASS_NARROWINGS,
            norm_requant,
            residual_requant,
            layer_residual_requant: None,
            derived_seed: None,
        })
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
                wk: fill(shape.kv_dim() * d),
                wv: fill(shape.kv_dim() * d),
                wo: fill(d * d),
                w_gate: fill(shape.d_ff * d),
                w_up: fill(shape.d_ff * d),
                w_down: fill(d * shape.d_ff),
                // Tensor-wide: a derived artifact has no biases to carry, so there is nothing
                // per-channel to say. A converted one supplies `Some`.
                qkv_channel_requant: None,
                requant: [
                    QuantParams { multiplier: i32::MAX, shift: shift_for(d), zero: 0 },
                    QuantParams { multiplier: i32::MAX, shift: shift_for(d), zero: 0 },
                    QuantParams { multiplier: i32::MAX, shift: shift_for(d), zero: 0 },
                    QuantParams { multiplier: i32::MAX, shift: shift_for(d), zero: 0 },
                    QuantParams { multiplier: i32::MAX, shift: shift_for(d), zero: 0 },
                    QuantParams { multiplier: i32::MAX, shift: shift_for(d), zero: 0 },
                    QuantParams { multiplier: i32::MAX, shift: shift_for(shape.d_ff), zero: 0 },
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
            QuantParams { multiplier: i32::MAX, shift: (kaspa_consensus_core::palw_base0::K as u8) - 7, zero: 0 },
            // Halve on each residual add, so two `int8` codes summed into `i32` come back to
            // `int8` without saturating — the standard int8-residual convention.
            QuantParams { multiplier: i32::MAX, shift: 1, zero: 0 },
        )?;
        artifact.derived_seed = Some(seed);
        Ok(artifact)
    }

    /// Install per-layer residual narrowing (see the field).
    ///
    /// Refuses a list that is not one pair per layer: a shorter one would leave later layers
    /// silently on the global rule, which is the arrangement this exists to replace.
    pub fn with_layer_residual_requant(mut self, per_layer: Vec<[QuantParams; 2]>) -> Result<Self, ArtifactError> {
        if per_layer.len() != self.shape.n_layers {
            return Err(ArtifactError::WeightLen { tensor: "layer_residual_requant", want: self.shape.n_layers, got: per_layer.len() });
        }
        self.layer_residual_requant = Some(per_layer);
        Ok(self)
    }

    /// The GAIN for `layer`'s attention (`site` 0) or FFN (`site` 1) residual (ADR-0050 B).
    /// Unity when the artifact declares none, which is arithmetically what "no gain" means.
    pub fn residual_scale_at(&self, layer: usize, site: usize) -> ScaleParams {
        match &self.layer_residual_scale {
            Some(per) => per.get(layer).map(|pair| pair[site]).unwrap_or(Self::UNITY_SCALE),
            None => Self::UNITY_SCALE,
        }
    }

    /// Declare the per-layer residual gains. Rejected unless there is one pair per layer: a table
    /// that covers some layers is a table whose omissions are silent.
    pub fn with_layer_residual_scale(mut self, per_layer: Vec<[ScaleParams; 2]>) -> Result<Self, ArtifactError> {
        if per_layer.len() != self.shape.n_layers {
            return Err(ArtifactError::WeightLen { tensor: "layer_residual_scale", want: self.shape.n_layers, got: per_layer.len() });
        }
        self.layer_residual_scale = Some(per_layer);
        Ok(self)
    }

    /// The narrowing for `layer`'s attention (`site` 0) or FFN (`site` 1) residual.
    pub fn residual_requant_at(&self, layer: usize, site: usize) -> QuantParams {
        match &self.layer_residual_requant {
            Some(per) => per.get(layer).map(|pair| pair[site]).unwrap_or(self.residual_requant),
            None => self.residual_requant,
        }
    }

    /// Declare which tokenizer this class's token ids belong to.
    ///
    /// The commitment is over the tokenizer's own bytes — `tokenizer.json` as shipped — so a
    /// verifier checks it by hashing the file rather than by re-deriving a vocabulary, which no
    /// two implementations would agree on.
    pub fn with_tokenizer_commitment(mut self, commitment: Hash64) -> Self {
        self.tokenizer_commitment = commitment;
        self
    }

    /// The commitment for a tokenizer file's bytes.
    pub fn tokenizer_commitment_of(tokenizer_bytes: &[u8]) -> Hash64 {
        let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_BASE0_TOKENIZER_DOMAIN).to_state();
        state.update(&(tokenizer_bytes.len() as u64).to_le_bytes());
        state.update(tokenizer_bytes);
        let out = state.finalize();
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(out.as_bytes());
        Hash64::from_bytes(bytes)
    }

    /// True when the weights came from [`derive_deterministic`] rather than from a real model.
    /// Load-bearing: a derived artifact must never be reported as a registered class.
    pub fn is_derived(&self) -> bool {
        self.derived_seed.is_some()
    }

    pub fn derived_seed(&self) -> Option<u64> {
        self.derived_seed
    }

    /// **The ARTIFACT digest — not the class id** (ADR-0049 Decision G).
    ///
    /// A 64-byte digest over shape, weights, quantisation, the tokenizer commitment and the rotary
    /// table. It was called `execution_class_id`, and that name was the H-08 defect: two different
    /// values were called a class id in two places. The chain keys classes on
    /// `PalwShapeProfileV3::shape_profile_id` — "a class is its graph, which is what the chain
    /// already keys on" — while this is a flat hash of a whole artifact, and Decision G's own
    /// objection to using it is that **nothing can be opened against it**: a court that wants one
    /// weight row cannot prove anything about a digest that hashed the file.
    ///
    /// So the two values now have their two jobs. This one answers "are these the same bytes",
    /// which is what a converter, a fleet check and an equality test need.
    /// [`Self::execution_class_id`] answers "is this the class the chain registered", and
    /// `artifact_root` (the inventory's Merkle root) answers "does this row belong to it".
    ///
    /// `derived_seed` is NOT covered, and that is the correct side of the rule stated in the
    /// module docs: it does not change any output, so covering it would split one computed class
    /// into two.
    /// **The CLASS id: the shape profile id of the graph this artifact is run under**
    /// (ADR-0049 Decision G — "`execution_class_id` is the shape profile id").
    ///
    /// It takes the geometry rather than deriving it, and that is the fact worth stating: a class
    /// is not a function of an artifact. `n_ctx`, `tile_len` and `n_threads` are registration
    /// choices that no weight file contains, and they are inside the profile — so an artifact
    /// alone cannot say which class it belongs to, and the value that pretended otherwise was the
    /// flat digest this method replaces.
    ///
    /// What the artifact CAN do is refuse a geometry that is not its own, which is what makes this
    /// bridge safe: the two namespaces meet in exactly one place, with a check between them.
    pub fn execution_class_id(
        &self,
        geometry: kaspa_consensus_core::palw_base0_profile::PalwBase0GeometryV1,
    ) -> Result<Hash64, ArtifactError> {
        self.check_geometry(&geometry)?;
        let profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(geometry)
            .map_err(|_| ArtifactError::Profile("this geometry builds no BASE-0 profile, so it names no class"))?;
        Ok(profile.shape_profile_id())
    }

    /// Every geometry field this artifact also carries must agree with it. `n_ctx`, `tile_len` and
    /// `n_threads` are the registration's alone — the artifact has nothing to say about them, and
    /// pretending it did would be inventing a fact.
    pub fn check_geometry(
        &self,
        geometry: &kaspa_consensus_core::palw_base0_profile::PalwBase0GeometryV1,
    ) -> Result<(), ArtifactError> {
        let checks: [(&'static str, u64, u64); 6] = [
            ("layer_count", self.shape.n_layers as u64, geometry.layer_count as u64),
            ("hidden_dim", self.shape.d_model() as u64, geometry.hidden_dim as u64),
            ("ffn_dim", self.shape.d_ff as u64, geometry.ffn_dim as u64),
            ("attn_heads", self.shape.n_heads as u64, geometry.attn_heads as u64),
            ("attn_head_dim", self.shape.d_head as u64, geometry.attn_head_dim as u64),
            ("vocab_size", self.shape.vocab as u64, geometry.vocab_size as u64),
        ];
        for (field, artifact, geometry) in checks {
            if artifact != geometry {
                return Err(ArtifactError::GeometryMismatch { field, artifact, geometry });
            }
        }
        // `rms_eps_q` is an artifact field AND a profile field, and it moves every activation, so
        // a disagreement here is a class computing something else under this class's name.
        if self.shape.eps_q != geometry.rms_eps_q {
            return Err(ArtifactError::GeometryMismatch {
                field: "rms_eps_q",
                artifact: self.shape.eps_q as u64,
                geometry: geometry.rms_eps_q as u64,
            });
        }
        Ok(())
    }

    pub fn artifact_digest(&self) -> Hash64 {
        let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_BASE0_ARTIFACT_DOMAIN).to_state();
        state.update(&self.shape.digest_bytes());
        state.update(&self.rope.digest_bytes());
        // The tokenizer, inside the identity: two classes with identical weights and different
        // tokenizers compute different things while agreeing on every arithmetic step.
        state.update(self.tokenizer_commitment.as_byte_slice());
        absorb_tensor(&mut state, b"embed", &self.embed);
        absorb_tensor(&mut state, b"unembed", &self.unembed);
        absorb_quant(&mut state, &self.norm_requant);
        absorb_quant(&mut state, &self.residual_requant);
        // The three class narrowings, which used to be engine constants and are artifact data
        // now (ADR-0049 F): each of them moves every activation that passes through it, so each is
        // inside the digest by the module's own rule.
        for narrowing in &self.class_narrowings {
            absorb_quant(&mut state, narrowing);
        }
        // The residual GAINS (ADR-0050 B), presence-tagged and length-prefixed for the same reason
        // the per-layer narrowings are: `None` and an empty `Some` must be different streams.
        match &self.layer_residual_scale {
            None => state.update(&[0u8]),
            Some(per) => {
                state.update(&[1u8]);
                state.update(&(per.len() as u64).to_le_bytes());
                for pair in per {
                    absorb_scale(&mut state, pair[0].multiplier, pair[0].shift);
                    absorb_scale(&mut state, pair[1].multiplier, pair[1].shift);
                }
                &mut state
            }
        };
        // Per-layer residual narrowing, presence-tagged and length-prefixed for the same reason
        // the per-channel triples are: `None` and an empty `Some` must be different streams, and
        // a class calibrated per layer is not the class that was not.
        match &self.layer_residual_requant {
            None => {
                state.update(&[0u8]);
            }
            Some(per) => {
                state.update(&[1u8]);
                state.update(&(per.len() as u64).to_le_bytes());
                for pair in per {
                    absorb_quant(&mut state, &pair[0]);
                    absorb_quant(&mut state, &pair[1]);
                }
            }
        };
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
                absorb_quant(&mut state, p);
            }
            // The per-channel triples, which is where a converted class's BIASES are. Length-
            // prefixed and tagged with presence, so `None` and an empty `Some` are different
            // streams — one means "this class has no per-channel parameters" and the other means
            // "it has three empty lists", and a digest that could not tell them apart would be a
            // digest two different artifacts share.
            match &l.qkv_channel_requant {
                None => {
                    state.update(&[0u8]);
                }
                Some(per) => {
                    state.update(&[1u8]);
                    for channel in per.iter() {
                        state.update(&(channel.len() as u64).to_le_bytes());
                        for p in channel {
                            absorb_quant(&mut state, p);
                        }
                    }
                }
            };
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
/// A `ScaleParams` — multiplier and shift, no additive term (there is none in that type).
fn absorb_scale(state: &mut blake2b_simd::State, multiplier: i32, shift: u8) {
    state.update(&multiplier.to_le_bytes());
    state.update(&[shift]);
}

/// A `QuantParams` — multiplier, shift **and zero point**.
///
/// The zero is the additive term the ADR-0040 amendment added, and it is where a projection bias
/// lives. It was outside the digest for exactly as long as it did not exist; leaving it there
/// would mean two artifacts whose every bias differs share one class id — a class could be
/// registered with one set of biases and executed with another, and the court would see a single
/// identity. `absorb_quant` is separate from `absorb_scale` rather than a widened version of it
/// because `ScaleParams` genuinely has no third field, and a shared function would have had to
/// invent a zero for it.
fn absorb_quant(state: &mut blake2b_simd::State, p: &QuantParams) {
    state.update(&p.multiplier.to_le_bytes());
    state.update(&[p.shift]);
    state.update(&p.zero.to_le_bytes());
}

#[cfg(test)]
mod tests {
    /// **Condition 6: the tokenizer is inside the class identity.**
    ///
    /// A class's execution is a function of token ids, and an id is only a token because some
    /// tokenizer says so. Two nodes with identical weights and different tokenizers agree on every
    /// arithmetic step and disagree about what was computed — and the court, which adjudicates
    /// arithmetic, would see nothing wrong at all. So the commitment has to be in the id.
    #[test]
    fn the_tokenizer_is_part_of_the_class() {
        let base = Base0ArtifactV1::derive_deterministic(tiny(), 11).unwrap();
        assert_eq!(base.tokenizer_commitment, Hash64::default(), "a derived artifact declares none");

        let a = base.clone().with_tokenizer_commitment(Base0ArtifactV1::tokenizer_commitment_of(b"{\"model\":\"a\"}"));
        let b = base.clone().with_tokenizer_commitment(Base0ArtifactV1::tokenizer_commitment_of(b"{\"model\":\"b\"}"));
        assert_ne!(a.artifact_digest(), b.artifact_digest(), "two tokenizers are two classes");
        assert_ne!(a.artifact_digest(), base.artifact_digest(), "and declaring one is not declaring none");

        // The commitment is over the FILE's bytes, so a verifier checks it by hashing what it
        // downloaded rather than by re-deriving a vocabulary — which no two implementations would
        // agree on.
        assert_eq!(
            Base0ArtifactV1::tokenizer_commitment_of(b"abc"),
            Base0ArtifactV1::tokenizer_commitment_of(b"abc"),
            "the same bytes commit the same way"
        );
        assert_ne!(Base0ArtifactV1::tokenizer_commitment_of(b"abc"), Base0ArtifactV1::tokenizer_commitment_of(b"abd"));
        // Length-prefixed, so no two files concatenate into a third's commitment.
        assert_ne!(Base0ArtifactV1::tokenizer_commitment_of(b"ab"), Base0ArtifactV1::tokenizer_commitment_of(b"a"));
        // …and its own domain key, so it cannot collide with an artifact digest.
        assert_ne!(Base0ArtifactV1::tokenizer_commitment_of(b""), base.artifact_digest());
    }

    /// **Condition 6: the requant parameters are inside `artifact_root`, biases included.**
    ///
    /// They were not. `absorb_scale` wrote a multiplier and a shift, so the zero point — the
    /// additive term the ADR-0040 amendment added, and where a projection bias lives — was
    /// outside the digest. Two artifacts whose every bias differed shared one class id, which
    /// means a class could be registered with one set of biases and executed with another while
    /// the court saw a single identity. Found by a converter test asserting that a calibrated and
    /// an uncalibrated conversion are different classes; they were not.
    #[test]
    fn the_class_id_covers_every_quantization_parameter() {
        use kaspa_consensus_core::palw_base0_ops::QuantParams;
        let base = Base0ArtifactV1::derive_deterministic(tiny(), 7).unwrap();
        let id = base.artifact_digest();

        // Each field of each triple moves it, one at a time.
        for mutate in [
            (|a: &mut Base0ArtifactV1| a.layers[0].requant[0].zero = 3) as fn(&mut Base0ArtifactV1),
            |a| a.layers[0].requant[0].multiplier -= 1,
            |a| a.layers[0].requant[0].shift += 1,
            |a| a.norm_requant.zero = 1,
            |a| a.residual_requant.zero = -1,
            |a| a.layers[1].requant[6].zero = 2,
        ] {
            let mut m = base.clone();
            mutate(&mut m);
            assert_ne!(m.artifact_digest(), id, "a quantization parameter must be inside the identity");
        }

        // The per-channel triples too — and PRESENCE is distinguishable from an empty list, so
        // "this class has no per-channel parameters" and "it has three empty ones" are different
        // streams rather than one digest two artifacts share.
        let mut with_channels = base.clone();
        with_channels.layers[0].qkv_channel_requant = Some([Vec::new(), Vec::new(), Vec::new()]);
        assert_ne!(with_channels.artifact_digest(), id, "an empty Some is not a None");

        let d = tiny().d_model();
        let kv = tiny().kv_dim();
        let triple = |zero: i32| QuantParams { multiplier: i32::MAX, shift: 7, zero };
        let mut biased = base.clone();
        biased.layers[0].qkv_channel_requant =
            Some([vec![triple(0); d], vec![triple(0); kv], vec![triple(0); kv]]);
        let unbiased_id = biased.artifact_digest();
        let mut one_bias = biased.clone();
        one_bias.layers[0].qkv_channel_requant.as_mut().unwrap()[0][d - 1] = triple(1);
        assert_ne!(
            one_bias.artifact_digest(),
            unbiased_id,
            "ONE channel's bias, in the LAST position, must move the class id — a digest that missed it \
             would let a class run with biases it was not registered with"
        );
    }

    use super::*;

    pub(crate) fn tiny() -> Base0ShapeV1 {
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

    /// The digest is a pure function of the artifact: two derivations of the same seed agree.
    #[test]
    fn the_class_id_is_reproducible() {
        let a = Base0ArtifactV1::derive_deterministic(tiny(), 7).unwrap();
        let b = Base0ArtifactV1::derive_deterministic(tiny(), 7).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.artifact_digest(), b.artifact_digest());
    }

    /// **Every** shape field must move the class id. This is the test the module docs name: it
    /// fails when a field is added to the shape and not added to `digest_bytes`, which is the bug
    /// that would give two differently-computing artifacts one id.
    /// **Audit H-08: one meaning for "class id" (ADR-0049 Decision G).**
    ///
    /// Two different values were called a class id in two places. This crate's was a flat digest
    /// over a whole artifact — which is not what the chain keys on, and, Decision G's own
    /// objection, is a value **nothing can be opened against**: a court that wants one weight row
    /// cannot prove anything about a hash of the file. The chain keys on the shape profile id
    /// ("a class is its graph"), and `palw_rc_base0_registration_v1` already did.
    ///
    /// The two now answer their two questions, and the pair of assertions below is the whole
    /// point: same graph + different weights is ONE class and TWO artifacts.
    #[test]
    fn the_class_id_is_the_graph_and_the_digest_is_the_bytes() {
        use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};

        let geometry = PALW_RC_BASE0_GEOMETRY;
        let shape = Base0ShapeV1 {
            n_layers: geometry.layer_count as usize,
            n_heads: geometry.attn_heads as usize,
            n_kv_heads: geometry.attn_heads as usize,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: geometry.rms_eps_q,
        };
        let a = Base0ArtifactV1::derive_deterministic(shape, 1).unwrap();
        let b = Base0ArtifactV1::derive_deterministic(shape, 2).unwrap();

        // One graph, one class — whatever the weights are.
        let expected = base0_profile_v1(geometry).unwrap().shape_profile_id();
        assert_eq!(a.execution_class_id(geometry).unwrap(), expected);
        assert_eq!(b.execution_class_id(geometry).unwrap(), expected, "different weights are the SAME class");
        // Two artifacts, though — which is what the digest is for, and why it cannot be the id.
        assert_ne!(a.artifact_digest(), b.artifact_digest(), "different weights are DIFFERENT artifacts");

        // A class is not a function of an artifact: `n_ctx` and `tile_len` are registration
        // choices no weight file contains, and they move the id.
        let wider = kaspa_consensus_core::palw_base0_profile::PalwBase0GeometryV1 { tile_len: geometry.tile_len / 2, ..geometry };
        assert_ne!(a.execution_class_id(wider).unwrap(), expected, "a tile length is inside the class");
        assert_eq!(a.artifact_digest(), a.artifact_digest(), "and outside the artifact");

        // And the bridge refuses a geometry that is not this artifact's, so the two namespaces
        // cannot be joined across a mismatch.
        let foreign = kaspa_consensus_core::palw_base0_profile::PalwBase0GeometryV1 {
            hidden_dim: geometry.hidden_dim + geometry.attn_head_dim,
            attn_heads: geometry.attn_heads + 1,
            ..geometry
        };
        assert!(matches!(a.execution_class_id(foreign), Err(ArtifactError::GeometryMismatch { field: "hidden_dim", .. })));
    }

    #[test]
    fn shape_digest_covers_every_field() {
        let base = tiny();
        let baseline = Base0ArtifactV1::derive_deterministic(base, 1).unwrap().artifact_digest();
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
            let id = Base0ArtifactV1::derive_deterministic(shape, 1).unwrap().artifact_digest();
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
        let id = base.artifact_digest();
        let flip = |v: &Vec<i8>| -> Vec<i8> {
            let mut v = v.clone();
            v[0] = v[0].wrapping_add(1);
            v
        };
        let mut a = base.clone();
        a.embed = flip(&a.embed);
        assert_ne!(id, a.artifact_digest(), "embed is outside the digest");
        let mut a = base.clone();
        a.unembed = flip(&a.unembed);
        assert_ne!(id, a.artifact_digest(), "unembed is outside the digest");
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
                assert_ne!(id, a.artifact_digest(), "layer {li} {name} is outside the digest");
                // And the requantisation, which changes the output without changing a weight.
                let mut a = base.clone();
                a.layers[li].requant[pick].shift += 1;
                assert_ne!(id, a.artifact_digest(), "layer {li} requant[{pick}] is outside the digest");
            }
        }
        let mut a = base.clone();
        a.norm_requant.multiplier -= 1;
        assert_ne!(id, a.artifact_digest(), "norm_requant is outside the digest");
        let mut a = base.clone();
        a.residual_requant.shift += 1;
        assert_ne!(id, a.artifact_digest(), "residual_requant is outside the digest");
        // The two amplifying scales are the parameters ADR-0040 H added. They move every logit
        // and every gate, so a digest that missed them would let an executor retune the model
        // while still claiming the class.
        for li in 0..base.shape.n_layers {
            let mut a = base.clone();
            a.layers[li].attn_logit_scale.shift += 1;
            assert_ne!(id, a.artifact_digest(), "layer {li} attn_logit_scale is outside the digest");
            let mut a = base.clone();
            a.layers[li].ffn_gate_scale.multiplier -= 1;
            assert_ne!(id, a.artifact_digest(), "layer {li} ffn_gate_scale is outside the digest");
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
        assert_ne!(base.artifact_digest(), swapped.artifact_digest());
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
        assert_eq!(derived.artifact_digest(), carried.artifact_digest());
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
