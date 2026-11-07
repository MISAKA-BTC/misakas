//! **Regression (feat/t12-aheld-node review, HIGH): a refused decoy held root claim is never a
//! challenger's filing.**
//!
//! The node's held challenger reads the accused's `CourtAttnRootClaimedHeld` back off the chain. The
//! walk returns every lifecycle object on an ACCEPTED carrier — including one the fold REFUSED — and
//! the node's first filter (execution root, tile leaf, tile opening) kept the first match for the
//! session's life: a liar's decoy, the genuine object with one slice sub-root swapped (refused at H3,
//! `HeldSubRootsDoNotRoot`, its carrier accepted) filed first, poisoned N2 on every retry and left the
//! honest seat charged at the backstop. The reader now applies the fold's own checks
//! (`PalwAttnHeldFilingV1::from_object_checked_v1`): this builds the genuine held root claim a real
//! responder files, then the decoy, and pins that the checked reader takes the genuine one, refuses
//! the decoy by H3's name, and that the old filter could not tell them apart (the regression's
//! reason). kaspad's own test runs the node's candidate selection and eviction over both.

use kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldFilingV1;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1;
use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepOpKindV1, PalwStepTableV1, canonical_step_leaf_index};
use kaspa_hashes::Hash64;
use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;
use std::sync::Arc;

const NETWORK: &[u8] = b"misaka-palw-rc";
const CANONICAL: (u32, u32) = (64, 8);
const LADDER: u64 = 1 << 22;

/// The node's OLD filter (before the review's fix), spelled with the same public calls.
fn node_filter_accepts(filing: &PalwAttnHeldFilingV1, execution_root: Hash64, narrowed: u64) -> bool {
    filing.binding.committed_execution_root == execution_root
        && filing.out_tile.opening.leaf_index == narrowed
        && kaspa_consensus_core::palw_step_leg::step_opening_root_capped_v1(
            filing.binding.step_leaf_count,
            &filing.out_tile.opening,
            kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(LADDER, &filing.binding.shape_profile),
        )
        .is_ok_and(|root| root == filing.binding.step_merkle_root)
}

#[test]
fn review_heldnode_a_refused_decoy_filing_is_never_the_challengers_filing() {
    let geometry = kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 32,
        ffn_dim: 64,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 8,
        vocab_size: 128,
        n_ctx: 128,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 8,
    };
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7(geometry).expect("held v7");
    let shape = Base0ShapeV1 {
        n_layers: 2,
        n_heads: 4,
        n_kv_heads: 2,
        d_head: 8,
        d_ff: 64,
        vocab: 128,
        max_position: 128,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: 1,
    };
    let artifact = Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .unwrap()
            .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
            .unwrap(),
    );
    let root = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).unwrap().root();
    let backend = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), CANONICAL)
        .unwrap()
        .with_step_ladder_cap(LADDER)
        .with_prompt_ids_form(PalwPromptIdsFormV1::Flat);
    let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x0A11_E1D0)).unwrap();
    let fused = profile.attn_nodes.iter().position(|n| n.op_kind == PalwStepOpKindV1::AttnFused).unwrap();
    let slot = profile.global_node_slot(PalwStepTableV1::Attn, 0, fused).unwrap();
    let leaf = canonical_step_leaf_index(
        &profile,
        &job,
        &PalwStepCoordinateV1 { call_index: CANONICAL.1 - 1, node_slot: slot, position: 0, tile_index: 0 },
    )
    .unwrap();
    let outcome = backend.execute(&job, &prompt).unwrap();
    let class_id = profile.shape_profile_id();
    let session = Hash64::from_u64_word(0x5E55);

    // The responder's genuine held root claim (tag 57), and the decoy: the same object with one
    // slice sub-root replaced (anyone holding the accusation's binding and tile can make it; the
    // fold drops it at H3, its carrier stays accepted).
    let evidence = backend.attn_site_evidence_held_v1(&outcome.material, leaf, None, None).expect("N1");
    let site = evidence.site_v1(root, false, PALW_HELD_STEP_LADDER_V1).unwrap();
    let genuine_object = evidence.root_claim_held_v1(&site, session, 2).unwrap();
    let mut decoy_object = genuine_object.clone();
    if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::CourtAttnRootClaimedHeld { slice_sub_roots, .. } =
        &mut decoy_object
    {
        slice_sub_roots[0] = Hash64::from_u64_word(0xDEC0);
    }
    let (_, genuine) = PalwAttnHeldFilingV1::from_object_v1(&genuine_object).unwrap();
    let (_, decoy) = PalwAttnHeldFilingV1::from_object_v1(&decoy_object).unwrap();

    // Why this is a regression: the old filter cannot tell the two apart.
    assert!(node_filter_accepts(&genuine, outcome.execution_root, leaf), "the genuine filing passes the old filter");
    assert!(node_filter_accepts(&decoy, outcome.execution_root, leaf), "and so did the decoy — it never checked H3");

    // The checked reader: the genuine object stands, the decoy is refused by H3's name.
    let check = |object| {
        PalwAttnHeldFilingV1::from_object_checked_v1(
            object,
            &session,
            outcome.execution_root,
            class_id,
            root,
            leaf,
            PALW_HELD_STEP_LADDER_V1,
        )
    };
    assert_eq!(check(&genuine_object), Ok(genuine.clone()), "the genuine held root claim is the filing");
    let refused = check(&decoy_object).expect_err("the decoy is refused");
    assert!(refused.contains("H3"), "by H3's name: {refused}");
    // And the rest of the fold's order: another session, another execution, another leaf.
    assert!(
        PalwAttnHeldFilingV1::from_object_checked_v1(
            &genuine_object,
            &Hash64::from_u64_word(1),
            outcome.execution_root,
            class_id,
            root,
            leaf,
            PALW_HELD_STEP_LADDER_V1
        )
        .is_err()
    );
    assert!(
        PalwAttnHeldFilingV1::from_object_checked_v1(
            &genuine_object,
            &session,
            Hash64::from_u64_word(2),
            class_id,
            root,
            leaf,
            PALW_HELD_STEP_LADDER_V1
        )
        .is_err()
    );
    assert!(check(&genuine_object).is_ok() && check(&decoy_object).is_err());

    // N2 builds from the checked filing; it refuses the decoy — the poison the node no longer caches.
    let seat = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile, CANONICAL)
        .unwrap()
        .with_step_ladder_cap(LADDER)
        .with_prompt_ids_form(PalwPromptIdsFormV1::Flat);
    assert!(seat.attn_site_evidence_held_v1(&[], leaf, None, Some(&genuine)).is_ok(), "N2 builds from the genuine filing");
    assert!(seat.attn_site_evidence_held_v1(&[], leaf, None, Some(&decoy)).is_err(), "N2 refuses the decoy");
}
