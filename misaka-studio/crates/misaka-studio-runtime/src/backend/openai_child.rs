//! Driving an engine that already speaks OpenAI, as a child process.
//!
//! `llama-server` and `mlx_lm.server` are both OpenAI-compatible HTTP servers. Writing two
//! supervisors for that would be writing the same process lifecycle, the same health wait, the
//! same SSE parser and the same "it died, here is why" plumbing twice — so this module is the
//! shared half, and each backend supplies only what differs: the program, its arguments, and
//! how it identifies itself.
//!
//! # Why a child process and not FFI
//!
//! Linking llama.cpp into the app would make a model that crashes the engine crash the app, on a
//! GPU driver fault the user cannot avoid and we cannot catch. As a child it takes its own
//! address space, its own OOM kill and its own segfault, and the Studio survives to say what
//! happened. It also means the engine can be updated without rebuilding the Studio, which is how
//! a user gets a new llama.cpp the week it ships support for a new architecture.
//!
//! The cost is honesty about identity: a binary we did not build cannot tell us its CMake flags,
//! so [`ChildEngine::descriptor`] records the ones it can prove and the literal `unknown` for
//! the rest, and its determinism class is scoped to (backend, OS, arch, accelerator). A pinned,
//! Studio-built engine would fill those in — that is the upgrade path, not a thing to pretend
//! about now.

use super::mock::async_stream;
use super::{Availability, GenerationRequest, LoadRequest, LoadedModel, StreamEvent, Usage, approximate_tokens};
use crate::{Error, Result};
use misaka_studio_core::provenance::RuntimeDescriptor;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::RwLock;

/// Lines of engine output kept for diagnostics.
///
/// The engine's last words are the only explanation a user gets when a load fails — "cannot
/// allocate 24.1 GiB on device 0" is the message, and it is on stderr, not in any exit code.
const LOG_LINES: usize = 400;

/// How often the health endpoint is polled while waiting for a load.
const HEALTH_POLL: Duration = Duration::from_millis(250);

/// What a concrete backend must supply.
pub struct ChildEngineConfig {
    /// Backend name, as it appears in records: `llamacpp`, `mlx`.
    pub name: &'static str,
    /// The executable.
    pub program: PathBuf,
    /// Build the argument list for a load, given the request and the port to listen on.
    pub args: Box<dyn Fn(&LoadRequest, u16) -> Vec<String> + Send + Sync>,
    /// Path of the health endpoint, relative to the base URL.
    pub health_path: &'static str,
    /// Seconds to wait for the engine to report healthy.
    pub startup_timeout: Duration,
    /// Extra environment for the child.
    pub env: Vec<(String, String)>,
}

struct Running {
    child: tokio::process::Child,
    port: u16,
    model: LoadedModel,
}

/// A supervised OpenAI-compatible engine.
pub struct ChildEngine {
    config: ChildEngineConfig,
    running: RwLock<Option<Running>>,
    log: Arc<Mutex<VecDeque<String>>>,
    http: reqwest::Client,
}

impl ChildEngine {
    pub fn new(config: ChildEngineConfig) -> Self {
        ChildEngine {
            config,
            running: RwLock::new(None),
            log: Arc::new(Mutex::new(VecDeque::with_capacity(LOG_LINES))),
            // No overall timeout: a generation legitimately runs for minutes. Connect timeouts
            // still apply, so a dead engine is detected quickly.
            http: reqwest::Client::builder().connect_timeout(Duration::from_secs(5)).build().expect("http client builds"),
        }
    }

    pub fn name(&self) -> &'static str {
        self.config.name
    }

    /// The engine's recent output, newest last.
    pub fn recent_log(&self) -> Vec<String> {
        self.log.lock().expect("log lock").iter().cloned().collect()
    }

    fn push_log(log: &Arc<Mutex<VecDeque<String>>>, line: String) {
        let mut guard = log.lock().expect("log lock");
        if guard.len() == LOG_LINES {
            guard.pop_front();
        }
        guard.push_back(line);
    }

    /// Whether the program exists and can be executed.
    pub async fn availability(&self, remedy: &str) -> Availability {
        match self.version_output().await {
            Some(v) => Availability::Available { detail: v.lines().next().unwrap_or("present").trim().to_string() },
            None => Availability::Unavailable {
                reason: format!("{} could not be run", self.config.program.display()),
                remedy: remedy.to_string(),
            },
        }
    }

    /// `<program> --version`, stdout and stderr together.
    ///
    /// Both streams, because llama.cpp prints its version banner to stderr and other engines
    /// print to stdout, and a version parser that reads the wrong one silently reports "unknown"
    /// forever.
    pub async fn version_output(&self) -> Option<String> {
        let out = tokio::process::Command::new(&self.config.program)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .ok()?;
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        Some(text)
    }

    /// Start the engine and wait until it answers its health endpoint.
    pub async fn load(&self, request: LoadRequest) -> Result<LoadedModel> {
        // A load replaces whatever was loaded. Doing it in this order — stop, then start — is
        // what keeps two engines from holding the same GPU memory at once, which on a card with
        // no headroom means the new load fails and the old model is gone too.
        self.unload().await?;

        let started = Instant::now();
        let port = free_port()?;
        let args = (self.config.args)(&request, port);

        let mut command = tokio::process::Command::new(&self.config.program);
        command
            .args(&args)
            .envs(self.config.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // If the Studio dies, the engine must not survive holding 20 GB of VRAM.
            .kill_on_drop(true);

        tracing::info!(program = %self.config.program.display(), ?args, "starting engine");
        let mut child = command.spawn().map_err(|e| {
            Error::Engine {
                backend: self.config.name,
                message: format!("could not start {}: {e}", self.config.program.display()),
            }
        })?;

        for stream in [child.stdout.take().map(Pipe::Out), child.stderr.take().map(Pipe::Err)].into_iter().flatten() {
            let log = self.log.clone();
            tokio::spawn(async move {
                match stream {
                    Pipe::Out(s) => drain(s, log).await,
                    Pipe::Err(s) => drain(s, log).await,
                }
            });
        }

        let base = format!("http://127.0.0.1:{port}");
        let health = format!("{base}{}", self.config.health_path);
        let deadline = Instant::now() + self.config.startup_timeout;

        loop {
            // A child that exited is a failure with an explanation waiting in the log — report
            // that rather than waiting out the full timeout on a process that will never answer.
            if let Ok(Some(status)) = child.try_wait() {
                let tail = self.recent_log().into_iter().rev().take(15).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>();
                return Err(Error::Engine {
                    backend: self.config.name,
                    message: format!("engine exited with {status} before it was ready:\n{}", tail.join("\n")),
                });
            }
            if let Ok(resp) = self.http.get(&health).send().await
                && resp.status().is_success()
            {
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill().await;
                return Err(Error::Engine {
                    backend: self.config.name,
                    message: format!(
                        "engine did not become ready within {}s. A large model on a slow disk can take longer — \
                         raise backend.startup_timeout_secs, or check the log:\n{}",
                        self.config.startup_timeout.as_secs(),
                        self.recent_log().into_iter().rev().take(10).collect::<Vec<_>>().join("\n")
                    ),
                });
            }
            tokio::time::sleep(HEALTH_POLL).await;
        }

        let model = LoadedModel {
            model_id: request.model_id.clone(),
            context_size: request.context_size,
            gpu_layers: request.gpu_layers,
            load_ms: started.elapsed().as_millis() as u64,
        };
        *self.running.write().await = Some(Running { child, port, model: model.clone() });
        tracing::info!(model = %model.model_id, ms = model.load_ms, "engine ready");
        Ok(model)
    }

    pub async fn unload(&self) -> Result<()> {
        if let Some(mut running) = self.running.write().await.take() {
            tracing::info!(model = %running.model.model_id, "stopping engine");
            let _ = running.child.kill().await;
            let _ = running.child.wait().await;
        }
        Ok(())
    }

    pub async fn loaded(&self) -> Option<LoadedModel> {
        self.running.read().await.as_ref().map(|r| r.model.clone())
    }

    async fn base_url(&self) -> Result<String> {
        let guard = self.running.read().await;
        let running = guard.as_ref().ok_or(Error::NoModelLoaded)?;
        Ok(format!("http://127.0.0.1:{}", running.port))
    }

    /// Send the request to the engine and stream what comes back.
    pub async fn generate(&self, request: GenerationRequest) -> Result<futures_util::stream::BoxStream<'static, Result<StreamEvent>>> {
        let base = self.base_url().await?;
        let chat = request.prompt.is_none();
        let url = if chat { format!("{base}/v1/chat/completions") } else { format!("{base}/v1/completions") };
        let body = request_body(&request, chat);
        let http = self.http.clone();
        let backend = self.config.name;
        let fallback_prompt_tokens = approximate_tokens(
            &request
                .prompt
                .clone()
                .unwrap_or_else(|| request.messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n")),
        );

        let response = http.post(&url).json(&body).send().await.map_err(|e| Error::Engine {
            backend,
            message: format!("the engine did not accept the request: {e}"),
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Engine { backend, message: format!("engine returned {status}: {}", text.trim()) });
        }

        Ok(async_stream(move |tx| async move {
            let mut parser = SseParser::new(chat);
            let mut byte_stream = response.bytes_stream();
            use futures_util::StreamExt;

            while let Some(chunk) = byte_stream.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Err(Error::Engine { backend, message: format!("stream broke: {e}") })).await;
                        return;
                    }
                };
                for event in parser.push(&chunk) {
                    if tx.send(Ok(event)).await.is_err() {
                        // Client hung up. Dropping the response cancels the HTTP request, which
                        // is what tells the engine to stop generating.
                        return;
                    }
                }
            }
            let _ = tx.send(Ok(parser.finish(fallback_prompt_tokens))).await;
        }))
    }

    /// The runtime identity of an engine we did not build.
    pub async fn descriptor(&self, accelerator_tag: &str) -> RuntimeDescriptor {
        let (commit, build_number) = self.version_output().await.map(|v| parse_version(&v)).unwrap_or((None, None));
        RuntimeDescriptor {
            backend: self.config.name.into(),
            engine_commit: commit.unwrap_or_else(|| "unknown".into()),
            // We did not build it, so we cannot claim it is unpatched. "unknown" is the honest
            // value and, being a distinct literal, it keeps an unidentified build from colliding
            // in `h_R` with a known-unpatched one.
            engine_patch_sha256: "unknown".into(),
            engine_build_number: build_number.unwrap_or(0),
            build_profile: format!(
                "external-binary/{}-{}/{}/unknown-flags/v1",
                std::env::consts::OS,
                std::env::consts::ARCH,
                accelerator_tag
            ),
            class_tag: format!(
                "misaka-studio/{}/{}-{}/{}/v1",
                self.config.name,
                std::env::consts::OS,
                std::env::consts::ARCH,
                accelerator_tag
            ),
        }
    }
}

enum Pipe {
    Out(tokio::process::ChildStdout),
    Err(tokio::process::ChildStderr),
}

async fn drain<R: tokio::io::AsyncRead + Unpin>(stream: R, log: Arc<Mutex<VecDeque<String>>>) {
    let mut lines = BufReader::new(stream).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::debug!(target: "engine", "{line}");
        ChildEngine::push_log(&log, line);
    }
}

/// Ask the OS for an unused port.
///
/// The bind-then-drop dance has a race — another process can take the port in between — but the
/// alternative (a fixed port) fails every time two Studios run at once, and the engine's own
/// failure to bind is immediate and reported.
fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| Error::io("127.0.0.1:0", e))?;
    let port = listener.local_addr().map_err(|e| Error::io("127.0.0.1:0", e))?.port();
    Ok(port)
}

fn request_body(request: &GenerationRequest, chat: bool) -> serde_json::Value {
    let p = &request.params;
    let mut body = serde_json::json!({
        "model": request.model,
        "stream": true,
        "stream_options": { "include_usage": true },
        "temperature": p.temperature,
        "top_p": p.top_p,
        "top_k": p.top_k,
        "min_p": p.min_p,
        "repeat_penalty": p.repeat_penalty,
        "max_tokens": p.max_tokens,
    });
    if let Some(seed) = p.seed {
        body["seed"] = serde_json::json!(seed);
    }
    if !request.stop.is_empty() {
        body["stop"] = serde_json::json!(request.stop);
    }
    if chat {
        body["messages"] = serde_json::json!(request.messages);
    } else {
        body["prompt"] = serde_json::json!(request.prompt.clone().unwrap_or_default());
    }
    body
}

/// `version: 4589 (a1b2c3d)` → `("a1b2c3d", 4589)`.
fn parse_version(text: &str) -> (Option<String>, Option<u64>) {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("version:") else { continue };
        let rest = rest.trim();
        let build = rest.split_whitespace().next().and_then(|n| n.parse::<u64>().ok());
        let commit = rest.split_once('(').and_then(|(_, c)| c.split_once(')')).map(|(c, _)| c.trim().to_string());
        if build.is_some() || commit.is_some() {
            return (commit, build);
        }
    }
    (None, None)
}

/// Server-sent-events parser for the OpenAI streaming shape.
///
/// Written by hand because the alternative is a dependency that mostly handles reconnection
/// semantics this never needs. The one thing it must get right is that **a chunk boundary can
/// land anywhere** — including inside a UTF-8 character or halfway through `data:` — so bytes
/// accumulate in a buffer and only whole lines are parsed.
struct SseParser {
    buffer: Vec<u8>,
    chat: bool,
    text_len: u64,
    usage: Option<Usage>,
    finish_reason: Option<String>,
}

impl SseParser {
    fn new(chat: bool) -> Self {
        SseParser { buffer: Vec::new(), chat, text_len: 0, usage: None, finish_reason: None }
    }

    fn push(&mut self, bytes: &[u8]) -> Vec<StreamEvent> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(idx) = self.buffer.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=idx).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim();
            let Some(payload) = line.strip_prefix("data:") else { continue };
            let payload = payload.trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }
            let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) else { continue };

            if let Some(usage) = json.get("usage").filter(|u| !u.is_null()) {
                self.usage = Some(Usage {
                    prompt_tokens: usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    completion_tokens: usage.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    total_tokens: usage.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                });
            }

            let Some(choice) = json.get("choices").and_then(|c| c.get(0)) else { continue };
            if let Some(reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                self.finish_reason = Some(reason.to_string());
            }
            let text = if self.chat {
                choice.get("delta").and_then(|d| d.get("content")).and_then(|v| v.as_str())
            } else {
                choice.get("text").and_then(|v| v.as_str())
            };
            if let Some(text) = text.filter(|t| !t.is_empty()) {
                self.text_len += text.chars().count() as u64;
                events.push(StreamEvent::Delta(text.to_string()));
            }
        }
        events
    }

    /// The terminating event, with the engine's usage when it sent one and an estimate when it
    /// did not — a blank tokens/sec readout is a worse answer than an approximate one, as long
    /// as it is never presented as exact.
    fn finish(self, fallback_prompt_tokens: u64) -> StreamEvent {
        let usage = self.usage.unwrap_or_else(|| {
            let completion = self.text_len.div_ceil(4);
            Usage {
                prompt_tokens: fallback_prompt_tokens,
                completion_tokens: completion,
                total_tokens: fallback_prompt_tokens + completion,
            }
        });
        StreamEvent::Done { usage, finish_reason: self.finish_reason.unwrap_or_else(|| "stop".into()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_banner_yields_a_commit_and_a_build_number() {
        let (commit, build) = parse_version("version: 4589 (a1b2c3d)\nbuilt with cc\n");
        assert_eq!(commit.as_deref(), Some("a1b2c3d"));
        assert_eq!(build, Some(4589));

        let (commit, build) = parse_version("some other program 1.2.3");
        assert_eq!(commit, None);
        assert_eq!(build, None);
    }

    /// The parser's real job: chunk boundaries fall wherever the network puts them.
    #[test]
    fn deltas_survive_being_split_across_chunks() {
        let mut parser = SseParser::new(true);
        let full = "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n";
        let bytes = full.as_bytes();
        let mut events = Vec::new();
        // One byte at a time — the most hostile split there is.
        for b in bytes {
            events.extend(parser.push(&[*b]));
        }
        assert_eq!(events, vec![StreamEvent::Delta("Hello".into()), StreamEvent::Delta(" world".into())]);
    }

    #[test]
    fn usage_is_taken_from_the_engine_when_it_sends_it() {
        let mut parser = SseParser::new(true);
        parser.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n");
        parser.push(b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,\"total_tokens\":18}}\n");
        parser.push(b"data: [DONE]\n");
        match parser.finish(999) {
            StreamEvent::Done { usage, finish_reason } => {
                assert_eq!(usage.prompt_tokens, 11);
                assert_eq!(usage.completion_tokens, 7);
                assert_eq!(finish_reason, "stop");
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    #[test]
    fn usage_falls_back_to_an_estimate_when_the_engine_is_silent() {
        let mut parser = SseParser::new(true);
        parser.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"12345678\"}}]}\n");
        match parser.finish(5) {
            StreamEvent::Done { usage, .. } => {
                assert_eq!(usage.prompt_tokens, 5);
                assert_eq!(usage.completion_tokens, 2, "8 characters ≈ 2 tokens");
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    #[test]
    fn completions_read_the_text_field_not_the_delta() {
        let mut parser = SseParser::new(false);
        let events = parser.push(b"data: {\"choices\":[{\"text\":\"raw\"}]}\n");
        assert_eq!(events, vec![StreamEvent::Delta("raw".into())]);
    }

    /// Garbage on the wire must not kill a generation that is otherwise fine.
    #[test]
    fn unparseable_lines_are_skipped() {
        let mut parser = SseParser::new(true);
        let events = parser.push(b": keep-alive\ndata: not json\ndata: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n");
        assert_eq!(events, vec![StreamEvent::Delta("ok".into())]);
    }

    #[test]
    fn the_request_body_carries_every_sampling_field() {
        let request = GenerationRequest {
            model: "m".into(),
            messages: vec![super::super::ChatMessage::new("user", "hi")],
            prompt: None,
            params: misaka_studio_core::provenance::SamplingCommitment { seed: Some(7), ..Default::default() },
            stop: vec!["</s>".into()],
        };
        let body = request_body(&request, true);
        assert_eq!(body["seed"], 7);
        assert_eq!(body["top_k"], 40);
        assert_eq!(body["stop"][0], "</s>");
        assert!(body["messages"].is_array());
        assert!(body.get("prompt").is_none(), "a chat request must not also send a raw prompt");
    }

    #[test]
    fn a_port_is_actually_free() {
        let port = free_port().expect("a port");
        assert!(port > 0);
        // Bindable, which is the only property that matters.
        let listener = std::net::TcpListener::bind(("127.0.0.1", port)).expect("binds");
        drop(listener);
    }
}
