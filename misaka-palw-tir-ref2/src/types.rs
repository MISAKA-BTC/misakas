//! Element types, dimensions and broadcasting (04b §2).

use crate::error::{Class, Res, err};

/// 04b §2.1. The tag is the §4.2 `DType` tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DType {
    I8,
    I16,
    I32,
    I64,
    I128,
    Idx,
}

impl DType {
    pub const ALL: [DType; 6] = [DType::I8, DType::I16, DType::I32, DType::I64, DType::I128, DType::Idx];

    pub fn from_tag(t: u8) -> Option<DType> {
        DType::ALL.get(t as usize).copied()
    }

    pub fn tag(self) -> u8 {
        match self {
            DType::I8 => 0,
            DType::I16 => 1,
            DType::I32 => 2,
            DType::I64 => 3,
            DType::I128 => 4,
            DType::Idx => 5,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            DType::I8 => "i8",
            DType::I16 => "i16",
            DType::I32 => "i32",
            DType::I64 => "i64",
            DType::I128 => "i128",
            DType::Idx => "idx",
        }
    }

    pub fn from_name(s: &str) -> Option<DType> {
        DType::ALL.iter().copied().find(|d| d.name() == s)
    }

    /// Bytes per element (§2.1 table).
    pub fn width(self) -> usize {
        match self {
            DType::I8 => 1,
            DType::I16 => 2,
            DType::I32 | DType::Idx => 4,
            DType::I64 => 8,
            DType::I128 => 16,
        }
    }

    /// The smallest value: `−2^(8·width − 1)` for the signed types, 0 for `idx`.
    pub fn min(self) -> i128 {
        match self {
            DType::Idx => 0,
            DType::I128 => i128::MIN,
            d => -pow2(8 * d.width() as u32 - 1),
        }
    }

    /// The largest value: `2^(8·width − 1) − 1` for the signed types, `2^32 − 1` for `idx`.
    pub fn max(self) -> i128 {
        match self {
            DType::Idx => pow2(32) - 1,
            DType::I128 => i128::MAX,
            d => pow2(8 * d.width() as u32 - 1) - 1,
        }
    }

    pub fn contains(self, v: i128) -> bool {
        self.min() <= v && v <= self.max()
    }

    /// §2.1 "committable": `i8`, `i16`, `i32`, `idx`.
    pub fn committable(self) -> bool {
        !matches!(self, DType::I64 | DType::I128)
    }
}

/// `2^k` for `k ≤ 126`.
pub fn pow2(k: u32) -> i128 {
    let mut v: i128 = 1;
    for _ in 0..k {
        v *= 2;
    }
    v
}

/// 04b §2.2: `Fixed(n)` or the history length `H`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dim {
    Fixed(u32),
    H,
}

/// The largest `Fixed` extent, `2^24`.
pub const MAX_DIM: u64 = 1 << 24;
/// The worst-case element cap of every computed tensor, `2^28`.
pub const MAX_ELEMENTS: u128 = 1 << 28;
/// The rank cap.
pub const MAX_RANK: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TensorType {
    pub dtype: DType,
    pub shape: Vec<Dim>,
}

impl TensorType {
    pub fn fixed(dtype: DType, shape: &[u32]) -> TensorType {
        TensorType { dtype, shape: shape.iter().map(|&n| Dim::Fixed(n)).collect() }
    }

    pub fn h_count(&self) -> usize {
        self.shape.iter().filter(|d| **d == Dim::H).count()
    }

    /// The concrete extents at a given `H`.
    pub fn extents(&self, h: u64) -> Vec<u64> {
        self.shape
            .iter()
            .map(|d| match d {
                Dim::Fixed(n) => *n as u64,
                Dim::H => h,
            })
            .collect()
    }

    /// §2.2 legality of a computed tensor's type: rank ≤ 4, at most one `H`, every `Fixed(n)` in
    /// `[1, 2^24]`, `H` only where the block has a window, and at most `2^28` elements at `H = W`.
    pub fn check_legal(&self, window: Option<u32>) -> Res<()> {
        if self.shape.len() > MAX_RANK {
            return err(Class::Shape, format!("rank {} above {}", self.shape.len(), MAX_RANK));
        }
        let hs = self.h_count();
        if hs > 1 {
            return err(Class::Shape, "more than one H in a shape");
        }
        for d in &self.shape {
            if let Dim::Fixed(n) = d {
                if *n == 0 || *n as u64 > MAX_DIM {
                    return err(Class::Shape, format!("dimension {n} outside [1, 2^24]"));
                }
            }
        }
        let w = match (hs, window) {
            (0, _) => 1u128,
            (_, Some(w)) => w as u128,
            (_, None) => return err(Class::Shape, "H in a block without a window"),
        };
        let mut count: u128 = 1;
        for d in &self.shape {
            let e = match d {
                Dim::Fixed(n) => *n as u128,
                Dim::H => w,
            };
            count *= e; // ≤ (2^24)^4 · … fits u128
        }
        if count > MAX_ELEMENTS {
            return err(Class::Shape, format!("{count} elements at the worst case, above 2^28"));
        }
        Ok(())
    }
}

/// §2.3 broadcasting of two shapes (symbolic).
pub fn broadcast_shapes(a: &[Dim], b: &[Dim]) -> Res<Vec<Dim>> {
    let r = a.len().max(b.len());
    let mut out = Vec::with_capacity(r);
    for i in 0..r {
        // Aligned from the right; a missing leading dimension is Fixed(1).
        let da = if i + a.len() >= r { a[i + a.len() - r] } else { Dim::Fixed(1) };
        let db = if i + b.len() >= r { b[i + b.len() - r] } else { Dim::Fixed(1) };
        let d = if da == db {
            da
        } else if da == Dim::Fixed(1) {
            db
        } else if db == Dim::Fixed(1) {
            da
        } else {
            return err(Class::Shape, format!("no broadcast of {da:?} and {db:?}"));
        };
        out.push(d);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!((DType::I8.min(), DType::I8.max()), (-128, 127));
        assert_eq!((DType::I16.min(), DType::I16.max()), (-32768, 32767));
        assert_eq!((DType::I32.min(), DType::I32.max()), (i32::MIN as i128, i32::MAX as i128));
        assert_eq!((DType::I64.min(), DType::I64.max()), (i64::MIN as i128, i64::MAX as i128));
        assert_eq!((DType::Idx.min(), DType::Idx.max()), (0, u32::MAX as i128));
    }

    #[test]
    fn broadcast_rules() {
        use Dim::*;
        assert_eq!(broadcast_shapes(&[Fixed(1), H], &[Fixed(3), Fixed(1)]).unwrap(), vec![Fixed(3), H]);
        assert_eq!(broadcast_shapes(&[H], &[Fixed(2), Fixed(1)]).unwrap(), vec![Fixed(2), H]);
        assert!(broadcast_shapes(&[H], &[Fixed(2)]).is_err());
        assert_eq!(broadcast_shapes(&[], &[]).unwrap(), vec![]);
    }
}
