//! `palw-worker` — the pinned Qwen3.5-2B palw-lite runtime, behind the exact subprocess contract
//! `misaka_palw::PalwWorkerRuntime` drives:
//!
//! * `--mode manifest` → `{ runtime_manifest_hash, runtime_class_id }`
//! * `--mode self-job --prompt-stdin --n-predict N` → execute the job over the stdin bytes
//! * `--mode verify   --prompt-stdin --n-predict N` → independently re-execute the same job
//!
//! Both job modes print a `misaka.palw.testnet-submission.v3` document: the 16
//! `MatchProjectionV1` fields, every one derived from the actual execution or from the pinned
//! identity — never from randomness. That is the entire determinism contract: an executor and a
//! verifier given the same bytes and the same ceiling must emit byte-identical documents, so
//! `verify` **is** `self-job` recomputed (PALW's two modes differ in bookkeeping this worker does
//! not keep; what must never differ is the computation).
//!
//! # What makes the projection honest
//!
//! * `output_commitment` covers the greedily-decoded token ids and their rendered bytes.
//! * `gemm_trace_root` chains a digest of the **full logits vector after every decode call** —
//!   ~150k floats per job. Nothing short of running the pinned model on the pinned kernels
//!   reproduces it; a byte-identical replay of it is evidence of re-execution, which is exactly
//!   what `CanonicalFullReplay` pays for.
//! * The model file is checked against `qwen35_pins` (size and SHA-256) before any inference: a
//!   worker holding the wrong artifact refuses to run rather than mint refutable receipts.
//!
//! Greedy argmax decoding, first-index tie-break, no sampler state: the spec's sampling seed
//! covers runtimes that sample, and this one deliberately does not.

use std::io::Read;
use std::path::{Path, PathBuf};

use kaspa_consensus_core::vlt::{derive_model_weights_hash, derive_runtime_class_id, derive_runtime_hash, qwen35_pins};
use kaspa_hashes::Hash64;
use sha2::Digest;

// ---------------------------------------------------------------------------------------------
// The pinned execution shape. These are consensus-relevant in the soft sense: they are hashed
// into `shape_profile_id` and `request_commitment`, so two workers disagreeing on any of them
// produce different projections — which is correct, because a different batch split or thread
// count is a different reduction order on some backends.
// ---------------------------------------------------------------------------------------------
const N_CTX: i32 = 4096;
const N_BATCH: i32 = 512;
const N_THREADS: i32 = 4;
const SHAPE_STRING: &str =
    "n_ctx=4096/n_batch=512/n_ubatch=512/n_seq=1/n_threads=4/flash-attn=disabled/gpu-layers=all/greedy-argmax-first-index/v1";
const CU_RULESET: &str = "cu = prefill + 8*decode";
const TRACE_SCHEME: &str = "full-logits-per-decode-call/keyed-blake2b-512/v1";

const SCHEMA: &str = "misaka.palw.testnet-submission.v3";

// ---------------------------------------------------------------------------------------------
// FFI to src/shim.c (compiled against the pinned llama.h by build.rs).
// ---------------------------------------------------------------------------------------------
#[repr(C)]
struct ShimCtx {
    _opaque: [u8; 0],
}

unsafe extern "C" {
    fn shim_open(model_path: *const u8, n_ctx: i32, n_batch: i32, n_threads: i32) -> *mut ShimCtx;
    fn shim_n_vocab(s: *const ShimCtx) -> i32;
    fn shim_tokenize(s: *const ShimCtx, text: *const u8, text_len: i32, out: *mut i32, max_out: i32) -> i32;
    fn shim_decode(s: *mut ShimCtx, tokens: *const i32, n: i32) -> i32;
    fn shim_logits_last(s: *mut ShimCtx, out: *mut f32) -> i32;
    fn shim_is_eog(s: *const ShimCtx, token: i32) -> i32;
    fn shim_token_to_piece(s: *const ShimCtx, token: i32, buf: *mut u8, buf_len: i32) -> i32;
    fn shim_close(s: *mut ShimCtx);
}

fn die(msg: String) -> ! {
    eprintln!("[palw-worker] {msg}");
    std::process::exit(1);
}

/// Keyed BLAKE2b-512 over `parts`, domain-separated by `key`.
fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for p in parts {
        h.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn hex(h: Hash64) -> String {
    faster_hex::hex_string(h.as_byte_slice())
}

// ---------------------------------------------------------------------------------------------
// Model artifact check: refuse to run anything but the pinned GGUF.
// ---------------------------------------------------------------------------------------------

/// SHA-256 of the model file, cached beside the working directory keyed on (path, size, mtime) —
/// the file is 1.2 GB and every job is its own process.
fn gguf_sha256(path: &Path) -> String {
    let meta = std::fs::metadata(path).unwrap_or_else(|e| die(format!("cannot stat model at {}: {e}", path.display())));
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    let cache_key = format!("{}|{}|{}", path.display(), meta.len(), mtime);
    let cache_path = PathBuf::from(".palw-gguf-sha.json");
    if let Ok(bytes) = std::fs::read(&cache_path)
        && let Ok(doc) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && doc.get("key").and_then(|v| v.as_str()) == Some(cache_key.as_str())
        && let Some(sha) = doc.get("sha256").and_then(|v| v.as_str())
    {
        return sha.to_owned();
    }
    let mut file = std::fs::File::open(path).unwrap_or_else(|e| die(format!("cannot open model at {}: {e}", path.display())));
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher).unwrap_or_else(|e| die(format!("cannot read model at {}: {e}", path.display())));
    let sha = faster_hex::hex_string(&hasher.finalize());
    let _ = std::fs::write(&cache_path, serde_json::json!({ "key": cache_key, "sha256": sha }).to_string());
    sha
}

fn pinned_model_path() -> PathBuf {
    let path = std::env::var("MISAKA_PALW_GGUF")
        .unwrap_or_else(|_| die("MISAKA_PALW_GGUF is not set; point it at the pinned Qwen3.5-2B-Q4_K_M.gguf".into()));
    let path = PathBuf::from(path);
    let meta = std::fs::metadata(&path).unwrap_or_else(|e| die(format!("cannot stat model at {}: {e}", path.display())));
    if meta.len() != qwen35_pins::GGUF_SIZE {
        die(format!(
            "model at {} is {} bytes, but the pinned {} is {} bytes — refusing to run an unpinned artifact",
            path.display(),
            meta.len(),
            qwen35_pins::GGUF_FILENAME,
            qwen35_pins::GGUF_SIZE
        ));
    }
    let sha = gguf_sha256(&path);
    if sha != qwen35_pins::GGUF_SHA256 {
        die(format!(
            "model at {} has SHA-256 {sha}, but the pinned {} is {} — refusing to run an unpinned artifact",
            path.display(),
            qwen35_pins::GGUF_FILENAME,
            qwen35_pins::GGUF_SHA256
        ));
    }
    path
}

// ---------------------------------------------------------------------------------------------
// Identity, straight from the shared pins (the same constants consensus derives the registered
// entry from — one source of truth, so drift is a compile error, not a refutation).
// ---------------------------------------------------------------------------------------------

fn runtime_manifest_hash() -> Hash64 {
    derive_runtime_hash(
        qwen35_pins::LLAMA_COMMIT,
        qwen35_pins::LLAMA_PATCH_SHA256,
        qwen35_pins::LLAMA_BUILD_NUMBER,
        qwen35_pins::METAL_BUILD_PROFILE,
    )
}

fn runtime_class_id() -> Hash64 {
    derive_runtime_class_id(qwen35_pins::METAL_RUNTIME_CLASS)
}

fn model_profile_id() -> Hash64 {
    derive_model_weights_hash(
        qwen35_pins::GGUF_SHA256,
        qwen35_pins::GGUF_SIZE,
        qwen35_pins::GGUF_FILENAME,
        qwen35_pins::BASE_REPO_ID,
        qwen35_pins::BASE_REVISION,
    )
}

// ---------------------------------------------------------------------------------------------
// The job itself.
// ---------------------------------------------------------------------------------------------

struct Execution {
    prefill_tokens: u32,
    decode_tokens: u32,
    output_commitment: Hash64,
    operation_schedule_commitment: Hash64,
    schedule_event_count: u64,
    gemm_trace_root: Hash64,
    trace_event_count: u64,
}

/// Greedy argmax with first-index tie-break: strict `>` never replaces on equality, and an
/// initial NaN can only lose, so the result is a pure function of the logits bytes.
fn argmax(logits: &[f32]) -> i32 {
    let mut best = 0usize;
    for (i, l) in logits.iter().enumerate().skip(1) {
        if *l > logits[best] {
            best = i;
        }
    }
    best as i32
}

fn execute(model_path: &Path, input: &[u8], n_predict: u32) -> Execution {
    let started = std::time::Instant::now();
    let ctx = unsafe { shim_open(format!("{}\0", model_path.display()).as_ptr(), N_CTX, N_BATCH, N_THREADS) };
    if ctx.is_null() {
        die(format!("llama.cpp failed to load {}", model_path.display()));
    }
    let n_vocab = unsafe { shim_n_vocab(ctx) };
    eprintln!("[palw-worker] model loaded in {:?} (n_vocab={n_vocab})", started.elapsed());

    // Tokenize the raw input bytes. A prompt that does not fit the job's own ceiling is a hard
    // error, not a truncation: `normalize_vlt` rejects `prefill + decode > max_tokens`, so a
    // silently-truncated job would commit to bytes it did not fully prefill.
    let max_prompt = (N_CTX - 8) as usize;
    let mut tokens = vec![0i32; max_prompt];
    let n = unsafe { shim_tokenize(ctx, input.as_ptr(), input.len() as i32, tokens.as_mut_ptr(), max_prompt as i32) };
    if n < 0 {
        die(format!("prompt tokenizes to more than {max_prompt} tokens; it does not fit the context"));
    }
    tokens.truncate(n as usize);
    let prefill_tokens = tokens.len() as u32;
    if prefill_tokens > n_predict {
        die(format!(
            "prompt tokenizes to {prefill_tokens} tokens but the job ceiling is {n_predict}; \
             the job would exceed its own spec (produced > max_tokens) and mint nothing"
        ));
    }
    let budget = n_predict - prefill_tokens;

    // Trace and schedule accumulators. One trace event per `llama_decode` call: the digest of
    // the full logits vector the call produced.
    let mut logits = vec![0f32; n_vocab as usize];
    let mut logits_bytes = vec![0u8; n_vocab as usize * 4];
    let mut trace_events: Vec<u8> = Vec::new();
    let mut trace_event_count: u64 = 0;
    let mut schedule = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw-lite/schedule/v1").to_state();
    let mut schedule_event_count: u64 = 0;

    let step = |ctx: *mut ShimCtx,
                fed: &[i32],
                logits: &mut [f32],
                logits_bytes: &mut [u8],
                trace_events: &mut Vec<u8>,
                trace_event_count: &mut u64,
                schedule: &mut blake2b_simd::State,
                schedule_event_count: &mut u64| {
        let rc = unsafe { shim_decode(ctx, fed.as_ptr(), fed.len() as i32) };
        if rc != 0 {
            die(format!("llama_decode failed with {rc}"));
        }
        schedule.update(&(*schedule_event_count).to_le_bytes());
        schedule.update(&(fed.len() as u64).to_le_bytes());
        *schedule_event_count += 1;
        let got = unsafe { shim_logits_last(ctx, logits.as_mut_ptr()) };
        if got != n_vocab {
            die(format!("logits unavailable after decode (got {got})"));
        }
        for (chunk, l) in logits_bytes.chunks_exact_mut(4).zip(logits.iter()) {
            chunk.copy_from_slice(&l.to_le_bytes());
        }
        let ev = keyed64(b"misaka-palw-lite/trace-event/v1", &[&trace_event_count.to_le_bytes(), logits_bytes]);
        trace_events.extend_from_slice(ev.as_byte_slice());
        *trace_event_count += 1;
    };

    // Prefill, then greedy decode. The last sampled token is not fed back (there is nothing left
    // to sample after it), so `schedule_event_count = 1 + tokens fed`, a fact both replicas
    // reproduce exactly.
    step(
        ctx,
        &tokens,
        &mut logits,
        &mut logits_bytes,
        &mut trace_events,
        &mut trace_event_count,
        &mut schedule,
        &mut schedule_event_count,
    );
    let mut outputs: Vec<i32> = Vec::new();
    let mut rendered: Vec<u8> = Vec::new();
    let mut piece = vec![0u8; 512];
    while (outputs.len() as u32) < budget {
        let tok = argmax(&logits);
        outputs.push(tok);
        let n = unsafe { shim_token_to_piece(ctx, tok, piece.as_mut_ptr(), piece.len() as i32) };
        if n > 0 {
            rendered.extend_from_slice(&piece[..n as usize]);
        }
        if unsafe { shim_is_eog(ctx, tok) } == 1 || (outputs.len() as u32) >= budget {
            break;
        }
        step(
            ctx,
            &outputs[outputs.len() - 1..],
            &mut logits,
            &mut logits_bytes,
            &mut trace_events,
            &mut trace_event_count,
            &mut schedule,
            &mut schedule_event_count,
        );
    }
    unsafe { shim_close(ctx) };

    let decode_tokens = outputs.len() as u32;
    let mut output_ids: Vec<u8> = Vec::with_capacity(outputs.len() * 4);
    for t in &outputs {
        output_ids.extend_from_slice(&t.to_le_bytes());
    }
    let output_commitment =
        keyed64(b"misaka-palw-lite/output/v1", &[&(outputs.len() as u64).to_le_bytes(), &output_ids, &[0xff], &rendered]);
    let mut schedule_out = [0u8; 64];
    schedule_out.copy_from_slice(schedule.finalize().as_bytes());
    eprintln!(
        "[palw-worker] executed: prefill={prefill_tokens} decode={decode_tokens} in {:?}; output: {:?}",
        started.elapsed(),
        String::from_utf8_lossy(&rendered)
    );
    Execution {
        prefill_tokens,
        decode_tokens,
        output_commitment,
        operation_schedule_commitment: Hash64::from_bytes(schedule_out),
        schedule_event_count,
        gemm_trace_root: keyed64(b"misaka-palw-lite/trace-root/v1", &[&trace_events]),
        trace_event_count,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut mode: Option<String> = None;
    let mut n_predict: Option<u32> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                i += 1;
                mode = args.get(i).cloned();
            }
            "--prompt-stdin" => {}
            "--n-predict" => {
                i += 1;
                n_predict = args.get(i).and_then(|s| s.parse().ok());
            }
            other => die(format!("unknown argument {other:?}")),
        }
        i += 1;
    }

    match mode.as_deref() {
        Some("manifest") => {
            let doc = serde_json::json!({
                "runtime_manifest_hash": hex(runtime_manifest_hash()),
                "runtime_class_id": hex(runtime_class_id()),
                "model_profile_id": hex(model_profile_id()),
                "worker": "misaka-palw-worker",
                "llama_commit": qwen35_pins::LLAMA_COMMIT,
                "llama_build_number": qwen35_pins::LLAMA_BUILD_NUMBER,
                "gguf": qwen35_pins::GGUF_FILENAME,
            });
            println!("{doc}");
        }
        Some(m @ ("self-job" | "verify")) => {
            let n_predict = n_predict.unwrap_or_else(|| die("--n-predict is required".into()));
            if n_predict == 0 {
                die("--n-predict must be at least 1".into());
            }
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input).unwrap_or_else(|e| die(format!("cannot read the prompt from stdin: {e}")));
            if input.is_empty() {
                die("the prompt on stdin is empty".into());
            }
            eprintln!("[palw-worker] mode={m} n_predict={n_predict} input={} bytes", input.len());
            let model_path = pinned_model_path();
            let exec = execute(&model_path, &input, n_predict);

            // The replay-stable projection. Job identity mixes the input, the ceiling and the
            // pinned identities — never the mode and never anything drawn at run time, so an
            // executor and its verifiers land on identical documents byte for byte.
            let prompt_digest = keyed64(b"misaka-palw-lite/input/v1", &[&(input.len() as u64).to_le_bytes(), &input]);
            let job_nullifier = keyed64(
                b"misaka-palw-lite/nullifier/v1",
                &[
                    prompt_digest.as_byte_slice(),
                    &n_predict.to_le_bytes(),
                    model_profile_id().as_byte_slice(),
                    runtime_manifest_hash().as_byte_slice(),
                ],
            );
            let request_commitment = keyed64(
                b"misaka-palw-lite/request/v1",
                &[prompt_digest.as_byte_slice(), &n_predict.to_le_bytes(), SHAPE_STRING.as_bytes()],
            );
            let doc = serde_json::json!({
                "schema": SCHEMA,
                "job_nullifier": hex(job_nullifier),
                "request_commitment": hex(request_commitment),
                "model_profile_id": hex(model_profile_id()),
                "runtime_class_id": hex(runtime_class_id()),
                "runtime_manifest_hash": hex(runtime_manifest_hash()),
                "shape_profile_id": hex(keyed64(b"misaka-palw-lite/shape/v1", &[SHAPE_STRING.as_bytes()])),
                "cu_ruleset_id": hex(keyed64(b"misaka-palw-lite/cu-ruleset/v1", &[CU_RULESET.as_bytes()])),
                "canonical_compute_units": exec.prefill_tokens as u64 + 8 * exec.decode_tokens as u64,
                "prefill_tokens": exec.prefill_tokens,
                "decode_tokens": exec.decode_tokens,
                "operation_schedule_commitment": hex(exec.operation_schedule_commitment),
                "schedule_event_count": exec.schedule_event_count,
                "output_commitment": hex(exec.output_commitment),
                "trace_scheme_id": hex(keyed64(b"misaka-palw-lite/trace-scheme/v1", &[TRACE_SCHEME.as_bytes()])),
                "gemm_trace_root": hex(exec.gemm_trace_root),
                "trace_event_count": exec.trace_event_count,
            });
            println!("{doc}");
        }
        other => die(format!(
            "usage: palw-worker --mode manifest | --mode self-job|verify --prompt-stdin --n-predict N (got mode {other:?})"
        )),
    }
}
