//! **The Hugging Face reference an integer program is held to** — and the unit check.
//!
//! A pack carries (as sidecars) the float logits `transformers` computes for a few sequences. The
//! program's logits, read as `code × scale`, must agree with them: not bit for bit (the program is
//! quantised) but in kind — the same units, the same order, the same shape of distribution. The fit
//! measures it:
//!
//! * `slope`: the least-squares slope of the HF logit on the program's value over every entry (≈ 1:
//!   a program whose codes are in other units than natural-log logits has a slope far from 1 — the
//!   check that makes a logit convention a fact);
//! * `corr`, `max_abs`, `rmse` of the entries;
//! * `top1`: the fraction of positions where both pick the same token;
//! * `kl_mean`: KL(HF ‖ program) per position, softmax over the vocabulary.
//!
//! Three forms are read: a pack sidecar (`hf-reference.json` + `hf-reference.f32`), an audit directory
//! (`hf.json` + `hf-logits.f32`, as `tools/audit_e2e.py` writes them), and a fixture's `logits.json`.

use super::conformance::LoadedArtifact;
use super::manifest::{FitRec, ToleranceRec};
use misaka_palw_tir_exec::{NodeValue, ParamData, StepSink, TirExecutor, TirParams, TirPlan};
use serde_json::{Value, json};
use std::io::Read;
use std::path::Path;

pub const HF_REFERENCE_SCHEMA_V1: &str = "misaka.palw.hf-reference.v1";
pub const HF_REFERENCE_FILE: &str = "hf-reference.json";
pub const HF_REFERENCE_LOGITS_FILE: &str = "hf-reference.f32";

#[derive(Clone, Debug, PartialEq)]
pub struct HfSequence {
    pub tokens: Vec<usize>,
    /// `tokens.len() × vocab` logits, row-major.
    pub logits: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HfReference {
    pub producer: Value,
    pub vocab: usize,
    pub sequences: Vec<HfSequence>,
}

fn f32s(bytes: &[u8]) -> Result<Vec<f32>, String> {
    if !bytes.len().is_multiple_of(4) {
        return Err("hf-reference: truncated f32".into());
    }
    let values: Vec<f32> = bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    if values.iter().any(|x| !x.is_finite()) {
        return Err("hf-reference: non-finite logit".into());
    }
    Ok(values)
}
fn tokens(v: &Value) -> Result<Vec<usize>, String> {
    v.as_array()
        .ok_or("hf-reference: tokens must be an array")?
        .iter()
        .map(|t| t.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or("hf-reference: invalid token".into()))
        .collect()
}
fn extent(at: usize, positions: usize, vocab: usize) -> Result<std::ops::Range<usize>, String> {
    let end = positions.checked_mul(vocab).and_then(|n| at.checked_add(n)).ok_or("hf-reference: extent overflow")?;
    Ok(at..end)
}

impl HfReference {
    pub fn positions(&self) -> usize {
        self.sequences.iter().map(|s| s.tokens.len()).sum()
    }

    /// Read a reference from a pack sidecar, an audit directory or a fixture's `logits.json` (a path to
    /// the file or to the directory holding it).
    pub fn load(path: &Path) -> Result<HfReference, String> {
        let dir = if path.is_dir() { path.to_path_buf() } else { path.parent().unwrap_or(Path::new(".")).to_path_buf() };
        let file = if path.is_dir() { None } else { path.file_name().and_then(|n| n.to_str()) };
        let rd = |p: &Path| -> Result<Vec<u8>, String> {
            let max = if p.extension().is_some_and(|x| x == "json") { 2 << 20 } else { 256 << 20 };
            let mut bytes = Vec::new();
            std::fs::File::open(p)
                .map_err(|e| format!("{}: {e}", p.display()))?
                .take(max + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > max {
                return Err("hf-reference: file byte limit".into());
            }
            Ok(bytes)
        };
        let json_of =
            |p: &Path| -> Result<Value, String> { serde_json::from_slice(&rd(p)?).map_err(|e| format!("{}: {e}", p.display())) };
        if file == Some(HF_REFERENCE_FILE) || (file.is_none() && dir.join(HF_REFERENCE_FILE).exists()) {
            let v = json_of(&dir.join(HF_REFERENCE_FILE))?;
            if v["schema"] != HF_REFERENCE_SCHEMA_V1 {
                return Err(format!("{HF_REFERENCE_FILE}: schema is not {HF_REFERENCE_SCHEMA_V1}"));
            }
            let vocab = v["vocab"].as_u64().and_then(|n| usize::try_from(n).ok()).ok_or("hf-reference: no vocab")?;
            let name = v["logits_file"].as_str().unwrap_or(HF_REFERENCE_LOGITS_FILE);
            super::manifest::safe_name(name)?;
            if name.contains(['\\', ':']) {
                return Err("hf-reference: portable relative logits path required".into());
            }
            let all = f32s(&rd(&dir.join(name))?)?;
            let mut at = 0;
            let mut sequences = Vec::new();
            for s in v["sequences"].as_array().ok_or("hf-reference: no sequences")? {
                let tokens = tokens(&s["tokens"])?;
                let range = extent(at, tokens.len(), vocab)?;
                let logits = all.get(range.clone()).ok_or("hf-reference: the logits file is shorter than the sequences say")?.to_vec();
                at = range.end;
                sequences.push(HfSequence { tokens, logits });
            }
            if at != all.len() {
                return Err("hf-reference: the logits file is longer than the sequences say".into());
            }
            return HfReference { producer: plain(&v["producer"]), vocab, sequences }.checked();
        }
        if file == Some("logits.json") || (file.is_none() && dir.join("logits.json").exists() && !dir.join("hf.json").exists()) {
            let v = json_of(&dir.join("logits.json"))?;
            let tokens = tokens(&v["tokens"])?;
            let rows = v["logits_full"].as_array().ok_or("logits.json: no logits_full")?;
            let vocab = rows.first().and_then(|r| r.as_array()).map_or(0, Vec::len);
            let logits: Vec<f32> =
                rows.iter().flat_map(|r| r.as_array().into_iter().flatten().map(|x| x.as_f64().unwrap_or(f64::NAN) as f32)).collect();
            if rows.len() != tokens.len()
                || rows.iter().any(|row| row.as_array().is_none_or(|r| r.len() != vocab))
                || logits.len() != extent(0, tokens.len(), vocab)?.end
            {
                return Err("logits.json: the rows and the tokens disagree".into());
            }
            return HfReference {
                producer: plain(
                    &json!({ "transformers": v["transformers"], "torch": v["torch"], "dtype": "float32", "from": "logits.json" }),
                ),
                vocab,
                sequences: vec![HfSequence { tokens, logits }],
            }
            .checked();
        }
        // An audit directory: hf.json (sequences, vocab, version) and hf-logits.f32 (their rows in order).
        let v = json_of(&dir.join("hf.json"))?;
        let vocab = v["vocab"].as_u64().and_then(|n| usize::try_from(n).ok()).ok_or("hf.json: no vocab")?;
        let all = f32s(&rd(&dir.join("hf-logits.f32"))?)?;
        let mut at = 0;
        let mut sequences = Vec::new();
        for s in v["sequences"].as_array().ok_or("hf.json: no sequences")? {
            let tokens = tokens(s)?;
            let range = extent(at, tokens.len(), vocab)?;
            let logits = all.get(range.clone()).ok_or("hf-logits.f32 is shorter than hf.json's sequences")?.to_vec();
            at = range.end;
            sequences.push(HfSequence { tokens, logits });
        }
        if at != all.len() {
            return Err("hf-reference: extra audit logits".into());
        }
        HfReference {
            producer: plain(&json!({ "transformers": v["transformers"], "dtype": "float32", "from": "audit directory" })),
            vocab,
            sequences,
        }
        .checked()
    }

    fn checked(self) -> Result<Self, String> {
        self.validate()?;
        Ok(self)
    }

    /// Producer-side acquisition/measurement limits, not a consensus model or context limit.
    pub fn validate(&self) -> Result<(), String> {
        if self.vocab == 0 || self.sequences.is_empty() || self.sequences.len() > 256 {
            return Err("hf-reference: empty or excessive vocabulary/sequences".into());
        }
        let mut positions = 0usize;
        let mut values = 0usize;
        for s in &self.sequences {
            positions = positions.checked_add(s.tokens.len()).filter(|n| *n <= 4096).ok_or("hf-reference: position limit")?;
            values = values.checked_add(s.logits.len()).filter(|n| *n <= (64 << 20)).ok_or("hf-reference: value limit")?;
            if s.tokens.is_empty()
                || s.tokens.iter().any(|t| *t >= self.vocab || *t > u32::MAX as usize)
                || s.logits.len() != extent(0, s.tokens.len(), self.vocab)?.end
                || s.logits.iter().any(|x| !x.is_finite())
            {
                return Err("hf-reference: invalid token, shape or non-finite logit".into());
            }
        }
        Ok(())
    }

    /// Write the sidecars into `dir`: `(json file name, f32 file name)`.
    pub fn write(&self, dir: &Path) -> Result<(String, String), String> {
        self.validate()?;
        let v = json!({
            "schema": HF_REFERENCE_SCHEMA_V1,
            "producer": self.producer,
            "vocab": self.vocab,
            "logits_file": HF_REFERENCE_LOGITS_FILE,
            "sequences": self.sequences.iter().map(|s| json!({ "tokens": s.tokens })).collect::<Vec<_>>(),
        });
        let bits: Vec<u8> = self.sequences.iter().flat_map(|s| s.logits.iter().flat_map(|x| x.to_le_bytes())).collect();
        std::fs::write(dir.join(HF_REFERENCE_FILE), serde_json::to_string_pretty(&v).unwrap_or_default() + "\n")
            .map_err(|e| e.to_string())?;
        std::fs::write(dir.join(HF_REFERENCE_LOGITS_FILE), bits).map_err(|e| e.to_string())?;
        Ok((HF_REFERENCE_FILE.into(), HF_REFERENCE_LOGITS_FILE.into()))
    }
}

struct Quiet;
impl StepSink for Quiet {
    fn node(&mut self, _: &NodeValue<'_>) {}
}

/// The program's logit rows for `tokens` on the typed backend: `value = code × scale`, one row per position.
pub fn program_logits(a: &LoadedArtifact, tokens: &[usize], scale: f64) -> Result<Vec<Vec<f64>>, String> {
    let plan = TirPlan::compile(&a.program).map_err(|e| format!("exec plan: {e}"))?;
    let mut xp = TirParams::new(&plan);
    for ((j, layer), b) in a.bytes_by_param() {
        let data = ParamData::from_le_bytes(a.program.params[*j as usize].dtype, b).map_err(|e| e.to_string())?;
        xp.insert(&plan, *j, *layer, data).map_err(|e| e.to_string())?;
    }
    let mut ex = TirExecutor::new(&plan, &xp).map_err(|e| e.to_string())?;
    let mut rows = Vec::with_capacity(tokens.len());
    for (pos, t) in tokens.iter().enumerate() {
        ex.step(*t as u32, &mut Quiet).map_err(|e| format!("exec at {pos}: {e}"))?;
        let (_, l) = ex.logits();
        rows.push(l.to_i128s().into_iter().map(|c| c as f64 * scale).collect());
    }
    Ok(rows)
}

fn softmax(v: &[f64], deterministic: bool) -> Vec<f64> {
    let m = v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let e: Vec<f64> = v.iter().map(|x| if deterministic { libm::exp(x - m) } else { (x - m).exp() }).collect();
    let z: f64 = e.iter().sum();
    e.into_iter().map(|x| x / z).collect()
}

/// The program's logit rows for `tokens` on the typed backend over a MAPPED artifact (a node's own [`TirArtifactV1`]): the weights are
/// the file's pages, not a copy — the streamed form of [`program_logits`] (RFC-0002 Part II §II.9 L2).
pub fn program_logits_streamed(
    a: &misaka_palw_tir_exec::node::TirArtifactV1,
    tokens: &[usize],
    scale: f64,
) -> Result<Vec<Vec<f64>>, String> {
    let mut ex = TirExecutor::new(a.plan(), a.params()).map_err(|e| e.to_string())?;
    let mut rows = Vec::with_capacity(tokens.len());
    for (pos, t) in tokens.iter().enumerate() {
        ex.step(*t as u32, &mut Quiet).map_err(|e| format!("exec at {pos}: {e}"))?;
        let (_, l) = ex.logits();
        rows.push(l.to_i128s().into_iter().map(|c| c as f64 * scale).collect());
    }
    Ok(rows)
}

/// **The fit of the program's logits to the reference's.**
pub fn measure(a: &LoadedArtifact, scale: f64, hf: &HfReference) -> Result<FitRec, String> {
    hf.validate()?;
    validate_scale(scale)?;
    measure_rows(&mut |tokens| program_logits(a, tokens, scale), hf, false)
}

/// [`measure`] over a mapped artifact: the same fit, the weights never copied.
pub fn measure_streamed(a: &misaka_palw_tir_exec::node::TirArtifactV1, scale: f64, hf: &HfReference) -> Result<FitRec, String> {
    hf.validate()?;
    validate_scale(scale)?;
    measure_rows(&mut |tokens| program_logits_streamed(a, tokens, scale), hf, false)
}

fn validate_scale(scale: f64) -> Result<(), String> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err("hf-reference: finite positive scale required".into());
    }
    Ok(())
}

/// Portable fit arithmetic for a versioned, reproducible third-party fidelity record.
pub fn measure_streamed_deterministic(
    a: &misaka_palw_tir_exec::node::TirArtifactV1,
    scale: f64,
    hf: &HfReference,
) -> Result<FitRec, String> {
    hf.validate()?;
    validate_scale(scale)?;
    measure_rows(&mut |tokens| program_logits_streamed(a, tokens, scale), hf, true)
}

type LogitRowsFn<'a> = dyn FnMut(&[usize]) -> Result<Vec<Vec<f64>>, String> + 'a;

fn measure_rows(rows_of: &mut LogitRowsFn<'_>, hf: &HfReference, deterministic: bool) -> Result<FitRec, String> {
    let (mut n, mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
    let (mut max_abs, mut sq, mut agree, mut positions, mut kl) = (0f64, 0f64, 0usize, 0usize, 0f64);
    for s in &hf.sequences {
        let rows = rows_of(&s.tokens)?;
        if rows.len() != s.tokens.len() || rows.iter().flatten().any(|x| !x.is_finite()) {
            return Err("hf-reference: non-finite or incorrectly sized program logits".into());
        }
        for (pos, row) in rows.iter().enumerate() {
            let want = &s.logits[pos * hf.vocab..(pos + 1) * hf.vocab];
            if row.len() != hf.vocab {
                return Err(format!("the program has {} logits, the reference {}", row.len(), hf.vocab));
            }
            let (mut ia, mut ib) = (0usize, 0usize);
            for i in 0..hf.vocab {
                let (x, y) = (row[i], want[i] as f64);
                n += 1.0;
                sx += x;
                sy += y;
                sxx += x * x;
                syy += y * y;
                sxy += x * y;
                max_abs = max_abs.max((x - y).abs());
                sq += (x - y) * (x - y);
                if x > row[ia] {
                    ia = i;
                }
                if y > want[ib] as f64 {
                    ib = i;
                }
            }
            positions += 1;
            agree += (ia == ib) as usize;
            let hf64: Vec<f64> = want.iter().map(|x| *x as f64).collect();
            let (p, q) = (softmax(&hf64, deterministic), softmax(row, deterministic));
            kl += p
                .iter()
                .zip(&q)
                .map(|(p, q)| {
                    if *p > 0.0 {
                        p * if deterministic { libm::log(p / q.max(1e-300)) } else { (p / q.max(1e-300)).ln() }
                    } else {
                        0.0
                    }
                })
                .sum::<f64>();
        }
    }
    if positions == 0 {
        return Err("the reference has no positions".into());
    }
    let vx = sxx - sx * sx / n;
    let slope = if vx > 0.0 { (sxy - sx * sy / n) / vx } else { f64::NAN };
    let intercept = (sy - slope * sx) / n;
    let vy = syy - sy * sy / n;
    let corr = if vx > 0.0 && vy > 0.0 { (sxy - sx * sy / n) / (vx * vy).sqrt() } else { f64::NAN };
    Ok(FitRec {
        slope,
        intercept,
        corr,
        max_abs,
        rmse: (sq / n).sqrt(),
        top1: agree as f64 / positions as f64,
        kl_mean: kl / positions as f64,
    })
}

/// Whether a measured fit is within a pack's tolerance; the failures, named.
pub fn check(fit: &FitRec, tol: &ToleranceRec) -> Vec<String> {
    let mut bad = Vec::new();
    // A fit that is NaN fails every bound.
    if fit.slope.is_nan() || fit.slope < tol.slope_min || fit.slope > tol.slope_max {
        bad.push(format!(
            "slope {:.4} outside [{}, {}] (the program's logits are not in the units of the reference)",
            fit.slope, tol.slope_min, tol.slope_max
        ));
    }
    if fit.corr.is_nan() || fit.corr < tol.corr_min {
        bad.push(format!("correlation {:.5} below {}", fit.corr, tol.corr_min));
    }
    if fit.top1.is_nan() || fit.top1 < tol.top1_min {
        bad.push(format!("top-1 agreement {:.3} below {}", fit.top1, tol.top1_min));
    }
    if fit.kl_mean.is_nan() || fit.kl_mean > tol.kl_max {
        bad.push(format!("mean KL {:.5} above {}", fit.kl_mean, tol.kl_max));
    }
    bad
}

/// A JSON value with its non-integer numbers written as strings (a producer record is informational,
/// and a pack's digest must not depend on how a reader parses a decimal).
pub fn plain(v: &Value) -> Value {
    match v {
        Value::Number(n) if !(n.is_i64() || n.is_u64()) => Value::String(n.to_string()),
        Value::Array(a) => Value::Array(a.iter().map(plain).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), plain(x))).collect()),
        other => other.clone(),
    }
}

/// The tolerance a build uses unless told otherwise.
pub fn default_tolerance() -> ToleranceRec {
    ToleranceRec { slope_min: 0.97, slope_max: 1.03, corr_min: 0.99, top1_min: 0.8, kl_max: 0.1 }
}
