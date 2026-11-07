use borsh::{BorshDeserialize, BorshSerialize};
use wasm_bindgen::JsValue;
use workflow_core::sendable::Sendable;

pub mod int;
pub mod uint;
pub mod wasm;

construct_uint!(Uint192, 3, BorshSerialize, BorshDeserialize);
construct_uint!(Uint256, 4);
construct_uint!(Uint320, 5);
// kaspa-pq Phase 8 (PR-8.2) wide integers for the Layered PoW.
//   Uint512  — PowTargetType / PowWorkType. 512-bit comparison domain.
//   Uint576  — BlueWorkType (Phase 1). One machine word wider than
//              the target so a 2^64 window of max-work blocks fits.
//   Uint640  — DAA internal accumulator. Another word above
//              BlueWorkType so DAA-window aggregation cannot
//              overflow in pathological edge cases.
// All three derive Borsh so they round-trip through wRPC and header
// storage symmetrically with Uint192. See ADR-0007.
construct_uint!(Uint512, 8, BorshSerialize, BorshDeserialize);
construct_uint!(Uint576, 9, BorshSerialize, BorshDeserialize);
construct_uint!(Uint640, 10, BorshSerialize, BorshDeserialize);
construct_uint!(Uint3072, 48);

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0:?}")]
    JsValue(Sendable<JsValue>),

    #[error("Invalid hex string: {0}")]
    Hex(#[from] faster_hex::Error),

    #[error(transparent)]
    TryFromSliceError(#[from] uint::TryFromSliceError),
    // TryFromSliceError(#[from] std::array::TryFromSliceError),
    #[error("Utf8 error: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error(transparent)]
    WorkflowWasm(#[from] workflow_wasm::error::Error),

    #[error(transparent)]
    SerdeWasmBindgen(#[from] serde_wasm_bindgen::Error),

    #[error("{0:?}")]
    JsSys(Sendable<js_sys::Error>),

    #[error("Supplied value is not compatible with this type")]
    NotCompatible,

    #[error("range error: {0:?}")]
    Range(Sendable<js_sys::RangeError>),
}

impl From<js_sys::Error> for Error {
    fn from(err: js_sys::Error) -> Self {
        Error::JsSys(Sendable(err))
    }
}

impl From<js_sys::RangeError> for Error {
    fn from(err: js_sys::RangeError) -> Self {
        Error::Range(Sendable(err))
    }
}

impl From<JsValue> for Error {
    fn from(err: JsValue) -> Self {
        Error::JsValue(Sendable(err))
    }
}

impl Uint256 {
    #[inline]
    pub fn from_compact_target_bits(bits: u32) -> Self {
        // This is a floating-point "compact" encoding originally used by
        // OpenSSL, which satoshi put into consensus code, so we're stuck
        // with it. The exponent needs to have 3 subtracted from it, hence
        // this goofy decoding code:
        let (mant, expt) = {
            let unshifted_expt = bits >> 24;
            if unshifted_expt <= 3 {
                ((bits & 0xFFFFFF) >> (8 * (3 - unshifted_expt)), 0)
            } else {
                (bits & 0xFFFFFF, 8 * ((bits >> 24) - 3))
            }
        };
        // The mantissa is signed but may not be negative
        if mant > 0x7FFFFF { Uint256::ZERO } else { Uint256::from_u64(u64::from(mant)) << expt }
    }

    #[inline]
    /// Computes the target value in float format from BigInt format.
    pub fn compact_target_bits(self) -> u32 {
        let mut size = self.bits().div_ceil(8);
        let mut compact = if size <= 3 {
            (self.as_u64() << (8 * (3 - size))) as u32
        } else {
            let bn = self >> (8 * (size - 3));
            bn.as_u64() as u32
        };

        if (compact & 0x00800000) != 0 {
            compact >>= 8;
            size += 1;
        }
        compact | (size << 24)
    }
}

impl From<Uint256> for Uint320 {
    #[inline]
    fn from(u: Uint256) -> Self {
        let mut result = Uint320::ZERO;
        result.0[..4].copy_from_slice(&u.0);
        result
    }
}

// ---------------------------------------------------------------------
// kaspa-pq Phase 8 (PR-8.2) Layered-PoW math helpers.
// ---------------------------------------------------------------------

impl Uint512 {
    /// kaspa-pq Layered-PoW target decoder. Bit-for-bit identical to
    /// upstream `Uint256::from_compact_target_bits` followed by the
    /// difficulty-lift identity `target_512 = target_256 << 256` from
    /// ADR-0007 §"Difficulty lift". Block-finding probability is
    /// preserved exactly under the ideal-uniform-hash model:
    /// `(target_256 << 256) / 2^512 == target_256 / 2^256`.
    #[inline]
    pub fn from_compact_target_bits_512(bits: u32) -> Self {
        let target_256 = Uint256::from_compact_target_bits(bits);
        Uint512::from(target_256) << 256
    }

    /// Inverse of [`from_compact_target_bits_512`] — returns the
    /// compact 32-bit `bits` encoding of `target_512 / 2^256`. For
    /// kaspa-pq Phase 1 the encoder is intentionally symmetric with
    /// the decoder; a future ADR can replace this if a finer-grained
    /// 512-bit-native compact form is wanted.
    #[inline]
    pub fn compact_target_bits_512(self) -> u32 {
        // Shift down 256 bits and re-use the 256-bit encoder.
        let lo = (self >> 256).try_into().unwrap_or(Uint256::ZERO);
        lo.compact_target_bits()
    }

    /// Block work `floor(2^512 / (target + 1))`. The +1 avoids the
    /// overflow that `2^512 / 2^512 = 1` would hit at the maximum
    /// possible target. Returns a [`Uint576`] so a 2^64 window of
    /// max-work blocks still fits without saturation.
    #[inline]
    pub fn calc_work_512(self) -> Uint576 {
        // (2^512 / (target + 1)) computed in 576-bit space so the
        // numerator fits without truncation.
        if self == Uint512::ZERO {
            // A zero target is consensus-invalid (it would mean every
            // hash satisfies the threshold). Return zero work — the
            // caller's bits-validation must catch this earlier.
            return Uint576::ZERO;
        }
        let two_pow_512 = {
            let mut limbs = [0u64; 9];
            // 2^512 = 1 << 512 in 576-bit width => set bit at position
            // 512 (the 8th u64 limb, zero-indexed).
            limbs[8] = 1;
            Uint576(limbs)
        };
        let target_plus_one: Uint576 = Uint576::from(self) + Uint576::from_u64(1);
        two_pow_512 / target_plus_one
    }
}

impl From<Uint192> for Uint576 {
    #[inline]
    fn from(u: Uint192) -> Self {
        let mut r = Uint576::ZERO;
        r.0[..3].copy_from_slice(&u.0);
        r
    }
}

impl From<Uint256> for Uint512 {
    #[inline]
    fn from(u: Uint256) -> Self {
        let mut r = Uint512::ZERO;
        r.0[..4].copy_from_slice(&u.0);
        r
    }
}

impl From<Uint256> for Uint576 {
    #[inline]
    fn from(u: Uint256) -> Self {
        let mut r = Uint576::ZERO;
        r.0[..4].copy_from_slice(&u.0);
        r
    }
}

impl From<Uint512> for Uint576 {
    #[inline]
    fn from(u: Uint512) -> Self {
        let mut r = Uint576::ZERO;
        r.0[..8].copy_from_slice(&u.0);
        r
    }
}

impl From<Uint512> for Uint640 {
    #[inline]
    fn from(u: Uint512) -> Self {
        let mut r = Uint640::ZERO;
        r.0[..8].copy_from_slice(&u.0);
        r
    }
}

impl From<Uint576> for Uint640 {
    #[inline]
    fn from(u: Uint576) -> Self {
        let mut r = Uint640::ZERO;
        r.0[..9].copy_from_slice(&u.0);
        r
    }
}

impl TryFrom<Uint512> for Uint256 {
    type Error = crate::uint::TryFromIntError;
    #[inline]
    fn try_from(value: Uint512) -> Result<Self, Self::Error> {
        // High four limbs must all be zero for a lossless narrowing.
        if value.0[4..].iter().any(|&w| w != 0) {
            return Err(crate::uint::TryFromIntError);
        }
        let mut r = Uint256::ZERO;
        r.0.copy_from_slice(&value.0[..4]);
        Ok(r)
    }
}

impl TryFrom<Uint576> for Uint512 {
    type Error = crate::uint::TryFromIntError;
    #[inline]
    fn try_from(value: Uint576) -> Result<Self, Self::Error> {
        if value.0[8] != 0 {
            return Err(crate::uint::TryFromIntError);
        }
        let mut r = Uint512::ZERO;
        r.0.copy_from_slice(&value.0[..8]);
        Ok(r)
    }
}

impl TryFrom<Uint640> for Uint576 {
    type Error = crate::uint::TryFromIntError;
    #[inline]
    fn try_from(value: Uint640) -> Result<Self, Self::Error> {
        if value.0[9] != 0 {
            return Err(crate::uint::TryFromIntError);
        }
        let mut r = Uint576::ZERO;
        r.0.copy_from_slice(&value.0[..9]);
        Ok(r)
    }
}

impl TryFrom<Uint320> for Uint256 {
    type Error = crate::uint::TryFromIntError;

    #[inline]
    fn try_from(value: Uint320) -> Result<Self, Self::Error> {
        if value.0[4] != 0 {
            Err(crate::uint::TryFromIntError)
        } else {
            let mut result = Uint256::ZERO;
            result.0.copy_from_slice(&value.0[..4]);
            Ok(result)
        }
    }
}

impl TryFrom<Uint256> for Uint192 {
    type Error = crate::uint::TryFromIntError;

    #[inline]
    fn try_from(value: Uint256) -> Result<Self, Self::Error> {
        if value.0[3] != 0 {
            Err(crate::uint::TryFromIntError)
        } else {
            let mut result = Uint192::ZERO;
            result.0.copy_from_slice(&value.0[..3]);
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Uint192, Uint256, Uint512, Uint576, Uint640, Uint3072};

    #[test]
    fn test_overflow_bug() {
        let a = Uint256::from_le_bytes([
            255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255,
        ]);
        let b = Uint256::from_le_bytes([
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 71, 33, 0, 0, 0, 0, 0, 0,
            0, 32, 0, 0, 0,
        ]);
        let c = a.overflowing_add(b).0;
        let expected = [254, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 71, 33, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0];
        assert_eq!(c.to_le_bytes(), expected);
    }
    /// kaspa-pq Phase 8 (PR-8.2). For every `bits` value the
    /// difficulty-lift identity from ADR-0007 §"Difficulty lift"
    /// holds at the integer level:
    ///   from_compact_target_bits_512(bits)
    ///     == Uint512::from(Uint256::from_compact_target_bits(bits)) << 256
    /// This is the math kaspa-pq's Layer 0 PoW comparison relies on.
    #[test]
    fn pq_difficulty_lift_identity() {
        // A handful of canonical compact-bits values: max difficulty,
        // the upstream Crescendo target, and a tiny mantissa near zero.
        for bits in [0x207fffffu32, 0x1d00ffffu32, 0x1e21bc1cu32, 486722099u32, 504155340u32, 0u32, 1u32] {
            let direct = Uint512::from_compact_target_bits_512(bits);
            let via_lift = Uint512::from(Uint256::from_compact_target_bits(bits)) << 256;
            assert_eq!(direct, via_lift, "lift identity failed for bits={bits:#x}");
        }
    }

    #[test]
    fn pq_compact_target_bits_512_roundtrip() {
        for bits in [0x207fffffu32, 0x1d00ffffu32, 0x1e21bc1cu32, 486722099u32] {
            let t = Uint512::from_compact_target_bits_512(bits);
            assert_eq!(t.compact_target_bits_512(), bits, "compact_bits roundtrip failed for {bits:#x}");
        }
    }

    /// `calc_work_512(target) ≈ 2^512 / (target + 1)`. We don't try to
    /// match upstream's `Uint256` work calculation byte-for-byte —
    /// kaspa-pq is a fresh chain — but the qualitative properties
    /// (max-target -> minimum work, zero-target -> zero work,
    /// halving target roughly doubles work) must hold.
    #[test]
    fn pq_calc_work_512_basic_properties() {
        // Max compact target = 0x207fffff in upstream simnet, lifted to 512 bits.
        let target_easy = Uint512::from_compact_target_bits_512(0x207fffff);
        let work_easy = target_easy.calc_work_512();
        assert!(work_easy != Uint576::ZERO, "max-target work must be positive");

        // Halve the target and the work must approximately double.
        let target_hard = target_easy >> 1;
        let work_hard = target_hard.calc_work_512();
        assert!(work_hard > work_easy, "harder target must yield more work");

        // Zero target is consensus-invalid; calc_work_512 returns zero.
        assert_eq!(Uint512::ZERO.calc_work_512(), Uint576::ZERO);
    }

    #[test]
    fn pq_uint_width_conversions() {
        // 192 -> 576 keeps the low 192 bits and leaves the rest zero.
        let u192 = Uint192::from_u64(0xdead_beef_cafe_babe);
        let u576: Uint576 = u192.into();
        assert_eq!(u576.0[0], 0xdead_beef_cafe_babe);
        assert_eq!(&u576.0[1..], &[0u64; 8]);

        // 256 -> 512 -> 576 -> 640 chain.
        let u256 = Uint256::from_u64(0x1234_5678_9abc_def0);
        let u512: Uint512 = u256.into();
        let u576: Uint576 = u512.into();
        let u640: Uint640 = u576.into();
        assert_eq!(u640.0[0], 0x1234_5678_9abc_def0);
        assert_eq!(&u640.0[1..], &[0u64; 9]);

        // Lossless narrowing.
        let small: Uint512 = Uint256::from_u64(7).into();
        let back: Uint256 = small.try_into().unwrap();
        assert_eq!(back, Uint256::from_u64(7));

        // Lossy narrowing fails.
        let big = Uint512::from_u64(1) << 257;
        let res: Result<Uint256, _> = big.try_into();
        assert!(res.is_err(), "lossy Uint512 -> Uint256 must error");
    }

    #[rustfmt::skip]
    #[test]
    fn div_rem_u3072_bug() {
        let r = Uint3072([
            18446744073708447899, 18446744069733351423, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073642442751, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
        ]);
        let newr = Uint3072([
            0, 3976200192, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 67108864, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        let expected = Uint3072([
            18446744073709551614, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 274877906943, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        assert_eq!(r / newr, expected);
    }
}
