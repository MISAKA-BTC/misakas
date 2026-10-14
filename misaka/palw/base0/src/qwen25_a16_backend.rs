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
use crate::classes::ArtifactSourceV1;
use crate::engine_a16::{A16Cache, A16Engine, A16PlanErrorV1};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_step::{PALW_STEP_MAX_LEAVES, PalwShapeProfileV3};
use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, output_commitment_v2};
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
/// **Positions a dense prefill runs through each layer together** (ADR-0117 Decision 2). Each of a
/// layer's projections reads its weights once for the run instead of once a position, so the
/// weights are read `⌈prefill / run⌉` times rather than `prefill`; the rows a run commits (a
/// position's whole trace, a few megabytes on the 1.5B row) are held until the run is pushed, which
/// is what bounds it. A node-local choice: the rows, their order and every root are the stepped
/// walk's whatever the run.
///
/// Measured on the real 1.5B artifact and testnet-11's graph-v5 row (a 12-core M-series host, the
/// prompt the `one_pass_prefill_on_the_real_dense_row` probe builds): 508 positions — the 512 row's
/// canonical prefill — step at 36.3 ms a position, and run at 16.3 ms in runs of 32 and 14.7 ms in
/// runs of 64 (2.45×); 256 positions, 33.8 against 15.4 ms. Every row, the cache and the logits the
/// same bits.
///
/// **The default, not the only width** (int-10.2 A2): a backend runs at its own
/// `prefill_run_positions` ([`Qwen25A16Backend::with_prefill_run_positions`], the trait's
/// `set_prefill_run_positions_v1`), which starts here; a node narrows it when the memory a replay can
/// be granted does not hold this width's trace scratch, and the instance it narrowed is the one that
/// runs. The free functions below keep this width.
pub const A16_PREFILL_RUN_POSITIONS: usize = 64;

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
        None,
        crate::engine_a16::KV_STORAGE_SHIPPED_V1,
        A16_PREFILL_RUN_POSITIONS,
        &mut |row: &[i32]| kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(row) as u32,
    )
}

/// **The one capture core, under the caller's storage profile.** Every wrapper above and below
/// reaches this with `KV_STORAGE_SHIPPED_V1`; the backend reaches it with the profile it was built
/// to hold (`Qwen25A16Backend::with_runtime_profile`). The capture kind, the drill's lie and the
/// streamed ids are the wrappers' arguments spelled out — so there is one loop, whichever
/// representation the cache is in, and the roots it commits are the same under all of them
/// (`the_backend_commits_the_same_roots_under_every_runtime_profile`).
#[allow(clippy::too_many_arguments)]
pub fn a16_execute_in_storage_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    plan: Option<&crate::engine_a16::A16ProfilePlanV1>,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
    capture_kind: crate::legs::Base0CaptureKindV1,
    on_token: &mut dyn FnMut(u32),
    drill_fault_leaf: Option<u64>,
    kv_storage: crate::engine_a16::KvStorageProfileV1,
) -> Result<crate::produce::Base0ExecutionV1, String> {
    a16_execute_in_storage_at_run_v1(
        artifact,
        profile,
        plan,
        ctx,
        prompt,
        max_step_leaf_count,
        capture_kind,
        on_token,
        drill_fault_leaf,
        kv_storage,
        A16_PREFILL_RUN_POSITIONS,
    )
}

/// [`a16_execute_in_storage_v1`] at a stated prefill run width (int-10.2 A2): the one capture core,
/// walking the prompt `run_positions` positions at a time where the plan allows a run (one at a time
/// where it does not, whatever is asked). The width is node-local — every width commits the same rows
/// and roots (`the_backend_commits_the_same_roots_at_every_prefill_run_width`) — and sizes the trace
/// scratch the resource profile prices; a backend passes its own (`Qwen25A16Backend::prefill_run_positions`).
#[allow(clippy::too_many_arguments)]
pub fn a16_execute_in_storage_at_run_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    plan: Option<&crate::engine_a16::A16ProfilePlanV1>,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
    capture_kind: crate::legs::Base0CaptureKindV1,
    on_token: &mut dyn FnMut(u32),
    drill_fault_leaf: Option<u64>,
    kv_storage: crate::engine_a16::KvStorageProfileV1,
    run_positions: usize,
) -> Result<crate::produce::Base0ExecutionV1, String> {
    a16_execute_streaming_v1(
        artifact,
        profile,
        plan,
        ctx,
        prompt,
        max_step_leaf_count,
        capture_kind,
        on_token,
        drill_fault_leaf.map(A16DrillLieV1::tile_v1),
        kv_storage,
        run_positions,
        // The replay selector (RFC-0001 §A): the shipped argmax unless the caller scoped a claim's
        // rule — a re-execution of a V4 claim's fold selects as that claim did.
        &mut {
            let mut row_index = 0u32;
            move |row: &[i32]| {
                let id = kaspa_consensus_core::palw_decode_pipeline_v4::palw_fp_replay_select_v1(row, row_index);
                row_index += 1;
                id
            }
        },
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
        None,
        crate::engine_a16::KV_STORAGE_SHIPPED_V1,
        A16_PREFILL_RUN_POSITIONS,
        &mut |row: &[i32]| kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(row) as u32,
    )
}

/// **DRILL ONLY: the same folded run, with one committed tile made a lie** — the held liar a
/// seat, a naming and a court are exercised against at a job's real size, where the attempt lane's
/// dense fault (`execute_with_injected_fault`) cannot reach: past the materialization cap a dense
/// capture is refused, and a fold hashes each tile as it is produced, so the lie is made in the
/// stream. The execution itself is honest — only the committed row at `leaf` is moved, as the
/// floor's drill moves a tile (`corrupt_capture_v1`) — so every other leaf is a seat's own and
/// the fault is at exactly `leaf`. Never reached on a network carrying value (the trait's contract).
#[allow(clippy::too_many_arguments)]
pub fn a16_execute_free_prompt_streaming_with_drill_fault_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    plan: Option<&crate::engine_a16::A16ProfilePlanV1>,
    ctx: &PalwJobContextV2,
    prompt: &[usize],
    max_step_leaf_count: u64,
    on_token: &mut dyn FnMut(u32),
    leaf: u64,
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
        Some(A16DrillLieV1::tile_v1(leaf)),
        crate::engine_a16::KV_STORAGE_SHIPPED_V1,
        A16_PREFILL_RUN_POSITIONS,
        &mut |row: &[i32]| kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(row) as u32,
    )
}

/// **DRILL ONLY: the lie as a run and every replay of its job apply it** (ADR-0152 §4-ter N5): a
/// committed tile moved — at the row lane `lane` (`None`: the tile's first) by `delta`, the
/// execution honest — or the engine's fused attention output moved and FOLLOWED
/// ([`crate::engine_a16::A16DrillAttnLieV1`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum A16DrillLieV1 {
    Tile { leaf: u64, lane: Option<usize>, delta: i32 },
    Attn(crate::engine_a16::A16DrillAttnLieV1),
}

impl A16DrillLieV1 {
    /// The shipped drill's lie: the tile at `leaf`, its first lane, by one.
    fn tile_v1(leaf: u64) -> Self {
        Self::Tile { leaf, lane: None, delta: 1 }
    }

    /// The trait's named fault under `ctx`: where its tile lies, or where its engine does.
    fn of_fault_v1(
        fault: &kaspa_consensus_core::palw_backend::PalwFreePromptDrillFaultV1,
        profile: &PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
    ) -> Result<Self, String> {
        use kaspa_consensus_core::palw_backend::PalwFreePromptDrillFaultV1;
        use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepOpKindV1, PalwStepTableV1};
        match *fault {
            PalwFreePromptDrillFaultV1::Leaf { leaf } => Ok(Self::tile_v1(leaf)),
            PalwFreePromptDrillFaultV1::AttnOutput { call, layer, position, head, lane, delta, follow } => {
                let fused = profile
                    .attn_nodes
                    .iter()
                    .position(|n| n.op_kind == PalwStepOpKindV1::AttnFused)
                    .ok_or("the class has no fused attention site")?;
                let slot = profile
                    .global_node_slot(PalwStepTableV1::Attn, layer, fused)
                    .ok_or_else(|| format!("layer {layer} has no fused site"))?;
                let (node, _) = profile.resolve_node_slot(slot).ok_or("the fused slot resolves to no node")?;
                if head >= profile.attn_heads || u32::from(lane) >= profile.attn_head_dim {
                    return Err(format!("head {head} lane {lane} is outside the fused row"));
                }
                let row_lane = usize::from(head) * profile.attn_head_dim as usize + usize::from(lane);
                if follow {
                    let absolute =
                        if call == 0 { position as usize } else { ctx.declared_prefill_tokens as usize + call as usize - 1 };
                    return Ok(Self::Attn(crate::engine_a16::A16DrillAttnLieV1 {
                        layer: usize::from(layer),
                        position: absolute,
                        lane: row_lane,
                        delta,
                        cache: None,
                    }));
                }
                let tile_len = node.tile_len.max(1) as usize;
                let coord =
                    PalwStepCoordinateV1 { call_index: call, node_slot: slot, position, tile_index: (row_lane / tile_len) as u32 };
                let leaf = kaspa_consensus_core::palw_step::canonical_step_leaf_index(profile, ctx, &coord)
                    .ok_or("the fused site's coordinate is not in this job")?;
                Ok(Self::Tile { leaf, lane: Some(row_lane), delta })
            }
            // ADR-0152 §4-ter.3 step 6's forger: the cache holds the row moved, the step row honest; the
            // engine follows it, so every replay and recompute of the job does too.
            PalwFreePromptDrillFaultV1::CacheRow { call, layer, position, kind, lane, delta } => {
                if kind > 1 {
                    return Err(format!("cache kind {kind} is neither K (0) nor V (1)"));
                }
                if usize::from(layer) >= profile.layer_count as usize {
                    return Err(format!("layer {layer} is outside the class"));
                }
                if u32::from(lane) >= u32::from(profile.attn_kv_heads) * profile.attn_head_dim {
                    return Err(format!("lane {lane} is outside the cache row"));
                }
                let absolute = if call == 0 { position as usize } else { ctx.declared_prefill_tokens as usize + call as usize - 1 };
                Ok(Self::Attn(crate::engine_a16::A16DrillAttnLieV1 {
                    layer: usize::from(layer),
                    position: absolute,
                    lane: usize::from(lane),
                    delta,
                    cache: Some(kind),
                }))
            }
        }
    }

    fn attn(&self) -> Option<crate::engine_a16::A16DrillAttnLieV1> {
        match self {
            Self::Attn(lie) => Some(*lie),
            Self::Tile { .. } => None,
        }
    }
}

/// **A drill's moved code.** The shipped tile drill moves its lane by one, wrapping (`in_range =
/// false`, byte for byte what it always did); a fused output's lie (ADR-0152 §4-ter N5) stays an A16
/// code — the other way when `delta` would leave the range — so the forger it commits is one whose
/// tile a root claim can finalize to, the dispute the drill exists to reach.
fn a16_drill_moved_v1(value: i32, delta: i32, in_range: bool) -> i32 {
    if !in_range {
        return value.wrapping_add(delta);
    }
    let up = value.saturating_add(delta);
    if (-32_767..=32_767).contains(&up) { up } else { value.saturating_sub(delta) }
}

/// Where a drill's lie at `leaf` lands: the call and position its tile is committed at, the row
/// (table, layer, index within the table) it is cut from, the lane it moves, by how much, and whether
/// the moved code stays an A16 code ([`a16_drill_moved_v1`]).
struct A16DrillFaultSiteV1 {
    call: u32,
    position: u32,
    table: kaspa_consensus_core::palw_step::PalwStepTableV1,
    layer: u16,
    index: usize,
    lane: usize,
    delta: i32,
    in_range: bool,
}

impl A16DrillFaultSiteV1 {
    fn of_leaf(profile: &PalwShapeProfileV3, ctx: &PalwJobContextV2, leaf: u64) -> Result<Self, String> {
        use kaspa_consensus_core::palw_step::PalwStepTableV1;
        let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(profile, ctx, leaf)
            .ok_or_else(|| format!("the drill's leaf {leaf} is not a main step coordinate"))?;
        let (node, layer) =
            profile.resolve_node_slot(coord.node_slot).ok_or_else(|| format!("slot {} names no node", coord.node_slot))?;
        use kaspa_consensus_core::palw_step::PalwLayerKindV1;
        // The slot walk `resolve_node_slot` takes: pre, then each layer's kind's table, then post.
        let table_len = |l: u16| match profile.layer_kind(l) {
            PalwLayerKindV1::GatedDeltaNet => profile.gdn_nodes.len() as u32,
            PalwLayerKindV1::Attention => profile.attn_nodes.len() as u32,
        };
        let pre = profile.pre_nodes.len() as u32;
        let (table, layer, index) = match layer {
            None if coord.node_slot < pre => (PalwStepTableV1::Pre, 0, coord.node_slot as usize),
            None => {
                let before: u32 = pre + (0..profile.layer_count).map(table_len).sum::<u32>();
                (PalwStepTableV1::Post, 0, (coord.node_slot - before) as usize)
            }
            Some(layer) => {
                let before: u32 = pre + (0..layer).map(table_len).sum::<u32>();
                let table = match profile.layer_kind(layer) {
                    PalwLayerKindV1::GatedDeltaNet => PalwStepTableV1::Gdn,
                    PalwLayerKindV1::Attention => PalwStepTableV1::Attn,
                };
                (table, layer, (coord.node_slot - before) as usize)
            }
        };
        let lane = coord.tile_index as usize * node.tile_len.max(1) as usize;
        Ok(Self { call: coord.call_index, position: coord.position, table, layer, index, lane, delta: 1, in_range: false })
    }

    /// Move the committed row the site names, when these are its call's rows.
    fn apply(&self, call: u32, position: u32, rows: &mut [crate::legs::Base0CapturedRowV1]) -> Result<(), String> {
        if (call, position) != (self.call, self.position) {
            return Ok(());
        }
        let row = rows
            .iter_mut()
            .find(|r| r.table == self.table && r.layer == self.layer && r.index == self.index)
            .ok_or_else(|| "the drill's fault lands on no committed row".to_string())?;
        let value = row.row.get_mut(self.lane).ok_or_else(|| "the drill's fault lands past its row".to_string())?;
        *value = a16_drill_moved_v1(*value, self.delta, self.in_range);
        Ok(())
    }
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
    drill: Option<A16DrillLieV1>,
    kv_storage: crate::engine_a16::KvStorageProfileV1,
    run_positions: usize,
    // The id each selecting row feeds forward: the shipped argmax on the attempt lane, the job's
    // decoder on the free-prompt lane (RFC-0001 §A.3, `palw_fp_decode_run_v1`).
    select: &mut dyn FnMut(&[i32]) -> u32,
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

    // DRILL ONLY (ADR-0152 §4-ter N5): an attention lie the run follows is the engine's own.
    let engine = A16Engine::new(artifact)
        .map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?
        .with_drill_attn_lie_v1(drill.and_then(|lie| lie.attn()));
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
    let fault = match drill {
        Some(A16DrillLieV1::Tile { leaf, lane, delta }) if leaf < leaf_count => {
            let site = A16DrillFaultSiteV1::of_leaf(profile, ctx, leaf)?;
            Some(A16DrillFaultSiteV1 { lane: lane.unwrap_or(site.lane), delta, in_range: lane.is_some(), ..site })
        }
        Some(A16DrillLieV1::Tile { leaf, .. }) => {
            return Err(format!("the drill's leaf {leaf} is outside the job's {leaf_count} leaves"));
        }
        Some(A16DrillLieV1::Attn(_)) | None => None,
    };
    let mut capture = crate::legs::Base0CaptureSinkV1::for_kind(capture_kind, profile, ctx, leaf_count, max_step_leaf_count)
        .map_err(|e| format!("{e:?}"))?;
    let checkpoint_profile = map::integer_kv_checkpoint_profile_v1(map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1);
    let mut checkpoints = crate::legs::Base0CheckpointCaptureV1::new(ctx, profile, &checkpoint_profile);
    // The cache in the caller's representation, its whole run reserved up front: the job's rows
    // are known here, so the buffers never double and never copy mid-walk (the resource profile's
    // `kv_resident_bytes` is exactly this reservation, and `resident_bytes_v1` reports it).
    let mut cache = A16Cache::with_storage(artifact.shape.n_layers, kv_storage);
    cache.reserve_positions(prefill + decode_tokens.saturating_sub(1), artifact.shape.kv_dim());

    // **The memory brackets** (`memory_phase`): what the engine holds, term by term, beside what the
    // process holds — at the cache's construction, every bracket of positions, and at each seal.
    // Integer facts only on this path (`tests/float_free.rs`): the GiB and the rate are formatted
    // in `memory_phase`, which is off the execution path.
    let started = std::time::Instant::now();
    let anon_at_start = crate::memory_phase::process_anon_bytes_v1();
    let bracket = |at: &str,
                   positions: usize,
                   cache: &A16Cache,
                   capture: &crate::legs::Base0CaptureSinkV1,
                   leg: &crate::legs::Base0CheckpointCaptureV1| {
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
                storage: cache.storage().name(),
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

    // Call 0 — prefill. Logits leaves exist only at its LAST position; the earlier rows predict
    // tokens the prompt already contains, so the capture drops their Post rows (steps this class's
    // step space does not have) and the retention keeps only the selecting row.
    //
    // **A run of positions at a time through each layer** (ADR-0117 Decision 2, the dense tier's
    // half): `forward_prefill_planned` walks the registered plan a layer at a time over
    // `run_positions` positions (the caller's width, int-10.2 A2; `A16_PREFILL_RUN_POSITIONS` unless a
    // node narrowed it), each projection's weights read once for the run
    // instead of once a position, and hands back the rows the stepped walk commits in the order it
    // commits them. The runs are pushed in position order, so the capture — dense or fold — sees
    // what it always saw. A checkpoint after a prefill position (the held map's, after every one)
    // is taken after its run: the cache only appends, and a checkpoint after position `p` reads rows
    // `0..=p` by index, which the run left exactly as the stepped walk would have.
    let mut last_logits = Vec::new();
    let run_plan = plan.filter(|plan| plan.one_pass_prefill_supported());
    let mut position = 0usize;
    while position < prefill {
        let end = match run_plan {
            Some(_) => (position + run_positions.max(1)).min(prefill),
            None => position + 1,
        };
        let last = end == prefill;
        let (logits, traces) = match run_plan {
            Some(plan) => engine
                .forward_prefill_planned(plan, &mut cache, &prompt[position..end], position, last)
                .map_err(|e| format!("the prefill run at {position}: {e:?}"))?,
            None => {
                let (logits, trace) =
                    forward(&engine, &mut cache, prompt[position], position).map_err(|e| format!("prefill at {position}: {e}"))?;
                (logits, vec![trace])
            }
        };
        for (k, trace) in traces.iter().enumerate() {
            let at = position + k;
            let mut rows = crate::legs::a16_captured_rows_v1(trace);
            if at + 1 != prefill {
                rows.retain(|r| r.table != kaspa_consensus_core::palw_step::PalwStepTableV1::Post);
            }
            if let Some(fault) = &fault {
                fault.apply(0, at as u32, &mut rows)?;
            }
            capture.push_call(profile, ctx, 0, at as u32, &rows).map_err(|e| format!("{e:?}"))?;
            // **A checkpoint after a PREFILL position, when the class's cadence says so** (ADR-0082
            // Decision 4, amended). A per-call class wants none of these and this is `false` at every
            // prefill position; a class whose map addresses history tiles wants one after every
            // position, because a dispute at a prefill position with no anchor opens `p + 1` cache
            // rows per kind and its bottom is three chunks no carrier can file.
            if checkpoints.wants_checkpoint_after_v1(0, at as u32) {
                checkpoints
                    .push_with_v1(|entry| cache.state_chunk_bytes_v1(entry))
                    .map_err(|e| format!("the prefill checkpoint at position {at}: {e:?}"))?;
            }
        }
        if last {
            last_logits = logits;
        }
        if position / crate::memory_phase::PALW_MEMORY_BRACKET_POSITIONS_V1
            != end / crate::memory_phase::PALW_MEMORY_BRACKET_POSITIONS_V1
        {
            bracket("prefill bracket", end, &cache, &capture, &checkpoints);
        }
        position = end;
    }
    bracket("prefill done", prefill, &cache, &capture, &checkpoints);
    let mut next = select(&last_logits);
    generated.push(next);
    on_token(next);
    // RFC-0001 P1: a cancelled run stops at the next token.
    crate::cancel::check_abort_v1()?;
    logits_rows.push(last_logits);

    // Calls 1..=D−1 — decode. The COORDINATE's position is 0 in every decode call (each call has
    // one position); the cache position is absolute. Conflating them lands every decode row on
    // top of the first one's.
    for call in 1..decode_tokens {
        let cache_position = prefill + call - 1;
        let (logits, trace) =
            forward(&engine, &mut cache, next as usize, cache_position).map_err(|e| format!("decode at {cache_position}: {e}"))?;
        let mut rows = crate::legs::a16_captured_rows_v1(&trace);
        if let Some(fault) = &fault {
            fault.apply(call as u32, 0, &mut rows)?;
        }
        capture.push_call(profile, ctx, call as u32, 0, &rows).map_err(|e| format!("{e:?}"))?;
        next = select(&logits);
        generated.push(next);
        on_token(next);
        crate::cancel::check_abort_v1()?;
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

    bracket("decode done", prefill + decode_tokens.saturating_sub(1), &cache, &capture, &checkpoints);
    let checkpoints = checkpoints.finish_canonical_v1().map_err(|e| format!("{e:?}"))?;
    let captured = capture.finish(max_step_leaf_count).map_err(|e| format!("{e:?}"))?;
    crate::memory_phase::execution_phase_v1(|| {
        crate::memory_phase::sealed_line_v1(captured.step_leaf_count, cache.resident_bytes_v1(), started.elapsed().as_millis() as u64)
    });

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
    // The cache is dropped here; what the return carries is the binding, the tiles or the tree,
    // the checkpoint leg and the logits rows.
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
    /// **The NETWORK's ladder — the materialization cap** (ADR-0121 Decision 1): the ruleset's
    /// `PalwCourtParamsV2::max_step_leaf_count`, not the leg's module constant.
    ///
    /// It bounds a `step_leaf_count` that arrived over gossip inside a borsh blob BEFORE the leaf
    /// vector is built, so it cannot simply be dropped; and it was the leg's default constant, so
    /// a class registered against a deeper frozen ladder was refused by its own executor. It
    /// defaults to that constant — every shipped construction site keeps exactly the behaviour it
    /// had — and [`Qwen25A16Backend::with_step_ladder_cap`] is how a caller that HOLDS the ruleset
    /// states the real one.
    ///
    /// **Two knobs where there was one.** A class under the held regime is walked at the regime's
    /// `2^40` (ADR-0119 Decision 1), and at that ladder the whole-capture paths — a dense
    /// re-execution, a leaf vector, a dense material's check — would let a gossiped count ask for
    /// `2^46` bytes. So this field stays the network's and bounds every job-sized site
    /// ([`Self::materialize_cap`]); [`Self::step_ladder_cap`] is the class's, derived from the
    /// profile, and prices, refuses and walks. For every class but a held one the two are equal.
    network_ladder: u64,
    /// The network's prompt-commitment form (ADR-0081 Decision 3); see `Base0Backend::prompt_ids_form`.
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    /// **DRILL ONLY: the job this instance lied in, and the leaf** — set by
    /// [`PalwExecutionBackendV1::execute_free_prompt_with_injected_fault`], read by every replay this
    /// instance runs for that job's context, so the liar's openings, blocks and evidence are its lie
    /// (a fold keeps no tiles: what it serves it replays). Keyed by the job's context hash, so the
    /// same instance judges every other job honestly. `None` everywhere but a drill.
    drill_fault: std::sync::Mutex<Option<(Hash64, A16DrillLieV1)>>,
    /// **ADR-0152 §4-ter N4's selector** ([`Self::with_held_answerability_v1`]): whether the chain this
    /// backend serves arms `palw_offence_attribution`. Past it
    /// [`PalwExecutionBackendV1::supports_dissection`] asks the consensus predicate
    /// (`palw_held_class_unanswerable_v1`, a pure function of the profile); `false` (every network
    /// without the fence) keeps it byte for byte what it was.
    held_answerability: bool,
    /// **The representation the cache is held in** (ADR-0151 follow-up): a node-local, named
    /// choice the resource profile prices and the telemetry reports. The same canonical execution
    /// commits the same rows and roots under every value — that is what makes it a runtime profile
    /// and not a class fact — and `KV_STORAGE_SHIPPED_V1` is what a backend built without
    /// saying holds.
    runtime_profile: kaspa_consensus_core::palw_resource_profile_v1::PalwRuntimeProfileV1,
    /// This instance's seat state and walk; nothing outside the instance reaches it.
    seat_memo: crate::fp_recompute::Base0FpSeatMemoV1,
    /// **RFC-0001 §2.6 stage 1: the node-only KV prefix cache** the answer-only path resumes from
    /// (`None`: no cache, every answer prefills fresh). Never read by a committed run.
    prefix_cache: Option<std::sync::Mutex<crate::prefix_cache::PrefixKvCacheV1>>,
    /// RFC-0001 §2.7 stage 2: the widest batched decode step.
    decode_batch_width: usize,
    /// **How many prompt positions this instance's prefill walks a layer at a time** (int-10.2 A2):
    /// [`A16_PREFILL_RUN_POSITIONS`] unless a node narrowed it for the memory it could reserve. Read by
    /// the resource profile (the trace scratch it prices) and by every execution this instance runs —
    /// one field, so the figure reserved and the run that spends it cannot be two numbers.
    prefill_run_positions: usize,
    /// **Which attempt rule this instance runs** (ADR-0152 v3.1, addendum §4-bis.1): the family's own
    /// (`Legacy`) or the chain's `CoreV1` — the job an anchor implies and the rendered rule of every
    /// output root, attempt and free prompt alike. Set by the node from its params
    /// (`set_attempt_rules_v1`); `Legacy` by default, so every other construction keeps its roots.
    attempt_rules: kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1,
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
        let shape_id = crate::artifact::artifact_digest_shared(&artifact);
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
            network_ladder: kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
            prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            drill_fault: std::sync::Mutex::new(None),
            held_answerability: false,
            runtime_profile: crate::engine_a16::KV_STORAGE_SHIPPED_V1,
            seat_memo: Default::default(),
            attempt_rules: Default::default(),
            prefill_run_positions: A16_PREFILL_RUN_POSITIONS,
            prefix_cache: None,
            decode_batch_width: 8,
        })
    }

    /// **Everything an answer does before its first selection**: the committed path's own checks,
    /// the prefix-cache lookup (with the sampled verification), the prefill of what the cache did
    /// not hold, and the cache fill. Returns the cache holding the whole prompt, how many leading
    /// positions came from the prefix cache, and the last prompt position's logits.
    fn answer_prefill_v1(
        &self,
        engine: &A16Engine<'_>,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
    ) -> Result<(A16Cache, usize, Vec<i32>), String> {
        self.artifact_read_probe_v1()?;
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
        if let Some(decode) = job.decode.as_ref().filter(|_| job.decodes_under_v4_rules())
            && decode.bans_cover_vocab(vocab as u32)
        {
            return Err("logit_bias bans every lane of this class's vocabulary: no position could commit a token".to_string());
        }
        let prompt_ids: Vec<u32> = prompt_tokens.iter().map(|t| *t as u32).collect();
        let prefill = prompt_tokens.len();
        let limit = job.decode_token_limit as usize;
        let (class_id, tokenizer_id) = (job.class_id, job.tokenizer_id);
        let mut resumed: Option<(usize, A16Cache)> = None;
        let mut verify = false;
        if let Some(lock) = &self.prefix_cache {
            let mut cache = lock.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(hit) = cache.lookup_v1(class_id, tokenizer_id, &prompt_ids) {
                verify = cache.wants_verification_v1();
                resumed = Some((hit.positions as usize, hit.cache));
            }
        }
        let (mut kv, mut reused, mut logits) = match resumed {
            Some((positions, mut kv)) => {
                kv.reserve_positions(prefill + limit, self.artifact.shape.kv_dim());
                let logits = self.answer_prefill_run_v1(engine, &mut kv, &prompt_tokens[positions..], positions)?;
                (kv, positions, logits)
            }
            None => {
                let mut kv = A16Cache::with_storage(self.artifact.shape.n_layers, self.runtime_profile);
                kv.reserve_positions(prefill + limit, self.artifact.shape.kv_dim());
                let logits = self.answer_prefill_run_v1(engine, &mut kv, prompt_tokens, 0)?;
                (kv, 0, logits)
            }
        };
        // Sampled verification: the resumed logits against a fresh prefill's. A difference turns the
        // cache off for the life of the process and the answer is served from the fresh run.
        if verify && reused > 0 {
            let mut fresh_kv = A16Cache::with_storage(self.artifact.shape.n_layers, self.runtime_profile);
            fresh_kv.reserve_positions(prefill + limit, self.artifact.shape.kv_dim());
            let fresh_logits = self.answer_prefill_run_v1(engine, &mut fresh_kv, prompt_tokens, 0)?;
            if fresh_logits != logits || fresh_kv.contents_as_i32() != kv.contents_as_i32() {
                eprintln!(
                    "[prefix-cache] a resumed prefill differed from a fresh one: the cache is DISABLED and this answer is served fresh"
                );
                if let Some(lock) = &self.prefix_cache {
                    lock.lock().unwrap_or_else(|e| e.into_inner()).disable_v1();
                }
                (kv, logits, reused) = (fresh_kv, fresh_logits, 0);
            }
        }
        if let Some(lock) = &self.prefix_cache {
            lock.lock().unwrap_or_else(|e| e.into_inner()).insert_v1(class_id, tokenizer_id, &prompt_ids, &kv);
        }
        Ok((kv, reused, logits))
    }

    /// Prefill `tokens` into `cache` from `first`, in runs of this node's own width exactly as the
    /// committed prefill walks them; the last position's logits.
    fn answer_prefill_run_v1(
        &self,
        engine: &A16Engine<'_>,
        cache: &mut A16Cache,
        tokens: &[usize],
        first: usize,
    ) -> Result<Vec<i32>, String> {
        match self.plan.as_ref() {
            Some(plan) if plan.one_pass_prefill_supported() => {
                let mut last = Vec::new();
                let mut at = 0usize;
                while at < tokens.len() {
                    let end = (at + self.prefill_run_positions.max(1)).min(tokens.len());
                    let is_last = end == tokens.len();
                    let (logits, _) = engine
                        .forward_prefill_planned(plan, cache, &tokens[at..end], first + at, is_last)
                        .map_err(|e| format!("the prefill run at {}: {e:?}", first + at))?;
                    if is_last {
                        last = logits;
                    }
                    at = end;
                }
                Ok(last)
            }
            Some(plan) => {
                let mut last = Vec::new();
                for (k, t) in tokens.iter().enumerate() {
                    last = engine
                        .forward_token_planned(plan, cache, *t, first + k)
                        .map_err(|e| format!("prefill at {}: {e:?}", first + k))?
                        .0;
                }
                Ok(last)
            }
            None => {
                let mut last = Vec::new();
                for (k, t) in tokens.iter().enumerate() {
                    last =
                        engine.forward_token_traced(cache, *t, first + k).map_err(|e| format!("prefill at {}: {e:?}", first + k))?.0;
                }
                Ok(last)
            }
        }
    }

    /// The whole conversation so far — prompt plus every answer id whose K/V the cache holds (the
    /// last selected id was never fed forward) — is what the next turn most often extends.
    fn remember_conversation_v1(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_ids: &[u32],
        generated: &[u32],
        kv: &A16Cache,
    ) {
        if let Some(lock) = &self.prefix_cache
            && generated.len() > 1
        {
            let mut ids = prompt_ids.to_vec();
            ids.extend_from_slice(&generated[..generated.len() - 1]);
            if kv.rows() == ids.len() {
                lock.lock().unwrap_or_else(|e| e.into_inner()).insert_v1(job.class_id, job.tokenizer_id, &ids, kv);
            }
        }
    }

    /// How many sequences one batched decode step carries at most (`--decode-batch`; default 8).
    pub fn with_decode_batch_width_v1(mut self, width: usize) -> Self {
        self.decode_batch_width = width.max(1);
        self
    }

    /// **Give this backend a KV prefix cache** of at most `budget_bytes` (RFC-0001 §2.6 stage 1;
    /// `verify_every` re-verifies every Nth hit against a fresh prefill, 0: never). Runs the boot
    /// self-check first — a resume that is not bit-identical to a fresh prefill leaves the cache
    /// DISABLED — and reports whether it is on.
    pub fn with_prefix_cache_v1(mut self, budget_bytes: u64, verify_every: u64) -> Self {
        let mut cache = crate::prefix_cache::PrefixKvCacheV1::new(budget_bytes).with_verify_every_v1(verify_every);
        if budget_bytes > 0 && !self.prefix_resume_self_check_v1(&mut cache) {
            eprintln!(
                "[prefix-cache] the boot self-check found a resumed prefill that is not bit-identical to a fresh one: the cache is DISABLED"
            );
        }
        self.prefix_cache = Some(std::sync::Mutex::new(cache));
        self
    }

    /// The cache's counters, `None` when this backend holds none.
    pub fn prefix_cache_stats_v1(&self) -> Option<crate::prefix_cache::PrefixCacheStatsV1> {
        self.prefix_cache.as_ref().map(|c| c.lock().unwrap_or_else(|e| e.into_inner()).stats_v1())
    }

    /// **The golden rule, run at boot**: prefill a fixed prompt fresh, prefill the same prompt in
    /// two pieces (the second resumed from a cache snapshot of the first), and require the same
    /// logits and the same K/V bits — at every split the prompt has. Returns whether the cache
    /// stays enabled (it is `disable_v1`-ed on the first difference).
    fn prefix_resume_self_check_v1(&self, cache: &mut crate::prefix_cache::PrefixKvCacheV1) -> bool {
        let Some(plan) = self.plan.as_ref() else { return true };
        let vocab = self.artifact.shape.vocab.max(1);
        let prompt: Vec<usize> = (0..6usize).map(|i| (i * 7 + 3) % vocab).collect();
        let Ok(engine) = A16Engine::new(&self.artifact) else {
            cache.disable_v1();
            return false;
        };
        let run = |cache: &mut A16Cache, tokens: &[usize], first: usize| -> Option<Vec<i32>> {
            if plan.one_pass_prefill_supported() {
                engine.forward_prefill_planned(plan, cache, tokens, first, true).ok().map(|(l, _)| l)
            } else {
                let mut last = Vec::new();
                for (k, t) in tokens.iter().enumerate() {
                    last = engine.forward_token_planned(plan, cache, *t, first + k).ok()?.0;
                }
                Some(last)
            }
        };
        let mut fresh = A16Cache::with_storage(self.artifact.shape.n_layers, self.runtime_profile);
        let Some(fresh_logits) = run(&mut fresh, &prompt, 0) else {
            cache.disable_v1();
            return false;
        };
        for split in 1..prompt.len() {
            let mut head = A16Cache::with_storage(self.artifact.shape.n_layers, self.runtime_profile);
            if run(&mut head, &prompt[..split], 0).is_none() {
                cache.disable_v1();
                return false;
            }
            let Some(mut resumed) = head.prefix_clone_v1(split) else {
                cache.disable_v1();
                return false;
            };
            let Some(resumed_logits) = run(&mut resumed, &prompt[split..], split) else {
                cache.disable_v1();
                return false;
            };
            if !cache.boot_check_v1(&(fresh_logits.clone(), fresh.clone()), &(resumed_logits, resumed)) {
                return false;
            }
        }
        true
    }

    /// **Hold the cache in `profile`'s representation** — `A16-KV-i32` (the oracle) or
    /// `A16-KV-i16`. A node-local choice: every root this backend commits is the same under either.
    pub fn with_runtime_profile(mut self, profile: kaspa_consensus_core::palw_resource_profile_v1::PalwRuntimeProfileV1) -> Self {
        self.runtime_profile = profile;
        self
    }

    /// **Walk the prefill `positions` at a time** (int-10.2 A2; `0` reads as one) — the builder form of
    /// [`PalwExecutionBackendV1::set_prefill_run_positions_v1`]. A node-local choice: every root this
    /// backend commits is the same at every width.
    pub fn with_prefill_run_positions(mut self, positions: usize) -> Self {
        self.prefill_run_positions = positions.max(1);
        self
    }

    /// The width this instance's prefill runs at.
    pub fn prefill_run_positions(&self) -> usize {
        self.prefill_run_positions
    }

    /// The job context the resource profile is derived on when a caller names none: this class's
    /// canonical job, with the commitments it does not read left zero.
    fn resource_job_v1(&self, prefill: u32, decode: u32) -> PalwJobContextV2 {
        PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: self.network_id.clone(),
            job_id: Hash64::default(),
            job_nullifier: Hash64::default(),
            assignment_id: Hash64::default(),
            execution_seed: [0; 32],
            model_profile_id: self.shape_id,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: self.shape_id,
            shape_profile_id: self.class_profile_id,
            trace_scheme_id: kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1(),
            cu_ruleset_id: Hash64::default(),
            tokenizer_id: self.artifact.tokenizer_commitment,
            prompt_token_ids_hash: Hash64::default(),
            declared_prefill_tokens: prefill,
            exact_decode_tokens: decode,
            max_context_tokens: self.artifact.shape.max_position as u32,
        }
    }

    /// The node-local limits the resource profile sizes its scratch terms with: this process's
    /// pool and this INSTANCE's prefill run width — the width its executions walk at, so the figure a
    /// node reserves for a run of this instance is the run's (int-10.2 A2).
    fn runtime_limits_v1(&self) -> kaspa_consensus_core::palw_resource_profile_v1::PalwRuntimeLimitsV1 {
        kaspa_consensus_core::palw_resource_profile_v1::PalwRuntimeLimitsV1 {
            threads: rayon::current_num_threads().max(1) as u32,
            prefill_run_positions: self.prefill_run_positions.max(1) as u32,
        }
    }

    /// **The ladder top from the ruleset**, for a caller that holds `PalwCourtParamsV2`. Passing
    /// `max_step_leaf_count` is the only correct argument; passing the leg's default constant is
    /// what both constructors already do. It is the NETWORK's number: a held class's own ladder is
    /// derived from it and the profile ([`Self::step_ladder_cap`]), never passed in.
    pub fn with_step_ladder_cap(mut self, max_step_leaf_count: u64) -> Self {
        self.network_ladder = max_step_leaf_count;
        self
    }

    /// The network's prompt-commitment form (ADR-0081 Decision 3); see `Base0Backend::prompt_ids_form`.
    /// Stored as THIS CLASS's form (ADR-0118 Decision 3): Merkle for a held class — the dense v7
    /// row — whatever the network's, so the job this backend derives and a seat's cannot disagree.
    pub fn with_prompt_ids_form(mut self, form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1) -> Self {
        self.prompt_ids_form = kaspa_consensus_core::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(form, &self.profile);
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
    /// prices a job at, refuses a capture above, and walks every Merkle path and every streamed
    /// replay against. A site that would build a vector of the job's leaves reads
    /// [`Self::materialize_cap`] instead.
    pub fn step_ladder_cap(&self) -> u64 {
        kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(self.network_ladder, &self.profile)
    }

    /// **The materialization cap** (ADR-0121 Decision 1, re-derived by the DoS audit of
    /// 2026-09-24, #4): the most leaves a whole-capture path (a dense re-execution of a fold, the
    /// whole-capture prover, the bisection's prefix, the fused site's honest re-execution) builds a
    /// vector of. Above it only the streamed routes answer, and the whole-capture ones refuse by
    /// name.
    ///
    /// It was the network's ladder, which testnet-12 mints at the held regime's `2^40`: the cap
    /// then equalled the held class's own ladder, the refusal never fired, and one held
    /// free-prompt claim made a sampling seat lay out 62 GiB (and, at 2M, 15,894 GiB) of tiles. It
    /// is now the HOST's number, [`crate::fp_interval::base0_materialize_cap_v1`]: never past the
    /// network's ladder, never past the historical `2^26`, never past what the operator's declared
    /// budget holds dense at this class's tile. Node policy — no root, price or verdict reads it.
    pub fn materialize_cap(&self) -> u64 {
        crate::fp_interval::base0_materialize_cap_v1(self.network_ladder, Some(&self.profile))
    }

    /// DRILL ONLY: the job context this instance's drill run lied in, and the lie, if it ran one.
    fn drill_fault_v1(&self) -> Option<(Hash64, A16DrillLieV1)> {
        *self.drill_fault.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// DRILL ONLY: the attention lie this instance's run of `ctx` followed, if it ran one — what
    /// every recompute of that job's state must follow too, or its own checkpoints would not root.
    fn drill_attn_for_v1(&self, ctx: &PalwJobContextV2) -> Option<crate::engine_a16::A16DrillAttnLieV1> {
        self.drill_fault_v1().filter(|(context, _)| *context == ctx.context_hash()).and_then(|(_, lie)| lie.attn())
    }

    /// **ADR-0152 §4-ter N4: whether the chain this backend serves is past `palw_offence_attribution`**
    /// — the node's `palw_offence_attribution_fence().is_some()`. Past it a held class answers a
    /// dissection only where an honest party can ([`PalwExecutionBackendV1::supports_dissection`], the
    /// predicate C1 and C5 read: the context bound, a recurrent layer, a compute turn past the cap).
    pub fn with_held_answerability_v1(mut self, armed: bool) -> Self {
        self.held_answerability = armed;
        self
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
    /// **The exemption is a property of the LANE, not of a byte inside the artifact** (audit D
    /// M-6). It was `artifact.is_derived()`, on the stated ground that "a derived artifact must
    /// never be reported as a registered class" — and nothing enforces either half of that.
    /// `derived_seed` is absent from `artifact_digest()` AND from the operand inventory, so the
    /// same bytes with the seed tag set have exactly the same `artifact_root`, a chain row still
    /// resolves to them, and `resolve_class_v1` / `dense_artifact_by_registered_root` match on
    /// that root alone. One flipped byte in a real dense artifact whose `tokenizer_commitment` is
    /// zero therefore bought a pass through this guard — and the floor class is itself
    /// `ArtifactSourceV1::Derived(..)` AND a registered class, so the invariant was already false
    /// on its face.
    ///
    /// `lane` is what the CALLER resolved the class as. Only [`ArtifactSourceV1::Derived`] — the
    /// seeded floor and the drill fixtures minted the same way — is exempt, and that is a fact
    /// about how the backend was reached, which no artifact byte can change. The chain-registered
    /// resolution path (`PalwClassSdk::resolve_chain_registered`) resolves `ConvertedA16` and is
    /// refused whatever the file says.
    pub fn check_tokenizer_declared_v1(artifact: &Base0ArtifactV1, lane: ArtifactSourceV1) -> Result<(), String> {
        if matches!(lane, ArtifactSourceV1::Derived(_)) || artifact.tokenizer_commitment != Hash64::default() {
            return Ok(());
        }
        Err(format!(
            "this artifact declares no tokenizer: `tokenizer_commitment` is all zeros, so every job this class \
             produces would publish `tokenizer_id` 0 and no replay could prove it read the ids this class means. \
             Bind one — `palw-class bind-tokenizer --network <id> --tokenizer <tokenizer.json> --out <path> --model-id \
             <this class's model id> <artifact>` sets that one field, reads the result back bound, and prints every \
             row's registered root before and after: a row that registers the INVENTORY root (the tiled-map rows — \
             graph-v2, graph-v3, graph-v5@512) keeps it, and a row that registers the artifact DIGEST gets a new \
             artifact and a genesis decision, because the commitment is inside `artifact_digest`. Re-converting with a \
             tokenizer bound (`qwen25-convert <checkpoint-dir> --a16 --out <path>`, tokenizer.json beside \
             model.safetensors) is the route that needs no existing artifact (artifact digest {})",
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
        // The lane a caller that did NOT resolve a class is in: a drill, a gate binary or a
        // fixture. It is the artifact's own answer, which is the reading audit D M-6 found is not
        // enforceable — so it is confined to this entry point, and every caller that resolved a
        // CLASS uses `from_registered_profile_in_lane_v1` and says which lane it resolved.
        let lane = artifact.derived_seed().map_or(ArtifactSourceV1::ConvertedA16, ArtifactSourceV1::Derived);
        Self::from_registered_profile_in_lane_v1(artifact, network_id, profile, canonical_job, lane)
    }

    /// [`Self::from_registered_profile`] with the lane the caller RESOLVED the class in
    /// (audit D M-6).
    ///
    /// The chain-registered path passes [`ArtifactSourceV1::ConvertedA16`]: no dense class the
    /// chain registers is the seeded floor, so no chain-registered row is ever exempt from the
    /// tokenizer refusal — whatever `derived_seed` a file carries, since neither the digest nor
    /// the inventory root covers it.
    pub fn from_registered_profile_in_lane_v1(
        artifact: std::sync::Arc<Base0ArtifactV1>,
        network_id: Vec<u8>,
        profile: PalwShapeProfileV3,
        canonical_job: (u32, u32),
        lane: ArtifactSourceV1,
    ) -> Result<Self, String> {
        Self::check_tokenizer_declared_v1(&artifact, lane)?;
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
        let shape_id = crate::artifact::artifact_digest_shared(&artifact);
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
            network_ladder: kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
            prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            drill_fault: std::sync::Mutex::new(None),
            held_answerability: false,
            runtime_profile: crate::engine_a16::KV_STORAGE_SHIPPED_V1,
            seat_memo: Default::default(),
            attempt_rules: Default::default(),
            prefill_run_positions: A16_PREFILL_RUN_POSITIONS,
            prefix_cache: None,
            decode_batch_width: 8,
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
        let mut cache = A16Cache::with_storage(self.artifact.shape.n_layers, self.runtime_profile);
        cache.reserve_positions(prompt.len() + job.exact_decode_tokens as usize, self.artifact.shape.kv_dim());
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
    /// The free-prompt run behind the trait's verbs: the honest one (`drill_fault = None`) and the
    /// drills', whose committed tile is a lie ([`a16_execute_free_prompt_streaming_with_drill_fault_v1`])
    /// or whose engine lies in a fused attention output (ADR-0152 §4-ter N5).
    fn execute_free_prompt_v1(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        on_token: &mut dyn FnMut(u32),
        drill_fault: Option<kaspa_consensus_core::palw_backend::PalwFreePromptDrillFaultV1>,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        use kaspa_consensus_core::palw_fp_execution_v3::{
            PalwFpClassFactsV3, PalwFpRunFactsV3, palw_fp_job_context_v3, palw_fp_run_facts_for_executed_v1,
        };

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
        // run. The pairing is DERIVED from the count (ADR-0074 Decision 7) rather than typed here,
        // so that a seat rebuilding this context for an early-stopping claim gets the run that
        // happened instead of the budget that was asked for.
        //
        // **RFC-0001 §A.3: the count is the job's DECODER's** (`palw_fp_decode_run_v1`): the budget
        // for a V3 job, run once exactly as before; for a V4 job, where its pipeline stops — the
        // capture runs at the budget and, if a stop sequence or an empty admitted set ended the
        // answer earlier, once more at exactly that count. Each context is built BEFORE its run and
        // run under: `palw_fp_execution_root_v3` recomputes the court's root from it, so an
        // execution carried out under any other one commits a root nobody can reproduce.
        let prompt_ids: Vec<u32> = prompt_tokens.iter().map(|t| *t as u32).collect();
        // **The one capture path this family has** (ADR-0049 Decision F's probe, the checkpoint
        // serializer at the class's declared width, and the selecting-rows retention all live in
        // it). The free-prompt lane differs from the attempt lane only in where its context, its
        // tokens and its selection rule come from, so the run itself must not be a second
        // implementation.
        let ((run, shape, ctx, drill), _stop) = kaspa_consensus_core::palw_decode_pipeline_v4::palw_fp_decode_run_v1(
            job,
            vocab as u32,
            on_token,
            |count, select, stream| {
                let shape = palw_fp_run_facts_for_executed_v1(job, count);
                let ctx = palw_fp_job_context_v3(job, &class, &shape, &self.network_id).map_err(|e| format!("{e:?}"))?;
                let drill = drill_fault.map(|fault| A16DrillLieV1::of_fault_v1(&fault, &self.profile, &ctx)).transpose()?;
                let run = a16_execute_streaming_v1(
                    &self.artifact,
                    &self.profile,
                    self.plan.as_ref(),
                    &ctx,
                    prompt_tokens,
                    self.step_ladder_cap(),
                    crate::legs::Base0CaptureKindV1::Fold,
                    stream,
                    drill,
                    self.runtime_profile,
                    self.prefill_run_positions,
                    select,
                )?;
                Ok((run, shape, ctx, drill))
            },
            |(run, _, _, _)| run.generated_token_ids.as_slice(),
        )?;
        if let Some(lie) = drill {
            // What this instance serves for the job from now on replays the lie it committed.
            *self.drill_fault.lock().unwrap_or_else(|e| e.into_inner()) = Some((ctx.context_hash(), lie));
        }

        // The four legs, measured — the derived roots the execution root is built from, which is
        // what `palw_fp_execution_root_v3` recomputes.
        let (checkpoint_leg_root, step_leg_root) = crate::legs::base0_leg_roots_from_binding_v1(&run.binding);
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
        // **Refused by name past the materialization cap, before a tile is produced** (DoS audit
        // 2026-09-24, #4). The binding's count is the relayed blob's claim; the re-execution below is
        // capped at the same number, so a blob that understates its count while its context names a
        // larger job is refused by the engine's own count too, not laid out.
        if let Some(why) = crate::fp_interval::base0_whole_capture_refusal_v1(
            material.binding.step_leaf_count,
            self.materialize_cap(),
            self.step_ladder_cap(),
        ) {
            return Err(why);
        }
        #[cfg(test)]
        FOLD_REEXECUTIONS_V1.with(|n| n.set(n.get() + 1));
        let prompt: Vec<usize> = material.prompt_token_ids.iter().map(|t| *t as usize).collect();
        // An HONEST dense re-execution, in this instance's own cache representation: a fold is judged
        // by re-deriving it, and a re-derivation that reproduced a lie would be evidence of nothing.
        // **The committed ids are fed back** (RFC-0001 §A): the arithmetic is re-derived, the token
        // choices are the fold's own — which is what every honest V3 fold committed anyway (the
        // shipped argmax) and the only way to re-derive a V4 fold, whose tokens follow its job's
        // pipeline. A wrong token choice is the decode close's question, not this conversion's.
        let run = kaspa_consensus_core::palw_decode_pipeline_v4::palw_fp_with_replay_rule_v1(
            Some(kaspa_consensus_core::palw_decode_pipeline_v4::PalwFpReplayRuleV1::forced(&material.generated_token_ids)),
            || {
                a16_execute_in_storage_at_run_v1(
                    &self.artifact,
                    &material.binding.shape_profile,
                    self.plan.as_ref(),
                    &material.binding.job_context,
                    &prompt,
                    self.materialize_cap(),
                    crate::legs::Base0CaptureKindV1::DenseTiles,
                    &mut |_| {},
                    None,
                    self.runtime_profile,
                    self.prefill_run_positions,
                )
            },
        )?;
        if run.binding.committed_execution_root != material.binding.committed_execution_root {
            return Err("the retained fold and its re-execution are not one execution".to_string());
        }
        Ok(run)
    }

    /// **What a fused site's evidence is read from, out of one capture** (ADR-0093 as built): the
    /// capture's OWN committed rows (retained dense tiles; a fold is re-executed and must reproduce
    /// its own root), the checkpoint leg from a re-execution (none is retained under the
    /// per-position cadence), and the prompt the job ran — carried for a free prompt (refused
    /// unless the job commits to it), re-derived from the anchor for an attempt.
    ///
    /// The rows are the capture's, never the re-execution's, because the bottom must open what the
    /// CLAIM committed: a challenger closing against a forged output tile opens the forged tile.
    #[allow(clippy::type_complexity)]
    fn attn_capture_parts_v1(
        &self,
        material: &[u8],
        carried_prompt: Option<&[u32]>,
        accused_out_tile_carried: bool,
    ) -> Result<
        (
            kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
            crate::legs::Base0StepTilesV1,
            bool,
            crate::legs::Base0CheckpointsV1,
            Vec<u32>,
            Vec<u32>,
        ),
        String,
    > {
        let retention =
            crate::produce::base0_material_decode_any_v1(material).map_err(|_| "the capture does not decode".to_string())?;
        let binding = retention.binding().clone();
        let prompt_ids = self.attn_prompt_ids_v1(&binding, carried_prompt)?;
        let generated = retention.generated_token_ids().to_vec();
        let rerun = self.attn_rerun_v1(&binding, &prompt_ids)?;
        // `true` in the middle: the rows are an honest PREFIX, not the capture's own (ADR-0093
        // Decision 7) — a fold whose re-execution is another execution, read with the accused's
        // opened output tile and against the fold's own checkpoint leaves.
        match &retention {
            crate::produce::Base0RetentionV1::Dense(_) => {
                Ok((binding, self.tiles_from_material_v1(&retention)?, false, rerun.checkpoints, prompt_ids, generated))
            }
            crate::produce::Base0RetentionV1::Folded(fold) => {
                if rerun.binding.committed_execution_root == binding.committed_execution_root {
                    return Ok((binding, rerun.tiles, false, rerun.checkpoints, prompt_ids, generated));
                }
                if !accused_out_tile_carried {
                    return Err(
                        "a folded capture keeps no rows, and its re-execution is not the same execution — its bottom needs the \
                         accused's opened output tile (its root claim's)"
                            .to_string(),
                    );
                }
                let leg =
                    crate::legs::base0_checkpoint_leg_of_retention_v1(&binding, &fold.checkpoint_chunks, &fold.checkpoint_leaves)
                        .map_err(|e| format!("the fold's checkpoint leg does not rebuild: {e:?}"))?;
                Ok((binding, rerun.tiles, true, leg, prompt_ids, generated))
            }
        }
    }

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
            None => qwen25_a16_prompt_for_anchor(
                binding.job_context.job_id,
                self.artifact.shape.vocab,
                binding.job_context.declared_prefill_tokens,
            )
            .iter()
            .map(|t| *t as u32)
            .collect(),
        })
    }

    /// An honest, dense re-execution of the binding's job — what this node's own execution of it
    /// commits.
    fn attn_rerun_v1(
        &self,
        binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
        prompt_ids: &[u32],
    ) -> Result<crate::produce::Base0ExecutionV1, String> {
        let prompt: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
        // A DENSE re-execution: bounded by the materialization cap, not the network's ladder (DoS
        // audit 2026-09-24, #4) — the dense sink ignores the ladder but for this bound.
        a16_execute_in_storage_at_run_v1(
            &self.artifact,
            &binding.shape_profile,
            self.plan.as_ref(),
            &binding.job_context,
            &prompt,
            self.materialize_cap(),
            crate::legs::Base0CaptureKindV1::DenseTiles,
            &mut |_| {},
            None,
            crate::engine_a16::KV_STORAGE_SHIPPED_V1,
            self.prefill_run_positions,
        )
    }

    /// The dense tiles either retention can answer with: the ones it kept, or the ones a
    /// re-execution reproduces.
    /// **ADR-0086 Decision 2, the executor's side.** The anchor state a fold interval resumes from,
    /// recomputed with this family's recompute kernels (a plain forward pass, no capture) and
    /// memoized under the binding's context — the same state a seat recomputes for the checkpoint
    /// check. Before this the span replay started at genesis with tiles captured (5–10 s a call).
    fn fold_anchor_state_v1(
        &self,
        material: &crate::produce::Base0FpMaterialV2,
        prompt_token_ids: &[u32],
        covered: u32,
    ) -> Option<crate::fp_recompute::Base0FpSeatStateV1> {
        // DRILL ONLY (ADR-0152 §4-ter N5): a run that followed its attention lie committed the state
        // the lie fed, so its own anchors are recomputed through the same lie — never memoized
        // beside an honest walk of the same job.
        if let Some(lie) = self.drill_attn_for_v1(&material.binding.job_context) {
            let mut kernels =
                crate::fp_recompute::A16RecomputeKernelsV1::new(&self.artifact, self.plan.as_ref()).ok()?.with_drill_attn_lie_v1(lie);
            return crate::fp_recompute::base0_fp_recompute_state_at_covered_v1(
                &material.binding.shape_profile,
                &material.binding.job_context,
                prompt_token_ids,
                &material.generated_token_ids,
                covered,
                &mut kernels,
                self.prompt_ids_form,
            )
            .ok();
        }
        // The walk is kept between questions about one job (ADR-0110 §9.5): an executor answering
        // several openings, a leaf's evidence and the held units of one claim walks it once.
        crate::fp_recompute::base0_fp_a16_seat_state_v1(
            &self.seat_memo,
            &self.artifact,
            self.plan.as_ref(),
            &material.binding.shape_profile,
            &material.binding.job_context,
            prompt_token_ids,
            &material.generated_token_ids,
            covered,
            self.prompt_ids_form,
        )
        .ok()
    }

    /// **ADR-0152 §4-ter N1: the windowed RESPONDER** — the accused executor's evidence at a held
    /// fused site, out of its own retention without laying the job out.
    ///
    /// (a) the state at the anchor (`covered = p + 1`) recomputed by a plain forward, memoized per
    /// job ([`Self::fold_anchor_state_v1`]) and required to root to (b) the anchor, the fold's own
    /// retained checkpoint leaf opened in its leg; (c) the slice sub-roots from (a); (d) the out
    /// tile and the query row opened out of the fold (a block replayed from the interval's anchor,
    /// every completed block folded against the retained digest, the path from the retained
    /// nodes); (e) the inputs, layer ℓ's K and V out of (a). What it holds is the state (the walk's
    /// cache and the anchor's chunks with their leaf hashes — ≈ 1.5–2 GB at the peak at 8,192
    /// positions of the 1.5B row, the review's F8; see the trait verb's note) and a few blocks of leaf
    /// hashes, never a leaf vector. The state walk is the seat memo's, which serialises forwards. A
    /// DENSE retention (a job small enough to have kept its tiles) answers through the dense builder,
    /// with the sub-roots of the anchor it reads.
    fn attn_held_responder_v1(
        &self,
        material: &[u8],
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
    ) -> Result<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldEvidenceV1, String> {
        use kaspa_consensus_core::palw_attn_court_v1::{PalwAttnCheckpointAnchorV1, PalwAttnRowOpeningV1};
        use kaspa_consensus_core::palw_step_leg::{PalwStepOpeningV1, step_merkle_path_capped_v1, step_merkle_root_capped_v1};
        let retention =
            crate::produce::base0_material_decode_any_v1(material).map_err(|_| "the capture does not decode".to_string())?;
        let fold = match &retention {
            crate::produce::Base0RetentionV1::Folded(fold) => fold,
            crate::produce::Base0RetentionV1::Dense(_) => {
                let evidence = self.attn_site_evidence(material, narrowed, carried_prompt, None)?;
                let anchor = evidence.anchor.as_ref().ok_or("a held site's evidence carries its anchor")?;
                let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(
                    &evidence.binding.shape_profile,
                    &evidence.binding.job_context,
                    anchor.anchor.leaf.covered_decode_call,
                );
                let layout =
                    kaspa_consensus_core::palw_state_chunk_map::palw_state_layout_v4(&evidence.binding.shape_profile, positions)
                        .map_err(|e| format!("the anchor's held layout: {e:?}"))?;
                let slice_sub_roots =
                    kaspa_consensus_core::palw_state_chunk_map::palw_state_slice_sub_roots_v4(&layout, &anchor.chunk_hashes)
                        .map_err(|e| format!("the state's sub-roots: {e:?}"))?;
                return Ok(kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldEvidenceV1 { evidence, slice_sub_roots });
            }
        };
        let binding = &fold.binding;
        if binding.shape_profile.shape_profile_id() != self.class_profile_id {
            return Err("the capture is not this class's".to_string());
        }
        let prompt_ids = self.attn_prompt_ids_v1(binding, carried_prompt)?;
        let cap = self.step_ladder_cap();
        let inventory = crate::inventory::a16_inventory_v1(&self.artifact, &self.profile).map_err(|e| format!("{e:?}"))?;
        let (site, _) =
            crate::attn_held::base0_attn_held_site_v1(binding, narrowed, inventory.operands(), inventory.root(), None, cap)?;
        let covered = site.site.anchor_covered_decode_call.ok_or("the held class's site derives no anchor")?;
        // (b) The anchor: the fold's own retained leaf, opened in the leg it committed.
        let leg = crate::legs::base0_checkpoint_leg_of_retention_v1(binding, &fold.checkpoint_chunks, &fold.checkpoint_leaves)
            .map_err(|e| format!("the fold's checkpoint leg does not rebuild: {e:?}"))?;
        if step_merkle_root_capped_v1(&leg.leaf_hashes, cap).ok() != Some(binding.checkpoint_merkle_root) {
            return Err("the fold's checkpoint leaves do not root to its own binding".to_string());
        }
        let i = leg
            .leaves
            .iter()
            .position(|leaf| leaf.covered_decode_call == covered)
            .ok_or_else(|| format!("the fold commits no checkpoint at counter {covered}"))?;
        let siblings = step_merkle_path_capped_v1(&leg.leaf_hashes, i, cap).map_err(|e| format!("{e:?}"))?;
        let anchor = PalwAttnCheckpointAnchorV1 {
            leaf: leg.leaves[i].clone(),
            opening: PalwStepOpeningV1 { leaf_index: i as u64, leaf_hash: leg.leaf_hashes[i], siblings },
        };
        let (site, operand_openings) =
            crate::attn_held::base0_attn_held_site_v1(binding, narrowed, inventory.operands(), inventory.root(), Some(&anchor), cap)?;
        // (a) The state at the anchor, as this executor's run committed it.
        let state = self
            .fold_anchor_state_v1(fold, &prompt_ids, covered)
            .ok_or_else(|| format!("the state at counter {covered} does not recompute"))?;
        // (d) The two committed leaves, out of the fold.
        let query_leaf = crate::attn_held::base0_attn_held_query_leaf_v1(binding, &site)?;
        let kernels = A16IntervalKernels {
            artifact: &self.artifact,
            plan: self.plan.as_ref(),
            fault: self.drill_fault_v1(),
            storage: self.runtime_profile,
            run_positions: self.prefill_run_positions,
        };
        let mut opened = crate::fp_interval::base0_fp_leaf_openings_from_fold_v1(
            fold,
            &[narrowed, query_leaf],
            &prompt_ids,
            self.checkpoint_interval(),
            cap,
            &kernels,
            &|c| self.fold_anchor_state_v1(fold, &prompt_ids, c),
            self.prompt_ids_form,
        )
        .map_err(|e| format!("the fold's own leaves: {e}"))?
        .into_iter();
        let mut row = || opened.next().map(|(leaf, opening)| PalwAttnRowOpeningV1 { leaf, opening }).ok_or("an opening per leaf");
        let (out_tile, query) = (row()?, row()?);
        crate::attn_held::base0_attn_held_evidence_v1(
            binding,
            narrowed,
            &site,
            operand_openings,
            out_tile,
            query,
            anchor,
            state.chunks,
            None,
        )
    }

    /// **ADR-0152 §4-ter N2: the windowed CHALLENGER** — a held fused site's evidence from the
    /// accused's filing (its held root claim) and ONE honest replay of this node's own.
    ///
    /// The replay runs from genesis through the site's step, streaming every leaf before the
    /// narrowed one into [`kaspa_consensus_core::palw_step_leg::PalwStepPrefixStreamV1`] with the
    /// accused's filed path of the narrowed leaf — the query row's opening against the accused's step
    /// root, refused unless the stream roots to it (the first divergence is not this leaf) — keeping
    /// the query tile, and leaving its cache at exactly the anchor's positions: the state this node
    /// reads layer ℓ's K and V from and builds its block paths over. Nothing the size of the job is
    /// held. The anchor, the out tile and the sub-roots are the accused's, checked against the site
    /// before a leaf is replayed.
    fn attn_held_challenger_v1(
        &self,
        filing: &kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldFilingV1,
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
    ) -> Result<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldEvidenceV1, String> {
        use kaspa_consensus_core::palw_attn_court_v1::PalwAttnRowOpeningV1;
        use kaspa_consensus_core::palw_step_leg::{PalwStepPrefixStreamV1, step_tile_leaf_hash_ctx_v1};
        let binding = &filing.binding;
        if binding.shape_profile.shape_profile_id() != self.class_profile_id {
            return Err("the filing is not this class's".to_string());
        }
        let (profile, ctx) = (&binding.shape_profile, &binding.job_context);
        let prompt_ids = self.attn_prompt_ids_v1(binding, carried_prompt)?;
        let cap = self.step_ladder_cap();
        let step_leaf_count = crate::fp_interval::base0_fp_binding_step_space_v1(binding, cap).map_err(|e| e.to_string())?;
        let inventory = crate::inventory::a16_inventory_v1(&self.artifact, &self.profile).map_err(|e| format!("{e:?}"))?;
        let (site, operand_openings) = crate::attn_held::base0_attn_held_site_v1(
            binding,
            narrowed,
            inventory.operands(),
            inventory.root(),
            Some(&filing.anchor),
            cap,
        )?;
        crate::attn_held::base0_attn_held_filing_is_the_sites_v1(filing, &site, narrowed)?;
        let query_leaf = crate::attn_held::base0_attn_held_query_leaf_v1(binding, &site)?;
        let step = crate::attn_held::base0_attn_held_site_step_v1(binding, &site);
        if u64::from(site.site.anchor_positions) != step {
            return Err(format!("the anchor covers {} positions and the site's step is {step}", site.site.anchor_positions));
        }
        let mut stream = PalwStepPrefixStreamV1::new_capped_v1(step_leaf_count, &filing.out_tile.opening, &[query_leaf], cap)
            .map_err(|e| format!("the accused's path of the disputed leaf: {e}"))?;
        // ONE honest replay, genesis through the site's step; its cache is this node's state at the
        // anchor when it returns.
        let prompt: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
        let row_elements = profile.attn_kv_heads as usize * profile.attn_head_dim as usize;
        let mut cache = A16Cache::with_storage(self.artifact.shape.n_layers, self.runtime_profile);
        cache.reserve_positions(step as usize, row_elements);
        let engine = A16Engine::new(&self.artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
        let mut replay = A16ReplayEngineV1 {
            engine,
            cache,
            plan: self.plan.as_ref(),
            vocab: self.artifact.shape.vocab,
            run: self.prefill_run_positions,
        };
        let (_ctx_hash, profile_hash) = (ctx.context_hash(), profile.shape_profile_id());
        let mut query_tile = None;
        let mut pushed: Result<(), String> = Ok(());
        crate::fp_interval::base0_fp_replay_interval_with_v1(
            profile,
            ctx,
            &crate::fp_interval::Base0FpIntervalStartV1::Genesis { prompt_tokens: &prompt },
            crate::fp_interval::Base0FpWindowV1 { first_step: 1, last_step: step },
            step_leaf_count,
            &mut replay,
            &mut |index, tile| {
                if index < narrowed && pushed.is_ok() {
                    pushed = stream.push(step_tile_leaf_hash_ctx_v1(ctx, &profile_hash, &tile)).map_err(|e| e.to_string());
                    if index == query_leaf {
                        query_tile = Some(tile);
                    }
                }
                Ok(())
            },
        )?;
        pushed?;
        let (root, mut openings) = stream.finish().map_err(|e| format!("the honest rows before the disputed leaf: {e}"))?;
        if root != binding.step_merkle_root {
            return Err(
                "the honest rows before the disputed leaf and the accused's opened tile do not root to the accused's binding — \
                 the first divergence is not this leaf"
                    .to_string(),
            );
        }
        let query = PalwAttnRowOpeningV1 {
            leaf: query_tile.ok_or("the replay produced no query row")?,
            opening: openings.pop().ok_or("the stream opened no query row")?,
        };
        // The state at the anchor, out of the replay's own cache, under the class's map.
        let geometry =
            crate::legs::base0_state_chunk_geometry_v1(profile, site.site.anchor_positions).map_err(|e| format!("{e:?}"))?;
        let chunks = (0..geometry.chunk_count())
            .map(|index| {
                kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_entry_v1(&geometry, index)
                    .and_then(|entry| replay.cache.state_chunk_bytes_v1(&entry))
                    .ok_or_else(|| format!("the replay's cache holds no chunk {index} of the anchor's state"))
            })
            .collect::<Result<Vec<Vec<u8>>, String>>()?;
        crate::attn_held::base0_attn_held_evidence_v1(
            binding,
            narrowed,
            &site,
            operand_openings,
            filing.out_tile.clone(),
            query,
            filing.anchor.clone(),
            chunks,
            Some(filing.slice_sub_roots.clone()),
        )
    }

    /// **ADR-0152 §4-ter.3 step 6** — the trait verb's body: the cache-write node of `kind` at layer
    /// `attn_layer`, every tile of it at history position `history_position` (a prefill position `p`
    /// is `(call 0, p)`, a decode position the head of its call), each leaf before the narrowed one,
    /// streamed from ONE honest replay to the site's step with the accused's filed path of the
    /// narrowed leaf — refused unless the stream roots to the accused's step root (the rows are then
    /// its committed ones).
    fn attn_held_cache_write_rows_of_v1(
        &self,
        filing: &kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldFilingV1,
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
        kind: kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1,
        attn_layer: u16,
        history_position: u32,
    ) -> Result<Vec<kaspa_consensus_core::palw_attn_court_v1::PalwAttnRowOpeningV1>, String> {
        use kaspa_consensus_core::palw_attn_court_v1::PalwAttnRowOpeningV1;
        use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepNodeRoleV1, PalwStepOutLenV1, PalwStepTableV1};
        use kaspa_consensus_core::palw_step_leg::{PalwStepPrefixStreamV1, step_tile_leaf_hash_ctx_v1};
        if !self.court_capable {
            return Err("the v1 class carries no capture to read a cache row out of".to_string());
        }
        let binding = &filing.binding;
        if binding.shape_profile.shape_profile_id() != self.class_profile_id {
            return Err("the filing is not this class's".to_string());
        }
        let (profile, ctx) = (&binding.shape_profile, &binding.job_context);
        let prompt_ids = self.attn_prompt_ids_v1(binding, carried_prompt)?;
        let cap = self.step_ladder_cap();
        let step_leaf_count = crate::fp_interval::base0_fp_binding_step_space_v1(binding, cap).map_err(|e| e.to_string())?;
        let inventory = crate::inventory::a16_inventory_v1(&self.artifact, &self.profile).map_err(|e| format!("{e:?}"))?;
        let (site, _) = crate::attn_held::base0_attn_held_site_v1(
            binding,
            narrowed,
            inventory.operands(),
            inventory.root(),
            Some(&filing.anchor),
            cap,
        )?;
        crate::attn_held::base0_attn_held_filing_is_the_sites_v1(filing, &site, narrowed)?;
        let role = match kind {
            kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1::Key => PalwStepNodeRoleV1::KCacheWrite,
            kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1::Value => PalwStepNodeRoleV1::VCacheWrite,
        };
        let index = profile.attn_nodes.iter().position(|n| n.role == role).ok_or("the class declares no such cache write")?;
        let node = &profile.attn_nodes[index];
        let PalwStepOutLenV1::Fixed { elements } = node.out_len else {
            return Err("a cache write's row is not a fixed width".to_string());
        };
        let tiles = elements.div_ceil(node.tile_len.max(1));
        let slot = profile.global_node_slot(PalwStepTableV1::Attn, attn_layer, index).ok_or("the layer has no such cache write")?;
        let prefill = ctx.declared_prefill_tokens;
        let (call_index, position) =
            if history_position < prefill { (0, history_position) } else { (history_position - prefill + 1, 0) };
        let wanted: Vec<u64> = (0..tiles)
            .map(|tile_index| {
                kaspa_consensus_core::palw_step::canonical_step_leaf_index(
                    profile,
                    ctx,
                    &PalwStepCoordinateV1 { call_index, node_slot: slot, position, tile_index },
                )
                .ok_or_else(|| format!("history position {history_position} has no cache-write leaf in this job"))
            })
            .collect::<Result<_, _>>()?;
        if wanted.iter().any(|leaf| *leaf >= narrowed) {
            return Err(format!("the cache-write row at {history_position} is not before the narrowed leaf {narrowed}"));
        }
        let step = crate::attn_held::base0_attn_held_site_step_v1(binding, &site);
        let mut stream = PalwStepPrefixStreamV1::new_capped_v1(step_leaf_count, &filing.out_tile.opening, &wanted, cap)
            .map_err(|e| format!("the accused's path of the disputed leaf: {e}"))?;
        let prompt: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
        let row_elements = profile.attn_kv_heads as usize * profile.attn_head_dim as usize;
        let mut cache = A16Cache::with_storage(self.artifact.shape.n_layers, self.runtime_profile);
        cache.reserve_positions(step as usize, row_elements);
        let engine = A16Engine::new(&self.artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
        let mut replay = A16ReplayEngineV1 {
            engine,
            cache,
            plan: self.plan.as_ref(),
            vocab: self.artifact.shape.vocab,
            run: self.prefill_run_positions,
        };
        let (_ctx_hash, profile_hash) = (ctx.context_hash(), profile.shape_profile_id());
        let mut kept: Vec<(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1)> = Vec::new();
        let mut pushed: Result<(), String> = Ok(());
        crate::fp_interval::base0_fp_replay_interval_with_v1(
            profile,
            ctx,
            &crate::fp_interval::Base0FpIntervalStartV1::Genesis { prompt_tokens: &prompt },
            crate::fp_interval::Base0FpWindowV1 { first_step: 1, last_step: step },
            step_leaf_count,
            &mut replay,
            &mut |index, tile| {
                if index < narrowed && pushed.is_ok() {
                    pushed = stream.push(step_tile_leaf_hash_ctx_v1(ctx, &profile_hash, &tile)).map_err(|e| e.to_string());
                    if wanted.contains(&index) {
                        kept.push((index, tile));
                    }
                }
                Ok(())
            },
        )?;
        pushed?;
        let (root, openings) = stream.finish().map_err(|e| format!("the honest rows before the disputed leaf: {e}"))?;
        if root != binding.step_merkle_root {
            return Err("the honest rows before the disputed leaf do not root to the accused's binding".to_string());
        }
        kept.sort_by_key(|(index, _)| *index);
        if kept.len() != wanted.len() || openings.len() != wanted.len() {
            return Err("the replay did not produce every tile of the cache-write row".to_string());
        }
        // `wanted` is in tile order, which is leaf order; the stream answers in the order it was asked.
        Ok(kept.into_iter().zip(openings).map(|((_, leaf), opening)| PalwAttnRowOpeningV1 { leaf, opening }).collect())
    }

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
        let prepared = self.prepare_whole_capture_v1(material, carried, Some(index))?;
        self.refutation_from_prepared_v1(&prepared, index)
    }

    /// **Everything the whole-capture prover reads that does not depend on the leaf**, built once:
    /// the decode, the materialization refusal, the carried (or derived) prompt, the dense tiles —
    /// a re-execution for a fold — and the decode-token pin. [`Self::refutation_with_prompt`] is
    /// this plus one [`Self::refutation_from_prepared_v1`]; the free-prompt leaf prover is this plus
    /// as many as the seat draws (DoS audit 2026-09-24, #4: one re-execution per sampled claim, where
    /// it was one per draw).
    ///
    /// `probe`, when given, is checked for a main-step coordinate BEFORE the tiles are built, so a
    /// single-leaf question about an aux leaf is refused without a re-execution, as it always was.
    /// The prompt is checked before the tiles for the same reason: a list that is not the binding's
    /// is refused for free rather than after a whole job's forward pass.
    fn prepare_whole_capture_v1(
        &self,
        material: &[u8],
        carried: Option<&[u32]>,
        probe: Option<u64>,
    ) -> Result<A16PreparedCaptureV1, String> {
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

        // **The prompt: re-derived for an attempt, carried for a free prompt** (ADR-0073 Decision
        // 1c). An attempt's `job_id` IS its anchor and its prompt is a pure function of it, so it
        // is re-derived; a context whose prompt hash does not match the derivation and arrives
        // with NO carried ids gets an empty prompt — the court's hash guard would refuse a wrong
        // one, and an absent one only narrows which leaves adjudicate. A free prompt is the user's
        // and derives from nothing: the caller hands it over, and one that is not the binding's
        // own is refused here rather than read by the court as no verdict.
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
                let derived = qwen25_a16_prompt_for_anchor(
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

        // **This class's own pin: the tiled scheme's.** The generated ids are bound through the
        // rows-tree root — carrying the rows themselves is exactly what the tiled scheme exists to
        // avoid at this vocabulary.
        let rows_root =
            kaspa_consensus_core::palw_step_refute::tiled_logits_rows_root_v1(&binding.job_context, retention.logits_rows())
                .ok_or_else(|| "the retained rows build no tree".to_string())?;
        let pin = kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1::TiledV1(
            kaspa_consensus_core::palw_step_refute::PalwTiledDecodeTokensV1 {
                rows_root,
                generated_token_ids: retention.generated_token_ids().to_vec(),
            },
        );
        let checkpoint_chunks = retention.checkpoint_chunks().to_vec();
        Ok(A16PreparedCaptureV1 { binding, step_tiles, pin, prompt_token_ids, checkpoint_chunks })
    }

    /// One leaf's refutation out of a prepared capture — the per-leaf half of the prover.
    fn refutation_from_prepared_v1(
        &self,
        prepared: &A16PreparedCaptureV1,
        index: u64,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        let binding = &prepared.binding;
        let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, index)
            .ok_or_else(|| format!("leaf {index} is not a main step coordinate"))?;

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
                &prepared.checkpoint_chunks,
            )
            .ok()
            .and_then(|checkpoints| crate::legs::base0_kv_anchor_for_call_v1(&checkpoints, coord.call_index))
        } else {
            None
        };

        crate::legs::base0_refutation_from_capture_capped_v1(
            &binding.shape_profile,
            &binding.job_context,
            &prepared.step_tiles,
            binding.clone(),
            coord,
            prepared.prompt_token_ids.clone(),
            Some(prepared.pin.clone()),
            kv_checkpoint,
            self.network_ladder,
        )
        .map_err(|e| format!("{e:?}"))
    }
}

#[cfg(test)]
thread_local! {
    /// Dense re-executions of a fold on this thread — the unit the DoS audit's #4 CPU half counts.
    static FOLD_REEXECUTIONS_V1: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// **A whole capture, decoded and laid out once** — what the dense tier's prover reads at every
/// leaf ([`Qwen25A16Backend::prepare_whole_capture_v1`]). Holds the dense tiles: its life is the
/// memory a seat reserved for the capture.
struct A16PreparedCaptureV1 {
    binding: kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    step_tiles: crate::legs::Base0StepTilesV1,
    pin: kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1,
    prompt_token_ids: Vec<u32>,
    checkpoint_chunks: Vec<Vec<Vec<u8>>>,
}

/// The dense tier's free-prompt leaf prover: one prepared capture, many leaves.
struct A16CaptureLeafProverV1<'a> {
    backend: &'a Qwen25A16Backend,
    prepared: A16PreparedCaptureV1,
}

impl kaspa_consensus_core::palw_backend::PalwCaptureLeafProverV1 for A16CaptureLeafProverV1<'_> {
    fn refutation_for_index(
        &self,
        index: u64,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        self.backend.refutation_from_prepared_v1(&self.prepared, index)
    }
}

/// **The dense tier's kernels, as a seat's interval replay needs them** (ADR-0077 Decision 8).
///
/// The window is walked by [`crate::fp_interval::base0_fp_replay_interval_v1`] — the capture's own
/// loop — and this supplies only what the family owns: restoring the cache the interval resumes
/// from and running one forward call, through the SAME plan-or-traced dispatch the capture uses.
/// A replay that took the other arm would recompute rows the capture never committed.
/// The dense tier's interval kernels, for tests in sibling modules that need to replay an interval
/// the way this backend does. Not a second implementation — the same struct the backend uses.
#[cfg(test)]
pub(crate) fn a16_interval_kernels_for_tests_v1<'a>(
    artifact: &'a Base0ArtifactV1,
    plan: Option<&'a crate::engine_a16::A16ProfilePlanV1>,
) -> impl crate::fp_interval::Base0FpIntervalKernelsV1 + 'a {
    A16IntervalKernels {
        artifact,
        plan,
        fault: None,
        storage: crate::engine_a16::KV_STORAGE_SHIPPED_V1,
        run_positions: A16_PREFILL_RUN_POSITIONS,
    }
}

struct A16IntervalKernels<'a> {
    artifact: &'a Base0ArtifactV1,
    plan: Option<&'a crate::engine_a16::A16ProfilePlanV1>,
    /// DRILL ONLY: the job context and the lie this replay makes where the drill's capture made it
    /// (`Qwen25A16Backend::drill_fault`) — a committed tile moved, or the engine's attention output
    /// moved and followed; `None` everywhere but a drill.
    fault: Option<(Hash64, A16DrillLieV1)>,
    /// The representation the replay's cache is held in — the seat's own choice, whatever the
    /// producer's was: the chunks it decodes are the map's `i32` rows either way.
    storage: crate::engine_a16::KvStorageProfileV1,
    /// The prefill run width the replay walks at (int-10.2 A2): the backend's own.
    run_positions: usize,
}

impl crate::fp_interval::Base0FpIntervalKernelsV1 for A16IntervalKernels<'_> {
    fn replay_interval_into(
        &self,
        profile: &PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        start: &crate::fp_interval::Base0FpIntervalStartV1<'_>,
        window: crate::fp_interval::Base0FpWindowV1,
        step_leaf_count: u64,
        sink: &mut dyn FnMut(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1) -> Result<(), String>,
    ) -> Result<(), String> {
        let attn = match self.fault {
            Some((context, A16DrillLieV1::Tile { leaf, lane, delta })) if context == ctx.context_hash() => {
                // The drill's lie, replayed where the capture made it: the tile's lane moved, as
                // `A16DrillFaultSiteV1::apply` moved the committed row's.
                let honest = Self { fault: None, ..*self };
                return honest.replay_interval_into(profile, ctx, start, window, step_leaf_count, &mut |at, mut tile| {
                    if at == leaf {
                        let tile_len = profile
                            .resolve_node_slot(tile.coord.node_slot)
                            .map(|(node, _)| node.tile_len.max(1) as usize)
                            .unwrap_or(1);
                        let offset = lane.map(|l| l.saturating_sub(tile.coord.tile_index as usize * tile_len)).unwrap_or(0);
                        if let Some(bytes) = tile.values_le.get_mut(4 * offset..4 * offset + 4) {
                            let value = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                            let moved = a16_drill_moved_v1(value, delta, lane.is_some());
                            bytes.copy_from_slice(&moved.to_le_bytes());
                        }
                    }
                    sink(at, tile)
                });
            }
            // ADR-0152 §4-ter N5 (`follow = true`): the engine lies and follows, as the run's did.
            Some((context, A16DrillLieV1::Attn(lie))) if context == ctx.context_hash() => Some(lie),
            _ => None,
        };
        let engine = A16Engine::new(self.artifact)
            .map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?
            .with_drill_attn_lie_v1(attn);
        let layers = self.artifact.shape.n_layers;
        let row_elements = profile.attn_kv_heads as usize * profile.attn_head_dim as usize;
        let mut cache = match start {
            crate::fp_interval::Base0FpIntervalStartV1::Genesis { .. } => A16Cache::with_storage(layers, self.storage),
            crate::fp_interval::Base0FpIntervalStartV1::Checkpoint { covered_decode_call, chunks, .. } => {
                let geometry =
                    crate::legs::base0_checkpoint_geometry_at_v1(profile, ctx, *covered_decode_call).map_err(|e| format!("{e:?}"))?;
                A16Cache::from_state_chunks_v1(self.storage, layers, row_elements, &geometry, chunks).map_err(|e| format!("{e:?}"))?
            }
        };
        // The window's rows, reserved once (an upper bound: virtual until touched).
        cache.reserve_positions(ctx.declared_prefill_tokens as usize + ctx.exact_decode_tokens as usize, row_elements);
        let mut replay =
            A16ReplayEngineV1 { engine, cache, plan: self.plan, vocab: self.artifact.shape.vocab, run: self.run_positions };
        crate::fp_interval::base0_fp_replay_interval_with_v1(profile, ctx, start, window, step_leaf_count, &mut replay, sink)
    }
}

/// **The dense tier's engine as a replay drives it** — a step through the plan (or the v2 reference
/// program), and, where the plan allows it, a run of prefill positions a layer at a time
/// (`A16Engine::forward_prefill_planned`, ADR-0117 Decision 2): a seat replaying a held interval of
/// prompt positions, or a shipped class's interval 0 (the whole prefill), reads each layer's
/// weights once a run of the backend's width ([`A16_PREFILL_RUN_POSITIONS`] unless narrowed), and
/// commits the same rows.
struct A16ReplayEngineV1<'a> {
    engine: A16Engine<'a>,
    cache: A16Cache,
    plan: Option<&'a crate::engine_a16::A16ProfilePlanV1>,
    vocab: usize,
    /// The prefill run width (int-10.2 A2): the backend's own, from its interval kernels.
    run: usize,
}

impl crate::fp_interval::Base0FpReplayForwardV1 for A16ReplayEngineV1<'_> {
    fn step(&mut self, token: usize, position: usize) -> Result<(Vec<i32>, Vec<crate::legs::Base0CapturedRowV1>), String> {
        if token >= self.vocab {
            return Err(format!("token {token} is outside this class's vocabulary of {}", self.vocab));
        }
        let (logits, trace) = match self.plan {
            Some(plan) => self
                .engine
                .forward_token_planned(plan, &mut self.cache, token, position)
                .map_err(|e| format!("planned forward: {e:?}"))?,
            None => self.engine.forward_token_traced(&mut self.cache, token, position).map_err(|e| format!("forward: {e:?}"))?,
        };
        Ok((logits, crate::legs::a16_captured_rows_v1(&trace)))
    }

    fn run_positions(&self) -> usize {
        match self.plan {
            Some(plan) if plan.one_pass_prefill_supported() => self.run.max(1),
            _ => 1,
        }
    }

    fn run(
        &mut self,
        tokens: &[usize],
        first_position: usize,
        with_post: bool,
    ) -> Result<(Vec<i32>, Vec<Vec<crate::legs::Base0CapturedRowV1>>), String> {
        let plan = self.plan.ok_or_else(|| "a run needs the registered plan".to_string())?;
        if let Some(bad) = tokens.iter().find(|t| **t >= self.vocab) {
            return Err(format!("token {bad} is outside this class's vocabulary of {}", self.vocab));
        }
        let (logits, traces) = self
            .engine
            .forward_prefill_planned(plan, &mut self.cache, tokens, first_position, with_post)
            .map_err(|e| format!("the prefill run at {first_position}: {e:?}"))?;
        Ok((logits, traces.iter().map(crate::legs::a16_captured_rows_v1).collect()))
    }
}

impl Qwen25A16Backend {
    /// The cadence this family checkpoints at — a class fact from the family's registration, never
    /// read off a capture.
    fn checkpoint_interval(&self) -> u32 {
        kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1
    }

    /// **ADR-0077 SA-6, at the job boundary — and what this tier's artifact actually is.**
    ///
    /// On a POSIX node it is a MAPPING, not owned memory: `DenseLineageV1::load` maps the `.palwart`
    /// read-only and every int8 slab is an `Int8SlabV1::Mapped` view of the file's page-cache pages
    /// (measured 2026-10-01: 1,694.6 of the 8k artifact's 1,716.0 MiB; the a16 store, 17.35 MiB, the
    /// rotary table and the small tables are the owned part). Those pages are shared by every process
    /// on the host that maps the file — one copy, `Shared_Clean` in each seat's `smaps` — and they CAN
    /// be evicted under pressure and refaulted under a job, 4 KiB a fault at 6–11 MB/s on the fleet's
    /// virtio disks: what the five 5.104 seats spent their 7–50-minute replays on. A node pins them
    /// when its policy allows (kaspad `palw_artifact_pin`, int-10.2): then no page of the weights
    /// faults under a job. The owned decode (`decode_artifact_file_v1`) is only what a platform that
    /// cannot map gets.
    ///
    /// The probe still answers `Ok`, and that is a known gap rather than a guarantee: a mapped page
    /// that refaults is slow, never wrong, but a file TRUNCATED in place under the mapping would fault
    /// past its end, and this tier does not yet walk its extents against the file's length the way
    /// the hybrid does (`Qwen36Backend::artifact_read_probe_v1`). A replacement by rename leaves the
    /// mapped inode untouched; an in-place rewrite of a held artifact is what this does not catch.
    fn artifact_read_probe_v1(&self) -> Result<(), String> {
        Ok(())
    }

    /// The dense tier's interval kernels as this backend replays with them — its plan, its drill
    /// fault (none off a drill) and its cache representation.
    fn interval_kernels_v1(&self) -> A16IntervalKernels<'_> {
        A16IntervalKernels {
            artifact: &self.artifact,
            plan: self.plan.as_ref(),
            fault: self.drill_fault_v1(),
            storage: self.runtime_profile,
            run_positions: self.prefill_run_positions,
        }
    }
}

impl PalwExecutionBackendV1 for Qwen25A16Backend {
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
        // **CoreV1 (ADR-0152 v3.1 J-5): the chain's job, over this class's profile** — the formula
        // canonical, the core prompt loop, the artifact-held fields zero. The registration's
        // canonical must be the formula (admission pins it past the fence); one that is not names a
        // job the chain would call another's, and is refused here rather than produced.
        if self.attempt_rules == kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1::CoreV1 {
            let formula = kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_canonical_v1(&self.profile, false);
            if formula != Some(self.canonical_job) {
                return Err(format!(
                    "the class's canonical job {:?} is not CoreV1's formula {formula:?}: the chain would convict it as another job",
                    self.canonical_job
                ));
            }
            return kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_job_for_anchor_v1(
                &self.profile,
                &anchor,
                self.canonical_job,
                self.prompt_ids_form,
            )
            .ok_or_else(|| "the canonical prompt does not commit under the class's form".to_string());
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
        if !self.court_capable {
            let outcome = self.execute(job, prompt)?;
            return Ok(PalwReplayRootsV1 {
                execution_root: outcome.execution_root,
                trace_root: outcome.trace_root,
                work_leaves: None,
                output_root: Some(outcome.output_root),
            });
        }
        // The fold sink: one execution, the dense run's roots, none of its tiles (ADR-0084 D7).
        let run = a16_execute_in_storage_at_run_v1(
            &self.artifact,
            &self.profile,
            self.plan.as_ref(),
            job,
            prompt,
            self.step_ladder_cap(),
            crate::legs::Base0CaptureKindV1::Fold,
            &mut |_| {},
            None,
            self.runtime_profile,
            self.prefill_run_positions,
        )?;
        // SEAT-S2: the output root the producer's `execute` puts in the claim — the same run
        // struct's, computed by the same executor over the ids this replay generated.
        Ok(PalwReplayRootsV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            work_leaves: Some(run.binding.step_leaf_count),
            output_root: Some(self.committed_output_root_v1(&run.binding.job_context, &run.generated_token_ids, run.output_root)),
        })
    }

    /// SEAT-S4: the authenticated `SC02` opening at this class's ladder — a dense capture proves the
    /// segment exactly, a held attempt's fold proves its span at the retained level.
    fn open_segment_checkpoint_v1(&self, capture: &[u8], seat_count: u16, segment_index: u16) -> Result<Vec<u8>, String> {
        // RFC-0001 §A: the executor replays its own committed ids (a V4 answer is not re-selected).
        crate::fp_interval::base0_fp_executor_replay_v1(capture, || -> Result<Vec<u8>, String> {
            crate::segment_opening::base0_open_segment_checkpoint_capped_v2(
                capture,
                seat_count,
                segment_index,
                Some(&self.profile),
                self.step_ladder_cap(),
            )
            .map_err(|e| e.to_string())
        })
    }

    /// SEAT-S4: authenticated against `claim` and this seat's `job`, then the window replayed in STEPS
    /// under the class's cadence — a genesis segment from step 1 to its own end, a resumed one from the
    /// step after its checkpoint's rows (`palw_checkpoint_positions_at_v1`), which is the resource
    /// profile's `(resume_rows, end_rows)` (`a_partial_seat_replays_its_segments_prefix_and_resumes_at_its_start`).
    fn replay_segment_from_checkpoint_v1(
        &self,
        job: &PalwJobContextV2,
        prompt: &[usize],
        opening: &[u8],
        claim: kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentClaimV1,
    ) -> Result<kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentReplayV1, String> {
        crate::segment_opening::base0_replay_segment_opening_v2(
            &self.interval_kernels_v1(),
            &self.profile,
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
        crate::segment_opening::base0_replay_capture_segment_v2(
            &self.interval_kernels_v1(),
            &self.profile,
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
        let (call_index, position) = if site.position < prefill { (0u32, site.position) } else { (site.position - prefill + 1, 0u32) };
        let coord = kaspa_consensus_core::palw_step::PalwStepCoordinateV1 { call_index, node_slot, position, tile_index: 0 };
        let leaf = kaspa_consensus_core::palw_step::canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &coord)
            .ok_or_else(|| "the sampled site is not a main step of this job".to_string())?;
        let seats = seat_count.max(1);
        let k = kaspa_consensus_core::palw_verification_v2::palw_segment_count_v2(seats);
        let index =
            kaspa_consensus_core::palw_verification_v2::palw_segment_index_of_leaf_v2(binding.step_leaf_count, k, leaf).unwrap_or(0);
        // The segment holding the site, from the capture's own authenticated opening (SEAT-S4): the
        // site's leaf is inside the proven range, so a match roots it to the capture's binding.
        let replay = crate::segment_opening::base0_replay_capture_segment_v2(
            &self.interval_kernels_v1(),
            &self.profile,
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
        // **The captured attempt, for the class that can carry one.** Its `execution_root` is the
        // step binding's own commitment — the value `check_execution_root_binding` compares a
        // close against — and its material is the family codec's, which is what a rung or a
        // refutation is later assembled from. The v1 class (one-byte map over an `i32` cache)
        // cannot capture, so it keeps the legacy composite exactly as registered.
        if self.court_capable {
            // **A held class's attempt FOLDS** (ADR-0151 follow-up; the 2026-09-23 kills). The
            // dense sink keeps every tile of every position and a leaf-hash vector of the whole step
            // space — on the 2M attempt 1.70 TB of address space (`dmesg`: `total-vm:1702360980kB`)
            // plus ~10 MB of tiles a position, 9 GB a minute — and no host holds it, whatever the
            // K/V width. The fold is the sink the free-prompt lane, the verdict replay and every seat
            // already use for this class; its roots are the dense sink's by construction
            // (`Base0CaptureSinkV1::finish`), and its material is the v2 codec the court verbs read
            // (`base0_material_decode_any_v1`), with the prompt the court would derive from the anchor
            // anyway. The rule is a class fact (`palw_attempt_capture_folds_v1`), so the producer, the
            // seat that prices it and the court cannot disagree about what an attempt retains.
            let cap = self.network_ladder;
            let folds = kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1(&self.profile);
            let kind = if folds { crate::legs::Base0CaptureKindV1::Fold } else { crate::legs::Base0CaptureKindV1::DenseTiles };
            let run = a16_execute_in_storage_at_run_v1(
                &self.artifact,
                &self.profile,
                self.plan.as_ref(),
                job,
                prompt,
                if folds { self.step_ladder_cap() } else { cap },
                kind,
                &mut |_| {},
                None,
                self.runtime_profile,
                self.prefill_run_positions,
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
        self.execute_free_prompt_v1(job, prompt_tokens, on_token, None)
    }

    /// **RFC-0001 §2.6: the answer with no commitment, resuming from the prefix cache.** The same
    /// engine, plan and decoder as the committed run, so the ids are the ones a committed job with
    /// these fields selects; nothing is captured, which is what makes a cached prefix usable (the
    /// fold would need the prefix's tiles). Fills the cache with the prompt's state after the
    /// prefill, and again with the whole conversation (prompt + answer) when the run completes.
    fn answer_free_prompt_v1(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        stop_ids: &[u32],
        on_token: &mut dyn FnMut(u32),
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpAnswerV1, String> {
        let engine = A16Engine::new(&self.artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
        let plan = self.plan.as_ref();
        let (mut kv, reused, mut logits) = self.answer_prefill_v1(&engine, job, prompt_tokens)?;
        let prompt_ids: Vec<u32> = prompt_tokens.iter().map(|t| *t as u32).collect();
        let prefill = prompt_tokens.len();
        let limit = job.decode_token_limit as usize;
        let mut decoder = job.decoder_v1();
        let mut generated: Vec<u32> = Vec::new();
        let mut ended_on_stop_id = false;
        loop {
            let next = decoder.select(&logits);
            if decoder.generated().len() == generated.len() {
                // The decoder committed nothing from this row: no lane was admissible.
                break;
            }
            generated.push(next);
            on_token(next);
            crate::cancel::check_abort_v1()?;
            if stop_ids.contains(&next) {
                ended_on_stop_id = true;
                break;
            }
            if decoder.stop().is_some() || generated.len() >= limit {
                break;
            }
            let position = prefill + generated.len() - 1;
            logits = match plan {
                Some(plan) => {
                    engine
                        .forward_token_planned(plan, &mut kv, next as usize, position)
                        .map_err(|e| format!("decode at {position}: {e:?}"))?
                        .0
                }
                None => {
                    engine
                        .forward_token_traced(&mut kv, next as usize, position)
                        .map_err(|e| format!("decode at {position}: {e:?}"))?
                        .0
                }
            };
        }
        self.remember_conversation_v1(job, &prompt_ids, &generated, &kv);
        Ok(kaspa_consensus_core::palw_backend::PalwFpAnswerV1 {
            output_token_ids: generated,
            cached_prefix_tokens: reused as u32,
            ended_on_stop_id,
        })
    }

    /// **RFC-0001 §2.6 stage 2: the prefix state of `prefix_ids`** — the K/V state after the prefix, digested (BLAKE2b-512
    /// over every layer's keys then values as little-endian `i32`s, in layer order) and named under the class and the
    /// prefix's length ([`kaspa_consensus_core::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1`]). Taken from the prefix
    /// cache when it holds exactly this prefix, by a fresh prefill otherwise — and the two agree bit for bit, which the
    /// golden test holds.
    fn prefix_state_v1(
        &self,
        class_id: Hash64,
        tokenizer_id: Hash64,
        prefix_ids: &[u32],
    ) -> Result<kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1, String> {
        if prefix_ids.is_empty() {
            return Err("a prefix of no ids has no state".to_string());
        }
        let vocab = self.artifact.shape.vocab;
        if let Some(bad) = prefix_ids.iter().find(|t| **t as usize >= vocab) {
            return Err(format!("token {bad} is outside this class's vocabulary of {vocab}"));
        }
        let engine = A16Engine::new(&self.artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
        let tokens: Vec<usize> = prefix_ids.iter().map(|t| *t as usize).collect();
        // The cache holds a snapshot of exactly this prefix when an earlier run ended its prefill here; otherwise walk it.
        let held = self.prefix_cache.as_ref().and_then(|lock| {
            let mut cache = lock.lock().unwrap_or_else(|e| e.into_inner());
            // `lookup_v1` leaves one token to run, so ask with one more id than the prefix.
            let mut probe = prefix_ids.to_vec();
            probe.push(0);
            cache
                .lookup_v1(class_id, tokenizer_id, &probe)
                .filter(|hit| hit.positions as usize == prefix_ids.len())
                .map(|hit| hit.cache)
        });
        let cache = match held {
            Some(cache) => cache,
            None => {
                let mut cache = A16Cache::with_storage(self.artifact.shape.n_layers, self.runtime_profile);
                cache.reserve_positions(tokens.len(), self.artifact.shape.kv_dim());
                self.answer_prefill_run_v1(&engine, &mut cache, &tokens, 0)?;
                cache
            }
        };
        let (keys, values) = cache.contents_as_i32();
        let mut state = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/qwen25-a16/prefix-kv-digest/v1").to_state();
        state.update(&(keys.len() as u64).to_le_bytes());
        for (k, v) in keys.iter().zip(values.iter()) {
            for x in k.iter().chain(v.iter()) {
                state.update(&x.to_le_bytes());
            }
        }
        let mut digest = [0u8; 64];
        digest.copy_from_slice(state.finalize().as_bytes());
        let prefix_tokens = u32::try_from(prefix_ids.len()).map_err(|_| "a prefix past u32".to_string())?;
        Ok(kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1 {
            state_root: kaspa_consensus_core::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(
                &class_id,
                prefix_tokens,
                &Hash64::from_bytes(digest),
            ),
            prefix_tokens,
            class_id,
        })
    }

    /// **RFC-0001 §2.8: the pooled final-layer hidden state of a prompt.** The residual stream after
    /// the last layer, per position (the layer table's last row — the same rows the committed walk
    /// feeds the post table), pooled by `pool`. **Mean** pooling needs every position's row, so it
    /// walks the whole prompt; **last-token** pooling needs only the final position's, so it resumes
    /// from the KV prefix cache like an answer does and fills it for the next request.
    fn embed_prompt_v1(
        &self,
        prompt_tokens: &[usize],
        class_id: Hash64,
        tokenizer_id: Hash64,
        pool: kaspa_consensus_core::palw_embedding_pool_v1::PalwEmbeddingPoolV1,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpEmbeddingV1, String> {
        use kaspa_consensus_core::palw_embedding_pool_v1::{PalwEmbeddingPoolV1, palw_embedding_pool_v1};
        self.artifact_read_probe_v1()?;
        if prompt_tokens.is_empty() {
            return Err("a prompt of no tokens has no embedding".to_string());
        }
        let vocab = self.artifact.shape.vocab;
        if let Some(bad) = prompt_tokens.iter().find(|t| **t >= vocab) {
            return Err(format!("token {bad} is outside this class's vocabulary of {vocab}"));
        }
        let plan =
            self.plan.as_ref().filter(|plan| plan.one_pass_prefill_supported()).ok_or("this class's plan serves no embedding path")?;
        let engine = A16Engine::new(&self.artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
        let ids: Vec<u32> = prompt_tokens.iter().map(|t| *t as u32).collect();
        let last_layer = self.artifact.shape.n_layers.checked_sub(1).ok_or("a class with no layers")?;
        // Mean pooling walks everything; last-token resumes from the cache when it can.
        let (mut kv, start) = match (pool, &self.prefix_cache) {
            (PalwEmbeddingPoolV1::LastToken, Some(lock)) => {
                match lock.lock().unwrap_or_else(|e| e.into_inner()).lookup_v1(class_id, tokenizer_id, &ids) {
                    Some(hit) => (hit.cache, hit.positions as usize),
                    None => (A16Cache::with_storage(self.artifact.shape.n_layers, self.runtime_profile), 0),
                }
            }
            _ => (A16Cache::with_storage(self.artifact.shape.n_layers, self.runtime_profile), 0),
        };
        kv.reserve_positions(prompt_tokens.len(), self.artifact.shape.kv_dim());
        let mut hidden: Vec<Vec<i32>> = Vec::new();
        let mut at = start;
        while at < prompt_tokens.len() {
            let end = (at + self.prefill_run_positions.max(1)).min(prompt_tokens.len());
            let (_, traces) = engine
                .forward_prefill_planned(plan, &mut kv, &prompt_tokens[at..end], at, false)
                .map_err(|e| format!("the embedding run at {at}: {e:?}"))?;
            for trace in traces {
                let row =
                    trace.attn.get(last_layer).and_then(|layer| layer.last()).ok_or("a position's trace has no last-layer row")?;
                hidden.push(row.clone());
            }
            at = end;
        }
        if let (PalwEmbeddingPoolV1::LastToken, Some(lock)) = (pool, &self.prefix_cache) {
            lock.lock().unwrap_or_else(|e| e.into_inner()).insert_v1(class_id, tokenizer_id, &ids, &kv);
        }
        let raw = palw_embedding_pool_v1(&hidden, pool).map_err(|e| e.to_string())?;
        Ok(kaspa_consensus_core::palw_backend::PalwFpEmbeddingV1 {
            raw,
            positions: prompt_tokens.len() as u32,
            cached_prefix_tokens: start as u32,
        })
    }

    /// **RFC-0001 §2.7 stage 2: several answers in ONE decode loop.** Each job prefills (resuming
    /// from the prefix cache where it can) and joins a [`crate::decode_scheduler::DecodeSchedulerV1`];
    /// their decode steps then run batched, the weights read once for the batch. Every answer is
    /// the one [`Self::answer_free_prompt_v1`] gives the job alone — batch invariance, held by
    /// `in_batch_equals_alone` — and `on_token(job index, id)` is called as each id is selected.
    fn answer_batch_free_prompt_v1(
        &self,
        jobs: &[kaspa_consensus_core::palw_backend::PalwFpAnswerJobV1<'_>],
        stop_ids: &[u32],
        on_token: &mut dyn FnMut(usize, u32),
    ) -> Result<Vec<kaspa_consensus_core::palw_backend::PalwFpAnswerV1>, String> {
        let Some(plan) = self.plan.as_ref().filter(|plan| plan.one_pass_prefill_supported()) else {
            // A class whose plan cannot batch answers its jobs one at a time.
            return jobs
                .iter()
                .enumerate()
                .map(|(i, j)| self.answer_free_prompt_v1(j.job, j.prompt_tokens, stop_ids, &mut |id| on_token(i, id)))
                .collect();
        };
        let engine = A16Engine::new(&self.artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
        let mut scheduler = crate::decode_scheduler::DecodeSchedulerV1::new(self.decode_batch_width);
        let mut reused = vec![0u32; jobs.len()];
        let mut prompts: Vec<Vec<u32>> = Vec::with_capacity(jobs.len());
        for (i, j) in jobs.iter().enumerate() {
            let (kv, hit, logits) = self.answer_prefill_v1(&engine, j.job, j.prompt_tokens)?;
            reused[i] = hit as u32;
            prompts.push(j.prompt_tokens.iter().map(|t| *t as u32).collect());
            scheduler.admit(crate::decode_scheduler::DecodeSequenceV1::start(
                i as u64,
                kv,
                j.prompt_tokens.len(),
                &logits,
                j.job.decoder_v1(),
                j.job.decode_token_limit as usize,
                stop_ids.to_vec(),
            ));
            for (id, token) in scheduler.drain_events() {
                on_token(id as usize, token);
            }
        }
        let mut answers: Vec<Option<kaspa_consensus_core::palw_backend::PalwFpAnswerV1>> = vec![None; jobs.len()];
        loop {
            for done in scheduler.take_finished() {
                let i = done.id as usize;
                self.remember_conversation_v1(jobs[i].job, &prompts[i], &done.output_token_ids, &done.cache);
                answers[i] = Some(kaspa_consensus_core::palw_backend::PalwFpAnswerV1 {
                    output_token_ids: done.output_token_ids,
                    cached_prefix_tokens: reused[i],
                    ended_on_stop_id: done.ended_on_stop_id,
                });
            }
            if scheduler.is_idle() {
                break;
            }
            scheduler.step(&engine, plan)?;
            for (id, token) in scheduler.drain_events() {
                on_token(id as usize, token);
            }
        }
        answers.into_iter().map(|a| a.ok_or_else(|| "a batched job finished with no answer".to_string())).collect()
    }

    /// DRILL ONLY: the honest run with the committed tile at `leaf` made a lie, in the stream of
    /// the fold (the trait's contract: never on a network carrying value).
    fn execute_free_prompt_with_injected_fault(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        leaf_index: u64,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        self.execute_free_prompt_v1(
            job,
            prompt_tokens,
            &mut |_| {},
            Some(kaspa_consensus_core::palw_backend::PalwFreePromptDrillFaultV1::Leaf { leaf: leaf_index }),
        )
    }

    /// DRILL ONLY (ADR-0152 §4-ter N5): the free-prompt run with the named lie — a tile, or a fused
    /// attention output followed downstream or not.
    fn execute_free_prompt_with_drill_fault_v2(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        fault: kaspa_consensus_core::palw_backend::PalwFreePromptDrillFaultV1,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        self.execute_free_prompt_v1(job, prompt_tokens, &mut |_| {}, Some(fault))
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
            // **An attempt folds only on a held class** (`palw_attempt_capture_folds_v1`: this
            // build's `execute` writes the dense capture for every other), and a fold's rows are
            // tied to its step tree by no rule a seat can run (SEAT-0's head rule reads dense
            // leaves) — so under an attempt claim of a class whose attempts do not fold, the
            // selecting row would be a free field: bend it, move the token, re-derive the roots,
            // and every seat licensed each bend. Refused, as material no honest producer serves.
            // A held class's fold attempt is the honest producer's own material and stays
            // licensable here; its selecting row is the residual SEAT-R (a full-mask `Valid` only
            // from a replay) and F1c's rule 12 close at `palw_offence_attribution`.
            if claim.attempt_draw.is_some()
                && !kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1(&self.profile)
            {
                return PalwMaterialVerdictV1::Mismatch;
            }
            // **SEAT-S1: the whole job, here too.** This branch checked the id alone, and a held
            // class's attempt IS a fold — so a fold of a smaller job under the right id (a
            // shortened prefill, the decode call skipped) was licensed by every seat holding it.
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
                crate::produce::Base0SeatFamilyV1::IntegerKv,
            ) {
                Ok(true) => PalwMaterialVerdictV1::Matches,
                Ok(false) => PalwMaterialVerdictV1::Mismatch,
                Err(_) => PalwMaterialVerdictV1::Unverifiable,
            };
        }
        if let Ok(decoded) = crate::produce::base0_material_decode_v1(material) {
            // **The whole job, not only its id** (ADR-0117; SEAT-S1): the same rule as the fold
            // branch above, `base0_material_job_is_the_claims_v1`.
            if let Err(verdict) = crate::produce::base0_material_answers_the_claim_v1(self, &decoded.0, &decoded.3, &claim) {
                return verdict;
            }
            // A capture for some OTHER class of this family is not this backend's to vouch for.
            if decoded.0.shape_profile.shape_profile_id() != self.class_profile_id {
                return PalwMaterialVerdictV1::Unverifiable;
            }
            return match crate::produce::base0_material_matches_claim_capped_v1(
                &decoded,
                claim.execution_root,
                claim.trace_root,
                self.network_ladder,
                crate::produce::Base0SeatFamilyV1::IntegerKv,
            ) {
                Ok(true) => PalwMaterialVerdictV1::Matches,
                Ok(false) => PalwMaterialVerdictV1::Mismatch,
                Err(_) => PalwMaterialVerdictV1::Unverifiable,
            };
        }
        let Some(run) = qwen25_a16_material_decode_v1(material) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        // **The legacy composite is the ledger-compiled class's alone.** A court-capable backend's
        // `execute` writes the family codec for every attempt, and this decode re-derives the roots
        // from the rows it was sent with no execution behind them — so on this backend every new
        // set of rows was a new root the seat licensed, and SEAT-0's rules never saw them.
        if self.court_capable {
            return PalwMaterialVerdictV1::Mismatch;
        }
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
        // The legacy composite is recomputed under the job the block asked for, ADR-0117's rule included.
        let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, claim.attempt_draw.unwrap_or(false));
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
            layer_count: binding.shape_profile.layer_count,
        })
    }

    /// **ADR-0062 D3's responder, for the dense tier** (mainnet audit 2026-09-06, C-5).
    ///
    /// This impl block declared `supports_court() == true` — whose contract is exactly this method
    /// and `bisect_prefix_state` — while taking the trait's
    /// `Err("this family has no data-availability responder")`, so on a network that arms
    /// `palw_da_court` (testnet-11 at DAA 1,900; a card from block one) every accusation against a
    /// dense-tier claim won by silence for the price of one bond. The body is the shared one: it
    /// reads the scheme, the vocabulary and the decode count off the capture's own binding, which
    /// is why this is a delegation and not a second implementation.
    fn disclose_trace_event(
        &self,
        material: &[u8],
        row: u32,
        tile: u8,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1, String> {
        crate::produce::base0_disclose_trace_event_v1(material, row, tile)
    }

    fn disclose_selected_event_v1(
        &self,
        material: &[u8],
        row: u32,
        tile: u32,
        maximum_leaves: u64,
    ) -> Result<
        (
            kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
            kaspa_consensus_core::palw_selected_event_source_v1::PalwSelectedTiledEventV1,
        ),
        String,
    > {
        crate::produce::base0_disclose_selected_event_v1(material, row, tile, maximum_leaves)
    }

    fn open_flat_public_source_v1(
        &self,
        source: std::sync::Arc<dyn kaspa_consensus_core::palw_backend::PalwOriginalCaptureReaderV1>,
        binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    ) -> Result<Box<dyn kaspa_consensus_core::palw_backend::PalwFlatPublicSourceReaderV1>, String> {
        Ok(Box::new(crate::flat_public_source::Base0FlatPublicSourceV1::open_v1(source, binding)?))
    }

    fn write_flat_public_source_v1(&self, material: &[u8], out: &mut dyn std::io::Write) -> Result<bool, String> {
        crate::flat_public_source::write_flat_public_source_v1(material, out)
    }

    fn public_execution_binding_header_v1(&self,material:&[u8])->Result<kaspa_consensus_core::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,String> {
        crate::flat_public_source::original_public_binding_header_v1(material)
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

    /// Prepared once, opened per draw (DoS audit 2026-09-24, #4): a fold is re-executed ONCE for a
    /// sampled claim, not once per leaf the seat draws.
    fn free_prompt_leaf_prover_v1<'a>(
        &'a self,
        material: &'a [u8],
        prompt_token_ids: &'a [u32],
    ) -> Result<Box<dyn kaspa_consensus_core::palw_backend::PalwCaptureLeafProverV1 + 'a>, String> {
        let prepared = self.prepare_whole_capture_v1(material, Some(prompt_token_ids), None)?;
        Ok(Box::new(A16CaptureLeafProverV1 { backend: self, prepared }))
    }

    // ---- ADR-0077 Decision 8: the interval seam -------------------------------------------
    //
    // Only for a class that can carry a capture at all. The v1 class declares the one-byte map
    // over an `i32` cache, so it commits no checkpoint leg and there is no interval to open —
    // `None`/refusal is the honest answer there, and it is the same fact `supports_court` reports.

    fn fp_interval_count(&self, capture: &[u8]) -> Option<u32> {
        let retention = crate::produce::base0_material_decode_any_v1(capture).ok()?;
        crate::fp_interval::Base0FpIntervalGeometryV1::from_binding_capped_v1(
            retention.binding(),
            self.checkpoint_interval(),
            self.step_ladder_cap(),
        )
        .ok()
        .map(|g| g.interval_count)
    }

    fn fp_interval_count_for(&self, prompt_tokens: u32, decode_tokens_executed: u32) -> Option<u32> {
        if !self.court_capable {
            return None;
        }
        crate::fp_interval::base0_fp_interval_count_for_class_v1(
            &self.profile,
            prompt_tokens,
            decode_tokens_executed,
            self.checkpoint_interval(),
        )
    }

    fn open_fp_interval(&self, capture: &[u8], index: u32, prompt_token_ids: &[u32]) -> Result<Vec<u8>, String> {
        // RFC-0001 §A: the executor replays its own committed ids (a V4 answer is not re-selected).
        crate::fp_interval::base0_fp_executor_replay_v1(capture, || -> Result<Vec<u8>, String> {
            // **Two retention forms, one opening, and the class's map decides whether the history travels.**
            // ADR-0082 Decision 7: a FOLDED retention kept no tiles, so the span's leaf hashes are
            // re-derived by replaying the interval from its checkpoint with this family's own kernels —
            // the loop a seat runs to check the opening. ADR-0082 Decision 9: a class that registers
            // the tiled map is served FLAT (the anchor's chunks stripped; the seat recomputes the
            // state), because the class's own declaration decides, not this executor's preference.
            let chunked =
                match crate::produce::base0_material_decode_any_v1(capture).map_err(|_| "the capture does not decode".to_string())? {
                    crate::produce::Base0RetentionV1::Folded(material) => {
                        crate::fp_interval::base0_open_fp_interval_sparse_anchored_capped_v1(
                            &material,
                            index,
                            prompt_token_ids,
                            self.checkpoint_interval(),
                            self.step_ladder_cap(),
                            &self.interval_kernels_v1(),
                            &|covered| self.fold_anchor_state_v1(&material, prompt_token_ids, covered),
                            self.prompt_ids_form,
                        )
                        .map_err(|e| e.to_string())?
                    }
                    crate::produce::Base0RetentionV1::Dense(material) => crate::fp_interval::base0_open_fp_interval_capped_v1(
                        &material,
                        index,
                        prompt_token_ids,
                        self.checkpoint_interval(),
                        self.network_ladder,
                        self.prompt_ids_form,
                    )
                    .map_err(|e| e.to_string())?,
                };
            if crate::fp_interval::base0_fp_class_requires_flat_openings_v1(&self.profile) {
                return crate::fp_interval::base0_strip_fp_interval_history_v1(&chunked).map_err(|e| e.to_string());
            }
            Ok(chunked)
        })
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
        let state = crate::fp_interval::base0_fp_interval_opening_seat_state_capped_v1(
            &self.seat_memo,
            opening,
            prompt_token_ids,
            self.checkpoint_interval(),
            self.step_ladder_cap(),
        );
        crate::fp_interval::base0_verify_fp_interval_opening_with_state_capped_v1(
            opening,
            claim,
            index,
            prompt_token_ids,
            work_leaves,
            self.checkpoint_interval(),
            self.step_ladder_cap(),
            state.as_ref(),
            &self.interval_kernels_v1(),
            self.prompt_ids_form,
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
    fn fp_held_route_v1(&self, window_receipt_daa: u64) -> Option<kaspa_consensus_core::palw_held_context_v1::PalwHeldSeatRouteV1> {
        crate::fp_interval::base0_fp_held_route_for_v1(&self.profile, window_receipt_daa)
    }

    fn open_fp_resume_v1(&self, capture: &[u8], index: u32, prompt_token_ids: &[u32]) -> Result<Vec<u8>, String> {
        match crate::produce::base0_material_decode_any_v1(capture).map_err(|_| "the capture does not decode".to_string())? {
            crate::produce::Base0RetentionV1::Folded(material) => crate::fp_interval::base0_open_fp_resume_v1(
                &material,
                index,
                prompt_token_ids,
                self.checkpoint_interval(),
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
        crate::fp_interval::base0_fp_accept_resume_v1(
            &self.seat_memo,
            resume,
            context,
            prompt_token_ids,
            covered,
            self.checkpoint_interval(),
            self.step_ladder_cap(),
        )
        .map(|state| state.state_chunks_root)
        .map_err(|e| format!("{e:?}"))
    }

    fn fp_recompute_checkpoint_root(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_token_ids: &[u32],
        output_token_ids: &[u32],
        covered: u32,
    ) -> Result<Hash64, String> {
        // The free-prompt spelling of the context-keyed seam (ADR-0084 Decision 4): one context
        // builder, one recompute.
        let ctx = self.fp_job_context_v1(job).ok_or_else(|| "this job builds no context for this class".to_string())?;
        self.checkpoint_root_for_context_v1(&ctx, prompt_token_ids, output_token_ids, covered)
    }

    /// **The largest `covered` this class's leg carries, in the class's own cadence unit**
    /// (audit B, C-2). Per decode call: `decode_calls`, the seam's default. Per position:
    /// `prefill + decode_calls`, every row the cache ever holds — the count
    /// `palw_checkpoint_count_v1` derives for the leg, whose last leaf is
    /// `palw_checkpoint_covered_at_index_v1(count - 1)` and therefore that number.
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
                self.checkpoint_interval(),
                self.step_ladder_cap(),
                &self.interval_kernels_v1(),
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
        match crate::produce::base0_material_decode_any_v1(capture).map_err(|_| "the capture does not decode".to_string())? {
            crate::produce::Base0RetentionV1::Folded(material) => crate::fp_interval::base0_fp_block_leaves_from_fold_capped_v1(
                &material,
                &opening,
                block_index,
                prompt_token_ids,
                self.checkpoint_interval(),
                self.step_ladder_cap(),
                &self.interval_kernels_v1(),
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
            self.checkpoint_interval(),
            cap,
            &self.interval_kernels_v1(),
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
            self.checkpoint_interval(),
            self.step_ladder_cap(),
            &|bytes| {
                crate::fp_interval::base0_fp_interval_opening_seat_state_capped_v1(
                    &self.seat_memo,
                    bytes,
                    prompt_token_ids,
                    self.checkpoint_interval(),
                    self.step_ladder_cap(),
                )
            },
            &self.interval_kernels_v1(),
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
            self.checkpoint_interval(),
            self.step_ladder_cap(),
            &|bytes| {
                crate::fp_interval::base0_fp_interval_opening_seat_state_capped_v1(
                    &self.seat_memo,
                    bytes,
                    prompt_token_ids,
                    self.checkpoint_interval(),
                    self.step_ladder_cap(),
                )
            },
            &self.interval_kernels_v1(),
            self.prompt_ids_form,
        )
    }

    fn fp_interval_of_leaf_v1(&self, context: &PalwJobContextV2, leaf: u64) -> Option<u32> {
        let interval = self.checkpoint_interval();
        crate::fp_interval::base0_fp_interval_of_leaf_v1(&self.profile, context, interval, leaf)
    }

    /// **A fold answers a leaf from its tree** (ADR-0121): the retained nodes and a replay of the
    /// blocks the step reads, never the interval; a dense retention keeps ADR-0085's annex path.
    fn fp_leaf_refutation_v1(
        &self,
        capture: &[u8],
        prompt_token_ids: &[u32],
        claim: PalwClaimRootsV1,
        work_leaves: u64,
        leaf: u64,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        match crate::produce::base0_material_decode_any_v1(capture) {
            Ok(crate::produce::Base0RetentionV1::Folded(material)) => {
                crate::fp_interval::base0_fp_fold_is_the_claims_v1(&material, claim, work_leaves)?;
                crate::fp_interval::base0_fp_leaf_refutation_from_fold_v1(
                    &material,
                    leaf,
                    prompt_token_ids,
                    self.checkpoint_interval(),
                    self.step_ladder_cap(),
                    &self.interval_kernels_v1(),
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
            self.checkpoint_interval(),
            self.step_ladder_cap(),
            &|bytes| {
                crate::fp_interval::base0_fp_interval_opening_seat_state_capped_v1(
                    &self.seat_memo,
                    bytes,
                    prompt_token_ids,
                    self.checkpoint_interval(),
                    self.step_ladder_cap(),
                )
            },
            &self.interval_kernels_v1(),
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

    /// **The one context this family runs a free-prompt job under** (ADR-0084 Decision 4): the
    /// class facts this backend holds, the budget THAT RAN — the caller's `decode_tokens_executed`,
    /// which is the ceiling for a run that reached it and less for an `EndOfGeneration` one
    /// (ADR-0074 Decision 7) — and the network id. `fp_recompute_checkpoint_root`,
    /// `fp_checkpoint_covered_bound_v1` and `fp_output_root_v1` all go through here, so there is
    /// one spelling of "which job is this".
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

    /// This class's price for a context (ADR-0074 Decision 5), at the ruleset ladder this backend
    /// was built with — the same function `base0_fp_binding_step_space_v1` re-derives an
    /// opening's binding with.
    fn fp_context_work_leaves_v1(&self, context: &PalwJobContextV2) -> Option<u64> {
        kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&self.profile, context, self.step_ladder_cap()).ok()
    }

    fn fp_opening_job_context_v1(&self, opening: &[u8]) -> Option<PalwJobContextV2> {
        crate::fp_interval::base0_fp_interval_opening_job_context_v1(opening)
    }

    /// **ADR-0082 Decision 9, keyed on the context** (ADR-0084 Decision 4): the prefix,
    /// teacher-forced on this seat's own kernels, and the tiled root of the state it reaches —
    /// never a byte of the executor's history. The context is the caller's: a free-prompt job's
    /// through [`Self::fp_job_context_v1`], an attempt claim's through [`Self::job_for_anchor`],
    /// and the memoised state is keyed on it, so the row check that follows finds exactly the
    /// state this claim's own recompute put there.
    fn checkpoint_root_for_context_v1(
        &self,
        context: &PalwJobContextV2,
        prompt_token_ids: &[u32],
        output_token_ids: &[u32],
        covered: u32,
    ) -> Result<Hash64, String> {
        self.artifact_read_probe_v1()?;
        if !self.court_capable {
            return Err("the v1 class commits no checkpoint leg, so it has no state root to recompute".to_string());
        }
        // A seat's drawn intervals of one claim walk the job once (ADR-0110 §9.5).
        crate::fp_recompute::base0_fp_a16_seat_state_v1(
            &self.seat_memo,
            &self.artifact,
            self.plan.as_ref(),
            &self.profile,
            context,
            prompt_token_ids,
            output_token_ids,
            covered,
            self.prompt_ids_form,
        )
        .map(|state| state.state_chunks_root)
        .map_err(|e| e.to_string())
    }

    /// The largest `covered` this class's leg carries for `context`, in the class's own cadence
    /// unit (audit B, C-2): per decode call `decode_calls`; per position `prefill + decode_calls`.
    fn checkpoint_covered_bound_for_context_v1(&self, context: &PalwJobContextV2) -> u32 {
        use kaspa_consensus_core::palw_context_ladder::{PalwCheckpointCadenceV1, palw_checkpoint_cadence_v1};
        let decode_calls = context.exact_decode_tokens.saturating_sub(1);
        match palw_checkpoint_cadence_v1(&self.profile) {
            PalwCheckpointCadenceV1::PerDecodeCall => decode_calls,
            PalwCheckpointCadenceV1::PerPosition => context.declared_prefill_tokens.saturating_add(decode_calls),
        }
    }

    /// `output_root` from the answer's ids under `context` (ADR-0078 X6; ADR-0084 Decisions 1
    /// and 4): the family's rendered hash is a keyed hash of the ids, so the root is recomputable
    /// by anyone holding the ids and the context — the seat binding a served envelope, on either
    /// lane.
    fn output_root_for_context_v1(&self, context: &PalwJobContextV2, output_token_ids: &[u32]) -> Option<Hash64> {
        let legacy = output_commitment_v2(&context.context_hash(), output_token_ids, &rendered_output_hash_v1(output_token_ids));
        Some(self.committed_output_root_v1(context, output_token_ids, legacy))
    }

    /// **ADR-0093 as built: the fused site's evidence out of this capture** — its own committed rows
    /// (see `attn_capture_parts_v1`), the checkpoint leg and the anchor's state recomputed with the
    /// kernels a seat's checkpoint check uses, each refused unless it roots to the capture's own
    /// binding.
    fn attn_site_evidence(
        &self,
        material: &[u8],
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
        accused_out_tile: Option<&kaspa_consensus_core::palw_attn_court_v1::PalwAttnRowOpeningV1>,
    ) -> Result<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnSiteEvidenceV1, String> {
        if !self.court_capable {
            return Err("the v1 class carries no capture to read a fused site out of".to_string());
        }
        let (binding, tiles, prefix, checkpoints, prompt_ids, generated) =
            self.attn_capture_parts_v1(material, carried_prompt, accused_out_tile.is_some())?;
        let rows = match (prefix, accused_out_tile) {
            (true, Some(accused_out_tile)) => {
                crate::attn_responder::Base0AttnRowsV1::HonestPrefix { honest: &tiles, accused_out_tile }
            }
            _ => crate::attn_responder::Base0AttnRowsV1::Committed(&tiles),
        };
        let inventory = crate::inventory::a16_inventory_v1(&self.artifact, &self.profile).map_err(|e| format!("{e:?}"))?;
        let (artifact, plan) = (&self.artifact, self.plan.as_ref());
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
                let mut kernels = crate::fp_recompute::A16RecomputeKernelsV1::new(artifact, plan).map_err(|e| e.to_string())?;
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

    /// **ADR-0093 Decisions 7 and 8: the bottom's evidence from the accused's filing alone** — this
    /// node's own re-execution of the accused's job for every row before the disputed leaf, the
    /// accused's own opened tile and (when filed) anchor off its root claim. No capture of the
    /// accused's is read, so an over-cap capture that is never pulled bottoms all the same.
    fn attn_site_evidence_from_filing(
        &self,
        filing: &kaspa_consensus_core::palw_attn_responder_v1::PalwAttnAccusedFilingV1,
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
    ) -> Result<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnSiteEvidenceV1, String> {
        if !self.court_capable {
            return Err("the v1 class carries no capture to read a fused site out of".to_string());
        }
        let binding = &filing.binding;
        let prompt_ids = self.attn_prompt_ids_v1(binding, carried_prompt)?;
        let rerun = self.attn_rerun_v1(binding, &prompt_ids)?;
        let inventory = crate::inventory::a16_inventory_v1(&self.artifact, &self.profile).map_err(|e| format!("{e:?}"))?;
        let (artifact, plan) = (&self.artifact, self.plan.as_ref());
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
                let mut kernels = crate::fp_recompute::A16RecomputeKernelsV1::new(artifact, plan).map_err(|e| e.to_string())?;
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

    /// **ADR-0152 §4-ter N4: held-answerability aware past `palw_offence_attribution`**
    /// ([`Qwen25A16Backend::with_held_answerability_v1`]). A held class's dissection is answered by
    /// the windowed builder ([`Self::attn_site_evidence_held_v1`] — this family has both halves, N1
    /// and N2) and only where an honest party can answer inside the chain's turn — the consensus
    /// predicate `palw_held_class_unanswerable_v1` (the context bound; the review's F5, a
    /// whole-context reference replay past the turn). A held class it refuses (the 2M row) says
    /// `false`, so the producer's guard refuses to underwrite a claim it could never defend. Below
    /// the fence, byte for byte what it was.
    fn supports_dissection(&self) -> bool {
        self.court_capable
            && kaspa_consensus_core::palw_class_admission_v2::palw_fused_sites_are_dissectable_v1(&self.profile)
            && !(self.held_answerability
                && kaspa_consensus_core::palw_class_admission_v2::palw_held_class_unanswerable_v1(&self.profile).is_some())
    }

    /// **ADR-0152 §4-ter N1/N2: a held fused site's evidence, windowed** — see
    /// [`kaspa_consensus_core::palw_backend::PalwExecutionBackendV1::attn_site_evidence_held_v1`].
    fn attn_site_evidence_held_v1(
        &self,
        material: &[u8],
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
        filing: Option<&kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldFilingV1>,
    ) -> Result<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldEvidenceV1, String> {
        if !self.court_capable {
            return Err("the v1 class carries no capture to read a fused site out of".to_string());
        }
        if !kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&self.profile) {
            return Err("the windowed builder answers a held class's fused site; this class is not held".to_string());
        }
        match filing {
            None => self.attn_held_responder_v1(material, narrowed, carried_prompt),
            Some(filing) => self.attn_held_challenger_v1(filing, narrowed, carried_prompt),
        }
    }

    /// **ADR-0152 §4-ter.3 step 6: the accused's committed cache-write row, opened against its step
    /// root** — see [`kaspa_consensus_core::palw_backend::PalwExecutionBackendV1::attn_held_cache_write_rows_v1`].
    fn attn_held_cache_write_rows_v1(
        &self,
        filing: &kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldFilingV1,
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
        kind: kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1,
        attn_layer: u16,
        history_position: u32,
    ) -> Result<Vec<kaspa_consensus_core::palw_attn_court_v1::PalwAttnRowOpeningV1>, String> {
        self.attn_held_cache_write_rows_of_v1(filing, narrowed, carried_prompt, kind, attn_layer, history_position)
    }

    fn has_fused_site(&self) -> bool {
        self.profile.attn_nodes.iter().any(|n| n.op_kind == kaspa_consensus_core::palw_step::PalwStepOpKindV1::AttnFused)
    }

    fn operand_openings_for(
        &self,
        refutation: &kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1,
    ) -> Result<Vec<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1>, String> {
        let inventory = crate::inventory::a16_inventory_v1(&self.artifact, &self.profile).map_err(|e| format!("{e:?}"))?;
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
        crate::inventory::a16_inventory_digest_v1(&self.artifact, &self.profile).map_err(|e| format!("{e:?}"))
    }

    fn canonical_job_prefill_tokens(&self) -> Option<usize> {
        Some(self.canonical_job.0 as usize)
    }

    /// The producer's figure for a `prefill_tokens` attempt — through the resource profile, so
    /// this legacy verb and `resource_profile_v1` cannot disagree.
    fn attempt_working_set_bytes(&self, prefill_tokens: usize) -> Option<u64> {
        // The attempt is the canonical job: its decode count is the class's, so the gate's figure
        // is the attempt's and not a one-call floor's.
        let job = self.resource_job_v1(u32::try_from(prefill_tokens).ok()?, self.canonical_job.1.max(1));
        self.resource_profile_v1(Some(&job), kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1::Producer)
            .map(|p| p.working_set_bytes())
    }

    fn runtime_profile_v1(&self) -> Option<kaspa_consensus_core::palw_resource_profile_v1::PalwRuntimeProfileV1> {
        Some(self.runtime_profile)
    }

    /// **The width this instance's prefill runs at** — `None` where its plan cannot run a prefill a
    /// layer at a time (the v2 reference route, a plan that reads the cache before a position writes
    /// it): every execution of it steps one position at a time, whatever is set, so its memory does not
    /// move with the width and a node prices it once (int-10.2 A2).
    fn prefill_run_positions_v1(&self) -> Option<u32> {
        self.plan.as_ref().filter(|plan| plan.one_pass_prefill_supported()).map(|_| self.prefill_run_positions as u32)
    }

    fn set_prefill_run_positions_v1(&mut self, positions: u32) {
        self.prefill_run_positions = (positions as usize).max(1);
    }

    /// The rotary table, generated at decode and resident for the artifact's life: `cos_q` and
    /// `sin_q`, `max_position × d_head / 2` lanes of `i32` each.
    fn artifact_derived_resident_bytes_v1(&self) -> u64 {
        (self.artifact.rope.cos_q.len() as u64).saturating_add(self.artifact.rope.sin_q.len() as u64).saturating_mul(4)
    }

    fn resource_profile_v1(
        &self,
        job: Option<&PalwJobContextV2>,
        role: kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1,
    ) -> Option<kaspa_consensus_core::palw_resource_profile_v1::PalwResourceProfileV1> {
        let canonical;
        let job = match job {
            Some(job) => job,
            None => {
                canonical = self.resource_job_v1(self.canonical_job.0, self.canonical_job.1);
                &canonical
            }
        };
        use kaspa_consensus_core::palw_resource_profile_v1::{
            PalwCaptureRetentionV1, PalwResourceRoleV1, palw_attempt_capture_folds_v1, palw_profile_max_tile_len_v1,
        };
        // The job's step space sizes every role's capture — the dense sink's vector, the fold's
        // retained set, a segment's window — and an unpriceable job must not turn into a zero-leaf
        // profile: `None`, by name.
        let leaf_count =
            kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&self.profile, job, self.step_ladder_cap()).ok()?;
        // **What each role's capture retains — the term the kills were made of**, chosen exactly as
        // the lanes choose their sinks: the attempt folds for a held class and keeps dense tiles
        // otherwise (`execute`); the verdict replay folds (`execute_for_verdict`); a segment replay
        // folds the range it proves (`segment_opening::base0_replay_segment_opening_v2`: its edges
        // and one digest a block), still priced as the window's hashes — an upper bound of it.
        let fold = PalwCaptureRetentionV1::Fold {
            retain_level: crate::fp_capture::palw_base0_sparse_retain_level_for_class_v1(&self.profile, self.step_ladder_cap()),
        };
        let capture = match role {
            PalwResourceRoleV1::Producer if !palw_attempt_capture_folds_v1(&self.profile) => {
                PalwCaptureRetentionV1::DenseTiles { tile_len: palw_profile_max_tile_len_v1(&self.profile) }
            }
            PalwResourceRoleV1::Producer | PalwResourceRoleV1::FullSeat => fold,
            PalwResourceRoleV1::PartialSeat { .. } => PalwCaptureRetentionV1::ReplayHashes,
        };
        kaspa_consensus_core::palw_resource_profile_v1::palw_resource_profile_v1(
            &self.profile,
            job,
            leaf_count,
            self.runtime_profile,
            role,
            self.runtime_limits_v1(),
            capture,
        )
    }

    fn artifact_root_and_leaf_count(&self) -> Result<(kaspa_consensus_core::Hash64, u32), String> {
        crate::inventory::a16_inventory_root_and_count_v1(&self.artifact, &self.profile).map_err(|e| format!("{e:?}"))
    }

    fn artifact_readiness_material(
        &self,
        draw: &[u32],
    ) -> Result<
        (
            kaspa_consensus_core::Hash64,
            Vec<kaspa_consensus_core::Hash64>,
            Vec<(u32, kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1)>,
        ),
        String,
    > {
        // One streamed walk; nothing materialized, nothing retained. See
        // `a16_readiness_material_streamed_v1` for what this replaced and what it cost.
        crate::inventory::a16_readiness_material_streamed_v1(&self.artifact, &self.profile, draw).map_err(|e| format!("{e:?}"))
    }

    fn artifact_readiness_material_streamed_v1(
        &self,
        draw: &[u32],
        on_leaf: &mut dyn FnMut(kaspa_consensus_core::Hash64),
    ) -> Option<
        Result<(kaspa_consensus_core::Hash64, u32, Vec<(u32, kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1)>), String>,
    > {
        // The same one walk as `artifact_readiness_material`, the leaf vector left to the caller's sink.
        Some(
            crate::inventory::a16_readiness_leaves_streamed_v1(&self.artifact, &self.profile, draw, on_leaf)
                .map_err(|e| format!("{e:?}")),
        )
    }

    fn artifact_row_opening(&self, index: u32) -> Result<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1, String> {
        // The digest roots the path; a second pass copies the bytes of the named leaf alone — the
        // inventory is never materialised to open one row of it (ADR-0135). Reuse the process
        // cache so a 2M rope walk is not paid again per leaf.
        let digest = crate::inventory::a16_inventory_digest_arc_v1(&self.artifact, &self.profile).map_err(|e| format!("{e:?}"))?;
        let row = digest
            .rows()
            .get(index as usize)
            .ok_or_else(|| format!("leaf {index} is outside an inventory of {}", digest.leaf_count()))?;
        let bytes =
            crate::inventory::a16_inventory_row_bytes_v1(&self.artifact, &self.profile, &row.tensor_name, row.layer, row.row_start)
                .map_err(|e| format!("{e:?}"))?
                .ok_or_else(|| format!("leaf {index} names a row the artifact does not emit"))?;
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
        if !self.court_capable {
            return Err("the v1 class carries no capture to drill".to_string());
        }
        // The dense capture on every class, the held one included — the reason is
        // `execute_with_injected_fault`'s.
        let (job, prompt) = crate::produce::base0_drill_job_v1(self, self.prompt_ids_form, job, prompt, fault)?;
        let mut run =
            a16_execute_for_attempt_capped_v1(&self.artifact, &self.profile, self.plan.as_ref(), &job, &prompt, self.network_ladder)?;
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
        if !self.court_capable {
            return Err("the v1 class carries no capture to tamper with".to_string());
        }
        // **The drill tampers a DENSE capture on every class, the held one included.** Production's
        // held attempt folds (`execute`), but this verb's whole purpose is a lie in a RETAINED tile that
        // the same instance then judges honestly — the seat's replay finds the fault, the court's
        // refutation carries the lying tile and convicts — and a fold holds no tile to lie in while a
        // lie made in the stream would poison this instance's own replays. Never reached on a network
        // carrying value (the trait's contract); at fixture scale the dense capture is small, and a 2M
        // drill on this lane is as impossible as it was before — it goes through the free-prompt drill.
        let mut run =
            a16_execute_for_attempt_capped_v1(&self.artifact, &self.profile, self.plan.as_ref(), job, prompt, self.network_ladder)?;
        let _ctx_hash = job.context_hash();
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
                kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_ctx_v1(job, &profile_hash, &slot.1);
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
/// refutation helpers read. The floor keeps an identical private helper beside its own backend;
/// this one is the A16 family's copy of the same eleven lines rather than a premature trait.
fn a16_leaves_by_position(
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    tiles: &[(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1)],
) -> Vec<Hash64> {
    let _ctx_hash = binding.job_context.context_hash();
    let profile_hash = binding.shape_profile.shape_profile_id();
    let mut leaves = vec![Hash64::default(); binding.step_leaf_count as usize];
    for (index, leaf) in tiles {
        if let Some(slot) = leaves.get_mut(*index as usize) {
            *slot = kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_ctx_v1(&binding.job_context, &profile_hash, leaf);
        }
    }
    leaves
}

#[cfg(test)]
mod free_prompt_tests;

/// A diagnostic, never part of the suite: open intervals of a staged free-prompt capture
/// (`MISAKA_PROBE_MATERIAL`, FPC1) with the real artifact (`MISAKA_PALW_ARTIFACT`) and report
/// what the opener says, interval by interval (`MISAKA_PROBE_INTERVALS`, comma-separated).
/// A diagnostic, never part of the suite: recompute a free-prompt claim's output root from a
/// served `FPA1` answer envelope (`MISAKA_ANSWER_FILE`) with a named artifact
/// (`MISAKA_PALW_ARTIFACT`), so the number a seat computes can be compared with the number the
/// chain committed without waiting for a seat's next duty tick.
#[cfg(test)]
mod probe_output_root {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;

    /// A diagnostic, never part of the suite: recompute a free-prompt claim's output root from a
    /// served `FPA1` answer envelope (`MISAKA_ANSWER_FILE`) with a named artifact
    /// (`MISAKA_PALW_ARTIFACT`), so the number a seat computes can be compared with the number the
    /// chain committed without waiting for a seat's next duty tick.
    #[test]
    #[ignore]
    fn recompute_the_output_root_of_a_served_answer() {
        let Ok(answer_path) = std::env::var("MISAKA_ANSWER_FILE") else { return };
        let Ok(artifact_path) = std::env::var("MISAKA_PALW_ARTIFACT") else { return };
        let network = std::env::var("MISAKA_NETWORK_ID").unwrap_or_else(|_| "testnet-11".to_string());
        let bytes = std::fs::read(&answer_path).expect("the answer file reads");
        let answer = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_answer_decode_v1(
            &bytes,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("an FPA1 envelope");
        let job = &answer.material.job;
        eprintln!(
            "job: class {} decode_limit {} prompt_ids {} output_ids {}",
            job.class_id,
            job.decode_token_limit,
            answer.material.prompt_token_ids.len(),
            answer.output_token_ids.len()
        );
        eprintln!("job id: {}", kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(job));
        let file = std::fs::read(&artifact_path).expect("the artifact reads");
        let artifact = std::sync::Arc::new(crate::artifact::decode_artifact_file_v1(&file).expect("the artifact decodes"));
        let court = kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(1 << 26, 20, 2).expect("court params");
        for row in crate::classes::canonical_classes_v1(&court) {
            let Ok(backend) = super::Qwen25A16Backend::from_registered_profile_in_lane_v1(
                artifact.clone(),
                network.as_bytes().to_vec(),
                row.profile.clone(),
                row.canonical_job,
                row.source,
            ) else {
                continue;
            };
            let root: Option<kaspa_hashes::Hash64> = backend.fp_output_root_v1(job, &answer.output_token_ids);
            let ctx = backend.fp_job_context_v1(job);
            let intervals = backend.fp_interval_count_for(
                ctx.as_ref().map(|c| c.declared_prefill_tokens).unwrap_or(0),
                ctx.as_ref().map(|c| c.exact_decode_tokens).unwrap_or(0),
            );
            let covered = ctx.as_ref().map(|c| backend.checkpoint_covered_bound_for_context_v1(c));
            eprintln!(
                "  {:<34} profile {}  ctx {}  output_root {}  intervals {:?}  covered_bound {:?}  prefill {:?} decode {:?}",
                row.model_id,
                &row.profile.shape_profile_id().to_string()[..16],
                ctx.as_ref().map(|c| c.context_hash().to_string()[..16].to_string()).unwrap_or_else(|| "-".into()),
                root.map(|r| r.to_string()).unwrap_or_else(|| "-".into()),
                intervals,
                covered,
                ctx.as_ref().map(|c| c.declared_prefill_tokens),
                ctx.as_ref().map(|c| c.exact_decode_tokens)
            );
        }
    }
}

#[cfg(test)]
mod probe_open_interval {
    #[test]
    #[ignore]
    fn open_intervals_of_a_staged_capture() {
        use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
        let Ok(material_path) = std::env::var("MISAKA_PROBE_MATERIAL") else { return };
        let artifact_path = std::env::var("MISAKA_PALW_ARTIFACT").expect("MISAKA_PALW_ARTIFACT");
        let indices: Vec<u32> = std::env::var("MISAKA_PROBE_INTERVALS")
            .unwrap_or_else(|_| "0".into())
            .split(',')
            .map(|s| s.trim().parse().expect("an interval index"))
            .collect();
        let started = std::time::Instant::now();
        let bytes = std::fs::read(&material_path).expect("the material reads");
        let payload = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_decode_v1(
            &bytes,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("an FPC1 capture");
        eprintln!(
            "material {} bytes, capture {} bytes, prompt ids {}, class {} ({:.0?})",
            bytes.len(),
            payload.capture.len(),
            payload.material.prompt_token_ids.len(),
            payload.material.job.class_id,
            started.elapsed()
        );
        let retention = crate::produce::base0_material_decode_any_v1(&payload.capture).expect("the capture decodes");
        let binding = match &retention {
            crate::produce::Base0RetentionV1::Folded(m) => m.binding.clone(),
            crate::produce::Base0RetentionV1::Dense(m) => m.0.clone(),
        };
        eprintln!(
            "binding: leaves {} checkpoints {} ctx prefill {} decode {} ({:.0?})",
            binding.step_leaf_count,
            binding.checkpoint_count,
            binding.job_context.declared_prefill_tokens,
            binding.job_context.exact_decode_tokens,
            started.elapsed()
        );
        let file = std::fs::read(&artifact_path).expect("the artifact reads");
        let artifact = crate::artifact::decode_artifact_file_v1(&file).expect("the artifact decodes");
        eprintln!("artifact decoded ({:.0?})", started.elapsed());
        let backend =
            super::Qwen25A16Backend::new(std::sync::Arc::new(artifact), b"devnet".to_vec(), binding.shape_profile.clone(), (63, 2))
                .expect("the backend builds")
                .with_step_ladder_cap(1 << 26);
        for index in indices {
            let t = std::time::Instant::now();
            match backend.open_fp_interval(&payload.capture, index, &payload.material.prompt_token_ids) {
                Ok(opening) => eprintln!("interval {index}: opened, {} bytes ({:.0?})", opening.len(), t.elapsed()),
                Err(e) => eprintln!("interval {index}: does not open ({:.0?}): {e}", t.elapsed()),
            }
        }
    }
}

/// **ADR-0121's measured run: a held job past the network's ladder, end to end.**
///
/// One drill, run twice: over a fixture in the default suite (a network ladder of `2^12` so the
/// fixture's job is past it), and over the real 1.5B artifact when `MISAKA_PALW_HELD_ARTIFACT`
/// names it. The held row (graph-v7), built at the network's ladder, runs a free-prompt job past
/// that ladder; then every route the node takes for a held claim, timed and sized: the executor's
/// interval openings, the seat's own anchor recompute and its verdict on the first, middle and
/// last interval; and a liar — a separate instance, as a liar is a separate node — whose fold
/// commits one wrong tile in the last interval: the seat's fault, the block it names by the
/// interval's count, the leaf named from the served block, and the one-move verdict on the
/// evidence the liar's own fold builds.
///
/// ```text
/// MISAKA_PALW_HELD_ARTIFACT=/path/qwen25-1.5b-a16.palwart MISAKA_PALW_HELD_POSITIONS=1024 \
///   cargo test --release -p misaka-palw-base0 --lib -- held_job_past_the_networks_ladder --nocapture
/// ```
#[cfg(test)]
mod held_real_row_probe;

/// **ADR-0152 §4-ter N1/N2/N4 at fixture scale** — the windowed builder of a held fused site, both
/// parties, on a held graph-v7 row (the held map, per-position checkpoints, a class ladder of `2^40`)
/// small enough to also run densely.
#[cfg(test)]
mod aheld_windowed_builder;

/// **ADR-0152 §4-ter T-A3 and the held happy path, end to end at fixture scale** — the node's
/// windowed builder (both parties) against the court's kernels and against the FOLD: a held
/// graph-v7 row past the materialization cap (a network ladder of `2^11`, the class's `2^40`), its
/// real executions (honest, or lying in a fused attention output and following it or not, through
/// the N5 drill), the one-move accusation the executor's own fold evidence makes, the held root
/// claim the responder builds (tag 57), the rounds and choices, and the bottom — every block through
/// `apply_palw_transition_v2_with_extras` with its delta re-applied and reverted and its carriage
/// reloaded, past `palw_offence_attribution` and below it.
#[cfg(test)]
mod aheld_end_to_end;

/// **RFC-0001 §2.6 stage 1 — the KV prefix cache and the answer-only path, on the real engine.**
#[cfg(test)]
mod prefix_cache_serving;
