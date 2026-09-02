//! **The A16 dense tier's execution backend** — what a node runs when the chain names the
//! Qwen2.5 A16 class.
//!
//! The tier already had an engine (`engine_a16`) and a fidelity story (Qwen2.5-1.5B is FAITHFUL
//! against its float reference and answers "The capital of France is Paris."). What it did not
//! have was a way to turn a running model into a CLAIM: an anchor-derived job, four committed
//! roots, and material a seat can check. That is this file, and it is deliberately the same shape
//! as `qwen36_backend` — a producer, a seat and a court reach an execution through three verbs, and
//! two families that answered them differently would be two protocols.
//!
//! # What it commits, and why it is the tiled scheme
//!
//! `full_logits_trace_root` is [`tiled_logits_trace_root_v1`] over the SELECTING rows — one row per
//! generated token, the row that token was chosen from. The class registers the tiled scheme
//! because at vocabulary 151,936 a flat pin row is 607,744 bytes against a carrier that holds
//! 81,920: a flat commitment would be a class whose every decode-token dispute is inadmissible.
//! The producer building the scheme the class registers is not an implementation detail — a class
//! that commits what it cannot produce mints and then makes no blocks.

use crate::artifact::Base0ArtifactV1;
use crate::engine_a16::{A16Cache, A16Engine, A16PlanErrorV1};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::{
    PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, output_commitment_v2, prompt_token_ids_hash_v2,
};
use kaspa_hashes::Hash64;

pub const QWEN25_A16_DOMAIN_EXECUTION: &[u8] = b"misaka-palw/qwen25-a16/execution/v1";
pub const QWEN25_A16_DOMAIN_JOB_PROMPT: &[u8] = b"misaka-palw/qwen25-a16/job-prompt/v1";
pub const QWEN25_A16_DOMAIN_MANIFEST: &[u8] = b"misaka-palw/qwen25-a16/manifest/v1";

fn keyed(domain: &'static [u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for p in parts {
        h.update(&(p.len() as u64).to_le_bytes());
        h.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **ADR-0078 X6: the family's rendered-output hash, as a public rule.** `output_root` is
/// `output_commitment_v2(job_context_hash, output_token_ids, rendered)`, and this family's
/// `rendered` is a keyed hash of the output ids — a pure function of the ids, so a consumer who
/// holds the answer's ids and the job's context hash recomputes the claim's `output_root` without
/// the model. Exported so the verifier does not have to restate the rule.
pub fn rendered_output_hash_v1(generated: &[u32]) -> Hash64 {
    keyed(QWEN25_A16_DOMAIN_EXECUTION, &[b"rendered", &generated.iter().flat_map(|t| t.to_le_bytes()).collect::<Vec<_>>()])
}

/// One execution: every logits row it produced, and the tokens it selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Qwen25A16RunV1 {
    pub logits_rows: Vec<Vec<i32>>,
    pub generated: Vec<u32>,
}

/// **The prompt an anchor implies.** A producer must not choose its own input — a class whose
/// executor picks the prompt is a class where "run the model" and "search for an input whose
/// output I like" are the same move — so the ids are derived from the anchor and the vocabulary.
pub fn qwen25_a16_prompt_for_anchor(anchor: Hash64, vocab: usize, prefill: u32) -> Vec<usize> {
    let mut prompt = Vec::with_capacity(prefill as usize);
    let mut counter = 0u64;
    while prompt.len() < prefill as usize {
        let block = keyed(QWEN25_A16_DOMAIN_JOB_PROMPT, &[anchor.as_byte_slice(), &counter.to_le_bytes()]);
        for word in block.as_byte_slice().chunks_exact(8) {
            if prompt.len() == prefill as usize {
                break;
            }
            let v = u64::from_le_bytes(word.try_into().expect("chunks_exact(8)"));
            prompt.push((v % vocab.max(1) as u64) as usize);
        }
        counter += 1;
    }
    prompt
}

/// The four roots, from one run — the same decomposition every family uses, because the header
/// does not know which family produced them.
/// `None` when the run does not carry the rows it claims to have selected from — the same refusal
/// `qwen36_roots_v1` makes, for the same reason and against the same gossiped-material attack.
pub fn qwen25_a16_roots_v1(
    job: &PalwJobContextV2,
    shape_id: Hash64,
    run: &Qwen25A16RunV1,
) -> Option<(Hash64, Hash64, Hash64, Hash64)> {
    let context = job.context_hash();
    let prefill = job.declared_prefill_tokens as usize;
    // **A missing row is a refusal, never an empty one** (ADR-0068 launch audit; see the twin
    // comment in `qwen36_roots_v1` for the full path). `unwrap_or_default()` fabricated a row with
    // no lanes, which tiles to a zero-leaf tree, which `step_merkle_root_v1` refuses under an
    // `.expect` — inside the panel service, on material anyone may gossip without a bond.
    if run.generated.is_empty() {
        return None;
    }
    let mut selecting: Vec<Vec<i32>> = Vec::with_capacity(run.generated.len());
    for i in 0..run.generated.len() {
        let row = run.logits_rows.get(prefill.saturating_sub(1) + i)?;
        if row.is_empty() {
            return None;
        }
        selecting.push(row.clone());
    }
    debug_assert!(
        selecting
            .iter()
            .zip(&run.generated)
            .all(|(row, t)| kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(row) as u32 == *t),
        "every committed token is its own row's argmax — the property the close adjudicates"
    );
    let trace_root = kaspa_consensus_core::palw_step_refute::tiled_logits_trace_root_v1(job, &selecting, &run.generated)?;
    let rendered =
        keyed(QWEN25_A16_DOMAIN_EXECUTION, &[b"rendered", &run.generated.iter().flat_map(|t| t.to_le_bytes()).collect::<Vec<_>>()]);
    let output_root = output_commitment_v2(&context, &run.generated, &rendered);
    let execution_root = keyed(
        QWEN25_A16_DOMAIN_EXECUTION,
        &[context.as_byte_slice(), shape_id.as_byte_slice(), trace_root.as_byte_slice(), output_root.as_byte_slice()],
    );
    // The consensus derivation (ADR-0072 Decision 8), the same one `execute` commits to — a seat
    // that recomputed this family's old domain hash here would refuse every honest claim.
    let manifest = kaspa_consensus_core::palw_attempt_v2::attempt_trace_manifest_root_v1(trace_root, 1);
    Some((trace_root, output_root, execution_root, manifest))
}

/// **The A16 tier's captured attempt: run, capture every declared step, commit the binding.**
///
/// The same object the floor's [`crate::produce::base0_execute_for_attempt_v1`] returns, because
/// it answers the same three verbs: `execution_root` is the step binding's own commitment (what a
/// court pins a refutation against), the retained material is the family codec's tuple, and the
/// tiles/checkpoints are what a rung or a close is later assembled from. What differs is the
/// family: the engine is [`A16Engine`], the trace scheme is the TILED one (at vocabulary 151,936 a
/// flat pin row cannot ride the close carrier), and the logits retention follows the floor's
/// convention — the SELECTING rows only, one per generated token, which are exactly the rows the
/// tiled root commits.
///
/// Refuses (rather than degrading) for a class whose profile cannot carry a capture: a graph that
/// does not name every narrowing (ADR-0049 Decision F, probed when no plan proves it
/// structurally), or a state map that cannot describe an `i32` cache (the v1 map). The v1
/// registered class therefore cannot reach this path — which is the honest statement of why
/// `qwen25_a16_profile_v2` exists.
pub fn a16_execute_for_attempt_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    plan: Option<&crate::engine_a16::A16ProfilePlanV1>,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
) -> Result<crate::produce::Base0ExecutionV1, String> {
    a16_execute_for_attempt_streaming_v1(artifact, profile, plan, ctx, prompt, &mut |_| {})
}

/// **The same capture, with each id handed over as it is SELECTED** (ADR-0077 Decision 2).
///
/// The streaming verb is the loop; the non-streaming one is the loop with a callback that does
/// nothing. The other way round — running the job, then replaying the committed ids to the stream
/// — is not streaming, and a second inference to produce the stream is the exact failure Decision
/// 2 forbids: a worker that shows one answer and commits another.
pub fn a16_execute_for_attempt_streaming_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    plan: Option<&crate::engine_a16::A16ProfilePlanV1>,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    on_token: &mut dyn FnMut(u32),
) -> Result<crate::produce::Base0ExecutionV1, String> {
    use kaspa_consensus_core::palw_state_chunk_map as map;

    let prefill = ctx.declared_prefill_tokens as usize;
    let decode_tokens = ctx.exact_decode_tokens as usize;
    if prefill == 0 || decode_tokens == 0 {
        return Err("an empty job is not a job".to_string());
    }
    if prompt.len() < prefill {
        return Err(format!("the job declares {prefill} prefill tokens and {} were supplied", prompt.len()));
    }
    let vocab = artifact.shape.vocab;
    if let Some(bad) = prompt.iter().take(prefill).find(|t| **t >= vocab) {
        return Err(format!("token {bad} is outside this class's vocabulary of {vocab}"));
    }

    let engine = A16Engine::new(artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
    // **ADR-0049 Decision F's obligation, before the first committed leaf.** Under a
    // registered-profile plan the correspondence is structural — the declaration IS the program —
    // and the probe would burn a forward pass re-proving a constructor invariant. Without one, the
    // engine and the profile are two authorities, and the probe is what compares them.
    if plan.is_none() {
        let probe = engine
            .forward_token_traced(&mut A16Cache::new(artifact.shape.n_layers), prompt[0], 0)
            .map_err(|e| format!("probing the graph: {e:?}"))?
            .1;
        let (declared_pre, recorded_pre) = (profile.pre_nodes.len(), probe.pre.len());
        let declared_attn = profile.attn_nodes.len();
        let recorded_attn = probe.attn.first().map(Vec::len).unwrap_or(0);
        if recorded_pre != declared_pre || recorded_attn != declared_attn {
            return Err(format!(
                "this class's registered graph does not name every narrowing its engine performs (ADR-0049 Decision F): pre \
                 declares {declared_pre} node(s) and the engine records {recorded_pre} — the embedding gather and the requant \
                 that lifts it onto the A16 stream, of which only the gather is declared; per-layer declares {declared_attn} \
                 against {recorded_attn} recorded. Committing a step leg under this profile would commit arithmetic the court \
                 recomputes differently."
            ));
        }
    }

    let forward = |engine: &A16Engine<'_>,
                   cache: &mut A16Cache,
                   token: usize,
                   position: usize|
     -> Result<(Vec<i32>, crate::engine_a16::A16TraceV1), String> {
        match plan {
            Some(plan) => engine.forward_token_planned(plan, cache, token, position).map_err(|e| format!("planned forward: {e:?}")),
            None => engine.forward_token_traced(cache, token, position).map_err(|e| format!("forward: {e:?}")),
        }
    };

    let leaf_count = kaspa_consensus_core::palw_step::step_leaf_count(profile, ctx).map_err(|e| format!("{e:?}"))?;
    let mut capture = crate::legs::Base0StepCaptureV1::new(leaf_count).map_err(|e| format!("{e:?}"))?;
    let checkpoint_profile = map::integer_kv_checkpoint_profile_v1(map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1);
    let mut checkpoints = crate::legs::Base0CheckpointCaptureV1::new(ctx, profile, &checkpoint_profile);
    let mut cache = A16Cache::new(artifact.shape.n_layers);

    let mut logits_rows: Vec<Vec<i32>> = Vec::with_capacity(decode_tokens);
    let mut generated: Vec<u32> = Vec::with_capacity(decode_tokens);

    // Call 0 — prefill. Logits leaves exist only at its LAST position; the earlier rows predict
    // tokens the prompt already contains, so the capture drops their Post rows (steps this class's
    // step space does not have) and the retention keeps only the selecting row.
    let mut last_logits = Vec::new();
    for (position, token) in prompt.iter().take(prefill).enumerate() {
        let (logits, trace) = forward(&engine, &mut cache, *token, position).map_err(|e| format!("prefill at {position}: {e}"))?;
        let mut rows = crate::legs::a16_captured_rows_v1(&trace);
        if position + 1 != prefill {
            rows.retain(|r| r.table != kaspa_consensus_core::palw_step::PalwStepTableV1::Post);
        }
        capture.push_call(profile, ctx, 0, position as u32, &rows).map_err(|e| format!("{e:?}"))?;
        last_logits = logits;
    }
    let mut next = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&last_logits) as u32;
    generated.push(next);
    on_token(next);
    logits_rows.push(last_logits);

    // Calls 1..=D−1 — decode. The COORDINATE's position is 0 in every decode call (each call has
    // one position); the cache position is absolute. Conflating them lands every decode row on
    // top of the first one's.
    for call in 1..decode_tokens {
        let cache_position = prefill + call - 1;
        let (logits, trace) =
            forward(&engine, &mut cache, next as usize, cache_position).map_err(|e| format!("decode at {cache_position}: {e}"))?;
        let rows = crate::legs::a16_captured_rows_v1(&trace);
        capture.push_call(profile, ctx, call as u32, 0, &rows).map_err(|e| format!("{e:?}"))?;
        next = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&logits) as u32;
        generated.push(next);
        on_token(next);
        logits_rows.push(logits);
        if call as u32 == checkpoints.next_covered_decode_call() {
            // Through the CACHE's own serializer, at the width the class declares. Under a map
            // that cannot describe this state — the v1 one-byte map over an `i32` cache — this
            // refuses, and the run fails here rather than committing a checkpoint that opens to a
            // state it never held.
            let geometry = checkpoints.next_geometry().map_err(|e| format!("{e:?}"))?;
            let mut chunks = Vec::with_capacity(geometry.chunk_count() as usize);
            for index in 0..geometry.chunk_count() {
                let entry =
                    map::integer_kv_state_chunk_entry_v1(&geometry, index).ok_or_else(|| format!("the map has no chunk {index}"))?;
                chunks.push(cache.state_chunk_bytes_v1(&entry).ok_or_else(|| {
                    format!(
                        "this cache does not fit the state map the class declares (chunk {index}, {} bytes per row)",
                        entry.row_bytes
                    )
                })?);
            }
            checkpoints.push_chunks(chunks).map_err(|e| format!("{e:?}"))?;
        }
    }

    let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    let checkpoints = checkpoints.finish(decode_calls / checkpoint_profile.checkpoint_interval).map_err(|e| format!("{e:?}"))?;
    let tiles = capture.finish().map_err(|e| format!("{e:?}"))?;

    // **This class's own trace scheme, not the floor's.** The retained rows ARE the selecting
    // rows (row `r` is the one `generated[r]` was chosen from), so the tiled root commits them
    // directly and a seat recomputes it from the same retention.
    let trace_root = kaspa_consensus_core::palw_step_refute::tiled_logits_trace_root_v1(ctx, &logits_rows, &generated)
        .ok_or_else(|| "the retained rows build no tree".to_string())?;
    let activation_leg_root = crate::produce::base0_activation_leg_root_v1(ctx);
    let binding = crate::legs::base0_binding_from_capture_v1(profile, ctx, &tiles, &checkpoints, trace_root, activation_leg_root)
        .map_err(|e| format!("{e:?}"))?;

    let context = ctx.context_hash();
    let rendered = rendered_output_hash_v1(&generated);
    let output_root = output_commitment_v2(&context, &generated, &rendered);
    // The consensus derivation (ADR-0072 Decision 8): admission pins the manifest root to
    // `attempt_trace_manifest_root_v1(trace_root, 1)`, whichever family produced it.
    let trace_manifest_root = kaspa_consensus_core::palw_attempt_v2::attempt_trace_manifest_root_v1(trace_root, 1);

    Ok(crate::produce::Base0ExecutionV1 {
        trace_root,
        output_root,
        execution_root: binding.committed_execution_root,
        trace_manifest_root,
        trace_chunk_count: 1,
        binding,
        tiles,
        checkpoints,
        logits_rows,
        generated_token_ids: generated,
    })
}

pub fn qwen25_a16_material_encode_v1(run: &Qwen25A16RunV1) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + run.logits_rows.iter().map(|r| r.len() * 4 + 8).sum::<usize>());
    out.extend_from_slice(&(run.logits_rows.len() as u64).to_le_bytes());
    for row in &run.logits_rows {
        out.extend_from_slice(&(row.len() as u64).to_le_bytes());
        for v in row {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend_from_slice(&(run.generated.len() as u64).to_le_bytes());
    for t in &run.generated {
        out.extend_from_slice(&t.to_le_bytes());
    }
    out
}

pub fn qwen25_a16_material_decode_v1(bytes: &[u8]) -> Option<Qwen25A16RunV1> {
    let mut i = 0usize;
    let u64_at = |i: &mut usize| -> Option<u64> {
        let end = i.checked_add(8)?;
        if end > bytes.len() {
            return None;
        }
        let v = u64::from_le_bytes(bytes[*i..end].try_into().ok()?);
        *i = end;
        Some(v)
    };
    let rows = u64_at(&mut i)? as usize;
    // A length prefix is an allocation instruction from a stranger: every count is checked against
    // the bytes actually present before a vector is reserved.
    if rows > bytes.len() {
        return None;
    }
    let mut logits_rows = Vec::with_capacity(rows.min(1024));
    for _ in 0..rows {
        let len = u64_at(&mut i)? as usize;
        let end = i.checked_add(len.checked_mul(4)?)?;
        if end > bytes.len() {
            return None;
        }
        logits_rows.push(bytes[i..end].chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect());
        i = end;
    }
    let count = u64_at(&mut i)? as usize;
    let end = i.checked_add(count.checked_mul(4)?)?;
    if end > bytes.len() {
        return None;
    }
    let generated = bytes[i..end].chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    i = end;
    if i != bytes.len() {
        return None; // trailing bytes are a different encoding, not this one
    }
    Some(Qwen25A16RunV1 { logits_rows, generated })
}

pub struct Qwen25A16Backend {
    artifact: std::sync::Arc<Base0ArtifactV1>,
    model_id: String,
    network_id: Vec<u8>,
    /// The artifact's own digest — what the class's roots are taken over.
    shape_id: Hash64,
    /// The CHAIN's class id (the shape profile's), which the job must name.
    /// **The class's graph, not just its id.** The id is `profile.shape_profile_id()`, so holding
    /// the profile cannot disagree with the class this backend claims to serve — and the
    /// free-prompt path needs the graph itself: the step space, the checkpoint layout and the
    /// state chunk map the class declares all come from here.
    profile: PalwShapeProfileV3,
    class_profile_id: Hash64,
    canonical_job: (u32, u32),
    /// **ADR-0067: `Some` when this backend executes FROM the registered profile.** The plan is
    /// compiled at construction — every declared node bound to a served kernel and a named
    /// operand, or the constructor refuses — and every forward walks it. `None` is the compiled
    /// engine, kept for the rows this build's own table names (and as the interpreter's
    /// reference vectors, per the differential test beside the plan).
    plan: Option<crate::engine_a16::A16ProfilePlanV1>,
    /// **Whether this instance's class can carry a capture at all** — the four-byte state map,
    /// which is the width `A16Cache` holds. Decided from the profile at construction (declared
    /// rather than probed: `supports_court` is read at boot, before any capture exists). The v1
    /// registered class declares the one-byte map and therefore cannot commit a checkpoint leg;
    /// its instances keep the legacy composite roots and say `supports_court() == false`, which
    /// is the honest description of that class. The Decision-F graph correspondence is still
    /// guarded inside the captured path itself.
    court_capable: bool,
}

impl Qwen25A16Backend {
    pub fn new(
        artifact: std::sync::Arc<Base0ArtifactV1>,
        network_id: Vec<u8>,
        profile: PalwShapeProfileV3,
        canonical_job: (u32, u32),
    ) -> Self {
        let shape_id = artifact.artifact_digest();
        let class_profile_id = profile.shape_profile_id();
        let court_capable =
            profile.state_chunk_map_id == kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_map_id_v2();
        Self {
            artifact,
            model_id: "PALW-QWEN25-A16".to_string(),
            network_id,
            shape_id,
            profile,
            class_profile_id,
            canonical_job,
            plan: None,
            court_capable,
        }
    }

    /// **ADR-0067 Decision 2's constructor: a backend for a class this build's table never
    /// heard of.** The profile arrives from chain state (the registration's admission carriage),
    /// and the plan it compiles to IS the admission decision — a graph outside this build's
    /// kernel vocabulary is refused here, with the node named, before anything executes.
    pub fn from_registered_profile(
        artifact: std::sync::Arc<Base0ArtifactV1>,
        network_id: Vec<u8>,
        profile: PalwShapeProfileV3,
        canonical_job: (u32, u32),
    ) -> Result<Self, String> {
        let engine = A16Engine::new(&artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
        // **Two different facts must not wear the same words** (round-3 defect I-3). "This build
        // cannot serve the registered graph" is true of a kernel this build does not carry, and an
        // operator reading it goes looking for missing software. `OverMemoryCeiling` is not that:
        // the chain admitted the class, this build's own capacity bound refused it, and a node
        // whose ceiling is larger runs the very same graph.
        let plan = engine.plan_from_profile(&profile).map_err(|e| match e {
            A16PlanErrorV1::OverMemoryCeiling { bytes, ceiling } => format!(
                "this node's interpreted-execution capacity refuses the registered graph: one token's committed trace \
                 is {bytes} bytes and this build's capacity is {ceiling} (ADR-0067 SA-1). The chain's admission caps \
                 accepted this class and do not bound a declared row's width, so this is node-local servability, not a \
                 statement about the class: a node built with a larger ceiling serves it, and this one will not produce \
                 or judge for it"
            ),
            other => format!("this build cannot serve the registered graph: {other:?}"),
        })?;
        let shape_id = artifact.artifact_digest();
        let class_profile_id = profile.shape_profile_id();
        let court_capable =
            profile.state_chunk_map_id == kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_map_id_v2();
        Ok(Self {
            artifact,
            model_id: "PALW-A16/chain-registered".to_string(),
            network_id,
            shape_id,
            profile,
            class_profile_id,
            canonical_job,
            plan: Some(plan),
            court_capable,
        })
    }

    /// One forward, through whichever authority constructed this backend: the registered plan
    /// where one exists, the compiled engine where the build's own table named the class.
    fn forward(
        &self,
        engine: &A16Engine<'_>,
        cache: &mut crate::engine_a16::A16Cache,
        token: usize,
        position: usize,
    ) -> Result<(Vec<i32>, crate::engine_a16::A16TraceV1), String> {
        match &self.plan {
            Some(plan) => engine.forward_token_planned(plan, cache, token, position).map_err(|e| format!("planned forward: {e:?}")),
            None => engine.forward_token_traced(cache, token, position).map_err(|e| format!("forward: {e:?}")),
        }
    }

    /// The class's graph, for callers that need it directly — the same reason `Base0Backend`
    /// exposes its own: the trait's job is the verbs, not the shape.
    pub fn profile(&self) -> &PalwShapeProfileV3 {
        &self.profile
    }

    fn run(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<Qwen25A16RunV1, String> {
        let engine = A16Engine::new(&self.artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
        let mut cache = A16Cache::new(self.artifact.shape.n_layers);
        let mut logits_rows: Vec<Vec<i32>> = Vec::with_capacity(prompt.len() + job.exact_decode_tokens as usize);
        let mut generated: Vec<u32> = Vec::with_capacity(job.exact_decode_tokens as usize);
        for (position, token) in prompt.iter().enumerate() {
            let (row, _) = self.forward(&engine, &mut cache, *token, position).map_err(|e| format!("prefill at {position}: {e}"))?;
            logits_rows.push(row);
        }
        // EXACT, never early: a job whose length depends on what the model said is a job whose cost
        // the producer controls.
        for step in 0..job.exact_decode_tokens as usize {
            let last = logits_rows.last().ok_or_else(|| "an empty prefill".to_string())?;
            let next = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(last) as u32;
            generated.push(next);
            let position = prompt.len() + step;
            let (row, _) =
                self.forward(&engine, &mut cache, next as usize, position).map_err(|e| format!("decode at {position}: {e}"))?;
            logits_rows.push(row);
        }
        Ok(Qwen25A16RunV1 { logits_rows, generated })
    }
}

impl Qwen25A16Backend {
    /// The one prover behind both [`PalwExecutionBackendV1::refutation_for_index`] and
    /// [`PalwExecutionBackendV1::refutation_for_free_prompt_index`]: `carried` is `None` for an
    /// attempt (the prompt is re-derived from the anchor) and the user's ids for a free prompt.
    fn refutation_with_prompt(
        &self,
        material: &[u8],
        index: u64,
        carried: Option<&[u32]>,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        let (binding, tiles, logits_rows, generated, checkpoint_chunks) =
            crate::produce::base0_material_decode_v1(material).map_err(|_| "the capture does not decode".to_string())?;
        if binding.step_leaf_count == 0 || binding.step_leaf_count > kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES {
            return Err("the binding's leaf count is outside the leg's own cap".to_string());
        }
        let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, index)
            .ok_or_else(|| format!("leaf {index} is not a main step coordinate"))?;
        let leaves = a16_leaves_by_position(&binding, &tiles);
        let step_tiles = crate::legs::Base0StepTilesV1 { leaves, tiles };

        // **This class's own pin: the tiled scheme's.** The generated ids are bound through the
        // rows-tree root — carrying the rows themselves is exactly what the tiled scheme exists to
        // avoid at this vocabulary.
        let rows_root = kaspa_consensus_core::palw_step_refute::tiled_logits_rows_root_v1(&binding.job_context, &logits_rows)
            .ok_or_else(|| "the retained rows build no tree".to_string())?;
        let pin = kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1::TiledV1(
            kaspa_consensus_core::palw_step_refute::PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: generated },
        );

        // **The prompt: re-derived for an attempt, carried for a free prompt** (ADR-0073 Decision
        // 1c). An attempt's `job_id` IS its anchor and its prompt is a pure function of it, so it
        // is re-derived; a context whose prompt hash does not match the derivation and arrives
        // with NO carried ids gets an empty prompt — the court's hash guard would refuse a wrong
        // one, and an absent one only narrows which leaves adjudicate. A free prompt is the user's
        // and derives from nothing: the caller hands it over, and one that is not the binding's
        // own is refused here rather than read by the court as no verdict.
        let prompt_token_ids: Vec<u32> = match carried {
            Some(ids) => {
                if kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(ids) != binding.job_context.prompt_token_ids_hash {
                    return Err("the carried prompt is not the one this capture's job context commits to".to_string());
                }
                ids.to_vec()
            }
            None => {
                let derived = qwen25_a16_prompt_for_anchor(
                    binding.job_context.job_id,
                    self.artifact.shape.vocab,
                    binding.job_context.declared_prefill_tokens,
                );
                let derived_ids: Vec<u32> = derived.iter().map(|t| *t as u32).collect();
                if kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&derived_ids) == binding.job_context.prompt_token_ids_hash {
                    derived_ids
                } else {
                    Vec::new()
                }
            }
        };

        // **The anchor its committed leg gives this call, when the node reads the cache.** A
        // carried anchor for a node with no KV refs is refused by the court rather than ignored,
        // so attachment follows the node's own declaration.
        let reads_cache = binding
            .shape_profile
            .resolve_node_slot(coord.node_slot)
            .map(|(node, _)| {
                node.input_refs.iter().any(|r| {
                    *r == kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_K
                        || *r == kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_V
                })
            })
            .unwrap_or(false);
        let kv_checkpoint = if reads_cache && coord.call_index > 0 {
            crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(
                &binding.job_context,
                &binding.shape_profile,
                &binding.checkpoint_profile,
                &checkpoint_chunks,
            )
            .ok()
            .and_then(|checkpoints| crate::legs::base0_kv_anchor_for_call_v1(&checkpoints, coord.call_index))
        } else {
            None
        };

        crate::legs::base0_refutation_from_capture_v1(
            &binding.shape_profile.clone(),
            &binding.job_context.clone(),
            &step_tiles,
            binding,
            coord,
            prompt_token_ids,
            Some(pin),
            kv_checkpoint,
        )
        .map_err(|e| format!("{e:?}"))
    }
}

/// **The dense tier's kernels, as a seat's interval replay needs them** (ADR-0077 Decision 8).
///
/// The window is walked by [`crate::fp_interval::base0_fp_replay_interval_v1`] — the capture's own
/// loop — and this supplies only what the family owns: restoring the cache the interval resumes
/// from and running one forward call, through the SAME plan-or-traced dispatch the capture uses.
/// A replay that took the other arm would recompute rows the capture never committed.
struct A16IntervalKernels<'a> {
    artifact: &'a Base0ArtifactV1,
    plan: Option<&'a crate::engine_a16::A16ProfilePlanV1>,
}

impl crate::fp_interval::Base0FpIntervalKernelsV1 for A16IntervalKernels<'_> {
    fn replay_interval(
        &self,
        profile: &PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        start: &crate::fp_interval::Base0FpIntervalStartV1<'_>,
        first_call: u32,
        last_call: u32,
    ) -> Result<Vec<(u64, Hash64)>, String> {
        let engine = A16Engine::new(self.artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
        let layers = self.artifact.shape.n_layers;
        let row_elements = profile.attn_kv_heads as usize * profile.attn_head_dim as usize;
        let mut cache = match start {
            crate::fp_interval::Base0FpIntervalStartV1::Genesis { .. } => A16Cache::new(layers),
            crate::fp_interval::Base0FpIntervalStartV1::Checkpoint { covered_decode_call, chunks, .. } => {
                let positions = kaspa_consensus_core::palw_state_chunk_map::integer_kv_positions_at_v1(ctx, *covered_decode_call);
                let geometry = crate::legs::base0_state_chunk_geometry_v1(profile, positions).map_err(|e| format!("{e:?}"))?;
                A16Cache::from_state_chunks_v1(layers, row_elements, &geometry, chunks).map_err(|e| format!("{e:?}"))?
            }
        };
        let vocab = self.artifact.shape.vocab;
        crate::fp_interval::base0_fp_replay_interval_v1(profile, ctx, start, first_call, last_call, |token, position| {
            if token >= vocab {
                return Err(format!("token {token} is outside this class's vocabulary of {vocab}"));
            }
            let (logits, trace) = match self.plan {
                Some(plan) => {
                    engine.forward_token_planned(plan, &mut cache, token, position).map_err(|e| format!("planned forward: {e:?}"))?
                }
                None => engine.forward_token_traced(&mut cache, token, position).map_err(|e| format!("forward: {e:?}"))?,
            };
            Ok((logits, crate::legs::a16_captured_rows_v1(&trace)))
        })
    }
}

impl Qwen25A16Backend {
    /// The cadence this family checkpoints at — a class fact from the family's registration, never
    /// read off a capture.
    fn checkpoint_interval(&self) -> u32 {
        kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1
    }

    /// **ADR-0077 SA-6, at the job boundary.** This tier's artifact is owned memory (the converter
    /// writes a `Base0ArtifactV1`), so no mapped page can fault under a job. Stated rather than
    /// assumed — the hybrid answers differently and one seam serves both.
    fn artifact_read_probe_v1(&self) -> Result<(), String> {
        Ok(())
    }
}

impl PalwExecutionBackendV1 for Qwen25A16Backend {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn job_for_anchor(&self, anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
        let (prefill, decode) = self.canonical_job;
        let shape = &self.artifact.shape;
        if prefill as usize + decode as usize >= shape.max_position {
            return Err(format!(
                "the canonical job needs {} positions and the table covers {}",
                prefill as usize + decode as usize,
                shape.max_position
            ));
        }
        let prompt = qwen25_a16_prompt_for_anchor(anchor, shape.vocab, prefill);
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let ctx = PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: self.network_id.clone(),
            job_id: anchor,
            job_nullifier: keyed(QWEN25_A16_DOMAIN_EXECUTION, &[b"nullifier", anchor.as_byte_slice()]),
            assignment_id: Hash64::default(),
            execution_seed: anchor.as_byte_slice()[..32].try_into().expect("a 64-byte hash has 32 bytes"),
            model_profile_id: self.shape_id,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: self.shape_id,
            shape_profile_id: self.class_profile_id,
            trace_scheme_id: kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1(),
            cu_ruleset_id: Hash64::default(),
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: prompt_token_ids_hash_v2(&ids),
            declared_prefill_tokens: prefill,
            exact_decode_tokens: decode,
            max_context_tokens: shape.max_position as u32,
        };
        Ok((ctx, prompt))
    }

    fn execute(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        // **The captured attempt, for the class that can carry one.** Its `execution_root` is the
        // step binding's own commitment — the value `check_execution_root_binding` compares a
        // close against — and its material is the family codec's, which is what a rung or a
        // refutation is later assembled from. The v1 class (one-byte map over an `i32` cache)
        // cannot capture, so it keeps the legacy composite exactly as registered.
        if self.court_capable {
            let run = a16_execute_for_attempt_v1(&self.artifact, &self.profile, self.plan.as_ref(), job, prompt)?;
            let material = crate::produce::base0_material_encode_v1(&run).map_err(|e| e.to_string())?;
            return Ok(PalwExecutionOutcomeV1 {
                trace_root: run.trace_root,
                output_root: run.output_root,
                execution_root: run.execution_root,
                trace_manifest_root: run.trace_manifest_root,
                trace_chunk_count: run.trace_chunk_count,
                material,
            });
        }
        let run = self.run(job, prompt)?;
        // Unreachable for a run this backend just performed; an error rather than an `expect`
        // because a producer that panics is worse than one that cannot commit.
        let (trace_root, output_root, execution_root, trace_manifest_root) = qwen25_a16_roots_v1(job, self.shape_id, &run)
            .ok_or_else(|| "this run did not keep the rows its tokens were selected from".to_string())?;
        Ok(PalwExecutionOutcomeV1 {
            trace_root,
            output_root,
            execution_root,
            trace_manifest_root,
            trace_chunk_count: 1,
            material: qwen25_a16_material_encode_v1(&run),
        })
    }

    /// The non-streaming verb IS the streaming one with a callback that does nothing — never the
    /// reverse (ADR-0077 Decision 2). One inference, one capture, one commitment.
    fn execute_free_prompt(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        self.execute_free_prompt_streaming(job, prompt_tokens, &mut |_| {})
    }

    fn execute_free_prompt_streaming(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        on_token: &mut dyn FnMut(u32),
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3, palw_fp_job_context_v3};
        use kaspa_consensus_core::palw_freeprompt_v3::PalwFpStopReasonV3;

        // ADR-0077 SA-6: an artifact this host can no longer read is a job failure named at the
        // boundary, not a fault taken three layers into a kernel.
        self.artifact_read_probe_v1()?;

        if job.prompt_tokens as usize != prompt_tokens.len() {
            return Err(format!("the job declares {} prompt tokens and {} were supplied", job.prompt_tokens, prompt_tokens.len()));
        }
        // **An empty prompt is refused HERE, where every other malformed job is.** The graceful
        // answer used to live at the end (`an empty prefill`, after the whole loop), and the
        // Decision-F probe below indexes `prompt_tokens[0]` before reaching it — so a zero-token
        // job PANICKED. That is network-reachable: a free-prompt material is gossiped by anyone,
        // a seat replays it in-process, and a panicked panel task stops filing receipts for every
        // claim it holds, not just this one.
        if prompt_tokens.is_empty() {
            return Err("a job with no prompt tokens is not a job".to_string());
        }
        let vocab = self.artifact.shape.vocab;
        if let Some(bad) = prompt_tokens.iter().find(|t| **t >= vocab) {
            return Err(format!("token {bad} is outside this class's vocabulary of {vocab}"));
        }

        // What the derivation asks for. `shape_profile_id` is the class; the rest are the values
        // this family's job contexts carry, taken from the same profile rather than invented.
        let class = PalwFpClassFactsV3 {
            model_profile_id: self.shape_id,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: self.shape_id,
            shape_profile_id: self.class_profile_id,
            cu_ruleset_id: Hash64::default(),
        };
        // A declared budget, decoded exactly: the count and the stop reason are known before the
        // run, and the context builder enforces the pairing rather than trusting this.
        let shape = PalwFpRunFactsV3 {
            decode_tokens_executed: job.decode_token_limit,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: Hash64::default(),
            step_leaf_count: 0,
        };
        // Built BEFORE the run and run under: `palw_fp_execution_root_v3` recomputes the court's
        // root from this context, so an execution carried out under any other one commits a root
        // nobody can reproduce.
        let ctx = palw_fp_job_context_v3(job, &class, &shape, &self.network_id).map_err(|e| format!("{e:?}"))?;

        // **The one capture path this family has** (ADR-0049 Decision F's probe, the checkpoint
        // serializer at the class's declared width, and the selecting-rows retention all live in
        // it). The free-prompt lane differs from the attempt lane only in where its context and
        // its tokens come from, so the run itself must not be a second implementation.
        let run =
            a16_execute_for_attempt_streaming_v1(&self.artifact, &self.profile, self.plan.as_ref(), &ctx, prompt_tokens, on_token)?;

        // The four legs, measured — the derived roots the execution root is built from, which is
        // what `palw_fp_execution_root_v3` recomputes.
        let (checkpoint_leg_root, step_leg_root) = crate::legs::base0_leg_roots_from_binding_v1(&run.binding);
        let material = crate::produce::base0_material_encode_v1(&run).map_err(|e| e.to_string())?;
        Ok(kaspa_consensus_core::palw_backend::PalwFpRunV1 {
            outcome: PalwExecutionOutcomeV1 {
                trace_root: run.trace_root,
                output_root: run.output_root,
                execution_root: run.execution_root,
                trace_manifest_root: run.trace_manifest_root,
                trace_chunk_count: run.trace_chunk_count,
                material,
            },
            facts: PalwFpRunFactsV3 {
                full_logits_trace_root: run.trace_root,
                activation_leg_root: run.binding.activation_leg_root,
                checkpoint_leg_root,
                step_leg_root,
                // The price (ADR-0074 Decision 5): read off the binding, never declared.
                step_leaf_count: run.binding.step_leaf_count,
                ..shape
            },
            output_token_ids: run.generated_token_ids,
        })
    }

    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        // **The family codec first** — a captured attempt's material carries its binding, and the
        // seat check rebuilds the step root from the tiles, the checkpoint leg from the chunks,
        // and the tiled trace root from the retained rows. The legacy composite decode stays as
        // the fallback for the v1 class's claims, whose material is rows-and-ids only.
        if let Ok(decoded) = crate::produce::base0_material_decode_v1(material) {
            if claim.anchor != Hash64::default() && decoded.0.job_context.job_id != claim.anchor {
                return PalwMaterialVerdictV1::Mismatch;
            }
            // A capture for some OTHER class of this family is not this backend's to vouch for.
            if decoded.0.shape_profile.shape_profile_id() != self.class_profile_id {
                return PalwMaterialVerdictV1::Unverifiable;
            }
            return match crate::produce::base0_material_matches_claim_v1(&decoded, claim.execution_root, claim.trace_root) {
                Ok(true) => PalwMaterialVerdictV1::Matches,
                Ok(false) => PalwMaterialVerdictV1::Mismatch,
                Err(_) => PalwMaterialVerdictV1::Unverifiable,
            };
        }
        let Some(run) = qwen25_a16_material_decode_v1(material) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        // Recomputed under the job the claim's ANCHOR implies — the job the chain asked for, and
        // the only one a producer was entitled to run. A claim with no anchor has no block to bind
        // to, and a capture verified without one is re-usable by anyone who mines a fresh block, so
        // that case is `Unverifiable` rather than a guess.
        if claim.anchor == Hash64::default() {
            return PalwMaterialVerdictV1::Unverifiable;
        }
        let Ok((job, _)) = self.job_for_anchor(claim.anchor) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        // Material that does not carry the rows it selected from is material this seat cannot
        // check — the honest `Unverifiable`, not an accusation, and not a panic.
        let Some((trace_root, _, execution_root, _)) = qwen25_a16_roots_v1(&job, self.shape_id, &run) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        if trace_root == claim.trace_root && execution_root == claim.execution_root {
            PalwMaterialVerdictV1::Matches
        } else {
            PalwMaterialVerdictV1::Mismatch
        }
    }

    /// The A16 tier can take a court's turn exactly when its class can carry a capture — the
    /// four-byte state map. The v1 registered class cannot, and says so here rather than stalling
    /// a court at round 0.
    fn supports_court(&self) -> bool {
        self.court_capable
    }

    fn capture_shape(&self, material: &[u8]) -> Option<kaspa_consensus_core::palw_backend::PalwCaptureShapeV1> {
        let (binding, ..) = crate::produce::base0_material_decode_v1(material).ok()?;
        Some(kaspa_consensus_core::palw_backend::PalwCaptureShapeV1 {
            job_context: binding.job_context.clone(),
            step_leaf_count: binding.step_leaf_count,
        })
    }

    fn bisect_prefix_state(&self, material: &[u8], index: u64) -> Option<kaspa_hashes::Hash64> {
        let (binding, tiles, _, _, _) = crate::produce::base0_material_decode_v1(material).ok()?;
        // The count arrived over gossip inside a borsh blob; bounding it BEFORE the allocation is
        // the lesson the seat check already wrote down.
        if binding.step_leaf_count == 0 || binding.step_leaf_count > kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES {
            return None;
        }
        let leaves = a16_leaves_by_position(&binding, &tiles);
        Some(crate::legs::base0_bisect_prefix_state_v1(&binding.job_context, &leaves, index))
    }

    fn refutation_for_index(
        &self,
        material: &[u8],
        index: u64,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        self.refutation_with_prompt(material, index, None)
    }

    fn refutation_for_free_prompt_index(
        &self,
        material: &[u8],
        index: u64,
        prompt_token_ids: &[u32],
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        self.refutation_with_prompt(material, index, Some(prompt_token_ids))
    }

    // ---- ADR-0077 Decision 8: the interval seam -------------------------------------------
    //
    // Only for a class that can carry a capture at all. The v1 class declares the one-byte map
    // over an `i32` cache, so it commits no checkpoint leg and there is no interval to open —
    // `None`/refusal is the honest answer there, and it is the same fact `supports_court` reports.

    fn fp_interval_count(&self, capture: &[u8]) -> Option<u32> {
        let (binding, ..) = crate::produce::base0_material_decode_v1(capture).ok()?;
        crate::fp_interval::Base0FpIntervalGeometryV1::from_binding_v1(&binding, self.checkpoint_interval())
            .ok()
            .map(|g| g.interval_count)
    }

    fn fp_interval_count_for(&self, prompt_tokens: u32, decode_tokens_executed: u32) -> Option<u32> {
        if !self.court_capable {
            return None;
        }
        crate::fp_interval::base0_fp_interval_count_for_v1(prompt_tokens, decode_tokens_executed, self.checkpoint_interval())
    }

    fn open_fp_interval(&self, capture: &[u8], index: u32, prompt_token_ids: &[u32]) -> Result<Vec<u8>, String> {
        let material = crate::produce::base0_material_decode_v1(capture).map_err(|_| "the capture does not decode".to_string())?;
        crate::fp_interval::base0_open_fp_interval_v1(&material, index, prompt_token_ids, self.checkpoint_interval())
            .map_err(|e| e.to_string())
    }

    fn verify_fp_interval_opening(
        &self,
        opening: &[u8],
        claim: PalwClaimRootsV1,
        index: u32,
        prompt_token_ids: &[u32],
        work_leaves: u64,
    ) -> kaspa_consensus_core::palw_backend::PalwFpIntervalVerdictV1 {
        crate::fp_interval::base0_verify_fp_interval_opening_v1(
            opening,
            claim,
            index,
            prompt_token_ids,
            work_leaves,
            self.checkpoint_interval(),
            &A16IntervalKernels { artifact: &self.artifact, plan: self.plan.as_ref() },
        )
    }

    fn operand_openings_for(
        &self,
        refutation: &kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1,
    ) -> Result<Vec<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1>, String> {
        let inventory = crate::inventory::a16_inventory_v1(&self.artifact, &self.profile).map_err(|e| format!("{e:?}"))?;
        let recorder = kaspa_consensus_core::palw_artifact::PalwRecordingOracleV1::new(inventory.operands());
        // The verdict is not ours to read here — this runs the adjudicator only to learn WHICH
        // rows it resolves, and it resolves the same rows whichever way the step reads.
        let _ = kaspa_consensus_core::palw_step_refute::check_execution_step_refutation_v1(refutation, &recorder);
        recorder.openings().ok_or_else(|| "the inventory could not open a recorded row".to_string())
    }

    fn execute_with_injected_fault(
        &self,
        job: &PalwJobContextV2,
        prompt: &[usize],
        leaf_index: u64,
    ) -> Result<PalwExecutionOutcomeV1, String> {
        if !self.court_capable {
            return Err("the v1 class carries no capture to tamper with".to_string());
        }
        let mut run = a16_execute_for_attempt_v1(&self.artifact, &self.profile, self.plan.as_ref(), job, prompt)?;
        let ctx_hash = job.context_hash();
        let profile_hash = self.profile.shape_profile_id();
        {
            let slot = run
                .tiles
                .tiles
                .iter_mut()
                .find(|(i, _)| *i == leaf_index)
                .ok_or_else(|| format!("the capture holds no tile at leaf {leaf_index}"))?;
            slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
            run.tiles.leaves[leaf_index as usize] =
                kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &slot.1);
        }
        // **Re-derive, do not patch.** The commitment must be the corrupted capture's OWN, or this
        // is a producer whose roots disagree with its material — which any seat catches without a
        // court, and which is therefore not the fraud under test.
        let binding = crate::legs::base0_binding_from_capture_v1(
            &self.profile,
            job,
            &run.tiles,
            &run.checkpoints,
            run.trace_root,
            crate::produce::base0_activation_leg_root_v1(job),
        )
        .map_err(|e| format!("{e:?}"))?;
        run.execution_root = binding.committed_execution_root;
        run.binding = binding;
        let material = crate::produce::base0_material_encode_v1(&run).map_err(|e| e.to_string())?;
        Ok(PalwExecutionOutcomeV1 {
            trace_root: run.trace_root,
            output_root: run.output_root,
            execution_root: run.execution_root,
            trace_manifest_root: run.trace_manifest_root,
            trace_chunk_count: run.trace_chunk_count,
            material,
        })
    }
}

/// The committed leaf-hash vector, rebuilt from retained tiles — the shape the rung and the
/// refutation helpers read. The floor keeps an identical private helper beside its own backend;
/// this one is the A16 family's copy of the same eleven lines rather than a premature trait.
fn a16_leaves_by_position(
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    tiles: &[(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1)],
) -> Vec<Hash64> {
    let ctx_hash = binding.job_context.context_hash();
    let profile_hash = binding.shape_profile.shape_profile_id();
    let mut leaves = vec![Hash64::default(); binding.step_leaf_count as usize];
    for (index, leaf) in tiles {
        if let Some(slot) = leaves.get_mut(*index as usize) {
            *slot = kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf);
        }
    }
    leaves
}

#[cfg(test)]
mod free_prompt_tests {
    use super::*;
    use crate::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use crate::engine_a16::derived_a16_store;
    use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_execution_root_v3, palw_fp_job_context_v3};
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFreePromptJobV3,
    };
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v1, qwen25_a16_profile_v2};
    use kaspa_consensus_core::palw_state_chunk_map as map;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    const NETWORK: &[u8] = b"misaka-palw-rc";

    /// A class small enough to run in a unit test, built from ONE geometry so the artifact and the
    /// profile cannot describe different models — which is the failure a hand-written pair invites.
    fn class(map_id: Hash64) -> (std::sync::Arc<Base0ArtifactV1>, PalwShapeProfileV3) {
        class_from(map_id, false)
    }

    /// `corrected` builds the class `qwen25_a16_profile_v2` describes: the pre table names the
    /// embed-lift requant and the state map is the four-byte one.
    fn class_from(map_id: Hash64, corrected: bool) -> (std::sync::Arc<Base0ArtifactV1>, PalwShapeProfileV3) {
        let geometry = PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 8,
            ffn_dim: 8,
            attn_heads: 2,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            vocab_size: 64,
            n_ctx: 32,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        };
        let mut profile = if corrected {
            qwen25_a16_profile_v2(geometry).expect("a valid corrected A16 profile")
        } else {
            qwen25_a16_profile_v1(geometry).expect("a valid A16 profile")
        };
        if !corrected {
            profile.state_chunk_map_id = map_id;
        }
        let shape = Base0ShapeV1 {
            n_layers: geometry.layer_count as usize,
            n_heads: geometry.attn_heads as usize,
            n_kv_heads: geometry.attn_kv_heads as usize,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("the derived store is sorted and unique");
        (std::sync::Arc::new(artifact), profile)
    }

    fn job(profile: &PalwShapeProfileV3, prompt_tokens: u32, decode: u32) -> PalwFreePromptJobV3 {
        PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(0xD0),
            class_id: profile.shape_profile_id(),
            executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
            executor_pubkey: vec![0x11; 32],
            operator_id: Hash64::from_u64_word(0x0B),
            anchor_block: Hash64::from_u64_word(0xA0),
            anchor_daa: 4242,
            job_nonce: [0x5A; 32],
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::from_u64_word(0x71),
            prompt_tokens,
            decode_token_limit: decode,
            max_context_tokens: profile.n_ctx,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
        }
    }

    /// **The corrected class runs a caller's prompt and commits a root the derivation recomputes.**
    ///
    /// This is the property everything else was for: a language model's own execution, under a
    /// class whose graph names what the engine does and whose state map is the width the cache
    /// holds, producing a free-prompt commitment a court could recompute. The two defects that
    /// stopped it were measured, not guessed, and correcting either one moves the class id — so
    /// this is a class to register, and the test is what says it would work once registered.
    #[test]
    fn the_corrected_a16_class_commits_the_root_the_derivation_recomputes() {
        let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
        let digest = artifact.artifact_digest();
        let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2));
        let prompt: Vec<usize> = vec![3, 9, 17, 33];
        let job = job(&profile, prompt.len() as u32, 3);

        let run = backend.execute_free_prompt(&job, &prompt).expect("the corrected class runs a caller's prompt");

        let class_facts = PalwFpClassFactsV3 {
            model_profile_id: digest,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: digest,
            shape_profile_id: profile.shape_profile_id(),
            cu_ruleset_id: Hash64::default(),
        };
        let ctx = palw_fp_job_context_v3(&job, &class_facts, &run.facts, NETWORK).expect("the finished run implies a context");
        assert_eq!(
            palw_fp_execution_root_v3(&ctx, &run.facts),
            run.outcome.execution_root,
            "the derivation and the run must agree, or the court convicts the honest"
        );

        // All four legs measured — four zero roots would satisfy the equality above and mean
        // nothing — and the answer, which is the other half of the one inference.
        assert_ne!(run.facts.full_logits_trace_root, Hash64::default());
        assert_ne!(run.facts.step_leg_root, Hash64::default());
        assert_ne!(run.facts.checkpoint_leg_root, Hash64::default());
        assert_eq!(run.facts.stop_reason, PalwFpStopReasonV3::ExactBudgetReached);
        assert_eq!(run.output_token_ids.len(), job.decode_token_limit as usize);

        // And it is a different class from the one testnet-11 carries, which is the cost.
        let (_, registered) = class_from(map::integer_kv_state_chunk_map_id_v1(), false);
        assert_ne!(profile.shape_profile_id(), registered.shape_profile_id());
    }

    /// **A16 refuses the free-prompt path, and the refusal names why — under either map.**
    ///
    /// This test was first written to assert the opposite: give the class a state map that fits its
    /// cache and the round trip should close. It does not, and the reason is a second gap the first
    /// one was hiding. ADR-0049 Decision F requires a class's profile to name every narrowing its
    /// engine performs; `Base0Engine` exposes `plan()` and `base0_check_graph_v1` enforces it.
    /// `A16Engine` has no plan and there is no A16 counterpart. Measured, the per-layer and post
    /// tables agree exactly — and the pre table does not: the engine records the embedding gather
    /// AND the requant that lifts it onto the A16 stream, while the profile declares only the
    /// gather. A requant is a narrowing, which is what Decision F is about.
    ///
    /// Both maps are exercised because the graph gap fires first: giving this class a state map
    /// that fits its cache would not make it adjudicable.
    #[test]
    fn a16_refuses_the_free_prompt_path_until_its_graph_is_reconciled() {
        for map_id in [map::integer_kv_state_chunk_map_id_v2(), map::integer_kv_state_chunk_map_id_v1()] {
            let (artifact, profile) = class(map_id);
            let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2));
            let prompt: Vec<usize> = vec![3, 9, 17, 33];
            let job = job(&profile, prompt.len() as u32, 3);
            let error = match backend.execute_free_prompt(&job, &prompt) {
                Err(e) => e,
                Ok(_) => panic!("a class whose graph does not name what its engine computes must not commit a step leg"),
            };
            assert!(error.contains("registered graph"), "the refusal names the gap: {error}");
            assert!(error.contains("requant"), "and the node it is missing: {error}");
        }
    }

    /// **The A16 step space, end to end: every leaf of a captured attempt adjudicates, and a
    /// tampered one convicts** — the theorem this family's court capability rests on.
    ///
    /// One captured run of the corrected class; then, for EVERY leaf of its step space, the
    /// backend's own prover assembles the refutation, the backend's own inventory answers for the
    /// operands through real Merkle openings against its root, and the court finds no fault. A
    /// single leaf that reads `Unadjudicable` is a step nobody can police — the
    /// coverage-clean-but-unprosecutable shape ADR-0049 exists to refuse — so the sweep is
    /// exhaustive rather than sampled. The same prover then convicts a run with one tampered lane
    /// at representative kernels, including a decode call (the tiled pin) and an anchored
    /// attention step (the v2-map checkpoint geometry).
    #[test]
    fn every_a16_leaf_adjudicates_and_a_tampered_one_convicts() {
        use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, check_execution_step_refutation_v1};

        let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
        let backend = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3));
        assert!(backend.supports_court(), "the corrected class takes a court's turn");

        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0xA16C0117)).expect("the anchor implies a job");
        let outcome = backend.execute(&job, &prompt).expect("the corrected class runs the attempt lane");
        let (binding, _tiles, _logits, _generated, _chunks) =
            crate::produce::base0_material_decode_v1(&outcome.material).expect("the captured material decodes");
        assert_eq!(outcome.execution_root, binding.committed_execution_root, "the claim commits the binding's own root");

        // The seat's half, against this very claim.
        let claim = PalwClaimRootsV1 {
            execution_root: outcome.execution_root,
            trace_root: outcome.trace_root,
            anchor: Hash64::from_u64_word(0xA16C0117),
        };
        assert_eq!(
            backend.verify_material(&outcome.material, claim),
            kaspa_consensus_core::palw_backend::PalwMaterialVerdictV1::Matches
        );

        // One proven oracle over the whole inventory — the production path a close takes.
        let inventory = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the corrected class yields an inventory");
        let openings: Vec<_> = (0..inventory.operands().len())
            .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
            .collect();
        let oracle = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
            .expect("every inventory row proves against its own root");

        // The sweep: every leaf of the step space clears the honest capture.
        let mut anchored_seen = 0u32;
        for index in 0..binding.step_leaf_count {
            let refutation = backend
                .refutation_for_index(&outcome.material, index)
                .unwrap_or_else(|e| panic!("leaf {index} must open from an honest capture: {e}"));
            anchored_seen += u32::from(refutation.kv_checkpoint.is_some());
            let got = check_execution_step_refutation_v1(&refutation, &oracle);
            let named = profile
                .resolve_node_slot(refutation.output_preimage.coord.node_slot)
                .map(|(n, l)| format!("{} (layer {l:?})", n.weight_name))
                .unwrap_or_default();
            assert!(
                matches!(got, Err(PalwStepRefuteError::NoFaultFound)),
                "an honest execution must clear itself at leaf {index} (coord {:?}, node {named}): got {got:?}",
                refutation.output_preimage.coord
            );
        }
        assert!(anchored_seen > 0, "the sweep exercised the checkpoint-anchored attention path (v2 map geometry)");

        // The other direction: one tampered lane convicts, at kernels of different shapes —
        // the embedding gather, a matmul tile, and a decode-call leaf (whose adjudication rides
        // the tiled decode-token pin).
        let coords = [0u64, binding.step_leaf_count / 3, binding.step_leaf_count - 1];
        for &index in &coords {
            let lying = backend.execute_with_injected_fault(&job, &prompt, index).expect("a tampered capture still commits");
            let refutation = backend.refutation_for_index(&lying.material, index).expect("a tampered capture opens too");
            let openings = backend.operand_openings_for(&refutation).expect("the prover opens what the court resolves");
            let proven = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
                .expect("recorded openings prove");
            assert!(
                check_execution_step_refutation_v1(&refutation, &proven).is_ok(),
                "a tampered lane at leaf {index} must convict, not read as no fault"
            );
        }
    }

    /// **Two authorities, one commitment.** The ledger-compiled backend and the chain-registered
    /// one (whose plan is compiled FROM the profile) must capture the same job to the same roots
    /// and the same material — otherwise the same class's claims would be prosecutable or not
    /// depending on WHICH node produced them, and "the class is adjudicable" would be a property
    /// of a deployment rather than of the class.
    #[test]
    fn both_authorities_capture_one_job_to_one_commitment() {
        let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
        let compiled = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3));
        let registered = Qwen25A16Backend::from_registered_profile(artifact, NETWORK.to_vec(), profile, (4, 3))
            .expect("the corrected profile plans");
        assert!(compiled.supports_court() && registered.supports_court());

        let anchor = Hash64::from_u64_word(0x2AA16);
        let (job, prompt) = compiled.job_for_anchor(anchor).expect("a job");
        let a = compiled.execute(&job, &prompt).expect("the compiled walk runs");
        let b = registered.execute(&job, &prompt).expect("the planned walk runs");
        assert_eq!(a.execution_root, b.execution_root, "one binding, whichever authority walked");
        assert_eq!(a.trace_root, b.trace_root);
        assert_eq!(a.material, b.material, "and one retained material, byte for byte");
    }

    /// **Under the map the class declares TODAY, the same run refuses.**
    ///
    /// `integer_kv_state_chunk_map_id_v1` describes one byte per element and this cache holds
    /// `i32`. The refusal is the whole point: the alternative implementation truncates, passes
    /// every downstream check, and commits a checkpoint that opens to a state the producer never
    /// had.
    #[test]
    fn a16_refuses_to_commit_under_a_map_that_cannot_describe_its_cache() {
        let (artifact, profile) = class(map::integer_kv_state_chunk_map_id_v1());
        let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2));
        let prompt: Vec<usize> = vec![3, 9, 17, 33];
        let job = job(&profile, prompt.len() as u32, 3);

        let error = match backend.execute_free_prompt(&job, &prompt) {
            Err(e) => e,
            Ok(_) => panic!("a one-byte map cannot describe an i32 cache, and committing anyway is the defect"),
        };
        assert!(error.contains("registered graph") || error.contains("state map"), "the error names the defect it hit first: {error}");
    }
}
