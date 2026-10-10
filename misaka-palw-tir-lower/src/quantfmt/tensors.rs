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

use super::desc::{CodeDesc, CodeRange, ConfigDesc, DecodeDesc, LayoutDesc, ParamDesc, QuantFormatDesc, RoleDesc, SizeDesc, TableDesc, ValDesc};
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

impl RoleTensor {
    /// A tensor known by its header only (no data loaded): enough for the shape expressions
    /// (`dim_<role>[axis]`); an expression that reads one of its elements is an error.
    pub fn header_only(shape: Vec<usize>, dtype: impl Into<String>) -> RoleTensor {
        RoleTensor { shape, dtype: dtype.into(), data: Vec::new() }
    }
    /// The stored size of this tensor's data, from its header.
    pub fn stored_bytes(&self) -> Option<usize> {
        self.shape.iter().try_fold(1usize, |n, d| n.checked_mul(*d))?.checked_mul(Rd::parse(&self.dtype)?.size())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Rd {
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
    pub(super) fn parse(s: &str) -> Option<Rd> {
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
    pub(super) fn size(self) -> usize {
        match self {
            Rd::I8 | Rd::U8 | Rd::Fp8E4m3 | Rd::Fp8E5m2 => 1,
            Rd::I16 | Rd::U16 | Rd::F16 | Rd::Bf16 => 2,
            Rd::I32 | Rd::U32 | Rd::F32 => 4,
            Rd::I64 | Rd::F64 => 8,
        }
    }
    pub(super) fn is_float(self) -> bool {
        matches!(self, Rd::F16 | Rd::Bf16 | Rd::F32 | Rd::F64 | Rd::Fp8E4m3 | Rd::Fp8E5m2)
    }
}

#[derive(Clone, Debug)]
pub(super) struct Role {
    pub(super) name: String,
    pub(super) suffix: String,
    pub(super) dtypes: Vec<Rd>,
    pub(super) required: bool,
    pub(super) rank: usize,
}

#[derive(Clone, Debug)]
enum Target {
    Integers { q: Node, scale: Node, zero: Option<Node>, min: Option<Node>, code: (Node, Node) },
    Floats { value: Node },
}

/// How a format reads its `quantization_config`; shared by every layout a `quant_method` can announce.
#[derive(Clone, Debug)]
pub(crate) struct ConfigReader {
    pub name: String,
    pub params: BTreeMap<String, ParamDesc>,
    pub config: Option<ConfigDesc>,
}

impl ConfigReader {
    /// The `quantization_config` keys this format reads or declares: every top-level key of a
    /// configuration must be among them.
    pub fn config_keys(&self) -> Vec<String> {
        let top = |p: &str| p.split(['.', '[']).next().unwrap_or(p).to_string();
        let mut keys: Vec<String> = vec!["quant_method".into()];
        keys.extend(self.params.values().filter_map(|p| p.config.as_deref().map(top)));
        if let Some(c) = &self.config {
            keys.extend(c.inert.iter().cloned());
            keys.extend(c.skip.iter().cloned());
            keys.extend(c.checks.iter().map(|k| top(&k.path)));
        }
        keys.sort();
        keys.dedup();
        keys
    }

    /// Read a `quantization_config` for this format: every key is read, declared inert, named by a
    /// check or refused; the checks hold; the parameters resolve.
    pub fn read_config(&self, q: &serde_json::Value) -> R<ConfigRead> {
        let obj = q.as_object().ok_or_else(|| DslError("quantization_config is not an object".into()))?;
        let known = self.config_keys();
        let unknown: Vec<&String> = obj.keys().filter(|k| !known.contains(k)).collect();
        if !unknown.is_empty() {
            return Err(DslError(format!(
                "quantization_config has keys the `{}` descriptor does not read: {unknown:?} (a key that does not change what the stored tensors mean belongs in the descriptor's config.inert; one that does is a parameter or a check)",
                self.name
            )));
        }
        let cfg = self.config.clone().unwrap_or_default();
        for c in &cfg.checks {
            let v = json_path(q, &c.path);
            if !c.one_of.contains(&v) {
                return Err(DslError(format!("{}: {} (quantization_config.{} is {})", self.name, c.message, c.path, v)));
            }
        }
        let params = self.resolve_params(q)?;
        let mode = match cfg.skip_match.as_deref() {
            None | Some("exact") => "exact",
            Some("contains") => "contains",
            Some("path") => "path",
            Some("regex") => "regex",
            Some(o) => return Err(DslError(format!("{}: config.skip_match `{o}` (exact, contains, path or regex)", self.name))),
        };
        let skip: Vec<String> = match &cfg.skip {
            None => Vec::new(),
            Some(k) => match obj.get(k) {
                None | Some(serde_json::Value::Null) => Vec::new(),
                Some(serde_json::Value::Array(a)) => a
                    .iter()
                    .map(|m| {
                        let s = m.as_str().ok_or_else(|| DslError(format!("quantization_config.{k} entry is not a string")))?;
                        Ok(if s.starts_with("re:") {
                            s.to_string()
                        } else if mode == "regex" {
                            format!("re:{s}")
                        } else {
                            format!("{mode}:{s}")
                        })
                    })
                    .collect::<R<_>>()?,
                Some(_) => return Err(DslError(format!("quantization_config.{k} is not a list"))),
            },
        };
        let lm_head = match cfg.lm_head.as_deref() {
            None | Some("never") => false,
            Some("unless_skipped") => !crate::prequant::skip_matches(&skip, "lm_head"),
            // bitsandbytes: an absent (or null) `llm_int8_skip_modules` means the library's own default skip, which keeps the
            // output embedding in float; a list the quantiser was given replaces the default, and the head is quantised
            // unless the list names it (an empty list names nothing).
            Some("when_skip_given") => {
                let given = cfg.skip.as_ref().is_some_and(|k| matches!(obj.get(k), Some(v) if !v.is_null()));
                given && !crate::prequant::skip_matches(&skip, "lm_head")
            }
            Some(o) => {
                return Err(DslError(format!("{}: config.lm_head `{o}` (never, unless_skipped or when_skip_given)", self.name)));
            }
        };
        Ok(ConfigRead { params, skip, lm_head })
    }

    /// Resolve the parameters from a `quantization_config` (`ParamDesc::config` is a key, optionally
    /// indexed `weight_block_size[1]`), applying defaults and the allowed values.
    pub fn resolve_params(&self, config: &serde_json::Value) -> R<BTreeMap<String, i64>> {
        use serde_json::Value;
        let mut out = BTreeMap::new();
        for (name, p) in &self.params {
            // Read from each module's own tensors, not from the configuration.
            if p.from_role.is_some() {
                continue;
            }
            let raw = p.config.as_ref().map(|path| json_path(config, path));
            let from_cfg: Option<i64> = match &raw {
                None | Some(Value::Null) => None,
                // A string-valued key reads through the parameter's table; a string it does not list is refused, not defaulted.
                Some(Value::String(s)) if !p.map.is_empty() => Some(*p.map.get(&s.to_ascii_lowercase()).ok_or_else(|| {
                    DslError(format!("{}: {name} = `{s}` (defined for {:?})", self.name, p.map.keys().collect::<Vec<_>>()))
                })?),
                Some(v) => Some(v.as_i64().or_else(|| v.as_bool().map(|b| b as i64)).ok_or_else(|| {
                    DslError(format!("{}: quantization_config.{} is {v}, not an integer or a bool", self.name, p.config.as_deref().unwrap_or(name)))
                })?),
            };
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

}

/// A compiled `tensors` format.
#[derive(Clone, Debug)]
pub struct TensorsFormat {
    pub name: String,
    roles: Vec<Role>,
    reader: ConfigReader,
    tables: Vec<Table>,
    out: Node,
    inp: Node,
    group_size: Node,
    group_index: Option<Node>,
    checks: Vec<(Node, String)>,
    target: Target,
    offset_term: Option<Node>,
    order: Option<Node>,
    /// Names of the constant variables, in slot order after the lanes.
    consts: Vec<String>,
}

/// What a lowering's program structure depends on, from the parameters alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TLayout {
    /// Columns per group; `0`: the whole row is one group.
    pub group: usize,
    /// The input is gathered through a column order (the group index) first.
    pub order: bool,
    /// The lowering carries a per-group offset term.
    pub offset_term: bool,
}

/// A `quantization_config` read for a format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigRead {
    pub params: BTreeMap<String, i64>,
    /// The entries of the descriptor's `skip` key, each prefixed by how it names a module: `exact:`,
    /// `contains:` or `re:`.
    pub skip: Vec<String>,
    /// Whether the language-model head is stored in this format.
    pub lm_head: bool,
}

pub(super) struct TScope<'a> {
    pub(super) lanes: &'a [&'a str],
    pub(super) consts: &'a [String],
    pub(super) roles: &'a [Role],
    pub(super) tables: &'a [Table],
}

impl Scope for TScope<'_> {
    fn resolve(&self, name: &str) -> Option<Name> {
        if let Some(i) = self.lanes.iter().position(|v| *v == name) {
            return Some(Name::Var(i));
        }
        if let Some(i) = self.consts.iter().position(|v| v == name) {
            return Some(Name::Var(self.lanes.len() + i));
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

pub(super) fn table(name: &str, t: &TableDesc) -> R<Table> {
    // The same table syntax as blocks formats.
    super::blocks::table_of(name, t)
}

/// The role declarations of a descriptor, checked: known dtypes, rank 1–4, no name a lane or another role has.
pub(super) fn compile_roles(roles: &[RoleDesc], lanes: &[&str]) -> R<Vec<Role>> {
    let mut rs: Vec<Role> = Vec::new();
    for RoleDesc { name, suffix, dtypes, required, rank } in roles {
        let dts = dtypes.iter().map(|t| Rd::parse(t).ok_or_else(|| DslError(format!("role `{name}`: dtype `{t}`")))).collect::<R<Vec<_>>>()?;
        if dts.is_empty() || *rank > 4 || lanes.contains(&name.as_str()) || rs.iter().any(|r: &Role| r.name == *name) {
            return Err(DslError(format!("role `{name}` is malformed or named like another")));
        }
        rs.push(Role { name: name.clone(), suffix: suffix.clone(), dtypes: dts, required: *required, rank: *rank });
    }
    Ok(rs)
}

/// The tensors a module holds against the declarations: every required role present, in an accepted dtype,
/// with the declared rank and (when its data is loaded) the bytes its shape says.
pub(super) fn check_roles(name: &str, defs: &[Role], roles: &[Option<RoleTensor>]) -> R<()> {
    for (r, t) in defs.iter().zip(roles) {
        match t {
            None if r.required => return Err(DslError(format!("{name}: the tensor for role `{}` ({}) is missing", r.name, r.suffix))),
            Some(t) => {
                if t.shape.len() != r.rank {
                    return Err(DslError(format!("{name}: role `{}` has {} axes, the format says {}", r.name, t.shape.len(), r.rank)));
                }
                let rd = Rd::parse(&t.dtype).ok_or_else(|| DslError(format!("{name}: role `{}` is stored as {}", r.name, t.dtype)))?;
                if !r.dtypes.contains(&rd) {
                    return Err(DslError(format!("{name}: role `{}` is stored as {}, the format reads {:?}", r.name, t.dtype, r.dtypes)));
                }
                let bytes = t.stored_bytes().ok_or_else(|| DslError(format!("{name}: role `{}` shape overflows", r.name)))?;
                if !t.data.is_empty() && t.data.len() != bytes {
                    return Err(DslError(format!(
                        "{name}: role `{}` holds {} bytes for shape {:?} of {}",
                        r.name,
                        t.data.len(),
                        t.shape,
                        t.dtype
                    )));
                }
            }
            None => {}
        }
    }
    Ok(())
}

impl TensorsFormat {
    pub fn compile(d: &QuantFormatDesc) -> R<TensorsFormat> {
        let LayoutDesc::Tensors { roles, dims, checks } = &d.layout else {
            return Err(DslError("not a tensors layout".into()));
        };
        let rs = compile_roles(roles, LANES)?;
        for (n, p) in &d.params {
            if let Some(rj) = &p.from_role {
                if p.config.is_some() {
                    return Err(DslError(format!("parameter `{n}` is read from a role: it has no `config`")));
                }
                if !rs.iter().any(|r| r.name == rj.role) {
                    return Err(DslError(format!("parameter `{n}` is read from role `{}`, which the format does not declare", rj.role)));
                }
                if !matches!(rj.kind.as_str(), "int" | "float" | "string") {
                    return Err(DslError(format!("parameter `{n}`: from_role.kind `{}` (int, float or string)", rj.kind)));
                }
            }
        }
        let mut tables = Vec::new();
        for (name, t) in &d.tables {
            tables.push(table(name, t)?);
        }
        // Constants: the parameters, the weight's shape, the group geometry, the optional roles.
        let mut consts: Vec<String> = d.params.keys().cloned().collect();
        consts.extend(["out", "inp", "gs", "ng"].map(String::from));
        consts.extend(rs.iter().map(|r| format!("has_{}", r.name)));
        consts.extend(rs.iter().map(|r| format!("float_{}", r.name)));
        for c in &consts {
            if LANES.contains(&c.as_str()) || rs.iter().any(|r| r.name == *c) {
                return Err(DslError(format!("the name `{c}` is taken")));
            }
        }
        let scope = TScope { lanes: LANES, consts: &consts, roles: &rs, tables: &tables };
        let comp = |what: &str, s: &Option<String>| -> R<Option<Node>> {
            s.as_ref().map(|s| compile(s, &scope).map_err(|e| DslError(format!("{what}: {e}")))).transpose()
        };
        let DecodeDesc { target, group, q, scale, zero, min, value, code, offset_term, order } = &d.decode;
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
        let offset_term = comp("decode.offset_term", offset_term)?;
        let order = comp("decode.order", order)?;
        let out_n = compile(&dims.out, &scope).map_err(|e| DslError(format!("dims.out: {e}")))?;
        let inp_n = compile(&dims.inp, &scope).map_err(|e| DslError(format!("dims.inp: {e}")))?;
        Ok(TensorsFormat {
            name: d.name.clone(),
            roles: rs,
            reader: ConfigReader { name: d.name.clone(), params: d.params.clone(), config: d.config.clone() },
            tables,
            out: out_n,
            inp: inp_n,
            group_size: gsz,
            group_index: gindex,
            checks: chk,
            target,
            offset_term,
            order,
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

    /// Whether a module's tensors are stored in this format: every required role is present
    /// (`present(suffix)` gives its dtype) and in a dtype the role accepts. A format that decodes to
    /// floats lets a module the quantiser left alone be read as the plain float tensor it is.
    pub fn stores(&self, present: impl Fn(&str) -> Option<String>) -> bool {
        self.roles.iter().filter(|r| r.required).all(|r| {
            present(&r.suffix).is_some_and(|dt| Rd::parse(&dt).is_some_and(|d| r.dtypes.contains(&d)))
        })
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

    /// The program structure this format lowers to under `params` (the row width is not known yet: a
    /// group size that says "the whole row" evaluates to 0).
    pub fn layout(&self, params: &BTreeMap<String, i64>) -> R<TLayout> {
        let none: Vec<Option<RoleTensor>> = vec![None; self.roles.len()];
        let env = self.const_env(&none, params, None)?;
        let ev = |n: &Node, what: &str| -> R<i64> { eval(n, &env, Mask(None)).and_then(|c| c.int_at(0)).map_err(|e| DslError(format!("{}: {what}: {e}", self.name))) };
        let group = ev(&self.group_size, "decode.group.size")?;
        let group = usize::try_from(group).map_err(|_| DslError(format!("{}: a group size of {group}", self.name)))?;
        let order = match &self.order {
            Some(n) => ev(n, "decode.order")? != 0,
            None => self.group_index.is_some(),
        };
        let offset_term = match (&self.offset_term, &self.target) {
            (Some(n), _) => ev(n, "decode.offset_term")? != 0,
            (None, Target::Floats { .. }) => false,
            (None, Target::Integers { min, code, zero, .. }) => {
                if min.is_some() {
                    true
                } else {
                    let (lo, hi) = (ev(&code.0, "decode.code.min")?, ev(&code.1, "decode.code.max")?);
                    let (zlo, zhi) = match zero {
                        None => (0, 0),
                        Some(Node::Int(z)) => (*z, *z),
                        Some(_) => (lo, hi),
                    };
                    lo - zhi < -128 || hi - zlo > 127
                }
            }
        };
        Ok(TLayout { group, order, offset_term })
    }

    /// The `quantization_config` keys this format reads or declares: every top-level key of a
    /// configuration must be among them.
    pub fn config_keys(&self) -> Vec<String> {
        self.reader.config_keys()
    }

    /// Read a `quantization_config` for this format: every key is read, declared inert, named by a
    /// check or refused; the checks hold; the parameters resolve.
    pub fn read_config(&self, q: &serde_json::Value) -> R<ConfigRead> {
        self.reader.read_config(q)
    }

    /// Resolve the parameters from a `quantization_config` (`ParamDesc::config` is a dotted path,
    /// optionally indexed), applying defaults, the allowed values and string tables.
    pub fn resolve_params(&self, config: &serde_json::Value) -> R<BTreeMap<String, i64>> {
        self.reader.resolve_params(config)
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
                "out" => Col::CI(out as i64),
                "inp" => Col::CI(inp as i64),
                "gs" => Col::CI(gs as i64),
                "ng" => Col::CI(ng as i64),
                n if n.starts_with("has_") => {
                    let r = self.roles.iter().position(|r| r.name == n[4..]).expect("a has_ constant names a role");
                    Col::CI(roles[r].is_some() as i64)
                }
                n if n.starts_with("float_") => {
                    let r = self.roles.iter().position(|r| r.name == n[6..]).expect("a float_ constant names a role");
                    Col::CI(is_float_role(roles[r].as_ref()))
                }
                n => match self.reader.params.get(n).filter(|p| p.from_role.is_some()) {
                    Some(p) => role_param(&self.name, n, p, &self.roles, roles)?,
                    None => Col::CI(*params.get(n).ok_or_else(|| DslError(format!("{}: parameter `{n}` is not bound", self.name)))?),
                },
            };
            vars.push(v);
        }
        Ok(TEnv { defs: Defs { roles: &self.roles, tables: &self.tables }, roles, vars, n: 1 })
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

// ───────────────────────────── a JSON document in a tensor ─────────────────────────────

/// A JSON value with its numbers kept as text: a decimal is parsed once, exactly, by whoever needs it
/// (serde_json's default parser is off by an ulp on some floats).
#[derive(Debug)]
enum J {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

struct JParser<'a> {
    b: &'a [u8],
    p: usize,
}

impl JParser<'_> {
    fn ws(&mut self) {
        while self.p < self.b.len() && self.b[self.p].is_ascii_whitespace() {
            self.p += 1;
        }
    }
    fn lit(&mut self, s: &str) -> bool {
        if self.b[self.p..].starts_with(s.as_bytes()) {
            self.p += s.len();
            true
        } else {
            false
        }
    }
    fn string(&mut self) -> Result<String, String> {
        self.p += 1;
        let mut out = String::new();
        loop {
            let c = *self.b.get(self.p).ok_or("an unterminated string")?;
            self.p += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let e = *self.b.get(self.p).ok_or("an unterminated escape")?;
                    self.p += 1;
                    out.push(match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let h = self.b.get(self.p..self.p + 4).ok_or("a short \\u escape")?;
                            self.p += 4;
                            char::from_u32(u32::from_str_radix(std::str::from_utf8(h).map_err(|e| e.to_string())?, 16).map_err(|e| e.to_string())?).unwrap_or('\u{fffd}')
                        }
                        other => return Err(format!("the escape \\{}", other as char)),
                    });
                }
                c if c < 0x80 => out.push(c as char),
                _ => {
                    // A multi-byte UTF-8 character: copy its bytes.
                    let start = self.p - 1;
                    let len = match c {
                        0xC0..=0xDF => 2,
                        0xE0..=0xEF => 3,
                        _ => 4,
                    };
                    let chunk = self.b.get(start..start + len).ok_or("a truncated character")?;
                    out.push_str(std::str::from_utf8(chunk).map_err(|e| e.to_string())?);
                    self.p = start + len;
                }
            }
        }
    }
    fn value(&mut self, depth: usize) -> Result<J, String> {
        if depth > 32 {
            return Err("nested too deeply".into());
        }
        self.ws();
        match self.b.get(self.p).copied() {
            None => Err("ends early".into()),
            Some(b'{') => {
                self.p += 1;
                let mut v = Vec::new();
                self.ws();
                if self.b.get(self.p) == Some(&b'}') {
                    self.p += 1;
                    return Ok(J::Obj(v));
                }
                loop {
                    self.ws();
                    match self.b.get(self.p) {
                        Some(b'"') => {}
                        None => return Err("ends early".into()),
                        Some(_) => return Err("an object key is not a string".into()),
                    }
                    let k = self.string()?;
                    self.ws();
                    match self.b.get(self.p) {
                        Some(b':') => {}
                        None => return Err("ends early".into()),
                        Some(_) => return Err("a `:` is missing".into()),
                    }
                    self.p += 1;
                    v.push((k, self.value(depth + 1)?));
                    self.ws();
                    match self.b.get(self.p) {
                        Some(b',') => self.p += 1,
                        Some(b'}') => {
                            self.p += 1;
                            return Ok(J::Obj(v));
                        }
                        _ => return Err("an object is not closed".into()),
                    }
                }
            }
            Some(b'[') => {
                self.p += 1;
                let mut v = Vec::new();
                self.ws();
                if self.b.get(self.p) == Some(&b']') {
                    self.p += 1;
                    return Ok(J::Arr(v));
                }
                loop {
                    v.push(self.value(depth + 1)?);
                    self.ws();
                    match self.b.get(self.p) {
                        Some(b',') => self.p += 1,
                        Some(b']') => {
                            self.p += 1;
                            return Ok(J::Arr(v));
                        }
                        _ => return Err("an array is not closed".into()),
                    }
                }
            }
            Some(b'"') => Ok(J::Str(self.string()?)),
            Some(_) if self.lit("true") => Ok(J::Bool(true)),
            Some(_) if self.lit("false") => Ok(J::Bool(false)),
            Some(_) if self.lit("null") => Ok(J::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => {
                let start = self.p;
                while self.p < self.b.len() && matches!(self.b[self.p], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                    self.p += 1;
                }
                Ok(J::Num(String::from_utf8_lossy(&self.b[start..self.p]).into_owned()))
            }
            Some(c) => Err(format!("unexpected `{}`", c as char)),
        }
    }
}

fn parse_json(bytes: &[u8]) -> Result<J, String> {
    let mut p = JParser { b: bytes, p: 0 };
    let v = p.value(0)?;
    p.ws();
    if p.p != bytes.len() {
        return Err("text after the document".into());
    }
    Ok(v)
}

/// A dotted path (`shape[1]`, `nested_offset`) into a document.
fn j_path<'a>(j: &'a J, path: &str) -> Option<&'a J> {
    let mut v = j;
    for seg in path.split('.') {
        let (key, idx) = match seg.split_once('[') {
            Some((k, rest)) => (k, Some(rest.trim_end_matches(']').parse::<usize>().ok()?)),
            None => (seg, None),
        };
        if !key.is_empty() {
            v = match v {
                J::Obj(o) => &o.iter().find(|(k, _)| k == key)?.1,
                _ => return None,
            };
        }
        if let Some(i) = idx {
            v = match v {
                J::Arr(a) => a.get(i)?,
                _ => return None,
            };
        }
    }
    Some(v)
}

/// 1 when a role's tensor is stored in a floating-point dtype, else 0 (also when it is absent): a role that
/// accepts both (bitsandbytes' `absmax` is float, or u8 under double quantisation) can be told apart.
pub(super) fn is_float_role(t: Option<&RoleTensor>) -> i64 {
    t.and_then(|t| Rd::parse(&t.dtype)).is_some_and(|d| d.is_float()) as i64
}

/// A parameter a module carries in a tensor of its own (a JSON document): the constant its expressions read.
fn role_param(fmt: &str, name: &str, p: &ParamDesc, defs: &[Role], roles: &[Option<RoleTensor>]) -> R<Col> {
    let rj = p.from_role.as_ref().expect("checked by the caller");
    let ri = defs.iter().position(|r| r.name == rj.role).expect("checked at compile time");
    let Some(t) = roles[ri].as_ref() else {
        // No module's tensors at all (a layout asked for from the parameters alone): a placeholder.
        return if roles.iter().all(Option::is_none) { Ok(Col::CI(0)) } else { Err(DslError(format!("{fmt}: parameter `{name}` is read from role `{}`, which this module lacks", rj.role))) };
    };
    if t.data.is_empty() {
        return Err(DslError(format!("{fmt}: parameter `{name}`: the document of role `{}` is not loaded", rj.role)));
    }
    let doc = parse_json(&t.data).map_err(|e| DslError(format!("{fmt}: role `{}` is not a JSON document: {e}", rj.role)))?;
    let Some(v) = j_path(&doc, &rj.path) else {
        // An absent key takes the parameter's default (bitsandbytes writes the nested-quantisation keys only when it nests).
        return match p.default {
            Some(d) => Ok(if rj.kind == "float" { Col::CF(d as f64) } else { Col::CI(d) }),
            None => Err(DslError(format!("{fmt}: the document of role `{}` has no `{}`", rj.role, rj.path))),
        };
    };
    let col = match (rj.kind.as_str(), v) {
        ("int", J::Num(s)) => Col::CI(s.parse::<i64>().map_err(|_| DslError(format!("{fmt}: `{}` is {s}, not an integer", rj.path)))?),
        ("int", J::Bool(b)) => Col::CI(*b as i64),
        ("float", J::Num(s)) => {
            let f = s.parse::<f64>().map_err(|_| DslError(format!("{fmt}: `{}` is {s}, not a number", rj.path)))?;
            if !f.is_finite() {
                return Err(DslError(format!("{fmt}: `{}` is not finite", rj.path)));
            }
            Col::CF(f)
        }
        ("string", J::Str(s)) => Col::CI(*p.map.get(&s.to_ascii_lowercase()).ok_or_else(|| DslError(format!("{fmt}: {name} = `{s}` (defined for {:?})", p.map.keys().collect::<Vec<_>>())))?),
        (k, other) => return Err(DslError(format!("{fmt}: `{}` is {other:?}, not a {k}", rj.path))),
    };
    if let (Col::CI(v), false) = (&col, p.allowed.is_empty())
        && !p.allowed.contains(v)
    {
        return Err(DslError(format!("{fmt}: {name} = {v} (defined for {:?})", p.allowed)));
    }
    Ok(col)
}

/// The value at a dotted path of the configuration — keys, each optionally indexed
/// (`config_groups.group_0.weights.num_bits`, `weight_block_size[1]`); an absent key is `null`.
pub fn json_path(config: &serde_json::Value, path: &str) -> serde_json::Value {
    let mut v = config;
    for seg in path.split('.') {
        let (key, idx) = match seg.split_once('[') {
            Some((k, rest)) => (k, rest.trim_end_matches(']').parse::<usize>().ok()),
            None => (seg, None),
        };
        match v.get(key) {
            Some(x) => v = x,
            None => return serde_json::Value::Null,
        }
        if let Some(i) = idx {
            match v.get(i) {
                Some(x) => v = x,
                None => return serde_json::Value::Null,
            }
        }
    }
    v.clone()
}

/// What an expression reads: the role declarations and the tables of a format.
#[derive(Clone, Copy)]
pub(super) struct Defs<'a> {
    pub(super) roles: &'a [Role],
    pub(super) tables: &'a [Table],
}

pub(super) struct TEnv<'a> {
    pub(super) defs: Defs<'a>,
    pub(super) roles: &'a [Option<RoleTensor>],
    pub(super) vars: Vec<Col>,
    pub(super) n: usize,
}

impl<'a> TEnv<'a> {
    pub(super) fn with_lanes(&self, n: usize) -> TEnv<'a> {
        TEnv { defs: self.defs, roles: self.roles, vars: self.vars.clone(), n }
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
        &self.defs.tables[t]
    }
    fn read(&self, slot: usize, index: &[Col], mask: Mask<'_>) -> R<Col> {
        self.read_with(slot, index, mask, &|r, range| {
            let t = self.roles[r].as_ref().expect("presence checked before reading");
            t.data.get(range).map(std::borrow::Cow::Borrowed).ok_or_else(|| {
                DslError(format!("role `{}`: an element is read but only the tensor's header is loaded", self.defs.roles[r].name))
            })
        })
    }
}

impl<'a> TEnv<'a> {
    /// The same typed element reader, with a bounded range provider instead of resident tensors.
    pub(super) fn read_with(
        &self,
        slot: usize,
        index: &[Col],
        mask: Mask<'_>,
        read: &dyn Fn(usize, std::ops::Range<usize>) -> R<std::borrow::Cow<'a, [u8]>>,
    ) -> R<Col> {
        let nr = self.defs.roles.len();
        // A role's shape.
        if slot >= nr {
            let r = slot - nr;
            let t = self.roles[r].as_ref().ok_or_else(|| DslError(format!("`dim_{}`: the role is absent", self.defs.roles[r].name)))?;
            let at = |axis: i64| -> R<i64> {
                let dim = *usize::try_from(axis)
                    .ok()
                    .and_then(|a| t.shape.get(a))
                    .ok_or_else(|| DslError(format!("`dim_{}[{axis}]`", self.defs.roles[r].name)))?;
                i64::try_from(dim).map_err(|_| DslError("role dimension outside i64".into()))
            };
            // A dimension indexed by a lane is a column, never lane zero broadcast to the block.
            // The latter made conversion depend on read/chunk boundaries for valid descriptors.
            return if let Col::CI(axis) = &index[0] { Ok(Col::CI(at(*axis)?)) }
            else { (0..self.n).map(|k| if mask.on(k) { at(index[0].int_at(k)?) } else { Ok(0) }).collect::<R<Vec<_>>>().map(Col::I) };
        }
        let role = &self.defs.roles[slot];
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
                flat = flat
                    .checked_mul(*dim)
                    .and_then(|n| n.checked_add(i as usize))
                    .ok_or_else(|| DslError(format!("role `{}` index overflows", role.name)))?;
            }
            let at = flat.checked_mul(size).ok_or_else(|| DslError("role byte offset overflows".into()))?;
            let end = at.checked_add(size).ok_or_else(|| DslError("role byte offset overflows".into()))?;
            let bytes = read(slot, at..end)?;
            if bytes.len() != size {
                return Err(DslError(format!("role `{}`: short element range", role.name)));
            }
            let b = bytes.as_ref();
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


#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantfmt::desc::RoleJsonDesc;

    #[test]
    fn the_json_reader_keeps_numbers_as_text_and_reads_what_bitsandbytes_writes() {
        // `json.dumps` of a quant state, with a float that serde_json's default parser is off by an ulp on, and escapes.
        let doc = br#"{"quant_type": "nf4", "blocksize": 64, "dtype": "bfloat16", "shape": [4096, 11008], "nested_offset": 0.30000000000000004, "note": "a\"b\\c\n", "x": null, "y": true}"#;
        let j = parse_json(doc).expect("parses");
        let num = |p: &str| match j_path(&j, p) {
            Some(J::Num(s)) => s.clone(),
            other => panic!("{p}: {other:?}"),
        };
        assert_eq!(num("blocksize"), "64");
        assert_eq!(num("shape[1]"), "11008");
        assert_eq!(num("nested_offset").parse::<f64>().unwrap(), 0.1 + 0.2, "parsed once, exactly");
        assert!(matches!(j_path(&j, "dtype"), Some(J::Str(s)) if s == "bfloat16"));
        assert!(matches!(j_path(&j, "note"), Some(J::Str(s)) if s == "a\"b\\c\n"));
        assert!(matches!(j_path(&j, "x"), Some(J::Null)) && matches!(j_path(&j, "y"), Some(J::Bool(true))));
        assert!(j_path(&j, "shape[2]").is_none() && j_path(&j, "nope").is_none() && j_path(&j, "blocksize.x").is_none());
        // A \u escape: the backslash-u-0-0-e-9 of `json.dumps(ensure_ascii=True)` is the character e-acute.
        let esc: Vec<u8> = [b'"', b'\\', b'u', b'0', b'0', b'e', b'9', b'"'].to_vec();
        assert!(matches!(parse_json(&esc), Ok(J::Str(s)) if s == "\u{e9}"));
        // Multi-byte characters straight in a string, whitespace everywhere, nesting.
        let j = parse_json("  { \"k\" : [ 1 , { \"é\" : \"ü→\" } ] }  ".as_bytes()).expect("parses");
        assert!(matches!(j_path(&j, "k[1].é"), Some(J::Str(s)) if s == "ü→"));
    }

    #[test]
    fn a_document_that_is_not_json_is_an_error_with_a_reason() {
        for (bad, why) in [
            ("", "ends early"),
            ("{", "ends early"),
            (r#"{"a": 1"#, "not closed"),
            (r#"{"a" 1}"#, "`:`"),
            (r#"{1: 2}"#, "not a string"),
            (r#"[1, 2"#, "not closed"),
            (r#""abc"#, "unterminated"),
            (r#"{"a": 1} x"#, "after the document"),
            ("nope", "unexpected"),
            (r#"{"a": "\q"}"#, "escape"),
        ] {
            let e = parse_json(bad.as_bytes()).err().unwrap_or_else(|| panic!("`{bad}` parsed"));
            assert!(e.contains(why), "`{bad}`: {e}");
        }
        let deep = "[".repeat(40) + &"]".repeat(40);
        assert!(parse_json(deep.as_bytes()).err().is_some_and(|e| e.contains("deeply")), "nesting is bounded");
        assert!(parse_json(&[0xff, 0xfe]).is_err());
    }

    fn pdesc(path: &str, kind: &str, default: Option<i64>) -> ParamDesc {
        ParamDesc {
            config: None,
            default,
            allowed: Vec::new(),
            map: [("float32".to_string(), 0), ("bfloat16".to_string(), 1)].into_iter().collect(),
            from_role: Some(RoleJsonDesc { role: "doc".into(), path: path.into(), kind: kind.into() }),
        }
    }

    fn doc_role(text: &str) -> Role {
        let _ = text;
        Role { name: "doc".into(), suffix: ".doc".into(), dtypes: vec![Rd::U8], required: true, rank: 1 }
    }

    fn doc_tensor(text: &str) -> Option<RoleTensor> {
        Some(RoleTensor { shape: vec![text.len()], dtype: "U8".into(), data: text.as_bytes().to_vec() })
    }

    #[test]
    fn a_parameter_read_from_a_tensor_has_a_type_a_default_and_a_range() {
        let defs = [doc_role("")];
        let t = doc_tensor(r#"{"n": 64, "dtype": "bfloat16", "off": 0.30000000000000004, "b": true, "neg": -3, "frac": 1.5, "inf": 1e999}"#);
        let get = |name: &str, p: ParamDesc| role_param("T", name, &p, &defs, std::slice::from_ref(&t));
        assert!(matches!(get("n", pdesc("n", "int", None)), Ok(Col::CI(64))));
        assert!(matches!(get("d", pdesc("dtype", "string", None)), Ok(Col::CI(1))), "a string read through the parameter's map");
        assert!(matches!(get("o", pdesc("off", "float", None)), Ok(Col::CF(v)) if v == 0.1 + 0.2));
        assert!(matches!(get("b", pdesc("b", "int", None)), Ok(Col::CI(1))), "a JSON bool is 0 or 1");
        assert!(matches!(get("neg", pdesc("neg", "int", None)), Ok(Col::CI(-3))));
        // An absent key takes the default, or is an error without one.
        assert!(matches!(get("m", pdesc("missing", "int", Some(256))), Ok(Col::CI(256))));
        assert!(matches!(get("m", pdesc("missing", "float", Some(0))), Ok(Col::CF(v)) if v == 0.0));
        assert!(get("m", pdesc("missing", "int", None)).unwrap_err().0.contains("has no `missing`"));
        // A value of the wrong type, a string the map does not know, a number that is not finite, a range the format does not allow.
        assert!(get("f", pdesc("frac", "int", None)).unwrap_err().0.contains("not an integer"));
        assert!(get("n", pdesc("dtype", "int", None)).unwrap_err().0.contains("not a int"));
        assert!(get("s", pdesc("n", "string", None)).unwrap_err().0.contains("not a string"));
        let mut p = pdesc("dtype", "string", None);
        p.map.remove("bfloat16");
        assert!(get("d", p).unwrap_err().0.contains("defined for"));
        assert!(get("i", pdesc("inf", "float", None)).unwrap_err().0.contains("not"));
        let mut p = pdesc("n", "int", None);
        p.allowed = vec![32, 128];
        assert!(get("n", p).unwrap_err().0.contains("defined for"));
        // The tensor's data is not loaded (a header-only role): an error, not a guess; no tensors at all: a placeholder for a layout-only evaluation.
        let header = Some(RoleTensor::header_only(vec![12], "U8"));
        assert!(role_param("T", "n", &pdesc("n", "int", None), &defs, std::slice::from_ref(&header)).unwrap_err().0.contains("not loaded"));
        assert!(matches!(role_param("T", "n", &pdesc("n", "int", None), &defs, &[None]), Ok(Col::CI(0))));
    }

    #[test]
    fn a_role_that_accepts_float_or_bytes_can_be_told_apart() {
        let f32s = RoleTensor { shape: vec![2], dtype: "F32".into(), data: vec![0; 8] };
        let u8s = RoleTensor { shape: vec![2], dtype: "U8".into(), data: vec![0; 2] };
        assert_eq!(is_float_role(Some(&f32s)), 1);
        assert_eq!(is_float_role(Some(&u8s)), 0);
        assert_eq!(is_float_role(None), 0);
        assert_eq!(is_float_role(Some(&RoleTensor::header_only(vec![1], "BF16"))), 1);
        assert_eq!(is_float_role(Some(&RoleTensor::header_only(vec![1], "F8_E4M3"))), 1);
        assert_eq!(is_float_role(Some(&RoleTensor::header_only(vec![1], "I8"))), 0);
    }
}
