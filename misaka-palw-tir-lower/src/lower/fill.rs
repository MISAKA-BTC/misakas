//! **Materialisation**: the integer params of a lowered program, per occurrence, from the float
//! checkpoint and the calibration statistics.
//!
//! One pass over the occurrences (`pre`, the layers, `post`), each with only its own float params
//! loaded ([`OccParams`]): every TIR param the occurrence's block references is filled once —
//! per layer for per-layer params, once for globals. Scales resolve here ([`FillCtx::scale`]):
//! a site's scale is its calibrated absmax in THIS occurrence times the policy's headroom over the
//! code range; the residual scale is one number for the program, sized on every value carried
//! at it (`Lowered::resid_sites`).

use super::qlinear::QInts;
use super::{Base, Lowered, ScaleKey, occurrences};
use crate::prequant::QLayout;
use crate::error::{LowerError, Result};
use crate::float_ref::stream::OccParams;
use crate::float_ref::{ParamStore, SiteStat};
use crate::hl::HlProgram;
use crate::quant::{CODE16_MAX, CODE32_MAX, QuantPolicy, RowCodes, RowCodes16, code_scale, quantize_rows, quantize_rows16};
use crate::weights::Tensor;
use misaka_palw_tir as tir;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use tir::DType;

/// An integer tensor in its own dtype's width (a param of the artifact).
#[derive(Clone, Debug, PartialEq)]
pub struct IntTensor {
    pub dtype: DType,
    pub shape: Vec<usize>,
    pub data: IntData,
}

#[derive(Clone, Debug, PartialEq)]
pub enum IntData {
    I8(Vec<i8>),
    I16(Vec<i16>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    /// `idx` (unsigned 32-bit) — selection indices.
    Idx(Vec<u32>),
}

impl IntTensor {
    pub fn idx(shape: Vec<usize>, v: Vec<u32>) -> Self {
        Self { dtype: DType::Idx, shape, data: IntData::Idx(v) }
    }
    pub fn i8(shape: Vec<usize>, v: Vec<i8>) -> Self {
        Self { dtype: DType::I8, shape, data: IntData::I8(v) }
    }
    pub fn i16(shape: Vec<usize>, v: Vec<i16>) -> Self {
        Self { dtype: DType::I16, shape, data: IntData::I16(v) }
    }
    pub fn i32(shape: Vec<usize>, v: Vec<i32>) -> Self {
        Self { dtype: DType::I32, shape, data: IntData::I32(v) }
    }
    pub fn i64(shape: Vec<usize>, v: Vec<i64>) -> Self {
        Self { dtype: DType::I64, shape, data: IntData::I64(v) }
    }
    pub fn len(&self) -> usize {
        match &self.data {
            IntData::I8(v) => v.len(),
            IntData::I16(v) => v.len(),
            IntData::I32(v) => v.len(),
            IntData::I64(v) => v.len(),
            IntData::Idx(v) => v.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Element `i` as an integer.
    pub fn get(&self, i: usize) -> i64 {
        match &self.data {
            IntData::I8(v) => v[i] as i64,
            IntData::I16(v) => v[i] as i64,
            IntData::I32(v) => v[i] as i64,
            IntData::I64(v) => v[i],
            IntData::Idx(v) => v[i] as i64,
        }
    }
    /// The evaluator's tensor (every element widened to `i128`).
    pub fn to_tir(&self) -> tir::Tensor {
        let data: Vec<i128> = match &self.data {
            IntData::I8(v) => v.iter().map(|x| *x as i128).collect(),
            IntData::I16(v) => v.iter().map(|x| *x as i128).collect(),
            IntData::I32(v) => v.iter().map(|x| *x as i128).collect(),
            IntData::I64(v) => v.iter().map(|x| *x as i128).collect(),
            IntData::Idx(v) => v.iter().map(|x| *x as i128).collect(),
        };
        tir::Tensor { dtype: self.dtype, shape: self.shape.clone(), data }
    }
    /// Little-endian two's complement, `width(dtype)` bytes per element.
    pub fn le_bytes(&self) -> Vec<u8> {
        match &self.data {
            IntData::I8(v) => v.iter().map(|x| *x as u8).collect(),
            IntData::I16(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            IntData::I32(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            IntData::I64(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            IntData::Idx(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
        }
    }
    /// The inverse of [`Self::le_bytes`].
    pub fn from_le_bytes(dtype: DType, shape: Vec<usize>, b: &[u8]) -> Result<Self> {
        let n: usize = shape.iter().product();
        let w = dtype.width();
        if b.len() != n * w {
            return Err(LowerError::bad(format!("{} bytes for {n} elements of {}", b.len(), dtype.name())));
        }
        Ok(match dtype {
            DType::I8 => Self::i8(shape, b.iter().map(|x| *x as i8).collect()),
            DType::I16 => Self::i16(shape, b.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()),
            DType::I32 => Self::i32(shape, b.chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().expect("4"))).collect()),
            DType::I64 => Self::i64(shape, b.chunks_exact(8).map(|c| i64::from_le_bytes(c.try_into().expect("8"))).collect()),
            DType::Idx => Self::idx(shape, b.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().expect("4"))).collect()),
            other => return Err(LowerError::bad(format!("no artifact tensors of {}", other.name()))),
        })
    }
}

/// The integer params of one artifact, by `(param index, layer)` (`None` for a global).
#[derive(Clone, Debug, Default)]
pub struct IntParams {
    pub tensors: BTreeMap<(u16, Option<u16>), IntTensor>,
}

impl tir::ParamSource for IntParams {
    fn param(&self, index: u16, layer: Option<u16>) -> Option<tir::Tensor> {
        self.tensors.get(&(index, layer)).map(IntTensor::to_tir)
    }
}

impl IntParams {
    pub fn bytes(&self) -> usize {
        self.tensors.values().map(|t| t.len() * t.dtype.width()).sum()
    }
}

/// What a fill sees: this occurrence's float params and statistics, and the program-wide scales.
pub struct FillCtx<'a> {
    pub hl: &'a HlProgram,
    pub params: &'a ParamStore,
    pub layer: Option<usize>,
    /// The statistics prefix of the occurrence (`pre.`, `L3.`, `post.`).
    pub prefix: &'a str,
    pub stats: &'a BTreeMap<String, SiteStat>,
    /// The residual stream's scale.
    pub resid: f64,
    pub policy: &'a QuantPolicy,
    memo: Mutex<BTreeMap<u32, Arc<RowCodes>>>,
    memo16: Mutex<BTreeMap<u32, Arc<RowCodes16>>>,
    split_memo: Mutex<BTreeMap<String, Arc<SplitCodes>>>,
    qmemo: Mutex<BTreeMap<String, Arc<QInts>>>,
    qstack_memo: Mutex<BTreeMap<u32, Arc<Vec<QInts>>>>,
}

impl<'a> FillCtx<'a> {
    /// A context that resolves scales only (decoding committed values; `params` may be empty).
    #[allow(clippy::too_many_arguments)]
    pub fn for_scales(
        hl: &'a HlProgram,
        params: &'a ParamStore,
        layer: Option<usize>,
        prefix: &'a str,
        stats: &'a BTreeMap<String, SiteStat>,
        resid: f64,
        policy: &'a QuantPolicy,
    ) -> Self {
        FillCtx {
            hl,
            params,
            layer,
            prefix,
            stats,
            resid,
            policy,
            memo: Mutex::new(BTreeMap::new()),
            memo16: Mutex::new(BTreeMap::new()),
            split_memo: Mutex::new(BTreeMap::new()),
            qmemo: Mutex::new(BTreeMap::new()),
            qstack_memo: Mutex::new(BTreeMap::new()),
        }
    }

    /// A pre-quantised projection's integers in `layout`, for the input key `kx` (its outlier
    /// columns route around the codes), computed once per occurrence.
    pub fn qints(&self, p: u32, layout: QLayout, kx: &ScaleKey) -> Result<Arc<QInts>> {
        let outl = self.outliers(kx)?;
        let mk = format!("{p}:{outl:?}");
        if let Some(r) = self.qmemo.lock().expect("memo").get(&mk) {
            return Ok(r.clone());
        }
        let qs = self
            .params
            .get_q(p, self.layer)
            .ok_or_else(|| LowerError::eval(format!("param {p} is lowered as pre-quantised, but the checkpoint gave no integers")))?;
        if qs.len() != 1 {
            return Err(LowerError::eval(format!("internal: param {p} is a stack of {} quantised experts", qs.len())));
        }
        let q = &qs[0];
        let ratio: Vec<f64> = if outl.is_empty() {
            Vec::new()
        } else {
            let (tv, tn) = (self.scale_vec(kx, q.inp)?, self.scale(kx)?);
            outl.iter().map(|c| tv[*c] / tn).collect()
        };
        let qi = Arc::new(super::qlinear::build(q, layout, &outl, &ratio)?);
        self.qmemo.lock().expect("memo").insert(mk, qi.clone());
        Ok(qi)
    }

    /// A stack of quantised experts' integers in `layout` (no outlier split: an expert's input is
    /// never split), one [`QInts`] per expert, computed once per occurrence.
    pub fn qints_stack(&self, p: u32, layout: QLayout) -> Result<Arc<Vec<QInts>>> {
        if let Some(r) = self.qstack_memo.lock().expect("memo").get(&p) {
            return Ok(r.clone());
        }
        let qs = self
            .params
            .get_q(p, self.layer)
            .ok_or_else(|| LowerError::eval(format!("param {p} is lowered as pre-quantised experts, but the checkpoint gave no integers")))?;
        let v: Vec<QInts> = qs.iter().map(|q| super::qlinear::build(q, layout, &[], &[])).collect::<Result<_>>()?;
        let v = Arc::new(v);
        self.qstack_memo.lock().expect("memo").insert(p, v.clone());
        Ok(v)
    }

    /// Scales of this occurrence's quantised projections that are not exact at their row's unit.
    pub fn quant_inexact(&self) -> usize {
        self.qmemo.lock().expect("memo").values().map(|q| q.inexact).sum::<usize>()
            + self.qstack_memo.lock().expect("memo").values().flat_map(|v| v.iter()).map(|q| q.inexact).sum::<usize>()
    }

    /// A float param of this occurrence.
    pub fn f(&self, p: u32) -> Result<&Tensor> {
        self.params.get(p, self.layer)
    }

    /// The calibrated absmax of a site in this occurrence.
    pub fn absmax(&self, site: &str) -> Result<f64> {
        let k = format!("{}{site}", self.prefix);
        self.stats.get(&k).map(|s| s.absmax).ok_or_else(|| LowerError::eval(format!("calibration has no statistics for `{k}`")))
    }

    /// The float value of one integer unit under `key`, in this occurrence. For a split key this is
    /// the scale every non-outlier channel shares.
    pub fn scale(&self, key: &ScaleKey) -> Result<f64> {
        let base = match &key.base {
            Base::Resid => self.resid,
            Base::Q24 => 1.0 / (1u64 << 24) as f64,
            Base::Fixed(v) => *v,
            Base::At { prefix, base } => {
                let at = FillCtx::for_scales(self.hl, self.params, None, prefix, self.stats, self.resid, self.policy);
                at.scale(&ScaleKey { base: (**base).clone(), factor: 1.0 })?
            }
            Base::Pow2Site { names } => {
                let mut a = 0f64;
                for n in names {
                    a = a.max(self.absmax(n)?);
                }
                pow2_unit(code_scale(a, CODE32_MAX, self.policy.headroom32))
            }
            Base::Site { names, wide, split } => {
                let a = if *split == 0 {
                    let mut a = 0f64;
                    for n in names {
                        a = a.max(self.absmax(n)?);
                    }
                    a
                } else {
                    let (amax, out) = self.split_of(names, *split)?;
                    amax.iter().enumerate().filter(|(c, _)| out.binary_search(c).is_err()).fold(0f64, |m, (_, v)| m.max(*v))
                };
                if *wide {
                    code_scale(a, CODE32_MAX, self.policy.headroom32)
                } else {
                    code_scale(a, CODE16_MAX, self.policy.headroom16)
                }
            }
        };
        Ok(base * key.factor)
    }

    /// Per-channel scales of an `n`-channel value under `key` (all equal unless the key is split).
    pub fn scale_vec(&self, key: &ScaleKey, n: usize) -> Result<Vec<f64>> {
        let common = self.scale(key)?;
        let mut v = vec![common; n];
        if let Base::Site { names, wide: false, split } = &key.base
            && *split > 0
        {
            let (amax, out) = self.split_of(names, *split)?;
            if amax.len() != n {
                return Err(LowerError::eval(format!("split scales: {} channels calibrated, {n} used", amax.len())));
            }
            for c in out {
                v[c] = code_scale(amax[c], CODE16_MAX, self.policy.headroom16).max(common / key.factor) * key.factor;
            }
        }
        Ok(v)
    }

    /// The outlier channels of a split key, ascending.
    pub fn outliers(&self, key: &ScaleKey) -> Result<Vec<usize>> {
        match &key.base {
            Base::Site { names, split, .. } if *split > 0 => Ok(self.split_of(names, *split)?.1),
            _ => Ok(Vec::new()),
        }
    }

    /// Per-channel absmax over `names` and the `k` largest channels (lowest index on ties),
    /// ascending.
    fn split_of(&self, names: &[String], k: usize) -> Result<(Vec<f64>, Vec<usize>)> {
        let mut amax: Vec<f64> = Vec::new();
        for n in names {
            let key = format!("{}{n}", self.prefix);
            let st = self.stats.get(&key).ok_or_else(|| LowerError::eval(format!("calibration has no statistics for `{key}`")))?;
            if st.ragged || st.chan_absmax.is_empty() {
                return Err(LowerError::eval(format!("`{key}` has no per-channel statistics")));
            }
            if amax.is_empty() {
                amax = st.chan_absmax.iter().map(|x| *x as f64).collect();
            } else if amax.len() == st.chan_absmax.len() {
                for (a, b) in amax.iter_mut().zip(&st.chan_absmax) {
                    *a = a.max(*b as f64);
                }
            } else {
                return Err(LowerError::eval(format!("`{key}`: channel counts differ across the sites of one scale")));
            }
        }
        let mut order: Vec<usize> = (0..amax.len()).collect();
        order.sort_by(|a, b| amax[*b].partial_cmp(&amax[*a]).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(b)));
        let mut out: Vec<usize> = order.into_iter().take(k.min(amax.len())).collect();
        out.sort_unstable();
        Ok((amax, out))
    }

    /// Per-row `i8` codes of an HL `[out, in]` param, computed once per occurrence.
    pub fn rows(&self, p: u32) -> Result<Arc<RowCodes>> {
        if let Some(r) = self.memo.lock().expect("memo").get(&p) {
            return Ok(r.clone());
        }
        let t = self.f(p)?;
        if t.shape.len() < 2 {
            return Err(LowerError::eval(format!("param {p}: row codes of a {:?} tensor", t.shape)));
        }
        // Rows are the last axis; leading axes (experts) stack rows.
        let cols = *t.shape.last().expect("rank ≥ 2");
        let rc = Arc::new(quantize_rows(&t.data, t.data.len() / cols, cols, None));
        self.memo.lock().expect("memo").insert(p, rc.clone());
        Ok(rc)
    }

    /// Per-row `i16` codes of a gathered table (an embedding, and a head tied to it).
    pub fn rows16(&self, p: u32) -> Result<Arc<RowCodes16>> {
        if let Some(r) = self.memo16.lock().expect("memo").get(&p) {
            return Ok(r.clone());
        }
        let t = self.f(p)?;
        if t.shape.len() < 2 {
            return Err(LowerError::eval(format!("param {p}: row codes of a {:?} tensor", t.shape)));
        }
        let cols = *t.shape.last().expect("rank ≥ 2");
        let rc = Arc::new(quantize_rows16(&t.data, t.data.len() / cols, cols));
        self.memo16.lock().expect("memo").insert(p, rc.clone());
        Ok(rc)
    }

    /// The split form of an HL `[out, in]` weight read by a split input `kx`: the main codes (the
    /// outlier columns zeroed, per-row scale over the rest), and the outlier columns in `i32`
    /// fixed point relative to each row's main unit, `wo[o][j] = W[o, c_j] · t_{c_j} /
    /// (sw[o] · t_main) · 2^f[o]`, with `f[o] ≤ f_max` as large as keeps them inside `±2^30`.
    pub fn split_rows(&self, p: u32, kx: &ScaleKey, f_max: i32) -> Result<Arc<SplitCodes>> {
        let out = self.outliers(kx)?;
        let mk = format!("{p}:{out:?}:{f_max}");
        if let Some(r) = self.split_memo.lock().expect("memo").get(&mk) {
            return Ok(r.clone());
        }
        let t = self.f(p)?;
        let (rows, cols) = (t.shape[0], t.shape[1]);
        let mut masked = t.data.clone();
        for r in 0..rows {
            for c in &out {
                masked[r * cols + c] = 0.0;
            }
        }
        let main = quantize_rows(&masked, rows, cols, None);
        let tv = self.scale_vec(kx, cols)?;
        let tn = self.scale(kx)?;
        let mut wo = Vec::with_capacity(rows * out.len());
        let mut f = Vec::with_capacity(rows);
        for r in 0..rows {
            let vals: Vec<f64> = out.iter().map(|c| t.data[r * cols + c] as f64 * tv[*c] / (main.scales[r] * tn)).collect();
            let mx = vals.iter().fold(0f64, |m, v| m.max(v.abs()));
            let fr = if mx > 0.0 { (30 - mx.log2().ceil() as i32).clamp(0, f_max) } else { f_max };
            f.push(fr as i8);
            wo.extend(
                vals.iter().map(|v| (v * 2f64.powi(fr)).round().clamp(-(1i64 << 31) as f64 + 1.0, (1i64 << 31) as f64 - 1.0) as i32),
            );
        }
        let sc = Arc::new(SplitCodes { main, outliers: out, wo, f });
        self.split_memo.lock().expect("memo").insert(mk, sc.clone());
        Ok(sc)
    }
}

/// A weight in split form (see [`FillCtx::split_rows`]).
#[derive(Clone, Debug)]
pub struct SplitCodes {
    pub main: RowCodes,
    pub outliers: Vec<usize>,
    /// `[rows, outliers]`, row-major.
    pub wo: Vec<i32>,
    pub f: Vec<i8>,
}

/// The artifact of a lowered program, and the two scales a reader of its logits needs.
pub struct Materialised {
    pub params: IntParams,
    /// Float value of one unit of the residual stream.
    pub resid_scale: f64,
    /// Float value of one unit of the logits.
    pub logits_scale: f64,
    /// Pre-quantised projections' per-group scales that are not exact at their row's unit (0 when
    /// every stored scale is represented exactly, `lower::qlinear`).
    pub quant_inexact: usize,
}

/// Params each TIR block references.
fn params_of_blocks(p: &tir::TirProgramV1) -> Vec<BTreeSet<u16>> {
    p.blocks
        .iter()
        .map(|b| {
            b.nodes
                .iter()
                .flat_map(|n| n.inputs.iter())
                .filter_map(|r| if let tir::Ref::Param(j) = r { Some(*j) } else { None })
                .collect()
        })
        .collect()
}

/// Fill every param of `lw` for every occurrence. `progress(done, total)` after each occurrence.
pub fn materialise(
    lw: &Lowered,
    hl: &HlProgram,
    loader: &dyn OccParams,
    stats: &BTreeMap<String, SiteStat>,
    policy: &QuantPolicy,
    progress: &dyn Fn(usize, usize),
) -> Result<Materialised> {
    let mut amax_r = 0f64;
    let mut seen = false;
    for (k, factor) in &lw.resid_sites {
        if let Some(s) = stats.get(k) {
            amax_r = amax_r.max(s.absmax / factor.abs());
            seen = true;
        }
    }
    if !seen {
        return Err(LowerError::eval("calibration has no statistics for the residual stream"));
    }
    let resid = code_scale(amax_r, CODE32_MAX, policy.headroom_resid);
    let used = params_of_blocks(&lw.program);
    let mut out = IntParams::default();
    let mut logits_scale = None;
    let mut quant_inexact = 0usize;
    let occs = occurrences(hl);
    let total = occs.len();
    for (oi, (hbk, layer, prefix)) in occs.into_iter().enumerate() {
        let tb = lw.block_map[hbk] as usize;
        let store = loader.load(hbk, layer)?;
        let ctx = FillCtx {
            hl,
            params: &store,
            layer,
            prefix: &prefix,
            stats,
            resid,
            policy,
            memo: Mutex::new(BTreeMap::new()),
            memo16: Mutex::new(BTreeMap::new()),
            split_memo: Mutex::new(BTreeMap::new()),
            qmemo: Mutex::new(BTreeMap::new()),
            qstack_memo: Mutex::new(BTreeMap::new()),
        };
        for &pi in &used[tb] {
            let d = &lw.program.params[pi as usize];
            let key = (pi, if d.per_layer { layer.map(|l| l as u16) } else { None });
            if out.tensors.contains_key(&key) {
                continue;
            }
            let t = (lw.fills[pi as usize])(&ctx).map_err(|e| LowerError::eval(format!("filling `{}` at {prefix}: {e}", d.name)))?;
            let want: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            if t.dtype != d.dtype || t.shape != want || t.len() != want.iter().product::<usize>() {
                return Err(LowerError::eval(format!(
                    "internal: fill of `{}` gave {} {:?}, declared {} {want:?}",
                    d.name,
                    t.dtype.name(),
                    t.shape,
                    d.dtype.name()
                )));
            }
            out.tensors.insert(key, t);
        }
        if hbk == hl.post {
            logits_scale = Some(ctx.scale(&lw.logits_key)?);
        }
        quant_inexact += ctx.quant_inexact();
        progress(oi + 1, total);
    }
    Ok(Materialised { params: out, resid_scale: resid, logits_scale: logits_scale.expect("post runs"), quant_inexact })
}

/// The smallest power of two `≥ s`, as `2^−q` with `q` in `[0, 31]` (an `EmbeddingI32` output's
/// fixed point, RFC-0003 §I.3.3).
pub fn pow2_unit(s: f64) -> f64 {
    2f64.powi(output_q(s).map_or(0, |q| -(q as i32)))
}

/// The fractional bits `q` of [`pow2_unit`]`(s)`.
pub fn output_q(s: f64) -> Option<u8> {
    if s.is_nan() || s <= 0.0 || !s.is_finite() {
        return None;
    }
    Some((-s.log2().ceil()).clamp(0.0, 31.0) as u8)
}
