//! Native element types and typed storage.
//!
//! Every PALW-TIR dtype has one native Rust type: `i8`, `i16`, `i32`, `i64`, `i128` and `u32` for
//! `idx`. A tensor is stored as a `Vec` (or borrowed as a slice) of that type — never boxed per
//! element and never widened to `i128` unless its declared dtype is `i128` (spec 04b §2.1: the
//! internal type exists for exact fixed-point products, and only nodes that declare it pay for it).

use misaka_palw_tir::DType;

/// A native element type of one PALW-TIR dtype.
pub trait Elem: Copy + Default + PartialEq + Eq + PartialOrd + Ord + Send + Sync + std::fmt::Debug + 'static {
    const DTYPE: DType;
    /// The mathematical value.
    fn to_i128(self) -> i128;
    /// The value as `i64`; truncating for an `i128` outside `i64` (callers prove it fits).
    fn to_i64(self) -> i64;
    /// Truncating conversion; callers prove `v` is a value of `DTYPE`.
    fn from_i128(v: i128) -> Self;
    /// Truncating conversion; callers prove `v` is a value of `DTYPE`.
    fn from_i64(v: i64) -> Self;
    fn slice_of(s: Slice<'_>) -> Option<&[Self]>;
    fn vec_of(b: &mut Buf) -> Option<&mut Vec<Self>>;
    fn into_buf(v: Vec<Self>) -> Buf;
}

macro_rules! impl_elem {
    ($t:ty, $dt:ident, $var:ident) => {
        impl Elem for $t {
            const DTYPE: DType = DType::$dt;
            #[inline(always)]
            fn to_i128(self) -> i128 {
                self as i128
            }
            #[inline(always)]
            fn to_i64(self) -> i64 {
                self as i64
            }
            #[inline(always)]
            fn from_i128(v: i128) -> Self {
                v as $t
            }
            #[inline(always)]
            fn from_i64(v: i64) -> Self {
                v as $t
            }
            #[inline(always)]
            fn slice_of(s: Slice<'_>) -> Option<&[Self]> {
                match s {
                    Slice::$var(v) => Some(v),
                    _ => None,
                }
            }
            #[inline(always)]
            fn vec_of(b: &mut Buf) -> Option<&mut Vec<Self>> {
                match b {
                    Buf::$var(v) => Some(v),
                    _ => None,
                }
            }
            #[inline(always)]
            fn into_buf(v: Vec<Self>) -> Buf {
                Buf::$var(v)
            }
        }
    };
}

impl_elem!(i8, I8, I8);
impl_elem!(i16, I16, I16);
impl_elem!(i32, I32, I32);
impl_elem!(i64, I64, I64);
impl_elem!(i128, I128, I128);
impl_elem!(u32, Idx, Idx);

/// An owned, typed, contiguous buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Buf {
    I8(Vec<i8>),
    I16(Vec<i16>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    I128(Vec<i128>),
    Idx(Vec<u32>),
}

impl Default for Buf {
    fn default() -> Self {
        Buf::I8(Vec::new())
    }
}

/// A borrowed, typed, contiguous run of elements.
#[derive(Clone, Copy, Debug)]
pub enum Slice<'a> {
    I8(&'a [i8]),
    I16(&'a [i16]),
    I32(&'a [i32]),
    I64(&'a [i64]),
    I128(&'a [i128]),
    Idx(&'a [u32]),
}

/// Expand `$body` once per native type, with `$v` bound to the typed slice.
#[macro_export]
macro_rules! with_slice {
    ($s:expr, $v:ident => $body:expr) => {
        match $s {
            $crate::elem::Slice::I8($v) => $body,
            $crate::elem::Slice::I16($v) => $body,
            $crate::elem::Slice::I32($v) => $body,
            $crate::elem::Slice::I64($v) => $body,
            $crate::elem::Slice::I128($v) => $body,
            $crate::elem::Slice::Idx($v) => $body,
        }
    };
}

/// Expand `$body` once per native type, with `$v` bound to the typed `&mut Vec`.
#[macro_export]
macro_rules! with_buf_mut {
    ($b:expr, $v:ident => $body:expr) => {
        match $b {
            $crate::elem::Buf::I8($v) => $body,
            $crate::elem::Buf::I16($v) => $body,
            $crate::elem::Buf::I32($v) => $body,
            $crate::elem::Buf::I64($v) => $body,
            $crate::elem::Buf::I128($v) => $body,
            $crate::elem::Buf::Idx($v) => $body,
        }
    };
}

/// Expand `$body` with the type alias `$t` bound to the native type of `$dt`.
#[macro_export]
macro_rules! with_dtype {
    ($dt:expr, $t:ident => $body:expr) => {
        match $dt {
            misaka_palw_tir::DType::I8 => {
                type $t = i8;
                $body
            }
            misaka_palw_tir::DType::I16 => {
                type $t = i16;
                $body
            }
            misaka_palw_tir::DType::I32 => {
                type $t = i32;
                $body
            }
            misaka_palw_tir::DType::I64 => {
                type $t = i64;
                $body
            }
            misaka_palw_tir::DType::I128 => {
                type $t = i128;
                $body
            }
            misaka_palw_tir::DType::Idx => {
                type $t = u32;
                $body
            }
        }
    };
}

impl Buf {
    /// An empty buffer of `dtype`.
    pub fn empty(dtype: DType) -> Self {
        with_dtype!(dtype, T => T::into_buf(Vec::new()))
    }

    /// `n` zeros of `dtype`.
    pub fn zeros(dtype: DType, n: usize) -> Self {
        with_dtype!(dtype, T => T::into_buf(vec![T::default(); n]))
    }

    pub fn dtype(&self) -> DType {
        match self {
            Buf::I8(_) => DType::I8,
            Buf::I16(_) => DType::I16,
            Buf::I32(_) => DType::I32,
            Buf::I64(_) => DType::I64,
            Buf::I128(_) => DType::I128,
            Buf::Idx(_) => DType::Idx,
        }
    }

    pub fn len(&self) -> usize {
        with_slice!(self.slice(), v => v.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn slice(&self) -> Slice<'_> {
        match self {
            Buf::I8(v) => Slice::I8(v),
            Buf::I16(v) => Slice::I16(v),
            Buf::I32(v) => Slice::I32(v),
            Buf::I64(v) => Slice::I64(v),
            Buf::I128(v) => Slice::I128(v),
            Buf::Idx(v) => Slice::Idx(v),
        }
    }

    /// Make this an owned buffer of `dtype` (reusing the allocation when the type already
    /// matches), with length `n`. The contents are unspecified; callers overwrite every element.
    pub fn reset(&mut self, dtype: DType, n: usize) {
        if self.dtype() != dtype {
            *self = Buf::zeros(dtype, n);
        } else {
            with_buf_mut!(self, v => v.resize(n, Default::default()));
        }
    }

    /// A buffer of `dtype` from mathematical values the caller has checked against the dtype.
    pub fn from_i128s(dtype: DType, data: &[i128]) -> Self {
        with_dtype!(dtype, T => T::into_buf(data.iter().map(|v| T::from_i128(*v)).collect()))
    }

    pub fn to_i128s(&self) -> Vec<i128> {
        with_slice!(self.slice(), v => v.iter().map(|x| x.to_i128()).collect())
    }

    /// Decode little-endian elements (consts, params, history rows).
    pub fn from_le_bytes(dtype: DType, bytes: &[u8]) -> Option<Self> {
        let w = dtype.width();
        if !bytes.len().is_multiple_of(w) {
            return None;
        }
        Some(with_dtype!(dtype, T => {
            let v: Vec<T> = bytes.chunks_exact(w).map(|c| T::from_i128(dtype.decode_le(c))).collect();
            T::into_buf(v)
        }))
    }

    pub fn to_le_bytes(&self) -> Vec<u8> {
        let dtype = self.dtype();
        let mut out = Vec::with_capacity(self.len() * dtype.width());
        with_slice!(self.slice(), v => {
            for x in v.iter() {
                dtype.encode_le(x.to_i128(), &mut out);
            }
        });
        out
    }
}

impl<'a> Slice<'a> {
    /// The elements as little-endian bytes of their dtype (a param's artifact form).
    pub fn to_le_bytes(&self) -> Vec<u8> {
        let dtype = self.dtype();
        let mut out = Vec::with_capacity(self.len() * dtype.width());
        with_slice!(*self, v => {
            for x in v.iter() {
                dtype.encode_le(x.to_i128(), &mut out);
            }
        });
        out
    }

    pub fn dtype(&self) -> DType {
        match self {
            Slice::I8(_) => DType::I8,
            Slice::I16(_) => DType::I16,
            Slice::I32(_) => DType::I32,
            Slice::I64(_) => DType::I64,
            Slice::I128(_) => DType::I128,
            Slice::Idx(_) => DType::Idx,
        }
    }

    pub fn len(&self) -> usize {
        with_slice!(*self, v => v.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Element `i` as a mathematical integer (tests, diagnostics, small reads).
    #[inline]
    pub fn get(&self, i: usize) -> i128 {
        with_slice!(*self, v => v[i].to_i128())
    }

    pub fn to_i128s(&self) -> Vec<i128> {
        with_slice!(*self, v => v.iter().map(|x| x.to_i128()).collect())
    }

    pub fn to_buf(&self) -> Buf {
        with_slice!(*self, v => Elem::into_buf(v.to_vec()))
    }

    /// Elements `offset .. offset + n`.
    pub fn sub(&self, offset: usize, n: usize) -> Slice<'a> {
        match *self {
            Slice::I8(v) => Slice::I8(&v[offset..offset + n]),
            Slice::I16(v) => Slice::I16(&v[offset..offset + n]),
            Slice::I32(v) => Slice::I32(&v[offset..offset + n]),
            Slice::I64(v) => Slice::I64(&v[offset..offset + n]),
            Slice::I128(v) => Slice::I128(&v[offset..offset + n]),
            Slice::Idx(v) => Slice::Idx(&v[offset..offset + n]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dtype_has_one_native_type_and_round_trips_its_ends() {
        for d in DType::ALL {
            let data = [d.min_value(), d.max_value(), 0];
            let b = Buf::from_i128s(d, &data);
            assert_eq!(b.dtype(), d);
            assert_eq!(b.to_i128s(), data.to_vec());
            let le = b.to_le_bytes();
            assert_eq!(le.len(), 3 * d.width());
            assert_eq!(Buf::from_le_bytes(d, &le).unwrap(), b);
        }
    }
}
