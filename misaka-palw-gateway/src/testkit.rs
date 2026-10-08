//! **Test support: an in-process worker the whole chat path can run against.** (`cfg(test)` only.)
//!
//! [`FloorWorker`] is a [`JobRunner`] over the repository's real BASE-0 floor engine
//! (`Base0Backend::execute_free_prompt_streaming`): it takes the SAME `PalwFpWorkerRequestV3` the resident worker takes, builds the
//! job from it the way `fp_worker::prepare_job_v1` does, runs the real engine, retains the capture where the gateway looks for it
//! (`traces/<job id>/`), and answers with a real `PalwFpWorkerResultV3` that the gateway then re-binds with
//! `validate_against_request` exactly as it does a subprocess's. What it is NOT: the A16/Qwen3.6 families' `FpWorkerRuntime` (those
//! refuse the floor's flat logits scheme, and need a tokenizer file) — so its "tokenizer" is byte-level, and the floor's registered
//! context is 12 tokens, which a chat template cannot fit: tests that run it use [`passthrough_template`], a sidecar template that
//! places the user's text and nothing else.
//!
//! The double is faithful on the properties the gateway's tests are about — what is bound into the job, what the stream shows, what
//! is written to the outbox, how many inferences ran — and says nothing about model quality.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_job_context_v3};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PalwFpPromptSegmentV1, PalwFpWorkerInputV3, PalwFpWorkerManifestV1, PalwFpWorkerRequestV3,
    PalwFpWorkerResultV3, PalwFreePromptJobV3, fp_job_id_v3, fp_worker_request_hash_v3,
};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_step::{PALW_STEP_MAX_LEAVES, PalwShapeProfileV3};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use misaka_palw_base0::backend::Base0Backend;

use crate::{AnswerBatchRun, AnswerRun, Config, EmbedRun, Identity, JobRunner, chain, serving};

pub const NETWORK: &[u8] = b"misaka-palw-rc";
/// The floor's id for the test's one control token (outside the byte range, inside the vocabulary).
pub const SPECIAL_ID: u32 = 1_000;
pub const EOG_ID: u32 = 1_001;

/// A hook called after each streamed token with its index (0-based): how a test disconnects a client mid-run.
pub type TokenHook = Box<dyn Fn(usize) + Send + Sync>;

pub struct FloorWorker {
    backend: Base0Backend,
    pub profile: PalwShapeProfileV3,
    manifest: PalwFpWorkerManifestV1,
    form: PalwPromptIdsFormV1,
    trace_out: PathBuf,
    slot: Mutex<()>,
    /// Inferences actually executed (the number a duplicate must not raise).
    pub runs: AtomicUsize,
    pub hook: Mutex<Option<Arc<TokenHook>>>,
    waiting: AtomicUsize,
}

impl FloorWorker {
    pub fn new(trace_out: &Path, form: PalwPromptIdsFormV1) -> Self {
        use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
        let court = PalwCourtParamsV2::new(PALW_STEP_MAX_LEAVES, 4, 2).expect("the shipped court");
        let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor is registered");
        let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
        let backend = Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("the floor resolves from nothing"))
            .with_step_ladder_cap(court.max_step_leaf_count())
            .with_prompt_ids_form(form);
        let profile = backend.profile().clone();
        let manifest = PalwFpWorkerManifestV1 {
            version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_WORKER_MANIFEST_V1_VERSION,
            model_id: "PALW-BASE-0/rc".into(),
            class_id: profile.shape_profile_id(),
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: profile.shape_profile_id(),
            trace_scheme_id: profile.logits_scheme_id,
            tokenizer_id: Hash64::from_u64_word(0x70C0),
            n_ctx: profile.n_ctx,
            prefill_single_batch_cap: profile.n_ctx,
            vocab: profile.vocab_size,
            special_tokens: vec![("<|test_special|>".into(), SPECIAL_ID)],
            eog_token_ids: vec![EOG_ID],
        };
        std::fs::create_dir_all(trace_out).expect("the retention dir");
        Self {
            backend,
            profile,
            manifest,
            form,
            trace_out: trace_out.to_path_buf(),
            slot: Mutex::new(()),
            runs: AtomicUsize::new(0),
            hook: Mutex::new(None),
            waiting: AtomicUsize::new(0),
        }
    }

    /// The real engine, for a seat's replay of what this worker produced.
    pub fn backend(&self) -> &Base0Backend {
        &self.backend
    }

    pub fn set_hook(&self, hook: impl Fn(usize) + Send + Sync + 'static) {
        *self.hook.lock().unwrap() = Some(Arc::new(Box::new(hook)));
    }

    pub fn runs(&self) -> usize {
        self.runs.load(Ordering::Acquire)
    }

    fn ids_of(&self, input: &PalwFpWorkerInputV3) -> Result<Vec<u32>, String> {
        let bytes_to_ids = |bytes: &[u8]| bytes.iter().map(|b| u32::from(*b)).collect::<Vec<u32>>();
        let ids = match input {
            PalwFpWorkerInputV3::Text(bytes) => bytes_to_ids(bytes),
            PalwFpWorkerInputV3::TokenIds(ids) => ids.clone(),
            PalwFpWorkerInputV3::Segments(segments) => {
                let mut out = Vec::new();
                for (at, segment) in segments.iter().enumerate() {
                    match segment {
                        PalwFpPromptSegmentV1::Special(id) => {
                            if *id != SPECIAL_ID && *id != EOG_ID {
                                return Err(format!("segment {at} declares an id this tokenizer does not hold as a control token"));
                            }
                            out.push(*id);
                        }
                        PalwFpPromptSegmentV1::Text(bytes) => out.extend(bytes_to_ids(bytes)),
                    }
                }
                out
            }
        };
        if ids.is_empty() {
            return Err("the prompt encoded to nothing".into());
        }
        Ok(ids)
    }

    /// The worker's half of one job: `fp_worker::run_one_job_v1`'s steps over the floor engine.
    fn execute(&self, request: &PalwFpWorkerRequestV3, request_hash: Hash64, on_token: &mut dyn FnMut(u32, &[u8])) -> Result<PalwFpWorkerResultV3, String> {
        misaka_palw_base0::fp_worker::precheck_request_v1(request)?;
        let m = &self.manifest;
        for (field, ours, theirs) in [
            ("class_id", m.class_id, request.class_id),
            ("shape_profile_id", m.shape_profile_id, request.shape_profile_id),
            ("model_profile_id", m.model_profile_id, request.model_profile_id),
            ("runtime_class_id", m.runtime_class_id, request.runtime_class_id),
            ("runtime_manifest_hash", m.runtime_manifest_hash, request.runtime_manifest_hash),
            ("trace_scheme_id", m.trace_scheme_id, request.trace_scheme_id),
        ] {
            if ours != theirs {
                return Err(format!("{field} mismatch — the request declares a runtime this worker is not"));
            }
        }
        if request.max_context_tokens == 0 || request.max_context_tokens > m.n_ctx {
            return Err(format!("max_context_tokens {} is outside this class's 1..={}", request.max_context_tokens, m.n_ctx));
        }
        let prompt_ids = self.ids_of(&request.input)?;
        if let Some(at) = prompt_ids.iter().position(|t| *t >= m.vocab) {
            return Err(format!("the prompt token at position {at} is outside the model's vocab ({})", m.vocab));
        }
        // Stop strings: each encoded alone, byte-level, into the decode config's token sequences.
        let decode = match &request.decode {
            None => None,
            Some(asked) => {
                let mut decode = asked.clone();
                for text in &request.stop_texts {
                    let ids: Vec<u32> = text.iter().map(|b| u32::from(*b)).collect();
                    if ids.is_empty() {
                        return Err("a stop string encodes to no token".into());
                    }
                    decode.stop_sequences.push(ids);
                }
                decode.stop_sequences.sort();
                decode.stop_sequences.dedup();
                decode.validate_canonical().map_err(|e| format!("the job's decode config is not canonical: {e}"))?;
                Some(decode)
            }
        };
        let prefill = prompt_ids.len() as u32;
        if u64::from(prefill) + u64::from(request.decode_token_limit) > u64::from(request.max_context_tokens) {
            return Err(format!("prompt {prefill} + decode ceiling {} exceeds max_context_tokens {}", request.decode_token_limit, request.max_context_tokens));
        }
        let job = PalwFreePromptJobV3 {
            version: request.version,
            network_domain: request.network_domain,
            class_id: request.class_id,
            executor_bond: request.executor_bond,
            executor_pubkey: request.executor_pubkey.clone(),
            operator_id: request.operator_id,
            anchor_block: request.anchor_block,
            anchor_daa: request.anchor_daa,
            job_nonce: request.job_nonce,
            tokenizer_id: m.tokenizer_id,
            prompt_token_ids_hash: prompt_token_ids_commitment_v1(self.form, &prompt_ids).map_err(|e| format!("the prompt does not commit: {e}"))?,
            prompt_tokens: prefill,
            decode_token_limit: request.decode_token_limit,
            max_context_tokens: request.max_context_tokens,
            privacy_mode: request.privacy_mode,
            prompt_mode: request.prompt_mode,
            sampling_seed: request.sampling_seed,
            temperature_q: request.temperature_q,
            decode,
            tail: None,
        };
        let binding = fp_job_id_v3(&job);
        let prompt_usize: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
        let hook = self.hook.lock().unwrap().clone();
        let mut index = 0usize;
        let mut sink = |id: u32| {
            let piece: Vec<u8> = if id < 256 { vec![id as u8] } else { Vec::new() };
            on_token(id, &piece);
            if let Some(hook) = &hook {
                hook(index);
            }
            index += 1;
        };
        let run = self.backend.execute_free_prompt_streaming(&job, &prompt_usize, &mut sink)?;
        let class_facts = PalwFpClassFactsV3 {
            model_profile_id: m.model_profile_id,
            runtime_manifest_hash: m.runtime_manifest_hash,
            runtime_class_id: m.runtime_class_id,
            shape_profile_id: m.shape_profile_id,
            cu_ruleset_id: Hash64::default(),
        };
        let context = palw_fp_job_context_v3(&job, &class_facts, &run.facts, NETWORK).map_err(|e| format!("the finished run implies no context: {e:?}"))?;
        let (schedule_root, _) = kaspa_consensus_core::palw_v2::expected_schedule_commitment_v2(&context.context_hash(), job.prompt_tokens, run.facts.decode_tokens_executed);
        // Retention, before the result frame exists (`fp_worker::retain_v1`'s files).
        let dir = self.trace_out.join(faster_hex::hex_string(binding.as_byte_slice()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("material.bin"), &run.outcome.material).map_err(|e| e.to_string())?;
        let context_borsh = borsh::to_vec(&context).map_err(|e| e.to_string())?;
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema": "misaka.palw.fp-v3-floor-retention.v1",
                "trace_binding": faster_hex::hex_string(binding.as_byte_slice()),
                "job_context_hash": faster_hex::hex_string(context.context_hash().as_byte_slice()),
                misaka_palw_base0::fp_worker::RETENTION_JOB_CONTEXT_FIELD_V1: faster_hex::hex_string(&context_borsh),
                "family": "floor-test",
            }))
            .unwrap(),
        )
        .map_err(|e| e.to_string())?;
        let rendered: Vec<u8> = run.output_token_ids.iter().filter(|id| **id < 256).map(|id| *id as u8).collect();
        Ok(PalwFpWorkerResultV3 {
            version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
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
            stop_reason: run.facts.stop_reason,
            output_token_ids: run.output_token_ids,
            rendered,
            model_load_ms: 0,
            execute_ms: 0,
        })
    }
}

impl JobRunner for FloorWorker {
    fn manifest(&self) -> &PalwFpWorkerManifestV1 {
        &self.manifest
    }
    fn processes(&self) -> usize {
        1
    }
    fn waiting(&self) -> usize {
        self.waiting.load(Ordering::Acquire)
    }
    fn answer_only_supported(&self) -> bool {
        false
    }
    fn run(
        &self,
        request: &PalwFpWorkerRequestV3,
        prompt_ids_form: PalwPromptIdsFormV1,
        on_token: &mut dyn FnMut(u32, &[u8]),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<PalwFpWorkerResultV3, String> {
        self.waiting.fetch_add(1, Ordering::AcqRel);
        let slot = self.slot.lock().unwrap();
        self.waiting.fetch_sub(1, Ordering::AcqRel);
        if cancelled() {
            return Err(serving::CANCELLED_BY_CLIENT.to_string());
        }
        self.runs.fetch_add(1, Ordering::AcqRel);
        let payload = borsh::to_vec(request).map_err(|e| format!("cannot serialize the worker request: {e}"))?;
        let request_hash = fp_worker_request_hash_v3(&payload);
        let result = self.execute(request, request_hash, on_token).map_err(|e| format!("the worker refused the job: {e}"))?;
        drop(slot);
        // The caller-side re-binding, exactly as `ResidentWorker::run_job` makes it.
        result
            .validate_against_request(request, request_hash, prompt_ids_form)
            .map_err(|e| format!("the worker result does not bind the request: {e}"))?;
        Ok(result)
    }
    fn run_answer(&self, _: &PalwFpWorkerRequestV3, _: &mut dyn FnMut(u32, &[u8]), _: &dyn Fn() -> bool) -> Result<AnswerRun, String> {
        Ok(AnswerRun::Unsupported)
    }
    fn run_answer_batch(&self, _: &[PalwFpWorkerRequestV3], _: &mut dyn FnMut(usize, u32, &[u8])) -> Result<AnswerBatchRun, String> {
        Ok(AnswerBatchRun::Unsupported)
    }
    fn run_embed(&self, _: &kaspa_consensus_core::palw_freeprompt_v3::PalwFpEmbedRequestV1) -> Result<EmbedRun, String> {
        Ok(EmbedRun::Unsupported)
    }
}

/// A sidecar chat template that places the user's text and nothing else: the floor's whole context is 12 tokens.
pub fn passthrough_template() -> crate::SidecarRuntime {
    use misaka_palw_base0::sidecar::{CHAT_TEMPLATE_SCHEMA_V1, ChatTemplateSpecV1, RoleTemplateV1};
    let plain = || RoleTemplateV1 { prefix: Vec::new(), suffix: Vec::new() };
    crate::SidecarRuntime {
        digest: "passthrough-test-template".into(),
        path: PathBuf::from("/nonexistent/sidecar"),
        template: Some(ChatTemplateSpecV1 {
            schema: CHAT_TEMPLATE_SCHEMA_V1.into(),
            default_system: None,
            system: plain(),
            user: plain(),
            assistant: plain(),
            generation_prefix: Vec::new(),
        }),
        template_ids: Some(("test/passthrough/v1", "test/passthrough-tools/v1")),
        generation: None,
    }
}

pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("misaka-gw-{name}-{}-{}", std::process::id(), next_id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temp dir");
    dir
}

fn next_id() -> usize {
    static N: AtomicUsize = AtomicUsize::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

/// A gateway config over `outbox` that commits: the bond has room, the public-job budget is the whole room, nothing is capped
/// tighter than the floor needs. The passthrough template is installed.
pub fn config(outbox: &Path) -> Config {
    Config {
        listen: "127.0.0.1:0".into(),
        worker: PathBuf::from("/nonexistent/worker"),
        outbox: outbox.to_path_buf(),
        identity_path: PathBuf::from("/nonexistent/identity.json"),
        class_leaves: 0,
        max_decode_default: 4,
        max_decode_cap: 8,
        trace_retention_window_daa: 500_000,
        derive_seed: None,
        artifact_inline_max: 4 << 20,
        workdir: std::env::temp_dir(),
        max_prompt_bytes: 64,
        bond_exposure_room_sompi: 0,
        public_job_budget_permille: 1_000,
        claim_exposure_sompi: 0,
        answer_never_commit: false,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        per_source_jobs_per_window: 1_000,
        confinement: misaka_palw::host_security::Confinement::none(),
        booted_at_unix: 0,
        worker_processes: 1,
        worker_args: Vec::new(),
        answer_fast_path: false,
        max_connections_per_source: 8,
        max_jobs_per_source: 4,
        sidecar: Some(passthrough_template()),
        cancel_on_disconnect: true,
        finality_depth: crate::status::DEFAULT_FINALITY_DEPTH,
        evidence_providers: Vec::new(),
        evidence_min_copies: 1,
    }
}

/// The floor's identity as the chain fixtures know it.
pub fn identity(profile: &PalwShapeProfileV3) -> Identity {
    let bond = TransactionOutpoint::new(TransactionId::from_u64_word(0xA77A_0001), 0);
    Identity {
        network_domain: Hash64::from_u64_word(0xD0D0),
        class_id: profile.shape_profile_id(),
        class_id_hex: faster_hex::hex_string(profile.shape_profile_id().as_byte_slice()),
        bond_txid_hex: faster_hex::hex_string(bond.transaction_id.as_bytes().as_slice()),
        executor_bond: bond,
        executor_pubkey: vec![0xA7; 32],
        operator_id: Hash64::from_u64_word(0x0B0B),
    }
}

/// Chain facts of a network where this gateway's class is certified and its bond has room.
pub fn certified_facts(decode_rules: bool) -> chain::ChainFacts {
    chain::ChainFacts {
        source: "test".into(),
        live: true,
        registered: true,
        fp_certified: true,
        bond_known: true,
        bond_active: true,
        exposure_room_sompi: 1_000_000_000,
        claim_exposure_sompi: 1_000,
        class_canonical_leaves: 1_000,
        fp_quanta_per_canonical_job: 8,
        fp_max_quanta_per_receipt: 64,
        fp_decode_rules_armed: decode_rules,
        prompt_ids_merkle: true,
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 100,
        ..Default::default()
    }
}

/// An anchor-file chain source (never asked for a price, never submits): what the test's `handle_chat` is handed.
pub fn offline_source(dir: &Path) -> chain::ChainSource {
    let path = dir.join("anchor.json");
    std::fs::write(&path, format!(r#"{{"anchor_block":"{}","anchor_daa":100}}"#, "ab".repeat(64))).unwrap();
    chain::ChainSource::AnchorFile(path)
}

/// Drive one chat request through the real `handle_chat` against `worker`.
pub fn chat(
    config: &Config,
    identity: &Identity,
    worker: &dyn JobRunner,
    facts: &chain::ChainFacts,
    source: &chain::ChainSource,
    body: &serde_json::Value,
    link: &dyn serving::ClientLink,
) -> Result<serde_json::Value, String> {
    let (chat_request, admitted) = crate::surface::parse_and_admit_with(&serde_json::to_vec(body).unwrap(), facts, |c| {
        config.sidecar.as_ref().map(|s| s.apply_defaults(c, facts))
    })
    .map(|(c, mut a, report)| {
        a.sidecar_report = report;
        (c, a)
    })?;
    let budget = Mutex::new(crate::PublicJobBudget::new());
    let mut sink = crate::BufferedSink;
    let ctx = crate::RequestCtx { link, request_digest: crate::idempotency::request_digest(&serde_json::to_vec(body).unwrap()).ok() };
    crate::handle_chat(config, identity, worker, &budget, facts, source, &chat_request, &admitted, admitted.sampling, &mut sink, &ctx)
}
