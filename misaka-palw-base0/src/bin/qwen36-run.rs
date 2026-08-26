//! Run a `PALW-QWEN36` artifact: open it, prefill a prompt, decode greedily, report.
//!
//! ```text
//! qwen36-run --artifact <file.palwq36> [--tokens "9707,11,1879"] [--generate N]
//! ```
//!
//! The artifact is memory-mapped, so opening a 33 GiB class costs the header. The resident set is
//! whatever the mixture actually touched — eight experts of 256 per layer per token — which is the
//! property that lets a machine with less RAM than the model produce a block on it.
//!
//! Token ids rather than text: a Qwen3.6 tokenizer is a separate piece, and what is being measured
//! here is the engine.

use misaka_palw_base0::engine::argmax_lowest;
use misaka_palw_base0::qwen36::{Qwen36Cache, Qwen36Engine, open_artifact};
use misaka_palw_base0::qwen36_reference::qwen36_score_fidelity;

fn die(message: String) -> ! {
    eprintln!("qwen36-run: {message}");
    std::process::exit(1)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path =
        flag(&args, "--artifact").unwrap_or_else(|| die("usage: qwen36-run --artifact <file> [--tokens ids] [--generate N]".into()));
    let tokens: Vec<usize> = flag(&args, "--tokens")
        .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![9707, 11, 1879, 0, 3555, 374]);
    let generate: usize = flag(&args, "--generate").and_then(|v| v.parse().ok()).unwrap_or(8);

    let started = std::time::Instant::now();
    let artifact = open_artifact(std::path::Path::new(path)).unwrap_or_else(|e| die(format!("{path}: {e}")));
    let shape = &artifact.shape;
    println!(
        "opened in {:?}: {} layers ({} linear, {} full), {:.2} GiB of weights, vocab {}",
        started.elapsed(),
        shape.n_layers(),
        shape.layer_types.iter().filter(|k| **k == misaka_palw_base0::qwen36::Qwen36LayerKind::LinearAttention).count(),
        shape.layer_types.iter().filter(|k| **k == misaka_palw_base0::qwen36::Qwen36LayerKind::FullAttention).count(),
        artifact.weight_bytes() as f64 / (1u64 << 30) as f64,
        shape.vocab
    );

    let engine = Qwen36Engine::new(&artifact);
    let mut cache = Qwen36Cache::new(shape);

    let prefill_started = std::time::Instant::now();
    let mut logits = Vec::new();
    for (position, token) in tokens.iter().enumerate() {
        logits = engine.forward_token(&mut cache, *token, position).unwrap_or_else(|e| die(format!("prefill at {position}: {e}")));
    }
    let prefill = prefill_started.elapsed();

    let decode_started = std::time::Instant::now();
    let mut produced = Vec::new();
    for step in 0..generate {
        let next = argmax_lowest(&logits);
        produced.push(next);
        let position = tokens.len() + step;
        if position + 1 >= shape.max_position {
            break;
        }
        logits = engine.forward_token(&mut cache, next, position).unwrap_or_else(|e| die(format!("decode at {position}: {e}")));
    }
    let decode = decode_started.elapsed();

    // **The fidelity check.** A calibrated class is one whose logits rank the same tokens the
    // unquantized model does, and only a comparison against the reference says whether they do.
    if let Some(path) = flag(&args, "--reference") {
        let bytes = std::fs::read(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
        let mut i = 0usize;
        let u64_at = |i: &mut usize| -> u64 {
            let v = u64::from_le_bytes(bytes[*i..*i + 8].try_into().expect("8"));
            *i += 8;
            v
        };
        let rows = u64_at(&mut i) as usize;
        let mut reference = Vec::with_capacity(rows);
        for _ in 0..rows {
            let n = u64_at(&mut i) as usize;
            reference
                .push(bytes[i..i + n * 4].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().expect("4"))).collect::<Vec<f32>>());
            i += n * 4;
        }
        // Re-run the prompt keeping every row, since the decode loop above kept only the last.
        let mut fresh = Qwen36Cache::new(shape);
        let mut integer = Vec::with_capacity(tokens.len());
        for (position, token) in tokens.iter().enumerate() {
            integer.push(
                engine.forward_token(&mut fresh, *token, position).unwrap_or_else(|e| die(format!("scoring at {position}: {e}"))),
            );
        }
        let scored = qwen36_score_fidelity(&reference, &integer);
        println!("fidelity vs the f32 reference");
        println!("  top-1 agree     {}/{}", scored.top1_agree, scored.positions);
        println!("  top-5 contains  {}/{}", scored.top5_contains, scored.positions);
        println!("  rank corr (100) {:.4}", scored.rank_correlation);
        println!("  cosine          {:.4}", scored.cosine);
        println!("  FAITHFUL        {}", scored.is_faithful());
    }

    // **Magnitude, site by site.** A scale error is invisible in the logits — it looks like a
    // different model — and shows immediately as a stream whose peak is a factor away from the
    // reference's at the same place.
    if let Some(path) = flag(&args, "--sites") {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
        let mut reference: std::collections::BTreeMap<&str, (f64, i32)> = std::collections::BTreeMap::new();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() == 4 {
                reference.insert(f[0], (f[1].parse().unwrap_or(0.0), f[3].parse().unwrap_or(0)));
            }
        }
        // Across ALL positions, because the reference's absmax is: comparing one position's peak
        // against every position's makes a healthy site look crushed.
        let mut fresh = Qwen36Cache::new(shape);
        let mut probe: Vec<(String, i32)> = Vec::new();
        for (position, token) in tokens.iter().enumerate() {
            let (_, p) = engine.forward_token_peaks(&mut fresh, *token, position).unwrap_or_else(|e| die(format!("probe: {e}")));
            if probe.is_empty() {
                probe = p;
            } else {
                for (slot, (_, peak)) in probe.iter_mut().zip(&p) {
                    slot.1 = slot.1.max(*peak);
                }
            }
        }
        println!("site                                   int-peak  ref-absmax    e   int/ref");
        for (name, peak) in &probe {
            let (absmax, e) = reference.get(name.as_str()).copied().unwrap_or((0.0, 0));
            let value = *peak as f64 / 2f64.powi(e);
            let ratio = if absmax > 0.0 { value / absmax } else { 0.0 };
            println!("{name:38} {peak:8}  {absmax:.3e}  {e:3}   {ratio:.3}");
        }
    }

    // **Direction, site by site.** A cosine that is one at a site and halved at the next names
    // the stage whose computation is wrong; a magnitude cannot, because a wrong function of the
    // right size looks healthy.
    if let Some(path) = flag(&args, "--rows") {
        let bytes = std::fs::read(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
        let mut i = 0usize;
        let u64_at = |i: &mut usize| -> u64 {
            let v = u64::from_le_bytes(bytes[*i..*i + 8].try_into().expect("8"));
            *i += 8;
            v
        };
        let n = u64_at(&mut i) as usize;
        let mut reference: std::collections::BTreeMap<String, Vec<f32>> = std::collections::BTreeMap::new();
        for _ in 0..n {
            let len = u64_at(&mut i) as usize;
            let name = String::from_utf8_lossy(&bytes[i..i + len]).into_owned();
            i += len;
            let m = u64_at(&mut i) as usize;
            let row = bytes[i..i + m * 4].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().expect("4"))).collect();
            i += m * 4;
            reference.insert(name, row);
        }
        let mut fresh = Qwen36Cache::new(shape);
        let mut rows: Vec<(String, Vec<i32>)> = Vec::new();
        for (position, token) in tokens.iter().enumerate() {
            let (_, r) = engine.forward_token_probed(&mut fresh, *token, position).unwrap_or_else(|e| die(format!("rows: {e}")));
            rows = r;
            let _ = position;
        }
        println!("site                                    cosine   len");
        for (name, row) in &rows {
            let Some(r) = reference.get(name) else { continue };
            if r.len() != row.len() {
                println!("{name:38} LEN {} vs {}", row.len(), r.len());
                continue;
            }
            let (mut dot, mut na, mut nb) = (0f64, 0f64, 0f64);
            for (x, y) in r.iter().zip(row) {
                let (x, y) = (*x as f64, *y as f64);
                dot += x * y;
                na += x * x;
                nb += y * y;
            }
            let cos = if na > 0.0 && nb > 0.0 { dot / (na.sqrt() * nb.sqrt()) } else { 0.0 };
            println!("{name:38} {cos:8.4}  {}", row.len());
        }
    }

    let filled = cache.gdn.iter().flatten().filter(|s| s.s.iter().any(|v| *v != 0)).count();
    let heads: usize = cache.gdn.iter().map(|l| l.len()).sum();
    println!("prefill {} tok in {prefill:?} ({:.1} ms/tok)", tokens.len(), prefill.as_secs_f64() * 1e3 / tokens.len() as f64);
    println!("decode  {} tok in {decode:?} ({:.2} tok/s)", produced.len(), produced.len() as f64 / decode.as_secs_f64());
    println!("  gdn heads with state  {filled}/{heads}");
    println!("  logits nonzero        {}/{}", logits.iter().filter(|v| **v != 0).count(), logits.len());
    println!("  produced token ids    {produced:?}");
}
