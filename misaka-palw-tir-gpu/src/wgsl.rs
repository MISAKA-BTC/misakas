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

/// The bindings and helpers every kernel starts with: `P`, `O` (of `out`), `S`, the operands, a
/// 64-bit constant reader and `fail`.
pub fn header(out: Form, ins: &[Form]) -> String {
    let mut s = String::new();
    s.push_str("@group(0) @binding(0) var<storage, read> P: array<u32>;\n");
    s.push_str(&format!("@group(0) @binding(1) var<storage, read_write> O: array<{}>;\n", out.wgsl_array()));
    s.push_str("@group(0) @binding(2) var<storage, read_write> S: array<atomic<u32>>;\n");
    for (k, f) in ins.iter().enumerate() {
        s.push_str(&loader(k, *f));
    }
    s.push_str(&storer(out));
    s.push_str(
        r#"
fn p64(i: u32) -> i64 { return bitcast<i64>((u64(P[i + 1u]) << 32u) | u64(P[i])); }
fn fail(e: u32, kind: u32) { atomicMax(&S[P[1]], ~((e << 2u) | kind)); }
"#,
    );
    s.push_str(INTLIB);
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
    };
    decl + &body
}

/// `st(i, v)`: element `i` of `O` holds `v` (which the plan proved, or the kernel checked, to be a
/// value of the output's dtype — a lane keeps its low 32 bits).
pub fn storer(f: Form) -> String {
    match f {
        Form::S32 | Form::U32 => "fn st(i: u32, v: i64) { O[i] = u32(bitcast<u64>(v) & 0xFFFFFFFFlu); }\n".to_string(),
        Form::I64 => "fn st(i: u32, v: i64) { O[i] = v; }\n".to_string(),
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
    /// Check the result against the output dtype's bounds (two `i64` constants after the op's own).
    pub check_out: bool,
    /// `Gather`: check every index against its axis.
    pub check_operand: bool,
}

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
    let load = |j: usize| format!("    let x{j} = ld{j}(off{j});\n");
    match k.op {
        EwOp::Gather => {
            // P[cc]: the gathered axis's extent; P[cc + 1]: its stride in the data.
            s.push_str(&load(1));
            if k.check_operand {
                s.push_str(&format!("    if (x1 < 0li || x1 >= i64(P[{cc}])) {{ fail(e, 3u); return; }}\n"));
            }
            s.push_str(&format!("    var v = ld0(off0 + u32(x1) * P[{}]);\n", cc + 1));
        }
        EwOp::Iota => {
            // P[cc]: the axis, right-aligned to rank 4.
            s.push_str(&format!(
                "    var ix = array<u32, 4>(i0, i1, i2, i3);\n    var v = p64({c}u) + p64({}u) * i64(ix[P[{cc}]]);\n",
                c + 2
            ));
        }
        op => {
            for j in 0..n {
                s.push_str(&load(j));
            }
            let body = match op {
                EwOp::Copy => "    var v = x0;\n".to_string(),
                EwOp::Add => "    var v = x0 + x1;\n".to_string(),
                EwOp::Sub => "    var v = x0 - x1;\n".to_string(),
                EwOp::Mul => "    var v = x0 * x1;\n".to_string(),
                EwOp::Div(rule) => format!("    if (x1 < 1li) {{ fail(e, 2u); return; }}\n    var v = div_rule64(x0, x1, {rule}u);\n"),
                EwOp::Compare(cmp) => {
                    let rel = ["==", "!=", "<", "<=", ">", ">="][cmp as usize];
                    format!("    var v = select(0li, 1li, x0 {rel} x1);\n")
                }
                EwOp::Select => "    var v = select(x2, x1, x0 != 0li);\n".to_string(),
                EwOp::Clamp => format!("    var v = min(max(x0, p64({c}u)), p64({}u));\n", c + 2),
                EwOp::Log2Floor => "    var v = log2_floor64(x0);\n".to_string(),
                EwOp::IntExp => "    var v = int_exp64(x0);\n".to_string(),
                EwOp::IntRsqrt => "    var v = int_rsqrt64(x0);\n".to_string(),
                EwOp::IntLn => "    var v = int_ln64(x0);\n".to_string(),
                EwOp::Iota | EwOp::Gather => unreachable!(),
            };
            s.push_str(&body);
        }
    }
    if k.check_out {
        let b = c + 2 * k.op.consts() + if k.op == EwOp::Gather || k.op == EwOp::Iota { 2 } else { 0 };
        s.push_str(&format!("    if (v < p64({b}u) || v > p64({}u)) {{ fail(e, 1u); return; }}\n", b + 2));
    }
    s.push_str("    st(off_o, v);\n}\n");
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
}

/// Parameter block: `P[0]` outputs, `P[1]` status slot, `P[2..11]` output geometry, `P[11..16]`
/// the operand's `(base, strides)` at the output's multi-index, `P[16]` the axis extent, `P[17]`
/// its stride, `P[18..22]` the dtype bounds (Pn).
pub fn reduce_source(k: &ReduceKey) -> String {
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
        s.push_str("    let n = P[16]; let st_ax = P[17];\n");
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
    s.push_str("    let n = P[16]; let st_ax = P[17];\n");
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

/// `MatMul`, one invocation per output element, any strides and forms: the general kernel.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MatMulKey {
    pub a: Form,
    pub b: Form,
    pub out: Form,
    pub mode: SumMode,
    /// Terms are formed in `i32` and summed in `i32` chunks of at most `P[16]` terms (the plan
    /// proved every operand inside `i32` and `chunk · max|term| < 2^31`); otherwise `i64` terms.
    pub chunked: bool,
}

/// Parameter block shared by the MatMul kernels: `P[0]` outputs, `P[1]` slot, `P[2]` M, `P[3]` N,
/// `P[4]` K, `P[5]` a_m, `P[6]` a_k, `P[7]` b_k, `P[8]` b_n, `P[9..11]` the output batch dims (two,
/// right-aligned), `P[11..13]` / `P[13..15]` `a`'s / `b`'s batch strides, `P[15]` reserved, `P[16]`
/// the i32 chunk, `P[17]` / `P[18]` the operands' base offsets, `P[19..23]` the dtype bounds (Pn),
/// `P[23]` the output base offset.
pub fn matmul_source(k: &MatMulKey) -> String {
    let mut s = header(k.out, &[k.a, k.b]);
    s.push_str(
        "\n@compute @workgroup_size(256)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {\n\
         \x20   let e = gid.x + gid.y * nwg.x * 256u;\n\
         \x20   if (e >= P[0]) { return; }\n\
         \x20   let m = P[2]; let n = P[3]; let kk = P[4];\n\
         \x20   let c = e % n; let rest = e / n; let r = rest % m; let bi = rest / m;\n\
         \x20   let b1 = bi % P[10]; let b0 = bi / P[10];\n\
         \x20   let ao = P[17] + b0 * P[11] + b1 * P[12] + r * P[5];\n\
         \x20   let bo = P[18] + b0 * P[13] + b1 * P[14] + c * P[8];\n\
         \x20   let a_k = P[6]; let b_k = P[7];\n",
    );
    let (acc_decl, fold64) = match k.mode {
        SumMode::Fast => ("    var acc = 0li;\n", "acc = acc + "),
        SumMode::PosNeg => ("    var accp = 0li; var accn = 0li;\n", ""),
    };
    s.push_str(acc_decl);
    if k.chunked {
        // Every term and every chunk total fit i32: chunk totals are exact, then widened.
        let flush = match k.mode {
            SumMode::Fast => "acc = acc + i64(sp);".to_string(),
            SumMode::PosNeg => "accp = accp + i64(sp); accn = accn + i64(sn);".to_string(),
        };
        let (decl, add) = match k.mode {
            SumMode::Fast => ("var sp = 0i;", "sp = sp + x * y;"),
            SumMode::PosNeg => ("var sp = 0i; var sn = 0i;", "let q = x * y; if (q > 0i) { sp = sp + q; } else { sn = sn + q; }"),
        };
        s.push_str(&format!(
            "    let ch = P[16];\n    var t = 0u;\n    loop {{\n        if (t >= kk) {{ break; }}\n        let end = min(t + ch, kk);\n        {decl}\n\
             \x20       for (; t < end; t = t + 1u) {{ let x = ld32_0(ao + t * a_k); let y = ld32_1(bo + t * b_k); {add} }}\n        {flush}\n    }}\n"
        ));
    } else {
        match k.mode {
            SumMode::Fast => s.push_str(&format!(
                "    for (var t = 0u; t < kk; t = t + 1u) {{ {fold64}ld0(ao + t * a_k) * ld1(bo + t * b_k); }}\n"
            )),
            SumMode::PosNeg => s.push_str(
                "    for (var t = 0u; t < kk; t = t + 1u) { let q = ld0(ao + t * a_k) * ld1(bo + t * b_k); if (q > 0li) { accp = accp + q; } else { accn = accn + q; } }\n",
            ),
        }
    }
    match k.mode {
        SumMode::Fast => s.push_str("    let v = acc;\n"),
        SumMode::PosNeg => {
            s.push_str("    if (accp > p64(19u) || accn < p64(21u)) { fail(e, 1u); return; }\n    let v = accp + accn;\n")
        }
    }
    s.push_str("    st(P[23] + e, v);\n}\n");
    s
}

/// `MatMul` as a matrix–vector product with `i8` packed weight rows: `out[r] = Σ_t W[r, t]·x[t]`,
/// one 64-invocation lane group per row, four rows per workgroup. `W` rows are contiguous and
/// word-aligned (`K % 4 = 0`, base and row stride multiples of 4 elements).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GemvKey {
    pub b: Form,
    pub out: Form,
    /// i32 terms in chunks of `P[16]` packed WORDS (4 terms each); otherwise `i64` terms.
    pub chunked: bool,
}

pub fn gemv_source(k: &GemvKey) -> String {
    let mut s = header(k.out, &[Form::P8, k.b]);
    s.push_str("var<workgroup> W0: array<i64, 256>;\n");
    s.push_str(
        "\n@compute @workgroup_size(256)\nfn main(@builtin(local_invocation_index) lid: u32, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {\n\
         \x20   let row = (wid.x + wid.y * nwg.x) * 4u + (lid >> 6u);\n\
         \x20   let lane = lid & 63u;\n\
         \x20   let m = P[2]; let kw = P[4] >> 2u;\n\
         \x20   var acc = 0li;\n\
         \x20   if (row < m) {\n\
         \x20       let wbase = (P[17] + row * P[5]) >> 2u;\n\
         \x20       let bo = P[18]; let b_k = P[7];\n",
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
         \x20   for (var w = 32u; w > 0u; w = w >> 1u) { if (lane < w) { W0[lid] = W0[lid] + W0[lid + w]; } workgroupBarrier(); }\n\
         \x20   if (lane == 0u && row < m) { st(P[23] + row, W0[lid]); }\n}\n",
    );
    s
}

/// `MatMul` tiled through workgroup memory for many output columns (a batch of positions):
/// 64×64 outputs per workgroup of 16×16 invocations, each 4×4, a K tile of 16. Any strides.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GemmKey {
    pub a: Form,
    pub b: Form,
    pub out: Form,
    /// i32 terms, summed in i32 over `P[16]` K-tiles before widening; otherwise `i64` terms.
    pub chunked: bool,
}

pub fn gemm_source(k: &GemmKey) -> String {
    let mut s = header(k.out, &[k.a, k.b]);
    let ty = if k.chunked { "i32" } else { "i64" };
    s.push_str(&format!("var<workgroup> TA: array<{ty}, 1024>;\nvar<workgroup> TB: array<{ty}, 1024>;\n"));
    let (lda, ldb) = if k.chunked { ("ld32_0", "ld32_1") } else { ("ld0", "ld1") };
    let zero = if k.chunked { "0i" } else { "0li" };
    s.push_str(&format!(
        "\n@compute @workgroup_size(16, 16)\nfn main(@builtin(local_invocation_id) lid3: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>) {{\n\
         \x20   let tx = lid3.x; let ty = lid3.y; let lid = ty * 16u + tx;\n\
         \x20   let m = P[2]; let n = P[3]; let kk = P[4];\n\
         \x20   let row0 = wid.y * 64u; let col0 = wid.x * 64u; let bi = wid.z;\n\
         \x20   let b1 = bi % P[10]; let b0 = bi / P[10];\n\
         \x20   let ab = P[17] + b0 * P[11] + b1 * P[12];\n\
         \x20   let bb = P[18] + b0 * P[13] + b1 * P[14];\n\
         \x20   var acc: array<i64, 16>;\n\
         \x20   for (var i = 0u; i < 16u; i = i + 1u) {{ acc[i] = 0li; }}\n\
         \x20   var part: array<{ty}, 16>;\n\
         \x20   let ch = P[16];\n\
         \x20   var tiles = 0u;\n\
         \x20   for (var i = 0u; i < 16u; i = i + 1u) {{ part[i] = {zero}; }}\n\
         \x20   for (var k0 = 0u; k0 < kk; k0 = k0 + 16u) {{\n\
         \x20       for (var q = 0u; q < 4u; q = q + 1u) {{\n\
         \x20           let li = lid * 4u + q;\n\
         \x20           let ar = li / 16u; let ak = li % 16u;\n\
         \x20           var av = {zero};\n\
         \x20           if (row0 + ar < m && k0 + ak < kk) {{ av = {lda}(ab + (row0 + ar) * P[5] + (k0 + ak) * P[6]); }}\n\
         \x20           TA[li] = av;\n\
         \x20           let bk = li / 64u; let bc = li % 64u;\n\
         \x20           var bv = {zero};\n\
         \x20           if (k0 + bk < kk && col0 + bc < n) {{ bv = {ldb}(bb + (k0 + bk) * P[7] + (col0 + bc) * P[8]); }}\n\
         \x20           TB[li] = bv;\n\
         \x20       }}\n\
         \x20       workgroupBarrier();\n\
         \x20       for (var t = 0u; t < 16u; t = t + 1u) {{\n\
         \x20           for (var i = 0u; i < 4u; i = i + 1u) {{\n\
         \x20               let a = TA[(ty * 4u + i) * 16u + t];\n\
         \x20               for (var j = 0u; j < 4u; j = j + 1u) {{ part[i * 4u + j] = part[i * 4u + j] + a * TB[t * 64u + tx * 4u + j]; }}\n\
         \x20           }}\n\
         \x20       }}\n\
         \x20       workgroupBarrier();\n\
         \x20       tiles = tiles + 1u;\n\
         \x20       if (tiles >= ch) {{\n\
         \x20           for (var i = 0u; i < 16u; i = i + 1u) {{ acc[i] = acc[i] + i64(part[i]); part[i] = {zero}; }}\n\
         \x20           tiles = 0u;\n\
         \x20       }}\n\
         \x20   }}\n\
         \x20   for (var i = 0u; i < 4u; i = i + 1u) {{\n\
         \x20       for (var j = 0u; j < 4u; j = j + 1u) {{\n\
         \x20           let r = row0 + ty * 4u + i; let c = col0 + tx * 4u + j;\n\
         \x20           if (r < m && c < n) {{ st(P[23] + (bi * m + r) * n + c, acc[i * 4u + j] + i64(part[i * 4u + j])); }}\n\
         \x20       }}\n\
         \x20   }}\n}}\n"
    ));
    s
}

/// `TopK`, pass 1: per element of a contiguous `[outer, n, inner]` operand, `1` if it is among the
/// row's `k` largest by (value descending, index ascending) — the definition, counted.
pub fn topk_rank_source(x: Form) -> String {
    let mut s = header(Form::U32, &[x]);
    s.push_str(
        "\n@compute @workgroup_size(256)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {\n\
         \x20   let e = gid.x + gid.y * nwg.x * 256u;\n\
         \x20   if (e >= P[0]) { return; }\n\
         \x20   let n = P[2]; let inner = P[3]; let kk = P[4]; let base = P[5];\n\
         \x20   let i = e % inner; let rest = e / inner; let t = rest % n; let o = rest / n;\n\
         \x20   let rb = base + o * n * inner + i;\n\
         \x20   let xt = ld0(rb + t * inner);\n\
         \x20   var ahead = 0u;\n\
         \x20   for (var u = 0u; u < n; u = u + 1u) {\n\
         \x20       let xu = ld0(rb + u * inner);\n\
         \x20       if (xu > xt || (xu == xt && u < t)) { ahead = ahead + 1u; }\n\
         \x20   }\n\
         \x20   O[e] = select(0u, 1u, ahead < kk);\n}\n",
    );
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
