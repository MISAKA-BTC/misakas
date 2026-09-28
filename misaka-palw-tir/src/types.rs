//! Element types, dimensions and tensor types (spec 04b §2, PALW-TIR-20..24).
//!
//! Tensors are raw integers. There is no float type, no scale type and no unsigned type other than
//! `idx`. Every value the interpreter holds is a mathematical integer carried in an `i128`; the
//! dtype is the interval that value must lie in.

use borsh::{BorshDeserialize, BorshSerialize};

/// An element type. The Borsh tag of each variant is its wire value and is frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub enum DType {
    /// tag 0: `[-2^7, 2^7 - 1]`.
    I8,
    /// tag 1: `[-2^15, 2^15 - 1]`.
    I16,
    /// tag 2: `[-2^31, 2^31 - 1]`.
    I32,
    /// tag 3: `[-2^63, 2^63 - 1]`. Never committed, never state (PALW-TIR-5).
    I64,
    /// tag 4: `[-2^127, 2^127 - 1]`. Never committed, never state, never a param; exists so that a
    /// fixed-point multiply of two `i64` values is an exact product rather than a fused primitive.
    I128,
    /// tag 5: `[0, 2^32 - 1]` — token ids, positions, selection indices.
    Idx,
}

impl DType {
    pub const ALL: [DType; 6] = [DType::I8, DType::I16, DType::I32, DType::I64, DType::I128, DType::Idx];

    pub const fn tag(self) -> u8 {
        match self {
            DType::I8 => 0,
            DType::I16 => 1,
            DType::I32 => 2,
            DType::I64 => 3,
            DType::I128 => 4,
            DType::Idx => 5,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            DType::I8 => "i8",
            DType::I16 => "i16",
            DType::I32 => "i32",
            DType::I64 => "i64",
            DType::I128 => "i128",
            DType::Idx => "idx",
        }
    }

    pub fn from_name(name: &str) -> Option<DType> {
        DType::ALL.into_iter().find(|d| d.name() == name)
    }

    pub const fn min(self) -> i128 {
        match self {
            DType::I8 => i8::MIN as i128,
            DType::I16 => i16::MIN as i128,
            DType::I32 => i32::MIN as i128,
            DType::I64 => i64::MIN as i128,
            DType::I128 => i128::MIN,
            DType::Idx => 0,
        }
    }

    pub const fn max(self) -> i128 {
        match self {
            DType::I8 => i8::MAX as i128,
            DType::I16 => i16::MAX as i128,
            DType::I32 => i32::MAX as i128,
            DType::I64 => i64::MAX as i128,
            DType::I128 => i128::MAX,
            DType::Idx => u32::MAX as i128,
        }
    }

    pub const fn contains(self, v: i128) -> bool {
        v >= self.min() && v <= self.max()
    }

    /// Bytes per element in a const, a param and a history row.
    pub const fn width(self) -> usize {
        match self {
            DType::I8 => 1,
            DType::I16 => 2,
            DType::I32 => 4,
            DType::I64 => 8,
            DType::I128 => 16,
            DType::Idx => 4,
        }
    }

    /// May a value of this type be a commit point, a carry, or state (4-byte lanes)?
    pub const fn committable(self) -> bool {
        matches!(self, DType::I8 | DType::I16 | DType::I32 | DType::Idx)
    }

    /// Decode one little-endian element of this type (`bytes.len() == width`).
    pub fn decode_le(self, bytes: &[u8]) -> i128 {
        match self {
            DType::I8 => bytes[0] as i8 as i128,
            DType::I16 => i16::from_le_bytes([bytes[0], bytes[1]]) as i128,
            DType::I32 => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i128,
            DType::Idx => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i128,
            DType::I64 => {
                let mut b = [0u8; 8];
                b.copy_from_slice(&bytes[..8]);
                i64::from_le_bytes(b) as i128
            }
            DType::I128 => {
                let mut b = [0u8; 16];
                b.copy_from_slice(&bytes[..16]);
                i128::from_le_bytes(b)
            }
        }
    }

    /// Encode one element (the caller guarantees `self.contains(v)`).
    pub fn encode_le(self, v: i128, out: &mut Vec<u8>) {
        match self {
            DType::I8 => out.push(v as i8 as u8),
            DType::I16 => out.extend_from_slice(&(v as i16).to_le_bytes()),
            DType::I32 => out.extend_from_slice(&(v as i32).to_le_bytes()),
            DType::Idx => out.extend_from_slice(&(v as u32).to_le_bytes()),
            DType::I64 => out.extend_from_slice(&(v as i64).to_le_bytes()),
            DType::I128 => out.extend_from_slice(&v.to_le_bytes()),
        }
    }
}

/// One dimension: a constant, or the history length `H` of the block the tensor lives in.
///
/// `H = min(pos + 1, window)` where `window` is the window shared by every `Hist` state the block
/// appends to (PALW-TIR-31). It is the only symbolic dimension (PALW-TIR-6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub enum Dim {
    /// tag 0: a constant in `[1, 2^24]`.
    Fixed(u32),
    /// tag 1: the block's history length.
    H,
}

impl Dim {
    pub const fn is_h(self) -> bool {
        matches!(self, Dim::H)
    }

    /// The concrete extent at a given `H`.
    pub const fn at(self, h: usize) -> usize {
        match self {
            Dim::Fixed(n) => n as usize,
            Dim::H => h,
        }
    }
}

/// Largest rank of any tensor.
pub const MAX_RANK: usize = 4;
/// Largest constant dimension.
pub const MAX_DIM: u32 = 1 << 24;
/// Largest element count of any tensor, evaluated at the worst-case `H`.
pub const MAX_ELEMENTS: u64 = 1 << 28;

/// A tensor type: element type and shape. Rank 0 is a scalar.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct TensorType {
    pub dtype: DType,
    pub shape: Vec<Dim>,
}

impl TensorType {
    pub fn new(dtype: DType, shape: Vec<Dim>) -> Self {
        Self { dtype, shape }
    }

    pub fn fixed(dtype: DType, shape: &[u32]) -> Self {
        Self { dtype, shape: shape.iter().map(|d| Dim::Fixed(*d)).collect() }
    }

    pub fn scalar(dtype: DType) -> Self {
        Self { dtype, shape: Vec::new() }
    }

    pub fn rank(&self) -> usize {
        self.shape.len()
    }

    pub fn has_h(&self) -> bool {
        self.shape.iter().any(|d| d.is_h())
    }

    /// The concrete shape at a given `H`.
    pub fn resolve(&self, h: usize) -> Vec<usize> {
        self.shape.iter().map(|d| d.at(h)).collect()
    }

    /// Element count at a given `H`, saturating (a count past `u64` is past every cap anyway).
    pub fn elements_at(&self, h: u64) -> u64 {
        self.shape.iter().fold(1u64, |acc, d| {
            acc.saturating_mul(match d {
                Dim::Fixed(n) => *n as u64,
                Dim::H => h,
            })
        })
    }
}

/// Numpy-style broadcast of two symbolic shapes: align trailing dimensions; equal dimensions stay,
/// a `Fixed(1)` takes the other side's dimension; anything else is a shape error.
pub fn broadcast_shapes(a: &[Dim], b: &[Dim]) -> Option<Vec<Dim>> {
    let rank = a.len().max(b.len());
    let mut out = vec![Dim::Fixed(1); rank];
    for (i, slot) in out.iter_mut().enumerate() {
        let da = if i + a.len() >= rank { a[i + a.len() - rank] } else { Dim::Fixed(1) };
        let db = if i + b.len() >= rank { b[i + b.len() - rank] } else { Dim::Fixed(1) };
        *slot = if da == db {
            da
        } else if da == Dim::Fixed(1) {
            db
        } else if db == Dim::Fixed(1) {
            da
        } else {
            return None;
        };
    }
    Some(out)
}

/// Row-major strides of a concrete shape.
pub fn strides(shape: &[usize]) -> Vec<usize> {
    let mut s = vec![1usize; shape.len()];
    for i in (0..shape.len().saturating_sub(1)).rev() {
        s[i] = s[i + 1] * shape[i + 1];
    }
    s
}

pub fn element_count(shape: &[usize]) -> usize {
    shape.iter().product()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtype_tags_and_ranges_are_frozen() {
        let tags: Vec<u8> = DType::ALL.iter().map(|d| d.tag()).collect();
        assert_eq!(tags, vec![0, 1, 2, 3, 4, 5]);
        for d in DType::ALL {
            let enc = borsh::to_vec(&d).unwrap();
            assert_eq!(enc, vec![d.tag()], "{} encodes as its tag", d.name());
            assert_eq!(DType::from_name(d.name()), Some(d));
        }
        assert_eq!((DType::I8.min(), DType::I8.max()), (-128, 127));
        assert_eq!((DType::Idx.min(), DType::Idx.max()), (0, 4_294_967_295));
        assert!(DType::I32.committable() && !DType::I64.committable() && !DType::I128.committable());
    }

    #[test]
    fn broadcasting_follows_numpy_and_carries_h() {
        use Dim::*;
        assert_eq!(broadcast_shapes(&[Fixed(3), Fixed(1)], &[Fixed(4)]), Some(vec![Fixed(3), Fixed(4)]));
        assert_eq!(broadcast_shapes(&[H, Fixed(2)], &[Fixed(1), Fixed(2)]), Some(vec![H, Fixed(2)]));
        assert_eq!(broadcast_shapes(&[H], &[Fixed(3)]), None);
        assert_eq!(broadcast_shapes(&[], &[Fixed(5)]), Some(vec![Fixed(5)]));
        assert_eq!(strides(&[2, 3, 4]), vec![12, 4, 1]);
    }

    #[test]
    fn element_codec_round_trips_every_type_at_its_ends() {
        for d in DType::ALL {
            for v in [d.min(), d.max(), 0, 1] {
                let mut buf = Vec::new();
                d.encode_le(v, &mut buf);
                assert_eq!(buf.len(), d.width());
                assert_eq!(d.decode_le(&buf), v, "{} {v}", d.name());
            }
        }
    }
}
