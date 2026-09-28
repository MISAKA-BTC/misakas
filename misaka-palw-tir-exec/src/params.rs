//! Params: typed, borrowed, zero-copy.
//!
//! A param instance is a slice of its declared native type. From a mapped artifact it is the
//! mapping itself (little-endian elements reinterpreted in place when the target is little-endian
//! and the bytes are aligned); nothing is copied or widened. Every bit pattern of `i8`…`i64` and
//! `u32` is a value of its dtype (spec 04b §3.5: params take their dtype's full range), so a
//! borrowed tensor needs only its length checked.

use std::borrow::Cow;

use misaka_palw_tir::{DType, MapParams, TirError, TirErrorKind, TirResult};

use crate::elem::{Buf, Elem, Slice};
use crate::plan::TirPlan;

/// One param instance's elements.
#[derive(Clone, Debug)]
pub enum ParamData<'a> {
    I8(Cow<'a, [i8]>),
    I16(Cow<'a, [i16]>),
    I32(Cow<'a, [i32]>),
    I64(Cow<'a, [i64]>),
    Idx(Cow<'a, [u32]>),
}

impl<'a> ParamData<'a> {
    pub fn dtype(&self) -> DType {
        match self {
            ParamData::I8(_) => DType::I8,
            ParamData::I16(_) => DType::I16,
            ParamData::I32(_) => DType::I32,
            ParamData::I64(_) => DType::I64,
            ParamData::Idx(_) => DType::Idx,
        }
    }

    pub fn len(&self) -> usize {
        self.slice().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn slice(&self) -> Slice<'_> {
        match self {
            ParamData::I8(v) => Slice::I8(v),
            ParamData::I16(v) => Slice::I16(v),
            ParamData::I32(v) => Slice::I32(v),
            ParamData::I64(v) => Slice::I64(v),
            ParamData::Idx(v) => Slice::Idx(v),
        }
    }

    /// Borrow little-endian bytes as elements of `dtype` — in place when possible (a little-endian
    /// target and aligned bytes), otherwise decoded into an owned copy.
    pub fn from_le_bytes(dtype: DType, bytes: &'a [u8]) -> TirResult<Self> {
        if !bytes.len().is_multiple_of(dtype.width()) {
            return Err(TirError::new(TirErrorKind::Operand, "param bytes are not whole elements"));
        }
        fn cast<'b, T: Elem>(bytes: &'b [u8], dtype: DType) -> Cow<'b, [T]> {
            if cfg!(target_endian = "little") {
                // SAFETY: every bit pattern is a valid value of the integer type T, and `align_to`
                // only returns a middle part that is correctly aligned.
                let (pre, mid, post) = unsafe { bytes.align_to::<T>() };
                if pre.is_empty() && post.is_empty() {
                    return Cow::Borrowed(mid);
                }
            }
            Cow::Owned(bytes.chunks_exact(dtype.width()).map(|c| T::from_i128(dtype.decode_le(c))).collect())
        }
        Ok(match dtype {
            DType::I8 => ParamData::I8(cast(bytes, dtype)),
            DType::I16 => ParamData::I16(cast(bytes, dtype)),
            DType::I32 => ParamData::I32(cast(bytes, dtype)),
            DType::I64 => ParamData::I64(cast(bytes, dtype)),
            DType::Idx => ParamData::Idx(cast(bytes, dtype)),
            DType::I128 => return Err(TirError::new(TirErrorKind::Operand, "a param is never i128")),
        })
    }

    /// An owned copy of a typed buffer.
    pub fn from_buf(b: Buf) -> TirResult<ParamData<'static>> {
        Ok(match b {
            Buf::I8(v) => ParamData::I8(Cow::Owned(v)),
            Buf::I16(v) => ParamData::I16(Cow::Owned(v)),
            Buf::I32(v) => ParamData::I32(Cow::Owned(v)),
            Buf::I64(v) => ParamData::I64(Cow::Owned(v)),
            Buf::Idx(v) => ParamData::Idx(Cow::Owned(v)),
            Buf::I128(_) => return Err(TirError::new(TirErrorKind::Operand, "a param is never i128")),
        })
    }
}

/// The param instances a plan reads: `[param][layer or 0]`.
#[derive(Clone, Debug)]
pub struct TirParams<'a> {
    table: Vec<Vec<Option<ParamData<'a>>>>,
    per_layer: Vec<bool>,
}

impl<'a> TirParams<'a> {
    pub fn new(plan: &TirPlan) -> Self {
        let layers = plan.program.schedule.layers.len().max(1);
        TirParams {
            table: plan.program.params.iter().map(|d| vec![None; if d.per_layer { layers } else { 1 }]).collect(),
            per_layer: plan.program.params.iter().map(|d| d.per_layer).collect(),
        }
    }

    /// Bind one instance, checking its dtype and element count against the declaration.
    pub fn insert(&mut self, plan: &TirPlan, j: u16, layer: Option<u16>, data: ParamData<'a>) -> TirResult<()> {
        let d = plan.program.params.get(j as usize).ok_or_else(|| TirError::new(TirErrorKind::Operand, format!("no param {j}")))?;
        let n: u64 = d.shape.iter().map(|x| *x as u64).product();
        if data.dtype() != d.dtype || data.len() as u64 != n {
            return Err(TirError::new(
                TirErrorKind::Operand,
                format!("param {}: {} × {} bound, declared {} × {n}", d.name, data.len(), data.dtype().name(), d.dtype.name()),
            ));
        }
        let key = if d.per_layer {
            layer.ok_or_else(|| TirError::new(TirErrorKind::Operand, "a per-layer param needs its layer"))? as usize
        } else {
            0
        };
        let row = &mut self.table[j as usize];
        let slot = row.get_mut(key).ok_or_else(|| TirError::new(TirErrorKind::Operand, "no such layer"))?;
        *slot = Some(data);
        Ok(())
    }

    /// Owned params from the reference's in-memory map (tests, tools). Every instance the plan
    /// reads must be present and a tensor of its declaration — exactly what the reference checks
    /// at each use (`Missing` / `Operand`).
    pub fn from_map(plan: &TirPlan, map: &MapParams) -> TirResult<TirParams<'static>> {
        let mut p = TirParams::new(plan);
        for &(j, layer) in &plan.param_instances {
            let d = &plan.program.params[j as usize];
            let t = map
                .tensors
                .get(&(j, layer))
                .ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("param {} (layer {layer:?})", d.name)))?;
            let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            if t.dtype != d.dtype
                || t.shape != shape
                || t.data.len() != shape.iter().product::<usize>()
                || t.data.iter().any(|v| !d.dtype.contains(*v))
            {
                return Err(TirError::new(TirErrorKind::Operand, format!("param {} is not a tensor of its declaration", d.name)));
            }
            let data = ParamData::from_buf(Buf::from_i128s(d.dtype, &t.data))?;
            let key = if d.per_layer { layer.map(|l| l as usize).unwrap_or(0) } else { 0 };
            p.table[j as usize][key] = Some(data);
        }
        Ok(p)
    }

    /// As [`Self::from_map`], but an absent instance stays unbound — the reader fails `Missing`
    /// only if it is read (cone evaluation reads a closure, not the whole program).
    pub fn from_map_lenient(plan: &TirPlan, map: &MapParams) -> TirResult<TirParams<'static>> {
        let mut p = TirParams::new(plan);
        for &(j, layer) in &plan.param_instances {
            let d = &plan.program.params[j as usize];
            let Some(t) = map.tensors.get(&(j, layer)) else { continue };
            let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            if t.dtype != d.dtype
                || t.shape != shape
                || t.data.len() != shape.iter().product::<usize>()
                || t.data.iter().any(|v| !d.dtype.contains(*v))
            {
                return Err(TirError::new(TirErrorKind::Operand, format!("param {} is not a tensor of its declaration", d.name)));
            }
            let key = if d.per_layer { layer.map(|l| l as usize).unwrap_or(0) } else { 0 };
            p.table[j as usize][key] = Some(ParamData::from_buf(Buf::from_i128s(d.dtype, &t.data))?);
        }
        Ok(p)
    }

    /// Every instance the plan reads is bound (else `Missing`, as the reference at first use).
    pub fn check_complete(&self, plan: &TirPlan) -> TirResult<()> {
        for &(j, layer) in &plan.param_instances {
            if self.get(j, layer).is_none() {
                return Err(TirError::new(
                    TirErrorKind::Missing,
                    format!("param {} (layer {layer:?})", plan.program.params[j as usize].name),
                ));
            }
        }
        Ok(())
    }

    /// The instance an occurrence at `layer` reads.
    #[inline]
    pub fn get(&self, j: u16, layer: Option<u16>) -> Option<Slice<'_>> {
        let key = if self.per_layer[j as usize] { layer.map(|l| l as usize).unwrap_or(0) } else { 0 };
        self.table.get(j as usize)?.get(key)?.as_ref().map(|d| d.slice())
    }
}
