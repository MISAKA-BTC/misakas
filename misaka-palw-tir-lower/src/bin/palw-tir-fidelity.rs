//! `palw-tir-fidelity` — lower a Hugging Face checkpoint to PALW-TIR, calibrate it, materialise
//! its integer artifact, and measure the integer program (on the `misaka-palw-tir` reference
//! evaluator) against the float reference: top-1 agreement, KL and the perplexity delta.
//!
//! Every stage streams one block occurrence at a time; run ONE real checkpoint at a time.
//!
//! Token files are JSON `{"source": …, "sequences": [[id, …], …]}` (tokenise with the model's own
//! `tokenizer.json`, offline). Without them, seeded random sequences are used (tiny models).

use clap::Parser;
use misaka_palw_tir_lower::float_ref::stream::{OccParams, Streamed};
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
    /// Write the integer artifact (a `PALWTIR1` container: program, tensors, provenance).
    #[arg(long)]
    artifact_out: Option<PathBuf>,
    /// Write the per-site calibration statistics (JSON).
    #[arg(long)]
    stats_out: Option<PathBuf>,
    /// Read calibration statistics written by `--stats-out` instead of calibrating (they must
    /// come from the same checkpoint and calibration set).
    #[arg(long, conflicts_with = "stats_out")]
    stats_in: Option<PathBuf>,
    /// Headroom of i16 codes over the calibrated absmax.
    #[arg(long, default_value_t = 2.0)]
    headroom16: f64,
    /// Stop after calibration (with `--stats-out`, a statistics dump).
    #[arg(long)]
    calibrate_only: bool,
    /// Print the result as JSON.
    #[arg(long)]
    json: bool,
    /// Run the integer program on the typed backend (`misaka-palw-tir-exec`, byte-identical to the
    /// reference evaluator and far faster) instead of the reference evaluator.
    #[arg(long)]
    exec: bool,
    /// With `--exec`: also run the first N positions of the first evaluation sequence on the
    /// reference evaluator and refuse unless every logit is equal.
    #[arg(long, default_value_t = 0)]
    cross_check: usize,
    /// Evaluate a recurrent program beyond its longest calibration sequence anyway (the
    /// calibration-length rule is refused otherwise; the result records the waiver).
    #[arg(long)]
    allow_short_calibration: bool,
    /// Report the recurrence drift: the mean KL over positions [64, 192) against the last 128
    /// (corpus-v1 §9: at 4,096 positions, within 1.5×).
    #[arg(long)]
    drift: bool,
    /// Diagnose: the N sites with the largest relative error (‖int − float‖ / ‖float‖ over the
    /// first `--site-positions` positions of the first evaluation sequence), from a traced float
    /// run and the reference evaluator. Loads every weight as f32: small models.
    #[arg(long, default_value_t = 0)]
    sites: usize,
    #[arg(long, default_value_t = 16)]
    site_positions: usize,
    /// Keep at most this many positions of history in any block (`LowerOpts::max_window`): a
    /// layout whose context is shorter than the history bound, whose attention cone is then
    /// counted at this window. Evaluation sequences must not be longer (the float reference
    /// keeps the model's own window).
    #[arg(long)]
    max_window: Option<u32>,
}

fn tokens(path: &Option<PathBuf>, vocab: usize, count: usize, seed: u64) -> Result<(Vec<Vec<usize>>, serde_json::Value), String> {
    match path {
        None => Ok((fidelity::random_sequences(vocab, count, 32, seed), serde_json::json!(format!("random (seed {seed})")))),
        Some(p) => {
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?)
                .map_err(|e| e.to_string())?;
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
    let opts = LowerOpts { max_window: a.max_window, ..LowerOpts::default() };
    let prep = fidelity::prepare(&cfg, &opts).map_err(|e| e.to_string())?;
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
    if let Some(w) = a.max_window {
        if let Some(n) = eval.iter().map(Vec::len).max().filter(|n| *n > w as usize) {
            return Err(format!("an evaluation sequence of {n} positions is longer than --max-window {w}"));
        }
    }
    // The calibration-length rule: a recurrent program is calibrated on a sequence as long as the
    // context it is evaluated at (with --stats-in, the statistics come from the --calib file).
    let context = eval.iter().map(Vec::len).max().unwrap_or(0);
    let calibrated = match fidelity::check_calibration_length(&prep.hl, &calib, context) {
        Ok(v) => serde_json::json!({ "rule": if v.is_some() { "met" } else { "not recurrent" }, "longest": v, "context": context }),
        Err(e) if a.allow_short_calibration => {
            log(format!("WAIVED: {e}"));
            serde_json::json!({ "rule": "waived", "longest": calib.iter().map(Vec::len).max(), "context": context })
        }
        Err(e) => return Err(format!("{e}; pass --allow-short-calibration to measure anyway")),
    };
    let progress = |what: &'static str| {
        move |d: usize, n: usize| {
            if d == n || d.is_multiple_of(8) {
                eprintln!("[{:>7.1}s]   {what} {d}/{n}", t0.elapsed().as_secs_f64());
            }
        }
    };
    let stats = match &a.stats_in {
        Some(p) => {
            log(format!("calibration statistics from {}", p.display()));
            serde_json::from_slice(&std::fs::read(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?
        }
        None => {
            log(format!("calibrating on {} sequences, {} positions", calib.len(), calib.iter().map(Vec::len).sum::<usize>()));
            fidelity::calibrate(&prep.hl, &loader, &calib, &progress("calibration")).map_err(|e| e.to_string())?
        }
    };
    if let Some(p) = &a.stats_out {
        std::fs::write(p, serde_json::to_vec_pretty(&stats).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    if a.calibrate_only {
        return Ok(serde_json::json!({ "architecture": prep.spec.architecture, "sites": stats.len(), "metrics": {} }));
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
            "calibrated_context": calib.iter().map(Vec::len).max(),
            // `{rule: met | not recurrent | waived, longest, context}` — the calibration-length rule
            // as this run applied it (`waived` under --allow-short-calibration).
            "calibration_length_rule": calibrated,
            "max_window": a.max_window,
        });
        // The checkpoint's tokenizer.json binds the artifact to its tokenizer (zero when absent).
        let tokenizer_id = match std::fs::read(a.model.join("tokenizer.json")) {
            Ok(bytes) => artifact::tokenizer_id_of(&bytes),
            Err(_) => [0u8; 64],
        };
        digest = Some(artifact::write(p, &prep.lowered.program, &mat.params, tokenizer_id, meta).map_err(|e| e.to_string())?);
    }
    log(format!("float reference on {} sequences", eval.len()));
    // On the typed backend both sides are compared a position at a time: the float reference runs
    // to the post block's inputs and the post block (stateless) is evaluated per position.
    let (fl, post_in) = if a.exec {
        let post_in = misaka_palw_tir_lower::float_ref::stream::run_to_post(&prep.hl, &loader, &eval, &progress("float"))
            .map_err(|e| e.to_string())?;
        (Vec::new(), post_in)
    } else {
        (fidelity::float_logits(&prep.hl, &loader, &eval, &progress("float")).map_err(|e| e.to_string())?, Vec::new())
    };
    // On the typed backend the integer rows are compared as they are made (a long evaluation
    // would otherwise hold both sides' rows); on the reference evaluator they are kept.
    let (m, drift) = if a.exec {
        log("integer program on the typed backend (misaka-palw-tir-exec)".into());
        let mut acc = fidelity::Accumulator::new((64, 192), 128);
        let mut first: Vec<Vec<f64>> = Vec::new();
        let post_store = loader.load(prep.hl.post, None).map_err(|e| e.to_string())?;
        let mut float_err: Option<String> = None;
        for (si, s) in eval.iter().enumerate() {
            fidelity::int_logits_exec_each(&prep.lowered.program, &mat.params, s, mat.logits_scale, &mut |p, row| {
                let f = match misaka_palw_tir_lower::float_ref::stream::post_logits(&prep.hl, &post_store, &post_in[si][p], s[p], p) {
                    Ok(f) => f,
                    Err(e) => {
                        float_err.get_or_insert(e.to_string());
                        return;
                    }
                };
                acc.push(p, s.len(), &f, row, s.get(p + 1).copied());
                if si == 0 && p < a.cross_check {
                    first.push(row.to_vec());
                }
                if (p + 1) % 64 == 0 || p + 1 == s.len() {
                    eprintln!("[{:>7.1}s]   integer seq {si}: {}/{}", t0.elapsed().as_secs_f64(), p + 1, s.len());
                }
            })
            .map_err(|e| e.to_string())?;
            if let Some(e) = float_err.take() {
                return Err(format!("float reference, post block: {e}"));
            }
        }
        if a.cross_check > 0 {
            let s = &eval[0][..a.cross_check.min(eval[0].len())];
            let r =
                fidelity::int_logits(&prep.lowered.program, &mat.params, s, mat.logits_scale, &|_| {}).map_err(|e| e.to_string())?;
            if r[..] != first[..r.len()] {
                return Err("the typed backend's logits differ from the reference evaluator's".into());
            }
            log(format!("cross-check: the first {} positions equal on the reference evaluator", r.len()));
        }
        (acc.metrics(), a.drift.then(|| acc.drift()))
    } else {
        log("integer program on the reference evaluator".into());
        let pool = rayon::ThreadPoolBuilder::new().num_threads(a.jobs.max(1)).build().map_err(|e| e.to_string())?;
        let il: Vec<Vec<Vec<f64>>> = pool
            .install(|| {
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
        (fidelity::compare(&fl, &il, &eval), a.drift.then(|| fidelity::drift(&fl, &il, (64, 192), 128)))
    };
    if a.sites > 0 {
        log(format!("site errors over the first {} positions (every weight as f32)", a.site_positions));
        let (params_f, _) =
            misaka_palw_tir_lower::float_ref::ParamStore::from_source(&prep.hl, &prep.binding, &ck).map_err(|e| e.to_string())?;
        let seq = &eval[0][..a.site_positions.min(eval[0].len())];
        let errs = fidelity::site_errors(&prep, &params_f, &stats, &policy, &mat, seq).map_err(|e| e.to_string())?;
        for e in errs.iter().take(a.sites) {
            eprintln!("  site {:>40}  rel {:.5}  max|Δ| {:.4e}  |f|max {:.4e}", e.key, e.rel_l2, e.max_abs, e.float_absmax);
        }
    }
    if let Some(d) = &drift {
        log(format!(
            "drift: mean KL {:.5} over positions {}..{}, {:.5} over {}..{} (×{:.2})",
            d.kl_early, d.early_window.0, d.early_window.1, d.kl_late, d.late_window.0, d.late_window.1, d.ratio
        ));
    }
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
        "calibration": { "source": calib_src, "sequences": calib.len(), "positions": calib.iter().map(Vec::len).sum::<usize>(), "length_rule": calibrated },
        "evaluation": { "source": eval_src, "sequences": eval.len(), "positions": eval.iter().map(Vec::len).sum::<usize>() },
        "metrics": m,
        "drift": drift,
        "backend": if a.exec { "misaka-palw-tir-exec" } else { "misaka-palw-tir reference evaluator" },
        "max_window": a.max_window,
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
