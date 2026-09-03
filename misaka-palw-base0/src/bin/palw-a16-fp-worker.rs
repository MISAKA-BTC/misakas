//! **The integer family's free-prompt worker: the dense tier's one runtime.**
//!
//! `misaka-palw-gateway` drives one worker binary over the v3 contract, and the only implementor
//! of it was `palw-worker` — the pinned-llama.cpp runtime, whose own v3 path documents that it
//! cannot produce a `committed_execution_root` ("no worker path on this tree captures a step leg
//! at all") and therefore returns a value consensus refuses. The A16 backend CAN: its
//! `execute_free_prompt` captures all four legs and its execution root is the one
//! `palw_fp_execution_root_v3` recomputes (proven by
//! `the_corrected_a16_class_commits_the_root_the_derivation_recomputes`).
//!
//! **What is left in this file is what is this family's alone** (ADR-0077 Decision 1): how the
//! artifact and the tokenizer are opened, and which catalog row the runtime embodies. Everything
//! after the request — the three modes, the identity checks, the segment-wise prompt, the
//! streamed tokens, the capture retention and the result frame — is
//! [`misaka_palw_base0::fp_worker`], shared with the hybrid tier, because the two binaries were
//! near-duplicates and a duplicated commitment path is two places for the court's inputs to drift.
//!
//! ```text
//!   --mode v3-manifest   the identity, as one JSON line              (map, print, exit)
//!   --mode v3-job        one framed request in, one result out       (map, run, exit)
//!   --mode v3-serve      the manifest, then a resident request loop  (map ONCE, then jobs)
//! ```
//!
//! Design points that survived the lift, each the resolution of a way this could silently diverge
//! from the court:
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
//! * **The served width is the CLASS's** `n_ctx`, read from the catalog row and never from the
//!   artifact's rotary table, which covers 512 positions against the row's 16. A runtime that
//!   answered at the artifact's width would answer wider than the court admits — the two-products
//!   split ADR-0077 R0 exists to close. The width becomes a practical one by moving the class
//!   table (Phase B), not by widening this binary.
//!
//! The artifact and tokenizer arrive by environment (`MISAKA_PALW_ARTIFACT`,
//! `MISAKA_PALW_TOKENIZER`) because the gateway spawns the worker with only the mode flags — the
//! same shape as `palw-worker`'s pinned-model env.

use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_WORKER_MODE_JOB_V3, PALW_FP_WORKER_MODE_MANIFEST_V3, PALW_FP_WORKER_MODE_SERVE_V3,
};
use misaka_palw_base0::artifact::decode_artifact_file_v1;
use misaka_palw_base0::classes::canonical_class_by_model_id_v1;
use misaka_palw_base0::fp_worker::{FpWorkerFamilyV1, FpWorkerRuntime, MappedArtifactV1, QWEN_EOG_TOKEN_NAMES};
use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;
use misaka_palw_base0::tokenizer::QwenTokenizer;
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

/// Artifact + tokenizer + catalog row, refused loudly when any of the three disagree. There is no
/// digest check against a constant here: the artifact IS the identity (the chain registers its
/// digest as `artifact_root`), so what must agree is artifact-shape vs catalog-shape, which
/// `Qwen25A16Backend`'s own probe enforces at execution.
///
/// What IS pinned is that the file does not move under a resident process (ADR-0077 SA-6): the
/// artifact's bytes are digested here, out of the buffer they were read into rather than by
/// reading them twice, and [`misaka_palw_base0::fp_worker::MappedArtifactV1`] re-verifies before
/// every job whenever the file's device, inode or size has changed.
fn load() -> FpWorkerRuntime<Qwen25A16Backend> {
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

    // **The ladder is the RULESET's, not this module's constant** (ADR-0082 Decision 8, W1b).
    // `PALW_STEP_MAX_LEAVES` decided how many tokens a user got — measured, 12 decode tokens on a
    // 26-token prompt — and it is a default rather than a rule. The court the network's own
    // binary ships is what the class catalog is walked against and what the capture prices
    // against, and they must be the same object or the worker serves rows its own chain refuses.
    let court = misaka_palw_base0::fp_worker::fp_worker_court_params_v1(&network_id).unwrap_or_else(|why| die(why));
    let entry =
        canonical_class_by_model_id_v1(&court, MODEL_ID).unwrap_or_else(|| die(format!("this build's catalog has no {MODEL_ID} row")));

    let started = std::time::Instant::now();
    let bytes = std::fs::read(&artifact_path).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    let guard = MappedArtifactV1::verify_from_bytes(std::path::Path::new(&artifact_path), &bytes).unwrap_or_else(|e| die(e));
    let artifact = decode_artifact_file_v1(&bytes).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    let digest = artifact.artifact_digest();
    // **The pair is checked, not assumed.** The artifact's `tokenizer_commitment` is inside its
    // digest, so the artifact CAN name the tokenizer its ids belong to — but until the file this
    // process opened is hashed and compared, that naming decides nothing, and a wrong
    // `tokenizer.json` fails SILENTLY: different ids, a different `prompt_token_ids_hash`, a
    // different `job_context_hash`, and an honest producer defaulted for a claim no seat can
    // reproduce. So the comparison happens here, once, where its answer is a name.
    let tokenizer_bytes = std::fs::read(&tokenizer_path).unwrap_or_else(|e| die(format!("{tokenizer_path}: {e}")));
    let binding = artifact.check_tokenizer_bytes_v1(&tokenizer_bytes);
    if let Some(why) = binding.refusal() {
        die(format!("{tokenizer_path}: {why} (artifact {artifact_path})"));
    }
    if binding == misaka_palw_base0::artifact::TokenizerBindingV1::Undeclared {
        // Stated at boot rather than defaulted in silence: the artifacts converted before
        // `qwen25-convert --a16` bound a commitment carry `Hash64::default()`, so this worker's
        // jobs publish `tokenizer_id` zero and NOTHING — here or on chain — can prove that the
        // file below is the one the weights were converted with. Binding it needs a re-converted
        // artifact, which moves the artifact root and is a genesis decision.
        eprintln!(
            "[palw-a16-fp-worker] WARNING: artifact {artifact_path} declares no tokenizer commitment, so {tokenizer_path}              was checked against nothing and every job publishes tokenizer_id 0. A replayer using a different              tokenizer.json derives different token ids and cannot reproduce this producer's claims."
        );
    }
    let tokenizer_commitment = binding.tokenizer_id();
    let tokenizer = QwenTokenizer::from_json(&tokenizer_bytes).unwrap_or_else(|e| die(format!("{tokenizer_path}: {e}")));
    let load_ms = started.elapsed().as_millis() as u64;

    let net = network_id.into_bytes();
    let backend = Qwen25A16Backend::new(std::sync::Arc::new(artifact), net.clone(), entry.profile.clone(), entry.canonical_job)
        .with_step_ladder_cap(court.max_step_leaf_count());
    FpWorkerRuntime::new(
        backend,
        &entry.profile,
        tokenizer,
        FpWorkerFamilyV1 {
            model_id: MODEL_ID.to_string(),
            // This family's runtime IS its artifact: the chain registers the digest as
            // `artifact_root`, and both `model_profile_id` and `runtime_class_id` are that value.
            runtime_identity: digest,
            // The answer the pair check gave for the file THIS process opened — the artifact's
            // commitment when it declares one (and the bytes matched, or we died above), zero
            // when it declares none.
            tokenizer_id: tokenizer_commitment,
            vocab: entry.profile.vocab_size,
            retention_schema: "misaka.palw.fp-v3-a16-retention.v1",
            retention_family: "qwen25-a16",
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
        // The resident mode: the artifact is mapped once here and never again, which is the whole
        // of ADR-0077 Decision 1 on the executor side.
        Some(PALW_FP_WORKER_MODE_SERVE_V3) => {
            let dir = trace_out();
            let rt = load();
            // The boot line names the artifact it will serve every job under (ADR-0079 Decision 5
            // prints the posture at boot; SA-6 makes the digest part of it) — and no prompt.
            eprintln!(
                "[palw-a16-fp-worker] v3-serve: {MODEL_ID} resident after {} ms, n_ctx {}, artifact {}",
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
