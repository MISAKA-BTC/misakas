//! **The eight `transformer_id`s, pinned — because otherwise they move in silence.**
//!
//! ADR-0078 Decision 3 makes a transformer's manifest carry "the build's source-tree hash", so
//! that `transformer_id` names the code. `build.rs` computes that hash over **every non-dot file
//! under `src/`, bytes and all** — `#[cfg(test)]` modules, doc comments and message strings
//! included (see `src/source_tree.rs`, which is the one spelling of the walk). Every manifest
//! quotes it. So:
//!
//! > **A single byte changed anywhere under `misaka-palw-derive/src/` moves ALL EIGHT
//! > `transformer_id`s at once.**
//!
//! That is correct and deliberate — Decision 3's whole point is that the id names the code — but
//! before this file the move was *invisible*. Measured on this branch, adding one doc comment and
//! rewording one error string moved every id:
//!
//! ```text
//!   source_tree  50ffcd18…  →  d2419027…
//!   scene/glb/v1 11ee2900…  →  17f041cd…        music/smf/v1 9067320c…  →  24be3094…
//!   cad/stl/v1   ccbbc7e5…  →  cde32f30…        …and the other five, likewise
//! ```
//!
//! …and **not one test went red, and no golden moved.** The corpus goldens pin `dsl_hash` and
//! `artifact_hash`, and neither is a function of `transformer_id`; all 43 reproduced exactly. A
//! grep of the tree for any of the eight id hexes finds nothing — they are computed at build time
//! and stored nowhere.
//!
//! # Why silence is the problem
//!
//! A derivation on chain carries `transformer_id`, and ADR-0078 Decision 5's promise is that a
//! consumer can re-run the named transformer and check `artifact_hash`. A consumer does that
//! through `registry::transformer_by_id`. So once a derivation exists on a public chain, a routine
//! comment fix in this crate makes every published derivation resolve to nothing — a
//! `DeriveError::UnknownTransformer` for every verifier holding the new build — while the whole
//! test suite stays green. The failure would appear as "verification is broken" long after the
//! commit that caused it, which is the shape of defect this repository keeps re-recording.
//!
//! # What this file does, and what it does NOT
//!
//! It makes the move LOUD. It does not forbid it: an id move is legitimate, and before the first
//! relaunch that publishes a derivation it is nearly free. Re-pinning is a normal act — it just
//! has to be a decided one.
//!
//! **When this test fails**, the id set moved. Decide which case you are in:
//!
//!   * *You changed the transformer's behaviour, its bounds, or its manifest.* The move is the
//!     rule working. Re-pin, and say in the commit message which ids moved.
//!   * *You changed a comment, a test, or a message.* The move is a side effect. It is still
//!     harmless **only while no derivation naming an old id exists on a chain you care about**.
//!     Before a relaunch: re-pin and move on. After one: the old build's ids must stay resolvable,
//!     or you are stranding published derivations — that is a deploy-order decision, not an edit.
//!
//! Re-pin by running `cargo run -p misaka-palw-derive --bin palw-derive -- drill --report r.json`
//! and copying `transformers[].transformer_id` and `source_tree_sha256` from it, or by reading the
//! values out of this test's own failure message. This pin is local to this crate; it is unrelated
//! to the consensus fingerprint pins and must not be re-pinned together with them.

use misaka_palw_derive::{SOURCE_TREE_SHA256_HEX, ids::transformer_id, registry};

/// The source-tree hash every manifest quotes.
const SOURCE_TREE: &str = "637858dba5ea5e34b9459a580b2b81d1361aecf450bc615a4ee9621d4953a988";

/// `(transformer name, transformer_id)` for every transformer this build registers.
const PINNED: &[(&str, &str)] = &[
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

fn actual() -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = registry::transformer_names()
        .into_iter()
        .map(|(name, _, _)| {
            let m = registry::transformer_by_name(name).expect("just enumerated").manifest();
            (name.to_string(), faster_hex::hex_string(transformer_id(&m).as_byte_slice()))
        })
        .collect();
    rows.sort();
    rows
}

/// **The pin.** Every registered transformer's id is the one this build was reviewed with.
#[test]
fn the_transformer_ids_are_the_ones_this_build_was_pinned_with() {
    let got = actual();
    let want: Vec<(String, String)> = PINNED.iter().map(|(n, i)| (n.to_string(), i.to_string())).collect();
    if got != want {
        let mut report = String::new();
        report.push_str(&format!("\n  source_tree_sha256\n    pinned {SOURCE_TREE}\n    actual {SOURCE_TREE_SHA256_HEX}\n"));
        report.push_str("\n  the pin, as it should now read:\n");
        for (name, id) in &got {
            report.push_str(&format!("    (\"{name}\", \"{id}\"),\n"));
        }
        panic!(
            "ADR-0078 Decision 3: the transformer id set moved.\n\
             Every id is a function of `source_tree_sha256`, which covers EVERY byte under `misaka-palw-derive/src/` \
             — a comment or a test is enough. Nothing else in the tree pins these, and the corpus goldens do NOT \
             catch the move (they pin dsl_hash and artifact_hash, neither of which depends on transformer_id).\n\n\
             Decide before you re-pin: a derivation already published on a chain names an OLD id, and a build that \
             cannot resolve it cannot verify it (ADR-0078 Decision 5). Before a relaunch this is nearly free; after \
             one it is a deploy-order decision. See this file's header.{report}"
        );
    }
    assert_eq!(SOURCE_TREE_SHA256_HEX, SOURCE_TREE, "the source-tree hash moved but every id matched: that cannot happen");
}

/// The pin covers the whole registry — a ninth transformer must be pinned, not merely appear.
#[test]
fn the_pin_covers_every_registered_transformer() {
    assert_eq!(
        actual().len(),
        PINNED.len(),
        "this build registers {} transformers and the pin lists {}: a transformer that is not pinned is one whose id \
         can move without anybody noticing, which is the whole thing this file exists to prevent",
        actual().len(),
        PINNED.len()
    );
}

/// The manifest bounds SA-2 declares are in the id's preimage, so a loosened bound cannot keep the
/// pinned id. Stated here as well as in `ids.rs` because this is the file a reader lands on when
/// the pin fires, and "did somebody widen a ceiling?" is the first question worth asking.
#[test]
fn every_pinned_transformer_still_declares_the_three_sa2_ceilings() {
    for (name, _) in PINNED {
        let m = registry::transformer_by_name(name).unwrap_or_else(|| panic!("{name} is pinned but not registered")).manifest();
        assert!(misaka_palw_derive::check_declared_bounds(&m).is_ok(), "{name} ships a zero ceiling (ADR-0078 SA-2)");
    }
}
