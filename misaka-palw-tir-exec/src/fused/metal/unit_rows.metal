#include <metal_stdlib>
using namespace metal;

// The integer unit-row kernels (l2_unit_q15, rms_unit_q24) of misaka-palw-tir-exec's fused module, on the GPU. Integer arithmetic
// only: every step is the template's step on the same values, and the exact sum of squares is order-free.

constant long K_ = 24;
constant long ONE_ = 1L << 24;
constant long SEED[16] = {15395829L, 14307657L, 13421772L, 12682383L, 12053107L, 11509075L, 11032629L, 10610843L,
                          10234005L, 9894662L, 9586980L, 9306325L, 9048957L, 8811825L, 8592409L, 8388608L};

inline long log2_floor_(long x) { return x <= 0 ? -1 : 63 - (long)clz((ulong)x); }

inline long int_rsqrt_(long v) {
    if (v <= 0) return 0;
    long bit = log2_floor_(v);
    long e = (bit - K_) >> 1;
    long m = (2 * e >= 0) ? (v >> (2 * e)) : (v << (-2 * e));
    while (m >= 4 * ONE_) { m >>= 2; e += 1; }
    while (m < ONE_) { m <<= 2; e -= 1; }
    long index = ((m - ONE_) * 16) / (3 * ONE_);
    index = index < 0 ? 0 : (index > 15 ? 15 : index);
    long y = SEED[index];
    for (int i = 0; i < 3; i++) {
        long y2 = (y * y) >> K_;
        long my2 = (m * y2) >> K_;
        y = (y * (3 * ONE_ - my2)) >> (K_ + 1);
        if (y <= 0) y = 1;
    }
    return e >= 0 ? (y >> e) : (y << (-e));
}


// ---- 128-bit signed words as (hi, lo): the template's i128 steps where a product leaves 64 bits ----
struct W128 { long hi; ulong lo; };
inline W128 mul64(long a, long b) { W128 r; r.lo = (ulong)(a * b); r.hi = mulhi(a, b); return r; }
inline W128 neg128(W128 x) { W128 r; r.lo = ~x.lo + 1; r.hi = ~x.hi + (r.lo == 0 ? 1 : 0); return r; }
inline W128 add_lo(W128 x, ulong v) { W128 r; r.lo = x.lo + v; r.hi = x.hi + (r.lo < x.lo ? 1 : 0); return r; }
inline W128 sar128(W128 x, long s) {
    if (s == 0) return x;
    W128 r; r.hi = x.hi >> s; r.lo = (x.lo >> s) | ((ulong)x.hi << (64 - s)); return r;
}
inline W128 shl128(W128 x, long s) {
    if (s == 0) return x;
    W128 r; r.hi = (x.hi << s) | (long)(x.lo >> (64 - s)); r.lo = x.lo << s; return r;
}
// round half away from zero of x / 2^s (0 <= s <= 62), exactly the rounding rule of the template's Div(HAFZ) by a power of two
inline W128 shr_hafz128(W128 x, long s) {
    if (s == 0) return x;
    bool negative = x.hi < 0;
    if (negative) x = neg128(x);
    x = add_lo(x, 1UL << (s - 1));
    x = sar128(x, s);
    return negative ? neg128(x) : x;
}
inline long clamp_i64_of(W128 x) {
    if (x.hi == 0 && x.lo <= (ulong)0x7FFFFFFFFFFFFFFFUL) return (long)x.lo;
    if (x.hi == -1 && x.lo >= (ulong)0x8000000000000000UL) return (long)x.lo;
    return x.hi < 0 ? (long)0x8000000000000000UL : 0x7FFFFFFFFFFFFFFFL;
}
inline long sat_add(long a, long b) {
    long r = (long)((ulong)a + (ulong)b);
    if (b >= 0 && r < a) return 0x7FFFFFFFFFFFFFFFL;
    if (b < 0 && r > a) return (long)0x8000000000000000UL;
    return r;
}
inline long sat_sub(long a, long b) {
    long r = (long)((ulong)a - (ulong)b);
    if (b >= 0 && r > a) return (long)0x8000000000000000UL;
    if (b < 0 && r < a) return 0x7FFFFFFFFFFFFFFFL;
    return r;
}
inline long clampl(long x, long lo, long hi) { return x < lo ? lo : (x > hi ? hi : x); }
// the A16 narrowing clamp_[lo,hi]( sat64( HAFZ(x*m / 2^s) ) + z ), `x`, `m` and `z` words
inline long narrow_pow2(long x, long m, long s, long z, long lo, long hi) {
    long q = clamp_i64_of(shr_hafz128(mul64(x, m), s));
    return clampl(sat_add(q, z), lo, hi);
}
// clamp a 128-bit word to i32
inline int clamp_i32_of(W128 x) {
    if (x.hi > 0 || (x.hi == 0 && x.lo > (ulong)2147483647UL)) return 2147483647;
    if (x.hi < -1 || (x.hi == -1 && x.lo < (ulong)0xFFFFFFFF80000000UL)) return (int)0x80000000;
    return (int)(long)x.lo;
}

// (a * b) >> h, floor, clamped to i32 — through the 128-bit product.
inline int shr_clamp_i32(long a, long b, long h) {
    ulong lo = (ulong)(a * b);
    long hi = mulhi(a, b);
    long hs;
    ulong low;
    if (h == 0) { hs = hi; low = lo; } else { hs = hi >> h; low = (lo >> h) | ((ulong)hi << (64 - h)); }
    if (hs > 0 || (hs == 0 && low > (ulong)2147483647UL)) return 2147483647;
    if (hs < -1 || (hs == -1 && low < (ulong)0xFFFFFFFF80000000UL)) return (int)0x80000000;
    return (int)(long)low;
}

// params: [kind (0 = l2, 1 = rms), n, eps]; flag[0] is set when a row leaves the range this kernel computes in (the host re-runs on the CPU).
kernel void unit_rows(device const int* x [[buffer(0)]], device int* out [[buffer(1)]], device const long* params [[buffer(2)]],
                      device atomic_int* flag [[buffer(3)]], uint row [[threadgroup_position_in_grid]],
                      uint tid [[thread_position_in_threadgroup]], uint tg [[threads_per_threadgroup]]) {
    threadgroup ulong partial[256];
    threadgroup long shared_r;
    threadgroup long shared_h;
    const long kind = params[0];
    const long n = params[1];
    const ulong base = (ulong)row * (ulong)n;
    ulong s = 0;
    for (long i = (long)tid; i < n; i += (long)tg) {
        long v = (long)x[base + (ulong)i];
        s += (ulong)(v * v);
    }
    partial[tid] = s;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (uint stride = tg / 2; stride > 0; stride >>= 1) {
        if (tid < stride) partial[tid] += partial[tid + stride];
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    if (tid == 0) {
        ulong sum = partial[0];
        if (kind == 0) {
            long sl = (long)sum;
            long h = (log2_floor_(sl) - K_) >> 1;
            h = h < 0 ? 0 : (h > 20 ? 20 : h);
            long m = sl >> (2 * h);
            shared_r = int_rsqrt_(m);
            shared_h = h + 21;
        } else if (kind == 2) {
            // rms_norm_wide_q36: eps = eps_zero * 2^eps_shift arrives computed (the host takes the rows whose eps leaves 63 bits to the CPU)
            ulong eps = (ulong)params[2];
            ulong q = sum / (ulong)n;
            ulong rem = sum % (ulong)n;
            bool bad = (q >> 38) != 0;
            ulong mean0 = (q << 24) + ((rem << 24) / (ulong)n);
            bad = bad || (mean0 > (~(ulong)0) - eps);
            ulong mean = mean0 + eps;
            long bit = mean == 0 ? -1 : 63 - (long)clz(mean);
            long h = (bit - K_) >> 1;
            long two_h = 2 * h;
            ulong m;
            if (mean == 0) { m = 0; h = 0; }
            else if (two_h >= 0) m = mean >> two_h;
            else { ulong small = mean > (ulong)ONE_ ? (ulong)ONE_ : mean; m = small << (-two_h); }
            long ml = m > (ulong)0x7FFFFFFFFFFFFFFFUL ? 0x7FFFFFFFFFFFFFFFL : (long)m;
            shared_r = mean == 0 ? 0 : int_rsqrt_(ml);
            shared_h = h;
            if (bad) atomic_store_explicit(flag, 1, memory_order_relaxed);
        } else {
            ulong eps = (ulong)params[2];
            ulong q = sum / (ulong)n;
            ulong rem = sum % (ulong)n;
            bool bad = (q >> 38) != 0;
            ulong mean0 = (q << 24) + ((rem << 24) / (ulong)n);
            bad = bad || (mean0 > (~(ulong)0) - eps);
            ulong mean = mean0 + eps;
            long bit = mean == 0 ? -1 : 63 - (long)clz(mean);
            long h = (bit - K_) >> 1;
            h = h < 0 ? 0 : (h > 51 ? 51 : h);
            ulong m = mean >> (2 * h);
            long ml = m > (ulong)0x7FFFFFFFFFFFFFFFUL ? 0x7FFFFFFFFFFFFFFFL : (long)m;
            shared_r = int_rsqrt_(ml);
            shared_h = h;
            if (bad) atomic_store_explicit(flag, 1, memory_order_relaxed);
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    const long r = shared_r;
    const long h = shared_h;
    for (long i = (long)tid; i < n; i += (long)tg) {
        long v = (long)x[base + (ulong)i];
        if (kind == 0) {
            long y = (v * r) >> h;
            out[base + (ulong)i] = (int)(y < -32767 ? -32767 : (y > 32767 ? 32767 : y));
        } else if (kind == 2) {
            W128 prod = mul64(v, r);
            W128 y = h >= 0 ? sar128(prod, h) : shl128(prod, -h);
            out[base + (ulong)i] = clamp_i32_of(y);
        } else {
            out[base + (ulong)i] = shr_clamp_i32(v, r, h);
        }
    }
}


// ---- gdn_step_q36: one position of the gated delta rule, a thread per (head, value row) ----
// P: [h, dv, dk, s_lo, s_hi] then twelve per-head arrays of h words: decay, beta, rm, rs, rz, dm, ds, dz, ws, om, os, oz
// (r/d/o: the read, delta and output narrowings' multiplier, shift (log2 of the divisor) and zero).
kernel void gdn_rows(device const int* s_now [[buffer(0)]], device const int* kk [[buffer(1)]], device const int* vv [[buffer(2)]],
                     device const int* qq [[buffer(3)]], device int* s_next [[buffer(4)]], device int* out [[buffer(5)]],
                     device const long* P [[buffer(6)]], uint gid [[thread_position_in_grid]]) {
    const long H = P[0], DV = P[1], DK = P[2], S_LO = P[3], S_HI = P[4];
    if ((long)gid >= H * DV) return;
    const long hh = (long)gid / DV;
    device const long* A = P + 5;
    const long dec = A[0 * H + hh], bt = A[1 * H + hh], rm = A[2 * H + hh], rs = A[3 * H + hh], rz = A[4 * H + hh];
    const long dm = A[5 * H + hh], ds = A[6 * H + hh], dz = A[7 * H + hh], wsh = A[8 * H + hh];
    const long om = A[9 * H + hh], os = A[10 * H + hh], oz = A[11 * H + hh];
    const long SMAX = 2147483647L;
    const long row = (long)gid * DK;          // (hh * DV + j) * DK
    const long base = hh * DK;
    const long left_on = wsh >= 0;
    const long lp = 1L << clampl(wsh, 0, 20);
    const long rp_shift = clampl(0 - wsh, 0, 62);
    // 1-2. S1 = clamp(HAFZ(S * decay / 2^24), +-(2^31 - 1)), and the read's exact sum
    long acc = 0;
    for (long i = 0; i < DK; i++) {
        long now = (long)s_now[row + i];
        long s1 = clampl(clamp_i64_of(shr_hafz128(mul64(now, dec), 24)), -SMAX, SMAX);
        acc = (long)((ulong)acc + (ulong)(s1 * (long)kk[base + i]));
    }
    long w = narrow_pow2(acc, rm, rs, rz, (long)0x8000000000000000UL, 0x7FFFFFFFFFFFFFFFL);
    // 3. u = narrow_delta(HAFZ(sat64(sat64(v - w) * beta) / 2^24))
    long diff = sat_sub((long)vv[gid], w);
    long db = clamp_i64_of(mul64(diff, bt));
    long scaled = clamp_i64_of(shr_hafz128(mul64(db, 1), 24));
    long u = narrow_pow2(scaled, dm, ds, dz, -16777215L, 16777215L);
    // 4-5. the rank-one write saturated into the state, and the output's exact sum
    long acc_o = 0;
    for (long i = 0; i < DK; i++) {
        long now = (long)s_now[row + i];
        long s1 = clampl(clamp_i64_of(shr_hafz128(mul64(now, dec), 24)), -SMAX, SMAX);
        long prod = (long)((ulong)u * (ulong)(long)kk[base + i]);
        long wr = left_on ? clamp_i64_of(mul64(prod, lp)) : clamp_i64_of(shr_hafz128(mul64(prod, 1), rp_shift));
        long n = clampl(sat_add(s1, wr), S_LO, S_HI);
        s_next[row + i] = (int)n;
        acc_o = (long)((ulong)acc_o + (ulong)(n * (long)qq[base + i]));
    }
    long o = narrow_pow2(acc_o, om, os, oz, -2147483648L, 2147483647L);
    out[gid] = (int)o;
}
