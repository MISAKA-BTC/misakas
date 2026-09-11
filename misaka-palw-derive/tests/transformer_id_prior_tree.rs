//! **The ids the PREVIOUS published tree gave — kept resolvable, and kept out of the pin file.**
//!
//! `transformer_id_pin.rs` holds the CURRENT tree's pins, and it is read by more than cargo:
//! `scripts/misaka-palw-derive-stranger.py` parses every `"name", "<128 hex>"` pair in it as the
//! pin set. These rows lived there for one release and the parser took them for the current pins
//! (the later spelling of a name wins), so the stranger gate reported the shipped tree as
//! disagreeing with itself. They are a different fact — what a live chain's derivations name — and
//! they sit in their own file. They never move: a re-pin of the current tree does not touch them.

use misaka_palw_derive::ids::transformer_id;

/// The ids the PREVIOUS published tree gave the same transformers — the pin as it read from
/// testnet-11 Relaunch 5f (2026-09-03) through main `a5f1bdf7`, and what every derivation the
/// public chain carries names (the 2026-09-04 3D job is the `cad/stl/v1` row). ADR-0096's json kind
/// moved the tree; `registry::PRIOR_SOURCE_TREES_SHA256_HEX` keeps these resolvable.
const PRIOR_SOURCE_TREE: &str = "637858dba5ea5e34b9459a580b2b81d1361aecf450bc615a4ee9621d4953a988";
const PRIOR_PINNED: &[(&str, &str)] = &[
    (
        "cad/stl/v1",
        "83e0f5088cd0f9b7e55e5add8fdfdf941f40e45f413e03943f23817d907bed17900747ecd0a7e4e1e8193d6ced9c1320d4775f72e38e4cc1e6238e391db05ca0",
    ),
    (
        "code/evm/v1",
        "9cc43a428fe50667dec97d5673162c11d864039206bf7754e38c13d285ce4661f7496f170fd1c30eb0a1fc20318601fde93a2f52984f509ebe83cf93b349eb26",
    ),
    (
        "contract/evm/v1",
        "efc8813e0bac6b94ef2ad35777583e16746bf25914b6b82cac98e026cf6b67be9243cbfa1e05fe36fee056472a5e98da7fda811c3517fae98396260e30efa0fe",
    ),
    (
        "image/png/v1",
        "67f57ddd196f4125b4d132f9160d2863973058ca18b193f92afac8f67531d7fa92a8fe94487f4da9002fa29ef3febf0e56f58108a253d0053cb7d3d7920a9e87",
    ),
    (
        "map/mmap/v1",
        "a1bfc8d9a06e12c08189c52a3bf243af7cbaed4d057c764c51e8f0aab4f6c98d04b76ba3f44d2f3aad75c79e837829b08b3041a9f50157d16b431085ef7fed47",
    ),
    (
        "music/smf/v1",
        "cb5f27b4e63d9601a3e743486ea61b6aed9825c651b8fefa4305756cfec8f5aca69f7c27161f2a8a2e6f69eafd626fcaa4e28878b8c8989d24491be9b58ed0a8",
    ),
    (
        "scene/glb/v1",
        "4dd08df643160b205fe46f14ffb9c2cf36de83bddd1342e50f91551bc6b6d5b8234f3bbf6fa9a36f9bd72c08dcc5b4ad2dcfa5f6d5aa33cbe94a197b238d8f7a",
    ),
    (
        "simulation/trace/v1",
        "389bf2942f7ee53cf0c9fe1096188b4de361e12f1b689f8b5b83d6874006c0fa6c1d8569ea31ac00deff1d1c19ae8b60a0b2373360e60f580ce41b2501627a62",
    ),
];

/// **The ids a live chain already carries still resolve — to the transformer they always named,
/// and to a manifest that hashes back to them.** Without this, the tree move in ADR-0096 would have
/// turned every published derivation into `UnknownTransformer` for anyone on the new build.
#[test]
fn the_previous_trees_ids_still_resolve_to_what_they_named() {
    use misaka_palw_derive::registry::{
        PRIOR_SOURCE_TREES_SHA256_HEX, TransformerIdTree, manifest_for_id, transformer_by_id_with_tree,
    };
    assert!(PRIOR_SOURCE_TREES_SHA256_HEX.contains(&PRIOR_SOURCE_TREE), "the tree testnet-11's derivations name must stay listed");
    for (name, hex) in PRIOR_PINNED {
        let mut bytes = [0u8; 64];
        faster_hex::hex_decode(hex.as_bytes(), &mut bytes).unwrap();
        let id = kaspa_hashes::Hash64::from_bytes(bytes);
        let (transformer, tree) =
            transformer_by_id_with_tree(&id).unwrap_or_else(|| panic!("{name}: the chain's id no longer resolves"));
        assert_eq!(transformer.manifest().name, *name, "{name}: an earlier id must name the transformer it always named");
        assert_eq!(tree, TransformerIdTree::Prior(PRIOR_SOURCE_TREE), "{name}");
        let served = manifest_for_id(&id).unwrap();
        assert_eq!(served.source_tree_sha256, PRIOR_SOURCE_TREE, "{name}: the manifest served for an earlier id is that tree's");
        assert_eq!(transformer_id(&served), id, "{name}: the manifest served for an earlier id hashes back to it");
    }
}
