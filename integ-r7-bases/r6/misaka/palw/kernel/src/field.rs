//! **Mersenne prime fields GF(2^e − 1)** — the fields every probabilistic relation of the K2 reference kernels is checked over.
//!
//! [`Fp`] = GF(2^127 − 1) is K2-TIR-v1's one field. It is wide enough that every integer error a TIR v1 `MatMul` up to an `i64`
//! accumulator can make stays nonzero in it (the alias bound is checked per relation in [`crate::check`]), so a uniform challenge
//! passes a false product with probability `1/p`.
//!
//! An `i128` accumulator can exceed `2^127 − 1`. K2-TIR-v1 refuses it as `KERNEL_EXTENSION_REQUIRED`; K2-TIR-v2's multi-modulus
//! dense relation ([`MODULI_V2`]) checks the same product modulo up to three Mersenne primes (`2^127 − 1`, `2^107 − 1`, `2^89 − 1`,
//! product ≈ `2^323`): a nonzero integer error below the product is nonzero modulo at least one of them (CRT), and that modulus's
//! check catches it with probability `≥ 1 − 1/p`. Nothing is approximated; a span at or above the product is still refused.
//!
//! Reduction uses `2^e ≡ 1`: a 256-bit product is folded in `e`-bit chunks.

/// `2^127 − 1`.
pub const P: u128 = (1u128 << 127) - 1;

/// log2 of `2^127 − 1`, floored: what one uniform challenge buys against a fixed nonzero error.
pub const FIELD_BITS: u32 = 126;

/// The Mersenne exponents of K2-TIR-v2's multi-modulus relation, largest first (fewest moduli for a given span).
pub const MODULI_V2: [u32; 3] = [127, 107, 89];

/// A canonical element of GF(2^E − 1), `< 2^E − 1`. `E` is a Mersenne-prime exponent `≤ 127`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mersenne<const E: u32>(u128);

/// GF(2^127 − 1).
pub type Fp = Mersenne<127>;
pub type F107 = Mersenne<107>;
pub type F89 = Mersenne<89>;

/// The prime `2^e − 1`.
pub const fn mersenne(e: u32) -> u128 {
    if e >= 128 { u128::MAX } else { (1u128 << e) - 1 }
}

/// log2 of `2^e − 1`, floored.
pub const fn mersenne_bits(e: u32) -> u32 {
    e - 1
}

/// `a · b` as `(hi, lo)`, the exact 256-bit product.
#[inline]
fn mul_wide(a: u128, b: u128) -> (u128, u128) {
    let m = u64::MAX as u128;
    let (a1, a0, b1, b0) = (a >> 64, a & m, b >> 64, b & m);
    let ll = a0 * b0;
    let lh = a0 * b1;
    let hl = a1 * b0;
    let hh = a1 * b1;
    // mid = lh + hl + (ll >> 64), kept as (carry, value).
    let (mid, c1) = lh.overflowing_add(hl);
    let (mid, c2) = mid.overflowing_add(ll >> 64);
    let lo = (mid << 64) | (ll & m);
    let hi = hh + (mid >> 64) + (((c1 as u128) + (c2 as u128)) << 64);
    (hi, lo)
}

/// `(hi·2^128 + lo) mod (2^e − 1)`, canonical.
#[inline]
fn reduce_wide(e: u32, mut hi: u128, mut lo: u128) -> u128 {
    let p = mersenne(e);
    // Fold while anything sits above bit e: N = (N mod 2^e) + (N >> e).
    while hi != 0 || lo > p {
        let low = lo & p;
        let (shi, slo) = if e == 128 { (0, hi) } else { (hi >> e, (lo >> e) | (hi << (128 - e))) };
        let (s, c) = slo.overflowing_add(low);
        hi = shi + c as u128;
        lo = s;
    }
    if lo == p { 0 } else { lo }
}

impl<const E: u32> Mersenne<E> {
    pub const ZERO: Self = Mersenne(0);
    pub const ONE: Self = Mersenne(1);
    /// The modulus.
    pub const MODULUS: u128 = mersenne(E);

    /// Reduce any `u128`.
    pub fn new(v: u128) -> Self {
        Mersenne(reduce_wide(E, 0, v))
    }

    /// A canonical representative, refused when `≥ p` (challenge sampling rejects it).
    pub fn from_canonical(v: u128) -> Option<Self> {
        (v < Self::MODULUS).then_some(Mersenne(v))
    }

    /// The image of a mathematical integer.
    pub fn from_i128(v: i128) -> Self {
        let m = Self::new(v.unsigned_abs());
        if v < 0 { m.fneg() } else { m }
    }

    pub fn value(self) -> u128 {
        self.0
    }

    pub fn fadd(self, o: Self) -> Self {
        // Both < 2^127: the sum fits.
        Mersenne(reduce_wide(E, 0, self.0 + o.0))
    }

    pub fn fneg(self) -> Self {
        if self.0 == 0 { self } else { Mersenne(Self::MODULUS - self.0) }
    }

    pub fn fsub(self, o: Self) -> Self {
        self.fadd(o.fneg())
    }

    pub fn fmul(self, o: Self) -> Self {
        let (hi, lo) = mul_wide(self.0, o.0);
        Mersenne(reduce_wide(E, hi, lo))
    }

    /// `Σ a_i · b_i`.
    pub fn dot(a: impl IntoIterator<Item = Self>, b: impl IntoIterator<Item = Self>) -> Self {
        a.into_iter().zip(b).fold(Self::ZERO, |acc, (x, y)| acc.fadd(x.fmul(y)))
    }
}

/// The operations the Freivalds checks are written over, so one routine serves every modulus.
pub trait FieldElemV1: Copy + Eq + std::fmt::Debug {
    /// The Mersenne exponent.
    const EXP: u32;
    fn zero() -> Self;
    fn of_i128(v: i128) -> Self;
    fn add(self, o: Self) -> Self;
    fn mul(self, o: Self) -> Self;
    /// A sampled 128-bit word, masked to `EXP` bits; `None` for the one rejected value (`p`).
    fn of_word(w: u128) -> Option<Self>;
    fn raw(self) -> u128;
    fn of_raw(v: u128) -> Self;
    fn dot_iter(a: impl IntoIterator<Item = Self>, b: impl IntoIterator<Item = Self>) -> Self {
        a.into_iter().zip(b).fold(Self::zero(), |acc, (x, y)| acc.add(x.mul(y)))
    }
}

impl<const E: u32> FieldElemV1 for Mersenne<E> {
    const EXP: u32 = E;
    fn zero() -> Self {
        Self::ZERO
    }
    fn of_i128(v: i128) -> Self {
        Self::from_i128(v)
    }
    fn add(self, o: Self) -> Self {
        self.fadd(o)
    }
    fn mul(self, o: Self) -> Self {
        self.fmul(o)
    }
    fn of_word(w: u128) -> Option<Self> {
        Self::from_canonical(w & Self::MODULUS)
    }
    fn raw(self) -> u128 {
        self.0
    }
    fn of_raw(v: u128) -> Self {
        Mersenne(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive_mul(a: u128, b: u128, p: u128) -> u128 {
        // Double-and-add over the field: slow and obviously correct (p < 2^127, so sums fit).
        let (mut acc, mut x, mut k) = (0u128, a % p, b);
        while k > 0 {
            if k & 1 == 1 {
                acc = (acc + x) % p;
            }
            x = (x + x) % p;
            k >>= 1;
        }
        acc
    }

    fn agrees<const E: u32>() {
        let p = mersenne(E);
        let samples = [
            0u128,
            1,
            2,
            3,
            p - 1,
            p - 2,
            p >> 1,
            1 << 64,
            (1 << 64) - 1,
            (1 << (E - 1)) + 12345,
            0xDEAD_BEEF_0123_4567_89AB_CDEF_0011_2233 & p,
        ];
        for &a in &samples {
            for &b in &samples {
                let got = Mersenne::<E>::new(a).fmul(Mersenne::<E>::new(b)).value();
                assert_eq!(got, naive_mul(a % p, b % p, p), "2^{E}−1: {a} · {b}");
            }
        }
    }

    #[test]
    fn multiplication_agrees_with_double_and_add_for_every_modulus() {
        agrees::<127>();
        agrees::<107>();
        agrees::<89>();
        agrees::<61>();
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
        assert_eq!(F89::new(1u128 << 89), F89::ONE, "2^89 ≡ 1");
        assert_eq!(F107::from_i128(i128::MIN).value(), F107::new(1u128 << 127).fneg().value());
        assert_eq!(F89::from_i128(-3).fadd(F89::from_i128(3)), F89::ZERO);
    }

    #[test]
    fn an_integer_error_below_the_moduli_product_survives_in_some_modulus() {
        // 2^127 − 1 itself vanishes in Fp but not in F107; (2^127 − 1)(2^107 − 1) vanishes in both but not in F89.
        let d = P as i128;
        assert_eq!(Fp::from_i128(d), Fp::ZERO);
        assert_ne!(F107::from_i128(d), F107::ZERO);
        let e = (P % F89::MODULUS) * (F107::MODULUS % F89::MODULUS);
        assert_ne!(F89::new(e), F89::ZERO);
    }
}
