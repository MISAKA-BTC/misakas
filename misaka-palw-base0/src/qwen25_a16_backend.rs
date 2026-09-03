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
use kaspa_consensus_core::palw_step::{PALW_STEP_MAX_LEAVES, PalwShapeProfileV3};
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
    a16_execute_for_attempt_capped_v1(artifact, profile, plan, ctx, prompt, PALW_STEP_MAX_LEAVES)
}

/// [`a16_execute_for_attempt_v1`] against the ladder top the CALLER states — the ruleset's
/// `PalwCourtParamsV2::max_step_leaf_count`, which is the only correct argument.
pub fn a16_execute_for_attempt_capped_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    plan: Option<&crate::engine_a16::A16ProfilePlanV1>,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
) -> Result<crate::produce::Base0ExecutionV1, String> {
    a16_execute_for_attempt_streaming_capped_v1(artifact, profile, plan, ctx, prompt, max_step_leaf_count, &mut |_| {})
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
    a16_execute_for_attempt_streaming_capped_v1(artifact, profile, plan, ctx, prompt, PALW_STEP_MAX_LEAVES, on_token)
}

/// **The capture, priced against the RULESET's ladder** (ADR-0077 Decision 12).
///
/// This is the one that decides how many tokens a user gets. The court's `max_step_leaf_count` is
/// what a network froze; `PALW_STEP_MAX_LEAVES` is what every shipped preset froze it at — and
/// until this argument existed, the executor read the constant and nothing else. That made the
/// decode budget a build-time fact: measured on the dense A16 row, a 26-token prefill buys 12
/// decode tokens against `2^22` and 11 120 against `2^32`, and **widening `n_ctx` moves neither
/// number**. A class registered against a deeper frozen ladder was refused by its own executor,
/// and every width ADR-0080's split close buys the court was worth nothing here.
///
/// `PALW_STEP_MAX_LEAVES` is what the delegating entry points above pass, so a caller that does
/// not hold a ruleset is byte-identical to what it was.
#[allow(clippy::too_many_arguments)]
pub fn a16_execute_for_attempt_streaming_capped_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    plan: Option<&crate::engine_a16::A16ProfilePlanV1>,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
    on_token: &mut dyn FnMut(u32),
) -> Result<crate::produce::Base0ExecutionV1, String> {
    a16_execute_streaming_v1(
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

/// **The same run, FOLDED** (ADR-0082 Decision 7) — the free-prompt lane's capture.
///
/// Identical in every respect a commitment can see: the same loop, the same rows, the same leaf
/// hashes, the same four roots. What differs is what survives the loop — one retained node per
/// `2^retain_level` leaves instead of every tile of every node of every position (~50 MB a
/// position on this tier) — and therefore what an opening costs to serve: a replay of the
/// interval rather than a lookup in memory the executor could not hold.
pub fn a16_execute_free_prompt_streaming_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    plan: Option<&crate::engine_a16::A16ProfilePlanV1>,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
    on_token: &mut dyn FnMut(u32),
) -> Result<crate::produce::Base0ExecutionV1, String> {
    a16_execute_streaming_v1(
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

/// **The one capture loop this family has**, over either sink. A second loop would be a second
/// enumeration, and two enumerations of one step space is two commitments.
#[allow(clippy::too_many_arguments)]
fn a16_execute_streaming_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    plan: Option<&crate::engine_a16::A16ProfilePlanV1>,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
    capture_kind: crate::legs::Base0CaptureKindV1,
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

    let leaf_count =
        kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, ctx, max_step_leaf_count).map_err(|e| format!("{e:?}"))?;
    let mut capture = crate::legs::Base0CaptureSinkV1::for_kind(capture_kind, profile, ctx, leaf_count, max_step_leaf_count)
        .map_err(|e| format!("{e:?}"))?;
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
        // **A checkpoint after a PREFILL position, when the class's cadence says so** (ADR-0082
        // Decision 4, amended). A per-call class wants none of these and this is `false` at every
        // prefill position; a class whose map addresses history tiles wants one after every
        // position, because a dispute at a prefill position with no anchor opens `p + 1` cache
        // rows per kind and its bottom is three chunks no carrier can file.
        if checkpoints.wants_checkpoint_after_v1(0, position as u32) {
            checkpoints
                .push_with_v1(|entry| cache.state_chunk_bytes_v1(entry))
                .map_err(|e| format!("the prefill checkpoint at position {position}: {e:?}"))?;
        }
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
        // Through the CACHE's own serializer, at the width the class declares. Under a map that
        // cannot describe this state — the v1 one-byte map over an `i32` cache — this refuses, and
        // the run fails here rather than committing a checkpoint that opens to a state it never
        // held. The boundary is the capture's own predicate, so the prefill arm above and this one
        // cannot drift into two cadences.
        if checkpoints.wants_checkpoint_after_v1(call as u32, 0) {
            checkpoints
                .push_with_v1(|entry| cache.state_chunk_bytes_v1(entry))
                .map_err(|e| format!("the checkpoint after decode call {call}: {e:?}"))?;
        }
    }

    let checkpoints = checkpoints.finish_canonical_v1().map_err(|e| format!("{e:?}"))?;
    let captured = capture.finish(max_step_leaf_count).map_err(|e| format!("{e:?}"))?;

    // **This class's own trace scheme, not the floor's.** The retained rows ARE the selecting
    // rows (row `r` is the one `generated[r]` was chosen from), so the tiled root commits them
    // directly and a seat recomputes it from the same retention.
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
    /// **ADR-0067: the class's declaration, compiled.** Every declared node bound to a served
    /// kernel and a named operand, or the constructor refuses — and every forward walks it. BOTH
    /// constructors fill it now (ADR-0082 audit E, H-1): the plan-less route was
    /// `forward_token_traced`, a hand-written twenty-seven-row v2 program that cannot serve a
    /// fused attention site, so a `None` here meant the shipped producer refused its own class as
    /// soon as the dense tier moved to graph v5. `Option` survives only because the field is read
    /// by `a16_execute_for_attempt_v1`, whose `plan` argument is public and whose `None` arm is
    /// the v2-reference path (and the Decision-F probe that guards it).
    plan: Option<crate::engine_a16::A16ProfilePlanV1>,
    /// **Whether this instance's class can carry a capture at all** — the four-byte state map,
    /// which is the width `A16Cache` holds. Decided from the profile at construction (declared
    /// rather than probed: `supports_court` is read at boot, before any capture exists). The v1
    /// registered class declares the one-byte map and therefore cannot commit a checkpoint leg;
    /// its instances keep the legacy composite roots and say `supports_court() == false`, which
    /// is the honest description of that class. The Decision-F graph correspondence is still
    /// guarded inside the captured path itself.
    court_capable: bool,
    /// **The ladder top this instance measures a capture against** — the ruleset's
    /// `PalwCourtParamsV2::max_step_leaf_count`, not the leg's module constant.
    ///
    /// It bounds a `step_leaf_count` that arrived over gossip inside a borsh blob BEFORE the leaf
    /// vector is built, so it cannot simply be dropped; and it was the leg's default constant, so
    /// a class registered against a deeper frozen ladder was refused by its own executor. It
    /// defaults to that constant — every shipped construction site keeps exactly the behaviour it
    /// had — and [`Qwen25A16Backend::with_step_ladder_cap`] is how a caller that HOLDS the ruleset
    /// states the real one.
    step_ladder_cap: u64,
}

/// **Can a class with this graph carry a capture at all — the ONE spelling of the predicate.**
///
/// The question is about the WIDTH of the cache the map describes, not about a particular map id:
/// a checkpoint leg needs the four-byte layout `A16Cache` holds. Two maps declare it — the
/// integer-kv v2 map, which addresses the whole history as one chunk, and ADR-0082 Decision 4's
/// tiled v3 map, which addresses it a tile at a time so a dissection's bottom can open one tile.
/// Both are court-capable; they differ in how finely an opening is addressed, which is a cost
/// question and not a capability one.
///
/// Spelled once because the same predicate was written out twice in the two constructors and a
/// third time in `misaka-palw-base0::classes` as the `artifact_root` discriminator, all three
/// naming v2 alone — so a `graph-v5` class, whose whole point is the tiled map, declared itself
/// NOT court-capable over its own registered profile and ADR-0069 Decision 5 would have admitted
/// it weightless. `palw_map_addresses_history_tiles_v1` is the map module's own dispatch for
/// exactly this: "the alternative is every caller writing `if map == v3 { … }` and the first one
/// to forget prices a tiled class at the whole history".
pub fn a16_court_capable_v1(profile: &PalwShapeProfileV3) -> bool {
    use kaspa_consensus_core::palw_state_chunk_map as map;
    map::palw_map_addresses_history_tiles_v1(profile) || profile.state_chunk_map_id == map::integer_kv_state_chunk_map_id_v2()
}

impl Qwen25A16Backend {
    /// **This build's own table names the class — and the class's DECLARATION is still the
    /// program.** (ADR-0067 Decision 2; the constructor that closes ADR-0082 audit E's H-1.)
    ///
    /// This used to store `plan: None` and execute through
    /// [`A16Engine::forward_token_traced`], which is a hand-written program: twenty-seven rows a
    /// layer, the attention site spelled as scores / softmax / requant / values. That was a second
    /// authority beside [`A16Engine::plan_from_profile`], and ADR-0082's graph v5 is exactly the
    /// row where the two part — a v5 layer declares TWENTY-FOUR nodes, so the shipped producer
    /// refused its own class by name the moment the dense tier moved to a fused site:
    ///
    /// ```text
    /// this class's registered graph does not name every narrowing its engine performs
    /// (ADR-0049 Decision F): … per-layer declares 24 against 27 recorded.
    /// ```
    ///
    /// Compiling the plan here is what makes that refusal impossible rather than merely unlikely:
    /// one authority, the same one the chain-registered path uses, and the traced route retires to
    /// being the v2 reference it always was. Teaching the traced route the fusion instead would
    /// have re-created the second authority.
    ///
    /// **Fallible, because the compile can honestly fail.** A profile whose geometry is not this
    /// artifact's — the `rms_eps_q` split that `qwen25_a16_artifact_row_profile_v*` exists to
    /// project is the live example — no longer executes silently under the artifact's constants
    /// while declaring somebody else's. That silence was the asymmetry
    /// [`crate::qwen25_a16_backend::Qwen25A16Backend::from_registered_profile`] was found by, and
    /// it is now closed on both constructors.
    pub fn new(
        artifact: std::sync::Arc<Base0ArtifactV1>,
        network_id: Vec<u8>,
        profile: PalwShapeProfileV3,
        canonical_job: (u32, u32),
    ) -> Result<Self, String> {
        let engine = A16Engine::new(&artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
        // The same two facts `from_registered_profile` keeps apart (round-3 defect I-3): a kernel
        // this build does not carry is not the same statement as this node's own capacity bound.
        let plan = engine.plan_from_profile(&profile).map_err(|e| match e {
            A16PlanErrorV1::OverMemoryCeiling { bytes, ceiling } => format!(
                "this node's interpreted-execution capacity refuses this class's graph: one token's committed trace is \
                 {bytes} bytes and this build's capacity is {ceiling} (ADR-0067 SA-1). This is node-local servability, \
                 not a statement about the class: a node built with a larger ceiling serves the very same graph"
            ),
            other => format!("this build cannot serve the graph this class declares: {other:?}"),
        })?;
        let shape_id = artifact.artifact_digest();
        let class_profile_id = profile.shape_profile_id();
        let court_capable = a16_court_capable_v1(&profile);
        Ok(Self {
            artifact,
            model_id: "PALW-QWEN25-A16".to_string(),
            network_id,
            shape_id,
            profile,
            class_profile_id,
            canonical_job,
            plan: Some(plan),
            court_capable,
            step_ladder_cap: kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
        })
    }

    /// **The ladder top from the ruleset**, for a caller that holds `PalwCourtParamsV2`. Passing
    /// `max_step_leaf_count` is the only correct argument; passing the leg's default constant is
    /// what both constructors already do.
    pub fn with_step_ladder_cap(mut self, max_step_leaf_count: u64) -> Self {
        self.step_ladder_cap = max_step_leaf_count;
        self
    }

    /// The ladder top this instance refuses a capture above.
    pub fn step_ladder_cap(&self) -> u64 {
        self.step_ladder_cap
    }

    /// **A dense class's artifact must SAY which tokenizer its ids belong to, or this constructor
    /// refuses it by name.**
    ///
    /// `Hash64::default()` is not "no opinion": it is published as the job's `tokenizer_id`
    /// ([`PalwExecutionBackendV1::job_for_anchor`] below reads this field), it goes into
    /// `PalwJobContextV2::context_hash`, and `PalwFreePromptJobV3::tokenizer_id` "MUST equal the
    /// class row's". A zero there pins nothing, so a seat replaying with a different
    /// `tokenizer.json` gets different ids, a different `prompt_token_ids_hash`, a different
    /// context hash and a claim nobody reproduces — an `Unavailable` quorum and a defaulted
    /// producer, not a refusal. `Base0ArtifactV1::check_tokenizer_bytes_v1` already answers
    /// `Undeclared` for this state and its own doc says a runtime reaching it "must SAY so"; this
    /// is the dense tier saying so at the one boundary where the answer can still be a refusal.
    ///
    /// **Why here and not in the file loader.** The floor has no tokenizer at all — its vocabulary
    /// is 1,024 derived ids and there is no file to commit to — so the rule cannot live in
    /// `decode_artifact_file_v1`, which serves both tiers. It is a property of the LANE, and this
    /// constructor is the lane's entrance: the chain-registered path every producer, panel and
    /// court replay of a dense class goes through.
    ///
    /// **A DERIVED artifact is exempt, and that exemption is not a loophole.** `is_derived()` is
    /// already load-bearing for exactly this distinction — "a derived artifact must never be
    /// reported as a registered class" — so a fixture minted from a seed carries no tokenizer by
    /// construction and no chain row can name it. Real weights are the only thing this refuses,
    /// which is the only thing that can be re-converted.
    pub fn check_tokenizer_declared_v1(artifact: &Base0ArtifactV1) -> Result<(), String> {
        if artifact.is_derived() || artifact.tokenizer_commitment != Hash64::default() {
            return Ok(());
        }
        Err(format!(
            "this artifact declares no tokenizer: `tokenizer_commitment` is all zeros, so every job this class \
             produces would publish `tokenizer_id` 0 and no replay could prove it read the ids this class means. \
             Re-convert it with a tokenizer bound — `qwen25-convert <checkpoint-dir> --a16 --out <path>`, where \
             <checkpoint-dir> holds tokenizer.json beside model.safetensors — and register the artifact_root the \
             re-conversion produces: binding a tokenizer moves `artifact_digest`, so this is a new artifact and a \
             genesis decision, not an upgrade of the one on disk (artifact digest {})",
            artifact.artifact_digest()
        ))
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
        Self::check_tokenizer_declared_v1(&artifact)?;
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
        let court_capable = a16_court_capable_v1(&profile);
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
            step_ladder_cap: kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
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
    /// **A folded retention, re-executed into the dense capture the court's assembly reads**
    /// (ADR-0082 Decision 7).
    ///
    /// The fold keeps no tiles, and `base0_refutation_from_capture_capped_v1` needs the whole leaf
    /// vector — one path per input row, one preimage per input leaf. Re-deriving exactly the
    /// leaves a refutation reads is ADR-0082 U-03's work (the dissection's own openings); until
    /// that lands, the party that wants to prosecute pays for ONE re-execution of a job it has the
    /// ids for, which is Decision 9's own sentence about a challenger ("a challenger is a seat that
    /// recomputed"). It is bounded by the ruleset's ladder, and the resulting binding must be the
    /// retention's own or this is not that execution.
    fn dense_capture_from_fold_v1(
        &self,
        material: &crate::produce::Base0FpMaterialV2,
    ) -> Result<crate::produce::Base0ExecutionV1, String> {
        let prompt: Vec<usize> = material.prompt_token_ids.iter().map(|t| *t as usize).collect();
        let run = a16_execute_for_attempt_streaming_capped_v1(
            &self.artifact,
            &material.binding.shape_profile,
            self.plan.as_ref(),
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

    /// The dense tiles either retention can answer with: the ones it kept, or the ones a
    /// re-execution reproduces.
    fn tiles_from_material_v1(&self, retention: &crate::produce::Base0RetentionV1) -> Result<crate::legs::Base0StepTilesV1, String> {
        match retention {
            crate::produce::Base0RetentionV1::Dense((binding, tiles, ..)) => {
                Ok(crate::legs::Base0StepTilesV1 { leaves: a16_leaves_by_position(binding, tiles), tiles: tiles.clone() })
            }
            crate::produce::Base0RetentionV1::Folded(material) => Ok(self.dense_capture_from_fold_v1(material)?.tiles),
        }
    }

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
        let checkpoint_chunks = retention.checkpoint_chunks().to_vec();
        if binding.step_leaf_count == 0 || binding.step_leaf_count > self.step_ladder_cap {
            return Err("the binding's leaf count is outside the ruleset's ladder".to_string());
        }
        let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, index)
            .ok_or_else(|| format!("leaf {index} is not a main step coordinate"))?;
        let step_tiles = self.tiles_from_material_v1(&retention)?;

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

        crate::legs::base0_refutation_from_capture_capped_v1(
            &binding.shape_profile.clone(),
            &binding.job_context.clone(),
            &step_tiles,
            binding,
            coord,
            prompt_token_ids,
            Some(pin),
            kv_checkpoint,
            self.step_ladder_cap,
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
            // **The tokenizer the ARTIFACT declares, not a constant zero.** The field was
            // `Hash64::default()` here, which is why "every job's `tokenizer_id` is zero" was true
            // of the dense tier even for an artifact that had bound one — the commitment reached
            // `artifact_digest` and stopped there. It is one line because there is only one place
            // the identity can come from: the artifact this backend is holding.
            //
            // For the artifact on disk today this is still zero, and that is the point — the value
            // now MOVES when the artifact is re-converted, so the job context stops claiming an
            // identity the class does not have. `check_tokenizer_declared_v1` is what stops a
            // chain-registered class from getting here with zeros at all.
            tokenizer_id: self.artifact.tokenizer_commitment,
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
            let cap = self.step_ladder_cap;
            let run = a16_execute_for_attempt_capped_v1(&self.artifact, &self.profile, self.plan.as_ref(), job, prompt, cap)?;
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

        let prompt_ids: Vec<u32> = prompt_tokens.iter().map(|t| *t as u32).collect();
        // **The one capture path this family has** (ADR-0049 Decision F's probe, the checkpoint
        // serializer at the class's declared width, and the selecting-rows retention all live in
        // it). The free-prompt lane differs from the attempt lane only in where its context and
        // its tokens come from, so the run itself must not be a second implementation.
        let run = a16_execute_free_prompt_streaming_v1(
            &self.artifact,
            &self.profile,
            self.plan.as_ref(),
            &ctx,
            prompt_tokens,
            self.step_ladder_cap,
            on_token,
        )?;

        // The four legs, measured — the derived roots the execution root is built from, which is
        // what `palw_fp_execution_root_v3` recomputes.
        let (checkpoint_leg_root, step_leg_root) = crate::legs::base0_leg_roots_from_binding_v1(&run.binding);
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
        // **The family codec first** — a captured attempt's material carries its binding, and the
        // seat check rebuilds the step root from the tiles, the checkpoint leg from the chunks,
        // and the tiled trace root from the retained rows. The legacy composite decode stays as
        // the fallback for the v1 class's claims, whose material is rows-and-ids only.
        // **The fold first** (ADR-0082 Decision 7): a free-prompt claim of this class retains v2,
        // and its step root is read off the retained tree rather than rebuilt from tiles there are
        // none of. Same three questions, same three answers.
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
        // A rung is a commitment to the execution PREFIX, so it is a fact about every leaf below
        // the index — which a fold answers by re-deriving them (`dense_capture_from_fold_v1`) and
        // a dense retention answers from what it kept.
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
    // Only for a class that can carry a capture at all. The v1 class declares the one-byte map
    // over an `i32` cache, so it commits no checkpoint leg and there is no interval to open —
    // `None`/refusal is the honest answer there, and it is the same fact `supports_court` reports.

    fn fp_interval_count(&self, capture: &[u8]) -> Option<u32> {
        let retention = crate::produce::base0_material_decode_any_v1(capture).ok()?;
        crate::fp_interval::Base0FpIntervalGeometryV1::from_binding_v1(retention.binding(), self.checkpoint_interval())
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
        // **Two retention forms, one opening, and the class's map decides whether the history travels.**
        // ADR-0082 Decision 7: a FOLDED retention kept no tiles, so the span's leaf hashes are
        // re-derived by replaying the interval from its checkpoint with this family's own kernels —
        // the loop a seat runs to check the opening. ADR-0082 Decision 9: a class that registers
        // the tiled map is served FLAT (the anchor's chunks stripped; the seat recomputes the
        // state), because the class's own declaration decides, not this executor's preference.
        let chunked =
            match crate::produce::base0_material_decode_any_v1(capture).map_err(|_| "the capture does not decode".to_string())? {
                crate::produce::Base0RetentionV1::Folded(material) => crate::fp_interval::base0_open_fp_interval_sparse_v1(
                    &material,
                    index,
                    prompt_token_ids,
                    self.checkpoint_interval(),
                    &A16IntervalKernels { artifact: &self.artifact, plan: self.plan.as_ref() },
                )
                .map_err(|e| e.to_string())?,
                crate::produce::Base0RetentionV1::Dense(material) => {
                    crate::fp_interval::base0_open_fp_interval_v1(&material, index, prompt_token_ids, self.checkpoint_interval())
                        .map_err(|e| e.to_string())?
                }
            };
        if crate::fp_interval::base0_fp_class_requires_flat_openings_v1(&self.profile) {
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
        // The state this seat recomputed for this interval, if it did (ADR-0082 Decision 9). With
        // one, the replay resumes from the seat's OWN bytes and a chunkless opening is evidence;
        // without one, this is ADR-0077 Decision 8 unchanged.
        let state = crate::fp_interval::base0_fp_interval_opening_seat_state_v1(opening, prompt_token_ids, self.checkpoint_interval());
        crate::fp_interval::base0_verify_fp_interval_opening_with_state_v1(
            opening,
            claim,
            index,
            prompt_token_ids,
            work_leaves,
            self.checkpoint_interval(),
            state.as_ref(),
            &A16IntervalKernels { artifact: &self.artifact, plan: self.plan.as_ref() },
        )
        .to_consensus_v1()
    }

    /// **ADR-0082 Decision 9.** The job's prefix, teacher-forced on this seat's own kernels, and
    /// the tiled root of the state it reaches — never a byte of the executor's history.
    ///
    /// The context is built exactly as [`Self::execute_free_prompt_streaming`] builds it, because
    /// it must be the one the honest producer ran under: a context that differed would name a
    /// different job, and the state kept for the row check that follows would be looked up under a
    /// key no opening produces (which reads as `Unverifiable`, never as an accusation).
    fn fp_recompute_checkpoint_root(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_token_ids: &[u32],
        output_token_ids: &[u32],
        decode_calls: u32,
    ) -> Result<Hash64, String> {
        use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3, palw_fp_job_context_v3};
        self.artifact_read_probe_v1()?;
        if !self.court_capable {
            return Err("the v1 class commits no checkpoint leg, so it has no state root to recompute".to_string());
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
            stop_reason: kaspa_consensus_core::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: Hash64::default(),
            step_leaf_count: 0,
        };
        let ctx = palw_fp_job_context_v3(job, &class, &shape, &self.network_id).map_err(|e| format!("{e:?}"))?;
        let mut kernels =
            crate::fp_recompute::A16RecomputeKernelsV1::new(&self.artifact, self.plan.as_ref()).map_err(|e| e.to_string())?;
        crate::fp_recompute::base0_fp_seat_state_memoized_v1(
            &self.profile,
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
        let mut run =
            a16_execute_for_attempt_capped_v1(&self.artifact, &self.profile, self.plan.as_ref(), job, prompt, self.step_ladder_cap)?;
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
        let binding = crate::legs::base0_binding_from_capture_with_profile_capped_v1(
            &self.profile,
            job,
            &run.tiles,
            &run.checkpoints,
            &kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(
                kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            ),
            run.trace_root,
            crate::produce::base0_activation_leg_root_v1(job),
            self.step_ladder_cap,
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
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
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
        let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2))
            .expect("the fixture's declaration is this engine's program");
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

    /// **The v1 graph's undeclared requant: refused where the two authorities are, executed as
    /// DECLARED where there is only one.**
    ///
    /// This test used to assert one thing: that `::new` refused a v1-graph class by name. ADR-0049
    /// Decision F requires a class's profile to name every narrowing its engine performs, and the
    /// v1 pre table declares the embedding gather while `A16Engine::forward_token_traced` also
    /// performs the requant that lifts it onto the A16 stream. Two authorities, and the probe was
    /// what compared them.
    ///
    /// Since ADR-0082 audit E's H-1 the constructor compiles `plan_from_profile`, so on that route
    /// there is no second authority left to disagree with: the declaration IS the program, the
    /// undeclared requant is simply not performed, and the v1 graph executes exactly what it says.
    /// That is not a softened refusal — it is the refusal becoming unnecessary, and it is what
    /// `from_registered_profile` has always done with the same profile, so the two constructors
    /// now agree instead of one of them being the lenient one.
    ///
    /// What is still pinned, and pinned here:
    /// * the plan-LESS route — `a16_execute_for_attempt_v1(.., None, ..)`, where the two
    ///   authorities still exist — refuses by name, and names the missing requant;
    /// * the v1 MAP is refused whichever route runs, because a one-byte map cannot describe an
    ///   `i32` cache (the sibling test below).
    #[test]
    fn a16_refuses_the_free_prompt_path_until_its_graph_is_reconciled() {
        for map_id in [map::integer_kv_state_chunk_map_id_v2(), map::integer_kv_state_chunk_map_id_v1()] {
            let (artifact, profile) = class(map_id);
            let prompt: Vec<usize> = vec![3, 9, 17, 33];

            // The route with two authorities still compares them, and still names the gap.
            let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prompt.len() as u32, 3);
            let error = a16_execute_for_attempt_v1(&artifact, &profile, None, &ctx, &prompt)
                .err()
                .expect("a graph that does not name what the traced engine computes must not commit a step leg");
            assert!(error.contains("registered graph"), "the refusal names the gap: {error}");
            assert!(error.contains("requant"), "and the node it is missing: {error}");

            // The route with ONE authority executes the declaration — the requant the profile does
            // not name is not performed, so there is nothing left to disagree about.
            let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2))
                .expect("the v1 declaration is a graph this build serves — a worse model, not a mis-executed one");
            let job = job(&profile, prompt.len() as u32, 3);
            match (map_id == map::integer_kv_state_chunk_map_id_v1(), backend.execute_free_prompt(&job, &prompt)) {
                (true, Ok(_)) => panic!("a one-byte map cannot describe an i32 cache, and committing anyway is the defect"),
                (true, Err(_)) => {}
                (false, Ok(run)) => assert!(run.facts.step_leg_root != Hash64::default(), "an executed declaration commits a leg"),
                (false, Err(e)) => panic!("the declaration this backend compiled is the one it must be able to run: {e}"),
            }
        }
    }

    /// **The executor prices the job against the RULESET's ladder, not a module literal.**
    ///
    /// This is W1b's whole claim. `a16_execute_for_attempt_streaming_v1` derived its leaf count
    /// with `step_leaf_count`, which hardcodes `PALW_STEP_MAX_LEAVES` and never reads
    /// `PalwCourtParamsV2::max_step_leaf_count` — so the number of tokens a user got was a
    /// build-time fact, a class registered against a deeper frozen ladder was refused by its own
    /// executor, and every width ADR-0080's split close buys the court was worth nothing here.
    ///
    /// Stated as a boundary rather than as a ratio because the boundary is what the ladder IS: one
    /// leaf short of this job's own price the executor refuses it, and at exactly its price it
    /// runs — same artifact, same graph, same job, one number of ruleset apart. (The size of the
    /// move is pinned where the arithmetic lives:
    /// `palw_step::tests::the_decode_budget_is_the_rulesets_ladder_not_the_context` measures a
    /// 26-token prefill at 12 decode tokens under `2^22` and 11 120 under `2^32`.)
    #[test]
    fn the_executor_prices_the_job_against_the_rulesets_ladder() {
        let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
        let anchor = Hash64::from_u64_word(0xA16C0117);
        let backend = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
            .expect("the fixture's declaration is this engine's program");
        let (ctx, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let price = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&profile, &ctx, u64::MAX)
            .expect("the canonical job has a step space");
        assert!(price > 1, "a one-leaf job cannot demonstrate a boundary");

        // A ruleset one leaf short of this job's price refuses it — in the executor, and named.
        let tight = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
            .expect("the fixture's declaration is this engine's program")
            .with_step_ladder_cap(price - 1);
        assert_eq!(tight.step_ladder_cap(), price - 1);
        let err = match tight.execute(&ctx, &prompt) {
            Err(e) => e,
            Ok(_) => panic!("a job past the ruleset's ladder is not executable"),
        };
        assert!(err.contains("TooManyLeaves"), "the refusal must name the ladder, got: {err}");
        assert!(err.contains(&format!("{}", price - 1)), "and the ladder it was measured against, got: {err}");

        // The same job, the same geometry, one more leaf of ladder — and it runs, committing the
        // price the ruleset admitted.
        let exact = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 3))
            .expect("the fixture's declaration is this engine's program")
            .with_step_ladder_cap(price);
        let Ok(outcome) = exact.execute(&ctx, &prompt) else {
            panic!("the ruleset that admits this job's price must execute it");
        };
        let (binding, ..) = crate::produce::base0_material_decode_v1(&outcome.material).expect("the capture decodes");
        assert_eq!(binding.step_leaf_count, price, "the binding commits the price the ruleset admitted");
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
        let backend = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
            .expect("the fixture's declaration is this engine's program");
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
        let compiled = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
            .expect("the fixture's declaration is this engine's program");
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
        let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2))
            .expect("the fixture's declaration is this engine's program");
        let prompt: Vec<usize> = vec![3, 9, 17, 33];
        let job = job(&profile, prompt.len() as u32, 3);

        let error = match backend.execute_free_prompt(&job, &prompt) {
            Err(e) => e,
            Ok(_) => panic!("a one-byte map cannot describe an i32 cache, and committing anyway is the defect"),
        };
        // **Which gate fires has moved, and the message with it.** The Decision-F probe used to
        // refuse this class before the map was ever consulted; a planned backend executes its
        // declaration, so the MAP is now what stops it — at the checkpoint it cannot serialise,
        // rather than at the class's door. The refusal is still a refusal, and nothing is
        // committed. That it is `CheckpointStateUnavailable` and not a NAMED map refusal is
        // recorded here rather than papered over: the predicate `a16_court_capable_v1` already
        // spells "can this map describe this cache", and nothing calls it as a door.
        assert!(
            error.contains("CheckpointStateUnavailable") || error.contains("state map") || error.contains("registered graph"),
            "the error names the defect it hit first: {error}"
        );
    }

    /// **The registered row, through the constructor a dense-tier demonstration actually uses.**
    ///
    /// Every other test in this module builds its class with `rms_eps_q: 1` in BOTH halves and so
    /// agrees with itself. The shipped ledger does not: `misaka-palw-base0::classes` gives this
    /// family a profile from `QWEN25_1_5B` (`rms_eps_q: 1`) and an `artifact_shape` at the
    /// converter's `eps_q: 1 << 8` — "the A16 engine norms at the shipped 1 << 8" — and
    /// `every_canonical_class_agrees_with_its_own_profile` EXEMPTS `ConvertedA16` from the
    /// equality that would have caught it. So a class whose artifact executes what
    /// `qwen25-convert` writes is refused by its own declaration:
    /// `GeometryMismatch { what: "rms_eps_q", profile: 1, artifact: 256 }`.
    ///
    /// It went unseen because the shipped worker took [`Qwen25A16Backend::new`], which compiled no
    /// plan and let the artifact's epsilon execute, while the ADR-0080 ladder row, the SDK and any
    /// chain-registered class go through [`Qwen25A16Backend::from_registered_profile`]. That
    /// asymmetry is closed — since ADR-0082 audit E's H-1 `::new` compiles the same plan and
    /// refuses the same mismatch — and this test keeps driving the registered constructor over an
    /// artifact built at the converter's epsilon, which is where the split was first made visible.
    ///
    /// Both directions are pinned: the corrected projection PLANS, and a declaration carrying the
    /// frozen `rms_eps_q: 1` still refuses on exactly that field and no other — the dense twin of
    /// `qwen36_plan`'s "a rms_eps_q 17 declaration refuses against the artifact's 1".
    #[test]
    fn a_row_served_over_a_converter_built_artifact_plans_and_the_frozen_declaration_refuses() {
        use crate::engine_a16::A16PlanErrorV1;
        use kaspa_consensus_core::palw_qwen25_profile::{
            QWEN25_A16_ARTIFACT_EPS_Q, qwen25_a16_artifact_row_profile_v1, qwen25_geometry_artifact_eps,
        };

        // The ledger's own pairing, at a size a unit test can hold: the profile from the geometry,
        // the artifact at the epsilon `qwen25-convert` writes.
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
        let shape = Base0ShapeV1 {
            n_layers: geometry.layer_count as usize,
            n_heads: geometry.attn_heads as usize,
            n_kv_heads: geometry.attn_kv_heads as usize,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            // What the converter writes. `classes.rs` spells this same constant for every
            // `ConvertedA16` row's `artifact_shape`.
            eps_q: QWEN25_A16_ARTIFACT_EPS_Q,
        };
        assert_eq!(shape.eps_q, 1 << 8, "the artifact half of the shipped pairing");
        let artifact = std::sync::Arc::new(
            Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
                .expect("a valid shape")
                .with_a16_params(derived_a16_store(&shape))
                .expect("the derived store is sorted and unique"),
        );

        // The frozen declaration: refused, on rms_eps_q and nothing else.
        let frozen = qwen25_a16_profile_v2(geometry).expect("the frozen projection is a valid profile");
        let engine = crate::engine_a16::A16Engine::new(&artifact).expect("the artifact is an A16 class");
        match engine.plan_from_profile(&frozen) {
            Err(A16PlanErrorV1::GeometryMismatch { what: "rms_eps_q", profile, artifact: got }) => {
                assert_eq!((profile, got), (1, 1 << 8), "the two epsilons, named");
            }
            other => panic!("a declaration the artifact does not execute must refuse on rms_eps_q and nothing else, got {other:?}"),
        }
        Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), frozen, (4, 3))
            .map(drop)
            .expect_err("and the backend a demonstration uses must refuse it too");

        // The corrected row: plans, and runs.
        let served = qwen25_a16_artifact_row_profile_v1(geometry).expect("the corrected projection is a valid profile");
        assert_eq!(
            qwen25_geometry_artifact_eps(geometry).rms_eps_q,
            QWEN25_A16_ARTIFACT_EPS_Q,
            "the correction is the epsilon and nothing else"
        );
        // The constructor FIRST, so the failure this pins is the refusal itself and not a
        // comparison standing in for it.
        let backend = Qwen25A16Backend::from_registered_profile(artifact, NETWORK.to_vec(), served.clone(), (4, 3))
            .expect("the row the artifact executes must be servable through the registered-profile constructor");
        assert_eq!(served.base0_rms_eps_q, shape.eps_q, "declared is executed");
        assert!(backend.supports_court(), "the corrected row still takes a court's turn");
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0xE95)).expect("the anchor implies a job");
        let outcome = backend.execute(&job, &prompt).expect("and the planned walk runs over the artifact's own epsilon");
        assert_eq!(
            backend.verify_material(
                &outcome.material,
                PalwClaimRootsV1 {
                    execution_root: outcome.execution_root,
                    trace_root: outcome.trace_root,
                    anchor: Hash64::from_u64_word(0xE95),
                }
            ),
            kaspa_consensus_core::palw_backend::PalwMaterialVerdictV1::Matches
        );
    }

    /// **The fold and the dense capture are ONE commitment** (ADR-0082 Decision 7).
    ///
    /// This is the test that makes Decision 7 safe to ship: the free-prompt lane stopped keeping
    /// its tiles, and the only thing that must not move is what the job COMMITS. Both sinks run
    /// the same loop over the same rows, and every root the chain sees — execution, trace, output,
    /// manifest, and the step leg's own root and leaf count inside the binding — is compared.
    ///
    /// Root equality is also what pins the fold's ENUMERATION. The step tree's leaves are
    /// `step_merkle_leaf_v1(index, hash)`, so a leaf placed at a different index is a different
    /// tree: the cursor the fold walks (call-major, position-major, slot-major, tile-major) agrees
    /// with `canonical_step_leaf_index` for every leaf of this job, or this assertion fails.
    #[test]
    fn the_folded_capture_commits_the_dense_captures_roots() {
        let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
        let backend = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
            .expect("the fixture's declaration is this engine's program");
        let (ctx, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0xF01D)).expect("the anchor implies a job");
        let cap = kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;

        let dense = a16_execute_for_attempt_streaming_capped_v1(&artifact, &profile, None, &ctx, &prompt, cap, &mut |_| {})
            .expect("the dense sink runs the job");
        let folded = a16_execute_free_prompt_streaming_v1(&artifact, &profile, None, &ctx, &prompt, cap, &mut |_| {})
            .expect("the folded sink runs the job");

        assert_eq!(dense.binding, folded.binding, "the two sinks commit the same binding, field for field");
        assert_eq!(dense.execution_root, folded.execution_root);
        assert_eq!(dense.trace_root, folded.trace_root);
        assert_eq!(dense.output_root, folded.output_root);
        assert_eq!(dense.trace_manifest_root, folded.trace_manifest_root);
        assert_eq!(dense.generated_token_ids, folded.generated_token_ids, "one execution, one answer");
        assert_eq!(dense.checkpoints.merkle_root, folded.checkpoints.merkle_root);

        // And the retention is the thing that changed: no tiles, one node per 2^retain_level leaves.
        let tree = folded.step_tree.as_ref().expect("a folded run keeps its tree");
        assert!(folded.tiles.tiles.is_empty() && folded.tiles.leaves.is_empty(), "the fold keeps no tiles");
        assert_eq!(tree.leaf_count(), dense.tiles.leaves.len() as u64);
        assert_eq!(tree.root().expect("the tree is its own shape"), dense.binding.step_merkle_root);
        let level = crate::fp_capture::palw_base0_sparse_retain_level_v1(cap);
        assert_eq!(tree.retain_level(), level, "the level is the ruleset ladder's derivation, not a constant");
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let folded_bytes = crate::produce::base0_fp_material_encode_v2(&folded, &ids).expect("the fold retains").len();
        let dense_bytes = crate::produce::base0_material_encode_v1(&dense).expect("the dense sink retains").len();
        assert!(
            folded_bytes < dense_bytes,
            "the fold must retain less than the tiles it replaced ({folded_bytes} against {dense_bytes})"
        );
        eprintln!(
            "Decision 7 on the A16 fixture: {} leaves, retention {folded_bytes} bytes folded against {dense_bytes} dense ({:.1}x)",
            tree.leaf_count(),
            dense_bytes as f64 / folded_bytes.max(1) as f64
        );
    }

    /// **An opening re-derived by replay IS the opening the dense capture would have served** —
    /// byte for byte, at every interval index of a multi-interval job (ADR-0082 Z6's second half).
    ///
    /// The executor kept no tiles, so the span's leaf hashes come from running the interval again
    /// from its checkpoint. If that replay were a different execution — a different seed id, a
    /// different resume point, a different order — the bytes would differ here, and a seat would
    /// have read the difference as this producer's fault.
    #[test]
    fn a_replayed_interval_opening_is_the_dense_ones() {
        let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
        let backend = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
            .expect("the fixture's declaration is this engine's program");
        let (ctx, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0BEE)).expect("the anchor implies a job");
        let cap = kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();

        let dense = a16_execute_for_attempt_streaming_capped_v1(&artifact, &profile, None, &ctx, &prompt, cap, &mut |_| {})
            .expect("the dense sink runs the job");
        let folded = a16_execute_free_prompt_streaming_v1(&artifact, &profile, None, &ctx, &prompt, cap, &mut |_| {})
            .expect("the folded sink runs the job");
        let dense_material: crate::produce::Base0RetainedMaterialV1 = (
            dense.binding.clone(),
            dense.tiles.tiles.clone(),
            dense.logits_rows.clone(),
            dense.generated_token_ids.clone(),
            dense.checkpoints.chunks.clone(),
        );
        let folded_bytes = crate::produce::base0_fp_material_encode_v2(&folded, &ids).expect("the fold retains");
        let folded_material = crate::produce::base0_fp_material_decode_v2(&folded_bytes).expect("its own retention decodes");

        let interval = kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1;
        let count = crate::fp_interval::Base0FpIntervalGeometryV1::from_binding_v1(&dense.binding, interval)
            .expect("the capture has an interval geometry")
            .interval_count;
        assert!(count >= 2, "a one-interval job cannot show that a replayed anchor is the committed one");
        let claim = PalwClaimRootsV1 { execution_root: dense.execution_root, trace_root: dense.trace_root, anchor: ctx.job_id };
        for index in 0..count {
            let from_tiles = crate::fp_interval::base0_open_fp_interval_v1(&dense_material, index, &ids, interval)
                .unwrap_or_else(|e| panic!("interval {index} opens from the tiles: {e}"));
            let from_replay = crate::fp_interval::base0_open_fp_interval_sparse_v1(
                &folded_material,
                index,
                &ids,
                interval,
                &A16IntervalKernels { artifact: &artifact, plan: None },
            )
            .unwrap_or_else(|e| panic!("interval {index} opens from the fold: {e}"));
            assert_eq!(from_replay, from_tiles, "interval {index}: the replayed opening is not the retained one");
            assert_eq!(
                backend.verify_fp_interval_opening(&from_replay, claim, index, &ids, dense.binding.step_leaf_count),
                kaspa_consensus_core::palw_backend::PalwFpIntervalVerdictV1::Valid,
                "interval {index}: a seat must license the replayed opening"
            );
        }
    }

    /// **Z6: what the fold retains is a fixed fraction of the leaf count — of nothing else.**
    ///
    /// Two jobs of different length on one class: the retained vector is `⌈leaves / 2^level⌉`
    /// nodes in both, so the retention per leaf is the same number whatever the context, and the
    /// only thing that moves it is the ladder the level is derived from. The dense capture's own
    /// retention is printed beside it, because the ratio is what ADR-0082 §1.5 is about.
    ///
    /// What this does NOT claim: that the whole retained MATERIAL is flat per position. It is not,
    /// and the reason is named — the checkpoint chunks hold the cache at every checkpoint
    /// (Decision 4's prefix-stability is not implemented here), and on a graph-v3 class the leaves
    /// per position themselves grow with `kv_len` (Decision 1). Both are other units' work, and a
    /// test that asserted flatness today would be asserting something this tree does not do.
    #[test]
    fn the_folds_retention_is_a_fixed_fraction_of_the_leaf_count() {
        let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
        let cap = kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;
        let level = crate::fp_capture::palw_base0_sparse_retain_level_v1(cap);
        let mut seen = Vec::new();
        for (prefill, decode) in [(4u32, 3u32), (9, 6)] {
            let ctx = {
                let mut ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, decode);
                ctx.job_id = Hash64::from_u64_word(0x2626 + prefill as u64);
                let prompt: Vec<u32> = (0..prefill).map(|i| i % profile.vocab_size).collect();
                ctx.prompt_token_ids_hash = kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&prompt);
                ctx
            };
            let prompt: Vec<usize> = (0..prefill as usize).map(|i| i % profile.vocab_size as usize).collect();
            let folded = a16_execute_free_prompt_streaming_v1(&artifact, &profile, None, &ctx, &prompt, cap, &mut |_| {})
                .expect("the folded sink runs the job");
            let tree = folded.step_tree.as_ref().expect("a folded run keeps its tree");
            let leaves = tree.leaf_count();
            let nodes = tree.retained_nodes().len() as u64;
            assert_eq!(nodes, leaves.div_ceil(1 << level), "the retained vector is the leaf count over the block, and nothing else");
            seen.push((leaves, nodes));
        }
        assert!(seen[0].0 != seen[1].0, "two jobs of the same length prove nothing about the context");
        // 64 bytes a node, and the budget the level was derived from bounds it — on both jobs.
        for (leaves, nodes) in seen {
            assert!(
                nodes * 64 <= crate::fp_capture::PALW_BASE0_SPARSE_RETAIN_BUDGET_BYTES,
                "{leaves} leaves retained {} bytes, past the budget the level derives from",
                nodes * 64
            );
        }
    }

    // -- W1b: the EXECUTOR's own ladder -------------------------------------------------------

    /// **The decode budget a fixed prefill buys is the RULESET's number, and it was a constant.**
    ///
    /// `a16_execute_for_attempt_streaming_v1` priced every job through `step_leaf_count`, which
    /// hardcodes `PALW_STEP_MAX_LEAVES = 2^22`. So every width the court gained at ADR-0080's
    /// close ceiling — dense A16 admitted out to `n_ctx` 39 against the shipped ladder and 1002
    /// against `2^32` — bought a user nothing: the executor refused the job, or truncated its
    /// budget, before the first forward pass.
    ///
    /// Measured here on the SHIPPED dense geometry (`QWEN25_1_5B`, `tile_len` 128) through the
    /// very call the capture path makes. The two prefills are the ones the launch brief measured
    /// on the real artifact, and the leaf counts are why: 26 prefill tokens leave room for 12
    /// decode tokens and 4,074,040 of the 4,194,304 leaves. **38 total positions is what the
    /// shipped committed path admits — a prompt, and a sentence of answer.**
    ///
    /// At `2^32` the same prefill buys 4,070, and the number that bounds it is no longer the
    /// ladder at all: it is the declared context (4,096 − 26). That is the whole point of the
    /// thread — the ladder stops being what decides what a user gets.
    #[test]
    fn the_decode_budget_a_fixed_prefill_buys_moves_with_the_ruleset() {
        use kaspa_consensus_core::palw_step::step_leaf_count_capped_v1;

        let geometry = PalwQwen25GeometryV1 { n_ctx: 4_096, ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B };
        let profile = qwen25_a16_profile_v2(geometry).expect("the dense class's registered graph");

        // The widest decode budget a ladder admits at this prefill: the job's own leaf count,
        // counted the way the executor counts it, walked up until it refuses.
        let widest = |prefill: u32, cap: u64| -> (u32, u64) {
            let mut best = (0u32, 0u64);
            for decode in 1..=(geometry.n_ctx - prefill) {
                let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, decode);
                match step_leaf_count_capped_v1(&profile, &ctx, cap) {
                    Ok(leaves) => best = (decode, leaves),
                    Err(_) => break,
                }
            }
            best
        };

        const SHIPPED: u64 = kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;
        const DEEP: u64 = 1 << 32;
        assert_eq!(SHIPPED, 4_194_304, "the shipped ladder moved without a fence");

        assert_eq!(widest(26, SHIPPED), (12, 4_074_040), "the shipped ladder's budget at prefill 26");
        assert_eq!(widest(24, SHIPPED), (14, 4_112_072), "the shipped ladder's budget at prefill 24");

        let (deep_26, _) = widest(26, DEEP);
        let (deep_24, _) = widest(24, DEEP);
        assert_eq!((deep_26, deep_24), (4_070, 4_072), "at 2^32 the DECLARED CONTEXT binds, not the ladder");
        assert!(deep_26 > 300 * 12, "the budget must move by orders of magnitude, not by rounding: {deep_26}");
    }

    /// **The executor reads the ladder it is HANDED — the whole of W1b in one assertion.**
    ///
    /// The capped entry point is given one leaf less than the job needs and must refuse; given
    /// exactly what the job needs it must run. Before this change the argument did not exist and
    /// the module constant decided both answers, so the first half of this test passed a job the
    /// caller's ruleset had no room for.
    ///
    /// It is stated as `TooManyLeaves` on the way in rather than a failure three layers down: the
    /// leaf count is the first line of the capture, and the 268 MB the leaf vector costs at the
    /// shipped ladder is exactly what must not be allocated for a job that cannot be committed.
    #[test]
    fn the_a16_executor_prices_its_capture_at_the_ladder_the_caller_states() {
        let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
        let prompt: Vec<usize> = vec![3, 9, 17, 33];
        let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prompt.len() as u32, 3);
        let needed = kaspa_consensus_core::palw_step::step_leaf_count(&profile, &ctx).expect("the job has a step space");
        assert!(needed > 1, "a one-leaf job could not tell the two ladders apart");

        // A ladder one leaf short of this job. **Refused, and named.**
        let refused = a16_execute_for_attempt_capped_v1(&artifact, &profile, None, &ctx, &prompt, needed - 1);
        let message = refused.err().expect("a job the caller's ladder has no room for must be refused");
        assert!(message.contains("TooManyLeaves"), "the refusal must name the ladder, not something downstream: {message}");

        // The same job, at the ladder it actually needs. **Runs, and commits.**
        let run = a16_execute_for_attempt_capped_v1(&artifact, &profile, None, &ctx, &prompt, needed)
            .expect("the same job runs under a ladder that admits it");
        assert_eq!(run.binding.step_leaf_count, needed);
        assert_ne!(run.execution_root, Hash64::default());

        // **And the budget the executor ACTUALLY grants is the arithmetic one, at two different
        // ladders.** This is the half `the_decode_budget_a_fixed_prefill_buys_moves_with_the_ruleset`
        // computes and cannot run: there, the class is the real 1.5B geometry and a forward pass
        // needs the artifact; here the class is small enough to execute, so the two ladders can be
        // walked end to end. A budget that came from a constant would grant the same number twice.
        let prefill = prompt.len() as u32;
        let arithmetic_budget = |cap: u64| -> u32 {
            let mut best = 0;
            for decode in 1..=8u32 {
                let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, decode);
                match kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&profile, &ctx, cap) {
                    Ok(_) => best = decode,
                    Err(_) => break,
                }
            }
            best
        };
        let leaves_for = |decode: u32| -> u64 {
            let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, decode);
            kaspa_consensus_core::palw_step::step_leaf_count(&profile, &ctx).expect("the job has a step space")
        };
        let (shallow, deep) = (leaves_for(2), leaves_for(5));
        assert!(shallow < deep, "two ladders that admit the same budget prove nothing");
        assert_eq!((arithmetic_budget(shallow), arithmetic_budget(deep)), (2, 5));

        for (cap, budget) in [(shallow, 2u32), (deep, 5u32)] {
            let admitted = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, budget);
            assert!(
                a16_execute_for_attempt_capped_v1(&artifact, &profile, None, &admitted, &prompt, cap).is_ok(),
                "the executor must grant the whole budget its ladder pays for ({budget} decode tokens at cap {cap})"
            );
            let one_more = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, budget + 1);
            assert!(
                a16_execute_for_attempt_capped_v1(&artifact, &profile, None, &one_more, &prompt, cap).is_err(),
                "the executor must stop exactly where its ladder does ({} decode tokens at cap {cap})",
                budget + 1
            );
        }
    }

    /// **The dormant default is byte-identical, proven by measurement rather than by argument.**
    ///
    /// Every shipped construction site passes the leg's own constant, so nothing on any live
    /// network moves: the uncapped names delegate with `PALW_STEP_LEG_MAX_LEAVES` and must return
    /// the same commitment, leaf for leaf and root for root, as the capped name given that value.
    /// The whole retained material is compared, not just the execution root — a capture that
    /// agreed on the root and differed in a tile would be a producer that cannot answer a
    /// challenge it has already been convicted by.
    #[test]
    fn the_uncapped_executor_is_the_capped_one_at_the_shipped_ladder() {
        let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
        let prompt: Vec<usize> = vec![3, 9, 17, 33];
        let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prompt.len() as u32, 3);

        let plain = a16_execute_for_attempt_v1(&artifact, &profile, None, &ctx, &prompt).expect("the default path runs");
        let capped = a16_execute_for_attempt_capped_v1(
            &artifact,
            &profile,
            None,
            &ctx,
            &prompt,
            kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
        )
        .expect("the capped path at the default runs");

        assert_eq!(plain.execution_root, capped.execution_root);
        assert_eq!(plain.trace_root, capped.trace_root);
        assert_eq!(plain.output_root, capped.output_root);
        assert_eq!(plain.trace_manifest_root, capped.trace_manifest_root);
        assert_eq!(
            crate::produce::base0_material_encode_v1(&plain).expect("encodes"),
            crate::produce::base0_material_encode_v1(&capped).expect("encodes"),
            "the retained material must be the same bytes, or the default was not dormant"
        );

        // And the backend's own default is that same constant, so an instance nobody told about a
        // ruleset behaves exactly as it did.
        let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile, (4, 2))
            .expect("the fixture's declaration is this engine's program");
        assert_eq!(backend.step_ladder_cap(), kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES);
    }

    /// **An instance TOLD a shallower ladder refuses the job its default would have run** — the
    /// field is wired to the executor, not merely stored beside it.
    ///
    /// `with_step_ladder_cap` is the seam a caller holding `PalwCourtParamsV2` uses. It already
    /// bounded the served-capture guards (`bisect_prefix_state`, `refutation_with_prompt`); it did
    /// not reach the run, which is the half that decides what gets produced in the first place.
    #[test]
    fn an_instance_told_a_ladder_produces_against_it() {
        let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
        let prompt: Vec<usize> = vec![3, 9, 17, 33];
        let fp = job(&profile, prompt.len() as u32, 3);
        let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prompt.len() as u32, 3);
        let needed = kaspa_consensus_core::palw_step::step_leaf_count(&profile, &ctx).expect("the job has a step space");

        let default = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 2))
            .expect("the fixture's declaration is this engine's program");
        assert!(default.execute_free_prompt(&fp, &prompt).is_ok(), "the shipped ladder admits this job");

        let shallow = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile, (4, 2))
            .expect("the fixture's declaration is this engine's program")
            .with_step_ladder_cap(needed - 1);
        let message = shallow.execute_free_prompt(&fp, &prompt).err().expect("a shallower ruleset must refuse it");
        assert!(message.contains("TooManyLeaves"), "the refusal must name the ladder: {message}");
    }

    /// The geometry the ADR-0082 dense rows are built from here, at a size a unit test can hold.
    /// One geometry for both rows, so the v2 and v5 profiles differ ONLY where the graph does.
    fn v5_geometry() -> PalwQwen25GeometryV1 {
        PalwQwen25GeometryV1 {
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
        }
    }

    /// The artifact those rows are SERVED over: built at the epsilon `qwen25-convert` writes,
    /// which is the pairing `qwen25_a16_artifact_row_profile_v*` exists to project.
    fn v5_artifact(geometry: PalwQwen25GeometryV1) -> std::sync::Arc<Base0ArtifactV1> {
        let shape = Base0ShapeV1 {
            n_layers: geometry.layer_count as usize,
            n_heads: geometry.attn_heads as usize,
            n_kv_heads: geometry.attn_kv_heads as usize,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: kaspa_consensus_core::palw_qwen25_profile::QWEN25_A16_ARTIFACT_EPS_Q,
        };
        std::sync::Arc::new(
            Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
                .expect("a valid shape")
                .with_a16_params(derived_a16_store(&shape))
                .expect("the derived store is sorted and unique"),
        )
    }

    /// **A `graph-v5` row is court-capable, and it says so through the constructor the chain uses.**
    ///
    /// `court_capable` was `state_chunk_map_id == integer_kv_state_chunk_map_id_v2()`, written out
    /// in both constructors. ADR-0082 Decision 4 has the v5 row register the TILED map — the whole
    /// reason the dense tier's close is flat, because the dissection's bottom opens ONE history
    /// tile — so the registered row answered `supports_court() == false` over its own profile.
    /// That is not a cosmetic label: ADR-0069 Decision 5 admits a class that cannot take a court's
    /// turn WEIGHTLESS, so the row the genesis registers would have been admitted and earned
    /// nothing, and the failure is silent because a `false` here is a legal answer for the v1 row.
    ///
    /// Both directions are pinned. A v2-mapped row must STILL be court-capable — the predicate got
    /// wider, not moved — because that row is a live chain fact.
    #[test]
    fn the_graph_v5_row_is_court_capable_and_the_v2_row_it_replaces_still_is() {
        use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_artifact_row_profile_v1, qwen25_a16_artifact_row_profile_v5};

        let geometry = v5_geometry();
        let artifact = v5_artifact(geometry);

        let v5 = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
        assert!(
            map::palw_map_addresses_history_tiles_v1(&v5),
            "the row under test must be the tiled one, or this test is about the wrong class"
        );
        let v5_backend = Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), v5.clone(), (4, 2))
            .expect("the v5 row compiles a plan over the artifact's epsilon");
        assert!(v5_backend.supports_court(), "a tiled-map class takes a court's turn — ADR-0069 D5 weightlessness turns on this");

        let v2 = qwen25_a16_artifact_row_profile_v1(geometry).expect("the v2 projection is a valid profile");
        assert_eq!(v2.state_chunk_map_id, map::integer_kv_state_chunk_map_id_v2(), "the v2 row keeps its own map");
        let v2_backend = Qwen25A16Backend::from_registered_profile(artifact, NETWORK.to_vec(), v2.clone(), (4, 2))
            .expect("the v2 row still compiles a plan");
        assert!(v2_backend.supports_court(), "widening the predicate must not drop the row already registered");

        assert_ne!(v5.shape_profile_id(), v2.shape_profile_id(), "a class IS its graph: these are two classes");
    }

    /// **The v5 row plans over the converter's epsilon, and executes.**
    ///
    /// `GeometryMismatch { what: "rms_eps_q", profile: 1, artifact: 256 }` is the refusal that
    /// stopped EVERY dense row from being built from its own registered profile — declared 1,
    /// executed `1 << 8` — and it was found only because a test drove `from_registered_profile`
    /// instead of `::new`. `qwen25_a16_artifact_row_profile_v1` closed it for the v2 row; this
    /// pins that the v5 row inherits the closure rather than re-opening it, and that the plan it
    /// compiles actually runs a job rather than merely type-checking.
    #[test]
    fn the_graph_v5_row_plans_over_the_converters_epsilon_and_runs_a_job() {
        use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_artifact_row_profile_v5, qwen25_a16_profile_v5};

        let geometry = v5_geometry();
        let artifact = v5_artifact(geometry);

        // The DECLARED epsilon, unprojected: still refused, on that field and no other. Without
        // this half the test above would pass on a row whose epsilon nobody had corrected.
        let frozen = qwen25_a16_profile_v5(geometry).expect("the frozen v5 projection is a valid profile");
        let message = Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), frozen, (4, 2))
            .err()
            .expect("a row declaring an epsilon its artifact does not execute must refuse");
        assert!(message.contains("rms_eps_q"), "the refusal names the field: {message}");

        let profile = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
        let backend = Qwen25A16Backend::from_registered_profile(artifact, NETWORK.to_vec(), profile.clone(), (4, 2))
            .expect("the served v5 row compiles a plan");
        let (ctx, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0082_5)).expect("the anchor implies a job");
        let outcome = backend.execute(&ctx, &prompt).expect("the compiled v5 plan executes its own canonical job");
        let (binding, ..) = crate::produce::base0_material_decode_v1(&outcome.material).expect("the capture decodes");
        assert_eq!(binding.shape_profile.shape_profile_id(), profile.shape_profile_id(), "it produced for the class it was given");
        assert!(binding.step_leaf_count > 0, "an execution with no committed steps is not one");
    }

    /// **The shipped constructor runs a graph-v5 job — which is H-1 of ADR-0082's audit E.**
    ///
    /// `Qwen25A16Backend::new` is the constructor the SDK's dense lineage and the shipped
    /// free-prompt worker take for a class this build's own table names. It stored `plan: None`,
    /// so it executed through `forward_token_traced` — the compiled twenty-seven-row v2 program —
    /// and the moment the dense tier moves to ADR-0082's fused site the producer refused its own
    /// class by name (the refusal is pinned by the test below). Not a consensus break: a shipping
    /// break, and precisely the row this ADR exists to ship.
    ///
    /// The close is one authority rather than two: `new` compiles `plan_from_profile`, exactly as
    /// `from_registered_profile` does, so a declaration is executed as declared or refused with
    /// the node named. This test is the difference measured on the same fixture the refusal test
    /// uses — same artifact, same graph, one constructor apart.
    #[test]
    fn the_shipped_constructor_compiles_the_plan_and_runs_a_graph_v5_job() {
        use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5;

        let geometry = v5_geometry();
        let artifact = v5_artifact(geometry);
        let profile = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
        assert_eq!(profile.attn_nodes.len(), 24, "a v5 layer declares twenty-four nodes — the traced route records 27");

        let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2))
            .expect("the constructor compiles the declared graph instead of assuming a v2 program");
        let (ctx, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0082_01)).expect("the anchor implies a job");
        let outcome = backend.execute(&ctx, &prompt).expect("the shipped constructor executes its own class's fused graph");
        let (binding, ..) = crate::produce::base0_material_decode_v1(&outcome.material).expect("the capture decodes");
        assert_eq!(
            binding.shape_profile.shape_profile_id(),
            profile.shape_profile_id(),
            "it produced for the class it was given, not for a v2 stand-in"
        );
        assert!(binding.step_leaf_count > 0, "an execution with no committed steps is not one");
        assert!(backend.supports_court(), "and the tiled-map row it serves takes a court's turn");

        // A declaration this artifact cannot serve is now a REFUSAL rather than a silent
        // execution under the artifact's own constants: the frozen v5 row declares `rms_eps_q` 1
        // where the converter writes 256, and that is the asymmetry H-1 closed.
        let frozen = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v5(geometry).expect("a valid profile");
        let message = Qwen25A16Backend::new(v5_artifact(geometry), NETWORK.to_vec(), frozen, (4, 2))
            .err()
            .expect("a row declaring an epsilon its artifact does not execute must refuse here too");
        assert!(message.contains("rms_eps_q"), "the refusal names the field: {message}");
    }

    /// **The plan-less route is the v2 reference, and a graph-v5 row is refused there by name.**
    ///
    /// `A16Engine::forward_token_traced` is a COMPILED program — twenty-seven rows a layer, the
    /// attention site spelled as scores / softmax / requant / values — and ADR-0082 Decision 1
    /// replaces those four with one. So the traced route cannot serve a v5 row and must not
    /// pretend to: teaching it the fusion would put a second authority beside
    /// `plan_from_profile`, which is exactly what ADR-0067 Decision 2 merged. The Decision-F probe
    /// is what states the boundary, and this pins that it states it on the fused row rather than
    /// mis-executing it.
    #[test]
    fn the_plan_less_route_is_the_v2_reference_and_refuses_a_fused_row() {
        use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5;

        let geometry = v5_geometry();
        let artifact = v5_artifact(geometry);
        let profile = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
        let (ctx, prompt) =
            crate::produce::base0_rc_job_v1(&profile, Hash64::from_u64_word(0x0082_00), geometry.vocab_size as usize, 3, 2);

        let message = a16_execute_for_attempt_v1(&artifact, &profile, None, &ctx, &prompt)
            .err()
            .expect("the twenty-seven-row traced program cannot execute a twenty-four-node declaration");
        println!("plan-less refusal on a v5 row: {message}");
        assert!(message.contains("registered graph"), "the refusal names the boundary: {message}");
        assert!(message.contains("per-layer declares 24 against 27 recorded"), "and the count that disagrees: {message}");
    }

    /// **A class whose artifact declares no tokenizer is refused at the lane's entrance, by name.**
    ///
    /// The shipped `qwen25-1.5b-a16.palwart` carries 64 zero bytes there (read off the file at
    /// offset 1,777,209,032), so every job it produced published `tokenizer_id` 0 — a value that
    /// pins nothing, inside `PalwJobContextV2::context_hash`, which
    /// `PalwFreePromptJobV3::tokenizer_id` "MUST equal". A replay under a different `tokenizer.json`
    /// then computes different ids and a different context hash and reproduces nothing, and the
    /// producer is defaulted for work it performed correctly. `Undeclared` was reported and
    /// stepped over; here it is a refusal.
    ///
    /// The exemption is `is_derived()`, which is already the "never a registered class" flag, so
    /// every fixture in this module keeps working and only real weights are refused.
    #[test]
    fn an_artifact_that_declares_no_tokenizer_cannot_serve_a_registered_dense_class() {
        use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5;

        let geometry = v5_geometry();
        let derived = v5_artifact(geometry);
        assert_eq!(derived.tokenizer_commitment, Hash64::default(), "a derived fixture declares none, and may");
        let profile = qwen25_a16_artifact_row_profile_v5(geometry).expect("the v5 projection is a valid profile");
        assert!(
            Qwen25A16Backend::from_registered_profile(derived.clone(), NETWORK.to_vec(), profile.clone(), (4, 2)).is_ok(),
            "a derived artifact is a fixture, not a registered class"
        );

        // The same weights with the derived marker gone — the shape a CONVERTED artifact has, and
        // the shape the file on disk has.
        let converted = Base0ArtifactV1::from_parts(
            derived.shape,
            derived.embed.clone(),
            derived.unembed.clone(),
            derived.layers.clone(),
            derived.norm_requant,
            derived.residual_requant,
        )
        .expect("the parts of a valid artifact rebuild one")
        .with_a16_params(derived_a16_store(&derived.shape))
        .expect("the derived store is sorted and unique");
        assert!(!converted.is_derived(), "this is the converted-artifact case");

        let message = Qwen25A16Backend::from_registered_profile(
            std::sync::Arc::new(converted.clone()),
            NETWORK.to_vec(),
            profile.clone(),
            (4, 2),
        )
        .err()
        .expect("a converted artifact with a zero tokenizer commitment must be refused");
        assert!(message.contains("tokenizer_commitment"), "the refusal names the field: {message}");
        assert!(message.contains("qwen25-convert"), "and the command that fixes it: {message}");

        // Bound: it serves, and the job it builds publishes THAT identity rather than zero.
        let bound = std::sync::Arc::new(
            converted.with_tokenizer_commitment(Base0ArtifactV1::tokenizer_commitment_of(b"{\"model\":\"qwen2.5\"}")),
        );
        let backend = Qwen25A16Backend::from_registered_profile(bound.clone(), NETWORK.to_vec(), profile, (4, 2))
            .expect("an artifact that declares its tokenizer serves");
        let (ctx, _) = backend.job_for_anchor(Hash64::from_u64_word(0x70CE_9155)).expect("the anchor implies a job");
        assert_eq!(ctx.tokenizer_id, bound.tokenizer_commitment, "the job publishes the artifact's own tokenizer identity");
        assert_ne!(ctx.tokenizer_id, Hash64::default(), "and it is not the zero that pinned nothing");
    }
}
