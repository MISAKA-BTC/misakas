//! **`base0-chat` — text in, text out, on the integer engine.**
//!
//! This is the binary that decides whether BASE-0 is a runtime or a forward pass with a test
//! harness. Everything before it measured the engine against a float reference on token ids; this
//! one takes a sentence, tokenizes it, renders Qwen's chat template, prefills, decodes greedily
//! and prints what the model said.
//!
//! ```text
//! base0-chat --artifact qwen25-1.5b-a16.palwart --tokenizer tokenizer.json \
//!            --prompt "What is the capital of Japan?" [--system S] [--max-tokens N] [--raw]
//! ```
//!
//! # Greedy, and deliberately only greedy
//!
//! There is no temperature, no top-p and no seed. PALW's verification target is the canonical
//! logit row and the argmax over it (lowest id on ties); a sampler belongs to an application
//! sitting on top of this, not to the runtime whose output a court has to reproduce. A `--raw`
//! flag skips the chat template for the same reason: what is being exercised is the model, and a
//! template is a per-class string contract rather than part of the computation.

use misaka_palw_base0::artifact::decode_artifact_file_v1;
use misaka_palw_base0::engine_a16::{A16Cache, A16Engine};
use misaka_palw_base0::tokenizer::{QwenTokenizer, qwen_chat_prompt};
use std::io::Write;

fn die(message: String) -> ! {
    eprintln!("base0-chat: {message}");
    std::process::exit(1)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let artifact_path = flag(&args, "--artifact")
        .unwrap_or_else(|| die("usage: base0-chat --artifact <file> --tokenizer <file> --prompt <text>".into()));
    let tokenizer_path = flag(&args, "--tokenizer").unwrap_or_else(|| die("--tokenizer <tokenizer.json> is required".into()));
    let prompt = flag(&args, "--prompt").unwrap_or_else(|| die("--prompt <text> is required".into()));
    let system = flag(&args, "--system");
    let max_tokens: usize = flag(&args, "--max-tokens").and_then(|v| v.parse().ok()).unwrap_or(128);
    let raw = args.iter().any(|a| a == "--raw");

    let started = std::time::Instant::now();
    let bytes = std::fs::read(artifact_path).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    let artifact = decode_artifact_file_v1(&bytes).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    let load = started.elapsed();
    drop(bytes);

    let tokenizer_bytes = std::fs::read(tokenizer_path).unwrap_or_else(|e| die(format!("{tokenizer_path}: {e}")));
    let tokenizer = QwenTokenizer::from_json(&tokenizer_bytes).unwrap_or_else(|e| die(format!("{tokenizer_path}: {e}")));

    let engine = A16Engine::new(&artifact).unwrap_or_else(|e| die(format!("the artifact is not an A16 class: {e:?}")));
    let shape = &artifact.shape;

    let text = if raw { prompt.to_string() } else { qwen_chat_prompt(system, &[("user", prompt)]) };
    let ids = tokenizer.encode(&text).unwrap_or_else(|e| die(format!("tokenizing: {e}")));
    if ids.len() >= shape.max_position {
        die(format!("the prompt is {} tokens and the artifact's rotary table covers {}", ids.len(), shape.max_position));
    }

    // The stop token: `<|im_end|>` under the chat template, `<|endoftext|>` when raw. Looked up
    // by content rather than hardcoded, because the id is a property of the tokenizer file.
    let stop = if raw { tokenizer.added_id("<|endoftext|>") } else { tokenizer.added_id("<|im_end|>") };

    eprintln!(
        "artifact  {} ({} MiB, {} layers, vocab {})",
        artifact.artifact_digest(),
        (bytes_of(&artifact)) >> 20,
        shape.n_layers,
        shape.vocab
    );
    eprintln!("loaded in {load:?}, prompt {} tokens", ids.len());

    let mut cache = A16Cache::new(shape.n_layers);
    let mut logits = Vec::new();

    // **Prefill.** Every prompt token through the forward pass, keeping only the last row: the
    // earlier rows predict tokens the prompt already contains.
    let prefill_started = std::time::Instant::now();
    for (position, token) in ids.iter().enumerate() {
        logits = engine
            .forward_token(&mut cache, *token as usize, position)
            .unwrap_or_else(|e| die(format!("prefill failed at position {position}: {e:?}")));
    }
    let prefill = prefill_started.elapsed();

    // **Decode.** Greedy, one token at a time, streamed. The text is re-decoded from the whole
    // generated run each step rather than per token: a multi-byte character can straddle two
    // tokens, and decoding each one alone would print a replacement character where a kanji
    // belongs.
    let decode_started = std::time::Instant::now();
    let mut generated: Vec<u32> = Vec::new();
    let mut shown = 0usize;
    let mut stopped = "length";
    for step in 0..max_tokens {
        let next = misaka_palw_base0::engine::argmax_lowest(&logits) as u32;
        if Some(next) == stop {
            stopped = "stop token";
            break;
        }
        generated.push(next);
        let text = tokenizer.decode_lossy_tail(&generated);
        if text.len() > shown {
            print!("{}", &text[shown..]);
            let _ = std::io::stdout().flush();
            shown = text.len();
        }
        let position = ids.len() + step;
        if position + 1 >= shape.max_position {
            stopped = "context";
            break;
        }
        logits = engine
            .forward_token(&mut cache, next as usize, position)
            .unwrap_or_else(|e| die(format!("decode failed at position {position}: {e:?}")));
    }
    let decode = decode_started.elapsed();
    println!();

    let per_prefill = prefill.as_secs_f64() / ids.len().max(1) as f64;
    let per_decode = decode.as_secs_f64() / generated.len().max(1) as f64;
    eprintln!(
        "prefill {} tok in {:?} ({:.1} tok/s) | decode {} tok in {:?} ({:.1} tok/s) | stopped on {stopped}",
        ids.len(),
        prefill,
        1.0 / per_prefill,
        generated.len(),
        decode,
        1.0 / per_decode
    );
}

fn bytes_of(a: &misaka_palw_base0::artifact::Base0ArtifactV1) -> usize {
    a.embed.len()
        + a.unembed.len()
        + a.layers
            .iter()
            .map(|l| l.wq.len() + l.wk.len() + l.wv.len() + l.wo.len() + l.w_gate.len() + l.w_up.len() + l.w_down.len())
            .sum::<usize>()
}
