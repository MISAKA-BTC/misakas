//! **The streaming converter** (RFC-0002 Part II): a prepared model (spec → HL → TIR program), a
//! loader over its checkpoint, the calibration statistics and a quantisation policy in; a
//! `PALWTIR1` container out — produced one tensor, and for the big ones one block of rows, at a time
//! through a content-addressed chunk store ([`crate::lower::stream`]), and assembled in inventory
//! order from the chunks ([`misaka_palw_tir_artifact::chunks`]).
//!
//! **A function of its inputs, nothing else.** The container's tensors depend on the program, the
//! checkpoint's tensors, the calibration statistics and the policy — not on the machine, the thread
//! count, the memory budget, the block or chunk sizes (`tests/streaming_convert.rs` pins each of
//! them on every fixture). Anyone holding the public source and a runtime pack that names those
//! inputs rebuilds the same artifact, and so the same inventory root, class id and file digest:
//! that is what `palw-class pack build` and `pack verify` are.

use crate::error::{LowerError, Result};
use crate::float_ref::SiteStat;
use crate::float_ref::stream::OccParams;
use crate::fidelity::Prepared;
use crate::lower::{ChunkSink, StreamMaterialised, StreamOpts, StreamStats, materialise_stream};
use crate::detmath::MathMode;
use crate::quant::QuantPolicy;
use misaka_palw_tir_artifact::chunks::{ChunkStore, ChunkStoreStats, write_container_v1_chunked};
use std::collections::BTreeMap;
use std::path::Path;

/// What goes into the container besides the tensors.
pub struct ConvertOpts<'a> {
    /// How the work is cut (changes nothing in the result).
    pub stream: StreamOpts,
    /// `borsh(PalwTirLayoutV1)`, or empty (a lowerer's output before `palw-class declare-layout`).
    pub layout: Vec<u8>,
    /// The tokenizer id the class binds ([`crate::artifact::tokenizer_id_of`]), zero when none.
    pub tokenizer_id: [u8; 64],
    /// The container's provenance JSON, composed from what the materialisation found (the
    /// residual and logit scales); enters no identity.
    pub meta: &'a dyn Fn(&StreamMaterialised) -> serde_json::Value,
    /// Keep the chunk store after the container is written (it deduplicates later conversions: a
    /// composite's parent chunks, a rerun). Off: the store directory is removed.
    pub keep_chunks: bool,
    /// The math the conversion's tables and scales are computed with ([`crate::detmath`]):
    /// `LibmV1` is the same on every platform; `Std` only where the artifact was first built.
    pub math: MathMode,
}

/// What a conversion did.
#[derive(Clone, Debug)]
pub struct ConvertReport {
    /// The container's file digest (`misaka_palw_tir_artifact::file_digest_v1`), hex.
    pub file_digest: String,
    pub file_bytes: u64,
    pub resid_scale: f64,
    pub logits_scale: f64,
    pub quant_inexact: usize,
    pub stream: StreamStats,
    pub store: ChunkStoreStats,
    /// Chunks the artifact names, and how many of them are distinct (equal chunks are stored once).
    pub chunks: usize,
    pub distinct_chunks: usize,
    /// The most the chunk writer ever buffered.
    pub peak_chunk_buffer: usize,
    /// The math mode the conversion ran in.
    pub math: &'static str,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Convert `prep` to the container `out`, spooling chunks in `store_dir`.
#[allow(clippy::too_many_arguments)]
pub fn convert_to_container(
    prep: &Prepared,
    loader: &dyn OccParams,
    calib: &BTreeMap<String, SiteStat>,
    policy: &QuantPolicy,
    store_dir: &Path,
    out: &Path,
    opts: &ConvertOpts<'_>,
    progress: &dyn Fn(usize, usize),
) -> Result<ConvertReport> {
    // The mode must be in force before anything is computed from floats — `prep` was lowered already,
    // and a caller that lowered under another mode (the HL's RoPE frequencies are computed then) must
    // set the same one for both: the converter sets what it was told and the caller's `prepare` ran under it.
    crate::detmath::set_mode(opts.math);
    let store = ChunkStore::open(store_dir).map_err(|e| LowerError::Io(format!("{}: {e}", store_dir.display())))?;
    let mut sink = ChunkSink::new(&store);
    let m = materialise_stream(&prep.lowered, &prep.hl, loader, calib, policy, &opts.stream, &mut sink, progress)?;
    let meta = (opts.meta)(&m).to_string();
    let digest = write_container_v1_chunked(out, &prep.lowered.program, opts.layout.clone(), opts.tokenizer_id, meta, &store, &sink.artifact)
        .map_err(|e| LowerError::Io(e.to_string()))?;
    let report = ConvertReport {
        file_digest: hex(&digest),
        file_bytes: std::fs::metadata(out).map_err(|e| LowerError::Io(e.to_string()))?.len(),
        resid_scale: m.resid_scale,
        logits_scale: m.logits_scale,
        quant_inexact: m.quant_inexact,
        stream: m.stats,
        store: store.stats(),
        chunks: sink.artifact.chunk_count(),
        distinct_chunks: sink.artifact.distinct_chunks().len(),
        peak_chunk_buffer: sink.peak_buffer,
        math: opts.math.name(),
    };
    if !opts.keep_chunks {
        drop(sink);
        let _ = std::fs::remove_dir_all(store_dir);
    }
    Ok(report)
}

/// Calibration sequences and where they came from (a JSON record, e.g. `{"source": "random (seed 11)"}`).
#[derive(Clone, Debug)]
pub struct CalibInput {
    pub sequences: Vec<Vec<usize>>,
    pub source: serde_json::Value,
}

impl CalibInput {
    /// A token file: `{"source": …, "sequences": [[id, …], …]}`.
    pub fn from_file(path: &Path) -> Result<CalibInput> {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| LowerError::Io(format!("{}: {e}", path.display())))?)
            .map_err(|e| LowerError::bad(format!("{}: {e}", path.display())))?;
        let sequences = v["sequences"]
            .as_array()
            .ok_or_else(|| LowerError::bad(format!("{}: token file: no `sequences`", path.display())))?
            .iter()
            .map(|s| s.as_array().map(|a| a.iter().filter_map(|t| t.as_u64()).map(|t| t as usize).collect()).unwrap_or_default())
            .collect();
        Ok(CalibInput { sequences, source: v.get("source").cloned().unwrap_or(serde_json::Value::Null) })
    }
}

/// Everything `palw-tir-convert` takes, as data: what a runtime pack records and `pack build` runs.
#[derive(Clone, Debug)]
pub struct ConvertRequest {
    /// Checkpoint directory (config.json + safetensors) or a GGUF file.
    pub model: std::path::PathBuf,
    pub out: std::path::PathBuf,
    /// Calibration statistics to use as they are (the input a runtime pack pins by digest) …
    pub stats_in: Option<std::path::PathBuf>,
    /// … or sequences to calibrate on here.
    pub calib: Option<CalibInput>,
    /// Keep at most this many sequences / positions of `calib` (the legacy `--calib-seqs`, `--positions`).
    pub calib_seqs: Option<usize>,
    pub positions: Option<usize>,
    /// Write the calibration statistics here.
    pub stats_out: Option<std::path::PathBuf>,
    /// The longest context the artifact will be served at (default: the longest calibration sequence).
    pub context: Option<usize>,
    /// **The longest calibration sequence pinned statistics were measured on** (a rebuild from `stats_in`): a recurrent program's
    /// artifact records it as `calibrated_context`, as the run that measured the statistics did, so the rebuild is the same file.
    pub calibrated_context: Option<usize>,
    pub policy: QuantPolicy,
    pub max_window: Option<u32>,
    /// [`LowerOpts::gdn_core_wide`]: recorded in the artifact's meta (only when set) and pinned by the runtime pack.
    pub gdn_core_wide: bool,
    pub chunk_store: Option<std::path::PathBuf>,
    pub keep_chunks: bool,
    /// MiB of `f32` per block of rows, and the size from which a row-wise tensor is made by blocks.
    pub block_mib: usize,
    pub defer_min_mib: usize,
    /// **Row-streamed calibration** (RFC-0002 Part II §II.9 L1): a param of at least this many MiB of `f32` is bound LAZILY in the float
    /// reference — read when an op asks (a stack of experts by the rows of the experts a router selected) — so calibration holds the
    /// largest single tensor, not a layer. `0` (the default): off, as before. The statistics are the resident run's, bit for bit.
    pub calib_lazy_mib: usize,
    /// What the lazy params may keep resident between asks, in MiB (default 512).
    pub calib_lazy_cache_mib: usize,
    /// The tokenizer the class binds (default `<model>/tokenizer.json`, zero when absent).
    pub tokenizer: Option<std::path::PathBuf>,
    pub math: MathMode,
    /// Quant-format descriptor files (`--quant-format`).
    pub quant_formats: Vec<std::path::PathBuf>,
    /// A user-supplied adapter file (`misaka.palw.model-adapter.v1`) instead of the built-in choice.
    pub adapter: Option<std::path::PathBuf>,
}

impl ConvertRequest {
    pub fn new(model: impl Into<std::path::PathBuf>, out: impl Into<std::path::PathBuf>) -> Self {
        ConvertRequest {
            model: model.into(),
            out: out.into(),
            stats_in: None,
            calib: None,
            calib_seqs: None,
            positions: None,
            stats_out: None,
            context: None,
            calibrated_context: None,
            policy: QuantPolicy::default(),
            max_window: None,
            gdn_core_wide: false,
            chunk_store: None,
            keep_chunks: false,
            block_mib: 16,
            defer_min_mib: 16,
            calib_lazy_mib: 0,
            calib_lazy_cache_mib: 512,
            tokenizer: None,
            math: MathMode::LibmV1,
            quant_formats: Vec::new(),
            adapter: None,
        }
    }
}

/// What a conversion found and wrote, beyond the container's bytes.
#[derive(Clone, Debug)]
pub struct ConvertOutcome {
    pub report: ConvertReport,
    pub frontend: crate::fidelity::Frontend,
    pub features: Vec<crate::model::FeatureUse>,
    pub scope: crate::model::FeatureScope,
    /// `(name, digest)` of every quant-format descriptor the weights are read with.
    pub descriptors: Vec<(String, String)>,
    pub stats_digest: String,
    pub stats_sites: usize,
    pub calib_source: serde_json::Value,
    pub tokenizer_id: [u8; 64],
    /// The tokenizer file the id was taken from, when there is one.
    pub tokenizer_file: Option<std::path::PathBuf>,
    pub architecture: String,
    pub seconds: f64,
}

/// **Convert a model** — the whole of `palw-tir-convert` as a function: open it (the frontend, with the
/// quant formats and the adapter the request names), take or measure the calibration statistics, and
/// write the container. The container's provenance records what the pack needs to check it:
/// the frontend's adapter and spec digest, the feature scope, the descriptors, the statistics' digest,
/// the policy and the math.
pub fn convert_model(req: &ConvertRequest, log: &dyn Fn(String)) -> Result<ConvertOutcome> {
    use crate::calib::{stats_digest_hex, stats_from_json, stats_to_json};
    use crate::float_ref::stream::Streamed;
    use crate::lower::{LowerOpts, StreamOpts};
    use crate::quantfmt::QuantRegistry;
    let t0 = std::time::Instant::now();
    // Before the model is opened: lowering evaluates RoPE frequencies and the like, in this mode.
    crate::detmath::set_mode(req.math);
    let opts = LowerOpts { max_window: req.max_window, gdn_core_wide: req.gdn_core_wide, ..LowerOpts::default() };
    let reg = QuantRegistry::with_files(&req.quant_formats)?;
    let read = match &req.adapter {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| LowerError::Io(format!("{}: {e}", p.display())))?;
            crate::hf_schema::ReadOptions { adapter: crate::hf_schema::AdapterChoice::Text(text) }
        }
        None => crate::hf_schema::ReadOptions::default(),
    };
    let opened = crate::fidelity::open_model_full(&req.model, &opts, &reg, &read)?;
    let prep = &opened.prepared;
    let ck = opened.source.as_ref();
    let descriptors = crate::fidelity::quant_descriptors_used(&req.model, prep, &reg)?;
    log(format!(
        "{} — {}",
        prep.spec.architecture,
        crate::lower::program_summary(&prep.lowered.program).lines().next().unwrap_or("")
    ));
    log(format!("frontend: {} (level {}), spec {}", opened.frontend.adapter.describe(), opened.frontend.level, &opened.frontend.spec_digest[..16]));
    for (n, d) in &descriptors {
        log(format!("quant format {n} {d}"));
    }
    // The scope: what the model has that this class does not compute.
    let tensor_index = if opened.gguf {
        crate::hf_schema::TensorIndex::from_source(ck)
    } else {
        crate::hf_schema::TensorIndex::from_checkpoint_path(&req.model)?
    };
    let scope = crate::model::scope_of(&opened.frontend.config, Some(&prep.spec), Some(&tensor_index), &crate::model::sibling_files(&req.model));
    log(scope.headline());
    let features = prep.spec.features();

    let loader = Streamed::new(&prep.hl, &prep.binding, ck).with_lazy(req.calib_lazy_mib << 18, req.calib_lazy_cache_mib << 20);
    let progress = |what: &'static str| {
        move |d: usize, n: usize| {
            if d == n || d.is_multiple_of(8) {
                log(format!("  {what} {d}/{n}"));
            }
        }
    };
    // A recurrent program (a fixed-size state carried across positions) is calibrated on a sequence as long as the context it is
    // served at (freeze-v1 §5.2); its artifact records the longest one (`calibrated_context`) and the rule as applied
    // (`calibration_length_rule`), which `declare-layout` checks — the same record `palw-tir-fidelity` writes. A program with no
    // such state records neither, so its file is unchanged.
    let recurrent = prep.hl.states.iter().any(|s| matches!(s.kind, crate::hl::StateKind::Fixed));
    let mut calibrated: Option<(usize, usize)> = None;
    let (stats, calib_source): (BTreeMap<String, SiteStat>, serde_json::Value) = match (&req.stats_in, &req.calib) {
        (Some(p), None) => {
            let text = std::fs::read_to_string(p).map_err(|e| LowerError::Io(format!("{}: {e}", p.display())))?;
            let (stats, exact) = stats_from_json(&text)?;
            if !exact {
                log(format!("WARNING: {} is in the legacy (decimal float) format — not bit-exact, not for a runtime pack", p.display()));
            }
            log(format!("calibration statistics from {}", p.display()));
            if recurrent && let Some(longest) = req.calibrated_context {
                calibrated = Some((longest, req.context.unwrap_or(longest)));
            }
            (stats, serde_json::json!(p.display().to_string()))
        }
        // **Pinned statistics AND the sequences they were measured on** (a recurrent model's long calibration, kept as `stats.json` +
        // `calib-tokens.json`): the statistics are read, not re-measured, and the sequences only give the artifact the same
        // `calibrated_context` / `calibration_length_rule` a run that measured them would record — the combination `pack verify --rebuild`
        // already uses (pinned statistics, `calibrated_context` from the pack's sequences). The caller vouches that the statistics are the
        // sequences' (the pack records them as "statistics supplied"; their digest is the pinned identity).
        (Some(p), Some(c)) => {
            let text = std::fs::read_to_string(p).map_err(|e| LowerError::Io(format!("{}: {e}", p.display())))?;
            let (stats, exact) = stats_from_json(&text)?;
            if !exact {
                log(format!("WARNING: {} is in the legacy (decimal float) format — not bit-exact, not for a runtime pack", p.display()));
            }
            let mut seqs = c.sequences.clone();
            if let Some(n) = req.calib_seqs {
                seqs.truncate(n);
            }
            if let Some(n) = req.positions {
                seqs.iter_mut().for_each(|q| q.truncate(n));
            }
            seqs.retain(|q| !q.is_empty());
            let longest = seqs.iter().map(Vec::len).max();
            let context = req.context.or(longest).unwrap_or(0);
            if let Some(longest) = crate::fidelity::check_calibration_length(&prep.hl, &seqs, context)
                .map_err(|e| LowerError::bad(format!("{e}; pass a context to declare the context served")))?
            {
                calibrated = Some((longest, context));
            }
            log(format!(
                "calibration statistics from {} (pinned; the {} sequences / {} positions they were measured on give the calibrated context)",
                p.display(),
                seqs.len(),
                seqs.iter().map(Vec::len).sum::<usize>()
            ));
            (stats, serde_json::json!(p.display().to_string()))
        }
        (None, Some(c)) => {
            let mut seqs = c.sequences.clone();
            if let Some(n) = req.calib_seqs {
                seqs.truncate(n);
            }
            if let Some(n) = req.positions {
                seqs.iter_mut().for_each(|q| q.truncate(n));
            }
            seqs.retain(|q| !q.is_empty());
            if seqs.iter().flatten().any(|t| *t >= prep.hl.vocab) {
                return Err(LowerError::bad(format!("a calibration token ≥ vocab {}", prep.hl.vocab)));
            }
            let longest = seqs.iter().map(Vec::len).max();
            let context = req.context.or(longest).unwrap_or(0);
            if let Some(longest) = crate::fidelity::check_calibration_length(&prep.hl, &seqs, context)
                .map_err(|e| LowerError::bad(format!("{e}; pass a context to declare the context served")))?
            {
                calibrated = Some((longest, context));
            }
            log(format!("calibrating on {} sequences, {} positions", seqs.len(), seqs.iter().map(Vec::len).sum::<usize>()));
            let s = crate::fidelity::calibrate(&prep.hl, &loader, &seqs, &progress("calibration"))?;
            (s, c.source.clone())
        }
        _ => {
            return Err(LowerError::bad(
                "calibration statistics are an input of the conversion: give the statistics a pack pins (stats_in) or sequences to calibrate on",
            ));
        }
    };
    // The statistics' identity: the digest of their canonical serialisation (key order fixed), the same
    // whether they were just measured or read back, so the container's provenance — and with it the file
    // digest — is a function of the statistics and not of how they arrived.
    let stats_digest = stats_digest_hex(&stats);
    log(format!("calibration statistics {} sites, digest {stats_digest} (from {calib_source})", stats.len()));
    if let Some(p) = &req.stats_out {
        std::fs::write(p, stats_to_json(&stats)).map_err(|e| LowerError::Io(format!("{}: {e}", p.display())))?;
    }
    // The tokenizer the class binds: the one given, else the model directory's first tokenizer file
    // ([`crate::artifact::TOKENIZER_FILES_V1`]; `tokenizer.json` when it has one), zero when it has none.
    let tokenizer_path = req
        .tokenizer
        .clone()
        .or_else(|| crate::artifact::tokenizer_path_in(&req.model))
        .unwrap_or_else(|| req.model.join("tokenizer.json"));
    let (tokenizer_id, tokenizer_file) = match std::fs::read(&tokenizer_path) {
        Ok(bytes) => (crate::artifact::tokenizer_id_of(&bytes), Some(tokenizer_path)),
        Err(_) => ([0u8; 64], None),
    };
    let store_dir = req.chunk_store.clone().unwrap_or_else(|| {
        let mut s = req.out.clone().into_os_string();
        s.push(".chunks");
        std::path::PathBuf::from(s)
    });
    let policy = req.policy.clone();
    let math = req.math;
    let meta = |m: &StreamMaterialised| {
        let mut meta = serde_json::json!({
            "architecture": prep.spec.architecture,
            "resid_scale": m.resid_scale,
            "logits_scale": m.logits_scale,
            "policy": { "headroom16": policy.headroom16, "headroom32": policy.headroom32, "headroom_resid": policy.headroom_resid },
            "calibration": { "schema": "misaka.palw.calib-stats.v1", "digest": stats_digest },
            "max_window": req.max_window,
            "quant": { "descriptors": descriptors.iter().map(|(n, d)| serde_json::json!({ "name": n, "digest": d })).collect::<Vec<_>>() },
            "frontend": {
                "adapter": adapter_json(&opened.frontend.adapter),
                "level": opened.frontend.level.to_string(),
                "spec_digest": opened.frontend.spec_digest,
            },
            "scope": scope,
            "converter": format!("palw-tir-convert {}", env!("CARGO_PKG_VERSION")),
            // The platform is recorded only for `std`: libm-v1 does not depend on it.
            "math": if math == MathMode::Std { serde_json::json!({ "mode": "std", "platform": crate::detmath::platform() }) } else { serde_json::json!({ "mode": "libm-v1" }) },
        });
        if req.gdn_core_wide {
            // Only when set: an artifact built without the option keeps its meta byte for byte.
            meta["gdn_core_wide"] = serde_json::json!(true);
        }
        if let Some((longest, context)) = calibrated {
            meta["calibrated_context"] = serde_json::json!(longest);
            meta["calibration_length_rule"] = serde_json::json!({ "rule": "met", "longest": longest, "context": context });
        }
        meta
    };
    let copts = ConvertOpts {
        stream: StreamOpts { defer_min_elems: req.defer_min_mib << 18, block_elems: req.block_mib << 18 },
        layout: Vec::new(),
        tokenizer_id,
        meta: &meta,
        keep_chunks: req.keep_chunks,
        math,
    };
    log("converting".into());
    let report = convert_to_container(prep, &loader, &stats, &policy, &store_dir, &req.out, &copts, &progress("materialise"))?;
    log(format!(
        "{}: {} tensors, {:.1} MiB in the tensors, {:.1} MiB file; {} chunks ({} distinct); file digest {}",
        req.out.display(),
        report.stream.instances,
        report.stream.bytes as f64 / (1 << 20) as f64,
        report.file_bytes as f64 / (1 << 20) as f64,
        report.chunks,
        report.distinct_chunks,
        report.file_digest
    ));
    Ok(ConvertOutcome {
        report,
        frontend: opened.frontend.clone(),
        features,
        scope,
        descriptors,
        stats_digest,
        stats_sites: stats.len(),
        calib_source,
        tokenizer_id,
        tokenizer_file,
        architecture: prep.spec.architecture.clone(),
        seconds: t0.elapsed().as_secs_f64(),
    })
}

/// An adapter source as the provenance and the pack record it: `{kind, id, hash}` (`kind: none` for
/// Level A, the standard decoder template being the reading).
pub fn adapter_json(a: &crate::model::AdapterSource) -> serde_json::Value {
    use crate::model::AdapterSource as S;
    match a {
        S::None => serde_json::json!({ "kind": "none" }),
        S::BuiltIn { id, hash } => serde_json::json!({ "kind": "built-in", "id": id, "hash": hash }),
        S::UserFile { id, hash } => serde_json::json!({ "kind": "user-file", "id": id, "hash": hash }),
        // A reader written in Rust (the diffusers route's): named, with no data file to hash.
        S::CoreReader { id } => serde_json::json!({ "kind": "core-reader", "id": id }),
    }
}
