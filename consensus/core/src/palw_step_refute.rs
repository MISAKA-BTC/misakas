//! PALW `ExecutionStepRefutationV1` — ADR-0027 §1's arithmetic conviction, adjudicable.
//!
//! The claim is 10¹⁵ ops; the adjudication is one tile: the challenger opens the committed
//! output tile of one step plus the committed inputs the profile's wiring names, every node
//! recomputes that ONE step with the frozen kernel program (`kernel_semantics_id` → code in
//! the catalog below), and compares exact bytes. Recomputed ≠ committed ⇒ the miner is
//! convicted (`ComputationMismatch`); equal ⇒ `NoFaultFound`, the challenger's bond.
//!
//! # The three-way outcome, and why the third exists
//!
//! * **Convicted** — the opened material breaks a pinned rule.
//! * **`NoFaultFound`** — the material is honest under every pinned rule.
//! * **`Unadjudicable`** — the step's kernel program is not in the catalog (or needs a
//!   registration-opaque decoder, e.g. checkpoint state bytes before the chunk map is a
//!   network fact). NOBODY is slashed: an unadjudicable claim is rejected, never guessed.
//!   The catalog only grows by transcription + the ADR-0030 §5.1 differential gate, so
//!   "adjudicable" expands monotonically and visibly.
//!
//! # Input integrity
//!
//! The refutation's input openings are checked against the CANONICAL input set derived from
//! the profile's `input_refs` wiring — exact leaf indices, exact order. A challenger cannot
//! open unrelated (honest) tiles as "the inputs" of a step and manufacture a mismatch; a
//! wrong input set is a malformed refutation, not evidence.
//!
//! Starter catalog (per-class ids; ADR-0030 premise "order is code named by id"):
//! l2-norm, rms-norm (fused ×weight), swiglu, sigmoid, softplus, and the **gated-delta-net
//! recurrence** (18 of 24 layers) with both `vec_dot_f32` lane structures (NEON 16×4, AVX2
//! 32×8) — genesis-anchored replay now; checkpoint-anchored replay turns on when the state
//! chunk map registers. Quantized-matmul and RoPE programs are named, pending transcription.

use kaspa_hashes::Hash64;

use crate::palw_reference::{
    ref_add_v1, ref_div_v2, ref_fma_v2, ref_mul_v1, ref_narrow_f64_to_f32_v2, ref_sqrt_v2, ref_sub_v1, ref_widen_f32_to_f64_v2,
    ref64_add_v2, ref64_div_v2, ref64_mul_v2,
};
use crate::palw_step::{
    PALW_STEP_INPUT_LAYER_IN, PALW_STEP_INPUT_SENTINEL_MIN, PalwShapeProfileV3, PalwStepCoordinateV1, canonical_step_leaf_index,
    kernel_semantics_id_v1,
};
use crate::palw_step_leg::{
    PalwStepBindingV2, PalwStepFaultV1, PalwStepOpeningV1, PalwStepRefutationVerdictV1, PalwStepTileLeafV1, step_opening_root_v1,
    step_tile_leaf_hash_v1,
};
use crate::palw_transcendental::{ggml_v_silu_v1, glibc_expf_v1};

// ---------------------------------------------------------------------------------------------
// Catalog descriptors
// ---------------------------------------------------------------------------------------------

pub const KDESC_L2_NORM: &str = "l2-norm/whole-row/double-sum-ascending/llama-030ebb558/v1";
pub const KDESC_RMS_NORM_FUSED: &str = "rms-norm/double-sum-ascending/fused-mul/llama-030ebb558/v1";
pub const KDESC_SWIGLU: &str = "glu/swiglu/v-silu-per-lane/llama-030ebb558/v1";
pub const KDESC_SIGMOID_GLIBC_FMA: &str = "sigmoid/scalar/glibc-2.39-expf-fma/llama-030ebb558/v1";
pub const KDESC_SOFTPLUS_GLIBC_FMA: &str = "softplus/scalar/glibc-2.39-expf-logf-fma/llama-030ebb558/v1";
pub const KDESC_GDN_CORE_NEON: &str = "gated-delta-net/fused-seq/f32dot-step16-epr4/glibc-2.39-expf-fma/llama-030ebb558/v1";
pub const KDESC_GDN_CORE_AVX2: &str = "gated-delta-net/fused-seq/f32dot-step32-epr8/glibc-2.39-expf-fma/llama-030ebb558/v1";

// --- PALW-BASE-0 (ADR-0040): the integer class, whose catalog can actually close ------------
//
// Every descriptor names `base0/v1` and no toolchain, because there is nothing toolchain-shaped
// left to name: no libm version, no FMA contraction flag, no SIMD lane structure. The float
// descriptors above each pin a reduction order because float addition is not associative; the
// integer ones cannot need to, because it is (ADR-0040 Decision E). That absence is the reason
// this class's coverage reaches 100% while the float classes' stalls at 6 of 17.
pub const KDESC_BASE0_MATMUL: &str = "base0/matmul-quant/i8xi8-i32-exact/v1";
pub const KDESC_BASE0_REQUANTIZE: &str = "base0/requantize/srdhm-rshift-sat8/v1";
pub const KDESC_BASE0_RMS_NORM: &str = "base0/rms-norm/i64-sumsq-intrsqrt/v1";
pub const KDESC_BASE0_ROPE: &str = "base0/rope/pinned-table-pairwise/v1";
pub const KDESC_BASE0_SOFTMAX: &str = "base0/softmax/rowmax-intexp-intrecip/v1";
pub const KDESC_BASE0_SILU: &str = "base0/silu/intexp-sigmoid/v1";
pub const KDESC_BASE0_MUL_ELEM: &str = "base0/mul-elem/i32-exact/v1";
pub const KDESC_BASE0_ADD_ELEM: &str = "base0/add-elem/i32-exact/v1";
pub const KDESC_BASE0_EMBED: &str = "base0/embed-lookup/row-gather/v1";
/// ADR-0040 Decision H's op 9. The catalog shipped without it while Decision H recorded that the
/// other nine CANNOT compute without it — `SoftMax` and `Silu` are defined on Qk and nothing else
/// in the class can raise an accumulator to that scale. A class using it therefore had an
/// uncatalogued kernel, and an uncatalogued kernel is an `Unadjudicable` hole (A4): the one op that
/// makes the class work was the one the court could not adjudicate.
pub const KDESC_BASE0_RESCALE: &str = "base0/rescale/i64-mul-rshift-sat32/v1";

// --- PALW-QWEN36 (ADR-0052): the A16 tier and the hybrid graph's own ops ---------------------
//
// Same discipline as the BASE-0 block above and for the same reason: integer addition is
// associative, so none of these descriptors pins a reduction order, a lane structure or a libm
// flavour. What they do name is the WIDTH the accumulator is formed in, because that is
// arithmetic here — `i64` and `i128` disagree exactly where a sum leaves 63 bits, and the state
// read of the recurrence does (`q36_gdn_step`'s own doc records the cosine of 0.007 that a wrapped
// accumulator produced).
//
// The A16 group is the tier, not the model: `PALW-QWEN25`'s A16 class reaches the same nine and a
// class registering either reads them from here.
pub const KDESC_A16_EMBED: &str = "a16/embed-lookup/row-gather-i8/v1";
pub const KDESC_A16_MATMUL_REQUANT: &str = "a16/matmul/i8xi16-i64-exact-requant16/v1";
pub const KDESC_A16_MATMUL_RESCALE: &str = "a16/matmul/i8xi16-i64-exact-rescale32/v1";
pub const KDESC_A16_RMS_NORM: &str = "a16/rms-norm/i64-sumsq-intrsqrt/v1";
pub const KDESC_A16_REQUANTIZE: &str = "a16/requantize/i128-mul-rshift-sat16/v1";
pub const KDESC_A16_ADD_ELEM: &str = "a16/add-elem/i32-exact/v1";
pub const KDESC_A16_SOFTMAX: &str = "a16/softmax/rowmax-shifted-intexp-intrecip/v1";
pub const KDESC_A16_ATTN_SCORES: &str = "a16/attn-scores/i16xi16-i64-gqa/v1";
pub const KDESC_A16_ATTN_VALUES: &str = "a16/attn-values/i16xi16-i64-gqa/v1";

/// The hybrid graph's own ops (ADR-0052). Each names the accumulator width because that is the
/// only degree of freedom an integer kernel has left.
pub const KDESC_Q36_MATMUL_GROUPED: &str = "q36/matmul-grouped/i8xi16-per32-exp-i64/v1";
pub const KDESC_Q36_MATMUL_GROUPED_WIDE: &str = "q36/matmul-grouped-wide/i8xi16-per32-exp-i64-sat32/v1";
pub const KDESC_Q36_ROPE_PARTIAL: &str = "q36/rope/pinned-table-pairwise-partial/v1";
pub const KDESC_Q36_SSM_CONV: &str = "q36/ssm-conv/4tap-per-channel-qk/v1";
pub const KDESC_Q36_L2_NORM: &str = "q36/l2-norm/i64-sumsq-intrsqrt-q15/v1";
pub const KDESC_Q36_SIGMOID: &str = "q36/sigmoid/intexp-intrecip/v1";
pub const KDESC_Q36_GATE_APPLY: &str = "q36/gate-apply/i64-mul-narrow16/v1";
pub const KDESC_Q36_MUL_WIDE: &str = "q36/mul-wide/i32xi32-i64-narrow16/v1";
pub const KDESC_Q36_RESCALE_ROW: &str = "q36/rescale-row/i128-mul-rshift-sat32/v1";
pub const KDESC_Q36_RMS_NORM_WIDE: &str = "q36/rms-norm-wide/i128-sumsq-normalized-intrsqrt/v1";
pub const KDESC_Q36_ROUTER_TOPK: &str = "q36/router-topk/softmax-shifted-k-passes-low-index/v1";
pub const KDESC_Q36_MOE_COMBINE: &str = "q36/moe-combine/i64-weighted-sum-narrow16/v1";
pub const KDESC_Q36_GDN_STEP: &str = "q36/gated-delta-net/decay-read-i128-write-shift-i64/v1";
pub const KDESC_Q36_DECAY: &str = "q36/decay/softplus-intln-exp-refined/v1";

/// The A16 tier's nine, for a caller assembling a reachable set for any class in it.
pub const KDESC_A16_ALL: &[&str] = &[
    KDESC_A16_EMBED,
    KDESC_A16_MATMUL_REQUANT,
    KDESC_A16_MATMUL_RESCALE,
    KDESC_A16_RMS_NORM,
    KDESC_A16_REQUANTIZE,
    KDESC_A16_ADD_ELEM,
    KDESC_A16_SOFTMAX,
    KDESC_A16_ATTN_SCORES,
    KDESC_A16_ATTN_VALUES,
];

/// Qwen3.6's own fourteen, on top of [`KDESC_A16_ALL`] and BASE-0's `Silu`.
pub const KDESC_Q36_ALL: &[&str] = &[
    KDESC_Q36_MATMUL_GROUPED,
    KDESC_Q36_MATMUL_GROUPED_WIDE,
    KDESC_Q36_ROPE_PARTIAL,
    KDESC_Q36_SSM_CONV,
    KDESC_Q36_L2_NORM,
    KDESC_Q36_SIGMOID,
    KDESC_Q36_GATE_APPLY,
    KDESC_Q36_MUL_WIDE,
    KDESC_Q36_RESCALE_ROW,
    KDESC_Q36_RMS_NORM_WIDE,
    KDESC_Q36_ROUTER_TOPK,
    KDESC_Q36_MOE_COMBINE,
    KDESC_Q36_GDN_STEP,
    KDESC_Q36_DECAY,
];

/// Every descriptor this build can adjudicate, in one place so the coverage gate reads the same
/// table the adjudicator resolves against.
pub const KDESC_ALL: &[&str] = &[
    KDESC_L2_NORM,
    KDESC_RMS_NORM_FUSED,
    KDESC_SWIGLU,
    KDESC_SIGMOID_GLIBC_FMA,
    KDESC_SOFTPLUS_GLIBC_FMA,
    KDESC_GDN_CORE_NEON,
    KDESC_GDN_CORE_AVX2,
    KDESC_BASE0_MATMUL,
    KDESC_BASE0_REQUANTIZE,
    KDESC_BASE0_RMS_NORM,
    KDESC_BASE0_ROPE,
    KDESC_BASE0_SOFTMAX,
    KDESC_BASE0_SILU,
    KDESC_BASE0_MUL_ELEM,
    KDESC_BASE0_ADD_ELEM,
    KDESC_BASE0_EMBED,
    KDESC_BASE0_RESCALE,
    KDESC_A16_EMBED,
    KDESC_A16_MATMUL_REQUANT,
    KDESC_A16_MATMUL_RESCALE,
    KDESC_A16_RMS_NORM,
    KDESC_A16_REQUANTIZE,
    KDESC_A16_ADD_ELEM,
    KDESC_A16_SOFTMAX,
    KDESC_A16_ATTN_SCORES,
    KDESC_A16_ATTN_VALUES,
    KDESC_Q36_MATMUL_GROUPED,
    KDESC_Q36_MATMUL_GROUPED_WIDE,
    KDESC_Q36_ROPE_PARTIAL,
    KDESC_Q36_SSM_CONV,
    KDESC_Q36_L2_NORM,
    KDESC_Q36_SIGMOID,
    KDESC_Q36_GATE_APPLY,
    KDESC_Q36_MUL_WIDE,
    KDESC_Q36_RESCALE_ROW,
    KDESC_Q36_RMS_NORM_WIDE,
    KDESC_Q36_ROUTER_TOPK,
    KDESC_Q36_MOE_COMBINE,
    KDESC_Q36_GDN_STEP,
    KDESC_Q36_DECAY,
];

/// The `kernel_semantics_id`s this build can adjudicate — the catalog side of the ADR-0038 A4
/// coverage gate, read from the adjudicator rather than claimed by a caller.
pub fn catalogued_kernel_ids_v1() -> std::collections::BTreeSet<Hash64> {
    // From the ADJUDICATION table, not from `KDESC_ALL`. The gate's promise is that a certified
    // kernel can actually be re-executed here, and only this table knows that.
    KERNEL_CATALOG.iter().map(|(d, _)| kernel_semantics_id_v1(d)).collect()
}

/// The ten BASE-0 kernels, for a caller assembling that class's reachable set (ADR-0040 D + H).
pub const KDESC_BASE0_ALL: &[&str] = &[
    KDESC_BASE0_MATMUL,
    KDESC_BASE0_REQUANTIZE,
    KDESC_BASE0_RMS_NORM,
    KDESC_BASE0_ROPE,
    KDESC_BASE0_SOFTMAX,
    KDESC_BASE0_SILU,
    KDESC_BASE0_MUL_ELEM,
    KDESC_BASE0_ADD_ELEM,
    KDESC_BASE0_EMBED,
    KDESC_BASE0_RESCALE,
];

/// The programs this build can adjudicate. Resolution is by id, never by guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KernelProgram {
    L2Norm,
    RmsNormFused,
    Swiglu,
    SigmoidGlibcFma,
    SoftplusGlibcFma,
    GdnCore {
        dot: DotStructure,
    },
    /// ADR-0040's nine. One variant per op; no lane structure and no libm flavour, because an
    /// integer kernel has neither.
    Base0(Base0Op),
    /// The A16 tier and Qwen3.6's own ops (ADR-0052), same discipline.
    Qwen36(Qwen36Op),
}

/// The BASE-0 op a catalogued kernel id resolves to (ADR-0040 Decision D).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Base0Op {
    MatMul,
    Requantize,
    Rescale,
    RmsNorm,
    Rope,
    Softmax,
    Silu,
    MulElem,
    AddElem,
    Embed,
}

/// The A16 tier's nine and the hybrid graph's fourteen (ADR-0052). One variant per op, resolved
/// by id and executed by calling the SAME function the engine calls — there is no second
/// implementation of a kernel here, because a court that reimplements the arithmetic it judges is
/// a court with its own bugs to be wrong about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Qwen36Op {
    Embed,
    MatMulRequant,
    MatMulRescale,
    RmsNorm,
    Requantize,
    AddElem,
    Softmax,
    AttnScores,
    AttnValues,
    MatMulGrouped,
    MatMulGroupedWide,
    RopePartial,
    SsmConv,
    L2Norm,
    Sigmoid,
    GateApply,
    MulWide,
    RescaleRow,
    RmsNormWide,
    RouterTopk,
    MoeCombine,
    GdnStep,
    Decay,
}

/// The class's `ggml_vec_dot_f32` lane structure (simd-mappings.h, read verbatim).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DotStructure {
    /// NEON: STEP 16, 4 accumulators × 4 lanes; reduce x0+=x2, x1+=x3, x0+=x1, then the
    /// pairwise `vaddvq` ((l0+l1)+(l2+l3)).
    Step16Epr4,
    /// AVX2: STEP 32, 4 accumulators × 8 lanes; reduce x0+=x2, x1+=x3, x0+=x1, then
    /// low128+high128 and two `hadd`s ((l0+l1)+(l2+l3) over the 4-lane sum).
    Step32Epr8,
}

/// The one table that says what this build can adjudicate: descriptor → program.
///
/// Single source on purpose. [`resolve_kernel`] and [`catalogued_kernel_ids_v1`] both read it, so
/// "the coverage gate certified this kernel" and "the adjudicator can execute it" cannot come
/// apart. They used to be two hand-maintained lists that merely happened to agree; a descriptor
/// added to one and not the other would have let A4 certify a class whose disputes all land
/// `Unadjudicable` — rejected but unslashed, the exact hole coverage exists to close.
const KERNEL_CATALOG: &[(&str, KernelProgram)] = &[
    (KDESC_L2_NORM, KernelProgram::L2Norm),
    (KDESC_RMS_NORM_FUSED, KernelProgram::RmsNormFused),
    (KDESC_SWIGLU, KernelProgram::Swiglu),
    (KDESC_SIGMOID_GLIBC_FMA, KernelProgram::SigmoidGlibcFma),
    (KDESC_SOFTPLUS_GLIBC_FMA, KernelProgram::SoftplusGlibcFma),
    (KDESC_GDN_CORE_NEON, KernelProgram::GdnCore { dot: DotStructure::Step16Epr4 }),
    (KDESC_GDN_CORE_AVX2, KernelProgram::GdnCore { dot: DotStructure::Step32Epr8 }),
    (KDESC_BASE0_MATMUL, KernelProgram::Base0(Base0Op::MatMul)),
    (KDESC_BASE0_REQUANTIZE, KernelProgram::Base0(Base0Op::Requantize)),
    (KDESC_BASE0_RESCALE, KernelProgram::Base0(Base0Op::Rescale)),
    (KDESC_BASE0_RMS_NORM, KernelProgram::Base0(Base0Op::RmsNorm)),
    (KDESC_BASE0_ROPE, KernelProgram::Base0(Base0Op::Rope)),
    (KDESC_BASE0_SOFTMAX, KernelProgram::Base0(Base0Op::Softmax)),
    (KDESC_BASE0_SILU, KernelProgram::Base0(Base0Op::Silu)),
    (KDESC_BASE0_MUL_ELEM, KernelProgram::Base0(Base0Op::MulElem)),
    (KDESC_BASE0_ADD_ELEM, KernelProgram::Base0(Base0Op::AddElem)),
    (KDESC_BASE0_EMBED, KernelProgram::Base0(Base0Op::Embed)),
    (KDESC_A16_EMBED, KernelProgram::Qwen36(Qwen36Op::Embed)),
    (KDESC_A16_MATMUL_REQUANT, KernelProgram::Qwen36(Qwen36Op::MatMulRequant)),
    (KDESC_A16_MATMUL_RESCALE, KernelProgram::Qwen36(Qwen36Op::MatMulRescale)),
    (KDESC_A16_RMS_NORM, KernelProgram::Qwen36(Qwen36Op::RmsNorm)),
    (KDESC_A16_REQUANTIZE, KernelProgram::Qwen36(Qwen36Op::Requantize)),
    (KDESC_A16_ADD_ELEM, KernelProgram::Qwen36(Qwen36Op::AddElem)),
    (KDESC_A16_SOFTMAX, KernelProgram::Qwen36(Qwen36Op::Softmax)),
    (KDESC_A16_ATTN_SCORES, KernelProgram::Qwen36(Qwen36Op::AttnScores)),
    (KDESC_A16_ATTN_VALUES, KernelProgram::Qwen36(Qwen36Op::AttnValues)),
    (KDESC_Q36_MATMUL_GROUPED, KernelProgram::Qwen36(Qwen36Op::MatMulGrouped)),
    (KDESC_Q36_MATMUL_GROUPED_WIDE, KernelProgram::Qwen36(Qwen36Op::MatMulGroupedWide)),
    (KDESC_Q36_ROPE_PARTIAL, KernelProgram::Qwen36(Qwen36Op::RopePartial)),
    (KDESC_Q36_SSM_CONV, KernelProgram::Qwen36(Qwen36Op::SsmConv)),
    (KDESC_Q36_L2_NORM, KernelProgram::Qwen36(Qwen36Op::L2Norm)),
    (KDESC_Q36_SIGMOID, KernelProgram::Qwen36(Qwen36Op::Sigmoid)),
    (KDESC_Q36_GATE_APPLY, KernelProgram::Qwen36(Qwen36Op::GateApply)),
    (KDESC_Q36_MUL_WIDE, KernelProgram::Qwen36(Qwen36Op::MulWide)),
    (KDESC_Q36_RESCALE_ROW, KernelProgram::Qwen36(Qwen36Op::RescaleRow)),
    (KDESC_Q36_RMS_NORM_WIDE, KernelProgram::Qwen36(Qwen36Op::RmsNormWide)),
    (KDESC_Q36_ROUTER_TOPK, KernelProgram::Qwen36(Qwen36Op::RouterTopk)),
    (KDESC_Q36_MOE_COMBINE, KernelProgram::Qwen36(Qwen36Op::MoeCombine)),
    (KDESC_Q36_GDN_STEP, KernelProgram::Qwen36(Qwen36Op::GdnStep)),
    (KDESC_Q36_DECAY, KernelProgram::Qwen36(Qwen36Op::Decay)),
];

/// **Whether this build's adjudicator can serve THIS node's operand shape (G5).**
///
/// The coverage gate compared kernel ids: "is this id in the catalog". That is a weaker question
/// than the one it promises to answer, because a kernel serves a SHAPE — a matmul needs something
/// to multiply by, a two-operand elementwise op needs two rows — and a node can name a
/// catalogued id while asking for a shape nothing can produce. A class was certifiable at "100%
/// coverage" with steps that could never be recomputed, which is the failure the gate exists to
/// prevent.
///
/// So the adjudicator states, next to the code that does the serving, which node shapes it can
/// serve. Anything it cannot is a registration-time refusal rather than a dispute-time
/// `Unadjudicable`.
/// **Can this node be adjudicated at this CALL CLASS?** (ADR-0049 Decision D.)
///
/// `kernel_can_serve_node_v1` asks whether the adjudicator can serve a node's SHAPE, and
/// `verify_catalog_coverage_v1` asks whether its kernel id is catalogued. Neither asks the question
/// A4 actually needs, which is whether every reachable COORDINATE adjudicates — and a coordinate has
/// a call index as well as a node slot.
///
/// The gap is not hypothetical. Every kernel `PALW-BASE-0` reaches is catalogued and every node it
/// declares is servable, and its embedding gather still refuses at `call_index != 0`
/// (`base0_row`'s `Embed` arm) while its own canonical job is prefill 8 / **decode 4**. Coverage
/// reported 100 % on a class with a whole call class it could not police.
pub fn kernel_can_serve_call_class_v1(node: &crate::palw_step::PalwStepNodeV1, call_is_decode: bool) -> Result<(), &'static str> {
    let Some(program) = resolve_kernel(&node.kernel_semantics_id) else {
        return Err("no program in this build resolves the node's kernel id");
    };
    if !call_is_decode {
        return Ok(());
    }
    // Every catalogued program serves both call classes since ADR-0049 Decision E: the gather was
    // the one that did not, and a decode token is now pinned by the claim's own logits trace root
    // rather than by nothing. The match stays so that the next kernel with a call-class-dependent
    // arm is refused here rather than discovered by a producer.
    let _ = program;
    Ok(())
}

pub fn kernel_can_serve_node_v1(node: &crate::palw_step::PalwStepNodeV1, table_is_pre: bool) -> Result<(), &'static str> {
    use crate::palw_step::PalwStepOutLenV1;
    let Some(program) = resolve_kernel(&node.kernel_semantics_id) else {
        return Err("no program in this build resolves the node's kernel id");
    };

    // **Can the canonical INPUT SET be built for this node at all?**
    //
    // Serving a shape is half the question. The other half is whether
    // `canonical_input_leaves` can name the leaves a challenger must open — and it answers
    // `None` for every KV and checkpoint sentinel ("registration-opaque today"), and for
    // `LAYER_IN` in the pre table, which has no upstream. A `None` there is `Unadjudicable` at
    // the first dispute no matter what the kernel can compute, so a node that asks for one is a
    // node no court can reach.
    for r in &node.input_refs {
        if *r < crate::palw_step::PALW_STEP_INPUT_SENTINEL_MIN {
            continue;
        }
        match *r {
            crate::palw_step::PALW_STEP_INPUT_LAYER_IN => {
                if table_is_pre {
                    return Err("the pre table has no upstream, so LAYER_IN names nothing there");
                }
            }
            // Resolvable since G5c: the sentinel names whichever node of this layer's table
            // carries the matching cache role, read over the position history. The role must
            // exist and be unique, or "the K cache" names nothing or two things — and a court
            // that had to choose would be choosing its own evidence.
            crate::palw_step::PALW_STEP_INPUT_KV_K | crate::palw_step::PALW_STEP_INPUT_KV_V => {
                if table_is_pre {
                    return Err("the pre table has no cache-role nodes for a KV sentinel to name");
                }
            }
            _ => return Err("the checkpoint input sentinel is registration-opaque: canonical_input_leaves cannot name its leaves"),
        }
    }
    let inputs = node.input_refs.len();
    match program {
        // **The A16/Q36 matmuls take their matrix from the registry, never from a second row.**
        // The tier's weights are `int8` rows with (for the grouped pair) a per-32 exponent table
        // beside them, and neither is an activation a leg could open — so a node that names no
        // weight has nothing to multiply by, and one that is kv-scaled names a matrix of a width
        // the oracle cannot know.
        KernelProgram::Qwen36(
            Qwen36Op::MatMulRequant | Qwen36Op::MatMulRescale | Qwen36Op::MatMulGrouped | Qwen36Op::MatMulGroupedWide,
        ) => {
            if node.weight_name.is_empty() {
                return Err("an A16 matmul must name its weight tensor: the tier has no weightless form");
            }
            if inputs < 1 {
                return Err("an A16 matmul must name the row it multiplies");
            }
            if matches!(node.out_len, PalwStepOutLenV1::KvScaled { .. }) {
                return Err("a registered weight has a fixed width; a kv-scaled one names no matrix the oracle holds");
            }
            Ok(())
        }
        // Two opened rows and a registered narrowing: the gate product, the wide product and the
        // mixture's combine. One row is a node that has nothing to multiply BY.
        KernelProgram::Qwen36(Qwen36Op::GateApply | Qwen36Op::MulWide | Qwen36Op::MoeCombine | Qwen36Op::AddElem) => {
            if inputs < 2 {
                return Err("a two-operand elementwise node must name both rows");
            }
            Ok(())
        }
        // Attention's two reductions read the query (or the probabilities) and the cached series.
        KernelProgram::Qwen36(Qwen36Op::AttnScores | Qwen36Op::AttnValues) => {
            if inputs < 2 {
                return Err("an attention reduction must name its row and its cached series");
            }
            if node.weight_name.is_empty() {
                return Err("an attention reduction must name the tensor its narrowing is registered in");
            }
            Ok(())
        }
        // The recurrence reads five rows per position — keys, the conv row, queries and the two
        // gates — and replays from the genesis; the narrowings are registration artifacts.
        KernelProgram::Qwen36(Qwen36Op::GdnStep) => {
            if inputs < 5 {
                return Err("the recurrence must name its five per-position rows");
            }
            if node.weight_name.is_empty() {
                return Err("the recurrence must name the tensor its four narrowings are registered in");
            }
            Ok(())
        }
        // Everything else in the tier is one opened row plus, for some, a registered parameter
        // table. The table is checked when it is read; what is refused here is a node with no row
        // to compute from at all.
        KernelProgram::Qwen36(
            Qwen36Op::RmsNorm
            | Qwen36Op::Requantize
            | Qwen36Op::Softmax
            | Qwen36Op::RopePartial
            | Qwen36Op::L2Norm
            | Qwen36Op::Sigmoid
            | Qwen36Op::RescaleRow
            | Qwen36Op::RmsNormWide
            | Qwen36Op::RouterTopk
            | Qwen36Op::Decay,
        ) => {
            if inputs < 1 {
                return Err("a one-operand node must name the row it computes from");
            }
            Ok(())
        }
        // The window: the three projection rows, whose per-position expansion is the leaf set's.
        KernelProgram::Qwen36(Qwen36Op::SsmConv) => {
            if inputs < 3 {
                return Err("the convolution must name the three projection rows its window is built from");
            }
            if node.weight_name.is_empty() {
                return Err("the convolution must name its tap tensor");
            }
            Ok(())
        }
        // The gather names a table and no row: its input is the token id, which the court holds.
        KernelProgram::Qwen36(Qwen36Op::Embed) => {
            if node.weight_name.is_empty() {
                return Err("the embedding gather must name its table");
            }
            if !inputs.is_multiple_of(1) || inputs != 0 {
                return Err("the embedding gather reads the carried token ids, not an opened row");
            }
            Ok(())
        }
        // Two operand sources, and exactly two: a registered weight, or a second opened row.
        // A node with neither has nothing to multiply by; one with both would have two answers.
        KernelProgram::Base0(Base0Op::MatMul) => {
            if node.weight_name.is_empty() {
                if inputs < 2 {
                    return Err("a weightless matmul must name a second input row to multiply by");
                }
            } else if matches!(node.out_len, PalwStepOutLenV1::KvScaled { .. }) {
                return Err("a registered weight has a fixed width; a kv-scaled one names no matrix the oracle holds");
            }
            Ok(())
        }
        // The ops whose second operand is a registration artifact: they need a name, and the
        // oracle serves a fixed width.
        KernelProgram::Base0(Base0Op::Requantize | Base0Op::Rope | Base0Op::Rescale) | KernelProgram::RmsNormFused => {
            if node.weight_name.is_empty() {
                return Err("this kernel resolves its parameters through the weight oracle and the node names none");
            }
            Ok(())
        }
        // Two opened rows, no weight.
        KernelProgram::Base0(Base0Op::MulElem | Base0Op::AddElem) | KernelProgram::Swiglu => {
            if inputs < 2 {
                return Err("a two-operand elementwise kernel needs two input rows");
            }
            Ok(())
        }
        // The gather takes NO opened row (G5d): its operands are the registered embedding table
        // and the token id the refutation carries, so a node that declared an input would be
        // declaring one nothing supplies — the pre table has no upstream.
        KernelProgram::Base0(Base0Op::Embed) => {
            if node.weight_name.is_empty() {
                return Err("an embedding gather names the table it reads from");
            }
            if !node.input_refs.is_empty() {
                return Err("an embedding gather takes no opened row: its operands are the table and the token id");
            }
            Ok(())
        }
        // One opened row.
        KernelProgram::Base0(Base0Op::RmsNorm | Base0Op::Softmax | Base0Op::Silu)
        | KernelProgram::L2Norm
        | KernelProgram::SigmoidGlibcFma
        | KernelProgram::SoftplusGlibcFma => {
            if inputs < 1 {
                return Err("a unary kernel needs one input row");
            }
            Ok(())
        }
        // The GDN core reads its five wiring rows per prior position; the profile's own
        // `canonical_input_leaves` builds that set, so the only thing to check here is that the
        // node declares wiring at all.
        KernelProgram::GdnCore { .. } => {
            if inputs < 5 {
                return Err("the gated-delta-net core needs its five wiring inputs");
            }
            Ok(())
        }
    }
}

fn resolve_kernel(id: &Hash64) -> Option<KernelProgram> {
    KERNEL_CATALOG.iter().find(|(d, _)| kernel_semantics_id_v1(d) == *id).map(|(_, p)| *p)
}

/// Recompute one BASE-0 node's output row (ADR-0040 Decision D).
///
/// Values ride the step leg as little-endian 4-byte groups, so a BASE-0 tile is `i32` bit
/// patterns — the same container the float classes use for `f32` bits, reinterpreted. Nothing is
/// converted: an integer class stores integers.
///
/// Every arm delegates to [`crate::palw_base0_ops`]. The adjudicator must run the SAME code a
/// conforming implementation runs, not a second transcription of it — a court whose reference
/// diverges from the class convicts honest producers, which is exactly the false-positive this
/// class was chosen to make unrepresentable.
/// The width of one per-head sub-row of a `KvScaled` node, or the whole row for a fixed one.
///
/// A `KvScaled { multiplier }` node is `multiplier` sub-rows of `kv_len` laid side by side — the
/// head-major concatenation the engine builds inside its query-head loop. Reading the multiplier
/// from the NODE rather than from `profile.attn_heads` is what keeps this correct for a graph whose
/// scaled nodes are not all per-head.
fn base0_kv_chunk_width(node: &crate::palw_step::PalwStepNodeV1, kv_len: u64, row_len: usize) -> Result<usize, PalwStepRefuteError> {
    match node.out_len {
        crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => {
            let chunk = usize::try_from(kv_len).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            let expected = chunk.checked_mul(multiplier as usize).ok_or(PalwStepRefuteError::Unadjudicable)?;
            if chunk == 0 || row_len != expected {
                return Err(PalwStepRefuteError::InputSetNotCanonical("base0 kv-scaled row is not multiplier x kv_len"));
            }
            Ok(chunk)
        }
        crate::palw_step::PalwStepOutLenV1::Fixed { .. } => {
            if row_len == 0 {
                return Err(PalwStepRefuteError::InputSetNotCanonical("base0 row is empty"));
            }
            Ok(row_len)
        }
    }
}

/// **Recompute one step of the A16 tier or of Qwen3.6's own graph** (ADR-0052).
///
/// Every arm calls the SAME function the engine calls. That is the point and not an economy: a
/// court that reimplements the arithmetic it judges has its own bugs to be wrong about, and the
/// class's whole claim is that one program has one answer. What this function does is the
/// plumbing the engine does not need — decoding operands out of opened leaves and registered
/// tensors, and refusing anything it cannot decode rather than guessing.
///
/// Parameters arrive as `A16QuantParams` wire bytes from the oracle, seventeen per channel. A
/// table that is short, mis-sized or undecodable is [`PalwStepRefuteError::Unadjudicable`] — the
/// court not being able to check is never someone's fault.
#[allow(clippy::too_many_arguments)]
fn qwen36_row(
    op: Qwen36Op,
    node: &crate::palw_step::PalwStepNodeV1,
    layer: Option<u16>,
    profile: &PalwShapeProfileV3,
    inputs: &[Vec<u32>],
    weights: &dyn PalwWeightOracleV1,
    kv_len: u64,
    gather: (&PalwStepCoordinateV1, &[u32], &[u32]),
) -> Result<Vec<u32>, PalwStepRefuteError> {
    use crate::palw_base0_a16 as a16;
    use crate::palw_base0_a16::A16QuantParams;
    use crate::palw_qwen36_ops as q36;

    let need = |n: usize| -> Result<(), PalwStepRefuteError> {
        if inputs.len() < n {
            return Err(PalwStepRefuteError::InputSetNotCanonical("a16 node has too few input rows"));
        }
        Ok(())
    };
    let as_i32 = |row: &Vec<u32>| -> Vec<i32> { row.iter().map(|v| *v as i32).collect() };
    let out = |row: Vec<i32>| -> Vec<u32> { row.into_iter().map(|v| v as u32).collect() };
    let shape16 = |_e: a16::PalwA16OpError| PalwStepRefuteError::InputSetNotCanonical("an a16 op refused its operand shape");
    let shape36 = |_e: q36::PalwQwen36OpError| PalwStepRefuteError::InputSetNotCanonical("a q36 op refused its operand shape");

    // `count` parameter triples from the node's registered tensor, starting at `first`.
    let params = |first: usize, count: usize| -> Result<Vec<A16QuantParams>, PalwStepRefuteError> {
        if node.weight_name.is_empty() || count == 0 {
            return Err(PalwStepRefuteError::Unadjudicable);
        }
        let width = A16QuantParams::WIRE_BYTES;
        let offset = u32::try_from(first.checked_mul(width).ok_or(PalwStepRefuteError::Unadjudicable)?)
            .map_err(|_| PalwStepRefuteError::Unadjudicable)?;
        let len = u32::try_from(count.checked_mul(width).ok_or(PalwStepRefuteError::Unadjudicable)?)
            .map_err(|_| PalwStepRefuteError::Unadjudicable)?;
        let bytes = weights
            .operand_bytes(node.weight_name.as_str(), layer, offset, len)
            .ok_or(PalwStepRefuteError::Unadjudicable)?;
        if bytes.len() != len as usize {
            return Err(PalwStepRefuteError::Unadjudicable);
        }
        bytes
            .chunks_exact(width)
            .map(|c| A16QuantParams::from_wire(c).map_err(|_| PalwStepRefuteError::InputSetNotCanonical("an a16 parameter triple is malformed")))
            .collect()
    };
    let fixed_width = || -> Result<usize, PalwStepRefuteError> {
        match node.out_len {
            crate::palw_step::PalwStepOutLenV1::Fixed { elements } => Ok(elements as usize),
            crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => {
                let n = (multiplier as u64).checked_mul(kv_len).ok_or(PalwStepRefuteError::Unadjudicable)?;
                usize::try_from(n).map_err(|_| PalwStepRefuteError::Unadjudicable)
            }
        }
    };

    match op {
        // The gather, adjudicated the way BASE-0's is: the row comes from the registered table at
        // the token's own offset, and the token from the carried ids the court has already matched
        // against the job context.
        Qwen36Op::Embed => {
            let (coord, prompt_ids, generated_ids) = gather;
            let token = if coord.call_index == 0 {
                *prompt_ids.get(coord.position as usize).ok_or(PalwStepRefuteError::Unadjudicable)?
            } else {
                let produced = (coord.call_index as usize).checked_sub(1).ok_or(PalwStepRefuteError::Unadjudicable)?;
                *generated_ids.get(produced).ok_or(PalwStepRefuteError::Unadjudicable)?
            };
            let width = fixed_width()?;
            let start = (token as u64).checked_mul(width as u64).ok_or(PalwStepRefuteError::Unadjudicable)?;
            let start = u32::try_from(start).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            let width32 = u32::try_from(width).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            let row = weights
                .operand_bytes(node.weight_name.as_str(), layer, start, width32)
                .ok_or(PalwStepRefuteError::Unadjudicable)?;
            if row.len() != width {
                return Err(PalwStepRefuteError::Unadjudicable);
            }
            Ok(row.iter().map(|b| *b as i8 as i32 as u32).collect())
        }
        // **The challenged TILE of the projection, not the whole matrix** (ADR-0049 Decision B).
        // An output channel reduces over the whole input row and over its own weight row alone, so
        // a tile of channels needs a contiguous slice of weight rows and nothing else — which is
        // what keeps the terminal adjudication from growing with the model.
        Qwen36Op::MatMulRequant | Qwen36Op::MatMulRescale | Qwen36Op::MatMulGrouped | Qwen36Op::MatMulGroupedWide => {
            need(1)?;
            let x = as_i32(&inputs[0]);
            if x.is_empty() {
                return Err(PalwStepRefuteError::InputSetNotCanonical("a16 matmul input row is empty"));
            }
            let out_dim = fixed_width()?;
            if out_dim == 0 {
                return Err(PalwStepRefuteError::InputSetNotCanonical("a16 matmul output width is zero"));
            }
            let first = (gather.0.tile_index as usize).checked_mul(node.tile_len as usize).ok_or(PalwStepRefuteError::Unadjudicable)?;
            if first >= out_dim {
                return Err(PalwStepRefuteError::InputSetNotCanonical("the challenged tile is past the node's output width"));
            }
            let rows = (node.tile_len as usize).min(out_dim - first);
            let byte_offset =
                u32::try_from(first.checked_mul(x.len()).ok_or(PalwStepRefuteError::Unadjudicable)?).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            let byte_len =
                u32::try_from(rows.checked_mul(x.len()).ok_or(PalwStepRefuteError::Unadjudicable)?).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            let w = weights
                .operand_bytes(node.weight_name.as_str(), layer, byte_offset, byte_len)
                .ok_or(PalwStepRefuteError::Unadjudicable)?;
            if w.len() != byte_len as usize {
                return Err(PalwStepRefuteError::Unadjudicable);
            }
            let codes: Vec<i8> = w.iter().map(|b| *b as i8).collect();
            let p = params(first, rows)?;
            match op {
                Qwen36Op::MatMulRequant => Ok(out(a16::a16_matmul_requant(&codes, &x, &p).map_err(shape16)?)),
                Qwen36Op::MatMulRescale => Ok(out(a16::a16_matmul_rescale(&codes, &x, &p).map_err(shape16)?)),
                // The grouped pair reads a second registered tensor: one `i8` exponent per 32
                // weights, named by suffix so a single wiring names both halves of one operand.
                _ => {
                    let group = q36::QWEN36_WEIGHT_GROUP;
                    let groups = x.len().div_ceil(group);
                    let e_offset = u32::try_from(first.checked_mul(groups).ok_or(PalwStepRefuteError::Unadjudicable)?)
                        .map_err(|_| PalwStepRefuteError::Unadjudicable)?;
                    let e_len = u32::try_from(rows.checked_mul(groups).ok_or(PalwStepRefuteError::Unadjudicable)?)
                        .map_err(|_| PalwStepRefuteError::Unadjudicable)?;
                    let name = format!("{}.exp", node.weight_name);
                    let e = weights.operand_bytes(name.as_str(), layer, e_offset, e_len).ok_or(PalwStepRefuteError::Unadjudicable)?;
                    if e.len() != e_len as usize {
                        return Err(PalwStepRefuteError::Unadjudicable);
                    }
                    let exps: Vec<i8> = e.iter().map(|b| *b as i8).collect();
                    if op == Qwen36Op::MatMulGrouped {
                        Ok(out(q36::q36_matmul_grouped(&codes, &exps, &x, &p).map_err(shape36)?))
                    } else {
                        Ok(out(q36::q36_matmul_grouped_wide(&codes, &exps, &x, &p).map_err(shape36)?))
                    }
                }
            }
        }
        Qwen36Op::RmsNorm => {
            need(1)?;
            Ok(out(a16::a16_rms_norm(&as_i32(&inputs[0]), profile.base0_rms_eps_q).map_err(shape16)?))
        }
        // **`eps` is a registered triple, not the profile's scalar.** The wide norm compares it
        // against a mean of squares of CODES, so it is only meaningful at the site's own exponent
        // — a shared constant rounds to nothing on a loud head and becomes the whole denominator
        // on a quiet one. The class registers it per head; the court reads what was registered.
        Qwen36Op::RmsNormWide => {
            need(1)?;
            let x = as_i32(&inputs[0]);
            let hd = profile.gdn_head_v_dim as usize;
            if hd == 0 || !x.len().is_multiple_of(hd) {
                return Err(PalwStepRefuteError::InputSetNotCanonical("the wide-norm row is not a whole number of heads"));
            }
            let heads = x.len() / hd;
            let eps = params(0, heads)?;
            let mut row = Vec::with_capacity(x.len());
            for (h, head) in x.chunks_exact(hd).enumerate() {
                row.extend(q36::q36_rms_norm_wide(head, eps[h]).map_err(shape36)?);
            }
            Ok(out(row))
        }
        Qwen36Op::Requantize => {
            need(1)?;
            let x = as_i32(&inputs[0]);
            let p = params(0, x.len())?;
            Ok(out(a16::a16_requant(&x, &p).map_err(shape16)?))
        }
        Qwen36Op::RescaleRow => {
            need(1)?;
            let x = as_i32(&inputs[0]);
            let p = params(0, x.len())?;
            Ok(out(q36::q36_rescale_row(&x, &p).map_err(shape36)?))
        }
        Qwen36Op::AddElem => {
            need(2)?;
            Ok(out(a16::a16_add_elem(&as_i32(&inputs[0]), &as_i32(&inputs[1])).map_err(shape16)?))
        }
        // **Per head over the row** — the node's width is the concatenation of every head, so a
        // one-head step space would not contain heads 1..N. The head dim is the profile's.
        Qwen36Op::L2Norm => {
            need(1)?;
            let x = as_i32(&inputs[0]);
            let hd = profile.gdn_head_k_dim as usize;
            if hd == 0 || !x.len().is_multiple_of(hd) {
                return Err(PalwStepRefuteError::InputSetNotCanonical("the l2 row is not a whole number of heads"));
            }
            let mut row = Vec::with_capacity(x.len());
            for head in x.chunks_exact(hd) {
                row.extend(q36::q36_l2_norm(head).map_err(shape36)?);
            }
            Ok(out(row))
        }
        Qwen36Op::Sigmoid => {
            need(1)?;
            Ok(out(q36::q36_sigmoid_gate(&as_i32(&inputs[0]))))
        }
        // `exp(a · softplus(dt))` for the head's registered coefficient, which rides the triple's
        // zero at Q[K] exactly as the converter writes it.
        Qwen36Op::Decay => {
            need(1)?;
            let x = as_i32(&inputs[0]);
            let p = params(0, x.len())?;
            Ok(out(x.iter().zip(&p).map(|(v, c)| q36::q36_decay(*v, c.zero) as i32).collect()))
        }
        Qwen36Op::GateApply => {
            need(2)?;
            let p = params(0, 1)?[0];
            Ok(out(q36::q36_gate_apply(&as_i32(&inputs[0]), &as_i32(&inputs[1]), p).map_err(shape36)?))
        }
        Qwen36Op::MulWide => {
            need(2)?;
            let a = as_i32(&inputs[0]);
            let p = params(0, a.len())?;
            Ok(out(q36::q36_mul_wide(&a, &as_i32(&inputs[1]), &p).map_err(shape36)?))
        }
        // **The window is assembled from the last (up to) four positions' projections** — three
        // rows per position, position-major, exactly the order `canonical_input_leaves` lists.
        // Missing leading positions are zero rows, which is the window the engine starts from.
        Qwen36Op::SsmConv => {
            if inputs.is_empty() || !inputs.len().is_multiple_of(3) || inputs.len() > 12 {
                return Err(PalwStepRefuteError::InputSetNotCanonical("the conv window is three rows per position, at most four positions"));
            }
            let rows: Vec<Vec<i32>> = inputs
                .chunks_exact(3)
                .map(|c| {
                    let mut row = as_i32(&c[0]);
                    row.extend(as_i32(&c[1]));
                    row.extend(as_i32(&c[2]));
                    row
                })
                .collect();
            let channels = rows.last().map(|r| r.len()).unwrap_or(0);
            if channels == 0 || rows.iter().any(|r| r.len() != channels) {
                return Err(PalwStepRefuteError::InputSetNotCanonical("the window's positions disagree about the channel count"));
            }
            let mut window = vec![0i32; (4 - rows.len()) * channels];
            for row in &rows {
                window.extend_from_slice(row);
            }
            // `q36_ssm_conv` reads `window[t * channels + c]` — position-major, oldest first —
            // which is exactly the concatenation above.
            let byte_len = u32::try_from(window.len()).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            let taps = weights
                .operand_bytes(node.weight_name.as_str(), layer, 0, byte_len)
                .ok_or(PalwStepRefuteError::Unadjudicable)?;
            if taps.len() != window.len() {
                return Err(PalwStepRefuteError::Unadjudicable);
            }
            let taps: Vec<i32> = taps.iter().map(|b| *b as i8 as i32).collect();
            let p = {
                let name = format!("{}.a16", node.weight_name);
                let width = A16QuantParams::WIRE_BYTES;
                let len = u32::try_from(channels * width).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
                let bytes = weights.operand_bytes(name.as_str(), layer, 0, len).ok_or(PalwStepRefuteError::Unadjudicable)?;
                if bytes.len() != len as usize {
                    return Err(PalwStepRefuteError::Unadjudicable);
                }
                bytes
                    .chunks_exact(width)
                    .map(|c| A16QuantParams::from_wire(c).map_err(|_| PalwStepRefuteError::InputSetNotCanonical("a conv triple is malformed")))
                    .collect::<Result<Vec<_>, _>>()?
            };
            Ok(out(q36::q36_ssm_conv(&window, &taps, channels, &p).map_err(shape36)?))
        }
        // The rotation reads the pinned table at this position, which the profile carries rather
        // than the oracle: a table the court derives is a table the court can get wrong.
        Qwen36Op::RopePartial => {
            need(1)?;
            let x = as_i32(&inputs[0]);
            let head_dim = profile.attn_head_dim as usize;
            let rotary = profile.rope_dims as usize;
            if head_dim == 0 || rotary == 0 || rotary > head_dim {
                return Err(PalwStepRefuteError::Unadjudicable);
            }
            let pairs = rotary / 2;
            let bytes = u32::try_from(pairs * 8).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            let offset = (gather.0.position as u64).checked_mul(pairs as u64 * 8).ok_or(PalwStepRefuteError::Unadjudicable)?;
            let offset = u32::try_from(offset).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            let table = weights
                .operand_bytes(node.weight_name.as_str(), layer, offset, bytes)
                .ok_or(PalwStepRefuteError::Unadjudicable)?;
            if table.len() != pairs * 8 {
                return Err(PalwStepRefuteError::Unadjudicable);
            }
            let cos: Vec<i32> = table[..pairs * 4].chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
            let sin: Vec<i32> = table[pairs * 4..].chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
            let clamp = {
                let name = format!("{}.clamp", node.weight_name);
                let width = A16QuantParams::WIRE_BYTES;
                let bytes = weights
                    .operand_bytes(name.as_str(), layer, 0, width as u32)
                    .ok_or(PalwStepRefuteError::Unadjudicable)?;
                A16QuantParams::from_wire(&bytes).map_err(|_| PalwStepRefuteError::InputSetNotCanonical("the rope clamp triple is malformed"))?
            };
            Ok(out(q36::q36_rope_partial(&x, head_dim, rotary, &cos, &sin, clamp).map_err(shape36)?))
        }
        // The router commits its narrowed logit row and the court recomputes the SELECTION from
        // it: the weights the mixture uses, in expert order. The tie rule is part of the op, which
        // is why the row is committed before the selection rather than after.
        Qwen36Op::RouterTopk => {
            need(1)?;
            let logits = as_i32(&inputs[0]);
            let p = params(0, 1)?[0];
            let k = usize::try_from(p.multiplier).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            let up = u8::try_from(p.zero.clamp(0, 62)).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            let routed = q36::q36_router_topk(&logits, k, up).map_err(shape36)?;
            Ok(routed.into_iter().flat_map(|r| [r.expert as i32 as u32, r.weight_q as u32]).collect())
        }
        Qwen36Op::MoeCombine => {
            need(2)?;
            let outputs = as_i32(&inputs[0]);
            let w = as_i32(&inputs[1]);
            let width = fixed_width()?;
            let p = params(0, 1)?[0];
            Ok(out(q36::q36_moe_combine(&outputs, &w, width, p).map_err(shape36)?))
        }
        Qwen36Op::Softmax => {
            need(1)?;
            let row_len = usize::try_from(kv_len).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            if row_len == 0 {
                return Err(PalwStepRefuteError::Unadjudicable);
            }
            let p = params(0, 1)?[0];
            let up = u8::try_from(p.zero.clamp(0, 62)).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            Ok(out(a16::a16_softmax_rows(&as_i32(&inputs[0]), row_len, up).map_err(shape16)?))
        }
        Qwen36Op::AttnScores | Qwen36Op::AttnValues => {
            need(2)?;
            let heads = profile.attn_heads as usize;
            let kv_heads = profile.attn_kv_heads as usize;
            let d_head = profile.attn_head_dim as usize;
            if heads == 0 || kv_heads == 0 || d_head == 0 {
                return Err(PalwStepRefuteError::Unadjudicable);
            }
            let row = as_i32(&inputs[0]);
            let series = as_i32(&inputs[1]);
            let count = if op == Qwen36Op::AttnScores { heads.saturating_mul(series.len() / (kv_heads * d_head).max(1)) } else { heads * d_head };
            let p = params(0, count.max(1))?;
            if op == Qwen36Op::AttnScores {
                Ok(out(a16::a16_attn_scores(&row, &series, heads, kv_heads, d_head, &p).map_err(shape16)?))
            } else {
                Ok(out(a16::a16_attn_values(&row, &series, heads, kv_heads, d_head, &p).map_err(shape16)?))
            }
        }
        // **The recurrence, replayed from the genesis** — the same anchoring the float `GdnCore`
        // uses, and for the same reason: the state is never an opened operand, so the court
        // recomputes it from the committed per-position rows. Five rows per position, position-
        // major and ascending, exactly as `canonical_input_leaves` lists them: the unit keys, the
        // conv row (whose third block is `v`), the unit queries, the decay gates and the beta
        // gates. A registered state chunk map later turns this checkpoint-anchored.
        Qwen36Op::GdnStep => {
            if inputs.is_empty() || !inputs.len().is_multiple_of(5) {
                return Err(PalwStepRefuteError::InputSetNotCanonical("the recurrence reads five rows per position"));
            }
            let (hd_k, hd_v) = (profile.gdn_head_k_dim as usize, profile.gdn_head_v_dim as usize);
            let heads = profile.gdn_heads as usize;
            if hd_k == 0 || hd_v == 0 || heads == 0 {
                return Err(PalwStepRefuteError::Unadjudicable);
            }
            // Four registered triples per head: read, delta, write (a shift in `zero`), out.
            let p = params(0, 4 * heads)?;
            let mut states: Vec<q36::Qwen36GdnStateV1> =
                (0..heads).map(|_| q36::Qwen36GdnStateV1 { d_v: hd_v, d_k: hd_k, s: vec![0; hd_v * hd_k] }).collect();
            let k_heads = {
                // The key row tiles the value heads: `vh % k_heads`, as the engine and the
                // reference both read it.
                let k_row = inputs[0].len();
                if k_row == 0 || !k_row.is_multiple_of(hd_k) {
                    return Err(PalwStepRefuteError::InputSetNotCanonical("the key row is not a whole number of heads"));
                }
                k_row / hd_k
            };
            let mut last = Vec::new();
            for step in inputs.chunks_exact(5) {
                let unit_k = as_i32(&step[0]);
                let conv = as_i32(&step[1]);
                let unit_q = as_i32(&step[2]);
                let decays = as_i32(&step[3]);
                let betas = as_i32(&step[4]);
                let dk_total = k_heads * hd_k;
                if conv.len() < 2 * dk_total + heads * hd_v || decays.len() < heads || betas.len() < heads {
                    return Err(PalwStepRefuteError::InputSetNotCanonical("a replay position's rows do not cover the geometry"));
                }
                let v_block = &conv[2 * dk_total..];
                let mut row = Vec::with_capacity(heads * hd_v);
                for vh in 0..heads {
                    let kh = vh % k_heads;
                    let gdn = q36::Qwen36GdnParamsV1 {
                        read: p[4 * vh],
                        delta: p[4 * vh + 1],
                        write_shift: p[4 * vh + 2].zero as i32,
                        out: p[4 * vh + 3],
                    };
                    let head_out = q36::q36_gdn_step(
                        &mut states[vh],
                        &unit_k[kh * hd_k..(kh + 1) * hd_k],
                        &v_block[vh * hd_v..(vh + 1) * hd_v],
                        &unit_q[kh * hd_k..(kh + 1) * hd_k],
                        decays[vh] as i64,
                        betas[vh] as i64,
                        gdn,
                    )
                    .map_err(shape36)?;
                    row.extend(head_out);
                }
                last = row;
            }
            Ok(out(last))
        }
    }
}

fn base0_row(
    op: Base0Op,
    node: &crate::palw_step::PalwStepNodeV1,
    layer: Option<u16>,
    profile: &PalwShapeProfileV3,
    inputs: &[Vec<u32>],
    weights: &dyn PalwWeightOracleV1,
    kv_len: u64,
    gather: (&PalwStepCoordinateV1, &[u32], &[u32]),
) -> Result<Vec<u32>, PalwStepRefuteError> {
    use crate::palw_base0_ops as ops;
    let need = |n: usize| -> Result<(), PalwStepRefuteError> {
        if inputs.len() < n {
            return Err(PalwStepRefuteError::InputSetNotCanonical("base0 node has too few input rows"));
        }
        Ok(())
    };
    let as_i32 = |row: &Vec<u32>| -> Vec<i32> { row.iter().map(|v| *v as i32).collect() };
    let out = |row: Vec<i32>| -> Vec<u32> { row.into_iter().map(|v| v as u32).collect() };
    // int8 codes ride the same i32 lanes; anything outside the range is not a BASE-0 activation.
    let as_i8 = |row: &Vec<u32>| -> Result<Vec<i8>, PalwStepRefuteError> {
        row.iter()
            .map(|v| i8::try_from(*v as i32).map_err(|_| PalwStepRefuteError::InputSetNotCanonical("base0 int8 lane out of range")))
            .collect()
    };
    let shape = |e: ops::PalwBase0OpError| -> PalwStepRefuteError {
        let _ = e;
        PalwStepRefuteError::InputSetNotCanonical("base0 op refused its operand shape")
    };

    match op {
        Base0Op::RmsNorm => {
            need(1)?;
            // The CLASS's epsilon, from its registered shape profile — not a constant. Recomputing
            // with a hardcoded `1` convicted every honest producer of a class registered with any
            // other epsilon, on every norm step (re-audit §3.3).
            Ok(out(ops::rms_norm(&as_i8(&inputs[0])?, profile.base0_rms_eps_q).map_err(shape)?))
        }
        Base0Op::Softmax => {
            need(1)?;
            // **One softmax PER HEAD, because that is what attention is** (false-conviction fix).
            //
            // The engine runs `softmax` inside its query-head loop and appends the per-head
            // distributions head-major into one row; this arm ran ONE softmax over the whole
            // concatenation. Same declared width, different arithmetic — so at any geometry with
            // more than one head the court returned `ComputationMismatch` for a perfectly honest
            // execution, and `map_refutation_outcome` turns that into `ExecutorGuilty`. Measured on
            // the RC floor (4 heads): all 44 softmax leaves convicted.
            //
            // The chunking is DERIVED from the node's own declared width, not from a constant: a
            // `KvScaled { multiplier }` node is `multiplier` rows of `kv_len` side by side, which is
            // exactly the per-head concatenation. Anything else is one row and one softmax.
            let row = as_i32(&inputs[0]);
            let chunk = base0_kv_chunk_width(node, kv_len, row.len())?;
            let mut acc = Vec::with_capacity(row.len());
            for part in row.chunks(chunk) {
                acc.extend(ops::softmax(part).map_err(shape)?);
            }
            Ok(out(acc))
        }
        Base0Op::Silu => {
            need(1)?;
            Ok(out(ops::silu(&as_i32(&inputs[0]))))
        }
        Base0Op::MulElem => {
            need(2)?;
            Ok(out(ops::mul_elem(&as_i8(&inputs[0])?, &as_i8(&inputs[1])?).map_err(shape)?))
        }
        Base0Op::AddElem => {
            need(2)?;
            Ok(out(ops::add_elem(&as_i8(&inputs[0])?, &as_i8(&inputs[1])?).map_err(shape)?))
        }
        // Embedding is a gather the leg has already opened: the challenged row IS the input, so
        // recomputation is the identity. Naming it in the catalog rather than leaving it
        // uncatalogued is the point — an uncatalogued op is an `Unadjudicable` hole (A4).
        // **The gather, adjudicated (G5d).**
        //
        // This used to return `inputs[0]` — the identity — which is an admission that a real
        // gather could not be checked, and it also forced the node to declare an input row that a
        // pre table has no upstream to supply. Both are gone: the row comes from the registered
        // embedding table at the token's own offset, and the token comes from the carried ids the
        // court has already matched against the job context's `prompt_token_ids_hash`.
        Base0Op::Embed => {
            let (coord, prompt_ids, generated_ids) = gather;
            // **Prefill reads the prompt; decode reads the claim's own generated ids** (ADR-0049
            // Decision E). This used to refuse every decode position outright, and the reason was
            // right: a token pinned by nothing lets a challenger name whatever convicts an honest
            // producer. What changed is that the ids ARE pinned — `output_token_ids_hash_v2` of
            // them is inside `full_logits_trace_root_v2`, which the binding carries, and the caller
            // has already recomputed that root. A decode call `c` consumes the token generated by
            // call `c - 1`.
            let token = if coord.call_index == 0 {
                *prompt_ids.get(coord.position as usize).ok_or(PalwStepRefuteError::Unadjudicable)?
            } else {
                let produced = (coord.call_index as usize).checked_sub(1).ok_or(PalwStepRefuteError::Unadjudicable)?;
                *generated_ids.get(produced).ok_or(PalwStepRefuteError::Unadjudicable)?
            };
            let width = match node.out_len {
                crate::palw_step::PalwStepOutLenV1::Fixed { elements } => elements,
                crate::palw_step::PalwStepOutLenV1::KvScaled { .. } => return Err(PalwStepRefuteError::Unadjudicable),
            };
            let start = (token as u64).checked_mul(width as u64).ok_or(PalwStepRefuteError::Unadjudicable)?;
            let start = u32::try_from(start).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
            let row = weights
                // BASE-0's embedding table is `int8`, so a value is a byte and `width` is both.
                .operand_bytes(node.weight_name.as_str(), layer, start, width)
                .ok_or(PalwStepRefuteError::Unadjudicable)?;
            if row.len() != width as usize {
                return Err(PalwStepRefuteError::Unadjudicable);
            }
            Ok(row.iter().map(|b| *b as i8 as i32 as u32).collect())
        }
        // The three ops whose operands are registration artifacts rather than opened leaves —
        // weight rows, quantization multipliers and the pinned rotary table. They resolve through
        // the weight oracle; a class that has not registered them cannot adjudicate them, which
        // is a coverage question answered at activation, not a silent pass here.
        // ADR-0040 H op 9: one per-tensor (multiplier, shift) pair, the scale change that is
        // allowed to amplify. Its params are a registration artifact, like Requantize's.
        Base0Op::Rescale => {
            need(1)?;
            // Five bytes: one i32 multiplier LE and one u8 shift. Asked for as five rather than
            // as "one element", which is what left op 9 unable to adjudicate through any real
            // opening (ADR-0049 Decision A).
            let row = weights.operand_bytes(node.weight_name.as_str(), layer, 0, 5).ok_or(PalwStepRefuteError::Unadjudicable)?;
            if row.len() != 5 {
                return Err(PalwStepRefuteError::InputSetNotCanonical("base0 rescale params are not 5 bytes"));
            }
            let shift = row[4];
            // Same domain discipline as Requantize: a committed shift outside the op's range is
            // malformed by construction, so the court refuses the step rather than clamping and
            // comparing against arithmetic the specification does not define.
            if shift > crate::palw_base0::RESCALE_MAX_SHIFT {
                return Err(PalwStepRefuteError::InputSetNotCanonical("base0 rescale shift exceeds the 0..=62 domain"));
            }
            let params = ops::ScaleParams { multiplier: i32::from_le_bytes([row[0], row[1], row[2], row[3]]), shift };
            Ok(out(ops::rescale_row(&as_i32(&inputs[0]), params)))
        }
        // MatMul asks the oracle for the WHOLE weight block, `out_dim × in_dim`, taken from the
        // node's declared output width. It used to request `in_dim` elements — one output row's
        // worth — so only a 1-element output could ever be recomputed, and a node producing more
        // failed as `InputSetNotCanonical`, i.e. the CHALLENGER was blamed for the adjudicator's
        // own under-request (re-audit §3.3). A width this side cannot determine, or an oracle that
        // cannot serve it, is `Unadjudicable`: not being able to check is never someone's fault.
        Base0Op::MatMul => {
            need(1)?;
            let x = as_i8(&inputs[0])?;
            if x.is_empty() {
                return Err(PalwStepRefuteError::InputSetNotCanonical("base0 matmul input row is empty"));
            }
            // The width. A `KvScaled` node is `multiplier x kv_len(position)` — the caller
            // derives that length from the coordinate and passes it, so the old refusal ("the
            // adjudicator does not hold it here") no longer describes anything.
            let out_dim = match node.out_len {
                crate::palw_step::PalwStepOutLenV1::Fixed { elements } => elements as usize,
                crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => {
                    let n = (multiplier as u64).checked_mul(kv_len).ok_or(PalwStepRefuteError::Unadjudicable)?;
                    usize::try_from(n).map_err(|_| PalwStepRefuteError::Unadjudicable)?
                }
            };
            if out_dim == 0 {
                return Err(PalwStepRefuteError::InputSetNotCanonical("base0 matmul output width is zero"));
            }
            // **ADR-0049 Decision B: the challenged TILE, not the whole matrix.**
            //
            // This computed `wanted = out_dim * in_len` — every weight the node has — opened all
            // of it and recomputed every output row before the caller compared one tile. Measured:
            // BASE-0's 4096x256 output projection is 1.0 MiB against 16 KiB for the tile actually
            // disputed, and Qwen2.5-1.5B's 151,936x1,536 unembed is ~223 MiB against 192 KiB.
            // ADR-0046 budgets a court close under 152 KB, so the terminal adjudication grew with
            // the model — the one property ADR-0038 W1 promised it would not.
            //
            // Output channel `j` reduces over the whole input row and over weight row `j` ALONE,
            // so a tile of output channels needs a contiguous slice of weight rows and nothing
            // else. That is the whole of the fix.
            let (tile_start, tile_width) = {
                let tile_len = node.tile_len as usize;
                if tile_len == 0 {
                    return Err(PalwStepRefuteError::InputSetNotCanonical("base0 matmul node declares a zero tile length"));
                }
                let start = (gather.0.tile_index as usize).checked_mul(tile_len).ok_or(PalwStepRefuteError::Unadjudicable)?;
                if start >= out_dim {
                    // The coordinate names a tile the node does not have. Malformed refutation,
                    // never a conviction.
                    return Err(PalwStepRefuteError::InputSetNotCanonical("base0 matmul tile index is past the output width"));
                }
                (start, tile_len.min(out_dim - start))
            };
            let wanted = tile_width.checked_mul(x.len()).ok_or(PalwStepRefuteError::Unadjudicable)?;
            let byte_offset = tile_start.checked_mul(x.len()).ok_or(PalwStepRefuteError::Unadjudicable)?;

            // **The second operand: a registered weight, or a second opened row (G5).**
            //
            // ADR-0040 Decision D defines `MatMulQuant` as `i32 acc = Sum(int8 x int8)`, exact,
            // and says nothing about one side being a weight. The adjudicator required one, so
            // attention — where Q.K^T multiplies an activation by the K cache and P.V multiplies
            // probabilities by the V cache — was structurally unadjudicable, and a profile
            // declaring those nodes passed the coverage gate anyway because that gate compares
            // kernel ids and never asks what a kernel can serve.
            //
            // A node that names a weight reads the oracle; a node that names none reads its
            // SECOND canonical input, which the leg has already opened and the Merkle path has
            // already bound. There is no third case: `kernel_can_serve_node_v1` refuses one at
            // registration, so an unadjudicable class cannot be certified.
            // **The two attention contractions are not `[out_dim][in_dim] x row`** (false-conviction
            // fix). A weightless `MatMulQuant` whose second input is a KV sentinel is Q·Kᵀ or P·V,
            // and its second operand is the CACHE — `[position][kv_dim]`, one row per position,
            // which is the layout `canonical_input_leaves_v1` concatenates. Reading that as a
            // `[out_dim][in_dim]` matrix is a transpose: it coincides only at `kv_len == 1`, so
            // position 0 passed and every later position convicted an honest producer.
            //
            // Neither is expressible as `matmul_quant`, because both slice per HEAD with the GQA
            // group mapping the engine uses. They get their own arms, discriminated by the IR's own
            // input refs rather than by slot number.
            let kv_ref = node.input_refs.get(1).copied();
            if node.weight_name.is_empty()
                && matches!(kv_ref, Some(crate::palw_step::PALW_STEP_INPUT_KV_K) | Some(crate::palw_step::PALW_STEP_INPUT_KV_V))
            {
                need(2)?;
                let cache = as_i8(&inputs[1])?;
                let heads = profile.attn_heads as usize;
                let kv_heads = profile.attn_kv_heads as usize;
                let head_dim = profile.attn_head_dim as usize;
                let kv_dim = kv_heads.checked_mul(head_dim).ok_or(PalwStepRefuteError::Unadjudicable)?;
                let history = usize::try_from(kv_len).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
                if heads == 0 || kv_heads == 0 || head_dim == 0 || history == 0 || !heads.is_multiple_of(kv_heads) {
                    return Err(PalwStepRefuteError::InputSetNotCanonical("base0 attention geometry is not expressible"));
                }
                if cache.len() != history.checked_mul(kv_dim).ok_or(PalwStepRefuteError::Unadjudicable)? {
                    return Err(PalwStepRefuteError::InputSetNotCanonical("base0 kv cache operand is not kv_len x kv_dim"));
                }
                // Contiguous grouping, the same mapping the engine uses: query heads
                // `[g·group, (g+1)·group)` read kv head `g`.
                let group = heads / kv_heads;
                let mut acc = Vec::with_capacity(tile_width);
                for out_index in tile_start..tile_start + tile_width {
                    let value = if kv_ref == Some(crate::palw_step::PALW_STEP_INPUT_KV_K) {
                        // Q·Kᵀ: output is `attn_heads x kv_len`, head-major. Lane `(h, j)` is the
                        // dot of head `h`'s query slice with position `j`'s key slice.
                        let (head, j) = (out_index / history, out_index % history);
                        let kv_off = (head / group) * head_dim;
                        let q = x.get(head * head_dim..head * head_dim + head_dim).ok_or(
                            PalwStepRefuteError::InputSetNotCanonical("base0 attention query row is not attn_heads x head_dim"),
                        )?;
                        let k = &cache[j * kv_dim + kv_off..j * kv_dim + kv_off + head_dim];
                        ops::dot_i8(q, k).map_err(shape)?
                    } else {
                        // P·V: output is `attn_heads x head_dim`, head-major. Lane `(h, i)` reduces
                        // head `h`'s probability row against the cache COLUMN at `kv_off + i` —
                        // a stride walk, which is exactly the transpose the old arm did not do.
                        let (head, i) = (out_index / head_dim, out_index % head_dim);
                        let kv_off = (head / group) * head_dim;
                        let probs = x.get(head * history..head * history + history).ok_or(
                            PalwStepRefuteError::InputSetNotCanonical("base0 attention probability row is not attn_heads x kv_len"),
                        )?;
                        let column: Vec<i8> = (0..history).map(|j| cache[j * kv_dim + kv_off + i]).collect();
                        ops::dot_i8(probs, &column).map_err(shape)?
                    };
                    acc.push(value);
                }
                return Ok(out(acc));
            }
            let w: Vec<i8> = if node.weight_name.is_empty() {
                need(2)?;
                let operand = as_i8(&inputs[1])?;
                // The opened row is the whole matrix — it rides the leg, already Merkle-bound, so
                // narrowing WHICH tiles a refutation must open is the leg's decision and not this
                // arm's. What this arm must not do is recompute rows outside the challenged tile,
                // so it takes the same contiguous slice the oracle path opens.
                let full = out_dim.checked_mul(x.len()).ok_or(PalwStepRefuteError::Unadjudicable)?;
                if operand.len() != full {
                    return Err(PalwStepRefuteError::InputSetNotCanonical("base0 matmul operand row is not out_dim x in_dim"));
                }
                operand[byte_offset..byte_offset + wanted].to_vec()
            } else {
                let row = weights
                    .operand_bytes(
                        node.weight_name.as_str(),
                        layer,
                        // `int8` weights: one byte per value, so the tile's row range IS its byte
                        // range. This is the opening that used to be the whole matrix.
                        u32::try_from(byte_offset).map_err(|_| PalwStepRefuteError::Unadjudicable)?,
                        u32::try_from(wanted).map_err(|_| PalwStepRefuteError::Unadjudicable)?,
                    )
                    .ok_or(PalwStepRefuteError::Unadjudicable)?;
                // The oracle served a different amount than the declared shape needs: the class's
                // registration and its weights disagree, which this court cannot resolve either.
                if row.len() != wanted {
                    return Err(PalwStepRefuteError::Unadjudicable);
                }
                row.iter().map(|b| *b as i8).collect()
            };
            Ok(out(ops::matmul_quant(&w, &x, tile_width).map_err(shape)?))
        }
        Base0Op::Requantize | Base0Op::Rope => {
            need(1)?;
            let name = node.weight_name.as_str();
            // The two ops share this site and their parameter blocks do NOT share a width, so the
            // byte count is computed per op before the request. One argument meaning two widths is
            // the defect ADR-0049 Decision A names.
            // **The rotary table is keyed by POSITION, and a rotation is per HEAD** (false-conviction
            // fix). This asked for `8 × row_len/2` bytes at offset 0 — the whole concatenated row's
            // worth of pairs, always from position 0's row.
            //
            // Both halves were wrong and the second hid the first. The inventory stores one row per
            // position, `d_head/2` pairs wide (`rope_row_bytes`), and the engine rotates each head
            // with the SAME position row. So at one head the widths coincided and the court silently
            // rotated by position 0 — convicting every honest step except position 0 — while at more
            // than one head the oversized request simply failed, turning a wrong-answer bug into an
            // `Unadjudicable` that looked like a different problem.
            //
            // The position is `kv_len - 1`: `kv_len` is the candidate's own history length, so the
            // row being produced is the one at its end. That is already in hand, which is why no new
            // coordinate plumbing is needed — and why the decode-call case is right for free, where
            // the coordinate's own `position` field is 0 and the absolute position is not.
            let rope_pairs = (profile.attn_head_dim as usize) / 2;
            let (byte_len, byte_offset) = match op {
                // (multiplier LE, shift, zero LE) per channel.
                Base0Op::Requantize => (9usize.checked_mul(inputs[0].len()), Some(0usize)),
                // cos row then sin row, 4 bytes each, one pair per two lanes — for ONE head.
                Base0Op::Rope => {
                    let per_row = 8usize.checked_mul(rope_pairs);
                    let position = usize::try_from(kv_len.saturating_sub(1)).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
                    (per_row, per_row.and_then(|w| w.checked_mul(position)))
                }
                _ => unreachable!("outer match restricts these three"),
            };
            let byte_len = byte_len.ok_or(PalwStepRefuteError::Unadjudicable)?;
            let byte_offset = byte_offset.ok_or(PalwStepRefuteError::Unadjudicable)?;
            let row = weights
                .operand_bytes(
                    name,
                    layer,
                    u32::try_from(byte_offset).map_err(|_| PalwStepRefuteError::Unadjudicable)?,
                    u32::try_from(byte_len).map_err(|_| PalwStepRefuteError::Unadjudicable)?,
                )
                // **A narrowing is one block for the whole row, or one block per channel — and
                // nothing else** (ADR-0049 Decision A, widened where building a real inventory
                // showed it had to be).
                //
                // Asking only for `9 × channels` made a UNIFORM narrowing uncarryable wherever its
                // row length is not fixed. BASE-0 applies one `qk_to_code` to the softmax output,
                // whose length is `kv_len` — a function of the position — so no registered tensor
                // of any fixed size could ever satisfy the request, and the step was
                // `Unadjudicable` for every real artifact. The two shapes cannot be confused: at
                // one channel they are the same nine bytes, and above one channel a uniform block
                // is nine and a per-channel block is more.
                .or_else(|| {
                    matches!(op, Base0Op::Requantize).then(|| weights.operand_bytes(name, layer, 0, 9)).flatten().map(|uniform| {
                        uniform.iter().copied().cycle().take(byte_len).collect::<Vec<u8>>()
                    })
                })
                .ok_or(PalwStepRefuteError::Unadjudicable)?;
            match op {
                Base0Op::Requantize => {
                    // The oracle row carries (multiplier LE, shift, zero LE) per channel: 9 bytes
                    // each. The zero point is the ADR-0040 amendment (G2) — the class's only
                    // additive registered term, and what makes a projection bias expressible.
                    // Widened from 5 rather than made optional: a length that could mean either
                    // layout is a length two implementations can read differently.
                    if row.len() != 9 * inputs[0].len() {
                        return Err(PalwStepRefuteError::InputSetNotCanonical("base0 requantize params are not 9 bytes per channel"));
                    }
                    // Reject a shift outside the C1 domain (0..=31) as non-canonical rather than
                    // recomputing with it. `rounding_shift_right` now clamps such a shift so it can
                    // never panic, but a step that COMMITTED an out-of-domain shift is malformed by
                    // construction — an honest producer never emits one — so the court must refuse
                    // the step, not silently clamp-and-compare it (mainnet-readiness audit 2.3).
                    if row.chunks_exact(9).any(|c| c[4] > 31) {
                        return Err(PalwStepRefuteError::InputSetNotCanonical("base0 requantize shift exceeds the 0..=31 domain"));
                    }
                    let params: Vec<ops::QuantParams> = row
                        .chunks_exact(9)
                        .map(|c| ops::QuantParams {
                            multiplier: i32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                            shift: c[4],
                            zero: i32::from_le_bytes([c[5], c[6], c[7], c[8]]),
                        })
                        .collect();
                    let q = ops::requantize_row(&as_i32(&inputs[0]), &params).map_err(shape)?;
                    Ok(q.into_iter().map(|v| v as i32 as u32).collect())
                }
                Base0Op::Rope => {
                    // ONE position's row: cos then sin, 4 bytes each, one pair per two lanes of a
                    // HEAD — and it is applied to every head of the row, which is what the engine
                    // does inside its own head loop.
                    if rope_pairs == 0 || row.len() != 8 * rope_pairs {
                        return Err(PalwStepRefuteError::InputSetNotCanonical("base0 rope table is not one head-row of pairs"));
                    }
                    let read = |o: usize| -> Vec<i32> {
                        row[o..o + 4 * rope_pairs].chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
                    };
                    let (cos_q, sin_q) = (read(0), read(4 * rope_pairs));
                    let lanes = as_i32(&inputs[0]);
                    let head_dim = profile.attn_head_dim as usize;
                    if head_dim == 0 || !lanes.len().is_multiple_of(head_dim) {
                        return Err(PalwStepRefuteError::InputSetNotCanonical("base0 rope row is not a whole number of heads"));
                    }
                    let mut acc = Vec::with_capacity(lanes.len());
                    for head in lanes.chunks(head_dim) {
                        acc.extend(ops::rope_table(head, &cos_q, &sin_q).map_err(shape)?);
                    }
                    Ok(out(acc))
                }
                _ => unreachable!("outer match restricts these three"),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwStepRefuteError {
    #[error("step-leg error: {0}")]
    Leg(crate::palw_step_leg::PalwStepLegError),
    #[error(
        "the step's kernel program is not in the catalog (or needs a registration-opaque decoder) — unadjudicable, nobody is slashed"
    )]
    Unadjudicable,
    #[error("the supplied input openings are not the step's canonical input set ({0})")]
    InputSetNotCanonical(&'static str),
    #[error("the weight oracle could not supply a pinned-artifact row")]
    WeightUnavailable,
    #[error("the addressed step recomputes to its committed bytes — refutation rejected")]
    NoFaultFound,
}

impl From<crate::palw_step_leg::PalwStepLegError> for PalwStepRefuteError {
    fn from(e: crate::palw_step_leg::PalwStepLegError) -> Self {
        PalwStepRefuteError::Leg(e)
    }
}

// ---------------------------------------------------------------------------------------------
// The refutation object
// ---------------------------------------------------------------------------------------------

/// One opened input tile (v1: node-output tiles only; KV-chunk and checkpoint-state arms
/// join when their decoders are registration facts).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwStepInputOpeningV1 {
    pub opening: PalwStepOpeningV1,
    pub preimage: PalwStepTileLeafV1,
}

/// ADR-0027 §1's object, with ADR-0030 coordinates: the committed root binding, the
/// challenged output tile, and the canonical inputs.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecutionStepRefutationV1 {
    pub binding: PalwStepBindingV2,
    pub output_opening: PalwStepOpeningV1,
    pub output_preimage: PalwStepTileLeafV1,
    /// MUST be exactly the canonical input set, in canonical order (§ Input integrity).
    pub inputs: Vec<PalwStepInputOpeningV1>,
    /// **The prompt's token ids (G5d), carried because a gather cannot be checked without them.**
    ///
    /// The fault a court reads is "the committed tile differs from the correct computation", and
    /// for an embedding the correct computation is `token_embd[t]`. A challenger may open any row
    /// it likes; that proves fraud only if `t` is the right token. The id is not a step input and
    /// the job context holds only `prompt_token_ids_hash` — so the requirement is irreducible and
    /// the ids ride here, the same proof-carrying shape the operand openings use.
    ///
    /// Checked against that hash before a single one is read: unchecked, a challenger would name
    /// whatever ids convict an honest producer. Empty is legal and means the refutation addresses
    /// no gather — every other kernel ignores it.
    pub prompt_token_ids: Vec<u32>,
    /// **The generated token ids and what pins them (ADR-0049 Decision E).**
    ///
    /// The prompt half above closes the gather at prefill. At a DECODE position the token is
    /// whatever the model produced, so it is in no prompt, and `base0_row`'s `Embed` arm refused
    /// the whole call class rather than let a challenger name it — correctly, because a freely
    /// named token convicts an honest producer. But the refusal makes every class whose canonical
    /// job decodes unadjudicable at a coordinate it reaches, which ADR-0049 Decision D's coverage
    /// now says out loud.
    ///
    /// Nothing new is committed to close it. `output_token_ids_hash_v2` of the generated ids is
    /// ALREADY bound inside `full_logits_trace_root_v2`, and the binding already carries that
    /// root — so carrying the ids beside the trace summary lets the court RECOMPUTE the root and
    /// compare. A challenger that alters one id gets a different root and is refused before a
    /// single id is read, exactly as the prompt half works.
    ///
    /// `None` means the refutation addresses no decode gather, which is every refutation of a
    /// prefill-only class and most refutations of any class.
    pub decode_tokens: Option<PalwDecodeTokenPinV1>,
}

/// What a court needs to recompute `full_logits_trace_root_v2` and so pin the generated tokens.
///
/// The summary's fields are the producer's own declarations, already inside the root; carrying
/// them lets the court check them rather than trust them. `ordered_event_commitment` is opaque to
/// this check — it is a hash the producer committed, and its only job here is to make the
/// recomputation exact.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDecodeTokensV1 {
    pub summary: crate::palw_v2::PalwTraceSummaryV2,
    pub ordered_event_commitment: Hash64,
    pub generated_token_ids: Vec<u32>,
}

/// **The generated ids, pinned through the scheme the class actually committed under — the
/// integer-leg dispatch.**
///
/// `binding.full_logits_trace_root` is one header slot with two occupants: a `Float32` class
/// commits [`crate::palw_v2::full_logits_trace_root_v2`] (an event tree over f32 rows), and the
/// `Int32` class commits [`base0_logits_trace_root_v1`] (a flat keyed hash over its i32 logits
/// rows and generated ids). The court's decode-token check used to recompute ONLY the v2 root, so
/// a BASE-0 execution — whose canonical job decodes — could never carry its generated ids into a
/// refutation, and every decode-call embedding gather ended `Unadjudicable`: 4 of the floor's
/// 914 leaves, pinned by the sweep until this dispatch existed.
///
/// The variant does NOT choose the scheme; the class's registered
/// [`crate::palw_step::PalwStepLaneV1`] does, and the checker refuses a pin that does not speak
/// the class's lane. Derive, never declare (ADR-0046): a challenger picks what to carry, never
/// which rules apply.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwDecodeTokenPinV1 {
    /// A `Float32` class's pin: recompute the v2 event-tree root from the carried summary.
    FloatV2(PalwDecodeTokensV1),
    /// The `Int32` class's pin: recompute [`base0_logits_trace_root_v1`] from the carried rows.
    Base0V1(PalwBase0DecodeTokensV1),
}

/// What a court needs to recompute [`base0_logits_trace_root_v1`] and so pin the generated
/// tokens of an integer-class execution: the logits rows themselves, i32 lanes, one row per
/// call. Affordable to carry whole because the class's vocabulary is small by construction —
/// the same reason its whole trace is one retained object.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBase0DecodeTokensV1 {
    pub logits_rows: Vec<Vec<i32>>,
    pub generated_token_ids: Vec<u32>,
}

/// Domain of [`base0_logits_trace_root_v1`]. Moved here from the producer crate with the byte
/// string unchanged, so every root ever committed under it still verifies.
pub const PALW_BASE0_DOMAIN_LOGITS_TRACE: &[u8] = b"misaka-palw/base0/logits-trace/v1";

/// **The integer class's logits trace root.**
///
/// One keyed hash over the context, the shape of the run, and every logits row the job produced,
/// as `i32` little-endian — the lanes the engine actually computes. Not
/// [`crate::palw_v2::full_logits_trace_root_v2`]: that scheme hashes f32 rows, and BASE-0 has
/// none.
///
/// Lives in the COURT's module and is re-exported by the producer crate, so the committing side
/// and the adjudicating side are one implementation — the correspondence-by-construction shape
/// the material codec already uses.
pub fn base0_logits_trace_root_v1(
    ctx: &crate::palw_v2::PalwJobContextV2,
    logits_rows: &[Vec<i32>],
    generated_token_ids: &[u32],
) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_BASE0_DOMAIN_LOGITS_TRACE).to_state();
    h.update(ctx.context_hash().as_byte_slice());
    h.update(&(ctx.declared_prefill_tokens as u64).to_le_bytes());
    h.update(&(ctx.exact_decode_tokens as u64).to_le_bytes());
    h.update(&(logits_rows.len() as u64).to_le_bytes());
    for row in logits_rows {
        h.update(&(row.len() as u64).to_le_bytes());
        for v in row {
            h.update(&v.to_le_bytes());
        }
    }
    h.update(&(generated_token_ids.len() as u64).to_le_bytes());
    for t in generated_token_ids {
        h.update(&t.to_le_bytes());
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **ADR-0049 Decision E's selection rule, pinned: argmax with ties broken to the LOWEST
/// index.** The engine's decode loop and the court's decode-token adjudication both call this
/// one function; a committed token is refutable exactly because the rule is a thing that can be
/// pointed at rather than an accident of `max_by_key`.
pub fn base0_decode_token_select_v1(values: &[i32]) -> usize {
    let mut best = 0usize;
    for (i, v) in values.iter().enumerate() {
        if *v > values[best] {
            best = i;
        }
    }
    best
}

/// **The Base0 pin, authenticated** — shared by the step-refutation dispatch and the
/// decode-token close. Structural bounds run BEFORE any hashing, so an oversized pin costs a
/// few comparisons to refuse: the row count is the context's own decode count, every row is the
/// registered vocabulary wide, and the ids pair one-to-one with the rows. Then the root is
/// recomputed from the carried material and compared against the binding's committed slot — a
/// challenger that alters one lane gets a different root and is refused before a single id is
/// read.
fn check_base0_decode_pin(binding: &PalwStepBindingV2, pin: &PalwBase0DecodeTokensV1) -> Result<(), PalwStepRefuteError> {
    let decode = binding.job_context.exact_decode_tokens as usize;
    if pin.logits_rows.len() != decode {
        return Err(PalwStepRefuteError::InputSetNotCanonical("the pin's row count is not the context's decode count"));
    }
    if pin.generated_token_ids.len() != decode {
        return Err(PalwStepRefuteError::InputSetNotCanonical("the pin's id count is not the context's decode count"));
    }
    let vocab = binding.shape_profile.vocab_size as usize;
    if pin.logits_rows.iter().any(|row| row.len() != vocab) {
        return Err(PalwStepRefuteError::InputSetNotCanonical("a pinned logits row is not the registered vocabulary wide"));
    }
    let root = base0_logits_trace_root_v1(&binding.job_context, &pin.logits_rows, &pin.generated_token_ids);
    if root != binding.full_logits_trace_root {
        return Err(PalwStepRefuteError::InputSetNotCanonical(
            "the carried logits do not reproduce the claim's own integer trace root",
        ));
    }
    Ok(())
}

/// **ADR-0049 Decision E, the on-chain half: refute a committed decode token.**
///
/// The producer's generated ids are INSIDE [`base0_logits_trace_root_v1`], beside the very rows
/// they were selected from — so a committed token that is not what the selection rule produces
/// from its own row is a lie the commitment itself carries. The challenger carries the pin; the
/// court authenticates it against the claim's committed root, recomputes
/// [`base0_decode_token_select_v1`] over the challenged position's row, and convicts on
/// inequality. One row, one argmax, no opening — the whole-vocabulary cost Decision E refuses to
/// pay is a few kilobytes here, because the integer class's vocabulary is small by construction.
///
/// The float lane has no decode-token adjudicator yet (its per-position openings arrive with the
/// class that needs them, Gate 3); a close against a `Float32` class is refused by name rather
/// than adjudicated wrongly.
///
/// The caller (the court's close arm) has already pinned `binding.committed_execution_root` to
/// the claim's own `execution_root`, which transitively pins `full_logits_trace_root` — the
/// binding recomputation below refuses a binding whose parts do not produce its root.
pub fn check_base0_decode_token_refutation_v1(
    binding: &PalwStepBindingV2,
    pin: &PalwBase0DecodeTokensV1,
    position: u32,
) -> Result<crate::palw_step_leg::PalwStepRefutationVerdictV1, PalwStepRefuteError> {
    crate::palw_step_leg::verify_binding_v1(binding).map_err(PalwStepRefuteError::Leg)?;
    if binding.shape_profile.lane != crate::palw_step::PalwStepLaneV1::Int32 {
        return Err(PalwStepRefuteError::InputSetNotCanonical(
            "the float logits leg has no decode-token adjudicator yet — a Float32 class cannot be closed here",
        ));
    }
    check_base0_decode_pin(binding, pin)?;
    let row = pin
        .logits_rows
        .get(position as usize)
        .ok_or(PalwStepRefuteError::InputSetNotCanonical("the challenged position is outside the job's decode calls"))?;
    let committed = pin.generated_token_ids[position as usize];
    let expected = base0_decode_token_select_v1(row) as u32;
    if committed != expected {
        let fault = crate::palw_step_leg::PalwStepFaultV1::DecodeTokenMismatch { position };
        return Ok(crate::palw_step_leg::PalwStepRefutationVerdictV1 {
            fault,
            evidence_id: crate::palw_step_leg::step_refutation_evidence_id(
                &binding.committed_execution_root,
                PALW_DECODE_TOKEN_EVIDENCE_KIND,
                position as u64,
                fault,
            ),
        });
    }
    Err(PalwStepRefuteError::NoFaultFound)
}

/// The §24.1 evidence-kind of a decode-token refutation. 0–4 are the structural arms
/// ([`crate::palw_step_leg::PalwStepEvidenceV1`]), 5 is the arithmetic step recomputation;
/// this is the sixth way an execution commitment can be caught lying.
pub const PALW_DECODE_TOKEN_EVIDENCE_KIND: u8 = 6;

/// Supplies raw rows of the pinned model artifact. The CALLER owns verifying the artifact
/// digest (`qwen35_pins::GGUF_SHA256`) before answering; adjudication trusts the oracle the
/// way it trusts the pinned GGUF itself.
pub trait PalwWeightOracleV1 {
    /// **`byte_len` raw bytes at `byte_offset` of the named tensor (layer-substituted).**
    ///
    /// Bytes on both sides, with no dtype arithmetic in the oracle (ADR-0049 Decision A). The
    /// parameter used to be `elements`, documented as a count of VALUES while the production
    /// implementation returned that many BYTES — two units under one name, which coincide for
    /// exactly one dtype width. `PALW-BASE-0` is `int8` throughout, so the only class that exists
    /// could not see it, and four call sites had already drifted to four different implicit
    /// widths: 1 byte per value at the embedding gather and the matmul, 4 at the fused norm, 5
    /// for a whole `Rescale` node and 9 per channel for `Requantize`.
    ///
    /// `Rescale` was the one that proved it: it asked for ONE element and required FIVE bytes, so
    /// through a real Merkle opening it received one byte and refused every step it was asked to
    /// adjudicate — while its test passed, because the double returned whole rows regardless of
    /// the size requested.
    ///
    /// Bytes rather than "dtype plus element count" because a Merkle opening already proves bytes:
    /// an oracle that speaks bytes needs no conversion between what it proves and what it returns,
    /// and there is no second place for the conversion to be written differently.
    ///
    /// **An implementation must return exactly `byte_len` bytes or `None`.** Returning more is how
    /// a mismatch stays invisible.
    fn operand_bytes(&self, tensor_name: &str, layer: Option<u16>, byte_offset: u32, byte_len: u32) -> Option<Vec<u8>>;
}

/// A node that holds no model weights, as a production type rather than a test double.
///
/// Every full node is one of these under ADR-0038 W1 — full nodes never run the LLM, and a
/// re-execution oracle exists only where somebody chose to keep the weights. It answers `None` to
/// every row, so every step conviction adjudicates `Unadjudicable`, and that is the CORRECT
/// direction rather than a degradation: a node that cannot check a refutation has not established
/// that the step is wrong, so it must not let the claim void the block's weight.
///
/// A production type because the alternative is a caller inventing one at each site, and the
/// tempting invention is an oracle that answers *something* — zeros, or a default row — which would
/// convict honest producers on every step. `None` is the only safe answer to "I do not have this".
pub struct PalwNoWeightsV1;

impl PalwWeightOracleV1 for PalwNoWeightsV1 {
    fn operand_bytes(&self, _tensor_name: &str, _layer: Option<u16>, _byte_offset: u32, _byte_len: u32) -> Option<Vec<u8>> {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// Canonical input derivation
// ---------------------------------------------------------------------------------------------

/// The positions a kernel needs its inputs at, given the challenged output position.
/// Elementwise/norm/glu kernels read the same `(call, position)`; the GDN recurrence reads
/// every position from the genesis up to and including the output's.
fn required_positions(program: KernelProgram, out: &PalwStepCoordinateV1) -> Vec<(u32, u32)> {
    match program {
        // **The integer recurrence replays from the genesis, exactly as the float one does.** A
        // registered state chunk map later turns this checkpoint-anchored; until then the state is
        // never an opened operand and the replay is the adjudication.
        KernelProgram::GdnCore { .. } | KernelProgram::Qwen36(Qwen36Op::GdnStep) => {
            let mut v = Vec::new();
            // Prefill positions 0..=(p or all), then decode calls 1..=c.
            if out.call_index == 0 {
                for p in 0..=out.position {
                    v.push((0, p));
                }
            } else {
                // All prefill positions precede every decode call.
                // (The caller's profile bounds keep this interval-sized in practice; the
                // genesis anchor makes it prefix-sized on the tiny shapes tests use.)
                v.push((u32::MAX, 0)); // marker replaced below — see expand_prefill
                for c in 1..=out.call_index {
                    v.push((c, 0));
                }
            }
            v
        }
        // The four-tap window: this position and up to three before it, crossing the
        // prefill/decode boundary the same way the recurrence's prefix does. `u32::MAX` markers
        // are expanded by the caller, which holds the prefill length this function does not.
        KernelProgram::Qwen36(Qwen36Op::SsmConv) => {
            if out.call_index == 0 {
                (out.position.saturating_sub(3)..=out.position).map(|p| (0, p)).collect()
            } else {
                // Decode call `c` sits after the whole prefill: the window is the last four of
                // [prefill…, call 1…c]. The marker asks the expander for the prefill TAIL.
                let calls_in_window = out.call_index.min(3);
                let mut v = vec![(u32::MAX, 3 - calls_in_window.min(3))];
                v.extend((out.call_index.saturating_sub(calls_in_window.saturating_sub(1))..=out.call_index).map(|c| (c, 0)));
                v
            }
        }
        _ => vec![(out.call_index, out.position)],
    }
}

/// **The prover's view of the canonical input set** — which leaves a refutation of `coord` must
/// open, in the order it must open them.
///
/// The checker computed this privately and refused any set that differed, so a producer building a
/// refutation had to guess the rule and would be told only "not the canonical one". That is the
/// shape of an evidence format nobody can produce: the verifier existed and the prover did not,
/// which is the same gap `open_artifact_leaf_v1` closed on the artifact tree (audit C-01).
///
/// `None` means the coordinate names a step this checker cannot resolve — the same answer the
/// checker itself gives, so a prover learns it before assembling anything rather than after.
pub fn canonical_input_leaves_v1(
    profile: &crate::palw_step::PalwShapeProfileV3,
    ctx: &crate::palw_v2::PalwJobContextV2,
    coord: &PalwStepCoordinateV1,
) -> Option<Vec<Vec<(u64, PalwStepCoordinateV1)>>> {
    let (node, _) = profile.resolve_node_slot(coord.node_slot)?;
    let program = resolve_kernel(&node.kernel_semantics_id)?;
    canonical_input_leaves(profile, ctx, coord.node_slot, coord, program)
}

/// The canonical input rows for one step: one entry per (required position × input_ref),
/// each listing that node-row's tiles ascending. The flattened order is the canonical
/// opening order; the grouping is what programs consume (whole rows, not tiles). Returns
/// `None` when the wiring uses a sentinel this checker cannot resolve yet (→
/// `Unadjudicable`).
fn canonical_input_leaves(
    profile: &PalwShapeProfileV3,
    context: &crate::palw_v2::PalwJobContextV2,
    out_slot: u32,
    out_coord: &PalwStepCoordinateV1,
    program: KernelProgram,
) -> Option<Vec<Vec<(u64, PalwStepCoordinateV1)>>> {
    let (node, layer) = profile.resolve_node_slot(out_slot)?;
    let table_first_slot = out_slot - intra_table_index(profile, out_slot)? as u32;
    let mut out = Vec::new();
    let mut positions = required_positions(program, out_coord);
    // Expand the prefill markers. `(MAX, 0)` is the WHOLE prefill (the recurrence's prefix);
    // `(MAX, k)` for `k > 0` is its last `k` positions (the conv window's tail).
    if let Some(&(u32::MAX, tail)) = positions.first() {
        let prefill = context.declared_prefill_tokens;
        let from = if tail == 0 { 0 } else { prefill.saturating_sub(tail) };
        let mut expanded: Vec<(u32, u32)> = (from..prefill).map(|p| (0, p)).collect();
        expanded.extend(positions.drain(1..));
        positions = expanded;
    }
    // **The KV arms (G5c).** An attention step reads its query at the CURRENT position and the
    // cached keys or values at EVERY position up to it — so the position set is a property of the
    // input ref, not of the node, which is why a node-wide `required_positions` could not express
    // it and the sentinels were left "registration-opaque".
    //
    // No new leaf format is needed and no float aux series is read: the cache contents are
    // already ordinary step tiles. The K and V projection nodes carry `KCacheWrite` /
    // `VCacheWrite` roles and commit their output at every position, which is what those roles
    // are FOR — the sentinel resolves to whichever node of this layer's table holds the role.
    //
    // The grouping is ref-major here, one concatenated row per input: `MatMulQuant` wants its
    // second operand as a single `out_dim x in_dim` row, and the history is that matrix. The
    // GDN path below keeps its position-major grouping untouched, because `gdn_core` consumes
    // five rows per prior position and reordering them would be a different program.
    use crate::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V};
    if node.input_refs.iter().any(|r| *r == PALW_STEP_INPUT_KV_K || *r == PALW_STEP_INPUT_KV_V) {
        let history: Vec<(u32, u32)> = if out_coord.call_index == 0 {
            (0..=out_coord.position).map(|p| (0, p)).collect()
        } else {
            (0..context.declared_prefill_tokens).map(|p| (0, p)).chain((1..=out_coord.call_index).map(|c| (c, 0))).collect()
        };
        for &r in &node.input_refs {
            let (in_slot, positions_for_ref) = match r {
                PALW_STEP_INPUT_KV_K | PALW_STEP_INPUT_KV_V => {
                    let want = if r == PALW_STEP_INPUT_KV_K {
                        crate::palw_step::PalwStepNodeRoleV1::KCacheWrite
                    } else {
                        crate::palw_step::PalwStepNodeRoleV1::VCacheWrite
                    };
                    let table = profile.layer_table(layer?);
                    // Exactly one node may hold each cache role: two would make "the K cache"
                    // ambiguous, and a court that had to choose would be choosing the evidence.
                    let mut found = table.iter().enumerate().filter(|(_, n)| n.role == want);
                    let (idx, _) = found.next()?;
                    if found.next().is_some() {
                        return None;
                    }
                    (table_first_slot + idx as u32, history.clone())
                }
                PALW_STEP_INPUT_LAYER_IN => {
                    if table_first_slot == 0 {
                        return None;
                    }
                    (table_first_slot - 1, vec![(out_coord.call_index, out_coord.position)])
                }
                _ if r >= PALW_STEP_INPUT_SENTINEL_MIN => return None,
                _ => (table_first_slot + r as u32, vec![(out_coord.call_index, out_coord.position)]),
            };
            let (in_node, _) = profile.resolve_node_slot(in_slot)?;
            let mut row = Vec::new();
            for (call, pos) in positions_for_ref {
                let kv_len = if call == 0 { pos as u64 + 1 } else { context.declared_prefill_tokens as u64 + call as u64 };
                let len = match in_node.out_len {
                    crate::palw_step::PalwStepOutLenV1::Fixed { elements } => elements as u64,
                    crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * kv_len,
                };
                for t in 0..len.div_ceil(in_node.tile_len as u64) as u32 {
                    let coord = PalwStepCoordinateV1 { call_index: call, node_slot: in_slot, position: pos, tile_index: t };
                    row.push((canonical_step_leaf_index(profile, context, &coord)?, coord));
                }
            }
            out.push(row);
        }
        return Some(out);
    }

    for &(call, pos) in &positions {
        for &r in &node.input_refs {
            let in_slot = if r >= PALW_STEP_INPUT_SENTINEL_MIN {
                match r {
                    PALW_STEP_INPUT_LAYER_IN => {
                        if table_first_slot == 0 {
                            return None; // the pre table has no upstream — unadjudicable wiring
                        }
                        table_first_slot - 1
                    }
                    _ => return None, // the checkpoint arm: registration-opaque today
                }
            } else {
                table_first_slot + r as u32
            };
            let (in_node, _) = profile.resolve_node_slot(in_slot)?;
            let kv_len = if call == 0 { pos as u64 + 1 } else { context.declared_prefill_tokens as u64 + call as u64 };
            let len = match in_node.out_len {
                crate::palw_step::PalwStepOutLenV1::Fixed { elements } => elements as u64,
                crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * kv_len,
            };
            let tiles = len.div_ceil(in_node.tile_len as u64) as u32;
            let mut row = Vec::with_capacity(tiles as usize);
            for t in 0..tiles {
                let coord = PalwStepCoordinateV1 { call_index: call, node_slot: in_slot, position: pos, tile_index: t };
                let idx = canonical_step_leaf_index(profile, context, &coord)?;
                row.push((idx, coord));
            }
            out.push(row);
        }
    }
    Some(out)
}

/// The node's index within its own table (pre/layer/post).
fn intra_table_index(profile: &PalwShapeProfileV3, slot: u32) -> Option<usize> {
    let mut cursor = slot as usize;
    if cursor < profile.pre_nodes.len() {
        return Some(cursor);
    }
    cursor -= profile.pre_nodes.len();
    for layer in 0..profile.layer_count {
        let n = match profile.layer_kind(layer) {
            crate::palw_step::PalwLayerKindV1::GatedDeltaNet => profile.gdn_nodes.len(),
            crate::palw_step::PalwLayerKindV1::Attention => profile.attn_nodes.len(),
        };
        if cursor < n {
            return Some(cursor);
        }
        cursor -= n;
    }
    if cursor < profile.post_nodes.len() { Some(cursor) } else { None }
}

// ---------------------------------------------------------------------------------------------
// The adjudicator
// ---------------------------------------------------------------------------------------------

/// Adjudicates one arithmetic step refutation. Structural faults in the opened output leaf
/// convict structurally (they subsume); an honest recomputation returns `NoFaultFound`.
pub fn check_execution_step_refutation_v1(
    refutation: &PalwExecutionStepRefutationV1,
    weights: &dyn PalwWeightOracleV1,
) -> Result<PalwStepRefutationVerdictV1, PalwStepRefuteError> {
    use crate::palw_step_leg::{PalwStepEvidenceV1, PalwStepRefutationV1, check_step_refutation_v1};

    let binding = &refutation.binding;
    // 1) Structural pass on the output leaf: a structurally-faulty leaf convicts without
    //    arithmetic; a structurally-honest one falls through to recomputation.
    let structural = PalwStepRefutationV1 {
        binding: binding.clone(),
        evidence: PalwStepEvidenceV1::StepTile {
            opening: refutation.output_opening.clone(),
            preimage: refutation.output_preimage.clone(),
        },
    };
    match check_step_refutation_v1(&structural) {
        Ok(verdict) => return Ok(verdict),                              // structural conviction subsumes
        Err(crate::palw_step_leg::PalwStepLegError::NoFaultFound) => {} // structurally honest
        Err(e) => return Err(e.into()),
    }

    let context_hash = binding.job_context.context_hash();
    let profile_hash = binding.shape_profile.shape_profile_id();
    let out_coord = refutation.output_preimage.coord;
    let (node, layer) = binding.shape_profile.resolve_node_slot(out_coord.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let program = resolve_kernel(&node.kernel_semantics_id).ok_or(PalwStepRefuteError::Unadjudicable)?;

    // 2) Canonical input set: exact leaves, exact order, all verified against the tree.
    let required = canonical_input_leaves(&binding.shape_profile, &binding.job_context, out_coord.node_slot, &out_coord, program)
        .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let required_flat: Vec<&(u64, PalwStepCoordinateV1)> = required.iter().flatten().collect();
    if refutation.inputs.len() != required_flat.len() {
        return Err(PalwStepRefuteError::InputSetNotCanonical("input count differs from the canonical set"));
    }
    for (supplied, (want_idx, want_coord)) in refutation.inputs.iter().zip(required_flat.iter().copied()) {
        if supplied.opening.leaf_index != *want_idx || supplied.preimage.coord != *want_coord {
            return Err(PalwStepRefuteError::InputSetNotCanonical("input leaf is not the canonical one"));
        }
        let implied = step_opening_root_v1(binding.step_leaf_count, &supplied.opening)?;
        if implied != binding.step_merkle_root {
            return Err(PalwStepRefuteError::Leg(crate::palw_step_leg::PalwStepLegError::CommittedRootMismatch));
        }
        if step_tile_leaf_hash_v1(&context_hash, &profile_hash, &supplied.preimage) != supplied.opening.leaf_hash {
            return Err(PalwStepRefuteError::Leg(crate::palw_step_leg::PalwStepLegError::LeafPreimageMismatch { leaf: "input tile" }));
        }
        if supplied.preimage.values_le.len() != 4 * supplied.preimage.value_count as usize {
            return Err(PalwStepRefuteError::InputSetNotCanonical("input bytes are not 4 per value"));
        }
    }

    // 3) Recompute the node's full output row(s) for the challenged position: concatenate
    //    each logical input row's tiles back into one row.
    let mut inputs: Vec<Vec<u32>> = Vec::with_capacity(required.len());
    let mut cursor = 0usize;
    for row_tiles in &required {
        let mut row = Vec::new();
        for _ in row_tiles {
            let supplied = &refutation.inputs[cursor];
            row.extend(supplied.preimage.values_le.chunks_exact(4).map(|q| u32::from_le_bytes([q[0], q[1], q[2], q[3]])));
            cursor += 1;
        }
        inputs.push(row);
    }
    // The TRUE kv length of the challenged position — never the padded cache length (ADR-0030
    // Fact 17). Prefill position `p` sees `p + 1` keys; decode call `c` sees `P + c`. The same
    // derivation `canonical_tile_values` uses, so the width the adjudicator recomputes at is the
    // width the leg committed at.
    let kv_len = if out_coord.call_index == 0 {
        out_coord.position as u64 + 1
    } else {
        binding.job_context.declared_prefill_tokens as u64 + out_coord.call_index as u64
    };
    // **G5d: the carried ids are checked BEFORE any of them is read.** Unchecked, a challenger
    // would name whatever ids convict an honest producer — the ids are the whole basis on which a
    // gather's "correct" output is decided. An empty list is legal (the refutation addresses no
    // gather); a non-empty one must be the prompt the job context committed to.
    if !refutation.prompt_token_ids.is_empty()
        && crate::palw_v2::prompt_token_ids_hash_v2(&refutation.prompt_token_ids) != binding.job_context.prompt_token_ids_hash
    {
        return Err(PalwStepRefuteError::InputSetNotCanonical("the carried prompt ids are not the ones the job context commits to"));
    }
    // **The decode half (ADR-0049 Decision E), checked the same way and before anything reads
    // it — dispatched on the class's registered LANE, because `full_logits_trace_root` is one
    // slot with two occupants.** A `Float32` class committed the v2 event-tree root; the `Int32`
    // class committed `base0_logits_trace_root_v1`. Recomputing the root the class actually
    // committed IS the check — no new commitment, and a challenger who alters one id produces a
    // different root. A pin that does not speak the class's lane is refused by name: the
    // challenger picks what to carry, never which rules apply.
    let generated: &[u32] = match (binding.shape_profile.lane, refutation.decode_tokens.as_ref()) {
        (_, None) => &[],
        (crate::palw_step::PalwStepLaneV1::Float32, Some(PalwDecodeTokenPinV1::FloatV2(d))) => {
            if crate::palw_v2::output_token_ids_hash_v2(&d.generated_token_ids) != d.summary.output_token_ids_hash {
                return Err(PalwStepRefuteError::InputSetNotCanonical(
                    "the carried generated ids are not the ones the summary commits to",
                ));
            }
            let root = crate::palw_v2::full_logits_trace_root_v2(&context_hash, &d.summary, &d.ordered_event_commitment);
            if root != binding.full_logits_trace_root {
                return Err(PalwStepRefuteError::InputSetNotCanonical(
                    "the carried trace summary does not reproduce the claim's own logits trace root",
                ));
            }
            &d.generated_token_ids
        }
        (crate::palw_step::PalwStepLaneV1::Int32, Some(PalwDecodeTokenPinV1::Base0V1(d))) => {
            check_base0_decode_pin(binding, d)?;
            &d.generated_token_ids
        }
        _ => return Err(PalwStepRefuteError::InputSetNotCanonical("the decode-token pin does not speak the class's lane")),
    };
    let (recomputed_row, row_offset) = run_program(
        program,
        node,
        layer,
        &binding.shape_profile,
        &inputs,
        weights,
        kv_len,
        (&out_coord, &refutation.prompt_token_ids, generated),
    )?;

    // 4) Compare the challenged tile's slice, exact bits.
    let tile_start = out_coord.tile_index as usize * node.tile_len as usize;
    let committed: Vec<u32> =
        refutation.output_preimage.values_le.chunks_exact(4).map(|q| u32::from_le_bytes([q[0], q[1], q[2], q[3]])).collect();
    // `run_program` says where its slice starts, so a kernel that recomputed only the tile is not
    // sliced a second time (ADR-0049 Decision B). Whole-row kernels report 0 and behave as before.
    let local_start = tile_start.checked_sub(row_offset).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let recomputed = recomputed_row
        .get(local_start..local_start + committed.len())
        .ok_or(PalwStepRefuteError::InputSetNotCanonical("recomputed row is shorter than the tile claims"))?;
    if let Some(i) = recomputed.iter().zip(committed.iter()).position(|(a, b)| a != b) {
        let fault = PalwStepFaultV1::ComputationMismatch { value_index: i as u32 };
        return Ok(PalwStepRefutationVerdictV1 {
            fault,
            evidence_id: crate::palw_step_leg::step_refutation_evidence_id(
                &binding.committed_execution_root,
                5,
                refutation.output_opening.leaf_index,
                fault,
            ),
        });
    }
    Err(PalwStepRefuteError::NoFaultFound)
}

// ---------------------------------------------------------------------------------------------
// The programs
// ---------------------------------------------------------------------------------------------

const ONE_F32: u32 = 0x3F80_0000;

/// Runs one kernel program over the resolved inputs, producing the node's full output row
/// at the challenged position (concatenated head-major where heads exist).
fn run_program(
    program: KernelProgram,
    node: &crate::palw_step::PalwStepNodeV1,
    layer: Option<u16>,
    profile: &PalwShapeProfileV3,
    inputs: &[Vec<u32>],
    weights: &dyn PalwWeightOracleV1,
    // The TRUE kv length of the challenged position (G5). The adjudicator used to refuse every
    // `KvScaled` node for not holding this, while its own caller derives it from the coordinate
    // it already has — so attention was unadjudicable for want of a value one frame up.
    kv_len: u64,
    // G5d: the challenged position and the carried, hash-checked prompt ids. Only the gather
    // reads them; every other kernel is a function of its opened rows and its weights.
    gather: (&PalwStepCoordinateV1, &[u32], &[u32]),
    // **The index in the node's output row at which the returned slice begins** (ADR-0049
    // Decision B). Every kernel but the BASE-0 matmul recomputes the whole row and returns it at
    // offset 0; the matmul opens only the challenged tile's weight rows, so it can only return
    // that tile and says so here rather than leaving the caller to assume.
) -> Result<(Vec<u32>, usize), PalwStepRefuteError> {
    // Only the BASE-0 matmul narrows what it computes to the challenged tile; every other kernel
    // is elementwise or a whole-row reduction and returns the row it recomputed, at offset 0.
    let row_offset = match program {
        KernelProgram::Base0(Base0Op::MatMul)
        | KernelProgram::Qwen36(Qwen36Op::MatMulRequant)
        | KernelProgram::Qwen36(Qwen36Op::MatMulRescale)
        | KernelProgram::Qwen36(Qwen36Op::MatMulGrouped)
        | KernelProgram::Qwen36(Qwen36Op::MatMulGroupedWide) => (gather.0.tile_index as usize).saturating_mul(node.tile_len as usize),
        _ => 0,
    };
    let row = match program {
        KernelProgram::Base0(op) => base0_row(op, node, layer, profile, inputs, weights, kv_len, gather),
        KernelProgram::Qwen36(op) => qwen36_row(op, node, layer, profile, inputs, weights, kv_len, gather),
        KernelProgram::L2Norm => {
            let x = inputs.first().ok_or(PalwStepRefuteError::InputSetNotCanonical("l2norm needs one input row"))?;
            Ok(l2_norm_row(x, profile.l2_eps_bits))
        }
        KernelProgram::RmsNormFused => {
            let x = inputs.first().ok_or(PalwStepRefuteError::InputSetNotCanonical("rmsnorm needs one input row"))?;
            // Four bytes per value: the fused norm's gain is an f32 lane.
            let wrow = weights
                .operand_bytes(
                    &node.weight_name,
                    layer,
                    0,
                    u32::try_from(x.len() * 4).map_err(|_| PalwStepRefuteError::WeightUnavailable)?,
                )
                .ok_or(PalwStepRefuteError::WeightUnavailable)?;
            if wrow.len() != x.len() * 4 {
                return Err(PalwStepRefuteError::WeightUnavailable);
            }
            let w: Vec<u32> = wrow.chunks_exact(4).map(|q| u32::from_le_bytes([q[0], q[1], q[2], q[3]])).collect();
            Ok(rms_norm_fused_row(x, &w, profile.rms_eps_bits))
        }
        KernelProgram::Swiglu => {
            let (gate, up) = match inputs {
                [g, u] if g.len() == u.len() => (g, u),
                _ => return Err(PalwStepRefuteError::InputSetNotCanonical("swiglu needs gate and up rows of one length")),
            };
            Ok(gate.iter().zip(up.iter()).map(|(&g, &u)| ref_mul_v1(ggml_v_silu_v1(g), u)).collect())
        }
        KernelProgram::SigmoidGlibcFma => {
            let x = inputs.first().ok_or(PalwStepRefuteError::InputSetNotCanonical("sigmoid needs one input row"))?;
            Ok(x.iter().map(|&v| crate::palw_transcendental::ggml_sigmoid_v1(v, true)).collect())
        }
        KernelProgram::SoftplusGlibcFma => {
            let x = inputs.first().ok_or(PalwStepRefuteError::InputSetNotCanonical("softplus needs one input row"))?;
            Ok(x.iter().map(|&v| crate::palw_transcendental::ggml_softplus_v1(v, true)).collect())
        }
        KernelProgram::GdnCore { dot } => {
            // The wiring supplies FULL node rows; g/β nodes may be wider than the head count
            // (row padding is a wiring fact) — the recurrence consumes the first `heads`.
            let heads = profile.gdn_heads as usize;
            if !inputs.len().is_multiple_of(5) || inputs.is_empty() {
                return Err(PalwStepRefuteError::InputSetNotCanonical("gdn expects 5 input rows per position"));
            }
            let narrowed: Vec<Vec<u32>> = inputs
                .chunks_exact(5)
                .flat_map(|c| {
                    let g = c[3].get(..heads).map(<[u32]>::to_vec);
                    let bta = c[4].get(..heads).map(<[u32]>::to_vec);
                    match (g, bta) {
                        (Some(g), Some(bta)) => vec![c[0].clone(), c[1].clone(), c[2].clone(), g, bta],
                        _ => Vec::new(),
                    }
                })
                .collect();
            if !narrowed.len().is_multiple_of(5) || narrowed.len() != inputs.len() {
                return Err(PalwStepRefuteError::InputSetNotCanonical("gdn gate rows shorter than the head count"));
            }
            gdn_core_genesis_replay(profile, &narrowed, dot)
        }
    }?;
    Ok((row, row_offset))
}

/// `l2_norm` (ops.cpp): double sum of squares ascending, `scale = 1/max(sqrtf(sum), eps)`,
/// then per-element f32 multiply. Per GGML the row is normalized per contiguous head row —
/// callers pass one such row.
fn l2_norm_row(x: &[u32], eps_bits: u32) -> Vec<u32> {
    let mut sum = 0u64; // +0.0 f64
    for &v in x {
        let w = ref_widen_f32_to_f64_v2(v);
        sum = ref64_add_v2(sum, ref64_mul_v2(w, w));
    }
    let sum32 = ref_narrow_f64_to_f32_v2(sum);
    let root = ref_sqrt_v2(sum32);
    // fmaxf(root, eps): ordered max, eps positive.
    let denom = if f32_gt_bits(eps_bits, root) { eps_bits } else { root };
    let scale = ref_div_v2(ONE_F32, denom);
    x.iter().map(|&v| ref_mul_v1(v, scale)).collect()
}

/// `rms_norm` fused with the weight multiply (the one CPU fusion): double sum ascending,
/// `mean = sum/n` (double divide, narrowed), `scale = 1/sqrtf(mean + eps)`, then
/// `(x·scale)·w` in that association.
fn rms_norm_fused_row(x: &[u32], w: &[u32], eps_bits: u32) -> Vec<u32> {
    let mut sum = 0u64;
    for &v in x {
        let wide = ref_widen_f32_to_f64_v2(v);
        sum = ref64_add_v2(sum, ref64_mul_v2(wide, wide));
    }
    let n = i64_to_f64_bits(x.len() as i64);
    let mean = ref_narrow_f64_to_f32_v2(ref64_div_v2(sum, n));
    let scale = ref_div_v2(ONE_F32, ref_sqrt_v2(ref_add_v1(mean, eps_bits)));
    x.iter().zip(w.iter()).map(|(&v, &wi)| ref_mul_v1(ref_mul_v1(v, scale), wi)).collect()
}

fn i64_to_f64_bits(k: i64) -> u64 {
    debug_assert!(k > 0 && k < (1 << 53));
    let mag = k as u64;
    let shift = mag.leading_zeros() - 11;
    let sig = mag << shift;
    ((1075 - shift as i32) as u64) << 52 | (sig & 0x000F_FFFF_FFFF_FFFF)
}

fn f32_gt_bits(a: u32, b: u32) -> bool {
    // Both operands here are non-NaN by construction (eps is a constant; sqrt output of a
    // finite sum is non-NaN or the comparison is moot and total order suffices).
    let key = |x: u32| -> i64 {
        let mag = (x & 0x7FFF_FFFF) as i64;
        if x & 0x8000_0000 != 0 { -mag } else { mag }
    };
    key(a) > key(b)
}

/// The class's `ggml_vec_dot_f32`: 4 accumulator vectors, lane-strided element assignment,
/// the frozen fold (x0+=x2, x1+=x3, x0+=x1) and the arch's final lane reduce. `n` must be a
/// multiple of the step (128 is, for both).
fn vec_dot_f32(a: &[u32], b: &[u32], dot: DotStructure) -> u32 {
    let (step, epr) = match dot {
        DotStructure::Step16Epr4 => (16usize, 4usize),
        DotStructure::Step32Epr8 => (32usize, 8usize),
    };
    debug_assert_eq!(a.len(), b.len());
    debug_assert!(a.len().is_multiple_of(step));
    // acc[j].lane[l]
    let mut acc = vec![vec![0u32; epr]; 4];
    let mut i = 0;
    while i < a.len() {
        for (j, accj) in acc.iter_mut().enumerate() {
            for (l, lane) in accj.iter_mut().enumerate() {
                let idx = i + j * epr + l;
                *lane = ref_fma_v2(a[idx], b[idx], *lane);
            }
        }
        i += step;
    }
    // Fold: x0+=x2, x1+=x3, x0+=x1 (per lane). Indexed form kept: it mirrors the macro
    // being transcribed, and the cross-row reads make an iterator form less literal.
    #[allow(clippy::needless_range_loop)]
    for l in 0..epr {
        let a02 = ref_add_v1(acc[0][l], acc[2][l]);
        let a13 = ref_add_v1(acc[1][l], acc[3][l]);
        acc[0][l] = ref_add_v1(a02, a13);
    }
    match dot {
        DotStructure::Step16Epr4 => {
            // vaddvq: (l0+l1) + (l2+l3).
            ref_add_v1(ref_add_v1(acc[0][0], acc[0][1]), ref_add_v1(acc[0][2], acc[0][3]))
        }
        DotStructure::Step32Epr8 => {
            // low128 + high128 per lane, then two hadds: ((t0+t1)+(t2+t3)).
            let t: Vec<u32> = (0..4).map(|l| ref_add_v1(acc[0][l], acc[0][l + 4])).collect();
            ref_add_v1(ref_add_v1(t[0], t[1]), ref_add_v1(t[2], t[3]))
        }
    }
}

/// The fused GDN recurrence (ops.cpp:10735-10945), genesis-anchored: state starts zero, and
/// every position from the sequence start to the challenged one is replayed for ALL heads
/// (the committed output row is head-major and one tile can straddle heads).
///
/// Per position, inputs arrive as the profile wiring's node rows in this pinned order:
/// `[q_row, k_row, v_row, g_row, beta_row]` — q/k/v are `heads × head_dim` head-major,
/// g/beta are per-head scalars. Per (position, head): decay `S *= expf(g)` (elementwise),
/// `sum_j = dot(S_col_j, k)`, `delta_j = (v_j − sum_j)·β`, `S[i][j] += k_i·delta_j` (fused),
/// `out_j = dot(S_col_j, q) · (1/sqrtf(head_k_dim))` — the scale on the OUTPUT (the fused
/// path's placement; the unfused graph scales q, a different rounding).
fn gdn_core_genesis_replay(
    profile: &PalwShapeProfileV3,
    inputs: &[Vec<u32>],
    dot: DotStructure,
) -> Result<Vec<u32>, PalwStepRefuteError> {
    let heads = profile.gdn_heads as usize;
    let kd = profile.gdn_head_k_dim as usize;
    let vd = profile.gdn_head_v_dim as usize;
    if kd == 0 || vd == 0 || heads == 0 || !kd.is_multiple_of(16) {
        return Err(PalwStepRefuteError::Unadjudicable);
    }
    if !inputs.len().is_multiple_of(5) || inputs.is_empty() {
        return Err(PalwStepRefuteError::InputSetNotCanonical("gdn expects 5 input rows per position"));
    }
    let positions = inputs.len() / 5;
    let scale = ref_div_v2(ONE_F32, ref_sqrt_v2(i32_len_to_f32_bits(kd as u32)));
    // state[h][j*kd + i] = S[i][j] (the kernel's transposed buffer layout).
    let mut state = vec![vec![0u32; kd * vd]; heads];
    let mut out_row = vec![0u32; heads * vd];
    for t in 0..positions {
        let q_row = &inputs[5 * t];
        let k_row = &inputs[5 * t + 1];
        let v_row = &inputs[5 * t + 2];
        let g_row = &inputs[5 * t + 3];
        let b_row = &inputs[5 * t + 4];
        if q_row.len() != heads * kd || k_row.len() != heads * kd || v_row.len() != heads * vd {
            return Err(PalwStepRefuteError::InputSetNotCanonical("gdn q/k/v row lengths"));
        }
        if g_row.len() != heads || b_row.len() != heads {
            return Err(PalwStepRefuteError::InputSetNotCanonical("gdn gate row lengths"));
        }
        for h in 0..heads {
            let q = &q_row[h * kd..(h + 1) * kd];
            let k = &k_row[h * kd..(h + 1) * kd];
            let v = &v_row[h * vd..(h + 1) * vd];
            let decay = glibc_expf_v1(g_row[h], true);
            let beta = b_row[h];
            let s = &mut state[h];
            for slot in s.iter_mut() {
                *slot = ref_mul_v1(*slot, decay);
            }
            for j in 0..vd {
                let sum = vec_dot_f32(&s[j * kd..(j + 1) * kd], k, dot);
                let delta = ref_mul_v1(ref_sub_v1(v[j], sum), beta);
                for i in 0..kd {
                    s[j * kd + i] = ref_fma_v2(k[i], delta, s[j * kd + i]);
                }
            }
            if t == positions - 1 {
                for j in 0..vd {
                    let sum = vec_dot_f32(&s[j * kd..(j + 1) * kd], q, dot);
                    out_row[h * vd + j] = ref_mul_v1(sum, scale);
                }
            }
        }
    }
    Ok(out_row)
}

fn i32_len_to_f32_bits(n: u32) -> u32 {
    // Small positive integers are exact in f32 for our head dims.
    debug_assert!(n > 0 && n <= 1 << 24);
    let shift = n.leading_zeros() - 8;
    let sig = n << shift;
    ((150 - shift as i32) as u32) << 23 | (sig & 0x007F_FFFF)
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
pub(crate) mod tests {
    /// No gather: the challenged coordinate is the graph's origin and no prompt ids are carried.
    /// Every kernel but `Embed` ignores it, and `Embed` has its own tests.
    const NO_GATHER: (&PalwStepCoordinateV1, &[u32], &[u32]) =
        (&PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 }, &[], &[]);

    use super::*;
    use crate::palw_legs::PalwCheckpointProfileV1;
    use crate::palw_step::{
        PALW_STEP_OBJECT_VERSION_V1, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1,
        canonical_step_coordinates, kv_aux_leaf_count, step_leaf_count,
    };
    use crate::palw_step_leg::{
        PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepLegBuilderV1, checkpoint_empty_root_v2, checkpoint_leg_root_v2,
        execution_commitment_root_v2, step_leg_root_v1, step_opening_v1,
    };
    use crate::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2};

    fn h64(fill: u8) -> Hash64 {
        Hash64::from_bytes([fill; 64])
    }

    struct NoWeights;
    impl PalwWeightOracleV1 for NoWeights {
        fn operand_bytes(&self, _t: &str, _l: Option<u16>, _o: u32, _n: u32) -> Option<Vec<u8>> {
            None
        }
    }

    /// Serves one fixed byte row, so a test can hand `base0_row` exactly the bytes an on-chain
    /// weight oracle would have committed.
    struct FixedRow(Vec<u8>);
    impl PalwWeightOracleV1 for FixedRow {
        /// **Honours `byte_len`, because a double that does not cannot witness a size defect.**
        /// This returned the whole row regardless of what was asked for, which is why op 9 asking
        /// for one byte and requiring five had a passing test (ADR-0049 Decision A).
        fn operand_bytes(&self, _t: &str, _l: Option<u16>, o: u32, n: u32) -> Option<Vec<u8>> {
            let (o, n) = (o as usize, n as usize);
            (self.0.len() >= o + n).then(|| self.0[o..o + n].to_vec())
        }
    }

    /// A node for the BASE-0 requantize path. `base0_row` dispatches on its `op` ARGUMENT and reads
    /// only `weight_name` off the node for this op, so the remaining fields are shape formality.
    fn requantize_node() -> PalwStepNodeV1 {
        PalwStepNodeV1 {
            op_kind: PalwStepOpKindV1::Scale,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: "blk.{layer}.requant".to_string(),
            weight_dtypes: Vec::new(),
            out_len: PalwStepOutLenV1::Fixed { elements: 4 },
            tile_len: 4,
            kernel_semantics_id: h64(0),
            input_refs: vec![0],
        }
    }

    /// **§3.3: `RmsNorm` is adjudicated with the CLASS's epsilon, not a constant.**
    ///
    /// The court recomputed with a hardcoded `eps = 1`, so every honest producer of a class
    /// registered with any other epsilon was convicted on every norm step. The epsilon now comes
    /// from the registered shape profile, which `shape_profile_id` binds.
    #[test]
    fn rms_norm_is_adjudicated_with_the_registered_epsilon() {
        let mut node = requantize_node();
        node.weight_name = String::new();
        let input: Vec<Vec<u32>> = vec![vec![10i32 as u32, (-20i32) as u32, 30, (-40i32) as u32]];
        let x: Vec<i8> = vec![10, -20, 30, -40];

        // A class registered with a large epsilon must be adjudicated with THAT epsilon.
        let mut big = profile();
        big.base0_rms_eps_q = 1 << 30;
        let got = base0_row(Base0Op::RmsNorm, &node, Some(0), &big, &input, &NoWeights, 1, NO_GATHER).expect("adjudicable");
        let want: Vec<u32> = crate::palw_base0_ops::rms_norm(&x, 1 << 30).unwrap().into_iter().map(|v| v as u32).collect();
        assert_eq!(got, want, "the registered epsilon must be the one used");

        // ...and it must NOT equal the hardcoded-1 recompute, or the class's epsilon is decorative
        // and an honest producer under `big` would be convicted.
        let with_one: Vec<u32> = crate::palw_base0_ops::rms_norm(&x, 1).unwrap().into_iter().map(|v| v as u32).collect();
        assert_ne!(got, with_one, "eps must actually change the adjudicated result");

        // A different registered epsilon is a different class identity, so the two cannot collide.
        let mut small = profile();
        small.base0_rms_eps_q = 1 << 8;
        assert_ne!(big.shape_profile_id(), small.shape_profile_id(), "the epsilon is inside the class id");
    }
    /// **Op 9 adjudicating through the PRODUCTION oracle, which it had never done.**
    ///
    /// `rescale_is_adjudicable_and_bounded` below proves the arithmetic, and proves it through
    /// `FixedRow` — a double that used to return whole rows regardless of the size requested. The
    /// arm asked for ONE element and required FIVE bytes, and `PalwProvenOperandsV1` returned
    /// `elements` bytes, so through a real Merkle opening `Rescale` received one byte and refused
    /// every step it was handed. Coverage still reported the kernel catalogued, because coverage
    /// compares ids.
    ///
    /// This is the same step, served by the oracle a full node actually holds: an opening verified
    /// against a registered `artifact_root`, no local model, no double. It is the test ADR-0049
    /// Decision A exists to make possible.
    #[test]
    fn op_nine_adjudicates_through_a_real_artifact_opening() {
        use crate::palw_artifact::{
            PalwArtifactOpeningV1, PalwArtifactOperandV1, PalwProvenOperandsV1, artifact_leaf_v1, artifact_root_v1,
        };

        let mut node = requantize_node();
        node.weight_name = "blk.{layer}.scale".to_string();
        let acc: Vec<i32> = vec![1_000, -2_000, 3_000, -4_000];
        let input: Vec<Vec<u32>> = vec![acc.iter().map(|v| *v as u32).collect()];

        // The five bytes a `Rescale` node's parameters are: one i32 multiplier LE, one u8 shift.
        let mut bytes = i32::MAX.to_le_bytes().to_vec();
        bytes.push(23);
        let operand = PalwArtifactOperandV1 { tensor_name: "blk.{layer}.scale".to_string(), layer: Some(0), row_start: 0, bytes };
        let leaf = artifact_leaf_v1(&operand);
        let root = artifact_root_v1(&[leaf]).expect("a one-leaf inventory has a root");
        let opening = PalwArtifactOpeningV1 { operand, leaf_index: 0, leaf_count: 1, path: vec![] };
        let oracle = PalwProvenOperandsV1::from_openings_v1(&[opening], root).expect("the opening verifies against its root");

        let got = base0_row(Base0Op::Rescale, &node, Some(0), &profile(), &input, &oracle, 1, NO_GATHER)
            .expect("op 9 adjudicates through a proven operand");
        let want: Vec<u32> =
            crate::palw_base0_ops::rescale_row(&acc, crate::palw_base0_ops::ScaleParams { multiplier: i32::MAX, shift: 23 })
                .into_iter()
                .map(|v| v as u32)
                .collect();
        assert_eq!(got, want, "the court recomputes the step from carried evidence alone");

        // And a node whose parameters nobody opened stays Unadjudicable — no proof, no conviction.
        let mut unopened = node.clone();
        unopened.weight_name = "blk.{layer}.never_registered".to_string();
        assert_eq!(
            base0_row(Base0Op::Rescale, &unopened, Some(0), &profile(), &input, &oracle, 1, NO_GATHER),
            Err(PalwStepRefuteError::Unadjudicable),
            "an operand nobody proved leaves the step unchecked rather than decided"
        );
    }

    /// ADR-0040 H op 9 is adjudicable, and a committed shift outside its domain is refused.
    ///
    /// `Rescale` was missing from the catalog entirely while Decision H recorded that the other
    /// nine cannot compute without it — so the one op that makes the class work was the one the
    /// court could not adjudicate, an `Unadjudicable` hole at the centre of the graph.
    #[test]
    fn rescale_is_adjudicable_and_bounded() {
        let mut node = requantize_node();
        node.weight_name = "blk.{layer}.scale".to_string();
        let acc: Vec<i32> = vec![1_000, -2_000, 3_000, -4_000];
        let input: Vec<Vec<u32>> = vec![acc.iter().map(|v| *v as u32).collect()];

        // One per-tensor (multiplier, shift): a gain of 2^8 at shift 23.
        let mut row = i32::MAX.to_le_bytes().to_vec();
        row.push(23);
        let got = base0_row(Base0Op::Rescale, &node, Some(0), &profile(), &input, &FixedRow(row), 1, NO_GATHER).expect("adjudicable");
        let want: Vec<u32> =
            crate::palw_base0_ops::rescale_row(&acc, crate::palw_base0_ops::ScaleParams { multiplier: i32::MAX, shift: 23 })
                .into_iter()
                .map(|v| v as u32)
                .collect();
        assert_eq!(got, want, "op 9 must recompute through the catalog op");

        // A shift past the op's domain is malformed by construction: refuse the step rather than
        // clamp-and-compare against arithmetic the specification does not define.
        for bad in [crate::palw_base0::RESCALE_MAX_SHIFT + 1, 100, 255] {
            let mut row = i32::MAX.to_le_bytes().to_vec();
            row.push(bad);
            assert!(
                matches!(
                    base0_row(Base0Op::Rescale, &node, Some(0), &profile(), &input, &FixedRow(row), 1, NO_GATHER),
                    Err(PalwStepRefuteError::InputSetNotCanonical(_))
                ),
                "shift {bad} is outside the 0..=62 domain and must be refused"
            );
        }

        // The descriptor is in the catalog, so the coverage gate sees it.
        assert!(catalogued_kernel_ids_v1().contains(&kernel_semantics_id_v1(KDESC_BASE0_RESCALE)));
    }

    /// **§3.3: `MatMulQuant` asks the oracle for the WHOLE weight block.**
    ///
    /// It used to request one input row's worth, so `out_dim` was always 1 and any node producing a
    /// wider row failed as `InputSetNotCanonical` — the CHALLENGER blamed for the adjudicator's own
    /// under-request. A width this side cannot determine, or an oracle that cannot serve it, is
    /// `Unadjudicable`.
    #[test]
    fn matmul_recomputes_the_whole_output_row() {
        let mut node = requantize_node();
        node.weight_name = "blk.{layer}.w".to_string();
        node.out_len = PalwStepOutLenV1::Fixed { elements: 3 };
        let x: Vec<i8> = vec![1, 2, 3, 4];
        let input: Vec<Vec<u32>> = vec![x.iter().map(|v| *v as i32 as u32).collect()];

        // 3 output rows × 4 inputs = 12 weight bytes.
        let w: Vec<i8> = (1..=12i8).collect();
        let row: Vec<u8> = w.iter().map(|v| *v as u8).collect();
        let got = base0_row(Base0Op::MatMul, &node, Some(0), &profile(), &input, &FixedRow(row), 1, NO_GATHER).expect("adjudicable");
        let want: Vec<u32> = crate::palw_base0_ops::matmul_quant(&w, &x, 3).unwrap().into_iter().map(|v| v as u32).collect();
        assert_eq!(got.len(), 3, "the full declared output row is recomputed, not one element");
        assert_eq!(got, want);

        // An oracle that serves the OLD amount (one row's worth) can no longer be mistaken for a
        // valid block: it is a class/registration disagreement this court cannot resolve.
        let short: Vec<u8> = w[..4].iter().map(|v| *v as u8).collect();
        assert_eq!(
            base0_row(Base0Op::MatMul, &node, Some(0), &profile(), &input, &FixedRow(short), 1, NO_GATHER),
            Err(PalwStepRefuteError::Unadjudicable),
            "a short weight block is unadjudicable, never the challenger's fault"
        );

        // **A kv-scaled width IS determinable here, and this assertion used to say otherwise.**
        //
        // It read "a kv-scaled width is not determinable here, so it is refused rather than
        // guessed" and passed — but not for that reason. `out_dim` is `multiplier x kv_len` and the
        // caller passes `kv_len`, which the arm's own comment records ("the old refusal … no longer
        // describes anything"). What produced the refusal was `FixedRow` serving twelve bytes when
        // the step asked for eight, so the length check rejected an oracle that had over-answered.
        // With the double honouring `byte_len` (ADR-0049 Decision A) the over-answer is gone and
        // the step adjudicates, which is correct.
        let mut kv = node.clone();
        kv.out_len = PalwStepOutLenV1::KvScaled { multiplier: 2 };
        let full: Vec<u8> = w.iter().map(|v| *v as u8).collect();
        let kv_got = base0_row(Base0Op::MatMul, &kv, Some(0), &profile(), &input, &FixedRow(full.clone()), 1, NO_GATHER)
            .expect("multiplier x kv_len is a width this arm holds");
        let kv_want: Vec<u32> = crate::palw_base0_ops::matmul_quant(&w[..8], &x, 2).unwrap().into_iter().map(|v| v as u32).collect();
        assert_eq!(kv_got, kv_want, "a kv-scaled node recomputes multiplier x kv_len output rows");

        // What IS still refused: a width the oracle cannot serve. Not being able to check is never
        // the challenger's fault, so it is `Unadjudicable` rather than a conviction or an acquittal.
        let mut wide = node.clone();
        wide.out_len = PalwStepOutLenV1::KvScaled { multiplier: 9 };
        assert_eq!(
            base0_row(Base0Op::MatMul, &wide, Some(0), &profile(), &input, &FixedRow(full), 1, NO_GATHER),
            Err(PalwStepRefuteError::Unadjudicable),
            "an oracle that cannot serve the declared width leaves the step unchecked, not decided"
        );
    }

    /// Audit §2.3: the court REFUSES a step whose committed requantize shift is outside ADR-0040
    /// C1's `0..=31` domain, instead of recomputing with it.
    ///
    /// `rounding_shift_right` clamps such a shift so it can never panic, but clamping is a release
    /// safety net, not an adjudication rule: a step that committed an out-of-domain shift is
    /// malformed by construction (no honest producer emits one), and comparing a clamped recompute
    /// against it would let a malformed step be *adjudicated* — convicting or acquitting on
    /// arithmetic the specification does not define. An adversarial verifier replaced this reject
    /// with `if false` and found every suite still green, so this is the coverage.
    #[test]
    fn a_requantize_shift_outside_the_domain_is_refused_not_recomputed() {
        let node = requantize_node();
        let input: Vec<Vec<u32>> = vec![vec![1_000i32 as u32, 2_000, 3_000, 4_000]];

        // 9 bytes per channel: multiplier LE, shift, zero LE (the ADR-0040 amendment's additive
        // term). In-domain shift (=10) must adjudicate.
        let mut ok_row = Vec::new();
        for _ in 0..4 {
            ok_row.extend_from_slice(&i32::MAX.to_le_bytes());
            ok_row.push(10);
            ok_row.extend_from_slice(&0i32.to_le_bytes());
        }
        let got = base0_row(Base0Op::Requantize, &node, Some(0), &profile(), &input, &FixedRow(ok_row), 1, NO_GATHER);
        assert!(got.is_ok(), "an in-domain shift must still be recomputed: {got:?}");

        // Out-of-domain shifts are refused as non-canonical — including 32, the first one past the
        // domain edge, and 255, the largest a byte can carry.
        for bad in [32u8, 63, 64, 200, 255] {
            let mut row = Vec::new();
            for channel in 0..4 {
                row.extend_from_slice(&i32::MAX.to_le_bytes());
                // Only ONE channel is out of domain: the check must scan every chunk, not just the
                // first, or a malformed shift hides behind three well-formed neighbours.
                row.push(if channel == 3 { bad } else { 10 });
                row.extend_from_slice(&0i32.to_le_bytes());
            }
            let refused = base0_row(Base0Op::Requantize, &node, Some(0), &profile(), &input, &FixedRow(row), 1, NO_GATHER);
            assert!(
                matches!(refused, Err(PalwStepRefuteError::InputSetNotCanonical(_))),
                "shift {bad} in the last channel must be refused as non-canonical, got {refused:?}"
            );
        }
    }

    /// A pure-GDN profile: pre = embed(one row feeding everything), one GDN layer whose
    /// nodes are the five wiring inputs then the core. Geometry: 2 heads × k16 × v16.
    fn profile() -> PalwShapeProfileV3 {
        let mk = |kind, elements, desc: &str, refs: Vec<u16>| PalwStepNodeV1 {
            op_kind: kind,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: String::new(),
            weight_dtypes: Vec::new(),
            out_len: PalwStepOutLenV1::Fixed { elements },
            tile_len: 16,
            kernel_semantics_id: kernel_semantics_id_v1(desc),
            input_refs: refs,
        };
        PalwShapeProfileV3 {
            version: PALW_STEP_OBJECT_VERSION_V1,
            lane: crate::palw_step::PalwStepLaneV1::Float32,
            layer_count: 1,
            full_attention_interval: 0, // pure recurrent
            hidden_dim: 32,
            ffn_dim: 32,
            attn_heads: 1,
            attn_kv_heads: 1,
            attn_head_dim: 16,
            rope_dims: 2,
            rope_sections: [1, 1, 0, 0],
            rope_freq_base_bits: 0x4CBE_BC20,
            rms_eps_bits: 0x3583_37BD,
            base0_rms_eps_q: 1 << 8,
            l2_eps_bits: 0x3583_37BD,
            gdn_heads: 2,
            gdn_head_k_dim: 16,
            gdn_head_v_dim: 16,
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
            pre_nodes: vec![mk(PalwStepOpKindV1::EmbedLookup, 32, "unimplemented/embed", vec![])],
            gdn_nodes: vec![
                // Five wiring rows (schema exercise: sourced from the layer input through
                // implemented elementwise kernels so the whole chain is adjudicable).
                mk(PalwStepOpKindV1::L2Norm, 32, KDESC_L2_NORM, vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN]), // q
                mk(PalwStepOpKindV1::L2Norm, 32, KDESC_L2_NORM, vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN]), // k
                mk(PalwStepOpKindV1::Sigmoid, 32, KDESC_SIGMOID_GLIBC_FMA, vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN]), // v
                mk(PalwStepOpKindV1::Softplus, 16, KDESC_SOFTPLUS_GLIBC_FMA, vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN]), // g (2 heads → 2, padded to tile-able 16)
                mk(PalwStepOpKindV1::Sigmoid, 16, KDESC_SIGMOID_GLIBC_FMA, vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN]),   // beta
                mk(PalwStepOpKindV1::GatedDeltaNet, 32, KDESC_GDN_CORE_NEON, vec![0, 1, 2, 3, 4]),
            ],
            attn_nodes: vec![],
            post_nodes: vec![mk(PalwStepOpKindV1::Silu, 32, "unimplemented/logits", vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN])],
            reference_ruleset_id: h64(0x22),
            transcendental_bindings: vec![],
            contraction_facts: vec![],
            kv_chunk_calls: 0,
            state_chunk_map_id: h64(0x44),
        }
    }

    fn context() -> PalwJobContextV2 {
        let mut ctx = PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"step-refute-test".to_vec(),
            job_id: h64(1),
            job_nullifier: h64(2),
            assignment_id: h64(3),
            execution_seed: [7; 32],
            model_profile_id: h64(4),
            runtime_manifest_hash: h64(5),
            runtime_class_id: h64(6),
            // Placeholder; the fixtures below overwrite it with the profile they actually carry,
            // because honest material declares the profile it was produced under.
            shape_profile_id: h64(7),
            trace_scheme_id: h64(8),
            cu_ruleset_id: h64(9),
            tokenizer_id: h64(10),
            prompt_token_ids_hash: h64(11),
            declared_prefill_tokens: 2,
            exact_decode_tokens: 2,
            max_context_tokens: 64,
        };
        ctx.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
        ctx
    }

    /// Deterministic pseudo-values in a tame numeric range (bit patterns of ~[-2, 2]).
    fn val(seed: u32) -> u32 {
        let m = seed.wrapping_mul(0x9E37_79B9) >> 9;
        0x3F00_0000 | (m & 0x007F_FFFF) | ((seed & 1) << 31)
    }

    /// Executes the profile's graph HONESTLY with the catalog programs, committing every
    /// node-output tile; returns the binding plus retained material.
    /// A structurally real refutation with NO openings — enough for callers that exercise the
    /// authorship half and the gating around adjudication, and deliberately not adjudicable, so a
    /// test using it cannot accidentally assert a conviction it did not prove.
    pub(crate) fn skeleton_refutation() -> PalwExecutionStepRefutationV1 {
        let (binding, _material, _rows) = honest_execution();
        PalwExecutionStepRefutationV1 {
            binding,
            output_opening: PalwStepOpeningV1 { leaf_index: 0, leaf_hash: h64(0x01), siblings: Vec::new() },
            output_preimage: PalwStepTileLeafV1 {
                version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                coord: PalwStepCoordinateV1 { node_slot: 0, call_index: 0, position: 0, tile_index: 0 },
                value_count: 0,
                values_le: Vec::new(),
            },
            inputs: Vec::new(),
            prompt_token_ids: Vec::new(),
            decode_tokens: None,
        }
    }

    fn honest_execution() -> (PalwStepBindingV2, crate::palw_step_leg::PalwStepLegMaterialV1, Vec<Vec<Vec<u32>>>) {
        let p = profile();
        // Honest material declares the profile it was produced under (the equality the step-leg
        // verifier enforces).
        let mut ctx = context();
        ctx.shape_profile_id = p.shape_profile_id();
        // rows[position_ordinal][node_slot] = full output row. Position ordinals: prefill
        // p0, p1, then decode call 1.
        let positions: Vec<(u32, u32)> = vec![(0, 0), (0, 1), (1, 0)];
        let slots = p.global_node_count();
        let mut rows: Vec<Vec<Vec<u32>>> = vec![vec![Vec::new(); slots as usize]; positions.len()];
        for (ord, &(_call, _pos)) in positions.iter().enumerate() {
            // pre: embed row — arbitrary but deterministic and distinct per position.
            rows[ord][0] = (0..32).map(|i| val(ord as u32 * 100 + i)).collect();
            // gdn nodes 1..=5 (slots 1..=5): elementwise from the layer input (slot 0).
            let layer_in = rows[ord][0].clone();
            rows[ord][1] = l2_norm_row(&layer_in, p.l2_eps_bits);
            rows[ord][2] = l2_norm_row(&layer_in, p.l2_eps_bits);
            rows[ord][3] = layer_in.iter().map(|&v| crate::palw_transcendental::ggml_sigmoid_v1(v, true)).collect();
            rows[ord][4] = layer_in.iter().take(16).map(|&v| crate::palw_transcendental::ggml_softplus_v1(v, true)).collect();
            rows[ord][5] = layer_in.iter().take(16).map(|&v| crate::palw_transcendental::ggml_sigmoid_v1(v, true)).collect();
            // gdn core (slot 6): genesis replay over ordinals 0..=ord.
            let mut gdn_inputs: Vec<Vec<u32>> = Vec::new();
            for row in rows.iter().take(ord + 1) {
                gdn_inputs.push(row[1].clone());
                gdn_inputs.push(row[2].clone());
                gdn_inputs.push(row[3].clone());
                gdn_inputs.push(row[4][..2].to_vec()); // 2 heads — hmm, see note below
                gdn_inputs.push(row[5][..2].to_vec());
            }
            // The checker resolves g/beta rows as the FULL node rows (16 wide); mirror that.
            let mut gdn_inputs_full: Vec<Vec<u32>> = Vec::new();
            for row in rows.iter().take(ord + 1) {
                gdn_inputs_full.push(row[1].clone());
                gdn_inputs_full.push(row[2].clone());
                gdn_inputs_full.push(row[3].clone());
                gdn_inputs_full.push(row[4].clone());
                gdn_inputs_full.push(row[5].clone());
            }
            let _ = gdn_inputs;
            rows[ord][6] = gdn_core_from_wide_rows(&p, &gdn_inputs_full).unwrap();
            // post (slot 7): logits-only positions — computed where committed (p1, decode).
            rows[ord][7] = rows[ord][6].iter().map(|&v| crate::palw_transcendental::ggml_v_silu_v1(v)).collect();
        }
        // Commit every tile in canonical order.
        let mut b = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).unwrap();
        let main = b.expected_main_leaves();
        for i in 0..main {
            let coord = canonical_step_coordinates(&p, &ctx, i).unwrap();
            let ord = match (coord.call_index, coord.position) {
                (0, 0) => 0usize,
                (0, 1) => 1,
                (1, 0) => 2,
                _ => unreachable!(),
            };
            let row = &rows[ord][coord.node_slot as usize];
            let start = coord.tile_index as usize * 16;
            let end = (start + 16).min(row.len());
            b.push_step_tile(coord, &row[start..end]).unwrap();
        }
        let material = b.finish().unwrap();
        // Assemble the composite (no checkpoints for this shape: decode_calls 1, interval 8).
        let ctx_hash = ctx.context_hash();
        let profile_hash = p.shape_profile_id();
        let ckpt_profile = PalwCheckpointProfileV1 {
            version: crate::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
            checkpoint_interval: 8,
            state_layout_id: h64(0x55),
        };
        let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, material.leaf_count, &material.merkle_root);
        let ckpt_root =
            checkpoint_leg_root_v2(&ctx_hash, &ckpt_profile.profile_hash(), &h64(0x44), 1, 0, &checkpoint_empty_root_v2(&ctx_hash));
        let committed = execution_commitment_root_v2(&ctx_hash, &h64(0xAA), &h64(0xBB), &ckpt_root, &step_root);
        let binding = PalwStepBindingV2 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            job_context: ctx,
            shape_profile: p,
            checkpoint_profile: ckpt_profile,
            state_chunk_map_id: h64(0x44),
            full_logits_trace_root: h64(0xAA),
            activation_leg_root: h64(0xBB),
            step_leaf_count: material.leaf_count,
            step_merkle_root: material.merkle_root,
            checkpoint_count: 0,
            checkpoint_merkle_root: checkpoint_empty_root_v2(&ctx_hash),
            committed_execution_root: committed,
        };
        (binding, material, rows)
    }

    /// The checker's view of GDN inputs: full node rows (g/beta 16-wide with only the first
    /// `heads` entries meaningful). Mirror of `run_program`'s slicing so the honest world
    /// and the adjudicator agree byte-for-byte.
    fn gdn_core_from_wide_rows(p: &PalwShapeProfileV3, inputs: &[Vec<u32>]) -> Result<Vec<u32>, PalwStepRefuteError> {
        let narrowed: Vec<Vec<u32>> = inputs
            .chunks_exact(5)
            .flat_map(|c| {
                vec![
                    c[0].clone(),
                    c[1].clone(),
                    c[2].clone(),
                    c[3][..p.gdn_heads as usize].to_vec(),
                    c[4][..p.gdn_heads as usize].to_vec(),
                ]
            })
            .collect();
        gdn_core_genesis_replay(p, &narrowed, DotStructure::Step16Epr4)
    }

    // ---- P0-8's end-to-end fixture: a BASE-0 MatMul execution, real weights, no model ----

    /// A BASE-0 profile whose layer is one `MatMulQuant` node over a registered weight tensor.
    ///
    /// BASE-0 rather than the float `profile()` above, and that is the point of the fixture: the
    /// float classes' matmuls are `Q4_K`/`Q5_K`/`Q6_K` and `KERNEL_CATALOG` has no adjudicator
    /// for any of them, so a float matmul conviction cannot be built at all today. BASE-0's
    /// `i8 x i8 -> i32` matmul is exact and IS in the catalog, which is the whole reason that
    /// class exists.
    pub(crate) fn base0_matmul_profile() -> PalwShapeProfileV3 {
        let mut p = profile();
        // The lane that made this fixture possible at all: BASE-0 commits int32 codes, and the
        // float finiteness rule rejects every integer in `[-8_388_608, -1]`.
        p.lane = crate::palw_step::PalwStepLaneV1::Int32;
        p.gdn_nodes = vec![PalwStepNodeV1 {
            op_kind: PalwStepOpKindV1::MatMulQuant,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: "blk.{layer}.w".to_string(),
            // One byte, because this profile's GDN table covers its one layer.
            weight_dtypes: vec![24],
            out_len: crate::palw_step::PalwStepOutLenV1::Fixed { elements: 32 },
            tile_len: 16,
            kernel_semantics_id: kernel_semantics_id_v1(KDESC_BASE0_MATMUL),
            input_refs: vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN],
        }];
        p.post_nodes = vec![PalwStepNodeV1 {
            op_kind: PalwStepOpKindV1::Silu,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: String::new(),
            weight_dtypes: Vec::new(),
            out_len: crate::palw_step::PalwStepOutLenV1::Fixed { elements: 32 },
            tile_len: 16,
            kernel_semantics_id: kernel_semantics_id_v1(KDESC_BASE0_SILU),
            input_refs: vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN],
        }];
        p.pre_nodes = vec![PalwStepNodeV1 {
            op_kind: PalwStepOpKindV1::EmbedLookup,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: String::new(),
            weight_dtypes: Vec::new(),
            out_len: crate::palw_step::PalwStepOutLenV1::Fixed { elements: 32 },
            tile_len: 16,
            kernel_semantics_id: kernel_semantics_id_v1(KDESC_BASE0_EMBED),
            input_refs: vec![],
        }];
        p
    }

    /// The weight block the class registered: `32 out x 32 in` int8 codes, deterministic.
    pub(crate) fn base0_matmul_weights() -> Vec<i8> {
        (0..32 * 32).map(|i| (((i * 7) % 13) - 6) as i8).collect()
    }

    /// An honest BASE-0 execution over [`base0_matmul_profile`], plus its leg material.
    ///
    /// Same three position ordinals the GDN fixture uses. Every value stays inside the int8 lane,
    /// because BASE-0 activations are int8 codes riding i32 lanes and an out-of-range lane is
    /// `InputSetNotCanonical` rather than arithmetic.
    pub(crate) fn base0_honest_execution() -> (PalwStepBindingV2, crate::palw_step_leg::PalwStepLegMaterialV1, Vec<Vec<Vec<u32>>>) {
        let p = base0_matmul_profile();
        let mut ctx = context();
        ctx.shape_profile_id = p.shape_profile_id();
        let w = base0_matmul_weights();
        let slots = p.global_node_count();
        let mut rows: Vec<Vec<Vec<u32>>> = vec![vec![Vec::new(); slots as usize]; 3];
        for (ord, row) in rows.iter_mut().enumerate().take(3) {
            // pre (slot 0): the embedding row, int8 codes.
            row[0] = (0..32).map(|i| (((ord * 5 + i) % 11) as i32 - 5) as u32).collect();
            let x: Vec<i8> = row[0].iter().map(|v| *v as i32 as i8).collect();
            // gdn (slot 1): the matmul, by the SAME function the court will recompute with.
            row[1] = crate::palw_base0_ops::matmul_quant(&w, &x, 32).unwrap().into_iter().map(|v| v as u32).collect();
            // post (slot 2): silu over the layer output.
            row[2] = crate::palw_base0_ops::silu(&row[1].iter().map(|v| *v as i32).collect::<Vec<_>>())
                .into_iter()
                .map(|v| v as u32)
                .collect();
        }
        let mut b = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).unwrap();
        for i in 0..b.expected_main_leaves() {
            let coord = canonical_step_coordinates(&p, &ctx, i).unwrap();
            let ord = match (coord.call_index, coord.position) {
                (0, 0) => 0usize,
                (0, 1) => 1,
                (1, 0) => 2,
                _ => unreachable!(),
            };
            let row = &rows[ord][coord.node_slot as usize];
            let start = coord.tile_index as usize * 16;
            let end = (start + 16).min(row.len());
            b.push_step_tile(coord, &row[start..end]).unwrap();
        }
        let material = b.finish().unwrap();
        let ctx_hash = ctx.context_hash();
        let profile_hash = p.shape_profile_id();
        let ckpt_profile = PalwCheckpointProfileV1 {
            version: crate::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
            checkpoint_interval: 8,
            state_layout_id: h64(0x55),
        };
        let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, material.leaf_count, &material.merkle_root);
        let ckpt_root =
            checkpoint_leg_root_v2(&ctx_hash, &ckpt_profile.profile_hash(), &h64(0x44), 1, 0, &checkpoint_empty_root_v2(&ctx_hash));
        let committed = execution_commitment_root_v2(&ctx_hash, &h64(0xAA), &h64(0xBB), &ckpt_root, &step_root);
        let binding = PalwStepBindingV2 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            job_context: ctx,
            shape_profile: p,
            checkpoint_profile: ckpt_profile,
            state_chunk_map_id: h64(0x44),
            full_logits_trace_root: h64(0xAA),
            activation_leg_root: h64(0xBB),
            step_leaf_count: material.leaf_count,
            step_merkle_root: material.merkle_root,
            checkpoint_count: 0,
            checkpoint_merkle_root: checkpoint_empty_root_v2(&ctx_hash),
            committed_execution_root: committed,
        };
        (binding, material, rows)
    }

    /// An HONEST BASE-0 execution and the artifact openings a challenger would carry against it.
    ///
    /// The mirror of [`base0_matmul_fraud`]: same profile, same weights, same coordinate — and
    /// nothing corrupted. What a court does with this is the whole false-slash question.
    pub(crate) fn base0_honest_case() -> (PalwExecutionStepRefutationV1, Vec<crate::palw_artifact::PalwArtifactOpeningV1>, Hash64) {
        use crate::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1};
        let (binding, material, rows) = base0_honest_execution();
        let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 1 };
        let refutation = build_refutation(&binding, &material, &rows, coord);
        // **ADR-0049 Decision B: the CHALLENGED TILE's weight rows, not the matrix.** The
        // coordinate is tile 1 of a 16-wide tiling over a 32x32 matmul, so the step reduces over
        // output rows 16..32 and nothing else — bytes 512..1024. Half the matrix at this fixture's
        // size; one part in 1,187 of Qwen2.5-1.5B's unembed, which is the difference between a
        // court close that fits a transaction and one that does not.
        const IN_LEN: usize = 32;
        const TILE_START_ROW: usize = 16;
        const TILE_WIDTH: usize = 16;
        let all_weights = base0_matmul_weights();
        let byte_offset = TILE_START_ROW * IN_LEN;
        let operands = [
            PalwArtifactOperandV1 {
                tensor_name: "blk.{layer}.w".to_string(),
                layer: Some(0),
                row_start: byte_offset as u32,
                bytes: all_weights[byte_offset..byte_offset + TILE_WIDTH * IN_LEN].iter().map(|v| *v as u8).collect(),
            },
            PalwArtifactOperandV1 { tensor_name: "decoy".to_string(), layer: None, row_start: 0, bytes: vec![9, 9, 9] },
        ];
        let leaves: Vec<Hash64> = operands.iter().map(artifact_leaf_v1).collect();
        let artifact_root = artifact_root_v1(&leaves).unwrap();
        let openings = vec![crate::palw_artifact::PalwArtifactOpeningV1 {
            operand: operands[0].clone(),
            leaf_index: 0,
            leaf_count: leaves.len() as u32,
            path: vec![leaves[1]],
        }];
        (refutation, openings, artifact_root)
    }

    /// **Condition 11, at the arithmetic layer: an honest step is NoFault, not a conviction.**
    ///
    /// The conviction test says the court can catch fraud. This says it does not catch anything
    /// else — which is the harder half, because a court that convicts on every challenge would
    /// pass every fraud test ever written.
    #[test]
    fn palw_v2_an_honest_matmul_is_not_a_fault() {
        use crate::palw_artifact::PalwProvenOperandsV1;
        let (refutation, openings, artifact_root) = base0_honest_case();
        let operands = PalwProvenOperandsV1::from_openings_v1(&openings, artifact_root).expect("the openings prove");
        assert_eq!(
            check_execution_step_refutation_v1(&refutation, &operands),
            Err(PalwStepRefuteError::NoFaultFound),
            "an honest step must be NoFaultFound — recomputed and found correct, not merely unchecked"
        );

        // And it is NoFault because the court RECOMPUTED it: with no weights the same refutation
        // is `Unadjudicable`, a different answer entirely. Without this, "not convicted" could
        // mean "never checked".
        assert_eq!(
            check_execution_step_refutation_v1(&refutation, &NoWeights),
            Err(PalwStepRefuteError::Unadjudicable),
            "not-convicted and not-checked must be distinguishable"
        );
    }

    /// The same execution with ONE committed MatMul value corrupted, plus the artifact openings a
    /// challenger carries. Returns everything a `CourtClosed` proof needs.
    pub(crate) fn base0_matmul_fraud() -> (PalwExecutionStepRefutationV1, Vec<crate::palw_artifact::PalwArtifactOpeningV1>, Hash64) {
        use crate::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1};
        let (mut binding, mut material, rows) = base0_honest_execution();
        let p = binding.shape_profile.clone();
        let ctx = binding.job_context.clone();
        let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 1 };
        let idx = canonical_step_leaf_index(&p, &ctx, &coord).unwrap();
        let ctx_hash = ctx.context_hash();
        let profile_hash = p.shape_profile_id();

        // The miner's world: the committed matmul output is off by one at value 3 of tile 1.
        let mut row = rows[2][1].clone();
        row[16 + 3] = (row[16 + 3] as i32).wrapping_add(1) as u32;
        let leaf = PalwStepTileLeafV1 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            coord,
            value_count: 16,
            values_le: row[16..32].iter().flat_map(|v| v.to_le_bytes()).collect(),
        };
        material.leaf_hashes[idx as usize] = step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &leaf);
        let merkle = crate::palw_step_leg::step_merkle_root_v1(&material.leaf_hashes).unwrap();
        material.merkle_root = merkle;
        binding.step_merkle_root = merkle;
        let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, binding.step_leaf_count, &merkle);
        let ckpt_root = checkpoint_leg_root_v2(
            &ctx_hash,
            &binding.checkpoint_profile.profile_hash(),
            &binding.state_chunk_map_id,
            1,
            0,
            &binding.checkpoint_merkle_root,
        );
        binding.committed_execution_root = execution_commitment_root_v2(
            &ctx_hash,
            &binding.full_logits_trace_root,
            &binding.activation_leg_root,
            &ckpt_root,
            &step_root,
        );

        let mut refutation = build_refutation(&binding, &material, &rows, coord);
        refutation.output_preimage = leaf;
        refutation.output_opening = step_opening_v1(&material.leaf_hashes, idx).unwrap();

        // The class's registered artifact inventory, and the opening that proves this weight
        // block belongs to it. This is the whole "no model" mechanism: the court never reads a
        // GGUF, it reads bytes a Merkle path binds to the root the class registered.
        // **ADR-0049 Decision B: the CHALLENGED TILE's weight rows, not the matrix.** The
        // coordinate is tile 1 of a 16-wide tiling over a 32x32 matmul, so the step reduces over
        // output rows 16..32 and nothing else — bytes 512..1024. Half the matrix at this fixture's
        // size; one part in 1,187 of Qwen2.5-1.5B's unembed, which is the difference between a
        // court close that fits a transaction and one that does not.
        const IN_LEN: usize = 32;
        const TILE_START_ROW: usize = 16;
        const TILE_WIDTH: usize = 16;
        let all_weights = base0_matmul_weights();
        let byte_offset = TILE_START_ROW * IN_LEN;
        let operands = [
            PalwArtifactOperandV1 {
                tensor_name: "blk.{layer}.w".to_string(),
                layer: Some(0),
                row_start: byte_offset as u32,
                bytes: all_weights[byte_offset..byte_offset + TILE_WIDTH * IN_LEN].iter().map(|v| *v as u8).collect(),
            },
            PalwArtifactOperandV1 { tensor_name: "decoy".to_string(), layer: None, row_start: 0, bytes: vec![9, 9, 9] },
        ];
        let leaves: Vec<Hash64> = operands.iter().map(artifact_leaf_v1).collect();
        let artifact_root = artifact_root_v1(&leaves).unwrap();
        let openings = vec![crate::palw_artifact::PalwArtifactOpeningV1 {
            operand: operands[0].clone(),
            leaf_index: 0,
            leaf_count: leaves.len() as u32,
            path: vec![leaves[1]],
        }];
        (refutation, openings, artifact_root)
    }

    /// **P0-8's owed end-to-end conviction, at this layer.** A node holding NO model recomputes a
    /// BASE-0 matmul from proof-carried weights and convicts.
    #[test]
    fn palw_v2_matmul_fraud_convicts_without_model() {
        use crate::palw_artifact::PalwProvenOperandsV1;
        let (refutation, openings, artifact_root) = base0_matmul_fraud();

        // The oracle IS the openings, verified against the class's registered root. Nothing here
        // opens a file, and `NoWeights` — the "a full node has no model" stand-in — is what the
        // court would otherwise be stuck with.
        let operands = PalwProvenOperandsV1::from_openings_v1(&openings, artifact_root).expect("the openings prove");
        let verdict = check_execution_step_refutation_v1(&refutation, &operands).expect("a recomputable step");
        assert_eq!(verdict.fault, PalwStepFaultV1::ComputationMismatch { value_index: 3 });

        // And the same refutation WITHOUT the weights is `Unadjudicable`, never a conviction:
        // not being able to check is nobody's fault. This is the half that makes the assertion
        // above mean something — otherwise it would pass on a court that convicts blindly.
        assert_eq!(
            check_execution_step_refutation_v1(&refutation, &NoWeights),
            Err(PalwStepRefuteError::Unadjudicable),
            "with no weights the court must refuse, not convict"
        );

        // An opening against a DIFFERENT root proves nothing, so it never becomes an oracle.
        assert!(PalwProvenOperandsV1::from_openings_v1(&openings, h64(0xBD)).is_err());
    }

    /// **G5d: the gather is adjudicated from the registered table and a hash-checked token id.**
    ///
    /// It used to return the identity of `inputs[0]`, which is an admission that a real gather
    /// could not be checked — and it also forced the node to declare an input row that a pre
    /// table has no upstream to supply. Both are gone.
    ///
    /// The safety half is the point: the ids decide what "correct" means for this step, so an
    /// unchecked list is a challenger naming whatever convicts an honest producer. They are
    /// matched against the job context's own commitment before one of them is read.
    #[test]
    fn palw_v2_the_gather_reads_the_table_at_a_hash_checked_token_id() {
        let node = PalwStepNodeV1 {
            op_kind: PalwStepOpKindV1::EmbedLookup,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: "token_embd.weight".to_string(),
            weight_dtypes: vec![24],
            out_len: crate::palw_step::PalwStepOutLenV1::Fixed { elements: 4 },
            tile_len: 16,
            kernel_semantics_id: kernel_semantics_id_v1(KDESC_BASE0_EMBED),
            input_refs: Vec::new(),
        };
        // A four-row table: row t is [t, t+1, t+2, t+3] as int8 codes.
        struct Table;
        impl PalwWeightOracleV1 for Table {
            fn operand_bytes(&self, name: &str, _l: Option<u16>, byte_offset: u32, byte_len: u32) -> Option<Vec<u8>> {
                // `int8` table: one byte per value, so the byte range is the value range.
                if name != "token_embd.weight" || byte_offset + byte_len > 16 {
                    return None;
                }
                Some((byte_offset..byte_offset + byte_len).map(|i| (i % 4 + i / 4) as u8).collect())
            }
        }
        let want = |t: u32| -> Vec<u32> { (0..4u32).map(|i| ((t * 4 + i) % 4 + t) as i32 as u32).collect() };

        let ids = [3u32, 1, 2];
        for (pos, id) in ids.iter().enumerate() {
            let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: pos as u32, tile_index: 0 };
            let got = base0_row(Base0Op::Embed, &node, None, &profile(), &[], &Table, 1, (&coord, &ids, &[]))
                .expect("a prefill gather adjudicates");
            assert_eq!(got, want(*id), "position {pos} must gather the row its token id names");
        }

        // **A DECODE gather adjudicates now** (ADR-0049 Decision E). It refused every decode
        // position, and the reason was right: a token pinned by nothing lets a challenger name
        // whatever convicts an honest producer. The ids are pinned — the caller has already
        // recomputed `full_logits_trace_root_v2` from the carried summary and them — so the arm
        // reads the token the claim itself committed. Decode call `c` consumes what call `c - 1`
        // generated.
        let generated: Vec<u32> = vec![2, 0];
        let decode = PalwStepCoordinateV1 { call_index: 1, node_slot: 0, position: 0, tile_index: 0 };
        assert_eq!(
            base0_row(Base0Op::Embed, &node, None, &profile(), &[], &Table, 1, (&decode, &ids, &generated))
                .expect("a decode gather adjudicates against the claim's own generated ids"),
            want(generated[0]),
            "decode call 1 gathers the row the token generated by call 0 names"
        );
        let decode2 = PalwStepCoordinateV1 { call_index: 2, node_slot: 0, position: 1, tile_index: 0 };
        assert_eq!(
            base0_row(Base0Op::Embed, &node, None, &profile(), &[], &Table, 1, (&decode2, &ids, &generated))
                .expect("and so does the next one"),
            want(generated[1])
        );
        // A decode position past what the claim generated stays a refusal, never a default —
        // index 0 would adjudicate every out-of-range gather against the first generated token.
        let past_decode = PalwStepCoordinateV1 { call_index: 9, node_slot: 0, position: 8, tile_index: 0 };
        assert_eq!(
            base0_row(Base0Op::Embed, &node, None, &profile(), &[], &Table, 1, (&past_decode, &ids, &generated)),
            Err(PalwStepRefuteError::Unadjudicable)
        );
        // And with no ids carried at all it is unadjudicable rather than falling back to the prompt,
        // which would gather a prefill row for a decode step and convict an honest producer.
        assert_eq!(
            base0_row(Base0Op::Embed, &node, None, &profile(), &[], &Table, 1, (&decode, &ids, &[])),
            Err(PalwStepRefuteError::Unadjudicable),
            "a decode step with no generated ids carried is unchecked, not decided"
        );

        // A position past the carried ids is a refusal, never a default: index 0 would adjudicate
        // every out-of-range gather against the FIRST token and convict honest producers.
        let past = PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 9, tile_index: 0 };
        assert_eq!(
            base0_row(Base0Op::Embed, &node, None, &profile(), &[], &Table, 1, (&past, &ids, &[])),
            Err(PalwStepRefuteError::Unadjudicable)
        );
        // A table that cannot serve the row is unadjudicable, not a conviction.
        struct Empty;
        impl PalwWeightOracleV1 for Empty {
            fn operand_bytes(&self, _n: &str, _l: Option<u16>, _r: u32, _e: u32) -> Option<Vec<u8>> {
                None
            }
        }
        let first = PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 };
        assert_eq!(
            base0_row(Base0Op::Embed, &node, None, &profile(), &[], &Empty, 1, (&first, &ids, &[])),
            Err(PalwStepRefuteError::Unadjudicable)
        );
    }

    /// **The carried ids are checked against the job context before any is read.**
    ///
    /// Without this the gather is a false-slash machine: a challenger picks the ids that make an
    /// honest producer's committed row look wrong, and the court convicts on the challenger's own
    /// choice of what the input was.
    #[test]
    fn palw_v2_a_refutation_carrying_foreign_prompt_ids_is_refused() {
        let (binding, material, rows) = honest_execution();
        let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 0 };
        let mut refutation = build_refutation(&binding, &material, &rows, coord);

        // Honest and empty: the refutation addresses no gather, and the check is inert.
        assert!(refutation.prompt_token_ids.is_empty());
        assert!(check_execution_step_refutation_v1(&refutation, &NoWeights).is_err(), "this fixture is a NoFault case");

        // Ids the job context does not commit to are refused as non-canonical, in the
        // `InputSetNotCanonical` family — the challenger's evidence is wrong, not the producer's.
        refutation.prompt_token_ids = vec![1, 2, 3];
        assert!(
            matches!(check_execution_step_refutation_v1(&refutation, &NoWeights), Err(PalwStepRefuteError::InputSetNotCanonical(_))),
            "ids the context does not commit to must never reach a kernel"
        );

        // …and the ids the context DOES commit to pass the check (the step then fails on its own
        // merits, which is a different error).
        let mut honest_ids = binding.job_context.clone();
        let ids = vec![7u32, 8];
        honest_ids.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&ids);
        let mut with_ids = refutation.clone();
        with_ids.binding.job_context = honest_ids;
        with_ids.prompt_token_ids = ids;
        assert!(
            !matches!(
                check_execution_step_refutation_v1(&with_ids, &NoWeights),
                Err(PalwStepRefuteError::InputSetNotCanonical(m)) if m.contains("carried prompt ids")
            ),
            "ids matching the commitment must pass the id check"
        );
    }

    /// **G5c: the KV sentinels resolve to the cache-role nodes over the position history.**
    ///
    /// `canonical_input_leaves` used to answer `None` for them — "KV / checkpoint arms:
    /// registration-opaque today" — and a `None` is `Unadjudicable` before any kernel runs, so
    /// attention was unreachable however capable the kernel became.
    ///
    /// The fix needs no new leaf format and reads no float aux series: the cache contents are
    /// already ordinary step tiles, because the K and V projection nodes carry `KCacheWrite` /
    /// `VCacheWrite` and commit their output at every position. This asserts the resolution
    /// itself — which node it names and which positions it spans — rather than that some function
    /// returned `Some`.
    #[test]
    fn palw_v2_the_kv_sentinels_resolve_to_the_cache_nodes_over_the_history() {
        use crate::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PalwStepNodeRoleV1};
        let mut p = base0_matmul_profile();
        // Every layer is an attention layer: `KvScaled` widths are only meaningful in a graph
        // that has one, and the cache roles live in the attention table.
        p.full_attention_interval = 1;
        let node = |kind, desc: &str, role, out, refs: Vec<u16>| PalwStepNodeV1 {
            op_kind: kind,
            role,
            weight_name: String::new(),
            weight_dtypes: Vec::new(),
            out_len: out,
            tile_len: 16,
            kernel_semantics_id: kernel_semantics_id_v1(desc),
            input_refs: refs,
        };
        // slot 0: the layer input. 1: K cache write. 2: V cache write. 3: scores (q x K^T).
        p.gdn_nodes = Vec::new();
        p.attn_nodes = vec![
            node(
                PalwStepOpKindV1::Silu,
                KDESC_BASE0_SILU,
                PalwStepNodeRoleV1::Plain,
                crate::palw_step::PalwStepOutLenV1::Fixed { elements: 32 },
                vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN],
            ),
            node(
                PalwStepOpKindV1::Silu,
                KDESC_BASE0_SILU,
                PalwStepNodeRoleV1::KCacheWrite,
                crate::palw_step::PalwStepOutLenV1::Fixed { elements: 16 },
                vec![0],
            ),
            node(
                PalwStepOpKindV1::Silu,
                KDESC_BASE0_SILU,
                PalwStepNodeRoleV1::VCacheWrite,
                crate::palw_step::PalwStepOutLenV1::Fixed { elements: 16 },
                vec![0],
            ),
            node(
                PalwStepOpKindV1::MatMulQuant,
                KDESC_BASE0_MATMUL,
                PalwStepNodeRoleV1::Plain,
                crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier: 1 },
                vec![0, PALW_STEP_INPUT_KV_K],
            ),
        ];
        p.validate_shape().expect("the probe profile is well-formed");
        let mut ctx = context();
        ctx.shape_profile_id = p.shape_profile_id();

        // Challenge the scores node at the last prefill position (the fixture context declares
        // two prefill tokens, so positions are 0 and 1): it reads its query at position 1 and the
        // cached keys at positions 0 and 1.
        let scores_slot = 1 + 3; // pre(1) + attn slots 0..2
        let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: scores_slot, position: 1, tile_index: 0 };
        let required = canonical_input_leaves(&p, &ctx, scores_slot, &coord, KernelProgram::Base0(Base0Op::MatMul))
            .expect("the KV sentinel resolves — this returned None before G5c");

        assert_eq!(required.len(), 2, "one group per input ref: the query row, then the whole key history");
        let query: Vec<_> = required[0].iter().map(|(_, c)| (c.node_slot, c.position)).collect();
        assert_eq!(query, vec![(1, 1), (1, 1)], "the query is this position's layer-input row, both its tiles");
        let keys: Vec<_> = required[1].iter().map(|(_, c)| (c.node_slot, c.position)).collect();
        assert_eq!(
            keys,
            vec![(2, 0), (2, 1)],
            "the keys are the KCacheWrite node (slot 2) at every position up to the challenged one"
        );

        // The V sentinel names the OTHER role, so the two are not interchangeable.
        let mut v_node = p.clone();
        v_node.attn_nodes[3].input_refs = vec![0, PALW_STEP_INPUT_KV_V];
        let required_v = canonical_input_leaves(&v_node, &ctx, scores_slot, &coord, KernelProgram::Base0(Base0Op::MatMul))
            .expect("the V sentinel resolves too");
        assert_eq!(
            required_v[1].iter().map(|(_, c)| c.node_slot).collect::<Vec<_>>(),
            vec![3, 3],
            "the V sentinel names the VCacheWrite node (slot 3), not the K one"
        );

        // A layer with no such role names nothing, and that is a refusal rather than a guess.
        let mut roleless = p.clone();
        roleless.attn_nodes[1].role = PalwStepNodeRoleV1::Plain;
        assert!(
            canonical_input_leaves(&roleless, &ctx, scores_slot, &coord, KernelProgram::Base0(Base0Op::MatMul)).is_none(),
            "no KCacheWrite node means the K sentinel names nothing"
        );
        // Two of them would make "the K cache" ambiguous, and a court that had to choose would be
        // choosing its own evidence.
        let mut doubled = p.clone();
        doubled.attn_nodes[2].role = PalwStepNodeRoleV1::KCacheWrite;
        assert!(
            canonical_input_leaves(&doubled, &ctx, scores_slot, &coord, KernelProgram::Base0(Base0Op::MatMul)).is_none(),
            "two KCacheWrite nodes is an ambiguous cache"
        );
    }

    #[test]
    fn honest_steps_recompute_to_no_fault() {
        let (binding, material, rows) = honest_execution();
        let p = &binding.shape_profile;
        let ctx = &binding.job_context;
        // Adjudicate an L2Norm step, a Sigmoid step, and the GDN core at the decode call.
        for (slot, tile) in [(1u32, 0u32), (3, 1), (6, 0), (6, 1)] {
            let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: slot, position: 0, tile_index: tile };
            let out_idx = canonical_step_leaf_index(p, ctx, &coord).unwrap();
            let refutation = build_refutation(&binding, &material, &rows, coord);
            let got = check_execution_step_refutation_v1(&refutation, &NoWeights);
            assert_eq!(got, Err(PalwStepRefuteError::NoFaultFound), "slot {slot} tile {tile} leaf {out_idx}");
        }
    }

    #[test]
    fn tampered_output_convicts_with_computation_mismatch() {
        let (mut binding, mut material, rows) = honest_execution();
        let p = binding.shape_profile.clone();
        let ctx = binding.job_context.clone();
        // Corrupt ONE committed value of the decode-call GDN output and rebuild the tree —
        // the miner's world where the computation lies.
        let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 6, position: 0, tile_index: 1 };
        let idx = canonical_step_leaf_index(&p, &ctx, &coord).unwrap();
        let ctx_hash = ctx.context_hash();
        let profile_hash = p.shape_profile_id();
        let mut row = rows[2][6].clone();
        row[16 + 3] = ref_add_v1(row[16 + 3], 0x3F80_0000); // +1.0 to value 3 of tile 1
        let leaf = PalwStepTileLeafV1 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            coord,
            value_count: 16,
            values_le: row[16..32].iter().flat_map(|v| v.to_le_bytes()).collect(),
        };
        material.leaf_hashes[idx as usize] = step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &leaf);
        let merkle = crate::palw_step_leg::step_merkle_root_v1(&material.leaf_hashes).unwrap();
        material.merkle_root = merkle;
        binding.step_merkle_root = merkle;
        let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, binding.step_leaf_count, &merkle);
        let ckpt_root = checkpoint_leg_root_v2(
            &ctx_hash,
            &binding.checkpoint_profile.profile_hash(),
            &binding.state_chunk_map_id,
            1,
            0,
            &binding.checkpoint_merkle_root,
        );
        binding.committed_execution_root = execution_commitment_root_v2(
            &ctx_hash,
            &binding.full_logits_trace_root,
            &binding.activation_leg_root,
            &ckpt_root,
            &step_root,
        );
        let mut refutation = build_refutation(&binding, &material, &rows, coord);
        refutation.output_preimage = leaf;
        refutation.output_opening = step_opening_v1(&material.leaf_hashes, idx).unwrap();
        let verdict = check_execution_step_refutation_v1(&refutation, &NoWeights).unwrap();
        assert_eq!(verdict.fault, PalwStepFaultV1::ComputationMismatch { value_index: 3 });
    }

    #[test]
    fn wrong_input_set_is_rejected_not_convicted() {
        let (binding, material, rows) = honest_execution();
        let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 0 };
        let mut refutation = build_refutation(&binding, &material, &rows, coord);
        // Swap in a DIFFERENT honest tile as "the input".
        let alien_coord = PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 1, tile_index: 0 };
        let alien_idx = canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &alien_coord).unwrap();
        refutation.inputs[0].opening = step_opening_v1(&material.leaf_hashes, alien_idx).unwrap();
        refutation.inputs[0].preimage = tile_preimage(&rows, &binding, alien_coord);
        let got = check_execution_step_refutation_v1(&refutation, &NoWeights);
        assert!(
            matches!(got, Err(PalwStepRefuteError::InputSetNotCanonical(_))),
            "an alien input must reject the refutation, got {got:?}"
        );
    }

    /// A refutation that passes every STRUCTURAL check and then meets an uncatalogued kernel — the
    /// only route to `Unadjudicable`, and therefore the only way anything downstream can test what
    /// a coverage gap does.
    ///
    /// `skeleton_refutation` beside it deliberately cannot get this far (it fails on its opening
    /// path), which is why the class-freeze rule was recorded twice as an untested path. Exposed so
    /// `palw_facts` can build the conviction carriage that reaches its freeze end to end, and used
    /// by the test below so the two cannot drift into disagreeing about what "unadjudicable" is.
    pub(crate) fn unadjudicable_refutation() -> PalwExecutionStepRefutationV1 {
        let (binding, material, rows) = honest_execution();
        // The post node carries an unimplemented descriptor.
        let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 7, position: 0, tile_index: 0 };
        build_refutation(&binding, &material, &rows, coord)
    }

    #[test]
    fn unknown_kernel_is_unadjudicable() {
        assert_eq!(
            check_execution_step_refutation_v1(&unadjudicable_refutation(), &NoWeights),
            Err(PalwStepRefuteError::Unadjudicable)
        );
    }

    // --- helpers ---

    fn tile_preimage(rows: &[Vec<Vec<u32>>], binding: &PalwStepBindingV2, coord: PalwStepCoordinateV1) -> PalwStepTileLeafV1 {
        let ord = match (coord.call_index, coord.position) {
            (0, 0) => 0usize,
            (0, 1) => 1,
            (1, 0) => 2,
            _ => unreachable!(),
        };
        let row = &rows[ord][coord.node_slot as usize];
        let start = coord.tile_index as usize * 16;
        let end = (start + 16).min(row.len());
        let _ = binding;
        PalwStepTileLeafV1 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            coord,
            value_count: (end - start) as u32,
            values_le: row[start..end].iter().flat_map(|v| v.to_le_bytes()).collect(),
        }
    }

    fn build_refutation(
        binding: &PalwStepBindingV2,
        material: &crate::palw_step_leg::PalwStepLegMaterialV1,
        rows: &[Vec<Vec<u32>>],
        coord: PalwStepCoordinateV1,
    ) -> PalwExecutionStepRefutationV1 {
        let p = &binding.shape_profile;
        let ctx = &binding.job_context;
        let out_idx = canonical_step_leaf_index(p, ctx, &coord).unwrap();
        let (node, _) = p.resolve_node_slot(coord.node_slot).unwrap();
        let program = resolve_kernel(&node.kernel_semantics_id);
        let inputs = match program {
            Some(prog) => canonical_input_leaves(p, ctx, coord.node_slot, &coord, prog)
                .unwrap_or_default()
                .into_iter()
                .flatten()
                .map(|(idx, c)| PalwStepInputOpeningV1 {
                    opening: step_opening_v1(&material.leaf_hashes, idx).unwrap(),
                    preimage: tile_preimage(rows, binding, c),
                })
                .collect(),
            None => Vec::new(),
        };
        PalwExecutionStepRefutationV1 {
            binding: binding.clone(),
            output_opening: step_opening_v1(&material.leaf_hashes, out_idx).unwrap(),
            output_preimage: tile_preimage(rows, binding, coord),
            inputs,
            prompt_token_ids: Vec::new(),
            decode_tokens: None,
        }
    }

    /// Recompute a binding's `committed_execution_root` from its own parts — what every fixture
    /// that edits a bound field must do, extracted so they cannot each do it differently.
    pub(crate) fn rebind_committed_root(binding: &mut PalwStepBindingV2) {
        let ctx_hash = binding.job_context.context_hash();
        let profile_hash = binding.shape_profile.shape_profile_id();
        let decode_calls = binding.job_context.exact_decode_tokens.saturating_sub(1);
        let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, binding.step_leaf_count, &binding.step_merkle_root);
        let ckpt_root = checkpoint_leg_root_v2(
            &ctx_hash,
            &binding.checkpoint_profile.profile_hash(),
            &binding.state_chunk_map_id,
            decode_calls,
            binding.checkpoint_count,
            &binding.checkpoint_merkle_root,
        );
        binding.committed_execution_root = execution_commitment_root_v2(
            &ctx_hash,
            &binding.full_logits_trace_root,
            &binding.activation_leg_root,
            &ckpt_root,
            &step_root,
        );
    }

    /// The matmul fixture's execution, re-committed with a REAL integer decode commitment:
    /// `full_logits_trace_root` becomes [`base0_logits_trace_root_v1`] over the given rows and
    /// ids. The rows must be `exact_decode_tokens` of them, each `vocab_size` wide — the shape
    /// the pin check enforces.
    pub(crate) fn base0_binding_with_decode_root(
        logits_rows: Vec<Vec<i32>>,
        generated: Vec<u32>,
    ) -> (PalwStepBindingV2, crate::palw_step_leg::PalwStepLegMaterialV1, Vec<Vec<Vec<u32>>>, PalwBase0DecodeTokensV1) {
        let (mut binding, material, rows) = base0_honest_execution();
        binding.full_logits_trace_root = base0_logits_trace_root_v1(&binding.job_context, &logits_rows, &generated);
        rebind_committed_root(&mut binding);
        (binding, material, rows, PalwBase0DecodeTokensV1 { logits_rows, generated_token_ids: generated })
    }

    /// Deterministic logits rows at the fixture's shape (2 decode calls, vocabulary 40), and the
    /// ids the pinned selection rule derives from them.
    pub(crate) fn base0_honest_decode_commitment()
    -> (PalwStepBindingV2, crate::palw_step_leg::PalwStepLegMaterialV1, Vec<Vec<Vec<u32>>>, PalwBase0DecodeTokensV1) {
        let (b, _, _) = base0_honest_execution();
        let vocab = b.shape_profile.vocab_size as usize;
        let decode = b.job_context.exact_decode_tokens as usize;
        let logits_rows: Vec<Vec<i32>> =
            (0..decode).map(|c| (0..vocab).map(|i| ((c * 31 + i * 7) % 23) as i32 - 11).collect()).collect();
        let generated: Vec<u32> = logits_rows.iter().map(|r| base0_decode_token_select_v1(r) as u32).collect();
        base0_binding_with_decode_root(logits_rows, generated)
    }

    /// **ADR-0049 Decision E fires, and fires positionally.** An honest commitment clears at
    /// every decode position; a commitment whose id at position 1 is not the selection rule's
    /// answer convicts at position 1 with `DecodeTokenMismatch { position: 1 }` — and STILL
    /// clears at position 0, because the fault is the lie, not the producer's neighborhood.
    #[test]
    fn the_decode_token_selection_rule_convicts_exactly_the_lying_position() {
        let (binding, _m, _r, pin) = base0_honest_decode_commitment();
        for p in 0..pin.generated_token_ids.len() as u32 {
            assert!(
                matches!(check_base0_decode_token_refutation_v1(&binding, &pin, p), Err(PalwStepRefuteError::NoFaultFound)),
                "an honest committed token at position {p} clears"
            );
        }
        // The lie is INSIDE the commitment: the root is recomputed over the lying ids, exactly
        // as a fraudulent producer would commit it.
        let mut lying_ids = pin.generated_token_ids.clone();
        lying_ids[1] = lying_ids[1].wrapping_add(1);
        let (lb, _m2, _r2, lpin) = base0_binding_with_decode_root(pin.logits_rows.clone(), lying_ids);
        let verdict = check_base0_decode_token_refutation_v1(&lb, &lpin, 1).expect("a lying committed token convicts");
        assert_eq!(verdict.fault, crate::palw_step_leg::PalwStepFaultV1::DecodeTokenMismatch { position: 1 });
        assert!(
            matches!(check_base0_decode_token_refutation_v1(&lb, &lpin, 0), Err(PalwStepRefuteError::NoFaultFound)),
            "position 0's token was honest, and stays cleared"
        );
        assert!(
            matches!(check_base0_decode_token_refutation_v1(&lb, &lpin, 7), Err(PalwStepRefuteError::InputSetNotCanonical(_))),
            "a position outside the job's decode calls is malformed evidence, never a verdict"
        );
    }

    /// **The tie rule is the LOWEST index, and the court holds a producer to it.** Two lanes tie
    /// at the max; a producer that committed the lower twin clears, one that committed the
    /// higher twin is convicted — same rows, same values, one rule.
    #[test]
    fn the_decode_token_tie_breaks_to_the_lowest_index() {
        let (b, _, _) = base0_honest_execution();
        let vocab = b.shape_profile.vocab_size as usize;
        let decode = b.job_context.exact_decode_tokens as usize;
        let mut row = vec![-5i32; vocab];
        row[3] = 7;
        row[9] = 7; // the twin
        let rows: Vec<Vec<i32>> = (0..decode).map(|_| row.clone()).collect();

        let (hb, _m, _r, hpin) = base0_binding_with_decode_root(rows.clone(), vec![3; decode]);
        for p in 0..decode as u32 {
            assert!(
                matches!(check_base0_decode_token_refutation_v1(&hb, &hpin, p), Err(PalwStepRefuteError::NoFaultFound)),
                "the lower twin is the selection"
            );
        }
        let (gb, _m2, _r2, gpin) = base0_binding_with_decode_root(rows, vec![9; decode]);
        let verdict = check_base0_decode_token_refutation_v1(&gb, &gpin, 0).expect("the higher twin convicts");
        assert_eq!(verdict.fault, crate::palw_step_leg::PalwStepFaultV1::DecodeTokenMismatch { position: 0 });
    }

    /// **A pin the commitment does not authenticate is refused, never adjudicated.** One lane
    /// flipped, one id flipped, one row the wrong width, the wrong row count — each is malformed
    /// evidence by name, and none of them can convict anyone.
    #[test]
    fn a_decode_pin_the_committed_root_does_not_authenticate_is_refused() {
        let (binding, _m, _r, pin) = base0_honest_decode_commitment();
        let cases: Vec<(&str, PalwBase0DecodeTokensV1)> = vec![
            ("a flipped lane", {
                let mut p = pin.clone();
                p.logits_rows[0][0] += 1;
                p
            }),
            ("a flipped id", {
                let mut p = pin.clone();
                p.generated_token_ids[0] = p.generated_token_ids[0].wrapping_add(1);
                p
            }),
            ("a row the wrong width", {
                let mut p = pin.clone();
                p.logits_rows[1].pop();
                p
            }),
            ("the wrong row count", {
                let mut p = pin.clone();
                p.logits_rows.pop();
                p
            }),
        ];
        for (what, bad) in cases {
            assert!(
                matches!(check_base0_decode_token_refutation_v1(&binding, &bad, 0), Err(PalwStepRefuteError::InputSetNotCanonical(_))),
                "{what} must be refused as malformed evidence"
            );
        }
    }

    /// **The lane guard is refusal by name, in both directions.** A `Float32` class has no
    /// decode-token adjudicator yet, so a close against one is refused rather than adjudicated
    /// wrongly; and at the step-refutation dispatch, an `Int32` class handed a `FloatV2` pin — or
    /// a pin whose root recompute fails — is refused before a single id is read.
    #[test]
    fn a_pin_that_does_not_speak_the_class_lane_is_refused() {
        // The close guard: same execution, lane rewritten to Float32 (with the context and the
        // committed root made consistent, so the refusal is the LANE's and not the binding's).
        let (binding, _m, _r, pin) = base0_honest_decode_commitment();
        let mut fb = binding.clone();
        fb.shape_profile.lane = crate::palw_step::PalwStepLaneV1::Float32;
        fb.job_context.shape_profile_id = fb.shape_profile.shape_profile_id();
        rebind_committed_root(&mut fb);
        match check_base0_decode_token_refutation_v1(&fb, &pin, 0) {
            Err(PalwStepRefuteError::InputSetNotCanonical(msg)) => {
                assert!(msg.contains("Float32"), "the refusal names the missing adjudicator: {msg}")
            }
            other => panic!("a Float32 class must be refused by name, got {other:?}"),
        }

        // The step-dispatch guard: a decode-embed refutation of the Int32 class carrying the
        // FLOAT scheme's pin. The refusal happens before any gather or oracle is consulted.
        let (ib, material, rows, _) = base0_honest_decode_commitment();
        let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 0, position: 0, tile_index: 0 };
        let mut refutation = build_refutation(&ib, &material, &rows, coord);
        refutation.decode_tokens = Some(PalwDecodeTokenPinV1::FloatV2(PalwDecodeTokensV1 {
            summary: crate::palw_v2::PalwTraceSummaryV2 {
                vocab_size: 40,
                logits_dtype: crate::palw_v2::PalwLogitsDtypeV2::F32Le,
                declared_prefill_tokens: 2,
                exact_decode_tokens: 2,
                event_count: 4,
                first_event_kind: crate::palw_v2::PalwTracePhaseV2::Prefill,
                last_event_kind: crate::palw_v2::PalwTracePhaseV2::Decode,
                output_token_ids_hash: crate::palw_v2::output_token_ids_hash_v2(&[1, 2]),
                stop_reason: crate::palw_v2::PalwStopReasonV2::ExactBudgetReached,
            },
            ordered_event_commitment: h64(0xF0),
            generated_token_ids: vec![1, 2],
        }));
        let empty_oracle = crate::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&[], h64(0x1)).unwrap();
        match check_execution_step_refutation_v1(&refutation, &empty_oracle) {
            Err(PalwStepRefuteError::InputSetNotCanonical(msg)) => {
                assert!(msg.contains("lane"), "the dispatch refuses the wrong-scheme pin by name: {msg}")
            }
            other => panic!("an Int32 class handed a FloatV2 pin must be refused, got {other:?}"),
        }

        // And a Base0 pin whose recompute does not reproduce the committed root — the same
        // refusal, at the same point, before anything downstream runs.
        let (ub, umaterial, urows) = base0_honest_execution(); // root left at the h64(0xAA) placeholder
        let mut unrooted = build_refutation(&ub, &umaterial, &urows, coord);
        unrooted.decode_tokens = Some(PalwDecodeTokenPinV1::Base0V1(pin));
        match check_execution_step_refutation_v1(&unrooted, &empty_oracle) {
            Err(PalwStepRefuteError::InputSetNotCanonical(msg)) => {
                assert!(msg.contains("integer trace root"), "the dispatch refuses an unauthenticated pin by name: {msg}")
            }
            other => panic!("a pin the committed root does not authenticate must be refused, got {other:?}"),
        }
    }

    #[test]
    fn total_leaf_shape_sanity() {
        let p = profile();
        // Honest material declares the profile it was produced under (the equality the step-leg
        // verifier enforces).
        let mut ctx = context();
        ctx.shape_profile_id = p.shape_profile_id();
        assert_eq!(kv_aux_leaf_count(&p, &ctx), 0);
        assert!(step_leaf_count(&p, &ctx).unwrap() > 0);
    }
}

#[cfg(test)]
mod catalog_through_line_tests {
    use super::{KDESC_ALL, KDESC_BASE0_ALL, KERNEL_CATALOG, resolve_kernel};
    use crate::palw_step::kernel_semantics_id_v1;
    use std::collections::BTreeSet;

    /// Every kernel the coverage gate will certify must be one this build can actually execute.
    ///
    /// Guaranteed by construction now (both read `KERNEL_CATALOG`), asserted anyway because the
    /// property is the whole point of A4: a certified-but-unresolvable kernel makes every dispute
    /// over it `Unadjudicable`, which is rejected-but-unslashed — the hole a forger farms.
    #[test]
    fn every_catalogued_kernel_resolves_to_a_program() {
        for id in super::catalogued_kernel_ids_v1() {
            assert!(resolve_kernel(&id).is_some(), "catalogued but not adjudicable: {id}");
        }
    }

    /// `KDESC_ALL` is the human-facing list and registration reads it; the table is what the
    /// adjudicator runs. They must be the same set, in both directions — a descriptor in the list
    /// but not the table certifies the unexecutable, and one in the table but not the list is a
    /// kernel registration cannot name.
    #[test]
    fn the_descriptor_list_and_the_adjudication_table_are_the_same_set() {
        let listed: BTreeSet<&str> = KDESC_ALL.iter().copied().collect();
        let tabled: BTreeSet<&str> = KERNEL_CATALOG.iter().map(|(d, _)| *d).collect();
        assert_eq!(listed, tabled, "KDESC_ALL and KERNEL_CATALOG have drifted apart");
        assert_eq!(KDESC_ALL.len(), KERNEL_CATALOG.len(), "a duplicate descriptor would hide a gap");
    }

    /// ADR-0040 Decision H's tenth op included: the closed BASE-0 catalog is closed on this side
    /// too. `Rescale` was the one missing when the audit looked, and Decision H records that the
    /// other nine cannot be computed without it.
    #[test]
    fn the_closed_base0_catalog_reaches_the_adjudicator_whole() {
        assert_eq!(KDESC_BASE0_ALL.len(), 10, "ADR-0040 froze ten BASE-0 kinds");
        for d in KDESC_BASE0_ALL {
            let id = kernel_semantics_id_v1(d);
            assert!(resolve_kernel(&id).is_some(), "BASE-0 op not adjudicable: {d}");
            assert!(super::catalogued_kernel_ids_v1().contains(&id), "BASE-0 op not catalogued: {d}");
        }
    }
}
