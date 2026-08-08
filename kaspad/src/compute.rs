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
//! # Why an unregistered runtime disables the whole role
//!
//! A node whose runtime is not the consensus-registered build mints nothing (the model table
//! lookup fails) and, if it were drawn as a verifier, would compute a different `R_j` than an
//! honest executor and sign `Refuted` — zeroing honest credit and arming `ForgedReceipt` slashing
//! against an honest party. So [`ComputeRole::new`] refuses to enable the role at all rather than
//! participate approximately. In particular [`MockRuntime`] never claims the registered identity,
//! so a node without a real worker binary is inert by construction rather than by configuration.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kaspa_consensus_core::vlt::{
    LlmJobSpec, MAX_JOB_INPUT_BYTES, ModelCostEntry, QuantizationProfile, VLT_PAYLOAD_VERSION_V1, VerificationScheme, VltParams,
    job_input_commitment,
};
use kaspa_core::{info, warn};
use kaspa_hashes::Hash64;
use misaka_palw::{ComputeRuntime, MatchProjection, MockRuntime, PalwError, PalwWorkerConfig, PalwWorkerRuntime};

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
}

impl ComputeRole {
    /// Resolve the compute role, or `None` with a logged reason.
    ///
    /// `vlt` is the network's VLT parameters; the model table is what decides whether the runtime
    /// this node is actually running is one consensus knows about. On every shipped preset that
    /// table is empty, so this returns `None` and the role stays dormant without any flag saying so.
    pub fn new(cfg: &ComputeConfig, vlt: Option<&VltParams>) -> Option<Self> {
        if !cfg.enabled {
            return None;
        }
        let Some(vlt) = vlt else {
            warn!("[{COMPUTE}] --enable-compute set, but this network has no DNS overlay configured; compute role disabled");
            return None;
        };

        let runtime: Arc<dyn ComputeRuntime> = match &cfg.worker_bin {
            Some(bin) => Arc::new(PalwWorkerRuntime::new(PalwWorkerConfig {
                worker_bin: bin.clone(),
                work_dir: cfg.work_dir.clone(),
                timeout: cfg.timeout,
            })),
            None => {
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
        info!(
            "[{COMPUTE}] enabled: runtime={} class={} max_tokens={} auto_challenge={}",
            identity.runtime_hash,
            entry.runtime_class_id,
            cfg.max_tokens.min(entry.max_tokens),
            cfg.auto_challenge
        );
        Some(Self { runtime, entry, prompt, max_tokens: cfg.max_tokens.min(entry.max_tokens), auto_challenge: cfg.auto_challenge })
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
fn load_prompt(path: &PathBuf) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("could not read the compute prompt at {}: {e}", path.display()))?;
    if bytes.is_empty() || bytes.len() > MAX_JOB_INPUT_BYTES {
        return Err(format!(
            "the compute prompt at {} is {} bytes; a job input must be 1..={MAX_JOB_INPUT_BYTES}",
            path.display(),
            bytes.len()
        ));
    }
    Ok(bytes)
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

    pub fn verdict_recent(&self, certificate_tx_id: kaspa_consensus_core::tx::TransactionId, now_daa: u64) -> bool {
        Self::is_recent(self.verdict_daa.get(&certificate_tx_id).copied(), now_daa)
    }

    pub fn note_capability(&mut self, now_daa: u64) {
        self.capability_daa = Some(now_daa);
    }

    pub fn note_commitment(&mut self, now_daa: u64) {
        self.commitment_daa = Some(now_daa);
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
        assert!(ComputeRole::new(&cfg, Some(&vlt)).is_none());
        // Disabled by flag, and on a network with no overlay at all.
        assert!(ComputeRole::new(&ComputeConfig::default(), Some(&vlt)).is_none());
        assert!(ComputeRole::new(&cfg, None).is_none());
    }

    /// An empty model table — every shipped preset — leaves the role dormant without any flag
    /// having to say so.
    #[test]
    fn an_empty_model_table_leaves_the_role_disabled() {
        let cfg = ComputeConfig { enabled: true, ..Default::default() };
        assert!(ComputeRole::new(&cfg, Some(&VltParams::INERT)).is_none());
        assert!(VltParams::INERT.model_cost_table.live().is_empty(), "the shipped presets register no model");
    }
}
