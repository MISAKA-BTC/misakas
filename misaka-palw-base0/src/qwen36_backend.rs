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
use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, output_commitment_v2};
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
    /// **The NETWORK's ladder — the materialization cap** (ADR-0121 Decision 1): the ruleset's
    /// `PalwCourtParamsV2::max_step_leaf_count`, defaulting to the leg's own constant (which is what
    /// every shipped preset froze). The dense tier already carried this; the hybrid one read the
    /// constant at five separate sites. It bounds every job-sized site ([`Self::materialize_cap`]);
    /// the class's own ladder — the regime's `2^40` for a held class — is [`Self::step_ladder_cap`],
    /// derived from the profile, as the dense tier's is.
    network_ladder: u64,
    /// The network's prompt-commitment form (ADR-0081 Decision 3); see `Base0Backend::prompt_ids_form`.
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    /// This instance's seat state and walk; nothing outside the instance reaches it.
    seat_memo: crate::fp_recompute::Base0FpSeatMemoV1,
    /// **Which attempt rule this instance runs** (ADR-0152 v3.1, addendum §4-bis.1): the family's own
    /// (`Legacy`) or the chain's `CoreV1` — the job an anchor implies and the rendered rule of every
    /// output root. Set by the node from its params (`set_attempt_rules_v1`); `Legacy` by default.
    attempt_rules: kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1,
    /// **ADR-0152 §4-ter N4's selector**, as the dense tier's
    /// (`Qwen25A16Backend::with_held_answerability_v1`): past `palw_offence_attribution` no held class
    /// this family serves answers a dissection — it has no windowed builder (the review's F4).
    held_answerability: bool,
}

impl Qwen36Backend {
    /// The prompt a binding's job ran: carried for a free prompt (refused unless the job commits to
    /// it), re-derived from the anchor for an attempt.
    fn attn_prompt_ids_v1(
        &self,
        binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
        carried_prompt: Option<&[u32]>,
    ) -> Result<Vec<u32>, String> {
        Ok(match carried_prompt {
            Some(ids) => {
                if !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
                    self.prompt_ids_form,
                    ids,
                    &binding.job_context.prompt_token_ids_hash,
                ) {
                    return Err("the carried prompt is not the one this capture's job context commits to".to_string());
                }
                ids.to_vec()
            }
            None => qwen36_prompt_for_anchor(
                binding.job_context.job_id,
                self.artifact.shape.vocab,
                binding.job_context.declared_prefill_tokens,
            )
            .iter()
            .map(|t| *t as u32)
            .collect(),
        })
    }

    /// An honest, dense re-execution of the binding's job through the registered plan.
    fn attn_rerun_v1(
        &self,
        plan: &crate::qwen36_plan::Qwen36ProfilePlanV1,
        binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
        prompt_ids: &[u32],
    ) -> Result<crate::produce::Base0ExecutionV1, String> {
        let prompt: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
        // A DENSE re-execution, bounded by the materialization cap (DoS audit 2026-09-24, #4).
        qwen36_execute_for_attempt_capped_v1(
            &self.artifact,
            &binding.shape_profile,
            plan,
            &binding.job_context,
            &prompt,
            self.materialize_cap(),
        )
    }

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
            network_ladder: LEG_MAX_LEAVES,
            prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            seat_memo: Default::default(),
            attempt_rules: Default::default(),
            held_answerability: false,
        }
    }

    /// **The ladder top from the ruleset**, for a caller that holds `PalwCourtParamsV2`. Passing
    /// `max_step_leaf_count` is the only correct argument; the constructors pass the leg's default,
    /// which is what every shipped preset froze.
    pub fn with_step_ladder_cap(mut self, max_step_leaf_count: u64) -> Self {
        self.network_ladder = max_step_leaf_count;
        self
    }

    /// **ADR-0152 §4-ter N4: whether the chain this backend serves is past `palw_offence_attribution`**
    /// (the dense tier's `with_held_answerability_v1`).
    pub fn with_held_answerability_v1(mut self, armed: bool) -> Self {
        self.held_answerability = armed;
        self
    }

    /// The network's prompt-commitment form (ADR-0081 Decision 3); see `Base0Backend::prompt_ids_form`.
    /// Stored as THIS CLASS's form (ADR-0118 Decision 3): Merkle for a held class whatever the
    /// network's, so the job this backend derives and the one a seat derives cannot disagree.
    pub fn with_prompt_ids_form(mut self, form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1) -> Self {
        self.prompt_ids_form = match &self.profile {
            Some(profile) => kaspa_consensus_core::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(form, profile),
            None => form,
        };
        self
    }

    pub fn prompt_ids_form(&self) -> kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1 {
        self.prompt_ids_form
    }

    /// Builder form of [`PalwExecutionBackendV1::set_attempt_rules_v1`].
    pub fn with_attempt_rules(mut self, rules: kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1) -> Self {
        self.attempt_rules = rules;
        self
    }

    /// **The output root this instance commits for `ids` under `ctx`**: `CoreV1`'s one rendered rule,
    /// or — `Legacy` — the family's own, which the executor already computed (`legacy`).
    fn committed_output_root_v1(&self, ctx: &PalwJobContextV2, ids: &[u32], legacy: Hash64) -> Hash64 {
        match self.attempt_rules {
            kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1::CoreV1 => {
                kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(ctx, ids)
            }
            kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1::Legacy => legacy,
        }
    }

    /// **The class's ladder** (ADR-0119 Decision 1; ADR-0121 Decision 1): the network's for every
    /// class but one under the held regime, whose ladder is the regime's `2^40` — what this backend
    /// prices a job at, refuses a capture above, and walks every Merkle path and streamed replay
    /// against. A backend with no profile (the legacy composite) prices at the network's.
    pub fn step_ladder_cap(&self) -> u64 {
        match &self.profile {
            Some(profile) => kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(self.network_ladder, profile),
            None => self.network_ladder,
        }
    }

    /// **The materialization cap** (ADR-0121 Decision 1; DoS audit 2026-09-24, #4): the most leaves
    /// a whole-capture path builds a vector of — the HOST's number, no longer the network's ladder.
    /// See the dense tier's `materialize_cap` for the t12 figures that moved it.
    pub fn materialize_cap(&self) -> u64 {
        crate::fp_interval::base0_materialize_cap_v1(self.network_ladder, self.profile.as_ref())
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
            network_ladder: LEG_MAX_LEAVES,
            prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            seat_memo: Default::default(),
            attempt_rules: Default::default(),
            held_answerability: false,
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
            network_ladder: LEG_MAX_LEAVES,
            prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            seat_memo: Default::default(),
            attempt_rules: Default::default(),
            held_answerability: false,
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

/// **Take one checkpoint of the hybrid's state, at whatever the class's map names** (ADR-0082
/// Decision 4, amended; audit B, C-3).
///
/// The shipped hybrid registers the checkpoint sentinel and never reaches here. A graph-v5 hybrid
/// registers the composed map, so its leg has `prefill + decode_calls` leaves and a producer that
/// took none sealed at zero against a canonical count that is never zero —
/// `CheckpointCaptureIncomplete`, and the class produced nothing at all.
///
/// Both halves come from the functions the SEAT uses (`qwen36_attn_chunk_bytes_v1`,
/// `qwen36_recurrence_state_v1`, `base0_gdn_state_chunks_v2`) and the order is
/// `base0_composed_state_chunks_v1`'s, which is `hybrid_state_chunk_entry_v3` itself. The
/// recurrence is built ONLY when this leaf carries it: under the per-position cadence the
/// attention tiles ride every position and the recurrence rides its derived spacing, and asking
/// the geometry is how this function avoids having an opinion about that.
fn qwen36_push_checkpoint_v1(
    checkpoints: &mut crate::legs::Base0CheckpointCaptureV1,
    shape: &crate::qwen36::Qwen36ShapeV1,
    cache: &Qwen36Cache,
) -> Result<(), String> {
    let geometry = checkpoints.next_capture_geometry_v1().map_err(|e| format!("{e:?}"))?;
    let needs_recurrence = match &geometry {
        crate::legs::Base0CaptureGeometryV1::Hybrid(g) => g.gdn_chunk_count() > 0,
        crate::legs::Base0CaptureGeometryV1::Flat(_) => false,
    };
    let gdn_chunks = if needs_recurrence {
        let (layers, states) = crate::fp_recompute::qwen36_recurrence_state_v1(shape, cache);
        let gdn_geometry = crate::fp_capture::base0_gdn_state_geometry_v2(
            &layers,
            shape.linear_v_heads as u32,
            shape.linear_head_dim as u32,
            shape.linear_head_dim as u32,
            shape.conv_kernel as u32,
        )
        .map_err(|e| e.to_string())?;
        crate::fp_capture::base0_gdn_state_chunks_v2(&gdn_geometry, &states).map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };
    checkpoints
        .push_composed_v1(|entry| crate::fp_recompute::qwen36_attn_chunk_bytes_v1(cache, entry), &gdn_chunks)
        .map_err(|e| format!("{e:?}"))
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
/// **The attempt lane's run under the sink the CLASS chooses** — the fold for a held hybrid row
/// (its dense capture at the 63 + 2 canonical job is ~10 GiB of tiles, the term that killed ibm's
/// producer on 2026-09-23), the dense tiles otherwise. Same loop, same roots either way.
pub fn qwen36_execute_for_attempt_with_sink_v1(
    artifact: &Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    plan: &crate::qwen36_plan::Qwen36ProfilePlanV1,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
    kind: crate::legs::Base0CaptureKindV1,
) -> Result<crate::produce::Base0ExecutionV1, String> {
    qwen36_execute_streaming_v1(artifact, profile, plan, ctx, prompt, max_step_leaf_count, kind, &mut |_| {})
}

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
    let mut checkpoints = crate::legs::Base0CheckpointCaptureV1::new(ctx, profile, &checkpoint_profile);
    let mut cache = Qwen36Cache::new(&artifact.shape);

    // **The memory brackets** (`memory_phase`), as the dense tier prints them: the cache's, the
    // sink's and the leg's own bytes beside the process's, at each phase. The hybrid producer that
    // died on ibm printed nothing between its start and `dmesg`; this is what says.
    let started = std::time::Instant::now();
    let anon_at_start = crate::memory_phase::process_anon_bytes_v1();
    let bracket = |at: &str, positions: usize, cache: &Qwen36Cache, capture: &crate::legs::Base0CaptureSinkV1, leg: &crate::legs::Base0CheckpointCaptureV1| {
        crate::memory_phase::execution_phase_v1(|| {
            let (filled, leaves) = capture.progress();
            let kind = match capture.kind() {
                crate::legs::Base0CaptureKindV1::DenseTiles => "DenseTiles",
                crate::legs::Base0CaptureKindV1::Fold => "Fold",
            };
            crate::memory_phase::bracket_line_v1(&crate::memory_phase::PalwBracketFactsV1 {
                at,
                positions,
                prefill,
                elapsed_ms: started.elapsed().as_millis() as u64,
                kv_bytes: cache.resident_bytes_v1(),
                storage: kaspa_consensus_core::palw_resource_profile_v1::PalwRuntimeProfileV1::Q36KvI32.name(),
                capture_bytes: capture.retained_bytes_v1(),
                capture_kind: kind,
                filled_leaves: filled,
                leaves,
                leg_bytes: leg.retained_bytes_v1(),
                anon_at_start,
            })
        });
    };
    bracket("cache constructed", 0, &cache, &capture, &checkpoints);

    let mut logits_rows: Vec<Vec<i32>> = Vec::with_capacity(decode_tokens);
    let mut generated: Vec<u32> = Vec::with_capacity(decode_tokens);

    // Call 0 — prefill. Post rows exist only at its LAST position; earlier rows predict tokens
    // the prompt already contains, and the step space has no coordinate for them.
    //
    // **In one pass over the weights when the class takes no checkpoint inside it** (ADR-0117
    // Decision 2): every prompt position through a layer before the next layer is read, which is
    // the same rows in the same capture order (`forward_prefill_planned`), with each layer's
    // tensors and its positions' experts read once for the prompt instead of once a position. A
    // class whose cadence wants a checkpoint after a prefill position needs the cache as it stands
    // after EACH position — which a layer-major pass never holds — and keeps the stepped pass.
    let one_pass = (0..prefill).all(|position| !checkpoints.wants_checkpoint_after_v1(0, position as u32));
    let mut last_logits = Vec::new();
    if one_pass {
        let (logits, traces) =
            engine.forward_prefill_planned(plan, &mut cache, &prompt[..prefill], 0).map_err(|e| format!("the prefill: {e}"))?;
        for (position, trace) in traces.iter().enumerate() {
            capture
                .push_call(profile, ctx, 0, position as u32, &qwen36_captured_rows_v1(profile, trace))
                .map_err(|e| format!("{e:?}"))?;
        }
        last_logits = logits;
    } else {
        for (position, token) in prompt.iter().take(prefill).enumerate() {
            let (logits, trace) =
                engine.forward_token_planned(plan, &mut cache, *token, position).map_err(|e| format!("prefill at {position}: {e}"))?;
            let mut rows = qwen36_captured_rows_v1(profile, &trace);
            if position + 1 != prefill {
                rows.retain(|r| r.table != kaspa_consensus_core::palw_step::PalwStepTableV1::Post);
            }
            capture.push_call(profile, ctx, 0, position as u32, &rows).map_err(|e| format!("{e:?}"))?;
            // **A checkpoint after a PREFILL position, when the class's cadence says so** (ADR-0082
            // Decision 4, amended). The sentinel-mapped hybrid wants none of these and this is
            // `false` at every position; a class that registered the composed map wants one after
            // every position, and before this the hybrid producer took NO checkpoint at any
            // coordinate and then sealed at a count that is `prefill + decode_calls`.
            if checkpoints.wants_checkpoint_after_v1(0, position as u32) {
                qwen36_push_checkpoint_v1(&mut checkpoints, &artifact.shape, &cache)
                    .map_err(|e| format!("the prefill checkpoint at position {position}: {e}"))?;
            }
            last_logits = logits;
        }
    }
    bracket("prefill done", prefill, &cache, &capture, &checkpoints);
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
        // The same predicate the prefill arm asks, so the two cannot drift into two cadences.
        if checkpoints.wants_checkpoint_after_v1(call as u32, 0) {
            qwen36_push_checkpoint_v1(&mut checkpoints, &artifact.shape, &cache)
                .map_err(|e| format!("the checkpoint after decode call {call}: {e}"))?;
        }
    }

    // The count the CLASS's cadence says this job has (ADR-0082 Decision 4, amended). The shipped
    // hybrid registers the checkpoint sentinel and commits none, and `palw_checkpoint_count_v1`
    // returns exactly what `decode_calls / interval` returned FOR THAT MAP — the sentinel's. It is
    // NOT `decode_calls / interval` in general: a class that registers the composed map is on the
    // per-position cadence and its canonical count is `prefill + decode_calls`, which is what the
    // two push sites above now file (audit B, C-3 and L-1 item 3).
    bracket("decode done", prefill + decode_tokens.saturating_sub(1), &cache, &capture, &checkpoints);
    let checkpoints = checkpoints.finish_canonical_v1().map_err(|e| format!("{e:?}"))?;
    let captured = capture.finish(max_step_leaf_count).map_err(|e| format!("{e:?}"))?;
    crate::memory_phase::execution_phase_v1(|| {
        crate::memory_phase::sealed_line_v1(captured.step_leaf_count, cache.resident_bytes_v1(), started.elapsed().as_millis() as u64)
    });

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
    drop(cache);
    crate::memory_phase::execution_phase_v1(|| {
        crate::memory_phase::returning_line_v1(
            tiles.tiles.len(),
            step_tree.as_ref().map(|t| t.retained_len()),
            checkpoints.leaves.len(),
            checkpoints.chunks.len(),
            started.elapsed().as_millis() as u64,
        )
    });

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
    /// ADR-0086 Decision 2, the executor's side: the anchor state a fold interval resumes from,
    /// recomputed with this family's kernels and memoized as a seat's is.
    fn fold_anchor_state_v1(
        &self,
        material: &crate::produce::Base0FpMaterialV2,
        prompt_token_ids: &[u32],
        covered: u32,
    ) -> Option<crate::fp_recompute::Base0FpSeatStateV1> {
        let plan = self.plan.as_ref()?;
        let mut kernels = crate::fp_recompute::Qwen36RecomputeKernelsV1::new(&self.artifact, plan);
        crate::fp_recompute::base0_fp_seat_state_memoized_v1(
            &self.seat_memo,
            &material.binding.shape_profile,
            &material.binding.job_context,
            prompt_token_ids,
            &material.generated_token_ids,
            covered,
            &mut kernels,
            self.prompt_ids_form,
        )
        .ok()
    }

    fn dense_capture_from_fold_v1(
        &self,
        material: &crate::produce::Base0FpMaterialV2,
    ) -> Result<crate::produce::Base0ExecutionV1, String> {
        let (Some(plan), Some(_)) = (&self.plan, &self.profile) else {
            return Err("this backend serves no registered graph, so it cannot re-execute a folded capture".to_string());
        };
        // Refused by name past the materialization cap, and the re-execution capped at it — the
        // dense tier's `dense_capture_from_fold_v1` says why (DoS audit 2026-09-24, #4).
        if let Some(why) = crate::fp_interval::base0_whole_capture_refusal_v1(
            material.binding.step_leaf_count,
            self.materialize_cap(),
            self.step_ladder_cap(),
        ) {
            return Err(why);
        }
        let prompt: Vec<usize> = material.prompt_token_ids.iter().map(|t| *t as usize).collect();
        let run = qwen36_execute_for_attempt_streaming_capped_v1(
            &self.artifact,
            &material.binding.shape_profile,
            plan,
            &material.binding.job_context,
            &prompt,
            self.materialize_cap(),
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
        let prepared = self.prepare_whole_capture_v1(material, carried, Some(index))?;
        self.refutation_from_prepared_v1(&prepared, index)
    }

    /// Everything the whole-capture prover reads that does not depend on the leaf, built once — the
    /// dense tier's `prepare_whole_capture_v1`, for the same reason (DoS audit 2026-09-24, #4: one
    /// re-execution of a fold per sampled claim, not one per draw). `probe` keeps the single-leaf
    /// path's coordinate refusal ahead of the tiles; the prompt is checked ahead of them too.
    fn prepare_whole_capture_v1(
        &self,
        material: &[u8],
        carried: Option<&[u32]>,
        probe: Option<u64>,
    ) -> Result<Q36PreparedCaptureV1, String> {
        let retention =
            crate::produce::base0_material_decode_any_v1(material).map_err(|_| "the capture does not decode".to_string())?;
        let binding = retention.binding().clone();
        if let Some(why) =
            crate::fp_interval::base0_whole_capture_refusal_v1(binding.step_leaf_count, self.materialize_cap(), self.step_ladder_cap())
        {
            return Err(why);
        }
        if let Some(index) = probe {
            kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, index)
                .ok_or_else(|| format!("leaf {index} is not a main step coordinate"))?;
        }

        let prompt_token_ids: Vec<u32> = match carried {
            Some(ids) => {
                if !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
                    self.prompt_ids_form,
                    ids,
                    &binding.job_context.prompt_token_ids_hash,
                ) {
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
                if kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
                    self.prompt_ids_form,
                    &derived_ids,
                    &binding.job_context.prompt_token_ids_hash,
                ) {
                    derived_ids
                } else {
                    Vec::new()
                }
            }
        };

        let step_tiles = self.tiles_from_material_v1(&retention)?;

        let rows_root =
            kaspa_consensus_core::palw_step_refute::tiled_logits_rows_root_v1(&binding.job_context, retention.logits_rows())
                .ok_or_else(|| "the retained rows build no tree".to_string())?;
        let pin = kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1::TiledV1(
            kaspa_consensus_core::palw_step_refute::PalwTiledDecodeTokensV1 {
                rows_root,
                generated_token_ids: retention.generated_token_ids().to_vec(),
            },
        );
        Ok(Q36PreparedCaptureV1 { binding, step_tiles, pin, prompt_token_ids })
    }

    /// One leaf's refutation out of a prepared capture.
    fn refutation_from_prepared_v1(
        &self,
        prepared: &Q36PreparedCaptureV1,
        index: u64,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        let binding = &prepared.binding;
        let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, index)
            .ok_or_else(|| format!("leaf {index} is not a main step coordinate"))?;
        // The prover opens at the ruleset's ladder (ADR-0080 W1b, ADR-0084 U-08) — the A16
        // backend's shape, which this one and the floor's did not share.
        crate::legs::base0_refutation_from_capture_capped_v1(
            &binding.shape_profile,
            &binding.job_context,
            &prepared.step_tiles,
            binding.clone(),
            coord,
            prepared.prompt_token_ids.clone(),
            Some(prepared.pin.clone()),
            None,
            self.network_ladder,
        )
        .map_err(|e| format!("{e:?}"))
    }
}

/// **A whole capture, decoded and laid out once** — the hybrid tier's twin of the dense tier's
/// `A16PreparedCaptureV1`. Holds the dense tiles for its life.
struct Q36PreparedCaptureV1 {
    binding: kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    step_tiles: crate::legs::Base0StepTilesV1,
    pin: kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1,
    prompt_token_ids: Vec<u32>,
}

/// The hybrid tier's free-prompt leaf prover: one prepared capture, many leaves.
struct Q36CaptureLeafProverV1<'a> {
    backend: &'a Qwen36Backend,
    prepared: Q36PreparedCaptureV1,
}

impl kaspa_consensus_core::palw_backend::PalwCaptureLeafProverV1 for Q36CaptureLeafProverV1<'_> {
    fn refutation_for_index(
        &self,
        index: u64,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        self.backend.refutation_from_prepared_v1(&self.prepared, index)
    }
}

/// Restore this family's cache from a checkpoint's composed chunks (ADR-0133 S1 hybrid).
fn qwen36_cache_from_checkpoint_chunks_v1(
    shape: &Qwen36ShapeV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    covered: u32,
    chunks: &[Vec<u8>],
) -> Result<Qwen36Cache, String> {
    use kaspa_consensus_core::palw_state_chunk_map as map;
    let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(profile, ctx, covered);
    let declared = profile.state_chunk_map_id;
    let mut cache = Qwen36Cache::new(shape);
    let apply_attn = |cache: &mut Qwen36Cache, geometry: &map::PalwStateChunkGeometryV1, attn_chunks: &[Vec<u8>]| -> Result<(), String> {
        if attn_chunks.len() as u64 != geometry.chunk_count() {
            return Err(format!(
                "the attention half names {} chunks and the opening carried {}",
                geometry.chunk_count(),
                attn_chunks.len()
            ));
        }
        let count = geometry.positions as usize;
        for (li, kind) in shape.layer_types.iter().enumerate() {
            if *kind == crate::qwen36::Qwen36LayerKind::FullAttention {
                cache.keys[li].resize(count, Vec::new());
                cache.values[li].resize(count, Vec::new());
            }
        }
        for (index, bytes) in attn_chunks.iter().enumerate() {
            let entry = map::integer_kv_state_chunk_entry_v1(geometry, index as u64)
                .ok_or_else(|| format!("the attention map has no entry for chunk {index}"))?;
            let width = entry.row_bytes as usize;
            for p in entry.position_start..entry.position_start + entry.position_count {
                let row = map::integer_kv_state_row_v1(&entry, bytes, p)
                    .ok_or_else(|| format!("attention chunk {index} is not its own length at position {p}"))?;
                let values: Vec<i32> = if row.len() == width {
                    if width % 4 == 0 {
                        row.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
                    } else {
                        row.iter().map(|b| *b as i8 as i32).collect()
                    }
                } else {
                    return Err("the map describes a row this cache does not hold".into());
                };
                let side = match entry.kind {
                    map::PalwStateChunkKindV1::Key => &mut cache.keys,
                    map::PalwStateChunkKindV1::Value => &mut cache.values,
                };
                let layer = side
                    .get_mut(entry.attn_layer as usize)
                    .ok_or_else(|| format!("attention chunk {index} names layer {} this cache does not hold", entry.attn_layer))?;
                let slot = layer
                    .get_mut(p as usize)
                    .ok_or_else(|| format!("attention chunk {index} names position {p} past the restored history"))?;
                *slot = values;
            }
        }
        Ok(())
    };
    let apply_gdn = |cache: &mut Qwen36Cache, gdn_chunks: &[Vec<u8>]| -> Result<(), String> {
        let (layers, _) = crate::fp_recompute::qwen36_recurrence_state_v1(shape, cache);
        let heads = shape.linear_v_heads as u32;
        let dim = shape.linear_head_dim as u32;
        let kernel = shape.conv_kernel as u32;
        let geometry = crate::fp_capture::base0_gdn_state_geometry_v2(&layers, heads, dim, dim, kernel).map_err(|e| e.to_string())?;
        let states = crate::fp_capture::base0_gdn_state_from_chunks_v2(&geometry, gdn_chunks).map_err(|e| e.to_string())?;
        for (i, layer) in layers.iter().enumerate() {
            let li = *layer as usize;
            cache.gdn[li] = states[i].heads.clone();
            cache.conv[li] = states[i].conv.clone();
        }
        Ok(())
    };
    if declared == map::gdn_state_chunk_map_id_v2() || declared == map::gdn_state_chunk_map_id_v1() {
        apply_gdn(&mut cache, chunks)?;
        return Ok(cache);
    }
    if declared == map::hybrid_state_chunk_map_id_v3() || declared == map::hybrid_state_chunk_map_id_v4() {
        let hybrid = map::hybrid_state_geometry_for_covered_v1(profile, positions).map_err(|e| format!("{e:?}"))?;
        if chunks.len() as u64 != hybrid.chunk_count() {
            return Err(format!(
                "the hybrid composition names {} chunks and the opening carried {}",
                hybrid.chunk_count(),
                chunks.len()
            ));
        }
        let attn_n = hybrid.attn.chunk_count() as usize;
        apply_attn(&mut cache, &hybrid.attn, &chunks[..attn_n])?;
        if hybrid.gdn_chunk_count() > 0 {
            apply_gdn(&mut cache, &chunks[attn_n..])?;
        }
        return Ok(cache);
    }
    let geometry = crate::legs::base0_checkpoint_geometry_at_v1(profile, ctx, covered).map_err(|e| format!("{e:?}"))?;
    apply_attn(&mut cache, &geometry, chunks)?;
    Ok(cache)
}

/// **The hybrid tier's kernels, as a seat's interval replay needs them** (ADR-0077 Decision 8).
///
/// Genesis from the prompt; a `Checkpoint` restores the composed cache the class registered
/// (ADR-0133 S1: dense first, then hybrid execute-from-checkpoint).
struct Qwen36IntervalKernels<'a> {
    artifact: &'a Qwen36ArtifactV1,
    plan: &'a crate::qwen36_plan::Qwen36ProfilePlanV1,
}

impl crate::fp_interval::Base0FpIntervalKernelsV1 for Qwen36IntervalKernels<'_> {
    fn replay_interval_into(
        &self,
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        start: &crate::fp_interval::Base0FpIntervalStartV1<'_>,
        window: crate::fp_interval::Base0FpWindowV1,
        step_leaf_count: u64,
        sink: &mut dyn FnMut(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1) -> Result<(), String>,
    ) -> Result<(), String> {
        let engine = Qwen36Engine::new(self.artifact);
        let mut cache = match start {
            crate::fp_interval::Base0FpIntervalStartV1::Genesis { .. } => Qwen36Cache::new(&self.artifact.shape),
            crate::fp_interval::Base0FpIntervalStartV1::Checkpoint { covered_decode_call, chunks, .. } => {
                qwen36_cache_from_checkpoint_chunks_v1(&self.artifact.shape, profile, ctx, *covered_decode_call, chunks)?
            }
        };
        let vocab = self.artifact.shape.vocab;
        let max_position = self.artifact.shape.max_position;
        crate::fp_interval::base0_fp_replay_interval_into_v1(
            profile,
            ctx,
            start,
            window,
            step_leaf_count,
            |token, position| {
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
            },
            sink,
        )
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
            // This probe validates the directory extent, not the weight bytes. In the low-memory
            // mapped fallback `tensor` is intentionally a contiguous streaming read; calling it
            // here would turn a cheap job-boundary check into a full artifact scan before every
            // draw. `tensor_len` performs the same truncation/name validation without touching a
            // page or allocating a tensor-sized buffer.
            self.artifact
                .tensor_len(&name)
                .map_err(|e| format!("this host can no longer read the mapped artifact: tensor {name}: {e}"))?;
        }
        Ok(())
    }

    /// The hybrid tier's interval kernels, with this backend's registered plan.
    fn interval_kernels_v1(
        &self,
    ) -> Result<(&kaspa_consensus_core::palw_step::PalwShapeProfileV3, Qwen36IntervalKernels<'_>), String> {
        let profile = self.profile.as_ref().ok_or_else(|| "this backend serves no registered graph".to_string())?;
        let plan = self.plan.as_ref().ok_or_else(|| "this backend serves no registered plan".to_string())?;
        Ok((profile, Qwen36IntervalKernels { artifact: &self.artifact, plan }))
    }
}

impl PalwExecutionBackendV1 for Qwen36Backend {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn set_attempt_rules_v1(&mut self, rules: kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1) {
        self.attempt_rules = rules;
    }

    fn attempt_rules_v1(&self) -> kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1 {
        self.attempt_rules
    }

    /// ADR-0152 §4-ter N4: the setter form of [`Self::with_held_answerability_v1`] — what a node's
    /// registry applies to every backend it resolves (`Params::palw_held_answerability_v1()`).
    fn set_held_answerability_v1(&mut self, armed: bool) {
        self.held_answerability = armed;
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
        // **CoreV1 (ADR-0152 v3.1 J-5): the chain's job, over this class's registered profile** — as
        // the dense tier's (see there): the formula canonical, the core prompt, no artifact field.
        if self.attempt_rules == kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1::CoreV1 {
            let profile = self.profile.as_ref().ok_or_else(|| "CoreV1 derives a job from the class's registered profile, and this instance holds none".to_string())?;
            let formula = kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_canonical_v1(profile, false);
            if formula != Some(self.canonical_job) {
                return Err(format!(
                    "the class's canonical job {:?} is not CoreV1's formula {formula:?}: the chain would convict it as another job",
                    self.canonical_job
                ));
            }
            return kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_job_for_anchor_v1(
                profile,
                &anchor,
                self.canonical_job,
                self.prompt_ids_form,
            )
            .ok_or_else(|| "the canonical prompt does not commit under the class's form".to_string());
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
            prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
                self.prompt_ids_form,
                &ids,
            )
            .map_err(|e| e.to_string())?,
            declared_prefill_tokens: prefill,
            exact_decode_tokens: decode,
            max_context_tokens: shape.max_position as u32,
        };
        Ok((ctx, prompt))
    }

    fn execute_for_verdict(
        &self,
        job: &PalwJobContextV2,
        prompt: &[usize],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwReplayRootsV1, String> {
        use kaspa_consensus_core::palw_backend::PalwReplayRootsV1;
        let (Some(plan), Some(profile)) = (&self.plan, &self.profile) else {
            let outcome = self.execute(job, prompt)?;
            return Ok(PalwReplayRootsV1 {
                execution_root: outcome.execution_root,
                trace_root: outcome.trace_root,
                work_leaves: None,
                output_root: Some(outcome.output_root),
            });
        };
        // The fold sink: one execution, the dense run's roots, none of its tiles (ADR-0084 D7).
        let run =
            qwen36_execute_free_prompt_streaming_v1(&self.artifact, profile, plan, job, prompt, self.step_ladder_cap(), &mut |_| {})?;
        // SEAT-S2: the output root the producer's run commits, from the ids this replay generated.
        Ok(PalwReplayRootsV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            work_leaves: Some(run.binding.step_leaf_count),
            output_root: Some(self.committed_output_root_v1(&run.binding.job_context, &run.generated_token_ids, run.output_root)),
        })
    }

    /// SEAT-S4: the authenticated `SC02` opening at this class's ladder.
    fn open_segment_checkpoint_v1(&self, capture: &[u8], seat_count: u16, segment_index: u16) -> Result<Vec<u8>, String> {
        crate::segment_opening::base0_open_segment_checkpoint_capped_v2(
            capture,
            seat_count,
            segment_index,
            self.profile.as_ref(),
            self.step_ladder_cap(),
        )
        .map_err(|e| e.to_string())
    }

    /// SEAT-S4: authenticated against `claim` and this seat's `job`; a resume restores the composed
    /// cache the class registered (ADR-0133 S1: hybrid execute-from-checkpoint).
    fn replay_segment_from_checkpoint_v1(
        &self,
        job: &PalwJobContextV2,
        prompt: &[usize],
        opening: &[u8],
        claim: kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentClaimV1,
    ) -> Result<kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentReplayV1, String> {
        let (profile, kernels) = self.interval_kernels_v1()?;
        crate::segment_opening::base0_replay_segment_opening_v2(
            &kernels,
            profile,
            job,
            prompt,
            opening,
            claim,
            self.step_ladder_cap(),
            self.prompt_ids_form,
        )
        .map_err(String::from)
    }

    fn replay_accused_segment_v1(
        &self,
        capture: &[u8],
        seat_count: u16,
        segment_index: u16,
        job: &PalwJobContextV2,
        prompt: &[usize],
    ) -> Result<kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentReplayV1, String> {
        let (profile, kernels) = self.interval_kernels_v1()?;
        crate::segment_opening::base0_replay_capture_segment_v2(
            &kernels,
            profile,
            capture,
            seat_count,
            segment_index,
            Some(job),
            &crate::segment_opening::base0_court_prompt_v1(self, job.job_id, prompt),
            self.step_ladder_cap(),
            self.prompt_ids_form,
        )
        .map_err(String::from)
    }

    fn replay_layer_site_v3(
        &self,
        capture: &[u8],
        site: kaspa_consensus_core::palw_layer_sample_v3::PalwLayerSiteV3,
        seat_count: u16,
    ) -> Result<bool, String> {
        let retention = crate::produce::base0_material_decode_any_v1(capture).map_err(|e| e.to_string())?;
        let binding = retention.binding();
        if site.layer >= binding.shape_profile.layer_count {
            return Err("the sampled layer is not in this class".into());
        }
        let table = match binding.shape_profile.layer_kind(site.layer) {
            kaspa_consensus_core::palw_step::PalwLayerKindV1::Attention => kaspa_consensus_core::palw_step::PalwStepTableV1::Attn,
            kaspa_consensus_core::palw_step::PalwLayerKindV1::GatedDeltaNet => kaspa_consensus_core::palw_step::PalwStepTableV1::Gdn,
        };
        let node_slot = binding
            .shape_profile
            .global_node_slot(table, site.layer, 0)
            .ok_or_else(|| "this layer has no node to sample".to_string())?;
        let prefill = binding.job_context.declared_prefill_tokens;
        let (call_index, position) = if site.position < prefill {
            (0u32, site.position)
        } else {
            (site.position - prefill + 1, 0u32)
        };
        let coord = kaspa_consensus_core::palw_step::PalwStepCoordinateV1 { call_index, node_slot, position, tile_index: 0 };
        let leaf = kaspa_consensus_core::palw_step::canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &coord)
            .ok_or_else(|| "the sampled site is not a main step of this job".to_string())?;
        let seats = seat_count.max(1);
        let k = kaspa_consensus_core::palw_verification_v2::palw_segment_count_v2(seats);
        let index = kaspa_consensus_core::palw_verification_v2::palw_segment_index_of_leaf_v2(binding.step_leaf_count, k, leaf)
            .unwrap_or(0);
        // The segment holding the site, from the capture's own authenticated opening (SEAT-S4).
        let (profile, kernels) = self.interval_kernels_v1()?;
        let replay = crate::segment_opening::base0_replay_capture_segment_v2(
            &kernels,
            profile,
            capture,
            seats,
            index,
            None,
            &crate::segment_opening::base0_court_prompt_v1(self, binding.job_context.job_id, &[]),
            self.step_ladder_cap(),
            self.prompt_ids_form,
        )
        .map_err(String::from)?;
        Ok(replay.matches)
    }

    fn execute(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        // **The captured attempt, where the declaration is the program.** A plan proves this
        // build serves the registered graph node for node, and the planned traced walk is what a
        // capture is placed from — so court capability rides exactly the constructor that proves
        // servability. The ledger-compiled path keeps the legacy composite: an engine whose op
        // order is this build's hardcode cannot commit a step space the COURT's coordinates
        // describe unless the two provably correspond, and the plan is that proof.
        if let (Some(plan), Some(profile)) = (&self.plan, &self.profile) {
            // **A held hybrid row's attempt FOLDS** — the dense tier's rule (`Qwen25A16Backend::execute`)
            // for the same reason: the dense sink keeps every tile of every position, ~10 GiB at the
            // held row's 63 + 2 canonical job, which grew ibm's producer from 5.87 to 14.63 GiB of anon
            // and killed it (2026-09-23 08:07Z). The fold is the sink the free-prompt lane and every
            // seat already use for this class; its roots are the dense sink's by construction, and its
            // material is the v2 codec the court verbs read, with the anchor's prompt aboard.
            let folds = kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1(profile);
            let kind = if folds { crate::legs::Base0CaptureKindV1::Fold } else { crate::legs::Base0CaptureKindV1::DenseTiles };
            let run = qwen36_execute_for_attempt_with_sink_v1(
                &self.artifact,
                profile,
                plan,
                job,
                prompt,
                if folds { self.step_ladder_cap() } else { self.network_ladder },
                kind,
            )?;
            let material = if folds {
                let prompt_ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
                crate::produce::base0_fp_material_encode_v2(&run, &prompt_ids).map_err(|e| e.to_string())?
            } else {
                crate::produce::base0_material_encode_v1(&run).map_err(|e| e.to_string())?
            };
            crate::memory_phase::execution_phase_v1(|| crate::memory_phase::material_line_v1(material.len(), folds));
            return Ok(PalwExecutionOutcomeV1 {
                trace_root: run.trace_root,
                output_root: self.committed_output_root_v1(&run.binding.job_context, &run.generated_token_ids, run.output_root),
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
    /// The producer's figure for a `prefill_tokens` attempt, through the resource profile — the
    /// verb the gate read as "this family derives no resource profile" until it did. The attempt
    /// the producer runs is the CANONICAL job, so its decode count is the class's, not a floor of
    /// one: a figure for fewer decode calls than the attempt makes would under-state it.
    fn attempt_working_set_bytes(&self, prefill_tokens: usize) -> Option<u64> {
        let profile = self.profile.as_ref()?;
        let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(
            profile,
            u32::try_from(prefill_tokens).ok()?,
            self.canonical_job.1.max(1),
        );
        self.resource_profile_v1(Some(&job), kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1::Producer)
            .map(|p| p.working_set_bytes())
    }

    fn runtime_profile_v1(&self) -> Option<kaspa_consensus_core::palw_resource_profile_v1::PalwRuntimeProfileV1> {
        Some(kaspa_consensus_core::palw_resource_profile_v1::PalwRuntimeProfileV1::Q36KvI32)
    }

    /// **What a role needs on this backend** (ADR-0151 follow-up, item 1, the hybrid's half): the
    /// same derivation the dense tier answers with, on this class's graph — its attention rows, its
    /// recurrence state, the capture its lane keeps (a held row's attempt and the verdict replay
    /// fold; a segment replay keeps its hashes), the leg's retention and the run's scratch. The
    /// mapping's residency is the HOLDING's term, priced by the node beside this.
    fn resource_profile_v1(
        &self,
        job: Option<&PalwJobContextV2>,
        role: kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1,
    ) -> Option<kaspa_consensus_core::palw_resource_profile_v1::PalwResourceProfileV1> {
        use kaspa_consensus_core::palw_resource_profile_v1::{
            PalwCaptureRetentionV1, PalwResourceRoleV1, PalwRuntimeLimitsV1, PalwRuntimeProfileV1, palw_attempt_capture_folds_v1,
            palw_profile_max_tile_len_v1,
        };
        let profile = self.profile.as_ref()?;
        let canonical;
        let job = match job {
            Some(job) => job,
            None => {
                canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, self.canonical_job.0, self.canonical_job.1);
                &canonical
            }
        };
        let leaf_count = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, job, self.step_ladder_cap()).ok()?;
        let fold = PalwCaptureRetentionV1::Fold {
            retain_level: crate::fp_capture::palw_base0_sparse_retain_level_for_class_v1(profile, self.step_ladder_cap()),
        };
        let capture = match role {
            PalwResourceRoleV1::Producer if !palw_attempt_capture_folds_v1(profile) => {
                PalwCaptureRetentionV1::DenseTiles { tile_len: palw_profile_max_tile_len_v1(profile) }
            }
            PalwResourceRoleV1::Producer | PalwResourceRoleV1::FullSeat => fold,
            PalwResourceRoleV1::PartialSeat { .. } => PalwCaptureRetentionV1::ReplayHashes,
        };
        // A held hybrid row walks its prefill a position at a time (a checkpoint after every one),
        // so one position's committed trace is what the run holds; a per-call row walks the whole
        // prefill in one pass and holds every position's trace until captured.
        let run_positions = if kaspa_consensus_core::palw_state_chunk_map::palw_map_addresses_history_tiles_v1(profile) {
            1
        } else {
            job.declared_prefill_tokens.max(1)
        };
        kaspa_consensus_core::palw_resource_profile_v1::palw_resource_profile_v1(
            profile,
            job,
            leaf_count,
            PalwRuntimeProfileV1::Q36KvI32,
            role,
            PalwRuntimeLimitsV1 { threads: rayon::current_num_threads().max(1) as u32, prefill_run_positions: run_positions },
            capture,
        )
    }

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
        use kaspa_consensus_core::palw_fp_execution_v3::{
            PalwFpClassFactsV3, PalwFpRunFactsV3, palw_fp_job_context_v3, palw_fp_run_facts_for_executed_v1,
        };

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
        // The pairing is derived from the executed count (ADR-0074 Decision 7); this producer runs
        // its declared ceiling, which is what it passes.
        let shape = palw_fp_run_facts_for_executed_v1(job, job.decode_token_limit);
        let ctx = palw_fp_job_context_v3(job, &class, &shape, &self.network_id).map_err(|e| format!("{e:?}"))?;

        let run = qwen36_execute_free_prompt_streaming_v1(
            &self.artifact,
            profile,
            plan,
            &ctx,
            prompt_tokens,
            self.step_ladder_cap(),
            on_token,
        )?;

        let (checkpoint_leg_root, step_leg_root) = crate::legs::base0_leg_roots_from_binding_v1(&run.binding);
        let prompt_ids: Vec<u32> = prompt_tokens.iter().map(|t| *t as u32).collect();
        let material = crate::produce::base0_fp_material_encode_v2(&run, &prompt_ids).map_err(|e| e.to_string())?;
        // The free-prompt lane's own manifest (palw_freeprompt_v3), not the attempt lane's the run carries.
        let (fp_trace_manifest_root, fp_trace_chunk_count) =
            crate::produce::base0_fp_trace_manifest_v3(&run.binding.job_context, &run.logits_rows)
                .ok_or_else(|| "the run's rows build no retained-trace manifest".to_string())?;
        Ok(kaspa_consensus_core::palw_backend::PalwFpRunV1 {
            outcome: PalwExecutionOutcomeV1 {
                trace_root: run.trace_root,
                output_root: self.committed_output_root_v1(&run.binding.job_context, &run.generated_token_ids, run.output_root),
                execution_root: run.execution_root,
                trace_manifest_root: fp_trace_manifest_root,
                trace_chunk_count: fp_trace_chunk_count,
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
            // **An attempt folds only on a held class** (`palw_attempt_capture_folds_v1`: this
            // build's `execute` writes the dense capture for every other), and a fold's rows are
            // tied to its step tree by no rule a seat can run (SEAT-0's head rule reads dense
            // leaves) — so under an attempt claim of a class whose attempts do not fold, the
            // selecting row would be a free field: bend it, move the token, re-derive the roots,
            // and every seat licensed each bend. Refused, as material no honest producer serves.
            // A held class's fold attempt is the honest producer's own material and stays
            // licensable here; its selecting row is the residual SEAT-R (a full-mask `Valid` only
            // from a replay) and F1c's rule 12 close at `palw_offence_attribution`.
            if claim.attempt_draw.is_some() && !self.profile.as_ref().is_some_and(kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1) {
                return PalwMaterialVerdictV1::Mismatch;
            }
            // **SEAT-S1: the whole job, here too** — a held class's attempt is a fold, and this
            // branch checked the id alone (`base0_material_job_is_the_claims_v1`).
            if let Err(verdict) =
                crate::produce::base0_material_answers_the_claim_v1(self, &folded.binding, &folded.generated_token_ids, &claim)
            {
                return verdict;
            }
            if folded.binding.shape_profile.shape_profile_id() != self.class_profile_id {
                return PalwMaterialVerdictV1::Unverifiable;
            }
            return match crate::produce::base0_fp_material_matches_claim_v2(
                &folded,
                claim.execution_root,
                claim.trace_root,
                crate::produce::Base0SeatFamilyV1::Qwen36,
            ) {
                Ok(true) => PalwMaterialVerdictV1::Matches,
                Ok(false) => PalwMaterialVerdictV1::Mismatch,
                Err(_) => PalwMaterialVerdictV1::Unverifiable,
            };
        }
        if let Ok(decoded) = crate::produce::base0_material_decode_v1(material) {
            // **The whole job, not only its id** (ADR-0117; SEAT-S1): the fold branch's rule,
            // `base0_material_job_is_the_claims_v1`.
            if let Err(verdict) = crate::produce::base0_material_answers_the_claim_v1(self, &decoded.0, &decoded.3, &claim) {
                return verdict;
            }
            if decoded.0.shape_profile.shape_profile_id() != self.class_profile_id {
                return PalwMaterialVerdictV1::Unverifiable;
            }
            return match crate::produce::base0_material_matches_claim_capped_v1(
                &decoded,
                claim.execution_root,
                claim.trace_root,
                self.network_ladder,
                crate::produce::Base0SeatFamilyV1::Qwen36,
            ) {
                Ok(true) => PalwMaterialVerdictV1::Matches,
                Ok(false) => PalwMaterialVerdictV1::Mismatch,
                Err(_) => PalwMaterialVerdictV1::Unverifiable,
            };
        }
        let Some(run) = qwen36_material_decode_v1(material) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        // **The legacy composite is the ledger-compiled class's alone.** A court-capable backend's
        // `execute` writes the family codec for every attempt, and this decode re-derives the roots
        // from the rows it was sent with no execution behind them — so on this backend every new
        // set of rows was a new root the seat licensed, and SEAT-0's rules never saw them.
        if self.supports_court() {
            return PalwMaterialVerdictV1::Mismatch;
        }
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
        // The legacy composite is recomputed under the job the block asked for, ADR-0117's rule included.
        let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, claim.attempt_draw.unwrap_or(false));
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
            layer_count: binding.shape_profile.layer_count,
        })
    }

    /// **ADR-0062 D3's responder, for the hybrid tier** — the twin of the dense tier's, and for the
    /// same reason (mainnet audit 2026-09-06, C-5). `supports_court()` above is true exactly when
    /// this backend holds the registered graph and its plan; the method that answer promises was
    /// the trait's refusal until this line.
    fn disclose_trace_event(
        &self,
        material: &[u8],
        row: u32,
        tile: u8,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1, String> {
        crate::produce::base0_disclose_trace_event_v1(material, row, tile)
    }

    fn bisect_prefix_state(&self, material: &[u8], index: u64) -> Option<kaspa_hashes::Hash64> {
        let retention = crate::produce::base0_material_decode_any_v1(material).ok()?;
        let binding = retention.binding().clone();
        // The count arrived over gossip inside a borsh blob; bounding it BEFORE the allocation is
        // the lesson the seat check already wrote down. At the materialization cap: the prefix is a
        // whole leaf vector (DoS audit 2026-09-24, #4).
        if binding.step_leaf_count == 0 || binding.step_leaf_count > self.materialize_cap() {
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

    /// Prepared once, opened per draw (DoS audit 2026-09-24, #4) — see the dense tier's.
    fn free_prompt_leaf_prover_v1<'a>(
        &'a self,
        material: &'a [u8],
        prompt_token_ids: &'a [u32],
    ) -> Result<Box<dyn kaspa_consensus_core::palw_backend::PalwCaptureLeafProverV1 + 'a>, String> {
        let prepared = self.prepare_whole_capture_v1(material, Some(prompt_token_ids), None)?;
        Ok(Box::new(Q36CaptureLeafProverV1 { backend: self, prepared }))
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
        crate::fp_interval::Base0FpIntervalGeometryV1::from_binding_capped_v1(retention.binding(), interval, self.step_ladder_cap())
            .ok()
            .map(|g| g.interval_count)
    }

    fn fp_interval_count_for(&self, prompt_tokens: u32, decode_tokens_executed: u32) -> Option<u32> {
        crate::fp_interval::base0_fp_interval_count_for_class_v1(
            self.profile.as_ref()?,
            prompt_tokens,
            decode_tokens_executed,
            self.checkpoint_interval()?,
        )
    }

    fn open_fp_interval(&self, capture: &[u8], index: u32, prompt_token_ids: &[u32]) -> Result<Vec<u8>, String> {
        let interval = self
            .checkpoint_interval()
            .ok_or_else(|| "this backend serves no registered graph, so it opens no interval".to_string())?;
        // Two retention forms, one opening, the class's map deciding whether the history travels —
        // ADR-0082 Decisions 7 and 9, exactly as the dense tier composes them.
        let chunked =
            match crate::produce::base0_material_decode_any_v1(capture).map_err(|_| "the capture does not decode".to_string())? {
                crate::produce::Base0RetentionV1::Folded(material) => {
                    let plan = self.plan.as_ref().ok_or_else(|| "this backend serves no registered graph".to_string())?;
                    crate::fp_interval::base0_open_fp_interval_sparse_anchored_capped_v1(
                        &material,
                        index,
                        prompt_token_ids,
                        interval,
                        self.step_ladder_cap(),
                        &Qwen36IntervalKernels { artifact: &self.artifact, plan },
                        &|covered| self.fold_anchor_state_v1(&material, prompt_token_ids, covered),
                        self.prompt_ids_form,
                    )
                    .map_err(|e| e.to_string())?
                }
                crate::produce::Base0RetentionV1::Dense(material) => crate::fp_interval::base0_open_fp_interval_capped_v1(
                    &material,
                    index,
                    prompt_token_ids,
                    interval,
                    self.network_ladder,
                    self.prompt_ids_form,
                )
                .map_err(|e| e.to_string())?,
            };
        if self.profile.as_ref().is_some_and(crate::fp_interval::base0_fp_class_requires_flat_openings_v1) {
            return crate::fp_interval::base0_strip_fp_interval_history_v1(&chunked).map_err(|e| e.to_string());
        }
        Ok(chunked)
    }

    fn fp_held_route_v1(&self, window_receipt_daa: u64) -> Option<kaspa_consensus_core::palw_held_context_v1::PalwHeldSeatRouteV1> {
        crate::fp_interval::base0_fp_held_route_for_v1(self.profile.as_ref()?, window_receipt_daa)
    }

    fn open_fp_resume_v1(&self, capture: &[u8], index: u32, prompt_token_ids: &[u32]) -> Result<Vec<u8>, String> {
        let interval = self.checkpoint_interval().ok_or_else(|| "this backend serves no registered graph".to_string())?;
        match crate::produce::base0_material_decode_any_v1(capture).map_err(|_| "the capture does not decode".to_string())? {
            crate::produce::Base0RetentionV1::Folded(material) => crate::fp_interval::base0_open_fp_resume_v1(
                &material,
                index,
                prompt_token_ids,
                interval,
                self.step_ladder_cap(),
                None,
                &|covered| self.fold_anchor_state_v1(&material, prompt_token_ids, covered),
                self.prompt_ids_form,
            )
            .map_err(|e| e.to_string()),
            crate::produce::Base0RetentionV1::Dense(_) => {
                Err("a dense retention is the attempt lane's; it resumes nothing".to_string())
            }
        }
    }

    fn fp_accept_resume_v1(
        &self,
        resume: &[u8],
        context: &PalwJobContextV2,
        prompt_token_ids: &[u32],
        covered: u32,
    ) -> Result<Hash64, String> {
        let interval = self.checkpoint_interval().ok_or_else(|| "this backend serves no registered graph".to_string())?;
        crate::fp_interval::base0_fp_accept_resume_v1(
            &self.seat_memo,
            resume,
            context,
            prompt_token_ids,
            covered,
            interval,
            self.step_ladder_cap(),
        )
        .map(|state| state.state_chunks_root)
        .map_err(|e| format!("{e:?}"))
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
        let state = crate::fp_interval::base0_fp_interval_opening_seat_state_capped_v1(
            &self.seat_memo,
            opening,
            prompt_token_ids,
            interval,
            self.step_ladder_cap(),
        );
        crate::fp_interval::base0_verify_fp_interval_opening_with_state_capped_v1(
            opening,
            claim,
            index,
            prompt_token_ids,
            work_leaves,
            interval,
            self.step_ladder_cap(),
            state.as_ref(),
            &Qwen36IntervalKernels { artifact: &self.artifact, plan },
            self.prompt_ids_form,
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
        covered: u32,
    ) -> Result<Hash64, String> {
        // The free-prompt spelling of the context-keyed seam (ADR-0084 Decision 4).
        let ctx = self.fp_job_context_v1(job).ok_or_else(|| "this job builds no context for this class".to_string())?;
        self.checkpoint_root_for_context_v1(&ctx, prompt_token_ids, output_token_ids, covered)
    }

    /// **The largest `covered` this class's leg carries, in the class's own cadence unit**
    /// (audit B, C-2). Per decode call: `decode_calls`. Per position (what the graph-v5 hybrid
    /// row's composed map registers): `prefill + decode_calls`, every row the cache ever holds.
    /// A backend with no registered graph has no cadence to read and answers the seam's default.
    fn fp_checkpoint_covered_bound_v1(&self, job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3) -> u32 {
        match self.fp_job_context_v1(job) {
            Some(ctx) => self.checkpoint_covered_bound_for_context_v1(&ctx),
            None => job.decode_token_limit.saturating_sub(1),
        }
    }

    /// **The committed answer's ids, read by the family that wrote the capture** (ADR-0084
    /// Decision 6): the fold and the dense tuple alike, through `base0_material_decode_any_v1`.
    /// `None` when the bytes are not this family's, and never an empty answer.
    fn open_fp_interval_with_close(
        &self,
        capture: &[u8],
        index: u32,
        prompt_token_ids: &[u32],
        disputed: &[u64],
    ) -> Result<Vec<u8>, String> {
        use crate::fp_interval::{Base0FpIntervalOpeningV4, base0_fp_close_annex_v1, base0_fp_interval_opening_with_close_v1};
        let opening = self.open_fp_interval(capture, index, prompt_token_ids)?;
        if disputed.is_empty() {
            return Ok(opening);
        }
        let v4 = Base0FpIntervalOpeningV4::decode_v1(&opening).map_err(|e| format!("the served opening is not V4: {e:?}"))?;
        let range = (v4.range.first_leaf_index, v4.range.first_leaf_index.saturating_add(v4.range.leaf_count));
        if !disputed.iter().any(|l| *l >= range.0 && *l < range.1) {
            return Ok(opening);
        }
        let (Some(interval), Some(plan)) = (self.checkpoint_interval(), self.plan.as_ref()) else {
            return Err("this class commits no checkpoint leg".to_string());
        };
        let retention =
            crate::produce::base0_material_decode_any_v1(capture).map_err(|_| "the capture does not decode".to_string())?;
        // The disputed leaves' tiles (a fold replays the interval and keeps only those, ADR-0121; a
        // dense retention holds them), plus the seed row — the anchor call's logits tiles — which the
        // opening itself carries.
        let mut by_index: std::collections::HashMap<u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1> = match &retention {
            crate::produce::Base0RetentionV1::Dense((_, tiles, ..)) => tiles.iter().cloned().collect(),
            crate::produce::Base0RetentionV1::Folded(material) => crate::fp_interval::base0_fp_disputed_tiles_from_fold_v1(
                material,
                index,
                prompt_token_ids,
                interval,
                self.step_ladder_cap(),
                &Qwen36IntervalKernels { artifact: &self.artifact, plan },
                &|covered| self.fold_anchor_state_v1(material, prompt_token_ids, covered),
                self.prompt_ids_form,
                disputed,
            )
            .map_err(|e| format!("{e:?}"))?
            .into_iter()
            .collect(),
        };
        for (k, tile) in v4.seed_row_tiles.iter().enumerate() {
            by_index.entry(range.0 + k as u64).or_insert_with(|| tile.clone());
        }
        let annex = base0_fp_close_annex_v1(
            retention.binding(),
            retention.logits_rows(),
            retention.checkpoint_chunks(),
            &|leaf| by_index.get(&leaf).cloned(),
            disputed,
            range,
        )?;
        base0_fp_interval_opening_with_close_v1(&opening, annex).map_err(|e| format!("{e:?}"))
    }

    fn open_fp_block_leaves(
        &self,
        capture: &[u8],
        interval_index: u32,
        block_index: u64,
        prompt_token_ids: &[u32],
    ) -> Result<Vec<u8>, String> {
        let opening = self.open_fp_interval(capture, interval_index, prompt_token_ids)?;
        let (Some(interval), Some(plan)) = (self.checkpoint_interval(), self.plan.as_ref()) else {
            return Err("this class commits no checkpoint leg".to_string());
        };
        match crate::produce::base0_material_decode_any_v1(capture).map_err(|_| "the capture does not decode".to_string())? {
            crate::produce::Base0RetentionV1::Folded(material) => crate::fp_interval::base0_fp_block_leaves_from_fold_capped_v1(
                &material,
                &opening,
                block_index,
                prompt_token_ids,
                interval,
                self.step_ladder_cap(),
                &Qwen36IntervalKernels { artifact: &self.artifact, plan },
                &|covered| self.fold_anchor_state_v1(&material, prompt_token_ids, covered),
            ),
            crate::produce::Base0RetentionV1::Dense((_, tiles, ..)) => {
                crate::fp_interval::base0_fp_block_leaves_from_tiles_v1(&opening, &tiles, block_index)
            }
        }
        .map_err(|e| format!("{e:?}"))
    }

    fn held_state_chunk_answer_v1(
        &self,
        capture: &[u8],
        prompt_token_ids: &[u32],
        checkpoint: u32,
        chunk: u32,
    ) -> Result<
        (
            kaspa_consensus_core::palw_attn_court_v1::PalwAttnCheckpointAnchorV1,
            kaspa_consensus_core::palw_attn_court_v1::PalwAttnChunkOpeningV1,
        ),
        String,
    > {
        let retention =
            crate::produce::base0_material_decode_any_v1(capture).map_err(|_| "the capture does not decode".to_string())?;
        crate::fp_interval::base0_fp_held_state_chunk_answer_v1(&retention, checkpoint, chunk, &|covered| match &retention {
            crate::produce::Base0RetentionV1::Folded(material) => self.fold_anchor_state_v1(material, prompt_token_ids, covered),
            crate::produce::Base0RetentionV1::Dense(_) => None,
        })
        .map_err(|e| e.to_string())
    }

    fn held_step_range_answer_v1(
        &self,
        capture: &[u8],
        prompt_token_ids: &[u32],
        first: u64,
        count: u32,
    ) -> Result<kaspa_consensus_core::palw_step_leg::PalwStepRangeOpeningV1, String> {
        let (Some(interval), Some(plan)) = (self.checkpoint_interval(), self.plan.as_ref()) else {
            return Err("this class commits no checkpoint leg".to_string());
        };
        let retention =
            crate::produce::base0_material_decode_any_v1(capture).map_err(|_| "the capture does not decode".to_string())?;
        // A fold replays the range's blocks (the class's ladder); a dense retention hashes every
        // tile it kept into one vector (the materialization cap, ADR-0121 Decision 1).
        let cap = match &retention {
            crate::produce::Base0RetentionV1::Folded(_) => self.step_ladder_cap(),
            crate::produce::Base0RetentionV1::Dense(_) => self.network_ladder,
        };
        crate::fp_interval::base0_fp_held_step_range_answer_v1(
            &retention,
            first,
            count,
            prompt_token_ids,
            interval,
            cap,
            &Qwen36IntervalKernels { artifact: &self.artifact, plan },
            &|covered| match &retention {
                crate::produce::Base0RetentionV1::Folded(material) => self.fold_anchor_state_v1(material, prompt_token_ids, covered),
                crate::produce::Base0RetentionV1::Dense(_) => None,
            },
            self.prompt_ids_form,
        )
        .map_err(|e| e.to_string())
    }

    fn fp_name_the_leaf_v1(
        &self,
        opening: &[u8],
        block_leaves: &[u8],
        claim: PalwClaimRootsV1,
        index: u32,
        prompt_token_ids: &[u32],
        generated_token_ids: &[u32],
        work_leaves: u64,
    ) -> Result<Option<u64>, String> {
        let (Some(interval), Some(plan)) = (self.checkpoint_interval(), self.plan.as_ref()) else {
            return Err("this class commits no checkpoint leg".to_string());
        };
        if let Ok(v4) = crate::fp_interval::Base0FpIntervalOpeningV4::decode_v1(opening)
            && let Some(anchor) = v4.anchor.as_ref()
        {
            let _ = self.checkpoint_root_for_context_v1(
                &v4.binding.job_context,
                prompt_token_ids,
                generated_token_ids,
                anchor.leaf.covered_decode_call,
            );
        }
        crate::fp_interval::base0_fp_name_the_leaf_capped_v1(
            opening,
            block_leaves,
            claim,
            index,
            prompt_token_ids,
            work_leaves,
            interval,
            self.step_ladder_cap(),
            &|bytes| {
                crate::fp_interval::base0_fp_interval_opening_seat_state_capped_v1(
                    &self.seat_memo,
                    bytes,
                    prompt_token_ids,
                    interval,
                    self.step_ladder_cap(),
                )
            },
            &Qwen36IntervalKernels { artifact: &self.artifact, plan },
            self.prompt_ids_form,
        )
    }

    /// **ADR-0121 §7: a served edge names the leaf** — the straddling blocks, checked by the range's
    /// root walk rather than a digest; the same warm-up and replay as a served block.
    fn fp_name_the_edge_leaf_v1(
        &self,
        opening: &[u8],
        served_edges: &[Vec<u8>],
        claim: PalwClaimRootsV1,
        index: u32,
        prompt_token_ids: &[u32],
        generated_token_ids: &[u32],
        work_leaves: u64,
    ) -> Result<Option<u64>, String> {
        let (Some(interval), Some(plan)) = (self.checkpoint_interval(), self.plan.as_ref()) else {
            return Err("this class commits no checkpoint leg".to_string());
        };
        if let Ok(v4) = crate::fp_interval::Base0FpIntervalOpeningV4::decode_v1(opening)
            && let Some(anchor) = v4.anchor.as_ref()
        {
            let _ = self.checkpoint_root_for_context_v1(
                &v4.binding.job_context,
                prompt_token_ids,
                generated_token_ids,
                anchor.leaf.covered_decode_call,
            );
        }
        crate::fp_interval::base0_fp_name_the_edge_leaf_capped_v1(
            opening,
            served_edges,
            claim,
            index,
            prompt_token_ids,
            work_leaves,
            interval,
            self.step_ladder_cap(),
            &|bytes| {
                crate::fp_interval::base0_fp_interval_opening_seat_state_capped_v1(
                    &self.seat_memo,
                    bytes,
                    prompt_token_ids,
                    interval,
                    self.step_ladder_cap(),
                )
            },
            &Qwen36IntervalKernels { artifact: &self.artifact, plan },
            self.prompt_ids_form,
        )
    }

    fn fp_interval_of_leaf_v1(&self, context: &PalwJobContextV2, leaf: u64) -> Option<u32> {
        let interval = self.checkpoint_interval()?;
        crate::fp_interval::base0_fp_interval_of_leaf_v1(self.profile.as_ref()?, context, interval, leaf)
    }

    /// **A fold answers a leaf from its tree** (ADR-0121) — as the dense tier's; a dense retention
    /// keeps ADR-0085's annex path.
    fn fp_leaf_refutation_v1(
        &self,
        capture: &[u8],
        prompt_token_ids: &[u32],
        claim: PalwClaimRootsV1,
        work_leaves: u64,
        leaf: u64,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        match (crate::produce::base0_material_decode_any_v1(capture), self.checkpoint_interval(), self.plan.as_ref()) {
            (Ok(crate::produce::Base0RetentionV1::Folded(material)), Some(interval), Some(plan)) => {
                crate::fp_interval::base0_fp_fold_is_the_claims_v1(&material, claim, work_leaves)?;
                crate::fp_interval::base0_fp_leaf_refutation_from_fold_v1(
                    &material,
                    leaf,
                    prompt_token_ids,
                    interval,
                    self.step_ladder_cap(),
                    &Qwen36IntervalKernels { artifact: &self.artifact, plan },
                    &|covered| self.fold_anchor_state_v1(&material, prompt_token_ids, covered),
                    self.prompt_ids_form,
                )
                .map_err(|e| format!("{e:?}"))
            }
            _ => kaspa_consensus_core::palw_backend::palw_fp_leaf_refutation_by_annex_v1(
                self,
                capture,
                prompt_token_ids,
                claim,
                work_leaves,
                leaf,
            ),
        }
    }

    fn refutation_from_served_intervals(
        &self,
        held: &[(u32, Vec<u8>)],
        claim: PalwClaimRootsV1,
        prompt_token_ids: &[u32],
        generated_token_ids: &[u32],
        work_leaves: u64,
        leaf: u64,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        let (Some(interval), Some(plan)) = (self.checkpoint_interval(), self.plan.as_ref()) else {
            return Err("this class commits no checkpoint leg".to_string());
        };
        // The replay of a served interval resumes from the state this node recomputed for the
        // interval's named anchor (ADR-0086 Decision 2) — warm that memo the way the seat's own
        // row check would have, for every held interval, before assembling.
        for (_, bytes) in held {
            if let Ok(v4) = crate::fp_interval::Base0FpIntervalOpeningV4::decode_v1(bytes)
                && let Some(anchor) = v4.anchor.as_ref()
            {
                let _ = self.checkpoint_root_for_context_v1(
                    &v4.binding.job_context,
                    prompt_token_ids,
                    generated_token_ids,
                    anchor.leaf.covered_decode_call,
                );
            }
        }
        crate::fp_interval::base0_refutation_from_served_intervals_capped_v1(
            held,
            claim,
            prompt_token_ids,
            generated_token_ids,
            work_leaves,
            leaf,
            interval,
            self.step_ladder_cap(),
            &|bytes| {
                crate::fp_interval::base0_fp_interval_opening_seat_state_capped_v1(
                    &self.seat_memo,
                    bytes,
                    prompt_token_ids,
                    interval,
                    self.step_ladder_cap(),
                )
            },
            &Qwen36IntervalKernels { artifact: &self.artifact, plan },
            self.prompt_ids_form,
        )
    }

    fn fp_forget_seat_state_v1(&self) {
        crate::fp_recompute::base0_fp_seat_state_forget_v1(&self.seat_memo)
    }

    fn fp_committed_output_ids(&self, capture: &[u8]) -> Option<Vec<u32>> {
        let retention = crate::produce::base0_material_decode_any_v1(capture).ok()?;
        let ids = retention.generated_token_ids().to_vec();
        (!ids.is_empty()).then_some(ids)
    }

    /// **`output_root` from the answer's ids** (ADR-0084 Decision 1; ADR-0078 X6). The context is
    /// the one [`Self::execute_free_prompt`] runs under and [`Self::fp_recompute_checkpoint_root`]
    /// rebuilds — the budget decoded exactly, `ExactBudgetReached` — and the rendered hash is this
    /// family's keyed hash of the ids. An honest claim's ids recompute its committed root; any
    /// other list does not, which is how a seat refuses a forged answer envelope before a forward
    /// pass.
    fn fp_output_root_v1(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        output_token_ids: &[u32],
    ) -> Option<Hash64> {
        let ctx = self.fp_job_context_v1(job)?;
        self.output_root_for_context_v1(&ctx, output_token_ids)
    }

    /// The one context this family runs a free-prompt job under (ADR-0084 Decision 4) — the
    /// same value `execute_free_prompt` builds, at the budget THAT RAN (ADR-0074 Decision 7);
    /// see the A16 backend's for the rationale.
    fn fp_job_context_for_executed_v1(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        decode_tokens_executed: u32,
    ) -> Option<PalwJobContextV2> {
        use kaspa_consensus_core::palw_fp_execution_v3::{
            PalwFpClassFactsV3, palw_fp_job_context_v3, palw_fp_run_facts_for_executed_v1,
        };
        let class = PalwFpClassFactsV3 {
            model_profile_id: self.shape_id,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: self.shape_id,
            shape_profile_id: self.class_profile_id,
            cu_ruleset_id: Hash64::default(),
        };
        let shape = palw_fp_run_facts_for_executed_v1(job, decode_tokens_executed);
        palw_fp_job_context_v3(job, &class, &shape, &self.network_id).ok()
    }

    /// This class's price for a context (ADR-0074 Decision 5). `None` for a backend holding no
    /// registered graph — it can price nothing, and saying so is not an accusation.
    fn fp_context_work_leaves_v1(&self, context: &PalwJobContextV2) -> Option<u64> {
        let profile = self.profile.as_ref()?;
        kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, context, self.step_ladder_cap()).ok()
    }

    fn fp_opening_job_context_v1(&self, opening: &[u8]) -> Option<PalwJobContextV2> {
        crate::fp_interval::base0_fp_interval_opening_job_context_v1(opening)
    }

    /// ADR-0082 Decision 9 keyed on the context (ADR-0084 Decision 4): the prefix on this seat's
    /// own kernels, and the tiled root of the state it reaches, for either lane's context.
    fn checkpoint_root_for_context_v1(
        &self,
        context: &PalwJobContextV2,
        prompt_token_ids: &[u32],
        output_token_ids: &[u32],
        covered: u32,
    ) -> Result<Hash64, String> {
        self.artifact_read_probe_v1()?;
        let (Some(profile), Some(plan)) = (self.profile.as_ref(), self.plan.as_ref()) else {
            return Err("this backend serves no registered graph, so it recomputes no state".to_string());
        };
        let mut kernels = crate::fp_recompute::Qwen36RecomputeKernelsV1::new(&self.artifact, plan);
        crate::fp_recompute::base0_fp_seat_state_memoized_v1(
            &self.seat_memo,
            profile,
            context,
            prompt_token_ids,
            output_token_ids,
            covered,
            &mut kernels,
            self.prompt_ids_form,
        )
        .map(|state| state.state_chunks_root)
        .map_err(|e| e.to_string())
    }

    /// The largest `covered` this class's leg carries for `context`, in the class's own cadence
    /// unit (audit B, C-2); a backend with no registered graph answers the seam's default.
    fn checkpoint_covered_bound_for_context_v1(&self, context: &PalwJobContextV2) -> u32 {
        use kaspa_consensus_core::palw_context_ladder::{PalwCheckpointCadenceV1, palw_checkpoint_cadence_v1};
        let decode_calls = context.exact_decode_tokens.saturating_sub(1);
        match self.profile.as_ref().map(palw_checkpoint_cadence_v1) {
            Some(PalwCheckpointCadenceV1::PerPosition) => context.declared_prefill_tokens.saturating_add(decode_calls),
            _ => decode_calls,
        }
    }

    /// `output_root` from the answer's ids under `context` (ADR-0078 X6; ADR-0084).
    fn output_root_for_context_v1(&self, context: &PalwJobContextV2, output_token_ids: &[u32]) -> Option<Hash64> {
        let legacy = output_commitment_v2(&context.context_hash(), output_token_ids, &rendered_output_hash_v1(output_token_ids));
        Some(self.committed_output_root_v1(context, output_token_ids, legacy))
    }

    /// **ADR-0093 as built: the fused site's evidence out of this capture** — the hybrid tier's twin
    /// of the dense tier's: the capture's own committed rows, the checkpoint leg from a
    /// re-execution through the registered plan, and the anchor's state (the composed map's every
    /// chunk, the recurrence's included — the checkpoint roots them all) recomputed with the seat's
    /// kernels, each refused unless it roots to the capture's own binding.
    fn attn_site_evidence(
        &self,
        material: &[u8],
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
        accused_out_tile: Option<&kaspa_consensus_core::palw_attn_court_v1::PalwAttnRowOpeningV1>,
    ) -> Result<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnSiteEvidenceV1, String> {
        let (Some(plan), Some(registered)) = (&self.plan, &self.profile) else {
            return Err("a backend with no registered graph carries no capture to read a fused site out of".to_string());
        };
        let retention =
            crate::produce::base0_material_decode_any_v1(material).map_err(|_| "the capture does not decode".to_string())?;
        let binding = retention.binding().clone();
        let prompt_ids = self.attn_prompt_ids_v1(&binding, carried_prompt)?;
        let generated = retention.generated_token_ids().to_vec();
        let rerun = self.attn_rerun_v1(plan, &binding, &prompt_ids)?;
        // The rows are the CAPTURE's (the bottom opens what the claim committed); a fold keeps
        // none and is re-executed. When that re-execution is another execution (a forged claim),
        // its rows before the disputed leaf are an honest PREFIX, read with the accused's opened
        // output tile and against the fold's own checkpoint leaves (ADR-0093 Decision 7).
        let (tiles, prefix, checkpoints) = match &retention {
            crate::produce::Base0RetentionV1::Dense(_) => (self.tiles_from_material_v1(&retention)?, false, rerun.checkpoints),
            crate::produce::Base0RetentionV1::Folded(fold) => {
                if rerun.binding.committed_execution_root == binding.committed_execution_root {
                    (rerun.tiles, false, rerun.checkpoints)
                } else if accused_out_tile.is_none() {
                    return Err(
                        "a folded capture keeps no rows, and its re-execution is not the same execution — its bottom needs the \
                         accused's opened output tile (its root claim's)"
                            .to_string(),
                    );
                } else {
                    let leg =
                        crate::legs::base0_checkpoint_leg_of_retention_v1(&binding, &fold.checkpoint_chunks, &fold.checkpoint_leaves)
                            .map_err(|e| format!("the fold's checkpoint leg does not rebuild: {e:?}"))?;
                    (rerun.tiles, true, leg)
                }
            }
        };
        let rows = match (prefix, accused_out_tile) {
            (true, Some(accused_out_tile)) => {
                crate::attn_responder::Base0AttnRowsV1::HonestPrefix { honest: &tiles, accused_out_tile }
            }
            _ => crate::attn_responder::Base0AttnRowsV1::Committed(&tiles),
        };
        let inventory = crate::inventory::qwen36_inventory_v1(&self.artifact, registered).map_err(|e| format!("{e:?}"))?;
        let artifact = &self.artifact;
        let (profile, ctx, generated) = (&binding.shape_profile, &binding.job_context, &generated);
        crate::attn_responder::base0_attn_site_evidence_v1(
            &binding,
            rows,
            crate::attn_responder::Base0AttnAnchorSourceV1::Leg(&checkpoints),
            narrowed,
            inventory.operands(),
            inventory.root(),
            self.step_ladder_cap(),
            &mut |covered| {
                let mut kernels = crate::fp_recompute::Qwen36RecomputeKernelsV1::new(artifact, plan);
                crate::fp_recompute::base0_fp_recompute_state_at_covered_v1(
                    profile,
                    ctx,
                    &prompt_ids,
                    generated,
                    covered,
                    &mut kernels,
                    self.prompt_ids_form,
                )
                .map(|state| state.chunks)
                .map_err(|e| e.to_string())
            },
        )
    }

    /// **ADR-0093 Decisions 7 and 8 on the hybrid tier: the bottom's evidence from the accused's
    /// filing alone** — the dense tier's verb, with this family's engine and recompute kernels.
    fn attn_site_evidence_from_filing(
        &self,
        filing: &kaspa_consensus_core::palw_attn_responder_v1::PalwAttnAccusedFilingV1,
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
    ) -> Result<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnSiteEvidenceV1, String> {
        let (Some(plan), Some(registered)) = (&self.plan, &self.profile) else {
            return Err("a backend with no registered graph carries no capture to read a fused site out of".to_string());
        };
        let binding = &filing.binding;
        let prompt_ids = self.attn_prompt_ids_v1(binding, carried_prompt)?;
        let rerun = self.attn_rerun_v1(plan, binding, &prompt_ids)?;
        let inventory = crate::inventory::qwen36_inventory_v1(&self.artifact, registered).map_err(|e| format!("{e:?}"))?;
        let artifact = &self.artifact;
        let (profile, ctx, generated) = (&binding.shape_profile, &binding.job_context, &rerun.generated_token_ids);
        let anchor_source = match &filing.anchor {
            Some(filed) => crate::attn_responder::Base0AttnAnchorSourceV1::Filed(filed),
            None => crate::attn_responder::Base0AttnAnchorSourceV1::Leg(&rerun.checkpoints),
        };
        crate::attn_responder::base0_attn_site_evidence_v1(
            binding,
            crate::attn_responder::Base0AttnRowsV1::HonestPrefix { honest: &rerun.tiles, accused_out_tile: &filing.out_tile },
            anchor_source,
            narrowed,
            inventory.operands(),
            inventory.root(),
            self.step_ladder_cap(),
            &mut |covered| {
                let mut kernels = crate::fp_recompute::Qwen36RecomputeKernelsV1::new(artifact, plan);
                crate::fp_recompute::base0_fp_recompute_state_at_covered_v1(
                    profile,
                    ctx,
                    &prompt_ids,
                    generated,
                    covered,
                    &mut kernels,
                    self.prompt_ids_form,
                )
                .map(|state| state.chunks)
                .map_err(|e| e.to_string())
            },
        )
        .map_err(|e| crate::attn_responder::base0_attn_name_the_missing_anchor_v1(e, filing))
    }

    /// **ADR-0152 §4-ter N4 (the review's F4):** past `palw_offence_attribution` EVERY held class this
    /// family serves says `false` — it has no windowed builder (`attn_site_evidence_held_v1` is the
    /// trait's `Err`), and its dense builder answers a held job only under the materialization cap.
    /// The consensus predicate agrees: a recurrent held class is unanswerable
    /// (`PalwHeldUnanswerableV1::Recurrent`), so C1 lists it and C5 refuses to register it. Below the
    /// fence, byte for byte what it was.
    fn supports_dissection(&self) -> bool {
        self.plan.is_some()
            && self.profile.as_ref().is_some_and(kaspa_consensus_core::palw_class_admission_v2::palw_fused_sites_are_dissectable_v1)
            && !(self.held_answerability
                && self.profile.as_ref().is_some_and(kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4))
    }

    fn has_fused_site(&self) -> bool {
        self.profile
            .as_ref()
            .is_some_and(|p| p.attn_nodes.iter().any(|n| n.op_kind == kaspa_consensus_core::palw_step::PalwStepOpKindV1::AttnFused))
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
        // Carried the way this network's form carries it (ADR-0081 Decision 3): under the Merkle
        // form the flat check refuses the list before a row is read, and records nothing.
        let _ = kaspa_consensus_core::palw_step_refute::check_execution_step_refutation_carried_capped_v1(
            refutation,
            &recorder,
            self.prompt_ids_form,
            self.step_ladder_cap(),
        );
        recorder.openings().ok_or_else(|| "the inventory could not open a recorded row".to_string())
    }

    fn artifact_inventory_digest(&self) -> Result<kaspa_consensus_core::palw_artifact::PalwArtifactInventoryDigestV1, String> {
        let profile = self.profile.as_ref().ok_or_else(|| "this backend holds no registered graph to root against".to_string())?;
        crate::inventory::qwen36_inventory_digest_v1(&self.artifact, profile).map_err(|e| format!("{e:?}"))
    }

    fn artifact_row_opening(&self, index: u32) -> Result<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1, String> {
        let profile = self.profile.as_ref().ok_or_else(|| "this backend holds no registered graph to open against".to_string())?;
        // The digest roots the path; one pass over the rows copies the bytes of the named leaf alone —
        // a 33 GiB artifact is never held in memory to open one row of it.
        let digest = crate::inventory::qwen36_inventory_digest_v1(&self.artifact, profile).map_err(|e| format!("{e:?}"))?;
        let mut at = 0u32;
        let mut wanted: Option<Vec<u8>> = None;
        crate::inventory::qwen36_visit_inventory_rows_v1(&self.artifact, profile, &mut |_name, _layer, _row_start, bytes| {
            if at == index {
                wanted = Some(bytes.to_vec());
            }
            at = at.saturating_add(1);
            Ok(())
        })
        .map_err(|e| format!("{e:?}"))?;
        let bytes = wanted.ok_or_else(|| format!("leaf {index} is outside an inventory of {}", digest.leaf_count()))?;
        digest.opening_v1(index, bytes).ok_or_else(|| format!("leaf {index} does not open against the digest"))
    }

    fn execute_with_drill_fault(
        &self,
        job: &PalwJobContextV2,
        prompt: &[usize],
        fault: kaspa_consensus_core::palw_backend::PalwDrillFaultV1,
    ) -> Result<PalwExecutionOutcomeV1, String> {
        if let kaspa_consensus_core::palw_backend::PalwDrillFaultV1::StepLeaf(leaf) = fault {
            return self.execute_with_injected_fault(job, prompt, leaf);
        }
        let (Some(plan), Some(profile)) = (&self.plan, &self.profile) else {
            return Err("a backend with no registered graph carries no capture to drill".to_string());
        };
        let (job, prompt) = crate::produce::base0_drill_job_v1(self, self.prompt_ids_form, job, prompt, fault)?;
        let mut run = qwen36_execute_for_attempt_capped_v1(&self.artifact, profile, plan, &job, &prompt, self.network_ladder)?;
        let honest_output = self.committed_output_root_v1(&run.binding.job_context, &run.generated_token_ids, run.output_root);
        run.output_root = honest_output;
        crate::produce::base0_drill_run_v1(&mut run, fault, |ctx, ids| {
            self.output_root_for_context_v1(ctx, ids)
                .unwrap_or_else(|| kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(ctx, ids))
        })?;
        let output_root = run.output_root;
        crate::produce::base0_drill_outcome_v1(&run, output_root, fault)
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
        let mut run = qwen36_execute_for_attempt_capped_v1(&self.artifact, profile, plan, job, prompt, self.network_ladder)?;
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
        let binding = crate::legs::base0_binding_from_capture_with_profile_capped_v1(
            profile,
            job,
            &run.tiles,
            &run.checkpoints,
            &checkpoint_profile,
            run.trace_root,
            crate::produce::base0_activation_leg_root_v1(job),
            self.network_ladder,
        )
        .map_err(|e| format!("{e:?}"))?;
        run.execution_root = binding.committed_execution_root;
        run.binding = binding;
        let material = crate::produce::base0_material_encode_v1(&run).map_err(|e| e.to_string())?;
        Ok(PalwExecutionOutcomeV1 {
            trace_root: run.trace_root,
            output_root: self.committed_output_root_v1(&run.binding.job_context, &run.generated_token_ids, run.output_root),
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
                PalwClaimRootsV1 {
                    execution_root: out.execution_root,
                    trace_root: out.trace_root,
                    anchor: Hash64::from_u64_word(7),
                    attempt_draw: None,
                    output_root: None,
                    job_pin: None,
                }
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
        let claim = PalwClaimRootsV1 {
            execution_root: honest.execution_root,
            trace_root: honest.trace_root,
            anchor,
            attempt_draw: None,
            output_root: None,
            job_pin: None,
        };

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
            planned.verify_material(
                &a.material,
                PalwClaimRootsV1 {
                    execution_root: a.execution_root,
                    trace_root: a.trace_root,
                    anchor,
                    attempt_draw: None,
                    output_root: None,
                    job_pin: None,
                }
            ),
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

    /// **ADR-0117 on the hybrid: the one-forward draw is the canonical prompt in one pass, and
    /// its material answers the block that asked for it.** The prefill-only job
    /// (`palw_attempt_job_v1(canonical, true)`) runs the canonical prompt with no decode call:
    /// one generated token, chosen from the last prefill position's logits — the same token the
    /// canonical job generates first, since both prefills are the same rows — and a trace root
    /// over that one selecting row. Its material matches a claim whose block drew with one forward
    /// and is a `Mismatch` against one that did not; the canonical run is the other way round.
    #[test]
    fn the_one_forward_draw_is_the_canonical_prompt_in_one_pass() {
        use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
        let artifact = std::sync::Arc::new(crate::qwen36::test_fixture(4, 8));
        let geometry = crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4);
        let profile = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(geometry).expect("the fixture geometry projects");
        let backend = Qwen36Backend::from_registered_profile(
            artifact.clone(),
            b"misaka-palw-test".to_vec(),
            profile,
            kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL,
        )
        .expect("servable");
        let anchor = Hash64::from_u64_word(0x0117_0936);
        let (canonical, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let one_forward = palw_attempt_job_v1(canonical.clone(), true);
        let full = backend.execute(&canonical, &prompt).expect("the canonical job runs");
        let short = backend.execute(&one_forward, &prompt).expect("the one-forward job runs");
        let full_ids = backend.fp_committed_output_ids(&full.material).expect("the canonical run's ids");
        let short_ids = backend.fp_committed_output_ids(&short.material).expect("the one-forward run's ids");
        assert_eq!(short_ids.len(), 1, "one generated token");
        assert_eq!(short_ids[0], full_ids[0], "the same prefill chooses the same first token");
        let roots = |o: &PalwExecutionOutcomeV1, draw: bool| PalwClaimRootsV1 {
            execution_root: o.execution_root,
            trace_root: o.trace_root,
            anchor,
            attempt_draw: Some(draw),
            output_root: None,
            job_pin: None,
        };
        assert_eq!(backend.verify_material(&short.material, roots(&short, true)), PalwMaterialVerdictV1::Matches);
        assert_eq!(backend.verify_material(&short.material, roots(&short, false)), PalwMaterialVerdictV1::Mismatch);
        assert_eq!(backend.verify_material(&full.material, roots(&full, false)), PalwMaterialVerdictV1::Matches);
        assert_eq!(backend.verify_material(&full.material, roots(&full, true)), PalwMaterialVerdictV1::Mismatch);
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
        let claim = PalwClaimRootsV1 {
            execution_root: outcome.execution_root,
            trace_root: outcome.trace_root,
            anchor,
            attempt_draw: None,
            output_root: None,
            job_pin: None,
        };
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
        let claim = PalwClaimRootsV1 {
            execution_root: folded.execution_root,
            trace_root: folded.trace_root,
            anchor: ctx.job_id,
            attempt_draw: None,
            output_root: None,
            job_pin: None,
        };
        assert_eq!(backend.verify_material(&bytes, claim), PalwMaterialVerdictV1::Matches);
        let dense_bytes = crate::produce::base0_material_encode_v1(&dense).expect("the dense sink retains").len();
        eprintln!(
            "Decision 7 on the Qwen3.6 fixture: {} leaves, retention {} bytes folded against {dense_bytes} dense ({:.1}x)",
            tree.leaf_count(),
            bytes.len(),
            dense_bytes as f64 / bytes.len().max(1) as f64
        );
    }

    /// **C-3, plan item 4: a graph-v5 hybrid executes a job, and its checkpoints are the ones its
    /// own seat recomputes** (ADR-0082 Decision 4, amended; audit B, C-3 and H-1).
    ///
    /// The v5 hybrid row registers `hybrid_state_chunk_map_id_v3()`, so its cadence is
    /// `PerPosition` and `palw_checkpoint_count_v1` is `prefill + decode_calls` — never zero. The
    /// producer constructed a capture, pushed nothing, and sealed at that count:
    /// `CheckpointCaptureIncomplete`, so the class produced no block and no free-prompt claim at
    /// all. It now has the two push sites the dense producer has.
    ///
    /// The second half is H-1: the chunks the producer commits must be the ones the SEAT
    /// enumerates. The producer used to walk the attention half alone while
    /// `Qwen36RecomputeKernelsV1::state_chunks` walked attention plus recurrence; both now walk
    /// `base0_composed_state_chunks_v1`, and this compares the roots that come out.
    ///
    /// The job is sized below the recurrence's derived spacing
    /// (`palw_anchored_interval_for_profile_v1`, which is `min(16, n_ctx)` = 8 on this fixture), so
    /// every leaf here carries the attention half alone. That is not a dodge: it is the case the
    /// per-position cadence spends almost every position in, and the spacing boundary itself is
    /// pinned by `the_hybrid_composition_serializes_in_the_order_its_map_name_spells`, which
    /// records that this fixture's own mismatched gdn head counts are what stop the recurrence
    /// serializer there.
    #[test]
    fn a_graph_v5_hybrid_executes_and_its_checkpoints_are_its_seats() {
        use kaspa_consensus_core::palw_context_ladder::{
            PalwCheckpointCadenceV1, palw_anchored_interval_for_profile_v1, palw_checkpoint_cadence_v1, palw_checkpoint_count_v1,
            palw_checkpoint_leaf_carries_recurrence_v1,
        };
        use kaspa_consensus_core::palw_state_chunk_map as map;

        let (artifact, profile) = crate::fuzz_qwen36::tiny_class_v5_for_tests();
        assert_eq!(profile.state_chunk_map_id, map::hybrid_state_chunk_map_id_v3(), "a v5 hybrid registers the composition");
        assert_eq!(palw_checkpoint_cadence_v1(&profile), PalwCheckpointCadenceV1::PerPosition);

        let engine = Qwen36Engine::new(&artifact);
        let plan = engine.plan_from_profile(&profile).expect("the fixture's declaration is its program");
        let (ctx, prompt) = crate::produce::base0_rc_job_v1(
            &profile,
            Hash64::from_u64_word(0x0000_82C3),
            artifact.shape.vocab,
            3,
            4,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        );
        let positions_total = ctx.declared_prefill_tokens + ctx.exact_decode_tokens.saturating_sub(1);
        assert!(
            positions_total < palw_anchored_interval_for_profile_v1(&profile),
            "this job must stay below the recurrence's spacing — see the doc comment"
        );

        let run = qwen36_execute_for_attempt_v1(&artifact, &profile, &plan, &ctx, &prompt)
            .expect("a graph-v5 hybrid must execute its own job — this returned CheckpointCaptureIncomplete");
        assert_eq!(
            run.checkpoints.leaves.len() as u32,
            palw_checkpoint_count_v1(&profile, &ctx, profile.n_ctx.max(1)),
            "the leg is the count the class's own cadence says the job has"
        );
        assert_eq!(run.checkpoints.leaves.len() as u32, positions_total);
        assert!(run.checkpoints.chunks.is_empty(), "the per-position cadence folds: zero state retained");

        // **H-1: the producer's chunks are the seat's.** The seat re-runs the job with its own
        // kernels and roots the state it reaches; a producer enumerating only the attention half
        // would disagree here at every leaf that carries the recurrence, and about the COUNT at
        // every leaf.
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        for leaf in &run.checkpoints.leaves {
            let positions = leaf.covered_decode_call; // POSITIONS, on this cadence
            assert!(!palw_checkpoint_leaf_carries_recurrence_v1(&profile, positions) || positions == 0);
            let geometry = map::hybrid_state_geometry_for_covered_v1(&profile, positions).expect("the composition derives");
            assert_eq!(
                leaf.state_chunk_count as u64,
                geometry.chunk_count(),
                "checkpoint {} must commit one chunk per entry the CLASS's map names",
                leaf.checkpoint_index
            );
            let mut kernels = crate::fp_recompute::Qwen36RecomputeKernelsV1::new(&artifact, &plan);
            let state = crate::fp_recompute::base0_fp_recompute_state_at_covered_v1(
                &profile,
                &ctx,
                &ids,
                &run.generated_token_ids,
                positions,
                &mut kernels,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("the seat can stop at any position of a per-position class");
            assert_eq!(
                state.state_chunks_root, leaf.state_chunks_root,
                "checkpoint {} (covering {positions} positions): producer and seat must root the same composition",
                leaf.checkpoint_index
            );
        }
    }

    /// **ADR-0133 S1 hybrid: a segment resumes only from a leaf that carries the recurrence**
    /// (SEAT-S4). The per-position leg commits the attention half at every position and the gdn
    /// state only at the recurrence's spacing (`palw_checkpoint_leaf_carries_recurrence_v1`; 8 here,
    /// the fixture's whole context, so no such leaf precedes the segment): every committed leaf
    /// before the segment authenticates and is refused as a resume point by name — a replay from
    /// one diverged at the first `SsmConv`, which the SC01 test (eight leaves compared) never
    /// reached — and the segment replays from the prompt to a match.
    #[test]
    fn a_hybrid_segment_resumes_only_from_a_leaf_that_carries_the_recurrence() {
        use kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1;
        use kaspa_consensus_core::palw_state_chunk_map as map;
        use kaspa_consensus_core::palw_verification_v2::palw_segment_count_v2;

        let (artifact, profile) = crate::fuzz_qwen36::tiny_class_v5_for_tests();
        assert_eq!(profile.state_chunk_map_id, map::hybrid_state_chunk_map_id_v3());
        let artifact = std::sync::Arc::new(artifact);
        let plan = Qwen36Engine::new(&artifact).plan_from_profile(&profile).expect("the fixture graph compiles");
        let backend = Qwen36Backend::with_class_profile(
            artifact.clone(),
            "Qwen3.6-fixture",
            (3, 4),
            profile.clone(),
            b"misaka-palw-test".to_vec(),
        );
        let (ctx, prompt) = crate::produce::base0_rc_job_v1(
            &profile,
            Hash64::from_u64_word(0x0000_13D3),
            artifact.shape.vocab,
            3,
            4,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        );
        let run = qwen36_execute_for_attempt_v1(&artifact, &profile, &plan, &ctx, &prompt).expect("the hybrid job runs");
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let capture = crate::produce::base0_material_encode_v1(&run).expect("the dense capture encodes");
        let retention = crate::produce::base0_material_decode_any_v1(&capture).expect("decodes");
        let seats = 3u16;
        let segments = palw_segment_count_v2(seats);
        let claim_of = |segment_index: u16| kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentClaimV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            seat_count: seats,
            segment_index,
        };
        // The segment that starts in a decode call, and the step it starts at.
        let segment = (0..segments)
            .find(|&i| {
                let (start, _) =
                    kaspa_consensus_core::palw_verification_v2::palw_segment_leaf_range_v2(run.binding.step_leaf_count, segments, i)
                        .expect("the cut names the segment");
                kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, start).is_some_and(|c| c.call_index > 0)
            })
            .expect("a segment starts past the prefill");
        let (start, _) =
            kaspa_consensus_core::palw_verification_v2::palw_segment_leaf_range_v2(run.binding.step_leaf_count, segments, segment)
                .expect("the cut names the segment");
        let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, start).expect("a main leaf");
        let step =
            crate::fp_interval::Base0FpWindowV1::step_of_coordinate_v1(ctx.declared_prefill_tokens, coord.call_index, coord.position);
        // An opening that resumes from the committed checkpoint covering `covered` positions, with the
        // seat's own recompute of its composed state (the recurrence's chunks where the leaf has them).
        let opening_at = |covered: u32| {
            let mut kernels = crate::fp_recompute::Qwen36RecomputeKernelsV1::new(&artifact, &plan);
            let state = crate::fp_recompute::base0_fp_recompute_state_at_covered_v1(
                &profile,
                &ctx,
                &ids,
                &run.generated_token_ids,
                covered,
                &mut kernels,
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("the seat can recompute the composed checkpoint");
            let mut anchor = crate::fp_interval::base0_checkpoint_operands_v1(&run.binding, &[], &run.checkpoints.leaves, covered)
                .expect("the leg commits the checkpoint");
            anchor.chunks = state.chunks;
            crate::segment_opening::base0_segment_opening_v2(
                &retention,
                seats,
                segment,
                crate::segment_opening::Base0SegmentAnchorV1::Given(anchor),
                Some(&profile),
                backend.step_ladder_cap(),
            )
            .expect("the producer opens the segment at the committed checkpoint")
            .encode_v2()
            .expect("the opening encodes")
        };
        // Every committed per-position leaf before the segment authenticates; only one that carries
        // the recurrence is a point a hybrid resumes from — the leaves between the recurrence's
        // spacing commit the attention half alone, and a replay from one diverged at the first
        // `SsmConv` (the SC01 test compared eight leaves and stopped before it).
        let mut resumed = 0usize;
        for covered in 1..step as u32 {
            let positions = palw_checkpoint_positions_at_v1(&profile, &ctx, covered);
            assert_eq!(positions, covered, "the hybrid's leg counts positions");
            let got = backend.replay_segment_from_checkpoint_v1(&ctx, &prompt, &opening_at(covered), claim_of(segment));
            if kaspa_consensus_core::palw_context_ladder::palw_checkpoint_leaf_carries_recurrence_v1(&profile, positions) {
                let replay = got.expect("hybrid execute-from-checkpoint must resume");
                assert!(replay.matches, "segment {segment} from {covered}: the resumed leaves are not the committed ones");
                assert!(!replay.window.genesis(), "a decode leaf does not resume from the prompt");
                resumed += 1;
            } else {
                assert_eq!(
                    got.expect_err("no resume point"),
                    crate::segment_opening::Base0SegmentRefusalV1::AnchorNotAResumePoint.to_string(),
                    "segment {segment} from {covered}: a leaf without the recurrence is refused, not replayed into a fault"
                );
            }
        }
        // And from the prompt, the same segment roots to the claim.
        let genesis = crate::segment_opening::base0_segment_opening_v2(
            &retention,
            seats,
            segment,
            crate::segment_opening::Base0SegmentAnchorV1::Genesis,
            Some(&profile),
            backend.step_ladder_cap(),
        )
        .expect("genesis")
        .encode_v2()
        .expect("encodes");
        let replay = backend.replay_segment_from_checkpoint_v1(&ctx, &prompt, &genesis, claim_of(segment)).expect("replays");
        assert!(replay.matches, "the genesis replay of segment {segment} roots to the claim");
        eprintln!("hybrid segment {segment} (step {step}): {resumed} recurrence-carrying anchors resumed");
    }

    /// The drill's forgery at `leaf` — one lane of its tile moved, the commitment re-derived exactly
    /// as `execute_with_injected_fault` re-derives it — retained as a FOLD, the free-prompt lane's
    /// retention (ADR-0093 Decision 7's case).
    fn forged_fold_v1(backend: &Qwen36Backend, job: &PalwJobContextV2, prompt: &[usize], leaf: u64) -> Vec<u8> {
        let (plan, profile) = (backend.plan.as_ref().expect("a plan"), backend.profile.as_ref().expect("a registered graph"));
        let cap = backend.step_ladder_cap();
        let mut run = qwen36_execute_for_attempt_capped_v1(&backend.artifact, profile, plan, job, prompt, cap).expect("runs");
        {
            let (ctx_hash, profile_hash) = (job.context_hash(), profile.shape_profile_id());
            let slot = run.tiles.tiles.iter_mut().find(|(i, _)| *i == leaf).expect("the tile");
            slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
            run.tiles.leaves[leaf as usize] =
                kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &slot.1);
        }
        run.binding = crate::legs::base0_binding_from_capture_with_profile_capped_v1(
            profile,
            job,
            &run.tiles,
            &run.checkpoints,
            &qwen36_checkpoint_profile_v1(profile),
            run.trace_root,
            crate::produce::base0_activation_leg_root_v1(job),
            cap,
        )
        .expect("the forged binding");
        run.step_tree = Some(
            crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(
                &run.tiles.leaves,
                crate::fp_capture::palw_base0_sparse_retain_level_v1(cap),
                cap,
            )
            .expect("the fold of the forged leaves"),
        );
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        crate::produce::base0_fp_material_encode_v2(&run, &ids).expect("the fold retains")
    }

    /// **ADR-0093 as built, on the HYBRID tier: a real fused capture answers its own dissection.**
    ///
    /// At THIS fixture's geometry graph-v5's hybrid fused tile is the whole 64-lane row over 16-lane
    /// heads (the shipped geometries budget 8 or 4 lanes, inside their heads), so its fused leaf is
    /// several heads' and the court refuses to dissect it by name — PINNED here as the limitation
    /// it is; admission refuses such a class past `Params::palw_fused_dissectable` (ADR-0093
    /// Decision 6). Graph-v6 cuts the fused row at the head: every fused leaf of the last position
    /// answered from the backend's evidence —
    /// the root finalizes to the committed tile, and the bottom, served out of the composed map's
    /// anchor (the recurrence's chunks rooted beside the cache's), recomputes it exactly. The job
    /// stays below the recurrence's spacing (see the graph-v5 test above), so a history is one
    /// court tile and the phase opens at its bottom; the rounds are the dense tier's test's.
    /// Forged, the least lie that finalizes to the forged tile is convicted at that bottom.
    #[test]
    fn a_real_hybrid_fused_capture_answers_its_dissection_exactly_and_a_forged_row_is_convicted() {
        use kaspa_consensus_core::palw_attn_court_v1::{
            PalwAttnCourtVerdictV1, PalwAttnDissectPhaseV1, check_attn_dissect_bottom_v1, palw_attn_opened_lanes_v1,
        };
        use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
        use kaspa_consensus_core::palw_step::{PalwStepOpKindV1, canonical_step_coordinates};

        let (artifact, v5) = crate::fuzz_qwen36::tiny_class_v5_for_tests();
        let geometry = kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
            n_ctx: v5.n_ctx,
            ..crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4)
        };
        assert_eq!(
            kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v5(geometry).expect("v5").shape_profile_id(),
            v5.shape_profile_id(),
            "one geometry for both graphs"
        );
        let v6 = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v6(geometry).expect("the v6 projection");
        // ADR-0103 Decision 3: graph-v7 is graph-v6 on the held composition — the same per-head
        // fused tile, and the bottom's anchor proving into its slice's sub-root and the top tree.
        let v7 = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7(geometry).expect("the v7 projection");
        assert!(kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&v7));
        let artifact = std::sync::Arc::new(artifact);
        let session = Hash64::from_u64_word(0x5E56);
        for (graph, profile) in [("v5", v5), ("v6", v6), ("v7", v7)] {
            let backend =
                Qwen36Backend::from_registered_profile(artifact.clone(), b"misaka-palw-test".to_vec(), profile.clone(), (3, 4))
                    .expect("the fused hybrid is servable");
            assert!(backend.has_fused_site());
            assert_eq!(backend.supports_dissection(), graph != "v5", "{graph}: the backend says whether its fused tile is one head's");
            let root = crate::inventory::qwen36_inventory_v1(&artifact, &profile).expect("the inventory").root();
            let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0000_9336)).expect("a job");
            let honest = backend.execute(&job, &prompt).expect("runs");
            let binding = crate::produce::base0_material_decode_any_v1(&honest.material).expect("decodes").binding().clone();
            let last_call = job.exact_decode_tokens.saturating_sub(1);
            let fused: Vec<u64> = (0..binding.step_leaf_count)
                .filter(|i| {
                    let c = canonical_step_coordinates(&profile, &binding.job_context, *i).expect("a coordinate");
                    c.call_index == last_call
                        && profile.resolve_node_slot(c.node_slot).is_some_and(|(n, _)| n.op_kind == PalwStepOpKindV1::AttnFused)
                })
                .collect();
            assert!(!fused.is_empty(), "the hybrid's attention layer has a fused site at the last position");
            if graph == "v5" {
                let refused = backend.attn_site_evidence(&honest.material, fused[0], None, None).expect_err("a two-head fused tile");
                assert!(refused.contains("ONE head"), "refused by name: {refused}");
                continue;
            }
            for &narrowed in &fused {
                let evidence = backend
                    .attn_site_evidence(&honest.material, narrowed, None, None)
                    .expect("the honest capture yields its evidence");
                let site = evidence.site_v1(root, false).expect("the site derives");
                let anchored = evidence.site_v1(root, true).expect("and with its anchor");
                let root_claim = evidence.root_claim_v1(&site).expect("the root computes");
                let committed =
                    palw_attn_opened_lanes_v1(&evidence.out_tile, &anchored.binding, site.head_lanes.2 as usize).expect("opens");
                let open = |claim: &kaspa_consensus_core::palw_attn_dissect::PalwAttnRootClaimV1, tile: &[i32]| {
                    PalwAttnDissectPhaseV1::open_with_arity(
                        session,
                        claim,
                        site.head_lanes,
                        site.history_positions,
                        tile,
                        site.site.params.values,
                        2,
                        site.tile_positions,
                        0,
                        10,
                        true,
                    )
                };
                let phase = open(&root_claim, &committed).expect("the honest root finalizes to the committed tile");
                assert_eq!(phase.turn(), PalwBisectTurnV1::Terminal, "one court tile: the phase opens at its bottom");
                let bottom = evidence.bottom_v1(&anchored, &phase).expect("the bottom builds out of the composed anchor");
                assert_eq!(
                    check_attn_dissect_bottom_v1(&phase, &bottom, &anchored.binding, &anchored.site, true),
                    Ok(PalwAttnCourtVerdictV1::ChallengerDefeated),
                    "leaf {narrowed}: the hybrid evidence must be exact on real committed rows"
                );

                // Forged: the output tile moved, the least lie that finalizes to it, convicted.
                let lying = backend.execute_with_injected_fault(&job, &prompt, narrowed).expect("a forged capture commits");
                let accused =
                    backend.attn_site_evidence(&lying.material, narrowed, None, None).expect("the accused's commitments open");
                let accused_site = accused.site_v1(root, true).expect("site");
                let forged =
                    palw_attn_opened_lanes_v1(&accused.out_tile, &accused_site.binding, site.head_lanes.2 as usize).expect("opens");
                assert!(open(&root_claim, &forged).is_err(), "an honest root cannot finalize to a forged tile");
                let values = site.site.params.values;
                let finalize = |v: &[i64]| kaspa_consensus_core::palw_base0_a16::a16_attn_finalize_v1(v, values);
                let lane = (0..forged.len()).find(|l| finalize(&root_claim.claim.v_acc)[*l] != forged[*l]).expect("a moved lane");
                let at = |delta: i64| {
                    let mut v = root_claim.claim.v_acc.clone();
                    v[lane] += delta;
                    finalize(&v)[lane]
                };
                let (mut lo, mut hi) = (-(1i64 << 44), 1i64 << 44);
                while lo < hi {
                    let mid = lo + (hi - lo) / 2;
                    if at(mid) < forged[lane] { lo = mid + 1 } else { hi = mid }
                }
                let mut lie = root_claim.clone();
                lie.claim.v_acc[lane] += lo;
                let phase = open(&lie, &forged).expect("the least lie opens");
                let bottom = accused.bottom_v1(&accused_site, &phase).expect("the accused's bottom builds");
                assert_eq!(
                    check_attn_dissect_bottom_v1(&phase, &bottom, &accused_site.binding, &accused_site.site, true),
                    Ok(PalwAttnCourtVerdictV1::ExecutorGuilty),
                    "leaf {narrowed}: the forged hybrid row is convicted"
                );

                // ADR-0093 Decision 7 on the hybrid tier: the same forgery retained as a FOLD opens
                // nothing alone, and with the accused's own tile (its root claim's) it IS the dense
                // capture's evidence — so the conviction above is the fold's too.
                let fold = forged_fold_v1(&backend, &job, &prompt, narrowed);
                assert!(
                    backend.attn_site_evidence(&fold, narrowed, None, None).is_err(),
                    "leaf {narrowed}: a forged fold alone opens nothing"
                );
                let from_fold = backend
                    .attn_site_evidence(&fold, narrowed, None, Some(&accused.out_tile))
                    .expect("the fold opens through the prefix");
                assert_eq!(from_fold, accused, "leaf {narrowed}: the hybrid fold's evidence IS the dense capture's");
            }
        }
    }

    /// **ADR-0102: `graph-v6` reads the embedding lift per TOKEN, so a calibrated store is
    /// adjudicable.** The converter writes one `embed_lift.a16` triple per vocabulary row and the
    /// engine applies `lift.get(token_id)`; `graph-v5`'s lane-sliced lift names a per-lane table no
    /// calibrated artifact has, so its inventory refuses the store (ADR-0070 §7(b)) and its court
    /// could not read it. Under `graph-v6` the same fixture — its lift made per-token and
    /// non-trivial — clears at every leaf the one-step court can try, and a tampered lift lane
    /// convicts at a prompt position and at a decode call (whose token is a generated one).
    #[test]
    fn a_graph_v6_hybrid_adjudicates_a_per_token_lift_and_a_tampered_lift_convicts() {
        use kaspa_consensus_core::palw_base0_a16::A16QuantParams;
        use kaspa_consensus_core::palw_qwen36_profile::{qwen36_profile_v2, qwen36_profile_v5, qwen36_profile_v6};
        use kaspa_consensus_core::palw_step::{PalwStepOpKindV1, kernel_semantics_id_v1};
        use kaspa_consensus_core::palw_step_refute::{
            KDESC_A16_REQUANTIZE_BY_TOKEN, PalwStepRefuteError, check_execution_step_refutation_v1,
        };

        let base = crate::qwen36::test_fixture(4, 8);
        let vocab = base.shape.vocab;
        // One triple per token, and not all alike: a lift the court resolved at the wrong row
        // would recompute a different row and convict an honest claim.
        let lift: Vec<A16QuantParams> =
            (0..vocab).map(|t| A16QuantParams { multiplier: 1 + (t % 3) as i64, shift: (t % 2) as u8, zero: 0 }).collect();
        let artifact = std::sync::Arc::new(base.with_params("embed_lift.a16", &lift));
        let geometry = crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4);

        // The gap this graph closes, pinned: the lane-sliced lift cannot serve the store.
        let lane_sliced = qwen36_profile_v2(geometry).expect("the v2 projection");
        let refused = crate::inventory::qwen36_inventory_v1(&artifact, &lane_sliced).expect_err("a per-token store is not per-lane");
        assert!(format!("{refused:?}").contains("embed_lift.a16"), "the refusal names the store: {refused:?}");

        let profile = qwen36_profile_v6(geometry).expect("the v6 projection");
        assert_ne!(
            profile.shape_profile_id(),
            qwen36_profile_v5(geometry).expect("the v5 projection").shape_profile_id(),
            "a class is its graph: v6 is a new class over the same weights"
        );
        let inventory = crate::inventory::qwen36_inventory_v1(&artifact, &profile).expect("graph-v6 serves the per-token store");
        let lift_rows = inventory.operands().iter().filter(|o| o.tensor_name == "embed_lift.a16").count();
        assert_eq!(lift_rows, vocab, "one leaf per vocabulary row, so an opening proves exactly one triple");
        // The engine's other reading, a one-row store lifting every token (the fixture's own): the
        // same graph serves it, tiled across the vocabulary — the engine and the court agree on
        // every store the engine executes.
        let singleton = crate::inventory::qwen36_inventory_v1(&crate::qwen36::test_fixture(4, 8), &profile)
            .expect("a one-row lift serves every token under graph-v6");
        assert_eq!(singleton.operands().iter().filter(|o| o.tensor_name == "embed_lift.a16").count(), vocab);
        // And the root this graph registers is that inventory's; the rows before it keep theirs.
        assert!(crate::inventory::qwen36_registers_inventory_root_v1(&profile));
        assert!(!crate::inventory::qwen36_registers_inventory_root_v1(&qwen36_profile_v5(geometry).expect("v5")));
        assert!(!crate::inventory::qwen36_registers_inventory_root_v1(&lane_sliced));

        // Three prompt positions and four decode calls: six positions, below the recurrence's
        // spacing on this fixture (`min(16, n_ctx)` = 8), where its mismatched gdn head counts
        // stop the serializer — the same sizing, for the same reason, as the graph-v5 test above.
        let backend = Qwen36Backend::from_registered_profile(artifact.clone(), b"misaka-palw-test".to_vec(), profile.clone(), (3, 4))
            .expect("graph-v6 is servable");
        assert!(backend.supports_court());
        let anchor = Hash64::from_u64_word(0x0102_0102);
        let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let outcome = backend.execute(&job, &prompt).expect("the v6 class runs");
        let (binding, _tiles, _logits, _generated, _chunks) =
            crate::produce::base0_material_decode_v1(&outcome.material).expect("the captured material decodes");

        let openings: Vec<_> = (0..inventory.operands().len())
            .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
            .collect();
        let oracle = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
            .expect("every inventory row proves against its own root");
        let coord_of = |index: u64| {
            kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &job, index).expect("a main step coordinate")
        };
        let node_of = |index: u64| profile.resolve_node_slot(coord_of(index).node_slot).map(|(n, _)| n.clone());
        let by_token = kernel_semantics_id_v1(KDESC_A16_REQUANTIZE_BY_TOKEN);

        // Every leaf the one-step court can try clears the honest capture; the fused attention
        // site is the dissection's (ADR-0082), not this court's.
        let mut lift_leaves = Vec::new();
        for index in 0..binding.step_leaf_count {
            let Some(node) = node_of(index) else { continue };
            if node.op_kind == PalwStepOpKindV1::AttnFused {
                continue;
            }
            if node.kernel_semantics_id == by_token {
                lift_leaves.push(index);
            }
            let refutation = backend.refutation_for_index(&outcome.material, index).expect("an honest leaf opens");
            let got = check_execution_step_refutation_v1(&refutation, &oracle);
            assert!(
                matches!(got, Err(PalwStepRefuteError::NoFaultFound)),
                "an honest execution must clear itself at leaf {index} ({}): got {got:?}",
                node.weight_name
            );
        }
        let prompt_lift = *lift_leaves.iter().find(|i| coord_of(**i).call_index == 0).expect("a prompt position's lift");
        let decode_lift = *lift_leaves.iter().find(|i| coord_of(**i).call_index > 0).expect("a decode call's lift");
        for index in [prompt_lift, decode_lift] {
            let lying = backend.execute_with_injected_fault(&job, &prompt, index).expect("a tampered capture still commits");
            let refutation = backend.refutation_for_index(&lying.material, index).expect("a tampered capture opens too");
            let openings = backend.operand_openings_for(&refutation).expect("the prover opens the one lift row the court reads");
            assert!(
                openings
                    .iter()
                    .any(|o| o.operand.tensor_name == "embed_lift.a16" && o.operand.bytes.len() == A16QuantParams::WIRE_BYTES),
                "the court read exactly one 17-byte triple of the lift"
            );
            let proven = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
                .expect("recorded openings prove");
            assert!(
                check_execution_step_refutation_v1(&refutation, &proven).is_ok(),
                "a tampered lift lane at leaf {index} (call {}) must convict",
                coord_of(index).call_index
            );
        }
    }

    /// **A held hybrid row's attempt folds, and stays adjudicable** — the dense tier's theorem
    /// (`a_held_classs_attempt_folds_and_stays_adjudicable`) on the hybrid graph, and the fix for
    /// the producer ibm lost on 2026-09-23: `Qwen3.6-35B-A3B/graph-v7@512` at 63 + 2 under the
    /// dense sink grew 9 GiB of tiles beside a 6.65 GiB residency on a 7 GiB share and was killed.
    /// Here the same fixture graph on the held (v7) composition: the attempt's material is the
    /// fold (no tiles, the anchor's prompt aboard), its four roots are the dense sink's to the
    /// bit, the verdict replay agrees, the seat check reads `Matches`, the ladder's rungs and a
    /// leaf's refutation are answered from the fold as from the tiles, the drill's tamper stays
    /// a DENSE capture that the honest claim refuses, and the resource profile prices the fold.
    #[test]
    fn a_held_hybrid_classs_attempt_folds_and_stays_adjudicable() {
        use kaspa_consensus_core::palw_resource_profile_v1::{PalwCaptureRetentionV1, PalwResourceRoleV1, palw_attempt_capture_folds_v1};
        let (artifact, v5) = crate::fuzz_qwen36::tiny_class_v5_for_tests();
        let geometry = kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
            n_ctx: v5.n_ctx,
            ..crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4)
        };
        let v7 = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7(geometry).expect("the v7 projection");
        assert!(palw_attempt_capture_folds_v1(&v7) && !palw_attempt_capture_folds_v1(&v5), "held folds, per-call keeps its tiles");
        let artifact = std::sync::Arc::new(artifact);
        let backend = Qwen36Backend::from_registered_profile(artifact.clone(), b"misaka-palw-test".to_vec(), v7.clone(), (3, 4))
            .expect("the held hybrid is servable");
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0151_9336)).expect("a job");
        let folded = backend.execute(&job, &prompt).expect("the held attempt executes");
        let material = crate::produce::base0_material_decode_any_v1(&folded.material).expect("decodes");
        let crate::produce::Base0RetentionV1::Folded(m) = &material else { panic!("a held attempt retains the fold") };
        assert!(material.tiles().is_none(), "no tiles were retained");
        assert!(m.step_tree.retained_len() >= 1);
        assert_eq!(m.prompt_token_ids, prompt.iter().map(|t| *t as u32).collect::<Vec<_>>(), "the anchor's prompt rides the material");

        // The roots are the dense sink's, to the bit.
        let plan = backend.plan.as_ref().expect("a registered graph has a plan");
        let dense = qwen36_execute_for_attempt_capped_v1(&artifact, &v7, plan, &job, &prompt, backend.step_ladder_cap())
            .expect("the dense capture of the same job");
        assert_eq!(folded.execution_root, dense.execution_root, "the fold's execution root is the dense capture's");
        assert_eq!(folded.trace_root, dense.trace_root);
        assert_eq!(folded.output_root, dense.output_root);
        assert_eq!(folded.trace_manifest_root, dense.trace_manifest_root);
        let verdict = backend.execute_for_verdict(&job, &prompt).expect("the verdict replay");
        assert_eq!((verdict.execution_root, verdict.trace_root), (folded.execution_root, folded.trace_root));

        // The seat check and the court verbs read the folded material.
        let claim = PalwClaimRootsV1 {
            execution_root: folded.execution_root,
            trace_root: folded.trace_root,
            anchor: job.job_id,
            attempt_draw: None,
            output_root: None,
            job_pin: None,
        };
        assert_eq!(backend.verify_material(&folded.material, claim), PalwMaterialVerdictV1::Matches);
        let dense_material = crate::produce::base0_material_encode_v1(&dense).expect("the dense material encodes");
        let leaves = dense.binding.step_leaf_count;
        for index in [0u64, leaves / 3, leaves / 2, leaves - 1] {
            assert_eq!(
                backend.bisect_prefix_state(&folded.material, index),
                backend.bisect_prefix_state(&dense_material, index),
                "a rung at {index} is answered from the fold as from the tiles"
            );
        }
        let refutation = backend.refutation_for_index(&folded.material, leaves / 2 + 1).expect("a leaf opens from the fold");
        let from_dense = backend.refutation_for_index(&dense_material, leaves / 2 + 1).expect("and from the tiles");
        assert_eq!(refutation.binding.committed_execution_root, folded.execution_root);
        assert_eq!(refutation.output_preimage, from_dense.output_preimage, "the same leaf, the same preimage");

        // The drill tampers a DENSE capture even on the held class.
        let guilty = backend.execute_with_injected_fault(&job, &prompt, 3).expect("the drill's dense tamper commits");
        assert_ne!(guilty.execution_root, folded.execution_root, "the lie moved the commitment");
        assert!(matches!(crate::produce::base0_material_decode_any_v1(&guilty.material), Ok(crate::produce::Base0RetentionV1::Dense(_))));
        assert_eq!(backend.verify_material(&guilty.material, claim), PalwMaterialVerdictV1::Mismatch, "a lie is not the honest claim");
        assert!(backend.refutation_for_index(&guilty.material, 3).is_ok(), "the tampered leaf opens from the retained tiles");
        assert_eq!(backend.execute_for_verdict(&job, &prompt).expect("replays").execution_root, folded.execution_root, "the replays stay honest");

        // The profile prices the fold, the recurrence state and the attention rows — and the
        // producer's figure the gate asks for is the same derivation.
        let profile = backend.resource_profile_v1(Some(&job), PalwResourceRoleV1::Producer).expect("derives");
        assert!(matches!(profile.capture, PalwCaptureRetentionV1::Fold { .. }), "{:?}", profile.capture);
        assert_eq!(profile.leaves, leaves);
        assert!(profile.recurrence_layers >= 1 && profile.gdn_state_bytes > 0, "a hybrid prices its recurrence state");
        assert!(profile.capture_retained_bytes < 1 << 20, "a small job's fold is small: {}", profile.capture_retained_bytes);
        assert_eq!(backend.runtime_profile_v1().map(|p| p.name()), Some("Q36-KV-i32"));
        assert_eq!(
            backend.attempt_working_set_bytes(job.declared_prefill_tokens as usize),
            Some(profile.working_set_bytes()),
            "the gate's figure is the profile's"
        );

        // A class outside the regime keeps its tiles, priced as such.
        let dense_backend = Qwen36Backend::from_registered_profile(artifact, b"misaka-palw-test".to_vec(), v5, (3, 4)).expect("servable");
        let (job2, prompt2) = dense_backend.job_for_anchor(Hash64::from_u64_word(0x0151_DE45)).expect("a job");
        let outcome = dense_backend.execute(&job2, &prompt2).expect("executes");
        assert!(matches!(crate::produce::base0_material_decode_any_v1(&outcome.material), Ok(crate::produce::Base0RetentionV1::Dense(_))));
        let dense_profile = dense_backend.resource_profile_v1(Some(&job2), PalwResourceRoleV1::Producer).expect("derives");
        assert!(matches!(dense_profile.capture, PalwCaptureRetentionV1::DenseTiles { .. }), "{:?}", dense_profile.capture);
        assert!(dense_profile.capture_retained_bytes >= dense_profile.leaves * (64 + 56));
    }

    /// **ADR-0152 §4-ter N4 (the review's F4): past `palw_offence_attribution` no held hybrid class
    /// takes a dissection's turn** — this family has no windowed builder, so its held (graph-v7) row
    /// says `false` there, while it says `true` below the fence and its non-held (graph-v6) row is
    /// unchanged on either side. The consensus predicate agrees (the class is `Recurrent`).
    #[test]
    fn past_the_attribution_fence_no_held_hybrid_class_takes_the_dissections_turn() {
        let (artifact, v5) = crate::fuzz_qwen36::tiny_class_v5_for_tests();
        let geometry = kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
            n_ctx: v5.n_ctx,
            ..crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4)
        };
        let v6 = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v6(geometry).expect("the v6 projection");
        let v7 = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7(geometry).expect("the v7 projection");
        let artifact = std::sync::Arc::new(artifact);
        let build = |profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3, armed: bool| {
            Qwen36Backend::from_registered_profile(artifact.clone(), b"misaka-palw-test".to_vec(), profile.clone(), (3, 4))
                .expect("servable")
                .with_held_answerability_v1(armed)
        };
        assert!(build(&v7, false).supports_dissection(), "below the fence the held hybrid answered as before");
        assert!(!build(&v7, true).supports_dissection(), "past it no held hybrid class takes the turn");
        assert!(build(&v6, false).supports_dissection() && build(&v6, true).supports_dissection(), "a non-held row is unchanged");
        assert!(matches!(
            kaspa_consensus_core::palw_class_admission_v2::palw_held_class_unanswerable_v1(&v7),
            Some(kaspa_consensus_core::palw_class_admission_v2::PalwHeldUnanswerableV1::Recurrent { .. })
        ));
    }
}
