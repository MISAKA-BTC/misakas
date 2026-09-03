//! **The family workers' one runtime** (ADR-0077 Decision 1, plus the worker halves of Decisions
//! 2 and 6).
//!
//! Before this module the tree held two products that never met. `misaka-palw-serve` was a
//! practical local LLM — the A16 engine speaking OpenAI at the artifact's full rotary span — and
//! its own header said a run served there was "NOT a claim anyone can adjudicate or mine". The
//! family workers mined with a prompt, and mapped the artifact ONCE PER JOB: `load()` inside
//! `run_job`, about eight minutes for the 33 GiB hybrid, per request. R0 refuses that split: there
//! is one runtime, every answer it gives is captured and committed by the same run, and the only
//! reasons a commitment does not reach the chain are the chain's.
//!
//! So the two family binaries keep only what is theirs — how their artifact and tokenizer are
//! opened, and which catalog row they embody — and everything after that lives here:
//!
//! ```text
//!   palw-a16-fp-worker  ─┐                       ┌─ v3-manifest : the identity, as JSON
//!                        ├─ FpWorkerRuntime ─────┼─ v3-job      : one frame in, one result out
//!  palw-qwen36-fp-worker ─┘   (mapped ONCE)      └─ v3-serve    : the manifest, then a loop
//! ```
//!
//! # `v3-serve`, and what stays up
//!
//! The artifact is mapped once, [`PalwFpWorkerFrameV1::Manifest`] is the first frame out, and then
//! every job is one framed [`PalwFpWorkerRequestV3`] in and a run of frames out. **A request the
//! worker will not run is answered with a `Refused` frame and the worker STAYS UP** — the contract's
//! own sentence is that one bad job must not drop a resident artifact, and a resident 33 GiB
//! mapping that a malformed request can kill is a resident mapping in name only.
//!
//! The frame grammar this module writes, stated once because it is slightly wider than the
//! contract's headline: per accepted request, **zero or more `Token` frames and then exactly one
//! terminator, `Result` or `Refused`.** A request refused before execution emits the terminator
//! alone, which is the contract's sentence verbatim. A request that failed AFTER tokens were
//! streamed — retention could not be written, say — also ends in `Refused`, because the
//! alternative is a truncated job the reader cannot tell from a slow one; the gateway writes no
//! commitment without a `Result` either way (Decision 2), so a terminator that says "no" is
//! strictly more information than silence.
//!
//! **One generation at a time.** A single engine and a single KV cache: concurrent decodes would
//! interleave in the cache and produce two wrong answers. That was `misaka-palw-serve`'s rule and
//! Decision 1 keeps it — here it is the loop's own shape, one request read, run and answered
//! before the next is read, rather than a lock somebody could take twice.
//!
//! `v3-job` is unchanged in every observable way (the drills and the replay arm use it): one frame
//! in, one bare [`PalwFpWorkerResultV3`] frame out, process exit, and the cheap request checks
//! still run BEFORE the artifact is mapped so a malformed request still fails in milliseconds
//! rather than after a map. What Decision 1 pins is that the two modes are the same code from the
//! request onward: a job's roots through `v3-serve` are byte-identical to the same job's roots
//! through `v3-job`.
//!
//! # Why the framing reader is local
//!
//! [`kaspa_consensus_core::palw_v2::read_framed`] is the wire this module speaks, and it is used
//! verbatim for `v3-job`. It cannot be used for the serve loop: after the payload it probes for
//! one more byte and returns `TrailingBytes` if it finds one, because it was written for a
//! one-frame-per-process contract. On a persistent stream that probe either consumes the next
//! request's length prefix or blocks until the gateway sends one, so a resident worker built on it
//! deadlocks on its second job. [`read_framed_stream`] is the same four-byte little-endian length
//! prefix and the same [`PALW_V2_MAX_FRAME_BYTES`] ceiling with the end-of-stream assertion
//! removed — identical bytes on the wire, and a frozen test compares the two readers on one frame
//! so they cannot drift. Every frame is FLUSHED as it is written: the worker's stdout is a
//! `LineWriter` and these frames are binary, so an unflushed manifest or token would sit in a 1 KiB
//! buffer waiting for a newline that a length prefix will not reliably contain — a resident
//! handshake that deadlocks against a gateway waiting to read it.
//!
//! # What a message may say (ADR-0079 SA-7)
//!
//! **No refusal and no log line in this module carries prompt text or a prompt id.** A refusal
//! names the rule and the POSITION it was broken at — "segment 3 declares a control id outside
//! this model's vocab" — because the gateway built the prompt and holds what it put there, while
//! the worker's stderr is a file an operator tails and a stranger's prompt must not be in it.
//! [`crate::tokenizer::TokenizerError::kind`] exists for the same reason: its `Display` carries
//! the piece that failed, which is a fragment of the text.
//!
//! # What the runtime re-verifies (ADR-0077 SA-6, ADR-0079 Decision 9 / SA-5)
//!
//! A one-shot worker verified its artifact by reading it. A resident one maps once and then serves
//! for days, so [`MappedArtifactV1`] carries the digest computed from a FULL READ at map time and
//! re-verifies before a job whenever the file's identity — device, inode, size — has changed.
//! Metadata never establishes identity (Decision 9 names `(path, size, mtime)` caching as a defect
//! class); it decides only when the read is paid for again. A file that no longer matches is a
//! `Refused` and the worker stays up: refusing is the answer, a crash is not.

use kaspa_consensus_core::palw_backend::{PalwExecutionBackendV1, PalwFpRunV1};
use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_job_context_v3};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_CANONICAL, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION,
    PALW_FP_WORKER_MANIFEST_V1_VERSION, PalwFpPromptSegmentV1, PalwFpWorkerFrameV1, PalwFpWorkerInputV3, PalwFpWorkerManifestV1,
    PalwFpWorkerRequestV3, PalwFpWorkerResultV3, PalwFreePromptJobV3, fp_job_id_v3, fp_worker_request_hash_v3,
};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::{PALW_V2_MAX_FRAME_BYTES, prompt_token_ids_hash_v2, read_framed, write_framed};
use kaspa_hashes::Hash64;
use std::io::{Read, Write};
use std::path::Path;

use crate::tokenizer::QwenTokenizer;

/// Lowercase hex of a hash, the form every worker log and retention manifest already uses.
pub fn hex(h: Hash64) -> String {
    faster_hex::hex_string(h.as_byte_slice())
}

/// **The control tokens at which a Qwen-family generation ENDS, by name.**
///
/// By name and not by id: 151645 is `<|im_end|>` in one checkpoint and something else in the next,
/// and a worker that published a guessed id would tell a gateway to stop displaying in the middle
/// of an answer. The names are resolved against the loaded tokenizer's own added-token table and a
/// name the table does not carry is simply absent from the manifest.
///
/// Both families are Qwen chat models and share this pair; a family whose generation ends
/// elsewhere passes its own list to [`FpWorkerFamilyV1`].
pub const QWEN_EOG_TOKEN_NAMES: &[&str] = &["<|im_end|>", "<|endoftext|>"];

// ---------------------------------------------------------------------------------------------
// SA-6: the artifact a resident runtime is still serving.
// ---------------------------------------------------------------------------------------------

/// How much of the artifact is read at a time when its digest is computed. 8 MiB: large enough
/// that a 33 GiB pass runs at the device's sequential rate rather than the fault rate (`mmap.rs`
/// measured 6 MB/s through a mapping against 1.3 GB/s through reads this size), small enough that
/// the buffer is not a footprint anyone notices.
const ARTIFACT_DIGEST_CHUNK: usize = 8 << 20;

/// **The file identity SA-6 names**: device, inode and size.
///
/// Not an identity for the artifact — Decision 9 forbids that, and the digest below is the
/// identity. This is the trigger: the cheap fact that says whether the expensive check must be
/// paid again. `(size, mtime)` off POSIX for the same purpose and stated as such, because the
/// platform has no inode to offer and a trigger that is merely conservative is still a trigger.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FileIdentityV1 {
    device: u64,
    inode: u64,
    size: u64,
    /// Zero on POSIX, where device and inode carry the identity; the mtime off it.
    fallback_mtime_ns: u128,
}

impl FileIdentityV1 {
    fn read(path: &Path) -> Result<Self, String> {
        let meta = std::fs::metadata(path).map_err(|e| format!("cannot stat the artifact {}: {e}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(Self { device: meta.dev(), inode: meta.ino(), size: meta.len(), fallback_mtime_ns: 0 })
        }
        #[cfg(not(unix))]
        {
            let mtime =
                meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
            Ok(Self { device: 0, inode: 0, size: meta.len(), fallback_mtime_ns: mtime })
        }
    }
}

/// **An artifact this runtime opened, and the digest it was opened under** (ADR-0077 SA-6).
///
/// Held for the life of a resident process. [`Self::revalidate`] runs before every job: it stats
/// the file, and only when the identity has moved does it pay for the full read again. Three
/// outcomes, and none of them is a crash:
///
/// * identity unchanged — nothing to do, and this is the ordinary case;
/// * identity changed, digest still equal — a rewrite of the same bytes (an `rsync`, a re-link).
///   The new identity is recorded and the job runs;
/// * identity changed, digest different or the file unreadable — the artifact on disk is no
///   longer the one this process verified, so the job is refused by name. For a MAPPED family
///   this is also the one check that reaches the fault SA-6 names before it happens: a truncated
///   file is a shrunken `size`, and touching a mapped page past the new end of file is a `SIGBUS`.
pub struct MappedArtifactV1 {
    path: std::path::PathBuf,
    /// Computed by reading every byte at map time. Decision 9: the full read IS the check, and no
    /// artifact identity may ever be derived from metadata.
    digest: [u8; 32],
    /// The last identity this digest was observed under. Behind a lock so a `&self` job path can
    /// record a re-verification instead of paying for it again on the next job; contention is
    /// impossible in a loop that runs one generation at a time.
    seen: std::sync::Mutex<FileIdentityV1>,
}

impl MappedArtifactV1 {
    /// Verify by reading the file — the hybrid's form, whose artifact is mapped and never held.
    ///
    /// The identity is taken before AND after the read and must agree: a file rewritten while it
    /// was being hashed would otherwise be recorded under a digest of a mixture of two versions.
    pub fn verify_by_reading(path: &Path) -> Result<Self, String> {
        let before = FileIdentityV1::read(path)?;
        let digest = digest_file_v1(path)?;
        let after = FileIdentityV1::read(path)?;
        if before != after {
            return Err(format!("the artifact {} changed while it was being verified", path.display()));
        }
        Ok(Self { path: path.to_path_buf(), digest, seen: std::sync::Mutex::new(after) })
    }

    /// Verify from bytes the caller has already read — the dense tier's form, which reads its
    /// artifact into memory to decode it and must not read it a second time to hash it.
    ///
    /// The identity is taken AFTER the caller's read for the same reason as above: it is the
    /// identity those bytes were observed under, as far as this process can tell.
    pub fn verify_from_bytes(path: &Path, bytes: &[u8]) -> Result<Self, String> {
        let digest = digest_bytes_v1(bytes);
        let seen = FileIdentityV1::read(path)?;
        if seen.size != bytes.len() as u64 {
            return Err(format!(
                "the artifact {} is {} bytes on disk and {} were read: it was rewritten during startup",
                path.display(),
                seen.size,
                bytes.len()
            ));
        }
        Ok(Self { path: path.to_path_buf(), digest, seen: std::sync::Mutex::new(seen) })
    }

    /// The digest every job runs under, for the boot line and for a report.
    pub fn digest_hex(&self) -> String {
        faster_hex::hex_string(&self.digest)
    }

    /// **Before every job.** See the struct note for the three outcomes.
    pub fn revalidate(&self) -> Result<(), String> {
        let now = FileIdentityV1::read(&self.path)?;
        {
            let seen = self.seen.lock().map_err(|_| "the artifact identity lock is poisoned".to_string())?;
            if *seen == now {
                return Ok(());
            }
        }
        let digest = digest_file_v1(&self.path)?;
        if digest != self.digest {
            return Err(format!(
                "the artifact {} is no longer the file this runtime verified (mapped {}, on disk now {}); every root this \
                 process committed hangs off the bytes it mapped, so a job against different bytes is refused rather than run",
                self.path.display(),
                self.digest_hex(),
                faster_hex::hex_string(&digest)
            ));
        }
        if let Ok(mut seen) = self.seen.lock() {
            *seen = now;
        }
        Ok(())
    }
}

/// The artifact digest: BLAKE2b-256 over every byte of the file.
///
/// Local to the host and never a consensus object — the chain's artifact root is the class's own
/// derivation — so the only requirements are that it is strong and that it is over the WHOLE file.
pub fn digest_bytes_v1(bytes: &[u8]) -> [u8; 32] {
    let hash = blake2b_simd::Params::new().hash_length(32).key(b"misaka-palw/worker-artifact/v1").hash(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_bytes());
    out
}

fn digest_file_v1(path: &Path) -> Result<[u8; 32], String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("cannot open the artifact {}: {e}", path.display()))?;
    let mut state = blake2b_simd::Params::new().hash_length(32).key(b"misaka-palw/worker-artifact/v1").to_state();
    let mut buf = vec![0u8; ARTIFACT_DIGEST_CHUNK];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                state.update(&buf[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            // An I/O error mid-read is exactly the fault SA-6 is about, and it is returned:
            // a caller turns it into a refusal, never a panic.
            Err(e) => return Err(format!("reading the artifact {}: {e}", path.display())),
        }
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(state.finalize().as_bytes());
    Ok(out)
}

/// **What only the family binary knows** — everything else in the manifest is derived from the
/// class's registered profile.
///
/// `runtime_identity` is the one value whose SOURCE differs between the two families: the dense
/// tier's runtime is its artifact (`artifact_digest()`), the hybrid's is the shape its backend
/// serves (`shape_id()`). Both binaries already fill `model_profile_id` and `runtime_class_id`
/// with that same value, so it is one field here rather than two that could disagree.
/// **The court parameters the RULESET of `network_id` froze** (ADR-0082 Decision 8; W1b
/// `bb4f145b` on the executor side).
///
/// A worker prices a job against a ladder — `step_leaf_count_capped_v1` is what decides how many
/// tokens a user actually gets — and both binaries built that ladder out of
/// `PALW_STEP_MAX_LEAVES`, the module DEFAULT. A node whose ruleset moved the ladder would serve
/// a row its own workers refuse, or (worse, and the direction that costs a producer money) a row
/// they execute past what the court can adjudicate.
///
/// The ruleset a worker can know is the one its network's binary ships: `Params::from(NetworkId)`
/// is exactly what kaspad boots with, and the bundle's `court` is the same object
/// `PalwCourtParamsV2` the class catalog and the step leg read. A network with PALW disabled has
/// no ladder at all, and this says so rather than substituting a constant — the mistake this
/// worker already made once with its network id, where the wrong default was silent.
///
/// It is not the LIVE ruleset: an activation moves the bundle and a worker cannot read chain
/// state. That gap is named here rather than papered over; closing it means the gateway telling
/// the worker, which is a change to `PalwFpWorkerRequestV3` and therefore a consensus-core change
/// this stream does not make (see the report's patch notes).
pub fn fp_worker_court_params_v1(network_id: &str) -> Result<kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2, String> {
    use kaspa_consensus_core::config::params::Params;
    use kaspa_consensus_core::network::NetworkId;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    let net: NetworkId = network_id.parse().map_err(|e| {
        format!("{network_id} is not a network this build knows ({e}); it must be the string kaspad prints for params.net")
    })?;
    let params = Params::from(net);
    match &params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => Ok(bundle.court.clone()),
        _ => Err(format!(
            "{network_id} ships with PALW off, so it freezes no court and no step ladder — there is nothing for this worker to \
             price a job against"
        )),
    }
}

pub struct FpWorkerFamilyV1 {
    /// The catalog row this worker embodies, e.g. `Qwen/Qwen2.5-1.5B/graph-v2`.
    pub model_id: String,
    /// The family's runtime identity hash — see the struct note.
    pub runtime_identity: Hash64,
    /// What the job's `tokenizer_id` is bound to, and it must be the answer
    /// [`crate::artifact::Base0ArtifactV1::check_tokenizer_bytes_v1`] gave for the file this
    /// runtime actually opened — never a field copied out of the artifact unchecked. The dense
    /// tier's artifact format carries a commitment; the hybrid's `.palwq36` deliberately carries
    /// none (PALW binds a prompt by the hash of its ids), so it is `Hash64::default()` there.
    ///
    /// **`Hash64::default()` here means the pair was not checked**, whichever tier it came from,
    /// and nothing on chain rejects a wrong tokenizer for such a job: a replay under a different
    /// file produces different ids and a claim that reproduces nothing. Every dense artifact
    /// written before `qwen25-convert` bound one on its `--a16` lane is in that state.
    pub tokenizer_id: Hash64,
    /// The id ceiling a prompt token is checked against — the model's, not the tokenizer's, which
    /// is padded differently.
    pub vocab: u32,
    /// `schema` of the per-job retention manifest, e.g. `misaka.palw.fp-v3-a16-retention.v1`.
    pub retention_schema: &'static str,
    /// The `family` field of that manifest, e.g. `qwen25-a16`.
    pub retention_family: &'static str,
    /// The names whose ids become [`PalwFpWorkerManifestV1::eog_token_ids`].
    pub eog_token_names: &'static [&'static str],
    /// **The artifact this runtime opened, verified** (ADR-0077 SA-6). `None` only where there is
    /// no file to verify — the derived fixture class, whose weights are computed from a seed and
    /// whose "artifact" never existed on disk. Every binary that opens one passes it.
    pub artifact: Option<MappedArtifactV1>,
}

/// **One mapped artifact, and everything derived from it once.**
///
/// Held for the life of the process in `v3-serve` and for the life of one job in `v3-job`. The
/// manifest is built at construction rather than per request precisely so that the identity a
/// worker announces and the identity it checks a request against cannot come apart.
pub struct FpWorkerRuntime<B: PalwExecutionBackendV1> {
    backend: B,
    /// The bytes this worker stamps into every job context. Every committed root hangs off a
    /// context hash that absorbs it, and a seat replaying the claim derives that hash from its
    /// node's own network name — so this is the OPERATOR's value, never a constant.
    network_id: Vec<u8>,
    tokenizer: QwenTokenizer,
    manifest: PalwFpWorkerManifestV1,
    retention_schema: &'static str,
    retention_family: &'static str,
    /// SA-6: re-verified before every job. See [`MappedArtifactV1`].
    artifact: Option<MappedArtifactV1>,
    /// How long the artifact took to open AND verify, reported on every result. In `v3-serve`
    /// this is the ONE map's cost and it does not recur — which is the measurable half of
    /// Decision 1, and why the SA-6 full read is affordable here and was not affordable per job.
    load_ms: u64,
}

impl<B: PalwExecutionBackendV1> FpWorkerRuntime<B> {
    /// Assemble the runtime, deriving the manifest from the class's registered profile.
    ///
    /// Fails closed and by name on the two things a family can get wrong at construction: a
    /// profile whose logits commitment is not the one this path adjudicates, and a tokenizer in
    /// which none of the family's end-of-generation names exists. Both would otherwise surface as
    /// a wrong answer much later — a close nobody can adjudicate, or a gateway that never stops
    /// displaying.
    pub fn new(
        backend: B,
        profile: &PalwShapeProfileV3,
        tokenizer: QwenTokenizer,
        family: FpWorkerFamilyV1,
        network_id: Vec<u8>,
        load_ms: u64,
    ) -> Result<Self, String> {
        if network_id.is_empty() {
            return Err("the network id is empty: every committed root hangs off a context hash that absorbs it, and a seat \
                        derives that hash from its node's own network name"
                .to_string());
        }
        // Derived from the class's own registration, not written down here: the scheme decides
        // which close arm can adjudicate the class and which recomputation a seat runs, and it
        // sits inside `shape_profile_id`. Announcing a scheme the profile does not declare would
        // let a gateway build requests for a court arm this class cannot reach.
        let trace_scheme_id = profile.logits_scheme_id;
        if trace_scheme_id != kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1() {
            return Err(format!(
                "{} registers the logits scheme {} — the free-prompt worker path commits and opens the TILED scheme, and a \
                 class whose close arm this runtime cannot assemble must not be served here",
                family.model_id,
                hex(trace_scheme_id)
            ));
        }

        let special_tokens: Vec<(String, u32)> = tokenizer.added_tokens().iter().map(|a| (a.content.clone(), a.id)).collect();
        let eog_token_ids: Vec<u32> = family.eog_token_names.iter().filter_map(|name| tokenizer.added_id(name)).collect();
        if eog_token_ids.is_empty() {
            return Err(format!(
                "this tokenizer names none of {:?}, so the manifest could publish no end-of-generation id and every answer \
                 would be displayed to the job's full decode budget",
                family.eog_token_names
            ));
        }

        let manifest = PalwFpWorkerManifestV1 {
            version: PALW_FP_WORKER_MANIFEST_V1_VERSION,
            model_id: family.model_id,
            class_id: profile.shape_profile_id(),
            model_profile_id: family.runtime_identity,
            // Zero on both families today: neither runtime ships a manifest of its own binary, and
            // the request pins it so the two sides agree about that rather than about nothing.
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: family.runtime_identity,
            shape_profile_id: profile.shape_profile_id(),
            trace_scheme_id,
            tokenizer_id: family.tokenizer_id,
            // **The CLASS's registered width, read from the catalog row's profile — never the
            // artifact's rotary span.** The dense artifact's table covers 512 positions and the
            // class registers 16; a runtime that answered at 512 would be answering wider than
            // the court admits, which is exactly the two-products split R0 exists to close. The
            // width becomes practical by moving the class table (Phase B), not by widening the
            // runtime.
            n_ctx: profile.n_ctx,
            prefill_single_batch_cap: profile.n_ctx,
            vocab: family.vocab,
            special_tokens,
            eog_token_ids,
        };
        Ok(Self {
            backend,
            network_id,
            tokenizer,
            manifest,
            retention_schema: family.retention_schema,
            retention_family: family.retention_family,
            artifact: family.artifact,
            load_ms,
        })
    }

    pub fn manifest(&self) -> &PalwFpWorkerManifestV1 {
        &self.manifest
    }

    pub fn tokenizer(&self) -> &QwenTokenizer {
        &self.tokenizer
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// How long the ONE mapping and its verification took. Reported on every result as
    /// `model_load_ms`, and printed at startup by the resident mode because it is the cost
    /// Decision 1 stops paying per job.
    pub fn load_ms(&self) -> u64 {
        self.load_ms
    }

    /// The verified artifact, for the boot line. `None` for the derived fixture class.
    pub fn artifact(&self) -> Option<&MappedArtifactV1> {
        self.artifact.as_ref()
    }
}

/// **The `v3-manifest` document, unchanged.**
///
/// The one-shot mode predates the serve handshake and `misaka-palw-gateway` reads it key by key,
/// so every key it read before is still here and spelled the same. The three added keys are the
/// ones Decision 6 needs on the one-shot path — a gateway cannot build a segment-wise prompt
/// without the special-token table, and cannot stop displaying without the EOG ids — and a reader
/// that does not know them ignores them.
pub fn manifest_json_v1(manifest: &PalwFpWorkerManifestV1) -> serde_json::Value {
    serde_json::json!({
        "schema": "misaka.palw.fp-v3-manifest.v1",
        "runtime_manifest_hash": hex(manifest.runtime_manifest_hash),
        "runtime_class_id": hex(manifest.runtime_class_id),
        "model_profile_id": hex(manifest.model_profile_id),
        "shape_profile_id": hex(manifest.shape_profile_id),
        "trace_scheme_id": hex(manifest.trace_scheme_id),
        "tokenizer_id": hex(manifest.tokenizer_id),
        "n_ctx": manifest.n_ctx,
        "prefill_single_batch_cap": manifest.prefill_single_batch_cap,
        "shape_string": manifest.model_id.clone(),
        "vocab": manifest.vocab,
        "special_tokens": manifest
            .special_tokens
            .iter()
            .map(|(name, id)| serde_json::json!({ "name": name.clone(), "id": *id }))
            .collect::<Vec<_>>(),
        "eog_token_ids": manifest.eog_token_ids.clone(),
    })
}

// ---------------------------------------------------------------------------------------------
// The request, checked twice: the cheap half before the artifact is mapped, the whole of it
// against the mapped runtime's own manifest.
// ---------------------------------------------------------------------------------------------

/// **The checks that need no artifact.** Run before `load()` on the one-shot path so a malformed
/// request still fails in milliseconds rather than after an eight-minute map, and run again inside
/// [`run_one_job_v1`] so the resident path cannot skip them.
pub fn precheck_request_v1(request: &PalwFpWorkerRequestV3) -> Result<(), String> {
    if request.version != PALW_FP_V3_VERSION {
        return Err(format!("request version {} is not {}", request.version, PALW_FP_V3_VERSION));
    }
    if request.privacy_mode != PALW_FP_PRIVACY_PUBLIC_DA {
        return Err(format!(
            "privacy mode {} is not PublicDa — a mode the panel cannot replay must not execute",
            request.privacy_mode
        ));
    }
    if request.decode_token_limit == 0 {
        return Err("a zero decode ceiling is not a job".to_string());
    }
    // Copied verbatim onto the job, so a mode the derivation refuses would be discovered after the
    // whole execution — `palw_fp_job_context_v3` checks it, and by then the run is paid for. The
    // same check, before the artifact is touched.
    if request.prompt_mode != PALW_FP_PROMPT_MODE_USER && request.prompt_mode != PALW_FP_PROMPT_MODE_CANONICAL {
        return Err(format!("prompt mode {} is neither User nor Canonical", request.prompt_mode));
    }
    Ok(())
}

/// The identity cross-check: a request that declares a different runtime or class is somebody
/// else's job, and executing it here would commit this artifact's arithmetic under that other
/// identity — a claim no seat can reproduce and a producer defaulted for honest work.
fn check_identity_v1(manifest: &PalwFpWorkerManifestV1, request: &PalwFpWorkerRequestV3) -> Result<(), String> {
    let pin = |field: &str, ours: Hash64, theirs: Hash64| -> Result<(), String> {
        if ours != theirs {
            return Err(format!(
                "{field} mismatch — the request declares a runtime this worker is not (ours {}, request {})",
                hex(ours),
                hex(theirs)
            ));
        }
        Ok(())
    };
    pin("class_id", manifest.class_id, request.class_id)?;
    pin("shape_profile_id", manifest.shape_profile_id, request.shape_profile_id)?;
    pin("model_profile_id", manifest.model_profile_id, request.model_profile_id)?;
    pin("runtime_class_id", manifest.runtime_class_id, request.runtime_class_id)?;
    pin("runtime_manifest_hash", manifest.runtime_manifest_hash, request.runtime_manifest_hash)?;
    pin("trace_scheme_id", manifest.trace_scheme_id, request.trace_scheme_id)?;
    Ok(())
}

/// **Decision 6: the prompt's ids, by arm.**
///
/// `Text` is the v1 plain-marker template — tokenized through [`QwenTokenizer::encode`], whose
/// added-token matching is what that template depends on. `TokenIds` is the replay arm and echoes.
/// `Segments` is the arm Decision 6 adds: a `Special` is emitted as that id VERBATIM, and a `Text`
/// segment is encoded with special-token parsing DISABLED and concatenated as ids, so untrusted
/// text can never smuggle a control token and the model still sees the template it was trained on
/// — which is why EOG fires and an answer ends where it ends instead of at the ceiling.
///
/// **Every refusal here names a position and never a value** (ADR-0079 SA-7): the input is a
/// stranger's prompt, the refusal is logged, and the gateway that built the prompt already holds
/// what it put at each index.
pub fn prompt_ids_for_input_v1(
    tokenizer: &QwenTokenizer,
    manifest: &PalwFpWorkerManifestV1,
    input: &PalwFpWorkerInputV3,
) -> Result<Vec<u32>, String> {
    let ids = match input {
        PalwFpWorkerInputV3::Text(bytes) => {
            if bytes.is_empty() {
                return Err("the text arm carries no bytes".to_string());
            }
            let text = std::str::from_utf8(bytes)
                .map_err(|_| "the text arm is not UTF-8 — a template renders text, not bytes".to_string())?;
            // ADR-0079 Decision 7 / S7 (and ADR-0077 Decision 6): the Text arm is a stranger's bytes,
            // so added tokens are NOT matched — a user's `<|im_start|>` is ordinary text. A template
            // that means to emit a control token says so through the Segments arm's `Special(id)`.
            tokenizer.encode_without_specials(text).map_err(|e| format!("the text arm did not tokenize: {}", e.kind()))?
        }
        PalwFpWorkerInputV3::TokenIds(ids) => {
            if ids.is_empty() {
                return Err("the ids arm carries no tokens".to_string());
            }
            ids.clone()
        }
        PalwFpWorkerInputV3::Segments(segments) => {
            if segments.is_empty() {
                return Err("the segments arm carries no segments".to_string());
            }
            let mut out = Vec::new();
            for (at, segment) in segments.iter().enumerate() {
                match segment {
                    // **A `Special` must NAME a control token, not merely be a number the model
                    // could produce.** The manifest publishes the table for exactly this reason,
                    // and a gateway that put an ordinary id here is a gateway whose template is
                    // not the model's — a prompt nobody wrote, executed and committed as if
                    // somebody had. Checked here rather than at the vocab sweep below so the
                    // refusal names the SEGMENT, and it names only the segment: the id is a prompt
                    // token and this message is logged (ADR-0079 SA-7).
                    PalwFpPromptSegmentV1::Special(id) => {
                        if *id >= manifest.vocab || !tokenizer.is_added_id(*id) {
                            return Err(format!(
                                "segment {at} declares an id this tokenizer does not hold as a control token; a gateway builds \
                                 Special from a NAME in the manifest's special_tokens, never from a guessed id"
                            ));
                        }
                        out.push(*id);
                    }
                    PalwFpPromptSegmentV1::Text(bytes) => {
                        if bytes.is_empty() {
                            return Err(format!("segment {at} is an empty text segment"));
                        }
                        let text = std::str::from_utf8(bytes)
                            .map_err(|_| format!("segment {at} is not UTF-8 — a template carries text, not bytes"))?;
                        let mut piece = tokenizer
                            .encode_without_specials(text)
                            .map_err(|e| format!("segment {at} could not be encoded with specials disabled: {}", e.kind()))?;
                        out.append(&mut piece);
                    }
                }
            }
            out
        }
    };
    if ids.is_empty() {
        return Err("the prompt encoded to nothing".to_string());
    }
    Ok(ids)
}

/// **One job, from a validated request to the result frame's contents.**
///
/// The whole of what `v3-job` and `v3-serve` share; that they share it is what makes W6 —
/// byte-identical roots through both modes — a property of the code rather than of a habit.
///
/// `on_token` is called once per generated id, in decode order, as soon as it is SELECTED
/// (Decision 2), with that id and the bytes of the answer that became renderable with it. Every
/// generated id is reported, INCLUDING an end-of-generation id and anything after it: the
/// execution runs to the job's declared decode budget because a step leaf hash binds the job
/// context, which binds the executed count, so hashing cannot begin before the count is fixed. EOG
/// is a DISPLAY stop and the manifest publishes the ids so the gateway can honour it — a worker
/// that stopped executing there would commit a count the court's ladder was not sized for.
pub fn run_one_job_v1<B: PalwExecutionBackendV1>(
    rt: &FpWorkerRuntime<B>,
    request: &PalwFpWorkerRequestV3,
    request_hash: Hash64,
    trace_out: &Path,
    on_token: &mut dyn FnMut(u32, &[u8]),
) -> Result<PalwFpWorkerResultV3, String> {
    precheck_request_v1(request)?;
    check_identity_v1(&rt.manifest, request)?;

    if request.max_context_tokens == 0 || request.max_context_tokens > rt.manifest.n_ctx {
        return Err(format!("max_context_tokens {} is outside this class's 1..={}", request.max_context_tokens, rt.manifest.n_ctx));
    }

    let prompt_ids = prompt_ids_for_input_v1(&rt.tokenizer, &rt.manifest, &request.input)?;
    // The position and not the id (SA-7): the value is a prompt token and this message is logged.
    if let Some(at) = prompt_ids.iter().position(|t| *t >= rt.manifest.vocab) {
        return Err(format!("the prompt token at position {at} is outside the model's vocab ({})", rt.manifest.vocab));
    }
    let prefill = prompt_ids.len() as u32;
    if prefill as u64 + request.decode_token_limit as u64 > request.max_context_tokens as u64 {
        return Err(format!(
            "prompt {prefill} + decode ceiling {} exceeds max_context_tokens {}",
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
        tokenizer_id: rt.manifest.tokenizer_id,
        prompt_token_ids_hash: prompt_token_ids_hash_v2(&prompt_ids),
        prompt_tokens: prefill,
        decode_token_limit: request.decode_token_limit,
        max_context_tokens: request.max_context_tokens,
        privacy_mode: request.privacy_mode,
        prompt_mode: request.prompt_mode,
    };
    let binding = fp_job_id_v3(&job);

    // **SA-6, immediately before the first page is touched.** Last of the checks because it is
    // the only one that can cost a read, and first of the things that happen to the artifact: a
    // file that is no longer the one this process verified must be refused before a mapped page
    // beyond a truncated end of file faults.
    if let Some(artifact) = &rt.artifact {
        artifact.revalidate()?;
    }

    let exec_started = std::time::Instant::now();
    let prompt_usize: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
    let (run, streamed) = execute_streaming_v1(rt, &job, &prompt_usize, on_token)?;
    let execute_ms = exec_started.elapsed().as_millis() as u64;

    // **F1 through the stream, on the worker's own side** (Decision 2's failure mode, closed
    // before the frames leave the process): a worker that showed one answer and committed another
    // would be a worker whose commitment is not the user's inference. The gateway re-renders the
    // committed ids and compares; this refuses to hand it the mismatch in the first place.
    if streamed != run.output_token_ids {
        return Err(format!(
            "the streamed ids are not the committed ids ({} streamed, {} committed) — one inference must be one answer and \
             one commitment",
            streamed.len(),
            run.output_token_ids.len()
        ));
    }
    // The same rule against the family's own facts: the count a step leaf binds is
    // `decode_tokens_executed`, so a backend that committed a different number of tokens than it
    // produced would price a run nobody can replay. Cheap, and it is the sentence Decision 1
    // makes checkable now that one code path serves both modes.
    if run.output_token_ids.len() as u64 != run.facts.decode_tokens_executed as u64 {
        return Err(format!(
            "the backend committed {} executed tokens and produced {} ids",
            run.facts.decode_tokens_executed,
            run.output_token_ids.len()
        ));
    }

    // The schedule root, derived exactly as `palw_fp_commitment_v3` derives it, so the gateway's
    // `to_commitment` equals the canonical assembly byte for byte.
    let class_facts = PalwFpClassFactsV3 {
        model_profile_id: rt.manifest.model_profile_id,
        runtime_manifest_hash: rt.manifest.runtime_manifest_hash,
        runtime_class_id: rt.manifest.runtime_class_id,
        shape_profile_id: rt.manifest.shape_profile_id,
        cu_ruleset_id: Hash64::default(),
    };
    let context = palw_fp_job_context_v3(&job, &class_facts, &run.facts, &rt.network_id)
        .map_err(|e| format!("the finished run implies no context: {e:?}"))?;
    let (schedule_root, _calls) = kaspa_consensus_core::palw_v2::expected_schedule_commitment_v2(
        &context.context_hash(),
        job.prompt_tokens,
        run.facts.decode_tokens_executed,
    );

    retain_v1(rt, trace_out, binding, &run, context.context_hash())?;

    let rendered = render_answer_v1(&rt.tokenizer, &run.output_token_ids);

    eprintln!(
        "[{}] v3 executed: prefill={prefill} decode={}/{} in {execute_ms}ms ({} leaves); exec root={}…",
        rt.retention_family,
        run.facts.decode_tokens_executed,
        job.decode_token_limit,
        run.facts.step_leaf_count,
        &hex(run.outcome.execution_root)[..16]
    );

    Ok(PalwFpWorkerResultV3 {
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
        // The run's own fact, not a constant. It is `ExactBudgetReached` on both shipped families
        // because execution runs to the declared budget and EOG is a display stop — but that is
        // the family's statement about its own capture, and the value the context was built
        // under, so it is read rather than repeated here.
        stop_reason: run.facts.stop_reason,
        output_token_ids: run.output_token_ids,
        rendered,
        model_load_ms: rt.load_ms,
        execute_ms,
    })
}

/// **The answer's bytes: every token's bytes, concatenated** (ADR-0077 Decision 2).
///
/// Never fails, and that is the point. [`QwenTokenizer::decode`] refuses a whole run for one id it
/// cannot spell, and two ids it cannot spell are ORDINARY: a class registers a padded `vocab_size`
/// and the engine's argmax may select in the padding, and a multi-byte character straddling the
/// last token leaves the run ending mid-sequence. Under `decode` either one turned a completed,
/// captured, committed inference into a refused job — the execution done, the capture written, and
/// the result discarded over its spelling. An id with no bytes contributes none.
///
/// This is also what makes the streamed pieces and this field the same bytes by construction: the
/// sink emits `token_bytes` per id and this is their concatenation, so the gateway's Decision 2
/// comparison is an identity rather than two decoders that happen to agree.
pub fn render_answer_v1(tokenizer: &QwenTokenizer, ids: &[u32]) -> Vec<u8> {
    ids.iter().filter_map(|id| tokenizer.token_bytes(*id)).flatten().collect()
}

/// The run itself, reporting each id as it is selected with THAT id's bytes.
///
/// The frame contract's own sentence — "`rendered` is this id's rendering alone" — and the reason
/// it is right: the concatenation of the pieces is then the `rendered` field of the result by
/// construction, whatever the tokenizer's table can and cannot spell. A multi-byte character
/// straddling two tokens arrives as two pieces neither of which is valid UTF-8 on its own; that is
/// the truth about the answer, and holding a partial sequence for display is the DISPLAY's
/// business — the gateway's, which buffers before it writes an SSE event. A worker that held bytes
/// back instead would be a worker whose stream and whose result are not the same bytes.
fn execute_streaming_v1<B: PalwExecutionBackendV1>(
    rt: &FpWorkerRuntime<B>,
    job: &PalwFreePromptJobV3,
    prompt_tokens: &[usize],
    on_token: &mut dyn FnMut(u32, &[u8]),
) -> Result<(PalwFpRunV1, Vec<u32>), String> {
    let mut streamed: Vec<u32> = Vec::new();
    let run = {
        let mut sink = |id: u32| {
            streamed.push(id);
            // An id past the tokenizer's table (a class's padded vocab) renders to nothing, and
            // the id is still reported: the gateway counts tokens and a silent one would
            // desynchronise it from the ids it will be asked to check the stream against.
            on_token(id, &rt.tokenizer.token_bytes(id).unwrap_or_default());
        };
        rt.backend.execute_free_prompt_streaming(job, prompt_tokens, &mut sink).map_err(|e| format!("execution refused: {e}"))?
    };
    Ok((run, streamed))
}

/// **Retention, before the result frame exists.** The family's disclosure object is the encoded
/// run — what a seat checks and a court opens — written under the job id, and a commitment whose
/// producer kept nothing cannot serve an opening and would default in court. So a retention
/// failure is a refusal, never a warning.
fn retain_v1<B: PalwExecutionBackendV1>(
    rt: &FpWorkerRuntime<B>,
    trace_out: &Path,
    binding: Hash64,
    run: &PalwFpRunV1,
    job_context_hash: Hash64,
) -> Result<(), String> {
    let retain_dir = trace_out.join(hex(binding));
    std::fs::create_dir_all(&retain_dir).map_err(|e| format!("cannot create the retention dir {}: {e}", retain_dir.display()))?;
    let material_path = retain_dir.join("material.bin");
    std::fs::write(&material_path, &run.outcome.material).map_err(|e| format!("cannot retain {}: {e}", material_path.display()))?;
    let manifest_doc = serde_json::json!({
        "schema": rt.retention_schema,
        "trace_binding": hex(binding),
        "trace_root": hex(run.outcome.trace_root),
        "trace_manifest_root": hex(run.outcome.trace_manifest_root),
        "chunk_count": run.outcome.trace_chunk_count,
        "material_bytes": run.outcome.material.len(),
        "execution_root": hex(run.outcome.execution_root),
        // ADR-0078 X6: the one input a consumer cannot derive from the answer alone — the job's
        // context hash — so that `output_root` is recomputable from (ids, this, the family's
        // rendered-hash rule) by anyone the answer is handed to.
        "job_context_hash": hex(job_context_hash),
        "output_root": hex(run.outcome.output_root),
        "family": rt.retention_family,
    });
    let doc = serde_json::to_vec_pretty(&manifest_doc).map_err(|e| format!("cannot render the retention manifest: {e}"))?;
    std::fs::write(retain_dir.join("manifest.json"), doc).map_err(|e| format!("cannot write the retention manifest: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The three modes.
// ---------------------------------------------------------------------------------------------

/// `--mode v3-manifest`: the identity the gateway pins requests with, as one JSON line.
pub fn run_v3_manifest_v1<B: PalwExecutionBackendV1, W: Write>(rt: &FpWorkerRuntime<B>, out: &mut W) -> Result<(), String> {
    let doc = manifest_json_v1(&rt.manifest);
    writeln!(out, "{doc}").map_err(|e| format!("cannot write the manifest: {e}"))?;
    out.flush().map_err(|e| format!("cannot flush the manifest: {e}"))
}

/// `--mode v3-job`: one framed request in, one bare framed [`PalwFpWorkerResultV3`] out, and the
/// process exits.
///
/// The one-shot form the drills and the replay arm use, and unchanged in every observable way. The
/// artifact is mapped by `load` only after [`precheck_request_v1`] has passed, which is the
/// ordering the binaries had: a malformed request must not cost an eight-minute map.
pub fn run_v3_job_v1<B, R, W, L>(input: &mut R, output: &mut W, trace_out: &Path, load: L) -> Result<(), String>
where
    B: PalwExecutionBackendV1,
    R: Read,
    W: Write,
    L: FnOnce() -> FpWorkerRuntime<B>,
{
    let payload = read_framed(input, PALW_V2_MAX_FRAME_BYTES).map_err(|e| format!("v3-job rejected: {e}"))?;
    let request_hash = fp_worker_request_hash_v3(&payload);
    let request: PalwFpWorkerRequestV3 = borsh::from_slice(&payload).map_err(|e| format!("v3-job rejected: {e}"))?;
    precheck_request_v1(&request).map_err(|e| format!("v3-job rejected: {e}"))?;

    let rt = load();
    let mut ignore = |_: u32, _: &[u8]| {};
    let result = run_one_job_v1(&rt, &request, request_hash, trace_out, &mut ignore).map_err(|e| format!("v3-job rejected: {e}"))?;
    let bytes = borsh::to_vec(&result).map_err(|e| format!("cannot serialize the result: {e}"))?;
    write_framed(output, &bytes).map_err(|e| format!("cannot write the result frame: {e}"))
}

/// `--mode v3-serve`: the artifact is already mapped; announce it once, then answer jobs until the
/// stream ends.
///
/// Returns `Ok` on a clean end of stream — the gateway closing the pipe is how a resident worker
/// is meant to stop — and `Err` only for a stream this worker can no longer read or write, which
/// is the one class of failure that must not be answered with a frame.
pub fn run_v3_serve_v1<B, R, W>(rt: &FpWorkerRuntime<B>, input: &mut R, output: &mut W, trace_out: &Path) -> Result<(), String>
where
    B: PalwExecutionBackendV1,
    R: Read,
    W: Write,
{
    write_frame_v1(output, &PalwFpWorkerFrameV1::Manifest(rt.manifest.clone()))?;
    loop {
        let payload = match read_framed_stream(input, PALW_V2_MAX_FRAME_BYTES)? {
            Some(payload) => payload,
            None => return Ok(()),
        };
        let request_hash = fp_worker_request_hash_v3(&payload);
        // A frame that is not a request at all is refused like any other bad job: the artifact is
        // resident and a gateway that sent nonsense gets to try again.
        let request: PalwFpWorkerRequestV3 = match borsh::from_slice(&payload) {
            Ok(request) => request,
            Err(e) => {
                write_frame_v1(output, &PalwFpWorkerFrameV1::Refused { reason: format!("the frame is not a v3 request: {e}") })?;
                continue;
            }
        };

        // The token sink writes straight to the stream. A write failure here is a dead gateway,
        // not a bad job, so it ends the session rather than becoming a `Refused` nobody can read;
        // it is recorded and re-raised after the run, because the run itself must not be abandoned
        // half-captured.
        let mut stream_error: Option<String> = None;
        // The sink is scoped so its borrow of `output` and `stream_error` ends with the run, and
        // the terminator below writes to the same stream without a `drop` anybody could move.
        let outcome = {
            let mut on_token = |token_id: u32, rendered: &[u8]| {
                if stream_error.is_some() {
                    return;
                }
                let frame = PalwFpWorkerFrameV1::Token { token_id, rendered: rendered.to_vec() };
                if let Err(e) = write_frame_v1(output, &frame) {
                    stream_error = Some(e);
                }
            };
            run_one_job_v1(rt, &request, request_hash, trace_out, &mut on_token)
        };
        if let Some(e) = stream_error {
            return Err(e);
        }
        match outcome {
            Ok(result) => write_frame_v1(output, &PalwFpWorkerFrameV1::Result(Box::new(result)))?,
            // **The worker stays up.** One bad job must not drop a resident artifact — that is the
            // contract's sentence and the reason this mode exists at all.
            Err(reason) => {
                eprintln!("[{}] v3-serve refused a job: {reason}", rt.retention_family);
                write_frame_v1(output, &PalwFpWorkerFrameV1::Refused { reason })?;
            }
        }
    }
}

/// One [`PalwFpWorkerFrameV1`], Borsh-encoded inside the v2 length-prefixed frame.
///
/// [`write_framed`] flushes, and on this path that is not hygiene but the protocol. A worker's
/// stdout is a `LineWriter` over a pipe and these frames are binary: unflushed, the manifest frame
/// sits in a 1 KiB buffer waiting for a newline byte that a length prefix and a Borsh struct will
/// not reliably contain, while the gateway blocks reading it and the worker blocks reading the
/// request the gateway has not sent. One-shot `v3-job` would never notice — process exit flushes —
/// and a resident loop deadlocks on its handshake, so
/// [`tests::every_frame_is_flushed_when_it_is_written`] pins the behaviour here rather than
/// leaving a shared writer free to drop it.
pub fn write_frame_v1<W: Write>(output: &mut W, frame: &PalwFpWorkerFrameV1) -> Result<(), String> {
    let bytes = borsh::to_vec(frame).map_err(|e| format!("cannot serialize a worker frame: {e}"))?;
    write_framed(output, &bytes).map_err(|e| format!("cannot write a worker frame: {e}"))
}

/// **One frame off a PERSISTENT stream**, or `None` at a clean end of stream.
///
/// Byte-for-byte the wire [`read_framed`] reads — a four-byte little-endian length, then that many
/// bytes, refused above `max_bytes` — without its trailing-byte probe. See the module note: that
/// probe asserts one frame per process and makes a resident loop impossible.
///
/// A stream that ends INSIDE a frame is an error and not an end: a truncated request must not be
/// mistaken for a gateway that hung up politely.
pub fn read_framed_stream<R: Read>(reader: &mut R, max_bytes: u32) -> Result<Option<Vec<u8>>, String> {
    let mut len_bytes = [0u8; 4];
    let mut filled = 0usize;
    while filled < len_bytes.len() {
        match reader.read(&mut len_bytes[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(format!("the stream ended {filled} bytes into a frame length")),
            Ok(n) => filled += n,
            Err(e) => return Err(format!("reading a frame length: {e}")),
        }
    }
    let len = u32::from_le_bytes(len_bytes);
    if len > max_bytes {
        return Err(format!("frame of {len} bytes exceeds the {max_bytes}-byte ceiling"));
    }
    let mut payload = vec![0u8; len as usize];
    let mut filled = 0usize;
    while filled < payload.len() {
        match reader.read(&mut payload[filled..]) {
            Ok(0) => return Err(format!("the stream ended {filled} bytes into a {len}-byte frame")),
            Ok(n) => filled += n,
            Err(e) => return Err(format!("reading a frame body: {e}")),
        }
    }
    Ok(Some(payload))
}

/// Read a run of frames a `v3-serve` worker wrote, for a caller holding the whole stream. The
/// gateway reads them one at a time off a pipe; a test reads them out of a buffer.
pub fn decode_frames_v1(mut bytes: &[u8]) -> Result<Vec<PalwFpWorkerFrameV1>, String> {
    let mut frames = Vec::new();
    while let Some(payload) = read_framed_stream(&mut bytes, PALW_V2_MAX_FRAME_BYTES)? {
        frames.push(borsh::from_slice(&payload).map_err(|e| format!("a worker frame does not decode: {e}"))?);
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qwen25_a16_backend::Qwen25A16Backend;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    /// **The fixture class, built the way `e2e_drill::a16_fixture_v1` builds it**: a two-layer
    /// derived A16 store under the corrected (graph-v2) profile. Small enough to run in CI, and
    /// the same registered graph the 1.5B artifact is served under — which is what makes a root
    /// comparison on it meaningful.
    fn fixture_runtime_v1() -> FpWorkerRuntime<Qwen25A16Backend> {
        fixture_runtime_with_artifact_v1(None)
    }

    /// The same fixture, with an SA-6 guard over a real file on disk — the shape both shipped
    /// binaries are in, and the only way to exercise re-verification without a 33 GiB artifact.
    fn fixture_runtime_with_artifact_v1(artifact_guard: Option<MappedArtifactV1>) -> FpWorkerRuntime<Qwen25A16Backend> {
        use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v2};

        let geometry = PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 8,
            ffn_dim: 8,
            attn_heads: 2,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            // The fixture tokenizer's whole table: the engine's argmax can select any id in the
            // class's logit row, and a tokenizer narrower than the class could not decode its own
            // model's answer.
            vocab_size: crate::tokenizer::FIXTURE_VOCAB,
            n_ctx: 32,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        };
        let shape = crate::artifact::Base0ShapeV1 {
            n_layers: geometry.layer_count as usize,
            n_heads: geometry.attn_heads as usize,
            n_kv_heads: geometry.attn_kv_heads as usize,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: crate::artifact::LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let artifact = crate::artifact::Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("the fixture weights derive")
            .with_a16_params(crate::engine_a16::derived_a16_store(&shape))
            .expect("the fixture A16 store derives");
        let digest = artifact.artifact_digest();
        let profile = qwen25_a16_profile_v2(geometry).expect("the fixture geometry projects");
        let backend = Qwen25A16Backend::from_registered_profile(
            std::sync::Arc::new(artifact),
            b"misaka-palw-rc".to_vec(),
            profile.clone(),
            (4, 2),
        )
        .expect("the fixture backend serves its registered graph");
        let (tokenizer, _, _) = crate::tokenizer::byte_level_fixture_v1();
        FpWorkerRuntime::new(
            backend,
            &profile,
            tokenizer,
            FpWorkerFamilyV1 {
                model_id: "fixture/Qwen2.5-A16/graph-v2".to_string(),
                runtime_identity: digest,
                tokenizer_id: Hash64::default(),
                vocab: profile.vocab_size,
                retention_schema: "misaka.palw.fp-v3-a16-retention.v1",
                retention_family: "qwen25-a16",
                eog_token_names: QWEN_EOG_TOKEN_NAMES,
                artifact: artifact_guard,
            },
            b"misaka-palw-rc".to_vec(),
            7,
        )
        .expect("the fixture runtime builds")
    }

    /// **The result with its two clocks zeroed.**
    ///
    /// `model_load_ms` and `execute_ms` are wall-clock measurements of the machine, not of the
    /// job: they differ between two runs of the SAME job by definition, and a byte-for-byte
    /// comparison that included them would be a test that passes when the host is fast enough for
    /// both runs to round to the same millisecond. Everything a commitment is made of — the four
    /// roots, the job, the ids, the counts, the stop reason — is compared.
    fn without_clocks(result: &PalwFpWorkerResultV3) -> Vec<u8> {
        let mut bare = result.clone();
        bare.model_load_ms = 0;
        bare.execute_ms = 0;
        borsh::to_vec(&bare).expect("a result serializes")
    }

    /// A request against the fixture runtime's own manifest, with the ids arm — the replay shape,
    /// and the one that needs no tokenizer agreement to be meaningful.
    fn fixture_request_v1(manifest: &PalwFpWorkerManifestV1, input: PalwFpWorkerInputV3) -> PalwFpWorkerRequestV3 {
        PalwFpWorkerRequestV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(0xD0),
            class_id: manifest.class_id,
            executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
            executor_pubkey: vec![0x11; 32],
            operator_id: Hash64::from_u64_word(0x0B),
            anchor_block: Hash64::from_u64_word(0xA0),
            anchor_daa: 1234,
            job_nonce: [0x5A; 32],
            decode_token_limit: 2,
            max_context_tokens: manifest.n_ctx,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            input,
            model_profile_id: manifest.model_profile_id,
            runtime_manifest_hash: manifest.runtime_manifest_hash,
            runtime_class_id: manifest.runtime_class_id,
            shape_profile_id: manifest.shape_profile_id,
            trace_scheme_id: manifest.trace_scheme_id,
        }
    }

    fn framed(request: &PalwFpWorkerRequestV3) -> Vec<u8> {
        let payload = borsh::to_vec(request).expect("a request serializes");
        let mut frame = (payload.len() as u32).to_le_bytes().to_vec();
        frame.extend_from_slice(&payload);
        frame
    }

    /// **W6: a job's roots through `v3-serve` are byte-identical to the same job's roots through
    /// `v3-job`.**
    ///
    /// The one thing Decision 1 pins, and the reason the serve loop is allowed to exist: if a
    /// resident artifact could produce a different commitment from a freshly mapped one, every
    /// drill and every replay would be verifying a runtime nobody uses. The whole result is
    /// compared, not just the four roots — a difference in the executed count or the retained
    /// chunk count would be the same defect wearing different clothes.
    #[test]
    fn a_jobs_roots_are_the_same_through_v3_serve_and_v3_job() {
        let temp = std::env::temp_dir().join(format!("palw-fp-worker-w6-{}", std::process::id()));
        let job_dir = temp.join("job");
        let serve_dir = temp.join("serve");
        let manifest = fixture_runtime_v1().manifest().clone();
        let request = fixture_request_v1(&manifest, PalwFpWorkerInputV3::TokenIds(vec![3, 5, 8, 13, 21]));
        let frame = framed(&request);

        let mut job_out = Vec::new();
        run_v3_job_v1(&mut frame.as_slice(), &mut job_out, &job_dir, fixture_runtime_v1).expect("the one-shot job runs");
        let job_result: PalwFpWorkerResultV3 = borsh::from_slice(
            &read_framed_stream(&mut job_out.as_slice(), PALW_V2_MAX_FRAME_BYTES)
                .expect("the one-shot frame reads")
                .expect("the one-shot mode wrote a frame"),
        )
        .expect("the one-shot result decodes");

        let rt = fixture_runtime_v1();
        let mut serve_out = Vec::new();
        run_v3_serve_v1(&rt, &mut frame.as_slice(), &mut serve_out, &serve_dir).expect("the serve session ends cleanly");
        let frames = decode_frames_v1(&serve_out).expect("the serve frames decode");

        assert!(matches!(frames.first(), Some(PalwFpWorkerFrameV1::Manifest(_))), "the first frame is the manifest");
        let Some(PalwFpWorkerFrameV1::Result(serve_result)) = frames.last() else {
            panic!("a serve session's accepted job ends in exactly one Result frame, got {frames:?}");
        };
        assert_eq!(frames.iter().filter(|f| matches!(f, PalwFpWorkerFrameV1::Result(_))).count(), 1);

        assert_eq!(serve_result.trace_root, job_result.trace_root, "W6: trace root");
        assert_eq!(serve_result.output_root, job_result.output_root, "W6: output root");
        assert_eq!(serve_result.schedule_root, job_result.schedule_root, "W6: schedule root");
        assert_eq!(serve_result.execution_root, job_result.execution_root, "W6: execution root");
        assert_eq!(serve_result.trace_manifest_root, job_result.trace_manifest_root, "W6: trace manifest root");
        assert_eq!(without_clocks(serve_result), without_clocks(&job_result), "W6: the whole result, byte for byte");
        // And the retention the two modes wrote is the same object under the same job id.
        let binding = hex(fp_job_id_v3(&job_result.job));
        assert_eq!(
            std::fs::read(job_dir.join(&binding).join("material.bin")).expect("the one-shot mode retained its capture"),
            std::fs::read(serve_dir.join(&binding).join("material.bin")).expect("the serve mode retained its capture"),
        );
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// **The resident artifact answers a second job, and one bad job does not drop it.**
    ///
    /// Decision 1's "done when" is that the hybrid answers a second job without re-mapping; a
    /// fixture cannot measure eight minutes, but it can measure the property that makes it
    /// possible — that ONE mapping serves an unbounded run of jobs, and that a request the worker
    /// refuses ends in exactly one `Refused` frame with the loop still reading.
    #[test]
    fn a_refused_job_is_one_frame_and_the_worker_stays_up() {
        let temp = std::env::temp_dir().join(format!("palw-fp-worker-stayup-{}", std::process::id()));
        let rt = fixture_runtime_v1();
        let manifest = rt.manifest().clone();

        let good = framed(&fixture_request_v1(&manifest, PalwFpWorkerInputV3::TokenIds(vec![3, 5, 8, 13, 21])));
        // A privacy mode the panel cannot replay: refused by name, before anything executes.
        let mut bad_request = fixture_request_v1(&manifest, PalwFpWorkerInputV3::TokenIds(vec![3, 5]));
        bad_request.privacy_mode = 2;
        let bad = framed(&bad_request);
        // A class this worker is not.
        let mut wrong_class = fixture_request_v1(&manifest, PalwFpWorkerInputV3::TokenIds(vec![3, 5]));
        wrong_class.class_id = Hash64::from_u64_word(0xDEAD);
        let wrong = framed(&wrong_class);

        let mut stream = Vec::new();
        stream.extend_from_slice(&bad);
        stream.extend_from_slice(&good);
        stream.extend_from_slice(&wrong);
        stream.extend_from_slice(&good);

        let mut out = Vec::new();
        run_v3_serve_v1(&rt, &mut stream.as_slice(), &mut out, &temp).expect("the serve session ends cleanly");
        let frames = decode_frames_v1(&out).expect("the serve frames decode");

        let terminators: Vec<&PalwFpWorkerFrameV1> =
            frames.iter().filter(|f| matches!(f, PalwFpWorkerFrameV1::Result(_) | PalwFpWorkerFrameV1::Refused { .. })).collect();
        assert_eq!(terminators.len(), 4, "four requests, four terminators: {frames:?}");
        assert!(matches!(terminators[0], PalwFpWorkerFrameV1::Refused { reason } if reason.contains("PublicDa")), "{terminators:?}");
        assert!(matches!(terminators[1], PalwFpWorkerFrameV1::Result(_)));
        assert!(matches!(terminators[2], PalwFpWorkerFrameV1::Refused { reason } if reason.contains("class_id")), "{terminators:?}");
        assert!(matches!(terminators[3], PalwFpWorkerFrameV1::Result(_)));

        // The two accepted jobs are the SAME job, so the resident runtime answered the second one
        // exactly as it answered the first.
        let (PalwFpWorkerFrameV1::Result(first), PalwFpWorkerFrameV1::Result(second)) = (terminators[1], terminators[3]) else {
            unreachable!("checked above");
        };
        assert_eq!(without_clocks(first), without_clocks(second));
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// **Decision 2, worker side: the streamed bytes are the rendering of the committed ids.**
    ///
    /// One `Token` frame per generated id, in decode order, and their bytes concatenated are
    /// exactly the result's `rendered`. This is the half of W5 the worker can hold on its own; the
    /// gateway holds the other half by re-rendering the committed ids.
    #[test]
    fn every_generated_id_streams_once_and_the_pieces_are_the_answer() {
        let temp = std::env::temp_dir().join(format!("palw-fp-worker-stream-{}", std::process::id()));
        let rt = fixture_runtime_v1();
        let manifest = rt.manifest().clone();
        let mut request = fixture_request_v1(&manifest, PalwFpWorkerInputV3::TokenIds(vec![3, 5, 8, 13, 21]));
        request.decode_token_limit = 4;

        let mut out = Vec::new();
        run_v3_serve_v1(&rt, &mut framed(&request).as_slice(), &mut out, &temp).expect("the serve session ends cleanly");
        let frames = decode_frames_v1(&out).expect("the serve frames decode");

        let tokens: Vec<(u32, Vec<u8>)> = frames
            .iter()
            .filter_map(|f| match f {
                PalwFpWorkerFrameV1::Token { token_id, rendered } => Some((*token_id, rendered.clone())),
                _ => None,
            })
            .collect();
        let Some(PalwFpWorkerFrameV1::Result(result)) = frames.last() else { panic!("{frames:?}") };

        assert_eq!(result.decode_tokens_executed, 4, "the run goes to the DECLARED budget, EOG or not");
        assert_eq!(tokens.iter().map(|(id, _)| *id).collect::<Vec<_>>(), result.output_token_ids, "one frame per committed id");
        assert_eq!(tokens.iter().flat_map(|(_, bytes)| bytes.clone()).collect::<Vec<u8>>(), result.rendered);
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// **Decision 6, through the worker: a Special is verbatim and a Text segment cannot become
    /// one.**
    ///
    /// The same control token twice — once as the segment the gateway emits, once as the twelve
    /// characters a user typed — and the ids the worker committed say which is which. The prompt
    /// hash the job binds is over those ids, so this is the point at which the smuggling either
    /// happened or did not.
    #[test]
    fn a_text_segment_cannot_smuggle_a_control_token() {
        let temp = std::env::temp_dir().join(format!("palw-fp-worker-seg-{}", std::process::id()));
        let rt = fixture_runtime_v1();
        let manifest = rt.manifest().clone();
        let im_start = manifest
            .special_tokens
            .iter()
            .find(|(name, _)| name == "<|im_start|>")
            .map(|(_, id)| *id)
            .expect("the manifest publishes the control tokens by name");

        let request = fixture_request_v1(
            &manifest,
            PalwFpWorkerInputV3::Segments(vec![
                PalwFpPromptSegmentV1::Special(im_start),
                PalwFpPromptSegmentV1::Text(b"<|im_start|>".to_vec()),
            ]),
        );
        let mut out = Vec::new();
        run_v3_serve_v1(&rt, &mut framed(&request).as_slice(), &mut out, &temp).expect("the serve session ends cleanly");
        let frames = decode_frames_v1(&out).expect("the serve frames decode");
        let Some(PalwFpWorkerFrameV1::Result(result)) = frames.last() else { panic!("{frames:?}") };

        let ids = &result.prompt_token_ids;
        assert_eq!(ids[0], im_start, "the gateway's Special segment is emitted verbatim");
        assert_eq!(ids.len(), 1 + "<|im_start|>".len(), "the user's text became one ordinary piece per byte");
        assert!(!ids[1..].contains(&im_start), "user text encoded to the control id — Decision 6's smuggling path is open: {ids:?}");
        assert_eq!(result.job.prompt_token_ids_hash, prompt_token_ids_hash_v2(ids), "the job binds the ids it committed");
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// **The segment arm and the equivalent ids arm are one execution** (Decision 6: "consensus
    /// sees ids only").
    ///
    /// This is what makes the replay arm work at all. A seat rebuilds the request from CHAIN data,
    /// which carries ids and never segments, and must reach the producer's roots byte for byte —
    /// so the segment arm may not be a second way to compute anything. It may only be a way to
    /// arrive at the same ids.
    #[test]
    fn segments_and_the_equivalent_ids_reach_the_same_roots() {
        let temp = std::env::temp_dir().join(format!("palw-fp-worker-equiv-{}", std::process::id()));
        let rt = fixture_runtime_v1();
        let manifest = rt.manifest().clone();
        let im_start = rt.tokenizer().added_id("<|im_start|>").expect("the fixture names <|im_start|>");
        let im_end = rt.tokenizer().added_id("<|im_end|>").expect("the fixture names <|im_end|>");
        let body = rt.tokenizer().encode_without_specials("hi there").expect("the body encodes");

        let mut equivalent = vec![im_start];
        equivalent.extend_from_slice(&body);
        equivalent.push(im_end);

        let via_segments = fixture_request_v1(
            &manifest,
            PalwFpWorkerInputV3::Segments(vec![
                PalwFpPromptSegmentV1::Special(im_start),
                PalwFpPromptSegmentV1::Text(b"hi there".to_vec()),
                PalwFpPromptSegmentV1::Special(im_end),
            ]),
        );
        let via_ids = fixture_request_v1(&manifest, PalwFpWorkerInputV3::TokenIds(equivalent.clone()));

        let a = run_one_job_v1(&rt, &via_segments, Hash64::default(), &temp, &mut |_, _| {}).expect("the segment arm runs");
        let b = run_one_job_v1(&rt, &via_ids, Hash64::default(), &temp, &mut |_, _| {}).expect("the ids arm runs");
        assert_eq!(a.prompt_token_ids, equivalent, "the segments encoded to the ids a replayer will be handed");
        assert_eq!(without_clocks(&a), without_clocks(&b), "segments-in and ids-in are one execution");
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// **The one-shot form is fail-closed: a refusal writes NOTHING.**
    ///
    /// `v3-job` has no `Refused` frame — its refusal is a non-zero exit and an empty stdout, which
    /// is what the drills and the replay arm read. A half-written frame there would be a result
    /// the caller could parse, so the ordering (validate, then write) is pinned rather than
    /// assumed; and the single-frame contract still holds, so a second request in the same stream
    /// is an error rather than a second job.
    #[test]
    fn the_one_shot_form_writes_nothing_on_a_refusal() {
        let temp = std::env::temp_dir().join(format!("palw-fp-worker-oneshot-{}", std::process::id()));
        let manifest = fixture_runtime_v1().manifest().clone();
        let mut bad = fixture_request_v1(&manifest, PalwFpWorkerInputV3::TokenIds(vec![3, 5]));
        bad.decode_token_limit = 0;

        let mut out = Vec::new();
        let err = run_v3_job_v1(&mut framed(&bad).as_slice(), &mut out, &temp, fixture_runtime_v1)
            .expect_err("a zero decode ceiling is not a job");
        assert!(err.contains("zero decode ceiling"), "{err}");
        assert!(out.is_empty(), "fail-closed means nothing on stdout: {out:?}");

        // Two frames in one stream: the contract's reader refuses the pair, and this is the check
        // `read_framed_stream` deliberately does not make (see its note).
        let good = framed(&fixture_request_v1(&manifest, PalwFpWorkerInputV3::TokenIds(vec![3, 5, 8])));
        let mut two = good.clone();
        two.extend_from_slice(&good);
        let mut out = Vec::new();
        assert!(run_v3_job_v1(&mut two.as_slice(), &mut out, &temp, fixture_runtime_v1).is_err());
        assert!(out.is_empty());
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// **The manifest states the CLASS's width and the class's own scheme.**
    ///
    /// A frozen test on a consensus-visible identity: `n_ctx` is the registered context and not
    /// the artifact's rotary span, and `trace_scheme_id` is read off the profile rather than
    /// written down. The fixture registers 32 positions against an artifact whose table also
    /// covers 32, so the second assertion is what carries the rule — on the shipped dense tier the
    /// two are 16 and 512.
    #[test]
    fn the_manifest_publishes_the_registered_width_and_scheme() {
        use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v2};
        let rt = fixture_runtime_v1();
        let manifest = rt.manifest();
        let profile = qwen25_a16_profile_v2(PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 8,
            ffn_dim: 8,
            attn_heads: 2,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            vocab_size: crate::tokenizer::FIXTURE_VOCAB,
            n_ctx: 32,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        })
        .expect("the fixture geometry projects");

        assert_eq!(manifest.version, PALW_FP_WORKER_MANIFEST_V1_VERSION);
        assert_eq!(manifest.n_ctx, profile.n_ctx, "the manifest's width is the class's registered n_ctx");
        assert_eq!(manifest.prefill_single_batch_cap, profile.n_ctx);
        assert_eq!(manifest.class_id, profile.shape_profile_id());
        assert_eq!(manifest.shape_profile_id, profile.shape_profile_id());
        assert_eq!(manifest.vocab, profile.vocab_size);
        assert_eq!(
            manifest.trace_scheme_id,
            kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1(),
            "the free-prompt classes commit the tiled logits scheme; a flat class is refused at construction"
        );
        assert_eq!(manifest.eog_token_ids, vec![rt.tokenizer().added_id("<|im_end|>").expect("the fixture names <|im_end|>")]);

        // Every key `misaka-palw-gateway` reads off the one-shot document is still spelled the
        // same, and reads back to the same value.
        let doc = manifest_json_v1(manifest);
        for key in [
            "schema",
            "runtime_manifest_hash",
            "runtime_class_id",
            "model_profile_id",
            "shape_profile_id",
            "trace_scheme_id",
            "tokenizer_id",
            "n_ctx",
            "prefill_single_batch_cap",
            "shape_string",
        ] {
            assert!(doc.get(key).is_some(), "the v3-manifest document lost the key {key} a gateway reads");
        }
        assert_eq!(doc["schema"], "misaka.palw.fp-v3-manifest.v1");
        assert_eq!(doc["n_ctx"], profile.n_ctx);
        assert_eq!(doc["shape_string"], manifest.model_id);
    }

    /// **ADR-0077 SA-6: the resident runtime re-verifies what it mapped, and a changed artifact is
    /// a refusal rather than a crash.**
    ///
    /// Three states of the same file across one serve session: unchanged (the job runs), rewritten
    /// with the SAME bytes (the identity moved, the digest did not — the job runs), and truncated
    /// (the digest moved — refused by name, the worker still reading). The third is also the fault
    /// SA-6 names, caught before it happens: on a MAPPED family, touching a page past a truncated
    /// end of file is a `SIGBUS`, so the size check is what turns that fault into a `JobFailed`.
    #[test]
    fn a_changed_artifact_is_refused_and_the_worker_stays_up() {
        let temp = std::env::temp_dir().join(format!("palw-fp-worker-sa6-{}", std::process::id()));
        std::fs::create_dir_all(&temp).expect("the test dir");
        let artifact_path = temp.join("artifact.bin");
        std::fs::write(&artifact_path, vec![0xA5u8; 4096]).expect("the fixture artifact writes");

        let guard = MappedArtifactV1::verify_by_reading(&artifact_path).expect("the artifact verifies at map time");
        let rt = fixture_runtime_with_artifact_v1(Some(guard));
        let manifest = rt.manifest().clone();
        let good = framed(&fixture_request_v1(&manifest, PalwFpWorkerInputV3::TokenIds(vec![3, 5, 8, 13, 21])));

        // 1. Unchanged.
        let mut out = Vec::new();
        run_v3_serve_v1(&rt, &mut good.as_slice(), &mut out, &temp).expect("the session ends cleanly");
        assert!(matches!(decode_frames_v1(&out).unwrap().last(), Some(PalwFpWorkerFrameV1::Result(_))));

        // 2. Rewritten, byte-identical: on a fresh inode, so the identity moved and the digest did
        //    not. The re-read is paid once and the job runs.
        let other = temp.join("artifact.new");
        std::fs::write(&other, vec![0xA5u8; 4096]).expect("the replacement writes");
        std::fs::rename(&other, &artifact_path).expect("the replacement lands");
        let mut out = Vec::new();
        run_v3_serve_v1(&rt, &mut good.as_slice(), &mut out, &temp).expect("the session ends cleanly");
        assert!(
            matches!(decode_frames_v1(&out).unwrap().last(), Some(PalwFpWorkerFrameV1::Result(_))),
            "the same bytes under a new inode are the same artifact"
        );

        // 3. Truncated: the digest moved. Refused by name, and the loop reads the NEXT request.
        std::fs::write(&artifact_path, vec![0xA5u8; 2048]).expect("the truncation writes");
        let mut two = good.clone();
        two.extend_from_slice(&good);
        let mut out = Vec::new();
        run_v3_serve_v1(&rt, &mut two.as_slice(), &mut out, &temp).expect("a refusal does not end the session");
        let frames = decode_frames_v1(&out).expect("the frames decode");
        let terminators: Vec<&PalwFpWorkerFrameV1> =
            frames.iter().filter(|f| matches!(f, PalwFpWorkerFrameV1::Result(_) | PalwFpWorkerFrameV1::Refused { .. })).collect();
        assert_eq!(terminators.len(), 2, "the worker answered both requests after the artifact moved: {frames:?}");
        for t in &terminators {
            assert!(
                matches!(t, PalwFpWorkerFrameV1::Refused { reason } if reason.contains("no longer the file this runtime verified")),
                "{terminators:?}"
            );
        }
        // And an artifact that vanished is the same kind of answer, not a panic.
        std::fs::remove_file(&artifact_path).expect("the artifact is removed");
        let mut out = Vec::new();
        run_v3_serve_v1(&rt, &mut good.as_slice(), &mut out, &temp).expect("a missing artifact does not end the session");
        assert!(matches!(decode_frames_v1(&out).unwrap().last(), Some(PalwFpWorkerFrameV1::Refused { .. })));
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// **ADR-0079 SA-7: no refusal this module writes carries prompt text or a prompt id.**
    ///
    /// The three refusals a stranger's prompt can provoke, each checked against the value that
    /// provoked it. "private unless disputed" is false if the default log is a disclosure, and the
    /// `Refused` reason is exactly what the serve loop prints to stderr.
    #[test]
    fn a_refusal_names_the_rule_and_never_the_prompt() {
        let rt = fixture_runtime_v1();
        let manifest = rt.manifest().clone();

        // A guessed control id, out of range: the refusal names the segment, not the id.
        let ghost = manifest.vocab + 4242;
        let err = prompt_ids_for_input_v1(
            rt.tokenizer(),
            &manifest,
            &PalwFpWorkerInputV3::Segments(vec![
                PalwFpPromptSegmentV1::Text(b"hello".to_vec()),
                PalwFpPromptSegmentV1::Special(ghost),
            ]),
        )
        .expect_err("an out-of-range control id is refused");
        assert!(err.contains("segment 1"), "{err}");
        assert!(!err.contains(&ghost.to_string()), "the refusal echoed the prompt id back: {err}");

        // A prompt id past the model's vocab on the ids arm: the position, not the id.
        let temp = std::env::temp_dir().join(format!("palw-fp-worker-sa7-{}", std::process::id()));
        let mut request = fixture_request_v1(&manifest, PalwFpWorkerInputV3::TokenIds(vec![3, 5, ghost]));
        request.decode_token_limit = 1;
        let err = run_one_job_v1(&rt, &request, Hash64::default(), &temp, &mut |_, _| {})
            .expect_err("a prompt token past the vocab is refused");
        assert!(err.contains("position 2"), "{err}");
        assert!(!err.contains(&ghost.to_string()), "the refusal echoed the prompt id back: {err}");

        // Text that has no id in this vocabulary: the reason, never the piece. The fixture has no
        // merges and every byte is a token, so the unrepresentable case is constructed directly.
        let unrepresentable = crate::tokenizer::TokenizerError::Unrepresentable("the user's secret".to_string());
        assert!(!unrepresentable.kind().contains("secret"), "{}", unrepresentable.kind());
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// **Every frame is flushed as it is written.**
    ///
    /// A resident worker's stdout is a `LineWriter` over a pipe and these frames are binary: an
    /// unflushed manifest waits for a newline that a length prefix will not reliably contain,
    /// while the gateway blocks reading it. `write_framed` flushes today; this counts the flushes
    /// so that a change to the shared writer shows up here rather than as a resident worker that
    /// hangs on its handshake in production. One per frame, and the manifest's before the loop
    /// reads its first request.
    #[test]
    fn every_frame_is_flushed_when_it_is_written() {
        #[derive(Default)]
        struct CountingWriter {
            bytes: Vec<u8>,
            flushed_at: Vec<usize>,
        }
        impl Write for CountingWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.bytes.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.flushed_at.push(self.bytes.len());
                Ok(())
            }
        }

        let temp = std::env::temp_dir().join(format!("palw-fp-worker-flush-{}", std::process::id()));
        let rt = fixture_runtime_v1();
        let manifest = rt.manifest().clone();
        let mut request = fixture_request_v1(&manifest, PalwFpWorkerInputV3::TokenIds(vec![3, 5, 8, 13, 21]));
        request.decode_token_limit = 3;

        let mut out = CountingWriter::default();
        run_v3_serve_v1(&rt, &mut framed(&request).as_slice(), &mut out, &temp).expect("the session ends cleanly");
        let frames = decode_frames_v1(&out.bytes).expect("the frames decode");
        assert_eq!(out.flushed_at.len(), frames.len(), "one flush per frame: {} frames, {:?}", frames.len(), out.flushed_at);
        // The manifest reaches the gateway before anything is decoded, which is what the handshake
        // needs: its flush is the first, and it happens at the manifest frame's own length.
        let manifest_len = 4 + borsh::to_vec(&PalwFpWorkerFrameV1::Manifest(manifest)).unwrap().len();
        assert_eq!(out.flushed_at.first(), Some(&manifest_len));
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// **The stream reader is the contract's reader, one frame at a time.**
    ///
    /// [`read_framed_stream`] exists only because [`read_framed`] asserts end-of-stream after the
    /// payload. That is the ONLY difference, and this is what keeps it the only one: on a single
    /// frame the two return the same bytes, and on a truncated one they both refuse.
    #[test]
    fn the_stream_reader_reads_the_same_wire_as_read_framed() {
        let payload = vec![7u8; 300];
        let mut frame = (payload.len() as u32).to_le_bytes().to_vec();
        frame.extend_from_slice(&payload);

        assert_eq!(read_framed(&mut frame.as_slice(), PALW_V2_MAX_FRAME_BYTES).expect("one frame"), payload);
        assert_eq!(read_framed_stream(&mut frame.as_slice(), PALW_V2_MAX_FRAME_BYTES).expect("one frame"), Some(payload.clone()));

        // Two frames: the contract's reader refuses the pair outright, which is precisely why a
        // resident loop cannot be built on it.
        let mut pair = frame.clone();
        pair.extend_from_slice(&frame);
        assert!(read_framed(&mut pair.as_slice(), PALW_V2_MAX_FRAME_BYTES).is_err());
        let mut rest: &[u8] = &pair;
        assert_eq!(read_framed_stream(&mut rest, PALW_V2_MAX_FRAME_BYTES).unwrap(), Some(payload.clone()));
        assert_eq!(read_framed_stream(&mut rest, PALW_V2_MAX_FRAME_BYTES).unwrap(), Some(payload));
        assert_eq!(read_framed_stream(&mut rest, PALW_V2_MAX_FRAME_BYTES).unwrap(), None, "a clean end of stream is not an error");

        // A frame that ends early is an error, not an end: a truncated request must not read as a
        // gateway hanging up politely.
        let mut truncated: &[u8] = &frame[..frame.len() - 1];
        assert!(read_framed_stream(&mut truncated, PALW_V2_MAX_FRAME_BYTES).is_err());
        // And the ceiling is the contract's.
        let mut oversized = (PALW_V2_MAX_FRAME_BYTES + 1).to_le_bytes().to_vec();
        oversized.extend_from_slice(&[0u8; 8]);
        assert!(read_framed_stream(&mut oversized.as_slice(), PALW_V2_MAX_FRAME_BYTES).is_err());
    }
}
