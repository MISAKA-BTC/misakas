//! **`qwen36-chat` — text in, text out, on the Qwen3.6 integer engine.**
//!
//! ```text
//! qwen36-chat --artifact q36.palwq36 --gguf <header-or-file> --prompt "..." [--system S]
//!             [--max-tokens N] [--raw] [--think] [--show-ids]
//! ```
//!
//! # Where the tokenizer comes from
//!
//! This checkpoint's repository ships no `tokenizer.json`: the vocabulary, the merge table and the
//! token types live in the GGUF header, which is where llama.cpp reads them from as well. So
//! `--gguf` may be the whole 24 GiB checkpoint or just its first few tens of megabytes — only the
//! header is parsed, and the file is not otherwise touched.
//!
//! The tokenizer is deliberately NOT part of the artifact. PALW binds a prompt by the hash of its
//! token ids, which puts tokenization outside the computation a court reproduces; an artifact that
//! carried a vocabulary would be committing to a string contract it does not evaluate.
//!
//! # Greedy, and deliberately only greedy
//!
//! No temperature, no top-p, no seed. The verification target is the canonical logit row and the
//! argmax over it, lowest id on ties. A sampler belongs to the application above this.
//!
//! `--show-ids` prints the prompt's ids and their round-trip and stops before the model runs,
//! which is how the tokenizer is checked without a 33 GiB artifact in hand.

use misaka_palw_base0::engine::argmax_lowest;
use misaka_palw_base0::gguf::parse_directory;
use misaka_palw_base0::qwen36::{Qwen36Cache, Qwen36Engine, open_artifact};
use misaka_palw_base0::tokenizer::QwenTokenizer;
use std::io::{Read, Write};

fn die(message: String) -> ! {
    eprintln!("qwen36-chat: {message}");
    std::process::exit(1)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

/// Qwen3.6's generation prompt. The template's own tail, reduced to the two branches a chat
/// driver needs: with thinking the assistant turn opens `<think>\n`, without it the empty pair is
/// supplied so the model starts on its answer.
fn chat_prompt(system: Option<&str>, user: &str, thinking: bool) -> String {
    let mut out = String::new();
    if let Some(system) = system {
        out.push_str("<|im_start|>system\n");
        out.push_str(system);
        out.push_str("<|im_end|>\n");
    }
    out.push_str("<|im_start|>user\n");
    out.push_str(user);
    out.push_str("<|im_end|>\n<|im_start|>assistant\n");
    out.push_str(if thinking { "<think>\n" } else { "<think>\n\n</think>\n\n" });
    out
}

/// The GGUF header, without reading the body. The directory is at the front and the tokenizer
/// arrays are inside it, so a bounded prefix is enough — grown until the parse stops complaining
/// about truncation rather than guessed at, because the header's size is a property of the file.
fn read_header(path: &str) -> Vec<u8> {
    let mut file = std::fs::File::open(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
    let mut buf = Vec::new();
    let mut want = 1usize << 22;
    loop {
        buf.resize(want, 0);
        let mut read = 0usize;
        while read < want {
            match file.read(&mut buf[read..]) {
                Ok(0) => break,
                Ok(n) => read += n,
                Err(e) => die(format!("{path}: {e}")),
            }
        }
        buf.truncate(read);
        if parse_directory(&buf).is_ok() || read < want {
            return buf;
        }
        want *= 2;
        if want > (1usize << 30) {
            die(format!("{path}: the header did not parse within a gigabyte"));
        }
        use std::io::Seek;
        file.rewind().unwrap_or_else(|e| die(format!("{path}: {e}")));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let artifact_path =
        flag(&args, "--artifact").unwrap_or_else(|| die("usage: qwen36-chat --artifact <file> --gguf <file> --prompt <text>".into()));
    let gguf_path = flag(&args, "--gguf").unwrap_or_else(|| die("--gguf <checkpoint-or-header> is required for the tokenizer".into()));
    let prompt = flag(&args, "--prompt").unwrap_or_else(|| die("--prompt <text> is required".into()));
    let system = flag(&args, "--system");
    let max_tokens: usize = flag(&args, "--max-tokens").and_then(|v| v.parse().ok()).unwrap_or(128);
    let raw = args.iter().any(|a| a == "--raw");
    let thinking = args.iter().any(|a| a == "--think");

    let started = std::time::Instant::now();
    let header = read_header(gguf_path);
    let directory = parse_directory(&header).unwrap_or_else(|e| die(format!("{gguf_path}: {e}")));
    let get = |key: &str| directory.metadata.get(key);
    let tokens = get("tokenizer.ggml.tokens").and_then(|v| v.as_strings()).unwrap_or_else(|| die("no tokenizer.ggml.tokens".into()));
    let merges = get("tokenizer.ggml.merges").and_then(|v| v.as_strings()).unwrap_or_else(|| die("no tokenizer.ggml.merges".into()));
    let types = get("tokenizer.ggml.token_type").and_then(|v| v.as_ints()).unwrap_or(&[]);
    let tokenizer = QwenTokenizer::from_gguf(tokens, merges, types).unwrap_or_else(|e| die(format!("{gguf_path}: {e}")));
    eprintln!("tokenizer {} tokens, {} merges, read in {:?}", tokenizer.len(), merges.len(), started.elapsed());
    drop(header);

    let opened = std::time::Instant::now();
    let artifact = open_artifact(std::path::Path::new(artifact_path)).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    let shape = &artifact.shape;
    eprintln!(
        "artifact  {} layers, {:.2} GiB of weights, vocab {}, context {} — mapped in {:?}",
        shape.n_layers(),
        artifact.weight_bytes() as f64 / (1u64 << 30) as f64,
        shape.vocab,
        shape.max_position,
        opened.elapsed()
    );

    let text = if raw { prompt.to_string() } else { chat_prompt(system, prompt, thinking) };
    let ids = tokenizer.encode(&text).unwrap_or_else(|e| die(format!("tokenizing: {e}")));
    // Before the context check: what a tokenizer produced is worth seeing even when no artifact in
    // hand is large enough to run it.
    if args.iter().any(|a| a == "--show-ids") {
        println!("{}", ids.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(","));
        eprintln!(
            "{} tokens, round-trip {}",
            ids.len(),
            if tokenizer.decode(&ids).ok().as_deref() == Some(text.as_str()) { "exact" } else { "LOSSY" }
        );
        return;
    }
    if ids.len() >= shape.max_position {
        die(format!("the prompt is {} tokens and this artifact's rotary table covers {}", ids.len(), shape.max_position));
    }
    // By content, not by id: the number is a property of the vocabulary the checkpoint shipped.
    let stop = if raw { tokenizer.added_id("<|endoftext|>") } else { tokenizer.added_id("<|im_end|>") };
    eprintln!("prompt    {} tokens, stop {stop:?}", ids.len());

    // **Residency, in the runtime's hands rather than the page cache's** (see `Qwen36Residency`).
    // A budget of zero leaves it to the kernel, which is the measurement this is compared against.
    let engine = match flag(&args, "--expert-cache-gib").and_then(|v| v.parse::<f64>().ok()) {
        Some(gib) if gib > 0.0 => {
            eprintln!("residency  pinning the always-set, {gib:.1} GiB budget for routed experts");
            Qwen36Engine::with_residency(&artifact, (gib * (1u64 << 30) as f64) as usize)
        }
        _ => Qwen36Engine::new(&artifact),
    };
    let mut cache = Qwen36Cache::new(shape);

    let prefill_started = std::time::Instant::now();
    let mut logits = Vec::new();
    for (position, token) in ids.iter().enumerate() {
        logits = engine
            .forward_token(&mut cache, *token as usize, position)
            .unwrap_or_else(|e| die(format!("prefill at position {position}: {e}")));
    }
    let prefill = prefill_started.elapsed();

    // The whole run is re-decoded each step rather than one token at a time: a multi-byte
    // character can straddle two tokens, and decoding each alone prints a replacement character
    // where a kanji belongs.
    let decode_started = std::time::Instant::now();
    let mut generated: Vec<u32> = Vec::new();
    let mut shown = 0usize;
    let mut stopped = "length";
    for step in 0..max_tokens {
        let next = argmax_lowest(&logits) as u32;
        if Some(next) == stop {
            stopped = "stop token";
            break;
        }
        generated.push(next);
        let out = tokenizer.decode_lossy_tail(&generated);
        if out.len() > shown {
            print!("{}", &out[shown..]);
            let _ = std::io::stdout().flush();
            shown = out.len();
        }
        let position = ids.len() + step;
        if position + 1 >= shape.max_position {
            stopped = "context";
            break;
        }
        logits = engine
            .forward_token(&mut cache, next as usize, position)
            .unwrap_or_else(|e| die(format!("decode at position {position}: {e}")));
    }
    println!();
    let decode = decode_started.elapsed();
    eprintln!(
        "\nprefill {} tok in {prefill:?} ({:.1} tok/s), decode {} tok in {decode:?} ({:.2} tok/s), stopped on {stopped}",
        ids.len(),
        ids.len() as f64 / prefill.as_secs_f64(),
        generated.len(),
        generated.len() as f64 / decode.as_secs_f64().max(f64::MIN_POSITIVE)
    );
    if let Some((hits, misses, evictions, bytes)) = engine.residency_stats() {
        eprintln!(
            "residency  {hits} hits / {} lookups ({:.1} %), {evictions} evictions, {:.1} GiB resident",
            hits + misses,
            100.0 * hits as f64 / (hits + misses).max(1) as f64,
            bytes as f64 / (1u64 << 30) as f64
        );
    }
}
