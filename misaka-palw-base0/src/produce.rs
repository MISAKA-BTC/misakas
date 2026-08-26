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
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, PalwStepTableV1, step_leaf_count};
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
}

impl std::fmt::Display for ProduceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Engine(e) => write!(f, "the engine refused the pass: {e:?}"),
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

pub const PALW_BASE0_DOMAIN_JOB_ANCHOR: &[u8] = b"misaka-palw/base0/rc-job-anchor/v1";
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
pub fn base0_rc_job_anchor_v1(
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    class_id: Hash64,
    bond: &kaspa_consensus_core::tx::TransactionOutpoint,
) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_BASE0_DOMAIN_JOB_ANCHOR).to_state();
    h.update(network_domain.as_byte_slice());
    h.update(pre_pow_hash.as_byte_slice());
    h.update(class_id.as_byte_slice());
    h.update(bond.transaction_id.as_bytes().as_slice());
    h.update(&bond.index.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
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
    pub tiles: Base0StepTilesV1,
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

    let leaf_count = step_leaf_count(profile, ctx).map_err(ProduceError::StepSpace)?;
    let mut capture = Base0StepCaptureV1::new(leaf_count).map_err(ProduceError::Leg)?;
    let engine = Base0Engine::new(artifact);
    let mut cache = KvCache::new(artifact);
    // The class's own checkpoint profile, at the producer's interval — the same object the binding
    // files, so the capture and the commitment cannot disagree about the layout or the cadence.
    let checkpoint_profile = kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(1);
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
        last_logits = logits;
    }
    let mut next = argmax_lowest(&last_logits);
    generated.push(next as u32);
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
        logits_rows.push(logits);
        // A checkpoint after this call if the cadence says so. `call` IS the covered decode call —
        // the cache now holds `prefill + call` positions, which is what the map derives for it.
        if call as u32 == checkpoints.next_covered_decode_call() {
            checkpoints.push(&cache).map_err(ProduceError::Leg)?;
        }
    }

    let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    let checkpoints = checkpoints.finish(decode_calls / checkpoint_profile.checkpoint_interval).map_err(ProduceError::Leg)?;
    let tiles = capture.finish().map_err(ProduceError::Leg)?;
    let trace_root = base0_logits_trace_root_v1(ctx, &logits_rows, &generated);
    let activation_leg_root = base0_activation_leg_root_v1(ctx);
    let binding = crate::legs::base0_binding_from_capture_v1(profile, ctx, &tiles, &checkpoints, trace_root, activation_leg_root)
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
    let trace_manifest_root = {
        let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_BASE0_DOMAIN_TRACE_MANIFEST).to_state();
        h.update(ctx_hash.as_byte_slice());
        h.update(trace_root.as_byte_slice());
        h.update(binding.step_merkle_root.as_byte_slice());
        h.update(&1u32.to_le_bytes());
        let mut out = [0u8; 64];
        out.copy_from_slice(h.finalize().as_bytes());
        Hash64::from_bytes(out)
    };

    Ok(Base0ExecutionV1 {
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
    use kaspa_consensus_core::palw_state_chunk_map as map;
    let prefill = ctx.declared_prefill_tokens;
    let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    let covered = checkpoint.covered_decode_call;
    if covered > decode_calls || calls == 0 || covered.saturating_add(calls) > decode_calls {
        return Err(ProduceError::Internal("the replay window is not inside this job's decode calls"));
    }
    let positions = map::integer_kv_positions_at_v1(ctx, covered);
    let geometry = map::integer_kv_state_geometry_v1(profile, positions).map_err(|_| ProduceError::Internal("no state map"))?;
    let mut cache = KvCache::from_state_chunks(artifact, &geometry, chunks).map_err(ProduceError::Engine)?;

    let leaf_count = step_leaf_count(profile, ctx).map_err(ProduceError::StepSpace)?;
    let mut capture = Base0StepCaptureV1::new(leaf_count).map_err(ProduceError::Leg)?;
    let engine = Base0Engine::new(artifact);
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
    let (binding, tiles, logits_rows, generated, checkpoint_chunks) = material;
    // The leg root over the retained tiles, recomputed rather than trusted: a producer that kept a
    // binding whose root does not match its own tiles kept a commitment, not an execution.
    let mut leaves = vec![Hash64::default(); binding.step_leaf_count as usize];
    let ctx_hash = binding.job_context.context_hash();
    let profile_hash = binding.shape_profile.shape_profile_id();
    for (index, leaf) in tiles {
        let Some(slot) = leaves.get_mut(*index as usize) else {
            return Ok(false); // a tile outside the space it claims to fill
        };
        *slot = kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, leaf);
    }
    let Ok(root) = kaspa_consensus_core::palw_step_leg::step_merkle_root_v1(&leaves) else {
        return Ok(false);
    };
    if root != binding.step_merkle_root {
        return Ok(false);
    }
    // The logits rows and generated ids must REPRODUCE the integer trace root the binding
    // carries — equality of the binding's field against the claim says the producer kept the
    // right commitment; this says it kept the execution behind it, which is what a decode-side
    // dispute (ADR-0049 Decision E) is adjudicated against.
    if kaspa_consensus_core::palw_step_refute::base0_logits_trace_root_v1(&binding.job_context, logits_rows, generated)
        != binding.full_logits_trace_root
    {
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
    let Ok(rebuilt) = crate::legs::Base0CheckpointCaptureV1::from_chunks_v1(
        &binding.job_context,
        &binding.shape_profile,
        &binding.checkpoint_profile,
        checkpoint_chunks,
    ) else {
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
        let expected = step_leaf_count(&profile, &ctx).expect("the job has a step space");
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
        let leaves = step_leaf_count(&profile, &ctx).expect("the job has a step space");
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
            let refutation =
                crate::legs::base0_refutation_from_capture_v1(&profile, &ctx, &lying, binding, coord, ids.clone(), Some(pin.clone()))
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
}
