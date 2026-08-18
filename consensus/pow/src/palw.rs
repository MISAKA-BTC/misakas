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
    POW_L1_PALW_OLLAMA_OUT_BYTES, POW_L1_PALW_OUT_BYTES, POW_L1_PALW_PROBE_SEED_V1, PowLayer0Error, palw_fixture_l1_tag_v1,
    palw_ollama_fixture_l1_tag_v1, palw_pow_seed_v1, palw_worker_calibration_v1,
};
use kaspa_hashes::Hash64;

/// Path to the `palw-worker` binary (the same variable the VLT compute runtime uses).
pub const PALW_WORKER_ENV: &str = "PALW_WORKER";
/// `"1"` selects the in-process fixture tag (no model, no subprocess).
pub const PALW_FIXTURE_ENV: &str = "MISAKA_PALW_POW_FIXTURE";
/// `"1"` runs verification through a RESIDENT agent — one `palw-worker --mode pow-agent` child
/// that holds the model for the life of the node — instead of one process per seed (ADR-0041
/// Decision 1′). Opt-in: it is a local sync policy with no consensus effect, but it is also a new
/// long-lived subprocess on the block-validation path, so a fleet turns it on deliberately. Any
/// failure falls back to the one-shot path, so the worst case of enabling it is the cost of the
/// path that ships today.
pub const PALW_AGENT_ENV: &str = "MISAKA_PALW_AGENT";
/// Upper bound on PALW inferences in flight in this process (default [`DEFAULT_CONCURRENCY`]).
///
/// Header validation is a burst load exactly once — a from-genesis sync, where a pruning proof buys
/// one inference per header — and a trickle forever after (one per block interval). This knob is
/// for the burst (ADR-0041 Decision 2). It changes NOTHING about what is accepted, only how many
/// workers may run at once, and it costs memory linearly: each concurrent inference holds a
/// resident 1.2 GB model, so `N` here means roughly `N × 1.4 GiB`.
pub const PALW_CONCURRENCY_ENV: &str = "MISAKA_PALW_CONCURRENCY";
/// Directory of host-wide inference slots. Set it to one directory shared by EVERY PALW process on
/// a machine and the concurrency bound covers the machine instead of one process.
///
/// [`PALW_CONCURRENCY_ENV`] alone bounds a process, and the resource it protects is the host: two
/// PALW nodes on one box are two concurrent inferences whatever either process's semaphore says,
/// and that configuration measured **0.38× the serial throughput** (ADR-0041 Decision 2) — entered
/// by co-locating nodes, without anyone touching the knob. With this set, a permit additionally
/// requires an exclusive `flock` on one of `MISAKA_PALW_CONCURRENCY` slot files in the directory.
///
/// `flock` rather than a PID file or a named semaphore for one reason: **the kernel releases it when
/// the holder dies.** A crashed node must not permanently consume a slot, and both alternatives leak
/// one. Unix only; elsewhere the variable is reported as unsupported and ignored.
///
/// Waiting for a slot is unbounded, exactly as waiting on the in-process semaphore already is — a
/// held slot means a live process is inferring, which is when queueing is the correct answer. A
/// wait past a minute logs which directory is full.
pub const PALW_LEASE_DIR_ENV: &str = "MISAKA_PALW_LEASE_DIR";
/// One — the serialized behaviour this path had before Decision 2. It stays the default because
/// raising it is a memory decision only an operator can make.
pub const DEFAULT_CONCURRENCY: usize = 1;

/// How many PALW inferences this process may run at once. Read once, on first use.
///
/// Also the batch size the pruning-proof validator prefetches header PoW in, so the bound and its
/// consumer cannot drift apart.
pub fn inference_concurrency() -> usize {
    use std::sync::OnceLock;
    static RESOLVED: OnceLock<usize> = OnceLock::new();
    *RESOLVED.get_or_init(|| {
        std::env::var(PALW_CONCURRENCY_ENV)
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|n| *n >= 1)
            .unwrap_or(DEFAULT_CONCURRENCY)
    })
}

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
    native::verify_model_pin(url, model)?;
    native::verify_calibration(url, model)
}

/// One deterministic generation against the pinned runtime — the same call the PoW path makes,
/// exposed so anything else that wants a REPRODUCIBLE answer from this network's model shares one
/// definition instead of writing its own HTTP client and its own idea of the options.
///
/// Returns `(response text, prompt_eval_count, eval_count)`.
///
/// `templated = false` reproduces the consensus request exactly (raw continuation — what a PoW
/// attempt is). `templated = true` applies the model's own chat template, which is what a person
/// asking a question wants; it is equally deterministic (the template lives inside the pinned
/// blob) but it is a DIFFERENT computation, so any receipt must record which mode produced it.
///
/// `think` is `None` on the consensus path — the field is then omitted entirely, keeping the
/// request byte-identical to what block validation sends. A caller that sets it is choosing
/// whether this thinking-capable model reasons before answering, which changes the output and so
/// belongs in that caller's receipt too.
///
/// Everything else stays at the consensus values on purpose — greedy (temperature 0), the CPU
/// backend, the 4096-token context. Those are what make an answer reproducible on another machine
/// of the same class, so they are not parameters.
#[cfg(not(target_arch = "wasm32"))]
pub fn palw_generate(
    url: &str,
    model: &str,
    prompt: &str,
    num_predict: u32,
    templated: bool,
    think: Option<bool>,
) -> Result<(String, u32, u32), PowLayer0Error> {
    native::generate(url, model, prompt, num_predict, templated, think)
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
    // Class-pinned networks verify the runtime ONCE per process, in the path every consumer —
    // node validation, miner, pruning proof — must pass through, so nothing can skip it. This
    // runs BEFORE the fixture branch on purpose: the fixture tag family must fail a class-pinned
    // net loudly instead of minting tags no real peer accepts (class-less nets return instantly).
    verify_worker_calibration(network_id)?;
    if fixture_enabled() {
        return Ok(palw_fixture_l1_tag_v1(&seed));
    }
    native::tag_for_seed(&seed)
}

/// Verify (once per process, memoized — failure too) that this host's worker runtime is in the
/// determinism class `network_id` pins, by replaying the canonical probe seed through the
/// ordinary tag path and comparing the 200-byte tag against the network's pinned calibration.
/// Networks that pin no class (devnet) pass trivially. The algo-4 mirror of
/// [`verify_ollama_model_pin`]'s calibration half: without it, an out-of-class runtime starts
/// happily and then silently forks — rejecting every honest block, having its own rejected —
/// with nothing pointing at the cause. Called eagerly by the kaspad startup rail (good message,
/// before any peer is dialed) and lazily by [`palw_l1_tag`] (so no consumer can skip it).
/// Costs one inference on first use; the probe tag lands in the ordinary seed cache.
pub fn verify_worker_calibration(network_id: &[u8]) -> Result<(), PowLayer0Error> {
    let Some(expected) = palw_worker_calibration_v1(network_id) else {
        return Ok(());
    };
    if fixture_enabled() {
        // The fixture is its own (model-free) tag family, permitted on devnet-class nets only —
        // and those pin no class. A class-pinned net running the fixture must fail the probe
        // loudly rather than mint fixture tags no real peer accepts.
        return Err(PowLayer0Error::PalwUnavailable(
            "this network pins a worker determinism class; the MISAKA_PALW_POW_FIXTURE tag family cannot join it".into(),
        ));
    }
    native::worker_calibration_once(expected)
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
    use std::sync::{Condvar, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    /// Completed tags by seed. Bounded by wholesale clearing: tags are pure functions of the seed
    /// and recomputable, so eviction precision is not worth an LRU here.
    static TAG_CACHE: OnceLock<Mutex<HashMap<[u8; 32], [u8; POW_L1_PALW_OUT_BYTES]>>> = OnceLock::new();
    const TAG_CACHE_MAX: usize = 8_192;

    /// Bounds inferences in flight. Until ADR-0041 Decision 2 this was a plain mutex — exactly
    /// one at a time — and it stays that by default; `MISAKA_PALW_CONCURRENCY` raises the permit
    /// count for a sync burst.
    ///
    /// Header validation fans out across thread pools (and the pruning-proof validator prefetches
    /// header PoW in parallel batches); each concurrent inference holds a 1.2 GB model, so
    /// unbounded concurrency is a memory cliff, not a speedup. Determinism is NOT what this
    /// protects — the tag is a pure function of the seed, and Metal-side determinism was verified
    /// 5-way in the VLT work — it is purely resource control. Duplicate concurrent computations of
    /// the SAME seed are not deduplicated (rare: callers validate distinct headers), merely bounded.
    struct Gate {
        permits: Mutex<usize>,
        released: Condvar,
    }

    static GATE: OnceLock<Gate> = OnceLock::new();

    fn gate() -> &'static Gate {
        GATE.get_or_init(|| Gate { permits: Mutex::new(inference_concurrency()), released: Condvar::new() })
    }

    /// Held for one inference; returns its permit on drop, INCLUDING during a panic unwind. That
    /// matters here: this path panics on an unusable runtime, and a permit leaked on that route
    /// would wedge every other validating thread behind a node that is already failing loudly.
    struct Permit {
        /// The host-wide slot, when [`PALW_LEASE_DIR_ENV`] is configured. Dropping the file releases
        /// the `flock`; so does the process dying, which is the property that makes this safe to
        /// hold across a panic, a kill, or an OOM.
        _slot: Option<std::fs::File>,
    }

    impl Drop for Permit {
        fn drop(&mut self) {
            let gate = gate();
            // Poison recovery rather than `unwrap`: this can run during unwind, where a second
            // panic aborts the process instead of letting the first one be the story.
            *gate.permits.lock().unwrap_or_else(|e| e.into_inner()) += 1;
            gate.released.notify_one();
        }
    }

    fn acquire_permit() -> Permit {
        {
            let gate = gate();
            let mut permits = gate.permits.lock().unwrap_or_else(|e| e.into_inner());
            while *permits == 0 {
                permits = gate.released.wait(permits).unwrap_or_else(|e| e.into_inner());
            }
            *permits -= 1;
            // The guard is dropped HERE, before the host slot is waited on. Blocking on a
            // cross-process lock while holding the semaphore's own mutex would stop every other
            // thread in this process from so much as checking the count.
        }
        // The in-process count is spoken for from here on, so build the guard that returns it
        // BEFORE doing anything that can block or unwind.
        let mut permit = Permit { _slot: None };
        permit._slot = acquire_host_slot();
        permit
    }

    /// An exclusive `flock` on one of `slots` files in `dir`, waiting until one is free.
    ///
    /// Takes `slots` explicitly rather than reading [`inference_concurrency`] so the mechanism can
    /// be tested without touching process-global state.
    #[cfg(unix)]
    fn host_slot(dir: &std::path::Path, slots: usize) -> Option<std::fs::File> {
        use std::os::unix::io::AsRawFd;
        if let Err(e) = std::fs::create_dir_all(dir) {
            warn_lease_unusable(format!("cannot create the PALW lease directory {}: {e}", dir.display()));
            return None;
        }
        let started = Instant::now();
        let mut warned = false;
        loop {
            for i in 0..slots.max(1) {
                let path = dir.join(format!("slot-{i}.lock"));
                // `truncate(false)`: the slot file is a lock handle, never a data file. Truncating
                // it would be harmless today (it is empty) and wrong the moment anyone writes
                // anything into it, so say what is meant.
                let file = match std::fs::OpenOptions::new().create(true).read(true).write(true).truncate(false).open(&path) {
                    Ok(file) => file,
                    Err(e) => {
                        warn_lease_unusable(format!("cannot open the PALW lease slot {}: {e}", path.display()));
                        return None;
                    }
                };
                // SAFETY: `file` owns the descriptor and outlives the call; `flock` on a valid fd
                // has no other precondition.
                if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                    return Some(file);
                }
            }
            if !warned && started.elapsed() > Duration::from_secs(60) {
                log::warn!(
                    "palw-pow: waiting over a minute for a host inference slot — all {slots} in {} are held by other \
                     PALW processes on this machine (this is the bound working, but check whether the co-location is \
                     intended: it measured 0.38x serial throughput)",
                    dir.display()
                );
                warned = true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// A misconfigured lease directory disables the HOST bound and leaves the per-process one — a
    /// performance control failing open, which is right (refusing to validate over a lock directory
    /// would wedge the node) but must be loud, once.
    fn warn_lease_unusable(message: String) {
        static WARNED: OnceLock<()> = OnceLock::new();
        WARNED.get_or_init(|| {
            log::warn!("palw-pow: {message}; the host-wide inference bound is NOT in effect on this process");
        });
    }

    #[cfg(unix)]
    fn acquire_host_slot() -> Option<std::fs::File> {
        let dir = std::env::var(PALW_LEASE_DIR_ENV).ok()?;
        host_slot(std::path::Path::new(&dir), inference_concurrency())
    }

    #[cfg(not(unix))]
    fn acquire_host_slot() -> Option<std::fs::File> {
        if std::env::var(PALW_LEASE_DIR_ENV).is_ok() {
            warn_lease_unusable(format!("{PALW_LEASE_DIR_ENV} needs flock and this is not a unix host"));
        }
        None
    }

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
            let _permit = acquire_permit();
            // Re-check under the gate: the seed may have been computed while we queued.
            if let Some(tag) = cache().lock().unwrap().get(seed) {
                return Ok(*tag);
            }
            // The resident agent when the operator enabled it and it can serve; the one-shot path
            // that ships today otherwise. Inside the gate on purpose: this landing amortises the
            // per-seed cost (ADR-0041 Decision 1′) and deliberately does not touch the concurrency
            // policy (Decision 2), so exactly one inference is in flight either way and the two
            // paths are measured under the same load.
            match resident::tag(&worker, seed) {
                Some(tag) => tag,
                None => run_worker_with_retry(&worker, seed)?,
            }
        };
        let mut cache = cache().lock().unwrap();
        if cache.len() >= TAG_CACHE_MAX {
            cache.clear();
        }
        cache.insert(*seed, tag);
        Ok(tag)
    }

    /// Attempts a `PalwWorkerFailed` gets before it is treated as permanent (ADR-0036 Decision 4).
    ///
    /// The tag is a pure function of the seed, so a retry is free of correctness risk: it either
    /// produces the same tag or it fails again. What it buys is the distinction the caller cannot
    /// otherwise make — `PalwWorkerFailed` covers a *transient* fault (a spawn failure, an OOM
    /// kill, or a timeout under validation load) as well as a permanent one, and the caller
    /// `calc_block_level_check_pow_layer0` panics on it. Panicking a node because one subprocess
    /// lost a race under load is a self-inflicted outage; three attempts convert the common
    /// transient into a delay, and a genuinely broken runtime still reaches the panic.
    const WORKER_ATTEMPTS: u32 = 3;

    fn run_worker_with_retry(worker: &str, seed: &[u8; 32]) -> Result<[u8; POW_L1_PALW_OUT_BYTES], PowLayer0Error> {
        let mut last = None;
        for attempt in 1..=WORKER_ATTEMPTS {
            match run_worker(worker, seed) {
                Ok(tag) => return Ok(tag),
                // A missing/unspawnable worker is a configuration fault: retrying cannot fix it
                // and would only delay the operator's error message.
                Err(e @ PowLayer0Error::PalwUnavailable(_)) => return Err(e),
                Err(e) => {
                    if attempt < WORKER_ATTEMPTS {
                        // Linear backoff: the common cause is transient memory or CPU pressure
                        // from concurrent validation, which needs time more than it needs jitter.
                        std::thread::sleep(Duration::from_millis(250 * attempt as u64));
                    }
                    last = Some(e);
                }
            }
        }
        Err(last.expect("the loop runs at least once and only stores Err"))
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

    /// The resident verification agent — ADR-0041 Decision 1′.
    ///
    /// NOT the `palw-agent` crate. That one supervises v2 COMPUTE jobs over a Unix socket and
    /// spawns a worker process per job; this is a child of the validator itself, on the Layer-1
    /// PoW tag path, and its entire purpose is that there is NO process per job.
    ///
    /// One `palw-worker --mode agent` child holds the model for the life of the node, so a seed
    /// costs an inference instead of an inference *plus* a 1.2 GB artifact read, a SHA-256 and a
    /// model load. Measured on the pinned Qwen3.5-2B: 3.2 s one-shot → 0.56 s resident, with the
    /// projection byte-identical in every field.
    ///
    /// It is an accelerator and nothing else, and this module is written so that stays true:
    ///
    /// * it runs only when `MISAKA_PALW_AGENT=1`;
    /// * EVERY failure — spawn, handshake, timeout, a frame out of order, a child that exited —
    ///   drops the handle and returns `None`, and the caller then runs the one-shot path that
    ///   ships today;
    /// * the document it returns is read by [`super::tag_from_doc`], the same code that reads a
    ///   one-shot's stdout, so there is no second parser to drift.
    ///
    /// The agent can therefore change how fast a tag arrives. It cannot change WHICH tag arrives,
    /// and it cannot wedge a node that the one-shot path would have synced.
    mod resident {
        use super::*;
        use std::io::{BufRead, BufReader};
        use std::process::{Child, ChildStderr, ChildStdin};
        use std::sync::mpsc::{Receiver, RecvTimeoutError};

        /// The marker every agent → driver line carries. Anything else on that stdout is
        /// third-party output (llama.cpp and ggml share the pipe) and is skipped rather than
        /// parsed — the reason the protocol is marked lines and not length-prefixed frames, which
        /// one stray byte would desynchronise silently and permanently.
        const MARKER: &str = "@palw-pow1 ";

        /// Whether this process may use a resident agent.
        fn enabled() -> bool {
            std::env::var(PALW_AGENT_ENV).as_deref() == Ok("1")
        }

        /// Idle agents, available for checkout.
        ///
        /// The pool never grows past the concurrency bound, and not because it counts: every
        /// caller holds a [`Permit`] for its whole checkout-to-checkin span, so at most
        /// `inference_concurrency()` callers can be here at once and at most that many agents can
        /// exist. It never shrinks either — an operator who sets the bound to 8 for a sync is
        /// asking for 8 resident models, and gets them for the life of the process.
        static POOL: Mutex<Vec<Agent>> = Mutex::new(Vec::new());

        fn pool() -> std::sync::MutexGuard<'static, Vec<Agent>> {
            // Poison recovery: a panic elsewhere must not disable the accelerator permanently, and
            // the pool holds no invariant a panic could have broken — it is a bag of idle handles.
            POOL.lock().unwrap_or_else(|e| e.into_inner())
        }

        struct Agent {
            child: Child,
            stdin: ChildStdin,
            /// Marker-stripped payloads, in the order the agent wrote them.
            frames: Receiver<String>,
            next_id: u64,
        }

        impl Drop for Agent {
            fn drop(&mut self) {
                // Kill rather than close-stdin-and-wait. Everything that drops a handle has
                // already decided it is unusable, and a wedged child is exactly the case where
                // waiting for a clean exit hangs the validator instead of the child.
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }

        /// A tag for `seed` from the resident agent, or `None` if it cannot serve one — in which
        /// case the caller runs the one-shot path. Never returns `Err`: there is no agent failure
        /// that the fallback does not answer better than an error would.
        pub(super) fn tag(worker: &str, seed: &[u8; 32]) -> Option<[u8; POW_L1_PALW_OUT_BYTES]> {
            if !enabled() {
                return None;
            }
            // Two rounds. The first may check out a handle whose child died since it was last
            // used (OOM-killed, or an operator restarted it) — indistinguishable from a healthy one
            // until we write to it. The second round is the respawn that answers that case.
            //
            // An agent goes back in the pool only after it answers correctly. Every other exit from
            // this loop drops it, and dropping kills the child, so a handle whose state this code
            // is unsure of is never handed to the next caller.
            for round in 1..=2u32 {
                let mut agent = match pool().pop() {
                    Some(agent) => agent,
                    None => match Agent::spawn(worker) {
                        Ok(agent) => agent,
                        Err(e) => {
                            log::warn!("palw-pow: resident agent unavailable ({e}); using one-shot workers");
                            return None;
                        }
                    },
                };
                match agent.request(&palw_pow_prompt_v1(seed), POW_L1_PALW_N_PREDICT_V1) {
                    Ok(doc) => match super::tag_from_doc(&doc) {
                        Ok(tag) => {
                            pool().push(agent);
                            return Some(tag);
                        }
                        // The agent answered and the answer is not a projection. That is a build
                        // mismatch, not a transient, so do not respawn into the same result.
                        Err(e) => {
                            log::warn!("palw-pow: resident agent returned an unusable document ({e}); using one-shot workers");
                            return None;
                        }
                    },
                    Err(e) => log::warn!("palw-pow: resident agent failed on round {round} ({e}); restarting it"),
                }
            }
            None
        }

        /// Drains a pipe for the LIFE of the child, on its own thread.
        ///
        /// A resident agent makes the pipe-buffer trap worse than it is for a one-shot: llama.cpp
        /// keeps writing to stderr for as long as the node runs, and a full 64 KiB buffer blocks
        /// the agent inside `write()` with a job half done — a hang that produces no error
        /// anywhere. The one-shot path learned this the expensive way; a long-lived child cannot
        /// afford to relearn it.
        fn spawn_stderr_drain(pipe: ChildStderr) -> Result<(), PowLayer0Error> {
            std::thread::Builder::new()
                .name("palw-agent-stderr".into())
                .spawn(move || {
                    let mut reader = BufReader::new(pipe);
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line) {
                            Ok(0) | Err(_) => break,
                            Ok(_) => log::trace!("palw-agent: {}", line.trim_end()),
                        }
                    }
                })
                .map(|_| ())
                .map_err(|e| PowLayer0Error::PalwWorkerFailed(format!("cannot start the PALW agent stderr drain: {e}")))
        }

        impl Agent {
            fn spawn(worker: &str) -> Result<Agent, PowLayer0Error> {
                let started = Instant::now();
                let mut child = Command::new(worker)
                    .args(["--mode", "pow-agent"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| PowLayer0Error::PalwUnavailable(format!("cannot spawn the PALW agent at {worker}: {e}")))?;
                let stdin = child.stdin.take().expect("stdin was piped above");
                let stdout = child.stdout.take().expect("stdout was piped above");
                let stderr = child.stderr.take().expect("stderr was piped above");

                // Unbounded on purpose: a bounded channel lets a slow or abandoned consumer block
                // the reader thread, which stops draining stdout, which is the same deadlock one
                // pipe over. The volume it has to hold is one frame per request.
                let (tx, frames) = std::sync::mpsc::channel::<String>();

                // Build the handle BEFORE anything else can fail, so every early return from here
                // on reaps the child through `Drop` rather than orphaning a process holding 1.2 GB.
                let mut agent = Agent { child, stdin, frames, next_id: 1 };
                spawn_stderr_drain(stderr)?;
                std::thread::Builder::new()
                    .name("palw-agent-stdout".into())
                    .spawn(move || {
                        let mut reader = BufReader::new(stdout);
                        let mut line = String::new();
                        loop {
                            line.clear();
                            match reader.read_line(&mut line) {
                                Ok(0) | Err(_) => break,
                                Ok(_) => match line.trim_end().strip_prefix(MARKER) {
                                    Some(payload) => {
                                        if tx.send(payload.to_owned()).is_err() {
                                            break;
                                        }
                                    }
                                    None => {
                                        if !line.trim().is_empty() {
                                            log::debug!("palw-agent stdout (not a frame): {}", line.trim_end());
                                        }
                                    }
                                },
                            }
                        }
                    })
                    .map_err(|e| PowLayer0Error::PalwWorkerFailed(format!("cannot start the PALW agent stdout reader: {e}")))?;

                // The handshake is the model load, so it gets the same budget as an inference —
                // a cold artifact read of 1.2 GB is not a fast operation on a busy host.
                let ready = agent.recv_frame(timeout())?;
                if ready.get("ready").and_then(|v| v.as_bool()) != Some(true) {
                    return Err(PowLayer0Error::PalwWorkerFailed(format!(
                        "the PALW agent's first frame is not a readiness frame: {ready}"
                    )));
                }
                log::info!(
                    "palw-pow: resident agent ready in {:?} (pid {}, runtime_class_id {})",
                    started.elapsed(),
                    ready.get("pid").and_then(|v| v.as_u64()).unwrap_or(0),
                    ready.get("runtime_class_id").and_then(|v| v.as_str()).unwrap_or("unreported"),
                );
                Ok(agent)
            }

            fn recv_frame(&mut self, budget: Duration) -> Result<serde_json::Value, PowLayer0Error> {
                match self.frames.recv_timeout(budget) {
                    Ok(payload) => serde_json::from_str(&payload)
                        .map_err(|e| PowLayer0Error::PalwWorkerFailed(format!("cannot parse a PALW agent frame: {e}"))),
                    Err(RecvTimeoutError::Timeout) => Err(PowLayer0Error::PalwWorkerFailed(format!(
                        "the PALW agent produced no frame within {budget:?} (raise {PALW_TIMEOUT_ENV} if the \
                         machine is genuinely this slow)"
                    ))),
                    Err(RecvTimeoutError::Disconnected) => Err(PowLayer0Error::PalwWorkerFailed("the PALW agent exited".into())),
                }
            }

            fn request(&mut self, prompt: &str, n_predict: u32) -> Result<serde_json::Value, PowLayer0Error> {
                let id = self.next_id;
                self.next_id += 1;
                // Hex, so the transport is byte-exact and newline-safe: the job is defined over
                // raw bytes, and a protocol that could not carry a `\n` would quietly change the
                // input and therefore the tag.
                let mut encoded = vec![0u8; prompt.len() * 2];
                faster_hex::hex_encode(prompt.as_bytes(), &mut encoded)
                    .map_err(|e| PowLayer0Error::PalwWorkerFailed(format!("cannot hex the PALW prompt: {e}")))?;
                let prompt_hex = String::from_utf8(encoded).expect("hex_encode emits ASCII");
                let request = serde_json::json!({ "id": id, "n_predict": n_predict, "prompt_hex": prompt_hex });
                writeln!(self.stdin, "{request}")
                    .and_then(|()| self.stdin.flush())
                    .map_err(|e| PowLayer0Error::PalwWorkerFailed(format!("cannot write a request to the PALW agent: {e}")))?;

                let started = Instant::now();
                let frame = self.recv_frame(timeout())?;
                // The id echo is the desync check. It should be unreachable — a timeout drops the
                // handle, so no abandoned frame can be waiting in the channel for the next request
                // to collect — which is precisely why a mismatch means the handle's state is not
                // what this code believes, and the handle has to go.
                if frame.get("id").and_then(|v| v.as_u64()) != Some(id) {
                    return Err(PowLayer0Error::PalwWorkerFailed(format!(
                        "PALW agent frame out of order: expected id {id}, got {}",
                        frame.get("id").map(|v| v.to_string()).unwrap_or_else(|| "no id".into())
                    )));
                }
                if frame.get("ok").and_then(|v| v.as_bool()) != Some(true) {
                    return Err(PowLayer0Error::PalwWorkerFailed(format!("the PALW agent refused a job: {frame}")));
                }
                let projection = frame
                    .get("projection")
                    .cloned()
                    .ok_or_else(|| PowLayer0Error::PalwWorkerFailed("a PALW agent frame carries no projection".into()))?;
                log::debug!("palw-pow: resident agent served a seed in {:?}", started.elapsed());
                Ok(projection)
            }
        }
    }

    #[cfg(all(test, unix))]
    mod host_lease_tests {
        use super::host_slot;
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

        /// `flock` bounds holders of independent open file descriptions, which is exactly what
        /// separate PALW node processes are.
        ///
        /// Threads test the real mechanism here, not a stand-in: `flock(2)` associates a lock with
        /// the open file DESCRIPTION, not the process, so two `open` calls in one process contend
        /// with each other precisely as two processes do ("An attempt to lock the file using one of
        /// these file descriptors may be denied by a lock that the calling process has already
        /// placed via another file descriptor" — flock(2)). Using threads keeps the test in CI,
        /// with no model, no worker and no spawned binary.
        #[test]
        fn the_host_lease_bounds_holders_of_independent_descriptions() {
            const SLOTS: usize = 2;
            const CONTENDERS: usize = 6;
            let dir = std::env::temp_dir().join(format!("misaka-palw-lease-test-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);

            let live = AtomicUsize::new(0);
            let peak = AtomicUsize::new(0);
            let granted = AtomicUsize::new(0);
            std::thread::scope(|scope| {
                for _ in 0..CONTENDERS {
                    scope.spawn(|| {
                        let slot = host_slot(&dir, SLOTS).expect("a temp directory is usable");
                        let now = live.fetch_add(1, SeqCst) + 1;
                        peak.fetch_max(now, SeqCst);
                        // Long enough that every contender is inside the window at some point, so
                        // the peak is a real observation and not a scheduling accident.
                        std::thread::sleep(std::time::Duration::from_millis(60));
                        live.fetch_sub(1, SeqCst);
                        granted.fetch_add(1, SeqCst);
                        drop(slot);
                    });
                }
            });

            assert_eq!(granted.load(SeqCst), CONTENDERS, "every contender must eventually get a slot");
            assert!(peak.load(SeqCst) <= SLOTS, "{} held the lease at once, bound was {SLOTS}", peak.load(SeqCst));
            // Without this the test would also pass if the lease handed out nothing concurrently —
            // i.e. if it had silently degraded to a global mutex.
            assert_eq!(peak.load(SeqCst), SLOTS, "the lease never reached its own bound; it is over-serialising");
            let _ = std::fs::remove_dir_all(&dir);
        }

        /// A slot is released by dropping the file, so the next caller gets it — the property that
        /// makes an inference-scoped permit correct rather than a one-shot allocation.
        #[test]
        fn a_dropped_slot_is_reusable() {
            let dir = std::env::temp_dir().join(format!("misaka-palw-lease-reuse-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let first = host_slot(&dir, 1).expect("a temp directory is usable");
            drop(first);
            let second = host_slot(&dir, 1).expect("the only slot must be free again");
            drop(second);
            let _ = std::fs::remove_dir_all(&dir);
        }
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
            let _permit = acquire_permit();
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

    /// Memoized worker-class calibration (algo 4): the runtime cannot change under a running
    /// process, and the probe costs a full inference — once per process is the right cadence.
    /// A FAILURE is memoized too: an out-of-class runtime is a configuration fact.
    static WORKER_CALIBRATION_VERIFIED: OnceLock<Result<(), String>> = OnceLock::new();

    pub(super) fn worker_calibration_once(expected: &'static str) -> Result<(), PowLayer0Error> {
        WORKER_CALIBRATION_VERIFIED
            .get_or_init(|| {
                let tag = tag_for_seed(&POW_L1_PALW_PROBE_SEED_V1).map_err(|e| e.to_string())?;
                let got = faster_hex::hex_string(&tag);
                if got == expected {
                    Ok(())
                } else {
                    Err(format!(
                        "this worker runtime is not in the network's pinned determinism class.\n  expected calibration {expected}\n  got               {got}\n\
                         The GGUF pin matches, so the difference is the worker build profile or the CPU architecture. \
                         Every block this node validated would disagree with the network. Build the pinned CPU-profile \
                         worker (docs/palw-algo4-crosshost-determinism-2026-08-16.md) or run a network pinned to this \
                         runtime's class."
                    ))
                }
            })
            .clone()
            .map_err(PowLayer0Error::PalwUnavailable)
    }

    /// Memoized `verify_model_pin`: the blob cannot change under a running server without a
    /// restart of `ollama pull`, and re-checking per attempt would put an HTTP round-trip in the
    /// mining hot loop. A FAILURE is memoized too — a wrong blob is a configuration fact, and
    /// re-querying it thousands of times per minute helps nobody.
    static MODEL_PIN_VERIFIED: OnceLock<Result<(), String>> = OnceLock::new();

    fn verify_model_pin_once(url: &str, model: &str) -> Result<(), PowLayer0Error> {
        MODEL_PIN_VERIFIED
            .get_or_init(|| verify_model_pin(url, model).and_then(|()| verify_calibration(url, model)).map_err(|e| e.to_string()))
            .clone()
            .map_err(PowLayer0Error::PalwUnavailable)
    }

    /// The class check: run the canonical probe through the ordinary tag path and compare against
    /// `POW_L1_PALW_OLLAMA_CALIBRATION_V1`. Costs one inference, once per process, and is what
    /// stops a runtime-drifted node from joining and silently rejecting everyone.
    pub(super) fn verify_calibration(url: &str, model: &str) -> Result<(), PowLayer0Error> {
        use kaspa_consensus_core::pow_layer0::{POW_L1_PALW_OLLAMA_CALIBRATION_V1, POW_L1_PALW_OLLAMA_PROBE_SEED_V1};
        let tag = run_ollama(url, model, &POW_L1_PALW_OLLAMA_PROBE_SEED_V1)?;
        let got = faster_hex::hex_string(&tag);
        if got != POW_L1_PALW_OLLAMA_CALIBRATION_V1 {
            return Err(PowLayer0Error::PalwUnavailable(format!(
                "this runtime is not in the network's determinism class.\n  expected calibration {POW_L1_PALW_OLLAMA_CALIBRATION_V1}\n  got               {got}\n                 The model blob matches, so the difference is the Ollama build or the CPU architecture.                  Every block this node validated would disagree with the network. Match the fleet's runtime                  (see docs/testnet10-palw-rollout-runbook.md) or run a network pinned to this class."
            )));
        }
        Ok(())
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

    /// The one place that speaks to the runtime. `templated` picks between the consensus request
    /// (raw continuation) and the chat-templated form a human question needs.
    pub(super) fn generate(
        url: &str,
        model: &str,
        prompt: &str,
        num_predict: u32,
        templated: bool,
        think: Option<bool>,
    ) -> Result<(String, u32, u32), PowLayer0Error> {
        use kaspa_consensus_core::pow_layer0::POW_L1_PALW_OLLAMA_NUM_GPU_V1;
        let mut body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "raw": !templated,
            "stream": false,
            "options": {
                "temperature": 0.0,
                "num_predict": num_predict,
                "num_ctx": 4096,
                "seed": 0,
                "num_gpu": POW_L1_PALW_OLLAMA_NUM_GPU_V1,
            },
        });
        // Omitted, not defaulted, when the caller does not choose: the consensus request must stay
        // exactly the bytes it has always been.
        if let Some(think) = think {
            body["think"] = serde_json::Value::Bool(think);
        }
        let body = body.to_string();
        let response = http_request("POST", url, "/api/generate", Some(&body), timeout())?;
        let doc: serde_json::Value = serde_json::from_slice(&response)
            .map_err(|e| PowLayer0Error::PalwWorkerFailed(format!("cannot parse the Ollama response: {e}")))?;
        if let Some(err) = doc.get("error").and_then(|v| v.as_str()) {
            return Err(PowLayer0Error::PalwWorkerFailed(format!(
                "Ollama refused the generate request: {err} (is model {model} present? `ollama list`)"
            )));
        }
        let text = doc
            .get("response")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PowLayer0Error::PalwWorkerFailed("Ollama response lacks the `response` field".into()))?;
        Ok((
            text.to_owned(),
            doc.get("prompt_eval_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            doc.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        ))
    }

    fn run_ollama(url: &str, model: &str, seed: &[u8; 32]) -> Result<[u8; POW_L1_PALW_OLLAMA_OUT_BYTES], PowLayer0Error> {
        use kaspa_consensus_core::pow_layer0::{
            POW_L1_PALW_OLLAMA_NUM_PREDICT_V1, palw_ollama_l1_tag_from_response,
        };
        let prompt = kaspa_consensus_core::pow_layer0::palw_pow_prompt_v1(seed);
        // The consensus request shape lives in `generate`: raw continuation (no chat template),
        // greedy, fixed context, CPU backend. What makes THIS call the consensus one is the pair
        // it passes — the frozen v1 decode budget and raw mode.
        let started = Instant::now();
        let (text, prompt_eval, eval) = generate(url, model, &prompt, POW_L1_PALW_OLLAMA_NUM_PREDICT_V1, false, None)?;
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
        tag_from_doc(&doc)
    }

    /// The Layer-1 tag from a projection document.
    ///
    /// Shared by the one-shot path and the resident agent, so the two cannot drift. A tag that
    /// depended on WHICH transport delivered the document would be a consensus bug; the way not to
    /// have one is not to have a second reader.
    fn tag_from_doc(doc: &serde_json::Value) -> Result<[u8; POW_L1_PALW_OUT_BYTES], PowLayer0Error> {
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

    pub(super) fn worker_calibration_once(_expected: &'static str) -> Result<(), PowLayer0Error> {
        Err(PowLayer0Error::PalwUnavailable("PALW (algo_id = 4) PoW cannot run in a wasm build".into()))
    }

    pub(super) fn ollama_tag_for_seed(_seed: &[u8; 32]) -> Result<[u8; POW_L1_PALW_OLLAMA_OUT_BYTES], PowLayer0Error> {
        Err(PowLayer0Error::PalwUnavailable("PALW-Ollama (algo_id = 5) PoW cannot run in a wasm build".into()))
    }
}
