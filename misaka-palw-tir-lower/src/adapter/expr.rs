//! **The adapter expression language** (`misaka.palw.model-adapter.v1`).
//!
//! An adapter is data: JSON in which an object with exactly one key that starts with `$` is an
//! *operator node* and every other object, array and scalar is a literal (objects and arrays are
//! evaluated element-wise, so a template for a `ModelSpec` is a `ModelSpec` with operator nodes where
//! a value depends on the configuration). The language is deliberately small, pure and total:
//!
//! * no I/O, no recursion by name, no unbounded loop: evaluation takes at most [`MAX_STEPS`] nodes,
//!   [`MAX_DEPTH`] levels, lists of at most [`MAX_LIST`] elements;
//! * every read of the configuration goes through the key-tracking [`Cfg`], so a key no rule reads
//!   is refused (`NOT_LOWERABLE`), never ignored;
//! * numbers keep their integer-ness (`$add` of two integers is an integer); `$div` is a float
//!   division, `$idiv` a floor division.
//!
//! | group | operators |
//! | --- | --- |
//! | configuration | `$cfg` (`"key"` or `["key", default]`), `$cfg?` (null if absent), `$cfgn` (`["key", default]`: absent → default, explicit null → null), `$alias` (`[["k1","k2"], default?]`), `$has`, `$root` (the wrapper config of a VLM), `$forbid` (`["key", why]`), `$require_eq` (`["key", value, why]`), `$get` (`[object, "key", default?]`) |
//! | tensors (when a tensor index is given) | `$has_tensor` (true / false / null if unknown), `$tensor_flag` (`[name, default]`: present?, else the default, recorded as assumed), `$tensor_shape`, `$tensor_prefix` (`{candidates, probe, default}`) |
//! | variables | `$var`, `$let` (`["name", value, body]`) |
//! | arithmetic | `$add $sub $mul $div $idiv $mod $neg $abs $min $max $pow $sqrt $ln $exp $floor $ceil $round $int $float` |
//! | logic | `$eq $ne $lt $le $gt $ge $and $or $not $if` (`[c, a, b]`) `$switch` (`[value, {case: result}, default]`) |
//! | lists, objects and strings | `$list $range $len $index $contains $concat $flatten $repeat $map $is_num` (`[list, "name", body]`) `$sum $cat $starts_with $ends_with $merge $omit $set` |
//! | checks | `$check` (`[cond, message]`: NOT_LOWERABLE if false), `$bad` (`[cond, message]`: bad config) |
//! | generic features | `$act` (an HF activation name → `Act`), `$rope` / `$rope_temp` (a rope spec / query temperature from the config's rope fields), `$rope_plain` (the default frequencies at a given base), `$alibi` (ALiBi slopes), `$scope` (`{key, inert, body}`: evaluate with the configuration narrowed to a nested object), `$partial_rotary` (the effective partial-rotary factor), `$layers` (`{count, each}`), `$layer_types` (`{n, allowed, each}`: the config's `layer_types`, validated, or the class's own rule) |

use crate::cfg::Cfg;
use crate::error::{LowerError, Result};
use crate::hf_schema::TensorIndex;
use serde_json::{Map, Number, Value};
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;

pub const MAX_STEPS: usize = 4_000_000;
pub const MAX_DEPTH: usize = 96;
pub const MAX_LIST: usize = 1 << 20;
pub const MAX_LAYERS: usize = 4096;


/// The configuration scope in force: the decoder's, or a nested object's inside a `$sub`.
pub enum CfgRef<'a> {
    Base(&'a Cfg<'a>),
    Nested(std::rc::Rc<Cfg<'a>>),
}

impl<'a> std::ops::Deref for CfgRef<'a> {
    type Target = Cfg<'a>;
    fn deref(&self) -> &Cfg<'a> {
        match self {
            CfgRef::Base(b) => b,
            CfgRef::Nested(n) => n,
        }
    }
}

/// Everything an expression can read.
pub struct Env<'a> {
    /// The decoder's configuration (key-tracked).
    base: &'a Cfg<'a>,
    /// Nested configuration scopes (`$scope`), innermost last.
    scopes: RefCell<Vec<std::rc::Rc<Cfg<'a>>>>,
    /// The wrapper configuration of a VLM, when the decoder lives in a nested object.
    pub root: Option<&'a Cfg<'a>>,
    /// HF config-class defaults: used when a key is absent from the configuration.
    pub defaults: &'a Map<String, Value>,
    pub tensors: Option<&'a TensorIndex>,
    /// `architectures[0]`, for messages.
    pub arch: &'a str,
    /// Scoped bindings (`$let`, `$map`, the layer index).
    vars: RefCell<Vec<(String, Value)>>,
    /// Global variables: evaluated on first use (dependencies in any order), memoised.
    globals_pending: RefCell<std::collections::BTreeMap<String, Value>>,
    globals_done: RefCell<std::collections::BTreeMap<String, Value>>,
    /// Per-layer variables (`"layer": true`): expressions, evaluated afresh for each layer of a
    /// `$layers`, on first use within it.
    layer_exprs: RefCell<Vec<(String, Value, bool)>>,
    layer_pending: RefCell<std::collections::BTreeMap<String, Value>>,
    layer_done: RefCell<std::collections::BTreeMap<String, Value>>,
    in_progress: RefCell<BTreeSet<String>>,
    steps: Cell<usize>,
    /// Keys whose value is the class default, not the configuration's.
    pub assumed: RefCell<BTreeSet<String>>,
}

fn bad(msg: impl Into<String>) -> LowerError {
    LowerError::bad(msg)
}

fn num(v: &Value) -> Option<N> {
    match v {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(N::I(i))
            } else if let Some(u) = n.as_u64() {
                i64::try_from(u).ok().map(N::I).or(Some(N::F(u as f64)))
            } else {
                n.as_f64().map(N::F)
            }
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
enum N {
    I(i64),
    F(f64),
}

impl N {
    fn f(self) -> f64 {
        match self {
            N::I(i) => i as f64,
            N::F(f) => f,
        }
    }
    fn value(self) -> Result<Value> {
        match self {
            N::I(i) => Ok(Value::from(i)),
            N::F(f) => Number::from_f64(f).map(Value::Number).ok_or_else(|| bad("an expression produced a non-finite number")),
        }
    }
}

/// Integral floats read as integers (a `16.0` where a count is wanted), everything else as is.
pub fn normalize(v: Value) -> Value {
    match v {
        Value::Number(n) => {
            if n.is_f64()
                && let Some(f) = n.as_f64()
                && f.fract() == 0.0
                && f.abs() < 9.0e15
            {
                Value::from(f as i64)
            } else {
                Value::Number(n)
            }
        }
        Value::Array(a) => Value::Array(a.into_iter().map(normalize).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalize(v))).collect()),
        other => other,
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (num(a), num(b)) {
        (Some(x), Some(y)) => x.f() == y.f(),
        _ => match (a, b) {
            (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| values_equal(p, q)),
            _ => a == b,
        },
    }
}

fn as_bool(v: &Value, op: &str) -> Result<bool> {
    v.as_bool().ok_or_else(|| bad(format!("`{op}` wants a boolean, got {v}")))
}

fn as_usize(v: &Value, op: &str) -> Result<usize> {
    match num(v) {
        Some(N::I(i)) if i >= 0 => Ok(i as usize),
        Some(N::F(f)) if f >= 0.0 && f.fract() == 0.0 && f < 9.0e15 => Ok(f as usize),
        _ => Err(bad(format!("`{op}` wants a non-negative integer, got {v}"))),
    }
}

fn as_str<'v>(v: &'v Value, op: &str) -> Result<&'v str> {
    v.as_str().ok_or_else(|| bad(format!("`{op}` wants a string, got {v}")))
}

/// The operator of a node: `Some((op, arg))` for an object with exactly one key starting with `$`.
pub fn operator(v: &Value) -> Option<(&str, &Value)> {
    let o = v.as_object()?;
    if o.len() == 1 {
        let (k, a) = o.iter().next()?;
        if k.starts_with('$') && k.len() > 1 {
            return Some((k.as_str(), a));
        }
    }
    None
}

impl<'a> Env<'a> {
    pub fn new(
        cfg: &'a Cfg<'a>,
        root: Option<&'a Cfg<'a>>,
        defaults: &'a Map<String, Value>,
        tensors: Option<&'a TensorIndex>,
        arch: &'a str,
    ) -> Self {
        Env {
            base: cfg,
            scopes: RefCell::new(Vec::new()),
            root,
            defaults,
            tensors,
            arch,
            vars: RefCell::new(Vec::new()),
            globals_pending: RefCell::new(Default::default()),
            globals_done: RefCell::new(Default::default()),
            layer_exprs: RefCell::new(Vec::new()),
            layer_pending: RefCell::new(Default::default()),
            layer_done: RefCell::new(Default::default()),
            in_progress: RefCell::new(BTreeSet::new()),
            steps: Cell::new(0),
            assumed: RefCell::new(BTreeSet::new()),
        }
    }

    fn cur(&self) -> CfgRef<'a> {
        match self.scopes.borrow().last() {
            Some(n) => CfgRef::Nested(n.clone()),
            None => CfgRef::Base(self.base),
        }
    }

    /// Declare a per-layer variable (an expression evaluated for each layer, `i` bound, when first
    /// read or — `force` — for every layer).
    pub fn add_layer_var(&self, name: &str, expr: Value, force: bool) {
        self.layer_exprs.borrow_mut().push((name.to_string(), expr, force));
    }

    /// Declare a global variable; it is evaluated when first read, or by [`Env::force_globals`].
    pub fn define_global(&self, name: &str, expr: Value) {
        self.globals_pending.borrow_mut().insert(name.to_string(), expr);
    }

    /// Bind a global variable to a value.
    pub fn set_var(&self, name: &str, v: Value) {
        self.globals_done.borrow_mut().insert(name.to_string(), v);
    }

    /// Evaluate the named globals now (the adapter's `"force": true` variables: checks and
    /// refusals that no other expression reads).
    pub fn force_globals(&self, order: &[String]) -> Result<()> {
        for n in order {
            self.global(n)?;
        }
        Ok(())
    }

    fn global(&self, name: &str) -> Result<Value> {
        if let Some(v) = self.globals_done.borrow().get(name) {
            return Ok(v.clone());
        }
        let expr = self.globals_pending.borrow().get(name).cloned();
        let expr = expr.ok_or_else(|| bad(format!("{}: the adapter reads variable `{name}`, which it never defines", self.arch)))?;
        if !self.in_progress.borrow_mut().insert(format!("g:{name}")) {
            return Err(bad(format!("{}: variable `{name}` is defined in terms of itself", self.arch)));
        }
        let r = self.ev(&expr, 1);
        self.in_progress.borrow_mut().remove(&format!("g:{name}"));
        let v = r?;
        self.globals_done.borrow_mut().insert(name.to_string(), v.clone());
        Ok(v)
    }

    fn var(&self, name: &str) -> Result<Value> {
        if let Some((_, v)) = self.vars.borrow().iter().rev().find(|(n, _)| n == name) {
            return Ok(v.clone());
        }
        if let Some(v) = self.layer_done.borrow().get(name) {
            return Ok(v.clone());
        }
        let lexpr = self.layer_pending.borrow().get(name).cloned();
        if let Some(expr) = lexpr {
            if !self.in_progress.borrow_mut().insert(format!("l:{name}")) {
                return Err(bad(format!("{}: layer variable `{name}` is defined in terms of itself", self.arch)));
            }
            let r = self.ev(&expr, 1);
            self.in_progress.borrow_mut().remove(&format!("l:{name}"));
            let v = r?;
            self.layer_done.borrow_mut().insert(name.to_string(), v.clone());
            return Ok(v);
        }
        self.global(name)
    }

    fn with_var<T>(&self, name: &str, v: Value, f: impl FnOnce() -> Result<T>) -> Result<T> {
        self.vars.borrow_mut().push((name.to_string(), v));
        let r = f();
        self.vars.borrow_mut().pop();
        r
    }

    /// A configuration value, or the class default; `None` when neither (explicit `null` reads as absent).
    fn lookup(&self, key: &str) -> Option<Value> {
        match self.cur().raw(key) {
            Some(v) => Some(v.clone()),
            None => self.defaults.get(key).filter(|v| !v.is_null()).map(|v| {
                self.assumed.borrow_mut().insert(key.to_string());
                v.clone()
            }),
        }
    }

    /// Evaluate a template or expression.
    pub fn eval(&self, v: &Value) -> Result<Value> {
        self.ev(v, 0)
    }

    fn ev(&self, v: &Value, depth: usize) -> Result<Value> {
        if depth > MAX_DEPTH {
            return Err(bad("adapter expression nested too deeply"));
        }
        let s = self.steps.get() + 1;
        self.steps.set(s);
        if s > MAX_STEPS {
            return Err(bad("adapter expression exceeds its evaluation budget"));
        }
        if let Some((op, arg)) = operator(v) {
            return self.op(op, arg, depth);
        }
        match v {
            Value::Array(a) => {
                if a.len() > MAX_LIST {
                    return Err(bad("adapter list too long"));
                }
                a.iter().map(|e| self.ev(e, depth + 1)).collect::<Result<Vec<_>>>().map(Value::Array)
            }
            Value::Object(o) => {
                let mut out = Map::new();
                for (k, e) in o {
                    if k.starts_with('$') {
                        return Err(bad(format!("`{k}` is an operator and must be the only key of its object")));
                    }
                    out.insert(k.clone(), self.ev(e, depth + 1)?);
                }
                Ok(Value::Object(out))
            }
            other => Ok(other.clone()),
        }
    }

    fn args(&self, arg: &Value, depth: usize) -> Result<Vec<Value>> {
        match arg {
            Value::Array(a) => a.iter().map(|e| self.ev(e, depth + 1)).collect(),
            other => Ok(vec![self.ev(other, depth + 1)?]),
        }
    }

    fn arity(&self, op: &str, a: &[Value], n: usize) -> Result<()> {
        if a.len() != n {
            return Err(bad(format!("`{op}` takes {n} argument(s), got {}", a.len())));
        }
        Ok(())
    }

    fn nums(&self, op: &str, a: &[Value]) -> Result<Vec<N>> {
        a.iter().map(|v| num(v).ok_or_else(|| bad(format!("`{op}` wants numbers, got {v}")))).collect()
    }

    #[allow(clippy::too_many_lines)]
    fn op(&self, op: &str, arg: &Value, depth: usize) -> Result<Value> {
        let d = depth + 1;
        match op {
            // ───────────── configuration ─────────────
            "$cfg" => {
                let (key, default) = match arg {
                    Value::String(k) => (k.as_str(), None),
                    Value::Array(a) if a.len() == 2 => (as_str(&a[0], "$cfg")?, Some(&a[1])),
                    _ => return Err(bad("`$cfg` takes a key or [key, default]")),
                };
                match self.lookup(key) {
                    Some(v) => Ok(v),
                    None => match default {
                        Some(e) => self.ev(e, d),
                        None => Err(bad(format!("{}: `{key}` is required", self.arch))),
                    },
                }
            }
            // The configuration's own value or null: never a class default (an optional key has none).
            "$cfg?" => Ok(self.cur().raw(as_str(arg, "$cfg?")?).cloned().unwrap_or(Value::Null)),
            "$cfgn" => {
                // `usize_or_null`: absent → the class default; an explicit `null` stays null (a
                // checkpoint switches `sliding_window` off by writing null).
                let (key, inline) = match arg {
                    Value::String(k) => (k.as_str(), None),
                    Value::Array(a) if a.len() == 2 => (as_str(&a[0], "$cfgn")?, Some(&a[1])),
                    _ => return Err(bad("`$cfgn` takes a key or [key, default]")),
                };
                match self.cur().raw_nullable(key) {
                    Some(Some(v)) => Ok(v.clone()),
                    Some(None) => Ok(Value::Null),
                    None => match self.defaults.get(key).filter(|v| !v.is_null()) {
                        Some(v) => {
                            self.assumed.borrow_mut().insert(key.to_string());
                            Ok(v.clone())
                        }
                        None => match inline {
                            Some(e) => self.ev(e, d),
                            None => Ok(Value::Null),
                        },
                    },
                }
            }
            "$alias" => {
                let (keys, default) = match arg {
                    Value::Array(a) if !a.is_empty() && a.len() <= 2 => (&a[0], a.get(1)),
                    _ => return Err(bad("`$alias` takes [[keys…], default?]")),
                };
                let keys: Vec<&str> =
                    keys.as_array().ok_or_else(|| bad("`$alias` wants a list of keys"))?.iter().filter_map(Value::as_str).collect();
                let mut out: Option<Value> = None;
                for k in &keys {
                    // The configuration's own values only: a class default for one spelling must
                    // not "disagree" with the value another spelling carries.
                    let v = self.cur().raw(k).cloned();
                    match (&out, v) {
                        (None, Some(v)) => out = Some(v),
                        (Some(a), Some(b)) if !values_equal(a, &b) => {
                            return Err(bad(format!("{}: aliases {keys:?} disagree: {a} vs {b}", self.arch)));
                        }
                        _ => {}
                    }
                }
                match (out, default) {
                    (Some(v), _) => Ok(v),
                    (None, Some(e)) => {
                        if let Some(first) = keys.first() {
                            self.assumed.borrow_mut().insert((*first).to_string());
                        }
                        self.ev(e, d)
                    }
                    (None, None) => Err(bad(format!("{}: one of {keys:?} is required", self.arch))),
                }
            }
            "$has" => Ok(Value::Bool(self.cur().has(as_str(arg, "$has")?))),
            "$root" => {
                let r = self.root.ok_or_else(|| bad("`$root` in an adapter with no wrapper configuration"))?;
                Ok(r.raw(as_str(arg, "$root")?).cloned().unwrap_or(Value::Null))
            }
            "$forbid" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 2)?;
                self.cur().forbid(as_str(&a[0], op)?, as_str(&a[1], op)?)?;
                Ok(Value::Bool(true))
            }
            "$require_eq" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 3)?;
                self.cur().require_eq(as_str(&a[0], op)?, &a[1], as_str(&a[2], op)?)?;
                Ok(Value::Bool(true))
            }
            "$get" => {
                let a = self.args(arg, d)?;
                if a.len() < 2 || a.len() > 3 {
                    return Err(bad("`$get` takes [object, key, default?]"));
                }
                let key = as_str(&a[1], op)?;
                match a[0].as_object().and_then(|o| o.get(key)).filter(|v| !v.is_null()) {
                    Some(v) => Ok(v.clone()),
                    None => Ok(a.get(2).cloned().unwrap_or(Value::Null)),
                }
            }
            // ───────────── tensors ─────────────
            "$has_tensor" => {
                let name = as_str(&self.ev(arg, d)?, op)?.to_string();
                Ok(match self.tensors {
                    Some(t) => Value::Bool(t.has(&name)),
                    None => Value::Null,
                })
            }
            "$tensor_flag" => {
                // Whether the checkpoint has a tensor: from the tensor index when there is one;
                // else the stated default, recorded as an assumption (a configuration alone cannot
                // say whether a projection carries a bias or a norm exists).
                let a = self.args(arg, d)?;
                self.arity(op, &a, 2)?;
                let name = as_str(&a[0], op)?;
                let default = as_bool(&a[1], op)?;
                Ok(Value::Bool(match self.tensors {
                    Some(t) => t.has(name),
                    None => {
                        self.assumed.borrow_mut().insert(format!("tensor `{name}` assumed {}", if default { "present" } else { "absent" }));
                        default
                    }
                }))
            }
            "$tensor_shape" => {
                let name = as_str(&self.ev(arg, d)?, op)?.to_string();
                Ok(match self.tensors.and_then(|t| t.shape(&name)) {
                    Some(s) => Value::Array(s.iter().map(|x| Value::from(*x as u64)).collect()),
                    None => Value::Null,
                })
            }
            "$tensor_prefix" => {
                let o = arg.as_object().ok_or_else(|| bad("`$tensor_prefix` takes {candidates, probe, default}"))?;
                let cands = self.ev(o.get("candidates").ok_or_else(|| bad("`$tensor_prefix`: candidates"))?, d)?;
                let probe = as_str(&self.ev(o.get("probe").ok_or_else(|| bad("`$tensor_prefix`: probe"))?, d)?, op)?.to_string();
                let default = self.ev(o.get("default").ok_or_else(|| bad("`$tensor_prefix`: default"))?, d)?;
                if let Some(t) = self.tensors {
                    for c in cands.as_array().ok_or_else(|| bad("`$tensor_prefix`: candidates is a list"))? {
                        let c = as_str(c, op)?;
                        if t.has(&format!("{c}{probe}")) {
                            return Ok(Value::String(c.to_string()));
                        }
                    }
                }
                Ok(default)
            }
            // ───────────── variables ─────────────
            "$var" => self.var(as_str(arg, "$var")?),
            "$let" => {
                let a = arg.as_array().filter(|a| a.len() == 3).ok_or_else(|| bad("`$let` takes [name, value, body]"))?;
                let name = as_str(&a[0], op)?;
                let v = self.ev(&a[1], d)?;
                self.with_var(name, v, || self.ev(&a[2], d))
            }
            // ───────────── arithmetic ─────────────
            "$add" | "$sub" | "$mul" => {
                let a = self.args(arg, d)?;
                if a.len() < 2 {
                    return Err(bad(format!("`{op}` takes at least two arguments")));
                }
                let ns = self.nums(op, &a)?;
                let mut acc = ns[0];
                for x in &ns[1..] {
                    acc = match (acc, *x) {
                        (N::I(p), N::I(q)) => {
                            let r = match op {
                                "$add" => p.checked_add(q),
                                "$sub" => p.checked_sub(q),
                                _ => p.checked_mul(q),
                            };
                            N::I(r.ok_or_else(|| bad("integer overflow in an adapter expression"))?)
                        }
                        (p, q) => N::F(match op {
                            "$add" => p.f() + q.f(),
                            "$sub" => p.f() - q.f(),
                            _ => p.f() * q.f(),
                        }),
                    };
                }
                acc.value()
            }
            "$div" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 2)?;
                let ns = self.nums(op, &a)?;
                if ns[1].f() == 0.0 {
                    return Err(bad("division by zero in an adapter expression"));
                }
                N::F(ns[0].f() / ns[1].f()).value()
            }
            "$idiv" | "$mod" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 2)?;
                let (x, y) = (as_i64(&a[0], op)?, as_i64(&a[1], op)?);
                if y == 0 {
                    return Err(bad("division by zero in an adapter expression"));
                }
                Ok(Value::from(if op == "$idiv" { x.div_euclid(y) } else { x.rem_euclid(y) }))
            }
            "$neg" | "$abs" | "$sqrt" | "$ln" | "$exp" | "$floor" | "$ceil" | "$round" | "$int" | "$float" => {
                let v = self.ev(arg, d)?;
                let n = num(&v).ok_or_else(|| bad(format!("`{op}` wants a number, got {v}")))?;
                match op {
                    "$neg" => match n {
                        N::I(i) => Ok(Value::from(i.checked_neg().ok_or_else(|| bad("integer overflow"))?)),
                        N::F(f) => N::F(-f).value(),
                    },
                    "$abs" => match n {
                        N::I(i) => Ok(Value::from(i.checked_abs().ok_or_else(|| bad("integer overflow"))?)),
                        N::F(f) => N::F(f.abs()).value(),
                    },
                    "$sqrt" => {
                        if n.f() < 0.0 {
                            return Err(bad("`$sqrt` of a negative number"));
                        }
                        N::F(n.f().sqrt()).value()
                    }
                    "$ln" => {
                        if n.f() <= 0.0 {
                            return Err(bad("`$ln` of a non-positive number"));
                        }
                        N::F(n.f().ln()).value()
                    }
                    "$exp" => N::F(n.f().exp()).value(),
                    "$floor" => N::I(n.f().floor() as i64).value(),
                    "$ceil" => N::I(n.f().ceil() as i64).value(),
                    "$round" => N::I(n.f().round() as i64).value(),
                    "$int" => match n {
                        N::I(i) => Ok(Value::from(i)),
                        N::F(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => Ok(Value::from(f as i64)),
                        N::F(f) => Err(bad(format!("`$int` of the non-integer {f}"))),
                    },
                    _ => N::F(n.f()).value(),
                }
            }
            "$min" | "$max" => {
                let a = self.args(arg, d)?;
                if a.is_empty() {
                    return Err(bad(format!("`{op}` takes at least one argument")));
                }
                let ns = self.nums(op, &a)?;
                let mut best = 0;
                for (i, x) in ns.iter().enumerate() {
                    let better = if op == "$min" { x.f() < ns[best].f() } else { x.f() > ns[best].f() };
                    if better {
                        best = i;
                    }
                }
                ns[best].value()
            }
            "$pow" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 2)?;
                let ns = self.nums(op, &a)?;
                match (ns[0], ns[1]) {
                    (N::I(b), N::I(e)) if (0..=62).contains(&e) => {
                        Ok(Value::from(b.checked_pow(e as u32).ok_or_else(|| bad("integer overflow in `$pow`"))?))
                    }
                    (b, e) => N::F(b.f().powf(e.f())).value(),
                }
            }
            // ───────────── comparison and logic ─────────────
            "$eq" | "$ne" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 2)?;
                let e = values_equal(&a[0], &a[1]);
                Ok(Value::Bool(if op == "$eq" { e } else { !e }))
            }
            "$lt" | "$le" | "$gt" | "$ge" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 2)?;
                let ord = match (num(&a[0]), num(&a[1])) {
                    (Some(x), Some(y)) => x.f().partial_cmp(&y.f()),
                    _ => match (&a[0], &a[1]) {
                        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
                        _ => return Err(bad(format!("`{op}` compares numbers or strings, got {} and {}", a[0], a[1]))),
                    },
                }
                .ok_or_else(|| bad("comparison of an undefined number"))?;
                Ok(Value::Bool(match op {
                    "$lt" => ord.is_lt(),
                    "$le" => ord.is_le(),
                    "$gt" => ord.is_gt(),
                    _ => ord.is_ge(),
                }))
            }
            "$and" | "$or" => {
                let a = arg.as_array().ok_or_else(|| bad(format!("`{op}` takes a list")))?;
                for e in a {
                    let b = as_bool(&self.ev(e, d)?, op)?;
                    if op == "$and" && !b {
                        return Ok(Value::Bool(false));
                    }
                    if op == "$or" && b {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(op == "$and"))
            }
            "$not" => Ok(Value::Bool(!as_bool(&self.ev(arg, d)?, op)?)),
            "$if" => {
                let a = arg.as_array().filter(|a| a.len() == 3).ok_or_else(|| bad("`$if` takes [cond, then, else]"))?;
                if as_bool(&self.ev(&a[0], d)?, op)? { self.ev(&a[1], d) } else { self.ev(&a[2], d) }
            }
            "$switch" => {
                let a = arg.as_array().filter(|a| a.len() == 3).ok_or_else(|| bad("`$switch` takes [value, {case: result}, default]"))?;
                let v = self.ev(&a[0], d)?;
                let key = match &v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "null".to_string(),
                    other => return Err(bad(format!("`$switch` on {other}"))),
                };
                match a[1].as_object().and_then(|m| m.get(&key)) {
                    Some(e) => self.ev(e, d),
                    None => self.ev(&a[2], d),
                }
            }
            // ───────────── lists and strings ─────────────
            "$list" => Ok(Value::Array(self.args(arg, d)?)),
            "$range" => {
                let n = as_usize(&self.ev(arg, d)?, op)?;
                if n > MAX_LIST {
                    return Err(bad("`$range` too long"));
                }
                Ok(Value::Array((0..n as u64).map(Value::from).collect()))
            }
            "$len" => {
                let v = self.ev(arg, d)?;
                match &v {
                    Value::Array(a) => Ok(Value::from(a.len() as u64)),
                    Value::String(s) => Ok(Value::from(s.chars().count() as u64)),
                    Value::Object(o) => Ok(Value::from(o.len() as u64)),
                    other => Err(bad(format!("`$len` of {other}"))),
                }
            }
            "$index" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 2)?;
                let l = a[0].as_array().ok_or_else(|| bad("`$index` wants a list"))?;
                let i = as_usize(&a[1], op)?;
                l.get(i).cloned().ok_or_else(|| bad(format!("`$index` {i} of a list of {}", l.len())))
            }
            "$contains" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 2)?;
                match &a[0] {
                    Value::Array(l) => Ok(Value::Bool(l.iter().any(|x| values_equal(x, &a[1])))),
                    Value::String(s) => Ok(Value::Bool(s.contains(as_str(&a[1], op)?))),
                    other => Err(bad(format!("`$contains` on {other}"))),
                }
            }
            "$is_num" => Ok(Value::Bool(num(&self.ev(arg, d)?).is_some())),
            "$flatten" => {
                let v = self.ev(arg, d)?;
                let mut out = Vec::new();
                for l in v.as_array().ok_or_else(|| bad("`$flatten` wants a list of lists"))? {
                    out.extend(l.as_array().ok_or_else(|| bad("`$flatten` wants a list of lists"))?.iter().cloned());
                }
                if out.len() > MAX_LIST {
                    return Err(bad("`$flatten` too long"));
                }
                Ok(Value::Array(out))
            }
            "$concat" => {
                let a = self.args(arg, d)?;
                let mut out = Vec::new();
                for l in &a {
                    out.extend(l.as_array().ok_or_else(|| bad("`$concat` wants lists"))?.iter().cloned());
                }
                if out.len() > MAX_LIST {
                    return Err(bad("`$concat` too long"));
                }
                Ok(Value::Array(out))
            }
            "$repeat" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 2)?;
                let n = as_usize(&a[1], op)?;
                if n > MAX_LIST {
                    return Err(bad("`$repeat` too long"));
                }
                Ok(Value::Array(vec![a[0].clone(); n]))
            }
            "$map" => {
                let a = arg.as_array().filter(|a| a.len() == 3).ok_or_else(|| bad("`$map` takes [list, name, body]"))?;
                let l = self.ev(&a[0], d)?;
                let name = as_str(&a[1], op)?;
                let items = l.as_array().ok_or_else(|| bad("`$map` wants a list"))?;
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    out.push(self.with_var(name, it.clone(), || self.ev(&a[2], d))?);
                }
                Ok(Value::Array(out))
            }
            "$sum" => {
                let l = self.ev(arg, d)?;
                let items = l.as_array().ok_or_else(|| bad("`$sum` wants a list"))?;
                let ns = self.nums(op, items)?;
                let mut acc = N::I(0);
                for x in ns {
                    acc = match (acc, x) {
                        (N::I(p), N::I(q)) => N::I(p.checked_add(q).ok_or_else(|| bad("integer overflow"))?),
                        (p, q) => N::F(p.f() + q.f()),
                    };
                }
                acc.value()
            }
            "$cat" => {
                let a = self.args(arg, d)?;
                let mut s = String::new();
                for v in &a {
                    match v {
                        Value::String(x) => s.push_str(x),
                        Value::Number(n) => s.push_str(&n.to_string()),
                        Value::Bool(b) => s.push_str(&b.to_string()),
                        other => return Err(bad(format!("`$cat` of {other}"))),
                    }
                }
                Ok(Value::String(s))
            }
            "$starts_with" | "$ends_with" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 2)?;
                let (s, p) = (as_str(&a[0], op)?, as_str(&a[1], op)?);
                Ok(Value::Bool(if op == "$starts_with" { s.starts_with(p) } else { s.ends_with(p) }))
            }
            "$merge" => {
                // Shallow merge of objects, later wins.
                let a = self.args(arg, d)?;
                let mut out = Map::new();
                for o in &a {
                    for (k, v) in o.as_object().ok_or_else(|| bad("`$merge` wants objects"))? {
                        out.insert(k.clone(), v.clone());
                    }
                }
                Ok(Value::Object(out))
            }
            "$omit" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 2)?;
                let mut o = a[0].as_object().cloned().ok_or_else(|| bad("`$omit` wants an object"))?;
                for k in a[1].as_array().ok_or_else(|| bad("`$omit` wants a list of keys"))? {
                    o.remove(as_str(k, op)?);
                }
                Ok(Value::Object(o))
            }
            "$set" => {
                let a = self.args(arg, d)?;
                self.arity(op, &a, 3)?;
                let mut o = a[0].as_object().cloned().ok_or_else(|| bad("`$set` wants an object"))?;
                o.insert(as_str(&a[1], op)?.to_string(), a[2].clone());
                Ok(Value::Object(o))
            }
            // ───────────── checks ─────────────
            "$check" | "$bad" => {
                let a = arg.as_array().filter(|a| a.len() == 2).ok_or_else(|| bad(format!("`{op}` takes [condition, message]")))?;
                if as_bool(&self.ev(&a[0], d)?, op)? {
                    Ok(Value::Bool(true))
                } else {
                    let m = format!("{}: {}", self.arch, as_str(&self.ev(&a[1], d)?, op)?);
                    Err(if op == "$check" { LowerError::not_lowerable(m) } else { LowerError::bad(m) })
                }
            }
            // ───────────── generic features ─────────────
            "$act" => {
                let v = self.ev(arg, d)?;
                let name = as_str(&v, op)?;
                let a = crate::spec::Act::from_hf(name)
                    .ok_or_else(|| LowerError::not_lowerable(format!("{}: activation `{name}` is not modelled", self.arch)))?;
                serde_json::to_value(a).map_err(|e| bad(e.to_string()))
            }
            "$layer_types" => {
                // `layer_types` (validated: one per layer, each allowed) or, when the config has
                // none, the class's own rule `each` (variable `i` = the layer).
                let o = arg.as_object().ok_or_else(|| bad("`$layer_types` takes {n, allowed, each}"))?;
                let n = as_usize(&self.ev(o.get("n").ok_or_else(|| bad("`$layer_types`: n"))?, d)?, op)?;
                let allowed: Vec<String> = self
                    .ev(o.get("allowed").ok_or_else(|| bad("`$layer_types`: allowed"))?, d)?
                    .as_array()
                    .ok_or_else(|| bad("`$layer_types`: allowed is a list"))?
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect();
                match self.cur().raw("layer_types") {
                    Some(Value::Array(a)) => {
                        if a.len() != n {
                            return Err(bad(format!("{}: layer_types has {} entries for {n} layers", self.arch, a.len())));
                        }
                        for t in a {
                            let t = t.as_str().ok_or_else(|| bad(format!("{}: layer_types holds a non-string", self.arch)))?;
                            if !allowed.iter().any(|x| x == t) {
                                return Err(LowerError::not_lowerable(format!("{}: layer type `{t}` is not modelled", self.arch)));
                            }
                        }
                        Ok(Value::Array(a.clone()))
                    }
                    Some(_) => Err(bad(format!("{}: `layer_types` is not a list", self.arch))),
                    None => {
                        let each = o.get("each").ok_or_else(|| bad("`$layer_types`: each"))?;
                        if n > MAX_LAYERS {
                            return Err(LowerError::not_lowerable(format!("{n} layers")));
                        }
                        let mut out = Vec::with_capacity(n);
                        for i in 0..n {
                            out.push(self.with_var("i", Value::from(i as u64), || self.ev(each, d))?);
                        }
                        Ok(Value::Array(out))
                    }
                }
            }
            "$scope" => {
                // Evaluate `body` with the configuration narrowed to a nested object (MPT's
                // attn_config): its own key tracking, refused leftovers and all.
                let o = arg.as_object().ok_or_else(|| bad("`$scope` takes {key, inert?, body}"))?;
                let key = as_str(&self.ev(o.get("key").ok_or_else(|| bad("`$scope`: key"))?, d)?, op)?.to_string();
                let Some(map) = self.cur().opt_obj(&key)? else {
                    // Absent: the body is not evaluated; its value is null.
                    return Ok(Value::Null);
                };
                let nested = std::rc::Rc::new(Cfg::new(self.arch.to_string(), map, key));
                if let Some(Value::Array(a)) = o.get("inert") {
                    let keys: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
                    nested.inert(&keys);
                }
                self.scopes.borrow_mut().push(nested.clone());
                let r = self.ev(o.get("body").ok_or_else(|| bad("`$scope`: body"))?, d);
                self.scopes.borrow_mut().pop();
                let v = r?;
                nested.finish()?;
                Ok(v)
            }
            "$alibi" => {
                // ALiBi slopes (`kind`: "bloom" or "mpt"), as the position term of an attention.
                let o = arg.as_object().ok_or_else(|| bad("`$alibi` takes {heads, kind, ...}"))?;
                let heads = as_usize(&self.ev(o.get("heads").ok_or_else(|| bad("`$alibi`: heads"))?, d)?, op)?;
                let kind = as_str(&self.ev(o.get("kind").ok_or_else(|| bad("`$alibi`: kind"))?, d)?, op)?.to_string();
                let slopes = match kind.as_str() {
                    "bloom" => crate::rope::alibi_slopes_bloom(heads),
                    "mpt" => {
                        let m = num(&self.ev(o.get("bias_max").ok_or_else(|| bad("`$alibi`: bias_max"))?, d)?).ok_or_else(|| bad("bias_max"))?.f();
                        crate::rope::alibi_slopes_mpt(heads, m)
                    }
                    other => return Err(bad(format!("`$alibi`: unknown slope kind `{other}`"))),
                };
                let flag = |k: &str| -> Result<bool> { o.get(k).map(|e| self.ev(e, d).and_then(|v| as_bool(&v, op))).transpose().map(|b| b.unwrap_or(false)) };
                let spec = crate::rope::AlibiSpec { slopes, scaled_by_softmax_scale: flag("scaled_by_softmax_scale")?, bf16_bias: flag("bf16_bias")? };
                serde_json::to_value(&spec).map_err(|e| bad(e.to_string()))
            }
            "$rope_plain" => {
                // A rope with the default frequencies at a given base, reading no rope key.
                let o = arg.as_object().ok_or_else(|| bad("`$rope_plain` takes {dim, theta, style?}"))?;
                let dim = as_usize(&self.ev(o.get("dim").ok_or_else(|| bad("`$rope_plain`: dim"))?, d)?, op)?;
                let theta = num(&self.ev(o.get("theta").ok_or_else(|| bad("`$rope_plain`: theta"))?, d)?).ok_or_else(|| bad("rope theta"))?.f();
                let style = match o.get("style").map(|e| self.ev(e, d)).transpose()? {
                    Some(Value::String(s)) if s == "Interleaved" => crate::rope::RopeStyle::Interleaved,
                    _ => crate::rope::RopeStyle::Half,
                };
                if dim == 0 || dim % 2 != 0 {
                    return Err(bad(format!("{}: rotary dim {dim} must be even and positive", self.arch)));
                }
                let spec = crate::rope::RopeSpec { rotary_dim: dim, offset: 0, style, freqs: crate::rope::RopeFreqs::plain(theta, dim) };
                serde_json::to_value(&spec).map_err(|e| bad(e.to_string()))
            }
            "$partial_rotary" => {
                // The effective partial-rotary factor: `rope_parameters.partial_rotary_factor`
                // (transformers 5), else the first of the legacy keys present, else the default.
                let o = arg.as_object().ok_or_else(|| bad("`$partial_rotary` takes {legacy, default}"))?;
                let legacy: Vec<String> = match o.get("legacy") {
                    Some(e) => self.ev(e, d)?.as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default(),
                    None => Vec::new(),
                };
                let default = num(&self.ev(o.get("default").ok_or_else(|| bad("`$partial_rotary`: default"))?, d)?).ok_or_else(|| bad("partial default"))?.f();
                let rp_factor = self.cur().opt_obj("rope_parameters")?.and_then(|rp| rp.get("partial_rotary_factor").and_then(Value::as_f64));
                if let Some(v) = rp_factor {
                    for k in &legacy {
                        if let Some(x) = self.cur().opt_f64(k)?
                            && (x - v).abs() > 1e-12
                        {
                            return Err(LowerError::not_lowerable(format!("{}: `{k}`={x} disagrees with rope_parameters' {v}", self.arch)));
                        }
                    }
                    return N::F(v).value();
                }
                for k in &legacy {
                    if let Some(x) = self.cur().opt_f64(k)? {
                        return N::F(x).value();
                    }
                }
                N::F(default).value()
            }
            "$rope" | "$rope_temp" => self.rope(op, arg, d),
            "$layers" => {
                let o = arg.as_object().ok_or_else(|| bad("`$layers` takes {count, each}"))?;
                let n = as_usize(&self.ev(o.get("count").ok_or_else(|| bad("`$layers`: count"))?, d)?, op)?;
                if n > MAX_LAYERS {
                    return Err(LowerError::not_lowerable(format!("{} layers (the lowerer reads at most {MAX_LAYERS})", n)));
                }
                let each = o.get("each").ok_or_else(|| bad("`$layers`: each"))?;
                let mut out = Vec::with_capacity(n);
                self.vars.borrow_mut().push(("n".into(), Value::from(n as u64)));
                let lvars: Vec<(String, Value, bool)> = self.layer_exprs.borrow().clone();
                for i in 0..n {
                    let mark = self.vars.borrow().len();
                    self.vars.borrow_mut().push(("i".into(), Value::from(i as u64)));
                    *self.layer_pending.borrow_mut() = lvars.iter().map(|(n, e, _)| (n.clone(), e.clone())).collect();
                    self.layer_done.borrow_mut().clear();
                    // Variables marked `force` are evaluated for every layer (their checks run).
                    let mut r: Result<Value> = Ok(Value::Null);
                    for (name, _, force) in &lvars {
                        if *force && let Err(x) = self.var(name) {
                            r = Err(x);
                            break;
                        }
                    }
                    if r.is_ok() {
                        r = self.ev(each, d);
                    }
                    self.vars.borrow_mut().truncate(mark);
                    self.layer_pending.borrow_mut().clear();
                    self.layer_done.borrow_mut().clear();
                    match r {
                        Ok(v) => out.push(v),
                        Err(e) => {
                            self.vars.borrow_mut().pop();
                            return Err(e);
                        }
                    }
                }
                self.vars.borrow_mut().pop();
                Ok(Value::Array(out))
            }
            other => Err(bad(format!("unknown adapter operator `{other}`"))),
        }
    }

    fn rope(&self, op: &str, arg: &Value, d: usize) -> Result<Value> {
        let o = arg.as_object().ok_or_else(|| bad(format!("`{op}` takes an object")))?;
        let get = |k: &str| -> Result<Option<Value>> { o.get(k).map(|e| self.ev(e, d)).transpose() };
        let rotary_dim = as_usize(&get("dim")?.ok_or_else(|| bad(format!("`{op}`: dim")))?, op)?;
        let style = match get("style")? {
            Some(Value::String(s)) if s == "Half" => crate::rope::RopeStyle::Half,
            Some(Value::String(s)) if s == "Interleaved" => crate::rope::RopeStyle::Interleaved,
            None => crate::rope::RopeStyle::Half,
            Some(other) => return Err(bad(format!("`{op}`: style {other}"))),
        };
        let theta = get("theta")?.filter(|v| !v.is_null()).map(|v| num(&v).map(N::f).ok_or_else(|| bad("rope theta"))).transpose()?;
        let layer_type = get("layer_type")?.and_then(|v| v.as_str().map(str::to_string));
        let partial = get("partial")?.map(|v| num(&v).map(N::f).ok_or_else(|| bad("rope partial"))).transpose()?.unwrap_or(1.0);
        let max_pos = get("max_pos")?.filter(|v| !v.is_null()).map(|v| as_usize(&v, op)).transpose()?;
        let top_orig = get("top_orig")?.filter(|v| !v.is_null()).map(|v| as_usize(&v, op)).transpose()?;
        let q_scaled = get("q_scaled")?.map(|v| as_bool(&v, op)).transpose()?.unwrap_or(false);
        let offset = get("offset")?.map(|v| as_usize(&v, op)).transpose()?.unwrap_or(0);
        let (mut spec, temp) =
            crate::rope::rope_spec_from_config(&self.cur(), rotary_dim, style, theta, layer_type.as_deref(), partial, max_pos, top_orig, q_scaled)?;
        spec.offset = offset;
        if op == "$rope" {
            serde_json::to_value(&spec).map_err(|e| bad(e.to_string()))
        } else {
            serde_json::to_value(&temp).map_err(|e| bad(e.to_string()))
        }
    }

    pub fn steps_used(&self) -> usize {
        self.steps.get()
    }
}

fn as_i64(v: &Value, op: &str) -> Result<i64> {
    match num(v) {
        Some(N::I(i)) => Ok(i),
        Some(N::F(f)) if f.fract() == 0.0 && f.abs() < 9.0e15 => Ok(f as i64),
        _ => Err(bad(format!("`{op}` wants integers, got {v}"))),
    }
}
