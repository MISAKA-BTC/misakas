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

    let filled = cache.gdn.iter().flatten().filter(|s| s.s.iter().any(|v| *v != 0)).count();
    let heads: usize = cache.gdn.iter().map(|l| l.len()).sum();
    println!("prefill {} tok in {prefill:?} ({:.1} ms/tok)", tokens.len(), prefill.as_secs_f64() * 1e3 / tokens.len() as f64);
    println!("decode  {} tok in {decode:?} ({:.2} tok/s)", produced.len(), produced.len() as f64 / decode.as_secs_f64());
    println!("  gdn heads with state  {filled}/{heads}");
    println!("  logits nonzero        {}/{}", logits.iter().filter(|v| **v != 0).count(), logits.len());
    println!("  produced token ids    {produced:?}");
}
