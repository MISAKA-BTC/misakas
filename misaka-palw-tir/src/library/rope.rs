//! **Rotary and positional terms.**
//!
//! * Pairings: adjacent (`interleaved`, GPT-J/Cohere/Llama-in-GGUF — [`BlockBuilder::rope_pairs`])
//!   and half-split (`rotate_half`, HF Llama — [`BlockBuilder::rope_half`]); partial rotary
//!   (Phi, NeoX, the hybrid) rotates the first lanes of each head and passes the rest
//!   ([`BlockBuilder::rope_partial`]).
//! * Angles: one per-position table is `history_bound × rotary/2` rows; the long-context form uses
//!   two tables of `2^b` rows and angle addition (spec 04b §11.3, [`BlockBuilder::rope_angles_two_level`]).
//!   YaRN, "llama3", linear and NTK-by-parts scalings only change the frequencies — table data.
//! * Position-dependent frequency SETS (LongRoPE's short/long factors; a bucketed dynamic NTK):
//!   the canonical semantics is per-position decode (RFC-0002 Gate 1 decision 4) — the set in force
//!   at absolute position `pos` is a function of `pos` alone, chosen by `Select` on position
//!   thresholds ([`BlockBuilder::rope_angles_by_position`]).
//! * ALiBi: `slope_h · j` over the window (`Iota` over `H`) — [`BlockBuilder::alibi`].

use crate::arith::{K, ONE};
use crate::builder::BlockBuilder;
use crate::prim::{Cmp, Rounding};
use crate::program::Ref;
use crate::types::{DType, Dim};

/// How the rotated lanes pair up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RopeStyle {
    /// `(2p, 2p + 1)` — GPT-J, Cohere, GGUF's Llama layout.
    Interleaved,
    /// `(p, p + d/2)` — HF's `rotate_half`.
    Half,
}

/// One frequency set of [`BlockBuilder::rope_angles_by_position`]: the first absolute position it
/// applies at, and its two-level tables `(cos_hi, sin_hi, cos_lo, sin_lo)` over `2^lo_bits` rows.
#[derive(Clone, Copy, Debug)]
pub struct AngleSet {
    pub from_position: u32,
    pub cos_hi: Ref,
    pub sin_hi: Ref,
    pub cos_lo: Ref,
    pub sin_lo: Ref,
    pub lo_bits: u32,
}

impl BlockBuilder<'_> {
    /// **`rotate_half`** on the last axis `d` (pairs `(p, p + d/2)`): `a = x[..d/2]`,
    /// `b = x[d/2..]`, `out = [a·c − b·s, a·s + b·c] >> 24` (floor), clamped to `[lo, hi]`. `cos`
    /// and `sin` are Q24 rows of `d/2`, broadcast over the leading axes. Sums in `i128`, so any
    /// `i32` row against any `i32` table is exact.
    pub fn rope_half(&mut self, x: Ref, cos: Ref, sin: Ref, lo: i64, hi: i64, dtype: DType) -> Ref {
        let s = self.shape(x);
        let r = s.len();
        let Dim::Fixed(d) = s[r - 1] else { panic!("rope over H") };
        let a = self.slice(x, r - 1, 0, d / 2);
        let b = self.slice(x, r - 1, d / 2, d / 2);
        let ac = self.mul(a, cos, DType::I64);
        let bs = self.mul(b, sin, DType::I64);
        let as_ = self.mul(a, sin, DType::I64);
        let bc = self.mul(b, cos, DType::I64);
        let re = self.sub(ac, bs, DType::I128);
        let im = self.add(as_, bc, DType::I128);
        let re = self.shr(re, K, Rounding::Floor, DType::I128);
        let im = self.shr(im, K, Rounding::Floor, DType::I128);
        let re = self.clamp(re, lo, hi, dtype);
        let im = self.clamp(im, lo, hi, dtype);
        self.concat(&[re, im], r - 1)
    }

    /// **Partial rotary**: of each head's `head_dim` lanes (the last axis of `x`, `[heads, head_dim]`)
    /// the first `rotary` rotate in `style` pairs and are clamped to `[lo, hi]`; the rest pass
    /// through. `cos`/`sin` have `rotary/2` entries. The output has `x`'s dtype.
    #[allow(clippy::too_many_arguments)]
    pub fn rope_partial(&mut self, x: Ref, rotary: u32, cos: Ref, sin: Ref, style: RopeStyle, lo: i64, hi: i64) -> Ref {
        let t = self.ty(x);
        let r = t.shape.len();
        let Dim::Fixed(hd) = t.shape[r - 1] else { panic!("rope over H") };
        let part = if rotary == hd { x } else { self.slice(x, r - 1, 0, rotary) };
        let rot = match style {
            RopeStyle::Interleaved => self.rope_pairs_wide(part, cos, sin, lo, hi, t.dtype),
            RopeStyle::Half => self.rope_half(part, cos, sin, lo, hi, t.dtype),
        };
        if rotary == hd {
            return rot;
        }
        let pass = self.slice(x, r - 1, rotary, hd - rotary);
        self.concat(&[rot, pass], r - 1)
    }

    /// RoPE angles for position `pos` from TWO pinned tables of `2^lo_bits` rows each — the
    /// long-context strategy (spec 04b §11.3): `pos = hi·2^lo_bits + lo`, and the angle-addition
    /// formulas combine the rows in Q24 (floor). A `history_bound` of `2^18` needs two 512-row
    /// tables instead of one 262,144-row table. Returns `(cos, sin)`, `i32` Q24.
    pub fn rope_angles_two_level(&mut self, pos: Ref, cos_hi: Ref, sin_hi: Ref, cos_lo: Ref, sin_lo: Ref, lo_bits: u32) -> (Ref, Ref) {
        let d = self.c(DType::I64, 1i128 << lo_bits);
        let hi = self.div(pos, d, Rounding::Floor, DType::Idx);
        let back = self.mul(hi, d, DType::I64);
        // `pos − hi·2^b` is `pos mod 2^b`, in `[0, 2^b)`; interval analysis cannot see the
        // correlation, so the (never active) clamp states the range it proves.
        let lo = self.sub(pos, back, DType::I64);
        let lo = self.clamp(lo, 0, (1i64 << lo_bits) - 1, DType::Idx);
        let ch = self.gather(cos_hi, hi, 0, 0);
        let sh = self.gather(sin_hi, hi, 0, 0);
        let cl = self.gather(cos_lo, lo, 0, 0);
        let sl = self.gather(sin_lo, lo, 0, 0);
        // Each product fits i64 (|i32·i32| ≤ 2^62); their sum may not (2^63), so it is i128 —
        // the tables are params and take the full i32 range (spec 04b §7).
        let a = self.mul(ch, cl, DType::I64);
        let b = self.mul(sh, sl, DType::I64);
        let c = self.sub(a, b, DType::I128);
        let c = self.shr(c, K, Rounding::Floor, DType::I64);
        let c = self.clamp(c, -(ONE as i64), ONE as i64, DType::I32);
        let e = self.mul(sh, cl, DType::I64);
        let f = self.mul(ch, sl, DType::I64);
        let s = self.add(e, f, DType::I128);
        let s = self.shr(s, K, Rounding::Floor, DType::I64);
        let s = self.clamp(s, -(ONE as i64), ONE as i64, DType::I32);
        (c, s)
    }

    /// **Position-dependent frequency sets**: the angles of the LAST set whose `from_position` is
    /// at most `pos`. `sets[0].from_position` must be 0 and the list ascending. LongRoPE is two
    /// sets (short factors below `original_max_position_embeddings`, long ones from it on — HF's
    /// switch on `seq_len > original_max`, read per position); a dynamic-NTK lowering is one set
    /// per position bucket, and its bucketing is a recorded fidelity choice. Every set costs its
    /// own angle computation; the selection is exact.
    pub fn rope_angles_by_position(&mut self, pos: Ref, sets: &[AngleSet]) -> (Ref, Ref) {
        assert!(!sets.is_empty() && sets[0].from_position == 0, "the first set starts at position 0");
        assert!(sets.windows(2).all(|w| w[0].from_position < w[1].from_position), "ascending thresholds");
        let s0 = sets[0];
        let (mut cos, mut sin) = self.rope_angles_two_level(pos, s0.cos_hi, s0.sin_hi, s0.cos_lo, s0.sin_lo, s0.lo_bits);
        for s in &sets[1..] {
            let (c, si) = self.rope_angles_two_level(pos, s.cos_hi, s.sin_hi, s.cos_lo, s.sin_lo, s.lo_bits);
            let t = self.c(DType::Idx, s.from_position as i128);
            let on = self.compare(pos, t, Cmp::Ge);
            cos = self.select(on, c, cos, DType::I32);
            sin = self.select(on, si, sin, DType::I32);
        }
        (cos, sin)
    }

    /// **ALiBi**: `scores + slope_h · j` with `j` the key's index in the window (`Iota` over `H`).
    /// HF adds `slope·key_position` (or `slope·(key − query)`); over one query row those differ by
    /// a constant, which the softmax's maximum subtraction removes EXACTLY in integers, so the
    /// window index is the same function. `scores` has `H` last; `slopes` broadcasts against its
    /// leading axes with a last axis of 1, at the scores' scale. `i64` out.
    pub fn alibi(&mut self, scores: Ref, slopes: Ref) -> Ref {
        let s = self.shape(scores);
        let r = s.len();
        assert!(s[r - 1].is_h(), "ALiBi reads the window index");
        let mut iota_shape = vec![Dim::Fixed(1); r];
        iota_shape[r - 1] = Dim::H;
        let j = self.iota(DType::I32, &iota_shape, r - 1, 0, 1);
        let bias = self.mul(slopes, j, DType::I64);
        self.add(scores, bias, DType::I64)
    }
}
