//! **The integer family's v3-job worker: the executable the free-prompt gateway was missing.**
//!
//! `misaka-palw-gateway` drives one worker binary over the two-mode contract (`--mode
//! v3-manifest`, `--mode v3-job --trace-out <dir>`), and the only implementor of that contract was
//! `palw-worker` — the pinned-llama.cpp runtime, whose own v3 path documents that it cannot
//! produce a `committed_execution_root` ("no worker path on this tree captures a step leg at
//! all") and therefore returns a value consensus refuses. The A16 backend CAN: its
//! `execute_free_prompt` captures all four legs and its execution root is the one
//! `palw_fp_execution_root_v3` recomputes (proven by
//! `the_corrected_a16_class_commits_the_root_the_derivation_recomputes`).
//!
//! This binary is that backend behind the gateway's contract, so the pipeline
//! `browser → gateway → worker → commitment` runs the integer runtime end to end. Design points,
//! each the resolution of a way this could silently diverge from the court:
//!
//! * **The class is looked up, never assembled here.** `canonical_class_by_model_id_v1` for
//!   `Qwen/Qwen2.5-1.5B/graph-v2` — the same row the chain-side SDK resolves — so the profile,
//!   canonical job and class id have one source. A worker that built its own profile would be one
//!   requant away from committing under a class nobody registered.
//! * **The network id is the OPERATOR'S, and required** (`MISAKA_PALW_NETWORK_ID`).
//!   `palw_fp_job_context_v3` stamps it into the context and every committed root hangs off that
//!   hash, so the executor and the seat that replays the claim must use the same bytes. They do
//!   NOT come from a constant: `Qwen25A16Backend` takes its network id as a parameter, and the
//!   node passes `params.net.to_string()` — `"testnet-11"` on the live network. This worker used
//!   to hardcode `misaka-palw-rc` (the FLOOR's constant, which `Base0Backend` bakes in), so its
//!   context hash was one no seat could reproduce: every honest claim it produced would have
//!   mismatched at replay, collected an `Unavailable` quorum, and DEFAULTED its own producer for
//!   work performed correctly. There is no default now, because the wrong default was silent.
//! * **The schedule root is derived, not measured** — `expected_schedule_commitment_v2` over the
//!   derived context, exactly what `palw_fp_commitment_v3` does, so the gateway's
//!   `to_commitment` and the canonical assembly produce byte-identical commitments.
//! * **Retention is the run's `material`.** The A16 family's disclosure object is the encoded run
//!   (`qwen25_a16_material_encode_v1`) — what a seat checks and a court opens — so that is what
//!   `--trace-out` retains, under the job id, before the result frame exists.
//!
//! The artifact and tokenizer arrive by environment (`MISAKA_PALW_ARTIFACT`,
//! `MISAKA_PALW_TOKENIZER`) because the gateway spawns the worker with only the mode flags — the
//! same shape as `palw-worker`'s pinned-model env.

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1 as _;
use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_job_context_v3};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFpWorkerInputV3, PalwFpWorkerRequestV3,
    PalwFpWorkerResultV3, PalwFreePromptJobV3, fp_job_id_v3, fp_worker_request_hash_v3,
};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_v2::{PALW_V2_MAX_FRAME_BYTES, prompt_token_ids_hash_v2, read_framed, write_framed};
use kaspa_hashes::Hash64;
use misaka_palw_base0::artifact::decode_artifact_file_v1;
use misaka_palw_base0::classes::canonical_class_by_model_id_v1;
use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;
use misaka_palw_base0::tokenizer::QwenTokenizer;
use std::io::Write;
use std::path::PathBuf;

/// The catalog row this worker embodies. One name: the corrected A16 graph, the only registered
/// or registrable class whose free-prompt path reaches an execution root today.
const MODEL_ID: &str = "Qwen/Qwen2.5-1.5B/graph-v2";
/// The environment variable the operator sets to the network this worker produces for — the
/// same string kaspad prints for `params.net` (e.g. `testnet-11`). No default: see the module doc.
const NETWORK_ID_ENV: &str = "MISAKA_PALW_NETWORK_ID";

fn die(msg: String) -> ! {
    eprintln!("[palw-a16-fp-worker] fatal: {msg}");
    std::process::exit(1);
}

fn hex(h: Hash64) -> String {
    faster_hex::hex_string(h.as_byte_slice())
}

struct Loaded {
    backend: Qwen25A16Backend,
    /// The bytes this worker stamps into every context. Held so the job-context derivation and
    /// the backend cannot come to two answers.
    network_id: Vec<u8>,
    tokenizer: QwenTokenizer,
    digest: Hash64,
    tokenizer_commitment: Hash64,
    n_ctx: u32,
    class_id: Hash64,
    load_ms: u64,
}

/// Artifact + tokenizer + catalog row, refused loudly when any of the three disagree. The digest
/// check against nothing here: the artifact IS the identity (the chain registers its digest as
/// `artifact_root`), so what must agree is artifact-shape vs catalog-shape, which
/// `Qwen25A16Backend`'s own probe enforces at execution.
fn load() -> Loaded {
    let network_id = std::env::var(NETWORK_ID_ENV).unwrap_or_else(|_| {
        die(format!(
            "{NETWORK_ID_ENV} is not set. It must be the network this worker produces for — the same string kaspad \
             prints for its params.net (e.g. testnet-11) — because every committed root hangs off a context hash that \
             absorbs it, and a seat replaying this producer's claim derives that hash from the node's own network \
             name. A guess here is a claim nobody can verify and a producer defaulted for honest work."
        ))
    });
    let artifact_path = std::env::var("MISAKA_PALW_ARTIFACT").unwrap_or_else(|_| die("MISAKA_PALW_ARTIFACT is not set".into()));
    let tokenizer_path = std::env::var("MISAKA_PALW_TOKENIZER").unwrap_or_else(|_| die("MISAKA_PALW_TOKENIZER is not set".into()));

    let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
        .unwrap_or_else(|e| die(format!("the shipped court params do not build: {e:?}")));
    let entry =
        canonical_class_by_model_id_v1(&court, MODEL_ID).unwrap_or_else(|| die(format!("this build's catalog has no {MODEL_ID} row")));

    let started = std::time::Instant::now();
    let bytes = std::fs::read(&artifact_path).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    let artifact = decode_artifact_file_v1(&bytes).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    let digest = artifact.artifact_digest();
    let tokenizer_commitment = artifact.tokenizer_commitment;
    let tokenizer =
        QwenTokenizer::from_json(&std::fs::read(&tokenizer_path).unwrap_or_else(|e| die(format!("{tokenizer_path}: {e}"))))
            .unwrap_or_else(|e| die(format!("{tokenizer_path}: {e}")));
    let load_ms = started.elapsed().as_millis() as u64;

    let n_ctx = entry.profile.n_ctx;
    let class_id = entry.profile.shape_profile_id();
    let net = network_id.into_bytes();
    let backend = Qwen25A16Backend::new(std::sync::Arc::new(artifact), net.clone(), entry.profile.clone(), entry.canonical_job);
    Loaded { backend, network_id: net, tokenizer, digest, tokenizer_commitment, n_ctx, class_id, load_ms }
}

/// `--mode v3-manifest`: the identity the gateway pins requests with. The hash values here are
/// exactly the `PalwFpClassFactsV3` the backend derives contexts under — the gateway echoes them
/// into every request and this worker refuses a request that names anybody else.
fn run_manifest() {
    let loaded = load();
    let doc = serde_json::json!({
        "schema": "misaka.palw.fp-v3-manifest.v1",
        "runtime_manifest_hash": hex(Hash64::default()),
        "runtime_class_id": hex(loaded.digest),
        "model_profile_id": hex(loaded.digest),
        "shape_profile_id": hex(loaded.class_id),
        "trace_scheme_id": hex(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1()),
        "tokenizer_id": hex(loaded.tokenizer_commitment),
        "n_ctx": loaded.n_ctx,
        "prefill_single_batch_cap": loaded.n_ctx,
        "shape_string": MODEL_ID,
    });
    println!("{doc}");
}

fn run_job(trace_out: PathBuf) {
    let mut stdin = std::io::stdin().lock();
    let payload = read_framed(&mut stdin, PALW_V2_MAX_FRAME_BYTES).unwrap_or_else(|e| die(format!("v3-job rejected: {e}")));
    let request_hash = fp_worker_request_hash_v3(&payload);
    let request: PalwFpWorkerRequestV3 = borsh::from_slice(&payload).unwrap_or_else(|e| die(format!("v3-job rejected: {e}")));

    if request.version != PALW_FP_V3_VERSION {
        die(format!("v3-job rejected: request version {} is not {}", request.version, PALW_FP_V3_VERSION));
    }
    if request.privacy_mode != PALW_FP_PRIVACY_PUBLIC_DA {
        die(format!(
            "v3-job rejected: privacy mode {} is not PublicDa — a mode the panel cannot replay must not execute",
            request.privacy_mode
        ));
    }
    if request.decode_token_limit == 0 {
        die("v3-job rejected: a zero decode ceiling is not a job".into());
    }

    let loaded = load();

    // The identity cross-check the llama worker performs, kept verbatim in spirit: a request that
    // declares a different runtime/class is somebody else's job, and executing it here would
    // commit this artifact's arithmetic under that other identity.
    let mismatch = |field: &str, ours: Hash64, theirs: Hash64| {
        if ours != theirs {
            die(format!(
                "v3-job rejected: {field} mismatch — the request declares a runtime this worker is not (ours {}, request {})",
                hex(ours),
                hex(theirs)
            ));
        }
    };
    mismatch("class_id", loaded.class_id, request.class_id);
    mismatch("shape_profile_id", loaded.class_id, request.shape_profile_id);
    mismatch("model_profile_id", loaded.digest, request.model_profile_id);
    mismatch("runtime_class_id", loaded.digest, request.runtime_class_id);
    mismatch("runtime_manifest_hash", Hash64::default(), request.runtime_manifest_hash);
    mismatch("trace_scheme_id", kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1(), request.trace_scheme_id);

    if request.max_context_tokens == 0 || request.max_context_tokens > loaded.n_ctx {
        die(format!(
            "v3-job rejected: max_context_tokens {} is outside this class's 1..={}",
            request.max_context_tokens, loaded.n_ctx
        ));
    }

    // Tokenize (the Text arm) or accept ids (the TokenIds arm echoes). `parse_special = false`
    // discipline is the tokenizer's own: `encode` treats the template markers as ordinary text.
    let prompt_ids: Vec<u32> = match &request.input {
        PalwFpWorkerInputV3::Text(bytes) => {
            if bytes.is_empty() {
                die("v3-job rejected: the text arm carries no bytes".into());
            }
            let text = std::str::from_utf8(bytes)
                .unwrap_or_else(|_| die("v3-job rejected: the text arm is not UTF-8 — a template renders text, not bytes".into()));
            let ids = loaded.tokenizer.encode(text).unwrap_or_else(|e| die(format!("v3-job rejected: tokenization failed: {e}")));
            if ids.is_empty() {
                die("v3-job rejected: tokenization produced nothing".into());
            }
            ids
        }
        PalwFpWorkerInputV3::TokenIds(ids) => {
            if ids.is_empty() {
                die("v3-job rejected: the ids arm carries no tokens".into());
            }
            ids.clone()
        }
        // ADR-0077 Decision 6 (P-06): the segment-wise arm is encoded by the worker library.
        PalwFpWorkerInputV3::Segments(_) => die("v3-job rejected: the segments arm is not wired on this worker yet".into()),
    };
    let vocab = loaded.backend.profile().vocab_size as u32;
    if let Some(bad) = prompt_ids.iter().find(|t| **t >= vocab) {
        die(format!("v3-job rejected: token id {bad} is outside the model's vocab ({vocab})"));
    }
    let prefill = prompt_ids.len() as u32;
    if prefill as u64 + request.decode_token_limit as u64 > request.max_context_tokens as u64 {
        die(format!(
            "v3-job rejected: prompt {prefill} + decode ceiling {} exceeds max_context_tokens {}",
            request.decode_token_limit, request.max_context_tokens
        ));
    }

    // The job identity the trace binds — rebuilt by every replayer from chain data alone.
    let job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: request.network_domain,
        class_id: request.class_id,
        executor_bond: request.executor_bond,
        executor_pubkey: request.executor_pubkey.clone(),
        operator_id: request.operator_id,
        anchor_block: request.anchor_block,
        anchor_daa: request.anchor_daa,
        job_nonce: request.job_nonce,
        tokenizer_id: loaded.tokenizer_commitment,
        prompt_token_ids_hash: prompt_token_ids_hash_v2(&prompt_ids),
        prompt_tokens: prefill,
        decode_token_limit: request.decode_token_limit,
        max_context_tokens: request.max_context_tokens,
        privacy_mode: request.privacy_mode,
        prompt_mode: request.prompt_mode,
    };
    let binding = fp_job_id_v3(&job);

    let exec_started = std::time::Instant::now();
    let prompt_usize: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
    let run = loaded.backend.execute_free_prompt(&job, &prompt_usize).unwrap_or_else(|e| die(format!("execution refused: {e}")));
    let execute_ms = exec_started.elapsed().as_millis() as u64;

    // The schedule root, derived exactly as `palw_fp_commitment_v3` derives it, so the gateway's
    // `to_commitment` equals the canonical assembly byte for byte.
    let class_facts = PalwFpClassFactsV3 {
        model_profile_id: loaded.digest,
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: loaded.digest,
        shape_profile_id: loaded.class_id,
        cu_ruleset_id: Hash64::default(),
    };
    let context = palw_fp_job_context_v3(&job, &class_facts, &run.facts, &loaded.network_id)
        .unwrap_or_else(|e| die(format!("the finished run implies no context: {e:?}")));
    let (schedule_root, _calls) = kaspa_consensus_core::palw_v2::expected_schedule_commitment_v2(
        &context.context_hash(),
        job.prompt_tokens,
        run.facts.decode_tokens_executed,
    );

    // Retention BEFORE the result frame exists: the family's disclosure object is the encoded run
    // (what a seat checks, what a court opens), written under the job id.
    let retain_dir = trace_out.join(hex(binding));
    std::fs::create_dir_all(&retain_dir)
        .unwrap_or_else(|e| die(format!("cannot create the retention dir {}: {e}", retain_dir.display())));
    let material_path = retain_dir.join("material.bin");
    std::fs::write(&material_path, &run.outcome.material)
        .unwrap_or_else(|e| die(format!("cannot retain {}: {e}", material_path.display())));
    let manifest_doc = serde_json::json!({
        "schema": "misaka.palw.fp-v3-a16-retention.v1",
        "trace_binding": hex(binding),
        "trace_root": hex(run.outcome.trace_root),
        "trace_manifest_root": hex(run.outcome.trace_manifest_root),
        "chunk_count": run.outcome.trace_chunk_count,
        "material_bytes": run.outcome.material.len(),
        "execution_root": hex(run.outcome.execution_root),
    });
    std::fs::write(retain_dir.join("manifest.json"), serde_json::to_vec_pretty(&manifest_doc).unwrap())
        .unwrap_or_else(|e| die(format!("cannot write the retention manifest: {e}")));

    let rendered = loaded.tokenizer.decode(&run.output_token_ids).unwrap_or_else(|e| die(format!("detokenizing the answer: {e}")));

    eprintln!(
        "[palw-a16-fp-worker] v3 executed: prefill={prefill} decode={}/{} in {execute_ms}ms; exec root={}…",
        run.facts.decode_tokens_executed,
        job.decode_token_limit,
        &hex(run.outcome.execution_root)[..16]
    );

    let result = PalwFpWorkerResultV3 {
        version: PALW_FP_V3_VERSION,
        request_hash,
        job,
        prompt_token_ids: prompt_ids,
        trace_root: run.outcome.trace_root,
        output_root: run.outcome.output_root,
        schedule_root,
        execution_root: run.outcome.execution_root,
        trace_manifest_root: run.outcome.trace_manifest_root,
        trace_chunk_count: run.outcome.trace_chunk_count,
        trace_event_count: run.facts.decode_tokens_executed,
        decode_tokens_executed: run.facts.decode_tokens_executed,
        step_leaf_count: run.facts.step_leaf_count,
        stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
        output_token_ids: run.output_token_ids,
        rendered: rendered.into_bytes(),
        model_load_ms: loaded.load_ms,
        execute_ms,
    };
    let bytes = borsh::to_vec(&result).unwrap_or_else(|e| die(format!("cannot serialize the result: {e}")));
    let mut stdout = std::io::stdout().lock();
    write_framed(&mut stdout, &bytes).unwrap_or_else(|e| die(format!("cannot write the result frame: {e}")));
    stdout.flush().ok();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    match flag("--mode").as_deref() {
        Some("v3-manifest") => run_manifest(),
        Some("v3-job") => {
            let trace_out = flag("--trace-out").unwrap_or_else(|| die("--trace-out <dir> is required for v3-job".into()));
            run_job(PathBuf::from(trace_out));
        }
        other => die(format!("unsupported --mode {other:?} (v3-manifest | v3-job)")),
    }
}
