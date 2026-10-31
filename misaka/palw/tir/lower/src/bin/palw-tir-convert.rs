//! `palw-tir-convert` — the streaming converter: a Hugging Face checkpoint (or a GGUF file) lowered
//! to PALW-TIR and its integer artifact written as a `PALWTIR1` container, one tensor — and for the
//! big ones one block of rows — at a time, through a content-addressed chunk store. Resident memory
//! is a block and a chunk, not the checkpoint (`tests/streaming_budget.rs`).
//!
//! The output is a function of its inputs: the checkpoint, the calibration statistics (`--stats-in`,
//! the file a runtime pack pins by hash; or `--calib` to calibrate here and `--stats-out` to
//! publish them) and the policy. The machine, the thread count and every size option change nothing
//! (`tests/streaming_convert.rs`), so anyone with the public source and the pack gets the same
//! file, and with it the same inventory root and class id.
//!
//! Token files are JSON `{"source": …, "sequences": [[id, …], …]}`.

#![allow(warnings)]
use clap::Parser;
use misaka_palw_tir_lower::convert::{CalibInput, ConvertRequest, convert_model};
use misaka_palw_tir_lower::detmath::MathMode;
use misaka_palw_tir_lower::quant::QuantPolicy;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "palw-tir-convert", about = "Streaming conversion of a checkpoint to a PALWTIR1 artifact")]
struct Args {
    /// Checkpoint directory (config.json + safetensors) or a GGUF file.
    model: PathBuf,
    /// The container to write.
    #[arg(long)]
    out: PathBuf,
    /// Calibration statistics written by `--stats-out` (`misaka.palw.calib-stats.v1`, bit-exact: the
    /// input a runtime pack pins by digest). With `--calib` as well, the statistics are read, not re-measured, and the
    /// sequences they were measured on give the calibrated context and the calibration-length rule (statistics measured
    /// on another machine: `palw-tir-fidelity --calibrate-only --stats-out`).
    #[arg(long)]
    stats_in: Option<PathBuf>,
    /// Calibrate on these sequences (JSON token file) instead.
    #[arg(long)]
    calib: Option<PathBuf>,
    #[arg(long)]
    calib_seqs: Option<usize>,
    #[arg(long)]
    positions: Option<usize>,
    /// Write the calibration statistics (JSON).
    #[arg(long)]
    stats_out: Option<PathBuf>,
    /// The longest context the artifact will be served at (the calibration-length rule of a recurrent
    /// program is checked against it); default: the longest calibration sequence.
    #[arg(long)]
    context: Option<usize>,
    #[arg(long, default_value_t = 2.0)]
    headroom16: f64,
    #[arg(long, default_value_t = 4.0)]
    headroom32: f64,
    #[arg(long, default_value_t = 4.0)]
    headroom_resid: f64,
    /// Keep at most this many positions of history in any block (`LowerOpts::max_window`).
    #[arg(long)]
    max_window: Option<u32>,
    /// Explicit decoder float-source linear main-weight profile, 8 (legacy) or 16.
    #[arg(long, default_value_t = 8)]
    linear_weight_bits: u8,
    /// Deliver every Gated DeltaNet core on the wide (32-bit) grid (`LowerOpts::gdn_core_wide`). The node graph is the
    /// same program; the constants and tensors differ, so the artifact is a different class. This is the fix of the
    /// Huihui-Qwen3.5-9B fit refusal (docs/evidence/hf-onboarding/20261010-huihui-qwen35-9b-fit-diagnosis, F7–F9:
    /// KL 0.268 → 0.0039 on the full 32-layer model); every genesis GDN-hybrid conversion sets it. Off: byte-identical
    /// to the artifacts built before the flag existed.
    #[arg(long)]
    gdn_core_wide: bool,
    /// Predeclared exact original source/context/component pins for NEW full-court25 artifacts.
    #[arg(long)]
    full_court_source_pin: Option<PathBuf>,
    /// Where the chunks spool (default `<out>.chunks`).
    #[arg(long)]
    chunk_store: Option<PathBuf>,
    /// Keep the chunk store afterwards (it deduplicates later conversions).
    #[arg(long)]
    keep_chunks: bool,
    /// MiB of `f32` per block of rows (default 16).
    #[arg(long, default_value_t = 16)]
    block_mib: usize,
    /// A row-wise tensor of at least this many MiB as `f32` is made by blocks (default 16).
    #[arg(long, default_value_t = 16)]
    defer_min_mib: usize,
    /// Row-streamed calibration (RFC-0002 Part II §II.9 L1): a param of at least this many MiB as `f32` is read when an op asks, never
    /// resident with its layer (a stack of experts by the experts a router selected). 0: off (default). The statistics are the same.
    #[arg(long, default_value_t = 0)]
    calib_lazy_mib: usize,
    /// What the lazy params may keep resident between asks, in MiB (default 512).
    #[arg(long, default_value_t = 512)]
    calib_lazy_cache_mib: usize,
    /// The tokenizer the class binds (default: `<model>/tokenizer.json`, else another tokenizer file of the directory
    /// (`artifact::TOKENIZER_FILES_V1`), zero when it has none).
    #[arg(long)]
    tokenizer: Option<PathBuf>,
    /// RFC-0020 §2: append RAM accounting (live and peak bytes by kind and stage, the resident set) as JSON lines to this file.
    #[arg(long)]
    ram_accounting: Option<PathBuf>,
    /// RFC-0020 §6 (opt-in): spill the carries between occurrences to this directory when a sequence's carries would exceed
    /// `--spill-carries-budget-mib` (bit-identical to the in-RAM run; trades RAM for disk I/O).
    #[arg(long)]
    spill_carries_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 0)]
    spill_carries_budget_mib: u64,
    /// The math the tables and scales are computed with: `libm-v1` (pure-Rust libm; the same bytes on
    /// every platform — the default) or `std` (the platform's libm: reproduces an artifact built
    /// before libm-v1, on the platform that built it).
    #[arg(long, default_value = "libm-v1")]
    math: String,
    /// A quant-format descriptor (`misaka.palw.quant-format.v1`, a JSON file) for a type or a
    /// `quantization_config` the built-in registry does not describe; repeatable. A runtime pack pins
    /// each by digest.
    #[arg(long = "quant-format")]
    quant_format: Vec<PathBuf>,
    /// A model adapter file (`misaka.palw.model-adapter.v1`) in place of the built-in choice; a runtime
    /// pack pins it by hash.
    #[arg(long)]
    adapter: Option<PathBuf>,
    /// Print the report as JSON.
    #[arg(long)]
    json: bool,

    // ---- genesis 8: the PALWART3 container, streamed (no PALWTIR1 is written; `--out` is ignored) ----
    /// Write a `PALWART3` artifact here (index `<stem>.palwart` and segments `<stem>.<k>.palwseg`).
    #[arg(long)]
    palwart_out_dir: Option<PathBuf>,
    #[arg(long)]
    palwart_stem: Option<String>,
    /// The manifest JSON (source repo and revision pin, license, attribution, conversion), written canonically.
    #[arg(long)]
    palwart_manifest: Option<PathBuf>,
    /// The weight family (`qwen35-gdn-hybrid`, `gemma4-unified-dense`, `gpt-oss-moe`, `llama-dense`, …).
    #[arg(long, default_value = "none")]
    palwart_family: String,
    #[arg(long, default_value_t = 2048)]
    palwart_segment_mib: u64,
    /// Where the tokenizer, chat template, configs and license files are read from (default: the model directory).
    #[arg(long)]
    palwart_source_dir: Option<PathBuf>,
    #[arg(long = "palwart-license-file")]
    palwart_license_file: Vec<PathBuf>,
    #[arg(long = "palwart-calibration-file")]
    palwart_calibration_file: Vec<PathBuf>,
    #[arg(long)]
    palwart_provenance: Option<PathBuf>,
    /// The program each finished segment is handed to (environment PALWART_SEGMENT_PATH/INDEX/BYTES/SHA256).
    #[arg(long)]
    palwart_on_segment: Option<String>,
    #[arg(long = "palwart-on-segment-arg", allow_hyphen_values = true)]
    palwart_on_segment_arg: Vec<String>,
    /// Delete each segment after the program succeeded (it uploaded and verified it).
    #[arg(long)]
    palwart_delete_after_hook: bool,

    // ---- genesis 8: shards fetched on first read, evicted after use ----
    /// Read the checkpoint lazily: `<shard>.header` stubs beside the index give every shard's header.
    #[arg(long)]
    lazy_shards: bool,
    /// JSON object `{shard file name: bytes}` (the registry's lengths).
    #[arg(long)]
    shard_sizes: Option<PathBuf>,
    /// The program that fetches a shard (environment PALW_SHARD_NAME/PATH/BYTES).
    #[arg(long)]
    shard_fetch: Option<String>,
    #[arg(long = "shard-fetch-arg", allow_hyphen_values = true)]
    shard_fetch_arg: Vec<String>,
    /// The most shards on disk at once.
    #[arg(long, default_value_t = 2)]
    shard_keep: usize,

    // ---- genesis 8: a GGUF streamed over HTTPS range reads, never stored (needs the `remote` feature) ----
    /// The pinned `resolve` URL of a GGUF file (`https://huggingface.co/<repo>/resolve/<sha>/<file>.gguf`); `model` is then only
    /// its label (a path beside the tokenizer and metadata files the container carries).
    #[arg(long)]
    gguf_url: Option<String>,
    /// Its length in bytes (the registry's).
    #[arg(long)]
    gguf_bytes: Option<u64>,
    /// Its LFS SHA-256 (hex): the whole-file hash of the streamed bytes, in order, must equal it before the index is written.
    #[arg(long)]
    gguf_sha256: Option<String>,
    /// Bytes per range read, in MiB.
    #[arg(long, default_value_t = 64)]
    gguf_window_mib: u64,
    /// Windows kept in memory.
    #[arg(long, default_value_t = 8)]
    gguf_cache_windows: usize,
}

#[cfg(feature = "remote")]
fn gguf_source(a: &Args) -> Result<Option<std::sync::Arc<dyn misaka_palw_tir_lower::weights::remote_blob::ByteSourceV1>>, String> {
    let Some(url) = &a.gguf_url else { return Ok(None) };
    let bytes = a.gguf_bytes.ok_or("--gguf-url needs --gguf-bytes")?;
    let hex = a.gguf_sha256.as_deref().ok_or("--gguf-url needs --gguf-sha256")?;
    if hex.len() != 64 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("--gguf-sha256: 64 hex digits".into());
    }
    let mut sha = [0u8; 32];
    for (i, o) in sha.iter_mut().enumerate() {
        *o = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())?;
    }
    let src = misaka_palw_tir_lower::weights::remote_blob::RemoteHashedFileV1::new(
        Box::new(misaka_palw_tir_lower::weights::CurlFetcher::new()),
        url,
        bytes,
        sha,
        a.gguf_window_mib << 20,
        a.gguf_cache_windows,
    )
    .map_err(|e| e.to_string())?;
    Ok(Some(std::sync::Arc::new(src)))
}

#[cfg(not(feature = "remote"))]
fn gguf_source(a: &Args) -> Result<Option<std::sync::Arc<dyn misaka_palw_tir_lower::weights::remote_blob::ByteSourceV1>>, String> {
    if a.gguf_url.is_some() {
        return Err("--gguf-url reads over the network: build palw-tir-convert with `--features remote`".into());
    }
    Ok(None)
}

fn read_json_file(p: &std::path::Path) -> Result<serde_json::Value, String> {
    let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
    if bytes.len() > 16 << 20 {
        return Err(format!("{}: larger than 16 MiB", p.display()));
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", p.display()))
}

/// The genesis-8 options of `a`, into the request.
fn genesis_options(a: &Args, req: &mut ConvertRequest) -> Result<(), String> {
    use misaka_palw_tir_artifact::palwart_v3::family;
    if let Some(dir) = &a.palwart_out_dir {
        let stem = a.palwart_stem.clone().ok_or("--palwart-out-dir needs --palwart-stem")?;
        let manifest = read_json_file(a.palwart_manifest.as_deref().ok_or("--palwart-out-dir needs --palwart-manifest")?)?;
        let fam = family::from_name(&a.palwart_family).ok_or_else(|| format!("--palwart-family {}: unknown", a.palwart_family))?;
        let provenance = match &a.palwart_provenance {
            Some(p) => read_json_file(p)?,
            None => serde_json::json!({}),
        };
        let source_dir = a.palwart_source_dir.clone().or_else(|| a.model.is_dir().then(|| a.model.clone()));
        let mut on_segment = Vec::new();
        if let Some(p) = &a.palwart_on_segment {
            on_segment.push(p.clone());
            on_segment.extend(a.palwart_on_segment_arg.iter().cloned());
        }
        if a.palwart_delete_after_hook && on_segment.is_empty() {
            return Err("--palwart-delete-after-hook without --palwart-on-segment would lose the segments".into());
        }
        req.palwart = Some(misaka_palw_tir_lower::palwart_out::PalwartOutV1 {
            inputs: misaka_palw_tir_artifact::palwart_pack::PalwArtPackInputsV3 {
                source_dir,
                manifest,
                family: fam,
                out_dir: dir.clone(),
                stem,
                segment_bytes: a.palwart_segment_mib << 20,
                license_files: a.palwart_license_file.clone(),
                calibration_files: a.palwart_calibration_file.clone(),
                provenance,
            },
            on_segment,
            delete_after_hook: a.palwart_delete_after_hook,
        });
    }
    if a.lazy_shards {
        let sizes = read_json_file(a.shard_sizes.as_deref().ok_or("--lazy-shards needs --shard-sizes")?)?;
        let sizes = sizes
            .as_object()
            .ok_or("--shard-sizes: a JSON object")?
            .iter()
            .map(|(k, v)| v.as_u64().map(|n| (k.clone(), n)).ok_or_else(|| format!("--shard-sizes: `{k}` is not a byte count")))
            .collect::<Result<std::collections::BTreeMap<String, u64>, String>>()?;
        let mut fetch = vec![a.shard_fetch.clone().ok_or("--lazy-shards needs --shard-fetch")?];
        fetch.extend(a.shard_fetch_arg.iter().cloned());
        req.lazy_shards = Some(misaka_palw_tir_lower::weights::lazy::LazyShardSpecV1 { fetch, keep: a.shard_keep });
        req.shard_sizes = sizes;
    }
    Ok(())
}

fn run(a: &Args) -> Result<serde_json::Value, String> {
    let t0 = Instant::now();
    let log = |m: String| eprintln!("[{:>7.1}s] {m}", t0.elapsed().as_secs_f64());
    let math = MathMode::parse(&a.math).ok_or_else(|| format!("--math {}: libm-v1 or std", a.math))?;
    if let Some(p) = &a.ram_accounting {
        misaka_palw_tir_lower::ram_account::enable(p).map_err(|e| format!("{}: {e}", p.display()))?;
        misaka_palw_tir_lower::ram_account::stage("open");
    }
    if let Some(dir) = &a.spill_carries_dir {
        misaka_palw_tir_lower::float_ref::stream::set_carry_spill(Some(misaka_palw_tir_lower::float_ref::stream::SpillCfg {
            dir: dir.clone(),
            budget_bytes: a.spill_carries_budget_mib << 20,
        }));
    }
    let mut req = ConvertRequest::new(&a.model, &a.out);
    req.stats_in = a.stats_in.clone();
    req.calib = match &a.calib {
        Some(p) => Some(CalibInput::from_file(p).map_err(|e| e.to_string())?),
        None => None,
    };
    req.calib_seqs = a.calib_seqs;
    req.positions = a.positions;
    req.stats_out = a.stats_out.clone();
    req.context = a.context;
    req.policy = QuantPolicy { headroom16: a.headroom16, headroom32: a.headroom32, headroom_resid: a.headroom_resid };
    req.max_window = a.max_window;
    req.linear_weight_bits = a.linear_weight_bits;
    req.gdn_core_wide = a.gdn_core_wide;
    req.full_court_source_pin = a
        .full_court_source_pin
        .as_ref()
        .map(|p| misaka_palw_tir_lower::full_court_source25::SourcePinV25::from_file(p))
        .transpose()
        .map_err(|e| e.to_string())?;
    req.chunk_store = a.chunk_store.clone();
    req.keep_chunks = a.keep_chunks;
    req.block_mib = a.block_mib;
    req.defer_min_mib = a.defer_min_mib;
    req.calib_lazy_mib = a.calib_lazy_mib;
    req.calib_lazy_cache_mib = a.calib_lazy_cache_mib;
    req.tokenizer = a.tokenizer.clone();
    req.math = math;
    req.quant_formats = a.quant_format.clone();
    req.adapter = a.adapter.clone();
    genesis_options(a, &mut req)?;
    req.gguf_source = gguf_source(a)?;
    let o = convert_model(&req, &log).map_err(|e| e.to_string())?;
    misaka_palw_tir_lower::ram_account::finish();
    if let Some(src) = &req.gguf_source {
        let (fetched, refetched) = src.fetched();
        log(format!(
            "streamed GGUF: {:.1} GiB fetched, {refetched} windows refetched and re-checked; whole-file SHA-256 equals the pin",
            fetched as f64 / (1u64 << 30) as f64
        ));
    }
    let r = &o.report;
    if !r.stream.whole.is_empty() {
        log(format!("loaded whole after deferral (no row-wise form): {:?}", r.stream.whole));
    }
    Ok(serde_json::json!({
        "architecture": o.architecture,
        "artifact": a.out.display().to_string(),
        "file_digest": r.file_digest,
        "file_bytes": r.file_bytes,
        "tensors": r.stream.instances,
        "tensor_bytes": r.stream.bytes,
        "by_blocks": r.stream.by_blocks,
        "blocks": r.stream.blocks,
        "chunks": r.chunks,
        "distinct_chunks": r.distinct_chunks,
        "resid_scale": r.resid_scale,
        "logits_scale": r.logits_scale,
        "quant_inexact": r.quant_inexact,
        "loaded_whole": r.stream.whole,
        "adapter": misaka_palw_tir_lower::convert::adapter_json(&o.frontend.adapter),
        "spec_digest": o.frontend.spec_digest,
        "scope": o.scope.headline(),
        "stats_digest": o.stats_digest,
        "seconds": t0.elapsed().as_secs_f64(),
        "palwart": o.palwart.as_ref().map(|p| {
            let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
            serde_json::json!({
                "root": hex(&p.written.root),
                "index": p.written.index_path.display().to_string(),
                "data_bytes": p.written.header.data_bytes,
                "segment_bytes": p.written.header.segment_bytes,
                "segments": p.written.header.segments.iter().enumerate().map(|(k, s)| serde_json::json!({
                    "index": k, "bytes": s.bytes, "sha256": hex(&s.sha256),
                })).collect::<Vec<_>>(),
                "tir_inventory_root": hex(&p.inner_root),
                "tir_leaves": p.leaves,
                "tensor_bytes": p.tensor_bytes,
                "kernel_id": hex(&p.written.header.program.kernel_id),
                "tokenizer_id": hex(&p.written.header.program.tokenizer_id),
            })
        }),
    }))
}

fn main() {
    let a = Args::parse();
    match run(&a) {
        Ok(v) => {
            if a.json {
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            } else {
                println!("{}: {}", v["artifact"].as_str().unwrap_or("?"), v["file_digest"].as_str().unwrap_or("?"));
            }
        }
        Err(e) => {
            eprintln!("palw-tir-convert: {e}");
            std::process::exit(1);
        }
    }
}
