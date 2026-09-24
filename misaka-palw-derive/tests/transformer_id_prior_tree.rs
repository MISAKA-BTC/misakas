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

/// The ids the tree after it gave — `fa80f768…`, from ea2fd48e (2026-09-11, ADR-0096's json kind)
/// until 7681c203 moved it without a re-pin: the pin as `transformer_id_pin.rs` read through
/// 2026-09-24, and the tree testnet-11's 2026-09-12 fleet release 13520042 shipped to every host.
/// A derivation filed on testnet-11 by a gateway on that build names one of these. Moved here
/// verbatim when the current tree was re-pinned, for the reason the rows above were: the stranger
/// parses every pair in the pin file as the CURRENT pin set.
const SECOND_PRIOR_SOURCE_TREE: &str = "fa80f7680783b644eb90a2722f5ca0cc2f001788f4fe7a82e521cf1005585303";
const SECOND_PRIOR_PINNED: &[(&str, &str)] = &[
    (
        "cad/stl/v1",
        "902ca5144d15c1f09a143b0078424bde981c8b2dc5d677ab8818dedc86f4a1c383cff7d8ac0b1445899c2db7e7ef7e25aa749a8811cdc7d45e837b2f883c0935",
    ),
    (
        "code/evm/v1",
        "c691c66ee5ccb7f575772683dae599c69ff50d4084ba445a9f7b6ac2893f22d56e1b755c212545a0490badad0cb5835821f7c2e349702914168533fac6588403",
    ),
    (
        "contract/evm/v1",
        "a5b34c70a41b2fe6513a2b8f6b2b9abaed0f59b38ece3ce2eb39d4af4b71c74834efece6683e835b0abe35268fb76942c805c7d3e728d6acf450bbc39ca51572",
    ),
    (
        "image/png/v1",
        "afc299335937bb65552ce74984569684cc468b5799a5abecf535575177f5e5bbcf0c6b418aaec0f2deb8f02f30d01f6a3e4e262b440b2f7e8fbd0c5a6291d5f9",
    ),
    (
        "json/canonical/v1",
        "74d4a76930d7beec8848c74edf05f1e8f982e1b585576db7dc9fd6615cceb7c4d754130962966eab4283675271a4d4603814041c1732b88f99f229e67bfd8b37",
    ),
    (
        "map/mmap/v1",
        "9a0f3765c6ea48efa211985a671878e6b9f85d5c42d118719d92db546e17603e2451e67cd7e229a830f654cfb3557e2f3ef82f82c113815b1fa58d17f174a613",
    ),
    (
        "music/smf/v1",
        "4f47c5786c58affbf0a24e3203d50728385e26aece93737d56b79eebe95cbfff91d10ba03b3ee4cf22cdd836bbe295537fcf810210c8b4cd16a77afa1e5eb50e",
    ),
    (
        "scene/glb/v1",
        "62931736fd5b306f158a9b945757c7ce8076ce8a9115297b00cb91afa466a62edc2ff7b92d629a8a13a0dce06272ebd1233ea5ac1ec8079b99382944a67c1cb2",
    ),
    (
        "simulation/trace/v1",
        "c92cdad2eca5a3326537281ff48cdf40df139187329accbae7f4a7e96baf4e115f10efc8e4c071fc4498d1cd86d69f580649a71462def3ef3d7f09afae238734",
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
    for (tree, pinned) in [(PRIOR_SOURCE_TREE, PRIOR_PINNED), (SECOND_PRIOR_SOURCE_TREE, SECOND_PRIOR_PINNED)] {
        assert!(PRIOR_SOURCE_TREES_SHA256_HEX.contains(&tree), "{tree}: the tree testnet-11's derivations name must stay listed");
        for (name, hex) in pinned {
            let mut bytes = [0u8; 64];
            faster_hex::hex_decode(hex.as_bytes(), &mut bytes).unwrap();
            let id = kaspa_hashes::Hash64::from_bytes(bytes);
            let (transformer, resolved) =
                transformer_by_id_with_tree(&id).unwrap_or_else(|| panic!("{name}: the chain's id no longer resolves"));
            assert_eq!(transformer.manifest().name, *name, "{name}: an earlier id must name the transformer it always named");
            assert_eq!(resolved, TransformerIdTree::Prior(tree), "{name}");
            let served = manifest_for_id(&id).unwrap();
            assert_eq!(served.source_tree_sha256, tree, "{name}: the manifest served for an earlier id is that tree's");
            assert_eq!(transformer_id(&served), id, "{name}: the manifest served for an earlier id hashes back to it");
        }
    }
}
