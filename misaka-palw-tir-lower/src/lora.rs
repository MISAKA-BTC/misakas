//! **LoRA adapters** (RFC-0004: a candidate model is a parent plus an adapter).
//!
//! A PEFT LoRA adapter (`adapter_config.json` + `adapter_model.safetensors`) is read directly, with
//! no `peft` installed, and attached to the parent's [`ArchSpec`]. Every targeted projection `W`
//! then computes, unmerged,
//!
//! ```text
//! y = W·x (+ b) + s · B·(A·x),     A: [r, in],  B: [out, r],  s = lora_alpha / r  (rsLoRA: / √r)
//! ```
//!
//! with `s` an exact rational `num/den` (a JSON number is an exact binary fraction). The parent's
//! weights are untouched. The adapter's `A` and `B` are params of their own, which the lowering
//! places after every parent param (`crate::lower::adapter_params_last`), so a candidate's artifact
//! is the parent's tensors unchanged followed by the adapter's section.
//!
//! What is modelled is the PEFT LoRA that a plain `nn.Linear` target takes: `bias: "none"`, no DoRA,
//! no `fan_in_fan_out` (GPT-2's `Conv1D`), no `modules_to_save`, and every layer transformed. A
//! target on a fused projection (`qkv_proj`, `gate_up_proj`, `c_attn`, `query_key_value`) or on a
//! role the lowering has no single matrix for is refused by name, never ignored.

use crate::error::{LowerError, Result};
use crate::spec::ArchSpec;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// One targeted role's adapter shape and scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LoraRole {
    pub rank: usize,
    /// `s = num / den`, exactly.
    pub num: i64,
    pub den: i64,
}

/// An adapter attached to a spec: the targeted roles (HL role names, `attn.q`, `mlp.down`, …) and
/// each role's checkpoint module path (for the adapter's tensor names).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LoraAdapter {
    pub roles: BTreeMap<String, LoraRole>,
    pub modules: BTreeMap<String, String>,
    /// The adapter's tensor-name prefix (PEFT's `base_model.model.`).
    pub prefix: String,
}

impl LoraAdapter {
    pub fn role(&self, name: &str) -> Option<&LoraRole> {
        self.roles.get(name)
    }
    /// The adapter tensor holding `A` (`lora_A`) or `B` (`lora_B`) of `role`, with `{L}` for the
    /// layer.
    pub fn tensor(&self, role: &str, b: bool) -> Option<String> {
        let m = self.modules.get(role)?;
        Some(format!("{}{m}.lora_{}.weight", self.prefix, if b { "B" } else { "A" }))
    }
    /// Params the adapter adds, and its MACs a position (per layer, over every targeted role):
    /// `Σ r·(in + out)` given each role's `(in, out)`.
    pub fn macs(&self, dims: &BTreeMap<String, (usize, usize)>) -> u64 {
        self.roles.iter().filter_map(|(k, l)| dims.get(k).map(|(i, o)| (l.rank * (i + o)) as u64)).sum()
    }
}

/// `x` as the exact rational `num/den` (`den` a power of two): an `f64` is a binary fraction.
fn exact_rational(x: f64) -> Result<(i64, i64)> {
    if !x.is_finite() || x <= 0.0 {
        return Err(LowerError::bad(format!("lora_alpha {x} is not a positive finite number")));
    }
    let (mut num, mut den) = (x, 1i64);
    while num.fract() != 0.0 {
        num *= 2.0;
        den = den.checked_mul(2).ok_or_else(|| LowerError::bad(format!("lora_alpha {x} needs more than 62 fraction bits")))?;
    }
    if num > (1i64 << 52) as f64 {
        return Err(LowerError::bad(format!("lora_alpha {x} is beyond 2^52")));
    }
    Ok((num as i64, den))
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.abs().max(1)
}

/// `alpha / r` (or `alpha / √r`, rsLoRA) reduced. rsLoRA's `√r` is exact only for a square rank.
fn scale(alpha: f64, r: usize, rslora: bool) -> Result<(i64, i64)> {
    let (an, ad) = exact_rational(alpha)?;
    let div = if rslora {
        let s = (r as f64).sqrt().round() as i64;
        if (s * s) as usize != r {
            return Err(LowerError::not_lowerable(format!("rsLoRA with rank {r}: alpha/√{r} is not a rational (a square rank is)")));
        }
        s
    } else {
        r as i64
    };
    let den = ad.checked_mul(div).ok_or_else(|| LowerError::bad("LoRA scale overflows"))?;
    let g = gcd(an, den);
    Ok((an / g, den / g))
}

/// Modules the HL keeps as several matrices: a LoRA on them would need one `A` shared by slices of
/// `B`, which is not modelled yet.
const FUSED: &[&str] = &["qkv_proj", "gate_up_proj", "c_attn", "query_key_value", "W_pack", "Wqkv", "c_fc"];

/// The HL per-layer projections the lowering adapts (each a single matrix), attention first.
const ADAPTED_ROLES: &[&str] = &[
    "attn.q",
    "attn.k",
    "attn.v",
    "attn.o",
    "attn.gate",
    "mlp.gate",
    "mlp.up",
    "mlp.down",
    "moe.shared.gate",
    "moe.shared.up",
    "moe.shared.down",
];

/// Whether the lowering adapts `role` (an HL per-layer projection with a single matrix).
pub fn adapts_role(role: &str) -> bool {
    ADAPTED_ROLES.contains(&role)
}

/// One module kind an adapter can target on `spec`: its HL role and its checkpoint leaf name (what
/// `target_modules` lists), and whether the leaf is a fused projection (refused).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LoraTarget {
    pub role: String,
    pub leaf: String,
    pub fused: bool,
}

/// **The module kinds an adapter can name on `spec`**: every per-layer projection the lowering
/// adapts, attention's first, then the fused projections, which are marked because naming one is
/// refused.
pub fn targets(spec: &ArchSpec) -> Vec<LoraTarget> {
    let mut out: Vec<LoraTarget> = Vec::new();
    let mut push = |role: &str, path: &str| {
        let leaf = path.rsplit('.').next().unwrap_or(path).to_string();
        if !out.iter().any(|t| t.leaf == leaf) {
            out.push(LoraTarget { role: role.to_string(), fused: FUSED.contains(&leaf.as_str()), leaf });
        }
    };
    for role in ADAPTED_ROLES {
        if let Some(path) = spec.hf.names.get(*role).filter(|p| p.contains("{L}")) {
            push(role, path);
        }
    }
    for (role, path) in &spec.hf.names {
        if path.contains("{L}") && FUSED.contains(&path.rsplit('.').next().unwrap_or(path)) {
            push(role, path);
        }
    }
    out
}

/// **Every tensor of an adapter file is one the attached adapter reads**: a LoRA pair of a targeted
/// projection, at some layer. PEFT's `all-linear` and its patterns also reach modules the lowering
/// does not adapt, such as a router, an expert or a gated-delta projection. Their tensors would
/// otherwise go unread, and the candidate would not be the trained model, so they are refused by
/// name.
pub fn check_adapter_tensors(spec: &ArchSpec, names: &[String]) -> Result<()> {
    let ad = spec.adapter.as_ref().ok_or_else(|| LowerError::bad("no adapter is attached"))?;
    let mut read = std::collections::BTreeSet::new();
    for role in ad.roles.keys() {
        for b in [false, true] {
            if let Some(t) = ad.tensor(role, b) {
                read.extend((0..spec.layers.len()).map(|l| t.replace("{L}", &l.to_string())));
            }
        }
    }
    let unread: Vec<&String> = names.iter().filter(|n| !read.contains(*n)).collect();
    match unread.first() {
        None => Ok(()),
        Some(first) => Err(LowerError::not_lowerable(format!(
            "LoRA adapter: {} tensor(s) of modules the lowering does not adapt, e.g. `{first}` (the targets it adapts: {})",
            unread.len(),
            ad.modules.values().map(|m| m.rsplit('.').next().unwrap_or(m)).collect::<Vec<_>>().join(", ")
        ))),
    }
}

/// Read `adapter_config.json` and attach the adapter to `spec`.
pub fn attach(spec: &mut ArchSpec, adapter_config: &str) -> Result<()> {
    let v: Value = serde_json::from_str(adapter_config).map_err(|e| LowerError::bad(format!("adapter_config.json: {e}")))?;
    let get_bool = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    let absent = |k: &str| match v.get(k) {
        None | Some(Value::Null) => true,
        Some(Value::Array(a)) => a.is_empty(),
        Some(Value::Object(o)) => o.is_empty(),
        _ => false,
    };
    let refuse = |m: String| Err(LowerError::not_lowerable(format!("LoRA adapter: {m}")));
    match v.get("peft_type").and_then(Value::as_str) {
        Some("LORA") => {}
        other => return refuse(format!("peft_type {other:?} is not LORA")),
    }
    if get_bool("use_dora") {
        return refuse("DoRA (use_dora) is not modelled".into());
    }
    if get_bool("fan_in_fan_out") {
        return refuse("fan_in_fan_out (a Conv1D base, GPT-2) is not modelled".into());
    }
    if get_bool("lora_bias") {
        return refuse("lora_bias is not modelled".into());
    }
    match v.get("bias").and_then(Value::as_str).unwrap_or("none") {
        "none" => {}
        b => return refuse(format!("bias `{b}` (trained biases) is not modelled")),
    }
    for k in
        ["modules_to_save", "layers_to_transform", "layers_pattern", "target_parameters", "trainable_token_indices", "exclude_modules"]
    {
        if !absent(k) {
            return refuse(format!("`{k}` is not modelled"));
        }
    }
    let r = v.get("r").and_then(Value::as_u64).ok_or_else(|| LowerError::bad("adapter_config.json has no rank `r`"))? as usize;
    let alpha = v.get("lora_alpha").and_then(Value::as_f64).unwrap_or(8.0);
    let rslora = get_bool("use_rslora");
    // Per-module overrides, by leaf name only (a pattern naming one layer would make layers differ).
    let pattern = |k: &str| -> Result<BTreeMap<String, f64>> {
        let mut out = BTreeMap::new();
        if let Some(o) = v.get(k).and_then(Value::as_object) {
            for (key, val) in o {
                if key.contains('.') || key.contains('*') || key.contains('\\') {
                    return Err(LowerError::not_lowerable(format!(
                        "LoRA adapter: `{k}` key `{key}` names part of the model, not a module kind"
                    )));
                }
                out.insert(key.clone(), val.as_f64().ok_or_else(|| LowerError::bad(format!("`{k}.{key}` is not a number")))?);
            }
        }
        Ok(out)
    };
    let (rank_p, alpha_p) = (pattern("rank_pattern")?, pattern("alpha_pattern")?);
    // Targets: a list of module leaf names, "all-linear", or a regex over module paths.
    enum Targets {
        Leaves(Vec<String>),
        AllLinear,
        Regex(String),
    }
    let targets = match v.get("target_modules") {
        Some(Value::Array(a)) => Targets::Leaves(a.iter().filter_map(Value::as_str).map(str::to_string).collect()),
        Some(Value::String(s)) if s == "all-linear" => Targets::AllLinear,
        Some(Value::String(s)) => Targets::Regex(s.clone()),
        _ => return refuse("no target_modules".into()),
    };
    if let Targets::Leaves(l) = &targets
        && let Some(f) = l.iter().find(|x| FUSED.contains(&x.as_str()))
    {
        return refuse(format!("a target on the fused projection `{f}` is not modelled yet"));
    }
    // The spec's per-layer projection roles and their module paths (`{L}` for the layer).
    let layer_linear = adapts_role;
    let regex_match = |pat: &str, path: &str| -> bool {
        // PEFT's string target is `re.fullmatch` over the module path. Only the forms adapters use
        // are modelled: `.*\.(a|b|c)` or `.*(a|b|c)`, and a plain leaf.
        let p = pat.trim_start_matches(".*").trim_start_matches("\\.").trim_start_matches('.');
        let leaf = path.rsplit('.').next().unwrap_or(path);
        let alts: Vec<&str> = p.trim_start_matches('(').trim_end_matches(')').split('|').collect();
        alts.contains(&leaf)
    };
    let mut roles = BTreeMap::new();
    let mut modules = BTreeMap::new();
    let mut matched = std::collections::BTreeSet::new();
    for (role, path) in &spec.hf.names {
        if !path.contains("{L}") {
            continue;
        }
        let leaf = path.rsplit('.').next().unwrap_or(path);
        let hit = match &targets {
            Targets::Leaves(l) => l.iter().any(|t| t == leaf),
            // PEFT's all-linear takes every linear projection, a fused one included (refused below).
            Targets::AllLinear => layer_linear(role) || FUSED.contains(&leaf),
            Targets::Regex(p) => regex_match(p, path),
        };
        if !hit {
            continue;
        }
        if FUSED.contains(&leaf) {
            return refuse(format!("the fused projection `{leaf}` is not modelled yet"));
        }
        if !layer_linear(role) {
            return refuse(format!("target `{leaf}` ({role}) is not a projection the lowering adapts"));
        }
        let rr = rank_p.get(leaf).map_or(r, |x| *x as usize);
        let aa = alpha_p.get(leaf).copied().unwrap_or(alpha);
        let (num, den) = scale(aa, rr, rslora)?;
        roles.insert(role.clone(), LoraRole { rank: rr, num, den });
        modules.insert(role.clone(), path.clone());
        matched.insert(leaf.to_string());
    }
    if let Targets::Leaves(l) = &targets
        && let Some(missing) = l.iter().find(|t| !matched.contains(*t))
    {
        return refuse(format!("target `{missing}` matches no projection of {}", spec.architecture));
    }
    if roles.is_empty() {
        return refuse("no projection is targeted".into());
    }
    spec.adapter = Some(LoraAdapter { roles, modules, prefix: "base_model.model.".into() });
    spec.notes.push("a LoRA adapter is attached (unmerged: W·x + s·B·A·x per targeted projection)".into());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_are_exact_rationals() {
        assert_eq!(scale(16.0, 64, false).unwrap(), (1, 4));
        assert_eq!(scale(12.0, 8, false).unwrap(), (3, 2));
        assert_eq!(scale(16.5, 4, false).unwrap(), (33, 8));
        assert_eq!(scale(8.0, 16, true).unwrap(), (2, 1));
        assert!(scale(8.0, 8, true).is_err(), "√8 is not rational");
        assert_eq!(scale(0.1, 1, false).unwrap().1 % 2, 0, "0.1 is a binary fraction");
    }
}
