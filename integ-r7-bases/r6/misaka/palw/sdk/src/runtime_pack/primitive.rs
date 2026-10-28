//! Reproducible third-party frontend packs through the existing artifact, inventory and engines.
//! This versioned companion records integer build/conformance only. Source fidelity, full-task
//! support and live lifecycle gates stay UNVERIFIED until their independent evidence exists.
use super::{
    build::{sha256_file, source_files},
    conformance::{self, ConformanceJob, ImplSet},
    manifest::{ConformanceVector, DeclaredClass, ImplRec, SourceFile},
};
use crate::tir_manifest::PalwTirManifestV1;
use misaka_palw_tir_lower::{
    admission, artifact,
    frontend_pack::{
        BuildRecord, FrontendPack, FrontendSource, checkpoint_inputs, checkpoint_path, distinct_output, is_gguf_checkpoint, source_dir,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const FORMAT: &str = "misaka.palw.runtime-pack.tir-frontend.v1";
pub const PACK_FILE: &str = "frontend-runtime-pack.json";
pub const FRONTEND_FILE: &str = "frontend.json";

/// Constructed only after the named source/rebuild/inventory/conformance checks succeed.
#[derive(Debug)]
pub struct VerifiedFrontendPack(PrimitiveRuntimePackV1);
impl VerifiedFrontendPack {
    pub fn pack(&self) -> &PrimitiveRuntimePackV1 {
        &self.0
    }
    pub fn report(&self) -> Result<Value, String> {
        Ok(json!({"pack_digest":self.0.digest()?,"rebuild":"PASS","integer_conformance":"PASS",
            "source_equivalence":self.0.source_equivalence,
            "reference_logits":if self.0.fidelity.is_some() { "WITHIN_PREDECLARED_TOLERANCE" } else { "UNVERIFIED" },
            "routing_fidelity":"UNVERIFIED","runtime_state_saturation":"UNVERIFIED","task_quality":"UNVERIFIED",
            "full_task":"UNVERIFIED","live_final":"UNVERIFIED"}))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PrimitiveRuntimePackV1 {
    pub format: String,
    pub revision: Option<String>,
    pub source_files: Vec<SourceFile>,
    pub build: BuildRecord,
    /// Common artifact/inventory manifest, derived by the streamed SDK path.
    pub artifact: Value,
    pub implementations: Vec<ImplRec>,
    pub implementation_revisions: Vec<ImplementationRevision>,
    pub admission_profile: Value,
    pub conformance: Vec<ConformanceVector>,
    /// This schema has no HF equivalence certificate; it cannot assert one by a user boolean.
    pub source_equivalence: String,
    /// Exact layouts bound from existing class artifacts, without ModelSpec or admission search.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub declared: Vec<DeclaredClass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fidelity: Option<super::frontend_fidelity::FidelityRecord>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ImplementationRevision {
    pub role: String,
    pub crate_name: String,
    pub source_digest: String,
}
fn revisions() -> Vec<ImplementationRevision> {
    super::commit::IMPL_REVISIONS
        .iter()
        .map(|(role, name, digest)| ImplementationRevision {
            role: (*role).into(),
            crate_name: (*name).into(),
            source_digest: (*digest).into(),
        })
        .collect()
}
fn admission_profile() -> Value {
    let i = admission::default_inputs();
    let c = i.ceilings;
    json!({"tile_len":i.tile_len,"h_chunk":i.h_chunk,"ceilings":{
        "max_tile_macs":c.max_tile_macs,"max_tile_transcendentals":c.max_tile_transcendentals,"max_tile_opened_bytes":c.max_tile_opened_bytes,
        "max_tile_operands":c.max_tile_operands,"max_position_macs":c.max_position_macs,"max_position_transcendentals":c.max_position_transcendentals,
        "max_state_bytes":c.max_state_bytes,"max_step_leaves":c.max_step_leaves,"max_checkpoint_interval":c.max_checkpoint_interval,"max_cone_work":c.max_cone_work}})
}
impl PrimitiveRuntimePackV1 {
    pub fn digest(&self) -> Result<String, String> {
        let v = serde_json::to_value(self).map_err(|e| e.to_string())?;
        Ok(hex(blake2b_simd::Params::new()
            .hash_length(32)
            .key(b"misaka.palw.runtime-pack.tir.v1")
            .hash(misaka_palw_tir_lower::adapter::canonical_json(&v).as_bytes())
            .as_bytes()))
    }
    pub fn read(dir: &Path) -> Result<Self, String> {
        let pack: Self = serde_json::from_slice(&read(&dir.join(PACK_FILE), 64 << 20)?).map_err(|e| e.to_string())?;
        if pack.format != FORMAT || pack.source_equivalence != crate::tir_registration::SOURCE_EQUIVALENCE_UNVERIFIED {
            return Err("FRONTEND_PACK_SCHEMA: format or unsupported fidelity assertion".into());
        }
        if pack.source_files.len() > 65_536 || pack.declared.len() > 256 {
            return Err("FRONTEND_PACK_SCHEMA: source/class count".into());
        }
        let mut last = None;
        for f in &pack.source_files {
            super::manifest::safe_name(&f.path)?;
            if f.path.len() > 1024
                || last.is_some_and(|name: &str| name >= f.path.as_str())
                || f.sha256.len() != 64
                || !f.sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err("FRONTEND_PACK_SCHEMA: sorted distinct source paths and SHA-256 required".into());
            }
            last = Some(f.path.as_str());
        }
        Ok(pack)
    }

    /// Check the pinned recipe/tool inputs without fetching a checkpoint. This is not a rebuild
    /// or a fidelity verdict; callers must name the checks they actually performed.
    pub fn check_recipe(&self, dir: &Path) -> Result<(), String> {
        if self.admission_profile != admission_profile() {
            return Err("FRONTEND_PROFILE_MISMATCH: compiler resource profile differs".into());
        }
        if self.implementation_revisions != revisions() || self.implementations != ImplSet::default().records() {
            return Err("FRONTEND_IMPLEMENTATION_MISMATCH: executor/checker sources differ".into());
        }
        if self.build.compiler_source_digest != misaka_palw_tir_lower::frontend_pack::compiler_digest() {
            return Err("FRONTEND_COMPILER_MISMATCH: compiler sources differ".into());
        }
        let frontend = FrontendPack::read(&dir.join(FRONTEND_FILE)).map_err(|e| e.to_string())?;
        if frontend.hash() != self.build.pack_hash {
            return Err("FRONTEND_BUILD_MISMATCH: frontend digest differs".into());
        }
        let jobs: Vec<_> = self
            .conformance
            .iter()
            .map(|v| ConformanceJob { label: v.label.clone(), prompt: v.prompt.clone(), decode: v.decode })
            .collect();
        jobs_checked(&jobs)?;
        if let Some(record) = &self.fidelity {
            record.check_pins(dir, self)?;
        }
        Ok(())
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) fn read(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .take((max + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err("FRONTEND_EXPANSION_LIMIT: sidecar bytes".into());
    }
    Ok(bytes)
}
fn files(model: &Path) -> Result<Vec<SourceFile>, String> {
    let selected = checkpoint_path(model).map_err(|e| e.to_string())?;
    let dir = source_dir(model);
    let gguf = is_gguf_checkpoint(&selected);
    if !gguf && !model.is_dir() {
        return Err("FRONTEND_SOURCE_FORMAT: a safetensors directory is required".into());
    }
    let mut names = if gguf {
        let mut names = checkpoint_inputs(&selected)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
                    .ok_or("FRONTEND_SOURCE_FORMAT: UTF-8 file name required".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut entries = 0;
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            entries += 1;
            if entries > misaka_palw_tir_lower::frontend_pack::MAX_SOURCE_TENSORS {
                return Err("FRONTEND_SOURCE_LIMIT: directory inventory".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().to_string();
            if (name == "config.json" || artifact::is_tokenizer_file(&name)) && entry.path().is_file() {
                names.push(name);
            }
        }
        names
    } else {
        source_files(model)?
    };
    names.sort();
    if !gguf && !names.iter().any(|n| n.ends_with(".safetensors")) {
        return Err("FRONTEND_SOURCE_FORMAT: safetensors weights required".into());
    }
    let index = dir.join("model.safetensors.index.json");
    if !gguf && index.exists() {
        let v: Value = serde_json::from_slice(&read(&index, 2 << 20)?).map_err(|e| e.to_string())?;
        let map = v.get("weight_map").and_then(Value::as_object).ok_or("FRONTEND_SOURCE_FORMAT: weight_map required")?;
        for shard in map.values() {
            let name = shard.as_str().ok_or("FRONTEND_SOURCE_FORMAT: shard name required")?;
            if !names.iter().any(|n| n == name) || !name.ends_with(".safetensors") {
                return Err("FRONTEND_SOURCE_FORMAT: every shard must be a pinned local safetensors file".into());
            }
        }
    }
    names
        .into_iter()
        .map(|path| {
            let (bytes, sha256) = sha256_file(&dir.join(&path))?;
            Ok(SourceFile { path, bytes, sha256 })
        })
        .collect()
}
fn tokenizer(model: &Path, source: &FrontendSource) -> Result<[u8; 64], String> {
    match artifact::tokenizer_path_in(source_dir(model)) {
        Some(path) => Ok(artifact::tokenizer_id_of(&read(&path, 64 << 20)?)),
        None => Ok(source.embedded_tokenizer_id().map_err(|e| e.to_string())?.unwrap_or([0; 64])),
    }
}
fn config(model: &Path, source: &FrontendSource) -> Result<Value, String> {
    let path = source_dir(model).join("config.json");
    let sidecar = if path.exists() || !source.is_gguf() {
        serde_json::from_slice(&read(&path, 2 << 20)?).map_err(|e| e.to_string())?
    } else {
        json!({})
    };
    source.configuration(sidecar).map_err(|e| e.to_string())
}
fn jobs_checked(jobs: &[ConformanceJob]) -> Result<(), String> {
    let mut total = 0usize;
    if jobs.is_empty() || jobs.len() > 256 {
        return Err("FRONTEND_CONFORMANCE: 1..=256 jobs required".into());
    }
    for job in jobs {
        if job.prompt.is_empty() || job.label.is_empty() || job.label.len() > 128 {
            return Err("FRONTEND_CONFORMANCE: prompt and label required".into());
        }
        total = total
            .checked_add(job.prompt.len())
            .and_then(|n| n.checked_add(job.decode))
            .filter(|n| *n <= 4096)
            .ok_or("FRONTEND_CONFORMANCE_LIMIT: at most 4096 positions in a receipt")?;
    }
    Ok(())
}
pub(crate) fn manifest(artifact: &Path) -> Result<Value, String> {
    serde_json::from_str(&PalwTirManifestV1::derive_streamed(artifact)?.to_json()).map_err(|e| e.to_string())
}

static NEXT_ARTIFACT: AtomicU64 = AtomicU64::new(0);
struct PendingArtifact(PathBuf);
impl PendingArtifact {
    fn new(output: &Path) -> Result<Self, String> {
        let parent = output.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let path = parent.join(format!(".frontend-pack-{}-{}.tmp", std::process::id(), NEXT_ARTIFACT.fetch_add(1, Ordering::Relaxed)));
        std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
    fn publish(&self, output: &Path) -> Result<(), String> {
        std::fs::rename(&self.0, output).map_err(|e| e.to_string())
    }
}
impl Drop for PendingArtifact {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Build a published recipe without any model-name or ModelSpec selection. All three existing
/// implementations must agree; there is no --no-ref2/--no-exec escape in this route.
pub fn build(
    model: &Path,
    frontend: &Path,
    artifact_path: &Path,
    pack_dir: &Path,
    jobs: &[ConformanceJob],
    revision: Option<String>,
    block_bytes: usize,
) -> Result<PrimitiveRuntimePackV1, String> {
    jobs_checked(jobs)?;
    let pins = files(model)?;
    let mut protected = pins.iter().map(|f| source_dir(model).join(&f.path)).collect::<Vec<_>>();
    protected.push(frontend.to_path_buf());
    std::fs::create_dir_all(pack_dir).map_err(|e| e.to_string())?;
    distinct_output(&pack_dir.join(FRONTEND_FILE), &protected).map_err(|e| e.to_string())?;
    protected.push(pack_dir.join(FRONTEND_FILE));
    distinct_output(&pack_dir.join(PACK_FILE), &protected).map_err(|e| e.to_string())?;
    protected.push(pack_dir.join(PACK_FILE));
    distinct_output(artifact_path, &protected).map_err(|e| e.to_string())?;
    let text = read(frontend, misaka_palw_tir_lower::frontend_pack::MAX_PACK_BYTES)?;
    let frontend = FrontendPack::parse(std::str::from_utf8(&text).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let source = FrontendSource::open(model, &frontend).map_err(|e| e.to_string())?;
    let config = config(model, &source)?;
    let compiled = frontend.compile_bounded(&config, &source, &admission::default_inputs(), block_bytes).map_err(|e| e.to_string())?;
    let pending = PendingArtifact::new(artifact_path)?;
    let conversion = compiled.write(&pending.0, &source, tokenizer(model, &source)?, block_bytes).map_err(|e| e.to_string())?;
    let (vectors, _) = conformance::run_streamed(&pending.0, jobs, ImplSet::default(), &|_| {})?;
    if pins != files(model)? {
        return Err("FRONTEND_SOURCE_CHANGED: source files changed during build".into());
    }
    let pack = PrimitiveRuntimePackV1 {
        format: FORMAT.into(),
        revision,
        source_files: pins,
        build: conversion.record,
        artifact: manifest(&pending.0)?,
        implementations: ImplSet::default().records(),
        implementation_revisions: revisions(),
        admission_profile: admission_profile(),
        conformance: vectors,
        source_equivalence: crate::tir_registration::SOURCE_EQUIVALENCE_UNVERIFIED.into(),
        declared: Vec::new(),
        fidelity: None,
    };
    std::fs::write(pack_dir.join(FRONTEND_FILE), text).map_err(|e| e.to_string())?;
    std::fs::write(pack_dir.join(PACK_FILE), serde_json::to_vec_pretty(&pack).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    pending.publish(artifact_path)?;
    Ok(pack)
}

/// A source-hash check, independent rebuild, streamed inventory derivation and all three engines.
/// A success proves those named checks only, never the quality or live Final of a checkpoint.
pub fn verify(
    pack_dir: &Path,
    model: &Path,
    artifact_path: &Path,
    rebuilt: &Path,
    block_bytes: usize,
) -> Result<VerifiedFrontendPack, String> {
    let pack = PrimitiveRuntimePackV1::read(pack_dir)?;
    let mut protected = pack.source_files.iter().map(|f| source_dir(model).join(&f.path)).collect::<Vec<_>>();
    protected.extend([artifact_path.to_path_buf(), pack_dir.join(FRONTEND_FILE), pack_dir.join(PACK_FILE)]);
    if pack.fidelity.is_some() {
        protected.extend([pack_dir.join(super::hfref::HF_REFERENCE_FILE), pack_dir.join(super::hfref::HF_REFERENCE_LOGITS_FILE)]);
    }
    distinct_output(rebuilt, &protected).map_err(|e| e.to_string())?;
    pack.check_recipe(pack_dir)?;
    let jobs: Vec<_> = pack
        .conformance
        .iter()
        .map(|v| ConformanceJob { label: v.label.clone(), prompt: v.prompt.clone(), decode: v.decode })
        .collect();
    jobs_checked(&jobs)?;
    if files(model)? != pack.source_files {
        return Err("FRONTEND_SOURCE_MISMATCH: public source SHAs differ".into());
    }
    if manifest(artifact_path)? != pack.artifact {
        return Err("FRONTEND_ARTIFACT_MISMATCH: artifact or inventory differs".into());
    }
    if ImplSet::default().records() != pack.implementations {
        return Err("FRONTEND_IMPLEMENTATION_MISMATCH: executor versions differ".into());
    }
    let frontend = FrontendPack::read(&pack_dir.join(FRONTEND_FILE)).map_err(|e| e.to_string())?;
    let source = FrontendSource::open(model, &frontend).map_err(|e| e.to_string())?;
    let config = config(model, &source)?;
    let compiled = frontend.compile_bounded(&config, &source, &admission::default_inputs(), block_bytes).map_err(|e| e.to_string())?;
    let pending = PendingArtifact::new(rebuilt)?;
    compiled
        .write_checked(&pending.0, &source, tokenizer(model, &source)?, block_bytes, Some(&pack.build))
        .map_err(|e| e.to_string())?;
    if manifest(&pending.0)? != pack.artifact {
        return Err("FRONTEND_REBUILD_MISMATCH: rebuilt inventory differs".into());
    }
    let (vectors, _) = conformance::run_streamed(&pending.0, &jobs, ImplSet::default(), &|_| {})?;
    if vectors != pack.conformance {
        return Err("FRONTEND_CONFORMANCE_MISMATCH: tokens, logits or commits differ".into());
    }
    if let Some(record) = &pack.fidelity {
        record.verify(pack_dir, &pack, &pending.0)?;
    }
    if files(model)? != pack.source_files {
        return Err("FRONTEND_SOURCE_CHANGED: source files changed during verification".into());
    }
    pending.publish(rebuilt)?;
    Ok(VerifiedFrontendPack(pack))
}

/// JSON token file for the generic CLI; users specify the exact jobs, rather than a family preset.
pub fn read_jobs(path: &Path, decode: usize) -> Result<Vec<ConformanceJob>, String> {
    let v: Value = serde_json::from_slice(&read(path, 2 << 20)?).map_err(|e| e.to_string())?;
    let sequences: Vec<Vec<usize>> =
        serde_json::from_value(v.get("sequences").cloned().ok_or("FRONTEND_CONFORMANCE: sequences required")?)
            .map_err(|e| e.to_string())?;
    let jobs: Vec<_> =
        sequences.into_iter().enumerate().map(|(i, prompt)| ConformanceJob { label: format!("public-{i}"), prompt, decode }).collect();
    jobs_checked(&jobs)?;
    Ok(jobs)
}
