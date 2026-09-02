//! ADR-0078 Decision 3: "no `f32`/`f64` on any path that reaches the output". Several artifact
//! formats in the kind table (glTF, STL) STORE IEEE-754 binary32 values, so the output path
//! needs a way to write one without ever computing in one. This module builds the bit pattern
//! of a binary32 from an integer and a binary scale with integer arithmetic only, and refuses
//! any value the format could not hold exactly — a refusal, never a rounding, because a rounded
//! value is a value two honest hosts might round differently.
//!
//! A fixed-point quantity is `mantissa / 2^frac_bits`. Every DSL in this crate expresses
//! geometry that way: integers in the JSON, a `frac_bits` the grammar fixes.

use crate::DeriveError;

/// The exact binary32 bit pattern of `value / 2^frac_bits`, or an error if that number is not
/// exactly representable as a normal binary32 (or zero). Positive zero for `value == 0`.
pub fn f32_bits_exact(value: i64, frac_bits: u32) -> Result<u32, DeriveError> {
    if value == 0 {
        return Ok(0);
    }
    let sign: u32 = if value < 0 { 1 << 31 } else { 0 };
    let mag: u64 = value.unsigned_abs();
    let tz = mag.trailing_zeros();
    let msb = 63 - mag.leading_zeros(); // position of the top set bit
    let significant_bits = msb - tz + 1;
    if significant_bits > 24 {
        return Err(DeriveError::Inexact(format!(
            "{value}/2^{frac_bits} needs {significant_bits} significant bits; binary32 holds 24"
        )));
    }
    // value = mag · 2^-frac_bits = 1.xxx · 2^(msb - frac_bits)
    let exp: i64 = msb as i64 - frac_bits as i64;
    let biased = exp + 127;
    if !(1..=254).contains(&biased) {
        return Err(DeriveError::Inexact(format!("{value}/2^{frac_bits} exponent {exp} is outside the normal binary32 range")));
    }
    // mantissa field: the 23 bits below the leading one
    let mant: u64 = if msb >= 23 { (mag >> (msb - 23)) & 0x7F_FFFF } else { (mag << (23 - msb)) & 0x7F_FFFF };
    Ok(sign | ((biased as u32) << 23) | (mant as u32))
}

/// Little-endian bytes of [`f32_bits_exact`], for writers.
pub fn f32_le_exact(value: i64, frac_bits: u32) -> Result<[u8; 4], DeriveError> {
    Ok(f32_bits_exact(value, frac_bits)?.to_le_bytes())
}

/// Integer square root (floor), for magnitudes that a kind may need to compare exactly.
pub fn isqrt_u64(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = 1u64 << ((64 - n.leading_zeros()).div_ceil(2));
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_hardware_conversion_where_exact() {
        for &(v, fb) in
            &[(1i64, 0u32), (-1, 0), (3, 1), (5, 2), (-7, 3), (1024, 10), (16_777_215, 0), (1, 16), (-12345, 16), (123, 24)]
        {
            let expected = ((v as f64) / f64::from(1u32 << fb)) as f32; // test-only: the oracle
            assert_eq!(f32_bits_exact(v, fb).unwrap(), expected.to_bits(), "{v}/2^{fb}");
        }
        assert_eq!(f32_bits_exact(0, 5).unwrap(), 0);
    }

    #[test]
    fn refuses_what_binary32_cannot_hold() {
        assert!(f32_bits_exact(16_777_217, 0).is_err()); // 2^24 + 1
        assert!(f32_bits_exact(1, 150).is_err()); // subnormal
        assert!(f32_bits_exact(i64::MAX, 0).is_err());
        assert!(f32_bits_exact(1 << 40, 0).is_ok()); // one significant bit, large exponent
    }

    #[test]
    fn isqrt() {
        for n in [0u64, 1, 2, 3, 4, 15, 16, 17, 99, 100, 1 << 40, u64::MAX] {
            let r = isqrt_u64(n);
            assert!(r.checked_mul(r).map(|sq| sq <= n).unwrap_or(false) || n == 0);
            assert!((r + 1).checked_mul(r + 1).map(|sq| sq > n).unwrap_or(true));
        }
    }
}
