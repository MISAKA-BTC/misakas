//! **An execution, turned into the roots an attempt carries** (ADR-0042, audit C-01).
//!
//! `palw_producer_v2` gives a producer the chain's half — the class target, the pwu, the keys. This
//! is the other half: the part that costs something. A `ConsensusV2` attempt commits four roots
//! over an execution, admission checks none of them, and a court checks all of them the moment
//! anyone opens a case. So they must be produced honestly by construction, because the only thing
//! that catches a dishonest one is a slash.
//!
//! # What a job is
//!
//! One prefill call over `declared_prefill_tokens` positions, then `exact_decode_tokens − 1`
//! decode calls of one position each — the enumeration `canonical_step_leaf_index` walks. The post
//! table (final norm, its narrowing, the logits head) has leaves only where logits exist: the last
//! prefill position and every decode position. A capture that pushed the head's row at every
//! prefill position would be describing steps this class's step space does not have, and the
//! profile refuses it rather than placing it somewhere.
//!
//! # The two legs an integer class cannot produce
//!
//! `full_logits_trace_root_v2` hashes rows of **f32** and refuses a non-finite value;
//! `PalwActivationTapProfileV1` requires a non-empty tap list of **f32** rows. BASE-0's logits are
//! `int32` accumulator lanes and it taps nothing — an `i32` above `2^24` does not survive the
//! conversion to f32, so committing converted floats would mean a producer's commitment and its
//! execution disagree for exactly the values a refutation would open.
//!
//! **This is a real gap in the leg schemes, not a shortcut taken here**: the v1/v2 legs were
//! written for float runtimes, and ADR-0039 then made an integer class the permanent liveness
//! floor. Rather than commit a lie in either slot, the integer class gets integer roots of its
//! own, domain-separated, in the same two slots of the same composite — which is exactly what
//! `base0_binding_from_capture_v1` was already shaped for, taking both as caller-supplied opaque
//! roots. A court that one day adjudicates those two legs has to know which scheme a class uses;
//! that is a class fact, and the class id is its graph.

use crate::artifact::Base0ArtifactV1;
use crate::engine::{Base0Engine, EngineError, KvCache, argmax_lowest};
use crate::legs::{Base0CapturedRowV1, Base0StepCaptureV1, Base0StepTilesV1, LegError, base0_captured_rows_v1};
use kaspa_consensus_core::palw_step::{PALW_STEP_MAX_LEAVES, PalwShapeProfileV3, PalwStepTableV1, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// Moved to the COURT's module (`kaspa_consensus_core::palw_step_refute`) with the byte string
/// unchanged, so the committing side and the adjudicating side are one implementation; re-exported
/// here for every existing caller.
pub use kaspa_consensus_core::palw_step_refute::{PALW_BASE0_DOMAIN_LOGITS_TRACE, base0_logits_trace_root_v1};

pub const PALW_BASE0_DOMAIN_ACTIVATION_LEG: &[u8] = b"misaka-palw/base0/activation-leg/v1";
pub const PALW_BASE0_DOMAIN_TRACE_MANIFEST: &[u8] = b"misaka-palw/base0/trace-manifest/v1";

/// Why an execution could not become an attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProduceError {
    Engine(EngineError),
    Leg(LegError),
    /// The job's step space could not be counted — the profile and the context disagree.
    StepSpace(kaspa_consensus_core::palw_step::PalwStepError),
    /// A job with no prefill has no first token, and a job with no decode produces no output.
    EmptyJob,
    /// The prompt is shorter than the prefill the context declares. The context is the commitment;
    /// a producer that ran a shorter prompt ran a different job from the one it committed to.
    PromptShorterThanPrefill {
        prompt: usize,
        declared: u32,
    },
    /// A prompt token outside the artifact's vocabulary.
    TokenOutOfVocab {
        token: usize,
        vocab: usize,
    },
    /// A codec failure with a fixed description — serialization is infallible for honest data, so
    /// hitting this on the encode side is a bug and on the decode side is a peer's garbage.
    Internal(&'static str),
    /// **The class's declared graph is not the graph this engine performs** (ADR-0049 Decision F).
    ///
    /// The profile arrives from the chain and the engine executes `BASE0_LAYER_IR`. If the two
    /// describe different computations, every leg this producer commits is a leg the court
    /// recomputes differently — so it is refused here rather than mined and convicted later.
    GraphMismatch(crate::plan::ProjectionMismatch),
}

impl std::fmt::Display for ProduceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Engine(e) => write!(f, "the engine refused the pass: {e:?}"),
            Self::GraphMismatch(e) => write!(f, "the class declares a graph this engine does not perform: {e}"),
            Self::Leg(e) => write!(f, "the capture could not become a leg: {e:?}"),
            Self::StepSpace(e) => write!(f, "the job has no step space: {e:?}"),
            Self::EmptyJob => write!(f, "a job needs at least one prefill token and one decode token"),
            Self::PromptShorterThanPrefill { prompt, declared } => {
                write!(f, "the prompt has {prompt} tokens and the context declares {declared} — a different job")
            }
            Self::TokenOutOfVocab { token, vocab } => write!(f, "token {token} is outside a vocabulary of {vocab}"),
            Self::Internal(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for ProduceError {}

/// **The integer class's activation leg: the statement that it taps nothing.**
///
/// Not `Hash64::default()`, which is indistinguishable from a field nobody set — the difference
/// between "this class declares no taps" and "somebody forgot" is the difference between a
/// commitment and an omission, and only one of them can be argued about later.
pub fn base0_activation_leg_root_v1(ctx: &PalwJobContextV2) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_BASE0_DOMAIN_ACTIVATION_LEG).to_state();
    h.update(ctx.context_hash().as_byte_slice());
    h.update(&(ctx.declared_prefill_tokens as u64).to_le_bytes());
    h.update(&(ctx.exact_decode_tokens as u64).to_le_bytes());
    h.update(b"no-taps");
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Re-exported, not re-typed. The derivation moved to `kaspa_consensus_core::palw_attempt_v2`;
/// two spellings of one domain key is how the anchor quietly moves for half the network.
pub use kaspa_consensus_core::palw_attempt_v2::PALW_DOMAIN_JOB_ANCHOR_V1 as PALW_BASE0_DOMAIN_JOB_ANCHOR;
pub const PALW_BASE0_DOMAIN_JOB_PROMPT: &[u8] = b"misaka-palw/base0/rc-job-prompt/v1";

/// **What the RC's job is a function of — and what it deliberately is NOT.**
///
/// A producer must not choose its own prompt: a class whose executor picks the input is a class
/// where "run the model" and "find an input whose output I like" are the same move. So the job is
/// derived, and the only question is from what.
///
/// It is derived from the **template**: `(network domain, pre-pow hash, class, bond)`. Not from
/// the challenge, which also binds the timestamp and the NONCE — and that difference is the whole
/// economics of the lane. `l1_tag_v2` is `Expand(commitment_root)`, a free CPU hash, precisely so
/// the Layer-0 nonce search stays a nonce search; a job that moved with the nonce would price one
/// full inference per PoW try and no producer could keep up. What limits a bond is the exposure
/// ceiling and the epoch budget, which is where ADR-0042 put the limit when it promoted the free
/// tag (audit P0-10's bundle).
///
/// What a producer CAN still move is the pre-pow hash, by reshuffling the block it builds. That is
/// job grinding, it is real, and it costs a full inference per try — which is the price the design
/// means to charge. Deriving from the challenge would charge it per NONCE instead, and deriving
/// from nothing would charge it never.
///
/// `nonce_bucket` is ADR-0071 Decision 2's middle term: one execution covers `2^k` nonces, so the
/// search inside a bucket stays a free CPU search and leaving the bucket costs another inference.
pub fn base0_rc_job_anchor_v1(
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    class_id: Hash64,
    bond: &kaspa_consensus_core::tx::TransactionOutpoint,
    nonce_bucket: u64,
) -> Hash64 {
    // The derivation moved to `kaspa_consensus_core::palw_attempt_v2` — it is the protocol's
    // anchor, not this family's, and a verifier must be able to compute it without depending on an
    // execution family's crate. Kept as a delegating name because the producer and the fixtures
    // call it here, and because two copies of a hash is how the value quietly moves.
    kaspa_consensus_core::palw_attempt_v2::palw_job_anchor_v1(network_domain, pre_pow_hash, class_id, bond, nonce_bucket)
}

/// **The anchor's job: the prompt it names, and the context that commits to it.**
///
/// The shape fields are `rc_job_context`'s, unchanged — they are what `step_leaf_count` reads and
/// what the class's catalog was measured over, so a producer that moved one would be running a job
/// its own class does not price. The identity fields are the anchor's, and `prompt_token_ids_hash`
/// is the real one: the court refuses a refutation whose carried prompt is not the one the context
/// commits to, which is how an honest execution proved unadjudicable the first time this was run
/// against a yardstick context.
pub fn base0_rc_job_v1(
    profile: &PalwShapeProfileV3,
    anchor: Hash64,
    vocab: usize,
    prefill: u32,
    decode: u32,
) -> (PalwJobContextV2, Vec<usize>) {
    let mut prompt = Vec::with_capacity(prefill as usize);
    let mut counter = 0u64;
    while prompt.len() < prefill as usize {
        let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_BASE0_DOMAIN_JOB_PROMPT).to_state();
        h.update(anchor.as_byte_slice());
        h.update(&counter.to_le_bytes());
        let block = h.finalize();
        for word in block.as_bytes().chunks_exact(8) {
            if prompt.len() == prefill as usize {
                break;
            }
            let v = u64::from_le_bytes(word.try_into().expect("chunks_exact(8)"));
            prompt.push((v % vocab.max(1) as u64) as usize);
        }
        counter += 1;
    }
    let mut ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, prefill, decode);
    ctx.job_id = anchor;
    ctx.execution_seed = anchor.as_byte_slice()[..32].try_into().expect("a 64-byte hash has 32 bytes");
    ctx.prompt_token_ids_hash =
        kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&prompt.iter().map(|t| *t as u32).collect::<Vec<_>>());
    (ctx, prompt)
}

/// The roots an attempt carries, and the material that answers for them.
pub struct Base0ExecutionV1 {
    pub trace_root: Hash64,
    pub output_root: Hash64,
    pub execution_root: Hash64,
    pub trace_manifest_root: Hash64,
    pub trace_chunk_count: u32,
    /// The producer's own commitment, kept because a refutation is assembled against it.
    pub binding: PalwStepBindingV2,
    /// Every step leaf, kept for the same reason — a producer that discarded these could not
    /// answer a challenge and would lose its bond by default.
    ///
    /// **Empty on a FOLDED run** (ADR-0082 Decision 7): the free-prompt lane keeps `step_tree`
    /// and re-derives any tile an opening needs by replay. `base0_material_encode_v1` refuses a
    /// folded run by name rather than encoding an empty tile set as if it were a capture.
    pub tiles: Base0StepTilesV1,
    /// **The retained tree — one node per `2^retain_level` leaves.** `Some` exactly when the run
    /// folded; the dense sink keeps its tiles and needs no second copy of the same commitment.
    pub step_tree: Option<crate::fp_capture::Base0SparseStepTreeV1>,
    /// The checkpoint leg's leaves and their state chunks. Kept for the third time for the same
    /// reason, and for one more: these are what let a challenge be answered — and adjudicated —
    /// from the calls SINCE a checkpoint instead of from the whole inference.
    pub checkpoints: crate::legs::Base0CheckpointsV1,
    /// Every logits row, i32 lanes, one per call — kept because a decode-side dispute
    /// (ADR-0049 Decision E) is adjudicated against them, and a producer that discarded them
    /// could not carry the pin that clears it.
    pub logits_rows: Vec<Vec<i32>>,
    pub generated_token_ids: Vec<u32>,
}

/// **Run the job and commit to it.**
///
/// The capture is COMPLETE or this fails: `Base0StepCaptureV1::finish` refuses a short one, and a
/// commitment over a short capture claims every unfilled leaf is zero. That object is what the
/// court exists to convict, and an executor must never be the one that emits it.
pub fn base0_execute_for_attempt_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
) -> Result<Base0ExecutionV1, ProduceError> {
    base0_execute_for_attempt_capped_v1(artifact, profile, ctx, prompt, PALW_STEP_MAX_LEAVES)
}

/// [`base0_execute_for_attempt_v1`] against the ladder top the CALLER states — the ruleset's
/// `PalwCourtParamsV2::max_step_leaf_count`.
pub fn base0_execute_for_attempt_capped_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
) -> Result<Base0ExecutionV1, ProduceError> {
    base0_execute_for_attempt_streaming_capped_v1(artifact, profile, ctx, prompt, max_step_leaf_count, &mut |_| {})
}

/// **The same run, with each id handed over as it is SELECTED** (ADR-0077 Decision 2).
///
/// One inference, both halves: `on_token` sees the ids in decode order at the moment
/// `argmax_lowest` picks them, and the capture, the roots and the returned ids are the same run's.
/// A second inference to produce the stream would be exactly the failure Decision 2 exists to
/// prevent — a worker that shows one answer and commits another — so the streaming verb is the
/// loop and the non-streaming one is the loop with a callback that does nothing, never the
/// reverse.
///
/// The callback cannot fail and cannot stop the run. Stopping is the caller's to do around this
/// (the frame is written at completion, as ADR-0044 Decision 10 says), and a stream that could
/// abort a run mid-capture would be a stream that decides what got committed.
pub fn base0_execute_for_attempt_streaming_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    on_token: &mut dyn FnMut(u32),
) -> Result<Base0ExecutionV1, ProduceError> {
    base0_execute_for_attempt_streaming_capped_v1(artifact, profile, ctx, prompt, PALW_STEP_MAX_LEAVES, on_token)
}

/// **The floor's capture, priced against the RULESET's ladder** (ADR-0077 Decision 12) — the same
/// threading the two model tiers carry. The delegating entry points above pass
/// `PALW_STEP_MAX_LEAVES`, which is what every shipped preset froze, so a caller that holds no
/// ruleset is byte-identical to what it was.
pub fn base0_execute_for_attempt_streaming_capped_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
    on_token: &mut dyn FnMut(u32),
) -> Result<Base0ExecutionV1, ProduceError> {
    let prefill = ctx.declared_prefill_tokens as usize;
    let decode_tokens = ctx.exact_decode_tokens as usize;
    if prefill == 0 || decode_tokens == 0 {
        return Err(ProduceError::EmptyJob);
    }
    if prompt.len() < prefill {
        return Err(ProduceError::PromptShorterThanPrefill { prompt: prompt.len(), declared: ctx.declared_prefill_tokens });
    }
    let vocab = artifact.shape.vocab;
    if let Some(bad) = prompt.iter().take(prefill).find(|t| **t >= vocab) {
        return Err(ProduceError::TokenOutOfVocab { token: *bad, vocab });
    }

    let leaf_count = step_leaf_count_capped_v1(profile, ctx, max_step_leaf_count).map_err(ProduceError::StepSpace)?;
    let mut capture = Base0StepCaptureV1::new(leaf_count).map_err(ProduceError::Leg)?;
    let engine = Base0Engine::new(artifact);
    // **ADR-0049 Decision F's obligation, before the first token.**
    //
    // "No worker may commit a step leg for a class whose profile does not name every narrowing the
    // engine performs." The profile is the registered class's graph and it comes from the chain;
    // the engine's steps come from `BASE0_LAYER_IR`. A producer that ran anyway would commit to
    // arithmetic the court recomputes differently and be convicted for performing it correctly.
    //
    // Checked at kv_len 1 AND 2: every `KvScaled` width is a function of the position, and at one
    // position a per-head width and a per-layer one are the same number — which is exactly how the
    // four attention nodes came to be declared once per layer.
    for kv_len in 1..=2 {
        crate::plan::base0_check_graph_v1(engine.plan().map_err(ProduceError::Engine)?, profile, &artifact.shape, kv_len)
            .map_err(ProduceError::GraphMismatch)?;
    }
    let mut cache = KvCache::new(artifact);
    // The class's own checkpoint profile, at the producer's interval — the same object the binding
    // files, so the capture and the commitment cannot disagree about the layout or the cadence.
    let checkpoint_profile = kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(
        kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
    );
    let mut checkpoints = crate::legs::Base0CheckpointCaptureV1::new(ctx, profile, &checkpoint_profile);
    let mut logits_rows: Vec<Vec<i32>> = Vec::with_capacity(decode_tokens);
    let mut generated: Vec<u32> = Vec::with_capacity(decode_tokens);

    // Call 0 — prefill. Logits leaves exist only at its LAST position, so the post table's rows are
    // dropped everywhere else: they are steps this class's step space does not have, and pushing
    // them is refused rather than placed.
    let mut last_logits = Vec::new();
    for (p, token) in prompt.iter().take(prefill).enumerate() {
        let (logits, probe) = engine.forward_token_probed(&mut cache, *token, p).map_err(ProduceError::Engine)?;
        let mut rows = base0_captured_rows_v1(&probe);
        if p + 1 != prefill {
            rows.retain(|r| r.table != PalwStepTableV1::Post);
        }
        capture.push_call(profile, ctx, 0, p as u32, &rows).map_err(ProduceError::Leg)?;
        // **The prefill arm the other two producers have** (ADR-0082 Decision 4, amended; audit
        // B, M-1). `false` at every prefill position on the floor's own per-call map — and the
        // moment any class routed through here registers a tiled one, this is where its
        // checkpoints come from instead of nowhere.
        if checkpoints.wants_checkpoint_after_v1(0, p as u32) {
            checkpoints.push(&cache).map_err(ProduceError::Leg)?;
        }
        last_logits = logits;
    }
    let mut next = argmax_lowest(&last_logits);
    generated.push(next as u32);
    on_token(next as u32);
    logits_rows.push(last_logits);

    // Calls 1..=D−1 — decode. The COORDINATE's position is 0 in every decode call (each call has
    // one position); the cache position is absolute. Conflating the two is a capture that lands
    // every decode row on top of the first one's.
    for call in 1..decode_tokens {
        let cache_position = prefill + call - 1;
        let (logits, probe) = engine.forward_token_probed(&mut cache, next, cache_position).map_err(ProduceError::Engine)?;
        let rows: Vec<Base0CapturedRowV1> = base0_captured_rows_v1(&probe);
        capture.push_call(profile, ctx, call as u32, 0, &rows).map_err(ProduceError::Leg)?;
        next = argmax_lowest(&logits);
        generated.push(next as u32);
        on_token(next as u32);
        logits_rows.push(logits);
        // A checkpoint after this call if the cadence says so — the capture's OWN predicate, not a
        // second spelling of the per-call rule (audit B, M-1). `call == next_covered_decode_call()`
        // is that rule written out, and under a tiled map `next_covered_decode_call()` returns a
        // POSITION: the test would then fire at the wrong coordinates and never during the prefill.
        if checkpoints.wants_checkpoint_after_v1(call as u32, 0) {
            checkpoints.push(&cache).map_err(ProduceError::Leg)?;
        }
    }

    // **Sealed at the count the CLASS's cadence says the job has** (audit B, M-1).
    // `finish_canonical_v1` exists precisely to remove the `decode_calls / interval` spelling from
    // the producers; the floor was the last one still carrying it, and it is the same number on
    // the map the floor registers.
    let checkpoints = checkpoints.finish_canonical_v1().map_err(ProduceError::Leg)?;
    let tiles = capture.finish().map_err(ProduceError::Leg)?;
    let trace_root = base0_logits_trace_root_v1(ctx, &logits_rows, &generated);
    let activation_leg_root = base0_activation_leg_root_v1(ctx);
    // The COMMIT side of the same ladder — see the A16 executor's note. `checkpoint_profile` is
    // this run's own, byte-for-byte the one the uncapped name builds for itself.
    let binding = crate::legs::base0_binding_from_capture_with_profile_capped_v1(
        profile,
        ctx,
        &tiles,
        &checkpoints,
        &checkpoint_profile,
        trace_root,
        activation_leg_root,
        max_step_leaf_count,
    )
    .map_err(ProduceError::Leg)?;
    let ctx_hash = ctx.context_hash();
    // BASE-0 has no tokenizer, so there are no rendered bytes — and the empty rendering is the
    // honest statement of that. Token ids are the identity in any case (v2 design §10.7).
    let output_root = kaspa_consensus_core::palw_v2::output_commitment_v2(
        &ctx_hash,
        &generated,
        &kaspa_consensus_core::palw_v2::rendered_output_hash_v2(&[]),
    );
    // One chunk: the whole trace is one object at this class's size, and a manifest that claimed
    // more chunks than the producer retained would be a retention promise it cannot keep.
    // **The consensus derivation, not this family's** (ADR-0072 Decision 8): admission pins the
    // manifest root by equality to `attempt_trace_manifest_root_v1(trace_root, 1)`, so a family
    // that hashed its own context in here would produce attempts the chain refuses.
    let trace_manifest_root = kaspa_consensus_core::palw_attempt_v2::attempt_trace_manifest_root_v1(trace_root, 1);

    Ok(Base0ExecutionV1 {
        // The floor's capture is the dense one: `backend.rs` reads its tiles back for the court's
        // assembly and for the injected-fault drill, and ADR-0082 Decision 7 is about the lane
        // whose jobs are thousands of positions long.
        step_tree: None,
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

// ---------------------------------------------------------------------------------------------
// The panel's half: reading back retained material and deciding a verdict
// ---------------------------------------------------------------------------------------------

/// The execution material a producer retains for `trace_retention_daa`, as it is stored.
///
/// `(binding, tiles, generated ids)` — everything a seat needs to decide whether the producer
/// committed to what it actually computed, and everything a refutation is assembled from later.
/// The canonical wire/disk encoding of the retained material — one codec, used by the producer's
/// retention file, the P2P material broadcast, and the panel seat's decode, so the three cannot
/// drift. borsh over the tuple, exactly the bytes `retain_execution` has always written.
pub fn base0_material_encode_v1(run: &Base0ExecutionV1) -> Result<Vec<u8>, ProduceError> {
    // **A folded run has no tiles, and an empty tile vector is not a capture.** Encoded through
    // here it would produce material that decodes, carries a binding, and reproduces no step root
    // at all — a seat's `Mismatch` against an honest producer. The fold's material is v2
    // ([`base0_fp_material_encode_v2`]), and this says so rather than serialising the absence.
    if run.step_tree.is_some() {
        return Err(ProduceError::Internal("a folded capture is retained as v2 material, not as tiles"));
    }
    borsh::to_vec(&(&run.binding, &run.tiles.tiles, &run.logits_rows, &run.generated_token_ids, &run.checkpoints.chunks))
        .map_err(|_| ProduceError::Internal("the execution material is not serializable"))
}

/// Decode what [`base0_material_encode_v1`] produced. `Err` is a seat's honest `Unavailable` —
/// bytes that do not decode are bytes that were not served.
pub fn base0_material_decode_v1(bytes: &[u8]) -> Result<Base0RetainedMaterialV1, ProduceError> {
    borsh::from_slice(bytes).map_err(|_| ProduceError::Internal("the served material does not decode"))
}

pub type Base0RetainedMaterialV1 = (
    kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    Vec<(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1)>,
    Vec<Vec<i32>>,
    Vec<u32>,
    // **Per checkpoint, its state chunks in map order** — added when the checkpoint leg started
    // carrying real state.
    //
    // Only the chunks travel, not the leaves: a leaf is a pure function of its chunks, the chain
    // and the job, so carrying it too would be a second source for the same fact and the received
    // copy would be the one a dishonest producer controls. `Base0CheckpointsV1::from_chunks_v1`
    // re-derives.
    //
    // This changes the retained-material encoding. Land-stage and crate-local — an older retention
    // file no longer decodes, which is a seat's honest `Unavailable` rather than a silent
    // mis-parse, because borsh refuses a short tuple.
    Vec<Vec<Vec<u8>>>,
);

/// The wire magic of the FOLDED retention (ADR-0082 Decision 7). Opaque bytes with a magic and a
/// version, borsh behind it — `Base0FpIntervalOpeningV1`'s shape, for its reason: bytes that are
/// not this retention are refused as such rather than mis-parsed as the dense tuple, which borsh
/// would happily attempt.
pub const PALW_BASE0_FP_MATERIAL_MAGIC_V2: [u8; 8] = *b"MSKFPMV2";
pub const PALW_BASE0_FP_MATERIAL_VERSION_V2: u16 = 2;

/// **What a free-prompt executor retains for the claim's life, after Decision 7.**
///
/// The dense tuple's `Vec<(u64, PalwStepTileLeafV1)>` is gone and nothing replaces it: an opening
/// re-derives the tiles it needs by replaying the interval from the checkpoint chunks
/// (`fp_interval`), which is the whole reason the checkpoint leg exists. What is left is what
/// cannot be recomputed from the class and the job, plus the one thing that can but is asked for
/// on every seat's first question:
///
/// * `binding` — the commitment itself, which is what makes any of this THIS claim's.
/// * `step_tree` — one 64-byte node per `2^retain_level` leaves. Recomputable by a full replay,
///   retained because `verify_material` is a seat's first question and a replay is its last resort.
/// * `logits_rows` — the rows the decode pin is adjudicated against (ADR-0049 Decision E) and the
///   rows an opening's SEED tile is cut from. Exactly what a seat needs, which is why the capture
///   already keeps only the selecting rows.
/// * `generated_token_ids` — the ids, which are also the seed tokens a checkpoint-anchored replay
///   resumes from.
/// * `prompt_token_ids` — the user's own ids. The dense retention did not carry them because it
///   carried every tile of the prefill instead; a fold retains neither, and a retention that
///   cannot re-derive its own execution can answer no court move. 4 bytes a token, against the
///   ~50 MB a position the tiles were.
/// * `checkpoint_chunks` — the cache at each committed checkpoint, in map order. **The one term
///   that still grows with the job**, and the one Decision 4 addresses: on a class whose map
///   addresses history tiles this is EMPTY, because retaining a chunk per position is `Θ(n²)`
///   (13.5 GB on a 4,096-position dense job) and the cache is prefix-stable, so the executor keeps
///   the cache once and re-derives any checkpoint's chunks from it.
/// * `checkpoint_leaves` — the leg's own leaves, which is what the fold retains INSTEAD of that
///   state (`Base0CheckpointRetentionV1::Fold`: "the leaves and their hashes, and NOT one byte of
///   state"). A leaf is 140 bytes and a chunk set is the whole cache, so this is the term that
///   makes the per-position cadence affordable: a seat re-derives the leg from them
///   (`Base0CheckpointCaptureV1::from_leaves_v1`) and compares its root to the binding's, and the
///   one thing leaves cannot decide — whether `state_chunks_root` is the root of a state the job
///   reaches — is arithmetic, which is what Decision 9 has the seat recompute for itself.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Base0FpMaterialV2 {
    pub version: u16,
    pub binding: kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    pub step_tree: crate::fp_capture::Base0SparseStepTreeV1,
    pub logits_rows: Vec<Vec<i32>>,
    pub generated_token_ids: Vec<u32>,
    pub prompt_token_ids: Vec<u32>,
    pub checkpoint_chunks: Vec<Vec<Vec<u8>>>,
    pub checkpoint_leaves: Vec<kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2>,
}

/// Encode a FOLDED run's retention. Refuses a dense one by name: two encodings of one commitment
/// is the thing the ADR warns about, and the way to keep them one is to make each refuse the
/// other's input.
pub fn base0_fp_material_encode_v2(run: &Base0ExecutionV1, prompt_token_ids: &[u32]) -> Result<Vec<u8>, ProduceError> {
    let Some(tree) = run.step_tree.as_ref() else {
        return Err(ProduceError::Internal("a dense capture is retained as tiles, not as a fold"));
    };
    // **No check that these hash to the context's `prompt_token_ids_hash` here.** They are the ids
    // this executor RAN, and that is what a replay of its own execution needs. Whether the job's
    // declared hash is the hash of the tokens it was handed is a rule about the JOB, enforced
    // where a job is built (`run_one_job_v1` derives the field from the ids it tokenized) — and a
    // retention that refused after the whole inference had run would be enforcing it in the one
    // place where the answer is already paid for. What guards a WRONG list is the re-execution
    // itself: `dense_capture_from_fold_v1` compares the binding it reproduces against this one.
    let material = Base0FpMaterialV2 {
        version: PALW_BASE0_FP_MATERIAL_VERSION_V2,
        binding: run.binding.clone(),
        step_tree: tree.clone(),
        logits_rows: run.logits_rows.clone(),
        generated_token_ids: run.generated_token_ids.clone(),
        prompt_token_ids: prompt_token_ids.to_vec(),
        checkpoint_chunks: run.checkpoints.chunks.clone(),
        checkpoint_leaves: run.checkpoints.leaves.clone(),
    };
    let body = borsh::to_vec(&material).map_err(|_| ProduceError::Internal("the execution material is not serializable"))?;
    let mut out = Vec::with_capacity(body.len() + PALW_BASE0_FP_MATERIAL_MAGIC_V2.len());
    out.extend_from_slice(&PALW_BASE0_FP_MATERIAL_MAGIC_V2);
    out.extend_from_slice(&body);
    Ok(out)
}

/// Decode what [`base0_fp_material_encode_v2`] produced. `Err` is a seat's honest `Unavailable` —
/// bytes that do not decode are bytes that were not served — and a tree that is not its own shape
/// is refused HERE, before any derivation indexes its retained vector.
pub fn base0_fp_material_decode_v2(bytes: &[u8]) -> Result<Base0FpMaterialV2, ProduceError> {
    let body = bytes
        .strip_prefix(&PALW_BASE0_FP_MATERIAL_MAGIC_V2)
        .ok_or(ProduceError::Internal("the served material is not this family's folded retention"))?;
    let material: Base0FpMaterialV2 =
        borsh::from_slice(body).map_err(|_| ProduceError::Internal("the served material does not decode"))?;
    if material.version != PALW_BASE0_FP_MATERIAL_VERSION_V2 {
        return Err(ProduceError::Internal("the served material is a different retention version"));
    }
    material.step_tree.validate_v1().map_err(|_| ProduceError::Internal("the served retained tree is not its own shape"))?;
    Ok(material)
}

/// **Either retention, decoded** — the folded one first, because it is the one a free-prompt lane
/// produces after ADR-0082 Decision 7 and the dense tuple's borsh would happily mis-read it.
pub enum Base0RetentionV1 {
    Dense(Base0RetainedMaterialV1),
    Folded(Base0FpMaterialV2),
}

impl Base0RetentionV1 {
    pub fn binding(&self) -> &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2 {
        match self {
            Self::Dense((binding, ..)) => binding,
            Self::Folded(m) => &m.binding,
        }
    }
    pub fn logits_rows(&self) -> &[Vec<i32>] {
        match self {
            Self::Dense((_, _, rows, _, _)) => rows,
            Self::Folded(m) => &m.logits_rows,
        }
    }
    pub fn generated_token_ids(&self) -> &[u32] {
        match self {
            Self::Dense((_, _, _, ids, _)) => ids,
            Self::Folded(m) => &m.generated_token_ids,
        }
    }
    pub fn checkpoint_chunks(&self) -> &[Vec<Vec<u8>>] {
        match self {
            Self::Dense((_, _, _, _, chunks)) => chunks,
            Self::Folded(m) => &m.checkpoint_chunks,
        }
    }
    /// The tiles a dense retention kept — `None` for a fold, whose caller must re-derive what it
    /// needs by replay.
    pub fn tiles(&self) -> Option<&[(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1)]> {
        match self {
            Self::Dense((_, tiles, ..)) => Some(tiles),
            Self::Folded(_) => None,
        }
    }
}

pub fn base0_material_decode_any_v1(bytes: &[u8]) -> Result<Base0RetentionV1, ProduceError> {
    match base0_fp_material_decode_v2(bytes) {
        Ok(folded) => Ok(Base0RetentionV1::Folded(folded)),
        Err(_) => base0_material_decode_v1(bytes).map(Base0RetentionV1::Dense),
    }
}

/// **What a panel seat checks before it signs `Valid`, on folded material.**
///
/// The dense check rebuilds the step root from the tiles; this reads it off the retained tree,
/// which is the same root by construction (`Base0CaptureSinkV1::finish`) and is checked against
/// the binding here rather than trusted. Everything after that is
/// [`base0_material_tail_matches_v1`] — one rule for both retentions.
pub fn base0_fp_material_matches_claim_v2(
    material: &Base0FpMaterialV2,
    committed_execution_root: Hash64,
    committed_trace_root: Hash64,
) -> Result<bool, ProduceError> {
    let binding = &material.binding;
    if material.step_tree.validate_v1().is_err() || material.step_tree.leaf_count() != binding.step_leaf_count {
        return Ok(false);
    }
    let Ok(root) = material.step_tree.root() else {
        return Ok(false);
    };
    if root != binding.step_merkle_root {
        return Ok(false);
    }
    base0_material_tail_matches_v1(
        binding,
        &material.logits_rows,
        &material.generated_token_ids,
        &material.checkpoint_chunks,
        &material.checkpoint_leaves,
        committed_execution_root,
        committed_trace_root,
    )
}

/// What a replay from a checkpoint produced.
///
/// The leaves are keyed by their canonical index, so a caller compares them against the committed
/// leg by index rather than by walking a whole execution — which is the point of the thing.
pub struct Base0CheckpointReplayV1 {
    pub tiles: Vec<(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1)>,
    pub leaf_hashes: Vec<(u64, Hash64)>,
    pub logits_rows: Vec<Vec<i32>>,
    pub generated_token_ids: Vec<u32>,
    /// Decode calls actually run. The number a dispute pays instead of the whole inference.
    pub calls_replayed: u32,
}

/// What an anchored check of one leaf concluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Base0AnchoredLeafCheckV1 {
    pub source: Base0ReplaySourceV1,
    /// The leg's own leaf hash at this index.
    pub committed: Hash64,
    /// What the anchored replay recomputed, or `None` when the source is `Genesis` and this unit
    /// does not reach the leaf.
    pub recomputed: Option<Hash64>,
}

impl Base0AnchoredLeafCheckV1 {
    /// `Some(false)` is a disagreement a court can act on; `Some(true)` is agreement; **`None` is
    /// "not checked"** and must never be read as either.
    pub fn agrees(&self) -> Option<bool> {
        self.recomputed.map(|h| h == self.committed)
    }
}

/// **The unit at the boundary a disputant actually stands at: served bytes in, one leaf's verdict
/// out.**
///
/// [`base0_anchored_leaf_replay_v1`] takes a `Base0CheckpointsV1`, which is what a PRODUCER holds.
/// A challenger or a seat holds the material the producer served, so without this the unit stopped
/// one seam short of its own caller.
///
/// # The step that makes it sound
///
/// The chunks in that material are the producer's. Resuming from them unchecked would let a
/// producer that lied about a step hand over a state consistent with the LIE and watch the replay
/// agree with it. So the leg is re-derived from the served chunks and its root compared to the
/// binding's `checkpoint_merkle_root` **before** anything resumes: the anchor has to be the one the
/// claim committed, or there is no anchored check to do.
///
/// The binding itself is verified the same way it always is — `check_step_refutation_v1`'s
/// `verify_binding` recomputes `committed_execution_root` — which is deliberately NOT re-done here:
/// this answers "does this leaf agree with the leg the binding carries", and whether that binding
/// is the claim's is the caller's separate question, with its own separate answer.
pub fn base0_anchored_leaf_check_v1(
    artifact: &Base0ArtifactV1,
    material: &Base0RetainedMaterialV1,
    leaf_index: u64,
) -> Result<Base0AnchoredLeafCheckV1, ProduceError> {
    let (binding, tiles, _, generated, chunks) = material;
    let profile = &binding.shape_profile;
    let ctx = &binding.job_context;

    let checkpoints = crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(ctx, profile, &binding.checkpoint_profile, chunks)
        .map_err(|_| ProduceError::Internal("the served checkpoint chunks do not form a leg"))?;
    if checkpoints.merkle_root != binding.checkpoint_merkle_root || checkpoints.leaf_hashes.len() as u32 != binding.checkpoint_count {
        return Err(ProduceError::Internal("the served checkpoints are not the ones the binding committed"));
    }

    let committed = tiles
        .iter()
        .find(|(i, _)| *i == leaf_index)
        .map(|(_, leaf)| {
            kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx.context_hash(), &profile.shape_profile_id(), leaf)
        })
        .ok_or(ProduceError::Internal("the served material holds no tile at the disputed leaf"))?;

    let (recomputed, source) = base0_anchored_leaf_replay_v1(artifact, profile, ctx, &checkpoints, generated, leaf_index)?;
    Ok(Base0AnchoredLeafCheckV1 { source, committed, recomputed })
}

/// Where an anchored replay of one disputed leaf has to start.
///
/// `Genesis` is not a failure — it is the honest answer for a leaf in the prefill call, or in a
/// decode call that runs before the first checkpoint. A unit that pretended every leaf had an
/// anchor would hand a court a resume point that does not reach the step under dispute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base0ReplaySourceV1 {
    /// No committed checkpoint precedes the disputed call. `calls` is what a run from the prefill
    /// costs — carried so a caller can state the cost rather than discover it.
    Genesis { calls: u32 },
    /// Resume from checkpoint `index`, which covers `covered_decode_call`, and run `calls` calls.
    Checkpoint { index: u32, covered_decode_call: u32, calls: u32 },
}

impl Base0ReplaySourceV1 {
    /// Decode calls this source has to run.
    pub fn calls(&self) -> u32 {
        match self {
            Self::Genesis { calls } | Self::Checkpoint { calls, .. } => *calls,
        }
    }
}

/// **Which checkpoint a disputed leaf should be replayed from** — the piece that turns a resume
/// primitive into an anchored replay.
///
/// The leaf index names a coordinate ([`kaspa_consensus_core::palw_step::canonical_step_coordinates`]),
/// the coordinate names a call, and the anchor is the LAST checkpoint whose `covered_decode_call`
/// is strictly below that call. Strictly: a checkpoint that covers the disputed call is the state
/// AFTER it ran, and resuming from there would skip the very step under dispute.
///
/// Chosen from the committed leaves rather than computed from the interval, because the leaves are
/// what a challenger holds and what the leg's root binds. Deriving the anchor from the interval
/// would agree with them only as long as nothing drifted, and a court would then resume from a
/// checkpoint the claim never committed.
pub fn base0_replay_anchor_for_leaf_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    checkpoints: &crate::legs::Base0CheckpointsV1,
    leaf_index: u64,
) -> Option<Base0ReplaySourceV1> {
    let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(profile, ctx, leaf_index)?;
    let disputed_call = coord.call_index;
    // Call 0 is the prefill: no checkpoint exists before it, by construction.
    if disputed_call == 0 {
        return Some(Base0ReplaySourceV1::Genesis { calls: 0 });
    }
    let best =
        checkpoints.leaves.iter().filter(|leaf| leaf.covered_decode_call < disputed_call).max_by_key(|leaf| leaf.covered_decode_call);
    Some(match best {
        Some(leaf) => Base0ReplaySourceV1::Checkpoint {
            index: leaf.checkpoint_index,
            covered_decode_call: leaf.covered_decode_call,
            calls: disputed_call - leaf.covered_decode_call,
        },
        None => Base0ReplaySourceV1::Genesis { calls: disputed_call },
    })
}

/// **Capture → restore → anchored replay, as one call.**
///
/// Given a disputed leaf, this picks the anchor, rebuilds the cache from the checkpoint the claim
/// committed, replays only the calls between, and hands back the leaf hash it recomputed beside the
/// source it used. Comparing that hash to the committed leg is the whole adjudication of that leaf,
/// and it costs the calls since a checkpoint rather than the inference.
///
/// `Ok(None)` for the leaf hash means the answer is honest and negative: the anchor is `Genesis`,
/// so this unit cannot reach the leaf and a caller must not read silence as agreement. That case is
/// returned rather than errored because it is a fact about the leaf, not a fault.
pub fn base0_anchored_leaf_replay_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    checkpoints: &crate::legs::Base0CheckpointsV1,
    generated_token_ids: &[u32],
    leaf_index: u64,
) -> Result<(Option<Hash64>, Base0ReplaySourceV1), ProduceError> {
    let source = base0_replay_anchor_for_leaf_v1(profile, ctx, checkpoints, leaf_index)
        .ok_or(ProduceError::Internal("the leaf index is not a main step coordinate"))?;
    let Base0ReplaySourceV1::Checkpoint { index, covered_decode_call, calls } = source else {
        return Ok((None, source));
    };
    // **By POSITION in the leg, not by `checkpoint_index`.** They coincide in a complete leg and
    // stop coinciding the moment one is absent — and a leg with a checkpoint missing is exactly the
    // material a dispute arrives with. Indexing `chunks` by the field would then pair a leaf with
    // another checkpoint's bytes, and the replay would resume from a state that verifies against
    // nothing.
    let at = checkpoints
        .leaves
        .iter()
        .position(|l| l.checkpoint_index == index)
        .ok_or(ProduceError::Internal("the chosen anchor is not in the leg it was chosen from"))?;
    let chunks = checkpoints.chunks.get(at).ok_or(ProduceError::Internal("the checkpoint leg holds no chunks for its own leaf"))?;
    let leaf = &checkpoints.leaves[at];
    // The token the call after the checkpoint consumes is the one the covered call produced.
    let seed = *generated_token_ids
        .get(covered_decode_call as usize)
        .ok_or(ProduceError::Internal("the trace material does not carry the token the anchor resumes on"))?;
    let replay = base0_replay_from_checkpoint_v1(artifact, profile, ctx, leaf, chunks, seed, calls)?;
    Ok((replay.leaf_hashes.iter().find(|(i, _)| *i == leaf_index).map(|(_, h)| *h), source))
}

/// **Replay forward from a committed checkpoint** — the reason the checkpoint leg exists.
///
/// A dispute over a leaf in decode call `k` used to cost the whole inference: prefill over every
/// position, then every decode call up to `k`, because the KV state had no committed form anyone
/// could resume from. With the state map registered and the leg captured, a verifier rebuilds the
/// cache from the chunks the producer committed and runs only the calls SINCE that checkpoint.
///
/// # What makes this sound rather than merely cheaper
///
/// The cache is rebuilt from bytes whose hash is under `checkpoint.state_chunks_root`, which is
/// under the checkpoint leaf, which is under `checkpoint_merkle_root`, which is inside
/// `committed_execution_root`. So resuming from it is resuming from something the producer is
/// already bound to: a producer that hands over state its execution did not have has changed its
/// own commitment, and a producer that hands over the honest state cannot then disown what the
/// replay computes from it.
///
/// The seed token is the caller's, deliberately. It is `generated[covered_decode_call − 1 + 1]` =
/// `generated[covered_decode_call]` — the token the next call consumes — and it comes from the
/// trace root's own material, which is bound separately. Deriving it here from the replay would
/// make the replay agree with itself instead of with the claim.
pub fn base0_replay_from_checkpoint_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    checkpoint: &kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2,
    chunks: &[Vec<u8>],
    seed_token: u32,
    calls: u32,
) -> Result<Base0CheckpointReplayV1, ProduceError> {
    base0_replay_from_checkpoint_capped_v1(
        artifact,
        profile,
        ctx,
        checkpoint,
        chunks,
        seed_token,
        calls,
        kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
    )
}

/// [`base0_replay_from_checkpoint_v1`] against the ruleset's ladder top. The replay prices the
/// SAME job the capture did, so it has to be told the same ladder or a class registered against a
/// deeper one replays nothing it produced.
#[allow(clippy::too_many_arguments)]
pub fn base0_replay_from_checkpoint_capped_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    checkpoint: &kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2,
    chunks: &[Vec<u8>],
    seed_token: u32,
    calls: u32,
    step_ladder_cap: u64,
) -> Result<Base0CheckpointReplayV1, ProduceError> {
    let prefill = ctx.declared_prefill_tokens;
    let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    let covered = checkpoint.covered_decode_call;
    if covered > decode_calls || calls == 0 || covered.saturating_add(calls) > decode_calls {
        return Err(ProduceError::Internal("the replay window is not inside this job's decode calls"));
    }
    // **The map the CLASS declares, at the cadence the CLASS counts in** — see
    // `crate::legs::base0_checkpoint_geometry_at_v1`. Chunking at capture and un-chunking at
    // replay are one decision; they were two, and the second one ignored the class.
    let geometry = crate::legs::base0_checkpoint_geometry_at_v1(profile, ctx, covered).map_err(ProduceError::Leg)?;
    let mut cache = KvCache::from_state_chunks(artifact, &geometry, chunks).map_err(ProduceError::Engine)?;

    let leaf_count = step_leaf_count_capped_v1(profile, ctx, step_ladder_cap).map_err(ProduceError::StepSpace)?;
    let mut capture = Base0StepCaptureV1::new(leaf_count).map_err(ProduceError::Leg)?;
    let engine = Base0Engine::new(artifact);
    // **ADR-0049 Decision F's obligation, before the first token.**
    //
    // "No worker may commit a step leg for a class whose profile does not name every narrowing the
    // engine performs." The profile is the registered class's graph and it comes from the chain;
    // the engine's steps come from `BASE0_LAYER_IR`. A producer that ran anyway would commit to
    // arithmetic the court recomputes differently and be convicted for performing it correctly.
    //
    // Checked at kv_len 1 AND 2: every `KvScaled` width is a function of the position, and at one
    // position a per-head width and a per-layer one are the same number — which is exactly how the
    // four attention nodes came to be declared once per layer.
    for kv_len in 1..=2 {
        crate::plan::base0_check_graph_v1(engine.plan().map_err(ProduceError::Engine)?, profile, &artifact.shape, kv_len)
            .map_err(ProduceError::GraphMismatch)?;
    }
    let mut next = seed_token as usize;
    if next >= artifact.shape.vocab {
        return Err(ProduceError::TokenOutOfVocab { token: next, vocab: artifact.shape.vocab });
    }
    let mut logits_rows = Vec::with_capacity(calls as usize);
    let mut generated = Vec::with_capacity(calls as usize);
    for call in covered + 1..=covered + calls {
        let cache_position = (prefill + call - 1) as usize;
        let (logits, probe) = engine.forward_token_probed(&mut cache, next, cache_position).map_err(ProduceError::Engine)?;
        let rows: Vec<Base0CapturedRowV1> = base0_captured_rows_v1(&probe);
        capture.push_call(profile, ctx, call, 0, &rows).map_err(ProduceError::Leg)?;
        next = argmax_lowest(&logits);
        generated.push(next as u32);
        logits_rows.push(logits);
    }
    // `finish_partial` is correct HERE and nowhere else: a replay deliberately covers a window, and
    // the leaves it did not touch are not claims about zero — they are simply not this replay's.
    // Which is why only the touched ones are returned.
    let partial = capture.finish_partial();
    let ctx_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    let leaf_hashes = partial
        .tiles
        .iter()
        .map(|(i, leaf)| (*i, kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf)))
        .collect();
    Ok(Base0CheckpointReplayV1 {
        tiles: partial.tiles,
        leaf_hashes,
        logits_rows,
        generated_token_ids: generated,
        calls_replayed: calls,
    })
}

/// **What a panel seat checks before it signs `Valid`.**
///
/// A seat's receipt is an attestation that the producer served material matching what its claim
/// committed. The check that makes it more than a rubber stamp is the one the court would run:
/// rebuild the step leg from the tiles and see whether it reproduces the `execution_root` the claim
/// carries. A producer that committed one root and retained a different execution fails here —
/// before any court, and without opening one.
///
/// `Err` is "I could not verify", which is a seat's honest `Unavailable`; `Ok(false)` is "the
/// material does not match what was committed", which is the same verdict for a different reason.
/// Neither is a conviction: convicting is the court's, on evidence a challenger assembles.
pub fn base0_material_matches_claim_v1(
    material: &Base0RetainedMaterialV1,
    committed_execution_root: Hash64,
    committed_trace_root: Hash64,
) -> Result<bool, ProduceError> {
    base0_material_matches_claim_capped_v1(
        material,
        committed_execution_root,
        committed_trace_root,
        kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
    )
}

/// [`base0_material_matches_claim_v1`] against the ladder top the CALLER states — the ruleset's
/// `PalwCourtParamsV2::max_step_leaf_count`, which a backend holds through `with_step_ladder_cap`
/// (ADR-0080 W1b).
///
/// The bound below is an ALLOCATION guard and it stays; what moves is the ceiling. At the leg's
/// default a seat answered `Ok(false)` — "the material does not match what was committed" — for
/// every honest dense capture of a class registered against a deeper ladder, which is the same
/// sentence said about an honest producer. `Ok(false)` is a verdict, not an error, so nothing
/// downstream could tell the two apart.
/// **A dense material's step leaves, rebuilt from its tiles under the ruleset's ladder** — one
/// spelling for the material verifier ([`base0_material_matches_claim_capped_v1`]) and the interval
/// opener (`fp_interval`), which is how the two cannot bound the space differently again. `None`
/// when the binding's space is empty or above the ladder, or when a tile lies outside the space
/// it claims to fill.
pub fn base0_dense_step_leaves_capped_v1(
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    tiles: &[(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1)],
    max_step_leaf_count: u64,
) -> Option<Vec<Hash64>> {
    if binding.step_leaf_count == 0 || binding.step_leaf_count > max_step_leaf_count {
        return None;
    }
    let ctx_hash = binding.job_context.context_hash();
    let profile_hash = binding.shape_profile.shape_profile_id();
    let mut leaves = vec![Hash64::default(); binding.step_leaf_count as usize];
    for (index, leaf) in tiles {
        let slot = leaves.get_mut(*index as usize)?;
        *slot = kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf);
    }
    Some(leaves)
}

/// The root of [`base0_dense_step_leaves_capped_v1`], under the same ladder. The ruleset's cap
/// bounds it, not the default one — a graph-v5 capture (6.6 M leaves) verified as a mismatch
/// under `step_merkle_root_v1`, which walks `PALW_STEP_LEG_MAX_LEAVES`.
pub fn base0_dense_step_root_capped_v1(
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    tiles: &[(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1)],
    max_step_leaf_count: u64,
) -> Option<Hash64> {
    let leaves = base0_dense_step_leaves_capped_v1(binding, tiles, max_step_leaf_count)?;
    kaspa_consensus_core::palw_step_leg::step_merkle_root_capped_v1(&leaves, max_step_leaf_count).ok()
}

pub fn base0_material_matches_claim_capped_v1(
    material: &Base0RetainedMaterialV1,
    committed_execution_root: Hash64,
    committed_trace_root: Hash64,
    max_step_leaf_count: u64,
) -> Result<bool, ProduceError> {
    let (binding, tiles, logits_rows, generated, checkpoint_chunks) = material;
    let Some(root) = base0_dense_step_root_capped_v1(binding, tiles, max_step_leaf_count) else {
        return Ok(false); // an empty space, one above the ladder, or a tile outside it
    };
    if root != binding.step_merkle_root {
        return Ok(false);
    }
    // The DENSE retention is the per-call one and carries no leaf vector: its leg comes from its
    // chunks, which is the arm this hands the empty slice to.
    base0_material_tail_matches_v1(
        binding,
        logits_rows,
        generated,
        checkpoint_chunks,
        &[],
        committed_execution_root,
        committed_trace_root,
    )
}

/// **Everything a retained material must reproduce that is not the step leg**: the logits trace
/// root under the scheme the class registered, the checkpoint leg re-derived from the served
/// chunks, and the binding's own two roots against the claim's.
///
/// One function because there are two retentions and one rule (ADR-0082 Decision 7): the dense
/// tuple rebuilds its step root from tiles, the folded one reads it off the retained tree, and
/// from there they owe exactly the same things.
pub fn base0_material_tail_matches_v1(
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    logits_rows: &[Vec<i32>],
    generated: &[u32],
    checkpoint_chunks: &[Vec<Vec<u8>>],
    checkpoint_leaves: &[kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2],
    committed_execution_root: Hash64,
    committed_trace_root: Hash64,
) -> Result<bool, ProduceError> {
    // The logits rows and generated ids must REPRODUCE the trace root the binding carries —
    // equality of the binding's field against the claim says the producer kept the right
    // commitment; this says it kept the execution behind it, which is what a decode-side dispute
    // (ADR-0049 Decision E) is adjudicated against. **Under the scheme the CLASS registered**:
    // the floor commits the flat integer root over every row, and the model tiers commit the
    // tiled root over their selecting rows — one check that recomputed only the flat root would
    // read every tiled-class material as a mismatch, which is a seat refusing every honest
    // producer of the classes this check exists to police.
    // **Rows that build no tree are a refusal here, not a hash.** The tiled derivation is total
    // (it answers `None` for a run with no rows, or a row with no lanes) because THIS is where a
    // stranger's bytes reach it: the material comes off the gossip pool, and the binding beside it
    // is the stranger's too, so the checks above — a leaf count in range, tiles that fill it, a
    // step root that matches — are all satisfiable by an attacker who simply computes them. What
    // is not satisfiable is a trace root over rows that do not exist.
    let recomputed_trace_root =
        if binding.shape_profile.logits_scheme_id == kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1() {
            match kaspa_consensus_core::palw_step_refute::tiled_logits_trace_root_v1(&binding.job_context, logits_rows, generated) {
                Some(root) => root,
                None => return Ok(false),
            }
        } else {
            kaspa_consensus_core::palw_step_refute::base0_logits_trace_root_v1(&binding.job_context, logits_rows, generated)
        };
    if recomputed_trace_root != binding.full_logits_trace_root {
        return Ok(false);
    }
    // **And the checkpoints must be the ones it committed.**
    //
    // Re-derived from the served chunks, not read from a served leaf: a leaf is a pure function of
    // its chunks, the chain and the job, so a producer that also SENT its leaves would be sending
    // the one copy it controls. This way the chunks are the only thing that can be wrong, and a
    // wrong one moves the root.
    //
    // Without this the checkpoint leg was unchecked material: a seat signed `Valid` over a claim
    // whose checkpoints it had never reproduced, and the first thing to notice would have been a
    // court that could not open one.
    //
    // **From WHICH bytes is the cadence's question, not the caller's** (ADR-0082 Decision 4,
    // amended; audit B, C-1). A per-call class retains its chunks and the leg is rebuilt from
    // them, exactly as before — no shipped material changes shape. A per-position class retains
    // NONE: a chunk per position is `Θ(n²)` and the cache is prefix-stable, so what it keeps is
    // the leg's leaves, and the leg is rebuilt from those. `from_leaves_v1` applies every
    // structural rule `palw_step_leg`'s own `checkpoint_fault` recomputes — the index, the
    // cadence's canonical counter, the chain — so a leaf vector that is not a leg this class could
    // have filed is refused rather than "rebuilt".
    //
    // What the leaves cannot say is that `state_chunks_root` is the root of a state this job
    // reaches. Nothing served can: it is arithmetic, and a producer that shipped bytes for it
    // would be shipping its own opinion. Decision 9 puts that question where it can be answered —
    // the seat's OWN recompute, compared against this same leaf at the interval it draws.
    let rebuilt = match kaspa_consensus_core::palw_context_ladder::palw_checkpoint_cadence_v1(&binding.shape_profile) {
        kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1::PerDecodeCall => {
            crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(
                &binding.job_context,
                &binding.shape_profile,
                &binding.checkpoint_profile,
                checkpoint_chunks,
            )
        }
        kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1::PerPosition => {
            // A folded class that nevertheless served state is not this class's retention: the
            // bytes it would be serving are the history Decision 9 exists to keep off the wire.
            if !checkpoint_chunks.is_empty() {
                return Ok(false);
            }
            crate::legs::Base0CheckpointCaptureV1::from_leaves_v1(
                &binding.job_context,
                &binding.shape_profile,
                &binding.checkpoint_profile,
                checkpoint_leaves,
            )
        }
    };
    let Ok(rebuilt) = rebuilt else {
        return Ok(false);
    };
    if rebuilt.merkle_root != binding.checkpoint_merkle_root || rebuilt.leaf_hashes.len() as u32 != binding.checkpoint_count {
        return Ok(false);
    }
    // And the binding the producer kept must be the one its CLAIM committed — otherwise it retained
    // a consistent execution of some other job.
    Ok(binding.committed_execution_root == committed_execution_root && binding.full_logits_trace_root == committed_trace_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rc::PALW_RC_BASE0_SEED;
    use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};

    /// A job small enough to run in a unit test and shaped exactly like the RC's — one prefill
    /// call and two decode calls, so the multi-call enumeration is exercised rather than assumed.
    fn small_job() -> (crate::artifact::Base0ArtifactV1, PalwShapeProfileV3, PalwJobContextV2, Vec<usize>) {
        let mut geometry = PALW_RC_BASE0_GEOMETRY;
        geometry.layer_count = 2;
        geometry.hidden_dim = 64;
        geometry.ffn_dim = 128;
        geometry.attn_heads = 2;
        geometry.attn_head_dim = 32;
        geometry.vocab_size = 128;
        geometry.n_ctx = 32;
        geometry.tile_len = 32;
        let artifact = crate::artifact::Base0ArtifactV1::derive_deterministic(
            crate::artifact::Base0ShapeV1 {
                n_layers: geometry.layer_count as usize,
                n_heads: geometry.attn_heads as usize,
                n_kv_heads: geometry.attn_heads as usize,
                d_head: geometry.attn_head_dim as usize,
                d_ff: geometry.ffn_dim as usize,
                vocab: geometry.vocab_size as usize,
                max_position: geometry.n_ctx as usize,
                ln_theta_gen_q: crate::artifact::LN_THETA_10000_GEN_Q,
                eps_q: geometry.rms_eps_q,
            },
            PALW_RC_BASE0_SEED,
        )
        .expect("the fixture shape is valid");
        let profile = base0_profile_v1(geometry).expect("expressible");
        let (ctx, prompt) = base0_rc_job_v1(&profile, Hash64::from_u64_word(0xA9C40), geometry.vocab_size as usize, 3, 3);
        (artifact, profile, ctx, prompt)
    }

    /// **The whole point, end to end: capture a checkpoint, resume from it, and refute from it.**
    ///
    /// `small_job` is prefill 3 / decode 3, so there are two decode CALLS and — at interval 1 —
    /// two checkpoints. Checkpoint 0 covers call 1; resuming from it costs ONE call, where a
    /// genesis-anchored replay of the same dispute costs the prefill's three positions plus both
    /// decode calls.
    ///
    /// Three claims, and the third is the one that makes the first two worth anything:
    ///
    /// 1. the resumed call reproduces the committed leaves **exactly** — same indices, same
    ///    hashes, so a court can compare by index instead of walking an execution;
    /// 2. it costs one call rather than the whole inference;
    /// 3. a single flipped byte in the committed state makes the replay disagree — so this is a
    ///    refutation channel and not merely a shortcut that happens to agree.
    #[test]
    fn a_committed_checkpoint_is_resumed_from_and_refuted_from() {
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");

        let decode_calls = ctx.exact_decode_tokens - 1;
        assert_eq!(run.checkpoints.leaves.len() as u32, decode_calls, "one checkpoint per decode call at interval 1");
        assert_eq!(run.binding.checkpoint_count, decode_calls);
        assert_eq!(run.binding.checkpoint_merkle_root, run.checkpoints.merkle_root, "the binding carries the captured root");
        assert_ne!(run.binding.checkpoint_merkle_root, Hash64::default(), "a zero root is the placeholder this replaced");

        // Resume from checkpoint 0, which covers decode call 1, and replay only call 2.
        let ckpt = &run.checkpoints.leaves[0];
        assert_eq!(ckpt.covered_decode_call, 1);
        let replay = base0_replay_from_checkpoint_v1(
            &artifact,
            &profile,
            &ctx,
            ckpt,
            &run.checkpoints.chunks[0],
            // The token call 2 consumes is the one call 1 produced.
            run.generated_token_ids[ckpt.covered_decode_call as usize],
            1,
        )
        .expect("the committed state resumes");

        assert_eq!(replay.calls_replayed, 1, "one call, not the whole inference");
        // Substantive, not one leaf: a call that filled a handful would make the comparison
        // vacuous while still passing an is-empty check.
        assert!(replay.leaf_hashes.len() >= 16, "a replayed call filled only {} leaves", replay.leaf_hashes.len());
        for (index, hash) in &replay.leaf_hashes {
            assert_eq!(
                run.tiles.leaves[*index as usize], *hash,
                "leaf {index} replayed from the checkpoint differs from the committed one"
            );
        }
        assert_eq!(
            replay.generated_token_ids[0],
            run.generated_token_ids[(ckpt.covered_decode_call + 1) as usize],
            "the resumed call produces the token the claim committed"
        );

        // (3) The refutation channel. One byte of the committed state, flipped: the resumed call
        // now computes something else, and the disagreement is exactly what a court reads.
        let mut tampered = run.checkpoints.chunks[0].clone();
        tampered[0][0] ^= 1;
        let refuted = base0_replay_from_checkpoint_v1(
            &artifact,
            &profile,
            &ctx,
            ckpt,
            &tampered,
            run.generated_token_ids[ckpt.covered_decode_call as usize],
            1,
        )
        .expect("a tampered state still runs — it is the RESULT that must differ");
        assert!(
            refuted.leaf_hashes.iter().any(|(index, hash)| run.tiles.leaves[*index as usize] != *hash),
            "a flipped state byte produced identical leaves — the replay is not reading the state"
        );

        // And the tampered chunks do not re-derive the committed leg, so a seat refuses them
        // before any court is opened.
        let rebuilt = crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(
            &ctx,
            &profile,
            &run.binding.checkpoint_profile,
            &[tampered, run.checkpoints.chunks[1].clone()],
        )
        .expect("wrong bytes of the right length still re-derive a leg");
        assert_ne!(rebuilt.merkle_root, run.binding.checkpoint_merkle_root, "a tampered chunk must move the leg root");
    }

    /// **The unit, closed: every leaf of the job is adjudicated through the anchor the leg gives
    /// it, and every one of them agrees with the commitment.**
    ///
    /// This is the difference between "a resume primitive exists" and "capture → restore →
    /// anchored replay is a thing you can hand a court". Nothing here picks a checkpoint by hand:
    /// the leaf index names a call, the call selects the last checkpoint strictly before it, and
    /// the replay runs from there.
    ///
    /// The `Genesis` arm is asserted too, not skipped. A leaf in the prefill call has no anchor by
    /// construction, and a unit that quietly returned "agrees" for those would be reporting
    /// agreement it never checked.
    #[test]
    fn every_leaf_is_adjudicated_through_the_anchor_its_leg_gives_it() {
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");

        let mut anchored = 0u32;
        let mut genesis = 0u32;
        let mut worst_calls = 0u32;
        for leaf_index in 0..run.binding.step_leaf_count {
            let (hash, source) =
                base0_anchored_leaf_replay_v1(&artifact, &profile, &ctx, &run.checkpoints, &run.generated_token_ids, leaf_index)
                    .expect("every main leaf resolves an anchor");
            match source {
                Base0ReplaySourceV1::Genesis { .. } => {
                    assert!(hash.is_none(), "a genesis-anchored leaf must not report a replayed hash");
                    genesis += 1;
                }
                Base0ReplaySourceV1::Checkpoint { covered_decode_call, calls, .. } => {
                    let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, leaf_index)
                        .expect("a main leaf has coordinates");
                    // The anchor is strictly before the disputed call — resuming from a checkpoint
                    // that covered it would skip the step under dispute.
                    assert!(covered_decode_call < coord.call_index, "the anchor is not strictly before the disputed call");
                    assert_eq!(calls, coord.call_index - covered_decode_call);
                    let hash = hash.expect("an anchored leaf is reached by its own replay");
                    assert_eq!(hash, run.tiles.leaves[leaf_index as usize], "leaf {leaf_index} disagrees with the commitment");
                    worst_calls = worst_calls.max(calls);
                    anchored += 1;
                }
            }
        }
        assert!(anchored > 0 && genesis > 0, "the fixture must exercise both arms (anchored {anchored}, genesis {genesis})");
        // At interval 1 the nearest checkpoint is always one call back, so no anchored leaf ever
        // costs more than a single call — the property the whole leg exists to buy.
        assert_eq!(worst_calls, 1, "an anchored dispute cost {worst_calls} calls at interval 1");
    }

    /// **The two sides of the cost ceiling agree, on a real close.**
    ///
    /// `derive_court_cost_v1` bounds what prosecuting a CLASS costs, from its graph, at admission.
    /// `arithmetic_close_bytes_v2` measures what an OBJECT costs, on the wire, at acceptance. The
    /// ceiling between them is inside `palw_ruleset_id_v2`, so if the two ever measure different
    /// quantities the network admits classes it cannot police, or refuses closes it admitted the
    /// class for — and neither failure announces itself.
    ///
    /// Nothing but a round trip finds that. This assembles the RC floor's most expensive close from
    /// a real execution — the KV-reading attention matmul at the last position of the longest job
    /// the class admits, anchored, which is the form an honest challenger builds — and asserts the
    /// derived bound covers what the real object weighs.
    ///
    /// It is a BOUND, not an equality: the derivation is over the class's worst node at its worst
    /// job, and any one close is at most that. The second assertion keeps the bound from going
    /// slack enough to stop meaning anything.
    #[test]
    fn the_derived_close_cost_bounds_a_real_one() {
        use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, PALW_RC_BASE0_WORST_CASE, base0_profile_v1};
        use kaspa_consensus_core::palw_class_admission_v2::derive_court_cost_v1;
        use kaspa_consensus_core::palw_court_v2::{PalwCourtVerdictProofV2, arithmetic_close_bytes_v2};
        use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepOpKindV1};

        let artifact = crate::rc::palw_rc_base0_artifact_v1().expect("the floor's artifact derives");
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let (prefill, decode) = PALW_RC_BASE0_WORST_CASE;
        let anchor_id = Hash64::from_u64_word(0xC057);
        let (ctx, prompt) = base0_rc_job_v1(&profile, anchor_id, artifact.shape.vocab, prefill, decode);
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the longest job runs");
        let binding = crate::legs::base0_binding_from_capture_v1(
            &profile,
            &ctx,
            &run.tiles,
            &run.checkpoints,
            Hash64::default(),
            Hash64::default(),
        )
        .expect("a capture yields its own commitment");

        // The node whose close costs most: an attention matmul reading the cached keys. Found by
        // ROLE rather than by slot number, so a graph edit moves the test instead of silencing it.
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let mut worst = 0u64;
        for slot in 0..profile.global_node_count() {
            let Some((node, layer)) = profile.resolve_node_slot(slot) else { continue };
            if layer != Some(0) || node.op_kind != PalwStepOpKindV1::MatMulQuant || !node.weight_name.is_empty() {
                continue;
            }
            // The last decode call, where the history is longest and an anchor exists.
            let call = decode - 1;
            let kv_anchor = crate::legs::base0_kv_anchor_for_call_v1(&run.checkpoints, call);
            let target = PalwStepCoordinateV1 { call_index: call, node_slot: slot, position: 0, tile_index: 0 };
            let Ok(refutation) = crate::legs::base0_refutation_from_capture_v1(
                &profile,
                &ctx,
                &run.tiles,
                binding.clone(),
                target,
                ids.clone(),
                None,
                kv_anchor,
            ) else {
                continue;
            };
            let proof = PalwCourtVerdictProofV2::Arithmetic { refutation, operand_openings: Vec::new() };
            worst = worst.max(arithmetic_close_bytes_v2(&proof).expect("an arithmetic proof has a size"));
        }
        assert!(worst > 0, "no KV-reading attention step assembled — the fixture found nothing to measure");

        let derived = derive_court_cost_v1(&profile).expect("derivable").max_close_bytes;
        assert!(worst <= derived, "a real close ({worst}) must fit the bound admission checked ({derived})");
        assert!(worst * 4 >= derived, "the bound has gone slack: {worst} real against {derived} derived");
    }

    /// **What the leg costs to retain and serve, pinned.**
    ///
    /// This is a cost the unit CREATED. Before it, `checkpoint_merkle_root` was a zero placeholder
    /// and a producer retained nothing for it; now real state travels in the material a producer
    /// must keep and serve. A number nobody measured is a number that grows, so it is asserted
    /// here against the map's own arithmetic rather than left to be discovered on a fleet.
    ///
    /// At `checkpoint_interval = 1` the producer takes a checkpoint per decode call — the most
    /// expensive end for the producer and the cheapest for a disputant (never more than one call to
    /// replay). That is a producer-side trade, and this test is where a change to it becomes
    /// visible instead of silent.
    #[test]
    fn the_checkpoint_leg_costs_what_the_map_says_it_costs() {
        use kaspa_consensus_core::palw_state_chunk_map as map;
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");

        let row = profile.attn_kv_heads as u64 * profile.attn_head_dim as u64;
        let layers = (0..profile.layer_count)
            .filter(|l| profile.layer_kind(*l) == kaspa_consensus_core::palw_step::PalwLayerKindV1::Attention)
            .count() as u64;

        let mut total = 0u64;
        for (i, leaf) in run.checkpoints.leaves.iter().enumerate() {
            let positions = map::integer_kv_positions_at_v1(&ctx, leaf.covered_decode_call) as u64;
            // K and V, every attention layer, one byte per element — the map, restated from the
            // geometry so the test cannot pass by agreeing with the code it is checking.
            let expected = 2 * layers * positions * row;
            let got: u64 = run.checkpoints.chunks[i].iter().map(|c| c.len() as u64).sum();
            assert_eq!(got, expected, "checkpoint {i} retains {got} bytes, the map implies {expected}");
            total += got;
        }

        // The whole leg, for this job's shape. `small_job` is prefill 3 / decode 3 on a 2-layer,
        // 2-head, 32-wide fixture: rows are 64 bytes, checkpoints cover calls 1 and 2 at 4 and 5
        // positions, so 2 × 2 × (4 + 5) × 64.
        assert_eq!(row, 64);
        assert_eq!(layers, 2);
        assert_eq!(total, 2 * 2 * (4 + 5) * 64, "the leg's retained size moved");

        // And it is what actually ships: the material carries these bytes and nothing else for the
        // leg — the leaves are re-derived, never sent.
        let encoded = base0_material_encode_v1(&run).expect("encodes");
        let bare = borsh::to_vec(&(&run.binding, &run.tiles.tiles, &run.logits_rows, &run.generated_token_ids))
            .expect("encodes without the leg");
        let overhead = encoded.len() as u64 - bare.len() as u64;
        assert!(
            overhead >= total && overhead < total + 256,
            "the leg adds {overhead} bytes to the material for {total} bytes of state — framing should be the only difference"
        );
    }

    /// **The unit at its own caller's boundary: served bytes in, one leaf's verdict out.**
    ///
    /// Three things, and the middle one is the soundness of the whole arrangement:
    ///
    /// 1. honest material agrees at an anchored leaf, and reports `None` (never `false`) at a
    ///    genesis-anchored one;
    /// 2. a producer that lies about a STEP and hands over checkpoints consistent with the lie is
    ///    refused before anything resumes — the anchor must be the one the CLAIM committed, or
    ///    there is no anchored check to do;
    /// 3. a producer that lies about a step and keeps its honest checkpoints is caught by the
    ///    replay, which is the case the leg exists for.
    #[test]
    fn served_bytes_in_one_leafs_verdict_out() {
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        let honest: Base0RetainedMaterialV1 = (
            run.binding.clone(),
            run.tiles.tiles.clone(),
            run.logits_rows.clone(),
            run.generated_token_ids.clone(),
            run.checkpoints.chunks.clone(),
        );

        // (1) A leaf in the last decode call — anchored — and one in the prefill — not.
        let leaf_in = |call: u32| {
            (0..run.binding.step_leaf_count)
                .find(|i| {
                    kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, *i)
                        .is_some_and(|c| c.call_index == call)
                })
                .expect("the call has leaves")
        };
        let anchored = base0_anchored_leaf_check_v1(&artifact, &honest, leaf_in(ctx.exact_decode_tokens - 1)).expect("checks");
        assert!(matches!(anchored.source, Base0ReplaySourceV1::Checkpoint { .. }));
        assert_eq!(anchored.agrees(), Some(true), "honest material must agree at an anchored leaf");

        let prefill = base0_anchored_leaf_check_v1(&artifact, &honest, leaf_in(0)).expect("checks");
        assert_eq!(prefill.source, Base0ReplaySourceV1::Genesis { calls: 0 });
        assert_eq!(prefill.agrees(), None, "not reached must never read as agreement OR disagreement");

        // (2) Lie about a step AND hand over checkpoints consistent with the lie. The leg no longer
        // re-derives the binding's root, so the check refuses rather than resuming from the
        // attacker's chosen state.
        let mut forged = honest.clone();
        forged.4[0][0][0] ^= 1;
        let err = base0_anchored_leaf_check_v1(&artifact, &forged, leaf_in(ctx.exact_decode_tokens - 1))
            .expect_err("checkpoints that are not the committed ones must be refused");
        assert!(format!("{err:?}").contains("not the ones the binding committed"), "{err:?}");

        // (3) Lie about a STEP, keep the honest checkpoints. The replay is what catches this, and
        // it is the case the leg exists for.
        let target = leaf_in(ctx.exact_decode_tokens - 1);
        let mut lying = honest.clone();
        {
            let slot = lying.1.iter_mut().find(|(i, _)| *i == target).expect("the tile is held");
            slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
        }
        let caught = base0_anchored_leaf_check_v1(&artifact, &lying, target).expect("checks");
        assert_eq!(caught.agrees(), Some(false), "a tampered step must disagree with its anchored replay");
        assert_eq!(caught.source.calls(), 1, "and it costs one call to say so");
    }

    /// The anchor is read from the COMMITTED leaves, so a leg with a checkpoint missing moves the
    /// anchor back rather than inventing one — and the bytes it resumes from are still that
    /// checkpoint's own.
    ///
    /// The second half is not decoration. `chunks` is positional and `checkpoint_index` is a field;
    /// they coincide only in a complete leg, and a dispute is precisely where one might not be.
    #[test]
    fn a_missing_checkpoint_moves_the_anchor_back_instead_of_inventing_one() {
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");

        // A leaf in the last decode call. Checkpoint `c` covers call `c + 1`, and the anchor is the
        // last one STRICTLY before the disputed call — so call 2's anchor is checkpoint 0.
        let last_call = ctx.exact_decode_tokens - 1;
        let leaf_index = (0..run.binding.step_leaf_count)
            .find(|i| {
                kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, *i)
                    .is_some_and(|c| c.call_index == last_call)
            })
            .expect("the last call has leaves");
        let (hash, full) =
            base0_anchored_leaf_replay_v1(&artifact, &profile, &ctx, &run.checkpoints, &run.generated_token_ids, leaf_index)
                .expect("resolves");
        assert_eq!(full, Base0ReplaySourceV1::Checkpoint { index: last_call - 2, covered_decode_call: last_call - 1, calls: 1 });
        assert_eq!(hash.expect("reached"), run.tiles.leaves[leaf_index as usize]);

        // Remove the checkpoint that WAS the anchor. Nothing committed now precedes the disputed
        // call, so the honest answer is `Genesis` — never the later checkpoint, which covers the
        // very call under dispute.
        let mut thinned = crate::legs::Base0CheckpointsV1 {
            leaves: run.checkpoints.leaves.clone(),
            leaf_hashes: run.checkpoints.leaf_hashes.clone(),
            merkle_root: run.checkpoints.merkle_root,
            chunks: run.checkpoints.chunks.clone(),
            bytes_serialised: run.checkpoints.bytes_serialised,
        };
        thinned.leaves.remove(0);
        thinned.leaf_hashes.remove(0);
        thinned.chunks.remove(0);
        let (hash, thin) = base0_anchored_leaf_replay_v1(&artifact, &profile, &ctx, &thinned, &run.generated_token_ids, leaf_index)
            .expect("resolves against the thinner leg");
        assert_eq!(thin, Base0ReplaySourceV1::Genesis { calls: last_call });
        assert!(hash.is_none(), "a genesis anchor reports no replayed hash");
        assert!(thin.calls() >= full.calls(), "a thinner leg cannot make a dispute cheaper");

        // And a leaf whose anchor SURVIVES the removal still resumes from that checkpoint's own
        // bytes — the positional lookup, exercised on a leg where the field and the position have
        // stopped agreeing (`leaves[0].checkpoint_index` is now 1).
        assert_eq!(thinned.leaves[0].checkpoint_index, 1, "the fixture must actually desynchronise them");
        let later_leaf = (0..run.binding.step_leaf_count)
            .find(|i| {
                kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, *i).is_some_and(|c| c.call_index == 0)
            })
            .expect("the prefill call has leaves");
        let (_, prefill_source) =
            base0_anchored_leaf_replay_v1(&artifact, &profile, &ctx, &thinned, &run.generated_token_ids, later_leaf)
                .expect("resolves");
        assert_eq!(prefill_source, Base0ReplaySourceV1::Genesis { calls: 0 }, "the prefill call never has an anchor");
    }

    /// A seat's `Valid` now covers the checkpoint leg too: served chunks that do not re-derive the
    /// committed root are material that does not match the claim.
    #[test]
    fn a_seat_refuses_material_whose_checkpoints_do_not_rebuild() {
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        let honest: Base0RetainedMaterialV1 = (
            run.binding.clone(),
            run.tiles.tiles.clone(),
            run.logits_rows.clone(),
            run.generated_token_ids.clone(),
            run.checkpoints.chunks.clone(),
        );
        assert!(base0_material_matches_claim_v1(&honest, run.execution_root, run.trace_root).expect("checkable"));

        let mut chunks = run.checkpoints.chunks.clone();
        chunks[1][0][0] ^= 1;
        let served: Base0RetainedMaterialV1 =
            (run.binding.clone(), run.tiles.tiles.clone(), run.logits_rows.clone(), run.generated_token_ids.clone(), chunks);
        assert!(
            !base0_material_matches_claim_v1(&served, run.execution_root, run.trace_root).expect("checkable"),
            "a tampered checkpoint chunk must fail the seat's check"
        );

        // And withholding them entirely is not a way to pass.
        let empty: Base0RetainedMaterialV1 =
            (run.binding.clone(), run.tiles.tiles.clone(), run.logits_rows.clone(), run.generated_token_ids.clone(), Vec::new());
        assert!(
            !base0_material_matches_claim_v1(&empty, run.execution_root, run.trace_root).expect("checkable"),
            "material with no checkpoints must fail a claim that committed some"
        );
    }

    /// **The capture covers the WHOLE step space** — which is the property the roots are worth
    /// nothing without.
    ///
    /// Before the pre and post tables were captured, a leg committed zero leaves for the embedding
    /// gather, the final norm, its narrowing and the logits head — so the node that decides what
    /// the model actually said was the one part of the graph no refutation could open, and the
    /// commitment could not tell "computed zero" from "never computed". `finish` refuses a short
    /// capture now, so this test failing is the same event as a producer refusing to publish.
    /// **What a claim's `trace_root` IS, pinned across the crate boundary the court reads it over.**
    ///
    /// `palw_producer.rs` puts `Base0ExecutionV1::trace_root` on the claim, and the court's close
    /// binding compares that value against a field of the refutation's binding. Which field is not
    /// a matter of taste: only `full_logits_trace_root` can ever equal it, because this is where
    /// the one is passed into the other. `step_merkle_root` is a different root over different
    /// leaves, pinned transitively through `committed_execution_root`.
    ///
    /// The court compared against `step_merkle_root`, so every close on a real claim failed
    /// `TraceRootMismatch` before reading any evidence — no fraud convictable, no honest producer
    /// able to clear itself. The court-side tests did not catch it because they built their claim
    /// by assigning `trace_root = binding.step_merkle_root`, which is the reverse of the line
    /// below. This test exists so that correspondence is asserted where it is actually created.
    #[test]
    fn a_claims_trace_root_is_the_bindings_logits_root() {
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        assert_eq!(
            run.trace_root, run.binding.full_logits_trace_root,
            "the claim's trace root and the binding's logits root are one value"
        );
        assert_ne!(
            run.trace_root, run.binding.step_merkle_root,
            "and the step root is a different root — comparing against it can only ever fail"
        );
        assert_eq!(run.execution_root, run.binding.committed_execution_root, "the execution root is the binding's own");
    }

    #[test]
    fn an_honest_execution_fills_every_leaf_of_its_step_space() {
        let (artifact, profile, ctx, prompt) = small_job();
        let expected = kaspa_consensus_core::palw_step::step_leaf_count(&profile, &ctx).expect("the job has a step space");
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        assert_eq!(run.tiles.leaves.len() as u64, expected);
        assert!(
            run.tiles.leaves.iter().all(|l| *l != Hash64::default()),
            "a leaf nobody filled is a leaf the commitment claims is zero"
        );
        assert_eq!(run.generated_token_ids.len(), ctx.exact_decode_tokens as usize, "one output token per decode token");
        assert_eq!(run.binding.step_leaf_count, expected, "the binding commits the space it covered");
    }

    /// **A real execution's own roots survive its own court** (audit C-01's round trip, end to end).
    ///
    /// Every prior version of this path stopped at "the checker exists". This runs the engine,
    /// commits, then asks the court to convict — at a coordinate in the POST table, which is the
    /// region that did not exist in any capture until now. `NoFaultFound` is the honest verdict,
    /// and the same function produces a conviction from a tampered capture, which is what makes
    /// the honest verdict mean anything.
    #[test]
    fn the_court_finds_no_fault_in_an_honest_post_table_step() {
        use kaspa_consensus_core::palw_step::PalwStepCoordinateV1;
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");

        // `output_norm.requant` of the last decode call — the head's narrowing, a POST-table node
        // with a real weight operand, at the coordinate the enumeration puts it at. Nothing could
        // target this before the post table was captured: the leaf was zero.
        let post_slot = profile.global_node_slot(PalwStepTableV1::Post, 0, 1).expect("the post table has a narrowing");
        let target =
            PalwStepCoordinateV1 { call_index: ctx.exact_decode_tokens - 1, node_slot: post_slot, position: 0, tile_index: 0 };
        let refutation = crate::legs::base0_refutation_from_capture_v1(
            &profile,
            &ctx,
            &run.tiles,
            run.binding.clone(),
            target,
            prompt.iter().map(|t| *t as u32).collect(),
            None,
            None,
        )
        .expect("a coordinate the capture covers produces a refutation");

        let mut geometry = PALW_RC_BASE0_GEOMETRY;
        geometry.layer_count = 2;
        geometry.hidden_dim = 64;
        geometry.ffn_dim = 128;
        geometry.attn_heads = 2;
        geometry.attn_head_dim = 32;
        geometry.vocab_size = 128;
        geometry.n_ctx = 32;
        geometry.tile_len = 32;
        let inventory = crate::inventory::base0_inventory_v1(&artifact, geometry).expect("a real inventory");
        let artifact_root = inventory.root();
        let openings: Vec<_> = (0..inventory.operands().len())
            .filter(|i| inventory.operands()[*i].tensor_name == "output_norm.requant")
            .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
            .collect();
        assert!(!openings.is_empty(), "the head's narrowing is in the inventory");
        let oracle = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, artifact_root)
            .expect("the narrowing's row proves against the artifact root");

        let verdict = kaspa_consensus_core::palw_step_refute::check_execution_step_refutation_v1(&refutation, &oracle);
        assert!(
            matches!(verdict, Err(kaspa_consensus_core::palw_step_refute::PalwStepRefuteError::NoFaultFound)),
            "an honest execution is not convicted by its own evidence: {verdict:?}"
        );
    }

    /// **The court reads a checkpoint-anchored KV history and reaches the SAME verdict.**
    ///
    /// This is the consumption side of the checkpoint leg, and the property that has to hold is
    /// equivalence, not merely "it also works": the anchored route and the long route are two
    /// encodings of the same committed rows, so a court must not be able to tell them apart by the
    /// answer it gives. If they could differ, a challenger would pick whichever route convicts.
    ///
    /// Four claims:
    ///
    /// 1. the anchored refutation opens **far fewer** leaves — that is the entire point;
    /// 2. it reaches the same verdict on honest material as the long form;
    /// 3. a tampered chunk is REFUSED, not silently ignored and not quietly convicting;
    /// 4. an anchor for the wrong call is refused by name.
    #[test]
    fn an_anchored_kv_history_reaches_the_same_verdict_as_the_long_one() {
        use kaspa_consensus_core::palw_step::PalwStepCoordinateV1;
        use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, check_execution_step_refutation_v1};

        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");

        // The scores node: `[Step(q_rot), CachedK]` — a MIXED input set, which is what the
        // anchored form has to cope with, at the last decode call so an anchor exists.
        let call = ctx.exact_decode_tokens - 1;
        let (idx, node) = profile
            .attn_nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.input_refs.contains(&kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_K))
            .expect("BASE-0's attention reads the K cache");
        assert!(node.input_refs.len() > 1, "the fixture must exercise a MIXED input set");
        let slot = profile.global_node_slot(PalwStepTableV1::Attn, 0, idx).expect("the node has a global slot");
        let target = PalwStepCoordinateV1 { call_index: call, node_slot: slot, position: 0, tile_index: 0 };

        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let build = |anchor: Option<kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1>| {
            crate::legs::base0_refutation_from_capture_v1(
                &profile,
                &ctx,
                &run.tiles,
                run.binding.clone(),
                target,
                ids.clone(),
                None,
                anchor,
            )
            .expect("a coordinate the capture covers produces a refutation")
        };

        let long = build(None);
        let anchor = crate::legs::base0_kv_anchor_for_call_v1(&run.checkpoints, call).expect("the leg has this call's anchor");
        let anchored = build(Some(anchor.clone()));

        // (1) The cost. The long form opens the whole cached history; the anchored one opens this
        // call's own write and nothing else.
        // **Pinned, not merely "fewer".** `assert!(a < b)` passes at 21 against 20 and would let
        // the whole point quietly evaporate, so the numbers are derived from the geometry and
        // compared.
        //
        // The scores node is `[Step(q_rot), CachedK]`. The query row is one position; the cache is
        // the whole history — `prefill + call` positions — and each row is `kv_dim` wide, tiled at
        // the node's `tile_len`. Anchored, the cache contributes ONE position.
        // Under the range carrier `inputs` is ROWS (one per ref), so the history lives in the
        // KV row's PREIMAGE count — same pinned arithmetic, one level down.
        let tiles_of = |elements: u32| elements.div_ceil(profile.attn_nodes[idx].tile_len) as usize;
        let kv_dim = profile.attn_kv_heads as u32 * profile.attn_head_dim;
        let history = ctx.declared_prefill_tokens + call;
        assert_eq!(long.inputs.len(), 2, "the scores node reads two refs");
        assert_eq!(anchored.inputs.len(), 2, "anchored too — the anchor replaces leaves, not rows");
        let leaves = |r: &kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1| {
            (r.inputs[0].preimages.len(), r.inputs[1].preimages.len())
        };
        assert_eq!(
            leaves(&long),
            // The query row is two tiles whatever the cache is; the SECOND ref is the history.
            (2, history as usize * tiles_of(kv_dim)),
            "the long set is not the history it should be"
        );
        assert_eq!(leaves(&anchored), (2, tiles_of(kv_dim)), "the anchored set is not one cached position");
        // On this fixture: 12 → 4 total leaves, the cache's share 10 → 2. On the RC's worst-case
        // shape (prefill 64, decode 64, kv_dim 256, tile_len 64) the same arithmetic is 508 → 4.
        let total = |r: &kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1| {
            r.inputs.iter().map(|i| i.preimages.len()).sum::<usize>()
        };
        assert_eq!((total(&long), total(&anchored)), (12, 4), "the fixture's opening counts moved");

        let oracle = kaspa_consensus_core::palw_step_refute::PalwNoWeightsV1;
        let verdict_of = |r: &kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1| {
            format!("{:?}", check_execution_step_refutation_v1(r, &oracle))
        };
        // (2) Equivalence on honest material. The scores node is weightless, so `PalwNoWeightsV1`
        // is enough and the answer is about the KV history and nothing else.
        assert_eq!(verdict_of(&long), verdict_of(&anchored), "the two encodings of one history disagree");
        // And it is the HONEST verdict, not a shared `Unadjudicable`: two routes that both refuse
        // to answer agree about nothing.
        assert!(
            matches!(check_execution_step_refutation_v1(&anchored, &oracle), Err(PalwStepRefuteError::NoFaultFound)),
            "the anchored route must reach the merits, got {}",
            verdict_of(&anchored)
        );

        // (2b) **And it convicts.** A route that can only acquit is not an adjudication. One lane
        // of the challenged tile is tampered, the binding is re-derived from the tampered capture
        // so its roots are its own, and both routes must convict — identically.
        let index = kaspa_consensus_core::palw_step::canonical_step_leaf_index(&profile, &ctx, &target).expect("canonical");
        let mut lying = run.tiles.clone();
        {
            let slot = lying.tiles.iter_mut().find(|(i, _)| *i == index).expect("the capture holds the tile");
            slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
            lying.leaves[index as usize] =
                kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx.context_hash(), &profile.shape_profile_id(), &slot.1);
        }
        let lying_binding = crate::legs::base0_binding_from_capture_v1(
            &profile,
            &ctx,
            &lying,
            &run.checkpoints,
            run.trace_root,
            base0_activation_leg_root_v1(&ctx),
        )
        .expect("a tampered capture still commits to itself");
        let lying_anchor =
            crate::legs::base0_kv_anchor_for_call_v1(&run.checkpoints, call).expect("the honest leg still has this anchor");
        let build_lying = |a: Option<kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1>| {
            crate::legs::base0_refutation_from_capture_v1(&profile, &ctx, &lying, lying_binding.clone(), target, ids.clone(), None, a)
                .expect("assembles")
        };
        let convicted_long = check_execution_step_refutation_v1(&build_lying(None), &oracle);
        let convicted_anchored = check_execution_step_refutation_v1(&build_lying(Some(lying_anchor)), &oracle);
        assert!(convicted_long.is_ok(), "the long route must convict a tampered tile, got {convicted_long:?}");
        assert_eq!(
            format!("{convicted_long:?}"),
            format!("{convicted_anchored:?}"),
            "the anchored route reaches a different conviction than the long one"
        );

        // (3) A tampered chunk. The leg root moves, so the anchor stops being the claim's.
        let mut bad = anchor.clone();
        bad.chunks[0][0] ^= 1;
        let err = check_execution_step_refutation_v1(&build(Some(bad)), &oracle).expect_err("a tampered anchor must be refused");
        assert!(
            matches!(err, PalwStepRefuteError::InputSetNotCanonical(m) if m.contains("state root")),
            "a tampered chunk must be refused by name, got {err:?}"
        );

        // (4) An anchor for another call. `covered_decode_call` must be exactly `call − 1`.
        if let Some(other) = crate::legs::base0_kv_anchor_for_call_v1(&run.checkpoints, call - 1) {
            let err = check_execution_step_refutation_v1(&build(Some(other)), &oracle)
                .expect_err("an anchor for another call must be refused");
            assert!(matches!(err, PalwStepRefuteError::InputSetNotCanonical(m) if m.contains("this step's anchor")), "got {err:?}");
        }
    }

    /// **The bisect half, on real committed material: a ladder seeded from the producer's own leg.**
    ///
    /// `open_anchored` takes an index and a state and cannot check where they came from. This is
    /// the caller that makes them derived rather than chosen: the state is a committed checkpoint's
    /// own leaf hash, and the index is the first step leaf of the first call that checkpoint does
    /// NOT cover — found by walking the space's own enumeration, not by arithmetic beside it.
    ///
    /// The property asserted is the one the anchor exists for: the ladder starts strictly inside
    /// the space, everything below the anchor is execution the checkpoint already commits to, and
    /// the interval left to bisect is smaller than the whole.
    #[test]
    fn a_ladder_seeded_from_the_committed_leg_starts_inside_the_space() {
        use kaspa_consensus_core::palw_step::canonical_step_coordinates;
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");

        let covered = 1u32;
        // The CHAIN's identity inputs: the claim id the session was opened for, and the claim's
        // announced trace root — the two values `court_session_id_v2` reads.
        let claim_id = Hash64::from_u64_word(0xC7A1);
        let ladder = crate::legs::base0_anchored_ladder_v1(
            &profile,
            &ctx,
            &run.checkpoints,
            &run.binding,
            &claim_id,
            covered,
            &Hash64::from_u64_word(0xC1),
            &Hash64::from_u64_word(0xE2),
            100,
            200,
        )
        .expect("the leg has a checkpoint at this call and room left to bisect");

        let (lo, hi) = ladder.interval();
        assert_eq!(hi, run.binding.step_leaf_count, "the ladder still spans to the end of the space");
        assert!(lo > 0 && lo < hi, "the anchor must start the ladder strictly inside the space, got {lo}..{hi}");

        // Everything below the anchor belongs to calls the checkpoint covers; the anchor itself is
        // the first leaf of the first call it does not. That is the whole claim about correctness —
        // an anchor one leaf too high would skip a step the dispute may be about.
        assert!(
            canonical_step_coordinates(&profile, &ctx, lo).expect("canonical").call_index > covered,
            "the anchor leaf belongs to a call the checkpoint already covers"
        );
        assert!(
            canonical_step_coordinates(&profile, &ctx, lo - 1).expect("canonical").call_index <= covered,
            "the leaf before the anchor is NOT covered — the anchor is too high and skips execution"
        );

        // Same session as the ladder the V2 TRANSITION opens — `open(claim_id, claim.trace_root,
        // …)`, the claim's announced trace root being the binding's logits trace root. A court
        // derives the id from the claim, and a ladder whose id moved is a ladder no court accepts;
        // comparing against a plain ladder built from the binding's internals (as this test once
        // did) only proved the two wrong derivations agreed with each other.
        let plain = kaspa_consensus_core::palw_bisect::PalwBisectLadderV1::open(
            &claim_id,
            &run.binding.full_logits_trace_root,
            &Hash64::from_u64_word(0xC1),
            &Hash64::from_u64_word(0xE2),
            kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
            run.binding.step_leaf_count,
            100,
            200,
        )
        .expect("opens");
        assert_eq!(ladder.session_id(), plain.session_id());
        assert_eq!(plain.interval(), (0, run.binding.step_leaf_count));
    }

    /// The fixture geometry, as a `PalwBase0GeometryV1` — needed to build the inventory oracle.
    fn small_geometry() -> kaspa_consensus_core::palw_base0_profile::PalwBase0GeometryV1 {
        let mut g = PALW_RC_BASE0_GEOMETRY;
        g.layer_count = 2;
        g.hidden_dim = 64;
        g.ffn_dim = 128;
        g.attn_heads = 2;
        g.attn_head_dim = 32;
        g.vocab_size = 128;
        g.n_ctx = 32;
        g.tile_len = 32;
        g
    }

    /// **The court must not convict an honest execution — at EVERY leaf, not at a chosen one.**
    ///
    /// Three arithmetic divergences between the engine and the adjudicator survived every
    /// single-coordinate test in this tree, because each needs a geometry with more than one head
    /// and a position past the first:
    ///
    /// * SoftMax — the engine runs one per query head and appends head-major; the court ran ONE
    ///   over the whole concatenation. Every softmax leaf convicted.
    /// * RoPE — the court asked the rotary table at byte offset 0, i.e. always position 0's row,
    ///   and for the whole row's worth of pairs rather than one head's. At one head the widths
    ///   coincided and it convicted every position but the first; at more than one head the
    ///   oversized request failed instead, so the wrong-answer bug wore an `Unadjudicable` mask.
    /// * P·V — the V cache is `[position][kv_dim]` and the court read it as `[out_dim][in_dim]`,
    ///   the transpose. They agree only at `kv_len == 1`.
    ///
    /// `map_refutation_outcome` turns any verdict into `ExecutorGuilty`, so each of these was a
    /// challenger burning an honest producer's bond by opening a court on a correct step. A sweep
    /// is the only shape that finds them: it is the difference between "the checker runs" and "the
    /// checker is right".
    #[test]
    #[ignore]
    fn measure_retained_material_size() {
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        let bytes = base0_material_encode_v1(&run).unwrap();
        println!("small_job material = {} bytes over {} tiles", bytes.len(), run.tiles.tiles.len());
        let rc_artifact = crate::rc::palw_rc_base0_artifact_v1().expect("derives");
        let rc_profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
            kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
        )
        .expect("expressible");
        let (rc_job, rc_prompt) = base0_rc_job_v1(
            &rc_profile,
            kaspa_hashes::Hash64::from_u64_word(7),
            rc_artifact.shape.vocab,
            kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.0,
            kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.1,
        );
        let rc_run = base0_execute_for_attempt_v1(&rc_artifact, &rc_profile, &rc_job, &rc_prompt).expect("the floor runs");
        let rc_bytes = base0_material_encode_v1(&rc_run).unwrap();
        println!("RC floor material = {} bytes over {} tiles", rc_bytes.len(), rc_run.tiles.tiles.len());
    }

    #[test]
    fn the_court_convicts_no_leaf_of_an_honest_execution() {
        use kaspa_consensus_core::palw_step::{PalwStepOpKindV1, canonical_step_coordinates};
        use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, check_execution_step_refutation_v1};

        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        let leaves = kaspa_consensus_core::palw_step::step_leaf_count(&profile, &ctx).expect("the job has a step space");
        assert!(profile.attn_heads > 1, "a single-head geometry cannot see two of the three defects");

        // One oracle over the WHOLE inventory, proven against its own root — the production path,
        // not a stub that answers whatever is asked.
        let inventory = crate::inventory::base0_inventory_v1(&artifact, small_geometry()).expect("a real inventory");
        let root = inventory.root();
        let openings: Vec<_> = (0..inventory.operands().len())
            .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
            .collect();
        let oracle = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, root)
            .expect("the inventory proves against its own root");

        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        // The integer-leg pin: the run's own logits rows and ids, which every decode-side leaf is
        // adjudicated against (ADR-0049 Decision E). Carried on every refutation — a prefill leaf
        // simply never reads it.
        let pin = kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1::Base0V1(
            kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1 {
                logits_rows: run.logits_rows.clone(),
                generated_token_ids: run.generated_token_ids.clone(),
            },
        );
        let mut adjudicated = 0usize;
        let mut convicted: Vec<String> = Vec::new();
        for leaf in 0..leaves {
            let Some(coord) = canonical_step_coordinates(&profile, &ctx, leaf) else { continue };
            let refutation = match crate::legs::base0_refutation_from_capture_v1(
                &profile,
                &ctx,
                &run.tiles,
                run.binding.clone(),
                coord,
                ids.clone(),
                Some(pin.clone()),
                None,
            ) {
                Ok(r) => r,
                Err(e) => panic!("leaf {leaf} at {coord:?} could not even be assembled: {e:?}"),
            };
            match check_execution_step_refutation_v1(&refutation, &oracle) {
                Err(PalwStepRefuteError::NoFaultFound) => adjudicated += 1,
                // The decode-embed carve-out that used to sit here is CLOSED: the integer-leg
                // dispatch authenticates the generated ids against `base0_logits_trace_root_v1`,
                // so there is no coordinate this class reaches that the court cannot check —
                // and any NEW hole fails the sweep by name instead of hiding in a loose count.
                Err(PalwStepRefuteError::Unadjudicable) => {
                    let (n, _) = profile.resolve_node_slot(coord.node_slot).unwrap();
                    panic!(
                        "leaf {leaf} is unadjudicable — {:?} at call {} pos {} tile {}; every coordinate this class reaches must adjudicate",
                        n.op_kind, coord.call_index, coord.position, coord.tile_index
                    );
                }
                other => convicted
                    .push(format!("leaf {leaf} slot {} pos {} tile {}: {other:?}", coord.node_slot, coord.position, coord.tile_index)),
            }
        }
        assert!(
            convicted.is_empty(),
            "the court convicted {} honest leaves — a challenger could burn this producer's bond by opening a court on a CORRECT step:\n{}",
            convicted.len(),
            convicted.iter().take(12).cloned().collect::<Vec<_>>().join("\n")
        );
        // Every leaf of the space adjudicated NoFaultFound — the sweep is exhaustive now.
        assert_eq!(adjudicated, leaves as usize, "every leaf of a real execution adjudicates");
        println!("swept {leaves} leaves: {adjudicated} adjudicated NoFaultFound, 0 unadjudicable");

        // **The other half, and without it this test is worthless.** A court that convicts nothing
        // passes the sweep above by being broken. So one lane of one tile is tampered at each of
        // the three repaired node kinds, and the court must still convict — the arms were made
        // CORRECT, not permissive.
        let mut still_convicts = 0usize;
        for leaf in 0..leaves {
            let Some(coord) = canonical_step_coordinates(&profile, &ctx, leaf) else { continue };
            let Some((_, node_layer)) = profile.resolve_node_slot(coord.node_slot) else { continue };
            let Some((node, _)) = profile.resolve_node_slot(coord.node_slot) else { continue };
            let is_repaired = matches!(node.op_kind, PalwStepOpKindV1::SoftMax | PalwStepOpKindV1::RopeImrope)
                || (node.op_kind == PalwStepOpKindV1::MatMulQuant && node.weight_name.is_empty());
            // One position past the first, where two of the three defects only appear.
            if !is_repaired || node_layer != Some(0) || coord.position == 0 {
                continue;
            }
            let mut lying = run.tiles.clone();
            let index = kaspa_consensus_core::palw_step::canonical_step_leaf_index(&profile, &ctx, &coord).expect("canonical");
            let Some(slot) = lying.tiles.iter_mut().find(|(i, _)| *i == index) else { continue };
            slot.1.values_le[0] = slot.1.values_le[0].wrapping_add(1);
            let leaf_hash =
                kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx.context_hash(), &profile.shape_profile_id(), &slot.1);
            lying.leaves[index as usize] = leaf_hash;
            let binding = crate::legs::base0_binding_from_capture_v1(
                &profile,
                &ctx,
                &lying,
                &run.checkpoints,
                run.trace_root,
                base0_activation_leg_root_v1(&ctx),
            )
            .expect("a tampered capture still commits");
            let refutation = crate::legs::base0_refutation_from_capture_v1(
                &profile,
                &ctx,
                &lying,
                binding,
                coord,
                ids.clone(),
                Some(pin.clone()),
                None,
            )
            .expect("assembles");
            match check_execution_step_refutation_v1(&refutation, &oracle) {
                Ok(_) => still_convicts += 1,
                Err(PalwStepRefuteError::NoFaultFound) => {
                    panic!(
                        "a tampered {:?} tile at slot {} position {} was NOT convicted — the arm is permissive, not correct",
                        node.op_kind, coord.node_slot, coord.position
                    )
                }
                Err(PalwStepRefuteError::Unadjudicable) => {}
                Err(e) => panic!("unexpected: {e:?}"),
            }
        }
        assert!(still_convicts > 0, "no tampered tile was convicted — the sweep above proves nothing");
        println!("and {still_convicts} tampered tiles at the repaired nodes were convicted");
    }

    /// **ADR-0049 Decision E over the REAL floor execution.** The engine's own selected tokens
    /// clear at every decode position; a producer that re-commits the same logits under one
    /// altered id is convicted at exactly that position — the commitment itself carries the lie,
    /// and no artifact opening is involved.
    #[test]
    fn the_court_refutes_a_committed_decode_token_or_clears_it() {
        use kaspa_consensus_core::palw_step_refute::{
            PalwBase0DecodeTokensV1, PalwStepRefuteError, check_base0_decode_token_refutation_v1,
        };
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");

        let honest =
            PalwBase0DecodeTokensV1 { logits_rows: run.logits_rows.clone(), generated_token_ids: run.generated_token_ids.clone() };
        for p in 0..ctx.exact_decode_tokens {
            assert!(
                matches!(check_base0_decode_token_refutation_v1(&run.binding, &honest, p), Err(PalwStepRefuteError::NoFaultFound)),
                "the engine's own selection clears at decode position {p}"
            );
        }

        // The fraud: same logits, one id altered, re-rooted and re-bound — exactly what a
        // producer that lied about its output would commit.
        let mut lying_ids = run.generated_token_ids.clone();
        lying_ids[1] = (lying_ids[1] + 1) % artifact.shape.vocab as u32;
        let lying_root = base0_logits_trace_root_v1(&ctx, &run.logits_rows, &lying_ids);
        let lying_binding = crate::legs::base0_binding_from_capture_v1(
            &profile,
            &ctx,
            &run.tiles,
            &run.checkpoints,
            lying_root,
            base0_activation_leg_root_v1(&ctx),
        )
        .expect("a lying commitment still binds");
        let lying_pin = PalwBase0DecodeTokensV1 { logits_rows: run.logits_rows.clone(), generated_token_ids: lying_ids };
        let verdict = check_base0_decode_token_refutation_v1(&lying_binding, &lying_pin, 1)
            .expect("a committed token the selection rule refutes convicts");
        assert_eq!(
            verdict.fault,
            kaspa_consensus_core::palw_step_leg::PalwStepFaultV1::DecodeTokenMismatch { position: 1 },
            "the fault names the lying position"
        );
        assert!(
            matches!(check_base0_decode_token_refutation_v1(&lying_binding, &lying_pin, 0), Err(PalwStepRefuteError::NoFaultFound)),
            "position 0's token was honest, and stays cleared"
        );
    }

    /// **ADR-0049 Decision F's obligation fires before the first token is run.**
    ///
    /// "No worker may commit a step leg for a class whose profile does not name every narrowing the
    /// engine performs." The profile is the graph a REGISTERED class carries and it arrives from the
    /// chain; the engine's steps come from `BASE0_LAYER_IR`. A producer that ran anyway would commit
    /// to arithmetic the court recomputes differently — and be convicted for arithmetic it performed
    /// correctly, which is the one verdict this court may never return.
    ///
    /// Both mutations below are invisible to a width comparison: the same op, the same row length,
    /// different arithmetic. That is why the guard is a correspondence check rather than a shape one.
    #[test]
    fn a_class_whose_graph_this_engine_does_not_perform_produces_nothing() {
        let (artifact, profile, ctx, prompt) = small_job();
        base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the class this engine performs runs");

        // A narrowing pointed at another tensor's parameters.
        let mut wrong_operand = profile.clone();
        wrong_operand.attn_nodes[17].weight_name = "blk.{layer}.attn_output.requant".to_string();
        assert!(
            matches!(
                base0_execute_for_attempt_v1(&artifact, &wrong_operand, &ctx, &prompt),
                Err(ProduceError::GraphMismatch(crate::plan::ProjectionMismatch::Node { slot: 17, field: "operand" }))
            ),
            "a producer must not commit a leg the court will recompute from other parameters"
        );

        // The per-head attention widths declared once per layer — the divergence that made 842 of
        // 1068 captured rows disagree, and the reason a single position cannot be trusted to find
        // it: at kv_len 1 this profile and the engine agree exactly.
        let mut per_layer_widths = profile.clone();
        for slot in 12..=15 {
            per_layer_widths.attn_nodes[slot].out_len = kaspa_consensus_core::palw_step::PalwStepOutLenV1::KvScaled { multiplier: 1 };
        }
        assert!(
            matches!(
                base0_execute_for_attempt_v1(&artifact, &per_layer_widths, &ctx, &prompt),
                Err(ProduceError::GraphMismatch(crate::plan::ProjectionMismatch::Node { field: "output width", .. }))
            ),
            "a width that only differs above one position is still a different graph"
        );
    }

    /// The roots follow the execution: a different prompt is a different commitment, in every slot
    /// that is supposed to move. A root that did not move would be one an executor could reuse.
    #[test]
    fn the_roots_follow_the_execution() {
        let (artifact, profile, ctx, prompt) = small_job();
        let a = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("runs");
        let mut other = prompt.clone();
        other[0] = (other[0] + 1) % artifact.shape.vocab;
        let b = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &other).expect("runs");
        assert_ne!(a.trace_root, b.trace_root);
        assert_ne!(a.execution_root, b.execution_root);
        assert_ne!(a.trace_manifest_root, b.trace_manifest_root);
        // The activation leg is the class's "no taps" statement, which is a fact about the JOB
        // shape and not about the run — equal here on purpose, and never the zero hash.
        assert_eq!(base0_activation_leg_root_v1(&ctx), base0_activation_leg_root_v1(&ctx));
        assert_ne!(base0_activation_leg_root_v1(&ctx), Hash64::default(), "a declaration, not an omission");
        // Determinism: the same prompt is the same commitment, or nothing above is checkable.
        let again = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("runs");
        assert_eq!(a.execution_root, again.execution_root);
    }

    /// **A seat's check catches a producer that kept something other than what it committed**
    /// (launch blockers §2).
    ///
    /// Nothing in the tree ever filed a `ReceiptLicensed`, so no claim could reach `Final` and every
    /// panel seat was slashed at `ReceiptTimeout`. A seat has to decide something before it signs,
    /// and this is that decision: rebuild the leg from the retained tiles and ask whether it
    /// reproduces the roots the CLAIM carries. A rubber stamp would license a producer that
    /// committed one root and kept another.
    /// **A tiled class's seat check panicked on material that carries no rows.**
    ///
    /// `tiled_logits_trace_root_v1` hashes a Merkle tree over the retained rows and `expect`s the
    /// tree to exist — but the rows arrive inside a gossiped material blob, and
    /// `PalwGossipFlow` relays that blob to every peer before anything decodes it. A blob with
    /// zero rows (or one empty row) makes `step_merkle_root_v1` return the leaf-count error under
    /// an `expect`, in `verify_material`, on a class whose registered `logits_scheme_id` is the
    /// tiled one — which is both model tiers. Every seat on the claim's panel dies, holding no
    /// bond and having landed no block, so a bondless stranger disarms the court.
    ///
    /// The binding is the attacker's too, so the earlier gates do not stop it: a leaf count of one
    /// with no tiles gives a root the attacker computes and writes into the binding it sends.
    #[test]
    fn a_seat_refuses_tiled_material_that_carries_no_rows() {
        use kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1;

        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");

        // The attacker's binding: the tiled scheme (what QWEN36 and the A16 row register), one
        // leaf and no tiles, and the step root that pair actually produces.
        let mut binding = run.binding.clone();
        binding.shape_profile.logits_scheme_id = tiled_logits_scheme_id_v1();
        binding.step_leaf_count = 1;
        binding.step_merkle_root =
            kaspa_consensus_core::palw_step_leg::step_merkle_root_v1(&[Hash64::default()]).expect("one leaf is a tree");

        for (what, rows, generated) in [
            ("no rows at all", Vec::<Vec<i32>>::new(), Vec::<u32>::new()),
            ("one row with no lanes", vec![Vec::<i32>::new()], vec![0u32]),
        ] {
            let hostile: Base0RetainedMaterialV1 = (binding.clone(), Vec::new(), rows, generated, Vec::new());
            assert!(
                !base0_material_matches_claim_v1(&hostile, run.execution_root, run.trace_root).expect("a refusal, not a panic"),
                "material with {what} must be refused, not hashed"
            );
        }
    }

    /// **A gossiped leaf count used to size an allocation before anything bounded it.**
    ///
    /// `step_leaf_count` is a plain `u64` inside a borsh blob that `PalwGossipFlow` relays before
    /// anything decodes it. The only rule that ever compares it to `PALW_STEP_LEG_MAX_LEAVES` lives
    /// in `step_merkle_root_v1`, five lines BELOW the `vec![Hash64::default(); count]` that used to
    /// come first. A `Hash64` is 64 bytes, so a few hundred bytes of material asking for 2^40 leaves
    /// is a 2^46-byte allocation: under `isize::MAX`, therefore `handle_alloc_error` and a process
    /// ABORT — not a panic anything can catch. Every seat that touched the blob would die.
    ///
    /// Note what this test does against the UNFIXED code: it does not fail, it takes the harness
    /// down with it. That is the defect stated exactly.
    #[test]
    fn a_seat_refuses_an_impossible_leaf_count_before_it_allocates_from_it() {
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        let mut binding = run.binding.clone();
        binding.step_leaf_count = 1 << 40;
        let hostile: Base0RetainedMaterialV1 = (
            binding,
            run.tiles.tiles.clone(),
            run.logits_rows.clone(),
            run.generated_token_ids.clone(),
            run.checkpoints.chunks.clone(),
        );
        assert!(
            !base0_material_matches_claim_v1(&hostile, run.execution_root, run.trace_root).expect("a refusal, not an abort"),
            "an out-of-range leaf count is material that does not match — and it must be refused before it is allocated from"
        );

        // Zero is the other end of the same rule, and `step_merkle_root_v1` refuses it too.
        let mut empty_binding = run.binding.clone();
        empty_binding.step_leaf_count = 0;
        let empty: Base0RetainedMaterialV1 = (
            empty_binding,
            run.tiles.tiles.clone(),
            run.logits_rows.clone(),
            run.generated_token_ids.clone(),
            run.checkpoints.chunks.clone(),
        );
        assert!(!base0_material_matches_claim_v1(&empty, run.execution_root, run.trace_root).expect("checkable"));
    }

    #[test]
    fn a_seat_licenses_only_material_that_matches_the_claim() {
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        let material: Base0RetainedMaterialV1 = (
            run.binding.clone(),
            run.tiles.tiles.clone(),
            run.logits_rows.clone(),
            run.generated_token_ids.clone(),
            run.checkpoints.chunks.clone(),
        );

        assert!(
            base0_material_matches_claim_v1(&material, run.execution_root, run.trace_root).expect("checkable"),
            "a producer that kept what it committed is licensed"
        );

        // A claim committing a DIFFERENT execution root — one execution published, another kept.
        // This is the case a rubber stamp would sign.
        assert!(
            !base0_material_matches_claim_v1(&material, Hash64::from_u64_word(0xBAD), run.trace_root).expect("checkable"),
            "material that does not match the committed execution root must not be licensed"
        );
        assert!(
            !base0_material_matches_claim_v1(&material, run.execution_root, Hash64::from_u64_word(0xBAD)).expect("checkable"),
            "nor material whose trace root is not the committed one"
        );

        // And material whose own tiles do not reproduce its own binding: a commitment kept without
        // the execution behind it.
        let mut tampered = material.clone();
        tampered.1[0].1.values_le[0] = tampered.1[0].1.values_le[0].wrapping_add(1);
        assert!(
            !base0_material_matches_claim_v1(&tampered, run.execution_root, run.trace_root).expect("checkable"),
            "a binding its own tiles do not reproduce is a commitment, not an execution"
        );
    }

    /// A producer that ran a shorter prompt than its context declares ran a DIFFERENT job from the
    /// one it committed to — refused at the source rather than committed and argued about later.
    #[test]
    fn a_prompt_that_does_not_match_the_committed_context_is_refused() {
        let (artifact, profile, ctx, prompt) = small_job();
        assert_eq!(
            base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt[..2]).err(),
            Some(ProduceError::PromptShorterThanPrefill { prompt: 2, declared: 3 })
        );
        assert_eq!(
            base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &[prompt[0], prompt[1], 9_999]).err(),
            Some(ProduceError::TokenOutOfVocab { token: 9_999, vocab: artifact.shape.vocab })
        );
    }

    /// **The ladder above the default one.** A class whose step space is larger than
    /// `PALW_STEP_LEG_MAX_LEAVES` — the graph-v5 attempt lane is 6.6 M leaves — roots under its
    /// RULESET's cap, and not under the default: the cap is the bound. The verifier reached for
    /// the uncapped root and reported a real graph-v5 capture as a mismatch (ADR-0084 §7 record).
    #[test]
    fn a_dense_material_above_the_default_ladder_roots_under_its_rulesets_cap() {
        use kaspa_consensus_core::palw_step::PalwStepCoordinateV1;
        use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_MAX_LEAVES, PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepTileLeafV1};
        let (artifact, profile, ctx, prompt) = small_job();
        let run = base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the job runs");
        let mut binding = run.binding.clone();
        let leaf_count = PALW_STEP_LEG_MAX_LEAVES + 1;
        let cap = PALW_STEP_LEG_MAX_LEAVES * 2;
        let tiles: Vec<(u64, PalwStepTileLeafV1)> = (0..leaf_count)
            .map(|i| {
                let leaf = PalwStepTileLeafV1 {
                    version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                    coord: PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: i as u32 },
                    value_count: 1,
                    values_le: (i as i32).to_le_bytes().to_vec(),
                };
                (i, leaf)
            })
            .collect();
        binding.step_leaf_count = leaf_count;
        let root = base0_dense_step_root_capped_v1(&binding, &tiles, cap).expect("the space roots under its ruleset's cap");
        assert_eq!(
            base0_dense_step_root_capped_v1(&binding, &tiles, PALW_STEP_LEG_MAX_LEAVES),
            None,
            "and not under the default ladder — the cap is the bound, not the constant"
        );
        let leaves = base0_dense_step_leaves_capped_v1(&binding, &tiles, cap).expect("the leaves");
        assert_eq!(leaves.len() as u64, leaf_count);
        assert_eq!(kaspa_consensus_core::palw_step_leg::step_merkle_root_capped_v1(&leaves, cap).ok(), Some(root));
        assert!(kaspa_consensus_core::palw_step_leg::step_merkle_root_v1(&leaves).is_err(), "the default ladder does not reach this space");
    }
}
