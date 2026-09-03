//! **`palw-qwen36-fp-worker` — the free-prompt worker for the Qwen3.6 hybrid tier.**
//!
//! The same v3 contract `palw-a16-fp-worker` serves, on `Qwen36Backend::execute_free_prompt`
//! (ADR-0075), which commits the captured step leg the attempt lane commits, priced by its leaf
//! count, so a seat can replay it and a court can try it.
//!
//! **What is left in this file is what is this family's alone** (ADR-0077 Decision 1): how the
//! artifact and the tokenizer are opened, and which catalog row the runtime embodies. Everything
//! after the request is [`misaka_palw_base0::fp_worker`], shared with the dense tier.
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
//! * **The artifact is mapped ONCE.** It used to be mapped per job — `load()` inside `run_job`,
//!   about eight minutes for the 33 GiB hybrid, per request — and `--mode v3-serve` is what ends
//!   that: one mapping, one manifest handshake, then a resident request loop. `v3-job` remains the
//!   one-shot form the drills and the replay arm use, and maps per invocation because that is what
//!   a one-shot IS.
//! * **The served width is the CLASS's** `n_ctx` (eight positions, prompt and answer together),
//!   read from the catalog row and never from the artifact's rotary span, which still covers 512.
//!   A runtime that answered wider than the court admits is the two-products split ADR-0077 R0
//!   exists to close.

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1 as _;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_WORKER_MODE_JOB_V3, PALW_FP_WORKER_MODE_MANIFEST_V3, PALW_FP_WORKER_MODE_SERVE_V3,
};
use kaspa_hashes::Hash64;
use misaka_palw_base0::classes::qwen36_canonical_classes_v1;
use misaka_palw_base0::fp_worker::{FpWorkerFamilyV1, FpWorkerRuntime, MappedArtifactV1, QWEN_EOG_TOKEN_NAMES};
use misaka_palw_base0::gguf::parse_directory;
use misaka_palw_base0::qwen36::open_artifact;
use misaka_palw_base0::qwen36_backend::Qwen36Backend;
use misaka_palw_base0::tokenizer::QwenTokenizer;
use std::io::Read;
use std::path::PathBuf;

const DEFAULT_MODEL_ID: &str = "Qwen3.6-35B-A3B/graph-v3";
const NETWORK_ID_ENV: &str = "MISAKA_PALW_NETWORK_ID";

fn die(msg: String) -> ! {
    eprintln!("[palw-qwen36-fp-worker] fatal: {msg}");
    std::process::exit(1);
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

fn load() -> FpWorkerRuntime<Qwen36Backend> {
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

    // **ADR-0077 SA-6 / ADR-0079 Decision 9, and the reason this costs a full read here.** The
    // `.palwq36` container is MAPPED, not held: its pages are the file's, so a rewrite of the file
    // rewrites this process's weights, and a truncation turns the next touch into a `SIGBUS`. A
    // one-shot worker verified the artifact by consuming it; a resident one has to say when it
    // last looked. The read is paid once at startup (the fleet measures ~1.3 GB/s for reads this
    // size, so ~26 s for the 33 GiB hybrid against the eight minutes the mapping used to cost per
    // JOB), and again only when the file's device, inode or size has moved.
    let guard = MappedArtifactV1::verify_by_reading(std::path::Path::new(&artifact_path)).unwrap_or_else(|e| die(e));
    let artifact = open_artifact(std::path::Path::new(&artifact_path)).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    row.shape_matches(&artifact.shape).unwrap_or_else(|e| die(format!("{artifact_path} is not a {model_id} artifact: {e}")));
    let load_ms = started.elapsed().as_millis() as u64;

    // **The ladder is the RULESET's, not the module default** (ADR-0082 Decision 8, W1b): the
    // number that decides how many tokens a user gets is the court's `max_step_leaf_count`, and
    // this binary was pricing every job against `PALW_STEP_MAX_LEAVES` because that is what
    // `step_leaf_count` reaches for when nobody states one.
    let court = misaka_palw_base0::fp_worker::fp_worker_court_params_v1(&network_id).unwrap_or_else(|why| die(why));
    let net = network_id.into_bytes();
    let backend = Qwen36Backend::with_class_profile(
        std::sync::Arc::new(artifact),
        model_id.clone(),
        row.canonical_job,
        profile.clone(),
        net.clone(),
    )
    .with_step_ladder_cap(court.max_step_leaf_count());
    if !backend.supports_court() {
        die(format!("this build cannot serve {model_id}'s registered graph, so it cannot commit a step leg for it"));
    }
    let shape_id = backend.shape_id();
    FpWorkerRuntime::new(
        backend,
        &profile,
        tokenizer,
        FpWorkerFamilyV1 {
            model_id,
            // This family's runtime is the SHAPE its backend serves, not a file digest: the
            // `.palwq36` container is mapped, not read whole, and the shape id is what the request
            // pins as `model_profile_id` and `runtime_class_id`.
            runtime_identity: shape_id,
            // Zero, and stated rather than defaulted: the artifact carries no tokenizer, PALW
            // binds a prompt by the hash of its ids, and nothing on chain checks this field.
            tokenizer_id: Hash64::default(),
            // The row's shape check above already pins the artifact's vocab to the row's, so the
            // profile is the one place this is read from.
            vocab: profile.vocab_size,
            retention_schema: "misaka.palw.fp-v3-qwen36-retention.v1",
            retention_family: "qwen36",
            eog_token_names: QWEN_EOG_TOKEN_NAMES,
            artifact: Some(guard),
        },
        net,
        load_ms,
    )
    .unwrap_or_else(|e| die(e))
}

fn main() {
    // ADR-0079 Decision 5: the supervisor's filter is installed before this exec and cannot deny
    // it; seccomp filters stack, so the worker denies `execve` on itself. A no-op unless a
    // confining supervisor spawned us.
    if let misaka_palw::host_security::ExecveDenial::Failed(why) = misaka_palw::host_security::confine_self_after_exec() {
        die(format!("refusing to run: cannot stack the execve denial: {why}"));
    }
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let trace_out = || -> PathBuf {
        PathBuf::from(flag("--trace-out").unwrap_or_else(|| {
            die("--trace-out <dir> is required: a commitment whose producer retained nothing cannot serve an opening and \
                 would default in court"
                .into())
        }))
    };
    match flag("--mode").as_deref() {
        Some(PALW_FP_WORKER_MODE_MANIFEST_V3) => {
            let rt = load();
            misaka_palw_base0::fp_worker::run_v3_manifest_v1(&rt, &mut std::io::stdout().lock()).unwrap_or_else(|e| die(e));
        }
        Some(PALW_FP_WORKER_MODE_JOB_V3) => {
            let dir = trace_out();
            misaka_palw_base0::fp_worker::run_v3_job_v1(&mut std::io::stdin().lock(), &mut std::io::stdout().lock(), &dir, load)
                .unwrap_or_else(|e| die(e));
        }
        // The resident mode. Eight minutes of mapping, once — and then every job after it is the
        // decode alone, which is what makes a 33 GiB class usable at all.
        Some(PALW_FP_WORKER_MODE_SERVE_V3) => {
            let dir = trace_out();
            let rt = load();
            eprintln!(
                "[palw-qwen36-fp-worker] v3-serve: {} resident after {} ms, n_ctx {}, artifact {}",
                rt.manifest().model_id,
                rt.load_ms(),
                rt.manifest().n_ctx,
                rt.artifact().map(|a| a.digest_hex()).unwrap_or_else(|| "unverified".to_string())
            );
            misaka_palw_base0::fp_worker::run_v3_serve_v1(&rt, &mut std::io::stdin().lock(), &mut std::io::stdout().lock(), &dir)
                .unwrap_or_else(|e| die(e));
        }
        other => die(format!(
            "unsupported --mode {other:?} ({PALW_FP_WORKER_MODE_MANIFEST_V3} | {PALW_FP_WORKER_MODE_JOB_V3} | \
             {PALW_FP_WORKER_MODE_SERVE_V3})"
        )),
    }
}
