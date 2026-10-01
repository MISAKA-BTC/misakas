//! **The `virtual` interpreter**: tensors a checkpoint stores packed, served as float tensors under
//! another name — with any number of axes, so a leading *expert* axis, and per-block scales over a
//! 3-D tensor, are nothing special: the first lane is `e`.
//!
//! A `virtual` format names its roles (`<module><suffix>` each, as a `tensors` format does) and
//! declares the tensor it serves: its lanes (`axes`, outermost first), its shape (one expression per
//! axis over the roles' shapes and the parameters) and, as one expression, the value of each element.
//! The served tensor is called `<module>`. OCP MXFP4 as Hugging Face stores it (gpt-oss) is
//!
//! ```text
//! roles  blocks  <name>_blocks  U8 [E, out, in/32, 16]    scales  <name>_scales  U8 [E, out, in/32]
//! axes   e, i, o            shape  [dim_blocks[0], dim_blocks[2] * 32, dim_blocks[1]]
//! value  fp4x2[(blocks[e, o, i / 32, (i % 32) / 2] >> (4 * (i % 2))) & 15] * 0.5 * e8m0(scales[e, o, i / 32])
//! ```
//!
//! — the `[E, in, out]` tensor the float checkpoint calls `<name>`, so every binding that reads the float
//! export reads this one unchanged. A source that serves them ([`crate::weights::described`]) lists the
//! served names instead of the packed ones; an element range of the served tensor is evaluated without
//! materialising the rest.

use super::desc::{ConfigDesc, DecodeDesc, LayoutDesc, QuantFormatDesc};
use super::expr::{Col, DslError, Mask, Node, R, compile, eval};
use super::tensors::{ConfigReader, ConfigRead, Defs, Rd, Role, RoleTensor, TEnv, TScope, check_roles, compile_roles, table};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::ops::Range;

/// Elements evaluated at once.
const CHUNK: usize = 1 << 18;

/// A compiled `virtual` format.
#[derive(Clone, Debug)]
pub struct VirtualFormat {
    pub name: String,
    roles: Vec<Role>,
    reader: ConfigReader,
    tables: Vec<super::expr::Table>,
    axes: Vec<String>,
    shape: Vec<Node>,
    checks: Vec<(Node, String)>,
    value: Node,
    consts: Vec<String>,
}

impl VirtualFormat {
    pub fn compile(d: &QuantFormatDesc) -> R<VirtualFormat> {
        let LayoutDesc::Virtual { roles, axes, shape, checks } = &d.layout else {
            return Err(DslError("not a virtual layout".into()));
        };
        let DecodeDesc { target, group, q, scale, zero, min, value, code, offset_term, order } = &d.decode;
        if target != "tensor" {
            return Err(DslError(format!("a virtual layout decodes to `tensor`, not `{target}`")));
        }
        if group.is_some() || q.is_some() || scale.is_some() || zero.is_some() || min.is_some() || code.is_some() || offset_term.is_some() || order.is_some() {
            return Err(DslError("a virtual layout's decode is `value` alone".into()));
        }
        if axes.is_empty() || axes.len() > 4 || shape.len() != axes.len() {
            return Err(DslError(format!("a virtual layout has 1 to 4 axes and one shape expression each ({} axes, {} shape expressions)", axes.len(), shape.len())));
        }
        let lane_names: Vec<&str> = axes.iter().map(String::as_str).collect();
        if lane_names.iter().enumerate().any(|(i, a)| lane_names[..i].contains(a)) {
            return Err(DslError("two axes share a name".into()));
        }
        let rs = compile_roles(roles, &lane_names)?;
        if rs.is_empty() || !rs[0].required {
            return Err(DslError("a virtual layout's first role is required: it names the module".into()));
        }
        let mut tables = Vec::new();
        for (name, t) in &d.tables {
            tables.push(table(name, t)?);
        }
        let mut consts: Vec<String> = d.params.keys().cloned().collect();
        consts.extend(rs.iter().map(|r| format!("has_{}", r.name)));
        for c in &consts {
            if lane_names.contains(&c.as_str()) || rs.iter().any(|r| r.name == *c) {
                return Err(DslError(format!("the name `{c}` is taken")));
            }
        }
        let scope = TScope { lanes: &lane_names, consts: &consts, roles: &rs, tables: &tables };
        let value = compile(value.as_deref().ok_or_else(|| DslError("a virtual layout declares decode.value".into()))?, &scope)
            .map_err(|e| DslError(format!("decode.value: {e}")))?;
        let shape = shape.iter().map(|e| compile(e, &scope).map_err(|x| DslError(format!("shape `{e}`: {x}")))).collect::<R<Vec<_>>>()?;
        let mut chk = Vec::new();
        for c in checks {
            chk.push((compile(&c.expr, &scope).map_err(|e| DslError(format!("check `{}`: {e}", c.expr)))?, c.message.clone()));
        }
        Ok(VirtualFormat {
            name: d.name.clone(),
            roles: rs,
            reader: ConfigReader { name: d.name.clone(), params: d.params.clone(), config: d.config.clone() },
            tables,
            axes: axes.clone(),
            shape,
            checks: chk,
            value,
            consts,
        })
    }

    /// `(role name, suffix, required)` — the tensors a module holds.
    pub fn roles(&self) -> impl Iterator<Item = (&str, &str, bool)> {
        self.roles.iter().map(|r| (r.name.as_str(), r.suffix.as_str(), r.required))
    }

    /// The suffix of the first role: a tensor ending with it is a module of this format.
    pub fn anchor_suffix(&self) -> &str {
        &self.roles[0].suffix
    }

    pub fn rank(&self) -> usize {
        self.axes.len()
    }

    /// Whether a module's tensors are stored in this format: every required role present, in an accepted
    /// dtype and with its declared rank (`present(suffix)` gives its dtype and rank).
    pub fn stores(&self, present: impl Fn(&str) -> Option<(String, usize)>) -> bool {
        self.roles.iter().filter(|r| r.required).all(|r| {
            present(&r.suffix).is_some_and(|(dt, rank)| rank == r.rank && Rd::parse(&dt).is_some_and(|d| r.dtypes.contains(&d)))
        })
    }

    pub fn config_keys(&self) -> Vec<String> {
        self.reader.config_keys()
    }

    pub fn read_config(&self, q: &serde_json::Value) -> R<ConfigRead> {
        self.reader.read_config(q)
    }

    pub fn resolve_params(&self, config: &serde_json::Value) -> R<BTreeMap<String, i64>> {
        self.reader.resolve_params(config)
    }

    fn const_env<'a>(&'a self, roles: &'a [Option<RoleTensor>], params: &BTreeMap<String, i64>) -> R<TEnv<'a>> {
        if roles.len() != self.roles.len() {
            return Err(DslError(format!("{}: {} role tensors for {} roles", self.name, roles.len(), self.roles.len())));
        }
        let mut vars = vec![Col::CI(0); self.axes.len()];
        for c in &self.consts {
            let v = match c.as_str() {
                n if n.starts_with("has_") => {
                    let r = self.roles.iter().position(|r| r.name == n[4..]).expect("a has_ constant names a role");
                    roles[r].is_some() as i64
                }
                n => *params.get(n).ok_or_else(|| DslError(format!("{}: parameter `{n}` is not bound", self.name)))?,
            };
            vars.push(Col::CI(v));
        }
        Ok(TEnv { defs: Defs { roles: &self.roles, tables: &self.tables }, roles, vars, n: 1 })
    }

    /// The served tensor's shape, from the roles' shapes (headers are enough) — after the declarations
    /// and the format's own consistency rules have been checked against them.
    pub fn shape(&self, roles: &[Option<RoleTensor>], params: &BTreeMap<String, i64>) -> R<Vec<usize>> {
        check_roles(&self.name, &self.roles, roles)?;
        let env = self.const_env(roles, params)?;
        for (n, msg) in &self.checks {
            if eval(n, &env, Mask(None)).map_err(|e| DslError(format!("{}: check: {e}", self.name)))?.int_at(0)? == 0 {
                return Err(DslError(format!("{}: {msg}", self.name)));
            }
        }
        self.shape
            .iter()
            .enumerate()
            .map(|(a, n)| {
                let v = eval(n, &env, Mask(None)).and_then(|c| c.int_at(0)).map_err(|e| DslError(format!("{}: shape of axis {a}: {e}", self.name)))?;
                usize::try_from(v).ok().filter(|v| *v > 0).ok_or_else(|| DslError(format!("{}: axis {a} has {v} elements", self.name)))
            })
            .collect()
    }

    /// The elements `range` of the served tensor (row-major over its shape) as `f32`. Needs the role
    /// tensors' data. The expression is evaluated by chunks, in parallel; an element that is not finite
    /// in `f32` is an error.
    pub fn decode_range(&self, roles: &[Option<RoleTensor>], params: &BTreeMap<String, i64>, range: Range<usize>) -> R<Vec<f32>> {
        let shape = self.shape(roles, params)?;
        let total: usize = shape.iter().product();
        if range.start > range.end || range.end > total {
            return Err(DslError(format!("{}: elements {range:?} of a tensor of {total}", self.name)));
        }
        let base = self.const_env(roles, params)?;
        // Row-major strides.
        let mut stride = vec![1usize; shape.len()];
        for a in (0..shape.len().saturating_sub(1)).rev() {
            stride[a] = stride[a + 1] * shape[a + 1];
        }
        let starts: Vec<usize> = (range.start..range.end).step_by(CHUNK).collect();
        let parts: Vec<R<Vec<f32>>> = starts
            .par_iter()
            .map(|&a| {
                let b = (a + CHUNK).min(range.end);
                let n = b - a;
                let mut env = base.with_lanes(n);
                for (ax, dim) in shape.iter().enumerate() {
                    env.vars[ax] = Col::I((a..b).map(|k| ((k / stride[ax]) % dim) as i64).collect());
                }
                let v = eval(&self.value, &env, Mask(None)).map_err(|e| DslError(format!("{}: decode.value: {e}", self.name)))?;
                let v = v.into_floats(n);
                v.into_iter()
                    .map(|x| {
                        let f = x as f32;
                        if f.is_finite() { Ok(f) } else { Err(DslError(format!("{}: a value {x:e} is not finite in f32", self.name))) }
                    })
                    .collect()
            })
            .collect();
        let mut out = Vec::with_capacity(range.len());
        for p in parts {
            out.extend(p?);
        }
        Ok(out)
    }

    pub fn config(&self) -> Option<&ConfigDesc> {
        self.reader.config.as_ref()
    }
}
