//! **RFC-0013 audit 13-A: the legacy A16 class (#1) compared with its float reference over a LONG sequence, by window.**
//!
//! ```text
//! qwen25-window-compare <dir> --a16 FILE --tokens FILE [--window-len 64] [--windows 0,16352,32704] [--control] [--out report.json]
//! ```
//!
//! `<dir>` holds the source checkpoint (`model.safetensors`, `config.json`); `--a16` is the `PALWB0A2` artifact the class runs;
//! `--tokens` a JSON array of token ids (the whole sequence — e.g. 32,768 ids). Both engines advance the WHOLE prefix: the
//! integer engine one position at a time on one cache, the float reference layer by layer over every position
//! (`reference_forward_windows_v1`); logits are kept only inside the windows (absolute start positions, `--window-len` long), so
//! memory never holds a sequence's worth of vocabulary rows. Per window: top-1 agreement, reference argmax inside the integer
//! top 5, and the mean Spearman correlation over the reference's top 100 (`score_fidelity`, the converter's own measure).
//!
//! `--control` also scores every window against an integer run that starts AT the window (its prefix cut off): a comparator that
//! cannot tell the full-prefix run from that one is not measuring long-context state, so the report states both. The context is
//! never shortened for the main score. Nothing here is a PASS: the report is a measurement, and the acceptance bar (RFC-0013) is
//! applied to it by the reader.
use misaka_palw_base0::artifact::decode_artifact_file_v1;
use misaka_palw_base0::engine_a16::{A16Cache, A16Engine};
use misaka_palw_base0::reference::{RefConfigV1, reference_forward_windows_v1, score_fidelity};

fn die(message: String) -> ! {
    eprintln!("qwen25-window-compare: {message}");
    std::process::exit(2)
}

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

/// KL(reference || integer) in physical logit units, with stable log-sum-exp.
fn logit_kl_v1(reference: &[f32], integer: &[i32], scale: f64) -> Result<f64, &'static str> {
    if reference.is_empty() || reference.len() != integer.len() || !scale.is_finite() || scale <= 0.0 {
        return Err("invalid logit dimensions or scale");
    }
    let r: Vec<f64> = reference.iter().map(|v| f64::from(*v)).collect();
    let q: Vec<f64> = integer.iter().map(|v| f64::from(*v) * scale).collect();
    if r.iter().chain(&q).any(|v| !v.is_finite()) { return Err("non-finite logit"); }
    let rm = r.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let qm = q.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let rz = r.iter().map(|v| (v - rm).exp()).sum::<f64>().ln();
    let qz = q.iter().map(|v| (v - qm).exp()).sum::<f64>().ln();
    let kl: f64 = r.iter().zip(&q).map(|(r, q)| {
        let rp = r - rm - rz;
        rp.exp() * (rp - (q - qm - qz))
    }).sum();
    if !kl.is_finite() { return Err("non-finite divergence"); }
    Ok(kl.max(0.0))
}

fn kl_summary_v1(reference: &[Vec<f32>], integer: &[Vec<i32>], scale: f64) -> serde_json::Value {
    if reference.is_empty() || reference.len() != integer.len() { die("incomplete comparison window".into()); }
    let values: Vec<f64> = reference.iter().zip(integer).map(|(r, q)| {
        logit_kl_v1(r, q, scale).unwrap_or_else(|why| die(why.into()))
    }).collect();
    serde_json::json!({"mean": values.iter().sum::<f64>() / values.len() as f64,
        "max": values.iter().copied().fold(0.0, f64::max)})
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).unwrap_or_else(|| die("usage: qwen25-window-compare <dir> --a16 FILE --tokens FILE [...]".into()));
    let a16 = arg(&args, "--a16").unwrap_or_else(|| die("--a16 FILE is required".into()));
    let tokens_path = arg(&args, "--tokens").unwrap_or_else(|| die("--tokens FILE is required".into()));
    let window_len: usize = arg(&args, "--window-len").map(|v| v.parse().unwrap_or_else(|_| die(format!("bad --window-len {v}")))).unwrap_or(64);
    let tokens: Vec<usize> = serde_json::from_slice(&std::fs::read(tokens_path).unwrap_or_else(|e| die(format!("{tokens_path}: {e}"))))
        .unwrap_or_else(|e| die(format!("{tokens_path}: {e}")));
    let n = tokens.len();
    if n == 0 || window_len == 0 {
        die("an empty sequence or window".into());
    }
    let windows: Vec<usize> = match arg(&args, "--windows") {
        Some(list) => list.split(',').map(|v| v.parse().unwrap_or_else(|_| die(format!("bad window start {v}")))).collect(),
        // Start, middle and end of the sequence.
        None => vec![0, (n / 2).saturating_sub(window_len / 2), n.saturating_sub(window_len)],
    };
    if windows.iter().any(|w| w + window_len > n) {
        die(format!("a window runs past the {n}-token sequence"));
    }
    let keep = |pos: usize| windows.iter().any(|w| pos >= *w && pos < w + window_len);

    let cfg_bytes = std::fs::read(format!("{dir}/config.json")).unwrap_or_else(|e| die(format!("config.json: {e}")));
    let cfg: serde_json::Value = serde_json::from_slice(&cfg_bytes).unwrap_or_else(|e| die(format!("config.json: {e}")));
    let num = |k: &str| cfg.get(k).and_then(|v| v.as_u64()).unwrap_or_else(|| die(format!("config.json has no {k}"))) as usize;
    let heads = num("num_attention_heads");
    let ref_cfg = RefConfigV1 {
        n_layers: num("num_hidden_layers"),
        n_heads: heads,
        n_kv_heads: num("num_key_value_heads"),
        d_head: num("hidden_size") / heads,
        d_ff: num("intermediate_size"),
        vocab: num("vocab_size"),
        rms_eps: cfg.get("rms_norm_eps").and_then(|v| v.as_f64()).unwrap_or(1e-6) as f32,
        rope_theta: cfg.get("rope_theta").and_then(|v| v.as_f64()).unwrap_or(1e6) as f32,
    };
    let artifact = decode_artifact_file_v1(&std::fs::read(a16).unwrap_or_else(|e| die(format!("{a16}: {e}"))))
        .unwrap_or_else(|e| die(format!("{a16}: {e}")));
    let engine = A16Engine::new(&artifact).unwrap_or_else(|e| die(format!("the A16 engine refused the artifact: {e:?}")));
    let calibration = arg(&args, "--calibration-report").map(|path| {
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)
            .unwrap_or_else(|e| die(format!("{path}: {e}")))).unwrap_or_else(|e| die(format!("{path}: {e}")));
        if value["schema"] != "misaka-palw/a16-source-calibration/v1"
            || value["artifact_digest"] != artifact.artifact_digest().to_string()
            || value["n_ctx"].as_u64() != Some(artifact.shape.max_position as u64) {
            die("calibration report does not name this artifact and context".into());
        }
        let scale = value["logit_scale"].as_f64().filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or_else(|| die("calibration report has no finite positive physical logit scale".into()));
        (value, scale)
    });

    // The integer run: the whole prefix on one cache.
    let started = std::time::Instant::now();
    let mut cache = A16Cache::new(ref_cfg.n_layers);
    let mut integer: Vec<(usize, Vec<i32>)> = Vec::new();
    for (pos, tok) in tokens.iter().enumerate() {
        let logits = engine.forward_token(&mut cache, *tok, pos).unwrap_or_else(|e| die(format!("A16 forward at {pos}: {e:?}")));
        if keep(pos) {
            integer.push((pos, logits));
        }
        if pos % 1024 == 0 {
            eprintln!("[window-compare] integer {pos}/{n} ({:?})", started.elapsed());
        }
    }
    let integer_secs = started.elapsed().as_secs_f64();

    // The float reference over the same whole sequence.
    let blob = std::fs::read(format!("{dir}/model.safetensors")).unwrap_or_else(|e| die(format!("model.safetensors: {e}")));
    let started = std::time::Instant::now();
    let reference = reference_forward_windows_v1(&blob, &ref_cfg, &tokens, &keep).unwrap_or_else(|e| die(format!("reference: {e}")));
    let reference_secs = started.elapsed().as_secs_f64();
    drop(blob);

    let mut report = Vec::new();
    for start in &windows {
        let in_window = |pos: &usize| *pos >= *start && *pos < start + window_len;
        let r: Vec<Vec<f32>> = reference.iter().filter(|(p, _)| in_window(p)).map(|(_, l)| l.clone()).collect();
        let q: Vec<Vec<i32>> = integer.iter().filter(|(p, _)| in_window(p)).map(|(_, l)| l.clone()).collect();
        let full = score_fidelity(&r, &q);
        let mut entry = serde_json::json!({
            "window_start": start,
            "window_len": window_len,
            "positions": full.positions,
            "top1_agree": full.top1_agree,
            "top5_contains": full.top5_contains,
            "top100_rank_correlation": full.top100_rank_correlation,
        });
        if let Some((_, scale)) = &calibration {
            entry["kl"] = kl_summary_v1(&r, &q, *scale);
        }
        if args.iter().any(|a| a == "--control") {
            // The same window, its prefix cut off: the integer engine restarted at the window.
            let mut cut = A16Cache::new(ref_cfg.n_layers);
            let control: Vec<Vec<i32>> = (*start..start + window_len)
                .map(|pos| engine.forward_token(&mut cut, tokens[pos], pos - start).unwrap_or_else(|e| die(format!("control: {e:?}"))))
                .collect();
            let c = score_fidelity(&r, &control);
            entry["control_prefix_cut"] = serde_json::json!({
                "top1_agree": c.top1_agree,
                "top5_contains": c.top5_contains,
                "top100_rank_correlation": c.top100_rank_correlation,
            });
            if let Some((_, scale)) = &calibration {
                entry["control_prefix_cut"]["kl"] = kl_summary_v1(&r, &control, *scale);
            }
        }
        report.push(entry);
    }
    let out = serde_json::json!({
        "tool": "qwen25-window-compare",
        "sequence_tokens": n,
        "a16_artifact_digest": artifact.artifact_digest().to_string(),
        "calibration": calibration.map(|(value, _)| value),
        "integer_seconds": integer_secs,
        "reference_seconds": reference_secs,
        "windows": report,
        "note": "whole prefix advanced by both engines; logits kept only inside the windows; a measurement, not a PASS",
    });
    let text = serde_json::to_string_pretty(&out).unwrap();
    match arg(&args, "--out") {
        Some(path) => std::fs::write(path, &text).unwrap_or_else(|e| die(format!("{path}: {e}"))),
        None => println!("{text}"),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_logit_scale_and_stable_kl_detect_a_wrong_distribution() {
        assert!(logit_kl_v1(&[1.0, -1.0], &[2, -2], 0.5).unwrap() < 1e-12);
        assert!(logit_kl_v1(&[1.0, -1.0], &[-2, 2], 0.5).unwrap() > 1.0);
        assert!(logit_kl_v1(&[-1000.0, -1002.0], &[0, -2], 1.0).unwrap() < 1e-12);
    }
    #[test]
    fn incomplete_or_non_finite_rows_never_produce_a_score() {
        assert!(logit_kl_v1(&[], &[], 1.0).is_err());
        assert!(logit_kl_v1(&[1.0], &[1, 2], 1.0).is_err());
        assert!(logit_kl_v1(&[f32::NAN], &[1], 1.0).is_err());
        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(logit_kl_v1(&[1.0], &[1], scale).is_err());
        }
    }
}
