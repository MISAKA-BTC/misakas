//! PALW canonical transcendentals — ADR-0031's program catalog.
//!
//! Every function here is a **transcription of a specific algorithm** the pinned class
//! actually runs — never "an exp". Programs are written in ruleset-v2 reference arithmetic
//! (`palw_reference`), so no hardware float participates in a normative result. Identity is
//! `transcendental_algorithm_id_v1(descriptor)` (the `palw_step` domain); the descriptors
//! implemented here are listed next to their functions.
//!
//! Sources (archived verbatim at transcription time, 2026-08-16):
//! * `ggml_v_expf` / `ggml_v_silu` — the pinned llama.cpp tree (`ggml/src/ggml-cpu/vec.h`,
//!   commit 030ebb558). ADR-0031 Fact 1: the NEON and AVX2 bodies are value-identical per
//!   element (fma argument order commutes; the group-wide fast/slow branch selects between
//!   per-lane-equal expressions), so ONE per-element program serves both CPU classes.
//! * `expf` / `logf` — glibc 2.39 `sysdeps/ieee754/flt-32/{e_expf.c, e_logf.c}` with their
//!   data tables, the algorithms the fleet's libm dispatches (in the FMA multiarch build —
//!   ADR-0031 Fact 2). The `contracted` flag selects the `-mfma`-compiled expression forms
//!   (all eligible `a·b±c` fused, GCC's `-ffp-contract=fast`) vs the baseline object; which
//!   one a class binds is a registration-time disassembly fact.
//!
//! NaN policy: outputs canonicalize (the ruleset rule). glibc's payload-preserving `x + x`
//! differs only for NaN inputs, which can never be committed bytes (fail-closed) — recorded
//! as unobservable in adjudication (ADR-0031).
//!
//! Validation stance: local twins here (a hardware-fma expression mirror for `v_expf`; a
//! ≤ 2-ulp envelope against the HOST libm for the glibc programs — the host is Apple libm,
//! so exact agreement would be the wrong assertion); **exact-bits differential against the
//! class's own binaries is the ADR-0030 §5.1 registration gate**, run on the fleet.

use crate::palw_reference::{
    ref64_add_v2, ref64_fma_v2, ref64_mul_v2, ref64_sub_v2, ref_add_v1, ref_div_v2, ref_fma_v2, ref_mul_v1, ref_narrow_f64_to_f32_v2,
    ref_neg_v1, ref_sub_v1, ref_widen_f32_to_f64_v2, PALW_REFERENCE_CANONICAL_NAN_V1,
};

// ---------------------------------------------------------------------------------------------
// Descriptors (ids come from `palw_step::transcendental_algorithm_id_v1`)
// ---------------------------------------------------------------------------------------------

pub const PALW_TRANSCENDENTAL_DESC_V_EXPF: &str = "source-poly/ggml-v-expf/llama-030ebb558/per-lane/v1";
pub const PALW_TRANSCENDENTAL_DESC_V_SILU: &str = "source-poly/ggml-v-silu/llama-030ebb558/per-lane/v1";
pub const PALW_TRANSCENDENTAL_DESC_GLIBC_EXPF_FMA: &str = "libm/glibc-2.39/expf/fma/v1";
pub const PALW_TRANSCENDENTAL_DESC_GLIBC_EXPF_NOFMA: &str = "libm/glibc-2.39/expf/nofma/v1";
pub const PALW_TRANSCENDENTAL_DESC_GLIBC_LOGF_FMA: &str = "libm/glibc-2.39/logf/fma/v1";
pub const PALW_TRANSCENDENTAL_DESC_GLIBC_LOGF_NOFMA: &str = "libm/glibc-2.39/logf/nofma/v1";
/// Reserved, unimplemented (the RoPE gate — ADR-0031 Fact 5).
pub const PALW_TRANSCENDENTAL_DESC_GLIBC_SINF_RESERVED: &str = "libm/glibc-2.39/sinf/fma/v1";
pub const PALW_TRANSCENDENTAL_DESC_GLIBC_COSF_RESERVED: &str = "libm/glibc-2.39/cosf/fma/v1";

// ---------------------------------------------------------------------------------------------
// f32 bit-level comparison helpers (total functions; NaN compares false, like the hardware
// ordered compares every transcribed branch uses)
// ---------------------------------------------------------------------------------------------

#[inline]
fn is_nan32(bits: u32) -> bool {
    (bits & 0x7FFF_FFFF) > 0x7F80_0000
}

/// Ordered `a > b` on f32 bit patterns (false on any NaN; ±0 equal).
fn f32_gt(a: u32, b: u32) -> bool {
    if is_nan32(a) || is_nan32(b) {
        return false;
    }
    // Signed-magnitude → two's-complement-orderable mapping.
    let key = |x: u32| -> i64 {
        let mag = (x & 0x7FFF_FFFF) as i64;
        if x & 0x8000_0000 != 0 {
            -mag
        } else {
            mag
        }
    };
    key(a) > key(b)
}

/// Ordered `a < b`.
fn f32_lt(a: u32, b: u32) -> bool {
    f32_gt(b, a)
}

/// `|a| > b` for non-negative constant `b` (false on NaN) — the vector compare-absolute.
fn f32_abs_gt(a: u32, b: u32) -> bool {
    if is_nan32(a) {
        return false;
    }
    (a & 0x7FFF_FFFF) > b
}

/// Ordered `a <= +0.0` (the `vclezq`/`_CMP_LE_OQ` predicate; false on NaN).
fn f32_le_zero(a: u32) -> bool {
    if is_nan32(a) {
        return false;
    }
    (a & 0x8000_0000) != 0 || (a & 0x7FFF_FFFF) == 0
}

// ---------------------------------------------------------------------------------------------
// ggml_v_expf / ggml_v_silu — per-element (ADR-0031 Fact 1)
// ---------------------------------------------------------------------------------------------

const VE_MAGIC: u32 = 0x4B40_0000; // 0x1.8p23f
const VE_LOG2E: u32 = 0x3FB8_AA3B; // 0x1.715476p+0f
const VE_LN2_HI: u32 = 0x3F31_7200; // 0x1.62e4p-1f
const VE_LN2_LO: u32 = 0x35BF_BE8E; // 0x1.7f7d1cp-20f
const VE_C1: u32 = 0x3F7F_FFF6; // 0x1.ffffecp-1f
const VE_C2: u32 = 0x3EFF_FEDB; // 0x1.fffdb6p-2f
const VE_C3: u32 = 0x3E2A_AF33; // 0x1.555e66p-3f
const VE_C4: u32 = 0x3D2B_9F17; // 0x1.573e2ep-5f
const VE_C5: u32 = 0x3C07_2010; // 0x1.0e4020p-7f
const VE_ONE: u32 = 0x3F80_0000; // 1.0f
const VE_126: u32 = 0x42FC_0000; // 126.0f
const VE_192: u32 = 0x4340_0000; // 192.0f

/// The vector exp, per element. `source-poly/ggml-v-expf/llama-030ebb558/per-lane/v1`.
pub fn ggml_v_expf_v1(x: u32) -> u32 {
    // z = fma(x, log2e, MAGIC); n = z − MAGIC (the round-to-int trick).
    let z = ref_fma_v2(x, VE_LOG2E, VE_MAGIC);
    let n = ref_sub_v1(z, VE_MAGIC);
    // b = (x − n·ln2_hi) − n·ln2_lo, each a single-rounding fused multiply-subtract.
    let b = ref_fma_v2(ref_neg_v1(n), VE_LN2_LO, ref_fma_v2(ref_neg_v1(n), VE_LN2_HI, x));
    // e = bits(z) << 23; k = bits(e + bits(1.0f)) — exact integer steps.
    let e = z.wrapping_shl(23);
    let k = e.wrapping_add(VE_ONE);
    let big = f32_abs_gt(n, VE_126);
    let u = ref_mul_v1(b, b);
    // j = C1·b + ((C2 + C3·b) + (C4 + C5·b)·u)·u — the C1·b product rounds separately.
    let p1 = ref_fma_v2(VE_C5, b, VE_C4);
    let p2 = ref_fma_v2(VE_C3, b, VE_C2);
    let p3 = ref_fma_v2(p1, u, p2);
    let j = ref_fma_v2(p3, u, ref_mul_v1(VE_C1, b));
    if !big {
        // k + j·k (== k + k·j; multiplication commutes — the two source spellings agree).
        return ref_fma_v2(j, k, k);
    }
    let d: u32 = if f32_le_zero(n) { 0x8200_0000 } else { 0 };
    let s1 = d.wrapping_add(0x7F00_0000);
    let s2 = e.wrapping_sub(d);
    if f32_abs_gt(n, VE_192) {
        ref_mul_v1(s1, s1)
    } else {
        // (s2 + s2·j) · s1
        ref_mul_v1(ref_fma_v2(s2, j, s2), s1)
    }
}

/// The vector SiLU, per element: `x / (1 + v_expf(0 − x))` — `0 − x` transcribed literally
/// (a subtraction, not a negation), true divide.
/// `source-poly/ggml-v-silu/llama-030ebb558/per-lane/v1`.
pub fn ggml_v_silu_v1(x: u32) -> u32 {
    let neg_x = ref_sub_v1(0, x); // 0.0f − x
    let e = ggml_v_expf_v1(neg_x);
    ref_div_v2(x, ref_add_v1(VE_ONE, e))
}

// ---------------------------------------------------------------------------------------------
// glibc 2.39 expf — `libm/glibc-2.39/expf/{fma,nofma}/v1`
// ---------------------------------------------------------------------------------------------

/// `__exp2f_data.tab`, verbatim (32 × u64).
const EXP2F_TAB: [u64; 32] = [
    0x3ff0000000000000,
    0x3fefd9b0d3158574,
    0x3fefb5586cf9890f,
    0x3fef9301d0125b51,
    0x3fef72b83c7d517b,
    0x3fef54873168b9aa,
    0x3fef387a6e756238,
    0x3fef1e9df51fdee1,
    0x3fef06fe0a31b715,
    0x3feef1a7373aa9cb,
    0x3feedea64c123422,
    0x3feece086061892d,
    0x3feebfdad5362a27,
    0x3feeb42b569d4f82,
    0x3feeab07dd485429,
    0x3feea47eb03a5585,
    0x3feea09e667f3bcd,
    0x3fee9f75e8ec5f74,
    0x3feea11473eb0187,
    0x3feea589994cce13,
    0x3feeace5422aa0db,
    0x3feeb737b0cdc5e5,
    0x3feec49182a3f090,
    0x3feed503b23e255d,
    0x3feee89f995ad3ad,
    0x3feeff76f2fb5e47,
    0x3fef199bdd85529c,
    0x3fef3720dcef9069,
    0x3fef5818dcfba487,
    0x3fef7c97337b9b5f,
    0x3fefa4afa2a490da,
    0x3fefd0765b6e4540,
];
/// `invln2_scaled = 0x1.71547652b82fep+0 × 32` (exact power-of-two scale).
const EXPF_INVLN2N: u64 = 0x40471547652B82FE;
/// `shift = 0x1.8p+52`.
const EXPF_SHIFT: u64 = 0x4338000000000000;
/// `poly_scaled = {C0/N³, C1/N², C2/N}` — /N scalings are by powers of two, hence exact.
const EXPF_C0: u64 = 0x3EBC6AF84B912394; // 0x1.c6af84b912394p-5 / 32768
const EXPF_C1: u64 = 0x3F2EBFCE50FAC4F3; // 0x1.ebfce50fac4f3p-3 / 1024
const EXPF_C2: u64 = 0x3F962E42FF0C52D6; // 0x1.62e42ff0c52d6p-1 / 32
const EXPF_ONE64: u64 = 0x3FF0000000000000;

const F32_NEG_INF: u32 = 0xFF80_0000;
const F32_POS_INF: u32 = 0x7F80_0000;
const EXPF_OFLOW_BOUND: u32 = 0x42B1_7217; // 0x1.62e42ep6f  (log 2^128)
const EXPF_UFLOW_BOUND: u32 = 0xC2CF_F1B4; // -0x1.9fe368p6f (log 2^-150)
const EXPF_MAY_UFLOW_BOUND: u32 = 0xC2CE_8ECF; // -0x1.9d1d9ep6f (log 2^-149)

#[inline]
fn top12(bits: u32) -> u32 {
    bits >> 20
}

/// glibc 2.39 `__expf`, transcribed. `contracted` selects the `-mfma` object's fused forms
/// (three sites) vs the baseline; everything else is identical.
pub fn glibc_expf_v1(x: u32, contracted: bool) -> u32 {
    let abstop = top12(x) & 0x7FF;
    if abstop >= top12(0x42B0_0000) {
        // |x| >= 88 or nan.
        if x == F32_NEG_INF {
            return 0;
        }
        if abstop >= top12(F32_POS_INF) {
            // glibc: x + x (payload-preserving quiet). Canonical here; unobservable in
            // adjudication (committed bytes are finite). +Inf stays +Inf.
            return if x == F32_POS_INF { F32_POS_INF } else { PALW_REFERENCE_CANONICAL_NAN_V1 };
        }
        if f32_gt(x, EXPF_OFLOW_BOUND) {
            return F32_POS_INF; // __math_oflowf(0): (0x1p97)² → +Inf
        }
        if f32_lt(x, EXPF_UFLOW_BOUND) {
            return 0; // __math_uflowf(0): (0x1p-95)² → +0
        }
        if f32_lt(x, EXPF_MAY_UFLOW_BOUND) {
            return 0x0000_0001; // __math_may_uflowf(0): (0x1.4p-75)² → min subnormal
        }
        // Otherwise fall through to the main path.
    }
    let xd = ref_widen_f32_to_f64_v2(x);
    // z = InvLn2N · xd
    let z = ref64_mul_v2(EXPF_INVLN2N, xd);
    // kd = (z + SHIFT); ki = bits(kd); kd −= SHIFT  — the ties-to-even round trick.
    let kd_plus = ref64_add_v2(z, EXPF_SHIFT);
    let ki = kd_plus;
    let kd = ref64_sub_v2(kd_plus, EXPF_SHIFT);
    let r = ref64_sub_v2(z, kd);
    // t = T[ki % N] + (ki << 47); s = asdouble(t) — exact integer steps.
    let t = EXP2F_TAB[(ki % 32) as usize].wrapping_add(ki.wrapping_shl(47));
    let s = t;
    // z2 = C0·r + C1;  r2 = r·r;  y = C2·r + 1;  y = z2·r2 + y;  y = y·s
    let r2 = ref64_mul_v2(r, r);
    let y = if contracted {
        let z2 = ref64_fma_v2(EXPF_C0, r, EXPF_C1);
        let y1 = ref64_fma_v2(EXPF_C2, r, EXPF_ONE64);
        ref64_fma_v2(z2, r2, y1)
    } else {
        let z2 = ref64_add_v2(ref64_mul_v2(EXPF_C0, r), EXPF_C1);
        let y1 = ref64_add_v2(ref64_mul_v2(EXPF_C2, r), EXPF_ONE64);
        ref64_add_v2(ref64_mul_v2(z2, r2), y1)
    };
    ref_narrow_f64_to_f32_v2(ref64_mul_v2(y, s))
}

// ---------------------------------------------------------------------------------------------
// glibc 2.39 logf — `libm/glibc-2.39/logf/{fma,nofma}/v1`
// ---------------------------------------------------------------------------------------------

/// `__logf_data.tab`, verbatim (16 × {invc, logc}).
const LOGF_TAB: [(u64, u64); 16] = [
    (0x3FF661EC79F8F3BE, 0xBFD57BF7808CAADE),
    (0x3FF571ED4AAF883D, 0xBFD2BEF0A7C06DDB),
    (0x3FF49539F0F010B0, 0xBFD01EAE7F513A67),
    (0x3FF3C995B0B80385, 0xBFCB31D8A68224E9),
    (0x3FF30D190C8864A5, 0xBFC6574F0AC07758),
    (0x3FF25E227B0B8EA0, 0xBFC1AA2BC79C8100),
    (0x3FF1BB4A4A1A343F, 0xBFBA4E76CE8C0E5E),
    (0x3FF12358F08AE5BA, 0xBFB1973C5A611CCC),
    (0x3FF0953F419900A7, 0xBFA252F438E10C1E),
    (0x3FF0000000000000, 0x0000000000000000),
    (0x3FEE608CFD9A47AC, 0x3FAAA5AA5DF25984),
    (0x3FECA4B31F026AA0, 0x3FBC5E53AA362EB4),
    (0x3FEB2036576AFCE6, 0x3FC526E57720DB08),
    (0x3FE9C2D163A1AA2D, 0x3FCBC2860D224770),
    (0x3FE886E6037841ED, 0x3FD1058BC8A07EE1),
    (0x3FE767DCF5534862, 0x3FD4043057B6EE09),
];
const LOGF_LN2: u64 = 0x3FE62E42FEFA39EF;
const LOGF_A0: u64 = 0xBFD00EA348B88334; // -0x1.00ea348b88334p-2
const LOGF_A1: u64 = 0x3FD5575B0BE00B6A; // 0x1.5575b0be00b6ap-2
const LOGF_A2: u64 = 0xBFDFFFFEF20A4123; // -0x1.ffffef20a4123p-2
const LOGF_OFF: u32 = 0x3F33_0000;
const LOGF_NEG_ONE64: u64 = 0xBFF0000000000000;

/// Exact i32 → binary64 conversion (|k| < 2^31 always fits the 53-bit significand).
fn i32_to_f64_bits(k: i32) -> u64 {
    if k == 0 {
        return 0;
    }
    let sign = if k < 0 { 1u64 << 63 } else { 0 };
    let mag = k.unsigned_abs() as u64;
    let shift = mag.leading_zeros() - 11; // top set bit → bit 52
    let sig = mag << shift; // ∈ [2^52, 2^53), value = sig · 2^(e − 1075) ⟹ e = 1075 − shift
    sign | ((1075 - shift as i32) as u64) << 52 | (sig & 0x000F_FFFF_FFFF_FFFF)
}

/// glibc 2.39 `__logf`, transcribed. Same `contracted` convention (five sites).
pub fn glibc_logf_v1(x: u32, contracted: bool) -> u32 {
    let mut ix = x;
    if ix == 0x3F80_0000 {
        return 0; // WANT_ROUNDING: log(1) = +0 exactly
    }
    if ix.wrapping_sub(0x0080_0000) >= 0x7F80_0000 - 0x0080_0000 {
        // x < 0x1p-126, or inf, or nan.
        if ix.wrapping_mul(2) == 0 {
            return F32_NEG_INF; // __math_divzerof(1): −1/0
        }
        if ix == F32_POS_INF {
            return F32_POS_INF;
        }
        if (ix & 0x8000_0000) != 0 || ix.wrapping_mul(2) >= 0xFF00_0000 {
            return PALW_REFERENCE_CANONICAL_NAN_V1; // __math_invalidf
        }
        // Subnormal: normalize via an exact ×2^23.
        ix = ref_mul_v1(ix, 0x4B00_0000); // x · 0x1p23f (exact)
        ix = ix.wrapping_sub(23 << 23);
    }
    let tmp = ix.wrapping_sub(LOGF_OFF);
    let i = ((tmp >> 19) % 16) as usize;
    let k = (tmp as i32) >> 23;
    let iz = ix.wrapping_sub(tmp & (0x1FF << 23));
    let (invc, logc) = LOGF_TAB[i];
    let z = ref_widen_f32_to_f64_v2(iz);
    // r = z·invc − 1;  y0 = logc + k·Ln2
    let kd = i32_to_f64_bits(k);
    let (r, y0) = if contracted {
        (ref64_fma_v2(z, invc, LOGF_NEG_ONE64), ref64_fma_v2(kd, LOGF_LN2, logc))
    } else {
        (ref64_add_v2(ref64_mul_v2(z, invc), LOGF_NEG_ONE64), ref64_add_v2(logc, ref64_mul_v2(kd, LOGF_LN2)))
    };
    // r2 = r·r;  y = A1·r + A2;  y = A0·r2 + y;  y = y·r2 + (y0 + r)
    let r2 = ref64_mul_v2(r, r);
    let y = if contracted {
        let y1 = ref64_fma_v2(LOGF_A1, r, LOGF_A2);
        let y2 = ref64_fma_v2(LOGF_A0, r2, y1);
        ref64_fma_v2(y2, r2, ref64_add_v2(y0, r))
    } else {
        let y1 = ref64_add_v2(ref64_mul_v2(LOGF_A1, r), LOGF_A2);
        let y2 = ref64_add_v2(ref64_mul_v2(LOGF_A0, r2), y1);
        ref64_add_v2(ref64_mul_v2(y2, r2), ref64_add_v2(y0, r))
    };
    ref_narrow_f64_to_f32_v2(y)
}

// ---------------------------------------------------------------------------------------------
// The scalar op compositions the graph binds (ADR-0030 Fact 15)
// ---------------------------------------------------------------------------------------------

const F32_20: u32 = 0x41A0_0000; // 20.0f

/// `GGML_UNARY_OP_SIGMOID`: `1 / (1 + expf(−x))` — scalar libm expf, true divide.
pub fn ggml_sigmoid_v1(x: u32, contracted: bool) -> u32 {
    let e = glibc_expf_v1(ref_neg_v1(x), contracted);
    ref_div_v2(VE_ONE, ref_add_v1(VE_ONE, e))
}

/// `GGML_UNARY_OP_SOFTPLUS`: `x > 20 ? x : logf(1 + expf(x))` — the threshold branch is an
/// ordered f32 compare (NaN falls to the log path and canonicalizes).
pub fn ggml_softplus_v1(x: u32, contracted: bool) -> u32 {
    if f32_gt(x, F32_20) {
        return x;
    }
    let e = glibc_expf_v1(x, contracted);
    glibc_logf_v1(ref_add_v1(VE_ONE, e), contracted)
}

/// The GDN decay site: one scalar `expf(g)` per (token, head) — ADR-0030 Fact 12.
pub fn gdn_decay_expf_v1(g: u32, contracted: bool) -> u32 {
    glibc_expf_v1(g, contracted)
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Every f32 constant above, re-derived from its C hex-float digits by integer
    /// construction — a transcription typo dies here, not in adjudication.
    #[test]
    fn f32_constants_match_their_hex_float_digits() {
        // (24 fraction bits from the C hex-float's six fraction hexits, exponent p, bits).
        // A representable f32 needs the 24th fraction bit clear; the constructor asserts it.
        let cases: &[(u32, i32, u32)] = &[
            (0x800000, 23, VE_MAGIC),   // 0x1.8p23
            (0x715476, 0, VE_LOG2E),    // 0x1.715476p+0
            (0x62E400, -1, VE_LN2_HI),  // 0x1.62e4p-1
            (0x7F7D1C, -20, VE_LN2_LO), // 0x1.7f7d1cp-20
            (0xFFFFEC, -1, VE_C1),
            (0xFFFDB6, -2, VE_C2),
            (0x555E66, -3, VE_C3),
            (0x573E2E, -5, VE_C4),
            (0x0E4020, -7, VE_C5),
            (0x62E42E, 6, EXPF_OFLOW_BOUND),
            (0x9FE368, 6, EXPF_UFLOW_BOUND & 0x7FFF_FFFF),
            (0x9D1D9E, 6, EXPF_MAY_UFLOW_BOUND & 0x7FFF_FFFF),
            (0x400000, 4, F32_20), // 0x1.4p4 = 20.0
        ];
        for &(frac24, p, expected) in cases {
            assert_eq!(frac24 & 1, 0, "not representable in 23 fraction bits");
            let bits = (((127 + p) as u32) << 23) | (frac24 >> 1);
            assert_eq!(bits, expected, "constant 0x1.{frac24:06x}p{p}");
        }
    }

    #[test]
    fn f64_constants_match_their_hex_float_digits() {
        let cases: &[(u64, i32, u64)] = &[
            (0x1_71547652B82FE, 5, EXPF_INVLN2N), // ×32 = p+5
            (0x1_8000000000000, 52, EXPF_SHIFT),
            (0x1_C6AF84B912394, -20, EXPF_C0), // p-5 / 2^15
            (0x1_EBFCE50FAC4F3, -13, EXPF_C1), // p-3 / 2^10
            (0x1_62E42FF0C52D6, -6, EXPF_C2),  // p-1 / 2^5
            (0x1_62E42FEFA39EF, -1, LOGF_LN2),
            (0x1_5575B0BE00B6A, -2, LOGF_A1),
        ];
        for &(sig53, p, expected) in cases {
            assert!(sig53 >= 1 << 52 && sig53 < 1 << 53);
            let bits = (((1023 + p) as u64) << 52) | (sig53 & 0x000F_FFFF_FFFF_FFFF);
            assert_eq!(bits, expected, "constant 0x{sig53:x}p{p}");
        }
        // Negative ones: same digits with the sign bit.
        assert_eq!(LOGF_A0, (((1023 - 2) as u64) << 52 | 0x00EA348B88334) | (1 << 63));
        assert_eq!(LOGF_A2, (((1023 - 2) as u64) << 52 | 0xFFFFEF20A4123) | (1 << 63));
    }

    /// The hardware twin: the same v_expf expression sequence in native f32 (mul_add is a
    /// true fma on this target). Exact agreement over wide sweeps proves the soft-op wiring;
    /// the class-binary differential on the fleet is the registration gate.
    fn hw_v_expf(x: f32) -> f32 {
        let r = f32::from_bits(VE_MAGIC);
        let z = x.mul_add(f32::from_bits(VE_LOG2E), r);
        let n = z - r;
        let b = (-n).mul_add(f32::from_bits(VE_LN2_LO), (-n).mul_add(f32::from_bits(VE_LN2_HI), x));
        let e = z.to_bits().wrapping_shl(23);
        let k = f32::from_bits(e.wrapping_add(1f32.to_bits()));
        let big = n.abs() > 126.0;
        let u = b * b;
        let p1 = f32::from_bits(VE_C5).mul_add(b, f32::from_bits(VE_C4));
        let p2 = f32::from_bits(VE_C3).mul_add(b, f32::from_bits(VE_C2));
        let p3 = p1.mul_add(u, p2);
        let j = p3.mul_add(u, f32::from_bits(VE_C1) * b);
        if !big {
            return j.mul_add(k, k);
        }
        let d: u32 = if n <= 0.0 { 0x8200_0000 } else { 0 };
        let s1 = f32::from_bits(d.wrapping_add(0x7F00_0000));
        let s2 = f32::from_bits(e.wrapping_sub(d));
        if n.abs() > 192.0 {
            s1 * s1
        } else {
            s2.mul_add(j, s2) * s1
        }
    }

    #[test]
    fn v_expf_matches_the_hardware_twin_exactly() {
        let canon = |x: f32| if x.is_nan() { PALW_REFERENCE_CANONICAL_NAN_V1 } else { x.to_bits() };
        // Dense sweep across the whole meaningful domain plus the branch boundaries.
        let mut x = -200.0f32;
        while x < 200.0 {
            let bits = x.to_bits();
            assert_eq!(ggml_v_expf_v1(bits), canon(hw_v_expf(x)), "x={x}");
            x += 0.037;
        }
        for special in [0f32, -0.0, 126.0, -126.0, 126.5, -126.5, 192.0, 192.5, -192.5, 88.0, -103.0, f32::INFINITY, f32::NEG_INFINITY]
        {
            assert_eq!(ggml_v_expf_v1(special.to_bits()), canon(hw_v_expf(special)), "x={special}");
        }
        // NaN canonicalizes.
        assert_eq!(ggml_v_expf_v1(0x7FC0_1234), PALW_REFERENCE_CANONICAL_NAN_V1);
        // Silu twin.
        let hw_silu = |x: f32| x / (1.0 + hw_v_expf(0.0 - x));
        let mut x = -50.0f32;
        while x < 50.0 {
            assert_eq!(ggml_v_silu_v1(x.to_bits()), canon(hw_silu(x)), "silu x={x}");
            x += 0.11;
        }
    }

    /// Golden anchors for v_expf (frozen 2026-08-16): exp(0)=1 exactly, exp(1), exp(−1).
    #[test]
    fn v_expf_goldens() {
        assert_eq!(ggml_v_expf_v1(0x0000_0000), 0x3F80_0000); // exp(0) = 1
        assert_eq!(ggml_v_expf_v1(0x3F80_0000), hw_v_expf(1.0).to_bits());
        assert_eq!(ggml_v_expf_v1(0xBF80_0000), hw_v_expf(-1.0).to_bits());
    }

    fn ulp_diff(a: u32, b: u32) -> u32 {
        let key = |x: u32| -> i64 {
            let mag = (x & 0x7FFF_FFFF) as i64;
            if x & 0x8000_0000 != 0 {
                -mag
            } else {
                mag
            }
        };
        (key(a) - key(b)).unsigned_abs() as u32
    }

    /// The glibc transcriptions against the HOST libm: a ≤ 2-ulp envelope (the host is Apple
    /// libm; glibc's own budget is 0.502 ulp for expf / 0.818 for logf, so exact equality
    /// would be the wrong assertion) plus exact special cases. Exact-bits validation runs on
    /// the fleet against the real glibc — the registration gate.
    #[test]
    fn glibc_expf_specials_and_envelope() {
        for contracted in [false, true] {
            assert_eq!(glibc_expf_v1(0, contracted), 0x3F80_0000, "exp(0)=1");
            assert_eq!(glibc_expf_v1(0x8000_0000, contracted), 0x3F80_0000, "exp(-0)=1");
            assert_eq!(glibc_expf_v1(F32_POS_INF, contracted), F32_POS_INF);
            assert_eq!(glibc_expf_v1(F32_NEG_INF, contracted), 0);
            assert_eq!(glibc_expf_v1(0x7FC0_0000, contracted), PALW_REFERENCE_CANONICAL_NAN_V1);
            assert_eq!(glibc_expf_v1(0x42B2_0000, contracted), F32_POS_INF, "exp(89) overflows");
            assert_eq!(glibc_expf_v1(0xC2D2_0000, contracted), 0, "exp(-105) underflows to +0");
            // The may-underflow band returns the min subnormal.
            assert_eq!(glibc_expf_v1((-103.5f32).to_bits(), contracted), 0x0000_0001);
            let mut x = -103.0f32;
            let mut checked = 0u32;
            while x < 88.5 {
                let mine = glibc_expf_v1(x.to_bits(), contracted);
                let host = x.exp().to_bits();
                assert!(ulp_diff(mine, host) <= 2, "expf({x}) mine={mine:08x} host={host:08x}");
                checked += 1;
                x += 0.173;
            }
            assert!(checked > 1000);
        }
        // NOTE on the contraction flag: the two variants differ only where a double-rounding
        // boundary falls within ~2^-30 of the f32 rounding decision — glibc's own "wrong
        // count" comments show the wrongly-rounded input sets differ between builds, but a
        // sweep will essentially never hit one. The flag's value is faithfulness to the
        // class's disassembly, not observable divergence density; no discriminator is
        // asserted here.
    }

    #[test]
    fn glibc_logf_specials_and_envelope() {
        for contracted in [false, true] {
            assert_eq!(glibc_logf_v1(0x3F80_0000, contracted), 0, "log(1)=+0");
            assert_eq!(glibc_logf_v1(0, contracted), F32_NEG_INF, "log(+0)=-inf");
            assert_eq!(glibc_logf_v1(0x8000_0000, contracted), F32_NEG_INF, "log(-0)=-inf");
            assert_eq!(glibc_logf_v1(F32_POS_INF, contracted), F32_POS_INF);
            assert_eq!(glibc_logf_v1((-1.0f32).to_bits(), contracted), PALW_REFERENCE_CANONICAL_NAN_V1);
            assert_eq!(glibc_logf_v1(0x7FC0_0000, contracted), PALW_REFERENCE_CANONICAL_NAN_V1);
            let mut x = 1e-40f32; // subnormal region first
            for _ in 0..50 {
                let mine = glibc_logf_v1(x.to_bits(), contracted);
                let host = x.ln().to_bits();
                assert!(ulp_diff(mine, host) <= 2, "logf({x:e}) mine={mine:08x} host={host:08x}");
                x *= 3.7;
            }
            let mut x = 1e-6f32;
            let mut checked = 0u32;
            while x < 1e6 {
                let mine = glibc_logf_v1(x.to_bits(), contracted);
                let host = x.ln().to_bits();
                assert!(ulp_diff(mine, host) <= 2, "logf({x}) mine={mine:08x} host={host:08x}");
                checked += 1;
                x *= 1.0173;
            }
            assert!(checked > 500);
        }
    }

    #[test]
    fn scalar_op_compositions_behave() {
        for contracted in [false, true] {
            // sigmoid(0) = 1/(1+1) = 0.5 exactly.
            assert_eq!(ggml_sigmoid_v1(0, contracted), 0x3F00_0000);
            // softplus threshold: above 20 the input passes through untouched.
            assert_eq!(ggml_softplus_v1(0x41A8_0000, contracted), 0x41A8_0000); // 21.0
            assert_eq!(ggml_softplus_v1(F32_POS_INF, contracted), F32_POS_INF);
            // softplus(0) = ln 2, within the envelope of the host's value.
            let mine = ggml_softplus_v1(0, contracted);
            assert!(ulp_diff(mine, (2f32.ln()).to_bits()) <= 2);
            // NaN canonicalizes through both.
            assert_eq!(ggml_sigmoid_v1(0x7FC0_1111, contracted), PALW_REFERENCE_CANONICAL_NAN_V1);
            assert_eq!(ggml_softplus_v1(0xFFC0_1111, contracted), PALW_REFERENCE_CANONICAL_NAN_V1);
        }
    }

    #[test]
    fn i32_to_f64_is_exact() {
        for k in [-150i32, -127, -23, -1, 1, 2, 23, 127, 150, 12345, -99999] {
            assert_eq!(f64::from_bits(i32_to_f64_bits(k)), k as f64, "k={k}");
        }
        assert_eq!(i32_to_f64_bits(0), 0);
    }
}
