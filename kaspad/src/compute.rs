//! MISAKA Verified LLM Token-Weighted BFT: the node-side compute role.
//!
//! This module holds everything about the compute cycle that can be decided without a chain
//! session or a funding UTXO — configuration, the runtime handle, how a job's spec is derived, and
//! what the cycle should do next. The transaction building and submission live in
//! [`crate::validator_service`], where the funding chain already is.
//!
//! The split is not cosmetic. The decisions here are the ones with consensus consequences (which
//! profile we declare, what spec we commit to, when a commitment is too old to certify) and they
//! are testable without a 24 GB model, a Metal GPU, or a DAG.
//!
//! # Two roles, one loop
//!
//! A validator participates as an **executor** (originating jobs it gets credited for) and as a
//! **verifier** (auditing jobs it was sortitioned onto). Both use the same runtime, but they are
//! not symmetric in importance: acceptance is refutation-dominant, so a verifier that goes quiet
//! costs *other* validators their credit, while an executor that goes quiet costs only itself.
//! The cycle therefore serves the verifier queue first.
//!
//! # Jobs are self-originated, and that is why only one role is paid
//!
//! There is no external party submitting work: a validator supplies its own input
//! (`--compute-prompt`) and is credited for computing it. §6's "Job fee" therefore has nobody to
//! collect it from and nobody to pay it — it is not missing, it does not apply.
//!
//! What follows from that is the shape of the incentives, and it explains why the §6 audit fee
//! exists but no matching execution reward does. Executing is **self-interested**: it costs GPU
//! time and two overlay transactions, and it buys this validator voting weight. Verifying is
//! **not**: it costs a full replay and a transaction, and everything it produces accrues to the
//! executor being audited. Only the second needs paying, and paying the first would contradict §7
//! anyway — "Reward の配分は投票 weight の定義と分離する". An executor's income is the ordinary
//! validator participation reward it earns by attesting, exactly like every other operator.
//!
//! # Why an unregistered runtime disables the whole role
//!
//! A node whose runtime is not the consensus-registered build mints nothing (the model table
//! lookup fails) and, if it were drawn as a verifier, would compute a different `R_j` than an
//! honest executor and sign `Refuted` — zeroing honest credit and arming `ForgedReceipt` slashing
//! against an honest party. So [`ComputeRole::new`] refuses to enable the role at all rather than
//! participate approximately. In particular [`MockRuntime`] never claims the registered identity,
//! so a node without a real worker binary is inert by construction rather than by configuration.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(feature = "devnet-vlt-fixture")]
use kaspa_consensus_core::vlt::devnet_fixture;
use kaspa_consensus_core::vlt::{
    LlmJobSpec, MAX_JOB_INPUT_BYTES, ModelCostEntry, QuantizationProfile, VLT_PAYLOAD_VERSION_V1, VerificationScheme, VltParams,
    job_input_commitment,
};
use kaspa_core::{info, warn};
use kaspa_hashes::Hash64;
#[cfg(not(feature = "devnet-vlt-fixture"))]
use misaka_palw::MockRuntime;
use misaka_palw::{ComputeRuntime, MatchProjection, PalwError, PalwWorkerConfig, PalwWorkerRuntime};

const COMPUTE: &str = "validator-compute";

/// Keyed-BLAKE2b domain for [`sampling_seed_for`].
const SAMPLING_SEED_KEY: &[u8] = b"misaka-vlt-node-sampling-seed-v1";

/// How far ahead of a capability declaration's expiry to renew it, in DAA score.
///
/// A declaration that lapses drops this validator out of every committee draw, and under
/// refutation-dominant acceptance an executor whose committee has silently shrunk below
/// `min_verifier_confirmations` produces work that cannot mint. Renewing at 10% of the validity
/// period leaves many heartbeats of margin for a renewal transaction to be mined.
const CAPABILITY_RENEW_LEAD_FRACTION: u64 = 10;

/// Blocks to wait before re-submitting a compute transaction whose effect is not on chain yet.
///
/// Every decision in the cycle is derived from the chain, so a submitted-but-unmined transaction
/// looks exactly like one that was never sent. Without this grace the heartbeat would re-submit
/// the same declaration or commitment every tick until it mined, spending a fee each time.
const RESUBMIT_GRACE_BLOCKS: u64 = 120;

/// Static configuration for the compute role, from the `--enable-compute` family of flags.
#[derive(Clone, Debug)]
pub struct ComputeConfig {
    pub enabled: bool,
    /// Path to the pinned `palw-worker` binary. `None` runs the in-process mock, which cannot
    /// claim the registered runtime identity and therefore leaves the role disabled — useful only
    /// to exercise the wiring.
    pub worker_bin: Option<PathBuf>,
    /// Scratch directory the worker runs in.
    pub work_dir: PathBuf,
    /// Wall-clock ceiling for one job.
    pub timeout: Duration,
    /// File holding the prompt this node executes as an executor. `None` ⇒ verifier-only, which
    /// is a legitimate configuration: auditing is what the network is short of.
    pub prompt_path: Option<PathBuf>,
    /// Token ceiling this node asks for, clamped down to the registered profile's own limit.
    pub max_tokens: u32,
    /// Whether to follow a refuting verdict with a `ForgedReceipt` fraud proof.
    ///
    /// Off by default, and that default is the considered one — more so now that §7(c) is real.
    ///
    /// A refuting verdict already denies the certificate its credit, so the challenge buys only the
    /// reporter reward. What it costs is this node's entire bond if the certificate's committee
    /// goes on to confirm the receipt: `adjudicate_compute_challenge` then rules the challenge
    /// disproved and slashes the challenger. A divergence is not yet proof of fraud — a
    /// mis-declared determinism class or a marginal hardware fault produces exactly the same
    /// observation — so filing automatically means betting the bond on this node's own hardware
    /// being the correct one.
    pub auto_challenge: bool,
    /// MISAKA devnet fixture: originate at most this many **jobs**, ever. `None` = unbounded, which
    /// is what a real executor wants and what an asymmetric-weight experiment must not have.
    ///
    /// Jobs, not VLT. One fixture job is worth 50 VLT (`devnet_fixture::JOB_*`), so a plan of
    /// 400/250/150/100/100 VLT is a limit of 8/5/3/2/2 here.
    pub fixture_job_limit: Option<u32>,
    /// Where the quota's count is persisted. Without it a restart resets the count and the
    /// experiment's weights grow every time a node bounces.
    pub fixture_state_path: Option<PathBuf>,
}

impl Default for ComputeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            worker_bin: None,
            work_dir: std::env::temp_dir(),
            timeout: Duration::from_secs(900),
            prompt_path: None,
            max_tokens: 512,
            auto_challenge: false,
            fixture_job_limit: None,
            fixture_state_path: None,
        }
    }
}

/// The compute role, resolved once at startup: a runtime whose identity matched the registered
/// profile, the registry entry it corresponds to, and (optionally) the job this node executes.
///
/// Constructing one is what proves the node can participate. If any part of that fails the role is
/// simply absent, and the validator carries on attesting exactly as before.
pub struct ComputeRole {
    runtime: Arc<dyn ComputeRuntime>,
    /// The registered `(model, runtime)` pair this node runs — the profile it declares, executes
    /// under, and can be sortitioned to audit.
    pub entry: ModelCostEntry,
    /// The executor's job input. `None` ⇒ this node audits but does not originate.
    pub prompt: Option<Vec<u8>>,
    pub max_tokens: u32,
    pub auto_challenge: bool,
    /// MISAKA devnet fixture: the job quota and its persisted count, or `None` for an unbounded
    /// (production) executor.
    pub fixture_quota: Option<Mutex<FixtureExecutionState>>,
    pub fixture_state_path: Option<PathBuf>,
}

impl ComputeRole {
    /// Whether this node may originate another job.
    ///
    /// Always true without a quota. With one, false once the target is met — and it stays false
    /// across restarts, because the count is on disk. A node that kept originating would push its
    /// own weight past the plan on every epoch, which is the difference between a fixed experiment
    /// and a drifting one.
    pub fn may_originate(&self) -> bool {
        match &self.fixture_quota {
            None => true,
            Some(q) => q.lock().unwrap().remaining() > 0,
        }
    }

    /// Claim a quota slot for `job_id` before its commitment is broadcast.
    ///
    /// `true` without a quota (a production executor is unbounded) and for a job already claimed —
    /// re-committing after a lost transaction is a retry, not a second job.
    pub fn reserve_job(&self, job_id: Hash64) -> bool {
        let (Some(q), Some(path)) = (&self.fixture_quota, &self.fixture_state_path) else { return true };
        let mut st = q.lock().unwrap();
        let ok = st.reserve(&job_id.to_string(), path);
        if !ok {
            info!("[{COMPUTE}] fixture quota: {}/{} claimed; originating no further jobs", st.claimed(), st.target_jobs);
        }
        ok
    }

    /// Record that a claimed job's commitment reached the network.
    pub fn note_job_committed(&self, job_id: Hash64) {
        let (Some(q), Some(path)) = (&self.fixture_quota, &self.fixture_state_path) else { return };
        q.lock().unwrap().advance(&job_id.to_string(), FixtureSlotState::Committed, path);
    }

    /// Record that one job reached a submitted certificate, and flush the count.
    ///
    /// A certificate is the right thing to count: it is the last step this node controls, and the
    /// only one whose VLT will actually land. Counting commitments instead would spend quota on a
    /// job that expired before it could be certified, and the validator would silently finish
    /// under its target.
    pub fn note_job_certified(&self, job_id: Hash64) {
        let (Some(q), Some(path)) = (&self.fixture_quota, &self.fixture_state_path) else { return };
        let mut st = q.lock().unwrap();
        let before = st.certified();
        st.advance(&job_id.to_string(), FixtureSlotState::Certified, path);
        // Only when it actually moved. A re-submitted certificate for a job already certified is
        // the same job, and saying "certified" again is how the count reached 6 against 5.
        if st.certified() != before {
            info!("[{COMPUTE}] fixture quota: {}/{} job(s) certified ({} claimed)", st.certified(), st.target_jobs, st.claimed());
        }
    }
    /// Resolve the compute role, or `None` with a logged reason.
    ///
    /// `vlt` is the network's VLT parameters; the model table is what decides whether the runtime
    /// this node is actually running is one consensus knows about. On every shipped preset that
    /// table is empty, so this returns `None` and the role stays dormant without any flag saying so.
    pub fn new(cfg: &ComputeConfig, vlt: Option<&VltParams>, genesis_hash: Hash64) -> Option<Self> {
        if !cfg.enabled {
            return None;
        }
        let Some(vlt) = vlt else {
            warn!("[{COMPUTE}] --enable-compute set, but this network has no DNS overlay configured; compute role disabled");
            return None;
        };

        let runtime: Arc<dyn ComputeRuntime> = match &cfg.worker_bin {
            Some(bin) => {
                // The worker runs with the work dir as its cwd. Nothing else creates it, and a
                // missing cwd fails the spawn with ENOENT — an error that reads as "worker binary
                // not found" and points the operator at exactly the wrong file.
                if let Err(err) = std::fs::create_dir_all(&cfg.work_dir) {
                    warn!(
                        "[{COMPUTE}] could not create the compute work dir {}: {err}; compute role disabled",
                        cfg.work_dir.display()
                    );
                    return None;
                }
                Arc::new(PalwWorkerRuntime::new(PalwWorkerConfig {
                    worker_bin: bin.clone(),
                    work_dir: cfg.work_dir.clone(),
                    timeout: cfg.timeout,
                }))
            }
            // MISAKA devnet fixture: a deterministic executor whose identity is the one this
            // network's own preset registered. It is chosen only when no real worker was given —
            // an explicit `--compute-worker` always means the operator wants the real thing — and
            // it only *works* where the fixture profile is registered, which is the devnet preset
            // alone. On any other network the table lookup below finds nothing and the role stays
            // disabled, so the feature flag is not the only thing standing between this and
            // production.
            #[cfg(feature = "devnet-vlt-fixture")]
            None => {
                warn!(
                    "[{COMPUTE}] --enable-compute without --compute-worker: using the DEVNET VLT FIXTURE runtime.                      Deterministic, not a model — valid only where the fixture profile is registered."
                );
                Arc::new(misaka_palw::DevnetFixtureRuntime::new(genesis_hash))
            }
            #[cfg(not(feature = "devnet-vlt-fixture"))]
            None => {
                let _ = genesis_hash;
                warn!("[{COMPUTE}] --enable-compute set without --compute-worker; the mock runtime cannot be the registered profile");
                Arc::new(MockRuntime::default())
            }
        };
        // Refuse rather than participate approximately: see the module docs.
        if let Err(err) = runtime.assert_registered() {
            warn!("[{COMPUTE}] {err}");
            return None;
        }
        let identity = match runtime.probe() {
            Ok(id) => id,
            Err(err) => {
                warn!("[{COMPUTE}] could not probe the compute runtime: {err}; compute role disabled");
                return None;
            }
        };
        // The registry is keyed by `(h_M, h_R)` and the probe only reports `h_R`, so the profile is
        // whichever registered entry names this runtime. Ambiguity would mean one runtime build
        // registered against several model weights, which the node cannot resolve on its own.
        let matching: Vec<&ModelCostEntry> =
            vlt.model_cost_table.live().iter().filter(|e| e.runtime_hash == identity.runtime_hash).collect();
        let entry = match matching.as_slice() {
            [only] => **only,
            [] => {
                warn!(
                    "[{COMPUTE}] this node's runtime ({}) is not in the network's model cost table; compute role disabled",
                    identity.runtime_hash
                );
                return None;
            }
            many => {
                warn!("[{COMPUTE}] {} registered models name this runtime; cannot resolve which profile to run", many.len());
                return None;
            }
        };
        if entry.runtime_class_id != identity.runtime_class_id {
            warn!(
                "[{COMPUTE}] runtime reports determinism class {} but the registered profile is {}; compute role disabled",
                identity.runtime_class_id, entry.runtime_class_id
            );
            return None;
        }

        let prompt = match &cfg.prompt_path {
            Some(path) => match load_prompt(path) {
                Ok(bytes) => {
                    info!("[{COMPUTE}] executor job input: {} ({} bytes)", path.display(), bytes.len());
                    Some(bytes)
                }
                Err(err) => {
                    warn!("[{COMPUTE}] {err}; running verifier-only");
                    None
                }
            },
            None => {
                info!("[{COMPUTE}] no --compute-prompt given; running verifier-only (auditing peers' jobs, originating none)");
                None
            }
        };
        // The devnet fixture executes ONE job shape (`devnet_fixture::JOB_*`) and refuses any
        // other, so on that profile the ceiling is the shape's rather than the operator's. Left to
        // `--compute-max-tokens`, a default of 512 would clamp to the profile's 256 and every job
        // would be worth 1 978 VLT instead of 50 — the plan off by 40× while every log line still
        // read "quota met". The profile decides, not a flag: this is keyed on the registered entry
        // being the fixture's, so a real worker on the same binary is untouched.
        #[cfg(feature = "devnet-vlt-fixture")]
        let max_tokens = if entry == kaspa_consensus_core::vlt::devnet_fixture_entry(genesis_hash) {
            if cfg.max_tokens != devnet_fixture::JOB_MAX_TOKENS {
                info!(
                    "[{COMPUTE}] devnet fixture: max_tokens pinned to the fixed job shape ({} = {} prefill + {} decode, 50 VLT/job); --compute-max-tokens={} ignored",
                    devnet_fixture::JOB_MAX_TOKENS,
                    devnet_fixture::JOB_PREFILL_TOKENS,
                    devnet_fixture::JOB_DECODE_TOKENS,
                    cfg.max_tokens
                );
            }
            devnet_fixture::JOB_MAX_TOKENS
        } else {
            cfg.max_tokens.min(entry.max_tokens)
        };
        #[cfg(not(feature = "devnet-vlt-fixture"))]
        let max_tokens = cfg.max_tokens.min(entry.max_tokens);
        info!(
            "[{COMPUTE}] enabled: runtime={} class={} max_tokens={} auto_challenge={}",
            identity.runtime_hash, entry.runtime_class_id, max_tokens, cfg.auto_challenge
        );
        // The plan id binds the count to what it counted: a different validator, target or job
        // shape is a different experiment, and the old count is not evidence about it.
        let fixture_quota = cfg.fixture_job_limit.map(|target| {
            let plan_id = format!("{}:{}:{}:{}", identity.runtime_hash, target, max_tokens, prompt.as_ref().map_or(0, |p| p.len()));
            let path = cfg.fixture_state_path.clone().unwrap_or_else(|| cfg.work_dir.join("fixture-quota.json"));
            let st = FixtureExecutionState::load(&path, &plan_id, target);
            info!(
                "[{COMPUTE}] fixture quota: {}/{} job(s) certified, {} slot(s) claimed (plan {plan_id})",
                st.certified(),
                st.target_jobs,
                st.claimed()
            );
            Mutex::new(st)
        });
        let fixture_state_path =
            fixture_quota.as_ref().map(|_| cfg.fixture_state_path.clone().unwrap_or_else(|| cfg.work_dir.join("fixture-quota.json")));
        Some(Self { runtime, entry, prompt, max_tokens, auto_challenge: cfg.auto_challenge, fixture_quota, fixture_state_path })
    }

    /// The spec for a job over `input`, as a pure function of the input and the registered profile.
    ///
    /// Purity is what lets the executor survive the gap between committing to a job and certifying
    /// it — including a restart. The commitment publishes the input on chain, so at certificate
    /// time the spec is re-derived from chain data rather than recovered from local state that
    /// might not be there. `job_spec_id` of the result must equal the committed `job_id`, and the
    /// caller checks exactly that before spending a fee on the certificate.
    pub fn job_spec(&self, input: &[u8]) -> LlmJobSpec {
        job_spec_for(&self.entry, input, self.max_tokens)
    }

    /// Execute a job this node originated.
    pub fn execute(&self, spec: &LlmJobSpec, input: &[u8]) -> Result<MatchProjection, PalwError> {
        self.runtime.execute(spec, input)
    }

    /// Independently re-execute a peer's job.
    pub fn replay(&self, spec: &LlmJobSpec, input: &[u8]) -> Result<MatchProjection, PalwError> {
        self.runtime.replay(spec, input)
    }

    /// A clonable handle to the runtime, for running a job on a blocking thread.
    pub fn runtime(&self) -> Arc<dyn ComputeRuntime> {
        self.runtime.clone()
    }
}

/// The spec for a job over `input` under `entry`. Free function so it can be tested — and reasoned
/// about — without constructing a runtime.
pub fn job_spec_for(entry: &ModelCostEntry, input: &[u8], max_tokens: u32) -> LlmJobSpec {
    let input_commitment = job_input_commitment(input);
    LlmJobSpec {
        version: VLT_PAYLOAD_VERSION_V1,
        model_weights_hash: entry.model_weights_hash,
        runtime_hash: entry.runtime_hash,
        // The registered PALW profile is the Q4_K_M GGUF; the registry keys on `(h_M, h_R)` and
        // consensus only requires the discriminant be a known one, so the profile's own
        // quantization is stated here rather than duplicated into the table.
        quantization: QuantizationProfile::Int4,
        input_commitment,
        sampling_seed: sampling_seed_for(input_commitment),
        // A job above the registered ceiling normalizes to zero VLT — clamp rather than mint
        // nothing for work already done.
        max_tokens: max_tokens.clamp(1, entry.max_tokens),
        verification_scheme: VerificationScheme::CanonicalFullReplay,
    }
}

/// What [`new_job_input`] appends to the configured prompt, and the widest it can get: the tag,
/// the 20 digits of `u64::MAX`, and the closing bracket.
const JOB_NONCE_TAG: &[u8] = b"\n[misaka-job@daa:";
pub const JOB_NONCE_MAX_BYTES: usize = JOB_NONCE_TAG.len() + 20 + 1;

/// The input bytes for a **new** job over this node's configured prompt.
///
/// One executor is credited for one `job_id` exactly once: `aggregate_compute_credits` dedups on
/// `(validator_id, job_id)` so that replaying a certificate — which carries perfectly valid
/// signatures — cannot mint twice. And `job_id` is `H(S_j)`, a pure function of the input.
///
/// So a node that committed to the same bytes on every cycle would be credited for its first job
/// and then spend GPU time, two transaction fees and its verifiers' replays on jobs consensus had
/// already counted, forever, while every log line reported a certified job and the quota counted
/// up. The failure is entirely silent from the node's side — the certificates are accepted; they
/// are simply not *added*. A quota of 8 would have been worth 50 VLT, not 400.
///
/// The DAA score is the discriminator because it needs no local state to survive a restart: at
/// certificate time the spec is re-derived from the input the commitment published on chain, so
/// nothing here has to be remembered. Two of this node's jobs cannot share one score — the cycle
/// originates at most one commitment per [`RESUBMIT_GRACE_BLOCKS`], and a commitment cannot even be
/// certified until the *next* epoch's beacon exists — so the sequence is distinct by construction
/// rather than by being unlikely to repeat.
pub fn new_job_input(prompt: &[u8], now_daa: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(prompt.len() + JOB_NONCE_MAX_BYTES);
    out.extend_from_slice(prompt);
    out.extend_from_slice(JOB_NONCE_TAG);
    out.extend_from_slice(now_daa.to_string().as_bytes());
    out.push(b']');
    out
}

/// `s_j`, derived from the input commitment.
///
/// The seed's job is to make sampling deterministic, not to be unpredictable: verifiers read it
/// off the certificate's spec, so nothing depends on where it came from. Deriving it keeps the
/// whole spec a function of the input, which is what makes the commit→certify gap stateless.
fn sampling_seed_for(input_commitment: Hash64) -> [u8; 32] {
    let digest = blake2b_simd::Params::new().hash_length(32).key(SAMPLING_SEED_KEY).hash(input_commitment.as_byte_slice());
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    out
}

/// Read an executor's job input, rejecting one that could not be committed to.
///
/// The ceiling is the consensus one less [`JOB_NONCE_MAX_BYTES`], because what gets committed is
/// [`new_job_input`]'s output rather than the file. A prompt that fits the raw limit but not the
/// suffixed one would be accepted at startup and then rejected by every commitment.
fn load_prompt(path: &PathBuf) -> Result<Vec<u8>, String> {
    const MAX_PROMPT_BYTES: usize = MAX_JOB_INPUT_BYTES - JOB_NONCE_MAX_BYTES;
    let bytes = std::fs::read(path).map_err(|e| format!("could not read the compute prompt at {}: {e}", path.display()))?;
    if bytes.is_empty() || bytes.len() > MAX_PROMPT_BYTES {
        return Err(format!(
            "the compute prompt at {} is {} bytes; a job input must be 1..={MAX_PROMPT_BYTES} \
             (the consensus limit of {MAX_JOB_INPUT_BYTES} less the per-job nonce)",
            path.display(),
            bytes.len()
        ));
    }
    Ok(bytes)
}

/// MISAKA devnet fixture: how many jobs this validator is allowed to originate, and how many it
/// already has.
///
/// The quota is what makes an *asymmetric* weight test possible: five validators running the same
/// fixed job differ only in how many they complete, so the resulting `W_i(E)` differ only by
/// supplied compute. Without it the compute role keeps originating jobs forever and every
/// validator converges on the same weight — a test of nothing.
///
/// **Persisted, deliberately.** Held only in memory, a restart resets the counter and the quota
/// starts again; the weights then keep growing every time a node bounces, and the expected
/// 400/250/150/100/100 quietly becomes 450/300/… A file is the difference between a fixed
/// experiment and a drifting one.
///
/// `plan_id` binds the count to the plan it was counted under — the validator, the target, and the
/// job shape. Change any of them and the old count is not evidence about the new plan, so it is
/// discarded rather than carried over.
/// How far one reserved job has got. A slot is consumed from [`Self::Reserved`] onward.
#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FixtureSlotState {
    /// Written to disk BEFORE the commitment is broadcast. A crash here costs one slot and the
    /// node finishes under target — visible, and the safe direction.
    Reserved,
    /// The commitment transaction was submitted.
    Committed,
    /// A certificate for it was submitted. Terminal.
    Certified,
}

/// MISAKA devnet fixture: which jobs this validator has claimed against its quota, and how far
/// each has got.
///
/// The quota is what makes an *asymmetric* weight test possible: five validators running the same
/// fixed job differ only in how many they complete, so the resulting `W_i(E)` differ only by
/// supplied compute. Without it the compute role keeps originating jobs forever and every
/// validator converges on the same weight — a test of nothing.
///
/// # Keyed by job, not counted by submission
///
/// This was a counter, and the counter went to 6 against a target of 5. Two ways, both of which a
/// count of *submissions* cannot see:
///
/// * a certificate is re-submitted until its transaction is accepted, and each submission
///   incremented the count — one job, several increments;
/// * a slot was only consumed at certification, so a commitment already in flight did not count
///   against the target.
///
/// So the unit is the job, identified by the `job_id` consensus itself dedups on
/// (`aggregate_compute_credits` keys `(validator_id, job_id)`), and the map is keyed by it. Two
/// certificates for one job are one entry, a restart re-certifying an open commitment is one
/// entry, and the quota is `slots.len()` — reserved-or-later, so work in flight counts.
///
/// **Persisted, deliberately.** Held only in memory, a restart resets the count and the quota
/// starts again; the weights then grow every time a node bounces, and the expected
/// 400/250/150/100/100 quietly becomes 450/300/…
///
/// `plan_id` binds the slots to the plan they were claimed under — the validator, the target, and
/// the job shape. Change any of them and the old slots are not evidence about the new plan, so
/// they are discarded rather than carried over.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct FixtureExecutionState {
    pub plan_id: String,
    pub target_jobs: u32,
    /// `job_id` (hex) → how far that job has got. `len()` is the quota consumed.
    #[serde(default)]
    pub slots: std::collections::BTreeMap<String, FixtureSlotState>,
}

impl FixtureExecutionState {
    /// Load the state for `plan_id`, or a fresh one if the file is absent, unreadable, or records
    /// a different plan.
    pub fn load(path: &std::path::Path, plan_id: &str, target_jobs: u32) -> Self {
        let fresh = Self { plan_id: plan_id.to_string(), target_jobs, slots: Default::default() };
        let Ok(raw) = std::fs::read_to_string(path) else { return fresh };
        match serde_json::from_str::<Self>(&raw) {
            Ok(saved) if saved.plan_id == plan_id && saved.target_jobs == target_jobs => saved,
            Ok(saved) => {
                warn!(
                    "[{COMPUTE}] fixture plan changed ({} -> {plan_id}); discarding {} slot(s) claimed under it",
                    saved.plan_id,
                    saved.slots.len()
                );
                fresh
            }
            Err(err) => {
                warn!("[{COMPUTE}] could not parse {}: {err}; starting the fixture quota from zero", path.display());
                fresh
            }
        }
    }

    /// Jobs claimed against the target, in any state. This — not the number certified — is what
    /// the quota is measured against, so a commitment in flight cannot be joined by another.
    pub fn claimed(&self) -> u32 {
        self.slots.len() as u32
    }

    pub fn certified(&self) -> u32 {
        self.slots.values().filter(|s| **s == FixtureSlotState::Certified).count() as u32
    }

    pub fn remaining(&self) -> u32 {
        self.target_jobs.saturating_sub(self.claimed())
    }

    /// Claim a slot for `job_id`, flushing before the caller broadcasts anything.
    ///
    /// `false` means the quota is full. An already-claimed job returns `true` — re-committing the
    /// same job after a lost transaction is a retry of that job, not a new one.
    ///
    /// The flush is deliberately *before* the broadcast, and a failed broadcast does **not**
    /// release the slot. A released slot could be re-used by a second job while the first
    /// transaction was in fact on the wire, which overshoots the target silently; a burnt slot
    /// leaves the node under target, which is visible in its own quota log.
    pub fn reserve(&mut self, job_id: &str, path: &std::path::Path) -> bool {
        if self.slots.contains_key(job_id) {
            return true;
        }
        if self.claimed() >= self.target_jobs {
            return false;
        }
        self.slots.insert(job_id.to_owned(), FixtureSlotState::Reserved);
        self.flush(path);
        true
    }

    /// Advance a claimed job, never backwards. Idempotent: the second certificate for one job
    /// finds it already `Certified` and changes nothing.
    pub fn advance(&mut self, job_id: &str, to: FixtureSlotState, path: &std::path::Path) {
        let rank = |s: FixtureSlotState| match s {
            FixtureSlotState::Reserved => 0,
            FixtureSlotState::Committed => 1,
            FixtureSlotState::Certified => 2,
        };
        match self.slots.get(job_id) {
            Some(current) if rank(*current) >= rank(to) => return,
            // Not claimed: only possible if the state file was lost between reserve and here.
            // Record it rather than drop it — the job exists on chain either way.
            None => {}
            Some(_) => {}
        }
        self.slots.insert(job_id.to_owned(), to);
        self.flush(path);
    }

    /// Flushed immediately rather than at shutdown: a node killed between a state change and the
    /// flush would re-claim the same job on restart, which is precisely the drift this exists to
    /// prevent.
    fn flush(&self, path: &std::path::Path) {
        if let Err(err) = std::fs::write(path, serde_json::to_string(self).unwrap_or_default()) {
            warn!("[{COMPUTE}] could not persist the fixture quota to {}: {err}", path.display());
        }
    }
}

/// Per-transaction-kind record of what this node has already sent and not yet seen on chain.
///
/// Every decision the cycle makes is read back off the chain, so a submitted-but-unmined
/// transaction is indistinguishable from one that was never sent. Without this the heartbeat would
/// re-send the same declaration or commitment on every tick, paying a fee each time.
#[derive(Default)]
pub struct ComputeInflight {
    capability_daa: Option<u64>,
    commitment_daa: Option<u64>,
    /// Keyed by the COMMITMENT a certificate certifies, because that is what stays open until the
    /// certificate is accepted — and what the cycle would otherwise certify again every pass.
    certificate_daa: std::collections::HashMap<kaspa_consensus_core::tx::TransactionId, u64>,
    /// Keyed by the certificate a verdict judges.
    verdict_daa: std::collections::HashMap<kaspa_consensus_core::tx::TransactionId, u64>,
}

impl ComputeInflight {
    /// Whether a transaction of this kind is recent enough that re-sending it would be a
    /// duplicate rather than a retry.
    fn is_recent(submitted: Option<u64>, now_daa: u64) -> bool {
        matches!(submitted, Some(at) if now_daa.saturating_sub(at) < RESUBMIT_GRACE_BLOCKS)
    }

    pub fn capability_recent(&self, now_daa: u64) -> bool {
        Self::is_recent(self.capability_daa, now_daa)
    }

    pub fn commitment_recent(&self, now_daa: u64) -> bool {
        Self::is_recent(self.commitment_daa, now_daa)
    }

    pub fn certificate_recent(&self, commitment_tx_id: kaspa_consensus_core::tx::TransactionId, now_daa: u64) -> bool {
        Self::is_recent(self.certificate_daa.get(&commitment_tx_id).copied(), now_daa)
    }

    pub fn verdict_recent(&self, certificate_tx_id: kaspa_consensus_core::tx::TransactionId, now_daa: u64) -> bool {
        Self::is_recent(self.verdict_daa.get(&certificate_tx_id).copied(), now_daa)
    }

    pub fn note_capability(&mut self, now_daa: u64) {
        self.capability_daa = Some(now_daa);
    }

    pub fn note_commitment(&mut self, now_daa: u64) {
        self.commitment_daa = Some(now_daa);
    }

    pub fn note_certificate(&mut self, commitment_tx_id: kaspa_consensus_core::tx::TransactionId, now_daa: u64) {
        self.certificate_daa.retain(|_, at| now_daa.saturating_sub(*at) < RESUBMIT_GRACE_BLOCKS);
        self.certificate_daa.insert(commitment_tx_id, now_daa);
    }

    pub fn note_verdict(&mut self, certificate_tx_id: kaspa_consensus_core::tx::TransactionId, now_daa: u64) {
        // Bounded by pruning of entries the grace has expired, so a long-running node's map does
        // not grow with every job it has ever audited.
        self.verdict_daa.retain(|_, at| now_daa.saturating_sub(*at) < RESUBMIT_GRACE_BLOCKS);
        self.verdict_daa.insert(certificate_tx_id, now_daa);
    }
}

/// Whether a capability declaration should be (re-)published now, and with what expiry.
///
/// `None` means the current declaration still stands with room to spare. Renewal is driven by the
/// declaration's own remaining validity rather than a fixed cadence, so a network that shortens
/// `max_capability_validity_blocks` automatically gets more frequent renewals.
pub fn capability_expiry_to_declare(current_expiry: Option<u64>, now_daa: u64, validity_blocks: u64) -> Option<u64> {
    let lead = (validity_blocks / CAPABILITY_RENEW_LEAD_FRACTION).max(1);
    match current_expiry {
        Some(expiry) if expiry > now_daa.saturating_add(lead) => None,
        // Consensus caps the declared expiry at `validity_blocks` past the accepting block, so
        // asking for exactly that is asking for the longest declaration the network will grant.
        _ => Some(now_daa.saturating_add(validity_blocks)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::vlt::{job_spec_id, palw_qwen36_metal_entry};

    fn entry() -> ModelCostEntry {
        palw_qwen36_metal_entry()
    }

    /// The executor commits to a job and certifies it later — possibly after a restart. Nothing
    /// but the on-chain input may be needed to rebuild the identical spec, or the certificate
    /// would name a different `job_id` than the commitment and credit nothing.
    #[test]
    fn a_spec_is_reproducible_from_the_published_input_alone() {
        let input = b"the capital of France is";
        let a = job_spec_for(&entry(), input, 512);
        let b = job_spec_for(&entry(), input, 512);
        assert_eq!(a, b);
        assert_eq!(job_spec_id(&a), job_spec_id(&b));
        assert_eq!(a.input_commitment, job_input_commitment(input), "the spec must commit to the input the commitment publishes");

        // A different input is a different job, all the way down to the sortition ticket.
        let other = job_spec_for(&entry(), b"an entirely different prompt", 512);
        assert_ne!(job_spec_id(&a), job_spec_id(&other));
        assert_ne!(a.sampling_seed, other.sampling_seed);
    }

    /// Consensus credits an executor for a `job_id` once and only once, so two jobs from one node
    /// must never be the same job. This is the property that makes a quota of N jobs worth N jobs
    /// of weight instead of one — and its absence would be invisible from the node, which would go
    /// on paying fees for certificates that are accepted and then not counted.
    #[test]
    fn each_new_job_is_a_different_job() {
        let prompt = b"the capital of France is";
        let a = new_job_input(prompt, 1_000);
        let b = new_job_input(prompt, 1_120);
        assert_ne!(a, b);
        assert_ne!(
            job_spec_id(&job_spec_for(&entry(), &a, 512)),
            job_spec_id(&job_spec_for(&entry(), &b, 512)),
            "two jobs sharing a job_id would be credited once between them"
        );
        // Same score ⇒ same job, which is what makes re-committing after a lost transaction a
        // retry of that job rather than a second one.
        assert_eq!(a, new_job_input(prompt, 1_000));
        // The prompt is still the prompt: the nonce is appended, not mixed in.
        assert!(a.starts_with(prompt));

        // A prompt `load_prompt` accepts must still fit the consensus limit once suffixed, at any
        // score — the check at startup is worth nothing if the commitment is refused later.
        let longest = vec![b'x'; MAX_JOB_INPUT_BYTES - JOB_NONCE_MAX_BYTES];
        assert!(new_job_input(&longest, u64::MAX).len() <= MAX_JOB_INPUT_BYTES);
    }

    /// The quota counts JOBS, and the two ways it stopped doing so both produced a real 6/5 on a
    /// running devnet: a slot taken only at certification does not count the commitment already in
    /// flight, and a certificate re-submitted until it mines incremented a counter each time.
    #[test]
    fn a_quota_slot_is_a_job_not_a_submission() {
        let dir = std::env::temp_dir().join(format!("misaka-quota-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fixture-quota.json");
        let _ = std::fs::remove_file(&path);
        let mut st = FixtureExecutionState::load(&path, "plan-a", 2);

        // Reserving consumes the slot immediately — before anything is broadcast — so a job in
        // flight cannot be joined by another.
        assert!(st.reserve("job-1", &path));
        assert_eq!(st.claimed(), 1);
        assert_eq!(st.certified(), 0, "a reserved job is not a certified one");
        assert_eq!(st.remaining(), 1);

        // Re-committing the SAME job after a lost transaction is a retry, not a second job.
        assert!(st.reserve("job-1", &path));
        assert_eq!(st.claimed(), 1);

        assert!(st.reserve("job-2", &path));
        assert_eq!(st.claimed(), 2);
        // Full: an in-flight commitment counts against the target, which is the half a
        // certification-time counter missed.
        assert!(!st.reserve("job-3", &path));
        assert_eq!(st.claimed(), 2);

        // Certifying one job twice — the exact shape of the resubmission that made it 6/5 — is one
        // job.
        st.advance("job-1", FixtureSlotState::Certified, &path);
        st.advance("job-1", FixtureSlotState::Certified, &path);
        assert_eq!(st.certified(), 1);
        assert_eq!(st.claimed(), 2);

        // State never goes backwards: a late "committed" for an already-certified job is ignored,
        // or a restart mid-certification would undo the record it just made.
        st.advance("job-1", FixtureSlotState::Committed, &path);
        assert_eq!(st.certified(), 1);

        // And it survives a restart, keyed by job, so re-certifying an open commitment after a
        // bounce claims nothing new.
        let reloaded = FixtureExecutionState::load(&path, "plan-a", 2);
        assert_eq!(reloaded.claimed(), 2);
        assert_eq!(reloaded.certified(), 1);
        assert!(!reloaded.clone().reserve("job-3", &path));

        // A different plan is a different experiment: the old slots are not evidence about it.
        let fresh = FixtureExecutionState::load(&path, "plan-b", 2);
        assert_eq!(fresh.claimed(), 0);
        // So is the same plan id at a different target — the count means something else.
        let retargeted = FixtureExecutionState::load(&path, "plan-a", 5);
        assert_eq!(retargeted.claimed(), 0);
        let _ = std::fs::remove_file(&path);
    }

    /// A certificate is rebuilt from the chain every pass while its commitment is open, and a
    /// commitment stays open until the certificate is ACCEPTED. Without a grace that is one
    /// transaction fee per heartbeat for as long as the mempool takes.
    #[test]
    fn a_certificate_is_not_resubmitted_until_the_grace_expires() {
        use kaspa_consensus_core::tx::TransactionId;
        let commitment = TransactionId::from_bytes([9u8; 64]);
        let mut inflight = ComputeInflight::default();
        assert!(!inflight.certificate_recent(commitment, 1_000), "nothing sent yet");

        inflight.note_certificate(commitment, 1_000);
        assert!(inflight.certificate_recent(commitment, 1_000));
        assert!(inflight.certificate_recent(commitment, 1_000 + RESUBMIT_GRACE_BLOCKS - 1));
        // Past the grace it is a retry rather than a duplicate: the transaction plainly did not
        // land, and the job is still uncertified.
        assert!(!inflight.certificate_recent(commitment, 1_000 + RESUBMIT_GRACE_BLOCKS));
        // Per commitment, not global — one job in flight must not silence another.
        assert!(!inflight.certificate_recent(TransactionId::from_bytes([8u8; 64]), 1_000));
    }

    /// A spec above the registered ceiling normalizes to zero VLT, so the ceiling is a clamp on
    /// what we ask for rather than a limit we discover after doing the work.
    #[test]
    fn max_tokens_is_clamped_into_the_registered_profile() {
        let e = entry();
        assert_eq!(job_spec_for(&e, b"x", e.max_tokens + 1_000).max_tokens, e.max_tokens);
        assert_eq!(job_spec_for(&e, b"x", 0).max_tokens, 1, "a zero-token job would be no job at all");
        assert_eq!(job_spec_for(&e, b"x", 64).max_tokens, 64);
    }

    #[test]
    fn capability_renews_before_it_lapses_and_not_before() {
        let validity = 1_000u64;
        // Never declared.
        assert_eq!(capability_expiry_to_declare(None, 5_000, validity), Some(6_000));
        // Comfortably live: nothing to do.
        assert_eq!(capability_expiry_to_declare(Some(5_900), 5_000, validity), None);
        // Inside the renewal lead (10% of validity = 100 blocks).
        assert_eq!(capability_expiry_to_declare(Some(5_050), 5_000, validity), Some(6_000));
        // Already lapsed.
        assert_eq!(capability_expiry_to_declare(Some(4_999), 5_000, validity), Some(6_000));
        // A degenerate validity must still produce a lead of at least one block rather than
        // renewing on every heartbeat forever.
        assert_eq!(capability_expiry_to_declare(Some(5_002), 5_000, 1), None);
    }

    /// The chain is the only thing the cycle reads, so an unmined transaction looks exactly like
    /// one that was never sent. The grace is what stops that from becoming a fee leak.
    #[test]
    fn inflight_suppresses_resubmission_until_the_grace_expires() {
        let mut inflight = ComputeInflight::default();
        assert!(!inflight.capability_recent(1_000));
        inflight.note_capability(1_000);
        assert!(inflight.capability_recent(1_000));
        assert!(inflight.capability_recent(1_000 + RESUBMIT_GRACE_BLOCKS - 1));
        assert!(!inflight.capability_recent(1_000 + RESUBMIT_GRACE_BLOCKS), "past the grace, a retry is a retry");

        let cert = kaspa_consensus_core::tx::TransactionId::from_bytes([7u8; 64]);
        let other = kaspa_consensus_core::tx::TransactionId::from_bytes([8u8; 64]);
        inflight.note_verdict(cert, 2_000);
        assert!(inflight.verdict_recent(cert, 2_000));
        assert!(!inflight.verdict_recent(other, 2_000), "the grace is per certificate, not global");

        // Entries are pruned as they age out, so auditing many jobs does not grow the map forever.
        inflight.note_verdict(other, 2_000 + RESUBMIT_GRACE_BLOCKS);
        assert!(!inflight.verdict_recent(cert, 2_000 + RESUBMIT_GRACE_BLOCKS));
        assert_eq!(inflight.verdict_daa.len(), 1);
    }

    /// A node that cannot prove it runs the registered build must not join committees: as a
    /// verifier it would refute honest peers, and its own certificates would mint nothing.
    #[test]
    fn an_unregistered_runtime_leaves_the_role_disabled() {
        let mut vlt = VltParams::INERT;
        vlt.model_cost_table = kaspa_consensus_core::vlt::ModelCostTable::palw_qwen36_metal();
        // No worker binary ⇒ the mock, which never claims the registered identity.
        let cfg = ComputeConfig { enabled: true, ..Default::default() };
        assert!(ComputeRole::new(&cfg, Some(&vlt), Hash64::from_u64_word(7)).is_none());
        // Disabled by flag, and on a network with no overlay at all.
        assert!(ComputeRole::new(&ComputeConfig::default(), Some(&vlt), Hash64::from_u64_word(7)).is_none());
        assert!(ComputeRole::new(&cfg, None, Hash64::from_u64_word(7)).is_none());
    }

    /// An empty model table — every shipped preset — leaves the role dormant without any flag
    /// having to say so.
    #[test]
    fn an_empty_model_table_leaves_the_role_disabled() {
        let cfg = ComputeConfig { enabled: true, ..Default::default() };
        assert!(ComputeRole::new(&cfg, Some(&VltParams::INERT), Hash64::from_u64_word(7)).is_none());
        assert!(VltParams::INERT.model_cost_table.live().is_empty(), "the shipped presets register no model");
    }
}
