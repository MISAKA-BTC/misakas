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
    /// Row-streamed calibration (RFC-0002 Part II §II.9 L1): a param of at least this many MiB as `f32` is read when an op asks, never
    /// resident with its layer (a stack of experts by the experts a router selected). 0: off (default). The statistics are the same.
    #[arg(long, default_value_t = 0)]
    calib_lazy_mib: usize,
    /// What the lazy params may keep resident between asks, in MiB (default 512).
    #[arg(long, default_value_t = 512)]
    calib_lazy_cache_mib: usize,
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
    /// A model adapter file (`misaka.palw.model-adapter.v1`) in place of the built-in choice; a runtime
    /// pack pins it by hash.
    #[arg(long)]
    adapter: Option<PathBuf>,
    /// Print the report as JSON.
    #[arg(long)]
    json: bool,
}

fn run(a: &Args) -> Result<serde_json::Value, String> {
    let t0 = Instant::now();
    let log = |m: String| eprintln!("[{:>7.1}s] {m}", t0.elapsed().as_secs_f64());
    let math = MathMode::parse(&a.math).ok_or_else(|| format!("--math {}: libm-v1 or std", a.math))?;
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
    let o = convert_model(&req, &log).map_err(|e| e.to_string())?;
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
