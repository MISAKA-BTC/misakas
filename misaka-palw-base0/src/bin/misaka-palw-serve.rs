//! **`misaka-palw-serve` — the integer runtime, speaking OpenAI.**
//!
//! `base0-chat` proved the engine answers a sentence; this serves that same path over HTTP so an
//! application can use it the way it already uses `llama-server`. MISAKA Studio is the first such
//! application, and the point of the exercise is that it stops needing llama.cpp at all: the
//! engine that answers the user is the engine whose execution a class registers and a court can
//! recompute.
//!
//! ```text
//! app ──POST /v1/chat/completions──▶ this ──▶ A16Engine (integer, greedy argmax)
//! ```
//!
//! # What it does not pretend
//!
//! **Greedy only.** No temperature, no top-p, no seed — the same rule `base0-chat` states: PALW's
//! verification target is the canonical logit row and the argmax over it, and a sampler belongs to
//! an application on top of this, not to a runtime whose output a court has to reproduce. Requests
//! carrying sampling parameters are answered anyway, and `/health` says the sampler is greedy, so
//! nobody has to infer it from output that happens to look deterministic.
//!
//! **Not court-capable, and it says so.** The A16 family produces logit rows and generated tokens;
//! it does not capture the activation, checkpoint and step legs that make an execution
//! adjudicable, and `Qwen25A16Backend` accordingly leaves `supports_court` at its default `false`.
//! A run served here is therefore a real inference under a registered class and NOT yet a
//! free-prompt claim anyone can mine — `court_capable: false` in `/health` is that fact, published
//! where a client can read it rather than discovered after a claim fails to adjudicate.
//!
//! **One generation at a time.** A single engine and a single KV cache: concurrent decodes would
//! interleave in the cache and produce two wrong answers. The lock is the whole concurrency story.

use misaka_palw_base0::artifact::decode_artifact_file_v1;
use misaka_palw_base0::engine_a16::{A16Cache, A16Engine};
use misaka_palw_base0::tokenizer::{QwenTokenizer, qwen_chat_prompt};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;

fn die(message: String) -> ! {
    eprintln!("misaka-palw-serve: {message}");
    std::process::exit(1)
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

/// Enough HTTP for one JSON POST and one probe, over std's listener — the same surface, and the
/// same reasoning, as the free-prompt gateway next door: an async stack for two routes is a
/// dependency nobody needs to audit.
fn read_http_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).map_err(|e| e.to_string())? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().map_err(|_| "content-length is not a number".to_string())?;
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    }
    Ok(Request { method, path, body })
}

fn respond(stream: &mut TcpStream, status: &str, body: &serde_json::Value) {
    let bytes = body.to_string().into_bytes();
    let head =
        format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", bytes.len());
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&bytes);
    let _ = stream.flush();
}

fn error_body(message: &str) -> serde_json::Value {
    serde_json::json!({ "error": { "message": message, "type": "invalid_request_error" } })
}

/// **The streaming reply, in the shape every OpenAI client already parses.**
///
/// Chunked, because the length is unknown until the model stops: `content-length` would mean
/// buffering the whole answer and defeating the point. One `data:` line per newly decoded piece,
/// a usage frame, then `[DONE]` — the frames MISAKA Studio's SSE parser reads, and the ones
/// llama-server emits, so a client cannot tell the two engines apart by their transport.
fn sse_open(stream: &mut TcpStream) {
    let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n";
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.flush();
}

fn sse_send(stream: &mut TcpStream, value: &serde_json::Value) -> Result<(), ()> {
    let line = format!("data: {value}\n\n");
    stream.write_all(line.as_bytes()).map_err(|_| ())?;
    stream.flush().map_err(|_| ())
}

/// One completion. Returns the answer and the counts, or the reason it could not run.
#[allow(clippy::too_many_arguments)]
fn complete(
    engine: &A16Engine,
    tokenizer: &QwenTokenizer,
    n_layers: usize,
    max_position: usize,
    system: Option<&str>,
    turns: &[(String, String)],
    max_tokens: usize,
    batch: usize,
    // Called with each newly decoded piece of text, for the streaming caller. Returning `Err`
    // stops the generation — a client that hung up should not keep a 28-layer decode running.
    on_text: &mut dyn FnMut(&str) -> Result<(), ()>,
) -> Result<(String, usize, usize, &'static str), String> {
    let borrowed: Vec<(&str, &str)> = turns.iter().map(|(r, c)| (r.as_str(), c.as_str())).collect();
    let text = qwen_chat_prompt(system, &borrowed);
    let ids = tokenizer.encode(&text).map_err(|e| format!("tokenizing: {e}"))?;
    if ids.len() >= max_position {
        return Err(format!("the prompt is {} tokens and this artifact's rotary table covers {}", ids.len(), max_position));
    }
    let stop = tokenizer.added_id("<|im_end|>");

    let mut cache = A16Cache::new(n_layers);
    let prompt_ids: Vec<usize> = ids.iter().map(|v| *v as usize).collect();
    let mut logits = engine.forward_prefill(&mut cache, &prompt_ids, 0, batch).map_err(|e| format!("prefill failed: {e:?}"))?;

    let mut generated: Vec<u32> = Vec::new();
    let mut shown = 0usize;
    let mut stopped = "length";
    for step in 0..max_tokens {
        let next = misaka_palw_base0::engine::argmax_lowest(&logits) as u32;
        if Some(next) == stop {
            stopped = "stop";
            break;
        }
        generated.push(next);
        // Decoded from the whole run each step, then only the new suffix is emitted: a multi-byte
        // character can straddle two tokens, and decoding each token alone would emit a
        // replacement character where a kanji belongs.
        let so_far = tokenizer.decode_lossy_tail(&generated);
        if so_far.len() > shown {
            if on_text(&so_far[shown..]).is_err() {
                stopped = "client";
                break;
            }
            shown = so_far.len();
        }
        let position = ids.len() + step;
        if position + 1 >= max_position {
            stopped = "context";
            break;
        }
        logits = engine.forward_token(&mut cache, next as usize, position).map_err(|e| format!("decode failed: {e:?}"))?;
    }
    // Decoded from the whole run, not per token: a multi-byte character can straddle two tokens.
    let answer = tokenizer.decode(&generated).map_err(|e| format!("detokenizing: {e}"))?;
    Ok((answer, ids.len(), generated.len(), stopped))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let artifact_path = flag(&args, "--artifact").unwrap_or_else(|| die("--artifact <file.palwart> is required".into()));
    let tokenizer_path = flag(&args, "--tokenizer").unwrap_or_else(|| die("--tokenizer <tokenizer.json> is required".into()));
    let listen = flag(&args, "--listen").unwrap_or_else(|| "127.0.0.1:8791".to_string());
    let default_max: usize = flag(&args, "--max-tokens").and_then(|v| v.parse().ok()).unwrap_or(256);
    let cap: usize = flag(&args, "--max-tokens-cap").and_then(|v| v.parse().ok()).unwrap_or(2048);
    let batch: usize = flag(&args, "--batch").and_then(|v| v.parse().ok()).unwrap_or(32);

    let bytes = std::fs::read(&artifact_path).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    let artifact = decode_artifact_file_v1(&bytes).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    drop(bytes);
    let tokenizer_bytes = std::fs::read(&tokenizer_path).unwrap_or_else(|e| die(format!("{tokenizer_path}: {e}")));
    let tokenizer = QwenTokenizer::from_json(&tokenizer_bytes).unwrap_or_else(|e| die(format!("{tokenizer_path}: {e}")));
    let engine = A16Engine::new(&artifact).unwrap_or_else(|e| die(format!("the artifact is not an A16 class: {e:?}")));
    let shape = artifact.shape;
    let digest = artifact.artifact_digest().to_string();

    let health = serde_json::json!({
        "status": "ok",
        "runtime": "misaka-palw-a16",
        // The identity a class registers and a node verifies against. Published because a client
        // that cannot name the artifact it is talking to cannot tell this apart from any other
        // engine wearing the same HTTP shape.
        "artifact_digest": digest,
        "n_layers": shape.n_layers,
        "vocab": shape.vocab,
        "max_position": shape.max_position,
        "sampler": "greedy-argmax-lowest-id",
        // See the module note: real inference under a registered class, and not yet an adjudicable
        // one. A client deciding whether this run can become a free-prompt claim needs this before
        // it asks, not after.
        "court_capable": false,
    });

    let lock = Mutex::new(());
    let listener = TcpListener::bind(&listen).unwrap_or_else(|e| die(format!("cannot bind {listen}: {e}")));
    eprintln!(
        "[misaka-palw-serve] listening on {listen} — artifact {}…, {} layers, vocab {}, greedy",
        &digest[..16],
        shape.n_layers,
        shape.vocab
    );

    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let request = match read_http_request(&mut stream) {
            Ok(r) => r,
            Err(e) => {
                respond(&mut stream, "400 Bad Request", &error_body(&e));
                continue;
            }
        };
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/health") => respond(&mut stream, "200 OK", &health),
            ("GET", "/v1/models") => respond(
                &mut stream,
                "200 OK",
                &serde_json::json!({ "object": "list", "data": [{ "id": "misaka-palw-a16", "object": "model" }] }),
            ),
            ("POST", "/v1/chat/completions") => {
                let _running = lock.lock().expect("the generation lock is never poisoned");
                let parsed: serde_json::Value = match serde_json::from_slice(&request.body) {
                    Ok(v) => v,
                    Err(e) => {
                        respond(&mut stream, "400 Bad Request", &error_body(&format!("the body is not JSON: {e}")));
                        continue;
                    }
                };
                let mut system: Option<String> = None;
                let mut turns: Vec<(String, String)> = Vec::new();
                for message in parsed.get("messages").and_then(|m| m.as_array()).into_iter().flatten() {
                    let role = message.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                    let content = message.get("content").and_then(|c| c.as_str()).unwrap_or_default().to_string();
                    if role == "system" {
                        system = Some(content);
                    } else {
                        turns.push((role.to_string(), content));
                    }
                }
                if turns.is_empty() {
                    respond(&mut stream, "400 Bad Request", &error_body("no messages to answer"));
                    continue;
                }
                let want = parsed.get("max_tokens").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(default_max).min(cap);

                let streaming = parsed.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);
                if streaming {
                    sse_open(&mut stream);
                    // The first frame carries the role, as OpenAI's does, so a client that renders
                    // an empty assistant bubble has something to attach the deltas to.
                    let _ = sse_send(
                        &mut stream,
                        &serde_json::json!({
                            "id": "misaka-palw-a16", "object": "chat.completion.chunk", "model": "misaka-palw-a16",
                            "choices": [{ "index": 0, "delta": { "role": "assistant" } }],
                        }),
                    );
                    let mut sink_stream = match stream.try_clone() {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    let mut on_text = |piece: &str| {
                        sse_send(
                            &mut sink_stream,
                            &serde_json::json!({
                                "id": "misaka-palw-a16", "object": "chat.completion.chunk", "model": "misaka-palw-a16",
                                "choices": [{ "index": 0, "delta": { "content": piece } }],
                            }),
                        )
                    };
                    let outcome = complete(
                        &engine,
                        &tokenizer,
                        shape.n_layers,
                        shape.max_position,
                        system.as_deref(),
                        &turns,
                        want,
                        batch,
                        &mut on_text,
                    );
                    match outcome {
                        Ok((_, prompt_tokens, completion_tokens, stopped)) => {
                            let _ = sse_send(
                                &mut stream,
                                &serde_json::json!({
                                    "id": "misaka-palw-a16", "object": "chat.completion.chunk", "model": "misaka-palw-a16",
                                    "choices": [{ "index": 0, "delta": {},
                                                  "finish_reason": if stopped == "stop" { "stop" } else { "length" } }],
                                    "usage": { "prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens,
                                               "total_tokens": prompt_tokens + completion_tokens },
                                }),
                            );
                        }
                        // A failure mid-stream cannot become a 400: the head is already sent. It
                        // rides as an error frame, which is what a client can still act on.
                        Err(e) => {
                            let _ = sse_send(&mut stream, &error_body(&e));
                        }
                    }
                    let _ = stream.write_all(b"data: [DONE]\n\n");
                    let _ = stream.flush();
                    continue;
                }

                let mut discard = |_: &str| Ok(());
                match complete(
                    &engine,
                    &tokenizer,
                    shape.n_layers,
                    shape.max_position,
                    system.as_deref(),
                    &turns,
                    want,
                    batch,
                    &mut discard,
                ) {
                    Ok((answer, prompt_tokens, completion_tokens, stopped)) => {
                        let body = serde_json::json!({
                            "id": "misaka-palw-a16",
                            "object": "chat.completion",
                            "model": "misaka-palw-a16",
                            "choices": [{
                                "index": 0,
                                "message": { "role": "assistant", "content": answer },
                                "finish_reason": if stopped == "stop" { "stop" } else { "length" },
                            }],
                            "usage": {
                                "prompt_tokens": prompt_tokens,
                                "completion_tokens": completion_tokens,
                                "total_tokens": prompt_tokens + completion_tokens,
                            },
                        });
                        respond(&mut stream, "200 OK", &body);
                    }
                    Err(e) => respond(&mut stream, "400 Bad Request", &error_body(&e)),
                }
            }
            _ => respond(
                &mut stream,
                "404 Not Found",
                &error_body("this server answers POST /v1/chat/completions, GET /v1/models and GET /health"),
            ),
        }
    }
}
