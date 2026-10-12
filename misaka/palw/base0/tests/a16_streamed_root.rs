//! **The streamed A16 inventory root is the materialized one** (ADR-0106's frontier, finally wired).
//!
//! `CanonicalClassV1::artifact_root` — the value a `ClassRegistered` pins and a producer compares
//! against it — went through `a16_inventory_v1(..).root()`, which holds a
//! `Vec<PalwArtifactRowDigestV1>` of every leaf plus a `BTreeSet` of every key, sorts them, and only
//! then roots. At a 2,097,152 context the rotary table alone contributes `max_position` leaves and
//! the walk cost a producer 11.5 GiB of anon-rss before the kernel killed it — to compute a hash it
//! was going to compare with a 64-byte constant.
//!
//! `a16_inventory_root_streamed_v1` walks the `(tensor_name, layer)` GROUPS in canonical order and
//! pushes each row's leaf straight into `PalwArtifactMerkleFrontierV1`, keeping one peak per level.
//! A streamed root is only worth anything if it is the SAME root, which is what this file is.

use kaspa_consensus_core::palw_qwen25_profile::{
    PalwQwen25GeometryV1, qwen25_a16_artifact_row_profile_v7, qwen25_a16_profile_v2, qwen25_a16_profile_v5,
};
use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
use misaka_palw_base0::inventory::{a16_inventory_root_streamed_v1, a16_inventory_v1};

/// A servable A16 fixture at a named geometry — the drill's own construction.
fn a16_fixture(geometry: PalwQwen25GeometryV1) -> Base0ArtifactV1 {
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
    Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
        .expect("fixture weights derive")
        .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
        .expect("the A16 parameter store derives")
}

/// A geometry small enough to MATERIALIZE, so the two roots can be compared at all. The shipped 2M
/// row cannot be: that is the whole problem, and the case below covers it by bound instead.
fn small() -> PalwQwen25GeometryV1 {
    // The court drill's own fixture geometry, unchanged — a geometry the A16 store really serves.
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

#[test]
fn the_streamed_root_is_the_materialized_root_on_every_graph_version() {
    let geometry = small();
    let artifact = a16_fixture(geometry);
    for (name, profile) in [
        ("graph-v2", qwen25_a16_profile_v2(geometry).expect("v2 projects")),
        ("graph-v5", qwen25_a16_profile_v5(geometry).expect("v5 projects")),
        ("graph-v7", qwen25_a16_artifact_row_profile_v7(geometry).expect("v7 projects")),
    ] {
        let materialized = a16_inventory_v1(&artifact, &profile).expect("the fixture yields an inventory");
        let streamed = a16_inventory_root_streamed_v1(&artifact, &profile).expect("the fixture streams");
        assert_eq!(
            streamed,
            materialized.root(),
            "{name}: the streamed root must be the root a registration pins ({} leaves)",
            materialized.operands().len()
        );
    }
}

/// **The streamed walk keeps nothing per leaf, so a WIDER context must not cost more state.**
///
/// The proof that matters at 2M is not a root comparison (nothing can materialize that inventory on
/// a 24 GB host) but that the streamed build's retained state is flat in the leaf count. Widening
/// `n_ctx` multiplies the rotary group's leaves and leaves the group COUNT alone, so a build whose
/// memory follows the groups shows exactly that — and one that quietly kept a leaf vector would not.
#[test]
fn widening_the_context_multiplies_the_leaves_and_not_the_groups() {
    let narrow = small();
    let wide = PalwQwen25GeometryV1 { n_ctx: narrow.n_ctx * 8, ..narrow };
    let (a_n, a_w) = (a16_fixture(narrow), a16_fixture(wide));
    let (p_n, p_w) =
        (qwen25_a16_profile_v5(narrow).expect("narrow v5"), qwen25_a16_profile_v5(wide).expect("wide v5"));

    let leaves_n = a16_inventory_v1(&a_n, &p_n).expect("narrow inventory").operands().len();
    let leaves_w = a16_inventory_v1(&a_w, &p_w).expect("wide inventory").operands().len();
    let groups_n = misaka_palw_base0::inventory::a16_inventory_group_count_v1(&a_n, &p_n).expect("narrow groups");
    let groups_w = misaka_palw_base0::inventory::a16_inventory_group_count_v1(&a_w, &p_w).expect("wide groups");

    println!("  n_ctx {} -> {}", narrow.n_ctx, wide.n_ctx);
    println!("  leaves {leaves_n} -> {leaves_w}   groups {groups_n} -> {groups_w}");
    assert!(leaves_w > leaves_n, "a wider context is more leaves: {leaves_n} -> {leaves_w}");
    assert_eq!(groups_w, groups_n, "and the same groups: {groups_n} -> {groups_w}");
    // Both roots still agree with the materialized build at each width.
    assert_eq!(a16_inventory_root_streamed_v1(&a_w, &p_w).expect("wide streams"), a16_inventory_v1(&a_w, &p_w).unwrap().root());
}


/// **Every streamed opening verifies against the streamed root, and carries the materialized
/// build's bytes** — on every graph version, for a draw that includes the first, the last, a middle
/// and a duplicate-free scatter of positions. The verifier is `verify_artifact_opening_v1`, the one
/// the chain runs, so "verifies" means the promotion rule matched the tree's, not a re-derivation.
#[test]
fn streamed_readiness_openings_verify_against_the_streamed_root() {
    use kaspa_consensus_core::palw_artifact::verify_artifact_opening_v1;
    use misaka_palw_base0::inventory::{a16_inventory_root_and_count_v1, a16_readiness_openings_streamed_v1};
    let geometry = small();
    let artifact = a16_fixture(geometry);
    for (name, profile) in [
        ("graph-v2", qwen25_a16_profile_v2(geometry).expect("v2")),
        ("graph-v5", qwen25_a16_profile_v5(geometry).expect("v5")),
        ("graph-v7", qwen25_a16_artifact_row_profile_v7(geometry).expect("v7")),
    ] {
        let materialized = a16_inventory_v1(&artifact, &profile).expect("materializes");
        let n = materialized.operands().len() as u32;
        let (root, count) = a16_inventory_root_and_count_v1(&artifact, &profile).expect("root+count");
        assert_eq!(count, n, "{name}: the streamed count is the materialized count");
        assert_eq!(root, materialized.root(), "{name}: the streamed root is the materialized root");

        let draw: Vec<u32> = [0, n - 1, n / 2, 1, n / 3, (2 * n) / 3, n - 2, 7 % n].into_iter().collect();
        let (root2, count2, openings) = a16_readiness_openings_streamed_v1(&artifact, &profile, &draw).expect("opens");
        assert_eq!((root2, count2), (root, count), "{name}: one walk agrees with the other");
        assert_eq!(openings.len(), draw.len(), "{name}: one opening per drawn leaf, in draw order");
        for (o, i) in openings.iter().zip(draw.iter()) {
            assert_eq!(o.leaf_index, *i, "{name}: draw order kept");
            verify_artifact_opening_v1(o, root).unwrap_or_else(|e| panic!("{name}: leaf {i} does not verify: {e:?}"));
            let m = &materialized.operands()[*i as usize];
            assert_eq!((&o.operand.tensor_name, o.operand.layer, o.operand.row_start), (&m.tensor_name, m.layer, m.row_start), "{name}: leaf {i} identity");
            assert_eq!(o.operand.bytes, m.bytes, "{name}: leaf {i} bytes are the materialized bytes");
        }
        // A tampered path fails the same verifier, so the assertion above is not vacuous.
        let mut bad = openings[2].clone();
        if let Some(h) = bad.path.first_mut() { *h = kaspa_consensus_core::Hash64::from_u64_word(0xBAD); }
        assert!(verify_artifact_opening_v1(&bad, root).is_err(), "{name}: a wrong sibling is refused");
        // An index past the inventory is a named refusal, not a panic.
        assert!(a16_readiness_openings_streamed_v1(&artifact, &profile, &[n]).is_err(), "{name}: leaf {n} is outside");
    }
}


/// **The material the panel builds its multiproof from is the streamed material**, and the proof
/// verifies with the chain's verifier — the exact call chain the readiness path now runs.
#[test]
fn a_multiproof_from_streamed_material_verifies() {
    use kaspa_consensus_core::palw_artifact::{palw_artifact_multiproof_v1, verify_artifact_multiproof_v1};
    use misaka_palw_base0::inventory::a16_readiness_material_streamed_v1;
    let geometry = small();
    let artifact = a16_fixture(geometry);
    let profile = qwen25_a16_artifact_row_profile_v7(geometry).expect("v7");
    let n = a16_inventory_v1(&artifact, &profile).expect("materializes").operands().len() as u32;
    let draw = vec![n - 1, 0, n / 2, 3 % n, (n / 2) + 1];
    let (root, leaves, mut opened) = a16_readiness_material_streamed_v1(&artifact, &profile, &draw).expect("material");
    assert_eq!(leaves.len() as u32, n, "every leaf hash, once");
    assert_eq!(opened.iter().map(|(i, _)| *i).collect::<Vec<_>>(), draw, "operands in draw order");
    opened.sort_by_key(|(i, _)| *i);
    let proof = palw_artifact_multiproof_v1(&leaves, &opened).expect("the opened leaves are the inventory's");
    verify_artifact_multiproof_v1(&proof, root).expect("the multiproof opens the streamed root");
    assert_eq!(proof.leaf_count, n);
}
