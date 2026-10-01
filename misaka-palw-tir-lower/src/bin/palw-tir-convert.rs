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

use clap::Parser;
use misaka_palw_tir_lower::calib::{stats_digest_hex, stats_from_json, stats_to_json};
use misaka_palw_tir_lower::convert::{ConvertOpts, convert_to_container};
use misaka_palw_tir_lower::detmath::{MathMode, platform, set_mode};
use misaka_palw_tir_lower::float_ref::SiteStat;
use misaka_palw_tir_lower::float_ref::stream::Streamed;
use misaka_palw_tir_lower::lower::{LowerOpts, StreamOpts};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::quantfmt::QuantRegistry;
use misaka_palw_tir_lower::{artifact, fidelity};
use std::collections::BTreeMap;
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
    /// input a runtime pack pins by digest).
    #[arg(long, conflicts_with = "calib")]
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
    /// The tokenizer the class binds (default: `<model>/tokenizer.json`, zero when absent).
    #[arg(long)]
    tokenizer: Option<PathBuf>,
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
    /// Print the report as JSON.
    #[arg(long)]
    json: bool,
}

fn tokens(path: &PathBuf) -> Result<(Vec<Vec<usize>>, serde_json::Value), String> {
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?).map_err(|e| e.to_string())?;
    let seqs: Vec<Vec<usize>> = v["sequences"]
        .as_array()
        .ok_or("token file: no `sequences`")?
        .iter()
        .map(|s| s.as_array().map(|a| a.iter().filter_map(|t| t.as_u64()).map(|t| t as usize).collect()).unwrap_or_default())
        .collect();
    Ok((seqs, v.get("source").cloned().unwrap_or(serde_json::Value::Null)))
}

fn run(a: &Args) -> Result<serde_json::Value, String> {
    let t0 = Instant::now();
    let log = |m: String| eprintln!("[{:>7.1}s] {m}", t0.elapsed().as_secs_f64());
    let math = MathMode::parse(&a.math).ok_or_else(|| format!("--math {}: libm-v1 or std", a.math))?;
    // Before `open_model`: lowering evaluates RoPE frequencies and the like, in this mode.
    set_mode(math);
    let opts = LowerOpts { max_window: a.max_window, ..LowerOpts::default() };
    let reg = QuantRegistry::with_files(&a.quant_format).map_err(|e| e.to_string())?;
    let (prep, ck) = fidelity::open_model_with(&a.model, &opts, &reg).map_err(|e| e.to_string())?;
    let descriptors = fidelity::quant_descriptors_used(&a.model, &prep, &reg).map_err(|e| e.to_string())?;
    for (n, d) in &descriptors {
        log(format!("quant format {n} {d}"));
    }
    log(format!("{} — {}", prep.spec.architecture, misaka_palw_tir_lower::lower::program_summary(&prep.lowered.program).lines().next().unwrap_or("")));
    let loader = Streamed::new(&prep.hl, &prep.binding, ck.as_ref());
    let progress = |what: &'static str| {
        move |d: usize, n: usize| {
            if d == n || d.is_multiple_of(8) {
                eprintln!("[{:>7.1}s]   {what} {d}/{n}", t0.elapsed().as_secs_f64());
            }
        }
    };
    let (stats, calib_source): (BTreeMap<String, SiteStat>, serde_json::Value) = match (&a.stats_in, &a.calib) {
        (Some(p), None) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let (stats, exact) = stats_from_json(&text).map_err(|e| e.to_string())?;
            if !exact {
                log(format!("WARNING: {} is in the legacy (decimal float) format — not bit-exact, not for a runtime pack", p.display()));
            }
            log(format!("calibration statistics from {}", p.display()));
            (stats, serde_json::json!(p.display().to_string()))
        }
        (None, Some(p)) => {
            let (mut seqs, source) = tokens(p)?;
            if let Some(n) = a.calib_seqs {
                seqs.truncate(n);
            }
            if let Some(n) = a.positions {
                seqs.iter_mut().for_each(|q| q.truncate(n));
            }
            seqs.retain(|q| !q.is_empty());
            if seqs.iter().flatten().any(|t| *t >= prep.hl.vocab) {
                return Err(format!("{}: a token ≥ vocab {}", p.display(), prep.hl.vocab));
            }
            let longest = seqs.iter().map(Vec::len).max();
            let context = a.context.or(longest).unwrap_or(0);
            fidelity::check_calibration_length(&prep.hl, &seqs, context).map_err(|e| format!("{e}; pass --context to declare the context served"))?;
            log(format!("calibrating on {} sequences, {} positions", seqs.len(), seqs.iter().map(Vec::len).sum::<usize>()));
            let s = fidelity::calibrate(&prep.hl, &loader, &seqs, &progress("calibration")).map_err(|e| e.to_string())?;
            (s, source)
        }
        _ => return Err("calibration statistics are an input of the conversion: give --stats-in (the file a runtime pack pins) or --calib".into()),
    };
    // The statistics' identity: the digest of their canonical serialisation (key order fixed), the
    // same whether they were just measured or read back, so the container's provenance — and with
    // it the file digest — is a function of the statistics and not of how they arrived.
    let stats_digest = stats_digest_hex(&stats);
    log(format!("calibration statistics {} sites, digest {stats_digest} (from {calib_source})", stats.len()));
    if let Some(p) = &a.stats_out {
        std::fs::write(p, stats_to_json(&stats)).map_err(|e| e.to_string())?;
    }
    let policy = QuantPolicy { headroom16: a.headroom16, headroom32: a.headroom32, headroom_resid: a.headroom_resid };
    let tokenizer_path = a.tokenizer.clone().unwrap_or_else(|| a.model.join("tokenizer.json"));
    let tokenizer_id = match std::fs::read(&tokenizer_path) {
        Ok(bytes) => artifact::tokenizer_id_of(&bytes),
        Err(_) => [0u8; 64],
    };
    let store_dir = a.chunk_store.clone().unwrap_or_else(|| {
        let mut s = a.out.clone().into_os_string();
        s.push(".chunks");
        PathBuf::from(s)
    });
    let meta = |m: &misaka_palw_tir_lower::lower::StreamMaterialised| {
        serde_json::json!({
            "architecture": prep.spec.architecture,
            "resid_scale": m.resid_scale,
            "logits_scale": m.logits_scale,
            "policy": { "headroom16": policy.headroom16, "headroom32": policy.headroom32, "headroom_resid": policy.headroom_resid },
            "calibration": { "schema": "misaka.palw.calib-stats.v1", "digest": stats_digest },
            "max_window": a.max_window,
            "quant": { "descriptors": descriptors.iter().map(|(n, d)| serde_json::json!({ "name": n, "digest": d })).collect::<Vec<_>>() },
            "converter": format!("palw-tir-convert {}", env!("CARGO_PKG_VERSION")),
            // The platform is recorded only for `std`: libm-v1 does not depend on it.
            "math": if math == MathMode::Std { serde_json::json!({ "mode": "std", "platform": platform() }) } else { serde_json::json!({ "mode": "libm-v1" }) },
        })
    };
    let copts = ConvertOpts {
        stream: StreamOpts { defer_min_elems: a.defer_min_mib << 18, block_elems: a.block_mib << 18 },
        layout: Vec::new(),
        tokenizer_id,
        meta: &meta,
        keep_chunks: a.keep_chunks,
        math,
    };
    log("converting".into());
    let r = convert_to_container(&prep, &loader, &stats, &policy, &store_dir, &a.out, &copts, &progress("materialise")).map_err(|e| e.to_string())?;
    log(format!(
        "{}: {} tensors, {:.1} MiB in the tensors, {:.1} MiB file; {} instances by {} blocks (largest {:.1} MiB f32); {} chunks ({} distinct, {} deduplicated at write); file digest {}",
        a.out.display(),
        r.stream.instances,
        r.stream.bytes as f64 / (1 << 20) as f64,
        r.file_bytes as f64 / (1 << 20) as f64,
        r.stream.by_blocks,
        r.stream.blocks,
        r.stream.max_block_f32_bytes as f64 / (1 << 20) as f64,
        r.chunks,
        r.distinct_chunks,
        r.store.deduplicated,
        r.file_digest
    ));
    if !r.stream.whole.is_empty() {
        log(format!("loaded whole after deferral (no row-wise form): {:?}", r.stream.whole));
    }
    Ok(serde_json::json!({
        "architecture": prep.spec.architecture,
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
        "seconds": t0.elapsed().as_secs_f64(),
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
