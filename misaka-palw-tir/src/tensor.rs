//! Concrete tensor values: a dtype, a concrete row-major shape, and the elements as mathematical
//! integers (`i128`). Slow and obviously correct: every element of every type is one `i128`.

use crate::error::{TirErrorKind, TirResult, err};
use crate::types::{DType, element_count};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Tensor {
    pub dtype: DType,
    pub shape: Vec<usize>,
    pub data: Vec<i128>,
}

impl Tensor {
    /// A tensor whose every element is checked against its dtype and whose length matches its shape.
    pub fn new(dtype: DType, shape: Vec<usize>, data: Vec<i128>) -> TirResult<Self> {
        if element_count(&shape) != data.len() {
            return err(TirErrorKind::Operand, format!("{} elements for shape {:?}", data.len(), shape));
        }
        if let Some(v) = data.iter().find(|v| !dtype.contains(**v)) {
            return err(TirErrorKind::Operand, format!("{v} is not a {}", dtype.name()));
        }
        Ok(Self { dtype, shape, data })
    }

    pub fn from_i64(dtype: DType, shape: &[usize], data: &[i64]) -> TirResult<Self> {
        Self::new(dtype, shape.to_vec(), data.iter().map(|v| *v as i128).collect())
    }

    pub fn scalar(dtype: DType, v: i128) -> TirResult<Self> {
        Self::new(dtype, Vec::new(), vec![v])
    }

    pub fn zeros(dtype: DType, shape: &[usize]) -> Self {
        Self { dtype, shape: shape.to_vec(), data: vec![0; element_count(shape)] }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Decode a little-endian byte image (consts, params, history rows).
    pub fn from_le_bytes(dtype: DType, shape: &[usize], bytes: &[u8]) -> TirResult<Self> {
        let n = element_count(shape);
        if bytes.len() != n * dtype.width() {
            return err(TirErrorKind::Operand, format!("{} bytes for {n} {} elements", bytes.len(), dtype.name()));
        }
        let data = bytes.chunks_exact(dtype.width()).map(|c| dtype.decode_le(c)).collect();
        Ok(Self { dtype, shape: shape.to_vec(), data })
    }

    pub fn to_le_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.data.len() * self.dtype.width());
        for v in &self.data {
            self.dtype.encode_le(*v, &mut out);
        }
        out
    }

    /// Every element as `i64` (tests and callers that know the values fit).
    pub fn to_i64(&self) -> Vec<i64> {
        self.data.iter().map(|v| *v as i64).collect()
    }
}
