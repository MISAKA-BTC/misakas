//! **RFC-0002/0013: the legacy PALWQ361 class (#8) compared with an f32 reference over a LONG sequence, by window.**
//!
//! Modelled on `qwen25-window-compare` (the A16 tool). The difference is where the reference comes from: the f32 forward of
//! the 35B-A3B MoE over 32K positions is not computed here (it is a full read of the source GGUF and hours of compute), so the
//! reference logits arrive as a FILE written by whatever host ran the f32 model over the same whole sequence. This tool binds
//! that file to the sequence (its SHA-256) and to the window starts, advances the integer engine over the WHOLE prefix on one
//! cache, keeps logits only inside the windows, and scores each window.
//!
//! ```text
//! qwen36-window-compare --artifact FILE.palwq36 --tokens FILE --reference-windows FILE
//!     [--logit-scale S] [--control] [--resident-gib G | --resident page-cache] [--out report.json]
//! ```
//!
//! `--reference-windows` layout: `u32 LE header length`, a JSON header
//! `{"schema":"misaka-palw/qwen36-reference-windows/v1","tokens_sha256":hex,"sequence_tokens":n,"window_len":64,
//!   "window_starts":[...],"vocab":V,"producer":"..."}`, then `window_starts.len() * window_len * vocab` f32 LE values in window
//! order. A header that names another sequence, a short body, or a non-finite value is refused: a reference that does not
//! describe this sequence is not a measurement.
//!
//! KL needs the integer logits in physical units. PALWQ361 has no calibrated logit scale on record; without `--logit-scale` the
//! report carries no `kl` and the pipeline's reader refuses it (typed missing, never zero). Nothing here is a PASS.
//!
//! Every window also reports `logit_scale_spread`, the physical size of one code implied by the window's own spread
//! (`logit_scale_spread_v1`). The R7 estimate mode of `genesis10_pipeline.py` (user decision 2026-10-22) refuses an estimated
//! scale that this disagrees with by more than its bound; the measure never replaces the scale the KL is computed at.
use misaka_palw_base0::qwen36::{Qwen36Cache, Qwen36Engine, Qwen36ResidencyPolicyV1, open_artifact_with_residency};
use misaka_palw_base0::qwen36_reference::qwen36_score_fidelity;
use sha2::{Digest, Sha256};

pub const REFERENCE_WINDOWS_SCHEMA: &str = "misaka-palw/qwen36-reference-windows/v1";

fn die(message: String) -> ! {
    eprintln!("qwen36-window-compare: {message}");
    std::process::exit(2)
}

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The reference windows file, checked against the sequence it claims to describe.
#[derive(Debug)]
struct ReferenceWindows {
    header: serde_json::Value,
    window_len: usize,
    starts: Vec<usize>,
    /// `rows[w][p]` = the reference logits at absolute position `starts[w] + p`.
    rows: Vec<Vec<Vec<f32>>>,
}

fn parse_reference_windows(bytes: &[u8], tokens_sha256: &str, n: usize) -> Result<ReferenceWindows, String> {
    if bytes.len() < 4 {
        return Err("reference file shorter than its header length".into());
    }
    let hlen = u32::from_le_bytes(bytes[..4].try_into().expect("4")) as usize;
    let body_at = 4usize.checked_add(hlen).filter(|e| *e <= bytes.len()).ok_or("reference header runs past the file")?;
    let header: serde_json::Value = serde_json::from_slice(&bytes[4..body_at]).map_err(|e| format!("reference header: {e}"))?;
    if header["schema"] != REFERENCE_WINDOWS_SCHEMA {
        return Err(format!("reference schema is not {REFERENCE_WINDOWS_SCHEMA}"));
    }
    if header["tokens_sha256"].as_str() != Some(tokens_sha256) || header["sequence_tokens"].as_u64() != Some(n as u64) {
        return Err("the reference names another token sequence".into());
    }
    let window_len = header["window_len"].as_u64().filter(|v| *v > 0).ok_or("reference has no window_len")? as usize;
    let vocab = header["vocab"].as_u64().filter(|v| *v > 0).ok_or("reference has no vocab")? as usize;
    let starts: Vec<usize> = header["window_starts"]
        .as_array()
        .ok_or("reference has no window_starts")?
        .iter()
        .map(|v| v.as_u64().map(|v| v as usize).ok_or("bad window start"))
        .collect::<Result<_, _>>()?;
    if starts.is_empty() || starts.windows(2).any(|w| w[0] >= w[1]) || starts.iter().any(|s| s + window_len > n) {
        return Err("reference window starts are empty, unsorted, duplicated or past the sequence".into());
    }
    let want =
        starts.len().checked_mul(window_len).and_then(|v| v.checked_mul(vocab)).and_then(|v| v.checked_mul(4)).ok_or("overflow")?;
    let body = &bytes[body_at..];
    if body.len() != want {
        return Err(format!("reference body is {} bytes, the header says {want}", body.len()));
    }
    let mut values = body.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().expect("4")));
    let mut rows = Vec::with_capacity(starts.len());
    for _ in &starts {
        let mut window = Vec::with_capacity(window_len);
        for _ in 0..window_len {
            let row: Vec<f32> = values.by_ref().take(vocab).collect();
            if row.iter().any(|v| !v.is_finite()) {
                return Err("non-finite reference logit".into());
            }
            window.push(row);
        }
        rows.push(window);
    }
    Ok(ReferenceWindows { header, window_len, starts, rows })
}

/// KL(reference || integer) in physical logit units, with stable log-sum-exp (the A16 tool's measure).
fn logit_kl_v1(reference: &[f32], integer: &[i32], scale: f64) -> Result<f64, &'static str> {
    if reference.is_empty() || reference.len() != integer.len() || !scale.is_finite() || scale <= 0.0 {
        return Err("invalid logit dimensions or scale");
    }
    let r: Vec<f64> = reference.iter().map(|v| f64::from(*v)).collect();
    let q: Vec<f64> = integer.iter().map(|v| f64::from(*v) * scale).collect();
    if r.iter().chain(&q).any(|v| !v.is_finite()) {
        return Err("non-finite logit");
    }
    let rm = r.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let qm = q.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let rz = r.iter().map(|v| (v - rm).exp()).sum::<f64>().ln();
    let qz = q.iter().map(|v| (v - qm).exp()).sum::<f64>().ln();
    let kl: f64 = r
        .iter()
        .zip(&q)
        .map(|(r, q)| {
            let rp = r - rm - rz;
            rp.exp() * (rp - (q - qm - qz))
        })
        .sum();
    if !kl.is_finite() {
        return Err("non-finite divergence");
    }
    Ok(kl.max(0.0))
}

/// **The physical size of one integer logit code, as the window's own spread says it is** (R7, 2026-10-22).
///
/// `sqrt(Σ (r − r̄)² / Σ (q − q̄)²)` pooled over the window's rows, each row centred on its own mean (logits are
/// shift-invariant). PALWQ361 writes `code = logit · 2^e_logits` (`qwen36_calibrate::projection_params`), so for a class that
/// computes the model this lands near `2^-e_logits`; a logit scale given to this tool that is a power of two away from it
/// is either a wrong scale or a head that is a power of two off — both are refusals for the reader, never a pass. It is
/// reported, not used: the scale the KL is computed at is still the one given (`--logit-scale`).
fn logit_scale_spread_v1(reference: &[Vec<f32>], integer: &[Vec<i32>]) -> Option<f64> {
    if reference.is_empty() || reference.len() != integer.len() {
        return None;
    }
    let (mut rr, mut qq) = (0f64, 0f64);
    for (r, q) in reference.iter().zip(integer) {
        if r.is_empty() || r.len() != q.len() {
            return None;
        }
        let rm = r.iter().map(|v| f64::from(*v)).sum::<f64>() / r.len() as f64;
        let qm = q.iter().map(|v| f64::from(*v)).sum::<f64>() / q.len() as f64;
        rr += r.iter().map(|v| (f64::from(*v) - rm).powi(2)).sum::<f64>();
        qq += q.iter().map(|v| (f64::from(*v) - qm).powi(2)).sum::<f64>();
    }
    let spread = (rr / qq).sqrt();
    (qq > 0.0 && spread.is_finite() && spread > 0.0).then_some(spread)
}

fn kl_summary_v1(reference: &[Vec<f32>], integer: &[Vec<i32>], scale: f64) -> serde_json::Value {
    if reference.is_empty() || reference.len() != integer.len() {
        die("incomplete comparison window".into());
    }
    let values: Vec<f64> =
        reference.iter().zip(integer).map(|(r, q)| logit_kl_v1(r, q, scale).unwrap_or_else(|why| die(why.into()))).collect();
    serde_json::json!({"mean": values.iter().sum::<f64>() / values.len() as f64, "max": values.iter().copied().fold(0.0, f64::max)})
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = arg(&args, "--artifact").unwrap_or_else(|| die("--artifact FILE is required".into()));
    let tokens_path = arg(&args, "--tokens").unwrap_or_else(|| die("--tokens FILE is required".into()));
    let reference_path = arg(&args, "--reference-windows").unwrap_or_else(|| die("--reference-windows FILE is required".into()));
    let scale: Option<f64> = arg(&args, "--logit-scale")
        .map(|v| v.parse::<f64>().ok().filter(|s| s.is_finite() && *s > 0.0).unwrap_or_else(|| die(format!("bad --logit-scale {v}"))));
    let token_bytes = std::fs::read(tokens_path).unwrap_or_else(|e| die(format!("{tokens_path}: {e}")));
    let tokens: Vec<usize> = serde_json::from_slice(&token_bytes).unwrap_or_else(|e| die(format!("{tokens_path}: {e}")));
    let n = tokens.len();
    if n == 0 {
        die("an empty sequence".into());
    }
    let tokens_sha256 = hex(&Sha256::digest(&token_bytes));
    let reference = parse_reference_windows(
        &std::fs::read(reference_path).unwrap_or_else(|e| die(format!("{reference_path}: {e}"))),
        &tokens_sha256,
        n,
    )
    .unwrap_or_else(|e| die(format!("{reference_path}: {e}")));
    let window_len = reference.window_len;
    let keep = |pos: usize| reference.starts.iter().any(|w| pos >= *w && pos < w + window_len);

    let policy = match (arg(&args, "--resident-gib").and_then(|v| v.parse::<f64>().ok()), arg(&args, "--resident")) {
        (Some(gib), _) if gib > 0.0 => Qwen36ResidencyPolicyV1::Bytes((gib * (1u64 << 30) as f64) as u64),
        (_, Some("page-cache")) => Qwen36ResidencyPolicyV1::PageCache,
        (_, Some(other)) => die(format!("--resident {other}: say page-cache, or give --resident-gib")),
        _ => Qwen36ResidencyPolicyV1::FifthOfTheWeights,
    };
    let artifact = open_artifact_with_residency(std::path::Path::new(path), policy).unwrap_or_else(|e| die(format!("{path}: {e}")));
    let shape = &artifact.shape;
    if n > shape.max_position {
        die(format!("the {n}-token sequence exceeds the artifact's {} positions", shape.max_position));
    }
    if reference.header["vocab"].as_u64() != Some(shape.vocab as u64) {
        die("the reference vocabulary is not the artifact's".into());
    }
    let engine = Qwen36Engine::new(&artifact);

    // The integer run: the whole prefix on one cache.
    let started = std::time::Instant::now();
    let mut cache = Qwen36Cache::new(shape);
    let mut integer: Vec<(usize, Vec<i32>)> = Vec::new();
    for (pos, tok) in tokens.iter().enumerate() {
        let logits = engine.forward_token(&mut cache, *tok, pos).unwrap_or_else(|e| die(format!("forward at {pos}: {e}")));
        if keep(pos) {
            integer.push((pos, logits));
        }
        if pos % 1024 == 0 {
            eprintln!("[qwen36-window-compare] integer {pos}/{n} ({:?})", started.elapsed());
        }
    }
    let integer_secs = started.elapsed().as_secs_f64();

    let mut report = Vec::new();
    for (w, start) in reference.starts.iter().enumerate() {
        let r = &reference.rows[w];
        let q: Vec<Vec<i32>> =
            integer.iter().filter(|(p, _)| *p >= *start && *p < start + window_len).map(|(_, l)| l.clone()).collect();
        let full = qwen36_score_fidelity(r, &q);
        let mut entry = serde_json::json!({
            "window_start": start, "window_len": window_len, "positions": full.positions,
            "top1_agree": full.top1_agree, "top5_contains": full.top5_contains,
            "top100_rank_correlation": full.rank_correlation, "cosine": full.cosine,
            "logit_scale_spread": logit_scale_spread_v1(r, &q),
        });
        if let Some(scale) = scale {
            entry["kl"] = kl_summary_v1(r, &q, scale);
        }
        if args.iter().any(|a| a == "--control") {
            let mut cut = Qwen36Cache::new(shape);
            let control: Vec<Vec<i32>> = (*start..start + window_len)
                .map(|pos| engine.forward_token(&mut cut, tokens[pos], pos - start).unwrap_or_else(|e| die(format!("control: {e}"))))
                .collect();
            let c = qwen36_score_fidelity(r, &control);
            entry["control_prefix_cut"] = serde_json::json!({
                "top1_agree": c.top1_agree, "top5_contains": c.top5_contains, "top100_rank_correlation": c.rank_correlation,
            });
            if let Some(scale) = scale {
                entry["control_prefix_cut"]["kl"] = kl_summary_v1(r, &control, scale);
            }
        }
        report.push(entry);
    }
    let out = serde_json::json!({
        "tool": "qwen36-window-compare",
        "sequence_tokens": n,
        "tokens_sha256": tokens_sha256,
        "palwq36_artifact_root": artifact.artifact_root().to_string(),
        "logit_scale": scale,
        "reference": reference.header,
        "integer_seconds": integer_secs,
        "windows": report,
        "note": "whole prefix advanced by the integer engine; reference windows supplied by an f32 host; a measurement, not a PASS",
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

    fn file(header: serde_json::Value, values: &[f32]) -> Vec<u8> {
        let h = serde_json::to_vec(&header).unwrap();
        let mut out = (h.len() as u32).to_le_bytes().to_vec();
        out.extend(h);
        for v in values {
            out.extend(v.to_le_bytes());
        }
        out
    }

    fn header(sha: &str) -> serde_json::Value {
        serde_json::json!({"schema": REFERENCE_WINDOWS_SCHEMA, "tokens_sha256": sha, "sequence_tokens": 8,
            "window_len": 2, "window_starts": [0, 6], "vocab": 3})
    }

    #[test]
    fn qwen36_reference_windows_bind_the_sequence_and_shape() {
        let ok = parse_reference_windows(&file(header("ab"), &[0.0; 12]), "ab", 8).unwrap();
        assert_eq!(ok.starts, vec![0, 6]);
        assert_eq!(ok.rows[1][1].len(), 3);
        assert!(parse_reference_windows(&file(header("ab"), &[0.0; 12]), "cd", 8).is_err());
        assert!(parse_reference_windows(&file(header("ab"), &[0.0; 11]), "ab", 8).is_err());
        let mut bad = [0.0f32; 12];
        bad[5] = f32::NAN;
        assert!(parse_reference_windows(&file(header("ab"), &bad), "ab", 8).is_err());
        let mut past = header("ab");
        past["window_starts"] = serde_json::json!([0, 7]);
        assert!(parse_reference_windows(&file(past, &[0.0; 12]), "ab", 8).is_err());
    }

    #[test]
    fn qwen36_kl_refuses_a_missing_or_bad_scale() {
        assert!(logit_kl_v1(&[1.0, -1.0], &[2, -2], 0.5).unwrap() < 1e-12);
        for scale in [0.0, -1.0, f64::NAN] {
            assert!(logit_kl_v1(&[1.0], &[1], scale).is_err());
        }
    }

    #[test]
    fn qwen36_logit_spread_names_the_power_of_two_and_refuses_a_flat_row() {
        // codes = logits · 2^9 (+ a per-row shift the measure must ignore): the spread is 2^-9.
        let reference = vec![vec![3.0f32, -1.0, 0.5, 7.25], vec![-2.0f32, 4.0, 1.0, 0.0]];
        let integer: Vec<Vec<i32>> =
            reference.iter().enumerate().map(|(i, r)| r.iter().map(|v| (v * 512.0) as i32 + 1000 * i as i32).collect()).collect();
        let spread = logit_scale_spread_v1(&reference, &integer).unwrap();
        assert!((spread.log2() + 9.0).abs() < 1e-9, "{spread}");
        // A head a power of two off moves the spread by exactly that power.
        let doubled: Vec<Vec<i32>> = integer.iter().map(|r| r.iter().map(|v| v * 2).collect()).collect();
        assert!((logit_scale_spread_v1(&reference, &doubled).unwrap().log2() + 10.0).abs() < 1e-9);
        assert!(logit_scale_spread_v1(&reference, &[vec![5; 4], vec![5; 4]]).is_none(), "a constant integer row has no scale");
        assert!(logit_scale_spread_v1(&reference, &integer[..1]).is_none());
        assert!(logit_scale_spread_v1(&[], &[]).is_none());
    }
}
