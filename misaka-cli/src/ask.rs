//! `misaka ask` — use the network's pinned model as an ordinary LLM, with a receipt.
//!
//! The chain's whole claim is that an LLM answer can be *checked* by someone who did not produce
//! it. Everywhere else that claim is machinery (PoW tags, VLT jobs, committees). Here it is a
//! feature you can hold: ask a question in any language, read the answer, and hand someone the
//! receipt line. They run `misaka ask --verify <receipt>` with the same prompt and either get the
//! same answer byte-for-byte or find out they did not.
//!
//! What makes that possible is that the request is pinned, not merely repeated: greedy decoding,
//! CPU backend, fixed context — the same options the legacy tag path uses, held in one place
//! (`misaka_palw_pow_driver::palw_generate`, the crate that owns every model-reaching call after
//! ADR-0042 Decision 4 moved them out of kaspa-pow) so this command and block validation cannot
//! drift apart.
//!
//! The one knob that changes the answer and therefore lives in the receipt is `--tokens`
//! (`num_predict`): a longer budget is a different computation, not merely more of the same one.

use crate::{CliError, CliResult, exit};
use kaspa_consensus_core::pow_layer0::PowLayer0Error;
use misaka_palw_pow_driver::{DEFAULT_OLLAMA_URL, PALW_OLLAMA_MODEL_ENV, PALW_OLLAMA_URL_ENV, palw_generate};
use std::io::Read;

/// Receipt format tag. Bump if any field or the digest derivation changes.
const RECEIPT_V1: &str = "misaka-ask-v1";
/// Domain separator for the receipt digests, so an ask digest can never be confused with a PoW
/// tag or a VLT job commitment computed over the same bytes.
const RECEIPT_DOMAIN: &[u8] = b"misaka-ask-receipt-v1";

#[derive(clap::Args, Debug)]
pub struct AskArgs {
    /// The question. Any language — it is sent as UTF-8. Omit to read the prompt from stdin,
    /// which is the better route for anything multi-line.
    pub prompt: Option<String>,

    /// Read the prompt from a file instead of the command line.
    #[arg(long, short = 'f')]
    pub file: Option<String>,

    /// Maximum tokens to generate. Part of the receipt: a different budget is a different answer.
    #[arg(long, short = 't', default_value_t = 512)]
    pub tokens: u32,

    /// Ollama model reference. Defaults to `MISAKA_PALW_OLLAMA_MODEL`, i.e. whatever this host
    /// validates the chain with — asking the network's model is the point.
    #[arg(long)]
    pub model: Option<String>,

    /// Ollama endpoint (default `MISAKA_PALW_OLLAMA_URL`, else 127.0.0.1:11434).
    #[arg(long)]
    pub url: Option<String>,

    /// Re-run this receipt's computation and report whether the answer still matches it.
    /// Supply the same prompt; everything else the receipt carries.
    #[arg(long)]
    pub verify: Option<String>,

    /// Let the model reason before answering, and show that reasoning. Off by default: this model
    /// is thinking-capable, and left on it spends the token budget narrating its plan instead of
    /// answering. Recorded in the receipt (`mode=chat+think`) because it changes the output.
    #[arg(long, default_value_t = false)]
    pub show_thinking: bool,

    /// Print only the answer — no receipt, no timing. For piping into other tools.
    #[arg(long, default_value_t = false)]
    pub quiet: bool,

    /// Reproduce the PoW-style raw continuation instead of a chat answer. Diagnostic: this is the
    /// mode block validation uses, so it is how you compare a node against a hand-run request.
    #[arg(long, default_value_t = false)]
    pub raw: bool,
}

/// `blake2b-256(key = RECEIPT_DOMAIN, label || len_le || bytes)`, hex — one derivation for every
/// field so no two of them can collide by concatenation.
fn digest(label: &[u8], bytes: &[u8]) -> String {
    let d = blake2b_simd::Params::new()
        .hash_length(32)
        .key(RECEIPT_DOMAIN)
        .to_state()
        .update(label)
        .update(&(bytes.len() as u64).to_le_bytes())
        .update(bytes)
        .finalize();
    faster_hex::hex_string(d.as_bytes())
}

/// The receipt line: everything needed to re-run the computation, plus what it produced.
struct Receipt {
    model: String,
    tokens: u32,
    templated: bool,
    think: bool,
    prompt: String,
    answer: String,
    counts: (u32, u32),
}

impl Receipt {
    fn mode(&self) -> &'static str {
        match (self.templated, self.think) {
            (false, _) => "raw",
            (true, true) => "chat+think",
            (true, false) => "chat",
        }
    }

    fn render(&self) -> String {
        format!(
            "{RECEIPT_V1} model={} mode={} tokens={} prompt={} answer={} eval={}+{}",
            self.model,
            self.mode(),
            self.tokens,
            &self.prompt[..16],
            &self.answer[..16],
            self.counts.0,
            self.counts.1
        )
    }

    /// Parse the fields a verifier must reuse. Deliberately strict: a receipt that cannot be
    /// parsed exactly is not a receipt.
    fn parse(line: &str) -> Result<(String, u32, bool, bool, String, String), String> {
        let line = line.trim();
        let mut model = None;
        let mut tokens = None;
        let mut templated = None;
        let mut think = None;
        let mut prompt = None;
        let mut answer = None;
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some(RECEIPT_V1) => {}
            other => return Err(format!("not a {RECEIPT_V1} receipt (starts with {other:?})")),
        }
        for f in fields {
            let Some((k, v)) = f.split_once('=') else { continue };
            match k {
                "model" => model = Some(v.to_owned()),
                "tokens" => tokens = v.parse().ok(),
                "mode" => {
                    templated = Some(v.starts_with("chat"));
                    think = Some(v == "chat+think");
                }
                "prompt" => prompt = Some(v.to_owned()),
                "answer" => answer = Some(v.to_owned()),
                _ => {}
            }
        }
        Ok((
            model.ok_or("receipt lacks model=")?,
            tokens.ok_or("receipt lacks a numeric tokens=")?,
            templated.ok_or("receipt lacks mode=")?,
            think.ok_or("receipt lacks mode=")?,
            prompt.ok_or("receipt lacks prompt=")?,
            answer.ok_or("receipt lacks answer=")?,
        ))
    }
}

/// Qwen wraps its reasoning in `<think>…</think>` ahead of the answer. It is part of the
/// generated text — and so of the digest — but reading it is rarely what someone wants, so it is
/// hidden unless asked for. Never stripped before hashing.
fn strip_thinking(text: &str) -> &str {
    match text.split_once("</think>") {
        Some((_, rest)) => rest.trim_start(),
        None => text,
    }
}

pub fn run(args: AskArgs) -> CliResult {
    let model = args.model.clone().or_else(|| std::env::var(PALW_OLLAMA_MODEL_ENV).ok()).ok_or_else(|| {
        CliError::generic(format!(
            "no model: pass --model, or set {PALW_OLLAMA_MODEL_ENV} to the one this host \
                 validates with (that is the point — you are asking the network's model)"
        ))
    })?;
    let url = args.url.clone().or_else(|| std::env::var(PALW_OLLAMA_URL_ENV).ok()).unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_owned());

    // Prompt: argument, file, or stdin. stdin is the one that handles multi-line text and any
    // language without shell quoting getting in the way.
    let prompt = match (&args.prompt, &args.file) {
        (Some(_), Some(_)) => return Err(CliError::generic("give a prompt or --file, not both")),
        (Some(p), None) => p.clone(),
        (None, Some(f)) => std::fs::read_to_string(f).map_err(|e| CliError::generic(format!("cannot read {f}: {e}")))?,
        (None, None) => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).map_err(|e| CliError::generic(format!("cannot read stdin: {e}")))?;
            if buf.trim().is_empty() {
                return Err(CliError::generic("no prompt: pass one as an argument, with --file, or on stdin"));
            }
            buf
        }
    };
    let prompt = prompt.trim_end().to_owned();

    // A verify run reuses the receipt's parameters, so only the prompt comes from the caller.
    let (model, tokens, templated, think, want_prompt, want_answer) = match &args.verify {
        Some(receipt) => {
            let (m, t, tm, th, p, a) = Receipt::parse(receipt).map_err(CliError::generic)?;
            (m, t, tm, th, Some(p), Some(a))
        }
        None => (model, args.tokens, !args.raw, args.show_thinking, None, None),
    };

    let prompt_digest = digest(b"prompt", prompt.as_bytes());
    if let Some(want) = &want_prompt
        && !prompt_digest.starts_with(want.as_str())
    {
        return Err(CliError::generic(format!(
            "this is a different question than the receipt's.\n  receipt prompt={want}\n  yours   prompt={}\n\
             Verification needs the ORIGINAL prompt, byte for byte.",
            &prompt_digest[..16]
        )));
    }

    let started = std::time::Instant::now();
    // `think` is only meaningful for the templated path; the raw diagnostic mode sends the
    // consensus request unchanged.
    let think_opt = if templated { Some(think) } else { None };
    let (answer, prompt_eval, eval) = palw_generate(&url, &model, &prompt, tokens, templated, think_opt).map_err(|e| match e {
        // "cannot reach / not configured" is an operator-fixable connection problem; a runtime
        // that ran and misbehaved is not.
        PowLayer0Error::PalwUnavailable(m) => CliError::new(exit::CONNECTION, m),
        other => CliError::generic(other.to_string()),
    })?;
    let elapsed = started.elapsed();
    let answer_digest = digest(b"answer", answer.as_bytes());

    let receipt =
        Receipt { model, tokens, templated, think, prompt: prompt_digest, answer: answer_digest.clone(), counts: (prompt_eval, eval) };

    if let Some(want) = want_answer {
        let matched = answer_digest.starts_with(want.as_str());
        println!("{}", if think { answer.as_str() } else { strip_thinking(&answer) });
        println!();
        if matched {
            println!("VERIFIED — this runtime reproduced the receipt's answer byte for byte.");
            println!("  {}", receipt.render());
            return Ok(());
        }
        println!("MISMATCH — same question, same parameters, DIFFERENT answer.");
        println!("  receipt answer={want}");
        println!("  this run answer={}", &answer_digest[..16]);
        println!(
            "\nEither the receipt did not come from this network's pinned model, or this host is not in\n\
             its determinism class (different model blob, Ollama build, or CPU architecture — the same\n\
             thing kaspad checks at startup)."
        );
        return Err(CliError::generic("receipt not reproduced"));
    }

    if args.quiet {
        print!("{}", if think { answer.as_str() } else { strip_thinking(&answer) });
        return Ok(());
    }

    println!("{}", if think { answer.as_str() } else { strip_thinking(&answer) });
    println!();
    println!("{}", receipt.render());
    println!(
        "  {} tokens in {:.1}s — anyone on this network can re-run it:  misaka ask --verify '{}' -f <your prompt file>",
        eval,
        elapsed.as_secs_f64(),
        receipt.render()
    );
    Ok(())
}
