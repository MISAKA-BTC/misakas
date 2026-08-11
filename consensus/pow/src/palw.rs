//! PALW (`algo_id = 4`) Layer-1 tag runner — the bridge between the Layer-0 PoW verifier and the
//! pinned deterministic LLM worker (`misaka-palw-worker`).
//!
//! One PoW attempt = one full inference: the 32-byte seed
//! (`pow_layer0::palw_pow_seed_v1` over network ∥ pre-PoW hash ∥ timestamp ∥ nonce) renders the
//! canonical prompt, the worker greedily decodes under the frozen
//! [`POW_L1_PALW_N_PREDICT_V1`] ceiling, and the replay-stable projection fields
//! (`output_commitment ∥ gemm_trace_root ∥ operation_schedule_commitment ∥ counts`) become the
//! 200-byte Layer-1 tag. Verification IS re-execution (the worker's `verify` mode is `self-job`
//! recomputed), which is the Open-then-Audit small-`q` full-replay regime.
//!
//! Modes, in resolution order:
//!  1. `MISAKA_PALW_POW_FIXTURE=1` — the in-process fixture tag
//!     (`pow_layer0::palw_fixture_l1_tag_v1`): CI/harness runs without the 1.2 GB model. A
//!     fixture node and a real-model node are DIFFERENT rule sets (different tags) and must not
//!     share a mesh — the `devnet-vlt-fixture` precedent.
//!  2. `PALW_WORKER=<path>` (+ `MISAKA_PALW_GGUF` consumed by the worker itself) — the real
//!     pinned runtime.
//!  3. Neither — [`PowLayer0Error::PalwUnavailable`]. On a PALW-active network this is a node
//!     configuration error, not a bad block: the consensus wrapper
//!     (`calc_block_level_check_pow_layer0`) escalates it to a panic rather than silently
//!     rejecting every valid header (the same fail-loud stance as the VLT devnet fence).
//!
//! Worker failures are never header-dependent: the prompt is a fixed ASCII frame around 64 hex
//! chars (comfortably under the ceiling — the worker's "prompt exceeds n_predict" death cannot
//! trigger), so a timeout / non-zero exit / unparseable document is environmental and surfaces
//! as [`PowLayer0Error::PalwWorkerFailed`], also escalated at the consensus boundary.

use kaspa_consensus_core::pow_layer0::{
    POW_L1_PALW_OLLAMA_OUT_BYTES, POW_L1_PALW_OUT_BYTES, PowLayer0Error, palw_fixture_l1_tag_v1,
    palw_ollama_fixture_l1_tag_v1, palw_pow_seed_v1,
};
use kaspa_hashes::Hash64;

/// Path to the `palw-worker` binary (the same variable the VLT compute runtime uses).
pub const PALW_WORKER_ENV: &str = "PALW_WORKER";
/// `"1"` selects the in-process fixture tag (no model, no subprocess).
pub const PALW_FIXTURE_ENV: &str = "MISAKA_PALW_POW_FIXTURE";
/// Per-inference wall-clock budget in seconds (default [`DEFAULT_TIMEOUT_SECS`]). Generous vs the
/// ~1-3 s a pinned Qwen3.5-2B attempt takes — it exists to reap a wedged worker, not to pace one.
pub const PALW_TIMEOUT_ENV: &str = "MISAKA_PALW_POW_TIMEOUT_SECS";
pub const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// Ollama model reference for the `algo_id = 5` runtime (e.g. `qwen3.5:2b`). Required on a
/// PALW-Ollama network; the operator pins the model DIGEST at deploy time (the runbook records
/// it) — every fleet host must serve the same blob.
pub const PALW_OLLAMA_MODEL_ENV: &str = "MISAKA_PALW_OLLAMA_MODEL";
/// Base URL of the host-local Ollama server (default [`DEFAULT_OLLAMA_URL`]).
pub const PALW_OLLAMA_URL_ENV: &str = "MISAKA_PALW_OLLAMA_URL";
pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";

/// Whether the fixture tag is selected in this process.
pub fn fixture_enabled() -> bool {
    std::env::var(PALW_FIXTURE_ENV).as_deref() == Ok("1")
}

/// Verify that the Ollama server at `url` serves the **pinned** model blob for `algo_id = 5`
/// (`POW_L1_PALW_OLLAMA_MODEL_DIGEST_V1` + size). A different blob computes different tags: the
/// node would reject every honest block and have its own rejected — a silent one-host fork that
/// looks like a network fault. Called eagerly by the kaspad startup rail (good message, before
/// any peer is dialed) and lazily, once per process, by the tag runner (so a miner, a test
/// harness or any other consumer cannot skip it).
///
/// `model` is the Ollama reference (`qwen3.5:2b`); the pin is on the BLOB, so a re-tagged copy
/// under another name verifies fine — which is correct, the name is not the algorithm.
#[cfg(not(target_arch = "wasm32"))]
pub fn verify_ollama_model_pin(url: &str, model: &str) -> Result<(), PowLayer0Error> {
    native::verify_model_pin(url, model)
}

/// The PALW Layer-1 tag for one (header, nonce) attempt. Deterministic across every conforming
/// node: fixture nodes derive it from the seed alone; real nodes replay the pinned inference.
/// Real-mode results are cached by seed, so the header pipeline, block-level derivation and
/// pruning-proof path pay for a given attempt's inference once per process.
pub fn palw_l1_tag(
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    network_id: &[u8],
) -> Result<[u8; POW_L1_PALW_OUT_BYTES], PowLayer0Error> {
    let seed = palw_pow_seed_v1(pre_pow_hash, timestamp, nonce, network_id);
    if fixture_enabled() {
        return Ok(palw_fixture_l1_tag_v1(&seed));
    }
    native::tag_for_seed(&seed)
}

/// The PALW-Ollama (`algo_id = 5`) Layer-1 tag for one (header, nonce) attempt. Same seed and
/// canonical prompt as algo 4; the inference runs on the host-local Ollama server and the tag
/// commits to the greedy response bytes + token counts. Cached by seed like the worker tag.
pub fn palw_ollama_l1_tag(
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    network_id: &[u8],
) -> Result<[u8; POW_L1_PALW_OLLAMA_OUT_BYTES], PowLayer0Error> {
    let seed = palw_pow_seed_v1(pre_pow_hash, timestamp, nonce, network_id);
    if fixture_enabled() {
        return Ok(palw_ollama_fixture_l1_tag_v1(&seed));
    }
    native::ollama_tag_for_seed(&seed)
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use kaspa_consensus_core::pow_layer0::{POW_L1_PALW_N_PREDICT_V1, palw_l1_tag_from_projection, palw_pow_prompt_v1};
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    /// Completed tags by seed. Bounded by wholesale clearing: tags are pure functions of the seed
    /// and recomputable, so eviction precision is not worth an LRU here.
    static TAG_CACHE: OnceLock<Mutex<HashMap<[u8; 32], [u8; POW_L1_PALW_OUT_BYTES]>>> = OnceLock::new();
    const TAG_CACHE_MAX: usize = 8_192;

    /// Serializes worker spawns. Header validation fans out across thread pools (and the
    /// pruning-proof path validates headers in parallel); each worker process loads the 1.2 GB
    /// model, so unbounded concurrency is a memory cliff, not a speedup. Metal-side determinism
    /// is concurrency-safe (verified 5-way in the VLT work) — this gate is purely resource
    /// control. Duplicate concurrent computations of the SAME seed are not deduplicated (rare —
    /// callers are validating distinct headers), merely serialized.
    static SPAWN_GATE: Mutex<()> = Mutex::new(());

    fn cache() -> &'static Mutex<HashMap<[u8; 32], [u8; POW_L1_PALW_OUT_BYTES]>> {
        TAG_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub(super) fn tag_for_seed(seed: &[u8; 32]) -> Result<[u8; POW_L1_PALW_OUT_BYTES], PowLayer0Error> {
        if let Some(tag) = cache().lock().unwrap().get(seed) {
            return Ok(*tag);
        }
        let worker = std::env::var(PALW_WORKER_ENV).map_err(|_| {
            PowLayer0Error::PalwUnavailable(format!(
                "{PALW_WORKER_ENV} is not set; point it at target/release/palw-worker (and set MISAKA_PALW_GGUF), \
                 or run with {PALW_FIXTURE_ENV}=1 for the model-free fixture"
            ))
        })?;
        let tag = {
            let _gate = SPAWN_GATE.lock().unwrap();
            // Re-check under the gate: the seed may have been computed while we queued.
            if let Some(tag) = cache().lock().unwrap().get(seed) {
                return Ok(*tag);
            }
            run_worker(&worker, seed)?
        };
        let mut cache = cache().lock().unwrap();
        if cache.len() >= TAG_CACHE_MAX {
            cache.clear();
        }
        cache.insert(*seed, tag);
        Ok(tag)
    }

    fn timeout() -> Duration {
        Duration::from_secs(
            std::env::var(PALW_TIMEOUT_ENV).ok().and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_TIMEOUT_SECS),
        )
    }

    fn run_worker(worker: &str, seed: &[u8; 32]) -> Result<[u8; POW_L1_PALW_OUT_BYTES], PowLayer0Error> {
        let prompt = palw_pow_prompt_v1(seed);
        let started = Instant::now();
        let mut child = Command::new(worker)
            .args(["--mode", "verify", "--prompt-stdin", "--n-predict", &POW_L1_PALW_N_PREDICT_V1.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| PowLayer0Error::PalwUnavailable(format!("cannot spawn PALW worker at {worker}: {e}")))?;

        // Feed the prompt and close stdin so the worker sees EOF.
        {
            let mut stdin = child.stdin.take().expect("stdin was piped above");
            stdin
                .write_all(prompt.as_bytes())
                .map_err(|e| PowLayer0Error::PalwWorkerFailed(format!("cannot write the prompt to the worker: {e}")))?;
        }

        // Drain both pipes on threads BEFORE waiting. llama.cpp's model-load stderr alone can
        // exceed the 64 KiB pipe buffer; polling `try_wait` without draining deadlocks the worker
        // in write() — the exact pipe-buffer trap that cost the first real-model VLT run.
        let mut stdout_pipe = child.stdout.take().expect("stdout was piped above");
        let mut stderr_pipe = child.stderr.take().expect("stderr was piped above");
        let stdout_reader = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout_pipe.read_to_end(&mut buf);
            buf
        });
        let stderr_reader = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stderr_pipe.read_to_end(&mut buf);
            buf
        });

        let deadline = started + timeout();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(PowLayer0Error::PalwWorkerFailed(format!(
                            "PALW worker exceeded the {:?} budget and was killed (raise {PALW_TIMEOUT_ENV} if the \
                             machine is genuinely this slow)",
                            timeout()
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => return Err(PowLayer0Error::PalwWorkerFailed(format!("wait on the PALW worker failed: {e}"))),
            }
        };
        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();

        if !status.success() {
            let tail: String = String::from_utf8_lossy(&stderr).chars().rev().take(400).collect::<Vec<_>>().into_iter().rev().collect();
            return Err(PowLayer0Error::PalwWorkerFailed(format!("worker exited with {status}: …{}", tail.trim())));
        }
        let tag = parse_projection(&stdout)?;
        log::debug!("palw-pow: inference attempt completed in {:?}", started.elapsed());
        Ok(tag)
    }

    // ── PALW-Ollama (algo_id = 5): host-local HTTP inference ────────────────────────────────────

    /// Completed Ollama tags by seed — separate from the worker cache (same seed under the two
    /// algos is a different computation).
    static OLLAMA_TAG_CACHE: OnceLock<Mutex<HashMap<[u8; 32], [u8; POW_L1_PALW_OLLAMA_OUT_BYTES]>>> = OnceLock::new();

    fn ollama_cache() -> &'static Mutex<HashMap<[u8; 32], [u8; POW_L1_PALW_OLLAMA_OUT_BYTES]>> {
        OLLAMA_TAG_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub(super) fn ollama_tag_for_seed(seed: &[u8; 32]) -> Result<[u8; POW_L1_PALW_OLLAMA_OUT_BYTES], PowLayer0Error> {
        if let Some(tag) = ollama_cache().lock().unwrap().get(seed) {
            return Ok(*tag);
        }
        let model = std::env::var(PALW_OLLAMA_MODEL_ENV).map_err(|_| {
            PowLayer0Error::PalwUnavailable(format!(
                "{PALW_OLLAMA_MODEL_ENV} is not set; point it at the fleet's pinned Qwen model \
                 (e.g. qwen3.5:2b) served by a local Ollama, or run with {PALW_FIXTURE_ENV}=1 \
                 (devnet only) for the model-free fixture"
            ))
        })?;
        let url = std::env::var(PALW_OLLAMA_URL_ENV).unwrap_or_else(|_| DEFAULT_OLLAMA_URL.to_string());
        // The blob check happens ONCE per process, before the first tag. Doing it here rather
        // than only in the kaspad rail means every consumer — miner, harness, test — is covered
        // by construction; a wrong blob must never silently mint tags no peer agrees with.
        verify_model_pin_once(&url, &model)?;
        let tag = {
            let _gate = SPAWN_GATE.lock().unwrap();
            if let Some(tag) = ollama_cache().lock().unwrap().get(seed) {
                return Ok(*tag);
            }
            run_ollama(&url, &model, seed)?
        };
        let mut cache = ollama_cache().lock().unwrap();
        if cache.len() >= TAG_CACHE_MAX {
            cache.clear();
        }
        cache.insert(*seed, tag);
        Ok(tag)
    }

    /// Memoized `verify_model_pin`: the blob cannot change under a running server without a
    /// restart of `ollama pull`, and re-checking per attempt would put an HTTP round-trip in the
    /// mining hot loop. A FAILURE is memoized too — a wrong blob is a configuration fact, and
    /// re-querying it thousands of times per minute helps nobody.
    static MODEL_PIN_VERIFIED: OnceLock<Result<(), String>> = OnceLock::new();

    fn verify_model_pin_once(url: &str, model: &str) -> Result<(), PowLayer0Error> {
        MODEL_PIN_VERIFIED
            .get_or_init(|| verify_model_pin(url, model).map_err(|e| e.to_string()))
            .clone()
            .map_err(PowLayer0Error::PalwUnavailable)
    }

    pub(super) fn verify_model_pin(url: &str, model: &str) -> Result<(), PowLayer0Error> {
        use kaspa_consensus_core::pow_layer0::{POW_L1_PALW_OLLAMA_MODEL_DIGEST_V1, POW_L1_PALW_OLLAMA_MODEL_SIZE_V1};
        let body = http_request("GET", url, "/api/tags", None, Duration::from_secs(15))?;
        let doc: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|e| PowLayer0Error::PalwUnavailable(format!("cannot parse the Ollama model list: {e}")))?;
        let models = doc
            .get("models")
            .and_then(|v| v.as_array())
            .ok_or_else(|| PowLayer0Error::PalwUnavailable("Ollama /api/tags has no `models` array".into()))?;
        // Ollama reports `qwen3.5:2b` for a `qwen3.5:2b` pull and appends `:latest` for a bare
        // name, so accept the exact ref or the `:latest` expansion of it.
        let wanted: Vec<String> =
            if model.contains(':') { vec![model.to_owned()] } else { vec![model.to_owned(), format!("{model}:latest")] };
        let entry = models
            .iter()
            .find(|m| m.get("name").and_then(|v| v.as_str()).is_some_and(|n| wanted.iter().any(|w| w == n)))
            .ok_or_else(|| {
                PowLayer0Error::PalwUnavailable(format!(
                    "the Ollama server at {url} does not serve model {model} — pull it first: `ollama pull {model}`"
                ))
            })?;
        let digest = entry.get("digest").and_then(|v| v.as_str()).unwrap_or_default();
        let size = entry.get("size").and_then(|v| v.as_u64()).unwrap_or_default();
        if digest != POW_L1_PALW_OLLAMA_MODEL_DIGEST_V1 || size != POW_L1_PALW_OLLAMA_MODEL_SIZE_V1 {
            return Err(PowLayer0Error::PalwUnavailable(format!(
                "model {model} on {url} is blob {digest} ({size} bytes), but PALW-Ollama v1 is pinned to \
                 {POW_L1_PALW_OLLAMA_MODEL_DIGEST_V1} ({POW_L1_PALW_OLLAMA_MODEL_SIZE_V1} bytes). A different blob \
                 computes different tags: this node would reject every honest block and have its own rejected. \
                 Re-pull the pinned model, or run a network whose PALW pin matches this blob."
            )));
        }
        Ok(())
    }

    fn run_ollama(url: &str, model: &str, seed: &[u8; 32]) -> Result<[u8; POW_L1_PALW_OLLAMA_OUT_BYTES], PowLayer0Error> {
        use kaspa_consensus_core::pow_layer0::{POW_L1_PALW_OLLAMA_NUM_PREDICT_V1, palw_ollama_l1_tag_from_response};
        let prompt = kaspa_consensus_core::pow_layer0::palw_pow_prompt_v1(seed);
        // Consensus-frozen request shape: raw continuation (no chat template), greedy
        // (temperature 0), the v1 decode budget, a fixed context size. Every option here is part
        // of what the network's runtime class reproduces — do not make these configurable.
        let body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "raw": true,
            "stream": false,
            "options": { "temperature": 0.0, "num_predict": POW_L1_PALW_OLLAMA_NUM_PREDICT_V1, "num_ctx": 4096, "seed": 0 },
        })
        .to_string();
        let started = Instant::now();
        let response = http_request("POST", url, "/api/generate", Some(&body), timeout())?;
        let doc: serde_json::Value = serde_json::from_slice(&response)
            .map_err(|e| PowLayer0Error::PalwWorkerFailed(format!("cannot parse the Ollama response: {e}")))?;
        if let Some(err) = doc.get("error").and_then(|v| v.as_str()) {
            return Err(PowLayer0Error::PalwWorkerFailed(format!(
                "Ollama refused the generate request: {err} (model {model} pulled? `ollama pull {model}`)"
            )));
        }
        let text = doc
            .get("response")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PowLayer0Error::PalwWorkerFailed("Ollama response lacks the `response` field".into()))?;
        let prompt_eval = doc.get("prompt_eval_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let eval = doc.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        log::debug!("palw-ollama: inference attempt completed in {:?} (prompt_eval={prompt_eval} eval={eval})", started.elapsed());
        Ok(palw_ollama_l1_tag_from_response(text.as_bytes(), prompt_eval, eval))
    }

    /// Minimal blocking HTTP/1.1 request for the host-local Ollama endpoint. Deliberately not a
    /// full client: `http://host:port` only, `Connection: close`, handles the two body framings
    /// Ollama uses (Content-Length and chunked). Keeps reqwest-class dependency weight out of
    /// the consensus tree. `body = None` sends a bodyless request (GET).
    fn http_request(
        method: &str,
        base_url: &str,
        path: &str,
        body: Option<&str>,
        budget: Duration,
    ) -> Result<Vec<u8>, PowLayer0Error> {
        use std::net::TcpStream;
        let hostport = base_url
            .strip_prefix("http://")
            .ok_or_else(|| PowLayer0Error::PalwUnavailable(format!("{PALW_OLLAMA_URL_ENV} must be http://host:port, got {base_url}")))?
            .trim_end_matches('/');
        let mut stream = TcpStream::connect(hostport).map_err(|e| {
            PowLayer0Error::PalwUnavailable(format!(
                "cannot reach the Ollama server at {hostport}: {e} (is `ollama serve` running?)"
            ))
        })?;
        stream.set_read_timeout(Some(budget)).ok();
        stream.set_write_timeout(Some(Duration::from_secs(10))).ok();
        let request = match body {
            Some(body) => format!(
                "{method} {path} HTTP/1.1\r\nHost: {hostport}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
            None => format!("{method} {path} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\n\r\n"),
        };
        stream
            .write_all(request.as_bytes())
            .map_err(|e| PowLayer0Error::PalwWorkerFailed(format!("cannot send the Ollama request: {e}")))?;
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).map_err(|e| {
            PowLayer0Error::PalwWorkerFailed(format!("reading the Ollama response failed (budget {budget:?}): {e}"))
        })?;
        let header_end = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or_else(|| PowLayer0Error::PalwWorkerFailed("malformed HTTP response from Ollama (no header end)".into()))?;
        let (head, rest) = raw.split_at(header_end + 4);
        let head_text = String::from_utf8_lossy(head);
        let status = head_text.lines().next().unwrap_or_default().to_string();
        if !status.contains(" 200 ") {
            let tail: String = String::from_utf8_lossy(rest).chars().take(300).collect();
            return Err(PowLayer0Error::PalwWorkerFailed(format!("Ollama answered {status}: {tail}")));
        }
        let chunked = head_text.to_ascii_lowercase().contains("transfer-encoding: chunked");
        if !chunked {
            return Ok(rest.to_vec());
        }
        // De-chunk: size lines are hex; a 0-size chunk terminates.
        let mut out = Vec::with_capacity(rest.len());
        let mut i = 0;
        while i < rest.len() {
            let line_end = match rest[i..].windows(2).position(|w| w == b"\r\n") {
                Some(p) => i + p,
                None => break,
            };
            let size = usize::from_str_radix(String::from_utf8_lossy(&rest[i..line_end]).trim(), 16)
                .map_err(|_| PowLayer0Error::PalwWorkerFailed("malformed chunk size from Ollama".into()))?;
            if size == 0 {
                break;
            }
            let start = line_end + 2;
            let end = (start + size).min(rest.len());
            out.extend_from_slice(&rest[start..end]);
            i = end + 2;
        }
        Ok(out)
    }

    /// Parse the worker's `misaka.palw.testnet-submission.v3` document (the LAST non-empty stdout
    /// line) into the 200-byte tag.
    fn parse_projection(stdout: &[u8]) -> Result<[u8; POW_L1_PALW_OUT_BYTES], PowLayer0Error> {
        let text = String::from_utf8_lossy(stdout);
        let line = text
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .ok_or_else(|| PowLayer0Error::PalwWorkerFailed("worker produced no stdout document".into()))?;
        let doc: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| PowLayer0Error::PalwWorkerFailed(format!("cannot parse the worker document: {e}")))?;
        let hash_field = |name: &str| -> Result<Hash64, PowLayer0Error> {
            let hex = doc
                .get(name)
                .and_then(|v| v.as_str())
                .ok_or_else(|| PowLayer0Error::PalwWorkerFailed(format!("worker document lacks the {name} field")))?;
            let mut bytes = [0u8; 64];
            faster_hex::hex_decode(hex.as_bytes(), &mut bytes)
                .map_err(|e| PowLayer0Error::PalwWorkerFailed(format!("worker {name} is not 64-byte hex: {e}")))?;
            Ok(Hash64::from_bytes(bytes))
        };
        let count_field = |name: &str| -> Result<u32, PowLayer0Error> {
            doc.get(name)
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| PowLayer0Error::PalwWorkerFailed(format!("worker document lacks a u32 {name} field")))
        };
        Ok(palw_l1_tag_from_projection(
            &hash_field("output_commitment")?,
            &hash_field("gemm_trace_root")?,
            &hash_field("operation_schedule_commitment")?,
            count_field("prefill_tokens")?,
            count_field("decode_tokens")?,
        ))
    }
}

#[cfg(target_arch = "wasm32")]
mod native {
    use super::*;

    pub(super) fn tag_for_seed(_seed: &[u8; 32]) -> Result<[u8; POW_L1_PALW_OUT_BYTES], PowLayer0Error> {
        Err(PowLayer0Error::PalwUnavailable("PALW (algo_id = 4) PoW cannot run in a wasm build".into()))
    }

    pub(super) fn ollama_tag_for_seed(_seed: &[u8; 32]) -> Result<[u8; POW_L1_PALW_OLLAMA_OUT_BYTES], PowLayer0Error> {
        Err(PowLayer0Error::PalwUnavailable("PALW-Ollama (algo_id = 5) PoW cannot run in a wasm build".into()))
    }
}
