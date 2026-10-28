//! **Recurrences over positions** — `Fixed` states updated once per position (corpus C4–C6).
//!
//! * Head mapping for `k ≠ v` heads ([`BlockBuilder::map_heads_group`]: HF's `repeat_interleave`,
//!   value head `vh` reads key head `vh / r`; [`BlockBuilder::map_heads_tile`]: the live kernel's
//!   `vh % k`). The mapping is data inside the program, so a class says which one it computes.
//! * The causal conv window ([`BlockBuilder::causal_conv`]) and token shift
//!   ([`BlockBuilder::token_shift`], [`BlockBuilder::lerp_q24`]).
//! * Exponentials of either sign ([`BlockBuilder::exp_q24`]) and the double exponential
//!   `exp(−exp(y))` of RWKV-6/7's data-dependent decay ([`BlockBuilder::exp_neg_exp_q24`]). A decay
//!   close to 1 is formed with the REFINED exponential (`exp_refined_q36`: one Newton step against
//!   `IntLn`) — `IntExp` alone is `2.7·10^−4` above `ONE` at 0, which would make a decay of
//!   `1 − 10^−4` grow the state instead of shrinking it.
//! * The selective scan: Mamba-1 ([`BlockBuilder::mamba1_step`], `A` and `dt` per channel and state:
//!   `d_inner · N` exponentials per layer per position — 81,920 for a 2.8B model, doubled when
//!   refined) and Mamba-2 ([`BlockBuilder::mamba2_step`], one decay per head, `B`/`C` per group).
//! * WKV: RWKV-4 ([`BlockBuilder::rwkv4_step`], the max-stabilised numerator/denominator form, with
//!   one exact division by a data denominator), RWKV-6 ([`BlockBuilder::rwkv6_step`], `S ← diag(w)S +
//!   kᵀv`, bonus `u`), RWKV-7 ([`BlockBuilder::rwkv7_step`], the generalised delta rule
//!   `S ← S(diag(w) − k̂ᵀ(a⊙k̂)) + vᵀk`).
//! * The gated delta rule is [`BlockBuilder::gdn_step_q36`] (legacy-exact); an HF-form layer
//!   composes it with `l2_norm_eps`, [`BlockBuilder::map_heads_group`] and the decay template.
//!
//! Every state update saturates in its `StateWrite` (the state's declared range), which is what
//! makes the range analysis of a recurrence total.

use crate::arith::{K, LN2_Q, ONE};
use crate::builder::BlockBuilder;
use crate::library::Narrowing;
use crate::prim::{Cmp, Rounding};
use crate::program::Ref;
use crate::types::{DType, Dim};

/// The narrowings a selective-scan step needs.
#[derive(Clone, Copy, Debug)]
pub struct ScanCfg {
    /// `dt · x · B` (an exact `i64` product of Q24 × code × code) into the state's units.
    pub input: Narrowing,
    /// `C · h` (`i64`) into the output's units (the `i32` rail).
    pub out_c: Narrowing,
    /// `D · x` (Q24 × code, `i64`) into the output's units.
    pub out_d: Narrowing,
}

/// The narrowings an RWKV-6 WKV step needs.
#[derive(Clone, Copy, Debug)]
pub struct Wkv6Cfg {
    /// `k ⊗ v` (codes², exact) into the state's units.
    pub kv: Narrowing,
    /// `r · (S + u·kv)` (`i64`) into the output's units (codes).
    pub y: Narrowing,
}

/// The narrowings an RWKV-7 step needs.
#[derive(Clone, Copy, Debug)]
pub struct Wkv7Cfg {
    /// `S · k̂` (`i64`) into the units the rank-one removal multiplies.
    pub sa: Narrowing,
    /// `(S·k̂) ⊗ (a ⊙ k̂)` (`i64`) into the state's units.
    pub ab: Narrowing,
    /// `v ⊗ k` (codes², exact) into the state's units.
    pub vk: Narrowing,
    /// `S · r` (`i64`) into the output's units (codes).
    pub y: Narrowing,
}

fn dims<const N: usize>(b: &BlockBuilder<'_>, x: Ref) -> [u32; N] {
    let s = b.shape(x);
    assert_eq!(s.len(), N, "rank");
    std::array::from_fn(|i| match s[i] {
        Dim::Fixed(n) => n,
        Dim::H => panic!("a recurrence is over positions, not H"),
    })
}

impl BlockBuilder<'_> {
    /// **Grouping** `k → k·r` heads, HF's `repeat_interleave(r)`: `[k, d] → [k, 1, d] → [k, r, d]
    /// → [k·r, d]`; value head `vh` reads key head `vh / r`.
    pub fn map_heads_group(&mut self, x: Ref, r: u32) -> Ref {
        let [k, d] = dims::<2>(self, x);
        let x3 = self.reshape_fixed(x, &[k, 1, d]);
        let b = self.broadcast(x3, &[Dim::Fixed(k), Dim::Fixed(r), Dim::Fixed(d)]);
        self.reshape_fixed(b, &[k * r, d])
    }

    /// **Tiling** `k → r·k` heads, the live kernel's `vh % k`: `[k, d] → [1, k, d] → [r, k, d] →
    /// [r·k, d]`. At `k ≠ v` a different function from grouping; at `r = 1` the same.
    pub fn map_heads_tile(&mut self, x: Ref, r: u32) -> Ref {
        let [k, d] = dims::<2>(self, x);
        let x3 = self.reshape_fixed(x, &[1, k, d]);
        let b = self.broadcast(x3, &[Dim::Fixed(r), Dim::Fixed(k), Dim::Fixed(d)]);
        self.reshape_fixed(b, &[r * k, d])
    }

    /// **The causal conv window**: `state:[w−1, C]` holds the previous `w − 1` rows (a `Fixed`
    /// state of `row`'s dtype, oldest first, zeros before the sequence start), `row:[C]` is this
    /// position's, `taps:[w, C]` are aligned with the window (oldest first). Writes the newest
    /// `w − 1` rows back and returns the exact per-channel sum `Σ_t window[t]·taps[t]`, `i64` —
    /// the caller narrows it (and adds a bias as the narrowing's `z`).
    pub fn causal_conv(&mut self, state: u16, row: Ref, taps: Ref) -> Ref {
        let [c] = dims::<1>(self, row);
        let sh = self.shape(Ref::State(state));
        let Dim::Fixed(w1) = sh[0] else { panic!("static") };
        let r1 = self.reshape_fixed(row, &[1, c]);
        let win = self.concat(&[Ref::State(state), r1], 0);
        let keep = self.slice(win, 0, 1, w1);
        self.state_write(state, keep);
        let prod = self.mul(win, taps, DType::I64);
        let acc = self.reduce_sum(prod, 0, DType::I64);
        self.reshape_fixed(acc, &[c])
    }

    /// **Token shift**: returns the previous position's `x` (zeros at the start) and stores this
    /// one. `state` is a `Fixed` state of `x`'s shape.
    pub fn token_shift(&mut self, state: u16, x: Ref) -> Ref {
        self.state_write(state, x);
        Ref::State(state)
    }

    /// `a + ((b − a)·μ) >> 24` for a Q24 `μ` — RWKV's token-shift interpolation
    /// (`x + (x_prev − x)·μ`), in `a`'s dtype (saturated).
    pub fn lerp_q24(&mut self, a: Ref, b: Ref, mu: Ref) -> Ref {
        let dt = self.ty(a).dtype;
        let d = self.sub(b, a, DType::I64);
        let t = self.mul_q24(d, mu, DType::I64);
        let s = self.add(a, t, DType::I128);
        let (lo, hi) = (dt.min_value().max(i64::MIN as i128) as i64, dt.max_value().min(i64::MAX as i128) as i64);
        self.clamp(s, lo, hi, dt)
    }

    /// **`exp(y)` for a Q24 `y` of either sign**, Q24 `i64` out, through ONE refined exponential
    /// (`exp_refined_q36`: `IntExp` and one Newton step against `IntLn` — `IntExp` alone is off by up
    /// to `3.5·10^−3` relative, which a rate inside a decay compounds): `y ≤ 0` is `exp(y)` itself;
    /// `y > 0` is reduced to `y = n·ln2 + r`, `exp(y) = 2^(n+1) · exp(r − ln2)` with the power a
    /// `Pow2` gather, saturating at `2^38 · ONE` (`exp(26.3)`).
    pub fn exp_q24(&mut self, y: Ref) -> Ref {
        let zero = self.c(DType::I32, 0);
        let neg = self.clamp(y, i32::MIN as i64, 0, DType::I32);
        let pos = self.clamp(y, 0, i32::MAX as i64, DType::I32);
        let ln2 = self.c(DType::I32, LN2_Q);
        let n = self.div(pos, ln2, Rounding::Floor, DType::I32);
        let nl = self.mul(n, ln2, DType::I64);
        let r = self.sub(pos, nl, DType::I64);
        let r = self.sub(r, ln2, DType::I64);
        let r = self.clamp(r, -(LN2_Q as i64), 0, DType::I32);
        let le = self.compare(y, zero, Cmp::Le);
        let arg = self.select(le, neg, r, DType::I32);
        let e = self.exp_refined_q36(arg);
        let one = self.c(DType::I32, 1);
        let n1 = self.add(n, one, DType::I32);
        let k = self.select(le, zero, n1, DType::I32);
        let p = self.pow2_128_of(k, 38);
        self.mul(e, p, DType::I64)
    }

    /// **`exp(−exp(y))`**, Q24 in `[0, ONE]` — RWKV-6/7's data-dependent decay. The inner
    /// exponential is [`Self::exp_q24`]; the outer is the REFINED exponential, so a decay near 1
    /// keeps its distance from 1.
    pub fn exp_neg_exp_q24(&mut self, y: Ref) -> Ref {
        let e = self.exp_q24(y);
        let zero = self.c(DType::I32, 0);
        let ne = self.sub(zero, e, DType::I64);
        let ne = self.clamp(ne, i32::MIN as i64, 0, DType::I32);
        self.exp_refined_q36(ne)
    }

    /// The decay `exp(a · dt)` of a scan, `a ≤ 0` and `dt ≥ 0` Q24: refined (precise near 1) or
    /// plain `IntExp`.
    fn scan_decay(&mut self, dt: Ref, a: Ref, refined: bool) -> Ref {
        let arg = self.mul_q24(dt, a, DType::I64);
        let arg = self.clamp(arg, i32::MIN as i64, 0, DType::I32);
        if refined {
            self.exp_refined_q36(arg)
        } else {
            let e = self.int_exp(arg);
            self.clamp(e, 0, ONE as i64, DType::I32)
        }
    }

    /// **One Mamba-2 step** (`mamba2_selective_state_update`, one token). `state:[nh, P, N]`
    /// (`i32`), `x:code[nh, P]`, `bmat`/`cmat:code[ng, N]` (grouped, mapped to heads by grouping),
    /// `dt:[nh]` Q24 (softplus'd and clamped by the caller), `a:[nh]` Q24 (`−exp(A_log)`), `d:[nh]`
    /// Q24. `h ← HAFZ(h·exp(dt·a), 2^24) + N_in(dt·x⊗B)`; `y = N_c(h·C) + N_d(D·x)`, `i32 [nh, P]`.
    #[allow(clippy::too_many_arguments)]
    pub fn mamba2_step(&mut self, state: u16, x: Ref, bmat: Ref, cmat: Ref, dt: Ref, a: Ref, d: Ref, cfg: &ScanCfg) -> Ref {
        let [nh, p] = dims::<2>(self, x);
        let [ng, n] = dims::<2>(self, bmat);
        let da = self.scan_decay(dt, a, true);
        let bh = self.map_heads_group(bmat, nh / ng);
        let ch = self.map_heads_group(cmat, nh / ng);
        let x3 = self.reshape_fixed(x, &[nh, p, 1]);
        let b3 = self.reshape_fixed(bh, &[nh, 1, n]);
        let xb = self.mul(x3, b3, DType::I64);
        let dt3 = self.reshape_fixed(dt, &[nh, 1, 1]);
        let dxb = self.mul(xb, dt3, DType::I64);
        let inc = self.narrow(dxb, &cfg.input, i32::MIN as i64, i32::MAX as i64, DType::I64);
        let da3 = self.reshape_fixed(da, &[nh, 1, 1]);
        let hd = self.mul(Ref::State(state), da3, DType::I64);
        let hd = self.shr(hd, K, Rounding::HalfAwayFromZero, DType::I64);
        let h2 = self.add(hd, inc, DType::I64);
        let h2 = self.state_write(state, h2);
        let c3 = self.reshape_fixed(ch, &[nh, n, 1]);
        let y = self.matmul(h2, c3, DType::I64);
        let y = self.reshape_fixed(y, &[nh, p]);
        let y = self.narrow_wide(y, &cfg.out_c);
        let d2 = self.reshape_fixed(d, &[nh, 1]);
        let dx = self.mul(x, d2, DType::I64);
        let dx = self.narrow_wide(dx, &cfg.out_d);
        let s = self.add(y, dx, DType::I64);
        self.clamp(s, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// **One Mamba-1 step** (`MambaMixer.slow_forward`, one token). `state:[I, N]` (`i32`),
    /// `x:code[I]`, `bvec`/`cvec:code[N]` (shared by every channel), `dt:[I]` Q24, `a:[I, N]` Q24
    /// (`−exp(A_log)`), `d:[I]` Q24. The decay is per `(channel, state)`: `I·N` exponentials,
    /// refined (`2·I·N` transcendentals) or plain. Returns `i32 [I]`.
    #[allow(clippy::too_many_arguments)]
    pub fn mamba1_step(
        &mut self,
        state: u16,
        x: Ref,
        bvec: Ref,
        cvec: Ref,
        dt: Ref,
        a: Ref,
        d: Ref,
        cfg: &ScanCfg,
        refined: bool,
    ) -> Ref {
        let [i] = dims::<1>(self, x);
        let [n] = dims::<1>(self, bvec);
        let dt2 = self.reshape_fixed(dt, &[i, 1]);
        let da = self.scan_decay(dt2, a, refined);
        let x2 = self.reshape_fixed(x, &[i, 1]);
        let b2 = self.reshape_fixed(bvec, &[1, n]);
        let xb = self.mul(x2, b2, DType::I64);
        let dxb = self.mul(xb, dt2, DType::I64);
        let inc = self.narrow(dxb, &cfg.input, i32::MIN as i64, i32::MAX as i64, DType::I64);
        let hd = self.mul(Ref::State(state), da, DType::I64);
        let hd = self.shr(hd, K, Rounding::HalfAwayFromZero, DType::I64);
        let h2 = self.add(hd, inc, DType::I64);
        let h2 = self.state_write(state, h2);
        let c2 = self.reshape_fixed(cvec, &[n, 1]);
        let y = self.matmul(h2, c2, DType::I64);
        let y = self.reshape_fixed(y, &[i]);
        let y = self.narrow_wide(y, &cfg.out_c);
        let dx = self.mul(x, d, DType::I64);
        let dx = self.narrow_wide(dx, &cfg.out_d);
        let s = self.add(y, dx, DType::I64);
        self.clamp(s, i32::MIN as i64, i32::MAX as i64, DType::I32)
    }

    /// **One RWKV-4 WKV step** (`rwkv_linear_attention_cpu`), the max-stabilised form, per channel:
    /// `num`, `den`, `max` are `Fixed [C]` `i32` states (`num` at `v`'s scale, `den` and `max` Q24);
    /// `k`, `u` (`time_first`), `w` (`time_decay = −exp(param)`, `≤ 0`) are Q24; `v` is at the output
    /// scale. `out = (e1·num + e2·v) / (e1·den + e2)` — ONE exact division by a data denominator
    /// (clamped `≥ 1`) — then the state update with `max(max + w, k)`. Returns `i32 [C]`.
    #[allow(clippy::too_many_arguments)]
    pub fn rwkv4_step(&mut self, num: u16, den: u16, max: u16, k: Ref, v: Ref, u: Ref, w: Ref) -> Ref {
        let i32r = |b: &mut Self, x: Ref| b.clamp(x, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let one = self.c(DType::I64, ONE);
        let e_of = |b: &mut Self, x: Ref, m: Ref| {
            let dlt = b.sub(x, m, DType::I64);
            let dlt = b.clamp(dlt, i32::MIN as i64, 0, DType::I32);
            b.int_exp(dlt)
        };
        // The output.
        let ww = self.add(k, u, DType::I64);
        let ww = i32r(self, ww);
        let p = self.max2(Ref::State(max), ww, DType::I32);
        let e1 = e_of(self, Ref::State(max), p);
        let e2 = e_of(self, ww, p);
        let a1 = self.mul(e1, Ref::State(num), DType::I64);
        let a2 = self.mul(e2, v, DType::I64);
        let numer = self.add(a1, a2, DType::I64);
        let b1 = self.mul(e1, Ref::State(den), DType::I64);
        let b2 = self.mul(e2, one, DType::I64);
        let denom = self.add(b1, b2, DType::I64);
        let denom = self.clamp(denom, 1, i64::MAX, DType::I64);
        let scaled = self.mul(numer, one, DType::I128);
        let out = self.div(scaled, denom, Rounding::Floor, DType::I128);
        let out = i32r(self, out);
        // The state update.
        let ww2 = self.add(Ref::State(max), w, DType::I64);
        let ww2 = i32r(self, ww2);
        let p2 = self.max2(ww2, k, DType::I32);
        let f1 = e_of(self, ww2, p2);
        let f2 = e_of(self, k, p2);
        let n1 = self.mul(f1, Ref::State(num), DType::I64);
        let n2 = self.mul(f2, v, DType::I64);
        let nn = self.add(n1, n2, DType::I64);
        let nn = self.shr(nn, K, Rounding::Floor, DType::I64);
        self.state_write(num, nn);
        let d1 = self.mul(f1, Ref::State(den), DType::I64);
        let d2 = self.mul(f2, one, DType::I64);
        let dd = self.add(d1, d2, DType::I64);
        let dd = self.shr(dd, K, Rounding::Floor, DType::I64);
        self.state_write(den, dd);
        self.state_write(max, p2);
        out
    }

    /// **One RWKV-6 WKV step**, per head: `state:[nh, hs, hs]` (`S[h][k][v]`, `i32`), `r`, `k`,
    /// `v:code[nh, hs]`, `w:[nh, hs]` Q24 decay (from [`Self::exp_neg_exp_q24`]), `u:[nh, hs]` Q24
    /// bonus. `y = N_y(r · (S + u·kv))`, `S ← HAFZ(diag(w)·S, 2^24) + kv` with `kv = N_kv(k ⊗ v)`.
    /// Returns codes `[nh, hs]`.
    #[allow(clippy::too_many_arguments)]
    pub fn rwkv6_step(&mut self, state: u16, r: Ref, k: Ref, v: Ref, w: Ref, u: Ref, cfg: &Wkv6Cfg) -> Ref {
        let [nh, hs] = dims::<2>(self, k);
        let k3 = self.reshape_fixed(k, &[nh, hs, 1]);
        let v3 = self.reshape_fixed(v, &[nh, 1, hs]);
        let kv = self.mul(k3, v3, DType::I64);
        let kv = self.narrow(kv, &cfg.kv, i32::MIN as i64, i32::MAX as i64, DType::I64);
        let u3 = self.reshape_fixed(u, &[nh, hs, 1]);
        let ukv = self.mul_q24(u3, kv, DType::I64);
        let sp = self.add(Ref::State(state), ukv, DType::I64);
        let r3 = self.reshape_fixed(r, &[nh, 1, hs]);
        let y = self.matmul(r3, sp, DType::I64);
        let y = self.narrow_codes(y, &cfg.y);
        let y = self.reshape_fixed(y, &[nh, hs]);
        let w3 = self.reshape_fixed(w, &[nh, hs, 1]);
        let sd = self.mul(Ref::State(state), w3, DType::I64);
        let sd = self.shr(sd, K, Rounding::HalfAwayFromZero, DType::I64);
        let s2 = self.add(sd, kv, DType::I64);
        self.state_write(state, s2);
        y
    }

    /// **One RWKV-7 step**, per head: `state:[nh, hs, hs]` as `S[h][v][k]` (`i32`); `r`, `k`,
    /// `v:code[nh, hs]`; `w:[nh, hs]` Q24 decay per key channel; `kk:[nh, hs]` the normalised key
    /// `k̂` (codes); `a:[nh, hs]` Q24 in-context learning rate. The generalised delta rule
    /// `S ← S·diag(w) − (S·k̂) ⊗ (a⊙k̂) + v ⊗ k`, then `y = N_y(S·r)`. Returns codes `[nh, hs]`.
    #[allow(clippy::too_many_arguments)]
    pub fn rwkv7_step(&mut self, state: u16, r: Ref, w: Ref, k: Ref, v: Ref, kk: Ref, a: Ref, cfg: &Wkv7Cfg) -> Ref {
        let [nh, hs] = dims::<2>(self, k);
        let w3 = self.reshape_fixed(w, &[nh, 1, hs]);
        let sw = self.mul(Ref::State(state), w3, DType::I64);
        let sw = self.shr(sw, K, Rounding::HalfAwayFromZero, DType::I64);
        let kk3 = self.reshape_fixed(kk, &[nh, hs, 1]);
        let sa = self.matmul(Ref::State(state), kk3, DType::I64);
        let sa = self.narrow(sa, &cfg.sa, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let ab = self.mul_q24(a, kk, DType::I32);
        let ab3 = self.reshape_fixed(ab, &[nh, 1, hs]);
        let sab = self.mul(sa, ab3, DType::I64);
        let sab = self.narrow(sab, &cfg.ab, i32::MIN as i64, i32::MAX as i64, DType::I64);
        let v3 = self.reshape_fixed(v, &[nh, hs, 1]);
        let k3 = self.reshape_fixed(k, &[nh, 1, hs]);
        let vk = self.mul(v3, k3, DType::I64);
        let vk = self.narrow(vk, &cfg.vk, i32::MIN as i64, i32::MAX as i64, DType::I64);
        let s1 = self.sub(sw, sab, DType::I64);
        let s2 = self.add(s1, vk, DType::I64);
        let s2 = self.state_write(state, s2);
        let r3 = self.reshape_fixed(r, &[nh, hs, 1]);
        let y = self.matmul(s2, r3, DType::I64);
        let y = self.narrow_codes(y, &cfg.y);
        self.reshape_fixed(y, &[nh, hs])
    }
}
