//! **`tir_library_v1` — the composite templates (RFC-0002 §3.3, spec 04b §11).**
//!
//! Every template here is a *builder*: a method on [`BlockBuilder`] that appends plain PALW-TIR v1
//! primitives to the block being built and returns the node that holds the result. Nothing in this
//! module is a primitive, nothing is known to the court, and nothing is part of a program's identity
//! except the primitives a template emitted — a program built with a template and the same program
//! written out by hand are the same bytes. The library is how lowerers avoid writing the same
//! forty-node subgraph twice; it is not a second instruction set.
//!
//! # Conventions
//!
//! | name | dtype | meaning |
//! | --- | --- | --- |
//! | code | `i16` in `±32767` | an A16 activation at a site's calibrated scale |
//! | w8 | `i8` param | a weight code (per-channel scale in the narrowing) |
//! | acc | `i64` | an exact accumulator (`MatMul`, `ReduceSum`) |
//! | wide | `i128` | a fixed-point product before its rounding; never committed |
//! | q24 | `i32` (or `i64` inside) | Q24 fixed point, `ONE = 2^24`: unit rows, probabilities, gates, decays |
//! | idx | `idx` | tokens, positions, selections |
//!
//! * **A narrowing** is the A16 rule `N[lo,hi](x; m, s, z) = clamp(sat64(HAFZ(x·m / 2^s)) + z)`
//!   ([`BlockBuilder::narrow`]). Its three operands are separately typed tensors — `m` (`i64`), `s`
//!   (any integer type, clamped into `[0, 62]`), `z` (`i64`, optional) — the storage decision of
//!   RFC-0002 Gate 1 (the legacy 17-byte triple is repacked into the three at conversion).
//! * **Params take their dtype's full range** (spec 04b §7): a template that must be admissible for
//!   any registered weight bounds what it relies on with a `Clamp` that never fires for sane data
//!   and makes the range analysis total for insane data.
//! * **A `Clamp` that never fires** states a fact interval analysis cannot see (a probability is at
//!   most `2^25`, `pos mod 2^b` is in `[0, 2^b)`). It is part of the program and changes no value.
//! * **Row operations act on the last axis** and broadcast over the leading ones, so one template
//!   serves a row, a head-major `[heads, d]` tensor and a batch of experts alike.
//! * **The builder panics on misuse** (a shape that does not fit): templates are construction-time
//!   code whose output [`crate::validate::validate`] checks before anything runs it.
//!
//! # The catalogue
//!
//! [`LIBRARY_V1`] lists every template with the section it belongs to and, where it reproduces a
//! live kernel byte for byte, that kernel. Byte identity is TESTED against the live code by the
//! `misaka-palw-tir-conformance` crate; the other templates are tested for admissibility (the range
//! rules of spec 04b §7 at full param ranges) and against a float reference in `tests/library.rs`.

use crate::builder::BlockBuilder;
use crate::program::Ref;
use crate::types::DType;

pub mod act;
pub mod attn;
pub mod legacy;
pub mod moe;
pub mod norm;
pub mod recur;
pub mod rope;
pub mod softmax;

/// The operands of one narrowing, as separately typed tensors that broadcast against the value.
#[derive(Clone, Copy, Debug)]
pub struct Narrowing {
    /// The multiplier, `i64` (any integer type is accepted).
    pub m: Ref,
    /// The right shift, any integer type; clamped into `[0, 62]` before it becomes `2^s`.
    pub s: Ref,
    /// The additive term at the output scale (a bias), or none.
    pub z: Option<Ref>,
}

impl Narrowing {
    pub fn new(m: Ref, s: Ref, z: Option<Ref>) -> Self {
        Self { m, s, z }
    }
}

/// One catalogue entry: `(template, section, the live kernel it reproduces byte for byte or "—")`.
pub type LibraryEntry = (&'static str, &'static str, &'static str);

/// Every template of `tir_library_v1`.
pub const LIBRARY_V1: &[LibraryEntry] = &[
    // The legacy rounding rules and kernels (spec 04b §11.1; conformance: misaka-palw-tir-conformance).
    ("narrow / narrow_a16", "legacy", "a16_scale_round + saturating zero + clamp"),
    ("requantize_base0 / requantize_base0_t", "legacy", "palw_base0::requantize_with_zero (BASE-0 op 2)"),
    ("rescale_base0 / rescale_base0_t", "legacy", "palw_base0::rescale_q (BASE-0 op 9)"),
    ("int_recip", "legacy", "palw_base0::int_recip"),
    ("int_sigmoid", "legacy", "palw_base0_ops::int_sigmoid, q36_sigmoid_gate"),
    ("silu", "legacy", "palw_base0_ops::silu (BASE-0 op 6, q36/silu)"),
    ("softmax_shifted", "legacy", "palw_base0_ops::softmax / softmax_shifted (ops 5, 5W), a16_softmax_rows"),
    ("embed", "legacy", "embed_lookup (BASE-0 op 0), the A16 gather"),
    ("base0_matmul", "legacy", "palw_base0_ops::matmul_quant (BASE-0 op 1)"),
    ("base0_rms_norm", "legacy", "palw_base0_ops::rms_norm (BASE-0 op 3)"),
    ("rope_pairs_wide", "legacy", "palw_base0_ops::rope_table (BASE-0 op 4)"),
    ("base0_mul_elem / base0_add_elem", "legacy", "palw_base0_ops::mul_elem / add_elem (ops 7, 8)"),
    ("a16_matmul (requant / rescale)", "legacy", "a16_matmul_requant, a16_matmul_rescale"),
    ("rms_norm_a16", "legacy", "a16_rms_norm (and the head-sliced norm)"),
    ("rope_pairs", "legacy", "a16_rope"),
    ("a16_add_elem / a16_mul_elem", "legacy", "a16_add_elem, a16_mul_elem"),
    (
        "a16_attn_scores / a16_attn_values / a16_attn_fused",
        "legacy",
        "a16_attn_scores, a16_attn_values_within, a16_attn_fused_reference_within_v1",
    ),
    ("q36_matmul_grouped", "legacy", "q36_matmul_grouped, q36_matmul_grouped_wide"),
    ("q36_rope_partial", "legacy", "q36_rope_partial"),
    ("q36_ssm_conv", "legacy", "q36_ssm_conv"),
    ("l2_norm_q15", "legacy", "q36_l2_norm"),
    ("l2_unit_q15 (the 17-node form)", "legacy", "q36_l2_norm"),
    ("q36_gate_apply / q36_mul_wide / q36_rescale_row", "legacy", "q36_gate_apply, q36_mul_wide, q36_rescale_row"),
    ("rms_norm_wide_q36", "legacy", "q36_rms_norm_wide"),
    ("rms_unit_q24 (the 21-node form, one i64 eps)", "legacy", "q36_rms_norm_wide"),
    ("router_topk_q36", "legacy", "q36_router_topk"),
    ("moe_combine_q36", "legacy", "q36_moe_combine"),
    ("softplus_q36 / exp_refined_q36 / decay_q36", "legacy", "q36_softplus, q36_exp_refined, q36_decay"),
    ("gdn_step_q36", "legacy", "q36_gdn_step"),
    ("requantize_by_token", "legacy", "the fenced RequantizeByToken arm (ADR-0102)"),
    // Norms.
    ("rms_norm_wide_q36 / rms_norm_wide_q36_exact", "norm", "the RMSNorm of any wide row (the legacy form is the general one)"),
    ("layer_norm_exact", "norm", "—"),
    ("group_norm_exact", "norm", "—"),
    ("l2_norm_eps", "norm", "—"),
    // Softmax variants.
    ("softmax_with_sink", "softmax", "—"),
    ("softcap_q24", "softmax", "—"),
    // Activations.
    ("tanh_q24", "act", "—"),
    ("gelu_tanh_q24 / gelu_erf_q24 / quick_gelu_q24", "act", "—"),
    ("relu / relu2", "act", "—"),
    ("swiglu_clamped_q24", "act", "—"),
    ("act_table / act_table_wide", "act", "—"),
    // Rotary and position.
    ("rope_half / rope_partial", "rope", "—"),
    ("rope_angles_two_level", "rope", "—"),
    ("rope_angles_by_position", "rope", "—"),
    ("alibi", "rope", "—"),
    // Attention.
    ("attention", "attn", "—"),
    ("mla_absorbed", "attn", "—"),
    // Routing.
    ("route_sigmoid", "moe", "—"),
    ("grouped_topk", "moe", "—"),
    ("renormalize_div / renormalize_recip / scale_q24", "moe", "—"),
    // Recurrences.
    ("map_heads_group / map_heads_tile", "recur", "—"),
    ("causal_conv", "recur", "—"),
    ("token_shift / lerp_q24", "recur", "—"),
    ("exp_q24 / exp_neg_exp_q24", "recur", "—"),
    ("mamba1_step / mamba2_step", "recur", "—"),
    ("rwkv4_step / rwkv6_step / rwkv7_step", "recur", "—"),
];

impl BlockBuilder<'_> {
    /// **The narrowing** `N[lo,hi](x; m, s, z) = clamp_[lo,hi]( sat64( HAFZ(x·m / 2^s) ) + z )` —
    /// [`Self::narrow_a16`] with the shift as a tensor of shift AMOUNTS (clamped into `[0, 62]`
    /// through the pinned `Pow2` table) and an optional `z`.
    pub fn narrow(&mut self, x: Ref, n: &Narrowing, lo: i64, hi: i64, dtype: DType) -> Ref {
        let p2 = self.pow2_of(n.s);
        match n.z {
            Some(z) => self.narrow_a16(x, n.m, p2, z, lo, hi, dtype),
            // **Without a zero term, three nodes**: the template's `Clamp_i64 → Add 0 → Clamp[lo, hi]`
            // is exactly `Clamp[lo, hi]` — `[lo, hi] ⊆ i64`, so clamping into `i64` first changes
            // nothing a clamp into `[lo, hi]` keeps — two nodes fewer, every value identical
            // (`misaka-palw-tir-conformance` holds the two forms and the live kernel equal).
            None => {
                let p = self.mul(x, n.m, DType::I128);
                let q = self.div(p, p2, crate::prim::Rounding::HalfAwayFromZero, DType::I128);
                self.clamp(q, lo, hi, dtype)
            }
        }
    }

    /// [`Self::narrow`] to A16 codes (`i16`, `±32767`).
    pub fn narrow_codes(&mut self, x: Ref, n: &Narrowing) -> Ref {
        self.narrow(x, n, -32767, 32767, DType::I16)
    }

    /// [`Self::narrow`] to the `i32` rail (a wide value, or Q24).
    pub fn narrow_wide(&mut self, x: Ref, n: &Narrowing) -> Ref {
        self.narrow(x, n, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// `max(a, b)` elementwise — `Select(Compare(a ≥ b), a, b)` (no primitive, corpus §6.1).
    pub fn max2(&mut self, a: Ref, b: Ref, dtype: DType) -> Ref {
        let ge = self.compare(a, b, crate::prim::Cmp::Ge);
        self.select(ge, a, b, dtype)
    }

    /// `min(a, b)` elementwise.
    pub fn min2(&mut self, a: Ref, b: Ref, dtype: DType) -> Ref {
        let le = self.compare(a, b, crate::prim::Cmp::Le);
        self.select(le, a, b, dtype)
    }

    /// `(a · b) >> 24`, floor, into `dtype` through an `i128` product — the Q24 product.
    pub fn mul_q24(&mut self, a: Ref, b: Ref, dtype: DType) -> Ref {
        let p = self.mul(a, b, DType::I128);
        let q = self.shr(p, crate::arith::K, crate::prim::Rounding::Floor, DType::I128);
        let (lo, hi) = (dtype.min_value().max(i64::MIN as i128) as i64, dtype.max_value().min(i64::MAX as i128) as i64);
        self.clamp(q, lo, hi, dtype)
    }
}
