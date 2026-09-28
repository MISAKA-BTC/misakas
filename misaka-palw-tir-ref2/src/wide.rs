//! Exact integers wider than any dtype.
//!
//! 04b §0: "All arithmetic in this chapter is on mathematical integers ℤ." Rather than reason about
//! which machine type is wide enough for each intermediate, every exact primitive of this crate
//! computes its mathematical result in [`Wide`] (sign and a 256-bit magnitude) and only then asks
//! whether it is a value of the output dtype. The largest intermediate anywhere is an `i128 · i128`
//! product (`2^254`); sums of at most `2^24` terms of `2^127` stay below `2^152`. An operation whose
//! result would not fit 256 bits returns `None`, which every caller treats as "outside every dtype".

use std::cmp::Ordering;

/// Little-endian 64-bit limbs of a magnitude `< 2^256`.
type Mag = [u64; 4];

const MAG_ZERO: Mag = [0, 0, 0, 0];

fn mag_from_u128(v: u128) -> Mag {
    [v as u64, (v >> 64) as u64, 0, 0]
}

fn mag_is_zero(a: &Mag) -> bool {
    a.iter().all(|&l| l == 0)
}

fn mag_cmp(a: &Mag, b: &Mag) -> Ordering {
    for i in (0..4).rev() {
        match a[i].cmp(&b[i]) {
            Ordering::Equal => continue,
            o => return o,
        }
    }
    Ordering::Equal
}

fn mag_add(a: &Mag, b: &Mag) -> Option<Mag> {
    let mut out = MAG_ZERO;
    let mut carry = 0u128;
    for i in 0..4 {
        let s = a[i] as u128 + b[i] as u128 + carry;
        out[i] = s as u64;
        carry = s >> 64;
    }
    if carry != 0 { None } else { Some(out) }
}

/// `a − b` for `a ≥ b`.
fn mag_sub(a: &Mag, b: &Mag) -> Mag {
    debug_assert!(mag_cmp(a, b) != Ordering::Less);
    let mut out = MAG_ZERO;
    let mut borrow = 0i128;
    for i in 0..4 {
        let mut d = a[i] as i128 - b[i] as i128 - borrow;
        if d < 0 {
            d += 1i128 << 64;
            borrow = 1;
        } else {
            borrow = 0;
        }
        out[i] = d as u64;
    }
    out
}

/// Schoolbook product of two 128-bit magnitudes (at most 256 bits, so it always fits).
fn mag_mul_u128(a: u128, b: u128) -> Mag {
    let al = [a as u64, (a >> 64) as u64];
    let bl = [b as u64, (b >> 64) as u64];
    let mut acc = [0u128; 5];
    for (i, &x) in al.iter().enumerate() {
        for (j, &y) in bl.iter().enumerate() {
            let p = x as u128 * y as u128;
            acc[i + j] += p & 0xFFFF_FFFF_FFFF_FFFF;
            acc[i + j + 1] += p >> 64;
        }
    }
    // Normalise the column sums (each < 2^66) into limbs.
    let mut out = MAG_ZERO;
    let mut carry = 0u128;
    for k in 0..4 {
        let s = acc[k] + carry;
        out[k] = s as u64;
        carry = s >> 64;
    }
    debug_assert!(carry + acc[4] == 0);
    out
}

/// A signed integer with a 256-bit magnitude. Zero is never negative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wide {
    neg: bool,
    mag: Mag,
}

impl Wide {
    pub const ZERO: Wide = Wide { neg: false, mag: MAG_ZERO };

    fn norm(neg: bool, mag: Mag) -> Wide {
        Wide { neg: neg && !mag_is_zero(&mag), mag }
    }

    pub fn from_i128(v: i128) -> Wide {
        Wide::norm(v < 0, mag_from_u128(v.unsigned_abs()))
    }

    pub fn from_u128(v: u128) -> Wide {
        Wide::norm(false, mag_from_u128(v))
    }

    pub fn is_negative(&self) -> bool {
        self.neg
    }

    pub fn is_positive(&self) -> bool {
        !self.neg && !mag_is_zero(&self.mag)
    }

    pub fn negate(self) -> Wide {
        Wide::norm(!self.neg, self.mag)
    }

    pub fn add(self, o: Wide) -> Option<Wide> {
        if self.neg == o.neg {
            return Some(Wide::norm(self.neg, mag_add(&self.mag, &o.mag)?));
        }
        // Opposite signs: subtract the smaller magnitude from the larger; the sign is the larger's.
        match mag_cmp(&self.mag, &o.mag) {
            Ordering::Less => Some(Wide::norm(o.neg, mag_sub(&o.mag, &self.mag))),
            _ => Some(Wide::norm(self.neg, mag_sub(&self.mag, &o.mag))),
        }
    }

    pub fn sub(self, o: Wide) -> Option<Wide> {
        self.add(o.negate())
    }

    /// The exact product of two `i128` values (always representable).
    pub fn mul_i128(a: i128, b: i128) -> Wide {
        Wide::norm((a < 0) != (b < 0), mag_mul_u128(a.unsigned_abs(), b.unsigned_abs()))
    }

    pub fn cmp_wide(&self, o: &Wide) -> Ordering {
        match (self.neg, o.neg) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => mag_cmp(&self.mag, &o.mag),
            (true, true) => mag_cmp(&o.mag, &self.mag),
        }
    }

    /// The value as an `i128`, or `None` when it is outside `[−2^127, 2^127 − 1]`.
    pub fn to_i128(&self) -> Option<i128> {
        if self.mag[2] != 0 || self.mag[3] != 0 {
            return None;
        }
        let m = (self.mag[0] as u128) | ((self.mag[1] as u128) << 64);
        if self.neg {
            // −m for m ≤ 2^127.
            if m <= 1u128 << 127 { Some(0i128.wrapping_sub_unsigned(m)) } else { None }
        } else if m < 1u128 << 127 {
            Some(m as i128)
        } else {
            None
        }
    }

    /// `lo ≤ self ≤ hi`.
    pub fn within(&self, lo: i128, hi: i128) -> bool {
        self.cmp_wide(&Wide::from_i128(lo)) != Ordering::Less && self.cmp_wide(&Wide::from_i128(hi)) != Ordering::Greater
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_at_the_extremes() {
        let p = Wide::mul_i128(i128::MIN, i128::MIN); // 2^254
        assert!(p.is_positive());
        assert_eq!(p.mag, [0, 0, 0, 1u64 << 62]);
        let q = Wide::mul_i128(i128::MIN, 1);
        assert_eq!(q.to_i128(), Some(i128::MIN));
        let r = Wide::mul_i128(i128::MIN, -1);
        assert_eq!(r.to_i128(), None);
        assert_eq!(Wide::mul_i128(i128::MAX, i128::MAX).sub(Wide::mul_i128(i128::MAX, i128::MAX)), Some(Wide::ZERO));
        assert_eq!(Wide::mul_i128(-3, 7).to_i128(), Some(-21));
        assert_eq!(Wide::mul_i128(0, -7), Wide::ZERO);
        assert!(!Wide::mul_i128(0, -7).is_negative());
    }

    #[test]
    fn sums_cross_zero() {
        let a = Wide::from_i128(i128::MAX);
        let b = Wide::from_i128(i128::MIN);
        assert_eq!(a.add(b).and_then(|w| w.to_i128()), Some(-1));
        let c = a.add(Wide::from_i128(1)).unwrap();
        assert_eq!(c.to_i128(), None);
        assert_eq!(c.sub(Wide::from_i128(1)).and_then(|w| w.to_i128()), Some(i128::MAX));
        assert!(Wide::from_i128(-5).within(-5, -5));
        assert!(!Wide::from_i128(-6).within(-5, 10));
        assert!(Wide::from_u128(u128::MAX).cmp_wide(&Wide::from_i128(i128::MAX)) == Ordering::Greater);
    }

    #[test]
    fn random_products_agree_with_i128_where_they_fit() {
        let mut s = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        for _ in 0..20000 {
            let a = (next() as i64) as i128 >> (next() % 64);
            let b = (next() as i64) as i128 >> (next() % 64);
            let w = Wide::mul_i128(a, b);
            assert_eq!(w.to_i128(), a.checked_mul(b));
            let c = (next() as i128) << 64 | next() as i128;
            let d = (next() as i128) << 64 | next() as i128;
            assert_eq!(Wide::from_i128(c).add(Wide::from_i128(d)).unwrap().to_i128(), c.checked_add(d));
            assert_eq!(Wide::from_i128(c).sub(Wide::from_i128(d)).unwrap().to_i128(), c.checked_sub(d));
        }
    }
}
