//! **Rotary and ALiBi position terms, with HF's exact frequency formulas.**
//!
//! `transformers` computes `inv_freq` in `float32` (`modeling_rope_utils.py`), multiplies it by the
//! position in `float32`, and scales `cos`/`sin` by an `attention_factor`. This module reproduces
//! that per rope type — default, linear, dynamic NTK, YaRN, Llama-3, LongRoPE — and exposes one
//! function of the position, [`RopeFreqs::cos_sin`]. For the integer program a RoPE table is
//! data indexed by `pos` (RFC-0002 §3.3), so position-dependent variants (dynamic NTK, the
//! LongRoPE switch) cost nothing extra there: they are just a different table row.
//!
//! **Per-position semantics.** HF recomputes dynamic/LongRoPE frequencies from the *sequence
//! length* of the current forward call. In cached decoding that is `pos + 1`, which is what a
//! one-position program sees; a single long prefill in HF uses the final length for every
//! position instead. The per-position (decode-path) reading is the one lowered here.

use crate::cfg::Cfg;
use crate::error::{LowerError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RopeStyle {
    /// `rotate_half`: pairs `(i, i + d/2)` (Llama, NeoX, …).
    Half,
    /// GPT-J `rotate_every_two`: pairs `(2i, 2i+1)` (GPT-J, Cohere, DeepSeek MLA with `rope_interleave`).
    Interleaved,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DynamicNtk {
    pub factor: f64,
    pub max_pos: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LongRopeSwitch {
    /// Used once `pos + 1 > original_max`.
    pub inv_freq_long: Vec<f32>,
    pub original_max: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RopeFreqs {
    pub rope_type: String,
    pub theta: f64,
    /// The rotary dimension the frequencies are built for (`int(head_dim · partial_rotary_factor)`).
    pub dim: usize,
    /// `dim / 2` inverse frequencies, as HF computes them (float32).
    pub inv_freq: Vec<f32>,
    /// Multiplies both `cos` and `sin` (YaRN mscale, LongRoPE scaling).
    pub attention_factor: f64,
    pub dynamic: Option<DynamicNtk>,
    pub longrope: Option<LongRopeSwitch>,
    /// The multimodal rope (Qwen2-VL, Qwen2.5-VL, Qwen3-VL, Qwen3.5): which position component
    /// `(t, h, w)` rotates each frequency. With the components equal (text only) it is the plain
    /// rope.
    pub mrope: Option<MRope>,
}

/// `mrope_section` and its layout over the `dim / 2` frequencies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MRope {
    pub section: [usize; 3],
    /// Qwen3-VL's and Qwen3.5's layout (`mrope_interleaved`): `h` and `w` take every third
    /// frequency from 1 and 2, `3 · section[k]` long, and `t` the rest. Otherwise (Qwen2-VL)
    /// contiguous sections `t`, `h`, `w`.
    pub interleaved: bool,
}

impl MRope {
    /// The position component (`0` t, `1` h, `2` w) that rotates frequency `j`.
    pub fn component(&self, j: usize) -> usize {
        let [t, h, w] = self.section;
        if self.interleaved {
            match j % 3 {
                1 if j < 3 * h => 1,
                2 if j < 3 * w => 2,
                _ => 0,
            }
        } else if j < t {
            0
        } else if j < t + h {
            1
        } else {
            2
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RopeSpec {
    /// Dimensions rotated in each head.
    pub rotary_dim: usize,
    /// First rotated dimension inside the head (0, or `qk_nope_head_dim` for MLA).
    pub offset: usize,
    pub style: RopeStyle,
    pub freqs: RopeFreqs,
}

/// HF's default `inv_freq` in float32: `1 / base^(arange(0, dim, 2) / dim)`.
pub fn default_inv_freq(base: f64, dim: usize) -> Vec<f32> {
    (0..dim / 2)
        .map(|i| {
            let e = (2 * i) as f32 / dim as f32;
            let p = crate::detmath::powf(base, e as f64) as f32;
            1.0f32 / p
        })
        .collect()
}

impl RopeFreqs {
    pub fn plain(theta: f64, dim: usize) -> Self {
        RopeFreqs {
            rope_type: "default".into(),
            theta,
            dim,
            inv_freq: default_inv_freq(theta, dim),
            attention_factor: 1.0,
            dynamic: None,
            longrope: None,
            mrope: None,
        }
    }

    /// The inverse frequencies in force at `pos`.
    pub fn inv_freq_at(&self, pos: usize) -> Vec<f32> {
        if let Some(d) = &self.dynamic {
            let seq_len = pos + 1;
            if seq_len > d.max_pos {
                let dim = self.dim as f64;
                let base = self.theta * crate::detmath::powf((d.factor * seq_len as f64 / d.max_pos as f64) - (d.factor - 1.0), dim / (dim - 2.0));
                return default_inv_freq(base, self.dim);
            }
        }
        if let Some(l) = &self.longrope
            && pos + 1 > l.original_max
        {
            return l.inv_freq_long.clone();
        }
        self.inv_freq.clone()
    }

    /// `(cos, sin)` of `pos · inv_freq[i]` for `i < dim/2`, times `attention_factor`, in float32 like
    /// HF (the product `pos · inv_freq` is a float32 multiply there, which is reproduced).
    pub fn cos_sin(&self, pos: usize) -> (Vec<f32>, Vec<f32>) {
        let inv = self.inv_freq_at(pos);
        let af = self.attention_factor as f32;
        let mut c = Vec::with_capacity(inv.len());
        let mut s = Vec::with_capacity(inv.len());
        for f in inv {
            let ang = (pos as f32) * f;
            c.push(crate::detmath::cos(ang as f64) as f32 * af);
            s.push(crate::detmath::sin(ang as f64) as f32 * af);
        }
        (c, s)
    }
}

/// The rope parameters a config names, before any computation.
#[derive(Clone, Debug)]
pub struct RopeConfig {
    pub rope_type: String,
    pub theta: f64,
    pub params: Map<String, Value>,
}

/// **LongRoPE as transformers 5.17 runs Phi-3.5-MoE** (`ROPE_LONGROPE_V1`, short-factors-only mode):
/// the SHORT factors at every length (its forward rebuilds the frequencies without the sequence
/// length) and cos/sin times `short_mscale`; `long_mscale` must equal it (the published checkpoint
/// does), and a factor list of the wrong length, a missing mscale or any other rope key is refused.
/// `Ok(None)` when the rope type is `default` (the caller reads the ordinary rope); any other type
/// is not modelled.
pub fn longrope_short_only_spec(cfg: &Cfg, head_dim: usize, theta_default: Option<f64>) -> Result<Option<RopeSpec>> {
    let arch = &cfg.arch;
    let rc = read_rope_config(cfg, theta_default, None)?;
    match rc.rope_type.as_str() {
        "default" => return Ok(None),
        "longrope" | "su" => {}
        other => return Err(LowerError::not_lowerable(format!("{arch}: rope type `{other}` is not modelled"))),
    }
    let list = |k: &str| -> Result<Vec<f64>> {
        rc.params
            .get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<f64>>())
            .ok_or_else(|| LowerError::bad(format!("{arch}: longrope without `{k}`")))
    };
    let num = |k: &str| rc.params.get(k).and_then(Value::as_f64);
    let short = list("short_factor")?;
    list("long_factor")?;
    if short.len() != head_dim / 2 {
        return Err(LowerError::bad(format!("{arch}: short_factor has {} entries for head dim {head_dim}", short.len())));
    }
    let (sm, lm) = match (num("short_mscale"), num("long_mscale")) {
        (Some(a), Some(b)) => (a, b),
        _ => return Err(LowerError::not_lowerable(format!("{arch}: longrope without short_mscale and long_mscale"))),
    };
    if sm != lm {
        return Err(LowerError::not_lowerable(format!(
            "{arch}: short_mscale {sm} ≠ long_mscale {lm} (a cos/sin factor that changes at the original length)"
        )));
    }
    if let Some(k) = rc.params.keys().find(|k| {
        !["short_factor", "long_factor", "short_mscale", "long_mscale", "original_max_position_embeddings", "factor", "attention_factor"].contains(&k.as_str())
    }) {
        return Err(LowerError::not_lowerable(format!("{arch}: rope parameter `{k}` is not modelled")));
    }
    Ok(Some(RopeSpec {
        rotary_dim: head_dim,
        offset: 0,
        style: RopeStyle::Half,
        freqs: RopeFreqs {
            rope_type: rc.rope_type.clone(),
            theta: rc.theta,
            dim: head_dim,
            inv_freq: longrope_inv_freq(rc.theta, head_dim, &short),
            attention_factor: sm,
            dynamic: None,
            longrope: None,
            mrope: None,
        },
    }))
}

/// Read `rope_theta` + `rope_scaling` (transformers 4.x) or `rope_parameters` (5.x; flat, or keyed
/// by layer type as Gemma-3 does). `theta_default` is the arch's default when no theta is given.
pub fn read_rope_config(cfg: &Cfg, theta_default: Option<f64>, layer_type: Option<&str>) -> Result<RopeConfig> {
    let theta_top = cfg.opt_f64("rope_theta")?;
    let scaling = cfg.opt_obj("rope_scaling")?;
    let params = cfg.opt_obj("rope_parameters")?;
    let flat: Option<Map<String, Value>> = match params {
        Some(p) if p.contains_key("rope_type") || p.contains_key("rope_theta") || p.contains_key("type") => Some(p.clone()),
        Some(p) => match layer_type {
            Some(lt) => match p.get(lt) {
                Some(Value::Object(o)) => Some(o.clone()),
                _ => {
                    return Err(LowerError::not_lowerable(format!(
                        "{}: rope_parameters has no entry for layer type `{lt}`",
                        cfg.arch
                    )));
                }
            },
            None => {
                return Err(LowerError::not_lowerable(format!(
                    "{}: rope_parameters is keyed by layer type but the layer has none",
                    cfg.arch
                )));
            }
        },
        None => scaling.cloned(),
    };
    let mut map = flat.unwrap_or_default();
    let theta = match map.remove("rope_theta").and_then(|v| v.as_f64()) {
        Some(t) => t,
        None => match theta_top.or(theta_default) {
            Some(t) => t,
            None => return Err(LowerError::bad(format!("{}: no rope_theta", cfg.arch))),
        },
    };
    let rope_type = match (map.remove("rope_type"), map.remove("type")) {
        (Some(Value::String(a)), _) => a,
        (_, Some(Value::String(b))) => b,
        _ => "default".to_string(),
    };
    Ok(RopeConfig { rope_type, theta, params: map })
}

fn take_f64(m: &mut Map<String, Value>, k: &str) -> Result<Option<f64>> {
    match m.remove(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().map(Some).ok_or_else(|| LowerError::bad(format!("rope `{k}` is not a number"))),
    }
}
fn take_usize(m: &mut Map<String, Value>, k: &str) -> Result<Option<usize>> {
    Ok(take_f64(m, k)?.map(|f| f as usize))
}
fn take_f64_list(m: &mut Map<String, Value>, k: &str) -> Result<Option<Vec<f64>>> {
    match m.remove(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_f64().ok_or_else(|| LowerError::bad(format!("rope `{k}` holds a non-number"))))
            .collect::<Result<Vec<_>>>()
            .map(Some),
        Some(_) => Err(LowerError::bad(format!("rope `{k}` is not a list"))),
    }
}

/// LongRoPE's inverse frequencies for extension factors `ext` (`dim / 2` of them), in float32 as HF
/// computes them: `1 / (ext_i · base^(2i/dim))`.
pub fn longrope_inv_freq(base: f64, dim: usize, ext: &[f64]) -> Vec<f32> {
    (0..dim / 2)
        .map(|i| {
            let e = (2 * i) as f32 / dim as f32;
            let p = crate::detmath::powf(base, e as f64) as f32;
            1.0f32 / (ext[i] as f32 * p)
        })
        .collect()
}

fn get_mscale(scale: f64, mscale: f64) -> f64 {
    if scale <= 1.0 { 1.0 } else { 0.1 * mscale * crate::detmath::ln(scale) + 1.0 }
}

/// Everything the frequency computation needs besides the rope dict.
#[derive(Clone, Copy, Debug)]
pub struct RopeContext {
    /// Rotary dimension (frequency formula's `dim`).
    pub dim: usize,
    pub max_position_embeddings: Option<usize>,
    /// Phi-3 keeps `original_max_position_embeddings` at the top level of the config.
    pub top_level_original_max: Option<usize>,
    /// The partial factor the caller applied; a `partial_rotary_factor` inside the rope dict must agree.
    pub partial_rotary_factor: f64,
}

/// Compute the frequencies for a rope config. Unknown keys in the rope dict are refused.
pub fn compute_freqs(arch: &str, rc: &RopeConfig, ctx: RopeContext) -> Result<RopeFreqs> {
    let mut m = rc.params.clone();
    let dim = ctx.dim;
    if dim == 0 || !dim.is_multiple_of(2) {
        return Err(LowerError::bad(format!("{arch}: rotary dim {dim} must be even and positive")));
    }
    // `proportional` (Gemma-4's full-attention rope, `ROPE_PROPORTIONAL_V1`): frequencies on the
    // first `partial_rotary_factor` of the head width, zeros after (those pairs are not rotated), all
    // divided by `factor`; its `partial_rotary_factor` is a property of the rope, not the caller's.
    if rc.rope_type == "proportional" {
        let prop = take_f64(&mut m, "partial_rotary_factor")?.unwrap_or(1.0);
        let factor = take_f64(&mut m, "factor")?.unwrap_or(1.0);
        if let Some(k) = m.keys().next() {
            return Err(LowerError::not_lowerable(format!("{arch}: proportional rope parameter `{k}`")));
        }
        // transformers: `rope_angles = int(prop · head_dim // 2)` frequencies over the head width.
        let angles = (prop * dim as f64 / 2.0).floor() as usize;
        let mut out = RopeFreqs::plain(rc.theta, dim);
        out.rope_type = rc.rope_type.clone();
        out.inv_freq = (0..dim / 2)
            .map(|i| {
                if i < angles {
                    let e = (2 * i) as f32 / dim as f32;
                    (1.0f32 / crate::detmath::powf(rc.theta, e as f64) as f32) / factor as f32
                } else {
                    0.0
                }
            })
            .collect();
        return Ok(out);
    }
    if let Some(p) = take_f64(&mut m, "partial_rotary_factor")?
        && (p - ctx.partial_rotary_factor).abs() > 1e-9
    {
        return Err(LowerError::not_lowerable(format!(
            "{arch}: rope dict partial_rotary_factor {p} disagrees with the config's {}",
            ctx.partial_rotary_factor
        )));
    }
    let base = rc.theta;
    let mut out = RopeFreqs::plain(base, dim);
    out.rope_type = rc.rope_type.clone();
    let refuse_leftover = |m: &Map<String, Value>| -> Result<()> {
        if m.is_empty() {
            Ok(())
        } else {
            let keys: Vec<&String> = m.keys().collect();
            Err(LowerError::not_lowerable(format!(
                "{arch}: rope type `{}` with key(s) {keys:?} this lowerer does not model",
                rc.rope_type
            )))
        }
    };
    match rc.rope_type.as_str() {
        "default" | "mrope" => {
            // The multimodal rope: with the three position components equal (text only) every
            // frequency reads the same position, which is the plain rope. The layout is kept: with
            // an image the components differ (`crate::lower::ImageRows::mrope`).
            let interleaved = m.remove("mrope_interleaved").and_then(|v| v.as_bool()) == Some(true);
            if let Some(v) = m.remove("mrope_section") {
                let s: Vec<usize> = v.as_array().map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as usize).collect()).unwrap_or_default();
                // HF splits the contiguous layout by the sections, which must cover the frequencies.
                if s.len() != 3 || (!interleaved && s.iter().sum::<usize>() != dim / 2) {
                    return Err(LowerError::bad(format!("{arch}: mrope_section {s:?} does not split {} frequencies in three", dim / 2)));
                }
                out.mrope = Some(MRope { section: [s[0], s[1], s[2]], interleaved });
            }
            refuse_leftover(&m)?;
        }
        "linear" => {
            let f = take_f64(&mut m, "factor")?.ok_or_else(|| LowerError::bad(format!("{arch}: linear rope without factor")))?;
            refuse_leftover(&m)?;
            out.inv_freq = out.inv_freq.iter().map(|x| x / f as f32).collect();
        }
        "dynamic" => {
            let f = take_f64(&mut m, "factor")?.ok_or_else(|| LowerError::bad(format!("{arch}: dynamic rope without factor")))?;
            let max_pos = ctx
                .max_position_embeddings
                .ok_or_else(|| LowerError::bad(format!("{arch}: dynamic rope needs max_position_embeddings")))?;
            if let Some(o) = take_usize(&mut m, "original_max_position_embeddings")?
                && o != max_pos
            {
                return Err(LowerError::not_lowerable(format!(
                    "{arch}: dynamic rope with original_max_position_embeddings {o} ≠ max_position_embeddings {max_pos} (HF reads the latter)"
                )));
            }
            refuse_leftover(&m)?;
            out.dynamic = Some(DynamicNtk { factor: f, max_pos });
        }
        "yarn" => {
            let max_pos = ctx.max_position_embeddings;
            let orig = take_usize(&mut m, "original_max_position_embeddings")?;
            let factor = match take_f64(&mut m, "factor")? {
                Some(f) => f,
                None => match (max_pos, orig) {
                    (Some(a), Some(b)) => a as f64 / b as f64,
                    _ => return Err(LowerError::bad(format!("{arch}: yarn without factor"))),
                },
            };
            let original_max = orig.or(max_pos).ok_or_else(|| LowerError::bad(format!("{arch}: yarn needs a max position")))?;
            let attention_factor = take_f64(&mut m, "attention_factor")?;
            let mscale = take_f64(&mut m, "mscale")?;
            let mscale_all_dim = take_f64(&mut m, "mscale_all_dim")?;
            let beta_fast = take_f64(&mut m, "beta_fast")?.filter(|v| *v != 0.0).unwrap_or(32.0);
            let beta_slow = take_f64(&mut m, "beta_slow")?.filter(|v| *v != 0.0).unwrap_or(1.0);
            let truncate = match m.remove("truncate") {
                None | Some(Value::Null) => true,
                Some(Value::Bool(b)) => b,
                Some(v) => return Err(LowerError::bad(format!("{arch}: yarn truncate = {v}"))),
            };
            refuse_leftover(&m)?;
            out.attention_factor = match attention_factor {
                Some(a) => a,
                None => match (mscale, mscale_all_dim) {
                    (Some(a), Some(b)) if a != 0.0 && b != 0.0 => get_mscale(factor, a) / get_mscale(factor, b),
                    _ => get_mscale(factor, 1.0),
                },
            };
            out.inv_freq = yarn_inv_freq(base, dim, factor, original_max, beta_fast, beta_slow, truncate);
        }
        "llama3" => {
            let factor = take_f64(&mut m, "factor")?.ok_or_else(|| LowerError::bad(format!("{arch}: llama3 rope without factor")))?;
            let low = take_f64(&mut m, "low_freq_factor")?
                .ok_or_else(|| LowerError::bad(format!("{arch}: llama3 rope without low_freq_factor")))?;
            let high = take_f64(&mut m, "high_freq_factor")?
                .ok_or_else(|| LowerError::bad(format!("{arch}: llama3 rope without high_freq_factor")))?;
            let old = take_f64(&mut m, "original_max_position_embeddings")?
                .ok_or_else(|| LowerError::bad(format!("{arch}: llama3 rope without original_max_position_embeddings")))?;
            refuse_leftover(&m)?;
            out.inv_freq = llama3_inv_freq(&out.inv_freq, factor, low, high, old);
        }
        "longrope" | "su" => {
            let long = take_f64_list(&mut m, "long_factor")?
                .ok_or_else(|| LowerError::bad(format!("{arch}: longrope without long_factor")))?;
            let short = take_f64_list(&mut m, "short_factor")?
                .ok_or_else(|| LowerError::bad(format!("{arch}: longrope without short_factor")))?;
            let dict_factor = take_f64(&mut m, "factor")?;
            let attention_factor = take_f64(&mut m, "attention_factor")?;
            let dict_orig = take_usize(&mut m, "original_max_position_embeddings")?;
            refuse_leftover(&m)?;
            if long.len() != dim / 2 || short.len() != dim / 2 {
                return Err(LowerError::bad(format!(
                    "{arch}: longrope factor lists have {}/{} entries, rotary dim/2 is {}",
                    long.len(),
                    short.len(),
                    dim / 2
                )));
            }
            let max_pos = ctx
                .max_position_embeddings
                .ok_or_else(|| LowerError::bad(format!("{arch}: longrope needs max_position_embeddings")))?;
            // HF (4.4x–4.5x): a top-level `original_max_position_embeddings` wins and the factor
            // becomes max/original; otherwise the dict factor (or 1) with original = max.
            let (original_max, factor) = match ctx.top_level_original_max.or(dict_orig) {
                Some(o) => (o, max_pos as f64 / o as f64),
                None => (max_pos, dict_factor.unwrap_or(1.0)),
            };
            out.attention_factor = match attention_factor {
                Some(a) => a,
                None if factor <= 1.0 => 1.0,
                None => (1.0 + crate::detmath::ln(factor) / crate::detmath::ln(original_max as f64)).sqrt(),
            };
            out.inv_freq = longrope_inv_freq(base, dim, &short);
            out.longrope = Some(LongRopeSwitch { inv_freq_long: longrope_inv_freq(base, dim, &long), original_max });
        }
        other => return Err(LowerError::not_lowerable(format!("{arch}: rope type `{other}` is not modelled"))),
    }
    Ok(out)
}

/// YaRN (`_compute_yarn_parameters`).
pub fn yarn_inv_freq(
    base: f64,
    dim: usize,
    factor: f64,
    original_max: usize,
    beta_fast: f64,
    beta_slow: f64,
    truncate: bool,
) -> Vec<f32> {
    let two_pi = 2.0 * std::f64::consts::PI;
    let find_dim = |rot: f64| (dim as f64 * crate::detmath::ln(original_max as f64 / (rot * two_pi))) / (2.0 * crate::detmath::ln(base));
    let (mut low, mut high) = (find_dim(beta_fast), find_dim(beta_slow));
    if truncate {
        low = low.floor();
        high = high.ceil();
    }
    let low = low.max(0.0);
    let high = high.min(dim as f64 - 1.0);
    let (lo, mut hi) = (low, high);
    if lo == hi {
        hi += 0.001;
    }
    (0..dim / 2)
        .map(|i| {
            let e = (2 * i) as f32 / dim as f32;
            let pos_freq = crate::detmath::powf(base, e as f64) as f32;
            let extra = 1.0f32 / pos_freq;
            let inter = 1.0f32 / (factor as f32 * pos_freq);
            let ramp = (((i as f32) - lo as f32) / (hi as f32 - lo as f32)).clamp(0.0, 1.0);
            let extra_factor = 1.0 - ramp;
            inter * (1.0 - extra_factor) + extra * extra_factor
        })
        .collect()
}

/// Llama 3.1 (`_compute_llama3_parameters`).
pub fn llama3_inv_freq(inv: &[f32], factor: f64, low: f64, high: f64, old_ctx: f64) -> Vec<f32> {
    let low_wavelen = old_ctx / low;
    let high_wavelen = old_ctx / high;
    inv.iter()
        .map(|&f| {
            let f = f as f64;
            let wavelen = 2.0 * std::f64::consts::PI / f;
            if wavelen < high_wavelen {
                f as f32
            } else if wavelen > low_wavelen {
                (f / factor) as f32
            } else {
                let smooth = (old_ctx / wavelen - low) / (high - low);
                ((1.0 - smooth) * f / factor + smooth * f) as f32
            }
        })
        .collect()
}

/// ALiBi as a per-head slope on the key distance: `score += −slope_h · (i − j)` (BLOOM, Falcon,
/// MPT). HF adds `slope · j` (or `slope · (j − i)`); a per-row constant does not change a softmax.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlibiSpec {
    pub slopes: Vec<f64>,
    /// Falcon adds the bias BEFORE multiplying by the softmax scale, so the bias is scaled too.
    pub scaled_by_softmax_scale: bool,
    /// Falcon rounds the slopes and the products to bfloat16 (`slopes.bfloat16() * arange`).
    pub bf16_bias: bool,
}

/// BLOOM / Falcon slopes (`build_alibi_tensor`).
pub fn alibi_slopes_bloom(n: usize) -> Vec<f64> {
    let closest = 1usize << (usize::BITS - 1 - n.leading_zeros());
    let base = crate::detmath::powf(2.0, -crate::detmath::powf(2.0, -((closest as f64).log2() - 3.0)));
    let mut s: Vec<f64> = (1..=closest).map(|p| (base as f32).powi(p as i32) as f64).collect();
    if closest != n {
        let extra_base = crate::detmath::powf(2.0, -crate::detmath::powf(2.0, -((2.0 * closest as f64).log2() - 3.0)));
        let remaining = closest.min(n - closest);
        s.extend((0..remaining).map(|i| (extra_base as f32).powi((1 + 2 * i) as i32) as f64));
    }
    s
}

/// MPT slopes (`build_mpt_alibi_tensor`).
pub fn alibi_slopes_mpt(n: usize, alibi_bias_max: f64) -> Vec<f64> {
    let p2 = n.next_power_of_two();
    let slopes: Vec<f64> = (1..=p2).map(|i| 1.0 / crate::detmath::powf(2.0, i as f64 * (alibi_bias_max / p2 as f64))).collect();
    if p2 == n {
        return slopes;
    }
    let mut r: Vec<f64> = slopes.iter().skip(1).step_by(2).copied().collect();
    r.extend(slopes.iter().step_by(2).copied());
    r.truncate(n);
    r
}

/// bfloat16 round-to-nearest-even of an f32 (Falcon's ALiBi path).
pub fn bf16_round(x: f32) -> f32 {
    let b = x.to_bits();
    if x.is_nan() {
        return x;
    }
    let lsb = (b >> 16) & 1;
    let rounded = b.wrapping_add(0x7FFF + lsb) & 0xFFFF_0000;
    f32::from_bits(rounded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rc(t: &str, theta: f64, v: Value) -> RopeConfig {
        RopeConfig { rope_type: t.into(), theta, params: v.as_object().cloned().unwrap_or_default() }
    }
    fn ctx(dim: usize, max: usize) -> RopeContext {
        RopeContext { dim, max_position_embeddings: Some(max), top_level_original_max: None, partial_rotary_factor: 1.0 }
    }

    #[test]
    fn default_frequencies_are_theta_to_the_minus_2i_over_d() {
        let f = compute_freqs("t", &rc("default", 10000.0, json!({})), ctx(8, 2048)).unwrap();
        let want = [1.0, 0.1, 0.01, 0.001];
        for (a, b) in f.inv_freq.iter().zip(want) {
            assert!(((*a as f64) - b).abs() < 1e-6 * b.max(1e-3), "{a} vs {b}");
        }
        let (c, s) = f.cos_sin(3);
        assert!((c[0] - 3f32.cos()).abs() < 1e-6 && (s[0] - 3f32.sin()).abs() < 1e-6);
        assert!((c[1] - 0.3f32.cos()).abs() < 1e-6);
    }

    /// The two M-RoPE layouts, against HF: Qwen2-VL's `cos.split(mrope_section · 2)` (contiguous
    /// `t`, `h`, `w`) and Qwen3.5's `recomposition_frequencies` (`h` at `1, 4, …` below `3 · 11`,
    /// `w` at `2, 5, …` below `3 · 10`, `t` elsewhere).
    #[test]
    fn m_rope_layouts_assign_each_frequency_as_hf_does() {
        let f = compute_freqs("t", &rc("default", 1e4, json!({"mrope_section": [2, 1, 1]})), ctx(8, 128)).unwrap();
        let mr = f.mrope.unwrap();
        assert!(!mr.interleaved);
        assert_eq!((0..4).map(|j| mr.component(j)).collect::<Vec<_>>(), [0, 0, 1, 2]);
        let f = compute_freqs("t", &rc("default", 1e7, json!({"mrope_section": [11, 11, 10], "mrope_interleaved": true})), ctx(64, 128)).unwrap();
        let mr = f.mrope.unwrap();
        let hf: Vec<usize> = (0..32).map(|j| if j % 3 == 1 && j < 33 { 1 } else if j % 3 == 2 && j < 30 { 2 } else { 0 }).collect();
        assert_eq!((0..32).map(|j| mr.component(j)).collect::<Vec<_>>(), hf);
        assert_eq!(hf.iter().filter(|k| **k == 1).count(), 11);
        assert_eq!(hf.iter().filter(|k| **k == 2).count(), 10);
        // A contiguous layout that does not cover the frequencies is HF's error too.
        assert!(compute_freqs("t", &rc("default", 1e4, json!({"mrope_section": [2, 1, 2]})), ctx(8, 128)).is_err());
    }

    #[test]
    fn linear_divides_every_frequency_by_the_factor() {
        let f = compute_freqs("t", &rc("linear", 10000.0, json!({"factor": 4.0})), ctx(8, 2048)).unwrap();
        assert!((f.inv_freq[0] - 0.25).abs() < 1e-7);
        assert!((f.inv_freq[1] - 0.025).abs() < 1e-8);
        assert!(compute_freqs("t", &rc("linear", 10000.0, json!({"factor": 4.0, "whatever": 1})), ctx(8, 2048)).is_err());
    }

    #[test]
    fn llama3_keeps_high_frequencies_scales_low_ones_and_blends_the_middle() {
        // Llama 3.1: factor 8, low 1, high 4, original 8192, theta 500000, head_dim 128.
        let f = compute_freqs(
            "t",
            &rc(
                "llama3",
                500000.0,
                json!({"factor": 8.0, "low_freq_factor": 1.0, "high_freq_factor": 4.0, "original_max_position_embeddings": 8192}),
            ),
            ctx(128, 131072),
        )
        .unwrap();
        let plain = default_inv_freq(500000.0, 128);
        // i = 0: wavelen 2π < 2048 → unchanged.
        assert_eq!(f.inv_freq[0], plain[0]);
        // Last: wavelen ≫ 8192 → divided by 8.
        let last = plain.len() - 1;
        assert!((f.inv_freq[last] - plain[last] / 8.0).abs() <= plain[last] * 1e-6);
        // A middle frequency: wavelen between 2048 and 8192 → smooth blend, strictly between.
        let mid = (0..plain.len())
            .find(|&i| {
                let w = 2.0 * std::f64::consts::PI / plain[i] as f64;
                w > 2048.0 && w < 8192.0
            })
            .unwrap();
        let w = 2.0 * std::f64::consts::PI / plain[mid] as f64;
        let smooth = (8192.0 / w - 1.0) / 3.0;
        let want = (1.0 - smooth) * plain[mid] as f64 / 8.0 + smooth * plain[mid] as f64;
        assert!(((f.inv_freq[mid] as f64) - want).abs() < want * 1e-6);
    }

    #[test]
    fn yarn_ramps_between_the_correction_dims_and_sets_the_mscale() {
        // DeepSeek-V3: factor 40, beta 32/1, mscale = mscale_all_dim = 1 → attention factor 1.
        let f = compute_freqs(
            "t",
            &rc(
                "yarn",
                10000.0,
                json!({"factor": 40, "beta_fast": 32, "beta_slow": 1, "mscale": 1.0, "mscale_all_dim": 1.0, "original_max_position_embeddings": 4096}),
            ),
            ctx(64, 163840),
        )
        .unwrap();
        assert!((f.attention_factor - 1.0).abs() < 1e-12);
        let plain = default_inv_freq(10000.0, 64);
        // Below `low` the frequency is extrapolated (unchanged); above `high`, interpolated (/40).
        let fd = |rot: f64| (64.0 * (4096.0 / (rot * 2.0 * std::f64::consts::PI)).ln()) / (2.0 * 10000f64.ln());
        let (low, high) = (fd(32.0).floor() as usize, fd(1.0).ceil() as usize);
        assert!(low > 0 && high < 31, "{low} {high}");
        assert_eq!(f.inv_freq[0], plain[0]);
        assert!((f.inv_freq[31] - plain[31] / 40.0).abs() <= plain[31] * 1e-6);
        assert!(
            f.inv_freq[(low + high) / 2] < plain[(low + high) / 2] && f.inv_freq[(low + high) / 2] > plain[(low + high) / 2] / 40.0
        );
        // No mscale given → 0.1·ln(factor)+1.
        let g = compute_freqs(
            "t",
            &rc("yarn", 10000.0, json!({"factor": 4.0, "original_max_position_embeddings": 32768})),
            ctx(64, 131072),
        )
        .unwrap();
        assert!((g.attention_factor - (0.1 * 4f64.ln() + 1.0)).abs() < 1e-12);
        let (c, _) = g.cos_sin(0);
        assert!((c[0] as f64 - g.attention_factor).abs() < 1e-6, "cos(0) carries the attention factor");
    }

    #[test]
    fn yarn_truncate_false_keeps_fractional_correction_dims() {
        let a = yarn_inv_freq(150000.0, 64, 32.0, 4096, 32.0, 1.0, true);
        let b = yarn_inv_freq(150000.0, 64, 32.0, 4096, 32.0, 1.0, false);
        assert_ne!(a, b, "gpt-oss sets truncate=false; the ramp must move");
    }

    #[test]
    fn dynamic_ntk_changes_the_base_only_past_max_position() {
        let f = compute_freqs("t", &rc("dynamic", 10000.0, json!({"factor": 2.0})), ctx(8, 16)).unwrap();
        assert_eq!(f.inv_freq_at(15), f.inv_freq);
        let at = f.inv_freq_at(31);
        let base = 10000.0 * ((2.0 * 32.0 / 16.0) - 1.0f64).powf(8.0 / 6.0);
        assert_eq!(at, default_inv_freq(base, 8));
        assert!(at[1] < f.inv_freq[1]);
    }

    #[test]
    fn longrope_switches_tables_past_the_original_length_and_scales_attention() {
        let dim = 4;
        let r = rc("longrope", 10000.0, json!({"long_factor": [1.0, 4.0], "short_factor": [1.0, 2.0]}));
        let mut c = ctx(dim, 131072);
        c.top_level_original_max = Some(4096);
        let f = compute_freqs("t", &r, c).unwrap();
        let want_af = (1.0 + 32f64.ln() / 4096f64.ln()).sqrt();
        assert!((f.attention_factor - want_af).abs() < 1e-12);
        assert!((f.inv_freq_at(4095)[1] - 0.01 / 2.0).abs() < 1e-8);
        assert!((f.inv_freq_at(4096)[1] - 0.01 / 4.0).abs() < 1e-8);
    }

    #[test]
    fn alibi_slopes_match_the_papers_geometric_sequence() {
        let s = alibi_slopes_bloom(8);
        for (i, v) in s.iter().enumerate() {
            assert!((v - 2f64.powi(-(i as i32 + 1))).abs() < 1e-7);
        }
        // 12 heads: 8 from base 2^-1, then 4 odd powers of 2^-0.5.
        let s = alibi_slopes_bloom(12);
        assert_eq!(s.len(), 12);
        assert!((s[8] - 2f64.powf(-0.5)).abs() < 1e-6);
        assert!((s[9] - 2f64.powf(-1.5)).abs() < 1e-6);
        let m = alibi_slopes_mpt(8, 8.0);
        assert!((m[0] - 0.5).abs() < 1e-12 && (m[7] - 1.0 / 256.0).abs() < 1e-12);
        let m = alibi_slopes_mpt(6, 8.0);
        // Non power of two: odd-indexed slopes of the 8-head set first, then even-indexed.
        let full = alibi_slopes_mpt(8, 8.0);
        assert_eq!(m, vec![full[1], full[3], full[5], full[7], full[0], full[2]]);
    }

    #[test]
    fn bf16_rounding_is_nearest_even() {
        assert_eq!(bf16_round(1.0), 1.0);
        assert_eq!(bf16_round(1.0 + 1.0 / 256.0), 1.0, "a tie rounds to the even mantissa");
        assert_eq!(bf16_round(1.0 + 3.0 / 256.0), 1.0 + 4.0 / 256.0);
        assert_eq!(bf16_round(0.70710677), 0.70703125);
    }
}

/// A rope spec from a config's rope fields (`rope_theta` + `rope_scaling`, or `rope_parameters`, flat or
/// keyed by layer type), and the query temperature a `llama_4_scaling_beta` among them asks for
/// (Ministral-3: `q ·= 1 + β·ln(1 + ⌊p / original_max_position_embeddings⌋)`). The temperature may
/// only be carried by an architecture that applies it (`q_scaled`): elsewhere the key is refused,
/// never dropped. Shared by the Rust parsers and the adapter evaluator (`rope` is a generic feature;
/// which config keys feed it is the adapter's business).
#[allow(clippy::too_many_arguments)]
pub fn rope_spec_from_config(
    cfg: &Cfg,
    rotary_dim: usize,
    style: RopeStyle,
    theta_default: Option<f64>,
    layer_type: Option<&str>,
    partial: f64,
    max_pos: Option<usize>,
    top_orig: Option<usize>,
    q_scaled: bool,
) -> Result<(RopeSpec, Option<crate::spec::QTemperature>)> {
    use crate::spec::QTemperature;
    let mut rc = read_rope_config(cfg, theta_default, layer_type)?;
    let beta = match rc.params.remove("llama_4_scaling_beta") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().ok_or_else(|| LowerError::bad(format!("{}: llama_4_scaling_beta {v}", cfg.arch)))?),
    };
    if beta.is_some() && !q_scaled {
        return Err(LowerError::not_lowerable(format!(
            "{}: llama_4_scaling_beta (a query scaling) on an architecture that does not apply it",
            cfg.arch
        )));
    }
    // transformers 5 also keeps the model's own length among the rope parameters.
    if let Some(v) = rc.params.remove("max_position_embeddings")
        && v.as_u64().map(|m| m as usize) != max_pos
    {
        return Err(LowerError::bad(format!("{}: rope_parameters.max_position_embeddings {v} ≠ {max_pos:?}", cfg.arch)));
    }
    let temp = match beta {
        Some(b) => {
            let floor = rc.params.get("original_max_position_embeddings").and_then(Value::as_u64).ok_or_else(|| {
                LowerError::bad(format!("{}: llama_4_scaling_beta without original_max_position_embeddings", cfg.arch))
            })?;
            Some(QTemperature { floor: floor as usize, scale: b, offset: 0 })
        }
        None => None,
    };
    let freqs = compute_freqs(
        &cfg.arch,
        &rc,
        RopeContext { dim: rotary_dim, max_position_embeddings: max_pos, top_level_original_max: top_orig, partial_rotary_factor: partial },
    )?;
    Ok((RopeSpec { rotary_dim, offset: 0, style, freqs }, temp))
}
