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
        } else {
            out[base + (ulong)i] = shr_clamp_i32(v, r, h);
        }
    }
}

