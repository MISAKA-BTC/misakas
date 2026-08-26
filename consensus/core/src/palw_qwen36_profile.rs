//! **Qwen3.6-35B-A3B as an adjudicable execution class** (ADR-0052, ADR-0039's precondition).
//!
//! The runtime runs this model; this is what lets a court judge a step of it. ADR-0039 says no
//! class may carry fork-choice weight until its kernel catalog is complete, and "complete" is
//! decided by comparing two sets: the kernels this graph can REACH, and the kernels the
//! adjudicator can RE-EXECUTE ([`crate::palw_step_refute::catalogued_kernel_ids_v1`]). This module
//! supplies the first set, by declaring the graph.
//!
//! # The table is projected from the engine's own step order
//!
//! `palw_qwen25_profile` records what happens otherwise: a table written by hand beside an engine
//! written by hand declared 27 nodes against the engine's 38, and 842 of 1068 captured rows
//! disagreed — so no execution could become a step leg and the class could not produce a block.
//! So the IR below is a transcription of `Qwen36Engine::forward_token_probed` and the two arms it
//! calls, in their order, and the profile is projected from it. Where the engine loops (the eight
//! chosen experts), the IR carries the CONCATENATION the engine builds, because that is the row
//! the engine commits.
//!
//! # Why the mixture is six nodes and not forty
//!
//! Each of the eight chosen experts is a gate projection, an up projection, a SiLU, a product and
//! a down projection — forty steps if each expert is its own node, against a per-table cap of 64
//! that the rest of the layer also has to fit inside. It does not need forty. Every expert reads
//! the SAME normalized row, so eight `[512 × 2048]` matrices against one input is arithmetic
//! identical to one `[4096 × 2048]` matrix against it, and the engine already builds exactly that
//! concatenation. The tile machinery then addresses an expert's slice of the row without knowing
//! that experts exist.
//!
//! WHICH eight matrices those are is the routing, which is data. The oracle serving
//! `…_exps.routed` resolves it from the committed router row — the row the `RouterTopk` node
//! commits one step earlier, so a court that disagrees about the selection convicts at that node
//! rather than silently adjudicating the wrong expert.
//!
//! # What this module is not
//!
//! It is the graph, not the weights, and not the registration. The artifact
//! (`misaka-palw-base0/src/qwen36.rs`) and the wiring that answers
//! [`crate::palw_step_refute::PalwWeightOracleV1`] for these tensor names are separate things, and
//! no function here can invent either.

use crate::Hash64;
use crate::palw_step::{
    PALW_STEP_INPUT_CHECKPOINT_STATE, PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN,
    PALW_STEP_OBJECT_VERSION_V1, PalwShapeProfileV3, PalwStepError, PalwStepLaneV1, PalwStepNodeRoleV1, PalwStepNodeV1,
    PalwStepOpKindV1, PalwStepOutLenV1, kernel_semantics_id_v1,
};
use crate::palw_step_refute::{
    KDESC_A16_ADD_ELEM, KDESC_A16_ATTN_SCORES, KDESC_A16_ATTN_VALUES, KDESC_A16_EMBED, KDESC_A16_MATMUL_RESCALE, KDESC_A16_REQUANTIZE, KDESC_A16_RMS_NORM, KDESC_A16_SOFTMAX, KDESC_BASE0_SILU,
    KDESC_Q36_DECAY, KDESC_Q36_GATE_APPLY, KDESC_Q36_GDN_STEP, KDESC_Q36_L2_NORM, KDESC_Q36_MATMUL_GROUPED,
    KDESC_Q36_MATMUL_GROUPED_WIDE, KDESC_Q36_MOE_COMBINE, KDESC_Q36_MUL_WIDE, KDESC_Q36_RESCALE_ROW,
    KDESC_Q36_RMS_NORM_WIDE, KDESC_Q36_ROPE_PARTIAL, KDESC_Q36_ROUTER_TOPK, KDESC_Q36_SIGMOID, KDESC_Q36_SSM_CONV,
};

/// The int8 dtype byte, as `palw_qwen25_profile` uses it. Every weight in the tier is `int8` rows
/// plus (for the grouped projections) an `i8` exponent per 32 of them; there is no second type,
/// and variance would mean it is not this class.
pub const QWEN36_WEIGHT_DTYPE_I8: u8 = 24;

/// The measured geometry. Every field is read from the pinned GGUF's metadata — a profile that
/// disagrees with the file describes an execution that never ran, and the court would then
/// adjudicate steps against it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwQwen36GeometryV1 {
    pub layer_count: u16,
    /// Layer `i` is full attention iff `(i + 1) % interval == 0`.
    pub full_attention_interval: u16,
    pub hidden_dim: u32,
    pub attn_heads: u16,
    pub attn_kv_heads: u16,
    pub attn_head_dim: u32,
    pub rope_dims: u16,
    pub rope_freq_base_bits: u32,
    pub gdn_k_heads: u16,
    pub gdn_v_heads: u16,
    pub gdn_head_dim: u32,
    pub gdn_conv_kernel: u16,
    pub n_experts: u32,
    pub experts_per_token: u32,
    pub moe_dim: u32,
    pub shared_dim: u32,
    pub vocab_size: u32,
    pub n_ctx: u32,
    pub n_threads: u32,
    /// The integer RMS-norm epsilon in Qk. The config says `1e-06`; this is the integer the class
    /// is registered with, because the court recomputes with the CLASS's constant.
    pub rms_eps_q: i64,
    pub tile_len: u32,
}

/// `Qwen3.6-35B-A3B`, from `Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf` (23,938,321,728 bytes),
/// read 2026-08-26. The same readings the converter prints on every run.
pub const QWEN36_35B_A3B: PalwQwen36GeometryV1 = PalwQwen36GeometryV1 {
    layer_count: 40,
    full_attention_interval: 4,
    hidden_dim: 2048,
    attn_heads: 16,
    attn_kv_heads: 2,
    attn_head_dim: 256,
    rope_dims: 64,
    // 1e7 as f32.
    rope_freq_base_bits: 0x4B18_9680,
    gdn_k_heads: 16,
    gdn_v_heads: 32,
    gdn_head_dim: 128,
    gdn_conv_kernel: 4,
    n_experts: 256,
    experts_per_token: 8,
    moe_dim: 512,
    shared_dim: 512,
    vocab_size: 248_320,
    n_ctx: 512,
    n_threads: 1,
    rms_eps_q: 17,
    tile_len: 256,
};

/// A step's output width, named the way the engine names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum W {
    Hidden,
    /// The GatedDeltaNet's key block (`k_heads × head_dim`) — q and k are the same width.
    GdnK,
    /// Its value block (`v_heads × head_dim`), which is also the `z` gate's width.
    GdnV,
    /// The convolution's row: `2·GdnK + GdnV`, q and k and v concatenated.
    Conv,
    /// One lane per value head (the two gates).
    GdnHeads,
    /// One head of the recurrence.
    HeadV,
    /// One head of the key space.
    HeadK,
    QDim,
    KvDim,
    Experts,
    /// Two lanes per chosen expert: its index and its weight.
    TopK2,
    /// The eight chosen experts' intermediate rows, concatenated.
    RoutedMid,
    /// The eight chosen experts' outputs, concatenated.
    RoutedOut,
    SharedMid,
    One,
    Vocab,
    /// `attn_heads × kv_len(position)` — the scores and their softmax, head-major.
    KvPerHead,
}

/// Where a step's operand comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum I {
    Step(u16),
    LayerIn,
    CachedK,
    CachedV,
    /// The recurrence's state and the convolution's window — registration-opaque until the state
    /// chunk map registers, and `Unadjudicable` rather than a conviction until then.
    State,
}

#[derive(Clone, Copy, Debug)]
struct Ir {
    op: PalwStepOpKindV1,
    role: PalwStepNodeRoleV1,
    kernel: &'static str,
    weight: &'static str,
    out: W,
    inputs: &'static [I],
}

const fn n(op: PalwStepOpKindV1, kernel: &'static str, weight: &'static str, out: W, inputs: &'static [I]) -> Ir {
    Ir { op, role: PalwStepNodeRoleV1::Plain, kernel, weight, out, inputs }
}
const fn c(
    op: PalwStepOpKindV1,
    role: PalwStepNodeRoleV1,
    kernel: &'static str,
    weight: &'static str,
    out: W,
    inputs: &'static [I],
) -> Ir {
    Ir { op, role, kernel, weight, out, inputs }
}

use I::{CachedK, CachedV, LayerIn, State, Step};
use PalwStepOpKindV1 as K;
use W::{Conv, Experts, GdnHeads, GdnK, GdnV, HeadK, HeadV, Hidden, KvDim, KvPerHead, One, QDim, RoutedMid, RoutedOut, SharedMid, TopK2, Vocab};

/// The mixture, appended verbatim to both arms — it is the same computation after either.
///
/// `FIRST` is the IR index this block starts at, because an input reference is an index into the
/// whole table and the two arms are different lengths. Written as a function of it rather than as
/// two copies, so the two tables cannot drift.
macro_rules! moe_tail {
    ($first:expr) => {
        [
            // The stream that reaches the mixture, normalized.
            n(K::RmsNorm, KDESC_A16_RMS_NORM, "", Hidden, &[Step($first - 1)]),
            n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.ffn_norm.a16", Hidden, &[Step($first)]),
            // Route. The logits are narrowed to codes BEFORE the selection, because the tie rule
            // is defined on what the class commits to.
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.ffn_router.weight", Experts, &[Step($first + 1)]),
            n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.ffn_router.a16", Experts, &[Step($first + 2)]),
            n(K::SoftMax, KDESC_Q36_ROUTER_TOPK, "blk.{layer}.ffn_router_topk.a16", TopK2, &[Step($first + 3)]),
            // The eight chosen experts, as the concatenation the engine builds.
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.ffn_gate_exps.routed", RoutedMid, &[Step($first + 1)]),
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.ffn_up_exps.routed", RoutedMid, &[Step($first + 1)]),
            n(K::Silu, KDESC_BASE0_SILU, "", RoutedMid, &[Step($first + 5)]),
            n(K::MulElem, KDESC_Q36_MUL_WIDE, "blk.{layer}.ffn_expert_gated.a16", RoutedMid, &[Step($first + 7), Step($first + 6)]),
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.ffn_down_exps.routed", RoutedOut, &[Step($first + 8)]),
            n(K::MulElem, KDESC_Q36_MOE_COMBINE, "blk.{layer}.ffn_combine.a16", Hidden, &[Step($first + 9), Step($first + 4)]),
            // The shared expert, always on, behind its own scalar gate.
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.ffn_shared_gate.weight", SharedMid, &[Step($first + 1)]),
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.ffn_shared_up.weight", SharedMid, &[Step($first + 1)]),
            n(K::Silu, KDESC_BASE0_SILU, "", SharedMid, &[Step($first + 11)]),
            n(K::MulElem, KDESC_Q36_MUL_WIDE, "blk.{layer}.ffn_shared_gated.a16", SharedMid, &[Step($first + 13), Step($first + 12)]),
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.ffn_shared_down.weight", Hidden, &[Step($first + 14)]),
            n(K::MatMulQuant, KDESC_A16_MATMUL_RESCALE, "blk.{layer}.ffn_shared_scalar.weight", One, &[Step($first + 1)]),
            n(K::Sigmoid, KDESC_Q36_SIGMOID, "", One, &[Step($first + 16)]),
            n(K::MulElem, KDESC_Q36_MUL_WIDE, "blk.{layer}.ffn_shared_apply.a16", Hidden, &[Step($first + 15), Step($first + 17)]),
            n(K::AddElem, KDESC_A16_ADD_ELEM, "", Hidden, &[Step($first + 10), Step($first + 18)]),
            n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.ffn_moe_out.a16", Hidden, &[Step($first + 19)]),
            // The residual. The stream is aligned to the delta's scale, added, and renormalized.
            n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.ffn_align.a16", Hidden, &[Step($first - 1)]),
            n(K::AddElem, KDESC_A16_ADD_ELEM, "", Hidden, &[Step($first + 21), Step($first + 20)]),
            n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.ffn_residual.a16", Hidden, &[Step($first + 22)]),
        ]
    };
}

/// **The GatedDeltaNet layer, in `Qwen36Engine::linear_arm`'s order.**
const QWEN36_LINEAR_IR: &[Ir] = &{
    const HEAD: [Ir; 24] = [
        // --- the arm's input ------------------------------------------------------------------
        n(K::RmsNorm, KDESC_A16_RMS_NORM, "", Hidden, &[LayerIn]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_norm.a16", Hidden, &[Step(0)]),
        // --- the four projections ---------------------------------------------------------------
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.linear_q.weight", GdnK, &[Step(1)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.linear_k.weight", GdnK, &[Step(1)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.linear_v.weight", GdnV, &[Step(1)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.linear_z.weight", GdnV, &[Step(1)]),
        // --- the convolution, over the four-position window -------------------------------------
        n(K::SsmConv, KDESC_Q36_SSM_CONV, "blk.{layer}.linear_conv.weight", Conv, &[Step(2), Step(3), Step(4), State]),
        n(K::Silu, KDESC_BASE0_SILU, "", Conv, &[Step(6)]),
        // **Per head, not per row.** The row holds sixteen query heads, sixteen key heads and
        // thirty-two value heads, and one exponent over all of them is set by the loudest.
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.linear_conv_act.a16", Conv, &[Step(7)]),
        // --- the two gates ----------------------------------------------------------------------
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.linear_dt.weight", GdnHeads, &[Step(1)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.linear_beta.weight", GdnHeads, &[Step(1)]),
        // `exp(a · softplus(dt + dt_bias))`. The bias and the coefficient are both registration
        // constants and both are read here; `sigmoid(-dt)^a` cannot express this checkpoint's
        // biases, which reach 15.6.
        n(K::Softplus, KDESC_Q36_DECAY, "blk.{layer}.linear_decay.a16", GdnHeads, &[Step(9)]),
        n(K::Sigmoid, KDESC_Q36_SIGMOID, "", GdnHeads, &[Step(10)]),
        // --- the recurrence, per head -----------------------------------------------------------
        n(K::L2Norm, KDESC_Q36_L2_NORM, "", HeadK, &[Step(8)]),
        n(K::L2Norm, KDESC_Q36_L2_NORM, "", HeadK, &[Step(8)]),
        c(
            K::GatedDeltaNet,
            PalwStepNodeRoleV1::Plain,
            KDESC_Q36_GDN_STEP,
            "blk.{layer}.linear_gdn.a16",
            HeadV,
            &[Step(13), Step(8), Step(14), State],
        ),
        // --- the output norm and gate -----------------------------------------------------------
        n(K::RmsNorm, KDESC_Q36_RMS_NORM_WIDE, "blk.{layer}.linear_norm_eps.a16", HeadV, &[Step(15)]),
        n(K::Scale, KDESC_Q36_RESCALE_ROW, "blk.{layer}.linear_norm.a16", GdnV, &[Step(16)]),
        n(K::Silu, KDESC_BASE0_SILU, "", GdnV, &[Step(5)]),
        n(K::MulElem, KDESC_Q36_MUL_WIDE, "blk.{layer}.linear_gated.a16", GdnV, &[Step(17), Step(18)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.linear_o.weight", Hidden, &[Step(19)]),
        // --- the residual -----------------------------------------------------------------------
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_align.a16", Hidden, &[LayerIn]),
        n(K::AddElem, KDESC_A16_ADD_ELEM, "", Hidden, &[Step(21), Step(20)]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_residual.a16", Hidden, &[Step(22)]),
    ];
    const TAIL: [Ir; 24] = moe_tail!(24u16);
    let mut all = [HEAD[0]; 48];
    let mut i = 0;
    while i < 24 {
        all[i] = HEAD[i];
        all[24 + i] = TAIL[i];
        i += 1;
    }
    all
};

/// **The gated-attention layer, in `Qwen36Engine::full_arm`'s order.**
const QWEN36_ATTN_IR: &[Ir] = &{
    const HEAD: [Ir; 21] = [
        n(K::RmsNorm, KDESC_A16_RMS_NORM, "", Hidden, &[LayerIn]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_norm.a16", Hidden, &[Step(0)]),
        // The query and the output gate are one fused tensor in the checkpoint, INTERLEAVED per
        // head; the converter de-interleaves them, so the class registers two matrices.
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.attn_q.weight", QDim, &[Step(1)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.attn_gate.weight", QDim, &[Step(1)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.attn_k.weight", KvDim, &[Step(1)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.attn_v.weight", KvDim, &[Step(1)]),
        // QK-norm per head, before the rotation.
        n(K::RmsNorm, KDESC_A16_RMS_NORM, "", QDim, &[Step(2)]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_q_norm.a16", QDim, &[Step(6)]),
        n(K::RmsNorm, KDESC_A16_RMS_NORM, "", KvDim, &[Step(4)]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_k_norm.a16", KvDim, &[Step(8)]),
        // The rotation, over the first 64 of each head's 256 dimensions. **The cache holds the
        // ROTATED key**: a court recomputing against unrotated keys convicts every honest producer.
        n(K::RopeImrope, KDESC_Q36_ROPE_PARTIAL, "blk.{layer}.rope_table", QDim, &[Step(7)]),
        c(K::RopeImrope, PalwStepNodeRoleV1::KCacheWrite, KDESC_Q36_ROPE_PARTIAL, "blk.{layer}.rope_table", KvDim, &[Step(9)]),
        c(K::MulElem, PalwStepNodeRoleV1::VCacheWrite, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_v_cache.a16", KvDim, &[Step(5)]),
        // Scores, softmax, values.
        n(K::MatMulQuant, KDESC_A16_ATTN_SCORES, "blk.{layer}.attn_logits.a16", KvPerHead, &[Step(10), CachedK]),
        n(K::SoftMax, KDESC_A16_SOFTMAX, "blk.{layer}.attn_softmax.a16", KvPerHead, &[Step(13)]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_probs.a16", KvPerHead, &[Step(14)]),
        n(K::MatMulQuant, KDESC_A16_ATTN_VALUES, "blk.{layer}.attn_values.a16", QDim, &[Step(15), CachedV]),
        // The output gate, then the projection.
        n(K::Sigmoid, KDESC_Q36_SIGMOID, "", QDim, &[Step(3)]),
        n(K::MulElem, KDESC_Q36_GATE_APPLY, "blk.{layer}.attn_gated.a16", QDim, &[Step(16), Step(17)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.attn_o.weight", Hidden, &[Step(18)]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_align.a16", Hidden, &[LayerIn]),
    ];
    const MID: [Ir; 2] = [
        n(K::AddElem, KDESC_A16_ADD_ELEM, "", Hidden, &[Step(20), Step(19)]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_residual.a16", Hidden, &[Step(21)]),
    ];
    const TAIL: [Ir; 24] = moe_tail!(23u16);
    let mut all = [HEAD[0]; 47];
    let mut i = 0;
    while i < 21 {
        all[i] = HEAD[i];
        i += 1;
    }
    all[21] = MID[0];
    all[22] = MID[1];
    let mut j = 0;
    while j < 24 {
        all[23 + j] = TAIL[j];
        j += 1;
    }
    all
};

/// The graph's head: the embedding gather and the per-token lift that follows it.
const QWEN36_PRE_IR: &[Ir] = &[
    n(K::EmbedLookup, KDESC_A16_EMBED, "token_embd.weight", Hidden, &[]),
    // **Per token, not per class.** One scale for a 248,320-row table is one scale for its
    // outliers, and an ordinary row then lands on a fraction of the range.
    n(K::MulElem, KDESC_A16_REQUANTIZE, "embed_lift.a16", Hidden, &[Step(0)]),
];

/// The graph's tail: the final norm and the unembedding.
const QWEN36_POST_IR: &[Ir] = &[
    n(K::RmsNorm, KDESC_A16_RMS_NORM, "", Hidden, &[LayerIn]),
    n(K::MulElem, KDESC_A16_REQUANTIZE, "final_norm.a16", Hidden, &[Step(0)]),
    n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "output.weight", Vocab, &[Step(1)]),
];

fn width(w: W, g: &PalwQwen36GeometryV1) -> PalwStepOutLenV1 {
    let k_dim = g.gdn_k_heads as u32 * g.gdn_head_dim;
    let v_dim = g.gdn_v_heads as u32 * g.gdn_head_dim;
    let fixed = |n: u32| PalwStepOutLenV1::Fixed { elements: n };
    match w {
        W::Hidden => fixed(g.hidden_dim),
        W::GdnK => fixed(k_dim),
        W::GdnV => fixed(v_dim),
        W::Conv => fixed(2 * k_dim + v_dim),
        W::GdnHeads => fixed(g.gdn_v_heads as u32),
        W::HeadV | W::HeadK => fixed(g.gdn_head_dim),
        W::QDim => fixed(g.attn_heads as u32 * g.attn_head_dim),
        W::KvDim => fixed(g.attn_kv_heads as u32 * g.attn_head_dim),
        W::Experts => fixed(g.n_experts),
        W::TopK2 => fixed(2 * g.experts_per_token),
        W::RoutedMid => fixed(g.experts_per_token * g.moe_dim),
        W::RoutedOut => fixed(g.experts_per_token * g.hidden_dim),
        W::SharedMid => fixed(g.shared_dim),
        W::One => fixed(1),
        W::Vocab => fixed(g.vocab_size),
        W::KvPerHead => PalwStepOutLenV1::KvScaled { multiplier: g.attn_heads as u32 },
    }
}

fn project(ir: &[Ir], g: &PalwQwen36GeometryV1, layer_span: usize) -> Vec<PalwStepNodeV1> {
    ir.iter()
        .map(|node| PalwStepNodeV1 {
            op_kind: node.op,
            role: node.role,
            weight_name: node.weight.to_string(),
            weight_dtypes: if node.weight.is_empty() { Vec::new() } else { vec![QWEN36_WEIGHT_DTYPE_I8; layer_span] },
            out_len: width(node.out, g),
            // A node narrower than the minimum tile is one tile; the cap is a floor on the tile,
            // not on the row.
            tile_len: match width(node.out, g) {
                PalwStepOutLenV1::Fixed { elements } => g.tile_len.min(elements.max(crate::palw_step::PALW_STEP_MIN_TILE_LEN)),
                PalwStepOutLenV1::KvScaled { .. } => g.tile_len,
            },
            kernel_semantics_id: kernel_semantics_id_v1(node.kernel),
            input_refs: node
                .inputs
                .iter()
                .map(|i| match i {
                    I::Step(s) => *s,
                    I::LayerIn => PALW_STEP_INPUT_LAYER_IN,
                    I::CachedK => PALW_STEP_INPUT_KV_K,
                    I::CachedV => PALW_STEP_INPUT_KV_V,
                    I::State => PALW_STEP_INPUT_CHECKPOINT_STATE,
                })
                .collect(),
        })
        .collect()
}

/// How many layers each table covers — the dtype list carries one byte per layer, so the two
/// tables split the count by the attention interval and the split must be exact.
fn layer_spans(g: &PalwQwen36GeometryV1) -> (usize, usize) {
    let mut attn = 0usize;
    for i in 0..g.layer_count {
        if g.full_attention_interval != 0 && (i + 1).is_multiple_of(g.full_attention_interval) {
            attn += 1;
        }
    }
    (g.layer_count as usize - attn, attn)
}

/// **The class's shape profile.** Projected from the IR above, which is the engine's own order.
pub fn qwen36_profile_v1(g: PalwQwen36GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    let (gdn_span, attn_span) = layer_spans(&g);
    let profile = PalwShapeProfileV3 {
        version: PALW_STEP_OBJECT_VERSION_V1,
        lane: PalwStepLaneV1::Int32,
        layer_count: g.layer_count,
        full_attention_interval: g.full_attention_interval,
        hidden_dim: g.hidden_dim,
        // The mixture's expert width. There is no dense FFN in this graph.
        ffn_dim: g.moe_dim,
        attn_heads: g.attn_heads,
        attn_kv_heads: g.attn_kv_heads,
        attn_head_dim: g.attn_head_dim,
        rope_dims: g.rope_dims,
        // Text-only: the three mRoPE sections carry the same position, so the rotation is the
        // plain NEOX one and the sections are not a degree of freedom.
        rope_sections: [g.rope_dims / 2, 0, 0, 0],
        rope_freq_base_bits: g.rope_freq_base_bits,
        // The float epsilons are not this class's arithmetic; the integer one below is.
        rms_eps_bits: 0x3589_705F,
        l2_eps_bits: 0x3589_705F,
        base0_rms_eps_q: g.rms_eps_q,
        gdn_heads: g.gdn_v_heads,
        gdn_head_k_dim: g.gdn_head_dim,
        gdn_head_v_dim: g.gdn_head_dim,
        gdn_conv_kernel: g.gdn_conv_kernel,
        vocab_size: g.vocab_size,
        repack_on: 0,
        llamafile_on: 0,
        flash_attn_disabled: 1,
        fused_gdn_on: 0,
        use_ref_off: 1,
        kv_cache_f16: 0,
        n_ctx: g.n_ctx,
        n_batch: 1,
        n_ubatch: 1,
        n_seq: 1,
        n_threads: g.n_threads,
        pre_nodes: project(QWEN36_PRE_IR, &g, 1),
        gdn_nodes: project(QWEN36_LINEAR_IR, &g, gdn_span),
        attn_nodes: project(QWEN36_ATTN_IR, &g, attn_span),
        post_nodes: project(QWEN36_POST_IR, &g, 1),
        reference_ruleset_id: crate::palw_reference::reference_arithmetic_ruleset_id_v2(),
        transcendental_bindings: Vec::new(),
        contraction_facts: Vec::new(),
        kv_chunk_calls: 0,
        state_chunk_map_id: Hash64::default(),
    };
    profile.validate_shape()?;
    Ok(profile)
}

/// **Every kernel this graph can reach**, for the ADR-0038 A4 coverage gate.
///
/// Read from the projected profile rather than from a hand-kept list: a list beside a table is a
/// list that drifts from it, and the gate's whole promise is that the set it certifies is the set
/// the class executes.
pub fn qwen36_reachable_kernels_v1(g: PalwQwen36GeometryV1) -> Result<std::collections::BTreeSet<Hash64>, PalwStepError> {
    let p = qwen36_profile_v1(g)?;
    let mut ids = std::collections::BTreeSet::new();
    for table in [&p.pre_nodes, &p.gdn_nodes, &p.attn_nodes, &p.post_nodes] {
        for node in table {
            ids.insert(node.kernel_semantics_id);
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The profile is a graph a court can walk: every table inside the cap, every reference
    /// strictly earlier, every width non-zero.
    #[test]
    fn the_profile_validates() {
        let p = qwen36_profile_v1(QWEN36_35B_A3B).expect("the measured geometry is adjudicable");
        assert_eq!(p.gdn_nodes.len(), 48, "the GatedDeltaNet layer is the arm plus the mixture");
        assert_eq!(p.attn_nodes.len(), 47, "the gated-attention layer is the arm plus the mixture");
        assert!(p.gdn_nodes.len() <= crate::palw_step::PALW_STEP_MAX_NODES_PER_TABLE);
        assert!(p.attn_nodes.len() <= crate::palw_step::PALW_STEP_MAX_NODES_PER_TABLE);
        // 30 recurrent layers and 10 attention ones, which is what `(i+1) % 4 == 0` gives at 40.
        assert_eq!(layer_spans(&QWEN36_35B_A3B), (30, 10));
    }

    /// **ADR-0039's precondition, as a test.** Every kernel the graph reaches is one the
    /// adjudicator can re-execute — not "almost every", which is the state A4 exists to refuse.
    #[test]
    fn every_reachable_kernel_is_catalogued() {
        let reachable = qwen36_reachable_kernels_v1(QWEN36_35B_A3B).expect("the profile projects");
        let catalogued = crate::palw_step_refute::catalogued_kernel_ids_v1();
        let missing: Vec<_> = reachable.difference(&catalogued).collect();
        assert!(missing.is_empty(), "{} of the graph's kernels are uncatalogued", missing.len());
        // And the count is the graph's, not a subset that happens to be covered.
        assert!(reachable.len() >= 18, "the hybrid graph reaches at least eighteen distinct kernels, got {}", reachable.len());
    }

    /// The gate itself, through its own constructor — "we checked coverage" and "a certificate
    /// exists" are the same fact only if this passes.
    #[test]
    fn the_coverage_gate_certifies_this_class() {
        use crate::palw_catalog_coverage::{PalwReachableKernelSetV1, verify_catalog_coverage_v1};
        // The catalog side is read from the adjudicator by the gate itself, never passed in: a
        // caller who supplies both sides certifies its own two parameters.
        let reachable = PalwReachableKernelSetV1 {
            execution_class_id: Hash64::default(),
            kernel_ids: qwen36_reachable_kernels_v1(QWEN36_35B_A3B).expect("the profile projects"),
        };
        verify_catalog_coverage_v1(&reachable).expect("every reachable kernel is adjudicable");
    }

    /// A node may only read something computed before it — the property that makes a step's input
    /// set canonical, and the one a hand-written table gets wrong.
    #[test]
    fn every_reference_points_backwards() {
        let p = qwen36_profile_v1(QWEN36_35B_A3B).expect("the profile projects");
        for (name, table) in [("gdn", &p.gdn_nodes), ("attn", &p.attn_nodes)] {
            for (i, node) in table.iter().enumerate() {
                for r in &node.input_refs {
                    if *r < crate::palw_step::PALW_STEP_INPUT_SENTINEL_MIN {
                        assert!((*r as usize) < i, "{name} node {i} reads node {r}, which is not earlier");
                    }
                }
            }
        }
    }
}
