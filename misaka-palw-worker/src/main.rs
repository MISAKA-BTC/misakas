//! `palw-worker` — the pinned Qwen3.5-2B palw-lite runtime, behind the exact subprocess contract
//! `misaka_palw::PalwWorkerRuntime` drives:
//!
//! * `--mode manifest` → `{ runtime_manifest_hash, runtime_class_id }`
//! * `--mode self-job --prompt-stdin --n-predict N` → execute the job over the stdin bytes
//! * `--mode verify   --prompt-stdin --n-predict N` → independently re-execute the same job
//!
//! and the **v2 full-logits-trace interface** (`kaspa_consensus_core::palw_v2`,
//! docs/palw-full-logits-trace-v2-design.md — Land stage, devnet/shadow/zero-credit only):
//!
//! * `--mode v2-job` → one framed Borsh `PalwJobEnvelopeV2` on stdin, one framed Borsh
//!   `PalwJobResultV2` on stdout. Token IDs are the input identity (the worker never
//!   tokenizes on this path), the decode budget is exact (`--n-predict` does not exist here,
//!   early EOG is telemetry and never terminates), non-finite logits abort with no output,
//!   and every commitment is bound to the job context. On ANY failure the worker exits
//!   non-zero having written NOTHING to stdout — partial results never leave the process.
//! * `--mode v2-manifest` → display-only JSON of the `RuntimeManifestV2` identity (the
//!   canonical identity is the manifest hash, not this JSON).
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

use kaspa_consensus_core::palw_v2::{
    canonical_compute_units_v2, cu_ruleset_id_v2, decode_framed_borsh, expected_schedule_commitment_v2,
    golden_vector_root_unpopulated_v2, job_request_hash_v2, logits_event_hash_v2, output_commitment_v2,
    output_token_ids_hash_v2, read_framed, rendered_output_hash_v2, shape_profile_id_v2, tokenizer_id_v2_for_gguf,
    trace_scheme_id_v2, write_framed, PalwGoldenExpectedV2, PalwGoldenJobV2, PalwGoldenVectorSetV2, PalwJobContextV2,
    PalwJobEnvelopeV2, PalwJobResultV2, PalwJobTelemetryV2, PalwLogitsDtypeV2, PalwResultProjectionV2,
    PalwRuntimeManifestV2, PalwScheduleCommitmentBuilderV2, PalwStopReasonV2, PalwTraceCommitmentV2, PalwTracePhaseV2,
    PalwTraceSummaryV2, PALW_GOLDEN_SET_VERSION_V2, PALW_JOB_WIRE_VERSION_V2, PALW_RUNTIME_MANIFEST_VERSION_V2,
    PALW_V2_MAX_FRAME_BYTES,
};
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
const N_THREADS: i32 = qwen35_pins::CPU_THREADS;
#[cfg(misaka_palw_cpu)]
const SHAPE_STRING: &str = "n_ctx=4096/n_batch=512/n_ubatch=512/n_seq=1/n_threads=4/flash-attn=disabled/gpu-layers=none/greedy-argmax-first-index/v1";
#[cfg(not(misaka_palw_cpu))]
const SHAPE_STRING: &str = "n_ctx=4096/n_batch=512/n_ubatch=512/n_seq=1/n_threads=4/flash-attn=disabled/gpu-layers=all/greedy-argmax-first-index/v1";
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

/// The build profile this binary actually IS. Selected by the same cfg `build.rs` uses to
/// compile the shim, so the identity a node reports cannot drift from the kernels it runs — the
/// one failure that would make an honest worker refute honest peers.
#[cfg(misaka_palw_cpu)]
const BUILD_PROFILE: &str = qwen35_pins::CPU_BUILD_PROFILE;
#[cfg(not(misaka_palw_cpu))]
const BUILD_PROFILE: &str = qwen35_pins::METAL_BUILD_PROFILE;
#[cfg(misaka_palw_cpu)]
const RUNTIME_CLASS: &str = qwen35_pins::CPU_RUNTIME_CLASS;
#[cfg(not(misaka_palw_cpu))]
const RUNTIME_CLASS: &str = qwen35_pins::METAL_RUNTIME_CLASS;

fn runtime_manifest_hash() -> Hash64 {
    derive_runtime_hash(
        qwen35_pins::LLAMA_COMMIT,
        qwen35_pins::LLAMA_PATCH_SHA256,
        qwen35_pins::LLAMA_BUILD_NUMBER,
        BUILD_PROFILE,
    )
}

fn runtime_class_id() -> Hash64 {
    derive_runtime_class_id(RUNTIME_CLASS)
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
// PALW v2 (full-logits trace scheme v2) — the canonical token-ID job path.
//
// Everything below is additive: the v1 `self-job`/`verify` path above is a frozen consensus
// surface (devnet algo 4) and is deliberately not refactored. The v2 path has its own shape
// string, its own identifiers and its own execution loop; the two share only the FFI shim and
// the pinned model artifacts.
// ---------------------------------------------------------------------------------------------

/// The floating-point control state the canonical profile fixes (v2 design §8). The probe reads
/// the live control register; the v2 path refuses to run — before AND after model load — unless
/// the state is exactly the profile's.
mod fp_env {
    pub struct FpEnvironment {
        pub rounding_rne: bool,
        pub ftz: bool,
        pub daz: bool,
    }

    impl FpEnvironment {
        pub fn canonical_string(&self) -> String {
            format!("rounding={},ftz={},daz={}", if self.rounding_rne { "rne" } else { "other" }, self.ftz as u8, self.daz as u8)
        }
        pub fn is_canonical(&self) -> bool {
            self.rounding_rne && !self.ftz && !self.daz
        }
    }

    #[cfg(target_arch = "x86_64")]
    pub fn probe() -> FpEnvironment {
        // MXCSR: rounding-control bits 13–14 (0b00 = round-to-nearest ties-to-even),
        // FTZ bit 15, DAZ bit 6.
        let mxcsr = unsafe { core::arch::x86_64::_mm_getcsr() };
        FpEnvironment { rounding_rne: (mxcsr >> 13) & 0b11 == 0, ftz: mxcsr & (1 << 15) != 0, daz: mxcsr & (1 << 6) != 0 }
    }

    #[cfg(target_arch = "aarch64")]
    pub fn probe() -> FpEnvironment {
        // FPCR: RMode bits 22–23 (0b00 = round-to-nearest ties-to-even), FZ bit 24. AArch64 has
        // no separate input-DAZ control — FZ governs both directions, so it is reported as both.
        let fpcr: u64;
        unsafe { core::arch::asm!("mrs {fpcr}, fpcr", fpcr = out(reg) fpcr, options(nomem, nostack, preserves_flags)) };
        let fz = fpcr & (1 << 24) != 0;
        FpEnvironment { rounding_rne: (fpcr >> 22) & 0b11 == 0, ftz: fz, daz: fz }
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    pub fn probe() -> FpEnvironment {
        // Unsupported architecture: report a non-canonical state so the v2 path fails closed
        // rather than executing under an unverified floating-point environment.
        FpEnvironment { rounding_rne: false, ftz: true, daz: true }
    }
}

/// The runtime-enforced half of the §8 policy; the build-side half (no fast-math, contraction
/// policy) is covered by `cmake_cache_sha256` in the manifest and by the activation-gate audit.
const FP_ENVIRONMENT_PROFILE_V2: &str = "rounding=rne,ftz=0,daz=0";

/// The v2 prompt template identity: there is none — token IDs are the input, and that absence
/// is itself pinned so a templated variant can never share this manifest.
const PALW_PROMPT_TEMPLATE_V2: &str = "none/token-ids-input/v2";

/// The v2 execution shape, built from the ACTUAL constants (the v1 `SHAPE_STRING` hardcoded
/// `n_threads=4` beside a `CPU_THREADS` constant that could drift; here drift is impossible).
fn shape_string_v2() -> String {
    let gpu_layers = if cfg!(misaka_palw_cpu) { "none" } else { "all" };
    format!(
        "n_ctx={N_CTX}/n_batch={N_BATCH}/n_ubatch={N_BATCH}/n_seq=1/n_threads={N_THREADS}/flash-attn=disabled/gpu-layers={gpu_layers}/greedy-argmax-first-index/token-ids-input/exact-decode/no-early-eog/prefill-single-batch/v2"
    )
}

fn hex_to_32(hex_str: &str, what: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    if hex_str.len() != 64 || faster_hex::hex_decode(hex_str.as_bytes(), &mut out).is_err() {
        die(format!("{what} is not 64 hex chars: {hex_str:?}"));
    }
    out
}

/// SHA-256 of this running binary — the live artifact hash the release manifest pins
/// (VPS design §16 P0 item 9). Self-measurement is an accidental-drift guard, not a proof:
/// a hostile host can fake it, which is why network correctness rests on independent replay.
fn worker_binary_sha256() -> [u8; 32] {
    let exe = std::env::current_exe().unwrap_or_else(|e| die(format!("cannot resolve own binary path: {e}")));
    let mut file = std::fs::File::open(&exe).unwrap_or_else(|e| die(format!("cannot open own binary {}: {e}", exe.display())));
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher).unwrap_or_else(|e| die(format!("cannot hash own binary {}: {e}", exe.display())));
    hasher.finalize().into()
}

/// The host's CPUID exposure — conformance telemetry, deliberately NOT a manifest-hash input:
/// with `GGML_CPU_ALL_VARIANTS=OFF` the instruction stream is fixed at build time, so a host
/// missing a required feature fails to execute (the safe failure) instead of computing
/// differently, and two hosts whose CPUID supersets differ still run identical arithmetic.
fn host_cpu_features_string() -> String {
    #[cfg(target_arch = "x86_64")]
    {
        format!(
            "sse4.2={},avx={},avx2={},fma={},f16c={}",
            std::arch::is_x86_feature_detected!("sse4.2") as u8,
            std::arch::is_x86_feature_detected!("avx") as u8,
            std::arch::is_x86_feature_detected!("avx2") as u8,
            std::arch::is_x86_feature_detected!("fma") as u8,
            std::arch::is_x86_feature_detected!("f16c") as u8,
        )
    }
    #[cfg(target_arch = "aarch64")]
    {
        format!("neon=1,dotprod={}", std::arch::is_aarch64_feature_detected!("dotprod") as u8)
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        "unknown".to_string()
    }
}

fn ggml_flag(v: &str) -> bool {
    v == "1"
}

/// Assembles this binary's `RuntimeManifestV2` from measured values: build-time hashes of the
/// exact CMake cache and static libraries linked (captured by `build.rs`), the pinned model
/// identity, and the live self-hash. `golden_root` is the registered set's canonical root, or
/// the `"unpopulated"` sentinel when none is registered. Fields a Land-stage build cannot
/// verify carry the literal `"unpinned"` — visible, hash-bound, and a mandatory rejection at
/// class registration.
fn runtime_manifest_v2(worker_sha: [u8; 32], golden_root: Hash64) -> PalwRuntimeManifestV2 {
    PalwRuntimeManifestV2 {
        version: PALW_RUNTIME_MANIFEST_VERSION_V2,
        target_arch: std::env::consts::ARCH.to_string(),
        target_triple: env!("MISAKA_PALW_TARGET_TRIPLE").to_string(),
        compiler_name: "rustc+cc".to_string(),
        compiler_version: env!("MISAKA_PALW_RUSTC_VERSION").to_string(),
        linker_version: "unpinned".to_string(),
        cmake_cache_sha256: hex_to_32(env!("MISAKA_PALW_CMAKE_CACHE_SHA256"), "build-time CMake cache SHA-256"),
        worker_binary_sha256: worker_sha,
        llama_static_library_sha256: hex_to_32(env!("MISAKA_PALW_LLAMA_LIBS_SHA256"), "build-time llama libs SHA-256"),
        llama_cpp_commit: qwen35_pins::LLAMA_COMMIT.to_string(),
        patchset_root: qwen35_pins::LLAMA_PATCH_SHA256.to_string(),
        exact_cpu_isa_baseline: "unpinned".to_string(),
        // The class property is what the BUILD requires, not what this host's CPUID happens to
        // expose (see `host_cpu_features_string`). Unverified until the disassembly audit pins
        // it — the same audit that produced the aarch64-dotprod finding.
        runtime_cpu_feature_mask: "build-required:unpinned".to_string(),
        ggml_native: ggml_flag(env!("MISAKA_PALW_GGML_NATIVE")),
        ggml_openmp: ggml_flag(env!("MISAKA_PALW_GGML_OPENMP")),
        ggml_blas: ggml_flag(env!("MISAKA_PALW_GGML_BLAS")),
        ggml_accelerate: ggml_flag(env!("MISAKA_PALW_GGML_ACCELERATE")),
        ggml_sse42: ggml_flag(env!("MISAKA_PALW_GGML_SSE42")),
        ggml_avx: ggml_flag(env!("MISAKA_PALW_GGML_AVX")),
        ggml_avx2: ggml_flag(env!("MISAKA_PALW_GGML_AVX2")),
        ggml_fma: ggml_flag(env!("MISAKA_PALW_GGML_FMA")),
        ggml_f16c: ggml_flag(env!("MISAKA_PALW_GGML_F16C")),
        ggml_cpu_all_variants: ggml_flag(env!("MISAKA_PALW_GGML_CPU_ALL_VARIANTS")),
        thread_count: N_THREADS as u32,
        thread_affinity_policy: "none/v1".to_string(),
        floating_point_environment: FP_ENVIRONMENT_PROFILE_V2.to_string(),
        gguf_sha256: hex_to_32(qwen35_pins::GGUF_SHA256, "pinned GGUF SHA-256"),
        // The tokenizer ships inside the GGUF; the artifact digest IS the tokenizer identity.
        tokenizer_sha256: hex_to_32(qwen35_pins::GGUF_SHA256, "pinned GGUF SHA-256 (embedded tokenizer)"),
        prompt_template_sha256: {
            let mut h = sha2::Sha256::new();
            h.update(PALW_PROMPT_TEMPLATE_V2.as_bytes());
            h.finalize().into()
        },
        trace_scheme_id: trace_scheme_id_v2(),
        golden_vector_root: golden_root,
    }
}

/// The v2 model gate: same pins as v1, but the SHA-256 is ALWAYS recomputed from the bytes —
/// the canonical policy forbids trusting a (path, size, mtime) cache for artifact identity
/// (VPS design §4.4). Costs a full read of the 1.2 GB file per job process; the persistent
/// agent (P1) amortizes it, correctness does not wait for it.
fn pinned_model_path_v2() -> PathBuf {
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
    let mut file = std::fs::File::open(&path).unwrap_or_else(|e| die(format!("cannot open model at {}: {e}", path.display())));
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher).unwrap_or_else(|e| die(format!("cannot read model at {}: {e}", path.display())));
    let sha = faster_hex::hex_string(&hasher.finalize());
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

// ---- golden vectors: registration, boot self-test, generation --------------------------------

/// Points at the registered golden vector set (framed Borsh `PalwGoldenVectorSetV2`). When set,
/// its canonical root replaces the `"unpopulated"` sentinel inside `RuntimeManifestV2` — so the
/// manifest hash CHANGES when goldens are registered, which is intended: a runtime with and
/// without a registered boot gate are different runtimes.
const PALW_GOLDEN_ENV: &str = "MISAKA_PALW_GOLDEN";

/// Loads, validates and identity-checks a golden set. A set generated under another class,
/// model or shape is refused — vectors must not certify a runtime they were not made under.
fn load_golden_set(path: &str) -> PalwGoldenVectorSetV2 {
    let mut file = std::fs::File::open(path).unwrap_or_else(|e| die(format!("cannot open golden set at {path}: {e}")));
    let payload = read_framed(&mut file, PALW_V2_MAX_FRAME_BYTES).unwrap_or_else(|e| die(format!("golden set at {path} rejected: {e}")));
    // Peek the schema version BEFORE decoding. `version` is the first field of the Borsh layout,
    // and a set from an older layout fails to decode structurally — which surfaces as
    // "Unexpected length of input" at some offset and tells an operator nothing about what to do.
    // Reading the two bytes first turns that into the actionable error.
    let declared_version = match payload.get(..2) {
        Some(bytes) => u16::from_le_bytes([bytes[0], bytes[1]]),
        None => die(format!("golden set at {path} rejected: truncated — {} bytes cannot hold a version", payload.len())),
    };
    if declared_version != PALW_GOLDEN_SET_VERSION_V2 {
        die(format!(
            "golden set at {path} rejected: schema version {declared_version}, but this worker speaks \
             {PALW_GOLDEN_SET_VERSION_V2}. Version {PALW_GOLDEN_SET_VERSION_V2} added the measured build identity \
             (cmake_cache_sha256, llama_static_library_sha256) that a v{declared_version} set does not carry, so it \
             cannot be checked against this build. Regenerate it on this build: \
             `palw-worker --mode v2-golden-gen --out {path}`."
        ));
    }
    let set: PalwGoldenVectorSetV2 =
        decode_framed_borsh(&payload).unwrap_or_else(|e| die(format!("golden set at {path} rejected: {e}")));
    set.validate_shape(N_CTX as u32).unwrap_or_else(|e| die(format!("golden set at {path} rejected: {e}")));
    let checks: [(&str, Hash64, Hash64); 3] = [
        ("runtime_class_id", runtime_class_id(), set.runtime_class_id),
        ("model_profile_id", model_profile_id(), set.model_profile_id),
        ("shape_profile_id", shape_profile_id_v2(&shape_string_v2()), set.shape_profile_id),
    ];
    for (field, ours, theirs) in checks {
        if ours != theirs {
            die(format!(
                "golden set at {path} rejected: {field} mismatch — the set was generated under a runtime this worker is not (ours {}, set {})",
                hex(ours),
                hex(theirs)
            ));
        }
    }
    // The three ids above are DECLARED identity: `runtime_class_id` hashes a compile-time class
    // string, so two builds that claim one class and compute different arithmetic pass all three.
    // The pair below is MEASURED. Without it, a host built with GGML_OPENMP=ON accepts a set
    // generated with it OFF, passes its own boot self-test, and then disagrees with the fleet on
    // every real job — surfacing as a determinism failure whose actual cause is the build. This is
    // also the gate for a validator's own claim: `PalwCapabilityDeclarationV2` bonds a
    // `runtime_manifest_hash` that covers exactly these bytes.
    let build_checks: [(&str, [u8; 32], [u8; 32], &str); 2] = [
        (
            "cmake_cache_sha256",
            hex_to_32(env!("MISAKA_PALW_CMAKE_CACHE_SHA256"), "build-time CMake cache SHA-256"),
            set.cmake_cache_sha256,
            "the llama.cpp tree was configured differently (ggml flags such as GGML_OPENMP, or the toolchain)",
        ),
        (
            "llama_static_library_sha256",
            hex_to_32(env!("MISAKA_PALW_LLAMA_LIBS_SHA256"), "build-time llama libs SHA-256"),
            set.llama_static_library_sha256,
            "different llama.cpp/ggml archives are linked into this worker",
        ),
    ];
    for (field, ours, theirs, why) in build_checks {
        if ours != theirs {
            die(format!(
                "golden set at {path} rejected: {field} mismatch — {why}. The vectors were MEASURED under \
                 a build this worker is not, so passing them would prove nothing (ours {}, set {}). Rebuild \
                 to match the fleet, or regenerate the set on this build with --mode v2-golden-gen.",
                faster_hex::hex_string(&ours),
                faster_hex::hex_string(&theirs)
            ));
        }
    }
    set
}

/// The golden root the manifest carries: the registered set's canonical root when
/// `MISAKA_PALW_GOLDEN` is set, else the explicit `"unpopulated"` sentinel.
fn resolve_golden_root() -> Hash64 {
    match std::env::var(PALW_GOLDEN_ENV) {
        Ok(path) => load_golden_set(&path).golden_root(),
        Err(_) => golden_vector_root_unpopulated_v2(),
    }
}

/// The fixed boot-probe corpus. Inputs only — expectations are measured by `v2-golden-gen` on a
/// reference machine of the class. Chosen to cover the profile's edges: the D=1 path (the trace
/// is a single Prefill event), the standard calibration decode, a prefill near the single-batch
/// bound, and a degenerate repeated-token prompt.
fn golden_probe_inputs() -> Vec<(&'static str, [u8; 32], Vec<u32>, u32)> {
    vec![
        ("golden-min-1tok-d1", [0x01; 32], vec![0], 1),
        ("golden-probe-12tok-d16", [0xA5; 32], vec![1000, 42, 7, 31337, 9999, 5, 88, 12345, 3, 777, 2024, 66], 16),
        ("golden-prefill96-d16", [0x33; 32], (0..96u32).map(|i| (i * 97 + 13) % 50_000).collect(), 16),
        ("golden-repeat8-d2", [0x5A; 32], vec![7; 8], 2),
    ]
}

const GOLDEN_NETWORK_ID: &[u8] = b"misaka-golden-v2";

/// `--mode v2-golden-gen --out <path>`: execute the probe corpus on THIS machine and write the
/// measured set. Run it on a reference machine of the class; a second machine of the class must
/// then pass `v2-selftest` against the same file — that second-machine agreement, not this
/// generation step, is what earns the word "golden" (v2 design §5 CAUTION).
fn run_v2_golden_gen(out_path: &str) {
    let fp = fp_env::probe();
    if !fp.is_canonical() {
        die(format!("v2-golden-gen refused: non-canonical floating-point environment: {}", fp.canonical_string()));
    }
    let model_path = pinned_model_path_v2();
    let mut set = PalwGoldenVectorSetV2 {
        version: PALW_GOLDEN_SET_VERSION_V2,
        runtime_class_id: runtime_class_id(),
        model_profile_id: model_profile_id(),
        shape_profile_id: shape_profile_id_v2(&shape_string_v2()),
        // Measured, from the same build-time values the runtime manifest is assembled out of —
        // so the set records the build it was generated under, not the class it claims.
        cmake_cache_sha256: hex_to_32(env!("MISAKA_PALW_CMAKE_CACHE_SHA256"), "build-time CMake cache SHA-256"),
        llama_static_library_sha256: hex_to_32(env!("MISAKA_PALW_LLAMA_LIBS_SHA256"), "build-time llama libs SHA-256"),
        jobs: Vec::new(),
    };
    for (name, seed, ids, decode) in golden_probe_inputs() {
        let mut job = PalwGoldenJobV2 {
            name: name.to_string(),
            network_id: GOLDEN_NETWORK_ID.to_vec(),
            execution_seed: seed,
            prompt_token_ids: ids,
            exact_decode_tokens: decode,
            max_context_tokens: N_CTX as u32,
            expected: PalwGoldenExpectedV2 {
                job_context_hash: golden_vector_root_unpopulated_v2(),
                full_logits_trace_root: golden_vector_root_unpopulated_v2(),
                output_commitment: golden_vector_root_unpopulated_v2(),
                operation_schedule_commitment: golden_vector_root_unpopulated_v2(),
                canonical_compute_units: 0,
                prefill_tokens: 0,
                decode_tokens: 0,
                trace_event_count: 0,
            },
        };
        let envelope = set.envelope_for(&job);
        envelope.validate_shape(N_CTX as u32).unwrap_or_else(|e| die(format!("golden probe {name} is malformed: {e}")));
        let exec = execute_v2(&model_path, &envelope);
        job.expected = PalwGoldenExpectedV2 {
            job_context_hash: exec.projection.job_context_hash,
            full_logits_trace_root: exec.projection.full_logits_trace_root,
            output_commitment: exec.projection.output_commitment,
            operation_schedule_commitment: exec.projection.operation_schedule_commitment,
            canonical_compute_units: exec.projection.canonical_compute_units,
            prefill_tokens: exec.projection.prefill_tokens,
            decode_tokens: exec.projection.decode_tokens,
            trace_event_count: exec.projection.trace_event_count,
        };
        set.jobs.push(job);
    }
    set.validate_shape(N_CTX as u32).unwrap_or_else(|e| die(format!("generated golden set is invalid: {e}")));
    let bytes = borsh::to_vec(&set).unwrap_or_else(|e| die(format!("cannot serialize the golden set: {e}")));
    let mut file = std::fs::File::create(out_path).unwrap_or_else(|e| die(format!("cannot create {out_path}: {e}")));
    write_framed(&mut file, &bytes).unwrap_or_else(|e| die(format!("cannot write {out_path}: {e}")));
    let doc = serde_json::json!({
        "schema": "misaka.palw.v2-golden-gen.debug",
        "out": out_path,
        "golden_root": hex(set.golden_root()),
        "runtime_class_id": hex(set.runtime_class_id),
        "jobs": set.jobs.iter().map(|j| serde_json::json!({
            "name": j.name,
            "prefill": j.expected.prefill_tokens,
            "decode": j.expected.decode_tokens,
            "root": hex(j.expected.full_logits_trace_root),
        })).collect::<Vec<_>>(),
    });
    println!("{doc}");
}

/// `--mode v2-selftest`: re-execute every registered golden job and compare the FULL projection
/// (64-byte roots, counts, CU). Any mismatch exits non-zero — a supervisor must treat that as
/// QUARANTINED and never enable compute over a worker that failed its own class's vectors.
fn run_v2_selftest() {
    let path = std::env::var(PALW_GOLDEN_ENV)
        .unwrap_or_else(|_| die(format!("{PALW_GOLDEN_ENV} is not set; v2-selftest needs the registered golden set")));
    let set = load_golden_set(&path);
    let fp = fp_env::probe();
    if !fp.is_canonical() {
        die(format!("v2-selftest FAILED before execution: non-canonical floating-point environment: {}", fp.canonical_string()));
    }
    let model_path = pinned_model_path_v2();
    let mut results = Vec::new();
    for job in &set.jobs {
        let envelope = set.envelope_for(job);
        let exec = execute_v2(&model_path, &envelope);
        let got = PalwGoldenExpectedV2 {
            job_context_hash: exec.projection.job_context_hash,
            full_logits_trace_root: exec.projection.full_logits_trace_root,
            output_commitment: exec.projection.output_commitment,
            operation_schedule_commitment: exec.projection.operation_schedule_commitment,
            canonical_compute_units: exec.projection.canonical_compute_units,
            prefill_tokens: exec.projection.prefill_tokens,
            decode_tokens: exec.projection.decode_tokens,
            trace_event_count: exec.projection.trace_event_count,
        };
        if got != job.expected {
            die(format!(
                "v2-selftest FAILED on {:?}: this machine does not reproduce the registered vectors \
                 (expected root {}, got {}) — QUARANTINE, do not enable compute",
                job.name,
                hex(job.expected.full_logits_trace_root),
                hex(got.full_logits_trace_root)
            ));
        }
        eprintln!("[palw-worker] v2-selftest ok: {} (root {}…)", job.name, &hex(got.full_logits_trace_root)[..16]);
        results.push(serde_json::json!({ "name": job.name, "root": hex(got.full_logits_trace_root) }));
    }
    let doc = serde_json::json!({
        "schema": "misaka.palw.v2-selftest.debug",
        "status": "pass",
        "jobs": results,
        "golden_root": hex(set.golden_root()),
        "runtime_manifest_hash_v2": hex(runtime_manifest_v2(worker_binary_sha256(), set.golden_root()).manifest_hash()),
    });
    println!("{doc}");
}

/// `--mode v2-golden-show`: display a golden set without loading the model.
fn run_v2_golden_show() {
    let path = std::env::var(PALW_GOLDEN_ENV)
        .unwrap_or_else(|_| die(format!("{PALW_GOLDEN_ENV} is not set; v2-golden-show needs a golden set to show")));
    let set = load_golden_set(&path);
    let doc = serde_json::json!({
        "schema": "misaka.palw.v2-golden-show.debug",
        "golden_root": hex(set.golden_root()),
        "runtime_class_id": hex(set.runtime_class_id),
        "model_profile_id": hex(set.model_profile_id),
        "shape_profile_id": hex(set.shape_profile_id),
        "jobs": set.jobs.iter().map(|j| serde_json::json!({
            "name": j.name,
            "network_id": String::from_utf8_lossy(&j.network_id),
            "seed": faster_hex::hex_string(&j.execution_seed),
            "prompt_tokens": j.prompt_token_ids.len(),
            "exact_decode_tokens": j.exact_decode_tokens,
            "expected_root": hex(j.expected.full_logits_trace_root),
            "expected_cu": j.expected.canonical_compute_units.to_string(),
        })).collect::<Vec<_>>(),
    });
    println!("{doc}");
}

/// Rejects a job whose declared runtime identity is not THIS worker. Every profile field must
/// match (VPS design §8.3: "profile ID不一致" is an admission rejection) — running a job under a
/// mis-declared identity would let one runtime impersonate another's determinism class.
fn check_v2_identity(envelope: &PalwJobEnvelopeV2, manifest: &PalwRuntimeManifestV2) {
    let checks: [(&str, Hash64, Hash64); 6] = [
        ("model_profile_id", model_profile_id(), envelope.model_profile_id),
        ("runtime_manifest_hash", manifest.manifest_hash(), envelope.runtime_manifest_hash),
        ("runtime_class_id", runtime_class_id(), envelope.runtime_class_id),
        ("shape_profile_id", shape_profile_id_v2(&shape_string_v2()), envelope.shape_profile_id),
        ("trace_scheme_id", trace_scheme_id_v2(), envelope.trace_scheme_id),
        ("cu_ruleset_id", cu_ruleset_id_v2(), envelope.cu_ruleset_id),
    ];
    for (field, ours, declared) in checks {
        if ours != declared {
            die(format!(
                "v2-job rejected: {field} mismatch — the envelope declares a runtime this worker is not (ours {}, envelope {})",
                hex(ours),
                hex(declared)
            ));
        }
    }
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

struct ExecutionV2 {
    projection: PalwResultProjectionV2,
    telemetry: PalwJobTelemetryV2,
}

/// One decode call of the v2 loop: feed, record the schedule, capture the full logits row,
/// commit it as a context-bound event. Non-finite logits abort the process here — before the
/// row can influence sampling and before any output exists to leak.
#[allow(clippy::too_many_arguments)]
fn step_v2(
    ctx: *mut ShimCtx,
    fed: &[i32],
    phase: PalwTracePhaseV2,
    phase_step: u32,
    event_index: u32,
    n_vocab: u32,
    job_context_hash: &Hash64,
    logits: &mut [f32],
    scratch: &mut Vec<u8>,
    events: &mut Vec<Hash64>,
    schedule: &mut PalwScheduleCommitmentBuilderV2,
) {
    let rc = unsafe { shim_decode(ctx, fed.as_ptr(), fed.len() as i32) };
    if rc != 0 {
        die(format!("llama_decode failed with {rc}"));
    }
    schedule.record_call(fed.len() as u32);
    let got = unsafe { shim_logits_last(ctx, logits.as_mut_ptr()) };
    if got != n_vocab as i32 {
        die(format!("logits unavailable after decode (got {got})"));
    }
    match logits_event_hash_v2(job_context_hash, phase, phase_step, event_index, n_vocab, logits, scratch) {
        Ok(event) => events.push(event),
        // v2 design §8: NaN/±Inf = execution invalid, fail closed — no receipt.
        Err(e) => die(format!("v2-job execution invalid: {e}")),
    }
}

fn execute_v2(model_path: &Path, envelope: &PalwJobEnvelopeV2) -> ExecutionV2 {
    let load_started = std::time::Instant::now();
    let ctx = unsafe { shim_open(format!("{}\0", model_path.display()).as_ptr(), N_CTX, N_BATCH, N_THREADS) };
    if ctx.is_null() {
        die(format!("llama.cpp failed to load {}", model_path.display()));
    }
    let n_vocab_raw = unsafe { shim_n_vocab(ctx) };
    if n_vocab_raw <= 0 {
        die(format!("model reports a non-positive vocab size ({n_vocab_raw})"));
    }
    let n_vocab = n_vocab_raw as u32;
    let model_load_ms = load_started.elapsed().as_millis() as u64;
    eprintln!("[palw-worker] model loaded in {:?} (n_vocab={n_vocab})", load_started.elapsed());

    if let Err(e) = envelope.validate_against_vocab(n_vocab) {
        die(format!("v2-job rejected: {e}"));
    }
    let prefill = envelope.declared_prefill_tokens();
    if prefill > N_BATCH as u32 {
        // The canonical prefill schedule is a single batch; a longer prompt is a different
        // schedule and therefore a different shape profile, not a silent chunking decision.
        die(format!("v2-job rejected: prefill {prefill} exceeds the single-batch prefill schedule (n_batch={N_BATCH})"));
    }

    // Re-probe AFTER model load: a linked backend that flipped MXCSR/FPCR during init would
    // change the arithmetic of every decode call.
    let fp = fp_env::probe();
    if !fp.is_canonical() {
        die(format!("v2-job rejected: floating-point environment drifted after model load: {}", fp.canonical_string()));
    }

    let context = PalwJobContextV2::from_envelope(envelope, tokenizer_id_v2_for_gguf(qwen35_pins::GGUF_SHA256));
    let job_context_hash = context.context_hash();

    let exec_started = std::time::Instant::now();
    let d = envelope.exact_decode_tokens;
    let tokens: Vec<i32> = envelope.prompt_token_ids.iter().map(|t| *t as i32).collect();
    let mut logits = vec![0f32; n_vocab as usize];
    let mut scratch: Vec<u8> = Vec::with_capacity(n_vocab as usize * 4);
    let mut events: Vec<Hash64> = Vec::with_capacity(d as usize);
    let mut schedule = PalwScheduleCommitmentBuilderV2::new(&job_context_hash);

    // Call 0: the prefill batch. Its final-position logits are event 0.
    step_v2(
        ctx,
        &tokens,
        PalwTracePhaseV2::Prefill,
        0,
        0,
        n_vocab,
        &job_context_hash,
        &mut logits,
        &mut scratch,
        &mut events,
        &mut schedule,
    );

    let mut outputs: Vec<u32> = Vec::with_capacity(d as usize);
    let mut rendered: Vec<u8> = Vec::new();
    let mut piece = vec![0u8; 512];
    let mut eog_first: Option<u32> = None;
    loop {
        let tok = argmax(&logits);
        outputs.push(tok as u32);
        let n = unsafe { shim_token_to_piece(ctx, tok, piece.as_mut_ptr(), piece.len() as i32) };
        if n > 0 {
            rendered.extend_from_slice(&piece[..n as usize]);
        }
        // Early EOG is telemetry, never termination: the exact-decode policy runs to D so a
        // miner cannot shrink the work by hunting seeds that reach EOG quickly (VPS §5.5).
        if eog_first.is_none() && unsafe { shim_is_eog(ctx, tok) } == 1 {
            eog_first = Some(outputs.len() as u32 - 1);
        }
        if outputs.len() as u32 == d {
            break;
        }
        let event_index = outputs.len() as u32;
        let fed = [tok];
        step_v2(
            ctx,
            &fed,
            PalwTracePhaseV2::Decode,
            event_index - 1,
            event_index,
            n_vocab,
            &job_context_hash,
            &mut logits,
            &mut scratch,
            &mut events,
            &mut schedule,
        );
    }
    unsafe { shim_close(ctx) };

    // Defense in depth: the streamed schedule must equal the schedule the shape mandates. A
    // divergence here is a worker bug, and a receipt must not paper over it.
    let (schedule_commitment, calls) = schedule.finalize();
    let (expected_schedule, expected_calls) = expected_schedule_commitment_v2(&job_context_hash, prefill, d);
    if schedule_commitment != expected_schedule || calls != expected_calls {
        die("internal error: the executed call schedule diverged from the canonical schedule".into());
    }

    let summary = PalwTraceSummaryV2 {
        vocab_size: n_vocab,
        logits_dtype: PalwLogitsDtypeV2::F32Le,
        declared_prefill_tokens: prefill,
        exact_decode_tokens: d,
        event_count: d,
        first_event_kind: PalwTracePhaseV2::Prefill,
        last_event_kind: if d == 1 { PalwTracePhaseV2::Prefill } else { PalwTracePhaseV2::Decode },
        output_token_ids_hash: output_token_ids_hash_v2(&outputs),
        stop_reason: PalwStopReasonV2::ExactBudgetReached,
    };
    let commitment = PalwTraceCommitmentV2::assemble(context, summary, events)
        .unwrap_or_else(|e| die(format!("internal error: trace commitment failed to assemble: {e}")));

    let projection = PalwResultProjectionV2 {
        job_context_hash,
        full_logits_trace_root: commitment.full_logits_sequence_root,
        output_commitment: output_commitment_v2(&job_context_hash, &outputs, &rendered_output_hash_v2(&rendered)),
        operation_schedule_commitment: schedule_commitment,
        canonical_compute_units: canonical_compute_units_v2(prefill, d),
        prefill_tokens: prefill,
        decode_tokens: d,
        trace_event_count: d,
        stop_reason: PalwStopReasonV2::ExactBudgetReached,
    };
    // Observability policy (VPS §14): root PREFIX and counts only — no prompt, no logits, no
    // complete rendered output on this path.
    eprintln!(
        "[palw-worker] v2 executed: prefill={prefill} decode={d} in {:?}; root={}…",
        exec_started.elapsed(),
        &hex(commitment.full_logits_sequence_root)[..16]
    );
    ExecutionV2 {
        projection,
        telemetry: PalwJobTelemetryV2 {
            model_load_ms,
            execute_ms: exec_started.elapsed().as_millis() as u64,
            eog_first_seen_at_decode_index: eog_first,
        },
    }
}

/// `--mode v2-job`: one framed request in, one framed response out, nothing else on stdout.
fn run_v2_job() {
    let mut stdin = std::io::stdin().lock();
    let payload = read_framed(&mut stdin, PALW_V2_MAX_FRAME_BYTES).unwrap_or_else(|e| die(format!("v2-job rejected: {e}")));
    let request_hash = job_request_hash_v2(&payload);
    let envelope: PalwJobEnvelopeV2 = decode_framed_borsh(&payload).unwrap_or_else(|e| die(format!("v2-job rejected: {e}")));
    envelope.validate_shape(N_CTX as u32).unwrap_or_else(|e| die(format!("v2-job rejected: {e}")));
    if envelope.deadline_unix_ms != 0 && now_unix_ms() >= envelope.deadline_unix_ms {
        // The supervisor enforces the wall-clock kill; refusing at admission just avoids
        // burning a model load on a job whose result can no longer be used.
        die("v2-job rejected: deadline already passed at admission".into());
    }
    let fp = fp_env::probe();
    if !fp.is_canonical() {
        die(format!(
            "v2-job rejected: floating-point environment is not the canonical profile ({}; required {FP_ENVIRONMENT_PROFILE_V2})",
            fp.canonical_string()
        ));
    }
    let manifest = runtime_manifest_v2(worker_binary_sha256(), resolve_golden_root());
    check_v2_identity(&envelope, &manifest);
    let model_path = pinned_model_path_v2();
    let exec = execute_v2(&model_path, &envelope);
    let result = PalwJobResultV2 {
        version: PALW_JOB_WIRE_VERSION_V2,
        request_hash,
        job_id: envelope.job_id,
        projection: exec.projection,
        telemetry: exec.telemetry,
    };
    let bytes = borsh::to_vec(&result).unwrap_or_else(|e| die(format!("cannot serialize the v2 result: {e}")));
    let mut stdout = std::io::stdout().lock();
    write_framed(&mut stdout, &bytes).unwrap_or_else(|e| die(format!("cannot write the v2 result frame: {e}")));
}

/// `--mode v2-manifest`: display-only JSON. The canonical identity is the manifest HASH over
/// the canonical preimage; this document exists so a harness can build envelopes and an
/// operator can read what the binary is. Does not load the model.
fn run_v2_manifest() {
    let fp = fp_env::probe();
    let manifest = runtime_manifest_v2(worker_binary_sha256(), resolve_golden_root());
    let doc = serde_json::json!({
        "schema": "misaka.palw.v2-manifest.debug",
        "note": "display document; canonical identity is runtime_manifest_hash_v2 over the canonical preimage",
        "runtime_manifest_hash_v2": hex(manifest.manifest_hash()),
        "runtime_class_id": hex(runtime_class_id()),
        "model_profile_id": hex(model_profile_id()),
        "tokenizer_id_v2": hex(tokenizer_id_v2_for_gguf(qwen35_pins::GGUF_SHA256)),
        "shape_string_v2": shape_string_v2(),
        "shape_profile_id_v2": hex(shape_profile_id_v2(&shape_string_v2())),
        "trace_scheme_id_v2": hex(trace_scheme_id_v2()),
        "cu_ruleset_id_v2": hex(cu_ruleset_id_v2()),
        "worker_binary_sha256": faster_hex::hex_string(&manifest.worker_binary_sha256),
        "cmake_cache_sha256": faster_hex::hex_string(&manifest.cmake_cache_sha256),
        "llama_static_library_sha256": faster_hex::hex_string(&manifest.llama_static_library_sha256),
        "golden_vector_root": hex(manifest.golden_vector_root),
        "golden_registered": std::env::var(PALW_GOLDEN_ENV).is_ok(),
        "ggml_flags": {
            "native": manifest.ggml_native,
            "openmp": manifest.ggml_openmp,
            "blas": manifest.ggml_blas,
            "accelerate": manifest.ggml_accelerate,
            "sse42": manifest.ggml_sse42,
            "avx": manifest.ggml_avx,
            "avx2": manifest.ggml_avx2,
            "fma": manifest.ggml_fma,
            "f16c": manifest.ggml_f16c,
            "cpu_all_variants": manifest.ggml_cpu_all_variants,
        },
        "fp_environment_probe": fp.canonical_string(),
        "fp_environment_canonical": fp.is_canonical(),
        "host_cpu_features": host_cpu_features_string(),
        "max_context_tokens": N_CTX,
        "prefill_max_batch_tokens": N_BATCH,
        "thread_count": N_THREADS,
    });
    println!("{doc}");
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
    step(ctx, &tokens, &mut logits, &mut logits_bytes, &mut trace_events, &mut trace_event_count, &mut schedule, &mut schedule_event_count);
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
    let output_commitment = keyed64(
        b"misaka-palw-lite/output/v1",
        &[&(outputs.len() as u64).to_le_bytes(), &output_ids, &[0xff], &rendered],
    );
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
    let mut out_path: Option<String> = None;
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
            "--out" => {
                i += 1;
                out_path = args.get(i).cloned();
            }
            other => die(format!("unknown argument {other:?}")),
        }
        i += 1;
    }

    // The v2 interface deliberately has no `--n-predict`: the prefill/decode budgets are
    // separate, explicit envelope fields (VPS design §5.4), and accepting the ambiguous v1
    // flag here would reintroduce exactly the total-vs-decode confusion v2 removes.
    if matches!(mode.as_deref(), Some("v2-job" | "v2-manifest" | "v2-golden-gen" | "v2-selftest" | "v2-golden-show"))
        && n_predict.is_some()
    {
        die("--n-predict is not a v2 interface; token budgets come from the job envelope".into());
    }

    match mode.as_deref() {
        Some("v2-job") => run_v2_job(),
        Some("v2-manifest") => run_v2_manifest(),
        Some("v2-golden-gen") => {
            let out = out_path.unwrap_or_else(|| die("--out <path> is required for v2-golden-gen".into()));
            run_v2_golden_gen(&out);
        }
        Some("v2-selftest") => run_v2_selftest(),
        Some("v2-golden-show") => run_v2_golden_show(),
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
            "usage: palw-worker --mode manifest | --mode self-job|verify --prompt-stdin --n-predict N | --mode v2-job | --mode v2-manifest (got mode {other:?})"
        )),
    }
}
