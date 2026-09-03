//! **Qwen3.6 behind the execution-backend seam** — the producer path.
//!
//! `PalwExecutionBackendV1` is what a node reaches for when a template says "run the job this
//! anchor implies, commit to what you ran, and check whether somebody else's material answers for
//! their claim". Implementing it is what makes a class producible rather than merely runnable.
//!
//! # What this backend can and cannot do, stated first
//!
//! `execute` and `verify_material` are real, and so is the court: a backend holding the
//! registered graph (either constructor — the chain-registered one, or the ledger-compiled one
//! armed with a `graph_version >= 2` class) captures every declared step, and
//! `bisect_prefix_state` / `refutation_for_index` / `operand_openings_for` /
//! `execute_with_injected_fault` answer over that capture exactly as the floor's and the dense
//! tier's do. A backend armed with neither a plan nor a profile — a v1 class, or a class this
//! build's ledger never heard of — keeps the trait's honest defaults (`None` and `Err`) rather
//! than something that looks like a court, and `supports_court()` says so at boot.
//!
//! The checkpoint leg is EMPTY by construction for this family
//! ([`qwen36_checkpoint_profile_v1`]), so a refutation never carries a KV anchor: the history
//! rides as ordinary step openings, which is also the required set the court derives for a
//! class with zero checkpoints.

use crate::qwen36::{Qwen36ArtifactV1, Qwen36Cache, Qwen36Engine, Qwen36ShapeV1};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES as LEG_MAX_LEAVES;
use kaspa_consensus_core::palw_v2::{
    PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, output_commitment_v2, prompt_token_ids_hash_v2,
};
use kaspa_hashes::Hash64;

/// Domain separators. Distinct from BASE-0's so that a root computed for one class can never be
/// read as the other's, which is the only thing a domain tag is for.
pub const QWEN36_DOMAIN_JOB_PROMPT: &[u8] = b"misaka-palw/qwen36/job-prompt/v1";
pub const QWEN36_DOMAIN_SHAPE: &[u8] = b"misaka-palw/qwen36/shape/v1";
pub const QWEN36_DOMAIN_EXECUTION: &[u8] = b"misaka-palw/qwen36/execution/v1";
pub const QWEN36_DOMAIN_MANIFEST: &[u8] = b"misaka-palw/qwen36/trace-manifest/v1";
pub const QWEN36_DOMAIN_MATERIAL: &[u8] = b"misaka-palw/qwen36/material/v1";

fn keyed(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
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
    keyed(QWEN36_DOMAIN_EXECUTION, &[b"rendered", &generated.iter().flat_map(|t| t.to_le_bytes()).collect::<Vec<_>>()])
}

/// **The graph's identity.** Every field of the shape, fixed-width, in declaration order.
///
/// Stands in for the court's `shape_profile_id` until the hybrid step space exists. It carries the
/// same obligation — two classes with different graphs must not share it — and none of the court's
/// other meaning, which is why it has its own domain.
pub fn qwen36_shape_id_v1(s: &Qwen36ShapeV1) -> Hash64 {
    let kinds: Vec<u8> = s
        .layer_types
        .iter()
        .map(|k| match k {
            crate::qwen36::Qwen36LayerKind::LinearAttention => 0u8,
            crate::qwen36::Qwen36LayerKind::FullAttention => 1u8,
        })
        .collect();
    let mut scalars = Vec::with_capacity(16 * 8);
    for v in [
        s.d_model,
        s.n_heads,
        s.n_kv_heads,
        s.head_dim,
        s.rotary_dim,
        s.linear_k_heads,
        s.linear_v_heads,
        s.linear_head_dim,
        s.conv_kernel,
        s.n_experts,
        s.experts_per_token,
        s.moe_dim,
        s.shared_dim,
        s.vocab,
        s.max_position,
    ] {
        scalars.extend_from_slice(&(v as u64).to_le_bytes());
    }
    scalars.extend_from_slice(&s.eps_q.to_le_bytes());
    scalars.push(s.router_up_bits);
    keyed(QWEN36_DOMAIN_SHAPE, &[&kinds, &scalars])
}

/// **The prompt a template's anchor implies.**
///
/// A producer must not choose its own prompt: a class whose executor picks the input is a class
/// where "run the model" and "find an input whose output I like" are the same move. So the ids are
/// a pure function of the anchor — the same construction BASE-0 uses, under this class's own
/// domain.
pub fn qwen36_prompt_for_anchor(anchor: Hash64, vocab: usize, prefill: u32) -> Vec<usize> {
    let mut prompt = Vec::with_capacity(prefill as usize);
    let mut counter = 0u64;
    while prompt.len() < prefill as usize {
        let block = keyed(QWEN36_DOMAIN_JOB_PROMPT, &[anchor.as_byte_slice(), &counter.to_le_bytes()]);
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

/// One Qwen3.6 class, bound to its artifact.
pub struct Qwen36Backend {
    /// `Arc`, because a node resolves per block while the artifact is a 33 GiB mapping opened
    /// once: the per-block cost must be a pointer clone, and a by-value artifact would make every
    /// resolve either a re-map or an impossible clone.
    artifact: std::sync::Arc<Qwen36ArtifactV1>,
    model_id: String,
    /// `(prefill, decode)` — the canonical job's shape, a class fact.
    canonical_job: (u32, u32),
    shape_id: Hash64,
    /// **The chain's id for this class** — `qwen36_profile_v1(...).shape_profile_id()`, passed in
    /// rather than re-derived per call (the profile is 95 nodes × 40 layers). The job context
    /// carries it, so the job a seat re-derives and the class the chain named cannot disagree.
    class_profile_id: Hash64,
    /// The network the node runs, from its own configuration — a job context is not portable
    /// across networks and a hardcoded string said otherwise.
    network_id: Vec<u8>,
    /// **ADR-0067: `Some` when this backend executes FROM the registered declaration.** The plan
    /// is compiled at construction — every declared node bound to a served kernel and a resolved
    /// operand, or the constructor refuses with the node named — and every forward walks it.
    /// `None` is the compiled engine, kept for the rows this build's own ledger names (and as the
    /// interpreter's reference vectors, per the differentials beside the plan).
    plan: Option<crate::qwen36_plan::Qwen36ProfilePlanV1>,
    /// The registered graph itself, kept beside the plan it compiled to: the capture places rows
    /// at the PROFILE's coordinates and the binding carries it whole, so a backend that dropped
    /// it after planning could execute but never commit a step space.
    profile: Option<kaspa_consensus_core::palw_step::PalwShapeProfileV3>,
    /// **The ladder top this instance prices a job against and refuses a served capture above** —
    /// the ruleset's `PalwCourtParamsV2::max_step_leaf_count`, defaulting to the leg's own
    /// constant (which is what every shipped preset froze). The dense tier already carried this;
    /// the hybrid one read the constant at five separate sites.
    step_ladder_cap: u64,
}

impl Qwen36Backend {
    pub fn new(
        artifact: std::sync::Arc<Qwen36ArtifactV1>,
        model_id: impl Into<String>,
        canonical_job: (u32, u32),
        class_profile_id: Hash64,
        network_id: Vec<u8>,
    ) -> Self {
        let shape_id = qwen36_shape_id_v1(&artifact.shape);
        // **The ledger-compiled authority captures too, when its class can carry a capture.**
        // The caller names the class by id; when this build's own ledger holds that class's
        // graph (a `graph_version >= 2` row — the criterion by which a trace can fill the
        // declared step space) and the interpreter can serve it over THIS artifact, the plan is
        // compiled here and every execute commits the captured binding — the same commitment the
        // chain-registered constructor produces, which is what keeps the two authorities one
        // protocol. A v1 row, a foreign id or a contradicted artifact stays on the legacy
        // composite and says `supports_court() == false`, stated rather than guessed.
        let armed = crate::classes::qwen36_canonical_classes_v1()
            .into_iter()
            .filter(|row| row.graph_version >= 2)
            .filter_map(|row| row.profile().ok())
            .find(|profile| profile.shape_profile_id() == class_profile_id)
            .and_then(|profile| {
                let plan = Qwen36Engine::new(&artifact).plan_from_profile(&profile).ok()?;
                Some((plan, profile))
            });
        let (plan, profile) = match armed {
            Some((plan, profile)) => (Some(plan), Some(profile)),
            None => (None, None),
        };
        Self {
            artifact,
            model_id: model_id.into(),
            canonical_job,
            shape_id,
            class_profile_id,
            network_id,
            plan,
            profile,
            step_ladder_cap: LEG_MAX_LEAVES,
        }
    }

    /// **The ladder top from the ruleset**, for a caller that holds `PalwCourtParamsV2`. Passing
    /// `max_step_leaf_count` is the only correct argument; the constructors pass the leg's default,
    /// which is what every shipped preset froze.
    pub fn with_step_ladder_cap(mut self, max_step_leaf_count: u64) -> Self {
        self.step_ladder_cap = max_step_leaf_count;
        self
    }

    /// The ladder top this instance prices a job against.
    pub fn step_ladder_cap(&self) -> u64 {
        self.step_ladder_cap
    }

    /// **The ledger-compiled authority, handed the graph it serves** — for callers that already
    /// hold the class's profile (a resolved ledger row, or a test's own fixture class) rather
    /// than only its id. Arms the capture exactly when the interpreter can serve the graph over
    /// this artifact; an unservable declaration keeps the legacy composite and stays
    /// court-incapable, because refusing to RUN is the registered-constructor's job
    /// ([`Self::from_registered_profile`]) and this one's callers chose the class themselves.
    pub fn with_class_profile(
        artifact: std::sync::Arc<Qwen36ArtifactV1>,
        model_id: impl Into<String>,
        canonical_job: (u32, u32),
        profile: kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        network_id: Vec<u8>,
    ) -> Self {
        let shape_id = qwen36_shape_id_v1(&artifact.shape);
        let class_profile_id = profile.shape_profile_id();
        let (plan, profile) = match Qwen36Engine::new(&artifact).plan_from_profile(&profile) {
            Ok(plan) => (Some(plan), Some(profile)),
            Err(_) => (None, None),
        };
        Self {
            artifact,
            model_id: model_id.into(),
            canonical_job,
            shape_id,
            class_profile_id,
            network_id,
            plan,
            profile,
            step_ladder_cap: LEG_MAX_LEAVES,
        }
    }

    /// **ADR-0067 Decision 2's constructor for the mmap container: a backend for a class this
    /// build's ledger never heard of.** The profile arrives from chain state (the registration's
    /// admission carriage), and the plan it compiles to IS the admission decision — a graph
    /// outside this build's kernel vocabulary, or one this artifact's geometry contradicts, is
    /// refused here with the node or the field named, before anything executes. The class id is
    /// derived from the profile (the id IS the declaration), never passed in.
    pub fn from_registered_profile(
        artifact: std::sync::Arc<Qwen36ArtifactV1>,
        network_id: Vec<u8>,
        profile: kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        canonical_job: (u32, u32),
    ) -> Result<Self, String> {
        let engine = Qwen36Engine::new(&artifact);
        // The A16 container's sibling, and the same distinction (round-3 defect I-3): a capacity
        // refusal by THIS build is not "the graph is unservable", and an operator must be able to
        // tell them apart from the log line alone.
        let plan = engine.plan_from_profile(&profile).map_err(|e| match e {
            crate::qwen36_plan::Qwen36PlanErrorV1::OverMemoryCeiling { bytes, ceiling } => format!(
                "this node's interpreted-execution capacity refuses the registered graph: one token's committed trace \
                 is {bytes} bytes and this build's capacity is {ceiling} (ADR-0067 SA-1). The chain's admission caps \
                 accepted this class and do not bound a declared row's width, so this is node-local servability, not a \
                 statement about the class: a node built with a larger ceiling serves it, and this one will not produce \
                 or judge for it"
            ),
            other => format!("this build cannot serve the registered graph: {other}"),
        })?;
        let shape_id = qwen36_shape_id_v1(&artifact.shape);
        let class_profile_id = profile.shape_profile_id();
        Ok(Self {
            artifact,
            model_id: "PALW-QWEN36/chain-registered".to_string(),
            canonical_job,
            shape_id,
            class_profile_id,
            network_id,
            plan: Some(plan),
            profile: Some(profile),
            step_ladder_cap: LEG_MAX_LEAVES,
        })
    }

    /// One forward, through whichever authority constructed this backend: the registered plan
    /// where one exists, the compiled engine where the build's own ledger named the class. The
    /// untraced planned walk, because this path needs the logit row and nothing else.
    fn forward(&self, engine: &Qwen36Engine<'_>, cache: &mut Qwen36Cache, token: usize, position: usize) -> Result<Vec<i32>, String> {
        match &self.plan {
            Some(plan) => {
                engine.forward_token_planned_logits(plan, cache, token, position).map_err(|e| format!("planned forward: {e}"))
            }
            None => engine.forward_token(cache, token, position).map_err(|e| e.to_string()),
        }
    }

    pub fn artifact(&self) -> &Qwen36ArtifactV1 {
        &self.artifact
    }

    /// The CHAIN's id for the class this backend serves — what a caller compares against the
    /// class a registration names.
    pub fn class_profile_id(&self) -> Hash64 {
        self.class_profile_id
    }

    pub fn shape_id(&self) -> Hash64 {
        self.shape_id
    }

    /// Run the canonical job and keep everything a commitment is computed from.
    fn run(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<Qwen36RunV1, String> {
        let engine = Qwen36Engine::new(&self.artifact);
        let mut cache = Qwen36Cache::new(&self.artifact.shape);
        let mut logits_rows: Vec<Vec<i32>> = Vec::with_capacity(prompt.len() + job.exact_decode_tokens as usize);
        let mut generated: Vec<u32> = Vec::with_capacity(job.exact_decode_tokens as usize);

        for (position, token) in prompt.iter().enumerate() {
            let row = self.forward(&engine, &mut cache, *token, position).map_err(|e| format!("prefill at {position}: {e}"))?;
            logits_rows.push(row);
        }
        // The decode budget is EXACT: an early end-of-generation is telemetry and never terminates,
        // because a job whose length depends on what the model said is a job whose cost a producer
        // controls.
        for step in 0..job.exact_decode_tokens as usize {
            let last = logits_rows.last().ok_or_else(|| "an empty prefill".to_string())?;
            let next = crate::engine::argmax_lowest(last) as u32;
            generated.push(next);
            let position = prompt.len() + step;
            if position >= self.artifact.shape.max_position {
                return Err(format!("the job runs past the rotary table at position {position}"));
            }
            let row = self.forward(&engine, &mut cache, next as usize, position).map_err(|e| format!("decode at {position}: {e}"))?;
            logits_rows.push(row);
        }
        Ok(Qwen36RunV1 { logits_rows, generated })
    }
}

/// What one execution produced, before it is committed to.
pub struct Qwen36RunV1 {
    pub logits_rows: Vec<Vec<i32>>,
    pub generated: Vec<u32>,
}

/// **One planned pass's rows, in the step space's own coordinates.** The trace records one row per
/// declared node per table; the capture places them by `(table kind, absolute layer, index)`, and
/// the layer's KIND comes from the profile — a GDN row filed as `Attn` is a row about a different
/// graph, and `push_call` refuses it.
pub fn qwen36_captured_rows_v1(
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    trace: &crate::qwen36_plan::Qwen36PlanTraceV1,
) -> Vec<crate::legs::Base0CapturedRowV1> {
    use kaspa_consensus_core::palw_step::{PalwLayerKindV1, PalwStepTableV1};
    let mut rows = Vec::with_capacity(trace.pre.len() + trace.post.len() + trace.layers.iter().map(Vec::len).sum::<usize>());
    for (index, row) in trace.pre.iter().enumerate() {
        rows.push(crate::legs::Base0CapturedRowV1 { table: PalwStepTableV1::Pre, layer: 0, index, row: row.clone() });
    }
    for (layer, nodes) in trace.layers.iter().enumerate() {
        let table = match profile.layer_kind(layer as u16) {
            PalwLayerKindV1::GatedDeltaNet => PalwStepTableV1::Gdn,
            PalwLayerKindV1::Attention => PalwStepTableV1::Attn,
        };
        for (index, row) in nodes.iter().enumerate() {
            rows.push(crate::legs::Base0CapturedRowV1 { table, layer: layer as u16, index, row: row.clone() });
        }
    }
    for (index, row) in trace.post.iter().enumerate() {
        rows.push(crate::legs::Base0CapturedRowV1 { table: PalwStepTableV1::Post, layer: 0, index, row: row.clone() });
    }
    rows
}

/// **The hybrid's checkpoint cadence: none, canonically.** The class registers no state chunk map
/// (`state_chunk_map_id` is the sentinel — the recurrence is genesis-anchored by declaration), so
/// no checkpoint can ever be CAPTURED; what makes zero also the canonical COUNT is the interval:
/// at `n_ctx`, every legal job's decode-call count sits below it, `decode_calls / interval` is
/// zero, and the leg is the empty one whose sentinel pairing the shape pass checks. A registered
/// map later replaces this constant with the real cadence — and moves the class id with it.
pub fn qwen36_checkpoint_profile_v1(
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
) -> kaspa_consensus_core::palw_legs::PalwCheckpointProfileV1 {
    kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(profile.n_ctx.max(1))
}

/// **The hybrid tier's captured attempt** — the same object the floor's and the dense tier's
/// captured runs return, because it answers the same three verbs. What differs is the walk (the
/// planned interpreter, one committed row per declared node) and the checkpoint leg (empty by
/// construction — see [`qwen36_checkpoint_profile_v1`]).
pub fn qwen36_execute_for_attempt_v1(
    artifact: &Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    plan: &crate::qwen36_plan::Qwen36ProfilePlanV1,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
) -> Result<crate::produce::Base0ExecutionV1, String> {
    qwen36_execute_for_attempt_capped_v1(artifact, profile, plan, ctx, prompt, kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES)
}

/// [`qwen36_execute_for_attempt_v1`] against the ladder top the CALLER states — the ruleset's
/// `PalwCourtParamsV2::max_step_leaf_count`.
pub fn qwen36_execute_for_attempt_capped_v1(
    artifact: &Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    plan: &crate::qwen36_plan::Qwen36ProfilePlanV1,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
) -> Result<crate::produce::Base0ExecutionV1, String> {
    qwen36_execute_for_attempt_streaming_capped_v1(artifact, profile, plan, ctx, prompt, max_step_leaf_count, &mut |_| {})
}

/// **The same capture, with each id handed over as it is SELECTED** (ADR-0077 Decision 2).
///
/// The streaming verb is the loop; the non-streaming one is the loop with a callback that does
/// nothing. On this tier the point is sharpest: one decode call is ~9 s of real inference, so a
/// stream assembled after the run would show the user nothing for the whole job and a second run
/// to feed it would double a 33 GiB model's work — and commit an answer nobody watched.
pub fn qwen36_execute_for_attempt_streaming_v1(
    artifact: &Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    plan: &crate::qwen36_plan::Qwen36ProfilePlanV1,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    on_token: &mut dyn FnMut(u32),
) -> Result<crate::produce::Base0ExecutionV1, String> {
    qwen36_execute_for_attempt_streaming_capped_v1(
        artifact,
        profile,
        plan,
        ctx,
        prompt,
        kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES,
        on_token,
    )
}

/// **The hybrid tier's capture, priced against the RULESET's ladder** (ADR-0077 Decision 12) — the
/// same threading the dense tier's `a16_execute_for_attempt_streaming_capped_v1` carries, and for
/// the same reason: the ladder the job is counted against is what decides how many tokens a user
/// gets, and reading it off a module constant makes that a build-time fact rather than a network
/// one. The delegating entry points above pass `PALW_STEP_MAX_LEAVES`, which is what every shipped
/// preset froze, so a caller that holds no ruleset is byte-identical to what it was.
#[allow(clippy::too_many_arguments)]
pub fn qwen36_execute_for_attempt_streaming_capped_v1(
    artifact: &Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    plan: &crate::qwen36_plan::Qwen36ProfilePlanV1,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
    on_token: &mut dyn FnMut(u32),
) -> Result<crate::produce::Base0ExecutionV1, String> {
    qwen36_execute_streaming_v1(
        artifact,
        profile,
        plan,
        ctx,
        prompt,
        max_step_leaf_count,
        crate::legs::Base0CaptureKindV1::DenseTiles,
        on_token,
    )
}

/// **The same run, FOLDED** (ADR-0082 Decision 7) — the free-prompt lane's capture on the hybrid
/// tier. The dense tier's `a16_execute_free_prompt_streaming_v1`, for its reasons: one loop, one
/// enumeration, one set of roots, and a retention of one node per `2^retain_level` leaves instead
/// of every tile of every node of every position (~298 k leaves a position here).
#[allow(clippy::too_many_arguments)]
pub fn qwen36_execute_free_prompt_streaming_v1(
    artifact: &Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    plan: &crate::qwen36_plan::Qwen36ProfilePlanV1,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
    on_token: &mut dyn FnMut(u32),
) -> Result<crate::produce::Base0ExecutionV1, String> {
    qwen36_execute_streaming_v1(
        artifact,
        profile,
        plan,
        ctx,
        prompt,
        max_step_leaf_count,
        crate::legs::Base0CaptureKindV1::Fold,
        on_token,
    )
}

/// **The one capture loop this family has**, over either sink.
#[allow(clippy::too_many_arguments)]
fn qwen36_execute_streaming_v1(
    artifact: &Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    plan: &crate::qwen36_plan::Qwen36ProfilePlanV1,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
    capture_kind: crate::legs::Base0CaptureKindV1,
    on_token: &mut dyn FnMut(u32),
) -> Result<crate::produce::Base0ExecutionV1, String> {
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

    let engine = Qwen36Engine::new(artifact);
    let leaf_count =
        kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, ctx, max_step_leaf_count).map_err(|e| format!("{e:?}"))?;
    let mut capture = crate::legs::Base0CaptureSinkV1::for_kind(capture_kind, profile, ctx, leaf_count, max_step_leaf_count)
        .map_err(|e| format!("{e:?}"))?;
    let checkpoint_profile = qwen36_checkpoint_profile_v1(profile);
    let checkpoints = crate::legs::Base0CheckpointCaptureV1::new(ctx, profile, &checkpoint_profile);
    let mut cache = Qwen36Cache::new(&artifact.shape);

    let mut logits_rows: Vec<Vec<i32>> = Vec::with_capacity(decode_tokens);
    let mut generated: Vec<u32> = Vec::with_capacity(decode_tokens);

    // Call 0 — prefill. Post rows exist only at its LAST position; earlier rows predict tokens
    // the prompt already contains, and the step space has no coordinate for them.
    let mut last_logits = Vec::new();
    for (position, token) in prompt.iter().take(prefill).enumerate() {
        let (logits, trace) =
            engine.forward_token_planned(plan, &mut cache, *token, position).map_err(|e| format!("prefill at {position}: {e}"))?;
        let mut rows = qwen36_captured_rows_v1(profile, &trace);
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

    for call in 1..decode_tokens {
        let cache_position = prefill + call - 1;
        if cache_position >= artifact.shape.max_position {
            return Err(format!("the job runs past the rotary table at position {cache_position}"));
        }
        let (logits, trace) = engine
            .forward_token_planned(plan, &mut cache, next as usize, cache_position)
            .map_err(|e| format!("decode at {cache_position}: {e}"))?;
        let rows = qwen36_captured_rows_v1(profile, &trace);
        capture.push_call(profile, ctx, call as u32, 0, &rows).map_err(|e| format!("{e:?}"))?;
        next = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&logits) as u32;
        generated.push(next);
        on_token(next);
        logits_rows.push(logits);
    }

    let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    let checkpoints = checkpoints.finish(decode_calls / checkpoint_profile.checkpoint_interval).map_err(|e| format!("{e:?}"))?;
    let captured = capture.finish(max_step_leaf_count).map_err(|e| format!("{e:?}"))?;

    // The retained rows ARE the selecting rows — row `r` is the one `generated[r]` was chosen
    // from — and the tiled root commits them directly.
    let trace_root = kaspa_consensus_core::palw_step_refute::tiled_logits_trace_root_v1(ctx, &logits_rows, &generated)
        .ok_or_else(|| "the retained rows build no tree".to_string())?;
    let activation_leg_root = crate::produce::base0_activation_leg_root_v1(ctx);
    let binding = crate::legs::base0_binding_from_step_root_v1(
        profile,
        ctx,
        captured.step_leaf_count,
        captured.step_merkle_root,
        &checkpoints,
        &checkpoint_profile,
        trace_root,
        activation_leg_root,
    )
    .map_err(|e| format!("{e:?}"))?;
    let (tiles, step_tree) = captured.into_execution_parts();

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
        step_tree,
        checkpoints,
        logits_rows,
        generated_token_ids: generated,
    })
}

/// The four roots, from a run.
///
/// `execution_root` is a composite over the job, the trace and the output. In BASE-0 that slot
/// holds the step leg's binding, which a refutation is pinned against; here there is no step leg
/// yet, so it holds the thing that is true today and is stated as such rather than dressed up.
/// `None` when the run does not carry the rows it claims to have selected from — see the refusal
/// inside. Every caller must treat that as "this material answers nothing", never as a root.
pub fn qwen36_roots_v1(job: &PalwJobContextV2, shape_id: Hash64, run: &Qwen36RunV1) -> Option<(Hash64, Hash64, Hash64, Hash64)> {
    let context = job.context_hash();
    // **The tiled trace, over the SELECTING rows.** The run keeps every logits row it produced —
    // prefill rows included — but the committed set is one row per generated token: the row that
    // token was selected FROM, which is `rows[prefill − 1 + i]`. Committing the prefill rows too
    // would put `prefill × vocab` lanes behind the root for no adjudicable claim: no token is
    // selected from them, so no decode-token dispute can ever open one.
    let prefill = job.declared_prefill_tokens as usize;
    // **A missing row is a refusal, never an empty one** (ADR-0068 launch audit, the panel-seat
    // panic).
    //
    // This read `.cloned().unwrap_or_default()`, which fabricated an empty `Vec<i32>` wherever the
    // material did not carry the row a token was selected from. That is a lie with teeth: an empty
    // row has no lanes, so `tiled_logits_row_root_v1` tiles it into zero leaves and
    // `step_merkle_root_v1` refuses a zero-leaf tree — under an `.expect`, in the panel service, on
    // material ANYONE may gossip with no bond. One message with `rows = 0, generated = 1` killed
    // every seat that read it, and seats are what a claim needs to license and a court needs to
    // open, so a bondless message could disarm the court.
    //
    // The honest answer is that material which does not carry the row it says a token came from is
    // material that answers nothing — `verify_material` turns this `None` into `Unverifiable`,
    // which is exactly the verdict for bytes a seat cannot check.
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
    // Nothing renders text on this path — the class commits token ids — so the rendered-output
    // hash is over the ids' own encoding rather than over bytes no one produced.
    let rendered =
        keyed(QWEN36_DOMAIN_EXECUTION, &[b"rendered", &run.generated.iter().flat_map(|t| t.to_le_bytes()).collect::<Vec<_>>()]);
    let output_root = output_commitment_v2(&context, &run.generated, &rendered);
    let execution_root = keyed(
        QWEN36_DOMAIN_EXECUTION,
        &[context.as_byte_slice(), shape_id.as_byte_slice(), trace_root.as_byte_slice(), output_root.as_byte_slice()],
    );
    // The consensus derivation (ADR-0072 Decision 8), the same one `execute` commits to — a seat
    // that recomputed this family's old domain hash here would refuse every honest claim.
    let manifest = kaspa_consensus_core::palw_attempt_v2::attempt_trace_manifest_root_v1(trace_root, 1);
    Some((trace_root, output_root, execution_root, manifest))
}

/// The retained material: the logit rows and the generated ids, which is everything a seat needs
/// to recompute the roots without re-running the model.
pub fn qwen36_material_encode_v1(run: &Qwen36RunV1) -> Vec<u8> {
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

/// Decode retained material. Returns `None` for bytes that are not this format — a seat's honest
/// "unavailable" rather than an accusation.
pub fn qwen36_material_decode_v1(bytes: &[u8]) -> Option<Qwen36RunV1> {
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
    let mut logits_rows = Vec::with_capacity(rows.min(1 << 16));
    for _ in 0..rows {
        let n = u64_at(&mut i)? as usize;
        let end = i.checked_add(n.checked_mul(4)?)?;
        if end > bytes.len() {
            return None;
        }
        logits_rows.push(bytes[i..end].chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().expect("4"))).collect());
        i = end;
    }
    let n = u64_at(&mut i)? as usize;
    let end = i.checked_add(n.checked_mul(4)?)?;
    if end > bytes.len() {
        return None;
    }
    let generated = bytes[i..end].chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().expect("4"))).collect();
    (end == bytes.len()).then_some(Qwen36RunV1 { logits_rows, generated })
}

impl Qwen36Backend {
    /// **A folded retention, re-executed into the dense capture the court's assembly reads**
    /// (ADR-0082 Decision 7) — the dense tier's `dense_capture_from_fold_v1`, for its reasons:
    /// `base0_refutation_from_capture_capped_v1` needs the whole leaf vector, re-deriving exactly
    /// the leaves a refutation reads is ADR-0082 U-03's work, and until then the party that wants
    /// to prosecute pays for one re-execution of a job whose ids it holds.
    fn dense_capture_from_fold_v1(
        &self,
        material: &crate::produce::Base0FpMaterialV2,
    ) -> Result<crate::produce::Base0ExecutionV1, String> {
        let (Some(plan), Some(_)) = (&self.plan, &self.profile) else {
            return Err("this backend serves no registered graph, so it cannot re-execute a folded capture".to_string());
        };
        let prompt: Vec<usize> = material.prompt_token_ids.iter().map(|t| *t as usize).collect();
        let run = qwen36_execute_for_attempt_streaming_capped_v1(
            &self.artifact,
            &material.binding.shape_profile,
            plan,
            &material.binding.job_context,
            &prompt,
            self.step_ladder_cap,
            &mut |_| {},
        )?;
        if run.binding.committed_execution_root != material.binding.committed_execution_root {
            return Err("the retained fold and its re-execution are not one execution".to_string());
        }
        Ok(run)
    }

    /// The dense tiles either retention can answer with.
    fn tiles_from_material_v1(&self, retention: &crate::produce::Base0RetentionV1) -> Result<crate::legs::Base0StepTilesV1, String> {
        match retention {
            crate::produce::Base0RetentionV1::Dense((binding, tiles, ..)) => {
                Ok(crate::legs::Base0StepTilesV1 { leaves: qwen36_leaves_by_position(binding, tiles), tiles: tiles.clone() })
            }
            crate::produce::Base0RetentionV1::Folded(material) => Ok(self.dense_capture_from_fold_v1(material)?.tiles),
        }
    }

    /// One refutation at `index`, with the prompt either CARRIED by the caller — a free-prompt
    /// lane's, whose tokens the user chose, checked against the capture's own commitment — or
    /// DERIVED from the anchor, the attempt lane's. The split A16 made in its
    /// `refutation_with_prompt`, for the same reason: a prover that can only re-derive the prompt
    /// opens nothing on a free-prompt capture, and a refutation with no prompt refutes nothing
    /// (ADR-0073 Decision 1, ADR-0075).
    fn refutation_with_prompt(
        &self,
        material: &[u8],
        index: u64,
        carried: Option<&[u32]>,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        let retention =
            crate::produce::base0_material_decode_any_v1(material).map_err(|_| "the capture does not decode".to_string())?;
        let binding = retention.binding().clone();
        let logits_rows = retention.logits_rows().to_vec();
        let generated = retention.generated_token_ids().to_vec();
        if binding.step_leaf_count == 0 || binding.step_leaf_count > self.step_ladder_cap {
            return Err("the binding's leaf count is outside the ruleset's ladder".to_string());
        }
        let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, index)
            .ok_or_else(|| format!("leaf {index} is not a main step coordinate"))?;
        let step_tiles = self.tiles_from_material_v1(&retention)?;

        let rows_root = kaspa_consensus_core::palw_step_refute::tiled_logits_rows_root_v1(&binding.job_context, &logits_rows)
            .ok_or_else(|| "the retained rows build no tree".to_string())?;
        let pin = kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1::TiledV1(
            kaspa_consensus_core::palw_step_refute::PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: generated },
        );

        let prompt_token_ids: Vec<u32> = match carried {
            Some(ids) => {
                if kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(ids) != binding.job_context.prompt_token_ids_hash {
                    return Err("the carried prompt is not the one this capture's job context commits to".to_string());
                }
                ids.to_vec()
            }
            None => {
                let derived = qwen36_prompt_for_anchor(
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

        crate::legs::base0_refutation_from_capture_v1(
            &binding.shape_profile.clone(),
            &binding.job_context.clone(),
            &step_tiles,
            binding,
            coord,
            prompt_token_ids,
            Some(pin),
            None,
        )
        .map_err(|e| format!("{e:?}"))
    }
}

/// **The hybrid tier's kernels, as a seat's interval replay needs them** (ADR-0077 Decision 8).
///
/// Genesis-anchored only, and that is a statement about the CLASS rather than a shortcut here: the
/// registered graph declares no state chunk map (`state_chunk_map_id` is the sentinel), so it
/// commits no checkpoint any replay could resume from, and interval 0 is the whole job. ADR-0077
/// Decision 10 is what changes that — a registered recurrence state map (the GatedDeltaNet state
/// plus the conv window, [`crate::fp_capture`]) moves the class id, and the `Checkpoint` arm below
/// becomes a restore instead of a refusal.
struct Qwen36IntervalKernels<'a> {
    artifact: &'a Qwen36ArtifactV1,
    plan: &'a crate::qwen36_plan::Qwen36ProfilePlanV1,
}

impl crate::fp_interval::Base0FpIntervalKernelsV1 for Qwen36IntervalKernels<'_> {
    fn replay_interval(
        &self,
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        start: &crate::fp_interval::Base0FpIntervalStartV1<'_>,
        first_call: u32,
        last_call: u32,
    ) -> Result<Vec<(u64, Hash64)>, String> {
        let crate::fp_interval::Base0FpIntervalStartV1::Genesis { .. } = start else {
            return Err(
                "this class registers no state chunk map, so no committed checkpoint exists to resume from (ADR-0077 Decision 10)"
                    .to_string(),
            );
        };
        let engine = Qwen36Engine::new(self.artifact);
        let mut cache = Qwen36Cache::new(&self.artifact.shape);
        let vocab = self.artifact.shape.vocab;
        let max_position = self.artifact.shape.max_position;
        crate::fp_interval::base0_fp_replay_interval_v1(profile, ctx, start, first_call, last_call, |token, position| {
            if token >= vocab {
                return Err(format!("token {token} is outside this class's vocabulary of {vocab}"));
            }
            if position >= max_position {
                return Err(format!("the job runs past the rotary table at position {position}"));
            }
            let (logits, trace) = engine
                .forward_token_planned(self.plan, &mut cache, token, position)
                .map_err(|e| format!("forward at {position}: {e}"))?;
            Ok((logits, qwen36_captured_rows_v1(profile, &trace)))
        })
    }
}

impl Qwen36Backend {
    /// The cadence this class checkpoints at — `n_ctx`, which is above every legal job's decode
    /// count, so `decode_calls / interval` is zero and the leg is the empty one
    /// ([`qwen36_checkpoint_profile_v1`]). `None` for a backend serving no registered graph.
    fn checkpoint_interval(&self) -> Option<u32> {
        self.profile.as_ref().map(|p| qwen36_checkpoint_profile_v1(p).checkpoint_interval)
    }

    /// **ADR-0077 SA-6, at the job boundary.**
    ///
    /// The artifact is opened read-only (`PROT_READ`, `MAP_PRIVATE`, an `O_RDONLY` descriptor —
    /// `crate::mmap::ReadOnlyMap`), so nothing this process does can write it. What CAN change is
    /// the file under the mapping, and the failure mode of a directory extent that no longer lies
    /// inside it is a read past the end. `Qwen36ArtifactV1::tensor` already answers that with a
    /// refusal rather than a fault; this walks every extent through it once, at the job boundary,
    /// so a host whose artifact has been truncated or swapped reports `JobFailed` with the tensor
    /// named instead of failing forty layers into a decode call. It touches directory entries, not
    /// pages: the cost is a `BTreeMap` walk, not a re-read of 33 GiB.
    fn artifact_read_probe_v1(&self) -> Result<(), String> {
        let names: Vec<String> = self.artifact.tensor_names().into_iter().map(str::to_string).collect();
        for name in names {
            self.artifact
                .tensor(&name)
                .map_err(|e| format!("this host can no longer read the mapped artifact: tensor {name}: {e}"))?;
        }
        Ok(())
    }
}

impl PalwExecutionBackendV1 for Qwen36Backend {
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
        let prompt = qwen36_prompt_for_anchor(anchor, shape.vocab, prefill);
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let ctx = PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: self.network_id.clone(),
            job_id: anchor,
            job_nullifier: keyed(QWEN36_DOMAIN_EXECUTION, &[b"nullifier", anchor.as_byte_slice()]),
            assignment_id: Hash64::default(),
            execution_seed: anchor.as_byte_slice()[..32].try_into().expect("a 64-byte hash has 32 bytes"),
            model_profile_id: self.shape_id,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: self.shape_id,
            // The COURT's id, not the backend's own shape hash: the chain registered the class by
            // its shape profile, and a job that named anything else would be a job for a class
            // that does not exist.
            shape_profile_id: self.class_profile_id,
            // The TILED commitment (the flat one prices a decode-token close at decode × vocab ×
            // 4 bytes, which at this vocabulary is megabytes against the ~80 KiB a lifecycle
            // carrier can relay). Declared here and nowhere else on this path: the scheme is what
            // the class registers, and the binding check compares this field against it.
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
        // **The captured attempt, where the declaration is the program.** A plan proves this
        // build serves the registered graph node for node, and the planned traced walk is what a
        // capture is placed from — so court capability rides exactly the constructor that proves
        // servability. The ledger-compiled path keeps the legacy composite: an engine whose op
        // order is this build's hardcode cannot commit a step space the COURT's coordinates
        // describe unless the two provably correspond, and the plan is that proof.
        if let (Some(plan), Some(profile)) = (&self.plan, &self.profile) {
            let run = qwen36_execute_for_attempt_capped_v1(&self.artifact, profile, plan, job, prompt, self.step_ladder_cap)?;
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
        // Unreachable for a run this backend just performed — `run` keeps a row per position and
        // decodes exactly `exact_decode_tokens` — and an error rather than an `expect` because the
        // one thing worse than a producer that cannot commit is a producer that panics instead.
        let (trace_root, output_root, execution_root, trace_manifest_root) = qwen36_roots_v1(job, self.shape_id, &run)
            .ok_or_else(|| "this run did not keep the rows its tokens were selected from".to_string())?;
        Ok(PalwExecutionOutcomeV1 {
            trace_root,
            output_root,
            execution_root,
            trace_manifest_root,
            trace_chunk_count: 1,
            material: qwen36_material_encode_v1(&run),
        })
    }

    /// **The free-prompt lane, on the registered graph** (ADR-0044, ADR-0074, ADR-0075). The
    /// caller's tokens ARE the prompt — nothing here derives one from an anchor — and the run is
    /// the same captured step leg the attempt lane commits, priced by its own leaf count
    /// (ADR-0074 Decision 5). Only a backend serving a registered graph can commit it: the
    /// composite path keeps no capture, and a claim without a step leg is not a claim.
    fn execute_free_prompt(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        self.execute_free_prompt_streaming(job, prompt_tokens, &mut |_| {})
    }

    /// The streaming verb IS the run; the non-streaming one is this with a callback that does
    /// nothing (ADR-0077 Decision 2). One inference, one capture, one commitment.
    fn execute_free_prompt_streaming(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        on_token: &mut dyn FnMut(u32),
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3, palw_fp_job_context_v3};
        use kaspa_consensus_core::palw_freeprompt_v3::PalwFpStopReasonV3;

        // ADR-0077 SA-6: the artifact is a 33 GiB read-only mapping and a job may outlive the file
        // it was opened from. A directory extent that no longer lies inside the mapping is named
        // HERE, as a job failure, rather than taken as a fault deep in a kernel.
        self.artifact_read_probe_v1()?;

        let (Some(plan), Some(profile)) = (&self.plan, &self.profile) else {
            return Err("this backend serves no registered graph, so it cannot commit a free-prompt step leg".to_string());
        };
        if job.prompt_tokens as usize != prompt_tokens.len() {
            return Err(format!("the job declares {} prompt tokens and {} were supplied", job.prompt_tokens, prompt_tokens.len()));
        }
        if prompt_tokens.is_empty() {
            return Err("a job with no prompt tokens is not a job".to_string());
        }
        let vocab = self.artifact.shape.vocab;
        if let Some(bad) = prompt_tokens.iter().find(|t| **t >= vocab) {
            return Err(format!("token {bad} is outside this class's vocabulary of {vocab}"));
        }

        let class = PalwFpClassFactsV3 {
            model_profile_id: self.shape_id,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: self.shape_id,
            shape_profile_id: self.class_profile_id,
            cu_ruleset_id: Hash64::default(),
        };
        let shape = PalwFpRunFactsV3 {
            decode_tokens_executed: job.decode_token_limit,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: Hash64::default(),
            step_leaf_count: 0,
        };
        let ctx = palw_fp_job_context_v3(job, &class, &shape, &self.network_id).map_err(|e| format!("{e:?}"))?;

        let run = qwen36_execute_free_prompt_streaming_v1(
            &self.artifact,
            profile,
            plan,
            &ctx,
            prompt_tokens,
            self.step_ladder_cap,
            on_token,
        )?;

        let (checkpoint_leg_root, step_leg_root) = crate::legs::base0_leg_roots_from_binding_v1(&run.binding);
        let prompt_ids: Vec<u32> = prompt_tokens.iter().map(|t| *t as u32).collect();
        let material = crate::produce::base0_fp_material_encode_v2(&run, &prompt_ids).map_err(|e| e.to_string())?;
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
        // The family codec first — a captured attempt's material carries its binding, and the
        // seat check rebuilds the legs from it. The legacy rows-and-ids decode stays for the
        // ledger-compiled path's claims.
        // **The fold first** (ADR-0082 Decision 7): a free-prompt claim of this class retains v2,
        // and its step root is read off the retained tree rather than rebuilt from tiles there are
        // none of.
        if let Ok(folded) = crate::produce::base0_fp_material_decode_v2(material) {
            if claim.anchor != Hash64::default() && folded.binding.job_context.job_id != claim.anchor {
                return PalwMaterialVerdictV1::Mismatch;
            }
            if folded.binding.shape_profile.shape_profile_id() != self.class_profile_id {
                return PalwMaterialVerdictV1::Unverifiable;
            }
            return match crate::produce::base0_fp_material_matches_claim_v2(&folded, claim.execution_root, claim.trace_root) {
                Ok(true) => PalwMaterialVerdictV1::Matches,
                Ok(false) => PalwMaterialVerdictV1::Mismatch,
                Err(_) => PalwMaterialVerdictV1::Unverifiable,
            };
        }
        if let Ok(decoded) = crate::produce::base0_material_decode_v1(material) {
            if claim.anchor != Hash64::default() && decoded.0.job_context.job_id != claim.anchor {
                return PalwMaterialVerdictV1::Mismatch;
            }
            if decoded.0.shape_profile.shape_profile_id() != self.class_profile_id {
                return PalwMaterialVerdictV1::Unverifiable;
            }
            return match crate::produce::base0_material_matches_claim_v1(&decoded, claim.execution_root, claim.trace_root) {
                Ok(true) => PalwMaterialVerdictV1::Matches,
                Ok(false) => PalwMaterialVerdictV1::Mismatch,
                Err(_) => PalwMaterialVerdictV1::Unverifiable,
            };
        }
        let Some(run) = qwen36_material_decode_v1(material) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        // **The claim carries the anchor now**, so the seat recomputes under the job the CHAIN
        // asked for rather than under one the material names about itself. That is the whole point
        // of the field: without it a gossiped capture is a re-usable asset — mine a fresh block,
        // announce the borrowed roots, and both halves of the check agree because both read the
        // capture. A seat with no anchor (`Hash64::default()`) has no block to bind to and says
        // `Unverifiable` rather than guessing a job.
        if claim.anchor == Hash64::default() {
            return PalwMaterialVerdictV1::Unverifiable;
        }
        let Ok((job, _)) = self.job_for_anchor(claim.anchor) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        // Material that does not carry the rows it selected from is material this seat cannot
        // check — the honest `Unverifiable`, not an accusation, and not a panic.
        let Some((trace_root, _, execution_root, _)) = qwen36_roots_v1(&job, self.shape_id, &run) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        if trace_root == claim.trace_root && execution_root == claim.execution_root {
            PalwMaterialVerdictV1::Matches
        } else {
            PalwMaterialVerdictV1::Mismatch
        }
    }

    /// The hybrid tier takes a court's turn exactly when it holds the registered graph and the
    /// plan that proves this build serves it — the pair every capture is placed by. A backend
    /// armed with neither keeps the trait's honest refusals.
    fn supports_court(&self) -> bool {
        self.plan.is_some() && self.profile.is_some()
    }

    fn capture_shape(&self, material: &[u8]) -> Option<kaspa_consensus_core::palw_backend::PalwCaptureShapeV1> {
        let retention = crate::produce::base0_material_decode_any_v1(material).ok()?;
        let binding = retention.binding();
        Some(kaspa_consensus_core::palw_backend::PalwCaptureShapeV1 {
            job_context: binding.job_context.clone(),
            step_leaf_count: binding.step_leaf_count,
        })
    }

    fn bisect_prefix_state(&self, material: &[u8], index: u64) -> Option<kaspa_hashes::Hash64> {
        let retention = crate::produce::base0_material_decode_any_v1(material).ok()?;
        let binding = retention.binding().clone();
        // The count arrived over gossip inside a borsh blob; bounding it BEFORE the allocation is
        // the lesson the seat check already wrote down.
        if binding.step_leaf_count == 0 || binding.step_leaf_count > self.step_ladder_cap {
            return None;
        }
        // A rung commits to the execution PREFIX — every leaf below the index — which a fold
        // answers by re-deriving them and a dense retention answers from what it kept.
        let tiles = self.tiles_from_material_v1(&retention).ok()?;
        Some(crate::legs::base0_bisect_prefix_state_v1(&binding.job_context, &tiles.leaves, index))
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
    // This class's interval count is one — `n_ctx` is its checkpoint cadence, so no legal job
    // reaches a checkpoint and interval 0 is the whole job (`qwen36_checkpoint_profile_v1`). The
    // count is still derived and still answered, because a seat draws against it and a family that
    // answered `None` would have its claims declined rather than checked whole.

    fn fp_interval_count(&self, capture: &[u8]) -> Option<u32> {
        let interval = self.checkpoint_interval()?;
        let retention = crate::produce::base0_material_decode_any_v1(capture).ok()?;
        crate::fp_interval::Base0FpIntervalGeometryV1::from_binding_v1(retention.binding(), interval).ok().map(|g| g.interval_count)
    }

    fn fp_interval_count_for(&self, prompt_tokens: u32, decode_tokens_executed: u32) -> Option<u32> {
        crate::fp_interval::base0_fp_interval_count_for_v1(prompt_tokens, decode_tokens_executed, self.checkpoint_interval()?)
    }

    fn open_fp_interval(&self, capture: &[u8], index: u32, prompt_token_ids: &[u32]) -> Result<Vec<u8>, String> {
        let interval = self
            .checkpoint_interval()
            .ok_or_else(|| "this backend serves no registered graph, so it opens no interval".to_string())?;
        // Two retention forms, one opening, the class's map deciding whether the history travels —
        // ADR-0082 Decisions 7 and 9, exactly as the dense tier composes them.
        let chunked = match crate::produce::base0_material_decode_any_v1(capture).map_err(|_| "the capture does not decode".to_string())? {
            crate::produce::Base0RetentionV1::Folded(material) => {
                let plan = self.plan.as_ref().ok_or_else(|| "this backend serves no registered graph".to_string())?;
                crate::fp_interval::base0_open_fp_interval_sparse_v1(
                    &material,
                    index,
                    prompt_token_ids,
                    interval,
                    &Qwen36IntervalKernels { artifact: &self.artifact, plan },
                )
                .map_err(|e| e.to_string())?
            }
            crate::produce::Base0RetentionV1::Dense(material) => {
                crate::fp_interval::base0_open_fp_interval_v1(&material, index, prompt_token_ids, interval).map_err(|e| e.to_string())?
            }
        };
        if self.profile.as_ref().is_some_and(crate::fp_interval::base0_fp_class_requires_flat_openings_v1) {
            return crate::fp_interval::base0_strip_fp_interval_history_v1(&chunked).map_err(|e| e.to_string());
        }
        Ok(chunked)
    }

    fn verify_fp_interval_opening(
        &self,
        opening: &[u8],
        claim: PalwClaimRootsV1,
        index: u32,
        prompt_token_ids: &[u32],
        work_leaves: u64,
    ) -> kaspa_consensus_core::palw_backend::PalwFpIntervalVerdictV1 {
        let (Some(interval), Some(plan)) = (self.checkpoint_interval(), self.plan.as_ref()) else {
            return kaspa_consensus_core::palw_backend::PalwFpIntervalVerdictV1::Unverifiable;
        };
        let state = crate::fp_interval::base0_fp_interval_opening_seat_state_v1(opening, prompt_token_ids, interval);
        crate::fp_interval::base0_verify_fp_interval_opening_with_state_v1(
            opening,
            claim,
            index,
            prompt_token_ids,
            work_leaves,
            interval,
            state.as_ref(),
            &Qwen36IntervalKernels { artifact: &self.artifact, plan },
        )
        .to_consensus_v1()
    }

    /// **ADR-0082 Decision 9, the hybrid's half.**
    ///
    /// The forward is this class's own planned walk. Whether a root comes out of it is the
    /// CLASS's answer: the shipped hybrid registers the checkpoint sentinel and commits no
    /// checkpoint at all, so this refuses by name and a seat files `Incapable` — the honest
    /// verdict for a row this family cannot seat (ADR-0075). A class that registers the recurrence
    /// map gets a real root; the hybrid composition is refused by name until the side that
    /// registers it spells the order its two halves compose in.
    fn fp_recompute_checkpoint_root(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_token_ids: &[u32],
        output_token_ids: &[u32],
        decode_calls: u32,
    ) -> Result<Hash64, String> {
        use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3, palw_fp_job_context_v3};
        self.artifact_read_probe_v1()?;
        let (Some(profile), Some(plan)) = (self.profile.as_ref(), self.plan.as_ref()) else {
            return Err("this backend serves no registered graph, so it recomputes no state".to_string());
        };
        let class = PalwFpClassFactsV3 {
            model_profile_id: self.shape_id,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: self.shape_id,
            shape_profile_id: self.class_profile_id,
            cu_ruleset_id: Hash64::default(),
        };
        let shape = PalwFpRunFactsV3 {
            decode_tokens_executed: job.decode_token_limit,
            stop_reason: kaspa_consensus_core::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: Hash64::default(),
            step_leaf_count: 0,
        };
        let ctx = palw_fp_job_context_v3(job, &class, &shape, &self.network_id).map_err(|e| format!("{e:?}"))?;
        let mut kernels = crate::fp_recompute::Qwen36RecomputeKernelsV1::new(&self.artifact, plan);
        crate::fp_recompute::base0_fp_seat_state_memoized_v1(
            profile,
            &ctx,
            prompt_token_ids,
            output_token_ids,
            decode_calls,
            &mut kernels,
        )
        .map(|state| state.state_chunks_root)
        .map_err(|e| e.to_string())
    }

    fn operand_openings_for(
        &self,
        refutation: &kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1,
    ) -> Result<Vec<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1>, String> {
        let profile = self.profile.as_ref().ok_or_else(|| "this backend holds no registered graph to open against".to_string())?;
        let inventory = crate::inventory::qwen36_inventory_v1(&self.artifact, profile).map_err(|e| format!("{e:?}"))?;
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
        let (Some(plan), Some(profile)) = (&self.plan, &self.profile) else {
            return Err("a backend with no registered graph carries no capture to tamper with".to_string());
        };
        let mut run = qwen36_execute_for_attempt_capped_v1(&self.artifact, profile, plan, job, prompt, self.step_ladder_cap)?;
        let ctx_hash = job.context_hash();
        let profile_hash = profile.shape_profile_id();
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
        // **Re-derive, do not patch.** The commitment must be the corrupted capture's OWN, or
        // this is a producer whose roots disagree with its material — which any seat catches
        // without a court, and which is therefore not the fraud under test.
        let checkpoint_profile = qwen36_checkpoint_profile_v1(profile);
        let binding = crate::legs::base0_binding_from_capture_with_profile_v1(
            profile,
            job,
            &run.tiles,
            &run.checkpoints,
            &checkpoint_profile,
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
/// refutation helpers read. The floor and the dense tier each keep an identical private helper
/// beside their own backends; this is the hybrid family's copy of the same eleven lines rather
/// than a premature trait.
fn qwen36_leaves_by_position(
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
mod tests {
    use super::*;
    use crate::qwen36::Qwen36LayerKind;

    fn backend() -> Qwen36Backend {
        let artifact = crate::qwen36::test_fixture(4, 8);
        Qwen36Backend::new(
            std::sync::Arc::new(artifact),
            "Qwen3.6-fixture",
            (4, 2),
            Hash64::from_u64_word(0x36),
            b"misaka-palw-test".to_vec(),
        )
    }

    /// **A producer can run the job an anchor implies, and two producers get the same roots.**
    /// That is the whole premise of the family: the chain names a job, and everybody who runs it
    /// honestly commits to the same four values.
    #[test]
    fn two_producers_on_one_anchor_commit_to_the_same_roots() {
        let a = backend();
        let b = backend();
        let anchor = Hash64::from_u64_word(0xA1);
        let (job_a, prompt_a) = a.job_for_anchor(anchor).expect("a job");
        let (job_b, prompt_b) = b.job_for_anchor(anchor).expect("a job");
        assert_eq!(prompt_a, prompt_b, "the prompt is a pure function of the anchor");
        assert_eq!(job_a.context_hash(), job_b.context_hash());

        let out_a = a.execute(&job_a, &prompt_a).expect("it runs");
        let out_b = b.execute(&job_b, &prompt_b).expect("it runs");
        assert_eq!(out_a.trace_root, out_b.trace_root);
        assert_eq!(out_a.output_root, out_b.output_root);
        assert_eq!(out_a.execution_root, out_b.execution_root);
        assert_eq!(out_a.material, out_b.material);
        assert_eq!(out_a.trace_chunk_count, 1);
    }

    /// A different anchor is a different job, a different prompt and different roots. A backend
    /// that ignored the anchor would pass the test above and fail this one.
    #[test]
    fn a_different_anchor_is_a_different_execution() {
        let a = backend();
        let one = Hash64::from_u64_word(1);
        let two = Hash64::from_u64_word(2);
        let (j1, p1) = a.job_for_anchor(one).expect("a job");
        let (j2, p2) = a.job_for_anchor(two).expect("a job");
        assert_ne!(p1, p2);
        let r1 = a.execute(&j1, &p1).expect("runs");
        let r2 = a.execute(&j2, &p2).expect("runs");
        assert_ne!(r1.trace_root, r2.trace_root);
        assert_ne!(r1.execution_root, r2.execution_root);
    }

    /// The material round-trips exactly, and bytes that are not this format are `Unverifiable`
    /// rather than an accusation.
    #[test]
    fn the_material_round_trips_and_refuses_what_it_cannot_read() {
        let a = backend();
        let anchor = Hash64::from_u64_word(7);
        let (job, prompt) = a.job_for_anchor(anchor).expect("a job");
        let out = a.execute(&job, &prompt).expect("runs");
        let run = qwen36_material_decode_v1(&out.material).expect("its own material decodes");
        assert_eq!(run.logits_rows.len(), prompt.len() + job.exact_decode_tokens as usize);
        assert_eq!(run.generated.len(), job.exact_decode_tokens as usize);
        let (trace_root, _, execution_root, _) = qwen36_roots_v1(&job, a.shape_id(), &run).expect("its own run carries its own rows");
        assert_eq!(trace_root, out.trace_root);
        assert_eq!(execution_root, out.execution_root);

        assert!(qwen36_material_decode_v1(&[]).is_none());
        assert!(qwen36_material_decode_v1(&out.material[..out.material.len() - 1]).is_none());
        let mut extra = out.material.clone();
        extra.push(0);
        assert!(qwen36_material_decode_v1(&extra).is_none(), "trailing bytes are not this format");
        assert_eq!(
            a.verify_material(
                b"not material",
                PalwClaimRootsV1 { execution_root: out.execution_root, trace_root: out.trace_root, anchor: Hash64::from_u64_word(7) }
            ),
            PalwMaterialVerdictV1::Unverifiable
        );
    }

    /// **A bondless gossiped message must not be able to kill every panel seat** (ADR-0068 launch
    /// audit; found by the Gate 0 sweep).
    ///
    /// `verify_material` is the one verb a stranger reaches: material is gossiped and no bond
    /// stands behind a message. The decoder reads the row count and the token count independently,
    /// so `rows = 0, generated = 1` parses — and `qwen36_roots_v1` then read the missing row as
    /// `unwrap_or_default()`, an empty `Vec<i32>`. An empty row has no lanes, so the tiled trace
    /// root tiles it into ZERO leaves, and `step_merkle_root_v1` refuses a zero-leaf tree under an
    /// `.expect`. `configure_panic` turns that into `process::exit(1)` with no `catch_unwind`
    /// anywhere on the path.
    ///
    /// One message, every seat that read it dead — and a claim with no seats never licenses and
    /// never reaches a court, so the attack disarms the court for free. Both variants are pinned
    /// (the missing row and the empty run), because they hit two different `.expect`s.
    #[test]
    fn a_material_that_kept_no_rows_is_unverifiable_rather_than_fatal() {
        let a = backend();
        let anchor = Hash64::from_u64_word(0x5EA7);
        let (job, prompt) = a.job_for_anchor(anchor).expect("a job");
        let honest = a.execute(&job, &prompt).expect("runs");
        let claim = PalwClaimRootsV1 { execution_root: honest.execution_root, trace_root: honest.trace_root, anchor };

        // `rows = 0, generated = 1` — the row a token was selected from is simply absent.
        let mut no_rows = Vec::new();
        no_rows.extend_from_slice(&0u64.to_le_bytes());
        no_rows.extend_from_slice(&1u64.to_le_bytes());
        no_rows.extend_from_slice(&7u32.to_le_bytes());
        assert!(qwen36_material_decode_v1(&no_rows).is_some(), "the premise: these bytes really do decode");
        assert_eq!(a.verify_material(&no_rows, claim), PalwMaterialVerdictV1::Unverifiable);

        // `rows = 0, generated = 0` — nothing was selected at all, which empties the row set the
        // trace root is taken over.
        let mut empty = Vec::new();
        empty.extend_from_slice(&0u64.to_le_bytes());
        empty.extend_from_slice(&0u64.to_le_bytes());
        assert!(qwen36_material_decode_v1(&empty).is_some(), "the premise: these bytes really do decode");
        assert_eq!(a.verify_material(&empty, claim), PalwMaterialVerdictV1::Unverifiable);

        // A row that is present and EMPTY is the same lie told a third way: the material says the
        // token came from a row with no lanes.
        let mut empty_row = Vec::new();
        empty_row.extend_from_slice(&1u64.to_le_bytes());
        empty_row.extend_from_slice(&0u64.to_le_bytes());
        empty_row.extend_from_slice(&1u64.to_le_bytes());
        empty_row.extend_from_slice(&7u32.to_le_bytes());
        assert!(qwen36_material_decode_v1(&empty_row).is_some());
        assert_eq!(a.verify_material(&empty_row, claim), PalwMaterialVerdictV1::Unverifiable);

        // And the honest material still verifies — a refusal that also refused the real thing
        // would be a seat that certifies nothing.
        assert_eq!(a.verify_material(&honest.material, claim), PalwMaterialVerdictV1::Matches);
    }

    /// **The court is unavailable and says so.** A backend that returned something plausible from
    /// `bisect_prefix_state` would let a ladder converge on a rung nothing can open, which reads as
    /// a party that lost rather than as a class that has no court.
    #[test]
    fn the_court_methods_are_honestly_unavailable() {
        let a = backend();
        assert_eq!(a.bisect_prefix_state(b"anything", 0), None);
        assert!(a.refutation_for_index(b"anything", 0).is_err());
        // …and it SAYS so, so a node can report it at startup instead of an operator discovering
        // it from a court that never resolves (audit3 H4).
        assert!(!a.supports_court(), "a family with no rung move must not claim it can take a turn");
        assert!(a.execute_with_injected_fault(&a.job_for_anchor(Hash64::default()).expect("a job").0, &[1], 0).is_err());
        // And the family is the one whose disputes CAN end in a conviction, because the arithmetic
        // is deterministic-integer — what is missing is the step space, not the premise.
    }

    /// **ADR-0067: the chain-registered constructor commits exactly what the ledger path
    /// commits.** Same artifact, same graph — one backend built the ledger-compiled way (handed
    /// the class it serves, as a resolved ledger row hands it), one FROM the registered
    /// declaration — one anchor: the derived jobs, all four roots and the material must be
    /// equal, or a chain-armed node and a ledger node would answer one claim differently. Both
    /// authorities CAPTURE: a capture-capable class commits the step binding's own root
    /// whichever door built the backend, which is what makes the roots comparable at all.
    #[test]
    fn the_registered_declaration_backend_commits_the_compiled_backends_roots() {
        let artifact = std::sync::Arc::new(crate::qwen36::test_fixture(4, 8));
        let geometry = crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4);
        let profile = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(geometry).expect("the fixture geometry projects");
        let network = b"misaka-palw-test".to_vec();
        let compiled =
            Qwen36Backend::with_class_profile(artifact.clone(), "Qwen3.6-fixture", (4, 2), profile.clone(), network.clone());
        let planned = Qwen36Backend::from_registered_profile(artifact, network, profile, (4, 2)).expect("the graph is servable");
        assert_eq!(planned.model_id(), "PALW-QWEN36/chain-registered");
        assert!(compiled.supports_court() && planned.supports_court(), "both authorities hold the capture");

        let anchor = Hash64::from_u64_word(0xC0FFEE);
        let (job_a, prompt_a) = compiled.job_for_anchor(anchor).expect("a job");
        let (job_b, prompt_b) = planned.job_for_anchor(anchor).expect("a job");
        assert_eq!(prompt_a, prompt_b);
        assert_eq!(job_a.context_hash(), job_b.context_hash(), "one job, whichever authority derived it");

        let a = compiled.execute(&job_a, &prompt_a).expect("the compiled path runs");
        let b = planned.execute(&job_b, &prompt_b).expect("the planned path runs");
        assert_eq!(a.trace_root, b.trace_root);
        assert_eq!(a.output_root, b.output_root);
        assert_eq!(a.execution_root, b.execution_root);
        assert_eq!(a.trace_manifest_root, b.trace_manifest_root);
        assert_eq!(a.material, b.material, "one retained material, bit for bit");

        // And the planned backend judges the compiled one's material as its own — the seat's
        // verb, which is where a chain-armed node meets a table producer's claim.
        assert_eq!(
            planned
                .verify_material(&a.material, PalwClaimRootsV1 { execution_root: a.execution_root, trace_root: a.trace_root, anchor }),
            PalwMaterialVerdictV1::Matches
        );
    }

    /// A declaration this artifact contradicts is refused at CONSTRUCTION with the field named —
    /// the admission decision, never a mid-forward surprise.
    #[test]
    fn a_contradicted_declaration_is_refused_at_construction() {
        let artifact = std::sync::Arc::new(crate::qwen36::test_fixture(4, 8));
        let mut geometry = crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4);
        geometry.hidden_dim *= 2;
        let profile = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(geometry).expect("the widened geometry projects");
        let err =
            Qwen36Backend::from_registered_profile(artifact, b"misaka-palw-test".to_vec(), profile, (4, 2)).map(drop).unwrap_err();
        assert!(err.contains("cannot serve the registered graph"), "the refusal names the boundary: {err}");
    }

    /// A job that runs past the rotary table is refused at derivation, not discovered mid-decode.
    #[test]
    fn a_job_longer_than_the_table_is_refused() {
        let artifact = crate::qwen36::test_fixture(2, 8);
        let context = artifact.shape.max_position as u32;
        let a = Qwen36Backend::new(
            std::sync::Arc::new(artifact),
            "Qwen3.6-fixture",
            (context, 1),
            Hash64::from_u64_word(0x36),
            b"misaka-palw-test".to_vec(),
        );
        assert!(a.job_for_anchor(Hash64::default()).is_err());
    }

    /// **The hybrid step space, end to end: every leaf of a captured attempt adjudicates, and a
    /// tampered one convicts** — the theorem this family's court capability rests on, and the
    /// same sweep the dense tier already passes
    /// (`every_a16_leaf_adjudicates_and_a_tampered_one_convicts`).
    ///
    /// One captured run of the corrected (`graph-v2`) class at the RC-canonical job shape; then,
    /// for EVERY leaf of its step space, the backend's own prover assembles the refutation, the
    /// backend's own inventory answers for the operands through real Merkle openings against its
    /// root, and the court finds no fault. A single leaf that reads `Unadjudicable` is a step
    /// nobody can police — the coverage-clean-but-unprosecutable shape ADR-0049 exists to refuse
    /// — so the sweep is exhaustive rather than sampled, and it is what held the court's arms to
    /// the registration: the routed experts' resolution, the router row's committed layout, the
    /// decay's two calibration rows and the sink convention's family scope were all its
    /// convictions. The same prover then convicts a run with one tampered lane at kernels of
    /// different shapes — the embedding gather, a GatedDeltaNet recurrence head, a routed-expert
    /// projection tile, and a decode-call leaf (whose adjudication rides the tiled pin).
    #[test]
    fn every_qwen36_leaf_adjudicates_and_a_tampered_one_convicts() {
        use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, check_execution_step_refutation_v1};

        let artifact = std::sync::Arc::new(crate::qwen36::test_fixture(4, 8));
        let geometry = crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4);
        let profile = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(geometry).expect("the fixture geometry projects");
        let backend = Qwen36Backend::from_registered_profile(
            artifact.clone(),
            b"misaka-palw-test".to_vec(),
            profile.clone(),
            kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL,
        )
        .expect("the corrected graph is servable");
        assert!(backend.supports_court(), "the corrected class takes a court's turn");

        let anchor = Hash64::from_u64_word(0x0936_C017);
        let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let outcome = backend.execute(&job, &prompt).expect("the corrected class runs the attempt lane");
        let (binding, _tiles, _logits, _generated, _chunks) =
            crate::produce::base0_material_decode_v1(&outcome.material).expect("the captured material decodes");
        assert_eq!(outcome.execution_root, binding.committed_execution_root, "the claim commits the binding's own root");

        // The seat's half, against this very claim.
        let claim = PalwClaimRootsV1 { execution_root: outcome.execution_root, trace_root: outcome.trace_root, anchor };
        assert_eq!(backend.verify_material(&outcome.material, claim), PalwMaterialVerdictV1::Matches);

        // One proven oracle over the whole inventory — the production path a close takes.
        let inventory = crate::inventory::qwen36_inventory_v1(&artifact, &profile).expect("the corrected class yields an inventory");
        let openings: Vec<_> = (0..inventory.operands().len())
            .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
            .collect();
        let oracle = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
            .expect("every inventory row proves against its own root");

        // The sweep: every leaf of the step space clears the honest capture.
        for index in 0..binding.step_leaf_count {
            let refutation = backend
                .refutation_for_index(&outcome.material, index)
                .unwrap_or_else(|e| panic!("leaf {index} must open from an honest capture: {e}"));
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

        // The other direction: one tampered lane convicts, at kernels of different shapes. The
        // coordinates are FOUND, not hardcoded, so a table edit cannot silently retarget the
        // tampering at some other kernel.
        let coord_of = |index: u64| {
            kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &job, index).expect("a main step coordinate")
        };
        let leaf_where = |want: &dyn Fn(&kaspa_consensus_core::palw_step::PalwStepNodeV1, u32) -> bool| -> u64 {
            (0..binding.step_leaf_count)
                .find(|i| {
                    let coord = coord_of(*i);
                    profile.resolve_node_slot(coord.node_slot).is_some_and(|(n, _)| want(n, coord.call_index))
                })
                .expect("the step space holds the wanted kernel")
        };
        let embed_leaf = 0u64;
        let gdn_leaf = leaf_where(&|n, _| {
            n.kernel_semantics_id
                == kaspa_consensus_core::palw_step::kernel_semantics_id_v1(kaspa_consensus_core::palw_step_refute::KDESC_Q36_GDN_STEP)
        });
        let routed_leaf = leaf_where(&|n, _| n.weight_name.ends_with(".routed"));
        let decode_leaf = leaf_where(&|_, call| call > 0);
        assert!(coord_of(decode_leaf).call_index > 0, "the decode representative rides the tiled pin");
        for index in [embed_leaf, gdn_leaf, routed_leaf, decode_leaf, binding.step_leaf_count - 1] {
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

    /// The all-attention (qwen3moe) flavor through the same sweep: no recurrence, no gate, no
    /// shared expert — the stripped v2 graph — every leaf adjudicates and a routed tile still
    /// convicts. Cheaper than the hybrid sweep (three layers, one call class fewer of kernels),
    /// and what says the Coder-shaped members are prosecutable, not just the hybrid.
    #[test]
    fn every_qwen3moe_leaf_adjudicates_and_a_tampered_routed_tile_convicts() {
        use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, check_execution_step_refutation_v1};

        let artifact = std::sync::Arc::new(crate::qwen36::qwen3moe_dev_fixture(3, 8));
        let geometry = crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 1);
        let profile = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(geometry).expect("the stripped geometry projects");
        let backend = Qwen36Backend::from_registered_profile(artifact.clone(), b"misaka-palw-test".to_vec(), profile.clone(), (4, 2))
            .expect("the stripped graph is servable");
        assert!(backend.supports_court());

        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x30E5_C017)).expect("a job");
        let outcome = backend.execute(&job, &prompt).expect("the stripped class runs the attempt lane");
        let (binding, _, _, _, _) = crate::produce::base0_material_decode_v1(&outcome.material).expect("decodes");

        let inventory = crate::inventory::qwen36_inventory_v1(&artifact, &profile).expect("an inventory");
        let openings: Vec<_> = (0..inventory.operands().len())
            .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
            .collect();
        let oracle =
            kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root()).expect("proves");

        for index in 0..binding.step_leaf_count {
            let refutation = backend.refutation_for_index(&outcome.material, index).unwrap_or_else(|e| panic!("leaf {index}: {e}"));
            let got = check_execution_step_refutation_v1(&refutation, &oracle);
            assert!(
                matches!(got, Err(PalwStepRefuteError::NoFaultFound)),
                "leaf {index} (coord {:?}): got {got:?}",
                refutation.output_preimage.coord
            );
        }

        let routed_leaf = (0..binding.step_leaf_count)
            .find(|i| {
                kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &job, *i)
                    .and_then(|c| profile.resolve_node_slot(c.node_slot).map(|(n, _)| n.weight_name.ends_with(".routed")))
                    .unwrap_or(false)
            })
            .expect("the stripped graph still routes");
        let lying = backend.execute_with_injected_fault(&job, &prompt, routed_leaf).expect("commits");
        let refutation = backend.refutation_for_index(&lying.material, routed_leaf).expect("opens");
        let openings = backend.operand_openings_for(&refutation).expect("the prover opens what the court resolves");
        let proven =
            kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root()).expect("proves");
        assert!(check_execution_step_refutation_v1(&refutation, &proven).is_ok(), "a tampered routed tile must convict");
    }

    /// The shape id separates two graphs. Two classes that shared one would be two classes the
    /// chain could not tell apart.
    #[test]
    fn the_shape_id_separates_two_graphs() {
        let four = crate::qwen36::test_fixture(4, 8);
        let eight = crate::qwen36::test_fixture(8, 8);
        assert_ne!(qwen36_shape_id_v1(&four.shape), qwen36_shape_id_v1(&eight.shape));
        let mut altered = four.shape.clone();
        altered.layer_types[0] = Qwen36LayerKind::FullAttention;
        assert_ne!(qwen36_shape_id_v1(&four.shape), qwen36_shape_id_v1(&altered));
        let mut wider = four.shape.clone();
        wider.n_experts += 1;
        assert_ne!(qwen36_shape_id_v1(&four.shape), qwen36_shape_id_v1(&wider));
    }

    /// **The fold and the dense capture are ONE commitment on the hybrid tier too** (ADR-0082
    /// Decision 7). The dense tier's `the_folded_capture_commits_the_dense_captures_roots`, on
    /// this family's own engine, cache and checkpoint profile — because "the roots do not move"
    /// is a claim about each family's capture loop, and this family has its own.
    #[test]
    fn the_folded_capture_commits_the_dense_captures_roots() {
        let artifact = std::sync::Arc::new(crate::qwen36::test_fixture(4, 8));
        let geometry = crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4);
        let profile = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(geometry).expect("the fixture geometry projects");
        let backend = Qwen36Backend::with_class_profile(
            artifact.clone(),
            "Qwen3.6-fixture",
            (4, 2),
            profile.clone(),
            b"misaka-palw-test".to_vec(),
        );
        let plan = Qwen36Engine::new(&artifact).plan_from_profile(&profile).expect("the fixture graph compiles");
        let (ctx, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0082_F01D)).expect("the anchor implies a job");
        let cap = kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;

        let dense = qwen36_execute_for_attempt_streaming_capped_v1(&artifact, &profile, &plan, &ctx, &prompt, cap, &mut |_| {})
            .expect("the dense sink runs the job");
        let folded = qwen36_execute_free_prompt_streaming_v1(&artifact, &profile, &plan, &ctx, &prompt, cap, &mut |_| {})
            .expect("the folded sink runs the job");

        assert_eq!(dense.binding, folded.binding, "the two sinks commit the same binding, field for field");
        assert_eq!(dense.execution_root, folded.execution_root);
        assert_eq!(dense.trace_root, folded.trace_root);
        assert_eq!(dense.output_root, folded.output_root);
        assert_eq!(dense.trace_manifest_root, folded.trace_manifest_root);
        assert_eq!(dense.generated_token_ids, folded.generated_token_ids, "one execution, one answer");

        let tree = folded.step_tree.as_ref().expect("a folded run keeps its tree");
        assert!(folded.tiles.tiles.is_empty() && folded.tiles.leaves.is_empty(), "the fold keeps no tiles");
        assert_eq!(tree.leaf_count(), dense.tiles.leaves.len() as u64);
        assert_eq!(tree.root().expect("the tree is its own shape"), dense.binding.step_merkle_root);
        assert_eq!(tree.retain_level(), crate::fp_capture::palw_base0_sparse_retain_level_v1(cap));

        // And the seat's first question is answered off the retained tree rather than off tiles.
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let bytes = crate::produce::base0_fp_material_encode_v2(&folded, &ids).expect("the fold retains");
        let claim = PalwClaimRootsV1 { execution_root: folded.execution_root, trace_root: folded.trace_root, anchor: ctx.job_id };
        assert_eq!(backend.verify_material(&bytes, claim), PalwMaterialVerdictV1::Matches);
        let dense_bytes = crate::produce::base0_material_encode_v1(&dense).expect("the dense sink retains").len();
        eprintln!(
            "Decision 7 on the Qwen3.6 fixture: {} leaves, retention {} bytes folded against {dense_bytes} dense ({:.1}x)",
            tree.leaf_count(),
            bytes.len(),
            dense_bytes as f64 / bytes.len().max(1) as f64
        );
    }
}
