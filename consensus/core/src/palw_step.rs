//! PALW step function v1 — the coordinate system of ADR-0030.
//!
//! Normative sources: ADR-0030 (all of it), ADR-0027 §1/§2 (the one-step refutation this
//! gives coordinates to), and the kernel maps of 2026-08-16 the ADR's Facts record.
//!
//! # What this module is — and is not
//!
//! This is the **step space**: the frozen operator taxonomy (`PalwStepOpKindV1`), the shape
//! profile v3 schema and its identity (`shape_profile_id_v3`), and the bijection between a
//! step index and its coordinates `(call, node, position, tile)`. It is what lets a
//! refutation *name* a step and every observer derive the same meaning for the name.
//!
//! It is deliberately **not** the commitment (the step leg is the execution-commitment v2
//! increment), **not** the kernel programs (`kernel_semantics_id` names a frozen program;
//! the catalog is its own increment, validated against the pinned kernels per class before
//! any id may appear in a registered profile), and **not** consensus-wired.
//!
//! # Pinned here vs registration-measured
//!
//! Pinned: the schema, its validation rules, the id derivations, the enumeration order and
//! both directions of the bijection. Registration-measured, per class (the tap-profile
//! discipline): every value a `PalwShapeProfileV3` carries — geometry, execution flags, node
//! tables, kernel ids, transcendental bindings, contraction facts. A profile in this module's
//! tests is a schema exercise, never a network fact.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use thiserror::Error;

use crate::palw_v2::PalwJobContextV2;

// ---------------------------------------------------------------------------------------------
// Versions, domains, caps
// ---------------------------------------------------------------------------------------------

/// Wire version of every step-v1 object in this module.
pub const PALW_STEP_OBJECT_VERSION_V1: u16 = 1;

/// The shape-profile v3 identity domain. v2's shape-string domain stays frozen; deployed
/// contexts keep meaning what they meant. Which id a class's jobs carry is a registration
/// fact (ADR-0030 §2).
pub const PALW_STEP_DOMAIN_SHAPE_PROFILE_V3: &[u8] = b"misaka-palw/shape-profile/v3";
/// The registration preimage of a `kernel_semantics_id`: a canonical descriptor string naming
/// one frozen reduction-order program (ADR-0030 premise: order is code named by id, never
/// interpreted data).
pub const PALW_STEP_DOMAIN_KERNEL_SEMANTICS: &[u8] = b"misaka-palw/kernel-semantics-id/v1";
/// The registration preimage of a transcendental algorithm id (ADR-0031's namespace anchor).
pub const PALW_STEP_DOMAIN_TRANSCENDENTAL: &[u8] = b"misaka-palw/transcendental-algorithm-id/v1";
/// The registration preimage of the checkpoint state chunk map (ADR-0030 §3).
pub const PALW_STEP_DOMAIN_STATE_CHUNK_MAP: &[u8] = b"misaka-palw/state-chunk-map-id/v1";

/// Every domain this module introduces (uniqueness-tested against every other PALW family).
pub const PALW_STEP_ALL_DOMAINS: &[&[u8]] = &[
    PALW_STEP_DOMAIN_SHAPE_PROFILE_V3,
    PALW_STEP_DOMAIN_KERNEL_SEMANTICS,
    PALW_STEP_DOMAIN_TRANSCENDENTAL,
    PALW_STEP_DOMAIN_STATE_CHUNK_MAP,
];

/// Cap on step-leg leaves per job (ADR-0030 §3 sizing: the pinned geometry at the credited
/// ceiling is ≈ 3.26 M — inside with headroom, tight against adversarial allocation).
pub const PALW_STEP_MAX_LEAVES: u64 = 1 << 22;

/// Cap on the ENUMERATION the shape drives — `n_ctx` positions times `layer_count` layers.
///
/// `PALW_STEP_MAX_LEAVES` bounds the ANSWER; this bounds the WORK of computing it, and the two
/// are not the same bound. `worst_case_step_leaf_count_v1` walks every position and every layer
/// before it can compare against the leaf cap, so with `n_ctx: u32` and `layer_count: u16`
/// unbounded a single ~5 KB `ClassRegistered` object buys ≈2.8e14 iterations — evaluated by every
/// node, for every chain candidate, on a block already stored by isolation validation. It
/// reproduces on restart and on resync, so the only recovery is a hard fork.
///
/// 1 << 24 is ≈8× the largest real class this bundle contemplates (Qwen at 32 K context and 64
/// layers is 2.1 M) and is a few tens of milliseconds of cheap iteration once, at registration.
pub const PALW_STEP_MAX_ENUMERATION: u64 = 1 << 24;

/// Ceiling on declared layers. A layer's table is walked at every position, so this is the inner
/// factor of [`PALW_STEP_MAX_ENUMERATION`]; it is also far above any architecture the court can
/// adjudicate.
/// The largest stabiliser an rms epsilon may declare (audit finding 4c).
///
/// It is added to a Qk mean of squares in `i64` and this crate builds with `overflow-checks`, so
/// the only question is where the line sits. 2^40 is far above any epsilon a real profile uses
/// (the shipped ones are single-digit Qk units) and far below the range where the addition can
/// carry, so it refuses the attack without constraining an honest class.
pub const PALW_STEP_MAX_RMS_EPS_Q: i64 = 1 << 40;

pub const PALW_STEP_MAX_LAYERS: u16 = 1024;
/// Most nodes one layer template may declare.
pub const PALW_STEP_MAX_NODES_PER_TABLE: usize = 64;
/// Tile length bounds (elements per committed tile).
pub const PALW_STEP_MIN_TILE_LEN: u32 = 16;
pub const PALW_STEP_MAX_TILE_LEN: u32 = 65_536;
/// Bounds on the KV aux-chunk width (calls per chunk leaf); 0 disables the aux series.
pub const PALW_STEP_MAX_KV_CHUNK_CALLS: u32 = 4096;

/// `kernel_semantics_id`: the identity of ONE frozen reduction-order program. The descriptor
/// string is the registration-time claim (kernel family, arch path, lane structure, source
/// tree commit); the program itself is code in the catalog, golden-tested against the pinned
/// kernel before the id may be registered.
pub fn kernel_semantics_id_v1(descriptor: &str) -> Hash64 {
    keyed64(PALW_STEP_DOMAIN_KERNEL_SEMANTICS, &[descriptor.as_bytes()])
}

/// `transcendental_algorithm_id`: same discipline for exp/log/sin/cos provenance —
/// `source-polynomial/…` for programs transcribed from the pinned tree, `libm/…` for the
/// class's libm algorithms (ADR-0030 Fact 15).
pub fn transcendental_algorithm_id_v1(descriptor: &str) -> Hash64 {
    keyed64(PALW_STEP_DOMAIN_TRANSCENDENTAL, &[descriptor.as_bytes()])
}

/// `state_chunk_map_id`: the identity of the measured checkpoint state chunk geometry
/// (per-(layer, head) slices; ADR-0030 §3). Opaque until a runtime demonstrates the layout.
pub fn state_chunk_map_id_v1(map_string: &str) -> Hash64 {
    keyed64(PALW_STEP_DOMAIN_STATE_CHUNK_MAP, &[map_string.as_bytes()])
}

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for p in parts {
        h.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwStepError {
    #[error("unsupported palw-step object version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("shape profile is not canonical: {0}")]
    ProfileNotCanonical(&'static str),
    #[error("job shape yields {got} step leaves, exceeding the {max} cap")]
    TooManyLeaves { got: u64, max: u64 },
    #[error("step leaf index {index} is not below the leaf count {count}")]
    LeafIndexOutOfRange { index: u64, count: u64 },
    #[error("step coordinates are not canonical for this (profile, context)")]
    CoordinatesNotCanonical,
}

// ---------------------------------------------------------------------------------------------
// The frozen operator taxonomy
// ---------------------------------------------------------------------------------------------

/// The closed set of operator shapes the pinned graph can contain (ADR-0030 §1). A graph
/// needing a kind outside this set is a new profile version, not a stretched meaning.
/// Discriminants are wire-frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwStepOpKindV1 {
    /// Token-id row lookup + dequantization of the (quantized) embedding table.
    EmbedLookup = 0,
    /// RMS norm, including the one CPU fusion (× weight) — `1/sqrtf(mean + eps)`.
    RmsNorm = 1,
    /// Quantized-weight matmul (repack gemv/gemm or classic vec_dot; which one is part of the
    /// node's `kernel_semantics_id`, per ADR-0030 Fact 14).
    MatMulQuant = 2,
    /// F16-operand matmul over cache material (attention scores / V mix).
    MatMulF16 = 3,
    /// Interleaved multimodal RoPE (partial rotation; sections; libm sinf/cosf).
    RopeImrope = 4,
    /// Fused softmax row: scale + mask + max + exp + double-sum + reciprocal-multiply.
    SoftMax = 5,
    /// `1/(1+expf(−x))`, scalar libm.
    Sigmoid = 6,
    /// `x > 20 → x`, else `logf(1+expf(x))`, scalar libm.
    Softplus = 7,
    /// The 4-tap causal conv over concatenated qkv channels.
    SsmConv = 8,
    /// Standalone SiLU (the conv activation path).
    Silu = 9,
    /// Fused SwiGLU GLU node (`silu(gate) · up`) — Fact: the FFN emits ONE node, not two.
    Glu = 10,
    /// The fused gated-delta-net recurrence (state update + output), ADR-0030 Fact 12.
    GatedDeltaNet = 11,
    /// `1/max(sqrtf(sum), eps)` — a different composition than RmsNorm (Fact 15).
    L2Norm = 12,
    /// Elementwise multiply (with the pinned broadcast rules).
    MulElem = 13,
    /// Elementwise add (residuals).
    AddElem = 14,
    /// Standalone scale node (none in the pinned graph — kq_scale lives inside SoftMax — but
    /// the kind exists so a future graph that emits one is expressible).
    Scale = 15,
    /// f32 → f16 cache write (SET_ROWS; the software RNE bit-twiddle = ruleset v2 semantics).
    CpyF32F16 = 16,
}

/// What role a node's output plays for the auxiliary series (ADR-0030 §3): K/V cache writes
/// feed the KV-chunk aux leaves that make full-context reductions openable in ~10 chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwStepNodeRoleV1 {
    Plain = 0,
    KCacheWrite = 1,
    VCacheWrite = 2,
}

/// How a node's output length is derived. `KvScaled` lengths use the TRUE kv length of the
/// position (`prefill position p` sees `p+1`; decode call `c` sees `P+c`) — never the padded
/// cache length (ADR-0030 Fact 17: padded rows must not be committed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwStepOutLenV1 {
    /// A fixed element count per position.
    Fixed { elements: u32 } = 0,
    /// `multiplier × kv_len(position)` elements (attention scores/softmax rows).
    KvScaled { multiplier: u32 } = 1,
}

/// One operator invocation slot in a layer template (or the pre/post graph).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStepNodeV1 {
    pub op_kind: PalwStepOpKindV1,
    pub role: PalwStepNodeRoleV1,
    /// The GGUF tensor name this node consumes as its weight operand (empty = none). Binds
    /// ADR-0030 Fact 4's per-layer dtype variance: `{layer}` in the name is substituted with
    /// the layer index at interpretation time.
    pub weight_name: String,
    /// **GGML dtype bytes of the weight operand, PER LAYER (empty when `weight_name` is empty).**
    ///
    /// One byte per layer this node's table applies to, in layer order — not one byte for the
    /// node. The field used to be a single `u8`, and its own doc claimed to bind "ADR-0030 Fact
    /// 4's per-layer dtype variance", which a single byte cannot do. Measured on the pinned
    /// Qwen3.5-2B-Q4_K_M (2026-08-20): `ffn_down.weight` is `Q6_K` on twelve layers and `Q4_K` on
    /// the other twelve, and `attn_v.weight` is `Q6_K` on four of the six attention layers and
    /// `Q4_K` on the other two. The split follows the quantizer's imatrix heuristics, not any
    /// rule a profile could restate — so a single byte could not describe the pinned model at
    /// all, and any profile written into the old type would have declared a dtype that is wrong
    /// for half the layers it covers.
    ///
    /// That matters because dtype IS arithmetic here: a `Q4_K` and a `Q6_K` matmul dequantize
    /// through different block layouts and accumulate differently. A court recomputing a step
    /// against the declared dtype would convict an honest producer on every layer where the
    /// declaration and the file disagree.
    pub weight_dtypes: Vec<u8>,
    pub out_len: PalwStepOutLenV1,
    /// Elements per committed tile (last tile ragged). Bounds: [MIN, MAX]_TILE_LEN.
    pub tile_len: u32,
    /// The frozen reduction-order program adjudicating this node's steps.
    pub kernel_semantics_id: Hash64,
    /// The node's data inputs — WHICH committed material a step of this node is recomputed
    /// from. Without this the adjudicator could not reject a challenger who opens unrelated
    /// tiles as "the inputs" (a manufactured conviction). Values are intra-table node
    /// indices (this layer's template), or the [`PALW_STEP_INPUT_*`] sentinels for the layer
    /// input, K/V aux chunks, and checkpoint state. The weight operand is `weight_name`, not
    /// listed here.
    pub input_refs: Vec<u16>,
}

/// **What a committed step value's 32 bits MEAN.**
///
/// The step leg refuses a value whose f32 reinterpretation is non-finite — a producer-side safety
/// net, because a float execution that produced a NaN is invalid and must emit no receipt. That
/// rule is a fact about FLOAT lanes, and applying it to an integer class is not conservative, it
/// is wrong: BASE-0 commits int32 codes, and every integer in `[-8_388_608, -1]` has the
/// all-ones exponent, so the builder rejected essentially every negative activation. The RC's
/// permanently-Active liveness floor could not commit a step leg at all.
///
/// Inside `shape_profile_id` by construction (the id digests this struct), so a class cannot
/// reinterpret its own lanes without changing identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwStepLaneV1 {
    /// IEEE-754 binary32. Non-finite values are refused at commit.
    Float32 = 0,
    /// Two's-complement int32 (BASE-0). Every bit pattern is a legal value, so there is nothing
    /// to refuse — and no float rule may be applied to them.
    Int32 = 1,
}

/// Which of a profile's four node tables is being talked about. Exists so
/// [`PalwShapeProfileV3::table_layer_span`] cannot be called with a table it does not know, and
/// so validation and the profile author name the same four things.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwStepTableV1 {
    Pre,
    Gdn,
    Attn,
    Post,
}

/// `input_refs` sentinel: the layer's input row (the previous layer's final output, or the
/// embedding output for layer 0; for pre/post tables, the preceding table's output).
pub const PALW_STEP_INPUT_LAYER_IN: u16 = 0xFFFF;
/// `input_refs` sentinel: the K aux-chunk series of this attention layer.
pub const PALW_STEP_INPUT_KV_K: u16 = 0xFFFE;
/// `input_refs` sentinel: the V aux-chunk series of this attention layer.
pub const PALW_STEP_INPUT_KV_V: u16 = 0xFFFD;
/// `input_refs` sentinel: this layer's slice of the checkpoint recurrent state.
pub const PALW_STEP_INPUT_CHECKPOINT_STATE: u16 = 0xFFFC;
/// Smallest sentinel value (everything below is an intra-table node index).
pub const PALW_STEP_INPUT_SENTINEL_MIN: u16 = 0xFFF0;

/// Layer kinds of the hybrid graph. Which kind a layer is follows from
/// `full_attention_interval` (Fact 1): layer `i` is `Attention` iff `(i+1) % interval == 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwLayerKindV1 {
    GatedDeltaNet = 0,
    Attention = 1,
}

/// A transcendental call site in the pinned graph, bound to the algorithm that computes it
/// (ADR-0030 Fact 15 / ADR-0031). Discriminants wire-frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwTranscendentalSiteV1 {
    /// The vectorized exp polynomial (softmax body, silu body, swiglu body).
    VectorExpPolynomial = 0,
    /// libm `expf` (vector-op tails, sigmoid, softplus, the GDN decay).
    LibmExpf = 1,
    /// libm `logf` (softplus).
    LibmLogf = 2,
    /// libm `sinf` (RoPE).
    LibmSinf = 3,
    /// libm `cosf` (RoPE).
    LibmCosf = 4,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTranscendentalBindingV1 {
    pub site: PalwTranscendentalSiteV1,
    pub algorithm_id: Hash64,
}

/// A named scalar site whose FMA-contraction state is a measured per-class fact
/// (ADR-0030 Fact 9: source cannot resolve it; only the shipped binary's disassembly can).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwContractionSiteV1 {
    /// RoPE's `x0·cos − x1·sin` / `x0·sin + x1·cos` pair.
    RopeRotate = 0,
    /// The classic q6_K trailing `d_all · yd · (isum − 32·mins)` chain (the LM head path).
    Q6KTail = 1,
    /// The ssm_conv 4-tap `sumf += s·c` chain.
    SsmConvTaps = 2,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwContractionFactV1 {
    pub site: PalwContractionSiteV1,
    /// 1 = the binary fuses (fmla/vfmadd), 0 = it does not. Measured, never assumed.
    pub contracted: u8,
}

// ---------------------------------------------------------------------------------------------
// Shape profile v3
// ---------------------------------------------------------------------------------------------

/// Everything `shape_profile_id` binds under ADR-0030 §2. Floats are carried as raw bit
/// patterns — a float never appears in a preimage.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwShapeProfileV3 {
    /// = [`PALW_STEP_OBJECT_VERSION_V1`].
    pub version: u16,
    /// What this class's committed step values are (see [`PalwStepLaneV1`]).
    pub lane: PalwStepLaneV1,

    // --- model geometry (restated from the pinned GGUF so the profile is self-contained) ---
    pub layer_count: u16,
    /// Layer `i` is Attention iff `(i+1) % full_attention_interval == 0`. 0 = no attention
    /// layers (a pure-recurrent graph); 1 = every layer.
    pub full_attention_interval: u16,
    pub hidden_dim: u32,
    pub ffn_dim: u32,
    pub attn_heads: u16,
    pub attn_kv_heads: u16,
    pub attn_head_dim: u32,
    pub rope_dims: u16,
    pub rope_sections: [u16; 4],
    /// f32 bit pattern of the rope frequency base.
    pub rope_freq_base_bits: u32,
    /// f32 bit patterns of the two norm epsilons.
    pub rms_eps_bits: u32,
    pub l2_eps_bits: u32,
    /// `PALW-BASE-0`'s RMS-norm epsilon, Qk (ADR-0040 D op 3) — the INTEGER class's own constant.
    ///
    /// Separate from [`Self::rms_eps_bits`] because it is a different type in a different
    /// arithmetic: that one is an f32 bit pattern for the float classes' `1/sqrtf(mean + eps)`,
    /// this one is a Qk integer added before `IntRsqrt`. Reinterpreting one as the other would be
    /// a silent change of both scale and semantics.
    ///
    /// It has to be a registration fact rather than a constant in the adjudicator: the court
    /// recomputed BASE-0's `RmsNorm` with a hardcoded `eps = 1`, so a class registered with any
    /// other epsilon had its honest producers convicted on every norm step (re-audit §3.3). It is
    /// inside `shape_profile_id` by construction — the id is a digest of this struct's Borsh bytes
    /// — so a class cannot change its epsilon without changing its identity.
    pub base0_rms_eps_q: i64,
    pub gdn_heads: u16,
    pub gdn_head_k_dim: u32,
    pub gdn_head_v_dim: u32,
    pub gdn_conv_kernel: u16,
    pub vocab_size: u32,

    // --- execution-shape facts (the previously under-pinned flags, ADR-0030 Fact 8) ---
    pub repack_on: u8,
    pub llamafile_on: u8,
    /// MUST be 1: the flash path reintroduces a cross-thread float reduction (Fact 7).
    pub flash_attn_disabled: u8,
    pub fused_gdn_on: u8,
    pub use_ref_off: u8,
    /// 1 = f16 KV cache.
    pub kv_cache_f16: u8,
    pub n_ctx: u32,
    pub n_batch: u32,
    pub n_ubatch: u32,
    pub n_seq: u32,
    pub n_threads: u32,

    // --- the node tables (ADR-0030 §1) ---
    pub pre_nodes: Vec<PalwStepNodeV1>,
    pub gdn_nodes: Vec<PalwStepNodeV1>,
    pub attn_nodes: Vec<PalwStepNodeV1>,
    pub post_nodes: Vec<PalwStepNodeV1>,

    // --- adjudication bindings ---
    /// The arithmetic the kernel programs are written in (ruleset v2 or later).
    pub reference_ruleset_id: Hash64,
    pub transcendental_bindings: Vec<PalwTranscendentalBindingV1>,
    pub contraction_facts: Vec<PalwContractionFactV1>,

    // --- aux series (ADR-0030 §3) ---
    /// Calls per KV aux-chunk leaf; 0 disables the series (then full-context reductions open
    /// per-call leaves via chunked carriage instead).
    pub kv_chunk_calls: u32,
    /// The measured checkpoint state chunk geometry (opaque until registration).
    pub state_chunk_map_id: Hash64,
}

impl PalwShapeProfileV3 {
    /// **The half of `validate_shape` that is about a MODEL, not about an adjudicable graph**
    /// (ADR-0051).
    ///
    /// Version, layer count, context, dimensions, the pinned flash-attention rule, thread count —
    /// every one of these is true or false about the thing being run, whoever verifies it and
    /// however. What follows them in `validate_shape` is a different subject: whether the declared
    /// NODE TABLES describe a graph a court can walk.
    ///
    /// A Family-M class has no such graph and says so with empty tables, which the node-table
    /// rules correctly refuse and which is correct *for the family they were written for*. So the
    /// non-court admission gate validates the geometry and stops, exactly as it skips the ladder
    /// depth and the opening cost. Family D still runs the whole of `validate_shape`, which is
    /// still what refuses a class with nothing to adjudicate.
    pub fn validate_geometry(&self) -> Result<(), PalwStepError> {
        use PalwStepError::ProfileNotCanonical as bad;
        if self.version != PALW_STEP_OBJECT_VERSION_V1 {
            return Err(PalwStepError::UnsupportedVersion { got: self.version, expected: PALW_STEP_OBJECT_VERSION_V1 });
        }
        if self.layer_count == 0 {
            return Err(bad("layer count is zero"));
        }
        if self.layer_count > PALW_STEP_MAX_LAYERS {
            return Err(bad("layer count exceeds the adjudicable ceiling"));
        }
        if self.n_ctx == 0 {
            return Err(bad("context length is zero"));
        }
        if (self.n_ctx as u64).saturating_mul(self.layer_count as u64) > PALW_STEP_MAX_ENUMERATION {
            return Err(bad("the declared shape drives an enumeration past the work ceiling"));
        }
        if self.hidden_dim == 0 || self.vocab_size == 0 {
            return Err(bad("zero geometry dimension"));
        }
        if self.flash_attn_disabled != 1 {
            return Err(bad("flash attention must be pinned disabled (ADR-0030 Fact 7)"));
        }
        if self.n_threads == 0 {
            return Err(bad("thread count is zero"));
        }
        // **The invariants the adjudicator's arithmetic assumes, refused HERE instead of discovered
        // in a dispute** (audit 2026-08-26, finding 4).
        //
        // The attention arms decompose an output index into a head and slice the KV cache at
        // `(head / group) * head_dim`, so a zero on either factor makes that arithmetic meaningless
        // before any cache exists.
        //
        // **What is NOT enforced, deliberately, and against the audit's own suggested fix:**
        // `attn_heads * attn_head_dim == hidden_dim`. It is true of every model this tree ships
        // (Qwen2.5-1.5B is 12 x 128 = 1536) and it is NOT a universal architectural identity — an
        // attention projection need not be square with the residual width, and the adjudicator's
        // own fixtures exercise 32 / 1 / 16 through paths that adjudicate correctly. Refusing that
        // profile would refuse classes this court can already judge. The reachable defect the
        // identity was proposed against — an out-of-range KV slice inside block validation — is
        // closed where it actually lives, by making those arms total (`get(..).ok_or(..)`), which
        // is a refusal of the DISPUTE rather than a refusal of the class.
        if self.attn_heads == 0 || self.attn_head_dim == 0 {
            return Err(bad("zero attention geometry"));
        }
        // `base0_rms_eps_q` is added to a Qk mean of squares in `i64` under `overflow-checks`, so a
        // registrant-chosen epsilon anywhere near the type's range is a panic they choose. An
        // epsilon is a small stabiliser by definition; a negative one is not an epsilon at all.
        if self.base0_rms_eps_q < 0 || self.base0_rms_eps_q > PALW_STEP_MAX_RMS_EPS_Q {
            return Err(bad("the rms epsilon is outside the adjudicable range"));
        }
        Ok(())
    }

    pub fn validate_shape(&self) -> Result<(), PalwStepError> {
        use PalwStepError::ProfileNotCanonical as bad;
        self.validate_geometry()?;
        if self.version != PALW_STEP_OBJECT_VERSION_V1 {
            return Err(PalwStepError::UnsupportedVersion { got: self.version, expected: PALW_STEP_OBJECT_VERSION_V1 });
        }
        if self.layer_count == 0 {
            return Err(bad("layer count is zero"));
        }
        if self.layer_count > PALW_STEP_MAX_LAYERS {
            return Err(bad("layer count exceeds the adjudicable ceiling"));
        }
        if self.n_ctx == 0 {
            return Err(bad("context length is zero"));
        }
        // **The work bound, not the answer bound.** See `PALW_STEP_MAX_ENUMERATION`: the leaf
        // enumeration is driven by this product, and it is computed BEFORE anything can compare
        // against the leaf cap.
        if (self.n_ctx as u64).saturating_mul(self.layer_count as u64) > PALW_STEP_MAX_ENUMERATION {
            return Err(bad("the declared shape drives an enumeration past the work ceiling"));
        }
        if self.hidden_dim == 0 || self.vocab_size == 0 {
            return Err(bad("zero geometry dimension"));
        }
        if self.flash_attn_disabled != 1 {
            return Err(bad("flash attention must be pinned disabled (ADR-0030 Fact 7)"));
        }
        if self.n_threads == 0 {
            return Err(bad("thread count is zero"));
        }
        let has_attention = self.attention_layer_exists();
        if has_attention && self.attn_nodes.is_empty() {
            return Err(bad("attention layers exist but the attention node table is empty"));
        }
        if !self.gdn_layer_exists() && !self.gdn_nodes.is_empty() {
            return Err(bad("no GDN layers but a GDN node table is declared"));
        }
        if self.gdn_layer_exists() && self.gdn_nodes.is_empty() {
            return Err(bad("GDN layers exist but the GDN node table is empty"));
        }
        if self.post_nodes.is_empty() {
            return Err(bad("post table must at least produce logits"));
        }
        for (name, which, table) in [
            ("pre", PalwStepTableV1::Pre, &self.pre_nodes),
            ("gdn", PalwStepTableV1::Gdn, &self.gdn_nodes),
            ("attn", PalwStepTableV1::Attn, &self.attn_nodes),
            ("post", PalwStepTableV1::Post, &self.post_nodes),
        ] {
            let table_layer_span = self.table_layer_span(which);
            if table.len() > PALW_STEP_MAX_NODES_PER_TABLE {
                let _ = name;
                return Err(bad("node table exceeds the per-table cap"));
            }
            for (node_pos, node) in table.iter().enumerate() {
                for &r in &node.input_refs {
                    if r >= PALW_STEP_INPUT_SENTINEL_MIN {
                        if !matches!(
                            r,
                            PALW_STEP_INPUT_LAYER_IN | PALW_STEP_INPUT_KV_K | PALW_STEP_INPUT_KV_V | PALW_STEP_INPUT_CHECKPOINT_STATE
                        ) {
                            return Err(bad("unknown input sentinel"));
                        }
                    } else if r as usize >= node_pos {
                        // Intra-table inputs must point strictly earlier: the graph is a DAG
                        // in template order, and a forward or self reference would let a
                        // committed "input" be defined by the output it explains.
                        return Err(bad("node input does not point strictly earlier in the table"));
                    }
                }
                if node.tile_len < PALW_STEP_MIN_TILE_LEN || node.tile_len > PALW_STEP_MAX_TILE_LEN {
                    return Err(bad("node tile length is out of bounds"));
                }
                match node.out_len {
                    PalwStepOutLenV1::Fixed { elements } => {
                        if elements == 0 {
                            return Err(bad("node output length is zero"));
                        }
                    }
                    PalwStepOutLenV1::KvScaled { multiplier } => {
                        if multiplier == 0 {
                            return Err(bad("kv-scaled multiplier is zero"));
                        }
                        if !has_attention {
                            return Err(bad("kv-scaled node in a graph with no attention layers"));
                        }
                    }
                }
                if node.weight_name.is_empty() != node.weight_dtypes.is_empty() {
                    return Err(bad("weight name and dtypes must be both present or both absent"));
                }
                // One byte per layer the table covers, and never a zero: a zero dtype names no
                // GGML type, so it is the "unset" value, and an unset entry inside a non-empty
                // list is a layer whose arithmetic nobody declared.
                if !node.weight_dtypes.is_empty() {
                    let expected = table_layer_span;
                    if node.weight_dtypes.len() != expected {
                        return Err(bad("a node's dtype list must carry exactly one byte per layer its table covers"));
                    }
                    if node.weight_dtypes.contains(&0) {
                        return Err(bad("a zero dtype names no GGML type — every covered layer must declare one"));
                    }
                }
                if node.weight_name.len() > 128 {
                    return Err(bad("weight name exceeds the cap"));
                }
                if node.input_refs.len() > 8 {
                    return Err(bad("node declares more than 8 inputs"));
                }
            }
        }
        if self.kv_chunk_calls > PALW_STEP_MAX_KV_CHUNK_CALLS {
            return Err(bad("kv chunk width exceeds the cap"));
        }
        if self.kv_chunk_calls != 0 && !has_attention {
            return Err(bad("kv aux series declared without attention layers"));
        }
        Ok(())
    }

    pub fn attention_layer_exists(&self) -> bool {
        self.full_attention_interval != 0 && (self.layer_count as u32) >= self.full_attention_interval as u32
    }

    pub fn gdn_layer_exists(&self) -> bool {
        self.full_attention_interval != 1 || self.layer_count == 0
    }

    /// How many layers a given node table covers — which is how many dtype bytes each of its
    /// nodes must carry. The pre/post tables run once for the whole graph; the per-layer tables
    /// run once per layer OF THEIR KIND, which on the pinned Qwen3.5-2B is 6 attention layers and
    /// 18 GatedDeltaNet layers out of 24.
    pub fn table_layer_span(&self, table: PalwStepTableV1) -> usize {
        match table {
            PalwStepTableV1::Pre | PalwStepTableV1::Post => 1,
            PalwStepTableV1::Attn => (0..self.layer_count).filter(|l| self.layer_kind(*l) == PalwLayerKindV1::Attention).count(),
            PalwStepTableV1::Gdn => (0..self.layer_count).filter(|l| self.layer_kind(*l) == PalwLayerKindV1::GatedDeltaNet).count(),
        }
    }

    /// Layer kind under the pinned rule (Fact 1). `layer` must be `< layer_count`.
    pub fn layer_kind(&self, layer: u16) -> PalwLayerKindV1 {
        if self.full_attention_interval != 0 && (layer as u32 + 1).is_multiple_of(self.full_attention_interval as u32) {
            PalwLayerKindV1::Attention
        } else {
            PalwLayerKindV1::GatedDeltaNet
        }
    }

    pub(crate) fn layer_table(&self, layer: u16) -> &[PalwStepNodeV1] {
        match self.layer_kind(layer) {
            PalwLayerKindV1::GatedDeltaNet => &self.gdn_nodes,
            PalwLayerKindV1::Attention => &self.attn_nodes,
        }
    }

    /// Global node slots: pre ‖ layer 0 ‖ layer 1 ‖ … ‖ post, each layer expanding to its
    /// kind's table. The slot count is a pure profile fact.
    pub fn global_node_count(&self) -> u32 {
        let mut n = self.pre_nodes.len() as u32 + self.post_nodes.len() as u32;
        for layer in 0..self.layer_count {
            n += self.layer_table(layer).len() as u32;
        }
        n
    }

    /// Resolves a global node slot to `(its node, the layer it belongs to)` (`None` layer for
    /// pre/post nodes).
    /// **The inverse of [`Self::resolve_node_slot`]** — where a capture must PUT the row it just
    /// produced.
    ///
    /// An executor knows what it computed by table and index ("the second post node"); a leaf
    /// index is derived from a global slot. Something has to convert, and until this existed the
    /// only converter was arithmetic written out at the call site — which is how a capture came to
    /// place every layer's rows on top of layer 0's. Walked the same way `resolve_node_slot`
    /// walks, so the two cannot drift; `the_slot_walk_inverts_itself` asserts the round trip over
    /// every slot of a real profile.
    ///
    /// `layer` is ignored for `Pre`/`Post`, which have no layer.
    pub fn global_node_slot(&self, table: PalwStepTableV1, layer: u16, index: usize) -> Option<u32> {
        match table {
            PalwStepTableV1::Pre => (index < self.pre_nodes.len()).then_some(index as u32),
            PalwStepTableV1::Attn | PalwStepTableV1::Gdn => {
                if layer >= self.layer_count {
                    return None;
                }
                // The layer's own table decides its width, and it must be the table the caller
                // named: a row captured as `Attn` in a GatedDeltaNet layer is a row about a
                // different graph.
                let expected = match self.layer_kind(layer) {
                    PalwLayerKindV1::Attention => PalwStepTableV1::Attn,
                    PalwLayerKindV1::GatedDeltaNet => PalwStepTableV1::Gdn,
                };
                if expected != table || index >= self.layer_table(layer).len() {
                    return None;
                }
                let mut cursor = self.pre_nodes.len() as u32;
                for l in 0..layer {
                    cursor = cursor.checked_add(self.layer_table(l).len() as u32)?;
                }
                cursor.checked_add(index as u32)
            }
            PalwStepTableV1::Post => {
                if index >= self.post_nodes.len() {
                    return None;
                }
                let mut cursor = self.pre_nodes.len() as u32;
                for l in 0..self.layer_count {
                    cursor = cursor.checked_add(self.layer_table(l).len() as u32)?;
                }
                cursor.checked_add(index as u32)
            }
        }
    }

    pub fn resolve_node_slot(&self, slot: u32) -> Option<(&PalwStepNodeV1, Option<u16>)> {
        let mut cursor = slot;
        if (cursor as usize) < self.pre_nodes.len() {
            return Some((&self.pre_nodes[cursor as usize], None));
        }
        cursor -= self.pre_nodes.len() as u32;
        for layer in 0..self.layer_count {
            let table = self.layer_table(layer);
            if (cursor as usize) < table.len() {
                return Some((&table[cursor as usize], Some(layer)));
            }
            cursor -= table.len() as u32;
        }
        self.post_nodes.get(cursor as usize).map(|n| (n, None))
    }

    /// Every `kernel_semantics_id` a step of this profile can be adjudicated under — the
    /// reachable set ADR-0038 A4's coverage rule is about.
    ///
    /// Walked through the SAME `global_node_count`/`resolve_node_slot` pair the court walks
    /// (`palw_step_refute`'s `resolve_kernel(&node.kernel_semantics_id)`), deliberately not a
    /// second opinion about reachability: if these two ever disagreed, coverage would certify
    /// a set that is not the set the court looks up, which is the failure mode the rule exists
    /// to prevent.
    ///
    /// A declared-but-unreachable node table is therefore excluded on purpose — an
    /// `attn_nodes` table in a graph with no attention layers contributes nothing, because the
    /// court can never resolve a slot in it either.
    pub fn reachable_kernel_ids_v1(&self) -> std::collections::BTreeSet<Hash64> {
        (0..self.global_node_count())
            .filter_map(|slot| self.resolve_node_slot(slot))
            .map(|(node, _)| node.kernel_semantics_id)
            .collect()
    }

    /// `shape_profile_id` (v3): the canonical Borsh bytes under the v3 domain. Borsh is the
    /// wire encoding of every field above (enums carry their frozen discriminants), so the
    /// preimage and the wire object cannot drift apart.
    pub fn shape_profile_id(&self) -> Hash64 {
        let bytes = borsh::to_vec(self).expect("borsh serialization of an owned struct cannot fail");
        keyed64(PALW_STEP_DOMAIN_SHAPE_PROFILE_V3, &[&bytes])
    }
}

// ---------------------------------------------------------------------------------------------
// The step space — enumeration and bijection
// ---------------------------------------------------------------------------------------------

/// Coordinates of one step (ADR-0030 §1). `call_index` 0 is the prefill call (positions
/// `0..P`); calls `1..D` are decode calls (single position). The KV length of `(call 0, p)`
/// is `p+1`; of decode call `c` it is `P+c`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct PalwStepCoordinateV1 {
    pub call_index: u32,
    pub node_slot: u32,
    pub position: u32,
    pub tile_index: u32,
}

fn tiles_for(len: u64, tile_len: u32) -> u64 {
    len.div_ceil(tile_len as u64)
}

fn node_out_len(node: &PalwStepNodeV1, kv_len: u64) -> u64 {
    match node.out_len {
        PalwStepOutLenV1::Fixed { elements } => elements as u64,
        PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * kv_len,
    }
}

/// Leaves contributed by one (call, position) pair across all global node slots.
fn leaves_per_position(profile: &PalwShapeProfileV3, kv_len: u64, with_logits: bool) -> u64 {
    let mut leaves = 0u64;
    for node in &profile.pre_nodes {
        leaves += tiles_for(node_out_len(node, kv_len), node.tile_len);
    }
    for layer in 0..profile.layer_count {
        for node in profile.layer_table(layer) {
            leaves += tiles_for(node_out_len(node, kv_len), node.tile_len);
        }
    }
    if with_logits {
        for node in &profile.post_nodes {
            leaves += tiles_for(node_out_len(node, kv_len), node.tile_len);
        }
    }
    leaves
}

/// KV aux-chunk leaves for the whole job (ADR-0030 §3): per attention layer, per kv head,
/// per {K, V}, `ceil(total_positions / kv_chunk_calls)` leaves.
pub fn kv_aux_leaf_count(profile: &PalwShapeProfileV3, context: &PalwJobContextV2) -> u64 {
    if profile.kv_chunk_calls == 0 {
        return 0;
    }
    let attn_layers = (0..profile.layer_count).filter(|&l| profile.layer_kind(l) == PalwLayerKindV1::Attention).count() as u64;
    let positions = context.declared_prefill_tokens as u64 + context.exact_decode_tokens.saturating_sub(1) as u64;
    attn_layers * profile.attn_kv_heads as u64 * 2 * positions.div_ceil(profile.kv_chunk_calls as u64)
}

/// The largest leaf count this profile can ever produce — the class's worst case, from shape alone.
///
/// The longest job a class admits is its whole context as prefill with one decode call, so this is
/// [`step_leaf_count`] at that point. Callers that need "can this class be adjudicated at all"
/// (`palw_schedule::class_is_adjudicable_v1`) want exactly this and nothing job-specific: admitting
/// a class whose TYPICAL job fits the ladder while its longest does not is admitting a class an
/// attacker chooses the job length for.
pub fn worst_case_step_leaf_count_v1(profile: &PalwShapeProfileV3) -> Result<u64, PalwStepError> {
    // **Validate BEFORE enumerating.** This walk is driven by `n_ctx` and `layer_count`, so a
    // shape that has not been bounded yet decides how long it runs. `step_leaf_count` has always
    // validated first; this sibling did not, and it is the one an unadmitted class reaches.
    profile.validate_shape()?;
    let prefill = profile.n_ctx.saturating_sub(1);
    let mut total = 0u64;
    for p in 0..prefill as u64 {
        total = total.saturating_add(leaves_per_position(profile, p + 1, p + 1 == prefill as u64));
        // Inside the loop, not after it: a cap tested at the end is an answer bound that has
        // already paid the whole cost of the answer.
        if total > PALW_STEP_MAX_LEAVES {
            return Err(PalwStepError::TooManyLeaves { got: total, max: PALW_STEP_MAX_LEAVES });
        }
    }
    // One decode call at the far end of the context, matching `step_leaf_count`'s own enumeration.
    total = total.saturating_add(leaves_per_position(profile, prefill as u64 + 1, true));
    if total > PALW_STEP_MAX_LEAVES {
        return Err(PalwStepError::TooManyLeaves { got: total, max: PALW_STEP_MAX_LEAVES });
    }
    Ok(total)
}

/// Total step-leg leaves for `(profile, context)`: the main enumeration then the aux series.
/// Errors when the job shape exceeds the cap.
pub fn step_leaf_count(profile: &PalwShapeProfileV3, context: &PalwJobContextV2) -> Result<u64, PalwStepError> {
    profile.validate_shape()?;
    let prefill = context.declared_prefill_tokens as u64;
    let decode_calls = context.exact_decode_tokens.saturating_sub(1) as u64;
    let mut total = 0u64;
    // Prefill call: per position p, kv_len = p+1; logits only at the last position.
    for p in 0..prefill {
        total = total.saturating_add(leaves_per_position(profile, p + 1, p + 1 == prefill));
        if total > PALW_STEP_MAX_LEAVES {
            return Err(PalwStepError::TooManyLeaves { got: total, max: PALW_STEP_MAX_LEAVES });
        }
    }
    // Decode calls c = 1..=decode_calls: kv_len = prefill + c, logits always.
    for c in 1..=decode_calls {
        total = total.saturating_add(leaves_per_position(profile, prefill + c, true));
        if total > PALW_STEP_MAX_LEAVES {
            return Err(PalwStepError::TooManyLeaves { got: total, max: PALW_STEP_MAX_LEAVES });
        }
    }
    total += kv_aux_leaf_count(profile, context);
    if total > PALW_STEP_MAX_LEAVES {
        return Err(PalwStepError::TooManyLeaves { got: total, max: PALW_STEP_MAX_LEAVES });
    }
    Ok(total)
}

/// The pinned enumeration: main leaves ordered call-major → position → global node slot →
/// tile; the KV aux series appended after all main leaves, ordered (attention layer, kv head,
/// K then V, chunk). Returns the coordinates of a main leaf, or `None` for aux leaves (they
/// have their own coordinate space) and out-of-range indices.
pub fn canonical_step_coordinates(
    profile: &PalwShapeProfileV3,
    context: &PalwJobContextV2,
    leaf_index: u64,
) -> Option<PalwStepCoordinateV1> {
    let prefill = context.declared_prefill_tokens as u64;
    let decode_calls = context.exact_decode_tokens.saturating_sub(1) as u64;
    let mut cursor = leaf_index;
    for call in 0..=decode_calls {
        let positions = if call == 0 { prefill } else { 1 };
        for p in 0..positions {
            let kv_len = if call == 0 { p + 1 } else { prefill + call };
            let with_logits = if call == 0 { p + 1 == prefill } else { true };
            let here = leaves_per_position(profile, kv_len, with_logits);
            if cursor >= here {
                cursor -= here;
                continue;
            }
            // Inside this position: walk global slots.
            let slot_count = profile.global_node_count();
            for slot in 0..slot_count {
                let (node, _layer) = profile.resolve_node_slot(slot).expect("slot < count");
                let is_post = slot >= slot_count - profile.post_nodes.len() as u32;
                if is_post && !with_logits {
                    continue;
                }
                let tiles = tiles_for(node_out_len(node, kv_len), node.tile_len);
                if cursor < tiles {
                    return Some(PalwStepCoordinateV1 {
                        call_index: call as u32,
                        node_slot: slot,
                        position: p as u32,
                        tile_index: cursor as u32,
                    });
                }
                cursor -= tiles;
            }
            unreachable!("leaves_per_position and the slot walk disagree");
        }
    }
    None // aux territory or out of range
}

/// The inverse: rank of canonical coordinates in the pinned enumeration. `None` when the
/// coordinates are not canonical for `(profile, context)` — which, on a committed leaf, is
/// itself the fault (the legs-v1 discipline).
pub fn canonical_step_leaf_index(
    profile: &PalwShapeProfileV3,
    context: &PalwJobContextV2,
    coord: &PalwStepCoordinateV1,
) -> Option<u64> {
    let prefill = context.declared_prefill_tokens as u64;
    let decode_calls = context.exact_decode_tokens.saturating_sub(1) as u64;
    let call = coord.call_index as u64;
    if call > decode_calls {
        return None;
    }
    let positions = if call == 0 { prefill } else { 1 };
    if (coord.position as u64) >= positions {
        return None;
    }
    let mut index = 0u64;
    // Whole calls before this one.
    for c in 0..call {
        let ps = if c == 0 { prefill } else { 1 };
        for p in 0..ps {
            let kv_len = if c == 0 { p + 1 } else { prefill + c };
            index += leaves_per_position(profile, kv_len, if c == 0 { p + 1 == prefill } else { true });
        }
    }
    // Whole positions before this one within the call (prefill only).
    for p in 0..coord.position as u64 {
        index += leaves_per_position(profile, p + 1, p + 1 == prefill);
    }
    let kv_len = if call == 0 { coord.position as u64 + 1 } else { prefill + call };
    let with_logits = if call == 0 { coord.position as u64 + 1 == prefill } else { true };
    // Slots before this one within the position.
    let slot_count = profile.global_node_count();
    if coord.node_slot >= slot_count {
        return None;
    }
    for slot in 0..coord.node_slot {
        let (node, _) = profile.resolve_node_slot(slot)?;
        let is_post = slot >= slot_count - profile.post_nodes.len() as u32;
        if is_post && !with_logits {
            continue;
        }
        index += tiles_for(node_out_len(node, kv_len), node.tile_len);
    }
    let (node, _) = profile.resolve_node_slot(coord.node_slot)?;
    let is_post = coord.node_slot >= slot_count - profile.post_nodes.len() as u32;
    if is_post && !with_logits {
        return None; // post nodes do not exist at non-logit positions
    }
    let tiles = tiles_for(node_out_len(node, kv_len), node.tile_len);
    if (coord.tile_index as u64) >= tiles {
        return None;
    }
    Some(index + coord.tile_index as u64)
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_carriage::PALW_CARRIAGE_ALL_DOMAINS;
    use crate::palw_legs::PALW_LEGS_ALL_DOMAINS;
    use crate::palw_reference::PALW_REFERENCE_ALL_DOMAINS;
    use crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS;
    use crate::palw_slash::PALW_S_ALL_DOMAINS;
    use crate::palw_v2::PALW_V2_ALL_DOMAINS;

    fn h64(fill: u8) -> Hash64 {
        Hash64::from_bytes([fill; 64])
    }

    /// **One ~5 KB object could stop every node forever, and the cap did not stop it.**
    ///
    /// `worst_case_step_leaf_count_v1` walks `n_ctx` positions and `layer_count` layers at each
    /// before comparing the total to `PALW_STEP_MAX_LEAVES`. With both fields unbounded, a
    /// `ClassRegistered` naming `n_ctx = u32::MAX` and `layer_count = u16::MAX` buys ≈2.8e14
    /// iterations from every node, for every chain candidate, on a block isolation validation has
    /// already stored — so it reproduces on restart and on resync, and nothing short of a hard
    /// fork recovers.
    ///
    /// The bound has to be on the WORK, not on the answer: a cap tested after the enumeration has
    /// already paid for the enumeration. Timed rather than merely asserted, because a regression
    /// here does not fail — it hangs, and a hanging test looks like a slow machine.
    #[test]
    fn an_unbounded_shape_cannot_buy_an_unbounded_enumeration() {
        let mut profile = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the shipped floor profile is well-formed");
        assert!(profile.validate_shape().is_ok(), "the real class must still pass");
        assert!(worst_case_step_leaf_count_v1(&profile).is_ok(), "the real class must still enumerate");

        profile.n_ctx = u32::MAX;
        profile.layer_count = u16::MAX;
        let started = std::time::Instant::now();
        let refused = worst_case_step_leaf_count_v1(&profile);
        let took = started.elapsed();
        assert!(refused.is_err(), "an unbounded shape must be refused, not enumerated");
        assert!(
            took < std::time::Duration::from_millis(200),
            "refusing the shape took {took:?} — it is being enumerated before it is refused"
        );
    }

    /// The product is the thing an attacker buys, so it is the thing that is bounded. Each factor
    /// alone can look reasonable while the product does not.
    #[test]
    fn the_enumeration_ceiling_binds_the_product_not_each_factor() {
        let mut profile = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the shipped floor profile is well-formed");
        let layers = profile.layer_count as u64;
        assert!(layers > 0);
        // Just over the ceiling, at this profile's real layer count.
        profile.n_ctx = ((PALW_STEP_MAX_ENUMERATION / layers) + 1) as u32;
        assert!(profile.validate_shape().is_err(), "a context that puts the product past the ceiling must be refused");
        // Just under it — the factor alone is enormous, and that is fine, because the product is not.
        profile.n_ctx = (PALW_STEP_MAX_ENUMERATION / layers) as u32;
        assert!(profile.validate_shape().is_ok(), "a huge context is admissible while the product fits");
    }

    fn node(kind: PalwStepOpKindV1, out: PalwStepOutLenV1, tile: u32) -> PalwStepNodeV1 {
        PalwStepNodeV1 {
            op_kind: kind,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: String::new(),
            weight_dtypes: Vec::new(),
            out_len: out,
            tile_len: tile,
            kernel_semantics_id: h64(0x11),
            input_refs: vec![PALW_STEP_INPUT_LAYER_IN],
        }
    }

    /// A tiny synthetic profile: 3 layers, interval 2 → kinds [GDN, Attention, GDN]. Small
    /// enough for exhaustive bijection checks, still exercising fixed + kv-scaled + ragged
    /// tiles and the logits-only-post rule.
    fn tiny_profile() -> PalwShapeProfileV3 {
        PalwShapeProfileV3 {
            version: PALW_STEP_OBJECT_VERSION_V1,
            lane: crate::palw_step::PalwStepLaneV1::Float32,
            layer_count: 3,
            full_attention_interval: 2,
            hidden_dim: 8,
            ffn_dim: 16,
            attn_heads: 2,
            attn_kv_heads: 1,
            attn_head_dim: 4,
            rope_dims: 2,
            rope_sections: [1, 1, 0, 0],
            rope_freq_base_bits: 0x4CBE_BC20, // 1e8f — a bit pattern, never a float
            rms_eps_bits: 0x358637BD,
            base0_rms_eps_q: 1 << 8,
            l2_eps_bits: 0x358637BD,
            gdn_heads: 2,
            gdn_head_k_dim: 4,
            gdn_head_v_dim: 4,
            gdn_conv_kernel: 4,
            vocab_size: 40,
            repack_on: 1,
            llamafile_on: 1,
            flash_attn_disabled: 1,
            fused_gdn_on: 1,
            use_ref_off: 1,
            kv_cache_f16: 1,
            n_ctx: 64,
            n_batch: 64,
            n_ubatch: 64,
            n_seq: 1,
            n_threads: 4,
            pre_nodes: vec![node(PalwStepOpKindV1::EmbedLookup, PalwStepOutLenV1::Fixed { elements: 8 }, 16)],
            gdn_nodes: vec![
                node(PalwStepOpKindV1::RmsNorm, PalwStepOutLenV1::Fixed { elements: 8 }, 16),
                node(PalwStepOpKindV1::MatMulQuant, PalwStepOutLenV1::Fixed { elements: 24 }, 16), // ragged: 2 tiles
                node(PalwStepOpKindV1::GatedDeltaNet, PalwStepOutLenV1::Fixed { elements: 8 }, 16),
            ],
            attn_nodes: vec![
                node(PalwStepOpKindV1::RmsNorm, PalwStepOutLenV1::Fixed { elements: 8 }, 16),
                node(PalwStepOpKindV1::MatMulF16, PalwStepOutLenV1::KvScaled { multiplier: 2 }, 16),
                node(PalwStepOpKindV1::SoftMax, PalwStepOutLenV1::KvScaled { multiplier: 2 }, 16),
                node(PalwStepOpKindV1::MatMulQuant, PalwStepOutLenV1::Fixed { elements: 8 }, 16),
            ],
            post_nodes: vec![
                node(PalwStepOpKindV1::RmsNorm, PalwStepOutLenV1::Fixed { elements: 8 }, 16),
                node(PalwStepOpKindV1::MatMulQuant, PalwStepOutLenV1::Fixed { elements: 40 }, 16), // logits: 3 tiles
            ],
            reference_ruleset_id: h64(0x22),
            transcendental_bindings: vec![PalwTranscendentalBindingV1 {
                site: PalwTranscendentalSiteV1::VectorExpPolynomial,
                algorithm_id: h64(0x33),
            }],
            contraction_facts: vec![PalwContractionFactV1 { site: PalwContractionSiteV1::RopeRotate, contracted: 1 }],
            kv_chunk_calls: 4,
            state_chunk_map_id: h64(0x44),
        }
    }

    fn tiny_context() -> PalwJobContextV2 {
        PalwJobContextV2 {
            version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"step-test".to_vec(),
            job_id: h64(1),
            job_nullifier: h64(2),
            assignment_id: h64(3),
            execution_seed: [7; 32],
            model_profile_id: h64(4),
            runtime_manifest_hash: h64(5),
            runtime_class_id: h64(6),
            shape_profile_id: h64(7),
            trace_scheme_id: h64(8),
            cu_ruleset_id: h64(9),
            tokenizer_id: h64(10),
            prompt_token_ids_hash: h64(11),
            declared_prefill_tokens: 3,
            exact_decode_tokens: 4, // 1 prefill call + 3 decode calls
            max_context_tokens: 64,
        }
    }

    #[test]
    fn step_domains_are_unique_across_all_palw_modules() {
        let mut seen = std::collections::HashSet::new();
        for d in PALW_STEP_ALL_DOMAINS {
            assert!(seen.insert(*d), "duplicate step domain");
            assert!(d.len() <= 64, "blake2b key cap exceeded");
        }
        for d in PALW_V2_ALL_DOMAINS
            .iter()
            .chain(PALW_S_ALL_DOMAINS.iter())
            .chain(PALW_LEGS_ALL_DOMAINS.iter())
            .chain(PALW_REFERENCE_ALL_DOMAINS.iter())
            .chain(PALW_SCHEDULE_ALL_DOMAINS.iter())
            .chain(PALW_CARRIAGE_ALL_DOMAINS.iter())
        {
            assert!(!seen.contains(d), "step module reuses a foreign domain: {}", String::from_utf8_lossy(d));
        }
    }

    /// **The pinned model's real dtype variance, and why one byte per node could not hold it.**
    ///
    /// Measured 2026-08-20 by reading the pinned `Qwen3.5-2B-Q4_K_M.gguf`'s tensor table
    /// directly. The numbers are the finding, not decoration: `ffn_down.weight` is `Q6_K` on
    /// twelve of the twenty-four layers and `Q4_K` on the other twelve, in an order the
    /// quantizer's imatrix heuristics chose — no rule a profile could restate. `attn_v.weight`
    /// splits 4/2 across the six attention layers the same way.
    ///
    /// The old `weight_dtype: u8` claimed in its own doc to bind "per-layer dtype variance". It
    /// could not: one byte per node describes one dtype for every layer that node covers. A
    /// profile written into it would have declared `Q4_K` where the file holds `Q6_K`, and a
    /// court recomputing that node would convict an honest producer on every layer where the two
    /// disagree — dequantization block layout and accumulation order are not the same between
    /// them.
    ///
    /// So this test does two things: it fixes the shape of the fix (one byte per covered layer,
    /// never zero), and it records the measurement that forced it, so a later simplification back
    /// to a scalar has to argue with the model rather than with a preference.
    #[test]
    fn a_nodes_dtypes_are_one_per_covered_layer_because_the_pinned_model_varies_them() {
        // The pinned model's shape, as measured: 24 layers, attention every 4th.
        let mut p = tiny_profile();
        p.layer_count = 24;
        p.full_attention_interval = 4;
        assert_eq!(p.table_layer_span(PalwStepTableV1::Attn), 6, "layers 3,7,11,15,19,23 are attention");
        assert_eq!(p.table_layer_span(PalwStepTableV1::Gdn), 18, "the other eighteen are GatedDeltaNet");
        assert_eq!(p.table_layer_span(PalwStepTableV1::Pre), 1, "the pre table runs once for the graph");
        assert_eq!(p.table_layer_span(PalwStepTableV1::Post), 1);

        // `ffn_down.weight` as the file really carries it: Q6_K on twelve layers, Q4_K on twelve.
        const Q4_K: u8 = 12;
        const Q6_K: u8 = 14;
        let ffn_down_by_layer: Vec<u8> =
            (0..24u16).map(|l| if [0, 1, 2, 5, 8, 11, 14, 17, 20, 21, 22, 23].contains(&l) { Q6_K } else { Q4_K }).collect();
        assert_eq!(ffn_down_by_layer.iter().filter(|d| **d == Q6_K).count(), 12);
        assert_eq!(ffn_down_by_layer.iter().filter(|d| **d == Q4_K).count(), 12);
        assert!(
            ffn_down_by_layer.iter().collect::<std::collections::BTreeSet<_>>().len() > 1,
            "the whole point: a single byte cannot carry this"
        );

        // A GDN-table node covering the eighteen GDN layers needs eighteen bytes — not one, and
        // not twenty-four.
        let gdn_layers: Vec<u16> = (0..24u16).filter(|l| p.layer_kind(*l) == PalwLayerKindV1::GatedDeltaNet).collect();
        let mut ffn_down = node(PalwStepOpKindV1::MatMulQuant, PalwStepOutLenV1::Fixed { elements: 8 }, 16);
        ffn_down.weight_name = "blk.{layer}.ffn_down.weight".to_string();
        ffn_down.weight_dtypes = gdn_layers.iter().map(|l| ffn_down_by_layer[*l as usize]).collect();
        assert_eq!(ffn_down.weight_dtypes.len(), 18);
        p.gdn_nodes = vec![ffn_down.clone()];
        p.attn_nodes = vec![{
            let mut n = ffn_down.clone();
            n.weight_dtypes =
                (0..24u16).filter(|l| p.layer_kind(*l) == PalwLayerKindV1::Attention).map(|l| ffn_down_by_layer[l as usize]).collect();
            n
        }];
        p.validate_shape().expect("a profile whose dtype lists match its tables is well-formed");

        // One byte short, one byte long, and a zero entry: each is a layer whose arithmetic
        // nobody declared, or a declaration for a layer that does not exist.
        for wrong in [17usize, 19, 24] {
            let mut short = p.clone();
            short.gdn_nodes[0].weight_dtypes = vec![Q4_K; wrong];
            assert!(short.validate_shape().is_err(), "{wrong} bytes must not pass for 18 GDN layers");
        }
        let mut zeroed = p.clone();
        zeroed.gdn_nodes[0].weight_dtypes[7] = 0;
        assert!(zeroed.validate_shape().is_err(), "a zero dtype names no GGML type");

        // And a name without dtypes, or dtypes without a name, stay refused as before.
        let mut nameless = p.clone();
        nameless.gdn_nodes[0].weight_name = String::new();
        assert!(nameless.validate_shape().is_err());
    }

    #[test]
    fn shape_profile_id_golden_vector() {
        // Frozen 2026-08-16: the SCHEMA hash derivation (canonical borsh under the v3 domain)
        // over the synthetic test profile. This pins the encoding, not a network value — no
        // class profile exists until registration measures one.
        assert_eq!(
            tiny_profile().shape_profile_id().to_string(),
            // Re-frozen 2026-08-17: the profile gained `base0_rms_eps_q`, so its Borsh bytes — and
            // therefore its id — moved. That is the point of the field: a class cannot change the
            // epsilon its norms are adjudicated under without changing its identity.
            //
            // Re-frozen again 2026-08-20: `weight_dtype: u8` became `weight_dtypes: Vec<u8>`,
            // one byte per layer the node's table covers. The single byte could not describe the
            // pinned Qwen3.5-2B at all — `ffn_down.weight` is Q6_K on twelve layers and Q4_K on
            // the other twelve — so a profile written into the old type would have declared the
            // wrong arithmetic for half the layers it covered. Same rule as last time: a
            // consensus change to the identity gets a new value, never a silent re-reading.
            //
            //
            // Re-frozen once more the same day: the profile gained `lane`. A class that commits
            // int32 codes and one that commits f32 are not the same class, and both the leg
            // builder and the adjudicator read the field — so it belongs inside the identity.
            "24a01c7f90b50beab4ba53d4032119d494f5bebc671c75d271723de634f6fcbe\
             f98c86768d86d0039e3286f7a297d27a46cbbc4fb3ac5fd9dd682552ecf0845a"
        );
    }

    #[test]
    fn shape_profile_id_moves_with_every_field_class() {
        let base = tiny_profile().shape_profile_id();
        let mut p = tiny_profile();
        p.n_threads = 8;
        assert_ne!(p.shape_profile_id(), base, "thread count must move the id");
        let mut p = tiny_profile();
        p.repack_on = 0;
        assert_ne!(p.shape_profile_id(), base, "repack flag must move the id");
        let mut p = tiny_profile();
        p.gdn_nodes[1].kernel_semantics_id = h64(0x99);
        assert_ne!(p.shape_profile_id(), base, "a kernel id must move the id");
        let mut p = tiny_profile();
        p.gdn_nodes[1].tile_len = 32;
        assert_ne!(p.shape_profile_id(), base, "a tile length must move the id");
        let mut p = tiny_profile();
        p.contraction_facts[0].contracted = 0;
        assert_ne!(p.shape_profile_id(), base, "a contraction fact must move the id");
        let mut p = tiny_profile();
        p.rope_freq_base_bits ^= 1;
        assert_ne!(p.shape_profile_id(), base, "a geometry bit must move the id");
    }

    #[test]
    fn validation_rejects_the_named_hazards() {
        let mut p = tiny_profile();
        p.flash_attn_disabled = 0;
        assert!(matches!(p.validate_shape(), Err(PalwStepError::ProfileNotCanonical(_))));
        let mut p = tiny_profile();
        p.gdn_nodes[0].tile_len = 1; // below MIN
        assert!(p.validate_shape().is_err());
        let mut p = tiny_profile();
        p.attn_nodes.clear(); // attention layers exist but no table
        assert!(p.validate_shape().is_err());
        let mut p = tiny_profile();
        p.gdn_nodes[0].weight_name = "w".into(); // name without dtype
        assert!(p.validate_shape().is_err());
        let mut p = tiny_profile();
        p.gdn_nodes[0].input_refs = vec![0]; // self/forward reference at position 0
        assert!(p.validate_shape().is_err());
        let mut p = tiny_profile();
        p.gdn_nodes[1].input_refs = vec![1]; // self reference
        assert!(p.validate_shape().is_err());
        let mut p = tiny_profile();
        p.gdn_nodes[1].input_refs = vec![0]; // strictly earlier: fine
        assert!(p.validate_shape().is_ok());
        let mut p = tiny_profile();
        p.gdn_nodes[0].input_refs = vec![0xFFF5]; // unknown sentinel
        assert!(p.validate_shape().is_err());
        assert!(tiny_profile().validate_shape().is_ok());
    }

    #[test]
    fn layer_kinds_follow_the_interval_rule() {
        let p = tiny_profile();
        assert_eq!(p.layer_kind(0), PalwLayerKindV1::GatedDeltaNet);
        assert_eq!(p.layer_kind(1), PalwLayerKindV1::Attention);
        assert_eq!(p.layer_kind(2), PalwLayerKindV1::GatedDeltaNet);
        // The pinned model's rule: 24 layers, interval 4 → attention at 3, 7, 11, 15, 19, 23.
        let mut q = tiny_profile();
        q.layer_count = 24;
        q.full_attention_interval = 4;
        let attn: Vec<u16> = (0..24).filter(|&l| q.layer_kind(l) == PalwLayerKindV1::Attention).collect();
        assert_eq!(attn, vec![3, 7, 11, 15, 19, 23]);
    }

    #[test]
    fn step_bijection_is_exhaustive_on_the_tiny_job() {
        let p = tiny_profile();
        let ctx = tiny_context();
        let total = step_leaf_count(&p, &ctx).unwrap();
        let aux = kv_aux_leaf_count(&p, &ctx);
        assert!(aux > 0, "the tiny profile must exercise the aux series");
        let main = total - aux;
        let mut seen = std::collections::HashSet::new();
        for i in 0..main {
            let coord =
                canonical_step_coordinates(&p, &ctx, i).unwrap_or_else(|| panic!("main leaf {i} of {main} has no coordinates"));
            assert!(seen.insert(coord), "coordinate collision at {i}: {coord:?}");
            let back = canonical_step_leaf_index(&p, &ctx, &coord).unwrap_or_else(|| panic!("coordinates {coord:?} do not rank back"));
            assert_eq!(back, i, "bijection broke at {i}: {coord:?}");
        }
        // Aux and beyond have no main coordinates.
        assert_eq!(canonical_step_coordinates(&p, &ctx, main), None);
        assert_eq!(canonical_step_coordinates(&p, &ctx, total), None);
        assert_eq!(canonical_step_coordinates(&p, &ctx, total + 1), None);
    }

    #[test]
    fn non_canonical_coordinates_do_not_rank() {
        let p = tiny_profile();
        let ctx = tiny_context();
        // Post node at a non-logit prefill position.
        let post_slot = p.global_node_count() - 1;
        let bad = PalwStepCoordinateV1 { call_index: 0, node_slot: post_slot, position: 0, tile_index: 0 };
        assert_eq!(canonical_step_leaf_index(&p, &ctx, &bad), None);
        // …but the same slot at the last prefill position is canonical.
        let good = PalwStepCoordinateV1 { call_index: 0, node_slot: post_slot, position: 2, tile_index: 0 };
        assert!(canonical_step_leaf_index(&p, &ctx, &good).is_some());
        // Position out of range for a decode call.
        let bad = PalwStepCoordinateV1 { call_index: 1, node_slot: 0, position: 1, tile_index: 0 };
        assert_eq!(canonical_step_leaf_index(&p, &ctx, &bad), None);
        // Tile past the ragged end.
        let bad = PalwStepCoordinateV1 { call_index: 1, node_slot: 2, position: 0, tile_index: 99 };
        assert_eq!(canonical_step_leaf_index(&p, &ctx, &bad), None);
        // Call past the job.
        let bad = PalwStepCoordinateV1 { call_index: 4, node_slot: 0, position: 0, tile_index: 0 };
        assert_eq!(canonical_step_leaf_index(&p, &ctx, &bad), None);
    }

    #[test]
    fn kv_scaled_lengths_use_true_kv_never_padded() {
        // Fact 17: the scores row at prefill position p has kv_len = p+1 elements per
        // multiplier unit — visible as tile counts that grow with p.
        let p = tiny_profile();
        let ctx = tiny_context();
        // attn layer is layer 1; its MatMulF16 node is global slot: pre(1) + gdn(3) + 1 = 5.
        let scores_slot = 5;
        let (node, layer) = p.resolve_node_slot(scores_slot).unwrap();
        assert_eq!(node.op_kind, PalwStepOpKindV1::MatMulF16);
        assert_eq!(layer, Some(1));
        // At prefill position 0, kv_len 1: 2 elements → 1 tile. At decode call 3, kv_len 6:
        // 12 elements → 1 tile (tile 16). Grow the multiplier's effect via leaf counts:
        let idx_p0 = canonical_step_leaf_index(
            &p,
            &ctx,
            &PalwStepCoordinateV1 { call_index: 0, node_slot: scores_slot, position: 0, tile_index: 0 },
        );
        assert!(idx_p0.is_some());
        let no_second_tile = canonical_step_leaf_index(
            &p,
            &ctx,
            &PalwStepCoordinateV1 { call_index: 0, node_slot: scores_slot, position: 0, tile_index: 1 },
        );
        assert_eq!(no_second_tile, None, "kv_len 1 × mult 2 = 2 elements is one tile");
    }

    #[test]
    fn leaf_cap_is_enforced() {
        let p = tiny_profile();
        let mut ctx = tiny_context();
        ctx.declared_prefill_tokens = 4096;
        ctx.exact_decode_tokens = 4095;
        // 4096 prefill positions × (kv-scaled tiles growing to 8192) blows the 2^22 cap for
        // this synthetic profile? If not, push decode.
        match step_leaf_count(&p, &ctx) {
            Ok(n) => assert!(n <= PALW_STEP_MAX_LEAVES),
            Err(PalwStepError::TooManyLeaves { got, max }) => assert!(got > max),
            Err(e) => panic!("unexpected error {e:?}"),
        }
    }

    #[test]
    fn kernel_and_transcendental_ids_are_domain_separated() {
        let k = kernel_semantics_id_v1("repack-gemv/q4_K_8x4_q8_K/neon-dotprod/llama-030ebb558/v1");
        let t = transcendental_algorithm_id_v1("repack-gemv/q4_K_8x4_q8_K/neon-dotprod/llama-030ebb558/v1");
        let s = state_chunk_map_id_v1("repack-gemv/q4_K_8x4_q8_K/neon-dotprod/llama-030ebb558/v1");
        assert_ne!(k, t, "same string under different domains must differ");
        assert_ne!(k, s);
        assert_ne!(t, s);
    }
}
