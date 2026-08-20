//! `palw-worker` — the pinned Qwen3.5-2B palw-lite runtime, behind the exact subprocess contract
//! `misaka_palw::PalwWorkerRuntime` drives:
//!
//! * `--mode manifest` → `{ runtime_manifest_hash, runtime_class_id }`
//! * `--mode self-job --prompt-stdin --n-predict N` → execute the job over the stdin bytes
//! * `--mode verify   --prompt-stdin --n-predict N` → independently re-execute the same job
//! * `--mode pow-agent` → the SAME v1 job, many times over one resident model: the pin is checked
//!   and the model loaded once at startup, then each newline-delimited JSON request on stdin
//!   yields one marked JSON frame on stdout (ADR-0041 Decision 1′). The document per job is the
//!   one `verify` prints, which is the point — a resident model computes the same tag, and this
//!   mode exists only to remove the ~97 % of a one-shot verification that is artifact read,
//!   SHA-256 and model load.
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

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use kaspa_consensus_core::palw_v2::{
    canonical_compute_units_v2, cu_ruleset_id_v2, decode_framed_borsh, expected_schedule_commitment_v2, full_logits_trace_root_v2,
    golden_vector_root_unpopulated_v2, job_request_hash_v2, logits_event_hash_v2, output_commitment_v2,
    output_token_ids_hash_v2, prompt_token_ids_hash_v2, read_framed, rendered_output_hash_v2, shape_profile_id_v2,
    tokenizer_id_v2_for_gguf, trace_event_merkle_root_v2, trace_scheme_id_v2, write_framed, PalwGoldenExpectedV2, PalwGoldenJobV2,
    PalwGoldenVectorSetV2, PalwJobContextV2,
    PalwJobEnvelopeV2, PalwJobResultV2, PalwJobTelemetryV2, PalwLogitsDtypeV2, PalwResultProjectionV2,
    PalwRuntimeManifestV2, PalwScheduleCommitmentBuilderV2, PalwStopReasonV2, PalwTraceCommitmentV2, PalwTracePhaseV2,
    PalwTraceSummaryV2, PALW_GOLDEN_SET_VERSION_V2, PALW_JOB_WIRE_VERSION_V2, PALW_RUNTIME_MANIFEST_VERSION_V3,
    PALW_V2_MAX_FRAME_BYTES,
};
use kaspa_consensus_core::palw_legs::{
    canonical_activation_leaf_coordinates, canonical_activation_leaf_count, canonical_activation_leaf_index,
    canonical_checkpoint_count, check_legs_opening_answer_v1, check_opening_request_shape, checkpoint_state_root_v1,
    execution_commitment_scheme_id_v1, leg_opening_v1, state_layout_id_v1, tap_semantics_id_v1, PalwActivationCoordinateV1,
    PalwActivationLeafV1, PalwActivationTapProfileV1, PalwCheckpointProfileV1, PalwLegsBindingV1, PalwLegsCommitmentBuilderV1,
    PalwLegsJobResultV1, PalwLegsMaterial, PalwLegsOpeningAnswerV1, PalwLegsOpeningCallV1, PalwLegsOpeningRequestV1,
    PalwOpenedActivationLeafV1, PalwOpenedCheckpointLeafV1, PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_LEAF,
    PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_NODE, PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_LEAF, PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_NODE,
    PALW_LEGS_MAX_ACTIVATION_LEAVES, PALW_LEGS_MAX_CHECKPOINTS, PALW_LEGS_OBJECT_VERSION_V1,
};
use kaspa_consensus_core::palw_freeprompt_v3::{
    fp_job_id_v3, fp_trace_manifest_v3, fp_worker_request_hash_v3, PalwFpStopReasonV3, PalwFpWorkerInputV3,
    PalwFpWorkerRequestV3, PalwFpWorkerResultV3, PalwFreePromptJobV3, PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V3_VERSION,
};
use kaspa_consensus_core::palw_schedule::{
    nearest_rank_percentile, replay_p99_fits_v1, PalwScheduleParamsV1, PALW_SCHEDULE_REPLAY_KAPPA,
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
    /// Returns the context to a pristine decode state without reloading the model — the primitive
    /// `--mode pow-agent` runs between jobs (ADR-0041 Decision 1′). Verified against the one-shot
    /// path: three jobs on one context produce byte-identical projections to each other and to a
    /// fresh process.
    fn shim_reset_context(s: *mut ShimCtx) -> i32;

    // Activation capture (execution-commitment legs v1). `shim_open_capture` with `n_taps = 0`
    // is `shim_open`; with taps it installs an eval callback, which is a different scheduler
    // path — see `run_v2_legs_selftest` for the measurement that says whether it is a different
    // ARITHMETIC path too.
    fn shim_open_capture(
        model_path: *const u8,
        n_ctx: i32,
        n_batch: i32,
        n_threads: i32,
        tap_layers: *const i32,
        n_taps: i32,
    ) -> *mut ShimCtx;
    fn shim_n_embd(s: *const ShimCtx) -> i32;
    fn shim_n_layer(s: *const ShimCtx) -> i32;
    // P0-8b: the remaining geometry a step-space shape profile restates. Measured from the
    // loaded model, never declared — a profile that disagrees with the GGUF describes an
    // execution that never ran, and the court would adjudicate steps against it.
    fn shim_n_head(s: *const ShimCtx) -> i32;
    fn shim_n_head_kv(s: *const ShimCtx) -> i32;
    fn shim_n_embd_head(s: *const ShimCtx) -> i32;
    fn shim_rope_type(s: *const ShimCtx) -> i32;
    fn shim_rope_freq_scale_train(s: *const ShimCtx) -> f32;
    // The GGUF's own key/value block. Most of a shape profile's constants (ffn dim, the norm
    // epsilons, the rope BASE, the vocab, the GatedDeltaNet geometry) have no llama.cpp accessor
    // and live only here.
    fn shim_meta_count(s: *const ShimCtx) -> i32;
    fn shim_meta_key_by_index(s: *const ShimCtx, i: i32, buf: *mut u8, buf_len: i32) -> i32;
    fn shim_meta_val_by_index(s: *const ShimCtx, i: i32, buf: *mut u8, buf_len: i32) -> i32;
    fn shim_capture_begin(s: *mut ShimCtx);
    fn shim_capture_status(s: *const ShimCtx) -> i32;
    fn shim_capture_positions(s: *const ShimCtx, slot: i32) -> i32;
    fn shim_capture_row(s: *const ShimCtx, slot: i32, position: i32, out: *mut f32, max_out: i32) -> i32;
    fn shim_state_seq_size(s: *mut ShimCtx) -> i32;
    fn shim_state_seq_read(s: *mut ShimCtx, out: *mut u8, max_out: i32) -> i32;
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

/// The resolved libm, probed behaviourally — the missing half of ADR-0031 (audit B8).
///
/// llama.cpp's GDN decay calls the C `expf` per (token, head) across 18 of 24 layers and that
/// arithmetic lands in the PoW tag, so *which* libm is linked is a class property. We resolve the
/// **same dynamic symbols llama.cpp resolves** — declared here rather than reached through
/// `f32::exp`, whose lowering Rust does not guarantee to be the libm call — and digest their
/// outputs over the frozen [`PALW_LIBM_PROBE_V1`] vector.
mod libm_probe {
    use kaspa_consensus_core::palw_v2::PALW_LIBM_PROBE_V1;
    use sha2::{Digest, Sha256};

    // The libm entry points llama.cpp itself calls. Linking these makes the probe measure the
    // resolved implementation (including an LD_PRELOAD or a patched build), not Rust's std.
    unsafe extern "C" {
        fn expf(x: f32) -> f32;
        fn logf(x: f32) -> f32;
    }

    /// Digest of `expf` then `logf` over the frozen probe vector. Raw output bits are hashed, so
    /// a one-ulp difference in either function is a different digest — and therefore a different
    /// class id — which is precisely the divergence B8 said could pass unannounced.
    pub fn arithmetic_digest() -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"misaka-palw/libm-probe/v1");
        for &bits in PALW_LIBM_PROBE_V1 {
            let x = f32::from_bits(bits);
            // SAFETY: both are pure, total libm functions on any float input (NaN/inf included).
            let (e, l) = unsafe { (expf(x), logf(x.abs())) };
            h.update(e.to_bits().to_le_bytes());
            h.update(l.to_bits().to_le_bytes());
        }
        h.finalize().into()
    }

    /// Human-readable identity for refusal messages. Diagnostic only — never load-bearing, since
    /// a version string can neither prove nor disprove that two hosts share the arithmetic.
    pub fn identity() -> String {
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        {
            unsafe extern "C" {
                fn gnu_get_libc_version() -> *const core::ffi::c_char;
            }
            // SAFETY: glibc always returns a valid static NUL-terminated string.
            let v = unsafe { core::ffi::CStr::from_ptr(gnu_get_libc_version()) };
            return format!("glibc/{}", v.to_string_lossy());
        }
        #[cfg(all(target_os = "linux", target_env = "musl"))]
        return "musl/unversioned".to_string();
        #[cfg(target_vendor = "apple")]
        return "apple/libsystem_m".to_string();
        #[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
        return format!("unknown/{}", std::env::consts::OS);
    }
}

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
        version: PALW_RUNTIME_MANIFEST_VERSION_V3,
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
        libm_identity: libm_probe::identity(),
        libm_arithmetic_digest: libm_probe::arithmetic_digest(),
        trace_scheme_id: trace_scheme_id_v2(),
        golden_vector_root: golden_root,
    }
}

/// The model gate — now the ONLY one, for every mode, always recomputing the SHA-256 from bytes.
///
/// The canonical policy forbids trusting a `(path, size, mtime)` cache for artifact identity
/// (VPS design §4.4). Costs a full read of the 1.2 GB file per job process; the persistent agent
/// (P1) amortizes it, correctness does not wait for it.
///
/// **There used to be a second gate with a hole (mainnet-readiness audit B15).** A v1
/// `pinned_model_path` consulted a `.palw-gguf-sha.json` in the *process working directory*, keyed
/// on `path|size|mtime`, and returned the cached digest on a key match — so writing that file made
/// any same-sized model pass the pin. Its one caller was `--mode verify`, the mode **block
/// validation itself invokes** (`consensus/pow/src/palw.rs::run_worker`), which put the bypass on
/// the consensus PoW path: a node running an unpinned model computes a different tag for every
/// header and silently forks itself off the network. v1 was folded into this function rather than
/// patched, so there is one policy and no second implementation to drift. Any cheaper check is
/// forgeable by whoever can write the cache, and the size check alone admits a same-size
/// substitute — the full read *is* the check.
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

// ---------------------------------------------------------------------------------------------
// PALW execution-commitment legs v1 — the capture side of `kaspa_consensus_core::palw_legs`.
//
// The schema was frozen first and deliberately: this file feeds it, it does not get to define
// it. Every canonical rule (leaf order, counts, the checkpoint chain, the fail-closed rule on
// non-finite values) lives in the builder there, so a capture bug is a build error here rather
// than a commitment a challenger convicts us on.
//
// The two opaque identities the schema left to registration are answered here, by declaration:
// what a tap reads, and what a checkpoint's bytes are. Both name the llama.cpp commit, because
// both are claims about a specific runtime's graph and serializer — not about "an LLM".
// ---------------------------------------------------------------------------------------------

/// The tapped tensor: the post-block residual stream, after the control-vector hook, which is
/// what the next block consumes. Tapping the residual stream (rather than, say, an attention
/// output) is the choice that makes a wrong row impossible to hide — every later layer, and
/// therefore the logits, is downstream of it.
fn tap_semantics_string() -> String {
    format!(
        "llama.cpp@{}/graph-node/l_out-{{il}}/post-block-residual-stream/f32-le/row-per-position/v1",
        qwen35_pins::LLAMA_COMMIT
    )
}

/// The checkpoint's bytes: llama.cpp's own sequence serialization, which is a format with a
/// version, hence the commit. Opaque on purpose — the commitment says "this runtime produced
/// these bytes at this point in the decode run", and only the same runtime can say it again.
fn state_layout_string() -> String {
    format!("llama.cpp@{}/llama_state_seq_get_data/seq-0/v1", qwen35_pins::LLAMA_COMMIT)
}

/// One checkpoint every 8 decode calls. A profile parameter, not a constant of the scheme: it
/// trades commitment size against how much re-execution a challenge costs (a challenger replays
/// from the last checkpoint, so the interval IS the worst-case replay length).
const CHECKPOINT_INTERVAL_V1: u32 = 8;

/// The tap schedule: quartile boundaries plus the final block. Four taps spread over depth
/// catch a divergence wherever it starts — an early tap that matches while a late one does not
/// localizes the fault to a layer range, which is what makes a challenge cheap.
fn canonical_tap_layers(n_layer: u32) -> Vec<u16> {
    let last = n_layer.saturating_sub(1);
    let mut layers: Vec<u16> =
        [n_layer / 4, n_layer / 2, (3 * n_layer) / 4, last].into_iter().map(|l| l.min(last) as u16).collect();
    layers.sort_unstable();
    layers.dedup();
    layers
}

/// Streams what the shim captured into the frozen builder. Holds no rows of its own beyond one
/// scratch row: the leg is tens of megabytes of f32 and the answer to a challenge is
/// re-execution, so nothing here needs to survive the job.
struct LegsCapture {
    builder: PalwLegsCommitmentBuilderV1,
    taps: u32,
    hidden_dim: u32,
    state_layout_id: Hash64,
    row: Vec<f32>,
    state_bytes: Vec<u8>,
    /// Rows captured, and how many of them were entirely zero. A backend whose tensor read
    /// silently no-ops would produce a perfectly stable, perfectly reproducible commitment to
    /// nothing — the one capture failure that does not announce itself, so it is counted.
    rows_seen: u64,
    rows_all_zero: u64,
    /// Coordinates whose rows the caller wants back (the open path); empty on the commit path.
    wanted: HashSet<(u32, u32, u32)>,
    retained: Vec<((u32, u32, u32), Vec<f32>)>,
}

/// What the open path keeps beside the binding: the tree material openings come from, and the
/// requested rows themselves. Process-local, like the material it wraps.
struct LegsOpenMaterial {
    material: PalwLegsMaterial,
    rows: Vec<((u32, u32, u32), Vec<f32>)>,
}

/// How (and whether) an execution captures legs.
enum LegsMode {
    /// No capture, no callback — the byte-identical frozen v2 path.
    Off,
    /// Capture and commit; material is dropped at the seal.
    Commit,
    /// Capture, commit, and keep what answering the named coordinates needs.
    Open(HashSet<(u32, u32, u32)>),
}

impl LegsCapture {
    /// Harvests one decode call. `expected_positions` is the schedule's claim about this call
    /// (the prefill's token count, or 1); a tap that captured anything else — including nothing,
    /// which means the graph never contained the node the profile claims to read — aborts the
    /// job. There is no partial-capture path: a short leg is a refutable commitment.
    fn harvest(&mut self, ctx: *mut ShimCtx, call_index: u32, expected_positions: u32) {
        let status = unsafe { shim_capture_status(ctx) };
        if status != 0 {
            die(format!("v2-legs execution invalid: activation capture fault {status} at call {call_index}"));
        }
        for tap_slot in 0..self.taps {
            let got = unsafe { shim_capture_positions(ctx, tap_slot as i32) };
            if got != expected_positions as i32 {
                die(format!(
                    "v2-legs execution invalid: tap {tap_slot} captured {got} positions at call {call_index}, expected {expected_positions}"
                ));
            }
        }
        // Tap-major then position — the pinned leaf order. The builder rejects any other order,
        // so this loop nesting is load-bearing, not stylistic.
        for tap_slot in 0..self.taps {
            for position in 0..expected_positions {
                let got =
                    unsafe { shim_capture_row(ctx, tap_slot as i32, position as i32, self.row.as_mut_ptr(), self.row.len() as i32) };
                if got != self.hidden_dim as i32 {
                    die(format!(
                        "v2-legs execution invalid: tap {tap_slot} position {position} of call {call_index} returned {got} values"
                    ));
                }
                self.rows_seen += 1;
                if self.row.iter().all(|v| *v == 0.0) {
                    self.rows_all_zero += 1;
                }
                if self.wanted.contains(&(call_index, tap_slot, position)) {
                    self.retained.push(((call_index, tap_slot, position), self.row.clone()));
                }
                self.builder
                    .push_activation_row(call_index, tap_slot, position, &self.row)
                    .unwrap_or_else(|e| die(format!("v2-legs execution invalid: {e}")));
            }
        }
    }

    /// Commits the replay state after `decode_call`. The bytes never leave the process — only
    /// the root does, and reproducing it requires having been in the same state.
    fn checkpoint(&mut self, ctx: *mut ShimCtx, decode_call: u32) {
        let size = unsafe { shim_state_seq_size(ctx) };
        if size <= 0 {
            die(format!("v2-legs execution invalid: replay state is unavailable at decode call {decode_call} (size {size})"));
        }
        self.state_bytes.clear();
        self.state_bytes.resize(size as usize, 0);
        let written = unsafe { shim_state_seq_read(ctx, self.state_bytes.as_mut_ptr(), size) };
        if written <= 0 {
            die(format!("v2-legs execution invalid: replay state read failed at decode call {decode_call} ({written})"));
        }
        self.state_bytes.truncate(written as usize);
        let state_root = checkpoint_state_root_v1(&self.state_layout_id, &self.state_bytes);
        self.builder
            .push_checkpoint(decode_call, state_root)
            .unwrap_or_else(|e| die(format!("v2-legs execution invalid: {e}")));
    }
}

struct ExecutionV2 {
    projection: PalwResultProjectionV2,
    telemetry: PalwJobTelemetryV2,
    /// `Some` only on the legs paths; the bare v2 path is unchanged and produces none.
    legs: Option<PalwLegsBindingV1>,
    /// `Some` only on the open path.
    open_material: Option<LegsOpenMaterial>,
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

/// The frozen v2 path: no capture, therefore no eval callback, therefore the exact scheduler
/// behaviour every v2 golden was measured under.
fn execute_v2(model_path: &Path, envelope: &PalwJobEnvelopeV2) -> ExecutionV2 {
    execute_v2_inner(model_path, envelope, LegsMode::Off)
}

/// The legs path: identical execution plus activation and checkpoint capture.
fn execute_v2_legs(model_path: &Path, envelope: &PalwJobEnvelopeV2) -> ExecutionV2 {
    execute_v2_inner(model_path, envelope, LegsMode::Commit)
}

/// The answering path: the legs path, additionally retaining the named rows and the tree
/// material. The computation is bit-identical to [`execute_v2_legs`] — retention is Rust-side
/// bookkeeping after each row is read, never a different call into the runtime.
fn execute_v2_legs_open(model_path: &Path, envelope: &PalwJobEnvelopeV2, wanted: HashSet<(u32, u32, u32)>) -> ExecutionV2 {
    execute_v2_inner(model_path, envelope, LegsMode::Open(wanted))
}

fn execute_v2_inner(model_path: &Path, envelope: &PalwJobEnvelopeV2, mode: LegsMode) -> ExecutionV2 {
    let capture_legs = !matches!(mode, LegsMode::Off);
    let keep_material = matches!(mode, LegsMode::Open(_));
    let wanted = match mode {
        LegsMode::Open(wanted) => wanted,
        _ => HashSet::new(),
    };
    let load_started = std::time::Instant::now();
    // Tap layers are a function of the model's depth, which is only known after load — so the
    // arm has to be provisional here and is checked against the real layer count below.
    let provisional_taps: Vec<i32> = if capture_legs {
        canonical_tap_layers(qwen35_pins::MODEL_LAYER_COUNT).into_iter().map(|l| l as i32).collect()
    } else {
        Vec::new()
    };
    let ctx = unsafe {
        shim_open_capture(
            format!("{}\0", model_path.display()).as_ptr(),
            N_CTX,
            N_BATCH,
            N_THREADS,
            provisional_taps.as_ptr(),
            provisional_taps.len() as i32,
        )
    };
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

    // The tap profile the capture was armed with, now checked against the model that actually
    // loaded. Pins first, measurement second: if the loaded artifact has a different depth or
    // width than the pins claim, every tap coordinate would be a lie about a different network.
    let mut legs = capture_legs.then(|| {
        let n_layer = unsafe { shim_n_layer(ctx) };
        let n_embd = unsafe { shim_n_embd(ctx) };
        if n_layer != qwen35_pins::MODEL_LAYER_COUNT as i32 || n_embd != qwen35_pins::MODEL_HIDDEN_DIM as i32 {
            die(format!(
                "v2-legs rejected: the loaded model is {n_layer} layers × {n_embd} wide, but the pins say {} × {}",
                qwen35_pins::MODEL_LAYER_COUNT,
                qwen35_pins::MODEL_HIDDEN_DIM
            ));
        }
        let tap_profile = PalwActivationTapProfileV1 {
            version: PALW_LEGS_OBJECT_VERSION_V1,
            tap_semantics_id: tap_semantics_id_v1(&tap_semantics_string()),
            tap_layer_indices: canonical_tap_layers(n_layer as u32),
            model_total_layers: n_layer as u16,
            hidden_dim: n_embd as u32,
            dtype: PalwLogitsDtypeV2::F32Le,
        };
        let state_layout_id = state_layout_id_v1(&state_layout_string());
        let checkpoint_profile = PalwCheckpointProfileV1 {
            version: PALW_LEGS_OBJECT_VERSION_V1,
            checkpoint_interval: CHECKPOINT_INTERVAL_V1,
            state_layout_id,
        };
        let taps = tap_profile.tap_count();
        let builder = PalwLegsCommitmentBuilderV1::new(context.clone(), tap_profile, checkpoint_profile)
            .unwrap_or_else(|e| die(format!("v2-legs rejected: {e}")));
        LegsCapture {
            builder,
            taps,
            hidden_dim: n_embd as u32,
            state_layout_id,
            row: vec![0f32; n_embd as usize],
            state_bytes: Vec::new(),
            rows_seen: 0,
            rows_all_zero: 0,
            wanted,
            retained: Vec::new(),
        }
    });

    let exec_started = std::time::Instant::now();
    let d = envelope.exact_decode_tokens;
    let tokens: Vec<i32> = envelope.prompt_token_ids.iter().map(|t| *t as i32).collect();
    let mut logits = vec![0f32; n_vocab as usize];
    let mut scratch: Vec<u8> = Vec::with_capacity(n_vocab as usize * 4);
    let mut events: Vec<Hash64> = Vec::with_capacity(d as usize);
    let mut schedule = PalwScheduleCommitmentBuilderV2::new(&job_context_hash);

    // Call 0: the prefill batch. Its final-position logits are event 0.
    if legs.is_some() {
        unsafe { shim_capture_begin(ctx) };
    }
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
    if let Some(capture) = legs.as_mut() {
        capture.harvest(ctx, 0, prefill);
    }

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
        if legs.is_some() {
            unsafe { shim_capture_begin(ctx) };
        }
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
        if let Some(capture) = legs.as_mut() {
            // Decode call k = event_index: one position, and a checkpoint on every interval
            // boundary. The checkpoint is taken AFTER the call it covers, so replaying from it
            // resumes at the next call — which is what makes a challenge cost one interval.
            capture.harvest(ctx, event_index, 1);
            if event_index.is_multiple_of(CHECKPOINT_INTERVAL_V1) {
                capture.checkpoint(ctx, event_index);
            }
        }
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

    // Seal the legs against the logits root they bind. `finish` is where a short leg or a
    // drifted checkpoint schedule becomes an error — the process must die here rather than emit
    // a receipt a challenger could convict it on.
    let (legs, open_material) = match legs {
        None => (None, None),
        Some(capture) => {
            if capture.rows_seen > 0 && capture.rows_all_zero == capture.rows_seen {
                die(format!(
                    "v2-legs execution invalid: all {} captured activation rows are zero — the tap read nothing",
                    capture.rows_seen
                ));
            }
            let LegsCapture { builder, retained, rows_seen, rows_all_zero, .. } = capture;
            let (binding, material) = builder
                .finish_with_material(commitment.full_logits_sequence_root)
                .unwrap_or_else(|e| die(format!("v2-legs execution invalid: {e}")));
            eprintln!("[palw-worker] v2 legs capture: {rows_seen} rows, {rows_all_zero} all-zero");
            let open_material = keep_material.then_some(LegsOpenMaterial { material, rows: retained });
            (Some(binding), open_material)
        }
    };

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
    if let Some(binding) = legs.as_ref() {
        eprintln!(
            "[palw-worker] v2 legs: {} activation leaves ({} taps), {} checkpoints; execution root={}…",
            binding.activation_leaf_count,
            binding.tap_profile.tap_count(),
            binding.checkpoint_count,
            &hex(binding.committed_execution_root)[..16]
        );
    }
    ExecutionV2 {
        projection,
        telemetry: PalwJobTelemetryV2 {
            model_load_ms,
            execute_ms: exec_started.elapsed().as_millis() as u64,
            eog_first_seen_at_decode_index: eog_first,
        },
        legs,
        open_material,
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

// ---------------------------------------------------------------------------------------------
// ADR-0044 (FP-06): the free-prompt v3 job — one execution, an ANSWER and a commitment.
// ---------------------------------------------------------------------------------------------

/// The v3 execution shape. Differs from v2 in exactly the stop semantics — the decode budget is
/// a CEILING and end-of-generation is a real stop (a chat answer that ends, ends) — and in
/// admitting a text arm (the worker tokenizes under the pinned GGUF tokenizer; the template, if
/// any, was the GATEWAY's frozen transform, never this process's). A different stop rule is a
/// different shape profile, never an in-place edit of v2's.
fn shape_string_v3() -> String {
    let gpu_layers = if cfg!(misaka_palw_cpu) { "none" } else { "all" };
    format!(
        "n_ctx={N_CTX}/n_batch={N_BATCH}/n_ubatch={N_BATCH}/n_seq=1/n_threads={N_THREADS}/flash-attn=disabled/gpu-layers={gpu_layers}/greedy-argmax-first-index/text-or-token-ids-input/ceiling-decode/early-eog-stops/prefill-single-batch/v3"
    )
}

/// Rejects a v3 request whose declared runtime identity is not THIS worker — the five pins
/// (`tokenizer_id` is inside the manifest-pinned GGUF; the CU rule is the consensus bundle's,
/// applied by the caller over returned counts — neither is a separate pin here).
fn check_v3_identity(request: &PalwFpWorkerRequestV3, manifest: &PalwRuntimeManifestV2) {
    let checks: [(&str, Hash64, Hash64); 5] = [
        ("model_profile_id", model_profile_id(), request.model_profile_id),
        ("runtime_manifest_hash", manifest.manifest_hash(), request.runtime_manifest_hash),
        ("runtime_class_id", runtime_class_id(), request.runtime_class_id),
        ("shape_profile_id", shape_profile_id_v2(&shape_string_v3()), request.shape_profile_id),
        ("trace_scheme_id", trace_scheme_id_v2(), request.trace_scheme_id),
    ];
    for (field, ours, declared) in checks {
        if ours != declared {
            die(format!(
                "v3-job rejected: {field} mismatch — the request declares a runtime this worker is not (ours {}, request {})",
                hex(ours),
                hex(declared)
            ));
        }
    }
}

/// `--mode v3-manifest`: the machine-readable identity a gateway pins its requests with. JSON on
/// stdout; the canonical identities are the hashes, this document is how a caller learns them.
fn run_v3_manifest() {
    let manifest = runtime_manifest_v2(worker_binary_sha256(), resolve_golden_root());
    let doc = serde_json::json!({
        "schema": "misaka.palw.fp-v3-manifest.v1",
        "runtime_manifest_hash": hex(manifest.manifest_hash()),
        "runtime_class_id": hex(runtime_class_id()),
        "model_profile_id": hex(model_profile_id()),
        "shape_profile_id": hex(shape_profile_id_v2(&shape_string_v3())),
        "trace_scheme_id": hex(trace_scheme_id_v2()),
        "tokenizer_id": hex(tokenizer_id_v2_for_gguf(qwen35_pins::GGUF_SHA256)),
        "n_ctx": N_CTX,
        "prefill_single_batch_cap": N_BATCH,
        "shape_string": shape_string_v3(),
    });
    println!("{doc}");
}

/// `--mode geometry`: the pinned model's real shape, measured (P0-8b).
///
/// A `PalwShapeProfileV3` restates the model's geometry so the step space is self-contained, and
/// every number in it must come from the loaded GGUF rather than from a constant someone typed —
/// a profile that disagrees with the model describes an execution that never ran, and the court
/// would adjudicate steps against it. This is the measurement that feeds a profile, kept as its
/// own mode so building one is never a guess and the numbers can be diffed against the pins.
///
/// It is a DISPLAY document, like `v2-manifest`: nothing here is a consensus identity. What it
/// produces is the input a registration is written from.
fn run_geometry() {
    let model_path = pinned_model_path_v2();
    let ctx = unsafe { shim_open(format!("{}\0", model_path.display()).as_ptr(), N_CTX, N_BATCH, N_THREADS) };
    if ctx.is_null() {
        die(format!("llama.cpp failed to load {}", model_path.display()));
    }
    let measured = |v: i32, what: &str| -> i32 {
        if v <= 0 {
            die(format!("the model reports no {what} ({v}) — a geometry that cannot be measured must not be claimed"));
        }
        v
    };
    let layer_count = measured(unsafe { shim_n_layer(ctx) }, "layer count");
    let hidden_dim = measured(unsafe { shim_n_embd(ctx) }, "hidden dim");
    let attn_heads = measured(unsafe { shim_n_head(ctx) }, "attention head count");
    let attn_kv_heads = measured(unsafe { shim_n_head_kv(ctx) }, "kv head count");
    let attn_head_dim = measured(unsafe { shim_n_embd_head(ctx) }, "head dim");
    let rope_type = unsafe { shim_rope_type(ctx) };
    let rope_freq_scale_train = unsafe { shim_rope_freq_scale_train(ctx) };
    // The whole metadata block, verbatim. Not filtered to "the keys a profile needs": the set of
    // keys an architecture carries is itself a fact about the model, and a filter written from
    // one architecture's expectations is how a GatedDeltaNet constant goes missing silently.
    let meta = {
        let count = unsafe { shim_meta_count(ctx) };
        let mut pairs = serde_json::Map::new();
        let mut key = vec![0u8; 512];
        let mut val = vec![0u8; 4096];
        for i in 0..count.max(0) {
            let klen = unsafe { shim_meta_key_by_index(ctx, i, key.as_mut_ptr(), key.len() as i32) };
            let vlen = unsafe { shim_meta_val_by_index(ctx, i, val.as_mut_ptr(), val.len() as i32) };
            if klen <= 0 || vlen < 0 {
                continue;
            }
            let k = String::from_utf8_lossy(&key[..klen as usize]).to_string();
            let v = String::from_utf8_lossy(&val[..(vlen as usize).min(val.len())]).to_string();
            // Tokenizer tables are megabytes of vocabulary and say nothing about execution shape.
            if k.starts_with("tokenizer.ggml.") && (k.ends_with("tokens") || k.ends_with("scores") || k.ends_with("token_type") || k.ends_with("merges")) {
                pairs.insert(k, serde_json::json!(format!("<{} bytes elided>", vlen)));
                continue;
            }
            pairs.insert(k, serde_json::json!(v));
        }
        pairs
    };
    unsafe { shim_close(ctx) };

    // The pins exist so the tap profile can be chosen BEFORE load; here they are checked against
    // what the model really reports. A mismatch means the artifact is not the pinned one in a way
    // the SHA-256 gate missed, and a profile built on it would be a fiction.
    let pins_agree = layer_count as u32 == qwen35_pins::MODEL_LAYER_COUNT && hidden_dim as u32 == qwen35_pins::MODEL_HIDDEN_DIM;
    let doc = serde_json::json!({
        "schema": "misaka.palw.geometry.v1",
        "layer_count": layer_count,
        "hidden_dim": hidden_dim,
        "attn_heads": attn_heads,
        "attn_kv_heads": attn_kv_heads,
        "attn_head_dim": attn_head_dim,
        "rope_type": rope_type,
        "rope_freq_scale_train": rope_freq_scale_train,
        "gguf_metadata": meta,
        "pinned_layer_count": qwen35_pins::MODEL_LAYER_COUNT,
        "pinned_hidden_dim": qwen35_pins::MODEL_HIDDEN_DIM,
        "pins_agree_with_the_model": pins_agree,
        "note": "display document; the input a PalwShapeProfileV3 registration is written from (P0-8b)",
    });
    println!("{doc}");
    if !pins_agree {
        die("the loaded model's geometry disagrees with the pins — this is not the pinned artifact".into());
    }
}

/// `--mode v3-job`: one framed Borsh [`PalwFpWorkerRequestV3`] on stdin, one framed Borsh
/// [`PalwFpWorkerResultV3`] on stdout, nothing else. Fail-closed exactly as v2: any error path
/// is `die` with NOTHING on stdout.
///
/// The trace is bound to [`fp_job_id_v3`] — a value a panel seat can rebuild from CHAIN data
/// alone (the commitment carries the job and the token ids whole), which is what makes the
/// `TokenIds` replay arm reach byte-identical roots. The v2 trace-layer summary records the
/// EXECUTED count as its exact count with its one stop variant — at the trace layer that reads
/// "this trace is complete at E events", which is true; the run's own stop reason
/// (budget vs end-of-generation) is the v3 result's, checked canonical by every consumer.
fn run_v3_job(trace_out: &Path) {
    let mut stdin = std::io::stdin().lock();
    let payload = read_framed(&mut stdin, PALW_V2_MAX_FRAME_BYTES).unwrap_or_else(|e| die(format!("v3-job rejected: {e}")));
    let request_hash = fp_worker_request_hash_v3(&payload);
    let request: PalwFpWorkerRequestV3 = decode_framed_borsh(&payload).unwrap_or_else(|e| die(format!("v3-job rejected: {e}")));
    if request.version != PALW_FP_V3_VERSION {
        die(format!("v3-job rejected: request version {} is not {}", request.version, PALW_FP_V3_VERSION));
    }
    if request.privacy_mode != PALW_FP_PRIVACY_PUBLIC_DA {
        die(format!("v3-job rejected: privacy mode {} is not PublicDa — a mode the panel cannot replay must not execute", request.privacy_mode));
    }
    if request.decode_token_limit == 0 {
        die("v3-job rejected: a zero decode ceiling is not a job".into());
    }
    if request.max_context_tokens == 0 || request.max_context_tokens > N_CTX as u32 {
        die(format!("v3-job rejected: max_context_tokens {} is outside this runtime's 1..={N_CTX}", request.max_context_tokens));
    }
    let fp = fp_env::probe();
    if !fp.is_canonical() {
        die(format!(
            "v3-job rejected: floating-point environment is not the canonical profile ({}; required {FP_ENVIRONMENT_PROFILE_V2})",
            fp.canonical_string()
        ));
    }
    let manifest = runtime_manifest_v2(worker_binary_sha256(), resolve_golden_root());
    check_v3_identity(&request, &manifest);
    let model_path = pinned_model_path_v2();

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

    // Re-probe AFTER model load, exactly as v2: a backend that flipped MXCSR/FPCR during init
    // would change the arithmetic of every decode call.
    let fp = fp_env::probe();
    if !fp.is_canonical() {
        die(format!("v3-job rejected: floating-point environment drifted after model load: {}", fp.canonical_string()));
    }

    // The two input arms converge on canonical token ids here; everything after this line is one
    // code path, which is what makes text-in and ids-in root equality a testable property.
    let prompt_ids: Vec<u32> = match &request.input {
        PalwFpWorkerInputV3::Text(bytes) => {
            if bytes.is_empty() {
                die("v3-job rejected: the text arm carries no bytes".into());
            }
            if std::str::from_utf8(bytes).is_err() {
                die("v3-job rejected: the text arm is not UTF-8 — a template renders text, not bytes".into());
            }
            let mut out = vec![0i32; N_CTX as usize];
            let n = unsafe { shim_tokenize(ctx, bytes.as_ptr(), bytes.len() as i32, out.as_mut_ptr(), out.len() as i32) };
            if n <= 0 {
                die(format!("v3-job rejected: tokenization failed or produced nothing (rc={n})"));
            }
            out[..n as usize].iter().map(|t| *t as u32).collect()
        }
        PalwFpWorkerInputV3::TokenIds(ids) => {
            if ids.is_empty() {
                die("v3-job rejected: the ids arm carries no tokens".into());
            }
            ids.clone()
        }
    };
    for &t in &prompt_ids {
        if t >= n_vocab {
            die(format!("v3-job rejected: token id {t} is outside the model's vocab ({n_vocab})"));
        }
    }
    let prefill = prompt_ids.len() as u32;
    if prefill > N_BATCH as u32 {
        die(format!("v3-job rejected: prefill {prefill} exceeds the single-batch prefill schedule (n_batch={N_BATCH})"));
    }
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
        tokenizer_id: tokenizer_id_v2_for_gguf(qwen35_pins::GGUF_SHA256),
        prompt_token_ids_hash: prompt_token_ids_hash_v2(&prompt_ids),
        prompt_tokens: prefill,
        decode_token_limit: request.decode_token_limit,
        max_context_tokens: request.max_context_tokens,
        privacy_mode: request.privacy_mode,
    };
    let binding = fp_job_id_v3(&job);

    let exec_started = std::time::Instant::now();
    let limit = request.decode_token_limit;
    let tokens: Vec<i32> = prompt_ids.iter().map(|t| *t as i32).collect();
    let mut logits = vec![0f32; n_vocab as usize];
    let mut scratch: Vec<u8> = Vec::with_capacity(n_vocab as usize * 4);
    let mut events: Vec<Hash64> = Vec::with_capacity(limit as usize);
    let mut schedule = PalwScheduleCommitmentBuilderV2::new(&binding);

    step_v2(ctx, &tokens, PalwTracePhaseV2::Prefill, 0, 0, n_vocab, &binding, &mut logits, &mut scratch, &mut events, &mut schedule);

    let mut outputs: Vec<u32> = Vec::with_capacity(limit as usize);
    let mut rendered: Vec<u8> = Vec::new();
    let mut piece = vec![0u8; 512];
    let stop_reason = loop {
        let tok = argmax(&logits);
        outputs.push(tok as u32);
        let n = unsafe { shim_token_to_piece(ctx, tok, piece.as_mut_ptr(), piece.len() as i32) };
        if n > 0 {
            rendered.extend_from_slice(&piece[..n as usize]);
        }
        // Canonical stop order: the budget edge FIRST (executed == limit is ExactBudgetReached
        // even when the last token is EOG — one executed count, one encoding), then EOG.
        if outputs.len() as u32 == limit {
            break PalwFpStopReasonV3::ExactBudgetReached;
        }
        if unsafe { shim_is_eog(ctx, tok) } == 1 {
            break PalwFpStopReasonV3::EndOfGeneration;
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
            &binding,
            &mut logits,
            &mut scratch,
            &mut events,
            &mut schedule,
        );
    };
    unsafe { shim_close(ctx) };

    let executed = outputs.len() as u32;
    // Defense in depth, as v2: the streamed schedule must equal the canonical schedule for the
    // EXECUTED shape.
    let (schedule_commitment, calls) = schedule.finalize();
    let (expected_schedule, expected_calls) = expected_schedule_commitment_v2(&binding, prefill, executed);
    if schedule_commitment != expected_schedule || calls != expected_calls {
        die("internal error: the executed call schedule diverged from the canonical schedule".into());
    }

    // The trace-layer summary records the EXECUTED count as its exact count (see the fn doc).
    let summary = PalwTraceSummaryV2 {
        vocab_size: n_vocab,
        logits_dtype: PalwLogitsDtypeV2::F32Le,
        declared_prefill_tokens: prefill,
        exact_decode_tokens: executed,
        event_count: executed,
        first_event_kind: PalwTracePhaseV2::Prefill,
        last_event_kind: if executed == 1 { PalwTracePhaseV2::Prefill } else { PalwTracePhaseV2::Decode },
        output_token_ids_hash: output_token_ids_hash_v2(&outputs),
        stop_reason: PalwStopReasonV2::ExactBudgetReached,
    };
    let event_merkle = trace_event_merkle_root_v2(&events).unwrap_or_else(|e| die(format!("internal error: trace merkle failed: {e}")));
    let trace_root = full_logits_trace_root_v2(&binding, &summary, &event_merkle);

    // Retained-trace DA (ADR-0044 Decision 3's obligation trio, made honest): the ordered
    // event-hash list, chunked to disk BEFORE the result frame exists — a commitment whose
    // producer kept nothing cannot serve an opening and would default in court, so failing to
    // retain is failing the job. Layout: <trace_out>/<job-id-hex>/chunk-<k>.bin (raw 64-byte
    // event hashes) + manifest.json (digests, for the serving layer's own bookkeeping).
    let (trace_manifest_root, trace_chunk_count, chunk_digests) = fp_trace_manifest_v3(binding, &events);
    let retain_dir = trace_out.join(hex(binding));
    std::fs::create_dir_all(&retain_dir).unwrap_or_else(|e| die(format!("cannot create the retention dir {}: {e}", retain_dir.display())));
    for (index, chunk) in events.chunks(kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_TRACE_CHUNK_EVENTS_V3 as usize).enumerate() {
        let mut bytes = Vec::with_capacity(chunk.len() * 64);
        for event in chunk {
            bytes.extend_from_slice(event.as_byte_slice());
        }
        let path = retain_dir.join(format!("chunk-{index}.bin"));
        std::fs::write(&path, &bytes).unwrap_or_else(|e| die(format!("cannot retain {}: {e}", path.display())));
    }
    let manifest_doc = serde_json::json!({
        "schema": "misaka.palw.fp-v3-trace-manifest.v1",
        "trace_binding": hex(binding),
        "trace_root": hex(trace_root),
        "chunk_events": kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_TRACE_CHUNK_EVENTS_V3,
        "chunk_count": trace_chunk_count,
        "chunk_digests": chunk_digests.iter().map(|d| hex(*d)).collect::<Vec<_>>(),
        "manifest_root": hex(trace_manifest_root),
    });
    std::fs::write(retain_dir.join("manifest.json"), serde_json::to_vec_pretty(&manifest_doc).unwrap())
        .unwrap_or_else(|e| die(format!("cannot write the retention manifest: {e}")));

    eprintln!(
        "[palw-worker] v3 executed: prefill={prefill} decode={executed}/{limit} stop={stop_reason:?} in {:?}; root={}…",
        exec_started.elapsed(),
        &hex(trace_root)[..16]
    );

    let result = PalwFpWorkerResultV3 {
        version: PALW_FP_V3_VERSION,
        request_hash,
        job,
        prompt_token_ids: prompt_ids,
        trace_root,
        output_root: output_commitment_v2(&binding, &outputs, &rendered_output_hash_v2(&rendered)),
        schedule_root: schedule_commitment,
        // **Deliberately null, and consensus refuses it — but the reason is NOT the one this
        // comment used to give, and the correction matters.**
        //
        // It said the v2-legs path "already produces the binding this field wants". It does not.
        // The court binds a refutation to `PalwStepBindingV2::committed_execution_root`, which
        // `execution_commitment_root_v2` builds from FOUR roots: the logits trace, the activation
        // leg, a **v2** checkpoint leg (keyed differently from the v1 one and carrying a
        // `state_chunk_map_id`), and a **step leg**. The v2-legs path produces the v1 composite —
        // `execution_commitment_root_v1`, no step leg — so its root is a different value for a
        // different purpose.
        //
        // **No worker path on this tree captures a step leg at all.** `PalwStepLegBuilderV1`
        // wants one leaf per (call, node slot, position, tile) of every kernel invocation, and
        // the shim exposes taps and logits, not per-kernel tile outputs. That is instrumentation
        // this runtime does not have, and it is the same gap on BOTH lanes: an attempt envelope's
        // `execution_root` is a value the miner supplies, bound into `attempt_id` and therefore
        // into the PoW, but recomputed from a real execution by nothing.
        //
        // So the free-prompt lane's fail-closed refusal is not this lane being behind the attempt
        // lane — it is the shared gap surfacing where a rule actually checks for it. A fabricated
        // value would be strictly worse than none: it would make disputes fail in a way that looks
        // like the producer winning them.
        //
        // What IS ready: `palw_fp_execution_v3` derives the context and the root from measured
        // facts, so the day the step leg is captured this becomes
        // `palw_fp_execution_root_v3(&ctx, &facts)` and nothing else moves. What is missing is the
        // capture, in the shim.
        execution_root: Hash64::default(),
        trace_manifest_root,
        trace_chunk_count,
        trace_event_count: executed,
        decode_tokens_executed: executed,
        stop_reason,
        output_token_ids: outputs,
        rendered,
        model_load_ms,
        execute_ms: exec_started.elapsed().as_millis() as u64,
    };
    let bytes = borsh::to_vec(&result).unwrap_or_else(|e| die(format!("cannot serialize the v3 result: {e}")));
    let mut stdout = std::io::stdout().lock();
    write_framed(&mut stdout, &bytes).unwrap_or_else(|e| die(format!("cannot write the v3 result frame: {e}")));
}

/// `--mode v2-legs-job`: the v2 job with execution-commitment legs. Same envelope in; a
/// `PalwLegsJobResultV1` frame out, carrying the v2 result **unchanged** plus the binding.
fn run_v2_legs_job() {
    let mut stdin = std::io::stdin().lock();
    let payload = read_framed(&mut stdin, PALW_V2_MAX_FRAME_BYTES).unwrap_or_else(|e| die(format!("v2-legs-job rejected: {e}")));
    let request_hash = job_request_hash_v2(&payload);
    let envelope: PalwJobEnvelopeV2 = decode_framed_borsh(&payload).unwrap_or_else(|e| die(format!("v2-legs-job rejected: {e}")));
    envelope.validate_shape(N_CTX as u32).unwrap_or_else(|e| die(format!("v2-legs-job rejected: {e}")));
    if envelope.deadline_unix_ms != 0 && now_unix_ms() >= envelope.deadline_unix_ms {
        die("v2-legs-job rejected: deadline already passed at admission".into());
    }
    let fp = fp_env::probe();
    if !fp.is_canonical() {
        die(format!(
            "v2-legs-job rejected: floating-point environment is not the canonical profile ({}; required {FP_ENVIRONMENT_PROFILE_V2})",
            fp.canonical_string()
        ));
    }
    let manifest = runtime_manifest_v2(worker_binary_sha256(), resolve_golden_root());
    check_v2_identity(&envelope, &manifest);
    let model_path = pinned_model_path_v2();
    let exec = execute_v2_legs(&model_path, &envelope);
    let binding = exec.legs.unwrap_or_else(|| die("internal error: the legs path produced no binding".into()));
    let result = PalwLegsJobResultV1 {
        version: PALW_LEGS_OBJECT_VERSION_V1,
        result: PalwJobResultV2 {
            version: PALW_JOB_WIRE_VERSION_V2,
            request_hash,
            job_id: envelope.job_id,
            projection: exec.projection,
            telemetry: exec.telemetry,
        },
        binding,
    };
    // The driver will check this; failing it here means the bug is ours, and a receipt whose two
    // halves describe different executions must not leave the process.
    result.validate_coherence().unwrap_or_else(|e| die(format!("internal error: {e}")));
    let bytes = borsh::to_vec(&result).unwrap_or_else(|e| die(format!("cannot serialize the v2 legs result: {e}")));
    let mut stdout = std::io::stdout().lock();
    write_framed(&mut stdout, &bytes).unwrap_or_else(|e| die(format!("cannot write the v2 legs result frame: {e}")));
}

/// Builds the answer from retained material, in request order. Every miss here is an internal
/// bug — the request was validated before execution and the material comes from the very
/// execution that produced the binding — so failures die rather than degrade.
fn compose_opening_answer(
    binding: &PalwLegsBindingV1,
    open: &LegsOpenMaterial,
    request: &PalwLegsOpeningRequestV1,
) -> PalwLegsOpeningAnswerV1 {
    let taps = binding.tap_profile.tap_count();
    let activation = request
        .activation
        .iter()
        .map(|coordinate| {
            let index = canonical_activation_leaf_index(
                &binding.job_context,
                taps,
                coordinate.call_index,
                coordinate.tap_slot,
                coordinate.position,
            )
            .unwrap_or_else(|| die("internal error: a validated coordinate stopped being canonical".into()));
            let opening = leg_opening_v1(
                PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_LEAF,
                PALW_LEGS_DOMAIN_ACTIVATION_MERKLE_NODE,
                &open.material.activation_leaf_hashes,
                index as u32,
                PALW_LEGS_MAX_ACTIVATION_LEAVES,
            )
            .unwrap_or_else(|e| die(format!("internal error: cannot open activation leaf {index}: {e}")));
            let row = open
                .rows
                .iter()
                .find(|(coords, _)| *coords == (coordinate.call_index, coordinate.tap_slot, coordinate.position))
                .unwrap_or_else(|| die("internal error: a requested row was not retained during capture".into()));
            PalwOpenedActivationLeafV1 {
                opening,
                preimage: PalwActivationLeafV1 {
                    call_index: coordinate.call_index,
                    tap_slot: coordinate.tap_slot,
                    position: coordinate.position,
                    hidden_dim: binding.tap_profile.hidden_dim,
                    value_count: row.1.len() as u32,
                    values_le_bytes: row.1.iter().flat_map(|v| v.to_le_bytes()).collect(),
                },
            }
        })
        .collect();
    let checkpoints = request
        .checkpoint_indices
        .iter()
        .map(|index| {
            let opening = leg_opening_v1(
                PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_LEAF,
                PALW_LEGS_DOMAIN_CHECKPOINT_MERKLE_NODE,
                &open.material.checkpoint_leaf_hashes,
                *index,
                PALW_LEGS_MAX_CHECKPOINTS,
            )
            .unwrap_or_else(|e| die(format!("internal error: cannot open checkpoint {index}: {e}")));
            PalwOpenedCheckpointLeafV1 { opening, preimage: open.material.checkpoint_leaves[*index as usize].clone() }
        })
        .collect();
    PalwLegsOpeningAnswerV1 { version: PALW_LEGS_OBJECT_VERSION_V1, binding: binding.clone(), activation, checkpoints }
}

/// `--mode v2-legs-open`: answer an opening call about a commitment THIS runtime can
/// reproduce. stdin carries one framed `PalwLegsOpeningCallV1` (the job envelope and the
/// request are one message — the v2 frame contract is one frame per stream, and a request
/// without its job is not answerable anyway); stdout carries one framed
/// `PalwLegsOpeningAnswerV1`.
///
/// The worker re-executes the job with capture and compares the recomputed committed root to
/// the requested one. On mismatch it dies with nothing on stdout: an honest answerer never
/// fabricates openings for a tree it cannot reproduce. That refusal is the security property —
/// within a class, ANY member can answer for any honest commitment (re-execution reproduces the
/// tree bit for bit), and nobody can answer for a fraudulent one.
fn run_v2_legs_open() {
    let mut stdin = std::io::stdin().lock();
    let payload = read_framed(&mut stdin, PALW_V2_MAX_FRAME_BYTES).unwrap_or_else(|e| die(format!("v2-legs-open rejected: {e}")));
    let call: PalwLegsOpeningCallV1 = decode_framed_borsh(&payload).unwrap_or_else(|e| die(format!("v2-legs-open rejected: {e}")));
    if call.version != PALW_LEGS_OBJECT_VERSION_V1 {
        die(format!("v2-legs-open rejected: unsupported call version {}", call.version));
    }
    let envelope = call.envelope;
    let request = call.request;
    envelope.validate_shape(N_CTX as u32).unwrap_or_else(|e| die(format!("v2-legs-open rejected: {e}")));
    let fp = fp_env::probe();
    if !fp.is_canonical() {
        die(format!(
            "v2-legs-open rejected: floating-point environment is not the canonical profile ({}; required {FP_ENVIRONMENT_PROFILE_V2})",
            fp.canonical_string()
        ));
    }
    let manifest = runtime_manifest_v2(worker_binary_sha256(), resolve_golden_root());
    check_v2_identity(&envelope, &manifest);

    // The request is validated against THIS worker's profiles before a model load is spent on
    // it — the same shape gate the answer checker applies afterwards.
    let context = PalwJobContextV2::from_envelope(&envelope, tokenizer_id_v2_for_gguf(qwen35_pins::GGUF_SHA256));
    let taps = canonical_tap_layers(qwen35_pins::MODEL_LAYER_COUNT).len() as u32;
    let expected_checkpoints = canonical_checkpoint_count(&context, CHECKPOINT_INTERVAL_V1);
    check_opening_request_shape(&request, &context, taps, expected_checkpoints)
        .unwrap_or_else(|e| die(format!("v2-legs-open rejected: {e}")));

    let wanted: HashSet<(u32, u32, u32)> =
        request.activation.iter().map(|c| (c.call_index, c.tap_slot, c.position)).collect();
    let model_path = pinned_model_path_v2();
    let exec = execute_v2_legs_open(&model_path, &envelope, wanted);
    let binding = exec.legs.unwrap_or_else(|| die("internal error: the open path produced no binding".into()));
    let open = exec.open_material.unwrap_or_else(|| die("internal error: the open path retained no material".into()));
    if binding.committed_execution_root != request.committed_execution_root {
        die(format!(
            "v2-legs-open refused: this runtime reproduces execution root {}, not the requested {} — refusing to open a commitment that is not this execution",
            hex(binding.committed_execution_root),
            hex(request.committed_execution_root)
        ));
    }
    let answer = compose_opening_answer(&binding, &open, &request);
    check_legs_opening_answer_v1(&request, &answer)
        .unwrap_or_else(|e| die(format!("internal error: the composed answer failed its own check: {e}")));
    let bytes = borsh::to_vec(&answer).unwrap_or_else(|e| die(format!("cannot serialize the opening answer: {e}")));
    write_framed(&mut std::io::stdout().lock(), &bytes).unwrap_or_else(|e| die(format!("cannot write the answer frame: {e}")));
    eprintln!(
        "[palw-worker] v2-legs-open: answered {} activation + {} checkpoint opening(s) for root {}…",
        answer.activation.len(),
        answer.checkpoints.len(),
        &hex(request.committed_execution_root)[..16]
    );
}

/// Reads one complete frame from a file — the file IS the stream, so the one-frame-then-EOF
/// contract applies to it exactly as to a pipe.
fn read_frame_file(path: &str, what: &str) -> Vec<u8> {
    let mut file = std::fs::File::open(path).unwrap_or_else(|e| die(format!("cannot open the {what} frame at {path}: {e}")));
    read_framed(&mut file, PALW_V2_MAX_FRAME_BYTES).unwrap_or_else(|e| die(format!("{what} frame at {path} rejected: {e}")))
}

/// `--mode v2-legs-open-request --envelope <path>`: harness plumbing. Reads a
/// `PalwLegsJobResultV1` frame on stdin (what `v2-legs-job` wrote) plus the job's envelope
/// frame, and emits the complete `PalwLegsOpeningCallV1`: a deterministic request for the
/// first, middle and last activation leaves plus every committed checkpoint (capped). A real
/// challenge protocol SAMPLES leaf indices from bound randomness — that protocol is future-ADR
/// work; this mode exists so the E2E harness can exercise commit → open → verify without
/// pretending to be it. Model-free.
fn run_v2_legs_open_request(envelope_path: &str) {
    let envelope_payload = read_frame_file(envelope_path, "envelope");
    let envelope: PalwJobEnvelopeV2 =
        decode_framed_borsh(&envelope_payload).unwrap_or_else(|e| die(format!("v2-legs-open-request rejected: {e}")));
    let mut stdin = std::io::stdin().lock();
    let payload =
        read_framed(&mut stdin, PALW_V2_MAX_FRAME_BYTES).unwrap_or_else(|e| die(format!("v2-legs-open-request rejected: {e}")));
    let result: PalwLegsJobResultV1 =
        decode_framed_borsh(&payload).unwrap_or_else(|e| die(format!("v2-legs-open-request rejected: {e}")));
    result.validate_coherence().unwrap_or_else(|e| die(format!("v2-legs-open-request rejected: {e}")));
    let binding = &result.binding;
    let taps = binding.tap_profile.tap_count();
    let count = binding.activation_leaf_count as u64;
    let mut indices = vec![0, count / 2, count.saturating_sub(1)];
    indices.sort_unstable();
    indices.dedup();
    let activation = indices
        .iter()
        .map(|index| {
            canonical_activation_leaf_coordinates(&binding.job_context, taps, *index)
                .unwrap_or_else(|| die(format!("v2-legs-open-request rejected: leaf {index} has no canonical coordinates")))
        })
        .collect();
    let call = PalwLegsOpeningCallV1 {
        version: PALW_LEGS_OBJECT_VERSION_V1,
        envelope,
        request: PalwLegsOpeningRequestV1 {
            version: PALW_LEGS_OBJECT_VERSION_V1,
            committed_execution_root: binding.committed_execution_root,
            activation,
            checkpoint_indices: (0..binding.checkpoint_count.min(8)).collect(),
        },
    };
    let bytes = borsh::to_vec(&call).unwrap_or_else(|e| die(format!("cannot serialize the opening call: {e}")));
    write_framed(&mut std::io::stdout().lock(), &bytes).unwrap_or_else(|e| die(format!("cannot write the call frame: {e}")));
}

/// `--mode v2-legs-open-verify --call <path>`: reads the call frame (whose request names what
/// was asked) and the answer frame on stdin, and adjudicates them. Needs no model, no goldens,
/// and no environment — answering costs a re-execution, checking an answer is pure
/// recomputation, and keeping that second half free of the runtime is the point of the seam.
fn run_v2_legs_open_verify(call_path: &str) {
    let call_payload = read_frame_file(call_path, "call");
    let call: PalwLegsOpeningCallV1 =
        decode_framed_borsh(&call_payload).unwrap_or_else(|e| die(format!("v2-legs-open-verify rejected: {e}")));
    let request = call.request;
    let mut stdin = std::io::stdin().lock();
    let answer_payload =
        read_framed(&mut stdin, PALW_V2_MAX_FRAME_BYTES).unwrap_or_else(|e| die(format!("v2-legs-open-verify rejected: {e}")));
    let answer: PalwLegsOpeningAnswerV1 =
        decode_framed_borsh(&answer_payload).unwrap_or_else(|e| die(format!("v2-legs-open-verify rejected: {e}")));
    match check_legs_opening_answer_v1(&request, &answer) {
        Ok(()) => {
            let doc = serde_json::json!({
                "schema": "misaka.palw.v2-legs-open-verify.v1",
                "answer_valid": true,
                "committed_execution_root": hex(request.committed_execution_root),
                "activation_openings": answer.activation.len(),
                "checkpoint_openings": answer.checkpoints.len(),
            });
            println!("{}", serde_json::to_string_pretty(&doc).expect("serializable"));
        }
        Err(e) => {
            let doc = serde_json::json!({
                "schema": "misaka.palw.v2-legs-open-verify.v1",
                "answer_valid": false,
                "error": e.to_string(),
            });
            println!("{}", serde_json::to_string_pretty(&doc).expect("serializable"));
            std::process::exit(1);
        }
    }
}

/// `--mode v2-replay-bench --name <golden> [--runs N] [--decode D] [--legs]`: measures the
/// cold replay cost this class would register as `p99_cold_replay` (ADR-0028 §3) and answers
/// the fit check `κ · p99 ≤ w_replay` against both networks' Stage-1 window defaults.
///
/// Each run is a fresh model load in this process — **cold process, warm OS page cache**; the
/// number a class REGISTERS must come from the fleet's own runs of this mode, not from a dev
/// machine. `--decode D` overrides the golden's decode budget so the measurement can be taken
/// at the credited ceiling (the golden set's jobs are far below it); percentiles are the
/// shared nearest-rank convention from `palw_schedule`, so this tool and the shadow ledger
/// can never disagree about what "p99" means. Roots must be identical across runs — a bench
/// that observed a drifting root has measured a broken class, and exits non-zero saying so.
fn run_v2_replay_bench(name: &str, runs: u32, decode_override: Option<u32>, legs: bool) {
    if runs == 0 {
        die("--runs must be at least 1".into());
    }
    let path = std::env::var(PALW_GOLDEN_ENV)
        .unwrap_or_else(|_| die(format!("{PALW_GOLDEN_ENV} is not set; v2-replay-bench needs the golden set")));
    let set = load_golden_set(&path);
    let job = set
        .jobs
        .iter()
        .find(|job| job.name == name)
        .unwrap_or_else(|| die(format!("no golden job named {name:?} in this set")));
    let mut envelope = set.envelope_for(job);
    if let Some(decode) = decode_override {
        envelope.exact_decode_tokens = decode;
        envelope.max_context_tokens = N_CTX as u32;
        envelope.validate_shape(N_CTX as u32).unwrap_or_else(|e| die(format!("--decode {decode} is not a valid budget: {e}")));
    }
    let fp = fp_env::probe();
    if !fp.is_canonical() {
        die(format!("v2-replay-bench refused: non-canonical floating-point environment: {}", fp.canonical_string()));
    }
    let model_path = pinned_model_path_v2();

    let mut load_ms: Vec<u64> = Vec::with_capacity(runs as usize);
    let mut execute_ms: Vec<u64> = Vec::with_capacity(runs as usize);
    let mut total_ms: Vec<u64> = Vec::with_capacity(runs as usize);
    let mut first_root: Option<Hash64> = None;
    let mut roots_identical = true;
    for run in 0..runs {
        let exec = if legs { execute_v2_legs(&model_path, &envelope) } else { execute_v2(&model_path, &envelope) };
        let total = exec.telemetry.model_load_ms + exec.telemetry.execute_ms;
        eprintln!(
            "[palw-worker] v2-replay-bench run {}/{runs}: load {} ms + execute {} ms = {total} ms",
            run + 1,
            exec.telemetry.model_load_ms,
            exec.telemetry.execute_ms
        );
        load_ms.push(exec.telemetry.model_load_ms);
        execute_ms.push(exec.telemetry.execute_ms);
        total_ms.push(total);
        let root = exec.projection.full_logits_trace_root;
        match first_root {
            None => first_root = Some(root),
            Some(expected) if expected != root => roots_identical = false,
            Some(_) => {}
        }
    }
    load_ms.sort_unstable();
    execute_ms.sort_unstable();
    total_ms.sort_unstable();
    let pct = |sorted: &[u64]| {
        serde_json::json!({
            "p50": nearest_rank_percentile(sorted, 50, 100),
            "p95": nearest_rank_percentile(sorted, 95, 100),
            "p99": nearest_rank_percentile(sorted, 99, 100),
            "max": sorted.last(),
        })
    };
    let p99_total = nearest_rank_percentile(&total_ms, 99, 100).expect("runs ≥ 1");
    let doc = serde_json::json!({
        "schema": "misaka.palw.v2-replay-bench.v1",
        "runtime_class_id": hex(runtime_class_id()),
        "job": job.name,
        "prefill_tokens": envelope.declared_prefill_tokens(),
        "exact_decode_tokens": envelope.exact_decode_tokens,
        "legs_path": legs,
        "runs": runs,
        "methodology": "cold process (fresh model load per run), warm OS page cache; register fleet-measured numbers only",
        "model_load_ms": pct(&load_ms),
        "execute_ms": pct(&execute_ms),
        "total_ms": pct(&total_ms),
        "kappa": PALW_SCHEDULE_REPLAY_KAPPA,
        "kappa_p99_ms": PALW_SCHEDULE_REPLAY_KAPPA * p99_total,
        "fits_w_replay": {
            "deci_bps": replay_p99_fits_v1(p99_total, &PalwScheduleParamsV1::stage1_defaults_deci_bps(), 10_000),
            "two_minute_bps": replay_p99_fits_v1(p99_total, &PalwScheduleParamsV1::stage1_defaults_two_minute_bps(), 120_000),
        },
        "roots_identical_across_runs": roots_identical,
        // The pairwise-class evidence: two hosts of one class benching the same envelope must
        // print the same root, and printing it is what lets a fleet check that cheaply.
        "logits_root": first_root.map(hex),
    });
    println!("{}", serde_json::to_string_pretty(&doc).expect("serializable"));
    if !roots_identical {
        die("v2-replay-bench FAILED: the logits root drifted across runs — this class is broken, not slow".into());
    }
}

/// `--mode v2-golden-envelope --name <job>`: writes the named golden job's envelope as one
/// framed Borsh message on stdout — exactly the frame `v2-job`, `v2-legs-job` and
/// `v2-legs-open` read first. Harness plumbing (needs the golden set, not the model), so
/// drivers get a canonical envelope without reimplementing the framing.
///
/// One field differs from the selftest's in-process envelope: the golden set deliberately
/// carries a SENTINEL manifest hash (goldens survive worker rebuilds), but the job admission
/// gate rightly refuses an envelope that does not declare this exact binary — so the sentinel
/// is replaced with this worker's real manifest hash. The envelope declares the runtime it is
/// about to drive, which is the truth.
fn run_v2_golden_envelope(name: &str) {
    let path = std::env::var(PALW_GOLDEN_ENV)
        .unwrap_or_else(|_| die(format!("{PALW_GOLDEN_ENV} is not set; v2-golden-envelope needs the registered golden set")));
    let set = load_golden_set(&path);
    let job = set
        .jobs
        .iter()
        .find(|job| job.name == name)
        .unwrap_or_else(|| die(format!("no golden job named {name:?} in this set")));
    let mut envelope = set.envelope_for(job);
    envelope.runtime_manifest_hash = runtime_manifest_v2(worker_binary_sha256(), resolve_golden_root()).manifest_hash();
    let bytes = borsh::to_vec(&envelope).unwrap_or_else(|e| die(format!("cannot serialize the golden envelope: {e}")));
    write_framed(&mut std::io::stdout().lock(), &bytes).unwrap_or_else(|e| die(format!("cannot write the envelope frame: {e}")));
}

/// `--mode v2-legs-selftest`: **the gate this whole increment stands on.**
///
/// Installing a capture callback makes ggml compute each graph split in sub-ranges cut at every
/// tapped node instead of one whole-split compute. Whether that changes the arithmetic is a
/// property of the backend's fusion and scheduling, not something anyone may assume — and it
/// matters totally, because the legs bind *the frozen v2 logits root*: if capture moves that
/// root, a capturing executor and a non-capturing verifier disagree about an honest execution.
///
/// So this replays the golden set with capture ON and demands the SAME roots the goldens hold.
/// A mismatch is not a bug to paper over — it means capture is a different determinism class on
/// this backend, and the honest outcomes are to say so and register it as one.
fn run_v2_legs_selftest() {
    let path = std::env::var(PALW_GOLDEN_ENV)
        .unwrap_or_else(|_| die(format!("{PALW_GOLDEN_ENV} is not set; v2-legs-selftest needs the registered golden set")));
    let set = load_golden_set(&path);
    let fp = fp_env::probe();
    if !fp.is_canonical() {
        die(format!("v2-legs-selftest FAILED before execution: non-canonical floating-point environment: {}", fp.canonical_string()));
    }
    let model_path = pinned_model_path_v2();
    let mut drift = 0usize;
    let mut results = Vec::new();
    for job in &set.jobs {
        let envelope = set.envelope_for(job);
        // The open path is the commit path plus Rust-side retention — running the selftest
        // through it measures the same computation AND leaves the material to prove, per job,
        // that the commitment is answerable.
        let context = PalwJobContextV2::from_envelope(&envelope, tokenizer_id_v2_for_gguf(qwen35_pins::GGUF_SHA256));
        let taps = canonical_tap_layers(qwen35_pins::MODEL_LAYER_COUNT).len() as u32;
        let expected_leaves = canonical_activation_leaf_count(&context, taps);
        let mut probe_indices = vec![0, expected_leaves / 2, expected_leaves.saturating_sub(1)];
        probe_indices.sort_unstable();
        probe_indices.dedup();
        let probe_coordinates: Vec<PalwActivationCoordinateV1> = probe_indices
            .iter()
            .map(|index| {
                canonical_activation_leaf_coordinates(&context, taps, *index)
                    .unwrap_or_else(|| die(format!("internal error: probe leaf {index} has no canonical coordinates")))
            })
            .collect();
        let wanted: HashSet<(u32, u32, u32)> =
            probe_coordinates.iter().map(|c| (c.call_index, c.tap_slot, c.position)).collect();
        let exec = execute_v2_legs_open(&model_path, &envelope, wanted);
        let binding = exec.legs.unwrap_or_else(|| die("internal error: the legs path produced no binding".into()));
        let open = exec.open_material.unwrap_or_else(|| die("internal error: the open path retained no material".into()));
        let got = exec.projection.full_logits_trace_root;
        let matches = got == job.expected.full_logits_trace_root;
        if !matches {
            drift += 1;
            eprintln!(
                "[palw-worker] v2-legs-selftest DRIFT: {} — capture-off root {}, capture-on root {}",
                job.name,
                hex(job.expected.full_logits_trace_root),
                hex(got)
            );
        } else {
            eprintln!(
                "[palw-worker] v2-legs-selftest ok: {} (logits root unmoved; execution root {}…)",
                job.name,
                &hex(binding.committed_execution_root)[..16]
            );
        }
        // Openability: the commitment just made must answer a request about itself, and the
        // answer must pass the model-free checker. A commitment that cannot be opened is
        // unanswerable evidence — as much a failure as a moved root.
        let request = PalwLegsOpeningRequestV1 {
            version: PALW_LEGS_OBJECT_VERSION_V1,
            committed_execution_root: binding.committed_execution_root,
            activation: probe_coordinates,
            checkpoint_indices: (0..binding.checkpoint_count.min(8)).collect(),
        };
        let answer = compose_opening_answer(&binding, &open, &request);
        check_legs_opening_answer_v1(&request, &answer)
            .unwrap_or_else(|e| die(format!("v2-legs-selftest FAILED: {} is not openable: {e}", job.name)));
        results.push(serde_json::json!({
            "name": job.name,
            "logits_root_unmoved": matches,
            "logits_root": hex(got),
            "execution_commitment_root": hex(binding.committed_execution_root),
            "activation_leaf_count": binding.activation_leaf_count,
            "checkpoint_count": binding.checkpoint_count,
            "openings_verified": { "activation": answer.activation.len(), "checkpoint": answer.checkpoints.len() },
        }));
    }
    let doc = serde_json::json!({
        "schema": "misaka.palw.v2-legs-selftest.v1",
        "runtime_class_id": hex(runtime_class_id()),
        "execution_commitment_scheme_id": hex(execution_commitment_scheme_id_v1()),
        "tap_semantics": tap_semantics_string(),
        "tap_semantics_id": hex(tap_semantics_id_v1(&tap_semantics_string())),
        "state_layout": state_layout_string(),
        "state_layout_id": hex(state_layout_id_v1(&state_layout_string())),
        "tap_layers": canonical_tap_layers(qwen35_pins::MODEL_LAYER_COUNT),
        "checkpoint_interval": CHECKPOINT_INTERVAL_V1,
        "jobs": results,
        "capture_is_logits_neutral": drift == 0,
    });
    println!("{}", serde_json::to_string_pretty(&doc).expect("serializable"));
    if drift > 0 {
        die(format!(
            "{drift} of {} golden jobs moved their logits root under capture — capture is NOT logits-neutral on this backend",
            set.jobs.len()
        ));
    }
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
        // The DISPLAY document reports the flags that actually decided this binary's kernels.
        //
        // Measured 2026-08-20 on an M4 Pro: the cache really contains `GGML_AVX2:BOOL=ON` on an
        // arm64 build, because those options are llama.cpp DEFAULTS that its arm64 kernel
        // selection never consults. Printing them made this document claim AVX2 for a binary
        // full of NEON — exactly the "measured from the real build, never declared" promise the
        // build script makes, broken by CMake's defaults rather than by anyone's intent.
        //
        // The x86 word is emitted only on x86; on aarch64 the honest evidence is
        // `host_cpu_features` (`neon=1,dotprod=1`), measured from the host. The
        // arch-independent flags print everywhere because they mean the same thing everywhere.
        //
        // `runtime_manifest_hash_v2` is UNCHANGED: it still covers the full cache-derived set, so
        // no consensus identity moves. This is a truthfulness fix to the human-facing document,
        // and keeping the two apart is deliberate — a display that lies is a bug, and a
        // fingerprint that changes is a fork.
        "ggml_flags": {
            "native": manifest.ggml_native,
            "openmp": manifest.ggml_openmp,
            "blas": manifest.ggml_blas,
            "accelerate": manifest.ggml_accelerate,
            "cpu_all_variants": manifest.ggml_cpu_all_variants,
            "x86_isa": if cfg!(target_arch = "x86_64") {
                serde_json::json!({
                    "sse42": manifest.ggml_sse42,
                    "avx": manifest.ggml_avx,
                    "avx2": manifest.ggml_avx2,
                    "fma": manifest.ggml_fma,
                    "f16c": manifest.ggml_f16c,
                })
            } else {
                // Not "false" — absent. Reporting `avx2: false` on arm64 would be a second
                // untrue claim about a flag nothing read.
                serde_json::json!("not applicable on this architecture; see host_cpu_features")
            },
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
    let exec = execute_on_context(ctx, input, n_predict, started);
    // This function OWNS the context it opened, so it closes it. `execute_on_context` deliberately
    // does not: a caller that reuses one context across jobs (the resident agent) must be the one
    // that decides when it dies. Moving the close out here is not cosmetic — leaving it inside the
    // shared body made every job free its caller's context, and the next job then read a freed
    // `shim_ctx`. That use-after-free is what looked like heap corruption for three rounds of
    // diagnosis.
    unsafe { shim_close(ctx) };
    exec
}

/// The job, on a context the CALLER owns and must close.
///
/// Split out of [`execute`] so the resident agent can run many jobs against one loaded model
/// (ADR-0041 Decision 1'). Nothing about the computation changes, which is the point: a tag from the
/// agent and a tag from a one-shot process must be the same tag.
fn execute_on_context(ctx: *mut ShimCtx, input: &[u8], n_predict: u32, started: std::time::Instant) -> Execution {
    let n_vocab = unsafe { shim_n_vocab(ctx) };

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

/// The replay-stable projection document, from an execution and the request that produced it.
///
/// Shared by the one-shot modes and `--mode agent` so there is exactly ONE construction of this
/// document in the worker. Job identity mixes the input, the ceiling and the pinned identities —
/// never the mode, never the transport, and never anything drawn at run time — so an executor,
/// its verifiers, and a resident agent land on identical documents byte for byte.
fn projection_doc(input: &[u8], n_predict: u32, exec: &Execution) -> serde_json::Value {
    let prompt_digest = keyed64(b"misaka-palw-lite/input/v1", &[&(input.len() as u64).to_le_bytes(), input]);
    let job_nullifier = keyed64(
        b"misaka-palw-lite/nullifier/v1",
        &[
            prompt_digest.as_byte_slice(),
            &n_predict.to_le_bytes(),
            model_profile_id().as_byte_slice(),
            runtime_manifest_hash().as_byte_slice(),
        ],
    );
    let request_commitment =
        keyed64(b"misaka-palw-lite/request/v1", &[prompt_digest.as_byte_slice(), &n_predict.to_le_bytes(), SHAPE_STRING.as_bytes()]);
    serde_json::json!({
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
    })
}

/// Marker every agent → driver line carries.
///
/// The agent protocol is newline-delimited JSON rather than the v2 length-prefixed frames for one
/// reason: llama.cpp and ggml are third-party code on this process's stdout, and a single stray
/// byte desynchronises a length-prefixed stream irrecoverably and SILENTLY. A marker makes noise
/// skippable instead of fatal, and the driver skips anything without it.
const AGENT_MARKER: &str = "@palw-pow1 ";
const AGENT_SCHEMA: &str = "misaka.palw.pow-agent.v1";

fn emit_agent_frame(v: &serde_json::Value) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    // One write, then flush: the driver blocks on a line, so a frame left in the buffer stalls it
    // until the NEXT job's output pushes it out — a deadlock that only appears under load.
    writeln!(out, "{AGENT_MARKER}{v}").unwrap_or_else(|e| die(format!("pow-agent: cannot write a frame: {e}")));
    out.flush().unwrap_or_else(|e| die(format!("pow-agent: cannot flush a frame: {e}")));
}

/// `--mode pow-agent`: one model load, many Layer-1 PoW jobs (ADR-0041 Decision 1′).
///
/// NOT the `palw-agent` crate, despite the word. That one is the VPS supervisor for v2 COMPUTE
/// jobs — a Unix socket, Borsh frames, admission control, and one worker process per job. This is
/// the opposite trade on a different path: no supervision, no admission, one resident model, and
/// the Layer-1 PoW tag path as its only client.
///
/// The pin is checked once, at startup, by the same `pinned_model_path_v2()` the one-shot modes
/// call — a full read and SHA-256 of the artifact. The path is NOT reopened afterwards: the model
/// this process serves for its whole life is the one it digested, which is what makes amortising
/// the read safe rather than a hole in the pin.
///
/// Protocol, newline-delimited JSON in both directions:
///
/// * driver → agent: `{"id":<u64>,"n_predict":<u32>,"prompt_hex":"<hex>"}`
/// * agent → driver: one `@palw-pow1 {"v":1,"ready":true,…}` after the model loads, then one
///   `@palw-pow1 {"v":1,"id":<u64>,"ok":true,"projection":{…}}` per request, in request order.
///
/// The prompt is hex so the transport is byte-exact and newline-safe: the job is defined over raw
/// bytes, and a protocol that could not carry a `\n` would quietly change the input.
///
/// A job error exits the process, exactly as it does in the one-shot modes. That is deliberate:
/// making per-job errors recoverable would mean turning `execute_on_context`'s `die` calls into a
/// `Result`, which is touching the computation to buy something the driver already has for free —
/// respawn, and fall back to a one-shot process. The agent is an accelerator, never an authority.
fn run_pow_agent() {
    use std::io::BufRead;

    let started = std::time::Instant::now();
    let model_path = pinned_model_path_v2();
    let ctx = unsafe { shim_open(format!("{}\0", model_path.display()).as_ptr(), N_CTX, N_BATCH, N_THREADS) };
    if ctx.is_null() {
        die(format!("llama.cpp failed to load {}", model_path.display()));
    }
    let n_vocab = unsafe { shim_n_vocab(ctx) };
    eprintln!("[palw-worker] pow-agent: model loaded in {:?} (n_vocab={n_vocab})", started.elapsed());

    emit_agent_frame(&serde_json::json!({
        "v": 1,
        "schema": AGENT_SCHEMA,
        "ready": true,
        "pid": std::process::id(),
        "n_vocab": n_vocab,
        "model_profile_id": hex(model_profile_id()),
        "runtime_class_id": hex(runtime_class_id()),
        "runtime_manifest_hash": hex(runtime_manifest_hash()),
    }));

    let mut stdin = std::io::stdin().lock();
    let mut line = String::new();
    let mut served: u64 = 0;
    loop {
        line.clear();
        let n = stdin.read_line(&mut line).unwrap_or_else(|e| die(format!("pow-agent: cannot read a request line: {e}")));
        if n == 0 {
            // EOF. The driver closed its end, or exited and the kernel closed it for us — the
            // reason a node that dies without reaping this process still leaves no orphan.
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: serde_json::Value =
            serde_json::from_str(trimmed).unwrap_or_else(|e| die(format!("pow-agent: cannot parse a request line: {e}")));
        let id = req.get("id").and_then(|v| v.as_u64()).unwrap_or_else(|| die("pow-agent: request lacks a u64 id".into()));
        let n_predict = req
            .get("n_predict")
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or_else(|| die("pow-agent: request lacks a u32 n_predict".into()));
        if n_predict == 0 {
            die("pow-agent: n_predict must be at least 1".into());
        }
        let prompt_hex = req
            .get("prompt_hex")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| die("pow-agent: request lacks a prompt_hex field".into()));
        if prompt_hex.is_empty() || prompt_hex.len() % 2 != 0 {
            die("pow-agent: prompt_hex is empty or is not an even number of hex digits".into());
        }
        let mut input = vec![0u8; prompt_hex.len() / 2];
        faster_hex::hex_decode(prompt_hex.as_bytes(), &mut input)
            .unwrap_or_else(|e| die(format!("pow-agent: prompt_hex is not hex: {e}")));

        let exec = execute_on_context(ctx, &input, n_predict, std::time::Instant::now());
        emit_agent_frame(&serde_json::json!({
            "v": 1,
            "id": id,
            "ok": true,
            "projection": projection_doc(&input, n_predict, &exec),
        }));
        served += 1;

        // BETWEEN jobs, never inside one: return the context to a pristine decode state. This is
        // what makes job N+1 see exactly what a fresh process sees, and the order matters — the
        // equivalence was measured as job-then-reset, so job 1 runs on a virgin context under
        // literally one-shot conditions.
        let rc = unsafe { shim_reset_context(ctx) };
        if rc != 0 {
            die(format!(
                "pow-agent: shim_reset_context failed with {rc} after job {id}; refusing to serve another job on a \
                 context of unknown state"
            ));
        }
    }

    unsafe { shim_close(ctx) };
    eprintln!("[palw-worker] pow-agent: stdin closed after {served} job(s) in {:?}; exiting", started.elapsed());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut mode: Option<String> = None;
    let mut n_predict: Option<u32> = None;
    let mut out_path: Option<String> = None;
    let mut name: Option<String> = None;
    let mut envelope_path: Option<String> = None;
    let mut call_path: Option<String> = None;
    let mut runs: Option<u32> = None;
    let mut decode_override: Option<u32> = None;
    let mut legs_flag = false;
    let mut trace_out: Option<String> = None;
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
            "--name" => {
                i += 1;
                name = args.get(i).cloned();
            }
            "--envelope" => {
                i += 1;
                envelope_path = args.get(i).cloned();
            }
            "--call" => {
                i += 1;
                call_path = args.get(i).cloned();
            }
            "--runs" => {
                i += 1;
                runs = args.get(i).and_then(|s| s.parse().ok());
            }
            "--decode" => {
                i += 1;
                decode_override = args.get(i).and_then(|s| s.parse().ok());
            }
            "--legs" => legs_flag = true,
            "--trace-out" => {
                i += 1;
                trace_out = args.get(i).cloned();
            }
            other => die(format!("unknown argument {other:?}")),
        }
        i += 1;
    }

    // The v2 interface deliberately has no `--n-predict`: the prefill/decode budgets are
    // separate, explicit envelope fields (VPS design §5.4), and accepting the ambiguous v1
    // flag here would reintroduce exactly the total-vs-decode confusion v2 removes. `pow-agent`
    // rejects it for a different reason — its ceiling is per REQUEST, so a process-wide one
    // would be either ignored or silently overriding, and both are worse than an error.
    if matches!(
        mode.as_deref(),
        Some("v2-job"
            | "v2-manifest"
            | "v2-golden-gen"
            | "v2-selftest"
            | "v2-golden-show"
            | "v2-golden-envelope"
            | "v2-legs-job"
            | "v2-legs-selftest"
            | "v2-legs-open"
            | "v2-legs-open-request"
            | "v2-legs-open-verify"
            | "v2-replay-bench"
            | "v3-job"
            | "v3-manifest"
            | "geometry"
            | "pow-agent")
    ) && n_predict.is_some()
    {
        die("--n-predict does not apply to this mode; the v2 modes take token budgets from the job envelope and              --mode pow-agent takes one per request"
            .into());
    }

    match mode.as_deref() {
        Some("v2-job") => run_v2_job(),
        Some("v2-manifest") => run_v2_manifest(),
        Some("v3-job") => {
            let dir = trace_out.unwrap_or_else(|| die("--trace-out <dir> is required for v3-job: a job whose trace is not retained cannot be defended".into()));
            run_v3_job(Path::new(&dir));
        }
        Some("v3-manifest") => run_v3_manifest(),
        Some("geometry") => run_geometry(),
        Some("v2-golden-gen") => {
            let out = out_path.unwrap_or_else(|| die("--out <path> is required for v2-golden-gen".into()));
            run_v2_golden_gen(&out);
        }
        Some("v2-selftest") => run_v2_selftest(),
        Some("v2-legs-job") => run_v2_legs_job(),
        Some("v2-legs-selftest") => run_v2_legs_selftest(),
        Some("v2-golden-envelope") => {
            let name = name.unwrap_or_else(|| die("--name <golden-job> is required for v2-golden-envelope".into()));
            run_v2_golden_envelope(&name);
        }
        Some("v2-replay-bench") => {
            let name = name.unwrap_or_else(|| die("--name <golden-job> is required for v2-replay-bench".into()));
            run_v2_replay_bench(&name, runs.unwrap_or(10), decode_override, legs_flag);
        }
        Some("v2-legs-open") => run_v2_legs_open(),
        Some("v2-legs-open-request") => {
            let envelope = envelope_path.unwrap_or_else(|| die("--envelope <path> is required for v2-legs-open-request".into()));
            run_v2_legs_open_request(&envelope);
        }
        Some("v2-legs-open-verify") => {
            let call = call_path.unwrap_or_else(|| die("--call <path> is required for v2-legs-open-verify".into()));
            run_v2_legs_open_verify(&call);
        }
        Some("v2-golden-show") => run_v2_golden_show(),
        Some("pow-agent") => run_pow_agent(),
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
            let model_path = pinned_model_path_v2();
            let exec = execute(&model_path, &input, n_predict);

            println!("{}", projection_doc(&input, n_predict, &exec));
        }
        other => die(format!(
            "usage: palw-worker --mode manifest | --mode self-job|verify --prompt-stdin --n-predict N |              --mode pow-agent | --mode v2-job | --mode v2-manifest (got mode {other:?})"
        )),
    }
}
