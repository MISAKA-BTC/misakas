//! **`palw-qwen36-fp-worker` — the free-prompt worker for the Qwen3.6 hybrid tier.**
//!
//! `misaka-palw-gateway` drives one worker binary over the two-mode contract (`--mode
//! v3-manifest`, `--mode v3-job --trace-out <dir>`). `palw-a16-fp-worker` was the only implementor
//! and so the only tier a browser prompt could reach; this is the same contract on
//! `Qwen36Backend::execute_free_prompt` (ADR-0075), which commits the captured step leg the attempt
//! lane commits, priced by its leaf count, so a seat can replay it and a court can try it.
//!
//! * **The class is looked up, never assembled here.** `MISAKA_PALW_MODEL_ID` names a catalog row
//!   (default `Qwen3.6-35B-A3B/graph-v3`, the class testnet-11 registers); the row's registered
//!   graph is what the backend serves, and the artifact must fit the row's shape or the worker
//!   refuses to start.
//! * **The network id is the operator's** (`MISAKA_PALW_NETWORK_ID`), never a constant: every
//!   committed root hangs off a context hash that absorbs it, and a seat replaying the claim
//!   derives that hash from the node's own network name.
//! * **The tokenizer comes from the GGUF header** (`MISAKA_PALW_GGUF`): a `.palwq36` artifact
//!   deliberately carries no tokenizer (PALW binds a prompt by the hash of its token ids), so the
//!   job's `tokenizer_id` is zero and nothing on the chain checks it — the same position the A16
//!   artifact is in today.
//! * The artifact (`MISAKA_PALW_ARTIFACT`, a converted `.palwq36`) is mapped per job; the page
//!   cache carries the 33 GiB between jobs.

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1 as _;
use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_job_context_v3};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFpWorkerInputV3, PalwFpWorkerRequestV3,
    PalwFpWorkerResultV3, PalwFreePromptJobV3, fp_job_id_v3, fp_worker_request_hash_v3,
};
use kaspa_consensus_core::palw_v2::{PALW_V2_MAX_FRAME_BYTES, prompt_token_ids_hash_v2, read_framed, write_framed};
use kaspa_hashes::Hash64;
use misaka_palw_base0::classes::qwen36_canonical_classes_v1;
use misaka_palw_base0::gguf::parse_directory;
use misaka_palw_base0::qwen36::open_artifact;
use misaka_palw_base0::qwen36_backend::Qwen36Backend;
use misaka_palw_base0::tokenizer::QwenTokenizer;
use std::io::{Read, Write};
use std::path::PathBuf;

const DEFAULT_MODEL_ID: &str = "Qwen3.6-35B-A3B/graph-v3";
const NETWORK_ID_ENV: &str = "MISAKA_PALW_NETWORK_ID";

fn die(msg: String) -> ! {
    eprintln!("[palw-qwen36-fp-worker] fatal: {msg}");
    std::process::exit(1);
}

fn hex(h: Hash64) -> String {
    faster_hex::hex_string(h.as_byte_slice())
}

struct Loaded {
    backend: Qwen36Backend,
    network_id: Vec<u8>,
    tokenizer: QwenTokenizer,
    model_id: String,
    shape_id: Hash64,
    n_ctx: u32,
    vocab: u32,
    class_id: Hash64,
    load_ms: u64,
}

/// The GGUF header, grown until the directory parses — the tokenizer lives in the metadata, and
/// the weights behind it are never read.
fn read_gguf_header(path: &str) -> Vec<u8> {
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

fn load() -> Loaded {
    let network_id = std::env::var(NETWORK_ID_ENV).unwrap_or_else(|_| {
        die(format!(
            "{NETWORK_ID_ENV} is not set. It must be the network this worker produces for — the same string kaspad \
             prints for its params.net (e.g. testnet-11) — because every committed root hangs off a context hash that \
             absorbs it, and a seat replaying this producer's claim derives that hash from the node's own network name."
        ))
    });
    let artifact_path = std::env::var("MISAKA_PALW_ARTIFACT").unwrap_or_else(|_| die("MISAKA_PALW_ARTIFACT is not set".into()));
    let gguf_path =
        std::env::var("MISAKA_PALW_GGUF").unwrap_or_else(|_| die("MISAKA_PALW_GGUF is not set (the tokenizer source)".into()));
    let model_id = std::env::var("MISAKA_PALW_MODEL_ID").unwrap_or_else(|_| DEFAULT_MODEL_ID.to_string());

    let row = qwen36_canonical_classes_v1()
        .into_iter()
        .find(|row| row.model_id == model_id)
        .unwrap_or_else(|| die(format!("this build's catalog has no {model_id} row")));
    if row.graph_version < 2 {
        die(format!("{model_id} is a legacy (graph-v1) row whose graph the court cannot adjudicate; use its /graph-v3 row"));
    }
    let profile = row.profile().unwrap_or_else(|e| die(format!("{model_id}: the row's geometry does not project: {e:?}")));

    let started = std::time::Instant::now();
    let header = read_gguf_header(&gguf_path);
    let directory = parse_directory(&header).unwrap_or_else(|e| die(format!("{gguf_path}: {e}")));
    let get = |key: &str| directory.metadata.get(key);
    let tokens = get("tokenizer.ggml.tokens").and_then(|v| v.as_strings()).unwrap_or_else(|| die("no tokenizer.ggml.tokens".into()));
    let merges = get("tokenizer.ggml.merges").and_then(|v| v.as_strings()).unwrap_or_else(|| die("no tokenizer.ggml.merges".into()));
    let types = get("tokenizer.ggml.token_type").and_then(|v| v.as_ints()).unwrap_or(&[]);
    let tokenizer = QwenTokenizer::from_gguf(tokens, merges, types).unwrap_or_else(|e| die(format!("{gguf_path}: {e}")));
    drop(header);

    let artifact = open_artifact(std::path::Path::new(&artifact_path)).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    row.shape_matches(&artifact.shape).unwrap_or_else(|e| die(format!("{artifact_path} is not a {model_id} artifact: {e}")));
    let load_ms = started.elapsed().as_millis() as u64;

    let n_ctx = profile.n_ctx;
    let vocab = artifact.shape.vocab as u32;
    let class_id = profile.shape_profile_id();
    let net = network_id.into_bytes();
    let backend =
        Qwen36Backend::with_class_profile(std::sync::Arc::new(artifact), model_id.clone(), row.canonical_job, profile, net.clone());
    if !backend.supports_court() {
        die(format!("this build cannot serve {model_id}'s registered graph, so it cannot commit a step leg for it"));
    }
    let shape_id = backend.shape_id();
    Loaded { backend, network_id: net, tokenizer, model_id, shape_id, n_ctx, vocab, class_id, load_ms }
}

fn run_manifest() {
    let loaded = load();
    let doc = serde_json::json!({
        "schema": "misaka.palw.fp-v3-manifest.v1",
        "runtime_manifest_hash": hex(Hash64::default()),
        "runtime_class_id": hex(loaded.shape_id),
        "model_profile_id": hex(loaded.shape_id),
        "shape_profile_id": hex(loaded.class_id),
        "trace_scheme_id": hex(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1()),
        "tokenizer_id": hex(Hash64::default()),
        "n_ctx": loaded.n_ctx,
        "prefill_single_batch_cap": loaded.n_ctx,
        "shape_string": loaded.model_id,
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
    mismatch("model_profile_id", loaded.shape_id, request.model_profile_id);
    mismatch("runtime_class_id", loaded.shape_id, request.runtime_class_id);
    mismatch("runtime_manifest_hash", Hash64::default(), request.runtime_manifest_hash);
    mismatch("trace_scheme_id", kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1(), request.trace_scheme_id);

    if request.max_context_tokens == 0 || request.max_context_tokens > loaded.n_ctx {
        die(format!(
            "v3-job rejected: max_context_tokens {} is outside this class's 1..={}",
            request.max_context_tokens, loaded.n_ctx
        ));
    }

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
    if let Some(bad) = prompt_ids.iter().find(|t| **t >= loaded.vocab) {
        die(format!("v3-job rejected: token id {bad} is outside the model's vocab ({})", loaded.vocab));
    }
    let prefill = prompt_ids.len() as u32;
    if prefill as u64 + request.decode_token_limit as u64 > request.max_context_tokens as u64 {
        die(format!(
            "v3-job rejected: prompt {prefill} + decode ceiling {} exceeds max_context_tokens {}",
            request.decode_token_limit, request.max_context_tokens
        ));
    }

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
        tokenizer_id: Hash64::default(),
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

    let class_facts = PalwFpClassFactsV3 {
        model_profile_id: loaded.shape_id,
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: loaded.shape_id,
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

    let retain_dir = trace_out.join(hex(binding));
    std::fs::create_dir_all(&retain_dir)
        .unwrap_or_else(|e| die(format!("cannot create the retention dir {}: {e}", retain_dir.display())));
    let material_path = retain_dir.join("material.bin");
    std::fs::write(&material_path, &run.outcome.material)
        .unwrap_or_else(|e| die(format!("cannot retain {}: {e}", material_path.display())));
    let manifest_doc = serde_json::json!({
        "schema": "misaka.palw.fp-v3-qwen36-retention.v1",
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
        "[palw-qwen36-fp-worker] v3 executed: prefill={prefill} decode={}/{} in {execute_ms}ms ({} leaves); exec root={}…",
        run.facts.decode_tokens_executed,
        job.decode_token_limit,
        run.facts.step_leaf_count,
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
