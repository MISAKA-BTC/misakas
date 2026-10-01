//! **The `tensors` interpreter**: a weight stored as several named checkpoint tensors (GPTQ's
//! `qweight` / `qzeros` / `scales` / `g_idx`, AWQ's, an FP8 weight and its block scales) decoded by
//! evaluating a descriptor's expressions, which index the role tensors by element
//! (`qweight[i / 8, o]`) and read the format's parameters (`bits`, `group_size`, from the model's
//! `quantization_config`). Same language, same lanes as [`super::blocks`]; the data differ.
//!
//! Variables of an expression: the lanes `o` (output row), `i` (input column), `g` (group of the
//! row), and constants — every parameter by name, `out` and `inp` (the weight's shape), `gs` (columns
//! per group), `ng` (groups per row), and `has_<role>` (1 when an optional role tensor is present, so
//! `has_g_idx ? g_idx[i] : i / gs` reads `g_idx` only where it exists). A role's shape is
//! `dim_<role>[axis]`.
//!
//! A format of this layout is decoded one module at a time: its role tensors are resident (a module is
//! one projection), the decode itself runs in batches of rows.

use super::desc::{CodeDesc, CodeRange, DecodeDesc, LayoutDesc, ParamDesc, QuantFormatDesc, RoleDesc, SizeDesc, TableDesc, ValDesc};
use super::expr::{Col, DslError, Env, Mask, Name, Node, R, Scope, Table, compile, eval};
use crate::prequant::QWeight;
use rayon::prelude::*;
use std::collections::BTreeMap;

const LANES: &[&str] = &["o", "i", "g"];

/// One role tensor as the checkpoint stores it.
#[derive(Clone, Debug)]
pub struct RoleTensor {
    pub shape: Vec<usize>,
    /// The checkpoint's dtype name (`I32`, `U8`, `F16`, `BF16`, `F32`, `F8_E4M3`, …).
    pub dtype: String,
    /// Little-endian, row-major.
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rd {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    F16,
    Bf16,
    F32,
    F64,
    Fp8E4m3,
    Fp8E5m2,
}

impl Rd {
    fn parse(s: &str) -> Option<Rd> {
        Some(match s {
            "I8" => Rd::I8,
            "U8" => Rd::U8,
            "I16" => Rd::I16,
            "U16" => Rd::U16,
            "I32" => Rd::I32,
            "U32" => Rd::U32,
            "I64" => Rd::I64,
            "F16" => Rd::F16,
            "BF16" => Rd::Bf16,
            "F32" => Rd::F32,
            "F64" => Rd::F64,
            "F8_E4M3" | "F8_E4M3FN" => Rd::Fp8E4m3,
            "F8_E5M2" => Rd::Fp8E5m2,
            _ => return None,
        })
    }
    fn size(self) -> usize {
        match self {
            Rd::I8 | Rd::U8 | Rd::Fp8E4m3 | Rd::Fp8E5m2 => 1,
            Rd::I16 | Rd::U16 | Rd::F16 | Rd::Bf16 => 2,
            Rd::I32 | Rd::U32 | Rd::F32 => 4,
            Rd::I64 | Rd::F64 => 8,
        }
    }
    fn is_float(self) -> bool {
        matches!(self, Rd::F16 | Rd::Bf16 | Rd::F32 | Rd::F64 | Rd::Fp8E4m3 | Rd::Fp8E5m2)
    }
}

#[derive(Clone, Debug)]
struct Role {
    name: String,
    suffix: String,
    dtypes: Vec<Rd>,
    required: bool,
    rank: usize,
}

#[derive(Clone, Debug)]
enum Target {
    Integers { q: Node, scale: Node, zero: Option<Node>, min: Option<Node>, code: (Node, Node) },
    Floats { value: Node },
}

/// A compiled `tensors` format.
#[derive(Clone, Debug)]
pub struct TensorsFormat {
    pub name: String,
    roles: Vec<Role>,
    params: BTreeMap<String, ParamDesc>,
    tables: Vec<Table>,
    out: Node,
    inp: Node,
    group_size: Node,
    group_index: Option<Node>,
    checks: Vec<(Node, String)>,
    target: Target,
    /// Names of the constant variables, in slot order after the lanes.
    consts: Vec<String>,
}

struct TScope<'a> {
    consts: &'a [String],
    roles: &'a [Role],
    tables: &'a [Table],
}

impl Scope for TScope<'_> {
    fn resolve(&self, name: &str) -> Option<Name> {
        if let Some(i) = LANES.iter().position(|v| *v == name) {
            return Some(Name::Var(i));
        }
        if let Some(i) = self.consts.iter().position(|v| v == name) {
            return Some(Name::Var(LANES.len() + i));
        }
        if let Some(i) = self.roles.iter().position(|r| r.name == name) {
            return Some(Name::Read { slot: i, arity: self.roles[i].rank });
        }
        if let Some(r) = name.strip_prefix("dim_")
            && let Some(i) = self.roles.iter().position(|x| x.name == r)
        {
            return Some(Name::Read { slot: self.roles.len() + i, arity: 1 });
        }
        self.tables.iter().position(|t| t.name == name).map(Name::Table)
    }
}

fn table(name: &str, t: &TableDesc) -> R<Table> {
    // The same table syntax as blocks formats.
    super::blocks::table_of(name, t)
}

impl TensorsFormat {
    pub fn compile(d: &QuantFormatDesc) -> R<TensorsFormat> {
        let LayoutDesc::Tensors { roles, dims, checks } = &d.layout else {
            return Err(DslError("not a tensors layout".into()));
        };
        let mut rs = Vec::new();
        for RoleDesc { name, suffix, dtypes, required, rank } in roles {
            let dts = dtypes.iter().map(|t| Rd::parse(t).ok_or_else(|| DslError(format!("role `{name}`: dtype `{t}`")))).collect::<R<Vec<_>>>()?;
            if dts.is_empty() || *rank == 0 || *rank > 4 || LANES.contains(&name.as_str()) || rs.iter().any(|r: &Role| r.name == *name) {
                return Err(DslError(format!("role `{name}` is malformed or named like another")));
            }
            rs.push(Role { name: name.clone(), suffix: suffix.clone(), dtypes: dts, required: *required, rank: *rank });
        }
        let mut tables = Vec::new();
        for (name, t) in &d.tables {
            tables.push(table(name, t)?);
        }
        // Constants: the parameters, the weight's shape, the group geometry, the optional roles.
        let mut consts: Vec<String> = d.params.keys().cloned().collect();
        consts.extend(["out", "inp", "gs", "ng"].map(String::from));
        consts.extend(rs.iter().map(|r| format!("has_{}", r.name)));
        for c in &consts {
            if LANES.contains(&c.as_str()) || rs.iter().any(|r| r.name == *c) {
                return Err(DslError(format!("the name `{c}` is taken")));
            }
        }
        let scope = TScope { consts: &consts, roles: &rs, tables: &tables };
        let comp = |what: &str, s: &Option<String>| -> R<Option<Node>> {
            s.as_ref().map(|s| compile(s, &scope).map_err(|e| DslError(format!("{what}: {e}")))).transpose()
        };
        let DecodeDesc { target, group, q, scale, zero, min, value, code } = &d.decode;
        let (gsz, gindex) = match group {
            Some(g) => (
                match &g.size {
                    SizeDesc::Fixed(n) => Node::Int(*n as i64),
                    SizeDesc::Expr(e) => compile(e, &scope).map_err(|e2| DslError(format!("decode.group.size: {e2}")))?,
                },
                g.index.as_ref().map(|s| compile(s, &scope).map_err(|e| DslError(format!("decode.group.index: {e}")))).transpose()?,
            ),
            // A float format has no scale groups: one group per row.
            None if target == "floats" => (compile("inp", &scope)?, None),
            None => return Err(DslError("a tensors format declares decode.group.size".into())),
        };
        let mut chk = Vec::new();
        for c in checks {
            chk.push((compile(&c.expr, &scope).map_err(|e| DslError(format!("check `{}`: {e}", c.expr)))?, c.message.clone()));
        }
        let target = match target.as_str() {
            "integers" => {
                let code: &CodeDesc = code.as_ref().ok_or_else(|| DslError("target `integers` declares decode.code {min, max}".into()))?;
                let val = |v: &ValDesc, what: &str| -> R<Node> {
                    match v {
                        ValDesc::Int(n) => Ok(Node::Int(*n)),
                        ValDesc::Expr(e) => compile(e, &scope).map_err(|e2| DslError(format!("decode.code.{what}: {e2}"))),
                    }
                };
                let code = (val(&code.min, "min")?, val(&code.max, "max")?);
                Target::Integers {
                    q: comp("decode.q", q)?.ok_or_else(|| DslError("target `integers` declares decode.q".into()))?,
                    scale: comp("decode.scale", scale)?.ok_or_else(|| DslError("target `integers` declares decode.scale".into()))?,
                    zero: comp("decode.zero", zero)?,
                    min: comp("decode.min", min)?,
                    code,
                }
            }
            "floats" => Target::Floats { value: comp("decode.value", value)?.ok_or_else(|| DslError("target `floats` declares decode.value".into()))? },
            other => return Err(DslError(format!("decode.target `{other}` (integers or floats)"))),
        };
        let out_n = compile(&dims.out, &scope).map_err(|e| DslError(format!("dims.out: {e}")))?;
        let inp_n = compile(&dims.inp, &scope).map_err(|e| DslError(format!("dims.inp: {e}")))?;
        Ok(TensorsFormat {
            name: d.name.clone(),
            roles: rs,
            params: d.params.clone(),
            tables,
            out: out_n,
            inp: inp_n,
            group_size: gsz,
            group_index: gindex,
            checks: chk,
            target,
            consts,
        })
    }

    /// `(role name, suffix, required)` — the tensors to fetch for a module.
    pub fn roles(&self) -> impl Iterator<Item = (&str, &str, bool)> {
        self.roles.iter().map(|r| (r.name.as_str(), r.suffix.as_str(), r.required))
    }

    pub fn is_integers(&self) -> bool {
        matches!(self.target, Target::Integers { .. })
    }

    /// Whether the weight's columns are gathered through a group index (the lowering then orders its
    /// input by group first).
    pub fn has_group_index(&self) -> bool {
        self.group_index.is_some()
    }

    /// The code range under `params` (it may depend on `bits`).
    pub fn code_range(&self, roles: &[Option<RoleTensor>], params: &BTreeMap<String, i64>) -> R<Option<CodeRange>> {
        let Target::Integers { code, .. } = &self.target else { return Ok(None) };
        let env = self.const_env(roles, params, None)?;
        let v = |n: &Node| -> R<i64> { eval(n, &env, Mask(None))?.int_at(0) };
        Ok(Some(CodeRange { min: v(&code.0)?, max: v(&code.1)? }.check()?))
    }

    pub fn has_min(&self) -> bool {
        matches!(&self.target, Target::Integers { min: Some(_), .. })
    }

    /// Resolve the parameters from a `quantization_config` (`ParamDesc::config` is a key, optionally
    /// indexed `weight_block_size[1]`), applying defaults and the allowed values.
    pub fn resolve_params(&self, config: &serde_json::Value) -> R<BTreeMap<String, i64>> {
        let mut out = BTreeMap::new();
        for (name, p) in &self.params {
            let from_cfg = p.config.as_ref().and_then(|path| config_path(config, path));
            let v = match (from_cfg, p.default) {
                (Some(v), _) => v,
                (None, Some(d)) => d,
                (None, None) => return Err(DslError(format!("{}: parameter `{name}` is not in the configuration and has no default", self.name))),
            };
            if !p.allowed.is_empty() && !p.allowed.contains(&v) {
                return Err(DslError(format!("{}: {name} = {v} (defined for {:?})", self.name, p.allowed)));
            }
            out.insert(name.clone(), v);
        }
        Ok(out)
    }

    /// `(out, inp)` of the weight the roles hold.
    pub fn dims(&self, roles: &[Option<RoleTensor>], params: &BTreeMap<String, i64>) -> R<(usize, usize)> {
        let env = self.const_env(roles, params, None)?;
        let ev = |n: &Node, what: &str| -> R<usize> {
            let c = eval(n, &env, Mask(None)).map_err(|e| DslError(format!("{}: dims.{what}: {e}", self.name)))?;
            let v = c.int_at(0)?;
            usize::try_from(v).ok().filter(|v| *v > 0).ok_or_else(|| DslError(format!("{}: dims.{what} = {v}", self.name)))
        };
        Ok((ev(&self.out, "out")?, ev(&self.inp, "inp")?))
    }

    fn const_env<'a>(&'a self, roles: &'a [Option<RoleTensor>], params: &BTreeMap<String, i64>, wh: Option<(usize, usize, usize, usize)>) -> R<TEnv<'a>> {
        if roles.len() != self.roles.len() {
            return Err(DslError(format!("{}: {} role tensors for {} roles", self.name, roles.len(), self.roles.len())));
        }
        let (out, inp, gs, ng) = wh.unwrap_or((0, 0, 0, 0));
        let mut vars = vec![Col::CI(0); LANES.len()];
        for c in &self.consts {
            let v = match c.as_str() {
                "out" => out as i64,
                "inp" => inp as i64,
                "gs" => gs as i64,
                "ng" => ng as i64,
                n if n.starts_with("has_") => {
                    let r = self.roles.iter().position(|r| r.name == n[4..]).expect("a has_ constant names a role");
                    roles[r].is_some() as i64
                }
                n => *params.get(n).ok_or_else(|| DslError(format!("{}: parameter `{n}` is not bound", self.name)))?,
            };
            vars.push(Col::CI(v));
        }
        Ok(TEnv { fmt: self, roles, vars, n: 1 })
    }

    /// A module's tensors → its stored integers.
    pub fn decode_integers(&self, roles: &[Option<RoleTensor>], params: &BTreeMap<String, i64>) -> R<QWeight> {
        let Target::Integers { q, scale, zero, min, code } = &self.target else {
            return Err(DslError(format!("{} decodes to floats, not stored integers", self.name)));
        };
        let g = self.geometry(roles, params)?;
        let (out, inp, gs, ng) = g;
        let base = self.const_env(roles, params, Some(g))?;
        let code = {
            let v = |n: &Node| -> R<i64> { eval(n, &base, Mask(None))?.int_at(0) };
            CodeRange { min: v(&code.0)?, max: v(&code.1)? }.check()?
        };
        // Each column's group.
        let gidx: Vec<u32> = {
            let mut env = base.with_lanes(inp);
            env.vars[1] = Col::I((0..inp as i64).collect());
            let c = match &self.group_index {
                Some(n) => eval(n, &env, Mask(None)).map_err(|e| DslError(format!("{}: decode.group.index: {e}", self.name)))?,
                None => Col::I((0..inp as i64).map(|i| i / gs as i64).collect()),
            };
            c.into_ints(inp)?
                .into_iter()
                .map(|v| u32::try_from(v).ok().filter(|v| (*v as usize) < ng).ok_or_else(|| DslError(format!("{}: a group index {v} outside [0, {ng})", self.name))))
                .collect::<R<_>>()?
        };
        // Per (row, group): scale, zero, min.
        let mut gctx = base.with_lanes(out * ng);
        gctx.vars[0] = Col::I((0..out * ng).map(|k| (k / ng) as i64).collect());
        gctx.vars[2] = Col::I((0..out * ng).map(|k| (k % ng) as i64).collect());
        let n = out * ng;
        let scales = eval(scale, &gctx, Mask(None)).map_err(|e| DslError(format!("{}: decode.scale: {e}", self.name)))?.into_floats(n);
        let zeros: Vec<i16> = match zero {
            None => vec![0; n],
            Some(z) => eval(z, &gctx, Mask(None))
                .map_err(|e| DslError(format!("{}: decode.zero: {e}", self.name)))?
                .into_ints(n)?
                .into_iter()
                .map(|v| i16::try_from(v).map_err(|_| DslError(format!("{}: a zero point {v} outside i16", self.name))))
                .collect::<R<_>>()?,
        };
        let mins = match min {
            None => None,
            Some(m) => Some(eval(m, &gctx, Mask(None)).map_err(|e| DslError(format!("{}: decode.min: {e}", self.name)))?.into_floats(n)),
        };
        // Codes, a batch of rows at a time.
        let step = (1usize << 18) / inp.max(1);
        let step = step.max(1);
        let parts: Vec<R<Vec<i16>>> = (0..out.div_ceil(step))
            .into_par_iter()
            .map(|b| {
                let (r0, r1) = (b * step, ((b + 1) * step).min(out));
                let mut env = base.with_lanes((r1 - r0) * inp);
                env.vars[0] = Col::I((r0..r1).flat_map(|o| std::iter::repeat_n(o as i64, inp)).collect());
                env.vars[1] = Col::I((r0..r1).flat_map(|_| 0..inp as i64).collect());
                env.vars[2] = Col::I((r0..r1).flat_map(|_| gidx.iter().map(|g| *g as i64)).collect());
                let qs = eval(q, &env, Mask(None)).map_err(|e| DslError(format!("{}: decode.q: {e}", self.name)))?.into_ints(env.n)?;
                qs.into_iter()
                    .map(|v| {
                        if v < code.min || v > code.max {
                            Err(DslError(format!("{}: a code {v} outside the declared {}..={}", self.name, code.min, code.max)))
                        } else {
                            Ok(v as i16)
                        }
                    })
                    .collect()
            })
            .collect();
        let mut q16 = Vec::with_capacity(out * inp);
        for p in parts {
            q16.extend(p?);
        }
        let range = code.max - code.min + 1;
        let bits = (64 - ((range - 1).max(1) as u64).leading_zeros()) as u8;
        Ok(QWeight { out, inp, group: gs, q: q16, scale: scales, zero: zeros, min: mins, gidx, bits, signed: code.min < 0, label: self.name.clone() })
    }

    /// A module's tensors → `f32` `[out, inp]`.
    pub fn decode_floats(&self, roles: &[Option<RoleTensor>], params: &BTreeMap<String, i64>) -> R<(Vec<f32>, usize, usize)> {
        match &self.target {
            Target::Integers { .. } => {
                let w = self.decode_integers(roles, params)?;
                let mut out = Vec::with_capacity(w.out * w.inp);
                for o in 0..w.out {
                    for i in 0..w.inp {
                        out.push(w.value(o, i) as f32);
                    }
                }
                Ok((out, w.out, w.inp))
            }
            Target::Floats { value } => {
                let g = self.geometry(roles, params)?;
                let (out, inp, ..) = g;
                let base = self.const_env(roles, params, Some(g))?;
                let step = ((1usize << 18) / inp.max(1)).max(1);
                let parts: Vec<R<Vec<f32>>> = (0..out.div_ceil(step))
                    .into_par_iter()
                    .map(|b| {
                        let (r0, r1) = (b * step, ((b + 1) * step).min(out));
                        let mut env = base.with_lanes((r1 - r0) * inp);
                        env.vars[0] = Col::I((r0..r1).flat_map(|o| std::iter::repeat_n(o as i64, inp)).collect());
                        env.vars[1] = Col::I((r0..r1).flat_map(|_| 0..inp as i64).collect());
                        let v = eval(value, &env, Mask(None)).map_err(|e| DslError(format!("{}: decode.value: {e}", self.name)))?;
                        Ok(v.into_floats(env.n).into_iter().map(|x| x as f32).collect())
                    })
                    .collect();
                let mut data = Vec::with_capacity(out * inp);
                for p in parts {
                    data.extend(p?);
                }
                Ok((data, out, inp))
            }
        }
    }

    /// `(out, inp, group size, groups per row)`, with the roles checked against their declarations.
    fn geometry(&self, roles: &[Option<RoleTensor>], params: &BTreeMap<String, i64>) -> R<(usize, usize, usize, usize)> {
        for (r, t) in self.roles.iter().zip(roles) {
            match t {
                None if r.required => return Err(DslError(format!("{}: the tensor for role `{}` ({}) is missing", self.name, r.name, r.suffix))),
                Some(t) => {
                    if t.shape.len() != r.rank {
                        return Err(DslError(format!("{}: role `{}` has {} axes, the format says {}", self.name, r.name, t.shape.len(), r.rank)));
                    }
                    let rd = Rd::parse(&t.dtype).ok_or_else(|| DslError(format!("{}: role `{}` is stored as {}", self.name, r.name, t.dtype)))?;
                    if !r.dtypes.contains(&rd) {
                        return Err(DslError(format!("{}: role `{}` is stored as {}, the format reads {:?}", self.name, r.name, t.dtype, r.dtypes)));
                    }
                    if t.data.len() != t.shape.iter().product::<usize>() * rd.size() {
                        return Err(DslError(format!("{}: role `{}` holds {} bytes for shape {:?} of {}", self.name, r.name, t.data.len(), t.shape, t.dtype)));
                    }
                }
                None => {}
            }
        }
        let (out, inp) = self.dims(roles, params)?;
        let env = self.const_env(roles, params, Some((out, inp, 0, 0)))?;
        let gs = eval(&self.group_size, &env, Mask(None)).map_err(|e| DslError(format!("{}: decode.group.size: {e}", self.name)))?.int_at(0)?;
        let gs = usize::try_from(gs).ok().filter(|g| *g > 0).ok_or_else(|| DslError(format!("{}: a group size of {gs}", self.name)))?;
        let ng = inp.div_ceil(gs);
        // The format's own consistency rules, with the group geometry known.
        let env = self.const_env(roles, params, Some((out, inp, gs, ng)))?;
        for (n, msg) in &self.checks {
            if eval(n, &env, Mask(None)).map_err(|e| DslError(format!("{}: check: {e}", self.name)))?.int_at(0)? == 0 {
                return Err(DslError(format!("{}: {msg}", self.name)));
            }
        }
        Ok((out, inp, gs, ng))
    }
}

/// A key of the configuration, optionally indexed (`weight_block_size[1]`), as an integer (a bool is
/// 0/1).
fn config_path(config: &serde_json::Value, path: &str) -> Option<i64> {
    let (key, idx) = match path.split_once('[') {
        Some((k, rest)) => (k, Some(rest.trim_end_matches(']').parse::<usize>().ok()?)),
        None => (path, None),
    };
    let v = config.get(key)?;
    let v = match idx {
        Some(i) => v.get(i)?,
        None => v,
    };
    v.as_i64().or_else(|| v.as_bool().map(|b| b as i64))
}

struct TEnv<'a> {
    fmt: &'a TensorsFormat,
    roles: &'a [Option<RoleTensor>],
    vars: Vec<Col>,
    n: usize,
}

impl<'a> TEnv<'a> {
    fn with_lanes(&self, n: usize) -> TEnv<'a> {
        TEnv { fmt: self.fmt, roles: self.roles, vars: self.vars.clone(), n }
    }
}

impl Env for TEnv<'_> {
    fn lanes(&self) -> usize {
        self.n
    }
    fn var(&self, slot: usize) -> &Col {
        &self.vars[slot]
    }
    fn table(&self, t: usize) -> &Table {
        &self.fmt.tables[t]
    }
    fn read(&self, slot: usize, index: &[Col], mask: Mask<'_>) -> R<Col> {
        let nr = self.fmt.roles.len();
        // A role's shape.
        if slot >= nr {
            let r = slot - nr;
            let t = self.roles[r].as_ref().ok_or_else(|| DslError(format!("`dim_{}`: the role is absent", self.fmt.roles[r].name)))?;
            let axis = index[0].int_at(0)?;
            return Ok(Col::CI(*usize::try_from(axis).ok().and_then(|a| t.shape.get(a)).ok_or_else(|| DslError(format!("`dim_{}[{axis}]`", self.fmt.roles[r].name)))? as i64));
        }
        let role = &self.fmt.roles[slot];
        let t = self.roles[slot].as_ref().ok_or_else(|| DslError(format!("the tensor of role `{}` is absent", role.name)))?;
        let rd = Rd::parse(&t.dtype).expect("checked in geometry");
        let size = rd.size();
        let mut ints: Vec<i64> = Vec::new();
        let mut floats: Vec<f64> = Vec::new();
        for k in 0..self.n {
            if !mask.on(k) {
                if rd.is_float() { floats.push(0.0) } else { ints.push(0) }
                continue;
            }
            let mut flat = 0usize;
            for (axis, dim) in t.shape.iter().enumerate() {
                let i = index[axis].int_at(k)?;
                if i < 0 || i as usize >= *dim {
                    return Err(DslError(format!("role `{}`{:?} indexed at {i} on axis {axis}", role.name, t.shape)));
                }
                flat = flat * dim + i as usize;
            }
            let b = &t.data[flat * size..flat * size + size];
            match rd {
                Rd::I8 => ints.push(b[0] as i8 as i64),
                Rd::U8 => ints.push(b[0] as i64),
                Rd::I16 => ints.push(i16::from_le_bytes([b[0], b[1]]) as i64),
                Rd::U16 => ints.push(u16::from_le_bytes([b[0], b[1]]) as i64),
                Rd::I32 => ints.push(i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
                Rd::U32 => ints.push(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
                Rd::I64 => ints.push(i64::from_le_bytes(b.try_into().expect("8 bytes"))),
                Rd::F16 => floats.push(super::expr::f16_bits_to_f64(u16::from_le_bytes([b[0], b[1]]))?),
                Rd::Bf16 => floats.push(super::expr::bf16_bits_to_f64(u16::from_le_bytes([b[0], b[1]]))?),
                Rd::F32 => {
                    let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                    if !v.is_finite() {
                        return Err(DslError(format!("role `{}`: a binary32 NaN or infinity", role.name)));
                    }
                    floats.push(v as f64)
                }
                Rd::F64 => {
                    let v = f64::from_le_bytes(b.try_into().expect("8 bytes"));
                    if !v.is_finite() {
                        return Err(DslError(format!("role `{}`: a binary64 NaN or infinity", role.name)));
                    }
                    floats.push(v)
                }
                Rd::Fp8E4m3 => floats.push(super::expr::fp8_e4m3_bits_to_f64(b[0])?),
                Rd::Fp8E5m2 => floats.push(super::expr::fp8_e5m2_bits_to_f64(b[0])?),
            }
        }
        Ok(if rd.is_float() { Col::F(floats) } else { Col::I(ints) })
    }
}
