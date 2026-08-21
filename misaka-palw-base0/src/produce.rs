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

pub const PALW_BASE0_DOMAIN_LOGITS_TRACE: &[u8] = b"misaka-palw/base0/logits-trace/v1";
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
        }
    }
}

impl std::error::Error for ProduceError {}

/// **The integer class's logits trace root.**
///
/// One keyed hash over the context, the shape of the run, and every logits row the job produced,
/// as `i32` little-endian — the lanes the engine actually computes. See the module docs for why
/// this is not `full_logits_trace_root_v2`.
pub fn base0_logits_trace_root_v1(ctx: &PalwJobContextV2, logits_rows: &[Vec<i32>], generated_token_ids: &[u32]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_BASE0_DOMAIN_LOGITS_TRACE).to_state();
    h.update(ctx.context_hash().as_byte_slice());
    h.update(&(ctx.declared_prefill_tokens as u64).to_le_bytes());
    h.update(&(ctx.exact_decode_tokens as u64).to_le_bytes());
    h.update(&(logits_rows.len() as u64).to_le_bytes());
    for row in logits_rows {
        h.update(&(row.len() as u64).to_le_bytes());
        for v in row {
            h.update(&v.to_le_bytes());
        }
    }
    h.update(&(generated_token_ids.len() as u64).to_le_bytes());
    for t in generated_token_ids {
        h.update(&t.to_le_bytes());
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

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
    }

    let tiles = capture.finish().map_err(ProduceError::Leg)?;
    let trace_root = base0_logits_trace_root_v1(ctx, &logits_rows, &generated);
    let activation_leg_root = base0_activation_leg_root_v1(ctx);
    let binding = crate::legs::base0_binding_from_capture_v1(profile, ctx, &tiles, trace_root, activation_leg_root)
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
        generated_token_ids: generated,
    })
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

    /// **The capture covers the WHOLE step space** — which is the property the roots are worth
    /// nothing without.
    ///
    /// Before the pre and post tables were captured, a leg committed zero leaves for the embedding
    /// gather, the final norm, its narrowing and the logits head — so the node that decides what
    /// the model actually said was the one part of the graph no refutation could open, and the
    /// commitment could not tell "computed zero" from "never computed". `finish` refuses a short
    /// capture now, so this test failing is the same event as a producer refusing to publish.
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
        let mut adjudicated = 0usize;
        let mut unadjudicable = 0usize;
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
            ) {
                Ok(r) => r,
                Err(e) => panic!("leaf {leaf} at {coord:?} could not even be assembled: {e:?}"),
            };
            match check_execution_step_refutation_v1(&refutation, &oracle) {
                Err(PalwStepRefuteError::NoFaultFound) => adjudicated += 1,
                Err(PalwStepRefuteError::Unadjudicable) => {
                    unadjudicable += 1;
                    // The only known-open unadjudicable is a DECODE-call embedding gather, and it
                    // is unadjudicable (cannot check) rather than mis-convicted (safe). Its token
                    // is a generated id whose BASE-0 commitment rides `base0_logits_trace_root_v1`,
                    // the integer trace root, while the court's decode-token check recomputes the
                    // v2 event-tree root — the integer-leg dispatch, a separate item. Pinning it
                    // means a NEW hole (an attention node, say) fails this test rather than hiding
                    // in a loose count.
                    let (n, _) = profile.resolve_node_slot(coord.node_slot).unwrap();
                    assert!(
                        n.op_kind == PalwStepOpKindV1::EmbedLookup && coord.call_index > 0,
                        "an unexpected leaf is unadjudicable — {:?} at call {} pos {} tile {}; only decode-embed is known-open",
                        n.op_kind,
                        coord.call_index,
                        coord.position,
                        coord.tile_index
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
        // And the sweep has to have actually adjudicated something, or it proves nothing.
        assert!(adjudicated > 0, "no leaf was adjudicated at all");
        println!("swept {leaves} leaves: {adjudicated} adjudicated NoFaultFound, {unadjudicable} unadjudicable");

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
            let binding =
                crate::legs::base0_binding_from_capture_v1(&profile, &ctx, &lying, run.trace_root, base0_activation_leg_root_v1(&ctx))
                    .expect("a tampered capture still commits");
            let refutation =
                crate::legs::base0_refutation_from_capture_v1(&profile, &ctx, &lying, binding, coord, ids.clone()).expect("assembles");
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
        let material: Base0RetainedMaterialV1 = (run.binding.clone(), run.tiles.tiles.clone(), run.generated_token_ids.clone());

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

// ---------------------------------------------------------------------------------------------
// The panel's half: reading back retained material and deciding a verdict
// ---------------------------------------------------------------------------------------------

/// The execution material a producer retains for `trace_retention_daa`, as it is stored.
///
/// `(binding, tiles, generated ids)` — everything a seat needs to decide whether the producer
/// committed to what it actually computed, and everything a refutation is assembled from later.
pub type Base0RetainedMaterialV1 = (
    kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    Vec<(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1)>,
    Vec<u32>,
);

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
    let (binding, tiles, _generated) = material;
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
    // And the binding the producer kept must be the one its CLAIM committed — otherwise it retained
    // a consistent execution of some other job.
    Ok(binding.committed_execution_root == committed_execution_root && binding.full_logits_trace_root == committed_trace_root)
}
