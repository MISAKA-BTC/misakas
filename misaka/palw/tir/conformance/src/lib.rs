//! **Legacy conformance of PALW-TIR v1** (RFC-0002 freeze criterion 6).
//!
//! Every INTEGER kernel of the live court's catalogue — the 38 of `KERNEL_CATALOG` that are not float
//! — and the fenced `RequantizeByToken` are reproduced by a `tir_library_v1` segment, evaluated by the
//! PALW-TIR reference evaluator, byte for byte, on seeded random operands and on the type extremes of
//! the kernel's own domain (the inputs it accepts rather than refuses). The live side is the code
//! that runs in `kaspa-consensus-core` — `palw_base0`, `palw_base0_ops`, `palw_base0_a16`,
//! `palw_qwen36_ops` — called directly, so the claim cannot drift from a copy of it.
//!
//! Every segment is also checked ADMISSIBLE: the range analysis of spec 04b §7 accepts it with its
//! params at their full dtype ranges.
//!
//! **Out of scope, and why.**
//!
//! * The seven float kernels (`KDESC_L2_NORM`, `KDESC_RMS_NORM_FUSED`, `KDESC_SWIGLU`, the glibc
//!   sigmoid and softplus, `GdnCore` NEON and AVX2): an integer IR cannot express them byte for byte
//!   (freeze criterion 6 covers the integer kernels only — RFC-0002 Gate 1 decision 6).
//! * The fenced Kimi K3 arms ([`KIMI_OUT_OF_SCOPE`]) are recorded as DEFECTIVE and are not a
//!   conformance target: `KdaStep` recomputes from a zeroed state with a unit output scale (correct at
//!   position 0 only) and takes one scalar decay where the model's is per channel; `MlaFused` uses one
//!   head dimension for `d_qk` and `d_v` and a hard-coded `>> 8` score scale with `up = 0`; the router
//!   is softmax plus an exact division where the model routes by sigmoid, a selection bias and groups
//!   (corpus-v1 §10 finding 4). No class can reach them (admission refuses a Kimi class until a fence
//!   arms); a Kimi IR class is written with `tir_library_v1`'s MLA, delta-rule and routing templates.
//!
//! The coverage itself is a test: `tests/coverage.rs` hashes [`CONFORMED_V1`] with
//! `kernel_semantics_id_v1` and requires it to be exactly the integer part of
//! `catalogued_kernel_ids_v1()` plus `fenced_kernel_ids_v1()`.

use kaspa_consensus_core::palw_step_refute::*;

/// The kernels this crate conforms, one test each (the test's name says which).
pub const CONFORMED_V1: &[&str] = &[
    // BASE-0 (ADR-0040 D + H): tests/base0.rs.
    KDESC_BASE0_EMBED,
    KDESC_BASE0_MATMUL,
    KDESC_BASE0_REQUANTIZE,
    KDESC_BASE0_RESCALE,
    KDESC_BASE0_RMS_NORM,
    KDESC_BASE0_ROPE,
    KDESC_BASE0_SOFTMAX,
    KDESC_BASE0_SILU,
    KDESC_BASE0_MUL_ELEM,
    KDESC_BASE0_ADD_ELEM,
    // The A16 dense tier: tests/a16.rs.
    KDESC_A16_EMBED,
    KDESC_A16_MATMUL_REQUANT,
    KDESC_A16_MATMUL_RESCALE,
    KDESC_A16_RMS_NORM,
    KDESC_A16_REQUANTIZE,
    KDESC_A16_ADD_ELEM,
    KDESC_A16_SOFTMAX,
    KDESC_A16_ATTN_SCORES,
    KDESC_A16_ATTN_VALUES,
    KDESC_A16_ATTN_FUSED,
    KDESC_A16_ROPE,
    KDESC_A16_MUL_ELEM,
    // Qwen3.6's own ops: tests/q36.rs.
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
    KDESC_Q36_SILU,
    KDESC_Q36_HEAD_RMS_NORM,
    // Fenced (ADR-0102's embedding lift): tests/a16.rs.
    KDESC_A16_REQUANTIZE_BY_TOKEN,
];

/// The float kernels of the catalogue: not an integer IR's to reproduce.
pub const FLOAT_V1: &[&str] = &[
    KDESC_L2_NORM,
    KDESC_RMS_NORM_FUSED,
    KDESC_SWIGLU,
    KDESC_SIGMOID_GLIBC_FMA,
    KDESC_SOFTPLUS_GLIBC_FMA,
    KDESC_GDN_CORE_NEON,
    KDESC_GDN_CORE_AVX2,
];

/// The fenced Kimi K3 arms: defective, recorded, out of scope (see the crate note).
pub const KIMI_OUT_OF_SCOPE: &[&str] = &[KDESC_KIMI_KDA_STEP, KDESC_KIMI_MLA_FUSED, KDESC_KIMI_ROUTER_TOPK, KDESC_KIMI_MOE_COMBINE];
