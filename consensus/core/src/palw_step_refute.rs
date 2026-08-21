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
        KernelProgram::Base0(
            Base0Op::RmsNorm | Base0Op::Softmax | Base0Op::Silu,
        )
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
fn base0_row(
    op: Base0Op,
    node: &crate::palw_step::PalwStepNodeV1,
    layer: Option<u16>,
    profile: &PalwShapeProfileV3,
    inputs: &[Vec<u32>],
    weights: &dyn PalwWeightOracleV1,
    kv_len: u64,
    gather: (&PalwStepCoordinateV1, &[u32]),
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
            Ok(out(ops::softmax(&as_i32(&inputs[0])).map_err(shape)?))
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
            let (coord, prompt_ids) = gather;
            // Prefill only. A DECODE token is whatever the model generated, so it is not in the
            // prompt and its id is pinned by nothing here — a challenger naming it freely would
            // convict an honest producer, which is the one failure this court may never have. The
            // remaining half of G5d (deriving it from the previous position's committed logits)
            // is recorded in `docs/palw-qwen25-class-phase0.md`.
            if coord.call_index != 0 {
                return Err(PalwStepRefuteError::Unadjudicable);
            }
            let token = *prompt_ids.get(coord.position as usize).ok_or(PalwStepRefuteError::Unadjudicable)?;
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
            let row =
                weights.operand_bytes(node.weight_name.as_str(), layer, 0, 5).ok_or(PalwStepRefuteError::Unadjudicable)?;
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
            let wanted = out_dim.checked_mul(x.len()).ok_or(PalwStepRefuteError::Unadjudicable)?;

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
            let w: Vec<i8> = if node.weight_name.is_empty() {
                need(2)?;
                let operand = as_i8(&inputs[1])?;
                // The opened row must be exactly the matrix the declared shape needs. A shorter
                // one is a producer that committed a different graph, not a challenger's fault.
                if operand.len() != wanted {
                    return Err(PalwStepRefuteError::InputSetNotCanonical("base0 matmul operand row is not out_dim x in_dim"));
                }
                operand
            } else {
                let row = weights
                    .operand_bytes(
                        node.weight_name.as_str(),
                        layer,
                        0,
                        // `int8` weights: one byte per value.
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
            Ok(out(ops::matmul_quant(&w, &x, out_dim).map_err(shape)?))
        }
        Base0Op::Requantize | Base0Op::Rope => {
            need(1)?;
            let name = node.weight_name.as_str();
            // The two ops share this site and their parameter blocks do NOT share a width, so the
            // byte count is computed per op before the request. One argument meaning two widths is
            // the defect ADR-0049 Decision A names.
            let byte_len = match op {
                // (multiplier LE, shift, zero LE) per channel.
                Base0Op::Requantize => 9usize.checked_mul(inputs[0].len()),
                // cos row then sin row, 4 bytes each, one pair per two lanes.
                Base0Op::Rope => 8usize.checked_mul(inputs[0].len() / 2),
                _ => unreachable!("outer match restricts these three"),
            }
            .ok_or(PalwStepRefuteError::Unadjudicable)?;
            let row = weights
                .operand_bytes(name, layer, 0, u32::try_from(byte_len).map_err(|_| PalwStepRefuteError::Unadjudicable)?)
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
                    // The pinned table: cos row then sin row, 4 bytes each, one pair per two lanes.
                    let pairs = inputs[0].len() / 2;
                    if row.len() != 8 * pairs {
                        return Err(PalwStepRefuteError::InputSetNotCanonical("base0 rope table is not 8 bytes per pair"));
                    }
                    let read = |o: usize| -> Vec<i32> {
                        row[o..o + 4 * pairs].chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
                    };
                    let (cos_q, sin_q) = (read(0), read(4 * pairs));
                    Ok(out(ops::rope_table(&as_i32(&inputs[0]), &cos_q, &sin_q).map_err(shape)?))
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
}

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
        KernelProgram::GdnCore { .. } => {
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
        _ => vec![(out.call_index, out.position)],
    }
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
    // Expand the prefill marker for decode-call GDN steps.
    if positions.first() == Some(&(u32::MAX, 0)) {
        let mut expanded: Vec<(u32, u32)> = (0..context.declared_prefill_tokens).map(|p| (0, p)).collect();
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
            (0..context.declared_prefill_tokens)
                .map(|p| (0, p))
                .chain((1..=out_coord.call_index).map(|c| (c, 0)))
                .collect()
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
    let recomputed_row = run_program(
        program,
        node,
        layer,
        &binding.shape_profile,
        &inputs,
        weights,
        kv_len,
        (&out_coord, &refutation.prompt_token_ids),
    )?;

    // 4) Compare the challenged tile's slice, exact bits.
    let tile_start = out_coord.tile_index as usize * node.tile_len as usize;
    let committed: Vec<u32> =
        refutation.output_preimage.values_le.chunks_exact(4).map(|q| u32::from_le_bytes([q[0], q[1], q[2], q[3]])).collect();
    let recomputed = recomputed_row
        .get(tile_start..tile_start + committed.len())
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
    gather: (&PalwStepCoordinateV1, &[u32]),
) -> Result<Vec<u32>, PalwStepRefuteError> {
    match program {
        KernelProgram::Base0(op) => base0_row(op, node, layer, profile, inputs, weights, kv_len, gather),
        KernelProgram::L2Norm => {
            let x = inputs.first().ok_or(PalwStepRefuteError::InputSetNotCanonical("l2norm needs one input row"))?;
            Ok(l2_norm_row(x, profile.l2_eps_bits))
        }
        KernelProgram::RmsNormFused => {
            let x = inputs.first().ok_or(PalwStepRefuteError::InputSetNotCanonical("rmsnorm needs one input row"))?;
            // Four bytes per value: the fused norm's gain is an f32 lane.
            let wrow = weights
                .operand_bytes(&node.weight_name, layer, 0, u32::try_from(x.len() * 4).map_err(|_| PalwStepRefuteError::WeightUnavailable)?)
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
    }
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
    const NO_GATHER: (&PalwStepCoordinateV1, &[u32]) =
        (&PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 }, &[]);

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
        let operand =
            PalwArtifactOperandV1 { tensor_name: "blk.{layer}.scale".to_string(), layer: Some(0), row_start: 0, bytes };
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
        let kv_want: Vec<u32> =
            crate::palw_base0_ops::matmul_quant(&w[..8], &x, 2).unwrap().into_iter().map(|v| v as u32).collect();
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
            for prior in 0..=ord {
                gdn_inputs.push(rows[prior][1].clone());
                gdn_inputs.push(rows[prior][2].clone());
                gdn_inputs.push(rows[prior][3].clone());
                gdn_inputs.push(rows[prior][4][..2].to_vec()); // 2 heads — hmm, see note below
                gdn_inputs.push(rows[prior][5][..2].to_vec());
            }
            // The checker resolves g/beta rows as the FULL node rows (16 wide); mirror that.
            let mut gdn_inputs_full: Vec<Vec<u32>> = Vec::new();
            for prior in 0..=ord {
                gdn_inputs_full.push(rows[prior][1].clone());
                gdn_inputs_full.push(rows[prior][2].clone());
                gdn_inputs_full.push(rows[prior][3].clone());
                gdn_inputs_full.push(rows[prior][4].clone());
                gdn_inputs_full.push(rows[prior][5].clone());
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
        (0..32 * 32).map(|i| (((i * 7) % 13) as i32 - 6) as i8).collect()
    }

    /// An honest BASE-0 execution over [`base0_matmul_profile`], plus its leg material.
    ///
    /// Same three position ordinals the GDN fixture uses. Every value stays inside the int8 lane,
    /// because BASE-0 activations are int8 codes riding i32 lanes and an out-of-range lane is
    /// `InputSetNotCanonical` rather than arithmetic.
    pub(crate) fn base0_honest_execution() -> (PalwStepBindingV2, crate::palw_step_leg::PalwStepLegMaterialV1, Vec<Vec<Vec<u32>>>)
    {
        let p = base0_matmul_profile();
        let mut ctx = context();
        ctx.shape_profile_id = p.shape_profile_id();
        let w = base0_matmul_weights();
        let slots = p.global_node_count();
        let mut rows: Vec<Vec<Vec<u32>>> = vec![vec![Vec::new(); slots as usize]; 3];
        for ord in 0..3usize {
            // pre (slot 0): the embedding row, int8 codes.
            rows[ord][0] = (0..32).map(|i| (((ord * 5 + i) % 11) as i32 - 5) as u32).collect();
            let x: Vec<i8> = rows[ord][0].iter().map(|v| *v as i32 as i8).collect();
            // gdn (slot 1): the matmul, by the SAME function the court will recompute with.
            rows[ord][1] =
                crate::palw_base0_ops::matmul_quant(&w, &x, 32).unwrap().into_iter().map(|v| v as u32).collect();
            // post (slot 2): silu over the layer output.
            rows[ord][2] = crate::palw_base0_ops::silu(&rows[ord][1].iter().map(|v| *v as i32).collect::<Vec<_>>())
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
    pub(crate) fn base0_honest_case() -> (PalwExecutionStepRefutationV1, Vec<crate::palw_artifact::PalwArtifactOpeningV1>, Hash64)
    {
        use crate::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1};
        let (binding, material, rows) = base0_honest_execution();
        let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: 1, position: 0, tile_index: 1 };
        let refutation = build_refutation(&binding, &material, &rows, coord);
        let operands = vec![
            PalwArtifactOperandV1 {
                tensor_name: "blk.{layer}.w".to_string(),
                layer: Some(0),
                row_start: 0,
                bytes: base0_matmul_weights().iter().map(|v| *v as u8).collect(),
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
    pub(crate) fn base0_matmul_fraud() -> (PalwExecutionStepRefutationV1, Vec<crate::palw_artifact::PalwArtifactOpeningV1>, Hash64)
    {
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
        let operands = vec![
            PalwArtifactOperandV1 {
                tensor_name: "blk.{layer}.w".to_string(),
                layer: Some(0),
                row_start: 0,
                bytes: base0_matmul_weights().iter().map(|v| *v as u8).collect(),
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
            let got = base0_row(Base0Op::Embed, &node, None, &profile(), &[], &Table, 1, (&coord, &ids))
                .expect("a prefill gather adjudicates");
            assert_eq!(got, want(*id), "position {pos} must gather the row its token id names");
        }

        // A DECODE gather is refused rather than guessed: the token is whatever the model
        // generated, so it is in no prompt and pinned by nothing here.
        let decode = PalwStepCoordinateV1 { call_index: 1, node_slot: 0, position: 0, tile_index: 0 };
        assert_eq!(
            base0_row(Base0Op::Embed, &node, None, &profile(), &[], &Table, 1, (&decode, &ids)),
            Err(PalwStepRefuteError::Unadjudicable),
            "a decode token is pinned by nothing here — refusing is the only safe answer"
        );

        // A position past the carried ids is a refusal, never a default: index 0 would adjudicate
        // every out-of-range gather against the FIRST token and convict honest producers.
        let past = PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 9, tile_index: 0 };
        assert_eq!(
            base0_row(Base0Op::Embed, &node, None, &profile(), &[], &Table, 1, (&past, &ids)),
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
            base0_row(Base0Op::Embed, &node, None, &profile(), &[], &Empty, 1, (&first, &ids)),
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
            matches!(
                check_execution_step_refutation_v1(&refutation, &NoWeights),
                Err(PalwStepRefuteError::InputSetNotCanonical(_))
            ),
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
