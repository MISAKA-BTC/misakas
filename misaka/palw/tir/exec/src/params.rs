//! Params: typed, borrowed, zero-copy — or served by rows.
//!
//! A param instance is a slice of its declared native type. From a mapped artifact it is the
//! mapping itself (little-endian elements reinterpreted in place when the target is little-endian
//! and the bytes are aligned); nothing is copied or widened. Every bit pattern of `i8`…`i64` and
//! `u32` is a value of its dtype (spec 04b §3.5: params take their dtype's full range), so a
//! borrowed tensor needs only its length checked.
//!
//! **Under a runtime residency** (ADR-0112 for IR classes, [`crate::tiers`], [`crate::rows`]) an
//! instance the residency routes or gathers is not bound at all: a [`TirRowSourceV1`] serves the rows
//! a `Gather` names, and every other instance is bound as above (from memory the residency owns).
//! A dense read of a row-served instance — which the tiers never plan — is answered with the whole
//! instance, read once and counted ([`TirParams::get`]), so a value never depends on the tiers.

use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

use misaka_palw_tir::interval::Interval;
use misaka_palw_tir::{DType, MapParams, TirError, TirErrorKind, TirResult};

use crate::elem::{Buf, Elem, Slice};
use crate::plan::TirPlan;
use crate::rows::TirRowSourceV1;

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
#[derive(Clone)]
pub struct TirParams<'a> {
    table: Vec<Vec<Option<ParamData<'a>>>>,
    per_layer: Vec<bool>,
    /// The instances a residency serves by rows, when one does.
    rows: Option<TirServedV1>,
    /// `[min, max]` of each instance, computed on first use: an executor is built per job, and
    /// every build read every weight again before this memo (the whole artifact, per run).
    ranges: Vec<Vec<OnceLock<Option<Interval>>>>,
}

/// The row-served half of a [`TirParams`].
#[derive(Clone)]
struct TirServedV1 {
    source: Arc<dyn TirRowSourceV1>,
    /// `[param][layer or 0]`: served by rows.
    served: Vec<Vec<bool>>,
    /// `[param][layer or 0]`: the whole instance, read on a dense read (the counted fallback).
    whole: Vec<Vec<OnceLock<Option<ParamData<'static>>>>>,
}

impl std::fmt::Debug for TirParams<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bound = self.table.iter().flatten().filter(|d| d.is_some()).count();
        let served = self.rows.as_ref().map_or(0, |r| r.served.iter().flatten().filter(|s| **s).count());
        write!(f, "TirParams({bound} instances bound, {served} served by rows)")
    }
}

impl<'a> TirParams<'a> {
    pub fn new(plan: &TirPlan) -> Self {
        let layers = plan.program.schedule.layers.len().max(1);
        let width = |per_layer: bool| if per_layer { layers } else { 1 };
        TirParams {
            table: plan.program.params.iter().map(|d| vec![None; width(d.per_layer)]).collect(),
            per_layer: plan.program.params.iter().map(|d| d.per_layer).collect(),
            rows: None,
            ranges: plan.program.params.iter().map(|d| (0..width(d.per_layer)).map(|_| OnceLock::new()).collect()).collect(),
        }
    }

    fn key(&self, j: u16, layer: Option<u16>) -> usize {
        if self.per_layer.get(j as usize).copied().unwrap_or(false) { layer.map(|l| l as usize).unwrap_or(0) } else { 0 }
    }

    /// The instance an occurrence at `layer` reads of param `j`: `(j, layer)` for a per-layer param,
    /// `(j, None)` for a global one — the key a row source serves it under.
    #[inline]
    pub fn instance_layer(&self, j: u16, layer: Option<u16>) -> Option<u16> {
        if self.per_layer.get(j as usize).copied().unwrap_or(false) { layer } else { None }
    }

    /// **Serve instances by rows from `source`** (a runtime residency): `served(j, layer)` names
    /// the instances it serves; each must be one the plan reads and must not be bound. A `Gather`
    /// of a served instance asks the source for its rows; anything else is answered whole, once,
    /// and counted by the source ([`TirRowSourceV1::read_whole`]).
    pub fn serve_rows(
        &mut self,
        plan: &TirPlan,
        source: Arc<dyn TirRowSourceV1>,
        served: &dyn Fn(u16, Option<u16>) -> bool,
    ) -> TirResult<()> {
        let mut flags: Vec<Vec<bool>> = self.table.iter().map(|row| vec![false; row.len()]).collect();
        for &(j, layer) in &plan.param_instances {
            if !served(j, layer) {
                continue;
            }
            let key = self.key(j, layer);
            if self.table[j as usize][key].is_some() {
                return Err(TirError::new(TirErrorKind::Operand, format!("param {j} (layer {layer:?}) is bound and served")));
            }
            if source.row_shape(j, layer).is_none() {
                return Err(TirError::new(
                    TirErrorKind::Missing,
                    format!("the row source does not serve param {j} (layer {layer:?})"),
                ));
            }
            flags[j as usize][key] = true;
            self.ranges[j as usize][key] = OnceLock::new();
        }
        let whole = self.table.iter().map(|row| (0..row.len()).map(|_| OnceLock::new()).collect()).collect();
        self.rows = Some(TirServedV1 { source, served: flags, whole });
        Ok(())
    }

    /// The row source, when a residency serves some instance by rows.
    pub fn row_source(&self) -> Option<&Arc<dyn TirRowSourceV1>> {
        self.rows.as_ref().map(|r| &r.source)
    }

    /// Is instance `(j, layer)` served by rows (not bound)?
    #[inline]
    pub fn serves_rows(&self, j: u16, layer: Option<u16>) -> bool {
        let Some(rows) = &self.rows else { return false };
        let key = self.key(j, layer);
        rows.served.get(j as usize).and_then(|r| r.get(key)).copied().unwrap_or(false)
    }

    /// Is instance `(j, layer)` bound or served — readable at all?
    pub fn has(&self, j: u16, layer: Option<u16>) -> bool {
        let key = self.key(j, layer);
        self.table.get(j as usize).and_then(|r| r.get(key)).is_some_and(|d| d.is_some()) || self.serves_rows(j, layer)
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
        self.ranges[j as usize][key] = OnceLock::new();
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

    /// Every instance the plan reads is bound or served (else `Missing`, as the reference at first
    /// use).
    pub fn check_complete(&self, plan: &TirPlan) -> TirResult<()> {
        for &(j, layer) in &plan.param_instances {
            if !self.has(j, layer) {
                return Err(TirError::new(
                    TirErrorKind::Missing,
                    format!("param {} (layer {layer:?})", plan.program.params[j as usize].name),
                ));
            }
        }
        Ok(())
    }

    /// `[min, max]` of one instance's elements (the interval [`TirPlan::refine`] plans with): a
    /// bound instance's computed once and kept, a served one's as its source knows it (a residency
    /// computes it in the pass it opens with). Never a reason to read a served instance whole.
    pub fn range(&self, j: u16, layer: Option<u16>) -> Option<Interval> {
        let key = self.key(j, layer);
        let memo = self.ranges.get(j as usize)?.get(key)?;
        *memo.get_or_init(|| {
            if let Some(d) = self.table.get(j as usize).and_then(|r| r.get(key)).and_then(|d| d.as_ref()) {
                return slice_range(d.slice());
            }
            if self.serves_rows(j, layer) {
                return self.rows.as_ref().and_then(|r| r.source.range(j, self.instance_layer(j, layer)));
            }
            None
        })
    }

    /// The instance an occurrence at `layer` reads. A row-served instance is answered whole — read
    /// once through its source, kept, and counted there: the tiers plan no dense read of one, so a
    /// node path never pays it, and a path the tiers did not foresee (a sink that asks for every
    /// node's value) still computes the reference's values.
    #[inline]
    pub fn get(&self, j: u16, layer: Option<u16>) -> Option<Slice<'_>> {
        let key = self.key(j, layer);
        if let Some(d) = self.table.get(j as usize)?.get(key)?.as_ref() {
            return Some(d.slice());
        }
        let rows = self.rows.as_ref()?;
        if !rows.served.get(j as usize)?.get(key).copied().unwrap_or(false) {
            return None;
        }
        let instance = self.instance_layer(j, layer);
        rows.whole[j as usize][key].get_or_init(|| rows.source.read_whole(j, instance).ok()).as_ref().map(|d| d.slice())
    }

    /// **Owned params from the reference's map, split by `tiers`**: the instances the tiers route
    /// or gather served by an in-memory row source ([`crate::rows::TirRowsInMemoryV1`]), the rest
    /// bound — what a residency does, without a file. Checked exactly as [`Self::from_map`].
    pub fn from_map_served(
        plan: &TirPlan,
        map: &MapParams,
        tiers: &crate::tiers::TirTiersV1,
    ) -> TirResult<(TirParams<'static>, Arc<crate::rows::TirRowsInMemoryV1>)> {
        let mut all = TirParams::from_map(plan, map)?;
        let mut held = std::collections::BTreeMap::new();
        for &(j, layer) in &plan.param_instances {
            let Some(t) = tiers.params.get(j as usize) else { continue };
            if !t.tier.is_rows() {
                continue;
            }
            let key = all.key(j, layer);
            let data = all.table[j as usize][key].take().expect("from_map bound every instance");
            held.insert((j, layer), (data.slice().to_buf(), t.rows, t.unit));
        }
        let source = Arc::new(crate::rows::TirRowsInMemoryV1::new(held));
        let keys: std::collections::BTreeSet<(u16, Option<u16>)> = source.instances().collect();
        all.serve_rows(plan, source.clone(), &|j, l| keys.contains(&(j, l)))?;
        Ok((all, source))
    }
}

/// `[min, max]` of a slice's elements (in parallel over large ones).
pub(crate) fn slice_range(s: Slice<'_>) -> Option<Interval> {
    use rayon::prelude::*;
    fn minmax<T: Elem>(v: &[T]) -> Option<(T, T)> {
        let fold = |c: &[T]| c.iter().fold((c[0], c[0]), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
        if v.is_empty() {
            return None;
        }
        if v.len() < 1 << 20 {
            return Some(fold(v));
        }
        v.par_chunks(1 << 20).map(fold).reduce_with(|a, b| (a.0.min(b.0), a.1.max(b.1)))
    }
    crate::with_slice!(s, v => minmax(v).map(|(lo, hi)| Interval::new(lo.to_i128(), hi.to_i128())))
}
