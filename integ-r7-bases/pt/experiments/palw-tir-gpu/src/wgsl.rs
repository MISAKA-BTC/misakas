//! **The WGSL sources: one exact integer library and the kernel templates.**
//!
//! # The integer library ([`INTLIB`])
//!
//! Every lossy site of spec 04b is written here once, as a function of `i64` (and `u64`) words,
//! transcribed from the CPU executor's native-width functions (`misaka_palw_tir_exec::scalar`,
//! themselves held equal to the reference's `i128` definitions): the three rounding rules of `Div`,
//! `Log2Floor`, and the fixed-iteration `IntExp`, `IntRsqrt`, `IntLn` (PALW-TIR-25/26). Three rules
//! keep the translation exact:
//!
//! * **WGSL integer arithmetic is two's complement and wrapping** (naga's MSL output casts signed
//!   operands to unsigned to keep it so). Every product and sum here is either proved inside its
//!   width (bounds stated per function, the same as `scalar.rs`'s) or is a wrapping ring operation
//!   whose true result is known to fit (a remainder `x − q·d` in `[0, d)`).
//! * **No `i64`/`u64` `/` or `%`.** naga 30's MSL helper for 64-bit division does not compile
//!   (`metal::select(rhs, 1, …)` is ambiguous between `int` and `long`), so 64-bit division is
//!   [`udiv64`](INTLIB): a 32-bit hardware division when both operands fit, else restoring long
//!   division — exact by construction and held against Rust by `tests/intlib.rs`. 32-bit division is
//!   used only where both operands are proved non-negative and below `2^32`.
//! * **No shift by the width or more, and no `select` over a shift** (both operands of a WGSL
//!   `select` are evaluated): every shift amount is a proved value in `[0, 63]`, branched on.
//!
//! # Kernels
//!
//! Every kernel binds: `0` the parameter words `P` (shapes, strides, offsets, constants), `1` the
//! output `O`, `2` the status words `S`, then its operands from `3`. A failing element records its
//! row-major output index and its class in the node's status word as `atomicMax(S[slot], ~(index·4 +
//! class))` — so the word ends holding the FIRST failing element in output order, the element whose
//! class the reference and the CPU executor report (spec 04b §9.3; classes: 1 `Overflow`, 2
//! `Divisor`, 3 `Index`). A word left at 0 means the node succeeded.

use crate::tensor::Form;

/// The exact integer library every kernel includes.
pub const INTLIB: &str = r#"
const ONE: i64 = 16777216li;
const LN2_Q: i64 = 11629080li;
const POLY2_A: i64 = 6014632li;
const POLY2_B: i64 = 22699573li;
const POLY2_C: i64 = 5771362li;
const EXP_ZERO_AT: i64 = -360501480li;

// |x| as an unsigned word (|i64::MIN| = 2^63 fits).
fn uabs64(x: i64) -> u64 {
    if (x < 0li) { return u64(-(x + 1li)) + 1lu; }
    return u64(x);
}

// ⌊n / d⌋ for d ≥ 1.
fn udiv64(n: u64, d: u64) -> u64 {
    if ((n >> 32u) == 0lu && (d >> 32u) == 0lu) {
        return u64(u32(n) / u32(d));
    }
    if (d > n) { return 0lu; }
    if ((d >> 63u) != 0lu) { return 1lu; }
    // d < 2^63, so the partial remainder r < d never loses its top bit to the shift.
    var q = 0lu;
    var r = 0lu;
    var i = 64u - u32(countLeadingZeros(n));
    loop {
        if (i == 0u) { break; }
        i = i - 1u;
        r = (r << 1u) | ((n >> i) & 1lu);
        if (r >= d) {
            r = r - d;
            q = q | (1lu << i);
        }
    }
    return q;
}

// ⌊x / d⌋ for d ≥ 1 (floor, also for negative x: ⌊x/d⌋ = −⌊(|x| − 1)/d⌋ − 1).
fn floordiv64(x: i64, d: i64) -> i64 {
    if (x >= 0li) { return i64(udiv64(u64(x), u64(d))); }
    return -i64(udiv64(u64(-(x + 1li)), u64(d))) - 1li;
}

// round_rule(x / 2^s), 0 ≤ s ≤ 62; rule 0 Floor, 1 HalfUp, 2 HalfAwayFromZero (the magnitude).
fn shr_rule64(x: i64, s: u32, rule: u32) -> i64 {
    if (s == 0u) { return x; }
    if (rule == 0u) { return x >> s; }
    if (rule == 1u) { return (x >> s) + ((x >> (s - 1u)) & 1li); }
    let m = uabs64(x);
    let q = (m >> s) + ((m >> (s - 1u)) & 1lu);
    if (x < 0li) { return -i64(q); }
    return i64(q);
}

// round_rule(x / d) for d ≥ 1 (spec 04b §6.4), exactly scalar::div_round_i64.
fn div_rule64(x: i64, d: i64, rule: u32) -> i64 {
    let ud = u64(d);
    if ((ud & (ud - 1lu)) == 0lu) { return shr_rule64(x, u32(countTrailingZeros(ud)), rule); }
    if (rule == 2u) {
        let m = uabs64(x);
        var q = udiv64(m, ud);
        let r = m - q * ud;
        if (r >= ud - r) { q = q + 1lu; }
        if (x < 0li) { return -i64(q); }
        return i64(q);
    }
    let q = floordiv64(x, d);
    if (rule == 0u) { return q; }
    // The remainder x − q·d lies in [0, d): exact in the wrapping ring whatever q·d does.
    let r = x - q * d;
    if (r >= d - r) { return q + 1li; }
    return q;
}

fn log2_floor64(x: i64) -> i64 {
    if (x <= 0li) { return -1li; }
    return 63li - i64(countLeadingZeros(u64(x)));
}

// IntExp (04b §6.5): z ≤ 30, t < 2^25 so t² < 2^50, A·⌊t²/2^24⌋ < 2^49.
fn int_exp64(x0: i64) -> i64 {
    let x = min(x0, 0li);
    if (x <= EXP_ZERO_AT) { return 0li; }
    // −x < 31·LN2_Q < 2^29: a 32-bit division of non-negative operands.
    let z = i64(u32(-x) / 11629080u);
    let p = x + z * LN2_Q;
    let t = p + POLY2_B;
    let square = (t * t) >> 24u;
    let poly = ((POLY2_A * square) >> 24u) + POLY2_C;
    if (z == 0li) { return poly; }
    let zu = u32(z);
    return (poly >> zu) + ((poly >> (zu - 1u)) & 1li);
}

// IntRsqrt (04b §6.5): m < 2^26, the Newton value below 2^26, every product below 2^56.
fn int_rsqrt64(v: i64) -> i64 {
    if (v <= 0li) { return 0li; }
    let bit = log2_floor64(v);
    var e = (bit - 24li) >> 1u;
    var m: i64;
    if (e >= 0li) { m = v >> u32(2li * e); } else { m = v << u32(-2li * e); }
    for (var i = 0u; i < 40u; i = i + 1u) {
        if (m < 4li * ONE) { break; }
        m = m >> 2u;
        e = e + 1li;
    }
    for (var i = 0u; i < 40u; i = i + 1u) {
        if (m >= ONE) { break; }
        m = m << 2u;
        e = e - 1li;
    }
    // m ∈ [2^24, 2^26): (m − ONE)·16 < 2^30, a 32-bit division of non-negative operands.
    var index = 0u;
    if (m > ONE) { index = min(u32(m - ONE) * 16u / 50331648u, 15u); }
    var seeds = array<i64, 16>(15395829li, 14307657li, 13421772li, 12682383li, 12053107li, 11509075li,
        11032629li, 10610843li, 10234005li, 9894662li, 9586980li, 9306325li, 9048957li, 8811825li,
        8592409li, 8388608li);
    var y = seeds[index];
    for (var it = 0u; it < 3u; it = it + 1u) {
        let y2 = (y * y) >> 24u;
        let my2 = (m * y2) >> 24u;
        y = (y * (3li * ONE - my2)) >> 25u;
        if (y <= 0li) { y = 1li; }
    }
    if (e >= 0li) { return y >> u32(e); }
    return y << u32(-e);
}

// IntLn (04b §6.5): m < 2^25, t < 2^24/3, every product below 2^48; the series terms are
// non-negative and below 2^24 (32-bit divisions).
fn int_ln64(x: i64) -> i64 {
    if (x <= 0li) { return 0li; }
    let s = log2_floor64(x) - 24li;
    var m: i64;
    if (s >= 0li) { m = x >> u32(s); } else { m = x << u32(-s); }
    let t = i64(udiv64(u64((m - ONE) << 24u), u64(m + ONE)));
    let t2 = (t * t) >> 24u;
    var term = t;
    var sum = t;
    term = (term * t2) >> 24u; sum = sum + i64(u32(term) / 3u);
    term = (term * t2) >> 24u; sum = sum + i64(u32(term) / 5u);
    term = (term * t2) >> 24u; sum = sum + i64(u32(term) / 7u);
    term = (term * t2) >> 24u; sum = sum + i64(u32(term) / 9u);
    term = (term * t2) >> 24u; sum = sum + i64(u32(term) / 11u);
    return 2li * sum + s * LN2_Q;
}
"#;

/// The exact 128-bit library: a value is `W { lo, hi }`, two's complement over two `u64` words.
/// Addition, subtraction and the low 128 bits of a product are ring operations mod 2^128 — exact
/// wherever the plan proved the true result inside `i128` (a node with `checked_arith` is never run
/// here); products of two words are formed from four 32×32 partial products; shifts branch on
/// the amount (never a shift by 64 or more); division is restoring long division over 128 bits.
pub const INTLIB128: &str = r#"
struct W { lo: u64, hi: u64 }

fn wi(x: i64) -> W { return W(bitcast<u64>(x), bitcast<u64>(x >> 63u)); }
fn wu(x: u64) -> W { return W(x, 0lu); }
fn w_add(a: W, b: W) -> W { let lo = a.lo + b.lo; return W(lo, a.hi + b.hi + select(0lu, 1lu, lo < a.lo)); }
fn w_sub(a: W, b: W) -> W { return W(a.lo - b.lo, a.hi - b.hi - select(0lu, 1lu, a.lo < b.lo)); }
fn w_neg(a: W) -> W { return w_sub(W(0lu, 0lu), a); }
fn w_isneg(a: W) -> bool { return (a.hi >> 63u) == 1lu; }
fn w_iszero(a: W) -> bool { return a.lo == 0lu && a.hi == 0lu; }
fn w_eq(a: W, b: W) -> bool { return a.lo == b.lo && a.hi == b.hi; }
// Signed a < b.
fn w_lt(a: W, b: W) -> bool {
    let ah = bitcast<i64>(a.hi); let bh = bitcast<i64>(b.hi);
    return ah < bh || (ah == bh && a.lo < b.lo);
}
// Unsigned a < b.
fn w_ult(a: W, b: W) -> bool { return a.hi < b.hi || (a.hi == b.hi && a.lo < b.lo); }
fn w_min(a: W, b: W) -> W { if (w_lt(b, a)) { return b; } return a; }
fn w_max(a: W, b: W) -> W { if (w_lt(a, b)) { return b; } return a; }
// The full 128-bit product of two words.
fn w_mul64(a: u64, b: u64) -> W {
    let a0 = a & 0xFFFFFFFFlu; let a1 = a >> 32u; let b0 = b & 0xFFFFFFFFlu; let b1 = b >> 32u;
    let p00 = a0 * b0; let p01 = a0 * b1; let p10 = a1 * b0; let p11 = a1 * b1;
    let mid = (p00 >> 32u) + (p01 & 0xFFFFFFFFlu) + (p10 & 0xFFFFFFFFlu);
    return W((p00 & 0xFFFFFFFFlu) | (mid << 32u), p11 + (p01 >> 32u) + (p10 >> 32u) + (mid >> 32u));
}
// The low 128 bits of a·b: the signed product whenever it fits i128.
fn w_mul(a: W, b: W) -> W { let p = w_mul64(a.lo, b.lo); return W(p.lo, p.hi + a.hi * b.lo + a.lo * b.hi); }
fn w_shl(a: W, s: u32) -> W {
    if (s == 0u) { return a; }
    if (s < 64u) { return W(a.lo << s, (a.hi << s) | (a.lo >> (64u - s))); }
    return W(0lu, a.lo << (s - 64u));
}
fn w_shr_u(a: W, s: u32) -> W {
    if (s == 0u) { return a; }
    if (s < 64u) { return W((a.lo >> s) | (a.hi << (64u - s)), a.hi >> s); }
    return W(a.hi >> (s - 64u), 0lu);
}
fn w_shr_s(a: W, s: u32) -> W {
    if (s == 0u) { return a; }
    let hs = bitcast<i64>(a.hi);
    if (s < 64u) { return W((a.lo >> s) | (a.hi << (64u - s)), bitcast<u64>(hs >> s)); }
    return W(bitcast<u64>(hs >> (s - 64u)), bitcast<u64>(hs >> 63u));
}
fn w_bit(a: W, i: u32) -> u64 { if (i < 64u) { return (a.lo >> i) & 1lu; } return (a.hi >> (i - 64u)) & 1lu; }
// |x| as an unsigned 128-bit value (|i128::MIN| = 2^127).
fn w_uabs(x: W) -> W { if (w_isneg(x)) { return w_neg(x); } return x; }
fn w_clz(a: W) -> u32 { if (a.hi != 0lu) { return u32(countLeadingZeros(a.hi)); } return 64u + u32(countLeadingZeros(a.lo)); }
fn w_log2(x: W) -> i64 { if (w_isneg(x) || w_iszero(x)) { return -1li; } return 127li - i64(w_clz(x)); }
// round_rule(x / 2^s), 0 ≤ s ≤ 126.
fn w_shr_rule(x: W, s: u32, rule: u32) -> W {
    if (s == 0u) { return x; }
    if (rule == 0u) { return w_shr_s(x, s); }
    if (rule == 1u) { return w_add(w_shr_s(x, s), wu(w_bit(x, s - 1u))); }
    let m = w_uabs(x);
    let q = w_add(w_shr_u(m, s), wu(w_bit(m, s - 1u)));
    if (w_isneg(x)) { return w_neg(q); }
    return q;
}
// ⌊n / d⌋, unsigned, d ≥ 1.
fn w_udiv(n: W, d: W) -> W {
    if (n.hi == 0lu && d.hi == 0lu) { return wu(udiv64(n.lo, d.lo)); }
    if (w_ult(n, d)) { return W(0lu, 0lu); }
    if ((d.hi >> 63u) != 0lu) { return wu(1lu); }
    var q = W(0lu, 0lu);
    var r = W(0lu, 0lu);
    var i = 128u - w_clz(n);
    loop {
        if (i == 0u) { break; }
        i = i - 1u;
        r = w_shl(r, 1u);
        r.lo = r.lo | w_bit(n, i);
        if (!w_ult(r, d)) {
            r = w_sub(r, d);
            if (i < 64u) { q.lo = q.lo | (1lu << i); } else { q.hi = q.hi | (1lu << (i - 64u)); }
        }
    }
    return q;
}
// round_rule(x / d) for d ≥ 1: arith::div_round on 128 bits.
fn w_div_rule(x: W, d: W, rule: u32) -> W {
    if (d.hi == 0lu && (d.lo & (d.lo - 1lu)) == 0lu) { return w_shr_rule(x, u32(countTrailingZeros(d.lo)), rule); }
    if (d.lo == 0lu && (d.hi & (d.hi - 1lu)) == 0lu) { return w_shr_rule(x, 64u + u32(countTrailingZeros(d.hi)), rule); }
    if (rule == 2u) {
        let m = w_uabs(x);
        var q = w_udiv(m, d);
        let r = w_sub(m, w_mul(q, d));
        if (!w_ult(r, w_sub(d, r))) { q = w_add(q, wu(1lu)); }
        if (w_isneg(x)) { return w_neg(q); }
        return q;
    }
    var q: W;
    if (!w_isneg(x)) { q = w_udiv(x, d); } else { q = w_sub(w_neg(w_udiv(w_neg(w_add(x, wu(1lu))), d)), wu(1lu)); }
    if (rule == 0u) { return q; }
    let r = w_sub(x, w_mul(q, d));
    if (!w_ult(r, w_sub(d, r))) { return w_add(q, wu(1lu)); }
    return q;
}
"#;

/// The bindings and helpers every kernel starts with: `P`, `O` (of `out`), `S`, the operands, a
/// 64-bit constant reader and `fail`.
pub fn header(out: Form, ins: &[Form]) -> String {
    let mut s = header_base(out);
    for (k, f) in ins.iter().enumerate() {
        s.push_str(&loader(k, *f));
    }
    s
}

/// [`header`] without the operand bindings (a kernel that declares its own views of them).
pub fn header_base(out: Form) -> String {
    let mut s = String::new();
    s.push_str("@group(0) @binding(0) var<storage, read> P: array<u32>;\n");
    s.push_str(&format!("@group(0) @binding(1) var<storage, read_write> O: array<{}>;\n", out.wgsl_array()));
    s.push_str("@group(0) @binding(2) var<storage, read_write> S: array<atomic<u32>>;\n");
    s.push_str(&storer(out));
    s.push_str(
        r#"
fn p64(i: u32) -> i64 { return bitcast<i64>((u64(P[i + 1u]) << 32u) | u64(P[i])); }
fn fail(e: u32, kind: u32) { atomicMax(&S[P[1]], ~((e << 2u) | kind)); }
"#,
    );
    s.push_str(INTLIB);
    s.push_str(INTLIB128);
    s.push_str("fn p128(i: u32) -> W { return W((u64(P[i + 1u]) << 32u) | u64(P[i]), (u64(P[i + 3u]) << 32u) | u64(P[i + 2u])); }\n");
    s.push_str(&storer128(out));
    s
}

/// Binding `3 + k` and its readers: `ld{k}(i) -> i64` (element `i` as its mathematical value) and
/// `ld32_{k}(i) -> i32` (the same value, for operands the plan proved inside `i32`).
pub fn loader(k: usize, f: Form) -> String {
    let b = 3 + k;
    let decl = format!("@group(0) @binding({b}) var<storage, read> B{k}: array<{}>;\n", f.wgsl_array());
    let body = match f {
        Form::S32 => format!(
            "fn ld32_{k}(i: u32) -> i32 {{ return bitcast<i32>(B{k}[i]); }}\nfn ld{k}(i: u32) -> i64 {{ return i64(bitcast<i32>(B{k}[i])); }}\n"
        ),
        Form::U32 => format!(
            "fn ld32_{k}(i: u32) -> i32 {{ return bitcast<i32>(B{k}[i]); }}\nfn ld{k}(i: u32) -> i64 {{ return i64(B{k}[i]); }}\n"
        ),
        Form::I64 => format!(
            "fn ld32_{k}(i: u32) -> i32 {{ return bitcast<i32>(u32(bitcast<u64>(B{k}[i]) & 0xFFFFFFFFlu)); }}\nfn ld{k}(i: u32) -> i64 {{ return B{k}[i]; }}\n"
        ),
        Form::P8 => format!(
            "fn ld32_{k}(i: u32) -> i32 {{ return extractBits(bitcast<i32>(B{k}[i >> 2u]), (i & 3u) * 8u, 8u); }}\nfn ld{k}(i: u32) -> i64 {{ return i64(ld32_{k}(i)); }}\n"
        ),
        Form::P16 => format!(
            "fn ld32_{k}(i: u32) -> i32 {{ return extractBits(bitcast<i32>(B{k}[i >> 1u]), (i & 1u) * 16u, 16u); }}\nfn ld{k}(i: u32) -> i64 {{ return i64(ld32_{k}(i)); }}\n"
        ),
        // An i128 operand is read whole by `ld128`; `ld`/`ld32` read its low word, which only a
        // kernel whose plan proved the value inside i64 (resp. i32) may use.
        Form::I128 => format!(
            "fn ld128_{k}(i: u32) -> W {{ return W(B{k}[2u * i], B{k}[2u * i + 1u]); }}\nfn ld{k}(i: u32) -> i64 {{ return bitcast<i64>(B{k}[2u * i]); }}\nfn ld32_{k}(i: u32) -> i32 {{ return bitcast<i32>(u32(B{k}[2u * i] & 0xFFFFFFFFlu)); }}\n"
        ),
    };
    let wide = if f == Form::I128 { String::new() } else { format!("fn ld128_{k}(i: u32) -> W {{ return wi(ld{k}(i)); }}\n") };
    decl + &body + &wide
}

/// `st(i, v)`: element `i` of `O` holds `v` (which the plan proved, or the kernel checked, to be a
/// value of the output's dtype — a lane keeps its low 32 bits).
pub fn storer(f: Form) -> String {
    match f {
        Form::S32 | Form::U32 => "fn st(i: u32, v: i64) { O[i] = u32(bitcast<u64>(v) & 0xFFFFFFFFlu); }\n".to_string(),
        Form::I64 => "fn st(i: u32, v: i64) { O[i] = v; }\n".to_string(),
        Form::I128 => "fn st(i: u32, v: i64) { O[2u * i] = bitcast<u64>(v); O[2u * i + 1u] = bitcast<u64>(v >> 63u); }\n".to_string(),
        Form::P8 | Form::P16 => unreachable!("a kernel never writes a packed form"),
    }
}

/// `st128(i, v)`: element `i` of `O` holds the 128-bit `v` (proved, or checked, to be a value of the
/// output's dtype; a narrower form keeps its low bits).
pub fn storer128(f: Form) -> String {
    match f {
        Form::S32 | Form::U32 => "fn st128(i: u32, v: W) { O[i] = u32(v.lo & 0xFFFFFFFFlu); }\n".to_string(),
        Form::I64 => "fn st128(i: u32, v: W) { O[i] = bitcast<i64>(v.lo); }\n".to_string(),
        Form::I128 => "fn st128(i: u32, v: W) { O[2u * i] = v.lo; O[2u * i + 1u] = v.hi; }\n".to_string(),
        Form::P8 | Form::P16 => unreachable!("a kernel never writes a packed form"),
    }
}

/// The output's multi-index (right-aligned to rank 4) and every operand's element offset, from the
/// parameter block's geometry: `P[2..6]` the output dims, `P[6..11]` the output's `(base, strides)`,
/// and `P[11 + 5k ..]` operand `k`'s.
pub fn index_prelude(n_ops: usize) -> String {
    let mut s = String::from(
        "    let d3 = P[5]; let d2 = P[4]; let d1 = P[3];\n\
         \x20   let i3 = e % d3; let r3 = e / d3; let i2 = r3 % d2; let r2 = r3 / d2; let i1 = r2 % d1; let i0 = r2 / d1;\n\
         \x20   let off_o = P[6] + i0 * P[7] + i1 * P[8] + i2 * P[9] + i3 * P[10];\n",
    );
    for k in 0..n_ops {
        let b = 11 + 5 * k;
        s.push_str(&format!(
            "    let off{k} = P[{b}] + i0 * P[{}] + i1 * P[{}] + i2 * P[{}] + i3 * P[{}];\n",
            b + 1,
            b + 2,
            b + 3,
            b + 4
        ));
    }
    s
}

/// The elementwise operations (one output element per invocation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EwOp {
    /// `v = x0` (Cast, a materialised view, a history row).
    Copy,
    Add,
    Sub,
    Mul,
    /// `Div` under rule 0/1/2.
    Div(u8),
    /// `Compare` with the relation's tag (Eq … Ge).
    Compare(u8),
    /// `x0 ≠ 0 ? x1 : x2`.
    Select,
    /// `min(max(x0, C0), C1)` (Clamp, StateWrite).
    Clamp,
    Log2Floor,
    IntExp,
    IntRsqrt,
    IntLn,
    /// `C0 + C1 · i_axis`.
    Iota,
    /// `data[off0 + x1 · stride]`, the index checked against its axis when asked.
    Gather,
}

impl EwOp {
    pub fn arity(self) -> usize {
        match self {
            EwOp::Iota => 0,
            EwOp::Copy | EwOp::Clamp | EwOp::Log2Floor | EwOp::IntExp | EwOp::IntRsqrt | EwOp::IntLn => 1,
            EwOp::Select => 3,
            _ => 2,
        }
    }
    /// 64-bit constants the op reads after the operand geometry.
    pub fn consts(self) -> usize {
        match self {
            EwOp::Clamp | EwOp::Iota => 2,
            _ => 0,
        }
    }
}

/// Everything that makes one elementwise source distinct.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EwKey {
    pub op: EwOp,
    pub ins: Vec<Form>,
    pub out: Form,
    /// Check the result against the output dtype's bounds (two constants after the op's own:
    /// `i64` each, or `i128` — four words — in wide mode).
    pub check_out: bool,
    /// `Gather`: check every index against its axis.
    pub check_operand: bool,
    /// Compute in 128 bits ([`INTLIB128`]): the plan's working type is `i128`, or a value moved is
    /// held as `i128`.
    pub wide: bool,
    /// A batch of positions with a padded history axis ([`RAGGED_MASK`]): an element past its
    /// position's `H_p` is padding — stored as 0, never loaded from, never failing.
    pub ragged: bool,
}

/// **The ragged history axis of a batch of positions** (`crate::batch`). A value whose shape has `H`
/// is held for positions `p0 … p0+T−1` as one tensor with a leading position axis and `H` padded to
/// the batch's largest. Position `p`'s valid extent is `H_p = min(p + 1, W)`. Parameter words:
/// `P[60]` the output's `H` axis and `P[61]` its position axis (both right-aligned to rank 4),
/// `P[62]` the window `W`, `P[63]` the first position `p0`. The mask runs before any load, so a
/// padding element reads nothing and reports nothing.
pub const RAGGED_MASK: &str = "    var rx = array<u32, 4>(i0, i1, i2, i3);\n    let hp = min(rx[P[61]] + P[63] + 1u, P[62]);\n";

/// The parameter word where an elementwise kernel's 64-bit constants start.
pub fn ew_const_base(n_ops: usize) -> usize {
    11 + 5 * n_ops
}

pub fn ew_source(k: &EwKey) -> String {
    let n = k.ins.len();
    let c = ew_const_base(n);
    let cc = c + 2 * k.op.consts();
    let mut s = header(k.out, &k.ins);
    s.push_str(
        "\n@compute @workgroup_size(256)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {\n\
         \x20   let e = gid.x + gid.y * nwg.x * 256u;\n\
         \x20   if (e >= P[0]) { return; }\n",
    );
    s.push_str(&index_prelude(n));
    let w = k.wide;
    if k.ragged {
        s.push_str(RAGGED_MASK);
        s.push_str(if w {
            "    if (rx[P[60]] >= hp) { st128(off_o, W(0lu, 0lu)); return; }\n"
        } else {
            "    if (rx[P[60]] >= hp) { st(off_o, 0li); return; }\n"
        });
    }
    let load = |j: usize| if w { format!("    let x{j} = ld128_{j}(off{j});\n") } else { format!("    let x{j} = ld{j}(off{j});\n") };
    match k.op {
        EwOp::Gather => {
            // P[cc]: the gathered axis's extent; P[cc + 1]: its stride in the data. Indices are never
            // i128 (type rule): read as i64.
            s.push_str("    let x1 = ld1(off1);\n");
            if k.check_operand {
                s.push_str(&format!("    if (x1 < 0li || x1 >= i64(P[{cc}])) {{ fail(e, 3u); return; }}\n"));
            }
            let ld = if w { "ld128_0" } else { "ld0" };
            s.push_str(&format!("    var v = {ld}(off0 + u32(x1) * P[{}]);\n", cc + 1));
        }
        EwOp::Iota => {
            // P[cc]: the axis, right-aligned to rank 4.
            s.push_str("    var ix = array<u32, 4>(i0, i1, i2, i3);\n");
            if w {
                s.push_str(&format!("    var v = w_add(wi(p64({c}u)), w_mul(wi(p64({}u)), wu(u64(ix[P[{cc}]]))));\n", c + 2));
            } else {
                s.push_str(&format!("    var v = p64({c}u) + p64({}u) * i64(ix[P[{cc}]]);\n", c + 2));
            }
        }
        op => {
            for j in 0..n {
                s.push_str(&load(j));
            }
            let body = match (op, w) {
                (EwOp::Copy, _) => "    var v = x0;\n".to_string(),
                (EwOp::Add, false) => "    var v = x0 + x1;\n".to_string(),
                (EwOp::Sub, false) => "    var v = x0 - x1;\n".to_string(),
                (EwOp::Mul, false) => "    var v = x0 * x1;\n".to_string(),
                (EwOp::Add, true) => "    var v = w_add(x0, x1);\n".to_string(),
                (EwOp::Sub, true) => "    var v = w_sub(x0, x1);\n".to_string(),
                (EwOp::Mul, true) => "    var v = w_mul(x0, x1);\n".to_string(),
                (EwOp::Div(rule), false) => {
                    format!("    if (x1 < 1li) {{ fail(e, 2u); return; }}\n    var v = div_rule64(x0, x1, {rule}u);\n")
                }
                (EwOp::Div(rule), true) => {
                    format!("    if (w_lt(x1, wu(1lu))) {{ fail(e, 2u); return; }}\n    var v = w_div_rule(x0, x1, {rule}u);\n")
                }
                (EwOp::Compare(cmp), false) => {
                    let rel = ["==", "!=", "<", "<=", ">", ">="][cmp as usize];
                    format!("    var v = select(0li, 1li, x0 {rel} x1);\n")
                }
                (EwOp::Compare(cmp), true) => {
                    let cond = ["w_eq(x0, x1)", "!w_eq(x0, x1)", "w_lt(x0, x1)", "!w_lt(x1, x0)", "w_lt(x1, x0)", "!w_lt(x0, x1)"]
                        [cmp as usize];
                    format!("    var v = wu(select(0lu, 1lu, {cond}));\n")
                }
                (EwOp::Select, false) => "    var v = select(x2, x1, x0 != 0li);\n".to_string(),
                (EwOp::Select, true) => "    var v = x2;\n    if (!w_iszero(x0)) { v = x1; }\n".to_string(),
                (EwOp::Clamp, false) => format!("    var v = min(max(x0, p64({c}u)), p64({}u));\n", c + 2),
                (EwOp::Clamp, true) => format!("    var v = w_min(w_max(x0, wi(p64({c}u))), wi(p64({}u)));\n", c + 2),
                (EwOp::Log2Floor, false) => "    var v = log2_floor64(x0);\n".to_string(),
                (EwOp::Log2Floor, true) => "    var v = wi(w_log2(x0));\n".to_string(),
                (EwOp::IntExp, false) => "    var v = int_exp64(x0);\n".to_string(),
                (EwOp::IntRsqrt, false) => "    var v = int_rsqrt64(x0);\n".to_string(),
                (EwOp::IntLn, false) => "    var v = int_ln64(x0);\n".to_string(),
                (EwOp::IntExp | EwOp::IntRsqrt | EwOp::IntLn, true) => unreachable!("a transcendental's operand is never i128"),
                (EwOp::Iota | EwOp::Gather, _) => unreachable!(),
            };
            s.push_str(&body);
        }
    }
    if k.check_out {
        let b = c + 2 * k.op.consts() + if k.op == EwOp::Gather || k.op == EwOp::Iota { 2 } else { 0 };
        if w {
            s.push_str(&format!("    if (w_lt(v, p128({b}u)) || w_lt(p128({}u), v)) {{ fail(e, 1u); return; }}\n", b + 4));
        } else {
            s.push_str(&format!("    if (v < p64({b}u) || v > p64({}u)) {{ fail(e, 1u); return; }}\n", b + 2));
        }
    }
    s.push_str(if w { "    st128(off_o, v);\n}\n" } else { "    st(off_o, v);\n}\n" });
    s
}

/// How an exact sum accumulates (the plan's `Acc`, for the cases the device serves).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SumMode {
    /// Every partial sum fits `i64` and the dtype: wrapping `i64` in any order is the value.
    Fast,
    /// Every partial sum fits `i64`, not necessarily the dtype: positives and negatives apart,
    /// checked against the dtype's bounds.
    PosNeg,
}

/// `ReduceSum` / `ReduceMax` along one axis.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReduceKey {
    pub max: bool,
    pub mode: SumMode,
    pub x: Form,
    pub out: Form,
    /// One workgroup per output element (long axes) instead of one invocation.
    pub cooperative: bool,
    /// 128-bit values and sums (`Acc::Fast128`/`Pn128`, a maximum in `i128`); never cooperative.
    pub wide: bool,
    /// A batch of positions ([`RAGGED_MASK`]): 1 — the reduced axis is `H`, reduced over `H_p`;
    /// 2 — the output has `H` on another axis, padding masked; 0 — neither.
    pub ragged: u8,
}

/// The ragged prologue of a reduction: the extent `n` it reduces over, and the padding mask.
fn reduce_ragged(ragged: u8, wide: bool) -> String {
    match ragged {
        1 => format!("{RAGGED_MASK}    let n = min(P[16], hp); let st_ax = P[17];\n"),
        2 => format!(
            "{RAGGED_MASK}    if (rx[P[60]] >= hp) {{ {} return; }}\n    let n = P[16]; let st_ax = P[17];\n",
            if wide { "st128(off_o, W(0lu, 0lu));" } else { "st(off_o, 0li);" }
        ),
        _ => "    let n = P[16]; let st_ax = P[17];\n".to_string(),
    }
}

/// Parameter block: `P[0]` outputs, `P[1]` status slot, `P[2..11]` output geometry, `P[11..16]`
/// the operand's `(base, strides)` at the output's multi-index, `P[16]` the axis extent, `P[17]`
/// its stride, `P[18..22]` the dtype bounds (Pn).
pub fn reduce_source(k: &ReduceKey) -> String {
    if k.wide {
        return reduce_wide_source(k);
    }
    let mut s = header(k.out, &[k.x]);
    let combine_sum = |acc: &str, x: &str| match k.mode {
        SumMode::Fast => format!("{acc} = {acc} + {x};"),
        SumMode::PosNeg => format!("if ({x} > 0li) {{ {acc}p = {acc}p + {x}; }} else {{ {acc}n = {acc}n + {x}; }}"),
    };
    let fin_pn = "    if (accp > p64(18u) || accn < p64(20u)) { fail(e, 1u); return; }\n    let v = accp + accn;\n";
    if !k.cooperative {
        s.push_str(
            "\n@compute @workgroup_size(256)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {\n\
             \x20   let e = gid.x + gid.y * nwg.x * 256u;\n\
             \x20   if (e >= P[0]) { return; }\n",
        );
        s.push_str(&index_prelude(1));
        s.push_str(&reduce_ragged(k.ragged, false));
        if k.max {
            s.push_str("    var acc = ld0(off0);\n    for (var t = 1u; t < n; t = t + 1u) { acc = max(acc, ld0(off0 + t * st_ax)); }\n    let v = acc;\n");
        } else {
            match k.mode {
                SumMode::Fast => s.push_str("    var acc = 0li;\n"),
                SumMode::PosNeg => s.push_str("    var accp = 0li; var accn = 0li;\n"),
            }
            s.push_str(&format!(
                "    for (var t = 0u; t < n; t = t + 1u) {{ let x = ld0(off0 + t * st_ax); {} }}\n",
                combine_sum("acc", "x")
            ));
            match k.mode {
                SumMode::Fast => s.push_str("    let v = acc;\n"),
                SumMode::PosNeg => s.push_str(fin_pn),
            }
        }
        s.push_str("    st(off_o, v);\n}\n");
        return s;
    }
    // One workgroup of 256 per output element: each invocation folds a strided share of the
    // axis, then a tree over the workgroup (an exact sum / a maximum in any order).
    s.push_str("var<workgroup> W0: array<i64, 256>;\nvar<workgroup> W1: array<i64, 256>;\n");
    s.push_str(
        "\n@compute @workgroup_size(256)\nfn main(@builtin(local_invocation_index) lid: u32, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {\n\
         \x20   let e = wid.x + wid.y * nwg.x;\n\
         \x20   if (e >= P[0]) { return; }\n",
    );
    s.push_str(&index_prelude(1));
    s.push_str(&reduce_ragged(k.ragged, false));
    if k.max {
        s.push_str(
            "    var acc = ld0(off0);\n    for (var t = lid; t < n; t = t + 256u) { acc = max(acc, ld0(off0 + t * st_ax)); }\n\
             \x20   W0[lid] = acc;\n    workgroupBarrier();\n\
             \x20   for (var w = 128u; w > 0u; w = w >> 1u) { if (lid < w) { W0[lid] = max(W0[lid], W0[lid + w]); } workgroupBarrier(); }\n\
             \x20   if (lid == 0u) { st(off_o, W0[0]); }\n}\n",
        );
        return s;
    }
    match k.mode {
        SumMode::Fast => {
            s.push_str(
                "    var acc = 0li;\n    for (var t = lid; t < n; t = t + 256u) { acc = acc + ld0(off0 + t * st_ax); }\n\
                 \x20   W0[lid] = acc;\n    workgroupBarrier();\n\
                 \x20   for (var w = 128u; w > 0u; w = w >> 1u) { if (lid < w) { W0[lid] = W0[lid] + W0[lid + w]; } workgroupBarrier(); }\n\
                 \x20   if (lid == 0u) { st(off_o, W0[0]); }\n}\n",
            );
        }
        SumMode::PosNeg => {
            s.push_str(&format!(
                "    var accp = 0li; var accn = 0li;\n    for (var t = lid; t < n; t = t + 256u) {{ let x = ld0(off0 + t * st_ax); {} }}\n\
                 \x20   W0[lid] = accp; W1[lid] = accn;\n    workgroupBarrier();\n\
                 \x20   for (var w = 128u; w > 0u; w = w >> 1u) {{ if (lid < w) {{ W0[lid] = W0[lid] + W0[lid + w]; W1[lid] = W1[lid] + W1[lid + w]; }} workgroupBarrier(); }}\n\
                 \x20   if (lid == 0u) {{ if (W0[0] > p64(18u) || W1[0] < p64(20u)) {{ fail(e, 1u); }} else {{ st(off_o, W0[0] + W1[0]); }} }}\n}}\n",
                combine_sum("acc", "x")
            ));
        }
    }
    s
}

/// A reduction in 128 bits, one invocation per output element. `PosNeg` checks every partial sum
/// as it grows — the positive part against the dtype's maximum, the negative part against its
/// minimum (`P[18..22]` / `P[22..26]`, four words each) — which is the CPU executor's checked
/// accumulation and also catches a sum past `i128` itself.
fn reduce_wide_source(k: &ReduceKey) -> String {
    let mut s = header(k.out, &[k.x]);
    s.push_str(
        "\n@compute @workgroup_size(256)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {\n\
         \x20   let e = gid.x + gid.y * nwg.x * 256u;\n\
         \x20   if (e >= P[0]) { return; }\n",
    );
    s.push_str(&index_prelude(1));
    s.push_str(&reduce_ragged(k.ragged, true));
    if k.max {
        s.push_str("    var acc = ld128_0(off0);\n    for (var t = 1u; t < n; t = t + 1u) { acc = w_max(acc, ld128_0(off0 + t * st_ax)); }\n    st128(off_o, acc);\n}\n");
        return s;
    }
    match k.mode {
        SumMode::Fast => s.push_str(
            "    var acc = W(0lu, 0lu);\n    for (var t = 0u; t < n; t = t + 1u) { acc = w_add(acc, ld128_0(off0 + t * st_ax)); }\n    st128(off_o, acc);\n}\n",
        ),
        SumMode::PosNeg => s.push_str(
            "    let hi = p128(18u); let lo = p128(22u);\n\
             \x20   var ps = W(0lu, 0lu); var ng = W(0lu, 0lu);\n\
             \x20   for (var t = 0u; t < n; t = t + 1u) {\n\
             \x20       let x = ld128_0(off0 + t * st_ax);\n\
             \x20       if (!w_isneg(x) && !w_iszero(x)) {\n\
             \x20           if (w_lt(w_sub(hi, ps), x)) { fail(e, 1u); return; }\n\
             \x20           ps = w_add(ps, x);\n\
             \x20       } else {\n\
             \x20           if (w_lt(x, w_sub(lo, ng))) { fail(e, 1u); return; }\n\
             \x20           ng = w_add(ng, x);\n\
             \x20       }\n\
             \x20   }\n\
             \x20   st128(off_o, w_add(ps, ng));\n}\n",
        ),
    }
    s
}

/// `MatMul` in 128 bits (`Acc::Fast128`/`Pn128`), one invocation per output element: each term is
/// the exact 128-bit product of two `i64` operands; `PosNeg` checks each partial as
/// [`reduce_wide_source`] does (bounds at `P[19..23]` hi and `P[24..28]` lo, four words each).
pub fn matmul_wide_source(a: Form, b: Form, out: Form, mode: SumMode, rag: u8) -> String {
    let mut s = header(out, &[a, b]);
    s.push_str(
        "\n@compute @workgroup_size(256)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {\n\
         \x20   let e = gid.x + gid.y * nwg.x * 256u;\n\
         \x20   if (e >= P[0]) { return; }\n\
         \x20   let m = P[2]; let n = P[3];\n\
         \x20   let c = e % n; let rest = e / n; let r = rest % m; let bi = rest / m;\n\
         \x20   let b1 = bi % P[10]; let b0 = bi / P[10];\n\
         \x20   let ao = P[17] + b0 * P[11] + b1 * P[12] + r * P[5];\n\
         \x20   let bo = P[18] + b0 * P[13] + b1 * P[14] + c * P[8];\n\
         \x20   let a_k = P[6]; let b_k = P[7];\n",
    );
    s.push_str(&matmul_ragged(rag, true, "P[4]"));
    s.push_str("    let kk = tk;\n");
    match mode {
        SumMode::Fast => s.push_str(
            "    var acc = W(0lu, 0lu);\n    for (var t = 0u; t < kk; t = t + 1u) { acc = w_add(acc, w_mul(wi(ld0(ao + t * a_k)), wi(ld1(bo + t * b_k)))); }\n    st128(P[23] + e, acc);\n}\n",
        ),
        SumMode::PosNeg => s.push_str(
            "    let hi = p128(24u); let lo = p128(28u);\n\
             \x20   var ps = W(0lu, 0lu); var ng = W(0lu, 0lu);\n\
             \x20   for (var t = 0u; t < kk; t = t + 1u) {\n\
             \x20       let x = w_mul(wi(ld0(ao + t * a_k)), wi(ld1(bo + t * b_k)));\n\
             \x20       if (!w_isneg(x) && !w_iszero(x)) {\n\
             \x20           if (w_lt(w_sub(hi, ps), x)) { fail(e, 1u); return; }\n\
             \x20           ps = w_add(ps, x);\n\
             \x20       } else {\n\
             \x20           if (w_lt(x, w_sub(lo, ng))) { fail(e, 1u); return; }\n\
             \x20           ng = w_add(ng, x);\n\
             \x20       }\n\
             \x20   }\n\
             \x20   st128(P[23] + e, w_add(ps, ng));\n}\n",
        ),
    }
    s
}

/// `MatMul`, one invocation per output element (or per output element and K-slice), any strides
/// and forms: the general kernel.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MatMulKey {
    pub a: Form,
    pub b: Form,
    pub out: Form,
    pub mode: SumMode,
    /// Terms are formed in `i32` and summed in `i32` chunks of at most `P[16]` terms (the plan
    /// proved every operand inside `i32` and `chunk · max|term| < 2^31`); otherwise `i64` terms.
    pub chunked: bool,
    /// Split the contraction into `P[25]` slices of `P[24]` terms: each invocation writes one
    /// slice's partial (an `i64` sum, or its positive and negative parts) to the temporary `O`, and
    /// [`splitk_finish_source`] adds the slices. A long contraction with few outputs (the values
    /// of an attention over a long history) then has an invocation per slice, not per output.
    pub split: bool,
    /// A batch of positions with a padded history axis ([`RAGGED_MASK`]'s words; `P[60]` names the
    /// output batch slot that holds the position): bit 1 — `M` is `H`, bit 2 — `N` is `H` (padding
    /// outputs stored 0, never failing); bit 4 — the contraction is `H`, summed over `H_p` terms.
    pub rag: u8,
}

/// The ragged prologue of a MatMul: `hp` for the output's position, the padding mask, and the
/// contraction's length `tk` (`kk` past the mask).
fn matmul_ragged(rag: u8, wide: bool, kk: &str) -> String {
    if rag == 0 {
        return format!("    let tk = {kk};\n");
    }
    let zero = if wide { "st128(P[23] + e, W(0lu, 0lu));" } else { "st(P[23] + e, 0li);" };
    let mut s = String::from("    let pos = select(b1, b0, P[60] == 0u);\n    let hp = min(pos + P[63] + 1u, P[62]);\n");
    if rag & 1 != 0 {
        s.push_str(&format!("    if (r >= hp) {{ {zero} return; }}\n"));
    }
    if rag & 2 != 0 {
        s.push_str(&format!("    if (c >= hp) {{ {zero} return; }}\n"));
    }
    if rag & 4 != 0 {
        s.push_str(&format!("    let tk = min({kk}, hp);\n"));
    } else {
        s.push_str(&format!("    let tk = {kk};\n"));
    }
    s
}

/// Parameter block shared by the MatMul kernels: `P[0]` outputs, `P[1]` slot, `P[2]` M, `P[3]` N,
/// `P[4]` K, `P[5]` a_m, `P[6]` a_k, `P[7]` b_k, `P[8]` b_n, `P[9..11]` the output batch dims (two,
/// right-aligned), `P[11..13]` / `P[13..15]` `a`'s / `b`'s batch strides, `P[16]` the i32 chunk,
/// `P[17]` / `P[18]` the operands' base offsets, `P[19..23]` the dtype bounds (hi, lo), `P[23]` the
/// output base offset, `P[24]` / `P[25]` the slice length and count of a split contraction.
pub fn matmul_source(k: &MatMulKey) -> String {
    let mut s = header(k.out, &[k.a, k.b]);
    let total = if k.split { "P[0] * P[25]" } else { "P[0]" };
    s.push_str(&format!(
        "\n@compute @workgroup_size(256)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {{\n\
         \x20   let ee = gid.x + gid.y * nwg.x * 256u;\n\
         \x20   if (ee >= {total}) {{ return; }}\n"
    ));
    if k.split {
        s.push_str("    let e = ee % P[0]; let slice = ee / P[0];\n    let t0 = slice * P[24]; let t1s = min(t0 + P[24], P[4]);\n");
    } else {
        s.push_str("    let e = ee;\n    let t0 = 0u; let t1s = P[4];\n");
    }
    s.push_str(
        "    let m = P[2]; let n = P[3];\n\
         \x20   let c = e % n; let rest = e / n; let r = rest % m; let bi = rest / m;\n\
         \x20   let b1 = bi % P[10]; let b0 = bi / P[10];\n\
         \x20   let ao = P[17] + b0 * P[11] + b1 * P[12] + r * P[5];\n\
         \x20   let bo = P[18] + b0 * P[13] + b1 * P[14] + c * P[8];\n\
         \x20   let a_k = P[6]; let b_k = P[7];\n",
    );
    s.push_str(&matmul_ragged(k.rag, false, "t1s"));
    s.push_str("    let t1 = tk;\n");
    match k.mode {
        SumMode::Fast => s.push_str("    var acc = 0li;\n"),
        SumMode::PosNeg => s.push_str("    var accp = 0li; var accn = 0li;\n"),
    }
    if k.chunked {
        // Every term and every chunk total fit i32: chunk totals are exact, then widened.
        let (decl, add, flush) = match k.mode {
            SumMode::Fast => ("var sp = 0i;", "sp = sp + x * y;", "acc = acc + i64(sp);"),
            SumMode::PosNeg => (
                "var sp = 0i; var sn = 0i;",
                "let q = x * y; if (q > 0i) { sp = sp + q; } else { sn = sn + q; }",
                "accp = accp + i64(sp); accn = accn + i64(sn);",
            ),
        };
        s.push_str(&format!(
            "    let ch = P[16];\n    var t = t0;\n    loop {{\n        if (t >= t1) {{ break; }}\n        let end = min(t + ch, t1);\n        {decl}\n\
             \x20       for (; t < end; t = t + 1u) {{ let x = ld32_0(ao + t * a_k); let y = ld32_1(bo + t * b_k); {add} }}\n        {flush}\n    }}\n"
        ));
    } else {
        match k.mode {
            SumMode::Fast => s.push_str("    for (var t = t0; t < t1; t = t + 1u) { acc = acc + ld0(ao + t * a_k) * ld1(bo + t * b_k); }\n"),
            SumMode::PosNeg => s.push_str(
                "    for (var t = t0; t < t1; t = t + 1u) { let q = ld0(ao + t * a_k) * ld1(bo + t * b_k); if (q > 0li) { accp = accp + q; } else { accn = accn + q; } }\n",
            ),
        }
    }
    match (k.split, k.mode) {
        (true, SumMode::Fast) => s.push_str("    O[ee] = acc;\n}\n"),
        (true, SumMode::PosNeg) => s.push_str("    O[2u * ee] = accp; O[2u * ee + 1u] = accn;\n}\n"),
        (false, SumMode::Fast) => s.push_str("    st(P[23] + e, acc);\n}\n"),
        (false, SumMode::PosNeg) => {
            s.push_str("    if (accp > p64(19u) || accn < p64(21u)) { fail(e, 1u); return; }\n    st(P[23] + e, accp + accn);\n}\n")
        }
    }
    s
}

/// The second pass of a split contraction: per output, the slices' partials added (any order:
/// each is a sum of a subset of the terms, inside `i64` by the plan's proof), the `Pn` check on the
/// whole positive and negative parts, the value stored.
pub fn splitk_finish_source(mode: SumMode, out: Form) -> String {
    let mut s = header(out, &[Form::I64]);
    s.push_str(
        "\n@compute @workgroup_size(256)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {\n\
         \x20   let e = gid.x + gid.y * nwg.x * 256u;\n\
         \x20   if (e >= P[0]) { return; }\n",
    );
    match mode {
        SumMode::Fast => s.push_str(
            "    var acc = 0li;\n    for (var q = 0u; q < P[25]; q = q + 1u) { acc = acc + B0[q * P[0] + e]; }\n    st(P[23] + e, acc);\n}\n",
        ),
        SumMode::PosNeg => s.push_str(
            "    var accp = 0li; var accn = 0li;\n\
             \x20   for (var q = 0u; q < P[25]; q = q + 1u) { accp = accp + B0[2u * (q * P[0] + e)]; accn = accn + B0[2u * (q * P[0] + e) + 1u]; }\n\
             \x20   if (accp > p64(19u) || accn < p64(21u)) { fail(e, 1u); return; }\n\
             \x20   st(P[23] + e, accp + accn);\n}\n",
        ),
    }
    s
}

/// `MatMul` as a matrix–vector product with `i8` packed weight rows: `out[β, r] = Σ_t W[β, r, t]·x[β, t]`.
/// Two shapes of it:
///
/// * **`vec4`**: 32 invocations per row, eight rows per workgroup, sixteen weights (one
///   `vec4<u32>`) and sixteen activation lanes (four) per load — `K % 16 = 0`, the weight rows and
///   base 16-aligned, the activations `S32` lanes, contiguous and 4-aligned. Terms in `i32`,
///   chunked (`P[16]` loads of sixteen terms per `i32` partial).
/// * **scalar**: 64 invocations per row, four rows per workgroup, one weight word per load, any
///   activation form and stride; `i32` chunks of `P[16]` words, or `i64` terms.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GemvKey {
    pub b: Form,
    pub out: Form,
    pub chunked: bool,
    pub vec4: bool,
}

pub fn gemv_source(k: &GemvKey) -> String {
    if k.vec4 {
        return gemv4_source(k.out);
    }
    let mut s = header(k.out, &[Form::P8, k.b]);
    s.push_str("var<workgroup> W0: array<i64, 256>;\n");
    s.push_str(
        "\n@compute @workgroup_size(256)\nfn main(@builtin(local_invocation_index) lid: u32, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {\n\
         \x20   let row = (wid.x + wid.y * nwg.x) * 4u + (lid >> 6u);\n\
         \x20   let lane = lid & 63u;\n\
         \x20   let bi = wid.z; let b1 = bi % P[10]; let b0 = bi / P[10];\n\
         \x20   let m = P[2];\n\
         \x20   var acc = 0li;\n\
         \x20   if (row < m) {\n\
         \x20       let abase = P[17] + b0 * P[11] + b1 * P[12] + row * P[5];\n\
         \x20       let bo = P[18] + b0 * P[13] + b1 * P[14];\n\
         \x20       let b_k = P[7];\n\
         \x20       let kw = P[4] >> 2u;\n\
         \x20       let wbase = abase >> 2u;\n",
    );
    if k.chunked {
        s.push_str(
            "        let ch = P[16];\n        var w = lane;\n        loop {\n            if (w >= kw) { break; }\n            var sp = 0i;\n            var c = 0u;\n\
             \x20           for (; w < kw && c < ch; w = w + 64u) {\n\
             \x20               let pw = bitcast<i32>(B0[wbase + w]);\n\
             \x20               let t = w * 4u;\n\
             \x20               sp = sp + extractBits(pw, 0u, 8u) * ld32_1(bo + t * b_k) + extractBits(pw, 8u, 8u) * ld32_1(bo + (t + 1u) * b_k)\n\
             \x20                       + extractBits(pw, 16u, 8u) * ld32_1(bo + (t + 2u) * b_k) + extractBits(pw, 24u, 8u) * ld32_1(bo + (t + 3u) * b_k);\n\
             \x20               c = c + 1u;\n\
             \x20           }\n\
             \x20           acc = acc + i64(sp);\n        }\n",
        );
    } else {
        s.push_str(
            "        for (var w = lane; w < kw; w = w + 64u) {\n\
             \x20           let pw = bitcast<i32>(B0[wbase + w]);\n\
             \x20           let t = w * 4u;\n\
             \x20           acc = acc + i64(extractBits(pw, 0u, 8u)) * ld1(bo + t * b_k) + i64(extractBits(pw, 8u, 8u)) * ld1(bo + (t + 1u) * b_k)\n\
             \x20                     + i64(extractBits(pw, 16u, 8u)) * ld1(bo + (t + 2u) * b_k) + i64(extractBits(pw, 24u, 8u)) * ld1(bo + (t + 3u) * b_k);\n\
             \x20       }\n",
        );
    }
    s.push_str(
        "    }\n\
         \x20   W0[lid] = acc;\n\
         \x20   workgroupBarrier();\n\
         \x20   for (var h = 32u; h > 0u; h = h >> 1u) { if (lane < h) { W0[lid] = W0[lid] + W0[lid + h]; } workgroupBarrier(); }\n\
         \x20   if (lane == 0u && row < m) { st(P[23] + bi * m + row, W0[lid]); }\n}\n",
    );
    s
}

/// The `vec4` GEMV: 32 invocations per group of FOUR rows, eight groups (32 rows) per workgroup.
/// Each step loads sixteen activation lanes once and sixteen weights of each of the four rows, so
/// an activation is read once per four rows rather than once per row; four `i32` partials (one per
/// row, `P[16]` steps each) widen into four `i64` totals.
fn gemv4_source(out: Form) -> String {
    let mut s = header_base(out);
    s.push_str("@group(0) @binding(3) var<storage, read> W4: array<vec4<u32>>;\n");
    s.push_str("@group(0) @binding(4) var<storage, read> X4: array<vec4<u32>>;\n");
    s.push_str("var<workgroup> W0: array<i64, 1024>;\n");
    s.push_str(
        "fn unpack(w: u32) -> vec4<i32> {\n    let x = bitcast<i32>(w);\n    return vec4<i32>(extractBits(x, 0u, 8u), extractBits(x, 8u, 8u), extractBits(x, 16u, 8u), extractBits(x, 24u, 8u));\n}\n\
         fn dot16(w: vec4<u32>, x0: vec4<i32>, x1: vec4<i32>, x2: vec4<i32>, x3: vec4<i32>) -> i32 {\n    return dot(unpack(w.x), x0) + dot(unpack(w.y), x1) + dot(unpack(w.z), x2) + dot(unpack(w.w), x3);\n}\n",
    );
    s.push_str(
        "\n@compute @workgroup_size(256)\nfn main(@builtin(local_invocation_index) lid: u32, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {\n\
         \x20   let group = lid >> 5u; let lane = lid & 31u;\n\
         \x20   let row0 = ((wid.x + wid.y * nwg.x) * 8u + group) * 4u;\n\
         \x20   let bi = wid.z; let b1 = bi % P[10]; let b0 = bi / P[10];\n\
         \x20   let m = P[2];\n\
         \x20   let ab = P[17] + b0 * P[11] + b1 * P[12];\n\
         \x20   let xb = (P[18] + b0 * P[13] + b1 * P[14]) >> 2u;\n\
         \x20   // Rows past m read the last row (uniform control flow) and are never stored.\n\
         \x20   let w0 = (ab + min(row0, m - 1u) * P[5]) >> 4u;\n\
         \x20   let w1 = (ab + min(row0 + 1u, m - 1u) * P[5]) >> 4u;\n\
         \x20   let w2 = (ab + min(row0 + 2u, m - 1u) * P[5]) >> 4u;\n\
         \x20   let w3 = (ab + min(row0 + 3u, m - 1u) * P[5]) >> 4u;\n\
         \x20   let kv = P[4] >> 4u;\n\
         \x20   let ch = P[16];\n\
         \x20   var q0 = 0li; var q1 = 0li; var q2 = 0li; var q3 = 0li;\n\
         \x20   var v = lane;\n\
         \x20   loop {\n\
         \x20       if (v >= kv) { break; }\n\
         \x20       var s0 = 0i; var s1 = 0i; var s2 = 0i; var s3 = 0i;\n\
         \x20       var c = 0u;\n\
         \x20       for (; v < kv && c < ch; v = v + 32u) {\n\
         \x20           let x0 = bitcast<vec4<i32>>(X4[xb + 4u * v]); let x1 = bitcast<vec4<i32>>(X4[xb + 4u * v + 1u]);\n\
         \x20           let x2 = bitcast<vec4<i32>>(X4[xb + 4u * v + 2u]); let x3 = bitcast<vec4<i32>>(X4[xb + 4u * v + 3u]);\n\
         \x20           s0 = s0 + dot16(W4[w0 + v], x0, x1, x2, x3);\n\
         \x20           s1 = s1 + dot16(W4[w1 + v], x0, x1, x2, x3);\n\
         \x20           s2 = s2 + dot16(W4[w2 + v], x0, x1, x2, x3);\n\
         \x20           s3 = s3 + dot16(W4[w3 + v], x0, x1, x2, x3);\n\
         \x20           c = c + 1u;\n\
         \x20       }\n\
         \x20       q0 = q0 + i64(s0); q1 = q1 + i64(s1); q2 = q2 + i64(s2); q3 = q3 + i64(s3);\n\
         \x20   }\n\
         \x20   let base = group * 128u + lane;\n\
         \x20   W0[base] = q0; W0[base + 32u] = q1; W0[base + 64u] = q2; W0[base + 96u] = q3;\n\
         \x20   workgroupBarrier();\n\
         \x20   for (var h = 16u; h > 0u; h = h >> 1u) {\n\
         \x20       if (lane < h) {\n\
         \x20           W0[base] = W0[base] + W0[base + h]; W0[base + 32u] = W0[base + 32u] + W0[base + 32u + h];\n\
         \x20           W0[base + 64u] = W0[base + 64u] + W0[base + 64u + h]; W0[base + 96u] = W0[base + 96u] + W0[base + 96u + h];\n\
         \x20       }\n\
         \x20       workgroupBarrier();\n\
         \x20   }\n\
         \x20   if (lane < 4u && row0 + lane < m) { st(P[23] + bi * m + row0 + lane, W0[group * 128u + lane * 32u]); }\n}\n",
    );
    s
}

/// `MatMul` tiled through workgroup memory for many output columns (a batch of positions):
/// 64×64 outputs per workgroup of 16×16 invocations, each a 4×4 block held in sixteen `i32`
/// partials and sixteen `i64` totals (named registers, not an array a compiler may spill), a K tile
/// of 32 staged in workgroup memory as `i32`. Terms in `i32`, widened every `P[16]` K-tiles (the
/// plan's chunk over 32). `fast_a`: `a` is packed `i8` rows, contiguous along K and word-aligned —
/// staged a word (four weights) at a time.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GemmKey {
    pub a: Form,
    pub b: Form,
    pub out: Form,
    pub fast_a: bool,
}

pub fn gemm_source(k: &GemmKey) -> String {
    let mut s = header(k.out, &[k.a, k.b]);
    s.push_str("var<workgroup> TA: array<i32, 2048>;\nvar<workgroup> TB: array<i32, 2048>;\n");
    let mut decl = String::new();
    let mut flush = String::new();
    let mut mac = String::new();
    let mut store = String::new();
    for i in 0..4 {
        for j in 0..4 {
            decl.push_str(&format!("    var p{i}{j} = 0i; var q{i}{j} = 0li;\n"));
            flush.push_str(&format!("            q{i}{j} = q{i}{j} + i64(p{i}{j}); p{i}{j} = 0i;\n"));
            mac.push_str(&format!("            p{i}{j} = p{i}{j} + a{i} * c{j};\n"));
            store.push_str(&format!(
                "    if (row0 + ty * 4u + {i}u < m && col0 + tx * 4u + {j}u < n) {{ st(P[23] + (bi * m + row0 + ty * 4u + {i}u) * n + col0 + tx * 4u + {j}u, q{i}{j} + i64(p{i}{j})); }}\n"
            ));
        }
    }
    let load_a = if k.fast_a {
        // 2 words = 8 weights per invocation: row (lid / 4), word ((lid % 4) · 2 + q) of the
        // 8-word K tile.
        "        for (var q = 0u; q < 2u; q = q + 1u) {\n\
         \x20           let ar = lid >> 2u; let kw = ((lid & 3u) << 1u) + q;\n\
         \x20           let kk0 = k0 + kw * 4u;\n\
         \x20           var w = 0i;\n\
         \x20           if (row0 + ar < m && kk0 < kk) { w = bitcast<i32>(B0[(ab + (row0 + ar) * P[5] + kk0) >> 2u]); }\n\
         \x20           let base = ar * 32u + kw * 4u;\n\
         \x20           TA[base] = extractBits(w, 0u, 8u); TA[base + 1u] = extractBits(w, 8u, 8u);\n\
         \x20           TA[base + 2u] = extractBits(w, 16u, 8u); TA[base + 3u] = extractBits(w, 24u, 8u);\n\
         \x20       }\n"
            .to_string()
    } else {
        "        for (var q = 0u; q < 8u; q = q + 1u) {\n\
         \x20           let li = lid * 8u + q; let ar = li >> 5u; let ak = li & 31u;\n\
         \x20           var av = 0i;\n\
         \x20           if (row0 + ar < m && k0 + ak < kk) { av = ld32_0(ab + (row0 + ar) * P[5] + (k0 + ak) * P[6]); }\n\
         \x20           TA[li] = av;\n\
         \x20       }\n"
            .to_string()
    };
    s.push_str(&format!(
        "\n@compute @workgroup_size(16, 16)\nfn main(@builtin(local_invocation_id) lid3: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>) {{\n\
         \x20   let tx = lid3.x; let ty = lid3.y; let lid = ty * 16u + tx;\n\
         \x20   let m = P[2]; let n = P[3]; let kk = P[4];\n\
         \x20   let row0 = wid.y * 64u; let col0 = wid.x * 64u; let bi = wid.z;\n\
         \x20   let b1 = bi % P[10]; let b0 = bi / P[10];\n\
         \x20   let ab = P[17] + b0 * P[11] + b1 * P[12];\n\
         \x20   let bb = P[18] + b0 * P[13] + b1 * P[14];\n\
         \x20   let ch = P[16];\n\
         \x20   var tiles = 0u;\n\
         {decl}\
         \x20   for (var k0 = 0u; k0 < kk; k0 = k0 + 32u) {{\n\
         {load_a}\
         \x20       for (var q = 0u; q < 8u; q = q + 1u) {{\n\
         \x20           let li = lid * 8u + q; let bk = li >> 6u; let bc = li & 63u;\n\
         \x20           var bv = 0i;\n\
         \x20           if (k0 + bk < kk && col0 + bc < n) {{ bv = ld32_1(bb + (k0 + bk) * P[7] + (col0 + bc) * P[8]); }}\n\
         \x20           TB[li] = bv;\n\
         \x20       }}\n\
         \x20       workgroupBarrier();\n\
         \x20       for (var t = 0u; t < 32u; t = t + 1u) {{\n\
         \x20           let a0 = TA[(ty * 4u) * 32u + t]; let a1 = TA[(ty * 4u + 1u) * 32u + t];\n\
         \x20           let a2 = TA[(ty * 4u + 2u) * 32u + t]; let a3 = TA[(ty * 4u + 3u) * 32u + t];\n\
         \x20           let c0 = TB[t * 64u + tx * 4u]; let c1 = TB[t * 64u + tx * 4u + 1u];\n\
         \x20           let c2 = TB[t * 64u + tx * 4u + 2u]; let c3 = TB[t * 64u + tx * 4u + 3u];\n\
         {mac}\
         \x20       }}\n\
         \x20       workgroupBarrier();\n\
         \x20       tiles = tiles + 1u;\n\
         \x20       if (tiles >= ch) {{\n\
         {flush}\
         \x20           tiles = 0u;\n\
         \x20       }}\n\
         \x20   }}\n\
         {store}}}\n"
    ));
    s
}

/// `TopK`, pass 1: per element of a contiguous `[outer, n, inner]` operand, `1` if it is among the
/// row's `k` largest by (value descending, index ascending) — the definition, counted.
pub fn topk_rank_source(x: Form, wide: bool) -> String {
    let mut s = header(Form::U32, &[x]);
    let (ld, beats) =
        if wide { ("ld128_0", "w_lt(xt, xu) || (w_eq(xu, xt) && u < t)") } else { ("ld0", "xu > xt || (xu == xt && u < t)") };
    s.push_str(&format!(
        "\n@compute @workgroup_size(256)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {{\n\
         \x20   let e = gid.x + gid.y * nwg.x * 256u;\n\
         \x20   if (e >= P[0]) {{ return; }}\n\
         \x20   let n = P[2]; let inner = P[3]; let kk = P[4]; let base = P[5];\n\
         \x20   let i = e % inner; let rest = e / inner; let t = rest % n; let o = rest / n;\n\
         \x20   let rb = base + o * n * inner + i;\n\
         \x20   let xt = {ld}(rb + t * inner);\n\
         \x20   var ahead = 0u;\n\
         \x20   for (var u = 0u; u < n; u = u + 1u) {{\n\
         \x20       let xu = {ld}(rb + u * inner);\n\
         \x20       if ({beats}) {{ ahead = ahead + 1u; }}\n\
         \x20   }}\n\
         \x20   O[e] = select(0u, 1u, ahead < kk);\n}}\n"
    ));
    s
}

/// `TopK`, pass 2: per row, the selected indices in ascending order into the `k` output slots.
pub fn topk_emit_source() -> String {
    let mut s = header(Form::U32, &[Form::U32]);
    s.push_str(
        "\n@compute @workgroup_size(64)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {\n\
         \x20   let row = gid.x + gid.y * nwg.x * 64u;\n\
         \x20   if (row >= P[0]) { return; }\n\
         \x20   let n = P[2]; let inner = P[3]; let kk = P[4];\n\
         \x20   let i = row % inner; let o = row / inner;\n\
         \x20   var slot = 0u;\n\
         \x20   for (var t = 0u; t < n; t = t + 1u) {\n\
         \x20       if (B0[(o * n + t) * inner + i] != 0u) {\n\
         \x20           O[(o * kk + slot) * inner + i] = t;\n\
         \x20           slot = slot + 1u;\n\
         \x20       }\n\
         \x20   }\n}\n",
    );
    s
}
