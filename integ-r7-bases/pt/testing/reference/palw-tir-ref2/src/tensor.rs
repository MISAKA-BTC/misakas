//! Concrete tensors: a dtype, concrete extents and row-major elements held as mathematical integers
//! (`i128` holds every value of every dtype).

use crate::error::{Class, Res, err};
use crate::types::DType;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tensor {
    pub dtype: DType,
    pub shape: Vec<u64>,
    pub data: Vec<i128>,
}

/// The element count of a concrete shape (`1` for rank 0), saturating at `u64::MAX` so that a
/// hostile shape can never wrap into a plausible count.
pub fn count(shape: &[u64]) -> u64 {
    shape.iter().fold(1u64, |a, &d| a.saturating_mul(d))
}

/// §0: multi-index of a linear position, by repeated division from the last axis.
pub fn unravel(mut linear: u64, shape: &[u64], out: &mut [u64]) {
    for k in (0..shape.len()).rev() {
        out[k] = linear % shape[k];
        linear /= shape[k];
    }
}

/// §0: the linear position `Σ i_k · s_k` with `s_(r−1) = 1`, `s_k = s_(k+1) · d_(k+1)`.
pub fn ravel(index: &[u64], shape: &[u64]) -> u64 {
    let mut stride: u64 = 1;
    let mut pos: u64 = 0;
    for k in (0..shape.len()).rev() {
        pos += index[k] * stride;
        stride *= shape[k];
    }
    pos
}

impl Tensor {
    /// A tensor checked to be well formed: `Π shape` elements, each a value of `dtype`.
    pub fn new(dtype: DType, shape: Vec<u64>, data: Vec<i128>) -> Res<Tensor> {
        if count(&shape) != data.len() as u64 {
            return err(Class::Operand, "element count is not the shape's");
        }
        if let Some(v) = data.iter().find(|v| !dtype.contains(**v)) {
            return err(Class::Operand, format!("{v} is not a value of {}", dtype.name()));
        }
        Ok(Tensor { dtype, shape, data })
    }

    pub fn zeros(dtype: DType, shape: Vec<u64>) -> Tensor {
        let n = count(&shape) as usize;
        Tensor { dtype, shape, data: vec![0; n] }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Decode little-endian two's-complement elements (§2.1; `idx` unsigned).
    pub fn from_le_bytes(dtype: DType, shape: Vec<u64>, bytes: &[u8]) -> Res<Tensor> {
        let w = dtype.width();
        let n = count(&shape);
        if bytes.len() as u128 != n as u128 * w as u128 {
            return err(Class::Operand, "byte length is not elements × width");
        }
        let mut data = Vec::with_capacity(n as usize);
        for chunk in bytes.chunks(w) {
            let mut u: u128 = 0;
            for (i, &b) in chunk.iter().enumerate() {
                u |= (b as u128) << (8 * i);
            }
            let v = if dtype == DType::Idx {
                u as i128
            } else {
                // Sign-extend from 8·w bits.
                let bits = 8 * w as u32;
                if bits == 128 {
                    u as i128
                } else if u >> (bits - 1) & 1 == 1 {
                    (u as i128) - (1i128 << bits)
                } else {
                    u as i128
                }
            };
            data.push(v);
        }
        Ok(Tensor { dtype, shape, data })
    }

    pub fn to_le_bytes(&self) -> Vec<u8> {
        let w = self.dtype.width();
        let mut out = Vec::with_capacity(self.data.len() * w);
        for &v in &self.data {
            let u = v as u128;
            for i in 0..w {
                out.push((u >> (8 * i)) as u8);
            }
        }
        out
    }
}
