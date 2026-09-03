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
const SOURCE_TREE: &str = "98265872fb7a372c07918e1171ff3f22273db9f58add165e82ff0caa837fb148";

/// `(transformer name, transformer_id)` for every transformer this build registers.
const PINNED: &[(&str, &str)] = &[
    (
        "cad/stl/v1",
        "167a8c47ff0c7a2f2ddd5b13ad2b10d4bae42a380feffa44d2f04499289f3c924bf0df9d02b2d56405b7c2f291df1c15a334b2bbf7b658aa36f6ce99bb40a547",
    ),
    (
        "code/evm/v1",
        "cc0750b1df6150a3f2f3f14b0d6ce10f2641c8d1c1db4fe767d0b299c1d83b8c103b87fd3c587ff0675f0b2d7ec3b30022f6131c7001c0a6eac637024d9781fc",
    ),
    (
        "contract/evm/v1",
        "38f98469784d13a40b9f369f7a7a287d7c2376e7c3798b5f097c6db6ebb687f8586fcf1f117e97417accc1d8fa4fd4cc618064ffd89d2b22db10f85764c17913",
    ),
    (
        "image/png/v1",
        "7d2c126e5e97c144c0bd58903a38a88c5147b5188598624f15554fbd5357da41f1c983401d34ad5a781015750336d31c9295f2cf15da938517ed55ad7b40f07c",
    ),
    (
        "map/mmap/v1",
        "b38b279365784f97c1b97462306a970f44570f11292df89664b4f46040b2f8f32ceb2c1e944de573f8d9f2c84ffe115d740aa21e82abe186a03bea0a7d409192",
    ),
    (
        "music/smf/v1",
        "4f4edd02c53ae28ed769acb3c6d0dcc6636427019bd3d52e413bd0ddb1ec4920a8d98228bd5fdb1f294207ca0b25f458c412abcb781dac7f98b0f9b53af59e1c",
    ),
    (
        "scene/glb/v1",
        "7e0ff4646cc8562bab8f7c7a4030f6890901ec16aefa3445c5ab3a5a0f9a95eeed911573205a942dd946bb74322e6ebb64f8d6948c5d2619dbd632094a4e311f",
    ),
    (
        "simulation/trace/v1",
        "037fa9c2cecc1dfa526dd50796b1de3029980f5f884d4afdeacc37ba3072060f90e24099421440d23c70e6d4d0f51e989e7ea036294a3e7bb843550f42bb00e6",
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
