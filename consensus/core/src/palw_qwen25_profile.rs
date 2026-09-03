//! **Qwen2.5 as a second deterministic execution class (Phase 0 → conditions 3 and 4).**
//!
//! `PALW-BASE-0` is the permanent liveness floor and this does not touch it. Qwen2.5 is a
//! *second* class that inherits BASE-0's integer arithmetic, its court, its artifact root and its
//! step-dispute machinery — the same closed kernel catalog, a different graph.
//!
//! # What is measured and what is chosen
//!
//! The geometry is MEASURED, from Hugging Face's own `config.json` and from the `safetensors`
//! header of the real weight file (`docs/palw-qwen25-class-phase0.md` records the readings and
//! the date). A profile that disagrees with the file describes an execution that never ran, and
//! the court would then adjudicate steps against it.
//!
//! One thing is chosen and is the user's to overrule: **`Qwen2.5-2B` does not exist.** Hugging
//! Face answers `{"error":"Invalid username or password."}` for that repository — its response
//! for one that is not there — while `Qwen2.5-1.5B` returns real metadata. The dense base family
//! is 0.5B, 1.5B, 3B, 7B, 14B, 32B, 72B. All three small members are the same architecture
//! (`Qwen2ForCausalLM`) and differ only in geometry, so the graph below is one graph; the size is
//! a constant, and [`QWEN25_1_5B`] and [`QWEN25_3B`] are both here.
//!
//! # The three transformations the artifact must record
//!
//! Qwen2.5 is not BASE-0's graph, and three of its steps have no BASE-0 op. Each is resolved by
//! an EXACT transformation applied when the artifact is built — none is an approximation, and
//! every one of them must be recorded in the artifact's quantization semantics so a verifier
//! reproduces it rather than trusting it:
//!
//! * **G1, the RMSNorm learned gain.** BASE-0's `RmsNorm` takes no weight, and neither `MulElem`
//!   nor `AddElem` can multiply by a *registered* vector (both need two opened rows). A gain
//!   followed by a linear layer is `W·diag(g)·x`, so `diag(g)` folds into `W`. Every norm here is
//!   consumed only by linear layers — `input_layernorm` by q/k/v, which all see the same gain;
//!   `post_attention_layernorm` by gate/up; `model.norm` by the tied lm_head. So there is no gain
//!   node in this graph, and that is not a simplification: the arithmetic is identical.
//! * **G2, the q/k/v bias.** BASE-0 had no additive registered term at all until `QuantParams`
//!   gained a zero point (ADR-0040 amendment). The bias rides that: `Requantize` after each of
//!   the three projections carries it per channel.
//! * **G3, RoPE's convention.** `KDESC_BASE0_ROPE` is `pinned-table-pairwise`; Qwen2 is NEOX-style
//!   `rotate_half`, pairing `(i, i + d/2)` where pairwise pairs `(2i, 2i+1)`. A fixed permutation
//!   of the head-dim axis converts one into the other, and it folds into the q and k projection
//!   rows — exact, and it leaves the adjudicated kernel untouched, which is the point.
//!
//! # What this module is not
//!
//! It is the graph, not the weights. The artifact — int8 rows, per-channel requantization
//! parameters carrying the folded biases, the pinned integer rotary table, the tokenizer
//! commitment — is Phase 2's, and no function can invent it.

use crate::Hash64;
use crate::palw_step::{
    PALW_STEP_OBJECT_VERSION_V1, PalwShapeProfileV3, PalwStepError, PalwStepLaneV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1,
};

/// The int8 dtype byte. One weight type throughout: the class is integer arithmetic, and any
/// variance would mean it is not this class.
pub const QWEN25_WEIGHT_DTYPE_I8: u8 = 24;

/// A Qwen2.5 dense member's measured geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwQwen25GeometryV1 {
    pub layer_count: u16,
    pub hidden_dim: u32,
    pub ffn_dim: u32,
    pub attn_heads: u16,
    /// **Grouped-query attention.** Every member of the family has 2, against 12–16 query heads,
    /// so this is never equal to `attn_heads` and the profile must carry both.
    pub attn_kv_heads: u16,
    pub attn_head_dim: u32,
    pub vocab_size: u32,
    pub n_ctx: u32,
    pub n_threads: u32,
    /// The integer RMS-norm epsilon in Qk. The float config says `1e-06`; the value here is the
    /// integer the class is registered with, because BASE-0's norm has no float epsilon and the
    /// court recomputes with the CLASS's constant.
    pub rms_eps_q: i64,
    pub tile_len: u32,
}

/// `Qwen2.5-1.5B`, measured 2026-08-21. The nearest existing member to the "2B" the goal names.
pub const QWEN25_1_5B: PalwQwen25GeometryV1 = PalwQwen25GeometryV1 {
    layer_count: 28,
    hidden_dim: 1536,
    ffn_dim: 8960,
    attn_heads: 12,
    attn_kv_heads: 2,
    attn_head_dim: 128,
    vocab_size: 151_936,
    n_ctx: 4_096,
    n_threads: 1,
    rms_eps_q: 1,
    tile_len: 128,
};

/// **The integer-engine shape an artifact for this geometry must have.**
///
/// The mirror of the floor's `palw_rc_base0_shape_v1`, and it exists for the same stated reason:
/// "every field is the geometry's, so the two cannot describe different classes". Before it, the
/// converter carried its own copy of the arithmetic — `eps_q: 1 << 8`, inherited from the floor —
/// while `QWEN25_1_5B` declared `rms_eps_q: 1`. That is two arithmetic specifications under one
/// model id, and it is not a cosmetic split: the engine norms with the ARTIFACT's epsilon and the
/// court re-norms with the CLASS's, so an artifact built at 256 under a class registered at 1 has
/// every honest execution convicted. This repo already has that bug in its history.
///
/// Returned as scalars because `Base0ShapeV1` lives in the engine crate, which consensus cannot
/// name. The engine crate's `classes` registry is what assembles them.
pub struct PalwQwen25ArtifactShapeV1 {
    pub n_layers: usize,
    pub n_heads: usize,
    pub n_kv_heads: usize,
    pub d_head: usize,
    pub d_ff: usize,
    pub vocab: usize,
    pub max_position: usize,
    pub eps_q: i64,
}

pub fn qwen25_artifact_shape_v1(g: PalwQwen25GeometryV1) -> PalwQwen25ArtifactShapeV1 {
    PalwQwen25ArtifactShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        n_kv_heads: g.attn_kv_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        // The rotary table is generated for the class's own context length, and `max_position` is
        // inside the artifact digest — so a class registered at one context and an artifact built
        // for another are two classes.
        max_position: g.n_ctx as usize,
        eps_q: g.rms_eps_q,
    }
}

/// The canonical geometry for a Hugging Face model id, or `None` for one this build does not know.
///
/// The single place a model id becomes numbers. A converter that read a checkpoint's `config.json`
/// and believed it would let the file decide the class; this makes the checkpoint something to
/// CHECK against instead.
pub fn qwen25_canonical_geometry_v1(model_id: &str) -> Option<PalwQwen25GeometryV1> {
    match model_id {
        "Qwen/Qwen2.5-1.5B" => Some(QWEN25_1_5B),
        "Qwen/Qwen2.5-3B" => Some(QWEN25_3B),
        _ => None,
    }
}

/// `Qwen2.5-3B`, measured the same day — the other reading of "2B".
pub const QWEN25_3B: PalwQwen25GeometryV1 = PalwQwen25GeometryV1 {
    layer_count: 36,
    hidden_dim: 2048,
    ffn_dim: 11_008,
    attn_heads: 16,
    attn_kv_heads: 2,
    attn_head_dim: 128,
    vocab_size: 151_936,
    n_ctx: 4_096,
    n_threads: 1,
    rms_eps_q: 1,
    tile_len: 128,
};

/// **The (tile, context) pair a given ruleset can actually adjudicate for this model** — audit
/// H-04's other half.
///
/// The two shipped constants above are the MODEL: 4,096 tokens is what Qwen2.5 has, and saying
/// otherwise would be a constant that lies about the thing it names.
/// `the_shipped_qwen_tile_len_does_not_admit_its_own_declared_context` pins that neither is
/// admissible as declared, and the measurement behind it is real — 132.4 M and 219.7 M worst-case
/// step leaves against a 4,194,304 ladder at `tile_len` 128.
///
/// What was missing is the other number: given a court, WHICH pair does this model fit into? It is
/// not a choice, it is a search, and leaving it as prose is how "either the tile grows or the
/// context shrinks" stayed a sentence instead of a value. Two ceilings pull against each other —
/// a bigger tile buys ladder depth and costs opening bytes — so the feasible set is an interval
/// per tile, and the answer is the pair with the widest context inside it.
///
/// Deterministic: tiles are tried in ascending order and the FIRST tile achieving the maximum
/// context wins, so two nodes computing this reach one geometry. `None` means the court admits
/// this model at no context at all, which is a real answer about a real ruleset.
#[cfg(test)]
mod a16_family {
    use super::*;
    /// **The A16 family's context ladder against the REAL shipped bundle.**
    ///
    /// Three facts the class ledger (`misaka_palw_base0::classes`) builds on, pinned here where
    /// the geometry lives: n_ctx 16 IS the genesis-registered dense class (its id is asserted
    /// byte-for-byte — a drift here would mean the ledger can no longer name the class the chain
    /// already runs); n_ctx 17..=20 are admissible under the RC court, which is the room the
    /// family has for sibling models before it needs a second axis; and everything past 20 is
    /// refused by the close budget or the ladder, so a sibling CANNOT be given a bigger context
    /// instead of a place in line.
    #[test]
    fn a16_context_ladder_against_the_shipped_bundle() {
        let p = crate::config::params::palw_rc_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &p.palw_consensus_mode else { panic!() };
        for nctx in [15u32, 16, 17, 18, 20, 24, 32, 48, 64, 90, 128] {
            let g = PalwQwen25GeometryV1 { n_ctx: nctx, ..QWEN25_1_5B };
            let profile = match qwen25_a16_profile_v1(g) {
                Ok(pr) => pr,
                Err(e) => {
                    eprintln!("nctx {nctx}: profile err {e:?}");
                    continue;
                }
            };
            let canonical = crate::palw_base0_profile::rc_job_context(&profile, QWEN25_A16_CANONICAL.0, QWEN25_A16_CANONICAL.1);
            let reg = crate::palw_class_admission_v2::palw_post_genesis_registration_v1(
                profile.clone(),
                canonical.clone(),
                kaspa_hashes::Hash64::default(),
                1,
                1,
                5,
                0,
                crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(kaspa_hashes::Hash64::default(), 0)),
                vec![],
            )
            .unwrap();
            // ADR-0069 Decision 5: the weight gate needs a certified set that hashes to the
            // bundle's own `court_e2e_root`. These tests are about the class's SHAPE, so the gate
            // is satisfied rather than exercised — see `catalog_covering_family_for_tests_v1`.
            let certified = crate::palw_e2e_adjudicability::catalog_covering_family_for_tests_v1();
            let mut b = b.clone();
            b.court_e2e_root = crate::palw_e2e_adjudicability::palw_court_e2e_root_of_v1(&certified);
            let verdict = crate::palw_class_admission_v2::verify_class_admission_v2(&b, &profile, &canonical, &reg, &certified);
            match nctx {
                16 => {
                    assert!(verdict.is_ok(), "the genesis dense geometry must stay admissible: {verdict:?}");
                    assert_eq!(
                        profile.shape_profile_id().to_string(),
                        "f942e268f43f05461f648adcb76a1300dbedd93f022d3bba0e88c2ef4349e38f3ac1b70871f3b5195b3b2fb3da221f9c29fe291773a094596add6951aa7902c1",
                        "n_ctx 16 no longer derives the class testnet-11 registered"
                    );
                }
                // **The room this family has, and what bounds it.** Under the 80 KiB
                // one-transaction ceiling the widest admitted context was 21 and the COST is what
                // refused 24; under ADR-0080 design A's 27-chunk group the cost stops binding
                // below the shipped `2^22` ladder, which refuses at 40. So the admitted set grew
                // from {15..21} to {15..39} and the REASON changed with it — see
                // `palw_class_admission_v2::tests::the_widest_context_each_family_admits`, which
                // measures both gates for both families.
                15 | 17 | 18 | 20 | 24 | 32 => {
                    assert!(verdict.is_ok(), "n_ctx {nctx} fell out of the family's room: {verdict:?}")
                }
                _ => assert!(
                    verdict.is_err(),
                    "n_ctx {nctx} was admitted — the family's ceiling moved, revisit the ledger comment: {verdict:?}"
                ),
            }
        }
    }
}
pub fn qwen25_admissible_geometry_v1(
    model: PalwQwen25GeometryV1,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
) -> Option<PalwQwen25GeometryV1> {
    let fits = |tile: u32, n_ctx: u32| -> bool {
        let candidate = PalwQwen25GeometryV1 { n_ctx, tile_len: tile, ..model };
        let Ok(profile) = qwen25_profile_v1(candidate) else { return false };
        // The ladder: the whole context as prefill is the longest job the class admits.
        if crate::palw_step::worst_case_step_leaf_count_v1(&profile).map(|w| w > court.max_step_leaf_count()).unwrap_or(true) {
            return false;
        }
        // ADR-0049 Decision C: and what prosecuting one of its steps costs.
        let Ok(cost) = crate::palw_class_admission_v2::derive_court_cost_v1(&profile) else { return false };
        cost.max_close_bytes <= court.max_close_bytes()
            && cost.max_terminal_macs <= court.max_terminal_macs()
            && u64::from(cost.max_operand_count) <= u64::from(court.max_operand_count())
    };
    let mut best: Option<PalwQwen25GeometryV1> = None;
    let mut tile = 64u32;
    while tile <= crate::palw_step::PALW_STEP_MAX_TILE_LEN {
        if fits(tile, 2) {
            // Widest context this tile admits. Monotone in `n_ctx` (more context is strictly more
            // leaves and never fewer opened bytes), so a binary search is exact.
            let (mut lo, mut hi) = (2u32, model.n_ctx.max(2) + 1);
            while lo + 1 < hi {
                let mid = lo + (hi - lo) / 2;
                if fits(tile, mid) { lo = mid } else { hi = mid }
            }
            if best.map(|b| lo > b.n_ctx).unwrap_or(true) {
                best = Some(PalwQwen25GeometryV1 { n_ctx: lo, tile_len: tile, ..model });
            }
        }
        tile *= 2;
    }
    best
}

/// The tensor names this graph consumes. `{layer}` is substituted with the layer index.
///
/// Compare against the measured safetensors table: the norm gains are ABSENT (G1 folds them), the
/// q/k/v biases are absent as tensors and present as requantization zero points (G2), and there
/// **This family's head tensor: the embedding table itself.**
///
/// `tie_word_embeddings` is true for every member, so there is no `output.weight` and the lm_head
/// reads `token_embd.weight`. It is the one name the post IR leaves to the class.
pub const QWEN25_HEAD_TENSOR: &str = "token_embd.weight";

/// is no `output.weight` because `tie_word_embeddings` is true — the lm_head reads the embedding
/// table. The `.requant` entries are the per-channel `(multiplier, shift, zero)` triples.
/// **Projected from the IR** (ADR-0049 Decision F), with this family's head tensor.
///
/// It was a hand-written list of 17 names beside a hand-written graph, and the graph it was
/// supposed to describe reads 27 — it omitted every narrowing (`qk_to_code.requant`,
/// `code_product.requant`, `rope_clamp.requant`, the two norm requants, the residual requants and
/// scales) exactly as the hand-written node table omitted every narrowing node. An inventory that
/// does not list a tensor the graph reads is an operand the court cannot open, and a step that
/// cannot be adjudicated.
pub fn qwen25_tensor_names_v1() -> Vec<&'static str> {
    // Tied embeddings: the head reads the embedding table, which is already first in the list, so
    // naming it here dedups to nothing and no `output.weight` is declared.
    crate::palw_base0_profile::base0_tensor_names_for_head_v1(QWEN25_HEAD_TENSOR)
}

/// Qwen2.5's graph, for `geometry`.
///
/// Twenty nodes per layer, and the order IS the execution order. `input_refs` names which
/// committed material each step is recomputed from — without it a challenger could open unrelated
/// tiles as "the inputs" and manufacture a conviction.
///
/// The two cache-role nodes are the ROTATED k, not the raw projection: RoPE is applied before the
/// key enters the cache, so a later position's attention must read the rotated value. That is
/// what the roles select, and getting it backwards would have the court recompute attention
/// against unrotated keys and convict every honest producer.
/// **The A16 tier's profile for a dense Qwen2.5 — the engine that actually works, registered.**
///
/// [`qwen25_profile_v1`] describes `engine.rs`, whose seven-bit activations are where static PTQ of
/// this checkpoint degenerates (ADR-0053). This describes `engine_a16.rs`, where Qwen2.5-1.5B is
/// FAITHFUL against its own float reference and chats. The difference is not a tuning knob: they
/// are different graphs, so they are different classes with different ids, and only one of them is
/// worth a network's cadence.
///
/// Three things it does that the W8A8 profile cannot:
/// * projects [`crate::palw_base0_profile::QWEN25_A16_LAYER_IR`] — the A16 engine's own
///   twenty-seven step order — through the same projector the floor uses, so the declared graph
///   cannot drift from the performed one (the 842-disagreement defect, structurally excluded);
/// * commits the **tiled** logits scheme, which a 151,936-lane vocabulary requires: one flat pin
///   row is 607,744 bytes against a carrier that holds 81,920;
/// * budgets its tiles PER NODE from the row each step reduces over, so an opening is bounded by
///   what a transaction can carry rather than by one number chosen for the whole graph.
/// **The A16 class as testnet-11 registers it.** Kept exactly as it was, including the two defects
/// `qwen25_a16_profile_v2` exists to correct — a class is its id, and repairing this one in place
/// would silently change what a network is running.
pub fn qwen25_a16_profile_v1(geometry: PalwQwen25GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    qwen25_a16_profile_inner(
        geometry,
        crate::palw_base0_profile::QWEN25_A16_PRE_IR,
        crate::palw_state_chunk_map::integer_kv_state_chunk_map_id_v1(),
        QWEN25_HEAD_TENSOR,
        false,
    )
}

/// **The same class with both measured defects corrected — a DIFFERENT class, deliberately.**
///
/// 1. the pre table names the embed-lift requant the engine performs (ADR-0049 Decision F);
/// 2. `state_chunk_map_id` is the four-byte map, which is the width `A16Cache` actually holds.
///
/// Either change moves `shape_profile_id`, and that id IS the class id, so this cannot be shipped
/// as a repair to a registered class: it is a class to register. What it buys is the thing the v1
/// class cannot do — a free-prompt run on a real language model whose commitment a court can
/// recompute.
pub fn qwen25_a16_profile_v2(geometry: PalwQwen25GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    qwen25_a16_profile_inner(
        geometry,
        crate::palw_base0_profile::QWEN25_A16_PRE_IR_V2,
        crate::palw_state_chunk_map::integer_kv_state_chunk_map_id_v2(),
        QWEN25_A16_HEAD_TENSOR_V2,
        false,
    )
}

/// **`graph-v5`: the dense tier with ONE fused attention node per layer** (ADR-0082 Decision 1).
///
/// The v2 graph in every respect but the attention site, where `ATTN_SCORES`, the row `SoftMax`,
/// the probability requantization and `ATTN_VALUES` — three of them committing `attn_heads × kv_len`
/// rows at every position — become one [`crate::palw_step::PalwStepOpKindV1::AttnFused`] node whose
/// committed row is the OUTPUT and nothing else. The scores, the row max, the exponent sum and the
/// probabilities are internal to the op: computed in whatever order an executor likes, never
/// committed, never carried, and refuted by a dissection over the history rather than by opening
/// the row (ADR-0082 Decision 2).
///
/// What it buys, measured against v2's own numbers (§1.2–1.3): no committed row of this class has a
/// context-shaped width, so the close stops growing with `n_ctx`, and an attention site costs
/// `⌈heads × d_head / tile_len⌉` leaves a position at EVERY context instead of a count linear in
/// the position — which returns the job's leaf count to the base count ADR-0077 Decision 12 was
/// sized against.
///
/// A class IS its graph (ADR-0049 Decision F), so this is a NEW class id, registered through
/// ADR-0075's route or minted at a relaunch. The v1 and v2 rows are untouched and stay exactly as
/// narrow as they are — they are live chain facts.
///
/// The artifact is UNCHANGED: the fused node reads the same four registered tensors the four v2
/// nodes read (`attn_logits.a16`, `attn_probs.a16`, `attn_values.a16` and `attn_softmax_up`), which
/// is why no re-conversion and no new inventory is implied — see
/// [`crate::palw_step_refute::palw_attn_fused_tensors_v1`].
pub fn qwen25_a16_profile_v5(geometry: PalwQwen25GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    qwen25_a16_profile_inner(
        geometry,
        crate::palw_base0_profile::QWEN25_A16_PRE_IR_V2,
        crate::palw_state_chunk_map::integer_kv_state_chunk_map_id_v2(),
        QWEN25_A16_HEAD_TENSOR_V2,
        true,
    )
}

/// **The `graph-v5` row over the epsilon the artifact executes** — the pairing any v5 row that has
/// to be SERVED must be built from, exactly as [`qwen25_a16_artifact_row_profile_v1`] is for v2.
pub fn qwen25_a16_artifact_row_profile_v5(geometry: PalwQwen25GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    qwen25_a16_profile_v5(qwen25_geometry_artifact_eps(geometry))
}

/// **The epsilon every dense artifact of this lineage actually executes.**
///
/// The hybrid twin is [`crate::palw_qwen36_profile::QWEN36_ARTIFACT_EPS_Q`], and this is the same
/// defect on the dense side, found the same way: by driving a REGISTERED row through
/// [`Qwen25A16Backend::from_registered_profile`] rather than through `::new`.
///
/// [`QWEN25_1_5B`] declares `rms_eps_q: 1`. `qwen25-convert` writes `eps_q: 1 << 8` into every
/// artifact header (`Base0ShapeV1::eps_q`, the value `misaka-palw-base0::classes` also spells for
/// this family's `artifact_shape`: "the A16 engine norms at the shipped 1 << 8"), and the engine
/// norms with the ARTIFACT's constant. So the declared epsilon is not the executed one, and
/// `A16Engine::plan_from_profile`'s geometry gate refuses the row over its own class's weights:
/// `GeometryMismatch { what: "rms_eps_q", profile: 1, artifact: 256 }`. The shipped worker never
/// saw it because it takes `Qwen25A16Backend::new`, which compiles no plan and lets the artifact's
/// epsilon execute — the asymmetry is exactly why nobody noticed.
///
/// **This constant does not move a registered class.** `QWEN25_1_5B` and [`QWEN25_1_5B_A16`] stay
/// exactly as testnet-11's genesis registered them (`params.rs` derives the registration from
/// [`qwen25_a16_registration_v2`], and a moved `rms_eps_q` would be a moved class id and a moved
/// consensus fingerprint). What declares the executed epsilon is the row that is not registered
/// yet: [`qwen25_a16_artifact_row_profile_v1`], which the ADR-0080 context ladder carries. Closing
/// the registered row means REGISTERING a corrected one, which is the integrator's cut, not a
/// repair that can be shipped under an existing id.
pub const QWEN25_A16_ARTIFACT_EPS_Q: i64 = 1 << 8;

/// A dense geometry with its epsilon corrected to [`QWEN25_A16_ARTIFACT_EPS_Q`].
///
/// A field update on the SAME const rather than a second hand-kept table — the exact shape of
/// [`crate::palw_qwen36_profile::qwen36_geometry_artifact_eps`] — so the corrected geometry cannot
/// drift from the frozen one in any other field.
pub const fn qwen25_geometry_artifact_eps(g: PalwQwen25GeometryV1) -> PalwQwen25GeometryV1 {
    PalwQwen25GeometryV1 { rms_eps_q: QWEN25_A16_ARTIFACT_EPS_Q, ..g }
}

/// **The corrected graph over the epsilon the artifact executes** — the projection any row that
/// has to be SERVED (not merely declared) must be built from.
///
/// [`qwen25_a16_profile_v2`] over [`qwen25_geometry_artifact_eps`]. A caller that reaches for
/// `qwen25_a16_profile_v2` directly is declaring an epsilon nothing runs; a caller that reaches
/// for this one gets a profile `from_registered_profile` can compile a plan for.
pub fn qwen25_a16_artifact_row_profile_v1(geometry: PalwQwen25GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    qwen25_a16_profile_v2(qwen25_geometry_artifact_eps(geometry))
}

/// **The v2 class's head node names the ENGINE's head view, not the embedding table.**
///
/// The v1 spelling (`token_embd.weight`, this family's tied head) puts two row SHAPES under one
/// inventory name: the gather's one-row-per-token and the head matmul's one-tile-of-rows. Their
/// byte offsets collide — both start at zero — and `find_operand_v1` resolves `(name, layer,
/// offset)` before it checks length, so whichever view the canonical inventory lists first makes
/// the other structurally unservable and its steps `Unadjudicable`. The floor's inventory solved
/// this years of commits ago by emitting BOTH views under two names ("a tied class is a size
/// question, never an adjudicability one"); the v2 class adopts the same spelling. Tying stays a
/// fact about bytes: the artifact's `unembed` equals its `embed` when tied.
pub const QWEN25_A16_HEAD_TENSOR_V2: &str = "output.weight";

fn qwen25_a16_profile_inner(
    geometry: PalwQwen25GeometryV1,
    pre_ir: &'static [crate::palw_base0_profile::Base0IrNodeV1],
    state_chunk_map_id: Hash64,
    head: &'static str,
    // **ADR-0082 Decision 1.** Graph v5 fuses the layer table's four attention nodes into one
    // `AttnFused` node after the per-node tile budget has run, so the fused node inherits the
    // budgeted tile of the row it commits and the site's leaf count per position is exactly what
    // `ATTN_VALUES` costs today. `false` is every shipped row, which is why their ids cannot move.
    fuse_attention: bool,
) -> Result<PalwShapeProfileV3, PalwStepError> {
    use crate::palw_base0_profile::{Base0IrGeometryV1, Base0IrScopeV1, QWEN25_A16_LAYER_IR, QWEN25_A16_POST_IR, base0_ir_nodes_v1};

    let ir_geometry = |tile: u32| Base0IrGeometryV1 {
        vocab_size: geometry.vocab_size,
        layer_count: geometry.layer_count,
        hidden_dim: geometry.hidden_dim,
        ffn_dim: geometry.ffn_dim,
        attn_heads: geometry.attn_heads,
        attn_kv_heads: geometry.attn_kv_heads,
        attn_head_dim: geometry.attn_head_dim,
        tile_len: tile,
        weight_dtype: QWEN25_WEIGHT_DTYPE_I8,
    };
    // **The per-node tile, budgeted from the reduction** — the same rule the hybrid class uses and
    // for the same reason: a step's Decision-B opening is `tile x in_w` bytes and the whole close
    // must ride one carrier, so one tile for every node is the U-shape where the number that fits
    // the fat matmuls explodes the narrow nodes' leaf counts.
    let budget = |nodes: &mut Vec<PalwStepNodeV1>, table: &[crate::palw_base0_profile::Base0IrNodeV1]| {
        for (node, ir) in nodes.iter_mut().zip(table) {
            let in_w = ir
                .inputs
                .first()
                .map(|i| match i {
                    crate::palw_base0_profile::Base0IrInputV1::LayerIn => geometry.hidden_dim as usize,
                    crate::palw_base0_profile::Base0IrInputV1::CachedK | crate::palw_base0_profile::Base0IrInputV1::CachedV => {
                        (geometry.attn_kv_heads as usize) * (geometry.attn_head_dim as usize) * (geometry.n_ctx as usize)
                    }
                    crate::palw_base0_profile::Base0IrInputV1::Step(prev) => match table.get(*prev as usize).map(|p| p.out) {
                        Some(crate::palw_base0_profile::Base0IrWidthV1::FfnDim) => geometry.ffn_dim as usize,
                        Some(crate::palw_base0_profile::Base0IrWidthV1::KvDim) => {
                            (geometry.attn_kv_heads as usize) * (geometry.attn_head_dim as usize)
                        }
                        Some(crate::palw_base0_profile::Base0IrWidthV1::Vocab) => geometry.vocab_size as usize,
                        _ => geometry.hidden_dim as usize,
                    },
                })
                .unwrap_or(geometry.hidden_dim as usize);
            let out_elems = match node.out_len {
                PalwStepOutLenV1::Fixed { elements } => elements as usize,
                PalwStepOutLenV1::KvScaled { .. } => usize::MAX,
            };
            let chosen = if node.op_kind == PalwStepOpKindV1::MatMulQuant {
                (QWEN25_A16_MATMUL_OPENING_BUDGET / in_w.max(1))
                    .next_power_of_two()
                    .checked_shr(1)
                    .unwrap_or(1)
                    .clamp(crate::palw_step::PALW_STEP_MIN_TILE_LEN as usize, geometry.tile_len as usize)
            } else {
                geometry.tile_len as usize
            };
            node.tile_len = chosen.min(out_elems.max(crate::palw_step::PALW_STEP_MIN_TILE_LEN as usize)) as u32;
        }
    };

    let mut pre_nodes = base0_ir_nodes_v1(pre_ir, ir_geometry(geometry.tile_len), Base0IrScopeV1::Graph, head);
    budget(&mut pre_nodes, pre_ir);
    let mut attn_nodes = base0_ir_nodes_v1(QWEN25_A16_LAYER_IR, ir_geometry(geometry.tile_len), Base0IrScopeV1::PerLayer, "");
    budget(&mut attn_nodes, QWEN25_A16_LAYER_IR);
    // **ADR-0082 Decision 1**, applied to the PROJECTED table and never to a second IR const: one
    // description of the fusion, read by both families (`palw_fuse_attention_site_v5`).
    if fuse_attention {
        attn_nodes = crate::palw_base0_profile::palw_fuse_attention_site_v5(&attn_nodes)?;
    }
    let mut post_nodes = base0_ir_nodes_v1(QWEN25_A16_POST_IR, ir_geometry(geometry.tile_len), Base0IrScopeV1::Graph, head);
    budget(&mut post_nodes, QWEN25_A16_POST_IR);

    let profile = PalwShapeProfileV3 {
        version: PALW_STEP_OBJECT_VERSION_V1,
        lane: PalwStepLaneV1::Int32,
        layer_count: geometry.layer_count,
        full_attention_interval: 1,
        hidden_dim: geometry.hidden_dim,
        ffn_dim: geometry.ffn_dim,
        attn_heads: geometry.attn_heads,
        attn_kv_heads: geometry.attn_kv_heads,
        attn_head_dim: geometry.attn_head_dim,
        rope_dims: geometry.attn_head_dim as u16,
        rope_sections: [0, 0, 0, 0],
        rope_freq_base_bits: 0,
        rms_eps_bits: 0,
        l2_eps_bits: 0,
        base0_rms_eps_q: geometry.rms_eps_q,
        // TILED — and this class can produce it: the A16 producer commits
        // `tiled_logits_trace_root_v1`, so the scheme it registers is the scheme its executor
        // builds. (The floor stays flat for exactly the opposite reason.)
        logits_scheme_id: crate::palw_step_refute::tiled_logits_scheme_id_v1(),
        gdn_heads: 0,
        gdn_head_k_dim: 0,
        gdn_head_v_dim: 0,
        gdn_conv_kernel: 0,
        vocab_size: geometry.vocab_size,
        repack_on: 0,
        llamafile_on: 0,
        flash_attn_disabled: 1,
        fused_gdn_on: 0,
        use_ref_off: 0,
        kv_cache_f16: 0,
        n_ctx: geometry.n_ctx,
        n_batch: geometry.n_ctx,
        n_ubatch: geometry.n_ctx,
        n_seq: 1,
        n_threads: geometry.n_threads,
        pre_nodes,
        gdn_nodes: Vec::new(),
        attn_nodes,
        post_nodes,
        reference_ruleset_id: crate::palw_reference::reference_arithmetic_ruleset_id_v2(),
        transcendental_bindings: Vec::new(),
        contraction_facts: Vec::new(),
        // **From the caller, because the two versions of this class differ in exactly this.** The
        // v1 class declares the one-byte map over an `i32` cache — the comment that used to sit
        // here reasoned that the map describes the cache's element type, which is right, and then
        // picked the map for `i8`.
        kv_chunk_calls: 0,
        state_chunk_map_id,
    };
    profile.validate_shape()?;
    Ok(profile)
}

/// **The A16 dense class as testnet registers it** — the geometry the court's own ceilings chose,
/// not a preference.
///
/// `n_ctx` 16 is the widest context whose worst close stays inside the carrier (96 % of 81,920);
/// 24 is 112 % and 32 is 148 %, and past 32 the step space leaves the ladder entirely. The binding
/// node is the SwiGLU's down projection, whose 8,960-lane reduction opens 35,840 bytes of weights
/// at the tile floor — the same arithmetic that set the floor.
///
/// The canonical job is (14, 2): footprint `14 + 2 − 1 = 15`, one inside the context, so the class
/// is priced at a job it can actually declare.
pub const QWEN25_1_5B_A16: PalwQwen25GeometryV1 = PalwQwen25GeometryV1 { n_ctx: 16, ..QWEN25_1_5B };
pub const QWEN25_A16_CANONICAL: (u32, u32) = (14, 2);

/// The class id testnet-11 registers for the A16 dense tier, derived from its own profile.
pub fn qwen25_a16_class_id_v1() -> Hash64 {
    qwen25_a16_profile_v1(QWEN25_1_5B_A16).expect("the registered A16 geometry projects").shape_profile_id()
}

/// **Everything a chain needs to carry the A16 dense class** — profile, catalog entry and the
/// genesis-form registration, from one geometry so no two can disagree.
pub fn qwen25_a16_registration_v1(
    artifact_root: Hash64,
    share_permille: u16,
    slash_value_per_pwu: u64,
    initial_target: u128,
) -> Result<
    (PalwShapeProfileV3, crate::palw_mode_v2::PalwClassCatalogEntryV2, crate::palw_state_v2::PalwConsensusObjectV2),
    PalwStepError,
> {
    let profile = qwen25_a16_profile_v1(QWEN25_1_5B_A16)?;
    let class_id = profile.shape_profile_id();
    let canonical = crate::palw_base0_profile::rc_job_context(&profile, QWEN25_A16_CANONICAL.0, QWEN25_A16_CANONICAL.1);
    let counted = crate::palw_step::step_leaf_count(&profile, &canonical)?;
    let entry = crate::palw_mode_v2::PalwClassCatalogEntryV2 {
        class_id,
        artifact_root,
        max_step_leaf_count: crate::palw_step::worst_case_step_leaf_count_v1(&profile)?,
        canonical_step_leaf_count: counted,
        reachable_kernels: [&profile.pre_nodes, &profile.attn_nodes, &profile.post_nodes]
            .into_iter()
            .flatten()
            .map(|n| n.kernel_semantics_id)
            .collect(),
        court_cost: crate::palw_class_admission_v2::derive_court_cost_v1(&profile)
            .map_err(|_| PalwStepError::ProfileNotCanonical("the A16 dense class's court cost does not derive"))?,
    };
    let object = crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target,
        share_permille,
        activation_daa: 0,
        admission: None,
    };
    Ok((profile, entry, object))
}

/// The CORRECTED class's id — the court-capable one: the four-byte state map, the embed-lift
/// requant named, the head under the engine's own view.
pub fn qwen25_a16_class_id_v2() -> Hash64 {
    qwen25_a16_profile_v2(QWEN25_1_5B_A16).expect("the corrected A16 geometry projects").shape_profile_id()
}

/// **The registration a court-capable A16 tier files** — [`qwen25_a16_profile_v2`]'s class, with
/// the same economics derivation as the v1 constructor beside it. Two obligations distinguish it:
/// `artifact_root` must be the class's INVENTORY root (`a16_inventory_v1` in the producer crate) —
/// a flat digest can say "same bytes" but a close's operand openings prove against this value and
/// nothing can be opened against a flat hash — and the producer that registers it must commit the
/// step binding's own execution root, which is what its captured attempt path does.
pub fn qwen25_a16_registration_v2(
    artifact_root: Hash64,
    share_permille: u16,
    slash_value_per_pwu: u64,
    initial_target: u128,
) -> Result<
    (PalwShapeProfileV3, crate::palw_mode_v2::PalwClassCatalogEntryV2, crate::palw_state_v2::PalwConsensusObjectV2),
    PalwStepError,
> {
    let profile = qwen25_a16_profile_v2(QWEN25_1_5B_A16)?;
    let class_id = profile.shape_profile_id();
    let canonical = crate::palw_base0_profile::rc_job_context(&profile, QWEN25_A16_CANONICAL.0, QWEN25_A16_CANONICAL.1);
    let counted = crate::palw_step::step_leaf_count(&profile, &canonical)?;
    let entry = crate::palw_mode_v2::PalwClassCatalogEntryV2 {
        class_id,
        artifact_root,
        max_step_leaf_count: crate::palw_step::worst_case_step_leaf_count_v1(&profile)?,
        canonical_step_leaf_count: counted,
        reachable_kernels: [&profile.pre_nodes, &profile.attn_nodes, &profile.post_nodes]
            .into_iter()
            .flatten()
            .map(|n| n.kernel_semantics_id)
            .collect(),
        court_cost: crate::palw_class_admission_v2::derive_court_cost_v1(&profile)
            .map_err(|_| PalwStepError::ProfileNotCanonical("the corrected A16 class's court cost does not derive"))?,
    };
    let object = crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target,
        share_permille,
        activation_daa: 0,
        admission: None,
    };
    Ok((profile, entry, object))
}

/// The A16 dense class's opening budget — 24 KiB, the same share of the carrier the hybrid class
/// reserves for weights so that the evidence beside them still fits.
const QWEN25_A16_MATMUL_OPENING_BUDGET: usize = 24 * 1024;

pub fn qwen25_profile_v1(geometry: PalwQwen25GeometryV1) -> Result<PalwShapeProfileV3, PalwStepError> {
    let tile = geometry.tile_len;
    let hidden = geometry.hidden_dim;
    let q_dim = geometry.attn_heads as u32 * geometry.attn_head_dim;
    let kv_dim = geometry.attn_kv_heads as u32 * geometry.attn_head_dim;

    // **No node is written here** (ADR-0049 Decision F): every table below is projected from the
    // IR, so the hand builders that used to stand here have nothing left to build.

    let ir_geometry = crate::palw_base0_profile::Base0IrGeometryV1 {
        layer_count: geometry.layer_count,
        hidden_dim: hidden,
        ffn_dim: geometry.ffn_dim,
        attn_heads: geometry.attn_heads,
        attn_kv_heads: geometry.attn_kv_heads,
        attn_head_dim: geometry.attn_head_dim,
        tile_len: tile,
        vocab_size: geometry.vocab_size,
        weight_dtype: QWEN25_WEIGHT_DTYPE_I8,
    };

    // **Projected too, at this family's head tensor** (ADR-0049 Decision F). One node, and it was
    // still a second description of one step — as the post table below proves a small table drifts
    // exactly like a large one.
    let pre_nodes = crate::palw_base0_profile::base0_ir_nodes_v1(
        crate::palw_base0_profile::BASE0_PRE_IR,
        ir_geometry,
        crate::palw_base0_profile::Base0IrScopeV1::Graph,
        QWEN25_HEAD_TENSOR,
    );

    // **Projected from `BASE0_LAYER_IR`, the engine's own step order** (ADR-0049 Decision F).
    //
    // This table was written here by hand, beside an engine written by hand, and the two were not
    // the same graph: it declared **27** nodes against the engine's **38**, and of the rows that
    // did land the widths diverged from the third node onward — the 1536/256 pairs traded places
    // (the grouped-query boundary sitting in different positions in the two descriptions) and the
    // FFN's 8960 arrived where the residual width was expected. Measured with
    // `qwen25-convert --check-capture`: **842 disagreements over 1068 captured rows**, so no
    // execution of this class could become a step leg, and the class could not produce a block.
    //
    // It was invisible because the only thing ever run against the real checkpoint was a FORWARD
    // PASS (`measure_depth_health`). The step capture is a different path. A model can run
    // perfectly and still be unable to commit to what it ran.
    //
    // The floor was already projected from the IR; the projection just could not express
    // grouped-query attention, because it read `PalwBase0GeometryV1`, which has no kv-head count.
    // `Base0IrGeometryV1` supplies one, and both classes now come from the same call.
    let attn_nodes = crate::palw_base0_profile::base0_ir_attn_nodes_v1(ir_geometry);
    debug_assert_eq!(kv_dim, geometry.attn_kv_heads as u32 * geometry.attn_head_dim);
    debug_assert_eq!(q_dim, hidden, "the query width is the hidden width for every member of this family");

    // The head, and it is THREE steps, not two. The final norm is `rms_norm` followed by the
    // narrowing back to activation codes; this table declared the first and not the second — the
    // same omission the floor's post table carried, in a table written twice. Projected now, from
    // `BASE0_POST_IR`, so the two classes cannot differ about what the head is.
    let post_nodes = crate::palw_base0_profile::base0_ir_nodes_v1(
        crate::palw_base0_profile::BASE0_POST_IR,
        ir_geometry,
        crate::palw_base0_profile::Base0IrScopeV1::Graph,
        QWEN25_HEAD_TENSOR,
    );

    let profile = PalwShapeProfileV3 {
        version: PALW_STEP_OBJECT_VERSION_V1,
        lane: PalwStepLaneV1::Int32,
        layer_count: geometry.layer_count,
        full_attention_interval: 1,
        hidden_dim: hidden,
        ffn_dim: geometry.ffn_dim,
        attn_heads: geometry.attn_heads,
        attn_kv_heads: geometry.attn_kv_heads,
        attn_head_dim: geometry.attn_head_dim,
        rope_dims: geometry.attn_head_dim as u16,
        rope_sections: [0, 0, 0, 0],
        // Every float constant is zero and every float table is empty, and each is a property of
        // the class rather than an unfilled field: the rotary is a pinned integer table, the norm
        // epsilon is the integer `rms_eps_q`, no cache holds floats, and integer addition is
        // exactly associative so there is no FMA contraction to pin (ADR-0040 Decision E).
        rope_freq_base_bits: 0,
        rms_eps_bits: 0,
        l2_eps_bits: 0,
        base0_rms_eps_q: geometry.rms_eps_q,
        // FLAT, because that is what this class's executor commits TODAY — and stated honestly:
        // at vocab 151,936 the flat close exceeds the carrier ceiling, so this W8A8 class cannot
        // pass the cost gate. The tier that can is `qwen25_a16_profile_v1`, which commits tiled.
        logits_scheme_id: crate::palw_step_refute::flat_logits_scheme_id_v1(),
        gdn_heads: 0,
        gdn_head_k_dim: 0,
        gdn_head_v_dim: 0,
        gdn_conv_kernel: 0,
        vocab_size: geometry.vocab_size,
        repack_on: 0,
        llamafile_on: 0,
        flash_attn_disabled: 1,
        fused_gdn_on: 0,
        use_ref_off: 0,
        kv_cache_f16: 0,
        n_ctx: geometry.n_ctx,
        n_batch: geometry.n_ctx,
        n_ubatch: geometry.n_ctx,
        n_seq: 1,
        n_threads: geometry.n_threads,
        pre_nodes,
        gdn_nodes: Vec::new(),
        attn_nodes,
        post_nodes,
        reference_ruleset_id: crate::palw_reference::reference_arithmetic_ruleset_id_v2(),
        transcendental_bindings: Vec::new(),
        contraction_facts: Vec::new(),
        kv_chunk_calls: 0,
        // **Registered**, and it is the same id BASE-0 registers: this class inherits BASE-0's
        // arithmetic and its engine, so its replay state is the same int8 KV rows under the same
        // derived layout (`palw_state_chunk_map`). A second id for one layout would be a second
        // thing to keep in sync — the defect this class was born from (`rms_eps_q`).
        state_chunk_map_id: crate::palw_state_chunk_map::integer_kv_state_chunk_map_id_v1(),
    };
    profile.validate_shape()?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    /// **The A16 dense class passes the whole admission gate.** Not a cost check: the same
    /// `verify_class_admission_v2` a post-genesis registration answers to — shape validation, both
    /// coverage gates, the ladder, the three court-cost ceilings and the PWU recount.
    ///
    /// This is the test the W8A8 Qwen2.5 class could never pass, and the reason is not tuning: at
    /// vocabulary 151,936 its FLAT pin alone is 607,744 bytes against an 81,920-byte carrier, so
    /// no (tile, context) pair existed. The A16 class commits the tiled scheme, which its own
    /// producer builds.
    #[test]
    fn the_a16_dense_class_passes_the_admission_gate() {
        use crate::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
        let (profile, entry, object) = qwen25_a16_registration_v1(Hash64::from_u64_word(0xA16), 1, 1, 1).expect("derives");
        assert_eq!(entry.class_id, qwen25_a16_class_id_v1(), "the entry is the registered class");
        assert_eq!(profile.attn_nodes.len(), crate::palw_base0_profile::QWEN25_A16_LAYER_IR.len());
        assert_eq!(
            profile.logits_scheme_id,
            crate::palw_step_refute::tiled_logits_scheme_id_v1(),
            "a 151,936-lane vocabulary cannot commit flat and be prosecutable"
        );

        let catalog = crate::palw_mode_v2::PalwClassCatalogV2::new(vec![entry.clone()]).expect("well-formed");
        let mut bundle = crate::palw_fp_devnet_v3::palw_fp_devnet_bundle_v3(
            entry.class_id,
            catalog.root(),
            crate::palw_catalog_coverage::palw_court_catalog_root_v1(),
            entry.canonical_step_leaf_count,
            entry.artifact_root,
            Vec::new(),
        )
        .expect("a bundle for this class assembles");
        bundle.court = crate::palw_mode_v2::PalwCourtParamsV2::new(crate::palw_step::PALW_STEP_MAX_LEAVES, 20, 2)
            .expect("the full ladder is a legal court");
        let canonical = crate::palw_base0_profile::rc_job_context(&profile, QWEN25_A16_CANONICAL.0, QWEN25_A16_CANONICAL.1);
        // ADR-0069 Decision 5, satisfied rather than exercised — this test is about the court
        // cost the gate derives, not about who may hold weight.
        let certified = crate::palw_e2e_adjudicability::catalog_covering_family_for_tests_v1();
        bundle.court_e2e_root = crate::palw_e2e_adjudicability::palw_court_e2e_root_of_v1(&certified);
        let admitted = crate::palw_class_admission_v2::verify_class_admission_v2(&bundle, &profile, &canonical, &object, &certified)
            .expect("the A16 dense class is admissible on a network with the shipped ceilings");
        assert_eq!(admitted.court_cost, entry.court_cost, "the gate and the mint derive one cost");
        assert!(
            admitted.court_cost.max_close_bytes <= bundle.court.max_close_bytes(),
            "close {} against ceiling {}",
            admitted.court_cost.max_close_bytes,
            bundle.court.max_close_bytes()
        );
        let PalwConsensusObjectV2::ClassRegistered { pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference }, .. } = object else {
            panic!("a derived registration");
        };
        assert_eq!(pwu_per_inference, entry.canonical_step_leaf_count, "the declared pwu is the counted one");

        // The canonical job fits the registered context in the enumeration's own form.
        let footprint = QWEN25_A16_CANONICAL.0 + QWEN25_A16_CANONICAL.1 - 1;
        assert!(footprint <= profile.n_ctx, "canonical footprint {footprint} inside n_ctx {}", profile.n_ctx);
    }

    /// **The CORRECTED class passes the same whole gate** — the admission the court-capable tier
    /// answers to at registration, at the real 1.5B geometry. Beside the v1 test on purpose: the
    /// corrections (the four-byte map, the named embed-lift requant, the head under the engine's
    /// own view) each move the id and each could in principle have moved a cost past a ceiling,
    /// and "the corrected class is registrable" is a claim this file must be able to make with a
    /// test rather than a comment.
    #[test]
    fn the_corrected_a16_class_passes_the_full_admission_gate() {
        use crate::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
        let (profile, entry, object) = qwen25_a16_registration_v2(Hash64::from_u64_word(0xA162), 1, 1, 1).expect("derives");
        assert_eq!(entry.class_id, qwen25_a16_class_id_v2());
        assert_ne!(entry.class_id, qwen25_a16_class_id_v1(), "a correction is a different class, never a repair in place");
        assert_eq!(
            profile.state_chunk_map_id,
            crate::palw_state_chunk_map::integer_kv_state_chunk_map_id_v2(),
            "the map the i32 cache actually has"
        );
        assert_eq!(profile.pre_nodes.len(), 2, "the embed-lift requant is named");
        assert_eq!(profile.post_nodes.last().expect("a head").weight_name, QWEN25_A16_HEAD_TENSOR_V2);

        let catalog = crate::palw_mode_v2::PalwClassCatalogV2::new(vec![entry.clone()]).expect("well-formed");
        let mut bundle = crate::palw_fp_devnet_v3::palw_fp_devnet_bundle_v3(
            entry.class_id,
            catalog.root(),
            crate::palw_catalog_coverage::palw_court_catalog_root_v1(),
            entry.canonical_step_leaf_count,
            entry.artifact_root,
            Vec::new(),
        )
        .expect("a bundle for this class assembles");
        bundle.court = crate::palw_mode_v2::PalwCourtParamsV2::new(crate::palw_step::PALW_STEP_MAX_LEAVES, 20, 2)
            .expect("the full ladder is a legal court");
        let canonical = crate::palw_base0_profile::rc_job_context(&profile, QWEN25_A16_CANONICAL.0, QWEN25_A16_CANONICAL.1);
        // ADR-0069 Decision 5, satisfied rather than exercised — this test is about the corrected
        // graph's court cost, not about who may hold weight.
        let certified = crate::palw_e2e_adjudicability::catalog_covering_family_for_tests_v1();
        bundle.court_e2e_root = crate::palw_e2e_adjudicability::palw_court_e2e_root_of_v1(&certified);
        let admitted = crate::palw_class_admission_v2::verify_class_admission_v2(&bundle, &profile, &canonical, &object, &certified)
            .expect("the corrected A16 class is admissible on a network with the shipped ceilings");
        assert_eq!(admitted.court_cost, entry.court_cost, "the gate and the mint derive one cost");
        assert!(admitted.court_cost.max_close_bytes <= bundle.court.max_close_bytes());
        let PalwConsensusObjectV2::ClassRegistered { pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference }, .. } = object else {
            panic!("a derived registration");
        };
        assert_eq!(pwu_per_inference, entry.canonical_step_leaf_count);
    }
    use super::*;
    // Test-only: the lib target does not name either of these, and an import that is unused
    // outside `cfg(test)` is a warning clippy denies.
    use crate::Hash64;
    use crate::palw_catalog_coverage::verify_profile_coverage_v1;
    use crate::palw_step::PalwStepTableV1;
    use crate::palw_step::{PalwStepNodeRoleV1, kernel_semantics_id_v1};
    use crate::palw_step_refute::{catalogued_kernel_ids_v1, kernel_can_serve_node_v1};

    /// Diagnostic: the smallest tile_len that admits the declared 4096 context, projected.
    /// `cargo test -p kaspa-consensus-core --lib dump_full_ctx_tile -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_full_ctx_tile() {
        for (name, g) in [("1.5B", QWEN25_1_5B), ("3B", QWEN25_3B)] {
            let mut found = None;
            for tile in [16_384u32, 32_768, 65_536, 131_072, 262_144] {
                let cand = PalwQwen25GeometryV1 { tile_len: tile, ..g };
                if let Ok(p) = qwen25_profile_v1(cand)
                    && crate::palw_step::worst_case_step_leaf_count_v1(&p).is_ok()
                {
                    found = Some(tile);
                    break;
                }
            }
            println!("{name}: smallest tile admitting n_ctx {} is {found:?}", g.n_ctx);
        }
    }

    /// Diagnostic: the node table the engine's capture must land in, slot by slot.
    /// `cargo test -p kaspa-consensus-core --lib dump_qwen_slots -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_qwen_slots() {
        let court = crate::palw_mode_v2::PalwCourtParamsV2::new(crate::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).unwrap();
        let g = qwen25_admissible_geometry_v1(QWEN25_1_5B, &court).unwrap();
        let p = qwen25_profile_v1(g).unwrap();
        println!(
            "tile_len(geometry)={} pre={} attn={} post={} per-layer-attn={}",
            g.tile_len,
            p.pre_nodes.len(),
            p.attn_nodes.len(),
            p.post_nodes.len(),
            p.attn_nodes.len()
        );
        for slot in 0..14u32 {
            if let Some((n, layer)) = p.resolve_node_slot(slot) {
                let w = match n.out_len {
                    crate::palw_step::PalwStepOutLenV1::Fixed { elements } => format!("Fixed {elements}"),
                    crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => format!("KvScaled x{multiplier}"),
                };
                println!("  slot {slot:3} layer {layer:?} {:?} out_len={w} tile_len={}", n.op_kind, n.tile_len);
            }
        }
    }

    /// **Would the chain admit this class?** — asked against the SHIPPED bundle, which is the only
    /// bundle whose answer matters.
    ///
    /// **The answer changed with ADR-0080 design A and the test is named for it.** It was
    /// `the_shipped_court_admits_no_qwen25_geometry_and_says_why`: under a `max_close_bytes` that
    /// counted what a close costs to carry in ONE TRANSACTION there was no admissible
    /// `(tile_len, n_ctx)` pair at all, and the honest form of the test was the refusal and its
    /// reason — a registration attempted with a 1.7 GiB artifact already on four hosts fails
    /// on-chain, which is finding it in the worst place. A 27-chunk group admits a pair, and the
    /// pair is three tokens: the shipped `2^22` ladder is what bounds it now, not the price.
    ///
    /// So both halves are exercised. The pair that EXISTS is pinned, small, and useless on its
    /// own; and the gate's refusal is still run, on the pair the LADDER would allow, because that
    /// is the error a deployer actually reads.
    ///
    /// What has to change for a usable context is still code and not this genesis number: the
    /// ladder (ADR-0077's fence), an openable logits commitment (ADR-0049 Decision E says "O(1) in
    /// vocabulary"; this family's root is a flat hash over every row) and a per-layer slice of the
    /// checkpoint the KV history arrives in.
    #[test]
    fn the_shipped_court_admits_one_three_token_qwen25_geometry_and_says_why() {
        let bundle = match &crate::config::params::palw_rc_shipped_params().palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b.clone(),
            _ => return, // a build whose card is unset ships no bundle; nothing to check against
        };
        // **This asserted `is_none()` until ADR-0080 design A**, and the doc above still records
        // why: under an 80 KiB one-transaction ceiling no `(tile_len, n_ctx)` pair of this family
        // had a prosecutable close, so a registration attempted with a 1.7 GiB artifact already on
        // four hosts would have failed on-chain. A 27-chunk group changes the answer, and the
        // honest form of the test is now the PAIR — printed, pinned, and small, because the
        // shipped `2^22` ladder is what bounds it rather than the price.
        let pair = qwen25_admissible_geometry_v1(QWEN25_1_5B, &bundle.court)
            .expect("the shipped court now admits a pair — if this is None again the ceiling went back");
        println!("shipped-court admissible pair: tile_len={} n_ctx={}", pair.tile_len, pair.n_ctx);
        assert_eq!(
            (pair.tile_len, pair.n_ctx),
            (64, 3),
            "the shipped court's widest Qwen2.5-1.5B pair — three tokens, bounded by the 2^22 ladder and not by the price"
        );

        // The gate's own refusal, on the pair the LADDER would allow, so the error a deployer would
        // actually read is exercised rather than described.
        let ladder_only = crate::palw_mode_v2::PalwCourtParamsV2::with_cost_ceilings(
            bundle.court.max_step_leaf_count(),
            bundle.court.turn_deadline_daa(),
            bundle.court.terminal_rounds(),
            u64::MAX,
            u64::MAX,
            u32::MAX,
        )
        .unwrap();
        let pair = qwen25_admissible_geometry_v1(QWEN25_1_5B, &ladder_only).expect("the ladder admits a pair");
        let profile = qwen25_profile_v1(pair).expect("expressible");
        let canonical = crate::palw_base0_profile::rc_job_context(&profile, 8, 4);
        let registration = crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
            class_id: profile.shape_profile_id(),
            artifact_root: crate::Hash64::from_u64_word(0xA1),
            slash_value_per_pwu: 5,
            // COUNTED from the canonical job, never declared — the gate recounts and refuses a
            // mismatch, which is what makes `pwu_per_inference` a fact rather than a multiplier
            // the registrant picks. (First run of this test declared 1 and was told 366,184.)
            pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 {
                pwu_per_inference: crate::palw_step::step_leaf_count(&profile, &canonical)
                    .expect("the canonical job has a step space"),
            },
            initial_target: u128::MAX / 2,
            share_permille: 1,
            activation_daa: 0,
            admission: None,
        };
        let certified = crate::palw_e2e_adjudicability::catalog_covering_family_for_tests_v1();
        let mut bundle = bundle;
        bundle.court_e2e_root = crate::palw_e2e_adjudicability::palw_court_e2e_root_of_v1(&certified);
        match crate::palw_class_admission_v2::verify_class_admission_v2(&bundle, &profile, &canonical, &registration, &certified) {
            Err(crate::palw_class_admission_v2::PalwClassAdmissionError::CourtCostExceedsCeiling { what, got, ceiling }) => {
                // In CHUNKS since ADR-0080 design A — the unit a close is carried in. The
                // order-of-magnitude clause is unchanged in meaning: the widest pair the ladder
                // admits needs more than ten times the carriers this ruleset pays for.
                assert_eq!(what, "court close chunks");
                assert!(got > ceiling * 10, "the refusal is by an order of magnitude: {got} against {ceiling}");
            }
            other => panic!("the shipped gate must refuse Qwen2.5-1.5B on court cost, got {other:?}"),
        }
    }

    /// **Print the admissible geometry a class must actually register at.** The constants above
    /// are the MODEL's shape; `n_ctx 4096` at `tile_len 128` is far past `PALW_STEP_MAX_LEAVES`,
    /// so a class registered at the model's own numbers is refused. Run when picking the pair:
    /// `cargo test -p kaspa-consensus-core --lib print_admissible -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_admissible_qwen25_geometry() {
        let court =
            crate::palw_mode_v2::PalwCourtParamsV2::new(crate::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("the shipped court");
        for (name, g) in [("1.5B", QWEN25_1_5B), ("3B", QWEN25_3B)] {
            match qwen25_admissible_geometry_v1(g, &court) {
                Some(a) => {
                    let p = qwen25_profile_v1(a).expect("expressible");
                    println!(
                        "{name}: tile_len={} n_ctx={} class_id={} (model declared tile={} n_ctx={})",
                        a.tile_len,
                        a.n_ctx,
                        p.shape_profile_id(),
                        g.tile_len,
                        g.n_ctx
                    );
                }
                None => println!("{name}: NO admissible (tile, n_ctx) under this court"),
            }
        }
    }

    /// **Condition 3: the profile derives from the MEASURED geometry**, for both readings of "2B".
    #[test]
    fn the_profile_derives_from_the_measured_geometry() {
        for (name, g) in [("1.5B", QWEN25_1_5B), ("3B", QWEN25_3B)] {
            let p = qwen25_profile_v1(g).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(p.layer_count, g.layer_count);
            assert_eq!(p.hidden_dim, g.hidden_dim);
            assert_eq!(p.ffn_dim, g.ffn_dim);
            assert_eq!(p.vocab_size, 151_936, "{name}: the family shares one vocabulary");
            // GQA is real and the profile carries both counts: 2 kv heads against 12 or 16.
            assert_eq!(p.attn_kv_heads, 2);
            assert!(p.attn_heads > p.attn_kv_heads, "{name}: grouped-query attention, not multi-head");
            assert_eq!(p.hidden_dim, p.attn_heads as u32 * p.attn_head_dim, "{name}: q width is the hidden width");
            // Every layer is attention: no GatedDeltaNet arm in this architecture.
            assert_eq!(p.table_layer_span(PalwStepTableV1::Attn), g.layer_count as usize);
            assert!(p.gdn_nodes.is_empty());
            assert_eq!(p.lane, PalwStepLaneV1::Int32, "{name}: an integer class commits integer codes");
        }
        // Two geometries are two classes, and the id says so.
        assert_ne!(
            qwen25_profile_v1(QWEN25_1_5B).unwrap().shape_profile_id(),
            qwen25_profile_v1(QWEN25_3B).unwrap().shape_profile_id()
        );
        // …and neither is BASE-0.
        let base0 = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).unwrap();
        assert_ne!(qwen25_profile_v1(QWEN25_1_5B).unwrap().shape_profile_id(), base0.shape_profile_id());
    }

    /// **Condition 4: the coverage gate passes, 100%.**
    ///
    /// Against `catalogued_kernel_ids_v1()` and `kernel_can_serve_node_v1` — the adjudication
    /// table and the adjudicator's own statement of what it can serve — never a restated list.
    /// This is the check a *float* Qwen profile fails: no float quantized matmul is catalogued at
    /// all, which is why this class is integer arithmetic and not llama.cpp's kernels.
    #[test]
    fn the_coverage_gate_passes_on_the_whole_graph() {
        let p = qwen25_profile_v1(QWEN25_1_5B).unwrap();
        // **Coverage over COORDINATES, not kernel ids** (ADR-0049 Decision D) — and it passes only
        // because Decision E landed with it. The gate swept prefill and decode, the embedding
        // gather refused every decode position, and this class's canonical job decodes; the tripwire
        // here asserted that refusal for exactly as long as it was true. What made it false is that
        // a decode token is now pinned by the claim's own `full_logits_trace_root` — which already
        // bound `output_token_ids_hash_v2` — rather than by nothing.
        verify_profile_coverage_v1(&p).expect("every reachable coordinate class adjudicates, decode included");

        let catalogued = catalogued_kernel_ids_v1();
        let mut checked = 0;
        for (name, nodes) in [("pre", &p.pre_nodes), ("gdn", &p.gdn_nodes), ("attn", &p.attn_nodes), ("post", &p.post_nodes)] {
            for node in nodes {
                assert!(catalogued.contains(&node.kernel_semantics_id), "{name}: {:?} names an uncatalogued kernel", node.op_kind);
                kernel_can_serve_node_v1(node, name == "pre").unwrap_or_else(|e| panic!("{name}: {:?}: {e}", node.op_kind));
                checked += 1;
            }
        }
        // Tracks the IR, because the layer table IS the IR now (ADR-0049 Decision F) — a literal
        // here is how the hand-written table's 27 nodes went unnoticed against the engine's 38.
        assert_eq!(checked, 1 + crate::palw_base0_profile::BASE0_LAYER_IR.len() + 3, "the whole graph was checked, not a prefix");
    }

    /// **The three transformations, asserted as absences.**
    ///
    /// G1, G2 and G3 are exact and applied when the artifact is built, so the way they show up
    /// here is that certain nodes are NOT in the graph. Asserting the absence is what stops one of
    /// them being quietly re-added as an unadjudicable op later.
    #[test]
    fn the_folded_transformations_leave_no_node_behind() {
        let p = qwen25_profile_v1(QWEN25_1_5B).unwrap();
        let names: Vec<&str> =
            [&p.pre_nodes, &p.attn_nodes, &p.post_nodes].into_iter().flatten().map(|n| n.weight_name.as_str()).collect();

        // G1: no norm gain tensor is consumed anywhere — it folded into the following linears.
        assert!(!names.iter().any(|n| n.contains("norm.weight")), "a norm gain node would mean G1 was not folded");
        // G2: no bias tensor either — the biases ride the requantize zero points.
        assert!(!names.iter().any(|n| n.ends_with(".bias")), "a bias tensor would mean G2 was not folded");
        assert!(names.iter().filter(|n| n.ends_with("attn_q.requant")).count() == 1, "q's bias has a home");
        // G3: the rotary is the pinned table, and the ONLY rope kernel is BASE-0's pairwise one.
        for node in &p.attn_nodes {
            if node.op_kind == PalwStepOpKindV1::RopeImrope {
                assert_eq!(node.kernel_semantics_id, kernel_semantics_id_v1(crate::palw_step_refute::KDESC_BASE0_ROPE));
                assert_eq!(node.weight_name, "blk.{layer}.rope_table");
            }
        }
        // Tied embeddings: the lm_head reads the embedding table, and no `output.weight` exists.
        // Found by op kind rather than by index: the post table gained its missing narrowing, and
        // an index would have moved silently.
        let head = p.post_nodes.iter().find(|n| n.op_kind == PalwStepOpKindV1::MatMulQuant).expect("the head is a matmul");
        assert_eq!(head.weight_name, "token_embd.weight", "tie_word_embeddings is true");
        assert!(!names.contains(&"output.weight"));
    }

    /// The cache roles sit on the ROTATED key, not the raw projection — a later position's
    /// attention reads rotated keys, and a court recomputing against unrotated ones would convict
    /// every honest producer.
    #[test]
    fn the_cache_roles_name_the_rotated_key_and_the_requantized_value() {
        let p = qwen25_profile_v1(QWEN25_1_5B).unwrap();
        let k = p.attn_nodes.iter().position(|n| n.role == PalwStepNodeRoleV1::KCacheWrite).expect("a K cache node");
        // The cached key is the rotated one, NARROWED — what a later position reads is an int8
        // code, not the Qk value rope produces, so the role sits on the narrowing that follows the
        // rotation. Asserted through the input chain rather than by node kind: the hand-written
        // table put the role on the raw rope output, and "is it a RopeImrope" was the check that
        // let that pass.
        assert_eq!(p.attn_nodes[k].op_kind, PalwStepOpKindV1::MulElem, "the cached key is a narrowing");
        assert_eq!(p.attn_nodes[k].weight_name, "blk.{layer}.rope_clamp.requant");
        let feeder = p.attn_nodes[k].input_refs[0] as usize;
        assert_eq!(
            p.attn_nodes[feeder].op_kind,
            PalwStepOpKindV1::RopeImrope,
            "and what it narrows is the rotation — an unrotated cached key convicts every honest producer"
        );
        let v = p.attn_nodes.iter().position(|n| n.role == PalwStepNodeRoleV1::VCacheWrite);
        assert!(v.is_none() || p.attn_nodes[v.unwrap()].op_kind != PalwStepOpKindV1::RopeImrope, "no rotation applies to V");
        // Exactly one node per role, or "the K cache" names two things.
        assert_eq!(p.attn_nodes.iter().filter(|n| n.role == PalwStepNodeRoleV1::KCacheWrite).count(), 1);
    }

    /// A Qwen-shaped geometry small enough to commit a whole step leg for.
    ///
    /// The STRUCTURE is the real thing — grouped-query attention with a real group, the cache
    /// role on the rotated key, kv-scaled scores — because that is what conditions 9 and 10 are
    /// about. The dimensions are not: a leg over 28 layers and a 151,936-wide vocabulary is not a
    /// thing a test commits.
    fn probe_geometry() -> PalwQwen25GeometryV1 {
        PalwQwen25GeometryV1 {
            layer_count: 1,
            hidden_dim: 32,
            ffn_dim: 64,
            attn_heads: 4,
            attn_kv_heads: 2,
            attn_head_dim: 8,
            vocab_size: 48,
            n_ctx: 16,
            n_threads: 1,
            rms_eps_q: 1 << 8,
            tile_len: 16,
        }
    }

    fn probe_context(profile: &PalwShapeProfileV3) -> crate::palw_v2::PalwJobContextV2 {
        let mut ctx = crate::palw_v2::PalwJobContextV2 {
            version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"qwen25-probe".to_vec(),
            job_id: Hash64::from_u64_word(1),
            job_nullifier: Hash64::from_u64_word(2),
            assignment_id: Hash64::from_u64_word(3),
            execution_seed: [5; 32],
            model_profile_id: Hash64::from_u64_word(4),
            runtime_manifest_hash: Hash64::from_u64_word(5),
            runtime_class_id: Hash64::from_u64_word(6),
            shape_profile_id: profile.shape_profile_id(),
            trace_scheme_id: Hash64::default(),
            cu_ruleset_id: Hash64::from_u64_word(9),
            tokenizer_id: Hash64::from_u64_word(10),
            prompt_token_ids_hash: Hash64::default(),
            declared_prefill_tokens: 2,
            exact_decode_tokens: 2,
            max_context_tokens: profile.n_ctx,
        };
        ctx.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
        ctx
    }

    /// **Condition 9: a Qwen-shaped execution commits a step trace with a Merkle root.**
    ///
    /// Every leaf is a canonical coordinate of THIS profile, in canonical order, and the builder
    /// refuses anything else — so the root commits to the graph, not merely to some bytes.
    #[test]
    fn a_qwen_execution_commits_a_step_leg() {
        use crate::palw_step::{canonical_step_coordinates, step_leaf_count};
        use crate::palw_step_leg::PalwStepLegBuilderV1;
        let p = qwen25_profile_v1(probe_geometry()).unwrap();
        let ctx = probe_context(&p);

        let expected = step_leaf_count(&p, &ctx).expect("the leaf space is countable");
        assert!(expected > 0);
        let mut builder = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).expect("a Qwen profile builds a leg");
        assert_eq!(builder.expected_main_leaves(), expected, "the builder and the counter agree");

        // Deterministic filler: a court reads ONE step, so the other tiles are opaque bytes to it.
        // What matters here is that every coordinate is canonical and the order is enforced.
        for i in 0..expected {
            let coord = canonical_step_coordinates(&p, &ctx, i).expect("every index is a coordinate");
            let (node, _) = p.resolve_node_slot(coord.node_slot).unwrap();
            let width = builder_tile_width(&p, &ctx, &coord);
            let values: Vec<u32> = (0..width).map(|j| ((i * 7 + j as u64 + 1) % 97) as i32 as u32).collect();
            builder.push_step_tile(coord, &values).unwrap_or_else(|e| panic!("leaf {i} ({:?}): {e:?}", node.op_kind));
        }
        let material = builder.finish().expect("the leg closes");
        assert_eq!(material.leaf_count, expected);
        assert_ne!(material.merkle_root, Hash64::default(), "and it has a root to commit");

        // Out of order is refused, which is what makes the root a commitment to the ORDER too.
        let mut wrong = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).unwrap();
        let second = canonical_step_coordinates(&p, &ctx, 1).unwrap();
        let width = builder_tile_width(&p, &ctx, &second);
        assert!(wrong.push_step_tile(second, &vec![0u32; width as usize]).is_err(), "leaf 1 cannot be pushed first");
    }

    /// **Condition 10: the court adjudicates a Qwen step from ONE tile opening and no model.**
    ///
    /// The node challenged is `attn_output` — a weighted matmul, so the court needs weights, and
    /// the ONLY weights it gets are the bytes a Merkle path binds to the class's registered
    /// artifact root. No checkpoint is opened, no model is loaded, and the 1.5B artifact is
    /// nowhere near this test.
    ///
    /// Both directions are asserted from the same fixture: the honest commitment recomputes to
    /// `NoFaultFound`, and one corrupted value convicts with `ComputationMismatch` at exactly
    /// that value's index. A court that only ever produced one of the two would be useless in a
    /// different way each time.
    #[test]
    fn a_qwen_step_is_adjudicated_from_one_tile_and_no_model() {
        use crate::palw_artifact::{PalwArtifactOperandV1, PalwProvenOperandsV1, artifact_leaf_v1, artifact_root_v1};
        use crate::palw_step::{PalwStepCoordinateV1, canonical_step_coordinates, canonical_step_leaf_index};
        use crate::palw_step_leg::{
            PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepLegBuilderV1, PalwStepTileLeafV1, checkpoint_empty_root_v2,
            checkpoint_leg_root_v2, execution_commitment_root_v2, step_leg_root_v1, step_opening_v1,
        };
        let g = probe_geometry();
        let p = qwen25_profile_v1(g).unwrap();
        let ctx = probe_context(&p);
        let total = crate::palw_step::step_leaf_count(&p, &ctx).unwrap();

        // The challenged node: `attn_output`, found by the tensor it reads rather than by a slot
        // index — the table is projected from the IR now and an index would be a second, silently
        // rotting description of where the node is.
        let out_index = p
            .attn_nodes
            .iter()
            .position(|n| n.weight_name == "blk.{layer}.attn_output.weight")
            .expect("the output projection is in the graph");
        let out_slot = 1 + out_index as u32;
        let in_slot = 1 + p.attn_nodes[out_index].input_refs[0] as u32; // its one input, the P.V result

        // The weight block the class registered, and the input row a producer committed.
        let hidden = g.hidden_dim as usize;
        let q_dim = (g.attn_heads as u32 * g.attn_head_dim) as usize;
        let weights: Vec<i8> = (0..hidden * q_dim).map(|i| (((i * 5) % 11) as i32 - 5) as i8).collect();
        let input: Vec<u32> = (0..q_dim).map(|i| ((i % 7) as i32 - 3) as u32).collect();
        let x: Vec<i8> = input.iter().map(|v| *v as i32 as i8).collect();
        let honest_out: Vec<u32> =
            crate::palw_base0_ops::matmul_quant(&weights, &x, hidden).unwrap().into_iter().map(|v| v as u32).collect();

        // Build the leg. Every leaf is canonical filler EXCEPT the challenged node's input row and
        // its output row — a court reads one step, so the rest are opaque bytes to it. That IS the
        // condition: adjudication from one opening, not from a model.
        let build = |corrupt: bool| {
            let mut b = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).unwrap();
            for i in 0..total {
                let coord = canonical_step_coordinates(&p, &ctx, i).unwrap();
                let width = builder_tile_width(&p, &ctx, &coord) as usize;
                let start = coord.tile_index as usize * p.attn_nodes[0].tile_len as usize;
                let values: Vec<u32> = if coord.node_slot == in_slot && coord.call_index == 1 {
                    input[start..start + width].to_vec()
                } else if coord.node_slot == out_slot && coord.call_index == 1 {
                    let mut row = honest_out[start..start + width].to_vec();
                    if corrupt && coord.tile_index == 0 {
                        row[3] = (row[3] as i32).wrapping_add(1) as u32;
                    }
                    row
                } else {
                    (0..width).map(|j| ((i * 3 + j as u64 + 1) % 61) as i32 as u32).collect()
                };
                b.push_step_tile(coord, &values).unwrap();
            }
            b.finish().unwrap()
        };

        let ctx_hash = ctx.context_hash();
        let profile_hash = p.shape_profile_id();
        // The class's own registered layout, not a fixture number: the binding check below
        // compares the carried map id against the profile's, and a fixture that files a made-up
        // one is a fixture testing a binding no producer could build.
        let ckpt = crate::palw_legs::PalwCheckpointProfileV1 {
            version: crate::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
            // This fixture's own interval; only the LAYOUT is the family's, and that is what
            // `verify_binding_v1` pins.
            checkpoint_interval: 8,
            state_layout_id: crate::palw_state_chunk_map::integer_kv_state_layout_id_v1(),
        };
        let state_chunk_map_id = crate::palw_state_chunk_map::integer_kv_state_chunk_map_id_v1();
        let adjudicate = |corrupt: bool| {
            let material = build(corrupt);
            let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, material.leaf_count, &material.merkle_root);
            let ckpt_root = checkpoint_leg_root_v2(
                &ctx_hash,
                &ckpt.profile_hash(),
                &state_chunk_map_id,
                1,
                0,
                &checkpoint_empty_root_v2(&ctx_hash),
            );
            let binding = crate::palw_step_leg::PalwStepBindingV2 {
                version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                job_context: ctx.clone(),
                shape_profile: p.clone(),
                checkpoint_profile: ckpt.clone(),
                state_chunk_map_id,
                full_logits_trace_root: Hash64::from_u64_word(0xAA),
                activation_leg_root: Hash64::from_u64_word(0xBB),
                step_leaf_count: material.leaf_count,
                step_merkle_root: material.merkle_root,
                checkpoint_count: 0,
                checkpoint_merkle_root: checkpoint_empty_root_v2(&ctx_hash),
                committed_execution_root: execution_commitment_root_v2(
                    &ctx_hash,
                    &Hash64::from_u64_word(0xAA),
                    &Hash64::from_u64_word(0xBB),
                    &ckpt_root,
                    &step_root,
                ),
            };
            let coord = PalwStepCoordinateV1 { call_index: 1, node_slot: out_slot, position: 0, tile_index: 0 };
            let out_idx = canonical_step_leaf_index(&p, &ctx, &coord).unwrap();
            let width = builder_tile_width(&p, &ctx, &coord) as usize;
            let mut committed = honest_out[..width].to_vec();
            if corrupt {
                committed[3] = (committed[3] as i32).wrapping_add(1) as u32;
            }
            // One canonical row: the input's tiles are consecutive leaves, so it rides as one
            // range run — the row form the carrier now requires.
            let tiles_n = (q_dim as u32).div_ceil(p.attn_nodes[13].tile_len);
            let mut preimages = Vec::with_capacity(tiles_n as usize);
            let mut first_idx = None;
            for t in 0..tiles_n {
                let c = PalwStepCoordinateV1 { call_index: 1, node_slot: in_slot, position: 0, tile_index: t };
                let idx = canonical_step_leaf_index(&p, &ctx, &c).unwrap();
                first_idx.get_or_insert(idx);
                let w = builder_tile_width(&p, &ctx, &c) as usize;
                let start = t as usize * p.attn_nodes[13].tile_len as usize;
                preimages.push(PalwStepTileLeafV1 {
                    version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                    coord: c,
                    value_count: w as u32,
                    values_le: input[start..start + w].iter().flat_map(|v| v.to_le_bytes()).collect(),
                });
            }
            let run = crate::palw_step_leg::step_merkle_range_siblings_v1(
                &material.leaf_hashes,
                first_idx.unwrap() as usize,
                tiles_n as usize,
            )
            .unwrap();
            let inputs = vec![crate::palw_step_refute::PalwStepInputRowV1 { preimages, run_siblings: vec![run] }];
            let refutation = crate::palw_step_refute::PalwExecutionStepRefutationV1 {
                binding,
                output_opening: step_opening_v1(&material.leaf_hashes, out_idx).unwrap(),
                output_preimage: PalwStepTileLeafV1 {
                    version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                    coord,
                    value_count: width as u32,
                    values_le: committed.iter().flat_map(|v| v.to_le_bytes()).collect(),
                },
                inputs,
                prompt_token_ids: Vec::new(),
                decode_tokens: None,
                kv_checkpoint: None,
            };
            // The class's registered inventory, and the opening that proves this block belongs.
            let operands = [
                // **ADR-0049 Decision B, and the reason this test's name is now true.** It opened
                // the whole `hidden x q_dim` block and called itself "from one tile". The step
                // reduces over the challenged tile's output rows alone, so that is what the
                // refutation carries: `width * q_dim` bytes at the tile's own offset. At
                // Qwen2.5-1.5B's real unembed the same change is ~223 MiB down to 192 KiB.
                PalwArtifactOperandV1 {
                    tensor_name: "blk.{layer}.attn_output.weight".to_string(),
                    layer: Some(0),
                    row_start: (coord.tile_index as usize * p.attn_nodes[out_slot as usize].tile_len as usize * q_dim) as u32,
                    bytes: weights[coord.tile_index as usize * p.attn_nodes[out_slot as usize].tile_len as usize * q_dim..]
                        [..width * q_dim]
                        .iter()
                        .map(|v| *v as u8)
                        .collect(),
                },
                PalwArtifactOperandV1 { tensor_name: "decoy".to_string(), layer: None, row_start: 0, bytes: vec![1, 2, 3] },
            ];
            let leaves: Vec<Hash64> = operands.iter().map(artifact_leaf_v1).collect();
            let root = artifact_root_v1(&leaves).unwrap();
            let openings = vec![crate::palw_artifact::PalwArtifactOpeningV1 {
                operand: operands[0].clone(),
                leaf_index: 0,
                leaf_count: 2,
                path: vec![leaves[1]],
            }];
            let proven = PalwProvenOperandsV1::from_openings_v1(&openings, root).expect("the opening proves");
            crate::palw_step_refute::check_execution_step_refutation_v1(&refutation, &proven)
        };

        // Honest: recomputed and found correct.
        assert_eq!(
            adjudicate(false),
            Err(crate::palw_step_refute::PalwStepRefuteError::NoFaultFound),
            "an honest Qwen step is NoFault"
        );
        // Fraudulent: convicted at the value that was moved, from the same one opening.
        let verdict = adjudicate(true).expect("a wrong Qwen matmul convicts");
        assert_eq!(verdict.fault, crate::palw_step_leg::PalwStepFaultV1::ComputationMismatch { value_index: 3 });
    }

    /// The canonical tile width at a coordinate — the ragged last tile included."""
    fn builder_tile_width(
        p: &PalwShapeProfileV3,
        ctx: &crate::palw_v2::PalwJobContextV2,
        coord: &crate::palw_step::PalwStepCoordinateV1,
    ) -> u32 {
        let (node, _) = p.resolve_node_slot(coord.node_slot).unwrap();
        let kv_len = if coord.call_index == 0 {
            coord.position as u64 + 1
        } else {
            ctx.declared_prefill_tokens as u64 + coord.call_index as u64
        };
        let len = match node.out_len {
            PalwStepOutLenV1::Fixed { elements } => elements as u64,
            PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * kv_len,
        };
        let start = coord.tile_index as u64 * node.tile_len as u64;
        (len - start).min(node.tile_len as u64) as u32
    }

    /// The graph consumes exactly the declared inventory, so an artifact cannot be built over a
    /// different set than the one the court will open against.
    #[test]
    fn the_graph_consumes_exactly_the_declared_inventory() {
        let p = qwen25_profile_v1(QWEN25_1_5B).unwrap();
        let mut used: Vec<&str> = Vec::new();
        for node in [&p.pre_nodes, &p.attn_nodes, &p.post_nodes].into_iter().flatten() {
            if !node.weight_name.is_empty() && !used.contains(&node.weight_name.as_str()) {
                used.push(node.weight_name.as_str());
            }
        }
        used.sort_unstable();
        let mut declared: Vec<&str> = qwen25_tensor_names_v1();
        declared.sort_unstable();
        assert_eq!(used, declared, "the graph's operands and the declared inventory are one list");
    }

    /// **Audit H-04's question, answered by a ceiling that counts the whole close: there is no
    /// admissible pair at all.**
    ///
    /// "Either the tile grows or the context shrinks" was the right shape of answer for a ceiling
    /// on WEIGHT BYTES, where `tile_len` traded context against opening size. It stops being the
    /// question once the ceiling counts what a close costs to carry, because the arm that dominates
    /// a Qwen close does not depend on `tile_len` at all: a decode-position gather must carry every
    /// logits row so the court can recompute `base0_logits_trace_root_v1`, and ONE row of a 128,256
    /// vocabulary is 513,024 bytes — four times the largest standard transaction.
    ///
    /// So the pair a genesis reads is `None`, at every tile and every context, and the shipped
    /// constants stay the MODEL's: 4,096 tokens is what Qwen2.5 has.
    #[test]
    fn no_qwen_geometry_is_admissible_under_a_carriable_ceiling() {
        let shipped = crate::palw_mode_v2::PalwCourtParamsV2::new(crate::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).unwrap();
        for (name, model) in [("1.5B", QWEN25_1_5B), ("3B", QWEN25_3B)] {
            let pair = qwen25_admissible_geometry_v1(model, &shipped)
                .unwrap_or_else(|| panic!("{name}: no pair is admissible — ADR-0080's ceiling went back to one transaction"));
            println!("{name}: admissible pair tile_len={} n_ctx={}", pair.tile_len, pair.n_ctx);
            // **Three tokens.** The pair exists now and it is not a usable class: the LADDER is
            // what bounds it (`PALW_STEP_MAX_LEAVES` = 2^22), not the price, so widening this
            // family is ADR-0077's fence and not this ceiling. The number is pinned so that
            // "Qwen2.5 became registrable" cannot be read as "at a context anyone wants".
            assert_eq!((pair.tile_len, pair.n_ctx), (64, 3), "{name}: the widest pair the shipped ladder admits");
        }

        // The cheapest close either model could ever be asked for — the minimum context, swept over
        // every legal tile. Pinned because it is the size of the gap, and because a change that
        // closes it should be visible here rather than inferred.
        let cheapest = |model: PalwQwen25GeometryV1| -> u64 {
            let mut best = u64::MAX;
            for n_ctx in [2u32, 4, 8, 16, 32, 64] {
                let mut tile = crate::palw_step::PALW_STEP_MIN_TILE_LEN;
                while tile <= crate::palw_step::PALW_STEP_MAX_TILE_LEN {
                    if let Ok(p) = qwen25_profile_v1(PalwQwen25GeometryV1 { n_ctx, tile_len: tile, ..model })
                        && crate::palw_step::worst_case_step_leaf_count_v1(&p).is_ok()
                        && let Ok(c) = crate::palw_class_admission_v2::derive_court_cost_v1(&p)
                    {
                        best = best.min(c.max_close_bytes);
                    }
                    tile = tile.saturating_mul(2);
                }
            }
            best
        };
        assert_eq!(cheapest(QWEN25_1_5B), 1_220_368, "1.5B's cheapest possible close");
        assert_eq!(cheapest(QWEN25_3B), 1_220_944, "3B's, and the two are within 0.05% — the pin dominates both");
        // **The gap, and the ceiling it was a gap from.** This read `> 14 x
        // DEFAULT_MAX_CLOSE_BYTES` when that constant was 80 KiB — the order-of-magnitude finding
        // ADR-0080 was written about. It is unchanged as arithmetic and the constant it was stated
        // against is not, so it is stated against the number that was true: 1,220,368 is 14.9x the
        // 80 KiB one-transaction carrier and 54% of the 27-chunk group.
        const CEILING_THAT_REFUSED_THEM: u64 = 80 * 1024;
        assert!(cheapest(QWEN25_1_5B) > 14 * CEILING_THAT_REFUSED_THEM, "the gap was never an order of magnitude");
        assert!(
            cheapest(QWEN25_1_5B) < crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES,
            "the cheapest close is outside ADR-0080's ceiling too — then nothing in this family is registrable"
        );

        // And a court with no ceiling at all still says something useful: the LADDER alone admits a
        // pair, so what refuses Qwen here is cost and not depth.
        let ladder_only = crate::palw_mode_v2::PalwCourtParamsV2::with_cost_ceilings(
            crate::palw_step::PALW_STEP_MAX_LEAVES,
            4,
            2,
            u64::MAX,
            u64::MAX,
            u32::MAX,
        )
        .unwrap();
        let pair = qwen25_admissible_geometry_v1(QWEN25_1_5B, &ladder_only).expect("the ladder admits a pair");
        assert!(pair.n_ctx < QWEN25_1_5B.n_ctx, "even then the context shrinks — that part of H-04 stands");

        // A court that admits nothing says so, rather than returning a pair nobody can use.
        let impossible =
            crate::palw_mode_v2::PalwCourtParamsV2::with_cost_ceilings(crate::palw_step::PALW_STEP_MAX_LEAVES, 4, 2, 1, 1, 8).unwrap();
        assert!(qwen25_admissible_geometry_v1(QWEN25_1_5B, &impossible).is_none());
    }

    /// **A row that has to be SERVED declares the epsilon its artifact executes** — the dense
    /// twin of `the_corrected_rows_declare_the_artifact_epsilon`.
    ///
    /// `qwen25-convert` writes `eps_q: 1 << 8` into every artifact header and the A16 engine norms
    /// with the ARTIFACT's constant, while `QWEN25_1_5B` declares `rms_eps_q: 1`. A profile built
    /// from the frozen geometry is therefore one `A16Engine::plan_from_profile` refuses over its
    /// own class's weights — `GeometryMismatch { what: "rms_eps_q", profile: 1, artifact: 256 }` —
    /// which is `Qwen25A16Backend::from_registered_profile` refusing the ladder's dense row at
    /// every width. The hybrid's ladder row went through `qwen36_geometry_artifact_eps`; the dense
    /// one had no twin, and the shipped worker hid it by taking `::new`, which compiles no plan.
    ///
    /// The frozen constants are asserted UNCHANGED in the same breath: correcting them in place
    /// would move `qwen25_a16_class_id_v2`, which `params.rs` derives testnet-11's genesis
    /// registration from.
    #[test]
    fn the_dense_ladder_row_declares_the_epsilon_its_artifacts_execute() {
        assert_eq!(QWEN25_1_5B.rms_eps_q, 1, "the registered geometry is a chain fact and must not move");
        assert_eq!(QWEN25_1_5B_A16.rms_eps_q, 1, "nor the registered A16 geometry");
        assert_eq!(QWEN25_A16_ARTIFACT_EPS_Q, 1 << 8, "what qwen25-convert writes into every artifact header");

        // The correction is one field and nothing else.
        let corrected = qwen25_geometry_artifact_eps(QWEN25_1_5B);
        assert_eq!(corrected.rms_eps_q, QWEN25_A16_ARTIFACT_EPS_Q);
        assert_eq!(PalwQwen25GeometryV1 { rms_eps_q: QWEN25_1_5B.rms_eps_q, ..corrected }, QWEN25_1_5B, "only the epsilon moved");

        for n_ctx in crate::palw_context_ladder::PALW_CONTEXT_LADDER_ROWS {
            let dense = crate::palw_context_ladder::palw_a16_context_row_profile_v1(n_ctx).expect("the dense row projects");
            assert_eq!(
                dense.base0_rms_eps_q, QWEN25_A16_ARTIFACT_EPS_Q,
                "the dense ladder row at n_ctx {n_ctx} declares an epsilon no artifact of this family executes, so \
                 from_registered_profile refuses it before a demonstration starts"
            );
            // The hybrid row it is the twin of, asserted beside it so the asymmetry cannot return.
            let hybrid = crate::palw_context_ladder::palw_qwen36_context_row_profile_v1(n_ctx).expect("the hybrid row projects");
            assert_eq!(hybrid.base0_rms_eps_q, crate::palw_qwen36_profile::QWEN36_ARTIFACT_EPS_Q);
        }

        // And the correction is the ONLY difference from the frozen projection: the graph itself
        // is untouched, so this is a declaration repair and not a second graph.
        let frozen = qwen25_a16_profile_v2(QWEN25_1_5B_A16).expect("projects");
        let served = qwen25_a16_artifact_row_profile_v1(QWEN25_1_5B_A16).expect("projects");
        assert_ne!(frozen.shape_profile_id(), served.shape_profile_id(), "a different epsilon is a different class");
        assert_eq!(PalwShapeProfileV3 { base0_rms_eps_q: frozen.base0_rms_eps_q, ..served.clone() }, frozen, "one field apart");
    }
}
