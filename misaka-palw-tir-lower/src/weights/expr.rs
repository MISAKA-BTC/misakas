//! **`WEIGHTS_EXPR_V1` — weights as data** (FR-01): the adapter-language surface of [`Src`].
//!
//! An adapter's `spec.hf.weights` maps an HL parameter name to a *weight expression*: a JSON array
//! whose first element is a checkpoint tensor's **complete** name and whose remaining elements are
//! steps applied in order — a left fold, each step wrapping the value so far. Every step is an exact
//! copy or re-indexing (the three named maps aside), so a conversion stays a pure re-indexing and the
//! artifact's tensor is what the checkpoint holds.
//!
//! ```jsonc
//! "moe.experts.down": ["transformer.blocks.{L}.ffn.experts.mlp.w2", {"reshape": [8, 16, 32]}, "transpose"]
//! ```
//!
//! | element | meaning |
//! | --- | --- |
//! | `"<name>"` (first) | the checkpoint tensor; `{L}` is the param's layer, `{E}`, `{H}`, `{S}`, … (one capital) are bound by a `stack` step |
//! | `"transpose"` | swap the last two axes |
//! | `{"reshape": [n, …]}` / `{"reshape": "param"}` | row-major reshape; `"param"` is the declared shape of the param being bound |
//! | `{"rows": [start, len]}` | rows of axis 0 |
//! | `{"take": {"axis", "start", "len"}}` | a range of an axis |
//! | `{"take": {"axis", "block", "offset", "len", "groups"}}` | per group, `len` indices at `group·block + offset` (per-head interleaving) |
//! | `{"take": {"axis", "per_layer"}}` | the layer's own `n` indices of a tensor packed over the layers |
//! | `{"take": {"axis", "repeat", "groups"}}` | each of the `groups` indices repeated `repeat` times in a row (a per-head value over the head's channels) |
//! | `{"stack": "E", "count": n}` | stack `n` instances of the value so far, `{E}` bound to `0..n`, on a new axis 0 |
//! | `{"pad_rows": n}` | flatten the leading axes into rows, append zero rows up to `n` |
//! | `{"map": "neg_exp"}`, `{"map": {"scale": c}}`, `{"map": {"rescale_by_layer": n}}` | the three element-wise maps |
//!
//! The grammar is **closed**: a step key it does not know is an error, never ignored (the same rule as
//! a config key: an unread flag is a wrong model with no error). The result is an ordinary [`Src`], so
//! the shape check, the unread-tensor report, the streaming loader and the conversion all see it as
//! they see every built-in binding. Design: `docs/design/palw/tir/frontend-as-data-v1.md` §2.

use super::{MapFn, Pick, Src};
use crate::error::{LowerError, Result};
use crate::hl::ParamDecl;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// At most this many steps after the source.
pub const MAX_STEPS: usize = 16;
/// At most this many overrides in one adapter.
pub const MAX_ENTRIES: usize = 4096;
/// A `stack` of at most this many instances.
pub const MAX_STACK: usize = 1 << 16;
/// No number, and no reshaped value, may exceed what a TIR param holds (spec 04b NF-8: 2^40 elements).
pub const MAX_ELEMENTS: u128 = 1 << 40;
/// The highest axis a step may name.
pub const MAX_AXIS: usize = 7;
/// The longest tensor-name template, in bytes.
pub const MAX_NAME: usize = 1024;
/// The step operations (the keys that start an object step).
const OPS: &[&str] = &["reshape", "rows", "take", "stack", "pad_rows", "map"];

fn bad(param: &str, msg: impl std::fmt::Display) -> LowerError {
    LowerError::bad(format!("weights expression for `{param}`: {msg}"))
}

/// A non-negative integer no larger than [`MAX_ELEMENTS`].
fn uint(param: &str, what: &str, v: &Value) -> Result<usize> {
    match v.as_u64() {
        Some(n) if (n as u128) <= MAX_ELEMENTS => Ok(n as usize),
        _ => Err(bad(param, format!("{what} must be a non-negative integer up to 2^40, got {v}"))),
    }
}

/// An integer of at least 1, no larger than [`MAX_ELEMENTS`].
fn positive(param: &str, what: &str, v: &Value) -> Result<usize> {
    let n = uint(param, what, v)?;
    if n == 0 {
        return Err(bad(param, format!("{what} must be at least 1")));
    }
    Ok(n)
}

/// The variables a tensor-name template mentions (`{L}` included), validated: every `{` opens one
/// capital letter and `}`.
fn template_vars(param: &str, t: &str) -> Result<BTreeSet<char>> {
    let b = t.as_bytes();
    let mut vars = BTreeSet::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' => {
                if i + 2 < b.len() && b[i + 2] == b'}' && b[i + 1].is_ascii_uppercase() {
                    vars.insert(b[i + 1] as char);
                    i += 3;
                } else if t[i..].starts_with("{p}") {
                    return Err(bad(
                        param,
                        format!("`{t}`: `{{p}}` is the adapter's tensor-name prefix; it is substituted when the adapter defines the variable `p`"),
                    ));
                } else {
                    return Err(bad(
                        param,
                        format!("`{t}`: a `{{` must open a variable of one capital letter (`{{L}}` layer, `{{E}}`, `{{H}}`, `{{S}}`, …) and `}}`"),
                    ));
                }
            }
            b'}' => return Err(bad(param, format!("`{t}`: a stray `}}`"))),
            _ => i += 1,
        }
    }
    Ok(vars)
}

/// A pair `[a, b]` of integers: `(a, b)`.
fn pair(param: &str, what: &str, v: &Value) -> Result<(usize, usize)> {
    match v.as_array().map(Vec::as_slice) {
        Some([a, b]) => Ok((uint(param, what, a)?, uint(param, what, b)?)),
        _ => Err(bad(param, format!("{what} is a pair [start, len]"))),
    }
}

/// `start + len` of a slice must stay within the element bound.
fn within(param: &str, what: &str, start: usize, len: usize) -> Result<()> {
    if len == 0 {
        return Err(bad(param, format!("{what}: the length must be at least 1")));
    }
    if (start as u128) + (len as u128) > MAX_ELEMENTS {
        return Err(bad(param, format!("{what}: {start} + {len} is past 2^40")));
    }
    Ok(())
}

fn apply_step(
    param: &str,
    src: Src,
    step: &Value,
    declared: &[usize],
    used: &BTreeSet<char>,
    bound: &mut BTreeSet<char>,
) -> Result<Src> {
    let o = match step {
        Value::String(s) if s == "transpose" => return Ok(Src::Transpose(Box::new(src))),
        Value::String(s) => return Err(bad(param, format!("unknown step `{s}` (the string steps are: \"transpose\")"))),
        Value::Object(o) => o,
        other => return Err(bad(param, format!("a step is \"transpose\" or an object, got {other}"))),
    };
    let ops: Vec<&str> = o.keys().map(String::as_str).filter(|k| OPS.contains(k)).collect();
    if ops.len() != 1 {
        return Err(bad(param, format!("a step has exactly one operation of {OPS:?}; this one has the keys {:?}", o.keys().collect::<Vec<_>>())));
    }
    let op = ops[0];
    let own = [op];
    let allowed: &[&str] = if op == "stack" { &["stack", "count"] } else { &own };
    if let Some(extra) = o.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(bad(param, format!("step `{op}` does not take `{extra}`")));
    }
    let arg = &o[op];
    Ok(match op {
        "reshape" => {
            let shape: Vec<usize> = match arg {
                Value::String(s) if s == "param" => declared.to_vec(),
                Value::Array(a) if !a.is_empty() && a.len() <= 8 => {
                    a.iter().map(|d| positive(param, "a reshape dimension", d)).collect::<Result<_>>()?
                }
                _ => return Err(bad(param, "reshape is a list of 1 to 8 positive integers, or \"param\"")),
            };
            if shape.iter().try_fold(1u128, |acc, d| acc.checked_mul(*d as u128).filter(|x| *x <= MAX_ELEMENTS)).is_none() {
                return Err(bad(param, format!("a reshape to {shape:?} is past 2^40 elements")));
            }
            Src::Reshape { src: Box::new(src), shape }
        }
        "rows" => {
            let (start, len) = pair(param, "rows", arg)?;
            within(param, "rows", start, len)?;
            Src::Take { src: Box::new(src), axis: 0, pick: Pick::Range { start, len } }
        }
        "take" => {
            let t = arg.as_object().ok_or_else(|| bad(param, "take is an object {axis, …}"))?;
            let get = |k: &str| -> Result<usize> {
                uint(param, &format!("take.{k}"), t.get(k).ok_or_else(|| bad(param, format!("take needs `{k}`")))?)
            };
            let axis = get("axis")?;
            if axis > MAX_AXIS {
                return Err(bad(param, format!("take.axis {axis} is past {MAX_AXIS}")));
            }
            let keys: BTreeSet<&str> = t.keys().map(String::as_str).collect();
            let pick = if keys == BTreeSet::from(["axis", "len", "start"]) {
                let (start, len) = (get("start")?, get("len")?);
                within(param, "take", start, len)?;
                Pick::Range { start, len }
            } else if keys == BTreeSet::from(["axis", "block", "groups", "len", "offset"]) {
                let (block, offset, len, groups) = (get("block")?, get("offset")?, get("len")?, get("groups")?);
                if block == 0 || groups == 0 || len == 0 {
                    return Err(bad(param, "take: block, len and groups are at least 1"));
                }
                if (offset as u128) + (len as u128) > block as u128 {
                    return Err(bad(param, format!("take: offset {offset} + len {len} runs past the block of {block}")));
                }
                if (groups as u128) * (block as u128) > MAX_ELEMENTS {
                    return Err(bad(param, "take: groups × block is past 2^40"));
                }
                Pick::Strided { block, offset, len, groups }
            } else if keys == BTreeSet::from(["axis", "groups", "repeat"]) {
                let (each, groups) = (positive(param, "take.repeat", &t["repeat"])?, positive(param, "take.groups", &t["groups"])?);
                if (each as u128) * (groups as u128) > MAX_ELEMENTS {
                    return Err(bad(param, "take: repeat × groups is past 2^40"));
                }
                Pick::Repeat { each, groups }
            } else if keys == BTreeSet::from(["axis", "per_layer"]) {
                Pick::PerLayer { len: positive(param, "take.per_layer", &t["per_layer"])? }
            } else {
                return Err(bad(
                    param,
                    "take is one of {axis, start, len}, {axis, block, offset, len, groups}, {axis, repeat, groups} or {axis, per_layer}",
                ));
            };
            Src::Take { src: Box::new(src), axis, pick }
        }
        "stack" => {
            let var = match arg {
                Value::String(s) if s.len() == 1 && s.as_bytes()[0].is_ascii_uppercase() && s != "L" => s.as_bytes()[0] as char,
                _ => return Err(bad(param, "stack names a variable: one capital letter other than L")),
            };
            let count = positive(param, "stack.count", o.get("count").ok_or_else(|| bad(param, "stack needs `count`"))?)?;
            if count > MAX_STACK {
                return Err(bad(param, format!("stack.count {count} is past {MAX_STACK}")));
            }
            // A stack of one is an added leading axis whatever the name says; a longer stack of a name that does
            // not use the variable would be `count` copies of one tensor — always a mistake.
            if count > 1 && !used.contains(&var) {
                return Err(bad(param, format!("stack binds {{{var}}} to 0..{count}, but the tensor name does not use it: every copy would be the same tensor")));
            }
            if !bound.insert(var) {
                return Err(bad(param, format!("{{{var}}} is bound by two stack steps")));
            }
            Src::Stack { src: Box::new(src), var, count }
        }
        "pad_rows" => Src::PadRows { src: Box::new(src), rows: positive(param, "pad_rows", arg)? },
        "map" => {
            let f = match arg {
                Value::String(s) if s == "neg_exp" => MapFn::NegExp,
                Value::Object(m) if m.len() == 1 => match m.iter().next() {
                    Some((k, v)) if k == "scale" => MapFn::Scale(
                        v.as_f64().filter(|c| c.is_finite()).ok_or_else(|| bad(param, "map.scale is a finite number"))?,
                    ),
                    Some((k, v)) if k == "rescale_by_layer" => MapFn::RescaleByLayer { every: positive(param, "map.rescale_by_layer", v)? },
                    _ => return Err(bad(param, "map is \"neg_exp\", {\"scale\": c} or {\"rescale_by_layer\": n}")),
                },
                _ => return Err(bad(param, "map is \"neg_exp\", {\"scale\": c} or {\"rescale_by_layer\": n}")),
            };
            Src::Map { src: Box::new(src), f }
        }
        _ => unreachable!("`OPS` lists the operations handled above"),
    })
}

/// Parse one `spec.hf.weights` entry. `declared` is the shape of the HL param being bound (the target of
/// `{"reshape": "param"}`) and `per_layer` whether it is bound once per layer (`{L}` is refused in a
/// global param).
pub fn parse_weight_expr(param: &str, v: &Value, declared: &[usize], per_layer: bool) -> Result<Src> {
    let items: Vec<&Value> = match v {
        Value::String(_) => vec![v],
        Value::Array(a) if !a.is_empty() => a.iter().collect(),
        _ => return Err(bad(param, "an expression is a tensor name or an array: [tensor name, step, …]")),
    };
    let Value::String(name) = items[0] else {
        return Err(bad(param, "the first element is the checkpoint tensor's complete name (a string)"));
    };
    if name.is_empty() || name.len() > MAX_NAME || name.contains('\0') {
        return Err(bad(param, format!("the tensor name is 1 to {MAX_NAME} bytes with no NUL")));
    }
    let used = template_vars(param, name)?;
    if used.contains(&'L') && !per_layer {
        return Err(bad(param, "the tensor name uses {L} but the param is not bound per layer"));
    }
    let steps = &items[1..];
    if steps.len() > MAX_STEPS {
        return Err(bad(param, format!("{} steps: at most {MAX_STEPS}", steps.len())));
    }
    let mut src = Src::Tensor(name.clone());
    let mut bound = BTreeSet::new();
    for step in steps {
        src = apply_step(param, src, step, declared, &used, &mut bound)?;
    }
    let free: Vec<String> = used.iter().filter(|c| **c != 'L' && !bound.contains(*c)).map(|c| format!("{{{c}}}")).collect();
    if !free.is_empty() {
        return Err(bad(param, format!("{} never bound: a `stack` step binds one variable", free.join(", "))));
    }
    Ok(src)
}

/// Parse every entry of `spec.hf.weights` against the program's params. An entry that names no param
/// is an error, with the params that share its prefix — a typo is never a silent no-op.
pub fn parse_all(weights: &BTreeMap<String, Value>, params: &[ParamDecl]) -> Result<BTreeMap<String, Src>> {
    if weights.len() > MAX_ENTRIES {
        return Err(LowerError::bad(format!("weights: {} expressions: at most {MAX_ENTRIES}", weights.len())));
    }
    let mut out = BTreeMap::new();
    for (name, v) in weights {
        let Some(d) = params.iter().find(|d| &d.name == name) else {
            let stem = name.rsplit_once('.').map_or(name.as_str(), |(a, _)| a);
            let near: Vec<&str> = params.iter().map(|d| d.name.as_str()).filter(|n| n.starts_with(stem)).take(8).collect();
            let hint = if near.is_empty() { String::new() } else { format!(" (params under `{stem}`: {})", near.join(", ")) };
            return Err(LowerError::bad(format!("weights: `{name}` is not a parameter of this model's graph{hint}")));
        };
        out.insert(name.clone(), parse_weight_expr(name, v, &d.shape, d.per_layer)?);
    }
    Ok(out)
}

/// The expression that denotes `src`: the inverse of [`parse_weight_expr`] over everything the grammar can
/// say, so an adapter author can start from a model's default binding and edit it (`None` for a pre-quantised
/// source, which has no expression). `parse_weight_expr(to_expr(s)) == s` for every other `Src` a built-in
/// binding produces — that is what `tests/weights_expr.rs` checks on every fixture.
pub fn to_expr(src: &Src) -> Option<Value> {
    use serde_json::json;
    // A `Src` other than a pre-quantised one is a straight chain of wrappers around one tensor name:
    // collect the steps outermost first, then reverse them into the order they apply.
    let mut steps: Vec<Value> = Vec::new();
    let mut cur = src;
    loop {
        match cur {
            Src::Tensor(t) => {
                steps.reverse();
                let mut v = vec![Value::String(t.clone())];
                v.extend(steps);
                return Some(Value::Array(v));
            }
            Src::Quant { .. } => return None,
            Src::Transpose(s) => {
                steps.push(json!("transpose"));
                cur = &**s;
            }
            Src::Reshape { src: s, shape } => {
                steps.push(json!({"reshape": shape}));
                cur = &**s;
            }
            Src::Take { src: s, axis, pick } => {
                steps.push(match pick {
                    Pick::Range { start, len } if *axis == 0 => json!({"rows": [start, len]}),
                    Pick::Range { start, len } => json!({"take": {"axis": axis, "start": start, "len": len}}),
                    Pick::Strided { block, offset, len, groups } => {
                        json!({"take": {"axis": axis, "block": block, "offset": offset, "len": len, "groups": groups}})
                    }
                    Pick::PerLayer { len } => json!({"take": {"axis": axis, "per_layer": len}}),
                    Pick::Repeat { each, groups } => json!({"take": {"axis": axis, "repeat": each, "groups": groups}}),
                });
                cur = &**s;
            }
            Src::Stack { src: s, var, count } => {
                steps.push(json!({"stack": var.to_string(), "count": count}));
                cur = &**s;
            }
            Src::PadRows { src: s, rows } => {
                steps.push(json!({"pad_rows": rows}));
                cur = &**s;
            }
            Src::Map { src: s, f } => {
                steps.push(match f {
                    MapFn::NegExp => json!({"map": "neg_exp"}),
                    MapFn::Scale(c) => json!({"map": {"scale": c}}),
                    MapFn::RescaleByLayer { every } => json!({"map": {"rescale_by_layer": every}}),
                });
                cur = &**s;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> Result<Src> {
        parse_weight_expr("p", &v, &[8], true)
    }

    #[test]
    fn a_bare_name_is_a_tensor_and_the_steps_fold_left() {
        assert_eq!(parse(json!("a.{L}.w")).unwrap(), Src::Tensor("a.{L}.w".into()));
        let s = parse(json!(["a.{L}.w", {"reshape": [2, 4]}, "transpose", {"rows": [1, 2]}])).unwrap();
        assert_eq!(s, Src::t("a.{L}.w").reshape(vec![2, 4]).transpose().rows(Pick::Range { start: 1, len: 2 }));
        // `"param"` is the declared shape.
        assert_eq!(parse(json!(["a.{L}.w", {"reshape": "param"}])).unwrap(), Src::t("a.{L}.w").reshape(vec![8]));
    }

    #[test]
    fn stack_binds_the_variable_the_name_uses() {
        let s = parse(json!(["e.{L}.{E}.w", {"stack": "E", "count": 3}])).unwrap();
        assert_eq!(s, Src::t("e.{L}.{E}.w").stack('E', 3));
        assert!(parse(json!(["e.{L}.{E}.w"])).is_err(), "{{E}} is never bound");
        assert!(parse(json!(["e.{L}.w", {"stack": "E", "count": 3}])).is_err(), "the name does not use {{E}}");
        assert!(parse(json!(["e.{L}.{E}.w", {"stack": "E", "count": 3}, {"stack": "E", "count": 2}])).is_err(), "bound twice");
        assert!(parse(json!(["e.{E}.w", {"stack": "L", "count": 3}])).is_err(), "L is the layer");
        assert!(parse(json!(["e.{E}.w", {"stack": "E"}])).is_err(), "no count");
    }

    #[test]
    fn the_grammar_is_closed() {
        for bad_expr in [
            json!([]),
            json!(7),
            json!([7]),
            json!(["a", "flip"]),
            json!(["a", {"reshape": [2], "extra": 1}]),
            json!(["a", {"reshape": [2], "rows": [0, 1]}]),
            json!(["a", {"reshape": [0]}]),
            json!(["a", {"reshape": []}]),
            json!(["a", {"reshape": [1099511627777u64]}]),
            json!(["a", {"reshape": [1048576, 1048577]}]),
            json!(["a", {"rows": [0]}]),
            json!(["a", {"rows": [0, 0]}]),
            json!(["a", {"rows": [-1, 2]}]),
            json!(["a", {"take": {"axis": 0, "start": 0}}]),
            json!(["a", {"take": {"axis": 8, "start": 0, "len": 1}}]),
            json!(["a", {"take": {"axis": 0, "block": 2, "offset": 2, "len": 1, "groups": 1}}]),
            json!(["a", {"take": {"axis": 0, "per_layer": 0}}]),
            json!(["a", {"take": {"axis": 0, "start": 0, "len": 1, "more": 1}}]),
            json!(["a", {"map": "sqrt"}]),
            json!(["a", {"map": {"scale": "x"}}]),
            json!(["a", {"map": {"scale": 1.0, "other": 2}}]),
            json!(["a", {"pad_rows": 0}]),
            json!(["a", {"unknown": 1}]),
            json!(["a{", {"rows": [0, 1]}]),
            json!(["{p}a"]),
            json!(["a.{l}.w"]),
            json!([""]),
        ] {
            assert!(parse(bad_expr.clone()).is_err(), "{bad_expr} must be refused");
        }
        let long: Vec<Value> = std::iter::once(json!("a")).chain((0..=MAX_STEPS).map(|_| json!("transpose"))).collect();
        assert!(parse(Value::Array(long)).is_err(), "too many steps");
    }

    #[test]
    fn a_global_param_has_no_layer() {
        assert!(parse_weight_expr("g", &json!("a.{L}.w"), &[8], false).is_err());
        assert!(parse_weight_expr("g", &json!("a.w"), &[8], false).is_ok());
    }

    #[test]
    fn every_step_maps_to_its_src() {
        let s = parse(json!(["t.{L}", {"take": {"axis": 1, "block": 3, "offset": 1, "len": 1, "groups": 2}}, {"take": {"axis": 0, "per_layer": 4}}, {"pad_rows": 9}, {"map": "neg_exp"}, {"map": {"scale": 0.5}}, {"map": {"rescale_by_layer": 6}}])).unwrap();
        let want = Src::t("t.{L}")
            .take(1, Pick::Strided { block: 3, offset: 1, len: 1, groups: 2 })
            .take(0, Pick::PerLayer { len: 4 })
            .pad_rows(9)
            .map(MapFn::NegExp)
            .map(MapFn::Scale(0.5))
            .map(MapFn::RescaleByLayer { every: 6 });
        assert_eq!(s, want);
    }

    #[test]
    fn a_repeat_step_spreads_a_per_head_value_over_the_head_channels() {
        let s = parse(json!(["a_log", {"reshape": [3]}, {"map": "neg_exp"}, {"take": {"axis": 0, "repeat": 2, "groups": 3}}])).unwrap();
        assert_eq!(s, Src::t("a_log").reshape(vec![3]).map(MapFn::NegExp).take(0, Pick::Repeat { each: 2, groups: 3 }));
        assert_eq!(Pick::Repeat { each: 2, groups: 3 }.indices_at(None).unwrap(), vec![0, 0, 1, 1, 2, 2]);
        assert_eq!(Pick::Repeat { each: 2, groups: 3 }.count(), 6);
        // The expression is written back as the same JSON.
        assert_eq!(super::to_expr(&s).unwrap(), json!(["a_log", {"reshape": [3]}, {"map": "neg_exp"}, {"take": {"axis": 0, "repeat": 2, "groups": 3}}]));
        for bad_expr in [
            json!(["a", {"take": {"axis": 0, "repeat": 0, "groups": 3}}]),
            json!(["a", {"take": {"axis": 0, "repeat": 2}}]),
            json!(["a", {"take": {"axis": 0, "repeat": 2, "groups": 3, "len": 1}}]),
        ] {
            assert!(parse(bad_expr.clone()).is_err(), "{bad_expr} must be refused");
        }
    }

    #[test]
    fn an_unknown_param_names_its_neighbours() {
        use crate::hl::Init;
        let p = |n: &str| ParamDecl { name: n.into(), shape: vec![2], per_layer: true, init: Init::Normal(0.1) };
        let params = [p("moe.experts.gate"), p("moe.experts.up"), p("attn.q.w")];
        let w: BTreeMap<String, Value> = [("moe.expert.gate".to_string(), json!("a"))].into();
        let e = parse_all(&w, &params).unwrap_err().to_string();
        assert!(e.contains("moe.expert.gate") && e.contains("moe.experts.gate") && e.contains("moe.experts.up"), "{e}");
        let ok: BTreeMap<String, Value> = [("attn.q.w".to_string(), json!("a.{L}"))].into();
        assert_eq!(parse_all(&ok, &params).unwrap().len(), 1);
    }
}
