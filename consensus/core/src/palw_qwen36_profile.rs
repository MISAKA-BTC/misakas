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
    /// 1 when the attention output rides a sigmoid gate off a fused q/gate projection (the
    /// hybrid's full-attention layers); 0 for the qwen3moe members, whose attention is plain
    /// q/k/v/o — the gate subgraph is then stripped from the projection the same way the shared
    /// expert is.
    pub attn_output_gate: u8,
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
    attn_output_gate: 1,
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

/// **`Qwen3-Coder-30B-A3B`** — and its finetunes (e.g. the Huihui abliteration), which share the
/// geometry exactly — read from its GGUF metadata 2026-08-28. The first full-attention-only
/// (qwen3moe) member: every layer is attention (`full_attention_interval` 1, the GDN dimensions
/// are zeros), the mixture has no shared expert, the attention no output gate, and the rotation
/// covers the whole 128-dim head at the same 1e7 base the hybrid uses.
pub const QWEN3_CODER_30B_A3B: PalwQwen36GeometryV1 = PalwQwen36GeometryV1 {
    layer_count: 48,
    full_attention_interval: 1,
    hidden_dim: 2048,
    attn_heads: 32,
    attn_kv_heads: 4,
    attn_head_dim: 128,
    rope_dims: 128,
    // 1e7 as f32 — the same base the hybrid pins.
    rope_freq_base_bits: 0x4B18_9680,
    gdn_k_heads: 0,
    gdn_v_heads: 0,
    gdn_head_dim: 0,
    gdn_conv_kernel: 0,
    n_experts: 128,
    experts_per_token: 8,
    moe_dim: 768,
    shared_dim: 0,
    attn_output_gate: 0,
    vocab_size: 151_936,
    // **9, from the family ladder** (`qwen3moe_geometry_probe`): without the recurrence there is
    // no per-position replay to pay for, so the binding constraint is the step LADDER, which
    // admits n_ctx 4..=10 under the RC bundle (close ≈54 KiB throughout). 9 carries the (7, 2)
    // canonical job (footprint 8) with one rung of head room, and leaves 10 for a sibling.
    n_ctx: 9,
    n_threads: 1,
    rms_eps_q: 17,
    tile_len: 512,
};

/// **`Qwen3.5-2B`** — the pinned `Qwen3.5-2B-Q4_K_M.gguf` (`.palw-gguf-sha.json`, sha
/// `aaf42c8b…`, 1,280,835,840 bytes), read 2026-08-30. The first DENSE-mixture member of the
/// hybrid family: the same GatedDeltaNet recurrence and fused q/gate attention as the 35B —
/// identical `head_dim` 256, GDN head 128, conv 4, rotary 64 at the same 1e7 base, and the
/// same 248,320-token vocabulary — with the mixture degenerated to a single always-chosen
/// expert (`n_experts` 1: the router's softmax over one logit is exactly 1.0, so the routed
/// path IS the checkpoint's dense SwiGLU FFN) and no shared expert. 24 layers at the family's
/// 1-in-4 attention interval; GDN value heads 16 (`ssm.time_step_rank`) against the 35B's 32.
pub const QWEN35_2B: PalwQwen36GeometryV1 = PalwQwen36GeometryV1 {
    layer_count: 24,
    full_attention_interval: 4,
    hidden_dim: 2048,
    attn_heads: 8,
    attn_kv_heads: 2,
    attn_head_dim: 256,
    rope_dims: 64,
    // 1e7 as f32 — the family base.
    rope_freq_base_bits: 0x4B18_9680,
    gdn_k_heads: 16,
    gdn_v_heads: 16,
    gdn_head_dim: 128,
    gdn_conv_kernel: 4,
    n_experts: 1,
    experts_per_token: 1,
    moe_dim: 6_144,
    shared_dim: 0,
    attn_output_gate: 1,
    vocab_size: 248_320,
    // 8, inherited from the hybrid's whole-close derivation: this class runs the same
    // 128-dim recurrence heads as the 35B, so the recurrence's per-position replay evidence
    // prices the context the same way and 8 is what the 80 KiB carrier admits.
    n_ctx: 8,
    n_threads: 1,
    rms_eps_q: 17,
    tile_len: 512,
};

/// **The epsilon every artifact of this lineage executes.** The fifth finding
/// (`misaka-palw-base0/src/qwen36_plan.rs`, the real-weights differential): the three geometries
/// above declare `rms_eps_q: 17` while `qwen36-convert` hardcodes `eps_q = 1` into every artifact
/// header, and the engine normalizes with the ARTIFACT's constant — so the declared epsilon was
/// not the executed one, and the planner's geometry gate refused each row over its own class's
/// weights. Measured five ways on 2026-09-01 before this constant was adopted: the converter
/// source, the local `Qwen3.5-2B` conversion, and the fleet's three `.palwq36` headers —
/// `original-from-hf` (40 layers), `huihui-30b` (48), and the 36.5 GB `qwen36.palwq36` the chain
/// registration actually loaded — all read `1`. The dense family did exactly this once already:
/// `Qwen/Qwen2.5-1.5B` declared `rms_eps_q: 1` against an artifact executing `1 << 8`, and its
/// `graph-v2` row corrected the declaration to what the converter builds (`classes.rs` calls it
/// "the defect this class was born from").
pub const QWEN36_ARTIFACT_EPS_Q: i64 = 1;

/// A lineage geometry with its epsilon corrected to [`QWEN36_ARTIFACT_EPS_Q`] — the graph-v3
/// rows' geometry. A field update on the SAME const rather than a fourth hand-kept table, so the
/// corrected geometry cannot drift from the frozen one in any other field: the v1 rows keep the
/// consts above exactly as the chain registered them (their ids are live chain facts), and this
/// is the one declared difference.
pub const fn qwen36_geometry_artifact_eps(g: PalwQwen36GeometryV1) -> PalwQwen36GeometryV1 {
    PalwQwen36GeometryV1 { rms_eps_q: QWEN36_ARTIFACT_EPS_Q, ..g }
}

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
    // The v1 spellings. Seven of these names were measured against a real artifact on 2026-09-01
    // and found to be wrong in three distinct ways (see `qwen36_profile_v2`); they stay EXACTLY as
    // they are because the genesis-registered class id is the borsh of this table, and moving it
    // orphans every registered QWEN36 claim. The named arm below is how a corrected graph says the
    // same computation with the names the engine actually reads.
    //
    // The four kernel parameters carry the fourth correction (also 2026-09-01, found by the
    // interpreter's differential): v1 declares the expert SwiGLU's wideness BACKWARDS — its gate
    // narrow and its up wide — while the engine projects the gate WIDE (silu is nonlinear, so its
    // input scale is part of the function; `Qwen36Engine::expert` says why) and the up narrow.
    // v1's spelling stays, for the same id-pinning reason as the names.
    //
    // The fifth kernel parameter carries the sixth correction (2026-09-01, found by the author's
    // review of the interpreter): the scalar gate's declared kernel. v1 says
    // `KDESC_A16_MATMUL_RESCALE` — the UNGROUPED matmul, whose court program reads no exponent
    // table — while the converter writes `ffn_shared_gate.weight.exp` like every other projection
    // and the engine's `project()` therefore runs the grouped-wide kernel. No differential can
    // see this one: the interpreter shares `project()` (agreement by construction) and the
    // fixtures carry no `.exp` (the ungrouped label is TRUE on fixtures). Only the court,
    // re-executing from the committed kernel id, would anchor the codes below their scale and
    // refute an honest producer at every hybrid layer's shared gate.
    ($first:expr) => {
        moe_tail!(
            $first,
            "blk.{layer}.ffn_router_topk.a16",
            "blk.{layer}.ffn_shared_gate.weight",
            "blk.{layer}.ffn_shared_up.weight",
            "blk.{layer}.ffn_shared_gated.a16",
            "blk.{layer}.ffn_shared_down.weight",
            "blk.{layer}.ffn_shared_scalar.weight",
            "blk.{layer}.ffn_shared_apply.a16",
            KDESC_Q36_MATMUL_GROUPED,
            KDESC_Q36_MATMUL_GROUPED_WIDE,
            KDESC_Q36_MATMUL_GROUPED,
            KDESC_Q36_MATMUL_GROUPED_WIDE,
            KDESC_A16_MATMUL_RESCALE
        )
    };
    ($first:expr, $router:literal, $sh_gate:literal, $sh_up:literal, $sh_gated:literal, $sh_down:literal, $sh_scalar:literal, $sh_apply:literal, $gate_k:expr, $up_k:expr, $shg_k:expr, $shu_k:expr, $scalar_k:expr) => {
        [
            // The stream that reaches the mixture, normalized.
            n(K::RmsNorm, KDESC_A16_RMS_NORM, "", Hidden, &[Step($first - 1)]),
            n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.ffn_norm.a16", Hidden, &[Step($first)]),
            // Route. The logits are narrowed to codes BEFORE the selection, because the tie rule
            // is defined on what the class commits to.
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.ffn_router.weight", Experts, &[Step($first + 1)]),
            n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.ffn_router.a16", Experts, &[Step($first + 2)]),
            n(K::SoftMax, KDESC_Q36_ROUTER_TOPK, $router, TopK2, &[Step($first + 3)]),
            // The eight chosen experts, as the concatenation the engine builds.
            n(K::MatMulQuant, $gate_k, "blk.{layer}.ffn_gate_exps.routed", RoutedMid, &[Step($first + 1)]),
            n(K::MatMulQuant, $up_k, "blk.{layer}.ffn_up_exps.routed", RoutedMid, &[Step($first + 1)]),
            n(K::Silu, KDESC_Q36_SILU, "", RoutedMid, &[Step($first + 5)]),
            n(K::MulElem, KDESC_Q36_MUL_WIDE, "blk.{layer}.ffn_expert_gated.a16", RoutedMid, &[Step($first + 7), Step($first + 6)]),
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.ffn_down_exps.routed", RoutedOut, &[Step($first + 8)]),
            n(K::MulElem, KDESC_Q36_MOE_COMBINE, "blk.{layer}.ffn_combine.a16", Hidden, &[Step($first + 9), Step($first + 4)]),
            // The shared expert, always on, behind its own scalar gate.
            n(K::MatMulQuant, $shg_k, $sh_gate, SharedMid, &[Step($first + 1)]),
            n(K::MatMulQuant, $shu_k, $sh_up, SharedMid, &[Step($first + 1)]),
            n(K::Silu, KDESC_Q36_SILU, "", SharedMid, &[Step($first + 11)]),
            n(K::MulElem, KDESC_Q36_MUL_WIDE, $sh_gated, SharedMid, &[Step($first + 13), Step($first + 12)]),
            n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, $sh_down, Hidden, &[Step($first + 14)]),
            n(K::MatMulQuant, $scalar_k, $sh_scalar, One, &[Step($first + 1)]),
            n(K::Sigmoid, KDESC_Q36_SIGMOID, "", One, &[Step($first + 16)]),
            n(K::MulElem, KDESC_Q36_MUL_WIDE, $sh_apply, Hidden, &[Step($first + 15), Step($first + 17)]),
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

// -------------------------------------------------------------------------------------------------
// graph-v2 — the corrected tables
// -------------------------------------------------------------------------------------------------
//
// Measured on 2026-09-01 (`misaka-palw-base0/tests/qwen36_profile_conformance.rs`): the v1 tables
// misdescribe `Qwen36Engine` in three ways, and the fix cannot be an edit because the class id is
// the borsh of the whole profile and the genesis id is pinned as a live chain fact. So the
// correction is a SECOND graph — the same computation, described with the names the engine reads —
// exactly as `Qwen/Qwen2.5-1.5B/graph-v2` shipped for the dense family.
//
// The three corrections:
// 1. The shared expert's names stop colliding. v1 declared `ffn_shared_gate.weight` 512 wide (the
//    expert's own gate projection) while the engine's tensor of that name is the SCALAR gate — the
//    engine fixed its side long ago (`Qwen36Engine::expert` names the base `ffn_shared_expert`)
//    and the IR never moved with it. v2 names the projections `ffn_shared_expert_*` and gives the
//    scalar node the scalar's real name.
// 2. The router's top-k names the store it reads. `ffn_router_topk.a16` exists in no artifact;
//    the widening the engine reads per layer — the value that decides WHICH eight experts run —
//    is `ffn_router_up.a16`.
// 3. The phantom V-cache requant is gone. v1 declared a `VCacheWrite` requant node
//    (`attn_v_cache.a16`) the engine does not perform: `full_arm` pushes the V projection into the
//    cache RAW. A declared node with no computation behind it is a step-leg slot that can never be
//    filled — every leg would be short one row. In v2 the `VCacheWrite` role sits where the write
//    actually happens: on the V projection itself.
//
// Also corrected while the row was open, both pure renames to real stores: the rope nodes name
// `attn_rope.a16` (the rotation's requant — the rope TABLE is a structured artifact field bound by
// the artifact root, not a named tensor), and the softmax names `attn_softmax_up.a16`.
//
// 4. (Found by the interpreter's differential, 2026-09-01, same day.) The expert SwiGLU's
//    wideness was declared BACKWARDS: v1 labels the gate projection `MATMUL_GROUPED` (narrow to
//    codes) and the up projection `MATMUL_GROUPED_WIDE`, while the engine projects the gate WIDE —
//    silu is nonlinear, so its input scale is part of the function, and a narrowed gate clamps at
//    the code rail exactly on the rows where the model is loudest — and the up narrow. Both the
//    routed and the shared expert carry the swap. The name-conformance measurement cannot see
//    this (the names are right); the fixture differential cannot either when every row sits
//    inside the code rail (narrow and wide agree there by construction). What convicts it is the
//    hot-row differential in `misaka-palw-base0/src/qwen36_plan.rs`, which drives a gate row past
//    the rail and holds the interpreter to the engine's bits.
//
// 5. (Found by the same differential run on real weights, 2026-09-01.) The declared epsilon was
//    not the executed one: every geometry pinned `rms_eps_q: 17` while the converter hardcodes
//    `eps_q = 1` into every artifact header and the engine normalizes with the artifact's. Not a
//    node-table defect — the correction lives in the ledger geometries, which take
//    [`qwen36_geometry_artifact_eps`] (measured five ways; see [`QWEN36_ARTIFACT_EPS_Q`]).
//
// 6. (Found by the graph author's review of the interpreter, 2026-09-01.) The scalar gate's
//    kernel id hid the group exponents. v1 declares the mixture's scalar gate
//    (`ffn_shared_scalar.weight` there, the engine's `ffn_shared_gate.weight`) as
//    `KDESC_A16_MATMUL_RESCALE` — the ungrouped matmul, whose court program
//    (`palw_step_refute.rs`, `Qwen36Op::MatMulRescale`) reads no exponent table — while the
//    converter writes `.exp` for that tensor like any other and `Qwen36Engine::project` therefore
//    runs the grouped-wide kernel over it. Engine and interpreter agree by construction (they
//    share `project()`), fixtures make the label true by omission (no `.exp`), and the one real
//    artifact differentially tested (`Qwen3.5-2B`) has no shared expert at all — so every
//    measurement was structurally blind, and only a court re-executing from the committed kernel
//    id would diverge, refuting an honest producer at every hybrid layer's shared gate. The
//    corrected arm declares `KDESC_Q36_MATMUL_GROUPED_WIDE`, which is what the engine performs
//    over the registered artifact.

/// The GDN arm, v2: the arm's own 24 nodes are IDENTICAL to v1's — copied, not transcribed, so
/// they cannot drift — and only the mixture's seven wrong names and five wrong kernel labels
/// change (the expert SwiGLU's gate is WIDE and its up narrow, and the scalar gate is GROUPED
/// wide, as the engine performs them).
const QWEN36_LINEAR_IR_V2: &[Ir] = &{
    const TAIL: [Ir; 24] = moe_tail!(
        24u16,
        "blk.{layer}.ffn_router_up.a16",
        "blk.{layer}.ffn_shared_expert_gate.weight",
        "blk.{layer}.ffn_shared_expert_up.weight",
        "blk.{layer}.ffn_shared_expert_gated.a16",
        "blk.{layer}.ffn_shared_expert_down.weight",
        "blk.{layer}.ffn_shared_gate.weight",
        "blk.{layer}.ffn_shared_gated.a16",
        KDESC_Q36_MATMUL_GROUPED_WIDE,
        KDESC_Q36_MATMUL_GROUPED,
        KDESC_Q36_MATMUL_GROUPED_WIDE,
        KDESC_Q36_MATMUL_GROUPED,
        KDESC_Q36_MATMUL_GROUPED_WIDE
    );
    let mut all = [QWEN36_LINEAR_IR[0]; 48];
    let mut i = 0;
    while i < 24 {
        all[i] = QWEN36_LINEAR_IR[i];
        i += 1;
    }
    let mut j = 0;
    while j < 24 {
        all[24 + j] = TAIL[j];
        j += 1;
    }
    all
};

/// The gated-attention arm, v2: 46 nodes, one fewer than v1 — the phantom is deleted, every later
/// step reference shifts down by one, and `structural_diff_v1_v2` in the tests holds this table to
/// v1 node by node so the renumbering is checked rather than trusted.
const QWEN36_ATTN_IR_V2: &[Ir] = &{
    const HEAD: [Ir; 20] = [
        n(K::RmsNorm, KDESC_A16_RMS_NORM, "", Hidden, &[LayerIn]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_norm.a16", Hidden, &[Step(0)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.attn_q.weight", QDim, &[Step(1)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED_WIDE, "blk.{layer}.attn_gate.weight", QDim, &[Step(1)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.attn_k.weight", KvDim, &[Step(1)]),
        // **The V cache write is the projection.** The engine pushes this row into the cache as
        // produced — no requant stands between them — so the role sits here, on the computation
        // that actually happens, instead of on a node nothing executes.
        c(K::MatMulQuant, PalwStepNodeRoleV1::VCacheWrite, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.attn_v.weight", KvDim, &[Step(1)]),
        n(K::RmsNorm, KDESC_Q36_HEAD_RMS_NORM, "", QDim, &[Step(2)]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_q_norm.a16", QDim, &[Step(6)]),
        n(K::RmsNorm, KDESC_Q36_HEAD_RMS_NORM, "", KvDim, &[Step(4)]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_k_norm.a16", KvDim, &[Step(8)]),
        n(K::RopeImrope, KDESC_Q36_ROPE_PARTIAL, "blk.{layer}.attn_rope.a16", QDim, &[Step(7)]),
        c(K::RopeImrope, PalwStepNodeRoleV1::KCacheWrite, KDESC_Q36_ROPE_PARTIAL, "blk.{layer}.attn_rope.a16", KvDim, &[Step(9)]),
        n(K::MatMulQuant, KDESC_A16_ATTN_SCORES, "blk.{layer}.attn_logits.a16", KvPerHead, &[Step(10), CachedK]),
        n(K::SoftMax, KDESC_A16_SOFTMAX, "blk.{layer}.attn_softmax_up.a16", KvPerHead, &[Step(12)]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_probs.a16", KvPerHead, &[Step(13)]),
        n(K::MatMulQuant, KDESC_A16_ATTN_VALUES, "blk.{layer}.attn_values.a16", QDim, &[Step(14), CachedV]),
        n(K::Sigmoid, KDESC_Q36_SIGMOID, "", QDim, &[Step(3)]),
        n(K::MulElem, KDESC_Q36_GATE_APPLY, "blk.{layer}.attn_gated.a16", QDim, &[Step(15), Step(16)]),
        n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.attn_o.weight", Hidden, &[Step(17)]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_align.a16", Hidden, &[LayerIn]),
    ];
    const MID: [Ir; 2] = [
        n(K::AddElem, KDESC_A16_ADD_ELEM, "", Hidden, &[Step(19), Step(18)]),
        n(K::MulElem, KDESC_A16_REQUANTIZE, "blk.{layer}.attn_residual.a16", Hidden, &[Step(20)]),
    ];
    const TAIL: [Ir; 24] = moe_tail!(
        22u16,
        "blk.{layer}.ffn_router_up.a16",
        "blk.{layer}.ffn_shared_expert_gate.weight",
        "blk.{layer}.ffn_shared_expert_up.weight",
        "blk.{layer}.ffn_shared_expert_gated.a16",
        "blk.{layer}.ffn_shared_expert_down.weight",
        "blk.{layer}.ffn_shared_gate.weight",
        "blk.{layer}.ffn_shared_gated.a16",
        KDESC_Q36_MATMUL_GROUPED_WIDE,
        KDESC_Q36_MATMUL_GROUPED,
        KDESC_Q36_MATMUL_GROUPED_WIDE,
        KDESC_Q36_MATMUL_GROUPED,
        KDESC_Q36_MATMUL_GROUPED_WIDE
    );
    let mut all = [HEAD[0]; 46];
    let mut i = 0;
    while i < 20 {
        all[i] = HEAD[i];
        i += 1;
    }
    all[20] = MID[0];
    all[21] = MID[1];
    let mut j = 0;
    while j < 24 {
        all[22 + j] = TAIL[j];
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

/// **Strip the subgraphs a member's geometry does not have.**
///
/// qwen3moe (Qwen3-Coder and kin) is the hybrid's layer minus two pieces: the always-on shared
/// expert (`shared_dim == 0` — no scalar gate over it, no add folding it back in) and the
/// attention output gate (`attn_output_gate == 0` — plain q/k/v/o, no fused gate projection, no
/// sigmoid, no gated multiply). The IR tables are consts written for the hybrid's layer, so the
/// projection derives the smaller graph instead of a second hand-kept table (a table beside a
/// table is how graphs drift):
///
/// 1. seed: every node whose weight lives under an absent subgraph's tensor prefix;
/// 2. closure: a node ALL of whose step inputs are already being dropped (the anonymous
///    activations between the seeded matmuls) joins them;
/// 3. references into the dropped set are removed from surviving nodes' input lists, and a
///    two-input fold left with one input this way — the mixture+shared AddElem, the gated-values
///    MulElem — is an identity: it is dropped too, and references to IT forward to its surviving
///    input;
/// 4. everything is re-indexed.
///
/// For a hybrid geometry every seed is empty and the function is the identity, so the shipped
/// class's profile — and therefore its id — cannot move.
/// One IR node with its input list OWNED — what the stripper produces and what `project` reads.
///
/// The const tables are `&'static [I]` because they are consts; a STRIPPED table's input lists are
/// computed, and the first version of this leaked them (`Box::leak`) so they would fit the same
/// type. Class ids are re-derived on every backend lookup, so that leaked ~600 bytes per resolve —
/// per block attempt and repeatedly inside the panel's sweep (audit M2-26). Owning them here costs
/// one allocation that is freed when the projection finishes.
#[derive(Clone)]
struct ProjIr {
    op: PalwStepOpKindV1,
    role: PalwStepNodeRoleV1,
    kernel: &'static str,
    weight: &'static str,
    out: W,
    inputs: Vec<I>,
}

impl From<&Ir> for ProjIr {
    fn from(n: &Ir) -> Self {
        ProjIr { op: n.op, role: n.role, kernel: n.kernel, weight: n.weight, out: n.out, inputs: n.inputs.to_vec() }
    }
}

fn strip_absent_subgraphs(ir: &[Ir], seeds: &[&str]) -> Vec<ProjIr> {
    let mut dropped = vec![false; ir.len()];
    for (i, node) in ir.iter().enumerate() {
        if seeds.iter().any(|prefix| node.weight.contains(prefix)) {
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
    // The fold: a two-input combine whose OTHER input was the stripped path — the mixture+shared
    // AddElem, the attention gate's MulElem. Forward it to its surviving side.
    let mut forward: Vec<Option<u16>> = vec![None; ir.len()];
    for (i, node) in ir.iter().enumerate() {
        if dropped[i] || !matches!(node.op, PalwStepOpKindV1::AddElem | PalwStepOpKindV1::MulElem) {
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
        out.push(ProjIr { inputs, ..ProjIr::from(node) });
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
    let mut seeds: Vec<&str> = Vec::new();
    if g.shared_dim == 0 {
        seeds.push("ffn_shared");
    }
    if g.attn_output_gate == 0 {
        // **Seed ONLY the gate's own projection.** The sigmoid falls to the closure and the gated
        // multiply falls to the FOLD, which forwards it to the attention values — that is the
        // whole reason rule 3 exists.
        //
        // Seeding `attn_gated.a16` as well (as this did until 2026-08-29) looked equivalent and
        // was not: the fold loop skips a node the seed pass already dropped, so the multiply got
        // no forwarding entry, and the closure then cascaded off it — `attn_o.weight`, whose only
        // input is that multiply, was deleted from every layer, and the residual `AddElem` left
        // with one input folded away to the layer input. The declared graph computed attention and
        // threw it away while the engine (`qwen36.rs`, `project("attn_o.weight")`, unconditional)
        // projected it, so the class the chain stored was not the class the node ran: pwu, court
        // cost and the ladder were all priced on a graph missing a [hidden x q_dim] matmul in each
        // of 48 layers, and fraud in that matmul was structurally unrefutable. Audit M2-9.
        seeds.push("attn_gate.weight");
    }
    // One owned table either way: the identity conversion for a hybrid geometry (no seeds) and the
    // stripped one otherwise. Same nodes in the same order — the shipped class's id cannot move.
    let table: Vec<ProjIr> = if seeds.is_empty() { ir.iter().map(ProjIr::from).collect() } else { strip_absent_subgraphs(ir, &seeds) };
    let ir = &table[..];
    // A matmul's reduction width, from the IR's own wiring — the first input's row.
    let in_width = |node: &ProjIr| -> usize {
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
    /// **The stripped graph is the hybrid's minus exactly the absent subgraphs — nothing else.**
    ///
    /// The regression this pins (audit M2-9): seeding the stripper with the gated multiply as well
    /// as the gate projection cascaded into `attn_o.weight`, so the qwen3moe graph computed
    /// attention and discarded it while the engine projected it — the chain stored a class the
    /// node did not run. Named tensors are compared as SETS so the test states the rule ("these
    /// four names disappear, everything else survives") rather than an index that any IR edit
    /// would have to re-baseline.
    #[test]
    fn stripping_removes_the_absent_subgraphs_and_nothing_else() {
        let names = |g: PalwQwen36GeometryV1| -> std::collections::BTreeSet<String> {
            let p = qwen36_profile_v1(g).expect("projects");
            [&p.pre_nodes, &p.gdn_nodes, &p.attn_nodes, &p.post_nodes]
                .into_iter()
                .flatten()
                .map(|n| n.weight_name.clone())
                .filter(|w| !w.is_empty())
                .collect()
        };
        let hybrid = names(QWEN36_35B_A3B);
        let moe = names(QWEN3_CODER_30B_A3B);

        // The output projection and the attention residual are NOT part of the gate.
        for kept in ["blk.{layer}.attn_o.weight", "blk.{layer}.attn_q.weight", "blk.{layer}.attn_residual.a16"] {
            assert!(moe.contains(kept), "the qwen3moe graph lost {kept} — it is not part of an absent subgraph");
        }
        // The residual ADD survives too (a name-less node, so checked by op).
        let moe_profile = qwen36_profile_v1(QWEN3_CODER_30B_A3B).expect("projects");
        assert_eq!(
            moe_profile.attn_nodes.iter().filter(|n| n.op_kind == crate::palw_step::PalwStepOpKindV1::AddElem).count(),
            2,
            "the attention residual add and the mixture add must both survive"
        );

        // And what DID disappear belongs to an absent subgraph — stated as the RULE, because the
        // three absences have different causes: the recurrence is absent because the member has no
        // GDN layers at all (an empty node table, not a strip), while the shared expert and the
        // attention gate are stripped out of a table that is otherwise shared with the hybrid.
        for gone in hybrid.difference(&moe) {
            let recurrence = gone.contains(".linear_");
            let shared_expert = gone.contains("ffn_shared");
            let attention_gate = gone.ends_with("attn_gate.weight") || gone.ends_with("attn_gated.a16");
            assert!(
                recurrence || shared_expert || attention_gate,
                "{gone} is not part of an absent subgraph, and the stripper deleted it anyway"
            );
        }
        assert!(moe.difference(&hybrid).next().is_none(), "stripping must never ADD a tensor");

        // The hybrid's own id is a live chain fact and must be untouched by any stripper change.
        assert_eq!(
            qwen36_profile_v1(QWEN36_35B_A3B).expect("projects").shape_profile_id().to_string(),
            "ec7bbcbffe13f36f1c2c418c65bdab840dd40b2bc22b217522dae836153078ddb77a92fb0645d34f98e9e3a1302e4543448a3924b3cd152fc74774ad3f02fb3f",
            "the shipped hybrid class id moved — every registered QWEN36 claim is now unreachable"
        );
    }

    /// **v2 differs from v1 in exactly the measured corrections and nothing else.**
    ///
    /// The renumbering after the phantom's deletion is derived by hand, and a hand-derived shift is
    /// exactly the kind of edit that slips one index and produces a graph that computes something
    /// adjacent to the truth. So the diff is CHECKED: every v2 attention node must equal its v1
    /// counterpart — the deleted node skipped, the three renames, the role move and the five
    /// kernel corrections excused (each in its exact direction), and every step reference
    /// shifted by exactly the deletion.
    #[test]
    fn structural_diff_v1_v2() {
        // The fourth and sixth corrections: the expert SwiGLU's wideness, which v1 declares
        // backwards, and the scalar gate's kernel, which v1 declares ungrouped. The excuse is
        // exact — gate narrow→wide, up wide→narrow, scalar ungrouped→grouped-wide, nothing else —
        // so the diff cannot quietly admit any other kernel change.
        let expect_kernel = |v1: &Ir, v2_weight: &str| -> &'static str {
            let widened = v2_weight.ends_with("ffn_gate_exps.routed") || v2_weight.ends_with("ffn_shared_expert_gate.weight");
            let narrowed = v2_weight.ends_with("ffn_up_exps.routed") || v2_weight.ends_with("ffn_shared_expert_up.weight");
            let regrouped = v2_weight.ends_with("ffn_shared_gate.weight");
            if widened {
                assert_eq!(v1.kernel, KDESC_Q36_MATMUL_GROUPED, "{v2_weight}: v1 declared the gate narrow");
                KDESC_Q36_MATMUL_GROUPED_WIDE
            } else if narrowed {
                assert_eq!(v1.kernel, KDESC_Q36_MATMUL_GROUPED_WIDE, "{v2_weight}: v1 declared the up wide");
                KDESC_Q36_MATMUL_GROUPED
            } else if regrouped {
                assert_eq!(v1.kernel, KDESC_A16_MATMUL_RESCALE, "{v2_weight}: v1 declared the scalar gate ungrouped");
                KDESC_Q36_MATMUL_GROUPED_WIDE
            } else {
                v1.kernel
            }
        };
        // The GDN arm: identical except the mixture's seven renamed operands.
        let renames = [
            ("ffn_router_topk.a16", "ffn_router_up.a16"),
            ("ffn_shared_gate.weight", "ffn_shared_expert_gate.weight"),
            ("ffn_shared_up.weight", "ffn_shared_expert_up.weight"),
            ("ffn_shared_gated.a16", "ffn_shared_expert_gated.a16"),
            ("ffn_shared_down.weight", "ffn_shared_expert_down.weight"),
            ("ffn_shared_scalar.weight", "ffn_shared_gate.weight"),
            ("ffn_shared_apply.a16", "ffn_shared_gated.a16"),
            ("rope_table", "attn_rope.a16"),
            ("attn_softmax.a16", "attn_softmax_up.a16"),
        ];
        // Position-sensitive: v1's `ffn_shared_gate.weight` becomes the EXPERT name while v2 reuses
        // that spelling for the scalar node, so a name map alone would be ambiguous. Compare by
        // index instead, renaming v1's name and asking v2 to match.
        let rename = |name: &str| -> String {
            for (from, to) in renames {
                if let Some(prefix) = name.strip_suffix(from) {
                    return format!("{prefix}{to}");
                }
            }
            name.to_string()
        };
        assert_eq!(QWEN36_LINEAR_IR.len(), QWEN36_LINEAR_IR_V2.len());
        for (i, (v1, v2)) in QWEN36_LINEAR_IR.iter().zip(QWEN36_LINEAR_IR_V2.iter()).enumerate() {
            assert_eq!(v1.op as u8, v2.op as u8, "gdn node {i}");
            assert_eq!(expect_kernel(v1, v2.weight), v2.kernel, "gdn node {i}");
            assert_eq!(rename(v1.weight), v2.weight, "gdn node {i}");
            assert_eq!(v1.inputs, v2.inputs, "gdn node {i}: the GDN arm renumbers nothing");
        }

        // The attention arm: node 12 (the phantom) is deleted; everything after shifts by one.
        const PHANTOM: usize = 12;
        assert_eq!(QWEN36_ATTN_IR.len(), QWEN36_ATTN_IR_V2.len() + 1);
        assert_eq!(QWEN36_ATTN_IR[PHANTOM].weight, "blk.{layer}.attn_v_cache.a16");
        for (i2, v2) in QWEN36_ATTN_IR_V2.iter().enumerate() {
            let i1 = if i2 < PHANTOM { i2 } else { i2 + 1 };
            let v1 = &QWEN36_ATTN_IR[i1];
            assert_eq!(v1.op as u8, v2.op as u8, "attn node v1[{i1}] vs v2[{i2}]");
            assert_eq!(expect_kernel(v1, v2.weight), v2.kernel, "attn node v1[{i1}] vs v2[{i2}]");
            assert_eq!(rename(v1.weight), v2.weight, "attn node v1[{i1}] vs v2[{i2}]");
            // The role move: V's projection carries VCacheWrite in v2 and Plain in v1.
            if v1.weight == "blk.{layer}.attn_v.weight" {
                assert_eq!(v1.role as u8, PalwStepNodeRoleV1::Plain as u8);
                assert_eq!(v2.role as u8, PalwStepNodeRoleV1::VCacheWrite as u8);
            } else {
                assert_eq!(v1.role as u8, v2.role as u8, "attn node v1[{i1}] vs v2[{i2}]");
            }
            // Every step reference at or past the phantom shifts down by exactly one.
            assert_eq!(v1.inputs.len(), v2.inputs.len());
            for (a, b) in v1.inputs.iter().zip(v2.inputs.iter()) {
                match (a, b) {
                    (I::Step(x), I::Step(y)) => {
                        let want = if *x > PHANTOM as u16 { x - 1 } else { *x };
                        assert_eq!(want, *y, "attn node v1[{i1}] vs v2[{i2}]: ref {x} should shift to {want}");
                    }
                    _ => assert_eq!(
                        std::mem::discriminant(a),
                        std::mem::discriminant(b),
                        "attn node v1[{i1}] vs v2[{i2}]: non-step ref changed"
                    ),
                }
            }
        }
    }

    /// v2 projects, validates, and is a DIFFERENT class than the genesis-registered one.
    #[test]
    fn v2_projects_and_moves_the_id() {
        let p1 = qwen36_profile_v1(QWEN36_35B_A3B).expect("v1 projects");
        let p2 = qwen36_profile_v2(QWEN36_35B_A3B).expect("v2 projects");
        assert_eq!(p2.attn_nodes.len(), p1.attn_nodes.len() - 1, "one node fewer: the phantom");
        assert_ne!(p1.shape_profile_id(), p2.shape_profile_id(), "a corrected graph is a new class");
        // The qwen3moe stripper still recognizes the renamed shared-expert subgraph.
        let m2 = qwen36_profile_v2(QWEN3_CODER_30B_A3B).expect("the stripped v2 projects");
        assert!(m2.validate_shape().is_ok());
    }

    /// The registrable corrected id — the `graph-v3` row's, pinned the day the row was authored:
    /// the corrected tables OVER the artifact-epsilon geometry, because that pair is what
    /// `classes.rs` registers. Anything that moves it is a NEW class again. Two prior pins died
    /// in review before ever being registered from a shipping build (`069b9482…` with the
    /// backwards expert wideness — though THAT spelling reached testnet-11 from a stale binary,
    /// which is why the ledger says v3 — and `23ef487f…` with the ungrouped scalar gate and the
    /// unexecuted epsilon).
    #[test]
    fn v3_shape_profile_id_golden_vector() {
        assert_eq!(
            qwen36_profile_v2(qwen36_geometry_artifact_eps(QWEN36_35B_A3B)).expect("projects").shape_profile_id().to_string(),
            "5bd9ae3d91df80650caffe3126a38bafb0b4feb9b046a416d353a7c3f71af6eab5aadf9b1ce41650007a980f1cc6044ef218424f4cbb8299ef9e92c97b99ef8e"
        );
    }

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
                attn_output_gate: 0,
                vocab_size: 151_936,
                n_ctx: nctx,
                n_threads: 1,
                rms_eps_q: 17,
                tile_len: 512,
            };
            let profile = match qwen36_profile_v1(g) {
                Ok(pr) => pr,
                Err(e) => {
                    eprintln!("nctx {nctx}: profile err {e:?}");
                    continue;
                }
            };
            let canonical = crate::palw_base0_profile::rc_job_context(&profile, (nctx - 1).min(7), 2);
            let reg = match crate::palw_class_admission_v2::palw_post_genesis_registration_v1(
                profile.clone(),
                canonical.clone(),
                kaspa_hashes::Hash64::default(),
                1,
                1,
                5,
                0,
                crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(kaspa_hashes::Hash64::default(), 0)),
                vec![],
            ) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("nctx {nctx}: builder err {e}");
                    continue;
                }
            };
            // ADR-0069 Decision 5: the weight gate needs a certified set that hashes to the
            // bundle's own `court_e2e_root`. These tests are about the class's SHAPE, so the gate
            // is satisfied rather than exercised — see `catalog_covering_family_for_tests_v1`.
            let certified = crate::palw_e2e_adjudicability::catalog_covering_family_for_tests_v1();
            let mut b = b.clone();
            b.court_e2e_root = crate::palw_e2e_adjudicability::palw_court_e2e_root_of_v1(&certified);
            let verdict = crate::palw_class_admission_v2::verify_class_admission_v2(&b, &profile, &canonical, &reg, &certified);
            match nctx {
                4 | 6 | 8 | 9 | 10 => assert!(verdict.is_ok(), "n_ctx {nctx} fell out of the qwen3moe family's room: {verdict:?}"),
                _ => assert!(verdict.is_err(), "n_ctx {nctx} was admitted — the qwen3moe ceiling moved, revisit the ladder comment"),
            }
        }
    }
}
pub fn qwen36_profile_v1(g: PalwQwen36GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    qwen36_profile_with(g, QWEN36_PRE_IR, QWEN36_LINEAR_IR, QWEN36_ATTN_IR, QWEN36_POST_IR)
}

/// **graph-v2: the same computation, described with the names the engine reads.**
///
/// A different class id by construction — the id is the borsh of the node tables and three of
/// v1's names were measured wrong (see the table comments above). v1 stays registrable and pinned;
/// this is the row an interpreter can actually follow, and the mmap interpreter (ADR-0067's
/// fourth clause) builds against THIS, never against v1.
pub fn qwen36_profile_v2(g: PalwQwen36GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    qwen36_profile_with(g, QWEN36_PRE_IR, QWEN36_LINEAR_IR_V2, QWEN36_ATTN_IR_V2, QWEN36_POST_IR)
}

fn qwen36_profile_with(
    g: PalwQwen36GeometryV1,
    pre: &[Ir],
    gdn: &[Ir],
    attn: &[Ir],
    post: &[Ir],
) -> Result<PalwShapeProfileV3, PalwStepError> {
    let (gdn_span, attn_span) = layer_spans(&g);
    // **The gate is STORED here and DERIVED in the engine, so they must be checked against each
    // other exactly once — here, before anything projects.**
    //
    // `Qwen36ShapeV1::attn_output_gate()` (the artifact side) answers `!is_full_attention_only()`,
    // because the artifact format has no field for it; this geometry answers with a field. The two
    // shipped members agree, and no remote input can make them disagree — but a future geometry
    // could, and then the producer would run a gated arm while the court re-derived a gateless
    // graph (or the reverse), which convicts an honest execution. Refusing the geometry is the
    // only place the two spellings can be reconciled without changing the artifact format.
    if (gdn_span == 0) != (g.attn_output_gate == 0) {
        return Err(PalwStepError::ProfileNotCanonical(
            "attn_output_gate disagrees with the layer stack: the engine derives the gate from \
             all-attention-ness, so a hybrid must gate and a full-attention-only member must not",
        ));
    }
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
        pre_nodes: project(pre, &g, 1),
        gdn_nodes: project(gdn, &g, gdn_span),
        attn_nodes: project(attn, &g, attn_span),
        post_nodes: project(post, &g, 1),
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

/// The class id the v1 declaration derives — the graph this build cannot serve. Kept because it
/// names a real historical class and [`qwen36_class_id_v2`]'s test asserts the two differ.
pub fn qwen36_class_id_v1() -> Hash64 {
    qwen36_profile_v1(QWEN36_35B_A3B).expect("the pinned geometry projects").shape_profile_id()
}

/// **The class id a chain should register for this tier** — the `graph-v3` row, which is the one
/// this build's SDK actually serves.
///
/// Not `qwen36_profile_v2(QWEN36_35B_A3B)`, which is a third id nothing dispatches on. The
/// lineage table's corrected rows pair the v2 PROJECTION with the eps-corrected GEOMETRY
/// (`qwen36_geometry_artifact_eps`), and the name is v3 rather than v2 because "graph-v2" is
/// burned: the superseded spelling reached testnet-11 first and a registered name cannot be
/// re-pointed at a different id. Derived through the same pair the table uses, so a node's
/// dispatch and a chain's registration cannot describe different classes.
pub fn qwen36_class_id_v3() -> Hash64 {
    qwen36_profile_v2(qwen36_geometry_artifact_eps(QWEN36_35B_A3B)).expect("the pinned geometry projects").shape_profile_id()
}

/// **The same registration, from the `graph-v3` declaration this build can serve** (ADR-0069).
///
/// `qwen36_registration_v1` is not merely older: the graph it declares is one
/// `Qwen36Backend::from_registered_profile` REFUSES — measured 2026-09-01, "gdn node 28 cannot be
/// served: op SoftMax with operand `blk.{layer}.ffn_router_topk.a16` is not arithmetic this build
/// serves". A class registered on it therefore has no backend, no capture and no court:
/// `supports_court` is false because the backend holds no plan, and every dispute over one of its
/// claims dies at round 0 whichever party is honest.
///
/// That was survivable while the tier held no weight, and it is not now that it does. ADR-0069
/// grants a nonzero share only to a class some end-to-end certified family covers, and no family
/// can be certified for a graph nobody can plan — so a chain that wants this tier to earn must
/// register THIS declaration.
///
/// A separate builder rather than a repair of v1, following the A16 tier's precedent and the
/// principle its test states: **a correction is a different class, never a repair in place.** The
/// id moves off `ec7bbcbf…`, which is the honest cost of the graph having changed.
pub fn qwen36_registration_v3(
    artifact_root: Hash64,
    share_permille: u16,
    slash_value_per_pwu: u64,
    initial_target: u128,
) -> Result<
    (PalwShapeProfileV3, crate::palw_mode_v2::PalwClassCatalogEntryV2, crate::palw_state_v2::PalwConsensusObjectV2),
    PalwStepError,
> {
    // The lineage table's own pairing: the v2 projection over the eps-corrected geometry. Written
    // as that pair rather than as a fourth spelling, because a class id derived two ways is two
    // classes waiting to disagree.
    let profile = qwen36_profile_v2(qwen36_geometry_artifact_eps(QWEN36_35B_A3B))?;
    let class_id = profile.shape_profile_id();
    let canonical = crate::palw_base0_profile::rc_job_context(&profile, QWEN36_RC_CANONICAL.0, QWEN36_RC_CANONICAL.1);
    let counted = crate::palw_step::step_leaf_count(&profile, &canonical)?;
    let entry = crate::palw_mode_v2::PalwClassCatalogEntryV2 {
        class_id,
        artifact_root,
        max_step_leaf_count: crate::palw_step::worst_case_step_leaf_count_v1(&profile)?,
        canonical_step_leaf_count: counted,
        // **Off the corrected profile's own nodes**, not the v1 helper: the two declarations differ
        // in the graph, so a reachable set read from the wrong one would describe a class nobody
        // registered — and it is exactly the set the admission gate compares against a certificate.
        reachable_kernels: crate::palw_class_admission_v2::reachable_kernels_v1(&profile),
        court_cost: crate::palw_class_admission_v2::derive_court_cost_v1(&profile)
            .map_err(|_| PalwStepError::ProfileNotCanonical("the corrected hybrid class's court cost does not derive"))?,
    };
    let object = crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target,
        share_permille,
        activation_daa: 0,
        admission: None,
    };
    Ok((profile, entry, object))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The corrected hybrid class is the one a chain can prosecute, and it is a DIFFERENT class**
    /// (ADR-0069).
    ///
    /// Two claims in one test because they are one fact: v1 declares a graph this build refuses to
    /// plan, so a class registered on it can never be certified and can never hold weight; v2 is
    /// the declaration that can. Asserted as a difference — same geometry, same weights, different
    /// id — so a future "repair" of v1 in place fails here rather than silently re-pointing a
    /// registered class at another graph.
    #[test]
    fn the_corrected_hybrid_class_is_a_different_class_and_the_servable_one() {
        let (profile, entry, _) = qwen36_registration_v3(Hash64::from_u64_word(0x36A7), 1, 1, 1).expect("derives");
        assert_eq!(entry.class_id, qwen36_class_id_v3());
        assert_ne!(entry.class_id, qwen36_class_id_v1(), "a correction is a different class, never a repair in place");
        assert_eq!(entry.reachable_kernels, crate::palw_class_admission_v2::reachable_kernels_v1(&profile));
        // Statically adjudicable, which the weight gate requires before it even asks about a
        // certificate.
        crate::palw_catalog_coverage::verify_profile_coverage_v1(&profile).expect("every node's shape is servable");
    }

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
