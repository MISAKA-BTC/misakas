//! **`qwen25_a16_backend`'s `aheld_windowed_builder` module**, moved out of `qwen25_a16_backend.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;
use kaspa_consensus_core::palw_attn_court_v1::{
    PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnCourtVerdictV1, PalwAttnDissectChoiceV1, PalwAttnDissectPhaseV1,
    check_attn_dissect_bottom_v1, palw_attn_opened_lanes_v1,
};
use kaspa_consensus_core::palw_attn_responder_v1::{PalwAttnHeldEvidenceV1, PalwAttnHeldFilingV1};
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1;
use kaspa_consensus_core::palw_state_chunk_map::{PALW_HELD_STEP_LADDER_V1, palw_profile_is_held_v4};
use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepOpKindV1, canonical_step_coordinates};
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

/// The held graph-v7 row the sweep and the drill use — at one head per tile — at `layers` layers
/// and context `n_ctx`.
pub(super) fn held_fixture(layers: u16, n_ctx: u32) -> (std::sync::Arc<Base0ArtifactV1>, PalwShapeProfileV3) {
    use crate::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
    let geometry = PalwQwen25GeometryV1 {
        layer_count: layers,
        hidden_dim: 32,
        ffn_dim: 64,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 8,
        vocab_size: 128,
        n_ctx,
        n_threads: 1,
        rms_eps_q: 1,
        // One head per tile: the fused site's output tile and the query slice are one head's,
        // which is what makes a fused site dissectable (`palw_fused_sites_are_dissectable_v1`).
        tile_len: 8,
    };
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7(geometry).expect("a held graph-v7 profile");
    let shape = Base0ShapeV1 {
        n_layers: geometry.layer_count as usize,
        n_heads: geometry.attn_heads as usize,
        n_kv_heads: geometry.attn_kv_heads as usize,
        d_head: geometry.attn_head_dim as usize,
        d_ff: geometry.ffn_dim as usize,
        vocab: geometry.vocab_size as usize,
        max_position: geometry.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: geometry.rms_eps_q,
    };
    let artifact = std::sync::Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(crate::engine_a16::derived_a16_store(&shape))
            .expect("the derived store is sorted and unique"),
    );
    (artifact, profile)
}

/// One instance of the class at the network's ladder `network`: `2^22` runs the fixture's job
/// densely too; `2^11` puts it past the materialization cap, where only the windowed builder runs.
pub(super) fn backend(artifact: &std::sync::Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3, network: u64) -> Qwen25A16Backend {
    Qwen25A16Backend::new(artifact.clone(), b"misaka-palw-rc".to_vec(), profile.clone(), (64, 8))
        .expect("the fixture's declaration is this engine's program")
        .with_step_ladder_cap(network)
        .with_prompt_ids_form(PalwPromptIdsFormV1::Flat)
}

pub(super) const DENSE_NETWORK: u64 = 1 << 22;
pub(super) const PAST_THE_CAP: u64 = 1 << 11;

/// A free-prompt job of `positions` prompt tokens and `decode` decode tokens, its prompt and ids.
pub(super) fn job_for(
    backend: &Qwen25A16Backend,
    profile: &PalwShapeProfileV3,
    positions: u32,
    decode: u32,
) -> (PalwFreePromptJobV3, Vec<usize>, Vec<u32>) {
    let vocab = backend.artifact.shape.vocab;
    let prompt: Vec<usize> = (0..positions as usize).map(|i| (i * 7919 + 1013) % vocab).collect();
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: Hash64::from_u64_word(0xD0),
        class_id: profile.shape_profile_id(),
        executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
        executor_pubkey: vec![0x11; 32],
        operator_id: Hash64::from_u64_word(0x0B),
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 4242,
        job_nonce: [0x5B; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(backend.prompt_ids_form(), &ids).expect("the ids commit"),
        prompt_tokens: positions,
        decode_token_limit: decode,
        max_context_tokens: profile.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    };
    (job, prompt, ids)
}

pub(super) fn binding_of(material: &[u8]) -> PalwStepBindingV2 {
    crate::produce::base0_material_decode_any_v1(material).expect("decodes").binding().clone()
}

/// Every fused output leaf of the binding's job, with its coordinate.
pub(super) fn fused_leaves(profile: &PalwShapeProfileV3, binding: &PalwStepBindingV2) -> Vec<(u64, PalwStepCoordinateV1)> {
    (0..binding.step_leaf_count)
        .filter_map(|leaf| canonical_step_coordinates(profile, &binding.job_context, leaf).map(|c| (leaf, c)))
        .filter(|(_, c)| profile.resolve_node_slot(c.node_slot).is_some_and(|(n, _)| n.op_kind == PalwStepOpKindV1::AttnFused))
        .collect()
}

/// The filing a held root claim built from `evidence` carries.
pub(super) fn filing_of(evidence: &PalwAttnHeldEvidenceV1) -> PalwAttnHeldFilingV1 {
    PalwAttnHeldFilingV1 {
        binding: evidence.evidence.binding.clone(),
        out_tile: evidence.evidence.out_tile.clone(),
        anchor: evidence.evidence.anchor.as_ref().expect("a held site anchors").anchor.clone(),
        slice_sub_roots: evidence.slice_sub_roots.clone(),
    }
}

pub(super) fn session() -> Hash64 {
    Hash64::from_u64_word(0xA5E1)
}

pub(super) fn choose(phase: &mut PalwAttnDissectPhaseV1, child: u8, daa: u64) {
    let choice =
        PalwAttnDissectChoiceV1 { version: PALW_ATTN_COURT_OBJECT_VERSION_V1, session_id: session(), round: phase.round(), child };
    phase.apply_choice(&choice, daa, 10).expect("a legal choice");
}

/// **An honest responder's dissection, played to tile `target`**, the rounds from `responder`, the
/// bottom from `bottom_from` (either party's evidence) — the court's verdict at the bottom.
pub(super) fn honest_bottom(
    responder: &PalwAttnHeldEvidenceV1,
    bottom_from: &PalwAttnHeldEvidenceV1,
    artifact_root: Hash64,
    target: u64,
) -> Result<PalwAttnCourtVerdictV1, kaspa_consensus_core::palw_attn_court_v1::PalwAttnCourtError> {
    let site = responder.site_v1(artifact_root, false, PALW_HELD_STEP_LADDER_V1).expect("the site derives");
    let anchored = bottom_from.site_v1(artifact_root, true, PALW_HELD_STEP_LADDER_V1).expect("and with its anchor");
    assert!(anchored.site.every_position_is_checkpointed, "the held map checkpoints every position");
    let root = responder.evidence.root_claim_v1(&site).expect("the root computes");
    let committed =
        palw_attn_opened_lanes_v1(&responder.evidence.out_tile, &anchored.binding, site.head_lanes.2 as usize).expect("opens");
    let mut phase = PalwAttnDissectPhaseV1::open_with_arity(
        session(),
        &root,
        site.head_lanes,
        site.history_positions,
        &committed,
        site.site.params.values,
        2,
        site.tile_positions,
        0,
        10,
        true,
    )
    .expect("the honest root finalizes to the committed tile");
    let mut daa = 1;
    while phase.turn() != PalwBisectTurnV1::Terminal {
        let round = responder.round_v1(&site, &phase).expect("a round computes");
        phase.apply_round(&round, daa, 10).expect("an honest round folds");
        assert_eq!(bottom_from.divergent_child_v1(&site, &phase).expect("recomputes"), None, "nothing to name");
        let child = phase.child_ranges().iter().position(|&(f, c)| target >= f && target < f + c).expect("covered") as u8;
        choose(&mut phase, child, daa + 1);
        daa += 2;
    }
    let bottom = bottom_from.bottom_v1(&anchored, &phase).expect("the bottom builds");
    check_attn_dissect_bottom_v1(&phase, &bottom, &anchored.binding, &anchored.site, true)
}

/// **T-A4: the windowed evidence IS the dense evidence where both run, and builds where only it
/// can.** On an honest held run, at fused leaves of both layers, the first and last prefill
/// positions, a middle one and both decode calls, the first and last tiles of the row:
///
/// * the responder's windowed evidence (its own fold, no dense re-execution) equals the dense
///   builder's byte for byte, and its sub-roots are the anchor's own;
/// * past the materialization cap the dense builder refuses by name and the windowed one builds
///   the same evidence;
/// * a challenger holding only the filing a held root claim carries — the out tile, the anchor,
///   the sub-roots — rebuilds the same evidence from its own honest replay, past the cap too, and
///   the root claim object carries exactly that filing back (`from_object_v1`).
#[test]
fn the_windowed_evidence_is_the_dense_evidence_where_both_run_and_builds_past_the_cap() {
    let (artifact, profile) = held_fixture(2, 128);
    assert!(palw_profile_is_held_v4(&profile));
    let wide = backend(&artifact, &profile, DENSE_NETWORK);
    let narrow = backend(&artifact, &profile, PAST_THE_CAP);
    let (job, prompt, ids) = job_for(&wide, &profile, 12, 3);
    let run = wide.execute_free_prompt(&job, &prompt).expect("the held producer runs");
    let material = run.outcome.material.clone();
    let binding = binding_of(&material);
    assert!(binding.step_leaf_count > PAST_THE_CAP, "the job is past the narrow instance's cap");
    assert!(binding.step_leaf_count <= wide.materialize_cap(), "and inside the wide one's");
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let fused = fused_leaves(&profile, &binding);
    let last_tile = fused.iter().map(|(_, c)| c.tile_index).max().expect("a fused leaf");
    let sample: Vec<u64> = fused
        .iter()
        .filter(|(_, c)| {
            (c.call_index == 0 && [0, 5, 11].contains(&c.position) || c.call_index > 0) && [0, last_tile].contains(&c.tile_index)
        })
        .map(|(leaf, _)| *leaf)
        .collect();
    assert_eq!(sample.len(), 2 * 5 * 2, "two layers, five positions, two tiles");
    for &leaf in &sample {
        let dense = wide.attn_site_evidence(&material, leaf, Some(&ids), None).expect("the dense builder runs here");
        let windowed = wide.attn_site_evidence_held_v1(&material, leaf, Some(&ids), None).expect("the windowed responder builds");
        assert_eq!(windowed.evidence, dense, "leaf {leaf}: the windowed evidence is the dense evidence, byte for byte");
        let anchor = dense.anchor.as_ref().expect("anchored");
        let positions = windowed.site_v1(root, true, PALW_HELD_STEP_LADDER_V1).expect("site").site.anchor_positions;
        let layout = kaspa_consensus_core::palw_state_chunk_map::palw_state_layout_v4(&profile, positions).expect("layout");
        assert_eq!(
            kaspa_consensus_core::palw_state_chunk_map::palw_state_top_root_from_sub_roots_v4(&layout, &windowed.slice_sub_roots),
            Ok(anchor.anchor.leaf.state_chunks_root),
            "leaf {leaf}: the sub-roots are the anchor's own"
        );

        let refused = narrow.attn_site_evidence(&material, leaf, Some(&ids), None).expect_err("past the cap");
        assert!(
            refused.contains("TooManyLeaves") || refused.contains("materialization cap"),
            "leaf {leaf}: the dense re-execution is refused at the cap: {refused}"
        );
        let past = narrow.attn_site_evidence_held_v1(&material, leaf, Some(&ids), None).expect("the windowed one builds");
        assert_eq!(past, windowed, "leaf {leaf}: the same evidence past the cap");

        let filing = filing_of(&windowed);
        let challenger =
            narrow.attn_site_evidence_held_v1(&[], leaf, Some(&ids), Some(&filing)).expect("the challenger builds from the filing");
        assert_eq!(challenger, windowed, "leaf {leaf}: an honest challenger's evidence is the responder's");
        let site = windowed.site_v1(root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
        let object = windowed.root_claim_held_v1(&site, session(), 2).expect("the held root claim");
        assert_eq!(
            PalwAttnHeldFilingV1::from_object_v1(&object),
            Some((session(), filing)),
            "leaf {leaf}: the object carries the filing a challenger reads"
        );
        assert_eq!(challenger.fallback_v1(&windowed.site_v1(root, true, PALW_HELD_STEP_LADDER_V1).expect("site")), Ok(None));
    }
}

/// **An honest held responder is acquitted at every tile, whichever party files the bottom** —
/// the responder's own (its block paths and its sub-roots) or a challenger's built from the
/// filing (its own block paths, the filed top path). At the job's last fused leaf of each layer
/// (the longest history), past the materialization cap.
#[test]
fn an_honest_held_responder_is_acquitted_at_every_tile_whichever_party_bottoms() {
    let (artifact, profile) = held_fixture(2, 128);
    let narrow = backend(&artifact, &profile, PAST_THE_CAP);
    let (job, prompt, ids) = job_for(&narrow, &profile, 30, 3);
    let run = narrow.execute_free_prompt(&job, &prompt).expect("the held producer runs");
    let binding = binding_of(&run.outcome.material);
    let root = crate::inventory::a16_inventory_v1(&artifact, &profile).expect("the inventory").root();
    let fused = fused_leaves(&profile, &binding);
    let last_call = fused.iter().map(|(_, c)| c.call_index).max().expect("a call");
    for layer in 0..2u16 {
        let leaf = fused
            .iter()
            .rfind(|(_, c)| c.call_index == last_call && profile.resolve_node_slot(c.node_slot).and_then(|(_, l)| l) == Some(layer))
            .map(|(leaf, _)| *leaf)
            .expect("a fused leaf of the layer at the last call");
        let responder = narrow.attn_site_evidence_held_v1(&run.outcome.material, leaf, Some(&ids), None).expect("responder");
        let challenger = narrow.attn_site_evidence_held_v1(&[], leaf, Some(&ids), Some(&filing_of(&responder))).expect("challenger");
        let site = responder.site_v1(root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
        let tiles = u64::from(site.history_positions).div_ceil(u64::from(site.tile_positions));
        assert!(tiles >= 2, "the last position's history is more than one court tile");
        for target in 0..tiles {
            for (who, bottom_from) in [("responder", &responder), ("challenger", &challenger)] {
                assert_eq!(
                    honest_bottom(&responder, bottom_from, root, target),
                    Ok(PalwAttnCourtVerdictV1::ChallengerDefeated),
                    "layer {layer}, tile {target}, the {who}'s bottom"
                );
            }
        }
    }
}

/// **N4: past `palw_offence_attribution` a held class the consensus predicate refuses declines the
/// dissection (`palw_held_class_unanswerable_v1`: past the answerable context, or — the review's
/// F5 — a whole-context replay past the turn); below the fence, and for an answerable one, nothing
/// changed.**
#[test]
fn past_the_attribution_fence_a_held_class_past_the_answerable_context_declines_the_dissection() {
    let answerable = kaspa_consensus_core::palw_state_v2::PALW_HELD_ANSWERABLE_N_CTX_V1;
    for (n_ctx, past) in [(128u32, false), (answerable, false), (2 * answerable, true)] {
        let (artifact, profile) = held_fixture(2, n_ctx);
        assert!(palw_profile_is_held_v4(&profile));
        let below = backend(&artifact, &profile, PAST_THE_CAP);
        assert!(below.supports_dissection(), "n_ctx {n_ctx}: below the fence the answer is what it was");
        let fenced = backend(&artifact, &profile, PAST_THE_CAP).with_held_answerability_v1(true);
        assert_eq!(fenced.supports_dissection(), !past, "n_ctx {n_ctx}: past the fence");
        // N4 is the consensus predicate C1 and C5 read (the review's F5 bound included — the
        // fixture's replay is milliseconds, so the context bound is what refuses it here).
        assert_eq!(
            fenced.supports_dissection(),
            kaspa_consensus_core::palw_class_admission_v2::palw_held_class_unanswerable_v1(&profile).is_none(),
            "n_ctx {n_ctx}: the backend's answer is the predicate's"
        );
    }
}
