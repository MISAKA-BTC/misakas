//! Reproducible third-party frontend packs through the existing artifact, inventory and engines.
//! This versioned companion records integer build/conformance only. Source fidelity, full-task
//! support and live lifecycle gates stay UNVERIFIED until their independent evidence exists.
use super::{
    build::{sha256_file, source_files},
    conformance::{self, ConformanceJob, ImplSet},
    manifest::{ConformanceVector, ImplRec, SourceFile},
};
use crate::tir_manifest::PalwTirManifestV1;
use misaka_palw_tir_lower::{
    admission, artifact,
    frontend_pack::{BuildRecord, FrontendPack, distinct_output},
    weights::Checkpoint,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Read;
use std::path::Path;

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
            "source_equivalence":self.0.source_equivalence,"full_task":"UNVERIFIED","live_final":"UNVERIFIED"}))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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
        Ok(pack)
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn read(path: &Path, max: usize) -> Result<Vec<u8>, String> {
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
    if !model.is_dir() {
        return Err("FRONTEND_SOURCE_FORMAT: a safetensors directory is required".into());
    }
    let names = source_files(model)?;
    if !names.iter().any(|n| n.ends_with(".safetensors")) {
        return Err("FRONTEND_SOURCE_FORMAT: safetensors weights required".into());
    }
    let index = model.join("model.safetensors.index.json");
    if index.exists() {
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
            let (bytes, sha256) = sha256_file(&model.join(&path))?;
            Ok(SourceFile { path, bytes, sha256 })
        })
        .collect()
}
fn tokenizer(model: &Path) -> Result<[u8; 64], String> {
    match artifact::tokenizer_path_in(model) {
        Some(path) => Ok(artifact::tokenizer_id_of(&read(&path, 64 << 20)?)),
        None => Ok([0; 64]),
    }
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
fn manifest(artifact: &Path) -> Result<Value, String> {
    serde_json::from_str(&PalwTirManifestV1::derive_streamed(artifact)?.to_json()).map_err(|e| e.to_string())
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
    let mut protected = pins.iter().map(|f| model.join(&f.path)).collect::<Vec<_>>();
    protected.extend([frontend.to_path_buf(), pack_dir.join(FRONTEND_FILE), pack_dir.join(PACK_FILE)]);
    distinct_output(artifact_path, &protected).map_err(|e| e.to_string())?;
    let text = read(frontend, misaka_palw_tir_lower::frontend_pack::MAX_PACK_BYTES)?;
    let frontend = FrontendPack::parse(std::str::from_utf8(&text).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let config: Value = serde_json::from_slice(&read(&model.join("config.json"), 2 << 20)?).map_err(|e| e.to_string())?;
    let source = Checkpoint::open(model).map_err(|e| e.to_string())?;
    let compiled = frontend.compile_bounded(&config, &source, &admission::default_inputs(), block_bytes).map_err(|e| e.to_string())?;
    let conversion = compiled.write(artifact_path, &source, tokenizer(model)?, block_bytes).map_err(|e| e.to_string())?;
    let (vectors, _) = conformance::run_streamed(artifact_path, jobs, ImplSet::default(), &|_| {})?;
    if pins != files(model)? {
        return Err("FRONTEND_SOURCE_CHANGED: source files changed during build".into());
    }
    let pack = PrimitiveRuntimePackV1 {
        format: FORMAT.into(),
        revision,
        source_files: pins,
        build: conversion.record,
        artifact: manifest(artifact_path)?,
        implementations: ImplSet::default().records(),
        implementation_revisions: revisions(),
        admission_profile: admission_profile(),
        conformance: vectors,
        source_equivalence: crate::tir_registration::SOURCE_EQUIVALENCE_UNVERIFIED.into(),
    };
    std::fs::create_dir_all(pack_dir).map_err(|e| e.to_string())?;
    std::fs::write(pack_dir.join(FRONTEND_FILE), text).map_err(|e| e.to_string())?;
    std::fs::write(pack_dir.join(PACK_FILE), serde_json::to_vec_pretty(&pack).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
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
    let mut protected = pack.source_files.iter().map(|f| model.join(&f.path)).collect::<Vec<_>>();
    protected.extend([artifact_path.to_path_buf(), pack_dir.join(FRONTEND_FILE), pack_dir.join(PACK_FILE)]);
    distinct_output(rebuilt, &protected).map_err(|e| e.to_string())?;
    if pack.admission_profile != admission_profile() {
        return Err("FRONTEND_PROFILE_MISMATCH: compiler resource profile differs".into());
    }
    if pack.implementation_revisions != revisions() {
        return Err("FRONTEND_IMPLEMENTATION_MISMATCH: executor/checker sources differ".into());
    }
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
    let config: Value = serde_json::from_slice(&read(&model.join("config.json"), 2 << 20)?).map_err(|e| e.to_string())?;
    let source = Checkpoint::open(model).map_err(|e| e.to_string())?;
    let compiled = frontend.compile_bounded(&config, &source, &admission::default_inputs(), block_bytes).map_err(|e| e.to_string())?;
    compiled.write_checked(rebuilt, &source, tokenizer(model)?, block_bytes, Some(&pack.build)).map_err(|e| e.to_string())?;
    if manifest(rebuilt)? != pack.artifact {
        return Err("FRONTEND_REBUILD_MISMATCH: rebuilt inventory differs".into());
    }
    let (vectors, _) = conformance::run_streamed(rebuilt, &jobs, ImplSet::default(), &|_| {})?;
    if vectors != pack.conformance {
        return Err("FRONTEND_CONFORMANCE_MISMATCH: tokens, logits or commits differ".into());
    }
    if files(model)? != pack.source_files {
        return Err("FRONTEND_SOURCE_CHANGED: source files changed during verification".into());
    }
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
