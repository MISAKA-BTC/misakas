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
    PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN, PALW_STEP_OBJECT_VERSION_V1, PalwShapeProfileV3,
    PalwStepError, PalwStepLaneV1, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1, kernel_semantics_id_v1,
};
use crate::palw_step_refute::{
    KDESC_A16_ADD_ELEM, KDESC_A16_ATTN_SCORES, KDESC_A16_ATTN_VALUES, KDESC_A16_EMBED, KDESC_A16_MATMUL_RESCALE, KDESC_A16_REQUANTIZE,
    KDESC_A16_RMS_NORM, KDESC_A16_SOFTMAX, KDESC_Q36_DECAY, KDESC_Q36_GATE_APPLY, KDESC_Q36_GDN_STEP, KDESC_Q36_HEAD_RMS_NORM,
    KDESC_Q36_L2_NORM, KDESC_Q36_MATMUL_GROUPED, KDESC_Q36_MATMUL_GROUPED_WIDE, KDESC_Q36_MOE_COMBINE, KDESC_Q36_MUL_WIDE,
    KDESC_Q36_RESCALE_ROW, KDESC_Q36_RMS_NORM_WIDE, KDESC_Q36_ROPE_PARTIAL, KDESC_Q36_ROUTER_TOPK, KDESC_Q36_SIGMOID, KDESC_Q36_SILU,
    KDESC_Q36_SSM_CONV,
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
    // **8, and it is the whole-close derivation that says so.** What the context multiplies is
    // the recurrence's replay evidence — five sliced rows per position, and even in range form
    // each row is one Merkle sibling set, so the replay costs ~8.4 KB of mostly-path per
    // position — and its per-head recomputation (`n_ctx × 128 × 128 × 4` MACs). Eight positions
    // hold the worst close at ~90 % of the 80 KiB carrier with the canonical job (7 + 2,
    // footprint 8) at the boundary. The runtime's rotary table still covers 512: this bounds the
    // JOB a claim may declare, not what the engine serves off-chain. A larger context returns
    // when the recurrence's replay is checkpoint-anchored (the state chunk map is registered;
    // the anchor consumption is wired for attention and not yet for the recurrence).
    n_ctx: 8,
    n_threads: 1,
    rms_eps_q: 17,
    // 512, not 256: at 256 the worst-case step space is 4,198,428 leaves against the ladder's
    // 2^22 — over by a tenth of a percent, which is the worst kind of over. The tile is a court
    // fact (what one dispute opens), not an engine fact, and 512 puts the space at half the
    // ceiling with the terminal opening still far inside the byte budget.
    tile_len: 512,
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

use I::{CachedK, CachedV, LayerIn, Step};
use PalwStepOpKindV1 as K;
use W::{Conv, Experts, GdnHeads, GdnK, GdnV, Hidden, KvDim, KvPerHead, One, QDim, RoutedMid, RoutedOut, SharedMid, TopK2, Vocab};

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
            n(K::Silu, KDESC_Q36_SILU, "", RoutedMid, &[Step($first + 5)]),
            n(K::MulElem, KDESC_Q36_MUL_WIDE, "blk.{layer}.ffn_expert_gated.a16", RoutedMid, &[Step($first + 7), Step($first + 6)]),
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.ffn_down_exps.routed", RoutedOut, &[Step($first + 8)]),
            n(K::MulElem, KDESC_Q36_MOE_COMBINE, "blk.{layer}.ffn_combine.a16", Hidden, &[Step($first + 9), Step($first + 4)]),
            // The shared expert, always on, behind its own scalar gate.
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.ffn_shared_gate.weight", SharedMid, &[Step($first + 1)]),
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.ffn_shared_up.weight", SharedMid, &[Step($first + 1)]),
            n(K::Silu, KDESC_Q36_SILU, "", SharedMid, &[Step($first + 11)]),
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
        // The window is the three projections at the last four positions — a per-ref position
        // set like the KV arms', not a sentinel. Positions before the sequence start are zero
        // rows, which is the window the engine starts from.
        n(K::SsmConv, KDESC_Q36_SSM_CONV, "blk.{layer}.linear_conv.weight", Conv, &[Step(2), Step(3), Step(4)]),
        n(K::Silu, KDESC_Q36_SILU, "", Conv, &[Step(6)]),
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
        // --- the recurrence -----------------------------------------------------------------
        // Per-head structure lives INSIDE the kernels (the arm splits by the profile's head
        // dims), the same way `a16_attn_scores` holds sixteen query heads in one node — a
        // one-head node would declare a step space that does not contain heads 1..31.
        n(K::L2Norm, KDESC_Q36_L2_NORM, "", GdnK, &[Step(8)]),
        n(K::L2Norm, KDESC_Q36_L2_NORM, "", GdnK, &[Step(8)]),
        // **Genesis-anchored replay, exactly as the float `GdnCore` is adjudicated.** The state
        // is not an opened operand: the five rows are read at EVERY position from the genesis up
        // to the challenged one, and the court replays the recurrence. A registered state chunk
        // map later turns this checkpoint-anchored; the sentinel it would use is refused today
        // as registration-opaque, which is the honest answer rather than a hole.
        c(
            K::GatedDeltaNet,
            PalwStepNodeRoleV1::Plain,
            KDESC_Q36_GDN_STEP,
            "blk.{layer}.linear_gdn.a16",
            GdnV,
            &[Step(13), Step(8), Step(14), Step(11), Step(12)],
        ),
        // --- the output norm and gate -----------------------------------------------------------
        n(K::RmsNorm, KDESC_Q36_RMS_NORM_WIDE, "blk.{layer}.linear_norm_eps.a16", GdnV, &[Step(15)]),
        n(K::Scale, KDESC_Q36_RESCALE_ROW, "blk.{layer}.linear_norm.a16", GdnV, &[Step(16)]),
        n(K::Silu, KDESC_Q36_SILU, "", GdnV, &[Step(5)]),
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
        n(K::RmsNorm, KDESC_Q36_HEAD_RMS_NORM, "", QDim, &[Step(2)]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_q_norm.a16", QDim, &[Step(6)]),
        n(K::RmsNorm, KDESC_Q36_HEAD_RMS_NORM, "", KvDim, &[Step(4)]),
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

/// The Decision-B artifact opening a challenged matmul tile costs is `tile × in_w` bytes, and the
/// whole close must ride one lifecycle carrier (~80 KiB) — so the tile is BUDGETED per node from
/// the row it reduces over. 24 KiB, not most of the carrier: the opening is one TERM of the close,
/// and the evidence beside it (the opened input rows and their paths) is the other. A budget that
/// spent the whole ceiling on weights left nothing for the rows those weights multiply.
/// One tile for every node was the U-shape: the number that made the fat matmuls' openings fit
/// exploded the narrow nodes' leaf counts, and vice versa. The field was always per-node; only
/// the projection flattened it.
const QWEN36_MATMUL_OPENING_BUDGET: usize = 24 * 1024;

/// **Strip the shared-expert subgraph from a layer IR, for members with `shared_dim == 0`.**
///
/// qwen3moe (Qwen3-Coder and kin) routes every token through the mixture ONLY — there is no
/// always-on shared expert, no scalar gate over it, and no add that folds it back in. The IR
/// tables are consts written for the hybrid's layer, so the projection derives the smaller graph
/// instead of a second hand-kept table (a table beside a table is how graphs drift):
///
/// 1. seed: every node whose weight lives under `ffn_shared`;
/// 2. closure: a node ALL of whose step inputs are already being dropped (the anonymous Silu and
///    Sigmoid between the seeded matmuls) joins them;
/// 3. references into the dropped set are removed from surviving nodes' input lists, and a
///    two-input add left with one input this way (the fold of mixture + shared) is an identity —
///    it is dropped too, and references to IT forward to its surviving input;
/// 4. everything is re-indexed.
///
/// For a hybrid geometry the seed is empty and the function is the identity, so the shipped
/// class's profile — and therefore its id — cannot move.
fn strip_shared_expert(ir: &[Ir]) -> Vec<Ir> {
    let mut dropped = vec![false; ir.len()];
    for (i, node) in ir.iter().enumerate() {
        if node.weight.contains("ffn_shared") {
            dropped[i] = true;
        }
    }
    loop {
        let mut changed = false;
        for (i, node) in ir.iter().enumerate() {
            if dropped[i] || node.inputs.is_empty() {
                continue;
            }
            let all_dropped = node.inputs.iter().all(|r| matches!(r, I::Step(s) if dropped[*s as usize]));
            if all_dropped {
                dropped[i] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // The fold: an AddElem whose OTHER input was the shared path. Forward it to the mixture side.
    let mut forward: Vec<Option<u16>> = vec![None; ir.len()];
    for (i, node) in ir.iter().enumerate() {
        if dropped[i] || node.op != PalwStepOpKindV1::AddElem {
            continue;
        }
        let survivors: Vec<u16> = node
            .inputs
            .iter()
            .filter_map(|r| match r {
                I::Step(s) if !dropped[*s as usize] => Some(*s),
                _ => None,
            })
            .collect();
        if node.inputs.len() == 2 && survivors.len() == 1 {
            dropped[i] = true;
            forward[i] = Some(survivors[0]);
        }
    }
    let mut new_index = vec![0u16; ir.len()];
    let mut next = 0u16;
    for (i, d) in dropped.iter().enumerate() {
        if !*d {
            new_index[i] = next;
            next += 1;
        }
    }
    let resolve = |mut s: u16| -> u16 {
        while let Some(f) = forward[s as usize] {
            s = f;
        }
        assert!(!dropped[s as usize], "a surviving node referenced a dropped one that forwards nowhere");
        new_index[s as usize]
    };
    let mut out = Vec::with_capacity(ir.len());
    for (i, node) in ir.iter().enumerate() {
        if dropped[i] {
            continue;
        }
        let inputs: Vec<I> = node
            .inputs
            .iter()
            .map(|r| match r {
                I::Step(s) => I::Step(resolve(*s)),
                other => *other,
            })
            .collect();
        out.push(Ir { inputs: Box::leak(inputs.into_boxed_slice()), ..*node });
    }
    out
}

fn project(ir: &[Ir], g: &PalwQwen36GeometryV1, layer_span: usize) -> Vec<PalwStepNodeV1> {
    // **A span of zero layers is an empty table, not a table of zero-weight nodes.** The
    // full-attention-only members of this lineage (qwen3moe: every layer is attention,
    // `full_attention_interval == 1`) have no GDN layers at all, and `validate_shape` rightly
    // refuses a declared node table for a layer kind that does not exist. The hybrid members
    // pass a positive span here and are untouched.
    if layer_span == 0 {
        return Vec::new();
    }
    let stripped;
    let ir = if g.shared_dim == 0 {
        stripped = strip_shared_expert(ir);
        &stripped[..]
    } else {
        ir
    };
    // A matmul's reduction width, from the IR's own wiring — the first input's row.
    let in_width = |node: &Ir| -> usize {
        node.inputs
            .first()
            .and_then(|i| match i {
                I::Step(s) => match width(ir[*s as usize].out, g) {
                    PalwStepOutLenV1::Fixed { elements } => Some(elements as usize),
                    PalwStepOutLenV1::KvScaled { .. } => None,
                },
                I::LayerIn => Some(g.hidden_dim as usize),
                _ => None,
            })
            .unwrap_or(g.hidden_dim as usize)
    };
    ir.iter()
        .map(|node| PalwStepNodeV1 {
            op_kind: node.op,
            role: node.role,
            weight_name: node.weight.to_string(),
            weight_dtypes: if node.weight.is_empty() { Vec::new() } else { vec![QWEN36_WEIGHT_DTYPE_I8; layer_span] },
            out_len: width(node.out, g),
            tile_len: {
                let out_elems = match width(node.out, g) {
                    PalwStepOutLenV1::Fixed { elements } => elements as usize,
                    PalwStepOutLenV1::KvScaled { .. } => usize::MAX,
                };
                let chosen = match node.op {
                    // Budgeted from the reduction width, so the opening fits the carrier whatever
                    // the row's fan-in: in_w 2048 → tile 32, in_w 512 → tile 128.
                    K::MatMulQuant => (QWEN36_MATMUL_OPENING_BUDGET / in_width(node).max(1))
                        .next_power_of_two()
                        .checked_shr(1)
                        .unwrap_or(1)
                        .clamp(crate::palw_step::PALW_STEP_MIN_TILE_LEN as usize, g.tile_len as usize),
                    // The head-sliced recurrence: the tile IS the head, by the slice derivation's
                    // own precondition (`tile_len == gdn_head_v_dim`).
                    K::GatedDeltaNet => g.gdn_head_dim as usize,
                    // The head-reducing norms: the tile IS the head, by their slice derivation's
                    // own precondition — a court opens one head, so a tile that spanned several
                    // would open rows the step does not read.
                    K::L2Norm => g.gdn_head_dim as usize,
                    K::RmsNorm if node.kernel == KDESC_Q36_RMS_NORM_WIDE => g.gdn_head_dim as usize,
                    K::RmsNorm if node.kernel == KDESC_Q36_HEAD_RMS_NORM => g.attn_head_dim as usize,
                    // **The evidence-heavy nodes get the minimum tile.** A step's close carries
                    // one opened leaf per covering tile of every ref it reads, so for a node that
                    // reads MANY rows the tile is an evidence multiplier and not an opening
                    // budget: the mixture's combine opens the challenged lanes of all eight
                    // experts' blocks, and the convolution opens four window positions. At the
                    // shared 512 those two priced the whole class (349 KB and 93 KB of evidence);
                    // at 16 they are the same arithmetic over a sixteenth of the bytes. Selected
                    // by KERNEL, because "how many rows does this node read" is a property of the
                    // program, not of the width.
                    // The two MULTI-ROW readers: the mixture's combine opens the challenged
                    // lanes of all eight expert blocks, the convolution four window positions. For
                    // them the tile is an evidence multiplier rather than an opening budget, so
                    // they take the minimum. Every other lane-sliced node keeps a WIDE tile: it is
                    // the SOURCE tile of whatever reads it, and a matmul reading a 2,048-lane row
                    // cut at 16 opens 128 leaves of path where 4 would do.
                    K::MulElem if node.kernel == KDESC_Q36_MOE_COMBINE => crate::palw_step::PALW_STEP_MIN_TILE_LEN as usize,
                    // The conv's tile is the RECURRENCE's slice unit: the head-sliced GDN reads
                    // one value-head slice of the conv row per replay position, and a conv tile
                    // equal to that slice makes the read exactly one leaf. Its own window close
                    // opens `4 × (tile / producer-tile)` leaves, which this width keeps small.
                    K::SsmConv => g.gdn_head_dim as usize,
                    _ => g.tile_len as usize,
                };
                // A node narrower than the minimum tile is one tile; the cap is a floor on the
                // tile, not on the row.
                (chosen.min(out_elems.max(crate::palw_step::PALW_STEP_MIN_TILE_LEN as usize))) as u32
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
#[cfg(test)]
mod qwen3moe_family {
    use super::*;
    /// **The qwen3moe (full-attention-only MoE) members are expressible and admissible.**
    ///
    /// Qwen3-Coder-30B-A3B's geometry — every layer attention (`full_attention_interval` 1, so
    /// the GDN table is EMPTY), no shared expert (`shared_dim` 0, so the shared subgraph is
    /// stripped), rope over the whole 128-dim head — projected through the same IR as the
    /// shipped hybrid and pushed through the REAL bundle's admission gate. The ladder measured
    /// 2026-08-28: n_ctx 4..=10 admit (close ≈54 KiB), 12 is past the step ladder — so the
    /// family's base registers at n_ctx 9 (canonical (7,2), footprint 8) with one rung of head
    /// room, and siblings take the remaining rungs.
    #[test]
    fn qwen3moe_geometry_probe() {
        let p = crate::config::params::palw_rc_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &p.palw_consensus_mode else { panic!() };
        for nctx in [4u32, 6, 8, 9, 10, 12, 16] {
            let g = PalwQwen36GeometryV1 {
                layer_count: 48,
                full_attention_interval: 1,
                hidden_dim: 2048,
                attn_heads: 32,
                attn_kv_heads: 4,
                attn_head_dim: 128,
                rope_dims: 128,
                rope_freq_base_bits: 0x4B18_9680,
                gdn_k_heads: 0,
                gdn_v_heads: 0,
                gdn_head_dim: 0,
                gdn_conv_kernel: 0,
                n_experts: 128,
                experts_per_token: 8,
                moe_dim: 768,
                shared_dim: 0,
                vocab_size: 151_936,
                n_ctx: nctx,
                n_threads: 1,
                rms_eps_q: 17,
                tile_len: 512,
            };
            let profile = match qwen36_profile_v1(g) { Ok(pr) => pr, Err(e) => { eprintln!("nctx {nctx}: profile err {e:?}"); continue } };
            let canonical = crate::palw_base0_profile::rc_job_context(&profile, (nctx - 1).min(7), 2);
            let reg = match crate::palw_class_admission_v2::palw_post_genesis_registration_v1(
                profile.clone(), canonical.clone(), kaspa_hashes::Hash64::default(), 1, 1, 5, 0,
                crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(kaspa_hashes::Hash64::default(), 0)), vec![]) {
                Ok(r) => r, Err(e) => { eprintln!("nctx {nctx}: builder err {e}"); continue }
            };
            let verdict = crate::palw_class_admission_v2::verify_class_admission_v2(b, &profile, &canonical, &reg);
            match nctx {
                4 | 6 | 8 | 9 | 10 => assert!(verdict.is_ok(), "n_ctx {nctx} fell out of the qwen3moe family's room: {verdict:?}"),
                _ => assert!(verdict.is_err(), "n_ctx {nctx} was admitted — the qwen3moe ceiling moved, revisit the ladder comment"),
            }
        }
    }
}
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
        // TILED — the scheme this class exists to prove out: at vocab 248,320 one flat row is
        // 993 KB against the ~80 KiB carrier, and `Qwen36Backend` commits the tiled root.
        logits_scheme_id: crate::palw_step_refute::tiled_logits_scheme_id_v1(),
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

/// **The canonical job's shape** — `(prefill, decode)`, the unit of this class's work.
///
/// Small on purpose: ten forward passes of a 35B hybrid is what one block costs, and per-class DAA
/// retargets the cadence to what producers actually achieve, so the job's size buys latency and
/// nothing else. The court prices this exact job (`pwu_per_inference` is its counted leaves), so
/// changing it changes the class id — which is what "canonical" means.
pub const QWEN36_RC_CANONICAL: (u32, u32) = (7, 2);

/// **Everything a chain needs to carry this class** — the profile, its catalog entry and the
/// genesis-form registration, derived from one geometry so no two of them can disagree.
///
/// `artifact_root` is the one input code cannot mint: which converted weights the class runs.
/// `share_permille`, `slash_value_per_pwu` and `initial_target` are the network's economics and
/// arrive from the bundle being assembled, because a class that chose its own would be choosing
/// its weight.
pub fn qwen36_registration_v1(
    artifact_root: Hash64,
    share_permille: u16,
    slash_value_per_pwu: u64,
    initial_target: u128,
) -> Result<
    (PalwShapeProfileV3, crate::palw_mode_v2::PalwClassCatalogEntryV2, crate::palw_state_v2::PalwConsensusObjectV2),
    PalwStepError,
> {
    let profile = qwen36_profile_v1(QWEN36_35B_A3B)?;
    let class_id = profile.shape_profile_id();
    let canonical = crate::palw_base0_profile::rc_job_context(&profile, QWEN36_RC_CANONICAL.0, QWEN36_RC_CANONICAL.1);
    let worst = crate::palw_step::worst_case_step_leaf_count_v1(&profile)?;
    let counted = crate::palw_step::step_leaf_count(&profile, &canonical)?;
    let entry = crate::palw_mode_v2::PalwClassCatalogEntryV2 {
        class_id,
        artifact_root,
        max_step_leaf_count: worst,
        canonical_step_leaf_count: counted,
        reachable_kernels: qwen36_reachable_kernels_v1(QWEN36_35B_A3B)?,
        court_cost: crate::palw_class_admission_v2::derive_court_cost_v1(&profile)
            .map_err(|_| PalwStepError::ProfileNotCanonical("the class's court cost does not derive"))?,
    };
    let object = crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target,
        share_permille,
        activation_daa: 0,
        // Genesis form: the gate that reads the admission carriage is the post-genesis acceptance
        // path; a genesis registration is authorized by `verify_palw_genesis_v2` over the whole
        // artifact, exactly as the floor's is.
        admission: None,
    };
    Ok((profile, entry, object))
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

    /// **The whole admission gate, on this profile** — the checks a post-genesis
    /// `ClassRegistered` for this class must pass: shape validation, the coverage pair (ids AND
    /// per-node shape service), the ladder depth, the court cost ceilings and the PWU recount.
    /// This is the test that says "registrable", not merely "expressible".
    #[test]
    fn the_admission_gate_admits_this_class() {
        use crate::palw_step::{step_leaf_count, worst_case_step_leaf_count_v1};
        let p = qwen36_profile_v1(QWEN36_35B_A3B).expect("the profile projects");
        p.validate_shape().expect("the shape validates");
        crate::palw_catalog_coverage::verify_profile_coverage_v1(&p).expect("every node's shape is servable");
        let worst = worst_case_step_leaf_count_v1(&p).expect("the step space enumerates");
        // The canonical CONSTANT, not a literal copy of it — the (8, 2) this line once spelled
        // out went stale the day the derivation moved the job to (7, 2), and a stale copy here
        // asserts a job the class no longer prices.
        let canonical = crate::palw_base0_profile::rc_job_context(&p, QWEN36_RC_CANONICAL.0, QWEN36_RC_CANONICAL.1);
        let counted = step_leaf_count(&p, &canonical).expect("the canonical job counts");
        assert!(counted <= worst, "canonical {counted} inside worst {worst}");
        // Stated so a registration knows what to declare: `pwu_per_inference` must equal this.
        assert!(counted > 0, "the canonical job commits at least one leaf, counted {counted}");
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
