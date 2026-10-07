//! **GF(p), p = 2^127 − 1** — the field every probabilistic relation of the K2 reference kernel is
//! checked over.
//!
//! One Mersenne prime of 127 bits, reduced with shifts and adds (`2^127 ≡ 1`, `2^128 ≡ 2`). It is wide
//! enough that every integer error a TIR v1 `MatMul` up to an `i64` accumulator can make stays nonzero
//! in the field (the alias bound is checked per relation in [`crate::check`]), so one field serves
//! every admitted relation and a uniform challenge passes a false product with probability `1/p`.
//! An `i128` accumulator can exceed it and is refused as `KERNEL_EXTENSION_REQUIRED`, not approximated.

/// `2^127 − 1`.
pub const P: u128 = (1u128 << 127) - 1;

/// log2 of the field size, floored: what one uniform challenge buys against a fixed nonzero error.
pub const FIELD_BITS: u32 = 126;

/// A canonical element, `< P`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fp(u128);

#[inline]
fn fold(v: u128) -> u128 {
    // v < 2^128: v = hi·2^127 + lo ≡ hi + lo.
    let r = (v & P) + (v >> 127);
    if r >= P { r - P } else { r }
}

impl Fp {
    pub const ZERO: Fp = Fp(0);
    pub const ONE: Fp = Fp(1);

    /// Reduce any `u128`.
    pub fn new(v: u128) -> Fp {
        Fp(fold(fold(v)))
    }

    /// A canonical representative, refused when `≥ P` (challenge sampling rejects it).
    pub fn from_canonical(v: u128) -> Option<Fp> {
        (v < P).then_some(Fp(v))
    }

    /// The image of a mathematical integer.
    pub fn from_i128(v: i128) -> Fp {
        let m = Fp::new(v.unsigned_abs());
        if v < 0 { m.fneg() } else { m }
    }

    pub fn value(self) -> u128 {
        self.0
    }

    pub fn fadd(self, o: Fp) -> Fp {
        Fp(fold(self.0 + o.0))
    }

    pub fn fneg(self) -> Fp {
        if self.0 == 0 { self } else { Fp(P - self.0) }
    }

    pub fn fsub(self, o: Fp) -> Fp {
        self.fadd(o.fneg())
    }

    pub fn fmul(self, o: Fp) -> Fp {
        let (a1, a0) = (self.0 >> 64, self.0 & u64::MAX as u128);
        let (b1, b0) = (o.0 >> 64, o.0 & u64::MAX as u128);
        // a·b = hi·2^128 + mid·2^64 + lo, with a1, b1 < 2^63.
        let lo = a0 * b0;
        let mid = a0 * b1 + a1 * b0; // < 2^128
        let hi = a1 * b1; // < 2^126
        let (m1, m0) = (mid >> 64, mid & u64::MAX as u128);
        // 2^128 ≡ 2: hi·2^128 ≡ 2·hi; mid·2^64 = m1·2^128 + m0·2^64 ≡ 2·m1 + m0·2^64.
        Fp::new(lo).fadd(Fp::new(m0 << 64)).fadd(Fp::new(2 * m1)).fadd(Fp::new(2 * hi))
    }

    /// `Σ a_i · b_i`.
    pub fn dot(a: impl IntoIterator<Item = Fp>, b: impl IntoIterator<Item = Fp>) -> Fp {
        a.into_iter().zip(b).fold(Fp::ZERO, |acc, (x, y)| acc.fadd(x.fmul(y)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive_mul(a: u128, b: u128) -> u128 {
        // Double-and-add over the field: slow and obviously correct.
        let (mut acc, mut x, mut k) = (0u128, a % P, b);
        while k > 0 {
            if k & 1 == 1 {
                acc = (acc + x) % P;
            }
            x = (x + x) % P;
            k >>= 1;
        }
        acc
    }

    #[test]
    fn multiplication_agrees_with_double_and_add() {
        let samples =
            [0u128, 1, 2, 3, P - 1, P - 2, 1 << 64, (1 << 64) - 1, (1 << 126) + 12345, 0xDEAD_BEEF_0123_4567_89AB_CDEF_0011_2233];
        for &a in &samples {
            for &b in &samples {
                assert_eq!(Fp::new(a).fmul(Fp::new(b)).value(), naive_mul(a % P, b % P), "{a} · {b}");
            }
        }
    }

    #[test]
    fn integers_map_with_sign_and_round_trip_small_values() {
        assert_eq!(Fp::from_i128(-1).value(), P - 1);
        assert_eq!(Fp::from_i128(-5).fadd(Fp::from_i128(5)), Fp::ZERO);
        assert_eq!(Fp::from_i128(i128::MIN), Fp::from_i128(-1), "2^127 ≡ 1, so −2^127 ≡ −1");
        assert_eq!(Fp::from_i128(i128::MIN).fadd(Fp::ONE), Fp::ZERO);
        assert_eq!(Fp::from_i128(7).fsub(Fp::from_i128(9)), Fp::from_i128(-2));
        assert_eq!(Fp::new(P), Fp::ZERO);
        assert_eq!(Fp::new(u128::MAX), Fp::new(1), "2^128 − 1 ≡ 2 − 1");
    }
}
