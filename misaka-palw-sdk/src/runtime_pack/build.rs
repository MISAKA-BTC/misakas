//! **`palw-class pack build`**: convert a model and write the pack that lets anyone rebuild the artifact
//! and judge it.
//!
//! The steps, each recorded: hash the public source files (SHA-256, the hub's git-lfs object ids); open the
//! model through the frontend with the adapter and quant descriptors named; take or measure the
//! calibration statistics; convert (the streaming converter, `libm-v1` math by default); derive the
//! artifact's identity (file digest, inventory root, graph root); run the conformance vectors on every
//! executor; hold the program to the Hugging Face reference; and, per network asked, declare the class
//! (layout and class id). A pack whose program is outside its own tolerance is not written unless told to be.

use super::conformance::{self, ImplSet};
use super::hfref::{self, HfReference};
use super::manifest::*;
use crate::tir_layout::{TirLayoutChoiceV1, tir_declare_layout_v1};
use crate::tir_manifest::PalwTirManifestV1;
use misaka_palw_tir_lower::adapter::{self, canonical_json};
use misaka_palw_tir_lower::convert::{ConvertOutcome, ConvertRequest, convert_model};
use misaka_palw_tir_lower::model::registry_digest;
use misaka_palw_tir_lower::quantfmt::{QuantFormat, QuantRegistry};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A class to declare from the artifact on a network.
#[derive(Clone, Debug)]
pub struct DeclareOpts {
    pub network: String,
    pub choice: TirLayoutChoiceV1,
}

#[derive(Clone, Debug)]
pub struct BuildOpts {
    /// The conversion: the model, the artifact path (`request.out`), the calibration, the profile.
    pub request: ConvertRequest,
    /// The pack directory to write (created; must not hold a pack already).
    pub pack_dir: PathBuf,
    pub name: String,
    /// Informational provenance of the source.
    pub repo: Option<String>,
    pub revision: Option<String>,
    /// A Hugging Face reference: a sidecar, an audit directory, or a fixture's `logits.json`.
    pub hf_reference: Option<PathBuf>,
    pub tolerance: ToleranceRec,
    pub prompts: usize,
    pub prefill: usize,
    pub decode: usize,
    pub seed: u64,
    pub impls: ImplSet,
    pub declare: Vec<DeclareOpts>,
    /// Write the pack although the program is outside its tolerance.
    pub allow_out_of_tolerance: bool,
}

impl BuildOpts {
    pub fn new(request: ConvertRequest, pack_dir: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        BuildOpts {
            request,
            pack_dir: pack_dir.into(),
            name: name.into(),
            repo: None,
            revision: None,
            hf_reference: None,
            tolerance: hfref::default_tolerance(),
            prompts: 3,
            prefill: 8,
            decode: 4,
            seed: 0x7061636b,
            impls: ImplSet::default(),
            declare: Vec::new(),
            allow_out_of_tolerance: false,
        }
    }
}

pub struct BuiltPack {
    pub pack: RuntimePackV1,
    pub digest: String,
    pub dir: PathBuf,
}

/// SHA-256 of a file, hex, streamed.
pub fn sha256_file(path: &Path) -> Result<(u64, String), String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut n = 0u64;
    loop {
        let k = f.read(&mut buf).map_err(|e| format!("{}: {e}", path.display()))?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
        n += k as u64;
    }
    Ok((n, hex(&h.finalize())))
}

/// The files of a model's public source that decide its artifact, relative to its directory: the
/// configuration, the tokenizer, the weight files and their index (a GGUF: the file itself).
pub fn source_files(model: &Path) -> Result<Vec<String>, String> {
    if let Some(g) = misaka_palw_tir_lower::fidelity::gguf_path(model) {
        let name = g.file_name().and_then(|n| n.to_str()).ok_or("a GGUF path without a file name")?;
        return Ok(vec![name.to_string()]);
    }
    let mut v = Vec::new();
    for e in std::fs::read_dir(model).map_err(|e| format!("{}: {e}", model.display()))?.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        let keep = n == "config.json" || n == "tokenizer.json" || n == "model.safetensors.index.json" || n.ends_with(".safetensors");
        if keep && e.path().is_file() {
            v.push(n);
        }
    }
    v.sort();
    if !v.iter().any(|n| n == "config.json") {
        return Err(format!("{}: no config.json", model.display()));
    }
    Ok(v)
}

/// The directory a model's source files are relative to.
pub fn source_dir(model: &Path) -> PathBuf {
    match misaka_palw_tir_lower::fidelity::gguf_path(model) {
        Some(g) => g.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")),
        None => model.to_path_buf(),
    }
}

/// `(params, bundle)` of a network preset with a PALW V2 bundle.
pub fn network(raw: &str) -> Result<kaspa_consensus_core::config::params::Params, String> {
    let id: kaspa_consensus_core::network::NetworkId = raw.parse().map_err(|e| format!("--declare {raw}: {e}"))?;
    let params: kaspa_consensus_core::config::params::Params = id.into();
    match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_) => Ok(params),
        _ => Err(format!("{id} has no PALW V2 bundle, so no classes to declare")),
    }
}

fn pin_of(a: &misaka_palw_tir_lower::model::AdapterSource, user_file: Option<&str>) -> AdapterPin {
    use misaka_palw_tir_lower::model::AdapterSource as S;
    match a {
        S::None => AdapterPin { kind: "none".into(), id: None, hash: None, file: None },
        S::BuiltIn { id, hash } => AdapterPin { kind: "built-in".into(), id: Some(id.clone()), hash: Some(hash.clone()), file: None },
        S::UserFile { id, hash } => AdapterPin { kind: "user-file".into(), id: Some(id.clone()), hash: Some(hash.clone()), file: user_file.map(str::to_string) },
        // A reader written in Rust (the diffusers route's): pinned by name; a pack of such a class has no adapter file to hash.
        S::CoreReader { id } => AdapterPin { kind: "core-reader".into(), id: Some(id.clone()), hash: None, file: None },
    }
}

/// What the descriptors a model uses are, as the pack pins them: built-in, or a supplied file (copied
/// into the pack under `descriptors/`).
fn descriptor_pins(used: &[(String, String)], supplied: &[PathBuf], pack_dir: &Path) -> Result<Vec<DescriptorPin>, String> {
    let builtin = QuantRegistry::builtin();
    let mut by_digest: BTreeMap<String, (PathBuf, String)> = BTreeMap::new();
    for p in supplied {
        let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
        let f = QuantFormat::from_json(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        by_digest.insert(f.digest_hex(), (p.clone(), text));
    }
    let mut out = Vec::new();
    for (name, digest) in used {
        let is_builtin = builtin.named(name).is_some_and(|f| f.digest_hex() == *digest);
        if is_builtin {
            out.push(DescriptorPin { name: name.clone(), digest: digest.clone(), source: "built-in".into(), file: None });
            continue;
        }
        let (_, text) = by_digest.get(digest).ok_or_else(|| format!("the model is read with quant format {name} {digest}, which is neither built in nor a supplied file"))?;
        let file = format!("descriptors/{name}.json");
        std::fs::create_dir_all(pack_dir.join("descriptors")).map_err(|e| e.to_string())?;
        std::fs::write(pack_dir.join(&file), text).map_err(|e| e.to_string())?;
        out.push(DescriptorPin { name: name.clone(), digest: digest.clone(), source: "file".into(), file: Some(file) });
    }
    Ok(out)
}

fn pack_file(dir: &Path, rel: &str) -> Result<PackFile, String> {
    let b = std::fs::read(dir.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
    Ok(PackFile { path: rel.to_string(), bytes: b.len() as u64, blake2b256: blake2b256_hex(&b) })
}

/// **Build a pack.**
pub fn build(opts: &BuildOpts, log: &dyn Fn(String)) -> Result<BuiltPack, String> {
    let dir = &opts.pack_dir;
    if dir.join(PACK_FILE).exists() {
        return Err(format!("{} holds a pack already", dir.display()));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let model = &opts.request.model;

    // 1. The source, by hash.
    let sdir = source_dir(model);
    let mut files = Vec::new();
    for rel in source_files(model)? {
        log(format!("hashing {rel}"));
        let (bytes, sha) = sha256_file(&sdir.join(&rel))?;
        files.push(SourceFile { path: rel, bytes, sha256: sha });
    }

    // 2. The conversion; the statistics and the calibration sequences become sidecars.
    let mut request = opts.request.clone();
    let stats_path = dir.join("stats.json");
    request.stats_out = Some(stats_path.clone());
    let conv: ConvertOutcome = convert_model(&request, log).map_err(|e| e.to_string())?;
    let mut sequences_file = None;
    if let Some(c) = &request.calib {
        let mut seqs = c.sequences.clone();
        if let Some(n) = request.calib_seqs {
            seqs.truncate(n);
        }
        if let Some(n) = request.positions {
            seqs.iter_mut().for_each(|q| q.truncate(n));
        }
        seqs.retain(|q| !q.is_empty());
        std::fs::write(dir.join("calib-tokens.json"), serde_json::to_string(&serde_json::json!({ "source": c.source, "sequences": seqs })).unwrap_or_default()).map_err(|e| e.to_string())?;
        sequences_file = Some("calib-tokens.json".to_string());
    }

    // 3. The artifact's identity.
    let artifact = &request.out;
    let m = PalwTirManifestV1::derive_streamed(artifact)?;
    let meta: serde_json::Value = {
        let c = misaka_palw_tir_artifact::PalwTirContainerV1::open(artifact).map_err(|e| e.to_string())?;
        serde_json::from_str(&c.header.meta).unwrap_or(serde_json::Value::Null)
    };
    let logits_scale = conv.report.logits_scale;
    let convention = meta.get("logits_convention").and_then(|v| v.as_str()).unwrap_or("legacy-greedy-only").to_string();
    let result = ResultSection {
        artifact_digest: hex(&m.artifact_digest),
        artifact_bytes: m.artifact_bytes,
        graph_ir_root: m.graph_ir_root.to_string(),
        inventory_root: m.inventory_root.to_string(),
        leaf_count: m.leaf_count,
        tokenizer_id: hex(&m.tokenizer_id),
        program_bytes: m.program_bytes,
    };
    log(format!("artifact {} inventory root {}", &result.artifact_digest[..16], &result.inventory_root[..16]));

    // 4. Conformance on every executor.
    log("loading the artifact for conformance".into());
    let loaded = conformance::LoadedArtifact::open(artifact)?;
    let vocab = loaded.program.token_bound as usize;
    let jobs = conformance::jobs(vocab, opts.prompts, opts.prefill, opts.decode, opts.seed);
    let vectors = conformance::run(&loaded, &jobs, opts.impls, &|j| log(format!("conformance vector {} of {}", j + 1, jobs.len())))?;

    // 5. The Hugging Face reference.
    let mut hf_section = None;
    if let Some(path) = &opts.hf_reference {
        let hf = HfReference::load(path)?;
        let fit = hfref::measure(&loaded, logits_scale, &hf)?;
        log(format!("against the reference: slope {:.4}, corr {:.5}, top-1 {:.3}, KL {:.5}, max |Δ| {:.3}", fit.slope, fit.corr, fit.top1, fit.kl_mean, fit.max_abs));
        let bad = hfref::check(&fit, &opts.tolerance);
        if !bad.is_empty() && !opts.allow_out_of_tolerance {
            return Err(format!("the program is outside the tolerance of its reference: {}", bad.join("; ")));
        }
        let (json_name, _) = hf.write(dir)?;
        let digest = blake2b256_hex(&std::fs::read(dir.join(&json_name)).map_err(|e| e.to_string())?);
        hf_section = Some(HfReferenceSection { file: json_name, digest, producer: hf.producer.clone(), sequences: hf.sequences.len(), positions: hf.positions(), vocab: hf.vocab, measured: fit });
    }

    // 6. Declared classes.
    let mut declared = Vec::new();
    for d in &opts.declare {
        let params = network(&d.network)?;
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { unreachable!("checked by network()") };
        let mut out = artifact.clone().into_os_string();
        out.push(format!(".{}.palwtir", d.network));
        let out = PathBuf::from(out);
        log(format!("declaring the class on {}", d.network));
        let r = tir_declare_layout_v1(&params, bundle, artifact, &out, &d.choice, Some(&opts.name))?;
        r.admission.as_ref().map_err(|e| format!("{}: the class is not admitted: {e}", d.network))?;
        let dm = PalwTirManifestV1::derive_streamed(&out)?;
        declared.push(DeclaredClass {
            network: d.network.clone(),
            layout_digest: dm.layout_digest.map(|h| h.to_string()).ok_or("the declared container carries no layout")?,
            class_id: r.class_id.to_string(),
            max_context: r.layout.max_context,
            checkpoint_interval: r.layout.checkpoint_interval,
            h_tile: r.layout.h_tile,
            file_digest: hex(&dm.artifact_digest),
        });
    }

    // 7. The manifest.
    let adapter_file = match &conv.frontend.adapter {
        misaka_palw_tir_lower::model::AdapterSource::UserFile { .. } => {
            let text = std::fs::read_to_string(request.adapter.as_ref().ok_or("a user adapter without a file")?).map_err(|e| e.to_string())?;
            std::fs::write(dir.join("adapter.json"), text).map_err(|e| e.to_string())?;
            Some("adapter.json")
        }
        _ => None,
    };
    let template = matches!(conv.frontend.adapter, misaka_palw_tir_lower::model::AdapterSource::None)
        .then(|| adapter::builtin::by_id("standard-decoder"))
        .flatten()
        .map(|a| AdapterPin { kind: "built-in".into(), id: Some(a.id.clone()), hash: Some(a.hash.clone()), file: None });
    let cfg = &conv.frontend.config;
    let architectures: Vec<String> = cfg.get("architectures").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|s| s.as_str()).map(str::to_string).collect()).unwrap_or_default();
    let uses: Vec<serde_json::Value> = conv.features.iter().map(|f| serde_json::to_value(f).unwrap_or_default()).collect();
    let calibration = CalibrationRec {
        schema: "misaka.palw.calib-stats.v1".into(),
        stats_digest: conv.stats_digest.clone(),
        stats_file: "stats.json".into(),
        sites: conv.stats_sites,
        source: if request.stats_in.is_some() { serde_json::json!("statistics supplied") } else { conv.calib_source.clone() },
        sequences_file,
    };
    let mut sidecars: Vec<&str> = vec!["stats.json"];
    if let Some(f) = &calibration.sequences_file {
        sidecars.push(f);
    }
    if hf_section.is_some() {
        sidecars.push(hfref::HF_REFERENCE_FILE);
        sidecars.push(hfref::HF_REFERENCE_LOGITS_FILE);
    }
    if let Some(f) = adapter_file {
        sidecars.push(f);
    }
    let quant = QuantSection { descriptors: descriptor_pins(&conv.descriptors, &request.quant_formats, dir)? };
    let desc_files: Vec<String> = quant.descriptors.iter().filter_map(|d| d.file.clone()).collect();
    let mut pack_files = Vec::new();
    for s in sidecars.iter().map(|s| s.to_string()).chain(desc_files) {
        pack_files.push(pack_file(dir, &s)?);
    }
    pack_files.sort_by(|a, b| a.path.cmp(&b.path));
    let math = if conv.report.math == "std" {
        MathRec { mode: "std".into(), platform: Some(misaka_palw_tir_lower::detmath::platform()) }
    } else {
        MathRec { mode: "libm-v1".into(), platform: None }
    };
    let pack = RuntimePackV1 {
        schema: PACK_SCHEMA_V1.into(),
        name: opts.name.clone(),
        model: ModelSection {
            format: if misaka_palw_tir_lower::fidelity::gguf_path(model).is_some() { "gguf".into() } else { "safetensors".into() },
            label: opts.repo.clone().unwrap_or_else(|| model.file_name().and_then(|n| n.to_str()).unwrap_or("model").to_string()),
            revision: opts.revision.clone(),
            model_type: cfg.get("model_type").and_then(|v| v.as_str()).map(str::to_string),
            architectures,
            config_digest: blake2b256_hex(canonical_json(cfg).as_bytes()),
            files,
        },
        frontend: FrontendSection {
            adapter: pin_of(&conv.frontend.adapter, adapter_file),
            template,
            builtin_pack_hash: adapter::builtin::pack_hash(),
            spec_digest: conv.frontend.spec_digest.clone(),
            level: conv.frontend.level.to_string(),
            assumed_defaults: conv.frontend.assumed_defaults.clone(),
        },
        features: FeaturesSection { registry_digest: registry_digest(), used: uses, scope: serde_json::to_value(&conv.scope).unwrap_or_default() },
        quant,
        profile: ProfileSection {
            policy: PolicyRec { headroom16: request.policy.headroom16, headroom32: request.policy.headroom32, headroom_resid: request.policy.headroom_resid },
            max_window: request.max_window,
            context: request.context,
            calibration,
        },
        converter: ConverterSection { name: "palw-tir-convert".into(), crate_version: env!("CARGO_PKG_VERSION").into(), lowering: LOWERING_VERSION_V1.into(), math },
        executor: ExecutorSection { program_format: "PALWTIR1".into(), prim_set_id: hex(&misaka_palw_tir::prim::PRIM_SET_ID_V1), implementations: opts.impls.records() },
        logits: LogitsSection { convention, scale: logits_scale, tolerance: opts.tolerance.clone() },
        result,
        conformance: ConformanceSection { vectors },
        hf_reference: hf_section,
        declared,
        files: pack_files,
    };
    pack.check_shape()?;
    std::fs::write(dir.join(PACK_FILE), pack.to_pretty()).map_err(|e| e.to_string())?;
    let digest = pack.digest();
    log(format!("pack {} written to {}", digest, dir.display()));
    Ok(BuiltPack { pack, digest, dir: dir.clone() })
}
