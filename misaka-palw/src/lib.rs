//! MISAKA Verified LLM Token-Weighted BFT — the compute runtime bridge.
//!
//! This crate is the seam between consensus ([`kaspa_consensus_core::vlt`]) and the PALW
//! llama.cpp runtime that actually executes Qwen3.6-35B-A3B. Consensus never sees a tensor; it
//! sees commitments. This crate is what produces those commitments and, on the verifier side,
//! reproduces them.
//!
//! # The one design decision everything else follows from
//!
//! PALW already defines what it means for two independent executions of one job to agree:
//! `MatchProjectionV1`, a 16-field projection that its k=2 replica matcher requires to byte-match.
//! Consensus separately requires an executor's `R_j` and a verifier's replayed `R_j` to be equal.
//!
//! These must be **the same predicate**, not two similar ones. So [`MatchProjection`] carries all
//! 16 fields and [`MatchProjection::to_compute_receipt`] folds them into a
//! [`ComputeReceipt`] losslessly: the four fields the receipt names directly, and the other
//! twelve hashed into `trace_commitment` by [`MatchProjection::residual_commitment`]. Because
//! [`compute_receipt_hash`] covers every field of the receipt, equal receipt hashes hold **iff**
//! the two projections are identical.
//!
//! Inventing a parallel notion of "same result" would be the classic way to get a consensus rule
//! that disagrees with the runtime it is supposed to be adjudicating.
//!
//! # What must never enter a commitment
//!
//! A PALW receipt carries values drawn from the OS CSPRNG on every run — `receipt_id`, the job
//! id, nonce, salt, and the per-run signing key. Its own documentation is explicit that these do
//! not reproduce across runs, and `validate_bindings` in fact requires them to **differ** between
//! two replicas of one job.
//!
//! None of them may reach [`ComputeReceipt`]. Folding any of them into `trace_commitment` would
//! make an honest verifier's replay differ from an honest executor's receipt **by construction** —
//! and since consensus acceptance is refutation-dominant, that would zero honest validators' VLT
//! and arm `ForgedReceipt` slashing against them. [`MatchProjection`] therefore models the
//! projection *only*; there is deliberately no field on it that could carry per-run randomness.
//!
//! # Hardware scope
//!
//! PALW's production determinism class is "fp per-vendor": byte-identical results hold within one
//! microarchitecture and toolchain, not across vendors. The registered profile is Apple Silicon +
//! Metal. Consensus enforces the matching half of this by drawing verifiers only from validators
//! declaring the same `runtime_class_id` (see `vlt::select_verifiers`); this crate enforces its
//! half by refusing to run against a runtime whose identity is not the registered one
//! ([`ComputeRuntime::probe`]).

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use blake2b_simd::Params as Blake2bParams;
use kaspa_consensus_core::vlt::{
    ComputeReceipt, LlmJobSpec, ReplayResiduals, VLT_PAYLOAD_VERSION_V1, derive_runtime_class_id, derive_runtime_hash, palw_pins,
    qwen35_pins, residual_commitment,
};
#[cfg(feature = "devnet-vlt-fixture")]
use kaspa_consensus_core::vlt::{devnet_fixture, devnet_fixture_id};
use kaspa_hashes::Hash64;
use serde::{Deserialize, Serialize};

/// Keyed-BLAKE2b-512 domain for [`MatchProjection::residual_commitment`].
///
/// Re-exported from consensus rather than restated: the fold's output is what a receipt carries as
/// `trace_commitment` and what a verdict's replay proof is checked against, so a second definition
/// here would be a second thing to keep in sync — and the failure mode of them drifting is that
/// every honest proof is rejected.
pub use kaspa_consensus_core::vlt::REPLAY_RESIDUAL_COMMITMENT_KEY as PALW_RESIDUAL_COMMITMENT_KEY;

/// Client for the v2 `palw-agent` UDS protocol (`misaka-palw-agent-borsh/v1`). Separate from the
/// v1 subprocess bridge below: the v1 `PalwWorkerRuntime` drives `palw-worker --mode self-job`
/// over the frozen JSON contract, while this speaks framed Borsh to a supervised agent.
///
/// **Unix-only, by protocol.** `misaka-palw-agent-borsh/v1` is defined over an `AF_UNIX`
/// stream socket (VPS design v0.1 §5, §10.3) and its peer-credential admission check
/// (`SO_PEERCRED`/`getpeereid` in `misaka-palw-agent`) has no Windows equivalent, so this module
/// is gated rather than stubbed: a stub would compile a client that can never connect. The rest
/// of this crate — the v1 `PalwWorkerRuntime` subprocess bridge — is portable, which is what
/// keeps `kaspad` (a non-optional dependent) building on every supported host.
#[cfg(unix)]
pub mod agent_client;

/// The submission schema this bridge understands.
pub const PALW_SUBMISSION_SCHEMA_V3: &str = "misaka.palw.testnet-submission.v3";

#[derive(Debug, thiserror::Error)]
pub enum PalwError {
    #[error("failed to launch the PALW worker at {path}: {source}")]
    Spawn { path: String, source: std::io::Error },
    #[error("the PALW worker exited with status {status}: {stderr}")]
    WorkerFailed { status: String, stderr: String },
    #[error("the PALW worker produced output that is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("submission schema is {got:?}, expected {expected:?}")]
    UnexpectedSchema { got: String, expected: &'static str },
    #[error("submission is missing the required field {0:?}")]
    MissingField(&'static str),
    #[error("field {field:?} is not a {expected}")]
    MalformedField { field: &'static str, expected: &'static str },
    #[error("field {field:?} is not a {expected_len}-byte hex digest")]
    BadDigest { field: &'static str, expected_len: usize },
    #[error(
        "runtime identity mismatch: this node is running {got}, but the consensus-registered profile is {expected}. \
         Executing against an unregistered build mints no VLT and would refute honest peers."
    )]
    RuntimeIdentityMismatch { got: Hash64, expected: Hash64 },
    #[error("the PALW worker did not finish within {0:?}")]
    Timeout(Duration),
    /// The devnet fixture runs exactly one job shape, so a spec asking for another is refused
    /// rather than executed at a different price. Abstaining is the safe half of the asymmetry:
    /// [`ComputeRuntime::replay`] returning an error costs a verdict, whereas a receipt at the
    /// wrong shape would either mint the wrong VLT or refute an honest peer.
    #[cfg(feature = "devnet-vlt-fixture")]
    #[error(
        "the devnet fixture executes a fixed {expected}-token job, but this spec declares max_tokens={declared}. \
         Every node on a fixture devnet must run the same shape, or one job is not one job's worth of weight."
    )]
    FixtureJobShape { declared: u32, expected: u32 },
}

/// PALW's `MatchProjectionV1` — the 16 fields its k=2 matcher requires two replicas of one job to
/// agree on, byte for byte.
///
/// Field names mirror the PALW integration spec exactly. Nothing per-run lives here: see the
/// module docs on why `receipt_id`, job id, nonce, salt and the signing key must stay out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchProjection {
    pub job_nullifier: Hash64,
    pub request_commitment: Hash64,
    pub model_profile_id: Hash64,
    pub runtime_class_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub shape_profile_id: Hash64,
    pub cu_ruleset_id: Hash64,
    pub canonical_compute_units: u64,
    pub prefill_tokens: u32,
    pub decode_tokens: u32,
    pub operation_schedule_commitment: Hash64,
    pub schedule_event_count: u64,
    pub output_commitment: Hash64,
    pub trace_scheme_id: Hash64,
    pub gemm_trace_root: Hash64,
    pub trace_event_count: u64,
}

impl MatchProjection {
    /// Digest over the twelve projection fields the [`ComputeReceipt`] does not name directly.
    ///
    /// Carried as the receipt's `trace_commitment`, which is what makes the receipt a lossless
    /// encoding of the projection: `compute_receipt_hash` covers `output_commitment`,
    /// `prefill_tokens`, `decode_tokens` and this digest, so equal receipt hashes hold **iff**
    /// the whole 16-field projection matches. Consensus's equality test and PALW's k=2 test are
    /// then literally the same test.
    ///
    /// Fixed-width little-endian scalars, fixed field order — a consensus identity must not move
    /// if a serializer's layout ever changes.
    pub fn residual_commitment(&self) -> Hash64 {
        residual_commitment(&self.residuals())
    }

    /// This projection's residual fields, as a verdict publishes them.
    ///
    /// A verifier reveals exactly this to prove it executed the job: the fold below is one-way, so
    /// a node holding only the certificate has the digest and cannot produce the preimage.
    pub fn residuals(&self) -> ReplayResiduals {
        ReplayResiduals {
            job_nullifier: self.job_nullifier,
            request_commitment: self.request_commitment,
            model_profile_id: self.model_profile_id,
            runtime_class_id: self.runtime_class_id,
            runtime_manifest_hash: self.runtime_manifest_hash,
            shape_profile_id: self.shape_profile_id,
            cu_ruleset_id: self.cu_ruleset_id,
            canonical_compute_units: self.canonical_compute_units,
            operation_schedule_commitment: self.operation_schedule_commitment,
            schedule_event_count: self.schedule_event_count,
            trace_scheme_id: self.trace_scheme_id,
            gemm_trace_root: self.gemm_trace_root,
            trace_event_count: self.trace_event_count,
        }
    }

    /// Fold this projection into the consensus [`ComputeReceipt`].
    ///
    /// `prefill_tokens` / `decode_tokens` cross over unchanged, because they are what
    /// `normalize_vlt` prices — and they are *projection* fields, so consensus prices only work
    /// two independent replicas agreed on.
    pub fn to_compute_receipt(&self) -> ComputeReceipt {
        ComputeReceipt {
            version: VLT_PAYLOAD_VERSION_V1,
            output_commitment: self.output_commitment,
            prefill_tokens: self.prefill_tokens,
            decode_tokens: self.decode_tokens,
            trace_commitment: self.residual_commitment(),
        }
    }

    /// Parse a `misaka.palw.testnet-submission.v3` document.
    ///
    /// Accepts the projection either nested under `match_projection` / `projection` or flattened
    /// at the top level, since the worker's own layout is not something consensus should be
    /// coupled to. Every field is required: a missing one is an error, never a zero default —
    /// silently defaulting would let two different executions produce the same projection.
    pub fn from_submission_json(doc: &serde_json::Value) -> Result<Self, PalwError> {
        if let Some(schema) = doc.get("schema").and_then(|v| v.as_str())
            && schema != PALW_SUBMISSION_SCHEMA_V3
        {
            return Err(PalwError::UnexpectedSchema { got: schema.to_owned(), expected: PALW_SUBMISSION_SCHEMA_V3 });
        }
        let p = doc.get("match_projection").or_else(|| doc.get("projection")).unwrap_or(doc);
        Ok(Self {
            job_nullifier: digest_field(p, "job_nullifier")?,
            request_commitment: digest_field(p, "request_commitment")?,
            model_profile_id: digest_field(p, "model_profile_id")?,
            runtime_class_id: digest_field(p, "runtime_class_id")?,
            runtime_manifest_hash: digest_field(p, "runtime_manifest_hash")?,
            shape_profile_id: digest_field(p, "shape_profile_id")?,
            cu_ruleset_id: digest_field(p, "cu_ruleset_id")?,
            canonical_compute_units: u64_field(p, "canonical_compute_units")?,
            prefill_tokens: u32_field(p, "prefill_tokens")?,
            decode_tokens: u32_field(p, "decode_tokens")?,
            operation_schedule_commitment: digest_field(p, "operation_schedule_commitment")?,
            schedule_event_count: u64_field(p, "schedule_event_count")?,
            output_commitment: digest_field(p, "output_commitment")?,
            trace_scheme_id: digest_field(p, "trace_scheme_id")?,
            gemm_trace_root: digest_field(p, "gemm_trace_root")?,
            trace_event_count: u64_field(p, "trace_event_count")?,
        })
    }
}

/// Widen a hex digest of any length up to 64 bytes into the overlay's [`Hash64`] identity space.
///
/// PALW emits 32-byte (SHA-256/BLAKE-256) digests for most commitments; the overlay is 64-byte
/// throughout. Right-padding rather than re-hashing keeps the mapping injective and inspectable —
/// the PALW digest is still readable in the first half of the consensus value.
fn widen_digest(bytes: &[u8], field: &'static str) -> Result<Hash64, PalwError> {
    if bytes.is_empty() || bytes.len() > 64 {
        return Err(PalwError::BadDigest { field, expected_len: 64 });
    }
    let mut out = [0u8; 64];
    out[..bytes.len()].copy_from_slice(bytes);
    Ok(Hash64::from_bytes(out))
}

fn digest_field(v: &serde_json::Value, field: &'static str) -> Result<Hash64, PalwError> {
    let s = v
        .get(field)
        .ok_or(PalwError::MissingField(field))?
        .as_str()
        .ok_or(PalwError::MalformedField { field, expected: "hex string" })?;
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() % 2 != 0 {
        return Err(PalwError::BadDigest { field, expected_len: 64 });
    }
    let mut bytes = vec![0u8; s.len() / 2];
    faster_hex::hex_decode(s.as_bytes(), &mut bytes).map_err(|_| PalwError::BadDigest { field, expected_len: 64 })?;
    widen_digest(&bytes, field)
}

fn u64_field(v: &serde_json::Value, field: &'static str) -> Result<u64, PalwError> {
    v.get(field)
        .ok_or(PalwError::MissingField(field))?
        .as_u64()
        .ok_or(PalwError::MalformedField { field, expected: "unsigned integer" })
}

fn u32_field(v: &serde_json::Value, field: &'static str) -> Result<u32, PalwError> {
    u32::try_from(u64_field(v, field)?).map_err(|_| PalwError::MalformedField { field, expected: "u32" })
}

/// Identity of the runtime a node is actually running, as reported by the worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeIdentity {
    /// `h_R`, comparable against the consensus-registered `ModelCostEntry::runtime_hash`.
    pub runtime_hash: Hash64,
    /// The determinism class, comparable against `ModelCostEntry::runtime_class_id`.
    pub runtime_class_id: Hash64,
}

impl RuntimeIdentity {
    /// The identity of the consensus-registered PALW Metal profile — what a correctly-configured
    /// executor must report.
    pub fn registered_palw_metal() -> Self {
        Self {
            runtime_hash: derive_runtime_hash(
                palw_pins::LLAMA_COMMIT,
                palw_pins::LLAMA_PATCH_SHA256,
                palw_pins::LLAMA_BUILD_NUMBER,
                palw_pins::METAL_BUILD_PROFILE,
            ),
            runtime_class_id: derive_runtime_class_id(palw_pins::METAL_RUNTIME_CLASS),
        }
    }

    /// The identity of the consensus-registered Qwen3.5-2B palw-lite Metal profile
    /// (`misaka-palw-worker` linking the pinned upstream llama.cpp).
    pub fn registered_qwen35_2b_metal() -> Self {
        Self {
            runtime_hash: derive_runtime_hash(
                qwen35_pins::LLAMA_COMMIT,
                qwen35_pins::LLAMA_PATCH_SHA256,
                qwen35_pins::LLAMA_BUILD_NUMBER,
                qwen35_pins::METAL_BUILD_PROFILE,
            ),
            runtime_class_id: derive_runtime_class_id(qwen35_pins::METAL_RUNTIME_CLASS),
        }
    }

    /// Every profile this build knows to be consensus-registered somewhere. The per-network
    /// question — is this profile registered *here* — is answered by the model cost table lookup
    /// in the compute role's startup, not by this list.
    pub fn known_registered() -> [Self; 2] {
        [Self::registered_palw_metal(), Self::registered_qwen35_2b_metal()]
    }
}

/// What a node needs to drive a compute runtime, as executor and as verifier.
///
/// A trait rather than a concrete type so the validator service can be exercised without a 24 GB
/// model on disk — see [`MockRuntime`]. The two methods differ only in intent: `execute` produces
/// a projection for a job this node originated, `verify` produces one for a job it was
/// sortitioned to audit. Both return a projection; consensus does the comparing.
pub trait ComputeRuntime: Send + Sync {
    /// The runtime this node is actually running.
    fn probe(&self) -> Result<RuntimeIdentity, PalwError>;

    /// Execute `spec` over `prompt` and return the resulting projection.
    fn execute(&self, spec: &LlmJobSpec, prompt: &[u8]) -> Result<MatchProjection, PalwError>;

    /// Re-execute `spec` over `prompt` as an independent replica of a peer's job.
    ///
    /// Separate from [`Self::execute`] because PALW distinguishes the two modes (`--mode
    /// self-job` vs `--mode verify`), and because a verifier must never reuse an executor's
    /// artifacts — re-running the peer's own receipt would confirm anything.
    ///
    /// It takes no argument describing what the peer claimed, and that is the point: a replay
    /// audit handed the answer is not an audit. Consensus compares the two receipts afterwards.
    /// (The caller could not supply one honestly in any case — a certificate carries a
    /// [`ComputeReceipt`], which is this projection already folded down.)
    fn replay(&self, spec: &LlmJobSpec, prompt: &[u8]) -> Result<MatchProjection, PalwError>;

    /// Whether this runtime's identity is one this build knows to be consensus-registered.
    ///
    /// A node running an unregistered build mints nothing (the model table lookup fails) and,
    /// worse, would refute honest peers if it were sortitioned as a verifier. Callers should
    /// refuse to participate rather than produce garbage. Which registered profile the network
    /// actually runs is the model cost table's question; this check only refuses builds that
    /// match none of them.
    fn assert_registered(&self) -> Result<(), PalwError> {
        let got = self.probe()?;
        let known = RuntimeIdentity::known_registered();
        if !known.iter().any(|expected| expected.runtime_hash == got.runtime_hash) {
            // Report the small-model profile as the expectation: it is the one a devnet operator
            // wiring a worker by hand is overwhelmingly likely to have meant.
            return Err(PalwError::RuntimeIdentityMismatch {
                got: got.runtime_hash,
                expected: RuntimeIdentity::registered_qwen35_2b_metal().runtime_hash,
            });
        }
        Ok(())
    }
}

/// How to reach the pinned PALW worker on this host.
#[derive(Clone, Debug)]
pub struct PalwWorkerConfig {
    /// Path to the `palw-worker` binary built from the pinned runtime tree.
    pub worker_bin: PathBuf,
    /// Scratch directory for specs, results and submissions.
    pub work_dir: PathBuf,
    /// Wall-clock ceiling for one job. A verifier that hangs is indistinguishable from one that
    /// is absent, so this bounds the damage rather than leaving the service stuck.
    pub timeout: Duration,
}

/// [`ComputeRuntime`] backed by the pinned `palw-worker` binary, driven as a subprocess.
///
/// A subprocess, not a linked library, on purpose: `runtime_hash` is supposed to commit to an
/// exact build of llama.cpp with the PALW patch, and the honest way to satisfy that is for the
/// node to invoke exactly that build rather than to link something it compiled itself. It also
/// keeps Metal, GGML and a 24 GB model out of `kaspad`'s address space and dependency tree.
pub struct PalwWorkerRuntime {
    cfg: PalwWorkerConfig,
}

impl PalwWorkerRuntime {
    pub fn new(cfg: PalwWorkerConfig) -> Self {
        Self { cfg }
    }

    fn run(&self, args: &[&str], stdin_bytes: Option<&[u8]>) -> Result<serde_json::Value, PalwError> {
        use std::io::{Read, Write};
        use std::process::Stdio;

        let mut cmd = Command::new(&self.cfg.worker_bin);
        cmd.args(args).current_dir(&self.cfg.work_dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|source| PalwError::Spawn { path: self.cfg.worker_bin.display().to_string(), source })?;
        // Close stdin after writing: the worker reads the prompt to EOF, so leaving the pipe open
        // would deadlock it against our own wait below.
        if let Some(mut stdin) = child.stdin.take()
            && let Some(bytes) = stdin_bytes
        {
            let _ = stdin.write_all(bytes);
        }
        // Drain BOTH pipes concurrently with the wait. An OS pipe buffer is ~64 KiB, and a real
        // worker's stderr can exceed that before it exits (llama.cpp's model-load narration alone
        // does) — the child then blocks in write(), never exits, and the poll below kills a
        // perfectly healthy job at the full timeout. Found live: five executors, every job
        // "did not finish within 900s", zero VLT credited.
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
        // Enforce the wall-clock ceiling. A worker that never returns is indistinguishable from an
        // absent one to the rest of the network, but to *this* node it is much worse: the job slot
        // stays occupied and the validator silently stops auditing. Kill it and report instead.
        // (On the kill path the readers see EOF and finish on their own.)
        self.wait_with_timeout(&mut child)?;
        let status = child.wait().map_err(|source| PalwError::Spawn { path: self.cfg.worker_bin.display().to_string(), source })?;
        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();
        if !status.success() {
            return Err(PalwError::WorkerFailed {
                status: status.to_string(),
                stderr: String::from_utf8_lossy(&stderr).trim().to_owned(),
            });
        }
        Ok(serde_json::from_slice(&stdout)?)
    }

    /// Poll for `child` to exit within [`PalwWorkerConfig::timeout`], killing it if it does not.
    ///
    /// A poll loop rather than a watchdog thread because the caller is already a dedicated
    /// blocking task: there is no other work to interleave, and a coarse poll costs nothing next
    /// to a job measured in minutes.
    fn wait_with_timeout(&self, child: &mut std::process::Child) -> Result<(), PalwError> {
        const POLL_INTERVAL: Duration = Duration::from_millis(200);
        let deadline = std::time::Instant::now() + self.cfg.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return Ok(()),
                Ok(None) => {}
                Err(source) => return Err(PalwError::Spawn { path: self.cfg.worker_bin.display().to_string(), source }),
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(PalwError::Timeout(self.cfg.timeout));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

impl ComputeRuntime for PalwWorkerRuntime {
    fn probe(&self) -> Result<RuntimeIdentity, PalwError> {
        let doc = self.run(&["--mode", "manifest"], None)?;
        Ok(RuntimeIdentity {
            runtime_hash: digest_field(&doc, "runtime_manifest_hash")?,
            runtime_class_id: digest_field(&doc, "runtime_class_id")?,
        })
    }

    fn execute(&self, spec: &LlmJobSpec, prompt: &[u8]) -> Result<MatchProjection, PalwError> {
        let n_predict = spec.max_tokens.to_string();
        let doc = self.run(&["--mode", "self-job", "--prompt-stdin", "--n-predict", &n_predict], Some(prompt))?;
        MatchProjection::from_submission_json(&doc)
    }

    fn replay(&self, spec: &LlmJobSpec, prompt: &[u8]) -> Result<MatchProjection, PalwError> {
        // Nothing about the peer's claim reaches the worker: a verifier must derive its own result
        // independently, and feeding it the answer it is meant to check is how a replay audit
        // turns into a rubber stamp. Consensus compares the two afterwards.
        let n_predict = spec.max_tokens.to_string();
        let doc = self.run(&["--mode", "verify", "--prompt-stdin", "--n-predict", &n_predict], Some(prompt))?;
        MatchProjection::from_submission_json(&doc)
    }
}

/// A deterministic in-process [`ComputeRuntime`] for tests and dry runs.
///
/// Derives a projection from `(spec, prompt)` by hashing, so two "replicas" agree exactly like an
/// honest pair would, and `divergent` lets a test model the dishonest/cross-class case without a
/// GPU. Never registered as the real runtime: [`Self::probe`] reports the registered identity only
/// when asked to, so `assert_registered` still behaves meaningfully in tests.
#[derive(Clone, Debug, Default)]
pub struct MockRuntime {
    /// Perturbs every projection, modelling an executor that computed something else (or a
    /// verifier in the wrong determinism class).
    pub divergent: bool,
    /// Report the registered runtime identity from [`Self::probe`].
    pub claim_registered: bool,
}

impl MockRuntime {
    fn project(&self, spec: &LlmJobSpec, prompt: &[u8]) -> MatchProjection {
        let d = |tag: &str| -> Hash64 {
            let mut h = Blake2bParams::new().hash_length(64).key(b"misaka-palw-mock-v1").to_state();
            h.update(tag.as_bytes());
            h.update(spec.model_weights_hash.as_byte_slice());
            h.update(spec.runtime_hash.as_byte_slice());
            h.update(&spec.sampling_seed);
            h.update(prompt);
            if self.divergent {
                h.update(b"divergent");
            }
            let mut out = [0u8; 64];
            out.copy_from_slice(h.finalize().as_bytes());
            Hash64::from_bytes(out)
        };
        MatchProjection {
            job_nullifier: d("job_nullifier"),
            request_commitment: d("request_commitment"),
            model_profile_id: spec.model_weights_hash,
            runtime_class_id: derive_runtime_class_id(palw_pins::METAL_RUNTIME_CLASS),
            runtime_manifest_hash: spec.runtime_hash,
            shape_profile_id: d("shape"),
            cu_ruleset_id: d("cu_ruleset"),
            canonical_compute_units: 41_692,
            prefill_tokens: prompt.len().min(u32::MAX as usize) as u32,
            decode_tokens: spec.max_tokens.saturating_sub(prompt.len().min(u32::MAX as usize) as u32).min(64),
            operation_schedule_commitment: d("schedule"),
            schedule_event_count: 80,
            output_commitment: d("output"),
            trace_scheme_id: d("trace_scheme"),
            gemm_trace_root: d("gemm"),
            trace_event_count: 2_466,
        }
    }
}

impl ComputeRuntime for MockRuntime {
    fn probe(&self) -> Result<RuntimeIdentity, PalwError> {
        if self.claim_registered {
            Ok(RuntimeIdentity::registered_palw_metal())
        } else {
            Ok(RuntimeIdentity { runtime_hash: Hash64::from_bytes([0xAB; 64]), runtime_class_id: Hash64::from_bytes([0xCD; 64]) })
        }
    }

    fn execute(&self, spec: &LlmJobSpec, prompt: &[u8]) -> Result<MatchProjection, PalwError> {
        Ok(self.project(spec, prompt))
    }

    fn replay(&self, spec: &LlmJobSpec, prompt: &[u8]) -> Result<MatchProjection, PalwError> {
        Ok(self.project(spec, prompt))
    }
}

/// The devnet fixture's executor: a deterministic runtime that reports the identity consensus
/// registered for `genesis_hash`, so a private devnet can drive the **whole** production compute
/// path with no model on disk.
///
/// Deliberately not [`MockRuntime`] with a flag. Mock is a test double whose whole contract is
/// that it does *not* claim a registered identity — reusing it here would delete the check that
/// catches a node running an unregistered build. This is a separate type behind a separate
/// feature, and it claims exactly one identity: the one derived from this network's own genesis.
///
/// Two replicas of this runtime agree byte-for-byte on the same `(spec, prompt)`, which is what
/// makes a verifier quorum reachable — and they disagree with any other network's fixture, because
/// the genesis hash is inside the projection as well as inside the identity.
///
/// # One job shape
///
/// It executes exactly [`devnet_fixture::JOB_MAX_TOKENS`]-token jobs — 10 prefill, 5 decode, 50
/// VLT — and refuses anything else. A fixture whose token counts varied with the prompt would make
/// each validator's weight depend on the size of a file on its own disk, which is precisely the
/// variable the asymmetric-weight experiment is trying to isolate.
#[cfg(feature = "devnet-vlt-fixture")]
#[derive(Clone, Debug)]
pub struct DevnetFixtureRuntime {
    genesis_hash: Hash64,
}

#[cfg(feature = "devnet-vlt-fixture")]
impl DevnetFixtureRuntime {
    pub fn new(genesis_hash: Hash64) -> Self {
        Self { genesis_hash }
    }

    /// The identity consensus registered for this network's fixture profile.
    pub fn identity(&self) -> RuntimeIdentity {
        RuntimeIdentity {
            runtime_hash: devnet_fixture_id(self.genesis_hash, devnet_fixture::RUNTIME_TAG),
            runtime_class_id: devnet_fixture_id(self.genesis_hash, devnet_fixture::CLASS_TAG),
        }
    }

    fn project(&self, spec: &LlmJobSpec, prompt: &[u8]) -> Result<MatchProjection, PalwError> {
        // One shape, checked rather than assumed. The token counts below are the *profile's*, so a
        // spec with a different ceiling would either produce a receipt claiming more work than its
        // own spec allowed (`ReceiptExceedsSpecLimit` — zero VLT, silently) or leave the executor
        // room to decode more than the plan priced. Refusing makes both a logged abstention.
        if spec.max_tokens != devnet_fixture::JOB_MAX_TOKENS {
            return Err(PalwError::FixtureJobShape { declared: spec.max_tokens, expected: devnet_fixture::JOB_MAX_TOKENS });
        }
        let d = |tag: &str| -> Hash64 {
            let mut h = Blake2bParams::new().hash_length(64).key(b"misaka-palw-devnet-fixture-v1").to_state();
            h.update(tag.as_bytes());
            h.update(self.genesis_hash.as_byte_slice());
            h.update(spec.model_weights_hash.as_byte_slice());
            h.update(spec.runtime_hash.as_byte_slice());
            h.update(&spec.sampling_seed);
            h.update(prompt);
            let mut out = [0u8; 64];
            out.copy_from_slice(h.finalize().as_bytes());
            Hash64::from_bytes(out)
        };
        // The token counts are the registered profile's fixed job shape, NOT a measurement of this
        // prompt. That is the point: the experiment varies how many jobs a validator completes, so
        // one job must be worth the same everywhere — 50 VLT, pinned by
        // `one_fixture_job_is_worth_fifty_vlt`. Sizing them off `prompt.len()` would instead make a
        // validator's weight a function of the size of a file on its own disk.
        //
        // The prompt is still fully inside the projection above (`job_nullifier`,
        // `request_commitment`, `output_commitment`, …), so two nodes' jobs stay distinct and a
        // verifier replaying the same `(spec, prompt)` still reproduces the receipt byte for byte.
        Ok(MatchProjection {
            job_nullifier: d("job_nullifier"),
            request_commitment: d("request_commitment"),
            model_profile_id: spec.model_weights_hash,
            runtime_class_id: devnet_fixture_id(self.genesis_hash, devnet_fixture::CLASS_TAG),
            runtime_manifest_hash: spec.runtime_hash,
            shape_profile_id: d("shape"),
            cu_ruleset_id: d("cu_ruleset"),
            canonical_compute_units: 1_024,
            prefill_tokens: devnet_fixture::JOB_PREFILL_TOKENS,
            decode_tokens: devnet_fixture::JOB_DECODE_TOKENS,
            operation_schedule_commitment: d("schedule"),
            schedule_event_count: 8,
            output_commitment: d("output"),
            trace_scheme_id: d("trace_scheme"),
            gemm_trace_root: d("gemm"),
            trace_event_count: 16,
        })
    }
}

#[cfg(feature = "devnet-vlt-fixture")]
impl ComputeRuntime for DevnetFixtureRuntime {
    fn probe(&self) -> Result<RuntimeIdentity, PalwError> {
        Ok(self.identity())
    }

    fn execute(&self, spec: &LlmJobSpec, prompt: &[u8]) -> Result<MatchProjection, PalwError> {
        self.project(spec, prompt)
    }

    fn replay(&self, spec: &LlmJobSpec, prompt: &[u8]) -> Result<MatchProjection, PalwError> {
        self.project(spec, prompt)
    }

    /// The fixture's own identity is the registered one *on the network it was derived for*, so
    /// this accepts it and nothing else. It does not accept the real PALW profile: a node running
    /// the fixture must never be able to pass itself off as a real executor.
    fn assert_registered(&self) -> Result<(), PalwError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::vlt::{QuantizationProfile, VerificationScheme, compute_receipt_hash};

    fn spec() -> LlmJobSpec {
        LlmJobSpec {
            version: VLT_PAYLOAD_VERSION_V1,
            model_weights_hash: Hash64::from_bytes([1; 64]),
            runtime_hash: Hash64::from_bytes([2; 64]),
            quantization: QuantizationProfile::Int4,
            input_commitment: Hash64::from_bytes([3; 64]),
            sampling_seed: [7; 32],
            max_tokens: 512,
            verification_scheme: VerificationScheme::CanonicalFullReplay,
        }
    }

    fn projection() -> MatchProjection {
        MockRuntime::default().project(&spec(), b"the capital of France is")
    }

    /// The property the whole bridge exists for: consensus receipt equality must be exactly PALW
    /// `MatchProjectionV1` equality — no looser (which would accept divergent executions) and no
    /// stricter (which would refute honest ones).
    #[test]
    fn receipt_equality_is_exactly_projection_equality() {
        let s = spec();
        let a = projection();
        assert_eq!(compute_receipt_hash(&s, &a.to_compute_receipt()), compute_receipt_hash(&s, &a.to_compute_receipt()));

        // Every one of the 16 fields must be load-bearing: perturbing any single one has to
        // change the receipt hash, or two different executions could pass as a match.
        let base = compute_receipt_hash(&s, &a.to_compute_receipt());
        let mutations: Vec<(&str, Box<dyn Fn(&mut MatchProjection)>)> = vec![
            ("job_nullifier", Box::new(|p: &mut MatchProjection| p.job_nullifier = Hash64::from_bytes([9; 64]))),
            ("request_commitment", Box::new(|p: &mut MatchProjection| p.request_commitment = Hash64::from_bytes([9; 64]))),
            ("model_profile_id", Box::new(|p: &mut MatchProjection| p.model_profile_id = Hash64::from_bytes([9; 64]))),
            ("runtime_class_id", Box::new(|p: &mut MatchProjection| p.runtime_class_id = Hash64::from_bytes([9; 64]))),
            ("runtime_manifest_hash", Box::new(|p: &mut MatchProjection| p.runtime_manifest_hash = Hash64::from_bytes([9; 64]))),
            ("shape_profile_id", Box::new(|p: &mut MatchProjection| p.shape_profile_id = Hash64::from_bytes([9; 64]))),
            ("cu_ruleset_id", Box::new(|p: &mut MatchProjection| p.cu_ruleset_id = Hash64::from_bytes([9; 64]))),
            ("canonical_compute_units", Box::new(|p: &mut MatchProjection| p.canonical_compute_units += 1)),
            ("prefill_tokens", Box::new(|p: &mut MatchProjection| p.prefill_tokens += 1)),
            ("decode_tokens", Box::new(|p: &mut MatchProjection| p.decode_tokens += 1)),
            (
                "operation_schedule_commitment",
                Box::new(|p: &mut MatchProjection| p.operation_schedule_commitment = Hash64::from_bytes([9; 64])),
            ),
            ("schedule_event_count", Box::new(|p: &mut MatchProjection| p.schedule_event_count += 1)),
            ("output_commitment", Box::new(|p: &mut MatchProjection| p.output_commitment = Hash64::from_bytes([9; 64]))),
            ("trace_scheme_id", Box::new(|p: &mut MatchProjection| p.trace_scheme_id = Hash64::from_bytes([9; 64]))),
            ("gemm_trace_root", Box::new(|p: &mut MatchProjection| p.gemm_trace_root = Hash64::from_bytes([9; 64]))),
            ("trace_event_count", Box::new(|p: &mut MatchProjection| p.trace_event_count += 1)),
        ];
        assert_eq!(mutations.len(), 16, "all 16 MatchProjectionV1 fields must be covered");
        for (name, mutate) in mutations {
            let mut m = a.clone();
            mutate(&mut m);
            assert_ne!(compute_receipt_hash(&s, &m.to_compute_receipt()), base, "field {name} does not affect the receipt hash");
        }
    }

    /// A verifier proves it executed the job by revealing the preimage of the receipt's
    /// `trace_commitment`. The bridge's projection must therefore produce residuals that fold back
    /// to exactly the value it put in the receipt — if the two ever disagreed, every honest
    /// confirmation would be rejected as a rubber stamp and no job could ever mint.
    #[test]
    fn residuals_fold_back_to_the_receipts_own_trace_commitment() {
        let p = projection();
        let receipt = p.to_compute_receipt();
        assert_eq!(residual_commitment(&p.residuals()), receipt.trace_commitment);

        // The certificate publishes only the fold, so a party holding the receipt cannot recover
        // the residuals; a different execution folds elsewhere.
        let other = MockRuntime { divergent: true, ..Default::default() }.project(&spec(), b"the capital of France is");
        assert_ne!(residual_commitment(&other.residuals()), receipt.trace_commitment);
    }

    /// Two honest replicas of one job agree; a divergent one does not. This is the k=2 test,
    /// expressed through the consensus receipt.
    #[test]
    fn honest_replicas_match_and_divergent_ones_do_not() {
        let s = spec();
        let prompt = b"the capital of France is";
        let executor = MockRuntime::default().execute(&s, prompt).unwrap();
        let verifier = MockRuntime::default().replay(&s, prompt).unwrap();
        assert_eq!(
            compute_receipt_hash(&s, &executor.to_compute_receipt()),
            compute_receipt_hash(&s, &verifier.to_compute_receipt()),
            "two honest replicas must reproduce one receipt"
        );

        let liar = MockRuntime { divergent: true, ..Default::default() }.replay(&s, prompt).unwrap();
        assert_ne!(compute_receipt_hash(&s, &executor.to_compute_receipt()), compute_receipt_hash(&s, &liar.to_compute_receipt()));
    }

    /// No per-run value may reach the receipt. PALW draws `receipt_id`, job id, nonce, salt and
    /// the signing key from the OS CSPRNG on every run — if any of them influenced the receipt,
    /// an honest verifier could never reproduce an honest executor's hash, VLT would never mint,
    /// and `ForgedReceipt` slashing would fire on honest parties.
    #[test]
    fn per_run_randomness_never_reaches_the_receipt() {
        let s = spec();
        let p = projection();
        let base = compute_receipt_hash(&s, &p.to_compute_receipt());

        // A submission carrying wildly different per-run identity fields must still parse to the
        // same projection, and therefore the same receipt.
        let mut doc = serde_json::to_value(&p).unwrap();
        doc["schema"] = serde_json::Value::String(PALW_SUBMISSION_SCHEMA_V3.to_owned());
        for (k, v) in [
            ("receipt_id", "aa".repeat(32)),
            ("scheduler_job_id", "bb".repeat(32)),
            ("nonce", "cc".repeat(16)),
            ("salt", "dd".repeat(16)),
            ("signer_key_id", "ee".repeat(32)),
            ("execution_nullifier", "ff".repeat(32)),
        ] {
            doc[k] = serde_json::Value::String(v);
        }
        let parsed = MatchProjection::from_submission_json(&doc).unwrap();
        assert_eq!(parsed, p, "per-run identity fields must not enter the projection");
        assert_eq!(compute_receipt_hash(&s, &parsed.to_compute_receipt()), base);
    }

    #[test]
    fn submission_parsing_is_strict() {
        let p = projection();
        let mut doc = serde_json::to_value(&p).unwrap();
        doc["schema"] = serde_json::Value::String(PALW_SUBMISSION_SCHEMA_V3.to_owned());
        assert_eq!(MatchProjection::from_submission_json(&doc).unwrap(), p);

        // Nested layouts are accepted; consensus should not be coupled to the worker's framing.
        let nested = serde_json::json!({ "schema": PALW_SUBMISSION_SCHEMA_V3, "match_projection": serde_json::to_value(&p).unwrap() });
        assert_eq!(MatchProjection::from_submission_json(&nested).unwrap(), p);

        // A foreign schema is refused rather than best-effort parsed.
        let mut wrong = doc.clone();
        wrong["schema"] = serde_json::Value::String("misaka.palw.testnet-submission.v2".to_owned());
        assert!(matches!(MatchProjection::from_submission_json(&wrong), Err(PalwError::UnexpectedSchema { .. })));

        // A missing field is an error, never a zero default: defaulting would let two different
        // executions collapse onto the same projection.
        let mut missing = doc.clone();
        missing.as_object_mut().unwrap().remove("output_commitment");
        assert!(matches!(MatchProjection::from_submission_json(&missing), Err(PalwError::MissingField("output_commitment"))));

        let mut bad = doc.clone();
        bad["schedule_event_count"] = serde_json::Value::String("many".to_owned());
        assert!(matches!(MatchProjection::from_submission_json(&bad), Err(PalwError::MalformedField { .. })));
    }

    /// PALW emits 32-byte digests; the overlay is 64-byte. The widening must be injective, or two
    /// distinct PALW results could map onto one consensus commitment.
    #[test]
    fn digest_widening_is_injective() {
        let a = widen_digest(&[1u8; 32], "x").unwrap();
        let b = widen_digest(&[2u8; 32], "x").unwrap();
        assert_ne!(a, b);
        // A 32-byte digest and the 64-byte value that starts with it are NOT the same input, and
        // must not collide.
        let mut long = [0u8; 64];
        long[..32].copy_from_slice(&[1u8; 32]);
        assert_eq!(widen_digest(&long, "x").unwrap(), a, "right-padding is the defined widening");
        assert!(widen_digest(&[], "x").is_err());
        assert!(widen_digest(&[0u8; 65], "x").is_err());
    }

    /// A node running an unregistered build must refuse to participate: it mints nothing, and as
    /// a sortitioned verifier it would refute honest peers.
    #[test]
    fn unregistered_runtimes_are_refused() {
        assert!(matches!(MockRuntime::default().assert_registered(), Err(PalwError::RuntimeIdentityMismatch { .. })));
        assert!(MockRuntime { claim_registered: true, ..Default::default() }.assert_registered().is_ok());
    }

    /// The fixture exists so that one job is worth one fixed amount of VLT on every node, and that
    /// has to be a property of the runtime rather than of each operator's configuration — the
    /// experiment varies how many jobs a validator completes, and nothing else.
    ///
    /// So: two prompts of very different lengths, one shape. And the prompt still separates the
    /// jobs, or five validators would be committing to the same job id and only the first would
    /// ever be creditable.
    #[cfg(feature = "devnet-vlt-fixture")]
    #[test]
    fn the_fixture_executes_one_job_shape_whatever_the_prompt() {
        let genesis = Hash64::from_bytes([0x11; 64]);
        let rt = DevnetFixtureRuntime::new(genesis);
        let mut s = spec();
        s.max_tokens = devnet_fixture::JOB_MAX_TOKENS;

        for prompt in [b"x".as_slice(), b"a very much longer prompt than the other one".as_slice()] {
            let p = rt.execute(&s, prompt).unwrap();
            assert_eq!(p.prefill_tokens, devnet_fixture::JOB_PREFILL_TOKENS, "prompt length must not price the job");
            assert_eq!(p.decode_tokens, devnet_fixture::JOB_DECODE_TOKENS);
        }

        // Same weight, different job: the prompt is still fully inside the projection.
        assert_ne!(rt.execute(&s, b"a").unwrap(), rt.execute(&s, b"b").unwrap());
        // A verifier reproduces the executor's projection exactly — the k=2 property the quorum
        // depends on — and another network's fixture does not.
        assert_eq!(rt.execute(&s, b"a").unwrap(), DevnetFixtureRuntime::new(genesis).replay(&s, b"a").unwrap());
        assert_ne!(
            rt.execute(&s, b"a").unwrap(),
            DevnetFixtureRuntime::new(Hash64::from_bytes([0x22; 64])).replay(&s, b"a").unwrap(),
            "a fixture certificate must be meaningless on the network it was not built for"
        );

        // Any other ceiling is refused rather than repriced. A node quietly executing a 256-token
        // spec would mint 1 978 VLT for a job every other node priced at 50, and the only visible
        // symptom would be one validator's weight being wrong.
        for ceiling in [devnet_fixture::JOB_MAX_TOKENS - 1, devnet_fixture::JOB_MAX_TOKENS + 1, devnet_fixture::MAX_TOKENS] {
            let off = LlmJobSpec { max_tokens: ceiling, ..s.clone() };
            assert!(matches!(rt.execute(&off, b"a"), Err(PalwError::FixtureJobShape { .. })), "ceiling {ceiling} must be refused");
            // Including as a verifier: abstaining costs one verdict, whereas replaying a peer's
            // off-shape job at this node's own shape would refute it.
            assert!(matches!(rt.replay(&off, b"a"), Err(PalwError::FixtureJobShape { .. })));
        }
    }

    #[test]
    fn spawn_failure_is_reported_not_panicked() {
        let rt = PalwWorkerRuntime::new(PalwWorkerConfig {
            worker_bin: PathBuf::from("/nonexistent/palw-worker"),
            work_dir: std::env::temp_dir(),
            timeout: Duration::from_secs(1),
        });
        assert!(matches!(rt.probe(), Err(PalwError::Spawn { .. })));
    }

    /// A worker that never returns must be killed, not waited on forever. An absent verifier only
    /// fails to confirm the job in front of it; a wedged one stops auditing every later job too,
    /// and under refutation-dominant acceptance that silently starves honest executors of credit.
    #[test]
    #[cfg(unix)]
    fn a_hung_worker_is_killed_at_the_timeout() {
        use std::os::unix::fs::PermissionsExt;

        // A script rather than `/bin/sleep`, because `probe` passes its own fixed arguments and
        // sleep would reject them and exit at once — testing nothing.
        let script = std::env::temp_dir().join("misaka-palw-hung-worker-test.sh");
        std::fs::write(&script, "#!/bin/sh\nsleep 300\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let rt =
            PalwWorkerRuntime::new(PalwWorkerConfig { worker_bin: script, work_dir: std::env::temp_dir(), timeout: TEST_TIMEOUT });
        let started = std::time::Instant::now();
        assert!(matches!(rt.probe(), Err(PalwError::Timeout(_))));
        assert!(started.elapsed() < Duration::from_secs(30), "the ceiling must fire long before the worker would have returned");
    }

    #[cfg(unix)]
    const TEST_TIMEOUT: Duration = Duration::from_millis(500);
}
