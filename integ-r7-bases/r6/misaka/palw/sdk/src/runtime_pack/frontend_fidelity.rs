//! Predeclared empirical fidelity of an independent frontend, outside consensus. A reference
//! provider's label is informational; passing logits never certifies source identity or a task.
use super::hfref::{self, HfReference};
use super::manifest::{HfReferenceSection, LogitsSection, PackFile, blake2b256_hex, f64_bits};
use super::primitive::{FRONTEND_FILE, PACK_FILE, PrimitiveRuntimePackV1};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SCHEMA: &str = "misaka.palw.tir-frontend-fidelity.v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FidelityPolicy {
    pub schema: String,
    pub checkpoint_revision: String,
    pub task: String,
    pub context: u32,
    pub minimum_sequences: u32,
    pub minimum_positions: u32,
    /// This protocol uses portable fit arithmetic, independently of integer execution.
    pub fit_math: String,
    pub logits: LogitsSection,
    #[serde(with = "f64_bits")]
    pub max_abs: f64,
    #[serde(with = "f64_bits")]
    pub rmse_max: f64,
    /// Import-time weight saturation only; not a bound on recurrent runtime state saturation.
    pub max_import_saturated_values: u64,
}
impl FidelityPolicy {
    pub fn validate(&self, pack: &PrimitiveRuntimePackV1) -> Result<(), String> {
        let t = &self.logits.tolerance;
        if self.schema != SCHEMA
            || self.fit_math != "libm-v1"
            || self.context == 0
            || self.minimum_sequences == 0
            || self.minimum_positions == 0
            || self.checkpoint_revision.is_empty()
            || self.checkpoint_revision.len() > 1024
            || pack.revision.as_deref() != Some(self.checkpoint_revision.as_str())
            || self.task != pack.build.scope.task
        {
            return Err("FRONTEND_FIDELITY_POLICY: schema, revision, task or coverage".into());
        }
        let values = [self.logits.scale, t.slope_min, t.slope_max, t.corr_min, t.top1_min, t.kl_max, self.max_abs, self.rmse_max];
        if values.iter().any(|v| !v.is_finite())
            || self.logits.scale <= 0.0
            || t.slope_min <= 0.0
            || t.slope_max < t.slope_min
            || !(0.0..=1.0).contains(&t.corr_min)
            || !(0.0..=1.0).contains(&t.top1_min)
            || t.kl_max < 0.0
            || self.max_abs < 0.0
            || self.rmse_max < 0.0
            || !matches!(self.logits.convention.as_str(), "legacy-greedy-only" | "q24-natural-v1")
            || (self.logits.convention == "q24-natural-v1" && self.logits.scale.to_bits() != 2f64.powi(-24).to_bits())
        {
            return Err("FRONTEND_FIDELITY_POLICY: finite meaningful thresholds and logit units required".into());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        let v = serde_json::to_value(self).map_err(|e| e.to_string())?;
        Ok(blake2b256_hex(misaka_palw_tir_lower::adapter::canonical_json(&v).as_bytes()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FidelityRecord {
    pub policy: FidelityPolicy,
    pub policy_digest: String,
    pub reference: HfReferenceSection,
    pub files: Vec<PackFile>,
}
fn file(dir: &Path, name: &str) -> Result<PackFile, String> {
    let max = if name == hfref::HF_REFERENCE_FILE { 2 << 20 } else { 256 << 20 };
    let bytes = super::primitive::read(&dir.join(name), max)?;
    Ok(PackFile { path: name.into(), bytes: bytes.len() as u64, blake2b256: blake2b256_hex(&bytes) })
}
fn pins(dir: &Path) -> Result<Vec<PackFile>, String> {
    [hfref::HF_REFERENCE_FILE, hfref::HF_REFERENCE_LOGITS_FILE].iter().map(|name| file(dir, name)).collect()
}
impl FidelityRecord {
    /// Pinned data checks without execution; they do not produce a fidelity verdict.
    pub fn check_pins(&self, dir: &Path, pack: &PrimitiveRuntimePackV1) -> Result<(), String> {
        self.policy.validate(pack)?;
        if self.policy_digest != self.policy.digest()?
            || self.files != pins(dir)?
            || self.reference.file != hfref::HF_REFERENCE_FILE
            || self.reference.digest != self.files[0].blake2b256
        {
            return Err("FRONTEND_FIDELITY_MISMATCH: policy/reference pins differ".into());
        }
        Ok(())
    }
    /// An independent fit rerun is required before the caller can report the named logit gate.
    pub fn verify(&self, dir: &Path, pack: &PrimitiveRuntimePackV1, artifact: &Path) -> Result<(), String> {
        self.check_pins(dir, pack)?;
        let hf = HfReference::load(&dir.join(hfref::HF_REFERENCE_FILE))?;
        let fit = measure(pack, artifact, &self.policy, &hf)?;
        if serde_json::to_value(&fit).map_err(|e| e.to_string())?
            != serde_json::to_value(&self.reference.measured).map_err(|e| e.to_string())?
            || self.reference.producer != hf.producer
            || self.reference.vocab != hf.vocab
            || self.reference.sequences != hf.sequences.len()
            || self.reference.positions != hf.positions()
        {
            return Err("FRONTEND_FIDELITY_MISMATCH: fit or reference coverage not reproduced".into());
        }
        Ok(())
    }
}
fn measure(
    pack: &PrimitiveRuntimePackV1,
    artifact: &Path,
    policy: &FidelityPolicy,
    hf: &HfReference,
) -> Result<super::manifest::FitRec, String> {
    policy.validate(pack)?;
    hf.validate()?;
    let art = misaka_palw_tir_exec::node::TirArtifactV1::open(artifact)?;
    if policy.context > art.plan().program.history_bound
        || hf.vocab != art.plan().program.token_bound as usize
        || hf.sequences.len() < policy.minimum_sequences as usize
        || hf.positions() < policy.minimum_positions as usize
        || hf.sequences.iter().any(|s| s.tokens.len() > policy.context as usize)
        || pack.build.saturated_values > policy.max_import_saturated_values
    {
        return Err("FRONTEND_FIDELITY_COVERAGE: vocabulary, positions, context or import saturation".into());
    }
    let fit = hfref::measure_streamed_deterministic(&art, policy.logits.scale, hf)?;
    let mut failures = hfref::check(&fit, &policy.logits.tolerance);
    if !fit.max_abs.is_finite() || fit.max_abs > policy.max_abs {
        failures.push("max_abs".into());
    }
    if !fit.rmse.is_finite() || fit.rmse > policy.rmse_max {
        failures.push("rmse".into());
    }
    if [fit.slope, fit.intercept, fit.corr, fit.top1, fit.kl_mean].iter().any(|v| !v.is_finite()) {
        failures.push("non-finite fit".into());
    }
    if !failures.is_empty() {
        return Err(format!("FRONTEND_FIDELITY_FAILED: {}", failures.join("; ")));
    }
    Ok(fit)
}

/// Add a reference and a policy fixed BEFORE its measurement to a new companion directory.
/// The old recipe/artifact is preserved on success and failure; no out-of-tolerance escape exists.
pub fn attach(
    pack_dir: &Path,
    artifact: &Path,
    reference: &Path,
    policy_file: &Path,
    out: &Path,
) -> Result<PrimitiveRuntimePackV1, String> {
    if out.exists() {
        return Err("fidelity output already exists; use a new pack directory".into());
    }
    let mut pack = PrimitiveRuntimePackV1::read(pack_dir)?;
    pack.check_recipe(pack_dir)?;
    if pack.artifact != super::primitive::manifest(artifact)? {
        return Err("FRONTEND_ARTIFACT_MISMATCH: base artifact differs".into());
    }
    // Acquire and validate policy before loading any reference logits or executing a position.
    let policy: FidelityPolicy = serde_json::from_slice(&super::primitive::read(policy_file, 64 << 10)?).map_err(|e| e.to_string())?;
    policy.validate(&pack)?;
    let hf = HfReference::load(reference)?;
    let fit = measure(&pack, artifact, &policy, &hf)?;
    std::fs::create_dir(out).map_err(|e| e.to_string())?;
    hf.write(out)?;
    let files = pins(out)?;
    pack.fidelity = Some(FidelityRecord {
        policy_digest: policy.digest()?,
        policy,
        reference: HfReferenceSection {
            file: hfref::HF_REFERENCE_FILE.into(),
            digest: files[0].blake2b256.clone(),
            producer: hf.producer.clone(),
            sequences: hf.sequences.len(),
            positions: hf.positions(),
            vocab: hf.vocab,
            measured: fit,
        },
        files,
    });
    std::fs::copy(pack_dir.join(FRONTEND_FILE), out.join(FRONTEND_FILE)).map_err(|e| e.to_string())?;
    std::fs::write(out.join(PACK_FILE), serde_json::to_vec_pretty(&pack).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(pack)
}
