//! `palw-tir-fidelity` — lower a Hugging Face checkpoint to PALW-TIR, calibrate it, materialise
//! its integer artifact, and measure the integer program (on the `misaka-palw-tir` reference
//! evaluator) against the float reference: top-1 agreement, KL and the perplexity delta.
//!
//! Every stage streams one block occurrence at a time; run ONE real checkpoint at a time.
//!
//! Token files are JSON `{"source": …, "sequences": [[id, …], …]}` (tokenise with the model's own
//! `tokenizer.json`, offline). Without them, seeded random sequences are used (tiny models).

use clap::Parser;
use misaka_palw_tir_lower::float_ref::stream::Streamed;
use misaka_palw_tir_lower::lower::{LowerOpts, materialise, program_summary};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{artifact, fidelity};
use rayon::prelude::*;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "palw-tir-fidelity", about = "Integer PALW-TIR program vs the float reference of a Hugging Face checkpoint")]
struct Args {
    /// Checkpoint directory: config.json and model.safetensors (or an index).
    model: PathBuf,
    /// Calibration sequences (JSON token file). Default: 4 random sequences of 32.
    #[arg(long)]
    calib: Option<PathBuf>,
    /// Evaluation sequences (JSON token file). Default: 2 random sequences of 32.
    #[arg(long)]
    eval: Option<PathBuf>,
    /// Keep at most this many positions of every sequence.
    #[arg(long)]
    positions: Option<usize>,
    /// Keep at most this many calibration sequences.
    #[arg(long)]
    calib_seqs: Option<usize>,
    /// Keep at most this many evaluation sequences.
    #[arg(long)]
    eval_seqs: Option<usize>,
    /// Integer sequences evaluated in parallel (each holds a whole-model evaluation).
    #[arg(long, default_value_t = 1)]
    jobs: usize,
    /// Write the TIR program (canonical encoding).
    #[arg(long)]
    tir_out: Option<PathBuf>,
    /// Write the integer artifact.
    #[arg(long)]
    artifact_out: Option<PathBuf>,
    /// Write the per-site calibration statistics (JSON).
    #[arg(long)]
    stats_out: Option<PathBuf>,
    /// Headroom of i16 codes over the calibrated absmax.
    #[arg(long, default_value_t = 2.0)]
    headroom16: f64,
    /// Print the result as JSON.
    #[arg(long)]
    json: bool,
}

fn tokens(path: &Option<PathBuf>, vocab: usize, count: usize, seed: u64) -> Result<(Vec<Vec<usize>>, serde_json::Value), String> {
    match path {
        None => Ok((fidelity::random_sequences(vocab, count, 32, seed), serde_json::json!(format!("random (seed {seed})")))),
        Some(p) => {
            let v: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?).map_err(|e| e.to_string())?;
            let seqs: Vec<Vec<usize>> = v["sequences"]
                .as_array()
                .ok_or("token file: no `sequences`")?
                .iter()
                .map(|s| s.as_array().map(|a| a.iter().filter_map(|t| t.as_u64()).map(|t| t as usize).collect()).unwrap_or_default())
                .collect();
            if seqs.iter().flatten().any(|t| *t >= vocab) {
                return Err(format!("{}: a token ≥ vocab {vocab}", p.display()));
            }
            Ok((seqs, v.get("source").cloned().unwrap_or(serde_json::Value::Null)))
        }
    }
}

fn run(a: &Args) -> Result<serde_json::Value, String> {
    let t0 = Instant::now();
    let log = |m: String| eprintln!("[{:>7.1}s] {m}", t0.elapsed().as_secs_f64());
    let cfg = std::fs::read_to_string(a.model.join("config.json")).map_err(|e| format!("config.json: {e}"))?;
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).map_err(|e| e.to_string())?;
    log(format!("{} — {}", prep.spec.architecture, program_summary(&prep.lowered.program).lines().next().unwrap_or("")));
    if let Some(p) = &a.tir_out {
        std::fs::write(p, prep.lowered.program.encode()).map_err(|e| e.to_string())?;
    }
    let ck = Checkpoint::open(&a.model).map_err(|e| e.to_string())?;
    let loader = Streamed { prog: &prep.hl, binding: &prep.binding, source: &ck };
    let vocab = prep.hl.vocab;
    let cut = |mut s: Vec<Vec<usize>>, n: Option<usize>| {
        if let Some(n) = n {
            s.truncate(n);
        }
        if let Some(p) = a.positions {
            s.iter_mut().for_each(|q| q.truncate(p));
        }
        s.retain(|q| !q.is_empty());
        s
    };
    let (calib, calib_src) = tokens(&a.calib, vocab, 4, 11)?;
    let calib = cut(calib, a.calib_seqs);
    let (eval, eval_src) = tokens(&a.eval, vocab, 2, 29)?;
    let eval = cut(eval, a.eval_seqs);
    let progress = |what: &'static str| move |d: usize, n: usize| {
        if d == n || d % 8 == 0 {
            eprintln!("[{:>7.1}s]   {what} {d}/{n}", t0.elapsed().as_secs_f64());
        }
    };
    log(format!("calibrating on {} sequences, {} positions", calib.len(), calib.iter().map(Vec::len).sum::<usize>()));
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &progress("calibration")).map_err(|e| e.to_string())?;
    if let Some(p) = &a.stats_out {
        std::fs::write(p, serde_json::to_vec_pretty(&stats).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    let policy = QuantPolicy { headroom16: a.headroom16, ..QuantPolicy::default() };
    log("materialising the integer artifact".into());
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &policy, &progress("materialise")).map_err(|e| e.to_string())?;
    log(format!(
        "artifact: {} tensors, {:.1} MiB; residual scale {:.3e}, logit scale {:.3e}",
        mat.params.tensors.len(),
        mat.params.bytes() as f64 / (1 << 20) as f64,
        mat.resid_scale,
        mat.logits_scale
    ));
    let mut digest = None;
    if let Some(p) = &a.artifact_out {
        let meta = serde_json::json!({
            "architecture": prep.spec.architecture,
            "resid_scale": mat.resid_scale,
            "logits_scale": mat.logits_scale,
            "policy": { "headroom16": policy.headroom16, "headroom32": policy.headroom32, "headroom_resid": policy.headroom_resid },
            "calibration": calib_src,
        });
        digest = Some(artifact::write(p, &prep.lowered.program, &mat.params, meta).map_err(|e| e.to_string())?);
    }
    log(format!("float reference on {} sequences", eval.len()));
    let fl = fidelity::float_logits(&prep.hl, &loader, &eval, &progress("float")).map_err(|e| e.to_string())?;
    log("integer program on the reference evaluator".into());
    let pool = rayon::ThreadPoolBuilder::new().num_threads(a.jobs.max(1)).build().map_err(|e| e.to_string())?;
    let il: Vec<Vec<Vec<f64>>> = pool.install(|| {
        eval.par_iter()
            .enumerate()
            .map(|(si, s)| {
                fidelity::int_logits(&prep.lowered.program, &mat.params, s, mat.logits_scale, &|p| {
                    if (p + 1) % 16 == 0 || p + 1 == s.len() {
                        eprintln!("[{:>7.1}s]   integer seq {si}: {}/{}", t0.elapsed().as_secs_f64(), p + 1, s.len());
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()
    })
    .map_err(|e| e.to_string())?;
    let m = fidelity::compare(&fl, &il, &eval);
    log(format!(
        "top-1 {:.4}  KL mean {:.5} max {:.4}  ppl float {:.3} int {:.3} (Δ {:+.2}%)",
        m.top1_agreement,
        m.kl_mean,
        m.kl_max,
        m.ppl_float,
        m.ppl_int,
        m.ppl_delta * 100.0
    ));
    Ok(serde_json::json!({
        "architecture": prep.spec.architecture,
        "program_bytes": prep.lowered.program.encode().len(),
        "program_digest": artifact::program_digest(&prep.lowered.program),
        "artifact_digest": digest,
        "artifact_bytes": mat.params.bytes(),
        "calibration": { "source": calib_src, "sequences": calib.len(), "positions": calib.iter().map(Vec::len).sum::<usize>() },
        "evaluation": { "source": eval_src, "sequences": eval.len(), "positions": eval.iter().map(Vec::len).sum::<usize>() },
        "metrics": m,
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
                let m = &v["metrics"];
                println!(
                    "{}: top-1 {:.4}, KL {:.5} nats (max {:.4}), ppl {:.3} → {:.3} ({:+.2}%), {} positions",
                    v["architecture"].as_str().unwrap_or("?"),
                    m["top1_agreement"].as_f64().unwrap_or(f64::NAN),
                    m["kl_mean"].as_f64().unwrap_or(f64::NAN),
                    m["kl_max"].as_f64().unwrap_or(f64::NAN),
                    m["ppl_float"].as_f64().unwrap_or(f64::NAN),
                    m["ppl_int"].as_f64().unwrap_or(f64::NAN),
                    m["ppl_delta"].as_f64().unwrap_or(f64::NAN) * 100.0,
                    m["positions"]
                );
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
