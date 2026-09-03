//! **ADR-0082 Decision 9: a seat recomputes the cache from the prompt it holds; it never fetches
//! the history.**
//!
//! # What this replaces, and the number that forced it
//!
//! ADR-0077 Decision 8 has the seat ask the executor for "the checkpoint chunk at the interval's
//! start" ([`crate::fp_interval`]). For an attention family that chunk IS the history:
//! `positions × 2 caches × kv_dim × 4 bytes × layers` — 7.5 GB on the dense tier at 131,072
//! positions, 5.4 GB on the hybrid's ten attention layers. A seat whose bytes are the history is
//! the seat ADR-0077 R1 and W10 forbid, and no chunking of the map repairs it: chunking changes
//! how finely the history is named, never how much of it travels.
//!
//! The seat already holds everything the state is a function of. The prompt ids ride on the
//! accepted 0x4a payload (under `PublicDa`) or in the served material (under `PanelDa`); the
//! output ids are committed under the decode pin. So the seat RUNS the job — the prefill and the
//! decode calls up to the interval's start, teacher-forced with the committed ids — and compares
//! the 64-byte state root it computes against the checkpoint root the executor committed. Bytes
//! for the state: none. Compute: one forward pass of the job, which is Ambient's validator cost
//! (ADR-0026 §1.7) with Ambient's tolerance replaced by equality.
//!
//! # Teacher-forced, and why that is not trusting the executor
//!
//! The ids fed here are the executor's committed output ids, and the seat does not select tokens
//! of its own. That is not a concession: the ids are bound to the claim (the decode pin over
//! `tiled_logits_outer_root_v1`, and the prompt hash inside the job context), and a lie in any of
//! them lands the recompute in a different state whose root does not match the committed
//! checkpoint — which is exactly the comparison this module exists to make. What the seat must
//! never do is take the STATE from the executor, and it never does: every byte of the state
//! returned here was computed by the seat's own kernels.
//!
//! # One spelling of the commitment
//!
//! The root is not hashed here. The chunks go through the same two consensus functions the
//! executor's checkpoint leg uses — `state_chunk_leaf_hash_v1` under the class's declared map id,
//! then `state_chunks_root_v1` — spelled once in
//! [`crate::fp_interval::base0_state_chunks_root_v1`] and shared with the check that authenticates
//! an executor's anchor. `the_seats_root_is_the_executors_leaf_root` pins the two against
//! `Base0CheckpointCaptureV1`, the producer's own path, on a real capture.
//!
//! # What is NOT decided here
//!
//! The v1 and v2 HYBRID compositions. Their names pair an attention half with a recurrence half
//! and nothing in the tree enumerates them, so this module refuses those two BY NAME rather than
//! inventing an order: an enumeration invented on the seat side would be a second opinion about a
//! consensus object.
//!
//! The **v3** composition is a different matter and this module no longer refuses it: it is
//! enumerated by `palw_state_chunk_map::hybrid_state_chunk_entry_v3` — the consensus side, which
//! is the side that gets to spell it — and a graph-v5 hybrid registers it (ADR-0082 Decision 4).
//! The walk here is not a second derivation either: both this seat and the PRODUCER's capture go
//! through `crate::legs::base0_composed_state_chunks_v1`, so the chunk at index `i` is the same
//! bytes on both sides by construction. (The header used to say this module refuses "such a class"
//! outright while implementing the v3 composition four hundred lines below — audit B, L-1.)

use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// Why a seat cannot recompute a class's state. Every arm is a REFUSAL a seat files as
/// `Incapable`, never an accusation: a seat that cannot run the class has learned nothing about
/// the producer (ADR-0082 Decision 9's third consequence, ADR-0075).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base0FpRecomputeError {
    /// The class takes no checkpoints, so it has no committed state root to compare against.
    NoStateChunkMapRegistered,
    /// The ids served are not the ones the job's context binds.
    PromptIdsAreNotTheJobs,
    /// The job declares one prefill length and another was supplied.
    PromptIsNotTheJobsLength { declared: u32, got: usize },
    /// The recompute was asked for more decode calls than the claim executed.
    DecodeCallsBeyondTheJob { asked: u32, job: u32 },
    /// Fewer committed output ids than the calls being teacher-forced need.
    OutputIdsTooShort { need: usize, got: usize },
    /// The state does not fit the map the class declares — the same refusal
    /// `A16Cache::state_chunk_bytes_v1` makes, surfaced with the chunk named.
    StateIsNotTheMaps { chunk_index: u64 },
    /// The class's map has no geometry this family can read.
    Map(String),
    /// The class's kernels refused the run.
    Engine(String),
    /// This family has no state serializer for the map the class registered — the hybrid
    /// composition, which no side of the tree spells yet.
    NoStateSerializer { why: &'static str },
}

impl std::fmt::Display for Base0FpRecomputeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoStateChunkMapRegistered => {
                write!(f, "this class registers no state chunk map, so it commits no checkpoint state to recompute")
            }
            Self::PromptIdsAreNotTheJobs => write!(f, "the served ids do not hash to the job's prompt_token_ids_hash"),
            Self::PromptIsNotTheJobsLength { declared, got } => {
                write!(f, "the job declares {declared} prefill tokens and {got} were supplied")
            }
            Self::DecodeCallsBeyondTheJob { asked, job } => {
                write!(f, "the recompute was asked for {asked} decode calls and the claim executed {job}")
            }
            Self::OutputIdsTooShort { need, got } => {
                write!(f, "teacher-forcing {need} calls needs {need} committed output ids and {got} were carried")
            }
            Self::StateIsNotTheMaps { chunk_index } => {
                write!(f, "this cache does not fit the state map the class declares (chunk {chunk_index})")
            }
            Self::Map(why) => write!(f, "the class's state chunk map has no geometry here: {why}"),
            Self::Engine(why) => write!(f, "the class's kernels could not run the job: {why}"),
            Self::NoStateSerializer { why } => write!(f, "this family cannot serialize the state this class commits: {why}"),
        }
    }
}

impl std::error::Error for Base0FpRecomputeError {}

/// **The state a seat computed for itself**, at one checkpoint's covered call.
///
/// `chunks` are the seat's OWN bytes in the class's map order, so an interval replay that resumes
/// from them resumes from arithmetic this node performed — which is the whole of Decision 9. They
/// are never compared against an executor's chunks, because no executor chunk is ever fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base0FpSeatStateV1 {
    /// **The checkpoint leaf's own counter for this state**, in the unit the class's cadence
    /// counts in: `index × checkpoint_interval` decode calls on a per-call class,
    /// `prefill + index × checkpoint_interval` POSITIONS on a per-position one. The name is the
    /// leaf field's and is kept so the comparison against an opening is field against field.
    pub covered_decode_call: u32,
    /// Positions folded into it —
    /// `palw_checkpoint_positions_at_v1(profile, ctx, covered_decode_call)`, which is
    /// `declared_prefill_tokens + covered_decode_call` on a per-call class and
    /// `covered_decode_call` itself on a per-position one.
    pub positions: u32,
    /// The committed root of those bytes under the class's map — the 64 bytes that replace the
    /// history on the wire.
    pub state_chunks_root: Hash64,
    pub chunks: Vec<Vec<u8>>,
}

/// **The class's own kernels, as a RECOMPUTE needs them** — a forward with no capture, and the
/// state serializer the class's map names.
///
/// Deliberately not [`crate::fp_interval::Base0FpIntervalKernelsV1`]: that one replays a WINDOW
/// and returns committed leaf hashes, which is `interval` calls of hashing a seat does not want
/// over the whole prefix of the job. This one runs the prefix for its state alone. A family
/// implements both over one engine.
pub trait Base0FpRecomputeKernelsV1 {
    /// One teacher-forced forward at an absolute cache position. The logits are dropped: the seat
    /// does not select tokens, and hashing rows over the prefix is work Decision 9 does not spend.
    fn forward_no_capture(&mut self, token: usize, position: usize) -> Result<(), Base0FpRecomputeError>;

    /// The class's committed state after the run so far, chunked in the CLASS's own map order.
    ///
    /// `positions` is derived from the job and the covered call, never from the cache — a cache a
    /// row short would otherwise serialize as a shorter state and the shortfall would look like a
    /// job that ran fewer calls (`Base0CheckpointCaptureV1::push`'s rule).
    fn state_chunks(&self, profile: &PalwShapeProfileV3, positions: u32) -> Result<Vec<Vec<u8>>, Base0FpRecomputeError>;
}

/// **Run the job's prefix and return the state a seat holds at `decode_calls`** (Decision 9).
///
/// The walk is the executor's own (`a16_execute_for_attempt_capped_v1`, `qwen36_…`): the prefill
/// over the declared prompt, then decode call `c` consuming the id call `c − 1` produced. Nothing
/// is captured and no token is selected; the ids are the committed ones.
pub fn base0_fp_recompute_state_v1<K: Base0FpRecomputeKernelsV1 + ?Sized>(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt_token_ids: &[u32],
    output_token_ids: &[u32],
    decode_calls: u32,
    kernels: &mut K,
) -> Result<Base0FpSeatStateV1, Base0FpRecomputeError> {
    // **The ids are an INPUT on this lane and are refused unless they are the job's** — the rule
    // `refutation_for_free_prompt_index` and `base0_open_fp_interval_v1` both state. A seat that
    // recomputed from another list would compare its own honest arithmetic on a different job
    // against this claim's checkpoint and call the producer a liar.
    if kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(prompt_token_ids) != ctx.prompt_token_ids_hash {
        return Err(Base0FpRecomputeError::PromptIdsAreNotTheJobs);
    }
    let prefill = ctx.declared_prefill_tokens as usize;
    if prompt_token_ids.len() != prefill {
        return Err(Base0FpRecomputeError::PromptIsNotTheJobsLength {
            declared: ctx.declared_prefill_tokens,
            got: prompt_token_ids.len(),
        });
    }
    let job_decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    if decode_calls > job_decode_calls {
        return Err(Base0FpRecomputeError::DecodeCallsBeyondTheJob { asked: decode_calls, job: job_decode_calls });
    }
    // Call `c` consumes the id call `c − 1` produced, so `decode_calls` calls need that many ids.
    if (output_token_ids.len() as u64) < decode_calls as u64 {
        return Err(Base0FpRecomputeError::OutputIdsTooShort { need: decode_calls as usize, got: output_token_ids.len() });
    }
    // A class with the checkpoint sentinel commits no state; saying so is the honest `Incapable`
    // rather than serving a root of a leg that does not exist.
    if profile.state_chunk_map_id == Hash64::default() {
        return Err(Base0FpRecomputeError::NoStateChunkMapRegistered);
    }

    for (position, token) in prompt_token_ids.iter().enumerate() {
        kernels.forward_no_capture(*token as usize, position)?;
    }
    for call in 1..=decode_calls {
        let token = output_token_ids[call as usize - 1];
        let position = prefill + call as usize - 1;
        kernels.forward_no_capture(token as usize, position)?;
    }

    let positions = kaspa_consensus_core::palw_state_chunk_map::integer_kv_positions_at_v1(ctx, decode_calls);
    seat_state_here_v1(profile, decode_calls, positions, kernels)
}

/// **Run the job's prefix and return the state a seat holds at absolute POSITION `positions`**
/// (ADR-0082 Decision 4, amended, on the seat's side).
///
/// [`base0_fp_recompute_state_v1`] counts in decode CALLS, which is the unit the per-call cadence
/// commits in and the only unit that had a checkpoint to compare against. A class whose map
/// addresses history tiles commits one after every position, prefill included, so its seat has to
/// be able to stop at a prefill position too — and its `covered_decode_call` field IS a position
/// count, which is what makes the returned state comparable to the committed leaf.
///
/// `positions` is the number of cache rows to stop after (`covered`), so `positions = 0` is a
/// state with nothing in it and is refused rather than served: a root over an empty map is not the
/// root of a state, it is the root of nothing.
pub fn base0_fp_recompute_state_at_position_v1<K: Base0FpRecomputeKernelsV1 + ?Sized>(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt_token_ids: &[u32],
    output_token_ids: &[u32],
    positions: u32,
    kernels: &mut K,
) -> Result<Base0FpSeatStateV1, Base0FpRecomputeError> {
    if kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(prompt_token_ids) != ctx.prompt_token_ids_hash {
        return Err(Base0FpRecomputeError::PromptIdsAreNotTheJobs);
    }
    let prefill = ctx.declared_prefill_tokens as usize;
    if prompt_token_ids.len() != prefill {
        return Err(Base0FpRecomputeError::PromptIsNotTheJobsLength {
            declared: ctx.declared_prefill_tokens,
            got: prompt_token_ids.len(),
        });
    }
    let job_decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    let job_positions = ctx.declared_prefill_tokens.saturating_add(job_decode_calls);
    if positions > job_positions || positions == 0 {
        return Err(Base0FpRecomputeError::DecodeCallsBeyondTheJob {
            asked: positions,
            job: job_positions,
        });
    }
    if profile.state_chunk_map_id == Hash64::default() {
        return Err(Base0FpRecomputeError::NoStateChunkMapRegistered);
    }
    // The decode calls this position count implies — zero while the walk is still inside the
    // prefill, which is the case the per-call form could not express at all.
    let decode_calls = (positions as usize).saturating_sub(prefill) as u32;
    if (output_token_ids.len() as u64) < decode_calls as u64 {
        return Err(Base0FpRecomputeError::OutputIdsTooShort { need: decode_calls as usize, got: output_token_ids.len() });
    }

    // The executor's own walk, stopped at `positions` rows: the prefill in order, then decode call
    // `c` consuming the id call `c − 1` produced.
    for (position, token) in prompt_token_ids.iter().enumerate().take((positions as usize).min(prefill)) {
        kernels.forward_no_capture(*token as usize, position)?;
    }
    for call in 1..=decode_calls {
        let token = output_token_ids[call as usize - 1];
        let position = prefill + call as usize - 1;
        kernels.forward_no_capture(token as usize, position)?;
    }
    seat_state_here_v1(profile, decode_calls, positions, kernels)
}

/// **The state the checkpoint carrying `covered` is the state of** (ADR-0082 Decision 4, amended;
/// audit B, C-2) — the entry every SEAT takes, because `covered` is the only number an opening
/// carries.
///
/// One conversion, in one place: `palw_checkpoint_positions_at_v1` is what turns the class's own
/// counter into cache rows, and the walk stops after that many. On a per-call class it is
/// `prefill + covered` and this is [`base0_fp_recompute_state_v1`] verbatim; on a per-position
/// class the counter already IS the row count and the walk can stop inside the prefill — the case
/// the per-call entry cannot express at all, and the case audit B measured as a guaranteed
/// state-root mismatch (a 403-position state compared against a 3-position root).
///
/// The returned `covered_decode_call` is the LEAF's counter, not the decode call, so a caller
/// comparing it against the checkpoint an opening names compares like with like.
pub fn base0_fp_recompute_state_at_covered_v1<K: Base0FpRecomputeKernelsV1 + ?Sized>(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt_token_ids: &[u32],
    output_token_ids: &[u32],
    covered: u32,
    kernels: &mut K,
) -> Result<Base0FpSeatStateV1, Base0FpRecomputeError> {
    let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(profile, ctx, covered);
    let state = base0_fp_recompute_state_at_position_v1(profile, ctx, prompt_token_ids, output_token_ids, positions, kernels)?;
    Ok(Base0FpSeatStateV1 { covered_decode_call: covered, ..state })
}

/// The state serialisation both entries share — one spelling of "chunk what the kernels hold and
/// root it", so a position-stopped walk and a call-stopped one cannot root the same state two ways.
fn seat_state_here_v1<K: Base0FpRecomputeKernelsV1 + ?Sized>(
    profile: &PalwShapeProfileV3,
    decode_calls: u32,
    positions: u32,
    kernels: &K,
) -> Result<Base0FpSeatStateV1, Base0FpRecomputeError> {
    let chunks = kernels.state_chunks(profile, positions)?;
    let state_chunks_root = crate::fp_interval::base0_state_chunks_root_v1(&profile.state_chunk_map_id, &chunks)
        .map_err(|e| Base0FpRecomputeError::Map(e.to_string()))?;
    Ok(Base0FpSeatStateV1 { covered_decode_call: decode_calls, positions, state_chunks_root, chunks })
}

// =================================================================================================
// The families' kernels
// =================================================================================================

/// **The dense tier's recompute** — the engine and the cache the class's own capture path uses
/// (`a16_execute_for_attempt_capped_v1`), walked from the plan when the class registered one.
///
/// It lives here rather than beside the backend because it is the seat's machinery and the
/// backend's own file is the executor's: what the backend contributes is the three lines that
/// build one of these out of its private artifact and plan.
pub struct A16RecomputeKernelsV1<'a> {
    engine: crate::engine_a16::A16Engine<'a>,
    plan: Option<&'a crate::engine_a16::A16ProfilePlanV1>,
    cache: crate::engine_a16::A16Cache,
    vocab: usize,
}

impl<'a> A16RecomputeKernelsV1<'a> {
    pub fn new(
        artifact: &'a crate::artifact::Base0ArtifactV1,
        plan: Option<&'a crate::engine_a16::A16ProfilePlanV1>,
    ) -> Result<Self, Base0FpRecomputeError> {
        let engine = crate::engine_a16::A16Engine::new(artifact)
            .map_err(|e| Base0FpRecomputeError::Engine(format!("the artifact is not an A16 class: {e:?}")))?;
        Ok(Self { engine, plan, cache: crate::engine_a16::A16Cache::new(artifact.shape.n_layers), vocab: artifact.shape.vocab })
    }
}

impl Base0FpRecomputeKernelsV1 for A16RecomputeKernelsV1<'_> {
    fn forward_no_capture(&mut self, token: usize, position: usize) -> Result<(), Base0FpRecomputeError> {
        if token >= self.vocab {
            return Err(Base0FpRecomputeError::Engine(format!("token {token} is outside this class's vocabulary of {}", self.vocab)));
        }
        // The PLANNED walk where the class registered a graph (ADR-0067: the court adjudicates
        // what was declared, so a seat's own state must come from the declaration too).
        match self.plan {
            Some(plan) => self
                .engine
                .forward_token_planned(plan, &mut self.cache, token, position)
                .map(|_| ())
                .map_err(|e| Base0FpRecomputeError::Engine(format!("planned forward at {position}: {e:?}"))),
            None => self
                .engine
                .forward_token(&mut self.cache, token, position)
                .map(|_| ())
                .map_err(|e| Base0FpRecomputeError::Engine(format!("forward at {position}: {e:?}"))),
        }
    }

    fn state_chunks(&self, profile: &PalwShapeProfileV3, positions: u32) -> Result<Vec<Vec<u8>>, Base0FpRecomputeError> {
        // **The CLASS's map, through the one dispatch both directions take.** The same geometry
        // `Base0CheckpointCaptureV1::next_geometry` takes, so a seat chunks the state exactly
        // where the executor did — including the v3 tiled map a graph-v5 class registers.
        let geometry = crate::legs::base0_state_chunk_geometry_v1(profile, positions)
            .map_err(|e| Base0FpRecomputeError::Map(format!("{e:?}")))?;
        let mut chunks = Vec::with_capacity(geometry.chunk_count() as usize);
        for index in 0..geometry.chunk_count() {
            let entry = kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_entry_v1(&geometry, index)
                .ok_or(Base0FpRecomputeError::StateIsNotTheMaps { chunk_index: index })?;
            chunks
                .push(self.cache.state_chunk_bytes_v1(&entry).ok_or(Base0FpRecomputeError::StateIsNotTheMaps { chunk_index: index })?);
        }
        Ok(chunks)
    }
}

/// **The hybrid tier's recompute.**
///
/// The forward is live; the state serializer answers for the RECURRENCE's own map and refuses the
/// hybrid composition by name. That is not a shortcut: the shipped hybrid class registers the
/// checkpoint sentinel and commits no checkpoint at all
/// ([`crate::qwen36_backend::qwen36_checkpoint_profile_v1`]), and no side of the tree spells the
/// order in which a hybrid's attention chunks and its recurrence chunks compose into one map. The
/// side that registers the composed class is the side that spells it; a seat that guessed would be
/// a second opinion about a consensus object.
pub struct Qwen36RecomputeKernelsV1<'a> {
    engine: crate::qwen36::Qwen36Engine<'a>,
    plan: &'a crate::qwen36_plan::Qwen36ProfilePlanV1,
    cache: crate::qwen36::Qwen36Cache,
    shape: &'a crate::qwen36::Qwen36ShapeV1,
}

impl<'a> Qwen36RecomputeKernelsV1<'a> {
    pub fn new(artifact: &'a crate::qwen36::Qwen36ArtifactV1, plan: &'a crate::qwen36_plan::Qwen36ProfilePlanV1) -> Self {
        Self {
            engine: crate::qwen36::Qwen36Engine::new(artifact),
            plan,
            cache: crate::qwen36::Qwen36Cache::new(&artifact.shape),
            shape: &artifact.shape,
        }
    }

    /// The recurrence layers' live state, in the shape [`crate::fp_capture`] chunks.
    fn recurrence_state(&self) -> (Vec<u16>, Vec<crate::fp_capture::Base0GdnLayerStateV1>) {
        qwen36_recurrence_state_v1(self.shape, &self.cache)
    }

    fn attn_chunk_bytes_v1(
        &self,
        entry: &kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkEntryV1,
    ) -> Option<Vec<u8>> {
        qwen36_attn_chunk_bytes_v1(&self.cache, entry)
    }
}

/// **The hybrid cache's recurrence layers, in the order the gdn map enumerates them** — one
/// spelling for the seat (above) and the PRODUCER (`qwen36_execute_for_attempt_capped_v1`), so a
/// checkpoint the executor commits and the one a seat recomputes are the same object.
pub fn qwen36_recurrence_state_v1(
    shape: &crate::qwen36::Qwen36ShapeV1,
    cache: &crate::qwen36::Qwen36Cache,
) -> (Vec<u16>, Vec<crate::fp_capture::Base0GdnLayerStateV1>) {
    let mut layers = Vec::new();
    let mut states = Vec::new();
    for (index, kind) in shape.layer_types.iter().enumerate() {
        if *kind != crate::qwen36::Qwen36LayerKind::LinearAttention {
            continue;
        }
        layers.push(index as u16);
        states.push(crate::fp_capture::Base0GdnLayerStateV1 {
            heads: cache.gdn.get(index).cloned().unwrap_or_default(),
            conv: cache.conv.get(index).cloned().unwrap_or_default(),
        });
    }
    (layers, states)
}

/// **This cache's bytes for one ATTENTION chunk of the v3 composition** — the hybrid's analogue of
/// `A16Cache::state_chunk_bytes_v1`, and the same discipline: the width is read off the entry
/// rather than assumed, and a row that does not fit the declared width is refused instead of
/// narrowed.
///
/// The composition's attention half is `tiled_kv_state_geometry_v3` verbatim, whose rows are
/// little-endian `i32` at `attn_kv_heads × attn_head_dim × 4` bytes — the width the `i32` cache
/// this engine holds actually is. A checkpoint that opened to a state the producer never held is
/// worse than a missing one, and the producer has signed for it.
///
/// A free function because the producer serializes the same cache with the same rule; two copies
/// of it is how a producer and a seat come to commit different bytes for one state.
pub fn qwen36_attn_chunk_bytes_v1(
    cache: &crate::qwen36::Qwen36Cache,
    entry: &kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkEntryV1,
) -> Option<Vec<u8>> {
    use kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1;
    let side = match entry.kind {
        PalwStateChunkKindV1::Key => &cache.keys,
        PalwStateChunkKindV1::Value => &cache.values,
    };
    let layer = side.get(entry.attn_layer as usize)?;
    let mut out = Vec::with_capacity((entry.position_count as usize) * (entry.row_bytes as usize));
    for p in entry.position_start..entry.position_start + entry.position_count {
        let row = layer.get(p as usize)?;
        if entry.row_bytes as usize != row.len().checked_mul(4)? {
            return None;
        }
        for value in row {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    Some(out)
}

impl Base0FpRecomputeKernelsV1 for Qwen36RecomputeKernelsV1<'_> {
    fn forward_no_capture(&mut self, token: usize, position: usize) -> Result<(), Base0FpRecomputeError> {
        if token >= self.shape.vocab {
            return Err(Base0FpRecomputeError::Engine(format!(
                "token {token} is outside this class's vocabulary of {}",
                self.shape.vocab
            )));
        }
        if position >= self.shape.max_position {
            return Err(Base0FpRecomputeError::Engine(format!("the job runs past the rotary table at position {position}")));
        }
        self.engine
            .forward_token_planned_logits(self.plan, &mut self.cache, token, position)
            .map(|_| ())
            .map_err(|e| Base0FpRecomputeError::Engine(format!("forward at {position}: {e}")))
    }

    fn state_chunks(&self, profile: &PalwShapeProfileV3, positions: u32) -> Result<Vec<Vec<u8>>, Base0FpRecomputeError> {
        use kaspa_consensus_core::palw_state_chunk_map as map;
        let declared = profile.state_chunk_map_id;
        let (layers, states) = self.recurrence_state();
        let heads = self.shape.linear_v_heads as u32;
        let dim = self.shape.linear_head_dim as u32;
        let kernel = self.shape.conv_kernel as u32;
        if declared == map::gdn_state_chunk_map_id_v2() {
            let geometry = crate::fp_capture::base0_gdn_state_geometry_v2(&layers, heads, dim, dim, kernel)
                .map_err(|e| Base0FpRecomputeError::Map(e.to_string()))?;
            return crate::fp_capture::base0_gdn_state_chunks_v2(&geometry, &states)
                .map_err(|e| Base0FpRecomputeError::Map(e.to_string()));
        }
        if declared == map::gdn_state_chunk_map_id_v1() {
            let geometry = crate::fp_capture::base0_gdn_state_geometry_v1(&layers, heads, dim, dim, kernel)
                .map_err(|e| Base0FpRecomputeError::Map(e.to_string()))?;
            return crate::fp_capture::base0_gdn_state_chunks_v1(&geometry, &states)
                .map_err(|e| Base0FpRecomputeError::Map(e.to_string()));
        }
        // **The v3 composition, in the order its own NAME spells** (ADR-0082 Decision 4; stream
        // G's patch note 5). The order is not this function's opinion:
        // `palw_hybrid_state_chunk_map_name_v3` is `palw-hybrid-state/attn=…/gdn=…/v3` — `attn=`
        // before `gdn=` — and `hybrid_state_chunk_entry_v3` is the enumeration that reads it. This
        // walks THAT enumeration rather than restating it, so a seat's chunks are the executor's
        // chunks by construction; a second derivation that merely agreed is how a producer and a
        // court come to open different bytes.
        //
        // The two halves run at two cadences and this function does not choose between them:
        // `hybrid_state_geometry_for_covered_v1` asks
        // `palw_checkpoint_leaf_carries_recurrence_v1` whether THIS checkpoint carries the
        // recurrence at all — every position for the attention tiles, the derived spacing for the
        // recurrence state, because a `heads × k_dim × v_dim × 4` state is not prefix-stable and a
        // per-position commitment of it would hash 2 MiB a token.
        if declared == map::hybrid_state_chunk_map_id_v3() {
            let geometry = map::hybrid_state_geometry_for_covered_v1(profile, positions)
                .map_err(|e| Base0FpRecomputeError::Map(format!("{e:?}")))?;
            let gdn_geometry = crate::fp_capture::base0_gdn_state_geometry_v2(&layers, heads, dim, dim, kernel)
                .map_err(|e| Base0FpRecomputeError::Map(e.to_string()))?;
            // The composition promises ONE chunk per `(kind, layer, head)`; the executor's own v2
            // geometry splits a head only when its slice does not fit a chunk, which
            // `hybrid_state_geometry_v3` refuses outright. Checked rather than assumed: two
            // enumerations of one state that disagree about their chunk count is a leg nobody can
            // open at the index the other named.
            let gdn_chunks = if geometry.gdn_chunk_count() == 0 {
                // This leaf does not carry the recurrence at all: under the per-position cadence
                // the attention half rides every position and the recurrence rides its own
                // spacing, so the executor's recurrence geometry is not the thing to compare
                // against here — there is nothing to compare it to.
                Vec::new()
            } else {
                if gdn_geometry.chunk_count() != geometry.gdn_chunk_count() {
                    return Err(Base0FpRecomputeError::Map(format!(
                        "the recurrence enumerates {} chunks and the composition names {}",
                        gdn_geometry.chunk_count(),
                        geometry.gdn_chunk_count()
                    )));
                }
                crate::fp_capture::base0_gdn_state_chunks_v2(&gdn_geometry, &states)
                    .map_err(|e| Base0FpRecomputeError::Map(e.to_string()))?
            };
            // **The walk is the PRODUCER's** (audit B, H-1). `base0_composed_state_chunks_v1` is
            // the one enumeration of a class's checkpoint layout and
            // `Base0CheckpointCaptureV1::push_composed_v1` takes it too, so a seat's chunk `i` is
            // the executor's chunk `i` by construction rather than by two derivations agreeing.
            return crate::legs::base0_composed_state_chunks_v1(
                &crate::legs::Base0CaptureGeometryV1::Hybrid(geometry),
                |entry| self.attn_chunk_bytes_v1(entry),
                &gdn_chunks,
            )
            .map_err(|e| Base0FpRecomputeError::Map(format!("{e:?}")));
        }
        if declared == map::hybrid_state_chunk_map_id_v1() || declared == map::hybrid_state_chunk_map_id_v2() {
            return Err(Base0FpRecomputeError::NoStateSerializer {
                why: "the v1 and v2 hybrid compositions name an attention half and a recurrence half and no side of the tree \
                      enumerates them; the v3 composition does (palw_state_chunk_map::hybrid_state_chunk_entry_v3) and is what a \
                      graph-v5 hybrid registers (ADR-0082 Decision 4)",
            });
        }
        Err(Base0FpRecomputeError::Map(format!("this family serves no geometry for the map {declared}")))
    }
}

// =================================================================================================
// One forward pass, not two
// =================================================================================================

/// **The last state this process recomputed, keyed by the question it answers.**
///
/// A seat asks twice about the same interval: once for the 64-byte root
/// (`PalwBackend::fp_recompute_checkpoint_root`, the panel's comparison) and once for the replay
/// that resumes from the state. Both are the SAME forward pass, and Decision 9 prices a seat at
/// one — so the second question is answered from here rather than by running the job again.
///
/// One entry, and deliberately: the memo exists to join two calls a panel makes back to back
/// about one duty, not to be a cache of the fleet's claims. The key is every input the state is a
/// function of, so a hit cannot be a state computed for another job, another class or another
/// call — and the value is the seat's own arithmetic either way, never anything received.
/// **The key is exactly what BOTH askers can compute**, which is what makes the second question
/// answerable at all: the row check (`PalwBackend::verify_fp_interval_opening`) is handed an
/// opening and no ids, so it can name the class, the job context, the prompt and the covered call
/// — and nothing else.
///
/// That the OUTPUT ids are not in the key is a statement about the class rather than a shortcut. A
/// PALW class is a pinned integer computation: one job context and one prompt determine the ids,
/// so two entries that agree on this key and disagree on the ids cannot both be honest, and the
/// seat's own comparison against the committed checkpoint is what says which one was not. The ids
/// the state was computed from are kept in the VALUE so a caller can say which they were.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SeatStateKeyV1 {
    class_id: Hash64,
    context_hash: Hash64,
    prompt_ids_hash: Hash64,
    /// **The checkpoint LEAF's own counter**, not a decode call — the unit the class's cadence
    /// counts in (`palw_checkpoint_covered_at_index_v1`). Both askers name a checkpoint and both
    /// name it by this number, so the key holds it verbatim; keying it in calls while the class
    /// counts positions is why a graph-v5 seat could hold the right state and never find it.
    /// `class_id` carries the registered map, so one number cannot mean two things under one key.
    covered: u32,
}

static SEAT_STATE_MEMO: std::sync::Mutex<Option<(SeatStateKeyV1, Hash64, Base0FpSeatStateV1)>> = std::sync::Mutex::new(None);

fn seat_state_key_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt_token_ids: &[u32],
    covered: u32,
) -> SeatStateKeyV1 {
    SeatStateKeyV1 {
        class_id: profile.shape_profile_id(),
        context_hash: ctx.context_hash(),
        prompt_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(prompt_token_ids),
        covered,
    }
}

/// [`base0_fp_recompute_state_at_covered_v1`], with the state kept for the row check that follows
/// it.
///
/// `covered` is the checkpoint leaf's own counter — the class's cadence unit — because that is the
/// number an opening carries and the number the row check will look the state up by.
///
/// `kernels` is built by the caller and only used on a miss, so a family pays for its engine setup
/// once per real recompute.
pub fn base0_fp_seat_state_memoized_v1<K: Base0FpRecomputeKernelsV1 + ?Sized>(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt_token_ids: &[u32],
    output_token_ids: &[u32],
    covered: u32,
    kernels: &mut K,
) -> Result<Base0FpSeatStateV1, Base0FpRecomputeError> {
    let key = seat_state_key_v1(profile, ctx, prompt_token_ids, covered);
    let ids = kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(output_token_ids);
    if let Ok(guard) = SEAT_STATE_MEMO.lock()
        && let Some((held, held_ids, state)) = guard.as_ref()
        && *held == key
        && *held_ids == ids
    {
        return Ok(state.clone());
    }
    let state = base0_fp_recompute_state_at_covered_v1(profile, ctx, prompt_token_ids, output_token_ids, covered, kernels)?;
    if let Ok(mut guard) = SEAT_STATE_MEMO.lock() {
        *guard = Some((key, ids, state.clone()));
    }
    Ok(state)
}

/// **The state this seat recomputed for this class, context, prompt and covered call, if it is
/// still the last one it computed** — the row check's only way to reach it.
///
/// `None` is not a fault and never an accusation: it says this seat has not done the recompute
/// this interval's replay would have to resume from, so it cannot judge the interval, and
/// `Unverifiable` is the honest verdict. The recompute is the caller's to order
/// (`PalwBackend::fp_recompute_checkpoint_root`), because only the caller holds the committed
/// output ids the teacher-forcing needs.
pub fn base0_fp_seat_state_held_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt_token_ids: &[u32],
    covered: u32,
) -> Option<Base0FpSeatStateV1> {
    let key = seat_state_key_v1(profile, ctx, prompt_token_ids, covered);
    let guard = SEAT_STATE_MEMO.lock().ok()?;
    let (held, _ids, state) = guard.as_ref()?;
    (*held == key).then(|| state.clone())
}

/// Drop the memo. For tests that measure how many forward passes a sequence costs, and for a node
/// that wants the memory back.
pub fn base0_fp_seat_state_forget_v1() {
    if let Ok(mut guard) = SEAT_STATE_MEMO.lock() {
        *guard = None;
    }
}

// =================================================================================================
// The seat's window bound (Decision 9's first consequence)
// =================================================================================================

/// **How many positions a seat prefills per DAA**, in milli-positions so the quotient survives
/// integer arithmetic — the `rate_seat_prefill` Decision 9 names.
///
/// Derived from a measurement and a cadence, never chosen: `cadence_ms / ms_per_position`, with
/// the cadence the frozen block time the court's own clocks are counted in
/// (`PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS`) and `ms_per_position` whatever the measurement
/// reported. `0` for a host that cannot prefill at all, which admits no width.
pub const fn base0_fp_seat_milli_positions_per_daa_v1(ms_per_position: u64) -> u64 {
    if ms_per_position == 0 {
        return u64::MAX;
    }
    (kaspa_consensus_core::palw_mode_v2::PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS.saturating_mul(1_000)) / ms_per_position
}

/// **`n_max = window_receipt × rate_seat_prefill`** (Decision 9), in positions.
///
/// The two quantities are the ruleset's receipt window and the measured rate; nothing else enters,
/// and neither number is typed at any call site. A seat that cannot recompute the job inside the
/// window it has to file in files `Incapable` — so a row above this bound is a row nobody seats,
/// and a row nobody seats certifies nothing (ADR-0075).
pub const fn base0_fp_seat_width_bound_v1(window_receipt: u64, milli_positions_per_daa: u64) -> u64 {
    window_receipt.saturating_mul(milli_positions_per_daa) / 1_000
}

/// **The recompute, timed** — where `rate_seat_prefill` comes from on a host that actually ran it.
///
/// Returns the state and the milliseconds ONE position cost, over every position the recompute
/// folded (the prefill and the teacher-forced decode calls). It is the same run
/// [`base0_fp_recompute_state_v1`] performs, wall-clocked: a rate measured any other way would be
/// a rate for some other work.
pub fn base0_fp_seat_measure_v1<K: Base0FpRecomputeKernelsV1 + ?Sized>(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt_token_ids: &[u32],
    output_token_ids: &[u32],
    decode_calls: u32,
    kernels: &mut K,
) -> Result<(Base0FpSeatStateV1, u64), Base0FpRecomputeError> {
    let started = std::time::Instant::now();
    let state = base0_fp_recompute_state_v1(profile, ctx, prompt_token_ids, output_token_ids, decode_calls, kernels)?;
    let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let per_position = base0_fp_seat_ms_per_position_v1(elapsed_ms, state.positions);
    Ok((state, per_position))
}

/// The milliseconds one prefilled position cost, from a measured wall clock over `positions`.
/// Rounded UP: a rate rounded up is a bound rounded down, and the direction that is safe for a
/// deadline is the one that admits fewer positions.
pub fn base0_fp_seat_ms_per_position_v1(elapsed_ms: u64, positions: u32) -> u64 {
    if positions == 0 {
        return u64::MAX;
    }
    elapsed_ms.div_ceil(positions as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bound is the product of two ruleset quantities and nothing else — the rule BRIEF §7
    /// states, pinned by naming both.
    #[test]
    fn the_width_bound_is_the_window_times_the_rate() {
        let windows = kaspa_consensus_core::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1;
        // A host that prefills a position in one second: the frozen cadence is 120 s, so 120
        // positions per DAA and 600 DAA of receipt window admit 72,000 positions.
        let rate = base0_fp_seat_milli_positions_per_daa_v1(1_000);
        assert_eq!(rate, 120_000, "120 s cadence / 1,000 ms per position, in milli-positions");
        assert_eq!(base0_fp_seat_width_bound_v1(windows.window_receipt, rate), 600 * 120);
        // Ten times slower is ten times fewer positions: the bound tracks the measurement.
        let slow = base0_fp_seat_milli_positions_per_daa_v1(10_000);
        assert_eq!(base0_fp_seat_width_bound_v1(windows.window_receipt, slow), 600 * 12);
    }

    /// A host that cannot prefill at all admits nothing, and the arithmetic says so rather than
    /// dividing by zero.
    #[test]
    fn a_host_that_never_finishes_admits_no_width() {
        assert_eq!(base0_fp_seat_ms_per_position_v1(1_000, 0), u64::MAX);
        assert_eq!(base0_fp_seat_width_bound_v1(600, 0), 0);
    }

    /// The rate rounds in the direction that admits FEWER positions: a measurement of 1.5 ms per
    /// position is charged at 2, never at 1.
    #[test]
    fn the_measured_rate_rounds_against_the_width() {
        assert_eq!(base0_fp_seat_ms_per_position_v1(3, 2), 2);
    }

    /// **The dense tier's own recompute reaches its own committed checkpoints** — the family that
    /// registers graph-v5 first, on a real A16 capture rather than on the shared driver alone.
    ///
    /// This is the half of Z5 that a floor-only test cannot reach: the kernels, the cache, and the
    /// serializer under the class's four-byte map are the dense tier's, and a seat that wired any
    /// of the three differently from the executor would produce an honest root that is not the
    /// committed one — a seat that accuses every honest producer of the class.
    #[test]
    fn the_dense_tiers_recompute_reaches_its_committed_checkpoints() {
        use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v2};
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
        let profile = qwen25_a16_profile_v2(geometry).expect("a valid A16 profile");
        assert_ne!(profile.state_chunk_map_id, Hash64::default(), "the v2 class declares a map, or it takes no checkpoints");
        let shape = crate::artifact::Base0ShapeV1 {
            n_layers: geometry.layer_count as usize,
            n_heads: geometry.attn_heads as usize,
            n_kv_heads: geometry.attn_kv_heads as usize,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: crate::artifact::LN_THETA_10000_GEN_Q,
            eps_q: geometry.rms_eps_q,
        };
        let artifact = crate::artifact::Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(crate::engine_a16::derived_a16_store(&shape))
            .expect("the derived store is sorted and unique");
        let (ctx, prompt) =
            crate::produce::base0_rc_job_v1(&profile, Hash64::from_u64_word(0xA16_5EA7), geometry.vocab_size as usize, 3, 4);
        let run = crate::qwen25_a16_backend::a16_execute_for_attempt_v1(&artifact, &profile, None, &ctx, &prompt)
            .expect("the dense fixture runs");
        assert!(!run.checkpoints.leaves.is_empty(), "the fixture must commit at least one checkpoint");

        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        for leaf in &run.checkpoints.leaves {
            let mut kernels = A16RecomputeKernelsV1::new(&artifact, None).expect("the dense kernels");
            let state =
                base0_fp_recompute_state_v1(&profile, &ctx, &ids, &run.generated_token_ids, leaf.covered_decode_call, &mut kernels)
                    .expect("the seat can run the dense fixture");
            assert_eq!(
                state.state_chunks_root, leaf.state_chunks_root,
                "checkpoint {} (covering call {}): the seat's recompute must reach the committed state root",
                leaf.checkpoint_index, leaf.covered_decode_call
            );
            assert_eq!(state.positions, ctx.declared_prefill_tokens + leaf.covered_decode_call);
        }
    }

    /// **Z5 at every POSITION, not only at every decode call** (ADR-0082 Decision 4, amended).
    ///
    /// The graph-v5 dense row registers the tiled map, so its leg commits a checkpoint after every
    /// position — prefill included — and its `covered_decode_call` counts POSITIONS. A seat has to
    /// reach those roots too, which is what `base0_fp_recompute_state_at_position_v1` exists for:
    /// the per-call entry cannot stop inside the prefill at all, and before this the prefill
    /// checkpoints simply did not exist to be compared against.
    ///
    /// The comparison is the same one Z5 has always made — the seat's OWN arithmetic against the
    /// committed leaf — and it now covers the positions where the amendment's whole benefit is.
    #[test]
    fn the_dense_tiers_recompute_reaches_its_committed_checkpoints_at_every_position() {
        use kaspa_consensus_core::palw_context_ladder::{PalwCheckpointCadenceV1, palw_checkpoint_cadence_v1};
        use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v5};
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
        // **The REGISTERED graph-v5 row, graph and map together.** The fused attention node is
        // executed here — by the PLANNED walk, `PlanOp::AttnFused`, which is the one authority
        // that serves it (`the_fused_arm_is_the_reference_composition` proves that arm bit-equal
        // to `a16_attn_fused_reference_v1` and to the tile route). What cannot execute it is the
        // plan-LESS route: `A16Engine::forward_token_traced` is the compiled twenty-seven-row v2
        // program, so `a16_execute_for_attempt_v1` with `plan: None` refuses a v5 declaration by
        // name ("per-layer declares 24 against 27 recorded"). Passing the compiled plan is
        // therefore not a convenience here — it is the difference between exercising the class the
        // genesis registers and exercising a stand-in for it.
        let profile = qwen25_a16_profile_v5(geometry).expect("a valid graph-v5 A16 profile");
        assert_eq!(
            profile.state_chunk_map_id,
            kaspa_consensus_core::palw_state_chunk_map::tiled_kv_state_chunk_map_id_v3(),
            "a graph-v5 row registers the tiled map (ADR-0082 Decision 4) — this test is about that pairing"
        );
        assert_eq!(palw_checkpoint_cadence_v1(&profile), PalwCheckpointCadenceV1::PerPosition, "the tiled map is per-position");
        let shape = crate::artifact::Base0ShapeV1 {
            n_layers: geometry.layer_count as usize,
            n_heads: geometry.attn_heads as usize,
            n_kv_heads: geometry.attn_kv_heads as usize,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: crate::artifact::LN_THETA_10000_GEN_Q,
            eps_q: geometry.rms_eps_q,
        };
        let artifact = crate::artifact::Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(crate::engine_a16::derived_a16_store(&shape))
            .expect("the derived store is sorted and unique");
        let engine = crate::engine_a16::A16Engine::new(&artifact).expect("the fixture is an A16 artifact");
        let plan = engine.plan_from_profile(&profile).expect("the v5 declaration is this engine's program");
        let (ctx, prompt) =
            crate::produce::base0_rc_job_v1(&profile, Hash64::from_u64_word(0xA16_5EA7), geometry.vocab_size as usize, 3, 4);
        let run = crate::qwen25_a16_backend::a16_execute_for_attempt_v1(&artifact, &profile, Some(&plan), &ctx, &prompt)
            .expect("the dense v5 fixture runs");

        // Every position the cache ever holds has a checkpoint, prefill included.
        let expected = ctx.declared_prefill_tokens + ctx.exact_decode_tokens.saturating_sub(1);
        assert_eq!(run.checkpoints.leaves.len() as u32, expected, "prefill {} + decode calls", ctx.declared_prefill_tokens);
        assert!(ctx.declared_prefill_tokens >= 2, "the fixture must have prefill positions to check");

        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let mut prefill_checked = 0u32;
        for leaf in &run.checkpoints.leaves {
            let positions = leaf.covered_decode_call; // POSITIONS, on this cadence
            let mut kernels = A16RecomputeKernelsV1::new(&artifact, Some(&plan)).expect("the dense kernels");
            let state =
                base0_fp_recompute_state_at_position_v1(&profile, &ctx, &ids, &run.generated_token_ids, positions, &mut kernels)
                    .expect("the seat can stop at any position");
            assert_eq!(
                state.state_chunks_root, leaf.state_chunks_root,
                "checkpoint {} (covering {positions} positions): the seat's recompute must reach the committed state root",
                leaf.checkpoint_index
            );
            assert_eq!(state.positions, positions);
            if positions <= ctx.declared_prefill_tokens {
                prefill_checked += 1;
            }
        }
        assert_eq!(prefill_checked, ctx.declared_prefill_tokens, "every PREFILL position's checkpoint was compared");

        // A tampered root names the checkpoint, which is Z5's second half.
        let leaf = &run.checkpoints.leaves[1];
        let mut kernels = A16RecomputeKernelsV1::new(&artifact, Some(&plan)).expect("the dense kernels");
        let state = base0_fp_recompute_state_at_position_v1(
            &profile,
            &ctx,
            &ids,
            &run.generated_token_ids,
            leaf.covered_decode_call + 1,
            &mut kernels,
        )
        .expect("a neighbouring position also recomputes");
        assert_ne!(state.state_chunks_root, leaf.state_chunks_root, "two positions must not share a state root");
    }

    /// **The hybrid composition has a serializer, and it is stream F's own enumeration**
    /// (ADR-0082 Decision 4; stream G's patch note 5).
    ///
    /// `Qwen36RecomputeKernelsV1::state_chunks` refused the v3 hybrid map by NAME — "no side of the
    /// tree spells the order they compose in" — and `hybrid_state_chunk_entry_v3` now does. This
    /// walks it: the attention tiles first, at the per-position cadence, with the chunk lengths the
    /// composition declares and the sections in the order the map's own name spells.
    ///
    /// # What this test does NOT reach, and why
    ///
    /// The RECURRENCE half at a spacing boundary. The gdn v2 map's gather
    /// (`conv-head-gather=[q:h*k, k:heads*k+h*k, v:2*heads*k+h*v]`) is written over ONE head count,
    /// so `base0_gdn_state_geometry_v2`'s `conv_width` is `(2·k_dim + v_dim) · heads` — which equals
    /// the engine's own `2·linear_k_dim + linear_v_dim` only when `gdn_k_heads == gdn_v_heads`. The
    /// only hybrid fixture in this crate (`qwen36_dev_fixture`) has 2 and 4, so the recurrence
    /// serializer refuses it, and it refuses it identically through the gdn-only maps that shipped
    /// before this arm existed. That is a finding about the MAP, recorded rather than papered over;
    /// this test asserts the refusal names the geometry rather than the composition, so a fixture
    /// with matching head counts turns it green without editing the assertion.
    #[test]
    fn the_hybrid_composition_serializes_in_the_order_its_map_name_spells() {
        use kaspa_consensus_core::palw_context_ladder::{
            PalwCheckpointCadenceV1, palw_anchored_interval_for_profile_v1, palw_checkpoint_cadence_v1,
            palw_checkpoint_leaf_carries_recurrence_v1,
        };
        use kaspa_consensus_core::palw_state_chunk_map as map;

        let (artifact, profile) = crate::fuzz_qwen36::tiny_class_v5_for_tests();
        assert_eq!(profile.state_chunk_map_id, map::hybrid_state_chunk_map_id_v3(), "a v5 hybrid registers the composition");
        assert_eq!(palw_checkpoint_cadence_v1(&profile), PalwCheckpointCadenceV1::PerPosition);

        let engine = crate::qwen36::Qwen36Engine::new(&artifact);
        let plan = engine.plan_from_profile(&profile).expect("the fixture's declaration is its program");
        let mut kernels = Qwen36RecomputeKernelsV1::new(&artifact, &plan);
        let spacing = palw_anchored_interval_for_profile_v1(&profile);
        let run_to = spacing.max(1) as usize;
        for position in 0..run_to {
            kernels.forward_no_capture(position % artifact.shape.vocab, position).expect("the fixture runs");
        }

        // **The attention half, at every position it rides — which is all of them.**
        let mut reached_a_boundary = false;
        for positions in 1..=run_to as u32 {
            let carries = palw_checkpoint_leaf_carries_recurrence_v1(&profile, positions);
            let geometry = map::hybrid_state_geometry_for_covered_v1(&profile, positions).expect("the composition derives");
            assert_eq!(geometry.gdn_chunk_count() > 0, carries, "the recurrence rides only its own spacing");
            assert!(geometry.attn.chunk_count() > 0, "the attention half is on EVERY leaf");
            // The order the NAME spells: `attn=` before `gdn=`, checked against the enumeration.
            for index in 0..geometry.chunk_count() {
                let entry = map::hybrid_state_chunk_entry_v3(&geometry, index).expect("the entry");
                let want = if index < geometry.attn.chunk_count() {
                    map::PalwHybridChunkSectionV1::AttentionCache
                } else {
                    map::PalwHybridChunkSectionV1::RecurrenceState
                };
                assert_eq!(entry.section(), want, "chunk {index} is in the wrong section");
            }

            match kernels.state_chunks(&profile, positions) {
                Ok(chunks) => {
                    assert_eq!(chunks.len() as u64, geometry.chunk_count(), "one chunk per entry the composition names");
                    for (index, bytes) in chunks.iter().enumerate() {
                        let entry = map::hybrid_state_chunk_entry_v3(&geometry, index as u64).expect("the entry");
                        assert_eq!(bytes.len() as u64, entry.byte_len(), "chunk {index} is not the length the map declares");
                    }
                    assert!(!carries, "a leaf carrying the recurrence serialized on a fixture whose head counts do not match");
                }
                Err(why) => {
                    // The ONLY refusal this arm may still make on a v3 map is the recurrence
                    // geometry's, and it must name the geometry — never the composition.
                    assert!(carries, "the attention-only composition must serialize, and it refused: {why}");
                    let text = why.to_string();
                    assert!(
                        text.contains("convolution window") || text.contains("geometry"),
                        "the refusal must name the recurrence geometry, not the composition: {text}"
                    );
                    assert!(
                        !text.contains("no side of the tree"),
                        "the composition is still refused BY NAME — patch note 5 did not land: {text}"
                    );
                    reached_a_boundary = true;
                }
            }
        }
        assert!(reached_a_boundary, "the sweep must reach a position the recurrence rides");
        // And the fixture's own head counts are why: recorded here so the refusal above is
        // attributable to the MAP rather than to this arm.
        assert_ne!(
            artifact.shape.linear_k_heads, artifact.shape.linear_v_heads,
            "this fixture's head counts now match — the recurrence half should serialize and this test's Err arm is dead"
        );
    }

    /// **The memo answers the second question with the first question's pass.**
    ///
    /// A seat asks twice about one interval — once for the 64-byte root, once for the state its
    /// replay resumes from — and Decision 9 prices a seat at ONE forward pass of the job. The
    /// counter here is the number of forwards the kernels actually performed.
    #[test]
    fn the_second_question_costs_no_second_forward_pass() {
        struct CountingKernels {
            forwards: std::cell::Cell<u32>,
            chunks: Vec<Vec<u8>>,
        }
        impl Base0FpRecomputeKernelsV1 for CountingKernels {
            fn forward_no_capture(&mut self, _token: usize, _position: usize) -> Result<(), Base0FpRecomputeError> {
                self.forwards.set(self.forwards.get() + 1);
                Ok(())
            }
            fn state_chunks(&self, _profile: &PalwShapeProfileV3, _positions: u32) -> Result<Vec<Vec<u8>>, Base0FpRecomputeError> {
                Ok(self.chunks.clone())
            }
        }

        let geometry = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY;
        let profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(geometry).expect("expressible");
        let (ctx, prompt) = crate::produce::base0_rc_job_v1(&profile, Hash64::from_u64_word(7), geometry.vocab_size as usize, 3, 4);
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let output = vec![1u32, 2, 3];
        let mut kernels = CountingKernels { forwards: std::cell::Cell::new(0), chunks: vec![vec![0u8; 8]] };

        base0_fp_seat_state_forget_v1();
        let first = base0_fp_seat_state_memoized_v1(&profile, &ctx, &ids, &output, 1, &mut kernels).expect("a state");
        let after_first = kernels.forwards.get();
        assert_eq!(after_first, ids.len() as u32 + 1, "the prefill plus one teacher-forced decode call");
        // The row check's question, asked the way `verify_fp_interval_opening` asks it: by the
        // class, the context, the prompt and the covered call — the four things an opening names.
        let held = base0_fp_seat_state_held_v1(&profile, &ctx, &ids, 1).expect("the state is held");
        assert_eq!(held, first, "the second question is answered with the first question's state");
        assert_eq!(kernels.forwards.get(), after_first, "and it costs no second forward pass");

        // A different covered call is a different question, and is not answered from the memo.
        assert!(base0_fp_seat_state_held_v1(&profile, &ctx, &ids, 2).is_none());
        base0_fp_seat_state_forget_v1();
        assert!(base0_fp_seat_state_held_v1(&profile, &ctx, &ids, 1).is_none(), "forgetting is what a node gets its memory back with");
    }

    /// **What the seat's one forward pass actually costs, on this host** — the measurement
    /// ADR-0082 §4 "Seat time" asserts and Decision 9's width bound is a function of.
    ///
    /// It prints rather than asserts a duration: a wall clock on a shared laptop is not a
    /// consensus fact, and a test that failed when the machine was busy would be a test that
    /// measures the fleet's mood. What it DOES assert is the thing the certifier depends on —
    /// that a measurement can only ever narrow the admitted width, never widen it — because that
    /// is the direction a wrong number is safe in.
    ///
    /// Run it with `--nocapture` to read the numbers.
    #[test]
    fn what_one_seat_forward_pass_costs_on_this_host() {
        // The floor's own RC class: real weights, the shipped geometry, this build's kernels.
        let geometry = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY;
        let profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(geometry).expect("expressible");
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
            crate::rc::PALW_RC_BASE0_SEED,
        )
        .expect("the RC floor shape");
        let prefill = (geometry.n_ctx / 2).max(1);
        let decode = 4u32;
        let (ctx, prompt) =
            crate::produce::base0_rc_job_v1(&profile, Hash64::from_u64_word(0x5EA7), geometry.vocab_size as usize, prefill, decode);
        let run = crate::produce::base0_execute_for_attempt_v1(&artifact, &profile, &ctx, &prompt).expect("the floor runs");
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let covered = ctx.exact_decode_tokens.saturating_sub(1);

        struct FloorRecompute<'a> {
            engine: crate::engine::Base0Engine<'a>,
            cache: crate::engine::KvCache,
        }
        impl Base0FpRecomputeKernelsV1 for FloorRecompute<'_> {
            fn forward_no_capture(&mut self, token: usize, position: usize) -> Result<(), Base0FpRecomputeError> {
                self.engine
                    .forward_token(&mut self.cache, token, position)
                    .map(|_| ())
                    .map_err(|e| Base0FpRecomputeError::Engine(format!("{e:?}")))
            }
            fn state_chunks(&self, profile: &PalwShapeProfileV3, positions: u32) -> Result<Vec<Vec<u8>>, Base0FpRecomputeError> {
                let geometry = crate::legs::base0_state_chunk_geometry_v1(profile, positions)
                    .map_err(|e| Base0FpRecomputeError::Map(format!("{e:?}")))?;
                let mut chunks = Vec::with_capacity(geometry.chunk_count() as usize);
                for index in 0..geometry.chunk_count() {
                    let entry = kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_entry_v1(&geometry, index)
                        .ok_or(Base0FpRecomputeError::StateIsNotTheMaps { chunk_index: index })?;
                    chunks.push(
                        self.cache.state_chunk_bytes(&entry).ok_or(Base0FpRecomputeError::StateIsNotTheMaps { chunk_index: index })?,
                    );
                }
                Ok(chunks)
            }
        }

        base0_fp_seat_state_forget_v1();
        let mut kernels =
            FloorRecompute { engine: crate::engine::Base0Engine::new(&artifact), cache: crate::engine::KvCache::new(&artifact) };
        let started = std::time::Instant::now();
        let state = base0_fp_recompute_state_v1(&profile, &ctx, &ids, &run.generated_token_ids, covered, &mut kernels)
            .expect("the seat runs the floor's RC job");
        let elapsed = started.elapsed();
        assert_eq!(state.positions, prefill + covered, "one pass over every position of the job");

        let micros_per_position = elapsed.as_micros().max(1) / state.positions.max(1) as u128;
        let measured_ms = base0_fp_seat_ms_per_position_v1(elapsed.as_millis().min(u64::MAX as u128) as u64, state.positions);
        let windows = kaspa_consensus_core::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1;
        println!(
            "seat forward pass (PALW-BASE-0/rc geometry, {} positions): {:?} total, {micros_per_position} us/position, \
             {measured_ms} ms/position rounded up",
            state.positions, elapsed
        );
        for (name, cost) in [
            ("PALW-BASE-0/A16", kaspa_consensus_core::palw_context_ladder::PALW_COURT_COST_A16),
            ("PALW-QWEN36", kaspa_consensus_core::palw_context_ladder::PALW_COURT_COST_QWEN36),
        ] {
            let ms = cost.replay_ms_per_position();
            let rate = base0_fp_seat_milli_positions_per_daa_v1(ms);
            println!(
                "  {name}: {ms} ms/position (fleet, ADR-0077 §4) -> {}.{:03} positions/DAA -> n_max {} at window_receipt {}",
                rate / 1_000,
                rate % 1_000,
                base0_fp_seat_width_bound_v1(windows.window_receipt, rate),
                windows.window_receipt
            );
        }

        // **The one assertion**: a measurement can only narrow the width. The certifier takes the
        // SLOWER of the fleet figure and the host's, so a host that is faster than the row's
        // figure — every host this fixture runs on — changes nothing, and a slower one binds.
        let fleet = kaspa_consensus_core::palw_context_ladder::PALW_COURT_COST_QWEN36.replay_ms_per_position();
        assert!(
            base0_fp_seat_width_bound_v1(windows.window_receipt, base0_fp_seat_milli_positions_per_daa_v1(fleet.max(measured_ms)))
                <= base0_fp_seat_width_bound_v1(windows.window_receipt, base0_fp_seat_milli_positions_per_daa_v1(fleet)),
            "taking the slower of the two measurements must never admit MORE positions"
        );
    }
}
