//! **A checkpoint served through a descriptor**: tensors stored packed appear, under the name a float
//! export would give them, as float tensors (`crate::quantfmt::virt`).
//!
//! OCP MXFP4 as Hugging Face stores it keeps `…experts.gate_up_proj` as `…gate_up_proj_blocks` and
//! `…gate_up_proj_scales`; the descriptor says how those become the `[E, in, out]` tensor, and this
//! source lists `…gate_up_proj` in their place. Every binding that reads the float export reads it
//! unchanged — fused experts, transposes, strided slices — because the served tensor is the float
//! export's. A row range of it is evaluated without decoding the rest; the packed role tensors of the
//! module in use are kept in memory (one module at a time).
//!
//! The packed tensors are hidden: they are not listed, so a checkpoint's coverage (every tensor read,
//! or ignored by design) is reckoned over what the model uses.

use super::{Tensor, TensorMeta, TensorSource};
use crate::error::{LowerError, Result};
use crate::quantfmt::QuantFormat;
use crate::quantfmt::tensors::RoleTensor;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::{Arc, Mutex};

struct Module {
    /// The checkpoint's name of each role's tensor (`None`: an optional role the module lacks).
    role_names: Vec<Option<String>>,
    shape: Vec<usize>,
    /// The parameters this module decodes with: the configuration's, or the module's own where the configuration gives it some
    /// (MLX's per-module entries: a router kept at 8 bits in a 4-bit model).
    params: BTreeMap<String, i64>,
}

type RoleCache = Mutex<Option<(String, Arc<Vec<Option<RoleTensor>>>)>>;

pub struct DescribedSource {
    base: Box<dyn TensorSource + Sync>,
    format: Arc<QuantFormat>,
    params: BTreeMap<String, i64>,
    modules: BTreeMap<String, Module>,
    hidden: BTreeSet<String>,
    cache: RoleCache,
}

impl DescribedSource {
    /// Serve `base` through `format` (a `virtual` descriptor) with the parameters its configuration gave.
    /// Every module of the format the checkpoint holds is found and its descriptor's rules checked; a module
    /// that breaks them is an error naming it.
    pub fn new(base: Box<dyn TensorSource + Sync>, format: Arc<QuantFormat>, params: BTreeMap<String, i64>) -> Result<DescribedSource> {
        Self::new_with(base, format, params, &BTreeMap::new())
    }

    /// [`Self::new`] with per-module entries (`crate::prequant::QuantConfig::module_params`), by module name (`<module>`
    /// before the role suffixes): `Some(params)` — the module decodes with those instead of `params`; `None` — the
    /// configuration keeps the module in float. Either kind of entry that the checkpoint contradicts — parameters for a module
    /// it does not store in this format, or a module declared float that it stores packed — is an error naming it: a
    /// configuration and a checkpoint that disagree are not read past.
    pub fn new_with(
        base: Box<dyn TensorSource + Sync>,
        format: Arc<QuantFormat>,
        params: BTreeMap<String, i64>,
        overrides: &BTreeMap<String, Option<BTreeMap<String, i64>>>,
    ) -> Result<DescribedSource> {
        let v = format.as_virtual().ok_or_else(|| LowerError::bad(format!("quant format `{}` is not a virtual format", format.name())))?;
        let anchor = v.anchor_suffix().to_string();
        let mut modules = BTreeMap::new();
        let mut hidden = BTreeSet::new();
        let mut used = BTreeSet::new();
        for name in base.names() {
            let Some(module) = name.strip_suffix(anchor.as_str()) else { continue };
            let present = |suffix: &str| base.metadata(&format!("{module}{suffix}")).map(|m| (m.dtype, m.shape.len()));
            if !v.stores(present) {
                continue;
            }
            let mut role_names = Vec::new();
            let mut roles = Vec::new();
            for (_, suffix, _) in v.roles() {
                let n = format!("{module}{suffix}");
                match base.metadata(&n) {
                    Some(m) => {
                        // A small role tensor's bytes, for a shape that reads them (bitsandbytes' `quant_state`). A source that holds
                        // headers only (a census, a preflight before the download) has none: the tensor is then known by its header, and
                        // a shape that needs its bytes is refused by the descriptor, by name.
                        let data = if m.bytes <= 4096 { base.read_slice(&n, 0..m.bytes).unwrap_or_default() } else { Vec::new() };
                        roles.push(Some(RoleTensor { shape: m.shape, dtype: m.dtype, data }));
                        hidden.insert(n.clone());
                        role_names.push(Some(n));
                    }
                    None => {
                        roles.push(None);
                        role_names.push(None);
                    }
                }
            }
            let module_params = match overrides.get(module) {
                Some(Some(p)) => {
                    used.insert(module.to_string());
                    p.clone()
                }
                Some(None) => {
                    return Err(LowerError::weights(format!(
                        "the configuration keeps `{module}` in float, and the checkpoint stores it packed in the {} format",
                        format.name()
                    )));
                }
                None => params.clone(),
            };
            let shape = v.shape(&roles, &module_params).map_err(|e| LowerError::weights(format!("`{module}`: {e}")))?;
            modules.insert(v.served_name(module), Module { role_names, shape, params: module_params });
        }
        if let Some(m) = overrides.iter().find(|(m, p)| p.is_some() && !used.contains(*m)).map(|(m, _)| m) {
            return Err(LowerError::weights(format!(
                "the configuration gives `{m}` its own {} parameters, and the checkpoint holds no such module in that format",
                format.name()
            )));
        }
        Ok(DescribedSource { base, format, params, modules, hidden, cache: Mutex::new(None) })
    }

    /// The names of the tensors a module of this source is stored as, for a coverage report.
    pub fn module_names(&self) -> impl Iterator<Item = &str> {
        self.modules.keys().map(String::as_str)
    }

    fn roles_of(&self, module: &str) -> Result<Arc<Vec<Option<RoleTensor>>>> {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((m, r)) = g.as_ref()
            && m == module
        {
            return Ok(r.clone());
        }
        let m = self.modules.get(module).ok_or_else(|| LowerError::weights(format!("no module `{module}`")))?;
        let mut roles = Vec::with_capacity(m.role_names.len());
        for n in &m.role_names {
            roles.push(match n {
                Some(n) => {
                    let meta = self.base.metadata(n).ok_or_else(|| LowerError::weights(format!("no header for `{n}`")))?;
                    let data = self.base.read_slice(n, 0..meta.bytes)?;
                    Some(RoleTensor { shape: meta.shape, dtype: meta.dtype, data })
                }
                None => None,
            });
        }
        let r = Arc::new(roles);
        *g = Some((module.to_string(), r.clone()));
        Ok(r)
    }

    fn decode(&self, module: &str, range: Range<usize>) -> Result<Vec<f32>> {
        let roles = self.roles_of(module)?;
        let v = self.format.as_virtual().expect("checked at construction");
        let params = self.modules.get(module).map_or(&self.params, |m| &m.params);
        v.decode_range(&roles, params, range).map_err(|e| LowerError::weights(format!("`{module}`: {e}")))
    }
}

impl TensorSource for DescribedSource {
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        match self.modules.get(name) {
            Some(m) => Some(m.shape.clone()),
            None if self.hidden.contains(name) => None,
            None => self.base.shape(name),
        }
    }

    fn load(&self, name: &str) -> Result<Tensor> {
        match self.modules.get(name) {
            Some(m) => {
                let n: usize = m.shape.iter().product();
                Ok(Tensor::new(m.shape.clone(), self.decode(name, 0..n)?))
            }
            None if self.hidden.contains(name) => Err(LowerError::weights(format!("`{name}` is a packed tensor the descriptor serves as its module's float tensor"))),
            None => self.base.load(name),
        }
    }

    fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.base.names().into_iter().filter(|n| !self.hidden.contains(n)).collect();
        v.extend(self.modules.keys().cloned());
        v.sort();
        v.dedup();
        v
    }

    fn load_i32(&self, name: &str) -> Result<(Vec<usize>, Vec<i32>)> {
        self.base.load_i32(name)
    }

    fn load_qweight(&self, name: &str) -> Result<crate::prequant::QWeight> {
        self.base.load_qweight(name)
    }

    fn metadata(&self, name: &str) -> Option<TensorMeta> {
        match self.modules.get(name) {
            Some(m) => Some(TensorMeta { dtype: "F32".into(), shape: m.shape.clone(), bytes: m.shape.iter().product::<usize>() as u64 * 4 }),
            None if self.hidden.contains(name) => None,
            None => self.base.metadata(name),
        }
    }

    fn read_slice(&self, name: &str, range: Range<u64>) -> Result<Vec<u8>> {
        match self.modules.get(name) {
            Some(m) => {
                let total = m.shape.iter().product::<usize>() as u64 * 4;
                if range.start > range.end || range.end > total || !range.start.is_multiple_of(4) || !range.end.is_multiple_of(4) {
                    return Err(LowerError::weights(format!("`{name}`: bytes {range:?} of a served tensor of {total} (whole f32 elements only)")));
                }
                let vals = self.decode(name, (range.start / 4) as usize..(range.end / 4) as usize)?;
                Ok(vals.iter().flat_map(|x| x.to_le_bytes()).collect())
            }
            None if self.hidden.contains(name) => Err(LowerError::weights(format!("`{name}` is a packed tensor the descriptor serves as its module's float tensor"))),
            None => self.base.read_slice(name, range),
        }
    }

    fn load_rows(&self, name: &str, rows: Range<usize>) -> Result<Tensor> {
        match self.modules.get(name) {
            Some(m) => {
                let cols = m.shape.last().copied().unwrap_or(1);
                let n: usize = m.shape.iter().product();
                let total_rows = if cols == 0 { 0 } else { n / cols };
                if rows.start > rows.end || rows.end > total_rows {
                    return Err(LowerError::weights(format!("`{name}`: rows {rows:?} of {total_rows}")));
                }
                let data = self.decode(name, rows.start * cols..rows.end * cols)?;
                Ok(Tensor::new(vec![rows.len(), cols], data))
            }
            None if self.hidden.contains(name) => Err(LowerError::weights(format!("`{name}` is a packed tensor the descriptor serves as its module's float tensor"))),
            None => self.base.load_rows(name, rows),
        }
    }

    fn serves_row_ranges(&self) -> bool {
        self.base.serves_row_ranges()
    }
}

/// The checkpoint's tensor names as a model sees them through `format`: the packed tensors of each
/// module replaced by the module's name. For a check that has only a listing of names (a hub's file list,
/// a safetensors index) and no tensors.
pub fn served_names(names: &BTreeSet<String>, format: &QuantFormat) -> BTreeSet<String> {
    let Some(v) = format.as_virtual() else { return names.clone() };
    let anchor = v.anchor_suffix();
    let mut out = names.clone();
    for n in names {
        let Some(module) = n.strip_suffix(anchor) else { continue };
        let all = v.roles().filter(|(_, _, req)| *req).all(|(_, suffix, _)| names.contains(&format!("{module}{suffix}")));
        if all {
            for (_, suffix, _) in v.roles() {
                out.remove(&format!("{module}{suffix}"));
            }
            out.insert(v.served_name(module));
        }
    }
    out
}
