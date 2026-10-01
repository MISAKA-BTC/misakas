//! **The fields a check runs over, and how a node chooses them** (RFC-0007 Part II, §II.5).
//!
//! A Freivalds check compares two random linear combinations modulo a prime `p`. It is sound for one
//! node exactly when every nonzero error the producer could have made stays nonzero modulo the
//! moduli the node is checked over. The served value and the true value both lie in the node's
//! proven interval `[lo, hi]` (the served one is checked against it before anything else), so an
//! error is an integer `e` with `0 < |e| ≤ hi − lo`. If the product of the node's moduli exceeds
//! `hi − lo`, `e` is nonzero modulo at least one of them (the Chinese remainder theorem), and that
//! modulus's check catches it with probability `1 − 1/p`. That is the whole rule
//! ([`tir_sketch_moduli_for_span_v1`]):
//!
//! | span `hi − lo` | moduli | why |
//! | --- | --- | --- |
//! | `< 2^61 − 1` | `P61` | every `i32` accumulator, and the `i64` ones a refined plan proves narrow |
//! | `< P61 · P64 ≈ 2^125` | `P61`, `P64` | an `i64` accumulator the plan cannot narrow |
//! | otherwise | `P61`, `P64`, `P63` | an `i128` node: the three primes' product is `≈ 2^188` |
//!
//! A node checked over one modulus too few is not merely weaker: an error of exactly `P61` passes
//! the `P61` check with certainty (`tests/soundness.rs` shows the hole and that the ladder closes it).
//!
//! `P61 = 2^61 − 1` is a Mersenne prime, reduced with shifts and adds; it carries every check of
//! the classes measured. `P64 = 2^64 − 59` and `P63 = 2^63 − 25` are the largest primes below their
//! powers of two and use the generic `u128` remainder — they run only on wide nodes.

/// A prime modulus. Every value of the field is a `u64` in `[0, p)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TirSketchModulusV1(u64);

const M61: u64 = (1u64 << 61) - 1;

impl TirSketchModulusV1 {
    /// `2^61 − 1`, the Mersenne prime every narrow node is checked over.
    pub const P61: Self = Self(M61);
    /// `2^64 − 59`, the second rung.
    pub const P64: Self = Self(u64::MAX - 58);
    /// `2^63 − 25`, the third rung.
    pub const P63: Self = Self((1u64 << 63) - 25);
    /// The ladder, in the order a node takes its moduli.
    pub const LADDER_V1: [Self; 3] = [Self::P61, Self::P64, Self::P63];

    /// **A small prime, for the statistical tests only** — a check over it passes a random tamper
    /// with probability `1/p`, which a test can observe; no node is ever checked over it.
    #[doc(hidden)]
    pub const fn toy(p: u64) -> Self {
        Self(p)
    }

    pub const fn p(self) -> u64 {
        self.0
    }

    /// The tag a derivation keys the modulus by (`255` for a toy prime).
    pub const fn tag(self) -> u8 {
        match self.0 {
            M61 => 0,
            0xFFFF_FFFF_FFFF_FFC5 => 1,
            0x7FFF_FFFF_FFFF_FFE7 => 2,
            _ => 255,
        }
    }

    /// `v mod p` for any `u128`.
    #[inline(always)]
    pub fn reduce_u128(self, v: u128) -> u64 {
        if self.0 == M61 {
            let m = M61 as u128;
            let x = (v & m) + (v >> 61); // < 2^61 + 2^67
            let x = (x & m) + (x >> 61); // < 2^61 + 2^7
            let mut r = x as u64;
            if r >= M61 {
                r -= M61;
            }
            r
        } else {
            (v % self.0 as u128) as u64
        }
    }

    /// `v mod p` in `[0, p)` for any `i128`.
    #[inline(always)]
    pub fn reduce_i128(self, v: i128) -> u64 {
        let r = self.reduce_u128(v.unsigned_abs());
        if v < 0 && r != 0 { self.0 - r } else { r }
    }

    #[inline(always)]
    pub fn mul(self, a: u64, b: u64) -> u64 {
        self.reduce_u128(a as u128 * b as u128)
    }

    #[inline(always)]
    pub fn add(self, a: u64, b: u64) -> u64 {
        let s = a as u128 + b as u128;
        if s >= self.0 as u128 { (s - self.0 as u128) as u64 } else { s as u64 }
    }

    #[inline(always)]
    pub fn sub(self, a: u64, b: u64) -> u64 {
        if a >= b { a - b } else { self.0 - (b - a) }
    }

    /// `Σ a[i]·b[i] mod p` over two reduced vectors of equal length.
    ///
    /// Over `P61` the products of two reduced values are below `2^122`, so 64 of them sum in a
    /// `u128` before one reduction — the inner loop is a multiply and an add.
    pub fn dot(self, a: &[u64], b: &[u64]) -> u64 {
        debug_assert_eq!(a.len(), b.len());
        if self.0 == M61 {
            let mut acc = 0u64;
            for (ca, cb) in a.chunks(64).zip(b.chunks(64)) {
                let mut s = 0u128;
                for (x, y) in ca.iter().zip(cb) {
                    s += *x as u128 * *y as u128;
                }
                acc = self.add(acc, self.reduce_u128(s));
            }
            acc
        } else {
            a.iter().zip(b).fold(0u64, |acc, (x, y)| self.add(acc, self.mul(*x, *y)))
        }
    }

    /// `Σ x[i]·s[i] mod p` with `x` raw signed integers of at most 64 bits (activations, served
    /// accumulators) and `s` reduced — the check's inner loop, without reducing `x` first.
    pub fn dot_i64(self, x: &[i64], s: &[u64]) -> u64 {
        self.dot_i64_bounded(x, s, 64)
    }

    /// [`Self::dot_i64`] for `x` whose every element satisfies `|x| < 2^x_bits` — what a node's
    /// proven interval says. Over `P61` each product is a signed `i128` below `2^(x_bits + 61)`, so
    /// `2^(126 − x_bits − 61)` of them sum without a reduction and without a branch: an `i16`
    /// activation sums 2^49 terms, a 36-bit accumulator 2^29, a full `i64` four.
    pub fn dot_i64_bounded(self, x: &[i64], s: &[u64], x_bits: u32) -> u64 {
        debug_assert_eq!(x.len(), s.len());
        if self.0 == M61 {
            let chunk = 1usize << (126 - x_bits.clamp(1, 64) - 61).min(30);
            let mut acc = 0u64;
            for (cx, cs) in x.chunks(chunk).zip(s.chunks(chunk)) {
                let sum: i128 = cx.iter().zip(cs).map(|(v, w)| *v as i128 * *w as i128).sum();
                acc = self.add(acc, self.reduce_i128(sum));
            }
            acc
        } else {
            x.iter().zip(s).fold(0u64, |acc, (v, w)| self.add(acc, self.mul(self.reduce_i128(*v as i128), *w)))
        }
    }
}

/// **The moduli a node whose values span `span = hi − lo` is checked over**: the fewest rungs of
/// [`TirSketchModulusV1::LADDER_V1`] whose product exceeds `span` (module note).
pub fn tir_sketch_moduli_for_span_v1(span: u128) -> Vec<TirSketchModulusV1> {
    let p61 = TirSketchModulusV1::P61.p() as u128;
    let p64 = TirSketchModulusV1::P64.p() as u128;
    if span < p61 {
        vec![TirSketchModulusV1::P61]
    } else if span < p61 * p64 {
        vec![TirSketchModulusV1::P61, TirSketchModulusV1::P64]
    } else {
        // P61·P64·P63 ≈ 2^188 exceeds every u128.
        TirSketchModulusV1::LADDER_V1.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic Miller–Rabin for `n < 3.3·10^24` (the first twelve primes as bases).
    fn is_prime(n: u64) -> bool {
        if n < 2 {
            return false;
        }
        let small = [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
        for p in small {
            if n.is_multiple_of(p) {
                return n == p;
            }
        }
        let (mut d, mut s) = (n - 1, 0);
        while d.is_multiple_of(2) {
            d /= 2;
            s += 1;
        }
        let m = TirSketchModulusV1::toy(n);
        let pow = |mut b: u64, mut e: u64| {
            let mut r = 1u64;
            while e > 0 {
                if e & 1 == 1 {
                    r = m.mul(r, b);
                }
                b = m.mul(b, b);
                e >>= 1;
            }
            r
        };
        'witness: for a in small {
            let mut x = pow(a % n, d);
            if x == 1 || x == n - 1 {
                continue;
            }
            for _ in 1..s {
                x = m.mul(x, x);
                if x == n - 1 {
                    continue 'witness;
                }
            }
            return false;
        }
        true
    }

    #[test]
    fn every_rung_of_the_ladder_is_prime_and_is_the_largest_below_its_power_of_two() {
        assert_eq!(TirSketchModulusV1::P61.p(), (1u64 << 61) - 1);
        assert_eq!(TirSketchModulusV1::P64.p() as u128, (1u128 << 64) - 59);
        assert_eq!(TirSketchModulusV1::P63.p(), (1u64 << 63) - 25);
        for m in TirSketchModulusV1::LADDER_V1 {
            assert!(is_prime(m.p()), "{} is prime", m.p());
        }
        // Nothing between each prime and its power of two is prime (2^61 − 1 is the power's own).
        for k in 1..59u64 {
            assert!(!is_prime(u64::MAX - k + 1), "2^64 − {k} is composite");
        }
        for k in 1..25u64 {
            assert!(!is_prime((1u64 << 63) - k), "2^63 − {k} is composite");
        }
        assert_eq!(TirSketchModulusV1::LADDER_V1.map(|m| m.tag()), [0, 1, 2]);
    }

    #[test]
    fn the_mersenne_reduction_agrees_with_the_remainder_on_every_edge() {
        let m = TirSketchModulusV1::P61;
        let p = m.p() as u128;
        for v in [0u128, 1, p - 1, p, p + 1, 2 * p, u64::MAX as u128, (1u128 << 122) + 12345, u128::MAX, u128::MAX - p] {
            assert_eq!(m.reduce_u128(v) as u128, v % p, "{v}");
        }
        for v in [0i128, -1, 1, i128::MIN, i128::MAX, -(p as i128), -(p as i128) - 1, i64::MIN as i128] {
            assert_eq!(m.reduce_i128(v) as i128, v.rem_euclid(p as i128), "{v}");
        }
    }

    #[test]
    fn the_dot_kernels_agree_with_exact_integer_arithmetic() {
        for m in [TirSketchModulusV1::P61, TirSketchModulusV1::P64, TirSketchModulusV1::P63, TirSketchModulusV1::toy(101)] {
            let p = m.p() as u128;
            let x: Vec<i64> = (0..200).map(|i| if i % 3 == 0 { i64::MIN + i } else { (i * 7_919_993) - 600_000_000 }).collect();
            let s: Vec<u64> = (0..200u64).map(|i| m.reduce_u128((i as u128) * 0x9E37_79B9_7F4A_7C15 + 3)).collect();
            let exact = x.iter().zip(&s).fold(0u128, |acc, (a, b)| {
                let a = (*a as i128).rem_euclid(p as i128) as u128;
                (acc + a * *b as u128 % p) % p
            });
            assert_eq!(m.dot_i64(&x, &s) as u128, exact);
            let xr: Vec<u64> = x.iter().map(|v| m.reduce_i128(*v as i128)).collect();
            assert_eq!(m.dot(&xr, &s) as u128, exact);
            // The bounded form over narrow values: one long chunk, the same sum.
            let narrow: Vec<i64> = (0..200).map(|i| ((i * 2_654_435_761u64 as i64) % 65_535) - 32_767).collect();
            let exact_narrow = narrow.iter().zip(&s).fold(0u128, |acc, (a, b)| {
                let a = (*a as i128).rem_euclid(p as i128) as u128;
                (acc + a * *b as u128 % p) % p
            });
            assert_eq!(m.dot_i64_bounded(&narrow, &s, 16) as u128, exact_narrow);
        }
    }

    #[test]
    fn a_node_takes_the_fewest_moduli_whose_product_exceeds_its_span() {
        let p61 = TirSketchModulusV1::P61.p() as u128;
        let p64 = TirSketchModulusV1::P64.p() as u128;
        assert_eq!(tir_sketch_moduli_for_span_v1(0), vec![TirSketchModulusV1::P61]);
        assert_eq!(tir_sketch_moduli_for_span_v1((1u128 << 32) - 1), vec![TirSketchModulusV1::P61], "every i32 accumulator");
        assert_eq!(tir_sketch_moduli_for_span_v1(p61 - 1), vec![TirSketchModulusV1::P61]);
        assert_eq!(tir_sketch_moduli_for_span_v1(p61).len(), 2, "an error of exactly P61 must not pass");
        assert_eq!(tir_sketch_moduli_for_span_v1(u64::MAX as u128).len(), 2, "the whole i64 range");
        assert_eq!(tir_sketch_moduli_for_span_v1(p61 * p64 - 1).len(), 2);
        assert_eq!(tir_sketch_moduli_for_span_v1(p61 * p64).len(), 3);
        assert_eq!(tir_sketch_moduli_for_span_v1(u128::MAX).len(), 3, "the whole i128 range");
    }
}
