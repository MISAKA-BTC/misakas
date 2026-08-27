//! **`PALW-BASE-0`'s step space, as a `PalwShapeProfileV3` (P0-8b, for the class that needs it).**
//!
//! The register's line was that no `PalwShapeProfileV3` exists for any real class — every
//! instance in the tree is a test fixture — and that the profile IS the step space's definition,
//! so without one there are no coordinates to capture tiles into and nothing recomputes
//! `execution_root` from an execution.
//!
//! Its work order said "write the Qwen3.5-2B profile". Measuring the model to write it showed
//! that cannot be the first step: a profile names a `kernel_semantics_id` per node, and
//! `verify_palw_genesis_v2` accepts a class only if every reachable kernel is in
//! `catalogued_kernel_ids_v1()`. **There is no float quantized matmul in that catalog at all** —
//! the only matmul is `base0/matmul-quant/i8xi8-i32-exact` — and no float RoPE and no float
//! softmax, while every layer of the pinned model is a `Q4_K`/`Q5_K`/`Q6_K` matmul and its six
//! attention layers are IMRoPE plus softmax. A faithful profile for it would name kernels this
//! build cannot adjudicate, and the coverage gate would refuse the class. Correctly.
//!
//! That is by design. `palw_base0_ops` opens by saying BASE-0's ops were "chosen for closability
//! rather than for parity with the float classes' graph", because integerising GatedDeltaNet,
//! IMRoPE and fused SwiGLU "would reproduce the catalog problem this class exists to escape."
//! And ADR-0039 makes BASE-0 the permanent liveness floor — so **the profile an RC genesis is
//! waiting on is this one**, and the pinned float model's is an FP-lane item blocked on
//! adjudicators existing first.
//!
//! # What this module is, and what it is not
//!
//! It is the GRAPH: ADR-0040 Decision D's plain decoder-only transformer, expressed as the four
//! node tables, with each node's kernel, tile length, output width and data inputs. Everything
//! here is a function of the class's geometry, so a registration cannot describe a graph the
//! adjudicator does not implement — [`base0_profile_v1`] names only catalogued kernels, and
//! `base0_profile_names_only_adjudicable_kernels` holds it to that against
//! `catalogued_kernel_ids_v1()` itself rather than against a restated list.
//!
//! It is NOT the weights. BASE-0's artifact inventory — the int8 weight rows, the per-tensor
//! requantization parameters, and the pinned integer sin/cos table Decision D substitutes for
//! `sinf`/`cosf` — is a registration artifact somebody produces and hashes into
//! `artifact_root`. Code cannot mint it, and a profile that pretended to would be describing an
//! execution nobody can perform.
//!
//! # Why the geometry is an argument
//!
//! For the pinned GGUF the geometry is MEASURED — a profile that disagrees with the file
//! describes an execution that never ran. BASE-0 has no file: it is a specification, and its
//! dimensions are what the network registering it chose. So they arrive as
//! [`PalwBase0GeometryV1`] and the profile is their consequence, which keeps "the RC picked these
//! numbers" separate from "this is what the graph must then look like".

use crate::palw_step::{
    PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN, PALW_STEP_OBJECT_VERSION_V1, PalwShapeProfileV3,
    PalwStepLaneV1, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1, kernel_semantics_id_v1,
};
use crate::palw_step_refute::{
    KDESC_A16_ADD_ELEM, KDESC_A16_ATTN_SCORES, KDESC_A16_ATTN_VALUES, KDESC_A16_EMBED, KDESC_A16_MATMUL_REQUANT, KDESC_A16_MATMUL_RESCALE, KDESC_A16_MUL_ELEM, KDESC_A16_REQUANTIZE, KDESC_A16_RMS_NORM, KDESC_A16_ROPE, KDESC_A16_SOFTMAX, KDESC_BASE0_ADD_ELEM, KDESC_BASE0_EMBED, KDESC_BASE0_MATMUL, KDESC_BASE0_MUL_ELEM, KDESC_BASE0_REQUANTIZE, KDESC_BASE0_RESCALE, KDESC_BASE0_RMS_NORM, KDESC_BASE0_ROPE, KDESC_BASE0_SILU, KDESC_BASE0_SOFTMAX, KDESC_Q36_SILU,
};
use crate::{Hash64, palw_step::PalwStepError};

/// The int8 GGML dtype byte. BASE-0 has exactly one weight dtype — that is the class — so the
/// per-layer dtype list is this value repeated, and any variance would mean it is not BASE-0.
pub const BASE0_WEIGHT_DTYPE_I8: u8 = 24;

/// The dimensions a network chooses when it registers BASE-0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBase0GeometryV1 {
    pub layer_count: u16,
    pub hidden_dim: u32,
    pub ffn_dim: u32,
    pub attn_heads: u16,
    pub attn_head_dim: u32,
    pub vocab_size: u32,
    pub n_ctx: u32,
    pub n_threads: u32,
    /// `Qk` integer epsilon added inside `RmsNorm` before `IntRsqrt` (ADR-0040 Decision D op 3).
    /// A registration fact, not a constant: the court recomputes norms with the CLASS's epsilon,
    /// and a hardcoded one convicted every honest producer of a class that chose another.
    pub rms_eps_q: i64,
    /// Elements per committed tile. One value for the whole graph: a tile length is how finely a
    /// dispute can be localized, and per-node tuning would be a knob with consensus meaning.
    pub tile_len: u32,
}

/// **The BASE-0 layer graph, as the engine performs it** (ADR-0049 Decision F, ADR-0050 Decision A).
///
/// # Why this exists
///
/// The profile used to be written by hand beside an engine written by hand, and the two described
/// different computations. Measured: the engine performs **thirty-six** steps per layer and the
/// profile declared **eighteen** — every arithmetic op and not one of the narrowings that follow
/// them, plus no rotation of K at all.
///
/// That is not a cosmetic gap. `base0_row`'s `RmsNorm` and `AddElem` arms read their inputs through
/// `as_i8`, which is `i8::try_from`. A declared `RmsNorm` returns Qk and a declared `AddElem`
/// returns the `i32` sum of two `int8` codes, range `[-256, 254]`. So on the declared graph the
/// first projection of every layer and both residual sites were `InputSetNotCanonical` — the class
/// could not be adjudicated anywhere the values left the `int8` lane, which is everywhere they
/// carry signal.
///
/// # What a narrowing needs
///
/// Every `Requantize` node names a registered tensor, because the court resolves a node's
/// parameters through `PalwWeightOracleV1` and a parameter that cannot be opened is a step that
/// cannot be adjudicated. That includes the three the engine holds as constants —
/// `QK_TO_CODE`, `CODE_CLAMP`, `CODE_PRODUCT_TO_CODE`. A constant the court must reproduce is
/// either part of a kernel's identity or part of the artifact, and putting it in the artifact keeps
/// ADR-0040 Decision D's op set at ten rather than minting a descriptor per constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base0IrWidthV1 {
    Hidden,
    KvDim,
    FfnDim,
    HeadDim,
    /// `multiplier x kv_len(position)` — attention scores and their softmax.
    KvScaled(u32),
    /// **One row PER QUERY HEAD, concatenated** — `attn_heads x kv_len(position)`.
    ///
    /// The engine runs the score / amplify / softmax / narrow sequence once per query head, and
    /// the IR declared it once per LAYER at `KvScaled(1)`. So the declared graph was
    /// `attn_heads` times smaller than the computation at exactly the four nodes attention
    /// happens in: `leaves_per_position` counted one head's scores for all of them, the ladder was
    /// sized against that count, and a challenger disputing the second head's softmax had no
    /// coordinate to name it with — the step space did not contain it.
    ///
    /// Expressed as a WIDTH rather than as a repeat count, so no coordinate field is added: the
    /// heads' rows are concatenated head-major, a tile index addresses a slice of that
    /// concatenation, and which head a tile belongs to is `tile · tile_len / kv_len`. The step
    /// space grows to contain every head; nothing about how a step is named changes.
    KvPerHead,
    /// The vocabulary — the logits row, and the only width in the graph that is not a function of
    /// the hidden size.
    Vocab,
}

/// Where a step's operand comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base0IrInputV1 {
    /// An earlier step of this layer, by IR index.
    Step(u16),
    LayerIn,
    CachedK,
    CachedV,
}

/// One step, named the way the engine names it.
#[derive(Clone, Copy, Debug)]
pub struct Base0IrNodeV1 {
    pub op: PalwStepOpKindV1,
    /// Which cache, if any, this step's output is written to. **On the ROTATED key**, because that
    /// is what a later position's attention reads — a court recomputing against unrotated keys
    /// convicts every honest producer. The hand-written table carried the role on the raw
    /// projection.
    pub role: PalwStepNodeRoleV1,
    pub kernel: &'static str,
    /// The registered tensor this step reads its weight or parameters from; empty means the step
    /// reads only its opened inputs.
    pub weight: &'static str,
    pub out: Base0IrWidthV1,
    pub inputs: &'static [Base0IrInputV1],
}

use Base0IrInputV1::{CachedK, CachedV, LayerIn, Step};
use Base0IrWidthV1::{FfnDim, HeadDim, Hidden, KvDim, KvPerHead, KvScaled, Vocab};

/// The thirty-six steps, in the engine's own order (`misaka-palw-base0/src/engine.rs`).
pub const BASE0_LAYER_IR: &[Base0IrNodeV1] = &[
    // --- attention ---------------------------------------------------------------------------
    // **No weight operand, and that is the engine's own shape** (ADR-0049 Decision F, audit
    // C-05/C-06). `Base0Engine::norm_to_code` is `rms_norm(h, eps_q)` — there is no gain vector in
    // `Base0ArtifactV1` and none is read. Naming one here made the profile demand a tensor no
    // honest artifact could carry, so a real inventory could never cover the graph: exactly the
    // defect this table's own doc says generating it from the IR was meant to end, arriving
    // through the IR instead of past it.
    n(PalwStepOpKindV1::RmsNorm, KDESC_BASE0_RMS_NORM, "", Hidden, &[LayerIn]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.attn_norm.requant", Hidden, &[Step(0)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, "blk.{layer}.attn_q.weight", Hidden, &[Step(1)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.attn_q.requant", Hidden, &[Step(2)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, "blk.{layer}.attn_k.weight", KvDim, &[Step(1)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.attn_k.requant", KvDim, &[Step(4)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, "blk.{layer}.attn_v.weight", KvDim, &[Step(1)]),
    c(
        PalwStepOpKindV1::MulElem,
        PalwStepNodeRoleV1::VCacheWrite,
        KDESC_BASE0_REQUANTIZE,
        "blk.{layer}.attn_v.requant",
        KvDim,
        &[Step(6)],
    ),
    // The rotation of Q **and of K**. The engine rotates both; the declared graph rotated neither's
    // narrowing and K not at all, so a court recomputing attention read unrotated keys — which
    // convicts every honest producer, the one failure this court may never have.
    n(PalwStepOpKindV1::RopeImrope, KDESC_BASE0_ROPE, "blk.{layer}.rope_table", Hidden, &[Step(3)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.rope_clamp.requant", Hidden, &[Step(8)]),
    n(PalwStepOpKindV1::RopeImrope, KDESC_BASE0_ROPE, "blk.{layer}.rope_table", KvDim, &[Step(5)]),
    c(
        PalwStepOpKindV1::MulElem,
        PalwStepNodeRoleV1::KCacheWrite,
        KDESC_BASE0_REQUANTIZE,
        "blk.{layer}.rope_clamp.requant",
        KvDim,
        &[Step(10)],
    ),
    // Scores, amplified into the Qk band SoftMax is defined on, then narrowed back to codes so the
    // value-weighted sum is an ordinary DotI8 (ADR-0040 Decision H).
    // Per QUERY HEAD, concatenated. The engine runs these four once per head; the table declared
    // them once per layer, so the step space was `attn_heads` times too small at exactly the
    // nodes attention happens in.
    n(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, "", KvPerHead, &[Step(9), CachedK]),
    n(PalwStepOpKindV1::Scale, KDESC_BASE0_RESCALE, "blk.{layer}.attn_logit.scale", KvPerHead, &[Step(12)]),
    n(PalwStepOpKindV1::SoftMax, KDESC_BASE0_SOFTMAX, "", KvPerHead, &[Step(13)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.qk_to_code.requant", KvPerHead, &[Step(14)]),
    // The value-weighted sum writes `attn`, which is `d_model` wide — one `d_head` slice per QUERY
    // head. `KvDim` is `n_kv_heads x d_head` and coincides with it only when attention is
    // multi-head; under grouped-query attention the declared width was short by the group factor.
    n(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, "", Hidden, &[Step(15), CachedV]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.code_product.requant", Hidden, &[Step(16)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, "blk.{layer}.attn_output.weight", Hidden, &[Step(17)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.attn_output.requant", Hidden, &[Step(18)]),
    // The residual, and the narrowing that was never declared.
    n(PalwStepOpKindV1::AddElem, KDESC_BASE0_ADD_ELEM, "", Hidden, &[Step(19), LayerIn]),
    // **ADR-0050 Decision A/B: the residual site is Add → Rescale → Requantize.** The gain exists
    // so a decayed stream can be LIFTED before it is re-quantized; a requantization can only
    // reduce, which is why the calibrated table on the real checkpoint came out with every layer
    // already at `shift = 0` and the stream still at 5 of 127.
    n(PalwStepOpKindV1::Scale, KDESC_BASE0_RESCALE, "blk.{layer}.attn_residual.scale", Hidden, &[Step(20)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.attn_residual.requant", Hidden, &[Step(21)]),
    // --- feed-forward ------------------------------------------------------------------------
    n(PalwStepOpKindV1::RmsNorm, KDESC_BASE0_RMS_NORM, "", Hidden, &[Step(22)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.ffn_norm.requant", Hidden, &[Step(23)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, "blk.{layer}.ffn_gate.weight", FfnDim, &[Step(24)]),
    n(PalwStepOpKindV1::Scale, KDESC_BASE0_RESCALE, "blk.{layer}.ffn_gate.scale", FfnDim, &[Step(25)]),
    n(PalwStepOpKindV1::Silu, KDESC_BASE0_SILU, "", FfnDim, &[Step(26)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.qk_to_code.requant", FfnDim, &[Step(27)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, "blk.{layer}.ffn_up.weight", FfnDim, &[Step(24)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.ffn_up.requant", FfnDim, &[Step(29)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_MUL_ELEM, "", FfnDim, &[Step(28), Step(30)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.code_product.requant", FfnDim, &[Step(31)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, "blk.{layer}.ffn_down.weight", Hidden, &[Step(32)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.ffn_down.requant", Hidden, &[Step(33)]),
    n(PalwStepOpKindV1::AddElem, KDESC_BASE0_ADD_ELEM, "", Hidden, &[Step(34), Step(22)]),
    n(PalwStepOpKindV1::Scale, KDESC_BASE0_RESCALE, "blk.{layer}.ffn_residual.scale", Hidden, &[Step(35)]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.ffn_residual.requant", Hidden, &[Step(36)]),
];

/// **The head tensor's placeholder.**
///
/// The lm_head is the one operand that is a property of the CLASS rather than of the graph: the
/// floor reads `output.weight`, and a class with tied embeddings reads `token_embd.weight` and
/// carries no `output.weight` at all. Everything else in the IR is named outright, and this is
/// substituted at projection time — which is what lets both classes share one post table.
pub const BASE0_IR_HEAD_TENSOR: &str = "{head}";

/// **The graph's first step: the embedding gather.**
///
/// No input refs, and that is the shape G5d settled: a gather's operands are the registered table
/// and the TOKEN ID, not an opened row. The pre table has no upstream to supply one, and the id
/// rides the refutation hash-checked against the job context's commitment.
pub const BASE0_PRE_IR: &[Base0IrNodeV1] = &[n(PalwStepOpKindV1::EmbedLookup, KDESC_BASE0_EMBED, "token_embd.weight", Hidden, &[])];

/// **The head, and it is THREE steps.**
///
/// `Base0Engine`'s final norm is `rms_norm` followed by the narrowing back to activation codes.
/// Both classes' post tables declared the first and not the second — the same omission the layer
/// table carried before it was generated, where a court recomputing the head would compare a Qk
/// value against a code. Written once here so it cannot be omitted in one class and not the other.
pub const BASE0_POST_IR: &[Base0IrNodeV1] = &[
    n(PalwStepOpKindV1::RmsNorm, KDESC_BASE0_RMS_NORM, "", Hidden, &[LayerIn]),
    n(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "output_norm.requant", Hidden, &[Step(0)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, BASE0_IR_HEAD_TENSOR, Vocab, &[Step(1)]),
];

/// `const fn` so the table above reads as a list rather than as a struct literal thirty-six times.


/// **The A16 tier's dense layer, in `engine_a16.rs`'s own order** — twenty-seven steps against
/// [`BASE0_LAYER_IR`]'s thirty-seven, because W8A16 spends no step re-narrowing an activation to
/// eight bits between every pair of matmuls.
///
/// # Why a dense class needs its own table rather than the floor's
///
/// The floor's IR describes `engine.rs`, whose activations are seven-bit codes. Static PTQ of a
/// real checkpoint into that stream is where Qwen2.5's argmax degenerates to a constant — a
/// measured result, not a suspicion (ADR-0053). `engine_a16` is the tier the quantization ladder
/// said this architecture needs, and Qwen2.5-1.5B is FAITHFUL on it. Registering the floor's graph
/// for a Qwen2.5 class therefore registers the wrong engine: the one that runs and does not work.
///
/// The differences from the floor's table are the tier's, and each is visible here:
/// * one narrowing after each norm instead of one after every step (`a16_requant`);
/// * the rotation is `a16_rope`, which narrows with its own rule rather than through a registered
///   clamp, so it is [`KDESC_A16_ROPE`] and not the partial-rotary op;
/// * the SwiGLU multiplies two CODE rows ([`KDESC_A16_MUL_ELEM`]), so `silu` is followed by one
///   narrowing and the product by one more;
/// * the attention reductions carry their scale in a registered triple instead of a separate
///   `Scale` step.
pub const QWEN25_A16_LAYER_IR: &[Base0IrNodeV1] = &[
    // --- attention ---------------------------------------------------------------------------
    n(PalwStepOpKindV1::RmsNorm, KDESC_A16_RMS_NORM, "", Hidden, &[LayerIn]),
    n(PalwStepOpKindV1::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_norm.a16", Hidden, &[Step(0)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_A16_MATMUL_REQUANT, "blk.{layer}.attn_q.weight", Hidden, &[Step(1)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_A16_MATMUL_REQUANT, "blk.{layer}.attn_k.weight", KvDim, &[Step(1)]),
    // V is the value cache's write — unrotated, which is why the role sits here and not later.
    c(
        PalwStepOpKindV1::MatMulQuant,
        PalwStepNodeRoleV1::VCacheWrite,
        KDESC_A16_MATMUL_REQUANT,
        "blk.{layer}.attn_v.weight",
        KvDim,
        &[Step(1)],
    ),
    n(PalwStepOpKindV1::RopeImrope, KDESC_A16_ROPE, "rope", Hidden, &[Step(2)]),
    // And the key cache is written with the ROTATED key, because that is what a later position
    // reads — the same correction the floor's table carries.
    c(PalwStepOpKindV1::RopeImrope, PalwStepNodeRoleV1::KCacheWrite, KDESC_A16_ROPE, "rope", KvDim, &[Step(3)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_A16_ATTN_SCORES, "blk.{layer}.attn_logits.a16", KvPerHead, &[Step(5), CachedK]),
    n(PalwStepOpKindV1::SoftMax, KDESC_A16_SOFTMAX, "blk.{layer}.attn_softmax_up", KvPerHead, &[Step(7)]),
    n(PalwStepOpKindV1::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_probs.a16", KvPerHead, &[Step(8)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_A16_ATTN_VALUES, "blk.{layer}.attn_values.a16", Hidden, &[Step(9), CachedV]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_A16_MATMUL_REQUANT, "blk.{layer}.attn_output.weight", Hidden, &[Step(10)]),
    n(PalwStepOpKindV1::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_align.a16", Hidden, &[LayerIn]),
    n(PalwStepOpKindV1::AddElem, KDESC_A16_ADD_ELEM, "", Hidden, &[Step(12), Step(11)]),
    n(PalwStepOpKindV1::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_residual.a16", Hidden, &[Step(13)]),
    // --- SwiGLU -------------------------------------------------------------------------------
    n(PalwStepOpKindV1::RmsNorm, KDESC_A16_RMS_NORM, "", Hidden, &[Step(14)]),
    n(PalwStepOpKindV1::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.ffn_norm.a16", Hidden, &[Step(15)]),
    // The gate leaves the matmul WIDE — `silu` is defined on Q[K], not on codes, and a class that
    // narrowed here would hand the nonlinearity a fraction of its argument (the floor's own
    // `rescale_q` correction, in this tier's costume).
    n(PalwStepOpKindV1::MatMulQuant, KDESC_A16_MATMUL_RESCALE, "blk.{layer}.ffn_gate.weight", FfnDim, &[Step(16)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_A16_MATMUL_REQUANT, "blk.{layer}.ffn_up.weight", FfnDim, &[Step(16)]),
    n(PalwStepOpKindV1::Silu, KDESC_Q36_SILU, "", FfnDim, &[Step(17)]),
    n(PalwStepOpKindV1::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.ffn_silu.a16", FfnDim, &[Step(19)]),
    n(PalwStepOpKindV1::MulElem, KDESC_A16_MUL_ELEM, "", FfnDim, &[Step(20), Step(18)]),
    n(PalwStepOpKindV1::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.ffn_gated.a16", FfnDim, &[Step(21)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_A16_MATMUL_REQUANT, "blk.{layer}.ffn_down.weight", Hidden, &[Step(22)]),
    n(PalwStepOpKindV1::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.ffn_align.a16", Hidden, &[Step(14)]),
    n(PalwStepOpKindV1::AddElem, KDESC_A16_ADD_ELEM, "", Hidden, &[Step(24), Step(23)]),
    n(PalwStepOpKindV1::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.ffn_residual.a16", Hidden, &[Step(25)]),
];

/// The A16 tier's pre and post tables — the gather, and the head's three steps.
pub const QWEN25_A16_PRE_IR: &[Base0IrNodeV1] =
    &[n(PalwStepOpKindV1::EmbedLookup, KDESC_A16_EMBED, "token_embd.weight", Hidden, &[])];

pub const QWEN25_A16_POST_IR: &[Base0IrNodeV1] = &[
    n(PalwStepOpKindV1::RmsNorm, KDESC_A16_RMS_NORM, "", Hidden, &[LayerIn]),
    n(PalwStepOpKindV1::MulElem, KDESC_A16_REQUANTIZE, "final_norm.a16", Hidden, &[Step(0)]),
    n(PalwStepOpKindV1::MatMulQuant, KDESC_A16_MATMUL_REQUANT, BASE0_IR_HEAD_TENSOR, Base0IrWidthV1::Vocab, &[Step(1)]),
];

const fn n(
    op: PalwStepOpKindV1,
    kernel: &'static str,
    weight: &'static str,
    out: Base0IrWidthV1,
    inputs: &'static [Base0IrInputV1],
) -> Base0IrNodeV1 {
    Base0IrNodeV1 { op, role: PalwStepNodeRoleV1::Plain, kernel, weight, out, inputs }
}

/// The same, for a step whose output is a cache write.
const fn c(
    op: PalwStepOpKindV1,
    role: PalwStepNodeRoleV1,
    kernel: &'static str,
    weight: &'static str,
    out: Base0IrWidthV1,
    inputs: &'static [Base0IrInputV1],
) -> Base0IrNodeV1 {
    Base0IrNodeV1 { op, role, kernel, weight, out, inputs }
}

/// Unused placeholder so `HeadDim` is not an unreachable variant while the IR is per-row.
const _: Base0IrWidthV1 = HeadDim;

/// The floor's head tensor: an untied `output.weight`, kept separate from the embedding table so
/// that a class which ties them does so by carrying equal bytes and the digest still sees it.
pub const BASE0_FLOOR_HEAD_TENSOR: &str = "output.weight";

/// The tensor names BASE-0's registration inventory must contain, with `{layer}` standing for the
/// layer index. Public because the inventory is built from them and `verify_palw_genesis_v2`'s
/// artifact root is over that inventory — one list, not two.
/// **Derived from the graph, not maintained beside it** (ADR-0049 Decision F).
///
/// Every tensor the layer IR names, deduplicated and in first-use order, plus the graph-level ones
/// the pre and post tables read. Maintaining this list by hand is how it came to omit every
/// narrowing's parameters while listing three norm gains the engine never reads.
pub fn base0_tensor_names_v1() -> Vec<&'static str> {
    base0_tensor_names_for_head_v1(BASE0_FLOOR_HEAD_TENSOR)
}

/// The same list for a class whose head reads a different tensor.
///
/// Tied embeddings are the case that needs it: the lm_head reads `token_embd.weight` and no
/// `output.weight` exists, so the head tensor is a property of the class rather than of the IR.
/// Everything else — every narrowing, every scale, every projection — comes from the IR, which is
/// the point: the inventory is a PROJECTION (ADR-0049 Decision F), and the hand-written Qwen list
/// it replaces declared 17 tensors against a graph that reads 27, omitting every narrowing exactly
/// as the hand-written node table omitted every narrowing node.
pub fn base0_tensor_names_for_head_v1(head: &'static str) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    // Every table, in execution order — the pre table's gather, the layer's operands, the head's.
    // Listing the graph-level ones by hand beside the projected layer ones is what let the post
    // table's narrowing go unnamed for as long as the post table itself omitted it.
    for table in [BASE0_PRE_IR, BASE0_LAYER_IR, BASE0_POST_IR] {
        for ir in table {
            let name = if ir.weight == BASE0_IR_HEAD_TENSOR { head } else { ir.weight };
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// ADR-0040 Decision D's graph, for `geometry`.
///
/// Every layer is an attention layer (`full_attention_interval = 1`), because BASE-0 is a plain
/// decoder-only transformer — there is no GatedDeltaNet arm to interleave, which is exactly the
/// simplification the class exists for. So the GDN table is empty and the attention table is the
/// per-layer template.
///
/// The node order IS the execution order, and `input_refs` names which committed material each
/// step is recomputed from — without it a challenger could open unrelated tiles as "the inputs"
/// and manufacture a conviction.
/// **The geometry the IR is projected at** (ADR-0049 Decision F).
///
/// `BASE0_LAYER_IR` is one description of one computation; this is the only thing that varies
/// between the classes projected from it. It exists as its own type because the projection used to
/// read `PalwBase0GeometryV1` directly, and that type has no kv-head count — so `kv_dim` was
/// `attn_heads x attn_head_dim` and the projection could express multi-head attention and nothing
/// else. The second class is grouped-query (12 query heads against 2 kv), so it could not be
/// projected at all, and a hand-written table was written beside the engine instead. That table
/// declared 27 nodes against the engine's 38 and its widths diverged from the third node onward:
/// **842 disagreements over 1068 captured rows**, and no execution of that class could become a
/// step leg.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Base0IrGeometryV1 {
    pub layer_count: u16,
    pub hidden_dim: u32,
    pub ffn_dim: u32,
    pub attn_heads: u16,
    /// Equal to `attn_heads` for multi-head attention, which is what the floor is and what every
    /// projection meant before this field existed.
    pub attn_kv_heads: u16,
    pub attn_head_dim: u32,
    pub tile_len: u32,
    /// The logits width — the post table's last node, and the only width here that is not a
    /// function of the hidden size.
    pub vocab_size: u32,
    /// One byte per weight dtype entry — the projection needs the layer count it covers.
    pub weight_dtype: u8,
}

/// How many layers a table's operands cover — which decides how long its `weight_dtypes` list is.
///
/// A per-layer tensor carries one dtype byte per layer; a graph-level one carries a single byte.
/// It is a property of the TABLE rather than a parameter, which is why it is named rather than
/// passed as a count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base0IrScopeV1 {
    PerLayer,
    Graph,
}

/// **Project `BASE0_LAYER_IR` into a profile's attention table.**
///
/// The one place a layer's node table comes from. Both classes call it; neither writes a node.
pub fn base0_ir_attn_nodes_v1(g: Base0IrGeometryV1) -> Vec<PalwStepNodeV1> {
    base0_ir_nodes_v1(BASE0_LAYER_IR, g, Base0IrScopeV1::PerLayer, "")
}

/// **Project any of the three IR tables into a profile's node table.**
///
/// `head` substitutes [`BASE0_IR_HEAD_TENSOR`], which only the post table names.
pub fn base0_ir_nodes_v1(table: &[Base0IrNodeV1], g: Base0IrGeometryV1, scope: Base0IrScopeV1, head: &str) -> Vec<PalwStepNodeV1> {
    let layers = match scope {
        Base0IrScopeV1::PerLayer => (g.layer_count as usize).max(1),
        Base0IrScopeV1::Graph => 1,
    };
    let per_layer = vec![g.weight_dtype; layers];
    let kv_dim = g.attn_kv_heads as u32 * g.attn_head_dim;
    table
        .iter()
        .map(|ir| PalwStepNodeV1 {
            op_kind: ir.op,
            role: ir.role,
            weight_name: if ir.weight == BASE0_IR_HEAD_TENSOR { head.to_string() } else { ir.weight.to_string() },
            weight_dtypes: if ir.weight.is_empty() { Vec::new() } else { per_layer.clone() },
            out_len: match ir.out {
                Hidden => PalwStepOutLenV1::Fixed { elements: g.hidden_dim },
                KvDim => PalwStepOutLenV1::Fixed { elements: kv_dim },
                FfnDim => PalwStepOutLenV1::Fixed { elements: g.ffn_dim },
                HeadDim => PalwStepOutLenV1::Fixed { elements: g.attn_head_dim },
                KvScaled(m) => PalwStepOutLenV1::KvScaled { multiplier: m },
                KvPerHead => PalwStepOutLenV1::KvScaled { multiplier: g.attn_heads as u32 },
                Vocab => PalwStepOutLenV1::Fixed { elements: g.vocab_size },
            },
            tile_len: g.tile_len,
            kernel_semantics_id: kernel_semantics_id_v1(ir.kernel),
            input_refs: ir
                .inputs
                .iter()
                .map(|r| match r {
                    Step(k) => *k,
                    LayerIn => PALW_STEP_INPUT_LAYER_IN,
                    CachedK => PALW_STEP_INPUT_KV_K,
                    CachedV => PALW_STEP_INPUT_KV_V,
                })
                .collect(),
        })
        .collect()
}

pub fn base0_profile_v1(geometry: PalwBase0GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    let tile = geometry.tile_len;
    let hidden = geometry.hidden_dim;
    let kv_dim = geometry.attn_heads as u32 * geometry.attn_head_dim;

    // **Every node table is projected — pre, attention and post** (ADR-0049 Decision F). The
    // builders that used to stand here (`plain`, `weighted`) existed to write nodes by hand, and
    // nothing in this file writes one any more.
    let ir_geometry = Base0IrGeometryV1 {
        layer_count: geometry.layer_count,
        hidden_dim: hidden,
        ffn_dim: geometry.ffn_dim,
        attn_heads: geometry.attn_heads,
        attn_kv_heads: geometry.attn_heads,
        attn_head_dim: geometry.attn_head_dim,
        tile_len: tile,
        vocab_size: geometry.vocab_size,
        weight_dtype: BASE0_WEIGHT_DTYPE_I8,
    };

    // --- pre: the embedding gather ---
    //
    // **Projected too** (ADR-0049 Decision F). A hand-written table of one node is still a second
    // description of one step, and the post table below is the proof that a small hand-written
    // table drifts exactly like a large one: it declared the final norm and not the narrowing after
    // it, in BOTH classes, for as long as it was written twice.
    let pre_nodes = base0_ir_nodes_v1(BASE0_PRE_IR, ir_geometry, Base0IrScopeV1::Graph, BASE0_FLOOR_HEAD_TENSOR);

    // --- the per-layer template. Slot numbers are intra-table indices; `input_refs` uses them. ---
    // **Generated from `BASE0_LAYER_IR`, which is the engine's own step order** (ADR-0049
    // Decision F). This table used to be written here by hand beside an engine written by hand,
    // and that is how it came to declare eighteen of the engine's thirty-six steps — every
    // arithmetic op and not one of the narrowings — with K never rotated and the cache role on the
    // raw projection instead of the rotated key.
    // Projected, not written. The floor is multi-head, so its kv-head count IS its query-head
    // count — which is what `kv_dim` above has always assumed and what keeps this byte-identical
    // to the hand-rolled projection this replaced.
    let attn_nodes: Vec<PalwStepNodeV1> = base0_ir_attn_nodes_v1(ir_geometry);
    debug_assert_eq!(kv_dim, geometry.attn_heads as u32 * geometry.attn_head_dim);

    // The `KvScaled` widths come from the IR now. They used to be patched in here by slot
    // index, which silently rewrote two unrelated nodes the moment the graph gained its narrowings.

    // --- post: the final norm, its narrowing, and the logits head ---
    let post_nodes = base0_ir_nodes_v1(BASE0_POST_IR, ir_geometry, Base0IrScopeV1::Graph, BASE0_FLOOR_HEAD_TENSOR);

    let profile = PalwShapeProfileV3 {
        version: PALW_STEP_OBJECT_VERSION_V1,
        // Int32: BASE-0 commits integer codes, and the float lanes' non-finite rule would convict
        // every negative activation of being a NaN.
        lane: PalwStepLaneV1::Int32,
        layer_count: geometry.layer_count,
        // Every layer is attention: there is no GDN arm in this class.
        full_attention_interval: 1,
        hidden_dim: hidden,
        ffn_dim: geometry.ffn_dim,
        attn_heads: geometry.attn_heads,
        // No grouped-query attention: one kv head per query head keeps the projection widths
        // equal and the graph one template.
        attn_kv_heads: geometry.attn_heads,
        attn_head_dim: geometry.attn_head_dim,
        rope_dims: geometry.attn_head_dim as u16,
        rope_sections: [0, 0, 0, 0],
        // The three float constants a float class pins are all ZERO here, and that is a property
        // rather than a gap: BASE-0 evaluates no float transcendental and holds no float epsilon.
        // Its norm epsilon is `base0_rms_eps_q`, an integer in Qk.
        rope_freq_base_bits: 0,
        rms_eps_bits: 0,
        l2_eps_bits: 0,
        base0_rms_eps_q: geometry.rms_eps_q,
        // FLAT: the floor's vocabulary is small by construction, so whole rows are the cheaper
        // close — and it is what `base0_execute_for_attempt_v1` has always committed.
        logits_scheme_id: crate::palw_step_refute::flat_logits_scheme_id_v1(),
        gdn_heads: 0,
        gdn_head_k_dim: 0,
        gdn_head_v_dim: 0,
        gdn_conv_kernel: 0,
        vocab_size: geometry.vocab_size,
        // Execution-shape flags. The repack/llamafile/fused-GDN paths are llama.cpp's and this
        // class does not run llama.cpp; flash attention is pinned disabled because the profile
        // requires it (Fact 7) and no BASE-0 attention kernel exists to enable.
        repack_on: 0,
        llamafile_on: 0,
        flash_attn_disabled: 1,
        fused_gdn_on: 0,
        use_ref_off: 0,
        // No cache holds floats — ADR-0040 Decision D's second deliberate absence.
        kv_cache_f16: 0,
        n_ctx: geometry.n_ctx,
        n_batch: geometry.n_ctx,
        n_ubatch: geometry.n_ctx,
        n_seq: 1,
        n_threads: geometry.n_threads,
        pre_nodes,
        // Empty, and `validate_shape` requires it to be: `full_attention_interval = 1` means no
        // layer is a GDN layer, and a table for a kind of layer that does not exist would be
        // unreachable arithmetic inside a consensus identity.
        gdn_nodes: Vec::new(),
        attn_nodes,
        post_nodes,
        reference_ruleset_id: crate::palw_reference::reference_arithmetic_ruleset_id_v2(),
        // Empty, and this is the class's whole thesis: a transcendental evaluated at registration
        // is data, and BASE-0 has no site that evaluates one at inference. `IntExp`, `IntRecip`
        // and `IntRsqrt` are integer programs in the ruleset above, not libm bindings.
        transcendental_bindings: Vec::new(),
        // Empty for the same kind of reason: integer addition is exactly associative, so there is
        // no FMA contraction to pin (ADR-0040 Decision E).
        contraction_facts: Vec::new(),
        kv_chunk_calls: 0,
        // **Registered** (was `Hash64::default()`, the unregistered sentinel). This class's
        // replay state is int8 KV rows whose layout is a function of the geometry above, so
        // the map derives rather than being measured — see `palw_state_chunk_map`. Until this
        // line, the checkpoint leg committed to bytes nothing could interpret and every
        // dispute replayed from step 0.
        state_chunk_map_id: crate::palw_state_chunk_map::integer_kv_state_chunk_map_id_v1(),
    };
    profile.validate_shape()?;
    Ok(profile)
}

/// **A BASE-0 catalog entry whose numbers are COUNTED from the profile, never chosen.**
///
/// `canonical_step_leaf_count` is what ADR-0045 Decision 1 makes `pwu_per_inference` normatively
/// equal to, and it is a direct, permanent multiplier on the class's fork-choice weight — so a
/// registration that could pick it could pick its own weight. `verify_palw_genesis_v2` already
/// refuses a registration whose declaration differs from the catalog's number; this is the other
/// half, which makes the catalog's number a measurement rather than a second declaration.
///
/// Both counts come from `step_leaf_count`, the same function the leg builder sizes itself with,
/// applied to the canonical job shape and to the class's worst case. Two counters would be two
/// answers, and the leg builder's is the one an execution actually has to satisfy.
///
/// `canonical` is the (prefill, decode) shape of one canonical inference — the unit the class is
/// paid per — and `worst_case` is the deepest run the ladder must still be able to walk.
pub fn base0_catalog_entry_v1(
    class_id: Hash64,
    artifact_root: Hash64,
    profile: &PalwShapeProfileV3,
    canonical: &crate::palw_v2::PalwJobContextV2,
    worst_case: &crate::palw_v2::PalwJobContextV2,
) -> Result<crate::palw_mode_v2::PalwClassCatalogEntryV2, PalwStepError> {
    let canonical_step_leaf_count = crate::palw_step::step_leaf_count(profile, canonical)?;
    let max_step_leaf_count = crate::palw_step::step_leaf_count(profile, worst_case)?;
    // Every kernel the graph can reach, read off the graph itself. A hand-maintained list here
    // would be the coverage gate certifying a set nobody derived from the thing it covers.
    let reachable_kernels = [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes]
        .into_iter()
        .flatten()
        .map(|node| node.kernel_semantics_id)
        .collect();
    // The SAME derivation the post-genesis gate runs — mint and admission build entries with one
    // function, or the genesis door enforces a different metric than the running chain.
    let court_cost = crate::palw_class_admission_v2::derive_court_cost_v1(profile)
        .map_err(|_| PalwStepError::ProfileNotCanonical("the class's court cost does not derive"))?;
    Ok(crate::palw_mode_v2::PalwClassCatalogEntryV2 {
        court_cost,
        class_id,
        artifact_root,
        max_step_leaf_count,
        canonical_step_leaf_count,
        reachable_kernels,
    })
}

/// The geometry the PALW-RC network registers BASE-0 with.
///
/// Small on purpose. ADR-0039 §2a makes BASE-0 the slow floor by design — it exists so the chain
/// can always produce and always adjudicate, not so it can be fast — and every dimension here is
/// inside the class identity, so this is the RC declaring what its floor is rather than a tuning
/// knob anyone may turn.
pub const PALW_RC_BASE0_GEOMETRY: PalwBase0GeometryV1 = PalwBase0GeometryV1 {
    layer_count: 4,
    hidden_dim: 256,
    ffn_dim: 512,
    attn_heads: 4,
    attn_head_dim: 64,
    vocab_size: 4_096,
    n_ctx: 512,
    n_threads: 1,
    rms_eps_q: 1 << 8,
    tile_len: 64,
};

/// The canonical inference BASE-0 is paid per, and the worst case its ladder must walk.
pub const PALW_RC_BASE0_CANONICAL: (u32, u32) = (8, 4);
pub const PALW_RC_BASE0_WORST_CASE: (u32, u32) = (64, 64);

/// **Everything the RC's BASE-0 registration needs, from the ONE thing code cannot mint.**
///
/// The class id, the shape profile, the catalog and its root, `pwu_per_inference`, the reachable
/// kernel set and the court catalog root are all DERIVED here — from
/// [`PALW_RC_BASE0_GEOMETRY`], from `step_leaf_count`, and from this build's own adjudication
/// table. The only input is `artifact_root`, the commitment to the int8 weights, the
/// requantization parameters and the pinned sin/cos table, because those are bytes somebody
/// produces and no function can invent.
///
/// The class id IS the shape profile id. A class is its graph: two registrations of the same
/// graph are the same class, two different graphs cannot share an id, and there is no separate
/// label anyone could pick.
pub fn palw_rc_base0_registration_v1(
    artifact_root: Hash64,
) -> Result<(PalwShapeProfileV3, crate::palw_mode_v2::PalwClassCatalogV2), PalwStepError> {
    let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY)?;
    let class_id = profile.shape_profile_id();
    let canonical = rc_job_context(&profile, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    let worst = rc_job_context(&profile, PALW_RC_BASE0_WORST_CASE.0, PALW_RC_BASE0_WORST_CASE.1);
    let entry = base0_catalog_entry_v1(class_id, artifact_root, &profile, &canonical, &worst)?;
    let catalog = crate::palw_mode_v2::PalwClassCatalogV2::new(vec![entry])
        .map_err(|_| PalwStepError::ProfileNotCanonical("the RC catalog is not well-formed"))?;
    Ok((profile, catalog))
}

/// The job context the RC's leaf counts are taken over.
///
/// Only the shape fields matter to `step_leaf_count`; the identity fields are fixed so two nodes
/// deriving the RC's catalog reach the same numbers. It is not an execution — nothing runs this
/// context — it is the yardstick the class is measured with.
/// The RC's own job context, at a given `(prefill, decode)`. Public because a post-genesis
/// registration must CARRY its canonical job (ADR-0049 Decision H) and there is one way to build
/// the floor's — two would be two declarations of what one unit of its work is.
pub fn rc_job_context(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> crate::palw_v2::PalwJobContextV2 {
    let mut ctx = crate::palw_v2::PalwJobContextV2 {
        version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"misaka-palw-rc".to_vec(),
        job_id: Hash64::default(),
        job_nullifier: Hash64::default(),
        assignment_id: Hash64::default(),
        execution_seed: [0; 32],
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: profile.shape_profile_id(),
        trace_scheme_id: Hash64::default(),
        cu_ruleset_id: Hash64::default(),
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: Hash64::default(),
        declared_prefill_tokens: prefill,
        exact_decode_tokens: decode,
        max_context_tokens: profile.n_ctx,
    };
    ctx.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
    ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_step::PalwStepTableV1;
    use crate::palw_step_refute::catalogued_kernel_ids_v1;

    /// **The slot walk inverts itself, over every slot of a real profile.**
    ///
    /// A capture converts (table, layer, index) into a global slot and the court converts back.
    /// Two walks that disagree anywhere would put an honest producer's row at a coordinate the
    /// court does not read — an execution nobody can adjudicate, committed as if it could be.
    #[test]
    fn the_slot_walk_inverts_itself() {
        use crate::palw_step::PalwStepTableV1;
        let p = base0_profile_v1(geometry()).expect("expressible");
        let mut seen = 0u32;
        for slot in 0..p.global_node_count() {
            let (_, layer) = p.resolve_node_slot(slot).expect("every slot below the count resolves");
            let table = match layer {
                None if (slot as usize) < p.pre_nodes.len() => PalwStepTableV1::Pre,
                None => PalwStepTableV1::Post,
                Some(l) => match p.layer_kind(l) {
                    crate::palw_step::PalwLayerKindV1::Attention => PalwStepTableV1::Attn,
                    crate::palw_step::PalwLayerKindV1::GatedDeltaNet => PalwStepTableV1::Gdn,
                },
            };
            // Recover the intra-table index the same way the forward walk consumed it.
            let index = match table {
                PalwStepTableV1::Pre => slot as usize,
                PalwStepTableV1::Post => {
                    let mut base = p.pre_nodes.len();
                    for l in 0..p.layer_count {
                        base += p.layer_table(l).len();
                    }
                    slot as usize - base
                }
                _ => {
                    let l = layer.unwrap();
                    let mut base = p.pre_nodes.len();
                    for prev in 0..l {
                        base += p.layer_table(prev).len();
                    }
                    slot as usize - base
                }
            };
            assert_eq!(
                p.global_node_slot(table, layer.unwrap_or(0), index),
                Some(slot),
                "slot {slot} does not come back from ({table:?}, {layer:?}, {index})"
            );
            seen += 1;
        }
        assert_eq!(seen, p.global_node_count(), "every slot was walked");
        // And a row claimed for a table that layer does not have is refused, not silently placed.
        assert!(p.global_node_slot(PalwStepTableV1::Gdn, 0, 0).is_none(), "BASE-0 has no GatedDeltaNet layer");
        assert!(p.global_node_slot(PalwStepTableV1::Post, 0, p.post_nodes.len()).is_none(), "past the end is not a slot");
    }

    fn geometry() -> PalwBase0GeometryV1 {
        PalwBase0GeometryV1 {
            layer_count: 4,
            hidden_dim: 256,
            ffn_dim: 512,
            attn_heads: 4,
            attn_head_dim: 64,
            vocab_size: 4_096,
            n_ctx: 512,
            n_threads: 4,
            rms_eps_q: 1 << 8,
            tile_len: 64,
        }
    }

    /// **The profile exists and is well-formed** — the sentence the register said was false for
    /// every real class.
    #[test]
    fn base0_has_a_real_shape_profile() {
        let p = base0_profile_v1(geometry()).expect("BASE-0's graph is a valid shape");
        assert_eq!(p.layer_count, 4);
        assert_eq!(p.lane, PalwStepLaneV1::Int32, "an integer class commits integer codes");
        assert!(p.gdn_nodes.is_empty(), "a plain decoder-only transformer has no GatedDeltaNet arm");
        assert_eq!(p.table_layer_span(PalwStepTableV1::Attn), 4, "every layer is an attention layer");
        assert_eq!(p.table_layer_span(PalwStepTableV1::Gdn), 0);
        // 1 pre + the IR's own step count per layer x 4 + 3 post. The per-layer figure is the
        // engine's, not a number kept beside it: 18 while the graph was hand-written, 36 once every
        // narrowing the engine performs was declared, and 38 with ADR-0050's two residual gains.
        // The post table is 3 for the same reason — `norm_to_code` is a norm AND a narrowing, and
        // only the norm was declared.
        assert_eq!(BASE0_LAYER_IR.len(), 38, "the engine performs thirty-eight steps per layer");
        assert_eq!(p.global_node_count() as usize, 1 + BASE0_LAYER_IR.len() * 4 + 3);
        // A profile id exists, which is what a class registration commits to.
        assert_ne!(p.shape_profile_id(), Hash64::default());
    }

    /// **Every kernel it names is one this build can adjudicate.**
    ///
    /// Checked against `catalogued_kernel_ids_v1()` — the ADJUDICATION table itself — rather than
    /// against a restated list, because the gate's promise is that a registered class can
    /// actually be re-executed here, and only that table knows. This is the check a faithful
    /// Qwen3.5-2B profile fails: its matmuls are `Q4_K`/`Q5_K`/`Q6_K` and no float quantized
    /// matmul is in the catalog at all.
    #[test]
    fn base0_profile_names_only_adjudicable_kernels() {
        let p = base0_profile_v1(geometry()).unwrap();
        let catalogued = catalogued_kernel_ids_v1();
        let mut seen = 0;
        for table in [&p.pre_nodes, &p.gdn_nodes, &p.attn_nodes, &p.post_nodes] {
            for node in table {
                assert!(
                    catalogued.contains(&node.kernel_semantics_id),
                    "{:?} names a kernel this build cannot recompute — the class would be unadjudicable",
                    node.op_kind
                );
                seen += 1;
            }
        }
        assert_eq!(seen, 1 + BASE0_LAYER_IR.len() + 3, "the whole graph was checked, not a prefix of it");
    }

    /// The float constants are zero and the float tables are empty, and every one of those is a
    /// property of the class rather than an unfilled field: BASE-0 evaluates no transcendental at
    /// inference (its rotary is a pinned table), holds no float epsilon, and has no FMA
    /// contraction to pin because integer addition is exactly associative.
    #[test]
    fn base0_pins_no_float_anything() {
        let p = base0_profile_v1(geometry()).unwrap();
        assert_eq!((p.rope_freq_base_bits, p.rms_eps_bits, p.l2_eps_bits), (0, 0, 0));
        assert!(p.transcendental_bindings.is_empty(), "no libm site to bind");
        assert!(p.contraction_facts.is_empty(), "no contraction to pin");
        assert_eq!(p.kv_cache_f16, 0, "no cache holds floats");
        assert_eq!(p.base0_rms_eps_q, 1 << 8, "the epsilon it DOES hold is the integer one, from the registration");
    }

    /// Geometry is an argument, so the identity moves with it: two networks that chose different
    /// dimensions are running different classes, and `shape_profile_id` says so.
    #[test]
    fn the_geometry_is_inside_the_identity() {
        let base = base0_profile_v1(geometry()).unwrap().shape_profile_id();
        for mutate in [
            (|g: &mut PalwBase0GeometryV1| g.layer_count = 5) as fn(&mut PalwBase0GeometryV1),
            |g| g.hidden_dim = 512,
            |g| g.ffn_dim = 1_024,
            |g| g.attn_heads = 8,
            |g| g.attn_head_dim = 32,
            |g| g.vocab_size = 8_192,
            |g| g.n_ctx = 1_024,
            |g| g.n_threads = 8,
            |g| g.rms_eps_q = 1 << 9,
            |g| g.tile_len = 128,
        ] {
            let mut g = geometry();
            mutate(&mut g);
            assert_ne!(base0_profile_v1(g).unwrap().shape_profile_id(), base, "a geometry change must move the class id");
        }
    }

    /// The tensor names the graph consumes are exactly the inventory list, so a registration
    /// cannot build an artifact root over a different set than the one the court will open
    /// against.
    #[test]
    fn the_graph_consumes_exactly_the_declared_inventory() {
        let p = base0_profile_v1(geometry()).unwrap();
        let mut used: Vec<&str> = Vec::new();
        for table in [&p.pre_nodes, &p.gdn_nodes, &p.attn_nodes, &p.post_nodes] {
            for node in table {
                if !node.weight_name.is_empty() && !used.contains(&node.weight_name.as_str()) {
                    used.push(node.weight_name.as_str());
                }
            }
        }
        used.sort_unstable();
        let mut declared: Vec<&str> = base0_tensor_names_v1();
        declared.sort_unstable();
        assert_eq!(used, declared, "the graph's operands and the declared inventory are one list");
    }

    /// A canonical job shape for the geometry above, and a worst case ten times deeper.
    fn job(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> crate::palw_v2::PalwJobContextV2 {
        let mut ctx = crate::palw_v2::PalwJobContextV2 {
            version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"base0-profile-test".to_vec(),
            job_id: Hash64::from_u64_word(1),
            job_nullifier: Hash64::from_u64_word(2),
            assignment_id: Hash64::from_u64_word(3),
            execution_seed: [7; 32],
            model_profile_id: Hash64::from_u64_word(4),
            runtime_manifest_hash: Hash64::from_u64_word(5),
            runtime_class_id: Hash64::from_u64_word(6),
            shape_profile_id: profile.shape_profile_id(),
            trace_scheme_id: Hash64::default(),
            cu_ruleset_id: Hash64::from_u64_word(9),
            tokenizer_id: Hash64::from_u64_word(10),
            prompt_token_ids_hash: Hash64::from_u64_word(11),
            declared_prefill_tokens: prefill,
            exact_decode_tokens: decode,
            max_context_tokens: profile.n_ctx,
        };
        ctx.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
        ctx
    }

    /// **The catalog's numbers are counted, not chosen.**
    ///
    /// `canonical_step_leaf_count` is what `pwu_per_inference` must equal (ADR-0045 Decision 1)
    /// and therefore a direct multiplier on the class's fork-choice weight — so a registration
    /// that could pick it could pick its own weight. `verify_palw_genesis_v2` refuses a
    /// declaration that differs from the catalog; this is the half that makes the catalog's
    /// number a measurement.
    #[test]
    fn the_catalog_entry_counts_the_profile_rather_than_declaring_a_number() {
        let p = base0_profile_v1(geometry()).unwrap();
        let canonical = job(&p, 8, 4);
        let worst = job(&p, 64, 64);
        let entry = base0_catalog_entry_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(0xA7), &p, &canonical, &worst)
            .expect("the entry counts");

        // The SAME counter the leg builder sizes itself with — one answer, not two.
        assert_eq!(entry.canonical_step_leaf_count, crate::palw_step::step_leaf_count(&p, &canonical).unwrap());
        assert!(entry.canonical_step_leaf_count > 0, "a canonical inference has steps");
        assert!(
            entry.canonical_step_leaf_count < entry.max_step_leaf_count,
            "a canonical run is strictly inside the class's worst case, which is what keeps \
             'work worth paying for' and 'work the ladder can walk' the same quantity"
        );

        // A deeper job counts more leaves: the number tracks the execution, so it cannot be
        // restated as a constant.
        let deeper = job(&p, 16, 8);
        let deeper_entry = base0_catalog_entry_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(0xA7), &p, &deeper, &worst).unwrap();
        assert!(deeper_entry.canonical_step_leaf_count > entry.canonical_step_leaf_count);

        // And the reachable set is read off the graph, so the coverage gate cannot pass on a set
        // nobody derived from the thing it covers.
        assert_eq!(
            entry.reachable_kernels,
            [&p.pre_nodes, &p.gdn_nodes, &p.attn_nodes, &p.post_nodes].into_iter().flatten().map(|n| n.kernel_semantics_id).collect()
        );
        assert!(entry.reachable_kernels.is_subset(&catalogued_kernel_ids_v1()), "and every one of them is adjudicable");
    }

    /// **G5 closed: every node of this graph is one the adjudicator can actually serve.**
    ///
    /// The id gate certified this profile at "100% coverage" while four of its twenty-one nodes
    /// could never be recomputed, because it compares kernel IDs and never asks what a kernel can
    /// SERVE. Asking properly — `kernel_can_serve_node_v1`, which lives beside the code that does
    /// the serving — turned each into a registration-time refusal with a reason, and each was
    /// then closed on its own terms:
    ///
    /// * **G5a** — `MatMulQuant` demanded a registered weight, so an activation × activation
    ///   product had nothing to multiply by. ADR-0040 Decision D never said one side must be a
    ///   weight; it takes its second operand from the second opened row now.
    /// * **G5b** — `KvScaled` widths were refused for want of the kv length, which the caller
    ///   already holds in the coordinate it passes.
    /// * **G5c** — the KV sentinels were "registration-opaque". They name this layer's
    ///   cache-role nodes over the position history: the cache contents are already ordinary step
    ///   tiles, so no new leaf format and no float aux series were needed.
    /// * **G5d** — the gather returned the identity of an input the pre table has no upstream to
    ///   supply. Its operands are the registered table and the TOKEN ID, and the id rides the
    ///   refutation hash-checked against the job context. Prefill only: a decode token is pinned
    ///   by nothing there, and a challenger naming it freely would convict an honest producer.
    ///
    /// The remaining half of G5d — deriving a decode token from the previous position's committed
    /// logits — is recorded in `docs/palw-qwen25-class-phase0.md`. It is a runtime `Unadjudicable`
    /// on decode gathers, not a coverage failure: the node's SHAPE is servable, which is what
    /// this gate decides.
    #[test]
    fn every_node_of_the_graph_is_one_the_adjudicator_can_serve() {
        use crate::palw_catalog_coverage::verify_profile_coverage_v1;
        use crate::palw_step_refute::kernel_can_serve_node_v1;
        let p = base0_profile_v1(geometry()).unwrap();

        // **Coverage over COORDINATES, not kernel ids** (ADR-0049 Decision D) — and it passes only
        // because Decision E landed with it. The gate swept prefill and decode, the embedding
        // gather refused every decode position, and this class's canonical job decodes; the tripwire
        // here asserted that refusal for exactly as long as it was true. What made it false is that
        // a decode token is now pinned by the claim's own `full_logits_trace_root` — which already
        // bound `output_token_ids_hash_v2` — rather than by nothing.
        verify_profile_coverage_v1(&p).expect("every reachable coordinate class adjudicates, decode included");
        let mut checked = 0;
        for (name, nodes) in [("pre", &p.pre_nodes), ("gdn", &p.gdn_nodes), ("attn", &p.attn_nodes), ("post", &p.post_nodes)] {
            for node in nodes {
                kernel_can_serve_node_v1(node, name == "pre").expect("every node individually");
                checked += 1;
            }
        }
        // 1 pre + 38 layer steps + 3 post. Both counts are the engine's own: the layer table is
        // generated from `BASE0_LAYER_IR` (18 while the graph was written by hand, 36 with its
        // narrowings, 38 with ADR-0050's residual gains), and the post table gained the narrowing
        // `norm_to_code` performs and the table did not declare.
        assert_eq!(checked, 42, "the whole graph was checked, not a prefix");
        assert_eq!(p.attn_nodes.len(), BASE0_LAYER_IR.len(), "the layer table IS the IR");

        // The two attention nodes multiply an activation by an opened row at a kv-scaled width —
        // the shape that was structurally unadjudicable before G5a/b/c. Their slots moved with the
        // narrowings, so they are found by shape rather than by index.
        let two_operand: Vec<usize> = p
            .attn_nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.weight_name.is_empty() && n.input_refs.len() == 2 && matches!(n.op_kind, PalwStepOpKindV1::MatMulQuant))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(two_operand.len(), 2, "Q.K^T and P.V are the graph's two weightless matmuls");
        for slot in two_operand.iter().copied() {
            let node = &p.attn_nodes[slot];
            assert!(node.weight_name.is_empty(), "slot {slot} multiplies by an opened row, not a weight");
            assert_eq!(node.input_refs.len(), 2, "so it names the row it multiplies by");
        }
        // Q.K^T is kv-wide (one score per cached key); P.V is not (it is a head-width sum over the
        // value series). The two used to sit at fixed slots; they are found by shape now, because
        // the IR moved them when it declared the narrowings between them.
        let scores = two_operand[0];
        assert!(matches!(p.attn_nodes[scores].out_len, PalwStepOutLenV1::KvScaled { .. }), "scores are kv-wide");
        let weighted_values = two_operand[1];
        assert!(
            matches!(p.attn_nodes[weighted_values].out_len, PalwStepOutLenV1::Fixed { .. }),
            "the value-weighted sum is head-width, not kv-width"
        );
        // The gather takes no opened row and names its table.
        assert!(p.pre_nodes[0].input_refs.is_empty(), "a gather's operands are the table and the token id");
        assert_eq!(p.pre_nodes[0].weight_name, "token_embd.weight");

        // And the gate still refuses each shape it names, so the 100% above is a measurement
        // rather than a gate that stopped asking.
        let mut orphan = p.attn_nodes[scores].clone();
        orphan.input_refs = vec![0];
        assert!(kernel_can_serve_node_v1(&orphan, false).is_err(), "a weightless matmul with one input has nothing to multiply");
        let mut oracle_kv = p.attn_nodes[2].clone();
        oracle_kv.out_len = PalwStepOutLenV1::KvScaled { multiplier: 1 };
        assert!(kernel_can_serve_node_v1(&oracle_kv, false).is_err(), "a kv-scaled weight matmul names no matrix the oracle holds");
        let mut fed_gather = p.pre_nodes[0].clone();
        fed_gather.input_refs = vec![0];
        assert!(
            kernel_can_serve_node_v1(&fed_gather, true).is_err(),
            "a gather with an opened row declares an input nothing supplies"
        );
        let mut tableless = p.pre_nodes[0].clone();
        tableless.weight_name = String::new();
        tableless.weight_dtypes = Vec::new();
        assert!(kernel_can_serve_node_v1(&tableless, true).is_err(), "a gather must name the table it reads");
    }

    /// Every weighted node carries one dtype byte per layer its table covers, all int8 — BASE-0
    /// has exactly one weight type, and variance would mean it is not BASE-0.
    #[test]
    fn every_weight_is_int8_once_per_covered_layer() {
        let p = base0_profile_v1(geometry()).unwrap();
        for (table, span) in [
            (&p.pre_nodes, p.table_layer_span(PalwStepTableV1::Pre)),
            (&p.attn_nodes, p.table_layer_span(PalwStepTableV1::Attn)),
            (&p.post_nodes, p.table_layer_span(PalwStepTableV1::Post)),
        ] {
            for node in table {
                if node.weight_name.is_empty() {
                    assert!(node.weight_dtypes.is_empty());
                    continue;
                }
                assert_eq!(node.weight_dtypes.len(), span, "{} needs one byte per covered layer", node.weight_name);
                assert!(node.weight_dtypes.iter().all(|d| *d == BASE0_WEIGHT_DTYPE_I8), "BASE-0 is int8 throughout");
            }
        }
    }

    /// **A producer with no model can mine the shipped network, and both lanes are open.**
    ///
    /// Two facts an operator depends on and neither was asserted anywhere:
    ///
    /// A producer that passes no `--palw-producer-class` mines `bundle.base_class_id`
    /// (`daemon.rs`), so what that id IS decides whether joining requires downloading a GGUF. It is
    /// the floor: a deterministic-integer class whose artifact derives from a seed, so a node needs
    /// no model and no worker to produce. If the shipped bundle's default ever moved to a
    /// black-box class, every new miner would silently need a multi-gigabyte download first, and
    /// nothing in the build would have said so.
    ///
    /// And the bundle admits two algo ids: the committed attempt (6) and the free-prompt receipt
    /// spend (7). The gate that enforces this compares against `accepts_algo_id`; a bundle that
    /// stopped answering for both would close a lane while the gate kept reporting agreement.
    #[test]
    fn the_shipped_network_mines_without_a_model_and_opens_both_lanes() {
        let id: crate::network::NetworkId = "testnet-11".parse().expect("the shipped PALW network");
        let params = crate::config::params::Params::from(id);
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("testnet-11 is the PALW-RC network; a non-V2 mode here means the suffix stopped routing to it");
        };

        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor is expressible");
        assert_eq!(
            bundle.base_class_id,
            floor.shape_profile_id(),
            "the default producer class must be the floor — anything else makes a model download a \
             precondition for mining, without any operator being told"
        );

        assert_eq!(bundle.algorithm_id, crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2, "the attempt lane");
        assert_eq!(bundle.freeprompt.receipt_algorithm_id(), crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3, "the receipt lane");
        assert!(bundle.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2), "the attempt lane must be open");
        assert!(bundle.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3), "the receipt lane must be open");
    }

    /// Print the floor's class id. It is pinned by the RC genesis and by a live testnet-12, so any
    /// refactor of the projection must leave it byte-identical.
    /// `cargo test -p kaspa-consensus-core --lib dump_floor_class_id -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_floor_class_id() {
        let p = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor is expressible");
        println!("FLOOR_CLASS_ID {}", p.shape_profile_id());
        println!("  pre={} attn={} post={}", p.pre_nodes.len(), p.attn_nodes.len(), p.post_nodes.len());
    }
}
